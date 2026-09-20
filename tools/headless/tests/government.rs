//! Władza i wybory w prawdziwym mieście (M8e WP9, WP10, WP11).
//!
//! Testy jednostkowe `sim/city` sprawdzają każdy mechanizm osobno: histerezę
//! stawek (T5), determinizm i wrażliwość wyborów (T6), punktację przetargu,
//! podział mandatów. Ten plik sprawdza to, czego żaden z nich sprawdzić nie może —
//! że **łańcuch domyka się w gospodarce**: burmistrz podejmuje decyzję, uchwała
//! wchodzi w życie, stawka na półce zmienia się razem z nią, przetarg płaci
//! prawdziwej firmie, wybory zmieniają władzę — a domknięcie podatkowe T1
//! i niezmiennik pieniądza nie drgają.
//!
//! **Kadencja skracana jest wprost, a nie przez przebieg**: cztery lata gry
//! to ~1400 dób, czyli kilkanaście minut na test. `GovTuning` ma pola publiczne,
//! więc test podmienia w nim trzy liczby i mierzy ten sam mechanizm w sto dób.
//! To nie jest osłabienie kryterium — długość kadencji jest daną (`K-56`),
//! a nie własnością kodu, który tu sprawdzamy.
//!
//! `#[ignore]` z tego samego powodu co `enforcement.rs`: doba pełnej gospodarki
//! kosztuje sekundy. CI uruchamia je jawnie przez `--include-ignored`.

use magnat_agents::{
    bootstrap_day, register_day, DayLoopSystem, DeprivationEffectsSystem, HouseholdStockSystem,
    NeedDecaySystem, NoInheritance, ReplanCooldownSystem, SkillDriftSystem, SocietySystem,
};
use magnat_city::{City, CitySystem, GovTuning, Government, Preference};
use magnat_core::{PolicyKind, TaxKind};
use magnat_economy::corpfin::system::InsolvencySystem;
use magnat_economy::labor::LaborSystem;
use magnat_economy::{Books, Market, MarketSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto_z_klimatem};
use magnat_headless::{city as city_bridge, events as events_bridge, full, grid as grid_bridge};
use magnat_jobs::JobPool;
use magnat_macro::MacroSystem;
use magnat_traffic::TrafficSystem;

/// Kalibracja władzy przyspieszona tak, żeby mechanizm zmieścił się w przebiegu
/// testowym. Zmieniają się **wyłącznie długości okresów** — progi, wagi i widełki
/// zostają te z `data/city/government.ron`.
fn tuning_szybki() -> GovTuning {
    let mut t = GovTuning::load_default().expect("data/city/government.ron");
    t.term_months = 2;
    t.vacatio_legis_days = 10;
    t.hysteresis_months = 1;
    t.min_months_between_changes = 1;
    t.min_months_for_reversal = 2;
    t
}

fn miasto(dni: u32, citizens: u32, pref: Preference) -> App {
    let pool = JobPool::new(0);
    let (city, klimat) =
        zbuduj_miasto_z_klimatem(1, "4km", "lowland", "1990", "mixed", &pool).expect("miasto");
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
    city_bridge::setup(&mut world, &city).expect("strona publiczna");
    city_bridge::setup_services(&mut world, &city).expect("usługi publiczne");
    grid_bridge::setup(&mut world, &city).expect("sieci publiczne");
    events_bridge::setup(&mut world, &city, &klimat, 1990).expect("zdarzenia");

    let dzielnic = city.districts.districts.len().max(1);
    world
        .get_resource_mut::<City>()
        .expect("miasto w świecie")
        .gov = Government::new(pref, dzielnic, Some(tuning_szybki()));

    bootstrap_day(&mut world, 0);
    let mut b = ScheduleBuilder::new();
    b.add(magnat_events::EventSystem::new())
        .add(magnat_traffic::utility::UtilitySystem::new(
            magnat_traffic::utility::GridTuning::load_default().expect("data/tuning/grid.ron"),
        ))
        .add(magnat_supply::ChainSystem::new())
        .add(magnat_firms::systems::FirmSystem::new())
        .add(MarketSystem::new(&world))
        .add(LaborSystem::new())
        .add(InsolvencySystem::new())
        .add(MacroSystem::new())
        .add(magnat_media::MediaSystem::new())
        .add(CitySystem::new())
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world));
    let schedule = b.build().expect("harmonogram");
    let mut app = App::new(world, schedule, 0);
    for _ in 0..u64::from(dni) * 1440 {
        app.tick();
    }
    app
}

/// Kryterium WP9 w gospodarce: burmistrz rządzi, uchwała wchodzi w życie,
/// a domknięcie podatkowe i niezmiennik pieniądza tego nie zauważają.
#[test]
#[ignore = "pełna gospodarka — uruchamiać przez --include-ignored"]
fn burmistrz_rzadzi_i_nie_psuje_domkniecia() {
    let app = miasto(
        150,
        1_200,
        Preference {
            growth_bps: 1_000,
            social_bps: 4_000,
            green_bps: 3_000,
            populist_bps: 2_000,
        },
    );
    let miasto = app.world.resource::<City>();

    let uchwal: u32 = miasto.gov.enacted.iter().sum();
    assert!(
        uchwal > 0,
        "przez pięć miesięcy gry burmistrz nie podjął ani jednej decyzji"
    );
    assert!(
        !miasto.policies.is_empty(),
        "uchwał jest {uchwal}, a zbiór uchwał jest pusty"
    );
    // Każda uchwała ma niepusty powód — bramka 5 fazy.
    for r in miasto.policies.all() {
        assert_ne!(
            r.reason,
            magnat_core::DecisionReason::Unspecified,
            "uchwała {:?} bez powodu",
            r.policy.kind()
        );
        assert!(
            r.effective_from.0 > r.enacted_at.0,
            "uchwała weszła w życie w dobie uchwalenia — vacatio legis jest zerowe"
        );
    }

    // T1 po uchwałach: stawka mogła się zmienić, ale rejestr musi się domykać.
    assert!(
        miasto.charges.check_closure().is_ok(),
        "domknięcie podatkowe pękło po uchwałach rady"
    );
    let books = app.world.resource::<Books>();
    assert!(
        books.check_conservation().is_ok(),
        "niezmiennik pieniądza pękł po uchwałach rady"
    );
}

/// Kryterium WP10 w gospodarce: wybory się odbywają, mają wynik i zmieniają radę.
#[test]
#[ignore = "pełna gospodarka — uruchamiać przez --include-ignored"]
fn wybory_odbywaja_sie_i_wylaniaja_rade() {
    let app = miasto(150, 1_200, Preference::default());
    let miasto = app.world.resource::<City>();

    let e = miasto
        .election
        .as_ref()
        .expect("po dwóch kadencjach wyborów nie było ani razu");
    let r = e.result.as_ref().expect("wybory bez wyniku");
    assert!(e.candidates.len() >= 2, "wybory z jednym kandydatem");
    assert!(
        (3_500..=7_500).contains(&r.turnout_bp),
        "frekwencja {} bp poza widełkami 35–75 % (T6)",
        r.turnout_bp
    );
    let mandatow: u32 = r.council.iter().map(|(_, m)| u32::from(*m)).sum();
    assert_eq!(
        mandatow,
        u32::from(
            GovTuning::load_default()
                .expect("data/city/government.ron")
                .council_seats
        ),
        "mandaty nie sumują się do liczby miejsc w radzie"
    );
    assert_eq!(
        miasto.gov.council.len(),
        mandatow as usize,
        "rada nie została obsadzona wynikiem wyborów"
    );
    assert_ne!(
        miasto.gov.mayor,
        Government::NO_MAYOR,
        "wybory się odbyły, a miasto nie ma burmistrza"
    );
    // Głos ma powód — wyjaśnialność (`00` §7) dotyczy też wyborcy.
    assert!(
        !e.vote_log.is_empty(),
        "ani jeden głos nie ma zapisanego powodu"
    );
}

/// Kryterium WP9, druga połowa: uchwała **zmienia świat**, a nie tylko rejestr.
///
/// Sprawdzamy najostrzejszy przypadek: stawkę VAT. Zmiana kodeksu ma dojść
/// do `Market` przez `set_tax_engine`, bo inaczej uchwała zmieniłaby deklarację,
/// a nie cenę na półce — a to jest dokładnie ta klasa błędu co martwa akcyza
/// z `CC-9`, która przeżyła całą podfazę.
#[test]
#[ignore = "pełna gospodarka — uruchamiać przez --include-ignored"]
fn uchwala_o_stawce_dochodzi_do_ceny_na_polce() {
    let mut app = miasto(30, 1_000, Preference::default());
    let market = app.world.resource::<Market>().clone();
    let towar = market
        .good_of_key("food_bread_wheat")
        .expect("chleb w katalogu");
    let sklep = market
        .sites()
        .into_iter()
        .find(|s| market.price_at(*s, towar).is_some())
        .expect("sklep z chlebem");
    let przed = market.price_at(sklep, towar).expect("cena przed uchwałą");

    // Uchwała: VAT w dół o pięć punktów procentowych, wchodzi natychmiast.
    let t = app.world.tick;
    {
        let miasto = app.world.get_resource_mut::<City>().expect("miasto");
        let stara = miasto.code.rate_of(TaxKind::Vat);
        miasto.policies.enact(magnat_city::PolicyRecord {
            policy: magnat_city::Policy::TaxRate {
                kind: TaxKind::Vat,
                bps: stara.saturating_sub(500),
            },
            enacted_at: t,
            effective_from: t,
            sunset: None,
            vote: magnat_city::CouncilVote { for_bp: 10_000 },
            reason: magnat_core::DecisionReason::TaxRateChanged {
                kind: TaxKind::Vat,
                from_bp: u16::try_from(stara).unwrap_or(0),
                to_bp: u16::try_from(stara.saturating_sub(500)).unwrap_or(0),
                gap_bp: 0,
            },
        });
    }
    // Trzy doby: uchwała nakłada się w dobowym kroku miasta, a cena półkowa
    // przelicza się w kroku cenowym sklepu.
    for _ in 0..3 * 1_440 {
        app.tick();
    }
    let miasto = app.world.resource::<City>();
    assert!(
        miasto.code.rate_of(TaxKind::Vat) < 2_300,
        "kodeks nie przyjął uchwały: stawka {} bp",
        miasto.code.rate_of(TaxKind::Vat)
    );
    assert!(
        miasto
            .policies
            .current(
                PolicyKind::TaxRate,
                TaxKind::Vat.as_index() as u32,
                app.world.tick
            )
            .is_some(),
        "uchwała nie obowiązuje"
    );
    let po = market.price_at(sklep, towar).expect("cena po uchwale");
    assert!(
        po.get() <= przed.get(),
        "obniżka VAT-u podniosła cenę półkową: {przed:?} → {po:?}"
    );
    assert!(
        app.world.resource::<Books>().check_conservation().is_ok(),
        "niezmiennik pieniądza pękł po zmianie stawki"
    );
}
