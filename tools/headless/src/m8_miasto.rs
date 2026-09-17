//! Scenariusz `m8-miasto` — **wynik podfazy M8a** (pieniądz publiczny).
//!
//! To jest `m7-miasto` z dołożoną stroną publiczną: te same firmy, ten sam rynek
//! pracy, ten sam model makro — plus siedem danin, budżet i kataster. Osobny
//! scenariusz, a nie przełącznik w tamtym, z powodu nazwanego w `city::setup`:
//! włączenie podatków zmienia świat, na którym skalibrowano bramki G1–G9, a to
//! jest robota panelu balansatora, czyli M8e.
//!
//! Runner odpowiada na trzy pytania i tylko na te trzy:
//! 1. **czy budżet się domyka** — `Σ Assessed = Σ Settled + Σ Overdue + Σ Abated`
//!    dla każdej pary (danina, okres), tolerancja zero groszy (test T1);
//! 2. **czy pieniądz się zachowuje** — suma świata razem z kontem miasta stała;
//! 3. **czy każda danina żyje** — siedem wierszy wpływów, a nie cztery i trzy zera.
//!
//! Trzecie pytanie jest najważniejsze, bo na nie najłatwiej odpowiedzieć źle:
//! danina, której nikt nigdy nie naliczył, przechodzi każdy test domknięcia.

use std::process::ExitCode;

use clap::Args;
use magnat_agents::{
    bootstrap_day, register_day, society, DayLoopSystem, DeprivationEffectsSystem,
    HouseholdStockSystem, NeedDecaySystem, NoInheritance, ReplanCooldownSystem, SkillDriftSystem,
    SocietySystem,
};
use magnat_city::{City, CitySystem};
use magnat_core::{Money, TaxKind, TAX_KIND_COUNT};
use magnat_economy::corpfin::system::InsolvencySystem;
use magnat_economy::labor::LaborSystem;
use magnat_economy::MarketSystem;
use magnat_ecs::{App, ScheduleBuilder};
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_headless::{city as city_bridge, full};
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use magnat_macro::MacroSystem;
use magnat_traffic::TrafficSystem;

#[derive(Args, Debug)]
pub struct M8MiastoArgs {
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

    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Ile dób gry przebiec. Domyślnie 90, bo pierwsza deklaracja VAT wypada
    /// dwudziestego drugiego miesiąca, a bez trzech miesięcy nie widać, czy rejestr
    /// należności domyka się **po** terminie, a nie tylko przed nim.
    #[arg(long, default_value_t = 90)]
    pub days: u32,

    /// Docelowa liczba mieszkańców; 0 = z pojemności miasta.
    #[arg(long, default_value_t = 0)]
    pub citizens: u32,

    /// Co ile ticków liczyć hash stanu; 0 = nie liczyć.
    #[arg(long, default_value_t = 43_200)]
    pub hash_every: u64,

    /// Zapis ciągu hashy: „<tick> <hash>" po jednym w linii.
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,

    /// Porównanie z zapisanym ciągiem; różnica → kod wyjścia 1 i wskazanie ticku.
    #[arg(long)]
    pub expect: Option<std::path::PathBuf>,

    /// Ile danin musi mieć niezerowe wpływy, żeby bramka była zielona.
    ///
    /// Domyślnie 3, i ta liczba jest **pomiarem, nie ambicją**. W przebiegu
    /// krótszym niż rok nie ma CIT-u ani koncesji, bo obie rozliczają się rocznie.
    /// Akcyza nie ma czego obłożyć: obłożone są paliwa, piwo i papierosy, a z tych
    /// trzech tylko piwo w ogóle stoi na półce — i przegrywa z sokiem, bo w kategorii
    /// `Drink` sok jest pierwszy w rangach substytutu (`data/economy/retail.ron`).
    /// Paliwo kupują pojazdy przez `FuelLedger` (M4), który nie ma jeszcze konta
    /// w księgach. To jest `R2` nazwany wprost, a nie przemilczany.
    #[arg(long, default_value_t = 3)]
    pub expect_taxes: usize,
}

pub fn run(a: &M8MiastoArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pool = JobPool::new(a.threads);
    let start = std::time::Instant::now();
    let city = zbuduj_miasto(a.seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
    eprintln!("miasto {:.1} s", start.elapsed().as_secs_f64());

    let mut world = swiat_agentow(a.seed)?;
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, a.citizens, 200_000)?;
    eprintln!(
        "{} mieszkańców, {} gospodarstw",
        society::population(&world),
        society::households(&world)
    );

    let f = full::setup(
        &mut world,
        &city,
        zaludnione.places.clone(),
        zaludnione.travel_oracle(),
        &zaludnione.traffic,
        a.seed,
        &pool,
    )?;
    eprintln!(
        "gospodarka: {} sklepów, {} zakładów produkcyjnych; firmy: {} z {} zakładami",
        f.retail.shops, f.retail.plants.sites, f.firms.firms, f.firms.sites
    );

    // Strona publiczna. Katalog towarów bierze się z łańcucha stojącego już
    // w świecie — patrz komentarz w `city_bridge::setup`.
    let publiczne = city_bridge::setup(&mut world, &city)?;
    eprintln!(
        "miasto jako aktor: {} zakładów w katastrze o wartości {} zł, {} rodzajów zakładu pod koncesją, {} towarów z VAT-em, {} z akcyzą",
        publiczne.cadastre,
        publiczne.cadastral_value.get() / 100,
        publiczne.licensed_types,
        publiczne.vat_goods,
        publiczne.excise_goods
    );

    bootstrap_day(&mut world, 0);
    let mut builder = ScheduleBuilder::new();
    builder
        .add(magnat_supply::ChainSystem::new())
        .add(magnat_firms::systems::FirmSystem::new())
        .add(MarketSystem::new(&world))
        .add(LaborSystem::new())
        .add(InsolvencySystem::new())
        .add(MacroSystem::new())
        .add(CitySystem::new())
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world));
    let schedule = builder.build()?;
    eprintln!(
        "harmonogram: {} systemów w {} etapach, odcisk {:#018x}",
        schedule.system_count(),
        schedule.stage_count(),
        schedule.fingerprint()
    );

    let mut app = App::new(world, schedule, a.threads);
    let pieniadz_start = crate::m7_miasto::pieniadz(&app.world);

    let ticki = u64::from(a.days) * 1440;
    let bieg = std::time::Instant::now();
    let mut hashe: Vec<(u64, String)> = Vec::new();
    for t in 1..=ticki {
        app.tick();
        if a.hash_every > 0 && t.is_multiple_of(a.hash_every) {
            hashe.push((t, world_state_hash(&app.world).to_string()));
        }
    }
    let czas = bieg.elapsed().as_secs_f64();

    let mut kod = ExitCode::SUCCESS;
    let miasto = app.world.resource::<City>();
    if !raport(a, miasto, czas, pieniadz_start, &app.world) {
        kod = ExitCode::FAILURE;
    }

    if let Some(p) = &a.out {
        std::fs::write(
            p,
            hashe
                .iter()
                .map(|(t, h)| format!("{t} {h}\n"))
                .collect::<String>(),
        )?;
    }
    if let Some(p) = &a.expect {
        let oczekiwane = std::fs::read_to_string(p)?;
        for (i, (linia, (t, h))) in oczekiwane.lines().zip(hashe.iter()).enumerate() {
            let mut cz = linia.split_whitespace();
            let (tt, hh) = (cz.next().unwrap_or(""), cz.next().unwrap_or(""));
            if tt != t.to_string() || hh != h {
                eprintln!("ROZJAZD w linii {i}: oczekiwano `{linia}`, jest `{t} {h}`");
                kod = ExitCode::FAILURE;
                break;
            }
        }
    }
    Ok(kod)
}

/// Zwraca `false`, gdy którakolwiek bramka scenariusza się nie zamknęła.
fn raport(
    a: &M8MiastoArgs,
    miasto: &City,
    czas: f64,
    pieniadz_start: [i64; 5],
    world: &magnat_ecs::World,
) -> bool {
    let mut ok = true;

    println!("── przebieg ──────────────────────────────────────────");
    println!(
        "{} dób gry w {:.1} s ({:.2} s/dobę)",
        a.days,
        czas,
        czas / f64::from(a.days.max(1))
    );

    println!("── wpływy budżetu ────────────────────────────────────");
    let mut zywe = 0;
    for k in TaxKind::ALL {
        let wplyw = miasto.budget.revenue_life[k.as_index()];
        let naliczone = naliczone_razem(miasto, *k);
        if wplyw.get() != 0 {
            zywe += 1;
        }
        println!(
            "{:<10} naliczone {:>16} zł, zapłacone {:>16} zł",
            k.name(),
            zlote(naliczone),
            zlote(wplyw)
        );
    }
    println!(
        "razem wpływy {} zł (w tym roku {} zł), wydatki w tym roku {} zł, dług {} zł, należności w rejestrze {}",
        miasto.budget.revenue_life_total().get() / 100,
        miasto.budget.revenue_total().get() / 100,
        miasto.budget.spend_total().get() / 100,
        miasto.budget.debt_outstanding().get() / 100,
        miasto.charges.len()
    );

    // CIT zerowy jest **odpowiedzią, a nie brakiem odpowiedzi** — ale tylko wtedy,
    // gdy widać, z czego wyszedł. Strata przeniesiona jest tą liczbą: zakład, który
    // zamknął rok na minusie, nie płaci podatku i zabiera stratę do rozliczenia.
    println!(
        "zakładów ze stratą do rozliczenia w CIT: {} (strata razem {} zł)",
        miasto.loss_carry.len(),
        miasto.loss_carry.values().map(|m| m.get()).sum::<i64>() / 100
    );

    println!("── domknięcie podatkowe (T1) ─────────────────────────");
    match miasto.charges.check_closure() {
        Ok(()) => println!("każda para (danina, okres) domyka się do zera groszy"),
        Err((k, p, lewa, prawa)) => {
            println!(
                "ROZJAZD: {} {p:?} — naliczone {lewa:?}, rozliczone {prawa:?}",
                k.name()
            );
            ok = false;
        }
    }
    let (zaplacone, w_budzecie) = suma_rozliczen(miasto);
    if zaplacone == w_budzecie {
        println!(
            "Σ zapłaconych należności == Σ wpływów budżetu ({} zł)",
            zaplacone.get() / 100
        );
    } else {
        println!(
            "ROZJAZD: zapłacone {:?} vs. wpływy budżetu {:?}",
            zaplacone, w_budzecie
        );
        ok = false;
    }

    println!("── pieniądz ──────────────────────────────────────────");
    let teraz = crate::m7_miasto::pieniadz(world);
    let (a0, a1) = (
        crate::m7_miasto::suma(pieniadz_start),
        crate::m7_miasto::suma(teraz),
    );
    println!(
        "start {} zł, koniec {} zł, różnica {} zł",
        a0 / 100,
        a1 / 100,
        (a1 - a0) / 100
    );
    if let Some(b) = world.get_resource::<magnat_economy::Books>() {
        match b.check_conservation() {
            Ok(()) => println!("`Books::check_conservation` zielone (konto miasta w środku)"),
            Err((sald, podaz)) => {
                println!("ROZJAZD ksiąg: salda {sald:?} vs. podaż {podaz:?}");
                ok = false;
            }
        }
    }

    if zywe < a.expect_taxes {
        println!(
            "BRAMKA: tylko {zywe} z {TAX_KIND_COUNT} danin ma niezerowe wpływy, oczekiwano ≥ {}",
            a.expect_taxes
        );
        ok = false;
    }
    ok
}

/// Kwota w złotych z groszami. Z groszami, bo danina naliczona na trzydzieści
/// złotych wygląda w raporcie zaokrąglonym do złotówek dokładnie tak samo jak
/// danina, której nikt nie naliczył — a to są dwie zupełnie różne odpowiedzi.
fn zlote(m: Money) -> String {
    let z = m.get() / 100;
    let g = (m.get() % 100).abs();
    format!("{z},{g:02}")
}

/// Suma naliczeń jednej daniny po wszystkich okresach.
fn naliczone_razem(miasto: &City, kind: TaxKind) -> Money {
    let mut suma = Money::ZERO;
    for (k, _, t) in miasto.charges.periods() {
        if k == kind {
            suma = Money(suma.get() + t.assessed.get());
        }
    }
    suma
}

/// Lewa i prawa strona równania `Σ Settled == Δ CityBudget.revenue`.
///
/// Prawą stroną jest licznik **od początku świata**, a nie roczny: `revenue_ytd`
/// zeruje się 1 stycznia, więc porównanie z nim miałoby sens wyłącznie w przebiegu
/// krótszym niż rok — i pierwszy przebieg na 380 dobach pokazał to rozjazdem
/// rzędu dziesięciokrotnego, który był własnością raportu, nie budżetu.
fn suma_rozliczen(miasto: &City) -> (Money, Money) {
    let mut zaplacone = Money::ZERO;
    for k in TaxKind::ALL {
        zaplacone = Money(zaplacone.get() + miasto.charges.settled_of(*k).get());
    }
    (zaplacone, miasto.budget.revenue_life_total())
}
