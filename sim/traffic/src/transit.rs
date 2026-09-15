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

use crate::mezo::{settle_edge, MezoState, VehicleSpecRef, CS_PER_MINUTE};
use crate::spec::{VehicleCatalog, VehicleClassId};
use magnat_core::{
    DayOfWeek, HashState, Mass, Money, SimMinute, StateHasher, WorldCoord,
};
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
        self.stops.iter().position(|s| s.node == node).map(|i| i as u16)
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
                            .map(|e| u64::from(road.edges[e.0 as usize].free_flow_cs(road.modality)))
                            .sum();
                        cs.div_ceil(CS_PER_MINUTE).clamp(1, u64::from(u16::MAX))
                            as u16
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

    /// Przystanki w promieniu dojścia od punktu, jako `(linia, przystanek, minuty)`.
    fn stops_near(&self, at: WorldCoord, radius_m: u16, out: &mut Vec<(u16, u16, u16)>) {
        out.clear();
        let Some(index) = self.index.as_ref() else {
            return;
        };
        let c = Vec2::new(at.x as f32 / 100.0, at.y as f32 / 100.0);
        index.for_each_in_radius(c, f32::from(radius_m), |_, i| {
            let (li, si) = self.stops_flat[i as usize];
            let s = &self.lines[li as usize].stops[si as usize];
            out.push((li, si, crate::parking::walk_minutes(s.at, at)));
        });
        // Klucz totalny: najpierw dojście, potem linia, potem przystanek. Bez tego
        // wynik zależałby od układu komórek indeksu przestrzennego.
        out.sort_unstable_by_key(|(li, si, m)| (*m, *li, *si));
    }

    /// Najlepsza podróż komunikacją między dwoma punktami albo `None`, gdy jej nie ma.
    ///
    /// Jedna linia albo jedna przesiadka. Kryterium jest sumaryczny czas, a remisy
    /// rozstrzyga `(linia, przystanek)` — deterministycznie, bez losowania.
    #[must_use]
    pub fn plan_journey(
        &self,
        from: WorldCoord,
        to: WorldCoord,
        depart: u16,
        dow: DayOfWeek,
    ) -> Option<TransitJourney> {
        if self.lines.is_empty() {
            return None;
        }
        let mut skad = Vec::new();
        let mut dokad = Vec::new();
        self.stops_near(from, MAX_ACCESS_M, &mut skad);
        self.stops_near(to, MAX_ACCESS_M, &mut dokad);
        if skad.is_empty() || dokad.is_empty() {
            return None;
        }
        let mut best: Option<(u16, TransitJourney)> = None;
        for (li, si, access) in &skad {
            let l = &self.lines[*li as usize];
            if !l.timetable.runs_on(dow) {
                continue;
            }
            let Some(odjazd) = l.timetable.next_departure(depart.saturating_add(*access)) else {
                continue;
            };
            let wait = odjazd.saturating_sub(depart.saturating_add(*access));
            for (lj, sj, egress) in &dokad {
                if lj != li || sj <= si {
                    continue;
                }
                let ride = self.ride_min(*li, *si, *sj);
                let j = TransitJourney {
                    line: l.id,
                    board_stop: *si,
                    alight_stop: *sj,
                    access_min: *access,
                    wait_min: wait,
                    ride_min: ride,
                    egress_min: *egress,
                    transfers: 0,
                    fare: l.fare,
                    transfer_to: (LineId(0), 0),
                };
                let suma = j.total_minutes();
                if best.as_ref().is_none_or(|(b, _)| suma < *b) {
                    best = Some((suma, j));
                }
            }
        }
        if best.is_none() {
            best = self.plan_with_transfer(&skad, &dokad, depart, dow);
        }
        best.map(|(_, j)| j)
    }

    /// Czas przejazdu między przystankami przy prędkości swobodnej, w minutach.
    fn ride_min(&self, line: u16, from: u16, to: u16) -> u16 {
        self.free_hop_min[line as usize][from as usize..to as usize]
            .iter()
            .map(|m| u32::from(*m))
            .sum::<u32>()
            .min(u32::from(u16::MAX)) as u16
    }

    /// Podróż z **jedną** przesiadką: linia A do przystanku wspólnego, linia B dalej.
    ///
    /// Przystanek wspólny to ten sam **węzeł grafu** na obu liniach — nie „blisko",
    /// tylko ten sam, bo przesiadka między przystankami oddalonymi o przecznicę jest
    /// dojściem, a dojście ma już swój składnik kosztu i nie należy go liczyć dwa razy.
    ///
    /// `ponytail:` jedna przesiadka, przeszukiwanie parami linii. Sufit nazwany
    /// i policzony: przy `L` liniach po `S` przystanków kosztuje to `O(L² · S)`,
    /// czyli przy 24 liniach po 20 przystanków ~11 tys. porównań na zapytanie —
    /// do przyjęcia, bo wołający ma jedno zapytanie na podróż, nie na krawędź.
    /// Ścieżka wyjścia: RAPTOR na tabeli połączeń, gdy linii będzie więcej niż
    /// kilkanaście na dzielnicę.
    fn plan_with_transfer(
        &self,
        skad: &[(u16, u16, u16)],
        dokad: &[(u16, u16, u16)],
        depart: u16,
        dow: DayOfWeek,
    ) -> Option<(u16, TransitJourney)> {
        let mut best: Option<(u16, TransitJourney)> = None;
        for (li, si, access) in skad {
            let a = &self.lines[*li as usize];
            if !a.timetable.runs_on(dow) {
                continue;
            }
            let Some(odjazd) = a.timetable.next_departure(depart.saturating_add(*access)) else {
                continue;
            };
            let wait = odjazd.saturating_sub(depart.saturating_add(*access));
            for (lj, sj, egress) in dokad {
                if lj == li {
                    continue;
                }
                let b = &self.lines[*lj as usize];
                if !b.timetable.runs_on(dow) {
                    continue;
                }
                // Przystanek wspólny: po `si` na linii A i przed `sj` na linii B.
                for (pa, sa) in a.stops.iter().enumerate().skip(*si as usize + 1) {
                    let Some(pb) = b.stops[..*sj as usize]
                        .iter()
                        .position(|s| s.node == sa.node)
                    else {
                        continue;
                    };
                    let pa = pa as u16;
                    let pb = pb as u16;
                    let jazda_a = self.ride_min(*li, *si, pa);
                    let jazda_b = self.ride_min(*lj, pb, *sj);
                    // Czekanie na drugą linię: połowa odstępu, bo rozkłady nie są
                    // zsynchronizowane. Wartość oczekiwana przy przyjeździe losowym.
                    let przesiadka = b.timetable.headway_base_min / 2;
                    let j = TransitJourney {
                        line: a.id,
                        board_stop: *si,
                        alight_stop: pa,
                        access_min: *access,
                        wait_min: wait,
                        ride_min: jazda_a
                            .saturating_add(przesiadka)
                            .saturating_add(jazda_b),
                        egress_min: *egress,
                        transfers: 1,
                        fare: Money(a.fare.0 + b.fare.0),
                        transfer_to: (b.id, *sj),
                    };
                    let suma = j.total_minutes();
                    if best.as_ref().is_none_or(|(x, _)| suma < *x) {
                        best = Some((suma, j));
                    }
                }
            }
        }
        best
    }

    /// Wstawia pasażera do kolejki przystanku. Wołane z `begin_trip`.
    pub fn enqueue(
        &mut self,
        j: &TransitJourney,
        citizen: u32,
        slot: u8,
        now: u32,
        walk_fallback_min: u16,
    ) {
        let Some(l) = self.lines.iter_mut().find(|l| l.id == j.line) else {
            return;
        };
        let Some(s) = l.stops.get_mut(j.board_stop as usize) else {
            return;
        };
        let w = Waiting {
            since_min: now,
            citizen,
            alight_stop: j.alight_stop,
            egress_min: j.egress_min,
            slot,
            next: j.transfer_to,
            walk_fallback_min,
            passed: 0,
        };
        // Kolejka jest posortowana po kluczu totalnym, więc wstawienie idzie na
        // właściwe miejsce, a nie na koniec: dwie osoby z tej samej minuty mają
        // wsiadać w kolejności indeksu encji niezależnie od kolejności zgłoszeń.
        let i = s.waiting.partition_point(|x| *x < w);
        s.waiting.insert(i, w);
    }

    /// Krok minutowy: odjazdy z rozkładu, przejazd kursów, wsiadanie i wysiadanie.
    ///
    /// `driver_ready` mówi, czy wskazany mieszkaniec jest w pracy — sieć nie ma
    /// dostępu do świata, więc pyta o to wołającego.
    #[allow(clippy::too_many_arguments)]
    pub fn step_minute(
        &mut self,
        now: u32,
        dow: DayOfWeek,
        road: &RoadGraph,
        mezo: &MezoState,
        cat: &VehicleCatalog,
        drivers: &[u32],
        driver_ready: &mut dyn FnMut(u32) -> bool,
        out: &mut Vec<TransitEvent>,
    ) {
        self.dispatch(now, dow, drivers, driver_ready, out);
        self.advance(now, road, mezo, cat, out);
        self.give_up(now, out);
    }

    /// Wypuszcza kursy, których odjazd wypada w tej minucie.
    fn dispatch(
        &mut self,
        now: u32,
        dow: DayOfWeek,
        drivers: &[u32],
        driver_ready: &mut dyn FnMut(u32) -> bool,
        out: &mut Vec<TransitEvent>,
    ) {
        let minuta_doby = (now % 1440) as u16;
        for li in 0..self.lines.len() {
            let (id, wypada, pojazdy, pojemnosc) = {
                let l = &self.lines[li];
                (
                    l.id,
                    l.timetable.runs_on(dow) && l.timetable.departs_at(minuta_doby),
                    l.fleet.clone(),
                    l.capacity,
                )
            };
            if !wypada || pojazdy.is_empty() {
                continue;
            }
            // Pojazd wolny = taki, którego nie prowadzi żaden kurs w toku. Kolejność
            // jest kolejnością taboru linii, więc ten sam rozkład daje ten sam pojazd.
            let Some(vehicle) = pojazdy
                .iter()
                .copied()
                .find(|v| !self.runs.iter().any(|r| r.vehicle == *v))
            else {
                self.stats.runs_cancelled += 1;
                out.push(TransitEvent::RunCancelled {
                    line: id,
                    at: SimMinute(u64::from(now)),
                });
                continue;
            };
            // Kierowca: pierwszy z listy linii, który jest dziś w pracy. Brak kierowcy
            // odwołuje kurs — to jest cały model absencji w tej podfazie (§5.6).
            let Some(driver) = drivers.iter().copied().find(|c| driver_ready(*c)) else {
                self.stats.runs_cancelled += 1;
                out.push(TransitEvent::RunCancelled {
                    line: id,
                    at: SimMinute(u64::from(now)),
                });
                continue;
            };
            self.runs.push(TransitRun {
                line: id,
                vehicle,
                driver,
                stop_index: 0,
                depart_min: now,
                arrive_min: now,
                occupancy: 0,
                capacity: pojemnosc,
                delay_minutes: 0,
                onboard: Vec::new(),
                fuel_ul: 0,
            });
            self.stats.runs_started += 1;
        }
    }

    /// Przesuwa kursy, które dotarły na przystanek: wysiadka, wsiadka, odjazd dalej.
    fn advance(
        &mut self,
        now: u32,
        road: &RoadGraph,
        mezo: &MezoState,
        cat: &VehicleCatalog,
        out: &mut Vec<TransitEvent>,
    ) {
        let at = SimMinute(u64::from(now));
        let mut zakonczone: Vec<usize> = Vec::new();
        for ri in 0..self.runs.len() {
            if self.runs[ri].arrive_min > now {
                continue;
            }
            let li = {
                let line = self.runs[ri].line;
                match self.lines.iter().position(|l| l.id == line) {
                    Some(i) => i,
                    None => continue,
                }
            };
            let stop = self.runs[ri].stop_index;

            // 1. Wysiadka — kto jedzie do tego przystanku, wysiada tutaj. Pasażer
            //    z przesiadką nie kończy tu podróży, tylko wraca do kolejki drugiej
            //    linii: gdyby dostał `Arrive`, przyjechałby do celu z przystanku
            //    przesiadkowego, czyli teleportował się przez drugą nogę.
            let mut wysiadlo = 0u32;
            let mut przesiadki: Vec<Waiting> = Vec::new();
            {
                let r = &mut self.runs[ri];
                r.onboard.retain(|w| {
                    if w.alight_stop != stop {
                        return true;
                    }
                    wysiadlo += 1;
                    if w.next.0 != LineId(0) {
                        przesiadki.push(*w);
                    } else {
                        out.push(TransitEvent::Alighted {
                            citizen: w.citizen,
                            slot: w.slot,
                            egress_min: w.egress_min,
                            at,
                        });
                    }
                    false
                });
                r.occupancy = r.occupancy.saturating_sub(wysiadlo.min(u32::from(u16::MAX)) as u16);
            }
            self.stats.alightings += u64::from(wysiadlo);
            for w in przesiadki {
                self.przesiadka(&w, self.lines[li].stops[stop as usize].node, now, out);
            }

            // 2. Wsiadka — FIFO, aż do pojemności. Reszta zostaje na przystanku.
            let ostatni = self.lines[li].stops.len() as u16 - 1;
            let mut wsiadlo = 0u32;
            if stop < ostatni {
                let (fare, id) = (self.lines[li].fare, self.lines[li].id);
                let wolne = self.runs[ri].capacity.saturating_sub(self.runs[ri].occupancy);
                let kolejka = &mut self.lines[li].stops[stop as usize].waiting;
                let mut zostaja: Vec<Waiting> = Vec::new();
                for w in std::mem::take(kolejka) {
                    if w.alight_stop <= stop {
                        // Kurs minął cel pasażera — poczeka na następny, który zaczyna
                        // od pierwszego przystanku.
                        zostaja.push(w);
                        continue;
                    }
                    if wsiadlo < u32::from(wolne) {
                        let czekal = now.saturating_sub(w.since_min).min(u32::from(u16::MAX)) as u16;
                        self.stats.wait_minutes += u64::from(czekal);
                        out.push(TransitEvent::Boarded {
                            citizen: w.citizen,
                            line: id,
                            fare,
                            wait_min: czekal,
                            at,
                        });
                        self.runs[ri].onboard.push(w);
                        wsiadlo += 1;
                    } else {
                        out.push(TransitEvent::LeftBehind {
                            citizen: w.citizen,
                            line: id,
                            at,
                        });
                        let mut w = w;
                        if w.passed == 0 {
                            self.stats.left_behind += 1;
                        }
                        w.passed = w.passed.saturating_add(1);
                        self.stats.boarding_refusals += 1;
                        zostaja.push(w);
                    }
                }
                self.lines[li].stops[stop as usize].waiting = zostaja;
                let r = &mut self.runs[ri];
                r.occupancy = r
                    .occupancy
                    .saturating_add(wsiadlo.min(u32::from(u16::MAX)) as u16);
                self.stats.boardings += u64::from(wsiadlo);
                self.stats.max_occupancy = self.stats.max_occupancy.max(r.occupancy);
                self.stats.fare_revenue = Money(
                    self.stats.fare_revenue.0 + fare.0 * i64::from(wsiadlo.min(u32::from(u16::MAX))),
                );
            }

            // 3. Postój: baza plus czas wymiany pasażerów. To on zamienia przepełnienie
            //    w spóźnienie — im więcej chętnych, tym dłużej autobus stoi.
            let dwell_s = u32::from(self.lines[li].stops[stop as usize].dwell_base_s)
                + wsiadlo * BOARD_S
                + wysiadlo * ALIGHT_S;

            if stop >= ostatni {
                zakonczone.push(ri);
                continue;
            }

            // 4. Przejazd do następnego przystanku — **tym samym `settle_edge`**, którym
            //    jadą samochody. Autobus dzieli korek z resztą miasta; tramwaj i metro
            //    mają wydzielone torowisko, więc jadą swobodnie.
            let (czas_cs, paliwo_ul, koszt) =
                self.hop_cost(li, stop as usize, road, mezo, cat, now);
            let r = &mut self.runs[ri];
            r.fuel_ul += paliwo_ul;
            r.stop_index = stop + 1;
            r.depart_min = now;
            let minut = (u64::from(dwell_s) * 100 + czas_cs)
                .div_ceil(CS_PER_MINUTE)
                .max(1);
            r.arrive_min = now + minut.min(u64::from(u32::MAX)) as u32;

            // Spóźnienie narastające: różnica między tym, ile przejazd trwał naprawdę,
            // a ile trwałby przy prędkości swobodnej.
            let swobodnie = u64::from(self.free_hop_min[li][stop as usize]);
            let opoznienie = minut.saturating_sub(swobodnie).min(i16::MAX as u64) as i16;
            r.delay_minutes = r.delay_minutes.saturating_add(opoznienie);
            self.stats.delay_minutes += i64::from(opoznienie);
            self.stats.max_delay_minutes = self.stats.max_delay_minutes.max(r.delay_minutes);
            if paliwo_ul > 0 {
                let (id, vehicle) = (r.line, r.vehicle);
                self.stats.fuel_ul += paliwo_ul;
                self.stats.fuel_cost = Money(self.stats.fuel_cost.0 + koszt.0);
                out.push(TransitEvent::RunFuelled {
                    line: id,
                    vehicle,
                    units_ul: paliwo_ul,
                    cost: koszt,
                    at,
                });
            }
        }

        // Kursy na pętli: wszyscy jeszcze na pokładzie wysiadają, kurs znika.
        for ri in zakonczone.into_iter().rev() {
            let r = self.runs.swap_remove(ri);
            for w in r.onboard {
                self.stats.alightings += 1;
                out.push(TransitEvent::Alighted {
                    citizen: w.citizen,
                    slot: w.slot,
                    egress_min: w.egress_min,
                    at,
                });
            }
            self.stats.runs_finished += 1;
        }
    }

    /// Wstawia pasażera do kolejki drugiej linii na przystanku przesiadkowym.
    ///
    /// Gdy druga linia nie obsługuje już tego węzła (zmiana trasy), podróż kończy się
    /// tutaj z dojściem pieszo — to jest ta sama reguła co `GaveUp`: zawsze musi być
    /// wyjście, które się kończy (`M-4`).
    fn przesiadka(&mut self, w: &Waiting, node: NodeId, now: u32, out: &mut Vec<TransitEvent>) {
        let (line, cel) = w.next;
        let Some(lj) = self.lines.iter().position(|l| l.id == line) else {
            out.push(TransitEvent::Alighted {
                citizen: w.citizen,
                slot: w.slot,
                egress_min: w.egress_min,
                at: SimMinute(u64::from(now)),
            });
            return;
        };
        let Some(board) = self.lines[lj].stop_at(node) else {
            out.push(TransitEvent::Alighted {
                citizen: w.citizen,
                slot: w.slot,
                egress_min: w.egress_min,
                at: SimMinute(u64::from(now)),
            });
            return;
        };
        let dalej = Waiting {
            since_min: now,
            citizen: w.citizen,
            alight_stop: cel,
            egress_min: w.egress_min,
            slot: w.slot,
            next: (LineId(0), 0),
            walk_fallback_min: w.walk_fallback_min,
            passed: 0,
        };
        let kolejka = &mut self.lines[lj].stops[board as usize].waiting;
        let i = kolejka.partition_point(|x| *x < dalej);
        kolejka.insert(i, dalej);
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
    pub fn feed_micro(&self, micro: &crate::micro::MicroLayer, road: &RoadGraph, cat: &VehicleCatalog) {
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

    fn hop_cost(
        &self,
        li: usize,
        stop: usize,
        road: &RoadGraph,
        mezo: &MezoState,
        cat: &VehicleCatalog,
        now: u32,
    ) -> (u64, i64, Money) {
        let l = &self.lines[li];
        if !l.mode.shares_road() {
            // Wydzielone torowisko: czas swobodny, bez wpływu ruchu drogowego (§5.6).
            return (
                u64::from(self.free_hop_min[li][stop]) * CS_PER_MINUTE,
                0,
                Money::ZERO,
            );
        }
        let veh = VehicleSpecRef {
            cat,
            class: l.class,
        };
        let mut cs = 0u64;
        let mut paliwo = 0i64;
        let mut entry_cs = u64::from(now) * CS_PER_MINUTE;
        for e in &l.hops[stop] {
            let edge = &road.edges[e.0 as usize];
            let link = &mezo.links[e.0 as usize];
            // Przystanek na początku odcinka to jedno zatrzymanie; zimny start
            // wyłącznie na pierwszej krawędzi kursu.
            let wpis = settle_edge(
                edge,
                link,
                &veh,
                Mass::ZERO,
                entry_cs,
                u8::from(e == &l.hops[stop][0]),
                stop == 0 && e == &l.hops[stop][0],
            );
            cs += u64::from(wpis.travel_cs);
            paliwo += wpis.fuel_ul;
            entry_cs += u64::from(wpis.travel_cs);
        }
        let koszt = cat.fuel_cost(cat.spec(l.class).fuel, paliwo);
        (cs, paliwo, koszt)
    }

    /// Pasażerowie, którzy czekają dłużej niż [`MAX_WAIT_MIN`], rezygnują.
    fn give_up(&mut self, now: u32, out: &mut Vec<TransitEvent>) {
        let at = SimMinute(u64::from(now));
        let mut poddanych = 0u64;
        for l in &mut self.lines {
            for s in &mut l.stops {
                s.waiting.retain(|w| {
                    if now.saturating_sub(w.since_min) < MAX_WAIT_MIN {
                        return true;
                    }
                    poddanych += 1;
                    out.push(TransitEvent::GaveUp {
                        citizen: w.citizen,
                        slot: w.slot,
                        walk_min: w.walk_fallback_min,
                        at,
                    });
                    false
                });
            }
        }
        self.stats.gave_up += poddanych;
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
        assert!(t.departs_at(6 * 60 + 10), "w szczycie odstęp się nie zagęścił");
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
            if let Some(TransitEvent::Alighted { slot, egress_min, .. }) = out
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
        assert!(minuta > 5 * 60 && minuta < 5 * 60 + 15, "przejazd trwał {}", minuta - 300);
        assert!(n.stats.fuel_ul > 0, "autobus przejechał kilometr bez paliwa");
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
