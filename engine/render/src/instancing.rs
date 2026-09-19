//! Instancing encji dynamicznych (M11a §5.3, WP2).
//!
//! Ścieżka klatki: snapshot → filtr stożka → klasyfikacja LOD → sortowanie po
//! `(model, lod)` → bufor instancji → po jednym wywołaniu rysowania na wsad.
//! Sortowanie jest tym, co sprowadza dwadzieścia tysięcy encji do kilkunastu wywołań:
//! wszystkie instancje tego samego modelu i poziomu detalu leżą obok siebie.
//!
//! ### Konwencja współrzędnych — obowiązkowo identyczna z M1 (`R11`)
//!
//! M1 trzyma pozycję kamery w `f64` (mapa ma 16 km, `f32` traci precyzję na krawędziach)
//! i do shaderów przekazuje pozycje **względem kamery**. Robimy tak samo i w tej samej
//! kolejności: pozycja z rekordu (`i32`, milimetry) **minus** origin kamery w `f64`,
//! i dopiero wynik różnicy rzutowany na `f32`.
//!
//! Odejmowanie **przed** konwersją, nigdy po. Przy współrzędnej rzędu 8 km krok `f32`
//! to ~0,5 m, więc kolejność odwrotna sprawia, że pieszy stojący w miejscu skacze między
//! klatkami — a objaw wygląda na błąd symulacji i szuka się go w złym miejscu. Pilnuje
//! tego test `encja_przy_krawedzi_mapy_nie_drga`.

use glam::{DVec3, Vec4};
use magnat_sim_snapshot::{CitizenRenderRec, RenderSnapshot, VehicleRenderRec};
use magnat_voxel::{ModelId, LOD_COUNT};

mod gpu;
pub use gpu::{decode_pick, InstanceRenderer, PickHit, PickKind, PICK_ENTITY_BITS};

/// Za tym promieniem encja jest mniejsza od piksela i przestaje być rysowana.
pub const DRAW_RADIUS_M: f32 = 600.0;

/// Ile instancji mieści bufor GPU w jednej klatce.
///
/// Cap snapshotu to 24 576 mieszkańców i 8 192 pojazdy, więc 32 768 pokrywa najgorszy
/// przypadek z zapasem zerowym i to jest zamierzone: liczby biorą się z jednego miejsca
/// (`SnapshotCaps`), a bufor na więcej byłby pamięcią pod stan, który nie może zaistnieć.
pub const MAX_INSTANCES: usize = 32_768;

/// Instancja w buforze GPU — 36 B, ta sama dla postaci, pojazdów i propów.
///
/// Dwie różnice wobec §5.3 i obie mają ten sam powód — pole, którego szkic nie miał,
/// jest potrzebne, żeby cokolwiek zadziałało.
///
/// **1. `extra` → `variant`.** Shader **musi** dostać wariant wyglądu: barwę wybiera
/// z zestawu palety funkcją `(variant, rola)` (`E-7`), a bez tej liczby cała dzielnica
/// chodziłaby w jednym kolorze. Wyparte `carry`/`load`/`wheel_phase` przeniosły się
/// do `anim`, które miało wolny bajt.
///
/// **2. Doszedł `pick` i instancja urosła z 32 B do 36.** Pass `pick_id` rysuje tę samą
/// geometrię co pass sceny i zapisuje identyfikator encji — a identyfikatora nie ma
/// skąd wziąć, jeśli nie jedzie w instancji. §5.3 wyliczał 32 B, nie wymieniając w nich
/// pickingu, choć §5.2 trzyma `entity_lo` „do pickingu" od pierwszej wersji. Koszt:
/// 720 KB uploadu na klatkę zamiast 640 KB, czyli 43 MB/s zamiast 38.
#[derive(Clone, Copy, PartialEq, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct GpuInstance {
    /// Pozycja **względem origin kamery**, w metrach.
    pub pos: [f32; 3],
    /// `yaw` w młodszych 16 bitach (1/65536 obrotu), `pitch` ze znakiem w starszych.
    pub yaw_pitch: u32,
    /// `DistrictPaletteId` — który zestaw barw obowiązuje w tej dzielnicy i epoce.
    pub palette_base: u16,
    /// `(ModelId << 2) | lod` — klucz wsadu rysowania.
    pub model_lod: u16,
    /// `clip | phase << 8 | aux << 16 | flags << 24`; `aux` to `carry`, `load`
    /// albo `wheel_phase`, zależnie od rodzaju encji.
    pub anim: u32,
    /// RGBA8 — filtry i podświetlenia z §14.2. Zero = bez zmiany.
    pub tint: u32,
    /// Wariant wyglądu: `Appearance` mieszkańca albo `paint | livery << 16` pojazdu.
    pub variant: u32,
    /// `(rodzaj << 28) | entity_lo` — to, co pass `pick_id` zapisuje do bufora
    /// identyfikatorów. Rodzaj jest tu, bo mieszkaniec i pojazd mają osobne karty
    /// i sam numer encji nie mówi, którą otworzyć (`K-62`).
    pub pick: u32,
}

/// Jeden wsad rysowania: ciągły zakres instancji o tym samym `(model, lod)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Batch {
    pub model_lod: u16,
    pub first: u32,
    pub count: u32,
}

/// Gdzie w scalonych buforach leży siatka jednego `(model, lod)`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct MeshSlot {
    pub index_offset: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    /// Promień kuli otaczającej w metrach — wejście do testu stożka widzenia.
    pub radius_m: f32,
}

/// Który model rysuje którą encję i gdzie leży jego geometria.
///
/// Odwzorowanie `VehicleModelId → ModelId` jest na razie stałe: wszystkie pojazdy jadą
/// bryłą `car`. ponytail: sufit jest jawny i kończy się razem z katalogiem modeli
/// pojazdów — wtedy tablica dostanie wiersz na klasę z `data/vehicles/classes.ron`,
/// a reszta ścieżki nie drgnie, bo już teraz sortuje po `ModelId`.
#[derive(Clone, Debug, Default)]
pub struct ModelTable {
    slots: Vec<MeshSlot>,
    citizen: ModelId,
    vehicle: ModelId,
    /// Który model ma wypalone kafle impostora. Bez tego encja modelu bez kafli
    /// zniknęłaby za progiem L2 zamiast zostać przy siatce uproszczonej.
    impostor: Vec<bool>,
}

impl ModelTable {
    #[must_use]
    pub fn new(slots: Vec<MeshSlot>, citizen: ModelId, vehicle: ModelId) -> ModelTable {
        ModelTable {
            slots,
            citizen,
            vehicle,
            impostor: Vec::new(),
        }
    }

    /// Zaznacza modele, dla których atlas ma kafle (`ImpostorAtlas::bake`).
    #[must_use]
    pub fn with_impostors(mut self, impostor: Vec<bool>) -> ModelTable {
        self.impostor = impostor;
        self
    }

    #[must_use]
    pub fn has_impostor(&self, model: ModelId) -> bool {
        self.impostor
            .get(model.0 as usize)
            .copied()
            .unwrap_or(false)
    }

    #[must_use]
    pub fn slot(&self, model_lod: u16) -> MeshSlot {
        let model = (model_lod >> 2) as usize;
        let lod = (model_lod & 0b11) as usize;
        self.slots
            .get(model * LOD_COUNT as usize + lod)
            .copied()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn citizen(&self) -> ModelId {
        self.citizen
    }

    #[must_use]
    pub fn vehicle(&self) -> ModelId {
        self.vehicle
    }
}

#[must_use]
pub const fn model_lod(model: ModelId, lod: u8) -> u16 {
    (model.0 << 2) | (lod as u16 & 0b11)
}

/// Poziom detalu z klucza wsadu.
#[must_use]
pub const fn lod_of(model_lod: u16) -> u8 {
    (model_lod & 0b11) as u8
}

/// Poziom, na którym encję rysuje kafel atlasu sylwetek zamiast siatki.
pub const LOD_IMPOSTOR: u8 = 2;

/// Progi poziomu detalu jednego rodzaju encji, w metrach (§5.5).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bands {
    /// Za tym progiem model schodzi na siatkę uproszczoną (L1).
    pub l1_m: f32,
    /// Za tym progiem zostaje impostor (L2) — dwa trójkąty z atlasu sylwetek.
    pub l2_m: f32,
    /// Za tym progiem encja nie jest rysowana wcale.
    pub draw_m: f32,
}

/// Progi poziomu detalu wizualnego (§5.5, WP4).
///
/// Mieszkaniec i pojazd mają **osobne progi** i to nie jest kosmetyka: auto jest cztery
/// razy dłuższe od postaci, więc na tej samej odległości zajmuje kilka razy więcej
/// pikseli i uproszczenie widać na nim wcześniej.
///
/// Pasma L3 z §5.5 („plamka cienia + sylwetka 4×6 px") **nie ma i nie będzie**:
/// impostor rysowany z czterystu metrów zajmuje dokładnie tyle pikseli sam z siebie,
/// a osobny poziom kosztowałby drugi pipeline i drugi zestaw kafli po to, żeby narysować
/// to samo. Zamiast trzeciego progu jest `draw_m` — odległość, za którą encji nie widać.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LodBands {
    pub citizen: Bands,
    pub vehicle: Bands,
}

impl Default for LodBands {
    fn default() -> Self {
        LodBands {
            citizen: Bands {
                l1_m: 40.0,
                l2_m: 120.0,
                draw_m: DRAW_RADIUS_M,
            },
            vehicle: Bands {
                l1_m: 60.0,
                l2_m: 200.0,
                draw_m: DRAW_RADIUS_M,
            },
        }
    }
}

impl Bands {
    /// Poziom detalu dla odległości. `has_impostor == false` znaczy „model nie ma kafli",
    /// więc encja zostaje na siatce uproszczonej zamiast zniknąć — tak zachowywał się
    /// cały render do M11b (`F-4`) i tak ma się zachować model, którego nikt nie wypalił.
    #[must_use]
    pub fn lod_for(&self, dist_m: f32, has_impostor: bool) -> u8 {
        if dist_m <= self.l1_m {
            0
        } else if dist_m <= self.l2_m || !has_impostor {
            1
        } else {
            2
        }
    }
}

/// Bufory robocze ścieżki klatki. Trzymane między klatkami, żeby nie alokować w środku.
#[derive(Clone, Debug, Default)]
pub struct InstanceScratch {
    pub instances: Vec<GpuInstance>,
    pub batches: Vec<Batch>,
}

/// Składa bufor instancji z widocznych encji snapshotu.
///
/// Wejście jest już **posortowane po odległości** (selektor top-K snapshotu, §5.2), więc
/// sortowanie po `(model, lod)` jest stabilne i porządek wewnątrz wsadu zostaje odległościowy.
/// To nie jest kosmetyka: degradacja „najdalszych ponad limit" z §5.2 pkt 2 potrzebuje
/// tego porządku, a M11b dostaje go za darmo.
pub fn build_instances(
    snap: &RenderSnapshot,
    eye: DVec3,
    frustum: &[Vec4; 6],
    models: &ModelTable,
    bands: LodBands,
    out: &mut InstanceScratch,
) {
    out.instances.clear();
    out.batches.clear();

    let palety = &snap.district_palette;
    for c in snap.citizens.as_slice() {
        if let Some(i) = citizen_instance(c, eye, frustum, models, bands.citizen, palety) {
            if !push(&mut out.instances, i) {
                break;
            }
        }
    }
    for v in snap.vehicles.as_slice() {
        if let Some(i) = vehicle_instance(v, eye, frustum, models, bands.vehicle, palety) {
            if !push(&mut out.instances, i) {
                break;
            }
        }
    }

    // ponytail: sortowanie porównaniami zamiast radixa z §5.3. Różnych kluczy jest
    // rzędu dziesiątek (modele × 2 poziomy), więc sortowanie kubełkowe byłoby szybsze —
    // ale dopiero wtedy, gdy 0,2 ms z budżetu zacznie być widoczne w pomiarze M11e.
    // Do tego czasu jedna linia zamiast trzydziestu.
    out.instances.sort_by_key(|i| i.model_lod);

    let mut i = 0usize;
    while i < out.instances.len() {
        let k = out.instances[i].model_lod;
        let start = i;
        while i < out.instances.len() && out.instances[i].model_lod == k {
            i += 1;
        }
        out.batches.push(Batch {
            model_lod: k,
            first: start as u32,
            count: (i - start) as u32,
        });
    }
}

fn push(v: &mut Vec<GpuInstance>, i: GpuInstance) -> bool {
    if v.len() >= MAX_INSTANCES {
        return false;
    }
    v.push(i);
    true
}

/// Pozycja względem kamery: **odejmij w `f64`, potem rzutuj** (`R11`).
fn relative(pos_mm: [i32; 3], eye: DVec3) -> [f32; 3] {
    let w = DVec3::new(
        f64::from(pos_mm[0]) * 0.001,
        f64::from(pos_mm[1]) * 0.001,
        f64::from(pos_mm[2]) * 0.001,
    );
    (w - eye).as_vec3().into()
}

/// Promień kuli otaczającej modelu w pełnym detalu; przy pustej tablicy metr z okładem,
/// żeby test stożka nie odrzucał wszystkiego.
fn radius_of(models: &ModelTable, model: ModelId) -> f32 {
    let r = models.slot(model_lod(model, 0)).radius_m;
    if r > 0.0 {
        r
    } else {
        1.5
    }
}

fn visible(rel: [f32; 3], radius_m: f32, frustum: &[Vec4; 6], bands: Bands) -> Option<f32> {
    let p = glam::Vec3::from(rel);
    let d2 = p.length_squared();
    if d2 > bands.draw_m * bands.draw_m {
        return None;
    }
    // Środek kuli metr nad pozycją: rekord niesie punkt na gruncie, a bryła rośnie w górę.
    if !crate::renderer::sphere_in_frustum(frustum, p + glam::Vec3::new(0.0, 0.0, 1.0), radius_m) {
        return None;
    }
    Some(d2.sqrt())
}

fn citizen_instance(
    c: &CitizenRenderRec,
    eye: DVec3,
    frustum: &[Vec4; 6],
    models: &ModelTable,
    bands: Bands,
    palety: &[u16],
) -> Option<GpuInstance> {
    // Mieszkaniec w pojeździe jedzie sylwetką w kabinie, a nie własną bryłą obok auta.
    if c.flags & magnat_sim_snapshot::CITIZEN_FLAG_IN_VEHICLE != 0 {
        return None;
    }
    let rel = relative(c.pos, eye);
    let dist = visible(rel, radius_of(models, models.citizen()), frustum, bands)?;
    let lod = bands.lod_for(dist, models.has_impostor(models.citizen()));
    Some(GpuInstance {
        pos: rel,
        yaw_pitch: u32::from(c.yaw),
        palette_base: paleta(palety, c.district),
        model_lod: model_lod(models.citizen(), lod),
        anim: u32::from(c.anim_state)
            | (u32::from(c.anim_phase) << 8)
            | (u32::from(c.carry) << 16)
            | (u32::from(c.flags) << 24),
        tint: tint_of(c.flags, magnat_sim_snapshot::CITIZEN_FLAG_HIGHLIGHTED),
        variant: c.appearance,
        pick: pick_id(gpu::PickKind::Citizen, c.entity_lo),
    })
}

fn vehicle_instance(
    v: &VehicleRenderRec,
    eye: DVec3,
    frustum: &[Vec4; 6],
    models: &ModelTable,
    bands: Bands,
    palety: &[u16],
) -> Option<GpuInstance> {
    let rel = relative(v.pos, eye);
    let dist = visible(rel, radius_of(models, models.vehicle()), frustum, bands)?;
    let lod = bands.lod_for(dist, models.has_impostor(models.vehicle()));
    Some(GpuInstance {
        pos: rel,
        yaw_pitch: u32::from(v.yaw) | ((v.pitch as u16 as u32) << 16),
        palette_base: paleta(palety, v.district),
        model_lod: model_lod(models.vehicle(), lod),
        anim: u32::from(v.anim_state)
            | (u32::from(v.anim_phase) << 8)
            | (u32::from(v.wheel_phase) << 16)
            | (u32::from(v.flags) << 24),
        tint: tint_of(v.flags, magnat_sim_snapshot::VEHICLE_FLAG_HIGHLIGHTED),
        variant: u32::from(v.paint) | (u32::from(v.livery) << 16),
        pick: pick_id(gpu::PickKind::Vehicle, v.entity_lo),
    })
}

/// Paleta dzielnicy. Rekord niesie `DistrictId`, bo tego potrzebuje warstwa dźwiękowa;
/// zestaw barw wybiera tablica ze snapshotu, bo odwzorowanie „dzielnica × epoka → paleta"
/// jest wiedzą o mieście, której render nie ma.
fn paleta(palety: &[u16], district: u16) -> u16 {
    palety.get(district as usize).copied().unwrap_or(0)
}

/// Identyfikator do bufora ID: rodzaj w czterech starszych bitach, indeks encji
/// w dwudziestu ośmiu. Zero znaczy „nic" (tekstura jest czyszczona zerem), więc indeks
/// jest przesunięty o jeden — odwrotność siedzi w [`gpu::decode_pick`].
///
/// Encja o indeksie spoza dwudziestu ośmiu bitów dostaje **zero**, czyli jest widoczna
/// i nieklikalna. To jest jawny sufit, a nie zawijanie: przy 268 mln encji miasto i tak
/// nie istnieje, a `entity + 1` z zawinięciem dałoby kliknięcie trafiające w cudzą kartę.
#[must_use]
pub fn pick_id(kind: gpu::PickKind, entity: u32) -> u32 {
    const MAX: u32 = (1 << gpu::PICK_ENTITY_BITS) - 1;
    if entity >= MAX {
        return 0;
    }
    ((kind as u32) << gpu::PICK_ENTITY_BITS) | (entity + 1)
}

/// Podświetlenie encji wskazanej filtrem widoku (§14.2). Zero = bez zmiany barwy.
///
/// Bit podświetlenia jest **argumentem**, bo zestawy flag mieszkańca i pojazdu są
/// rozłączne: bit 2 znaczy u jednego „podświetlony", u drugiego „silnik pracuje".
fn tint_of(flags: u8, highlight_bit: u8) -> u32 {
    if flags & highlight_bit != 0 {
        // Ciepła żółć o połowicznej sile — podświetlenie ma wskazywać, a nie zamalowywać.
        0x8040_E0FF
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_sim_snapshot::SnapshotCaps;

    fn stozek_wszystko() -> [Vec4; 6] {
        // Sześć płaszczyzn o normalnych do środka i ogromnym przesunięciu: nic nie odpada.
        [
            Vec4::new(1.0, 0.0, 0.0, 1.0e6),
            Vec4::new(-1.0, 0.0, 0.0, 1.0e6),
            Vec4::new(0.0, 1.0, 0.0, 1.0e6),
            Vec4::new(0.0, -1.0, 0.0, 1.0e6),
            Vec4::new(0.0, 0.0, 1.0, 1.0e6),
            Vec4::new(0.0, 0.0, -1.0, 1.0e6),
        ]
    }

    fn tablica() -> ModelTable {
        let slot = MeshSlot {
            index_offset: 0,
            index_count: 36,
            base_vertex: 0,
            radius_m: 1.0,
        };
        ModelTable::new(vec![slot; 2 * LOD_COUNT as usize], ModelId(0), ModelId(1))
    }

    fn snapshot_z(mieszkancy: usize, pojazdy: usize) -> RenderSnapshot {
        let mut s = RenderSnapshot::new(SnapshotCaps::DEFAULT);
        for i in 0..mieszkancy {
            // Co drugi dalej niż próg L1, żeby powstały dwa poziomy detalu.
            let x = if i % 2 == 0 { 5_000 } else { 100_000 };
            s.citizens.push(CitizenRenderRec {
                pos: [x, 0, 0],
                entity_lo: i as u32,
                appearance: i as u32,
                ..Default::default()
            });
        }
        for i in 0..pojazdy {
            s.vehicles.push(VehicleRenderRec {
                pos: [3_000, 80_000 * i as i32 / 1_000, 0],
                entity_lo: 10_000 + i as u32,
                paint: i as u16,
                ..Default::default()
            });
        }
        s
    }

    /// Kryterium WP1 w postaci sprawdzalnej bez GPU: trzy warianty palety tego samego
    /// modelu na tym samym poziomie detalu to **jeden** wsad, czyli jedno wywołanie
    /// rysowania. Gdyby barwa siedziała w modelu, byłyby trzy.
    #[test]
    fn trzy_warianty_palety_to_jeden_wsad() {
        let mut s = RenderSnapshot::new(SnapshotCaps::DEFAULT);
        for (i, wariant) in [11u32, 22, 33].into_iter().enumerate() {
            s.citizens.push(CitizenRenderRec {
                pos: [1_000 * i as i32, 0, 0],
                entity_lo: i as u32,
                appearance: wariant,
                ..Default::default()
            });
        }
        let mut out = InstanceScratch::default();
        build_instances(
            &s,
            DVec3::ZERO,
            &stozek_wszystko(),
            &tablica(),
            LodBands::default(),
            &mut out,
        );
        assert_eq!(out.batches.len(), 1, "wsadów: {:?}", out.batches);
        assert_eq!(out.batches[0].count, 3);
        let warianty: Vec<u32> = out.instances.iter().map(|i| i.variant).collect();
        assert_eq!(warianty, vec![11, 22, 33]);
    }

    /// Kryterium WP2: 20 000 mieszkańców i 6 000 pojazdów schodzą do ≤ 24 wywołań
    /// rysowania. Wsadów jest tyle, ile par (model, poziom detalu) naprawdę w kadrze.
    #[test]
    fn dwadziescia_tysiecy_encji_miesci_sie_w_dwudziestu_czterech_wsadach() {
        let s = snapshot_z(20_000, 6_000);
        let mut out = InstanceScratch::default();
        build_instances(
            &s,
            DVec3::ZERO,
            &stozek_wszystko(),
            &tablica(),
            LodBands::default(),
            &mut out,
        );
        assert_eq!(out.instances.len(), 26_000);
        assert!(
            out.batches.len() <= 24,
            "{} wsadów, limit 24",
            out.batches.len()
        );
        // Dwa modele × dwa poziomy detalu, wszystkie cztery obsadzone.
        assert_eq!(out.batches.len(), 4);
        let suma: u32 = out.batches.iter().map(|b| b.count).sum();
        assert_eq!(suma as usize, out.instances.len(), "wsady gubią instancje");
    }

    /// Wsady muszą być **ciągłe i rozłączne**, inaczej `draw_indexed` rysuje cudze
    /// instancje cudzą siatką — i wygląda to jak uszkodzony model, nie jak błąd zakresu.
    #[test]
    fn wsady_pokrywaja_bufor_bez_dziur_i_zakladek() {
        let s = snapshot_z(1_000, 500);
        let mut out = InstanceScratch::default();
        build_instances(
            &s,
            DVec3::ZERO,
            &stozek_wszystko(),
            &tablica(),
            LodBands::default(),
            &mut out,
        );
        let mut oczekiwany = 0u32;
        for b in &out.batches {
            assert_eq!(b.first, oczekiwany, "dziura albo zakładka we wsadach");
            for i in &out.instances[b.first as usize..(b.first + b.count) as usize] {
                assert_eq!(i.model_lod, b.model_lod);
            }
            oczekiwany += b.count;
        }
        assert_eq!(oczekiwany as usize, out.instances.len());
    }

    /// `R11`: przy krawędzi mapy (16 km) kolejność „odejmij w `f64`, potem rzutuj"
    /// zachowuje milimetr, a kolejność odwrotna go gubi.
    ///
    /// Skala błędu jest przy tym **dwa rzędy mniejsza**, niż mówi §5.3 („krok ~0,5 m
    /// przy 8 km"): `f32` przy 16 000 m ma krok ok. 2 mm, a nie pół metra. Rzecz w tym,
    /// że ten krok jest **siatką, która przesuwa się razem z kamerą** — pieszy stojący
    /// w miejscu przeskakuje między jej oczkami przy każdym ruchu oka, a migotanie widać
    /// mimo drobnej amplitudy. Reguła zostaje bez zmian, bo kosztuje jedno odejmowanie.
    #[test]
    fn encja_przy_krawedzi_mapy_nie_drga() {
        let oko = DVec3::new(16_000.0, 16_000.0, 20.0);
        let baza = [16_000_000i32, 16_000_000, 0];

        // Nieruchoma encja daje tę samą liczbę przy każdym wywołaniu.
        let a = relative(baza, oko);
        for _ in 0..600 {
            assert_eq!(relative(baza, oko), a, "pozycja względna drga");
        }

        // Encja stojąca dokładnie w punkcie kamery ma dać zero co do bitu.
        assert_eq!(a[0], 0.0, "odejmowanie w f64 nie jest dokładne");
        // Ruch o milimetr ma dać dokładnie milimetr.
        let przesunieta = relative([baza[0] + 1, baza[1], baza[2]], oko);
        assert!(
            (przesunieta[0] - 0.001).abs() < 1.0e-6,
            "milimetr wyszedł jako {}",
            przesunieta[0]
        );

        // Kolejność odwrotna („rzutuj, potem odejmij") myli się o rząd milimetra
        // **już w punkcie zerowym** — i to jest ta siatka, która przesuwa się z kamerą.
        let zle = |p: [i32; 3]| {
            let w = glam::Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) * 0.001;
            w - oko.as_vec3()
        };
        assert!(
            zle(baza).x.abs() > 0.0005,
            "test nie mierzy tego, co miał mierzyć: błąd złej kolejności to {} m",
            zle(baza).x
        );
    }

    /// Mieszkaniec w pojeździe nie dostaje własnej bryły — inaczej szedłby obok auta.
    #[test]
    fn pasazer_nie_jest_rysowany_osobno() {
        let mut s = RenderSnapshot::new(SnapshotCaps::DEFAULT);
        s.citizens.push(CitizenRenderRec {
            pos: [0, 0, 0],
            flags: magnat_sim_snapshot::CITIZEN_FLAG_IN_VEHICLE,
            ..Default::default()
        });
        let mut out = InstanceScratch::default();
        build_instances(
            &s,
            DVec3::ZERO,
            &stozek_wszystko(),
            &tablica(),
            LodBands::default(),
            &mut out,
        );
        assert!(out.instances.is_empty());
    }

    #[test]
    fn instancja_ma_trzydziesci_szesc_bajtow() {
        assert_eq!(core::mem::size_of::<GpuInstance>(), 36);
    }

    /// Bufor ID musi odróżnić mieszkańca od pojazdu, bo mają osobne karty (`K-62`),
    /// i musi odróżnić „nic" od encji o indeksie zero.
    #[test]
    fn identyfikator_niesie_rodzaj_i_encje() {
        assert_eq!(decode_pick(0), None);
        for (kind, entity) in [
            (PickKind::Citizen, 0u32),
            (PickKind::Citizen, 274_000),
            (PickKind::Vehicle, 0),
            (PickKind::Vehicle, (1 << PICK_ENTITY_BITS) - 2),
        ] {
            let hit = decode_pick(pick_id(kind, entity)).expect("trafienie");
            assert_eq!((hit.kind, hit.entity), (kind, entity));
        }
        // Indeks spoza zakresu daje „nic", a nie cudzą encję.
        assert_eq!(pick_id(PickKind::Citizen, (1 << PICK_ENTITY_BITS) - 1), 0);
        assert_eq!(pick_id(PickKind::Citizen, u32::MAX), 0);
    }
}

#[cfg(test)]
mod testy_kamery {
    use super::*;
    use crate::camera::{CameraMode, CameraState};
    use magnat_sim_snapshot::SnapshotCaps;

    /// Encja stojąca w punkcie, na który patrzy kamera orbitalna, **musi** trafić
    /// do bufora instancji. Test istnieje, bo pierwsza próba obejrzenia animacji
    /// w oknie pokazała 44 mieszkańców w snapshocie i zero instancji — a stożek
    /// widzenia jest jedynym miejscem między jednym a drugim.
    #[test]
    fn pieszy_w_celu_kamery_trafia_do_bufora() {
        for dist in [12.0f32, 60.0, 300.0] {
            let cel = glam::DVec3::new(4009.0, 4599.0, 30.0);
            let camera = CameraState {
                mode: CameraMode::Orbit {
                    target: cel,
                    dist,
                    yaw: 0.7,
                    pitch: 0.7,
                },
                ..Default::default()
            };
            let mut s = RenderSnapshot::new(SnapshotCaps::DEFAULT);
            s.citizens.push(CitizenRenderRec {
                pos: [
                    (cel.x * 1000.0) as i32,
                    (cel.y * 1000.0) as i32,
                    (cel.z * 1000.0) as i32,
                ],
                entity_lo: 1,
                ..Default::default()
            });
            let planes = crate::renderer::frustum_planes(&camera.view_proj_relative(16.0 / 9.0));
            let mut out = InstanceScratch::default();
            let slot = MeshSlot {
                index_offset: 0,
                index_count: 36,
                base_vertex: 0,
                radius_m: 1.0,
            };
            let modele =
                ModelTable::new(vec![slot; 2 * LOD_COUNT as usize], ModelId(0), ModelId(1));
            build_instances(
                &s,
                camera.eye(),
                &planes,
                &modele,
                LodBands::default(),
                &mut out,
            );
            assert_eq!(
                out.instances.len(),
                1,
                "z {dist} m kamera nie widzi encji, na którą patrzy"
            );
        }
    }
}

#[cfg(test)]
mod testy_lod {
    use super::*;

    fn pasmo() -> Bands {
        Bands {
            l1_m: 40.0,
            l2_m: 120.0,
            draw_m: 600.0,
        }
    }

    /// Trzy pasma i jedna zasada awaryjna: model bez kafli **nie znika** za progiem
    /// impostora, tylko zostaje na siatce uproszczonej (`F-4`).
    #[test]
    fn progi_wybieraja_poziom_detalu() {
        let b = pasmo();
        assert_eq!(b.lod_for(0.0, true), 0);
        assert_eq!(b.lod_for(40.0, true), 0);
        assert_eq!(b.lod_for(40.1, true), 1);
        assert_eq!(b.lod_for(120.0, true), 1);
        assert_eq!(b.lod_for(120.1, true), LOD_IMPOSTOR);
        assert_eq!(b.lod_for(599.0, true), LOD_IMPOSTOR);
        assert_eq!(b.lod_for(599.0, false), 1, "model bez kafli zniknął");
    }

    /// Mieszkaniec schodzi na impostor wcześniej niż pojazd: auto jest cztery razy
    /// dłuższe, więc na tej samej odległości zajmuje kilka razy więcej pikseli.
    #[test]
    fn pojazd_ma_dalsze_progi_niz_mieszkaniec() {
        let d = LodBands::default();
        assert!(d.vehicle.l1_m > d.citizen.l1_m);
        assert!(d.vehicle.l2_m > d.citizen.l2_m);
    }

    /// Wsady rozdzielają się po poziomie detalu, więc rysowanie wie, który potok
    /// wziąć bez ani jednego porównania na encję.
    #[test]
    fn klucz_wsadu_niesie_poziom_detalu() {
        let k = model_lod(ModelId(7), LOD_IMPOSTOR);
        assert_eq!(lod_of(k), LOD_IMPOSTOR);
        assert_eq!(k >> 2, 7);
        assert_eq!(lod_of(model_lod(ModelId(7), 0)), 0);
    }
}
