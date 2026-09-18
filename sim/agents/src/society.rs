//! Rytm społeczeństwa: co się dzieje w dobie i co w miesiącu (M3c).
//!
//! Jedno miejsce, w którym stoi **kolejność** wywołań, bo od niej zależy wynik:
//! zgon musi się wykonać przed migracją (zwolniony lokal jest pustostanem w tym samym
//! miesiącu), status przed doborem partnera (kompatybilność czyta status), a odbudowa
//! indeksu kontaktów po migracji (inaczej nowo przybyli nie mają z kim rozmawiać).
//!
//! **To nie są systemy ECS i to jest celowe.** Tabela systemów i ich częstotliwości
//! (§5.12) należy do M3d — tam `DemographySystem`, `MigrationSystem`, `StatusSystem`,
//! `RelationDecaySystem` i `GossipSystem` opakują te funkcje i dostaną swoje `Cadence`.
//! Do tego czasu pętlę przewija runner scenariusza, dokładnie tak jak w M3a (kolejka
//! zdarzeń) i w M3b (plan dnia). Powód jest praktyczny: tryb demograficzny `m3_century`
//! przeskakuje **dobę na krok**, a `App::tick` idzie minuta po minucie — 36 000 dób
//! to 51,8 mln ticków, z czego 51,8 mln minęłoby bez żadnej pracy do wykonania.

use crate::demography::{self, DayReport, InheritanceHook, MonthReport, Population};
use crate::household::Household;
use crate::migration::{self, MigrationReport, Unsettled, Vacancies};
use crate::social::{self, CityFacts, SocialIndex, SocialReport, StatusDistribution, StatusReport};
use magnat_core::time::DAYS_PER_MONTH;
use magnat_ecs::World;

/// Wszystko, co wydarzyło się w jednej dobie. Miesięczne pola są `None` w dobach,
/// które nie są początkiem miesiąca — a nie zerami, bo zero znaczy „policzono, wyszło
/// zero", a to jest inna informacja niż „nie liczono".
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SocietyReport {
    pub day: DayReport,
    pub social: SocialReport,
    pub lifecycle: Option<MonthReport>,
    pub migration: Option<MigrationReport>,
    pub status: Option<StatusReport>,
}

/// Rejestruje komponent i zasoby podfazy M3c wraz z hakami hasha stanu (00 §3.6).
///
/// Osobno od `register` z M3a, bo tabela demografii jest **danymi wejściowymi**
/// (00 §5) i ładuje ją wywołujący — rejestracja nie ma prawa się wywalić na I/O.
/// Scenariusz M3a, który demografii nie uruchamia, nie płaci za nią ani bajtem hasha.
pub fn register_society(world: &mut World, demography: demography::DemographyTable) {
    world.insert_resource(demography);
    world.insert_resource(Population::new());
    world.insert_resource(demography::LifeQueue::new());
    world.insert_resource(crate::household::HouseholdOverflow::new());
    world.insert_resource(Vacancies::default());
    world.insert_resource(Unsettled::new());
    world.insert_resource(StatusDistribution::default());
    world.insert_resource(CityFacts::default());
    world.insert_resource(SocialIndex::new());
    // Katalog miejsc: dobowy cykl życia szuka w nim szkoły dla siedmiolatka (`R2-WP1`).
    // Świat bez miasta zostaje z pustym katalogiem i wtedy uczeń ma samą flagę.
    world.insert_resource(crate::places::PlaceCatalog::default());

    // Tabela demografii **nie jest** haszowana: to dane wejściowe, nie stan świata —
    // ta sama zasada co przy `NeedTable` w M3a. `PlaceCatalog` też nie — katalog miejsc
    // jest daną miasta. Reszta jest stanem i wchodzi do hasha, bo od niej zależy,
    // co się wydarzy jutro.
    world.register_resource_hash::<Population>();
    world.register_resource_hash::<demography::LifeQueue>();
    world.register_resource_hash::<crate::household::HouseholdOverflow>();
    world.register_resource_hash::<Vacancies>();
    world.register_resource_hash::<Unsettled>();
    world.register_resource_hash::<StatusDistribution>();
    world.register_resource_hash::<CityFacts>();
    world.register_resource_hash::<SocialIndex>();
}

/// Czy doba `day` zaczyna miesiąc (K-1: miesiąc = 30 dób, zawsze).
#[inline]
#[must_use]
pub const fn is_month_start(day: u64) -> bool {
    day.is_multiple_of(DAYS_PER_MONTH)
}

/// Jedna doba życia społeczeństwa.
///
/// Kolejność w obrębie doby: **demografia → społeczeństwo**, a na początku miesiąca
/// jeszcze **status → gospodarstwa → migracja → indeks kontaktów**.
///
/// Dlaczego status idzie przed doborem partnera: kompatybilność z §5.6 czyta `Vitals.status`,
/// więc przeliczony w środku miesiąca dawałby parom status z poprzedniego miesiąca dla
/// jednych, a bieżący dla drugich — zależnie od indeksu encji.
/// Dlaczego indeks kontaktów odbudowuje się na końcu: migracja właśnie zmieniła adresy
/// i miejsca pracy, a plotka rusza już w tej samej dobie.
pub fn step_day(world: &mut World, day: u64, hooks: &mut dyn InheritanceHook) -> SocietyReport {
    let mut raport = SocietyReport::default();

    if is_month_start(day) {
        raport.status = Some(social::step_month(world, day));
        raport.lifecycle = Some(demography::month::step_month(world, day));
        raport.migration = Some(migration::step_month(world, day));
        odbuduj_indeks(world);
    }

    raport.day = demography::day::step_day(world, day, hooks);
    raport.social = social::step_day(world, day);
    raport
}

/// Odbudowa indeksu kontaktów. Wydzielona, bo `SocialIndex::rebuild` bierze `&World`,
/// a zasób siedzi w tym samym świecie — trzeba go na chwilę wyjąć.
fn odbuduj_indeks(world: &mut World) {
    let mut idx = std::mem::take(world.resource_mut::<SocialIndex>());
    idx.rebuild(world);
    *world.resource_mut::<SocialIndex>() = idx;
}

/// Liczba żywych mieszkańców — skrót do raportów i testów.
#[must_use]
pub fn population(world: &World) -> usize {
    world.resource::<Population>().len()
}

/// Liczba aktywnych gospodarstw.
#[must_use]
pub fn households(world: &World) -> usize {
    world.resource::<Population>().households().len()
}

/// Suma pieniądza w świecie plus to, co z niego wyszło (00 §6).
///
/// Test własnościowy fazy porównuje tę liczbę na początku i na końcu przebiegu:
/// dziedziczenie, ślub i wyjazd **przenoszą** pieniądz, a nie tworzą go ani nie niszczą.
#[must_use]
pub fn total_money(world: &World) -> i64 {
    let p = world.resource::<Population>();
    let mut suma = p.escheat.get().saturating_add(p.emigrated.get());
    for e in p.citizens() {
        if let Some(w) = world.get::<crate::components::Wealth>(*e) {
            suma = suma
                .saturating_add(w.cash.get())
                .saturating_add(w.personal_assets.get());
        }
    }
    for e in p.households() {
        if let Some(h) = world.get::<Household>(*e) {
            suma = suma
                .saturating_add(h.cash.get())
                .saturating_add(h.bank.get())
                .saturating_add(h.savings.get());
        }
    }
    suma
}
