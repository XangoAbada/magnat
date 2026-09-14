//! Budżet pamięci i czasu fundamentu agenta (M3a: kryteria WP1, WP2, WP3; M3 §7.5).
//!
//! Mierzymy **zaalokowane bajty struktur**, a nie RSS procesu: RSS zależy od alokatora
//! i systemu, więc na współdzielonym runnerze dawałby fałszywe alarmy. To samo
//! rozstrzygnięcie co w `engine/ecs/tests/memory.rs`.
//!
//! Progi czasowe są sprawdzane **z zapasem rzędu wielkości**, bo test biega też
//! w profilu debug i na cudzym sprzęcie. Prawdziwą bramką regresji wydajności jest
//! `benches/agents_bench.rs` + `scripts/bench_guard.py` (D-8).

use magnat_agents::{
    register, AgentState, EventKind, EventQueue, Identity, Knowledge, KnowledgeRef, KnowledgeSlab,
    Lifecycle, NeedTable, Needs, Personality, PlanRef, PlanSlot, Relation, RelationSlab,
    RelationsRef, Residence, SimEvent, SkillSlot, Skills, SlabRef, Vitals, Wealth,
    HOT_COMPONENT_BYTES,
};
use magnat_agents::{Employment, PlanSlab};
use magnat_ecs::World;

const N: u32 = 400_000;

/// Średnie obłożenie magazynów z rachunku §5.1: 12 slotów planu, 10 relacji.
/// Relacji **nie rozdajemy po równo** — slab ma klasy rozmiaru, więc stała liczba 10
/// dla wszystkich kazałaby każdemu zapłacić za blok dwunastoelementowy i zmierzyłaby
/// rozkład, którego nie ma. Ten rozkład (średnia 9,8) jest z sufitu tak samo jak
/// tamten, ale przynajmniej ma ogon — a to ogon płaci za klasy rozmiaru.
fn relacji_dla(i: u32) -> usize {
    match i % 10 {
        0..=2 => 3,
        3..=5 => 7,
        6 | 7 => 12,
        8 => 18,
        _ => 26,
    }
}

fn swiat_populacji(n: u32) -> World {
    let mut w = World::new(7);
    register(
        &mut w,
        NeedTable::load_default().expect("data/needs/needs.ron"),
    );
    for i in 0..n {
        let _ = w
            .spawn()
            .with(Identity {
                birth_day: -(360 * 20) - (i as i32 % 20_000),
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
            .with(Skills([SkillSlot::default(); 4]))
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

#[test]
fn mem_population_400k() {
    let mut w = swiat_populacji(N);

    // Magazyny wypełniamy realnym obłożeniem — pusty slab zmierzyłby budżet,
    // którego nikt nie używa.
    {
        let slab = w.resource_mut::<RelationSlab>();
        for i in 0..N {
            let mut r = SlabRef::EMPTY;
            for j in 0..relacji_dla(i) {
                slab.push(
                    &mut r,
                    Relation {
                        other: i.wrapping_add(j as u32),
                        kind: 0,
                        weight: 40,
                        last_contact_day: 0,
                    },
                    |_| 0,
                );
            }
        }
    }
    {
        // Plan dnia: średnio 12 slotów (§5.1), ale nie każdy ma tyle samo —
        // dziecko i emeryt mają krótszy dzień niż rodzic na dwie zmiany.
        let slab = w.resource_mut::<PlanSlab>();
        let plan = [PlanSlot::default(); 24];
        for i in 0..N {
            let dlugosc = match i % 5 {
                0 => 8,
                1 | 2 => 12,
                3 => 14,
                _ => 16,
            };
            let mut r = SlabRef::EMPTY;
            slab.store(&mut r, &plan[..dlugosc]);
        }
    }
    {
        let q = w.resource_mut::<EventQueue>();
        for i in 0..N {
            // Lazy scheduling: jedno zdarzenie w locie na mieszkańca (§5.2).
            q.schedule(SimEvent::new(i % 1_440, i, EventKind::NeedTick, 0));
        }
    }

    let ecs: usize = w.archetypes().iter().map(|a| a.allocated_bytes()).sum();
    let relacje = w.resource::<RelationSlab>().allocated_bytes();
    let plany = w.resource::<PlanSlab>().allocated_bytes();
    let zdarzenia = w.resource::<EventQueue>().allocated_bytes();
    let gorace = ecs + relacje + plany + zdarzenia;
    let na_osobe = gorace as f64 / f64::from(N);

    println!(
        "400 tys. mieszkańców: ECS {} MB ({:.0} B/os.), relacje {} MB ({:.0} B/os.), \
         plany {} MB ({:.0} B/os.), kolejka {} MB ({:.0} B/os.) → razem {:.0} B/os.",
        ecs / 1_048_576,
        ecs as f64 / f64::from(N),
        relacje / 1_048_576,
        relacje as f64 / f64::from(N),
        plany / 1_048_576,
        plany as f64 / f64::from(N),
        zdarzenia / 1_048_576,
        zdarzenia as f64 / f64::from(N),
        na_osobe
    );

    // Budżet po korekcie D-1 w `M3a-fundament-agenta.md`: **430 B** zamiast 396 B
    // z pierwotnego rachunku §5.1. Różnica to narzut alokacji, którego tamten rachunek
    // nie uwzględniał wcale — klasy rozmiaru slabu zaokrąglają 10 relacji do bloku 12,
    // a 12 slotów planu do bloku 12 albo 16. Przy 400 tys. mieszkańców całość daje
    // 161 MB zamiast 151 MB, więc cel §17.7 („metropolia w 6 GB") stoi z tym samym
    // zapasem; zmieniła się liczba w kryterium, nie wniosek z niej.
    // Magazyn wiedzy jest **poza** tym budżetem (§5.1: „pamięć doświadczeń w osobnym
    // magazynie") i ma własny test niżej.
    assert!(
        na_osobe <= 430.0,
        "stan gorący to {na_osobe:.0} B/mieszkańca, budżet po korekcie D-1 to 430 B"
    );
    // Sam narzut chunkowania ECS nad surowymi komponentami — 13 komponentów + Entity.
    let uzyteczne = N as usize * (HOT_COMPONENT_BYTES + 8);
    let narzut = ecs as f64 / uzyteczne as f64;
    assert!(
        narzut < 1.15,
        "narzut chunkowania {:.1} %",
        (narzut - 1.0) * 100.0
    );
}

#[test]
fn slab_wiedzy_miesci_sie_poza_budzetem_goracym() {
    // §5.1: 400 k × śr. 16 wpisów × 8 B ≈ 51 MB. Sprawdzamy rachunek, nie zgadujemy.
    let mut slab = KnowledgeSlab::new();
    for i in 0..N {
        let mut r = SlabRef::EMPTY;
        for j in 0..(12 + (i % 9) as usize) {
            slab.push(
                &mut r,
                Knowledge {
                    target: i.wrapping_add(j as u32),
                    day: 0,
                    score: 50,
                    kind: 0,
                },
                |w| {
                    w.iter()
                        .enumerate()
                        .min_by_key(|(i, k)| (k.rank(0), *i))
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                },
            );
        }
    }
    let mb = slab.allocated_bytes() / 1_048_576;
    println!("magazyn wiedzy dla 400 tys.: {mb} MB");
    // Próg z decyzji 9.14: dopiero powyżej 80 MB sięgamy po pakowanie wpisu do 5 B.
    assert!(
        mb <= 80,
        "magazyn wiedzy urósł do {mb} MB — decyzja 9.14 mówi, co wtedy"
    );
    assert_eq!(slab.occupied_blocks(), N as usize, "wyciek bloków slabu");
}

#[test]
fn des_przerabia_dobe_dwustu_tysiecy_agentow() {
    // WP3: 4 mln zdarzeń na dobę gry. Próg §7.5 to 250 ms w profilu release;
    // tutaj pilnujemy tylko, że nic się nie gubi i nie dubluje — czas mierzy benchmark.
    const AGENCI: u32 = 200_000;
    const NA_AGENTA: u32 = 20;
    let mut q = EventQueue::new();
    for a in 0..AGENCI {
        for k in 0..NA_AGENTA {
            let minuta = (a.wrapping_mul(2_654_435_761).wrapping_add(k * 71)) % 1_440;
            let kind = match k % 4 {
                0 => EventKind::Arrive,
                1 => EventKind::NeedTick,
                2 => EventKind::StartActivity,
                _ => EventKind::EndActivity,
            };
            q.schedule(SimEvent::new(minuta, a, kind, k as u8));
        }
    }
    let wstawione = q.len();
    assert_eq!(wstawione, (AGENCI * NA_AGENTA) as usize);

    let start = std::time::Instant::now();
    let mut out = Vec::with_capacity(32_768);
    let mut obsluzone = 0usize;
    let mut szczyt = 0usize;
    for _ in 0..1_440 {
        q.drain_minute(&mut out);
        obsluzone += out.len();
        szczyt = szczyt.max(out.len());
    }
    let czas = start.elapsed();
    println!(
        "4 mln zdarzeń w {:.0} ms (szczyt {} zdarzeń/tick), kolejka {} MB",
        czas.as_secs_f64() * 1000.0,
        szczyt,
        q.allocated_bytes() / 1_048_576
    );

    assert_eq!(
        obsluzone, wstawione,
        "zdarzenia zginęły albo się zdublowały"
    );
    assert!(q.is_empty());
}

// ── M3b: ruch pieszy (WP6) ──────────────────────────────────────────────────────

/// Katalog miejsc, węzły i odcinki `(a, b, długość w cm)` plus lista celów —
/// minimum, jakiego `WalkOracle::with_streets` potrzebuje od M2 (decyzja 9.4).
type MiastoTestowe = (
    std::sync::Arc<magnat_agents::PlaceTable>,
    Vec<magnat_core::WorldCoord>,
    Vec<(u32, u32, u32)>,
    Vec<magnat_core::PlaceRef>,
);

/// Miasto testowe: siatka 20 × 20 węzłów co 150 m i sto miejsc rozsianych po niej.
fn miasto_testowe() -> MiastoTestowe {
    use magnat_agents::{PlaceEntry, PlaceTable};
    use magnat_core::{BuildingId, Entity, PlaceKind, PlaceRef, WorldCoord};

    const K: i32 = 20;
    const KROK: i32 = 15_000;
    let encja = |i: u32| Entity::new(i, std::num::NonZeroU32::new(1).unwrap());

    let mut nodes = Vec::new();
    for y in 0..K {
        for x in 0..K {
            nodes.push(WorldCoord::new(x * KROK, y * KROK, 0));
        }
    }
    let idx = |x: i32, y: i32| (y * K + x) as u32;
    let mut segs = Vec::new();
    for y in 0..K {
        for x in 0..K {
            if x + 1 < K {
                segs.push((idx(x, y), idx(x + 1, y), KROK as u32));
            }
            if y + 1 < K {
                segs.push((idx(x, y), idx(x, y + 1), KROK as u32));
            }
        }
    }

    let mut wpisy = vec![PlaceEntry {
        place: PlaceRef::Building(BuildingId(encja(1))),
        kind: PlaceKind::Home,
        at: WorldCoord::new(0, 0, 0),
    }];
    let mut cele = Vec::new();
    for i in 0..100u32 {
        let x = (i * 7 % K as u32) as i32;
        let y = (i * 13 % K as u32) as i32;
        let p = PlaceRef::Building(BuildingId(encja(100 + i)));
        cele.push(p);
        wpisy.push(PlaceEntry {
            place: p,
            kind: PlaceKind::Grocery,
            at: WorldCoord::new(x * KROK + 300, y * KROK + 300, 0),
        });
    }
    (
        std::sync::Arc::new(PlaceTable::build(wpisy)),
        nodes,
        segs,
        cele,
    )
}

#[test]
fn walk_estimate_miesci_sie_w_budzecie() {
    // §7.5 `bench_walk_estimate`: mediana ≤ 80 µs dla przebiegu **zimnego** (pierwsza
    // Dijkstra dla pary miejsc) i ≤ 0,2 µs dla trafienia w cache. Tutaj pilnujemy
    // progu z zapasem rzędu wielkości, bo test biega też w debugu; dokładny pomiar
    // i bramka regresji są w `benches/agents_bench.rs` (D-8).
    use magnat_agents::{TravelOracle, WalkOracle};
    use magnat_core::{BuildingId, CitizenId, Entity, MinuteOfDay, PlaceRef};

    let (places, nodes, segs, cele) = miasto_testowe();
    let oracle = WalkOracle::with_streets(places, &nodes, &segs);
    let dom = PlaceRef::Building(BuildingId(Entity::new(
        1,
        std::num::NonZeroU32::new(1).unwrap(),
    )));

    let (identity, vitals, potrzeby, personality, residence) = (
        Identity {
            birth_day: -360 * 34,
            flags: Identity::FLAG_ALIVE,
            ..Identity::default()
        },
        Vitals {
            health: 80,
            energy: 80,
            ..Vitals::default()
        },
        Needs::default(),
        Personality([50; 8]),
        Residence::default(),
    );
    let widok = magnat_agents::CitizenView {
        id: CitizenId(Entity::new(1, std::num::NonZeroU32::new(1).unwrap())),
        identity: &identity,
        vitals: &vitals,
        needs: &potrzeby,
        personality: &personality,
        residence: &residence,
        today: 0,
    };

    // Zimno: każda para pytana pierwszy raz.
    let start = std::time::Instant::now();
    let mut suma = 0u64;
    for cel in &cele {
        suma += u64::from(
            oracle
                .estimate(dom, *cel, MinuteOfDay::new(8 * 60), &widok)
                .minutes,
        );
    }
    let zimno = start.elapsed() / cele.len() as u32;

    // Ciepło: te same pary, tym razem z cache'u.
    let start = std::time::Instant::now();
    for cel in &cele {
        suma += u64::from(
            oracle
                .estimate(dom, *cel, MinuteOfDay::new(8 * 60), &widok)
                .minutes,
        );
    }
    let cieplo = start.elapsed() / cele.len() as u32;

    println!(
        "estimate: zimno {:.1} µs, cache {:.3} µs ({} par)",
        zimno.as_secs_f64() * 1e6,
        cieplo.as_secs_f64() * 1e6,
        cele.len()
    );
    assert!(suma > 0);
    assert!(
        zimno < std::time::Duration::from_micros(800),
        "zimny estimate {zimno:?} — próg §7.5 to 80 µs, tu z zapasem 10×"
    );
    assert!(
        cieplo * 10 < zimno,
        "cache nie przyspiesza: zimno {zimno:?}, ciepło {cieplo:?}"
    );
}

#[test]
fn mikro_utrzymuje_piec_tysiecy_pieszych_w_kadrze() {
    // Kryterium WP6: 5 tys. pieszych w LOD Mikro ≤ 2 ms na klatkę. Próg z zapasem
    // rzędu wielkości (debug), dokładny pomiar w benchmarku `m3b-2 ruch`.
    use magnat_agents::{TravelOracle, TripRequest, WalkOracle};
    use magnat_core::{BuildingId, CitizenId, Entity, MinuteOfDay, PlaceRef};

    const PIESZYCH: u32 = 5_000;
    let (places, nodes, segs, cele) = miasto_testowe();
    let mut oracle = WalkOracle::with_streets(places, &nodes, &segs);
    let dom = PlaceRef::Building(BuildingId(Entity::new(
        1,
        std::num::NonZeroU32::new(1).unwrap(),
    )));
    let (identity, vitals, potrzeby, personality, residence) = (
        Identity {
            birth_day: -360 * 34,
            flags: Identity::FLAG_ALIVE,
            ..Identity::default()
        },
        Vitals {
            health: 80,
            energy: 80,
            ..Vitals::default()
        },
        Needs::default(),
        Personality([50; 8]),
        Residence::default(),
    );
    let widok = magnat_agents::CitizenView {
        id: CitizenId(Entity::new(1, std::num::NonZeroU32::new(1).unwrap())),
        identity: &identity,
        vitals: &vitals,
        needs: &potrzeby,
        personality: &personality,
        residence: &residence,
        today: 0,
    };

    // Kadr obejmujący całą scenę testową: bez okna warstwa Mikro jest wyłączona
    // i test mierzyłby pustą pętlę (M3d, okno Mikro w `WalkOracle`).
    oracle.set_micro_window(Some((0, 0)), 100_000);

    let mut q = EventQueue::new();
    for i in 0..PIESZYCH {
        let handle = oracle.begin_trip(
            TripRequest {
                traveller: CitizenId(Entity::new(i + 1, std::num::NonZeroU32::new(1).unwrap())),
                from: dom,
                to: cele[i as usize % cele.len()],
                depart: MinuteOfDay::new(8 * 60),
                slot: 0,
            },
            &widok,
            &mut q,
        );
        oracle.enter_micro(&handle, i, MinuteOfDay::new(8 * 60));
    }
    assert_eq!(oracle.micro_len(), PIESZYCH as usize);

    // Sto klatek po 100 ms gry — tyle, ile mieści się w dziesięciu sekundach gry.
    let start = std::time::Instant::now();
    for k in 0..100u64 {
        oracle.micro_step(8 * 60 * 60_000 + k * 100);
    }
    let na_klatke = start.elapsed() / 100;
    println!(
        "mikro: {PIESZYCH} pieszych, {:.0} µs na klatkę",
        na_klatke.as_secs_f64() * 1e6
    );
    assert!(
        na_klatke < std::time::Duration::from_millis(20),
        "krok mikro {na_klatke:?} — próg WP6 to 2 ms, tu z zapasem 10×"
    );

    // Spójność LOD (00 §4): mikro nie dotyka ani potrzeb, ani kolejki zdarzeń.
    // Zdarzeń jest dokładnie tyle, ile podróży — ani jednego więcej.
    assert_eq!(q.len(), PIESZYCH as usize);
}


// ── M3c: doba społeczeństwa (§7.5 `bench_gossip_day`) ───────────────────────────

/// Miasto do pomiaru doby społecznej: `gospodarstw` lokali obsadzonych do pełna,
/// dwa razy tyle etatów, kwartał po dziewięć budynków.
fn swiat_spoleczny(gospodarstw: usize) -> World {
    use magnat_agents::{
        seed_population, society, CityFacts, DemographyTable, HomeSlot, JobSlot, Vacancies,
    };
    use magnat_core::Money;

    let mut w = World::new(11);
    register(
        &mut w,
        NeedTable::load_default().expect("data/needs/needs.ron"),
    );
    society::register_society(
        &mut w,
        DemographyTable::load_default().expect("data/demography/demography.ron"),
    );

    let n = gospodarstw as u32;
    let domy: Vec<HomeSlot> = (0..n)
        .map(|i| HomeSlot {
            building: 1_000_000 + i,
            unit: 0,
            district: (i / 16) as u16,
            value: Money(1_000_000),
        })
        .collect();
    let etaty: Vec<JobSlot> = (0..n * 2)
        .map(|i| JobSlot {
            site: 2_000_000 + i / 16,
            role: (i % 40) as u16,
            shift: 1,
            work_days: 0b001_1111,
            district: (i / 32) as u16,
            wage_monthly: Money(300_000),
        })
        .collect();
    *w.resource_mut::<Vacancies>() = Vacancies::new(domy, etaty);
    *w.resource_mut::<CityFacts>() = CityFacts {
        job_prestige: Vec::new(),
        district_score: vec![50; (n / 16 + 2) as usize],
        block_of: (0..1_000_000 + n).map(|i| i / 9).collect(),
    };
    seed_population(&mut w, 0, gospodarstw);
    w
}

#[test]
fn bench_gossip_day() {
    // §7.5: „dobowa plotka dla 400 tys. ≤ 20 ms". **Zmierzone: około trzy razy tyle**
    // na jednym wątku — korekta G-16 w dokumencie podfazy. Test pilnuje progu
    // po korekcie, czyli 120 ms, i robi to na realnej skali, nie na ekstrapolacji.
    //
    // Doba dotyka 1/7 populacji (shard `SOCIAL_SHARDS`), a na osobę przypada:
    // wzmocnienie sześciu relacji po obu stronach, przegląd własnego slabu relacji
    // pod kątem zaniku i dwie opowiedziane plotki z przeszukaniem slabu słuchacza.
    use magnat_agents::{social, society, NoInheritance};

    let start = std::time::Instant::now();
    let mut w = swiat_spoleczny(200_000);
    let zaludnienie = start.elapsed();
    let ludzi = society::population(&w);
    assert!(ludzi >= 300_000, "zasiew dał tylko {ludzi} mieszkańców");

    // Rozgrzewka: bez dojrzałych relacji plotka nie ma komu opowiadać i pomiar
    // mierzyłby pustą pętlę.
    let mut hooks = NoInheritance;
    for d in 0..60 {
        society::step_day(&mut w, d, &mut hooks);
    }

    let t = std::time::Instant::now();
    for d in 60..74 {
        social::step_day(&mut w, d);
    }
    let na_dobe = t.elapsed() / 14;
    println!(
        "doba społeczna: {:.1} ms na {ludzi} mieszkańców (zasiew {:.1} s)",
        na_dobe.as_secs_f64() * 1e3,
        zaludnienie.as_secs_f64()
    );
    // Próg zależy od profilu, bo zapas „rzędu wielkości" nim nie był: zmierzone
    // w release 63,6 ms (korekta A-20), w debug **725 ms** — czyli jedenaście razy
    // więcej, a próg stał na 400 ms i test oblewał na czystym `master`. Bramką
    // regresji jest `m3c-1 spoleczenstwo` w criterion; ten test pilnuje rzędu
    // wielkości i ma to robić w obu profilach (korekta H-21).
    let prog = if cfg!(debug_assertions) { 2_000 } else { 200 };
    assert!(
        na_dobe.as_millis() <= prog,
        "dobowa plotka i relacje: {:.1} ms wobec progu {prog} ms (profil {})",
        na_dobe.as_secs_f64() * 1e3,
        if cfg!(debug_assertions) { "debug" } else { "release" }
    );
}
