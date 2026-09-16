//! Wynik podfazy M7b: **pensje emergentne** w prawdziwym mieście.
//!
//! Scenariusz odpowiada na jedno pytanie: czy niedobór zawodu podnosi stawkę w ofercie
//! bez żadnej tabeli płac w kodzie. Wydruk pokazuje dla każdego zawodu, jak przez
//! przebieg zmieniła się mediana **zawartych umów** — a ta jest agregatem po tym,
//! co rynek zrobił, a nie odczytem z danych.
//!
//! Czego tu nie ma: rynku detalicznego i produkcji. Powód stoi w nagłówku
//! `magnat_headless::labor` — doba pełnego miasta kosztuje kilkanaście sekund,
//! a ten przebieg potrzebuje ich dziewięćdziesięciu.

use std::error::Error;
use std::process::ExitCode;

use clap::Args;
use magnat_core::Money;
use magnat_economy::labor::{LaborDay, LaborHandle, LaborSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_firms::systems::FirmSystem;
use magnat_headless::labor;
use magnat_jobs::JobPool;

#[derive(Args, Debug)]
pub struct M7LaborArgs {
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    #[arg(long, default_value = "4km")]
    pub size: String,
    #[arg(long, default_value = "lowland")]
    pub region: String,
    #[arg(long, default_value = "1990")]
    pub epoch: String,
    #[arg(long, default_value = "mixed")]
    pub profile: String,
    /// Liczba mieszkańców; 0 = tylu, ilu mieści miasto.
    #[arg(long, default_value_t = 0)]
    pub citizens: u32,
    #[arg(long, default_value_t = 90)]
    pub days: u32,
    #[arg(long, default_value_t = 0)]
    pub threads: usize,
    /// Pasmo bezrobocia w promilach, w którym przebieg ma się zamknąć —
    /// `--expect 30:90` odpowiada kryterium WP4 (3–9 %).
    #[arg(long)]
    pub expect: Option<String>,
}

pub fn run(a: &M7LaborArgs) -> Result<ExitCode, Box<dyn Error>> {
    let pool = JobPool::new(a.threads);
    println!("Buduję miasto {} i zaludniam…", a.size);
    let mut m = labor::setup(
        a.seed, &a.size, &a.region, &a.epoch, &a.profile, a.citizens, &pool,
    )?;
    println!(
        "Firmy: {}, zakłady: {}, stanowiska: {}, etatów razem: {}, obsadzonych przy starcie: {}",
        m.report.firms, m.report.sites, m.report.positions, m.slots, m.report.hired
    );

    let mut builder = ScheduleBuilder::new();
    builder.add(FirmSystem::new());
    builder.add(LaborSystem::new());
    let schedule = builder.build()?;
    let world = std::mem::replace(&mut m.world, magnat_ecs::World::new(a.seed));
    let mut app = App::new(world, schedule, a.threads);

    let mut ostatni = LaborDay::default();
    let mut suma = Suma::default();
    // Mediana odniesienia powstaje **po pierwszej dobie**, nie przed nią: przed nią
    // nie ma ani jednej zawartej umowy, więc nie ma z czym porównywać.
    let mut start = std::collections::BTreeMap::new();
    for d in 0..a.days {
        for _ in 0..1440 {
            app.tick();
        }
        let dzien = dzien_rynku(&app.world);
        if d == 0 {
            start = mediany(&app.world);
        }
        suma.zbierz(dzien);
        ostatni = dzien;
        if d % 15 == 0 || d + 1 == a.days {
            println!(
                "doba {d:>3}: bezrobocie {:>4} ‰, wakaty {:>5}, zatrudnień {:>4}, \
                 podwyżek {:>4}, ofert bezpośrednich {:>3}",
                dzien.unemployment_permille(),
                dzien.vacancies,
                dzien.hires,
                dzien.raises,
                dzien.headhunts
            );
        }
    }

    println!("\nRazem przez {} dób:", a.days);
    println!(
        "  zatrudnień {}, odejść {}, zwolnień {}, wyjść z rynku pracy {}",
        suma.hires, suma.quits, suma.dismissals, suma.left
    );
    println!(
        "  podwyżek {}, ofert zamrożonych na suficie marży {}, ofert bezpośrednich {}",
        suma.raises, suma.frozen, suma.headhunts
    );
    println!("  szkoleń {}", suma.trained);

    println!("\nMediana zawartych umów per zawód (tam, gdzie zmieniła się o ≥ 1 %):");
    let koniec = mediany(&app.world);
    let mut zmiany: Vec<(String, i64, i64, i64)> = Vec::new();
    for (klucz, po) in &koniec {
        let przed = start.get(klucz).copied().unwrap_or(0);
        if przed == 0 || *po == 0 {
            continue;
        }
        let delta = (po - przed) * 100 / przed;
        if delta.abs() >= 1 {
            zmiany.push((klucz.clone(), przed, *po, delta));
        }
    }
    zmiany.sort_by_key(|(_, _, _, d)| -d);
    for (klucz, przed, po, delta) in zmiany.iter().take(20) {
        println!("  {klucz:<28} {przed:>9} → {po:>9} gr  ({delta:+} %)");
    }
    if zmiany.is_empty() {
        println!("  (żadna mediana nie drgnęła — rynek pracy stoi)");
    }

    let stopa = ostatni.unemployment_permille();
    println!(
        "\nNa koniec: siła robocza {}, pracuje {}, bez pracy {} ({} ‰)",
        ostatni.labour_force, ostatni.employed, ostatni.unemployed, stopa
    );
    if let Some(pasmo) = &a.expect {
        let (lo, hi) = rozbierz_pasmo(pasmo)?;
        if !(lo..=hi).contains(&stopa) {
            eprintln!("BŁĄD: bezrobocie {stopa} ‰ poza pasmem {lo}–{hi} ‰");
            return Ok(ExitCode::FAILURE);
        }
        println!("Pasmo {lo}–{hi} ‰ dotrzymane.");
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Default)]
struct Suma {
    hires: u32,
    quits: u32,
    dismissals: u32,
    left: u32,
    raises: u32,
    frozen: u32,
    headhunts: u32,
    trained: u32,
}

impl Suma {
    fn zbierz(&mut self, d: LaborDay) {
        self.hires += d.hires;
        self.quits += d.quits;
        self.dismissals += d.dismissals;
        self.left += d.left_force;
        self.raises += d.raises;
        self.frozen += d.frozen;
        self.headhunts += d.headhunts;
        self.trained += d.trained;
    }
}

fn dzien_rynku(world: &magnat_ecs::World) -> LaborDay {
    world
        .get_resource::<LaborHandle>()
        .and_then(LaborHandle::get)
        .map(magnat_economy::labor::LaborMarket::last_day)
        .unwrap_or_default()
}

/// Mediana zawartych umów per `zawód @ dzielnica`.
fn mediany(world: &magnat_ecs::World) -> std::collections::BTreeMap<String, i64> {
    let Some(m) = world
        .get_resource::<LaborHandle>()
        .and_then(LaborHandle::get)
    else {
        return std::collections::BTreeMap::new();
    };
    m.stats()
        .per_role
        .iter()
        .filter(|(_, s)| s.median_wage_accepted > Money::ZERO)
        .map(|((role, district), s)| {
            (
                format!("{}@{}", m.roles().key(*role), district.0),
                s.median_wage_accepted.get(),
            )
        })
        .collect()
}

fn rozbierz_pasmo(s: &str) -> Result<(u16, u16), Box<dyn Error>> {
    let (a, b) = s
        .split_once(':')
        .ok_or("pasmo podaje się jako `min:max` w promilach, np. 30:90")?;
    Ok((a.trim().parse()?, b.trim().parse()?))
}
