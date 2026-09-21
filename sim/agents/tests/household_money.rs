//! Majątek gospodarstwa przy rozwiązaniu i przy podziale (`R2-WP8`, `K-61`).
//!
//! Każdy test w tym pliku padał przed naprawą — to jest warunek z `R2` §3 pkt 1.
//! Wspólna przyczyna jest jedna: `rozwiaz_gospodarstwo` nie dotykało ani `cash`,
//! ani `bank`, ani `savings`, a `zaloz_gospodarstwo` startowało od `Household::default()`,
//! więc pieniądz znikał razem z encją albo zostawał po niewłaściwej stronie.

use magnat_agents::{
    demography, household, migration, register, society, CityFacts, DemographyTable, HomeSlot,
    Household, HouseholdOverflow, Identity, JobSlot, NeedTable, NoInheritance, Population,
    Vacancies,
};
use magnat_core::{Entity, Money};
use magnat_ecs::World;

const ID_DOM: u32 = 1_000_000;
const BOK: u32 = 8;

fn swiat(seed: u64) -> World {
    let mut world = World::new(seed);
    register(&mut world, NeedTable::load_default().expect("data/needs"));
    society::register_society(
        &mut world,
        DemographyTable::load_default().expect("data/demography"),
    );
    let mut homes = Vec::new();
    let mut block_of = vec![0u32; (ID_DOM + BOK * BOK) as usize];
    for i in 0..BOK * BOK {
        homes.push(HomeSlot {
            building: ID_DOM + i,
            unit: 0,
            district: 0,
            value: Money(1_000_000),
        });
        block_of[(ID_DOM + i) as usize] = 0;
    }
    let jobs: Vec<JobSlot> = (0..16)
        .map(|i| JobSlot {
            site: 2_000_000 + i % 4,
            role: (i % 4) as u16,
            shift: 1,
            work_days: 0b001_1111,
            district: 0,
            wage_monthly: Money(250_000),
        })
        .collect();
    *world.resource_mut::<Vacancies>() = Vacancies::new(homes, jobs);
    *world.resource_mut::<CityFacts>() = CityFacts {
        job_prestige: Vec::new(),
        district_score: vec![50],
        block_of,
    };
    world
}

fn dom(i: u32) -> HomeSlot {
    HomeSlot {
        building: ID_DOM + i,
        unit: 0,
        district: 0,
        value: Money(1_000_000),
    }
}

/// Skład gospodarstwa jako encje.
fn sklad(world: &World, hh: Entity) -> Vec<Entity> {
    let h = world.get::<Household>(hh).copied().expect("skład");
    household::members_of(hh.index(), &h, world.resource::<HouseholdOverflow>())
        .iter()
        .filter_map(|m| demography::citizen_by_index(world, *m))
        .collect()
}

/// Wpisuje saldo do gospodarstwa: gotówka, rachunek i oszczędności.
fn zasil(world: &mut World, hh: Entity, cash: i64, bank: i64, savings: i64) {
    let h = world.get_mut::<Household>(hh).expect("gospodarstwo");
    h.cash = Money(cash);
    h.bank = Money(bank);
    h.savings = Money(savings);
}

/// Pozycja 16 wykazu: `rozwiaz_gospodarstwo` oddawało lokal i etat, a salda
/// despawnowało razem z encją.
///
/// Przed naprawą brakowało dokładnie 100 000 gr — i **nie widział tego żaden test**,
/// bo niezmiennik P1 nie obejmuje gospodarstw (`§5.4`), a `society::total_money`
/// liczy tylko gospodarstwa **żywe**.
#[test]
fn rozwiazane_gospodarstwo_nie_gubi_pieniedzy() {
    let mut world = swiat(29);
    let mut r = magnat_core::rng(
        world.seed,
        magnat_core::StreamId::Migration,
        0,
        magnat_core::Tick(0),
    );

    // Samotny dorosły: po jego zgonie gospodarstwo zostaje puste i rozwiązuje się.
    // Z dzieckiem w składzie rozwiązania by nie było — byłby opiekun (`R2-WP4`).
    let gd = migration::spawn_household_aged(&mut world, 0, &mut r, 1, 0, dom(0), 40, 1);
    let senior = sklad(&world, gd)[0];
    zasil(&mut world, gd, 30_000, 50_000, 20_000);

    let ages = world.resource::<DemographyTable>().ages();
    if let Some(id) = world.get_mut::<Identity>(senior) {
        id.birth_day = -(i32::from(ages.max) + 5) * 360;
    }

    let przed = society::total_money(&world);
    assert_eq!(
        przed, 100_000,
        "świat testowy nie ma 100 000 gr do zgubienia"
    );

    let mut hooks = NoInheritance;
    let mut d = 0;
    while d <= 360 {
        society::step_day(&mut world, d, &mut hooks);
        if world.get::<Identity>(senior).is_none_or(|i| !i.is_alive()) {
            society::step_day(&mut world, d + 1, &mut hooks);
            break;
        }
        d += 1;
    }
    assert!(
        world.get::<Identity>(senior).is_none_or(|i| !i.is_alive()),
        "senior nie umarł — test mierzyłby własny brak"
    );
    assert!(
        world.get::<Household>(gd).is_none()
            || !world.resource::<Population>().households().contains(&gd),
        "gospodarstwo nie zostało rozwiązane — test mierzyłby własny brak"
    );

    assert_eq!(
        society::total_money(&world),
        przed,
        "rozwiązanie gospodarstwa zgubiło pieniądze"
    );
}

/// Pozycja 19 wykazu, pierwsza połowa: wyprowadzka z gniazda.
///
/// `zaloz_gospodarstwo` tworzyło dom z `Household::default()`, więc
/// dwudziestopięciolatek zabierał wyłącznie własny portfel, a wspólne
/// oszczędności zostawały w całości po drugiej stronie.
#[test]
fn usamodzielnienie_zabiera_udzial_w_majatku() {
    let mut world = swiat(31);
    let mut r = magnat_core::rng(
        world.seed,
        magnat_core::StreamId::Migration,
        0,
        magnat_core::Tick(0),
    );

    let gniazdo = migration::spawn_household_aged(&mut world, 0, &mut r, 2, 0, dom(0), 40, 1);
    zasil(&mut world, gniazdo, 40_000, 40_000, 20_000);
    let przed = society::total_money(&world);

    let kto = sklad(&world, gniazdo)[1];
    let nowe = migration::zaloz_gospodarstwo(&mut world, kto, 0, migration::Czesci::Domownicy)
        .expect("pustostan");

    let stare = world.get::<Household>(gniazdo).copied().expect("gniazdo");
    let mlode = world.get::<Household>(nowe).copied().expect("nowe");

    assert_eq!(
        society::total_money(&world),
        przed,
        "podział przy wyprowadzce zgubił albo stworzył pieniądze"
    );
    assert!(
        mlode.cash.get() + mlode.bank.get() + mlode.savings.get() > 0,
        "wyprowadzający się wyszedł z pustymi rękami"
    );
    // Dwoje dorosłych, więc udział wynosi połowę — co do grosza po obu stronach.
    assert_eq!(
        mlode.cash.get() + mlode.bank.get() + mlode.savings.get(),
        50_000
    );
    assert_eq!(
        stare.cash.get() + stare.bank.get() + stare.savings.get(),
        50_000
    );
    // `D-N9`: dług zostaje po stronie gospodarstwa, które go zaciągnęło.
    assert_eq!(
        mlode.debt,
        Money::ZERO,
        "dług pojechał za wyprowadzającym się"
    );
}

/// Pozycja 19 wykazu, druga połowa: rozstanie.
///
/// Przed naprawą wyprowadzający się partner zakładał gospodarstwo z saldami zerowymi,
/// a wspólne oszczędności i wspólny dług zostawały po drugiej stronie w całości.
#[test]
fn rozstanie_dzieli_majatek_i_dlug_na_pol() {
    let mut world = swiat(37);
    let mut r = magnat_core::rng(
        world.seed,
        magnat_core::StreamId::Migration,
        0,
        magnat_core::Tick(0),
    );

    let para = migration::spawn_household_aged(&mut world, 0, &mut r, 2, 0, dom(0), 40, 1);
    zasil(&mut world, para, 33_333, 40_000, 26_667);
    if let Some(h) = world.get_mut::<Household>(para) {
        h.debt = Money(80_001);
    }
    let przed = society::total_money(&world);

    // Wyprowadza się ten z wyższym indeksem — tak samo jak w `rozstania`.
    let mut czlonkowie = sklad(&world, para);
    czlonkowie.sort_unstable_by_key(|e| e.index());
    let nowe =
        migration::zaloz_gospodarstwo(&mut world, czlonkowie[1], 0, migration::Czesci::Polowa)
            .expect("pustostan");
    // Ten sam ruch, który robi `rozstania` zaraz po założeniu gospodarstwa.
    let polowa = world
        .get_mut::<Household>(para)
        .map(Household::split_debt)
        .expect("stare");
    world.get_mut::<Household>(nowe).expect("nowe").debt = polowa;

    let stare = world.get::<Household>(para).copied().expect("stare");
    let mlode = world.get::<Household>(nowe).copied().expect("nowe");

    assert_eq!(
        society::total_money(&world),
        przed,
        "rozstanie zgubiło albo stworzyło pieniądze"
    );
    assert_eq!(
        stare.purse().total().get() + mlode.purse().total().get(),
        100_000
    );
    assert_eq!(
        stare.debt.get() + mlode.debt.get(),
        80_001,
        "podział długu nie sumuje się do kwoty wyjściowej"
    );
    assert!(mlode.debt.get() > 0, "cały dług został po jednej stronie");
}

// ── test własnościowy podziału ──────────────────────────────────────────────────

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(10_000))]

    /// Podział przy rozstaniu i przy usamodzielnieniu sumuje się do kwoty
    /// wyjściowej z tolerancją **zero groszy** — dla każdej kombinacji sald
    /// i każdego rozmiaru gospodarstwa.
    #[test]
    fn prop_podzial_majatku_sumuje_sie_co_do_grosza(
        cash in 0i64..1_000_000_000,
        bank in 0i64..1_000_000_000,
        savings in 0i64..1_000_000_000,
        czesci in 1u8..=8,
    ) {
        let mut h = Household {
            cash: Money(cash),
            bank: Money(bank),
            savings: Money(savings),
            ..Household::default()
        };
        let przed = h.purse().total();
        let udzial = h.split_off(czesci);
        proptest::prop_assert_eq!(
            h.purse().total().get() + udzial.total().get(),
            przed.get()
        );
        // Udział nigdy nie jest większy od całości ani ujemny.
        proptest::prop_assert!(udzial.total().get() >= 0);
        proptest::prop_assert!(udzial.total() <= przed);
    }
}
