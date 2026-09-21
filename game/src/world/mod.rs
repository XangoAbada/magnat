//! Stawianie świata — od parametrów generacji do harmonogramu gotowego do tykania.
//!
//! # Po co ten moduł istnieje
//!
//! Do M8e **żadnego takiego miejsca nie było**. Scenariusz `m8miasto` składał
//! jedenaście kroków ręcznie, klient graficzny składał ich pięć i pomijał firmy,
//! miasto i zdarzenia, a balansator brał jeszcze inny podzbiór. Trzy drogi do
//! świata, z których każda dawała inny świat — i nikt nie miał tego pilnować,
//! bo żaden dokument nie wskazywał właściciela tej drogi.
//!
//! M9 §7 stawia z tego kryterium: „`new_game(NewGameParams)` i `magnat --seed …`
//! dają **identyczny hash terenu i miasta**". Spełnia się je jednym sposobem —
//! jedną funkcją, którą wołają obaj. To jest ta funkcja: [`stand_up`].
//!
//! # Co tu jest, a czego nie ma
//!
//! Tu jest **kolejność i spięcie**, a nie logika. Populację liczy
//! `sim/world::population`, gospodarkę stawia [`retail`], firmy [`firms`],
//! stronę publiczną [`city`], sieci [`grid`], zdarzenia [`events`]. Gdyby
//! którakolwiek z tych rzeczy zaczęła się liczyć tutaj, przestałaby być
//! sprawdzalna osobno.

pub mod city;
pub mod events;
pub mod firms;
pub mod full;
pub mod grid;
pub mod labor;
pub mod plants;
pub mod population;
pub mod retail;

use magnat_agents::{
    bootstrap_day, register_day, society, AgentSources, DayLoopSystem, DeprivationEffectsSystem,
    HouseholdStockSystem, InfinitePlaces, NeedDecaySystem, NeedTable, NoInheritance,
    ReplanCooldownSystem, SkillDriftSystem, SocietySystem, TravelMicroSystem,
};
use magnat_ecs::{App, Schedule, ScheduleBuilder, World};
use magnat_jobs::JobPool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub use population::BuiltCity;

/// Nastawy stawiania świata, które **nie są** parametrami generacji.
///
/// Rozróżnienie jest istotne dla replayu: `WorldGenParams` opisuje świat i jedzie
/// w kopercie `StartGame`, a to tutaj jest konfiguracją przebiegu — ile ludzi
/// zaludnić (0 = z pojemności miasta, czyli tak, jak gra to robi), ile prób
/// zamiany mieszkań wykonać i czy w ogóle stawiać gospodarkę. W grze te wartości
/// są domyślne; różnią się wyłącznie w scenariuszach i testach, więc jadą
/// w **nagłówku dziennika**, a nie w komendzie gracza.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SessionOpts {
    /// Docelowa liczba mieszkańców; 0 = wylicz z pojemności miasta.
    pub citizens: u32,
    /// Ile prób zamiany mieszkań w kroku 7 Etapu 8.
    pub commute_swaps: u32,
    /// Gospodarka, firmy, miasto, sieci i zdarzenia. `false` wraca do świata M3
    /// (atrapa `InfinitePlaces`) — udokumentowana droga wyjścia `--no-economy`.
    pub economy: bool,
    /// Warstwa Mikro ruchu. Potrzebna wyłącznie tam, gdzie ktoś patrzy —
    /// w oknie. Nie ma prawa zapisu do stanu (00 §4), więc nie zmienia wyniku.
    pub micro: bool,
    /// Ile lat historii „na sucho" przepuścić przez świat przed pierwszą dobą
    /// rozgrywki. `0` = żadnych, czyli świat startuje sterylny tak jak do M10f.
    ///
    /// Decyzja `D5` fazy M10 oddaje przepływ startu partii fazie M9, a pomiar
    /// z M10a mówi, ile to kosztuje: trzydzieści lat metropolii to 3300 kroków
    /// i ~0,2 s po wygenerowaniu miasta. Nastawa jedzie w **nagłówku dziennika**
    /// razem z resztą `SessionOpts`, bo zmienia świat, na którym replay stoi.
    #[serde(default)]
    pub dry_run_years: u16,
}

impl Default for SessionOpts {
    fn default() -> SessionOpts {
        SessionOpts {
            citizens: 0,
            commute_swaps: 200_000,
            economy: true,
            micro: false,
            dry_run_years: 0,
        }
    }
}

/// Świat postawiony i gotowy do tykania.
pub struct Standing {
    pub app: App,
    /// Rynek, jeśli gospodarka jest włączona. `Market` jest `Clone` i wewnętrznie
    /// współdzielony, więc sesja trzyma go **obok** świata i czyta bez `&World`.
    pub market: Option<magnat_economy::Market>,
    pub report: StandingReport,
    /// Kronika historii „na sucho", jeśli `SessionOpts::dry_run_years > 0`.
    ///
    /// Most przekazuje ją **dalej**, a nie zapisuje do świata: to jest widok
    /// pochodny, więc nie wchodzi do hasha stanu i nie ma go w ECS. Odbiera ją
    /// `Session::begin` i wkłada do kroniki gracza (`DK-2`).
    pub history: Vec<magnat_macro::ChronicleEvent>,
}

/// Liczby, które most ma powiedzieć po postawieniu świata. Raport tekstowy
/// należy do wołającego — most nie pisze na stderr, bo robi to też w oknie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StandingReport {
    pub citizens: usize,
    pub households: usize,
    pub shops: usize,
    pub plants: usize,
    pub firms: usize,
    pub services: usize,
    pub grids: usize,
    pub events: usize,
    pub systems: usize,
    pub stages: usize,
    pub schedule_fingerprint: u64,
}

/// Stawia świat na gotowym mieście: ludzie, gospodarka, firmy, strona publiczna,
/// sieci, zdarzenia i harmonogram.
///
/// Kolejność jest wymuszona i każdy jej krok ma powód wypisany w mostach:
/// rynek przed firmami (rejestr czyta obsadę), miasto po gospodarce (kataster
/// czyta łańcuch), usługi po mieście (placówka jest finansowana z budżetu),
/// władza po usługach (poparcie stoi na pokryciu), sieci po zakładach (moc
/// bierze się z linii), zdarzenia po sieciach (bramka pyta rejestr sieci).
///
/// # Errors
/// Brak albo niespójny katalog w `data/`, miasto bez mieszkań, cykl w harmonogramie.
pub fn stand_up(
    built: &BuiltCity,
    seed: u64,
    opts: SessionOpts,
    pool: &JobPool,
) -> Result<Standing, Box<dyn std::error::Error>> {
    let city = &built.city;
    let mut world = population::swiat_agentow(seed)?;
    register_day(&mut world);
    let zaludnione = population::zaludnij(&mut world, city, opts.citizens, opts.commute_swaps)?;
    let mut rap = StandingReport {
        citizens: society::population(&world),
        households: society::households(&world),
        ..StandingReport::default()
    };

    let oracle = zaludnione.travel_oracle();
    oracle.set_micro_window(None, 0);

    let market = if opts.economy {
        let f = full::setup(
            &mut world,
            city,
            zaludnione.places.clone(),
            oracle,
            &zaludnione.traffic,
            seed,
            pool,
        )?;
        rap.shops = f.retail.shops;
        rap.plants = f.retail.plants.sites;
        rap.firms = f.firms.firms as usize;
        let m = f.retail.market.clone();

        city::setup(&mut world, city)?;
        let uslugi = city::setup_services(&mut world, city)?;
        rap.services = uslugi.services;
        city::setup_government(&mut world, city, seed)?;
        let sieci = grid::setup(&mut world, city)?;
        rap.grids = sieci.nets;
        let zdarzenia = events::setup(
            &mut world,
            city,
            &built.climate,
            built.city.plan.epoch.year(),
        )?;
        rap.events = zdarzenia.defs;
        Some(m)
    } else {
        let tabela = Arc::new(NeedTable::load_default()?);
        *world.resource_mut::<AgentSources>() = AgentSources::new(
            Box::new(InfinitePlaces::new(zaludnione.places.clone(), tabela)),
            oracle,
        );
        None
    };

    bootstrap_day(&mut world, 0);

    // Historia „na sucho" (M10 §4.2 Etap 9). Stoi **tu**, a nie w generatorze:
    // potrzebuje gospodarki, firm i ludzi, czyli wszystkiego, co powyżej,
    // a jednocześnie musi się wykonać, zanim ruszy pierwszy tick — inaczej
    // przepisywałaby świat, który już żyje.
    let history = if opts.dry_run_years > 0 && market.is_some() {
        let cfg = magnat_macro::DryRunConfig {
            seed,
            years: opts.dry_run_years.clamp(1, 100),
            start_year: i32::from(city.plan.epoch.year()) - i32::from(opts.dry_run_years),
            basket: dry_run_basket(&world),
            ..magnat_macro::DryRunConfig::default()
        };
        magnat_macro::dry_run(&cfg, &mut world, &magnat_macro::MacroParams::default()).chronicle
    } else {
        Vec::new()
    };

    let schedule = zbuduj_harmonogram(&world, opts, market.is_some())?;
    rap.systems = schedule.system_count();
    rap.stages = schedule.stage_count();
    rap.schedule_fingerprint = schedule.fingerprint();

    let app = App::new(world, schedule, pool.thread_count());
    Ok(Standing {
        app,
        market,
        report: rap,
        history,
    })
}

/// Koszyk podstawowy do bramki 8 Etapu 10 — pary (towar, sztuk na miesiąc).
///
/// Buduje go **wołający**, bo `sim/macro` nie ma katalogu towarów i mieć nie
/// powinien. Bierzemy towary o najszerszej dostępności w mieście: koszyk, którego
/// połowy nie da się kupić, mierzyłby nie tyle drożyznę, ile braki w asortymencie.
/// Docelowy koszyk `data/economy/cpi.ron` składa scenariusz gry, nie ta funkcja.
///
/// Adres jest tutaj, a nie w `tools/headless`, z powodu kierunku zależności
/// (`K-68`): przebieg bezgłowy woła `game`, nigdy odwrotnie — a obie drogi mają
/// mierzyć **ten sam** koszyk, inaczej bramka 8 znaczy co innego w każdej z nich.
#[must_use]
pub fn dry_run_basket(world: &World) -> Vec<(magnat_core::GoodId, i64)> {
    /// Ile pozycji ma koszyk.
    const POZYCJI: usize = 12;
    /// Ile sztuk każdej pozycji miesięcznie.
    const SZTUK_MIESIECZNIE: i64 = 30;

    let st = magnat_macro::lift(world);
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
        .take(POZYCJI)
        .map(|(_, g)| (magnat_core::GoodId(g), SZTUK_MIESIECZNIE))
        .collect()
}

/// Hak dziedziczenia zależny od tego, czy świat ma gospodarkę (`R2-WP10`).
fn hak_dziedziczenia(
    world: &World,
    gospodarka: bool,
) -> Box<dyn magnat_agents::demography::InheritanceHook> {
    match world.get_resource::<magnat_economy::Market>() {
        Some(m) if gospodarka => {
            Box::new(magnat_economy::inherit::EconomyInheritance::new(m.clone()))
        }
        _ => Box::new(NoInheritance),
    }
}

/// Harmonogram świata gry. **Jeden dla okna i dla headlessa** — inaczej odcisk
/// harmonogramu (`Schedule::fingerprint`) różniłby się między nimi, a razem z nim
/// kolejność systemów, czyli wynik.
fn zbuduj_harmonogram(
    world: &World,
    opts: SessionOpts,
    gospodarka: bool,
) -> Result<Schedule, Box<dyn std::error::Error>> {
    let mut b = ScheduleBuilder::new();
    if gospodarka {
        b.add(magnat_events::EventSystem::new())
            .add(magnat_traffic::utility::UtilitySystem::new(
                magnat_traffic::utility::GridTuning::load_default()?,
            ))
            .add(magnat_supply::ChainSystem::new())
            .add(magnat_firms::systems::FirmSystem::new())
            .add(magnat_economy::MarketSystem::new(world))
            .add(magnat_economy::labor::LaborSystem::new())
            .add(magnat_economy::corpfin::system::InsolvencySystem::new())
            .add(magnat_macro::MacroSystem::new())
            .add(magnat_media::MediaSystem::new())
            .add(magnat_economy::insurance::system::InsuranceSystem::new())
            .add(magnat_economy::equity::system::EquitySystem::new())
            .add(magnat_economy::RelationsSystem::new())
            .add(magnat_city::CitySystem::new());
    }
    b.add(DayLoopSystem::new(world))
        .add(ReplanCooldownSystem::new(world))
        .add(NeedDecaySystem::new(world))
        .add(DeprivationEffectsSystem::new(world))
        .add(SkillDriftSystem::new(world))
        .add(HouseholdStockSystem::new(world))
        // Hak dziedziczenia: w świecie z gospodarką udziały w firmach i kredyty mają
        // gdzie pójść po śmierci właściciela (`R2-WP10`); bez niej nie ma po czym
        // dziedziczyć i zostaje zaślepka.
        .add(SocietySystem::new(hak_dziedziczenia(world, gospodarka)))
        // Warstwa mezo musi tu być, i to nie dla widoku. Zlecenie przejazdu
        // zebrane przez `begin_trip` wykonuje **tylko** ten system; bez niego
        // kierowca zgłasza podróż, której nikt nie realizuje.
        .add(magnat_traffic::TrafficSystem::new(world))
        .add(magnat_traffic::VehicleWearSystem::new(world));
    if opts.micro {
        b.add(TravelMicroSystem::new(world));
    }
    Ok(b.build()?)
}
