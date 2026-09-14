//! Estymator dojścia pieszo — implementacja tymczasowa `TravelOracle` (K-2).
//!
//! **Moduł jest `pub(crate)` i taki zostaje.** Właścicielem grafu nawigacyjnego jest
//! M4 (`engine/nav`); M3 ma tu wyłącznie prywatny estymator za wspólnym interfejsem.
//! Zero publicznych typów grafu, zero funkcji `route`/`path`/`graph` w API crate'a —
//! kryterium akceptacyjne nr 8 fazy sprawdza to testem architektonicznym. Gdy M4
//! dostarczy swoją implementację, ten moduł **znika w całości**; nie ma tu nic
//! do zmigrowania i nic, co trzeba by utrzymywać równolegle.
//!
//! **M3b (WP6) zastąpił odległość manhattanowską odległością sieciową** po centroliniach
//! ulic z M2 — bez zmiany ani jednej sygnatury, bo to jest cały sens `TravelOracle`.
//! Sieć wsypuje się z zewnątrz jako dwie płaskie tablice (`with_streets`), tak samo jak
//! katalog miejsc (korekta A-5): `sim/agents` **nie zależy od `sim/world`**. Bez sieci
//! `WalkOracle::new` nadal liczy manhattanowo z korektą 1,25× — to jest fallback
//! z ryzyka R7 i zostaje dla testów oraz scenariuszy bez miasta.
//!
//! **Mikro** (§5.10) to `PedestrianBuffer`: pozycja interpolowana po polilinii trasy
//! na ticku 100 ms. Czas przybycia liczy **wyłącznie** warstwa mezo — mikro interpoluje
//! między wyliczonym startem a wyliczonym przybyciem i nie ma jak go zmienić. Dlatego
//! test spójności LOD przechodzi z tolerancją 0 **z definicji**, a nie przez kalibrację.
//!
//! Arytmetyka jest całkowitoliczbowa w centymetrach. Nie dlatego, że float byłby
//! niedeterministyczny (00 §K-6 mówi, że nie byłby), tylko dlatego, że czas przybycia
//! jest stanem trwałym i wchodzi do hasha — a liczba całkowita nie ma wariantów
//! zaokrąglenia, o które można się spierać przy porównaniu mikro z mezo.

use crate::des::{EventKind, EventQueue, SimEvent};
use crate::places::{
    coord_to_vec2, CitizenView, PlaceTable, TravelEstimate, TravelOracle, TripHandle, TripRequest,
    MAX_ON_ROUTE,
};
use crate::ArrayVec;
use magnat_core::{
    seeded_map, DecisionReason, MinuteOfDay, Money, PlaceRef, SeededMap, TransportMode, WorldCoord,
};
use magnat_spatial::{Aabb2, CsrGrid, GridSpec, Vec2};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Prędkość marszu: 1,35 m/s = 81 m/min = 8100 cm/min (§5.10).
pub(crate) const BASE_SPEED_CM_PER_MIN: i64 = 8_100;

/// Ta sama prędkość w metrach na minutę — do przeliczenia zasięgu osobistego
/// na promień zapytania przestrzennego.
pub const BASE_SPEED_M_PER_MIN: f32 = 81.0;

/// Korekta odległości manhattanowej do sieciowej — używana wyłącznie tam, gdzie sieci
/// nie ma: przy zgrubnym rankingu kandydatów (`InfinitePlaces`) i w fallbacku z R7.
const DETOUR_NUM: i64 = 125;
const DETOUR_DEN: i64 = 100;

/// Szerokość korytarza, w którym miejsce liczy się jako „widoczne z trasy" (§5.7).
const ROUTE_CORRIDOR_CM: i64 = 6_000; // 60 m

/// Zasięg pieszy: powyżej pięciu kilometrów Dijkstra przerywa i oddaje fallback.
///
/// `ponytail:` odcięcie promieniem zamiast dwukierunkowego A*. Sufit nazwany: dla tras
/// dłuższych niż 5 km wracamy do oszacowania manhattanowego. Przy dystansach pieszych
/// (≤ 3 km) to jest ta sama robota za połowę kodu, a cała implementacja i tak jest
/// do wyrzucenia w M4 razem z modułem.
const MAX_WALK_CM: u32 = 500_000;

/// Rozmiar komórki indeksu węzłów. 60 m to mniej niż typowy kwartał, więc
/// „najbliższy węzeł" znajduje się zwykle w jednej komórce.
const NODE_CELL_M: u16 = 60;

/// Pojemność cache'u odległości; po przekroczeniu czyścimy go w całości.
///
/// `ponytail:` czyszczenie zamiast LRU. Sufit nazwany: raz na jakiś czas tracimy
/// wszystkie trafienia naraz zamiast najstarszych. Lista dwukierunkowa LRU kosztowałaby
/// w ścieżce gorącej więcej, niż daje, bo mieszkaniec pyta w dobie o 2–3 pary miejsc
/// (dom↔praca, dom↔sklep) i para dom↔praca siedzi w cache'u tak długo, jak jest pytana.
const CACHE_MAX: usize = 1 << 18;

/// Czas dojścia w minutach, zawsze ≥ 1: podróż zerominutowa wywróciłaby kolejkę
/// zdarzeń (dwa zdarzenia tego samego aktora w tej samej minucie, §5.2).
#[inline]
#[must_use]
fn minutes_for(dystans_cm: i64, speed_pct: u32) -> u16 {
    let predkosc = (BASE_SPEED_CM_PER_MIN * i64::from(speed_pct) / 100).max(1);
    ((dystans_cm + predkosc - 1) / predkosc).clamp(1, i64::from(u16::MAX)) as u16
}

/// Odległość manhattanowa z korektą — zgrubna miara bez sieci ulic.
#[inline]
#[must_use]
fn manhattan_cm(from: WorldCoord, to: WorldCoord) -> i64 {
    let dx = i64::from(from.x - to.x).abs();
    let dy = i64::from(from.y - to.y).abs();
    (dx + dy) * DETOUR_NUM / DETOUR_DEN
}

/// Odległość w linii prostej w centymetrach. `isqrt` zamiast `f64::sqrt`, bo wynik
/// wchodzi do czasu przybycia, czyli do hasha stanu.
#[inline]
#[must_use]
fn euclid_cm(a: WorldCoord, b: WorldCoord) -> u32 {
    (a.distance_sq_xy(b).max(0) as u64)
        .isqrt()
        .min(u64::from(u32::MAX)) as u32
}

/// Czas dojścia liczony manhattanowo — fallback z R7 i ranking kandydatów.
#[must_use]
pub(crate) fn walk_minutes(from: WorldCoord, to: WorldCoord, speed_pct: u32) -> u16 {
    minutes_for(manhattan_cm(from, to), speed_pct)
}

/// Prędkość marszu mieszkańca jako procent bazowej (§5.10: wiek, zdrowie, energia).
///
/// Progi, nie krzywa: różnica między 96 % a 97 % prędkości nie jest widoczna ani
/// w histogramie dojazdów, ani w karcie inspekcji, a krzywa wymagałaby kalibracji,
/// której nie ma na czym oprzeć przed M3d.
#[must_use]
pub(crate) fn speed_pct(who: &CitizenView<'_>) -> u32 {
    let lata = who.identity.age_years(who.today);
    let mut pct: i32 = match lata {
        ..=5 => 60,
        6..=13 => 85,
        14..=64 => 100,
        65..=79 => 85,
        _ => 65,
    };
    if who.vitals.health < 40 {
        pct -= 15;
    }
    if who.vitals.energy < 30 {
        pct -= 10;
    }
    pct.clamp(45, 110) as u32
}

// ── graf odcinków ulic (prywatny; M4 kasuje razem z modułem) ────────────────────

/// Ważony graf odcinków ulic zbudowany z centrolinii M2 (decyzja 9.4: lista
/// `(a, b, długość)` plus pozycje węzłów — **nie** graf nawigacyjny).
///
/// CSR zamiast listy sąsiedztwa per węzeł: sieć powstaje raz przy starcie i potem
/// jest tylko czytana, a Dijkstra dotyka sąsiadów ciągiem.
struct StreetGraph {
    nodes: Vec<WorldCoord>,
    adj_start: Vec<u32>,
    adj_node: Vec<u32>,
    adj_len: Vec<u32>,
    index: CsrGrid<u32>,
}

impl StreetGraph {
    fn build(nodes: &[WorldCoord], segments: &[(u32, u32, u32)]) -> StreetGraph {
        let n = nodes.len();
        let poprawny =
            |s: &&(u32, u32, u32)| (s.0 as usize) < n && (s.1 as usize) < n && s.0 != s.1;

        let mut adj_start = vec![0u32; n + 1];
        for s in segments.iter().filter(poprawny) {
            adj_start[s.0 as usize + 1] += 1;
            adj_start[s.1 as usize + 1] += 1;
        }
        for i in 0..n {
            adj_start[i + 1] += adj_start[i];
        }

        let ile = adj_start[n] as usize;
        let mut kursor = adj_start.clone();
        let mut adj_node = vec![0u32; ile];
        let mut adj_len = vec![0u32; ile];
        for s in segments.iter().filter(poprawny) {
            // Długość z danych, nie z pozycji węzłów: odcinek jest łamaną, a nie
            // cięciwą — M2 zna jej realną długość, my znamy tylko końce.
            let d = s.2.max(1);
            for (a, b) in [(s.0, s.1), (s.1, s.0)] {
                let p = kursor[a as usize] as usize;
                adj_node[p] = b;
                adj_len[p] = d;
                kursor[a as usize] += 1;
            }
        }

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
        let index = CsrGrid::build(
            spec,
            nodes
                .iter()
                .enumerate()
                .map(|(i, p)| (coord_to_vec2(*p), i as u32)),
        );

        StreetGraph {
            nodes: nodes.to_vec(),
            adj_start,
            adj_node,
            adj_len,
            index,
        }
    }

    /// Najbliższy węzeł sieci. Remis rozstrzyga indeks węzła, nie układ komórek
    /// indeksu przestrzennego — inaczej trasa zależałaby od rozmiaru komórki.
    fn nearest(&self, at: WorldCoord, scratch: &mut Vec<(f32, u32)>) -> Option<u32> {
        if self.nodes.is_empty() {
            return None;
        }
        scratch.clear();
        self.index.k_nearest(coord_to_vec2(at), 4, scratch);
        scratch
            .iter()
            .map(|(_, i)| (euclid_cm(at, self.nodes[*i as usize]), *i))
            .min()
            .map(|(_, i)| i)
    }

    #[inline]
    fn neighbours(&self, n: u32) -> (&[u32], &[u32]) {
        let a = self.adj_start[n as usize] as usize;
        let b = self.adj_start[n as usize + 1] as usize;
        (&self.adj_node[a..b], &self.adj_len[a..b])
    }
}

/// Bufory robocze Dijkstry i cache odległości. Jeden zamek na obie rzeczy, bo obie
/// są dotykane w tym samym wywołaniu.
///
/// `ponytail:` globalny `Mutex` zamiast bufora per wątek. Sufit nazwany: przy
/// planowaniu równoległym (M3d §5.12) to jest punkt rywalizacji. W M3b planer biegnie
/// z jednego wątku, a M4 i tak pisze `engine/nav` od nowa — bufor per wątek dopiero,
/// gdy pomiar pokaże, że to boli.
struct Scratch {
    dist: Vec<u32>,
    prev: Vec<u32>,
    /// Znacznik generacji zamiast czyszczenia `dist` — przy 30 tys. węzłów zerowanie
    /// tablicy kosztowałoby więcej niż sama Dijkstra na dystansie pieszym.
    stamp: Vec<u32>,
    gen: u32,
    heap: BinaryHeap<Reverse<(u32, u32)>>,
    knn: Vec<(f32, u32)>,
    cache: SeededMap<(PlaceRef, PlaceRef), u32>,
}

impl Scratch {
    fn new(nodes: usize) -> Scratch {
        Scratch {
            dist: vec![0; nodes],
            prev: vec![u32::MAX; nodes],
            stamp: vec![0; nodes],
            gen: 0,
            heap: BinaryHeap::new(),
            knn: Vec::new(),
            cache: seeded_map(),
        }
    }

    fn nowa_generacja(&mut self) {
        self.gen = match self.gen.checked_add(1) {
            Some(g) => g,
            None => {
                self.stamp.fill(0);
                1
            }
        };
        self.heap.clear();
    }
}

/// Estymator dojścia. Trzyma katalog miejsc, opcjonalną sieć ulic, bufory robocze
/// i pieszych w kadrze — nic więcej, bo nic więcej nie jest mu potrzebne do policzenia
/// czasu i zaplanowania `Arrive`.
pub struct WalkOracle {
    places: Arc<PlaceTable>,
    streets: Option<StreetGraph>,
    scratch: Mutex<Scratch>,
    next_trip: AtomicU32,
    pedestrians: Mutex<PedestrianBuffer>,
}

impl WalkOracle {
    /// Bez sieci ulic: odległość manhattanowa z korektą 1,25× (fallback z ryzyka R7).
    #[must_use]
    pub fn new(places: Arc<PlaceTable>) -> WalkOracle {
        WalkOracle {
            places,
            streets: None,
            scratch: Mutex::new(Scratch::new(0)),
            next_trip: AtomicU32::new(0),
            pedestrians: Mutex::new(PedestrianBuffer::new()),
        }
    }

    /// Z siecią ulic M2. Parametry są **płaskie celowo**: gdyby `with_streets`
    /// przyjmował typ grafu, ten typ musiałby być publiczny — a wtedy K-2 przestałoby
    /// obowiązywać w praktyce, mimo że obowiązuje na papierze.
    ///
    /// `nodes` to pozycje węzłów, `segments` to trójki `(węzeł A, węzeł B, długość w cm)`;
    /// długość pochodzi z centrolinii M2, a nie z odległości między końcami, bo odcinek
    /// jest łamaną. Odcinki wskazujące poza tablicę węzłów i pętle własne są pomijane —
    /// sieć z M2 nie ma prawa takich mieć, ale scenariusz testowy ma prawo się pomylić.
    #[must_use]
    pub fn with_streets(
        places: Arc<PlaceTable>,
        nodes: &[WorldCoord],
        segments: &[(u32, u32, u32)],
    ) -> WalkOracle {
        let graf = StreetGraph::build(nodes, segments);
        let n = graf.nodes.len();
        WalkOracle {
            places,
            streets: Some(graf),
            scratch: Mutex::new(Scratch::new(n)),
            next_trip: AtomicU32::new(0),
            pedestrians: Mutex::new(PedestrianBuffer::new()),
        }
    }

    /// Czy estymator stoi na sieci ulic, czy na fallbacku manhattanowym.
    #[must_use]
    pub fn has_streets(&self) -> bool {
        self.streets.is_some()
    }

    fn coord(&self, p: PlaceRef) -> WorldCoord {
        match self.places.coord_of(p) {
            Some(c) => c,
            None => {
                // Miejsce spoza katalogu to błąd wywołującego: kandydaci pochodzą
                // z `PlaceProvider`, a dom z `Residence`. W debug pada test, w release
                // podróż trwa minutę — symulacja nie ma się zatrzymywać przez jeden
                // wpis w danych.
                debug_assert!(false, "miejsce {p:?} spoza katalogu");
                WorldCoord::ORIGIN
            }
        }
    }

    /// Odległość sieciowa między miejscami, w centymetrach. Cache jest po parze miejsc,
    /// a nie po czasie: czas zależy od mieszkańca (wiek, zdrowie, energia), odległość nie.
    fn distance_cm(&self, from: PlaceRef, to: PlaceRef) -> i64 {
        if from == to {
            return 0;
        }
        let a = self.coord(from);
        let b = self.coord(to);
        let Some(graf) = self.streets.as_ref() else {
            return manhattan_cm(a, b);
        };
        // Klucz jest symetryczny: marsz tam i z powrotem to ta sama odległość,
        // więc para dom↔praca zajmuje w cache'u jeden wpis, nie dwa.
        let klucz = if from <= to { (from, to) } else { (to, from) };

        let mut s = self.scratch.lock().expect("scratch");
        if let Some(d) = s.cache.get(&klucz) {
            return i64::from(*d);
        }
        let d = dijkstra(graf, &mut s, a, b, None)
            .unwrap_or_else(|| manhattan_cm(a, b).clamp(0, i64::from(u32::MAX)) as u32);
        if s.cache.len() >= CACHE_MAX {
            s.cache.clear();
        }
        s.cache.insert(klucz, d);
        i64::from(d)
    }

    /// Trasa jako łamana po węzłach sieci, z dołożonymi końcami (drzwi → węzeł).
    /// Prywatna i wołana wyłącznie przy wejściu w LOD Mikro oraz przy szukaniu miejsc
    /// widocznych z trasy — mezo jej nie potrzebuje.
    fn route_points(&self, from: PlaceRef, to: PlaceRef, out: &mut Vec<WorldCoord>) {
        out.clear();
        let a = self.coord(from);
        let b = self.coord(to);
        out.push(a);
        if let Some(graf) = self.streets.as_ref() {
            let mut s = self.scratch.lock().expect("scratch");
            let mut wezly = Vec::new();
            if dijkstra(graf, &mut s, a, b, Some(&mut wezly)).is_some() {
                for n in wezly {
                    out.push(graf.nodes[n as usize]);
                }
            }
        }
        out.push(b);
        out.dedup();
    }

    /// Wpuszcza podróż do warstwy Mikro (§5.10). Wołane, gdy mieszkaniec wchodzi
    /// w kadr; mezo działa bez tego i nie zmienia się od tego ani o minutę.
    ///
    /// Cztery metody `micro_*` są **publiczne, ale nie zdradzają grafu**: biorą
    /// i oddają pozycje oraz postęp, a nie trasy, węzły czy odcinki. Dzięki temu
    /// renderer i system LOD mają czym karmić kadr, a K-2 nadal obowiązuje —
    /// `PedestrianBuffer` zostaje prywatny i M4 kasuje go razem z modułem.
    pub fn enter_micro(&self, handle: &TripHandle, citizen: u32, depart: MinuteOfDay) {
        let mut trasa = Vec::new();
        self.route_points(handle.from, handle.to, &mut trasa);
        if trasa.len() < 2 {
            return;
        }
        let przybycie = depart.get().saturating_add(handle.minutes);
        self.pedestrians.lock().expect("pedestrians").spawn(
            citizen,
            &trasa,
            depart.get(),
            przybycie,
        );
    }

    /// Krok mikro: `now_ms` to milisekunda doby, tick 100 ms (00 §4).
    pub fn micro_step(&self, now_ms: u64) {
        self.pedestrians.lock().expect("pedestrians").step(now_ms);
    }

    /// Usuwa pieszych, którzy już dotarli.
    pub fn micro_retire(&self, now_min: u16) {
        self.pedestrians
            .lock()
            .expect("pedestrians")
            .retire(now_min);
    }

    #[must_use]
    pub fn micro_len(&self) -> usize {
        self.pedestrians.lock().expect("pedestrians").len()
    }

    /// Zrzut dla renderera: `(indeks encji, pozycja w metrach, postęp 0..=1)`.
    pub fn micro_snapshot(&self, out: &mut Vec<(u32, [f32; 3], f32)>) {
        out.clear();
        let buf = self.pedestrians.lock().expect("pedestrians");
        for i in 0..buf.len() {
            let p = buf.get(i);
            out.push((p.citizen, p.pos, p.progress));
        }
    }
}

/// Dijkstra z odcięciem promieniem i wcześniejszym przerwaniem na celu.
///
/// Zwraca odległość `drzwi → węzeł → … → węzeł → drzwi` w centymetrach, albo `None`,
/// gdy cel jest poza zasięgiem pieszym lub w innej spójnej składowej. Kiedy `path`
/// jest podane, wypełnia je ciągiem węzłów od startu do celu.
fn dijkstra(
    graf: &StreetGraph,
    s: &mut Scratch,
    a: WorldCoord,
    b: WorldCoord,
    path: Option<&mut Vec<u32>>,
) -> Option<u32> {
    let mut knn = std::mem::take(&mut s.knn);
    let start = graf.nearest(a, &mut knn);
    let meta = graf.nearest(b, &mut knn);
    s.knn = knn;
    let (start, meta) = (start?, meta?);

    let wejscie = euclid_cm(a, graf.nodes[start as usize]);
    let wyjscie = euclid_cm(b, graf.nodes[meta as usize]);

    if start == meta {
        if let Some(p) = path {
            p.clear();
            p.push(start);
        }
        return Some(wejscie.saturating_add(wyjscie));
    }

    // A*, nie czysta Dijkstra: cel jest znany, a odległość euklidesowa do niego jest
    // heurystyką **dopuszczalną i spójną** — odcinek ulicy jest łamaną, więc nigdy
    // nie jest krótszy od odcinka między swoimi końcami. Wynik jest ten sam co
    // Dijkstry (pilnuje tego `astar_daje_te_sama_odleglosc_co_dijkstra`), a przy
    // dojazdach rzędu pięciu kilometrów w sieci metropolii przeszukanie jest
    // kilkakrotnie mniejsze. Bez tego Etap 8 zjada 25 s z budżetu 30 s (§7.4).
    let cel_wezla = graf.nodes[meta as usize];
    let h = |n: u32| euclid_cm(graf.nodes[n as usize], cel_wezla);

    s.nowa_generacja();
    let gen = s.gen;
    s.dist[start as usize] = 0;
    s.prev[start as usize] = u32::MAX;
    s.stamp[start as usize] = gen;
    s.heap.push(Reverse((h(start), start)));

    let mut wynik = None;
    while let Some(Reverse((f, n))) = s.heap.pop() {
        let d = s.dist[n as usize];
        if s.stamp[n as usize] != gen || f != d.saturating_add(h(n)) {
            continue; // nieaktualny wpis — kopiec nie umie zmniejszać klucza
        }
        if n == meta {
            wynik = Some(d);
            break;
        }
        if d > MAX_WALK_CM {
            break;
        }
        let (sasiedzi, dlugosci) = graf.neighbours(n);
        for (i, m) in sasiedzi.iter().enumerate() {
            let nd = d.saturating_add(dlugosci[i]);
            if s.stamp[*m as usize] != gen || nd < s.dist[*m as usize] {
                s.dist[*m as usize] = nd;
                s.prev[*m as usize] = n;
                s.stamp[*m as usize] = gen;
                s.heap.push(Reverse((nd.saturating_add(h(*m)), *m)));
            }
        }
    }

    let d = wynik?;
    if let Some(p) = path {
        p.clear();
        let mut n = meta;
        loop {
            p.push(n);
            if n == start || n == u32::MAX {
                break;
            }
            n = s.prev[n as usize];
        }
        p.reverse();
    }
    Some(d.saturating_add(wejscie).saturating_add(wyjscie))
}

impl TravelOracle for WalkOracle {
    fn estimate(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        _depart: MinuteOfDay,
        who: &CitizenView<'_>,
    ) -> TravelEstimate {
        let minutes = minutes_for(self.distance_cm(from, to), speed_pct(who));
        TravelEstimate {
            minutes,
            cost: Money::ZERO,
            mode: TransportMode::Walk,
            reason: DecisionReason::ModeWalkOnly { minutes },
        }
    }

    fn begin_trip(
        &mut self,
        trip: TripRequest,
        who: &CitizenView<'_>,
        q: &mut EventQueue,
    ) -> TripHandle {
        let minutes = minutes_for(self.distance_cm(trip.from, trip.to), speed_pct(who));
        let arrive_at = q.now() + u32::from(minutes);
        q.schedule(SimEvent::new(
            arrive_at,
            trip.traveller.entity().index(),
            EventKind::Arrive,
            trip.slot,
        ));
        let id = self.next_trip.fetch_add(1, Ordering::Relaxed);
        TripHandle {
            id,
            from: trip.from,
            to: trip.to,
            arrive_at,
            minutes,
            mode: TransportMode::Walk,
        }
    }

    fn places_on_route(&self, trip: &TripHandle, out: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>) {
        out.clear();
        let mut trasa = Vec::new();
        self.route_points(trip.from, trip.to, &mut trasa);
        if trasa.len() < 2 {
            return;
        }

        // Zbieramy kandydatów ze wszystkich rodzajów miejsc: „widoczne z trasy" nie
        // zależy od tego, czego mieszkaniec akurat szuka (§5.7 — to jest źródło wiedzy,
        // nie wybór celu).
        let mut znalezione: Vec<(i64, PlaceRef)> = Vec::new();
        for para in trasa.windows(2) {
            let (a, b) = (para[0], para[1]);
            let srodek = WorldCoord::new((a.x + b.x) / 2, (a.y + b.y) / 2, 0);
            let promien = (euclid_cm(a, b) / 2) as f32 / 100.0 + (ROUTE_CORRIDOR_CM / 100) as f32;
            for kind in magnat_core::PlaceKind::ALL {
                self.places.for_each_near(*kind, srodek, promien, |e| {
                    if e.place == trip.from || e.place == trip.to {
                        return;
                    }
                    if let Some(d) = odleglosc_od_odcinka(a, b, e.at) {
                        if d <= ROUTE_CORRIDOR_CM {
                            znalezione.push((d, e.place));
                        }
                    }
                });
            }
        }
        // Remis rozstrzyga `PlaceRef`, nie kolejność komórek indeksu. Deduplikacja po
        // miejscu, bo sąsiednie odcinki łamanej widzą ten sam sklep dwa razy.
        znalezione.sort_unstable();
        let mut widziane: Vec<PlaceRef> = Vec::new();
        for (_, p) in znalezione {
            if widziane.contains(&p) {
                continue;
            }
            widziane.push(p);
            if !out.push(p) {
                break;
            }
        }
    }
}

/// Odległość punktu od odcinka w centymetrach; `None`, gdy rzut pada poza odcinek —
/// miejsce „za plecami" nie jest widoczne z trasy, choć leży blisko jej końca.
fn odleglosc_od_odcinka(a: WorldCoord, b: WorldCoord, p: WorldCoord) -> Option<i64> {
    let (abx, aby) = (i64::from(b.x - a.x), i64::from(b.y - a.y));
    let (apx, apy) = (i64::from(p.x - a.x), i64::from(p.y - a.y));
    let dlugosc_sq = abx * abx + aby * aby;
    if dlugosc_sq == 0 {
        return Some(((apx * apx + apy * apy) as u64).isqrt() as i64);
    }
    let t = apx * abx + apy * aby;
    if t < 0 || t > dlugosc_sq {
        return None;
    }
    // |AP × AB| / |AB| — bez dzielenia przez zero, bo długość sprawdzona wyżej.
    let cross = (apx * aby - apy * abx).abs();
    Some(cross / (dlugosc_sq as u64).isqrt() as i64)
}

// ── warstwa Mikro (§5.10) ───────────────────────────────────────────────────────

/// Jeden pieszy w kadrze — 32 B. Bez unikania kolizji i bez steeringu: przy voxelu 1 m
/// tłum czyta się dobrze bez tego, a steering należy do M11 razem z animacjami.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) struct Pedestrian {
    /// Pozycja w metrach — to jest warstwa prezentacji, więc float jest tu na miejscu.
    pub pos: [f32; 3],
    /// Postęp 0..=1 wzdłuż trasy.
    pub progress: f32,
    /// Indeks trasy w arenie polilinii.
    pub path: u32,
    /// Indeks encji mieszkańca.
    pub citizen: u32,
    pub depart_min: u16,
    pub arrive_min: u16,
    /// 0 = od początku trasy do końca. Zostawione dla M4 (trasy dwukierunkowe).
    pub dir: u8,
    pub _pad: [u8; 3],
}

/// Pozycje pieszych w LOD Mikro. **Wizualizator bez prawa zapisu** do stanu
/// ekonomicznego (00 §4): nie dotyka potrzeb, nie dotyka kolejki zdarzeń, nie zmienia
/// czasu przybycia. M4 zastąpi go jednym buforem dla pieszych, pojazdów i pasażerów
/// (decyzja 9.17).
#[derive(Debug, Default)]
pub(crate) struct PedestrianBuffer {
    peds: Vec<Pedestrian>,
    points: Vec<WorldCoord>,
    /// `(offset w points, liczba punktów, długość trasy w cm)`.
    paths: Vec<(u32, u32, u32)>,
}

impl PedestrianBuffer {
    #[must_use]
    pub(crate) fn new() -> PedestrianBuffer {
        PedestrianBuffer::default()
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.peds.len()
    }

    #[must_use]
    pub(crate) fn get(&self, i: usize) -> Pedestrian {
        self.peds[i]
    }

    pub(crate) fn spawn(
        &mut self,
        citizen: u32,
        route: &[WorldCoord],
        depart_min: u16,
        arrive_min: u16,
    ) -> usize {
        debug_assert!(route.len() >= 2, "trasa bez dwóch końców");
        let offset = self.points.len() as u32;
        self.points.extend_from_slice(route);
        let dlugosc: u32 = route
            .windows(2)
            .map(|w| euclid_cm(w[0], w[1]))
            .fold(0u32, u32::saturating_add);
        self.paths
            .push((offset, route.len() as u32, dlugosc.max(1)));
        let pierwszy = route[0];
        self.peds.push(Pedestrian {
            pos: [
                pierwszy.x as f32 / 100.0,
                pierwszy.y as f32 / 100.0,
                pierwszy.z as f32 / 100.0,
            ],
            progress: 0.0,
            path: (self.paths.len() - 1) as u32,
            citizen,
            depart_min,
            arrive_min,
            dir: 0,
            _pad: [0; 3],
        });
        self.peds.len() - 1
    }

    /// Krok mikro: tick 100 ms gry. `now_ms` to milisekunda doby.
    ///
    /// Postęp wynika **wyłącznie** z pary `(depart, arrive)` policzonej przez mezo —
    /// dlatego pieszy dociera dokładnie w swojej minucie niezależnie od tego, czy krok
    /// mikro w ogóle się wykonał i ile razy. To jest konstrukcja, nie kalibracja (00 §4).
    pub(crate) fn step(&mut self, now_ms: u64) {
        let teraz = now_ms as f32 / 60_000.0;
        for p in &mut self.peds {
            let start = f32::from(p.depart_min);
            let koniec = f32::from(p.arrive_min);
            let t = if koniec <= start {
                1.0
            } else {
                ((teraz - start) / (koniec - start)).clamp(0.0, 1.0)
            };
            p.progress = t;
            let (offset, len, dlugosc) = self.paths[p.path as usize];
            let punkty = &self.points[offset as usize..(offset + len) as usize];
            p.pos = punkt_na_lamanej(punkty, dlugosc, t);
        }
    }

    /// Usuwa pieszych, którzy dotarli. Arena punktów nie jest kompaktowana w locie:
    /// zbiór encji Mikro jest rzędu tysięcy (kadr kamery), więc zwolnienie jej dopiero
    /// wtedy, gdy opustoszeje, kosztuje mniej niż przepisywanie offsetów co minutę.
    pub(crate) fn retire(&mut self, now_min: u16) {
        self.peds.retain(|p| p.arrive_min > now_min);
        if self.peds.is_empty() {
            self.points.clear();
            self.paths.clear();
        }
    }
}

/// Punkt na łamanej w ułamku `t` jej długości.
fn punkt_na_lamanej(punkty: &[WorldCoord], dlugosc_cm: u32, t: f32) -> [f32; 3] {
    let cel = t.clamp(0.0, 1.0) * dlugosc_cm as f32;
    let mut przebyte = 0.0f32;
    for w in punkty.windows(2) {
        let d = euclid_cm(w[0], w[1]) as f32;
        if przebyte + d >= cel {
            let u = if d == 0.0 { 0.0 } else { (cel - przebyte) / d };
            return [
                (w[0].x as f32 + (w[1].x - w[0].x) as f32 * u) / 100.0,
                (w[0].y as f32 + (w[1].y - w[0].y) as f32 * u) / 100.0,
                (w[0].z as f32 + (w[1].z - w[0].z) as f32 * u) / 100.0,
            ];
        }
        przebyte += d;
    }
    let ostatni = punkty[punkty.len() - 1];
    [
        ostatni.x as f32 / 100.0,
        ostatni.y as f32 / 100.0,
        ostatni.z as f32 / 100.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Identity, Needs, Personality, Residence, Vitals};
    use crate::places::{PlaceEntry, PlaceTable};
    use magnat_core::{BuildingId, CitizenId, Entity, PlaceKind};

    fn widok<'a>(
        i: &'a Identity,
        v: &'a Vitals,
        n: &'a Needs,
        p: &'a Personality,
        r: &'a Residence,
    ) -> CitizenView<'a> {
        CitizenView {
            id: CitizenId(Entity::new(1, std::num::NonZeroU32::new(1).unwrap())),
            identity: i,
            vitals: v,
            needs: n,
            personality: p,
            residence: r,
            today: 0,
        }
    }

    fn bud(i: u32) -> PlaceRef {
        PlaceRef::Building(BuildingId(Entity::new(
            i,
            std::num::NonZeroU32::new(1).unwrap(),
        )))
    }

    fn dorosly() -> (Identity, Vitals, Needs, Personality, Residence) {
        (
            Identity {
                birth_day: -360 * 30,
                ..Identity::default()
            },
            Vitals {
                health: 90,
                energy: 90,
                ..Vitals::default()
            },
            Needs::default(),
            Personality::default(),
            Residence::default(),
        )
    }

    #[test]
    fn kilometr_w_linii_prostej_to_kwadrans_marszu() {
        // 1 km manhattanowo × 1,25 = 1250 m przy 81 m/min → 16 minut.
        let m = walk_minutes(
            WorldCoord::new(0, 0, 0),
            WorldCoord::new(100_000, 0, 0),
            100,
        );
        assert_eq!(m, 16);
        // Podróż do sąsiedniego wejścia nigdy nie trwa zero minut.
        assert_eq!(
            walk_minutes(WorldCoord::ORIGIN, WorldCoord::new(10, 0, 0), 100),
            1
        );
    }

    #[test]
    fn dziecko_i_chory_ida_wolniej_niz_dorosly() {
        let dorosly = Identity {
            birth_day: -360 * 30,
            ..Identity::default()
        };
        let dziecko = Identity {
            birth_day: -360 * 8,
            ..Identity::default()
        };
        let zdrowy = Vitals {
            health: 90,
            energy: 90,
            ..Vitals::default()
        };
        let chory = Vitals {
            health: 20,
            energy: 20,
            ..Vitals::default()
        };
        let (n, p, r) = (
            Needs::default(),
            Personality::default(),
            Residence::default(),
        );

        assert_eq!(speed_pct(&widok(&dorosly, &zdrowy, &n, &p, &r)), 100);
        assert_eq!(speed_pct(&widok(&dziecko, &zdrowy, &n, &p, &r)), 85);
        assert_eq!(speed_pct(&widok(&dorosly, &chory, &n, &p, &r)), 75);
    }

    #[test]
    fn podroz_harmonogramuje_przybycie_na_wlasciwa_minute() {
        let table = Arc::new(PlaceTable::build(vec![
            PlaceEntry {
                place: bud(1),
                kind: PlaceKind::Home,
                at: WorldCoord::new(0, 0, 0),
            },
            PlaceEntry {
                place: bud(2),
                kind: PlaceKind::Workplace,
                at: WorldCoord::new(100_000, 0, 0),
            },
            // Sklep w korytarzu trasy — powinien być widoczny z drogi do pracy.
            PlaceEntry {
                place: bud(3),
                kind: PlaceKind::Grocery,
                at: WorldCoord::new(50_000, 3_000, 0),
            },
            // Sklep 300 m w bok — nie jest.
            PlaceEntry {
                place: bud(4),
                kind: PlaceKind::Grocery,
                at: WorldCoord::new(50_000, 30_000, 0),
            },
        ]));
        let mut oracle = WalkOracle::new(table);
        let (i, v, n, p, r) = dorosly();
        let who = widok(&i, &v, &n, &p, &r);

        let mut q = EventQueue::new();
        let handle = oracle.begin_trip(
            TripRequest {
                traveller: who.id,
                from: bud(1),
                to: bud(2),
                depart: MinuteOfDay::new(8 * 60),
                slot: 3,
            },
            &who,
            &mut q,
        );
        assert_eq!(handle.minutes, 16);
        assert_eq!(handle.arrive_at, 16);
        assert_eq!(q.len(), 1);

        let mut out = Vec::new();
        for _ in 0..17 {
            q.drain_minute(&mut out);
        }
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].event_kind(), Some(EventKind::Arrive));
        assert_eq!(out[0].slot, 3);

        let mut widoczne = ArrayVec::new();
        oracle.places_on_route(&handle, &mut widoczne);
        assert_eq!(
            widoczne.as_slice(),
            &[bud(3)],
            "korytarz trasy wpuścił sklep 300 m w bok albo zgubił ten przy drodze"
        );
    }

    /// Kwadrat ulic 200 × 200 m: dom w rogu (0,0), praca w rogu przeciwległym
    /// (200, 200). W linii prostej to 283 m, po ulicach 400 m — i właśnie ta różnica
    /// jest treścią WP6.
    /// Katalog miejsc, pozycje węzłów i odcinki `(a, b, długość w cm)` — minimum,
    /// jakiego `with_streets` potrzebuje od M2 (decyzja 9.4).
    type Scena = (Arc<PlaceTable>, Vec<WorldCoord>, Vec<(u32, u32, u32)>);

    fn kwadrat() -> Scena {
        let w = |x: i32, y: i32| WorldCoord::new(x, y, 0);
        let nodes = vec![w(0, 0), w(20_000, 0), w(20_000, 20_000), w(0, 20_000)];
        // Brak przekątnej: żeby dojść z (0,0) do (200,200) trzeba obejść bokiem.
        let segs = vec![
            (0u32, 1u32, 20_000u32),
            (1, 2, 20_000),
            (2, 3, 20_000),
            (3, 0, 20_000),
        ];
        let places = Arc::new(PlaceTable::build(vec![
            PlaceEntry {
                place: bud(1),
                kind: PlaceKind::Home,
                at: w(0, 0),
            },
            PlaceEntry {
                place: bud(2),
                kind: PlaceKind::Workplace,
                at: w(20_000, 20_000),
            },
            PlaceEntry {
                place: bud(3),
                kind: PlaceKind::Grocery,
                at: w(20_000, 10_000),
            },
        ]));
        (places, nodes, segs)
    }

    #[test]
    fn odleglosc_idzie_po_ulicach_a_nie_na_skroty() {
        let (places, nodes, segs) = kwadrat();
        let oracle = WalkOracle::with_streets(places.clone(), &nodes, &segs);
        assert!(oracle.has_streets());
        let (i, v, n, p, r) = dorosly();
        let who = widok(&i, &v, &n, &p, &r);

        let po_ulicach = oracle.estimate(bud(1), bud(2), MinuteOfDay::MIDNIGHT, &who);
        // 400 m po ulicach przy 81 m/min → 5 minut. Manhattan z korektą dałby 500 m → 7.
        assert_eq!(po_ulicach.minutes, 5);

        let manhattan = WalkOracle::new(places);
        assert_eq!(
            manhattan
                .estimate(bud(1), bud(2), MinuteOfDay::MIDNIGHT, &who)
                .minutes,
            7,
            "fallback z R7 przestał być fallbackiem"
        );

        // Drugie pytanie o tę samą parę idzie z cache'u i musi dać dokładnie to samo.
        assert_eq!(
            oracle
                .estimate(bud(2), bud(1), MinuteOfDay::MIDNIGHT, &who)
                .minutes,
            5,
            "cache jest niesymetryczny albo zwraca co innego niż pierwszy przebieg"
        );
    }

    #[test]
    fn mikro_dociera_dokladnie_w_minucie_wyliczonej_przez_mezo() {
        // Spójność LOD (00 §4) z tolerancją 0 — z konstrukcji, nie z kalibracji:
        // warstwa mikro interpoluje między `depart` a `arrive` i nie ma jak ich zmienić.
        let (places, nodes, segs) = kwadrat();
        let mut oracle = WalkOracle::with_streets(places, &nodes, &segs);
        let (i, v, n, p, r) = dorosly();
        let who = widok(&i, &v, &n, &p, &r);

        // Przewijamy zegar kolejki na ósmą rano. `now` stoi na minucie **obsługiwanej**
        // (korekta D-14), więc `begin_trip` w tej minucie planuje przybycie dokładnie
        // na `8:00 + czas dojścia` — bez tego mezo i planer rozjechałyby się o minutę.
        let mut q = EventQueue::new();
        let mut out = Vec::new();
        while q.now() < 8 * 60 {
            q.drain_minute(&mut out);
        }
        let odjazd = MinuteOfDay::new(8 * 60);
        let handle = oracle.begin_trip(
            TripRequest {
                traveller: who.id,
                from: bud(1),
                to: bud(2),
                depart: odjazd,
                slot: 0,
            },
            &who,
            &mut q,
        );
        assert_eq!(handle.arrive_at, 8 * 60 + u32::from(handle.minutes));

        oracle.enter_micro(&handle, who.id.entity().index(), odjazd);
        assert_eq!(oracle.micro_len(), 1);
        let cel = [200.0f32, 200.0, 0.0];
        let mut zrzut = Vec::new();

        // Sześćset kroków po 100 ms = minuta gry; przebiegamy całą podróż.
        let krokow = u64::from(handle.minutes) * 600;
        let mut w_celu_przed_czasem = false;
        for krok in 0..=krokow {
            oracle.micro_step(u64::from(odjazd.get()) * 60_000 + krok * 100);
            oracle.micro_snapshot(&mut zrzut);
            if krok < krokow && zrzut[0].2 >= 1.0 {
                w_celu_przed_czasem = true;
            }
        }
        assert!(!w_celu_przed_czasem, "mikro dotarło przed czasem mezo");
        let (_, pos, postep) = zrzut[0];
        assert!((postep - 1.0).abs() < 1e-6);
        assert!(
            (pos[0] - cel[0]).abs() < 0.5 && (pos[1] - cel[1]).abs() < 0.5,
            "pieszy skończył w {pos:?}, a nie w celu {cel:?}"
        );

        // Krok mikro po czasie przybycia niczego nie psuje i nie cofa.
        oracle.micro_step(u64::from(handle.arrive_at) * 60_000 + 5_000);
        oracle.micro_snapshot(&mut zrzut);
        assert!((zrzut[0].2 - 1.0).abs() < 1e-6);
        oracle.micro_retire(handle.arrive_at as u16);
        assert_eq!(
            oracle.micro_len(),
            0,
            "pieszy po przybyciu został w buforze"
        );
    }

    #[test]
    fn trasa_przez_miasto_ma_ksztalt_ulic() {
        // Miejsca widoczne z trasy idą po łamanej, a nie po cięciwie: sklep przy
        // rogu (200, 100) leży 100 m od cięciwy dom→praca, więc korytarz 60 m
        // wpuściłby go tylko wtedy, gdy trasa faktycznie biegnie bokiem kwadratu.
        let (places, nodes, segs) = kwadrat();
        let mut oracle = WalkOracle::with_streets(places, &nodes, &segs);
        let (i, v, n, p, r) = dorosly();
        let who = widok(&i, &v, &n, &p, &r);
        let mut q = EventQueue::new();
        let handle = oracle.begin_trip(
            TripRequest {
                traveller: who.id,
                from: bud(1),
                to: bud(2),
                depart: MinuteOfDay::MIDNIGHT,
                slot: 0,
            },
            &who,
            &mut q,
        );
        let mut widoczne = ArrayVec::new();
        oracle.places_on_route(&handle, &mut widoczne);
        assert_eq!(
            widoczne.as_slice(),
            &[bud(3)],
            "sklep przy trasie nie został zauważony z łamanej"
        );
    }

    #[test]
    fn sieciowka_znosi_niespojne_odcinki_zamiast_panikowac() {
        // Dane z generatora mogą mieć pętlę własną albo indeks poza tablicą; sieć
        // ma wtedy działać dalej, a nie wywracać symulacji.
        let w = |x: i32| WorldCoord::new(x, 0, 0);
        let nodes = vec![w(0), w(10_000)];
        let segs = vec![(0u32, 0u32, 5u32), (0, 1, 10_000), (0, 99, 7)];
        let places = Arc::new(PlaceTable::build(vec![
            PlaceEntry {
                place: bud(1),
                kind: PlaceKind::Home,
                at: w(0),
            },
            PlaceEntry {
                place: bud(2),
                kind: PlaceKind::Workplace,
                at: w(10_000),
            },
        ]));
        let oracle = WalkOracle::with_streets(places, &nodes, &segs);
        let (i, v, n, p, r) = dorosly();
        let who = widok(&i, &v, &n, &p, &r);
        // 100 m przy 81 m/min → 2 minuty (zaokrąglenie w górę).
        assert_eq!(
            oracle
                .estimate(bud(1), bud(2), MinuteOfDay::MIDNIGHT, &who)
                .minutes,
            2
        );
    }

    #[test]
    fn rozmiar_pieszego_zgadza_sie_z_budzetem() {
        assert_eq!(size_of::<Pedestrian>(), 32);
    }
}
