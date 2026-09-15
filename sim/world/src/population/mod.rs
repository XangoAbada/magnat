//! Generacja populacji — Etap 8 (M3d §5.9, WP10, PRD §4.2).
//!
//! **To jest most między miastem a mieszkańcami.** `sim/agents` nie zna budynków,
//! zakładów ani geometrii ulic (decyzja 9.11: zależność idzie w drugą stronę), więc
//! wszystko, przez co agenci widzą miasto, powstaje tutaj: katalog miejsc (`PlaceTable`),
//! pula lokali i etatów (`Vacancies`), fakty o mieście dla funkcji statusu (`CityFacts`)
//! i sieć piesza dla `WalkOracle` (korekty E-1, E-7, E-14).
//!
//! **Drugiego generatora gospodarstw tu nie ma.** Zasiedlanie idzie przez
//! `migration::spawn_household_aged` — ten sam kod, którym miasto przyjmuje napływ
//! migracyjny (korekta E-13). Etap 8 nadpisuje nad nim to, czego tamten nie umie:
//! piramidę wieku epoki, wykształcenie, osobowość, dopasowanie pracy i mieszkania.
//! Drugi generator obok rozjechałby się z pierwszym przy pierwszej zmianie w `Household`.
//!
//! Dziesięć kroków z §5.9 w kolejności: piramida → gospodarstwa → wykształcenie →
//! osobowość → praca → mieszkania → dojazd → wiedza → relacje → weryfikacja.
//!
//! Kroki 6 i 7 pracują na **lustrze** stanu (`Pracownik`), a nie na
//! świecie: pętla poprawkowa wykonuje 200 tys. prób zamiany, a każda z nich musi
//! kosztować kilka odczytów tablicy, nie przejście po archetypach ECS. Do świata
//! wraca dopiero wynik.

pub mod table;

mod catalog;
mod homes;
mod jobs;
mod pyramid;
mod report;
mod seeding;
mod traits;

use crate::city::build::{ShiftId, UnitKind};
use crate::city::sites::SectorId;
use crate::city::CityData;
use crate::traffic_build::FleetReport;
use catalog::{fakty_miasta, katalog_miejsc, pustostany, wakaty};
use homes::dopasuj_mieszkania;
use jobs::{dopasuj_prace, przypisz_szkoly};
use magnat_agents::{
    demography, household, migration, social, Ages, ArrayVec, CitizenView, CityFacts,
    DemographyTable, Employment, HomeSlot, Household, HouseholdOverflow, Identity, JobSlot,
    KnowledgeKind, Needs, Personality, PlaceEntry, PlaceTable, RelationKind, Residence, ShiftKind,
    SkillSlot, Skills, Vacancies, Vitals, MAX_ON_ROUTE,
};
use magnat_core::{
    det_math, rng, BuildingId, CitizenId, Entity, Money, PlaceKind, PlaceRef, Rng, SiteId,
    StreamId, Tick, WorldCoord,
};
use magnat_ecs::World;
use magnat_traffic::{OracleHandle, TrafficOracle};
use pyramid::{docelowa_populacja, piramida, sklady, zasiedl};
use report::{raport, Liczby};
use seeding::{relacje_startowe, zasiej_wiedze};
use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use table::{PopulationTable, TableError, AGE_BANDS, BAND_YEARS};
use traits::nadaj_cechy;

pub use report::PopulationReport;

/// Zakłady zaczynają numerację kluczy miejsc od tej wartości.
///
/// Kluczem wiedzy (`Knowledge.target`) jest **sam indeks encji**, bez rodzaju miejsca —
/// więc budynek nr 7 i zakład nr 7 byłyby dla magazynu wiedzy tym samym miejscem.
/// Przesunięcie rozdziela obie przestrzenie raz na zawsze: 16,7 mln to dwa rzędy
/// wielkości ponad liczbę budynków metropolii (19 tys.), a `CityFacts.block_of`
/// indeksuje się dalej samym indeksem budynku, bez dziury na 16 mln pozycji.
pub const SITE_KEY_BASE: u32 = 1 << 24;

/// Ile kubełków ma histogram czasu dojazdu (raport i test χ²).
pub const COMMUTE_BINS: usize = 12;
/// Szerokość kubełka w minutach; ostatni jest otwarty.
pub const COMMUTE_BIN_MIN: u16 = 5;

#[inline]
#[must_use]
fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("1 != 0"))
}

/// `PlaceRef` domu mieszkańca z `Residence.building`.
#[inline]
#[must_use]
pub fn home_place(r: &Residence) -> Option<PlaceRef> {
    (r.building != Residence::HOMELESS).then(|| PlaceRef::Building(BuildingId(encja(r.building))))
}

/// `PlaceRef` zakładu z `Employment.site` (klucz już przesunięty o [`SITE_KEY_BASE`]).
#[inline]
#[must_use]
pub fn site_place(site_key: u32) -> Option<PlaceRef> {
    (site_key != Employment::NO_SITE).then(|| PlaceRef::Site(SiteId(encja(site_key))))
}

/// Parametry Etapu 8. `None` znaczy „wylicz z miasta i z danych" — i to jest tryb
/// domyślny, bo liczba mieszkańców **wynika** z liczby etatów i lokali, a nie odwrotnie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PopulationParams {
    /// Docelowa liczba mieszkańców; `None` = z pojemności miasta.
    pub target_population: Option<u32>,
    /// Docelowe bezrobocie w promilach; `None` = z `population.ron`.
    pub unemployment_target_permille: Option<u16>,
    /// Docelowa mediana czasu dojazdu w minutach; `None` = z `population.ron`.
    pub commute_median_min: Option<u16>,
    /// Ile prób zamiany mieszkań wykonać w kroku 7; `None` = 200 tys. (§5.9).
    pub commute_swaps: Option<u32>,
}

/// Miasto zaludnione: to, czego `sim/agents` potrzebuje, żeby zacząć dobę.
///
/// Po M4b nie ma tu już dwóch płaskich tablic sieci pieszej (`Z-3`): graf buduje
/// `nav_build` z `RoadNetwork` bezpośrednio, a estymator podróży jest crate'em ruchu.
pub struct Populated {
    /// Katalog miejsc dla `InfinitePlaces` (korekta E-1).
    pub places: Arc<PlaceTable>,
    /// Oracle ruchu — router, flota i stacje tego miasta. Ten sam `Arc` siedzi
    /// w zasobie `TrafficServices` świata, więc system ruchu i planer widzą
    /// dokładnie jeden obiekt.
    pub traffic: Arc<TrafficOracle>,
    pub fleet: FleetReport,
    pub report: PopulationReport,
}

impl Populated {
    /// Estymator podróży do `Sources.travel` — wykonanie `Z-1`.
    #[must_use]
    pub fn travel_oracle(&self) -> Box<dyn magnat_agents::TravelOracle> {
        Box::new(OracleHandle(self.traffic.clone()))
    }
}

#[derive(Debug)]
pub enum PopulationError {
    Table(TableError),
    /// Sieć transportowa miasta nie dała się zbudować albo dane ruchu są niespójne.
    Traffic(Box<crate::traffic_build::TrafficBuildError>),
    /// Miasto bez ani jednego mieszkania nie da się zaludnić — i to jest błąd Etapu 6,
    /// a nie stan do obsłużenia tutaj.
    NoHomes,
}

impl std::fmt::Display for PopulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PopulationError::Table(e) => write!(f, "{e}"),
            PopulationError::Traffic(e) => write!(f, "sieć transportowa: {e}"),
            PopulationError::NoHomes => write!(f, "miasto nie ma ani jednego mieszkania"),
        }
    }
}

impl std::error::Error for PopulationError {}

impl From<TableError> for PopulationError {
    fn from(e: TableError) -> PopulationError {
        PopulationError::Table(e)
    }
}

// ── generacja ───────────────────────────────────────────────────────────────────

/// Etap 8 w całości (§5.9).
///
/// Świat musi mieć zarejestrowane komponenty M3a i zasoby M3c
/// (`magnat_agents::register` + `society::register_society`) — Etap 8 zaludnia świat,
/// a nie buduje go od zera.
#[allow(clippy::too_many_lines)]
pub fn generate_population(
    world: &mut World,
    city: &CityData,
    params: &PopulationParams,
) -> Result<Populated, PopulationError> {
    let mut zegar = std::time::Instant::now();
    let mut czasy: Vec<(&'static str, f32)> = Vec::new();
    let odcinek =
        |nazwa: &'static str, czasy: &mut Vec<(&'static str, f32)>, z: &mut std::time::Instant| {
            czasy.push((nazwa, z.elapsed().as_secs_f32()));
            *z = std::time::Instant::now();
        };

    let t = PopulationTable::load()?;
    let jobs_table = crate::city::build::JobTable::load()
        .map_err(|e| TableError::Missing(format!("data/jobs/roles.ron ({e})")))?;
    let epoch = klucz_epoki(city);
    let seed = world.seed;

    // ── krok 0: most ────────────────────────────────────────────────────────────
    let places = Arc::new(PlaceTable::build(katalog_miejsc(city)));
    crate::traffic_build::register_vehicle_components(world);
    *world.resource_mut::<CityFacts>() = fakty_miasta(city, &jobs_table);

    let mut domy = pustostany(city);
    let etaty = wakaty(city);
    if domy.is_empty() {
        return Err(PopulationError::NoHomes);
    }
    let homes_total = domy.len() as u32;
    let jobs_total = etaty.len() as u32;

    // Zasiedlanie nie bierze etatów z puli: krok 5 przydziela je po dopasowaniu,
    // a `spawn_household_aged` rozdałby je wcześniej „pierwszy lepszy z dzielnicy".
    // To jest zarazem odpowiedź na korektę E-20 — wąskiego gardła `take_job_in`
    // Etap 8 po prostu nie dotyka.
    *world.resource_mut::<Vacancies>() = Vacancies::default();

    let ages = world.resource::<DemographyTable>().ages();
    let adult_age = i32::from(ages.adult);
    let max_age = i32::from(ages.max);
    let unemp = params
        .unemployment_target_permille
        .unwrap_or(t.unemployment_permille);

    // ── krok 1: piramida wieku ──────────────────────────────────────────────────
    let bands = t.pyramid(&epoch).to_vec();
    let cel = docelowa_populacja(params, &bands, jobs_total, homes_total, unemp, &t, ages);
    let mut pula = piramida(seed, cel, &bands, ages.max);
    odcinek("most", &mut czasy, &mut zegar);

    // ── krok 2: skład gospodarstw ───────────────────────────────────────────────
    // Lokale rosnąco po wartości: krok 6 i tak je przestawi, ale punkt startowy jest
    // wtedy funkcją miasta, a nie kolejności w tablicy budynków.
    domy.sort_by(|a, b| {
        a.value
            .get()
            .cmp(&b.value.get())
            .then((a.building, a.unit).cmp(&(b.building, b.unit)))
    });
    let sklady = sklady(seed, &mut pula, &domy, &t.households, adult_age, max_age);

    // ── krok 2b: spawn tym samym generatorem co migracja (korekta E-13) ──────────
    let (gospodarstwa, mieszkancy) = zasiedl(world, &sklady, seed, ages);

    odcinek("spawn", &mut czasy, &mut zegar);

    // ── kroki 3 i 4: wykształcenie, umiejętności, osobowość ─────────────────────
    nadaj_cechy(
        world,
        &mieszkancy,
        &t,
        &jobs_table,
        &epoch,
        seed,
        adult_age,
        ages,
    );

    odcinek("cechy", &mut czasy, &mut zegar);

    // ── krok 5: dopasowanie pracy ───────────────────────────────────────────────
    let zatrudnienie = dopasuj_prace(world, &t, &jobs_table, &mieszkancy, etaty, unemp, ages);

    // ── krok 5b: szkoła dla ucznia ──────────────────────────────────────────────
    let bez_szkoly = przypisz_szkoly(world, &mieszkancy, &places);

    odcinek("praca", &mut czasy, &mut zegar);

    // ── kroki 6 i 7: mieszkania i dojazd ────────────────────────────────────────
    //
    // Estymator dojazdu jest już **siecią M4**, nie dwiema płaskimi tablicami M3
    // (`Z-3`): krok 7 steruje medianą dojazdu całego miasta, więc odległość musi
    // być sieciowa, a nie manhattanowa. Flota jest jeszcze pusta — auta rozdaje się
    // po dopasowaniu mieszkań, bo dopiero wtedy wiadomo, kto gdzie mieszka.
    let commuters = zatrudnienie.employed.max(1);
    let (mut oracle, vdf, pojemnosc_cache) =
        crate::traffic_build::build_oracle(city, places.clone(), commuters)
            .map_err(|e| PopulationError::Traffic(Box::new(e)))?;
    let commute_cel = params.commute_median_min.unwrap_or(t.commute.median_min);
    let mieszkania = dopasuj_mieszkania(
        world,
        &gospodarstwa,
        &domy,
        &oracle,
        seed,
        commute_cel,
        params.commute_swaps.unwrap_or(200_000),
    );

    odcinek("mieszkania", &mut czasy, &mut zegar);

    // ── kroki 8 i 9: wiedza i relacje startowe ──────────────────────────────────
    let wiedza = zasiej_wiedze(world, &mieszkancy, &places, &t.knowledge);
    odcinek("wiedza", &mut czasy, &mut zegar);
    let relacje = relacje_startowe(world, &mieszkancy, &t.starting_relations, seed);
    odcinek("relacje", &mut czasy, &mut zegar);

    // Status społeczny liczy się z **rozkładu**, więc dopiero teraz, gdy rozkład
    // istnieje: dochody, adresy i zawody są już przypisane. Bez tego kroku miasto
    // startuje z zerowym statusem u wszystkich i pierwsza karta inspekcji pokazuje
    // metropolię złożoną z klasy niższej — a dobór partnera (§5.6) czyta tę liczbę
    // przez cały pierwszy miesiąc.
    social::step_month(world, 0);
    odcinek("status", &mut czasy, &mut zegar);

    // ── krok 10: pula po zasiedleniu i weryfikacja ──────────────────────────────
    let zajetych = gospodarstwa.len();
    let wolne_domy: Vec<HomeSlot> = domy[zajetych.min(domy.len())..].to_vec();
    let homes_free = wolne_domy.len() as u32;
    let jobs_free = zatrudnienie.wolne.len() as u32;
    *world.resource_mut::<Vacancies>() = Vacancies::with_occupancy(
        wolne_domy,
        zatrudnienie.wolne.clone(),
        homes_total,
        &zatrudnienie.wszystkie,
    );
    debug_assert_eq!(
        homes_free as usize + zajetych,
        domy.len(),
        "księgowość lokali się nie zamyka"
    );

    let mut report = raport(
        world,
        &mieszkancy,
        &gospodarstwa,
        &bands,
        &zatrudnienie,
        &mieszkania,
        Liczby {
            homes_total,
            homes_free,
            jobs_total,
            jobs_free,
            unemp,
            commute_cel,
            commute_sigma: t.commute.sigma_centi,
            places: places.len() as u32,
            wiedza,
            relacje,
            bez_szkoly,
        },
    );

    odcinek("raport", &mut czasy, &mut zegar);

    // ── krok 11: flota ──────────────────────────────────────────────────────────
    //
    // Na końcu, bo kierowcą zostaje pracujący dorosły, a adres gospodarstwa jest
    // znany dopiero po kroku 7. Auto stoi zaparkowane pod domem właściciela.
    let catalog = oracle.catalog().clone();
    // Parkingi **przed** flotą: auto, którego nie ma gdzie trzymać, nie powstaje
    // (WP7, niezmiennik `parking_no_ghosts`).
    let mut parking = crate::traffic_build::build_parking(&oracle);
    let (drivers, mut fleet, mut flota) = crate::traffic_build::seed_fleet(
        world,
        seed,
        &catalog,
        crate::traffic_build::MOTORISATION_PER_MILLE,
        &mut parking,
        &places,
    );
    flota.stations = oracle.stations().len() as u32;
    flota.route_cache_capacity = pojemnosc_cache;
    oracle.set_seed(seed);
    oracle.set_incomes(crate::traffic_build::dochody_gospodarstw(world));
    oracle.set_parking(parking);
    oracle.set_drivers(drivers);

    // Komunikacja miejska (WP10) po flocie, bo tabor dokłada się do tej samej tablicy
    // pojazdów — kurs sięga po pojazd tym samym slotem co kierowca po swoje auto.
    let (transit, kierowcy, tranzyt) =
        crate::traffic_build::seed_transit(world, city, &oracle, &catalog, &mut fleet);
    flota.transit_lines = tranzyt.transit_lines;
    flota.transit_stops = tranzyt.transit_stops;
    flota.transit_buses = tranzyt.transit_buses;
    oracle.set_transit(transit);
    let traffic = Arc::new(oracle);
    crate::traffic_build::install_traffic(world, traffic.clone(), vdf, fleet, kierowcy);

    odcinek("flota", &mut czasy, &mut zegar);
    report.timings = czasy;

    Ok(Populated {
        places,
        traffic,
        fleet: flota,
        report,
    })
}

/// Klucz pierścienia epoki, w którym miasto zaczyna grę.
///
/// Piramida wieku i rozkład wykształcenia zależą od epoki startowej, a ta jest
/// **ostatnim** pierścieniem `rings_for` — tym częściowo zabudowanym. Gdy danych
/// nie da się wczytać, zostaje klucz domyślny z `population.ron`; brak epoki nie ma
/// prawa wywrócić zaludniania miasta, które już stoi.
fn klucz_epoki(city: &CityData) -> String {
    crate::city::zoning::EpochTable::load()
        .ok()
        .and_then(|t| t.rings_for(city.plan.epoch).last().map(|e| e.key.clone()))
        .unwrap_or_default()
}
