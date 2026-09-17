//! `DecisionReason` → tekst. **Jedno miejsce w całej grze** (M3 §6.1).
//!
//! `match` jest wyczerpujący i bez ramienia `_` — to jest cały mechanizm z 00 §K-12:
//! faza, która dopisze wariant, **nie skompiluje interfejsu**, dopóki nie napisze,
//! jak ten powód pokazać graczowi. Test `kazdy_powod_ma_tekst` sprawdza to od drugiej
//! strony: każdy wariant musi dać niepusty napis w obu językach.

use crate::loc::{Catalog, Locale};
use magnat_agents::SocialClass;
use magnat_core::{
    ActionKind, ActivityKind, BankruptcyTrigger, ClaimPriority, CommitmentKind, DecisionReason,
    DeprivationEffect, FirmStrategy, FixedCost, LeaveCause, LifeEventKind, LineStopCause, LoanKind,
    MigrationKind, Money, NeedKind, PriceDriver, ReactionKind, RejectCause, RejectCredit,
    ShortageStageKind, StockCat, TraitId, TransportMode, UtilityKind, WageCause,
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
    c.plural(l, c.must("ui.unit.months"), u64::from(n))
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
    c.plural(l, c.must("ui.unit.minutes"), u64::from(n))
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
    c.plural(l, c.must("ui.unit.days"), u64::from(n))
}

/// Lata jako odmieniony liczebnik.
#[must_use]
pub fn years(c: &Catalog, l: Locale, n: u32) -> String {
    c.plural(l, c.must("ui.unit.years"), u64::from(n))
}

/// Powód decyzji w języku gracza.
///
/// **Ta funkcja jest jedynym miejscem, w którym `DecisionReason` staje się tekstem.**
/// Karta inspekcji, wydruk osi dnia i konsola deweloperska wołają ją — nie mają jak
/// się rozjechać, bo nie ma drugiej.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn describe(c: &Catalog, l: Locale, r: DecisionReason) -> String {
    match r {
        DecisionReason::Unspecified => c.fmt_key(l, "ui.reason.Unspecified", &[]),
        DecisionReason::Commitment { kind } => c.fmt_key(
            l,
            "ui.reason.Commitment",
            &[("co", &commitment(c, l, kind))],
        ),
        DecisionReason::NeedCritical { need: n, level } => c.fmt_key(
            l,
            "ui.reason.NeedCritical",
            &[("co", &need(c, l, n)), ("poziom", &level.get().to_string())],
        ),
        DecisionReason::StockBelowThreshold { cat, days_left } => c.fmt_key(
            l,
            "ui.reason.StockBelowThreshold",
            &[
                ("co", &stock(c, l, cat)),
                ("dni", &days(c, l, u32::from(days_left))),
            ],
        ),
        DecisionReason::FreeTimePreference { trait_id, weight } => c.fmt_key(
            l,
            "ui.reason.FreeTimePreference",
            &[
                ("co", &trait_name(c, l, trait_id)),
                ("waga", &weight.to_string()),
            ],
        ),
        DecisionReason::NoTimeWindow {
            need: n,
            needed_min,
            longest_gap_min,
        } => c.fmt_key(
            l,
            "ui.reason.NoTimeWindow",
            &[
                ("co", &need(c, l, n)),
                ("ile", &minutes(c, l, needed_min)),
                ("luka", &minutes(c, l, longest_gap_min)),
            ],
        ),
        DecisionReason::SlotBudgetExhausted { dropped } => c.fmt_key(
            l,
            "ui.reason.SlotBudgetExhausted",
            &[("co", &need(c, l, dropped))],
        ),
        DecisionReason::ChosenNearest {
            travel_min,
            runner_up_min,
        } => c.fmt_key(
            l,
            "ui.reason.ChosenNearest",
            &[
                ("czas", &minutes(c, l, travel_min)),
                (
                    "gorsze",
                    &minutes(c, l, runner_up_min.saturating_sub(travel_min)),
                ),
            ],
        ),
        DecisionReason::ChosenOnRoute {
            detour_min,
            direct_min,
        } => c.fmt_key(
            l,
            "ui.reason.ChosenOnRoute",
            &[
                ("objazd", &minutes(c, l, detour_min)),
                ("wprost", &minutes(c, l, direct_min)),
            ],
        ),
        DecisionReason::PlaceUnknown {
            need: n,
            known_count,
        } => c.fmt_key(
            l,
            "ui.reason.PlaceUnknown",
            &[("co", &need(c, l, n)), ("ile", &known_count.to_string())],
        ),
        DecisionReason::PlaceClosed { place: _, opens_at } => c.fmt_key(
            l,
            "ui.reason.PlaceClosed",
            &[("otwarcie", &zegar(opens_at.get()))],
        ),
        DecisionReason::Arrived { planned, actual } => c.fmt_key(
            l,
            "ui.reason.Arrived",
            &[
                ("faktycznie", &zegar(actual.get())),
                ("plan", &zegar(planned.get())),
            ],
        ),
        DecisionReason::Replanned {
            cause_tag: _,
            slots_changed,
        } => c.fmt_key(
            l,
            "ui.reason.Replanned",
            &[("ile", &slots_changed.to_string())],
        ),
        DecisionReason::Deprivation { need: n, effect } => c.fmt_key(
            l,
            "ui.reason.Deprivation",
            &[
                ("co", &need(c, l, n)),
                ("skutek", &deprivation(c, l, effect)),
            ],
        ),
        DecisionReason::ModeWalkOnly { minutes: m } => {
            c.fmt_key(l, "ui.reason.ModeWalkOnly", &[("czas", &minutes(c, l, m))])
        }
        DecisionReason::NeedSatisfied { need: n, gain } => c.fmt_key(
            l,
            "ui.reason.NeedSatisfied",
            &[("co", &need(c, l, n)), ("zysk", &gain.get().to_string())],
        ),
        DecisionReason::PartnerChosen {
            compatibility,
            candidates,
        } => c.fmt_key(
            l,
            "ui.reason.PartnerChosen",
            &[
                ("zgodnosc", &compatibility.get().to_string()),
                ("ilu", &candidates.to_string()),
            ],
        ),
        DecisionReason::SeparationFiled {
            stress,
            years_together,
        } => c.fmt_key(
            l,
            "ui.reason.SeparationFiled",
            &[
                ("lata", &years(c, l, u32::from(years_together))),
                ("stres", &stress.get().to_string()),
            ],
        ),
        DecisionReason::MigrationDecision {
            kind,
            months_jobless,
        } => c.fmt_key(
            l,
            "ui.reason.MigrationDecision",
            &[
                ("co", &migration(c, l, kind)),
                ("miesiace", &months_jobless.to_string()),
            ],
        ),
        DecisionReason::Inheritance { permille, heirs } => c.fmt_key(
            l,
            "ui.reason.Inheritance",
            &[
                ("promile", &permille.to_string()),
                ("ilu", &heirs.to_string()),
            ],
        ),
        DecisionReason::LifeEvent { kind } => {
            c.fmt_key(l, "ui.reason.LifeEvent", &[("co", &life_event(c, l, kind))])
        }
        DecisionReason::ModeChosen { mode, minutes: m } => c.fmt_key(
            l,
            "ui.reason.ModeChosen",
            &[
                ("co", &transport_mode(c, l, mode)),
                ("czas", &minutes(c, l, m)),
            ],
        ),
        DecisionReason::NoRouteForMode { mode, fallback } => c.fmt_key(
            l,
            "ui.reason.NoRouteForMode",
            &[
                ("co", &transport_mode(c, l, mode)),
                ("zamiast", &transport_mode(c, l, fallback)),
            ],
        ),
        DecisionReason::RefuelNeeded { level_permille } => c.fmt_key(
            l,
            "ui.reason.RefuelNeeded",
            &[("promile", &level_permille.to_string())],
        ),
        DecisionReason::StationChosen {
            detour_min,
            price_gr_per_l,
        } => c.fmt_key(
            l,
            "ui.reason.StationChosen",
            &[
                ("objazd", &minutes(c, l, detour_min)),
                ("cena", &crate::zlotowki(Money(i64::from(price_gr_per_l)))),
            ],
        ),
        DecisionReason::TripDelayed {
            planned_min,
            actual_min,
        } => c.fmt_key(
            l,
            "ui.reason.TripDelayed",
            &[
                ("faktycznie", &minutes(c, l, actual_min)),
                ("plan", &minutes(c, l, planned_min)),
            ],
        ),
        DecisionReason::ModeCompared {
            chosen,
            runner_up,
            delta_gr,
        } => c.fmt_key(
            l,
            "ui.reason.ModeCompared",
            &[
                ("srodek", &transport_mode(c, l, chosen)),
                ("drugi", &transport_mode(c, l, runner_up)),
                ("roznica", &crate::zlotowki(Money(i64::from(delta_gr)))),
            ],
        ),
        DecisionReason::NoParkingAtDestination { lots_searched } => c.fmt_key(
            l,
            "ui.reason.NoParkingAtDestination",
            &[("ile", &lots_searched.to_string())],
        ),
        DecisionReason::LeftBehind { line, waited_min } => c.fmt_key(
            l,
            "ui.reason.LeftBehind",
            &[
                ("linia", &line.to_string()),
                ("czekal", &minutes(c, l, waited_min)),
            ],
        ),
        DecisionReason::ShopChosen {
            site: _,
            dominant,
            delta_bp,
        } => c.fmt_key(
            l,
            "ui.reason.ShopChosen",
            &[
                ("czlon", &utility_term(c, l, dominant)),
                ("roznica", &procent_bp(i32::from(delta_bp))),
            ],
        ),
        DecisionReason::OfferRejected {
            site: _,
            cause,
            detail,
        } => c.fmt_key(
            l,
            "ui.reason.OfferRejected",
            &[
                ("powod", &reject_cause(c, l, cause)),
                ("szczegol", &detail.to_string()),
            ],
        ),
        DecisionReason::PurchaseDeferred {
            need: n,
            cause,
            gap_permille,
        } => c.fmt_key(
            l,
            "ui.reason.PurchaseDeferred",
            &[
                ("co", &need(c, l, n)),
                ("powod", &reject_cause(c, l, cause)),
                (
                    "brakowalo",
                    &format!("{},{:03}", gap_permille / 1000, (gap_permille % 1000).abs()),
                ),
            ],
        ),
        DecisionReason::Repricing {
            site: _,
            good: _,
            driver,
            delta_bp,
        } => c.fmt_key(
            l,
            "ui.reason.Repricing",
            &[
                ("czlon", &price_driver(c, l, driver)),
                ("roznica", &procent_bp(i32::from(delta_bp))),
            ],
        ),
        DecisionReason::CreditApproved {
            kind,
            rate_bp,
            load_bp,
        } => c.fmt_key(
            l,
            "ui.reason.CreditApproved",
            &[
                ("produkt", &loan_kind(c, l, kind)),
                ("oprocentowanie", &procent_bp(i32::from(rate_bp))),
                ("obciazenie", &procent_bp(i32::from(load_bp))),
            ],
        ),
        DecisionReason::CreditRejected {
            kind,
            cause,
            margin_bp,
        } => c.fmt_key(
            l,
            "ui.reason.CreditRejected",
            &[
                ("produkt", &loan_kind(c, l, kind)),
                ("powod", &reject_credit(c, l, cause)),
                ("roznica", &procent_bp(i32::from(margin_bp))),
            ],
        ),
        DecisionReason::BudgetShortfall { cost, gap_permille } => c.fmt_key(
            l,
            "ui.reason.BudgetShortfall",
            &[
                ("pozycja", &fixed_cost(c, l, cost)),
                ("brakowalo", &procent_bp(i32::from(gap_permille) * 10)),
            ],
        ),
        // ── M6: łańcuch dostaw ───────────────────────────────────────────────────
        // Towar jest w ładunku, ale nie w zdaniu: `GoodId` rozwiązuje katalog z
        // `sim/supply`, a `engine/ui` od niego nie zależy i zależeć nie ma (`AD-3`).
        // Nazwę towaru dokłada panel, który katalog trzyma — tak samo jak przy
        // `Repricing` od M5c.
        DecisionReason::Shortage {
            good: _,
            from,
            to,
            coverage_minutes,
        } => c.fmt_key(
            l,
            "ui.reason.Shortage",
            &[
                ("z", &shortage_stage(c, l, from)),
                ("na", &shortage_stage(c, l, to)),
                ("pokrycie", &godziny(coverage_minutes)),
            ],
        ),
        DecisionReason::ProductionHalted {
            site: _,
            line,
            cause,
        } => c.fmt_key(
            l,
            "ui.reason.ProductionHalted",
            &[
                ("linia", &(u32::from(line) + 1).to_string()),
                ("powod", &line_stop_cause(c, l, cause)),
            ],
        ),
        DecisionReason::SubstituteUsed {
            good: _,
            alt: _,
            quality_loss,
        } => c.fmt_key(
            l,
            "ui.reason.SubstituteUsed",
            &[("strata", &quality_loss.to_string())],
        ),
        DecisionReason::SupplierChosen {
            good: _,
            seller: _,
            quotes,
            saving_bp,
        } => c.fmt_key(
            l,
            "ui.reason.SupplierChosen",
            &[
                ("ofert", &quotes.to_string()),
                ("przewaga", &procent_bp(i32::from(saving_bp))),
            ],
        ),
        DecisionReason::ContractSigned {
            good: _,
            seller: _,
            months,
            indexed,
        } => c.fmt_key(
            l,
            "ui.reason.ContractSigned",
            &[
                ("miesiecy", &months.to_string()),
                ("cennik", &contract_pricing(c, l, indexed)),
            ],
        ),
        DecisionReason::ExportChosen {
            good: _,
            premium_bp,
            mass_kg,
        } => c.fmt_key(
            l,
            "ui.reason.ExportChosen",
            &[
                ("przewaga", &procent_bp(i32::from(premium_bp))),
                ("masa", &mass_kg.to_string()),
            ],
        ),
        DecisionReason::Hired {
            role: _,
            score,
            runner_up,
        } => c.fmt_key(
            l,
            "ui.reason.Hired",
            &[
                ("wynik", &score.to_string()),
                ("drugi", &drugi_w_kolejce(c, l, runner_up)),
            ],
        ),
        // Podwyżka przycięta do zera jest **innym zdaniem**, a nie tym samym z liczbą
        // zero: firma nie podniosła stawki, bo przy wyższej ten etat przestaje się
        // opłacać. Gracz pytający „dlaczego wakat stoi pusty" dostaje tu odpowiedź.
        DecisionReason::WageRaise {
            role: _,
            delta_bp: 0,
            days_open,
            cause: _,
        } => c.fmt_key(
            l,
            "ui.reason.WageFrozen",
            &[(
                "dni",
                &c.plural(l, c.must("ui.unit.days"), u64::from(days_open)),
            )],
        ),
        DecisionReason::WageRaise {
            role: _,
            delta_bp,
            days_open,
            cause,
        } => c.fmt_key(
            l,
            "ui.reason.WageRaise",
            &[
                ("przyrost", &procent_bp(i32::from(delta_bp))),
                (
                    "dni",
                    &c.plural(l, c.must("ui.unit.days"), u64::from(days_open)),
                ),
                ("powod", &wage_cause(c, l, cause)),
            ],
        ),
        DecisionReason::JobLeft {
            role: _,
            cause,
            tenure_days,
        } => c.fmt_key(
            l,
            "ui.reason.JobLeft",
            &[
                ("powod", &leave_cause(c, l, cause)),
                (
                    "staz",
                    &c.plural(l, c.must("ui.unit.days"), u64::from(tenure_days)),
                ),
            ],
        ),
        // Reguła zapasowa („INACZEJ" z gramatyki M9d) jest **innym zdaniem**, a nie
        // regułą numer 255: gracz pytający „która reguła to zrobiła" ma usłyszeć,
        // że nie pasowała żadna, a nie zobaczyć numer, którego nie ma na liście.
        DecisionReason::PolicyApplied {
            policy,
            rule: u8::MAX,
            action,
        } => c.fmt_key(
            l,
            "ui.reason.PolicyFallback",
            &[
                ("polityka", &policy.get().to_string()),
                ("akcja", &action_kind(c, l, action)),
            ],
        ),
        DecisionReason::PolicyApplied {
            policy,
            rule,
            action,
        } => c.fmt_key(
            l,
            "ui.reason.PolicyApplied",
            &[
                ("polityka", &policy.get().to_string()),
                // Reguły numeruje się dla gracza od jedynki — w edytorze M9 są
                // wierszami listy, a pierwszy wiersz nie jest wierszem zerowym.
                ("regula", &(u16::from(rule) + 1).to_string()),
                ("akcja", &action_kind(c, l, action)),
            ],
        ),
        DecisionReason::ManagerAssigned {
            site: _,
            skill_mgmt,
            prev,
        } => c.fmt_key(
            l,
            "ui.reason.ManagerAssigned",
            &[
                ("umiejetnosc", &skill_mgmt.get().to_string()),
                ("poprzednia", &prev.to_string()),
            ],
        ),
        DecisionReason::LoanTaken {
            kind,
            rate_bp,
            term_months,
        } => c.fmt_key(
            l,
            "ui.reason.LoanTaken",
            &[
                ("produkt", &loan_kind(c, l, kind)),
                ("oprocentowanie", &procent_bp(i32::from(rate_bp))),
                ("okres", &months(c, l, u32::from(term_months))),
            ],
        ),
        DecisionReason::LeaseSigned { site: _, months: m } => c.fmt_key(
            l,
            "ui.reason.LeaseSigned",
            &[("okres", &months(c, l, u32::from(m)))],
        ),
        DecisionReason::ReceivablesFactored { count, discount_bp } => c.fmt_key(
            l,
            "ui.reason.ReceivablesFactored",
            &[
                ("ile", &count.to_string()),
                ("dyskonto", &procent_bp(i32::from(discount_bp))),
            ],
        ),
        DecisionReason::BondIssued {
            coupon_bp,
            months: m,
        } => c.fmt_key(
            l,
            "ui.reason.BondIssued",
            &[
                ("kupon", &procent_bp(i32::from(coupon_bp))),
                ("okres", &months(c, l, u32::from(m))),
            ],
        ),
        // Niewypłacalność mierzona czasem czyta się inaczej niż ta mierzona bilansem:
        // „nie płaci od trzech miesięcy" i „ma więcej długów niż majątku" to dwa
        // różne zdania o firmie, a gracz zadaje o nie dwa różne pytania.
        DecisionReason::BankruptcyOpened {
            trigger: BankruptcyTrigger::Illiquid,
            days: d,
        } => c.fmt_key(
            l,
            "ui.reason.BankruptcyIlliquid",
            &[("dni", &days(c, l, u32::from(d)))],
        ),
        DecisionReason::BankruptcyOpened { trigger, days: _ } => c.fmt_key(
            l,
            "ui.reason.BankruptcyOpened",
            &[("powod", &bankruptcy_trigger(c, l, trigger))],
        ),
        DecisionReason::ClaimSettled { priority, ratio_bp } => c.fmt_key(
            l,
            "ui.reason.ClaimSettled",
            &[
                ("grupa", &claim_priority(c, l, priority)),
                ("stopien", &procent_bp(i32::from(ratio_bp))),
            ],
        ),
        // ── M7e: AI firm ─────────────────────────────────────────────────────────
        // Towar znowu jest w ładunku, a nie w zdaniu — z tego samego powodu co w M6:
        // `GoodId` rozwiązuje katalog z `sim/supply`, którego `engine/ui` nie widzi.
        DecisionReason::MarginTargetSet {
            good: _,
            margin_bp,
            prev_bp,
        } => c.fmt_key(
            l,
            "ui.reason.MarginTargetSet",
            &[
                ("cel", &procent_bp(margin_bp)),
                ("poprzednio", &procent_bp(prev_bp)),
            ],
        ),
        DecisionReason::RestockTargetSet {
            good: _,
            days: d,
            prev,
        } => c.fmt_key(
            l,
            "ui.reason.RestockTargetSet",
            &[
                ("cel", &days(c, l, u32::from(d))),
                ("poprzednio", &days(c, l, u32::from(prev))),
            ],
        ),
        DecisionReason::SiteClosed {
            months: m,
            margin_bp,
        } => c.fmt_key(
            l,
            "ui.reason.SiteClosed",
            &[
                ("okres", &months(c, l, u32::from(m))),
                ("marza", &procent_bp(margin_bp)),
            ],
        ),
        DecisionReason::StrategySet { strategy, prev } => c.fmt_key(
            l,
            "ui.reason.StrategySet",
            &[
                ("kurs", &firm_strategy(c, l, strategy)),
                ("poprzednio", &firm_strategy(c, l, prev)),
            ],
        ),
        DecisionReason::CompetitiveResponse {
            kind,
            target: _,
            depth_bp,
        } => c.fmt_key(
            l,
            "ui.reason.CompetitiveResponse",
            &[
                ("odpowiedz", &reaction_kind(c, l, kind)),
                ("koszt", &procent_bp(i32::from(depth_bp))),
            ],
        ),
        // **Tu nie ma kwoty i nie może jej być** (§5.10, `R15`). Decyzja stoi na
        // porównaniu wariantów w modelu makro, a ten deklaruje własny błąd 3–12 %.
        // Gracz dostaje więc to, co model faktycznie wie: z ilu wariantów rywal
        // wybierał, jak szeroki był margines i w którą stronę szedł wynik.
        DecisionReason::SiteOpened {
            district: _,
            variants,
            margin_bp,
            trend: kierunek,
        } => c.fmt_key(
            l,
            "ui.reason.SiteOpened",
            &[
                ("warianty", &u32::from(variants).to_string()),
                ("margines", &procent_bp(i32::from(margin_bp))),
                ("kierunek", &trend(c, l, kierunek)),
            ],
        ),
        DecisionReason::VoluntaryClosure { months: m, cash } => c.fmt_key(
            l,
            "ui.reason.VoluntaryClosure",
            &[
                ("okres", &months(c, l, u32::from(m))),
                ("kasa", &crate::zlotowki(cash)),
            ],
        ),
        DecisionReason::FirmFounded { score, capital } => c.fmt_key(
            l,
            "ui.reason.FirmFounded",
            &[
                ("ocena", &u32::from(score).to_string()),
                ("kapital", &crate::zlotowki(capital)),
            ],
        ),
        DecisionReason::ChainEntered { capital, sites } => c.fmt_key(
            l,
            "ui.reason.ChainEntered",
            &[
                ("kapital", &crate::zlotowki(capital)),
                ("zaklady", &u32::from(sites).to_string()),
            ],
        ),
        DecisionReason::TaxAssessed {
            kind,
            rate_bp,
            amount,
        } => c.fmt_key(
            l,
            "ui.reason.TaxAssessed",
            &[
                ("danina", &tax_kind(c, l, kind)),
                ("kwota", &crate::zlotowki(amount)),
                ("stawka", &procent(u32::from(rate_bp))),
            ],
        ),
        DecisionReason::TaxSettled { kind, amount } => c.fmt_key(
            l,
            "ui.reason.TaxSettled",
            &[
                ("danina", &tax_kind(c, l, kind)),
                ("kwota", &crate::zlotowki(amount)),
            ],
        ),
        DecisionReason::TaxOverdue {
            kind,
            days: dni,
            amount,
        } => c.fmt_key(
            l,
            "ui.reason.TaxOverdue",
            &[
                ("danina", &tax_kind(c, l, kind)),
                ("kwota", &crate::zlotowki(amount)),
                ("dni", &days(c, l, u32::from(dni))),
            ],
        ),
        DecisionReason::TaxAbated { kind, why, amount } => c.fmt_key(
            l,
            "ui.reason.TaxAbated",
            &[
                ("danina", &tax_kind(c, l, kind)),
                ("kwota", &crate::zlotowki(amount)),
                ("powod", &abate_reason(c, l, why)),
            ],
        ),
        DecisionReason::PublicSpend { category, amount } => c.fmt_key(
            l,
            "ui.reason.PublicSpend",
            &[
                ("kierunek", &spend_category(c, l, category)),
                ("kwota", &crate::zlotowki(amount)),
            ],
        ),
        DecisionReason::MunicipalBondIssued {
            coupon_bp,
            principal,
        } => c.fmt_key(
            l,
            "ui.reason.MunicipalBondIssued",
            &[
                ("kwota", &crate::zlotowki(principal)),
                ("kupon", &procent(u32::from(coupon_bp))),
            ],
        ),
        DecisionReason::BudgetDeficitClosed { gap, cut_bp } => c.fmt_key(
            l,
            "ui.reason.BudgetDeficitClosed",
            &[
                ("luka", &crate::zlotowki(gap)),
                ("ciecie", &procent(u32::from(cut_bp))),
            ],
        ),
        DecisionReason::LoadShed {
            service,
            priority,
            shortfall_w,
        } => c.fmt_key(
            l,
            "ui.reason.LoadShed",
            &[
                ("medium", &utility_service(c, l, service)),
                ("priorytet", &priority.to_string()),
                ("moc", &kilowaty(shortfall_w)),
            ],
        ),
        DecisionReason::GridTripped {
            service,
            repair_minutes,
        } => c.fmt_key(
            l,
            "ui.reason.GridTripped",
            &[
                ("medium", &utility_service(c, l, service)),
                ("czas", &minutes(c, l, repair_minutes)),
            ],
        ),
        DecisionReason::EventStarted {
            event,
            category,
            severity_bps,
        } => c.fmt_key(
            l,
            "ui.reason.EventStarted",
            &[
                ("kategoria", &event_category(c, l, category)),
                ("sila", &procent(u32::from(severity_bps))),
                ("numer", &event.0.to_string()),
            ],
        ),
        DecisionReason::EventEnded {
            event,
            category,
            days,
        } => c.fmt_key(
            l,
            "ui.reason.EventEnded",
            &[
                ("kategoria", &event_category(c, l, category)),
                ("dni", &days_txt(c, l, u32::from(days))),
                ("numer", &event.0.to_string()),
            ],
        ),
        DecisionReason::ServiceQuality {
            kind,
            district,
            quality,
            funding_bp,
            staff_bp,
            load_bp,
        } => c.fmt_key(
            l,
            "ui.reason.ServiceQuality",
            &[
                ("usluga", &service_kind(c, l, kind)),
                ("dzielnica", &district.0.to_string()),
                ("jakosc", &quality.get().to_string()),
                ("pieniadze", &procent(u32::from(funding_bp))),
                ("obsada", &procent(u32::from(staff_bp))),
                ("obciazenie", &procent(u32::from(load_bp))),
            ],
        ),
        DecisionReason::PermitIssued { kind, waited_days } => c.fmt_key(
            l,
            "ui.reason.PermitIssued",
            &[
                ("pozwolenie", &permit_kind(c, l, kind)),
                ("dni", &days_txt(c, l, u32::from(waited_days))),
            ],
        ),
        DecisionReason::CaseOpened { agency, evidence } => c.fmt_key(
            l,
            "ui.reason.CaseOpened",
            &[
                ("urzad", &agency_kind(c, l, agency)),
                ("dowody", &evidence.get().to_string()),
            ],
        ),
        // Dwa klucze, bo to są dwa różne zdania: kara z kwotą i kara bez kwoty.
        // Środek, którego dolegliwością jest czas albo majątek, ma kwotę zerową —
        // wpisanie tam „0 zł" mówiłoby graczowi, że nic go to nie kosztowało.
        DecisionReason::RemedyImposed {
            agency,
            remedy,
            amount,
        } => {
            if amount.get() > 0 {
                c.fmt_key(
                    l,
                    "ui.reason.RemedyImposed",
                    &[
                        ("urzad", &agency_kind(c, l, agency)),
                        ("srodek", &remedy_kind(c, l, remedy)),
                        ("kwota", &crate::zlotowki(amount)),
                    ],
                )
            } else {
                c.fmt_key(
                    l,
                    "ui.reason.RemedyImposedNoAmount",
                    &[
                        ("urzad", &agency_kind(c, l, agency)),
                        ("srodek", &remedy_kind(c, l, remedy)),
                    ],
                )
            }
        }
        DecisionReason::ShadowShareSet {
            share_bp,
            last_result,
        } => c.fmt_key(
            l,
            "ui.reason.ShadowShareSet",
            &[
                ("udzial", &procent(u32::from(share_bp))),
                ("wynik", &crate::zlotowki(last_result)),
            ],
        ),
    }
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
            DecisionReason::Commitment {
                kind: CommitmentKind::Work,
            },
            DecisionReason::NeedCritical {
                need: NeedKind::Sleep,
                level: Q::new(12),
            },
            DecisionReason::StockBelowThreshold {
                cat: StockCat::Food,
                days_left: 1,
            },
            DecisionReason::FreeTimePreference {
                trait_id: TraitId::Sociability,
                weight: 7,
            },
            DecisionReason::NoTimeWindow {
                need: NeedKind::Health,
                needed_min: 45,
                longest_gap_min: 20,
            },
            DecisionReason::SlotBudgetExhausted {
                dropped: NeedKind::Clothing,
            },
            DecisionReason::ChosenNearest {
                travel_min: 8,
                runner_up_min: 14,
            },
            DecisionReason::ChosenOnRoute {
                detour_min: 3,
                direct_min: 9,
            },
            DecisionReason::PlaceUnknown {
                need: NeedKind::Hunger,
                known_count: 0,
            },
            DecisionReason::PlaceClosed {
                place: PlaceRef::default(),
                opens_at: MinuteOfDay::new(7 * 60),
            },
            DecisionReason::Arrived {
                planned: MinuteOfDay::new(480),
                actual: MinuteOfDay::new(482),
            },
            DecisionReason::Replanned {
                cause_tag: 2,
                slots_changed: 3,
            },
            DecisionReason::Deprivation {
                need: NeedKind::Sleep,
                effect: DeprivationEffect::AbsenceRisk,
            },
            DecisionReason::ModeWalkOnly { minutes: 3 },
            DecisionReason::NeedSatisfied {
                need: NeedKind::Hunger,
                gain: Q::new(40),
            },
            DecisionReason::PartnerChosen {
                compatibility: Q::new(80),
                candidates: 4,
            },
            DecisionReason::SeparationFiled {
                stress: Q::new(70),
                years_together: 12,
            },
            DecisionReason::MigrationDecision {
                kind: MigrationKind::Arrived,
                months_jobless: 0,
            },
            DecisionReason::Inheritance {
                permille: 500,
                heirs: 2,
            },
            DecisionReason::LifeEvent {
                kind: LifeEventKind::Died,
            },
            DecisionReason::ModeChosen {
                mode: TransportMode::Bus,
                minutes: 24,
            },
            DecisionReason::NoRouteForMode {
                mode: TransportMode::Car,
                fallback: TransportMode::Walk,
            },
            DecisionReason::RefuelNeeded { level_permille: 80 },
            DecisionReason::StationChosen {
                detour_min: 4,
                price_gr_per_l: 649,
            },
            DecisionReason::TripDelayed {
                planned_min: 18,
                actual_min: 31,
            },
            DecisionReason::ModeCompared {
                chosen: TransportMode::Bus,
                runner_up: TransportMode::Car,
                delta_gr: -320,
            },
            DecisionReason::NoParkingAtDestination { lots_searched: 4 },
            DecisionReason::LeftBehind {
                line: 12,
                waited_min: 9,
            },
            DecisionReason::ShopChosen {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                dominant: UtilityKind::Price,
                delta_bp: -1_200,
            },
            DecisionReason::OfferRejected {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                cause: RejectCause::OutOfStock,
                detail: 0,
            },
            DecisionReason::PurchaseDeferred {
                need: NeedKind::Hunger,
                cause: RejectCause::BelowThreshold,
                gap_permille: -140,
            },
            DecisionReason::Repricing {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                good: magnat_core::GoodId(3),
                driver: PriceDriver::Stock,
                delta_bp: -450,
            },
            DecisionReason::CreditApproved {
                kind: LoanKind::Consumer,
                rate_bp: 1_200,
                load_bp: 2_800,
            },
            DecisionReason::CreditRejected {
                kind: LoanKind::WorkingCapital,
                cause: RejectCredit::DscrTooLow,
                margin_bp: -900,
            },
            DecisionReason::BudgetShortfall {
                cost: FixedCost::Housing,
                gap_permille: 420,
            },
            DecisionReason::Shortage {
                good: magnat_core::GoodId(3),
                from: ShortageStageKind::Buffer,
                to: ShortageStageKind::Throttled,
                coverage_minutes: 192,
            },
            DecisionReason::ProductionHalted {
                site: magnat_core::SiteId(magnat_core::Entity::new(9, std::num::NonZeroU32::MIN)),
                line: 1,
                cause: LineStopCause::NoPower,
            },
            DecisionReason::SubstituteUsed {
                good: magnat_core::GoodId(3),
                alt: magnat_core::GoodId(4),
                quality_loss: 8,
            },
            DecisionReason::SupplierChosen {
                good: magnat_core::GoodId(3),
                seller: magnat_core::FirmId(magnat_core::Entity::new(4, std::num::NonZeroU32::MIN)),
                quotes: 5,
                saving_bp: 320,
            },
            DecisionReason::ContractSigned {
                good: magnat_core::GoodId(3),
                seller: magnat_core::FirmId(magnat_core::Entity::new(4, std::num::NonZeroU32::MIN)),
                months: 12,
                indexed: true,
            },
            // Drugi wariant cennika ma własny klucz, więc bez tego wpisu test
            // „każdy powód ma tekst w obu językach" nie dotknąłby `...pricing.fixed`.
            DecisionReason::ContractSigned {
                good: magnat_core::GoodId(3),
                seller: magnat_core::FirmId(magnat_core::Entity::new(4, std::num::NonZeroU32::MIN)),
                months: 6,
                indexed: false,
            },
            DecisionReason::ExportChosen {
                good: magnat_core::GoodId(3),
                premium_bp: 1_450,
                mass_kg: 24_000,
            },
            DecisionReason::Hired {
                role: magnat_core::JobRoleId(4),
                score: 780,
                runner_up: 640,
            },
            // Jedyny chętny ma własny klucz, tak samo jak drugi wariant cennika wyżej.
            DecisionReason::Hired {
                role: magnat_core::JobRoleId(4),
                score: 780,
                runner_up: i32::MIN,
            },
            DecisionReason::WageRaise {
                role: magnat_core::JobRoleId(4),
                delta_bp: 930,
                days_open: 14,
                cause: WageCause::NoCandidates,
            },
            // Krok przycięty do sufitu marży — drugie zdanie, nie ta sama liczba.
            DecisionReason::WageRaise {
                role: magnat_core::JobRoleId(4),
                delta_bp: 0,
                days_open: 21,
                cause: WageCause::Ceiling,
            },
            DecisionReason::JobLeft {
                role: magnat_core::JobRoleId(4),
                cause: LeaveCause::BetterOffer,
                tenure_days: 420,
            },
            DecisionReason::PolicyApplied {
                policy: magnat_core::PolicyId(3),
                rule: 0,
                action: ActionKind::SetPrice,
            },
            // Reguła zapasowa wybiera **inny klucz** lokalizacji, więc bez drugiego
            // wpisu połowa ramienia zostałaby niesprawdzona — ten sam powód, dla
            // którego `Hired` i `WageRaise` stoją na tej liście po dwa razy.
            DecisionReason::PolicyApplied {
                policy: magnat_core::PolicyId(3),
                rule: u8::MAX,
                action: ActionKind::SetMargin,
            },
            DecisionReason::ManagerAssigned {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                skill_mgmt: Q::new(71),
                prev: 50,
            },
            DecisionReason::LoanTaken {
                kind: LoanKind::Investment,
                rate_bp: 740,
                term_months: 60,
            },
            DecisionReason::LeaseSigned {
                site: magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN)),
                months: 36,
            },
            DecisionReason::ReceivablesFactored {
                count: 4,
                discount_bp: 400,
            },
            DecisionReason::BondIssued {
                coupon_bp: 1100,
                months: 36,
            },
            // Upadłość z braku płynności wybiera **inny klucz** niż upadłość
            // bilansowa — ten sam powód, dla którego `PolicyApplied` stoi tu dwa razy.
            DecisionReason::BankruptcyOpened {
                trigger: BankruptcyTrigger::Illiquid,
                days: 92,
            },
            DecisionReason::BankruptcyOpened {
                trigger: BankruptcyTrigger::NegativeEquity,
                days: 0,
            },
            DecisionReason::ClaimSettled {
                priority: ClaimPriority::Wages,
                ratio_bp: 10_000,
            },
            DecisionReason::MarginTargetSet {
                good: magnat_core::GoodId(3),
                margin_bp: 1_800,
                prev_bp: 2_500,
            },
            DecisionReason::RestockTargetSet {
                good: magnat_core::GoodId(3),
                days: 10,
                prev: 6,
            },
            DecisionReason::SiteClosed {
                months: 3,
                margin_bp: -820,
            },
            DecisionReason::StrategySet {
                strategy: FirmStrategy::Discount,
                prev: FirmStrategy::Cautious,
            },
            DecisionReason::CompetitiveResponse {
                kind: ReactionKind::PriceWar,
                target: magnat_core::SiteId(magnat_core::Entity::new(
                    11,
                    std::num::NonZeroU32::MIN,
                )),
                depth_bp: 1_200,
            },
            DecisionReason::SiteOpened {
                district: magnat_core::DistrictId(3),
                variants: 4,
                margin_bp: 620,
                trend: magnat_core::Trend::Up,
            },
            DecisionReason::VoluntaryClosure {
                months: 7,
                cash: Money(12_400),
            },
            DecisionReason::FirmFounded {
                score: 74,
                capital: Money(1_800_000),
            },
            DecisionReason::ChainEntered {
                capital: Money(54_000_000),
                sites: 3,
            },
            DecisionReason::TaxAssessed {
                kind: magnat_core::TaxKind::Vat,
                rate_bp: 2300,
                amount: Money(412_900),
            },
            DecisionReason::TaxSettled {
                kind: magnat_core::TaxKind::Cit,
                amount: Money(1_140_000),
            },
            DecisionReason::TaxOverdue {
                kind: magnat_core::TaxKind::Property,
                days: 41,
                amount: Money(19_200),
            },
            DecisionReason::TaxAbated {
                kind: magnat_core::TaxKind::Pit,
                why: magnat_core::AbateReason::TimeBarred,
                amount: Money(8_400),
            },
            DecisionReason::PublicSpend {
                category: magnat_core::SpendCategory::Education,
                amount: Money(22_000_000),
            },
            DecisionReason::MunicipalBondIssued {
                coupon_bp: 650,
                principal: Money(80_000_000),
            },
            DecisionReason::BudgetDeficitClosed {
                gap: Money(4_500_000),
                cut_bp: 1250,
            },
            DecisionReason::LoadShed {
                service: magnat_core::UtilityService::Electricity,
                priority: 3,
                shortfall_w: 1_450_000,
            },
            DecisionReason::GridTripped {
                service: magnat_core::UtilityService::Electricity,
                repair_minutes: 195,
            },
            DecisionReason::EventStarted {
                event: magnat_core::EventId(7),
                category: magnat_core::EventCategory::Natural,
                severity_bps: 4200,
            },
            DecisionReason::EventEnded {
                event: magnat_core::EventId(7),
                category: magnat_core::EventCategory::Natural,
                days: 23,
            },
            DecisionReason::ServiceQuality {
                kind: magnat_core::ServiceKind::School,
                district: magnat_core::DistrictId(3),
                quality: Q::new(62),
                funding_bp: 7400,
                staff_bp: 8800,
                load_bp: 11_200,
            },
            DecisionReason::PermitIssued {
                kind: magnat_core::PermitKind::Build,
                waited_days: 31,
            },
            DecisionReason::CaseOpened {
                agency: magnat_core::AgencyKind::TaxOffice,
                evidence: Q::new(20),
            },
            DecisionReason::RemedyImposed {
                agency: magnat_core::AgencyKind::TaxOffice,
                remedy: magnat_core::RemedyKind::BackTax,
                amount: Money(1_240_000),
            },
            // Ósmy wpis dwukrotny: środek bez kwoty wybiera inny klucz.
            DecisionReason::RemedyImposed {
                agency: magnat_core::AgencyKind::Sanitary,
                remedy: magnat_core::RemedyKind::Closure,
                amount: Money::ZERO,
            },
            DecisionReason::ShadowShareSet {
                share_bp: 1800,
                last_result: Money(-420_000),
            },
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
        assert_eq!(wszystkie().len(), 84);
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
        }
    }

    #[test]
    fn zegar_zawija_dobe() {
        assert_eq!(zegar(0), "00:00");
        assert_eq!(zegar(8 * 60 + 5), "08:05");
        assert_eq!(zegar(1439), "23:59");
    }
}
