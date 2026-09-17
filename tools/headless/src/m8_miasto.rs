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
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto_z_klimatem};
use magnat_headless::{city as city_bridge, events as events_bridge, full, grid as grid_bridge};
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
    /// **Sprostowanie po M8b (`CC-9`).** Do M8b ten komentarz tłumaczył zerową akcyzę
    /// brakiem obłożonego towaru w obrocie i było to tłumaczenie fałszywe: przyczyną
    /// było to, że `CityTaxEngine` **nie nadpisywał** `TaxEngine::excise_on`, więc
    /// ciało domyślne zwracało zero i nikt nigdy nie zapytał o stawkę. Po poprawce
    /// akcyza od towarów nalicza się normalnie, a M8b dokłada do niej akcyzę
    /// od energii na rachunku za media.
    ///
    /// Co zostaje prawdą o obrocie: paliwo kupują pojazdy przez `FuelLedger` (M4),
    /// który nie ma konta w księgach, a piwo przegrywa z sokiem w rangach substytutu
    /// kategorii `Drink` (`data/economy/retail.ron`).
    ///
    /// Trzy daniny z wpływami wymagają przebiegu **≥ 51 dób**: VAT, PIT i podatek
    /// od nieruchomości mają termin dwudziestego miesiąca następnego, a cło jest
    /// płatne tego samego dnia. Krótszy przebieg mierzy naliczenia, nie wpływy.
    #[arg(long, default_value_t = 3)]
    pub expect_taxes: usize,
}

pub fn run(a: &M8MiastoArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pool = JobPool::new(a.threads);
    let start = std::time::Instant::now();
    let (city, klimat) =
        zbuduj_miasto_z_klimatem(a.seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
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
        "miasto jako aktor: {} zakładów w katastrze o wartości {} zł, {} rodzajów zakładu pod koncesją, {} towarów z VAT-em, {} z akcyzą towarową, {} mediów z akcyzą energetyczną",
        publiczne.cadastre,
        publiczne.cadastral_value.get() / 100,
        publiczne.licensed_types,
        publiczne.vat_goods,
        publiczne.excise_goods,
        publiczne.excise_services
    );

    // Usługi publiczne, urzędy i egzekucja (M8d). Po mieście, bo placówka jest
    // finansowana z budżetu, i po populacji, bo jej obsada to komponent
    // `Employment` mieszkańców, a nie liczba wpisana w dane.
    let uslugi = city_bridge::setup_services(&mut world, &city)?;
    eprintln!(
        "usługi publiczne: {} placówek ({} rodzajów z 8 ma choć jedną), {} urzędów, {} etatów w normatywie; próg emisji {} g pyłu/min",
        uslugi.services, uslugi.live_kinds, uslugi.offices, uslugi.staff, uslugi.emission_limit_g_per_min
    );

    // Władza. Po usługach, bo poparcie stoi na pokryciu usługami, a po mieście,
    // bo uchwała zmienia stawki w kodeksie (M8e).
    let wladza = city_bridge::setup_government(&mut world, &city, a.seed)?;
    eprintln!(
        "władza miejska: {} dzielnic, burmistrz {}, rada {} mandatów, kadencja {} miesięcy",
        wladza.districts, wladza.mayor_axis, wladza.council_seats, wladza.term_months
    );

    // Sieci przesyłowe. Po zakładach, bo moc przyłączeniowa bierze się z linii
    // produkcyjnych, i po mieście, bo akcyza od energii nalicza się na rachunku.
    let sieci = grid_bridge::setup(&mut world, &city)?;
    eprintln!(
        "sieci przesyłowe: {} sieci, {} węzłów, {} krawędzi; elektrownia {} MW przy szczycie {} MW, {} zakładów na prądzie, {} na wodzie",
        sieci.nets,
        sieci.nodes,
        sieci.edges,
        sieci.source_w / 1_000_000,
        sieci.peak_w / 1_000_000,
        sieci.powered_sites,
        sieci.watered_sites
    );

    // Zdarzenia. Po sieciach, bo bramka „sieć ma czynne źródło" pyta rejestr sieci,
    // i po zakładach, bo wycinek miasta bierze się z areny zakładów.
    let zdarzenia = events_bridge::setup(
        &mut world,
        &city,
        &klimat,
        a.epoch
            .parse::<magnat_world::Epoch>()
            .map_or(1990, |e| e.year()),
    )?;
    eprintln!(
        "zdarzenia: {} definicji, {} zakładów w wycinku ({} rolnych) w {} dzielnicach;          norma klimatu {} °C i {} mm rocznie",
        zdarzenia.defs,
        zdarzenia.sites,
        zdarzenia.farms,
        zdarzenia.districts,
        zdarzenia.mean_temp_dc / 10,
        zdarzenia.annual_precip_mm
    );

    bootstrap_day(&mut world, 0);
    let mut builder = ScheduleBuilder::new();
    builder
        .add(magnat_events::EventSystem::new())
        .add(magnat_traffic::utility::UtilitySystem::new(
            magnat_traffic::utility::GridTuning::load_default()?,
        ))
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

/// Sekcja „zdarzenia i pogoda" raportu. Zwraca `false`, gdy bramka podfazy M8c
/// się nie zamknęła: przebieg dłuższy niż rok gry ma pokazać zdarzenia
/// z **każdej** kategorii, bo katalog, z którego odpala się jedna piąta,
/// wygląda w raporcie tak samo jak katalog kompletny (`R2`).
fn raport_zdarzen(world: &magnat_ecs::World, dob: u32) -> bool {
    use magnat_core::EventCategory;
    let Some(ev) = world.get_resource::<magnat_events::Events>() else {
        return true;
    };
    println!("── zdarzenia i pogoda ────────────────────────────────");
    let w = ev.weather().weather();
    let wsk = ev.indicators();
    // Znak przed przecinkiem, nie po: `-5` dziesiątych to −0,5 °C, a `temp/10`
    // daje wtedy zero i minus przepada. Ten sam błąd co przy dzieleniu groszy.
    let znak = if w.temp_dc < 0 { "-" } else { "" };
    println!(
        "pogoda: {znak}{},{} °C, opad {} ‰, wiatr {} km/h, pokrywa śnieżna {} mm",
        (w.temp_dc / 10).abs(),
        (w.temp_dc % 10).abs(),
        w.precip_permille,
        w.wind_kmh,
        w.snow_cover_mm
    );
    println!(
        "wskaźniki miasta: CPI {} bp (r/r {} bp), bezrobocie {} ‰, kredyt {} bp, nastrój {}, koniunktura {} bp",
        wsk.cpi_index_bp,
        wsk.cpi_yoy_bp,
        wsk.unemployment_permille,
        wsk.credit_growth_bp,
        wsk.mood_mean.get(),
        wsk.heat_bps
    );

    let mut razem = 0u32;
    let mut zywe_kategorie = 0;
    for kat in EventCategory::ALL {
        let ile: u32 = ev
            .catalog()
            .defs
            .iter()
            .enumerate()
            .filter(|(_, d)| d.category == *kat)
            .filter_map(|(i, _)| ev.diagnosis(u16::try_from(i).unwrap_or(0)))
            .map(|d| d.fired)
            .sum();
        razem += ile;
        if ile > 0 {
            zywe_kategorie += 1;
        }
        println!("{:<16} {ile:>4} wystąpień", kat.name());
    }
    println!(
        "razem {razem} zdarzeń, {} trwa teraz, {} parametrów pod nakładką",
        ev.active().len(),
        ev.overlay().len()
    );

    // „Dlaczego jeszcze nie" — pięć definicji o najwyższym hazardzie w ostatniej
    // ocenie, z rozbiciem na czynniki. To jest odpowiedź, której wymaga WP4:
    // gracz ma zobaczyć, że blok stoi na ×5,0 od wieku, a nie że hazard wynosi 137 ppm.
    let mut wiersze: Vec<(u32, &str, String)> = Vec::new();
    for (i, d) in ev.catalog().defs.iter().enumerate() {
        let Some(diag) = ev.diagnosis(u16::try_from(i).unwrap_or(0)) else {
            continue;
        };
        let czynniki = diag
            .best_factors
            .iter()
            .map(|f| {
                format!(
                    "{:?}={} ×{},{:02}",
                    f.probe,
                    f.value,
                    f.mul_bps / 10_000,
                    (f.mul_bps % 10_000) / 100
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        wiersze.push((diag.best_ppm, d.key.as_str(), czynniki));
    }
    wiersze.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    println!("najbliżej zajścia w ostatniej ocenie:");
    for (ppm, klucz, czynniki) in wiersze.iter().take(5) {
        println!("  {klucz:<40} {ppm:>7} ppm   {czynniki}");
    }

    // Bramka. Rok gry to minimum, w którym każda pora roku wypadła raz — poniżej
    // niego brak zdarzeń zimowych nie jest wadą katalogu, tylko krótkim przebiegiem.
    if dob < 360 {
        println!("(przebieg krótszy niż rok gry — bramka kategorii nie obowiązuje)");
        return true;
    }
    if zywe_kategorie < 6 {
        println!(
            "BRAMKA: zdarzenia z {zywe_kategorie} z 6 kategorii, oczekiwano wszystkich sześciu"
        );
        return false;
    }
    println!("bramka M8c: zdarzenia ze wszystkich sześciu kategorii — zielona");
    true
}

/// Zwraca `false`, gdy którakolwiek bramka scenariusza się nie zamknęła.
/// Sekcja „usługi publiczne i urzędy" raportu (M8d).
///
/// Odpowiada na trzy pytania i tylko na te trzy: czy placówki mają obsadę, czy
/// kolejka urzędu się rusza i czy urzędy kontrolne cokolwiek robią. Każde z nich
/// jest pytaniem o **martwy mechanizm** — placówka bez obsady, urząd bez wniosku
/// i urząd kontrolny bez sprawy przechodzą każdy test i w raporcie wyglądają
/// tak samo jak działające (`R2`).
fn raport_uslug(miasto: &City, world: &magnat_ecs::World) {
    use magnat_core::{AgencyKind, ServiceKind};
    if miasto.services.is_empty() {
        return;
    }
    println!("── usługi publiczne ──────────────────────────────────");
    for k in ServiceKind::ALL {
        let ile = miasto.services.count_of(*k);
        if ile == 0 {
            println!("{:<9} — brak placówek w tym mieście", k.name());
            continue;
        }
        let swoje: Vec<&magnat_city::PublicService> = miasto
            .services
            .all()
            .iter()
            .filter(|s| s.kind == *k)
            .collect();
        let jakosc: u32 = swoje.iter().map(|s| u32::from(s.quality.get())).sum::<u32>() / ile;
        let etaty: u32 = swoje.iter().map(|s| s.staff_target).sum();
        let obsada: u32 = swoje.iter().map(|s| s.staff).sum();
        let obciazenie: u32 =
            swoje.iter().map(|s| s.utilization_bps).sum::<u32>() / ile;
        println!(
            "{:<9} {ile:>3} placówek, jakość {jakosc:>3}, obsada {obsada}/{etaty}, obłożenie {} %, pokrycie miasta {}",
            k.name(),
            obciazenie / 100,
            miasto.services.coverage().city_mean(*k).get()
        );
    }

    println!("── urząd i egzekucja ─────────────────────────────────");
    println!(
        "pozwolenia: {} wydanych, {} w kolejce, {} wygasłych, mediana oczekiwania {}",
        miasto.permits.issued(),
        miasto.permits.open_count(),
        miasto.permits.expired(),
        miasto
            .permits
            .median_wait_days()
            .map_or("— (żadnego nie wydano)".to_string(), |d| format!("{d} dób"))
    );
    for k in AgencyKind::ALL {
        let a = miasto.enforcement.agencies()[k.as_index()];
        println!(
            "{:<16} {} inspektorów, spraw otwartych {}, zamkniętych {}",
            k.name(),
            a.inspectors,
            a.opened - a.closed,
            a.closed
        );
    }
    let domiary: i64 = miasto
        .enforcement
        .cases()
        .iter()
        .filter_map(|c| c.remedy)
        .map(|r| r.amount().get())
        .sum();
    println!("kary i domiary razem: {} zł", domiary / 100);

    // Szara strefa i emisje: dwie liczby, których brak wygląda tak samo jak zero.
    // `R10` mówi „cel 5–20 % firm", a emisje są jedynym wejściem ochrony środowiska.
    if let Some(m) = world.get_resource::<magnat_economy::Market>() {
        let (ile, sredni) = m.shadow_stats();
        println!(
            "szara strefa: {ile} zakładów ukrywa część obrotu, średnio {} %",
            sredni / 100
        );
    }
    let max_pyl = miasto.emissions.iter().map(|(_, _, g)| *g).max().unwrap_or(0);
    println!(
        "emisje: {} zakładów z niezerowym pyłem, najbrudniejszy {max_pyl} g/min",
        miasto.emissions.len()
    );

    // Diagnostyka kontroli skarbowej: „dlaczego jeszcze nie" dla **tej jednej**
    // definicji. Stoi osobno od listy pięciu najbliższych, bo to jest definicja,
    // od której zależy kryterium ukończenia WP8 — a definicja odsiana bramką
    // nie pojawia się w tamtej liście wcale i wygląda tak samo jak definicja
    // o niskim hazardzie (`R2`).
    if let Some(ev) = world.get_resource::<magnat_events::Events>() {
        if let Some((i, _)) = ev
            .catalog()
            .defs
            .iter()
            .enumerate()
            .find(|(_, d)| d.key == "political/tax_audit")
        {
            match ev.diagnosis(u16::try_from(i).unwrap_or(0)) {
                Some(d) => println!(
                    "kontrola skarbowa: {} kandydatów, {} odsianych bramką, hazard {} ppm, {} wystąpień",
                    d.candidates, d.gated_out, d.best_ppm, d.fired
                ),
                None => println!("kontrola skarbowa: definicja nigdy nie była oceniana"),
            }
        }
    }
}

/// Sekcja „władza i wybory" raportu (M8e).
///
/// Zwraca `false`, gdy bramka podfazy się nie zamknęła. Bramka jest jedna i pyta
/// o **martwy mechanizm**, tak samo jak bramka kategorii zdarzeń: przebieg dłuższy
/// niż kadencja ma pokazać przynajmniej jedne wybory i przynajmniej jedną uchwałę.
/// Burmistrz, który przez dwadzieścia lat nie podjął ani jednej decyzji, przechodzi
/// każdy test i w raporcie wygląda tak samo jak burmistrz ostrożny (`R2`).
fn raport_wladzy(miasto: &City, dob: u32) -> bool {
    use magnat_core::PolicyKind;
    let gov = &miasto.gov;
    if !gov.is_active() {
        return true;
    }
    println!("── władza i wybory ───────────────────────────────────");
    println!(
        "burmistrz {} (mandat #{}), rada {} mandatów, poparcie {} %, miesiąc kadencji {}",
        gov.mayor_pref.dominant().name(),
        if gov.mayor == magnat_city::Government::NO_MAYOR {
            "—".to_string()
        } else {
            gov.mayor.to_string()
        },
        gov.council.len(),
        gov.approval_mean_bp() / 100,
        gov.month
    );
    let s = gov.signals;
    println!(
        "sygnały: saldo {} bp, dług {} % wpływów rocznych, luka w usługach {} %, bezrobocie {} ‰, szara strefa {} %, emisje {} % limitu",
        s.fiscal_bp,
        s.debt_bp / 100,
        s.service_gap_bp / 100,
        s.unemployment_permille,
        s.shadow_bp / 100,
        s.emission_bp / 100
    );

    // Poparcie per dzielnica — najlepsza i najgorsza. To jest liczba, na której
    // stoi zdanie z §1 dokumentu fazy („przegrana w tym obwodzie").
    if !gov.approval_bps_by_district.is_empty() {
        let naj = gov.approval_bps_by_district.iter().max().unwrap_or(&0);
        let min = gov.approval_bps_by_district.iter().min().unwrap_or(&0);
        println!(
            "poparcie w obwodach: od {} % do {} % (rozpiętość {} pkt proc.)",
            min / 100,
            naj / 100,
            (naj - min) / 100
        );
    }

    let uchwal: u32 = gov.enacted.iter().sum();
    print!("uchwały: {uchwal} razem");
    for k in PolicyKind::ALL {
        let ile = gov.enacted[k.as_index()];
        if ile > 0 {
            print!(", {} ×{ile}", k.name());
        }
    }
    println!();
    println!(
        "stawki dziś: CIT {} bp, PIT {} bp, VAT {} bp, nieruchomości {} bp, akcyza ×{} bp",
        miasto.code.rate_of(magnat_core::TaxKind::Cit),
        miasto.code.rate_of(magnat_core::TaxKind::Pit),
        miasto.code.rate_of(magnat_core::TaxKind::Vat),
        miasto.code.rate_of(magnat_core::TaxKind::Property),
        miasto.code.rate_of(magnat_core::TaxKind::Excise)
    );

    let rozstrzygniete = miasto
        .tenders
        .all()
        .iter()
        .filter(|x| x.outcome.is_some())
        .count();
    let umowy: i64 = miasto
        .tenders
        .all()
        .iter()
        .filter_map(|x| x.outcome.as_ref())
        .map(|o| o.price.get())
        .sum();
    println!(
        "przetargi: {} ogłoszonych, {rozstrzygniete} rozstrzygniętych, umowy na {} zł miesięcznie",
        miasto.tenders.len(),
        umowy / 100
    );

    let mut wyborow = 0;
    if let Some(e) = &miasto.election {
        wyborow = 1;
        if let Some(r) = &e.result {
            println!(
                "wybory: frekwencja {} %, {} kandydatów, zwycięzca {} % głosów, {}",
                r.turnout_bp / 100,
                e.candidates.len(),
                r.winner_bp / 100,
                if r.incumbent_won {
                    "burmistrz utrzymał urząd"
                } else {
                    "miasto ma nowego burmistrza"
                }
            );
            // Rozpiętość wyniku zwycięzcy po obwodach: to jest dokładnie ta liczba,
            // o którą chodzi w demie fazy — spadek poparcia w dzielnicy z awarią.
            let mut naj = 0u32;
            let mut min = 10_000u32;
            for d in &r.per_district {
                if d.voted == 0 {
                    continue;
                }
                let u = d.share_bp(usize::from(r.mayor));
                naj = naj.max(u);
                min = min.min(u);
            }
            if naj > 0 {
                println!("  wynik zwycięzcy po obwodach: od {} % do {} %", min / 100, naj / 100);
            }
        } else {
            println!("wybory: kampania trwa, {} kandydatów", e.candidates.len());
        }
    }

    // „Dlaczego" — ostatnie decyzje władzy. Bez tego uchwały są liczbą w tabeli,
    // a bramka 5 fazy żąda powodu widocznego w inspektorze.
    let cat = magnat_ui::Catalog::load().ok();
    if let Some(c) = &cat {
        println!("ostatnie decyzje władzy:");
        for (tick, powod) in miasto.reasons.iter().rev().take(6) {
            println!(
                "  doba {:>5}  {}",
                tick.get() / 1_440,
                magnat_ui::inspect::reason::describe(c, magnat_ui::Locale::Pl, *powod)
            );
        }
    }

    // Bramka: przebieg dłuższy niż kadencja ma pokazać wybory i uchwały.
    let kadencja_dob = u32::try_from(gov.term_ticks / 1_440).unwrap_or(u32::MAX);
    if dob < kadencja_dob + 60 {
        println!("(przebieg krótszy niż kadencja — bramka władzy nie obowiązuje)");
        return true;
    }
    if uchwal == 0 {
        println!("BRAMKA: burmistrz nie podjął ani jednej decyzji przez całą kadencję");
        return false;
    }
    if wyborow == 0 {
        println!("BRAMKA: minęła kadencja, a wybory się nie odbyły");
        return false;
    }
    println!("bramka M8e: władza rządzi i wybory się odbyły — zielona");
    true
}

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

    ok &= raport_zdarzen(world, a.days);
    raport_uslug(miasto, world);
    ok &= raport_wladzy(miasto, a.days);

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
