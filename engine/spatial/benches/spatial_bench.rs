//! Benchmarki `engine/spatial` — wynik do pokazania dla podfazy M2a:
//! cztery struktury odpowiadają na zapytania w budżecie z §5.1, na danych
//! syntetycznych, bez generatora miasta.
//!
//! Budżety (M2 §5.1, tabela „Złożoności i budżety"):
//!
//! | Benchmark | Budżet |
//! |---|---|
//! | `csr/1M zapytań R=250, 8 wątków` | ≤ 180 ms |
//! | `dynamic/przebudowa 400 tys., 8 wątków` | ≤ 3 ms |
//! | `tree/punkt → parcela` | ≤ 2 µs |
//! | `csr/k=15 najbliższych` | ≤ 8 µs |
//! | `field/próbka` | ≤ 30 ns |
//! | `field/dijkstra 1 mln komórek` | ≤ 400 ms |
//!
//! Scenariusz odpowiada metropolii z §7 dokumentu fazy: obszar zurbanizowany
//! ~70 km² (8,4 × 8,4 km), 300 tys. encji statycznych, 400 tys. dynamicznych,
//! 42 tys. parcel o powierzchniach rozpiętych na trzy rzędy wielkości.

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_core::{rng, Entity, ParcelId, StreamId, Tick};
use magnat_jobs::{map_reduce_indexed, JobPool};
use magnat_spatial::{
    Aabb2, BatchResult, CategoryGrid, CellId, CsrGrid, DynamicGrid, GridSpec, ParcelTree,
    ScalarField, Vec2,
};
use std::hint::black_box;
use std::num::NonZeroU32;

/// Bok obszaru zurbanizowanego w metrach (~70 km²).
const BOK: u32 = 8_400;

fn punkty_na(seed: u64, n: u32, bok: u32) -> Vec<Vec2> {
    let mut r = rng(seed, StreamId::EngineSelfTest, 0, Tick(0));
    (0..n)
        .map(|_| Vec2::new(r.gen_range_u32(bok) as f32, r.gen_range_u32(bok) as f32))
        .collect()
}

fn punkty(seed: u64, n: u32) -> Vec<Vec2> {
    punkty_na(seed, n, BOK)
}

fn siatka_na(cell_m: u16, bok: u32) -> GridSpec {
    let n = (bok / u32::from(cell_m)) as u16 + 1;
    GridSpec::new(Vec2::ZERO, cell_m, n, n)
}

fn siatka(cell_m: u16) -> GridSpec {
    siatka_na(cell_m, BOK)
}

/// Zapytania wsadowe bez materializacji wyniku — tak z indeksu korzysta konsument
/// (M3: sąsiedzi agenta, M5: oferty w promieniu): liczy się przejście po trafieniach,
/// nie kopiowanie miliarda identyfikatorów do wektora.
fn wsadowo_zliczaj(g: &CsrGrid<u32>, centra: &[Vec2], r: f32, pool: &JobPool) -> u64 {
    let chunks: Vec<&[Vec2]> = centra.chunks(256).collect();
    map_reduce_indexed(
        pool,
        &chunks,
        |_, chunk| {
            let mut n = 0u64;
            for c in *chunk {
                g.for_each_in_radius(*c, r, |_, t| n += u64::from(t & 1));
            }
            n
        },
        |a, b| a + b,
        0u64,
    )
}

fn csr(c: &mut Criterion) {
    let spec = siatka(64);
    let pos = punkty(1, 300_000);
    let g = CsrGrid::build(spec, pos.iter().copied().zip(0u32..));
    let pool = JobPool::new(8);
    let centra = punkty(2, 1_000_000);

    let s = g.stats();
    println!(
        "csr: {} encji, {} komórek zajętych z {}, max {} w komórce, średnio {:.1}",
        s.items,
        s.occupied_cells,
        s.cells,
        s.max_per_cell,
        s.avg_per_occupied()
    );

    // Ten sam milion zapytań na całej mapie 16 × 16 km (M1): te same 300 tys. encji,
    // ale gęstość 1,2 tys./km² zamiast 4,3 tys./km². Koszt zapytania promieniowego
    // jest zdominowany przez `m` (liczbę trafień), nie przez `k` (liczbę komórek),
    // więc te dwa przypadki różnią się kilkakrotnie — patrz korekta budżetu w §5.1.
    let spec_mapa = siatka_na(64, 16_000);
    let pos_mapa = punkty_na(11, 300_000, 16_000);
    let g_mapa = CsrGrid::build(spec_mapa, pos_mapa.iter().copied().zip(0u32..));
    let centra_mapa = punkty_na(12, 1_000_000, 16_000);

    let mut gr = c.benchmark_group("m2a-1 csr");
    gr.sample_size(10);
    gr.bench_function("1M zapytan R=250, 8 watkow (miasto 70 km2)", |b| {
        b.iter(|| black_box(wsadowo_zliczaj(&g, &centra, 250.0, &pool)))
    });
    gr.bench_function("1M zapytan R=250, 8 watkow (mapa 256 km2)", |b| {
        b.iter(|| black_box(wsadowo_zliczaj(&g_mapa, &centra_mapa, 250.0, &pool)))
    });
    gr.sample_size(100);
    gr.bench_function("pojedyncze zapytanie R=250", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % centra.len();
            let mut n = 0u32;
            g.for_each_in_radius(centra[i], 250.0, |_, t| n += t & 1);
            black_box(n)
        })
    });
    gr.bench_function("k=15 najblizszych", |b| {
        let mut out = Vec::new();
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % centra.len();
            g.k_nearest(centra[i], 15, &mut out);
            black_box(out.len())
        })
    });
    gr.bench_function("query_rect 200x200 m", |b| {
        let mut out = Vec::new();
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % centra.len();
            g.query_rect(Aabb2::from_center_radius(centra[i], 100.0), &mut out);
            black_box(out.len())
        })
    });
    gr.sample_size(10);
    gr.bench_function("query_radius_batch 100k x R=60", |b| {
        let mut out = BatchResult::new();
        b.iter(|| {
            g.query_radius_batch(&centra[..100_000], 60.0, &pool, &mut out);
            black_box(out.total())
        })
    });
    gr.bench_function("budowa indeksu 300 tys.", |b| {
        b.iter(|| black_box(CsrGrid::build(spec, pos.iter().copied().zip(0u32..)).len()))
    });
    gr.finish();
}

fn dynamic(c: &mut Criterion) {
    let spec = siatka(32);
    let pos = punkty(3, 400_000);
    let ids: Vec<u32> = (0..400_000).collect();

    let mut gr = c.benchmark_group("m2a-2 dynamic");
    gr.sample_size(20);
    for threads in [1usize, 8] {
        let pool = JobPool::new(threads);
        let mut g = DynamicGrid::new(spec);
        gr.bench_function(format!("przebudowa 400 tys., {threads} watkow"), |b| {
            b.iter(|| {
                g.rebuild(&pos, &ids, &pool);
                black_box(g.len())
            })
        });
    }
    gr.finish();
}

fn parcels(c: &mut Criterion) {
    // 42 tys. parcel: co pięćdziesiąta to pole rolne rzędu hektarów, reszta to działki
    // 10–40 m. Rozpiętość jest sednem testu — na jednorodnych działkach każde drzewo wygra.
    let mut r = rng(4, StreamId::EngineSelfTest, 0, Tick(0));
    let items: Vec<(Aabb2, ParcelId)> = (0..42_000u32)
        .map(|i| {
            let x = r.gen_range_u32(BOK) as f32;
            let y = r.gen_range_u32(BOK) as f32;
            let bok = if i % 50 == 0 {
                300.0 + r.gen_range_u32(200) as f32
            } else {
                10.0 + r.gen_range_u32(30) as f32
            };
            (
                Aabb2::new(Vec2::new(x, y), Vec2::new(x + bok, y + bok)),
                ParcelId(Entity::new(i + 1, NonZeroU32::new(1).unwrap())),
            )
        })
        .collect();
    let t = ParcelTree::build(&items);
    println!("parcel_tree: {} parcel, {} węzłów", t.len(), t.node_count());
    let zapytania = punkty(5, 100_000);

    let mut gr = c.benchmark_group("m2a-3 tree");
    gr.bench_function("punkt -> parcela", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % zapytania.len();
            black_box(t.at_point(zapytania[i], |_| true))
        })
    });
    gr.bench_function("prostokat 300x300 m", |b| {
        let mut out = Vec::new();
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % zapytania.len();
            t.query_rect(Aabb2::from_center_radius(zapytania[i], 150.0), &mut out);
            black_box(out.len())
        })
    });
    gr.bench_function("odcinek 500 m", |b| {
        let mut out = Vec::new();
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % zapytania.len();
            t.query_segment(zapytania[i], zapytania[i] + Vec2::splat(354.0), &mut out);
            black_box(out.len())
        })
    });
    gr.sample_size(20);
    gr.bench_function("budowa 42 tys. parcel", |b| {
        b.iter(|| black_box(ParcelTree::build(&items).node_count()))
    });
    gr.finish();
}

fn fields(c: &mut Criterion) {
    // Pole 16 m dla całej mapy 16 km = 1 mln komórek (§5.1, „odległość po drogach").
    let spec = GridSpec::new(Vec2::ZERO, 16, 1_000, 1_000);
    let zrodla: Vec<CellId> = (0..64).map(|i| CellId(i * 15_731 % 1_000_000)).collect();
    let pole = ScalarField::multi_source_dijkstra(spec, &zrodla, |_, _| 10);
    let zapytania = punkty(6, 100_000);

    let mut gr = c.benchmark_group("m2a-4 field");
    gr.bench_function("probka dwuliniowa", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % zapytania.len();
            black_box(pole.sample(zapytania[i]))
        })
    });
    gr.sample_size(10);
    gr.bench_function("dijkstra 1 mln komorek", |b| {
        b.iter(|| {
            black_box(ScalarField::multi_source_dijkstra(spec, &zrodla, |_, _| 10).full_scale())
        })
    });
    gr.bench_function("combine 2 pol 1 mln komorek", |b| {
        b.iter(|| black_box(ScalarField::combine(&[(&pole, 0.5), (&pole, 0.5)]).full_scale()))
    });
    gr.finish();
}

fn categories(c: &mut Criterion) {
    // PRD §17.5: dużo kategorii, mało encji w każdej — 40 kategorii ofert po 750 sztuk.
    let spec = siatka(128);
    let pos = punkty(7, 30_000);
    let cg = CategoryGrid::build(
        spec,
        pos.iter()
            .enumerate()
            .map(|(i, p)| ((i % 40) as u8, *p, i as u32)),
    );
    let zapytania = punkty(8, 100_000);

    let mut gr = c.benchmark_group("m2a-5 category");
    gr.bench_function("oferty kategorii w R=600", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % zapytania.len();
            let mut n = 0u32;
            cg.for_each_in_radius((i % 40) as u8, zapytania[i], 600.0, |_, t| n += t & 1);
            black_box(n)
        })
    });
    gr.finish();
}

criterion_group!(benches, csr, dynamic, parcels, fields, categories);
criterion_main!(benches);
