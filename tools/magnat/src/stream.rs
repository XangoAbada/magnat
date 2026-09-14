//! Strumieniowanie chunków wokół kamery (M1 WP-V4 od strony konsumenta).
//!
//! Pętla idzie **po pierścieniach LOD**, od najbliższego do najdalszego, i to nie jest
//! szczegół organizacyjny: `ChunkCoord` jest współrzędną **względną dla poziomu
//! szczegółowości** — ta sama trójka liczb znaczy 32 m przy LOD0 i 256 m przy LOD3.
//! Zbiór „chunków w zasięgu" bez przypisanego LOD jest więc niedomówiony, a próba
//! policzenia go raz dla wszystkich poziomów kończy się tym, że warstwy pionowe wychodzą
//! w jednej skali, a materializacja idzie w innej — i teren nie pojawia się wcale.
//!
//! **Praca na klatkę jest ograniczona.** Przesunięcie kamery o kilometr unieważnia tysiące
//! chunków; zmaterializowanie ich wszystkich w jednej klatce oznaczałoby sekundę zawieszenia.
//! Limit rozkłada to na kilkadziesiąt klatek, kosztem chwilowo pustego horyzontu —
//! co widać mniej niż zacięcie (§7.4: ≤ 2 zacięcia > 33 ms na 60 s).

use magnat_jobs::JobPool;
use magnat_render::{CameraState, Renderer};
use magnat_voxel::{
    build_mesh, ChunkBuilder, ChunkCoord, ChunkMesh, ColumnSource, EditIndex, MaterialRegistry,
    ViewPoint, VoxelWorld, CHUNK_DIM, LOD_RADII_M, MAX_LOD,
};
use magnat_world::{Terrain, TerrainQuery};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Ile chunków wolno zmaterializować i zmeshować w jednej klatce.
///
/// Materializacja i meshing to razem ~1,1 ms na chunk (zmierzone w `mesh_budget.rs`),
/// więc 32 chunki na ośmiu rdzeniach to ~4,4 ms — mieści się w klatce 16,6 ms obok
/// samego rysowania.
const MAX_NA_KLATKE: usize = 32;
/// Ile chunków wolno zwolnić w jednej klatce.
const MAX_ZWOLNIEN: usize = 128;
/// Ile warstw pionowych wokół powierzchni brać pod uwagę.
///
/// Reszta kolumny to lita skała albo czyste powietrze — jedno i drugie daje mesh pusty,
/// a kosztuje pełną materializację.
const WARSTWY_WOKOL_POWIERZCHNI: i32 = 1;
/// Liczba warstw chunków w pionie przy LOD0 (M1 §5.1: −64 m … +192 m).
const WARSTW_LOD0: i32 = 16;

/// Chunk rezydentny po stronie GPU: współrzędna **razem ze skalą**, w której powstała.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Slot {
    lod: u8,
    coord: ChunkCoord,
}

/// Podgląd trzyma **własną** rezydencję, a nie `VoxelWorld`: ta ostatnia wyznacza LOD
/// z odległości chunka, a tu potrzebne są pierścienie rozłączne w metrach i warstwy pionowe
/// wybrane z wysokości terenu — czyli wiedza o świecie, nie o voxelach. Budżet pamięci
/// i eksmisja LRU z `VoxelWorld` (WP-V4) obsługują edycje terenu w M11; do samego oglądania
/// wystarczy zbiór slotów, bo o zwolnieniu decydują pierścienie, nie ciśnienie na pamięć.
pub struct Streamer {
    terrain: Arc<Terrain>,
    materials: Arc<MaterialRegistry>,
    /// Komendy voxelowe miasta (M2d, WP12b). Stosowane **przy materializacji**, a nie
    /// przez nakładkę per voxel: miasto zmienia setki milionów voxeli, a komend jest
    /// kilkaset tysięcy. Pusty indeks = widok czystego terenu M1 (`--no-city`).
    edits: Arc<EditIndex>,
    pool: JobPool,
    na_gpu: BTreeSet<Slot>,
    /// Wymuszony promień LOD0 w metrach — tylko do pomiaru z §7.4 („3000 chunków LOD0").
    /// W normalnym biegu `None`, bo pierścienie mają wynikać z odległości, nie z flagi.
    wymuszony_lod0_m: Option<i32>,
}

impl Streamer {
    pub fn new(
        terrain: Arc<Terrain>,
        materials: Arc<MaterialRegistry>,
        edits: Arc<EditIndex>,
    ) -> Streamer {
        Streamer {
            pool: JobPool::new(0),
            na_gpu: BTreeSet::new(),
            wymuszony_lod0_m: None,
            terrain,
            materials,
            edits,
        }
    }

    /// Każe trzymać wszystko w LOD0 do zadanego promienia — scena pomiarowa dla WP-R2.
    pub fn wymus_lod0(&mut self, promien_m: i32) {
        self.wymuszony_lod0_m = Some(promien_m);
    }

    /// Rozkład chunków po poziomach szczegółowości — do tytułu okna.
    #[must_use]
    pub fn opis(&self) -> String {
        let mut per_lod: BTreeMap<u8, usize> = BTreeMap::new();
        for slot in &self.na_gpu {
            *per_lod.entry(slot.lod).or_default() += 1;
        }
        per_lod
            .iter()
            .map(|(lod, n)| format!("L{lod}:{n}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn update(&mut self, camera: &CameraState, renderer: &mut Renderer) {
        let eye = camera.eye();
        let view = ViewPoint {
            pos: magnat_core::IVec3::new(eye.x as i32, eye.y as i32, (eye.z * 2.0) as i32),
        };

        let chciane = self.pierscienie(view);

        // Do zrobienia w tej klatce: to, czego jeszcze nie ma, od najbliższych.
        let mut zadania: Vec<(Slot, i64)> = chciane
            .iter()
            .filter(|s| !self.na_gpu.contains(s))
            .map(|s| {
                (
                    *s,
                    VoxelWorld::chunk_center(s.coord, s.lod).distance_sq(view.pos),
                )
            })
            .collect();
        // Klucz zawiera współrzędną, więc kolejność jest w pełni określona — dwa chunki
        // w tej samej odległości nie zależą od kolejności iteracji zbioru.
        zadania.sort_by_key(|(s, d)| (s.lod, *d, s.coord));
        zadania.truncate(MAX_NA_KLATKE);

        // Materializacja i meshing równolegle: każdy chunk jest niezależny, a `ColumnSource`
        // jest czysty (M1 §5.2), więc nie ma czego synchronizować.
        let terrain = self.terrain.as_ref();
        let materials = self.materials.as_ref();
        let edits = self.edits.as_ref();
        let gotowe: Vec<(Slot, ChunkMesh)> = magnat_jobs::map_reduce_indexed(
            &self.pool,
            &zadania,
            |_, (slot, _)| {
                let mut b = ChunkBuilder::new(slot.coord, slot.lod, 1);
                terrain.fill_chunk(slot.coord, slot.lod, &mut b);
                // Miasto ma pierwszeństwo przed terenem — to ono jest stanem trwałym.
                edits.apply_to(slot.coord, slot.lod, &mut b);
                (*slot, build_mesh(&b, materials))
            },
            |mut acc: Vec<(Slot, ChunkMesh)>, v| {
                acc.push(v);
                acc
            },
            Vec::new(),
        );

        for (slot, mesh) in gotowe {
            renderer.upload_chunk(slot.coord, slot.lod, 0, &mesh);
            self.na_gpu.insert(slot);
        }

        // Zwolnienia: chunki, które wypadły z pierścieni.
        let do_zwolnienia: Vec<Slot> = self
            .na_gpu
            .iter()
            .filter(|s| !chciane.contains(*s))
            .copied()
            .take(MAX_ZWOLNIEN)
            .collect();
        for slot in do_zwolnienia {
            renderer.remove_chunk(slot.coord, slot.lod);
            self.na_gpu.remove(&slot);
        }
    }

    /// Chunki, które mają być rezydentne — po jednym pierścieniu na poziom szczegółowości.
    ///
    /// Pierścienie są **kwadratowe i rozłączne**, wyznaczane w jednostkach chunków, a nie
    /// przez test odległości środka chunka. Różnica jest widoczna gołym okiem: przy teście
    /// odległości chunk LOD2 o środku tuż za promieniem LOD1 wchodzi do pierścienia LOD2
    /// **całą swoją szerokością**, więc jego bliższa połowa nachodzi na chunki LOD1 rysowane
    /// w tym samym miejscu. Dwie powierzchnie w jednej objętości to walka o bufor głębi,
    /// a na ekranie — siatka migających pasków wzdłuż poziomic. Granice liczone w chunkach
    /// i wyrównane do siatki grubszego poziomu wykluczają to z definicji.
    fn pierscienie(&self, view: ViewPoint) -> BTreeSet<Slot> {
        let mut out = BTreeSet::new();
        let size_m = self.terrain.size_m();
        let (mut outer, mut inner) = granice_pierscieni();
        if let Some(r) = self.wymuszony_lod0_m {
            outer = [r / CHUNK_DIM as i32, 0, 0, 0];
            inner = [0; 4];
        }

        for lod in 0..=MAX_LOD {
            let l = lod as usize;
            let span = (CHUNK_DIM as i32) << lod; // bok chunka: metry w poziomie, voxele w pionie
            let (cx, cy) = (view.pos.x.div_euclid(span), view.pos.y.div_euclid(span));
            let chunkow = (size_m / span).max(1);
            // Ile warstw pionowych ma ten poziom: przy LOD3 cały zakres świata mieści się
            // w dwóch warstwach, bo chunk pokrywa 256 voxeli, czyli 128 m.
            let warstw = (WARSTW_LOD0 >> lod).max(1);

            if outer[l] == 0 && l > 0 {
                continue;
            }
            for y in (cy - outer[l]).max(0)..=(cy + outer[l]).min(chunkow - 1) {
                for x in (cx - outer[l]).max(0)..=(cx + outer[l]).min(chunkow - 1) {
                    // Odległość Czebyszewa w chunkach: kwadratowy pierścień, nie kołowy.
                    // Kołowy zostawiałby narożniki bez pokrycia albo je dublował.
                    let d = (x - cx).abs().max((y - cy).abs());
                    if d < inner[l] {
                        continue; // ten obszar obsługuje drobniejszy poziom
                    }

                    // Wysokość w środku kolumny, w voxelach po 0,5 m.
                    let h = self
                        .terrain
                        .height_at(x * span + span / 2, y * span + span / 2);
                    let warstwa = h.div_euclid(span);
                    for dz in -WARSTWY_WOKOL_POWIERZCHNI..=WARSTWY_WOKOL_POWIERZCHNI {
                        let z = warstwa + dz;
                        if !(0..warstw).contains(&z) {
                            continue;
                        }
                        out.insert(Slot {
                            lod,
                            coord: ChunkCoord::new(x, y, z as i16),
                        });
                    }
                }
            }
        }
        out
    }
}

/// Granice pierścieni w jednostkach chunków danego poziomu: `(zewnętrzna, wewnętrzna)`.
///
/// Granica między poziomem `n−1` a `n` musi wypadać na **tym samym metrze** i jednocześnie
/// leżeć na siatce grubszego poziomu — inaczej między pierścieniami zostaje szczelina albo
/// zakładka. Stąd wyrównanie: wewnętrzna granica poziomu `n` wyznacza zewnętrzną poziomu
/// `n−1`, a nie odwrotnie.
fn granice_pierscieni() -> ([i32; 4], [i32; 4]) {
    let mut outer = [0i32; 4];
    for (l, r) in LOD_RADII_M.iter().enumerate() {
        outer[l] = r / ((CHUNK_DIM as i32) << l);
    }
    let mut inner = [0i32; 4];
    for n in 1..4 {
        inner[n] = (outer[n - 1] + 1) / 2;
        outer[n - 1] = inner[n] * 2;
    }
    (outer, inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pierscienie_sa_rozlaczne_i_bez_szczelin() {
        let (outer, inner) = granice_pierscieni();
        for n in 1..4 {
            let span_grubszy = (CHUNK_DIM as i32) << n;
            let span_drobniejszy = (CHUNK_DIM as i32) << (n - 1);
            // Granica wewnętrzna poziomu `n` i zewnętrzna poziomu `n−1` mają wypadać
            // na tym samym metrze — inaczej między pierścieniami jest szczelina
            // (widać przez nią niebo) albo zakładka (dwie powierzchnie w jednej objętości).
            assert_eq!(
                inner[n] * span_grubszy,
                outer[n - 1] * span_drobniejszy,
                "granica między LOD{} a LOD{n} nie pokrywa się",
                n - 1
            );
            assert!(outer[n] > inner[n], "pierścień LOD{n} jest pusty");
        }
        assert!(outer[0] > 0, "pierścień LOD0 jest pusty");
    }
}
