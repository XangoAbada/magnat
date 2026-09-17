//! Nakładka parametrów: zapis, składanie i **powrót do wartości zastanej**.
//!
//! To jest własność, na której stoi cała reszta: świat po wygaśnięciu zdarzenia
//! ma wrócić dokładnie tam, gdzie był. Właściciel pola nie wie, że ktoś mu je
//! podmienił, więc jedynym, kto pamięta stan sprzed, jest nakładka — a pamięć,
//! której nikt nie sprawdza, myli się cicho.
//!
//! Test buduje sieć ręcznie, bez miasta: zdarzenie jest tu **wymuszone**
//! (`EventCause::Forced`), więc mierzy się skutek, a nie losowanie.

use magnat_core::{EventId, Tick, UtilityService};
use magnat_ecs::World;
use magnat_events::{
    ClimateNorms, EpochClock, EventCatalog, EventCause, Events, ParamPatch, ScopeInstance,
    SimParam, WeatherState, WorldEvent,
};
use magnat_traffic::utility::{
    register_grids, LoadProfile, UtilityEdge, UtilityGrids, UtilityNetwork, UtilityNode,
};

const MOC: i64 = 1_000_000;

fn swiat() -> World {
    let mut w = World::new(1);
    let nodes = vec![
        UtilityNode::source(MOC),
        UtilityNode::connection(None, 2, 500_000, LoadProfile::Household),
    ];
    let edges = vec![UtilityEdge::new(0, 1, 10_000_000, 0)];
    let mut g = UtilityGrids::default();
    g.push(UtilityNetwork::new(
        UtilityService::Electricity,
        magnat_core::FirmId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
        nodes,
        edges,
        magnat_traffic::utility::Tariff {
            standing_charge_per_month: magnat_core::Money::ZERO,
            per_unit: magnat_core::Money::ZERO,
        },
    ));
    register_grids(&mut w, g);
    magnat_agents::register_world_params(&mut w);
    w
}

fn rejestr() -> Events {
    Events::new(
        EventCatalog::default(),
        WeatherState::new(ClimateNorms::default()),
        EpochClock::new(1990),
    )
}

fn zdarzenie(patches: Vec<ParamPatch>, do_doby: u64) -> WorldEvent {
    WorldEvent {
        id: EventId(0),
        def: 0,
        scope: ScopeInstance::Network(0),
        started_day: 0,
        ends_day: Some(do_doby),
        min_end_day: 0,
        severity_bps: 10_000,
        cause: EventCause::Forced,
        patches,
    }
}

fn moc_zrodla(world: &World) -> i64 {
    world
        .resource::<UtilityGrids>()
        .net(UtilityService::Electricity)
        .and_then(|n| n.source_info(0))
        .map_or(0, |(c, _, _)| c)
}

fn czynne(world: &World) -> bool {
    world
        .resource::<UtilityGrids>()
        .net(UtilityService::Electricity)
        .and_then(|n| n.source_info(0))
        .is_some_and(|(_, on, _)| on)
}

#[test]
fn moc_zrodla_wraca_po_wygasnieciu() {
    let mut world = swiat();
    let mut ev = rejestr();
    assert_eq!(moc_zrodla(&world), MOC);

    ev.open(
        zdarzenie(
            vec![ParamPatch {
                param: SimParam::SourceCapacityMulBps(
                    UtilityService::Electricity.as_index() as u8,
                    0,
                ),
                value: 4_000,
            }],
            2,
        ),
        Tick(0),
    );
    magnat_events::apply::apply(&mut ev, &mut world);
    assert_eq!(moc_zrodla(&world), MOC * 4 / 10, "awaria nie ścięła mocy");

    ev.close_expired(3, Tick(3 * 1440), |_| false);
    magnat_events::apply::apply(&mut ev, &mut world);
    assert_eq!(
        moc_zrodla(&world),
        MOC,
        "moc nie wróciła do wartości sprzed zdarzenia"
    );
}

#[test]
fn dwa_zdarzenia_na_jeden_parametr_mnoza_sie_i_wracaja_po_kolei() {
    let mut world = swiat();
    let mut ev = rejestr();
    let p = SimParam::SourceCapacityMulBps(UtilityService::Electricity.as_index() as u8, 0);

    let a = ev.open(
        zdarzenie(
            vec![ParamPatch {
                param: p,
                value: 5_000,
            }],
            2,
        ),
        Tick(0),
    );
    let b = ev.open(
        zdarzenie(
            vec![ParamPatch {
                param: p,
                value: 5_000,
            }],
            5,
        ),
        Tick(0),
    );
    assert_ne!(a.0, b.0, "dwa zdarzenia dostały ten sam numer");
    magnat_events::apply::apply(&mut ev, &mut world);
    assert_eq!(
        moc_zrodla(&world),
        MOC / 4,
        "dwa zdarzenia po połowie mocy nie złożyły się w ćwierć"
    );

    // Pierwsze gaśnie: zostaje mnożnik drugiego, liczony **od wartości zastanej**,
    // a nie od tej, którą świat ma w tej chwili.
    ev.close_expired(3, Tick(3 * 1440), |_| false);
    magnat_events::apply::apply(&mut ev, &mut world);
    assert_eq!(moc_zrodla(&world), MOC / 2);

    ev.close_expired(6, Tick(6 * 1440), |_| false);
    magnat_events::apply::apply(&mut ev, &mut world);
    assert_eq!(moc_zrodla(&world), MOC);
}

#[test]
fn mnoznik_rowny_mocy_nie_udaje_powrotu() {
    // Regresja: rozpoznawanie powrotu po `v == base` zamieniało przycięcie mocy
    // w brak zmiany dokładnie wtedy, gdy mnożnik w punktach bazowych był liczbowo
    // równy mocy źródła w watach. Sieć o mocy 5 000 W przy mnożniku 5 000 bps
    // powinna zejść do 2 500 W, a zostawała na 5 000.
    let mut world = World::new(1);
    let nodes = vec![
        UtilityNode::source(5_000),
        UtilityNode::connection(None, 2, 1_000, LoadProfile::Flat),
    ];
    let mut g = UtilityGrids::default();
    g.push(UtilityNetwork::new(
        UtilityService::Electricity,
        magnat_core::FirmId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
        nodes,
        vec![UtilityEdge::new(0, 1, 1_000_000, 0)],
        magnat_traffic::utility::Tariff {
            standing_charge_per_month: magnat_core::Money::ZERO,
            per_unit: magnat_core::Money::ZERO,
        },
    ));
    register_grids(&mut world, g);

    let mut ev = rejestr();
    ev.open(
        zdarzenie(
            vec![ParamPatch {
                param: SimParam::SourceCapacityMulBps(
                    UtilityService::Electricity.as_index() as u8,
                    0,
                ),
                value: 5_000,
            }],
            2,
        ),
        Tick(0),
    );
    magnat_events::apply::apply(&mut ev, &mut world);
    assert_eq!(moc_zrodla(&world), 2_500);
}

#[test]
fn dwa_rodzaje_konca_mieszaja_sie_bez_szkody() {
    // Zdarzenie o stałym terminie i zdarzenie o końcu warunkowym trwają obok
    // siebie. Pytanie „czy świat je trzyma" dotyczy **wyłącznie** drugiego,
    // więc odpowiedzi trzeba adresować numerem zdarzenia, a nie pozycją w liście
    // aktywnych — przy pozycji pierwszy termin przesuwa całą resztę o jeden
    // i gaśnie nie to, co trzeba. `krok` adresuje numerem i tak ma zostać.
    let mut world = swiat();
    let mut ev = rejestr();
    let p = SimParam::SourceOnline(UtilityService::Electricity.as_index() as u8, 0);

    // Zdarzenie o stałym czasie (gaśnie po dobie 1) i zdarzenie warunkowe (trwa).
    ev.open(zdarzenie(vec![], 1), Tick(0));
    let mut warunkowe = zdarzenie(vec![ParamPatch { param: p, value: 0 }], 0);
    warunkowe.ends_day = None;
    warunkowe.min_end_day = 0;
    ev.open(warunkowe, Tick(0));
    magnat_events::apply::apply(&mut ev, &mut world);
    assert!(!czynne(&world), "zdarzenie nie zgasiło źródła");

    // Doba 2: pierwsze wygasa z terminu, drugie **trzyma**, bo świat je trzyma.
    ev.close_expired(2, Tick(2 * 1440), |_| true);
    magnat_events::apply::apply(&mut ev, &mut world);
    assert_eq!(ev.active().len(), 1, "wygasło nie to zdarzenie, co trzeba");
    assert!(!czynne(&world), "źródło wróciło, choć zdarzenie trwa");

    // Doba 3: świat przestaje je trzymać.
    ev.close_expired(3, Tick(3 * 1440), |_| false);
    magnat_events::apply::apply(&mut ev, &mut world);
    assert!(ev.active().is_empty());
    assert!(czynne(&world), "źródło nie wróciło po wygaśnięciu");
}
