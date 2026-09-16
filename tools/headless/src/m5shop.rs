//! Scenariusz `m5shop` — **wynik podfazy M5b**: „mieszkaniec wychodzi po chleb,
//! wybiera ofertę i wraca; pieniądz i sztuki zgadzają się po obu stronach".
//!
//! Różnica wobec `m3day` jest w jednej linii mostu: `AgentSources.places` dostaje
//! `Market` zamiast `InfinitePlaces` (`Z-1`). Wszystko inne — planer, pętla doby,
//! ruch, potrzeby — zostaje bez zmian, i to jest cały dowód, że punkt podmiany
//! był właściwy.
//!
//! Runner mierzy i wypisuje trzy rzeczy, które ta podfaza ma pokazać:
//! 1. **zakupy** — ile transakcji, za ile, ile odmów i z jakiego powodu,
//! 2. **pieniądz** — suma sald w księgach **plus** pieniądz gospodarstw musi być
//!    niezmienna co do grosza; to jest niezmiennik P1 rozszerzony o sektor
//!    gospodarstw domowych, którego saldo mieszka w komponencie M3,
//! 3. **determinizm** — hash stanu świata co `hash-every` ticków.

use std::process::ExitCode;

use clap::Args;
use magnat_agents::{
    bootstrap_day, register_day, society, DayLoopSystem, DayStats, DeprivationEffectsSystem,
    HouseholdStockSystem, NeedDecaySystem, NoInheritance, Population, ReplanCooldownSystem,
    SkillDriftSystem, SocietySystem,
};
use magnat_core::{DecisionReason, Money, RejectCause, StockCat, Tick};
use magnat_economy::{Books, LedgerAccount, MarketSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_headless::retail;
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use magnat_traffic::{FareLedger, FuelLedger, TrafficSystem};

#[derive(Args, Debug)]
pub struct M5ShopArgs {
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

    /// Ile dób gry przebiec.
    ///
    /// Sześć, nie dwie: gospodarstwo startuje z pełnym zapasem, a `purchase_days`
    /// z `choice.ron` wynosi cztery, więc przez pierwsze cztery doby **nikt nie
    /// wychodzi po zakupy** i bramka scenariusza „nikt nic nie kupił" zapalała się
    /// przy domyślnym wywołaniu. Domyślna wartość ma pokazywać to, co scenariusz
    /// obiecuje, a nie pustą pętlę.
    #[arg(long, default_value_t = 6)]
    pub days: u16,

    /// Docelowa liczba mieszkańców; 0 = z pojemności miasta.
    #[arg(long, default_value_t = 0)]
    pub citizens: u32,

    /// Co ile ticków liczyć hash stanu; 0 = nie liczyć.
    #[arg(long, default_value_t = 1440)]
    pub hash_every: u64,

    /// Zapis ciągu hashy: „<tick> <hash>" po jednym w linii.
    ///
    /// Razem z `--expect` jest to bramka determinizmu fazy M5 (§7.2): CI puszcza
    /// ten sam seed dwa razy z różną liczbą wątków i porównuje ciągi. **Złotego
    /// pliku w repozytorium nie ma i nie będzie** — wartość hasha zmienia się przy
    /// każdym dopisaniu stanu do świata, więc plik zatwierdzony w gicie byłby
    /// zobowiązaniem do nieruszania niczego, a nie bramką.
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,

    /// Porównanie z zapisanym ciągiem; różnica → kod wyjścia 1 i wskazanie ticku.
    #[arg(long)]
    pub expect: Option<std::path::PathBuf>,
}

/// Suma pieniądza w świecie: księgi, komponenty i rejestry, które jeszcze nie mają konta.
///
/// **Trzy składniki, nie jeden** — i każdy z nich został dopisany dopiero wtedy,
/// kiedy przebieg zrobił się dość długi, żeby go było widać:
///
/// 1. `Books::total_balance()` — pieniądz na kontach (niezmiennik P1).
/// 2. `society::total_money()` — gospodarstwa, mieszkańcy **oraz** `Population::escheat`
///    (majątek po zmarłym bez spadkobiercy) i `Population::emigrated` (majątek wywieziony
///    z miasta). Własna suma po `Household` pomija te dwie pozycje, a gospodarstwo znika
///    ze świata przy zgonie, przy scaleniu po ślubie i przy wyprowadzce — wszystkie trzy
///    na granicy miesiąca. Przebieg 8-dobowy pokazywał więc różnicę 0 gr i wyglądał
///    na zielony.
/// 3. Rejestry ruchu (M4): stacja paliw, przewoźnik, taksówka i parking **nie mają
///    jeszcze kont** — M5 daje je dopiero razem z obrotem stacji (`T-2`), a przewoźnika
///    M7. Do tego czasu drugą stroną każdego grosza wydanego przez mieszkańca na dojazd
///    jest `FuelLedger`/`FareLedger` i bez nich pieniądz „znika" tym szybciej, im więcej
///    się jeździ. `transit_fuel_cost` odejmuje się, bo przewoźnik płaci nim stacji:
///    ta sama kwota siedzi już w `FuelLedger.revenue`.
fn pieniadz_swiata(world: &magnat_ecs::World) -> [i64; 6] {
    let ksiegi = world
        .get_resource::<Books>()
        .map_or(0, |b| b.total_balance().get());
    let (ludzie, poza) = match world.get_resource::<Population>() {
        Some(p) => {
            let poza = p.escheat.get() + p.emigrated.get();
            (society::total_money(world) - poza, poza)
        }
        None => (0, 0),
    };
    let paliwo = world
        .get_resource::<FuelLedger>()
        .map_or(0, |l| l.revenue.get());
    let przewoz = world.get_resource::<FareLedger>().map_or(0, |l| {
        l.transit_revenue.get() + l.parking_revenue.get() - l.transit_fuel_cost.get()
    });
    let taxi = world
        .get_resource::<FareLedger>()
        .map_or(0, |l| l.taxi_revenue.get());
    [ksiegi, ludzie, poza, paliwo, przewoz, taxi]
}

/// Suma składników — to ona ma być stała.
fn suma(p: [i64; 6]) -> i64 {
    p.iter().sum()
}

pub fn run(a: &M5ShopArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pool = JobPool::new(a.threads);
    let start = std::time::Instant::now();
    let city = zbuduj_miasto(a.seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
    eprintln!("miasto {:.1} s", start.elapsed().as_secs_f64());

    let mut world = swiat_agentow(a.seed)?;
    register_day(&mut world);
    let start = std::time::Instant::now();
    let zaludnione = zaludnij(&mut world, &city, a.citizens, 200_000)?;
    eprintln!(
        "Etap 8 {:.1} s, {} mieszkańców, {} gospodarstw",
        start.elapsed().as_secs_f64(),
        society::population(&world),
        society::households(&world)
    );

    // ── gospodarka ───────────────────────────────────────────────────────────────
    // Most `retail::setup` stawia rynek i podmienia `Sources.places` (`Z-1`).
    // Ten sam kod stawia gospodarkę w kliencie graficznym i w balansatorze —
    // scenariusz nie ma **własnej** gospodarki, bo wtedy mierzyłby inną niż gra.
    let r = retail::setup(
        &mut world,
        &city,
        zaludnione.places.clone(),
        zaludnione.travel_oracle(),
        &zaludnione.traffic,
        a.seed,
        &pool,
    )?;
    let market = r.market.clone();
    eprintln!(
        "rynek: {} sklepów, {} ofert, zapas startowy za {} zł",
        r.shops,
        market.offer_count(),
        market
            .sites()
            .iter()
            .map(|s| market.inventory_value(*s).get())
            .sum::<i64>()
            / 100
    );
    eprintln!(
        "zakłady: {} produkcyjnych, {} linii, {} kopalń ze złożem ({} bez), zapas startowy za {} zł",
        r.plants.sites,
        r.plants.lines,
        r.plants.mines,
        r.plants.mines_without_deposit,
        r.plants.stock_value.get() / 100
    );
    if r.shops == 0 {
        eprintln!("BRAK SKLEPÓW — scenariusz nie ma czego pokazać");
        return Ok(ExitCode::FAILURE);
    }
    eprintln!(
        "wypłata startowa: {} gospodarstw, {} zł",
        r.incomes.0,
        r.incomes.1.get() / 100
    );
    eprintln!(
        "budżety startowe: {} gospodarstw, koszty stałe {} zł, {} wniosków kredytowych ({} przyznanych)",
        r.budgets.planned,
        r.budgets.fixed_paid.get() / 100,
        r.budgets.credit_applications,
        r.budgets.credit_granted
    );

    let zaplanowanych = bootstrap_day(&mut world, 0);
    eprintln!("kolejka zasiana: {zaplanowanych} mieszkańców");

    let mut builder = ScheduleBuilder::new();
    builder
        .add(magnat_supply::ChainSystem::new())
        .add(MarketSystem::new(&world))
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
    let pieniadz_start = pieniadz_swiata(&app.world);
    let kredyt_start = app.world.get_resource::<Books>().map_or(0, |b| {
        b.supply().credit_created.get() - b.supply().credit_repaid.get()
    });

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

    // ── raport ───────────────────────────────────────────────────────────────────
    let s = market.stats();
    let dzien = app.world.resource::<DayStats>();
    println!("── doba ──────────────────────────────────────────────");
    println!(
        "{} dób w {:.2} s, {} zdarzeń, {} wizyt zrealizowanych, {} odmów",
        a.days, czas, dzien.events, dzien.fulfilled, dzien.refused
    );
    println!("── zakupy ────────────────────────────────────────────");
    println!(
        "transakcje {}, obrót {} zł, sprzedano {} jednostek",
        s.purchases,
        s.revenue.get() / 100,
        s.purchased_qty / 1_000
    );
    println!(
        "odmowy: brak towaru {}, brak środków {}, poniżej progu {}, brak ofert w zasięgu {}",
        s.stockouts, s.budget_refusals, s.deferrals, s.no_candidates
    );
    println!(
        "zaopatrzenie: {} dostaw, {} uzupełnień półki",
        s.deliveries, s.restocks
    );

    let books = app.world.resource::<Books>();
    let pieniadz_koniec = pieniadz_swiata(&app.world);
    println!("── pieniądz ──────────────────────────────────────────");
    println!(
        "P1 (księgi): {}",
        match books.check_conservation() {
            Ok(()) => "OK".to_string(),
            Err((suma, podaz)) => format!("ROZJAZD suma={suma:?} podaż={podaz:?}"),
        }
    );
    println!(
        "kanał sektora gospodarstw: +{} zł / −{} zł",
        books.supply().household_sector_in.get() / 100,
        books.supply().household_sector_out.get() / 100
    );
    // Od M5d suma świata **ma prawo rosnąć**: kredyt tworzy pieniądz, a spłata go
    // niszczy (§5.10). Niezmiennikiem jest więc różnica **po odjęciu** kreacji netto,
    // a nie sama różnica — i to jest jedyna zmiana, jaką WP9 wnosi do tej sekcji.
    let kredyt_netto =
        books.supply().credit_created.get() - books.supply().credit_repaid.get() - kredyt_start;
    println!(
        "suma świata (księgi + ludzie + rejestry ruchu): start {} zł, koniec {} zł, różnica {} gr\n\
         \u{20} z tego pieniądz kredytowy netto: {} gr, poza kredytem: {} gr",
        suma(pieniadz_start) / 100,
        suma(pieniadz_koniec) / 100,
        suma(pieniadz_koniec) - suma(pieniadz_start),
        kredyt_netto,
        suma(pieniadz_koniec) - suma(pieniadz_start) - kredyt_netto
    );
    for (nazwa, i) in [
        ("księgi", 0),
        ("ludzie", 1),
        ("spadki + emigracja", 2),
        ("obrót stacji", 3),
        ("przewoźnicy i parkingi", 4),
        ("taryfy taksówkowe", 5),
    ] {
        println!(
            "  {nazwa}: {} zł → {} zł ({:+} gr)",
            pieniadz_start[i] / 100,
            pieniadz_koniec[i] / 100,
            pieniadz_koniec[i] - pieniadz_start[i]
        );
    }

    println!("── budżety, banki, inflacja (M5d) ────────────────────");
    println!(
        "kredytów {}, niespłacony kapitał {} zł, stopa bazowa {},{:02} %",
        market.loan_count(),
        market.credit_outstanding().get() / 100,
        market.base_rate().bp / 100,
        market.base_rate().bp % 100
    );
    println!(
        "CPI {},{:02} (baza 100,00){}{}",
        market.cpi_index_bp() / 100,
        market.cpi_index_bp() % 100,
        market.cpi_mom_bp().map_or(String::new(), |v| format!(
            ", m/m {:+},{:02} %",
            v / 100,
            (v % 100).abs()
        )),
        market.cpi_yoy_bp().map_or(String::new(), |v| format!(
            ", r/r {:+},{:02} %",
            v / 100,
            (v % 100).abs()
        ))
    );
    // Okno decyzji budżetowych: to jest odpowiedź na „czemu tej rodzinie nie starczyło".
    let log = market.budget_log();
    let odmowy = log
        .iter()
        .filter(|(_, r)| matches!(r, DecisionReason::CreditRejected { .. }))
        .count();
    let niedopłaty = log
        .iter()
        .filter(|(_, r)| matches!(r, DecisionReason::BudgetShortfall { .. }))
        .count();
    println!(
        "okno decyzji budżetowych: {} wpisów, w tym {odmowy} odmów kredytu i {niedopłaty} niedopłat",
        log.len()
    );

    println!("── ceny (M5c) ────────────────────────────────────────");
    println!(
        "{} przecen, {} odświeżeń obrazu konkurencji, {} odpisów za {} zł ({} jednostek)",
        s.reprices,
        s.observations,
        s.write_offs,
        s.write_off_value.get() / 100,
        s.expired_qty / 1_000
    );
    println!(
        "poślizg ceny między decyzją a wizytą: {} przypadków",
        s.slippage_rechecks
    );

    // ── pierwszy sklep z brzegu: co widać w panelu (M5e zrobi z tego ekran) ──────
    let mut bilans_ok = true;
    if let Some(site) = market.sites().first().copied() {
        market.set_tracking(site, magnat_economy::LostSaleTracking::Full);
        println!(
            "── sklep {} ─────────────────────────────────────────",
            site.entity().index()
        );
        println!(
            "wartość zapasu {} zł, konto {} zł, czujność na konkurencję {} dni",
            market.inventory_value(site).get() / 100,
            market
                .account_of(site)
                .and_then(|a| books.balance(a))
                .unwrap_or(Money::ZERO)
                .get()
                / 100,
            market.observe_delay(site).unwrap_or(0)
        );
        if let Some(h) = market.lost_histogram(site) {
            for c in RejectCause::ALL {
                let n = h.by_cause[c.as_index()];
                if n > 0 {
                    println!("  utracone ({}): {n}", c.name());
                }
            }
        }
        // Księgowość: RZiS i bilans **z księgi**, nie z liczników obok (M5c §5.8).
        let koniec = Tick(ticki);
        if let (Some(rzis), Some(bil)) = (
            market.income_statement(site, Tick(0), koniec),
            market.balance_sheet(site, koniec),
        ) {
            let zl = |m: Money| m.get() as f64 / 100.0;
            println!(
                "RZiS: przychód {:.2} zł, koszt własny {:.2} zł, marża brutto {:.2} zł, odpisy {:.2} zł, wynik {:.2} zł",
                zl(rzis.revenue),
                zl(rzis.cogs),
                zl(rzis.gross_margin()),
                zl(rzis.write_off),
                zl(rzis.net_result())
            );
            println!(
                "bilans: aktywa {:.2} zł (zapas {:.2}, rachunek {:.2}, majątek trwały {:.2}), pasywa {:.2} zł, kapitał {:.2} zł",
                zl(bil.assets),
                zl(bil.inventory),
                zl(bil.bank),
                zl(bil.fixed_net),
                zl(bil.liabilities),
                zl(bil.equity_total)
            );
            println!("bilans domyka się na {} gr", bil.imbalance().get());
            bilans_ok = bil.imbalance() == Money::ZERO;
            // P5: `InventoryGoods` == wycena zaplecza **i** półki.
            let zapas_ks = market
                .ledger_balance(site, LedgerAccount::InventoryGoods)
                .unwrap_or(Money::ZERO);
            let zapas = market.inventory_value(site);
            println!(
                "P5 (zapas w bilansie vs wycena): różnica {} gr",
                zapas_ks.get() - zapas.get()
            );
            bilans_ok &= zapas_ks == zapas;
            // `BankCurrent` ma nadążać za rachunkiem w `Books`.
            let rachunek = market
                .account_of(site)
                .and_then(|a| books.balance(a))
                .unwrap_or(Money::ZERO);
            let ks = market
                .ledger_balance(site, LedgerAccount::BankCurrent)
                .unwrap_or(Money::ZERO);
            println!(
                "konto w księdze vs rachunek: różnica {} gr",
                ks.get() - rachunek.get()
            );
            bilans_ok &= ks == rachunek;
        }
        for r in market.reprice_log(site).iter().take(5) {
            println!("  przecena: {r:?}");
        }
    }

    for (t, h) in &hashe {
        println!("hash {t:>7}: {h}");
    }

    let mut hashe_ok = true;
    if let Some(p) = &a.out {
        let tekst: String = hashe.iter().map(|(t, h)| format!("{t} {h}\n")).collect();
        std::fs::write(p, tekst)?;
        eprintln!("zapisano {} hashy do {}", hashe.len(), p.display());
    }
    if let Some(p) = &a.expect {
        let wzorzec = std::fs::read_to_string(p)?;
        let nasz: String = hashe.iter().map(|(t, h)| format!("{t} {h}\n")).collect();
        if wzorzec == nasz {
            eprintln!("ciąg {} hashy zgodny z wzorcem", hashe.len());
        } else {
            let pierwsza = wzorzec
                .lines()
                .zip(nasz.lines())
                .find(|(a, b)| a != b)
                .map_or("(inna długość ciągu)".to_string(), |(a, b)| {
                    format!("oczekiwano `{a}`, jest `{b}`")
                });
            eprintln!("BŁĄD: ciąg hashy się rozjechał — {pierwsza}");
            hashe_ok = false;
        }
    }

    // Bramką jest to, czego M5 jest właścicielem: niezmiennik P1 w księgach
    // i domknięcie księgowości zakładu — oba z tolerancją 0 groszy.
    //
    // **Suma całego świata bramką jeszcze nie jest** i jest to stan jawny, nie
    // przeoczenie: stacja paliw, przewoźnik, taksówka i parking nie mają kont
    // (`T-2` obiecuje stację M5, przewoźnika M7), więc druga strona każdego grosza
    // wydanego na dojazd siedzi w rejestrach M4, a nie na rachunku. Rozbicie wyżej
    // pokazuje, ile zostaje po zsumowaniu wszystkich znanych pozycji; przy 31 dobach
    // miasta 28 tys. mieszkańców to −5,6 tys. zł na 100 mln zł, czyli 0,006 %.
    // Domknięcie należy do M5d (decyzja otwarta nr 16 dokumentu fazy).
    let zgadza_sie = books.check_conservation().is_ok();
    let reszta = suma(pieniadz_koniec) - suma(pieniadz_start) - kredyt_netto;
    let sprzedano = s.purchases > 0;
    if !zgadza_sie {
        eprintln!("BŁĄD: niezmiennik P1 w księgach nie domyka się");
    }
    if reszta != 0 {
        eprintln!(
            "UWAGA: suma świata nie domyka się o {reszta} gr — rejestry ruchu M4 bez kont              (decyzja otwarta nr 16 dokumentu fazy, domyka M5d)"
        );
    }
    if !sprzedano {
        eprintln!("BŁĄD: przez {} dób nikt nic nie kupił", a.days);
    }
    if !bilans_ok {
        eprintln!("BŁĄD: księgowość sklepu nie domyka się co do grosza");
    }
    let _ = StockCat::Food;
    Ok(if zgadza_sie && sprzedano && bilans_ok && hashe_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
