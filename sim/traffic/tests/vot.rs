//! `R2-WP13`: wartość czasu idzie za dochodem gospodarstwa.
//!
//! Pozycja 4 wykazu `R2`. `TrafficOracle::set_incomes` było wołane **dokładnie raz**,
//! przy zasiedlaniu świata, i tablica dochodów zostawała migawką z generacji. Mieszkaniec,
//! który po pięciu latach awansował, stracił pracę albo ją znalazł, wyceniał swoją
//! godzinę tak, jak w dniu powstania świata — a wartość czasu waży największy składnik
//! kosztu uogólnionego, czyli rozstrzyga o wyborze środka transportu w **każdej** podróży.
//!
//! Świat jest tu najmniejszy, jaki wystarcza: cztery węzły drogowe, jedno gospodarstwo,
//! jeden mieszkaniec. Usterka siedzi w tym, że nikt tablicy nie odświeża — do jej pokazania
//! nie trzeba metropolii.

use magnat_agents::{society, DemographyTable, Household, Identity, NeedTable, Population, Vitals};
use magnat_core::{DistrictId, IVec2, Mass, Money, RoadClass};
use magnat_ecs::{App, ScheduleBuilder, World};
use magnat_nav::{EdgeSpec, GeomRef, Modality, NavGraphs, NavRouter, RoadGraphBuilder};
use magnat_traffic::{
    TrafficOracle, TrafficServices, TrafficSystem, TripPurpose, VdfTable, VehicleCatalog,
};
use std::sync::Arc;

/// Kwadrat czterech węzłów — router potrzebuje grafu, a nie miasta.
fn siatka() -> NavGraphs {
    let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
    let n: Vec<_> = [(0, 0), (100_000, 0), (100_000, 100_000), (0, 100_000)]
        .iter()
        .map(|(x, y)| b.add_node(IVec2::new(*x, *y), 0))
        .collect();
    for i in 0..4 {
        b.add_edge(EdgeSpec {
            from: n[i],
            to: n[(i + 1) % 4],
            geometry_ref: GeomRef(0),
            length_cm: 100_000,
            lanes: 2,
            class: RoadClass::Collector,
            speed_limit_dkmh: 500,
            max_mass: Mass::ZERO,
            grade_permille: 0,
            bridge: None,
            curb_parking: 0,
            district: DistrictId(0),
        });
    }
    let pusty = |m: Modality| RoadGraphBuilder::new(m).with_turns(false).finish();
    NavGraphs::new(
        [
            b.finish(),
            pusty(Modality::Foot),
            pusty(Modality::Bike),
            pusty(Modality::Rail),
        ],
        Vec::new(),
    )
}

/// Świat z jednym gospodarstwem o zadanym dochodzie miesięcznym i z zainstalowanym
/// ruchem. Zwraca świat i uchwyt do oracle'a.
fn swiat(dochod: Money) -> (World, Arc<TrafficOracle>, u32) {
    let mut world = World::new(7);
    magnat_agents::register(&mut world, NeedTable::load_default().expect("data/needs"));
    society::register_society(
        &mut world,
        DemographyTable::load_default().expect("data/demography"),
    );

    let hh = world
        .spawn()
        .with(Household {
            flags: Household::FLAG_ACTIVE,
            income_monthly: dochod,
            ..Household::default()
        })
        .id();
    world.resource_mut::<Population>().add_household(hh);
    let c = world
        .spawn()
        .with(Identity {
            birth_day: -30 * 360,
            flags: Identity::FLAG_ALIVE,
            household: hh.index(),
            ..Identity::default()
        })
        .with(Vitals::default())
        .id();
    world.resource_mut::<Population>().add_citizen(c);

    let router = NavRouter::build(siatka(), 1, 1 << 10);
    let catalog = Arc::new(VehicleCatalog::load_default().expect("data/vehicles/classes.ron"));
    let mut oracle = TrafficOracle::new(
        Arc::new(magnat_agents::places::PlaceTable::build(Vec::new())),
        router,
        catalog,
        Vec::new(),
        Vec::new(),
    );
    oracle.set_seed(7);
    // Tak, jak robi to zasiedlenie świata: tablica dochodów wypełniona raz, na starcie.
    oracle.set_incomes(magnat_traffic::household_incomes(&world));
    let oracle = Arc::new(oracle);
    let vdf = VdfTable::load_default().expect("data/roads/vdf.ron");
    let network = oracle.with_road(|road| magnat_traffic::TrafficNetwork::new(road, &vdf));
    magnat_traffic::register_traffic(
        &mut world,
        TrafficServices {
            oracle: Arc::clone(&oracle),
            vdf,
            fleet: Vec::new(),
            transit_drivers: Vec::new(),
        },
        network,
    );
    (world, oracle, hh.index())
}

fn doba(world: World) -> World {
    let schedule = {
        let mut b = ScheduleBuilder::new();
        b.add(TrafficSystem::new(&world));
        b.build().expect("harmonogram")
    };
    let mut app = App::new(world, schedule, 1);
    app.run_ticks(1_440);
    app.world
}

/// Kryterium `R2-WP13`: mieszkaniec, którego dochód wzrósł, ma **po dobie** wyższą
/// wartość czasu. Przed naprawą jest identyczna, bo tablicy nie odświeża nikt.
#[test]
fn awans_podnosi_wartosc_czasu_w_ciagu_doby() {
    let (mut world, oracle, hh) = swiat(Money(300_000));
    let przed = oracle.vot_gr_per_min(hh, TripPurpose::Work);
    assert!(przed > 0, "gospodarstwo bez wartości czasu: {przed}");

    // Awans: dochód rośnie trzykrotnie. Pisze go rynek pracy (`przesun_dochod`),
    // tu wystarczy sam skutek — pakiet naprawia to, że nikt tej zmiany nie czyta.
    world
        .get_mut::<Household>(
            magnat_agents::demography::household_by_index(&world, hh).expect("gospodarstwo"),
        )
        .expect("gospodarstwo")
        .income_monthly = Money(900_000);

    let _ = doba(world);
    let po = oracle.vot_gr_per_min(hh, TripPurpose::Work);
    assert!(
        po > przed,
        "wartość czasu nie drgnęła po awansie: {przed} → {po}"
    );
}

/// Odwrotna strona tej samej usterki: utrata pracy też ma być widoczna.
#[test]
fn utrata_pracy_obniza_wartosc_czasu() {
    let (mut world, oracle, hh) = swiat(Money(900_000));
    let przed = oracle.vot_gr_per_min(hh, TripPurpose::Work);

    world
        .get_mut::<Household>(
            magnat_agents::demography::household_by_index(&world, hh).expect("gospodarstwo"),
        )
        .expect("gospodarstwo")
        .income_monthly = Money::ZERO;

    let _ = doba(world);
    let po = oracle.vot_gr_per_min(hh, TripPurpose::Work);
    assert!(
        po < przed,
        "wartość czasu nie drgnęła po utracie pracy: {przed} → {po}"
    );
}
