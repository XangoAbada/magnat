//! Opłaty mobilne wchodzą do ksiąg (`R2-WP32`, `K-72`, pozycja 60 wykazu `R2`).
//!
//! Drugą stroną każdego grosza wydanego na paliwo, bilet i taryfę był do R2 **rejestr
//! w `sim/traffic`, a nie konto**: `FuelLedger` i `FareLedger` wchodziły do hasha, bo
//! to jest pieniądz, ale żaden z nich nie miał konta w `Books`. Grosz wychodził
//! z jednej sumy i nie wchodził do drugiej, więc niezmiennik świata
//! `society::total_money + Books::total_balance()` domykał się tylko wtedy, gdy ręcznie
//! doliczyło się rejestry — i nawet wtedy nie do zera, bo taryfa taksówkowa rosła bez
//! płatnika. Rozjazd był **mnożnikowy**: +63,2 tys. zł na 40 dób po M5c, +163,0 tys.
//! po M5d, bo pieniądz kredytowy zwiększył wydatki na dojazdy.
//!
//! Kanałem między jednym a drugim jest [`MobilityDue`] w `engine/core` — `sim/traffic`
//! i `sim/economy` nie widzą się nawzajem i widzieć nie mają. Ten sam wzorzec, którym
//! `sim/city` oddaje pokrycie usług (`K-64`).

use magnat_core::{DecisionReason, MobilityDue, Money, Tick};
use magnat_ecs::World;

use crate::books::{Books, TxKind, TxMemo};
use crate::market::Market;

/// Co zaksięgowała minuta opłat mobilnych — do raportu scenariusza i do testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MobilityDay {
    /// Suma opłat pobranych od mieszkańców, która weszła na konta kanałów.
    pub collected: Money,
    /// Paliwo taboru: przepływ przewoźnik → stacja, obie strony w księgach.
    pub transit_fuel: Money,
}

/// Konsument [`MobilityDue`] — jedyna droga, którą opłata za dojazd wchodzi do ksiąg.
///
/// Stoi obok `absorb_settlements` i `absorb_payroll`, bo to ten sam kształt: kwotę
/// zbiera crate, który jej nie zaksięguje, a księguje ten, który ma `Books`. Świat bez
/// ruchu nie płaci za ten mechanizm ani cyklu — `MobilityDue` jest wtedy zerowe albo
/// nie ma go w ogóle.
pub fn absorb_mobility(world: &mut World, market: &Market, t: Tick) -> MobilityDay {
    let mut dzien = MobilityDay::default();
    let Some(due) = world
        .get_resource_mut::<MobilityDue>()
        .filter(|d| !d.is_empty())
        .map(MobilityDue::take)
    else {
        return dzien;
    };
    let Some(books) = world.get_resource_mut::<Books>() else {
        return dzien;
    };
    // Kolejność kanałów jest kolejnością wariantów `MobilityChannel`, a nie kolejnością
    // pobrania — dziennik transakcji wchodzi do hasha stanu (00 §3.2).
    for (kanal, kwota) in due.channels() {
        if kwota.get() <= 0 {
            continue;
        }
        let konto = market.mobility_account(kanal);
        let memo = TxMemo::new(
            TxKind::Mobility { channel: kanal },
            DecisionReason::Unspecified,
        );
        // `household_pay`: pieniądz wszedł do ksiąg **z komponentu** — `sim/traffic`
        // zdjął go z `Wealth.cash` w tej samej operacji, w której odłożył go tutaj.
        if books.household_pay(konto, kwota, memo, t).is_ok() {
            dzien.collected = Money(dzien.collected.get() + kwota.get());
        }
    }
    let paliwo_taboru = due.pending_transit_fuel();
    if paliwo_taboru.get() > 0 {
        let od = market.mobility_account(magnat_core::MobilityChannel::TransitTicket);
        let do_stacji = market.mobility_account(magnat_core::MobilityChannel::Fuel);
        dzien.transit_fuel = paliwo_taboru;
        // Przewoźnik płaci stacji — **przelew między kontami**, a nie wpłata
        // z komponentu: po obu stronach jest konto, więc podaż pieniądza nie drga.
        //
        // Dopóki nikt nie obsadził kont kanałów, oba są kontem reszty świata i przelew
        // z konta na to samo konto jest niczym — `Books::transfer` odrzuciłby go jako
        // `SameAccount`. To nie jest zgubiona kwota: nie było czego przenosić.
        if od != do_stacji {
            let memo = TxMemo::new(
                TxKind::Mobility {
                    channel: magnat_core::MobilityChannel::Fuel,
                },
                DecisionReason::Unspecified,
            );
            let _ = books.transfer(od, do_stacji, paliwo_taboru, memo, t);
        }
    }
    dzien
}
