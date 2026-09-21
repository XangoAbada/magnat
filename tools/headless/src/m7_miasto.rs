//! Scenariusz `m7-miasto` — **artefakt fazy M7** (§1 dokumentu fazy).
//!
//! Pierwszy przebieg, w którym **wszystko biegnie razem**: doba mieszkańca, ruch,
//! produkcja, detal, rynek pracy, firmy AI, ich finanse, cykl życia i model makro.
//! Do M7f takiego przebiegu nie było: `m5shop` stawiał rynek bez rejestru firm,
//! `m7labor` rejestr bez rynku — więc polityki (M7c) i AI firm (M7e) wykonywały się
//! wyłącznie w testach.
//!
//! Runner mierzy i wypisuje pięć rzeczy, na których stoi §1:
//! 1. **firmy** — ile ich jest, ile powstało, ile zniknęło, ile sieci weszło;
//! 2. **rynek pracy** — bezrobocie, rotacja, podwyżki, wakaty;
//! 3. **decyzje AI** — ile firm decydowało na każdym poziomie i co z tego wyszło;
//! 4. **pieniądz** — suma świata przed i po, z rozbiciem na kanały emisji;
//! 5. **wyjaśnialność** — licznik wykonanych akcji wobec licznika zapisanych powodów.
//!
//! Determinizm sprawdza się tak samo jak w `m5shop`: ciąg hashy co `hash-every`
//! ticków, `--out` zapisuje, `--expect` porównuje.

use std::process::ExitCode;

use clap::Args;
use magnat_agents::{
    bootstrap_day, register_day, society, DayLoopSystem, DayStats, DeprivationEffectsSystem,
    HouseholdStockSystem, NeedDecaySystem, NoInheritance, Population, ReplanCooldownSystem,
    SkillDriftSystem, SocietySystem,
};
use magnat_economy::corpfin::system::InsolvencySystem;
use magnat_economy::firmlife::FirmLifeLog;
use magnat_economy::labor::{LaborHandle, LaborSystem};
use magnat_economy::{Books, MarketSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_firms::{Firms, StrategicOutlooks};
use magnat_headless::full;
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use magnat_macro::MacroSystem;
use magnat_traffic::TrafficSystem;

#[derive(Args, Debug)]
pub struct M7MiastoArgs {
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

    /// Ile dób gry przebiec. Artefakt fazy mówi o pięciu latach (1800 dób);
    /// wartością domyślną jest trzydzieści, bo scenariusz uruchamiany odruchowo ma
    /// zdążyć coś pokazać, a pięć lat pełnego miasta liczy się w minutach.
    #[arg(long, default_value_t = 30)]
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

    /// Pasmo bezrobocia w promilach, `min:max`. Poza pasmem → kod wyjścia 1.
    /// Domyślne 30:120 to pasmo z §7.10 („bezrobocie 3–12 %").
    #[arg(long, default_value = "30:120")]
    pub expect_unemployment: String,
}

pub fn run(a: &M7MiastoArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
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
    let market = f.retail.market.clone();
    eprintln!(
        "gospodarka: {} sklepów, {} zakładów produkcyjnych; firmy: {} z {} zakładami, {} etatów,          {} sieci w katalogu, {} zakładów ubezpieczeń",
        f.retail.shops,
        f.retail.plants.sites,
        f.firms.firms,
        f.firms.sites,
        f.slots,
        f.chains,
        f.insurers
    );
    if f.firms.firms == 0 || f.retail.shops == 0 {
        eprintln!("BRAK FIRM ALBO SKLEPÓW — scenariusz nie ma czego pokazać");
        return Ok(ExitCode::FAILURE);
    }

    bootstrap_day(&mut world, 0);
    let mut builder = ScheduleBuilder::new();
    builder
        .add(magnat_supply::ChainSystem::new())
        .add(magnat_firms::systems::FirmSystem::new())
        .add(MarketSystem::new(&world))
        .add(LaborSystem::new())
        .add(InsolvencySystem::new())
        .add(MacroSystem::new())
        .add(magnat_media::MediaSystem::new())
        .add(magnat_economy::insurance::system::InsuranceSystem::new())
        .add(magnat_economy::equity::system::EquitySystem::new())
        .add(magnat_economy::RelationsSystem::new())
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
    let firm_start = app.world.resource::<Firms>().len();
    let pieniadz_start = pieniadz(&app.world);

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

    raport(a, &app.world, &market, czas, firm_start, pieniadz_start);
    let bezrobocie = app
        .world
        .get_resource::<LaborHandle>()
        .and_then(LaborHandle::get)
        .map_or(0, |m| m.last_day().unemployment_permille());

    let mut kod = ExitCode::SUCCESS;
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
    if let Some((min, max)) = pasmo(&a.expect_unemployment) {
        if bezrobocie < min || bezrobocie > max {
            eprintln!(
                "BRAMKA: bezrobocie {},{:01} % poza pasmem {},{:01}–{},{:01} %",
                bezrobocie / 10,
                bezrobocie % 10,
                min / 10,
                min % 10,
                max / 10,
                max % 10
            );
            kod = ExitCode::FAILURE;
        }
    }
    Ok(kod)
}

/// Pasmo z argumentu `min:max` w promilach.
pub(crate) fn pasmo(s: &str) -> Option<(u16, u16)> {
    let (a, b) = s.split_once(':')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// Suma pieniądza w świecie, **w rozbiciu na składniki**. Ta sama definicja co
/// w `m5shop` i z tego samego powodu: dwie definicje sumy pieniądza znaczyłyby dwa
/// różne zdania o tym, czy się zgadza.
///
/// Rozbicie, a nie jedna liczba, bo jedna liczba nie mówi, **gdzie** się rozjechało.
/// Pierwszy przebieg WP17 pokazał to dosłownie: różnica wyglądała na ubytek warstwy
/// firm, a siedziała w rejestrach ruchu (M4), które nie mają jeszcze kont i przez to
/// same z siebie „tworzą" pieniądz — dokładnie tak samo, jak w `m5shop` na tym samym
/// horyzoncie. Rozbicie odpowiada na to pytanie w jednym spojrzeniu.
pub(crate) fn pieniadz(world: &magnat_ecs::World) -> [i64; 5] {
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
        .get_resource::<magnat_traffic::FuelLedger>()
        .map_or(0, |l| l.revenue.get());
    let przewoz = world
        .get_resource::<magnat_traffic::FareLedger>()
        .map_or(0, |l| {
            l.transit_revenue.get() + l.parking_revenue.get() - l.transit_fuel_cost.get()
                + l.taxi_revenue.get()
        });
    [ksiegi, ludzie, poza, paliwo, przewoz]
}

/// Suma składników — to ona ma być stała po odjęciu emisji.
pub(crate) fn suma(p: [i64; 5]) -> i64 {
    p.iter().sum()
}

#[allow(clippy::too_many_lines)]
fn raport(
    a: &M7MiastoArgs,
    world: &magnat_ecs::World,
    market: &magnat_economy::Market,
    czas: f64,
    firm_start: usize,
    pieniadz_start: [i64; 5],
) {
    let firms = world.resource::<Firms>();
    let zycie = world
        .get_resource::<FirmLifeLog>()
        .copied()
        .unwrap_or_default();
    let dzien = world.resource::<DayStats>();

    println!("── przebieg ──────────────────────────────────────────");
    println!(
        "{} dób gry w {:.1} s ({:.2} s/dobę), {} zdarzeń doby, {} wizyt, {} odmów",
        a.days,
        czas,
        czas / f64::from(a.days.max(1)),
        dzien.events,
        dzien.fulfilled,
        dzien.refused
    );

    println!("── firmy ─────────────────────────────────────────────");
    println!(
        "start {firm_start}, koniec {} ({:+}), zakładów {}",
        firms.len(),
        firms.len() as i64 - firm_start as i64,
        firms.site_count()
    );
    let upadlosci = world
        .get_resource::<magnat_economy::corpfin::CorpFinance>()
        .map_or(0, magnat_economy::corpfin::CorpFinance::case_count);
    println!(
        "powstało {}, zwinięto dobrowolnie {}, postępowań upadłościowych {upadlosci}, zakładów otwartych {}, wejść sieci {}",
        zycie.total.founded, zycie.total.wound_down, zycie.total.opened, zycie.total.chain_entries
    );
    println!(
        "kapitał założycieli {} zł, kapitał zewnętrzny {} zł (punkt emisji, D10)",
        zycie.total.capital_own.get() / 100,
        zycie.total.capital_in.get() / 100
    );

    println!("── rynek pracy ───────────────────────────────────────");
    if let Some(m) = world
        .get_resource::<LaborHandle>()
        .and_then(LaborHandle::get)
    {
        let d = m.last_day();
        println!(
            "zatrudnionych {}, siła robocza {}, bezrobocie {},{:01} %",
            d.employed,
            d.labour_force,
            d.unemployment_permille() / 10,
            d.unemployment_permille() % 10
        );
        println!(
            "ostatnia doba: {} zatrudnień, {} odejść, {} zwolnień, {} podwyżek, {} ofert zamrożonych na suficie, {} wakatów",
            d.hires, d.quits, d.dismissals, d.raises, d.frozen, d.vacancies
        );
    }

    println!("── decyzje AI firm ───────────────────────────────────");
    println!(
        "firmo-decyzji: operacyjnych {}, taktycznych {}, kwartalnych {}",
        zycie.ai.ops_firms, zycie.ai.tactical_firms, zycie.ai.strategic_firms
    );
    println!(
        "akcje: {} celów marży, {} celów zapasu, {} zmian kursu, {} presetów, {} kampanii, {} zamknięć zakładu",
        zycie.ai.margin_moves,
        zycie.ai.restock_moves,
        zycie.ai.strategy_changes,
        zycie.ai.policies_adopted,
        zycie.ai.campaigns_started,
        zycie.ai.sites_closed
    );
    let widoki = world
        .get_resource::<StrategicOutlooks>()
        .map_or(0, StrategicOutlooks::len);
    println!("uporządkowań wariantów w tablicy: {widoki} (model makro, horyzont kwartału)");

    println!("── pieniądz ──────────────────────────────────────────");
    let books = world.resource::<Books>();
    let teraz = pieniadz(world);
    let emisja = books.supply();
    println!(
        "P1 (księgi): {}",
        match books.check_conservation() {
            Ok(()) => "OK".to_string(),
            Err((suma, podaz)) => format!("ROZJAZD suma={suma:?} podaż={podaz:?}"),
        }
    );
    println!(
        "kanał sektora gospodarstw: +{} zł / −{} zł",
        emisja.household_sector_in.get() / 100,
        emisja.household_sector_out.get() / 100
    );
    let kredyt = emisja.credit_created.get() - emisja.credit_repaid.get();
    let zewnetrzny = emisja.external_capital_in.get() - emisja.external_capital_out.get();
    println!(
        "suma świata: start {} zł, koniec {} zł, różnica {} gr\n\
         \u{20} z tego kredyt netto {} gr, kapitał zewnętrzny {} gr, poza tym {} gr",
        suma(pieniadz_start) / 100,
        suma(teraz) / 100,
        suma(teraz) - suma(pieniadz_start),
        kredyt,
        zewnetrzny,
        suma(teraz) - suma(pieniadz_start) - kredyt - zewnetrzny
    );
    for (nazwa, i) in [
        ("księgi", 0),
        ("ludzie", 1),
        ("spadki + emigracja", 2),
        ("obrót stacji (M4, bez konta)", 3),
        ("przewoźnicy i taryfy (M4, bez konta)", 4),
    ] {
        println!(
            "  {nazwa}: {} zł → {} zł ({:+} gr)",
            pieniadz_start[i] / 100,
            teraz[i] / 100,
            teraz[i] - pieniadz_start[i]
        );
    }

    println!("── wyjaśnialność (§7.8) ──────────────────────────────");
    let akcje = zycie.ai.actions();
    let powody = firms.reasons_logged();
    println!(
        "akcje AI {akcje}, powody zapisane {powody} — {}",
        if powody >= akcje {
            "OK (każda akcja ma powód)"
        } else {
            "ROZJAZD: akcja bez powodu"
        }
    );

    println!("── marka i media (M10b) ──────────────────────────────");
    let kampanie = world.get_resource::<magnat_media::Campaigns>();
    let tytuly = world.get_resource::<magnat_media::Outlets>();
    match (kampanie, tytuly) {
        (Some(k), Some(o)) => {
            let ekspozycje: u64 = k.iter().map(|(_, c)| c.metrics.exposures_total).sum();
            let pierwsze: u64 = k.iter().map(|(_, c)| c.metrics.first_contacts).sum();
            let wydane: i64 = k.iter().map(|(_, c)| c.spent.get()).sum();
            println!(
                "kampanie {} (ekspozycje {ekspozycje}, pierwsze kontakty {pierwsze},                  wydane {} zł), tytuły {}, teksty w obiegu {}",
                k.len(),
                wydane / 100,
                o.len(),
                o.stories().len()
            );
            // Histogram kanałów. Bez niego „0 ekspozycji przy sześciu kampaniach"
            // jest zagadką: kanały docierają różnymi drogami i każda może zawieść
            // osobno (ulotka potrzebuje współrzędnej zakładu z `PlaceCatalog`,
            // prasa tytułu, billboard przejazdów w `EdgeWatch`).
            //
            // **Liczby są dożywotnie, a nie z żywych kampanii** — i to jest poprawka
            // przyrządu, nie kosmetyka (`GG-8`). Kampania żyje trzydzieści dób
            // i ginie razem ze swoim pomiarem, a `ai::monthly` otwiera nowe **za**
            // dobowym rozdaniem ekspozycji. Przebieg kończący się na wielokrotności
            // trzydziestu widzi więc wyłącznie kampanie jednotickowe i pokazuje
            // zero na każdym kanale. Tak powstał `GF-2` — z `--days 300`.
            {
                let zywe = k.len();
                let doz = k.lifetime_by_channel();
                let opis: Vec<String> = magnat_core::AdChannelKind::ALL
                    .iter()
                    .zip(doz.iter())
                    .filter(|(_, n)| **n > 0)
                    .map(|(kanal, n)| format!("{} {n}", kanal.name()))
                    .collect();
                println!(
                    "ekspozycje od początku gry per kanał: {}   (żywych kampanii {zywe}{})",
                    if opis.is_empty() {
                        "brak".to_string()
                    } else {
                        opis.join(", ")
                    },
                    if a.days.is_multiple_of(30) {
                        ", próbka na granicy miesiąca — one dopiero powstały"
                    } else {
                        ""
                    }
                );
            }
            // Ilu mieszkańców ma w pamięci **jakąkolwiek** markę — to jest liczba,
            // która odróżnia świat z marką od świata, w którym marka jest strukturą.
            let spis = world.resource::<magnat_agents::Population>().citizens();
            // Zanik liczy się od doby odczytu; raport pyta o „dziś", czyli o koniec
            // przebiegu — a ten jest w `czas`, nie w świecie.
            let doba = u64::from(a.days);
            let znajacy = spis
                .iter()
                .filter(|e| !magnat_agents::slots_of(world, **e, doba).is_empty())
                .count();
            println!(
                "mieszkańcy ze slotem marki: {znajacy} z {} ({} ‰)",
                spis.len(),
                if spis.is_empty() {
                    0
                } else {
                    znajacy * 1000 / spis.len()
                }
            );
            // Mediana liczby marek u tych, którzy znają **jakąkolwiek** — to jest
            // pomiar rozstrzygający decyzję `D4` fazy: powyżej 13 slotów
            // `BRAND_SLOTS = 16` zaczyna wypierać lojalność i idzie na 24.
            // Liczona wśród znających, nie wśród wszystkich: mediana po całym
            // mieście mówiłaby o zasięgu reklamy, a nie o ciasnocie pamięci.
            let mut ile_marek: Vec<usize> = spis
                .iter()
                .map(|e| magnat_agents::slots_of(world, *e, doba).len())
                .filter(|n| *n > 0)
                .collect();
            ile_marek.sort_unstable();
            println!(
                "marek na znającego: mediana {}, maksimum {} (sufit {}; `D4` podnosi przy > 13)",
                ile_marek.get(ile_marek.len() / 2).copied().unwrap_or(0),
                ile_marek.last().copied().unwrap_or(0),
                magnat_agents::BRAND_SLOTS
            );
            // `FF-25`: ile marek obrywa od prasy. Liczba rozstrzyga, czy
            // `SCANDAL_MAX_DROP` ma zejść z kodu do `data/tuning/brand.ron`.
            let lat = f64::from(a.days).max(1.0) / 360.0;
            println!(
                "teksty uderzające w markę: {} przez {:.2} roku gry ({:.1} na rok)",
                o.scandal_stories(),
                lat,
                f64::from(o.scandal_stories()) / lat
            );
            // Najszerzej znana marka w mieście — agregat liczony z pamięci
            // mieszkańców, a nie z pola przy firmie (`brand_strength`).
            let najlepsza = firms
                .iter()
                .filter_map(|(key, _)| magnat_supply::brand_of(magnat_firms::firm_id(key)))
                .map(|b| (magnat_agents::brand_strength(world, b, doba), b))
                .max_by_key(|(s, b)| (s.known, std::cmp::Reverse(b.0)));
            if let Some((sila, marka)) = najlepsza {
                println!(
                    "najszerzej znana marka: #{} — zna ją {} osób, średnia sympatia {},                      spodziewana jakość {}",
                    marka.0,
                    sila.known,
                    sila.mean_affinity,
                    sila.mean_expected.get()
                );
            }
        }
        _ => println!("brak rejestru kampanii albo tytułów — media nie stoją w tym świecie"),
    }

    println!("── badania i rozwój (M10c) ───────────────────────────");
    match world.get_resource::<magnat_firms::RndData>() {
        Some(dane) if !dane.tree.is_empty() => {
            let st = firms.rnd();
            let w_toku: Vec<_> = st.projects.iter().collect();
            let badaczy: u32 = magnat_firms::RoleTable::load_default()
                .ok()
                .and_then(|r| r.id(magnat_economy::rnd::RESEARCHER_ROLE))
                .map(|rola| firms.sites().map(|(_, s)| s.researchers(rola)).sum())
                .unwrap_or(0);
            println!(
                "drzewo: {} węzłów w {} gałęziach; badaczy na etatach {badaczy}",
                dane.tree.len(),
                dane.tree.branches.len()
            );
            // Średni opłacony budżet materiałowy to liczba, która tłumaczy tempo:
            // projekt firmy bez gotówki nie stoi, tylko pełznie.
            let oplacony: u32 = if w_toku.is_empty() {
                0
            } else {
                w_toku
                    .iter()
                    .map(|(_, p)| u32::from(p.budget_permille))
                    .sum::<u32>()
                    / w_toku.len() as u32
            };
            println!(
                "projekty w toku {}, firmy z wiedzą {}, patenty {}, licencje {}",
                w_toku.len(),
                st.known.len(),
                st.patents.len(),
                st.licenses.len()
            );
            println!("średni opłacony budżet badań: {oplacony} ‰");
            if let Some((key, p)) = w_toku
                .iter()
                .max_by_key(|(_, p)| p.done_mrp.saturating_mul(1000) / p.cost_mrp.max(1))
            {
                println!(
                    "najdalej zaawansowany: firma {} bada `{}` (świat zna od {}) — \
                     {} % kosztu zebrane, projekt od doby {}, przełomów {}",
                    key.0,
                    dane.tree.node(p.tech).key,
                    dane.tree.node(p.tech).world_year,
                    p.done_mrp.saturating_mul(100) / p.cost_mrp.max(1),
                    p.started.0 / 1_440,
                    p.breakthroughs
                );
            }
            // Licencje z datą podpisania i stawką — to jest jedyne miejsce, w którym
            // widać, że patent komuś płaci. Karta umowy należy do M10f.
            for l in st.licenses.values() {
                println!(
                    "licencja: firma {} płaci firmie {} {} bp od utargu za `{}` (od doby {})",
                    l.licensee.0,
                    l.licensor.0,
                    l.royalty_bp,
                    dane.tree.node(l.tech).key,
                    l.signed.0 / 1_440
                );
            }
            // Towary poza obiegiem: ile technologia jeszcze trzyma za drzwiami.
            let poza: Vec<String> = magnat_firms::gated_goods(&dane.tree)
                .into_iter()
                .filter(|g| market.is_good_locked(*g))
                .map(|g| format!("#{}", g.0))
                .collect();
            println!(
                "towary poza obiegiem: {}",
                if poza.is_empty() {
                    "— (wszystko wypuszczone)".to_owned()
                } else {
                    poza.join(", ")
                }
            );
        }
        _ => println!("brak drzewa technologii — badania nie stoją w tym świecie"),
    }

    println!("── giełda i ubezpieczenia (M10d) ─────────────────────");
    match (
        world.get_resource::<magnat_economy::equity::Equity>(),
        world.get_resource::<magnat_economy::insurance::Insurers>(),
    ) {
        (Some(eq), Some(ins)) => {
            // `FF-17`: ile oddziałów ubezpieczeniowych stawia generator. Rejestr
            // bez ani jednego znaczy miasto, w którym nikt nie wystawia polis —
            // a mechanizm wygląda wtedy tak samo jak działający.
            let biur = world
                .get_resource::<magnat_firms::SiteTypeCatalog>()
                .map(|kat| {
                    firms
                        .sites()
                        .filter(|(_, s)| {
                            kat.get(s.site_type).key
                                == magnat_economy::insurance::system::INSURER_SITE_TYPE
                        })
                        .count()
                })
                .unwrap_or(0);
            println!(
                "notowanych firm {}, polis czynnych {}, biur ubezpieczeniowych {biur}",
                eq.listed_count(),
                ins.cover_count()
            );
            // `FF-18`: ile gospodarstw przekracza próg majątku inwestora. Zero
            // znaczy giełdę bez kupujących, niezależnie od tego, ile firm debiutuje.
            // Majątek liczy się **tak samo** jak w `equity::system`: oszczędności
            // plus rachunek, bo inaczej pomiar mierzyłby inny próg niż mechanizm.
            let prog = eq.params().investor_wealth_min;
            let inwestorow = world
                .resource::<magnat_agents::Population>()
                .households()
                .iter()
                .filter_map(|h| world.get::<magnat_agents::Household>(*h))
                .filter(|gd| gd.savings.get().max(0) + gd.bank.get().max(0) >= prog.get())
                .count();
            println!(
                "gospodarstw nad progiem inwestora ({} zł): {inwestorow}",
                prog.get() / 100
            );
            for l in eq.listings().take(5) {
                println!(
                    "firma {} od doby {}: kurs {} gr za 0,01 % (wycena {} zł), wolumen ostatniej sesji {} bp",
                    l.firm.0,
                    l.since.0 / 1_440,
                    l.last_fixing.get(),
                    l.last_fixing.get().saturating_mul(10_000) / 100,
                    l.last_volume_bp
                );
            }
            for d in eq.disclosures().iter().rev().take(5) {
                println!(
                    "ujawnienie w dobie {}: firma {} — pakiet {} bp{}",
                    d.day,
                    d.firm.0,
                    d.bp,
                    if d.control { " (kontrola)" } else { "" }
                );
            }
            // Szkodowość per ryzyko: to z niej bierze się składka i to ona jest
            // jedynym śladem po tym, że zdarzenie cokolwiek zniszczyło (`GD-4`).
            for p in magnat_core::PerilKind::ALL {
                let m = ins.city_stats(*p);
                println!(
                    "ryzyko {}: polisomiesięcy {}, szkód {}, strata {} zł, suma ubezpieczenia {} zł",
                    p.name(),
                    m.exposures,
                    m.claims,
                    m.loss_total.get() / 100,
                    m.sum_total.get() / 100
                );
            }
        }
        _ => println!("brak giełdy albo ubezpieczeń — rynek kapitałowy nie stoi w tym świecie"),
    }

    println!("── relacje, zmowy i związki (M10e) ───────────────────");
    match (
        world.get_resource::<magnat_economy::Unions>(),
        world.get_resource::<magnat_economy::Cartels>(),
    ) {
        (Some(u), Some(c)) => {
            let strajkuje = u.iter().filter(|z| z.state.is_striking()).count();
            println!(
                "związków {}, w tym strajkuje {}; zmów czynnych {}",
                u.len(),
                strajkuje,
                c.len()
            );
            // `FF-24`: ile załóg **jest rozżalonych**, a nie ile już się zrzeszyło.
            // Związek powstaje dopiero po dwóch pomiarach nad progiem i przy dość
            // gęstej składowej grafu relacji, więc miasto ze stoma rozżalonymi
            // załogami i zerem związków wygląda w raporcie tak samo jak zadowolone.
            let prog = magnat_economy::relations::RelationsTuning::load_default()
                .map(|d| d.union.grievance_threshold)
                .unwrap_or(55);
            let (zmierzonych, nad_progiem) =
                u.watched()
                    .fold((0usize, 0usize), |(n, k), (_, g)| {
                        (n + 1, k + usize::from(g.level >= prog))
                    });
            println!(
                "zakładów z pomiarem żalu {zmierzonych}, nad progiem {prog}: {nad_progiem}"
            );
            // **Zero związków nie znaczy zero żalu** i to jest cała treść tego
            // akapitu. Związek powstaje dopiero wtedy, gdy trzy warunki zejdą się
            // naraz (§5.9), więc miasto z setką rozżalonych załóg i miasto
            // zadowolone wyglądają w liczbie związków identycznie. Raport pokazuje
            // **którego warunku brakuje** — inaczej kalibracja byłaby zgadywaniem.
            let p = magnat_economy::RelationsTuning::default().union;
            let (mut zal, mut siec, mut gestosc, mut ile) = (0u32, 0u32, 0u32, 0u32);
            let mut najwyzszy = 0u8;
            for (site, g) in u.watched() {
                ile += 1;
                najwyzszy = najwyzszy.max(g.level);
                if g.level >= p.grievance_threshold {
                    zal += 1;
                }
                if u32::from(g.component) >= u32::from(p.min_component) {
                    siec += 1;
                }
                if g.density_bp >= p.density_bp {
                    gestosc += 1;
                }
                let _ = site;
            }
            println!(
                "zakładów pod obserwacją {ile}: żal ≥ {} w {zal}, sieć relacji ≥ {} w {siec},                  gęstość ≥ {} bp w {gestosc}; najwyższy zmierzony żal {najwyzszy}",
                p.grievance_threshold, p.min_component, p.density_bp
            );
            for z in u.iter().take(5) {
                println!(
                    "zakład {}: gęstość {}, wojowniczość {}, stan {:?}, fundusz {} zł",
                    z.site.entity().index(),
                    z.density.get(),
                    z.militancy.get(),
                    z.state,
                    z.strike_fund.get() / 100
                );
            }
            for k in c.iter().take(5) {
                println!(
                    "zmowa {} na towarze {} w dzielnicy {}: {} firm, cena {} gr, odchylenie {} %",
                    k.id.0,
                    k.good.0,
                    k.district.0,
                    k.members.len(),
                    k.floor_price.get(),
                    k.deviation_pct()
                );
            }
        }
        _ => println!("brak związków albo zmów — ten świat ich nie stawia"),
    }
    // Zaufanie do dostawców: ile par handluje ze sobą na tyle długo, żeby to
    // cokolwiek zmieniało w przetargu.
    {
        let ch = market.chain();
        let rel = ch.lock();
        let wszystkie = rel.b2b.relations().len();
        let z_preferencja = rel
            .b2b
            .relations()
            .iter()
            .filter(|r| r.discount_bp(rel.b2b.relations().tuning()) > 0)
            .count();
        println!("relacji z dostawcami {wszystkie}, w tym z preferencją {z_preferencja}");
    }

    println!("── rynek detaliczny ──────────────────────────────────");
    let s = market.stats();
    println!(
        "transakcje {}, obrót {} zł, {} przecen, {} odpisów",
        s.purchases,
        s.revenue.get() / 100,
        s.reprices,
        s.write_offs
    );
}
