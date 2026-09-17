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
}

impl Default for SessionOpts {
    fn default() -> SessionOpts {
        SessionOpts {
            citizens: 0,
            commute_swaps: 200_000,
            economy: true,
            micro: false,
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
    let schedule = zbuduj_harmonogram(&world, opts, market.is_some())?;
    rap.systems = schedule.system_count();
    rap.stages = schedule.stage_count();
    rap.schedule_fingerprint = schedule.fingerprint();

    let app = App::new(world, schedule, pool.thread_count());
    Ok(Standing {
        app,
        market,
        report: rap,
    })
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
            .add(magnat_city::CitySystem::new());
    }
    b.add(DayLoopSystem::new(world))
        .add(ReplanCooldownSystem::new(world))
        .add(NeedDecaySystem::new(world))
        .add(DeprivationEffectsSystem::new(world))
        .add(SkillDriftSystem::new(world))
        .add(HouseholdStockSystem::new(world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
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
