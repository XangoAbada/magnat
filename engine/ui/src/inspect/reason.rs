//! `DecisionReason` → tekst. **Jedno miejsce w całej grze** (M3 §6.1).
//!
//! `match` jest wyczerpujący i bez ramienia `_` — to jest cały mechanizm z 00 §K-12:
//! faza, która dopisze wariant, **nie skompiluje interfejsu**, dopóki nie napisze,
//! jak ten powód pokazać graczowi. Test `kazdy_powod_ma_tekst` sprawdza to od drugiej
//! strony: każdy wariant musi dać niepusty napis w obu językach.

use crate::loc::{Catalog, Locale};
use magnat_agents::SocialClass;
use magnat_core::{
    ActivityKind, CommitmentKind, DecisionReason, DeprivationEffect, LifeEventKind, MigrationKind,
    Money, NeedKind, StockCat, TraitId, TransportMode,
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
    }
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
        // Bloki M3 i M4 są kompletne: 26 wariantów (Unspecified + 100..=119 + 200..=204).
        assert_eq!(wszystkie().len(), 26);
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
        }
    }

    #[test]
    fn zegar_zawija_dobe() {
        assert_eq!(zegar(0), "00:00");
        assert_eq!(zegar(8 * 60 + 5), "08:05");
        assert_eq!(zegar(1439), "23:59");
    }
}
