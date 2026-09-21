//! **Dowód spójności mikro ↔ mezo** (M4d §5.4 i §7.2, WP9).
//!
//! Kontrakt z dok. 00 §4 brzmi: *wynik ekonomiczny nie zależy od poziomu LOD, tolerancja 0.*
//! Zbiór encji w warstwie Mikro zależy od kamery gracza, a kamera nie wchodzi do hasha
//! stanu — więc gdyby Mikro wpływało na cokolwiek, obrócenie kamery zmieniałoby salda
//! gospodarstw domowych i determinizm (00 §3) przestałby obowiązywać.
//!
//! **A1–A5 są prawdziwe z konstrukcji, nie przez kalibrację**: obie ścieżki wołają tę
//! samą `settle_edge` na tym samym, zamrożonym `LinkState` (`N-2`), a Mikro nie ma
//! deklaracji zapisu do niczego poza własnym buforem (`micro_writes_nothing`, `N-3`).
//! Rolą tego testu jest **bycie strażnikiem regresji**: pierwsza osoba, która „dla
//! realizmu" podepnie mikro pod ekonomię, zobaczy czerwony CI.
//!
//! Scenariusz odniesienia jest miastem **wygenerowanym**, a nie syntetyczną siatką
//! 20×20 z §5.4: to ta sama ścieżka kodu, tylko z sygnalizacją, rondami i korkami,
//! które naprawdę powstały, a nie takimi, które ktoś wpisał do scenariusza.

use magnat_agents::{
    bootstrap_day, register, register_day, society, AgentSources, DayLoopSystem, DemographyTable,
    DeprivationEffectsSystem, HouseholdStockSystem, InfinitePlaces, NeedDecaySystem, NeedTable,
    NoInheritance, ReplanCooldownSystem, SkillDriftSystem, SocietySystem, TravelMicroSystem,
};
use magnat_core::Money;
use magnat_ecs::{App, ScheduleBuilder, World};
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use magnat_traffic::{TrafficNetwork, TrafficServices, TrafficSystem, VehicleWearSystem};
use magnat_voxel::MaterialRegistry;
use magnat_world::{
    generate, generate_city, generate_population, CityData, CityPlan, Difficulty, PopulationParams,
    Terrain, WorldGenParams,
};
use std::sync::Arc;

/// Do godziny 9:00. Scenariusz §5.4 mówi „3 godziny gry" i **to za mało, żeby test
/// cokolwiek dowiódł**: doba zaczyna się o północy, więc trzy godziny to noc, w której
/// nikt nie jedzie. Porównywalibyśmy dwa puste przebiegi i oba byłyby zgodne.
/// Stąd bramka na niepustość kadru — kryterium, które samo się spełnia, nie jest bramką.
const MINUT: u64 = 540;
const HASH_CO: u64 = 60;

fn miasto(seed: u64) -> CityData {
    let pool = JobPool::new(2);
    let params = WorldGenParams {
        seed,
        size: "4km".parse().expect("rozmiar"),
        region: "lowland".parse().expect("region"),
        epoch: "1990".parse().expect("epoka"),
        profile: "mixed".parse().expect("profil"),
        difficulty: Difficulty::Normal,
    };
    params.validate().expect("parametry");
    let (data, _) = generate(params, &pool).expect("świat");
    let reg = Arc::new(
        MaterialRegistry::load_dir(&magnat_world::data_path("materials")).expect("materiały"),
    );
    let terrain = Terrain::new(data, reg);
    let plan = CityPlan::from_world(&params);
    generate_city(&plan, &terrain, terrain.materials(), &pool).expect("miasto")
}

/// Wynik przebiegu: wszystko, co ma być identyczne niezależnie od LOD.
struct Przebieg {
    hashe: Vec<String>,
    gotowka: i64,
    paliwo_spalone_ul: i64,
    paliwo_kupione_ul: i64,
    wydane_na_paliwo: Money,
    przybycia: u64,
    minut_podrozy: u64,
    bilety: Money,
    pojazdow_w_mikro: usize,
}

/// `kamera` dostaje minutę i środek miasta w metrach, a oddaje pozycję okna LOD Mikro.
/// `None` = warstwa wyłączona (tryb `ForceMezo`).
fn przebieg(seed: u64, kamera: impl Fn(u64, (i32, i32)) -> Option<(i32, i32)>) -> Przebieg {
    let city = miasto(seed);
    let mut world = World::new(seed);
    register(&mut world, NeedTable::load_default().expect("data/needs/"));
    society::register_society(
        &mut world,
        DemographyTable::load_default().expect("data/demography/"),
    );
    register_day(&mut world);
    let p = generate_population(&mut world, &city, &PopulationParams::default()).expect("Etap 8");
    let tabela = Arc::new(NeedTable::load_default().expect("data/needs/"));
    *world.resource_mut::<AgentSources>() = AgentSources::new(
        Box::new(InfinitePlaces::new(p.places.clone(), tabela)),
        p.travel_oracle(),
    );
    bootstrap_day(&mut world, 0);

    // Harmonogram jest **ten sam** w obu przebiegach, łącznie z systemem Mikro:
    // gdyby różnił się liczbą systemów, różniłby się odciskiem i porównywalibyśmy
    // dwa różne światy zamiast dwóch poziomów szczegółu tego samego.
    let mut b = ScheduleBuilder::new();
    b.add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world))
        .add(VehicleWearSystem::new(&world))
        .add(TravelMicroSystem::new(&world));
    let schedule = b.build().expect("harmonogram");
    let mut app = App::new(world, schedule, 0);

    let oracle = app.world.resource::<TrafficServices>().oracle.clone();
    // Środek kadru bierze się z miasta, a nie ze stałej: mapa 4 km nie musi mieć
    // początku układu w rogu, a okno postawione obok miasta dałoby pusty kadr
    // i test, który przechodzi, bo niczego nie porównał.
    let centrum = {
        let e = p.places.entries();
        let n = e.len().clamp(1, 512);
        let mut sx = 0i64;
        let mut sy = 0i64;
        for wpis in e.iter().take(n) {
            let c = p
                .places
                .coord_of(wpis.place)
                .unwrap_or(magnat_core::WorldCoord::ORIGIN);
            sx += i64::from(c.x);
            sy += i64::from(c.y);
        }
        ((sx / n as i64 / 100) as i32, (sy / n as i64 / 100) as i32)
    };

    let mut hashe = Vec::new();
    for t in 0..MINUT {
        oracle.micro().set_window(kamera(t, centrum), 4_000);
        app.tick();
        if (t + 1).is_multiple_of(HASH_CO) {
            hashe.push(world_state_hash(&app.world).to_string());
        }
    }

    let net = app.world.resource::<TrafficNetwork>();
    let net_stats = net.stats;
    // Pieniądz wydany na dojazdy zszedł w `R2-WP32` z rejestrów ruchu do ksiąg
    // (`K-72`). Ten świat ksiąg nie ma, więc kwoty czekają w `MobilityDue` —
    // i to jest ta sama liczba, co przedtem w rejestrze, tylko po drodze do konta.
    let due = *app.world.resource::<magnat_core::MobilityDue>();
    Przebieg {
        hashe,
        gotowka: society::total_money(&app.world),
        paliwo_spalone_ul: net_stats.fuel_burned_ul,
        paliwo_kupione_ul: net_stats.fuel_bought_ul,
        wydane_na_paliwo: due.pending(magnat_core::MobilityChannel::Fuel),
        przybycia: net_stats.arrived,
        minut_podrozy: net_stats.actual_minutes_total,
        bilety: due.pending(magnat_core::MobilityChannel::TransitTicket),
        pojazdow_w_mikro: oracle.micro().vehicles(),
    }
}

/// Kamera nieruchoma w środku miasta — promień 4 km obejmuje je w całości,
/// więc jest to **najgorszy możliwy przypadek** dla tezy „mikro niczego nie zmienia".
fn srodek(_: u64, c: (i32, i32)) -> Option<(i32, i32)> {
    Some(c)
}

/// A1–A4: `ForceMezo` vs `ForceMicro`, tolerancja 0 na wszystkim, co jest pieniądzem,
/// paliwem albo minutą przybycia.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn micro_mezo_equivalence() {
    let mezo = przebieg(11, |_, _| None);
    let mikro = przebieg(11, srodek);

    assert!(
        mikro.pojazdow_w_mikro > 0,
        "przebieg „ForceMicro\" nie wpuścił do kadru ani jednego pojazdu — \
         test porównałby wtedy dwa identyczne przebiegi mezo i nie dowiódłby niczego"
    );
    assert_eq!(mezo.paliwo_spalone_ul, mikro.paliwo_spalone_ul, "A1 paliwo");
    assert_eq!(
        mezo.paliwo_kupione_ul, mikro.paliwo_kupione_ul,
        "A1 tankowanie"
    );
    assert_eq!(
        mezo.wydane_na_paliwo, mikro.wydane_na_paliwo,
        "A2 obrót stacji"
    );
    assert_eq!(mezo.bilety, mikro.bilety, "A2 taryfy komunikacji");
    assert_eq!(mezo.przybycia, mikro.przybycia, "A3 liczba przybyć");
    assert_eq!(mezo.minut_podrozy, mikro.minut_podrozy, "A3 minuty podróży");
    assert_eq!(mezo.gotowka, mikro.gotowka, "A4 suma gotówki gospodarstw");
    assert_eq!(mezo.hashe, mikro.hashe, "A4 ciąg hashy stanu ECS");
}

/// A5: **kamera nie zmienia świata.** Skryptowana ścieżka przełącza LOD w środku
/// podróży — co dwadzieścia minut okno skacze w inne miejsce miasta albo gaśnie.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn camera_does_not_change_world() {
    let mezo = przebieg(11, |_, _| None);
    let z_kamera = przebieg(11, |t, c| match (t / 20) % 4 {
        0 => None,
        1 => Some(c),
        2 => Some((c.0 + 800, c.1 - 500)),
        _ => Some((c.0 - 600, c.1 + 900)),
    });

    assert_eq!(
        mezo.hashe, z_kamera.hashe,
        "ścieżka kamery zmieniła ciąg hashy stanu — LOD Mikro zapisuje do symulacji"
    );
    assert_eq!(mezo.gotowka, z_kamera.gotowka, "kamera zmieniła salda");
    assert_eq!(
        mezo.paliwo_spalone_ul, z_kamera.paliwo_spalone_ul,
        "kamera zmieniła zużycie paliwa"
    );
}
