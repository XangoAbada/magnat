//! Sieci przesyłowe: bilans, zrzut obciążenia, kaskada i determinizm (M8b WP3).
//!
//! To jest połowa testu T2 z §7 dokumentu fazy — ta, która dotyczy samej sieci.
//! Druga połowa (blackout zatrzymuje produkcję i widać to w kosztach) stoi
//! w `tools/headless/tests/blackout.rs`, bo wymaga zakładu, ksiąg i miasta.

use magnat_core::{Entity, FirmId, Money, Tick};
use magnat_traffic::utility::solve::{ProfileTable, RepairWindow};
use magnat_traffic::utility::{
    LoadProfile, NodeRole, SupplyState, Tariff, UtilityEdge, UtilityNetwork, UtilityNode,
};
use std::num::NonZeroU32;

const SEED: u64 = 0xBEEF_1234;

fn operator() -> FirmId {
    FirmId(Entity::new(7, NonZeroU32::MIN))
}

fn taryfa() -> Tariff {
    Tariff {
        standing_charge_per_month: Money(4_500),
        per_unit: Money(65),
    }
}

/// Sieć promieniowa: źródło (0), stacja (1) i trzech odbiorców o różnych
/// priorytetach. `moc` to moc źródła, reszta jest stała.
///
/// Odbiorcy: 2 — przemysł (priorytet 3, 40 kW), 3 — gospodarstwa (2, 30 kW),
/// 4 — szpital (0, 10 kW). Razem 80 kW plus straty.
fn promieniowa(moc: i64, przepustowosc_odgalezienia: i64) -> UtilityNetwork {
    let nodes = vec![
        UtilityNode::source(moc),
        UtilityNode::hub(),
        UtilityNode::connection(None, 3, 40_000, LoadProfile::Flat),
        UtilityNode::connection(None, 2, 30_000, LoadProfile::Flat),
        UtilityNode::connection(None, 0, 10_000, LoadProfile::Flat),
    ];
    let edges = vec![
        UtilityEdge::new(0, 1, 1_000_000, 90),
        UtilityEdge::new(1, 2, przepustowosc_odgalezienia, 210),
        UtilityEdge::new(1, 3, przepustowosc_odgalezienia, 210),
        UtilityEdge::new(1, 4, przepustowosc_odgalezienia, 210),
    ];
    UtilityNetwork::new(
        magnat_core::UtilityService::Electricity,
        operator(),
        nodes,
        edges,
        taryfa(),
    )
}

fn krok(net: &mut UtilityNetwork, t: u64) -> magnat_traffic::utility::SolveReport {
    net.solve(
        SEED,
        Tick(t),
        12,
        &ProfileTable::default(),
        RepairWindow {
            min_minutes: 120,
            max_minutes: 120,
        },
    )
}

/// Sieć z zapasem mocy nie robi nic: nikt nie gaśnie, nic nie wypada, jedna runda.
#[test]
fn siec_z_zapasem_nie_zrzuca_niczego() {
    let mut n = promieniowa(200_000, 1_000_000);
    let r = krok(&mut n, 1);
    assert!(r.shed_nodes.is_empty());
    assert!(r.tripped_edges.is_empty());
    assert_eq!(r.unserved, 0);
    assert_eq!(r.cascade_rounds, 1);
    for v in 2..5 {
        assert_eq!(n.state_of(v), SupplyState::Ok);
    }
}

/// Wyłączone źródło zabiera prąd całej wyspie — i to jest `Isolated`, nie `Shed`.
/// Nie ma kogo odłączać: prądu po prostu nie ma, a to są dwa różne zdania o sieci.
#[test]
fn wylaczone_zrodlo_izoluje_cala_wyspe() {
    let mut n = promieniowa(200_000, 1_000_000);
    assert!(n.set_source_online(0, false));
    let r = krok(&mut n, 1);
    assert_eq!(n.state_of(0), SupplyState::Faulted);
    for v in 2..5 {
        assert_eq!(n.state_of(v), SupplyState::Isolated);
    }
    assert_eq!(r.unserved, 80_000);
}

/// Zrzut idzie od najwyższego numeru priorytetu: najpierw przemysł, potem
/// gospodarstwa, szpital na końcu. To jest odwrotnie, niż mówiło §5.4 —
/// przy tamtej kolejności pierwszy zrzut w mieście gasiłby szpital.
#[test]
fn zrzut_gasi_przemysl_przed_szpitalem() {
    // Moc na tyle mała, że musi zgasnąć przemysł, ale nie gospodarstwa.
    let mut n = promieniowa(45_000, 1_000_000);
    let r = krok(&mut n, 1);
    assert_eq!(r.shed_nodes, vec![2], "zgasł ktoś inny niż przemysł");
    assert_eq!(n.state_of(2), SupplyState::Shed);
    assert_eq!(n.state_of(3), SupplyState::Ok);
    assert_eq!(n.state_of(4), SupplyState::Ok);
    assert_eq!(r.unserved, 40_000);

    // Przy jeszcze mniejszej mocy schodzą gospodarstwa, a szpital zostaje.
    let mut n = promieniowa(12_000, 1_000_000);
    let r = krok(&mut n, 1);
    assert_eq!(r.shed_nodes, vec![2, 3]);
    assert_eq!(n.state_of(4), SupplyState::Ok, "zgasł szpital");
}

/// Straty składają się wzdłuż drogi, więc źródło musi dać **więcej** niż suma
/// popytu. Sieć o mocy równej co do wata sumie popytu zrzuca — i dobrze,
/// bo prąd zużyty na grzanie przewodów też trzeba wyprodukować.
#[test]
fn straty_na_przesyle_podnosza_zapotrzebowanie() {
    let mut n = promieniowa(80_000, 1_000_000);
    let r = krok(&mut n, 1);
    assert!(
        !r.shed_nodes.is_empty(),
        "sieć bez zapasu na straty domknęła bilans — straty nie są liczone"
    );
}

/// Przeciążone odgałęzienie wypada z sieci, a to, co za nim, zostaje bez prądu.
/// Zabezpieczenie wraca samo po czasie naprawy.
#[test]
fn przeciazone_odgalezienie_wypada_i_wraca() {
    // Odgałęzienia uniosą po 35 kW. Przemysł ciągnie 40 kW i wypada;
    // gospodarstwa (30 kW ze stratą 30,6 kW) i szpital mieszczą się.
    let mut n = promieniowa(200_000, 35_000);
    let r = krok(&mut n, 1);
    assert_eq!(r.tripped_edges, vec![1], "zabezpieczenie nie zadziałało");
    assert!(r.cascade_rounds >= 2, "wypadnięcie linii nie dało rundy 2");
    assert_eq!(n.state_of(2), SupplyState::Isolated);
    // Gospodarstwa i szpital nie ucierpiały: ich odgałęzienia mieszczą się
    // w przepustowości.
    assert_eq!(n.state_of(3), SupplyState::Ok);

    // Czas naprawy to w tym teście równe 120 minut. Wcześniej linia stoi…
    let _ = krok(&mut n, 100);
    assert_eq!(n.state_of(2), SupplyState::Isolated);
    // …a po naprawie wraca i natychmiast wypada znowu, bo przyczyna nie ustąpiła.
    // To jest poprawne zachowanie sieci, której nikt nie przebudował — i dowód,
    // że krawędź naprawdę wróciła, a nie została wyłączona na zawsze.
    let r = krok(&mut n, 121);
    assert_eq!(r.tripped_edges, vec![1]);
}

/// Pierścień nie przeciąża się nigdy — to jest sufit modelu nazwany wprost
/// w `utility::topo`. Krawędź w pierścieniu ma obok siebie objazd, więc przepływ
/// policzony dla niej na drzewie jest artefaktem wyboru drzewa.
#[test]
fn pierscien_nie_wypada_mimo_ciasnej_przepustowosci() {
    let nodes = vec![
        UtilityNode::source(200_000),
        UtilityNode::hub(),
        UtilityNode::hub(),
        UtilityNode::connection(None, 3, 40_000, LoadProfile::Flat),
    ];
    // 0-1, 1-2, 2-0 to pierścień; 2-3 to odgałęzienie.
    let edges = vec![
        UtilityEdge::new(0, 1, 1_000, 0),
        UtilityEdge::new(1, 2, 1_000, 0),
        UtilityEdge::new(2, 0, 1_000, 0),
        UtilityEdge::new(2, 3, 1_000_000, 0),
    ];
    let mut n = UtilityNetwork::new(
        magnat_core::UtilityService::Electricity,
        operator(),
        nodes,
        edges,
        taryfa(),
    );
    let r = krok(&mut n, 1);
    assert!(r.tripped_edges.is_empty(), "pierścień wypadł");
    assert_eq!(n.state_of(3), SupplyState::Ok);
}

/// Kaskada: otwarcie pierścienia zamienia go w ścieżkę, a wtedy odcinek przy
/// źródle musi unieść popyt całego miasta i wypada. To jest awaria N-1 i dokładnie
/// tak kaskadują prawdziwe sieci. Wariant kaskadowy testu T2: kaskada kończy się
/// w ≤ 8 rundach, a każda wyspa po ustabilizowaniu ma `supply ≥ demand`.
#[test]
fn otwarcie_pierscienia_wywoluje_kaskade() {
    let nodes = vec![
        UtilityNode::source(200_000),
        UtilityNode::hub(),
        UtilityNode::hub(),
        UtilityNode::connection(None, 3, 40_000, LoadProfile::Flat),
        UtilityNode::connection(None, 3, 40_000, LoadProfile::Flat),
    ];
    let edges = vec![
        UtilityEdge::new(0, 1, 50_000, 0), // pierścień: źródło → stacja A
        UtilityEdge::new(1, 2, 50_000, 0), // pierścień: stacja A → stacja B
        UtilityEdge::new(2, 0, 50_000, 0), // pierścień: stacja B → źródło
        UtilityEdge::new(1, 3, 1_000_000, 0),
        UtilityEdge::new(2, 4, 1_000_000, 0),
    ];
    let mut n = UtilityNetwork::new(
        magnat_core::UtilityService::Electricity,
        operator(),
        nodes,
        edges,
        taryfa(),
    );
    // Pierścień zamknięty: 80 kW rozpływa się dwiema drogami i nic nie wypada.
    let r = krok(&mut n, 1);
    assert!(r.tripped_edges.is_empty());

    // Otwieramy pierścień awarią odcinka B → źródło. Teraz obie stacje wiszą
    // szeregowo na odcinku 0-1, przez który idzie 80 kW przy przepustowości 50 kW.
    n.edges[2].state = magnat_traffic::utility::EdgeState::Tripped { until: Tick(9_999) };
    n.mark_topology_dirty();
    let r = krok(&mut n, 2);
    assert_eq!(r.tripped_edges, vec![0], "odcinek przy źródle nie wypadł");
    assert!(r.cascade_rounds >= 2);
    assert!(r.cascade_rounds <= magnat_traffic::utility::MAX_CASCADE_ROUNDS);
    // Po kaskadzie całe miasto jest bez prądu — i to jest wynik, nie błąd.
    assert_eq!(n.state_of(3), SupplyState::Isolated);
    assert_eq!(n.state_of(4), SupplyState::Isolated);
    assert_eq!(r.unserved, 80_000);
}

/// Kaskada ma twardy sufit: nawet sieć, w której każda runda wywala kolejną
/// linię, kończy się po ośmiu rundach i zostawia znany górny koszt ticku.
#[test]
fn kaskada_konczy_sie_w_limicie_rund() {
    // Łańcuch stacji o coraz ciaśniejszej przepustowości — każda runda odcina
    // kolejny odcinek, a odcięcie zmienia topologię, czyli otwiera następną rundę.
    let mut nodes = vec![UtilityNode::source(10_000_000)];
    let mut edges = Vec::new();
    for i in 1..=12u32 {
        nodes.push(UtilityNode::connection(None, 3, 10_000, LoadProfile::Flat));
        edges.push(UtilityEdge::new(i - 1, i, 1, 0));
    }
    let mut n = UtilityNetwork::new(
        magnat_core::UtilityService::Electricity,
        operator(),
        nodes,
        edges,
        taryfa(),
    );
    let r = krok(&mut n, 1);
    assert!(r.cascade_rounds <= magnat_traffic::utility::MAX_CASCADE_ROUNDS);
}

/// Determinizm: ten sam seed i ten sam tick dają ten sam ciąg zdarzeń co do
/// krawędzi i co do minuty naprawy (00 §3.1, `StreamId::GridFault`).
#[test]
fn dwa_przebiegi_tego_samego_ziarna_sa_identyczne() {
    let przebieg = || {
        let mut n = promieniowa(200_000, 20_000);
        let mut slad = Vec::new();
        for t in 1..200u64 {
            let r = n.solve(
                SEED,
                Tick(t),
                u32::try_from(t % 1440 / 60).expect("godzina"),
                &ProfileTable::default(),
                RepairWindow {
                    min_minutes: 60,
                    max_minutes: 420,
                },
            );
            slad.push((t, r.shed_nodes, r.tripped_edges, r.unserved));
        }
        slad
    };
    assert_eq!(przebieg(), przebieg());
}

/// Kształt doby ma skutek: ta sama sieć przechodzi wieczorny szczyt albo nie,
/// zależnie od godziny. Bez tego popyt byłby stały, a sieć o stałym popycie
/// przeciąża się wyłącznie wtedy, gdy ktoś wymusi awarię (`R2`).
#[test]
fn szczyt_wieczorny_zrzuca_to_czego_noc_nie_zrzuca() {
    let profile = magnat_traffic::utility::GridTuning::load_default()
        .expect("data/tuning/grid.ron")
        .profiles;
    let mut nodes = vec![UtilityNode::source(60_000), UtilityNode::hub()];
    nodes.push(UtilityNode::connection(
        None,
        2,
        100_000,
        LoadProfile::Household,
    ));
    let edges = vec![
        UtilityEdge::new(0, 1, 1_000_000, 0),
        UtilityEdge::new(1, 2, 1_000_000, 0),
    ];
    let mut n = UtilityNetwork::new(
        magnat_core::UtilityService::Electricity,
        operator(),
        nodes,
        edges,
        taryfa(),
    );
    let noc = n.solve(SEED, Tick(1), 3, &profile, RepairWindow::default());
    assert!(noc.shed_nodes.is_empty(), "noc zrzuciła obciążenie");
    let wieczor = n.solve(SEED, Tick(2), 19, &profile, RepairWindow::default());
    assert_eq!(wieczor.shed_nodes, vec![2], "szczyt nie zrzucił niczego");
}

/// Ani jednego floata w sieci — to jest wymóg §5.0 fazy, mocniejszy niż `K-6`.
/// Test strukturalny, bo sygnatura z `f64` przeszłaby każdy test zachowania
/// i rozjechałaby dopiero dwa przebiegi na dwóch maszynach.
#[test]
fn solver_nie_ma_ani_jednego_floata() {
    for plik in [
        include_str!("../src/utility.rs"),
        include_str!("../src/utility/solve.rs"),
        include_str!("../src/utility/topo.rs"),
        include_str!("../src/utility/system.rs"),
        include_str!("../src/utility/tuning.rs"),
    ] {
        for (i, l) in plik.lines().enumerate() {
            let kod = l.split("//").next().unwrap_or("");
            assert!(
                !kod.contains("f64") && !kod.contains("f32"),
                "float w linii {}: {l}",
                i + 1
            );
        }
    }
}

/// Węzeł bez przypisanej roli źródła nie da się wyłączyć — `set_source_online`
/// mówi o tym wprost zamiast po cichu nie zrobić nic.
#[test]
fn wylaczyc_da_sie_tylko_zrodlo() {
    let mut n = promieniowa(200_000, 1_000_000);
    assert!(!n.set_source_online(2, false));
    assert!(matches!(n.nodes[2].role, NodeRole::Connection { .. }));
}
