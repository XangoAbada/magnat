//! Benchmarki fundamentu agenta (M3 §7.5, podfaza M3a).
//!
//! Progi bezwzględne z §7.5 pilnują **testy** (`tests/budgets.rs`) — one wiedzą,
//! na jakim sprzęcie biegną. Te benchmarki karmią bramkę regresji `bench_guard.py`,
//! która porównuje mediany z linią bazową repozytorium i łapie spowolnienie o 10 %
//! nawet wtedy, gdy wynik nadal mieści się w progu (D-8).

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_agents::{
    register, AgentState, DeprivationEffectsSystem, EventKind, EventQueue, Identity, Lifecycle,
    NeedDecaySystem, NeedTable, Needs, Personality, PlanRef, Residence, SimEvent, Skills, Vitals,
    Wealth,
};
use magnat_agents::{Employment, KnowledgeRef, RelationsRef};
use magnat_ecs::{App, ScheduleBuilder, World};
use std::hint::black_box;

fn tabela() -> NeedTable {
    NeedTable::load_default().expect("data/needs/needs.ron")
}

fn swiat(n: u32) -> World {
    let mut w = World::new(7);
    register(&mut w, tabela());
    for i in 0..n {
        let _ = w
            .spawn()
            .with(Identity {
                birth_day: -(360 * 30) - i as i32 % 10_000,
                flags: Identity::FLAG_ALIVE,
                household: i / 3,
                ..Identity::default()
            })
            .with(Personality([50; 8]))
            .with(Vitals {
                health: 90,
                energy: 80,
                ..Vitals::default()
            })
            .with(Needs::default())
            .with(Skills::default())
            .with(Wealth::default())
            .with(Employment::default())
            .with(Residence::default())
            .with(PlanRef::default())
            .with(AgentState::default())
            .with(KnowledgeRef::default())
            .with(RelationsRef::default())
            .with(Lifecycle::default());
    }
    w
}

/// Rozkład zdarzeń w dobie: średnio 20 na mieszkańca, ze szczytem porannym
/// i popołudniowym. Bez szczytów benchmark mierzyłby przypadek, który nie występuje.
fn zaplanuj_dobe(q: &mut EventQueue, agenci: u32) {
    for a in 0..agenci {
        for k in 0..20u32 {
            // Rozrzut po minutach doby, zagęszczony wokół 7:00 i 16:00.
            let baza = match k % 5 {
                0 => 420, // 7:00
                1 => 960, // 16:00
                2 => 720, // 12:00
                3 => 1_140,
                _ => 300,
            };
            let minuta = (baza + (a.wrapping_mul(2_654_435_761).wrapping_add(k) % 90)) % 1_440;
            let kind = match k % 4 {
                0 => EventKind::Arrive,
                1 => EventKind::NeedTick,
                2 => EventKind::StartActivity,
                _ => EventKind::EndActivity,
            };
            q.schedule(SimEvent::new(minuta, a, kind, k as u8));
        }
    }
}

fn des(c: &mut Criterion) {
    let mut g = c.benchmark_group("m3a-1 des");

    // Średnia dla 200 tys. agentów: 4 mln zdarzeń / 1440 minut = 2 778 na tick.
    g.bench_function("dispatch 2778 zdarzen", |b| {
        b.iter_batched(
            || {
                let mut q = EventQueue::new();
                for i in 0..2_778u32 {
                    q.schedule(SimEvent::new(0, i, EventKind::Arrive, (i % 24) as u8));
                }
                (q, Vec::with_capacity(4_096))
            },
            |(mut q, mut out)| {
                q.drain_minute(&mut out);
                black_box(out.len())
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Szczyt: 4× średniej.
    g.bench_function("dispatch 11000 zdarzen", |b| {
        b.iter_batched(
            || {
                let mut q = EventQueue::new();
                for i in 0..11_000u32 {
                    q.schedule(SimEvent::new(
                        0,
                        i,
                        EventKind::StartActivity,
                        (i % 24) as u8,
                    ));
                }
                (q, Vec::with_capacity(16_384))
            },
            |(mut q, mut out)| {
                q.drain_minute(&mut out);
                black_box(out.len())
            },
            criterion::BatchSize::SmallInput,
        );
    });

    // Pełna doba: 200 tys. agentów × 20 zdarzeń = 4 mln, cel ≤ 250 ms na 1 wątku.
    g.sample_size(10);
    g.bench_function("doba 200 tys. agentow", |b| {
        b.iter_batched(
            || {
                let mut q = EventQueue::new();
                zaplanuj_dobe(&mut q, 200_000);
                (q, Vec::with_capacity(32_768))
            },
            |(mut q, mut out)| {
                let mut suma = 0usize;
                for _ in 0..1_440 {
                    q.drain_minute(&mut out);
                    suma += out.len();
                }
                black_box(suma)
            },
            criterion::BatchSize::LargeInput,
        );
    });
    g.finish();
}

fn potrzeby(c: &mut Criterion) {
    let mut g = c.benchmark_group("m3a-2 potrzeby");
    g.sample_size(20);

    // Dwa systemy mierzone osobno, bo mają różną częstotliwość i różny koszt:
    // spadek biegnie co minutę na 1/60 populacji, skutki deprywacji co godzinę
    // na całej. Wspólny pomiar nie powiedziałby, który z nich rośnie.
    //
    // Osiem wątków, nie jeden: oba systemy idą `par_for_each` po chunkach, a cel
    // z §7.5 dotyczy kosztu ticku w symulacji, nie kosztu jednego rdzenia.
    let mut swiat_spadku = swiat(400_000);
    let plan_spadku = {
        let mut b = ScheduleBuilder::new();
        b.add(NeedDecaySystem::new(&swiat_spadku));
        b.build().expect("harmonogram")
    };
    swiat_spadku.tick = magnat_core::Tick(0);
    let mut app_spadku = App::new(swiat_spadku, plan_spadku, 8);
    g.bench_function("tick 400 tys. (spadek, shard 1/60)", |b| {
        b.iter(|| {
            app_spadku.tick();
            black_box(app_spadku.world.tick.0)
        });
    });

    let mut swiat_deprywacji = swiat(400_000);
    let plan_deprywacji = {
        let mut b = ScheduleBuilder::new();
        b.add(DeprivationEffectsSystem::new(&swiat_deprywacji));
        b.build().expect("harmonogram")
    };
    // Zegar ustawiony tak, żeby każdy tick wypadał na granicy godziny — inaczej
    // `Cadence::EveryHour` pominąłby system i benchmark mierzyłby pusty przebieg.
    swiat_deprywacji.tick = magnat_core::Tick(59);
    let mut app_deprywacji = App::new(swiat_deprywacji, plan_deprywacji, 8);
    g.bench_function("godzina 400 tys. (skutki deprywacji)", |b| {
        b.iter(|| {
            app_deprywacji.tick();
            app_deprywacji.world.tick = magnat_core::Tick(59);
            black_box(app_deprywacji.world.tick.0)
        });
    });
    g.finish();
}

fn populacja(c: &mut Criterion) {
    let mut g = c.benchmark_group("m3a-3 populacja");
    g.sample_size(10);
    g.bench_function("spawn 400 tys. mieszkancow", |b| {
        b.iter(|| black_box(swiat(400_000).entity_count()));
    });
    g.finish();
}

criterion_group!(benches, des, potrzeby, populacja);
criterion_main!(benches);
