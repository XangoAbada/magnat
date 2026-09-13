//! Benchmarki ECS (M0 §7.3, WP-13).
//!
//! Progi bezwzględne z §7.3 są **celami projektowymi** na sprzęcie odniesienia;
//! bramką CI jest regresja względna wobec `benches/baseline.json` (10 % ostrzeżenie,
//! 25 % błąd). Dlatego benchmark mierzy, a nie asercjuje.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use magnat_core::{Cadence, HashState, StateHasher};
use magnat_ecs::{
    flush_commands, App, CommandBuffer, Component, Entity, ScheduleBuilder, System, SystemCtx,
    SystemDesc, SystemId, With, Without, World,
};
use magnat_jobs::JobPool;
use std::hint::black_box;

macro_rules! komponent {
    ($name:ident, $inner:ty) => {
        #[derive(Debug, Clone, Copy)]
        struct $name($inner);
        impl HashState for $name {
            fn hash_state(&self, h: &mut StateHasher) {
                h.write(&self.0.to_le_bytes());
            }
        }
        impl Component for $name {
            const NAME: &'static str = stringify!($name);
        }
    };
}

komponent!(A, i64);
komponent!(B, i64);
komponent!(C, i64);
komponent!(D, i64);
komponent!(E, i64);
komponent!(F, i64);

#[derive(Debug, Clone, Copy)]
struct Bulk([u8; 400]);
impl HashState for Bulk {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&self.0);
    }
}
impl Component for Bulk {
    const NAME: &'static str = "Bulk";
}

const N: i64 = 400_000;

fn swiat_jeden_archetyp() -> World {
    let mut w = World::new(1);
    for i in 0..N {
        let _ = w.spawn().with(A(i)).with(B(i * 2)).id();
    }
    w
}

fn swiat_szesc_komponentow() -> World {
    let mut w = World::new(1);
    for i in 0..N {
        let mut e = w.spawn().with(A(i)).with(B(i)).with(C(i));
        match i % 4 {
            0 => e = e.with(D(i)).with(E(i)).with(F(i)),
            1 => e = e.with(D(i)).with(E(i)),
            2 => e = e.with(D(i)).with(F(i)),
            _ => {}
        }
        let _ = e.id();
    }
    w
}

/// B-1: iteracja po 400 tys. encji, dwa komponenty po 16 B, jeden archetyp.
fn b1_iteracja(c: &mut Criterion) {
    let mut w = swiat_jeden_archetyp();
    c.bench_function("B-1 iteracja 400k (2 komponenty)", |bench| {
        bench.iter(|| {
            let mut suma = 0i64;
            for (a, b) in w.query::<(&mut A, &B), ()>().iter() {
                a.0 = a.0.wrapping_add(b.0);
                suma = suma.wrapping_add(a.0);
            }
            black_box(suma)
        });
    });
}

/// B-1b: realistyczny profil — 6 komponentów, 4 archetypy, filtry.
fn b1b_iteracja_z_filtrem(c: &mut Criterion) {
    let mut w = swiat_szesc_komponentow();
    c.bench_function("B-1b iteracja 400k (6 komponentów, filtry)", |bench| {
        bench.iter(|| {
            let mut suma = 0i64;
            for (a, b, cc) in w.query::<(&mut A, &B, &C), (With<D>, Without<F>)>().iter() {
                a.0 = a.0.wrapping_add(b.0 ^ cc.0);
                suma = suma.wrapping_add(a.0);
            }
            black_box(suma)
        });
    });
}

/// B-1c: to samo równolegle na 16 wątkach — mierzy skalowanie.
fn b1c_iteracja_rownolegla(c: &mut Criterion) {
    let mut w = swiat_szesc_komponentow();
    let pool = JobPool::new(16);
    c.bench_function("B-1c par_for_each 400k, 16 wątków", |bench| {
        bench.iter(|| {
            w.query::<(&mut A, &B, &C), (With<D>, Without<F>)>()
                .par_for_each(&pool, |(a, b, cc)| {
                    a.0 = a.0.wrapping_add(b.0 ^ cc.0);
                });
        });
    });
}

/// B-2: koszt zgłoszenia pojedynczej komendy strukturalnej.
fn b2_komenda(c: &mut Criterion) {
    c.bench_function("B-2 zgłoszenie komendy (spawn+insert)", |bench| {
        let mut buf = CommandBuffer::new(SystemId::from_name("bench"));
        bench.iter(|| {
            let r = buf.spawn();
            buf.insert_reserved(r, A(1));
            if buf.len() > 10_000 {
                buf.clear();
            }
        });
    });
}

/// B-2b: flush 100 tys. komend — sortowanie plus aplikacja.
fn b2b_flush(c: &mut Criterion) {
    c.bench_function("B-2b flush 100k komend", |bench| {
        bench.iter_batched(
            || {
                let mut w = World::new(1);
                let encje: Vec<Entity> = (0..50_000).map(|i| w.spawn().with(A(i)).id()).collect();
                let mut bufory: Vec<CommandBuffer> = (0..16)
                    .map(|i| CommandBuffer::new(SystemId::from_name(&format!("bench{i}"))))
                    .collect();
                for (i, e) in encje.iter().enumerate() {
                    let buf = &mut bufory[i % 16];
                    buf.insert(*e, B(i as i64));
                    if i % 10 == 0 {
                        buf.despawn(*e);
                    } else {
                        let r = buf.spawn();
                        buf.insert_reserved(r, A(i as i64));
                    }
                }
                (w, bufory)
            },
            |(mut w, mut bufory)| {
                black_box(flush_commands(&mut w, &mut bufory));
            },
            BatchSize::LargeInput,
        );
    });
}

/// B-3: przeniesienie encji między archetypami przy wierszu 400 B.
fn b3_przeniesienie(c: &mut Criterion) {
    c.bench_function("B-3 zmiana archetypu (wiersz 400 B)", |bench| {
        bench.iter_batched(
            || {
                let mut w = World::new(1);
                let encje: Vec<Entity> = (0..20_000)
                    .map(|i| w.spawn().with(A(i)).with(Bulk([7; 400])).id())
                    .collect();
                (w, encje)
            },
            |(mut w, encje)| {
                for e in &encje {
                    w.insert(*e, B(1));
                }
                for e in &encje {
                    black_box(w.remove::<B>(*e));
                }
            },
            BatchSize::LargeInput,
        );
    });
}

struct Pusty(SystemDesc);
impl System for Pusty {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, _ctx: &mut SystemCtx<'_>) {}
}

/// B-5: narzut bariery i harmonogramu przy 60 pustych systemach.
fn b5_bariera(c: &mut Criterion) {
    let world = World::new(1);
    let mut builder = ScheduleBuilder::new();
    for i in 0..60 {
        let name: &'static str = Box::leak(format!("bench.system{i}").into_boxed_str());
        builder.add(Pusty(SystemDesc::new(name, Cadence::EveryMinute)));
    }
    let mut app = App::new(world, builder.build().expect("harmonogram"), 16);
    c.bench_function("B-5 tick z 60 pustymi systemami, 16 wątków", |bench| {
        bench.iter(|| app.tick());
    });
}

criterion_group!(
    benches,
    b1_iteracja,
    b1b_iteracja_z_filtrem,
    b1c_iteracja_rownolegla,
    b2_komenda,
    b2b_flush,
    b3_przeniesienie,
    b5_bariera
);
criterion_main!(benches);
