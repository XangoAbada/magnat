//! Most „miasto → rynek pracy" (M7b): miasto, mieszkańcy, firmy i rynek w jednym świecie.
//!
//! Rusztowanie, nie scenariusz — w bibliotece, bo ma **dwóch konsumentów**: scenariusz
//! `m7labor` i test integracyjny `labor_city.rs` (`K-32`). Kopia rusztowania rozjechałaby
//! się z oryginałem, a rozjazd byłoby widać dopiero jako inny wynik bramki.
//!
//! Czego tu **nie ma**: rynku detalicznego i łańcucha dostaw. Rynek pracy ich nie
//! potrzebuje, a doba miasta z pełną gospodarką kosztuje kilkanaście sekund — przebieg
//! dziewięćdziesięciu dób byłby wtedy liczony w godzinach. Pełne miasto ze wszystkim
//! naraz stawia `m5shop`, a złożenie jednego i drugiego należy do M7f.

use std::error::Error;

use magnat_economy::labor::{register_labor, LaborMarket};
use magnat_ecs::World;
use magnat_firms::{LaborTuning, RoleTable, SiteTypeCatalog};
use magnat_jobs::JobPool;
use magnat_world::CityData;
use std::collections::BTreeMap;

use crate::{firms, population};

/// Miasto gotowe do przebiegu rynku pracy.
pub struct LaborCity {
    pub city: CityData,
    pub world: World,
    pub report: firms::FirmsReport,
    /// Ile etatów w sumie mają zakłady — mianownik, wobec którego czyta się obsadę.
    pub slots: u32,
}

/// Buduje miasto, zaludnia je, stawia firmy i wpina rynek pracy.
///
/// `citizens = 0` znaczy „tylu, ilu mieści miasto" — i to jest wartość, przy której
/// stopa bezrobocia w ogóle coś znaczy: przy garstce mieszkańców etatów jest wielokrotnie
/// więcej niż ludzi i każdy ma pracę z definicji.
pub fn setup(
    seed: u64,
    size: &str,
    region: &str,
    epoch: &str,
    profile: &str,
    citizens: u32,
    pool: &JobPool,
) -> Result<LaborCity, Box<dyn Error>> {
    let city = population::zbuduj_miasto(seed, size, region, epoch, profile, pool)?;
    let mut world = population::swiat_agentow(seed)?;
    population::zaludnij(&mut world, &city, citizens, 0)?;

    let roles = RoleTable::load_default()?;
    let goods = magnat_supply::catalog::load_default("contemporary")?;
    let archetypy: BTreeMap<String, Vec<String>> = city
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
    let types = SiteTypeCatalog::load_default(&roles, &goods, &archetypy)?;

    let (firmy, report) = firms::zbuduj_firmy(&city, &mut world, &types);
    let slots = firmy.sites().map(|(_, s)| s.required_slots()).sum();
    magnat_firms::systems::register_firms(&mut world, firmy);
    world.insert_resource(magnat_firms::systems::PayrollOutbox::default());
    register_labor(
        &mut world,
        LaborMarket::new(LaborTuning::load_default()?, roles),
    );

    Ok(LaborCity {
        city,
        world,
        report,
        slots,
    })
}
