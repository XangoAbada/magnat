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

mod minute;
mod queue;

use crate::mezo::{
    settle_edge, settle_node, turn_priority, LedgerEntry, MezoState, VehicleSpecRef, CS_PER_MINUTE,
};
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
        self.arrive
            .0
            .saturating_sub(self.depart.0)
            .min(u64::from(u16::MAX)) as u16
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
    /// Podsłuch krawędzi: kto przejechał tamtędy w tej minucie (M10b WP10.6).
    watch: EdgeWatch,
}

/// Podsłuch wybranych krawędzi — **kto** nimi przejechał, nie ilu ich było.
///
/// Powstał dla billboardów, ale nie wie o nich nic i wiedzieć nie ma: ruch jest
/// własnością M4, a to, co znaczy przejazd obok tablicy, jest własnością M10.
/// To ta sama granica, którą `K-14` postawił między urbanistyką a geometrią pasów.
///
/// **Pusty zbiór krawędzi kosztuje zero** — jedno porównanie z pustym wektorem na
/// wpuszczaną podróż. Świat bez reklamy nie płaci za ten mechanizm ani cyklu, a hash
/// stanu ma w nim wtedy dwie zerowe długości.
///
/// `ponytail:` sufit nazwany — przy ponad `WATCH_EDGE_CAP` obserwowanych krawędziach
/// (~2000 billboardów w mieście, M10b §5.2) koszt rośnie liniowo z długością tras
/// wpuszczanych podróży. Droga wyjścia to agregat per krawędź zamiast listy
/// tożsamości; nie wcześniej, bo dziś to jest `binary_search` po kilkuset wpisach.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgeWatch {
    /// Posortowane, bez powtórzeń — `binary_search` na gorącej ścieżce `dispatch`.
    edges: Vec<EdgeId>,
    /// `(indeks encji podróżnego, krawędź)` w kolejności wpuszczania podróży.
    passes: Vec<(u32, EdgeId)>,
}

/// Ile przejazdów po obserwowanych krawędziach sieć zapamięta, zanim zacznie je gubić.
///
/// Sufit istnieje dla świata, w którym krawędzie ktoś ustawił, a przejazdów nie
/// odbiera nikt: bez niego wektor rósłby przez całą sesję. Przejazdy ponad sufit są
/// **gubione po cichu** i tak ma być: licznik zgubionych byłby albo stanem w hashu
/// (czyli kolejnym polem, które musi przeżyć zapis), albo pomiarem poza hashem —
/// a odbiorca opróżnia bufor co godzinę, więc sufit sięgnąłby wyłącznie w świecie,
/// w którym system mediów w ogóle nie stoi.
pub const WATCH_PASS_CAP: usize = 65_536;

impl EdgeWatch {
    /// Ustawia zbiór obserwowanych krawędzi. Pusty zbiór wyłącza mechanizm.
    pub fn set_edges(&mut self, mut edges: Vec<EdgeId>) {
        edges.sort_unstable();
        edges.dedup();
        self.edges = edges;
        if self.edges.is_empty() {
            self.passes.clear();
        }
    }

    #[must_use]
    pub fn edges(&self) -> &[EdgeId] {
        &self.edges
    }

    /// Odbiera przejazdy z tej minuty i opróżnia bufor.
    pub fn take_passes(&mut self) -> Vec<(u32, EdgeId)> {
        std::mem::take(&mut self.passes)
    }

    /// Ile przejazdów czeka na odbiór.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.passes.len()
    }

    /// Notuje przejazd po trasie. Woła to `dispatch` przy wpuszczaniu podróży —
    /// publiczne, bo to jest **jedyne wejście** do bufora i test zasięgu billboardu
    /// musi mieć czym podać przejazdy bez stawiania całej sieci drogowej.
    pub fn note(&mut self, traveller: u32, edges: &[EdgeId]) {
        if self.edges.is_empty() {
            return;
        }
        for e in edges {
            if self.edges.binary_search(e).is_ok() {
                if self.passes.len() >= WATCH_PASS_CAP {
                    return;
                }
                self.passes.push((traveller, *e));
            }
        }
    }
}

impl HashState for EdgeWatch {
    /// Do hasha wchodzi **jedno i drugie**: zbiór krawędzi jest wyprowadzony z kampanii,
    /// a nieodebrane przejazdy są tym, co świat ma do przekazania w następnej minucie.
    /// Licznik zgubionych **nie** wchodzi — jest pomiarem, nie stanem (jak `rejects_total`).
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.edges.len() as u64);
        for e in &self.edges {
            h.write_u32(e.0);
        }
        h.write_u64(self.passes.len() as u64);
        for (t, e) in &self.passes {
            h.write_u32(*t);
            h.write_u32(e.0);
        }
    }
}

impl PartialEq for TrafficNetwork {
    fn eq(&self, other: &TrafficNetwork) -> bool {
        self.mezo == other.mezo
            && self.trips == other.trips
            && self.stats == other.stats
            && self.watch == other.watch
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

    /// Podsłuch krawędzi do odczytu — dla systemu, który odbiera przejazdy.
    #[must_use]
    pub fn watch(&self) -> &EdgeWatch {
        &self.watch
    }

    /// Podsłuch krawędzi do zapisu — dla systemu, który ustawia obserwowane krawędzie.
    pub fn watch_mut(&mut self) -> &mut EdgeWatch {
        &mut self.watch
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
    pub fn feed_micro(
        &self,
        micro: &crate::micro::MicroLayer,
        road: &RoadGraph,
        cat: &VehicleCatalog,
    ) {
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
}

impl HashState for TrafficNetwork {
    /// Podróże w kolejności indeksu encji pojazdu, nie w kolejności slotów: slot
    /// zależy od historii zwolnień, a ta sama sytuacja logiczna ma dać ten sam hash
    /// niezależnie od tego, jak się do niej doszło (M0, test T-D5).
    fn hash_state(&self, h: &mut StateHasher) {
        self.mezo.hash_state(h);
        self.watch.hash_state(h);
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
