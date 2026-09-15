//! Podróż: zlecenie, rejestr przejazdu i sieć podróży w toku (M4b/WP4).
//!
//! **Kto decyduje o minucie przybycia.** W M3 decydował `TravelOracle`: liczył czas
//! marszu i od razu wstawiał `Arrive` do kolejki DES. Od M4b dla podróży samochodem
//! decyduje **mezo**: `begin_trip` zwraca `TripHandle` z czasem *planowanym* (tym samym,
//! który zwróciłby `estimate`, bo planer musi dostać tę samą liczbę co przy układaniu
//! planu), a zdarzenie `Arrive` wstawia dopiero ten krok minutowy, w którym pojazd
//! faktycznie dojechał. Różnica między jednym a drugim to spóźnienie — i to jest cały
//! mechanizm, przez który korek dociera do mieszkańca (`ReplanCause::Late`).
//!
//! Podróże piesze zostają przy ścieżce M3: pieszy nie tworzy korka, a przepuszczanie
//! 274 tys. mieszkańców przez kolejki krawędzi kosztowałoby budżet, którego broni §7.3.

use crate::mezo::{settle_edge, settle_node, turn_priority, LedgerEntry, MezoState, VehicleSpecRef, CS_PER_MINUTE};
use crate::spec::{VdfTable, VehicleCatalog, VehicleClassId};
use magnat_core::{
    DecisionReason, HashState, Mass, Money, PlaceRef, SimMinute, StateHasher, TransportMode,
    WorldCoord,
};
use magnat_nav::{EdgeId, RoadGraph, Route};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;

/// Identyfikator podróży. Numeracja jest ciągła i rośnie przez całą grę — uchwyt
/// zwolniony nigdy nie wraca, tak samo jak w arenach (`K-16`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct TripId(pub u32);

/// Po co mieszkaniec jedzie. Wchodzi do wartości czasu (`vot_gr_per_min`) w M4c.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TripPurpose {
    Work = 0,
    School = 1,
    Shopping = 2,
    Refuel = 3,
    Leisure = 4,
    Medical = 5,
    Escort = 6,
}

/// Dlaczego podróż się nie udała.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TripFailure {
    NoRoute = 0,
    NoFuel = 1,
    VehicleBroken = 2,
    /// Sieć nie przepuściła pojazdu w rozsądnym czasie — zawór bezpieczeństwa R4.
    Gridlock = 3,
}

/// Wynik podróży — kontrakt z M3, M5, M6 i M7 (M4 §6).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TripOutcome {
    Arrived { at: SimMinute, ledger: TripLedger },
    Failed { at: SimMinute, reason: TripFailure },
}

/// Rejestr przejazdu — oś czasu podróży w karcie inspekcji (§14.4) i podstawa księgowania.
///
/// `entries` wypełnia się **tylko dla śledzonych** mieszkańców. Podróż metropolii
/// dotyka średnio ~160 krawędzi; 12 tys. pojazdów naraz razy 160 wpisów po 32 B to
/// 61 MB rejestru, którego nikt nie czyta. Sumy są zawsze — to one wchodzą do pieniądza.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TripLedger {
    pub trip: TripId,
    pub entries: Vec<LedgerEntry>,
    /// Paliwo w mikrolitrach (patrz nagłówek `mezo`).
    pub total_fuel_ul: i64,
    pub total_money: Money,
    pub edges: u32,
    pub stops: u32,
    pub distance_cm: u64,
    pub depart: SimMinute,
    pub arrive: SimMinute,
    pub mode: u8,
    /// **Powód, który karta inspekcji pokaże dla tej podróży** (00 §7, bramka 5).
    /// Jeden na przejazd i wybrany po ważności: tankowanie przesłania spóźnienie,
    /// spóźnienie przesłania sam wybór środka — bo to o tym mieszkaniec chce wiedzieć.
    pub reason: DecisionReason,
}

impl Default for TripLedger {
    fn default() -> TripLedger {
        TripLedger {
            trip: TripId(0),
            entries: Vec::new(),
            total_fuel_ul: 0,
            total_money: Money::ZERO,
            edges: 0,
            stops: 0,
            distance_cm: 0,
            depart: SimMinute(0),
            arrive: SimMinute(0),
            mode: TransportMode::Car as u8,
            reason: DecisionReason::Unspecified,
        }
    }
}

impl TripLedger {
    #[must_use]
    pub fn minutes(&self) -> u16 {
        self.arrive.0.saturating_sub(self.depart.0).min(u64::from(u16::MAX)) as u16
    }

    #[must_use]
    pub fn transport_mode(&self) -> TransportMode {
        TransportMode::from_index(self.mode as usize).unwrap_or(TransportMode::Car)
    }
}

/// Zlecenie przejazdu gotowe do wpuszczenia na sieć. Powstaje w `TravelOracle::begin_trip`
/// (tam jest router), konsumuje je krok minutowy (tu jest sieć).
#[derive(Clone)]
pub struct PendingTrip {
    pub trip: TripId,
    pub traveller: u32,
    pub vehicle: u32,
    pub household: u32,
    pub class: VehicleClassId,
    pub slot: u8,
    pub origin: PlaceRef,
    pub dest: PlaceRef,
    pub depart: SimMinute,
    pub planned_minutes: u16,
    pub purpose: TripPurpose,
    pub load: Mass,
    /// Odcinki trasy. Dwa, gdy po drodze jest tankowanie: dom → stacja → cel.
    pub legs: Vec<Arc<Route>>,
    /// Stacja na końcu odcinka 0; `None` = bez tankowania.
    pub station: Option<PlaceRef>,
    /// Stan zbiornika odczytany z komponentu w chwili wyruszenia. Podróż niesie go
    /// ze sobą, bo w ruchu nikt inny do baku nie sięga — pojazd nie jest wtedy
    /// zaparkowany, więc nie jest opcją dla żadnej innej podróży. Dzięki temu
    /// zatankowanie da się policzyć w całości po stronie sieci, bez drugiego
    /// źródła prawdy o paliwie.
    pub tank_level_ul: i64,
    pub tank_capacity_ul: i64,
    /// Uzasadnienie wyboru środka transportu, gotowe do karty inspekcji (00 §7).
    pub reason: DecisionReason,
    pub traced: bool,
}

/// Podróż w toku. Żyje w tablicy sieci, nie w ECS — jest krótkotrwała i dociera się
/// do niej zawsze przez pojazd albo przez kopiec odjazdów.
#[derive(Clone, PartialEq, Eq, Debug)]
struct ActiveTrip {
    trip: TripId,
    traveller: u32,
    vehicle: u32,
    household: u32,
    class: VehicleClassId,
    slot: u8,
    dest: PlaceRef,
    station: Option<PlaceRef>,
    edges: Vec<EdgeId>,
    /// Indeks krawędzi kończącej odcinek 0 (tankowanie). `u32::MAX` = bez tankowania.
    refuel_after: u32,
    pos: u32,
    entry_cs: u64,
    exit_cs: u64,
    depart: SimMinute,
    planned_minutes: u16,
    load: Mass,
    fuel_ul: i64,
    distance_cm: u64,
    tank_level_ul: i64,
    tank_capacity_ul: i64,
    bought_ul: i64,
    money: Money,
    stops: u32,
    blocked_since: u32,
    /// Uzasadnienie wyboru środka; przy tankowaniu przesłania je `station_reason`.
    reason: DecisionReason,
    station_reason: Option<DecisionReason>,
    /// Nastepna podroz w kolejce oczekujacych na te sama krawedz; `NO_WAIT` = ostatnia.
    wait_next: u32,
    /// Opóźnienie węzła i czekanie na wjazd, policzone **przed** wjazdem na krawędź
    /// i doliczone do jej wiersza w rejestrze, gdy ten powstanie (`N-6`). Bez tego
    /// karta inspekcji musiałaby liczyć czas kolejki jako resztę z odejmowania.
    pending_node_cs: u32,
    pending_blocked_cs: u32,
    entries: Vec<LedgerEntry>,
    traced: bool,
}

/// Co krok minutowy ma do przekazania światu. Sieć nie dotyka ECS sama —
/// zwraca zdarzenia, a system je stosuje. Dzięki temu krok jest testowalny bez świata.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TrafficEvent {
    /// Pojazd zatankował do pełna. `units` w mikrolitrach.
    Refuelled {
        trip: TripId,
        traveller: u32,
        vehicle: u32,
        household: u32,
        station: PlaceRef,
        /// Ile dolano, w mikrolitrach.
        units: i64,
        cost: Money,
        at: SimMinute,
    },
    Arrived {
        trip: TripId,
        traveller: u32,
        vehicle: u32,
        slot: u8,
        dest: PlaceRef,
        at: SimMinute,
        planned_minutes: u16,
        /// Poziom zbiornika po podróży: `stan początkowy − spalone + zatankowane`.
        tank_level_ul: i64,
        ledger: TripLedger,
    },
    Failed {
        trip: TripId,
        traveller: u32,
        vehicle: u32,
        slot: u8,
        dest: PlaceRef,
        at: SimMinute,
        reason: TripFailure,
        tank_level_ul: i64,
        distance_cm: u64,
    },
}

/// Licznik zbiorczy — wejście raportów i testów własnościowych.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TrafficStats {
    pub dispatched: u64,
    pub arrived: u64,
    pub failed: u64,
    pub failed_gridlock: u64,
    /// Zawracania na trasie: router ich nie widzi, mezo je odgrywa i liczy
    /// (patrz [`crate::mezo::turn_priority`]).
    pub u_turns: u64,
    pub max_blocked_minutes: u64,
    pub max_edges_per_trip: u64,
    pub entries: u64,
    pub exits: u64,
    pub edge_settles: u64,
    pub spillbacks: u64,
    pub gridlock_releases: u64,
    pub refuels: u64,
    pub fuel_burned_ul: i64,
    pub fuel_bought_ul: i64,
    pub fuel_spent: Money,
    pub late_arrivals: u64,
    pub late_minutes: u64,
    /// Ile podróży zakończyło się którym uzasadnieniem — wejście bramki 5 (00 §7).
    /// Kolejność: `ModeChosen`, `ModeCompared`, `NoRouteForMode`,
    /// `NoParkingAtDestination`, `RefuelNeeded`, `StationChosen`, `TripDelayed`, inne.
    ///
    /// Ostatni kubełek musi zostać **pusty**: podróż bez uzasadnienia łamie bramkę 5
    /// z §7.4 i kryterium zamknięcia M4c („100 % decyzji transportowych ma uzasadnienie").
    pub reasons: [u64; 8],
    /// Suma czasów planowanych i faktycznych — z ilorazu bierze się narzut
    /// planistyczny, który `TrafficOracle` dokłada do czasu swobodnego z routera.
    pub planned_minutes_total: u64,
    pub actual_minutes_total: u64,
    /// Rozbiór czasu przejazdu na trzy składniki, w setnych sekundy — to z niego
    /// widać, czy miasto stoi w korku, na skrzyżowaniach, czy po prostu jedzie.
    pub travel_cs_total: u64,
    pub node_delay_cs_total: u64,
    pub blocked_cs_total: u64,
}

/// Ile minut trwa tankowanie razem z kolejką do dystrybutora.
///
/// `ponytail:` stała zamiast modelu kolejkowego M/M/c z §5.7. Sufit nazwany:
/// stacja nie rozróżnia szczytu od nocy. Ścieżka wyjścia: obłożenie stacji jest
/// liczone w M4c razem z parkingami (WP7) i wchodzi tu jako drugi składnik.
pub const REFUEL_DWELL_MIN: u32 = 6;

/// Ile minut krawędź może być pełna **i nieruchoma**, zanim sieć wpuści na nią
/// pojazd wbrew pojemności.
///
/// Warunkiem jest bezruch krawędzi docelowej, a nie czas oczekiwania pojazdu:
/// kolejka przed przewężeniem potrafi trwać kwadrans i jest zjawiskiem pożądanym,
/// więc rozplątywanie jej „bo długo czeka" skasowałoby korek, czyli dokładnie to,
/// co faza ma pokazać.
///
/// `ponytail:` zawór lokalny zamiast detektora cyklu blokad (ryzyko R4). Sufit
/// nazwany: rozplątanie nie wie, że rozwiązuje cykl, i wpuszcza jeden pojazd
/// ponad pojemność. Ścieżka wyjścia: detektor cyklu per minuta w WP8, gdy mikro
/// i tak będzie liczyć zależności między krawędziami.
pub const GRIDLOCK_RELEASE_MIN: u32 = 3;

/// Twardy sufit czasu podróży: wielokrotność planu plus stała. Po jego przekroczeniu
/// podróż kończy się `TripFailure::Gridlock`, żeby mieszkaniec nie utknął na zawsze.
pub const TRIP_TIMEOUT_FACTOR: u32 = 6;
pub const TRIP_TIMEOUT_EXTRA_MIN: u32 = 120;

/// Ile przejść przez krawędzie sieć wykonuje najwyżej w jednej minucie **łącznie**.
/// Metropolia w szczycie potrzebuje ~100 tys.; limit chroni przed pętlą, nie przed
/// normalnym ruchem, a to, czego nie zdąży, czeka w kopcu do następnej minuty.
const MAX_EDGE_STEPS_PER_MINUTE: u64 = 4_000_000;

/// Sieć podróży w toku plus stan mezo. To jest zasób ECS fazy i to on wchodzi do hasha.
///
/// `PartialEq` porównuje stan, a nie kopiec: kolejka odjazdów jest w całości
/// wyprowadzalna z `exit_cs` żywych podróży, więc dwie sieci o tych samych
/// podróżach są tym samym stanem niezależnie od układu kopca.
#[derive(Clone, Debug, Default)]
pub struct TrafficNetwork {
    pub mezo: MezoState,
    pub stats: TrafficStats,
    trips: Vec<Option<ActiveTrip>>,
    free: Vec<u32>,
    /// Klucz totalny `(setna sekundy wyjazdu, indeks encji pojazdu)` — remisy
    /// niemożliwe, bo pojazd jest w jednej podróży naraz (§5.9 punkt 1).
    ///
    /// Rozdzielczość jest **setną sekundy, nie minutą**, i to nie jest szczegół:
    /// przy kluczu minutowym wszystkie pojazdy z tej samej minuty szłyby w kolejności
    /// indeksu, a każdy przejeżdżałby całą swoją trasę, zanim ruszy następny. Krawędź
    /// nigdy nie miałaby wtedy dwóch pojazdów naraz i **korek nie mógłby powstać**,
    /// bo obłożenie mierzy obecność równoczesną.
    heap: BinaryHeap<Reverse<(u64, u32, u32)>>,
    /// Krawedzie z niepusta kolejka oczekujacych. Krotka lista -- tylko przewezenia.
    waiting_edges: Vec<u32>,
    /// Ile razy każda krawędź odmówiła wjazdu przez całą sesję. Licznik diagnostyczny
    /// dla nakładki korków i dla raportu — **nie wchodzi do hasha**, bo nie jest stanem
    /// symulacji, tylko jej pomiarem.
    rejects_total: Vec<u32>,
}

impl PartialEq for TrafficNetwork {
    fn eq(&self, other: &TrafficNetwork) -> bool {
        self.mezo == other.mezo && self.trips == other.trips && self.stats == other.stats
    }
}

impl Eq for TrafficNetwork {}

impl TrafficNetwork {
    #[must_use]
    pub fn new(road: &RoadGraph, vdf: &VdfTable) -> TrafficNetwork {
        TrafficNetwork {
            mezo: MezoState::new(road, vdf),
            rejects_total: vec![0; road.edge_count()],
            ..TrafficNetwork::default()
        }
    }

    #[must_use]
    pub fn active(&self) -> usize {
        self.trips.iter().filter(|t| t.is_some()).count()
    }

    /// Krawędź, która odmówiła wjazdu najczęściej — pierwszy adres, pod który idzie
    /// się z pytaniem „gdzie stoi to miasto".
    #[must_use]
    pub fn worst_bottleneck(&self) -> Option<(EdgeId, u32)> {
        self.rejects_total
            .iter()
            .enumerate()
            .max_by_key(|(i, r)| (**r, std::cmp::Reverse(*i)))
            .filter(|(_, r)| **r > 0)
            .map(|(i, r)| (EdgeId(i as u32), *r))
    }

    /// Zasila warstwę Mikro pojazdami będącymi w tej chwili na krawędziach (WP8).
    ///
    /// To jest **jedyny** kanał mezo → mikro i idzie w jedną stronę: warstwa dostaje
    /// krawędź, na której pojazd stoi, oraz parę `(entry_cs, exit_cs)` policzoną przez
    /// `settle_edge`. Kopii czasu przybycia nie ma nigdzie — mikro czyta tę wartość
    /// i domyka do niej serwo (`N-1`), bo druga kopia byłaby drugim źródłem prawdy
    /// o tym, kiedy pojazd dojedzie, czyli dokładnie tym, przed czym broni §5.4.
    ///
    /// Bramka okna i sufit rysowania siedzą po stronie `MicroLayer`, więc przy
    /// zamkniętym kadrze (headless) ta pętla kosztuje jedno sprawdzenie na pojazd.
    pub fn feed_micro(&self, micro: &crate::micro::MicroLayer, road: &RoadGraph, cat: &VehicleCatalog) {
        for t in self.trips.iter().flatten() {
            let Some(e) = t.edges.get(t.pos as usize).copied() else {
                continue;
            };
            let edge = road.edge(e);
            let a = road.nodes[edge.from.0 as usize];
            let b = road.nodes[edge.to.0 as usize];
            let spec = cat.spec(t.class);
            let trasa = [
                WorldCoord::new(a.pos_cm.x, a.pos_cm.y, a.z_cm),
                WorldCoord::new(b.pos_cm.x, b.pos_cm.y, b.z_cm),
            ];
            // Prędkość swobodna bryły: limit krawędzi przycięty prędkością maksymalną
            // klasy. Decykilometr na godzinę to 100 000 cm / 3 600 s / 10 = 2,778 cm/s.
            let dkmh = u32::from(
                edge.free_speed_dkmh(magnat_nav::Modality::Road)
                    .min(spec.top_speed_dkmh)
                    .max(1),
            );
            micro.feed_vehicle(
                crate::micro::VehicleFeed {
                    vehicle: t.vehicle,
                    edge: e.0,
                    class: t.class.0 as u8,
                    lanes: edge.lanes,
                    len_cm: spec.length_cm,
                    v_free_cms: dkmh as f32 * 100.0 / 36.0,
                    entry_cs: t.entry_cs,
                    exit_cs: t.exit_cs,
                    car_following: true,
                },
                &trasa,
            );
        }
    }

    /// Niezmiennik zachowania: pojazdów na sieci == wjazdy − wyjazdy.
    #[must_use]
    pub fn conserved(&self) -> bool {
        self.mezo.vehicles_on_network() == self.stats.entries - self.stats.exits
    }

    /// Krok minutowy warstwy mezo. Zwraca zdarzenia do zastosowania w świecie.
    ///
    /// Kolejność jest w całości wyznaczona przez klucz kopca i przez indeks krawędzi —
    /// nigdzie nie pytamy o zegar, wątek ani kolejność ukończenia jobów (§5.9).
    pub fn step_minute(
        &mut self,
        now: u32,
        road: &RoadGraph,
        vdf: &VdfTable,
        cat: &VehicleCatalog,
        pending: Vec<PendingTrip>,
        out: &mut Vec<TrafficEvent>,
    ) {
        self.mezo.freeze_minute(road, vdf);

        self.unjam(now);

        for p in pending {
            self.dispatch(now, road, cat, p, out);
        }

        // Zdarzenia tej minuty w porządku chronologicznym. Prędkości są zamrożone
        // (`freeze_minute`), więc kolejność nie wpływa na czas przejazdu nikogo —
        // wpływa wyłącznie na to, kto pierwszy wjedzie na krawędź bliską zapełnienia,
        // a ten porządek jest totalny i wynika z klucza kopca.
        let limit = (u64::from(now) + 1) * CS_PER_MINUTE;
        let mut kroki = 0u64;
        while let Some(Reverse((cs, _, slot))) = self.heap.peek().copied() {
            if cs >= limit {
                break;
            }
            self.heap.pop();
            self.advance(slot, now, road, cat, out);
            kroki += 1;
            if kroki >= MAX_EDGE_STEPS_PER_MINUTE {
                break;
            }
        }
    }

    fn dispatch(
        &mut self,
        now: u32,
        road: &RoadGraph,
        cat: &VehicleCatalog,
        p: PendingTrip,
        out: &mut Vec<TrafficEvent>,
    ) {
        let mut edges: Vec<EdgeId> = Vec::new();
        let mut refuel_after = u32::MAX;
        for (i, leg) in p.legs.iter().enumerate() {
            for l in &leg.legs {
                edges.extend_from_slice(&l.edges);
            }
            if i == 0 && p.station.is_some() {
                refuel_after = edges.len().saturating_sub(1) as u32;
            }
        }
        if edges.is_empty() {
            // Trasa pusta znaczy „origin == cel": mieszkaniec jest już na miejscu.
            out.push(TrafficEvent::Arrived {
                trip: p.trip,
                traveller: p.traveller,
                vehicle: p.vehicle,
                slot: p.slot,
                dest: p.dest,
                at: SimMinute(u64::from(now) + 1),
                planned_minutes: p.planned_minutes,
                tank_level_ul: p.tank_level_ul,
                ledger: TripLedger {
                    trip: p.trip,
                    depart: p.depart,
                    arrive: SimMinute(u64::from(now) + 1),
                    mode: TransportMode::Car as u8,
                    reason: p.reason,
                    ..TripLedger::default()
                },
            });
            self.stats.dispatched += 1;
            self.stats.arrived += 1;
            return;
        }

        let first = edges[0];
        let mut t = ActiveTrip {
            trip: p.trip,
            traveller: p.traveller,
            vehicle: p.vehicle,
            household: p.household,
            class: p.class,
            slot: p.slot,
            dest: p.dest,
            station: p.station,
            edges,
            refuel_after,
            pos: 0,
            entry_cs: u64::from(now) * CS_PER_MINUTE,
            exit_cs: 0,
            depart: p.depart,
            planned_minutes: p.planned_minutes,
            load: p.load,
            fuel_ul: 0,
            distance_cm: 0,
            tank_level_ul: p.tank_level_ul,
            tank_capacity_ul: p.tank_capacity_ul,
            bought_ul: 0,
            money: Money::ZERO,
            stops: 0,
            blocked_since: u32::MAX,
            reason: p.reason,
            station_reason: None,
            wait_next: crate::mezo::NO_WAIT,
            pending_node_cs: 0,
            pending_blocked_cs: 0,
            entries: Vec::new(),
            traced: p.traced,
        };

        self.mezo.enter(first);
        self.stats.entries += 1;
        self.stats.dispatched += 1;
        // Zimny start liczy się raz na podróż, na pierwszej krawędzi (§5.4).
        self.settle_current(&mut t, road, cat, 0, true);

        let slot = match self.free.pop() {
            Some(i) => {
                self.trips[i as usize] = Some(t);
                i
            }
            None => {
                self.trips.push(Some(t));
                (self.trips.len() - 1) as u32
            }
        };
        let (cs, veh) = {
            let t = self.trips[slot as usize].as_ref().expect("świeża podróż");
            (t.exit_cs, t.vehicle)
        };
        self.heap.push(Reverse((cs, veh, slot)));
    }

    /// Rozlicza przejazd bieżącej krawędzi i ustawia minutę wyjazdu.
    fn settle_current(
        &mut self,
        t: &mut ActiveTrip,
        road: &RoadGraph,
        cat: &VehicleCatalog,
        stops: u8,
        cold_start: bool,
    ) {
        let edge = t.edges[t.pos as usize];
        let e = &road.edges[edge.0 as usize];
        let link = &self.mezo.links[edge.0 as usize];
        let veh = VehicleSpecRef {
            cat,
            class: t.class,
        };
        let mut entry = settle_edge(e, link, &veh, t.load, t.entry_cs, stops, cold_start);
        entry.edge = edge;
        entry.node_delay_cs = std::mem::take(&mut t.pending_node_cs);
        entry.blocked_cs = std::mem::take(&mut t.pending_blocked_cs);
        t.exit_cs = t.entry_cs
            + u64::from(entry.travel_cs)
            + u64::from(stops) * u64::from(crate::mezo::HEADWAY_CS);
        t.fuel_ul += entry.fuel_ul;
        t.distance_cm += u64::from(e.length_cm);
        t.money = Money(t.money.0 + entry.money.0);
        t.stops += u32::from(stops);
        if t.traced {
            t.entries.push(entry);
        }
        self.stats.edge_settles += 1;
        self.stats.travel_cs_total += u64::from(entry.travel_cs);
        self.stats.fuel_burned_ul += entry.fuel_ul;
    }

    /// Jedno przejście przez krawędź. **Dokładnie jedno** — po nim podróż wraca
    /// do kopca, żeby przeplot z innymi był chronologiczny, a nie „każdy swoją trasę
    /// do końca". Od tego zależy, czy na krawędzi da się spotkać dwa pojazdy naraz,
    /// a więc czy w ogóle istnieje korek.
    fn advance(
        &mut self,
        slot: u32,
        now: u32,
        road: &RoadGraph,
        cat: &VehicleCatalog,
        out: &mut Vec<TrafficEvent>,
    ) {
        let Some(t) = self.trips[slot as usize].as_ref() else {
            return;
        };

        // Zawór bezpieczeństwa: podróż, która przekroczyła sufit czasu, kończy się
        // porażką zamiast wisieć na sieci w nieskończoność (ryzyko R4).
        let sufit = u32::from(t.planned_minutes)
            .saturating_mul(TRIP_TIMEOUT_FACTOR)
            .saturating_add(TRIP_TIMEOUT_EXTRA_MIN);
        if now.saturating_sub(t.depart.0 as u32) > sufit {
            let edge = t.edges[t.pos as usize];
            let ev = TrafficEvent::Failed {
                trip: t.trip,
                traveller: t.traveller,
                vehicle: t.vehicle,
                slot: t.slot,
                dest: t.dest,
                at: SimMinute(u64::from(now)),
                reason: TripFailure::Gridlock,
                tank_level_ul: t.tank_level_ul - t.fuel_ul + t.bought_ul,
                distance_cm: t.distance_cm,
            };
            self.stats.max_edges_per_trip = self.stats.max_edges_per_trip.max(u64::from(t.pos));
            self.mezo.leave(edge);
            self.stats.exits += 1;
            self.stats.failed += 1;
            self.stats.failed_gridlock += 1;
            self.release(slot);
            out.push(ev);
            return;
        }

        if t.pos as usize + 1 == t.edges.len() {
            self.finish(slot, now, out);
            return;
        }

        let cur = t.edges[t.pos as usize];
        let next = t.edges[t.pos as usize + 1];

        // Spillback: krawędź docelowa jest pełna, więc pojazd **zostaje** tam, gdzie
        // jest. Nie znika i nie dubluje się — stąd bierze się zachowanie liczby
        // pojazdów w systemie, sprawdzane co minutę w teście korka.
        let zapchana = self.mezo.queues[next.0 as usize].is_full();
        let wymuszone =
            self.mezo.queues[next.0 as usize].stuck_minutes >= GRIDLOCK_RELEASE_MIN as u16;
        if zapchana && !wymuszone {
            self.mezo.reject(next);
            if let Some(r) = self.rejects_total.get_mut(next.0 as usize) {
                *r += 1;
            }
            {
                let t = self.trips[slot as usize].as_mut().expect("podróż w toku");
                if t.blocked_since == u32::MAX {
                    t.blocked_since = now;
                    self.stats.spillbacks += 1;
                }
                let czekal = u64::from(now.saturating_sub(t.blocked_since));
                if czekal > self.stats.max_blocked_minutes {
                    self.stats.max_blocked_minutes = czekal;
                }
            }
            // Pojazd **czeka na zdarzenie**, a nie ponawia próby co jakiś czas: budzi
            // go zwolnienie miejsca na krawędzi, na którą czeka. Ponawianie co minutę
            // obcinałoby przepustowość każdego przewężenia do `storage_capacity`
            // pojazdów na minutę, a ponawianie co odstęp zamieniałoby korek
            // w miliony operacji na kopcu.
            self.enqueue_wait(next, slot);
            return;
        }
        if zapchana && wymuszone {
            self.stats.gridlock_releases += 1;
        }

        // Węzeł: opóźnienie i zatrzymania na wjeździe w następną krawędź.
        let node = road.edges[cur.0 as usize].to;
        let control = road.controls[node.0 as usize];
        let (priority, zawracanie) = turn_priority(road, cur, next);
        if zawracanie {
            self.stats.u_turns += 1;
        }
        let node_state = self.mezo.nodes[node.0 as usize];
        let (entry_cs, stops) = settle_node(control, priority, &node_state, t.exit_cs);
        let opoznienie_wezla = entry_cs.saturating_sub(t.exit_cs);
        self.stats.node_delay_cs_total += opoznienie_wezla;
        // Zawracanie to pełne zatrzymanie plus manewr — jedno dodatkowe zatrzymanie
        // ponad to, co policzył model węzła.
        let stops = stops.saturating_add(u8::from(zawracanie));
        self.mezo.nodes[node.0 as usize].arrivals_this_min = self.mezo.nodes[node.0 as usize]
            .arrivals_this_min
            .saturating_add(1);

        self.mezo.leave(cur);
        self.mezo.enter(next);
        self.wake_one(cur, entry_cs);

        let refuel = {
            let t = self.trips[slot as usize].as_mut().expect("podróż");
            t.pos += 1;
            t.entry_cs = entry_cs;
            t.blocked_since = u32::MAX;
            // Opóźnienie węzła należy do wiersza krawędzi, na którą pojazd właśnie
            // wjeżdża — powstanie ono dopiero w `settle_current` niżej (`N-6`).
            t.pending_node_cs = t
                .pending_node_cs
                .saturating_add(opoznienie_wezla.min(u64::from(u32::MAX)) as u32);
            t.refuel_after != u32::MAX && t.pos == t.refuel_after + 1
        };
        if refuel {
            self.refuel(slot, cat, out);
        }
        let mut t = self.trips[slot as usize].take().expect("podróż");
        self.settle_current(&mut t, road, cat, stops, false);
        let (cs, veh) = (t.exit_cs, t.vehicle);
        self.trips[slot as usize] = Some(t);
        self.heap.push(Reverse((cs, veh, slot)));
    }

    fn finish(&mut self, slot: u32, now: u32, out: &mut Vec<TrafficEvent>) {
        let t = self.trips[slot as usize].take().expect("podróż w toku");
        let edge = t.edges[t.pos as usize];
        self.mezo.leave(edge);
        self.wake_one(edge, t.exit_cs);
        self.stats.exits += 1;
        self.stats.arrived += 1;

        // Przybycie nie może wypaść w minucie już rozdanej przez pętlę doby —
        // zdarzenie na przeszłość nigdy nie zostałoby doręczone.
        let at = SimMinute(u64::from(now) + 1);
        let planned_arrive = t.depart.0 + u64::from(t.planned_minutes);
        self.stats.planned_minutes_total += u64::from(t.planned_minutes);
        self.stats.actual_minutes_total += at.0.saturating_sub(t.depart.0);
        if at.0 > planned_arrive {
            self.stats.late_arrivals += 1;
            self.stats.late_minutes += at.0 - planned_arrive;
        }

        self.stats.max_edges_per_trip = self.stats.max_edges_per_trip.max(t.edges.len() as u64);
        let ledger = TripLedger {
            trip: t.trip,
            entries: t.entries,
            total_fuel_ul: t.fuel_ul,
            total_money: t.money,
            edges: t.edges.len() as u32,
            stops: t.stops,
            distance_cm: t.distance_cm,
            depart: t.depart,
            arrive: at,
            mode: TransportMode::Car as u8,
            reason: t.station_reason.unwrap_or(if at.0 > planned_arrive {
                DecisionReason::TripDelayed {
                    planned_min: t.planned_minutes,
                    actual_min: at.0.saturating_sub(t.depart.0).min(u64::from(u16::MAX)) as u16,
                }
            } else {
                t.reason
            }),
        };
        debug_assert!(
            t.tank_level_ul - t.fuel_ul + t.bought_ul >= 0,
            "pojazd {} dojechał z ujemnym bakiem — wybór środka wpuścił podróż poza zasięg",
            t.vehicle
        );
        self.stats.reasons[match ledger.reason {
            DecisionReason::ModeChosen { .. } => 0,
            DecisionReason::ModeCompared { .. } => 1,
            DecisionReason::NoRouteForMode { .. } => 2,
            DecisionReason::NoParkingAtDestination { .. } => 3,
            DecisionReason::RefuelNeeded { .. } => 4,
            DecisionReason::StationChosen { .. } => 5,
            DecisionReason::TripDelayed { .. } => 6,
            _ => 7,
        }] += 1;
        out.push(TrafficEvent::Arrived {
            trip: t.trip,
            traveller: t.traveller,
            vehicle: t.vehicle,
            slot: t.slot,
            dest: t.dest,
            at,
            planned_minutes: t.planned_minutes,
            tank_level_ul: t.tank_level_ul - t.fuel_ul + t.bought_ul,
            ledger,
        });
        self.free.push(slot);
    }

    /// Wstawia podroz na koniec kolejki oczekujacych na wjazd na krawedz.
    fn enqueue_wait(&mut self, edge: EdgeId, slot: u32) {
        let i = edge.0 as usize;
        self.trips[slot as usize]
            .as_mut()
            .expect("podroz w toku")
            .wait_next = crate::mezo::NO_WAIT;
        let ogon = self.mezo.wait_tail[i];
        if ogon == crate::mezo::NO_WAIT {
            self.mezo.wait_head[i] = slot;
            self.waiting_edges.push(edge.0);
        } else {
            self.trips[ogon as usize]
                .as_mut()
                .expect("oczekujacy")
                .wait_next = slot;
        }
        self.mezo.wait_tail[i] = slot;
    }

    /// Budzi pierwszego oczekujacego na krawedz, ktora wlasnie sie zwolnila.
    /// Godzina przebudzenia to chwila zwolnienia miejsca plus jeden odstep --
    /// tyle, ile trwa ruszenie z kolejki.
    fn wake_one(&mut self, edge: EdgeId, at_cs: u64) {
        let i = edge.0 as usize;
        let slot = self.mezo.wait_head[i];
        if slot == crate::mezo::NO_WAIT {
            return;
        }
        let czekal;
        let (nastepny, veh, cs) = {
            let t = self.trips[slot as usize].as_mut().expect("oczekujacy");
            let przed = t.exit_cs;
            t.exit_cs = at_cs.max(t.exit_cs) + u64::from(crate::mezo::HEADWAY_CS);
            czekal = t.exit_cs - przed;
            t.pending_blocked_cs = t
                .pending_blocked_cs
                .saturating_add(czekal.min(u64::from(u32::MAX)) as u32);
            let n = t.wait_next;
            t.wait_next = crate::mezo::NO_WAIT;
            (n, t.vehicle, t.exit_cs)
        };
        self.mezo.wait_head[i] = nastepny;
        if nastepny == crate::mezo::NO_WAIT {
            self.mezo.wait_tail[i] = crate::mezo::NO_WAIT;
        }
        self.stats.blocked_cs_total += czekal;
        self.heap.push(Reverse((cs, veh, slot)));
    }

    /// Rozplatanie zakleszczenia (ryzyko R4): krawedz pelna i nieruchoma od
    /// [`GRIDLOCK_RELEASE_MIN`] minut wypuszcza pierwszego oczekujacego mimo braku
    /// miejsca. To jedyne miejsce, w ktorym oblozenie krawedzi przekracza pojemnosc.
    fn unjam(&mut self, now: u32) {
        let at_cs = u64::from(now) * CS_PER_MINUTE;
        let mut edges = std::mem::take(&mut self.waiting_edges);
        edges.sort_unstable();
        edges.dedup();
        edges.retain(|e| {
            let i = *e as usize;
            if self.mezo.wait_head[i] == crate::mezo::NO_WAIT {
                return false;
            }
            if self.mezo.queues[i].stuck_minutes >= GRIDLOCK_RELEASE_MIN as u16 {
                // Krawędź pełna i nieruchoma od kilku minut to **zakleszczenie**,
                // a nie kolejka: jej pojazdy czekają na coś, co czeka na nią.
                // Budzenie jednego oczekującego rozwiązywałoby taki cykl w tempie
                // jednego pojazdu na kilka minut, czyli wcale. Wypuszczamy cały
                // ogonek naraz — nadmiar rozejdzie się w następnych minutach.
                while self.mezo.wait_head[i] != crate::mezo::NO_WAIT {
                    self.stats.gridlock_releases += 1;
                    self.wake_one(EdgeId(*e), at_cs);
                }
                self.mezo.queues[i].stuck_minutes = 0;
            }
            self.mezo.wait_head[i] != crate::mezo::NO_WAIT
        });
        self.waiting_edges = edges;
    }

    fn release(&mut self, slot: u32) {
        self.trips[slot as usize] = None;
        self.free.push(slot);
    }

    /// Tankowanie do pełna w chwili dojazdu na stację.
    ///
    /// Bilans jest domknięty w jednym miejscu: `bought = pojemność − (stan − spalone)`,
    /// więc suma zatankowanego minus suma spalonego równa się zmianie poziomów baków
    /// co do mikrolitra — i to jest cały dowód testu własnościowego paliwa.
    fn refuel(&mut self, slot: u32, cat: &VehicleCatalog, out: &mut Vec<TrafficEvent>) {
        let (trip, traveller, vehicle, household, station, units, cost, minute) = {
            let t = self.trips[slot as usize].as_mut().expect("podróż");
            let Some(station) = t.station else {
                return;
            };
            let w_baku = t.tank_level_ul - t.fuel_ul + t.bought_ul;
            let units = (t.tank_capacity_ul - w_baku).max(0);
            let kind = cat.spec(t.class).fuel;
            let cost = cat.fuel_cost(kind, units);
            t.bought_ul += units;
            t.money = Money(t.money.0 + cost.0);
            // Objazd liczony z tego, co już wiadomo: różnica między planem a chwilą
            // dojazdu na stację. Cena jest za litr, więc z mikrolitrów na litry.
            t.station_reason = Some(DecisionReason::StationChosen {
                detour_min: REFUEL_DWELL_MIN as u16,
                price_gr_per_l: cat.price_gr(kind).clamp(0, i64::from(u16::MAX)) as u16,
            });
            // Postój przy dystrybutorze przesuwa cały dalszy ciąg podróży.
            t.entry_cs += u64::from(REFUEL_DWELL_MIN) * CS_PER_MINUTE;
            (
                t.trip,
                t.traveller,
                t.vehicle,
                t.household,
                station,
                units,
                cost,
                SimMinute(t.entry_cs / CS_PER_MINUTE),
            )
        };
        self.stats.refuels += 1;
        self.stats.fuel_bought_ul += units;
        self.stats.fuel_spent = Money(self.stats.fuel_spent.0 + cost.0);
        out.push(TrafficEvent::Refuelled {
            trip,
            traveller,
            vehicle,
            household,
            station,
            units,
            cost,
            at: minute,
        });
    }
}

impl HashState for TrafficNetwork {
    /// Podróże w kolejności indeksu encji pojazdu, nie w kolejności slotów: slot
    /// zależy od historii zwolnień, a ta sama sytuacja logiczna ma dać ten sam hash
    /// niezależnie od tego, jak się do niej doszło (M0, test T-D5).
    fn hash_state(&self, h: &mut StateHasher) {
        self.mezo.hash_state(h);
        let mut order: Vec<(u32, usize)> = self
            .trips
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.as_ref().map(|t| (t.vehicle, i)))
            .collect();
        order.sort_unstable();
        for (_, i) in order {
            let t = self.trips[i].as_ref().expect("podróż");
            h.write_u32(t.trip.0);
            h.write_u32(t.traveller);
            h.write_u32(t.vehicle);
            h.write_u32(t.pos);
            h.write_u64(t.exit_cs);
            h.write_u64(t.entry_cs);
            h.write_u64(t.fuel_ul as u64);
            h.write_u64(t.bought_ul as u64);
            h.write_u64(t.money.0 as u64);
            h.write_u32(t.stops);
            h.write_u32(t.wait_next);
            // Obie zaległości przeżywają granicę ticku (kolejka oczekujących budzi się
            // w minucie następnej), więc są stanem, a nie scratchem jednego kroku.
            h.write_u32(t.pending_node_cs);
            h.write_u32(t.pending_blocked_cs);
        }
    }
}
