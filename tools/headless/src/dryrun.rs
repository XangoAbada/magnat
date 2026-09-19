//! Scenariusz `dry-run` — **artefakt A fazy M10** (§1 dokumentu fazy, WP10.3).
//!
//! Stawia miasto, przepuszcza je przez `sim/macro::dry_run` i wypisuje to, po co
//! historia „na sucho" w ogóle istnieje:
//!
//! 1. **oś czasu** — wpisy kronikarskie z `provenance: DryRun`, rok po roku;
//! 2. **bramki Etapu 10** — osiem liczb z progiem i werdyktem, także dla tych,
//!    które są czerwone; bramka bez pomiaru jest deklaracją, nie bramką;
//! 3. **relacje dostawców** — ile firm ma stałego dostawcę i ilu jest różnych;
//! 4. **rozwarstwienie** — kwartyle majątku, bo to one odróżniają świat „zużyty"
//!    od świata postawionego na zero;
//! 5. **pieniądz** — suma świata przed i po, z tolerancją zero.
//!
//! Determinizm sprawdza się jak w pozostałych scenariuszach: `--expect` porównuje
//! hash świata po rozwinięciu z zapisanym wcześniej przez `--out`.

use std::process::ExitCode;

use clap::Args;
use magnat_agents::{bootstrap_day, register_day};
use magnat_core::GoodId;
use magnat_headless::full;
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use magnat_macro::{dry_run, lift, DryRunConfig, MacroParams};

#[derive(Args, Debug)]
pub struct DryRunArgs {
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "4km")]
    pub size: String,

    #[arg(long, default_value = "lowland")]
    pub region: String,

    #[arg(long, default_value = "1990")]
    pub epoch: String,

    #[arg(long, default_value = "mixed")]
    pub profile: String,

    /// Ile lat historii liczyć przed startem partii (30–100, §5.7).
    #[arg(long, default_value_t = 30)]
    pub years: u16,

    /// Ile mieszkańców zaludnić. 0 = z pojemności miasta.
    #[arg(long, default_value_t = 0)]
    pub citizens: u32,

    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Ile dób obejmuje krok w latach wczesnych (§5.7: 6).
    #[arg(long, default_value_t = 6)]
    pub step_days: u8,

    /// Ile wpisów kronikarskich wypisać. 0 = wszystkie.
    #[arg(long, default_value_t = 40)]
    pub chronicle: usize,

    /// Zapisz hash świata po rozwinięciu.
    #[arg(long)]
    pub out: Option<String>,

    /// Porównaj hash świata po rozwinięciu z zapisanym.
    #[arg(long)]
    pub expect: Option<String>,
}

/// Ile pozycji ma koszyk podstawowy do bramki 8 i ile sztuk każdej na miesiąc.
///
/// Koszyk buduje **wołający**, bo to on ma katalog towarów (`sim/macro` go nie ma
/// i mieć nie powinien). Tu bierzemy towary o najszerszej dostępności w mieście;
/// docelowy koszyk `data/economy/cpi.ron` składa scenariusz gry, nie ten runner.
const KOSZYK_POZYCJI: usize = 12;
const KOSZYK_SZTUK_MIESIECZNIE: i64 = 30;

pub fn run(a: &DryRunArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pool = JobPool::new(a.threads);
    let t0 = std::time::Instant::now();
    let city = zbuduj_miasto(a.seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
    let mut world = swiat_agentow(a.seed)?;
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, a.citizens, 200_000)?;
    full::setup(
        &mut world,
        &city,
        zaludnione.places.clone(),
        zaludnione.travel_oracle(),
        &zaludnione.traffic,
        a.seed,
        &pool,
    )?;
    bootstrap_day(&mut world, 0);
    let budowa = t0.elapsed();

    let pieniadz_przed = pieniadz(&world);
    let koszyk = koszyk_miasta(&world);

    let cfg = DryRunConfig {
        seed: a.seed,
        years: a.years.clamp(1, 100),
        start_year: 1990 - i32::from(a.years),
        step_days_early: a.step_days.max(1),
        basket: koszyk,
        ..DryRunConfig::default()
    };
    let t1 = std::time::Instant::now();
    let wynik = dry_run(&cfg, &mut world, &MacroParams::default());
    let historia = t1.elapsed();

    // ── raport ───────────────────────────────────────────────────────────────
    println!(
        "== historia na sucho: {} lat, ziarno {} ==",
        cfg.years, a.seed
    );
    println!(
        "miasto {} zbudowane w {:.1} s; {} kroków makro w {:.1} s",
        a.size,
        budowa.as_secs_f64(),
        wynik.steps,
        historia.as_secs_f64()
    );

    println!("\n-- oś czasu ({} wpisów) --", wynik.chronicle.len());
    let ile = if a.chronicle == 0 {
        wynik.chronicle.len()
    } else {
        a.chronicle
    };
    for e in wynik.chronicle.iter().take(ile) {
        match e.district {
            Some(d) => println!(
                "  {} {:<28} dzielnica {:>3}  {:+} ‰",
                e.year,
                e.kind.key(),
                d.0,
                e.magnitude
            ),
            None => println!("  {} {:<28} {:+}", e.year, e.kind.key(), e.magnitude),
        }
    }

    println!("\n-- bramki Etapu 10 --");
    for c in &wynik.verification.checks {
        let werdykt = match (c.measured, c.pass) {
            (false, _) => "NIE ZMIERZONO",
            (true, true) => "ok",
            (true, false) => "CZERWONA",
        };
        println!(
            "  {} {:<30} {:>10}  [{}..{}]  {}",
            c.id,
            c.key,
            c.value,
            c.lo,
            if c.hi == i64::MAX {
                i64::from(u32::MAX)
            } else {
                c.hi
            },
            werdykt
        );
    }
    println!(
        "  rundy naprawcze: {} z {}",
        wynik.rebalance_rounds, cfg.max_rebalance_rounds
    );

    println!("\n-- relacje dostawców --");
    let z_dostawca = wynik
        .state
        .firms
        .iter()
        .filter(|f| !f.suppliers.is_empty())
        .count();
    let par: usize = wynik.state.firms.iter().map(|f| f.suppliers.len()).sum();
    println!(
        "  {z_dostawca} z {} firm ma stałego dostawcę; {par} par (firma, towar)",
        wynik.state.firms.len()
    );

    println!("\n-- rozwarstwienie --");
    let mut kwartyle: Vec<(i64, i64, i64, i64)> = wynik
        .state
        .cells
        .iter()
        .map(|c| {
            (
                c.wealth_q[0].get(),
                c.wealth_q[1].get(),
                c.wealth_q[2].get(),
                c.wealth_q[3].get(),
            )
        })
        .collect();
    kwartyle.sort_unstable();
    if let (Some(dol), Some(gora)) = (kwartyle.first(), kwartyle.last()) {
        println!(
            "  najuboższa komórka: {} .. {} gr;  najzamożniejsza: {} .. {} gr",
            dol.0, dol.3, gora.0, gora.3
        );
    }
    if let Some(r) = wynik.lower {
        println!(
            "  rozwinięcie: {} gospodarstw, {} firm, {} rozkraczonych, nienaniesione {} gr",
            r.households,
            r.firms,
            r.straddling_households,
            r.unsettled.get()
        );
    }

    println!("\n-- pieniądz --");
    let pieniadz_po = pieniadz(&world);
    println!("  przed {pieniadz_przed} gr, po {pieniadz_po} gr");
    if pieniadz_przed != pieniadz_po {
        eprintln!(
            "BŁĄD: historia na sucho zmieniła sumę pieniądza o {} gr",
            pieniadz_po - pieniadz_przed
        );
        return Ok(ExitCode::FAILURE);
    }

    // ── determinizm ──────────────────────────────────────────────────────────
    let hash = world_state_hash(&world).to_string();
    if let Some(path) = &a.out {
        std::fs::write(path, format!("{hash}\n"))?;
        println!("\nhash świata po rozwinięciu zapisany w {path}");
    }
    if let Some(path) = &a.expect {
        let oczekiwany = std::fs::read_to_string(path)?;
        let oczekiwany = oczekiwany.trim();
        if oczekiwany != hash {
            eprintln!("BŁĄD: hash {hash} wobec oczekiwanego {oczekiwany}");
            return Ok(ExitCode::FAILURE);
        }
        println!("\nhash zgodny z {path}");
    }

    Ok(ExitCode::SUCCESS)
}

fn pieniadz(world: &magnat_ecs::World) -> i64 {
    magnat_agents::total_money(world)
        + world
            .get_resource::<magnat_economy::Books>()
            .map_or(0, |b| b.total_balance().get())
}

/// Towary o najszerszej dostępności w mieście, po `KOSZYK_SZTUK_MIESIECZNIE` sztuk.
fn koszyk_miasta(world: &magnat_ecs::World) -> Vec<(GoodId, i64)> {
    let st = lift(world);
    let mut licznik: std::collections::BTreeMap<u16, u32> = std::collections::BTreeMap::new();
    for f in &st.firms {
        for (g, cena) in f.price.iter() {
            if cena.get() > 0 {
                *licznik.entry(g.0).or_default() += 1;
            }
        }
    }
    let mut v: Vec<(u32, u16)> = licznik.into_iter().map(|(g, n)| (n, g)).collect();
    v.sort_unstable_by(|a, b| b.cmp(a));
    v.into_iter()
        .take(KOSZYK_POZYCJI)
        .map(|(_, g)| (GoodId(g), KOSZYK_SZTUK_MIESIECZNIE))
        .collect()
}
