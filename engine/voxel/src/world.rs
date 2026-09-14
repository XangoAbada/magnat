//! `VoxelWorld` — rezydencja chunków, budżet pamięci i punkt synchronizacji edycji
//! (M1 §5.4, §5.4a, WP-V4).
//!
//! Świat 16 km ma 4,2 mln slotów chunków i nigdy nie trzyma ich wszystkich: rezydentny jest
//! pierścień wokół kamery, a poza nim wyłącznie mapa wysokości. Ten moduł odpowiada za to,
//! **które** chunki są w pamięci, w jakim LOD i co się dzieje, gdy budżet się kończy.
//!
//! Zasada, która rozstrzyga wszystkie przypadki sporne: **budżet jest asercją, nie sugestią**
//! (M1 ryzyko R5). Przekroczenie budżetu obniża promień LOD0, a nie wyczerpuje pamięć.

use crate::chunk::{Chunk, ChunkCoord, ChunkState, ChunkStorage, CHUNK_DIM, CHUNK_VOXELS};
use crate::edit::{find_overlaps, EditQueue, EditReport, VoxelEditCmd};
use crate::lod::{aggregate, MAX_LOD};
use crate::material::{LocalIdx, MaterialId};
use crate::ColumnSource;
use crate::{ChunkBuilder, Palette};
use magnat_core::{IVec3, SeededMap};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Punkt obserwacji sterujący rezydencją.
///
/// **Nie** `CameraState` z `engine/render`: zależność szłaby w złą stronę i wciągnęłaby
/// `wgpu` do crate'u, który ma działać headless (00 §6). Render przelicza swoją kamerę
/// na tę strukturę — to dwie linie po jego stronie zamiast całego grafu zależności po naszej.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ViewPoint {
    /// Pozycja obserwatora w voxelach świata.
    pub pos: IVec3,
}

/// Promienie pierścieni LOD w metrach (M1 §5.4).
pub const LOD_RADII_M: [i32; 4] = [192, 512, 1400, 4000];
/// Histereza promienia. Bez niej kamera stojąca na granicy pierścienia przerzuca chunk
/// tam i z powrotem co klatkę, płacąc za to pełną regeneracją i meshingiem.
pub const LOD_HYSTERESIS_PERMILLE: i32 = 150;

/// Budżet zasobów puli chunków (M1 §5.9).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VoxelBudget {
    /// Twardy limit pamięci na zawartość chunków.
    pub chunk_pool_bytes: usize,
    /// Ile chunków wolno mieć „w locie" (w generacji) jednocześnie.
    pub max_in_flight: usize,
}

impl Default for VoxelBudget {
    fn default() -> Self {
        VoxelBudget {
            chunk_pool_bytes: 384 * 1024 * 1024,
            max_in_flight: 64,
        }
    }
}

/// Żądanie materializacji chunka.
///
/// Porządek `Ord` jest **odwrotnością priorytetu**, bo `BinaryHeap` jest kopcem maksimum:
/// mniejszy LOD i mniejszy dystans mają iść pierwsze. Klucz zawiera `coord`, więc kolejność
/// jest w pełni określona — dwa chunki o identycznym dystansie nie zależą od tego,
/// który worker zgłosił je wcześniej (00 §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Request {
    lod: u8,
    dist_sq: i64,
    coord: ChunkCoord,
}

impl Ord for Request {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .lod
            .cmp(&self.lod)
            .then(other.dist_sq.cmp(&self.dist_sq))
            .then(other.coord.cmp(&self.coord))
    }
}

impl PartialOrd for Request {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Statystyki puli — wejście do inspektora devtools i do asercji budżetowych.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct VoxelStats {
    pub resident: usize,
    pub bytes: usize,
    pub edited_chunks: usize,
    pub evicted_total: u64,
    /// Promień LOD0 w metrach po ewentualnym obniżeniu przez budżet.
    pub lod0_radius_m: i32,
}

/// Rzadka nakładka edycji: zmienione voxele chunka, który **nie musi być rezydentny**.
///
/// To jest mechanizm, dzięki któremu M2 może postawić budynek 3 km od kamery, a M6 wydrążyć
/// kopalnię poza kadrem. Nakładka jest stanem trwałym i jest konsultowana przy każdej
/// materializacji chunka z `ColumnSource`.
#[derive(Clone, Default, Debug)]
pub struct EditOverlay {
    /// `BTreeMap`, nie `HashMap`: kolejność iteracji wchodzi do zapisu gry i do hasha (00 §3.2).
    chunks: BTreeMap<ChunkCoord, Vec<(u16, MaterialId)>>,
}

impl EditOverlay {
    #[must_use]
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Liczba zapisanych voxeli we wszystkich chunkach.
    #[must_use]
    pub fn voxel_count(&self) -> usize {
        self.chunks.values().map(Vec::len).sum()
    }

    pub fn set(&mut self, coord: ChunkCoord, local: u16, m: MaterialId) {
        let lista = self.chunks.entry(coord).or_default();
        match lista.binary_search_by_key(&local, |(k, _)| *k) {
            Ok(i) => lista[i].1 = m,
            Err(i) => lista.insert(i, (local, m)),
        }
    }

    #[must_use]
    pub fn get(&self, coord: ChunkCoord) -> Option<&[(u16, MaterialId)]> {
        self.chunks.get(&coord).map(Vec::as_slice)
    }

    /// Nakłada zmiany na świeżo zmaterializowany chunk **danego poziomu szczegółowości**.
    ///
    /// Nakładka jest zawsze indeksowana w LOD0, bo w tych jednostkach przychodzą komendy.
    /// Chunk LOD n pokrywa `2^n` chunków LOD0 w każdej osi, więc trzeba przejrzeć wszystkie
    /// i zrzutować voxele na grubszą siatkę. Bez tego budynek znika, gdy tylko kamera
    /// odsunie się poza pierścień LOD0 (192 m) — czyli w każdym widoku dzielnicy.
    pub fn apply_to(&self, coord: ChunkCoord, lod: u8, b: &mut ChunkBuilder) {
        let d = CHUNK_DIM as i32;
        if lod == 0 {
            if let Some(lista) = self.chunks.get(&coord) {
                for (local, m) in lista {
                    let (x, y, z) = crate::chunk::unlin(*local as usize);
                    b.set(x as i32, y as i32, z as i32, *m);
                }
            }
            return;
        }
        let n = 1i32 << lod;
        let (x0, y0, z0) = (coord.x * n, coord.y * n, i32::from(coord.z) * n);
        // Zakres po `x` wycina `BTreeMap`; `y` i `z` odsiewa filtr. Porządek iteracji
        // pozostaje porządkiem klucza, więc o zwycięzcy kolizji decyduje ten sam
        // deterministyczny wybór przy 1 i przy 16 wątkach (00 §3.2).
        let lo = ChunkCoord::new(x0, i32::MIN, i16::MIN);
        let hi = ChunkCoord::new(x0 + n - 1, i32::MAX, i16::MAX);
        for (c, lista) in self.chunks.range(lo..=hi) {
            if c.y < y0 || c.y >= y0 + n || i32::from(c.z) < z0 || i32::from(c.z) >= z0 + n {
                continue;
            }
            let (dx, dy, dz) = ((c.x - x0) * d, (c.y - y0) * d, (i32::from(c.z) - z0) * d);
            for (local, m) in lista {
                let (x, y, z) = crate::chunk::unlin(*local as usize);
                b.set(
                    (dx + x as i32) >> lod,
                    (dy + y as i32) >> lod,
                    (dz + z as i32) >> lod,
                    *m,
                );
            }
        }
    }

    /// Wpisuje do nakładki zbiór komend w **porządku kanonicznym** i zwraca dotknięte chunki.
    pub fn extend_from(&mut self, cmds: &[VoxelEditCmd]) -> (Vec<ChunkCoord>, u64) {
        let dim = CHUNK_DIM as i32;
        let mut dotkniete = Vec::new();
        let mut zapisanych = 0u64;
        for (coord, indeksy) in crate::edit::bucket_by_chunk(cmds) {
            let origin = coord.origin_voxels(0);
            let mut zapis: SeededMap<u16, MaterialId> = magnat_core::seeded_map();
            for i in indeksy {
                crate::edit::rasterize(&cmds[i], origin, 0, dim, &mut |x, y, z, m| {
                    zapis.insert(crate::chunk::lin(x as u32, y as u32, z as u32) as u16, m);
                });
            }
            // Kolejność zapisu do nakładki: rosnący indeks lokalny, nie kolejność wstawiania
            // do mapy — nakładka jest stanem trwałym i wchodzi do hasha (00 §3.2, §3.6).
            let mut wpisy: Vec<(u16, MaterialId)> = zapis.into_iter().collect();
            wpisy.sort_by_key(|(k, _)| *k);
            for (local, m) in wpisy {
                self.set(coord, local, m);
                zapisanych += 1;
            }
            dotkniete.push(coord);
        }
        (dotkniete, zapisanych)
    }
}

/// Pula chunków sterowana kamerą.
pub struct VoxelWorld {
    src: Arc<dyn ColumnSource>,
    budget: VoxelBudget,
    /// Rezydentne chunki. `BTreeMap` — kolejność iteracji jest deterministyczna (00 §3.2).
    resident: BTreeMap<ChunkCoord, Chunk>,
    /// Znacznik ostatniego użycia, do wywłaszczania LRU.
    touched: BTreeMap<ChunkCoord, u64>,
    overlay: EditOverlay,
    zegar: u64,
    bytes: usize,
    evicted: u64,
    lod0_radius_m: i32,
    /// Materiał wody — potrzebny tylko do statystyk i do testów; pusty rejestr jest dozwolony.
    air: MaterialId,
}

impl VoxelWorld {
    #[must_use]
    pub fn new(src: Arc<dyn ColumnSource>, budget: VoxelBudget) -> VoxelWorld {
        VoxelWorld {
            src,
            budget,
            resident: BTreeMap::new(),
            touched: BTreeMap::new(),
            overlay: EditOverlay::default(),
            zegar: 0,
            bytes: 0,
            evicted: 0,
            lod0_radius_m: LOD_RADII_M[0],
            air: MaterialId::AIR,
        }
    }

    #[must_use]
    pub fn stats(&self) -> VoxelStats {
        VoxelStats {
            resident: self.resident.len(),
            bytes: self.bytes,
            edited_chunks: self.overlay.len(),
            evicted_total: self.evicted,
            lod0_radius_m: self.lod0_radius_m,
        }
    }

    #[must_use]
    pub fn overlay(&self) -> &EditOverlay {
        &self.overlay
    }

    /// LOD dla zadanej **odległości w metrach**, z histerezą.
    ///
    /// Odległość, a nie współrzędna chunka — i to jest istotna różnica. Współrzędna chunka
    /// jest **względna dla poziomu szczegółowości**: `ChunkCoord(1, 0, 0)` przy LOD0 leży
    /// 32 m od początku układu, a przy LOD3 — 256 m. Funkcja przyjmująca samą współrzędną
    /// musiałaby zgadywać, o który poziom chodzi, i myliła się dokładnie tam, gdzie pomyłki
    /// nie widać: strumieniowanie prosiło o warstwy pionowe policzone dla LOD0, dostawało je
    /// jako LOD1 i teren nie pojawiał się w ogóle.
    ///
    /// `None` = poza zasięgiem strumieniowania (tam rysuje clipmapa terenu).
    #[must_use]
    pub fn lod_for_distance(&self, dist_m: i32, current: Option<u8>) -> Option<u8> {
        for (lod, r) in LOD_RADII_M.iter().enumerate() {
            let promien = if lod == 0 { self.lod0_radius_m } else { *r };
            // Histereza: chunk już będący w tym LOD trzyma się go dłużej, chunk spoza
            // wchodzi wcześniej. Progi rozjeżdżają się o ±15 %, więc nie ma punktu,
            // w którym decyzja miota się co klatkę.
            let prog = if current == Some(lod as u8) {
                promien + promien * LOD_HYSTERESIS_PERMILLE / 1000
            } else {
                promien - promien * LOD_HYSTERESIS_PERMILLE / 1000
            };
            if dist_m <= prog {
                return Some(lod as u8);
            }
        }
        None
    }

    /// LOD, w jakim chunk ma być rezydentny. `lod_of_coord` mówi, w jakiej skali podana
    /// jest współrzędna — patrz [`VoxelWorld::lod_for_distance`].
    #[must_use]
    pub fn desired_lod(
        &self,
        coord: ChunkCoord,
        lod_of_coord: u8,
        view: ViewPoint,
        current: Option<u8>,
    ) -> Option<u8> {
        self.lod_for_distance(self.distance_m(coord, lod_of_coord, view), current)
    }

    /// Środek chunka w voxelach świata, dla współrzędnej w skali `lod`.
    #[must_use]
    pub fn chunk_center(coord: ChunkCoord, lod: u8) -> IVec3 {
        let d = (CHUNK_DIM as i32) << lod;
        IVec3::new(
            coord.x * d + d / 2,
            coord.y * d + d / 2,
            i32::from(coord.z) * d + d / 2,
        )
    }

    fn distance_m(&self, coord: ChunkCoord, lod: u8, view: ViewPoint) -> i32 {
        // Pion liczy się w jednostkach 0,5 m, więc przed porównaniem trzeba go sprowadzić
        // do metrów — inaczej pionowe pierścienie byłyby dwa razy za ciasne.
        let c = Self::chunk_center(coord, lod);
        let (dx, dy, dz) = (
            i64::from(c.x - view.pos.x),
            i64::from(c.y - view.pos.y),
            i64::from(c.z - view.pos.z) / 2,
        );
        magnat_core::det_math::sqrt((dx * dx + dy * dy + dz * dz) as f64) as i32
    }

    /// Aktualizuje rezydencję: materializuje brakujące chunki w kolejności priorytetu
    /// i wywłaszcza najdawniej używane, gdy budżet się kończy.
    ///
    /// Zwraca liczbę chunków zmaterializowanych w tym wywołaniu — ograniczoną przez
    /// `max_in_flight`, żeby jedno przesunięcie kamery nie zatrzymało klatki na sekundę.
    pub fn update_residency(&mut self, view: ViewPoint, wanted: &[ChunkCoord]) -> usize {
        use std::collections::BinaryHeap;
        self.zegar += 1;

        let mut kolejka: BinaryHeap<Request> = BinaryHeap::new();
        for coord in wanted {
            let obecny = self.resident.get(coord).map(|c| c.lod);
            // Współrzędne w `wanted` są w skali LOD, w jakiej chunk już jest, albo LOD0
            // dla nowych — wywołujący przechodzi po pierścieniach, więc zna ją z góry.
            let Some(lod) = self.desired_lod(*coord, obecny.unwrap_or(0), view, obecny) else {
                continue;
            };
            if obecny == Some(lod) {
                self.touched.insert(*coord, self.zegar);
                continue;
            }
            kolejka.push(Request {
                lod,
                dist_sq: Self::chunk_center(*coord, lod).distance_sq(view.pos),
                coord: *coord,
            });
        }

        let mut zrobione = 0;
        while let Some(req) = kolejka.pop() {
            if zrobione >= self.budget.max_in_flight {
                break;
            }
            self.materialize(req.coord, req.lod);
            zrobione += 1;
            self.enforce_budget(req.coord);
        }
        zrobione
    }

    /// Materializuje chunk i wstawia go do puli.
    pub fn materialize(&mut self, coord: ChunkCoord, lod: u8) {
        assert!(lod <= MAX_LOD);
        let mut b = ChunkBuilder::new(coord, lod, 0);
        aggregate(self.src.as_ref(), coord, lod, &mut b);
        // Nakładka edycji ma pierwszeństwo przed generatorem — to ona jest zapisem gry.
        self.overlay.apply_to(coord, lod, &mut b);

        let rewizja = self.resident.get(&coord).map_or(0, |c| c.revision);
        let (palette, storage) = b.finish();
        let chunk = Chunk {
            coord,
            lod,
            palette,
            storage,
            revision: rewizja,
            state: ChunkState::Ready,
        };
        if let Some(stary) = self.resident.remove(&coord) {
            self.bytes -= stary.bytes();
        }
        self.bytes += chunk.bytes();
        self.resident.insert(coord, chunk);
        self.touched.insert(coord, self.zegar);
    }

    /// Wywłaszczanie LRU do granicy budżetu.
    ///
    /// Po wyczyszczeniu wszystkiego, co da się wyrzucić, a wciąż przekroczonym budżecie —
    /// **obniża promień LOD0**. To jest reguła z ryzyka R5: budżet nie ustępuje, ustępuje
    /// zasięg widzenia.
    fn enforce_budget(&mut self, chroniony: ChunkCoord) {
        if self.bytes <= self.budget.chunk_pool_bytes {
            return;
        }
        // Kandydaci w kolejności rosnącego znacznika czasu; remis po współrzędnej.
        let mut kandydaci: Vec<(u64, ChunkCoord)> =
            self.touched.iter().map(|(c, t)| (*t, *c)).collect();
        kandydaci.sort();

        for (_, coord) in kandydaci {
            if self.bytes <= self.budget.chunk_pool_bytes {
                return;
            }
            // Chunka, który właśnie powstał, nie wyrzucamy: to on jest powodem, dla którego
            // rezydencja w ogóle się aktualizowała. Bez tego wyjątku pula oscylowałaby
            // między „zmaterializuj" a „wyrzuć" i budżet nigdy by nie zabolał — a ma zaboleć
            // zasięgiem widzenia, bo to jest jedyna rzecz, która może ustąpić (ryzyko R5).
            if coord == chroniony {
                continue;
            }
            if let Some(c) = self.resident.remove(&coord) {
                self.bytes -= c.bytes();
                self.touched.remove(&coord);
                self.evicted += 1;
            }
        }

        if self.bytes > self.budget.chunk_pool_bytes {
            self.lod0_radius_m = (self.lod0_radius_m * 3 / 4).max(32);
        }
    }

    /// Materiał w punkcie świata. `MaterialId::AIR` dla chunków nierezydentnych —
    /// zapytanie o voxel poza pulą nie materializuje go, bo to otwierałoby drogę do
    /// przypadkowego wciągnięcia całej mapy jednym `for`.
    #[must_use]
    pub fn get(&self, pos: IVec3) -> MaterialId {
        let d = CHUNK_DIM as i32;
        let coord = ChunkCoord::new(
            pos.x.div_euclid(d),
            pos.y.div_euclid(d),
            pos.z.div_euclid(d) as i16,
        );
        let Some(c) = self.resident.get(&coord) else {
            return self.air;
        };
        let skala = 1i32 << c.lod;
        let lokalne = (
            (pos.x.rem_euclid(d * skala) / skala) as u32,
            (pos.y.rem_euclid(d * skala) / skala) as u32,
            (pos.z.rem_euclid(d * skala) / skala) as u32,
        );
        c.material_at(lokalne.0, lokalne.1, lokalne.2)
    }

    /// **Faza B** — jedyne miejsce, w którym voxele się zmieniają (M1 §5.4a).
    ///
    /// Kolejność jest kanoniczna (patrz [`crate::edit`]), rozdział po chunkach jest wyliczony
    /// z zasięgów, a chunki są rozłączne — więc zastosowanie po chunkach mogłoby iść
    /// równolegle i dałoby ten sam wynik. Na razie idzie sekwencyjnie: punkt synchronizacji
    /// zdarza się raz na tick, a liczba komend jest rzędu setek.
    ///
    /// ponytail: sekwencyjne stosowanie. Sufit — tysiące komend w jednym ticku (M6 drążący
    /// kopalnie). Ścieżka wyjścia: `for_each_chunk_mut` po kubełkach, bez zmiany kontraktu.
    pub fn apply_edits(&mut self, q: &EditQueue) -> EditReport {
        let cmds = q.drain_sorted();
        let mut report = EditReport {
            commands: cmds.len(),
            overlaps: find_overlaps(&cmds),
            ..EditReport::default()
        };
        if cmds.is_empty() {
            return report;
        }

        let (dotkniete, zapisanych) = self.overlay.extend_from(&cmds);
        report.chunks_touched = dotkniete.len();
        report.voxels_written = zapisanych;
        for coord in dotkniete {
            if let Some(c) = self.resident.get_mut(&coord) {
                c.revision += 1;
            }
            // Chunk rezydentny odświeżamy natychmiast, żeby zapytania i mesh widziały zmianę.
            if self.resident.contains_key(&coord) {
                let lod = self.resident[&coord].lod;
                self.materialize(coord, lod);
            }
        }
        report
    }
}

/// Pomocnik dla testów i dla `engine/render`: rozpakowanie chunka do bufora roboczego.
#[must_use]
pub fn decompress(chunk: &Chunk) -> Box<[LocalIdx; CHUNK_VOXELS]> {
    let mut out = Box::new([LocalIdx::AIR; CHUNK_VOXELS]);
    chunk.storage.decompress_into(&chunk.palette, &mut out);
    out
}

/// Paleta pustego chunka — używane przy tworzeniu zastępczych chunków w testach.
#[must_use]
pub fn empty_storage() -> (Palette, ChunkStorage) {
    (Palette::new(), ChunkStorage::Uniform(MaterialId::AIR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{EditOp, EditSource};
    use magnat_core::IAabb3;

    /// Źródło testowe: płaski teren na wysokości 16 voxeli.
    struct Plaski(MaterialId);

    impl ColumnSource for Plaski {
        fn fill_chunk(&self, coord: ChunkCoord, lod: u8, out: &mut ChunkBuilder) {
            let skala = 1i32 << lod;
            let origin = coord.origin_voxels(lod);
            let r = out.range();
            for y in r.clone() {
                for x in r.clone() {
                    let lokalna = (16 - origin.z).div_euclid(skala);
                    out.fill_column(x, y, -(out.pad() as i32), lokalna, self.0);
                }
            }
        }
    }

    fn swiat() -> VoxelWorld {
        VoxelWorld::new(Arc::new(Plaski(MaterialId(4))), VoxelBudget::default())
    }

    #[test]
    fn lod_rosnie_z_odlegloscia() {
        let w = swiat();
        let view = ViewPoint {
            pos: IVec3::new(0, 0, 32),
        };
        // Chunk pod kamerą: LOD0. Coraz dalej: coraz grubszy agregat, aż poza zasięg.
        assert_eq!(
            w.desired_lod(ChunkCoord::new(0, 0, 0), 0, view, None),
            Some(0)
        );
        assert_eq!(
            w.desired_lod(ChunkCoord::new(10, 0, 0), 0, view, None),
            Some(1)
        );
        assert_eq!(
            w.desired_lod(ChunkCoord::new(30, 0, 0), 0, view, None),
            Some(2)
        );
        assert_eq!(
            w.desired_lod(ChunkCoord::new(100, 0, 0), 0, view, None),
            Some(3)
        );
        assert_eq!(
            w.desired_lod(ChunkCoord::new(300, 0, 0), 0, view, None),
            None
        );

        // Ta sama współrzędna w innej skali leży gdzie indziej — i dostaje inny LOD.
        assert_eq!(
            w.desired_lod(ChunkCoord::new(10, 0, 0), 3, view, None),
            Some(3)
        );
    }

    #[test]
    fn histereza_trzyma_chunk_w_dotychczasowym_lod() {
        let w = swiat();
        let view = ViewPoint {
            pos: IVec3::new(0, 0, 32),
        };
        // Chunk tuż za progiem LOD0 (192 m): wchodząc, jeszcze nie dostaje LOD0;
        // będąc w LOD0, jeszcze go nie traci. To jest cały sens histerezy.
        let przy_progu = ChunkCoord::new(6, 0, 0); // środek ~208 m
        let wchodzacy = w.desired_lod(przy_progu, 0, view, None);
        let trwajacy = w.desired_lod(przy_progu, 0, view, Some(0));
        assert_eq!(wchodzacy, Some(1), "chunk wszedł do LOD0 za wcześnie");
        assert_eq!(trwajacy, Some(0), "chunk wypadł z LOD0 za wcześnie");
    }

    #[test]
    fn rezydencja_materializuje_i_odpytuje() {
        let mut w = swiat();
        let view = ViewPoint {
            pos: IVec3::new(16, 16, 32),
        };
        let chciane: Vec<ChunkCoord> = (0..3)
            .flat_map(|y| (0..3).map(move |x| ChunkCoord::new(x, y, 0)))
            .collect();
        let n = w.update_residency(view, &chciane);
        assert_eq!(n, 9, "zmaterializowano {n} chunków zamiast dziewięciu");
        assert_eq!(w.stats().resident, 9);
        assert!(w.stats().bytes > 0);

        assert_eq!(
            w.get(IVec3::new(5, 5, 10)),
            MaterialId(4),
            "teren nie istnieje"
        );
        assert_eq!(
            w.get(IVec3::new(5, 5, 20)),
            MaterialId::AIR,
            "teren za wysoki"
        );
        // Chunk spoza puli nie jest materializowany przy okazji odczytu.
        assert_eq!(w.get(IVec3::new(5000, 5000, 10)), MaterialId::AIR);
    }

    #[test]
    fn budzet_wywlaszcza_zamiast_rosnac() {
        // Ryzyko R5: budżet jest asercją. Pula o limicie na trzy chunki ma wyrzucać
        // najdawniej używane, a nie przekraczać limit.
        let jeden = {
            let mut w = swiat();
            w.materialize(ChunkCoord::new(0, 0, 0), 0);
            w.stats().bytes
        };
        let limit = jeden * 3;
        let mut w = VoxelWorld::new(
            Arc::new(Plaski(MaterialId(4))),
            VoxelBudget {
                chunk_pool_bytes: limit,
                max_in_flight: 64,
            },
        );
        let view = ViewPoint {
            pos: IVec3::new(0, 0, 32),
        };
        let chciane: Vec<ChunkCoord> = (0..4)
            .flat_map(|y| (0..4).map(move |x| ChunkCoord::new(x, y, 0)))
            .collect();
        w.update_residency(view, &chciane);
        assert!(
            w.stats().bytes <= limit,
            "pula urosła do {} B ponad limit {limit} B",
            w.stats().bytes
        );
        assert!(w.stats().evicted_total > 0, "nic nie zostało wywłaszczone");
        assert_eq!(
            w.stats().lod0_radius_m,
            LOD_RADII_M[0],
            "zasięg ustąpił, mimo że wywłaszczanie wystarczyło"
        );
    }

    #[test]
    fn przy_budzecie_mniejszym_niz_chunk_ustepuje_zasieg() {
        // Granica, w której wywłaszczanie już nie pomaga: jeden chunk nie mieści się
        // w budżecie. Wtedy — i tylko wtedy — kurczy się promień LOD0.
        let mut w = VoxelWorld::new(
            Arc::new(Plaski(MaterialId(4))),
            VoxelBudget {
                chunk_pool_bytes: 16,
                max_in_flight: 8,
            },
        );
        let view = ViewPoint {
            pos: IVec3::new(0, 0, 32),
        };
        let chciane: Vec<ChunkCoord> = (0..3)
            .flat_map(|y| (0..3).map(move |x| ChunkCoord::new(x, y, 0)))
            .collect();
        w.update_residency(view, &chciane);
        assert!(
            w.stats().lod0_radius_m < LOD_RADII_M[0],
            "promień LOD0 nie ustąpił mimo wyczerpania budżetu"
        );
        assert!(
            w.stats().resident <= 1,
            "w puli zostało {} chunków",
            w.stats().resident
        );
    }

    #[test]
    fn edycja_dziala_na_chunku_nierezydentnym() {
        // M1 §5.4a: M2 stawia budynek 3 km od kamery. Nakładka jest stanem trwałym
        // i nie wymaga strumieniowania ani GPU.
        let mut w = swiat();
        let q = EditQueue::new();
        q.push(
            EditSource(2),
            EditOp::Fill {
                aabb: IAabb3::new(IVec3::new(3000, 3000, 16), IVec3::new(3004, 3004, 20)),
                material: MaterialId(9),
            },
        );
        let r = w.apply_edits(&q);
        assert_eq!(r.commands, 1);
        assert_eq!(r.voxels_written, 4 * 4 * 4);
        assert_eq!(w.stats().edited_chunks, 1);
        assert_eq!(w.stats().resident, 0, "edycja wciągnęła chunk do puli");

        // Po zmaterializowaniu chunka edycja jest widoczna.
        w.materialize(ChunkCoord::new(93, 93, 0), 0);
        assert_eq!(w.get(IVec3::new(3001, 3001, 17)), MaterialId(9));
    }

    #[test]
    fn edycja_przecinajaca_granice_chunka_dziala_w_calosci() {
        // M1 §7.1 `edits_cross_chunk`.
        let mut w = swiat();
        let q = EditQueue::new();
        q.push(
            EditSource(2),
            EditOp::Fill {
                aabb: IAabb3::new(IVec3::new(30, 30, 16), IVec3::new(35, 35, 18)),
                material: MaterialId(9),
            },
        );
        let r = w.apply_edits(&q);
        assert_eq!(
            r.chunks_touched, 4,
            "komenda nie trafiła do wszystkich chunków"
        );
        assert_eq!(r.voxels_written, 5 * 5 * 2);

        for c in [
            ChunkCoord::new(0, 0, 0),
            ChunkCoord::new(1, 0, 0),
            ChunkCoord::new(0, 1, 0),
            ChunkCoord::new(1, 1, 0),
        ] {
            w.materialize(c, 0);
        }
        for (x, y) in [(30, 30), (34, 34), (31, 33), (34, 30)] {
            assert_eq!(w.get(IVec3::new(x, y, 17)), MaterialId(9), "({x}, {y})");
        }
    }

    #[test]
    fn kolejnosc_kolejkowania_nie_zmienia_stanu_voxeli() {
        // M1 §7.1 `edits_order_invariant`: ten sam zbiór komend w losowej kolejności
        // daje identyczny stan i identyczny raport.
        let komendy = |kolejnosc: &[usize]| {
            let q = EditQueue::new();
            let ops = [
                EditOp::Fill {
                    aabb: IAabb3::new(IVec3::new(0, 0, 16), IVec3::new(8, 8, 18)),
                    material: MaterialId(5),
                },
                EditOp::Fill {
                    aabb: IAabb3::new(IVec3::new(4, 4, 16), IVec3::new(12, 12, 18)),
                    material: MaterialId(6),
                },
                EditOp::Carve {
                    shape: crate::edit::CarveShape::Box(IAabb3::new(
                        IVec3::new(6, 6, 16),
                        IVec3::new(7, 7, 18),
                    )),
                },
            ];
            for i in kolejnosc {
                q.push(EditSource(1), ops[*i].clone());
            }
            let mut w = swiat();
            let r = w.apply_edits(&q);
            w.materialize(ChunkCoord::new(0, 0, 0), 0);
            let stan: Vec<MaterialId> = (0..16)
                .flat_map(|y| (0..16).map(move |x| (x, y)))
                .map(|(x, y)| w.get(IVec3::new(x, y, 17)))
                .collect();
            (stan, r.overlaps.len(), r.voxels_written)
        };

        let wzorzec = komendy(&[0, 1, 2]);
        for kolejnosc in [[2, 1, 0], [1, 0, 2], [2, 0, 1], [1, 2, 0]] {
            assert_eq!(komendy(&kolejnosc), wzorzec, "kolejność {kolejnosc:?}");
        }
        // Nakładki są raportowane, a nie odrzucane.
        assert!(
            wzorzec.1 > 0,
            "nakładające się komendy nie zostały zgłoszone"
        );
    }

    #[test]
    fn rewizja_rosnie_przy_edycji_rezydentnego_chunka() {
        let mut w = swiat();
        w.materialize(ChunkCoord::new(0, 0, 0), 0);
        let q = EditQueue::new();
        q.push(
            EditSource(1),
            EditOp::Fill {
                aabb: IAabb3::new(IVec3::new(1, 1, 16), IVec3::new(2, 2, 17)),
                material: MaterialId(9),
            },
        );
        w.apply_edits(&q);
        assert!(
            w.resident[&ChunkCoord::new(0, 0, 0)].revision > 0,
            "rewizja nie wzrosła — mesh nie zostanie unieważniony"
        );
    }
}
