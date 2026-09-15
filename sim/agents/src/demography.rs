//! Cykl życia, hazardy i dziedziczenie (M3c §5.6, WP7).
//!
//! **Hazard roczny stosuje się raz w roku, nie 360 razy po 1/360.** System jest
//! shardowany 1/360 po indeksie encji (§5.6), więc każdy mieszkaniec przechodzi swoje
//! losowania dokładnie raz na 360 dób — w swojej dobie. Daje to tę samą częstość
//! zdarzeń co losowanie dzienne, kosztuje 1/360 pracy i, co ważniejsze, **zużywa jedną
//! wartość z jednego strumienia**: `rng(seed, StreamId::Demography, citizen_idx, day)`.
//! Dodanie albo usunięcie mieszkańca nie przesuwa wtedy losowań pozostałych
//! (`det_demography_independence`).
//!
//! Zdarzenia rozciągnięte w czasie — poród po ciąży, wyzdrowienie po chorobie — nie
//! mieszczą się w tym rytmie, bo mają własny termin. Idą do `LifeQueue`: kalendarza
//! dobowego opartego na `BTreeMap`, czyli tego samego pomysłu co przelew kolejki DES,
//! tylko w skali dób, a nie minut. Kolejki DES nie da się tu użyć wprost: ta liczy
//! w minutach i przewija się minuta po minucie, a tryb demograficzny (`m3_century`)
//! skacze po dobach.
//!
//! Czego tu **nie ma**: migracji (`migration.rs`), statusu, relacji i plotki
//! (`social.rs`), generacji populacji Etapu 8 (M3d). Tu jest to, co się dzieje
//! z mieszkańcem, który już istnieje.

use crate::components::{
    AgentState, Employment, Identity, KnowledgeRef, Lifecycle, Needs, PlanRef, Personality,
    Residence, RelationsRef, Skills, Vitals, Wealth,
};
use crate::household::{self, Household, HouseholdOverflow, MemberView};
use crate::store::{KnowledgeSlab, PlanSlab, Relation, RelationKind, RelationSlab, SlabRef};
use magnat_core::{
    rng, CitizenId, DecisionReason, Entity, HashState, LifeEventKind, Money, NeedKind, Rng,
    StateHasher, StreamId, Tick, Q,
};
use magnat_ecs::{CommandBuffer, SystemId, World};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

pub const DEMOGRAPHY_SCHEMA_VERSION: u32 = 1;

/// Ile dób dzieli dwa losowania hazardów tego samego mieszkańca (§5.6, sharding 1/360).
pub const DEMOGRAPHY_SHARDS: u64 = 360;

/// Dób w roku gry (K-1). Lokalna stała **nie istnieje** — to re-eksport, żeby
/// czytający ten plik nie musiał sprawdzać, czy ktoś tu nie policzył roku po swojemu.
pub const DAYS_PER_YEAR: u64 = magnat_core::time::DAYS_PER_MONTH * magnat_core::time::MONTHS_PER_YEAR;

// ── dane (data/demography/demography.ron) ───────────────────────────────────────

/// Pasmo wieku z roczną stopą na 100 000. Zakres jest domknięty obustronnie.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AgeRate {
    pub from: u8,
    pub to: u8,
    pub per_100k: u32,
}

/// Pasmo wieku z hazardem **miesięcznym** w promilach.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AgeChance {
    pub from: u8,
    pub to: u8,
    pub permille: u16,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Ages {
    pub adult: u8,
    pub escort: u8,
    pub school_start: u8,
    pub school_end: u8,
    pub work_start: u8,
    pub retirement: u8,
    pub senior: u8,
    pub max: u8,
    pub leave_nest: u8,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Range8 {
    pub min: u8,
    pub max: u8,
}

/// Wagi funkcji statusu (§5.8). Suma musi być 100 — inaczej status przestaje być
/// skalą 0..=100 i przedziały klas przestają cokolwiek znaczyć.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct StatusWeights {
    pub income: u16,
    pub wealth: u16,
    pub education: u16,
    pub occupation: u16,
    pub address: u16,
    pub consumption: u16,
    pub family: u16,
}

impl StatusWeights {
    #[must_use]
    pub const fn sum(&self) -> u16 {
        self.income
            + self.wealth
            + self.education
            + self.occupation
            + self.address
            + self.consumption
            + self.family
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MigrationParams {
    pub k_in_permille: u16,
    pub jobless_months_to_leave: u8,
    pub settle_grace_days: u16,
    pub housing_need_threshold: u8,
    pub safety_need_threshold: u8,
    pub need_leave_permille: u16,
    pub jobless_leave_permille: u16,
    pub arrival_sizes: Vec<u8>,
    pub arrival_adult_age_min: u8,
    pub arrival_adult_age_spread: u8,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct SocialParams {
    pub relation_decay_per_week: u8,
    pub coworker_gain_per_week: u8,
    pub coworker_max: u8,
    pub neighbour_gain_per_week: u8,
    pub neighbour_max: u8,
    pub family_weight: u8,
    pub close_contacts: u8,
    pub gossip_targets: u8,
    pub gossip_min_weight: u8,
    pub gossip_score_penalty: u8,
    pub route_score: u8,
}

#[derive(Clone, Debug, Deserialize)]
struct DemographyFile {
    schema_version: u32,
    ages: Ages,
    mortality: Vec<AgeRate>,
    mortality_sick_permille: u32,
    fertility: Vec<AgeRate>,
    fertility_parity_permille: Vec<u32>,
    fertility_single_permille: u32,
    fertility_housing_permille: u32,
    fertility_housing_threshold: u8,
    gestation_days: u16,
    illness: Vec<AgeRate>,
    illness_health_drop: Range8,
    illness_days: Range8,
    illness_hygiene_threshold: u8,
    partnering: Vec<AgeChance>,
    partner_max_status_delta: u8,
    partner_max_age_delta: u8,
    partner_min_relation_weight: u8,
    wedding_min_days: u16,
    wedding_min_weight: u8,
    divorce_base_permille: u16,
    divorce_stress_permille: u16,
    divorce_stress_threshold: u8,
    divorce_debt_permille: u16,
    divorce_debt_to_income: u16,
    divorce_grace_days: u16,
    status_weights: StatusWeights,
    migration: MigrationParams,
    social: SocialParams,
}

/// Tabela hazardów demograficznych — zasób świata, czytany, nigdy zmieniany w biegu.
#[derive(Clone, Debug)]
pub struct DemographyTable {
    f: DemographyFile,
}

#[derive(Debug)]
pub enum DemographyError {
    Io(std::io::Error),
    Ron(String),
    Schema { found: u32, want: u32 },
    AgeGap { at: u8, table: &'static str },
    StatusWeights(u16),
}

impl std::fmt::Display for DemographyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DemographyError::Io(e) => write!(f, "data/demography: {e}"),
            DemographyError::Ron(m) => write!(f, "data/demography: {m}"),
            DemographyError::Schema { found, want } => {
                write!(f, "data/demography: schema_version {found}, oczekiwano {want}")
            }
            DemographyError::AgeGap { at, table } => write!(
                f,
                "data/demography: tabela {table} nie pokrywa wieku {at} — hazard byłby \
                 po cichu zerowy"
            ),
            DemographyError::StatusWeights(s) => write!(
                f,
                "data/demography: wagi statusu sumują się do {s}, a mają do 100"
            ),
        }
    }
}

impl std::error::Error for DemographyError {}

impl From<std::io::Error> for DemographyError {
    fn from(e: std::io::Error) -> Self {
        DemographyError::Io(e)
    }
}

impl DemographyTable {
    /// Wczytanie i **walidacja przed użyciem** (00 §5).
    ///
    /// Dziura w pokryciu wieku jest błędem ładowania, nie stanem do obsłużenia:
    /// hazard zerowy dla pasma 45–54 wygląda w symulacji jak dekada nieśmiertelności
    /// i wychodzi dopiero po dziesięciu latach gry.
    pub fn load(path: &Path) -> Result<DemographyTable, DemographyError> {
        let txt = std::fs::read_to_string(path)?;
        let f: DemographyFile =
            ron::from_str(&txt).map_err(|e| DemographyError::Ron(e.to_string()))?;
        if f.schema_version != DEMOGRAPHY_SCHEMA_VERSION {
            return Err(DemographyError::Schema {
                found: f.schema_version,
                want: DEMOGRAPHY_SCHEMA_VERSION,
            });
        }
        if f.status_weights.sum() != 100 {
            return Err(DemographyError::StatusWeights(f.status_weights.sum()));
        }
        pokrycie(&f.mortality, 0, f.ages.max, "mortality")?;
        pokrycie(&f.illness, 0, f.ages.max, "illness")?;
        Ok(DemographyTable { f })
    }

    pub fn load_default() -> Result<DemographyTable, DemographyError> {
        DemographyTable::load(&magnat_core::data_path("demography/demography.ron"))
    }

    #[inline]
    #[must_use]
    pub fn ages(&self) -> Ages {
        self.f.ages
    }

    #[inline]
    #[must_use]
    pub fn status_weights(&self) -> StatusWeights {
        self.f.status_weights
    }

    #[inline]
    #[must_use]
    pub fn migration(&self) -> &MigrationParams {
        &self.f.migration
    }

    #[inline]
    #[must_use]
    pub fn social(&self) -> SocialParams {
        self.f.social
    }

    #[inline]
    #[must_use]
    pub fn gestation_days(&self) -> u16 {
        self.f.gestation_days
    }

    /// Roczny hazard zgonu na 100 000, po korekcie o zdrowie.
    #[must_use]
    pub fn mortality_per_100k(&self, age_years: i32, health: Q) -> u32 {
        let baza = stopa(&self.f.mortality, age_years);
        // Zdrowie 100 → ×1,0, zdrowie 0 → ×`mortality_sick_permille`/1000. Liniowo.
        let mnoznik = 1000
            + (self.f.mortality_sick_permille.saturating_sub(1000))
                * u32::from(100 - health.get().min(100))
                / 100;
        (u64::from(baza) * u64::from(mnoznik) / 1000).min(100_000) as u32
    }

    /// Roczny hazard zachorowania na 100 000.
    #[must_use]
    pub fn illness_per_100k(&self, age_years: i32, hygiene: Q) -> u32 {
        let baza = stopa(&self.f.illness, age_years);
        if hygiene.get() < self.f.illness_hygiene_threshold {
            (baza + baza / 2).min(100_000)
        } else {
            baza.min(100_000)
        }
    }

    /// Roczny hazard urodzenia dziecka na 100 000 kobiet, po korektach (§5.6).
    #[must_use]
    pub fn fertility_per_100k(
        &self,
        age_years: i32,
        children: usize,
        partnered: bool,
        housing: Q,
    ) -> u32 {
        let mut v = u64::from(stopa(&self.f.fertility, age_years));
        if v == 0 {
            return 0;
        }
        let parity = self
            .f
            .fertility_parity_permille
            .get(children)
            .or_else(|| self.f.fertility_parity_permille.last())
            .copied()
            .unwrap_or(1000);
        v = v * u64::from(parity) / 1000;
        if !partnered {
            v = v * u64::from(self.f.fertility_single_permille) / 1000;
        }
        if housing.get() < self.f.fertility_housing_threshold {
            v = v * u64::from(self.f.fertility_housing_permille) / 1000;
        }
        v.min(100_000) as u32
    }

    /// Miesięczny hazard wejścia w związek, w promilach.
    #[must_use]
    pub fn partnering_permille(&self, age_years: i32) -> u16 {
        for b in &self.f.partnering {
            if age_years >= i32::from(b.from) && age_years <= i32::from(b.to) {
                return b.permille;
            }
        }
        0
    }

    /// Miesięczny hazard rozstania, w promilach (§5.6).
    #[must_use]
    pub fn divorce_permille(&self, stress: Q, debt: Money, income_monthly: Money) -> u16 {
        let mut p = self.f.divorce_base_permille;
        if stress.get() >= self.f.divorce_stress_threshold {
            p += self.f.divorce_stress_permille;
        }
        let prog = income_monthly
            .checked_mul_int(i64::from(self.f.divorce_debt_to_income))
            .unwrap_or(Money(i64::MAX));
        if income_monthly.get() > 0 && debt.get() > prog.get() {
            p += self.f.divorce_debt_permille;
        }
        p
    }

    #[must_use]
    pub fn partner_limits(&self) -> (u8, u8, u8) {
        (
            self.f.partner_max_status_delta,
            self.f.partner_max_age_delta,
            self.f.partner_min_relation_weight,
        )
    }

    #[must_use]
    pub fn wedding_rules(&self) -> (u16, u8) {
        (self.f.wedding_min_days, self.f.wedding_min_weight)
    }

    #[must_use]
    pub fn divorce_grace_days(&self) -> u16 {
        self.f.divorce_grace_days
    }
}

fn pokrycie(t: &[AgeRate], from: u8, to: u8, name: &'static str) -> Result<(), DemographyError> {
    for wiek in from..=to {
        if !t
            .iter()
            .any(|b| wiek >= b.from && wiek <= b.to)
        {
            return Err(DemographyError::AgeGap { at: wiek, table: name });
        }
    }
    Ok(())
}

fn stopa(t: &[AgeRate], age_years: i32) -> u32 {
    if age_years < 0 {
        return 0;
    }
    let a = age_years.min(255) as u8;
    for b in t {
        if a >= b.from && a <= b.to {
            return b.per_100k;
        }
    }
    0
}

// ── rejestr populacji ───────────────────────────────────────────────────────────

/// Spis żywych mieszkańców i gospodarstw plus księga zmian.
///
/// Spis, a nie zapytanie ECS, z dwóch powodów. Po pierwsze **kolejność**: archetypowy
/// ECS oddaje encje w kolejności chunków, a ta zależy od historii mutacji
/// strukturalnych; demografia musi iterować po rosnącym indeksie encji, bo to on jest
/// argumentem strumienia RNG. Po drugie **księga**: `prop_population_identity` żąda,
/// żeby dla każdej doby `pop(t+1) = pop(t) + narodziny − zgony + napływ − odpływ`
/// zgadzało się co do osoby, a to jest stwierdzenie o czterech licznikach, nie
/// o rozmiarze tabeli.
#[derive(Clone, Debug, Default)]
pub struct Population {
    citizens: Vec<Entity>,
    households: Vec<Entity>,
    pub births: u64,
    pub deaths: u64,
    pub arrivals: u64,
    pub departures: u64,
    /// Pieniądz, który wyjechał z miasta razem z mieszkańcem.
    pub emigrated: Money,
    /// Pieniądz po zmarłym bez spadkobierców. Nie znika — inaczej test zachowania
    /// pieniądza (00 §6) miałby dziurę wielkości jednego pokolenia.
    pub escheat: Money,
}

impl Population {
    #[must_use]
    pub fn new() -> Population {
        Population::default()
    }

    #[must_use]
    pub fn citizens(&self) -> &[Entity] {
        &self.citizens
    }

    #[must_use]
    pub fn households(&self) -> &[Entity] {
        &self.households
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.citizens.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.citizens.is_empty()
    }

    /// Tożsamość księgowa doby: stan początkowy plus bilans = stan bieżący.
    #[must_use]
    pub fn identity_holds(&self, start: u64) -> bool {
        let saldo = start as i128 + self.births as i128 + self.arrivals as i128
            - self.deaths as i128
            - self.departures as i128;
        saldo == self.citizens.len() as i128
    }

    pub fn add_citizen(&mut self, e: Entity) {
        if let Err(i) = self.citizens.binary_search_by_key(&e.index(), |x| x.index()) {
            self.citizens.insert(i, e);
        }
    }

    pub fn remove_citizen(&mut self, e: Entity) {
        if let Ok(i) = self.citizens.binary_search_by_key(&e.index(), |x| x.index()) {
            self.citizens.remove(i);
        }
    }

    pub fn add_household(&mut self, e: Entity) {
        if let Err(i) = self.households.binary_search_by_key(&e.index(), |x| x.index()) {
            self.households.insert(i, e);
        }
    }

    pub fn remove_household(&mut self, e: Entity) {
        if let Ok(i) = self.households.binary_search_by_key(&e.index(), |x| x.index()) {
            self.households.remove(i);
        }
    }

    /// Odbudowa spisu ze świata — dla scenariuszy, które zaludniają go same
    /// (Etap 8 w M3d, testy). Po niej spis jest posortowany po indeksie encji.
    pub fn rebuild(&mut self, world: &World) {
        self.citizens.clear();
        self.households.clear();
        let mut c: Vec<Entity> = Vec::new();
        let mut h: Vec<Entity> = Vec::new();
        for a in world.archetypes().iter() {
            for chunk in a.chunks() {
                for e in chunk.entities() {
                    if world.get::<Identity>(*e).is_some_and(Identity::is_alive) {
                        c.push(*e);
                    } else if world.has::<Household>(*e) {
                        h.push(*e);
                    }
                }
            }
        }
        c.sort_unstable_by_key(|e| e.index());
        h.sort_unstable_by_key(|e| e.index());
        self.citizens = c;
        self.households = h;
    }
}

impl HashState for Population {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.citizens.len() as u64);
        for e in &self.citizens {
            h.write_u32(e.index());
        }
        h.write_u64(self.households.len() as u64);
        for e in &self.households {
            h.write_u32(e.index());
        }
        h.write_u64(self.births);
        h.write_u64(self.deaths);
        h.write_u64(self.arrivals);
        h.write_u64(self.departures);
        self.emigrated.hash_state(h);
        self.escheat.hash_state(h);
    }
}

// ── kalendarz zdarzeń życiowych ─────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum LifeTaskKind {
    /// Koniec ciąży.
    Birth = 0,
    /// Koniec choroby.
    Recover = 1,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct LifeTask {
    pub kind: LifeTaskKind,
    pub actor: u32,
}

/// Terminarz zdarzeń o własnej dacie: poród, wyzdrowienie.
///
/// `BTreeMap` po dobie; w obrębie doby zadania sortują się po `(kind, actor)`, czyli
/// tak samo jak zdarzenia DES po `order_key` — porządek totalny niezależny od kolejności
/// wstawiania (00 §3.3).
#[derive(Clone, Debug, Default)]
pub struct LifeQueue {
    by_day: BTreeMap<u32, Vec<LifeTask>>,
    len: usize,
}

impl LifeQueue {
    #[must_use]
    pub fn new() -> LifeQueue {
        LifeQueue::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn schedule(&mut self, day: u64, task: LifeTask) {
        self.by_day.entry(day as u32).or_default().push(task);
        self.len += 1;
    }

    /// Zadania na dobę `day`, w porządku totalnym. Zadania z dób **wcześniejszych**
    /// też wychodzą: tryb demograficzny skacze po dobach, a zadanie pominięte
    /// zostałoby w mapie na zawsze i wisiało w hashu stanu.
    pub fn take_due(&mut self, day: u64, out: &mut Vec<LifeTask>) {
        out.clear();
        let granica = day as u32;
        while let Some((&d, _)) = self.by_day.iter().next() {
            if d > granica {
                break;
            }
            let v = self.by_day.remove(&d).unwrap_or_default();
            self.len -= v.len();
            out.extend(v);
        }
        out.sort_unstable();
    }

    /// Usuwa wszystkie zadania aktora — wołane przy zgonie, żeby martwa ciężarna
    /// nie urodziła za trzy miesiące.
    pub fn cancel(&mut self, actor: u32) {
        let mut puste: Vec<u32> = Vec::new();
        for (d, v) in &mut self.by_day {
            let przed = v.len();
            v.retain(|t| t.actor != actor);
            self.len -= przed - v.len();
            if v.is_empty() {
                puste.push(*d);
            }
        }
        for d in puste {
            self.by_day.remove(&d);
        }
    }
}

impl HashState for LifeQueue {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.len as u64);
        for (d, v) in &self.by_day {
            h.write_u32(*d);
            let mut s = v.clone();
            s.sort_unstable();
            h.write_u64(s.len() as u64);
            for t in s {
                h.write_u8(t.kind as u8);
                h.write_u32(t.actor);
            }
        }
    }
}

// ── hook dziedziczenia (§5.6, decyzja 9.10) ─────────────────────────────────────

/// Rejestracja aktywów, których M3 nie zna.
///
/// M3 dzieli **wyłącznie pieniądz i mieszkanie**. M7 rejestruje tu udziały w firmach,
/// M5 długi i rachunki, M10 aktywa giełdowe. Permile sumują się do 1000; reszta
/// z dzielenia idzie do pierwszego spadkobiercy wg `entity_index` — ta sama reguła,
/// co przy podziale kwoty w 00 §2.
pub trait InheritanceHook: Send + Sync {
    /// Wołany po podziale `Money` i mieszkania, **przed** usunięciem encji zmarłego.
    fn on_inheritance(
        &mut self,
        deceased: CitizenId,
        heirs: &[(CitizenId, u16)],
        cmd: &mut CommandBuffer,
    );
}

/// Hook, który nic nie robi — domyślny do czasu M5/M7.
pub struct NoInheritance;

impl InheritanceHook for NoInheritance {
    fn on_inheritance(&mut self, _d: CitizenId, _h: &[(CitizenId, u16)], _c: &mut CommandBuffer) {}
}

// ── raport doby ─────────────────────────────────────────────────────────────────

/// Co się wydarzyło w tej dobie. Wejście do raportu `m3_century` i do testów.
///
/// `reasons` niesie **uzasadnienia** (00 §7): karta inspekcji M3d bierze je stąd,
/// a nie z logu per mieszkaniec. Lista jest krótka z natury rzeczy — hazardy dotykają
/// 1/360 populacji na dobę, a zdarza się z tego kilka promili — więc `Vec` nie jest
/// tu kosztem, tylko najprostszą rzeczą, która działa.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DayReport {
    pub births: u32,
    pub deaths: u32,
    pub conceptions: u32,
    pub illnesses: u32,
    pub recoveries: u32,
    pub retirements: u32,
    pub reasons: Vec<(u32, DecisionReason)>,
}

/// Co się wydarzyło w tym miesiącu.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MonthReport {
    pub partnerships: u32,
    pub weddings: u32,
    pub separations: u32,
    pub left_nest: u32,
    pub reasons: Vec<(u32, DecisionReason)>,
}

/// Klucz strumienia RNG w obrębie doby: `doba × 8 + numer losowania`.
///
/// Osiem miejsc na dobę, bo hazardy dobowe i miesięczne trafiają w te same doby
/// (miesiąc zaczyna się co 30 dób, shard mieszkańca wypada co 360) i bez rozdzielenia
/// czerpałyby z **tego samego** generatora — czyli decyzja o rozwodzie zależałaby od
/// tego, czy tego dnia wypadło też losowanie zgonu. Numery są przypisane raz:
/// 0 hazardy dobowe, 1 poród, 2 dobór partnera, 3 rozstanie, 4 wyprowadzka z domu,
/// 5 napływ, 6 plotka, 7 odpływ z powodu bezrobocia. Mnożnik 8 jest do podniesienia
/// przez fazę, której zabraknie miejsc — zmienia wtedy **każdy** świat z tego ziarna,
/// więc robi się to raz i świadomie, a nie przy okazji.
#[inline]
#[must_use]
pub const fn stream_key(day: u64, slot: u64) -> Tick {
    Tick(day * 8 + slot)
}

pub const K_DAILY: u64 = 0;
pub const K_BIRTH: u64 = 1;
pub const K_PARTNER: u64 = 2;
pub const K_SEPARATION: u64 = 3;
pub const K_NEST: u64 = 4;
pub const K_MIGRATION: u64 = 5;
pub const K_GOSSIP: u64 = 6;
pub const K_JOBLESS: u64 = 7;

/// Identyfikator systemu dla buforów komend tej podfazy (00 §3.4: sortowanie po
/// `(SystemId, entity_index)`). Jedna nazwa, a nie `SystemId::from_name` w każdym
/// wywołaniu — kolejność aplikacji komend jest wtedy funkcją nazwy, nie miejsca w kodzie.
#[must_use]
pub fn demography_system_id() -> SystemId {
    SystemId::from_name("agents.Demography")
}

// ── doba ────────────────────────────────────────────────────────────────────────

/// Jedna doba demografii (§5.6).
///
/// Kolejność jest istotna i jest kontraktem: najpierw terminarz (porody, wyzdrowienia),
/// potem hazardy shardu. Odwrotna kolejność kazałaby ciężarnej, która dziś umiera,
/// najpierw urodzić — co jest może i realistyczne, ale przestaje być deterministyczne,
/// bo zależy od tego, czy jej doba porodu wypadła w jej dobie shardu.
pub fn step_day(
    world: &mut World,
    day: u64,
    hooks: &mut dyn InheritanceHook,
) -> DayReport {
    let mut raport = DayReport::default();
    terminarz(world, day, &mut raport);
    hazardy(world, day, hooks, &mut raport);
    raport
}

fn terminarz(world: &mut World, day: u64, raport: &mut DayReport) {
    let mut zadania: Vec<LifeTask> = Vec::new();
    world.resource_mut::<LifeQueue>().take_due(day, &mut zadania);
    for t in zadania {
        let Some(actor) = encja(world, t.actor) else {
            continue;
        };
        match t.kind {
            LifeTaskKind::Birth => {
                if uroda(world, actor, day) {
                    raport.births += 1;
                    raport.reasons.push((
                        t.actor,
                        DecisionReason::LifeEvent {
                            kind: LifeEventKind::Born,
                        },
                    ));
                }
            }
            LifeTaskKind::Recover => {
                if let Some(l) = world.get_mut::<Lifecycle>(actor) {
                    l.flags &= !Lifecycle::FLAG_ILL;
                    raport.recoveries += 1;
                    raport.reasons.push((
                        t.actor,
                        DecisionReason::LifeEvent {
                            kind: LifeEventKind::Recovered,
                        },
                    ));
                }
            }
        }
    }
}

fn hazardy(
    world: &mut World,
    day: u64,
    hooks: &mut dyn InheritanceHook,
    raport: &mut DayReport,
) {
    let shard = day % DEMOGRAPHY_SHARDS;
    // `ponytail:` shard jest filtrem w przebiegu po spisie, a nie 360 kubełkami.
    // Sufit znany: jedno dzielenie na mieszkańca na dobę, czyli przy 400 tys.
    // mieszkańców kilka milisekund na dobę gry. Kubełki per shard opłacą się dopiero,
    // gdyby tryb 50× z M12 mierzył się z tym samym rachunkiem co tryb demograficzny.
    let dzisiaj: Vec<Entity> = world
        .resource::<Population>()
        .citizens()
        .iter()
        .copied()
        .filter(|e| u64::from(e.index()) % DEMOGRAPHY_SHARDS == shard)
        .collect();
    if dzisiaj.is_empty() {
        return;
    }

    let tabela = world.resource::<DemographyTable>().clone();
    let ages = tabela.ages();
    let mut zgony: Vec<Entity> = Vec::new();

    for e in dzisiaj {
        let Some(identity) = world.get::<Identity>(e).copied() else {
            continue;
        };
        if !identity.is_alive() {
            continue;
        }
        let wiek = identity.age_years(day as i32);
        let vitals = world.get::<Vitals>(e).copied().unwrap_or_default();
        let needs = world.get::<Needs>(e).copied().unwrap_or_default();
        let mut r = rng(
            world.seed,
            StreamId::Demography,
            e.index(),
            stream_key(day, K_DAILY),
        );

        // 1. Zgon. Wiek ponad sufit tabeli jest pewny — hazard sam z siebie zostawia
        // ogon, a `prop_no_immortals` nie przyjmuje ogona.
        let hazard = if wiek >= i32::from(ages.max) {
            100_000
        } else {
            tabela.mortality_per_100k(wiek, vitals.health_q())
        };
        if r.gen_range_u32(100_000) < hazard {
            zgony.push(e);
            continue;
        }

        // 2. Emerytura — na najbliższej dobie shardu po przekroczeniu progu.
        let na_emeryturze = world
            .get::<Employment>(e)
            .is_some_and(|x| x.flags & Employment::FLAG_RETIRED != 0);
        if wiek >= i32::from(ages.retirement) && !na_emeryturze {
            // Etat wraca do puli **zanim** zniknie z komponentu — inaczej miasto traci
            // miejsce pracy przy każdej emeryturze i pojemność rynku maleje z każdym
            // pokoleniem (korekta G-7).
            crate::migration::release_job_of(world, e);
            if let Some(emp) = world.get_mut::<Employment>(e) {
                emp.flags |= Employment::FLAG_RETIRED;
                // Emeryt nie jest bezrobotny: `release_job_of` podnosi flagę
                // bezrobocia dla każdego, kto oddaje etat, a tu jest ona nieprawdą.
                emp.flags &= !Employment::FLAG_UNEMPLOYED;
                emp.site = Employment::NO_SITE;
                emp.work_days = 0;
            }
            if let Some(l) = world.get_mut::<Lifecycle>(e) {
                l.flags |= Lifecycle::FLAG_RETIRED;
            }
            raport.retirements += 1;
            raport.reasons.push((
                e.index(),
                DecisionReason::LifeEvent {
                    kind: LifeEventKind::Retired,
                },
            ));
        }

        // 3. Choroba. C-9: choroba **obniża `Health`** zamiast dokładać flagę do
        // `PlanCtx` — wyzwalacz „chory → wizyta u lekarza" w planerze już czyta
        // `Needs[Health] < 30`, więc nie trzeba niczego podłączać.
        let chory = world
            .get::<Lifecycle>(e)
            .is_some_and(|l| l.flags & Lifecycle::FLAG_ILL != 0);
        if !chory {
            let h = tabela.illness_per_100k(wiek, needs.get(NeedKind::Hygiene));
            if r.gen_range_u32(100_000) < h {
                zachoruj(world, e, day, &tabela, &mut r);
                raport.illnesses += 1;
                raport.reasons.push((
                    e.index(),
                    DecisionReason::LifeEvent {
                        kind: LifeEventKind::FellIll,
                    },
                ));
            }
        }

        // 4. Poczęcie.
        if !identity.is_male() && poczecie(world, e, day, wiek, &tabela, &mut r) {
            raport.conceptions += 1;
            raport.reasons.push((
                e.index(),
                DecisionReason::LifeEvent {
                    kind: LifeEventKind::Conceived,
                },
            ));
        }
    }

    if !zgony.is_empty() {
        raport.deaths += zgony.len() as u32;
        let mut cmd = CommandBuffer::new(demography_system_id());
        for e in zgony {
            raport.reasons.push((
                e.index(),
                DecisionReason::LifeEvent {
                    kind: LifeEventKind::Died,
                },
            ));
            for (h, permille) in smierc(world, e, day, hooks, &mut cmd) {
                raport.reasons.push((h, permille));
            }
        }
        magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
    }
}

fn encja(world: &World, index: u32) -> Option<Entity> {
    let p = world.resource::<Population>();
    p.citizens
        .binary_search_by_key(&index, |x| x.index())
        .ok()
        .map(|i| p.citizens[i])
}

fn zachoruj(world: &mut World, e: Entity, day: u64, tabela: &DemographyTable, r: &mut Rng) {
    let f = &tabela.f;
    let spadek = f.illness_health_drop.min
        + r.gen_range_u32(u32::from(
            f.illness_health_drop.max - f.illness_health_drop.min + 1,
        )) as u8;
    let dni = u64::from(
        f.illness_days.min
            + r.gen_range_u32(u32::from(f.illness_days.max - f.illness_days.min + 1)) as u8,
    );
    if let Some(n) = world.get_mut::<Needs>(e) {
        let teraz = n.get(NeedKind::Health);
        n.set(NeedKind::Health, teraz.saturating_sub(spadek));
    }
    if let Some(v) = world.get_mut::<Vitals>(e) {
        v.health = v.health.saturating_sub(spadek / 2);
    }
    if let Some(l) = world.get_mut::<Lifecycle>(e) {
        l.flags |= Lifecycle::FLAG_ILL;
    }
    world.resource_mut::<LifeQueue>().schedule(
        day + dni,
        LifeTask {
            kind: LifeTaskKind::Recover,
            actor: e.index(),
        },
    );
}

fn poczecie(
    world: &mut World,
    e: Entity,
    day: u64,
    wiek: i32,
    tabela: &DemographyTable,
    r: &mut Rng,
) -> bool {
    let l = world.get::<Lifecycle>(e).copied().unwrap_or_default();
    if l.flags & Lifecycle::FLAG_PREGNANT != 0 {
        return false;
    }
    let partnered = l.partner != Lifecycle::NO_PARTNER;
    let hh_idx = world.get::<Identity>(e).map_or(u32::MAX, |i| i.household);
    let dzieci = liczba_dzieci(world, hh_idx, tabela.ages().adult, day);
    let housing = world
        .get::<Needs>(e)
        .map_or(Q::MAX, |n| n.get(NeedKind::Housing));
    let h = tabela.fertility_per_100k(wiek, dzieci, partnered, housing);
    if h == 0 || r.gen_range_u32(100_000) >= h {
        return false;
    }
    if let Some(l) = world.get_mut::<Lifecycle>(e) {
        l.flags |= Lifecycle::FLAG_PREGNANT;
    }
    world.resource_mut::<LifeQueue>().schedule(
        day + u64::from(tabela.gestation_days()),
        LifeTask {
            kind: LifeTaskKind::Birth,
            actor: e.index(),
        },
    );
    true
}

fn liczba_dzieci(world: &World, household: u32, adult_age: u8, day: u64) -> usize {
    let Some(hh_e) = encja_gospodarstwa(world, household) else {
        return 0;
    };
    let Some(hh) = world.get::<Household>(hh_e) else {
        return 0;
    };
    let ov = world.resource::<HouseholdOverflow>();
    household::members_of(household, hh, ov)
        .iter()
        .filter(|m| {
            encja(world, **m)
                .and_then(|c| world.get::<Identity>(c))
                .is_some_and(|i| i.age_years(day as i32) < i32::from(adult_age))
        })
        .count()
}

fn encja_gospodarstwa(world: &World, index: u32) -> Option<Entity> {
    let p = world.resource::<Population>();
    p.households
        .binary_search_by_key(&index, |x| x.index())
        .ok()
        .map(|i| p.households[i])
}

// ── narodziny ───────────────────────────────────────────────────────────────────

/// Poród. Dziecko dostaje komplet trzynastu komponentów mieszkańca, gospodarstwo matki
/// i relacje rodzinne — nic ponadto: pracy szuka M7, plan dnia ułoży mu planer.
fn uroda(world: &mut World, matka: Entity, day: u64) -> bool {
    let Some(l) = world.get_mut::<Lifecycle>(matka) else {
        return false;
    };
    if l.flags & Lifecycle::FLAG_PREGNANT == 0 {
        return false;
    }
    l.flags &= !Lifecycle::FLAG_PREGNANT;

    let Some(m_id) = world.get::<Identity>(matka).copied() else {
        return false;
    };
    let hh_idx = m_id.household;
    let Some(hh_e) = encja_gospodarstwa(world, hh_idx) else {
        return false;
    };
    let hh = world.get::<Household>(hh_e).copied().unwrap_or_default();
    let ojciec = world
        .get::<Lifecycle>(matka)
        .map(|l| l.partner)
        .filter(|p| *p != Lifecycle::NO_PARTNER);

    let mut r = rng(
        world.seed,
        StreamId::Demography,
        matka.index(),
        stream_key(day, K_BIRTH),
    );
    let plec = if r.gen_bool_permille(500) {
        Identity::FLAG_MALE
    } else {
        0
    };
    let cechy: [u8; 8] = std::array::from_fn(|_| r.gen_q().get());

    // Imię z puli **regionu rodziny**, odczytanego z dziedziczonego nazwiska (§5.10):
    // dziecko Schmidtów nie nazywa się Agnieszka, a `Identity` nie rośnie o pole regionu.
    let nazwy = crate::names::catalog();
    let imie = nazwy.pick_first(
        nazwy.region_of_surname(m_id.last_name),
        plec == Identity::FLAG_MALE,
        &mut r,
    );

    let dziecko = world
        .spawn()
        .with(Identity {
            first_name: imie,
            last_name: m_id.last_name,
            birth_day: day as i32,
            birth_district: hh.district,
            flags: Identity::FLAG_ALIVE | plec,
            _pad: 0,
            household: hh_idx,
        })
        .with(Personality(cechy))
        .with(Vitals {
            health: 90 + r.gen_range_u32(11) as u8,
            energy: 100,
            ..Vitals::default()
        })
        .with(Needs {
            level: [100; magnat_core::NEED_COUNT],
            updated_at: (day * 1440) as u32,
        })
        .with(Skills::default())
        .with(Wealth::default())
        .with(Employment::default())
        .with(Residence {
            building: hh.building,
            unit: hh.unit,
            district: hh.district,
        })
        .with(PlanRef::default())
        .with(AgentState::default())
        .with(KnowledgeRef::default())
        .with(RelationsRef::default())
        .with(Lifecycle::default())
        .id();

    world.resource_mut::<Population>().add_citizen(dziecko);
    world.resource_mut::<Population>().births += 1;

    // Miejsce w gospodarstwie; brak miejsca (ponad `HH_MAX_MEMBERS`) znaczy, że
    // dziecko i tak mieszka z rodzicami — składu się wtedy nie powiększa, ale
    // `Identity.household` zostaje, więc `prop_no_orphan_household` widzi spójność.
    let mut hh_kopia = world.get::<Household>(hh_e).copied().unwrap_or_default();
    {
        let ov = world.resource_mut::<HouseholdOverflow>();
        household::add_member(hh_idx, &mut hh_kopia, ov, dziecko.index());
    }
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh_kopia;
    }

    // Relacje rodzinne — obustronne z definicji (`prop_relation_symmetry`).
    let waga = world.resource::<DemographyTable>().social().family_weight;
    powiaz(world, matka, dziecko, RelationKind::Child, waga, day);
    if let Some(o) = ojciec.and_then(|i| encja(world, i)) {
        powiaz(world, o, dziecko, RelationKind::Child, waga, day);
    }
    for m in household::members_of(
        hh_idx,
        &hh_kopia,
        world.resource::<HouseholdOverflow>(),
    )
    .iter()
    .copied()
    .collect::<Vec<u32>>()
    {
        if m == dziecko.index() || Some(m) == ojciec || m == matka.index() {
            continue;
        }
        let Some(inny) = encja(world, m) else { continue };
        let rodzenstwo = world
            .get::<Identity>(inny)
            .is_some_and(|i| i.household == hh_idx);
        if rodzenstwo {
            powiaz(world, inny, dziecko, RelationKind::Sibling, waga, day);
        }
    }
    true
}

/// Zawiązuje relację obustronnie. Strona `b` dostaje relację odwrotną: rodzic widzi
/// dziecko, dziecko rodzica — `prop_relation_symmetry` sprawdza obie strony.
pub fn powiaz(
    world: &mut World,
    a: Entity,
    b: Entity,
    kind: RelationKind,
    weight: u8,
    day: u64,
) {
    wpisz_relacje(world, a, b, kind, weight, day);
    wpisz_relacje(world, b, a, odwrotna(kind), weight, day);
}

const fn odwrotna(k: RelationKind) -> RelationKind {
    match k {
        RelationKind::Parent => RelationKind::Child,
        RelationKind::Child => RelationKind::Parent,
        other => other,
    }
}

fn wpisz_relacje(
    world: &mut World,
    kto: Entity,
    z_kim: Entity,
    kind: RelationKind,
    weight: u8,
    day: u64,
) {
    let Some(rel) = world.get::<RelationsRef>(kto).copied() else {
        return;
    };
    let mut r = SlabRef {
        handle: rel.handle,
        len: rel.len,
        class: rel.class,
    };
    if rel.len == 0 && rel.handle == 0 {
        r = SlabRef::EMPTY;
    }
    let dzis = (day % 65_536) as u16;
    let slab = world.resource_mut::<RelationSlab>();
    if let Some(istnieje) = slab
        .entries_mut(r)
        .iter_mut()
        .find(|x| x.other == z_kim.index())
    {
        istnieje.weight = istnieje.weight.max(weight);
        istnieje.last_contact_day = dzis;
        return;
    }
    slab.push(
        &mut r,
        Relation {
            other: z_kim.index(),
            kind: kind as u8,
            weight,
            last_contact_day: dzis,
        },
        |wpisy| {
            // Wypycha najsłabszą relację, a przy remisie najstarszy kontakt.
            wpisy
                .iter()
                .enumerate()
                .min_by_key(|(i, w)| (w.weight, w.last_contact_day, *i))
                .map_or(0, |(i, _)| i)
        },
    );
    if let Some(slot) = world.get_mut::<RelationsRef>(kto) {
        slot.handle = r.handle;
        slot.len = r.len;
        slot.class = r.class;
    }
}

/// Uchwyt relacji mieszkańca jako `SlabRef`. Jedno miejsce konwersji — komponent
/// i slab mają trzy te same pola, ale `SlabRef::EMPTY` jest sentinelem, a
/// `RelationsRef::default()` zerem.
#[must_use]
pub fn relations_ref(r: &RelationsRef) -> SlabRef {
    if r.len == 0 {
        SlabRef::EMPTY
    } else {
        SlabRef {
            handle: r.handle,
            len: r.len,
            class: r.class,
        }
    }
}

/// Uchwyt wiedzy mieszkańca jako `SlabRef`.
#[must_use]
pub fn knowledge_ref(k: &KnowledgeRef) -> SlabRef {
    if k.len == 0 {
        SlabRef::EMPTY
    } else {
        SlabRef {
            handle: k.handle,
            len: k.len,
            class: k.class,
        }
    }
}

// ── zgon i dziedziczenie ────────────────────────────────────────────────────────

/// Zgon: podział majątku, zwolnienie miejsca, usunięcie relacji, despawn.
///
/// Kolejność nie jest dowolna. Spadkobierców szuka się **przed** usunięciem relacji,
/// bo to relacje mówią, kto jest dzieckiem; hook woła się **przed** despawnem, bo M7
/// musi zdążyć przeczytać udziały zmarłego (§5.6).
fn smierc(
    world: &mut World,
    e: Entity,
    day: u64,
    hooks: &mut dyn InheritanceHook,
    cmd: &mut CommandBuffer,
) -> Vec<(u32, DecisionReason)> {
    let mut powody: Vec<(u32, DecisionReason)> = Vec::new();
    let spadkobiercy = spadkobiercy(world, e);
    let majatek = world.get::<Wealth>(e).copied().unwrap_or_default();
    let suma = majatek
        .cash
        .checked_add(majatek.personal_assets)
        .unwrap_or(majatek.cash);

    if spadkobiercy.is_empty() {
        world.resource_mut::<Population>().escheat = world
            .resource::<Population>()
            .escheat
            .checked_add(suma)
            .unwrap_or(Money(i64::MAX));
    } else {
        let wagi: Vec<u64> = vec![1; spadkobiercy.len()];
        let udzialy = magnat_core::split_proportional(suma, &wagi);
        let permil = (1000 / spadkobiercy.len()) as u16;
        let mut lista: Vec<(CitizenId, u16)> = Vec::with_capacity(spadkobiercy.len());
        for (i, h) in spadkobiercy.iter().enumerate() {
            if let Some(w) = world.get_mut::<Wealth>(*h) {
                w.cash = w.cash.checked_add(udzialy[i]).unwrap_or(w.cash);
            }
            let p = if i == 0 {
                permil + (1000 - permil * spadkobiercy.len() as u16)
            } else {
                permil
            };
            lista.push((CitizenId(*h), p));
            powody.push((
                h.index(),
                DecisionReason::Inheritance {
                    permille: p,
                    heirs: spadkobiercy.len().min(255) as u8,
                },
            ));
        }
        hooks.on_inheritance(CitizenId(e), &lista, cmd);
    }
    if let Some(w) = world.get_mut::<Wealth>(e) {
        w.cash = Money::ZERO;
        w.personal_assets = Money::ZERO;
    }

    // Partner zostaje sam.
    let partner = world
        .get::<Lifecycle>(e)
        .map(|l| l.partner)
        .filter(|p| *p != Lifecycle::NO_PARTNER)
        .and_then(|p| encja(world, p));
    if let Some(p) = partner {
        if let Some(l) = world.get_mut::<Lifecycle>(p) {
            l.partner = Lifecycle::NO_PARTNER;
            l.flags &= !Lifecycle::FLAG_PARTNERED;
        }
    }

    crate::migration::release_job_of(world, e);
    usun_relacje(world, e);
    zwolnij_slaby(world, e);
    world.resource_mut::<LifeQueue>().cancel(e.index());
    opusc_gospodarstwo(world, e, day, cmd);

    if let Some(id) = world.get_mut::<Identity>(e) {
        id.flags &= !Identity::FLAG_ALIVE;
    }
    world.resource_mut::<Population>().remove_citizen(e);
    world.resource_mut::<Population>().deaths += 1;
    cmd.despawn(e);
    powody
}

/// Spadkobiercy: współmałżonek, dalej dzieci po równo (§5.6).
fn spadkobiercy(world: &World, e: Entity) -> Vec<Entity> {
    if let Some(p) = world
        .get::<Lifecycle>(e)
        .map(|l| l.partner)
        .filter(|p| *p != Lifecycle::NO_PARTNER)
        .and_then(|p| encja(world, p))
    {
        return vec![p];
    }
    let Some(rel) = world.get::<RelationsRef>(e).copied() else {
        return Vec::new();
    };
    let slab = world.resource::<RelationSlab>();
    let mut dzieci: Vec<Entity> = slab
        .entries(relations_ref(&rel))
        .iter()
        .filter(|r| r.kind == RelationKind::Child as u8)
        .filter_map(|r| encja(world, r.other))
        .collect();
    dzieci.sort_unstable_by_key(|x| x.index());
    dzieci.dedup();
    dzieci
}

fn usun_relacje(world: &mut World, e: Entity) {
    let Some(rel) = world.get::<RelationsRef>(e).copied() else {
        return;
    };
    let inni: Vec<u32> = world
        .resource::<RelationSlab>()
        .entries(relations_ref(&rel))
        .iter()
        .map(|r| r.other)
        .collect();
    for idx in inni {
        let Some(inny) = encja(world, idx) else { continue };
        let Some(r2) = world.get::<RelationsRef>(inny).copied() else {
            continue;
        };
        let mut sr = relations_ref(&r2);
        let slab = world.resource_mut::<RelationSlab>();
        if let Some(i) = slab.entries(sr).iter().position(|x| x.other == e.index()) {
            slab.remove_at(&mut sr, i);
            if let Some(slot) = world.get_mut::<RelationsRef>(inny) {
                slot.handle = sr.handle;
                slot.len = sr.len;
                slot.class = sr.class;
            }
        }
    }
}

fn zwolnij_slaby(world: &mut World, e: Entity) {
    if let Some(rel) = world.get::<RelationsRef>(e).copied() {
        let mut r = relations_ref(&rel);
        world.resource_mut::<RelationSlab>().free(&mut r);
        if let Some(slot) = world.get_mut::<RelationsRef>(e) {
            *slot = RelationsRef::default();
        }
    }
    if let Some(k) = world.get::<KnowledgeRef>(e).copied() {
        let mut r = knowledge_ref(&k);
        world.resource_mut::<KnowledgeSlab>().free(&mut r);
        if let Some(slot) = world.get_mut::<KnowledgeRef>(e) {
            *slot = KnowledgeRef::default();
        }
    }
    if let Some(p) = world.get::<PlanRef>(e).copied() {
        let mut r = p.slab_ref();
        world.resource_mut::<PlanSlab>().free(&mut r);
        if let Some(slot) = world.get_mut::<PlanRef>(e) {
            *slot = PlanRef::default();
        }
    }
}

/// Wypisanie ze składu gospodarstwa; gospodarstwo bez członków **rozwiązuje się**.
///
/// Rozwiązanie oddaje lokal do puli pustostanów i to jest główna droga, którą pustostan
/// w ogóle powstaje (§5.7). Bez niej gospodarstwo zmarłego trzymałoby mieszkanie do końca
/// świata: pustostanów by nie było, `min(wakaty, pustostany)` zostawałoby na zerze,
/// napływ wygasłby, a populacja mogłaby już tylko maleć — regulator przestałby regulować.
pub fn opusc_gospodarstwo(
    world: &mut World,
    e: Entity,
    day: u64,
    cmd: &mut CommandBuffer,
) -> Option<Entity> {
    let hh_idx = world.get::<Identity>(e)?.household;
    let hh_e = encja_gospodarstwa(world, hh_idx)?;
    let mut hh = world.get::<Household>(hh_e).copied()?;
    {
        let ov = world.resource_mut::<HouseholdOverflow>();
        household::remove_member(hh_idx, &mut hh, ov, e.index());
    }
    if hh.size == 0 {
        rozwiaz_gospodarstwo(world, hh_e, &hh, cmd);
        return Some(hh_e);
    }
    przeklasyfikuj(world, hh_idx, &mut hh, day);
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh;
    }
    Some(hh_e)
}

/// Rozwiązanie pustego gospodarstwa: lokal wraca do puli, wpisy pomocnicze znikają.
fn rozwiaz_gospodarstwo(
    world: &mut World,
    hh_e: Entity,
    hh: &Household,
    cmd: &mut CommandBuffer,
) {
    if hh.has_home() {
        world
            .resource_mut::<crate::migration::Vacancies>()
            .release_home(crate::migration::HomeSlot {
                building: hh.building,
                unit: hh.unit,
                district: hh.district,
                value: Money::ZERO,
            });
    }
    world
        .resource_mut::<HouseholdOverflow>()
        .clear_household(hh_e.index());
    world
        .resource_mut::<crate::migration::Unsettled>()
        .forget(hh_e.index());
    world.resource_mut::<Population>().remove_household(hh_e);
    cmd.despawn(hh_e);
}

/// Przelicza typ gospodarstwa po zmianie składu (§5.6: typ jest funkcją składu).
pub fn przeklasyfikuj(world: &mut World, hh_idx: u32, hh: &mut Household, day: u64) {
    let ages = world.resource::<DemographyTable>().ages();
    let sklad = household::members_of(hh_idx, hh, world.resource::<HouseholdOverflow>());
    let widoki: Vec<MemberView> = sklad
        .iter()
        .filter_map(|m| {
            let c = encja(world, *m)?;
            let id = world.get::<Identity>(c)?;
            let emp = world.get::<Employment>(c)?;
            let partner_w_domu = world
                .get::<Lifecycle>(c)
                .map(|l| l.partner)
                .filter(|p| *p != Lifecycle::NO_PARTNER)
                .is_some_and(|p| sklad.contains(&p));
            Some(MemberView::new(
                *m,
                id,
                emp,
                day as i32,
                partner_w_domu,
            ))
        })
        .collect();
    hh.kind = household::classify(&widoki, i32::from(ages.adult), i32::from(ages.senior)) as u8;
}

// -- miesiac: zwiazki, sluby, rozstania -----------------------------------------

/// Jeden miesiąc cyklu życia gospodarstw (§5.6). Wołany co 30 dób (K-1).
///
/// Kolejność: dobór partnera → ślub → rozstanie. Para dobrana dziś nie bierze dziś
/// ślubu (dzieli je `wedding_min_days`), a para, która wzięła ślub dziś, nie rozstaje
/// się dziś (`divorce_grace_days`) — więc kolejność nie tworzy ścieżki „w jednym
/// miesiącu od poznania do rozwodu". Jedno pole `Lifecycle.next_event_day` niesie oba
/// terminy, bo są rozłączne w czasie: najpierw jest to najwcześniejsza data ślubu,
/// potem najwcześniejsza data rozstania.
pub fn step_month(world: &mut World, day: u64) -> MonthReport {
    let mut raport = MonthReport::default();
    dobierz_partnerow(world, day, &mut raport);
    sluby(world, day, &mut raport);
    rozstania(world, day, &mut raport);
    raport
}

/// Zgodność kandydata: 100 minus kara za różnicę statusu i wieku.
#[must_use]
pub fn compatibility(status_a: u8, status_b: u8, age_a: i32, age_b: i32) -> Q {
    let ds = i32::from(status_a).abs_diff(i32::from(status_b)) as i32;
    let dw = age_a.abs_diff(age_b) as i32;
    Q::new((100 - ds * 2 - dw * 3).clamp(0, 100) as u8)
}

const fn rodzinna(kind: u8) -> bool {
    kind == RelationKind::Parent as u8
        || kind == RelationKind::Child as u8
        || kind == RelationKind::Sibling as u8
}

fn dobierz_partnerow(world: &mut World, day: u64, raport: &mut MonthReport) {
    let tabela = world.resource::<DemographyTable>().clone();
    let ages = tabela.ages();
    let (max_ds, max_dw, min_waga) = tabela.partner_limits();
    let (wedding_min_days, _) = tabela.wedding_rules();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();

    for e in spis {
        let Some(id) = world.get::<Identity>(e).copied() else {
            continue;
        };
        let wiek = id.age_years(day as i32);
        if wiek < i32::from(ages.adult) {
            continue;
        }
        let l = world.get::<Lifecycle>(e).copied().unwrap_or_default();
        if l.partner != Lifecycle::NO_PARTNER {
            continue;
        }
        let mut r = rng(
            world.seed,
            StreamId::Relations,
            e.index(),
            stream_key(day, K_PARTNER),
        );
        if !r.gen_bool_permille(tabela.partnering_permille(wiek)) {
            continue;
        }

        let status = world.get::<Vitals>(e).map_or(50, |v| v.status);
        let Some(rel) = world.get::<RelationsRef>(e).copied() else {
            continue;
        };
        let kandydaci: Vec<(u32, u8)> = world
            .resource::<RelationSlab>()
            .entries(relations_ref(&rel))
            .iter()
            .filter(|x| x.weight >= min_waga && !rodzinna(x.kind))
            .map(|x| (x.other, x.weight))
            .collect();

        let mut najlepszy: Option<(Entity, Q, u8)> = None;
        let mut rozwazonych = 0u8;
        for (inny, waga) in kandydaci {
            let Some(c) = encja(world, inny) else { continue };
            let Some(cid) = world.get::<Identity>(c).copied() else {
                continue;
            };
            let cwiek = cid.age_years(day as i32);
            if cwiek < i32::from(ages.adult) || cwiek.abs_diff(wiek) > u32::from(max_dw) {
                continue;
            }
            if world
                .get::<Lifecycle>(c)
                .is_some_and(|x| x.partner != Lifecycle::NO_PARTNER)
            {
                continue;
            }
            let cstatus = world.get::<Vitals>(c).map_or(50, |v| v.status);
            if u32::from(status).abs_diff(u32::from(cstatus)) > u32::from(max_ds) {
                continue;
            }
            rozwazonych = rozwazonych.saturating_add(1);
            let zgodnosc = compatibility(status, cstatus, wiek, cwiek);
            // Remis rozstrzyga waga relacji, a potem indeks encji (00 §3.2).
            let lepszy = najlepszy.is_none_or(|(be, bz, bw)| {
                (zgodnosc.get(), waga, std::cmp::Reverse(c.index()))
                    > (bz.get(), bw, std::cmp::Reverse(be.index()))
            });
            if lepszy {
                najlepszy = Some((c, zgodnosc, waga));
            }
        }

        let Some((partner, zgodnosc, _)) = najlepszy else {
            continue;
        };
        for (a, b) in [(e, partner), (partner, e)] {
            if let Some(l) = world.get_mut::<Lifecycle>(a) {
                l.partner = b.index();
                l.flags |= Lifecycle::FLAG_PARTNERED;
                l.next_event_day = ((day + u64::from(wedding_min_days)) % 65_536) as u16;
            }
        }
        let waga = tabela.social().family_weight;
        powiaz(world, e, partner, RelationKind::Partner, waga, day);
        raport.partnerships += 1;
        raport.reasons.push((
            e.index(),
            DecisionReason::PartnerChosen {
                compatibility: zgodnosc,
                candidates: rozwazonych,
            },
        ));
    }
}

/// Wspólne gospodarstwo po `wedding_min_days` dobach związku o wadze >= progu (§5.6).
///
/// Przenosi się gospodarstwo **mniejsze** — razem z dziećmi, bo samotny rodzic nie
/// zostawia ich pod starym adresem. Opuszczony lokal wraca do puli pustostanów i to
/// jest jedna z trzech dróg, którymi pustostan powstaje (pozostałe to zgon i wyjazd).
fn sluby(world: &mut World, day: u64, raport: &mut MonthReport) {
    let tabela = world.resource::<DemographyTable>().clone();
    let (_, min_waga) = tabela.wedding_rules();
    let grace = tabela.divorce_grace_days();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();

    for e in spis {
        let l = world.get::<Lifecycle>(e).copied().unwrap_or_default();
        if l.partner == Lifecycle::NO_PARTNER || l.partner < e.index() {
            continue; // parę obsługuje ten z niższym indeksem
        }
        let Some(partner) = encja(world, l.partner) else {
            continue;
        };
        let (Some(a), Some(b)) = (
            world.get::<Identity>(e).copied(),
            world.get::<Identity>(partner).copied(),
        ) else {
            continue;
        };
        if a.household == b.household {
            continue; // już mieszkają razem
        }
        if (day % 65_536) < u64::from(l.next_event_day) {
            continue;
        }
        let Some(rel) = world.get::<RelationsRef>(e).copied() else {
            continue;
        };
        let waga = world
            .resource::<RelationSlab>()
            .entries(relations_ref(&rel))
            .iter()
            .find(|x| x.other == partner.index())
            .map_or(0, |x| x.weight);
        if waga < min_waga {
            continue;
        }
        if polacz_gospodarstwa(world, a.household, b.household, day) {
            raport.weddings += 1;
            for kto in [e, partner] {
                if let Some(lc) = world.get_mut::<Lifecycle>(kto) {
                    lc.next_event_day = ((day + u64::from(grace)) % 65_536) as u16;
                }
            }
        }
    }
}

/// Scala dwa gospodarstwa. Zwraca `false`, gdy nie ma miejsca — para zostaje wtedy
/// osobno i spróbuje w następnym miesiącu.
fn polacz_gospodarstwa(world: &mut World, a: u32, b: u32, day: u64) -> bool {
    let (Some(ea), Some(eb)) = (encja_gospodarstwa(world, a), encja_gospodarstwa(world, b)) else {
        return false;
    };
    let (Some(ha), Some(hb)) = (
        world.get::<Household>(ea).copied(),
        world.get::<Household>(eb).copied(),
    ) else {
        return false;
    };
    // Zostaje gospodarstwo z lokalem, a przy remisie liczniejsze.
    let (docelowe, zrodlo) = if (ha.has_home(), ha.size) >= (hb.has_home(), hb.size) {
        ((ea, a), (eb, b))
    } else {
        ((eb, b), (ea, a))
    };

    let zrodlowe = world
        .get::<Household>(zrodlo.0)
        .copied()
        .unwrap_or_default();
    let przenoszeni = household::members_of(
        zrodlo.1,
        &zrodlowe,
        world.resource::<HouseholdOverflow>(),
    );
    let mut cel = world
        .get::<Household>(docelowe.0)
        .copied()
        .unwrap_or_default();
    for m in przenoszeni.iter() {
        let ov = world.resource_mut::<HouseholdOverflow>();
        if !household::add_member(docelowe.1, &mut cel, ov, *m) {
            return false;
        }
    }
    for m in przenoszeni.iter() {
        let Some(c) = encja(world, *m) else { continue };
        if let Some(id) = world.get_mut::<Identity>(c) {
            id.household = docelowe.1;
        }
        if let Some(res) = world.get_mut::<Residence>(c) {
            res.building = cel.building;
            res.unit = cel.unit;
            res.district = cel.district;
        }
    }

    // Majątek gospodarstwa idzie za ludźmi — inaczej znikałby przy każdym ślubie.
    cel.cash = cel.cash.checked_add(zrodlowe.cash).unwrap_or(cel.cash);
    cel.bank = cel.bank.checked_add(zrodlowe.bank).unwrap_or(cel.bank);
    cel.savings = cel
        .savings
        .checked_add(zrodlowe.savings)
        .unwrap_or(cel.savings);
    cel.debt = cel.debt.checked_add(zrodlowe.debt).unwrap_or(cel.debt);
    cel.income_monthly = cel
        .income_monthly
        .checked_add(zrodlowe.income_monthly)
        .unwrap_or(cel.income_monthly);
    przeklasyfikuj(world, docelowe.1, &mut cel, day);
    if let Some(slot) = world.get_mut::<Household>(docelowe.0) {
        *slot = cel;
    }

    // Opuszczony lokal wraca do puli, gospodarstwo znika.
    if zrodlowe.has_home() {
        world
            .resource_mut::<crate::migration::Vacancies>()
            .release_home(crate::migration::HomeSlot {
                building: zrodlowe.building,
                unit: zrodlowe.unit,
                district: zrodlowe.district,
                value: Money::ZERO,
            });
    }
    world
        .resource_mut::<HouseholdOverflow>()
        .clear_household(zrodlo.1);
    world
        .resource_mut::<crate::migration::Unsettled>()
        .forget(zrodlo.1);
    world.resource_mut::<Population>().remove_household(zrodlo.0);
    let mut cmd = CommandBuffer::new(demography_system_id());
    cmd.despawn(zrodlo.0);
    magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
    true
}

fn rozstania(world: &mut World, day: u64, raport: &mut MonthReport) {
    let tabela = world.resource::<DemographyTable>().clone();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();

    for e in spis {
        let l = world.get::<Lifecycle>(e).copied().unwrap_or_default();
        if l.partner == Lifecycle::NO_PARTNER || l.partner < e.index() {
            continue;
        }
        let Some(partner) = encja(world, l.partner) else {
            continue;
        };
        let (Some(a), Some(b)) = (
            world.get::<Identity>(e).copied(),
            world.get::<Identity>(partner).copied(),
        ) else {
            continue;
        };
        if a.household != b.household || (day % 65_536) < u64::from(l.next_event_day) {
            continue;
        }

        let stres = world
            .get::<Vitals>(e)
            .map_or(0, |v| v.stress)
            .max(world.get::<Vitals>(partner).map_or(0, |v| v.stress));
        let hh = encja_gospodarstwa(world, a.household)
            .and_then(|h| world.get::<Household>(h).copied())
            .unwrap_or_default();
        let p = tabela.divorce_permille(Q::new(stres), hh.debt, hh.income_monthly);
        let mut r = rng(
            world.seed,
            StreamId::Relations,
            e.index(),
            stream_key(day, K_SEPARATION),
        );
        if !r.gen_bool_permille(p) {
            continue;
        }

        // Długość związku liczona od najwcześniejszej możliwej daty ślubu, czyli od
        // momentu, w którym `next_event_day` przestał być terminem ślubu — z dokładnością
        // do miesiąca, bo dokładniejsza data nie ma gdzie mieszkać, a i tak jest tylko
        // treścią wyjaśnienia (00 §7), nie wejściem żadnej reguły.
        let razem = ((day.saturating_sub(u64::from(l.next_event_day))) / DAYS_PER_YEAR)
            .min(255) as u8;
        for kto in [e, partner] {
            if let Some(lc) = world.get_mut::<Lifecycle>(kto) {
                lc.partner = Lifecycle::NO_PARTNER;
                lc.flags &= !Lifecycle::FLAG_PARTNERED;
                lc.next_event_day = 0;
            }
        }
        let _ = b;
        // Wyprowadza się ten z wyższym indeksem — dzieci zostają z drugim.
        // Brak pustostanu nie blokuje rozstania: gospodarstwo bez lokalu obsługuje
        // migracja (§5.7, `settle_grace_days`), a zmuszanie pary do wspólnego życia
        // z powodu rynku mieszkaniowego byłoby regułą, której nikt nie zapisał.
        // Przeprowadzka jest lokalna albo jej nie ma: w mieście bez pustostanu
        // w dzielnicy rozstana para mieszka dalej razem, a próbuje ponownie
        // w następnym miesiącu. Związek jest już rozwiązany w obu `Lifecycle`.
        crate::migration::zaloz_gospodarstwo(world, partner, day);
        raport.separations += 1;
        raport.reasons.push((
            e.index(),
            DecisionReason::SeparationFiled {
                stress: Q::new(stres),
                years_together: razem,
            },
        ));
    }
}

/// Wyjazd mieszkańca z miasta: zwolnienie relacji i slabów, wypisanie ze spisu.
///
/// Różni się od zgonu tym, że nie ma dziedziczenia — majątek jedzie z człowiekiem,
/// więc trafia do licznika `Population::emigrated`, żeby test zachowania pieniądza
/// (00 §6) widział, dokąd poszedł.
pub fn wyprowadz_mieszkanca(world: &mut World, e: Entity, cmd: &mut CommandBuffer) {
    let majatek = world.get::<Wealth>(e).copied().unwrap_or_default();
    let suma = majatek
        .cash
        .checked_add(majatek.personal_assets)
        .unwrap_or(majatek.cash);
    let p = world.resource_mut::<Population>();
    p.emigrated = p.emigrated.checked_add(suma).unwrap_or(p.emigrated);

    if let Some(partner) = world
        .get::<Lifecycle>(e)
        .map(|l| l.partner)
        .filter(|x| *x != Lifecycle::NO_PARTNER)
        .and_then(|x| encja(world, x))
    {
        if let Some(l) = world.get_mut::<Lifecycle>(partner) {
            l.partner = Lifecycle::NO_PARTNER;
            l.flags &= !Lifecycle::FLAG_PARTNERED;
        }
    }
    crate::migration::release_job_of(world, e);
    usun_relacje(world, e);
    zwolnij_slaby(world, e);
    world.resource_mut::<LifeQueue>().cancel(e.index());
    if let Some(id) = world.get_mut::<Identity>(e) {
        id.flags &= !Identity::FLAG_ALIVE;
    }
    if let Some(w) = world.get_mut::<Wealth>(e) {
        w.cash = Money::ZERO;
        w.personal_assets = Money::ZERO;
    }
    world.resource_mut::<Population>().remove_citizen(e);
    cmd.despawn(e);
}

/// Encja mieszkańca po indeksie — publiczna, bo używa jej `migration` i Etap 8 w M3d.
#[must_use]
pub fn citizen_by_index(world: &World, index: u32) -> Option<Entity> {
    encja(world, index)
}

/// Encja gospodarstwa po indeksie.
#[must_use]
pub fn household_by_index(world: &World, index: u32) -> Option<Entity> {
    encja_gospodarstwa(world, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabela_z_danych_pokrywa_kazdy_wiek() {
        let t = DemographyTable::load_default().expect("data/demography/demography.ron");
        let ages = t.ages();
        assert!(ages.retirement > ages.work_start);
        assert!(ages.max > ages.retirement);
        for wiek in 0..=i32::from(ages.max) {
            assert!(
                t.mortality_per_100k(wiek, Q::MAX) > 0,
                "zerowa umieralność w wieku {wiek} — dziura w tabeli"
            );
            assert!(t.illness_per_100k(wiek, Q::MAX) > 0, "wiek {wiek}");
        }
        assert_eq!(t.status_weights().sum(), 100);
    }

    #[test]
    fn plodnosc_spada_z_kolejnym_dzieckiem_i_bez_partnera() {
        let t = DemographyTable::load_default().expect("dane");
        let pelna = t.fertility_per_100k(28, 0, true, Q::MAX);
        assert!(pelna > 0);
        assert!(t.fertility_per_100k(28, 3, true, Q::MAX) < pelna);
        assert!(t.fertility_per_100k(28, 0, false, Q::MAX) < pelna);
        assert!(t.fertility_per_100k(28, 0, true, Q::new(10)) < pelna);
        // Poza oknem rozrodczym hazard jest zerowy, a nie „mały".
        assert_eq!(t.fertility_per_100k(60, 0, true, Q::MAX), 0);
        assert_eq!(t.fertility_per_100k(10, 0, true, Q::MAX), 0);
    }

    #[test]
    fn chory_umiera_czesciej_niz_zdrowy() {
        let t = DemographyTable::load_default().expect("dane");
        assert!(t.mortality_per_100k(40, Q::MIN) > t.mortality_per_100k(40, Q::MAX));
        // Sufit tabeli to nie „bardzo duży hazard", tylko koniec tabeli — pewność
        // dokłada `hazardy`, żeby `prop_no_immortals` nie zależał od ogona rozkładu.
        assert!(t.mortality_per_100k(i32::from(t.ages().max), Q::MAX) < 100_000);
    }

    #[test]
    fn terminarz_wydaje_zadania_w_porzadku_totalnym() {
        let mut q = LifeQueue::new();
        for a in [7u32, 3, 9, 1] {
            q.schedule(
                10,
                LifeTask {
                    kind: LifeTaskKind::Recover,
                    actor: a,
                },
            );
            q.schedule(
                10,
                LifeTask {
                    kind: LifeTaskKind::Birth,
                    actor: a,
                },
            );
        }
        q.schedule(
            5,
            LifeTask {
                kind: LifeTaskKind::Birth,
                actor: 100,
            },
        );
        assert_eq!(q.len(), 9);

        // Doba 12 zabiera też zaległą dobę 5 — w trybie demograficznym skacze się
        // po dobach i zadanie pominięte wisiałoby w mapie na zawsze.
        let mut out = Vec::new();
        q.take_due(12, &mut out);
        assert_eq!(out.len(), 9);
        assert!(q.is_empty());
        assert!(
            out.windows(2).all(|w| w[0] <= w[1]),
            "porządek nie jest totalny: {out:?}"
        );
        assert_eq!(out[0].kind, LifeTaskKind::Birth);

        q.schedule(
            20,
            LifeTask {
                kind: LifeTaskKind::Birth,
                actor: 42,
            },
        );
        q.cancel(42);
        assert!(q.is_empty(), "anulowanie zostawiło zadanie martwego aktora");
    }

    #[test]
    fn spis_populacji_trzyma_porzadek_i_ksiege() {
        use std::num::NonZeroU32;
        let g = NonZeroU32::new(1).unwrap();
        let mut p = Population::new();
        for i in [5u32, 1, 9, 3] {
            p.add_citizen(Entity::new(i, g));
        }
        assert_eq!(
            p.citizens().iter().map(|e| e.index()).collect::<Vec<_>>(),
            vec![1, 3, 5, 9]
        );
        p.add_citizen(Entity::new(5, g));
        assert_eq!(p.len(), 4, "podwójne dopisanie tej samej encji");

        let start = 4;
        assert!(p.identity_holds(start));
        p.remove_citizen(Entity::new(3, g));
        p.deaths += 1;
        assert!(p.identity_holds(start));
        p.add_citizen(Entity::new(11, g));
        assert!(!p.identity_holds(start), "spis rozjechał się z księgą");
        p.births += 1;
        assert!(p.identity_holds(start));
    }
}
