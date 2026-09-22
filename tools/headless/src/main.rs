//! `magnat-headless` — runner symulacji bez GPU (M0 §5.11, WP-12).
//!
//! Jedno narzędzie obsługuje trzy rzeczy, których M0 musi dowieść: że ten sam seed
//! daje ten sam ciąg hashy niezależnie od liczby wątków, że zapis i wznowienie są
//! nieodróżnialne od przebiegu ciągłego, i że rozbieżność da się zlokalizować
//! co do części stanu (archetyp, arena, zasób), a nie tylko co do ticku.

#![forbid(unsafe_code)]

mod agents;
mod century;
mod day;
mod dryrun;
mod export_drains;
mod m3day;
mod m5shop;
mod m7_miasto;
mod m7labor;
mod m8_miasto;
mod m9session;
mod nav;
mod testworld;
mod worldgen;

use clap::{Parser, Subcommand};
use magnat_core::Tick;
use magnat_devtools::{Console, Inspector, MetricSink};
use magnat_ecs::{App, World};
use magnat_headless::population;
use magnat_io::{load_world, save_world, state_hash_parts, world_state_hash, StateHash};
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
    /// Wynik podfazy M5b: sklepy z magazynem, zakupy i rozliczenie pieniądza.
    M5shop(m5shop::M5ShopArgs),
    /// Wynik podfazy M7b: rynek pracy i pensje emergentne w mieście (M7b).
    M7labor(m7labor::M7LaborArgs),
    /// **Artefakt fazy M7**: pełne miasto — detal, produkcja, praca, firmy AI, makro.
    /// Szok ceny zewnętrznej +40 %: odpływ masy i cena półkowa (`R2-WP34`, M6 §7.7).
    ExportDrains(export_drains::ExportDrainsArgs),
    M7miasto(m7_miasto::M7MiastoArgs),
    /// Wynik podfazy M8a: budżet miasta, siedem danin i cykl życia należności.
    M8miasto(m8_miasto::M8MiastoArgs),
    /// **Wynik podfazy M9a**: sesja gry z pliku parametrów, dziennik wejść i replay.
    NewGame(m9session::M9SessionArgs),
    /// **Artefakt A fazy M10**: historia „na sucho" — 30–100 lat makro przed startem
    /// partii, bramki Etapu 10 i kronika (M10a/WP10.3).
    DryRun(dryrun::DryRunArgs),
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
        Some(Command::M5shop(a)) => return m5shop::run(a),
        Some(Command::M7labor(a)) => return m7labor::run(a),
        Some(Command::ExportDrains(a)) => return export_drains::run(a),
        Some(Command::M7miasto(a)) => return m7_miasto::run(a),
        Some(Command::M8miasto(a)) => return m8_miasto::run(a),
        Some(Command::NewGame(a)) => return m9session::run(a),
        Some(Command::DryRun(a)) => return dryrun::run(a),
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
    // Hash w częściach (archetyp, arena, zasób) przy każdym punkcie kontrolnym —
    // tylko przy `--out`, bo to on robi wzorzec, z którym raport rozbieżności
    // porównuje części (N1.11).
    let mut czesci: Vec<(u64, Vec<(String, StateHash)>)> = Vec::new();
    let start = std::time::Instant::now();

    // Hash stanu początkowego — bez niego rozjazd w świecie startowym wyszedłby
    // dopiero po tysiącu ticków. **Porównany z `--expect` jak każdy inny** (N1.11):
    // do E1 był liczony i zapisywany, ale nie sprawdzany.
    for krok in 0..=args.ticks {
        if krok > 0 {
            app.tick();
        }
        let tick = app.world.tick.0;
        if args.hash_every > 0 && (krok == 0 || tick.is_multiple_of(args.hash_every)) {
            let hash = world_state_hash(&app.world);
            hashe.push((tick, hash));
            if args.out.is_some() {
                czesci.push((tick, state_hash_parts(&app.world)));
            }
            if let Some((_, chciany)) = oczekiwane
                .as_ref()
                .and_then(|o| o.iter().find(|(t, _)| *t == tick))
            {
                if *chciany != hash {
                    return Ok(raport_rozbieznosci(&mut app, tick, *chciany, hash, &args));
                }
            }
        }
        if krok == 0 {
            continue;
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
        let mut f = std::fs::File::create(plik_czesci(path))?;
        for (tick, lista) in &czesci {
            for (nazwa, hash) in lista {
                writeln!(f, "{tick}\t{nazwa}\t{hash}")?;
            }
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

/// Plik z hashem w częściach, obok pliku hashy: `run_1.hashes` → `run_1.hashes.parts`.
fn plik_czesci(path: &std::path::Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".parts");
    PathBuf::from(p)
}

/// Części wzorca z ticku `tick`: `nazwa → hash`. Pusta, gdy wzorzec nie ma pliku części.
fn wczytaj_czesci(path: &std::path::Path, tick: u64) -> Vec<(String, String)> {
    let Ok(tresc) = std::fs::read_to_string(plik_czesci(path)) else {
        return Vec::new();
    };
    tresc
        .lines()
        .filter_map(|l| {
            let mut pola = l.split('\t');
            let (t, n, h) = (pola.next()?, pola.next()?, pola.next()?);
            (t.parse::<u64>().ok()? == tick).then(|| (n.to_string(), h.to_string()))
        })
        .collect()
}

/// Nazwy części, które różnią się między wzorcem a bieżącym stanem, w kolejności
/// kanonicznej. Część obecna tylko po jednej stronie też jest różnicą.
fn rozne_czesci(wzorzec: &[(String, String)], biezace: &[(String, StateHash)]) -> Vec<String> {
    let mut nazwy: Vec<&str> = wzorzec
        .iter()
        .map(|(n, _)| n.as_str())
        .chain(biezace.iter().map(|(n, _)| n.as_str()))
        .collect();
    nazwy.sort_unstable();
    nazwy.dedup();
    nazwy
        .into_iter()
        .filter(|n| {
            let a = wzorzec.iter().find(|(m, _)| m == n).map(|(_, h)| h.clone());
            let b = biezace
                .iter()
                .find(|(m, _)| m == n)
                .map(|(_, h)| h.to_string());
            a != b
        })
        .map(str::to_string)
        .collect()
}

/// Raport rozbieżności: pierwszy niezgodny tick i **które części stanu** się rozjechały
/// — archetyp po składzie komponentów, arena albo zasób (N1.11, `M0#8`).
///
/// Do E1 raport wypisywał tylko listę archetypów bieżącego świata, a jego opis
/// obiecywał „różnice encja po encji", których nie liczył. Części porównuje się
/// z plikiem `.parts`, który `--out` zapisuje obok hashy wzorca.
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
    let wzorzec = args
        .expect
        .as_deref()
        .map(|p| wczytaj_czesci(p, tick))
        .unwrap_or_default();
    if wzorzec.is_empty() {
        eprintln!("  wzorzec nie ma pliku .parts — części stanu nieporównane");
        eprintln!("{}", Inspector::archetypes(&app.world));
    } else {
        for nazwa in rozne_czesci(&wzorzec, &state_hash_parts(&app.world)) {
            eprintln!("  rozjechana część stanu: {nazwa}");
        }
    }
    std::process::ExitCode::from(1)
}

fn konsola(world: &mut World) -> std::io::Result<()> {
    // Bez `sim.step`: w trybie konsoli świat stoi, a komenda niczego nie robiła (N1.11).
    let mut console = Console::with_builtins();
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
