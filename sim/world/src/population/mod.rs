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

use crate::city::build::{ShiftId, UnitKind};
use crate::city::sites::SectorId;
use crate::city::CityData;
use magnat_agents::{
    demography, household, migration, social, Ages, ArrayVec, CitizenView, CityFacts,
    DemographyTable, Employment, HomeSlot, Household, HouseholdOverflow, Identity, JobSlot,
    KnowledgeKind, Needs, Personality, PlaceEntry, PlaceTable, RelationKind, Residence, ShiftKind,
    SkillSlot, Skills, Vacancies, Vitals, MAX_ON_ROUTE,
};
use magnat_traffic::{OracleHandle, TrafficOracle};
use crate::traffic_build::FleetReport;
use magnat_core::{
    det_math, rng, BuildingId, CitizenId, Entity, Money, PlaceKind,
    PlaceRef, Rng, SiteId, StreamId, Tick, WorldCoord,
};
use magnat_ecs::World;
use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use table::{PopulationTable, TableError, AGE_BANDS, BAND_YEARS};

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

/// Wynik generacji — liczby, które sprawdza §7.4 dokumentu fazy.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct PopulationReport {
    pub citizens: u32,
    pub households: u32,
    pub homes_total: u32,
    pub homes_free: u32,
    pub jobs_total: u32,
    pub jobs_free: u32,
    /// Aktywni zawodowo: wiek produkcyjny i nie uczą się.
    pub active: u32,
    pub employed: u32,
    pub pupils: u32,
    pub unemployment_permille: u16,
    pub unemployment_target_permille: u16,
    /// Udział zatrudnionych z `skill_match(role) ≥ 40`, w promilach.
    pub skill_fit_permille: u16,
    pub commute_median_min: u16,
    pub commute_target_min: u16,
    /// Histogram uzyskany — **liczności**, nie promile: χ² liczy się z liczności,
    /// a przeliczenie na promile gubi po jednym na kubełek i samo w sobie daje
    /// χ² rzędu kilkudziesięciu przy stu tysiącach mieszkańców.
    pub commute_hist: [u32; COMMUTE_BINS],
    /// Histogram docelowy w promilach.
    pub commute_target_hist: [u32; COMMUTE_BINS],
    /// Korelacja rang Spearmana (dochód GD, wartość lokalu) × 100.
    pub income_housing_rho_centi: i16,
    /// Piramida wieku uzyskana — liczności (jak wyżej).
    pub pyramid: [u32; AGE_BANDS],
    /// Piramida docelowa epoki, w promilach.
    pub pyramid_target: [u16; AGE_BANDS],
    pub places: u32,
    pub knowledge_entries: u64,
    pub relations: u64,
    /// Żądana mediana dojazdu, zanim przycięło ją to, co miasto dopuszcza.
    pub commute_requested_min: u16,
    /// Przedział median osiągalnych w tym mieście przy ruchu wyłącznie pieszym.
    pub commute_feasible: (u16, u16),
    /// Czas kroków w sekundach — wejście bramki wydajności §7.5 (`bench_population_gen`).
    /// Mierzony tak samo jak czasy etapów w `GenerationReport` M2.
    pub timings: Vec<(&'static str, f32)>,
    /// Mieszkańcy bez lokalu — kryterium `gen_everyone_has_home` mówi „zero".
    pub homeless: u32,
    /// Uczniowie bez przypisanej placówki — tyle samo warte co wyżej.
    pub pupils_without_school: u32,
    /// Mieszkańcy, którzy nie znają **żadnego** miejsca zaspokajającego potrzebę.
    ///
    /// To **nie jest błąd**, tylko treść §5.7: kto mieszka na obrzeżu bez sklepu
    /// w promieniu zasiewu, ten zna tylko własną pracę, a resztę dostanie od sąsiadów
    /// przez plotkę. Ale udział takich ludzi trzeba widzieć — jeden procent to
    /// przedmieście, połowa to miasto stojące w miejscu (korekta H-20).
    pub without_knowledge: u32,
}

impl PopulationReport {
    /// Statystyka χ² histogramu czasu dojazdu wobec celu (test `gen_commute_hist`).
    #[must_use]
    pub fn commute_chi2(&self) -> f64 {
        let n: u32 = self.commute_hist.iter().sum();
        chi2(&self.commute_hist, &self.commute_target_hist, n)
    }

    /// Statystyka χ² piramidy wieku wobec piramidy epoki (test `gen_age_pyramid`).
    #[must_use]
    pub fn pyramid_chi2(&self) -> f64 {
        let b: [u32; AGE_BANDS] = std::array::from_fn(|i| u32::from(self.pyramid_target[i]));
        chi2(&self.pyramid, &b, self.citizens)
    }

    /// Histogram dojazdu w promilach — do wydruku, nie do testu.
    #[must_use]
    pub fn commute_permille(&self) -> [u32; COMMUTE_BINS] {
        let n: u32 = self.commute_hist.iter().sum();
        std::array::from_fn(|i| (self.commute_hist[i] * 1000).checked_div(n).unwrap_or(0))
    }

    /// Piramida w promilach — do wydruku, nie do testu.
    #[must_use]
    pub fn pyramid_permille(&self) -> [u32; AGE_BANDS] {
        std::array::from_fn(|i| {
            (self.pyramid[i] * 1000)
                .checked_div(self.citizens)
                .unwrap_or(0)
        })
    }

    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        out.push(format!(
            "populacja: {} mieszkańców w {} gospodarstwach, {} miejsc w katalogu",
            self.citizens, self.households, self.places
        ));
        out.push(format!(
            "lokale: {}/{} wolnych · etaty: {}/{} wolnych",
            self.homes_free, self.homes_total, self.jobs_free, self.jobs_total
        ));
        out.push(format!(
            "praca: {} aktywnych, {} zatrudnionych, bezrobocie {} ‰ (cel {} ‰), dopasowanie {} ‰",
            self.active,
            self.employed,
            self.unemployment_permille,
            self.unemployment_target_permille,
            self.skill_fit_permille
        ));
        out.push(format!(
            "dojazd: mediana {} min (cel {} min, żądano {}, osiągalne {}–{}), χ² {:.1}",
            self.commute_median_min,
            self.commute_target_min,
            self.commute_requested_min,
            self.commute_feasible.0,
            self.commute_feasible.1,
            self.commute_chi2()
        ));
        out.push(format!(
            "dochód↔mieszkanie: ρ = {:.2} · piramida χ² {:.1}",
            f64::from(self.income_housing_rho_centi) / 100.0,
            self.pyramid_chi2()
        ));
        out.push(format!(
            "wiedza: {} wpisów, relacje: {} krawędzi, uczniów {} (bez szkoły {}), bezdomnych {}",
            self.knowledge_entries,
            self.relations,
            self.pupils,
            self.pupils_without_school,
            self.homeless
        ));
        out.push(format!(
            "bez znajomości miejsc: {} ({:.1} %)",
            self.without_knowledge,
            f64::from(self.without_knowledge) * 100.0 / f64::from(self.citizens.max(1))
        ));
        if !self.timings.is_empty() {
            out.push(format!(
                "kroki: {}",
                self.timings
                    .iter()
                    .map(|(n, t)| format!("{n} {t:.1} s"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        out
    }
}

/// χ² liczności wobec rozkładu docelowego podanego w promilach.
///
/// Liczności, nie promile: przeliczenie obserwacji na promile obcina po ułamku
/// na kubełek, a przy stu tysiącach obserwacji samo to obcięcie daje χ² rzędu
/// kilkudziesięciu — czyli więcej niż próg testu.
fn chi2(obs: &[u32], exp_permille: &[u32], n: u32) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let mut s = 0.0;
    for (o, e) in obs.iter().zip(exp_permille) {
        let oczekiwane = f64::from(*e) * f64::from(n) / 1000.0;
        if oczekiwane < 1.0 {
            continue;
        }
        let d = f64::from(*o) - oczekiwane;
        s += d * d / oczekiwane;
    }
    s
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

// ── krok 0: most między miastem a agentami ──────────────────────────────────────

/// Środek bryły budynku w centymetrach.
fn srodek(city: &CityData, building: u32) -> WorldCoord {
    let b = &city.buildings.buildings[building as usize];
    WorldCoord::new(
        ((b.aabb.min.x + b.aabb.max.x) * 50.0) as i32,
        ((b.aabb.min.y + b.aabb.max.y) * 50.0) as i32,
        (b.aabb.min.z * 100.0) as i32,
    )
}

fn dzielnica_budynku(city: &CityData, building: u32) -> u16 {
    let b = &city.buildings.buildings[building as usize];
    city.parcels
        .parcels
        .get(b.parcel.0.index() as usize)
        .map_or(0, |p| p.district.0)
}

/// Katalog miejsc (korekta E-1).
///
/// Budynek z choćby jednym mieszkaniem jest `Home`; zakład dostaje rodzaj z archetypu
/// (`data/buildings/`, pole `place_kind`), a gdy go nie ma — `Workplace`. Jeden zakład
/// ma jeden rodzaj: szkoła jest `Education` **i** miejscem pracy, ale w indeksie
/// kategorii siedzi raz, bo do pracy dociera się przez `PlaceRef`, a nie przez
/// wyszukiwanie po rodzaju.
fn katalog_miejsc(city: &CityData) -> Vec<PlaceEntry> {
    let mut out = Vec::with_capacity(city.buildings.buildings.len() + city.sites.sites.len());
    for (i, b) in city.buildings.buildings.iter().enumerate() {
        let mieszkalny = city.buildings.units[b.units.start as usize..b.units.end as usize]
            .iter()
            .any(|u| matches!(u.kind, UnitKind::Dwelling { .. }));
        if mieszkalny {
            out.push(PlaceEntry {
                place: PlaceRef::Building(BuildingId(encja(i as u32))),
                kind: PlaceKind::Home,
                at: srodek(city, i as u32),
            });
        }
    }
    for (i, s) in city.sites.sites.iter().enumerate() {
        let kind = city
            .site_catalog
            .get(s.archetype)
            .spec
            .place_kind
            .unwrap_or(PlaceKind::Workplace);
        out.push(PlaceEntry {
            place: PlaceRef::Site(SiteId(encja(SITE_KEY_BASE + i as u32))),
            kind,
            at: srodek(city, s.building.0.index()),
        });
    }
    out
}

/// Pustostany. Wartość lokalu wyprowadzona z podpowiedzi czynszowej M2 — `rent_hint`
/// jest już funkcją powierzchni i wartości gruntu, więc mnożnik nie wnosi nowej wiedzy,
/// tylko zmienia jednostkę na „cenę lokalu". Krok 6 porównuje **decyle**, więc każde
/// przekształcenie rosnące daje ten sam wynik.
fn pustostany(city: &CityData) -> Vec<HomeSlot> {
    /// Ile miesięcy czynszu składa się na wartość lokalu — 20 lat.
    const CZYNSZOW: i64 = 240;
    let mut out = Vec::new();
    for (bi, b) in city.buildings.buildings.iter().enumerate() {
        let d = dzielnica_budynku(city, bi as u32);
        for (k, u) in city.buildings.units[b.units.start as usize..b.units.end as usize]
            .iter()
            .enumerate()
        {
            if !matches!(u.kind, UnitKind::Dwelling { .. }) {
                continue;
            }
            out.push(HomeSlot {
                building: bi as u32,
                unit: k as u16,
                district: d,
                value: Money(u.rent_hint.get().saturating_mul(CZYNSZOW)),
            });
        }
    }
    out
}

/// Grafik zmiany i maska dni tygodnia wg profilu branży (§5.9 krok 5, K-15).
///
/// Maska jest **własnością obsady, nie kalendarza**: tydzień dryfuje względem miesiąca,
/// więc liczba dni roboczych w miesiącu waha się między 20 a 23 i żaden wiersz tej
/// funkcji nie ma prawa tego zakładać.
fn grafik(sector: SectorId, base: ShiftId, i: u32) -> (ShiftKind, u8) {
    const PN_PT: u8 = 0b001_1111;
    const WT_SB: u8 = 0b011_1110;
    /// Środa plus weekend — obsada, dla której sobota i niedziela są dniami pracy.
    const SR_WEEKEND: u8 = 0b110_0100;
    /// Ruch ciągły: cztery brygady po pięć dni, wolne w innej parze dni każda.
    const BRYGADY: [u8; 4] = [0b001_1111, 0b011_1110, 0b111_1001, 0b110_0111];

    match sector {
        // Handel, usługi i zieleń mają obsadę weekendową — inaczej sobota wyglądałaby
        // jak miasto zamknięte na klucz (PRD §5.5, test `prop_weekly_rhythm`).
        SectorId::Retail | SectorId::Services | SectorId::Green => match i % 6 {
            0 => (ShiftKind::Early, PN_PT),
            1 => (ShiftKind::Day, PN_PT),
            2 => (ShiftKind::Afternoon, PN_PT),
            3 => (ShiftKind::Early, WT_SB),
            4 => (ShiftKind::Afternoon, WT_SB),
            _ => (ShiftKind::Weekend, SR_WEEKEND),
        },
        // Ruch ciągły tam, gdzie dane mówią o trzeciej zmianie.
        SectorId::Industry | SectorId::Extraction if base == ShiftId::III => {
            let b = (i % 4) as usize;
            (
                [
                    ShiftKind::Early,
                    ShiftKind::Afternoon,
                    ShiftKind::Night,
                    ShiftKind::Day,
                ][b],
                BRYGADY[b],
            )
        }
        SectorId::Office | SectorId::Public => (ShiftKind::Day, PN_PT),
        _ => match i % 4 {
            0 => (ShiftKind::Early, PN_PT),
            3 => (ShiftKind::Afternoon, PN_PT),
            _ => (ShiftKind::Day, PN_PT),
        },
    }
}

/// Wakaty. Zakład wchodzi do puli z kluczem przesuniętym o [`SITE_KEY_BASE`].
fn wakaty(city: &CityData) -> Vec<JobSlot> {
    let mut out = Vec::with_capacity(city.buildings.workplaces.len());
    for (si, s) in city.sites.sites.iter().enumerate() {
        let sector = city.site_catalog.get(s.archetype).sector();
        let d = dzielnica_budynku(city, s.building.0.index());
        for (k, wi) in (s.workplaces.start..s.workplaces.end).enumerate() {
            let Some(w) = city.buildings.workplaces.get(wi as usize) else {
                continue;
            };
            let (shift, days) = grafik(sector, w.shift, k as u32);
            out.push(JobSlot {
                site: SITE_KEY_BASE + si as u32,
                role: w.role.0,
                shift: shift as u8,
                work_days: days,
                district: d,
                wage_monthly: w.wage_band.median,
            });
        }
    }
    out
}

/// Fakty o mieście dla funkcji statusu i sąsiedztwa (korekta E-14).
fn fakty_miasta(city: &CityData, jobs: &crate::city::build::JobTable) -> CityFacts {
    let prestiz: Vec<u8> = jobs.roles.iter().map(|r| r.prestige).collect();

    // Pozycja dzielnicy to **percentyl** średniej wartości gruntu, a nie sama wartość:
    // status jest pozycją w społeczeństwie, więc i adres musi być pozycją wśród adresów.
    let mut wartosci: Vec<(u16, i64)> = city
        .districts
        .districts
        .iter()
        .enumerate()
        .map(|(i, d)| (i as u16, d.avg_land_value.get()))
        .collect();
    wartosci.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let n = wartosci.len().max(1);
    let mut score = vec![50u8; city.districts.districts.len()];
    for (rank, (d, _)) in wartosci.iter().enumerate() {
        score[*d as usize] = ((rank * 100) / n) as u8;
    }

    let block_of: Vec<u32> = city
        .buildings
        .buildings
        .iter()
        .enumerate()
        .map(|(i, b)| {
            city.parcels
                .parcels
                .get(b.parcel.0.index() as usize)
                .map_or(i as u32, |p| p.block.0)
        })
        .collect();

    CityFacts {
        job_prestige: prestiz,
        district_score: score,
        block_of,
    }
}


// ── pula wieku (krok 1) ─────────────────────────────────────────────────────────

/// Wielozbiór wieków z piramidy. Nie lista osób: osoby powstają dopiero przy spawnie,
/// a tu chodzi o to, żeby skład gospodarstw **zużył piramidę do zera** — inaczej test
/// χ² mierzy nie rozkład, tylko to, kogo zabrakło pod koniec pętli.
struct PulaWieku {
    counts: Vec<u32>,
    total: u32,
}

impl PulaWieku {
    fn new(max_age: u8) -> PulaWieku {
        PulaWieku {
            counts: vec![0; usize::from(max_age) + 1],
            total: 0,
        }
    }

    fn add(&mut self, age: usize) {
        let i = age.min(self.counts.len() - 1);
        self.counts[i] += 1;
        self.total += 1;
    }

    fn take_at(&mut self, age: usize) -> Option<i32> {
        if self.counts.get(age).copied().unwrap_or(0) == 0 {
            return None;
        }
        self.counts[age] -= 1;
        self.total -= 1;
        Some(age as i32)
    }

    /// Najbliższy niepusty rocznik w przedziale `[lo, hi]`, licząc od `target`.
    fn take_near(&mut self, target: i32, lo: i32, hi: i32) -> Option<i32> {
        let hi = hi.min(self.counts.len() as i32 - 1);
        let lo = lo.max(0);
        if lo > hi {
            return None;
        }
        let t = target.clamp(lo, hi);
        for d in 0..=(hi - lo) {
            for kandydat in [t - d, t + d] {
                if (lo..=hi).contains(&kandydat) {
                    if let Some(a) = self.take_at(kandydat as usize) {
                        return Some(a);
                    }
                }
            }
        }
        None
    }

    /// Losowy rocznik z przedziału, proporcjonalnie do liczebności.
    fn take_in(&mut self, lo: i32, hi: i32, r: &mut Rng) -> Option<i32> {
        let hi = hi.min(self.counts.len() as i32 - 1);
        let lo = lo.max(0);
        if lo > hi {
            return None;
        }
        let suma: u32 = self.counts[lo as usize..=hi as usize].iter().sum();
        if suma == 0 {
            return None;
        }
        let mut k = r.gen_range_u32(suma);
        for a in lo..=hi {
            let c = self.counts[a as usize];
            if k < c {
                return self.take_at(a as usize);
            }
            k -= c;
        }
        None
    }

    fn len_in(&self, lo: i32, hi: i32) -> u32 {
        let hi = hi.min(self.counts.len() as i32 - 1);
        let lo = lo.max(0);
        if lo > hi {
            return 0;
        }
        self.counts[lo as usize..=hi as usize].iter().sum()
    }
}

/// Krok 1: piramida wieku epoki.
fn piramida(seed: u64, n: u32, bands: &[u16], max_age: u8) -> PulaWieku {
    let mut pula = PulaWieku::new(max_age);
    for i in 0..n {
        let mut r = rng(seed, StreamId::PopGen, i, Tick(1));
        let mut k = r.gen_range_u32(1000);
        let mut band = bands.len() - 1;
        for (b, w) in bands.iter().enumerate() {
            if k < u32::from(*w) {
                band = b;
                break;
            }
            k -= u32::from(*w);
        }
        let wiek = band as u32 * BAND_YEARS + r.gen_range_u32(BAND_YEARS);
        pula.add(wiek as usize);
    }
    pula
}

// ── krok 2: składanie gospodarstw ───────────────────────────────────────────────

/// Jedno gospodarstwo w trakcie składania: wieki członków, zanim powstaną encje.
struct Sklad {
    adults: Vec<i32>,
    children: Vec<i32>,
    home: HomeSlot,
}

/// Krok 2: skład gospodarstw z puli wieku.
///
/// Typ gospodarstwa jest **punktem wyjścia**, nie wyrokiem: liczba dzieci jest skalowana
/// tak, żeby pula dzieci z piramidy wyszła na zero. Bez tego skład i piramida opisywałyby
/// dwa różne miasta i test χ² mierzyłby, które z nich wygrało.
fn sklady(
    seed: u64,
    pula: &mut PulaWieku,
    homes: &[HomeSlot],
    mix: &[table::HouseholdMix],
    adult_age: i32,
    max_age: i32,
) -> Vec<Sklad> {
    let doroslych = pula.len_in(adult_age, max_age);
    let dzieci = pula.len_in(0, adult_age - 1);
    let w_suma: u32 = mix.iter().map(|m| u32::from(m.weight)).sum::<u32>().max(1);
    // Średni skład z wag — w setnych osoby, żeby nie schodzić do floatów.
    let sr_doroslych = (mix
        .iter()
        .map(|m| u32::from(m.weight) * u32::from(m.adults))
        .sum::<u32>()
        * 100
        / w_suma)
        .max(100);
    let sr_dzieci = mix
        .iter()
        .map(|m| u32::from(m.weight) * u32::from(m.children))
        .sum::<u32>()
        * 100
        / w_suma;

    let h = ((doroslych * 100 / sr_doroslych) as usize).min(homes.len());
    let chciane = (h as u32) * sr_dzieci / 100;
    // Skala liczby dzieci: ile ich naprawdę jest wobec tego, ile chciałby rozkład.
    let skala = (dzieci * 1000).checked_div(chciane).unwrap_or(0);

    let mut out: Vec<Sklad> = Vec::with_capacity(h);
    for (i, home) in homes.iter().enumerate().take(h) {
        let mut r = rng(seed, StreamId::PopGen, i as u32, Tick(2));
        let mut k = r.gen_range_u32(w_suma);
        let mut typ = mix[0];
        for m in mix {
            if k < u32::from(m.weight) {
                typ = *m;
                break;
            }
            k -= u32::from(m.weight);
        }

        // Pierwszy dorosły z rozkładu; partner z Δwieku ~ 2 ± 4 lata (§5.9 krok 2).
        let Some(pierwszy) = pula.take_in(adult_age, max_age, &mut r) else {
            break;
        };
        let mut adults = vec![pierwszy];
        for _ in 1..typ.adults {
            let delta = 2 + r.gen_range_u32(9) as i32 - 4;
            match pula.take_near(pierwszy + delta, adult_age, max_age) {
                Some(a) => adults.push(a),
                None => break,
            }
        }

        let chce = (u32::from(typ.children) * skala + r.gen_range_u32(1000)) / 1000;
        let najstarszy = adults.iter().copied().max().unwrap_or(adult_age);
        let mut children = Vec::new();
        for _ in 0..chce.min(u32::from(typ.children) + 2) {
            // Rodzic starszy od dziecka o co najmniej 18 lat (§5.9 krok 2).
            let gorna = (najstarszy - adult_age).min(adult_age - 1);
            match pula.take_near((najstarszy - 30).max(0), 0, gorna) {
                Some(c) => children.push(c),
                None => break,
            }
        }

        out.push(Sklad {
            adults,
            children,
            home: *home,
        });
    }

    // Dzieci, które zostały, dopisujemy do gospodarstw mających dorosłego w wieku
    // rodzicielskim. Mieszkaniec zgubiony „bo skończyła się pętla" byłby dziurą
    // w piramidzie, której test χ² nie odróżni od błędu rozkładu.
    let mut i = 0usize;
    let limit = out.len().saturating_mul(4) + 1;
    let ile_gosp = out.len();
    while pula.len_in(0, adult_age - 1) > 0 && ile_gosp > 0 && i < limit {
        let g = &mut out[i % ile_gosp];
        let najstarszy = g.adults.iter().copied().max().unwrap_or(adult_age);
        let gorna = (najstarszy - adult_age).min(adult_age - 1);
        if g.adults.len() + g.children.len() < household::HH_MAX_MEMBERS {
            if let Some(c) = pula.take_near((najstarszy - 30).max(0), 0, gorna) {
                g.children.push(c);
            }
        }
        i += 1;
    }
    out
}

// ── kroki 3–4: wykształcenie, umiejętności, osobowość ───────────────────────────

/// Krok 3: wykształcenie i kierunek.
fn wyksztalcenie(
    mix: &[u16],
    fields: &[u16],
    wiek: i32,
    klasa: u32,
    adult_age: i32,
    r: &mut Rng,
) -> (u8, u8) {
    if wiek < 7 {
        return (0, 0);
    }
    if wiek < 15 {
        return (1, 0);
    }
    if wiek < adult_age {
        return (u8::from(r.gen_bool_permille(300)) + 1, 0);
    }
    let mut k = r.gen_range_u32(1000);
    let mut level = 0u8;
    for (i, w) in mix.iter().enumerate() {
        if k < u32::from(*w) {
            level = i as u8;
            break;
        }
        k -= u32::from(*w);
    }
    // Klasa zalążkowa gospodarstwa podnosi wykształcenie o stopień z prawdopodobieństwem
    // rosnącym z decylem — to jest cała „f(klasa)" z §5.9, wyrażona jednym hazardem.
    if level < 4 && r.gen_bool_permille((klasa * 60).min(1000) as u16) {
        level += 1;
    }
    // Dyplom przed dwudziestym czwartym rokiem życia jest rzadki.
    if level == 4 && wiek < 24 {
        level = 3;
    }
    if level <= 1 {
        return (level, 0);
    }
    let mut k = r.gen_range_u32(1000);
    let mut field = 1u8;
    for (i, w) in fields.iter().enumerate() {
        if k < u32::from(*w) {
            field = i as u8 + 1;
            break;
        }
        k -= u32::from(*w);
    }
    (level, field)
}

/// Krok 4: osiem cech. Rozkład trójkątny wokół średniej przesuniętej wykształceniem
/// i klasą — korelacja siedzi w danych (`population.ron`), nie tutaj.
fn osobowosc(specs: &[table::TraitSpec], level: u8, klasa: u32, r: &mut Rng) -> [u8; 8] {
    std::array::from_fn(|i| {
        let s = specs[i];
        let srodek = i32::from(s.mean)
            + i32::from(s.edu_bias) * (i32::from(level) - 2)
            + i32::from(s.status_bias) * (klasa as i32 - 5) / 2;
        let rozrzut = u32::from(s.spread).max(2);
        let szum =
            (r.gen_range_u32(rozrzut) + r.gen_range_u32(rozrzut)) as i32 / 2 - (rozrzut as i32) / 2;
        (srodek + szum).clamp(0, 100) as u8
    })
}

/// Krok 3: umiejętności. 1–3 sloty, poziom rośnie ze stażem i z dopasowaniem kierunku.
fn umiejetnosci(
    t: &PopulationTable,
    jobs: &crate::city::build::JobTable,
    wiek: i32,
    level: u8,
    field: u8,
    work_start: u8,
    r: &mut Rng,
) -> Skills {
    let mut out = Skills([SkillSlot {
        role: Skills::ROLE_NONE,
        level: 0,
        decay: 0,
    }; 4]);
    if wiek < i32::from(work_start) || jobs.roles.is_empty() {
        return out;
    }
    let staz = (wiek - i32::from(work_start)).clamp(0, 40) as u32;
    let ile = (1 + r.gen_range_u32(3)).min(jobs.roles.len() as u32).min(4);
    let start = r.gen_range_u32(jobs.roles.len() as u32);
    for k in 0..ile {
        let idx = ((start + k) % jobs.roles.len() as u32) as usize;
        let dopasowanie = u32::from(t.fit_of(&jobs.roles[idx].key, field));
        let poziom =
            (staz * 3 / 2 + dopasowanie / 3 + u32::from(level) * 4 + r.gen_range_u32(15)).min(100);
        out.0[k as usize] = SkillSlot {
            role: idx as u16,
            level: poziom as u8,
            decay: 2,
        };
    }
    out
}

/// Dopasowanie mieszkańca do roli: kierunek wykształcenia (dane), poziom wykształcenia
/// i posiadana umiejętność. 0..=100.
fn skill_match(t: &PopulationTable, role_key: &str, v: &Vitals, s: &Skills, role: u16) -> u8 {
    let baza = u32::from(t.fit_of(role_key, v.edu_field));
    let poziom = u32::from(s.level_in(role).get());
    let edu = u32::from(v.edu_level) * 5;
    ((baza * 6 + poziom * 3 + edu * 10) / 10).min(100) as u8
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
    let odcinek = |nazwa: &'static str, czasy: &mut Vec<(&'static str, f32)>, z: &mut std::time::Instant| {
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
    let mut gospodarstwa: Vec<Entity> = Vec::with_capacity(sklady.len());
    let mut mieszkancy: Vec<Entity> = Vec::new();
    for (i, s) in sklady.iter().enumerate() {
        let mut r = rng(seed, StreamId::PopGen, i as u32, Tick(3));
        let hh = migration::spawn_household_aged(
            world,
            0,
            &mut r,
            s.adults.len() as u8,
            s.children.len() as u8,
            s.home,
            ages.adult,
            1,
        );
        gospodarstwa.push(hh);

        // Nadpisanie wieków: `spawn_household_aged` losuje je z parametrów migracji,
        // a Etap 8 ma je z piramidy epoki. Kolejność członków jest kolejnością spawnu —
        // najpierw dorośli, potem dzieci (§5.9 krok 2).
        let hh_c = *world.get::<Household>(hh).expect("gospodarstwo po spawnie");
        let sklad =
            household::members_of(hh.index(), &hh_c, world.resource::<HouseholdOverflow>());
        for (k, m) in sklad.iter().enumerate() {
            let wiek = if k < s.adults.len() {
                s.adults[k]
            } else {
                s.children[k - s.adults.len()]
            };
            let Some(c) = demography::citizen_by_index(world, *m) else {
                continue;
            };
            if let Some(id) = world.get_mut::<Identity>(c) {
                id.birth_day = -wiek * 360;
                id.birth_district = s.home.district;
            }
            if let Some(e) = world.get_mut::<Employment>(c) {
                let uczen =
                    wiek >= i32::from(ages.school_start) && wiek < i32::from(ages.school_end);
                e.flags = if uczen {
                    Employment::FLAG_PUPIL
                } else if wiek >= i32::from(ages.retirement) {
                    Employment::FLAG_RETIRED | Employment::FLAG_UNEMPLOYED
                } else {
                    Employment::FLAG_UNEMPLOYED
                };
                e.site = Employment::NO_SITE;
            }
            mieszkancy.push(c);
        }
        // Typ gospodarstwa liczy się ze składu, a skład właśnie zmienił wiek.
        let mut hh_c = hh_c;
        demography::przeklasyfikuj(world, hh.index(), &mut hh_c, 0);
        if let Some(slot) = world.get_mut::<Household>(hh) {
            slot.kind = hh_c.kind;
        }
    }

    odcinek("spawn", &mut czasy, &mut zegar);

    // ── kroki 3 i 4: wykształcenie, umiejętności, osobowość ─────────────────────
    let edu_mix = t.education_of(&epoch).to_vec();
    for c in &mieszkancy {
        let mut r = rng(seed, StreamId::PersonalityGen, c.index(), Tick(4));
        // Klasa zalążkowa: dzielnica zamieszkania mówi o zamożności adresu, reszta
        // jest losowa. Prawdziwy status policzy `social::step_month` po pierwszym
        // miesiącu — tu chodzi tylko o to, żeby wykształcenie nie było niezależne
        // od tego, gdzie ktoś się urodził.
        let klasa = (u32::from(world.get::<Residence>(*c).map_or(0, |r| r.district) % 10)
            + r.gen_range_u32(10))
            / 2;
        let wiek = world.get::<Identity>(*c).map_or(0, |id| id.age_years(0));
        let (level, field) = wyksztalcenie(&edu_mix, &t.fields, wiek, klasa, adult_age, &mut r);
        if let Some(v) = world.get_mut::<Vitals>(*c) {
            v.edu_level = level;
            v.edu_field = field;
        }
        let cechy = osobowosc(&t.traits, level, klasa, &mut r);
        if let Some(p) = world.get_mut::<Personality>(*c) {
            *p = Personality(cechy);
        }
        let skills = umiejetnosci(&t, &jobs_table, wiek, level, field, ages.work_start, &mut r);
        if let Some(s) = world.get_mut::<Skills>(*c) {
            *s = skills;
        }
    }

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
    let (drivers, fleet, mut flota) = crate::traffic_build::seed_fleet(
        world,
        seed,
        &catalog,
        crate::traffic_build::MOTORISATION_PER_MILLE,
    );
    flota.stations = oracle.stations().len() as u32;
    flota.route_cache_capacity = pojemnosc_cache;
    oracle.set_drivers(drivers);
    let traffic = Arc::new(oracle);
    crate::traffic_build::install_traffic(world, traffic.clone(), vdf, fleet);

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

/// Docelowa liczba mieszkańców, gdy nikt jej nie narzucił.
///
/// Liczba **wynika z miasta**: kryterium `gen_jobs_filled` mówi
/// `|JobSlot| ≈ |aktywni| × (1 + bezrobocie)`, więc to liczba etatów wyznacza liczbę
/// aktywnych, a piramida wieku przelicza aktywnych na mieszkańców. Drugą granicą
/// jest liczba lokali: miasto nie zmieści więcej ludzi, niż ma mieszkań.
fn docelowa_populacja(
    params: &PopulationParams,
    bands: &[u16],
    jobs_total: u32,
    homes_total: u32,
    unemp: u16,
    t: &PopulationTable,
    ages: Ages,
) -> u32 {
    if let Some(n) = params.target_population {
        return n;
    }
    // Pasma piramidy są pięcioletnie, a granice wieku produkcyjnego nie muszą na nie
    // trafiać: 18 lat wypada w środku pasma 15–19. Zaokrąglenie pasma w dół albo w górę
    // przesuwa udział aktywnych o kilka procent, a kryterium `gen_jobs_filled` ma
    // tolerancję 2 % — więc liczymy **część wspólną** pasma z przedziałem wieku.
    let (od, do_) = (u32::from(ages.work_start), u32::from(ages.retirement));
    let aktywnych_permille: u32 = bands
        .iter()
        .enumerate()
        .map(|(b, w)| {
            let (a, z) = (b as u32 * BAND_YEARS, b as u32 * BAND_YEARS + BAND_YEARS);
            let wspolne = z.min(do_).saturating_sub(a.max(od));
            u32::from(*w) * wspolne / BAND_YEARS
        })
        .sum::<u32>()
        .max(1);
    let aktywni = u64::from(jobs_total) * 1000 / u64::from(1000 + u32::from(unemp));
    let z_etatow = (aktywni * 1000 / u64::from(aktywnych_permille)) as u32;

    let w_suma: u32 = t
        .households
        .iter()
        .map(|m| u32::from(m.weight))
        .sum::<u32>()
        .max(1);
    let sr_osob = t
        .households
        .iter()
        .map(|m| u32::from(m.weight) * u32::from(m.adults + m.children))
        .sum::<u32>()
        / w_suma;
    z_etatow.min(homes_total.saturating_mul(sr_osob.max(1)))
}

// ── krok 5: dopasowanie pracy ───────────────────────────────────────────────────

/// Wynik kroku 5.
struct Zatrudnienie {
    /// Etaty, które zostały wolne.
    wolne: Vec<JobSlot>,
    /// Wszystkie etaty miasta — `Vacancies` potrzebuje ich do mapy zakładów.
    wszystkie: Vec<JobSlot>,
    active: u32,
    employed: u32,
    pupils: u32,
    fit_ok: u32,
}

/// Ile wolnych etatów przejrzeć w dzielnicy, zanim wybierzemy najlepszy. Limit jest
/// twardy, bo bez niego krok 5 jest kwadratowy wobec wielkości dzielnicy.
const PRZEGLAD_ETATOW: usize = 24;

/// Krok 5: dopasowanie pracy (§5.9, korekta E-15).
///
/// Zachłannie po malejącym `skill_match`, ale **w granicach dzielnicy**: praca ma być
/// w zasięgu dojazdu z domu. Bez tego relacja współpracownicza spina przeciwne końce
/// miasta i plotka przeskakuje pięć kilometrów w jednym kroku (kryterium WP9).
fn dopasuj_prace(
    world: &mut World,
    t: &PopulationTable,
    jobs: &crate::city::build::JobTable,
    mieszkancy: &[Entity],
    etaty: Vec<JobSlot>,
    unemp: u16,
    ages: Ages,
) -> Zatrudnienie {
    let wszystkie = etaty.clone();
    // Wolne etaty per dzielnica — indeks, którego `Vacancies` nie ma (korekta E-20).
    let mut wolne_w: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for (i, j) in etaty.iter().enumerate() {
        wolne_w.entry(j.district).or_default().push(i as u32);
    }
    let mut zajety = vec![false; etaty.len()];

    // Kandydaci: aktywni zawodowo, malejąco po najlepszym dopasowaniu. Remis
    // rozstrzyga indeks encji (00 §3.2).
    let mut kandydaci: Vec<(u8, u32, Entity, u16)> = Vec::new();
    let (mut active, mut pupils) = (0u32, 0u32);
    for c in mieszkancy {
        let Some(id) = world.get::<Identity>(*c) else {
            continue;
        };
        let wiek = id.age_years(0);
        let emp = world.get::<Employment>(*c).copied().unwrap_or_default();
        if emp.flags & Employment::FLAG_PUPIL != 0 {
            pupils += 1;
            continue;
        }
        if wiek < i32::from(ages.work_start) || wiek >= i32::from(ages.retirement) {
            continue;
        }
        active += 1;
        let v = world.get::<Vitals>(*c).copied().unwrap_or_default();
        let s = world.get::<Skills>(*c).copied().unwrap_or_default();
        let najlepsze = jobs
            .roles
            .iter()
            .enumerate()
            .map(|(i, r)| skill_match(t, &r.key, &v, &s, i as u16))
            .max()
            .unwrap_or(0);
        let d = world.get::<Residence>(*c).map_or(0, |r| r.district);
        kandydaci.push((najlepsze, c.index(), *c, d));
    }
    kandydaci.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    // Ilu ma zostać bez pracy: dokładnie tylu, ilu każe cel bezrobocia (§5.9 krok 5).
    let limit = ((u64::from(active) * u64::from(1000 - unemp.min(1000)) / 1000) as usize)
        .min(etaty.len());

    let mut employed = 0usize;
    let mut fit_ok = 0u32;
    for (_, _, c, dzielnica) in &kandydaci {
        if employed >= limit {
            break;
        }
        let v = world.get::<Vitals>(*c).copied().unwrap_or_default();
        let s = world.get::<Skills>(*c).copied().unwrap_or_default();

        // Najlepszy etat w dzielnicy zamieszkania; dopiero gdy dzielnica jest pusta,
        // szukamy gdziekolwiek — i to jest ta połowa kryterium WP9, której §5.9
        // nie wypisywało (korekta E-15).
        let wybor = wybierz_etat(&mut wolne_w, &zajety, &etaty, *dzielnica, t, jobs, &v, &s)
            .or_else(|| wybierz_gdziekolwiek(&mut wolne_w, &zajety, &etaty, t, jobs, &v, &s));
        let Some(idx) = wybor else { continue };
        zajety[idx as usize] = true;
        let j = etaty[idx as usize];

        if let Some(e) = world.get_mut::<Employment>(*c) {
            e.site = j.site;
            e.role = j.role;
            e.shift = j.shift;
            e.work_days = j.work_days;
            e.flags &= !Employment::FLAG_UNEMPLOYED;
        }
        let klucz = &jobs.roles[(j.role as usize).min(jobs.roles.len() - 1)].key;
        if skill_match(t, klucz, &v, &s, j.role) >= 40 {
            fit_ok += 1;
        }
        let hh = gospodarstwo_mieszkanca(world, *c);
        if let Some(h) = hh.and_then(|e| world.get_mut::<Household>(e)) {
            h.income_monthly = Money(h.income_monthly.get().saturating_add(j.wage_monthly.get()));
        }
        employed += 1;
    }

    let wolne: Vec<JobSlot> = etaty
        .iter()
        .enumerate()
        .filter(|(i, _)| !zajety[*i])
        .map(|(_, j)| *j)
        .collect();

    Zatrudnienie {
        wolne,
        wszystkie,
        active,
        employed: employed as u32,
        pupils,
        fit_ok,
    }
}

#[allow(clippy::too_many_arguments)]
fn wybierz_etat(
    wolne: &mut BTreeMap<u16, Vec<u32>>,
    zajety: &[bool],
    etaty: &[JobSlot],
    dzielnica: u16,
    t: &PopulationTable,
    jobs: &crate::city::build::JobTable,
    v: &Vitals,
    s: &Skills,
) -> Option<u32> {
    let lista = wolne.get_mut(&dzielnica)?;
    while lista.last().is_some_and(|i| zajety[*i as usize]) {
        lista.pop();
    }
    if lista.is_empty() {
        return None;
    }
    let od = lista.len().saturating_sub(PRZEGLAD_ETATOW);
    let (mut best, mut best_score) = (None, -1i32);
    for (k, i) in lista[od..].iter().enumerate() {
        if zajety[*i as usize] {
            continue;
        }
        let role = etaty[*i as usize].role;
        let key = &jobs.roles[(role as usize).min(jobs.roles.len() - 1)].key;
        let score = i32::from(skill_match(t, key, v, s, role));
        if score > best_score {
            best_score = score;
            best = Some((od + k, *i));
        }
    }
    let (poz, idx) = best?;
    lista.remove(poz);
    Some(idx)
}

#[allow(clippy::too_many_arguments)]
fn wybierz_gdziekolwiek(
    wolne: &mut BTreeMap<u16, Vec<u32>>,
    zajety: &[bool],
    etaty: &[JobSlot],
    t: &PopulationTable,
    jobs: &crate::city::build::JobTable,
    v: &Vitals,
    s: &Skills,
) -> Option<u32> {
    let dzielnice: Vec<u16> = wolne
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(d, _)| *d)
        .collect();
    for d in dzielnice {
        if let Some(i) = wybierz_etat(wolne, zajety, etaty, d, t, jobs, v, s) {
            return Some(i);
        }
    }
    None
}

fn gospodarstwo_mieszkanca(world: &World, c: Entity) -> Option<Entity> {
    let id = world.get::<Identity>(c)?;
    demography::household_by_index(world, id.household)
}

// ── krok 5b: szkoły ─────────────────────────────────────────────────────────────

/// Uczeń bez placówki nie jest odprowadzany i nie ma szkoły w planie dnia
/// (`household::roles` wymaga `site != NO_SITE`). Zwraca liczbę uczniów, dla których
/// miasto nie miało ani jednej szkoły w zasięgu — to jest liczba do raportu, nie
/// do wygładzenia.
fn przypisz_szkoly(world: &mut World, mieszkancy: &[Entity], places: &PlaceTable) -> u32 {
    /// Promienie szukania szkoły: kwartał, dzielnica, pół miasta.
    const PROMIENIE: [f32; 3] = [800.0, 2500.0, 8000.0];
    let mut bez = 0u32;
    for c in mieszkancy {
        let uczen = world
            .get::<Employment>(*c)
            .is_some_and(|e| e.flags & Employment::FLAG_PUPIL != 0);
        if !uczen {
            continue;
        }
        let Some(dom) = world.get::<Residence>(*c).and_then(home_place) else {
            bez += 1;
            continue;
        };
        let Some(at) = places.coord_of(dom) else {
            bez += 1;
            continue;
        };
        let mut najblizsza: Option<(i64, u32)> = None;
        for r in PROMIENIE {
            places.for_each_near(PlaceKind::Education, at, r, |e| {
                let d = e.at.distance_sq_xy(at);
                let klucz = magnat_agents::knowledge_key(e.place).unwrap_or(u32::MAX);
                if najblizsza.is_none_or(|(bd, bk)| (d, klucz) < (bd, bk)) {
                    najblizsza = Some((d, klucz));
                }
            });
            if najblizsza.is_some() {
                break;
            }
        }
        match najblizsza {
            Some((_, klucz)) => {
                if let Some(e) = world.get_mut::<Employment>(*c) {
                    e.site = klucz;
                    e.work_days = 0b001_1111;
                    e.shift = ShiftKind::Early as u8;
                }
            }
            None => bez += 1,
        }
    }
    bez
}

// ── kroki 6 i 7: mieszkanie i dojazd ────────────────────────────────────────────

/// Pracujący członek gospodarstwa w lustrze kroków 6–7.
struct Pracownik {
    citizen: Entity,
    identity: Identity,
    vitals: Vitals,
    work: PlaceRef,
    commute: u16,
}

/// Wynik kroków 6 i 7.
struct Mieszkania {
    rho_centi: i16,
    hist: [u32; COMMUTE_BINS],
    median: u16,
    /// Cel po przycięciu do tego, co geometria miasta w ogóle dopuszcza.
    cel: u16,
    /// Zmierzony przedział osiągalnych median.
    osiagalne: (u16, u16),
}

/// Kroki 6 i 7: dochód ↔ wartość lokalu, a potem histogram czasu dojazdu.
///
/// `ponytail:` krok 7 to zwykła zachłanna wymiana sterowana χ², nie wyżarzanie ani
/// transport optymalny (§5.9). Sufit znany: przy bardzo nierównomiernym rozkładzie
/// miejsc pracy zbieżność może nie zejść poniżej 5 % błędu mediany — wtedy podmieniamy
/// na wyżarzanie. Nie wcześniej.
#[allow(clippy::too_many_arguments)]
fn dopasuj_mieszkania(
    world: &mut World,
    gospodarstwa: &[Entity],
    domy: &[HomeSlot],
    oracle: &TrafficOracle,
    seed: u64,
    cel_min: u16,
    proby: u32,
) -> Mieszkania {
    let n = gospodarstwa.len();
    if n < 2 {
        return Mieszkania {
            rho_centi: 0,
            hist: [0; COMMUTE_BINS],
            median: 0,
            cel: cel_min,
            osiagalne: (0, 0),
        };
    }

    // Lustro: dla każdego gospodarstwa jego pracujący i ich kopie `Identity`/`Vitals`.
    // Prędkość marszu zależy od wieku i zdrowia, więc widok musi być **czyjś**, a nie
    // przeciętny — a 200 tys. prób zamiany nie ma prawa chodzić po archetypach ECS.
    let mut zaloga: Vec<Vec<Pracownik>> = Vec::with_capacity(n);
    for hh in gospodarstwa {
        let hh_c = world.get::<Household>(*hh).copied().unwrap_or_default();
        let sklad = household::members_of(hh.index(), &hh_c, world.resource::<HouseholdOverflow>());
        let mut v = Vec::new();
        for m in sklad.iter() {
            let Some(c) = demography::citizen_by_index(world, *m) else {
                continue;
            };
            let Some(work) = world
                .get::<Employment>(c)
                .filter(|e| e.flags & Employment::FLAG_PUPIL == 0)
                .and_then(|e| site_place(e.site))
            else {
                continue;
            };
            v.push(Pracownik {
                citizen: c,
                identity: world.get::<Identity>(c).copied().unwrap_or_default(),
                vitals: world.get::<Vitals>(c).copied().unwrap_or_default(),
                work,
                commute: 0,
            });
        }
        zaloga.push(v);
    }

    // Krok 6: rangi dochodu wobec rang wartości lokalu, z szumem kopuły.
    let mut po_dochodzie: Vec<(i64, u32, usize)> = gospodarstwa
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let h = world.get::<Household>(*e).copied().unwrap_or_default();
            (h.income_monthly.get(), e.index(), i)
        })
        .collect();
    po_dochodzie.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // `domy` są posortowane rosnąco po wartości, a gospodarstwo `i` zajmuje `domy[i]`,
    // więc ranga lokalu to po prostu jego indeks.
    let mut lokal_dla_rangi: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let mut r = rng(seed, StreamId::PopGen, i as u32, Tick(6));
        // p(przesunięcia o k) ∝ exp(−k): rozkład geometryczny z p = ½, czytany
        // z ciągu jedynek na końcu słowa losowego.
        let k = r.next_u32().trailing_ones().min(6) as usize;
        if k == 0 {
            continue;
        }
        // §5.9 mówi o przesunięciu **o k decyli** — i tak też się je liczy.
        let skok = (k * n / 10).max(1);
        let j = if r.gen_bool_permille(500) {
            (i + skok).min(n - 1)
        } else {
            i.saturating_sub(skok)
        };
        lokal_dla_rangi.swap(i, j);
    }

    // `adres[i]` — lokal, który ma gospodarstwo o indeksie `i` w `gospodarstwa`.
    let mut adres: Vec<usize> = vec![0; n];
    for (ranga, lokal) in lokal_dla_rangi.iter().enumerate() {
        adres[po_dochodzie[ranga].2] = *lokal;
    }
    // Ranga wartości lokalu przypisana gospodarstwu o danej randze dochodu — wejście
    // korelacji Spearmana.
    let rho = spearman(&lokal_dla_rangi);

    // Krok 7: mediana czasu dojazdu.
    //
    // **Poprawka do §5.9** (korekta H-2): pętla poprawkowa steruje **medianą**, a nie
    // całym histogramem. Powód jest mierzalny: w M3 wszyscy chodzą pieszo (K-2 —
    // środki transportu dokłada M4), a praca jest przydzielana w granicach dzielnicy
    // (E-15), więc kształt rozkładu jest własnością geometrii miasta i zamiana mieszkań
    // go nie zmienia. Minimalizowanie χ² całego histogramu przesuwało przy tym medianę
    // **w złą stronę**, bo nadrabiało ogon rozkładu kosztem środka — a kryterium
    // `gen_commute_hist` mierzy właśnie medianę. χ² zostaje w raporcie jako diagnostyka
    // kształtu; progu na niego §7.4 nigdy nie podało i przy n ≈ 20 tys. żaden
    // osiągalny fit nie przeszedłby testu p > 0,05.
    //
    // Cel też nie jest przyjmowany na wiarę: mierzymy przedział median, w którym miasto
    // w ogóle da się ustawić, i przycinamy do niego żądanie. Raport mówi, że przyciął.
    let mut hist = [0u32; COMMUTE_BINS];
    let mut razem = 0u32;
    for (i, lista) in zaloga.iter_mut().enumerate() {
        let dom = miejsce_domu(domy[adres[i]]);
        dojazdy_gospodarstwa(oracle, dom, lista);
        for p in lista.iter() {
            hist[kubelek(p.commute)] += 1;
            razem += 1;
        }
    }
    let dolna = mediana_minutowa(&zaloga);

    // Górny koniec: adresy przemieszane **wewnątrz decyli wartości**, czyli tak
    // rozrzucone, jak krok 6 w ogóle pozwala.
    //
    // `ponytail:` mierzone na **próbce** gospodarstw, nie na wszystkich. Sufit nazwany:
    // przy próbce 4 tys. błąd mediany to ułamek minuty, a pełny przebieg kosztuje dwa
    // dodatkowe przejścia po całej populacji — czyli sekundy z budżetu 30 s. Gdyby
    // kiedyś zaczęło zależeć na dokładności tego przedziału co do minuty, próbkę
    // podnosi się jedną stałą.
    const PROBKA_GRANIC: usize = 4_000;
    let krok_probki = (n / PROBKA_GRANIC.max(1)).max(1);
    let wymieszane = przemieszaj_w_decylach(&adres, seed, n);
    let mut probka: Vec<Vec<Pracownik>> = Vec::new();
    for i in (0..n).step_by(krok_probki) {
        let dom = miejsce_domu(domy[wymieszane[i]]);
        let mut v: Vec<Pracownik> = zaloga[i]
            .iter()
            .map(|p| Pracownik {
                citizen: p.citizen,
                identity: p.identity,
                vitals: p.vitals,
                work: p.work,
                commute: 0,
            })
            .collect();
        dojazdy_gospodarstwa(oracle, dom, &mut v);
        probka.push(v);
    }
    let gorna = mediana_minutowa(&probka);

    let (lo, hi) = (dolna.min(gorna), dolna.max(gorna));
    let cel_min = cel_min.clamp(lo, hi);

    // Mediana jest równa `cel_min` dokładnie wtedy, gdy połowa dojazdów jest krótsza —
    // więc sterujemy licznikiem „krótszych niż cel", a nie samą medianą. Aktualizacja
    // jest wtedy O(1) na zmieniony dojazd, a nie przejściem po całym rozkładzie.
    let mut krotszych = policz_krotsze(&zaloga, cel_min);
    let mut odchylenie = blad_mediany(krotszych, razem);
    for i in 0..proby {
        if razem == 0 || odchylenie == 0 {
            break;
        }
        let mut r = rng(seed, StreamId::PopGen, i, Tick(7));
        let a = r.gen_range_u32(n as u32) as usize;
        let b = r.gen_range_u32(n as u32) as usize;
        if a == b || (zaloga[a].is_empty() && zaloga[b].is_empty()) {
            continue;
        }
        // Zamiana wyłącznie **wewnątrz tego samego decyla wartości** — inaczej krok 7
        // zjadałby dopasowanie z kroku 6 (§5.9).
        if adres[a] * 10 / n != adres[b] * 10 / n {
            continue;
        }

        let stare: Vec<u16> = zaloga[a]
            .iter()
            .chain(zaloga[b].iter())
            .map(|p| p.commute)
            .collect();
        adres.swap(a, b);
        for k in [a, b] {
            let dom = miejsce_domu(domy[adres[k]]);
            for p in &zaloga[k] {
                hist[kubelek(p.commute)] -= 1;
                krotszych -= u32::from(p.commute <= cel_min);
            }
            dojazdy_gospodarstwa(oracle, dom, &mut zaloga[k]);
            for p in &zaloga[k] {
                hist[kubelek(p.commute)] += 1;
                krotszych += u32::from(p.commute <= cel_min);
            }
        }
        let nowe = blad_mediany(krotszych, razem);
        if nowe <= odchylenie {
            odchylenie = nowe;
        } else {
            adres.swap(a, b);
            let mut it = stare.into_iter();
            for k in [a, b] {
                for p in zaloga[k].iter_mut() {
                    hist[kubelek(p.commute)] -= 1;
                    krotszych -= u32::from(p.commute <= cel_min);
                    p.commute = it.next().unwrap_or(p.commute);
                    hist[kubelek(p.commute)] += 1;
                    krotszych += u32::from(p.commute <= cel_min);
                }
            }
        }
    }

    // Wynik wraca do świata: adres gospodarstwa, adresy członków i czas dojazdu
    // odniesienia, który §7.4 porównuje z histogramem.
    for (i, hh) in gospodarstwa.iter().enumerate() {
        let d = domy[adres[i]];
        przeprowadz(world, *hh, d);
        for p in &zaloga[i] {
            if let Some(e) = world.get_mut::<Employment>(p.citizen) {
                e.commute_baseline_min = p.commute;
            }
        }
    }

    let mediana = mediana_minutowa(&zaloga);
    Mieszkania {
        rho_centi: rho,
        hist,
        median: mediana,
        cel: cel_min,
        osiagalne: (lo, hi),
    }
}

/// Ilu pracujących ma dojazd nie dłuższy niż `cel`.
fn policz_krotsze(zaloga: &[Vec<Pracownik>], cel: u16) -> u32 {
    zaloga
        .iter()
        .flatten()
        .filter(|p| p.commute <= cel)
        .count() as u32
}

/// Odległość od stanu „mediana = cel". Mediana to pierwsza minuta, w której skumulowany
/// rozkład sięga połowy — więc trafia w `cel` dokładnie wtedy, gdy dojazdów **nie
/// dłuższych** niż `cel` jest dokładnie połowa.
fn blad_mediany(krotszych: u32, razem: u32) -> u32 {
    (i64::from(krotszych) * 2 - i64::from(razem)).unsigned_abs() as u32
}

/// Permutacja adresów przemieszana w obrębie decyli wartości — górny koniec rozrzutu
/// dojazdów, jaki krok 6 dopuszcza.
///
/// Pełne tasowanie wewnątrz każdego decyla, nie „losowa zamiana z losowym sąsiadem":
/// to drugie zostawia większość gospodarstw na miejscu, więc mierzyłoby nie górny
/// koniec przedziału, tylko punkt startowy.
fn przemieszaj_w_decylach(adres: &[usize], seed: u64, n: usize) -> Vec<usize> {
    let mut out = adres.to_vec();
    let mut grupy: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, a) in adres.iter().enumerate() {
        grupy.entry(a * 10 / n).or_default().push(i);
    }
    for (d, idx) in &grupy {
        let mut r = rng(seed, StreamId::PopGen, *d as u32, Tick(11));
        let mut wartosci: Vec<usize> = idx.iter().map(|i| adres[*i]).collect();
        r.shuffle(&mut wartosci);
        for (k, i) in idx.iter().enumerate() {
            out[*i] = wartosci[k];
        }
    }
    out
}

fn miejsce_domu(h: HomeSlot) -> PlaceRef {
    PlaceRef::Building(BuildingId(encja(h.building)))
}

fn widok<'a>(p: &'a Pracownik, puste: &'a Puste) -> CitizenView<'a> {
    CitizenView {
        id: CitizenId(p.citizen),
        identity: &p.identity,
        vitals: &p.vitals,
        needs: &puste.needs,
        personality: &puste.personality,
        residence: &puste.residence,
        today: 0,
    }
}

/// Komponenty, których estymator pieszy nie czyta, a `CitizenView` ich wymaga.
/// Jedna instancja na przebieg zamiast trzech konstrukcji na wywołanie.
#[derive(Default)]
struct Puste {
    needs: Needs,
    personality: Personality,
    residence: Residence,
}

/// Dojazdy całego gospodarstwa. Każdy pracownik osobno, bo prędkość marszu zależy
/// od wieku i zdrowia — i bo cache par miejsc w estymatorze i tak zbiera powtórzenia.
fn dojazdy_gospodarstwa(oracle: &TrafficOracle, dom: PlaceRef, lista: &mut [Pracownik]) {
    let puste = Puste::default();
    for p in lista.iter_mut() {
        let tempo = magnat_traffic::speed_pct(&widok(p, &puste));
        p.commute = oracle.network_walk_minutes(dom, p.work, tempo);
    }
}

/// Zmiana adresu gospodarstwa razem z adresami wszystkich jego członków.
fn przeprowadz(world: &mut World, hh: Entity, home: HomeSlot) {
    let Some(h) = world.get_mut::<Household>(hh) else {
        return;
    };
    h.building = home.building;
    h.unit = home.unit;
    h.district = home.district;
    let hh_c = world.get::<Household>(hh).copied().unwrap_or_default();
    let sklad = household::members_of(hh.index(), &hh_c, world.resource::<HouseholdOverflow>());
    for m in sklad.iter() {
        let Some(c) = demography::citizen_by_index(world, *m) else {
            continue;
        };
        if let Some(r) = world.get_mut::<Residence>(c) {
            r.building = home.building;
            r.unit = home.unit;
            r.district = home.district;
        }
    }
}

fn kubelek(min: u16) -> usize {
    ((min / COMMUTE_BIN_MIN) as usize).min(COMMUTE_BINS - 1)
}

/// Mediana czasu dojazdu liczona **co minutę**, nie z kubełków histogramu.
///
/// Kubełek ma pięć minut, a kryterium `gen_commute_hist` mówi o odchyleniu mediany
/// ≤ 5 % — czyli przy medianie 20 minut o jednej minucie. Mediana odczytana z kubełka
/// miałaby rozdzielczość gorszą od kryterium, które ma mierzyć.
fn mediana_minutowa(zaloga: &[Vec<Pracownik>]) -> u16 {
    let mut licznik = [0u32; 512];
    let mut n = 0u32;
    for lista in zaloga {
        for p in lista {
            licznik[usize::from(p.commute).min(511)] += 1;
            n += 1;
        }
    }
    let polowa = n / 2;
    let mut suma = 0u32;
    for (m, k) in licznik.iter().enumerate() {
        suma += k;
        if suma >= polowa {
            return m as u16;
        }
    }
    0
}

/// Docelowy histogram czasu dojazdu: rozkład logarytmiczno-normalny w promilach.
fn histogram_docelowy(median: u16, sigma_centi: u16) -> [u32; COMMUTE_BINS] {
    let mu = det_math::ln(f64::from(median.max(1)));
    let sigma = f64::from(sigma_centi.max(1)) / 100.0;
    let mut gestosc = [0f64; COMMUTE_BINS];
    let mut suma = 0f64;
    for (i, g) in gestosc.iter_mut().enumerate() {
        let x = (i as f64 + 0.5) * f64::from(COMMUTE_BIN_MIN);
        let z = (det_math::ln(x) - mu) / sigma;
        *g = det_math::exp(-0.5 * z * z) / x;
        suma += *g;
    }
    let mut out = [0u32; COMMUTE_BINS];
    for (i, g) in gestosc.iter().enumerate() {
        out[i] = (g / suma * 1000.0) as u32;
    }
    out
}

/// Korelacja rang Spearmana ×100 dla permutacji „ranga dochodu → ranga wartości".
fn spearman(perm: &[usize]) -> i16 {
    let n = perm.len() as i128;
    if n < 2 {
        return 0;
    }
    let suma_d2: i128 = perm
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let d = i as i128 - *p as i128;
            d * d
        })
        .sum();
    let rho = 100 - (600 * suma_d2) / (n * (n * n - 1));
    rho.clamp(-100, 100) as i16
}

// ── kroki 8 i 9: wiedza i relacje ───────────────────────────────────────────────

/// Krok 8: zasiew wiedzy (§5.7, korekta E-2).
///
/// Bez tego kroku **nic się w mieście nie wydarzy**: `candidates` zwraca wyłącznie
/// miejsca znane mieszkańcowi, więc mieszkaniec bez wpisów dostaje `PlaceUnknown`
/// i nigdzie nie idzie. To jest warunek działania scenariusza, a nie ozdoba.
fn zasiej_wiedze(
    world: &mut World,
    mieszkancy: &[Entity],
    places: &PlaceTable,
    seed: &table::KnowledgeSeed,
) -> u64 {
    /// Rodzaje miejsc, które w ogóle zaspokajają potrzeby (`data/needs/needs.ron`).
    /// `Home` i `Workplace` mieszkaniec zna z definicji — one nie wymagają zasiewu.
    const POTRZEBNE: [PlaceKind; 8] = [
        PlaceKind::Grocery,
        PlaceKind::Eatery,
        PlaceKind::Clothing,
        PlaceKind::Doctor,
        PlaceKind::Pharmacy,
        PlaceKind::Hospital,
        PlaceKind::Leisure,
        PlaceKind::Social,
    ];
    let mut wpisow = 0u64;
    let mut na_trasie: ArrayVec<PlaceRef, MAX_ON_ROUTE> = ArrayVec::new();
    // Sąsiedztwo jest cechą **adresu**, nie mieszkańca: w jednym budynku mieszka
    // kilkanaście osób i wszystkie mają te same sklepy za rogiem. Zapytanie
    // przestrzenne robi się więc raz na budynek, a nie raz na człowieka —
    // przy metropolii to różnica między 19 tys. a 274 tys. zapytań.
    let mut okolica: BTreeMap<u32, Vec<u32>> = BTreeMap::new();

    for c in mieszkancy {
        let Some(dom) = world.get::<Residence>(*c).and_then(home_place) else {
            continue;
        };
        let Some(at) = places.coord_of(dom) else {
            continue;
        };
        let budynek = magnat_agents::knowledge_key(dom).unwrap_or(u32::MAX);

        let blisko = okolica.entry(budynek).or_insert_with(|| {
            // Po najbliższych, nie po wszystkich: magazyn wiedzy ma 32 wpisy,
            // a w centrum w promieniu 400 m bywa ich kilkaset.
            let mut v: Vec<(i64, u32)> = Vec::new();
            for kind in POTRZEBNE {
                places.for_each_near(kind, at, f32::from(seed.home_radius_m), |e| {
                    if let Some(k) = magnat_agents::knowledge_key(e.place) {
                        v.push((e.at.distance_sq_xy(at), k));
                    }
                });
            }
            v.sort_unstable();
            v.truncate(usize::from(seed.max_near_home));
            v.into_iter().map(|(_, k)| k).collect()
        });
        let blisko = blisko.clone();
        for k in &blisko {
            social::learn_place(world, *c, *k, KnowledgeKind::Visited, seed.home_score, 0);
            wpisow += 1;
        }

        // Miejsce pracy i miejsca widoczne z trasy dom↔praca.
        let praca = world
            .get::<Employment>(*c)
            .and_then(|e| site_place(e.site));
        if let Some(p) = praca {
            if let Some(k) = magnat_agents::knowledge_key(p) {
                social::learn_place(world, *c, k, KnowledgeKind::Visited, seed.work_score, 0);
                wpisow += 1;
            }
            for k in korytarz(places, at, p, &mut na_trasie) {
                social::learn_place(world, *c, k, KnowledgeKind::SeenOnRoute, seed.route_score, 0);
                wpisow += 1;
            }
        }
    }
    wpisow
}

/// Miejsca widoczne po drodze dom↔praca, liczone z **korytarza wokół odcinka**,
/// a nie z odtworzonej trasy.
///
/// `ponytail:` `WalkOracle::places_on_route` daje prawdziwą trasę po ulicach, ale kosztuje
/// jedną Dijkstrę na mieszkańca — 90 µs × 270 tys. to 25 s z budżetu 30 s na cały Etap 8
/// (§7.4 `gen_perf`). Sufit nazwany: korytarz nie widzi objazdu wokół rzeki ani wiaduktu,
/// więc mieszkaniec zza mostu pozna czasem sklep, którego nie mija. W **rozgrywce** trasą
/// zajmuje się `places_on_route` i to ona rozdaje wiedzę z codziennych dojazdów; tu chodzi
/// o zasiew startowy, którego i tak nikt nie widział. Prawdziwa trasa wejdzie, kiedy M4
/// zastąpi ten moduł `engine/nav` z routingiem wielokrotnego użytku (K-2).
fn korytarz(
    places: &PlaceTable,
    dom: WorldCoord,
    praca: PlaceRef,
    bufor: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>,
) -> Vec<u32> {
    /// Ile punktów na odcinku dom↔praca próbkować.
    const PROBEK: i32 = 4;
    /// Promień korytarza w metrach — dwie pierzeje.
    const PROMIEN_M: f32 = 150.0;
    bufor.clear();
    let Some(cel) = places.coord_of(praca) else {
        return Vec::new();
    };
    let mut out: Vec<u32> = Vec::new();
    for k in 1..=PROBEK {
        let at = WorldCoord::new(
            dom.x + (cel.x - dom.x) / PROBEK * k,
            dom.y + (cel.y - dom.y) / PROBEK * k,
            0,
        );
        for kind in [PlaceKind::Grocery, PlaceKind::Eatery, PlaceKind::Pharmacy] {
            places.for_each_near(kind, at, PROMIEN_M, |e| {
                if let Some(key) = magnat_agents::knowledge_key(e.place) {
                    out.push(key);
                }
            });
        }
    }
    out.sort_unstable();
    out.dedup();
    out.truncate(MAX_ON_ROUTE);
    out
}

/// Krok 9: relacje startowe — współpracownicy z tego samego zakładu i sąsiedzi
/// z tego samego kwartału.
///
/// Rodzina powstała już przy spawnie (`spawn_household_aged`). Tu dochodzą dwa
/// pozostałe źródła z §5.9: bez nich graf relacji ma same wyspy rodzinne i plotka
/// nie ma po czym chodzić.
fn relacje_startowe(
    world: &mut World,
    mieszkancy: &[Entity],
    s: &table::StartingRelations,
    seed: u64,
) -> u64 {
    let waga_wsp = world
        .resource::<DemographyTable>()
        .social()
        .coworker_max
        .min(60);
    let waga_sas = world
        .resource::<DemographyTable>()
        .social()
        .neighbour_max
        .min(45);

    let mut w_zakladzie: BTreeMap<u32, Vec<Entity>> = BTreeMap::new();
    let mut w_kwartale: BTreeMap<u32, Vec<Entity>> = BTreeMap::new();
    for c in mieszkancy {
        if let Some(e) = world.get::<Employment>(*c) {
            if e.has_job() && e.flags & Employment::FLAG_PUPIL == 0 {
                w_zakladzie.entry(e.site).or_default().push(*c);
            }
        }
        if let Some(r) = world.get::<Residence>(*c) {
            let blok = world.resource::<CityFacts>().block(r.building);
            w_kwartale.entry(blok).or_default().push(*c);
        }
    }

    let mut krawedzi = 0u64;
    for (grupa, kind, waga, lo, hi, tick) in [
        (
            &w_zakladzie,
            RelationKind::Colleague,
            waga_wsp,
            s.coworkers_min,
            s.coworkers_max,
            8u64,
        ),
        (
            &w_kwartale,
            RelationKind::Neighbour,
            waga_sas,
            s.neighbours_min,
            s.neighbours_max,
            9u64,
        ),
    ] {
        for lista in grupa.values() {
            if lista.len() < 2 {
                continue;
            }
            for (i, a) in lista.iter().enumerate() {
                let mut r = rng(seed, StreamId::Relations, a.index(), Tick(tick));
                let ile = u32::from(lo) + r.gen_range_u32(u32::from(hi.saturating_sub(lo)) + 1);
                // Sąsiadami w liście są ci, którzy siedzą obok — kolejność listy jest
                // kolejnością indeksów encji, więc wybór jest deterministyczny i lokalny.
                for k in 1..=ile as usize {
                    let j = (i + k) % lista.len();
                    if j == i {
                        break;
                    }
                    let b = lista[j];
                    if a.index() < b.index() {
                        demography::powiaz(world, *a, b, kind, waga, 0);
                        krawedzi += 1;
                    }
                }
            }
        }
    }
    krawedzi
}

// ── krok 10: weryfikacja ────────────────────────────────────────────────────────

struct Liczby {
    homes_total: u32,
    homes_free: u32,
    jobs_total: u32,
    jobs_free: u32,
    unemp: u16,
    commute_cel: u16,
    commute_sigma: u16,
    places: u32,
    wiedza: u64,
    relacje: u64,
    bez_szkoly: u32,
}

fn raport(
    world: &World,
    mieszkancy: &[Entity],
    gospodarstwa: &[Entity],
    bands: &[u16],
    z: &Zatrudnienie,
    m: &Mieszkania,
    l: Liczby,
) -> PopulationReport {
    let n = mieszkancy.len() as u32;
    let mut piramida = [0u32; AGE_BANDS];
    let mut bezdomnych = 0u32;
    let mut bez_wiedzy = 0u32;
    for c in mieszkancy {
        if world
            .get::<magnat_agents::KnowledgeRef>(*c)
            .is_none_or(|k| k.len == 0)
        {
            bez_wiedzy += 1;
        }
        if let Some(id) = world.get::<Identity>(*c) {
            let b = (id.age_years(0).max(0) as usize / BAND_YEARS as usize).min(AGE_BANDS - 1);
            piramida[b] += 1;
        }
        if world
            .get::<Residence>(*c)
            .is_none_or(|r| r.building == Residence::HOMELESS)
        {
            bezdomnych += 1;
        }
    }

    let bezrobocie = if z.active == 0 {
        0
    } else {
        ((u64::from(z.active - z.employed) * 1000) / u64::from(z.active)) as u16
    };

    PopulationReport {
        citizens: n,
        households: gospodarstwa.len() as u32,
        homes_total: l.homes_total,
        homes_free: l.homes_free,
        jobs_total: l.jobs_total,
        jobs_free: l.jobs_free,
        active: z.active,
        employed: z.employed,
        pupils: z.pupils,
        unemployment_permille: bezrobocie,
        unemployment_target_permille: l.unemp,
        skill_fit_permille: if z.employed == 0 {
            0
        } else {
            ((u64::from(z.fit_ok) * 1000) / u64::from(z.employed)) as u16
        },
        commute_median_min: m.median,
        commute_target_min: m.cel,
        commute_hist: m.hist,
        commute_target_hist: histogram_docelowy(m.cel, l.commute_sigma),
        income_housing_rho_centi: m.rho_centi,
        commute_requested_min: l.commute_cel,
        commute_feasible: m.osiagalne,
        pyramid: piramida,
        pyramid_target: std::array::from_fn(|i| bands.get(i).copied().unwrap_or(0)),
        places: l.places,
        knowledge_entries: l.wiedza,
        relations: l.relacje,
        timings: Vec::new(),
        homeless: bezdomnych,
        pupils_without_school: l.bez_szkoly,
        without_knowledge: bez_wiedzy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pula_wieku_oddaje_najblizszy_niepusty_rocznik() {
        let mut p = PulaWieku::new(110);
        p.add(30);
        p.add(34);
        // Remis rozstrzyga się na korzyść młodszego rocznika — deterministycznie
        // i niezależnie od kolejności wstawiania.
        assert_eq!(p.take_near(32, 18, 110), Some(30));
        assert_eq!(p.take_near(32, 18, 110), Some(34));
        assert_eq!(p.take_near(32, 18, 110), None);
    }

    #[test]
    fn piramida_odtwarza_rozklad() {
        let bands: Vec<u16> = (0..AGE_BANDS)
            .map(|i| if i < 10 { 100 } else { 0 })
            .collect();
        let pula = piramida(7, 20_000, &bands, 110);
        assert_eq!(pula.total, 20_000);
        // Wszyscy poniżej 50 lat, bo dziesięć pierwszych pasm po pięć lat.
        assert_eq!(pula.len_in(50, 110), 0);
        // Każde z dziesięciu pasm ma ~2000 osób (±3 σ ≈ ±135).
        for b in 0..10i32 {
            let n = pula.len_in(b * 5, b * 5 + 4);
            assert!((1700..2300).contains(&n), "pasmo {b}: {n}");
        }
    }

    #[test]
    fn spearman_liczy_zgodnie_z_definicja() {
        assert_eq!(spearman(&[0, 1, 2, 3, 4]), 100);
        assert_eq!(spearman(&[4, 3, 2, 1, 0]), -100);
    }

    #[test]
    fn histogram_docelowy_jest_rozkladem_z_maksimum_przy_medianie() {
        let h = histogram_docelowy(24, 62);
        let suma: u32 = h.iter().sum();
        assert!((980..=1000).contains(&suma), "{suma}");
        let szczyt = h.iter().enumerate().max_by_key(|(_, v)| **v).map(|(i, _)| i);
        // Moda rozkładu log-normalnego leży poniżej mediany — kubełek 2 lub 3 (10–20 min).
        assert!(matches!(szczyt, Some(2..=3)), "{szczyt:?}");
    }

    #[test]
    fn grafik_daje_obsade_weekendowa_w_handlu_i_pelny_tydzien_w_ruchu_ciaglym() {
        let weekendowa = (0..6)
            .map(|i| grafik(SectorId::Retail, ShiftId::II, i).1)
            .any(|m| m & 0b110_0000 != 0);
        assert!(weekendowa, "handel bez obsady weekendowej");
        let pokrycie = (0..4)
            .map(|i| grafik(SectorId::Industry, ShiftId::III, i).1)
            .fold(0u8, |a, b| a | b);
        assert_eq!(pokrycie, 0b111_1111, "ruch ciągły nie pokrywa tygodnia");
        for i in 0..4 {
            assert_eq!(
                grafik(SectorId::Industry, ShiftId::III, i).1.count_ones(),
                5,
                "brygada {i} pracuje inną liczbę dni niż pięć"
            );
        }
        assert_eq!(
            grafik(SectorId::Office, ShiftId::I, 3),
            (ShiftKind::Day, 0b001_1111)
        );
    }
}
