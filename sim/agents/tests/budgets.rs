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
