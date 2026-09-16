//! `DecisionReason` → tekst. **Jedno miejsce w całej grze** (M3 §6.1).
//!
//! `match` jest wyczerpujący i bez ramienia `_` — to jest cały mechanizm z 00 §K-12:
//! faza, która dopisze wariant, **nie skompiluje interfejsu**, dopóki nie napisze,
//! jak ten powód pokazać graczowi. Test `kazdy_powod_ma_tekst` sprawdza to od drugiej
//! strony: każdy wariant musi dać niepusty napis w obu językach.

use crate::loc::{Catalog, Locale};
use magnat_agents::SocialClass;
use magnat_core::{
    ActivityKind, CommitmentKind, DecisionReason, DeprivationEffect, FixedCost, LifeEventKind,
    LineStopCause, LoanKind, MigrationKind, Money, NeedKind, PriceDriver, RejectCause,
    RejectCredit, ShortageStageKind, StockCat, TraitId, TransportMode, UtilityKind,
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
        assert_eq!(wszystkie().len(), 43);
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
            for f in FixedCost::ALL {
                assert!(!fixed_cost(&c, l, *f).is_empty());
            }
            for r in RejectCause::ALL {
                assert!(!reject_cause(&c, l, *r).is_empty());
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
