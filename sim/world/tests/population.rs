//! Etap 8 i doba w zaludnionym mieście — testy §7.4 fazy M3 (podfaza M3d, WP10 i WP13).
//!
//! Miasto jest **prawdziwe**: generator M2 na małej mapie, a nie siatka testowa.
//! Dopasowania statystyczne nie mają sensu na atrapie — mierzą, czy generator potrafi
//! ułożyć ludzi w mieście, które ktoś inny wygenerował, a nie czy zgadza się sam ze sobą.
//!
//! Pełną macierz (10 ziaren × 4 rozmiary) puszcza runner `headless population`; tutaj
//! jest jedno ziarno i najmniejsza mapa, żeby CI nie czekał minuty na każdy test.
//!
//! **Cztery z siedmiu jadą na 2 km i chodzą przy każdym `cargo test`** (`R2-WP25`).
//! Trzy zostają na 4 km i mają wiersz w `D-R7` (`R1-refaktor-po-M5.md`) z nazwą joba
//! nocnego: dwa mierzą statystykę populacji (2 km daje 5 102 mieszkańców, czyli za
//! małą próbkę), trzeci wywraca się na `debug_assert` kolejki zdarzeń w profilu
//! testowym — poz. 73 wykazu `R2`, i to jest jego prawdziwy powód, a nie rozmiar mapy.
//!
//! Uruchomienie tych trzech: `cargo test --release -p magnat-world --test population -- --include-ignored`.

use magnat_agents::{
    bootstrap_day, register, register_day, society, AgentSources, DayLoopSystem, DayStats,
    DemographyTable, DeprivationEffectsSystem, Employment, HouseholdStockSystem, Identity,
    InfinitePlaces, KnowledgeRef, NeedDecaySystem, NeedTable, NoInheritance, Population,
    ReplanCooldownSystem, Residence, SkillDriftSystem, SocietySystem, Trace, Vacancies,
    MAX_WATCHED,
};
use magnat_ecs::{App, ScheduleBuilder, World};
use magnat_jobs::JobPool;
use magnat_voxel::MaterialRegistry;
use magnat_world::{
    generate, generate_city, generate_population, home_place, CityData, CityPlan, Difficulty,
    EconomyProfile, Epoch, Populated, PopulationParams, PopulationReport, Region, Terrain,
    WorldGenParams, WorldSize,
};
use std::sync::Arc;

/// Rozmiar testowy 2 km (`R2-WP25`, `D-N18`) — ta sama struktura co 4 km,
/// ułamek czasu generacji. Dzięki temu testy populacji chodzą przy każdym
/// `cargo test`, a nie tylko wtedy, gdy ktoś poda `--include-ignored`.
const ROZMIAR: WorldSize = WorldSize::Km2;

fn miasto(seed: u64) -> CityData {
    miasto_w(seed, ROZMIAR)
}

/// Miasto o zadanym rozmiarze — dla trzech testów, którym 2 km nie wystarcza,
/// bo mierzą **statystykę populacji**, a nie strukturę miasta (`D-R7` w R1).
fn miasto_w(seed: u64, size: WorldSize) -> CityData {
    let pool = JobPool::new(0);
    let params = WorldGenParams {
        seed,
        size,
        region: Region::Lowland,
        epoch: Epoch::Y1990,
        profile: EconomyProfile::Mixed,
        difficulty: Difficulty::Normal,
    };
    let (data, _) = generate(params, &pool).expect("generacja świata");
    let reg = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path("materials")).unwrap());
    let t = Terrain::new(data, reg);
    let plan = CityPlan::from_world(&params);
    generate_city(&plan, &t, t.materials(), &pool).expect("generacja miasta")
}

fn swiat(seed: u64) -> World {
    let mut w = World::new(seed);
    register(&mut w, NeedTable::load_default().expect("data/needs/"));
    society::register_society(
        &mut w,
        DemographyTable::load_default().expect("data/demography/"),
    );
    register_day(&mut w);
    w
}

fn zaludnij(world: &mut World, city: &CityData) -> Populated {
    generate_population(
        world,
        city,
        &PopulationParams {
            // Liczba mieszkańców **wynika z miasta** (§5.9): tyle etatów, ilu aktywnych.
            target_population: None,
            unemployment_target_permille: None,
            commute_median_min: None,
            commute_swaps: Some(50_000),
        },
    )
    .expect("Etap 8")
}

// ── §7.4: cztery dopasowania statystyczne ───────────────────────────────────────

#[test]
#[ignore = "dopasowania statystyczne wymagaja 4 km — job nocny `determinism`, `D-R7` (R1)"]
fn etap8_spelnia_dopasowania_statystyczne() {
    let city = miasto_w(1, WorldSize::Small4km);
    let mut world = swiat(1);
    let r: PopulationReport = zaludnij(&mut world, &city).report;

    // `gen_everyone_has_home`: 100 % mieszkańców ma lokal.
    assert_eq!(r.homeless, 0, "mieszkańcy bez lokalu");
    assert!(r.citizens > 10_000, "za mała populacja: {}", r.citizens);

    // `gen_unemployment`: |bezrobocie − cel| ≤ 1 pp.
    let d = i32::from(r.unemployment_permille) - i32::from(r.unemployment_target_permille);
    assert!(
        d.abs() <= 10,
        "bezrobocie {} ‰ wobec celu {} ‰",
        r.unemployment_permille,
        r.unemployment_target_permille
    );

    // `gen_jobs_filled`: |JobSlot| ≈ |aktywni| × (1 + cel), odchylenie ≤ 2 %.
    let oczekiwane =
        u64::from(r.active) * u64::from(1000 + u32::from(r.unemployment_target_permille)) / 1000;
    let odchylenie =
        (i64::from(r.jobs_total) - oczekiwane as i64).abs() as f64 * 100.0 / oczekiwane as f64;
    assert!(
        odchylenie <= 2.0,
        "etatów {} wobec oczekiwanych {oczekiwane} ({odchylenie:.1} %)",
        r.jobs_total
    );

    // `gen_age_pyramid`: χ² wobec piramidy epoki, 21 stopni swobody, próg dla
    // **p = 0,001** (korekta H-22): przy α = 0,05 i macierzy 40 przebiegów dwa
    // fałszywe alarmy są wartością oczekiwaną, więc kryterium „p > 0,05 w każdym
    // przebiegu" jest niespełnialne dla poprawnego generatora.
    let chi = r.pyramid_chi2();
    assert!(chi < 46.8, "piramida wieku: χ² = {chi:.1}");

    // `gen_income_housing`: korelacja rang Spearmana ≥ 0,60.
    assert!(
        r.income_housing_rho_centi >= 60,
        "ρ = {:.2}",
        f64::from(r.income_housing_rho_centi) / 100.0
    );

    // `gen_commute_hist`: odchylenie mediany ≤ 5 % celu (przyciętego do tego,
    // co geometria miasta dopuszcza przy ruchu wyłącznie pieszym — korekta H-2).
    let od = (i32::from(r.commute_median_min) - i32::from(r.commute_target_min)).abs() as f64
        * 100.0
        / f64::from(r.commute_target_min.max(1));
    assert!(
        od <= 5.0,
        "mediana dojazdu {} min wobec celu {} min ({od:.1} %)",
        r.commute_median_min,
        r.commute_target_min
    );

    // `gen_skill_fit`: ≥ 70 % zatrudnionych ma dopasowanie ≥ 40.
    assert!(
        r.skill_fit_permille >= 700,
        "dopasowanie {} ‰",
        r.skill_fit_permille
    );

    // Uczeń bez szkoły nie jest odprowadzany i nie ma szkoły w planie dnia.
    assert_eq!(r.pupils_without_school, 0);
    assert!(r.pupils > 0, "miasto bez uczniów");
}

#[test]
fn etap8_jest_deterministyczny() {
    // `gen_determinism`: ten sam seed → identyczna populacja co do bajtu. Porównujemy
    // hash stanu ECS, a nie raport — raport jest podsumowaniem, hash jest stanem.
    let city = miasto(2);
    let mut a = swiat(2);
    let ra = zaludnij(&mut a, &city).report;
    let mut b = swiat(2);
    let rb = zaludnij(&mut b, &city).report;

    assert_eq!(ra.citizens, rb.citizens);
    assert_eq!(ra.pyramid, rb.pyramid);
    assert_eq!(ra.commute_hist, rb.commute_hist);
    assert_eq!(
        magnat_io::world_state_hash(&a),
        magnat_io::world_state_hash(&b),
        "dwa przebiegi tego samego ziarna dały różne światy"
    );
}

#[test]
#[ignore = "zasiew wiedzy mierzony na 4 km — job nocny `determinism`, `D-R7` (R1)"]
fn etap8_daje_agentom_wszystko_czego_potrzebuja() {
    // Kontrakt z korekt E-1, E-2, E-6, E-7 i E-14: bez tych czterech rzeczy `sim/agents`
    // nie ma jak zobaczyć miasta i scenariusz `m3day` pokazuje miasto stojące w miejscu.
    let city = miasto_w(3, WorldSize::Small4km);
    let mut world = swiat(3);
    let p = zaludnij(&mut world, &city);

    assert!(!p.places.is_empty(), "pusty katalog miejsc (E-1)");
    // Po M4b sieci pieszej nie ma w `Populated` (`Z-3`) — jest w `engine/nav`,
    // a Etap 8 dostaje ją przez oracle ruchu. Sprawdzamy to, co z tego wynika:
    // miasto ma flotę, stacje i router z pojemnością wyliczoną z liczby dojeżdżających.
    assert!(
        p.fleet.vehicles > 0,
        "miasto bez ani jednego samochodu (M4b/WP5)"
    );
    assert!(
        p.fleet.route_cache_capacity >= 1_024,
        "pojemność cache tras nie została wyliczona z populacji (Y-4)"
    );
    assert!(
        !p.traffic.drivers().is_empty(),
        "flota jest, ale nikt nie ma prawa jazdy"
    );

    let v = world.resource::<Vacancies>();
    assert_eq!(
        v.homes_total(),
        p.report.homes_total,
        "pojemność mieszkaniowa (E-14)"
    );
    assert_eq!(
        v.jobs_total(),
        p.report.jobs_total,
        "pojemność rynku pracy (E-14)"
    );
    assert!(
        v.free_homes() > 0,
        "miasto bez pustostanów nie przyjmie nikogo"
    );

    let facts = world.resource::<magnat_agents::CityFacts>();
    assert!(
        !facts.job_prestige.is_empty(),
        "brak prestiżu zawodów (E-14)"
    );
    assert!(
        !facts.block_of.is_empty(),
        "brak przypisania budynek → kwartał (E-14)"
    );

    // Każdy mieszkaniec ma dom jako `PlaceRef` (E-6).
    let ludzie: Vec<_> = world.resource::<Population>().citizens().to_vec();
    for c in ludzie.iter().take(2000) {
        let res = world.get::<Residence>(*c).expect("Residence");
        let dom = home_place(res).expect("dom jako PlaceRef");
        assert!(
            p.places.coord_of(dom).is_some(),
            "dom spoza katalogu miejsc"
        );
        assert!(
            world.get::<KnowledgeRef>(*c).is_some(),
            "brak uchwytu wiedzy"
        );
    }

    // Zasiew wiedzy (E-2) jest warunkiem, żeby cokolwiek się wydarzyło: `candidates`
    // zwraca **wyłącznie** miejsca znane mieszkańcowi. Kto mieszka na obrzeżu bez
    // sklepu w promieniu zasiewu, ten zna tylko własną pracę — i to jest treść §5.7,
    // a nie błąd. Mierzymy **udział**, bo jeden procent to przedmieście, a połowa to
    // miasto stojące w miejscu (korekta H-20).
    let udzial = f64::from(p.report.without_knowledge) * 100.0 / f64::from(p.report.citizens);
    assert!(
        udzial < 5.0,
        "{udzial:.1} % mieszkańców nie zna żadnego miejsca — zasiew wiedzy nie działa"
    );
}

// ── §5.12: doba przez systemy ECS ───────────────────────────────────────────────

/// Buduje świat z miastem, populacją i harmonogramem systemów M3d.
fn gotowy_swiat(seed: u64) -> (App, u32) {
    // Jedyny wołający to test doby, który zostaje na 4 km (poz. 73 wykazu R2).
    let city = miasto_w(seed, WorldSize::Small4km);
    let mut world = swiat(seed);
    let p = zaludnij(&mut world, &city);
    let ludzi = p.report.citizens;
    let tabela = Arc::new(NeedTable::load_default().expect("data/needs/"));
    *world.resource_mut::<AgentSources>() = AgentSources::new(
        Box::new(InfinitePlaces::new(p.places.clone(), tabela)),
        p.travel_oracle(),
    );
    bootstrap_day(&mut world, 0);

    let mut b = ScheduleBuilder::new();
    b.add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)));
    let s = b.build().expect("harmonogram");
    (App::new(world, s, 0), ludzi)
}

#[test]
#[ignore = "poz. 73 wykazu R2: `debug_assert` kolejki zdarzen wywraca dobe w profilu testowym — job nocny `determinism`, `D-R7` (R1)"]
fn doba_przez_systemy_ecs_planuje_dowozi_i_zaspokaja() {
    let (mut app, ludzi) = gotowy_swiat(4);
    for _ in 0..1440 {
        app.tick();
    }
    let s = *app.world.resource::<DayStats>();

    assert_eq!(s.plans, u64::from(ludzi), "nie każdy dostał plan doby");
    assert!(s.trips > 0, "nikt nigdzie nie poszedł");
    assert_eq!(s.arrivals, s.trips, "nie każda podróż się skończyła");
    assert!(
        s.fulfilled > u64::from(ludzi),
        "mniej niż jedna wizyta na mieszkańca: {}",
        s.fulfilled
    );
    // `InfinitePlaces` nigdy nie odmawia — odmowa znaczyłaby, że atrapa przestała
    // być atrapą (ścieżkę odmowy testuje `FlakyPlaces` w M3b).
    assert_eq!(s.refused, 0);

    // Praca jest zobowiązaniem stałym: w dobie roboczej większość zatrudnionych ma
    // ją w planie. Zero znaczyłoby, że coś zatrzymało miasto (korekta H-3).
    let pracujacy = app
        .world
        .resource::<Population>()
        .citizens()
        .iter()
        .filter(|e| {
            app.world
                .get::<Employment>(**e)
                .is_some_and(magnat_agents::Employment::has_job)
        })
        .count();
    assert!(pracujacy > 0);
    let z_praca = app
        .world
        .resource::<Population>()
        .citizens()
        .iter()
        .filter(|e| {
            app.world
                .get::<magnat_agents::PlanRef>(**e)
                .is_some_and(|p| {
                    magnat_agents::load_plan(p, app.world.resource::<magnat_agents::PlanSlab>())
                        .iter()
                        .any(|s| s.kind == magnat_core::ActivityKind::Work as u8)
                })
        })
        .count();
    assert!(
        z_praca * 2 > pracujacy,
        "tylko {z_praca} z {pracujacy} zatrudnionych ma pracę w planie"
    );
}

#[test]
fn wynik_doby_nie_zalezy_od_liczby_watkow() {
    // 00 §3.3: równolegle biegną wyłącznie systemy o rozłącznych dostępach, więc wynik
    // jest funkcją stanu, a nie kolejności ukończenia jobów.
    let hash = |watki: usize| {
        let city = miasto(5);
        let mut world = swiat(5);
        let p = zaludnij(&mut world, &city);
        let tabela = Arc::new(NeedTable::load_default().unwrap());
        *world.resource_mut::<AgentSources>() = AgentSources::new(
            Box::new(InfinitePlaces::new(p.places.clone(), tabela)),
            p.travel_oracle(),
        );
        bootstrap_day(&mut world, 0);
        let mut b = ScheduleBuilder::new();
        b.add(DayLoopSystem::new(&world))
            .add(ReplanCooldownSystem::new(&world))
            .add(NeedDecaySystem::new(&world))
            .add(DeprivationEffectsSystem::new(&world));
        let mut app = App::new(world, b.build().unwrap(), watki);
        for _ in 0..600 {
            app.tick();
        }
        magnat_io::world_state_hash(&app.world)
    };
    assert_eq!(hash(1), hash(8));
}

#[test]
fn bufor_sledzenia_trzyma_najwyzej_osmiu() {
    // Decyzja 9.16: pełne logi decyzji dla 400 tys. mieszkańców to 14 GB na rok gry.
    let mut t = Trace::new();
    for i in 0..(MAX_WATCHED as u32 + 4) {
        let ok = t.watch(i);
        assert_eq!(ok, (i as usize) < MAX_WATCHED, "mieszkaniec {i}");
    }
    assert_eq!(t.len(), MAX_WATCHED);
    // Powtórne zgłoszenie już śledzonego jest bezkosztowe i nie zajmuje miejsca.
    assert!(t.watch(0));
    assert_eq!(t.len(), MAX_WATCHED);
    t.unwatch(0);
    assert!(t.watch(100), "zwolnione miejsce ma się dać zająć");
}

#[test]
fn mieszkaniec_ma_tozsamosc_wieku_z_piramidy() {
    // Piramida wieku jest **zużywana do zera** przy składaniu gospodarstw (§5.9 krok 2):
    // mieszkaniec zgubiony pod koniec pętli byłby dziurą, której test χ² nie odróżni
    // od błędu rozkładu.
    let city = miasto(6);
    let mut world = swiat(6);
    let r = zaludnij(&mut world, &city).report;
    let z_piramidy: u32 = r.pyramid.iter().sum();
    assert_eq!(
        z_piramidy, r.citizens,
        "piramida nie sumuje się do populacji"
    );

    // Nikt nie żyje dłużej, niż pozwala tabela demografii (`prop_no_immortals`).
    let ages = world.resource::<DemographyTable>().ages();
    for c in world.resource::<Population>().citizens() {
        let wiek = world.get::<Identity>(*c).map_or(0, |i| i.age_years(0));
        assert!(
            (0..=i32::from(ages.max)).contains(&wiek),
            "wiek {wiek} poza zakresem 0..={}",
            ages.max
        );
    }
}
