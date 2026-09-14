//! Scenariusz `m3day` — **artefakt końcowy fazy M3** (§1, punkt 1).
//!
//! „30 dni gry na 120 tys. mieszkańców bez GPU; na wyjściu: histogram wykorzystania
//! czasu doby, rozkład czasu dojazdu, poziomy potrzeb, log zdarzeń DES, hash stanu
//! co 1000 ticków."
//!
//! Różnica wobec `day` z M3b jest cała w tym, co go otacza: miasto jest prawdziwe
//! (M2 + Etap 8), mieszkańcy są encjami ECS, a pętlę przewijają **systemy z §5.12**,
//! nie runner. Runner mierzy i wypisuje.

use clap::Args;
use magnat_agents::{
    bootstrap_day, register_day, society, AgentSources, DayLoopSystem, DayStats, EventQueue,
    HouseholdStockSystem, InfinitePlaces, NeedTable, Needs, NoInheritance, Population,
    ReplanCooldownSystem, SkillDriftSystem, SocietySystem, WalkMicroSystem,
};
use magnat_agents::{AgentState, DeprivationEffectsSystem, Employment, NeedDecaySystem};
use magnat_core::{ActivityKind, NeedKind};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use std::process::ExitCode;
use std::sync::Arc;

use crate::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_agents::Trace;
use magnat_core::{SimSpeed, Tick};
use magnat_ui::{
    CitizenPanel, InspectorPanel, ListPicker, Selection, TimeControlsWidget, UiContext,
};

#[derive(Args, Debug)]
pub struct M3DayArgs {
    #[arg(long, default_value = "1")]
    pub seed: String,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "8km")]
    pub size: String,

    #[arg(long, default_value = "lowland")]
    pub region: String,

    #[arg(long, default_value = "1990")]
    pub epoch: String,

    #[arg(long, default_value = "mixed")]
    pub profile: String,

    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Ile dób gry przebiec.
    #[arg(long, default_value_t = 1)]
    pub days: u32,

    /// Docelowa liczba mieszkańców; 0 = z pojemności miasta.
    #[arg(long, default_value_t = 0)]
    pub citizens: u32,

    /// Co ile ticków liczyć hash stanu (0 = nigdy).
    #[arg(long, default_value_t = 1000)]
    pub hash_every: u64,

    /// Zapis ciągu hashy do pliku — wejście testu determinizmu `det_agents_hash`.
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,

    /// Porównanie z zapisanym ciągiem; różnica → kod wyjścia 1.
    #[arg(long)]
    pub expect: Option<std::path::PathBuf>,

    /// Ilu mieszkańców trzymać w LOD Mikro (pozycja co 100 ms).
    #[arg(long, default_value_t = 0)]
    pub micro: u32,

    /// Karta inspekcji tego mieszkańca (indeks na liście populacji) po przebiegu.
    #[arg(long)]
    pub inspect: Option<usize>,

    /// Język karty: `pl` albo `en`.
    #[arg(long, default_value = "pl")]
    pub locale: String,

    /// Prędkość gry: `1`, `3`, `10` albo `0` (pauza). Doba przy każdej z nich ma dać
    /// **ten sam** hash stanu — to jest kryterium WP11 i wymóg PRD §14.5.
    #[arg(long, default_value_t = 1)]
    pub speed: u32,
}

fn parse_seed(s: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let t = s.trim();
    Ok(
        match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            Some(hex) => u64::from_str_radix(&hex.replace('_', ""), 16)?,
            None => t.replace('_', "").parse::<u64>()?,
        },
    )
}

pub fn run(a: &M3DayArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let seed = parse_seed(&a.seed)?;
    let pool = JobPool::new(a.threads);
    let start = std::time::Instant::now();
    let city = zbuduj_miasto(seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
    eprintln!("miasto {:.1} s", start.elapsed().as_secs_f64());

    let mut world = swiat_agentow(seed)?;
    register_day(&mut world);
    let start = std::time::Instant::now();
    let zaludnione = zaludnij(&mut world, &city, a.citizens, 200_000)?;
    eprintln!("Etap 8 {:.1} s", start.elapsed().as_secs_f64());
    for l in zaludnione.report.lines() {
        eprintln!("{l}");
    }

    // Źródła miejsc i podróży: to jest cały most między miastem a agentami.
    // M5 podmienia `InfinitePlaces` na indeks ofert, M4 `WalkOracle` na `engine/nav`.
    let tabela = Arc::new(NeedTable::load_default()?);
    *world.resource_mut::<AgentSources>() = AgentSources::new(
        Box::new(InfinitePlaces::new(zaludnione.places.clone(), tabela)),
        zaludnione.walk_oracle(),
    );

    let zaplanowanych = bootstrap_day(&mut world, 0);
    eprintln!(
        "kolejka zasiana: {zaplanowanych} mieszkańców, {} zdarzeń",
        world.resource::<EventQueue>().len()
    );

    let mut builder = ScheduleBuilder::new();
    builder
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)));
    if a.micro > 0 {
        builder.add(WalkMicroSystem::new(&world));
    }
    let schedule = builder.build()?;
    eprintln!(
        "harmonogram: {} systemów w {} etapach, odcisk {:#018x}",
        schedule.system_count(),
        schedule.stage_count(),
        schedule.fingerprint()
    );

    let mut app = App::new(world, schedule, a.threads);

    // Karta inspekcji potrzebuje **realizacji**, a nie tylko planu: bufor śledzenia
    // zbiera zdarzenia DES obserwowanego mieszkańca (decyzja 9.16, najwyżej ośmiu).
    let wybrany = a.inspect.and_then(|n| {
        let picker = ListPicker::new(app.world.resource::<Population>().citizens().to_vec());
        match picker.by_index(n) {
            Selection::Citizen(c) => {
                app.world.resource_mut::<Trace>().watch(c.entity().index());
                Some(c)
            }
            _ => None,
        }
    });

    // Prędkość gry idzie przez ten sam widget, którego używa klient graficzny.
    // Wynik **nie zależy** od niej: zegar zamienia czas realny na liczbę minut,
    // a każda minuta jest tym samym tickiem (§5.11, test `det_speed_invariance`).
    let mut zegar = TimeControlsWidget::new(Tick(0));
    zegar.set_speed(match a.speed {
        0 => SimSpeed::Paused,
        3 => SimSpeed::X3,
        10 => SimSpeed::X10,
        _ => SimSpeed::X1,
    });
    let ticki = u64::from(a.days) * 1440;
    let mut hashe: Vec<(u64, String)> = Vec::new();
    let bieg = std::time::Instant::now();
    let mut wykonane = 0u64;
    while wykonane < ticki {
        // Klatka 16 ms — tyle samo, ile w kliencie przy 60 FPS.
        let minut = u64::from(zegar.advance(16)).min(ticki - wykonane);
        for _ in 0..minut {
            app.tick();
            wykonane += 1;
            if a.hash_every > 0 && wykonane.is_multiple_of(a.hash_every) {
                hashe.push((wykonane, world_state_hash(&app.world).to_string()));
            }
        }
        if zegar.speed() == SimSpeed::Paused {
            eprintln!("pauza: symulacja stoi, przebieg kończy się po {wykonane} tickach");
            break;
        }
    }
    let czas = bieg.elapsed();

    let stats = *app.world.resource::<DayStats>();
    let ludzi = society::population(&app.world);
    eprintln!(
        "{} dób gry dla {ludzi} mieszkańców w {:.2} s ({:.0}× czasu rzeczywistego)",
        a.days,
        czas.as_secs_f64(),
        ticki as f64 * 60.0 / czas.as_secs_f64().max(1e-9)
    );
    println!("zdarzenia DES: {}", stats.events);
    println!(
        "  plany {}, przeplanowania {}, dojazdy {}, przybycia {}",
        stats.plans, stats.replans, stats.trips, stats.arrivals
    );
    println!(
        "  spóźnienia: {} przybyć, średnio {:.1} min — plan powstaje o północy, a marsz          zwalnia razem z energią; od tego jest `ReplanCause::Late`",
        stats.late,
        stats.late_minutes as f64 / stats.late.max(1) as f64
    );
    println!(
        "  wizyty zaspokojone {}, odmowy {}",
        stats.fulfilled, stats.refused
    );

    profil_doby(&app.world);
    potrzeby(&app.world);
    if let Some(c) = wybrany {
        karta(&app.world, c, wykonane / 1440, seed, &a.locale)?;
    }

    if let Some(p) = &a.out {
        let tekst: String = hashe
            .iter()
            .map(|(t, h)| format!("{t} {h}\n"))
            .collect();
        std::fs::write(p, tekst)?;
        eprintln!("zapisano {} hashy do {}", hashe.len(), p.display());
    }
    if let Some(p) = &a.expect {
        let wzorzec = std::fs::read_to_string(p)?;
        let nasz: String = hashe
            .iter()
            .map(|(t, h)| format!("{t} {h}\n"))
            .collect();
        if wzorzec != nasz {
            let pierwsza = wzorzec
                .lines()
                .zip(nasz.lines())
                .find(|(a, b)| a != b)
                .map_or("(inna długość ciągu)".to_string(), |(a, b)| {
                    format!("oczekiwano `{a}`, jest `{b}`")
                });
            eprintln!("BŁĄD: ciąg hashy się rozjechał — {pierwsza}");
            return Ok(ExitCode::FAILURE);
        }
        eprintln!("ciąg {} hashy zgodny z wzorcem", hashe.len());
    }

    // Spóźnienie musi wywołać przeplanowanie — inaczej mieszkaniec wykonuje plan,
    // który już nie pasuje do jego doby (§5.2). Debouncing 15 minut sprawia, że
    // przeplanowań bywa mniej niż spóźnień, ale zera przy niepustych spóźnieniach
    // być nie może.
    if stats.late > 0 && stats.replans == 0 {
        eprintln!("BŁĄD: {} spóźnień i ani jednego przeplanowania", stats.late);
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// Karta inspekcji mieszkańca w formie tekstowej — ten sam model, który w kliencie
/// karmi widget (M3d §5.11). Plan odtwarza się z ziarna przez `plan_day_explained`,
/// realizacja pochodzi z bufora śledzenia.
fn karta(
    world: &magnat_ecs::World,
    citizen: magnat_core::CitizenId,
    day: u64,
    seed: u64,
    locale: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ui = UiContext::new(locale.parse()?, Tick(day * 1440))?;
    ui.selection = Selection::Citizen(citizen);
    let mut panel = CitizenPanel { day, seed };
    println!("
── {} ──", panel.title(&ui));
    print!("{}", panel.build(&ui, world));
    Ok(())
}

/// Histogram wykorzystania doby: ile minut mieszkaniec spędza w której czynności.
///
/// Liczony z **planów**, nie z licznika zdarzeń: plan jest tym, co mieszkaniec
/// zamierzał, a różnicę między zamiarem a wykonaniem pokazuje osobno karta inspekcji
/// (§5.11, ścieżka „realizacja").
fn profil_doby(world: &magnat_ecs::World) {
    let mut minuty = [0u64; ActivityKind::ALL.len()];
    let mut ludzi = 0u64;
    for e in world.resource::<Population>().citizens() {
        let Some(plan) = world.get::<magnat_agents::PlanRef>(*e) else {
            continue;
        };
        ludzi += 1;
        for s in magnat_agents::load_plan(plan, world.resource::<magnat_agents::PlanSlab>()) {
            minuty[s.kind as usize] += u64::from(s.dur_min);
        }
    }
    if ludzi == 0 {
        return;
    }
    let suma: u64 = minuty.iter().sum();
    println!("\nwykorzystanie doby (średnio minut na mieszkańca)");
    for (i, k) in ActivityKind::ALL.iter().enumerate() {
        if minuty[i] == 0 {
            continue;
        }
        println!(
            "  {:<9} {:>7.1} min  {:>5.1} %",
            k.name(),
            minuty[i] as f64 / ludzi as f64,
            minuty[i] as f64 * 100.0 / suma.max(1) as f64
        );
    }
}

/// Średni poziom dwunastu potrzeb — czy doba je zaspokaja, czy miasto głoduje.
fn potrzeby(world: &magnat_ecs::World) {
    let mut suma = [0u64; magnat_core::NEED_COUNT];
    let mut ludzi = 0u64;
    let mut bez_pracy = 0u64;
    let mut idle = 0u64;
    for e in world.resource::<Population>().citizens() {
        let Some(n) = world.get::<Needs>(*e) else {
            continue;
        };
        ludzi += 1;
        for (i, v) in n.level.iter().enumerate() {
            suma[i] += u64::from(*v);
        }
        if world.get::<Employment>(*e).is_some_and(|x| !x.has_job()) {
            bez_pracy += 1;
        }
        if world
            .get::<AgentState>(*e)
            .is_some_and(|a| a.activity == ActivityKind::Idle as u8)
        {
            idle += 1;
        }
    }
    if ludzi == 0 {
        return;
    }
    println!("\npoziomy potrzeb (średnia 0–100)");
    for (i, k) in NeedKind::ALL.iter().enumerate() {
        println!("  {:<12} {:>5.1}", k.name(), suma[i] as f64 / ludzi as f64);
    }
    println!(
        "\nbez pracy: {bez_pracy} ({:.1} %) · bezczynnych w tej minucie: {idle}",
        bez_pracy as f64 * 100.0 / ludzi as f64
    );
    let _ = Tick(0);
}
