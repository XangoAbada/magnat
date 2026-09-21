//! Scenariusz `agents` — wynik do pokazania podfazy M3a.
//!
//! „400 tys. mieszkańców w pamięci, potrzeby spadają zgodnie z tabelą, koło czasu
//! rozdaje zdarzenia" — dokładnie to i nic więcej. Mieszkańcy są **syntetyczni**:
//! miasto z M2 podłącza dopiero generator populacji (Etap 8, M3d), a plan dnia
//! powstaje w M3b. Tu chodzi o warstwę pod tym: rachunek pamięci, tempo potrzeb
//! i przepustowość kolejki zdarzeń.
//!
//! Pętla zdarzeń jest w runnerze, a nie w systemie ECS, i to jest celowe: w M3a nie ma
//! jeszcze **czego** dyspozytorować — handlery zdarzeń to planer dnia (M3b). Tutaj
//! każde `NeedTick` harmonogramuje po prostu następne, co pokazuje lazy scheduling
//! z §5.2 i mierzy koszt koła czasu na realnym rozkładzie.

use clap::Args;
use magnat_agents::{
    deprivation_of, register, AgentState, DeprivationEffectsSystem, Employment, EventKind,
    EventQueue, Identity, KnowledgeRef, Lifecycle, NeedDecaySystem, NeedTable, Needs, Personality,
    PlanRef, RelationsRef, Residence, SimEvent, Skills, Vitals, Wealth,
};
use magnat_core::{rng, DecisionReason, NeedKind, StreamId, Tick};
use magnat_ecs::{App, ScheduleBuilder, World};
use magnat_io::world_state_hash;

#[derive(Args, Debug)]
pub struct AgentsArgs {
    /// Ziarno świata.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// Liczba mieszkańców.
    #[arg(long, default_value_t = 400_000)]
    pub citizens: u32,

    /// Ile dób gry przebiec (doba = 1440 ticków minutowych).
    #[arg(long, default_value_t = 1)]
    pub days: u32,

    /// 0 = liczba rdzeni.
    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Co ile ticków liczyć hash stanu (0 = nigdy).
    #[arg(long, default_value_t = 1000)]
    pub hash_every: u64,
}

/// Syntetyczna populacja: wiek, osobowość i zmiana z ziarna, reszta domyślna.
/// **Nie jest to Etap 8** — nie ma tu dopasowania do miejsc pracy ani piramidy wieku,
/// bo to jest treść M3d. Jest za to rozrzut, bez którego pomiar pamięci i potrzeb
/// mierzyłby jeden przypadek powielony 400 tys. razy.
fn zaludnij(seed: u64, n: u32) -> World {
    let mut w = World::new(seed);
    register(
        &mut w,
        NeedTable::load_default().expect("data/needs/needs.ron"),
    );

    for i in 0..n {
        let mut r = rng(seed, StreamId::PersonalityGen, i, Tick(0));
        let wiek_dni = 360 * i64::from(r.gen_range_u32(85)) + i64::from(r.gen_range_u32(360));
        let cechy: [u8; 8] = std::array::from_fn(|_| r.gen_q().get());
        let pracuje = (360 * 18..360 * 65).contains(&wiek_dni);
        // Świat syntetyczny też losuje z puli, a nie ze stałej: karta inspekcji jest tu
        // ta sama co w mieście, więc indeks bez wpisu wyszedłby dokładnie tak samo (WP13).
        let nazwy = magnat_agents::name_catalog();
        let region = nazwy.pick_region(&mut r);
        let mezczyzna = r.gen_bool_permille(500);
        let _ = w
            .spawn()
            .with(Identity {
                first_name: nazwy.pick_first(region, mezczyzna, &mut r),
                last_name: nazwy.pick_surname(region, &mut r),
                birth_day: -(wiek_dni as i32),
                birth_district: (i % 12) as u16,
                flags: Identity::FLAG_ALIVE | u8::from(mezczyzna),
                _pad: 0,
                household: i / 3,
            })
            .with(Personality(cechy))
            .with(Vitals {
                health: 50 + r.gen_range_u32(50) as u8,
                energy: 40 + r.gen_range_u32(60) as u8,
                ..Vitals::default()
            })
            .with(Needs::default())
            .with(Skills::default())
            .with(Wealth::default())
            .with(Employment {
                site: if pracuje {
                    i % 20_000
                } else {
                    Employment::NO_SITE
                },
                work_days: if pracuje { Employment::WEEKDAYS } else { 0 },
                ..Employment::default()
            })
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

pub fn run(a: &AgentsArgs) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let mut world = zaludnij(a.seed, a.citizens);
    eprintln!(
        "zaludnienie: {} mieszkańców w {:.2} s",
        world.entity_count(),
        start.elapsed().as_secs_f64()
    );

    // Lazy scheduling (§5.2): jedno zdarzenie na mieszkańca, reszta dokłada się w biegu.
    {
        let q = world.resource_mut::<EventQueue>();
        for i in 0..a.citizens {
            q.schedule(SimEvent::new(
                i % 60,
                i,
                EventKind::NeedTick,
                SimEvent::NO_SLOT,
            ));
        }
    }

    let schedule = {
        let mut b = ScheduleBuilder::new();
        b.add(NeedDecaySystem::new(&world));
        b.add(DeprivationEffectsSystem::new(&world));
        b.build()?
    };
    let mut app = App::new(world, schedule, a.threads);
    eprintln!(
        "{} systemów · {} wątków · {} archetypów",
        app.schedule().system_count(),
        app.thread_count(),
        app.world.archetypes().len()
    );

    let ticki = u64::from(a.days) * 1_440;
    let mut obsluzone: u64 = 0;
    let mut szczyt = 0usize;
    let mut bufor: Vec<SimEvent> = Vec::with_capacity(32_768);
    let mut hashe: Vec<(u64, String)> = Vec::new();
    let bieg = std::time::Instant::now();

    for _ in 0..ticki {
        app.tick();
        let tick = app.world.tick.0;

        let q = app.world.resource_mut::<EventQueue>();
        q.drain_minute(&mut bufor);
        obsluzone += bufor.len() as u64;
        szczyt = szczyt.max(bufor.len());
        // Każdy tik potrzeb harmonogramuje następny za godzinę — to jest cała
        // „obsługa" w M3a. Handlery czynności dokłada planer (M3b).
        for e in &bufor {
            if e.event_kind() == Some(EventKind::NeedTick) {
                q.schedule(SimEvent::new(
                    tick as u32 + 60,
                    e.actor,
                    EventKind::NeedTick,
                    SimEvent::NO_SLOT,
                ));
            }
        }

        if a.hash_every > 0 && tick.is_multiple_of(a.hash_every) {
            hashe.push((tick, world_state_hash(&app.world).to_string()));
        }
    }

    let czas = bieg.elapsed();
    eprintln!(
        "{} ticków ({} dób gry) w {:.2} s · {} zdarzeń, szczyt {} na tick",
        ticki,
        a.days,
        czas.as_secs_f64(),
        obsluzone,
        szczyt
    );

    raport_potrzeb(&app.world, a.citizens);

    for (tick, h) in &hashe {
        println!("{tick} {h}");
    }
    Ok(std::process::ExitCode::SUCCESS)
}

/// Histogram poziomów i lista deprywacji — sprawdzian „potrzeby spadają zgodnie z tabelą"
/// wykonywany okiem, obok testu, który sprawdza to liczbowo.
fn raport_potrzeb(world: &World, n: u32) {
    let tabela = world.resource::<NeedTable>();
    let mut sumy = [0u64; magnat_core::NEED_COUNT];
    let mut w_deprywacji = [0u32; magnat_core::NEED_COUNT];
    let mut powody: Vec<DecisionReason> = Vec::new();
    let mut przyklad: Option<String> = None;

    let encje: Vec<magnat_ecs::Entity> = world
        .archetypes()
        .iter()
        .flat_map(|a| a.chunks())
        .flat_map(|c| c.entities().to_vec())
        .collect();
    for e in encje {
        let Some(needs) = world.get::<Needs>(e) else {
            continue;
        };
        for (i, v) in needs.level.iter().enumerate() {
            sumy[i] += u64::from(*v);
            if u32::from(*v) < u32::from(tabela.spec(NeedKind::ALL[i]).critical) {
                w_deprywacji[i] += 1;
            }
        }
        if przyklad.is_none() {
            deprivation_of(needs, tabela, &mut powody);
            if !powody.is_empty() {
                przyklad = Some(format!("{powody:?}"));
            }
        }
    }

    println!("potrzeba        średnia  w deprywacji");
    for (i, need) in NeedKind::ALL.iter().enumerate() {
        println!(
            "{:<14} {:>7.1} {:>13}",
            need.name(),
            sumy[i] as f64 / f64::from(n.max(1)),
            w_deprywacji[i]
        );
    }
    if let Some(p) = przyklad {
        println!("przykładowe uzasadnienie deprywacji: {p}");
    }
}
