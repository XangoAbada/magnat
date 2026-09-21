//! Dziedziczenie ponad gotówkę osobistą (`R2-WP10`, pozycja 18 wykazu `R2`).
//!
//! `InheritanceHook` jest punktem wymiany zaprojektowanym w M3 dla M5, M7 i M10 —
//! udziały w firmach, długi, majątek. Do R2 jego **jedyną** implementacją była
//! `NoInheritance` z pustym ciałem, a dziedziczona była wyłącznie gotówka osobista
//! zmarłego. Skutki były dwa i oba ciche: właściciel firmy umierał, a `Firm.owners`
//! dalej wskazywał na nieżyjącego mieszkańca; kredyt gospodarstwa, które właśnie się
//! rozwiązało, zostawał w `LoanBook` jako zobowiązanie bytu, którego nie ma.
//!
//! Hak mieszka tutaj, a nie w `sim/agents`, bo tu są księgi, kredyty i rejestr firm —
//! `sim/agents` nadal nie wie, czym jest udział w firmie i wiedzieć nie ma.

use magnat_agents::demography::InheritanceHook;
use magnat_agents::Household;
use magnat_core::{CitizenId, DecisionReason, Money, Tick};
use magnat_ecs::{CommandBuffer, World};
use magnat_firms::{FirmKey, Firms, Owner};

use crate::books::Books;
use crate::market::Market;

/// Dziedziczenie po stronie gospodarki: zobowiązania masy i udziały w firmach.
///
/// Trzyma uchwyt do rynku, bo `Market` jest `Arc<Mutex<…>>` i da się go skopiować
/// przed wstawieniem do świata; `Firms` i `Books` są zasobami i bierze je z `world`.
pub struct EconomyInheritance {
    market: Market,
    t: Tick,
}

impl EconomyInheritance {
    #[must_use]
    pub fn new(market: Market) -> EconomyInheritance {
        EconomyInheritance { market, t: Tick(0) }
    }

    /// Firmy, w których zmarły miał udział, w kolejności klucza.
    fn firmy_zmarlego(firms: &Firms, kto: CitizenId) -> Vec<(FirmKey, u16)> {
        let ja = Owner::Citizen(kto);
        firms
            .iter()
            .filter_map(|(key, f)| f.owners.iter().find(|s| s.owner == ja).map(|s| (key, s.bp)))
            .collect()
    }
}

impl InheritanceHook for EconomyInheritance {
    fn set_day(&mut self, day: u64) {
        self.t = Tick(day * magnat_core::time::MINUTES_PER_DAY);
    }

    /// Niespłacony kapitał kredytu obciąża masę spadkową — ale **tylko wtedy, gdy
    /// gospodarstwo przestaje istnieć**.
    ///
    /// Dopóki dom żyje, kredyt ma dłużnika: `Loan.borrower` to `AccountOwner::Household`,
    /// a nie mieszkaniec, więc śmierć jednego z domowników niczego w nim nie zmienia
    /// i spłata idzie dalej harmonogramem. Dopiero rozwiązanie gospodarstwa zostawia
    /// zobowiązanie bez dłużnika — i to jest cała luka, którą ten pakiet zamyka.
    fn estate_charge(&mut self, world: &mut World, deceased: CitizenId, estate: Money) -> Money {
        if estate.get() <= 0 {
            return Money::ZERO;
        }
        let Some(hh) = world
            .get::<magnat_agents::Identity>(deceased.0)
            .map(|id| id.household)
        else {
            return Money::ZERO;
        };
        // Ostatni domownik: po nim gospodarstwo się rozwiązuje (`R2-WP8`).
        let ostatni = magnat_agents::demography::household_by_index(world, hh)
            .and_then(|e| world.get::<Household>(e))
            .is_some_and(|h| h.size == 1);
        if !ostatni {
            return Money::ZERO;
        }
        let t = self.t;
        let market = self.market.clone();
        let Some(books) = world.get_resource_mut::<Books>() else {
            return Money::ZERO;
        };
        market.settle_household_loan(hh, estate, books, t)
    }

    /// Udziały w firmach przechodzą na spadkobierców **tymi samymi wagami co gotówka**.
    ///
    /// Brak spadkobiercy znaczy `Owner::City`: majątek bez właściciela przejmuje miasto,
    /// tak samo jak gotówka idzie wtedy na konto techniczne spadków. Firma bez żywego
    /// udziałowca przestaje być stanem świata — a była nim w każdym przebiegu, w którym
    /// ktokolwiek umarł.
    ///
    /// `ponytail:` przekroczenie progu ujawnienia albo kontroli przez **spadkobiercę**
    /// nie jest ogłaszane, bo `equity::corp::check_thresholds` wymaga zasobu `Equity`,
    /// którego świat bez giełdy nie ma. Sufit: pakiet kontrolny odziedziczony milczy
    /// do najbliższego fixingu, który i tak progi przelicza (M10d).
    fn on_inheritance(
        &mut self,
        world: &mut World,
        deceased: CitizenId,
        heirs: &[(CitizenId, u16)],
        _cmd: &mut CommandBuffer,
    ) {
        let t = self.t;
        let Some(firms) = world.get_resource_mut::<Firms>() else {
            return;
        };
        let udzialy = EconomyInheritance::firmy_zmarlego(firms, deceased);
        for (key, bp) in udzialy {
            let zmarly = Owner::Citizen(deceased);
            if heirs.is_empty() {
                crate::equity::cap::move_stake(
                    firms.get_mut(key).expect("firma z listy"),
                    zmarly,
                    Owner::City,
                    bp,
                );
                firms.log(
                    key,
                    t,
                    DecisionReason::Inheritance {
                        permille: 1_000,
                        heirs: 0,
                    },
                );
                continue;
            }
            // Podział permilami spadkobierców — sumują się do 1000, więc reszta
            // z dzielenia trafia do pierwszego z nich, tak samo jak przy gotówce.
            let mut rozdane = 0u16;
            for (i, (h, permille)) in heirs.iter().enumerate() {
                let czesc = if i + 1 == heirs.len() {
                    bp.saturating_sub(rozdane)
                } else {
                    ((u32::from(bp) * u32::from(*permille)) / 1_000) as u16
                };
                if czesc == 0 {
                    continue;
                }
                rozdane = rozdane.saturating_add(czesc);
                crate::equity::corp::settle_stake(firms, key, zmarly, Owner::Citizen(*h), czesc);
            }
            firms.log(
                key,
                t,
                DecisionReason::Inheritance {
                    permille: 1_000,
                    heirs: heirs.len().min(255) as u8,
                },
            );
        }
    }
}
