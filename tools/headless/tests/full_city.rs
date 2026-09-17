//! Pełne miasto: rynek, firmy, praca i makro w jednym świecie (M7f WP15, WP17).
//!
//! Trzy pytania, na które ta podfaza musi umieć odpowiedzieć liczbą, a nie opisem:
//! czy kapitał sieci zewnętrznej przechodzi przez rejestr emisji, czy powstawanie
//! firm się zatrzymuje, i czy każda wykonana akcja AI ma zapisany powód.
//!
//! `#[ignore]` z tego samego powodu co `labor_city.rs`: najmniejsze miasto to
//! kilkadziesiąt tysięcy mieszkańców, a doba pełnej gospodarki kosztuje sekundy.
//! CI uruchamia je jawnie przez `--include-ignored`.

use magnat_agents::{
    bootstrap_day, register_day, DayLoopSystem, DeprivationEffectsSystem, HouseholdStockSystem,
    NeedDecaySystem, NoInheritance, ReplanCooldownSystem, SkillDriftSystem, SocietySystem,
};
use magnat_economy::corpfin::system::InsolvencySystem;
use magnat_economy::firmlife::FirmLifeLog;
use magnat_economy::labor::LaborSystem;
use magnat_economy::{Books, MarketSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_firms::Firms;
use magnat_headless::full;
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_jobs::JobPool;
use magnat_macro::MacroSystem;
use magnat_traffic::TrafficSystem;

/// Miasto po `dni` dobach pełnej symulacji, razem z liczbą firm na starcie.
fn miasto(dni: u32, citizens: u32) -> (App, usize) {
    let pool = JobPool::new(0);
    let city = zbuduj_miasto(1, "4km", "lowland", "1990", "mixed", &pool).expect("miasto");
    let mut world = swiat_agentow(1).expect("świat");
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, citizens, 200_000).expect("Etap 8");
    full::setup(
        &mut world,
        &city,
        zaludnione.places.clone(),
        zaludnione.travel_oracle(),
        &zaludnione.traffic,
        1,
        &pool,
    )
    .expect("gospodarka");
    bootstrap_day(&mut world, 0);

    let mut b = ScheduleBuilder::new();
    b.add(magnat_supply::ChainSystem::new())
        .add(magnat_firms::systems::FirmSystem::new())
        .add(MarketSystem::new(&world))
        .add(LaborSystem::new())
        .add(InsolvencySystem::new())
        .add(MacroSystem::new())
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world));
    let schedule = b.build().expect("harmonogram");
    let firm_start = world.resource::<Firms>().len();
    let mut app = App::new(world, schedule, 0);
    for _ in 0..u64::from(dni) * 1440 {
        app.tick();
    }
    (app, firm_start)
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn firmy_rynek_i_makro_stoja_w_jednym_harmonogramie() {
    // To jest test, dla którego WP17 w ogóle istnieje: do M7f `Firms` i `Market`
    // nie stały razem w żadnym przebiegu, więc polityki i AI firm wykonywały się
    // wyłącznie w testach jednostkowych. Pierwsza próba zszycia **nie zbudowała
    // harmonogramu** — cykl przez pięć systemów wyłącznych (`K-53`).
    let (app, start) = miasto(3, 4_000);
    assert!(start > 0, "most nie postawił ani jednej firmy");
    let zycie = app
        .world
        .get_resource::<FirmLifeLog>()
        .copied()
        .expect("FirmLifeLog");
    assert!(
        zycie.ai.ops_firms > 0,
        "tier operacyjny nie podjął ani jednej decyzji — AI firm nie biegnie w przebiegu"
    );
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn kazda_akcja_ai_ma_zapisany_powod() {
    // §7.8: licznik decyzji == licznik powodów. Tutaj „>=", bo powody zapisują też
    // rynek pracy i finanse — a to jest właściwa nierówność: akcja **bez** powodu
    // obniżyłaby prawą stronę poniżej lewej i tylko to ma tu znaczenie.
    let (app, _) = miasto(3, 4_000);
    let zycie = app.world.resource::<FirmLifeLog>();
    let powody = app.world.resource::<Firms>().reasons_logged();
    assert!(
        powody >= zycie.ai.actions(),
        "akcje AI {} > powody zapisane {powody}",
        zycie.ai.actions()
    );
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn ksiegi_domykaja_sie_po_przebiegu_z_firmami() {
    // Niezmiennik P1 z warstwą firm: powstawanie firm przenosi pieniądz z komponentu
    // gospodarstwa na rachunek, a wejście sieci **tworzy** go przez rejestr emisji.
    // Obie drogi muszą zostawić księgi domknięte.
    let (app, _) = miasto(5, 4_000);
    let books = app.world.resource::<Books>();
    assert!(
        books.check_conservation().is_ok(),
        "księgi się nie domykają: {:?}",
        books.check_conservation()
    );
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn kapital_sieci_zewnetrznej_jest_zarejestrowana_emisja() {
    // **Kryterium ukończenia WP15.** Sieć wnosi pieniądz spoza miasta i jest to
    // jedyny taki punkt w tej fazie. Bez zapisu w rejestrze podaży globalny test
    // zachowania pieniądza pękłby na sumie — czyli daleko od przyczyny (`D10`).
    let (app, _) = miasto(35, 4_000);
    let zycie = app.world.resource::<FirmLifeLog>();
    let books = app.world.resource::<Books>();
    let zarejestrowany = books.supply().external_capital_in.get();
    assert_eq!(
        zarejestrowany,
        zycie.total.capital_in.get(),
        "kapitał sieci nie zgadza się z pozycją `external_capital_in` rejestru podaży"
    );
    if zycie.total.chain_entries > 0 {
        assert!(
            zarejestrowany > 0,
            "sieć weszła, a rejestr emisji tego nie widzi"
        );
    }
}
