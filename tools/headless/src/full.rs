//! Most „pełne miasto": rynek detaliczny, produkcja, rynek pracy, firmy AI i makro
//! w jednym świecie (M7f WP17).
//!
//! # Po co to istnieje
//!
//! Do M7f żaden przebieg headless nie stawiał `Firms` i `Market` naraz: `m5shop`
//! budował rynek **bez** rejestru firm, `m7labor` rejestr **bez** rynku. Skutek był
//! taki, że polityki (M7c) i AI firm (M7e) wykonywały się wyłącznie w testach —
//! artefakt fazy z §1 nie miał gdzie zaistnieć.
//!
//! Zszycie jest jednym wywołaniem, ale **zmienia świat, na którym stoją skalibrowane
//! bramki G1–G9**: dochodzą firmy, które zatrudniają, płacą i bankrutują. Dlatego
//! most stoi w bibliotece (`K-32`), a nie w scenariuszu — balansator ma mierzyć
//! dokładnie to miasto, które widzi gracz, a nie jego kopię.
//!
//! # Czego tu nie ma
//!
//! Nie ma własnej gospodarki. Wszystko, co dotyczy rynku i łańcuchu dostaw,
//! przychodzi z [`crate::retail::setup`] bez zmian; ten moduł dokłada **warstwę
//! zarządczą** i zegar makro.

use std::error::Error;
use std::sync::Arc;

use magnat_agents::{PlaceTable, TravelOracle};
use magnat_economy::firmlife::{ChainCatalog, FirmLifeLog, FoundingParams};
use magnat_economy::labor::{register_labor, LaborMarket};
use magnat_ecs::World;
use magnat_firms::{LaborTuning, RoleTable, SiteTypeCatalog};
use magnat_jobs::JobPool;
use magnat_traffic::TrafficOracle;
use magnat_world::CityData;

use crate::{firms, retail};

/// Miasto z pełną gospodarką i pełną warstwą firm.
pub struct FullCity {
    pub retail: retail::Retail,
    pub firms: firms::FirmsReport,
    /// Suma etatów zamówionych przez zakłady — mianownik obsady.
    pub slots: u32,
    /// Ile sieci zewnętrznych czeka w katalogu.
    pub chains: usize,
}

/// Stawia gospodarkę i warstwę firm w tym samym świecie.
///
/// Kolejność jest wymuszona i warto ją nazwać: **najpierw rynek, potem firmy**.
/// Rejestr firm czyta obsadę z komponentów mieszkańców i wiąże ją z zakładami
/// kluczem przesuniętym o `SITE_KEY_BASE` (`K-46`) — a te same klucze nadaje
/// sklepom i zakładom produkcyjnym most detaliczny. Odwrotna kolejność dałaby
/// firmy wiążące się z zakładami, których jeszcze nie ma.
///
/// # Errors
/// Zwraca błąd, gdy któregoś z katalogów danych nie da się wczytać.
pub fn setup(
    world: &mut World,
    city: &CityData,
    places: Arc<PlaceTable>,
    travel: Box<dyn TravelOracle>,
    traffic: &Arc<TrafficOracle>,
    seed: u64,
    pool: &JobPool,
) -> Result<FullCity, Box<dyn Error>> {
    let r = retail::setup(world, city, places, travel, traffic, seed, pool)?;

    let roles = RoleTable::load_default()?;
    let types = katalog_typow(city, &roles)?;
    let (mut firmy, report) = firms::zbuduj_firmy(city, world, &types);
    let slots = firmy.sites().map(|(_, s)| s.required_slots()).sum();
    // Firmy zastane w mieście nie mają dyrektora-mieszkańca (`Owner::External`),
    // więc cechy biorą się z klucza i ziarna świata. Firmy zakładane w trakcie gry
    // dostają cechy swojego założyciela — tą samą funkcją.
    firmy.refresh_personalities(seed, |_| None);
    magnat_firms::systems::register_firms(world, firmy);
    world.insert_resource(magnat_firms::systems::PayrollOutbox::default());
    world.insert_resource(magnat_firms::systems::DecisionOutbox::default());
    register_labor(world, LaborMarket::new(LaborTuning::load_default()?, roles));

    // Katalog typów zakładów wchodzi do świata jako **zasób**, a nie parametr mostu:
    // od M7f czyta go też powstawanie firm w trakcie gry, a nie tylko most startowy.
    // Poza hashem stanu — to są dane z `data/site_types/`, tak samo jak presety.
    world.insert_resource(types);
    let chains = ChainCatalog::load_default()?;
    let ile = chains.chains.len();
    world.insert_resource(chains);
    world.insert_resource(FoundingParams::default());
    world.insert_resource(FirmLifeLog::default());

    // Niewypłacalność i upadłość (M7d). Do M7f `CorpFinance` nie stał w żadnym
    // przebiegu headless — mechanizm był, ale nikt go nie wołał, więc firma z pustym
    // rachunkiem mogła w mieście stać w nieskończoność. Bramka §7.10 („≥ 1 bankructwo
    // na 100 firm rocznie") nie miała jak zapalić się ani na zielono, ani na czerwono.
    magnat_economy::corpfin::system::register_corpfin(
        world,
        magnat_economy::corpfin::CorpFinance::new(
            magnat_economy::corpfin::params::InsolvencyParams::load_default()?,
        ),
    );

    // Model makro i tablica uporządkowań wariantów (M7f WP13). Oba wchodzą do hasha.
    magnat_macro::register_macro(world);

    Ok(FullCity {
        retail: r,
        firms: report,
        slots,
        chains: ile,
    })
}

/// Katalog typów zakładów dla tego miasta. Wymaga receptur archetypów, bo walidator
/// sprawdza, czy receptura z `data/site_types/` w ogóle istnieje w katalogu towarów.
fn katalog_typow(city: &CityData, roles: &RoleTable) -> Result<SiteTypeCatalog, Box<dyn Error>> {
    let goods = magnat_supply::catalog::load_default("contemporary")?;
    let archetypy: std::collections::BTreeMap<String, Vec<String>> = city
        .site_catalog
        .archetypes
        .iter()
        .map(|a| {
            (
                a.key().to_owned(),
                a.recipes
                    .iter()
                    .map(|r| goods.recipe(*r).key.to_string())
                    .collect(),
            )
        })
        .collect();
    Ok(SiteTypeCatalog::load_default(roles, &goods, &archetypy)?)
}
