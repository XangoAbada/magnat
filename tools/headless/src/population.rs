//! Scenariusz `population` — Etap 8 na prawdziwym mieście (M3d, WP10).
//!
//! Zaludnia miasto wygenerowane przez M2 i sprawdza **cztery dopasowania statystyczne**
//! z §7.4 dokumentu fazy: piramidę wieku, bezrobocie, korelację dochód↔mieszkanie
//! i histogram czasu dojazdu. Zwraca kod wyjścia 1, gdy któreś nie wychodzi — więc
//! nadaje się do macierzy ziaren w CI tak samo jak `--test consistency` z M2.
//!
//! To jest zarazem **wejście scenariusza `m3day`**: ten sam kod buduje świat, tylko
//! tamten puszcza na nim dobę.

use clap::Args;
use magnat_agents::{register, society, DemographyTable, NeedTable, Population};
use magnat_ecs::World;
use magnat_jobs::JobPool;
use magnat_voxel::MaterialRegistry;
use magnat_world::{
    generate, generate_city, generate_population, CityData, CityPlan, Difficulty, Populated,
    PopulationParams, PopulationReport, Terrain, WorldGenParams,
};
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Args, Debug, Clone)]
pub struct PopulationArgs {
    #[arg(long, default_value = "1")]
    pub seed: String,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "4km")]
    pub size: String,

    /// `coastal` | `mountain` | `lowland` | `river` | `desert`.
    #[arg(long, default_value = "lowland")]
    pub region: String,

    #[arg(long, default_value = "1990")]
    pub epoch: String,

    #[arg(long, default_value = "mixed")]
    pub profile: String,

    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Docelowa liczba mieszkańców; 0 = wylicz z pojemności miasta (§5.9).
    #[arg(long, default_value_t = 0)]
    pub citizens: u32,

    /// Ile prób zamiany mieszkań w kroku 7.
    #[arg(long, default_value_t = 200_000)]
    pub swaps: u32,

    /// Ile ziaren przebiec (macierz §7.4). Kolejne ziarna to `seed`, `seed+1`, …
    #[arg(long, default_value_t = 1)]
    pub runs: u32,
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

/// Buduje miasto M2 dla podanych parametrów. Wydzielone, bo używa go też `m3day`.
pub fn zbuduj_miasto(
    seed: u64,
    size: &str,
    region: &str,
    epoch: &str,
    profile: &str,
    pool: &JobPool,
) -> Result<CityData, Box<dyn std::error::Error>> {
    Ok(zbuduj_miasto_z_klimatem(seed, size, region, epoch, profile, pool)?.0)
}

/// To samo co [`zbuduj_miasto`], plus **normy klimatyczne środka miasta**.
///
/// Osobna funkcja, a nie zmieniona sygnatura tamtej: normy potrzebuje jeden
/// konsument (pogoda M8c), a pozostałych pięciu wywołań nie ma powodu przepisywać.
///
/// Normy są kopiowane z komórki klimatu pod środkiem miasta i to jest cała
/// ich droga do `sim/events`. Kopia, a nie zapytanie: `ClimateCell` ma 48 bajtów,
/// nie zmienia się nigdy, a zapytanie wymagałoby trzymania całego terenu przy życiu
/// przez cały przebieg — czyli gigabajtów pod jedną tablicę dwunastu liczb.
///
/// # Errors
/// Jak [`zbuduj_miasto`].
pub fn zbuduj_miasto_z_klimatem(
    seed: u64,
    size: &str,
    region: &str,
    epoch: &str,
    profile: &str,
    pool: &JobPool,
) -> Result<(CityData, magnat_world::ClimateCell), Box<dyn std::error::Error>> {
    let params = WorldGenParams {
        seed,
        size: size.parse()?,
        region: region.parse()?,
        epoch: epoch.parse()?,
        profile: profile.parse()?,
        difficulty: Difficulty::Normal,
    };
    params.validate()?;
    let (data, _) = generate(params, pool)?;
    let reg = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path(
        "materials",
    ))?);
    let terrain = Terrain::new(data, reg);
    let plan = CityPlan::from_world(&params);
    let city = generate_city(&plan, &terrain, terrain.materials(), pool)?;
    let klimat = {
        use magnat_world::TerrainQuery;
        *terrain.climate_at(city.center.x as i32, city.center.y as i32)
    };
    Ok((city, klimat))
}

/// Świat ECS z zarejestrowaną warstwą M3a i M3c, gotowy do zaludnienia.
pub fn swiat_agentow(seed: u64) -> Result<World, Box<dyn std::error::Error>> {
    let mut world = World::new(seed);
    register(&mut world, NeedTable::load_default()?);
    society::register_society(&mut world, DemographyTable::load_default()?);
    Ok(world)
}

/// Zaludnia świat i zwraca most do `sim/agents`.
pub fn zaludnij(
    world: &mut World,
    city: &CityData,
    citizens: u32,
    swaps: u32,
) -> Result<Populated, Box<dyn std::error::Error>> {
    let p = PopulationParams {
        target_population: (citizens > 0).then_some(citizens),
        unemployment_target_permille: None,
        commute_median_min: None,
        commute_swaps: Some(swaps),
    };
    Ok(generate_population(world, city, &p)?)
}

/// Wartość krytyczna χ² dla 21 stopni swobody przy p = 0,001 (korekta H-22).
const CHI2_21_P001: f64 = 46.8;

/// Kryteria §7.4 — zwraca opisy naruszeń.
fn ocena(r: &PopulationReport) -> Vec<String> {
    let mut zle = Vec::new();

    // `gen_everyone_has_home`: 100 % mieszkańców ma lokal.
    if r.homeless > 0 {
        zle.push(format!("gen_everyone_has_home: {} bez lokalu", r.homeless));
    }

    // `gen_unemployment`: |bezrobocie − cel| ≤ 1 pp = 10 ‰.
    let d = i32::from(r.unemployment_permille) - i32::from(r.unemployment_target_permille);
    if d.abs() > 10 {
        zle.push(format!(
            "gen_unemployment: {} ‰ wobec celu {} ‰ (różnica {} ‰ > 10)",
            r.unemployment_permille, r.unemployment_target_permille, d
        ));
    }

    // `gen_jobs_filled`: |JobSlot| ≈ |aktywni| × (1 + cel), odchylenie ≤ 2 %.
    if r.active > 0 {
        let oczekiwane = u64::from(r.active)
            * u64::from(1000 + u32::from(r.unemployment_target_permille))
            / 1000;
        let odchylenie =
            (i64::from(r.jobs_total) - oczekiwane as i64).abs() as f64 * 100.0 / oczekiwane as f64;
        if odchylenie > 2.0 {
            zle.push(format!(
                "gen_jobs_filled: {} etatów wobec oczekiwanych {oczekiwane} ({odchylenie:.1} % > 2 %)",
                r.jobs_total
            ));
        }
    }

    // `gen_age_pyramid`: χ² wobec piramidy epoki, 21 stopni swobody.
    //
    // Próg jest dla **p = 0,001**, a nie 0,05, i to jest poprawka do kryterium, nie
    // jego rozluźnienie (korekta H-22). Test χ² na poziomie α = 0,05 odrzuca prawdziwą
    // hipotezę w 5 % przebiegów **z definicji**, a §7.4 każe przepuścić macierz
    // 10 ziaren × 4 rozmiary — czyli 40 przebiegów, w których dwa fałszywe alarmy są
    // wartością oczekiwaną. Poprawka Bonferroniego na 40 porównań daje α = 0,00125;
    // 46,8 to próg χ²(21) dla 0,001, czyli nieco ostrożniejszy.
    let chi_pir = r.pyramid_chi2();
    if chi_pir > CHI2_21_P001 {
        zle.push(format!(
            "gen_age_pyramid: χ² = {chi_pir:.1} > {CHI2_21_P001} (p < 0,001)"
        ));
    }

    // `gen_income_housing`: Spearman ≥ 0,60.
    if r.income_housing_rho_centi < 60 {
        zle.push(format!(
            "gen_income_housing: ρ = {:.2} < 0,60",
            f64::from(r.income_housing_rho_centi) / 100.0
        ));
    }

    // `gen_commute_hist`: odchylenie mediany ≤ 5 %.
    if r.commute_target_min > 0 {
        let odchylenie = (i32::from(r.commute_median_min) - i32::from(r.commute_target_min)).abs()
            as f64
            * 100.0
            / f64::from(r.commute_target_min);
        if odchylenie > 5.0 {
            zle.push(format!(
                "gen_commute_hist: mediana {} min wobec celu {} min ({odchylenie:.1} % > 5 %)",
                r.commute_median_min, r.commute_target_min
            ));
        }
    }

    // `gen_skill_fit`: ≥ 70 % zatrudnionych ma dopasowanie ≥ 40.
    if r.skill_fit_permille < 700 {
        zle.push(format!("gen_skill_fit: {} ‰ < 700 ‰", r.skill_fit_permille));
    }
    zle
}

pub fn run(a: &PopulationArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let baza = parse_seed(&a.seed)?;
    let pool = JobPool::new(a.threads);
    let mut naruszen = 0usize;

    for k in 0..a.runs {
        let seed = baza + u64::from(k);
        let start = std::time::Instant::now();
        let city = zbuduj_miasto(seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
        let miasto_s = start.elapsed().as_secs_f64();

        let mut world = swiat_agentow(seed)?;
        let start = std::time::Instant::now();
        let p = zaludnij(&mut world, &city, a.citizens, a.swaps)?;
        let etap8_s = start.elapsed().as_secs_f64();

        println!("── ziarno {seed} ({} {}) ──", a.size, a.profile);
        println!(
            "miasto {miasto_s:.1} s · Etap 8 {etap8_s:.1} s · {} mieszkańców w świecie ECS",
            society::population(&world)
        );
        for l in p.report.lines() {
            println!("{l}");
        }
        histogram(&p.report);
        piramida(&p.report);

        let zle = ocena(&p.report);
        if zle.is_empty() {
            println!("§7.4: wszystkie dopasowania w normie");
        } else {
            naruszen += zle.len();
            for z in &zle {
                println!("  ✗ {z}");
            }
        }

        // Rachunek populacji musi się zamykać także po samej generacji.
        let ksiega = world.resource::<Population>();
        if ksiega.len() != p.report.citizens as usize {
            println!(
                "  ✗ księga populacji: {} wobec {} z raportu",
                ksiega.len(),
                p.report.citizens
            );
            naruszen += 1;
        }
    }

    if naruszen > 0 {
        eprintln!("{naruszen} naruszeń kryteriów §7.4");
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

fn piramida(r: &PopulationReport) {
    println!("piramida wieku (uzyskana ‖ docelowa, ‰)");
    let uzyskana = r.pyramid_permille();
    for (i, (a, b)) in uzyskana.iter().zip(&r.pyramid_target).enumerate() {
        if *a == 0 && *b == 0 {
            continue;
        }
        println!(
            "  {:>3}–{:<3} {a:>4} {:<22} {b:>4} {}",
            i * 5,
            i * 5 + 4,
            "#".repeat((*a / 4) as usize),
            "·".repeat((*b / 4) as usize)
        );
    }
}

fn histogram(r: &PopulationReport) {
    println!("czas dojazdu (uzyskany ‖ docelowy, ‰)");
    let hist = r.commute_permille();
    for (i, uzyskany) in hist.iter().enumerate() {
        let od = i * 5;
        let uzyskany = *uzyskany;
        let cel = r.commute_target_hist[i];
        println!(
            "  {od:>3}–{:<3} {uzyskany:>4} {:<26} {cel:>4} {}",
            od + 4,
            "#".repeat((uzyskany / 8) as usize),
            "·".repeat((cel / 8) as usize),
        );
    }
}
