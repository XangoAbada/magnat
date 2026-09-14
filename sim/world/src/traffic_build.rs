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
use magnat_core::{rng, PlaceRef, SimMinute, SiteId, StreamId, Tick, WorldCoord};
use magnat_ecs::{Entity, World};
use magnat_nav::NavRouter;
use magnat_traffic::{
    register_traffic, DriverEntry, FuelTank, ParkingRegistry, Station, TrafficNetwork,
    TrafficOracle, TrafficServices, TransitNetwork, VdfTable, VehicleClass, VehicleCatalog,
    VehicleClassId, VehicleCondition, VehicleLocation, VehicleOwner,
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

/// Udział w promilach parkingów przyulicznych, które są płatne.
///
/// Płaci się na ulicach zbiorczych, nie na osiedlowych — to jedyny podział, jaki
/// M4 potrafi wyprowadzić z danych, które ma (klasa drogi). Strefa płatnego parkowania
/// jako **polityka miejska** należy do M8 i wtedy ten podział zastąpi jej mapa.
pub const PAID_CURB_PRICE_GR_PER_HOUR: i64 = 200;

/// Ile linii komunikacji miejskiej stawia się na każde 50 tys. mieszkańców.
///
/// `ponytail:` liczba linii z populacji zamiast z planu sieci. Sufit nazwany: linie
/// łączą najludniejsze dzielnice parami i nikt nie patrzy, czy się dublują. Ścieżka
/// wyjścia: M8 wprowadza przetargi i inwestycje, a wtedy sieć linii staje się
/// decyzją miasta, nie parametrem generatora.
pub const LINES_PER_50K: u32 = 6;

/// Ile pojazdów przypada na linię. Odstęp w szczycie to 10 minut, a przejazd końcowy
/// trwa ~40, więc cztery autobusy trzymają rozkład bez odwołań.
pub const BUSES_PER_LINE: u32 = 4;

/// Klucz klasy pojazdu komunikacji w `data/vehicles/classes.ron`.
pub const BUS_CLASS_KEY: &str = "city_bus";

/// Ile pojazdów wjeżdża na sieć — wejście rachunku pamięci floty (§7.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FleetReport {
    pub households: u32,
    pub vehicles: u32,
    pub stations: u32,
    pub route_cache_capacity: u32,
    /// Gospodarstwa, które **nie dostały auta, bo nie miały gdzie go trzymać**.
    /// To nie jest błąd generatora: parking jest warunkiem posiadania pojazdu,
    /// inaczej pierwszej doby połowa floty byłaby widmami (`parking_no_ghosts`).
    pub no_home_parking: u32,
    pub parking_lots: u32,
    pub parking_spaces: u64,
    pub transit_lines: u32,
    pub transit_stops: u32,
    pub transit_buses: u32,
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
    parking: &mut ParkingRegistry,
    places: &PlaceTable,
) -> (Vec<DriverEntry>, Vec<Entity>, FleetReport) {
    let mieszkancy: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    let mut obsadzone: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut drivers: Vec<DriverEntry> = Vec::new();
    let mut fleet: Vec<Entity> = Vec::new();
    let mut gospodarstw = 0u32;
    let mut bez_parkingu = 0u32;

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
        // **Auto, którego nie ma gdzie trzymać, nie powstaje.** Bez tego warunku
        // pierwszej doby każdy pojazd startowy byłby widmem: stałby pod domem, nie
        // zajmując żadnego miejsca, i `parking_no_ghosts` nie miałby czego pilnować.
        // Konsekwencja jest zamierzona i realistyczna — na osiedlu bez krawężnika
        // motoryzacja jest niższa.
        let slot = fleet.len() as u32;
        parking.resize_fleet(slot as usize + 1);
        let pod_domem = places.coord_of(dom).unwrap_or(magnat_core::WorldCoord::ORIGIN);
        if parking
            .find_and_reserve(pod_domem, HOME_PARKING_RADIUS_M, slot, 10, SimMinute(u64::MAX))
            .is_err()
        {
            // Krawężnika w promieniu nie ma albo jest pełny. Dom wolnostojący ma wtedy
            // podjazd, blok — nie; tego rozróżnienia M2 nie daje (`BuildingSpec` nie zna
            // pojemności postojowej), więc podjazd dostaje **co czwarte** gospodarstwo,
            // deterministycznie po indeksie.
            //
            // `ponytail:` sufit nazwany. Ścieżka wyjścia jest ta sama, co przy parkingach
            // przy budynkach: pole `parking_spaces` w `BuildingSpec` z M2 zastępuje ten
            // ułamek liczbą, a rejestr i wyszukiwanie nie drgną.
            if hh % 4 != 0 {
                bez_parkingu += 1;
                continue;
            }
            parking.add_private(pod_domem, 1);
            if parking
                .find_and_reserve(pod_domem, HOME_PARKING_RADIUS_M, slot, 10, SimMinute(u64::MAX))
                .is_err()
            {
                bez_parkingu += 1;
                continue;
            }
        }
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
            vehicle: slot,
            class,
            household: hh,
        });
        fleet.push(veh);
    }

    let report = FleetReport {
        households: gospodarstw,
        vehicles: fleet.len() as u32,
        no_home_parking: bez_parkingu,
        parking_lots: parking.lots().len() as u32,
        parking_spaces: parking.occupied_total() + parking.free_total(),
        ..FleetReport::default()
    };
    (drivers, fleet, report)
}

/// W jakim promieniu od domu szuka się miejsca postojowego dla pojazdu startowego.
/// Szerzej niż przy celu podróży: pod domem stoi się na noc i chodzi się dalej.
pub const HOME_PARKING_RADIUS_M: u16 = 400;

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

/// Buduje rejestr parkingów miasta: postój przyuliczny z krawędzi grafu, z ceną
/// wyprowadzoną z klasy drogi.
///
/// Parkingi przy budynkach **nie powstają** — M2 nie deklaruje ich pojemności
/// (`ponytail:` w `parking.rs` nazywa sufit i ścieżkę wyjścia).
#[must_use]
pub fn build_parking(oracle: &TrafficOracle) -> ParkingRegistry {
    let mut r = oracle.with_road(|road| ParkingRegistry::build(road, Vec::new(), 0));
    oracle.with_road(|road| {
        for (i, l) in r.lots_mut().iter_mut().enumerate() {
            let _ = i;
            if l.kind != magnat_traffic::ParkingKind::Curb {
                continue;
            }
            let klasa = road
                .out(l.access_node)
                .iter()
                .map(|e| road.edges[*e as usize].class)
                .max_by_key(|c| c.rank());
            if klasa == Some(magnat_core::RoadClass::Collector) {
                l.price_gr_per_hour = magnat_core::Money(PAID_CURB_PRICE_GR_PER_HOUR);
            }
        }
    });
    r
}

/// Stawia sieć komunikacji miejskiej: linie między najludniejszymi dzielnicami,
/// tabor jako encje pojazdów i kierowców wskazanych imiennie.
///
/// **Korytarze wybiera miasto, przystanki stawia komunikacja.** Tutaj powstaje tylko
/// para „skąd–dokąd" per linia; rozstawienie przystanków wzdłuż trasy jest regułą
/// M4 i siedzi w `TransitLine::from_route`.
///
/// Kierowcy: mieszkańcy pracujący w dzielnicy końcowej linii, wybrani po indeksie
/// encji. `ponytail:` sufit nazwany — zajezdnia **nie jest** miejscem pracy, więc
/// kierowca ma w planie dnia swój dotychczasowy etat, a nie etat przewoźnika.
/// Ścieżka wyjścia: M7 (rynek pracy) daje zajezdni własne wakaty i wtedy wybór
/// kierowcy idzie przez `Vacancies`, nie przez ten wektor.
pub fn seed_transit(
    world: &mut World,
    city: &CityData,
    oracle: &TrafficOracle,
    catalog: &VehicleCatalog,
    fleet: &mut Vec<Entity>,
) -> (TransitNetwork, Vec<u32>, FleetReport) {
    let mut raport = FleetReport::default();
    let Some(class) = catalog.by_key(BUS_CLASS_KEY) else {
        return (TransitNetwork::default(), Vec::new(), raport);
    };
    let ludzi = world.resource::<Population>().citizens().len() as u32;
    let ile_linii = (ludzi * LINES_PER_50K / 50_000).clamp(4, 24) as usize;

    // Korytarze: linia zaczyna się w ludnej dzielnicy i kończy w **najdalszej od niej**,
    // a nie w drugiej z listy. Inaczej cztery linie łączą cztery sąsiadujące centra
    // i sieć obsługuje jedną trzecią miasta — dokładnie to pokazał pierwszy pomiar
    // (27 przystanków na miasto 8 km, 2,9 % udziału komunikacji).
    let mut dzielnice: Vec<(u32, usize)> = city
        .districts
        .districts
        .iter()
        .enumerate()
        .map(|(i, d)| (d.pop_capacity, i))
        .collect();
    dzielnice.sort_unstable_by_key(|(p, i)| (std::cmp::Reverse(*p), *i));
    dzielnice.truncate(ile_linii);

    let mut linie = Vec::new();
    let rozklad = magnat_traffic::Timetable {
        first_min: 5 * 60,
        last_min: 23 * 60,
        headway_peak_min: 5,
        headway_base_min: 15,
        days: 0b111_1111,
    };
    let srodek = |i: usize| city.districts.districts[i].centroid;
    for (_, a) in dzielnice.iter().copied().take(ile_linii) {
        // Drugi koniec: dzielnica najdalsza od pierwszej. Remisy po indeksie.
        let sa = srodek(a);
        let Some((_, b)) = city
            .districts
            .districts
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let dx = f64::from(d.centroid.x - sa.x);
                let dy = f64::from(d.centroid.y - sa.y);
                ((dx * dx + dy * dy) as i64, i)
            })
            .max_by_key(|(d, i)| (*d, std::cmp::Reverse(*i)))
        else {
            continue;
        };
        let wezel = |i: usize| {
            let c = city.districts.districts[i].centroid;
            oracle.nearest_node(magnat_core::WorldCoord::new(
                (c.x * 100.0) as i32,
                (c.y * 100.0) as i32,
                0,
            ))
        };
        let (Some(od), Some(do_)) = (wezel(a), wezel(b)) else {
            continue;
        };
        let Some(trasa) = oracle.route_nodes(od, do_, magnat_core::MinuteOfDay::new(8 * 60)) else {
            continue;
        };
        let krawedzie: Vec<magnat_nav::EdgeId> =
            trasa.legs.iter().flat_map(|l| l.edges.iter().copied()).collect();
        let linia = oracle.with_road(|road| {
            magnat_traffic::TransitLine::from_route(
                magnat_traffic::LineId(linie.len() as u16 + 1),
                magnat_traffic::TransitMode::Bus,
                &krawedzie,
                road,
                rozklad,
                class,
                magnat_core::Money(i64::from(oracle.params().transit_fare_gr as i32)),
                catalog.spec(class).seats.into(),
            )
        });
        if let Some(l) = linia {
            linie.push(l);
        }
    }

    // Tabor: pojazdy jako encje ECS, tak samo jak auta osobowe. Autobus stojący
    // w zajezdni **nie zajmuje miejsca postojowego** — zajezdnia nie jest parkingiem
    // publicznym i nie wchodzi do niezmiennika `parking_no_ghosts`.
    for l in &mut linie {
        // Ile pojazdów trzyma rozkład: tyle, ile kursów jest jednocześnie w trasie.
        // Przejazd w jedną stronę dzielony przez odstęp w szczycie, razy dwa (powrót),
        // z zapasem jednego na postój na pętli. Cztery autobusy na linię wystarczały
        // przy trzech przystankach; przy dwudziestu kurs nie zdąża wrócić po pojazd.
        let dlugosc_min: u32 = l.stops.len() as u32 * 2;
        let ile = (dlugosc_min * 2 / u32::from(l.timetable.headway_peak_min) + 1)
            .clamp(BUSES_PER_LINE, 40);
        for _ in 0..ile {
            let slot = fleet.len() as u32;
            let veh = world
                .spawn()
                .with(VehicleOwner::household(u32::MAX, u32::MAX))
                .with(VehicleCondition::new())
                .with(bak(catalog.spec(class), 1_000))
                .with(VehicleLocation::parked(PlaceRef::District(
                    magnat_core::DistrictId(0),
                )))
                .with(VehicleClass::new(class))
                .id();
            fleet.push(veh);
            l.fleet.push(slot);
            raport.transit_buses += 1;
        }
        raport.transit_stops += l.stop_count() as u32;
    }
    raport.transit_lines = linie.len() as u32;

    // Kierowcy: co setny mieszkaniec z pracą, po indeksie encji. Kurs bierze
    // pierwszego, który jest dziś w pracy; reszta jest rezerwą.
    let kierowcy: Vec<u32> = world
        .resource::<Population>()
        .citizens()
        .iter()
        .map(|e| e.index())
        .filter(|i| i % 97 == 0)
        .take(raport.transit_buses as usize * 4)
        .collect();

    let siec = oracle.with_road(|road| TransitNetwork::new(linie, road));
    (siec, kierowcy, raport)
}

/// Wstawia usługi ruchu i stan sieci do świata razem z hakami hasha.
pub fn install_traffic(
    world: &mut World,
    oracle: Arc<TrafficOracle>,
    vdf: VdfTable,
    fleet: Vec<Entity>,
    transit_drivers: Vec<u32>,
) {
    let network = oracle.with_road(|road| TrafficNetwork::new(road, &vdf));
    register_traffic(
        world,
        TrafficServices {
            oracle,
            vdf,
            fleet,
            transit_drivers,
        },
        network,
    );
}

/// Dochód netto gospodarstw w groszach na godzinę, indeksowany indeksem encji GD.
///
/// `income_monthly` jest brutto-miesięczny; dzielimy przez typowy miesięczny czas
/// pracy. Podatek dochodowy jest w M8 — do tego czasu „netto" znaczy „to, co
/// gospodarstwo widzi", i jest to ta sama liczba.
#[must_use]
pub fn dochody_gospodarstw(world: &World) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for e in world.resource::<Population>().citizens() {
        let Some(id) = world.get::<Identity>(*e).copied() else {
            continue;
        };
        if id.household == u32::MAX {
            continue;
        }
        let Some(hh) = hh_entity(world, id.household) else {
            continue;
        };
        let Some(h) = world.get::<Household>(hh).copied() else {
            continue;
        };
        if out.len() <= id.household as usize {
            out.resize(id.household as usize + 1, 0);
        }
        out[id.household as usize] = h.income_monthly.0 / WORK_HOURS_PER_MONTH;
    }
    out
}

/// Typowy miesięczny czas pracy — 168 godzin (21 dni roboczych × 8 h w kalendarzu
/// 360-dniowym z `K-1`).
const WORK_HOURS_PER_MONTH: i64 = 168;

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
