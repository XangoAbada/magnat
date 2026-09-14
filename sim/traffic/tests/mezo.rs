//! Kryteria zamknięcia podfazy M4b: korek, bilans pojazdów i bilans paliwa.
//!
//! Testy chodzą **bez świata ECS** — `TrafficNetwork::step_minute` bierze zlecenia
//! i oddaje zdarzenia, a stan trzyma u siebie. To nie jest wygoda testu, tylko
//! własność architektury: zbiór rzeczy, które ruch zapisuje do symulacji, jest
//! wypisany w jednym miejscu (`systems::zastosuj`), więc da się go przejrzeć okiem.

use magnat_core::{DistrictId, IVec2, Mass, RoadClass, SimMinute};
use magnat_nav::{EdgeSpec, GeomRef, Modality, RoadGraph, RoadGraphBuilder, Route, RouteLeg};
use magnat_traffic::{
    PendingTrip, TrafficEvent, TrafficNetwork, TripId, TripPurpose, VdfTable, VehicleCatalog,
    VehicleClassId,
};
use std::sync::Arc;

/// Korytarz z przewężeniem: szeroka arteria, krótki jednopasmowy przejazd, szeroki wylot.
///
/// Przepustowość krawędzi wynika z diagramu podstawowego i jest niezależna od jej
/// długości: `pasy × prędkość / odstęp`. Dla jednego pasa przy 10 km/h to ~22 pojazdy
/// na minutę w ruchu swobodnym i ~11 przy gęstości korkowej — a arteria dwupasmowa
/// przy 60 km/h przepuszcza ich ~266. Zasilanie 25 pojazdami na minutę przeciąża więc
/// przewężenie ponad dwukrotnie i to na nim, a nie na dolocie, staje kolejka.
fn korytarz() -> (RoadGraph, Vec<magnat_nav::EdgeId>) {
    let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
    let a = b.add_node(IVec2::new(0, 0), 0);
    let c = b.add_node(IVec2::new(120_000, 0), 0);
    let d = b.add_node(IVec2::new(126_000, 0), 0);
    let e = b.add_node(IVec2::new(246_000, 0), 0);

    let spec = |from, to, length_cm, lanes, class, dkmh| EdgeSpec {
        from,
        to,
        geometry_ref: GeomRef(0),
        length_cm,
        lanes,
        class,
        speed_limit_dkmh: dkmh,
        max_mass: Mass::ZERO,
        grade_permille: 0,
        bridge: None,
        curb_parking: 0,
        district: DistrictId(0),
    };
    let e0 = b.add_edge(spec(a, c, 120_000, 2, RoadClass::Arterial, 600));
    let e1 = b.add_edge(spec(c, d, 6_000, 1, RoadClass::Service, 100));
    let e2 = b.add_edge(spec(d, e, 120_000, 2, RoadClass::Arterial, 600));
    (b.finish(), vec![e0, e1, e2])
}

fn trasa(edges: &[magnat_nav::EdgeId]) -> Arc<Route> {
    Arc::new(Route {
        mode: magnat_core::TransportMode::Car,
        legs: smallvec(RouteLeg {
            mode: magnat_core::TransportMode::Car,
            modality: Modality::Road,
            edges: edges.to_vec(),
            entry_transfer: None,
        }),
        planned_minutes: 10,
        planned_cost: magnat_core::Money::ZERO,
        cost_cs: 0,
    })
}

fn smallvec(leg: RouteLeg) -> smallvec::SmallVec<[RouteLeg; 4]> {
    let mut v = smallvec::SmallVec::new();
    v.push(leg);
    v
}

fn zlecenie(
    i: u32,
    route: &Arc<Route>,
    class: VehicleClassId,
    tank: i64,
    depart: u32,
) -> PendingTrip {
    PendingTrip {
        trip: TripId(i + 1),
        traveller: i,
        vehicle: i,
        household: i,
        class,
        slot: 0,
        origin: magnat_core::PlaceRef::default(),
        dest: magnat_core::PlaceRef::default(),
        depart: SimMinute(u64::from(depart)),
        planned_minutes: 10,
        purpose: TripPurpose::Work,
        load: Mass::ZERO,
        legs: vec![route.clone()],
        station: None,
        tank_level_ul: tank,
        tank_capacity_ul: tank,
        reason: magnat_core::DecisionReason::ModeChosen {
            mode: magnat_core::TransportMode::Car,
            minutes: 10,
        },
        traced: false,
    }
}

/// Kryterium WP3: „korek na przewężeniu powstaje i rozładowuje się; liczba pojazdów
/// w systemie = wjazdy − wyjazdy".
#[test]
fn korek_na_przewezeniu_powstaje_i_sie_rozladowuje() {
    /// Zasilanie: 25 pojazdów na minutę przez 16 minut. Górne ograniczenie bierze się
    /// z pojemności postojowej dolotu (320 pojazdów): kolejka ma się zmieścić na
    /// arterii, bo pojazd wpuszczany na sieć nie ma jak poczekać w bramie wyjazdowej.
    const NA_MINUTE: u32 = 25;
    const MINUT_ZASILANIA: u32 = 16;
    const POJAZDOW: u32 = NA_MINUTE * MINUT_ZASILANIA;

    let cat = VehicleCatalog::load_default().expect("data/vehicles/");
    let vdf = VdfTable::load_default().expect("data/roads/");
    let (road, edges) = korytarz();
    let mut net = TrafficNetwork::new(&road, &vdf);
    let route = trasa(&edges);
    let class = cat.by_key("car_small").expect("car_small");
    let most = edges[1].0 as usize;
    let dolot = edges[0].0 as usize;

    let swobodna_dolot = net.mezo.links[dolot].free_flow_dkmh;
    let mut zdarzenia = Vec::new();
    let mut przybyli = 0u64;
    let mut najwieksze_obciazenie = 0u16;
    let mut byl_spillback = false;
    let mut min_predkosc_dolotu = swobodna_dolot;

    for minuta in 0..600u32 {
        let pending: Vec<PendingTrip> = if minuta < MINUT_ZASILANIA {
            // Szczyt poranny w miniaturze.
            (0..NA_MINUTE)
                .map(|k| zlecenie(minuta * NA_MINUTE + k, &route, class, 40_000_000, minuta))
                .collect()
        } else {
            Vec::new()
        };

        zdarzenia.clear();
        net.step_minute(minuta, &road, &vdf, &cat, pending, &mut zdarzenia);

        // Niezmiennik zachowania — **na każdym ticku**, nie tylko na końcu.
        assert!(
            net.conserved(),
            "minuta {minuta}: {} na sieci wobec {} wjazdów − {} wyjazdów",
            net.mezo.vehicles_on_network(),
            net.stats.entries,
            net.stats.exits
        );

        najwieksze_obciazenie = najwieksze_obciazenie.max(net.mezo.queues[most].occupancy);
        byl_spillback |= net.stats.spillbacks > 0;
        min_predkosc_dolotu = min_predkosc_dolotu.min(net.mezo.links[dolot].mean_speed_dkmh);
        przybyli += zdarzenia
            .iter()
            .filter(|e| matches!(e, TrafficEvent::Arrived { .. }))
            .count() as u64;
    }

    let pojemnosc = net.mezo.queues[most].storage_capacity;
    assert!(
        najwieksze_obciazenie >= pojemnosc,
        "most nie zapchał się ani razu: {najwieksze_obciazenie} z {pojemnosc}"
    );
    assert!(byl_spillback, "nie było ani jednego wstrzymanego wjazdu");
    assert!(
        min_predkosc_dolotu < swobodna_dolot,
        "kolejka nie spowolniła dolotu: {min_predkosc_dolotu} = {swobodna_dolot}"
    );
    assert_eq!(przybyli, u64::from(POJAZDOW), "nie wszyscy dojechali");
    assert_eq!(
        net.mezo.vehicles_on_network(),
        0,
        "korek nie zszedł: {} pojazdów wisi na sieci",
        net.mezo.vehicles_on_network()
    );
    assert_eq!(net.stats.failed, 0, "podróże nieudane w zwykłym korku");
    assert_eq!(
        net.mezo.links[dolot].mean_speed_dkmh, swobodna_dolot,
        "po rozładowaniu dolot nie wrócił do prędkości swobodnej"
    );
}

/// Test własnościowy paliwa (§7.1): `Σ zatankowane − Σ spalone == Σ poziomów baków
/// − stan początkowy`, tolerancja 0.
#[test]
fn bilans_paliwa_domyka_sie_co_do_mikrolitra() {
    let cat = VehicleCatalog::load_default().expect("data/vehicles/");
    let vdf = VdfTable::load_default().expect("data/roads/");
    let (road, edges) = korytarz();
    let mut net = TrafficNetwork::new(&road, &vdf);
    let route = trasa(&edges);

    const POJAZDOW: u32 = 60;
    let klasy = ["car_small", "car_medium", "car_large", "van"];
    let mut poczatkowe: i64 = 0;
    let mut pending = Vec::new();
    for i in 0..POJAZDOW {
        let class = cat.by_key(klasy[i as usize % klasy.len()]).expect("klasa");
        let bak = cat.spec(class).tank_ml * 1_000;
        // Co trzeci pojazd rusza z prawie pustym bakiem i tankuje po drodze.
        let stan = if i % 3 == 0 { bak / 20 } else { bak };
        poczatkowe += stan;
        let mut p = zlecenie(i, &route, class, stan, 0);
        p.tank_capacity_ul = bak;
        if i % 3 == 0 {
            p.station = Some(magnat_core::PlaceRef::default());
            // Tankowanie po pierwszej krawędzi: dwa odcinki trasy zamiast jednego.
            p.legs = vec![trasa(&edges[..1]), trasa(&edges[1..])];
        }
        pending.push(p);
    }

    let mut zdarzenia = Vec::new();
    let mut koncowe: i64 = 0;
    let mut przybylo = 0u32;
    for minuta in 0..600u32 {
        let dzis = if minuta == 0 {
            std::mem::take(&mut pending)
        } else {
            Vec::new()
        };
        zdarzenia.clear();
        net.step_minute(minuta, &road, &vdf, &cat, dzis, &mut zdarzenia);
        for e in &zdarzenia {
            if let TrafficEvent::Arrived { tank_level_ul, .. } = e {
                koncowe += *tank_level_ul;
                przybylo += 1;
            }
        }
    }

    assert_eq!(przybylo, POJAZDOW, "nie wszyscy dojechali");
    assert!(net.stats.refuels > 0, "nikt nie tankował, mimo pustych baków");
    assert_eq!(
        koncowe - poczatkowe,
        net.stats.fuel_bought_ul - net.stats.fuel_burned_ul,
        "bilans paliwa się nie domyka: koniec {koncowe}, start {poczatkowe}, \
         zatankowane {}, spalone {}",
        net.stats.fuel_bought_ul,
        net.stats.fuel_burned_ul
    );
    assert!(
        net.stats.fuel_spent.0 > 0,
        "tankowanie nic nie kosztowało — paliwo za darmo łamie bilans pieniądza"
    );
    // Bramka 5 (00 §7): każda podróż wychodzi z uzasadnieniem, a tankowanie
    // przesłania wybór środka — bo to o nim mieszkaniec chce przeczytać w karcie.
    assert_eq!(net.stats.reasons[5], 0, "podróż bez uzasadnienia z bloku M4");
    assert!(
        net.stats.reasons[3] > 0,
        "żadna podróż nie zapisała wyboru stacji, mimo {} tankowań",
        net.stats.refuels
    );
}

/// Determinizm kroku minutowego: ten sam scenariusz, ten sam hash stanu (00 §3.6).
#[test]
fn dwa_przebiegi_daja_ten_sam_hash() {
    use magnat_core::{HashState, StateHasher};

    let cat = VehicleCatalog::load_default().expect("data/vehicles/");
    let vdf = VdfTable::load_default().expect("data/roads/");
    let class = cat.by_key("car_medium").expect("car_medium");

    let przebieg = || {
        let (road, edges) = korytarz();
        let mut net = TrafficNetwork::new(&road, &vdf);
        let route = trasa(&edges);
        let mut zdarzenia = Vec::new();
        let mut hashe = Vec::new();
        for minuta in 0..200u32 {
            let pending: Vec<PendingTrip> = if minuta < 20 {
                (0..5)
                    .map(|k| zlecenie(minuta * 5 + k, &route, class, 50_000_000, minuta))
                    .collect()
            } else {
                Vec::new()
            };
            zdarzenia.clear();
            net.step_minute(minuta, &road, &vdf, &cat, pending, &mut zdarzenia);
            let mut h = StateHasher::new();
            net.hash_state(&mut h);
            hashe.push(h.finish());
        }
        hashe
    };

    let a = przebieg();
    assert_eq!(a, przebieg(), "ciąg hashy się rozjechał");
    // Hash musi **reagować** na ruch, inaczej test powyżej broniłby stałej.
    // Stan sieci wchodzi do hasha świata przez `register_resource_hash`, więc
    // niezmienny hash znaczyłby, że rozjazd ruchu przechodzi przez CI niezauważony.
    assert!(
        a.windows(2).any(|w| w[0] != w[1]),
        "hash sieci nie zmienił się ani razu przez dwieście minut z ruchem"
    );
}

/// Przeniesione z `sim/agents` razem z warstwą Mikro (`Z-4`): 5 tys. pieszych
/// w kadrze mieści się w budżecie klatki, a warstwa nie dotyka niczego poza sobą.
#[test]
fn mikro_utrzymuje_piec_tysiecy_pieszych_w_kadrze() {
    use magnat_core::WorldCoord;
    use magnat_traffic::MicroLayer;

    const PIESZYCH: u32 = 5_000;
    let warstwa = MicroLayer::new();
    warstwa.set_window(Some((0, 0)), 100_000);
    for i in 0..PIESZYCH {
        let trasa = [
            WorldCoord::new(0, 0, 0),
            WorldCoord::new(((i % 100) * 500) as i32, ((i / 100) * 500) as i32, 0),
        ];
        warstwa.enter(i, &trasa, 8 * 60, 8 * 60 + 30);
    }
    assert_eq!(warstwa.len(), PIESZYCH as usize);

    let start = std::time::Instant::now();
    for k in 0..100u64 {
        warstwa.step(8 * 60 * 60_000 + k * 100);
    }
    let na_klatke = start.elapsed() / 100;
    println!(
        "mikro: {PIESZYCH} pieszych, {:.0} µs na klatkę",
        na_klatke.as_secs_f64() * 1e6
    );
    assert!(
        na_klatke < std::time::Duration::from_millis(20),
        "krok mikro {na_klatke:?} — próg WP6 to 2 ms, tu z zapasem 10×"
    );

    warstwa.retire(8 * 60 + 31);
    assert_eq!(warstwa.len(), 0, "pieszy po przybyciu został w buforze");
}
