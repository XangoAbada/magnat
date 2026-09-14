//! WP7 — strefowanie z kwotami i pierścienie epok (M2 §5.3).
//!
//! Trzy kroki, wszystkie deterministyczne i wszystkie na **kwartałach**, nie na komórkach:
//! pierścienie epok (rozrost footprintu), punktacja stref z pól wpływu, przydział kwotowy
//! z wygładzaniem. Strefa przypisana komórce dawałaby szum i kwartał z czterema strefami —
//! plan mówi o tym wprost.
//!
//! Pola wpływu (`CityFields`) liczone są raz, na siatce 16 m, i żyją dalej: M2e wycenia
//! z nich grunt, a `engine/render` rysuje z nich nakładki.

use super::blocks::BlockSet;
use super::gates::GateKind;
use super::poly;
use super::road::{RoadClass, RoadNetwork};
use super::CityPlan;
use crate::assets::data_path;
use crate::params::Epoch;
use crate::query::TerrainQuery;
use magnat_core::det_math;
use magnat_spatial::{Aabb2, CellId, GridSpec, ScalarField, Vec2};
use serde::{Deserialize, Serialize};

/// Bok komórki pól wpływu w metrach (M2 §5.3: „wszystkie `ScalarField`, siatka 16 m").
pub const FIELD_CELL_M: u16 = 16;

/// Powyżej tej powierzchni ściana grafu dróg **poza obszarem zurbanizowanym** nie jest
/// kwartałem, tylko kawałkiem terenu między drogami wylotowymi (korekta C6).
///
/// Próg to największe `max_block_area` z tabeli §5.4 (30 ha, `IndustryHeavy`). Takie
/// ściany są w generacji normalne — dwie autostrady i rzeka zamykają setki hektarów
/// pola — ale do przydziału kwotowego nie mogą wejść: pojedyncza ściana o 340 ha
/// to 84 % powierzchni miasta czterdziestotysięcznego, więc żadna kwota nie ma prawa
/// się zgodzić i test T9 mierzyłby wtedy geometrię, nie urbanistykę. Takie kwartały
/// dostają strefę otwartą wprost z punktacji terenu, poza kwotami i poza udziałami.
pub const MAX_URBAN_BLOCK_M2: f64 = 300_000.0;

/// Strefy otwarte — jedyne, jakie może dostać kwartał pozamiejski.
const RURAL_ZONES: [ZoneKind; 3] = [ZoneKind::Agriculture, ZoneKind::Green, ZoneKind::Extraction];

// ── Klasy stref ──────────────────────────────────────────────────────────────────────

/// Gęstość zabudowy mieszkaniowej (PRD §4.2).
/// R1 domy wolnostojące · R2 szeregowa/bliźniacza · R3 kamienice/niska zwarta
/// R4 bloki 4–11 kondygnacji · R5 wieżowce mieszkalne
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum ResDensity {
    R1,
    R2,
    R3,
    R4,
    R5,
}

impl ResDensity {
    pub const ALL: [ResDensity; 5] = [
        ResDensity::R1,
        ResDensity::R2,
        ResDensity::R3,
        ResDensity::R4,
        ResDensity::R5,
    ];
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum ZoneKind {
    Residential(ResDensity),
    Commercial,
    Office,
    IndustryLight,
    IndustryHeavy,
    Logistics,
    Agriculture,
    Institutional,
    Green,
    Extraction,
    Water,
    Undevelopable,
}

impl ZoneKind {
    /// Wszystkie strefy w porządku indeksu. Kolejność jest kluczem do tablic punktacji
    /// i kwot — wyliczana raz, nigdy po `HashMap` (00 §3.2).
    pub const ALL: [ZoneKind; 16] = [
        ZoneKind::Residential(ResDensity::R1),
        ZoneKind::Residential(ResDensity::R2),
        ZoneKind::Residential(ResDensity::R3),
        ZoneKind::Residential(ResDensity::R4),
        ZoneKind::Residential(ResDensity::R5),
        ZoneKind::Commercial,
        ZoneKind::Office,
        ZoneKind::IndustryLight,
        ZoneKind::IndustryHeavy,
        ZoneKind::Logistics,
        ZoneKind::Agriculture,
        ZoneKind::Institutional,
        ZoneKind::Green,
        ZoneKind::Extraction,
        ZoneKind::Water,
        ZoneKind::Undevelopable,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            ZoneKind::Residential(d) => d as usize,
            ZoneKind::Commercial => 5,
            ZoneKind::Office => 6,
            ZoneKind::IndustryLight => 7,
            ZoneKind::IndustryHeavy => 8,
            ZoneKind::Logistics => 9,
            ZoneKind::Agriculture => 10,
            ZoneKind::Institutional => 11,
            ZoneKind::Green => 12,
            ZoneKind::Extraction => 13,
            ZoneKind::Water => 14,
            ZoneKind::Undevelopable => 15,
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ZoneKind::Residential(ResDensity::R1) => "r1",
            ZoneKind::Residential(ResDensity::R2) => "r2",
            ZoneKind::Residential(ResDensity::R3) => "r3",
            ZoneKind::Residential(ResDensity::R4) => "r4",
            ZoneKind::Residential(ResDensity::R5) => "r5",
            ZoneKind::Commercial => "commercial",
            ZoneKind::Office => "office",
            ZoneKind::IndustryLight => "industry_light",
            ZoneKind::IndustryHeavy => "industry_heavy",
            ZoneKind::Logistics => "logistics",
            ZoneKind::Agriculture => "agriculture",
            ZoneKind::Institutional => "institutional",
            ZoneKind::Green => "green",
            ZoneKind::Extraction => "extraction",
            ZoneKind::Water => "water",
            ZoneKind::Undevelopable => "undevelopable",
        }
    }

    /// Czy strefa jest przydzielana z kwoty. `Water` i `Undevelopable` wynikają z terenu
    /// i kwoty nie zużywają — inaczej jezioro zjadałoby limit zabudowy mieszkaniowej.
    #[must_use]
    pub const fn from_quota(self) -> bool {
        !matches!(self, ZoneKind::Water | ZoneKind::Undevelopable)
    }

    #[must_use]
    pub const fn is_residential(self) -> bool {
        matches!(self, ZoneKind::Residential(_))
    }

    /// Czy strefa w ogóle dzieli się na parcele (WP8).
    #[must_use]
    pub const fn parcelled(self) -> bool {
        !matches!(self, ZoneKind::Water | ZoneKind::Undevelopable)
    }
}

// ── Pola wpływu ──────────────────────────────────────────────────────────────────────

/// Pole wejściowe punktacji stref. Wartości są znormalizowane 0..=1; dla pól odległości
/// 1 znaczy „najdalej", więc waga „chcę blisko" jest ujemna.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum ZoneField {
    DCenter,
    AccessRoad,
    GateRoad,
    GateRail,
    Port,
    Noise,
    Amenity,
    Slope,
    Flood,
    Deposit,
    Soil,
    /// Wiek pierścienia zabudowy: 0 = najstarszy, 1 = epoka startowa.
    EpochAge,
    /// Położenie z podwietrznej względem środka miasta: 1 = pełna zgodność z wiatrem.
    Downwind,
}

/// Pola wpływu generatora miasta (M2 §5.3). Powstają raz i żyją do końca generacji;
/// M2e wycenia z nich grunt, `engine/render` rysuje z nich nakładki.
#[derive(Clone, Debug)]
pub struct CityFields {
    pub spec: GridSpec,
    /// Odległość od centrum **po sieci dróg** — rzeka bez mostu jest barierą.
    pub d_center: ScalarField,
    pub access_road: ScalarField,
    pub d_gate_road: ScalarField,
    pub d_gate_rail: ScalarField,
    pub d_port: ScalarField,
    pub noise: ScalarField,
    pub amenity: ScalarField,
    pub slope: ScalarField,
    /// Ranga najwyższej klasy drogi w komórce; 0 = brak drogi. Używane też przez WP5b.
    pub road_rank: Vec<u8>,
    /// Komórka nieprzejezdna dla pola odległości (woda bez drogi).
    pub blocked: Vec<bool>,
}

/// Mnożnik kosztu ruchu po drodze danej rangi — „wjazd z Highway ≠ wjazd z Service".
const fn road_speedup(rank: u8) -> u32 {
    match rank {
        6 => 8, // Highway
        5 => 6, // Arterial
        4 => 4, // Collector
        3 => 3, // Local
        2 => 2, // Service
        _ => 1,
    }
}

impl CityFields {
    /// Wartość pola w punkcie, 0..=1. Pola sięgające terenu (`Flood`, `Deposit`, `Soil`)
    /// nie są tu trzymane — patrz [`BlockSamples`].
    #[must_use]
    pub fn sample(&self, f: ZoneField, p: Vec2) -> f32 {
        match f {
            ZoneField::DCenter => self.d_center.sample(p),
            ZoneField::AccessRoad => self.access_road.sample(p),
            ZoneField::GateRoad => self.d_gate_road.sample(p),
            ZoneField::GateRail => self.d_gate_rail.sample(p),
            ZoneField::Port => self.d_port.sample(p),
            ZoneField::Noise => self.noise.sample(p),
            ZoneField::Amenity => self.amenity.sample(p),
            ZoneField::Slope => self.slope.sample(p),
            _ => 0.0,
        }
    }
}

/// Pola wpływu dla całego miasta. Siedem przebiegów Dijkstry po siatce 16 m — przy
/// metropolii 1 mln komórek to ~35 ms każdy (zmierzone w M2a).
#[must_use]
pub fn build_fields(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
    net: &RoadNetwork,
    gp: &super::gates::GatePlan,
    center: Vec2,
) -> CityFields {
    let map = plan.map_size_m() as f32;
    let spec = GridSpec::covering(Aabb2::new(Vec2::ZERO, Vec2::new(map, map)), FIELD_CELL_M);
    let n = spec.cell_count();
    let cell_m = f32::from(FIELD_CELL_M);

    // Rasteryzacja sieci: ranga klasy i tory. Krok pół komórki, żeby nie zostawić dziur.
    let mut road_rank = vec![0u8; n];
    let mut rail = vec![false; n];
    // Po węzłach, nie po arenie geometrii: arena bywa w tym miejscu wyjęta z sieci
    // (jedno pożyczenie na zapis obrysów), a segment i tak jest odcinkiem prostym.
    for s in &net.segments {
        let (a, b) = (net.nodes[s.a.0 as usize].pos, net.nodes[s.b.0 as usize].pos);
        let steps = ((b - a).length() / (cell_m * 0.5)).ceil().max(1.0) as u32;
        for k in 0..=steps {
            let p = a + (b - a) * (k as f32 / steps as f32);
            let c = spec.cell_of(p).0 as usize;
            if s.class.is_rail() {
                rail[c] = true;
            } else {
                road_rank[c] = road_rank[c].max(s.class.rank());
            }
        }
    }

    // Woda bez drogi jest barierą; most rasteryzuje się jako droga, więc przepuszcza.
    let mut blocked = vec![false; n];
    for c in 0..n {
        let p = spec.cell_center(CellId(c as u32));
        blocked[c] = road_rank[c] == 0 && t.water_depth_at(p.x as i32, p.y as i32) > 0;
    }

    let koszt = |a: CellId, b: CellId| -> u32 {
        let (bi, ai) = (b.0 as usize, a.0 as usize);
        if blocked[bi] {
            return u32::MAX;
        }
        let (ax, ay) = spec.cell_of_id(a);
        let (bx, by) = spec.cell_of_id(b);
        let m = if ax != bx && ay != by { 23 } else { 16 };
        (m * 4 / road_speedup(road_rank[ai])).max(1)
    };

    let dijkstra = |src: Vec<CellId>| -> ScalarField {
        if src.is_empty() {
            // Brak źródła (np. miasto bez portu): pole „nieskończenie daleko".
            return ScalarField::from_data(spec, vec![u16::MAX; n], 1.0);
        }
        ScalarField::multi_source_dijkstra(spec, &src, koszt)
    };

    // Bramy bierzemy z **planu**, nie z gotowej sieci: kolejowe nie mają jeszcze węzła
    // (tory powstają w WP5b, czyli po strefowaniu), a to one wyznaczają `d_gate_rail`.
    let gate_cells = |pred: fn(GateKind) -> bool| -> Vec<CellId> {
        gp.gates
            .iter()
            .filter(|g| pred(g.kind))
            .map(|g| spec.cell_of(g.node_pos))
            .collect()
    };

    let d_center = dijkstra(vec![spec.cell_of(center)]);
    let access_road = dijkstra(
        (0..n)
            .filter(|&c| road_rank[c] > 0)
            .map(|c| CellId(c as u32))
            .collect(),
    );
    let d_gate_road = dijkstra(gate_cells(|k| matches!(k, GateKind::Highway)));
    let d_gate_rail = dijkstra(gate_cells(GateKind::is_rail));
    let d_port = dijkstra(gate_cells(|k| matches!(k, GateKind::Port)));

    // Hałas i atrakcyjność: **zanik z odległości**, nie suma `decay_from` po źródłach.
    // Korekta C1: źródła są tu obszarowe (dziesiątki tysięcy komórek drogi i wody),
    // a `decay_from` kosztuje zasięg² na źródło — dla metropolii to miliardy operacji.
    let noise = zanik(
        spec,
        (0..n)
            .filter(|&c| road_rank[c] >= RoadClass::Arterial.rank() || rail[c])
            .map(|c| CellId(c as u32))
            .collect(),
        120.0,
    );
    let zielen: Vec<CellId> = (0..n)
        .filter(|&c| {
            let p = spec.cell_center(CellId(c as u32));
            t.water_depth_at(p.x as i32, p.y as i32) > 0
                || t.biome_at(p.x as i32, p.y as i32).is_forest()
        })
        .map(|c| CellId(c as u32))
        .collect();
    let amenity = zanik(spec, zielen, 300.0);

    let mut slope = vec![0u16; n];
    for (c, v) in slope.iter_mut().enumerate() {
        let p = spec.cell_center(CellId(c as u32));
        *v = u16::from(t.slope_at(p.x as i32, p.y as i32)) * 257;
    }

    CityFields {
        spec,
        d_center,
        access_road,
        d_gate_road,
        d_gate_rail,
        d_port,
        noise,
        amenity,
        slope: ScalarField::from_data(spec, slope, 1.0),
        road_rank,
        blocked,
    }
}

/// Pole zaniku wykładniczego od obszaru źródłowego: `2^(−d / half_life)`.
fn zanik(spec: GridSpec, src: Vec<CellId>, half_life_m: f32) -> ScalarField {
    if src.is_empty() {
        return ScalarField::from_data(spec, vec![0; spec.cell_count()], 1.0);
    }
    let d = ScalarField::multi_source_dijkstra(spec, &src, |a, b| {
        let (ax, ay) = spec.cell_of_id(a);
        let (bx, by) = spec.cell_of_id(b);
        if ax != bx && ay != by {
            23
        } else {
            16
        }
    });
    let skala = f64::from(d.full_scale());
    let data = d
        .data()
        .iter()
        .map(|&v| {
            let m = f64::from(v) / 65535.0 * skala;
            // det_math zamiast `f64::exp2` — 00 §K-6.
            (det_math::exp2(-m / f64::from(half_life_m)) * 65535.0) as u16
        })
        .collect();
    ScalarField::from_data(spec, data, 1.0)
}

// ── Dane: epoki ──────────────────────────────────────────────────────────────────────

/// Numer pierścienia epoki: 0 = najstarszy, `rings.len() − 1` = epoka startowa gry.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct EpochId(pub u8);

/// Klucz do palety materiałów i zestawu gramatyk (konsument: M2d, M11).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct StyleId(pub u16);

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct EpochSpec {
    pub key: String,
    /// Rok, do którego trwa epoka. Pierścienie nowsze od epoki startowej odpadają.
    pub until_year: u16,
    /// Udział w powierzchni obszaru zurbanizowanego.
    pub area_share: f32,
    /// Przesunięcie punktacji gęstości mieszkaniowej R1..R5 w tym pierścieniu.
    pub density_bias: [f32; 5],
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct EpochTable {
    pub schema_version: u32,
    pub epochs: Vec<EpochSpec>,
}

pub const EPOCH_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum EpochError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    Schema { found: u32 },
    Empty,
}

impl std::fmt::Display for EpochError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EpochError::Io(e) => write!(f, "data/epochs/epochs.ron: {e}"),
            EpochError::Ron(e) => write!(f, "data/epochs/epochs.ron: {e}"),
            EpochError::Schema { found } => write!(
                f,
                "data/epochs/epochs.ron: schema_version {found}, oczekiwano {EPOCH_SCHEMA_VERSION}"
            ),
            EpochError::Empty => write!(f, "data/epochs/epochs.ron: pusta lista epok"),
        }
    }
}

impl std::error::Error for EpochError {}

impl EpochTable {
    pub fn load() -> Result<EpochTable, EpochError> {
        let txt =
            std::fs::read_to_string(data_path("epochs/epochs.ron")).map_err(EpochError::Io)?;
        let t: EpochTable = ron::from_str(&txt).map_err(EpochError::Ron)?;
        if t.schema_version != EPOCH_SCHEMA_VERSION {
            return Err(EpochError::Schema {
                found: t.schema_version,
            });
        }
        if t.epochs.is_empty() {
            return Err(EpochError::Empty);
        }
        Ok(t)
    }

    /// Pierścienie widoczne w mieście startującym w danej epoce: wszystkie zamknięte
    /// przed jej rokiem plus ta, w której gra się zaczyna (zabudowana częściowo).
    /// Udziały renormalizowane do 1 (decyzja 9.2/4: starówka w mieście z 1990 jest
    /// historią zabudowy, a nie stanem techniki dostępnej graczowi).
    #[must_use]
    pub fn rings_for(&self, epoch: Epoch) -> Vec<EpochSpec> {
        let rok = epoch.year();
        let mut out: Vec<EpochSpec> = Vec::new();
        for e in &self.epochs {
            out.push(e.clone());
            if e.until_year >= rok {
                break;
            }
        }
        let suma: f32 = out.iter().map(|e| e.area_share).sum();
        if suma > 0.0 {
            for e in &mut out {
                e.area_share /= suma;
            }
        }
        out
    }
}

// ── Dane: wagi i kwoty ───────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ZoneRule {
    pub zone: ZoneKind,
    /// Maksymalne nachylenie w jednostkach `slope_at` (tan α × 64).
    pub max_slope: u8,
    pub weights: Vec<(ZoneField, f32)>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ZoningWeights {
    pub schema_version: u32,
    /// Ryzyko powodziowe (0..=100), powyżej którego kwartał jest `Undevelopable`.
    pub flood_max: u8,
    /// Bufor między `IndustryHeavy` a zabudową mieszkaniową, w metrach.
    pub heavy_buffer_m: f32,
    pub zones: Vec<ZoneRule>,
}

pub const WEIGHTS_SCHEMA_VERSION: u32 = 1;

/// Kwoty stref profilu gospodarczego — sekcja `mix` w `data/zoning/profile_*.ron`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ZoneMix {
    /// Udziały stref niemieszkaniowych.
    pub shares: Vec<(ZoneKind, f32)>,
    /// Łączny udział zabudowy mieszkaniowej.
    pub residential: f32,
    /// Rozkład gęstości R1..R5 w epoce startowej.
    pub density: Vec<(Epoch, [f32; 5])>,
}

impl ZoneMix {
    /// Docelowe udziały wszystkich 14 stref kwotowych, sumujące się do 1.
    #[must_use]
    pub fn targets(&self, epoch: Epoch) -> [f32; 16] {
        let mut out = [0.0f32; 16];
        for (z, s) in &self.shares {
            out[z.index()] += *s;
        }
        let gest = self
            .density
            .iter()
            .find(|(e, _)| *e == epoch)
            .map_or([0.2f32; 5], |(_, d)| *d);
        let suma_g: f32 = gest.iter().sum();
        for (i, g) in gest.iter().enumerate() {
            out[i] = self.residential * g / suma_g.max(1e-6);
        }
        let suma: f32 = out.iter().sum();
        if suma > 0.0 {
            for v in &mut out {
                *v /= suma;
            }
        }
        out
    }
}

impl ZoningWeights {
    pub fn load() -> Result<ZoningWeights, EpochError> {
        let txt =
            std::fs::read_to_string(data_path("zoning/weights.ron")).map_err(EpochError::Io)?;
        let w: ZoningWeights = ron::from_str(&txt).map_err(EpochError::Ron)?;
        if w.schema_version != WEIGHTS_SCHEMA_VERSION {
            return Err(EpochError::Schema {
                found: w.schema_version,
            });
        }
        if w.zones.is_empty() {
            return Err(EpochError::Empty);
        }
        Ok(w)
    }

    fn rule(&self, z: ZoneKind) -> Option<&ZoneRule> {
        self.zones.iter().find(|r| r.zone == z)
    }
}

// ── Próbki terenu per kwartał ────────────────────────────────────────────────────────

/// Wartości terenowe policzone **raz na kwartał**, a nie na komórkę siatki.
///
/// Korekta C2: plan wymienia `flood_risk`, `deposit` i `soil_quality` jako `ScalarField`,
/// ale konsumentem punktacji jest kwartał (6 tys. sztuk), a nie komórka (1 mln). Trzy pola
/// po milionie wywołań `buildability_at` kosztowałyby więcej niż cała reszta etapu.
#[derive(Clone, Debug, Default)]
pub struct BlockSamples {
    pub flood: Vec<f32>,
    pub deposit: Vec<f32>,
    pub soil: Vec<f32>,
    pub downwind: Vec<f32>,
    pub centroid: Vec<Vec2>,
    pub water: Vec<bool>,
}

/// Pięć punktów na kwartał: centroid i cztery w połowie drogi do wierzchołków skrajnych.
fn probe_points(pts: &[Vec2]) -> [Vec2; 5] {
    let c = poly::centroid(pts);
    let mut out = [c; 5];
    for (i, s) in out.iter_mut().skip(1).enumerate() {
        let v = pts[(i * pts.len() / 4).min(pts.len() - 1)];
        *s = c + (v - c) * 0.5;
    }
    out
}

#[must_use]
pub fn sample_blocks(
    blocks: &BlockSet,
    geom: &super::road::PolyArena,
    t: &dyn TerrainQuery,
    center: Vec2,
) -> BlockSamples {
    let mut s = BlockSamples::default();
    // Wiatr podaje się jako kierunek, **z którego** wieje (M1 `prevailing_wind_deg`),
    // więc podwietrzna strona miasta leży w kierunku `deg + 180`.
    let wind_from = f64::from(t.prevailing_wind(center.x as i32, center.y as i32));
    let bearing = (wind_from + 180.0) % 360.0;
    let r = bearing * std::f64::consts::PI / 180.0;
    // Namiar kompasowy: 0° = północ (+Y), 90° = wschód (+X).
    let wind_to = Vec2::new(det_math::sin(r) as f32, det_math::cos(r) as f32);

    for b in &blocks.blocks {
        let pts = geom.get(b.poly);
        let probes = probe_points(pts);
        let mut flood = 0.0f32;
        let mut soil = 0.0f32;
        let mut deposit = 0.0f32;
        let mut woda = 0u8;
        for p in probes {
            let (x, y) = (p.x as i32, p.y as i32);
            flood += f32::from(t.flood_risk_at(x, y).get()) / 100.0;
            soil += f32::from(t.soil_quality_at(x, y).get()) / 100.0;
            if let Some(id) = t.deposit_at(x, y) {
                let d = t.deposit(id);
                deposit += f32::from(d.concentration.get()) / 100.0;
            }
            if t.water_depth_at(x, y) > 0 {
                woda += 1;
            }
        }
        let n = probes.len() as f32;
        let c = poly::centroid(pts);
        s.flood.push(flood / n);
        s.soil.push(soil / n);
        s.deposit.push(deposit / n);
        s.downwind
            .push(((c - center).normalize_or_zero().dot(wind_to) + 1.0) * 0.5);
        s.centroid.push(c);
        s.water.push(woda >= 3);
    }
    s
}

// ── Przydział stref ──────────────────────────────────────────────────────────────────

/// Wynik WP7 — po jednym wpisie na kwartał, w kolejności `BlockId`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ZoneResult {
    pub zone: Vec<ZoneKind>,
    pub epoch_ring: Vec<u8>,
    pub rings: Vec<EpochSpec>,
    /// Zrealizowany udział powierzchniowy każdej strefy — wejście testu T9.
    pub share: [f32; 16],
    pub target: [f32; 16],
    /// Liczba kwartałów w każdej strefie, **łącznie** z zawetowanymi. `share` liczy
    /// powierzchnię i tylko kwartały kwotowe, więc same udziały nie powiedzą, że
    /// generator zawetował pół miasta.
    pub count: [u32; 16],
    /// Ile kwartałów w ogóle **mogło** dostać daną strefę (punktacja skończona, weta
    /// przeszły). Strefa z zerem kandydatów to błąd kalibracji wag, nie generacji,
    /// i bez tej liczby wygląda identycznie jak strefa, której zabrakło kwoty.
    pub candidates: [u32; 16],
    /// Udział największego kwartału kwotowego w powierzchni przydzielanej.
    ///
    /// To **dolna granica** odchylenia od kwot: strefa dostaje kwartały w całości,
    /// więc jeden kwartał wart 10 % miasta gwarantuje 10-punktowy błąd w jedną albo
    /// w drugą stronę i żaden przydział tego nie naprawi. Wielkość rośnie, im mniejsze
    /// miasto (metropolia: 0,6 %, miasto 40-tysięczne: 10,4 %) — patrz korekta C13.
    pub largest_block_share: f32,
    /// Kwartały pozamiejskie (> [`MAX_URBAN_BLOCK_M2`]) — poza kwotami i poza `share`.
    pub rural_blocks: u32,
    pub rural_area_m2: f64,
    pub warnings: Vec<String>,
}

impl ZoneResult {
    /// Największe odchylenie udziału od kwoty, w punktach procentowych.
    #[must_use]
    pub fn max_deviation_pp(&self) -> f32 {
        (0..16)
            .map(|i| ((self.share[i] - self.target[i]) * 100.0).abs())
            .fold(0.0, f32::max)
    }
}

/// Pierścienie epok: rozrost footprintu po grafie sąsiedztwa kwartałów.
///
/// Korekta C3: plan rozszerza footprint po **komórkach** siatki 16 m, ale jedynym
/// konsumentem `epoch_ring` jest kwartał, a rasteryzacja 6 tys. wielokątów na milion
/// komórek kosztuje więcej niż cały etap. Rozrost po kwartałach daje to samo
/// uporządkowanie („miasto rosło od środka ku najlepszemu terenowi") przy O(B log B).
fn grow_epoch_rings(
    blocks: &BlockSet,
    adj: &(Vec<u32>, Vec<u32>),
    f: &CityFields,
    s: &BlockSamples,
    rings: &[EpochSpec],
    center: Vec2,
) -> Vec<u8> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let n = blocks.blocks.len();
    let mut out = vec![rings.len().saturating_sub(1) as u8; n];
    if n == 0 || rings.is_empty() {
        return out;
    }
    let total: f64 = blocks.blocks.iter().map(|b| f64::from(b.area_m2)).sum();

    // Priorytet: dostępność drogowa + atrakcyjność − nachylenie (M2 §5.3).
    // Kwantyzowany do u32, żeby porządek kolejki był totalny i niezależny od NaN.
    let prio = |i: usize| -> u32 {
        let c = s.centroid[i];
        let v = (1.0 - f.access_road.sample(c)) + f.amenity.sample(c) - f.slope.sample(c) * 2.0;
        ((v.clamp(-2.0, 2.0) + 2.0) * 1e6) as u32
    };

    let start = (0..n)
        .min_by_key(|&i| ((s.centroid[i] - center).length() * 1000.0) as u64)
        .unwrap_or(0);
    let mut seen = vec![false; n];
    let mut heap: BinaryHeap<(u32, Reverse<u32>)> = BinaryHeap::new();
    seen[start] = true;
    heap.push((prio(start), Reverse(start as u32)));

    let (adj_start, adj_items) = adj;
    let mut ring = 0usize;
    let mut acc = 0.0f64;
    let mut limit = total * f64::from(rings[0].area_share);
    while let Some((_, Reverse(i))) = heap.pop() {
        let i = i as usize;
        while acc > limit && ring + 1 < rings.len() {
            ring += 1;
            limit += total * f64::from(rings[ring].area_share);
        }
        out[i] = ring as u8;
        acc += f64::from(blocks.blocks[i].area_m2);
        for k in adj_start[i]..adj_start[i + 1] {
            let j = adj_items[k as usize] as usize;
            if !seen[j] {
                seen[j] = true;
                heap.push((prio(j), Reverse(j as u32)));
            }
        }
    }
    // Kwartały odcięte od centrum (wyspa za rzeką bez mostu) należą do ostatniego
    // pierścienia — powstały najpóźniej, bo najpóźniej dało się tam dojechać.
    for (i, v) in out.iter_mut().enumerate() {
        if !seen[i] {
            *v = rings.len() as u8 - 1;
        }
    }
    out
}

/// WP7 w całości: pierścienie epok, punktacja, przydział kwotowy, wygładzanie, bufor
/// przemysłu ciężkiego.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn assign_zones(
    plan: &CityPlan,
    blocks: &BlockSet,
    adj: &(Vec<u32>, Vec<u32>),
    f: &CityFields,
    s: &BlockSamples,
    w: &ZoningWeights,
    mix: &ZoneMix,
    rings: Vec<EpochSpec>,
    center: Vec2,
) -> ZoneResult {
    let n = blocks.blocks.len();
    let epoch_ring = grow_epoch_rings(blocks, adj, f, s, &rings, center);
    let n_rings = rings.len().max(1) as f32;
    let mut zone = vec![ZoneKind::Undevelopable; n];
    // Weto terenowe trzymamy osobno, a nie po `zone[i].from_quota()`: strefa startowa
    // też jest niekwotowa, więc test po niej uznałby **każdy** kwartał za zawetowany.
    let mut veto = vec![false; n];
    let mut rural = vec![false; n];
    let mut warnings = Vec::new();

    // 1. Punktacja i twarde weta.
    let mut score = vec![[f32::NEG_INFINITY; 16]; n];
    for i in 0..n {
        let c = s.centroid[i];
        let slope_u = (f.slope.sample(c) * 255.0) as u8;
        if s.water[i] {
            zone[i] = ZoneKind::Water;
            veto[i] = true;
            continue;
        }
        if s.flood[i] * 100.0 > f32::from(w.flood_max) {
            zone[i] = ZoneKind::Undevelopable;
            veto[i] = true;
            continue;
        }
        // Wielkość **i** położenie: ściana 30 ha wciśnięta między arterie w środku
        // miasta jest kwartałem, tylko nienaturalnie dużym — dzieli ją siecią lokalną
        // WP8. Pozamiejska jest dopiero taka, która leży poza promieniem obszaru
        // zurbanizowanego. Bez drugiego warunku środek miasta wypełniały parki
        // wielkości dzielnicy (zmierzone: 23 % powierzchni w 16 ścianach).
        rural[i] = f64::from(blocks.blocks[i].area_m2) > MAX_URBAN_BLOCK_M2
            && (s.centroid[i] - center).length() > plan.urban_radius_m();
        let wiek = f32::from(epoch_ring[i]) / (n_rings - 1.0).max(1.0);
        for z in ZoneKind::ALL {
            if !z.from_quota() || (rural[i] && !RURAL_ZONES.contains(&z)) {
                continue;
            }
            let Some(rule) = w.rule(z) else { continue };
            if slope_u > rule.max_slope {
                continue;
            }
            if z == ZoneKind::Extraction && s.deposit[i] <= 0.01 {
                continue;
            }
            let mut v = 0.0f32;
            for (field, waga) in &rule.weights {
                let x = match field {
                    ZoneField::Flood => s.flood[i],
                    ZoneField::Deposit => s.deposit[i],
                    ZoneField::Soil => s.soil[i],
                    ZoneField::EpochAge => wiek,
                    ZoneField::Downwind => s.downwind[i],
                    other => f.sample(*other, c),
                };
                v += waga * x;
            }
            if z.is_residential() {
                let d = match z {
                    ZoneKind::Residential(d) => d as usize,
                    _ => 0,
                };
                v += rings[usize::from(epoch_ring[i])].density_bias[d];
            }
            score[i][z.index()] = v;
        }
    }

    // 2. Kwartały pozamiejskie: strefa wprost z punktacji, bez kwoty.
    let mut rural_blocks = 0u32;
    let mut rural_area = 0.0f64;
    for i in 0..n {
        if veto[i] || !rural[i] {
            continue;
        }
        let najlepsza = (0..16)
            .filter(|&z| score[i][z].is_finite())
            .max_by(|a, b| score[i][*a].total_cmp(&score[i][*b]).then(b.cmp(a)));
        zone[i] = najlepsza.map_or(ZoneKind::Green, |z| ZoneKind::ALL[z]);
        rural_blocks += 1;
        rural_area += f64::from(blocks.blocks[i].area_m2);
    }

    // 3. Kwoty. Powierzchnia kwartałów wetowanych i pozamiejskich nie zasila kwot.
    let target_share = mix.targets(plan.epoch);
    let przydzielalne: f64 = (0..n)
        .filter(|&i| !veto[i] && !rural[i])
        .map(|i| f64::from(blocks.blocks[i].area_m2))
        .sum();
    let mut left = [0.0f64; 16];
    for z in ZoneKind::ALL {
        left[z.index()] = przydzielalne * f64::from(target_share[z.index()]);
    }

    // 4. Przydział zachłanny z marginesem: najpierw kwartały o najbardziej jednoznacznej
    //    punktacji. Remisy po `BlockId` rosnąco (stabilny `sort_by`).
    let mut kolejnosc: Vec<u32> = (0..n as u32)
        .filter(|&i| !veto[i as usize] && !rural[i as usize])
        .collect();
    let margines = |i: usize| -> f32 {
        let mut v: Vec<f32> = score[i].iter().copied().filter(|x| x.is_finite()).collect();
        v.sort_by(|a, b| b.total_cmp(a));
        match v.len() {
            0 => f32::NEG_INFINITY,
            1 => f32::INFINITY,
            _ => v[0] - v[1],
        }
    };
    let marg: Vec<f32> = (0..n).map(margines).collect();
    kolejnosc.sort_by(|&a, &b| marg[b as usize].total_cmp(&marg[a as usize]));

    let mut area = [0.0f64; 16];
    for &bi in &kolejnosc {
        let i = bi as usize;
        let a = f64::from(blocks.blocks[i].area_m2);
        let mut ranking: Vec<(usize, f32)> = (0..16)
            .filter(|&z| score[i][z].is_finite())
            .map(|z| (z, score[i][z]))
            .collect();
        ranking.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(&y.0)));
        // Najlepsza strefa, w której starczy kwoty; a gdy nie starczy w żadnej —
        // strefa z **największą niedobraną kwotą**.
        //
        // Ostatni krok celowo nie patrzy na punktację. Kwartałów jest skończenie wiele
        // i te największe trafiają tu zawsze, bo żadna reszta kwoty nie jest od nich
        // większa; gdyby rozstrzygała punktacja, cały ogon lądowałby w jednej strefie
        // (zmierzone: kwartał 23 ha wchodził do R1 przy resztce kwoty 500 m², dając
        // 21,7 % przy kwocie 11,3 %), a strefy o niskiej punktacji nie dostawałyby
        // **nigdy nic** (Logistics: 86 kandydatów, 0 kwartałów). Przy wyborze największej
        // reszty przekroczenie jest z definicji najmniejsze z możliwych.
        let wybor = ranking
            .iter()
            .find(|(z, _)| left[*z] >= a)
            .or_else(|| {
                ranking
                    .iter()
                    .max_by(|x, y| left[x.0].total_cmp(&left[y.0]))
            })
            .map(|(z, _)| *z);
        let Some(z) = wybor else {
            // Kwartał, któremu każde weto odmówiło każdej strefy — teren nie pozwala.
            zone[i] = ZoneKind::Undevelopable;
            veto[i] = true;
            continue;
        };
        zone[i] = ZoneKind::ALL[z];
        left[z] -= a;
        area[z] += a;
    }

    // 5. Wygładzanie: dwa przebiegi, zmiany aplikowane po przebiegu (M2 §5.3 krok 4).
    let (adj_start, adj_items) = adj;
    for _ in 0..2 {
        let mut zmiany: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            if veto[i] || rural[i] || marg[i] >= 0.15 {
                continue;
            }
            let mut licznik = [0u32; 16];
            let mut sasiadow = 0u32;
            for k in adj_start[i]..adj_start[i + 1] {
                let j = adj_items[k as usize] as usize;
                licznik[zone[j].index()] += 1;
                sasiadow += 1;
            }
            if sasiadow < 3 {
                continue;
            }
            let (z, c) = licznik
                .iter()
                .enumerate()
                .max_by_key(|(z, c)| (**c, std::cmp::Reverse(*z)))
                .map(|(z, c)| (z, *c))
                .unwrap_or((0, 0));
            let a = f64::from(blocks.blocks[i].area_m2);
            if c * 4 >= sasiadow * 3
                && z != zone[i].index()
                && ZoneKind::ALL[z].from_quota()
                && score[i][z].is_finite()
                && left[z] >= a
            {
                zmiany.push((i, z));
            }
        }
        for (i, z) in zmiany {
            let a = f64::from(blocks.blocks[i].area_m2);
            left[zone[i].index()] += a;
            area[zone[i].index()] -= a;
            left[z] -= a;
            area[z] += a;
            zone[i] = ZoneKind::ALL[z];
        }
    }

    // 6. Bufor przemysłu ciężkiego. Weto z planu („IndustryHeavy bliżej niż 400 m od
    //    jakiejkolwiek Residential") jest z natury cykliczne — mieszkaniówka powstaje
    //    w kroku 3, więc w kroku 1 nie ma czego pilnować. Stąd przebieg naprawczy
    //    (korekta C12) — i to **zamiana**, nie przeniesienie, żeby kwota strefy została
    //    tam, gdzie była.
    let mut przeniesione = 0u32;
    let mut nierozwiazane = 0u32;
    let blisko_mieszkaniowki = |i: usize, zone: &[ZoneKind]| -> bool {
        (0..n).any(|j| {
            zone[j].is_residential() && (s.centroid[j] - s.centroid[i]).length() < w.heavy_buffer_m
        })
    };
    // Drugie kryterium tego samego przebiegu: zakład ma stać **z podwietrznej** miasta.
    // W punktacji siedzi to jako `ZoneField::Downwind`, ale waga przegrywa czasem
    // z bliskością bocznicy, a kryterium WP7 jest progowe (90 %), nie ważone —
    // dlatego naprawa jawna (korekta C12).
    let pod_wiatr = |i: usize| s.downwind[i] >= 0.5;
    for i in 0..n {
        if zone[i] != ZoneKind::IndustryHeavy || (!blisko_mieszkaniowki(i, &zone) && pod_wiatr(i)) {
            continue;
        }
        // **Zamiana, nie przeniesienie** (korekta C12). Przeniesienie kwartału na
        // kolejną strefę z rankingu oddaje kwotę przemysłu ciężkiego, której nikt już
        // nie odbierze — w mieście czterdziestotysięcznym każdy kwartał leży w promieniu
        // 400 m od jakiegoś mieszkania, więc weto kasowało **całą** strefę (5 % kwoty
        // wsiąkało w R1). Zamiana z najlepszym kwartałem poza buforem honoruje regułę
        // tam, gdzie geometria na to pozwala, i zachowuje kwoty tam, gdzie nie pozwala.
        let ih = ZoneKind::IndustryHeavy.index();
        let ai = f64::from(blocks.blocks[i].area_m2);
        let mut mozliwe: Vec<usize> = (0..n)
            .filter(|&j| {
                j != i
                    && !veto[j]
                    && !rural[j]
                    && zone[j] != ZoneKind::IndustryHeavy
                    && score[j][ih].is_finite()
                    && score[i][zone[j].index()].is_finite()
                    && !blisko_mieszkaniowki(j, &zone)
                    && pod_wiatr(j)
            })
            .collect();
        // Zamiana ma być **neutralna dla kwot**, więc kandydat musi być podobnej
        // wielkości: bez tego warunku przemysł ciężki przeskakiwał na kwartał dwa razy
        // większy i jego udział rósł z 5 % do 10,7 %. Dopiero gdy takiego nie ma,
        // bierzemy najbliższy powierzchnią.
        mozliwe.sort_by(|a, b| score[*b][ih].total_cmp(&score[*a][ih]).then(a.cmp(b)));
        let podobny = |j: &usize| {
            let aj = f64::from(blocks.blocks[*j].area_m2);
            aj >= ai * 0.5 && aj <= ai * 1.5
        };
        // Brak kandydata podobnej wielkości znaczy „miasta nie da się przemeblować
        // bez naruszenia kwot" — wtedy kwartał zostaje przy mieszkaniówce i mówimy
        // o tym w raporcie. Zamiana na kwartał półtora raza większy naprawiałaby
        // jedno kryterium kosztem drugiego (zmierzone: przemysł ciężki 10,7 % przy
        // kwocie 5 %).
        let kandydat = mozliwe.iter().copied().find(podobny);
        match kandydat {
            Some(j) => {
                let aj = f64::from(blocks.blocks[j].area_m2);
                let zj = zone[j];
                area[ih] += aj - ai;
                area[zj.index()] += ai - aj;
                zone[j] = ZoneKind::IndustryHeavy;
                zone[i] = zj;
                przeniesione += 1;
            }
            None => nierozwiazane += 1,
        }
    }
    if przeniesione > 0 {
        warnings.push(format!(
            "bufor {} m: {przeniesione} kwartałów przemysłu ciężkiego zamienionych na kwartały poza sąsiedztwem mieszkaniówki",
            w.heavy_buffer_m as i32
        ));
    }
    if nierozwiazane > 0 {
        warnings.push(format!(
            "bufor {} m: {nierozwiazane} kwartałów przemysłu ciężkiego zostaje przy mieszkaniówce — miasto jest za ciasne, żeby je rozdzielić",
            w.heavy_buffer_m as i32
        ));
    }

    let najwiekszy = (0..n)
        .filter(|&i| !veto[i] && !rural[i])
        .map(|i| f64::from(blocks.blocks[i].area_m2))
        .fold(0.0f64, f64::max);
    let mut candidates = [0u32; 16];
    for sc in &score {
        for (z, v) in sc.iter().enumerate() {
            if v.is_finite() {
                candidates[z] += 1;
            }
        }
    }
    let mut count = [0u32; 16];
    for z in &zone {
        count[z.index()] += 1;
    }
    let suma: f64 = area.iter().sum::<f64>().max(1.0);
    let mut share = [0.0f32; 16];
    for (i, a) in area.iter().enumerate() {
        share[i] = (a / suma) as f32;
    }

    ZoneResult {
        zone,
        epoch_ring,
        rings,
        share,
        target: target_share,
        count,
        candidates,
        largest_block_share: (najwiekszy / przydzielalne.max(1.0)) as f32,
        rural_blocks,
        rural_area_m2: rural_area,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indeks_strefy_jest_odwracalny() {
        for (i, z) in ZoneKind::ALL.iter().enumerate() {
            assert_eq!(z.index(), i, "{z:?}");
        }
    }

    #[test]
    fn tabela_epok_obcina_pierscienie_do_epoki_startowej() {
        let t = EpochTable::load().expect("data/epochs/epochs.ron");
        let r1950 = t.rings_for(Epoch::Y1950);
        let r2020 = t.rings_for(Epoch::Y2020);
        assert!(
            r1950.len() < r2020.len(),
            "miasto z 1950 nie może mieć pierścienia z XXI wieku"
        );
        for r in [&r1950, &r2020] {
            let s: f32 = r.iter().map(|e| e.area_share).sum();
            assert!((s - 1.0).abs() < 1e-4, "udziały nie sumują się do 1: {s}");
        }
    }

    #[test]
    fn wagi_pokrywaja_wszystkie_strefy_kwotowe() {
        let w = ZoningWeights::load().expect("data/zoning/weights.ron");
        for z in ZoneKind::ALL.iter().filter(|z| z.from_quota()) {
            assert!(w.rule(*z).is_some(), "brak reguły dla {}", z.key());
        }
    }
}
