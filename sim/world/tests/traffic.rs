//! Most miasto → ruch (M4b): czy trasy, które router oddaje warstwie mezo, da się
//! przejechać krawędź po krawędzi.
//!
//! To jest test **kontraktu między M4a a M4b**, a nie testu routera: router liczy
//! trasę węzłową i nie patrzy na manewry skrętne (`J-8`), więc dopiero mezo dowiaduje
//! się, czy sekwencja krawędzi jest ciągła i czy któryś manewr nie jest zabroniony.

use magnat_agents::{register, society, DemographyTable, NeedTable};
use magnat_ecs::World;
use magnat_jobs::JobPool;
use magnat_traffic::TrafficOracle;
use magnat_voxel::MaterialRegistry;
use magnat_world::{
    generate, generate_city, generate_population, CityData, CityPlan, Difficulty, PopulationParams,
    Terrain, WorldGenParams,
};
use std::sync::Arc;

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

fn swiat(seed: u64) -> World {
    let mut w = World::new(seed);
    register(&mut w, NeedTable::load_default().expect("data/needs/"));
    society::register_society(
        &mut w,
        DemographyTable::load_default().expect("data/demography/"),
    );
    w
}

/// Każda trasa oddana przez router musi być **ciągłą sekwencją krawędzi bez
/// zabronionych manewrów**. Inaczej warstwa mezo kończy przejazd porażką w połowie
/// drogi, a M4 §7.1 wymaga, żeby niewykonalność objawiała się przy planowaniu.
#[test]
fn trasy_samochodowe_sa_ciagle_i_bez_zakazanych_manewrow() {
    let city = miasto(7);
    let mut world = swiat(7);
    let p = generate_population(&mut world, &city, &PopulationParams::default()).expect("Etap 8");
    let oracle: &TrafficOracle = &p.traffic;

    let miejsca: Vec<magnat_core::PlaceRef> = p.places.entries().iter().map(|e| e.place).collect();
    assert!(miejsca.len() > 100, "za mało miejsc do próbkowania");

    let mut tras = 0u32;
    let mut bez_trasy = 0u32;
    let mut nieciagle = 0u32;
    let mut zakazane = 0u32;

    // Trasy najpierw, oględziny potem: `with_road` trzyma zamek routera, a `car_route`
    // bierze ten sam zamek — `std::sync::Mutex` nie jest wznawialny.
    let mut trasy: Vec<Vec<magnat_nav::EdgeId>> = Vec::new();
    for i in 0..200usize {
        // Pary deterministyczne: co 37. miejsce wobec co 101. — bez RNG, bo test ma
        // być powtarzalny bez wiązania się ze strumieniem losowym.
        let a = miejsca[(i * 37) % miejsca.len()];
        let b = miejsca[(i * 101 + 13) % miejsca.len()];
        match oracle.car_route(a, b, magnat_core::MinuteOfDay::new(8 * 60)) {
            Some(route) => {
                tras += 1;
                trasy.push(
                    route
                        .legs
                        .iter()
                        .flat_map(|l| l.edges.iter().copied())
                        .collect(),
                );
            }
            None => bez_trasy += 1,
        }
    }

    oracle.with_road(|road| {
        for edges in &trasy {
            for w in edges.windows(2) {
                let (cur, next) = (w[0], w[1]);
                if road.edge(cur).to != road.edge(next).from {
                    nieciagle += 1;
                }
                if road
                    .turns_from(cur)
                    .iter()
                    .any(|t| t.out_edge == next && t.banned)
                {
                    zakazane += 1;
                }
            }
        }
    });

    println!(
        "tras {tras}, bez trasy {bez_trasy}, nieciągłych przejść {nieciagle}, \
         zabronionych manewrów {zakazane}"
    );
    assert!(tras > 100, "router nie oddał ani stu tras");
    assert_eq!(
        nieciagle, 0,
        "trasa zawiera przejście między rozłącznymi krawędziami"
    );
    assert_eq!(
        zakazane, 0,
        "trasa zawiera manewr oznaczony jako zabroniony — mezo zakończy ją porażką"
    );
}
