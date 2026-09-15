//! Scenariusz `m3day` — **artefakt końcowy fazy M3** (§1, punkt 1).
//!
//! „30 dni gry na 120 tys. mieszkańców bez GPU; na wyjściu: histogram wykorzystania
//! czasu doby, rozkład czasu dojazdu, poziomy potrzeb, log zdarzeń DES, hash stanu
//! co 1000 ticków."
//!
//! Różnica wobec `day` z M3b jest cała w tym, co go otacza: miasto jest prawdziwe
//! (M2 + Etap 8), mieszkańcy są encjami ECS, a pętlę przewijają **systemy z §5.12**,
//! nie runner. Runner mierzy i wypisuje.
//!
//! **Od M4b jest to też artefakt tej podfazy**: w świecie jest flota, a mieszkańcy
//! z samochodem jadą siecią zamiast teleportować się po czasie z formuły. Sekcja
//! „ruch (warstwa mezo)" wypisuje to, co M4b ma pokazać: przejazdy, korki, rozbiór
//! czasu przejazdu i dwa bilanse, które muszą wyjść co do zera.

use clap::Args;
use magnat_agents::{
    bootstrap_day, register_day, society, AgentSources, DayLoopSystem, DayStats, EventQueue,
    HouseholdStockSystem, InfinitePlaces, NeedTable, Needs, NoInheritance, Population,
    ReplanCooldownSystem, SkillDriftSystem, SocietySystem, TravelMicroSystem,
};
use magnat_agents::{AgentState, DeprivationEffectsSystem, Employment, NeedDecaySystem};
use magnat_core::{ActivityKind, NeedKind};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use std::process::ExitCode;
use std::sync::Arc;

use crate::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_agents::Trace;
use magnat_core::{SimSpeed, Tick};
use magnat_traffic::{FuelLedger, TrafficNetwork, TrafficSystem, VehicleWearSystem, UL_PER_ML};
use magnat_ui::{
    CitizenPanel, InspectorPanel, ListPicker, Selection, TimeControlsWidget, UiContext,
};

#[derive(Args, Debug)]
pub struct M3DayArgs {
    #[arg(long, default_value = "1")]
    pub seed: String,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "8km")]
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
    #[arg(long, default_value_t = 1)]
    pub days: u32,

    /// Docelowa liczba mieszkańców; 0 = z pojemności miasta.
    #[arg(long, default_value_t = 0)]
    pub citizens: u32,

    /// Co ile ticków liczyć hash stanu (0 = nigdy).
    #[arg(long, default_value_t = 1000)]
    pub hash_every: u64,

    /// Zapis ciągu hashy do pliku — wejście testu determinizmu `det_agents_hash`.
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,

    /// Porównanie z zapisanym ciągiem; różnica → kod wyjścia 1.
    #[arg(long)]
    pub expect: Option<std::path::PathBuf>,

    /// Promień okna LOD Mikro w metrach wokół środka miasta; 0 = warstwa wyłączona.
    ///
    /// Headless nie ma kadru, więc domyślnie nie płaci za warstwę **nic** (`Z-6`).
    /// Otwarcie okna jest tu po to, żeby dowieść §5.4 na pełnym scenariuszu, a nie
    /// tylko na teście jednostkowym: ten sam seed z oknem i bez okna ma dać identyczny
    /// ciąg hashy (`camera_does_not_change_world`).
    #[arg(long, default_value_t = 0)]
    pub micro: u32,

    /// Karta inspekcji tego mieszkańca (indeks na liście populacji) po przebiegu.
    #[arg(long)]
    pub inspect: Option<usize>,

    /// Język karty: `pl` albo `en`.
    #[arg(long, default_value = "pl")]
    pub locale: String,

    /// Prędkość gry: `1`, `3`, `10` albo `0` (pauza). Doba przy każdej z nich ma dać
    /// **ten sam** hash stanu — to jest kryterium WP11 i wymóg PRD §14.5.
    #[arg(long, default_value_t = 1)]
    pub speed: u32,

    /// Nakładka ruchu do zrzucenia na PNG po przebiegu: `traffic_flow`, `congestion`,
    /// `isochrone`, `parking_occupancy` albo `transit_load` (M4d/WP11).
    ///
    /// Podgląd headless jest **dowodem, że nakładka nie jest efektem shadera, tylko
    /// danych**: paleta, progi i jednostka pochodzą z `data/ui/overlays.ron`, a klient
    /// graficzny czyta dokładnie ten sam plik.
    #[arg(long)]
    pub overlay: Option<String>,

    /// Gdzie zapisać zrzut nakładki.
    #[arg(long, default_value = "podglad_ruch.png")]
    pub overlay_out: std::path::PathBuf,
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

pub fn run(a: &M3DayArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let seed = parse_seed(&a.seed)?;
    let pool = JobPool::new(a.threads);
    let start = std::time::Instant::now();
    let city = zbuduj_miasto(seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
    eprintln!("miasto {:.1} s", start.elapsed().as_secs_f64());

    let mut world = swiat_agentow(seed)?;
    register_day(&mut world);
    let start = std::time::Instant::now();
    let zaludnione = zaludnij(&mut world, &city, a.citizens, 200_000)?;
    eprintln!("Etap 8 {:.1} s", start.elapsed().as_secs_f64());
    for l in zaludnione.report.lines() {
        eprintln!("{l}");
    }

    // Źródła miejsc i podróży: to jest cały most między miastem a agentami.
    // M5 podmienia `InfinitePlaces` na indeks ofert; **M4b podmienił `travel`**
    // na oracle ruchu (`Z-1`) i od tej chwili mieszkańcy z samochodem naprawdę
    // jadą siecią, zamiast teleportować się po czasie z formuły.
    let tabela = Arc::new(NeedTable::load_default()?);
    eprintln!(
        "flota: {} pojazdów w {} gospodarstwach, {} stacji, cache tras {}",
        zaludnione.fleet.vehicles,
        zaludnione.fleet.households,
        zaludnione.fleet.stations,
        zaludnione.fleet.route_cache_capacity
    );
    *world.resource_mut::<AgentSources>() = AgentSources::new(
        Box::new(InfinitePlaces::new(zaludnione.places.clone(), tabela)),
        zaludnione.travel_oracle(),
    );

    let zaplanowanych = bootstrap_day(&mut world, 0);
    eprintln!(
        "kolejka zasiana: {zaplanowanych} mieszkańców, {} zdarzeń",
        world.resource::<EventQueue>().len()
    );

    let mut builder = ScheduleBuilder::new();
    builder
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world))
        .add(VehicleWearSystem::new(&world));
    if a.micro > 0 {
        builder.add(TravelMicroSystem::new(&world));
    }
    if a.micro > 0 {
        // Środek kadru bierze się z miasta, nie ze środka mapy: miasto nie musi leżeć
        // pośrodku, a okno postawione obok niego dałoby pusty kadr i „dowód", który
        // niczego nie dowodzi.
        let wpisy = zaludnione.places.entries();
        let n = wpisy.len().clamp(1, 512);
        let (mut sx, mut sy) = (0i64, 0i64);
        for w in wpisy.iter().take(n) {
            let c = zaludnione
                .places
                .coord_of(w.place)
                .unwrap_or(magnat_core::WorldCoord::ORIGIN);
            sx += i64::from(c.x);
            sy += i64::from(c.y);
        }
        let srodek = ((sx / n as i64 / 100) as i32, (sy / n as i64 / 100) as i32);
        zaludnione
            .traffic
            .micro()
            .set_window(Some(srodek), a.micro);
        eprintln!(
            "LOD Mikro: okno {} m wokół ({}, {})",
            a.micro, srodek.0, srodek.1
        );
    }
    let schedule = builder.build()?;
    eprintln!(
        "harmonogram: {} systemów w {} etapach, odcisk {:#018x}",
        schedule.system_count(),
        schedule.stage_count(),
        schedule.fingerprint()
    );

    let mut app = App::new(world, schedule, a.threads);

    // Karta inspekcji potrzebuje **realizacji**, a nie tylko planu: bufor śledzenia
    // zbiera zdarzenia DES obserwowanego mieszkańca (decyzja 9.16, najwyżej ośmiu).
    let wybrany = a.inspect.and_then(|n| {
        let picker = ListPicker::new(app.world.resource::<Population>().citizens().to_vec());
        match picker.by_index(n) {
            Selection::Citizen(c) => {
                // Dwa bufory śledzenia, bo dwie różne rzeczy: `Trace` zbiera zdarzenia
                // DES (co mieszkaniec robił), `TrafficOracle::watch` włącza rejestr
                // krawędź po krawędzi i zapamiętywanie porównania środków transportu
                // (jak jechał i dlaczego tak). Bez tego drugiego `TripLedger.entries`
                // jest puste dla **każdego** mieszkańca, a karta podróży nie ma z czego
                // policzyć rozbioru czasu (`N-6`).
                app.world.resource_mut::<Trace>().watch(c.entity().index());
                app.world
                    .resource::<magnat_traffic::TrafficServices>()
                    .oracle
                    .watch(c.entity().index());
                Some(c)
            }
            _ => None,
        }
    });

    // Prędkość gry idzie przez ten sam widget, którego używa klient graficzny.
    // Wynik **nie zależy** od niej: zegar zamienia czas realny na liczbę minut,
    // a każda minuta jest tym samym tickiem (§5.11, test `det_speed_invariance`).
    let mut zegar = TimeControlsWidget::new(Tick(0));
    zegar.set_speed(match a.speed {
        0 => SimSpeed::Paused,
        3 => SimSpeed::X3,
        10 => SimSpeed::X10,
        _ => SimSpeed::X1,
    });
    let ticki = u64::from(a.days) * 1440;
    let mut hashe: Vec<(u64, String)> = Vec::new();
    let bieg = std::time::Instant::now();
    let mut wykonane = 0u64;
    while wykonane < ticki {
        // Klatka 16 ms — tyle samo, ile w kliencie przy 60 FPS.
        let minut = u64::from(zegar.advance(16)).min(ticki - wykonane);
        for _ in 0..minut {
            app.tick();
            wykonane += 1;
            if a.hash_every > 0 && wykonane.is_multiple_of(a.hash_every) {
                hashe.push((wykonane, world_state_hash(&app.world).to_string()));
            }
        }
        if zegar.speed() == SimSpeed::Paused {
            eprintln!("pauza: symulacja stoi, przebieg kończy się po {wykonane} tickach");
            break;
        }
    }
    let czas = bieg.elapsed();

    let stats = *app.world.resource::<DayStats>();
    let ludzi = society::population(&app.world);
    eprintln!(
        "{} dób gry dla {ludzi} mieszkańców w {:.2} s ({:.0}× czasu rzeczywistego)",
        a.days,
        czas.as_secs_f64(),
        ticki as f64 * 60.0 / czas.as_secs_f64().max(1e-9)
    );
    println!("zdarzenia DES: {}", stats.events);
    println!(
        "  plany {}, przeplanowania {}, dojazdy {}, przybycia {}",
        stats.plans, stats.replans, stats.trips, stats.arrivals
    );
    println!(
        "  spóźnienia: {} przybyć, średnio {:.1} min — plan powstaje o północy, a marsz          zwalnia razem z energią; od tego jest `ReplanCause::Late`",
        stats.late,
        stats.late_minutes as f64 / stats.late.max(1) as f64
    );
    println!(
        "  wizyty zaspokojone {}, odmowy {}",
        stats.fulfilled, stats.refused
    );

    let ruch_ok = ruch(&app.world);
    let rozklad_ok = wybor_srodka(&app.world);
    let parking_ok = parkingi(&app.world);
    let transit_ok = komunikacja(&app.world);
    profil_doby(&app.world);
    potrzeby(&app.world);
    if let Some(klucz) = &a.overlay {
        nakladka(&app.world, &city, klucz, &a.overlay_out)?;
    }
    if let Some(c) = wybrany {
        karta(&app.world, c, wykonane / 1440, seed, &a.locale)?;
    }

    if let Some(p) = &a.out {
        let tekst: String = hashe
            .iter()
            .map(|(t, h)| format!("{t} {h}\n"))
            .collect();
        std::fs::write(p, tekst)?;
        eprintln!("zapisano {} hashy do {}", hashe.len(), p.display());
    }
    if let Some(p) = &a.expect {
        let wzorzec = std::fs::read_to_string(p)?;
        let nasz: String = hashe
            .iter()
            .map(|(t, h)| format!("{t} {h}\n"))
            .collect();
        if wzorzec != nasz {
            let pierwsza = wzorzec
                .lines()
                .zip(nasz.lines())
                .find(|(a, b)| a != b)
                .map_or("(inna długość ciągu)".to_string(), |(a, b)| {
                    format!("oczekiwano `{a}`, jest `{b}`")
                });
            eprintln!("BŁĄD: ciąg hashy się rozjechał — {pierwsza}");
            return Ok(ExitCode::FAILURE);
        }
        eprintln!("ciąg {} hashy zgodny z wzorcem", hashe.len());
    }

    // Spóźnienie musi wywołać przeplanowanie — inaczej mieszkaniec wykonuje plan,
    // który już nie pasuje do jego doby (§5.2). Debouncing 15 minut sprawia, że
    // przeplanowań bywa mniej niż spóźnień, ale zera przy niepustych spóźnieniach
    // być nie może.
    if stats.late > 0 && stats.replans == 0 {
        eprintln!("BŁĄD: {} spóźnień i ani jednego przeplanowania", stats.late);
        return Ok(ExitCode::FAILURE);
    }
    if !ruch_ok {
        eprintln!(
            "BŁĄD: podróż bez uzasadnienia — bramka 5 z §7.4 i kryterium zamknięcia M4c"
        );
        return Ok(ExitCode::FAILURE);
    }
    if !parking_ok {
        eprintln!(
            "BŁĄD: obłożenie parkingów rozjechało się z przypisaniem pojazdów —              `parking_no_ghosts` (§7.1)"
        );
        return Ok(ExitCode::FAILURE);
    }
    if !transit_ok {
        eprintln!("BŁĄD: pasażerów w pojeździe więcej niż pojemność — `transit_capacity` (§7.1)");
        return Ok(ExitCode::FAILURE);
    }
    if !rozklad_ok {
        eprintln!(
            "BŁĄD: rozkład udziału środków transportu poza widełkami odniesienia              (`data/roads/mode_choice.ron`) — kryterium WP6"
        );
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// Rozkład udziału środków transportu i jego bramka (kryterium WP6).
///
/// Widełki są w `data/roads/mode_choice.ron`, bo PRD §20.1 mówi „w zakresach
/// referencyjnych **dla epoki**" i liczb nie podaje — podaje je tabela, a tabela
/// jest daną, nie kodem. Zwraca `false`, gdy którykolwiek udział z niej wypadł.
fn wybor_srodka(world: &magnat_ecs::World) -> bool {
    use magnat_traffic::{TravelOption, TrafficServices};
    let services = world.resource::<TrafficServices>();
    let n = services.oracle.mode_counts();
    let suma: u64 = n.iter().sum();
    println!("
wybór środka transportu (WP6)");
    if suma == 0 {
        println!("  brak podróży — nie ma czego mierzyć");
        return true;
    }
    let permille = |k: u64| (k * 1_000 / suma) as u16;
    for (i, o) in TravelOption::ALL.iter().enumerate() {
        println!(
            "  {:<14} {:>7} podróży ({:>3},{} %)",
            o.key(),
            n[i],
            permille(n[i]) / 10,
            permille(n[i]) % 10
        );
    }
    // Widełki są dla pięciu pozycji: auto własne i rodzinne to jeden środek.
    let udzialy = [
        permille(n[0]),
        permille(n[1]),
        permille(n[2]),
        permille(n[3] + n[4]),
        permille(n[5]),
    ];
    let nazwy = ["pieszo", "rower", "komunikacja", "samochód", "taksówka"];
    let params = services.oracle.params();
    let widelki = &params.reference_share_permille;
    // Widełki obowiązują dla scenariusza odniesienia z §7.3. Mniejsze miasto ma
    // krótsze podróże i inny rozkład — i to jest poprawne, więc bramka tam milczy
    // zamiast kłamać.
    let ludzi = world.resource::<magnat_agents::Population>().citizens().len() as u32;
    let bramka = ludzi >= params.reference_population;
    let mut ok = true;
    for (i, u) in udzialy.iter().enumerate() {
        let w = widelki[i];
        let werdykt = match (w.contains(*u), bramka) {
            (true, _) => "w widełkach",
            (false, false) => "poza (miasto mniejsze od odniesienia — bramka nie działa)",
            (false, true) => "POZA",
        };
        if !w.contains(*u) && bramka {
            ok = false;
        }
        println!(
            "  odniesienie {:<12} {:>3},{} % wobec {},{}–{},{} % → {werdykt}",
            nazwy[i],
            u / 10,
            u % 10,
            w.min / 10,
            w.min % 10,
            w.max / 10,
            w.max % 10
        );
    }
    let nie = services.oracle.infeasible_counts();
    println!(
        "  odpadło opcji: {} brak auta w GD, {} auto zajęte, {} brak parkingu, {} zasięg,         {} brak połączenia, {} brak trasy, {} za daleko, {} wiek/awaria",
        nie[0], nie[1], nie[2], nie[3], nie[4], nie[5], nie[6], nie[7]
    );
    ok
}

/// Obłożenie parkingów i niezmiennik „brak pojazdów widm" (kryterium WP7).
///
/// Zwraca `false`, gdy suma obłożeń rozjechała się z liczbą przypisanych pojazdów:
/// to jest test `parking_no_ghosts` z §7.1, tylko liczony na całym mieście zamiast
/// na scenariuszu.
fn parkingi(world: &magnat_ecs::World) -> bool {
    use magnat_traffic::{TrafficNetwork, TrafficServices};
    let services = world.resource::<TrafficServices>();
    let na_sieci = world.resource::<TrafficNetwork>().mezo.vehicles_on_network();
    services.oracle.with_parking(|p| {
        let zajete = p.occupied_total();
        let stoi = p.parked();
        let wolne = p.free_total();
        println!("
parkingi (WP7)");
        println!(
            "  {} parkingów, {} miejsc, zajętych {} ({},{} %)",
            p.lots().len(),
            zajete + wolne,
            zajete,
            zajete * 1_000 / (zajete + wolne).max(1) / 10,
            zajete * 1_000 / (zajete + wolne).max(1) % 10
        );
        println!(
            "  szukań miejsca {}, odmów {}, blokad wygasłych {}",
            p.searches, p.denials, p.expired
        );
        // `parking_no_ghosts` (§7.1): każdy zaparkowany pojazd zajmuje **jedno**
        // miejsce, a pojazd na sieci nie zajmuje żadnego.
        println!(
            "  bilans miejsc: {zajete} obłożeń == {stoi} zaparkowanych → {}",
            if zajete == stoi { "zgodny" } else { "ROZJAZD" }
        );
        println!(
            "  pojazdy: {stoi} na parkingach + {na_sieci} na sieci = {}",
            stoi + na_sieci
        );
        zajete == stoi
    })
}

/// Kursy, pasażerowie i przepełnienie (kryterium WP10).
///
/// Zwraca `false`, gdy w którymkolwiek kursie jest więcej pasażerów niż miejsc —
/// to jest test `transit_capacity` z §7.1.
fn komunikacja(world: &magnat_ecs::World) -> bool {
    use magnat_traffic::{FareLedger, TrafficServices};
    let services = world.resource::<TrafficServices>();
    let oplaty = *world.resource::<FareLedger>();
    services.oracle.with_transit(|t| {
        let s = t.stats;
        println!("
komunikacja miejska (WP10)");
        if t.is_empty() {
            println!("  brak linii — miasto nie ma komunikacji");
            return true;
        }
        println!(
            "  {} linii, {} przystanków, kursy: {} wypuszczone, {} zakończone, {} odwołane",
            t.lines().len(),
            t.lines().iter().map(magnat_traffic::TransitLine::stop_count).sum::<usize>(),
            s.runs_started,
            s.runs_finished,
            s.runs_cancelled
        );
        println!(
            "  pasażerowie: {} wsiadło, {} wysiadło, {} nie zmieściło się ({} odmów              wsiadania), {} zrezygnowało",
            s.boardings, s.alightings, s.left_behind, s.boarding_refusals, s.gave_up
        );
        println!(
            "  oczekiwanie średnio {:.1} min, największe obłożenie kursu {}",
            s.wait_minutes as f64 / s.boardings.max(1) as f64,
            s.max_occupancy
        );
        println!(
            "  spóźnienia kursów: suma {} min, największe {} min",
            s.delay_minutes, s.max_delay_minutes
        );
        println!(
            "  bilet: {:.2} zł przychodu z {} biletów; taksówki {:.2} zł; paliwo taboru {:.2} zł",
            oplaty.transit_revenue.0 as f64 / 100.0,
            oplaty.transit_tickets,
            oplaty.taxi_revenue.0 as f64 / 100.0,
            oplaty.transit_fuel_cost.0 as f64 / 100.0
        );
        t.runs().iter().all(|r| r.occupancy <= r.capacity)
    })
}

/// Raport warstwy mezo: przejazdy, korki, paliwo i dwa bilanse, które muszą
/// wyjść co do zera (kryterium zamknięcia M4b).
///
/// Zwraca `false`, gdy choć jedna podróż skończyła się bez uzasadnienia — bramka 5
/// z §7.4 i kryterium zamknięcia M4c („100 % decyzji transportowych ma uzasadnienie").
fn ruch(world: &magnat_ecs::World) -> bool {
    let net = world.resource::<TrafficNetwork>();
    let paliwo = world.resource::<FuelLedger>();
    let s = net.stats;
    println!("
ruch (warstwa mezo)");
    println!(
        "  przejazdy: {} wysłane, {} zakończone, {} nieudane, {} w toku",
        s.dispatched,
        s.arrived,
        s.failed,
        net.active()
    );
    println!(
        "  krawędzie: {} rozliczeń, {} wstrzymanych wjazdów (spillback), {} rozplątań",
        s.edge_settles, s.spillbacks, s.gridlock_releases
    );
    println!(
        "  korki: {} krawędzi poniżej połowy prędkości swobodnej w tej minucie",
        net.mezo.congested_edges()
    );
    println!(
        "  czas przejazdu: plan {:.1} min, faktycznie {:.1} min (iloraz {:.2})",
        s.planned_minutes_total as f64 / s.arrived.max(1) as f64,
        s.actual_minutes_total as f64 / s.arrived.max(1) as f64,
        s.actual_minutes_total as f64 / s.planned_minutes_total.max(1) as f64
    );
    let min = |cs: u64| cs as f64 / 6_000.0 / s.arrived.max(1) as f64;
    println!(
        "  uzasadnienia: {} środek, {} porównanie opcji, {} brak trasy, {} brak parkingu,          {} tankowanie, {} wybór stacji, {} spóźnienie, {} BEZ POWODU",
        s.reasons[0],
        s.reasons[1],
        s.reasons[2],
        s.reasons[3],
        s.reasons[4],
        s.reasons[5],
        s.reasons[6],
        s.reasons[7]
    );
    println!(
        "  rozbiór przejazdu: jazda {:.1} min, skrzyżowania {:.1} min, kolejki {:.1} min",
        min(s.travel_cs_total),
        min(s.node_delay_cs_total),
        min(s.blocked_cs_total)
    );
    println!(
        "  spóźnienia wobec planu: {} przybyć, średnio {:.1} min",
        s.late_arrivals,
        s.late_minutes as f64 / s.late_arrivals.max(1) as f64
    );
    println!(
        "  paliwo: spalone {:.1} l, zatankowane {:.1} l w {} tankowaniach za {:.2} zł",
        s.fuel_burned_ul as f64 / (UL_PER_ML * 1_000) as f64,
        paliwo.volume_ul as f64 / (UL_PER_ML * 1_000) as f64,
        paliwo.purchases,
        paliwo.revenue.0 as f64 / 100.0
    );
    let mut pelne = 0u32;
    let mut male = 0u32;
    let mut min_storage = u16::MAX;
    let mut zablokowane = 0u32;
    for (i, q) in net.mezo.queues.iter().enumerate() {
        if q.is_full() {
            pelne += 1;
        }
        if q.storage_capacity < 3 {
            male += 1;
        }
        min_storage = min_storage.min(q.storage_capacity);
        if q.stuck_minutes > 0 {
            zablokowane += 1;
        }
        let _ = i;
    }
    println!(
        "  porażki: {} zakleszczenie; {} zawrócenia, max czekania {} min, max krawędzi {}",
        s.failed_gridlock, s.u_turns, s.max_blocked_minutes, s.max_edges_per_trip
    );
    if let Some((e, n)) = net.worst_bottleneck() {
        let services = world.resource::<magnat_traffic::TrafficServices>();
        services.oracle.with_road(|road| {
            let r = road.edge(e);
            println!(
                "  najwęższe gardło: krawędź {} ({:?}, {} m, {} pasów, pojemność {},                  v0 {} dkmh) — {} odmów wjazdu",
                e.0,
                r.class,
                r.length_cm / 100,
                r.lanes,
                net.mezo.queues[e.0 as usize].storage_capacity,
                net.mezo.links[e.0 as usize].free_flow_dkmh,
                n
            );
        });
    }
    println!(
        "  krawędzie pełne: {pelne}, o pojemności < 3: {male}, min {min_storage},          nieruchome: {zablokowane} z {}",
        net.mezo.queues.len()
    );
    println!(
        "  bilans pojazdów: {} na sieci == {} wjazdów − {} wyjazdów → {}",
        net.mezo.vehicles_on_network(),
        s.entries,
        s.exits,
        if net.conserved() { "zgodny" } else { "ROZJAZD" }
    );
    s.reasons[7] == 0 && net.conserved()
}

/// Zrzut nakładki ruchu do PNG (WP11) — **bez GPU**.
///
/// Klient graficzny i ten podgląd czytają tę samą tabelę `data/ui/overlays.ron`
/// (`K-19`) i tę samą funkcję rastrującą z `sim/traffic`, więc jeśli barwa tutaj się
/// zgadza, to zgadza się i tam. Odwrotnie byłoby bez wartości: nakładka narysowana
/// wyłącznie shaderem nie daje się sprawdzić w CI, a headless-first jest wymogiem (00 §6).
fn nakladka(
    world: &magnat_ecs::World,
    city: &magnat_world::CityData,
    klucz: &str,
    out: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use magnat_traffic::{rasterize_edges, rasterize_points, TrafficField, TrafficOverlay};

    let pole = TrafficField::ALL
        .iter()
        .copied()
        .find(|f| f.key() == klucz)
        .ok_or_else(|| {
            format!(
                "nieznana nakładka {klucz}; dostępne: {}",
                TrafficField::ALL
                    .iter()
                    .map(|f| f.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    let tab = magnat_world::OverlayTable::load()?;
    let spec = tab.get(klucz)?;
    let paleta = spec.palette();
    let bok = u32::from(magnat_world::city::overlay::OVERLAY_CELL_M);
    let dim = (city.plan.map_size_m().max(1) as u32 / bok).max(1);

    let oracle = world.resource::<magnat_traffic::TrafficServices>().oracle.clone();
    let raster = world
        .resource::<TrafficOverlay>()
        .with_front(|snap| {
            oracle.with_road(|road| {
                if pole.is_edge_field() {
                    let wartosci: Vec<u16> = (0..road.edge_count())
                        .map(|i| snap.edge_value(pole, i).clamp(0, i64::from(u16::MAX)) as u16)
                        .collect();
                    // Pas ma rząd wielkości jednej komórki, więc krawędź stempluje się
                    // z promieniem 1: cieńsza linia gubi się przy skali całego miasta.
                    rasterize_edges(road, &wartosci, dim, bok, 1)
                } else {
                    rasterize_points(&snap.lots, dim, bok, 1)
                }
            })
        });

    let mut px = vec![18u8; (dim as usize) * (dim as usize) * 3];
    let mut niepustych = 0u32;
    for y in 0..dim as usize {
        for x in 0..dim as usize {
            let v = raster[y * dim as usize + x];
            if v == 0 {
                continue;
            }
            niepustych += 1;
            let c = paleta[spec.index_of(i64::from(v)) as usize];
            // Oś Y obrazu rośnie w dół, oś świata w górę.
            let o = ((dim as usize - 1 - y) * dim as usize + x) * 3;
            px[o..o + 3].copy_from_slice(&c[..3]);
        }
    }
    magnat_devtools::write_rgb(out, dim, dim, &px)?;
    println!(
        "
nakładka {klucz} -> {} ({dim}x{dim}, {niepustych} komórek, minuta {})",
        out.display(),
        world.resource::<TrafficOverlay>().minute()
    );
    println!(
        "  legenda ({}): {}",
        spec.unit,
        spec.legend()
            .iter()
            .map(|(_, v)| v.to_string())
            .collect::<Vec<_>>()
            .join(" · ")
    );
    Ok(())
}

/// Rejestr podróży, która nie weszła na sieć drogową (marsz, rower, komunikacja).
///
/// Zerowe paliwo i zerowy koszt są tu **prawdą**, a nie brakiem danych: pieszy nie pali
/// benzyny, a `settle_edge` nigdy go nie dotknęło. Czas bierze się z decyzji, bo to ona
/// jest dla takiej podróży jedynym źródłem prawdy — i to jest ta sama zasada, co
/// w warstwie Mikro: kto nie wyznacza czasu, ten go odgrywa.
fn pusty_rejestr(minutes: u16, reason: magnat_core::DecisionReason) -> magnat_traffic::TripLedger {
    magnat_traffic::TripLedger {
        arrive: magnat_core::SimMinute(u64::from(minutes)),
        // Powód przebiegu jest ten sam co powód wyboru — karta pokaże go raz, bo drugi
        // wiersz wypisuje wyłącznie wtedy, gdy wnosi coś ponad pierwszy.
        reason,
        ..magnat_traffic::TripLedger::default()
    }
}

/// Karta inspekcji mieszkańca w formie tekstowej — ten sam model, który w kliencie
/// karmi widget (M3d §5.11). Plan odtwarza się z ziarna przez `plan_day_explained`,
/// realizacja pochodzi z bufora śledzenia.
fn karta(
    world: &magnat_ecs::World,
    citizen: magnat_core::CitizenId,
    day: u64,
    seed: u64,
    locale: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ui = UiContext::new(locale.parse()?, Tick(day * 1440))?;
    ui.selection = Selection::Citizen(citizen);
    let mut panel = CitizenPanel { day, seed };
    println!("
── {} ──", panel.title(&ui));
    print!("{}", panel.build(&ui, world));

    // Karta inspekcji podróży (WP11) — pierwszy ekran, na którym mieszkaniec pojawia
    // się graczowi **w zdaniu**, a nie jako wiersz tabeli. Bez niej bramka 5 z §7.4
    // („każda decyzja transportowa ma uzasadnienie widoczne w karcie") byłaby
    // deklaracją: uzasadnienia są liczone, ale nikt ich nie pokazuje.
    let indeks = citizen.entity().index();
    let Some(id) = world
        .get::<magnat_agents::components::Identity>(citizen.entity())
        .copied()
    else {
        return Ok(());
    };
    // Rejestr krawędź po krawędzi ma tylko podróż, która **weszła na sieć** — czyli
    // samochodowa. Kto poszedł pieszo albo pojechał autobusem, ma wyłącznie decyzję,
    // i to jest dokładnie ta połowa karty, która odpowiada na „dlaczego tak".
    // Pusty rejestr jest tu prawdą, a nie brakiem: przejazd sieci się nie odbył.
    let oracle = world
        .resource::<magnat_traffic::TrafficServices>()
        .oracle
        .clone();
    let z_logu = world
        .resource::<magnat_traffic::TripLog>()
        .last_of(indeks)
        .cloned();
    let (origin, dest, decision, ledger, plan_min) = match z_logu {
        Some(r) if r.decision.is_some() => {
            let d = r.decision.clone().expect("sprawdzone wyżej");
            (r.origin.unwrap_or(r.dest), r.dest, d, r.ledger, r.planned_minutes)
        }
        _ => match oracle.last_decision(indeks) {
            Some((from, to, d)) => {
                let (minuty, powod) = (d.minutes, d.reason);
                (from, to, d, pusty_rejestr(minuty, powod), minuty)
            }
            None => return Ok(()),
        },
    };
    let karta = magnat_ui::TripCard::build(
        &ui.catalog,
        ui.locale,
        &magnat_ui::TripView {
            traveller: &id,
            origin,
            dest,
            planned_minutes: plan_min,
            decision: &decision,
            ledger: &ledger,
        },
    );
    println!();
    print!("{}", karta.render_text(&ui.catalog, ui.locale));
    Ok(())
}

/// Histogram wykorzystania doby: ile minut mieszkaniec spędza w której czynności.
///
/// Liczony z **planów**, nie z licznika zdarzeń: plan jest tym, co mieszkaniec
/// zamierzał, a różnicę między zamiarem a wykonaniem pokazuje osobno karta inspekcji
/// (§5.11, ścieżka „realizacja").
fn profil_doby(world: &magnat_ecs::World) {
    let mut minuty = [0u64; ActivityKind::ALL.len()];
    let mut ludzi = 0u64;
    for e in world.resource::<Population>().citizens() {
        let Some(plan) = world.get::<magnat_agents::PlanRef>(*e) else {
            continue;
        };
        ludzi += 1;
        for s in magnat_agents::load_plan(plan, world.resource::<magnat_agents::PlanSlab>()) {
            minuty[s.kind as usize] += u64::from(s.dur_min);
        }
    }
    if ludzi == 0 {
        return;
    }
    let suma: u64 = minuty.iter().sum();
    println!("\nwykorzystanie doby (średnio minut na mieszkańca)");
    for (i, k) in ActivityKind::ALL.iter().enumerate() {
        if minuty[i] == 0 {
            continue;
        }
        println!(
            "  {:<9} {:>7.1} min  {:>5.1} %",
            k.name(),
            minuty[i] as f64 / ludzi as f64,
            minuty[i] as f64 * 100.0 / suma.max(1) as f64
        );
    }
}

/// Średni poziom dwunastu potrzeb — czy doba je zaspokaja, czy miasto głoduje.
fn potrzeby(world: &magnat_ecs::World) {
    let mut suma = [0u64; magnat_core::NEED_COUNT];
    let mut ludzi = 0u64;
    let mut bez_pracy = 0u64;
    let mut idle = 0u64;
    for e in world.resource::<Population>().citizens() {
        let Some(n) = world.get::<Needs>(*e) else {
            continue;
        };
        ludzi += 1;
        for (i, v) in n.level.iter().enumerate() {
            suma[i] += u64::from(*v);
        }
        if world.get::<Employment>(*e).is_some_and(|x| !x.has_job()) {
            bez_pracy += 1;
        }
        if world
            .get::<AgentState>(*e)
            .is_some_and(|a| a.activity == ActivityKind::Idle as u8)
        {
            idle += 1;
        }
    }
    if ludzi == 0 {
        return;
    }
    println!("\npoziomy potrzeb (średnia 0–100)");
    for (i, k) in NeedKind::ALL.iter().enumerate() {
        println!("  {:<12} {:>5.1}", k.name(), suma[i] as f64 / ludzi as f64);
    }
    println!(
        "\nbez pracy: {bez_pracy} ({:.1} %) · bezczynnych w tej minucie: {idle}",
        bez_pracy as f64 * 100.0 / ludzi as f64
    );
    let _ = Tick(0);
}
