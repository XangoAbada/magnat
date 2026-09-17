//! Most „generator zdarzeń": katalog, normy klimatyczne i wycinek miasta
//! w świecie, który ma już gospodarkę i sieci (M8c WP4–WP6).
//!
//! # Czego tu nie ma
//!
//! Ani jednej reguły hazardu. Wszystko, co liczy szansę, siłę i skutek, mieszka
//! w `magnat_events`; ten plik wyłącznie **wiąże**: normy klimatu z pogodą,
//! zakład gospodarki z rodzajem („pole czy fabryka"), a klucz towaru z katalogiem
//! tego miasta.
//!
//! # Dlaczego wycinek miasta powstaje raz
//!
//! Dzielnica zakładu i rodzaj jego receptury nie zmieniają się w trakcie gry,
//! a policzenie ich wymaga parcel, stref i katalogu receptur — czyli `sim/world`
//! i całego generatora. Gdyby generator zdarzeń miał je liczyć sam, musiałby od nich
//! zależeć; tak zależy wyłącznie od `core`, `ecs` i czterech crate'ów, których
//! liczby naprawdę czyta. To ta sama droga, którą `AgentSources` zdejmuje
//! zależność od miasta z M3.

use std::error::Error;

use magnat_core::{GoodId, SiteId, TariffClassId};
use magnat_ecs::World;
use magnat_events::{
    register_events, ClimateNorms, EpochClock, EventCatalog, Events, SiteClass, SiteRef,
    WeatherState,
};
use magnat_supply::catalog::RecipeSource;
use magnat_supply::ChainHandle;
use magnat_world::{CityData, ClimateCell};

/// Co most postawił — liczby do raportu scenariusza.
pub struct EventsSetup {
    pub defs: usize,
    pub sites: usize,
    pub farms: usize,
    pub districts: u16,
    /// Średnia roczna temperatura norm miasta, w dziesiątych °C — kotwica, wokół
    /// której krąży pogoda. Raport pokazuje ją obok zmierzonej średniej przebiegu,
    /// bo kryterium WP6 mówi „±0,5 °C" i bez obu liczb nie da się tego sprawdzić.
    pub mean_temp_dc: i16,
    pub annual_precip_mm: u32,
}

/// Stawia generator zdarzeń w gotowym świecie.
///
/// Kolejność jest wymuszona: **zakłady i sieci muszą już stać**, bo wycinek miasta
/// bierze się z areny zakładów, a bramka „sieć ma czynne źródło" — z rejestru sieci.
///
/// # Errors
/// Zwraca błąd, gdy katalogu `data/events/` nie da się wczytać albo gdy któraś
/// definicja wskazuje towar lub klasę taryfową spoza katalogu tego miasta.
/// To **ma** być błąd, a nie ciche pominięcie: zdarzenie bez skutku wygląda
/// w raporcie tak samo jak zdarzenie, które działa.
pub fn setup(
    world: &mut World,
    city: &CityData,
    klimat: &ClimateCell,
    start_year: u16,
) -> Result<EventsSetup, Box<dyn Error>> {
    let catalog = EventCatalog::load_default()?;
    let chain = world
        .get_resource::<ChainHandle>()
        .cloned()
        .ok_or("świat bez łańcucha dostaw — generator zdarzeń nie ma czego obserwować")?;

    let normy = ClimateNorms {
        temp_monthly_dc: klimat.temp_monthly_dc,
        precip_monthly_mm: klimat.precip_monthly_mm,
    };
    let mut ev = Events::new(
        catalog,
        WeatherState::new(normy),
        EpochClock::new(start_year),
    );

    let zaklady = wycinek_miasta(&chain, city);
    let farms = zaklady.iter().filter(|s| s.kind == SiteClass::Farm).count();
    let dzielnic = u16::try_from(city.districts.districts.len().max(1)).unwrap_or(1);
    let raport = EventsSetup {
        defs: ev.catalog().defs.len(),
        sites: zaklady.len(),
        farms,
        districts: dzielnic,
        mean_temp_dc: klimat.mean_annual_temp_dc(),
        annual_precip_mm: klimat.annual_precip_mm(),
    };
    ev.set_sites(zaklady, dzielnic);

    // Klucze tekstowe na identyfikatory **tego** miasta. Rozwiązanie raz, przy
    // stawianiu świata: nieistniejący klucz ma pęknąć teraz, a nie objawić się
    // za dziesięć lat gry jako zdarzenie, które niczego nie zmieniło.
    let c = chain.clone();
    let good_of = move |k: &str| -> Option<GoodId> {
        c.cat
            .goods
            .iter()
            .position(|g| &*g.key == k)
            .and_then(|i| u16::try_from(i).ok())
            .map(GoodId)
    };
    let c2 = chain.clone();
    let class_of = move |k: &str| -> Option<TariffClassId> { c2.lock().b2b.tariff_class_of(k) };
    ev.resolve(&good_of, &class_of)?;

    register_events(world, ev);
    Ok(raport)
}

/// Zakłady miasta z dzielnicą i rodzajem. Kolejność rosnąca po identyfikatorze
/// zakładu — po niej idą instancje zakresu `Site`, a po nich klucze losowań.
fn wycinek_miasta(chain: &ChainHandle, city: &CityData) -> Vec<SiteRef> {
    let mut out = Vec::new();
    let c = chain.lock();
    for (i, s) in city.sites.sites.iter().enumerate() {
        let site: SiteId = crate::world::plants::site_id(i);
        let dzielnica = city
            .parcels
            .parcels
            .get(s.parcel.0.index() as usize)
            .map_or(0, |p| p.district.0);
        let kind = match c.plant.get(site) {
            // Zakład produkcyjny: pole albo fabryka, w zależności od tego, skąd
            // bierze masę. `Agriculture` znaczy „z gleby", a susza zabiera plon
            // wyłącznie takiemu zakładowi.
            Some(z) => {
                let rolny = z.lines.iter().any(|l| {
                    l.recipe
                        .is_some_and(|r| chain.cat.recipe(r).source == RecipeSource::Agriculture)
                });
                if rolny {
                    SiteClass::Farm
                } else {
                    SiteClass::Industry
                }
            }
            // Zakład bez linii produkcyjnych to sklep, biuro albo usługa —
            // i dla zdarzeń jest to jedna klasa, bo żadne z nich nie ma ani pola,
            // ani maszyny.
            None => SiteClass::Service,
        };
        out.push(SiteRef {
            site,
            district: dzielnica,
            kind,
        });
    }
    out
}
