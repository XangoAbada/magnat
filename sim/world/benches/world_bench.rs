//! Benchmarki generatora świata (M1 §7.3, WP-X1).
//!
//! Progi bezwzględne z §7.3 pilnują **testy** (`tests/budgets.rs`), bo one wiedzą, na jakim
//! sprzęcie biegną. Te benchmarki służą do czegoś innego: bramka regresji w CI porównuje
//! mediany z linią bazową repozytorium (`scripts/bench_guard.py`, decyzja D-8), więc łapie
//! spowolnienie o 10 % nawet wtedy, gdy wynik nadal mieści się w progu.
//!
//! Rozmiary są dobrane tak, żeby criterion zdążył zebrać próbki: pełne 16 km trwa
//! kilkanaście sekund na przebieg, a criterion chce ich dziesięć.

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_jobs::JobPool;
use magnat_world::{generate, Region, WorldGenParams, WorldSize};
use std::hint::black_box;

fn params(size: WorldSize, region: Region) -> WorldGenParams {
    WorldGenParams {
        seed: 0xC0FFEE,
        size,
        region,
        ..WorldGenParams::default()
    }
}

fn generacja(c: &mut Criterion) {
    let pool = JobPool::new(0);
    let mut g = c.benchmark_group("m1-1 generacja");
    // Dziesięć próbek zamiast stu: jeden przebieg 4 km to ponad sekunda, a sto próbek
    // zamieniłoby bramkę regresji w pół godziny oczekiwania na każdy pull request.
    g.sample_size(10);
    for (nazwa, size) in [("4km", WorldSize::Small4km), ("8km", WorldSize::Medium8km)] {
        g.bench_function(nazwa, |b| {
            b.iter(|| black_box(generate(params(size, Region::Mountain), &pool).unwrap().1));
        });
    }
    g.finish();
}

fn przebiegi(c: &mut Criterion) {
    let pool = JobPool::new(0);
    // Raport generacji rozbija czas na przebiegi P1–P12, więc pojedynczy pomiar całości
    // wystarcza, żeby wyciągnąć z niego koszt hydrologii — nie trzeba osobnego benchmarku
    // wołającego wnętrze potoku, którego i tak nie ma w API publicznym.
    let mut g = c.benchmark_group("m1-2 przebiegi 8km");
    g.sample_size(10);
    for region in [Region::Mountain, Region::Lowland] {
        g.bench_function(region.key(), |b| {
            b.iter(|| {
                let (_, r) = generate(params(WorldSize::Medium8km, region), &pool).unwrap();
                black_box(r.total_millis)
            });
        });
    }
    g.finish();
}

criterion_group!(benches, generacja, przebiegi);
criterion_main!(benches);
