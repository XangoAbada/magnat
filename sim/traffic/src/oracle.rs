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
use crate::spec::{VehicleCatalog, VehicleClassId};
use crate::trip::{PendingTrip, TripId, TripPurpose};
use magnat_agents::{
    ArrayVec, CitizenView, EventKind, EventQueue, PlaceTable, SimEvent, TravelEstimate,
    TravelOracle, TripHandle, TripRequest, MAX_ON_ROUTE,
};
use magnat_core::{
    DecisionReason, Mass, MinuteOfDay, Money, PlaceRef, SimMinute, TransportMode, WorldCoord,
};
use magnat_nav::{NavRouter, NodeId, RouteQuery, Router};
use magnat_spatial::{Aabb2, CsrGrid, GridSpec, Vec2};
use std::sync::atomic::{AtomicU32, Ordering};
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

/// Poniżej tego dystansu kierowca i tak idzie pieszo — wyprowadzenie auta,
/// zaparkowanie i dojście zjadają zysk. Pełny koszt uogólniony liczy M4c/WP6.
const MIN_CAR_DISTANCE_CM: i64 = 80_000;

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
    micro: MicroLayer,
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
            micro: MicroLayer::new(),
        }
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

    /// Szacunek czasu i środka — wspólny dla `estimate` i `begin_trip`, żeby plan
    /// i podróż mówiły o tej samej liczbie.
    fn plan(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        who: &CitizenView<'_>,
    ) -> (TransportMode, u16, DecisionReason) {
        let a = self.coord_of(from);
        let b = self.coord_of(to);
        let prosto = manhattan_cm(a, b);
        let driver = self.driver_of(who.id.entity().index());
        if driver.is_some() && prosto >= MIN_CAR_DISTANCE_CM {
            // Trasa z routera, nie formuła: plan i przejazd mają mówić o tej samej
            // podróży, inaczej każdy dojazd jest spóźniony i mieszkaniec przeplanowuje
            // dobę bez przerwy. Zapytanie idzie przez `RouteCache`, a para dom–praca
            // powtarza się co dobę, więc to jest prawie zawsze trafienie.
            if let Some(r) = self.car_route(from, to, MinuteOfDay::MIDNIGHT) {
                let m = i64::from(r.planned_minutes) * PLANNING_MARGIN_PERMILLE / 1_000;
                let minutes = m.clamp(1, i64::from(u16::MAX)) as u16;
                return (
                    TransportMode::Car,
                    minutes,
                    DecisionReason::ModeChosen {
                        mode: TransportMode::Car,
                        minutes,
                    },
                );
            }
            // Trasy samochodem nie ma — 2,8 % par metropolii jej nie ma i to jest
            // prawda o sieci M2, nie błąd routera (`Y-1`). Mieszkaniec dostaje
            // uzasadnienie, a nie porażkę przejazdu.
            let minuty = self.network_walk_minutes(from, to, speed_pct(who));
            return (
                TransportMode::Walk,
                minuty,
                DecisionReason::NoRouteForMode {
                    mode: TransportMode::Car,
                    fallback: TransportMode::Walk,
                },
            );
        }
        // Pieszo: odległość po chodnikach, nie w linii prostej. Miasto M2 potrafi mieć
        // 2,5-krotne nadłożenie wzdłuż rzeki albo torów, a mnożnik 1,25 z M3 zgadywał
        // tam o połowę za mało — i każde takie dojście kończyło się spóźnieniem.
        let minuty = self.network_walk_minutes(from, to, speed_pct(who));
        (
            TransportMode::Walk,
            minuty,
            DecisionReason::ModeWalkOnly { minutes: minuty },
        )
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
        _depart: MinuteOfDay,
        who: &CitizenView<'_>,
    ) -> TravelEstimate {
        let (mode, minutes, reason) = self.plan(from, to, who);
        TravelEstimate {
            minutes,
            // Koszt pieniężny podróży powstaje przy tankowaniu, nie przy planowaniu:
            // paliwo kupuje się na stacji, a nie na drodze. Pełny koszt uogólniony
            // (czas × wartość czasu + dyskomfort) liczy M4c/WP6.
            cost: Money::ZERO,
            mode,
            reason,
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
        let (mode, minutes, reason) = self.plan(trip.from, trip.to, who);
        let id = self.next_trip.fetch_add(1, Ordering::Relaxed);
        let now = q.now();
        let citizen = trip.traveller.entity().index();

        // Samochód: trasa idzie na sieć, a `Arrive` wstawi mezo w minucie faktycznego
        // przyjazdu. Brak trasy odbiera opcję **przy planowaniu** (`Y-1`) — mieszkaniec
        // idzie pieszo, a nie „podróż się nie udała".
        if mode == TransportMode::Car {
            if let Some(driver) = self.driver_of(citizen).filter(|d| self.take_vehicle(d.vehicle)) {
                if let Some(route) =
                    self.car_route(trip.from, trip.to, MinuteOfDay::new((now % 1440) as u16))
                {
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
        }

        // Pieszo: ścieżka M3 bez zmian. Pieszy nie tworzy korka i nie wchodzi na sieć.
        let minutes = if mode == TransportMode::Car {
            // Trasy samochodem nie ma albo auto jest w innej podróży — mieszkaniec
            // idzie pieszo i liczymy mu to uczciwie, po chodnikach.
            self.network_walk_minutes(trip.from, trip.to, speed_pct(who))
        } else {
            minutes
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
            mode: TransportMode::Walk,
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

    fn micro_snapshot(&self, out: &mut Vec<(u32, [f32; 3], f32)>) {
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
