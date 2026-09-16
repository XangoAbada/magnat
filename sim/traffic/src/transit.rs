//! Komunikacja miejska (M4c/WP10, §5.6, PRD §9.3).
//!
//! Cztery rzeczy, które ta warstwa ma robić naprawdę, a nie pozornie:
//!
//! 1. **Autobus jedzie tą samą siecią i podlega temu samemu [`settle_edge`].** Czas
//!    przejazdu między przystankami liczy się z `LinkState` krawędzi trasy, czyli
//!    z tego samego obłożenia, które opóźnia samochody. Korek opóźnia autobus —
//!    to nie jest mnożnik dopisany do rozkładu, tylko ta sama funkcja kosztu.
//! 2. **Pojemność jest twarda.** Gdy `occupancy == capacity`, wsiadanie jest odrzucane;
//!    pasażer zostaje na przystanku i czeka na następny kurs.
//! 3. **Kierowca jest mieszkańcem.** Kurs bez obecnego kierowcy jest odwołany, a nie
//!    „jedzie sam" — obecność sprawdza [`TransitNetwork::step_minute`] przez predykat,
//!    który podaje mu system ruchu, bo to on ma dostęp do świata.
//! 4. **Pasażer, który nie dojedzie, i tak dostaje `Arrive`** (`M-4`). Po
//!    [`MAX_WAIT_MIN`] minutach oczekiwania rezygnuje i idzie pieszo. Podróż, która
//!    nie ma jak się skończyć, zawiesza mieszkańca do końca gry — najgorszy rodzaj
//!    błędu, bo nie wywala się, tylko zatrzymuje miasto.
//!
//! ## Czego tu nie ma
//!
//! - **Autobus nie zajmuje miejsca w `EdgeQueue`.** `ponytail:` sufit nazwany: kurs
//!   cierpi od korka, ale go nie tworzy. Przy ~200 kursach wobec 13 500 pojazdów
//!   w szczycie jest to 1,5 % floty na sieci, czyli mniej niż szerokość widełek
//!   kalibracji VDF. Ścieżka wyjścia: M4d/WP8 stawia pojazd komunikacji w warstwie
//!   mikro razem z samochodami i wtedy wchodzi też do kolejki krawędzi.
//! - **Więcej niż jedna przesiadka.** Routing zna trasę bezpośrednią i jedną
//!   przesiadkę; `ponytail:` sufit nazwany, ścieżka wyjścia to RAPTOR na tabeli
//!   połączeń, gdy linii będzie więcej niż kilkanaście na dzielnicę.
//! - **Tramwaju i metra jako osobnych warstw.** [`TransitMode`] je rozróżnia i wydzielone
//!   torowisko już działa (kurs po `Modality::Rail` nie czyta `LinkState` drogi), ale
//!   sieć szynowa metropolii ma dziś 20 węzłów w składowej (`M-7`), więc instancji nie ma.

mod plan;
mod sim;

use crate::mezo::{settle_edge, MezoState, VehicleSpecRef, CS_PER_MINUTE};
use crate::spec::{VehicleCatalog, VehicleClassId};
use magnat_core::{DayOfWeek, HashState, Mass, Money, SimMinute, StateHasher, WorldCoord};
use magnat_nav::{EdgeId, NodeId, RoadGraph};
use magnat_spatial::{Aabb2, CsrGrid, GridSpec, Vec2};

/// Identyfikator linii.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct LineId(pub u16);

/// Rodzaj komunikacji. Dyskryminanty wchodzą do zapisu gry.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum TransitMode {
    Bus = 0,
    Tram = 1,
    SuburbanRail = 2,
    Metro = 3,
}

impl TransitMode {
    /// Czy pojazd dzieli jezdnię z ruchem samochodowym. Tramwaj i metro mają
    /// wydzielone torowisko, więc korek ich nie dotyczy (§5.6).
    #[must_use]
    pub const fn shares_road(self) -> bool {
        matches!(self, TransitMode::Bus)
    }
}

/// Kto obsługuje linię. Przetargi i dotacje dokłada M8 — tu jest sam adres.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OperatorRef {
    City,
    Firm(u32),
}

/// Przystanek: węzeł sieci plus kolejka oczekujących.
#[derive(Clone, Debug)]
pub struct TransitStop {
    pub node: NodeId,
    pub at: WorldCoord,
    pub dwell_base_s: u16,
    /// Kolejka oczekujących — **FIFO po minucie przybycia**, remisy po indeksie encji
    /// (§5.6). Kolejność jest kluczem totalnym, więc to, kto wsiądzie do pełnego
    /// autobusu, nie zależy od kolejności iteracji nigdzie w kodzie.
    pub waiting: Vec<Waiting>,
}

/// Pasażer czekający na przystanku.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Waiting {
    /// Minuta wejścia do kolejki — pierwszy człon klucza porządkującego.
    pub since_min: u32,
    /// Indeks encji mieszkańca — drugi człon, rozstrzyga remisy.
    pub citizen: u32,
    /// Do którego przystanku tej linii jedzie.
    pub alight_stop: u16,
    /// Dokąd ma iść po wysiadce — minuty dojścia od przystanku docelowego.
    pub egress_min: u16,
    /// Slot planu dnia, do którego wróci zdarzenie `Arrive`.
    pub slot: u8,
    /// Druga noga podróży, gdy jest przesiadka: `(linia, przystanek docelowy)`.
    /// `LineId(0)` = brak — numeracja linii zaczyna się od 1.
    pub next: (LineId, u16),
    /// Ile minut zajmie dojście pieszo, gdy pasażer się podda. **Bez tego pola
    /// rezygnacja byłaby teleportacją**: mieszkaniec stałby 45 minut na przystanku
    /// i docierał do celu w następnej minucie. Liczy je wołający, bo to on ma sieć
    /// pieszą; warstwa komunikacji zna tylko rozkład.
    pub walk_fallback_min: u16,
    /// Ile kursów przejechało obok bez miejsca. Licznik jest **per pasażer**, bo
    /// inaczej `left_behind` liczyłby zdarzenia, a nie ludzi: kto czeka pół godziny
    /// na zapchanej linii, zostaje pominięty przez pięć kursów i piąciokrotnie
    /// zawyżał statystykę przepełnienia.
    pub passed: u8,
}

/// Rozkład: pierwszy i ostatni odjazd plus odstęp, osobny dla szczytu i reszty doby.
///
/// `ponytail:` dwa odstępy zamiast tabeli odjazdów. Sufit nazwany: rozkład jest ten sam
/// w każdy dzień roboczy i nie da się zaplanować kursu na konkretną minutę. Ścieżka
/// wyjścia: `Vec<u16>` odjazdów wczytywany z `data/`, gdy M8 wprowadzi przetargi
/// i gracz zacznie rozkład układać.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Timetable {
    pub first_min: u16,
    pub last_min: u16,
    pub headway_peak_min: u16,
    pub headway_base_min: u16,
    /// Maska dni tygodnia, w które linia kursuje (bit N = dzień N, jak `Employment`).
    pub days: u8,
}

/// Szczyt poranny i popołudniowy — te same okna, z których korzysta kalibracja ruchu.
pub const PEAK_AM: (u16, u16) = (6 * 60, 9 * 60);
pub const PEAK_PM: (u16, u16) = (15 * 60, 18 * 60);

impl Timetable {
    #[must_use]
    pub fn runs_on(&self, dow: DayOfWeek) -> bool {
        self.days & (1 << dow as u8) != 0
    }

    /// Czy w tej minucie doby z pierwszego przystanku rusza kurs.
    ///
    /// Odstęp zmienia się skokowo na granicy szczytu, więc odjazdy liczy się od
    /// początku doby narastająco, a nie modulo — inaczej kurs wypadałby dwa razy
    /// albo znikał dokładnie w minucie zmiany odstępu.
    #[must_use]
    pub fn departs_at(&self, minute_of_day: u16) -> bool {
        if minute_of_day < self.first_min || minute_of_day > self.last_min {
            return false;
        }
        let mut t = self.first_min;
        while t <= minute_of_day {
            if t == minute_of_day {
                return true;
            }
            t = t.saturating_add(self.headway(t));
        }
        false
    }

    /// Najbliższy odjazd nie wcześniejszy niż `minute_of_day`; `None` = już po ostatnim.
    #[must_use]
    pub fn next_departure(&self, minute_of_day: u16) -> Option<u16> {
        let mut t = self.first_min;
        while t <= self.last_min {
            if t >= minute_of_day {
                return Some(t);
            }
            t = t.saturating_add(self.headway(t));
        }
        None
    }

    #[must_use]
    fn headway(&self, minute_of_day: u16) -> u16 {
        let szczyt = (minute_of_day >= PEAK_AM.0 && minute_of_day < PEAK_AM.1)
            || (minute_of_day >= PEAK_PM.0 && minute_of_day < PEAK_PM.1);
        let h = if szczyt {
            self.headway_peak_min
        } else {
            self.headway_base_min
        };
        h.max(1)
    }
}

/// Linia: trasa, przystanki, rozkład, tabor i taryfa.
#[derive(Clone, Debug)]
pub struct TransitLine {
    pub id: LineId,
    pub mode: TransitMode,
    pub stops: Vec<TransitStop>,
    /// Krawędzie między kolejnymi przystankami: `hops[i]` prowadzi ze `stops[i]`
    /// do `stops[i+1]`. Długość zawsze o jeden mniejsza niż `stops`.
    pub hops: Vec<Vec<EdgeId>>,
    pub timetable: Timetable,
    /// Sloty floty przypisane linii — kurs bierze pojazd po kolei.
    pub fleet: Vec<u32>,
    pub class: VehicleClassId,
    pub operator: OperatorRef,
    pub fare: Money,
    pub capacity: u16,
}

/// Docelowy odstęp między przystankami w metrach. 500 m to około sześciu minut
/// dojścia z najdalszego punktu obsługiwanego pasa — tyle, ile mieszkaniec akceptuje
/// bez rezygnacji z komunikacji.
pub const STOP_SPACING_M: u32 = 400;

impl TransitLine {
    /// Buduje linię z gotowej trasy drogowej: przystanki rozstawia co
    /// [`STOP_SPACING_M`], zawsze na węźle grafu, zawsze na początku i na końcu.
    ///
    /// Funkcja jest tutaj, a nie w `sim/world`, mimo że to tam powstaje miasto:
    /// rozstawienie przystanków wzdłuż trasy jest regułą **komunikacji**, a nie
    /// urbanistyki, i jej właścicielem jest ta sama faza, która potem po nich jeździ.
    /// `sim/world` podaje wyłącznie korytarz — parę węzłów do połączenia.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn from_route(
        id: LineId,
        mode: TransitMode,
        edges: &[EdgeId],
        road: &RoadGraph,
        timetable: Timetable,
        class: VehicleClassId,
        fare: Money,
        capacity: u16,
    ) -> Option<TransitLine> {
        if edges.is_empty() {
            return None;
        }
        let wezel = |n: NodeId| {
            let p = &road.nodes[n.0 as usize];
            WorldCoord::new(p.pos_cm.x, p.pos_cm.y, p.z_cm)
        };
        let przystanek = |n: NodeId| TransitStop {
            node: n,
            at: wezel(n),
            dwell_base_s: 20,
            waiting: Vec::new(),
        };
        let mut stops = vec![przystanek(road.edges[edges[0].0 as usize].from)];
        let mut hops: Vec<Vec<EdgeId>> = Vec::new();
        let mut biezacy: Vec<EdgeId> = Vec::new();
        let mut od_ostatniego_cm = 0u32;
        for (i, e) in edges.iter().enumerate() {
            let edge = &road.edges[e.0 as usize];
            biezacy.push(*e);
            od_ostatniego_cm = od_ostatniego_cm.saturating_add(edge.length_cm);
            let ostatnia = i + 1 == edges.len();
            if od_ostatniego_cm >= STOP_SPACING_M * 100 || ostatnia {
                stops.push(przystanek(edge.to));
                hops.push(std::mem::take(&mut biezacy));
                od_ostatniego_cm = 0;
            }
        }
        // Linia o jednym przystanku nie wozi nikogo — trasa krótsza niż odstęp między
        // przystankami znaczy, że korytarz był źle dobrany, a nie że linia jest mała.
        if stops.len() < 2 {
            return None;
        }
        Some(TransitLine {
            id,
            mode,
            stops,
            hops,
            timetable,
            fleet: Vec::new(),
            class,
            operator: OperatorRef::City,
            fare,
            capacity,
        })
    }

    #[must_use]
    pub fn stop_count(&self) -> usize {
        self.stops.len()
    }

    /// Indeks przystanku na linii dla podanego węzła; `None` = linia go nie obsługuje.
    #[must_use]
    pub fn stop_at(&self, node: NodeId) -> Option<u16> {
        self.stops
            .iter()
            .position(|s| s.node == node)
            .map(|i| i as u16)
    }
}

/// Kurs — instancja rozkładu.
#[derive(Clone, Debug)]
pub struct TransitRun {
    pub line: LineId,
    pub vehicle: u32,
    /// Indeks encji mieszkańca prowadzącego kurs.
    pub driver: u32,
    /// Przystanek, na którym kurs właśnie stoi albo do którego jedzie.
    pub stop_index: u16,
    /// Minuta, w której kurs dotrze do `stop_index`.
    pub arrive_min: u32,
    /// Minuta, w której kurs ruszył do `stop_index`. Warstwa Mikro interpoluje między
    /// tą parą i **niczego nie wyznacza** — bryła kursu odgrywa czas tak samo jak pieszy.
    pub depart_min: u32,
    pub occupancy: u16,
    pub capacity: u16,
    /// Narastające wobec rozkładu, dodatnie = spóźnienie.
    pub delay_minutes: i16,
    /// Pasażerowie na pokładzie: `(mieszkaniec, przystanek docelowy, dojście, slot)`.
    pub onboard: Vec<Waiting>,
    /// Paliwo spalone przez kurs w mikrolitrach — obciąża operatora, nie pasażera.
    pub fuel_ul: i64,
}

/// Co warstwa ma do przekazania światu. Tak samo jak w [`crate::trip`]: sieć zwraca
/// zdarzenia, a system je stosuje — dzięki temu krok jest testowalny bez świata.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransitEvent {
    /// Pasażer wsiadł i zapłacił. Grosze idą do operatora.
    Boarded {
        citizen: u32,
        line: LineId,
        fare: Money,
        wait_min: u16,
        at: SimMinute,
    },
    /// Nie zmieścił się — zostaje na przystanku (`TripDecisionReason::LeftBehind`).
    LeftBehind {
        citizen: u32,
        line: LineId,
        at: SimMinute,
    },
    /// Wysiadł na przystanku docelowym; `egress_min` to dojście do celu.
    Alighted {
        citizen: u32,
        slot: u8,
        egress_min: u16,
        at: SimMinute,
    },
    /// Czekał za długo i idzie pieszo. Bez tego zdarzenia mieszkaniec stałby na
    /// przystanku do końca gry (`M-4`) — a bez `walk_min` docierałby do celu
    /// w następnej minucie, czyli teleportował się za karę za cierpliwość.
    GaveUp {
        citizen: u32,
        slot: u8,
        walk_min: u16,
        at: SimMinute,
    },
    /// Kurs spalił paliwo — obciążenie operatora, druga strona obrotu stacji.
    RunFuelled {
        line: LineId,
        vehicle: u32,
        units_ul: i64,
        cost: Money,
        at: SimMinute,
    },
    /// Kurs odwołany, bo nie było kierowcy.
    RunCancelled { line: LineId, at: SimMinute },
}

/// Liczniki zbiorcze — wejście raportu i testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TransitStats {
    pub runs_started: u64,
    pub runs_finished: u64,
    pub runs_cancelled: u64,
    pub boardings: u64,
    /// Ilu **różnych** pasażerów nie zmieściło się choć raz.
    pub left_behind: u64,
    /// Ile razy odmówiono wsiadania — ten sam pasażer może tu wejść wiele razy.
    pub boarding_refusals: u64,
    pub gave_up: u64,
    pub alightings: u64,
    pub wait_minutes: u64,
    pub fare_revenue: Money,
    pub fuel_ul: i64,
    pub fuel_cost: Money,
    /// Suma spóźnień kursów wobec rozkładu i największe z nich.
    pub delay_minutes: i64,
    pub max_delay_minutes: i16,
    pub max_occupancy: u16,
}

/// Ile sekund zajmuje jedno wejście i jedno wyjście z pojazdu.
const BOARD_S: u32 = 3;
const ALIGHT_S: u32 = 2;

/// Po ilu minutach oczekiwania pasażer rezygnuje i idzie pieszo.
///
/// Nie jest to parametr komfortu, tylko **zawór bezpieczeństwa** (`M-4`): ostatni kurs
/// odjeżdża przed północą, a kto przyszedł po nim, nie doczeka się nigdy.
pub const MAX_WAIT_MIN: u32 = 45;

/// Maksymalne dojście do przystanku, żeby linia była w ogóle rozważana.
pub const MAX_ACCESS_M: u16 = 800;

/// Podróż komunikacją tak, jak widzi ją wybór środka (§5.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TransitJourney {
    pub line: LineId,
    pub board_stop: u16,
    pub alight_stop: u16,
    /// Dojście do przystanku początkowego.
    pub access_min: u16,
    /// Oczekiwanie z rozkładu.
    pub wait_min: u16,
    /// Przejazd z rozkładu plus bieżące spóźnienie linii.
    pub ride_min: u16,
    /// Dojście z przystanku docelowego do celu.
    pub egress_min: u16,
    pub transfers: u8,
    pub fare: Money,
    /// Druga noga: `(linia, przystanek docelowy)`. `LineId(0)` = bez przesiadki.
    pub transfer_to: (LineId, u16),
}

impl TransitJourney {
    #[must_use]
    pub fn total_minutes(&self) -> u16 {
        self.access_min
            .saturating_add(self.wait_min)
            .saturating_add(self.ride_min)
            .saturating_add(self.egress_min)
    }
}

/// Sieć komunikacji: linie, kursy w toku i indeks przystanków.
#[derive(Clone, Debug, Default)]
pub struct TransitNetwork {
    lines: Vec<TransitLine>,
    runs: Vec<TransitRun>,
    /// `(indeks linii, indeks przystanku)` spłaszczone do indeksu przestrzennego.
    stops_flat: Vec<(u16, u16)>,
    index: Option<CsrGrid<u32>>,
    /// Czas przejazdu międzyprzystankowego przy prędkości swobodnej, w minutach —
    /// podstawa rozkładu i szacunku dla wyboru środka.
    free_hop_min: Vec<Vec<u16>>,
    pub stats: TransitStats,
}

impl PartialEq for TransitNetwork {
    fn eq(&self, other: &TransitNetwork) -> bool {
        self.stats == other.stats
            && self.runs.len() == other.runs.len()
            && self.runs.iter().zip(&other.runs).all(|(a, b)| {
                a.line == b.line
                    && a.stop_index == b.stop_index
                    && a.arrive_min == b.arrive_min
                    && a.occupancy == b.occupancy
            })
    }
}

impl Eq for TransitNetwork {}

impl TransitNetwork {
    #[must_use]
    pub fn new(lines: Vec<TransitLine>, road: &RoadGraph) -> TransitNetwork {
        let mut stops_flat = Vec::new();
        let mut punkty = Vec::new();
        for (li, l) in lines.iter().enumerate() {
            for (si, s) in l.stops.iter().enumerate() {
                stops_flat.push((li as u16, si as u16));
                punkty.push(s.at);
            }
        }
        let index = build_index(&punkty);
        let free_hop_min = lines
            .iter()
            .map(|l| {
                l.hops
                    .iter()
                    .map(|edges| {
                        let cs: u64 = edges
                            .iter()
                            .map(|e| {
                                u64::from(road.edges[e.0 as usize].free_flow_cs(road.modality))
                            })
                            .sum();
                        cs.div_ceil(CS_PER_MINUTE).clamp(1, u64::from(u16::MAX)) as u16
                    })
                    .collect()
            })
            .collect();
        TransitNetwork {
            lines,
            stops_flat,
            index,
            free_hop_min,
            ..TransitNetwork::default()
        }
    }

    #[must_use]
    pub fn lines(&self) -> &[TransitLine] {
        &self.lines
    }

    #[must_use]
    pub fn runs(&self) -> &[TransitRun] {
        &self.runs
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Ilu pasażerów czeka na wszystkich przystankach.
    #[must_use]
    pub fn waiting_total(&self) -> u64 {
        self.lines
            .iter()
            .map(|l| l.stops.iter().map(|s| s.waiting.len() as u64).sum::<u64>())
            .sum()
    }

    #[must_use]
    pub fn onboard_total(&self) -> u64 {
        self.runs.iter().map(|r| r.onboard.len() as u64).sum()
    }

    /// Czas, paliwo i koszt odcinka między przystankami `stop` i `stop + 1`.
    /// Zasila warstwę Mikro bryłami kursów (WP8, odpowiedź na `R-8`).
    ///
    /// **Kurs wchodzi do kadru, ale nie do kolejki krawędzi.** Tak było od M4c
    /// (`P-11`) i tak zostaje: gdyby autobus zajmował slot `EdgeQueue`, obłożenie
    /// krawędzi zależałoby od tego, którą warstwę akurat liczymy, a `micro_mezo_equivalence`
    /// przestałoby być prawdziwe z konstrukcji. Bryła odgrywa więc czas między
    /// przystankami z pary `(depart_min, arrive_min)` — dokładnie jak pieszy — i nie
    /// bierze udziału w car-followingu (`edge == NO_EDGE`).
    pub fn feed_micro(
        &self,
        micro: &crate::micro::MicroLayer,
        road: &RoadGraph,
        cat: &VehicleCatalog,
    ) {
        for r in &self.runs {
            if r.stop_index == 0 || r.arrive_min <= r.depart_min {
                continue;
            }
            let Some(li) = self.lines.iter().position(|l| l.id == r.line) else {
                continue;
            };
            let line = &self.lines[li];
            let Some(hop) = line.hops.get(usize::from(r.stop_index) - 1) else {
                continue;
            };
            if hop.is_empty() {
                continue;
            }
            let wezel = |n: magnat_nav::NodeId| {
                let p = road.nodes[n.0 as usize];
                WorldCoord::new(p.pos_cm.x, p.pos_cm.y, p.z_cm)
            };
            let mut trasa = Vec::with_capacity(hop.len() + 1);
            trasa.push(wezel(road.edge(hop[0]).from));
            for e in hop {
                trasa.push(wezel(road.edge(*e).to));
            }
            micro.feed_vehicle(
                crate::micro::VehicleFeed {
                    vehicle: r.vehicle,
                    edge: crate::micro::NO_EDGE,
                    class: line.class.0 as u8,
                    lanes: 1,
                    len_cm: cat.spec(line.class).length_cm,
                    v_free_cms: 1.0,
                    entry_cs: u64::from(r.depart_min) * CS_PER_MINUTE,
                    exit_cs: u64::from(r.arrive_min) * CS_PER_MINUTE,
                    car_following: false,
                },
                &trasa,
            );
        }
    }
}

impl HashState for TransitNetwork {
    /// Kursy w kolejności `(linia, pojazd)`, przystanki w kolejności indeksów —
    /// oba porządki totalne i niezależne od historii `swap_remove` (00 §3.2).
    fn hash_state(&self, h: &mut StateHasher) {
        let mut order: Vec<(u16, u32, usize)> = self
            .runs
            .iter()
            .enumerate()
            .map(|(i, r)| (r.line.0, r.vehicle, i))
            .collect();
        order.sort_unstable();
        h.write_u32(order.len() as u32);
        for (_, _, i) in order {
            let r = &self.runs[i];
            h.write_u16(r.line.0);
            h.write_u32(r.vehicle);
            h.write_u32(r.driver);
            h.write_u16(r.stop_index);
            h.write_u32(r.arrive_min);
            h.write_u32(r.depart_min);
            h.write_u16(r.occupancy);
            // §5.9 punkt 7 wymienia `delay_minutes` wprost: spóźnienie narastające jest
            // stanem kursu, a nie jego pomiarem — jutro decyduje o punktualności.
            h.write_u16(r.delay_minutes as u16);
            h.write_u64(r.fuel_ul as u64);
            for w in &r.onboard {
                h.write_u32(w.citizen);
                h.write_u16(w.alight_stop);
            }
        }
        for l in &self.lines {
            for s in &l.stops {
                h.write_u32(s.waiting.len() as u32);
                for w in &s.waiting {
                    h.write_u32(w.citizen);
                    h.write_u32(w.since_min);
                }
            }
        }
    }
}

fn build_index(punkty: &[WorldCoord]) -> Option<CsrGrid<u32>> {
    if punkty.is_empty() {
        return None;
    }
    let mut min = Vec2::new(f32::MAX, f32::MAX);
    let mut max = Vec2::new(f32::MIN, f32::MIN);
    for p in punkty {
        let v = Vec2::new(p.x as f32 / 100.0, p.y as f32 / 100.0);
        min = Vec2::new(min.x.min(v.x), min.y.min(v.y));
        max = Vec2::new(max.x.max(v.x), max.y.max(v.y));
    }
    let spec = GridSpec::covering(Aabb2::new(min, max), 150);
    Some(CsrGrid::build(
        spec,
        punkty
            .iter()
            .enumerate()
            .map(|(i, p)| (Vec2::new(p.x as f32 / 100.0, p.y as f32 / 100.0), i as u32)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{DistrictId, IVec2, RoadClass};
    use magnat_nav::{EdgeSpec, GeomRef, Modality, RoadGraphBuilder};

    /// Prosta linia: cztery przystanki w linii prostej co 500 m.
    fn linia() -> (TransitLine, RoadGraph) {
        let mut b = RoadGraphBuilder::new(Modality::Road);
        for i in 0..4 {
            b.add_node(IVec2::new(i * 50_000, 0), 0);
        }
        let mut hops = Vec::new();
        for i in 0..3u32 {
            let (e, _) = b.add_edge_pair(EdgeSpec {
                from: NodeId(i),
                to: NodeId(i + 1),
                geometry_ref: GeomRef(0),
                length_cm: 50_000,
                lanes: 2,
                class: RoadClass::Collector,
                speed_limit_dkmh: 500,
                max_mass: Mass::ZERO,
                grade_permille: 0,
                bridge: None,
                curb_parking: 0,
                district: DistrictId(0),
            });
            hops.push(vec![e]);
        }
        let road = b.finish();
        let stops = (0..4u32)
            .map(|i| TransitStop {
                node: NodeId(i),
                at: WorldCoord::new(i as i32 * 50_000, 0, 0),
                dwell_base_s: 20,
                waiting: Vec::new(),
            })
            .collect();
        let line = TransitLine {
            id: LineId(1),
            mode: TransitMode::Bus,
            stops,
            hops,
            timetable: Timetable {
                first_min: 5 * 60,
                last_min: 23 * 60,
                headway_peak_min: 10,
                headway_base_min: 20,
                days: 0b111_1111,
            },
            fleet: vec![0, 1],
            class: VehicleClassId(0),
            operator: OperatorRef::City,
            fare: Money(400),
            capacity: 2,
        };
        (line, road)
    }

    fn katalog() -> VehicleCatalog {
        VehicleCatalog::load_default().expect("data/vehicles/classes.ron")
    }

    #[test]
    fn rozklad_zageszcza_sie_w_szczycie() {
        let t = Timetable {
            first_min: 5 * 60,
            last_min: 23 * 60,
            headway_peak_min: 10,
            headway_base_min: 30,
            days: 0b111_1111,
        };
        // Od 5:00 co 30 min: 5:00, 5:30, 6:00 — i od 6:00 wchodzi odstęp szczytu.
        assert!(t.departs_at(5 * 60));
        assert!(t.departs_at(5 * 60 + 30));
        assert!(t.departs_at(6 * 60));
        assert!(
            t.departs_at(6 * 60 + 10),
            "w szczycie odstęp się nie zagęścił"
        );
        assert!(!t.departs_at(6 * 60 + 15));
        assert_eq!(t.next_departure(6 * 60 + 1), Some(6 * 60 + 10));
        assert_eq!(t.next_departure(23 * 60 + 30), None);
    }

    #[test]
    fn przepelnienie_zostawia_pasazera_na_przystanku() {
        let (line, road) = linia();
        let mut n = TransitNetwork::new(vec![line], &road);
        let mezo = MezoState::new(&road, &crate::spec::VdfTable::load_default().expect("vdf"));
        let cat = katalog();
        let j = TransitJourney {
            line: LineId(1),
            board_stop: 0,
            alight_stop: 3,
            access_min: 0,
            wait_min: 0,
            ride_min: 3,
            egress_min: 0,
            transfers: 0,
            fare: Money(400),
            transfer_to: (LineId(0), 0),
        };
        // Trzech chętnych, pojemność dwa.
        for c in 0..3u32 {
            n.enqueue(&j, c, 0, 5 * 60, 30);
        }
        let mut out = Vec::new();
        n.step_minute(
            5 * 60,
            DayOfWeek::from_day_index(0),
            &road,
            &mezo,
            &cat,
            &[100],
            &mut |_| true,
            &mut out,
        );
        assert_eq!(n.stats.boardings, 2, "wsiadło więcej niż mieści pojazd");
        assert_eq!(n.stats.left_behind, 1);
        assert!(out
            .iter()
            .any(|e| matches!(e, TransitEvent::LeftBehind { citizen: 2, .. })));
        assert_eq!(n.waiting_total(), 1, "pozostawiony zniknął z przystanku");
        assert_eq!(n.stats.fare_revenue, Money(800));
    }

    #[test]
    fn kurs_bez_kierowcy_jest_odwolany() {
        let (line, road) = linia();
        let mut n = TransitNetwork::new(vec![line], &road);
        let mezo = MezoState::new(&road, &crate::spec::VdfTable::load_default().expect("vdf"));
        let cat = katalog();
        let mut out = Vec::new();
        n.step_minute(
            5 * 60,
            DayOfWeek::from_day_index(0),
            &road,
            &mezo,
            &cat,
            &[100],
            &mut |_| false,
            &mut out,
        );
        assert_eq!(n.stats.runs_started, 0);
        assert_eq!(n.stats.runs_cancelled, 1);
        assert!(matches!(out[0], TransitEvent::RunCancelled { .. }));
    }

    #[test]
    fn pasazer_dojezdza_i_placi_a_kurs_pali_paliwo() {
        let (line, road) = linia();
        let mut n = TransitNetwork::new(vec![line], &road);
        let mut mezo = MezoState::new(&road, &crate::spec::VdfTable::load_default().expect("vdf"));
        let vdf = crate::spec::VdfTable::load_default().expect("vdf");
        mezo.freeze_minute(&road, &vdf);
        let cat = katalog();
        let j = TransitJourney {
            line: LineId(1),
            board_stop: 0,
            alight_stop: 2,
            access_min: 0,
            wait_min: 0,
            ride_min: 2,
            egress_min: 4,
            transfers: 0,
            fare: Money(400),
            transfer_to: (LineId(0), 0),
        };
        n.enqueue(&j, 7, 3, 5 * 60, 30);
        let mut out = Vec::new();
        let mut wysiadl = None;
        for m in 5 * 60..5 * 60 + 60 {
            out.clear();
            n.step_minute(
                m,
                DayOfWeek::from_day_index(0),
                &road,
                &mezo,
                &cat,
                &[100],
                &mut |_| true,
                &mut out,
            );
            if let Some(TransitEvent::Alighted {
                slot, egress_min, ..
            }) = out
                .iter()
                .find(|e| matches!(e, TransitEvent::Alighted { citizen: 7, .. }))
            {
                wysiadl = Some((m, *slot, *egress_min));
                break;
            }
        }
        let (minuta, slot, egress) = wysiadl.expect("pasażer nigdy nie wysiadł");
        assert_eq!(slot, 3, "slot planu zgubił się w podróży");
        assert_eq!(egress, 4);
        assert!(
            minuta > 5 * 60 && minuta < 5 * 60 + 15,
            "przejazd trwał {}",
            minuta - 300
        );
        assert!(
            n.stats.fuel_ul > 0,
            "autobus przejechał kilometr bez paliwa"
        );
        assert_eq!(n.stats.boardings, 1);
        assert_eq!(n.onboard_total(), 0);
    }

    #[test]
    fn czekajacy_za_dlugo_rezygnuje() {
        let (line, road) = linia();
        let mut n = TransitNetwork::new(vec![line], &road);
        let mezo = MezoState::new(&road, &crate::spec::VdfTable::load_default().expect("vdf"));
        let cat = katalog();
        let j = TransitJourney {
            line: LineId(1),
            board_stop: 1,
            alight_stop: 3,
            access_min: 0,
            wait_min: 0,
            ride_min: 2,
            egress_min: 0,
            transfers: 0,
            fare: Money(400),
            transfer_to: (LineId(0), 0),
        };
        // Wchodzi o 23:30 — po ostatnim odjeździe, więc żaden kurs go nie zabierze.
        n.enqueue(&j, 9, 1, 23 * 60 + 30, 30);
        let mut out = Vec::new();
        let mut poddal = false;
        for m in 23 * 60 + 30..23 * 60 + 30 + MAX_WAIT_MIN + 2 {
            out.clear();
            n.step_minute(
                m,
                DayOfWeek::from_day_index(0),
                &road,
                &mezo,
                &cat,
                &[100],
                &mut |_| true,
                &mut out,
            );
            poddal |= out
                .iter()
                .any(|e| matches!(e, TransitEvent::GaveUp { citizen: 9, .. }));
        }
        assert!(poddal, "pasażer stoi na przystanku do końca gry");
        assert_eq!(n.waiting_total(), 0);
    }

    #[test]
    fn kolejka_jest_fifo_z_remisem_po_indeksie_encji() {
        let (line, road) = linia();
        let mut n = TransitNetwork::new(vec![line], &road);
        let j = TransitJourney {
            line: LineId(1),
            board_stop: 0,
            alight_stop: 3,
            access_min: 0,
            wait_min: 0,
            ride_min: 3,
            egress_min: 0,
            transfers: 0,
            fare: Money(400),
            transfer_to: (LineId(0), 0),
        };
        // Zgłoszenia w odwrotnej kolejności: późniejszy pierwszy, potem dwaj z tej
        // samej minuty w kolejności malejącej.
        n.enqueue(&j, 5, 0, 310, 30);
        n.enqueue(&j, 9, 0, 300, 30);
        n.enqueue(&j, 3, 0, 300, 30);
        let kolejka = &n.lines[0].stops[0].waiting;
        let kolejnosc: Vec<u32> = kolejka.iter().map(|w| w.citizen).collect();
        assert_eq!(kolejnosc, vec![3, 9, 5], "kolejka nie ma klucza totalnego");
    }
}
