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
            .with(magnat_agents::BrandsRef::default())
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

criterion_group!(
    benches,
    des,
    potrzeby,
    populacja,
    planer,
    ruch,
    spoleczenstwo
);
criterion_main!(benches);

// ── M3b: planer dnia i ruch pieszy ──────────────────────────────────────────────

/// Miasto testowe: siatka 20 × 20 węzłów co 150 m (≈ 3 × 3 km) i sto miejsc.
/// Graf jest realistycznej wielkości, żeby Dijkstra miała po czym chodzić —
/// na czterech węzłach każdy routing jest darmowy i benchmark kłamałby.
type Miasto = (
    std::sync::Arc<magnat_agents::PlaceTable>,
    Vec<magnat_core::WorldCoord>,
    Vec<(u32, u32, u32)>,
);

fn miasto() -> Miasto {
    use magnat_agents::{PlaceEntry, PlaceTable};
    use magnat_core::{BuildingId, Entity, PlaceKind, WorldCoord};

    const N: i32 = 20;
    const KROK: i32 = 15_000; // 150 m w centymetrach
    let mut nodes = Vec::new();
    for y in 0..N {
        for x in 0..N {
            nodes.push(WorldCoord::new(x * KROK, y * KROK, 0));
        }
    }
    let idx = |x: i32, y: i32| (y * N + x) as u32;
    let mut segs = Vec::new();
    for y in 0..N {
        for x in 0..N {
            if x + 1 < N {
                segs.push((idx(x, y), idx(x + 1, y), KROK as u32));
            }
            if y + 1 < N {
                segs.push((idx(x, y), idx(x, y + 1), KROK as u32));
            }
        }
    }

    let encja = |i: u32| Entity::new(i, std::num::NonZeroU32::new(1).unwrap());
    let mut wpisy = Vec::new();
    let rodzaje = [
        PlaceKind::Grocery,
        PlaceKind::Eatery,
        PlaceKind::Doctor,
        PlaceKind::Clothing,
        PlaceKind::Leisure,
    ];
    wpisy.push(PlaceEntry {
        place: magnat_core::PlaceRef::Building(BuildingId(encja(1))),
        kind: PlaceKind::Home,
        at: WorldCoord::new(0, 0, 0),
    });
    wpisy.push(PlaceEntry {
        place: magnat_core::PlaceRef::Site(magnat_core::SiteId(encja(2))),
        kind: PlaceKind::Workplace,
        at: WorldCoord::new((N - 1) * KROK, (N - 1) * KROK, 0),
    });
    for i in 0..100u32 {
        let x = (i * 7 % N as u32) as i32;
        let y = (i * 13 % N as u32) as i32;
        wpisy.push(PlaceEntry {
            place: magnat_core::PlaceRef::Building(BuildingId(encja(100 + i))),
            kind: rodzaje[i as usize % rodzaje.len()],
            at: WorldCoord::new(x * KROK + 300, y * KROK + 300, 0),
        });
    }
    (std::sync::Arc::new(PlaceTable::build(wpisy)), nodes, segs)
}

/// Stan mieszkańca żyjący dłużej niż `PlanCtx` — kontekst trzyma same referencje.
struct Mieszkaniec {
    identity: Identity,
    vitals: Vitals,
    potrzeby: Needs,
    personality: Personality,
    residence: Residence,
    employment: Employment,
    wiedza: Vec<magnat_agents::Knowledge>,
    stock: [u8; magnat_core::STOCK_CAT_COUNT],
    escorts: Vec<magnat_core::PlaceRef>,
    tabela: std::sync::Arc<NeedTable>,
    miejsca: magnat_agents::InfinitePlaces,
    oracle: magnat_agents::StraightLineTravel,
}

fn mieszkaniec() -> Mieszkaniec {
    use magnat_agents::{InfinitePlaces, Knowledge, KnowledgeKind, StraightLineTravel};
    let (places, _nodes, _segs) = miasto();
    let tabela = std::sync::Arc::new(tabela());
    let mut stock = [30u8; magnat_core::STOCK_CAT_COUNT];
    stock[0] = 1;
    Mieszkaniec {
        identity: Identity {
            birth_day: -360 * 34,
            flags: Identity::FLAG_ALIVE,
            ..Identity::default()
        },
        vitals: Vitals {
            health: 80,
            energy: 70,
            ..Vitals::default()
        },
        potrzeby: Needs {
            level: [45; 12],
            updated_at: 0,
        },
        personality: Personality([55, 50, 60, 50, 45, 65, 40, 70]),
        residence: Residence::default(),
        employment: Employment {
            site: 2,
            work_days: Employment::WEEKDAYS,
            ..Employment::default()
        },
        // Mieszkaniec zna szesnaście miejsc — dokładnie tyle, ile wynosi twardy
        // limit kandydatów (R6), więc `candidates` pracuje w najgorszym przypadku.
        wiedza: (0..16u32)
            .map(|i| Knowledge {
                target: 100 + i,
                day: 0,
                score: 70,
                kind: KnowledgeKind::Visited as u8,
            })
            .collect(),
        stock,
        escorts: Vec::new(),
        miejsca: InfinitePlaces::new(places.clone(), tabela.clone()),
        oracle: StraightLineTravel::new(places.clone()),
        tabela,
    }
}

impl Mieszkaniec {
    fn widok(&self) -> magnat_agents::CitizenView<'_> {
        magnat_agents::CitizenView {
            id: magnat_core::CitizenId(magnat_core::Entity::new(
                77,
                std::num::NonZeroU32::new(1).unwrap(),
            )),
            identity: &self.identity,
            vitals: &self.vitals,
            needs: &self.potrzeby,
            personality: &self.personality,
            residence: &self.residence,
            today: 3,
            brands: Default::default(),
        }
    }

    fn ctx(&self) -> magnat_agents::PlanCtx<'_> {
        magnat_agents::PlanCtx {
            seed: 4242,
            day: 3,
            dow: magnat_core::DayOfWeek::Thursday,
            citizen: self.widok(),
            household: magnat_agents::HouseholdView {
                id: magnat_core::HouseholdId(magnat_core::Entity::new(
                    100,
                    std::num::NonZeroU32::new(1).unwrap(),
                )),
                stock: &self.stock,
                escorts: &self.escorts,
                pickups: &self.escorts,
                unescorted: 0,
            },
            employment: &self.employment,
            known: magnat_agents::KnowledgeView::new(&self.wiedza),
            needs: &self.tabela,
            home: dom(),
            work: Some(praca()),
            school: None,
            places: &self.miejsca,
            travel: &self.oracle,
            max_task_travel_min: 30,
        }
    }
}

fn dom() -> magnat_core::PlaceRef {
    magnat_core::PlaceRef::Building(magnat_core::BuildingId(magnat_core::Entity::new(
        1,
        std::num::NonZeroU32::new(1).unwrap(),
    )))
}

fn praca() -> magnat_core::PlaceRef {
    magnat_core::PlaceRef::Site(magnat_core::SiteId(magnat_core::Entity::new(
        2,
        std::num::NonZeroU32::new(1).unwrap(),
    )))
}

/// Planer dnia (§7.5: mediana ≤ 10 µs, p99 ≤ 30 µs na jednym wątku).
///
/// Mierzone na mieszkańcu znającym szesnaście miejsc, czyli przy pełnym obłożeniu
/// listy kandydatów (ryzyko R6). Kosztowną częścią wyboru miejsca jest w M5 funkcja
/// użyteczności §6.4 po stronie `PlaceProvider` — tu mierzymy to, za co odpowiada M3.
fn planer(c: &mut Criterion) {
    use magnat_agents::{plan_day, plan_day_explained, replan, DayCanvas, ReasonLog, ReplanCause};
    use magnat_core::{MinuteOfDay, NeedKind};

    let m = mieszkaniec();
    let mut g = c.benchmark_group("m3b-1 planer");

    g.bench_function("plan_day", |b| {
        let ctx = m.ctx();
        let mut canvas = DayCanvas::new();
        b.iter(|| {
            plan_day(black_box(&ctx), &mut canvas);
            black_box(canvas.len())
        });
    });

    g.bench_function("plan_day_explained", |b| {
        let ctx = m.ctx();
        let mut canvas = DayCanvas::new();
        let mut log = ReasonLog::new();
        b.iter(|| {
            plan_day_explained(black_box(&ctx), &mut canvas, &mut log);
            black_box(log.entries().len())
        });
    });

    g.bench_function("replan", |b| {
        let ctx = m.ctx();
        let mut wzorzec = DayCanvas::new();
        plan_day(&ctx, &mut wzorzec);
        b.iter(|| {
            let mut canvas = wzorzec;
            replan(
                black_box(&ctx),
                MinuteOfDay::new(10 * 60),
                ReplanCause::NeedCritical {
                    need: NeedKind::Hunger,
                },
                &mut canvas,
            );
            black_box(canvas.len())
        });
    });
    g.finish();
}

/// Ruch pieszy (§7.5: trafienie cache ≤ 0,2 µs; 5 tys. pieszych w Mikro ≤ 2 ms/klatkę).
///
/// Przebieg zimny — pierwsza Dijkstra dla pary miejsc — mierzy `tests/budgets.rs`,
/// bo on umie wyczyścić stan między pomiarami, a criterion z definicji powtarza
/// ten sam pomiar i od drugiego razu trafiałby w cache.
fn ruch(c: &mut Criterion) {
    use magnat_agents::{EventQueue, TravelOracle, TripRequest};
    use magnat_core::{CitizenId, Entity, MinuteOfDay};

    let m = mieszkaniec();
    let widok = m.widok();
    let mut g = c.benchmark_group("m3b-2 ruch");

    g.bench_function("estimate (trafienie cache)", |b| {
        b.iter(|| {
            black_box(
                m.oracle
                    .estimate(dom(), praca(), MinuteOfDay::new(8 * 60), &widok),
            )
        });
    });

    g.bench_function("mikro 5 tys. pieszych", |b| {
        let mut oracle = mieszkaniec();
        let mut q = EventQueue::new();
        for i in 0..5_000u32 {
            let handle = oracle.oracle.begin_trip(
                TripRequest {
                    traveller: CitizenId(Entity::new(i + 1, std::num::NonZeroU32::new(1).unwrap())),
                    from: dom(),
                    to: praca(),
                    depart: MinuteOfDay::new(8 * 60),
                    slot: 0,
                },
                &widok,
                &mut q,
            );
            oracle
                .oracle
                .enter_micro(&handle, i, MinuteOfDay::new(8 * 60));
        }
        let mut ms = 8 * 60 * 60_000u64;
        b.iter(|| {
            ms += 100;
            oracle.oracle.micro_step(black_box(ms));
        });
    });
    g.finish();
}

// ── M3c: demografia, migracja, plotka ───────────────────────────────────────────

/// Miasto do pomiaru społeczeństwa: `n` lokali, `n` etatów, zasiew do pełna.
///
/// Jedna doba dotyka **1/360 populacji** hazardami i **1/7** relacjami i plotką, więc
/// mierzony jest realny koszt doby, a nie koszt pełnego przebiegu po wszystkich.
fn swiat_spoleczny(gospodarstw: usize) -> magnat_ecs::World {
    use magnat_agents::{society, CityFacts, DemographyTable, HomeSlot, JobSlot, Vacancies};
    use magnat_core::Money;

    let mut w = magnat_ecs::World::new(11);
    register(&mut w, tabela());
    society::register_society(
        &mut w,
        DemographyTable::load_default().expect("data/demography/demography.ron"),
    );

    let n = (gospodarstw * 2) as u32;
    let domy: Vec<HomeSlot> = (0..n)
        .map(|i| HomeSlot {
            building: 1_000_000 + i,
            unit: 0,
            district: (i / 16) as u16,
            value: Money(1_000_000),
        })
        .collect();
    let etaty: Vec<JobSlot> = (0..n)
        .map(|i| JobSlot {
            site: 2_000_000 + i / 8,
            role: (i % 40) as u16,
            shift: 1,
            work_days: 0b001_1111,
            district: (i / 16) as u16,
            wage_monthly: Money(300_000),
        })
        .collect();
    *w.resource_mut::<Vacancies>() = Vacancies::new(domy, etaty);
    *w.resource_mut::<CityFacts>() = CityFacts {
        job_prestige: Vec::new(),
        district_score: vec![50; (n / 16 + 1) as usize],
        block_of: (0..1_000_000 + n).map(|i| i / 9).collect(),
    };
    magnat_agents::seed_population(&mut w, 0, gospodarstw);
    w
}

fn spoleczenstwo(c: &mut Criterion) {
    use magnat_agents::{demography, social, society, NoInheritance};

    let mut g = c.benchmark_group("m3c-1 spoleczenstwo");
    g.sample_size(20);

    // §7.5: `bench_gossip_day` — dobowa plotka. Shard 1/7 znaczy, że doba dotyka
    // siódmej części populacji; próg 20 ms dotyczy 400 tys. mieszkańców, tu mierzymy
    // koszt jednostkowy, a bramka regresji porównuje go z linią bazową (D-8).
    g.bench_function("plotka i relacje, doba 20 tys. mieszkańców", |b| {
        let mut w = swiat_spoleczny(10_000);
        let mut hooks = NoInheritance;
        // Rozgrzewka: bez dojrzałych relacji plotka nie ma komu opowiadać i pomiar
        // mierzyłby pustą pętlę.
        for d in 0..90 {
            society::step_day(&mut w, d, &mut hooks);
        }
        let mut dzien = 90u64;
        b.iter(|| {
            dzien += 1;
            black_box(social::step_day(&mut w, dzien));
        });
    });

    g.bench_function("hazardy demograficzne, doba 20 tys. mieszkańców", |b| {
        let mut w = swiat_spoleczny(10_000);
        let mut hooks = NoInheritance;
        let mut dzien = 0u64;
        b.iter(|| {
            dzien += 1;
            black_box(demography::step_day(&mut w, dzien, &mut hooks));
        });
    });

    g.bench_function("miesiac: status, gospodarstwa, migracja", |b| {
        let mut w = swiat_spoleczny(10_000);
        let mut hooks = NoInheritance;
        for d in 0..30 {
            society::step_day(&mut w, d, &mut hooks);
        }
        let mut miesiac = 1u64;
        b.iter(|| {
            miesiac += 1;
            black_box(society::step_day(&mut w, miesiac * 30, &mut hooks));
        });
    });
    g.finish();
}
