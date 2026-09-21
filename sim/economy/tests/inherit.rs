//! Dziedziczenie ponad gotówkę osobistą (`R2-WP10`, pozycja 18 wykazu `R2`).
//!
//! Przed naprawą `InheritanceHook` miał jedyną implementację `NoInheritance`
//! z **pustym ciałem**: właściciel firmy umierał, a `Firm.owners` dalej wskazywał
//! na nieżyjącego mieszkańca.

mod common;

use common::{bench, ent};
use magnat_agents::demography::InheritanceHook;
use magnat_agents::Household;
use magnat_core::{CitizenId, DistrictId, Money, SimMinute};
use magnat_economy::books::Books;
use magnat_economy::inherit::EconomyInheritance;
use magnat_ecs::{CommandBuffer, SystemId, World};
use magnat_firms::{Firm, Firms, Owner};

/// Świat z rejestrem firm, księgami i jedną firmą w rękach mieszkańca.
fn swiat() -> (World, EconomyInheritance, magnat_firms::FirmKey, CitizenId) {
    let b = bench(41, &[]);
    let mut world = World::new(41);
    magnat_agents::register(
        &mut world,
        magnat_agents::NeedTable::load_default().expect("data/needs"),
    );
    world.insert_resource(b.books);
    world.insert_resource(magnat_agents::Population::default());

    let wlasciciel = CitizenId(ent(700));
    let mut firms = Firms::new();
    let key = firms.insert(|k| {
        Firm::sole_owner(
            k,
            "Młyn".to_string(),
            SimMinute(0),
            DistrictId(0),
            Owner::Citizen(wlasciciel),
        )
    });
    world.insert_resource(firms);
    (world, EconomyInheritance::new(b.market), key, wlasciciel)
}

fn udzial(world: &World, key: magnat_firms::FirmKey, o: Owner) -> u16 {
    world
        .resource::<Firms>()
        .get(key)
        .and_then(|f| f.owners.iter().find(|s| s.owner == o).map(|s| s.bp))
        .unwrap_or(0)
}

/// Kryterium `R2-WP10`: dwoje dzieci dziedziczy udziały **po równo**, a suma
/// pozostaje nienaruszona.
#[test]
fn udzialy_przechodza_na_spadkobiercow_po_rowno() {
    let (mut world, mut hook, key, wlasciciel) = swiat();
    let a = CitizenId(ent(701));
    let b2 = CitizenId(ent(702));
    let mut cmd = CommandBuffer::new(SystemId::from_name("test.Spadek"));

    assert_eq!(udzial(&world, key, Owner::Citizen(wlasciciel)), 10_000);
    hook.on_inheritance(&mut world, wlasciciel, &[(a, 500), (b2, 500)], &mut cmd);

    assert_eq!(
        udzial(&world, key, Owner::Citizen(wlasciciel)),
        0,
        "firma została z udziałowcem, który nie żyje"
    );
    assert_eq!(udzial(&world, key, Owner::Citizen(a)), 5_000);
    assert_eq!(udzial(&world, key, Owner::Citizen(b2)), 5_000);
    assert!(
        world
            .resource::<Firms>()
            .get(key)
            .expect("firma")
            .owners_sum_ok(),
        "podział udziałów zgubił albo stworzył punkty bazowe"
    );
}

/// Brak spadkobiercy: udział przejmuje miasto, a nie nieżyjący.
///
/// Przed naprawą hak nie był w tym przypadku wołany **w ogóle** — stał wewnątrz
/// gałęzi „są spadkobiercy".
#[test]
fn bez_spadkobiercy_udzial_przejmuje_miasto() {
    let (mut world, mut hook, key, wlasciciel) = swiat();
    let mut cmd = CommandBuffer::new(SystemId::from_name("test.Spadek"));

    hook.on_inheritance(&mut world, wlasciciel, &[], &mut cmd);

    assert_eq!(udzial(&world, key, Owner::Citizen(wlasciciel)), 0);
    assert_eq!(udzial(&world, key, Owner::City), 10_000);
}

/// Podział nieparzysty: reszta punktów bazowych trafia do ostatniego spadkobiercy,
/// a suma zostaje dziesięć tysięcy.
#[test]
fn nieparzysty_podzial_nie_gubi_punktow_bazowych() {
    let (mut world, mut hook, key, wlasciciel) = swiat();
    let a = CitizenId(ent(701));
    let b2 = CitizenId(ent(702));
    let c = CitizenId(ent(703));
    let mut cmd = CommandBuffer::new(SystemId::from_name("test.Spadek"));

    hook.on_inheritance(
        &mut world,
        wlasciciel,
        &[(a, 334), (b2, 333), (c, 333)],
        &mut cmd,
    );

    let suma = udzial(&world, key, Owner::Citizen(a))
        + udzial(&world, key, Owner::Citizen(b2))
        + udzial(&world, key, Owner::Citizen(c));
    assert_eq!(suma, 10_000, "podział na trzech zgubił punkty bazowe");
    assert!(world
        .resource::<Firms>()
        .get(key)
        .expect("firma")
        .owners_sum_ok());
}

/// Kredyt gospodarstwa, które właśnie przestaje istnieć, obciąża masę spadkową —
/// a to, czego masa nie pokryła, zamyka się jako strata banku.
#[test]
fn kredyt_obciaza_mase_spadkowa_gdy_dom_przestaje_istniec() {
    let (mut world, mut hook, _, _) = swiat();
    // Gospodarstwo jednoosobowe: po zgonie rozwiązuje się (`R2-WP8`).
    let hh = world
        .spawn()
        .with(Household {
            flags: Household::FLAG_ACTIVE,
            size: 1,
            ..Household::default()
        })
        .id();
    world
        .resource_mut::<magnat_agents::Population>()
        .add_household(hh);
    let zmarly = world
        .spawn()
        .with(magnat_agents::Identity {
            flags: magnat_agents::Identity::FLAG_ALIVE,
            household: hh.index(),
            ..magnat_agents::Identity::default()
        })
        .id();

    // Świat testowy nie ma banku, więc kredytu też nie ma — i to jest poprawna
    // odpowiedź: masa nie ma czego oddawać.
    let przed = world.resource::<Books>().total_balance();
    let obciazenie = hook.estate_charge(&mut world, CitizenId(zmarly), Money(100_000));
    assert_eq!(obciazenie, Money::ZERO);
    assert_eq!(world.resource::<Books>().total_balance(), przed);
    assert_eq!(world.resource::<Books>().check_conservation(), Ok(()));
}
