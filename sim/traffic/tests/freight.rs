//! Trasa ładunku i konsolidacja — kryterium ukończenia WP12 (M6d).
//!
//! **Dlaczego ten test mieszka w `sim/traffic`, a nie w `sim/supply`.** Pyta o rzecz,
//! która wymaga obu stron naraz: konsolidacji zleceń (M6) i prawdziwej sieci drogowej
//! (M4). `sim/supply` jest crate'em liściastym i ma nim zostać — od `magnat-traffic`
//! zależeć nie może, więc jedynym miejscem, z którego widać jedno i drugie, jest ta
//! strona szwu. Tak samo rozstrzygnął to M4 dla `ramp_wait_lod_invariant`: test wspólny
//! stoi u tego, kto widzi obie połowy.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use magnat_core::{BodyType, Entity, Mass, Money, SimMinute, SiteId, Volume, WorldCoord};
use magnat_nav::{EdgeSpec, Modality, NavGraphs, NavRouter, RoadGraphBuilder};
use magnat_supply::transport::{consolidate, ConsolidationLimits};
use magnat_supply::{StorageClass, VehicleRequirements};
use magnat_traffic::freight::RoadFreight;
use magnat_traffic::oracle::TrafficOracle;
use magnat_traffic::spec::VehicleCatalog;

/// Rozstaw skrzyżowań w centymetrach — 200 m.
const ROZSTAW: i32 = 20_000;
const KOLUMN: i32 = 11;
const PODSTAWIENIE_GR: i64 = 8_000;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

fn wspolrzedne(x: i32, y: i32) -> WorldCoord {
    WorldCoord::new(x * ROZSTAW, y * ROZSTAW, 0)
}

/// Siatka ulic 11 × 11 o boku 200 m — 2 × 2 km miasta. Wszystkie krawędzie są
/// przejezdne dla ruchu ciężkiego, bo pytanie tego testu dotyczy odległości,
/// nie zakazów; te mają własny test po stronie routera.
fn siatka() -> NavGraphs {
    let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
    for y in 0..KOLUMN {
        for x in 0..KOLUMN {
            b.add_node(magnat_core::IVec2::new(x * ROZSTAW, y * ROZSTAW), 0);
        }
    }
    let wezel = |x: i32, y: i32| magnat_nav::NodeId((y * KOLUMN + x) as u32);
    let dodaj = |b: &mut RoadGraphBuilder, a, c| {
        b.add_edge_pair(EdgeSpec {
            from: a,
            to: c,
            geometry_ref: magnat_nav::GeomRef(0),
            length_cm: ROZSTAW as u32,
            lanes: 2,
            class: magnat_core::RoadClass::Collector,
            speed_limit_dkmh: 500,
            max_mass: Mass(40_000_000),
            grade_permille: 0,
            bridge: None,
            curb_parking: 0,
            district: magnat_core::DistrictId(0),
        });
    };
    for y in 0..KOLUMN {
        for x in 0..KOLUMN {
            if x + 1 < KOLUMN {
                dodaj(&mut b, wezel(x, y), wezel(x + 1, y));
            }
            if y + 1 < KOLUMN {
                dodaj(&mut b, wezel(x, y), wezel(x, y + 1));
            }
        }
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

/// Producent w jednym rogu, dziesięć sklepów w dzielnicy po przekątnej, centrum
/// dystrybucyjne w środku tej dzielnicy.
struct Miasto {
    freight: RoadFreight,
    producent: SiteId,
    dc: SiteId,
    sklepy: Vec<SiteId>,
}

fn miasto() -> Miasto {
    let router = NavRouter::build(siatka(), 1, 1 << 12);
    let catalog = Arc::new(VehicleCatalog::load_default().expect("data/vehicles/classes.ron"));
    let oracle = Arc::new(TrafficOracle::new(
        Arc::new(magnat_agents::places::PlaceTable::build(Vec::new())),
        router,
        Arc::clone(&catalog),
        Vec::new(),
        Vec::new(),
    ));

    let producent = SiteId(encja(1));
    let dc = SiteId(encja(2));
    let mut sites: BTreeMap<SiteId, WorldCoord> = BTreeMap::new();
    sites.insert(producent, wspolrzedne(0, 0));
    sites.insert(dc, wspolrzedne(8, 8));

    // Dziesięć sklepów wokół centrum dystrybucyjnego, w promieniu dwóch przecznic.
    let wokol = [
        (7, 7),
        (8, 7),
        (9, 7),
        (7, 8),
        (9, 8),
        (7, 9),
        (8, 9),
        (9, 9),
        (10, 8),
        (8, 10),
    ];
    let mut sklepy = Vec::new();
    for (i, (x, y)) in wokol.into_iter().enumerate() {
        let s = SiteId(encja(10 + i as u32));
        sites.insert(s, wspolrzedne(x, y));
        sklepy.push(s);
    }

    Miasto {
        freight: RoadFreight::new(oracle, catalog, sites, PODSTAWIENIE_GR),
        producent,
        dc,
        sklepy,
    }
}

fn wymagania(mass: Mass) -> VehicleRequirements {
    VehicleRequirements {
        body: BodyType::Box,
        min_payload: mass,
        min_volume: Volume(mass.0 * 4),
        adr: false,
        storage_class: StorageClass::Ambient,
    }
}

#[test]
fn trasa_towarowa_ma_prawdziwe_kilometry() {
    use magnat_supply::FreightOracle;
    let m = miasto();
    let q = m
        .freight
        .quote(
            m.producent,
            m.sklepy[0],
            Mass(1_200_000),
            &wymagania(Mass(1_200_000)),
        )
        .expect("trasa istnieje");
    // Manhattan z (0,0) do (7,7) po siatce 200 m to 2 800 m; router nie ma krótszej drogi.
    assert_eq!(
        q.distance_m, 2_800,
        "odległość trasy nie zgadza się z geometrią siatki"
    );
    assert!(q.cost.0 > PODSTAWIENIE_GR, "koszt to samo podstawienie");
    assert_eq!(q.call_out, Money(PODSTAWIENIE_GR));
}

#[test]
fn ladunek_wiekszy_od_pojazdu_kosztuje_tyle_ile_kursow() {
    use magnat_supply::FreightOracle;
    let m = miasto();
    let jeden = Mass(20_000_000); // mieści się w cysternie 26 t
    let trzy = Mass(60_000_000); // trzy kursy tą samą cysterną
    let req = |mass: Mass| VehicleRequirements {
        body: BodyType::Tanker,
        min_payload: mass,
        min_volume: Volume(mass.0 * 6 / 5),
        adr: true,
        storage_class: StorageClass::Tank,
    };
    let a = m
        .freight
        .quote(m.producent, m.sklepy[0], jeden, &req(jeden))
        .expect("cysterna istnieje w katalogu");
    let b = m
        .freight
        .quote(m.producent, m.sklepy[0], trzy, &req(trzy))
        .expect("cysterna istnieje w katalogu");
    // Trzy kursy tą samą trasą kosztują trzy razy tyle — razem z trzema podstawieniami,
    // bo to trzy osobne przejazdy, a nie jeden dłuższy.
    assert_eq!(
        b.cost.0,
        a.cost.0 * 3,
        "kursy się nie liczą: {} gr wobec {} gr",
        b.cost.0,
        a.cost.0
    );
    assert_eq!(
        b.distance_m, a.distance_m,
        "dystans jednego kursu bez zmian"
    );
}

/// `dc_beats_direct` (§7.7): dziesięć sklepów zaopatrywanych przez centrum
/// dystrybucyjne ma **niższy koszt dostawy na tonę** niż dziesięć sklepów
/// zaopatrywanych wprost od producenta — i nie ma reguły, która by to wymuszała.
/// Wychodzi z dwóch rzeczy naraz: z geografii (długi odcinek jedzie się raz, a nie
/// dziesięć razy) i ze stawki za kilometr **pojazdu**, a nie za tonokilometr.
#[test]
fn dc_beats_direct() {
    use magnat_core::DecisionReason;
    use magnat_supply::store::WarehouseRole;
    use magnat_supply::{Carrier, StorageClass, Store, Transport, TransportRequest};
    let m = miasto();
    let cat = magnat_supply::load_default("contemporary").expect("katalog z data/");
    let chleb = cat.good_id("food_bread_wheat").expect("food_bread_wheat");
    let na_sklep = Mass(1_200_000); // 1,2 t na sklep

    // ── wariant A: dostawa bezpośrednia ───────────────────────────────────────
    let mut store_a = Store::new(cat.goods.len());
    let mut t_a = Transport::new();
    let zrodlo_a = store_a.add_slot(
        m.producent,
        WarehouseRole::Output,
        StorageClass::Ambient,
        Mass(100_000_000),
        Volume(1_000_000_000),
        0,
    );
    let mut wprost = Vec::new();
    for s in &m.sklepy {
        let cel = store_a.add_slot(
            *s,
            WarehouseRole::Backroom,
            StorageClass::Ambient,
            Mass(10_000_000),
            Volume(100_000_000),
            0,
        );
        wprost.push(t_a.order(
            &m.freight,
            TransportRequest {
                from: m.producent,
                to: *s,
                from_slot: zrodlo_a,
                to_slot: cel,
                good: chleb,
                mass: na_sklep,
                requires: wymagania(na_sklep),
                ready_at: SimMinute(0),
                due_at: SimMinute(600),
            },
            DecisionReason::Unspecified,
        ));
    }
    let koszt_a: i64 = wprost
        .iter()
        .map(|id| t_a.get(*id).expect("zlecenie").price.0)
        .sum();

    // ── wariant B: jeden kurs do centrum, stamtąd trasa objazdowa ─────────────
    let mut store_b = Store::new(cat.goods.len());
    let mut t_b = Transport::new();
    let zrodlo_b = store_b.add_slot(
        m.producent,
        WarehouseRole::Output,
        StorageClass::Ambient,
        Mass(100_000_000),
        Volume(1_000_000_000),
        0,
    );
    let w_dc = store_b.add_slot(
        m.dc,
        WarehouseRole::Distribution,
        StorageClass::Ambient,
        Mass(100_000_000),
        Volume(1_000_000_000),
        0,
    );
    let razem = Mass(na_sklep.0 * m.sklepy.len() as i64);
    // Towar musi w centrum **stać**, zanim ruszy trasa objazdowa: `dispatch_run` ładuje
    // ze slotu nadawcy i odmawia, gdy go tam nie ma. Dowóz od producenta jest wyceniony
    // niżej, ale do rozładunku w tym teście nie dochodzi — porównujemy koszty, nie ruch.
    store_b
        .put(
            &cat,
            w_dc,
            magnat_supply::store::BatchDraft {
                good: chleb,
                mass: razem,
                quality: magnat_core::Q::new(70),
                brand: None,
                producer: magnat_core::FirmId(encja(1)),
                produced_at: SimMinute(0),
                cost: Money(razem.0 * 140 / 1_000),
                origin: Default::default(),
                flags: Default::default(),
            },
            magnat_supply::store::MassIn::Initial,
        )
        .expect("zapas w centrum dystrybucyjnym");
    let dowoz = t_b.order(
        &m.freight,
        TransportRequest {
            from: m.producent,
            to: m.dc,
            from_slot: zrodlo_b,
            to_slot: w_dc,
            good: chleb,
            mass: razem,
            requires: wymagania(razem),
            ready_at: SimMinute(0),
            due_at: SimMinute(600),
        },
        DecisionReason::Unspecified,
    );
    let mut z_dc = Vec::new();
    for s in &m.sklepy {
        let cel = store_b.add_slot(
            *s,
            WarehouseRole::Backroom,
            StorageClass::Ambient,
            Mass(10_000_000),
            Volume(100_000_000),
            0,
        );
        z_dc.push(t_b.order(
            &m.freight,
            TransportRequest {
                from: m.dc,
                to: *s,
                from_slot: w_dc,
                to_slot: cel,
                good: chleb,
                mass: na_sklep,
                requires: wymagania(na_sklep),
                ready_at: SimMinute(0),
                due_at: SimMinute(600),
            },
            DecisionReason::Unspecified,
        ));
    }
    let trasy = consolidate(
        &t_b,
        &m.freight,
        &z_dc,
        ConsolidationLimits {
            capacity: Mass(24_000_000),
            max_stops: 12,
            radius_m: 4_000,
        },
    );
    assert!(
        trasy.iter().any(|r| r.stops.len() > 1),
        "konsolidacja nie połączyła ani jednej pary sklepów"
    );
    let koszt_b: i64 =
        t_b.get(dowoz).expect("dowóz").price.0 + trasy.iter().map(|r| r.cost.0).sum::<i64>();

    assert!(
        koszt_b < koszt_a,
        "centrum dystrybucyjne nie wygrało: wprost {koszt_a} gr, przez DC {koszt_b} gr"
    );

    // Podział kosztu trasy między zlecenia ma się sumować do kosztu trasy co do grosza.
    for r in &trasy {
        t_b.dispatch_run(&mut store_b, r, Carrier::Unassigned, SimMinute(0))
            .expect("wysyłka trasy");
        let suma: i64 = r
            .stops
            .iter()
            .map(|id| t_b.get(*id).expect("zlecenie").price.0)
            .sum();
        assert_eq!(suma, r.cost.0, "podział kosztu trasy zgubił grosze");
    }
}
