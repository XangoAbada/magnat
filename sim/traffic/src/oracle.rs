//! `TrafficOracle` — implementacja `TravelOracle` z M3 na sieci M4 (`Z-1`, `Y-2`).
//!
//! To jest **punkt podmiany**: `sim/agents::Sources.travel` dostaje ten typ zamiast
//! `WalkOracle`, a planer i systemy doby nie zmieniają ani jednego wywołania.
//! Moduł `walk` z M3 znika w całości razem z tą zmianą.
//!
//! ## Podział pracy między oracle a krokiem mezo
//!
//! Oracle **szacuje i trasuje**, sieć **jedzie i rozlicza**. Granica jest tam, bo:
//!
//! - `estimate` woła planer ~4 razy na plan na mieszkańca na dobę — dla metropolii
//!   to ponad milion wywołań na dobę gry. Routing kosztuje 33 µs, więc byłoby to
//!   37 sekund na dobę: szacunek **musi** być formułą, nie zapytaniem do grafu.
//! - `begin_trip` woła się raz na faktyczną podróż (~1 000/min w szczycie) i dopiero
//!   tam opłaca się prawdziwa trasa z `RouteCache`.
//!
//! Konsekwencja jest zamierzona: `TripHandle.minutes` niesie **czas planowany**
//! (dokładnie ten sam, który zwróciło `estimate` — inaczej slot `Commute` w planie
//! nie zgadzałby się z podróżą, a planer stoi na tej równości), a minutę faktycznego
//! przybycia wyznacza mezo. Różnica jest spóźnieniem i jedzie do mieszkańca przez
//! `ReplanCause::Late`.

use crate::micro::MicroLayer;
use crate::mode::{
    evaluate_modes, Infeasible, ModeChoiceParams, ModeContext, ModeDecision, OptionOffer,
    TravelOption,
};
use crate::parking::ParkingRegistry;
use crate::spec::{VehicleCatalog, VehicleClassId};
use crate::transit::TransitNetwork;
use crate::trip::{PendingTrip, TripId, TripPurpose};
use magnat_agents::{
    ArrayVec, CitizenView, EventKind, EventQueue, PlaceTable, SimEvent, TravelEstimate,
    TravelOracle, TripHandle, TripRequest, MAX_ON_ROUTE,
};
use magnat_core::{
    weather_at, DayOfWeek, DecisionReason, HashState, Mass, MinuteOfDay, Money, PlaceRef,
    SimMinute, StateHasher, TransportMode, Weather, WorldCoord,
};
use magnat_nav::{NavRouter, NodeId, RouteQuery, Router};
use magnat_spatial::{Aabb2, CsrGrid, GridSpec, Vec2};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Prędkość marszu bazowa — centymetry na minutę (4,86 km/h). Przeniesiona z M3
/// bez zmiany wartości: podmiana warstwy transportu nie ma prawa zmienić tempa
/// pieszego, bo wtedy rozjechałyby się wszystkie kalibracje doby z M3b.
pub const WALK_SPEED_CM_PER_MIN: i64 = 8_100;

/// Nadłożenie trasy pieszej wobec linii prostej — **wyłącznie na wypadek braku
/// sieci** (dom poza składową spójności). 1,25 to wartość zmierzona w M3.
const WALK_DETOUR_NUM: i64 = 125;
const DETOUR_DEN: i64 = 100;

/// Typowa prędkość miejska w dekakm/h — do szacunku nadłożenia objazdu na stację,
/// gdzie chodzi o rząd wielkości, a nie o wycenę.
const CAR_FALLBACK_DKMH: i64 = 280;

/// Narzut planistyczny w promilach: o ile czas swobodny z routera jest optymistyczny
/// wobec przejazdu z sygnalizacją, pierwszeństwem i ruchem.
///
/// Router liczy wagi z prędkości swobodnych i **nie zna węzłów** (`J-8`), a przejazd
/// przez miasto to w połowie stanie na skrzyżowaniach. Bez narzutu każdy plan byłby
/// spóźniony systematycznie i mieszkańcy przeplanowywaliby dobę bez przerwy.
///
/// Zmierzone na mieście 4 km (dziennik M4b): przejazd trwa średnio **3,1×** dłużej niż
/// plan z narzutem 1,8, z czego jazda to 3,2 min, skrzyżowania 0,6 min, a kolejki
/// 6,3 min. Kolejki biorą się stąd, że router liczy wagi z prędkości swobodnych
/// i **nie ma sprzężenia zwrotnego z obciążeniem**: wszyscy dostają tę samą najkrótszą
/// trasę i zjeżdżają się na te same kolektory. Narzut 3,0 zabiera połowę tego błędu,
/// reszta zostaje jako spóźnienie — i tak ma być, bo spóźnienie z korka jest treścią
/// tej fazy, a nie jej wadą.
///
/// `ponytail:` jedna stała zamiast profilu godzinowego. Sufit nazwany: narzut nie wie
/// nic o porze dnia ani o dzielnicy. Ścieżka wyjścia jest zaprojektowana i czeka —
/// `TravelTimeMatrix` (dzielnica × godzina × środek) karmiona obserwacjami z ledgera;
/// jej konsumentem jest wybór środka transportu w M4c/WP6.
const PLANNING_MARGIN_PERMILLE: i64 = 3_000;

/// Prędkość roweru w centymetrach na minutę (15 km/h — tyle, ile warstwa rowerowa
/// grafu deklaruje jako prędkość swobodną).
const BIKE_SPEED_CM_PER_MIN: i64 = 25_000;

/// W jakim promieniu od celu szuka się miejsca postojowego. Poszczególne parkingi
/// mają własny, węższy promień dojścia — ten jest sufitem zapytania (§5.5).
const PARKING_SEARCH_RADIUS_M: u16 = 400;

/// Ile godzin postoju wchodzi do kosztu uogólnionego przy parkingu płatnym.
const PARKING_ASSUMED_HOURS: i64 = 2;

/// Zapas zasięgu: podróż wchodzi bez tankowania tylko wtedy, gdy w baku jest
/// wielokrotność szacowanego zużycia. Bez tego pojazd mógłby stanąć w trasie,
/// a `TripFailure::NoFuel` nie jest sytuacją, którą M4b chce produkować —
/// niewykonalność ma się objawiać **przy planowaniu** (M4 §7.1).
pub const FUEL_RESERVE_FACTOR: i64 = 4;

/// Kierowca: mieszkaniec z przypisanym pojazdem.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct DriverEntry {
    /// Indeks encji mieszkańca.
    pub citizen: u32,
    /// Indeks encji pojazdu.
    pub vehicle: u32,
    pub class: VehicleClassId,
    /// Gospodarstwo właściciela — to przez nie dociera do auta **drugi** domownik
    /// (`TravelOption::CarHousehold`). Bez tego pola „auto rodzinne" nie miałoby jak
    /// istnieć: `drivers` jest indeksowane kierowcą, a szuka się po gospodarstwie.
    pub household: u32,
}

/// Stacja paliw: miejsce w katalogu M2 plus węzeł sieci drogowej, do którego się dojeżdża.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Station {
    pub place: PlaceRef,
    pub at: WorldCoord,
    pub node: NodeId,
}

/// Bufory routera. Osobny zamek niż kolejka zleceń, bo trasowanie jest długie,
/// a wstawienie zlecenia krótkie.
///
/// `ponytail:` jeden `Mutex` zamiast sesji per wątek (`Y-3`). Sufit nazwany i zmierzony:
/// p95 zapytania to 33 µs, 100 zapytań na minutę gry to 3,3 ms jednowątkowo wobec
/// budżetu 1,5 ms z §7.3. Dziś oba systemy, które tu wchodzą, są **wyłączne**
/// (`DayLoopSystem` i `TrafficSystem`), więc rywalizacji o zamek nie ma w ogóle,
/// a zrównoleglenie routingu nie kupiłoby nic, dopóki wołający jest jednowątkowy.
/// Ścieżka wyjścia: sesja routera per wątek, gdy M4c zrównolegli wybór środka.
struct RouterCell {
    router: NavRouter,
}

/// Implementacja `TravelOracle` oparta na `engine/nav` i na warstwie mezo.
pub struct TrafficOracle {
    places: Arc<PlaceTable>,
    router: Mutex<RouterCell>,
    /// Pozycje węzłów sieci — wspólne dla wszystkich warstw (`nav_build` trzyma
    /// tę samą przestrzeń indeksów w każdej modalności).
    nodes: Vec<WorldCoord>,
    index: CsrGrid<u32>,
    /// Kierowcy, posortowani po indeksie encji mieszkańca — wyszukiwanie binarne.
    drivers: Vec<DriverEntry>,
    stations: Vec<Station>,
    catalog: Arc<VehicleCatalog>,
    pending: Mutex<Vec<PendingTrip>>,
    next_trip: AtomicU32,
    watched: Mutex<Vec<u32>>,
    /// Który pojazd jest w tej chwili w podróży. To **nie jest** kopia stanu
    /// z ECS, tylko własność oracle: to on tworzy podróże, więc on jeden wie,
    /// że pojazd już wyruszył i nie stoi pod domem.
    ///
    /// Bez tego mieszkaniec, którego auto stoi w korku, zgłaszałby kolejną podróż
    /// tym samym autem przy każdym przeplanowaniu — a każda z nich wjeżdżałaby na
    /// sieć osobno. Flota rozmnażałaby się dokładnie wtedy, kiedy najmniej trzeba.
    busy: Mutex<Vec<bool>>,
    /// Ten sam zbiór co `drivers`, posortowany po gospodarstwie — wyszukiwanie
    /// binarne dla opcji „auto rodzinne".
    by_household: Vec<DriverEntry>,
    micro: MicroLayer,
    /// Parkingi miasta razem z tym, gdzie stoi każdy pojazd (WP7).
    parking: Mutex<ParkingRegistry>,
    /// Linie, kursy i kolejki na przystankach (WP10).
    transit: Mutex<TransitNetwork>,
    params: ModeChoiceParams,
    /// Dochód netto gospodarstwa w groszach na godzinę, indeksowany indeksem encji GD.
    ///
    /// `ponytail:` migawka z generacji świata zamiast odczytu z `Household` co podróż.
    /// Sufit nazwany: awans i utrata pracy nie zmieniają wartości czasu do końca sesji.
    /// Ścieżka wyjścia: M5 wprowadza budżet gospodarstwa i wtedy `vot` czyta go wprost,
    /// bo budżet i tak musi być odpytywalny z decyzji zakupowej.
    incomes: Vec<i64>,
    /// Ostatnio wybrany środek per mieszkaniec — podstawa premii nawyku (§5.3).
    /// `u8::MAX` = brak historii.
    habit: Mutex<Vec<u8>>,
    seed: u64,
    /// Doba świata i pogoda tej doby. Liczone raz na dobę przez `TrafficSystem`,
    /// bo `weather_at` jest czystą funkcją i wołanie jej milion razy nic nie zmienia.
    day: AtomicU64,
    weather: Mutex<Weather>,
    /// Suma taryf zapłaconych przewoźnikom taksówkowym, w groszach. Druga strona
    /// bilansu pieniądza dla opcji, która nie wjeżdża na sieć (`D5`).
    taxi_fares: AtomicU64,
    /// Ile podróży rozpoczęto którą opcją — **licznik diagnostyczny, nie stan**,
    /// więc nie wchodzi do hasha (tak samo jak `rejects_total` w `TrafficNetwork`).
    /// To on jest wejściem bramki rozkładu udziałów z §20.1.
    mode_counts: [AtomicU64; 6],
    /// Ile razy któraś opcja odpadła z podanego powodu — indeks jak w `Infeasible`.
    infeasible_counts: [AtomicU64; 8],
}

/// Rozmiar komórki indeksu węzłów w metrach. Tyle samo, ile miał indeks sieci
/// pieszej w M3 — zapytanie idzie o najbliższy węzeł, nie o promień.
const NODE_CELL_M: u16 = 60;

impl TrafficOracle {
    /// Buduje oracle na gotowym routerze. Router przynosi ze sobą grafy — to on
    /// jest właścicielem `NavGraphs` od M4a.
    #[must_use]
    pub fn new(
        places: Arc<PlaceTable>,
        router: NavRouter,
        catalog: Arc<VehicleCatalog>,
        drivers: Vec<DriverEntry>,
        stations: Vec<Station>,
    ) -> TrafficOracle {
        let nodes: Vec<WorldCoord> = router
            .graphs()
            .layer(magnat_nav::Modality::Road)
            .nodes
            .iter()
            .map(|n| WorldCoord::new(n.pos_cm.x, n.pos_cm.y, n.z_cm))
            .collect();
        let index = build_index(&nodes, nodes.iter().enumerate().map(|(i, p)| (*p, i as u32)));
        let mut drivers = drivers;
        drivers.sort_unstable();
        let mut by_household = drivers.clone();
        by_household.sort_unstable_by_key(|d| (d.household, d.citizen));
        let flota = drivers.iter().map(|d| d.vehicle + 1).max().unwrap_or(0) as usize;
        // Stacje zrzutowane na najbliższy węzeł drogowy — wołający podaje tylko
        // miejsce, bo indeks węzłów powstaje dopiero tutaj.
        let mut stations = stations;
        for st in &mut stations {
            st.node = nearest(&index, &nodes, st.at).map_or(NodeId(0), NodeId);
        }
        TrafficOracle {
            places,
            router: Mutex::new(RouterCell { router }),
            nodes,
            index,
            drivers,
            stations,
            catalog,
            pending: Mutex::new(Vec::new()),
            next_trip: AtomicU32::new(1),
            watched: Mutex::new(Vec::new()),
            busy: Mutex::new(vec![false; flota]),
            by_household,
            micro: MicroLayer::new(),
            parking: Mutex::new(ParkingRegistry::default()),
            transit: Mutex::new(TransitNetwork::default()),
            params: ModeChoiceParams::load_default()
                .unwrap_or_else(|e| panic!("data/roads/mode_choice.ron: {e}")),
            incomes: Vec::new(),
            habit: Mutex::new(Vec::new()),
            seed: 0,
            day: AtomicU64::new(0),
            weather: Mutex::new(Weather::default()),
            taxi_fares: AtomicU64::new(0),
            mode_counts: std::array::from_fn(|_| AtomicU64::new(0)),
            infeasible_counts: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Suma taryf taksówkowych od początku sesji, w groszach.
    #[must_use]
    pub fn taxi_fares(&self) -> Money {
        Money(self.taxi_fares.load(Ordering::Relaxed) as i64)
    }

    /// Ile podróży rozpoczęto którą opcją, w kolejności [`TravelOption::ALL`].
    #[must_use]
    pub fn mode_counts(&self) -> [u64; 6] {
        std::array::from_fn(|i| self.mode_counts[i].load(Ordering::Relaxed))
    }

    /// Ile razy opcja odpadła z którego powodu: `NoCarInHousehold`, `CarInUseBy`,
    /// `NoParkingWithinRadius`, `InsufficientFuelRange`, `NoTransitConnection`,
    /// `NoRoute`, `DistanceOverPersonalLimit`, `BelowMinimumAge`/`VehicleBroken`.
    #[must_use]
    pub fn infeasible_counts(&self) -> [u64; 8] {
        std::array::from_fn(|i| self.infeasible_counts[i].load(Ordering::Relaxed))
    }

    fn policz_niewykonalne(&self, d: &ModeDecision) {
        for c in d.candidates.as_slice() {
            let i = match c.infeasible {
                Some(Infeasible::NoCarInHousehold) => 0,
                Some(Infeasible::CarInUseBy(_)) => 1,
                Some(Infeasible::NoParkingWithinRadius { .. }) => 2,
                Some(Infeasible::InsufficientFuelRange { .. }) => 3,
                Some(Infeasible::NoTransitConnection) => 4,
                Some(Infeasible::NoRoute) => 5,
                Some(Infeasible::DistanceOverPersonalLimit { .. }) => 6,
                Some(
                    Infeasible::BelowMinimumAge
                    | Infeasible::VehicleBroken
                    | Infeasible::BeyondBudget { .. },
                ) => 7,
                None => continue,
            };
            self.infeasible_counts[i].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Wstawia rejestr parkingów. Wołane raz, przy budowie świata — rejestr potrzebuje
    /// grafu drogowego, a ten jest własnością routera, czyli tego obiektu.
    pub fn set_parking(&mut self, parking: ParkingRegistry) {
        *self.parking.lock().expect("parking") = parking;
    }

    pub fn set_transit(&mut self, transit: TransitNetwork) {
        *self.transit.lock().expect("transit") = transit;
    }

    /// Dochody gospodarstw w groszach na godzinę, indeksowane indeksem encji GD.
    pub fn set_incomes(&mut self, incomes: Vec<i64>) {
        self.incomes = incomes;
    }

    /// Ziarno świata — wchodzi do pogody i do losowań wyboru miejsca.
    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        *self.weather.lock().expect("weather") = weather_at(seed, 0);
    }

    /// Doba świata. Woła to `TrafficSystem`; pogoda przelicza się przy zmianie doby
    /// i tylko wtedy.
    pub fn set_day(&self, day: u64) {
        if self.day.swap(day, Ordering::Relaxed) != day {
            *self.weather.lock().expect("weather") = weather_at(self.seed, day);
        }
    }

    #[must_use]
    pub fn weather(&self) -> Weather {
        *self.weather.lock().expect("weather")
    }

    #[must_use]
    pub fn params(&self) -> &ModeChoiceParams {
        &self.params
    }

    /// Dostęp do parkingów pod zamkiem — dla systemu ruchu i dla raportu.
    pub fn with_parking<R>(&self, f: impl FnOnce(&mut ParkingRegistry) -> R) -> R {
        f(&mut self.parking.lock().expect("parking"))
    }

    pub fn with_transit<R>(&self, f: impl FnOnce(&mut TransitNetwork) -> R) -> R {
        f(&mut self.transit.lock().expect("transit"))
    }

    /// Auto gospodarstwa, którego nie prowadzi ten mieszkaniec. Zwraca pierwsze
    /// w kolejności indeksu kierowcy — klucz totalny, więc wynik nie zależy od tego,
    /// kto zapytał pierwszy.
    #[must_use]
    pub fn household_car(&self, household: u32, except: u32) -> Option<DriverEntry> {
        let i = self
            .by_household
            .partition_point(|d| (d.household, d.citizen) < (household, 0));
        self.by_household[i..]
            .iter()
            .take_while(|d| d.household == household)
            .find(|d| d.citizen != except)
            .copied()
    }

    /// Dochód netto gospodarstwa w groszach na godzinę.
    #[must_use]
    fn income_of(&self, household: u32) -> i64 {
        self.incomes.get(household as usize).copied().unwrap_or(0)
    }

    fn habit_of(&self, citizen: u32) -> Option<TravelOption> {
        let h = self.habit.lock().expect("habit");
        match h.get(citizen as usize).copied() {
            Some(v) if (v as usize) < TravelOption::ALL.len() => Some(TravelOption::ALL[v as usize]),
            _ => None,
        }
    }

    fn remember_habit(&self, citizen: u32, option: TravelOption) {
        let mut h = self.habit.lock().expect("habit");
        if h.len() <= citizen as usize {
            h.resize(citizen as usize + 1, u8::MAX);
        }
        h[citizen as usize] = option as u8;
    }

    #[must_use]
    pub fn catalog(&self) -> &Arc<VehicleCatalog> {
        &self.catalog
    }

    #[must_use]
    pub fn drivers(&self) -> &[DriverEntry] {
        &self.drivers
    }

    #[must_use]
    pub fn stations(&self) -> &[Station] {
        &self.stations
    }

    /// Warstwa drogowa grafu — pod zamkiem routera, bo to on jest właścicielem
    /// `NavGraphs` (M4a). Domknięcie zamiast zwracania referencji: `Mutex` nie da
    /// się wypożyczyć na zewnątrz bez strażnika.
    ///
    /// **Wewnątrz domknięcia nie wolno wołać niczego, co trasuje** (`car_route`,
    /// `route_nodes`, `network_walk_minutes`): `std::sync::Mutex` nie jest wznawialny
    /// i drugie wejście zakleszcza wątek. Krok minutowy warstwy mezo tego nie robi —
    /// trasy powstają przed nim, w `begin_trip` i w przygotowaniu zleceń.
    pub fn with_road<R>(&self, f: impl FnOnce(&magnat_nav::RoadGraph) -> R) -> R {
        let g = self.router.lock().expect("router");
        f(g.router.graphs().layer(magnat_nav::Modality::Road))
    }

    #[must_use]
    pub fn micro(&self) -> &MicroLayer {
        &self.micro
    }

    /// Mieszkaniec, którego podróże mają trafiać do karty inspekcji z pełnym
    /// rejestrem krawędź po krawędzi (decyzja 9.16 z M3: najwyżej ośmiu).
    pub fn watch(&self, citizen: u32) {
        let mut w = self.watched.lock().expect("watched");
        if !w.contains(&citizen) {
            w.push(citizen);
        }
    }

    /// Odbiera zlecenia zebrane od ostatniego kroku minutowego.
    #[must_use]
    pub fn take_pending(&self) -> Vec<PendingTrip> {
        std::mem::take(&mut *self.pending.lock().expect("pending"))
    }

    /// Węzeł sieci najbliższy podanemu miejscu. Remis rozstrzyga indeks węzła,
    /// nie układ komórek indeksu — inaczej trasa zależałaby od rozmiaru komórki.
    #[must_use]
    pub fn nearest_node(&self, at: WorldCoord) -> Option<NodeId> {
        nearest(&self.index, &self.nodes, at).map(NodeId)
    }

    #[must_use]
    pub fn coord_of(&self, p: PlaceRef) -> WorldCoord {
        match p {
            PlaceRef::Coord(c) => c,
            _ => self
                .places
                .entries()
                .binary_search_by(|e| e.place.cmp(&p))
                .map_or(WorldCoord::ORIGIN, |i| self.places.entries()[i].at),
        }
    }

    /// Czy ten mieszkaniec ma pojazd. Tablica jest statyczna — obsadzanie floty
    /// dzieje się raz, przy generacji świata.
    #[must_use]
    pub fn driver_of(&self, citizen: u32) -> Option<DriverEntry> {
        self.drivers
            .binary_search_by_key(&citizen, |d| d.citizen)
            .ok()
            .map(|i| self.drivers[i])
    }

    /// Obsadza flotę po zbudowaniu oracle. Wołane raz, przy generacji świata:
    /// Etap 8 potrzebuje estymatora **zanim** wie, kto ma samochód (dopasowanie
    /// mieszkań liczy dojazdy pieszo), a przypisanie pojazdów jest jego ostatnim
    /// krokiem. Rozbijanie tego na dwa obiekty kosztowałoby drugi router.
    pub fn set_drivers(&mut self, mut drivers: Vec<DriverEntry>) {
        drivers.sort_unstable();
        let n = drivers.iter().map(|d| d.vehicle + 1).max().unwrap_or(0) as usize;
        *self.busy.lock().expect("busy") = vec![false; n];
        self.by_household = drivers.clone();
        self.by_household
            .sort_unstable_by_key(|d| (d.household, d.citizen));
        self.parking.lock().expect("parking").resize_fleet(n);
        self.drivers = drivers;
    }

    /// Zwalnia pojazd po zakończonej albo przerwanej podróży. Woła to system ruchu
    /// przy każdym `Arrived` i `Failed` — inaczej auto zostałoby zajęte na zawsze.
    pub fn release_vehicle(&self, vehicle: u32) {
        let mut b = self.busy.lock().expect("busy");
        if let Some(v) = b.get_mut(vehicle as usize) {
            *v = false;
        }
    }

    /// Czy pojazd jest wolny; `true` rezerwuje go na tę podróż.
    fn take_vehicle(&self, vehicle: u32) -> bool {
        let mut b = self.busy.lock().expect("busy");
        match b.get_mut(vehicle as usize) {
            Some(v) if !*v => {
                *v = true;
                true
            }
            _ => false,
        }
    }

    /// Czas dojścia **po sieci pieszej** — dla generacji świata, nie dla planera.
    ///
    /// Etap 8 (dopasowanie mieszkań do miejsc pracy) potrzebuje realnej odległości
    /// sieciowej, bo steruje medianą dojazdu całego miasta; woła się ją raz na parę
    /// dom–praca, a nie milion razy na dobę, więc stać ją na trasowanie. Planer
    /// w pętli doby używa `estimate`, czyli formuły — i to jest świadoma różnica,
    /// opisana w nagłówku modułu.
    ///
    /// Brak trasy (dom poza składową spójności) spada na szacunek manhattanowy,
    /// a nie na błąd: mieszkanie bez chodnika to znalezisko o mieście M2, nie awaria.
    #[must_use]
    pub fn network_walk_minutes(&self, from: PlaceRef, to: PlaceRef, speed_pct: u32) -> u16 {
        let a = self.coord_of(from);
        let b = self.coord_of(to);
        let zapasowy = || walk_minutes_for(manhattan_cm(a, b) * WALK_DETOUR_NUM / DETOUR_DEN, speed_pct);
        let (Some(od), Some(do_)) = (self.nearest_node(a), self.nearest_node(b)) else {
            return zapasowy();
        };
        if od == do_ {
            return zapasowy();
        }
        let q = RouteQuery::passenger(od, do_, TransportMode::Walk, 8);
        let Some(r) = self.router.lock().expect("router").router.route(&q) else {
            return zapasowy();
        };
        // Router liczy prędkością swobodną warstwy pieszej; wiek i zdrowie
        // skalują ją tak samo jak w M3.
        let m = u32::from(r.planned_minutes) * 100 / speed_pct.max(1);
        m.clamp(1, u32::from(u16::MAX)) as u16
    }

    /// Trasa samochodowa między dwoma miejscami. `None` = nie ma połączenia,
    /// co dla 2,8 % par metropolii jest **prawdą o sieci M2**, a nie błędem
    /// routera (`Y-1`, `J-18`).
    #[must_use]
    pub fn car_route(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        depart: MinuteOfDay,
    ) -> Option<Arc<magnat_nav::Route>> {
        let a = self.nearest_node(self.coord_of(from))?;
        let b = self.nearest_node(self.coord_of(to))?;
        if a == b {
            return None;
        }
        let q = RouteQuery::passenger(a, b, TransportMode::Car, depart.hour());
        self.router.lock().expect("router").router.route(&q)
    }

    /// Trasa samochodowa między węzłami — używa jej system, gdy wstawia postój
    /// na stacji w środek podróży.
    #[must_use]
    pub fn route_nodes(
        &self,
        a: NodeId,
        b: NodeId,
        depart: MinuteOfDay,
    ) -> Option<Arc<magnat_nav::Route>> {
        if a == b {
            return None;
        }
        let q = RouteQuery::passenger(a, b, TransportMode::Car, depart.hour());
        self.router.lock().expect("router").router.route(&q)
    }

    /// Stacja paliw najbliżej odcinka „skąd–dokąd", czyli w korytarzu trasy (§5.7).
    ///
    /// Kandydatów szuka się wokół punktu środkowego; wygrywa ta o najmniejszym
    /// nadłożeniu, liczonym w linii prostej. Remisy rozstrzyga indeks stacji, nie
    /// kolejność iteracji po indeksie przestrzennym.
    #[must_use]
    pub fn station_on_route(&self, from: WorldCoord, to: WorldCoord) -> Option<(Station, u16)> {
        if self.stations.is_empty() {
            return None;
        }
        let wprost = euclid_cm(from, to);
        let mut best: Option<(i64, u32)> = None;
        for (i, s) in self.stations.iter().enumerate() {
            let przez = i64::from(euclid_cm(from, s.at)) + i64::from(euclid_cm(s.at, to));
            let detour = przez - i64::from(wprost);
            if best.is_none_or(|(d, _)| detour < d) {
                best = Some((detour, i as u32));
            }
        }
        let (detour_cm, i) = best?;
        let detour_min = (detour_cm * 36 / (CAR_FALLBACK_DKMH * 6_000)).clamp(0, i64::from(u16::MAX));
        Some((self.stations[i as usize], detour_min as u16))
    }

    /// Wybór środka transportu — **wspólny dla `estimate` i `begin_trip`** (WP6, §5.3).
    ///
    /// To jest punkt, w którym M4c wymienia regułę zastępczą M4b (`M-1`): zamiast
    /// „kto ma auto i jedzie dalej niż 800 m, ten jedzie", jest pełne porównanie
    /// kosztu uogólnionego wszystkich wykonalnych opcji.
    ///
    /// **Jedna funkcja dla planu i dla przejazdu** — plan i podróż muszą mówić o tej
    /// samej liczbie, inaczej slot `Commute` w planie M3 nie zgadza się z podróżą
    /// i mieszkaniec przeplanowuje dobę bez przerwy. Różnicę robi wyłącznie `commit`:
    ///
    /// - `commit == false` (szacunek planera): parking jest **sprawdzany**, nie
    ///   rezerwowany, a nawyk nie jest zapisywany. Planer woła to ~4 razy na dobę na
    ///   mieszkańca i nie ma prawa zająć miejsca postojowego na podróż, której może
    ///   nigdy nie być.
    /// - `commit == true` (faktyczne wyruszenie): miejsce jest rezerwowane, pasażer
    ///   wchodzi do kolejki przystanku, a wybór zapisuje się jako nawyk.
    fn plan(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        who: &CitizenView<'_>,
        purpose: TripPurpose,
        depart: MinuteOfDay,
        commit: bool,
    ) -> ModeDecision {
        let a = self.coord_of(from);
        let b = self.coord_of(to);
        let citizen = who.id.entity().index();
        let ctx = ModeContext {
            citizen,
            age_years: who.identity.age_years(who.today).max(0) as u16,
            status_percentile: who.vitals.status,
            hourly_net_income_gr: self.income_of(who.identity.household),
            purpose,
            weather: self.weather(),
            // Zakupy wraca się z siatkami, do pracy z teczką. Jedna liczba na cel
            // zamiast modelu koszyka — koszyk jest w M5 i wtedy wejdzie tu wprost.
            luggage_kg: match purpose {
                TripPurpose::Shopping => 6,
                _ => 0,
            },
            habit: self.habit_of(citizen),
        };

        // Sloty pojazdów, dla których wycena zarezerwowała miejsce postojowe. Przegrane
        // opcje oddają je zaraz po wyborze — patrz niżej.
        let mut zarezerwowane: Vec<(TravelOption, u32)> = Vec::new();
        let mut oferty: Vec<(TravelOption, Result<OptionOffer, Infeasible>)> =
            Vec::with_capacity(TravelOption::ALL.len());
        oferty.push((TravelOption::Walk, self.offer_walk(from, to, who)));
        oferty.push((TravelOption::Bike, self.offer_bike(a, b, &ctx)));
        oferty.push((
            TravelOption::Transit,
            self.offer_transit(a, b, depart, who.today),
        ));

        // Auto własne i auto rodzinne różnią się **wyłącznie** dostępnością pojazdu —
        // trasa, paliwo i parking są te same, więc liczą się raz.
        let wlasne = self.driver_of(citizen);
        let rodzinne = self
            .household_car(who.identity.household, citizen)
            .filter(|_| wlasne.is_none());
        for (opcja, kierowca) in [
            (TravelOption::CarOwn, wlasne),
            (TravelOption::CarHousehold, rodzinne),
        ] {
            let oferta = self.offer_car(from, to, a, b, depart, &ctx, kierowca, commit);
            if let (Ok(o), Some(k)) = (&oferta, kierowca) {
                if o.parking.is_some() {
                    zarezerwowane.push((opcja, k.vehicle));
                }
            }
            oferty.push((opcja, oferta));
        }
        oferty.push((
            TravelOption::Taxi,
            self.offer_taxi(from, to, a, b, depart, &ctx),
        ));

        match evaluate_modes(&self.params, &ctx, &oferty) {
            Some(d) => {
                if commit {
                    self.remember_habit(citizen, d.chosen);
                    // Rezerwacja powstaje przy **wycenie** auta, bo bez niej nie
                    // wiadomo ani czy opcja jest wykonalna, ani ile kosztuje dojście
                    // od parkingu. Gdy wygra co innego, miejsce wraca do puli w tej
                    // samej minucie — inaczej każda rozważona i odrzucona jazda
                    // zabierałaby centrum jedno miejsce na zawsze.
                    for (opcja, vehicle) in &zarezerwowane {
                        if *opcja != d.chosen {
                            self.with_parking(|p| p.release(*vehicle));
                        }
                    }
                }
                d
            }
            // **Żadna opcja nie jest wykonalna.** Nie wolno tego zostawić bez podróży:
            // mieszkaniec czeka na `Arrive` i bez niego stoi do końca gry (`M-4`).
            // Idzie więc pieszo, choćby długo — marsz jest jedyną opcją, która nie
            // może zawieść, bo nie potrzebuje ani pojazdu, ani miejsca, ani rozkładu.
            None => {
                for (_, vehicle) in &zarezerwowane {
                    self.with_parking(|p| p.release(*vehicle));
                }
                let minuty = self.network_walk_minutes(from, to, speed_pct(who));
                ModeDecision {
                    chosen: TravelOption::Walk,
                    minutes: minuty.max(1),
                    money: Money::ZERO,
                    parking: None,
                    candidates: ArrayVec::new(),
                    reason: DecisionReason::ModeWalkOnly { minutes: minuty },
                }
            }
        }
    }

    fn offer_walk(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        who: &CitizenView<'_>,
    ) -> Result<OptionOffer, Infeasible> {
        // Odległość po chodnikach, nie w linii prostej. Miasto M2 potrafi mieć
        // 2,5-krotne nadłożenie wzdłuż rzeki albo torów, a mnożnik 1,25 z M3 zgadywał
        // tam o połowę za mało — i każde takie dojście kończyło się spóźnieniem.
        let minutes = self.network_walk_minutes(from, to, speed_pct(who));
        if minutes > self.params.max_walk_min {
            return Err(Infeasible::DistanceOverPersonalLimit { minutes });
        }
        Ok(OptionOffer {
            minutes,
            exposed_minutes: minutes,
            ..OptionOffer::default()
        })
    }

    /// Rower: formuła, nie router.
    ///
    /// `ponytail:` sufit nazwany i zmierzony w budżecie: `estimate` woła się ponad
    /// milion razy na dobę metropolii, a zapytanie do warstwy rowerowej kosztuje tyle
    /// samo co pieszej. Rower jest przy tym opcją marginalną (widełki 0,5–12 %), więc
    /// dokładność jego czasu przesuwa rozkład udziałów mniej niż szerokość tych widełek.
    /// Ścieżka wyjścia: ta sama co dla marszu — zapytanie do routera z `RouteCache`,
    /// gdy udział roweru zacznie mieć znaczenie dla bilansu (epoki po 2010).
    fn offer_bike(
        &self,
        a: WorldCoord,
        b: WorldCoord,
        ctx: &ModeContext,
    ) -> Result<OptionOffer, Infeasible> {
        if ctx.age_years < self.params.min_bike_age {
            return Err(Infeasible::BelowMinimumAge);
        }
        // Rower jest **własnością, nie prawem**. Losowanie idzie raz per mieszkaniec
        // (tick 0), więc jest stałe przez całą sesję i niezależne od liczby wątków
        // (00 §3.1) — nie trzeba go nigdzie trzymać ani haszować.
        if !magnat_core::rng(self.seed, magnat_core::StreamId::ModeChoice, ctx.citizen, magnat_core::Tick(0))
            .gen_bool_permille(self.params.bike_ownership_permille)
        {
            return Err(Infeasible::VehicleBroken);
        }
        let dist = manhattan_cm(a, b) * WALK_DETOUR_NUM / DETOUR_DEN;
        let minutes = (dist / BIKE_SPEED_CM_PER_MIN).clamp(1, i64::from(u16::MAX)) as u16;
        if minutes > self.params.max_bike_min {
            return Err(Infeasible::DistanceOverPersonalLimit { minutes });
        }
        Ok(OptionOffer {
            minutes,
            exposed_minutes: minutes,
            ..OptionOffer::default()
        })
    }

    fn offer_transit(
        &self,
        a: WorldCoord,
        b: WorldCoord,
        depart: MinuteOfDay,
        today: i32,
    ) -> Result<OptionOffer, Infeasible> {
        let dow = DayOfWeek::from_day_index(today.max(0) as u64);
        let t = self.transit.lock().expect("transit");
        let Some(j) = t.plan_journey(a, b, depart.get(), dow) else {
            return Err(Infeasible::NoTransitConnection);
        };
        // Obłożenie linii bierze się z kursów w toku: pasażer stojący na przystanku
        // widzi, jak pełne są autobusy, które właśnie przejechały.
        let tloczno = t
            .runs()
            .iter()
            .filter(|r| r.line == j.line && r.capacity > 0)
            .map(|r| u32::from(r.occupancy) * 1_000 / u32::from(r.capacity))
            .max()
            .unwrap_or(0)
            .min(1_000) as u16;
        Ok(OptionOffer {
            minutes: j.total_minutes(),
            money: j.fare,
            exposed_minutes: j.access_min + j.wait_min + j.egress_min,
            crowding_permille: tloczno,
            transfers: j.transfers,
            walk_access_min: j.access_min + j.egress_min,
            parking: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn offer_car(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        a: WorldCoord,
        b: WorldCoord,
        depart: MinuteOfDay,
        ctx: &ModeContext,
        kierowca: Option<DriverEntry>,
        commit: bool,
    ) -> Result<OptionOffer, Infeasible> {
        let Some(d) = kierowca else {
            return Err(Infeasible::NoCarInHousehold);
        };
        if ctx.age_years < self.params.min_drive_age {
            return Err(Infeasible::BelowMinimumAge);
        }
        let dist_cm = manhattan_cm(a, b);
        if dist_cm < i64::from(self.params.min_car_distance_m) * 100 {
            // Poniżej tego dystansu nikt nie wyprowadza auta: manewry, wyjazd
            // i parkowanie zjadają zysk, zanim jeszcze ruszy.
            return Err(Infeasible::DistanceOverPersonalLimit { minutes: 0 });
        }
        if self.busy.lock().expect("busy").get(d.vehicle as usize) == Some(&true) {
            return Err(Infeasible::CarInUseBy(d.citizen));
        }
        // Trasa z routera, nie formuła: plan i przejazd mają mówić o tej samej podróży.
        // Zapytanie idzie przez `RouteCache`, a para dom–praca powtarza się co dobę,
        // więc to jest prawie zawsze trafienie.
        let Some(r) = self.car_route(from, to, depart) else {
            // Trasy samochodem nie ma — 2,8 % par metropolii jej nie ma i to jest
            // prawda o sieci M2, nie błąd routera (`Y-1`).
            return Err(Infeasible::NoRoute);
        };
        let minuty_jazdy =
            (i64::from(r.planned_minutes) * PLANNING_MARGIN_PERMILLE / 1_000).max(1) as u16;

        // Parking u celu — **warunek wykonalności opcji, sprawdzany przy planowaniu**
        // (PRD §9.4, `M-4`). Brak miejsca odbiera opcję, a nie podróż w trakcie.
        let vot = self
            .params
            .vot_gr_per_min(ctx.hourly_net_income_gr, ctx.purpose);
        let do_kiedy = SimMinute(u64::from(depart.get()) + u64::from(minuty_jazdy) + 1);
        let miejsce = self.with_parking(|p| {
            if commit {
                p.find_and_reserve(b, PARKING_SEARCH_RADIUS_M, d.vehicle, vot, do_kiedy)
                    .map(Some)
            } else if p.any_free_within(b, PARKING_SEARCH_RADIUS_M) {
                Ok(None)
            } else {
                Err(crate::parking::ParkingDenied::AllFull { searched: 0 })
            }
        });
        let miejsce = match miejsce {
            Ok(m) => m,
            Err(crate::parking::ParkingDenied::NoLotInRadius) => {
                return Err(Infeasible::NoParkingWithinRadius { lots_searched: 0 })
            }
            Err(crate::parking::ParkingDenied::AllFull { searched }) => {
                return Err(Infeasible::NoParkingWithinRadius {
                    lots_searched: searched,
                })
            }
        };
        let dojscie = miejsce.map_or(0, |s| s.walk_minutes);

        // Pieniądz: paliwo z katalogu klasy plus amortyzacja (`D3` — do decyzji, nie
        // do budżetu) plus opłata parkingowa. Wszystko całkowite (00 §2).
        let spec = self.catalog.spec(d.class);
        let km = dist_cm / 100_000;
        let paliwo_ul = i64::from(spec.base_ml_per_100km) * dist_cm / 10_000;
        let paliwo = self.catalog.fuel_cost(spec.fuel, paliwo_ul);
        let zuzycie = spec.wear_gr_per_100km * km / 100;
        let oplata = miejsce.map_or(0, |s| s.price_gr_per_hour.0 * PARKING_ASSUMED_HOURS);
        Ok(OptionOffer {
            minutes: minuty_jazdy + dojscie,
            money: Money(paliwo.0 + zuzycie + oplata),
            exposed_minutes: dojscie,
            crowding_permille: 0,
            transfers: 0,
            walk_access_min: dojscie,
            parking: miejsce,
        })
    }

    /// Taksówka (`D5`): opcja z ceną z danych, bez encji firmy i bez floty.
    ///
    /// `ponytail:` sufit nazwany — taksówka **nie wjeżdża na sieć**, więc nie tworzy
    /// korka i nie pali paliwa; jej opłata idzie do rejestru przewoźnika, żeby bilans
    /// pieniądza się domykał. Ścieżka wyjścia: M7 stawia operatora z flotą i wtedy
    /// kurs staje się zwykłym `PendingTrip` z pojazdem.
    fn offer_taxi(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        a: WorldCoord,
        b: WorldCoord,
        depart: MinuteOfDay,
        ctx: &ModeContext,
    ) -> Result<OptionOffer, Infeasible> {
        let km = manhattan_cm(a, b) / 100_000;
        let taryfa = self.params.taxi_base_gr + self.params.taxi_gr_per_km * km;
        // **Sama cena taksówki jej nie hamuje.** Dla mieszkańca bez auta i bez zasięgu
        // komunikacji jest ona jedyną szybką opcją, więc koszt uogólniony wybiera ją
        // nawet wtedy, gdy kurs kosztuje dniówkę. Próg budżetowy jest tu tym, czym
        // parking dla samochodu: warunkiem wykonalności, a nie składnikiem kosztu.
        let dzienny = ctx.hourly_net_income_gr * 8;
        if taryfa * 1_000 > dzienny * self.params.taxi_max_fare_permille_of_daily {
            return Err(Infeasible::BeyondBudget {
                fare_gr: taryfa.clamp(0, i64::from(u32::MAX)) as u32,
            });
        }
        let Some(r) = self.car_route(from, to, depart) else {
            return Err(Infeasible::NoRoute);
        };
        let minuty =
            (i64::from(r.planned_minutes) * PLANNING_MARGIN_PERMILLE / 1_000).max(1) as u16;
        Ok(OptionOffer {
            minutes: minuty + self.params.taxi_wait_min,
            money: Money(taryfa),
            exposed_minutes: 0,
            crowding_permille: 0,
            transfers: 0,
            walk_access_min: 0,
            parking: None,
        })
    }

    fn is_watched(&self, citizen: u32) -> bool {
        self.watched.lock().expect("watched").contains(&citizen)
    }
}

impl TrafficOracle {
    fn estimate_inner(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        depart: MinuteOfDay,
        who: &CitizenView<'_>,
    ) -> TravelEstimate {
        let d = self.plan(from, to, who, TripPurpose::Work, depart, false);
        TravelEstimate {
            minutes: d.minutes,
            // Od M4c koszt jest realny: bilet, taryfa, paliwo i amortyzacja policzone
            // w `GeneralizedCost.money_cost`. Planer M3 widzi więc, ile podróż kosztuje,
            // zanim ją zaplanuje — a M5 dostanie tę liczbę do budżetu gospodarstwa.
            cost: d.money,
            mode: d.chosen.mode(),
            reason: d.reason,
        }
    }

    /// Wspólne ciało `begin_trip` — bierze `&self`, bo cała mutacja idzie przez
    /// zamki. Dzięki temu ten sam oracle da się trzymać w `Arc` i sięgać do niego
    /// z systemu ruchu, który potrzebuje routera do wstawienia postoju na stacji.
    pub fn start_trip(
        &self,
        trip: TripRequest,
        who: &CitizenView<'_>,
        q: &mut EventQueue,
    ) -> TripHandle {
        let now = q.now();
        let teraz = MinuteOfDay::new((now % 1440) as u16);
        let decision = self.plan(trip.from, trip.to, who, TripPurpose::Work, teraz, true);
        self.mode_counts[decision.chosen.as_index()].fetch_add(1, Ordering::Relaxed);
        self.policz_niewykonalne(&decision);
        let (minutes, reason) = (decision.minutes, decision.reason);
        let id = self.next_trip.fetch_add(1, Ordering::Relaxed);
        let citizen = trip.traveller.entity().index();

        // Komunikacja: pasażer wchodzi do kolejki przystanku, a `Arrive` wstawi
        // warstwa komunikacji w chwili, gdy wysiądzie (`TransitEvent::Alighted`).
        // Nie planujemy go tutaj, bo przepełniony autobus może go zostawić — i to
        // jest cała treść przepełnienia (§5.6).
        if decision.chosen == TravelOption::Transit {
            let a = self.coord_of(trip.from);
            let b = self.coord_of(trip.to);
            let dow = DayOfWeek::from_day_index(who.today.max(0) as u64);
            let podroz = self
                .transit
                .lock()
                .expect("transit")
                .plan_journey(a, b, teraz.get(), dow);
            if let Some(j) = podroz {
                // Ścieżka odwrotu liczona **przed** wejściem do kolejki: gdy pasażer
                // się podda, ma dojść pieszo, a nie znaleźć się na miejscu.
                let pieszo = self.network_walk_minutes(trip.from, trip.to, speed_pct(who));
                self.transit
                    .lock()
                    .expect("transit")
                    .enqueue(&j, citizen, trip.slot, now, pieszo);
                return TripHandle {
                    id,
                    from: trip.from,
                    to: trip.to,
                    arrive_at: now + u32::from(minutes),
                    minutes,
                    mode: TransportMode::Bus,
                };
            }
        }

        // Taksówka: pasażer dociera w czasie planowanym i płaci taryfę. Kurs nie
        // wjeżdża na sieć (`D5`), więc zdarzenie przybycia stawia się tutaj.
        if decision.chosen == TravelOption::Taxi {
            let arrive_at = now + u32::from(minutes);
            self.taxi_fares
                .fetch_add(decision.money.0.max(0) as u64, Ordering::Relaxed);
            q.schedule(SimEvent::new(
                arrive_at,
                citizen,
                EventKind::Arrive,
                trip.slot,
            ));
            return TripHandle {
                id,
                from: trip.from,
                to: trip.to,
                arrive_at,
                minutes,
                mode: TransportMode::Car,
            };
        }

        // Samochód: trasa idzie na sieć, a `Arrive` wstawi mezo w minucie faktycznego
        // przyjazdu. Brak trasy odbiera opcję **przy planowaniu** (`Y-1`) — mieszkaniec
        // idzie pieszo, a nie „podróż się nie udała".
        let mode = decision.chosen.mode();
        if decision.chosen.needs_parking() {
            let kierowca = match decision.chosen {
                TravelOption::CarOwn => self.driver_of(citizen),
                _ => self.household_car(who.identity.household, citizen),
            };
            if let Some(driver) = kierowca.filter(|d| self.take_vehicle(d.vehicle)) {
                if let Some(route) = self.car_route(trip.from, trip.to, teraz) {
                    let mut kolejka = self.pending.lock().expect("pending");
                    // Zlecenia odbiera **wyłącznie** `TrafficSystem`. Harmonogram bez
                    // niego nie kompiluje się inaczej ani nie pada — po prostu nikt
                    // nigdy nie dojeżdża, a auta zostają zajęte na zawsze. Rosnąca
                    // kolejka jest jedynym objawem, więc to ona ma krzyknąć.
                    debug_assert!(
                        kolejka.len() < 100_000,
                        "{} niewykonanych zleceń przejazdu — czy `TrafficSystem`                          jest w harmonogramie?",
                        kolejka.len()
                    );
                    kolejka.push(PendingTrip {
                        trip: TripId(id),
                        traveller: citizen,
                        vehicle: driver.vehicle,
                        household: who.identity.household,
                        class: driver.class,
                        slot: trip.slot,
                        origin: trip.from,
                        dest: trip.to,
                        depart: SimMinute(u64::from(now)),
                        planned_minutes: minutes,
                        purpose: TripPurpose::Work,
                        load: Mass::ZERO,
                        legs: vec![route],
                        station: None,
                        tank_level_ul: 0,
                        tank_capacity_ul: 0,
                        reason,
                        traced: self.is_watched(citizen),
                    });
                    drop(kolejka);
                    return TripHandle {
                        id,
                        from: trip.from,
                        to: trip.to,
                        arrive_at: now + u32::from(minutes),
                        minutes,
                        mode: TransportMode::Car,
                    };
                }
                // Trasy nie ma (`Y-1`) — auto wraca do puli i mieszkaniec idzie pieszo.
                self.release_vehicle(driver.vehicle);
            }
            // Auto odpadło po wyborze — zwolnione miejsce postojowe wraca do puli,
            // inaczej stałoby zajęte przez pojazd, który nigdzie nie pojechał.
            if let Some(p) = decision.parking {
                let _ = p;
                if let Some(k) = match decision.chosen {
                    TravelOption::CarOwn => self.driver_of(citizen),
                    _ => self.household_car(who.identity.household, citizen),
                } {
                    self.with_parking(|p| p.release(k.vehicle));
                }
            }
        }

        // Pieszo i rowerem: ścieżka M3 bez zmian. Ani pieszy, ani rowerzysta nie tworzy
        // korka i nie wchodzi na sieć drogową.
        let (mode, minutes) = if decision.chosen.needs_parking() || mode == TransportMode::Car {
            // Trasy samochodem nie ma albo auto jest w innej podróży — mieszkaniec
            // idzie pieszo i liczymy mu to uczciwie, po chodnikach.
            (
                TransportMode::Walk,
                self.network_walk_minutes(trip.from, trip.to, speed_pct(who)),
            )
        } else {
            (mode, minutes)
        };
        let arrive_at = now + u32::from(minutes);
        q.schedule(SimEvent::new(
            arrive_at,
            citizen,
            EventKind::Arrive,
            trip.slot,
        ));
        TripHandle {
            id,
            from: trip.from,
            to: trip.to,
            arrive_at,
            minutes,
            mode,
        }
    }

    fn places_on_route_inner(&self, trip: &TripHandle, out: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>) {
        // Miejsca „po drodze" liczy się w korytarzu odcinka skąd–dokąd. Dla M4b
        // to ten sam model, co w M3: pełny korytarz po realnej trasie wymaga
        // geometrii krawędzi, a ta jest po stronie `sim/world` (`J-3`).
        out.clear();
        let a = self.coord_of(trip.from);
        let b = self.coord_of(trip.to);
        let srodek = WorldCoord::new((a.x + b.x) / 2, (a.y + b.y) / 2, (a.z + b.z) / 2);
        let promien = (euclid_cm(a, b) as f32 / 200.0).max(50.0);
        for kind in magnat_core::PlaceKind::ALL {
            if out.is_full() {
                break;
            }
            self.places.for_each_near(*kind, srodek, promien, |e| {
                if !out.is_full() {
                    let _ = out.push(e.place);
                }
            });
        }
    }

    fn enter_micro_inner(&self, handle: &TripHandle, citizen: u32, depart: MinuteOfDay) {
        if handle.mode != TransportMode::Walk {
            return;
        }
        let route = [self.coord_of(handle.from), self.coord_of(handle.to)];
        self.micro.enter(
            citizen,
            &route,
            depart.get(),
            (u32::from(depart.get()) + u32::from(handle.minutes)).min(1_439) as u16,
        );
    }

}

/// **Oracle trzyma stan symulacji i ten stan musi wchodzić do hasha** (00 §3.6).
///
/// Do M4b oracle był traktowany jak dane wejściowe miasta i nie haszowano go wcale —
/// a już wtedy trzymał `busy`, czyli informację o tym, które auto jest w podróży,
/// która wpływa na plan mieszkańca. M4c dokłada do niego parkingi, komunikację
/// i nawyk, więc luka przestała być teoretyczna. Haszujemy **stan**, nie dane:
/// graf, katalog pojazdów i tabela VDF zostają poza, bo są wejściem, nie wynikiem.
///
/// `TravelTimeMatrix` też jest tutaj i to jest rozstrzygnięcie `M-2`: macierz karmiona
/// obserwacjami z ledgera **jest stanem symulacji** (wpływa na plan, plan na świat),
/// więc albo wchodzi do hasha, albo nie wolno jej karmić. Wchodzi.
impl HashState for TrafficOracle {
    fn hash_state(&self, h: &mut StateHasher) {
        for b in self.busy.lock().expect("busy").iter() {
            h.write_u8(u8::from(*b));
        }
        self.parking.lock().expect("parking").hash_state(h);
        self.transit.lock().expect("transit").hash_state(h);
        for m in self.habit.lock().expect("habit").iter() {
            h.write_u8(*m);
        }
        h.write_u64(self.taxi_fares.load(Ordering::Relaxed));
        self.router
            .lock()
            .expect("router")
            .router
            .matrix()
            .hash_state(h);
    }
}

/// Uchwyt współdzielony. `Sources.travel` dostaje właśnie ten typ, a `TrafficSystem`
/// trzyma drugi `Arc` na ten sam oracle — inaczej system nie miałby jak dopytać
/// router o objazd na stację, bo `Box<dyn TravelOracle>` nie da się rzutować w dół.
///
/// Newtype, a nie `impl TravelOracle for Arc<TrafficOracle>`: `Arc` nie jest typem
/// fundamentalnym, więc reguła sieroty na to nie pozwala.
#[derive(Clone)]
pub struct OracleHandle(pub Arc<TrafficOracle>);

impl TravelOracle for OracleHandle {
    fn estimate(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        depart: MinuteOfDay,
        who: &CitizenView<'_>,
    ) -> TravelEstimate {
        self.0.estimate_inner(from, to, depart, who)
    }

    fn begin_trip(
        &mut self,
        trip: TripRequest,
        who: &CitizenView<'_>,
        q: &mut EventQueue,
    ) -> TripHandle {
        self.0.start_trip(trip, who, q)
    }

    fn places_on_route(&self, trip: &TripHandle, out: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>) {
        self.0.places_on_route_inner(trip, out);
    }

    fn enter_micro(&self, handle: &TripHandle, traveller: u32, depart: MinuteOfDay) {
        self.0.enter_micro_inner(handle, traveller, depart);
    }

    fn micro_step(&self, now_ms: u64) {
        self.0.micro.step(now_ms);
    }

    fn micro_retire(&self, now_min: u16) {
        self.0.micro.retire(now_min);
    }

    fn micro_len(&self) -> usize {
        self.0.micro.len()
    }

    fn set_micro_window(&self, center: Option<(i32, i32)>, radius_m: u32) {
        self.0.micro.set_window(center, radius_m);
    }

    fn micro_snapshot(&self, out: &mut Vec<magnat_sim_snapshot::PedestrianRecord>) {
        self.0.micro.snapshot(out);
    }
}

// ── pomocnicze ──────────────────────────────────────────────────────────────────

fn build_index(
    nodes: &[WorldCoord],
    it: impl Iterator<Item = (WorldCoord, u32)>,
) -> CsrGrid<u32> {
    let spec = if nodes.is_empty() {
        GridSpec::new(Vec2::new(0.0, 0.0), NODE_CELL_M, 1, 1)
    } else {
        let mut min = Vec2::new(f32::MAX, f32::MAX);
        let mut max = Vec2::new(f32::MIN, f32::MIN);
        for p in nodes {
            let v = coord_to_vec2(*p);
            min = Vec2::new(min.x.min(v.x), min.y.min(v.y));
            max = Vec2::new(max.x.max(v.x), max.y.max(v.y));
        }
        GridSpec::covering(Aabb2::new(min, max), NODE_CELL_M)
    };
    CsrGrid::build(spec, it.map(|(p, i)| (coord_to_vec2(p), i)))
}

fn nearest(index: &CsrGrid<u32>, nodes: &[WorldCoord], at: WorldCoord) -> Option<u32> {
    if nodes.is_empty() {
        return None;
    }
    let mut buf: Vec<(f32, u32)> = Vec::with_capacity(4);
    index.k_nearest(coord_to_vec2(at), 4, &mut buf);
    buf.iter()
        .map(|(_, i)| (euclid_cm(at, nodes[*i as usize]), *i))
        .min()
        .map(|(_, i)| i)
}

#[inline]
fn coord_to_vec2(c: WorldCoord) -> Vec2 {
    Vec2::new(c.x as f32 / 100.0, c.y as f32 / 100.0)
}

#[inline]
fn manhattan_cm(from: WorldCoord, to: WorldCoord) -> i64 {
    i64::from((to.x - from.x).abs()) + i64::from((to.y - from.y).abs())
}

#[inline]
fn euclid_cm(a: WorldCoord, b: WorldCoord) -> u32 {
    let d = a.distance_sq_xy(b);
    (d as f64).sqrt() as u32
}

/// Czas marszu dla przebytej odległości. Wzór z M3 bez zmiany — patrz `WALK_SPEED_CM_PER_MIN`.
#[must_use]
pub fn walk_minutes_for(dist_cm: i64, speed_pct: u32) -> u16 {
    let v = WALK_SPEED_CM_PER_MIN * i64::from(speed_pct) / 100;
    let m = dist_cm / v.max(1);
    m.clamp(1, i64::from(u16::MAX)) as u16
}

/// Prędkość marszu mieszkańca jako procent bazowej (M3 §5.10: wiek, zdrowie, energia).
/// Przeniesiona z `sim/agents::walk` bez zmiany progów.
#[must_use]
pub fn speed_pct(who: &CitizenView<'_>) -> u32 {
    let lata = ((who.today - who.identity.birth_day).max(0) / 360) as u32;
    let mut p: i32 = match lata {
        0..=5 => 60,
        6..=13 => 85,
        14..=64 => 100,
        65..=79 => 85,
        _ => 65,
    };
    if who.vitals.health < 40 {
        p -= 15;
    }
    if who.vitals.energy < 30 {
        p -= 10;
    }
    p.clamp(45, 110) as u32
}
