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

mod capacity;
mod footprint;
mod interiors;
mod model;

pub use capacity::{
    recompute_pop_capacity, rescale_dwellings, ETATOW_NA_MIESZKANCA, ETATY_SCALE_MAX,
    ETATY_SCALE_MIN, GESTOSC_MAX, GESTOSC_MIN, OSOB_NA_MIESZKANIE,
};
pub use footprint::{footprint_for, rozpietosc};
pub use model::{
    building_id, wage_band, BuildReport, Building, BuildingSet, BuildingSignature, Entrance,
    EntranceKind, JobRole, JobTable, ShiftId, ShiftKey, Unit, UnitIdx, UnitKind, UnitOccupant,
    WageBand, Workplace, JOBS_SCHEMA_VERSION, MIN_BUDYNKOW_DZIELNICY, PROMIEN_SASIEDZTWA_M,
};

use footprint::{bryla, rogi, wejscia, zapas, Axis2, MIN_FOOTPRINT_M2, MIN_FRONT_M, OVERHANG_M};
use interiors::{interiors, stan_techniczny};

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
            let (g, ile) =
                gr.iter().enumerate().fold(
                    (0usize, 0u32),
                    |b, (i, c)| {
                        if *c > b.1 {
                            (i, *c)
                        } else {
                            b
                        }
                    },
                );
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
        overhang_front_m: if p.frontage.is_none() {
            0.0
        } else {
            OVERHANG_M
        },
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
