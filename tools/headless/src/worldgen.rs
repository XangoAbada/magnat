//! Podpolecenia `generate` i `verify` — generacja świata bez GPU (M1 §1, artefakty 3 i 4).
//!
//! Headless-first jest kontraktem (00 §6): cała `sim/world` musi dać się uruchomić
//! i przetestować na maszynie CI bez karty graficznej. To jest to miejsce.

use crate::png;
use clap::Args as ClapArgs;
use magnat_jobs::JobPool;
use magnat_world::{generate as generate_world, Difficulty, WorldData, WorldGenParams};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(ClapArgs, Debug)]
pub struct GenerateArgs {
    #[arg(long, default_value = "1")]
    seed: String,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "4km")]
    size: String,

    /// `coastal` | `mountain` | `lowland` | `river` | `desert`.
    #[arg(long, default_value = "lowland")]
    region: String,

    /// `1950` | `1970` | `1990` | `2010` | `2020`.
    #[arg(long, default_value = "1990")]
    epoch: String,

    /// `industrial` | `port` | `university` | `tourist` | `agricultural` | `mixed`.
    #[arg(long, default_value = "mixed")]
    profile: String,

    #[arg(long, default_value_t = 0)]
    threads: usize,

    /// Zapis świata do pliku `.mgw`.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(ClapArgs, Debug)]
pub struct VerifyArgs {
    #[arg(long, default_value = "1")]
    seed: String,

    #[arg(long, default_value = "4km")]
    size: String,

    #[arg(long, default_value = "lowland")]
    region: String,

    /// Liczba przebiegów do porównania.
    #[arg(long, default_value_t = 2)]
    runs: u32,

    /// Liczba wątków w kolejnych przebiegach; podane wielokrotnie testuje niezależność
    /// od zrównoleglenia (M1 §7.1, `terrain_thread_invariant`).
    #[arg(long, value_delimiter = ',', default_value = "1,8")]
    threads: Vec<usize>,
}

/// Ziarno dziesiętnie albo szesnastkowo — `0xC0FFEE` z PRD §4.1 ma się dać wpisać wprost.
fn parse_seed(s: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let t = s.trim();
    let v = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(&hex.replace('_', ""), 16)?,
        None => t.replace('_', "").parse::<u64>()?,
    };
    Ok(v)
}

fn params(
    seed: &str,
    size: &str,
    region: &str,
    epoch: &str,
    profile: &str,
) -> Result<WorldGenParams, Box<dyn std::error::Error>> {
    let p = WorldGenParams {
        seed: parse_seed(seed)?,
        size: size.parse()?,
        region: region.parse()?,
        epoch: epoch.parse()?,
        profile: profile.parse()?,
        difficulty: Difficulty::Normal,
    };
    p.validate()?;
    Ok(p)
}

pub fn generate(a: &GenerateArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let p = params(&a.seed, &a.size, &a.region, &a.epoch, &a.profile)?;
    let pool = JobPool::new(a.threads);
    eprintln!(
        "generacja: seed {:#x} · {} · {} · {} · {} · {} wątków",
        p.seed,
        p.size.key(),
        p.region.key(),
        p.epoch.year(),
        p.profile.key(),
        pool.thread_count()
    );

    let (world, report) = generate_world(p, &pool)?;
    for line in report.lines() {
        println!("{line}");
    }

    if let Some(path) = &a.out {
        let bytes = magnat_world::save_mgw(&world, path)?;
        eprintln!("zapisano {} ({} B)", path.display(), bytes);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn verify(a: &VerifyArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let p = params(&a.seed, &a.size, &a.region, "1990", "mixed")?;
    let mut hashes = Vec::new();

    for run in 0..a.runs.max(1) {
        // Liczba wątków cyklicznie z listy — dwa przebiegi przy 1 i 8 wątkach to
        // najtańszy test niezależności od zrównoleglenia (00 §3.3).
        let threads = a.threads[(run as usize) % a.threads.len().max(1)];
        let pool = JobPool::new(threads);
        let (_, report) = generate_world(p, &pool)?;
        eprintln!(
            "przebieg {run}: {} wątków, {:.1} ms, hash {:032x}",
            pool.thread_count(),
            report.total_millis,
            report.terrain_hash.0
        );
        hashes.push(report.terrain_hash);
    }

    if hashes.windows(2).all(|w| w[0] == w[1]) {
        println!("hash terenu identyczny w {} przebiegach", hashes.len());
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("ROZBIEŻNOŚĆ hashy terenu — determinizm złamany (00 §3)");
        Ok(ExitCode::from(1))
    }
}


#[derive(ClapArgs, Debug)]
pub struct PreviewArgs {
    #[arg(long, default_value = "1")]
    seed: String,

    #[arg(long, default_value = "4km")]
    size: String,

    #[arg(long, default_value = "lowland")]
    region: String,

    #[arg(long, default_value = "1990")]
    epoch: String,

    #[arg(long, default_value = "mixed")]
    profile: String,

    /// Pole do narysowania: `height` | `water` | `biome` | `temp` | `precip` | `fertility`.
    #[arg(long, default_value = "height")]
    field: String,

    /// Maksymalny bok obrazu w pikselach; większe mapy są podpróbkowane.
    #[arg(long, default_value_t = 1024)]
    max_px: u32,

    #[arg(long, default_value_t = 0)]
    threads: usize,

    #[arg(long, default_value = "preview.png")]
    out: PathBuf,
}

pub fn preview(a: &PreviewArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let p = params(&a.seed, &a.size, &a.region, &a.epoch, &a.profile)?;
    let pool = JobPool::new(a.threads);
    let (world, report) = generate_world(p, &pool)?;
    eprintln!("generacja {:.1} ms", report.total_millis);

    let src = world.height.dim();
    let step = (src as u32).div_ceil(a.max_px).max(1) as usize;
    let w = src / step;
    let mut px = vec![0u8; w * w * 3];

    for iy in 0..w {
        for ix in 0..w {
            let c = color_at(&world, &a.field, ix * step, iy * step)?;
            // Obraz ma oś Y w dół, mapa w górę — odbicie, żeby północ była u góry.
            let o = ((w - 1 - iy) * w + ix) * 3;
            px[o..o + 3].copy_from_slice(&c);
        }
    }

    let bytes = png::write_rgb(&a.out, w as u32, w as u32, &px)?;
    println!("{} — {}×{} px, {} B", a.out.display(), w, w, bytes);
    Ok(ExitCode::SUCCESS)
}

/// Barwa piksela dla wybranego pola. Palety są celowo proste: to przyrząd diagnostyczny,
/// a nie ilustracja — liczy się, czy da się zobaczyć błąd, nie czy ładnie wygląda.
fn color_at(
    w: &WorldData,
    field: &str,
    x: usize,
    y: usize,
) -> Result<[u8; 3], Box<dyn std::error::Error>> {
    use magnat_world::WaterClass;
    let i = w.height.idx(x, y);
    let h = w.height[i];

    Ok(match field {
        "height" => {
            if h <= 0 {
                // Morze: głębiej = ciemniej.
                let t = (f32::from(-h) / 600.0).clamp(0.0, 1.0);
                [(20.0 + 30.0 * (1.0 - t)) as u8, (60.0 + 60.0 * (1.0 - t)) as u8, (110.0 + 80.0 * (1.0 - t)) as u8]
            } else {
                // Ląd: zieleń → brąz → biel, skala 0…192 m.
                let t = (f32::from(h) / 1920.0).clamp(0.0, 1.0);
                if t < 0.5 {
                    let k = t * 2.0;
                    [(70.0 + 110.0 * k) as u8, (120.0 + 40.0 * k) as u8, (60.0 + 30.0 * k) as u8]
                } else {
                    let k = (t - 0.5) * 2.0;
                    [(180.0 + 70.0 * k) as u8, (160.0 + 90.0 * k) as u8, (90.0 + 160.0 * k) as u8]
                }
            }
        }
        "water" => match w.water[i].class() {
            WaterClass::Dry => [40, 40, 40],
            WaterClass::Sea => [20, 60, 140],
            WaterClass::Lake => [60, 140, 200],
            WaterClass::River => [255, 255, 255],
        },
        "biome" => {
            let c = w.climate.get(
                x * magnat_world::WORK_CELL_M as usize / magnat_world::CLIMATE_CELL_M as usize,
                y * magnat_world::WORK_CELL_M as usize / magnat_world::CLIMATE_CELL_M as usize,
            );
            biome_color(c.biome)
        }
        "temp" | "precip" | "fertility" => {
            let cx = x * magnat_world::WORK_CELL_M as usize / magnat_world::CLIMATE_CELL_M as usize;
            let cy = y * magnat_world::WORK_CELL_M as usize / magnat_world::CLIMATE_CELL_M as usize;
            let c = w.climate.get(cx, cy);
            let t = match field {
                "temp" => (f32::from(c.mean_annual_temp_dc()) + 200.0) / 500.0,
                "precip" => c.annual_precip_mm() as f32 / 2000.0,
                _ => f32::from(c.soil_fertility.get()) / 100.0,
            }
            .clamp(0.0, 1.0);
            ramp(t)
        }
        other => return Err(format!("nieznane pole `{other}`").into()),
    })
}

/// Rampa niebieski → zielony → żółty → czerwony. Czytelna też w druku na szaro,
/// bo jasność rośnie monotonicznie.
fn ramp(t: f32) -> [u8; 3] {
    let r = (t * 3.0 - 1.0).clamp(0.0, 1.0);
    let g = (1.0 - (t * 2.0 - 1.0).abs()).clamp(0.0, 1.0);
    let b = (1.0 - t * 3.0).clamp(0.0, 1.0);
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

fn biome_color(b: magnat_core::Biome) -> [u8; 3] {
    use magnat_core::Biome::*;
    match b {
        Sea => [20, 60, 140],
        Lake => [60, 140, 200],
        River => [90, 170, 220],
        Marsh => [80, 110, 90],
        BroadleafForest => [50, 120, 45],
        ConiferForest => [30, 80, 55],
        MixedForest => [45, 100, 50],
        Grassland => [130, 165, 80],
        Cropland => [180, 175, 90],
        Scrub => [150, 140, 95],
        Rock => [130, 130, 130],
        Sand => [220, 205, 150],
        Snow => [240, 245, 250],
    }
}
