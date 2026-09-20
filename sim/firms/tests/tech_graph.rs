//! Walidator drzewa technologii w CI (M10c WP10.8).
//!
//! Kryterium WP10.8 wymienia go z nazwy: „walidator grafu `data/tech/` w CI (każdy
//! prereq istnieje, brak cykli, każdy `NewGood` istnieje w `data/goods/`)". Wszystkie
//! trzy reguły są **w ładowarce**, nie tutaj — tu jest dowód, że ładowarka je stosuje
//! i że plik w repozytorium je przechodzi.
//!
//! Test negatywny nie jest ozdobą: bramka, która nigdy nie świeci na czerwono, nie
//! jest bramką (ta sama zasada, co w `sim/economy/tests/single_entry_point.rs`).

use magnat_firms::rnd::tree::{TechTree, TechTreeError};

fn katalog() -> magnat_supply::Catalog {
    magnat_supply::catalog::load_default("contemporary").expect("data/goods/, data/recipes/")
}

fn sciezka() -> std::path::PathBuf {
    magnat_core::data_path("tech/tech.ron")
}

#[test]
fn drzewo_z_repozytorium_sie_wczytuje_i_przechodzi_walidacje() {
    let cat = katalog();
    let t = TechTree::load(&sciezka(), 1990, &cat).expect("data/tech/tech.ron");
    assert!(!t.is_empty(), "drzewo bez węzłów nie jest drzewem");
    assert!(t.validate_acyclic().is_ok());
    // Klucze są posortowane — na tym stoi `TechTree::id` (wyszukiwanie binarne)
    // i stabilność `TechId` między wersjami danych.
    let klucze: Vec<&str> = t.nodes.iter().map(|n| &*n.key).collect();
    let mut posortowane = klucze.clone();
    posortowane.sort_unstable();
    assert_eq!(klucze, posortowane, "klucze węzłów nie są posortowane");
    for (i, n) in t.nodes.iter().enumerate() {
        assert_eq!(n.id.0 as usize, i, "`TechId` nie jest pozycją w liście");
    }
}

#[test]
fn kazdy_warunek_wstepny_jest_wczesniejszym_wezlem() {
    let cat = katalog();
    let t = TechTree::load(&sciezka(), 1990, &cat).expect("data/tech/tech.ron");
    for n in &t.nodes {
        for p in &n.prereqs {
            assert!(
                t.get(*p).is_some(),
                "`{}` wymaga nieistniejącego węzła",
                n.key
            );
        }
        // Warunki są posortowane — po tej kolejności idzie sprawdzanie dostępności,
        // a ono wchodzi do wyboru projektu, czyli do hasha stanu.
        let mut s = n.prereqs.clone();
        s.sort_unstable();
        assert_eq!(&s, &n.prereqs);
    }
}

#[test]
fn rok_swiatowy_przeliczony_na_dobe_zalezy_od_roku_startu() {
    let cat = katalog();
    let wczesny = TechTree::load(&sciezka(), 1950, &cat).expect("tech.ron");
    let pozny = TechTree::load(&sciezka(), 1990, &cat).expect("tech.ron");
    let t = wczesny.id("mobile_telephony").expect("węzeł telefonii");
    // Ten sam węzeł, dwa światy: w mieście z 1950 świat pozna go po 42 latach,
    // w mieście z 1990 — po dwóch. Rok w pliku się nie zmienia, doba tak.
    assert_eq!(wczesny.node(t).world_year, 1992);
    assert_eq!(wczesny.node(t).world_day, 42 * 360);
    assert_eq!(pozny.node(t).world_day, 2 * 360);
    // Węzeł, którego rok minął przed startem partii, jest wiedzą powszechną
    // od pierwszej doby i patentu nie da już nikomu.
    let stary = pozny.id("cold_chain").expect("łańcuch chłodniczy");
    assert_eq!(pozny.node(stary).world_day, 0);
    assert!(!pozny.node(stary).patentable(0));
}

#[test]
fn nieznany_towar_w_efekcie_zatrzymuje_ladowanie() {
    let cat = katalog();
    let dir = std::env::temp_dir().join("magnat_tech_graph_zly_towar");
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    let p = dir.join("tech.ron");
    std::fs::write(
        &p,
        r#"(
            schema_version: 1,
            branches: [(key: "x")],
            nodes: [(key: "a", branch: "x", prereqs: [], cost_rp: 100, world_year: 1970,
                     effects: [NewGood("cons_nie_ma_takiego")])],
        )"#,
    )
    .expect("zapis");
    let Err(TechTreeError::UnknownGood { node, good }) = TechTree::load(&p, 1990, &cat) else {
        panic!("walidator przepuścił towar spoza katalogu");
    };
    assert_eq!(node, "a");
    assert_eq!(good, "cons_nie_ma_takiego");
}

#[test]
fn cykl_warunkow_zatrzymuje_ladowanie() {
    let cat = katalog();
    let dir = std::env::temp_dir().join("magnat_tech_graph_cykl");
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    let p = dir.join("tech.ron");
    std::fs::write(
        &p,
        r#"(
            schema_version: 1,
            branches: [(key: "x")],
            nodes: [
                (key: "a", branch: "x", prereqs: ["b"], cost_rp: 100, world_year: 1970),
                (key: "b", branch: "x", prereqs: ["a"], cost_rp: 100, world_year: 1970),
            ],
        )"#,
    )
    .expect("zapis");
    let Err(TechTreeError::Cycle(v)) = TechTree::load(&p, 1990, &cat) else {
        panic!("walidator przepuścił cykl");
    };
    assert_eq!(v.len(), 2);
}

#[test]
fn nieznana_klasa_maszyn_zatrzymuje_ladowanie() {
    let cat = katalog();
    let dir = std::env::temp_dir().join("magnat_tech_graph_zla_maszyna");
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    let p = dir.join("tech.ron");
    std::fs::write(
        &p,
        r#"(
            schema_version: 1,
            branches: [(key: "x")],
            nodes: [(key: "a", branch: "x", prereqs: [], cost_rp: 100, world_year: 1970,
                     effects: [MachineUpgrade(machine_class: "teleporter", throughput_bps: 100, failure_bps: 0)])],
        )"#,
    )
    .expect("zapis");
    assert!(matches!(
        TechTree::load(&p, 1990, &cat),
        Err(TechTreeError::UnknownMachineClass { .. })
    ));
}

#[test]
fn telefon_jest_jedynym_towarem_poza_obiegiem_na_starcie() {
    // Kryterium WP10.9 stoi na tym, że przed odkryciem towaru w mieście **nie ma**.
    // Gdyby drzewo przestało go blokować, test spirali cenowej i tak by przeszedł —
    // bo telefon po prostu byłby na półkach od pierwszej doby.
    let cat = katalog();
    let t = TechTree::load(&sciezka(), 1990, &cat).expect("tech.ron");
    let zablokowane = magnat_firms::gated_goods(&t);
    let klucze: Vec<&str> = zablokowane.iter().map(|g| cat.key_of(*g)).collect();
    assert_eq!(klucze, vec!["cons_mobile_phone"]);
}
