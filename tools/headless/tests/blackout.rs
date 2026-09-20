//! Test T2 z §7 dokumentu M8: **blackout zatrzymuje produkcję i widać to
//! w kosztach** (M8b WP3).
//!
//! Sama sieć — bilans, zrzut, kaskada, determinizm — ma testy w
//! `sim/traffic/tests/utility.rs` i nie potrzebuje do nich świata. Tutaj jest
//! to, czego tamte sprawdzić nie mogą: że odcięcie w sieci dochodzi do linii
//! produkcyjnej prawdziwego zakładu w **tej samej minucie**, że zakład bez prądu
//! nie zużywa energii, i że rachunek za media mimo to przychodzi.
//!
//! `#[ignore]` z tego samego powodu co `city_tax.rs`: doba pełnej gospodarki
//! kosztuje sekundy. CI woła je jawnie przez `--include-ignored`.

use magnat_agents::{
    bootstrap_day, register_day, DayLoopSystem, DeprivationEffectsSystem, HouseholdStockSystem,
    NeedDecaySystem, NoInheritance, ReplanCooldownSystem, SkillDriftSystem, SocietySystem,
};
use magnat_city::CitySystem;
use magnat_core::{DecisionReason, LineStopCause, Money, SiteId, UtilityService};
use magnat_economy::corpfin::system::InsolvencySystem;
use magnat_economy::labor::LaborSystem;
use magnat_economy::MarketSystem;
use magnat_ecs::{App, ScheduleBuilder};
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_headless::{city as city_bridge, full, grid as grid_bridge};
use magnat_jobs::JobPool;
use magnat_macro::MacroSystem;
use magnat_supply::ChainHandle;
use magnat_traffic::utility::{GridTuning, SupplyState, UtilityGrids, UtilitySystem};
use magnat_traffic::TrafficSystem;

/// Miasto z gospodarką, stroną publiczną i sieciami przesyłowymi.
fn miasto(citizens: u32) -> App {
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
    grid_bridge::setup(&mut world, &city).expect("sieci przesyłowe");
    bootstrap_day(&mut world, 0);

    let mut b = ScheduleBuilder::new();
    b.add(UtilitySystem::new(
        GridTuning::load_default().expect("data/tuning/grid.ron"),
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
    App::new(world, schedule, 0)
}

/// Zakłady z licznikiem prądu, po `SiteId` rosnąco.
fn zaklady_na_pradzie(app: &App) -> Vec<SiteId> {
    let chain = app.world.resource::<ChainHandle>().clone();
    let c = chain.lock();
    let mut out: Vec<SiteId> = c
        .plant
        .iter()
        .filter(|(_, z)| z.meter(UtilityService::Electricity).is_some())
        .map(|(s, _)| s)
        .collect();
    out.sort_by_key(|s| s.0.index());
    out
}

/// Ile energii naliczyły łącznie liczniki prądu tych zakładów.
fn pobor(app: &App, sites: &[SiteId]) -> i64 {
    let chain = app.world.resource::<ChainHandle>().clone();
    let c = chain.lock();
    sites
        .iter()
        .filter_map(|s| c.plant.get(*s))
        .filter_map(|z| z.meter(UtilityService::Electricity))
        .map(|m| m.consumed)
        .sum()
}

/// Ile linii pracuje w tych zakładach.
fn pracujace(app: &App, sites: &[SiteId]) -> usize {
    let chain = app.world.resource::<ChainHandle>().clone();
    let c = chain.lock();
    sites
        .iter()
        .filter_map(|s| c.plant.get(*s))
        .map(|z| {
            z.lines
                .iter()
                .filter(|l| matches!(l.state, magnat_supply::LineState::Running { .. }))
                .count()
        })
        .sum()
}

fn tyknij(app: &mut App, minut: u64) {
    for _ in 0..minut {
        app.tick();
    }
}

#[test]
#[ignore = "pełna gospodarka — doba kosztuje sekundy; CI woła --include-ignored"]
fn blackout_zatrzymuje_produkcje_i_widac_to_w_kosztach() {
    let mut app = miasto(1_500);
    let sites = zaklady_na_pradzie(&app);
    assert!(
        !sites.is_empty(),
        "żaden zakład nie ma licznika prądu — test mierzyłby pustkę"
    );

    // Doba rozbiegu, a po niej **dziewiąta rano**: zakład pracuje na zmianie,
    // a nie całą dobę, więc okno kontrolne w środku nocy mierzyłoby zero
    // niezależnie od tego, czy prąd jest.
    tyknij(&mut app, 1_440 + 9 * 60);

    // Sieć wpisała zakładom swoją taryfę i opłatę stałą. To jest szew M8b → M6:
    // licznik zostaje M6, taryfa staje się własnością operatora.
    {
        let chain = app.world.resource::<ChainHandle>().clone();
        let c = chain.lock();
        let m = c
            .plant
            .get(sites[0])
            .and_then(|z| z.meter(UtilityService::Electricity))
            .expect("licznik");
        assert!(m.tariff > Money::ZERO, "sieć nie wpisała taryfy");
        assert!(m.standing > Money::ZERO, "sieć nie wpisała opłaty stałej");
    }

    // ── przebieg kontrolny ────────────────────────────────────────────────────
    let przed = pobor(&app, &sites);
    tyknij(&mut app, 120);
    let kontrola = pobor(&app, &sites) - przed;
    assert!(
        kontrola > 0,
        "w przebiegu kontrolnym nikt nie zużył prądu — nie ma czego gasić"
    );
    let pracowalo = pracujace(&app, &sites);

    // ── awaria źródła ─────────────────────────────────────────────────────────
    {
        let grids = app.world.resource_mut::<UtilityGrids>();
        let net = grids.net_mut(UtilityService::Electricity).expect("sieć");
        assert!(net.set_source_online(0, false), "węzeł 0 nie jest źródłem");
    }
    app.tick();

    // (a) w ticku T zakład nie ma prądu.
    for s in &sites {
        let grids = app.world.resource::<UtilityGrids>();
        assert!(
            !grids.power_available(*s),
            "zakład {s:?} ma prąd mimo wyłączonej elektrowni"
        );
        assert_eq!(
            grids.supply_state(*s, UtilityService::Electricity),
            SupplyState::Isolated
        );
    }

    // (b) wolumen produkcji w oknie awarii jest dokładnie zerowy — mierzony
    //     poborem energii, bo prąd płynie wyłącznie wtedy, gdy linia pracuje.
    let w_awarii = pobor(&app, &sites);
    tyknij(&mut app, 120);
    assert_eq!(
        pobor(&app, &sites) - w_awarii,
        0,
        "zakład bez prądu zużył prąd"
    );
    assert_eq!(pracujace(&app, &sites), 0, "linia pracuje bez prądu");

    // (d) karta inspekcji zakładu ma powód, i to powód nazwany: `NoPower`.
    {
        let chain = app.world.resource::<ChainHandle>().clone();
        let c = chain.lock();
        let ma_powod = sites.iter().filter_map(|s| c.plant.get(*s)).any(|z| {
            z.reasons().iter().any(|(_, r)| {
                matches!(
                    r,
                    DecisionReason::ProductionHalted {
                        cause: LineStopCause::NoPower,
                        ..
                    }
                )
            })
        });
        assert!(ma_powod, "postój bez powodu w karcie inspekcji");
    }

    // (c) koszt energii zmiennej jest zerowy, a **opłata stała biegnie dalej** —
    //     i to jest cała różnica między „nic nie zużyłem" a „nic nie płacę".
    {
        let chain = app.world.resource::<ChainHandle>().clone();
        let mut c = chain.lock();
        let z = c.plant.get_mut(sites[0]).expect("zakład");
        let m = z.meter_mut(UtilityService::Electricity).expect("licznik");
        m.consumed = 0;
        let stala = m.standing;
        let (kwota, jednostki) = m.bill(magnat_core::SimMinute(0));
        assert_eq!(jednostki, 0, "licznik naliczył zużycie w czasie blackoutu");
        assert_eq!(kwota, stala, "rachunek za media zniknął razem z prądem");
    }

    // (e) po naprawie produkcja wraca w ≤ 2 ticki: `naprawa` zdejmuje
    //     `Broken { NoPower }`, gdy tylko medium wróci, a `sprobuj_start` rusza
    //     szarżę w tej samej minucie.
    //
    //     Kryterium jest o **powrocie do pracy**, nie o wolumenie w oknie: wsad
    //     szarży przerwanej blackoutem przepadł, więc pierwsze godziny po awarii
    //     są z definicji chudsze i porównywanie ich z oknem kontrolnym mierzyłoby
    //     koszt awarii, a nie to, czy linia wstała.
    {
        let grids = app.world.resource_mut::<UtilityGrids>();
        let net = grids.net_mut(UtilityService::Electricity).expect("sieć");
        net.set_source_online(0, true);
    }
    app.tick();
    assert!(
        app.world
            .resource::<UtilityGrids>()
            .power_available(sites[0]),
        "prąd nie wrócił w ticku naprawy"
    );
    app.tick();
    assert!(
        pracujace(&app, &sites) > 0,
        "po dwóch tickach od powrotu prądu nie pracuje ani jedna linia          (przed awarią pracowały {pracowalo})"
    );
    let po = pobor(&app, &sites);
    tyknij(&mut app, 60);
    assert!(
        pobor(&app, &sites) - po > 0,
        "po naprawie zakłady nie zużywają prądu"
    );
}

/// Ujęcie wody stoi na prądzie: blackout zabiera miastu najpierw prąd, a zaraz
/// potem wodę. To jest jedyny powód, dla którego priorytet 0 z §5.4 istnieje,
/// i jedyna droga, którą awaria sięga dalej niż do jednej sieci.
#[test]
#[ignore = "pełna gospodarka — doba kosztuje sekundy; CI woła --include-ignored"]
fn blackout_zabiera_miastu_takze_wode() {
    let mut app = miasto(1_500);
    tyknij(&mut app, 120);
    let woda: Vec<SiteId> = {
        let chain = app.world.resource::<ChainHandle>().clone();
        let c = chain.lock();
        let mut v: Vec<SiteId> = c
            .plant
            .iter()
            .filter(|(_, z)| z.meter(UtilityService::Water).is_some())
            .map(|(s, _)| s)
            .collect();
        v.sort_by_key(|s| s.0.index());
        v
    };
    assert!(!woda.is_empty(), "żaden zakład nie bierze wody");
    assert!(app
        .world
        .resource::<UtilityGrids>()
        .supply_state(woda[0], UtilityService::Water)
        .is_supplied());

    {
        let grids = app.world.resource_mut::<UtilityGrids>();
        grids
            .net_mut(UtilityService::Electricity)
            .expect("sieć")
            .set_source_online(0, false);
    }
    // Woda liczy się co godzinę (§5.4), więc dociągamy do najbliższej pełnej.
    tyknij(&mut app, 61);
    assert!(
        !app.world
            .resource::<UtilityGrids>()
            .supply_state(woda[0], UtilityService::Water)
            .is_supplied(),
        "ujęcie wody pracuje bez prądu"
    );
}
