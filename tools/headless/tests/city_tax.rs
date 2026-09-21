//! Miasto jako aktor fiskalny w prawdziwej gospodarce (M8a WP1, WP2).
//!
//! Domknięcie rejestru sprawdza `sim/city/tests/closure.rs` na losowych ciągach —
//! ten test sprawdza to, czego tamten sprawdzić nie może: że daniny naliczają się
//! **z faktów gospodarki**, a nie z wywołań testu. Trzy pytania:
//!
//! 1. czy konto miasta mieści się w niezmienniku pieniądza, czy go rozrywa;
//! 2. czy siedem danin ma źródło — czyli czy któraś jest martwa, i **która**;
//! 3. czy `Σ Settled` w rejestrze równa się wpływom budżetu co do grosza.
//!
//! `#[ignore]` z tego samego powodu co `full_city.rs`: doba pełnej gospodarki
//! kosztuje sekundy, a deklaracja miesięczna wypada po trzydziestu dobach.
//! CI uruchamia je jawnie przez `--include-ignored`.

use magnat_agents::{
    bootstrap_day, register_day, DayLoopSystem, DeprivationEffectsSystem, HouseholdStockSystem,
    NeedDecaySystem, NoInheritance, ReplanCooldownSystem, SkillDriftSystem, SocietySystem,
};
use magnat_city::{City, CitySystem, TaxPayer};
use magnat_core::{CityReason, Money, TaxKind};
use magnat_economy::corpfin::system::InsolvencySystem;
use magnat_economy::labor::LaborSystem;
use magnat_economy::{Books, MarketSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_headless::{city as city_bridge, full};
use magnat_jobs::JobPool;
use magnat_macro::MacroSystem;
use magnat_traffic::TrafficSystem;

/// Miasto z pełną gospodarką i stroną publiczną po `dni` dobach.
fn miasto(dni: u32, citizens: u32) -> App {
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
    city_bridge::setup(&mut world, &city).expect("strona publiczna");
    bootstrap_day(&mut world, 0);

    let mut b = ScheduleBuilder::new();
    b.add(magnat_supply::ChainSystem::new())
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

#[test]
#[ignore = "pełna gospodarka — doba kosztuje sekundy; CI woła --include-ignored"]
fn budzet_miasta_domyka_sie_do_grosza() {
    // 35 dób, bo pierwsza deklaracja miesięczna wypada trzydziestego pierwszego,
    // a termin VAT-u dwudziestego miesiąca następnego. Krócej znaczyłoby sprawdzać
    // sam rejestr, a ten ma własny test.
    let app = miasto(35, 3_000);
    let miasto = app.world.resource::<City>();

    // 1. Domknięcie rejestru na prawdziwych naliczeniach.
    miasto
        .charges
        .check_closure()
        .expect("Σ Assessed == Σ Settled + Σ Overdue + Σ Abated");

    // 2. Suma zapłaconych należności == wpływy budżetu, co do grosza.
    let mut zaplacone = Money::ZERO;
    for k in TaxKind::ALL {
        zaplacone = Money(zaplacone.get() + miasto.charges.settled_of(*k).get());
    }
    assert_eq!(
        zaplacone,
        miasto.budget.revenue_life_total(),
        "wpływy budżetu rozjechały się z rejestrem należności"
    );

    // 3. Pieniądz się zachowuje **razem z kontem miasta**. To jest kryterium
    //    ukończenia WP1 i jedyny powód, dla którego `CityBudget` nie ma pola `cash`.
    app.world
        .resource::<Books>()
        .check_conservation()
        .expect("Σ sald == podaż pieniądza");

    // 4. Daniny mają źródło. Trzy muszą być żywe po trzydziestu pięciu dobach;
    //    CIT i koncesja rozliczają się rocznie, a akcyza nie ma czego obłożyć
    //    w tym koszyku — patrz komentarz przy `--expect-taxes` w scenariuszu.
    for k in [TaxKind::Pit, TaxKind::Vat, TaxKind::Property] {
        let naliczone: i64 = miasto
            .charges
            .periods()
            .filter(|(kind, _, _)| *kind == k)
            .map(|(_, _, t)| t.assessed.get())
            .sum();
        assert!(naliczone > 0, "{} nie naliczyło się ani razu", k.name());
    }
}

#[test]
#[ignore = "pełna gospodarka — doba kosztuje sekundy; CI woła --include-ignored"]
fn kazde_obciazenie_ma_platnika_i_powod() {
    // Kryterium ukończenia WP2: obciążenie widoczne w karcie inspekcji firmy
    // **z nazwą, stawką i podstawą**. Kartę rysuje M9; tutaj sprawdzamy, że ma
    // z czego — czyli że każda należność zakładu niesie komplet.
    let app = miasto(35, 3_000);
    let miasto = app.world.resource::<City>();
    let zakladowe: Vec<_> = miasto
        .charges
        .iter()
        .filter(|c| matches!(c.payer, TaxPayer::Site(_)))
        .collect();
    assert!(
        !zakladowe.is_empty(),
        "ani jeden zakład nie dostał obciążenia — karta inspekcji byłaby pusta"
    );
    for c in &zakladowe {
        assert!(c.amount.get() > 0, "należność na zero nie powinna powstać");
        assert!(
            c.base.get() != 0 || c.base_mass.0 != 0,
            "{:?} bez podstawy — nie da się wytłumaczyć kwoty",
            c.kind
        );
        assert!(
            c.due_at.get() >= c.assessed_at.get(),
            "termin przed naliczeniem"
        );
        // Powód jest wyprowadzalny i niesie daninę oraz kwotę (00 §7).
        match c.reason() {
            magnat_core::DecisionReason::City(CityReason::TaxAssessed { kind, amount, .. }) => {
                assert_eq!(kind, c.kind);
                assert_eq!(amount, c.amount);
            }
            inny => panic!("należność opisana powodem {inny:?}"),
        }
    }
    // Karta firmy: obciążenia dają się wyjąć po zakładzie.
    let site = match zakladowe[0].payer {
        TaxPayer::Site(s) => s,
        _ => unreachable!("odfiltrowane wyżej"),
    };
    assert!(!miasto.charges_of_site(site).is_empty());
}
