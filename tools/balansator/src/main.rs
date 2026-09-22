//! `balansator` — narzędzie strojenia gospodarki i bramek CI (M5e, WP13).

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use magnat_balansator::{calibrate, gates, report, run};

#[derive(Parser, Debug)]
#[command(
    name = "balansator",
    about = "Przebiegi strojeniowe gospodarki i bramki CI (M5 §7.4)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Przebiegi N ziaren jednego scenariusza; metryki doba po dobie do katalogu.
    Run(RunArgs),
    /// Werdykt bramek G1–G9 na katalogu metryk.
    Gate(GateArgs),
    /// Kalibracja diagramu podstawowego do parametrów IDM — offline (M4d/WP9, `T-1`).
    CalibrateVdf(calibrate::CalibrateArgs),
}

/// Domyślne rozmiary są **budżetem czasu bramki na PR**, nie wygodą.
///
/// Zmierzone na tym repozytorium (miasto `4km`, 3 000 mieszkańców, wydanie
/// release, 16 wątków sprzętowych, jedno ziarno na wątek symulacji):
///
/// | Co | Ile |
/// |---|---|
/// | budowa świata + Etap 8 + `retail::setup` | 1,2 s na ziarno |
/// | doba w rozbiegu (przed pierwszymi zakupami) | ~0,3 s |
/// | doba od ~10. (zakupy już ruszyły), 2 ziarna naraz | **1,1–1,2 s** |
/// | 60 dób × 1 ziarno + bliźniak G8, `--jobs 2` | 69 s |
/// | 40 dób × 2 ziarna + bliźniak G8, `--jobs 4` | 65 s |
///
/// Stąd liczba, której §7.4 oczekuje: **profil `ci` (8 ziaren × 365 dób) nie
/// mieści się w 10 minutach**. Jedno ziarno to ~7,3 min czasu procesora, a przy
/// `--jobs 4` dziewięć przebiegów (osiem ziaren plus bliźniak G8) idzie trzema
/// falami, czyli ponad 20 min. Puszczone wszystkie naraz (`--jobs 9`) mieszczą
/// się dopiero na krawędzi budżetu i tylko na maszynie o co najmniej dziewięciu
/// wolnych rdzeniach. Liczba jest tu zapisana **zmierzona, nie życzeniowa** —
/// obniżenie kosztu doby należy do `U-24` (dwa wywołania routera M4 na każdy
/// zakup), a nie do balansatora.
///
/// Miasto 28,5 tys. mieszkańców liczy dobę ~15 s i **nie nadaje się** do bramki
/// na PR — to jest właśnie powód tych wartości domyślnych, a nie ich skutek.
#[derive(Args, Debug)]
struct RunArgs {
    /// Ile kolejnych ziaren policzyć, od `--seed0`.
    #[arg(long, default_value_t = 8)]
    seeds: u32,

    /// Ile dób gry przebiec. Kalendarz ma 360 dób (`K-1`), więc 365 to rok z okładem.
    #[arg(long, default_value_t = 365)]
    days: u16,

    #[arg(long, value_enum, default_value = "base")]
    scenario: run::Scenario,

    #[arg(long, default_value = "runs")]
    out: PathBuf,

    /// Ile ziaren liczyć równolegle. Każde dostaje `JobPool::new(1)`.
    #[arg(long, default_value_t = 4)]
    jobs: usize,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "4km")]
    size: String,

    /// Docelowa liczba mieszkańców; 0 = z pojemności miasta.
    #[arg(long, default_value_t = 3_000)]
    citizens: u32,

    #[arg(long, default_value_t = 1)]
    seed0: u64,
}

#[derive(Args, Debug)]
struct GateArgs {
    /// Katalog z plikami metryk (`*.ron`).
    dir: PathBuf,

    /// `ci` = G1–G3, G5, G7–G9; `nightly` = wszystkie.
    #[arg(long, default_value = "ci")]
    profile: String,

    /// Dopisek raportu Markdown.
    #[arg(long)]
    report: Option<PathBuf>,
}

fn main() -> ExitCode {
    match uruchom() {
        Ok(kod) => kod,
        Err(e) => {
            eprintln!("błąd: {e}");
            ExitCode::from(2)
        }
    }
}

fn uruchom() -> Result<ExitCode, Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Run(a) => przebieg(&a),
        Command::Gate(a) => bramki(&a),
        Command::CalibrateVdf(a) => calibrate::run(&a),
    }
}

fn przebieg(a: &RunArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let cfg = run::RunCfg {
        scenario: a.scenario,
        days: a.days,
        size: a.size.clone(),
        citizens: a.citizens,
    };
    let start = std::time::Instant::now();
    let wyniki = run::many(&cfg, a.seed0, a.seeds, a.jobs);
    let sciezki = run::zapisz(&a.out, &wyniki)?;
    println!(
        "{} przebiegów ({} dób, scenariusz {}) w {:.1} s → {}",
        sciezki.len(),
        a.days,
        a.scenario.key(),
        start.elapsed().as_secs_f64(),
        a.out.display()
    );
    // Brakujący przebieg to błąd, a nie mniejsza próbka: bramka policzona na
    // ośmiu ziarnach z dziesięciu wygląda tak samo zielono jak na dziesięciu.
    let oczekiwane = a.seeds as usize + usize::from(a.seeds > 0);
    Ok(if sciezki.len() == oczekiwane {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "BŁĄD: policzono {} z {oczekiwane} przebiegów",
            sciezki.len()
        );
        ExitCode::FAILURE
    })
}

fn bramki(a: &GateArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let profil = match a.profile.as_str() {
        "ci" => gates::Profile::Ci,
        "nightly" => gates::Profile::Nightly,
        inne => return Err(format!("nieznany profil {inne}; ci | nightly").into()),
    };
    let runs = run::wczytaj(&a.dir)?;
    if runs.is_empty() {
        return Err(format!("brak plików metryk w {}", a.dir.display()).into());
    }
    let werdykty = gates::evaluate(&runs, profil, run::min_margin_bp()?);

    println!(
        "── bramki ({}, {} przebiegów) ─────────────────────────",
        a.profile,
        runs.len()
    );
    for g in &werdykty {
        println!(
            "{:<3} {:<22} {:<10} {}",
            g.gate,
            g.name,
            g.verdict.slowo(),
            g.value
        );
        if !g.pass() {
            println!("    próg: {}", g.threshold);
        }
    }

    if let Some(p) = &a.report {
        std::fs::write(p, report::markdown(&runs, &werdykty, &a.profile))?;
        println!("raport → {}", p.display());
    }

    // Pominięcie wywraca **bieg nocny** (`D-N17`, `R2-WP24`): w profilu `nightly`
    // bramka bez werdyktu jest błędem konfiguracji, a nie stanem świata.
    Ok(if werdykty.iter().any(|g| g.verdict.blokuje(profil)) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
