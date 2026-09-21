//! `DecisionReason` → tekst. **Jedno miejsce w całej grze** (M3 §6.1).
//!
//! `match` jest wyczerpujący i bez ramienia `_` — to jest cały mechanizm z 00 §K-12:
//! faza, która dopisze wariant, **nie skompiluje interfejsu**, dopóki nie napisze,
//! jak ten powód pokazać graczowi. Test `kazdy_powod_ma_tekst` sprawdza to od drugiej
//! strony: każdy wariant musi dać niepusty napis w obu językach.

use crate::loc::{Catalog, Locale};
use magnat_agents::SocialClass;
use magnat_core::{
    ActionKind, ActivityKind, AdChannelKind, BankruptcyTrigger, CitizenReason, CityReason,
    ClaimPriority, CommitmentKind, DecisionReason, DeprivationEffect, EditorialBias, FirmReason,
    FirmStrategy, FixedCost, LeaveCause, LifeEventKind, LineStopCause, LoanKind, MigrationKind,
    Money, NeedKind, PriceDriver, ReactionKind, RejectCause, RejectCredit, ShortageStageKind,
    StockCat, TouchSource, TraitId, TransportMode, UtilityKind, WageCause,
};

/// Nazwa potrzeby w języku gracza.
#[must_use]
pub fn need(c: &Catalog, l: Locale, n: NeedKind) -> String {
    c.fmt_key(l, &format!("ui.need.{}", n.name()), &[])
}

/// Nazwa czynności planu dnia.
#[must_use]
pub fn activity(c: &Catalog, l: Locale, a: ActivityKind) -> String {
    c.fmt_key(l, &format!("ui.activity.{}", a.name()), &[])
}

/// Nazwa kategorii zapasu gospodarstwa.
#[must_use]
pub fn stock(c: &Catalog, l: Locale, s: StockCat) -> String {
    c.fmt_key(l, &format!("ui.stock.{}", s.name()), &[])
}

/// Nazwa cechy osobowości.
#[must_use]
pub fn trait_name(c: &Catalog, l: Locale, t: TraitId) -> String {
    c.fmt_key(l, &format!("ui.trait.{}", t.name()), &[])
}

/// Nazwa środka transportu.
#[must_use]
pub fn transport_mode(c: &Catalog, l: Locale, m: TransportMode) -> String {
    c.fmt_key(l, &format!("ui.mode.{}", m.name()), &[])
}

/// Nazwa członu funkcji użyteczności zakupu (PRD §6.4).
#[must_use]
pub fn utility_term(c: &Catalog, l: Locale, u: UtilityKind) -> String {
    c.fmt_key(l, &format!("ui.utility.{}", u.name()), &[])
}

/// Nazwa powodu odrzucenia oferty (M5b §5.4).
#[must_use]
pub fn reject_cause(c: &Catalog, l: Locale, r: RejectCause) -> String {
    c.fmt_key(l, &format!("ui.reject.{}", r.name()), &[])
}

/// Nazwa członu, który przeważył przy przecenie (M5c §5.6).
#[must_use]
pub fn price_driver(c: &Catalog, l: Locale, d: PriceDriver) -> String {
    c.fmt_key(l, &format!("ui.price_driver.{}", d.name()), &[])
}

/// Nazwa produktu kredytowego (M5d §5.10).
#[must_use]
pub fn loan_kind(c: &Catalog, l: Locale, k: LoanKind) -> String {
    c.fmt_key(l, &format!("ui.loan_kind.{}", k.name()), &[])
}

/// Nazwa powodu odmowy kredytu (M5d §5.10).
#[must_use]
pub fn reject_credit(c: &Catalog, l: Locale, r: RejectCredit) -> String {
    c.fmt_key(l, &format!("ui.reject_credit.{}", r.name()), &[])
}

/// Nazwa pozycji kosztów stałych gospodarstwa (M5d §5.9).
#[must_use]
pub fn fixed_cost(c: &Catalog, l: Locale, f: FixedCost) -> String {
    c.fmt_key(l, &format!("ui.fixed_cost.{}", f.name()), &[])
}

/// Nazwa powodu postoju linii produkcyjnej (M6b §5.5).
#[must_use]
pub fn line_stop_cause(c: &Catalog, l: Locale, s: LineStopCause) -> String {
    c.fmt_key(l, &format!("ui.line_stop.{}", s.name()), &[])
}

/// Nazwa stopnia kaskady niedoboru (M6b §5.7, PRD §8.4).
#[must_use]
pub fn shortage_stage(c: &Catalog, l: Locale, s: ShortageStageKind) -> String {
    c.fmt_key(l, &format!("ui.shortage.{}", s.name()), &[])
}

/// Nazwa wyzwalacza postępowania upadłościowego (M7d §5.13).
#[must_use]
pub fn bankruptcy_trigger(c: &Catalog, l: Locale, b: BankruptcyTrigger) -> String {
    c.fmt_key(l, &format!("ui.bankruptcy.{}", b.name()), &[])
}

/// Nazwa priorytetu zaspokojenia w upadłości (M7d §5.13).
#[must_use]
pub fn claim_priority(c: &Catalog, l: Locale, p: ClaimPriority) -> String {
    c.fmt_key(l, &format!("ui.claim_priority.{}", p.name()), &[])
}

/// Nazwa kursu, na którym stoi firma (M7e §5.7).
#[must_use]
pub fn firm_strategy(c: &Catalog, l: Locale, s: FirmStrategy) -> String {
    c.fmt_key(l, &format!("ui.strategy.{}", s.name()), &[])
}

/// Nazwa odpowiedzi konkurencyjnej (M7e WP14).
#[must_use]
pub fn reaction_kind(c: &Catalog, l: Locale, r: ReactionKind) -> String {
    c.fmt_key(l, &format!("ui.reaction.{}", r.name()), &[])
}

/// Nazwa daniny publicznej (M8a §5.1).
#[must_use]
pub fn tax_kind(c: &Catalog, l: Locale, k: magnat_core::TaxKind) -> String {
    c.fmt_key(l, &format!("ui.tax.{}", k.name()), &[])
}

/// Nazwa kierunku wydatku publicznego (M8a WP1).
#[must_use]
pub fn spend_category(c: &Catalog, l: Locale, s: magnat_core::SpendCategory) -> String {
    c.fmt_key(l, &format!("ui.spend.{}", s.name()), &[])
}

/// Dlaczego należność umorzono (M8a WP2).
#[must_use]
pub fn abate_reason(c: &Catalog, l: Locale, a: magnat_core::AbateReason) -> String {
    c.fmt_key(l, &format!("ui.abate.{}", a.name()), &[])
}

/// Nazwa medium sieciowego (M8b §5.4).
#[must_use]
pub fn utility_service(c: &Catalog, l: Locale, u: magnat_core::UtilityService) -> String {
    c.fmt_key(l, &format!("ui.utility.{}", u.name()), &[])
}

/// Kategoria zdarzenia świata jako słowo (M8c).
#[must_use]
pub fn event_category(c: &Catalog, l: Locale, k: magnat_core::EventCategory) -> String {
    c.fmt_key(l, &format!("ui.event.category.{}", k.name()), &[])
}

/// Waty jako kilowaty z jednym miejscem po przecinku — bez floata, bo moc sieci
/// jest liczbą całkowitą i „1 MW" zamiast 1,4 MW gubiłoby połowę deficytu.
#[must_use]
pub fn kilowaty(w: u32) -> String {
    format!("{},{} kW", w / 1000, (w % 1000) / 100)
}

/// Punkty bazowe jako procent z dwoma miejscami — bez floata, bo stawka podatkowa
/// jest liczbą całkowitą i zaokrąglenie jej do „19 %" gubiłoby 19,5 %.
#[must_use]
pub fn procent(bp: u32) -> String {
    let calosc = bp / 100;
    let reszta = bp % 100;
    if reszta == 0 {
        format!("{calosc} %")
    } else {
        format!("{calosc},{reszta:02} %")
    }
}

/// Miesiące jako odmieniony liczebnik.
#[must_use]
pub fn months(c: &Catalog, l: Locale, n: u32) -> String {
    c.plural_key(l, "ui.unit.months", u64::from(n))
}

/// Nazwa powodu ruszenia stawki w ofercie pracy (M7b §5.5).
#[must_use]
pub fn wage_cause(c: &Catalog, l: Locale, w: WageCause) -> String {
    c.fmt_key(l, &format!("ui.wage_cause.{}", w.name()), &[])
}

/// Nazwa powodu odejścia z pracy (M7b WP6).
#[must_use]
pub fn leave_cause(c: &Catalog, l: Locale, k: LeaveCause) -> String {
    c.fmt_key(l, &format!("ui.leave_cause.{}", k.name()), &[])
}

/// Nazwa rodzaju akcji polityki (M7c §5.11).
#[must_use]
pub fn action_kind(c: &Catalog, l: Locale, a: ActionKind) -> String {
    c.fmt_key(l, &format!("ui.action_kind.{}", a.name()), &[])
}

/// Dopisek o ręce menedżera przy zastosowanej polityce (M9d WP9).
///
/// Pusty, gdy menedżer wykonał regułę dokładnie i na świeżych danych — a wtedy nie
/// ma o czym mówić. Zdanie rośnie **tylko wtedy, gdy niesie informację**: „dane sprzed
/// 4 dni, menedżer spudłował o 0,9 %" jest odpowiedzią na „czemu cena jest inna niż
/// w regule", a ten sam dopisek z zerami byłby szumem w każdej karcie.
#[must_use]
pub fn reka_menedzera(c: &Catalog, l: Locale, lag_days: u8, deviation_bp: i16) -> String {
    if lag_days <= 1 && deviation_bp == 0 {
        return String::new();
    }
    let dni = c.plural_key(l, "ui.unit.days", u64::from(lag_days));
    if deviation_bp == 0 {
        return c.fmt_key(l, "ui.reason.PolicyLag", &[("dni", &dni)]);
    }
    // Punkty bazowe na procenty z jednym miejscem po przecinku — gracz nie czyta bp.
    // 10 000 bp = 100 %, więc dziesiąta część procenta to dziesięć punktów bazowych.
    let procent = crate::fmt::decimal(l, i64::from(deviation_bp) / 10, 1);
    c.fmt_key(
        l,
        "ui.reason.PolicyDeviation",
        &[("dni", &dni), ("odchylenie", &procent)],
    )
}

/// Rodzaj cennika w kontrakcie dostawy (M6c §5.8). Dwa słowa zamiast wariantu enuma:
/// gracza obchodzi wyłącznie to, czy cena stoi, czy chodzi za indeksem — `Collar`
/// jest indeksowany z klamrą, więc po jego stronie zdania nic się nie zmienia.
#[must_use]
pub fn contract_pricing(c: &Catalog, l: Locale, indexed: bool) -> String {
    let key = if indexed {
        "ui.contract.pricing.indexed"
    } else {
        "ui.contract.pricing.fixed"
    };
    c.fmt_key(l, key, &[])
}

/// Nazwa skutku deprywacji.
#[must_use]
pub fn deprivation(c: &Catalog, l: Locale, d: DeprivationEffect) -> String {
    c.fmt_key(l, &format!("ui.deprivation.{}", d.name()), &[])
}

/// Nazwa klasy społecznej.
#[must_use]
pub fn social_class(c: &Catalog, l: Locale, k: SocialClass) -> String {
    c.fmt_key(l, &format!("ui.class.{}", k.name()), &[])
}

/// Nazwa typu gospodarstwa.
#[must_use]
pub fn household_kind(c: &Catalog, l: Locale, k: magnat_agents::HouseholdKind) -> String {
    c.fmt_key(l, &format!("ui.household.{}", k.name()), &[])
}

/// Minuty jako odmieniony liczebnik.
#[must_use]
pub fn minutes(c: &Catalog, l: Locale, n: u16) -> String {
    c.plural_key(l, "ui.unit.minutes", u64::from(n))
}

/// Dni jako odmieniony liczebnik. Alias, bo nazwa pola `days` w wariancie powodu
/// przesłania nazwę funkcji w ramieniu `match`.
#[must_use]
pub fn days_txt(c: &Catalog, l: Locale, n: u32) -> String {
    days(c, l, n)
}

/// Dni jako odmieniony liczebnik.
#[must_use]
pub fn days(c: &Catalog, l: Locale, n: u32) -> String {
    c.plural_key(l, "ui.unit.days", u64::from(n))
}

/// Lata jako odmieniony liczebnik.
#[must_use]
pub fn years(c: &Catalog, l: Locale, n: u32) -> String {
    c.plural_key(l, "ui.unit.years", u64::from(n))
}

/// Nazwy, których `engine/ui` nie zna, bo mieszkają w danych symulacji.
///
/// Jeden punkt wstrzyknięcia zamiast drugiej kopii zdania (`FF-10`). `describe`
/// dostaje `&Catalog` i `Locale`, a drzewo technologii mieszka w `sim/firms`
/// i w `data/tech/` — ta sama granica, którą `GoodId` ma od M6. Zamiast przenosić
/// drzewo albo składać zdanie drugi raz po stronie `game/`, wołający, który drzewo
/// **ma**, podaje odwzorowanie identyfikatora na nazwę.
///
/// `None` w polu znaczy „nie wiem" i daje etykietę zastępczą — numer węzła. Świat
/// bez R&D (scenariusze M3–M8) nie płaci za ten mechanizm ani jednej gałęzi.
#[derive(Clone, Copy, Default)]
pub struct Names<'a> {
    /// Nazwa węzła technologii w języku gracza.
    pub tech: Option<&'a dyn Fn(magnat_core::TechId) -> Option<String>>,
}

/// Powód decyzji w języku gracza.
///
/// **Ta funkcja jest jedynym miejscem, w którym `DecisionReason` staje się tekstem.**
/// Karta inspekcji, wydruk osi dnia i konsola deweloperska wołają ją — nie mają jak
/// się rozjechać, bo nie ma drugiej.
#[must_use]
pub fn describe(c: &Catalog, l: Locale, r: DecisionReason) -> String {
    describe_named(c, l, r, Names::default())
}

mod citizen;
mod city;
mod firm;

/// To samo, z nazwami, których interfejs sam nie zna (patrz [`Names`]).
///
/// Od R2e **dyspozytor na trzy ramiona**, a nie `match` na 1290 linii (`R2-WP20`).
/// Gwarancja z `K-12` nie osłabła: wyczerpujący `match` bez `_` obowiązuje tutaj
/// **i** w każdym z trzech plików aktorów, więc wariant bez zdania nadal łamie
/// kompilację. Zmieniło się to, że dopisanie powodu firmie nie dotyka pliku
/// mieszkańca — a to była jedyna przyczyna, dla której tamta funkcja rosła.
#[must_use]
pub fn describe_named(c: &Catalog, l: Locale, r: DecisionReason, n: Names<'_>) -> String {
    match r {
        DecisionReason::Unspecified => c.fmt_key(l, "ui.reason.Unspecified", &[]),
        DecisionReason::Citizen(x) => citizen::opisz(c, l, x, n),
        DecisionReason::Firm(x) => firm::opisz(c, l, x, n),
        DecisionReason::City(x) => city::opisz(c, l, x, n),
    }
}

/// Wycena całej firmy z kursu jednego punktu bazowego.
///
/// Mnożenie nasycające, a nie `checked`: karta inspekcji ma pokazać liczbę, a nie
/// zniknąć przy kursie, który i tak nie mógłby powstać z fixingu.
fn wycena_firmy(price: Money) -> Money {
    Money(price.get().saturating_mul(10_000))
}

/// Nazwa rodzaju ryzyka ubezpieczeniowego (M10d §5.5).
#[must_use]
pub fn peril_kind(c: &Catalog, l: Locale, p: magnat_core::PerilKind) -> String {
    c.fmt_key(l, &format!("ui.peril.{}", p.name()), &[])
}

/// Posiadacz pakietu jako etykieta w karcie inspekcji.
///
/// `ponytail:` numer encji zamiast nazwy — ten sam sufit i ta sama droga wyjścia
/// co przy [`marka`] i [`technologia`]: `describe` dostaje katalog tekstów
/// i `Locale`, a nazwiska mieszkają w `sim/agents`, nazwy firm w `sim/firms`.
/// Kartę z odnośnikiem zbuduje `game::inspect` (`K-62`), który widzi jedno i drugie.
fn podmiot(s: magnat_core::Subject) -> String {
    match s.entity() {
        Some(e) => format!("#{}", e.index()),
        None => String::new(),
    }
}

/// Technologia jako etykieta w karcie inspekcji.
///
/// Nazwa, jeśli wołający ją zna (`Names::tech`, `FF-10`), inaczej numer węzła.
/// Numer zostaje jako etykieta zastępcza, a nie jako brak: powód bez nazwy
/// technologii nadal mówi, **której** technologii dotyczy.
fn technologia(n: Names<'_>, t: magnat_core::TechId) -> String {
    n.tech
        .and_then(|f| f(t))
        .unwrap_or_else(|| format!("#{}", t.0))
}

/// Marka jako etykieta w karcie inspekcji.
///
/// `ponytail:` numer marki zamiast nazwy firmy. Sufit nazwany: `reason::describe`
/// dostaje `&Catalog` i `Locale`, a nie rejestr firm — nazwa firmy jest stanem
/// świata, nie tekstem. Droga wyjścia: `Subject::Firm(magnat_supply::firm_of(brand))`
/// jako odnośnik w karcie (`K-62`), kiedy karta marki powstanie w M10f.
fn marka(b: magnat_core::BrandId) -> String {
    format!("#{}", b.0)
}

/// Skąd mieszkaniec zna markę (M10b §5.1).
#[must_use]
pub fn touch_source(c: &Catalog, l: Locale, s: TouchSource) -> String {
    c.fmt_key(l, &format!("ui.touch_source.{}", s.name()), &[])
}

/// Kanał kampanii reklamowej (M10b §5.2).
#[must_use]
pub fn ad_channel(c: &Catalog, l: Locale, k: AdChannelKind) -> String {
    c.fmt_key(l, &format!("ui.ad_channel.{}", k.name()), &[])
}

/// Linia redakcyjna tytułu (M10b §5.3).
#[must_use]
pub fn editorial_bias(c: &Catalog, l: Locale, b: EditorialBias) -> String {
    c.fmt_key(l, &format!("ui.editorial_bias.{}", b.name()), &[])
}

/// Rodzaj uchwały rady jako nazwa (M8e).
#[must_use]
pub fn policy_kind(c: &Catalog, l: Locale, k: magnat_core::PolicyKind) -> String {
    c.fmt_key(l, &format!("ui.policy.{}", k.name()), &[])
}

/// Przedmiot przetargu jako nazwa (M8e).
#[must_use]
pub fn tender_kind(c: &Catalog, l: Locale, k: magnat_core::TenderKind) -> String {
    c.fmt_key(l, &format!("ui.tender.{}", k.name()), &[])
}

/// Motyw głosu wyborcy jako nazwa (M8e).
#[must_use]
pub fn vote_driver(c: &Catalog, l: Locale, k: magnat_core::VoteDriver) -> String {
    c.fmt_key(l, &format!("ui.vote.{}", k.name()), &[])
}

/// Rodzaj usługi publicznej jako nazwa (M8d).
#[must_use]
pub fn service_kind(c: &Catalog, l: Locale, k: magnat_core::ServiceKind) -> String {
    c.fmt_key(l, &format!("ui.service.{}", k.name()), &[])
}

/// Urząd kontrolny jako nazwa (M8d).
#[must_use]
pub fn agency_kind(c: &Catalog, l: Locale, k: magnat_core::AgencyKind) -> String {
    c.fmt_key(l, &format!("ui.agency.{}", k.name()), &[])
}

/// Środek zaradczy jako nazwa (M8d).
#[must_use]
pub fn remedy_kind(c: &Catalog, l: Locale, k: magnat_core::RemedyKind) -> String {
    c.fmt_key(l, &format!("ui.remedy.{}", k.name()), &[])
}

/// Rodzaj pozwolenia jako nazwa (M8d).
#[must_use]
pub fn permit_kind(c: &Catalog, l: Locale, k: magnat_core::PermitKind) -> String {
    c.fmt_key(l, &format!("ui.permit.{}", k.name()), &[])
}

/// Kierunek prognozy jako słowo. **Jedyna** rzecz, którą model makro mówi graczowi
/// o wielkości — czyli nic o wielkości, tylko o znaku (`Trend`, `K-52`).
#[must_use]
pub fn trend(c: &Catalog, l: Locale, t: magnat_core::Trend) -> String {
    c.fmt_key(l, &format!("ui.trend.{}", t.name()), &[])
}

/// Wynik drugiego kandydata albo informacja, że drugiego nie było.
///
/// `i32::MIN` znaczy „jedyny chętny", a nie „kandydat fatalny" — i te dwa zdania
/// muszą się różnić, bo oferta z jednym chętnym mówi o rynku pracy coś innego
/// niż oferta, w której ktoś przegrał.
fn drugi_w_kolejce(c: &Catalog, l: Locale, runner_up: i32) -> String {
    if runner_up == i32::MIN {
        c.fmt_key(l, "ui.reason.no_runner_up", &[])
    } else {
        runner_up.to_string()
    }
}

/// Pokrycie zapasu w godzinach z jednym miejscem po przecinku. Minuty są jednostką
/// kaskady, ale gracz myśli w godzinach — „zostały ci 3,2 h mąki" jest zdaniem,
/// a „192 min" jest odczytem z przyrządu.
fn godziny(minuty: u32) -> String {
    format!("{},{}", minuty / 60, minuty % 60 * 10 / 60)
}

/// Punkty bazowe jako procent z jednym miejscem po przecinku, ze znakiem.
/// Format jest ten sam w obu językach — separator dziesiętny lokalizuje M12
/// razem z resztą formatów liczbowych.
///
/// Publiczne od M5e: marża półki i różnica wobec ceny konkurenta w panelu sklepu
/// są tą samą wielkością co `delta_bp` w powodach decyzji i mają wyglądać tak samo.
#[must_use]
pub fn procent_bp(bp: i32) -> String {
    let znak = if bp < 0 { "-" } else { "+" };
    let a = bp.abs();
    format!("{znak}{},{}%", a / 100, (a % 100) / 10)
}

fn commitment(c: &Catalog, l: Locale, k: CommitmentKind) -> String {
    c.fmt_key(l, &format!("ui.commitment.{}", k.name()), &[])
}

fn migration(c: &Catalog, l: Locale, k: MigrationKind) -> String {
    c.fmt_key(l, &format!("ui.migration.{}", k.name()), &[])
}

fn life_event(c: &Catalog, l: Locale, k: LifeEventKind) -> String {
    c.fmt_key(l, &format!("ui.life.{}", k.name()), &[])
}

/// Minuta doby jako `HH:MM`. Format jest ten sam w obu językach — zegar dwunastogodzinny
/// dołoży M12 razem z pełną lokalizacją formatów.
#[must_use]
pub fn zegar(minuta: u16) -> String {
    format!("{:02}:{:02}", minuta / 60 % 24, minuta % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{MinuteOfDay, PlaceRef, Q};

    /// Wszystkie warianty bloków M3 i M4 — ta sama lista co w teście dyskryminant `core`.
    fn wszystkie() -> Vec<DecisionReason> {
        vec![
            DecisionReason::Unspecified,
            DecisionReason::Citizen(CitizenReason::Commitment {
                kind: CommitmentKind::Work,
            }),
            DecisionReason::Citizen(CitizenReason::NeedCritical {
                need: NeedKind::Sleep,
                level: Q::new(12),
            }),
            DecisionReason::Citizen(CitizenReason::StockBelowThreshold {
                cat: StockCat::Food,
                days_left: 1,
            }),
            DecisionReason::Citizen(CitizenReason::FreeTimePreference {
                trait_id: TraitId::Sociability,
                weight: 7,
            }),
            DecisionReason::Citizen(CitizenReason::NoTimeWindow {
                need: NeedKind::Health,
                needed_min: 45,
                longest_gap_min: 20,
            }),
            DecisionReason::Citizen(CitizenReason::SlotBudgetExhausted {
                dropped: NeedKind::Clothing,
            }),
            DecisionReason::Citizen(CitizenReason::ChosenNearest {
                travel_min: 8,
                runner_up_min: 14,
            }),
            DecisionReason::Citizen(CitizenReason::ChosenOnRoute {
                detour_min: 3,
                direct_min: 9,
            }),
            DecisionReason::Citizen(CitizenReason::PlaceUnknown {
                need: NeedKind::Hunger,
                known_count: 0,
            }),
            DecisionReason::Citizen(CitizenReason::PlaceClosed {
                place: PlaceRef::default(),
                opens_at: MinuteOfDay::new(7 * 60),
            }),
            DecisionReason::Citizen(CitizenReason::Arrived {
                planned: MinuteOfDay::new(480),
                actual: MinuteOfDay::new(482),
            }),
            DecisionReason::Citizen(CitizenReason::Replanned {
                cause_tag: 2,
                slots_changed: 3,
            }),
            DecisionReason::Citizen(CitizenReason::Deprivation {
                need: NeedKind::Sleep,
                effect: DeprivationEffect::AbsenceRisk,
            }),
            DecisionReason::Citizen(CitizenReason::ModeWalkOnly { minutes: 3 }),
            DecisionReason::Citizen(CitizenReason::NeedSatisfied {
                need: NeedKind::Hunger,
                gain: Q::new(40),
            }),
            DecisionReason::Citizen(CitizenReason::PartnerChosen {
                compatibility: Q::new(80),
                candidates: 4,
            }),
            DecisionReason::Citizen(CitizenReason::SeparationFiled {
                stress: Q::new(70),
                years_together: 12,
            }),
            DecisionReason::Citizen(CitizenReason::MigrationDecision {
                kind: MigrationKind::Arrived,
                months_jobless: 0,
            }),
            DecisionReason::Citizen(CitizenReason::Inheritance {
                permille: 500,
                heirs: 2,
            }),
            DecisionReason::Citizen(CitizenReason::LifeEvent {
                kind: LifeEventKind::Died,
            }),
            DecisionReason::Citizen(CitizenReason::ModeChosen {
                mode: TransportMode::Bus,
                minutes: 24,
            }),
            DecisionReason::Citizen(CitizenReason::NoRouteForMode {
                mode: TransportMode::Car,
                fallback: TransportMode::Walk,
            }),
            DecisionReason::Citizen(CitizenReason::RefuelNeeded { level_permille: 80 }),
            DecisionReason::Citizen(CitizenReason::StationChosen {
                detour_min: 4,
                price_gr_per_l: 649,
            }),
            DecisionReason::Citizen(CitizenReason::TripDelayed {
                planned_min: 18,
                actual_min: 31,
            }),
            DecisionReason::Citizen(CitizenReason::ModeCompared {
                chosen: TransportMode::Bus,
                runner_up: TransportMode::Car,
                delta_gr: -320,
            }),
            DecisionReason::Citizen(CitizenReason::NoParkingAtDestination { lots_searched: 4 }),
            DecisionReason::Citizen(CitizenReason::LeftBehind {
                line: 12,
                waited_min: 9,
            }),
            DecisionReason::Citizen(CitizenReason::ShopChosen {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                dominant: UtilityKind::Price,
                delta_bp: -1_200,
            }),
            DecisionReason::Citizen(CitizenReason::OfferRejected {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                cause: RejectCause::OutOfStock,
                detail: 0,
            }),
            DecisionReason::Citizen(CitizenReason::PurchaseDeferred {
                need: NeedKind::Hunger,
                cause: RejectCause::BelowThreshold,
                gap_permille: -140,
            }),
            DecisionReason::Firm(FirmReason::Repricing {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                good: magnat_core::GoodId(3),
                driver: PriceDriver::Stock,
                delta_bp: -450,
            }),
            DecisionReason::Citizen(CitizenReason::CreditApproved {
                kind: LoanKind::Consumer,
                rate_bp: 1_200,
                load_bp: 2_800,
            }),
            DecisionReason::Citizen(CitizenReason::CreditRejected {
                kind: LoanKind::WorkingCapital,
                cause: RejectCredit::DscrTooLow,
                margin_bp: -900,
            }),
            DecisionReason::Citizen(CitizenReason::BudgetShortfall {
                cost: FixedCost::Housing,
                gap_permille: 420,
            }),
            DecisionReason::Firm(FirmReason::Shortage {
                good: magnat_core::GoodId(3),
                from: ShortageStageKind::Buffer,
                to: ShortageStageKind::Throttled,
                coverage_minutes: 192,
            }),
            DecisionReason::Firm(FirmReason::ProductionHalted {
                site: magnat_core::SiteId(magnat_core::Entity::new(9, std::num::NonZeroU32::MIN)),
                line: 1,
                cause: LineStopCause::NoPower,
            }),
            DecisionReason::Firm(FirmReason::SubstituteUsed {
                good: magnat_core::GoodId(3),
                alt: magnat_core::GoodId(4),
                quality_loss: 8,
            }),
            DecisionReason::Firm(FirmReason::SupplierChosen {
                good: magnat_core::GoodId(3),
                seller: magnat_core::FirmId(magnat_core::Entity::new(4, std::num::NonZeroU32::MIN)),
                quotes: 5,
                saving_bp: 320,
            }),
            DecisionReason::Firm(FirmReason::ContractSigned {
                good: magnat_core::GoodId(3),
                seller: magnat_core::FirmId(magnat_core::Entity::new(4, std::num::NonZeroU32::MIN)),
                months: 12,
                indexed: true,
            }),
            // Drugi wariant cennika ma własny klucz, więc bez tego wpisu test
            // „każdy powód ma tekst w obu językach" nie dotknąłby `...pricing.fixed`.
            DecisionReason::Firm(FirmReason::ContractSigned {
                good: magnat_core::GoodId(3),
                seller: magnat_core::FirmId(magnat_core::Entity::new(4, std::num::NonZeroU32::MIN)),
                months: 6,
                indexed: false,
            }),
            DecisionReason::Firm(FirmReason::ExportChosen {
                good: magnat_core::GoodId(3),
                premium_bp: 1_450,
                mass_kg: 24_000,
            }),
            DecisionReason::Firm(FirmReason::Hired {
                role: magnat_core::JobRoleId(4),
                score: 780,
                runner_up: 640,
            }),
            // Jedyny chętny ma własny klucz, tak samo jak drugi wariant cennika wyżej.
            DecisionReason::Firm(FirmReason::Hired {
                role: magnat_core::JobRoleId(4),
                score: 780,
                runner_up: i32::MIN,
            }),
            DecisionReason::Firm(FirmReason::WageRaise {
                role: magnat_core::JobRoleId(4),
                delta_bp: 930,
                days_open: 14,
                cause: WageCause::NoCandidates,
            }),
            // Krok przycięty do sufitu marży — drugie zdanie, nie ta sama liczba.
            DecisionReason::Firm(FirmReason::WageRaise {
                role: magnat_core::JobRoleId(4),
                delta_bp: 0,
                days_open: 21,
                cause: WageCause::Ceiling,
            }),
            DecisionReason::Firm(FirmReason::JobLeft {
                role: magnat_core::JobRoleId(4),
                cause: LeaveCause::BetterOffer,
                tenure_days: 420,
            }),
            DecisionReason::Firm(FirmReason::PolicyApplied {
                policy: magnat_core::PolicyId(3),
                rule: 0,
                action: ActionKind::SetPrice,
                lag_days: 4,
                deviation_bp: 90,
            }),
            // Menedżer doskonały nie dokłada zdania — trzeci wpis sprawdza tę gałąź,
            // bo pusty dopisek jest tu decyzją, a nie brakiem tekstu.
            DecisionReason::Firm(FirmReason::PolicyApplied {
                policy: magnat_core::PolicyId(3),
                rule: 2,
                action: ActionKind::Markdown,
                lag_days: 1,
                deviation_bp: 0,
            }),
            // Reguła zapasowa wybiera **inny klucz** lokalizacji, więc bez drugiego
            // wpisu połowa ramienia zostałaby niesprawdzona — ten sam powód, dla
            // którego `Hired` i `WageRaise` stoją na tej liście po dwa razy.
            DecisionReason::Firm(FirmReason::PolicyApplied {
                policy: magnat_core::PolicyId(3),
                rule: u8::MAX,
                action: ActionKind::SetMargin,
                lag_days: 7,
                deviation_bp: -250,
            }),
            DecisionReason::Firm(FirmReason::ManagerAssigned {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                skill_mgmt: Q::new(71),
                prev: 50,
            }),
            DecisionReason::Firm(FirmReason::LoanTaken {
                kind: LoanKind::Investment,
                rate_bp: 740,
                term_months: 60,
            }),
            DecisionReason::Firm(FirmReason::LeaseSigned {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                months: 36,
            }),
            DecisionReason::Firm(FirmReason::ReceivablesFactored {
                count: 4,
                discount_bp: 400,
            }),
            DecisionReason::Firm(FirmReason::BondIssued {
                coupon_bp: 1100,
                months: 36,
            }),
            // Upadłość z braku płynności wybiera **inny klucz** niż upadłość
            // bilansowa — ten sam powód, dla którego `PolicyApplied` stoi tu dwa razy.
            DecisionReason::Firm(FirmReason::BankruptcyOpened {
                trigger: BankruptcyTrigger::Illiquid,
                days: 92,
            }),
            DecisionReason::Firm(FirmReason::BankruptcyOpened {
                trigger: BankruptcyTrigger::NegativeEquity,
                days: 0,
            }),
            DecisionReason::Firm(FirmReason::ClaimSettled {
                priority: ClaimPriority::Wages,
                ratio_bp: 10_000,
            }),
            DecisionReason::Firm(FirmReason::MarginTargetSet {
                good: magnat_core::GoodId(3),
                margin_bp: 1_800,
                prev_bp: 2_500,
            }),
            DecisionReason::Firm(FirmReason::RestockTargetSet {
                good: magnat_core::GoodId(3),
                days: 10,
                prev: 6,
            }),
            DecisionReason::Firm(FirmReason::SiteClosed {
                months: 3,
                margin_bp: -820,
            }),
            DecisionReason::Firm(FirmReason::StrategySet {
                strategy: FirmStrategy::Discount,
                prev: FirmStrategy::Cautious,
            }),
            DecisionReason::Firm(FirmReason::CompetitiveResponse {
                kind: ReactionKind::PriceWar,
                target: magnat_core::SiteId(magnat_core::Entity::new(
                    11,
                    std::num::NonZeroU32::MIN,
                )),
                depth_bp: 1_200,
            }),
            DecisionReason::Firm(FirmReason::SiteOpened {
                district: magnat_core::DistrictId(3),
                variants: 4,
                margin_bp: 620,
                trend: magnat_core::Trend::Up,
            }),
            DecisionReason::Firm(FirmReason::VoluntaryClosure {
                months: 7,
                cash: Money(12_400),
            }),
            DecisionReason::Firm(FirmReason::FirmFounded {
                score: 74,
                capital: Money(1_800_000),
            }),
            DecisionReason::Firm(FirmReason::ChainEntered {
                capital: Money(54_000_000),
                sites: 3,
            }),
            DecisionReason::City(CityReason::TaxAssessed {
                kind: magnat_core::TaxKind::Vat,
                rate_bp: 2300,
                amount: Money(412_900),
            }),
            DecisionReason::City(CityReason::TaxSettled {
                kind: magnat_core::TaxKind::Cit,
                amount: Money(1_140_000),
            }),
            DecisionReason::City(CityReason::TaxOverdue {
                kind: magnat_core::TaxKind::Property,
                days: 41,
                amount: Money(19_200),
            }),
            DecisionReason::City(CityReason::TaxAbated {
                kind: magnat_core::TaxKind::Pit,
                why: magnat_core::AbateReason::TimeBarred,
                amount: Money(8_400),
            }),
            DecisionReason::City(CityReason::PublicSpend {
                category: magnat_core::SpendCategory::Education,
                amount: Money(22_000_000),
            }),
            DecisionReason::City(CityReason::MunicipalBondIssued {
                coupon_bp: 650,
                principal: Money(80_000_000),
            }),
            DecisionReason::City(CityReason::BudgetDeficitClosed {
                gap: Money(4_500_000),
                cut_bp: 1250,
            }),
            DecisionReason::City(CityReason::LoadShed {
                service: magnat_core::UtilityService::Electricity,
                priority: 3,
                shortfall_w: 1_450_000,
            }),
            DecisionReason::City(CityReason::GridTripped {
                service: magnat_core::UtilityService::Electricity,
                repair_minutes: 195,
            }),
            DecisionReason::City(CityReason::EventStarted {
                event: magnat_core::EventId(7),
                category: magnat_core::EventCategory::Natural,
                severity_bps: 4200,
            }),
            DecisionReason::City(CityReason::EventEnded {
                event: magnat_core::EventId(7),
                category: magnat_core::EventCategory::Natural,
                days: 23,
            }),
            DecisionReason::City(CityReason::ServiceQuality {
                kind: magnat_core::ServiceKind::School,
                district: magnat_core::DistrictId(3),
                quality: Q::new(62),
                funding_bp: 7400,
                staff_bp: 8800,
                load_bp: 11_200,
            }),
            DecisionReason::City(CityReason::PermitIssued {
                kind: magnat_core::PermitKind::Build,
                waited_days: 31,
            }),
            DecisionReason::City(CityReason::CaseOpened {
                agency: magnat_core::AgencyKind::TaxOffice,
                evidence: Q::new(20),
            }),
            DecisionReason::City(CityReason::RemedyImposed {
                agency: magnat_core::AgencyKind::TaxOffice,
                remedy: magnat_core::RemedyKind::BackTax,
                amount: Money(1_240_000),
            }),
            // Ósmy wpis dwukrotny: środek bez kwoty wybiera inny klucz.
            DecisionReason::City(CityReason::RemedyImposed {
                agency: magnat_core::AgencyKind::Sanitary,
                remedy: magnat_core::RemedyKind::Closure,
                amount: Money::ZERO,
            }),
            DecisionReason::City(CityReason::ShadowShareSet {
                share_bp: 1800,
                last_result: Money(-420_000),
            }),
            // ── M8e: władza i wybory ──
            // **Powinny tu stać od M8e i nie stały** — ta sama luka, którą komentarz
            // niżej opisuje dla M8c: ramiona w `describe` były, wpisu tutaj nie było,
            // więc przez pięć podfaz nikt nie sprawdził, czy te zdania składają się
            // w obu językach. Znalezione przy dokładaniu bloku M10b.
            DecisionReason::City(CityReason::PolicyEnacted {
                kind: magnat_core::PolicyKind::MinWage,
                for_bp: 6_400,
                delay_days: 30,
            }),
            DecisionReason::City(CityReason::TaxRateChanged {
                kind: magnat_core::TaxKind::Vat,
                from_bp: 2_300,
                to_bp: 2_500,
                gap_bp: -1_200,
            }),
            DecisionReason::City(CityReason::TenderPublished {
                subject: magnat_core::TenderKind::WasteCollection,
                subject_id: 3,
                budget: Money(12_000_000),
            }),
            DecisionReason::City(CityReason::TenderAwarded {
                subject: magnat_core::TenderKind::WasteCollection,
                price: Money(9_800_000),
                score_bp: 7_600,
                runner_up_bp: 7_100,
                bids: 3,
            }),
            // Drugi wpis: przetarg bez ofert wybiera inny klucz.
            DecisionReason::City(CityReason::TenderAwarded {
                subject: magnat_core::TenderKind::Construction,
                price: Money::ZERO,
                score_bp: 0,
                runner_up_bp: 0,
                bids: 0,
            }),
            DecisionReason::City(CityReason::ElectionHeld {
                turnout_bp: 5_400,
                winner_bp: 5_100,
                incumbent: true,
            }),
            // Drugi wpis: zmiana burmistrza wybiera inny klucz.
            DecisionReason::City(CityReason::ElectionHeld {
                turnout_bp: 6_200,
                winner_bp: 4_400,
                incumbent: false,
            }),
            DecisionReason::Citizen(CitizenReason::VoteCast {
                candidate: 1,
                driver: magnat_core::VoteDriver::Taxes,
                margin_bp: 800,
            }),
            DecisionReason::Firm(FirmReason::CampaignBacked {
                candidate: 0,
                amount: Money(2_500_000),
                illegal: false,
            }),
            // Drugi wpis: łapówka wybiera inny klucz niż darowizna.
            DecisionReason::Firm(FirmReason::CampaignBacked {
                candidate: 2,
                amount: Money(9_000_000),
                illegal: true,
            }),
            // ── M10b: marka i media ──
            DecisionReason::Citizen(CitizenReason::BrandLearned {
                brand: magnat_core::BrandId(41),
                source: TouchSource::Ad,
                channel: Some(AdChannelKind::Billboard),
                awareness: Q::new(18),
            }),
            // Drugi wpis: źródło bez kanału wybiera inne podstawienie.
            DecisionReason::Citizen(CitizenReason::BrandLearned {
                brand: magnat_core::BrandId(41),
                source: TouchSource::Rumor,
                channel: None,
                awareness: Q::new(45),
            }),
            DecisionReason::Citizen(CitizenReason::BrandExperience {
                brand: magnat_core::BrandId(41),
                expected: Q::new(80),
                actual: Q::new(55),
                delta: -18,
            }),
            // Drugi wpis: zachwyt wybiera inny klucz niż rozczarowanie.
            DecisionReason::Citizen(CitizenReason::BrandExperience {
                brand: magnat_core::BrandId(41),
                expected: Q::new(50),
                actual: Q::new(70),
                delta: 5,
            }),
            DecisionReason::Firm(FirmReason::AdCampaignStarted {
                brand: magnat_core::BrandId(41),
                channel: AdChannelKind::Tv,
                budget: Money(8_400_000),
                claim: Q::new(88),
            }),
            DecisionReason::Firm(FirmReason::StoryPublished {
                outlet: magnat_core::BrandId(9),
                event: magnat_core::EventId(77),
                bias: EditorialBias::Sensational,
                reach_bp: 3_200,
            }),
            // ── M10c: R&D i nowe produkty ──
            DecisionReason::Firm(FirmReason::ResearchStarted {
                tech: magnat_core::TechId(4),
                cost_rp: 1_200,
                months_est: 9,
            }),
            DecisionReason::Firm(FirmReason::TechDiscovered {
                tech: magnat_core::TechId(4),
                patented: true,
                rp_spent: 1_200,
                months: 9,
            }),
            // Drugi wpis: odkrycie po roku „światowym" wybiera inny klucz — nie ma
            // patentu, więc nie ma tego samego zdania z dopiskiem, tylko inne zdanie.
            DecisionReason::Firm(FirmReason::TechDiscovered {
                tech: magnat_core::TechId(7),
                patented: false,
                rp_spent: 480,
                months: 4,
            }),
            DecisionReason::Firm(FirmReason::LicenseSigned {
                tech: magnat_core::TechId(4),
                licensor: magnat_core::FirmId(magnat_core::Entity::new(
                    12,
                    std::num::NonZeroU32::MIN,
                )),
                royalty_bp: 450,
            }),
            DecisionReason::Firm(FirmReason::ProductLaunched {
                good: magnat_core::GoodId(11),
                tech: magnat_core::TechId(4),
                shops: 23,
            }),
            // ── M10d ────────────────────────────────────────────────────────────
            DecisionReason::Firm(FirmReason::StockListed {
                firm: magnat_core::FirmId(magnat_core::Entity::new(12, std::num::NonZeroU32::MIN)),
                price: magnat_core::Money(4_200),
            }),
            DecisionReason::Firm(FirmReason::StockFixing {
                firm: magnat_core::FirmId(magnat_core::Entity::new(12, std::num::NonZeroU32::MIN)),
                price: magnat_core::Money(4_350),
            }),
            // Dwa wpisy, bo posiadacz-osoba i posiadacz-firma wybierają inny klucz
            // lokalizacji — ta sama zasada, co przy `TechDiscovered` wyżej.
            DecisionReason::Firm(FirmReason::StakeDisclosed {
                holder: magnat_core::Subject::Citizen(magnat_core::CitizenId(
                    magnat_core::Entity::new(7, std::num::NonZeroU32::MIN),
                )),
            }),
            DecisionReason::Firm(FirmReason::StakeDisclosed {
                holder: magnat_core::Subject::Firm(magnat_core::FirmId(magnat_core::Entity::new(
                    9,
                    std::num::NonZeroU32::MIN,
                ))),
            }),
            DecisionReason::Firm(FirmReason::ControlAcquired {
                holder: magnat_core::Subject::Citizen(magnat_core::CitizenId(
                    magnat_core::Entity::new(7, std::num::NonZeroU32::MIN),
                )),
            }),
            DecisionReason::Firm(FirmReason::ControlAcquired {
                holder: magnat_core::Subject::Firm(magnat_core::FirmId(magnat_core::Entity::new(
                    9,
                    std::num::NonZeroU32::MIN,
                ))),
            }),
            DecisionReason::Firm(FirmReason::DividendPaid {
                firm: magnat_core::FirmId(magnat_core::Entity::new(12, std::num::NonZeroU32::MIN)),
                total: magnat_core::Money(1_250_000),
            }),
            DecisionReason::Firm(FirmReason::SharesIssued {
                bp: 1_500,
                price: magnat_core::Money(3_900),
            }),
            DecisionReason::Firm(FirmReason::PerilStruck {
                peril: magnat_core::PerilKind::Flood,
                district: magnat_core::DistrictId(3),
                loss: magnat_core::Money(840_000),
            }),
            DecisionReason::Firm(FirmReason::Underwritten {
                peril: magnat_core::PerilKind::Fire,
                rate_bp: 320,
                premium: magnat_core::Money(26_000),
            }),
            DecisionReason::Firm(FirmReason::ClaimPaid {
                insurer: magnat_core::FirmId(magnat_core::Entity::new(
                    5,
                    std::num::NonZeroU32::MIN,
                )),
                paid: magnat_core::Money(620_000),
            }),
        ]
    }

    #[test]
    fn kazdy_powod_ma_tekst_w_obu_jezykach() {
        let c = Catalog::load().expect("data/locale/");
        for r in wszystkie() {
            for l in Locale::ALL {
                let t = describe(&c, l, r);
                assert!(!t.is_empty(), "{r:?} w {} jest puste", l.code());
                assert!(
                    !t.contains('{'),
                    "{r:?} w {}: nietrafione podstawienie w `{t}`",
                    l.code()
                );
            }
        }
        // Lista musi być **kompletna**, inaczej bramka nie jest bramką: po M5c mieściła
        // 29 wariantów i nie obejmowała ani `Repricing` (303), ani trzech powodów M4c/M4d
        // (205–207), które miały już ramiona w `describe`. Stan po M5d: Unspecified
        // + 100..=119 + 200..=207 + 300..=306 = 36. Po M6b dochodzi blok M6
        // (400..=402), czyli 39. Po M6c trzy kolejne (403..=405) i **czwarty wpis**:
        // `ContractSigned` stoi na liście dwa razy, bo `indexed` wybiera klucz
        // lokalizacji, a wariant z jednym wpisem zostawiłby drugi klucz niesprawdzony.
        // Po M7b blok M7 (500..=502) plus dwa wpisy z tego samego powodu co wyżej:
        // `Hired` bez drugiego kandydata i `WageRaise` przycięty do sufitu marży
        // wybierają inne klucze — razem 48. Po M7c dochodzą `PolicyApplied` (503,
        // dwa wpisy: reguła zwykła i zapasowa) oraz `ManagerAssigned` (504) — 51.
        // Po M7d blok finansowy (505..=510) i **siódmy wpis**: `BankruptcyOpened`
        // stoi dwa razy, bo brak płynności mierzy się dobami, a ujemny kapitał nie —
        // i to są dwa różne zdania o firmie, więc i dwa klucze. Razem 58.
        // Po M7e pięć powodów AI firm (511..=515), po jednym wpisie — żaden z nich
        // nie rozgałęzia się na dwa klucze lokalizacji. Razem 63.
        // Po M7f cztery powody cyklu życia firm (516..=519), po jednym wpisie.
        // `SiteOpened` nie rozgałęzia się mimo trzech wariantów `Trend`, bo kierunek
        // wchodzi **podstawieniem** do jednego zdania, a nie wyborem klucza — i to
        // jest właściwy podział: „w górę" i „w dół" to ta sama decyzja o innym znaku,
        // a nie dwie różne decyzje. Razem 67.
        // Po M8a siedem powodów miasta (600..=606), po jednym wpisie: danina,
        // kierunek wydatku i przyczyna umorzenia wchodzą **podstawieniem**, tak samo
        // jak `Trend` wyżej — siedem danin nie robi siedmiu zdań o naliczeniu, tylko
        // jedno zdanie z siedmioma podstawieniami. Razem 74.
        // Po M8b dwa powody sieci przesyłowej (607, 608), po jednym wpisie: rodzaj
        // medium wchodzi podstawieniem, więc siedem sieci nie robi czternastu zdań.
        // Razem 76.
        // **Po M8c powinno być 78 i nie było** — `EventStarted` i `EventEnded`
        // (609, 610) miały ramiona w `describe`, ale nie miały wpisu tutaj, więc
        // przez całą podfazę nikt nie sprawdził, czy ich zdanie składa się w obu
        // językach. Uzupełnione w M8d, razem z własnym blokiem.
        // Po M8d pięć powodów usług, urzędów i egzekucji (611..=615) plus **ósmy
        // wpis dwukrotny**: `RemedyImposed` stoi dwa razy, bo kara z kwotą i kara
        // bez kwoty wybierają inne klucze lokalizacji. Razem 78 + 6 = 84.
        // Po M9d **trzeci wpis `PolicyApplied`**: menedżer doskonały nie dokłada
        // dopisku o wieku danych i odchyłce, a menedżer słaby dokłada — to są dwa
        // różne zdania z jednego ramienia, więc oba muszą tu stać. Razem 85.
        // **Blok M8e (616..=622) dopisany dopiero w M10b** — siedem powodów władzy
        // i wyborów plus trzy wpisy dwukrotne (przetarg bez ofert, zmiana burmistrza,
        // łapówka), razem 10. Ta sama luka co przy M8c i ten sam wniosek: ramię
        // w `describe` kompilator wymusza, wpisu w tej liście nie wymusza nikt.
        // Razem 95.
        // Po M10b cztery powody marki i mediów (800..=803) plus **dwa wpisy
        // dwukrotne**: `BrandLearned` ze źródła z kanałem i bez kanału, oraz
        // `BrandExperience` przy rozczarowaniu i przy spełnionych oczekiwaniach.
        // Razem 101.
        // Po M10c cztery powody R&D (804..=807) plus **jeden wpis dwukrotny**:
        // `TechDiscovered` z patentem i bez patentu to dwa różne zdania o tym samym
        // odkryciu, bo różnicę robi rok „światowy", a nie znak liczby. Razem 106.
        // Po M10d dziewięć powodów giełdy i ubezpieczeń (808..=816) plus **dwa wpisy
        // dwukrotne**: `StakeDisclosed` i `ControlAcquired` wybierają inny klucz dla
        // posiadacza-osoby i posiadacza-firmy, bo po polsku różnią się rodzajem
        // czasownika, a po angielsku rzeczownikiem. Razem 117.
        assert_eq!(wszystkie().len(), 117);
    }

    #[test]
    fn kazdy_slownik_domenowy_ma_nazwy() {
        let c = Catalog::load().expect("data/locale/");
        for l in Locale::ALL {
            for n in NeedKind::ALL {
                assert!(!need(&c, l, *n).is_empty());
            }
            for a in ActivityKind::ALL {
                assert!(!activity(&c, l, *a).is_empty());
            }
            for s in StockCat::ALL {
                assert!(!stock(&c, l, *s).is_empty());
            }
            for t in TraitId::ALL {
                assert!(!trait_name(&c, l, *t).is_empty());
            }
            for d in DeprivationEffect::ALL {
                assert!(!deprivation(&c, l, *d).is_empty());
            }
            for m in TransportMode::ALL {
                assert!(!transport_mode(&c, l, *m).is_empty());
            }
            for u in UtilityKind::ALL {
                assert!(!utility_term(&c, l, *u).is_empty());
            }
            for d in PriceDriver::ALL {
                assert!(!price_driver(&c, l, *d).is_empty());
            }
            for k in LoanKind::ALL {
                assert!(!loan_kind(&c, l, *k).is_empty());
            }
            for r in RejectCredit::ALL {
                assert!(!reject_credit(&c, l, *r).is_empty());
            }
            for a in ActionKind::ALL {
                assert!(!action_kind(&c, l, *a).is_empty());
            }
            for f in FixedCost::ALL {
                assert!(!fixed_cost(&c, l, *f).is_empty());
            }
            for r in RejectCause::ALL {
                assert!(!reject_cause(&c, l, *r).is_empty());
            }
            for b in BankruptcyTrigger::ALL {
                assert!(!bankruptcy_trigger(&c, l, *b).is_empty());
            }
            for p in ClaimPriority::ALL {
                assert!(!claim_priority(&c, l, *p).is_empty());
            }
            for s in FirmStrategy::ALL {
                assert!(!firm_strategy(&c, l, *s).is_empty());
            }
            for r in ReactionKind::ALL {
                assert!(!reaction_kind(&c, l, *r).is_empty());
            }
            for k in magnat_core::TaxKind::ALL {
                assert!(!tax_kind(&c, l, *k).is_empty());
            }
            for p in magnat_core::PerilKind::ALL {
                assert!(!peril_kind(&c, l, *p).is_empty());
            }
            for s in magnat_core::SpendCategory::ALL {
                assert!(!spend_category(&c, l, *s).is_empty());
            }
            for a in magnat_core::AbateReason::ALL {
                assert!(!abate_reason(&c, l, *a).is_empty());
            }
            for k in magnat_core::ServiceKind::ALL {
                assert!(!service_kind(&c, l, *k).is_empty());
            }
            for a in magnat_core::AgencyKind::ALL {
                assert!(!agency_kind(&c, l, *a).is_empty());
            }
            for r in magnat_core::RemedyKind::ALL {
                assert!(!remedy_kind(&c, l, *r).is_empty());
            }
            for k in magnat_core::PermitKind::ALL {
                assert!(!permit_kind(&c, l, *k).is_empty());
            }
            for k in magnat_core::PolicyKind::ALL {
                assert!(!policy_kind(&c, l, *k).is_empty());
            }
            for k in magnat_core::TenderKind::ALL {
                assert!(!tender_kind(&c, l, *k).is_empty());
            }
            for k in magnat_core::VoteDriver::ALL {
                assert!(!vote_driver(&c, l, *k).is_empty());
            }
        }
    }

    #[test]
    fn zegar_zawija_dobe() {
        assert_eq!(zegar(0), "00:00");
        assert_eq!(zegar(8 * 60 + 5), "08:05");
        assert_eq!(zegar(1439), "23:59");
    }
}
