//! Benchmarki `magnat-core` (M0 §7.3): B-8 `det_math`, B-9 arena.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use magnat_core::{det_math, Arena, ArenaHandle};
use std::hint::black_box;

/// B-8: `det_math` obok tych samych funkcji z libm platformy.
///
/// Próg: **nie wolniej niż 2×** względem libm. Przekroczenie oznacza optymalizację
/// wielomianu, a nie powrót do libm — libm nie jest deterministyczny między
/// platformami i to jest cały powód istnienia tego modułu (00 §K-6).
fn b8_det_math(c: &mut Criterion) {
    let argumenty: Vec<f64> = (1..=1_000_000).map(|i| f64::from(i) / 1_000.0).collect();

    let mut grupa = c.benchmark_group("B-8 det_math");
    grupa.bench_function("ln (det_math)", |b| {
        b.iter(|| {
            let mut suma = 0.0;
            for x in &argumenty {
                suma += det_math::ln(*x);
            }
            black_box(suma)
        });
    });
    grupa.bench_function("ln (libm platformy)", |b| {
        b.iter(|| {
            let mut suma = 0.0;
            for x in &argumenty {
                // ponytail: libm jako punkt odniesienia pomiaru — benchmark porównuje
                // det_math z tym, co zastępuje. Wynik trafia do `black_box`, nie do stanu.
                #[allow(clippy::disallowed_methods)]
                {
                    suma += x.ln();
                }
            }
            black_box(suma)
        });
    });
    grupa.bench_function("exp (det_math)", |b| {
        b.iter(|| {
            let mut suma = 0.0;
            for x in &argumenty {
                suma += det_math::exp(x / 1_000.0);
            }
            black_box(suma)
        });
    });
    grupa.bench_function("exp (libm platformy)", |b| {
        b.iter(|| {
            let mut suma = 0.0;
            for x in &argumenty {
                // ponytail: libm jako punkt odniesienia pomiaru, jak `ln` wyżej.
                #[allow(clippy::disallowed_methods)]
                {
                    suma += (x / 1_000.0).exp();
                }
            }
            black_box(suma)
        });
    });
    grupa.bench_function("pow (det_math)", |b| {
        b.iter(|| {
            let mut suma = 0.0;
            for x in &argumenty {
                suma += det_math::pow(*x, 1.7);
            }
            black_box(suma)
        });
    });

    // Typowy koszyk M5: 16 opcji do wyboru. Próg §7.3: ≤ 350 ns.
    let wagi: Vec<f64> = (0..16).map(|i| f64::from(i) * 0.37 - 3.0).collect();
    grupa.bench_function("softmax 16 opcji", |b| {
        b.iter(|| black_box(det_math::softmax(&wagi, 0.8)));
    });
    grupa.finish();
}

/// B-9: arena 600 tys. partii. Punkt odniesienia: ta sama liczba encji ECS
/// z komendą strukturalną (B-2) — arena ma być **rząd wielkości** tańsza,
/// inaczej K-16 nie miało sensu.
fn b9_arena(c: &mut Criterion) {
    const N: usize = 600_000;

    let mut grupa = c.benchmark_group("B-9 arena");
    grupa.bench_function("insert 600k", |b| {
        b.iter_batched(
            || Arena::<u64>::with_capacity(N),
            |mut arena| {
                for i in 0..N as u64 {
                    black_box(arena.insert(i));
                }
                arena
            },
            BatchSize::LargeInput,
        );
    });

    let (pelna, uchwyty) = {
        let mut arena = Arena::<u64>::with_capacity(N);
        let uchwyty: Vec<ArenaHandle<u64>> = (0..N as u64).map(|i| arena.insert(i)).collect();
        (arena, uchwyty)
    };

    grupa.bench_function("get po uchwycie", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 7_919) % uchwyty.len();
            black_box(pelna.get(uchwyty[i]))
        });
    });

    // Iteracja po arenie z 30 % dziur — typowy stan po dobie obrotu partiami.
    let dziurawa = {
        let mut arena = Arena::<u64>::with_capacity(N);
        let uchwyty: Vec<ArenaHandle<u64>> = (0..N as u64).map(|i| arena.insert(i)).collect();
        for h in uchwyty.iter().step_by(10).take(N * 3 / 100) {
            arena.remove(*h);
        }
        arena
    };
    grupa.bench_function("pełna iteracja (30 % dziur)", |b| {
        b.iter(|| {
            let mut suma = 0u64;
            for (_, v) in dziurawa.iter() {
                suma = suma.wrapping_add(*v);
            }
            black_box(suma)
        });
    });

    grupa.bench_function("insert + remove", |b| {
        let mut arena = Arena::<u64>::with_capacity(1024);
        b.iter(|| {
            let h = arena.insert(1);
            black_box(arena.remove(h))
        });
    });
    grupa.finish();
}

criterion_group!(benches, b8_det_math, b9_arena);
criterion_main!(benches);
