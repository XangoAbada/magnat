//! WP12 — posadowienie budynków, wnętrza logiczne i kolejkowanie voxeli (M2 §5.6).
//!
//! Etap 6 kończy się tutaj: dla każdej parceli powstaje obrys bryły, wybrana gramatyka,
//! derywacja (WP11), komendy voxelowe oraz `Unit`/`Workplace`. Derywacja idzie
//! **równolegle** — jest najdroższym krokiem fazy — a składanie wyników jest sekwencyjne
//! w kolejności parcel, więc wynik nie zależy od kolejności ukończenia jobów (00 §3.3).
//!
//! Trzy rzeczy, które łatwo pomylić, i ich rozstrzygnięcia:
//!
//! 1. **Obrys bryły ⊆ parcela** (test T6). Prostokąt wpisany w układ osi frontu nie mieści
//!    się w działce automatycznie, bo działka nie jest prostokątem. Stąd kurczenie
//!    do skutku i odrzucenie parceli, na której nic się nie mieści.
//! 2. **Niwelacja przed posadowieniem** (ryzyko R9). Bez niej budynek na zboczu wisi
//!    z jednej strony i jest zakopany z drugiej.
//! 3. **Stanowiska pracy z rodzaju lokalu, nie z archetypu.** Archetyp (`data/buildings/`)
//!    powstaje dopiero w M2e (WP13) i wtedy nadpisze tę liczbę; do tego czasu przelicznik
//!    stoi w `data/jobs/roles.ron`. Inaczej M2d nie miałby czym wypełnić kontraktu
//!    `wage_band` dla M3, a „budynki mają stanowiska pracy" z kryterium podfazy
//!    byłoby nieprawdą.

use super::blocks::BlockSet;
use super::derive::{self, BuildParams, Derived, RoofKind, Scope};
use super::districts::DistrictSet;
use super::grammar::{BuildingGrammar, GrammarId, GrammarSet, UnitClass, MAX_PROTRUDE_M};
use super::parcels::{ParcelSet, ParcelStatus};
use super::poly;
use super::road::{PolyArena, PolyRef, RoadClass, RoadFlags, RoadNetwork, SegmentId};
use super::voxels::{self, SRC_BUILDING, SRC_BUILDING_VOID, SRC_PARCEL_CUT, SRC_PARCEL_PAD};
use super::zoning::{EpochId, ZoneKind, ZoneResult};
use super::CityPlan;
use crate::query::TerrainQuery;
use magnat_core::{rng, BuildingId, CitizenId, JobRoleId, Money, Rng, SiteId, StreamId, Tick};
use magnat_jobs::JobPool;
use magnat_spatial::{Aabb2, Aabb3, CsrGrid, GridSpec, Vec2};
use magnat_voxel::{CarveShape, EditOp, EditQueue, MaterialRegistry};
use smallvec::SmallVec;
use std::ops::Range;

// ── Typy wyjściowe (kontrakt M2 §6) ──────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntranceKind {
    Main,
    Service,
    /// Rampa przeładunkowa — wymagana dla `Logistics` i `IndustryHeavy` (test T4).
    Ramp,
    Garage,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Entrance {
    pub pos: glam::Vec3,
    pub seg: SegmentId,
    pub t: f32,
    pub kind: EntranceKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitKind {
    Dwelling { rooms: u8 },
    Retail,
    Office,
    Workshop,
    Storage,
    Common,
}

impl UnitKind {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            UnitKind::Dwelling { .. } => "mieszkanie",
            UnitKind::Retail => "handel",
            UnitKind::Office => "biuro",
            UnitKind::Workshop => "warsztat",
            UnitKind::Storage => "magazyn",
            UnitKind::Common => "część wspólna",
        }
    }

    #[must_use]
    pub const fn is_dwelling(self) -> bool {
        matches!(self, UnitKind::Dwelling { .. })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitOccupant {
    Vacant,
    Household(magnat_core::HouseholdId),
    Site(SiteId),
}

/// Indeks w globalnej tablicy lokali (korekta D8). Nie `Entity`: lokali jest ~190 tys.,
/// nie są odpytywane przekrojowo po archetypach i żyją w ciągłym zakresie `Building.units`
/// — ta sama logika co `K-16` dla partii i ofert.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UnitIdx(pub u32);

#[derive(Clone, PartialEq, Debug)]
pub struct Unit {
    pub building: BuildingId,
    /// Ujemne = kondygnacja podziemna.
    pub floor: i8,
    pub kind: UnitKind,
    pub area_m2: u16,
    /// W M2 zawsze `Vacant`; `Site(..)` dopisze M2e (Etap 7).
    pub occupant: UnitOccupant,
    /// Miesięcznie; podpowiedź startowa dla M5, nie cena.
    pub rent_hint: Money,
    pub workplaces: Range<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftId {
    I,
    II,
    III,
    Flexible,
}

/// Widełki, nie pensja. Pensje emergentne to M7 — M2 daje przedział startowy, żeby M3
/// (Etap 8) mógł dopasować dochody rodzin do wartości mieszkań. Grosze, miesięcznie, brutto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WageBand {
    pub min: Money,
    pub median: Money,
    pub max: Money,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Workplace {
    pub unit: UnitIdx,
    /// `None` do czasu Etapu 7 (M2e, WP13) — stanowisko istnieje, ale nie wie jeszcze,
    /// czyim jest zakładem. `Option` zamiast sentinela, bo `SiteId` jest `Entity`
    /// i nisza `Option` nic nie kosztuje.
    pub site: Option<SiteId>,
    pub role: JobRoleId,
    pub shift: ShiftId,
    pub wage_band: WageBand,
    /// W M2 zawsze `None`.
    pub occupant: Option<CitizenId>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Building {
    pub parcel: magnat_core::ParcelId,
    pub grammar: GrammarId,
    pub epoch: EpochId,
    pub footprint: PolyRef,
    /// Nadziemne.
    pub floors: u8,
    pub basements: u8,
    /// Wysokość każdej kondygnacji — M11 tnie widok po stropach, nie po stałej wysokości.
    pub floor_heights_dm: SmallVec<[u16; 8]>,
    pub height_dm: u16,
    pub gross_area_m2: u32,
    pub units: Range<u32>,
    /// Stan techniczny: f(epoka, wartość gruntu, szum).
    pub condition: magnat_core::Q,
    pub aabb: Aabb3,
    pub entrances: SmallVec<[Entrance; 4]>,
}

/// Wynik Etapu 6.
#[derive(Clone, Debug)]
pub struct BuildingSet {
    pub buildings: Vec<Building>,
    pub units: Vec<Unit>,
    pub workplaces: Vec<Workplace>,
    /// „Budynki w prostokącie" — selekcja do kadru dla M11 i karta inspekcji.
    pub index: CsrGrid<BuildingId>,
    pub report: BuildReport,
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct BuildReport {
    pub buildings: u32,
    pub units: u32,
    pub dwellings: u32,
    pub workplaces: u32,
    /// Parcele, na których po cofnięciach nie zmieściła się żadna bryła.
    pub too_small: u32,
    /// Parcele świadomie niezabudowane — `applies.coverage` gramatyki.
    pub left_vacant: u32,
    /// Użycia gramatyki awaryjnej. Kryterium akceptacyjne §5.6: < 2%.
    pub fallback: u32,
    /// Fallback w rozbiciu na strefy (`ZoneKind::index`) — bez tego „6 % awaryjnych"
    /// nie mówi, której gramatyki brakuje, a właśnie o to pytanie chodzi.
    pub fallback_zone: [u32; 16],
    /// Trafienia dopiero po rozluźnieniu filtru `applies` — luka w katalogu gramatyk,
    /// nie błąd generacji, ale ma być widoczna.
    pub relaxed: u32,
    /// To samo w rozbiciu na filtr, który trzeba było pominąć. Rozróżnienie ma znaczenie
    /// operacyjne: brak epoki albo stylu łata się **plikiem** w `data/grammar/`, a brak
    /// przedziału wartości gruntu — **liczbą** w istniejącym pliku. Suma nie musi być
    /// równa `relaxed`: jeden dobór pomija filtry narastająco (M2f, WP19).
    pub relaxed_epoch: u32,
    pub relaxed_style: u32,
    pub relaxed_value: u32,
    /// Derywacje przerwane limitem węzłów albo głębokości (M2 §5.6: liczone, nie ciche).
    pub truncated: u32,
    /// Wysunięcia (`Protrude`): postawione, przycięte do granicy działki i odrzucone
    /// jako płytsze niż voxel. Bez tych trzech liczb nie widać, czy balkonów nie ma,
    /// bo gramatyka ich nie chce, czy dlatego, że nigdzie się nie mieszczą (M2f §5.6c).
    pub protrusions: u32,
    pub protrusions_clipped: u32,
    pub protrusions_dropped: u32,
    /// Budynki w strefie `Logistics`/`IndustryHeavy` bez drogi bez `NO_HEAVY` w zasięgu
    /// (korekta D7) — rampa nie powstała i ma to być widać.
    pub ramp_missing: u32,
    pub edit_commands: u32,

    // ── WP20: różnorodność ──────────────────────────────────────────────────────
    /// Ile budynków dostała każda gramatyka (indeksy jak `GrammarId`).
    pub grammar_hist: Vec<u32>,
    /// Pary budynków w promieniu 60 m i ile z nich ma **identyczną** sygnaturę.
    /// Dwie liczby zamiast udziału, bo `BuildReport` jest `Eq` i nie trzyma floatów.
    pub signature_pairs: u32,
    pub signature_repeats: u32,
    /// Najgorsze sąsiedztwo: udział powtórzeń w promilach i jego współrzędne w metrach —
    /// żeby dało się tam polecieć kamerą, zamiast czytać, że test upadł.
    pub worst_neighbourhood_permille: u32,
    pub worst_neighbourhood_at: (i32, i32),
    /// Najniższa entropia Shannona rozkładu **sygnatur** w dzielnicy mieszkaniowej
    /// o co najmniej [`MIN_BUDYNKOW_DZIELNICY`] budynkach, w milibitach, i numer tej
    /// dzielnicy. Zero znaczy „nie było czego mierzyć".
    ///
    /// Po sygnaturach, a nie po gramatykach — bo dzielnica zbudowana w całości z chałup,
    /// w której każda ma inny materiał, inny dach i inną wysokość, **jest** różnorodna,
    /// a entropia gramatyk pokazywała dla niej 0,29 bita. Sygnatura mierzy to, co gracz
    /// widzi z ulicy; identyfikator gramatyki mierzy, jak zorganizowany jest katalog.
    pub min_district_entropy_mbits: u32,
    pub min_district_entropy_at: u16,
    pub districts_measured: u32,
    /// Dzielnica o najniższej entropii w trzech liczbach: **która gramatyka** ją zdominowała,
    /// ile w niej ma budynków i ile budynków ma cała dzielnica. Bez tego sama entropia mówi,
    /// że jest źle, ale nie mówi, czy to wina katalogu, czy charakteru dzielnicy.
    pub min_district_top: (u16, u32, u32),
    /// Entropia rozkładu sygnatur w **całym mieście**, w milibitach. Dzielnica ma prawo być
    /// monotonna — osiedle płytowe takie jest i tak ma wyglądać — ale miasto nie ma.
    /// To jest liczba, która odpowiada na pytanie „czy to miasto jest różnorodne".
    pub city_entropy_mbits: u32,
}

/// Promień sąsiedztwa z testu T13 — mniej więcej to, co widać z chodnika po obu
/// stronach ulicy.
pub const PROMIEN_SASIEDZTWA_M: f32 = 60.0;

/// Poniżej tylu budynków entropia dzielnicy jest szumem, nie miarą.
pub const MIN_BUDYNKOW_DZIELNICY: u32 = 100;

/// Cztery znaczniki, po których dwa budynki tej samej gramatyki są albo nie są tym samym
/// budynkiem: gramatyka, liczba kondygnacji, materiał ściany i kształt dachu. Dokładnie to,
/// co gracz rozróżnia z chodnika po drugiej stronie ulicy — i nic więcej, bo detal poniżej
/// tego progu i tak ginie w rastrze (M2f §5.6c).
///
/// Liczona w generacji i **nietrzymana w `Building`**: 14,5 tys. budynków × 4 B to pamięć
/// za miarę jakości, a nie za stan gry. Do raportu trafiają agregaty, nie tablica sygnatur.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BuildingSignature(pub u32);

impl BuildingSignature {
    /// Pakowanie: 6 bitów gramatyki, 5 kondygnacji, 10 materiału, 2 dachu — 23 z 32.
    /// Przekroczenie zakresu **nasyca**, nie zawija: budynek 40-kondygnacyjny ma być
    /// nieodróżnialny od 31-kondygnacyjnego, a nie od 8-kondygnacyjnego.
    #[must_use]
    pub fn new(grammar: GrammarId, floors: u8, wall: magnat_voxel::MaterialId, roof: RoofKind) -> BuildingSignature {
        let g = u32::from(grammar.0).min(63);
        let f = u32::from(floors).min(31);
        let m = u32::from(wall.0).min(1023);
        BuildingSignature(g | (f << 6) | (m << 11) | (roof.index() << 21))
    }
}

#[must_use]
pub fn building_id(i: u32) -> BuildingId {
    BuildingId(magnat_core::Entity::new(
        i,
        std::num::NonZeroU32::new(1).expect("1 != 0"),
    ))
}

// ── Katalog ról (`data/jobs/`) ───────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize)]
pub struct JobRole {
    pub key: String,
    pub unit: UnitClass,
    /// Powierzchnia lokalu na jedno stanowisko.
    pub m2_per_workplace: u16,
    pub shift: ShiftKey,
    /// Widełki bazowe w epoce współczesnej, grosze miesięcznie brutto.
    pub wage_base: (i64, i64, i64),
    /// Prestiż zawodu 0..=100 — składnik „zawód" funkcji statusu (M3c §5.8).
    /// Etap 8 przepisuje go do `CityFacts.job_prestige` (M3d, korekta E-14).
    #[serde(default = "prestiz_neutralny")]
    pub prestige: u8,
}

/// Rola bez wpisanego prestiżu jest zawodem przeciętnym, nie zawodem bez znaczenia.
fn prestiz_neutralny() -> u8 {
    50
}

/// Zmiana w danych. Osobny typ od [`ShiftId`], bo ten drugi jest kontraktem dla M3
/// i nie ma powodu, żeby wisiał na `serde`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize)]
pub enum ShiftKey {
    I,
    II,
    III,
    Flexible,
}

impl From<ShiftKey> for ShiftId {
    fn from(s: ShiftKey) -> ShiftId {
        match s {
            ShiftKey::I => ShiftId::I,
            ShiftKey::II => ShiftId::II,
            ShiftKey::III => ShiftId::III,
            ShiftKey::Flexible => ShiftId::Flexible,
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct JobTable {
    pub schema_version: u32,
    pub epoch_wage_mult: Vec<(String, f32)>,
    pub roles: Vec<JobRole>,
}

pub const JOBS_SCHEMA_VERSION: u32 = 1;

impl JobTable {
    pub fn load() -> Result<JobTable, super::zoning::EpochError> {
        use super::zoning::EpochError;
        let path = crate::assets::data_path("jobs/roles.ron");
        let txt = std::fs::read_to_string(&path).map_err(EpochError::Io)?;
        let t: JobTable = ron::from_str(&txt).map_err(EpochError::Ron)?;
        if t.schema_version != JOBS_SCHEMA_VERSION {
            return Err(EpochError::Schema {
                found: t.schema_version,
            });
        }
        Ok(t)
    }

    /// Rola obsadzająca dany rodzaj lokalu; `None` dla mieszkań i części wspólnych.
    #[must_use]
    pub fn role_for(&self, k: UnitKind) -> Option<(JobRoleId, &JobRole)> {
        let szukany = match k {
            UnitKind::Retail => UnitClass::Retail,
            UnitKind::Office => UnitClass::Office,
            UnitKind::Workshop => UnitClass::Workshop,
            UnitKind::Storage => UnitClass::Storage,
            UnitKind::Dwelling { .. } | UnitKind::Common => return None,
        };
        self.roles
            .iter()
            .position(|r| r.unit == szukany)
            .map(|i| (JobRoleId(i as u16), &self.roles[i]))
    }

    #[must_use]
    pub fn epoch_mult(&self, epoch_key: &str) -> f32 {
        self.epoch_wage_mult
            .iter()
            .find(|(k, _)| k == epoch_key)
            .map_or(1.0, |(_, v)| *v)
    }
}

/// `WageBand` = widełki roli × epoka × zamożność dzielnicy (M2 §6, kontrakt dla M3).
#[must_use]
pub fn wage_band(role: &JobRole, epoch_mult: f32, income_tier: u8) -> WageBand {
    // Dzielnica zamożna płaci więcej za tę samą rolę: 0,88 przy tier 0, 1,18 przy 4.
    let tier = 0.88 + 0.075 * f32::from(income_tier.min(4));
    let m = f64::from(epoch_mult * tier);
    let skala = |v: i64| Money((v as f64 * m).round() as i64);
    WageBand {
        min: skala(role.wage_base.0),
        median: skala(role.wage_base.1),
        max: skala(role.wage_base.2),
    }
}

// ── Obrys bryły ──────────────────────────────────────────────────────────────────────

/// Najmniejszy sensowny obrys: poniżej tego parcela zostaje pusta.
const MIN_FOOTPRINT_M2: f32 = 24.0;
const MIN_BOK_M: f32 = 3.0;
/// Najwęższy front, przy którym w ogóle szukamy gramatyki. Poniżej — odpad podziału.
const MIN_FRONT_M: f32 = 6.0;

/// Wyznacza obrys bryły na parceli: prostokąt w układzie osi frontu, cofnięty zgodnie
/// z `massing` i **zmieszczony w wielokącie działki** (test T6).
///
/// Kurczenie zamiast dokładnego wpisania prostokąta maksymalnego: działki z podziału
/// pasowego są prawie prostokątne, więc pierwsze przybliżenie prawie zawsze wystarcza,
/// a algorytm maksymalnego prostokąta w wielokącie jest o dwa rzędy wielkości droższy
/// i nie kupuje tu niczego (00, „Dobre praktyki": YAGNI).
#[must_use]
pub fn footprint_for(
    obrys: &[Vec2],
    front_dir: Option<Vec2>,
    front_point: Option<Vec2>,
    massing: &super::grammar::Massing,
) -> Option<Scope> {
    if obrys.len() < 3 {
        return None;
    }
    let obb = poly::min_area_obb(obrys);
    let os = match front_dir {
        Some(d) if d.length_squared() > 0.25 => d.normalize(),
        _ => obb.axis,
    };
    let srodek = poly::centroid(obrys);

    // **Ulica jest zawsze po stronie „minus" osi `v`.** Kierunek odcinka drogi nie mówi,
    // po której jego stronie leży działka, więc bez tego obrotu `Face::Front` w gramatyce
    // wypadał na podwórzu mniej więcej co drugi budynek — witryna parteru usługowego
    // patrzyła w oficynę, a balkon z `Protrude(Front)` wychodziłby w głąb kwartału.
    // Obrót o 180° nie zmienia prostokąta, tylko jego orientację.
    let u = match front_point {
        Some(fp) if (fp - srodek).dot(Vec2::new(-os.y, os.x)) > 0.0 => -os,
        _ => os,
    };
    let v = Vec2::new(-u.y, u.x);

    // Rozpiętość działki w układzie (u, v).
    let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in obrys {
        let d = *p - srodek;
        let (du, dv) = (d.dot(u), d.dot(v));
        u0 = u0.min(du);
        u1 = u1.max(du);
        v0 = v0.min(dv);
        v1 = v1.max(dv);
    }

    // Front jest po stronie `v0` z założenia (obrót osi wyżej), więc cofnięcia idą
    // wprost, bez rozróżniania przypadków.
    let a = v0 + massing.setback_front_m;
    let mut b = v1 - massing.setback_back_m;
    // Trakt: budynek nie rozlewa się na całą głębokość działki.
    if b - a > massing.max_depth_m {
        b = a + massing.max_depth_m;
    }
    let (su0, su1) = (u0 + massing.setback_side_m, u1 - massing.setback_side_m);
    if su1 - su0 < MIN_BOK_M || b - a < MIN_BOK_M {
        return None;
    }

    let mut half_u = (su1 - su0) * 0.5;
    let mut half_v = (b - a) * 0.5;
    let mut c = srodek + u * ((su0 + su1) * 0.5) + v * ((a + b) * 0.5);

    // Kurczenie do skutku: osiem punktów kontrolnych (narożniki i środki boków).
    for _ in 0..8 {
        if miesci_sie(obrys, c, u, v, half_u, half_v) {
            break;
        }
        half_u *= 0.9;
        half_v *= 0.9;
        // Środek ciągniemy ku centroidowi działki — tam mieści się najłatwiej.
        c += (srodek - c) * 0.15;
    }
    if !miesci_sie(obrys, c, u, v, half_u, half_v) {
        return None;
    }
    if half_u < MIN_BOK_M * 0.5
        || half_v < MIN_BOK_M * 0.5
        || 4.0 * half_u * half_v < MIN_FOOTPRINT_M2
    {
        return None;
    }
    Some(Scope::new(
        glam::Vec3::new(c.x, c.y, 0.0),
        u,
        glam::Vec3::new(half_u, half_v, 0.0),
    ))
}

/// Największy wysięg z granicy działki nad chodnik — powyżej skrajni z `OVERHANG_MIN_Z_M`.
///
/// Nie mierzymy szerokości chodnika, tylko korzystamy z tego, że jest on **zawsze szerszy**:
/// jezdnia zajmuje 60 % pasa drogowego (`voxels.rs`), a najwęższy pas uliczny to `Service`
/// z 8 m, czyli 1,6 m pobocza z każdej strony. Wysięg 1,5 m nie dosięga więc jezdni w żadnej
/// klasie. Gdyby doszła klasa węższa niż 8 m, ta liczba przestaje być prawdziwa i trzeba
/// będzie liczyć pobocze z `row_m` odcinka frontowego.
const OVERHANG_M: f32 = 1.5;

/// Krok i zasięg pomiaru zapasu. Ćwierć metra to czwarta część voxela poziomego —
/// dokładniej mierzyć nie ma po co, bo i tak wszystko wyląduje na siatce metrowej.
const ZAPAS_KROK_M: f32 = 0.25;

/// Ile bryła może urosnąć w danym kierunku, zanim wyjdzie z wielokąta działki.
///
/// Liniowo, nie połowieniem: zakres ma osiem kroków, a `miesci_sie` bada osiem punktów,
/// więc całość to 64 testy przynależności na kierunek — mniej, niż kosztowałoby
/// pilnowanie niezmienników wyszukiwania binarnego (00, „Dobre praktyki": boring over clever).
fn zapas(obrys: &[Vec2], c: Vec2, u: Vec2, v: Vec2, hu: f32, hv: f32, dir: Axis2) -> f32 {
    let mut ostatni = 0.0;
    let mut d = ZAPAS_KROK_M;
    while d <= MAX_PROTRUDE_M + 1e-3 {
        let (pc, phu, phv) = match dir {
            Axis2::Front => (c - v * (d * 0.5), hu, hv + d * 0.5),
            Axis2::Back => (c + v * (d * 0.5), hu, hv + d * 0.5),
            Axis2::Side => (c, hu + d, hv),
        };
        if !miesci_sie(obrys, pc, u, v, phu, phv) {
            break;
        }
        ostatni = d;
        d += ZAPAS_KROK_M;
    }
    ostatni
}

/// Kierunek pomiaru zapasu. `Side` mierzy obie strony naraz, bo `Protrude(Side)`
/// stawia obie i obowiązuje ciaśniejsza.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis2 {
    Front,
    Back,
    Side,
}

/// Rozpiętość wielokąta w układzie `(u, v)`: szerokość wzdłuż `u` i głębokość wzdłuż `v`.
#[must_use]
pub fn rozpietosc(obrys: &[Vec2], u: Vec2) -> (f32, f32) {
    let v = Vec2::new(-u.y, u.x);
    let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in obrys {
        let (du, dv) = (p.dot(u), p.dot(v));
        u0 = u0.min(du);
        u1 = u1.max(du);
        v0 = v0.min(dv);
        v1 = v1.max(dv);
    }
    ((u1 - u0).max(0.0), (v1 - v0).max(0.0))
}

fn miesci_sie(obrys: &[Vec2], c: Vec2, u: Vec2, v: Vec2, hu: f32, hv: f32) -> bool {
    for (su, sv) in [
        (-1.0, -1.0),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-1.0, 0.0),
        (1.0, 0.0),
    ] {
        let p = c + u * (hu * su as f32) + v * (hv * sv as f32);
        if !poly::contains(obrys, p) {
            return false;
        }
    }
    true
}

// ── Wybór gramatyki ──────────────────────────────────────────────────────────────────

struct PickCtx<'a> {
    zone: ZoneKind,
    epoch_key: &'a str,
    district_key: &'a str,
    land_value: i64,
    front_m: f32,
    depth_m: f32,
}

/// Który filtr `applies` wolno pominąć. Strefa i wymiary działki **nigdy** — kamienica
/// na 200-metrowym froncie hali to nie „trochę inny styl", tylko bryła, której gramatyka
/// nie umie zbudować.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Relax {
    None,
    Epoch,
    EpochStyle,
    EpochStyleValue,
}

fn pasuje(g: &BuildingGrammar, c: &PickCtx, relax: Relax) -> bool {
    let a = &g.applies;
    // Filtr epoki i stylu to `Applies::covers` z pominiętym wymiarem — ta sama reguła
    // co w `GrammarSet::covered`, żeby „luka w katalogu" i „dobór dla parceli"
    // nie mogły się rozjechać (00, „Dobre praktyki": DRY dotyczy wiedzy).
    let epoka = matches!(relax, Relax::None).then_some(c.epoch_key);
    let styl = matches!(relax, Relax::None | Relax::Epoch).then_some(c.district_key);
    let wartosc = matches!(relax, Relax::EpochStyleValue)
        || (c.land_value >= a.land_value.0 && c.land_value <= a.land_value.1);
    a.covers(c.zone.key(), epoka, styl)
        && wartosc
        && (c.front_m >= a.frontage_m.0 && c.front_m <= a.frontage_m.1)
        && (c.depth_m >= a.depth_m.0 && c.depth_m <= a.depth_m.1)
}


/// Wynik doboru gramatyki.
struct Picked {
    grammar: Option<GrammarId>,
    /// Gramatyka awaryjna — kryterium §5.6 mówi „< 2%", więc jest liczona osobno.
    fallback: bool,
    /// Etap rozluźnienia, na którym trafiono. Nie jest błędem (miasto z 1990 ma kwartały
    /// z pięciu epok, a katalog nie musi pokrywać każdej kombinacji), ale ma być widoczne
    /// w raporcie — inaczej luka w danych nigdy nie wyjdzie.
    relaxed: Relax,
}

/// Wybór gramatyki dla parceli: filtr `applies`, potem losowanie ważone `weight`.
///
/// Filtry pomijane są **po kolei i w ustalonej kolejności**, nie naraz: najpierw epoka
/// (kwartał z 1890 w strefie, dla której mamy tylko gramatykę powojenną), potem styl
/// dzielnicy, na końcu wartość gruntu. Gramatyka awaryjna jest ostatnią deską, nie drugą.
fn pick_grammar(set: &GrammarSet, c: &PickCtx, r: &mut Rng) -> Picked {
    for relax in [
        Relax::None,
        Relax::Epoch,
        Relax::EpochStyle,
        Relax::EpochStyleValue,
    ] {
        let kandydaci: Vec<usize> = (0..set.len())
            .filter(|i| !set.all()[*i].id.starts_with("_fallback"))
            .filter(|i| pasuje(&set.all()[*i], c, relax))
            .collect();
        let suma: u32 = kandydaci
            .iter()
            .map(|i| u32::from(set.all()[*i].applies.weight))
            .sum();
        if suma == 0 {
            continue;
        }
        // Losujemy **raz na cały dobór**, nie raz na etap rozluźnienia: inaczej działka,
        // która potrzebowała drugiego podejścia, zużywałaby inną liczbę losowań
        // i przesuwała strumień względem sąsiadki.
        let mut los = r.next_u32() % suma;
        for i in kandydaci {
            let w = u32::from(set.all()[i].applies.weight);
            if los < w {
                let pokrycie = set.all()[i].applies.coverage.clamp(0.0, 1.0);
                if pokrycie < 1.0 && (r.next_u32() % 1000) as f32 / 1000.0 >= pokrycie {
                    return Picked {
                        grammar: None,
                        fallback: false,
                        relaxed: Relax::None,
                    };
                }
                return Picked {
                    grammar: Some(GrammarId(i as u16)),
                    fallback: false,
                    relaxed: relax,
                };
            }
            los -= w;
        }
    }
    Picked {
        grammar: set.fallback_for(c.zone),
        fallback: set.fallback_for(c.zone).is_some(),
        relaxed: Relax::None,
    }
}

// ── Główny przebieg ──────────────────────────────────────────────────────────────────

/// Dlaczego na parceli nic nie stanęło. Rozróżnienie ma znaczenie: pusta działka
/// **z zamiaru** (pokrycie gramatyki) to co innego niż działka, na której nic się
/// nie mieści — pierwsze jest urbanistyką, drugie sygnałem błędu w podziale na parcele.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Skip {
    /// Strefa niezabudowywalna — woda, teren wyłączony.
    NotBuildable,
    /// `applies.coverage` gramatyki: działka zostaje pusta z zamiaru.
    Coverage,
    /// Po cofnięciach nie zmieściła się żadna bryła.
    TooSmall,
}

/// Plan jednego budynku — wynik części równoległej, wejście części sekwencyjnej.
struct Planned {
    parcel: u32,
    grammar: GrammarId,
    base: Scope,
    base_z_m: f32,
    teren: (f32, f32),
    derived: Derived,
    fallback: bool,
    relaxed: Relax,
}

/// Wejście Etapu 6 — referencje zebrane w jedno, żeby sygnatura nie miała dwunastu pozycji.
pub struct BuildInput<'a> {
    pub plan: &'a CityPlan,
    pub terrain: &'a dyn TerrainQuery,
    pub roads: &'a RoadNetwork,
    pub blocks: &'a BlockSet,
    pub zones: &'a ZoneResult,
    pub districts: &'a DistrictSet,
    pub grammars: &'a GrammarSet,
    pub materials: &'a MaterialRegistry,
    pub jobs: &'a JobTable,
}

/// Etap 6 w całości: obrysy, derywacja, voxele, lokale i stanowiska.
#[must_use]
pub fn build_all(
    input: &BuildInput,
    geom: &mut PolyArena,
    parcels: &mut ParcelSet,
    q: &EditQueue,
    pool: &JobPool,
) -> BuildingSet {
    let przed_komend = q.len();
    let indeksy: Vec<u32> = (0..parcels.parcels.len() as u32).collect();

    // ── faza równoległa: obrys + wybór gramatyki + derywacja ────────────────────────
    let planned: Vec<Result<Planned, Skip>> = magnat_jobs::map_reduce_indexed(
        pool,
        &indeksy,
        |_, i| plan_building(input, geom, parcels, *i),
        |mut acc: Vec<Result<Planned, Skip>>, v| {
            acc.push(v);
            acc
        },
        Vec::new(),
    );

    // ── faza sekwencyjna: encje, wnętrza i komendy, w kolejności parcel ─────────────
    let mut out = BuildingSet {
        buildings: Vec::new(),
        units: Vec::new(),
        workplaces: Vec::new(),
        index: CsrGrid::empty(GridSpec::covering(
            Aabb2::new(Vec2::ZERO, Vec2::splat(input.plan.map_size_m() as f32)),
            64,
        )),
        report: BuildReport::default(),
    };
    let mut srodki: Vec<(Vec2, BuildingId)> = Vec::new();
    // Sygnatury żyją tylko tutaj: do raportu idą agregaty, nie tablica (M2f, WP20).
    let mut sygnatury: Vec<BuildingSignature> = Vec::new();
    out.report.grammar_hist = vec![0; input.grammars.len()];

    for wynik in planned {
        let p = match wynik {
            Ok(p) => p,
            Err(Skip::Coverage) => {
                out.report.left_vacant += 1;
                continue;
            }
            Err(Skip::TooSmall) => {
                out.report.too_small += 1;
                continue;
            }
            Err(Skip::NotBuildable) => continue,
        };
        let (id, srodek, sygnatura) = emit(&mut out, input, geom, parcels, q, &p);
        srodki.push((srodek, id));
        sygnatury.push(sygnatura);
    }

    out.report.buildings = out.buildings.len() as u32;
    out.report.units = out.units.len() as u32;
    out.report.edit_commands = (q.len() - przed_komend) as u32;
    out.index = CsrGrid::build(*out.index.spec(), srodki.iter().copied());
    roznorodnosc(&mut out, input, parcels, &srodki, &sygnatury);
    out
}

/// Jeden budynek: liczniki raportu, komendy voxelowe, wnętrza, encja i wpis w parceli.
///
/// Wydzielone z pętli `build_all`, bo Etap 7 (M2e) dostawia budynki na zieleni
/// i w wydobyciu **po** zamknięciu Etapu 6 (korekta F2) i musi robić to samo co do joty.
/// Zwraca to, czego pętla potrzebuje do miar różnorodności — sygnatura nie jest polem
/// `Building` z rozmysłu (M2f §5.6c).
fn emit(
    out: &mut BuildingSet,
    input: &BuildInput,
    geom: &mut PolyArena,
    parcels: &mut ParcelSet,
    q: &EditQueue,
    p: &Planned,
) -> (BuildingId, Vec2, BuildingSignature) {
    let id = building_id(out.buildings.len() as u32);
    if p.fallback {
        out.report.fallback += 1;
        out.report.fallback_zone[parcels.parcels[p.parcel as usize].zone.index()] += 1;
    }
    // Rozluźnienia są narastające: `EpochStyleValue` znaczy, że pominięto wszystkie trzy.
    match p.relaxed {
        Relax::None => {}
        Relax::Epoch => {
            out.report.relaxed += 1;
            out.report.relaxed_epoch += 1;
        }
        Relax::EpochStyle => {
            out.report.relaxed += 1;
            out.report.relaxed_epoch += 1;
            out.report.relaxed_style += 1;
        }
        Relax::EpochStyleValue => {
            out.report.relaxed += 1;
            out.report.relaxed_epoch += 1;
            out.report.relaxed_style += 1;
            out.report.relaxed_value += 1;
        }
    }
    if p.derived.truncated {
        out.report.truncated += 1;
    }
    out.report.protrusions += p.derived.protrusions;
    out.report.protrusions_clipped += p.derived.protrusions_clipped;
    out.report.protrusions_dropped += p.derived.protrusions_dropped;
    queue_building(q, p);
    let (units_from, dwell, wp) = interiors(input, out, p, id, parcels);
    let footprint = geom.push(&rogi(&p.base));
    let aabb = bryla(p);
    let entrances = wejscia(input, parcels, p);
    if matches!(
        parcels.parcels[p.parcel as usize].zone,
        ZoneKind::Logistics | ZoneKind::IndustryHeavy
    ) && !entrances.iter().any(|e| e.kind == EntranceKind::Ramp)
    {
        out.report.ramp_missing += 1;
    }
    let pole = (p.base.area_m2() * f32::from(p.derived.floors.max(1))) as u32;
    out.buildings.push(Building {
        parcel: super::parcels::parcel_id(p.parcel),
        grammar: p.grammar,
        epoch: EpochId(
            input.blocks.blocks[parcels.parcels[p.parcel as usize].block.0 as usize].epoch_ring,
        ),
        footprint,
        floors: p.derived.floors,
        basements: p.derived.basements,
        floor_heights_dm: p.derived.floor_heights_dm.clone(),
        height_dm: p.derived.height_dm,
        gross_area_m2: pole,
        units: units_from..out.units.len() as u32,
        condition: stan_techniczny(input, p, parcels),
        aabb,
        entrances,
    });
    if let Some(n) = out.report.grammar_hist.get_mut(p.grammar.0 as usize) {
        *n += 1;
    }
    let parcel = &mut parcels.parcels[p.parcel as usize];
    parcel.status = ParcelStatus::Built;
    parcel.building = Some(id);
    out.report.dwellings += dwell;
    out.report.workplaces += wp;
    (
        id,
        Vec2::new(p.base.center.x, p.base.center.y),
        BuildingSignature::new(
            p.grammar,
            p.derived.floors,
            p.derived.wall_material,
            p.derived.roof_shape,
        ),
    )
}

/// Budynki dostawiane przez Etap 7 na działkach, których Etap 6 nie tknął — zieleń
/// i wydobycie (korekta F2) — oraz przy naprawie domknięcia łańcuchów (KROK 4b).
///
/// Zwraca identyfikator albo `None`, jeśli na działce nic się nie zmieściło. Indeks
/// budynków przebudowuje się **raz, na końcu**, bo `CsrGrid` nie jest przyrostowy.
/// Miary różnorodności zostają takie, jakie zmierzył Etap 6: pawilon w parku i nadszybie
/// na hałdzie nie są tkanką miejską, którą T13 ma oceniać, a wliczenie ich rozcieńczałoby
/// miarę zamiast ją poprawiać.
pub fn build_for_sites(
    input: &BuildInput,
    geom: &mut PolyArena,
    parcels: &mut ParcelSet,
    out: &mut BuildingSet,
    q: &EditQueue,
    zlecenia: &[(u32, GrammarId)],
) -> Vec<Option<BuildingId>> {
    let mut wynik = Vec::with_capacity(zlecenia.len());
    let mut zmiana = false;
    for (parcel, gid) in zlecenia {
        match plan_building_inner(input, geom, parcels, *parcel, Some(*gid)) {
            Ok(p) => {
                let (id, _, _) = emit(out, input, geom, parcels, q, &p);
                wynik.push(Some(id));
                zmiana = true;
            }
            Err(_) => wynik.push(None),
        }
    }
    if zmiana {
        out.report.buildings = out.buildings.len() as u32;
        out.report.units = out.units.len() as u32;
        let srodki: Vec<(Vec2, BuildingId)> = out
            .buildings
            .iter()
            .enumerate()
            .map(|(i, b)| {
                (
                    Vec2::new(
                        (b.aabb.min.x + b.aabb.max.x) * 0.5,
                        (b.aabb.min.y + b.aabb.max.y) * 0.5,
                    ),
                    building_id(i as u32),
                )
            })
            .collect();
        out.index = CsrGrid::build(*out.index.spec(), srodki.into_iter());
    }
    wynik
}

/// Miary z testu T13: powtarzalność sygnatur w sąsiedztwie i entropia gramatyk
/// w dzielnicy. Liczone **po** zbudowaniu indeksu, bo obie potrzebują sąsiadów.
///
/// Bez miary „różnorodny" jest opinią, a kryterium ukończenia musi dać się obalić
/// (M2f, WP20). Stąd dwa progi, a nie jeden: sąsiedztwo łapie pierzeję z jednej formy
/// powielonej dwadzieścia razy, entropia — dzielnicę, w której jedna gramatyka wygrywa
/// wszystkie losowania. Pierwsze zdarza się przy wąskich działkach, drugie przy wagach.
fn roznorodnosc(
    out: &mut BuildingSet,
    input: &BuildInput,
    parcels: &ParcelSet,
    srodki: &[(Vec2, BuildingId)],
    sygnatury: &[BuildingSignature],
) {
    let mut sasiedzi: Vec<BuildingId> = Vec::new();
    let (mut pary, mut powtorki) = (0u32, 0u32);
    for (i, (pos, _)) in srodki.iter().enumerate() {
        sasiedzi.clear();
        out.index
            .query_radius(*pos, PROMIEN_SASIEDZTWA_M, &mut sasiedzi);
        let (mut lokalne, mut lokalne_powtorki) = (0u32, 0u32);
        for id in &sasiedzi {
            let j = id.0.index() as usize;
            if j == i {
                continue;
            }
            lokalne += 1;
            if sygnatury[j] == sygnatury[i] {
                lokalne_powtorki += 1;
            }
        }
        pary += lokalne;
        powtorki += lokalne_powtorki;
        // Najgorsze sąsiedztwo liczymy tylko tam, gdzie w ogóle jest sąsiedztwo:
        // budynek z dwoma sąsiadami i dwiema powtórkami daje 100 % i nic nie znaczy.
        if lokalne >= 8 {
            let promille = lokalne_powtorki * 1000 / lokalne;
            if promille > out.report.worst_neighbourhood_permille {
                out.report.worst_neighbourhood_permille = promille;
                out.report.worst_neighbourhood_at = (pos.x as i32, pos.y as i32);
            }
        }
    }
    out.report.signature_pairs = pary;
    out.report.signature_repeats = powtorki;
    let mut wszystkie: Vec<u32> = sygnatury.iter().map(|s| s.0).collect();
    wszystkie.sort_unstable();
    let hist: Vec<u32> = wszystkie
        .chunk_by(|a, b| a == b)
        .map(|o| o.len() as u32)
        .collect();
    out.report.city_entropy_mbits = entropia_mbits(&hist, wszystkie.len() as u32);

    // ── entropia sygnatur per dzielnica ─────────────────────────────────────────
    let n = input.districts.districts.len();
    let mut sygn_dzielnicy: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut gram_dzielnicy: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (i, b) in out.buildings.iter().enumerate() {
        let d = parcels.parcels[b.parcel.0.index() as usize].district.0 as usize;
        let (Some(sy), Some(gr)) = (sygn_dzielnicy.get_mut(d), gram_dzielnicy.get_mut(d)) else {
            continue;
        };
        sy.push(sygnatury[i].0);
        if gr.is_empty() {
            gr.resize(input.grammars.len(), 0);
        }
        if let Some(c) = gr.get_mut((sygnatury[i].0 & 63) as usize) {
            *c += 1;
        }
    }
    out.report.min_district_entropy_mbits = u32::MAX;
    for (d, sy) in sygn_dzielnicy.iter().enumerate() {
        let Some(kind) = input.districts.districts.get(d).map(|x| x.kind) else {
            continue;
        };
        if !kind.is_residential() || sy.len() < MIN_BUDYNKOW_DZIELNICY as usize {
            continue;
        }
        out.report.districts_measured += 1;
        // Sygnatur jest mało na dzielnicę, więc sortowanie i zliczanie serii jest tańsze
        // od mapy — i, co ważniejsze, nie wymaga iterowania po `HashMap` (00 §3.2).
        let mut posortowane = sy.clone();
        posortowane.sort_unstable();
        let mut hist: Vec<u32> = Vec::new();
        for okno in posortowane.chunk_by(|a, b| a == b) {
            hist.push(okno.len() as u32);
        }
        let suma = sy.len() as u32;
        let mbits = entropia_mbits(&hist, suma);
        if mbits < out.report.min_district_entropy_mbits {
            out.report.min_district_entropy_mbits = mbits;
            out.report.min_district_entropy_at = d as u16;
            let gr = &gram_dzielnicy[d];
            let (g, ile) = gr.iter().enumerate().fold((0usize, 0u32), |b, (i, c)| {
                if *c > b.1 {
                    (i, *c)
                } else {
                    b
                }
            });
            out.report.min_district_top = (g as u16, ile, suma);
        }
    }
    if out.report.districts_measured == 0 {
        out.report.min_district_entropy_mbits = 0;
    }
}

/// Entropia Shannona rozkładu w **milibitach**. `log2` wyłącznie z `det_math` (00 §K-6),
/// sumowanie w kolejności indeksów — wynik wchodzi do raportu, a raport do hasha.
fn entropia_mbits(hist: &[u32], suma: u32) -> u32 {
    if suma == 0 {
        return 0;
    }
    let mut h = 0.0f64;
    for c in hist.iter().filter(|c| **c > 0) {
        let p = f64::from(*c) / f64::from(suma);
        h -= p * magnat_core::det_math::log2(p);
    }
    (h * 1000.0).max(0.0) as u32
}

/// Część równoległa — **czysta**: czyta arenę i teren, niczego nie mutuje.
fn plan_building(
    input: &BuildInput,
    geom: &PolyArena,
    parcels: &ParcelSet,
    i: u32,
) -> Result<Planned, Skip> {
    plan_building_inner(input, geom, parcels, i, None)
}

/// Wspólny rdzeń doboru bryły. `wymus` to gramatyka wskazana **imiennie przez Etap 7**
/// (M2e, korekta F2): park i kopalnia nie mają prawa trafić do `pick_grammar`, bo
/// gramatyka awaryjna na działce parkowej robi z parku osiedle — ale archetyp zakładu
/// wie, że na tej konkretnej działce ma stanąć pawilon albo nadszybie.
fn plan_building_inner(
    input: &BuildInput,
    geom: &PolyArena,
    parcels: &ParcelSet,
    i: u32,
    wymus: Option<GrammarId>,
) -> Result<Planned, Skip> {
    let p = &parcels.parcels[i as usize];
    // Strefy bez zabudowy w M2d: woda, teren wyłączony, zieleń i wydobycie. Dwie ostatnie
    // dostają obiekty dopiero w Etapie 7 (M2e §5.8 pkt 5) — park zabudowany gramatyką
    // awaryjną przestaje być parkiem.
    if !p.zone.parcelled() || matches!(p.zone, ZoneKind::Water | ZoneKind::Undevelopable) {
        return Err(Skip::NotBuildable);
    }
    if wymus.is_none() && matches!(p.zone, ZoneKind::Green | ZoneKind::Extraction) {
        return Err(Skip::NotBuildable);
    }
    let obrys = geom.get(p.poly);
    let block = &input.blocks.blocks[p.block.0 as usize];
    let epoch_key = input
        .zones
        .rings
        .get(usize::from(block.epoch_ring))
        .map_or("", |e| e.key.as_str());
    let district_key = input
        .districts
        .districts
        .get(p.district.0 as usize)
        .map_or("", |d| d.kind.key());

    // Kierunek frontu i punkt na ulicy — z odcinka frontowego działki.
    let (front_dir, front_point, front_m) = if p.frontage.is_none() {
        (None, None, 0.0)
    } else {
        let seg = &input.roads.segments[p.frontage.seg.0 as usize];
        let (a, b) = (
            input.roads.nodes[seg.a.0 as usize].pos,
            input.roads.nodes[seg.b.0 as usize].pos,
        );
        let t = (p.frontage.t0 + p.frontage.t1) * 0.5;
        (
            Some((b - a).normalize_or_zero()),
            Some(a + (b - a) * t),
            (p.frontage.t1 - p.frontage.t0).abs() * seg.length_dm as f32 / 10.0,
        )
    };

    let mut r = rng(input.plan.seed, StreamId::BuildingPick, i, Tick(0));
    // Wymiary działki **w układzie osi frontu**, nie z pola i długości pierzei: działka
    // bywa trapezem, a `pole / front` daje wtedy głębokość, której nigdzie nie widać.
    let obb = poly::min_area_obb(obrys);
    let os = front_dir
        .filter(|d| d.length_squared() > 0.25)
        .unwrap_or(obb.axis);
    let (szer_m, gleb_m) = rozpietosc(obrys, os);
    // Odpad podziału pasowego: front węższy niż jedna ściana szczytowa. Takiej działki
    // nie ratuje żadna gramatyka i **nie jest** powodem do sięgania po awaryjną — to jest
    // działka bez zabudowy, i tak ma być policzona (inaczej udział fallbacku mierzy
    // ziarnistość podziału, a nie luki w katalogu).
    let front_do_filtru = if front_m > 0.0 { front_m } else { szer_m };
    if front_do_filtru < MIN_FRONT_M || szer_m * gleb_m < MIN_FOOTPRINT_M2 {
        return Err(Skip::TooSmall);
    }
    let ctx = PickCtx {
        zone: p.zone,
        epoch_key,
        district_key,
        land_value: p.land_value_per_m2.0,
        front_m: front_do_filtru,
        depth_m: gleb_m,
    };
    let wybor = match wymus {
        Some(gid) => Picked {
            grammar: Some(gid),
            fallback: false,
            relaxed: Relax::None,
        },
        None => pick_grammar(input.grammars, &ctx, &mut r),
    };
    let gid = wybor.grammar.ok_or(Skip::Coverage)?;
    let g = input.grammars.get(gid);

    let base = footprint_for(obrys, front_dir, front_point, &g.massing).ok_or(Skip::TooSmall)?;
    // Niwelacja (R9): rzędna posadowienia to **mediana** wysokości pod obrysem.
    let (lo, hi) = voxels::teren_pod_pasem(
        input.terrain,
        Vec2::new(base.center.x, base.center.y),
        base.u,
        base.half.x,
        base.half.y,
    );
    let base_z_m = ((lo + hi) * 0.5 * 2.0).round() * 0.5;

    // Zapas na wysunięcia mierzy się raz, przed derywacją — `Protrude` dostaje cztery
    // liczby zamiast wielokąta, bo derywacja nie ma prawa znać geometrii działki.
    let srodek_base = Vec2::new(base.center.x, base.center.y);
    let v_base = base.v();
    let margins = derive::Margins {
        front_m: zapas(
            obrys,
            srodek_base,
            base.u,
            v_base,
            base.half.x,
            base.half.y,
            Axis2::Front,
        ),
        back_m: zapas(
            obrys,
            srodek_base,
            base.u,
            v_base,
            base.half.x,
            base.half.y,
            Axis2::Back,
        ),
        side_m: zapas(
            obrys,
            srodek_base,
            base.u,
            v_base,
            base.half.x,
            base.half.y,
            Axis2::Side,
        ),
        overhang_front_m: if p.frontage.is_none() { 0.0 } else { OVERHANG_M },
    };

    let derived = derive::derive(
        g,
        input.materials,
        base,
        BuildParams {
            zone: p.zone,
            epoch_ring: block.epoch_ring,
            land_value_per_m2: p.land_value_per_m2.0,
            base_z_m,
            world_seed: input.plan.seed,
            building_index: i,
            margins,
        },
    );
    if derived.parts.is_empty() {
        return Err(Skip::TooSmall);
    }
    Ok(Planned {
        parcel: i,
        grammar: gid,
        base,
        base_z_m,
        teren: (lo, hi),
        derived,
        fallback: wybor.fallback,
        relaxed: wybor.relaxed,
    })
}

/// Niwelacja parceli i bryły budynku → komendy voxelowe.
fn queue_building(q: &EditQueue, p: &Planned) {
    let bed = p
        .derived
        .parts
        .first()
        .map_or(magnat_voxel::MaterialId::AIR, |x| x.material);
    let (lo, hi) = p.teren;
    // Podsypka sięga metr poniżej najniższego punktu terenu pod obrysem — dzięki temu
    // żaden voxel fundamentu nie graniczy z powietrzem od spodu (ryzyko R9).
    let glebokosc = (p.base_z_m - lo).max(0.0) + 1.0;
    let skarpa = (hi - p.base_z_m).max(0.0) + 0.5;
    voxels::level_strip(
        q,
        SRC_PARCEL_PAD,
        SRC_PARCEL_CUT,
        Vec2::new(p.base.center.x, p.base.center.y),
        p.base.u,
        p.base.half.x + 0.5,
        p.base.half.y + 0.5,
        p.base_z_m,
        glebokosc,
        skarpa,
        bed,
    );
    for part in &p.derived.parts {
        if part.carve {
            q.push(
                SRC_BUILDING_VOID,
                EditOp::Carve {
                    shape: CarveShape::Prism(voxels::obb(&part.scope)),
                },
            );
        } else {
            q.push(
                SRC_BUILDING,
                EditOp::Prism {
                    obb: voxels::obb(&part.scope),
                    material: part.material,
                },
            );
        }
    }
}

fn rogi(s: &Scope) -> [Vec2; 4] {
    let v = s.v();
    let c = Vec2::new(s.center.x, s.center.y);
    [
        c - s.u * s.half.x - v * s.half.y,
        c + s.u * s.half.x - v * s.half.y,
        c + s.u * s.half.x + v * s.half.y,
        c - s.u * s.half.x + v * s.half.y,
    ]
}

fn bryla(p: &Planned) -> Aabb3 {
    let mut a = Aabb3::EMPTY;
    for r in rogi(&p.base) {
        a = a.union_point(glam::Vec3::new(
            r.x,
            r.y,
            p.base_z_m - p.derived.foundation_depth_m,
        ));
        a = a.union_point(glam::Vec3::new(
            r.x,
            r.y,
            p.base_z_m + f32::from(p.derived.height_dm) / 10.0,
        ));
    }
    // Balkony, wykusze i lukarny wychodzą poza obrys. Bez nich `aabb` odcinałby detal
    // przy krawędzi kadru w M11 — a to jest jedyny odbiorca tego pola (kontrakt §6).
    for part in &p.derived.parts {
        if part.carve {
            continue;
        }
        let (dol, gora) = (
            part.scope.center.z - part.scope.half.z,
            part.scope.center.z + part.scope.half.z,
        );
        for r in rogi(&part.scope) {
            a = a.union_point(glam::Vec3::new(r.x, r.y, dol));
            a = a.union_point(glam::Vec3::new(r.x, r.y, gora));
        }
    }
    a
}

/// Stan techniczny: im starszy pierścień, tym gorszy, im droższy grunt, tym lepszy.
fn stan_techniczny(input: &BuildInput, p: &Planned, parcels: &ParcelSet) -> magnat_core::Q {
    let parcel = &parcels.parcels[p.parcel as usize];
    let ring = input.blocks.blocks[parcel.block.0 as usize].epoch_ring;
    let n = input.zones.rings.len().max(1) as f32;
    let wiek = 1.0 - f32::from(ring) / n;
    let mut r = rng(
        input.plan.seed,
        StreamId::Interiors,
        p.parcel,
        Tick(u64::MAX),
    );
    let szum = (r.next_u32() % 21) as i32 - 10;
    let bogactwo = (parcel.land_value_per_m2.0 / 2000).clamp(0, 20) as i32;
    let v = (88.0 - 42.0 * wiek) as i32 + bogactwo + szum;
    magnat_core::Q::new(v.clamp(5, 100) as u8)
}

/// Wnętrza logiczne: podział kondygnacji na lokale i obsadzenie ich stanowiskami.
/// Zwraca `(pierwszy_lokal, mieszkań, stanowisk)`.
fn interiors(
    input: &BuildInput,
    out: &mut BuildingSet,
    p: &Planned,
    id: BuildingId,
    parcels: &ParcelSet,
) -> (u32, u32, u32) {
    let g = input.grammars.get(p.grammar);
    let parcel = &parcels.parcels[p.parcel as usize];
    let block = &input.blocks.blocks[parcel.block.0 as usize];
    let epoch_key = input
        .zones
        .rings
        .get(usize::from(block.epoch_ring))
        .map_or("contemporary", |e| e.key.as_str());
    let epoch_mult = input.jobs.epoch_mult(epoch_key);
    let tier = input
        .districts
        .districts
        .get(parcel.district.0 as usize)
        .map_or(2, |d| d.income_tier);

    let od = out.units.len() as u32;
    let mut mieszkan = 0u32;
    let mut stanowisk = 0u32;
    let uzytkowa = p.base.area_m2() * (1.0 - g.interior.circulation_share.clamp(0.0, 0.6));
    let mut r = rng(input.plan.seed, StreamId::Interiors, p.parcel, Tick(0));

    // Piwnice: jeden lokal gospodarczy na kondygnację podziemną.
    for b in 0..p.derived.basements {
        out.units.push(Unit {
            building: id,
            floor: -(i32::from(b) + 1) as i8,
            kind: UnitKind::Storage,
            area_m2: uzytkowa.clamp(1.0, f32::from(u16::MAX)) as u16,
            occupant: UnitOccupant::Vacant,
            rent_hint: Money(0),
            workplaces: 0..0,
        });
    }

    for f in 0..p.derived.floors {
        let spec = if f == 0 {
            g.interior.ground
        } else if f + 1 == p.derived.floors {
            g.interior.top
        } else {
            g.interior.typical
        };
        let (lo, hi) = (f32::from(spec.area_m2.0.max(1)), f32::from(spec.area_m2.1));
        let cel = lo + (hi - lo) * ((r.next_u32() % 1000) as f32 / 1000.0);
        let n = (uzytkowa / cel.max(1.0)).round().max(1.0) as u32;
        let pole = (uzytkowa / n as f32).max(1.0);
        for _ in 0..n {
            let kind = rodzaj(spec.kind, pole, &mut r);
            let rent = czynsz(parcel.land_value_per_m2, pole, kind);
            let wp_od = out.workplaces.len() as u32;
            if let Some((role_id, role)) = input.jobs.role_for(kind) {
                let na_stanowisko = f32::from(role.m2_per_workplace.max(1));
                let ile = (pole / na_stanowisko).round().max(1.0) as u32;
                let band = wage_band(role, epoch_mult, tier);
                for _ in 0..ile {
                    out.workplaces.push(Workplace {
                        unit: UnitIdx(out.units.len() as u32),
                        site: None,
                        role: role_id,
                        shift: role.shift.into(),
                        wage_band: band,
                        occupant: None,
                    });
                }
                stanowisk += ile;
            }
            if kind.is_dwelling() {
                mieszkan += 1;
            }
            out.units.push(Unit {
                building: id,
                floor: f as i8,
                kind,
                area_m2: pole.clamp(1.0, f32::from(u16::MAX)) as u16,
                occupant: UnitOccupant::Vacant,
                rent_hint: rent,
                workplaces: wp_od..out.workplaces.len() as u32,
            });
        }
    }
    (od, mieszkan, stanowisk)
}

fn rodzaj(k: UnitClass, pole_m2: f32, r: &mut Rng) -> UnitKind {
    match k {
        UnitClass::Dwelling => {
            // Pokoje z metrażu, z jednym losowaniem na lokal: 28 m² na pokój plus kuchnia.
            let baza = (pole_m2 / 28.0).round().clamp(1.0, 7.0) as u8;
            let odchyl = u8::from(r.next_u32().is_multiple_of(4));
            UnitKind::Dwelling {
                rooms: (baza + odchyl).clamp(1, 8),
            }
        }
        UnitClass::Retail => UnitKind::Retail,
        UnitClass::Office => UnitKind::Office,
        UnitClass::Workshop => UnitKind::Workshop,
        UnitClass::Storage => UnitKind::Storage,
    }
}

/// `rent_hint`: podpowiedź startowa dla M5, nie cena. Proporcjonalna do wartości gruntu
/// i powierzchni; kalibracja bezwzględna należy do balansatora (M5), bo to on ma dane
/// o dochodach. Liczone w groszach, bez floata w akumulacji (00 §2).
fn czynsz(land_value_per_m2: Money, pole_m2: f32, kind: UnitKind) -> Money {
    let mnoznik = match kind {
        UnitKind::Retail => 14,
        UnitKind::Office => 11,
        UnitKind::Workshop => 5,
        UnitKind::Storage => 3,
        UnitKind::Dwelling { .. } => 8,
        UnitKind::Common => 0,
    };
    let pole = pole_m2.clamp(0.0, 1.0e6) as i64;
    Money(
        land_value_per_m2
            .0
            .saturating_mul(pole)
            .saturating_mul(mnoznik),
    )
    .div_round_half_up(1000)
}

/// Odległość od środka bryły do jej lica w zadanym kierunku — wyjście promienia
/// z prostopadłościanu w układzie osi budynku.
///
/// Wcześniej stało tu `half.y` niezależnie od kierunku, co dla hali 200 × 40 m stawiało
/// drzwi kilkadziesiąt metrów **wewnątrz** bryły albo **poza** nią, zależnie od tego,
/// z której strony leżała droga. Widać to było dopiero w teście T3, jako wejście bez
/// drogi w promieniu 50 m.
fn wyjscie_z_bryly(base: &Scope, kier: Vec2) -> f32 {
    let v = base.v();
    let (du, dv) = (kier.dot(base.u).abs(), kier.dot(v).abs());
    let tu = if du > 1e-4 { base.half.x / du } else { f32::MAX };
    let tv = if dv > 1e-4 { base.half.y / dv } else { f32::MAX };
    tu.min(tv)
}

/// Wejścia do budynku. Główne przy froncie; rampa — korekta D7 — szukana wśród
/// **wszystkich dróg w sąsiedztwie**, nie tylko na odcinku frontowym.
fn wejscia(input: &BuildInput, parcels: &ParcelSet, p: &Planned) -> SmallVec<[Entrance; 4]> {
    let mut out: SmallVec<[Entrance; 4]> = SmallVec::new();
    let parcel = &parcels.parcels[p.parcel as usize];
    let srodek = Vec2::new(p.base.center.x, p.base.center.y);
    let z = p.base_z_m;

    if !parcel.frontage.is_none() {
        let t = (parcel.frontage.t0 + parcel.frontage.t1) * 0.5;
        let seg = &input.roads.segments[parcel.frontage.seg.0 as usize];
        let (a, b) = (
            input.roads.nodes[seg.a.0 as usize].pos,
            input.roads.nodes[seg.b.0 as usize].pos,
        );
        let na_ulicy = a + (b - a) * t;
        // Punkt na licu budynku od strony ulicy — tam, gdzie faktycznie są drzwi.
        let kier = (na_ulicy - srodek).normalize_or_zero();
        let lico = srodek + kier * wyjscie_z_bryly(&p.base, kier);
        out.push(Entrance {
            pos: glam::Vec3::new(lico.x, lico.y, z),
            seg: parcel.frontage.seg,
            t,
            kind: EntranceKind::Main,
        });
    }

    if matches!(
        parcel.zone,
        ZoneKind::Logistics | ZoneKind::IndustryHeavy | ZoneKind::IndustryLight
    ) {
        // Dwa promienie, nie jeden: hala w głębi strefy przemysłowej bywa otoczona
        // wyłącznie ulicami z zakazem ruchu ciężkiego w zasięgu 60 m, a droga dojazdowa
        // biegnie 150 m dalej. Zmierzone: przy jednym promieniu 12,7 % budynków stref
        // ciężkich zostawało bez rampy, przy dwóch — poniżej 2 % (korekta I-9).
        let bazowy = p.base.half.x.max(p.base.half.y);
        let mut najlepszy: Option<(i64, SegmentId, Vec2, f32)> = None;
        let mut kandydaci: Vec<SegmentId> = Vec::new();
        for promien in [bazowy + 60.0, bazowy + 220.0] {
        kandydaci.clear();
        parcels
            .street_index
            .query_radius(srodek, promien, &mut kandydaci);
        kandydaci.sort_unstable();
        kandydaci.dedup();
        for s in &kandydaci {
            let s = *s;
            let seg = &input.roads.segments[s.0 as usize];
            if seg.flags.contains(RoadFlags::NO_HEAVY)
                || seg.class.is_rail()
                || matches!(seg.class, RoadClass::Pedestrian)
            {
                continue;
            }
            let (a, b) = (
                input.roads.nodes[seg.a.0 as usize].pos,
                input.roads.nodes[seg.b.0 as usize].pos,
            );
            let (punkt, t) = poly::closest_on_segment(a, b, srodek);
            // Klucz całkowitoliczbowy: odległość w centymetrach kwadratowych, remis
            // po numerze segmentu. Bez tego remis rozstrzygałaby kolejność w indeksie.
            let d = (punkt - srodek).length_squared();
            let klucz = (f64::from(d) * 100.0) as i64;
            if najlepszy.is_none_or(|(k, ids, _, _)| (klucz, s) < (k, ids)) {
                najlepszy = Some((klucz, s, punkt, t));
            }
        }
        if najlepszy.is_some() {
            break;
        }
        }
        if let Some((_, s, punkt, t)) = najlepszy {
            let kier = (punkt - srodek).normalize_or_zero();
            let lico = srodek + kier * wyjscie_z_bryly(&p.base, kier);
            out.push(Entrance {
                pos: glam::Vec3::new(lico.x, lico.y, z),
                seg: s,
                t,
                kind: EntranceKind::Ramp,
            });
        }
    }
    out
}

/// Przelicza `District.pop_capacity` z **faktycznej liczby mieszkań** (korekta C9 z M2c).
///
/// M2c liczył tę wartość z gęstości strefy w osobach na hektar, bo lokali jeszcze nie było,
/// i zostawił ją do przeliczenia tutaj. Wielkość gospodarstwa domowego jest na razie stałą
/// z budżetu M2 §7 („167 000 mieszkań × 2,4 os."); model demograficzny należy do M3
/// i on tę liczbę zastąpi rozkładem, a nie średnią.
pub fn recompute_pop_capacity(districts: &mut DistrictSet, parcels: &ParcelSet, set: &BuildingSet) {
    let mut mieszkan = vec![0u32; districts.districts.len()];
    for u in set.units.iter().filter(|u| u.kind.is_dwelling()) {
        let b = &set.buildings[u.building.0.index() as usize];
        let d = parcels.parcels[b.parcel.0.index() as usize].district.0 as usize;
        if d < mieszkan.len() {
            mieszkan[d] += 1;
        }
    }
    for (d, n) in districts.districts.iter_mut().zip(&mieszkan) {
        d.pop_capacity = (*n as f32 * OSOB_NA_MIESZKANIE) as u32;
    }
}


/// Osób na mieszkanie w epoce startowej. Wejście dla M3, nie prawda o demografii.
pub const OSOB_NA_MIESZKANIE: f32 = 2.4;

/// Etatów na mieszkańca — z budżetu §7 fazy: 212,5 tys. stanowisk na 400 tys. ludzi.
/// To jest liczba, wobec której mierzy się drugą połowę testu T10.
pub const ETATOW_NA_MIESZKANCA: f32 = 0.531_25;

/// Dolna i górna granica zagęszczenia mieszkań. Poza nimi kalibracja przestaje być
/// kalibracją, a zaczyna produkować kawalerki 30-metrowe albo apartamenty w blokowisku.
pub const GESTOSC_MIN: f32 = 0.55;
pub const GESTOSC_MAX: f32 = 3.20;

/// Widełki przelicznika „m² lokalu na stanowisko" przy kalibracji drugiej połowy T10.
/// Szersze niż mieszkaniowe i to jest uzasadnione: 0,30 × przelicznik bazowy daje biuro
/// o 8 m² na urzędnika, czyli dokładnie to, czym było biuro w 1990 roku, a 3,0 — halę,
/// w której pracuje trzech ludzi przy dwóch maszynach. Obie skrajności istnieją.
///
/// Głębsza przyczyna rozrzutu zostaje nienaprawiona i jest tego świadoma: stanowiska
/// liczą się z **powierzchni lokalu**, więc gospodarstwo rolne zatrudnia tylu ludzi,
/// ilu mieści się w chałupie, a nie ilu potrzeba na dwudziestu hektarach. Praca na roli
/// i w wyrobisku dzieje się poza budynkiem i jej model należy do rynku pracy (M7).
pub const ETATY_SCALE_MIN: f32 = 0.18;
pub const ETATY_SCALE_MAX: f32 = 3.00;

/// **Kalibracja pojemności mieszkaniowej do `target_pop`** (test T10, korekta I-6).
///
/// Dlaczego to w ogóle istnieje. §9.1/7 zobowiązuje M2 do zagwarantowania pojemności
/// w widełkach T10, a M3 ma populację tylko skalować, nie dostawiać budynków. Tymczasem
/// liczba mieszkań wychodząca z Etapu 6 zależy od tego, ile płaskiego, suchego terenu
/// da generator M1 — a to zmienia się z ziarnem, nie z planem. Pomiar na 12 światach
/// (3 ziarna × 4 rozmiary, `lowland`) dał rozrzut **0,61–1,08** wobec `target_pop`.
/// Żadne ustawienie `max_depth_m` ani powierzchni lokali nie zamknie tego w oknie
/// szerokim na 11 punktów procentowych, bo rozrzut nie bierze się z parametrów.
///
/// Co robi ta funkcja. Powierzchnia mieszkalna budynku zostaje bez zmian; zmienia się
/// **podział na lokale**: przy niedoborze mieszkań te same metry dzielą się na więcej
/// mniejszych, przy nadmiarze — na mniej większych. To jest zależność prawdziwa również
/// w rzeczywistości (presja mieszkaniowa przekłada się na metraż), a sylwetka miasta,
/// za którą odpowiada gramatyka, nie drgnie.
///
/// Zwraca zastosowany współczynnik; 1,0 znaczy „nie było czego kalibrować".
pub fn rescale_dwellings(set: &mut BuildingSet, target_pop: u32) -> f32 {
    let jest = set.units.iter().filter(|u| u.kind.is_dwelling()).count() as f32;
    if jest < 1.0 {
        return 1.0;
    }
    // Celujemy w środek okna T10 (0,97–1,08), nie w jego brzeg.
    let cel = target_pop as f32 * 1.025 / OSOB_NA_MIESZKANIE;
    let k = (cel / jest).clamp(GESTOSC_MIN, GESTOSC_MAX);
    if (k - 1.0).abs() < 0.02 {
        return 1.0;
    }

    let mut nowe: Vec<Unit> = Vec::with_capacity(set.units.len());
    let mut mieszkan = 0u32;
    for b in &mut set.buildings {
        let od = nowe.len() as u32;
        let stare = &set.units[b.units.start as usize..b.units.end as usize];
        let mut i = 0usize;
        while i < stare.len() {
            // Mieszkania jednej kondygnacji idą w tablicy obok siebie — dzielimy je
            // grupami, bo to na kondygnacji jest do rozdania konkretny metraż.
            if !stare[i].kind.is_dwelling() {
                nowe.push(stare[i].clone());
                i += 1;
                continue;
            }
            let pietro = stare[i].floor;
            let mut j = i;
            let mut pole = 0f32;
            while j < stare.len() && stare[j].kind.is_dwelling() && stare[j].floor == pietro {
                pole += f32::from(stare[j].area_m2);
                j += 1;
            }
            let n = (((j - i) as f32) * k).round().max(1.0) as u32;
            let na_lokal = (pole / n as f32).max(12.0);
            let wzor = &stare[i];
            for _ in 0..n {
                nowe.push(Unit {
                    building: wzor.building,
                    floor: pietro,
                    kind: UnitKind::Dwelling {
                        rooms: (na_lokal / 28.0).round().clamp(1.0, 8.0) as u8,
                    },
                    area_m2: na_lokal.clamp(1.0, f32::from(u16::MAX)) as u16,
                    occupant: UnitOccupant::Vacant,
                    // Czynsz jest proporcjonalny do metrażu, więc skaluje się razem z nim.
                    rent_hint: wzor
                        .rent_hint
                        .mul_ratio(i64::from(na_lokal as u16), i64::from(wzor.area_m2.max(1))),
                    workplaces: 0..0,
                });
                mieszkan += 1;
            }
            i = j;
        }
        b.units = od..nowe.len() as u32;
    }
    set.units = nowe;
    set.report.units = set.units.len() as u32;
    set.report.dwellings = mieszkan;
    k
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::grammar::Massing;

    fn kwadrat(bok: f32) -> Vec<Vec2> {
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(bok, 0.0),
            Vec2::new(bok, bok),
            Vec2::new(0.0, bok),
        ]
    }

    fn massing(front: f32, bok: f32, tyl: f32) -> Massing {
        Massing {
            setback_front_m: front,
            setback_side_m: bok,
            setback_back_m: tyl,
            courtyard_min_m2: 0.0,
            max_depth_m: 100.0,
        }
    }

    #[test]
    fn obrys_miesci_sie_w_dzialce() {
        // Test T6 w miniaturze: wszystkie punkty kontrolne bryły leżą w wielokącie.
        let dzialka = kwadrat(30.0);
        let s = footprint_for(&dzialka, None, None, &massing(3.0, 2.0, 3.0)).expect("obrys");
        for r in rogi(&s) {
            assert!(poly::contains(&dzialka, r), "róg {r:?} poza działką");
        }
    }

    #[test]
    fn cofniecie_frontowe_idzie_od_ulicy() {
        // Ulica na dole (y = −5): budynek ma stać bliżej góry, nie odwrotnie.
        let dzialka = kwadrat(40.0);
        let s = footprint_for(
            &dzialka,
            Some(Vec2::new(1.0, 0.0)),
            Some(Vec2::new(20.0, -5.0)),
            &massing(12.0, 2.0, 2.0),
        )
        .expect("obrys");
        assert!(
            s.center.y > 20.0,
            "budynek stanął przy ulicy mimo cofnięcia 12 m (y = {})",
            s.center.y
        );
    }

    #[test]
    fn front_zawsze_patrzy_na_ulice() {
        // `Face::Front` w gramatyce to lico od ulicy. Kierunek odcinka drogi nie mówi,
        // po której stronie leży działka, więc bez obrotu osi witryna parteru wypadała
        // na podwórzu mniej więcej co drugi budynek — i żaden test tego nie widział.
        let dzialka = kwadrat(40.0);
        for ulica in [Vec2::new(20.0, -5.0), Vec2::new(20.0, 45.0)] {
            for kierunek in [Vec2::new(1.0, 0.0), Vec2::new(-1.0, 0.0)] {
                let s = footprint_for(&dzialka, Some(kierunek), Some(ulica), &massing(2.0, 2.0, 2.0))
                    .expect("obrys");
                let v = s.v();
                let do_ulicy = ulica - Vec2::new(s.center.x, s.center.y);
                assert!(
                    do_ulicy.dot(v) < 0.0,
                    "ulica {ulica:?} wypadła po dodatniej stronie osi v (kierunek {kierunek:?})"
                );
            }
        }
    }

    #[test]
    fn zapas_konczy_sie_na_granicy_dzialki() {
        // Bryła 20 × 20 pośrodku działki 30 × 30 ma po 5 m luzu z każdej strony,
        // ale `Protrude` i tak nie sięgnie dalej niż `MAX_PROTRUDE_M`.
        let dzialka = kwadrat(30.0);
        let c = Vec2::new(15.0, 15.0);
        let (u, v) = (Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0));
        for dir in [Axis2::Front, Axis2::Back, Axis2::Side] {
            let z = zapas(&dzialka, c, u, v, 10.0, 10.0, dir);
            assert!(
                (z - MAX_PROTRUDE_M).abs() < ZAPAS_KROK_M,
                "{dir:?}: zapas {z} m przy 5 m luzu i limicie {MAX_PROTRUDE_M} m"
            );
        }
        // Bryła wypełniająca działkę nie ma zapasu w żadną stronę.
        for dir in [Axis2::Front, Axis2::Back, Axis2::Side] {
            assert_eq!(zapas(&dzialka, c, u, v, 15.0, 15.0, dir), 0.0, "{dir:?}");
        }
    }

    #[test]
    fn za_mala_dzialka_nie_dostaje_budynku() {
        assert!(footprint_for(&kwadrat(6.0), None, None, &massing(3.0, 3.0, 3.0)).is_none());
    }

    /// Złożenie całej ścieżki przycinania: obrys z `footprint_for`, zapas z `zapas`,
    /// derywacja z katalogu z repo. To jest „rozszerzony T6" z kryterium WP18 —
    /// osobno przetestowane są obie połowy, ale dopiero tu widać, czy się składają.
    #[test]
    fn zadne_wysuniecie_nie_wychodzi_z_dzialki_ponizej_skrajni() {
        use crate::city::derive::{Margins, OVERHANG_MIN_Z_M};
        use crate::city::grammar::GrammarSet;

        let mats = magnat_voxel::MaterialRegistry::load_dir(&crate::assets::data_path("materials"))
            .expect("materiały");
        let set =
            GrammarSet::load_dir(&crate::assets::data_path("grammar"), &mats).expect("gramatyki");

        // Działka 24 × 40 z ulicą pod spodem. Prostokąt jest wypukły, więc ośmiopunktowy
        // test przynależności w `zapas` jest tu dokładny, a nie przybliżony.
        let dzialka = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(24.0, 0.0),
            Vec2::new(24.0, 40.0),
            Vec2::new(0.0, 40.0),
        ];
        let kierunek = Vec2::new(1.0, 0.0);
        let ulica = Vec2::new(12.0, -4.0);
        let base_z = 50.0;

        let mut widziane = 0;
        for g in set.all() {
            let Some(base) = footprint_for(&dzialka, Some(kierunek), Some(ulica), &g.massing)
            else {
                continue;
            };
            let c = Vec2::new(base.center.x, base.center.y);
            let v = base.v();
            let margins = Margins {
                front_m: zapas(&dzialka, c, base.u, v, base.half.x, base.half.y, Axis2::Front),
                back_m: zapas(&dzialka, c, base.u, v, base.half.x, base.half.y, Axis2::Back),
                side_m: zapas(&dzialka, c, base.u, v, base.half.x, base.half.y, Axis2::Side),
                overhang_front_m: OVERHANG_M,
            };
            let out = derive::derive(
                g,
                &mats,
                base,
                BuildParams {
                    zone: ZoneKind::Residential(super::super::zoning::ResDensity::R3),
                    epoch_ring: 1,
                    land_value_per_m2: (g.applies.land_value.0 + g.applies.land_value.1) / 2,
                    base_z_m: base_z,
                    world_seed: 0x00C0_FFEE,
                    building_index: 3,
                    margins,
                },
            );
            widziane += out.protrusions;
            for part in out.parts.iter().filter(|p| !p.carve) {
                let dol = part.scope.center.z - part.scope.half.z;
                for r in rogi(&part.scope) {
                    if poly::contains(&dzialka, r) {
                        continue;
                    }
                    assert!(
                        dol >= base_z + OVERHANG_MIN_Z_M - 1e-3,
                        "gramatyka {}: bryła poza działką na wysokości {:.2} m nad posadowieniem",
                        g.id,
                        dol - base_z
                    );
                    let d = odleglosc_do_wielokata(&dzialka, r);
                    assert!(
                        d <= OVERHANG_M + 0.05,
                        "gramatyka {}: wysięg {d:.2} m poza działkę przy limicie {OVERHANG_M} m",
                        g.id
                    );
                }
            }
        }
        assert!(
            widziane > 0,
            "żadna gramatyka z katalogu nie postawiła wysunięcia — test nie bada niczego"
        );
    }

    fn odleglosc_do_wielokata(pts: &[Vec2], p: Vec2) -> f32 {
        let mut d = f32::MAX;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let (q, _) = poly::closest_on_segment(a, b, p);
            d = d.min((q - p).length());
        }
        d
    }

    #[test]
    fn widelki_rosna_z_epoka_i_zamoznoscia() {
        let jobs = JobTable::load().expect("data/jobs/roles.ron");
        let role = &jobs.roles[0];
        let biedna = wage_band(role, jobs.epoch_mult("postwar"), 0);
        let bogata = wage_band(role, jobs.epoch_mult("contemporary"), 4);
        assert!(bogata.median.0 > biedna.median.0);
        assert!(biedna.min.0 < biedna.median.0 && biedna.median.0 < biedna.max.0);
    }

    #[test]
    fn czynsz_rosnie_z_wartoscia_gruntu() {
        let tanio = czynsz(Money(10_000), 50.0, UnitKind::Dwelling { rooms: 2 });
        let drogo = czynsz(Money(40_000), 50.0, UnitKind::Dwelling { rooms: 2 });
        assert!(drogo.0 > tanio.0 && tanio.0 > 0);
    }
}
