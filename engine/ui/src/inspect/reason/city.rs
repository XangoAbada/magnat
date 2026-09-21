//! Powody decyzji miasta w języku gracza (`R2-WP20`).
//!
//! Dwadzieścia jeden powodów: daniny, wydatki, sieci, zdarzenia, usługi,
//! urzędy, uchwały i wybory.
//!
//! Osobny plik, bo `describe` rosła liniowo z liczbą faz i miała 1290 linii przy
//! progu błędu kontroli strukturalnej wynoszącym 250. Po podziale dopisanie powodu
//! przez fazę dotykającą firm nie powiększa pliku, który obsługuje mieszkańców.
//!
//! **Wyczerpujący `match` bez ramienia `_`** — `K-12` obowiązuje tutaj tak samo jak
//! przed podziałem, tylko na jednym aktorze zamiast na wszystkich naraz. Wariant bez
//! zdania łamie kompilację; pilnuje tego dodatkowo `tests/ui/city_reason_bez_wildcard.rs`.

use super::*;

#[must_use]
#[allow(clippy::too_many_lines)]
pub(super) fn opisz(c: &Catalog, l: Locale, r: CityReason, _n: Names<'_>) -> String {
    match r {
        CityReason::TaxAssessed {
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
        CityReason::TaxSettled { kind, amount } => c.fmt_key(
            l,
            "ui.reason.TaxSettled",
            &[
                ("danina", &tax_kind(c, l, kind)),
                ("kwota", &crate::zlotowki(amount)),
            ],
        ),
        CityReason::TaxOverdue {
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
        CityReason::TaxAbated { kind, why, amount } => c.fmt_key(
            l,
            "ui.reason.TaxAbated",
            &[
                ("danina", &tax_kind(c, l, kind)),
                ("kwota", &crate::zlotowki(amount)),
                ("powod", &abate_reason(c, l, why)),
            ],
        ),
        CityReason::PublicSpend { category, amount } => c.fmt_key(
            l,
            "ui.reason.PublicSpend",
            &[
                ("kierunek", &spend_category(c, l, category)),
                ("kwota", &crate::zlotowki(amount)),
            ],
        ),
        CityReason::MunicipalBondIssued {
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
        CityReason::BudgetDeficitClosed { gap, cut_bp } => c.fmt_key(
            l,
            "ui.reason.BudgetDeficitClosed",
            &[
                ("luka", &crate::zlotowki(gap)),
                ("ciecie", &procent(u32::from(cut_bp))),
            ],
        ),
        CityReason::LoadShed {
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
        CityReason::GridTripped {
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
        CityReason::EventStarted {
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
        CityReason::EventEnded {
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
        CityReason::ServiceQuality {
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
        CityReason::PermitIssued { kind, waited_days } => c.fmt_key(
            l,
            "ui.reason.PermitIssued",
            &[
                ("pozwolenie", &permit_kind(c, l, kind)),
                ("dni", &days_txt(c, l, u32::from(waited_days))),
            ],
        ),
        CityReason::CaseOpened { agency, evidence } => c.fmt_key(
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
        CityReason::RemedyImposed {
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
        CityReason::ShadowShareSet {
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
        CityReason::PolicyEnacted {
            kind,
            for_bp,
            delay_days,
        } => c.fmt_key(
            l,
            "ui.reason.PolicyEnacted",
            &[
                ("uchwala", &policy_kind(c, l, kind)),
                ("poparcie", &procent(u32::from(for_bp))),
                ("dni", &days(c, l, u32::from(delay_days))),
            ],
        ),
        CityReason::TaxRateChanged {
            kind,
            from_bp,
            to_bp,
            gap_bp,
        } => c.fmt_key(
            l,
            if to_bp > from_bp {
                "ui.reason.TaxRateUp"
            } else {
                "ui.reason.TaxRateDown"
            },
            &[
                ("danina", &tax_kind(c, l, kind)),
                ("z", &procent(u32::from(from_bp))),
                ("na", &procent(u32::from(to_bp))),
                ("luka", &format!("{gap_bp}")),
            ],
        ),
        CityReason::TenderPublished {
            subject,
            subject_id,
            budget,
        } => c.fmt_key(
            l,
            "ui.reason.TenderPublished",
            &[
                ("przedmiot", &tender_kind(c, l, subject)),
                ("numer", &format!("{subject_id}")),
                ("budzet", &crate::zlotowki(budget)),
            ],
        ),
        CityReason::TenderAwarded {
            subject,
            price,
            score_bp,
            runner_up_bp,
            bids,
        } => {
            if bids == 0 {
                c.fmt_key(
                    l,
                    "ui.reason.TenderNoBids",
                    &[("przedmiot", &tender_kind(c, l, subject))],
                )
            } else {
                c.fmt_key(
                    l,
                    "ui.reason.TenderAwarded",
                    &[
                        ("przedmiot", &tender_kind(c, l, subject)),
                        ("cena", &crate::zlotowki(price)),
                        ("punkty", &procent(u32::from(score_bp))),
                        ("drugi", &procent(u32::from(runner_up_bp))),
                        ("oferty", &format!("{bids}")),
                    ],
                )
            }
        }
        CityReason::ElectionHeld {
            turnout_bp,
            winner_bp,
            incumbent,
        } => c.fmt_key(
            l,
            if incumbent {
                "ui.reason.ElectionHeldIncumbent"
            } else {
                "ui.reason.ElectionHeldChange"
            },
            &[
                ("frekwencja", &procent(u32::from(turnout_bp))),
                ("wynik", &procent(u32::from(winner_bp))),
            ],
        ),
    }
}
