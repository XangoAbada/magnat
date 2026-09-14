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
use super::derive::{self, BuildParams, Derived, Scope};
use super::districts::DistrictSet;
use super::grammar::{BuildingGrammar, GrammarId, GrammarSet, UnitClass};
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
    /// Derywacje przerwane limitem węzłów albo głębokości (M2 §5.6: liczone, nie ciche).
    pub truncated: u32,
    /// Budynki w strefie `Logistics`/`IndustryHeavy` bez drogi bez `NO_HEAVY` w zasięgu
    /// (korekta D7) — rampa nie powstała i ma to być widać.
    pub ramp_missing: u32,
    pub edit_commands: u32,
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
    let u = match front_dir {
        Some(d) if d.length_squared() > 0.25 => d.normalize(),
        _ => obb.axis,
    };
    let v = Vec2::new(-u.y, u.x);
    let srodek = poly::centroid(obrys);

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

    // Po której stronie osi `v` jest ulica. Bez tego cofnięcie frontowe wypadałoby
    // w połowie przypadków od podwórza i budynek stałby tyłem do ulicy.
    let front_na_minusie = front_point.is_none_or(|fp| (fp - srodek).dot(v) < 0.0);
    let (cof_min, cof_max) = if front_na_minusie {
        (massing.setback_front_m, massing.setback_back_m)
    } else {
        (massing.setback_back_m, massing.setback_front_m)
    };
    let mut a = v0 + cof_min;
    let mut b = v1 - cof_max;
    // Trakt: budynek nie rozlewa się na całą głębokość działki.
    if b - a > massing.max_depth_m {
        if front_na_minusie {
            b = a + massing.max_depth_m;
        } else {
            a = b - massing.max_depth_m;
        }
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
    let epoka = matches!(relax, Relax::None)
        && !(a.epochs.is_empty() || a.epochs.iter().any(|e| e == c.epoch_key));
    let styl = matches!(relax, Relax::None | Relax::Epoch)
        && !(a.styles.is_empty() || a.styles.iter().any(|s| s == c.district_key));
    let wartosc = !(matches!(relax, Relax::EpochStyleValue)
        || (c.land_value >= a.land_value.0 && c.land_value <= a.land_value.1));
    (a.zones.is_empty() || a.zones.iter().any(|z| z == c.zone.key()))
        && !epoka
        && !styl
        && !wartosc
        && (c.front_m >= a.frontage_m.0 && c.front_m <= a.frontage_m.1)
        && (c.depth_m >= a.depth_m.0 && c.depth_m <= a.depth_m.1)
}

/// Wynik doboru gramatyki.
struct Picked {
    grammar: Option<GrammarId>,
    /// Gramatyka awaryjna — kryterium §5.6 mówi „< 2%", więc jest liczona osobno.
    fallback: bool,
    /// Trafienie dopiero po rozluźnieniu filtru. Nie jest błędem (miasto z 1990 ma
    /// kwartały z pięciu epok, a katalog gramatyk nie pokrywa każdej kombinacji),
    /// ale ma być widoczne w raporcie — inaczej luka w danych nigdy nie wyjdzie.
    relaxed: bool,
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
                        relaxed: false,
                    };
                }
                return Picked {
                    grammar: Some(GrammarId(i as u16)),
                    fallback: false,
                    relaxed: !matches!(relax, Relax::None),
                };
            }
            los -= w;
        }
    }
    Picked {
        grammar: set.fallback_for(c.zone),
        fallback: set.fallback_for(c.zone).is_some(),
        relaxed: false,
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
    relaxed: bool,
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
        let id = building_id(out.buildings.len() as u32);
        if p.fallback {
            out.report.fallback += 1;
            out.report.fallback_zone[parcels.parcels[p.parcel as usize].zone.index()] += 1;
        }
        if p.relaxed {
            out.report.relaxed += 1;
        }
        if p.derived.truncated {
            out.report.truncated += 1;
        }
        queue_building(q, &p);
        let (units_from, dwell, wp) = interiors(input, &mut out, &p, id, parcels);
        let footprint = geom.push(&rogi(&p.base));
        let aabb = bryla(&p);
        let entrances = wejscia(input, parcels, &p);
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
            condition: stan_techniczny(input, &p, parcels),
            aabb,
            entrances,
        });
        srodki.push((Vec2::new(p.base.center.x, p.base.center.y), id));
        let parcel = &mut parcels.parcels[p.parcel as usize];
        parcel.status = ParcelStatus::Built;
        parcel.building = Some(id);
        out.report.dwellings += dwell;
        out.report.workplaces += wp;
    }

    out.report.buildings = out.buildings.len() as u32;
    out.report.units = out.units.len() as u32;
    out.report.edit_commands = (q.len() - przed_komend) as u32;
    out.index = CsrGrid::build(*out.index.spec(), srodki.into_iter());
    out
}

/// Część równoległa — **czysta**: czyta arenę i teren, niczego nie mutuje.
fn plan_building(
    input: &BuildInput,
    geom: &PolyArena,
    parcels: &ParcelSet,
    i: u32,
) -> Result<Planned, Skip> {
    let p = &parcels.parcels[i as usize];
    // Strefy bez zabudowy w M2d: woda, teren wyłączony, zieleń i wydobycie. Dwie ostatnie
    // dostają obiekty dopiero w Etapie 7 (M2e §5.8 pkt 5) — park zabudowany gramatyką
    // awaryjną przestaje być parkiem.
    if !p.zone.parcelled()
        || matches!(
            p.zone,
            ZoneKind::Water | ZoneKind::Undevelopable | ZoneKind::Green | ZoneKind::Extraction
        )
    {
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
    let wybor = pick_grammar(input.grammars, &ctx, &mut r);
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
        let lico = srodek + kier * p.base.half.y;
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
        let promien = p.base.half.x.max(p.base.half.y) + 60.0;
        let mut kandydaci: Vec<SegmentId> = Vec::new();
        parcels
            .street_index
            .query_radius(srodek, promien, &mut kandydaci);
        kandydaci.sort_unstable();
        kandydaci.dedup();
        let mut najlepszy: Option<(i64, SegmentId, Vec2, f32)> = None;
        for s in kandydaci {
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
        if let Some((_, s, punkt, t)) = najlepszy {
            let kier = (punkt - srodek).normalize_or_zero();
            let lico = srodek + kier * p.base.half.y;
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
    /// Osób na mieszkanie, epoka startowa. Wejście dla M3, nie prawda o demografii.
    const OSOB_NA_MIESZKANIE: f32 = 2.4;
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
    fn za_mala_dzialka_nie_dostaje_budynku() {
        assert!(footprint_for(&kwadrat(6.0), None, None, &massing(3.0, 3.0, 3.0)).is_none());
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
