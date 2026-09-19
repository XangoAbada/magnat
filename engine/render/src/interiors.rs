//! Cięcie budynków poziomami i wnętrza generowane proceduralnie (M11c §5.7, WP5).
//!
//! ### Co jest czyje
//!
//! Sam **clip** należy do M1 i już działa: `CameraState.clip_plane_z` jedzie w uniformie
//! klatki, a `voxel.wgsl` odrzuca fragmenty powyżej rzędnej. M11c dokłada dwie rzeczy,
//! których M1 nie miał czym zrobić: **wyliczenie rzędnej ze stropów** ([`CutPlane`])
//! i **domknięcie przekroju** ([`CapGeometry`]) — bez niego przecięta ściana jest pustą
//! skorupą i widać wnętrze geometrii, czyli dziurę zamiast przekroju architektonicznego.
//!
//! ### Dlaczego `generate_interior` nie bierze `BuildingGrammar`
//!
//! §5.7 zapisywało sygnaturę `generate_interior(&BuildingGrammar, &SiteRenderRec, seed)`.
//! Tak się nie da: `BuildingGrammar` mieszka w `sim/world`, a `engine/render` **nie ma
//! prawa** zależeć od żadnego `sim/*` poza `sim-snapshot` (§6.3 pkt 1), i pilnuje tego
//! `cargo tree` w CI. Wejściem jest więc [`InteriorSpec`] — płaski opis kondygnacji
//! i obrysu, który składa klient, bo to on widzi naraz miasto i renderer. Treść się
//! nie zmienia: te same kondygnacje, ten sam sposób użytkowania, ten sam obrys.

use magnat_sim_snapshot::SiteRenderRec;
use magnat_voxel::ModelId;

mod gpu;
pub use gpu::{CapRenderer, MAX_CAP_VERTICES};

/// Pół metra w metrach — pion voxela M1. Jednostka `clip_plane_z` i `world_y`.
///
/// **To jest najłatwiejszy błąd o czynnik dwa w tej fazie.** PRD §15.1 mówi o siatce 1 m
/// i to prawda w poziomie; §4.2 dokłada 0,5 m w pionie i to §4.2 jest wiążące. Kondygnacja
/// 3 m ma więc **sześć** voxeli, nie trzy.
pub const HALF_METER_M: f32 = 0.5;

/// Miękka granica cięcia w metrach. Twarda krawędź wygląda jak błąd renderu.
pub const CUT_FADE_M: f32 = 0.5;

/// Grubość muru w metrach — szerokość pierścienia domykającego przekrój.
///
/// Voxel M1 ma metr w poziomie, więc pół metra jest najmniejszą sensowną liczbą: cieńszy
/// pierścień znikałby między voxelami ściany i przekrój znów wyglądałby jak dziura.
pub const WALL_M: f32 = 0.5;

/// Co ciąć.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CutMode {
    #[default]
    Off,
    /// Zdejmij wszystko powyżej stropu kondygnacji `n` (0 = nad parterem).
    Level(u8),
    /// Zdejmij wszystko powyżej rzędnej `z` w metrach — tryb pierwszoosobowy używa go,
    /// żeby ściana za plecami nie zasłaniała.
    Above(f32),
}

/// Płaszczyzna przekroju.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct CutPlane {
    pub mode: CutMode,
    /// Rzędna cięcia w metrach — **wyprowadzana ze stropów, nie zgadywana**.
    pub world_y: f32,
    /// Szerokość miękkiej granicy w metrach.
    pub fade: f32,
}

impl CutPlane {
    #[must_use]
    pub const fn off() -> CutPlane {
        CutPlane {
            mode: CutMode::Off,
            world_y: 0.0,
            fade: CUT_FADE_M,
        }
    }

    /// Cięcie po **realnym stropie** budynku, nie po stałej wysokości.
    ///
    /// `base_z_m` to rzędna parteru, `floor_heights_dm` — wysokości kolejnych kondygnacji
    /// w decymetrach (M2 `Building.floor_heights_dm`). Parter usługowy jest wyższy niż
    /// piętro mieszkalne, więc cięcie co stałą wysokość przechodziłoby przez środek
    /// witryny każdej kamienicy.
    ///
    /// `Level(n)` większe niż liczba kondygnacji tnie nad dachem, czyli nie tnie nic —
    /// to jest normalny stan przy jednym progu dla całej dzielnicy, a nie błąd.
    #[must_use]
    pub fn level(base_z_m: f32, floor_heights_dm: &[u16], n: u8) -> CutPlane {
        let ile = usize::from(n).min(floor_heights_dm.len());
        let dm: u32 = floor_heights_dm[..ile].iter().map(|h| u32::from(*h)).sum();
        CutPlane {
            mode: CutMode::Level(n),
            world_y: base_z_m + dm as f32 * 0.1,
            fade: CUT_FADE_M,
        }
    }

    /// Cięcie na zadanej rzędnej — tryb pierwszoosobowy i `CutMode::Above`.
    #[must_use]
    pub const fn above(world_y: f32) -> CutPlane {
        CutPlane {
            mode: CutMode::Above(world_y),
            world_y,
            fade: CUT_FADE_M,
        }
    }

    /// Wartość dla `CameraState.clip_plane_z`, czyli **w jednostkach 0,5 m**.
    ///
    /// Konwersja jest w jednym miejscu i to jest cała ostrożność, jakiej wymaga sufit
    /// z nagłówka modułu: `floor_heights_dm` jest w decymetrach, `world_y` w metrach,
    /// a uniform w połówkach metra.
    #[must_use]
    pub fn clip_plane_z(&self) -> Option<i32> {
        match self.mode {
            CutMode::Off => None,
            _ => Some((self.world_y / HALF_METER_M).floor() as i32),
        }
    }
}

// ── Domknięcie przekroju ─────────────────────────────────────────────────────────────

/// Budynek do domknięcia: obrys w metrach i barwa materiału.
///
/// Klient składa to z `CityData`; `engine/render` nie widzi miasta i widzieć nie może.
#[derive(Clone, PartialEq, Debug)]
pub struct BuildingCut {
    /// Obrys w metrach, w kolejności obiegu. Pierwszy punkt **nie** powtarza się na końcu.
    pub footprint: Vec<[f32; 2]>,
    /// Rzędna parteru w metrach.
    pub base_z_m: f32,
    /// Wysokość całkowita w metrach — budynek niższy od cięcia nie dostaje czapki.
    pub height_m: f32,
    /// Barwa przekroju, RGBA8.
    pub color: [u8; 4],
}

/// Wierzchołek czapki przekroju. Płaski trójkąt na rzędnej cięcia, bez normalnych —
/// normalna jest z definicji pionowa.
#[derive(Clone, Copy, PartialEq, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct CapVertex {
    /// Pozycja **względem origin kamery**, w metrach (`R11`: odejmij w `f64`, potem rzutuj).
    pub pos: [f32; 3],
    pub color: u32,
}

/// Trójkąty domykające przekrój — jeden bufor na klatkę, jedno wywołanie rysowania.
#[derive(Clone, Debug, Default)]
pub struct CapGeometry {
    pub vertices: Vec<CapVertex>,
}

impl CapGeometry {
    /// Buduje czapki dla budynków przeciętych płaszczyzną.
    ///
    /// Czapka jest **pierścieniem ściany**, a nie płytą na całym obrysie, i to jest cała
    /// istota przekroju architektonicznego: płyta zakryłaby wnętrze, po które gracz
    /// w ogóle tnie budynek. Pierścień powstaje z obrysu i jego kopii odsuniętej do środka
    /// o grubość muru; trójkąty łączą obie pętle parami.
    ///
    /// Budynek, którego dach jest **poniżej** cięcia, nie jest przecięty i czapki nie
    /// dostaje — inaczej nad każdym parterowym domem wisiałby wieniec w powietrzu.
    pub fn build(&mut self, cuts: &[BuildingCut], plane: &CutPlane, eye: glam::DVec3) {
        self.vertices.clear();
        if plane.mode == CutMode::Off {
            return;
        }
        let y = f64::from(plane.world_y);
        for b in cuts {
            if b.footprint.len() < 3 {
                continue;
            }
            // Cięcie poniżej fundamentu zdejmuje budynek w całości — nie ma czego domykać.
            if plane.world_y <= b.base_z_m || plane.world_y >= b.base_z_m + b.height_m {
                continue;
            }
            let color = u32::from_le_bytes(b.color);
            let v = |p: [f32; 2]| CapVertex {
                pos: [
                    (f64::from(p[0]) - eye.x) as f32,
                    (f64::from(p[1]) - eye.y) as f32,
                    (y - eye.z) as f32,
                ],
                color,
            };
            // Środek obrysu — kierunek odsunięcia. Dla obrysów M2 (prostokąty
            // i wieloboki zbliżone do wypukłych) środek ciężkości wierzchołków wystarcza;
            // dla mocno wklęsłego pierścień mógłby się na wcięciu przeciąć sam ze sobą,
            // a to jest błąd niewidoczny z zewnątrz budynku.
            let n = b.footprint.len();
            let (mut cx, mut cy) = (0.0f32, 0.0f32);
            for p in &b.footprint {
                cx += p[0];
                cy += p[1];
            }
            let (cx, cy) = (cx / n as f32, cy / n as f32);
            let do_srodka = |p: [f32; 2]| -> [f32; 2] {
                let (dx, dy) = (cx - p[0], cy - p[1]);
                let d = (dx * dx + dy * dy).sqrt();
                if d <= WALL_M {
                    return [cx, cy];
                }
                [p[0] + dx / d * WALL_M, p[1] + dy / d * WALL_M]
            };
            for i in 0..n {
                let a = b.footprint[i];
                let c = b.footprint[(i + 1) % n];
                let (ai, ci) = (do_srodka(a), do_srodka(c));
                self.vertices.extend_from_slice(&[v(a), v(c), v(ci)]);
                self.vertices.extend_from_slice(&[v(a), v(ci), v(ai)]);
            }
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

// ── Wnętrza ──────────────────────────────────────────────────────────────────────────

/// Sposób użytkowania kondygnacji — steruje tym, co generator w niej postawi.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FloorUse {
    #[default]
    Retail,
    Office,
    Workshop,
    Storage,
    Dwelling,
}

/// Płaski opis budynku dla generatora wnętrz — **zastępuje `BuildingGrammar` z §5.7**
/// (powód w nagłówku modułu).
#[derive(Clone, PartialEq, Debug)]
pub struct InteriorSpec {
    /// Prostokąt opisany na obrysie, w metrach: `[min_x, min_y, max_x, max_y]`.
    pub bounds: [f32; 4],
    pub base_z_m: f32,
    /// Wysokości kondygnacji w decymetrach, od parteru.
    pub floor_heights_dm: Vec<u16>,
    /// Sposób użytkowania kolejnych kondygnacji; krótsza lista powtarza ostatni wpis.
    pub floor_use: Vec<FloorUse>,
    /// Udział klatek i korytarzy — z nich propów nie stawiamy.
    pub circulation_share: f32,
}

/// Jeden prop wnętrza.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PropPlacement {
    pub model: ModelId,
    /// Pozycja w metrach świata.
    pub pos: [f32; 3],
    pub yaw: u16,
    /// Wypełnienie 0..255 — regał z `fill = 0` stoi pusty.
    pub fill: u8,
    /// `entity_lo` zakładu, w którym prop stoi. Jedzie do bufora identyfikatorów, więc
    /// kliknięcie w regał otwiera kartę sklepu — a nie kartę regału, którego nie ma.
    pub site: u32,
}

/// Wyposażenie jednego budynku.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct InteriorKit {
    pub props: Vec<PropPlacement>,
}

/// Modele propów, których używa generator. Klient podstawia identyfikatory z `data/models/`;
/// model nieustawiony ([`MODEL_BRAK`]) znaczy „nie ma czym tego narysować" i prop
/// wtedy nie powstaje.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PropModels {
    pub shelf: ModelId,
    pub crate_: ModelId,
    pub desk: ModelId,
    pub machine: ModelId,
}

impl Default for PropModels {
    fn default() -> PropModels {
        PropModels {
            shelf: MODEL_BRAK,
            crate_: MODEL_BRAK,
            desk: MODEL_BRAK,
            machine: MODEL_BRAK,
        }
    }
}

/// Model, którego nie ma w bibliotece. Zero jest zajęte przez pierwszy model katalogu,
/// więc brak znaczy się maksimum — tak samo jak `NO_CLIP` w `engine/voxel`.
pub const MODEL_BRAK: ModelId = ModelId(u16::MAX);

/// Odstęp regałów w metrach. Jeden rząd co trzy metry czyta się jako sklep, a nie jako
/// magazyn — i tyle mieści się między półkami razem z przejściem.
const SIATKA_M: f32 = 3.0;
/// Sufit propów na budynek. Wnętrze niewidoczne nie jest generowane, ale widoczne
/// centrum handlowe ma cztery kondygnacje po kilkaset metrów i bez sufitu zjadłoby
/// cały budżet instancji.
const PROPY_MAX: usize = 512;

/// Generuje wyposażenie budynku (M11c §5.7).
///
/// Deterministyczne z `seed` — strumień `StreamId::Interior` (`K-76` pkt 1) nadaje
/// wołający, bo `engine/render` nie zna `StreamId` i znać go nie musi: dostaje gotowe
/// ziarno i miesza je indeksem propu.
///
/// `stock_fill` steruje **liczbą widocznych skrzynek**, a nie ich barwą: gracz ma
/// zobaczyć pustkę w magazynie, zanim otworzy panel.
#[must_use]
pub fn generate_interior(
    spec: &InteriorSpec,
    site: &SiteRenderRec,
    models: &PropModels,
    seed: u64,
) -> InteriorKit {
    let mut props = Vec::new();
    let [x0, y0, x1, y1] = spec.bounds;
    let uzytkowa = 1.0 - spec.circulation_share.clamp(0.0, 0.9);
    let mut z = spec.base_z_m;
    for (piętro, wys_dm) in spec.floor_heights_dm.iter().enumerate() {
        let uzycie = spec
            .floor_use
            .get(piętro)
            .or_else(|| spec.floor_use.last())
            .copied()
            .unwrap_or_default();
        if uzycie != FloorUse::Dwelling {
            rozstaw(
                &mut props, x0, y0, x1, y1, z, uzytkowa, uzycie, site, models, seed, piętro,
            );
        }
        z += f32::from(*wys_dm) * 0.1;
        if props.len() >= PROPY_MAX {
            break;
        }
    }
    props.truncate(PROPY_MAX);
    InteriorKit { props }
}

#[allow(clippy::too_many_arguments)]
fn rozstaw(
    out: &mut Vec<PropPlacement>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    z: f32,
    uzytkowa: f32,
    uzycie: FloorUse,
    site: &SiteRenderRec,
    models: &PropModels,
    seed: u64,
    pietro: usize,
) {
    let (model, wypelnienie) = match uzycie {
        FloorUse::Retail => (models.shelf, site.stock_fill),
        FloorUse::Storage => (models.crate_, site.stock_fill),
        FloorUse::Office => (models.desk, 255),
        FloorUse::Workshop => (models.machine, site.activity),
        FloorUse::Dwelling => return,
    };
    if model == MODEL_BRAK {
        return;
    }
    // Margines od ściany: prop postawiony w obrysie wchodziłby w mur, bo obrys jest
    // zewnętrzny, a mur ma grubość.
    let (mx, my) = (x0 + 1.0, y0 + 1.0);
    let (nx, ny) = (
        (((x1 - x0 - 2.0) / SIATKA_M).floor().max(0.0)) as usize,
        (((y1 - y0 - 2.0) / SIATKA_M).floor().max(0.0)) as usize,
    );
    // Powierzchnia użytkowa zjada część siatki — klatka schodowa i korytarz stoją
    // w środku kondygnacji, więc odejmujemy rzędy, a nie rozrzedzamy je losowo.
    let rzedow = ((ny as f32 * uzytkowa).round() as usize).min(ny);
    for j in 0..rzedow {
        for i in 0..nx {
            if out.len() >= PROPY_MAX {
                return;
            }
            // Ziarno propu: mieszanka ziarna wnętrza z pozycją w siatce. Jedna liczba
            // rozstrzyga dwie rzeczy — czy prop w ogóle stoi i jak jest obrócony.
            let mieszane = seed
                .wrapping_add((i + j * 61 + pietro * 977) as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .rotate_left(29)
                .wrapping_mul(0xBF58_476D_1CE4_E5B9);
            // Przy `stock_fill = 0` znika **prawie każdy** regał, a nie każdy: znika
            // ich tylu, ilu odpowiada zapasowi. Pusty magazyn ma wyglądać jak magazyn
            // z pustymi regałami, a nie jak magazyn bez regałów — to dwa różne stany.
            let prog = ((mieszane >> 40) & 0xFF) as u32;
            if u32::from(wypelnienie) < prog {
                continue;
            }
            // Cztery orientacje wystarczą, żeby rząd regałów nie wyglądał jak wydruk.
            let yaw = ((mieszane >> 32) & 0b11) as u16 * 16_384;
            out.push(PropPlacement {
                model,
                site: site.entity_lo,
                pos: [mx + i as f32 * SIATKA_M, my + j as f32 * SIATKA_M, z + 0.05],
                yaw,
                fill: wypelnienie,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> InteriorSpec {
        InteriorSpec {
            bounds: [0.0, 0.0, 20.0, 14.0],
            base_z_m: 10.0,
            // Parter usługowy 4,5 m, dwa piętra po 3,0 m.
            floor_heights_dm: vec![45, 30, 30],
            floor_use: vec![FloorUse::Retail, FloorUse::Office, FloorUse::Office],
            circulation_share: 0.2,
        }
    }

    fn zaklad(stock: u8) -> SiteRenderRec {
        SiteRenderRec {
            stock_fill: stock,
            activity: 200,
            ..Default::default()
        }
    }

    fn modele() -> PropModels {
        PropModels {
            shelf: ModelId(1),
            crate_: ModelId(2),
            desk: ModelId(3),
            machine: ModelId(4),
        }
    }

    /// `cut_plane_hits_slab` z §5.7: cięcie trafia **dokładnie w strop**, a nie 1,5 m obok.
    ///
    /// Budynek o kondygnacjach 4,5 / 3,0 / 3,0 m. `Level(1)` ma być na 4,5 m nad parterem,
    /// `Level(2)` na 7,5 — gdyby ktoś liczył po stałej wysokości, wyszłoby 3,0 i 6,0.
    #[test]
    fn cut_plane_hits_slab() {
        let s = spec();
        let p1 = CutPlane::level(s.base_z_m, &s.floor_heights_dm, 1);
        let p2 = CutPlane::level(s.base_z_m, &s.floor_heights_dm, 2);
        assert!((p1.world_y - 14.5).abs() < 1e-3, "{}", p1.world_y);
        assert!((p2.world_y - 17.5).abs() < 1e-3, "{}", p2.world_y);
        // Uniform jest w połówkach metra: 14,5 m to 29 jednostek, nie 14 i nie 145.
        assert_eq!(p1.clip_plane_z(), Some(29));
        assert_eq!(p2.clip_plane_z(), Some(35));
        assert_eq!(CutPlane::off().clip_plane_z(), None);
    }

    /// Poziom powyżej dachu nie jest błędem — po prostu nic nie tnie.
    #[test]
    fn ciecie_nad_dachem_nie_wychodzi_poza_budynek() {
        let s = spec();
        let p = CutPlane::level(s.base_z_m, &s.floor_heights_dm, 9);
        assert!((p.world_y - 20.5).abs() < 1e-3, "{}", p.world_y);
    }

    /// Czapkę dostaje **przecięty** budynek, a nie każdy widoczny.
    #[test]
    fn czapka_tylko_dla_przecietych_budynkow() {
        let niski = BuildingCut {
            footprint: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0]],
            base_z_m: 10.0,
            height_m: 3.0,
            color: [200, 180, 160, 255],
        };
        let wysoki = BuildingCut {
            height_m: 30.0,
            ..niski.clone()
        };
        let plane = CutPlane::above(20.0);
        let mut g = CapGeometry::default();
        g.build(std::slice::from_ref(&niski), &plane, glam::DVec3::ZERO);
        assert!(g.is_empty(), "parterowy dom dostał płytę w powietrzu");
        g.build(&[wysoki], &plane, glam::DVec3::ZERO);
        // Pierścień ściany: cztery boki po dwa trójkąty, czyli dwadzieścia cztery
        // wierzchołki. Płyta na całym obrysie zakryłaby wnętrze, po które gracz tnie.
        assert_eq!(g.vertices.len(), 24);
        assert!(g.vertices.iter().all(|v| (v.pos[2] - 20.0).abs() < 1e-3));
        // Wyłączone cięcie nie zostawia po sobie geometrii z poprzedniej klatki.
        g.build(std::slice::from_ref(&niski), &CutPlane::off(), glam::DVec3::ZERO);
        assert!(g.is_empty());
    }

    /// Kryterium WP5: **zmiana zapasu o połowę zmienia liczbę widocznych propów**.
    #[test]
    fn regaly_odzwierciedlaja_stan_magazynu() {
        let s = spec();
        let pelny = generate_interior(&s, &zaklad(255), &modele(), 7)
            .props
            .len();
        let polowa = generate_interior(&s, &zaklad(128), &modele(), 7)
            .props
            .len();
        let pusty = generate_interior(&s, &zaklad(0), &modele(), 7).props.len();
        assert!(pelny > polowa && polowa > pusty, "{pelny} {polowa} {pusty}");
        // Biura nie zależą od zapasu, więc puste nie znaczy zero propów w całym budynku.
        assert!(pusty > 0, "pusty magazyn zabrał biurka z pięter");
    }

    /// Ten sam budynek i to samo ziarno dają to samo wnętrze — inaczej regały
    /// przeskakiwałyby przy każdym odświeżeniu cache'u.
    #[test]
    fn wnetrze_jest_deterministyczne() {
        let s = spec();
        let a = generate_interior(&s, &zaklad(200), &modele(), 42);
        let b = generate_interior(&s, &zaklad(200), &modele(), 42);
        assert_eq!(a, b);
        let c = generate_interior(&s, &zaklad(200), &modele(), 43);
        assert_ne!(
            a.props.first().map(|p| p.yaw),
            c.props.first().map(|p| p.yaw)
        );
    }

    /// Mieszkania zostają prywatne: cięcie nad kamienicą nie odsłania cudzych regałów.
    #[test]
    fn kondygnacje_mieszkalne_nie_dostaja_propow() {
        let s = InteriorSpec {
            floor_use: vec![FloorUse::Dwelling],
            ..spec()
        };
        assert!(generate_interior(&s, &zaklad(255), &modele(), 1)
            .props
            .is_empty());
    }
}
