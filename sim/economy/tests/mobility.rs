//! Opłaty mobilne wchodzą do ksiąg (`R2-WP32`, `K-72`, pozycja 60 wykazu `R2`).
//!
//! Przed naprawą drugą stroną każdego grosza wydanego na paliwo, bilet i taryfę był
//! **rejestr w `sim/traffic`, nie konto**, więc niezmiennik świata
//! `society::total_money + Books::total_balance()` nie domykał się z tolerancją zero —
//! a rozjazd rósł razem z wydatkami na dojazdy.

mod common;

use common::bench;
use magnat_core::{MobilityChannel, MobilityDue, Money, Tick};
use magnat_economy::books::Books;
use magnat_economy::mobility::absorb_mobility;
use magnat_ecs::World;

/// Świat z księgami, rynkiem i kanałem opłat — bez mieszkańców, bo tu chodzi
/// wyłącznie o drogę pieniądza z komponentu na konto.
fn swiat() -> (World, magnat_economy::Market) {
    let b = bench(23, &[]);
    let mut world = World::new(23);
    world.insert_resource(b.books);
    world.insert_resource(MobilityDue::default());
    (world, b.market)
}

/// Pieniądz zdjęty z portfela wchodzi na konto kanału — co do grosza i w całości.
#[test]
fn oplaty_wchodza_na_konta_kanalow() {
    let (mut world, market) = swiat();
    let przed = world.resource::<Books>().total_balance();

    {
        let d = world.resource_mut::<MobilityDue>();
        d.charge(MobilityChannel::Fuel, Money(12_000));
        d.charge(MobilityChannel::TransitTicket, Money(3_400));
        d.charge(MobilityChannel::Taxi, Money(9_900));
    }
    let dzien = absorb_mobility(&mut world, &market, Tick(1));

    assert_eq!(
        dzien.collected,
        Money(25_300),
        "nie wszystko weszło do ksiąg"
    );
    assert_eq!(
        world.resource::<Books>().total_balance(),
        Money(przed.get() + 25_300),
        "suma sald nie urosła o pobrane opłaty"
    );
    assert!(
        world.resource::<MobilityDue>().is_empty(),
        "kanał nie został opróżniony — druga minuta zaksięgowałaby to samo raz jeszcze"
    );
}

/// Niezmiennik świata domyka się **z tolerancją zero groszy**, kiedy druga strona
/// opłaty jest kontem, a nie rejestrem (`K-61`, `K-72`).
///
/// `sektor` to pieniądz po stronie komponentów. Kurs taksówką zdejmuje go z portfela
/// i wkłada na konto: lewa strona niezmiennika ma zostać ta sama.
#[test]
fn niezmiennik_swiata_domyka_sie_po_zaksiegowaniu_oplat() {
    let (mut world, market) = swiat();
    // Świat startuje z całym pieniądzem na kontach; żeby mieszkańcy mieli czym
    // zapłacić, musi im on najpierw wyjść do komponentów — tą samą drogą, którą
    // idzie wypłata.
    let mut sektor = Money::ZERO;
    assert_eq!(
        world.resource::<Books>().check_world_conservation(sektor),
        Ok(())
    );
    let rest = market.rest_of_world();
    world
        .resource_mut::<Books>()
        .household_receive(
            rest,
            Money(40_000),
            magnat_economy::books::TxMemo::new(
                magnat_economy::books::TxKind::Withdrawal,
                magnat_core::DecisionReason::Unspecified,
            ),
            Tick(0),
        )
        .expect("wypłata do komponentów");
    sektor = Money(40_000);
    assert_eq!(
        world.resource::<Books>().check_world_conservation(sektor),
        Ok(()),
        "niezmiennik pękł zaraz po wypłacie"
    );

    // Trzy kursy taksówką: portfele chudną, kanał tyje.
    let kurs = Money(1_300);
    for _ in 0..3 {
        world
            .resource_mut::<MobilityDue>()
            .charge(MobilityChannel::Taxi, kurs);
        sektor = Money(sektor.get() - kurs.get());
    }
    // Dopóki opłata jest w drodze, pieniądz jest po stronie komponentów — kanał
    // jest przedłużeniem portfela, a nie osobnym workiem.
    let w_drodze = world
        .resource::<MobilityDue>()
        .pending(MobilityChannel::Taxi);
    assert_eq!(
        world
            .resource::<Books>()
            .check_world_conservation(Money(sektor.get() + w_drodze.get())),
        Ok(()),
        "niezmiennik pękł, zanim opłata doszła do ksiąg"
    );

    absorb_mobility(&mut world, &market, Tick(2));
    assert_eq!(
        world.resource::<Books>().check_world_conservation(sektor),
        Ok(()),
        "niezmiennik pękł po zaksięgowaniu opłat"
    );
}

/// Paliwo taboru przechodzi między dwoma kontami, więc podaż pieniądza nie drga.
#[test]
fn paliwo_taboru_nie_zmienia_podazy() {
    let (mut world, market) = swiat();
    let przed = world.resource::<Books>().total_balance();
    world
        .resource_mut::<MobilityDue>()
        .charge_transit_fuel(Money(7_700));

    let dzien = absorb_mobility(&mut world, &market, Tick(3));
    assert_eq!(dzien.transit_fuel, Money(7_700));
    assert_eq!(
        world.resource::<Books>().total_balance(),
        przed,
        "przelew między kontami zmienił sumę sald"
    );
    assert_eq!(world.resource::<Books>().check_conservation(), Ok(()));
}
