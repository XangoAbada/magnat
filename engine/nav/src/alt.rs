//! A\* z heurystyką landmarkową (ALT) — warstwa piesza i rowerowa (M4a §5.1, WP2).
//!
//! **Dlaczego nie CCH.** Trasy pieszo i rowerem są krótkie: kilkaset metrów, rzadko
//! kilometry. Zapytanie i tak przeszukuje ułamek miasta, więc kontrakcja kosztuje
//! więcej, niż oszczędza — a warstwy `Foot`/`Bike` przeliczają wagi częściej niż
//! jezdnia (pogoda, pora dnia), czyli płaciłyby za kustomizację przy każdej zmianie.
//! Landmarki są tanie: to zwykłe drzewa Dijkstry, `k` par na topologię.
//!
//! **Dlaczego wynik jest optymalny.** Heurystyka landmarkowa wynika z nierówności
//! trójkąta i jest **dopuszczalna** — nigdy nie przeszacowuje rzeczywistego kosztu.
//! A\* z heurystyką dopuszczalną zwraca dokładnie tę trasę, którą zwróciłaby
//! Dijkstra; różni się tylko liczbą zdjętych z kopca węzłów. Oba te zdania mają
//! swój test niżej i nie są tu na wiarę.
//!
//! Waga `u32::MAX` znaczy „krawędź wyłączona z tego profilu" (zamknięty most,
//! zakaz wjazdu) — taka krawędź **nigdy** nie jest relaksowana, w Dijkstrze
//! landmarkowej tak samo jak w zapytaniu.

use crate::graph::{EdgeId, NodeId, RoadGraph};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Zestaw punktów orientacyjnych i odległości do/od nich (ALT).
///
/// Odległości trzymane są płasko, `k × n` na kierunek: układ pozwala czytać oba
/// wiersze landmarka jednym `chunks_exact`, bez tablicy tablic i bez pościgu za
/// wskaźnikami w pętli wewnętrznej heurystyki.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Landmarks {
    /// Liczba węzłów grafu, dla którego policzono odległości.
    n: usize,
    /// Wybrane punkty orientacyjne, w kolejności wyboru.
    nodes: Vec<NodeId>,
    /// `to_lm[l * n + v]` = koszt `v → L_l`. `u32::MAX` = nieosiągalny.
    to_lm: Vec<u32>,
    /// `from_lm[l * n + v]` = koszt `L_l → v`. `u32::MAX` = nieosiągalny.
    from_lm: Vec<u32>,
}

impl Landmarks {
    /// Wybiera `k` punktów metodą najdalszego punktu (farthest-point) i liczy
    /// pełne drzewa odległości w obie strony.
    ///
    /// Deterministyczne: pierwszym punktem jest węzeł o **najmniejszym indeksie**
    /// mający jakąkolwiek krawędź wychodzącą, każdy następny maksymalizuje
    /// odległość od punktów już wybranych, a remisy rozstrzyga niższy indeks
    /// węzła. Kandydaci nieosiągalni z dotychczasowych punktów są pomijani —
    /// inaczej `u32::MAX` wygrywałby każde maksimum i landmarki wylądowałyby
    /// w wysepkach, z których nic nie widać.
    ///
    /// Zwraca mniej niż `k` punktów, gdy graf ich nie ma (mały lub rozdrobniony).
    #[must_use]
    pub fn build(g: &RoadGraph, weights: &[u32], k: usize) -> Landmarks {
        let n = g.node_count();
        let mut lm = Landmarks {
            n,
            nodes: Vec::new(),
            to_lm: Vec::new(),
            from_lm: Vec::new(),
        };
        if n == 0 || k == 0 {
            return lm;
        }
        let Some(first) = (0..n).find(|&v| !g.out(NodeId(v as u32)).is_empty()) else {
            return lm;
        };

        // `cover[v]` = odległość do najbliższego dotąd wybranego punktu.
        let mut cover = vec![u32::MAX; n];
        let mut next = Some(NodeId(first as u32));
        while let Some(l) = next {
            let from = dijkstra(g, weights, l, false);
            let to = dijkstra(g, weights, l, true);
            for (c, &d) in cover.iter_mut().zip(from.iter()) {
                *c = (*c).min(d);
            }
            lm.nodes.push(l);
            lm.from_lm.extend_from_slice(&from);
            lm.to_lm.extend_from_slice(&to);
            if lm.nodes.len() >= k {
                break;
            }
            next = farthest(&cover);
        }
        lm
    }

    /// Liczba faktycznie wybranych punktów — bywa mniejsza niż `k` z `build`.
    #[must_use]
    pub fn count(&self) -> usize {
        self.nodes.len()
    }

    /// Zajętość pamięci w bajtach: dwie tablice `k × n × 4 B` plus drobiazgi.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Landmarks>()
            + self.nodes.len() * std::mem::size_of::<NodeId>()
            + (self.to_lm.len() + self.from_lm.len()) * std::mem::size_of::<u32>()
    }

    /// Dolne oszacowanie kosztu `v → t`: `max_L max(d(L,t) − d(L,v), d(v,L) − d(t,L))`.
    ///
    /// Oba składniki wynikają z nierówności trójkąta (`d(L,t) ≤ d(L,v) + d(v,t)`
    /// i `d(v,L) ≤ d(v,t) + d(t,L)`), więc **żaden nie przeszacowuje** i A\* pozostaje
    /// optymalny. Składnik oparty na nieosiągalnej odległości jest pomijany osobno —
    /// jeden kierunek może być użyteczny, gdy drugi nie istnieje.
    ///
    /// Zwraca `0`, gdy żaden punkt nie daje się użyć albo gdy indeks jest spoza
    /// grafu; zero jest zawsze dopuszczalne, więc to bezpieczny wynik, a nie błąd.
    #[must_use]
    pub fn lower_bound(&self, v: NodeId, t: NodeId) -> u32 {
        let (vi, ti) = (v.0 as usize, t.0 as usize);
        if self.n == 0 || vi >= self.n || ti >= self.n {
            return 0;
        }
        let mut best = 0u32;
        for (fl, tl) in self
            .from_lm
            .chunks_exact(self.n)
            .zip(self.to_lm.chunks_exact(self.n))
        {
            let (fv, ft) = (fl[vi], fl[ti]);
            if fv != u32::MAX && ft != u32::MAX {
                best = best.max(ft.saturating_sub(fv));
            }
            let (tv, tt) = (tl[vi], tl[ti]);
            if tv != u32::MAX && tt != u32::MAX {
                best = best.max(tv.saturating_sub(tt));
            }
        }
        best
    }
}

/// Węzeł najdalszy od dotąd wybranych punktów. Remis → niższy indeks (`>`, nie `>=`).
/// Zero znaczy „to jest wybrany punkt albo leży w nim zerowym kosztem" — bezużyteczny
/// kandydat; `u32::MAX` znaczy „nieosiągalny" i też odpada.
fn farthest(cover: &[u32]) -> Option<NodeId> {
    let mut best: Option<(u32, u32)> = None;
    for (v, &d) in cover.iter().enumerate() {
        if d == 0 || d == u32::MAX {
            continue;
        }
        if best.is_none_or(|(bd, _)| d > bd) {
            best = Some((d, v as u32));
        }
    }
    best.map(|(_, v)| NodeId(v))
}

/// Pełne drzewo odległości od `src`. `backward = true` idzie po krawędziach
/// wchodzących, czyli liczy `v → src` zamiast `src → v`.
fn dijkstra(g: &RoadGraph, weights: &[u32], src: NodeId, backward: bool) -> Vec<u32> {
    let n = g.node_count();
    let mut dist = vec![u32::MAX; n];
    if (src.0 as usize) >= n {
        return dist;
    }
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    dist[src.0 as usize] = 0;
    heap.push(Reverse((0, src.0)));
    while let Some(Reverse((d, v))) = heap.pop() {
        if d > dist[v as usize] {
            continue;
        }
        let adj = if backward {
            g.inc(NodeId(v))
        } else {
            g.out(NodeId(v))
        };
        for &e in adj {
            let w = weights.get(e as usize).copied().unwrap_or(u32::MAX);
            if w == u32::MAX {
                continue;
            }
            let edge = g.edge(EdgeId(e));
            let u = if backward { edge.from } else { edge.to };
            let nd = d.saturating_add(w);
            if nd < dist[u.0 as usize] {
                dist[u.0 as usize] = nd;
                heap.push(Reverse((nd, u.0)));
            }
        }
    }
    dist
}

/// Bufory wielokrotnego użytku jednego wątku wyszukującego.
///
/// Tablice `dist`/`parent` mają rozmiar grafu i **nie są czyszczone** między
/// zapytaniami: znacznik generacji mówi, czy wpis pochodzi z bieżącego przebiegu.
/// Bez tego każde zapytanie na trasę długą na 30 węzłów płaciłoby `O(n)` za samo
/// wyzerowanie miasta.
#[derive(Default)]
pub struct AltScratch {
    dist: Vec<u32>,
    /// Krawędź, którą wszedł najlepszy znany poprzednik; `u32::MAX` = brak.
    parent: Vec<u32>,
    stamp: Vec<u32>,
    settled: Vec<u32>,
    heap: BinaryHeap<Reverse<(u32, u32)>>,
    generation: u32,
    visited: u32,
}

impl AltScratch {
    #[must_use]
    pub fn new() -> AltScratch {
        AltScratch::default()
    }

    /// Ile węzłów zdjęto z kopca w ostatnim zapytaniu. Miara skuteczności
    /// heurystyki: to nią mierzy się, czy landmarki w ogóle zarabiają na siebie.
    #[must_use]
    pub fn visited(&self) -> u32 {
        self.visited
    }

    /// Przygotowuje bufory pod graf o `n` węzłach i otwiera nową generację.
    fn begin(&mut self, n: usize) {
        if self.dist.len() != n {
            self.dist = vec![0; n];
            self.parent = vec![u32::MAX; n];
            self.stamp = vec![0; n];
            self.settled = vec![0; n];
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // Przepełnienie licznika: raz na 4 mld zapytań płacimy pełne zerowanie.
            self.stamp.fill(0);
            self.settled.fill(0);
            self.generation = 1;
        }
        self.heap.clear();
        self.visited = 0;
    }
}

/// A\* z heurystyką landmarkową. Zwraca koszt trasy i ciąg krawędzi `s → t`.
///
/// `lm == None` daje heurystykę zerową, czyli zwykłą Dijkstrę — to nie jest tryb
/// awaryjny, tylko odniesienie, względem którego testy sprawdzają optymalność.
///
/// Zwraca `None`, gdy trasy nie ma albo gdy któryś z węzłów jest spoza grafu.
/// `s == t` to `Some((0, []))` — trasa zerowej długości istnieje.
pub fn route(
    g: &RoadGraph,
    weights: &[u32],
    lm: Option<&Landmarks>,
    s: NodeId,
    t: NodeId,
    scratch: &mut AltScratch,
) -> Option<(u32, Vec<EdgeId>)> {
    let n = g.node_count();
    if s.0 as usize >= n || t.0 as usize >= n {
        return None;
    }
    scratch.begin(n);
    if s == t {
        return Some((0, Vec::new()));
    }

    let gen = scratch.generation;
    let heuristic = |v: NodeId| lm.map_or(0, |l| l.lower_bound(v, t));

    let si = s.0 as usize;
    scratch.dist[si] = 0;
    scratch.stamp[si] = gen;
    scratch.parent[si] = u32::MAX;
    scratch.heap.push(Reverse((heuristic(s), s.0)));

    let mut cost = None;
    while let Some(Reverse((_, v))) = scratch.heap.pop() {
        let vi = v as usize;
        if scratch.settled[vi] == gen {
            continue;
        }
        scratch.settled[vi] = gen;
        scratch.visited += 1;
        let gv = scratch.dist[vi];
        if v == t.0 {
            cost = Some(gv);
            break;
        }
        for &e in g.out(NodeId(v)) {
            let w = weights.get(e as usize).copied().unwrap_or(u32::MAX);
            if w == u32::MAX {
                continue;
            }
            let u = g.edge(EdgeId(e)).to;
            let ui = u.0 as usize;
            if scratch.settled[ui] == gen {
                continue;
            }
            let nd = gv.saturating_add(w);
            if scratch.stamp[ui] != gen || nd < scratch.dist[ui] {
                scratch.dist[ui] = nd;
                scratch.stamp[ui] = gen;
                scratch.parent[ui] = e;
                scratch
                    .heap
                    .push(Reverse((nd.saturating_add(heuristic(u)), u.0)));
            }
        }
    }

    let cost = cost?;
    let mut path = Vec::new();
    let mut cur = t.0;
    while cur != s.0 {
        let e = scratch.parent[cur as usize];
        // Łańcuch poprzedników Dijkstry jest drzewem, więc warunek jest asekuracją
        // przed danymi, nie przed algorytmem — ale pętla nieskończona w routingu
        // zawiesiłaby całą symulację, a nie tylko zapytanie.
        if e == u32::MAX || path.len() > n {
            return None;
        }
        path.push(EdgeId(e));
        cur = g.edge(EdgeId(e)).from.0;
    }
    path.reverse();
    Some((cost, path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{synthetic_grid, EdgeSpec, GeomRef, Modality, RoadGraphBuilder};
    use magnat_core::{DistrictId, IVec2, Mass, RoadClass};

    /// Deterministyczny generator par (LCG) — testy nie mają prawa zależeć
    /// od kolejności hashowania ani od `rand` (00 §3.1).
    fn pary(n: u32, ile: usize, ziarno: u64) -> Vec<(NodeId, NodeId)> {
        let mut x = ziarno;
        let krok = |s: &mut u64| {
            *s = s
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((*s >> 33) as u32) % n
        };
        (0..ile)
            .map(|_| {
                let a = krok(&mut x);
                let b = krok(&mut x);
                (NodeId(a), NodeId(b))
            })
            .collect()
    }

    fn lancuch(dlugosci: &[u32]) -> RoadGraph {
        let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
        for i in 0..=dlugosci.len() {
            b.add_node(IVec2::new(i as i32 * 10_000, 0), 0);
        }
        for (i, &d) in dlugosci.iter().enumerate() {
            b.add_edge(EdgeSpec {
                from: NodeId(i as u32),
                to: NodeId(i as u32 + 1),
                geometry_ref: GeomRef(0),
                length_cm: d,
                lanes: 1,
                class: RoadClass::Local,
                speed_limit_dkmh: 300,
                max_mass: Mass::ZERO,
                grade_permille: 0,
                bridge: None,
                curb_parking: 0,
                district: DistrictId(0),
            });
        }
        b.finish()
    }

    #[test]
    fn heurystyka_nigdy_nie_przeszacowuje() {
        let g = synthetic_grid(30, 30, 10_000);
        let w = g.free_flow_weights();
        let lm = Landmarks::build(&g, &w, 6);
        assert_eq!(lm.count(), 6);

        // 25 źródeł × 20 celów = 500 par przy 25 pełnych Dijkstrach zamiast 500.
        let mut sprawdzone = 0;
        for zrodlo in (0..900u32).step_by(36) {
            let prawda = dijkstra(&g, &w, NodeId(zrodlo), false);
            for cel in (0..900u32).step_by(45) {
                let d = prawda[cel as usize];
                if d == u32::MAX {
                    continue;
                }
                let dolne = lm.lower_bound(NodeId(zrodlo), NodeId(cel));
                assert!(
                    dolne <= d,
                    "heurystyka {dolne} > prawdziwy koszt {d} dla {zrodlo}→{cel}"
                );
                sprawdzone += 1;
            }
        }
        assert_eq!(sprawdzone, 500);
    }

    #[test]
    fn alt_daje_ten_sam_koszt_co_dijkstra() {
        let g = synthetic_grid(30, 30, 10_000);
        let w = g.free_flow_weights();
        let lm = Landmarks::build(&g, &w, 6);
        let mut sc = AltScratch::new();

        let mut sprawdzone = 0;
        for zrodlo in (0..900u32).step_by(36) {
            let prawda = dijkstra(&g, &w, NodeId(zrodlo), false);
            for cel in (0..900u32).step_by(45) {
                let oczekiwany = prawda[cel as usize];
                let got = route(&g, &w, Some(&lm), NodeId(zrodlo), NodeId(cel), &mut sc);
                match got {
                    Some((koszt, sciezka)) => {
                        assert_eq!(koszt, oczekiwany, "para {zrodlo}→{cel}");
                        // Ścieżka ma się sumować do zwróconego kosztu.
                        let suma: u32 = sciezka.iter().map(|&e| w[e.0 as usize]).sum();
                        assert_eq!(suma, koszt, "suma krawędzi ≠ koszt dla {zrodlo}→{cel}");
                    }
                    None => assert_eq!(oczekiwany, u32::MAX, "para {zrodlo}→{cel}"),
                }
                sprawdzone += 1;
            }
        }
        assert_eq!(sprawdzone, 500);
    }

    #[test]
    fn alt_odwiedza_mniej_wezlow_niz_dijkstra() {
        let g = synthetic_grid(30, 30, 10_000);
        let w = g.free_flow_weights();
        let lm = Landmarks::build(&g, &w, 6);
        let mut sc = AltScratch::new();

        let (mut z_lm, mut bez_lm) = (0u64, 0u64);
        for (s, t) in pary(900, 200, 0x2545_F491_4F6C_DD1D) {
            if route(&g, &w, Some(&lm), s, t, &mut sc).is_some() {
                z_lm += u64::from(sc.visited());
            }
            if route(&g, &w, None, s, t, &mut sc).is_some() {
                bez_lm += u64::from(sc.visited());
            }
        }
        assert!(
            z_lm < bez_lm,
            "ALT odwiedził {z_lm} węzłów, Dijkstra {bez_lm} — heurystyka nic nie daje"
        );
    }

    #[test]
    fn graf_rozlaczny_nie_ma_trasy() {
        let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
        for i in 0..4 {
            b.add_node(IVec2::new(i * 10_000, 0), 0);
        }
        let dodaj = |from: u32, to: u32| EdgeSpec {
            from: NodeId(from),
            to: NodeId(to),
            geometry_ref: GeomRef(0),
            length_cm: 10_000,
            lanes: 1,
            class: RoadClass::Local,
            speed_limit_dkmh: 300,
            max_mass: Mass::ZERO,
            grade_permille: 0,
            bridge: None,
            curb_parking: 0,
            district: DistrictId(0),
        };
        b.add_edge_pair(dodaj(0, 1));
        b.add_edge_pair(dodaj(2, 3));
        let g = b.finish();
        let w = g.free_flow_weights();
        let lm = Landmarks::build(&g, &w, 4);
        let mut sc = AltScratch::new();

        assert!(route(&g, &w, Some(&lm), NodeId(0), NodeId(2), &mut sc).is_none());
        assert!(route(&g, &w, None, NodeId(0), NodeId(2), &mut sc).is_none());
        assert!(route(&g, &w, Some(&lm), NodeId(0), NodeId(1), &mut sc).is_some());
        // Nieosiągalny cel: heurystyka musi milczeć, a nie kłamać.
        assert_eq!(lm.lower_bound(NodeId(0), NodeId(2)), 0);
    }

    #[test]
    fn krawedz_wylaczona_z_profilu_nie_jest_relaksowana() {
        let g = lancuch(&[10_000, 10_000, 10_000]);
        let mut w = g.free_flow_weights();
        let mut sc = AltScratch::new();
        assert!(route(&g, &w, None, NodeId(0), NodeId(3), &mut sc).is_some());
        w[1] = u32::MAX; // środkowy odcinek zamknięty
        assert!(route(&g, &w, None, NodeId(0), NodeId(3), &mut sc).is_none());
        assert!(route(&g, &w, None, NodeId(0), NodeId(1), &mut sc).is_some());
    }

    #[test]
    fn trasa_do_samego_siebie_jest_pusta() {
        let g = synthetic_grid(5, 5, 10_000);
        let w = g.free_flow_weights();
        let mut sc = AltScratch::new();
        assert_eq!(
            route(&g, &w, None, NodeId(7), NodeId(7), &mut sc),
            Some((0, Vec::new()))
        );
        assert!(route(&g, &w, None, NodeId(999), NodeId(0), &mut sc).is_none());
    }

    #[test]
    fn landmarki_sa_deterministyczne_i_mierzalne() {
        let g = synthetic_grid(20, 20, 10_000);
        let w = g.free_flow_weights();
        let a = Landmarks::build(&g, &w, 4);
        let b = Landmarks::build(&g, &w, 4);
        assert_eq!(a, b);
        assert_eq!(a.count(), 4);
        // 4 punkty × 400 węzłów × 2 kierunki × 4 B = 12 800 B danych.
        assert!(a.memory_bytes() >= 4 * 400 * 2 * 4);
    }
}
