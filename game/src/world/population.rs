//! Most „parametry świata → stojące miasto z ludźmi" (M2 + M3, przeniesiony w M9a).
//!
//! Do M8e mieszkał w `tools/headless` i to było odwrócenie zależności: narzędzie
//! testowe było właścicielem drogi, którą powstaje świat, a gra tej drogi nie miała
//! wcale. M9a odwraca to z powrotem — mosty są w `game/`, a scenariusze headlessa
//! stały się ich konsumentem. Nazwy funkcji zostają bez zmian, bo cytuje je
//! kilkanaście scenariuszy i testów.
//!
//! Wejściem jest [`WorldGenParams`] — **ten sam typ, który dostaje generator
//! z wiersza poleceń i który siedzi w zapisie gry** (M1 §5.5). Warianty
//! przyjmujące napisy zostają dla scenariuszy, ale liczą to samo, bo sprowadzają
//! się do tej samej funkcji: dwie drogi do świata rozjechałyby się przy pierwszej
//! zmianie parametru (M9 §7, „jedna droga do świata").

use magnat_agents::{register, society, DemographyTable, NeedTable};
use magnat_ecs::World;
use magnat_jobs::JobPool;
use magnat_voxel::MaterialRegistry;
use magnat_world::{
    generate_city, generate_population, CityData, CityPlan, Difficulty, Populated,
    PopulationParams, Terrain, WorldGenParams,
};
use std::sync::Arc;

use crate::shell::GenWatch;

/// Miasto razem z terenem, na którym stanęło.
///
/// Teren jest tu, a nie wyrzucony za burtę, bo podgląd świata (`WorldPreview`,
/// §5.13) rysuje z niego miniaturę, a klient graficzny materializuje z niego
/// chunki. Do M8e każdy wołający generował go sobie sam i wyrzucał — łącznie
/// z klientem, który potem generował go drugi raz.
pub struct BuiltCity {
    /// `Arc`, bo teren czyta jednocześnie strumieniowanie chunków w kliencie
    /// i zapytania generatora — a kopia znaczyłaby gigabajty.
    pub terrain: Arc<Terrain>,
    pub city: Arc<CityData>,
    /// Normy klimatyczne środka miasta — wejście pogody M8c.
    pub climate: magnat_world::ClimateCell,
    pub report: magnat_world::WorldGenReport,
}

/// Buduje miasto M2 dla podanych parametrów. Wydzielone, bo używa go też `m3day`.
///
/// # Errors
/// Błąd parsowania parametru, walidacji geometrii albo generacji miasta.
pub fn zbuduj_miasto(
    seed: u64,
    size: &str,
    region: &str,
    epoch: &str,
    profile: &str,
    pool: &JobPool,
) -> Result<CityData, Box<dyn std::error::Error>> {
    Ok(zbuduj_miasto_z_klimatem(seed, size, region, epoch, profile, pool)?.0)
}

/// To samo co [`zbuduj_miasto`], plus **normy klimatyczne środka miasta**.
///
/// Osobna funkcja, a nie zmieniona sygnatura tamtej: normy potrzebuje jeden
/// konsument (pogoda M8c), a pozostałych pięciu wywołań nie ma powodu przepisywać.
///
/// Normy są kopiowane z komórki klimatu pod środkiem miasta i to jest cała
/// ich droga do `sim/events`. Kopia, a nie zapytanie: `ClimateCell` ma 48 bajtów,
/// nie zmienia się nigdy, a zapytanie wymagałoby trzymania całego terenu przy życiu
/// przez cały przebieg — czyli gigabajtów pod jedną tablicę dwunastu liczb.
///
/// # Errors
/// Jak [`zbuduj_miasto`].
pub fn zbuduj_miasto_z_klimatem(
    seed: u64,
    size: &str,
    region: &str,
    epoch: &str,
    profile: &str,
    pool: &JobPool,
) -> Result<(CityData, magnat_world::ClimateCell), Box<dyn std::error::Error>> {
    let params = WorldGenParams {
        seed,
        size: size.parse()?,
        region: region.parse()?,
        epoch: epoch.parse()?,
        profile: profile.parse()?,
        difficulty: Difficulty::Normal,
    };
    let b = zbuduj_z_params(params, pool, &GenWatch::none())?
        .expect("generacja bez obserwatora nie da się anulować");
    Ok((Arc::unwrap_or_clone(b.city), b.climate))
}

/// Teren i miasto z kompletu parametrów — **jedyne miejsce, w którym świat powstaje**.
///
/// Zwraca `Ok(None)`, jeśli [`GenWatch`] poprosił o przerwanie. Anulowanie jest
/// normalnym wynikiem, a nie błędem: gracz, który wyszedł z ekranu ładowania,
/// nie zrobił nic złego.
///
/// # Errors
/// Parametry poza zakresem (`validate`) albo błąd generatora miasta.
pub fn zbuduj_z_params(
    params: WorldGenParams,
    pool: &JobPool,
    watch: &GenWatch,
) -> Result<Option<BuiltCity>, Box<dyn std::error::Error>> {
    params.validate()?;
    let Some((data, report)) =
        magnat_world::generate_observed(params, pool, &mut |i, name| watch.pass_done(i, name))?
    else {
        return Ok(None);
    };
    let reg = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path(
        "materials",
    ))?);
    let terrain = Arc::new(Terrain::new(data, reg));
    let plan = CityPlan::from_world(&params);
    let city = generate_city(&plan, terrain.as_ref(), terrain.materials(), pool)?;
    watch.pass_done(magnat_world::PASSES.len(), "miasto");
    if watch.cancelled() {
        return Ok(None);
    }
    let climate = {
        use magnat_world::TerrainQuery;
        *terrain.climate_at(city.center.x as i32, city.center.y as i32)
    };
    Ok(Some(BuiltCity {
        terrain,
        city: Arc::new(city),
        climate,
        report,
    }))
}

/// Ile kroków ma etap A zakładania gry — dwanaście passów terenu plus miasto.
///
/// `ponytail:` ziarnistość „miasto to jeden krok" jest świadoma. Generator miasta
/// ma własny raport etapów (`GenerationReport::stage_millis`), ale nie ma kanału,
/// którym mógłby go oddawać w trakcie — dokładanie go to zmiana w siedmiu miejscach
/// `sim/world` dla paska, który i tak rusza się raz na kilka sekund.
#[must_use]
pub fn kroki_generacji() -> usize {
    magnat_world::PASSES.len() + 1
}

/// Świat ECS z zarejestrowaną warstwą M3a i M3c, gotowy do zaludnienia.
///
/// # Errors
/// Brak albo niepoprawny katalog potrzeb lub demografii w `data/`.
pub fn swiat_agentow(seed: u64) -> Result<World, Box<dyn std::error::Error>> {
    let mut world = World::new(seed);
    register(&mut world, NeedTable::load_default()?);
    society::register_society(&mut world, DemographyTable::load_default()?);
    Ok(world)
}

/// Zaludnia świat i zwraca most do `sim/agents`.
///
/// # Errors
/// Miasto bez mieszkań albo błąd budowy tablic ruchu.
pub fn zaludnij(
    world: &mut World,
    city: &CityData,
    citizens: u32,
    swaps: u32,
) -> Result<Populated, Box<dyn std::error::Error>> {
    let p = PopulationParams {
        target_population: (citizens > 0).then_some(citizens),
        unemployment_target_permille: None,
        commute_median_min: None,
        commute_swaps: Some(swaps),
    };
    Ok(generate_population(world, city, &p)?)
}
