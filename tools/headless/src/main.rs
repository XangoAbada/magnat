//! `magnat-headless` — runner symulacji bez GPU (M0 §5.11, WP-12).
//!
//! Jedno narzędzie obsługuje trzy rzeczy, których M0 musi dowieść: że ten sam seed
//! daje ten sam ciąg hashy niezależnie od liczby wątków, że zapis i wznowienie są
//! nieodróżnialne od przebiegu ciągłego, i że rozbieżność da się zlokalizować
//! co do encji, a nie tylko co do ticku.

#![forbid(unsafe_code)]

mod agents;
mod century;
mod day;
mod m3day;
mod nav;
mod population;
mod testworld;
mod worldgen;

use clap::{Parser, Subcommand};
use magnat_core::Tick;
use magnat_devtools::{Console, Inspector, MetricSink};
use magnat_ecs::{App, World};
use magnat_io::{load_world, save_world, world_state_hash, StateHash};
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "magnat-headless",
    about = "Runner symulacji bez GPU: determinizm, zapis, konsola"
)]
struct Args {
    /// Podpolecenie. Brak = przebieg symulacji świata syntetycznego (tryb M0).
    #[command(subcommand)]
    command: Option<Command>,

    /// Ziarno świata.
    #[arg(long, default_value_t = 1)]
    seed: u64,

    /// Liczba ticków ekonomicznych do wykonania.
    #[arg(long, default_value_t = 0)]
    ticks: u64,

    /// 0 = liczba rdzeni.
    #[arg(long, default_value_t = 0)]
    threads: usize,

    /// Rozmiar świata syntetycznego.
    #[arg(long, default_value_t = 400_000)]
    entities: u32,

    /// Co ile ticków liczyć hash stanu (0 = nigdy).
    #[arg(long, default_value_t = 1000)]
    hash_every: u64,

    /// Zapis ciągu hashy: „<tick> <hash>" po jednym w linii.
    #[arg(long)]
    out: Option<PathBuf>,

    /// Porównanie z zapisanym ciągiem; różnica → kod wyjścia 1 i raport.
    #[arg(long)]
    expect: Option<PathBuf>,

    /// Zapis stanu świata po przebiegu.
    #[arg(long)]
    save: Option<PathBuf>,

    /// Wczytanie stanu zamiast budowania świata z ziarna.
    #[arg(long)]
    load: Option<PathBuf>,

    /// Tryb interaktywny (stdin).
    #[arg(long, default_value_t = false)]
    console: bool,

    /// Eksport metryk do CSV.
    #[arg(long)]
    metrics: Option<PathBuf>,
}

fn main() -> std::process::ExitCode {
    match uruchom() {
        Ok(kod) => kod,
        Err(e) => {
            eprintln!("błąd: {e}");
            std::process::ExitCode::from(2)
        }
    }
}

/// Podpolecenia M1. Świadomie **opcjonalne**: bez podpolecenia narzędzie zachowuje się
/// tak jak w M0 (przebieg ticków świata syntetycznego), więc wywołania w CI i w skryptach
/// determinizmu M0 działają bez zmian.
#[derive(Subcommand, Debug)]
enum Command {
    /// Generacja świata bez GPU: raport czasów, hash terenu, statystyki.
    Generate(worldgen::GenerateArgs),
    /// Dwa przebiegi tego samego ziarna → porównanie hashy (M1 §7.1).
    Verify(worldgen::VerifyArgs),
    /// Podgląd wybranego pola generatora jako PNG — sanity-check bez GPU.
    Preview(worldgen::PreviewArgs),
    /// Populacja bez miasta: rachunek pamięci, spadek potrzeb, koło czasu (M3a).
    Agents(agents::AgentsArgs),
    /// Doba mieszkańca: planer, kolejka zdarzeń, ruch pieszy (M3b).
    Day(day::DayArgs),
    /// Sto lat gry w trybie demograficznym: cykl życia, migracja, status (M3c).
    Century(century::CenturyArgs),
    /// Etap 8: zaludnienie miasta M2 i cztery dopasowania statystyczne (M3d).
    Population(population::PopulationArgs),
    /// Artefakt fazy M3: doba w zaludnionym mieście przez systemy ECS §5.12 (M3d).
    M3day(m3day::M3DayArgs),
    /// Graf nawigacyjny i router: inspektor krawędzi, budżety zapytań (M4a).
    Nav(nav::NavArgs),
}

fn uruchom() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let args = Args::parse();

    match &args.command {
        Some(Command::Generate(a)) => return worldgen::generate(a),
        Some(Command::Verify(a)) => return worldgen::verify(a),
        Some(Command::Preview(a)) => return worldgen::preview(a),
        Some(Command::Agents(a)) => return agents::run(a),
        Some(Command::Day(a)) => return day::run(a),
        Some(Command::Century(a)) => return century::run(a),
        Some(Command::Population(a)) => return population::run(a),
        Some(Command::M3day(a)) => return m3day::run(a),
        Some(Command::Nav(a)) => return nav::run(a),
        None => {}
    }

    let world = match &args.load {
        Some(path) => {
            // Rejestr komponentów musi być ten sam co przy zapisie — bierzemy go
            // ze świeżo zbudowanego świata testowego (bez encji).
            let wzorzec = testworld::build(args.seed, 0);
            let w = load_world(path, wzorzec.components())?;
            eprintln!(
                "wczytano {} ({} encji, tick {})",
                path.display(),
                w.entity_count(),
                w.tick.0
            );
            w
        }
        None => testworld::build(args.seed, args.entities),
    };

    let schedule = testworld::schedule(&world).build()?;
    let mut app = App::new(world, schedule, args.threads);
    let mut metryki = args.metrics.as_ref().map(|_| MetricSink::new(100_000));

    eprintln!(
        "świat: {} encji · {} archetypów · {} systemów · {} wątków",
        app.world.entity_count(),
        app.world.archetypes().len(),
        app.schedule().system_count(),
        app.thread_count()
    );

    let oczekiwane = match &args.expect {
        Some(path) => Some(wczytaj_hashe(path)?),
        None => None,
    };

    let mut hashe: Vec<(u64, StateHash)> = Vec::new();
    let start = std::time::Instant::now();

    // Hash stanu początkowego — bez niego rozjazd w świecie startowym wyszedłby
    // dopiero po tysiącu ticków.
    if args.hash_every > 0 {
        hashe.push((app.world.tick.0, world_state_hash(&app.world)));
    }

    for _ in 0..args.ticks {
        app.tick();
        let tick = app.world.tick.0;
        if args.hash_every > 0 && tick.is_multiple_of(args.hash_every) {
            let hash = world_state_hash(&app.world);
            hashe.push((tick, hash));
            if let Some(oczekiwane) = &oczekiwane {
                if let Some((_, chciany)) = oczekiwane.iter().find(|(t, _)| *t == tick) {
                    if *chciany != hash {
                        return Ok(raport_rozbieznosci(&mut app, tick, *chciany, hash, &args));
                    }
                }
            }
        }
        if let Some(m) = &mut metryki {
            m.record(Tick(tick), "encje", i64::from(app.world.entity_count()));
            m.record(Tick(tick), "archetypy", app.world.archetypes().len() as i64);
            let flush = app.last_flush();
            m.record(Tick(tick), "spawn", i64::from(flush.spawned));
            m.record(Tick(tick), "despawn", i64::from(flush.despawned));
        }
    }

    let czas = start.elapsed();
    if args.ticks > 0 {
        eprintln!(
            "{} ticków w {:.2} s ({:.0} ticków/s)",
            args.ticks,
            czas.as_secs_f64(),
            args.ticks as f64 / czas.as_secs_f64().max(f64::MIN_POSITIVE)
        );
    }

    if let Some(path) = &args.out {
        let mut f = std::fs::File::create(path)?;
        for (tick, hash) in &hashe {
            writeln!(f, "{tick} {hash}")?;
        }
        eprintln!("zapisano {} hashy do {}", hashe.len(), path.display());
    }

    if let Some(oczekiwane) = &oczekiwane {
        // Brakujący wpis też jest rozbieżnością — inaczej krótszy przebieg
        // „przechodziłby" test.
        if oczekiwane.len() != hashe.len() {
            eprintln!(
                "różna liczba punktów kontrolnych: oczekiwano {}, policzono {}",
                oczekiwane.len(),
                hashe.len()
            );
            return Ok(std::process::ExitCode::from(1));
        }
        eprintln!("zgodność {} punktów kontrolnych", hashe.len());
    }

    if let Some(path) = &args.save {
        let raport = save_world(&app.world, path)?;
        eprintln!(
            "zapisano {} ({} B, {} sekcji, hash {})",
            path.display(),
            raport.bytes,
            raport.sections,
            raport.hash
        );
    }

    if let (Some(m), Some(path)) = (&metryki, &args.metrics) {
        m.export_csv(path)?;
        eprintln!("metryki → {}", path.display());
    }

    if args.console {
        konsola(&mut app.world)?;
    }

    Ok(std::process::ExitCode::SUCCESS)
}

fn wczytaj_hashe(path: &PathBuf) -> std::io::Result<Vec<(u64, StateHash)>> {
    let tresc = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for linia in tresc.lines() {
        let mut pola = linia.split_whitespace();
        let (Some(tick), Some(hash)) = (pola.next(), pola.next()) else {
            continue;
        };
        let tick: u64 = tick
            .parse()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "zły numer ticku"))?;
        let hash: StateHash = hash
            .parse()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "zły hash"))?;
        out.push((tick, hash));
    }
    Ok(out)
}

/// Raport rozbieżności: pierwszy niezgodny tick plus różnice pierwszych encji.
///
/// Sam numer ticku nie skraca szukania przyczyny — dlatego runner odtwarza świat
/// wzorcowy z zapisu, jeśli go ma, i pokazuje różnice encja po encji.
fn raport_rozbieznosci(
    app: &mut App,
    tick: u64,
    oczekiwany: StateHash,
    policzony: StateHash,
    args: &Args,
) -> std::process::ExitCode {
    eprintln!("ROZBIEŻNOŚĆ na ticku {tick}");
    eprintln!("  oczekiwano: {oczekiwany}");
    eprintln!("  policzono:  {policzony}");
    eprintln!(
        "  konfiguracja: seed {} · {} encji · {} wątków",
        args.seed,
        args.entities,
        app.thread_count()
    );
    eprintln!("{}", Inspector::archetypes(&app.world));
    eprintln!(
        "Aby zobaczyć różnice encja po encji, uruchom przebieg wzorcowy z --save \
         i porównaj: magnat-headless --load wzorzec.mgs --ticks 0 --console"
    );
    std::process::ExitCode::from(1)
}

fn konsola(world: &mut World) -> std::io::Result<()> {
    let mut console = Console::with_builtins();
    console.register(
        "sim.step",
        "sim.step <n> — komenda dostępna po wznowieniu pętli (M0: informacyjna)",
        |_, _| "sim.step działa w pętli App::tick — w trybie konsoli świat stoi\n".to_string(),
    );
    println!("konsola magnat-headless; 'help' wypisze komendy, 'quit' kończy");
    let stdin = std::io::stdin();
    for linia in stdin.lock().lines() {
        let linia = linia?;
        let linia = linia.trim();
        if linia == "quit" || linia == "exit" {
            break;
        }
        print!("{}", console.exec(world, linia));
        std::io::stdout().flush()?;
    }
    Ok(())
}
