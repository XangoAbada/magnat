//! Benchmarki `magnat-nav` — wynik do pokazania dla podfazy M4a: graf 200 tys. węzłów
//! buduje się, kontrahuje i odpowiada na zapytania w budżetach z M4 §7.3.
//!
//! Budżety (M4 §7.3, M4a WP1/WP2):
//!
//! | Benchmark | Budżet |
//! |---|---|
//! | `cch/query passenger` | p95 ≤ 60 µs |
//! | `cch/query heavy_day` | p95 ≤ 60 µs (profil z ograniczeniami jest nieco droższy, §7.3 daje mu 80 µs w budżecie towarowym) |
//! | `cch/kustomizacja 3 profile` | ≤ 450 ms łącznie |
//! | `graph/budowa sieci 100x100x10` | ≤ 400 ms |
//! | `alt/trasa` | brak progu bezwzględnego — mierzy, czy landmarki zwracają swój koszt |
//! | `cache/get + insert` | brak progu — pilnuje regresu narzutu cache wobec 60 µs zapytania |
//!
//! **Instancją odniesienia jest `synthetic_road_network(100, 100, 10)`, nie siatka
//! jednorodna** (`J-11`): obie mają ~188 tys. węzłów, ale siatka `450 × 450` ma
//! szerokość drzewową 450 i mierzyłaby najgorszy przypadek dysekcji zagnieżdżonej
//! zamiast przypadku, który w grze wystąpi. Siatka zostaje w benchmarku ALT, gdzie
//! jest akurat właściwa — trasa piesza po kwartałach jest siatką.
//!
//! Kontrakcja tej instancji trwa sekundy, więc wszystkie ciężkie struktury powstają
//! **raz**, poza `b.iter`.

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_core::TransportMode;
use magnat_core::{rng, Mass, StreamId, Tick};
use magnat_nav::alt::{AltScratch, Landmarks};
use magnat_nav::{
    alt, cch, profile_weights, synthetic_grid, synthetic_road_network, ChGraph, ChScratch,
    ContractionOrder, NodeId, RoadGraph, RouteCache, RouteKey, RouteProfile,
};
use std::hint::black_box;
use std::sync::LazyLock;

/// Instancja odniesienia sieci drogowej: 100 × 100 skrzyżowań po 10 odcinków na ulicę.
const COLS: u32 = 100;
const ROWS: u32 = 100;
const PER_STREET: u32 = 10;
const SPACING_CM: u32 = 10_000;

/// Co siódma **ulica** dostaje limit 10 t — ~14 % krawędzi, deterministycznie po
/// indeksie. Bez tego profil `HeavyDay` miałby te same wagi co `Passenger`
/// i benchmark mierzyłby to samo dwa razy.
///
/// Wyboru nie robi się po pojedynczej krawędzi i nie jest to drobiazg: ulica składa się
/// z `PER_STREET` odcinków, więc losowe 15 % krawędzi zamyka **każdą** ulicę w co najmniej
/// jednym kierunku i profil ciężarowy nie ma dokąd jechać — zapytanie kończy się w 280 ns,
/// bo trasy nie ma nigdzie. Tonaż w mieście też obowiązuje na całej ulicy, nie na 40 m jej
/// środka, więc wybór uliczny jest jednocześnie modelem prawdziwszym i jedynym, który mierzy
/// zapytanie, a nie odmowę.
const TONNAGE_LIMIT: Mass = Mass(10_000_000);

/// Krawędzi na ulicę: `per_street` odcinków × dwa kierunki.
const EDGES_PER_STREET: usize = 2 * PER_STREET as usize;

struct Siec {
    road: RoadGraph,
    ch_pass: ChGraph,
    ch_heavy: ChGraph,
    order: ContractionOrder,
    pary: Vec<(NodeId, NodeId)>,
}

static SIEC: LazyLock<Siec> = LazyLock::new(|| {
    let mut road = synthetic_road_network(COLS, ROWS, PER_STREET, SPACING_CM);
    for (i, e) in road.edges.iter_mut().enumerate() {
        if (i / EDGES_PER_STREET).is_multiple_of(7) {
            e.max_mass = TONNAGE_LIMIT;
        }
    }
    let order = cch::build_order(&road);
    let mut ch_pass = cch::contract(&road, &order);
    ch_pass.customize(
        &road,
        &profile_weights(&road, RouteProfile::Passenger, None),
        road.weight_version,
    );
    let mut ch_heavy = cch::contract(&road, &order);
    ch_heavy.customize(
        &road,
        &profile_weights(&road, RouteProfile::HeavyDay, None),
        road.weight_version,
    );
    let pary = pary(&road, 0xC0FFEE, 4_096);
    Siec {
        road,
        ch_pass,
        ch_heavy,
        order,
        pary,
    }
});

/// Deterministyczne pary origin–dest wśród węzłów, z których da się wyjechać.
fn pary(g: &RoadGraph, seed: u64, n: u32) -> Vec<(NodeId, NodeId)> {
    let zrodla: Vec<u32> = (0..g.node_count() as u32)
        .filter(|&v| !g.out(NodeId(v)).is_empty())
        .collect();
    let mut r = rng(seed, StreamId::EngineSelfTest, 0, Tick(0));
    let mut out = Vec::with_capacity(n as usize);
    while out.len() < n as usize {
        let a = zrodla[r.gen_range_u32(zrodla.len() as u32) as usize];
        let b = zrodla[r.gen_range_u32(zrodla.len() as u32) as usize];
        if a != b {
            out.push((NodeId(a), NodeId(b)));
        }
    }
    out
}

fn cch_query(c: &mut Criterion) {
    let s = &*SIEC;
    println!(
        "siec {}x{}x{}: {} wezlow, {} krawedzi, {} lukow po kontrakcji",
        COLS,
        ROWS,
        PER_STREET,
        s.road.node_count(),
        s.road.edge_count(),
        s.ch_pass.arc_count()
    );

    let mut gr = c.benchmark_group("m4a-2 cch");
    let mut scratch = ChScratch::new();
    let mut i = 0usize;
    gr.bench_function("query passenger (188k wezlow sieci drogowej)", |b| {
        b.iter(|| {
            i = (i + 1) % s.pary.len();
            let (o, d) = s.pary[i];
            black_box(s.ch_pass.query(o, d, &mut scratch))
        })
    });
    gr.bench_function("query heavy_day (co 7. ulica z limitem 10 t)", |b| {
        b.iter(|| {
            i = (i + 1) % s.pary.len();
            let (o, d) = s.pary[i];
            black_box(s.ch_heavy.query(o, d, &mut scratch))
        })
    });
    gr.finish();
}

fn cch_customize(c: &mut Criterion) {
    let s = &*SIEC;
    // Osobny graf, bo kustomizacja bierze `&mut`. Kontrakcja raz, poza pomiarem;
    // sama kustomizacja jest idempotentna, więc wolno ją powtarzać w pętli.
    let mut ch = cch::contract(&s.road, &s.order);
    let wagi: Vec<Vec<u32>> = RouteProfile::ALL
        .iter()
        .map(|p| profile_weights(&s.road, *p, None))
        .collect();

    let mut gr = c.benchmark_group("m4a-2 cch");
    gr.sample_size(10);
    gr.bench_function("kustomizacja 3 profile", |b| {
        b.iter(|| {
            for (k, w) in wagi.iter().enumerate() {
                ch.customize(&s.road, w, s.road.weight_version + k as u32);
            }
            black_box(ch.arc_count())
        })
    });
    gr.finish();
}

fn graph_build(c: &mut Criterion) {
    let mut gr = c.benchmark_group("m4a-1 graph");
    gr.sample_size(10);
    gr.bench_function("budowa sieci 100x100x10", |b| {
        b.iter(|| black_box(synthetic_road_network(COLS, ROWS, PER_STREET, SPACING_CM)))
    });
    gr.finish();
}

fn alt_route(c: &mut Criterion) {
    // Warstwa piesza jest siatką kwartałów, nie siecią ulic — tu siatka jednorodna
    // jest modelem właściwym, a nie najgorszym przypadkiem.
    let g = synthetic_grid(150, 150, 10_000);
    let w = g.free_flow_weights();
    let lm = Landmarks::build(&g, &w, magnat_nav::LANDMARK_COUNT);
    let pary = pary(&g, 7, 1_024);
    let mut scratch = AltScratch::new();
    println!(
        "alt: siatka 150x150 = {} wezlow, {} landmarkow, {} B",
        g.node_count(),
        lm.count(),
        lm.memory_bytes()
    );

    let mut gr = c.benchmark_group("m4a-2 alt");
    let mut i = 0usize;
    gr.bench_function("trasa z landmarkami", |b| {
        b.iter(|| {
            i = (i + 1) % pary.len();
            let (o, d) = pary[i];
            black_box(alt::route(&g, &w, Some(&lm), o, d, &mut scratch))
        })
    });
    gr.bench_function("trasa bez landmarkow (Dijkstra)", |b| {
        b.iter(|| {
            i = (i + 1) % pary.len();
            let (o, d) = pary[i];
            black_box(alt::route(&g, &w, None, o, d, &mut scratch))
        })
    });
    gr.finish();
}

fn route_cache(c: &mut Criterion) {
    let klucze: Vec<RouteKey> = (0..8_192u32)
        .map(|i| RouteKey {
            origin: NodeId(i),
            dest: NodeId(i ^ 0x5A5A),
            mode: TransportMode::Car,
            hour_bucket: (i % 24) as u8,
            profile: RouteProfile::Passenger,
            gross_t: 0,
        })
        .collect();
    let mut cache: RouteCache<u32> = RouteCache::new(1 << 13);
    for (i, k) in klucze.iter().enumerate() {
        cache.insert(*k, i as u32);
    }

    let mut gr = c.benchmark_group("m4a-2 cache");
    let mut i = 0usize;
    gr.bench_function("get (trafienie)", |b| {
        b.iter(|| {
            i = (i + 1) % klucze.len();
            black_box(cache.get(&klucze[i]).copied())
        })
    });
    gr.bench_function("insert", |b| {
        b.iter(|| {
            i = (i + 1) % klucze.len();
            cache.insert(klucze[i], i as u32);
            black_box(cache.stats().len)
        })
    });
    gr.finish();
}

criterion_group!(
    benches,
    graph_build,
    cch_query,
    cch_customize,
    alt_route,
    route_cache
);
criterion_main!(benches);
