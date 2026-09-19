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
    Terrain, TerrainQuery, WorldGenParams,
};
use std::sync::Arc;

fn miasto_i_teren(seed: u64) -> (CityData, Terrain) {
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
    let city = generate_city(&plan, &terrain, terrain.materials(), &pool).expect("miasto");
    (city, terrain)
}

fn miasto(seed: u64) -> CityData {
    miasto_i_teren(seed).0
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

/// Pieszy idzie **polilinią z grafu pieszego**, a nie prostą od środka budynku
/// do środka budynku (WP12 fazy M11c, pozycja 70 wykazu `R2`).
///
/// Do M11b `journey.rs::enter_micro_inner` podawał warstwie Mikro dwa punkty, więc
/// mieszkaniec przenikał przez kwartały, a w połowie drogi bywał pod ziemią albo nad nią.
/// Węzły grafu niosą **rzędną niwelety** (`RoadNode.z_dm`) i nikt ich w tej ścieżce
/// nie czytał. `G-13` z M11b wyprostowało końce trasy; ten test pilnuje środka.
///
/// **Odchylenie od terenu naturalnego nie jest zerem i być nie może** (`J-9`): droga jest
/// w przekroju podłużnym cięciwą między swoimi końcami, a różnicę wobec terenu pokrywa
/// nasyp albo wykop. Mierzalne jest co innego — że pieszy trzyma się nawierzchni, czyli
/// że jego odchylenie mieści się w robotach ziemnych i jest **wyraźnie mniejsze** niż
/// odchylenie odcinka prostego, który po prostu wisi nad doliną.
#[test]
fn pieszy_idzie_ulica_a_nie_przez_kwartal() {
    let (city, terrain) = miasto_i_teren(7);
    let mut world = swiat(7);
    let p = generate_population(&mut world, &city, &PopulationParams::default()).expect("Etap 8");
    let oracle: &TrafficOracle = &p.traffic;

    let miejsca: Vec<magnat_agents::PlaceEntry> = p.places.entries().to_vec();
    assert!(miejsca.len() > 100, "za mało miejsc do próbkowania");

    /// Największe roboty ziemne, jakie M2 dopuszcza pod jezdnią. Powyżej tego pieszy
    /// nie stoi na nasypie, tylko wisi w powietrzu.
    const ROBOTY_ZIEMNE_M: f32 = 5.0;

    // ponytail: kandydatów na budynek bierzemy z indeksu po prostokącie 240 m wokół
    // próbki. Sufit nazwany: budynek o półboku większym niż 120 m zostałby pominięty;
    // w mieście 4 km największa bryła ma ok. 60 m.
    let mut kandydaci = Vec::new();
    let mut w_obrysie = |x: f32, y: f32| {
        city.buildings.index.query_rect(
            magnat_spatial::Aabb2 {
                min: glam::Vec2::new(x - 120.0, y - 120.0),
                max: glam::Vec2::new(x + 120.0, y + 120.0),
            },
            &mut kandydaci,
        );
        kandydaci.iter().any(|b| {
            let bud = &city.buildings.buildings[b.0.index() as usize];
            punkt_w_wielokacie(x, y, city.roads.geom.get(bud.footprint))
        })
    };

    let mut tras = 0u32;
    let mut zalamane = 0u32;
    let mut przez_budynek = 0u32;
    let mut prosta_przez_budynek = 0u32;
    let mut max_blad = 0.0f32;
    let mut max_blad_prostej = 0.0f32;

    for i in 0..120usize {
        let a = miejsca[(i * 37) % miejsca.len()];
        let b = miejsca[(i * 101 + 13) % miejsca.len()];
        if a.at == b.at {
            continue;
        }
        let trasa = oracle.walk_polyline(a.at, b.at, magnat_core::MinuteOfDay::new(8 * 60));
        tras += 1;
        if trasa.len() > 2 {
            zalamane += 1;
        }

        // Środek trasy, czyli wszystko poza pierwszym i ostatnim odcinkiem: trasa
        // **zaczyna się i kończy w budynku** i to nie jest usterka.
        for punkt in trasa.iter().skip(1).take(trasa.len().saturating_sub(2)) {
            let (x, y) = (punkt.x as f32 / 100.0, punkt.y as f32 / 100.0);
            if w_obrysie(x, y) {
                przez_budynek += 1;
            }
            // `height_at` jest w jednostkach 0,5 m (`K-13`), `z` rekordu w centymetrach.
            let teren_m = terrain.height_at(punkt.x / 100, punkt.y / 100) as f32 * 0.5;
            max_blad = max_blad.max((punkt.z as f32 / 100.0 - teren_m).abs());
        }

        // Ta sama para odcinkiem prostym — czyli to, co warstwa Mikro dostawała do M11b.
        let n = 40;
        let mut trafiony = false;
        for k in 1..n {
            let t = k as f32 / n as f32;
            let x = a.at.x as f32 + (b.at.x - a.at.x) as f32 * t;
            let y = a.at.y as f32 + (b.at.y - a.at.y) as f32 * t;
            let z = a.at.z as f32 + (b.at.z - a.at.z) as f32 * t;
            if !trafiony && w_obrysie(x / 100.0, y / 100.0) {
                prosta_przez_budynek += 1;
                trafiony = true;
            }
            let teren_m = terrain.height_at((x / 100.0) as i32, (y / 100.0) as i32) as f32 * 0.5;
            max_blad_prostej = max_blad_prostej.max((z / 100.0 - teren_m).abs());
        }
    }

    println!(
        "tras {tras}, załamanych {zalamane}, punktów w obrysie budynku {przez_budynek};          odchylenie od terenu: trasa {max_blad:.2} m, prosta {max_blad_prostej:.2} m;          prostych przez budynek {prosta_przez_budynek}"
    );
    assert!(tras > 50, "za mało tras do oceny");
    // Bez tych dwóch liczb test przechodziłby również przed naprawą — próbka musi
    // trafiać w zabudowę i w teren, inaczej niczego nie odróżnia.
    assert!(
        prosta_przez_budynek * 4 > tras,
        "próbka nie odtwarza usterki: proste między tymi parami prawie nie tykają zabudowy"
    );
    assert!(
        max_blad_prostej > 2.0 * ROBOTY_ZIEMNE_M,
        "próbka nie odtwarza usterki: proste nie odrywają się od terenu"
    );
    assert!(
        zalamane * 2 > tras,
        "większość tras nadal jest odcinkiem prostym — polilinia z grafu nie działa"
    );
    assert_eq!(
        przez_budynek, 0,
        "trasa piesza przechodzi przez obrys budynku"
    );
    assert!(
        max_blad <= ROBOTY_ZIEMNE_M && max_blad < max_blad_prostej,
        "trasa piesza odrywa się od nawierzchni: {max_blad:.2} m wobec {max_blad_prostej:.2} m prostej"
    );
}

/// Punkt w wielokącie — zliczanie przecięć promienia. Obrys budynku jest wypukły
/// albo prawie, ale reguła parzystości działa i bez tego założenia.
fn punkt_w_wielokacie(x: f32, y: f32, poly: &[magnat_spatial::Vec2]) -> bool {
    let mut w = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > y) != (b.y > y) {
            let t = (y - a.y) / (b.y - a.y);
            if x < a.x + t * (b.x - a.x) {
                w = !w;
            }
        }
    }
    w
}
