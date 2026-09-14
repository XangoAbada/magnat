//! WP6 — kwartały z grafu dróg (M2 §5.4, część geometryczna).
//!
//! `RoadNetwork` jest grafem planarnym: każde przecięcie dostało węzeł w ograniczeniu
//! lokalnym 4, a mosty i tunele są z planaryzacji wyłączone flagą `GRADE_SEPARATED`.
//! Ściany wyznaczamy obchodem półkrawędzi — w każdym węźle krawędzie posortowane po
//! kącie, następna półkrawędź to „najbardziej w prawo". O(E).
//!
//! Kąty sortujemy **pseudokątem**, a nie `atan2`: `atan2` jest zakazane w kodzie
//! symulacji (00 §K-6, brak w `det_math`), a do samego uporządkowania wystarczy dowolna
//! funkcja monotoniczna z kątem. Ta jest dokładna, bo używa wyłącznie dodawania,
//! odejmowania i dzielenia.

use super::road::{PolyArena, PolyRef, RoadFlags, RoadNetwork, SegmentId, UNASSIGNED_DISTRICT};
use super::zoning::ZoneKind;
use magnat_core::DistrictId;
use magnat_spatial::Vec2;
use std::ops::Range;

/// Kwartał. Geometria powstaje w M2b (WP6), reszta pól w M2c: strefa i pierścień epoki
/// w WP7, zakres parcel w WP8, dzielnica i osiedle w WP9.
#[derive(Clone, PartialEq, Debug)]
pub struct Block {
    pub id: BlockId,
    /// Obrys użytkowy: ściana grafu odsunięta do wewnątrz o połowę pasa drogowego.
    pub poly: PolyRef,
    /// Ściana grafu po osiach dróg — potrzebna M2c do przypisywania frontów.
    pub face: PolyRef,
    pub area_m2: u32,
    /// CSR → `SegmentId` dróg otaczających kwartał.
    pub bounding: Range<u32>,
    pub district: DistrictId,
    /// „Osiedle" — tożsamość drobniejsza niż dzielnica (M2 §9.1/6).
    pub neighborhood: u16,
    pub zone: ZoneKind,
    pub epoch_ring: u8,
    /// Ciągły zakres w globalnej tablicy parcel.
    pub parcels: Range<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BlockId(pub u32);

#[derive(Clone, PartialEq, Debug)]
pub struct BlockSet {
    pub blocks: Vec<Block>,
    pub bounding_items: Vec<SegmentId>,
    /// Pole obszaru zurbanizowanego liczone jako suma ścian wewnętrznych, w m².
    pub urban_area_m2: f64,
    /// Pole pasa drogowego, w m².
    pub road_area_m2: f64,
    /// Ściany odrzucone jako zbyt małe (< `MIN_BLOCK_M2`).
    pub dropped: u32,
    /// Ściany, dla których odsunięcie pochłonęło cały kwartał.
    pub swallowed: u32,
    /// Wszystkie orbity obchodu (ściany wewnętrzne + zewnętrzne + zdegenerowane).
    pub orbits: u32,
    /// Orbity o niedodatnim polu — ściany zewnętrzne składowych.
    pub outer: u32,
    /// Orbity krótsze niż trójkąt.
    pub degenerate: u32,
    /// Krawędzie wyłączone z planaryzacji (wiadukty).
    pub excluded_edges: u32,
    /// Suma długości orbit — musi równać się 2·E podgrafu ścian.
    pub visited_half_edges: u32,
    /// Najdłuższa orbita.
    pub max_orbit: u32,
    /// `(V, E, C)` podgrafu ścian **w chwili budowy kwartałów**. Zapamiętane, bo M2c
    /// dokłada do sieci ulice lokalne i tory, a niezmiennik Eulera dotyczy tego grafu,
    /// z którego kwartały powstały — policzony później mierzyłby co innego.
    pub euler: (u32, u32, u32),
}

/// Ściana mniejsza niż to jest wysepką w skrzyżowaniu, nie kwartałem.
const MIN_BLOCK_M2: f64 = 300.0;
/// Poniżej tego pola orbita nie jest kwartałem, tylko obiegiem ściany zewnętrznej.
const ZERO_AREA_M2: f64 = 1.0;

/// Pseudokąt: funkcja ściśle rosnąca z kątem, o wartościach 0..4, bez trygonometrii.
fn pseudo_angle(d: Vec2) -> f32 {
    let s = d.x.abs() + d.y.abs();
    if s == 0.0 {
        return 0.0;
    }
    let p = d.x / s;
    if d.y < 0.0 {
        3.0 + p
    } else {
        1.0 - p
    }
}

/// Pole ze znakiem — jedna implementacja dla całego generatora (`city::poly`).
fn shoelace(pts: &[Vec2]) -> f64 {
    super::poly::signed_area(pts)
}

/// WP6: ściany planarne grafu dróg jezdnych.
#[must_use]
pub fn build_blocks(net: &RoadNetwork, geom: &mut PolyArena) -> BlockSet {
    // Półkrawędzie: 2·i = a→b, 2·i+1 = b→a. Tory, ciągi piesze i przeprawy
    // bezkolizyjne nie tworzą ścian — pierwsze dwa nie ograniczają kwartału,
    // trzecie przechodzą nad nim albo pod nim.
    let usable = |s: &super::road::RoadSegment| {
        s.class.is_driveable() && !s.flags.contains(RoadFlags::GRADE_SEPARATED)
    };

    let n_half = net.segments.len() * 2;
    let mut origin = vec![u32::MAX; n_half];
    let mut dest = vec![u32::MAX; n_half];
    for (i, s) in net.segments.iter().enumerate() {
        if !usable(s) {
            continue;
        }
        origin[2 * i] = s.a.0;
        dest[2 * i] = s.b.0;
        origin[2 * i + 1] = s.b.0;
        dest[2 * i + 1] = s.a.0;
    }

    // Wyjścia z węzła, posortowane po pseudokącie. Remis rozstrzyga indeks półkrawędzi,
    // żeby dwie krawędzie o identycznym kierunku nie zamieniały się miejscami.
    let mut out_of: Vec<Vec<u32>> = vec![Vec::new(); net.nodes.len()];
    for h in 0..n_half {
        if origin[h] == u32::MAX {
            continue;
        }
        out_of[origin[h] as usize].push(h as u32);
    }
    for (v, list) in out_of.iter_mut().enumerate() {
        let p = net.nodes[v].pos;
        list.sort_by(|&a, &b| {
            let da = net.nodes[dest[a as usize] as usize].pos - p;
            let db = net.nodes[dest[b as usize] as usize].pos - p;
            pseudo_angle(da)
                .total_cmp(&pseudo_angle(db))
                .then(a.cmp(&b))
        });
    }
    // Pozycja półkrawędzi w liście swojego źródła — żeby `next` był O(1).
    let mut slot = vec![u32::MAX; n_half];
    for list in &out_of {
        for (i, &h) in list.iter().enumerate() {
            slot[h as usize] = i as u32;
        }
    }

    // next(h): bliźniak h, cofnięty o jeden w porządku przeciwnym do ruchu wskazówek.
    let next = |h: usize| -> usize {
        let twin = h ^ 1;
        let v = origin[twin] as usize;
        let list = &out_of[v];
        let k = slot[twin] as usize;
        list[(k + list.len() - 1) % list.len()] as usize
    };

    let mut visited = vec![false; n_half];
    let mut blocks = Vec::new();
    let mut bounding_items: Vec<SegmentId> = Vec::new();
    let mut urban = 0.0f64;
    let mut road = 0.0f64;
    let mut dropped = 0;
    let mut swallowed = 0;
    let mut orbits = 0u32;
    let mut outer = 0u32;
    let mut degenerate = 0u32;
    let mut visited_half_edges = 0u32;
    let mut max_orbit = 0u32;

    for start in 0..n_half {
        if origin[start] == u32::MAX || visited[start] {
            continue;
        }
        let mut cycle = Vec::new();
        let mut h = start;
        loop {
            visited[h] = true;
            cycle.push(h);
            h = next(h);
            if h == start {
                break;
            }
            // Zabezpieczenie przed cyklem, który nie wraca do startu — gdyby graf
            // przestał być planarny, ta pętla nie ma prawa kręcić się w nieskończoność.
            if cycle.len() > n_half {
                break;
            }
        }
        let pts: Vec<Vec2> = cycle
            .iter()
            .map(|&e| net.nodes[origin[e] as usize].pos)
            .collect();
        orbits += 1;
        visited_half_edges += cycle.len() as u32;
        max_orbit = max_orbit.max(cycle.len() as u32);
        if pts.len() < 3 {
            degenerate += 1;
            continue;
        }
        let area = shoelace(&pts);
        // Ściana zewnętrzna ma przy tej orientacji obchodu pole ujemne, a orbita obiegająca
        // składową drzewiastą — zerowe. Próg zamiast czystego zera, bo „zero" liczone
        // wzorem Gaussa na współrzędnych metrowych wychodzi rzędu 10⁻³ i raz wypada
        // dodatnie: wtedy ściana zewnętrzna udawałaby kwartał i psuła bilans Eulera.
        if area <= ZERO_AREA_M2 {
            outer += 1;
            continue;
        }
        urban += area;
        if area < MIN_BLOCK_M2 {
            dropped += 1;
            road += area;
            continue;
        }

        let rows: Vec<f32> = cycle
            .iter()
            .map(|&e| f32::from(net.segments[e / 2].row_m))
            .collect();
        let Some(inner) = inset(&pts, &rows) else {
            swallowed += 1;
            road += area;
            continue;
        };
        let inner_area = shoelace(&inner);
        if inner_area <= MIN_BLOCK_M2 {
            swallowed += 1;
            road += area;
            continue;
        }
        road += area - inner_area;

        let b0 = bounding_items.len() as u32;
        for &e in &cycle {
            bounding_items.push(SegmentId((e / 2) as u32));
        }
        blocks.push(Block {
            id: BlockId(blocks.len() as u32),
            poly: geom.push(&inner),
            face: geom.push(&pts),
            area_m2: inner_area as u32,
            bounding: b0..bounding_items.len() as u32,
            district: UNASSIGNED_DISTRICT,
            neighborhood: 0,
            zone: ZoneKind::Undevelopable,
            epoch_ring: 0,
            parcels: 0..0,
        });
    }

    BlockSet {
        blocks,
        bounding_items,
        urban_area_m2: urban,
        road_area_m2: road,
        dropped,
        swallowed,
        orbits,
        outer,
        degenerate,
        visited_half_edges,
        max_orbit,
        excluded_edges: net
            .segments
            .iter()
            .filter(|s| s.class.is_driveable() && s.flags.contains(RoadFlags::GRADE_SEPARATED))
            .count() as u32,
        euler: {
            let (v, e, c) = face_graph_stats(net);
            (v as u32, e as u32, c as u32)
        },
    }
}

/// Sąsiedztwo kwartałów w postaci CSR: dwa kwartały są sąsiadami, gdy dzielą segment
/// drogi. Wejście dla pierścieni epok (WP7), wygładzania stref i Voronoi dzielnic (WP9) —
/// wszystkie trzy chcą tego samego grafu, więc liczymy go raz.
#[must_use]
pub fn block_adjacency(set: &BlockSet, segments: usize) -> (Vec<u32>, Vec<u32>) {
    // Segment ogranicza co najwyżej dwa kwartały (jest krawędzią grafu planarnego).
    let mut po_segmencie = vec![[u32::MAX; 2]; segments];
    for b in &set.blocks {
        for &s in &set.bounding_items[b.bounding.start as usize..b.bounding.end as usize] {
            let slot = &mut po_segmencie[s.0 as usize];
            if slot[0] == u32::MAX {
                slot[0] = b.id.0;
            } else if slot[1] == u32::MAX && slot[0] != b.id.0 {
                slot[1] = b.id.0;
            }
        }
    }
    let n = set.blocks.len();
    let mut sasiedzi: Vec<Vec<u32>> = vec![Vec::new(); n];
    for para in &po_segmencie {
        if para[0] == u32::MAX || para[1] == u32::MAX {
            continue;
        }
        sasiedzi[para[0] as usize].push(para[1]);
        sasiedzi[para[1] as usize].push(para[0]);
    }
    let mut start = vec![0u32; n + 1];
    let mut items = Vec::new();
    for (i, mut v) in sasiedzi.into_iter().enumerate() {
        v.sort_unstable();
        v.dedup();
        items.extend_from_slice(&v);
        start[i + 1] = items.len() as u32;
    }
    (start, items)
}

/// Statystyki podgrafu, na którym wyznaczane są ściany: `(V, E, C)`.
///
/// Ogólny wzór Eulera dla grafu planarnego o `C` składowych daje `E − V + C` ścian
/// wewnętrznych. Test WP6 porównuje z tym liczbę znalezionych orbit — i to jest
/// jedyny sprawdzian, który naprawdę potrafi wyłapać błąd w obchodzie półkrawędzi.
#[must_use]
pub fn face_graph_stats(net: &RoadNetwork) -> (usize, usize, usize) {
    let uzywany = |s: &super::road::RoadSegment| {
        s.class.is_driveable() && !s.flags.contains(RoadFlags::GRADE_SEPARATED)
    };
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); net.nodes.len()];
    let mut e = 0;
    for (i, s) in net.segments.iter().enumerate() {
        if uzywany(s) {
            adj[s.a.0 as usize].push(i as u32);
            adj[s.b.0 as usize].push(i as u32);
            e += 1;
        }
    }
    let v = adj.iter().filter(|a| !a.is_empty()).count();
    let mut seen = vec![false; net.nodes.len()];
    let mut c = 0;
    let mut stos = Vec::new();
    for start in 0..net.nodes.len() {
        if seen[start] || adj[start].is_empty() {
            continue;
        }
        c += 1;
        seen[start] = true;
        stos.push(start);
        while let Some(x) = stos.pop() {
            for &si in &adj[x] {
                let sg = &net.segments[si as usize];
                let o = if sg.a.0 as usize == x {
                    sg.b.0 as usize
                } else {
                    sg.a.0 as usize
                };
                if !seen[o] {
                    seen[o] = true;
                    stos.push(o);
                }
            }
        }
    }
    (v, e, c)
}

/// Odsunięcie wielokąta CCW do wewnątrz o połowę pasa drogowego każdej krawędzi.
/// `None`, gdy odsunięcie wywraca wielokąt (kwartał węższy niż drogi wokół niego).
fn inset(pts: &[Vec2], rows: &[f32]) -> Option<Vec<Vec2>> {
    let n = pts.len();
    // Proste odsunięte: punkt i kierunek.
    let mut lines: Vec<(Vec2, Vec2)> = Vec::with_capacity(n);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let d = (b - a).normalize_or_zero();
        if d.length_squared() < 0.5 {
            return None;
        }
        // Dla obchodu CCW wnętrze leży po lewej stronie krawędzi.
        let inward = Vec2::new(-d.y, d.x);
        lines.push((a + inward * (rows[i] * 0.5), d));
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (p1, d1) = lines[(i + n - 1) % n];
        let (p2, d2) = lines[i];
        let denom = d1.x * d2.y - d1.y * d2.x;
        if denom.abs() < 1e-5 {
            // Krawędzie współliniowe: punkt odsunięty wystarczy.
            out.push(p2);
            continue;
        }
        let q = p2 - p1;
        let t = (q.x * d2.y - q.y * d2.x) / denom;
        let v = p1 + d1 * t;
        // **Ogranicznik ostrza.** Przy dwóch krawędziach zbiegających się pod ostrym kątem
        // przecięcie prostych odsuniętych ucieka dowolnie daleko — na podglądzie widać to
        // jako kwartał w kształcie kilkusetmetrowej igły biegnącej przez pół mapy.
        // Powyżej limitu bierzemy punkt odsunięty wprost, kosztem ścięcia narożnika.
        let limit = rows[i] * 2.0 + 8.0;
        out.push(if (v - pts[i]).length() > limit { p2 } else { v });
    }
    // Samo dodatnie pole nie wystarczy: kwadrat odsunięty o więcej niż połowę boku
    // wywraca się na drugą stronę, a odbicie przez środek **zachowuje** orientację,
    // więc wynik znowu ma pole dodatnie. Odsunięcie może tylko zmniejszać pole —
    // i to jest warunek, który tę patologię łapie.
    let a = shoelace(&out);
    if a <= 0.0 || a > shoelace(pts) {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudokat_rosnie_z_katem() {
        // Kolejność: wschód, północ, zachód, południe.
        let e = pseudo_angle(Vec2::new(1.0, 0.0));
        let n = pseudo_angle(Vec2::new(0.0, 1.0));
        let w = pseudo_angle(Vec2::new(-1.0, 0.0));
        let s = pseudo_angle(Vec2::new(0.0, -1.0));
        assert!(e < n && n < w && w < s, "{e} {n} {w} {s}");
    }

    #[test]
    fn odsuniecie_zmniejsza_pole_o_pas_drogowy() {
        // Kwadrat 100 × 100 m, drogi o ROW 20 m → kwartał 80 × 80 m.
        let sq = [
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(100.0, 100.0),
            Vec2::new(0.0, 100.0),
        ];
        let inner = inset(&sq, &[20.0; 4]).expect("kwadrat da się odsunąć");
        let a = shoelace(&inner);
        assert!((a - 6400.0).abs() < 1.0, "pole = {a}");
    }

    #[test]
    fn odsuniecie_odrzuca_kwartal_wezszy_od_drog() {
        let sq = [
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        assert!(inset(&sq, &[30.0; 4]).is_none());
    }
}
