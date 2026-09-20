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
//! przychodzi z [`crate::world::retail::setup`] bez zmian; ten moduł dokłada **warstwę
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

use crate::world::{firms, retail};

/// Miasto z pełną gospodarką i pełną warstwą firm.
pub struct FullCity {
    pub retail: retail::Retail,
    pub firms: firms::FirmsReport,
    /// Suma etatów zamówionych przez zakłady — mianownik obsady.
    pub slots: u32,
    /// Ile sieci zewnętrznych czeka w katalogu.
    pub chains: usize,
    /// Ile zakładów ubezpieczeń dostało rachunek (M10d).
    pub insurers: u32,
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
    // Drzewo technologii wczytuje się **przed** rynkiem i to jest kolejność, nie
    // przypadek: blokada towarów musi stanąć zanim generator obsadzi sklepy
    // asortymentem. Rok startowy partii wchodzi tutaj i nigdzie indziej —
    // `TechTree::load` zamienia rok „światowy" każdego węzła na dobę świata,
    // więc symulacja nigdy nie pyta o kalendarz i nie ma drugiego źródła roku
    // obok `EpochClock` z M8.
    let goods = magnat_supply::catalog::load_default("contemporary")?;
    let rnd = magnat_firms::load_rnd_default(i32::from(city.plan.epoch.year()), &goods)?;
    let zablokowane = magnat_firms::gated_goods(&rnd.tree);
    let r = retail::setup(
        world,
        city,
        places,
        travel,
        traffic,
        seed,
        pool,
        &zablokowane,
    )?;

    let roles = RoleTable::load_default()?;
    let types = katalog_typow(city, &roles, &goods)?;
    let (mut firmy, report) = firms::zbuduj_firmy(city, world, &types);
    let slots = firmy.sites().map(|(_, s)| s.required_slots()).sum();
    // Firmy zastane w mieście nie mają dyrektora-mieszkańca (`Owner::External`),
    // więc cechy biorą się z klucza i ziarna świata. Firmy zakładane w trakcie gry
    // dostają cechy swojego założyciela — tą samą funkcją.
    firmy.refresh_personalities(seed, |_| None);
    world.insert_resource(rnd);
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

    // Kampanie reklamowe i tytuły medialne (M10b WP10.6, WP10.7). Rejestr jest pusty
    // do pierwszej kampanii i do pierwszej redakcji — świat bez reklamy nie płaci
    // za ten mechanizm ani bajtem hasha poza dwiema zerowymi długościami.
    // Strojenie marki i kanałów **z danych**, nie z `Default` (`K-35`): bez tego
    // `data/tuning/brand.ron` byłby plikiem, którego nikt nie czyta, a walidacje
    // `schema_version` i asymetrii `k_down > k_up` — martwe.
    world.insert_resource(magnat_agents::BrandData::load_default()?);
    magnat_media::register_media(world);
    magnat_media::ai::stand_up_outlets(world);

    // Giełda i ubezpieczenia (M10d). Oba rejestry są puste do pierwszego debiutu
    // i do pierwszej polisy, więc świat bez rynku kapitałowego nie płaci za nie
    // ani bajtem hasha poza zerowymi długościami. Kalibracja **z danych**
    // (`data/tuning/insurance.ron`, `K-35`), a nie z `Default` — ten sam wniosek,
    // co przy `brand.ron` w M10b: ładowarka, której nikt nie woła, ukrywa
    // niepoprawny plik do czasu, aż ktoś go otworzy.
    let strojenie = magnat_economy::insurance::data::M10dTuning::load_default()?;
    magnat_economy::equity::system::register_equity(
        world,
        magnat_economy::equity::Equity::new(strojenie.equity),
    );
    magnat_economy::insurance::system::register_insurers(
        world,
        magnat_economy::insurance::Insurers::new(strojenie.insurance),
    );
    // Rachunki zakładom ubezpieczeń. Konta otwierają dziś tylko sklepy i zakłady
    // z linią produkcyjną, a biuro nie jest ani jednym, ani drugim — bez tego kroku
    // polisy powstają, ale ani składka, ani odszkodowanie nie ruszają grosza.
    let ubezpieczycieli = magnat_economy::insurance::system::stand_up_insurers(world);

    // Relacje z dostawcami, zmowy cenowe i związki zawodowe (M10e). Wszystkie trzy
    // rejestry są puste do pierwszej dostawy, pierwszej zmowy i pierwszego sporu,
    // więc świat bez nich nie płaci ani bajtem hasha poza zerowymi długościami.
    // Kalibracja **z danych** (`data/tuning/relations.ron`, `K-88`) — ten sam
    // wniosek co przy `brand.ron` i `insurance.ron`: ładowarka, której nikt nie woła,
    // ukrywa niepoprawny plik do czasu, aż ktoś go otworzy. Wołanie jest **po**
    // rynku, bo strojenie zaufania zjeżdża stąd do `sim/supply` przez `Market`.
    magnat_economy::register_relations(world, magnat_economy::RelationsTuning::load_default()?);

    Ok(FullCity {
        retail: r,
        firms: report,
        slots,
        chains: ile,
        insurers: ubezpieczycieli,
    })
}

/// Katalog typów zakładów dla tego miasta. Wymaga receptur archetypów, bo walidator
/// sprawdza, czy receptura z `data/site_types/` w ogóle istnieje w katalogu towarów.
fn katalog_typow(
    city: &CityData,
    roles: &RoleTable,
    goods: &magnat_supply::Catalog,
) -> Result<SiteTypeCatalog, Box<dyn Error>> {
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
    Ok(SiteTypeCatalog::load_default(roles, goods, &archetypy)?)
}
