//! Polityki zapasów (M6b §5.7, WP6).
//!
//! Zakład decyduje, **ile** i **kiedy** zamówić. Kaskada niedoboru (§5.7, [`crate::shortage`])
//! decyduje, co robić, gdy zamówienie nie zdążyło — to są dwie różne odpowiedzi na dwa
//! różne pytania i dlatego mieszkają osobno.
//!
//! Dwie rzeczy są tu obroną przed efektem byczego bicza (`R3`), i obie są decyzjami,
//! nie przypadkiem:
//!
//! 1. **Prognoza zużycia jest nominalna, nie wczorajsza.** Zapotrzebowanie liczy się
//!    z przepustowości linii i receptury, a nie ze zużycia z ostatniej doby. Doba,
//!    w której zakład stał, dałaby prognozę zero i zamówienie zero — a potem
//!    zamówienie podwójne. To jest dokładnie ten mechanizm, który wzmacnia wahania.
//! 2. **To, co już jedzie, liczy się do zapasu.** Zamawianie bez odjęcia towaru
//!    w drodze zamawia to samo tyle razy, ile przeglądów zmieści się w czasie dostawy.

use magnat_core::{GoodId, HashState, Mass, SimCalendar, SimMinute, SiteId, StateHasher};

use crate::catalog::Catalog;
use crate::plant::PlantSite;
use crate::shortage::consumption_per_minute;
use crate::transport::Transport;
use crate::Store;

/// Skąd zakład woli brać towar. Rozstrzyganie należy do rynku (M6c); tutaj jest sama
/// preferencja, bo to ona jest własnością zakładu, a nie rynku.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PreferredSource {
    Contract(magnat_core::ContractId),
    Spot,
    Import,
    Any,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MinMaxParams {
    pub reorder_point: Mass,
    pub target: Mass,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InventoryPolicy {
    MinMax(MinMaxParams),
    /// Zamawiaj dokładnie na czas dostawy plus bufor bezpieczeństwa.
    Jit {
        lead_minutes: u32,
        safety_minutes: u32,
    },
    /// Sezonowa: `base` przemnożone przez krzywą roczną.
    ///
    /// Krzywa ma **dwanaście punktów po trzydzieści dni**, nie 365 — kalendarz jest
    /// 360-dniowy (`K-1`) i żniwa wypadają na stałym dniu roku. Mnożnik w promilach,
    /// 1000 = bez zmiany.
    ///
    /// `ponytail:` krzywa siedzi w regule, a nie w rejestrze pod `SeasonCurveId`.
    /// Sufit nazwany: dwanaście `u16` to dwadzieścia cztery bajty, czyli tyle co uchwyt
    /// plus wpis w tablicy, a rejestr kosztowałby własne dane i własne ładowanie.
    /// Gdy krzywych zacznie być więcej niż reguł — czyli gdy zaczną się powtarzać —
    /// wraca `SeasonCurveId` i tablica w `data/`.
    Seasonal {
        base: MinMaxParams,
        curve_permille: [u16; 12],
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Review {
    Continuous,
    Periodic { every_minutes: u32, at_minute: u32 },
}

impl Review {
    /// Czy w tej minucie wypada przegląd.
    #[must_use]
    pub fn due(self, now: SimMinute) -> bool {
        match self {
            Review::Continuous => true,
            Review::Periodic {
                every_minutes,
                at_minute,
            } => {
                if every_minutes == 0 {
                    return false;
                }
                now.0 % u64::from(every_minutes) == u64::from(at_minute % every_minutes)
            }
        }
    }
}

/// Reguła zapasu jednego towaru w jednym zakładzie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InventoryRule {
    pub good: GoodId,
    pub policy: InventoryPolicy,
    pub review: Review,
    pub preferred: PreferredSource,
}

impl InventoryRule {
    /// Sklep osiedlowy: przegląd dobowy o 4:00, bo jedna dostawa dziennie jest tańsza
    /// niż pięć.
    #[must_use]
    pub fn daily_shop(good: GoodId, reorder_point: Mass, target: Mass) -> InventoryRule {
        InventoryRule {
            good,
            policy: InventoryPolicy::MinMax(MinMaxParams {
                reorder_point,
                target,
            }),
            review: Review::Periodic {
                every_minutes: 1_440,
                at_minute: 240,
            },
            preferred: PreferredSource::Any,
        }
    }

    /// Zakład ciągły: przegląd ciągły, bo linia stoi w minutach, nie w dobach.
    #[must_use]
    pub fn continuous(good: GoodId, lead_minutes: u32, safety_minutes: u32) -> InventoryRule {
        InventoryRule {
            good,
            policy: InventoryPolicy::Jit {
                lead_minutes,
                safety_minutes,
            },
            review: Review::Continuous,
            preferred: PreferredSource::Any,
        }
    }
}

impl HashState for InventoryRule {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.good.0);
        match self.policy {
            InventoryPolicy::MinMax(p) => {
                h.write_u8(0);
                p.reorder_point.hash_state(h);
                p.target.hash_state(h);
            }
            InventoryPolicy::Jit {
                lead_minutes,
                safety_minutes,
            } => {
                h.write_u8(1);
                h.write_u32(lead_minutes);
                h.write_u32(safety_minutes);
            }
            InventoryPolicy::Seasonal {
                base,
                curve_permille,
            } => {
                h.write_u8(2);
                base.reorder_point.hash_state(h);
                base.target.hash_state(h);
                for c in curve_permille {
                    h.write_u16(c);
                }
            }
        }
        match self.review {
            Review::Continuous => h.write_u8(0),
            Review::Periodic {
                every_minutes,
                at_minute,
            } => {
                h.write_u8(1);
                h.write_u32(every_minutes);
                h.write_u32(at_minute);
            }
        }
        match self.preferred {
            PreferredSource::Contract(c) => {
                h.write_u8(0);
                c.0.hash_state(h);
            }
            PreferredSource::Spot => h.write_u8(1),
            PreferredSource::Import => h.write_u8(2),
            PreferredSource::Any => h.write_u8(3),
        }
    }
}

/// Czego zakład potrzebuje. Zamienia to na zlecenie transportowe wołający — bo to on
/// wie, **skąd** wziąć towar, a to jest pytanie do rynku (M6c), nie do magazynu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReplenishRequest {
    pub site: SiteId,
    pub good: GoodId,
    pub mass: Mass,
    pub preferred: PreferredSource,
}

/// Masa towaru już jadącego do tego zakładu — to, czego **nie** trzeba zamawiać drugi raz.
#[must_use]
pub fn on_order(transport: &Transport, site: SiteId, good: GoodId) -> Mass {
    Mass(
        transport
            .iter()
            .filter(|o| o.to == site && o.good == good && !o.state.is_final())
            .map(|o| o.mass.0)
            .sum(),
    )
}

/// Przegląd zapasów zakładu. Zwraca zapotrzebowanie, którego jeszcze nikt nie pokrywa.
pub fn review(
    cat: &Catalog,
    store: &Store,
    transport: &Transport,
    site: &PlantSite,
    rules: &[InventoryRule],
    now: SimMinute,
) -> Vec<ReplenishRequest> {
    let mut zamowienia = Vec::new();
    for r in rules {
        if !r.review.due(now) {
            continue;
        }
        let zapas = crate::plant::input_stock(store, site, r.good).0;
        let w_drodze = on_order(transport, site.site, r.good).0;
        let rate = consumption_per_minute(cat, site, r.good);
        let (prog, cel) = progi(r, rate, now);
        if zapas + w_drodze > prog {
            continue;
        }
        let brakuje = cel - zapas - w_drodze;
        if brakuje > 0 {
            zamowienia.push(ReplenishRequest {
                site: site.site,
                good: r.good,
                mass: Mass(brakuje),
                preferred: r.preferred,
            });
        }
    }
    zamowienia
}

/// Próg zamówienia i cel uzupełnienia w gramach.
fn progi(r: &InventoryRule, rate_per_minute: i64, now: SimMinute) -> (i64, i64) {
    match r.policy {
        InventoryPolicy::MinMax(p) => (p.reorder_point.0, p.target.0),
        InventoryPolicy::Jit {
            lead_minutes,
            safety_minutes,
        } => {
            let prog = rate_per_minute * i64::from(lead_minutes + safety_minutes);
            // Cel to próg plus jeszcze jeden czas dostawy: zamówienie dokładnie do progu
            // wywołałoby kolejne zamówienie w następnej minucie i zalało przewoźnika.
            (prog, prog + rate_per_minute * i64::from(lead_minutes))
        }
        InventoryPolicy::Seasonal {
            base,
            curve_permille,
        } => {
            let miesiac = SimCalendar::from_minute(now).month_of_year() as usize - 1;
            let m = i64::from(curve_permille[miesiac.min(11)]);
            (
                base.reorder_point.0 * m / 1_000,
                base.target.0 * m / 1_000,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn przeglad_dobowy_wypada_raz_na_dobe_o_swojej_godzinie() {
        let r = Review::Periodic {
            every_minutes: 1_440,
            at_minute: 240,
        };
        assert!(r.due(SimMinute(240)));
        assert!(r.due(SimMinute(1_440 + 240)));
        assert!(!r.due(SimMinute(241)));
        assert!(Review::Continuous.due(SimMinute(7)));
    }

    /// Cel JIT jest **wyżej** niż próg. Równy próg dawałby zamówienie w każdej minucie
    /// po dostawie — czyli dokładnie ten bicz, którego polityka ma unikać.
    #[test]
    fn cel_jit_stoi_ponad_progiem() {
        let r = InventoryRule::continuous(GoodId(0), 120, 60);
        let (prog, cel) = progi(&r, 1_000, SimMinute(0));
        assert_eq!(prog, 180_000);
        assert_eq!(cel, 300_000);
        assert!(cel > prog);
    }
}
