//! Powody decyzji mieszkańca w języku gracza (`R2-WP20`).
//!
//! Trzydzieści dziewięć powodów: doba, potrzeby, podróż, zakupy, dom i rodzina.
//!
//! Osobny plik, bo `describe` rosła liniowo z liczbą faz i miała 1290 linii przy
//! progu błędu kontroli strukturalnej wynoszącym 250. Po podziale dopisanie powodu
//! przez fazę dotykającą firm nie powiększa pliku, który obsługuje mieszkańców.
//!
//! **Wyczerpujący `match` bez ramienia `_`** — `K-12` obowiązuje tutaj tak samo jak
//! przed podziałem, tylko na jednym aktorze zamiast na wszystkich naraz. Wariant bez
//! zdania łamie kompilację; pilnuje tego dodatkowo `tests/ui/citizen_reason_bez_wildcard.rs`.

use super::*;

#[must_use]
#[allow(clippy::too_many_lines)]
pub(super) fn opisz(c: &Catalog, l: Locale, r: CitizenReason, _n: Names<'_>) -> String {
    match r {
        CitizenReason::Commitment { kind } => c.fmt_key(
            l,
            "ui.reason.Commitment",
            &[("co", &commitment(c, l, kind))],
        ),
        CitizenReason::NeedCritical { need: n, level } => c.fmt_key(
            l,
            "ui.reason.NeedCritical",
            &[("co", &need(c, l, n)), ("poziom", &level.get().to_string())],
        ),
        CitizenReason::StockBelowThreshold { cat, days_left } => c.fmt_key(
            l,
            "ui.reason.StockBelowThreshold",
            &[
                ("co", &stock(c, l, cat)),
                ("dni", &days(c, l, u32::from(days_left))),
            ],
        ),
        CitizenReason::FreeTimePreference { trait_id, weight } => c.fmt_key(
            l,
            "ui.reason.FreeTimePreference",
            &[
                ("co", &trait_name(c, l, trait_id)),
                ("waga", &weight.to_string()),
            ],
        ),
        CitizenReason::NoTimeWindow {
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
        CitizenReason::SlotBudgetExhausted { dropped } => c.fmt_key(
            l,
            "ui.reason.SlotBudgetExhausted",
            &[("co", &need(c, l, dropped))],
        ),
        CitizenReason::ChosenNearest {
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
        CitizenReason::ChosenOnRoute {
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
        CitizenReason::PlaceUnknown {
            need: n,
            known_count,
        } => c.fmt_key(
            l,
            "ui.reason.PlaceUnknown",
            &[("co", &need(c, l, n)), ("ile", &known_count.to_string())],
        ),
        CitizenReason::PlaceClosed { place: _, opens_at } => c.fmt_key(
            l,
            "ui.reason.PlaceClosed",
            &[("otwarcie", &zegar(opens_at.get()))],
        ),
        CitizenReason::Arrived { planned, actual } => c.fmt_key(
            l,
            "ui.reason.Arrived",
            &[
                ("faktycznie", &zegar(actual.get())),
                ("plan", &zegar(planned.get())),
            ],
        ),
        CitizenReason::Replanned {
            cause_tag: _,
            slots_changed,
        } => c.fmt_key(
            l,
            "ui.reason.Replanned",
            &[("ile", &slots_changed.to_string())],
        ),
        CitizenReason::Deprivation { need: n, effect } => c.fmt_key(
            l,
            "ui.reason.Deprivation",
            &[
                ("co", &need(c, l, n)),
                ("skutek", &deprivation(c, l, effect)),
            ],
        ),
        CitizenReason::ModeWalkOnly { minutes: m } => {
            c.fmt_key(l, "ui.reason.ModeWalkOnly", &[("czas", &minutes(c, l, m))])
        }
        CitizenReason::NeedSatisfied { need: n, gain } => c.fmt_key(
            l,
            "ui.reason.NeedSatisfied",
            &[("co", &need(c, l, n)), ("zysk", &gain.get().to_string())],
        ),
        CitizenReason::PartnerChosen {
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
        CitizenReason::SeparationFiled {
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
        CitizenReason::MigrationDecision {
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
        CitizenReason::Inheritance { permille, heirs } => c.fmt_key(
            l,
            "ui.reason.Inheritance",
            &[
                ("promile", &permille.to_string()),
                ("ilu", &heirs.to_string()),
            ],
        ),
        CitizenReason::LifeEvent { kind } => {
            c.fmt_key(l, "ui.reason.LifeEvent", &[("co", &life_event(c, l, kind))])
        }
        CitizenReason::EscortUnavailable { count } => c.fmt_key(
            l,
            "ui.reason.EscortUnavailable",
            &[("ile", &count.to_string())],
        ),
        CitizenReason::GuardianAppointed { wards, weight, kin } => c.fmt_key(
            l,
            if kin {
                "ui.reason.GuardianAppointed.kin"
            } else {
                "ui.reason.GuardianAppointed.other"
            },
            &[("ilu", &wards.to_string()), ("waga", &weight.to_string())],
        ),
        CitizenReason::ModeChosen { mode, minutes: m } => c.fmt_key(
            l,
            "ui.reason.ModeChosen",
            &[
                ("co", &transport_mode(c, l, mode)),
                ("czas", &minutes(c, l, m)),
            ],
        ),
        CitizenReason::NoRouteForMode { mode, fallback } => c.fmt_key(
            l,
            "ui.reason.NoRouteForMode",
            &[
                ("co", &transport_mode(c, l, mode)),
                ("zamiast", &transport_mode(c, l, fallback)),
            ],
        ),
        CitizenReason::RefuelNeeded { level_permille } => c.fmt_key(
            l,
            "ui.reason.RefuelNeeded",
            &[("promile", &level_permille.to_string())],
        ),
        CitizenReason::StationChosen {
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
        CitizenReason::TripDelayed {
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
        CitizenReason::ModeCompared {
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
        CitizenReason::NoParkingAtDestination { lots_searched } => c.fmt_key(
            l,
            "ui.reason.NoParkingAtDestination",
            &[("ile", &lots_searched.to_string())],
        ),
        CitizenReason::LeftBehind { line, waited_min } => c.fmt_key(
            l,
            "ui.reason.LeftBehind",
            &[
                ("linia", &line.to_string()),
                ("czekal", &minutes(c, l, waited_min)),
            ],
        ),
        CitizenReason::ShopChosen {
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
        CitizenReason::OfferRejected {
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
        CitizenReason::PurchaseDeferred {
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
        CitizenReason::CreditApproved {
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
        CitizenReason::CreditRejected {
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
        CitizenReason::BudgetShortfall { cost, gap_permille } => c.fmt_key(
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
        CitizenReason::VoteCast {
            candidate,
            driver,
            margin_bp,
        } => c.fmt_key(
            l,
            "ui.reason.VoteCast",
            &[
                ("kandydat", &format!("{}", candidate + 1)),
                ("motyw", &vote_driver(c, l, driver)),
                ("przewaga", &procent(u32::from(margin_bp))),
            ],
        ),
        CitizenReason::BrandLearned {
            brand,
            source,
            channel,
            awareness,
        } => c.fmt_key(
            l,
            "ui.reason.BrandLearned",
            &[
                ("marka", &marka(brand)),
                (
                    "skad",
                    &match channel {
                        Some(ch) => ad_channel(c, l, ch),
                        None => touch_source(c, l, source),
                    },
                ),
                ("znajomosc", &format!("{}", awareness.get())),
            ],
        ),
        CitizenReason::BrandExperience {
            brand,
            expected,
            actual,
            delta,
        } => c.fmt_key(
            l,
            if delta < 0 {
                "ui.reason.BrandExperienceDown"
            } else {
                "ui.reason.BrandExperienceUp"
            },
            &[
                ("marka", &marka(brand)),
                ("oczekiwana", &format!("{}", expected.get())),
                ("faktyczna", &format!("{}", actual.get())),
                ("zmiana", &format!("{}", delta.abs())),
            ],
        ),
    }
}
