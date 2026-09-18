//! Dry-run polityki: „co by ustawiła przez ostatnie doby" (M9d §5.6 pkt 8, WP8).
//!
//! # Dlaczego to nie jest drugi model
//!
//! Podgląd liczy **tym samym kodem**, którym liczy wykonanie ([`super::decide`]),
//! nad **zapisanymi** faktami zakładu ([`super::facts::PolicyTrace`]). Druga
//! arytmetyka podglądu rozjechałaby się z wykonaniem przy pierwszej zmianie wzoru —
//! a dry-run istnieje właśnie po to, żeby powiedzieć, co się stanie.
//!
//! Menedżer jest w podglądzie **doskonały**, bo pytanie brzmi „co ta reguła zrobi",
//! a nie „jak bardzo spartaczy to ten człowiek". Odchyłkę pokazuje karta inspekcji
//! po fakcie, a nie podgląd przed przypięciem.

use magnat_core::{GoodId, Money, SimCalendar, SiteId, Tick};

use super::decide::{decyduj, PolicyOutcome, RunCtx};
use crate::manager_exec::ManagerExecution;
use crate::market::Market;

impl Market {
    /// „Co by ustawiła ta polityka przez ostatnie dni" — dry-run z M9d §5.6 pkt 8.
    ///
    /// Odtwarza politykę na **zapisanych** faktach zakładu, tym samym kodem, którym
    /// liczy się wykonanie ([`decyduj`]). Menedżer jest doskonały, bo pytanie brzmi
    /// „co ta reguła zrobi", a nie „jak bardzo spartaczy to ten człowiek" — odchyłkę
    /// menedżera pokazuje karta inspekcji po fakcie, nie podgląd przed przypięciem.
    ///
    /// Pusty wynik znaczy „zakład nie ma śladu": śladu nie prowadzi zakład nieśledzony,
    /// a śledzenie włącza otwarcie karty albo przejęcie zakładu przez gracza.
    #[must_use]
    pub fn dry_run(&self, site: SiteId, policy: &magnat_policy::Policy) -> DryRun {
        let m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return DryRun::default();
        };
        let vat = {
            let brutto = m.tax.gross_from_net(GoodId(0), Money(10_000));
            i32::try_from(brutto.get() - 10_000).unwrap_or(0)
        };
        let dni = m.shops[i as usize]
            .trace
            .days()
            .iter()
            .map(|d| {
                let mut fakty = d.facts.clone();
                let ctx = RunCtx {
                    cal: SimCalendar::new(d.day),
                    vat_bp: vat,
                    // Saldo, nastrój załogi i wakaty są w podglądzie **nieznane**,
                    // bo ślad niesie półkę — to o niej mówi polityka cenowa i zapasowa.
                    // Konsekwencja jest jawna i widać ją w podsumowaniu: reguła oparta
                    // na saldzie wychodzi jako `Blind`, czyli „nie umiem odpowiedzieć",
                    // a nie jako „warunek niespełniony". `DrySummary.blind` liczy
                    // dokładnie te przypadki i to jest jedyny sposób, żeby gracz
                    // dowiedział się o tym **przed** przypięciem polityki.
                    //
                    // ponytail: sufit nazwany — dopisanie salda do śladu kosztuje
                    // jedną liczbę na dobę i wejdzie razem z panelem finansów (`M9e`),
                    // czyli wtedy, gdy będzie miało czytelnika.
                    cash: None,
                    manager_skill: None,
                    staff_mood: None,
                    open_positions: 0,
                    exec: ManagerExecution::flawless(),
                    error_bp: 0,
                };
                let decyzje = decyduj(policy, &mut fakty, &ctx, &*m.tax);
                DryDay {
                    day: d.day,
                    decisions: decyzje.into_iter().map(|(s, _)| s).collect(),
                }
            })
            .collect();
        DryRun { days: dni }
    }
}

/// Wynik dry-runu: decyzje polityki na każdej zapisanej dobie.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DryRun {
    pub days: Vec<DryDay>,
}

impl DryRun {
    /// Ceny, które polityka ustawiłaby temu towarowi — nakładka na wykres ceny.
    #[must_use]
    pub fn prices_of(&self, good: GoodId) -> Vec<(Tick, Money)> {
        self.days
            .iter()
            .filter_map(|d| {
                d.decisions
                    .iter()
                    .rev()
                    .find_map(|s| match s {
                        PolicyOutcome::Price(g, m) if *g == good => Some(*m),
                        _ => None,
                    })
                    .map(|m| (d.day, m))
            })
            .collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.days.is_empty()
    }
}

/// Jedna doba dry-runu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DryDay {
    pub day: Tick,
    pub decisions: Vec<PolicyOutcome>,
}
