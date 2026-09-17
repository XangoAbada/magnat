//! System ECS generatora zdarzeń i pogody (M8c WP4/WP6).
//!
//! Jeden system, nie trzy. Pogoda, hazardy i wskaźniki dzielą ten sam zasób,
//! ten sam moment w ticku i tę samą kolejność wobec reszty świata — a dwa systemy
//! wyłączne o wymuszonej kolejności to `K-53` bez potrzeby (ta sama korekta,
//! którą M8a zrobiła tabeli systemów podatkowych w `CB-2`).
//!
//! **Kolejność w ticku.** System otwiera tick (`opens_tick`, `K-53`) i stoi
//! **po** `traffic.Utility`. Powód jest mierzalny: sonda obciążenia sieci ma
//! opisywać ten tick, a nie poprzedni, a wyłączenie bloku ma zgasić dzielnicę
//! w tym samym ticku, w którym zapadło — inaczej blackout spóźniałby się o minutę,
//! a T2 mierzy „wolumen produkcji w oknie awarii dokładnie 0".

use crate::apply;
use crate::eval;
use crate::indicators::CityIndicators;
use crate::registry::{EventCause, Events, WorldEvent};
use magnat_core::{Cadence, Mood, Tick};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};
use magnat_traffic::utility::UtilityGrids;

const MINUTES_PER_DAY: u64 = 1440;
const DAYS_PER_MONTH: u64 = 30;

/// Generator zdarzeń, pogoda i wskaźniki miasta.
pub struct EventSystem {
    desc: SystemDesc,
}

impl EventSystem {
    #[must_use]
    pub fn new() -> EventSystem {
        EventSystem {
            desc: SystemDesc::new("events.Event", Cadence::EveryHour)
                .exclusive()
                .opens_tick()
                .after_if_present(SystemId::from_name("traffic.Utility")),
        }
    }
}

impl Default for EventSystem {
    fn default() -> EventSystem {
        EventSystem::new()
    }
}

impl System for EventSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        let seed = ctx.world().seed;
        // Zdarzenia wychodzą ze świata na czas kroku — tak samo jak miasto
        // w `city.Tax` i finanse firm w `economy.Market`. Dwóch pożyczek
        // `&mut World` naraz nie ma.
        let Some(mut ev) = ctx
            .world_mut()
            .get_resource_mut::<Events>()
            .map(std::mem::take)
        else {
            return;
        };
        krok(&mut ev, ctx.world_mut(), t, seed);
        if let Some(slot) = ctx.world_mut().get_resource_mut::<Events>() {
            *slot = ev;
        }
    }
}

/// Jeden krok godzinowy. Wyjęty z systemu, żeby test mógł go zawołać bez schedulera.
pub fn krok(ev: &mut Events, world: &mut World, t: Tick, seed: u64) {
    let day = t.0 / MINUTES_PER_DAY;
    let hour = u32::try_from(t.0 % MINUTES_PER_DAY / 60).unwrap_or(0);
    let poczatek_doby = hour == 0;

    // 1. Pogoda. Zawsze pierwsza, bo jest sondą dla wszystkiego poniżej.
    ev.weather_mut().step(seed, day, hour);
    rozeslij_pogode(ev, world);
    ev.epoch_mut().set_day(day);

    // 2. Wskaźniki miasta — raz na miesiąc gry, bo spis nastroju jest przejściem
    //    po całej populacji, a statystyka publiczna i tak liczy miesięcznie.
    if poczatek_doby && day.is_multiple_of(DAYS_PER_MONTH) {
        zmierz_miasto(ev, world, day);
    }

    // 3. Wygaszanie. Przed oceną, żeby zwolnione miejsce (`max_concurrent`)
    //    było dostępne jeszcze w tej dobie — inaczej definicja o jednej instancji
    //    miałaby dobę przerwy po każdym zdarzeniu, której nikt nie zamawiał.
    if poczatek_doby {
        // Odpowiedzi indeksowane **numerem zdarzenia**, a nie pozycją w liście.
        // Pozycja nie wystarcza, bo `close_expired` pyta wyłącznie o zdarzenia
        // o końcu warunkowym — licznik przesuwałby się wtedy o jeden, a tablica
        // o tyle pozycji, ile zdarzeń w ogóle trwa, i odpowiedzi rozjechałyby się
        // z pytaniami przy pierwszym zdarzeniu o stałym czasie w rejestrze.
        let trzyma = warunki_konca(ev, world, t);
        ev.close_expired(day, t, |e| {
            trzyma
                .binary_search_by_key(&e.id.0, |(id, _)| *id)
                .is_ok_and(|i| trzyma[i].1)
        });
    }

    // 4. Ocena: co godzinę awarie, raz na dobę reszta.
    oceny(ev, world, t, seed, true);
    if poczatek_doby {
        oceny(ev, world, t, seed, false);
    }

    // 5. Nakładka na świat. Zawsze, bo zbiór aktywnych zdarzeń mógł się zmienić
    //    w obie strony — i dlatego, że jej krok jest pusty, gdy się nie zmienił.
    apply::apply(ev, world);
}

/// Pogoda w popycie na media i w wyborze środka transportu.
///
/// Dwa odbiory, obydwa **pisane**, a nie czytane: sieć dostaje mnożnik popytu,
/// wyrocznia ruchu — pogodę doby. Gdyby jedno i drugie czytało `sim/events`,
/// `traffic → events → traffic` nie zbudowałoby się.
fn rozeslij_pogode(ev: &Events, world: &mut World) {
    let w = ev.weather().weather();
    if let Some(ts) = world.get_resource::<magnat_traffic::systems::TrafficServices>() {
        ts.oracle.set_weather(w);
    }
    let mnozniki: Vec<(usize, u32)> =
        world
            .get_resource::<UtilityGrids>()
            .map_or_else(Vec::new, |g| {
                g.nets()
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (i, ev.weather().demand_bps(n.service)))
                    .collect()
            });
    if let Some(g) = world.get_resource_mut::<UtilityGrids>() {
        for (i, bps) in mnozniki {
            if let Some(n) = g.nets_mut().get_mut(i) {
                n.weather_bps = bps;
            }
        }
    }
}

/// Miesięczny odczyt wskaźników miasta i wyprowadzenie z nich parametrów demografii.
fn zmierz_miasto(ev: &mut Events, world: &mut World, day: u64) {
    let (cpi_index, cpi_yoy, kredyt) =
        world
            .get_resource::<magnat_economy::Market>()
            .map_or((10_000, None, 0), |m| {
                (
                    m.cpi_index_bp(),
                    m.cpi_yoy_bp(),
                    m.credit_outstanding().get(),
                )
            });
    let bezrobocie = world
        .get_resource::<magnat_economy::LaborHandle>()
        .and_then(|h| h.get().map(|m| m.last_day().unemployment_permille()))
        .unwrap_or(0);
    let nastroj = sredni_nastroj(world);
    let miesiac = u32::try_from(day / DAYS_PER_MONTH).unwrap_or(0);
    ev.indicators_mut().close_month(
        miesiac,
        cpi_index,
        cpi_yoy,
        u32::from(bezrobocie),
        kredyt,
        nastroj,
    );
    let temp = ev.weather().weather().temp_dc;
    let p = ev.indicators().demography(temp);
    if let Some(slot) = world.get_resource_mut::<magnat_agents::DemographyParams>() {
        *slot = p;
    }
}

/// Średni nastrój żywych mieszkańców. Spis, nie zapytanie ECS — kolejność spisu
/// jest ustalona, a kolejność archetypów nie (00 §3.2).
fn sredni_nastroj(world: &World) -> Mood {
    let Some(pop) = world.get_resource::<magnat_agents::demography::Population>() else {
        return Mood::NEUTRAL;
    };
    let mut suma: i64 = 0;
    let mut ile: i64 = 0;
    for e in pop.citizens() {
        if let Some(v) = world.get::<magnat_agents::Vitals>(*e) {
            suma += i64::from(v.mood);
            ile += 1;
        }
    }
    if ile == 0 {
        Mood::NEUTRAL
    } else {
        Mood::new(i8::try_from(suma / ile).unwrap_or(0))
    }
}

/// Czy stan świata wciąż trzyma każde z aktywnych zdarzeń o końcu warunkowym.
///
/// Zwraca pary `(numer zdarzenia, trzyma)` posortowane po numerze — numery rosną
/// monotonicznie, więc lista aktywnych jest już posortowana i wyszukiwanie binarne
/// po niej jest poprawne bez dodatkowego sortowania.
fn warunki_konca(ev: &Events, world: &World, t: Tick) -> Vec<(u32, bool)> {
    use crate::catalog::DurationSpec;
    let mut pw = eval::ProbeWorld::new(world, ev, t);
    let mut out: Vec<(u32, bool)> = ev
        .active()
        .iter()
        .map(|e| {
            let trzyma = match ev.def(e.def).map(|d| d.duration.clone()) {
                Some(DurationSpec::UntilBelow { probe, value, .. }) => {
                    pw.value(probe, e.scope) >= value
                }
                // Warianty o znanym czasie rozstrzyga doba, nie sonda — ta funkcja
                // ich nie dotyczy i musi odpowiedzieć „trzyma", żeby nie zamknąć
                // ich przedwcześnie drugą ścieżką.
                Some(_) => true,
                None => false,
            };
            (e.id.0, trzyma)
        })
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// Ocena definicji o zadanej częstotliwości i otwarcie tego, co zaszło.
fn oceny(ev: &mut Events, world: &mut World, t: Tick, seed: u64, hourly: bool) {
    let day = t.0 / MINUTES_PER_DAY;
    let (zaszly, diag) = eval::evaluate(ev, world, t, hourly, seed);
    for (idx, kand, odsiane, slad) in diag {
        if let Some(d) = ev.diagnosis_mut(idx) {
            d.candidates = kand;
            d.gated_out = odsiane;
            d.best_ppm = slad.ppm;
            d.best_factors = slad.factors;
        }
    }
    for f in zaszly {
        let Some(d) = ev.def(f.def).cloned() else {
            continue;
        };
        let patches = {
            let firms = world.get_resource::<magnat_firms::Firms>();
            let grids = world.get_resource::<UtilityGrids>();
            apply::expand(ev, f.def, f.scope, f.severity_bps, firms, grids)
        };
        let dni = crate::hazard::duration_days(&d.duration, seed, f.def, f.scope.key(), t);
        let (ends, min_end) = match d.duration {
            crate::catalog::DurationSpec::UntilBelow { min_days, .. } => {
                (None, day + u64::from(min_days.max(1)))
            }
            _ => (Some(day + u64::from(dni)), day),
        };
        ev.open(
            WorldEvent {
                id: magnat_core::EventId(0),
                def: f.def,
                scope: f.scope,
                started_day: day,
                ends_day: ends,
                min_end_day: min_end,
                severity_bps: f.severity_bps,
                cause: EventCause::Hazard,
                patches,
            },
            t,
        );
    }
}

/// Wstawia rejestr do świata i wpina go w hash stanu.
pub fn register_events(world: &mut World, events: Events) {
    world.insert_resource(events);
    world.register_resource_hash::<Events>();
    // Parametry, które zdarzenia zapisują mieszkańcom, muszą istnieć **zanim**
    // pierwsze zdarzenie zajdzie — inaczej patch trafiłby w nieobecny zasób
    // i wygasłby bez śladu.
    magnat_agents::register_world_params(world);
}

/// Podgląd wskaźników — dla raportu scenariusza i dla inspektora.
#[must_use]
pub fn indicators(world: &World) -> CityIndicators {
    world
        .get_resource::<Events>()
        .map_or_else(CityIndicators::default, |e| *e.indicators())
}
