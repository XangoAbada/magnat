//! Most między miastem M2 a ruchem M4: flota, stacje paliw i złożenie usług ruchu.
//!
//! Ten moduł jest po stronie `sim/world` z tego samego powodu, co `nav_build`
//! (`J-5`): `sim/traffic` nie może zależeć od `sim/world`, bo `sim/world` zależy
//! od `sim/agents`, a `sim/traffic` też — zależność w drugą stronę zamknęłaby cykl,
//! którego Cargo nie zbuduje.
//!
//! Co tu powstaje i w jakiej kolejności:
//!
//! 1. grafy (`nav_build`) i router CCH z pojemnością cache'u wyliczoną **z liczby
//!    dojeżdżających** (`Y-4` — pojemność jest wymaganiem wdrożeniowym, nie parametrem
//!    do strojenia),
//! 2. stacje paliw wyłuskane z zakładów M2 po kluczu archetypu,
//! 3. flota: pojazd jako encja ECS, jeden na wylosowane gospodarstwo, z kierowcą
//!    wskazanym imiennie,
//! 4. `TrafficServices` i `TrafficNetwork` wstawione do świata razem z hakami hasha.

use crate::city::CityData;
use crate::nav_build::{build_nav, NavBuildError};
use magnat_agents::{
    household, Household, HouseholdOverflow, Identity, PlaceTable, Population,
};
use magnat_core::{rng, PlaceRef, SiteId, StreamId, Tick, WorldCoord};
use magnat_ecs::{Entity, World};
use magnat_nav::NavRouter;
use magnat_traffic::{
    register_traffic, DriverEntry, FuelTank, Station, TrafficNetwork, TrafficOracle,
    TrafficServices, VdfTable, VehicleClass, VehicleCatalog, VehicleClassId, VehicleCondition,
    VehicleLocation, VehicleOwner,
};
use std::sync::Arc;

/// Klucz archetypu stacji paliw w `data/buildings/commerce.ron`.
pub const FUEL_STATION_KEY: &str = "petrol_station";

/// Ile gospodarstw na tysiąc ma samochód.
///
/// Jedna liczba, nie tabela epok — bo dopóki nie ma budżetów gospodarstw (M5)
/// ani rynku aut (M7), zależność od dochodu byłaby zmyślona. Wartość odpowiada
/// polskiej motoryzacji przełomu lat 90.: ~160 aut na 1000 mieszkańców przy
/// gospodarstwie ~2,7-osobowym to ~430 aut na 1000 gospodarstw.
///
/// `ponytail:` sufit nazwany — posiadanie auta nie zależy od zamożności. Ścieżka
/// wyjścia: próg majątkowy, gdy M5 wprowadzi budżet gospodarstwa domowego.
pub const MOTORISATION_PER_MILLE: u16 = 430;

/// Udziały klas pojazdów w promilach, w kolejności katalogu. Suma musi dać 1000.
const CLASS_SHARE_PER_MILLE: [u16; 4] = [480, 350, 130, 40];

/// Ile pojazdów wjeżdża na sieć — wejście rachunku pamięci floty (§7.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FleetReport {
    pub households: u32,
    pub vehicles: u32,
    pub stations: u32,
    pub route_cache_capacity: u32,
}

#[derive(Debug)]
pub enum TrafficBuildError {
    Nav(NavBuildError),
    Data(magnat_traffic::DataError),
}

impl std::fmt::Display for TrafficBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrafficBuildError::Nav(e) => write!(f, "budowa grafu: {e}"),
            TrafficBuildError::Data(e) => write!(f, "dane ruchu: {e}"),
        }
    }
}

impl std::error::Error for TrafficBuildError {}

impl From<NavBuildError> for TrafficBuildError {
    fn from(e: NavBuildError) -> Self {
        TrafficBuildError::Nav(e)
    }
}

impl From<magnat_traffic::DataError> for TrafficBuildError {
    fn from(e: magnat_traffic::DataError) -> Self {
        TrafficBuildError::Data(e)
    }
}

/// Buduje oracle ruchu **bez floty** — tyle, ile potrzeba Etapowi 8 do liczenia
/// dojazdów po sieci pieszej. Flotę dokłada [`seed_fleet`] po dopasowaniu mieszkań.
pub fn build_oracle(
    city: &CityData,
    places: Arc<PlaceTable>,
    commuters: u32,
) -> Result<(TrafficOracle, VdfTable, u32), TrafficBuildError> {
    let (graphs, _) = build_nav(city)?;
    let catalog = Arc::new(VehicleCatalog::load_default()?);
    let vdf = VdfTable::load_default()?;

    // `Y-4`: pojemność ≥ 2 × liczba odrębnych par origin–cel, a klucz niesie kubełek
    // godzinowy, więc par jest ~2 × liczba dojeżdżających. Stąd czterokrotność,
    // zaokrąglona w górę do potęgi dwójki (cache i tak to robi).
    let pojemnosc = (commuters.max(1) * 4).next_power_of_two().clamp(1_024, 1 << 20);
    let router = NavRouter::build(
        graphs,
        city.districts.districts.len().max(1) as u16,
        pojemnosc as usize,
    );
    let stations = stacje(city, &places);
    let oracle = TrafficOracle::new(places, router, catalog, Vec::new(), stations);
    Ok((oracle, vdf, pojemnosc))
}

/// Stacje paliw miasta: zakłady o archetypie [`FUEL_STATION_KEY`], zrzutowane
/// na najbliższy węzeł sieci drogowej.
fn stacje(city: &CityData, places: &PlaceTable) -> Vec<Station> {
    let mut out = Vec::new();
    for (i, s) in city.sites.sites.iter().enumerate() {
        if city.site_catalog.get(s.archetype).spec.key != FUEL_STATION_KEY {
            continue;
        }
        let place = PlaceRef::Site(SiteId(crate::city::sites::site_id(i as u32).0));
        let at = places
            .coord_of(place)
            .unwrap_or(WorldCoord::ORIGIN);
        out.push(Station {
            place,
            at,
            // Węzeł nadaje `TrafficOracle::new` po zbudowaniu indeksu przestrzennego —
            // tutaj nie ma jeszcze do czego zrzutować.
            node: magnat_nav::NodeId(0),
        });
    }
    out
}

/// Obsadza gospodarstwa pojazdami i zwraca listę kierowców.
///
/// Kolejność jest kolejnością indeksów encji mieszkańców, a losowanie idzie
/// strumieniem [`StreamId::VehicleSeed`] z kluczem gospodarstwa — więc ta sama
/// populacja daje tę samą flotę niezależnie od liczby wątków (00 §3.1).
///
/// Kierowcą zostaje **pierwszy pracujący dorosły** gospodarstwa. To jest cały model
/// dostępności auta w M4b: kto ma pojazd, ten nim jeździ. Pełny wybór środka
/// transportu — z kosztem uogólnionym, parkingiem u celu i dostępnością auta dla
/// pozostałych domowników — należy do M4c/WP6.
pub fn seed_fleet(
    world: &mut World,
    seed: u64,
    catalog: &VehicleCatalog,
    motorisation_per_mille: u16,
) -> (Vec<DriverEntry>, Vec<Entity>, FleetReport) {
    let mieszkancy: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    let mut obsadzone: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut drivers: Vec<DriverEntry> = Vec::new();
    let mut fleet: Vec<Entity> = Vec::new();
    let mut gospodarstw = 0u32;

    for c in mieszkancy {
        let Some(id) = world.get::<Identity>(c).copied() else {
            continue;
        };
        let hh = id.household;
        if hh == u32::MAX || !obsadzone.insert(hh) {
            continue;
        }
        gospodarstw += 1;
        if !dorosly_z_praca(world, c) {
            continue;
        }
        let mut r = rng(seed, StreamId::VehicleSeed, hh, Tick(0));
        if !r.gen_bool_permille(motorisation_per_mille) {
            continue;
        }
        let class = losuj_klase(&mut r, catalog);
        // Bak od 20 % do pełna. Świat nie zaczyna się na stacji: część floty ma
        // tankowanie przed sobą już pierwszego dnia i to jest jedyny sposób, żeby
        // scenariusz jednodobowy w ogóle pokazał tankowanie — pełny bak starcza
        // na kilkaset kilometrów, czyli na kilka tygodni dojazdów.
        let napelnienie = 200 + r.gen_range_u32(801);
        let dom = world
            .get::<Household>(hh_entity(world, hh).unwrap_or(c))
            .map(|h| PlaceRef::Building(magnat_core::BuildingId(encja(h.building))))
            .unwrap_or_default();
        let veh = world
            .spawn()
            .with(VehicleOwner::household(hh, c.index()))
            .with(VehicleCondition::new())
            .with(bak(catalog.spec(class), napelnienie))
            .with(VehicleLocation::parked(dom))
            .with(VehicleClass::new(class))
            .id();
        drivers.push(DriverEntry {
            citizen: c.index(),
            vehicle: fleet.len() as u32,
            class,
        });
        fleet.push(veh);
    }

    let report = FleetReport {
        households: gospodarstw,
        vehicles: fleet.len() as u32,
        stations: 0,
        route_cache_capacity: 0,
    };
    (drivers, fleet, report)
}

/// Zbiornik napełniony w `permille` pojemności.
fn bak(spec: &magnat_traffic::VehicleClassSpec, permille: u32) -> FuelTank {
    let mut t = FuelTank::full(spec);
    t.level = t.capacity * i64::from(permille) / 1000;
    t
}

fn losuj_klase(r: &mut magnat_core::Rng, catalog: &VehicleCatalog) -> VehicleClassId {
    let mut p = r.gen_range_u32(1_000);
    for (i, udzial) in CLASS_SHARE_PER_MILLE.iter().enumerate() {
        if i >= catalog.len() {
            break;
        }
        if p < u32::from(*udzial) {
            return VehicleClassId(i as u16);
        }
        p -= u32::from(*udzial);
    }
    VehicleClassId(0)
}

fn dorosly_z_praca(world: &World, c: Entity) -> bool {
    world
        .get::<magnat_agents::Employment>(c)
        .is_some_and(magnat_agents::Employment::has_job)
}

fn hh_entity(world: &World, index: u32) -> Option<Entity> {
    magnat_agents::household_by_index(world, index)
}

fn encja(index: u32) -> magnat_core::Entity {
    magnat_core::Entity::new(index, std::num::NonZeroU32::new(1).expect("1 != 0"))
}

/// Wstawia usługi ruchu i stan sieci do świata razem z hakami hasha.
pub fn install_traffic(
    world: &mut World,
    oracle: Arc<TrafficOracle>,
    vdf: VdfTable,
    fleet: Vec<Entity>,
) {
    let network = oracle.with_road(|road| TrafficNetwork::new(road, &vdf));
    register_traffic(
        world,
        TrafficServices {
            oracle,
            vdf,
            fleet,
        },
        network,
    );
}

/// Rejestracja samych komponentów pojazdu — musi się wydarzyć **przed** obsadzeniem
/// floty, bo `spawn().with(...)` wymaga zarejestrowanego typu.
pub fn register_vehicle_components(world: &mut World) {
    world.register_component::<VehicleOwner>();
    world.register_component::<VehicleCondition>();
    world.register_component::<FuelTank>();
    world.register_component::<VehicleLocation>();
    world.register_component::<VehicleClass>();
}

/// Ilu mieszkańców gospodarstwa mieszka pod tym samym adresem — pomocnicze
/// dla raportu floty.
#[must_use]
pub fn household_size(world: &World, hh: u32) -> usize {
    let Some(e) = hh_entity(world, hh) else {
        return 0;
    };
    let Some(h) = world.get::<Household>(e).copied() else {
        return 0;
    };
    household::members_of(hh, &h, world.resource::<HouseholdOverflow>()).len()
}
