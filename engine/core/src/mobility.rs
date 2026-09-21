//! Opłaty mobilne w drodze z komponentu do ksiąg (`R2-WP32`, `K-72`).
//!
//! # Dlaczego to stoi w `core`, skoro zbiera to `sim/traffic`, a księguje `sim/economy`
//!
//! Ten sam powód, dla którego w `core` stoi [`crate::service::ServiceCoverage`]
//! (`K-64`) i `OpenHours` (`K-34`): to jest **struktura danych bez logiki**, a piszący
//! i czytający stoją w grafie zależności **obok siebie**. `sim/traffic` nie zależy od
//! `sim/economy` i nie może, `sim/economy` nie zależy od `sim/traffic` i nie chce —
//! a wspólny przodek jest jeden.
//!
//! # Co to naprawia
//!
//! Do R2 drugą stroną każdego grosza wydanego na paliwo, bilet i taryfę był **rejestr
//! w `sim/traffic`, nie konto**. Grosz wychodził z `Wealth.cash` i nie wchodził do
//! `Books`, więc niezmiennik świata (`society::total_money + Books::total_balance`)
//! domykał się tylko wtedy, gdy ręcznie doliczyło się rejestry — a i wtedy nie do zera,
//! bo taryfa taksówkowa rosła bez płatnika. Rozjazd był **mnożnikowy**: rósł razem
//! z wydatkami na dojazdy, więc każda faza dokładająca transport go powiększała.
//!
//! Tutaj kwota czeka jedną minutę. `sim/traffic` odkłada ją w tej samej operacji,
//! w której zdejmuje pieniądz z komponentu; `sim/economy` odbiera ją i wpłaca na konto
//! kanału. Kwota nieodebrana jest stanem świata i wchodzi do hasha, tak samo jak
//! naliczona danina w `b2b_outbox`.

use crate::hash::{HashState, StateHasher};
use crate::types::Money;
use crate::vocab::{MobilityChannel, MOBILITY_CHANNEL_COUNT};

/// Opłaty pobrane od mieszkańców, jeszcze niezaksięgowane.
///
/// Zasób świata. Pisze `sim/traffic` (w tej samej operacji, w której obciąża
/// `Wealth.cash`), czyta i zeruje `sim/economy`. Świat bez ruchu ma tu same zera
/// i nie płaci za ten mechanizm ani cyklu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MobilityDue {
    pending: [Money; MOBILITY_CHANNEL_COUNT],
    /// Paliwo spalone przez tabor komunikacji: **przewoźnik płaci stacji**, więc obie
    /// strony są w księgach i nie ma tu nic do pobrania od mieszkańca. Osobne pole,
    /// bo to jedyny przepływ mobilny między dwoma kontami, a nie z komponentu na konto.
    transit_fuel: Money,
}

impl MobilityDue {
    /// Dopisuje opłatę pobraną od mieszkańca.
    pub fn charge(&mut self, channel: MobilityChannel, amount: Money) {
        let slot = &mut self.pending[channel.as_index()];
        *slot = Money(slot.get().saturating_add(amount.get()));
    }

    /// Dopisuje koszt paliwa taboru — przepływ przewoźnik → stacja.
    pub fn charge_transit_fuel(&mut self, amount: Money) {
        self.transit_fuel = Money(self.transit_fuel.get().saturating_add(amount.get()));
    }

    /// Ile czeka na kanale.
    #[must_use]
    pub fn pending(&self, channel: MobilityChannel) -> Money {
        self.pending[channel.as_index()]
    }

    #[must_use]
    pub fn pending_transit_fuel(&self) -> Money {
        self.transit_fuel
    }

    /// Czy jest cokolwiek do zaksięgowania — jedno porównanie na minutę w świecie
    /// bez ruchu.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.transit_fuel.get() == 0 && self.pending.iter().all(|m| m.get() == 0)
    }

    /// Wyjmuje wszystko, zostawiając zera.
    ///
    /// Wyjmuje, a nie czyta — z tego samego powodu co `Market::take_charges`: dwa
    /// odczyty bez wyzerowania zaksięgowałyby tę samą opłatę dwa razy.
    pub fn take(&mut self) -> MobilityDue {
        std::mem::take(self)
    }

    /// Kanały w kolejności wariantów — deterministyczna kolejność księgowania.
    pub fn channels(&self) -> impl Iterator<Item = (MobilityChannel, Money)> + '_ {
        MobilityChannel::ALL
            .iter()
            .copied()
            .map(move |c| (c, self.pending[c.as_index()]))
    }
}

impl HashState for MobilityDue {
    fn hash_state(&self, h: &mut StateHasher) {
        for m in &self.pending {
            m.hash_state(h);
        }
        self.transit_fuel.hash_state(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_zostawia_zera_i_oddaje_calosc() {
        let mut d = MobilityDue::default();
        assert!(d.is_empty());
        d.charge(MobilityChannel::Fuel, Money(1_200));
        d.charge(MobilityChannel::Taxi, Money(800));
        d.charge_transit_fuel(Money(500));
        assert!(!d.is_empty());

        let wzięte = d.take();
        assert!(d.is_empty(), "po odebraniu kanały nie są puste");
        assert_eq!(wzięte.pending(MobilityChannel::Fuel), Money(1_200));
        assert_eq!(wzięte.pending(MobilityChannel::Taxi), Money(800));
        assert_eq!(wzięte.pending(MobilityChannel::Parking), Money::ZERO);
        assert_eq!(wzięte.pending_transit_fuel(), Money(500));
        // Kolejność kanałów jest kolejnością wariantów, a nie wstawiania.
        let kolejnosc: Vec<_> = wzięte.channels().map(|(c, _)| c).collect();
        assert_eq!(kolejnosc, MobilityChannel::ALL.to_vec());
    }
}
