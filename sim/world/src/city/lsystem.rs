//! WP4 + WP5 — L-system arterii z ograniczeniami terenu oraz struktury inżynierskie
//! (M2 §5.2).
//!
//! Wariant parametryczny sterowany kolejką priorytetową (schemat Parish–Müller),
//! rozwijany **jednowątkowo**. Determinizm stoi na jednej rzeczy: klucz kolejki
//! `(prio, seq)` jest porządkiem **totalnym**, bo `seq` jest monotonicznym licznikiem
//! nadawanym przy wpychaniu. Nie ma remisów, więc nie ma czego rozstrzygać kolejnością
//! wstawiania do kopca.
//!
//! Kolej towarowa **nie** powstaje w tej podfazie: jej trasy prowadzą do klastrów stref
//! `IndustryHeavy` / `Logistics`, a strefowanie to Etap 4 (M2c, WP7). Przeniesienie
//! odnotowane w „Korektach planu" dokumentu podfazy.

use super::gates::{to_ivec, AirportFootprint, GatePlan};
use super::pattern::{Pattern, RingTable};
use super::road::{
    ClassSpec, NodeFlags, PolyArena, RoadClass, RoadFlags, RoadNode, RoadSegment, RoadStructure,
    SegmentId, UNASSIGNED_DISTRICT,
};
use super::{road::NodeId, CityPlan};
use crate::query::{Crossing, TerrainQuery};
use magnat_core::{det_math, rng, Rng, StreamId, Tick};
use magnat_spatial::Vec2;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Bok komórki indeksu roboczego L-systemu, w metrach.
///
/// Własny indeks przyrostowy, a nie `CsrGrid` z `engine/spatial`: tamten jest z założenia
/// statyczny („budowany raz, tylko do odczytu", M2a §5.1), a tu co segment dokładamy
/// węzeł i krawędź. Przebudowa CSR przy każdym z 16 tys. segmentów byłaby O(n²).
const BUCKET_M: f32 = 64.0;

/// Najmniejsze wypiętrzenie drogi nad teren, przy którym mówimy o nasypie, w decymetrach.
const EMBANKMENT_MIN_DM: i32 = 15;

/// Statystyki odrzuceń — mitygacja ryzyka R1 fazy. Bez nich rozbieganie się generatora
/// widać dopiero na obrazku, a nie w liczbie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LStats {
    pub proposals: u32,
    pub accepted: u32,
    pub rejected_slope: u32,
    pub rejected_crossing: u32,
    pub rejected_angle: u32,
    pub rejected_zone: u32,
    pub rejected_budget: u32,
    pub snapped: u32,
    pub split: u32,
    pub bridges: u32,
    pub tunnels: u32,
    pub embankments: u32,
    pub pruned: u32,
}

impl LStats {
    /// Udział odrzuceń wśród rozpatrzonych propozycji, w procentach.
    #[must_use]
    pub fn rejection_pct(&self) -> u32 {
        let rej =
            self.rejected_slope + self.rejected_crossing + self.rejected_angle + self.rejected_zone;
        if self.proposals == 0 {
            return 0;
        }
        rej * 100 / self.proposals
    }
}

/// Propozycja w kolejce L-systemu.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Proposal {
    from: NodeId,
    dir: Vec2,
    class: RoadClass,
    gen: u16,
    prio: u32,
    seq: u32,
    /// Propozycja łącznika (reguła P4) — cel jest znany, kierunek nie decyduje.
    target: Option<NodeId>,
    /// Propozycja obwodnicowa: kierunek jest styczną do okręgu wokół środka miasta.
    ring: bool,
    /// Łańcuch wyprowadzony z bramy. Rośnie aż do miasta, nawet przez pustkę —
    /// inaczej droga od portu leżącego poza obszarem zurbanizowanym kończy się po
    /// jednym odcinku i wypada przy domykaniu sieci razem z bramą.
    gate: bool,
}

/// Klucz kopca. `Ord` po `(prio, seq)`, a `BinaryHeap` jest kopcem maksimum, więc
/// wpychamy `Reverse`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Key(u32, u32);

pub struct Grown {
    pub nodes: Vec<RoadNode>,
    pub segments: Vec<RoadSegment>,
    pub geom: PolyArena,
    /// Węzeł sieci dla kolejnych bram z `GatePlan`; `None` dla bram kolejowych (M2c).
    pub gate_nodes: Vec<Option<NodeId>>,
    pub stats: LStats,
}

struct Builder<'a> {
    t: &'a dyn TerrainQuery,
    plan: &'a CityPlan,
    center: Vec2,
    map_m: f32,
    urban_r: f32,
    rings: &'a RingTable,
    grid_theta: f32,
    airport: Option<AirportFootprint>,

    nodes: Vec<RoadNode>,
    segments: Vec<RoadSegment>,
    geom: PolyArena,
    node_grid: Vec<Vec<u32>>,
    seg_grid: Vec<Vec<u32>>,
    grid_dim: usize,
    /// Ranga najwyższej klasy incydentnej — do reguły snapowania „klasa ≥ class".
    node_rank: Vec<u8>,
    stats: LStats,
}

impl<'a> Builder<'a> {
    fn new(
        plan: &'a CityPlan,
        t: &'a dyn TerrainQuery,
        gp: &GatePlan,
        rings: &'a RingTable,
    ) -> Builder<'a> {
        let map_m = plan.map_size_m() as f32;
        let dim = (map_m / BUCKET_M).ceil() as usize + 1;
        // Kąt siatki miasta: jedna liczba na świat, losowana raz. Bez niej każde miasto
        // z wzorcem `Grid` miałoby ulice dokładnie wzdłuż osi mapy.
        let mut r = rng(plan.seed, StreamId::RoadsL, magnat_core::NO_ENTITY, Tick(0));
        let grid_theta = r.gen_range_u32(90) as f32;
        Builder {
            t,
            plan,
            center: gp.center,
            map_m,
            urban_r: plan.urban_radius_m(),
            rings,
            grid_theta,
            airport: gp.airport,
            nodes: Vec::new(),
            segments: Vec::new(),
            geom: PolyArena::new(),
            node_grid: vec![Vec::new(); dim * dim],
            seg_grid: vec![Vec::new(); dim * dim],
            grid_dim: dim,
            node_rank: Vec::new(),
            stats: LStats::default(),
        }
    }

    // ── indeks roboczy ───────────────────────────────────────────────────────────────

    fn cell(&self, p: Vec2) -> usize {
        let x = (p.x / BUCKET_M).clamp(0.0, self.grid_dim as f32 - 1.0) as usize;
        let y = (p.y / BUCKET_M).clamp(0.0, self.grid_dim as f32 - 1.0) as usize;
        y * self.grid_dim + x
    }

    fn add_node(&mut self, p: Vec2, flags: NodeFlags) -> NodeId {
        // `height_at` jest w jednostkach 0,5 m (K-13) → ×5 na decymetry.
        let z = self.t.height_at(p.x as i32, p.y as i32) * 5;
        self.add_node_z(p, z, flags)
    }

    fn add_node_z(&mut self, p: Vec2, z_dm: i32, flags: NodeFlags) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(RoadNode {
            pos: p,
            z_dm,
            degree: 0,
            flags,
        });
        self.node_rank.push(0);
        let c = self.cell(p);
        self.node_grid[c].push(id.0);
        id
    }

    /// Najbliższy węzeł w promieniu `r` o randze co najmniej `min_rank`.
    fn find_node(&self, p: Vec2, r: f32, min_rank: u8, exclude: &[NodeId]) -> Option<NodeId> {
        let mut best: Option<(f32, NodeId)> = None;
        let span = (r / BUCKET_M).ceil() as i32;
        let (cx, cy) = ((p.x / BUCKET_M) as i32, (p.y / BUCKET_M) as i32);
        for dy in -span..=span {
            for dx in -span..=span {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= self.grid_dim as i32 || y >= self.grid_dim as i32 {
                    continue;
                }
                for &n in &self.node_grid[y as usize * self.grid_dim + x as usize] {
                    let id = NodeId(n);
                    if exclude.contains(&id) || self.node_rank[n as usize] < min_rank {
                        continue;
                    }
                    let d = (self.nodes[n as usize].pos - p).length();
                    if d <= r
                        && best.is_none_or(|(bd, bi)| {
                            // Remis rozstrzyga indeks — bez tego kolejność komórek
                            // decydowałaby o wyniku.
                            d < bd || (d == bd && id < bi)
                        })
                    {
                        best = Some((d, id));
                    }
                }
            }
        }
        best.map(|(_, id)| id)
    }

    fn index_segment(&mut self, s: SegmentId, a: Vec2, b: Vec2) {
        let len = (b - a).length().max(1.0);
        let kroki = (len / (BUCKET_M * 0.5)).ceil() as i32;
        let mut last = usize::MAX;
        for k in 0..=kroki {
            let p = a + (b - a) * (k as f32 / kroki as f32);
            let c = self.cell(p);
            if c != last {
                self.seg_grid[c].push(s.0);
                last = c;
            }
        }
    }

    fn segments_near(&self, a: Vec2, b: Vec2, out: &mut Vec<u32>) {
        out.clear();
        let len = (b - a).length().max(1.0);
        let kroki = (len / (BUCKET_M * 0.5)).ceil() as i32;
        for k in 0..=kroki {
            let p = a + (b - a) * (k as f32 / kroki as f32);
            let (cx, cy) = ((p.x / BUCKET_M) as i32, (p.y / BUCKET_M) as i32);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (x, y) = (cx + dx, cy + dy);
                    if x < 0 || y < 0 || x >= self.grid_dim as i32 || y >= self.grid_dim as i32 {
                        continue;
                    }
                    out.extend_from_slice(&self.seg_grid[y as usize * self.grid_dim + x as usize]);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }

    // ── tworzenie segmentów ──────────────────────────────────────────────────────────

    fn push_segment(
        &mut self,
        a: NodeId,
        b: NodeId,
        class: RoadClass,
        structure: RoadStructure,
        extra: RoadFlags,
    ) -> SegmentId {
        let spec = class.spec();
        let (pa, pb) = (self.nodes[a.0 as usize].pos, self.nodes[b.0 as usize].pos);
        let geom = self.geom.push(&[pa, pb]);
        let mut flags = extra.with(RoadFlags::SIDEWALK);
        if spec.lanes_bwd == 0 {
            flags = flags.with(RoadFlags::ONEWAY);
        }
        if class.is_rail() {
            flags = RoadFlags::RAIL.with(extra);
        }
        if matches!(class, RoadClass::Pedestrian) || spec.max_tonnage_t > 0 {
            flags = flags.with(RoadFlags::NO_HEAVY);
        }
        let id = SegmentId(self.segments.len() as u32);
        self.segments.push(RoadSegment {
            a,
            b,
            class,
            geom,
            structure,
            lanes_fwd: spec.lanes_fwd,
            lanes_bwd: spec.lanes_bwd,
            row_m: spec.row_m,
            speed_kph: spec.speed_kph,
            max_tonnage_t: spec.max_tonnage_t,
            length_dm: ((pb - pa).length() * 10.0) as u32,
            district: UNASSIGNED_DISTRICT,
            flags,
        });
        self.nodes[a.0 as usize].degree = self.nodes[a.0 as usize].degree.saturating_add(1);
        self.nodes[b.0 as usize].degree = self.nodes[b.0 as usize].degree.saturating_add(1);
        let r = class.rank();
        self.node_rank[a.0 as usize] = self.node_rank[a.0 as usize].max(r);
        self.node_rank[b.0 as usize] = self.node_rank[b.0 as usize].max(r);
        self.index_segment(id, pa, pb);
        match structure {
            RoadStructure::Bridge { .. } => self.stats.bridges += 1,
            RoadStructure::Tunnel { .. } => self.stats.tunnels += 1,
            RoadStructure::Embankment { .. } => self.stats.embankments += 1,
            RoadStructure::AtGrade => {}
        }
        id
    }

    /// Podział istniejącego segmentu w punkcie `p` (ograniczenie lokalne 4).
    ///
    /// Stare wpisy w indeksie zostają: pokrywają **nadzbiór** nowego przebiegu, więc
    /// zapytanie zwróci ten segment jako kandydata, a dokładny test i tak go odrzuci.
    /// Kasowanie ich kosztowałoby przeszukanie komórek bez żadnej korzyści.
    fn split_segment(&mut self, s: SegmentId, p: Vec2) -> NodeId {
        let seg = self.segments[s.0 as usize];
        let (na, nb) = (self.nodes[seg.a.0 as usize], self.nodes[seg.b.0 as usize]);
        let dl = (nb.pos - na.pos).length().max(1.0);
        let t = ((p - na.pos).length() / dl).clamp(0.0, 1.0);
        let z = na.z_dm + ((nb.z_dm - na.z_dm) as f32 * t) as i32;
        let n = self.add_node_z(p, z, NodeFlags::JUNCTION);
        let pa = na.pos;
        self.segments[s.0 as usize].b = n;
        self.segments[s.0 as usize].geom = self.geom.push(&[pa, p]);
        self.segments[s.0 as usize].length_dm = ((p - pa).length() * 10.0) as u32;
        self.nodes[n.0 as usize].degree += 1;
        self.node_rank[n.0 as usize] = seg.class.rank();
        // Druga połowa dziedziczy wszystko poza geometrią.
        let pb = self.nodes[seg.b.0 as usize].pos;
        let id = SegmentId(self.segments.len() as u32);
        let mut half = seg;
        half.a = n;
        half.geom = self.geom.push(&[p, pb]);
        half.length_dm = ((pb - p).length() * 10.0) as u32;
        self.segments.push(half);
        self.nodes[n.0 as usize].degree += 1;
        self.index_segment(id, p, pb);
        self.stats.split += 1;
        n
    }

    // ── ograniczenia lokalne ─────────────────────────────────────────────────────────

    /// Ograniczenie 6 (w planie numerowane 7): strefa zakazana.
    ///
    /// „Na wodzie bez mostu" znaczy: **węzeł** nie ma prawa stanąć w wodzie. Sam odcinek
    /// wolno poprowadzić nad wodą, bo od tego jest most — ale bez tego warunku miasto
    /// portowe rozrastało się mostami **w morze**: każdy 200-metrowy odcinek arterii
    /// mieścił się w limicie rozpiętości 300 m, więc L-system budował po tafli w nieskończoność.
    fn forbidden(&self, p: Vec2) -> bool {
        if p.x < 8.0 || p.y < 8.0 || p.x > self.map_m - 8.0 || p.y > self.map_m - 8.0 {
            return true;
        }
        if super::gates::is_water(self.t, p) {
            return true;
        }
        self.airport.is_some_and(|fp| fp.contains(p))
    }

    /// Ograniczenie 1: **niweleta**, czyli podłużne nachylenie samej drogi.
    ///
    /// **Korekta wobec planu.** Plan każe próbkować `slope_at` co 8 m i porównywać
    /// z `max_slope[class]`. To jest inna wielkość: `slope_at` zwraca moduł gradientu
    /// terenu, więc droga biegnąca poziomo po zboczu ma niweletę 0%, a `slope_at` 20%.
    /// Przy kryterium z planu autostrada (5%) była odrzucana już na pierwszym segmencie
    /// w każdym regionie — zmierzone: 100% odrzuceń. Liczymy więc spadek między końcami
    /// odcinka; za profil pośredni (wykop, nasyp) odpowiada ograniczenie 2, które ma
    /// od M1 gotową odpowiedź w `crossing_cost`.
    ///
    /// Jednostka wyniku jest ta sama co `slope_at` (tan α × 64), żeby próg z tabeli klas
    /// dało się porównać bez konwersji w dwie strony.
    fn grade(&self, a: Vec2, b: Vec2) -> u8 {
        let ha = self.t.height_at(a.x as i32, a.y as i32) * 5;
        let hb = self.t.height_at(b.x as i32, b.y as i32) * 5;
        grade_units(a, ha, b, hb)
    }

    /// Ograniczenie 2: przeszkoda i wybór struktury.
    ///
    /// **Kolejność zamieniona względem planu** (tam nachylenie idzie przed strukturą):
    /// most i tunel z definicji nie podążają za terenem, więc test nachylenia stosuje się
    /// tylko do przebiegu po gruncie. Przy kolejności z planu żadna przeprawa przez dolinę
    /// nie miałaby szansy powstać — odrzuciłby ją zbocze, którego most nie dotyka.
    fn structure_for(&self, a: Vec2, b: Vec2, spec: &ClassSpec) -> Option<RoadStructure> {
        match self.t.crossing_cost(to_ivec(a), to_ivec(b)) {
            Crossing::Flat => Some(RoadStructure::AtGrade),
            Crossing::Bridge {
                span_m,
                clearance_m,
            } => (span_m <= u32::from(spec.bridge_max_m)).then(|| RoadStructure::Bridge {
                clearance_dm: clearance_m.saturating_mul(10),
            }),
            Crossing::Tunnel { len_m, .. } => {
                if spec.tunnel_trigger_m == 0 || len_m > u32::from(spec.tunnel_max_m) {
                    return None;
                }
                let cover = self.cover_dm(a, b);
                (cover >= i32::from(spec.tunnel_trigger_m) * 10).then_some(RoadStructure::Tunnel {
                    cover_dm: cover.clamp(0, i32::from(u16::MAX)) as u16,
                })
            }
            Crossing::Embankment { .. } => {
                // Znak mówi, po której stronie drogi jest teren: dodatni to wykop,
                // ujemny nasyp. Limit 8 m obowiązuje w obie strony — powyżej robi się
                // z tego wiadukt albo przekop, a żadnego z nich tabela klas nie zna.
                //
                // `Crossing::Embankment` z M1 pada już przy 3 m deniwelacji na całym
                // odcinku, czyli przy zwykłej ulicy na stoku. Budowlą jest to dopiero
                // powyżej `EMBANKMENT_MIN_DM` — inaczej co czwarty segment miasta
                // byłby „nasypem" (zmierzone: 230 na 1012).
                let d = self.cover_dm(a, b);
                if d.abs() > 80 {
                    return None;
                }
                Some(if -d >= EMBANKMENT_MIN_DM {
                    RoadStructure::Embankment {
                        height_dm: (-d) as u16,
                    }
                } else {
                    RoadStructure::AtGrade
                })
            }
        }
    }

    /// Rzędna niwelety segmentu `s` w punkcie `p` leżącym na nim.
    fn interp_z(&self, s: SegmentId, p: Vec2) -> i32 {
        let seg = &self.segments[s.0 as usize];
        let (na, nb) = (self.nodes[seg.a.0 as usize], self.nodes[seg.b.0 as usize]);
        let dl = (nb.pos - na.pos).length().max(1.0);
        let t = ((p - na.pos).length() / dl).clamp(0.0, 1.0);
        na.z_dm + ((nb.z_dm - na.z_dm) as f32 * t) as i32
    }

    /// Przewyższenie terenu nad cięciwą w połowie odcinka, w decymetrach.
    /// Dodatnie = teren nad drogą (tunel), ujemne = droga nad terenem (nasyp).
    fn cover_dm(&self, a: Vec2, b: Vec2) -> i32 {
        let h = |p: Vec2| self.t.height_at(p.x as i32, p.y as i32);
        let mid = (a + b) * 0.5;
        // `height_at` jest w jednostkach 0,5 m (K-13) — stąd ×5 na decymetry.
        (h(mid) - (h(a) + h(b)) / 2) * 5
    }

    /// Ograniczenie 5: minimalny kąt do krawędzi incydentnych w węźle docelowym.
    fn angle_ok(&self, node: NodeId, dir_in: Vec2, class: RoadClass) -> bool {
        // cos 30° = 0,866; dla `Local` próg to 22° → cos = 0,927.
        let limit = if matches!(class, RoadClass::Local | RoadClass::Service) {
            0.927_f32
        } else {
            0.866_f32
        };
        let p = self.nodes[node.0 as usize].pos;
        let incoming = -dir_in.normalize_or_zero();
        for &s in self.segments_at_node(node).iter() {
            let seg = &self.segments[s as usize];
            let other = if seg.a == node { seg.b } else { seg.a };
            let d = (self.nodes[other.0 as usize].pos - p).normalize_or_zero();
            if d.dot(incoming) > limit {
                return false;
            }
        }
        true
    }

    /// Segmenty incydentne z węzłem — liniowo po komórce indeksu, bo na tym etapie
    /// nie ma jeszcze CSR (ten powstaje przy domykaniu).
    fn segments_at_node(&self, n: NodeId) -> Vec<u32> {
        let p = self.nodes[n.0 as usize].pos;
        let c = self.cell(p);
        self.seg_grid[c]
            .iter()
            .copied()
            .filter(|&s| {
                let seg = &self.segments[s as usize];
                seg.a == n || seg.b == n
            })
            .collect()
    }

    // ── wzrost ───────────────────────────────────────────────────────────────────────

    fn pattern_at(&self, p: Vec2) -> Pattern {
        let r = (p - self.center).length() / self.urban_r.max(1.0);
        let slope = self.t.slope_at(p.x as i32, p.y as i32);
        self.rings.pattern_at(r, slope, self.grid_theta)
    }

    /// Kierunek kolejnego segmentu wg wzorca globalnego.
    fn steer(&self, prop: &Proposal, from: Vec2, r: &mut Rng) -> Vec2 {
        if prop.ring {
            // Obwodnica: styczna do okręgu wokół środka, zawsze w tę samą stronę.
            let radial = (from - self.center).normalize_or_zero();
            return Vec2::new(-radial.y, radial.x);
        }
        let d = prop.dir.normalize_or_zero();
        match self.pattern_at(from) {
            Pattern::Radial => {
                let to_c = (self.center - from).normalize_or_zero();
                let base = if d.dot(to_c) > 0.0 { to_c } else { -to_c };
                rotate(base, jitter(r, 8.0))
            }
            Pattern::Grid(theta) => {
                let snapped = snap_to_grid(d, theta);
                rotate(snapped, jitter(r, 3.0))
            }
            Pattern::Organic => rotate(d, jitter(r, 25.0)),
            Pattern::Contour => {
                // Pięciu kandydatów co 12°, wygrywa najmniejsze nachylenie.
                let mut best = (u8::MAX, d);
                for i in -2..=2 {
                    let c = rotate(d, (i * 12) as f32);
                    let probe = from + c * 40.0;
                    let s = self.t.slope_at(probe.x as i32, probe.y as i32);
                    if s < best.0 {
                        best = (s, c);
                    }
                }
                best.1
            }
            Pattern::Superblock(theta) => snap_to_grid(d, theta),
        }
    }

    /// Próba wyprowadzenia segmentu z propozycji. Zwraca węzeł końcowy i to,
    /// czy kontynuacja ma sens (scalenie z istniejącym węzłem ją wygasza).
    fn grow(&mut self, prop: &Proposal, near: &mut Vec<u32>) -> Option<(NodeId, Vec2, bool)> {
        let spec = prop.class.spec();
        let from = self.nodes[prop.from.0 as usize].pos;
        let mut r = rng(self.plan.seed, StreamId::RoadsL, prop.seq, Tick(0));

        // Cel jawny (reguła P4) omija wzorzec: łącznik ma trafić w konkretny węzeł.
        let (dir0, len0) = match prop.target {
            Some(t) => {
                let d = self.nodes[t.0 as usize].pos - from;
                (d.normalize_or_zero(), d.length())
            }
            None => {
                // Blokowisko: kolektor prowadzi trzykrotnie dłuższy odcinek,
                // bo superblok jest z definicji kwartałem bez ulic w środku.
                let mult = if matches!(self.pattern_at(from), Pattern::Superblock(_))
                    && matches!(prop.class, RoadClass::Collector)
                {
                    3.0
                } else {
                    1.0
                };
                (
                    self.steer(prop, from, &mut r),
                    f32::from(spec.seg_len_m) * mult,
                )
            }
        };
        if dir0.length_squared() < 0.5 {
            return None;
        }

        // Ograniczenie 1 z próbami ratunkowymi: obrót ±15°, ±30°, potem skrócenie do 50%.
        let proby: [(f32, f32); 6] = [
            (0.0, 1.0),
            (15.0, 1.0),
            (-15.0, 1.0),
            (30.0, 1.0),
            (-30.0, 1.0),
            (0.0, 0.5),
        ];
        let limit = spec.max_slope_units();
        let mut wybrane: Option<(Vec2, Vec2, RoadStructure)> = None;
        let mut powod_slope = false;
        let mut powod_cross = false;

        for (deg, skala) in proby {
            if prop.target.is_some() && (deg != 0.0 || skala != 1.0) {
                break; // łącznik nie negocjuje przebiegu — albo trafia, albo nie
            }
            let dir = rotate(dir0, deg);
            let end = from + dir * (len0 * skala);
            if self.forbidden(end) {
                self.stats.rejected_zone += 1;
                continue;
            }
            let Some(structure) = self.structure_for(from, end, &spec) else {
                powod_cross = true;
                continue;
            };
            // Nachylenie dotyczy przebiegu po gruncie; most i tunel go nie widzą.
            let po_gruncie = matches!(
                structure,
                RoadStructure::AtGrade | RoadStructure::Embankment { .. }
            );
            if po_gruncie && self.grade(from, end) > limit {
                powod_slope = true;
                continue;
            }
            wybrane = Some((dir, end, structure));
            break;
        }

        let Some((dir, mut end, structure)) = wybrane else {
            if powod_slope {
                self.stats.rejected_slope += 1;
            } else if powod_cross {
                self.stats.rejected_crossing += 1;
            }
            return None;
        };

        // Ograniczenie 3 (w planie 4): snapowanie do istniejącego węzła.
        let snap_r = len0 * 0.25;
        let mut scalony = false;
        let mut target = match prop.target {
            Some(t) => Some(t),
            None => self.find_node(end, snap_r, prop.class.rank(), &[prop.from]),
        };
        if let Some(t) = target {
            end = self.nodes[t.0 as usize].pos;
            scalony = true;
        }

        // Ograniczenie 4 (w planie 5): przecięcie z istniejącym segmentem.
        //
        // Sonda jest **przedłużona o 25%**, gdy koniec nie trafił w żaden węzeł.
        // To jest to samo, co robi snapowanie, tylko wobec krawędzi zamiast wierzchołka:
        // ulica kończąca się 20 m przed równoległą do niej ulicą ma się do niej podłączyć,
        // a nie zostać ślepym zaułkiem, który i tak wypadnie przy domykaniu sieci.
        // Bez tego sieć zostaje drzewem: zmierzone 161 ścian na 923 segmenty.
        let probe_end = if scalony {
            end
        } else {
            from + dir * (end - from).length() * 1.25
        };
        self.segments_near(from, probe_end, near);
        // Trafienie bliżej niż to od węzła jest **stykiem**, nie skrzyżowaniem.
        // Próg metrowy, nie ułamkowy: przy ułamkowym przecięcie 40 cm od końca
        // czterdziestometrowej dojazdówki wypadało z wykrywania, nie dostawało węzła
        // i po cichu łamało planarność grafu, na której stoi wyznaczanie kwartałów.
        const STYK_M: f32 = 1.0;
        let mut najblizsze: Option<(f32, SegmentId, Vec2)> = None;
        let mut do_wezla: Option<(f32, NodeId)> = None;
        let mut wiadukt: Option<SegmentId> = None;
        for &s in near.iter() {
            let seg = &self.segments[s as usize];
            let (pa, pb) = (
                self.nodes[seg.a.0 as usize].pos,
                self.nodes[seg.b.0 as usize].pos,
            );
            let Some((p, t_param)) = segment_intersection(from, probe_end, pa, pb) else {
                continue;
            };
            // Styk w węźle początkowym kandydata — to nie jest przecięcie, tylko wyjście
            // z tego samego skrzyżowania.
            if (p - from).length() < STYK_M {
                continue;
            }
            // Bezkolizyjny jest **most i tunel**, nie nasyp: droga na nasypie nadal
            // krzyżuje się z innymi w poziomie (plan §5.2 mówi wprost o `Bridge × AtGrade`).
            let bezkolizyjny = |st: RoadStructure| {
                matches!(
                    st,
                    RoadStructure::Bridge { .. } | RoadStructure::Tunnel { .. }
                )
            };
            if bezkolizyjny(structure) || bezkolizyjny(seg.structure) {
                if wiadukt.is_none() {
                    wiadukt = Some(SegmentId(s));
                }
                continue;
            }
            // Trafienie tuż przy końcu istniejącego segmentu: kandydat ma się podłączyć
            // do **tego węzła**, a nie przejść obok niego bez połączenia.
            let blisko = if (p - pa).length() < STYK_M {
                Some(seg.a)
            } else if (p - pb).length() < STYK_M {
                Some(seg.b)
            } else {
                None
            };
            if let Some(n) = blisko {
                if n != prop.from && do_wezla.is_none_or(|(b, _)| t_param < b) {
                    do_wezla = Some((t_param, n));
                }
                continue;
            }
            if self.forbidden(p) {
                continue;
            }
            if najblizsze
                .is_none_or(|(b, bs, _)| t_param < b || (t_param == b && SegmentId(s) < bs))
            {
                najblizsze = Some((t_param, SegmentId(s), p));
            }
        }
        // Bliżej jest to, co napotkamy pierwsze — węzeł albo środek segmentu.
        if let Some((tw, n)) = do_wezla {
            if najblizsze.is_none_or(|(t, _, _)| tw < t) {
                najblizsze = None;
                target = Some(n);
                end = self.nodes[n.0 as usize].pos;
                scalony = true;
            }
        }

        let mut structure = structure;
        // Podział istniejącego segmentu jest **ostatnią** rzeczą, jaką robimy: mutuje sieć,
        // więc nie może się wydarzyć przed sprawdzeniem, czy segment w ogóle powstanie.
        let mut do_podzialu: Option<(SegmentId, Vec2)> = None;
        if let Some((_, s, p)) = najblizsze {
            end = p;
            do_podzialu = Some((s, p));
            scalony = true;
        }

        // Snapowanie, przedłużenie i podział — każde z nich zmienia koniec odcinka,
        // a więc i jego długość. Struktura musi zostać przeliczona na **ostateczny**
        // przebieg, inaczej most rósłby ponad limit rozpiętości swojej klasy.
        if scalony {
            let Some(st) = self.structure_for(from, end, &spec) else {
                self.stats.rejected_crossing += 1;
                return None;
            };
            structure = st;
        }

        // **Niweleta ostateczna.** Pierwsze sprawdzenie (ograniczenie 1) dotyczyło końca
        // sprzed snapowania i przedłużenia; po nich odcinek ma inną długość i inny koniec,
        // a limit klasy obowiązuje ten, który naprawdę powstanie.
        let z_from = self.nodes[prop.from.0 as usize].z_dm;
        let z_end = match (target, do_podzialu) {
            (_, Some((s, p))) => self.interp_z(s, p),
            (Some(t), None) => self.nodes[t.0 as usize].z_dm,
            (None, None) => self.t.height_at(end.x as i32, end.y as i32) * 5,
        };
        if matches!(
            structure,
            RoadStructure::AtGrade | RoadStructure::Embankment { .. }
        ) && grade_units(from, z_from, end, z_end) > limit
        {
            self.stats.rejected_slope += 1;
            return None;
        }

        if let Some((s, p)) = do_podzialu {
            target = Some(self.split_segment(s, p));
        }

        // Ograniczenie 5 (w planie 6): minimalny kąt — **także w węźle początkowym**.
        // Bez tego dwie propozycje wychodzące z tego samego węzła w niemal tym samym
        // kierunku dawały dwie **nakładające się** krawędzie. Geometrycznie one się nie
        // „przecinają" (wyznacznik zeruje się na współliniowości), więc test planarności
        // ich nie widzi, ale porządek kątowy w węźle przestaje odpowiadać rysunkowi
        // i obchód półkrawędzi scala sąsiednie ściany: zmierzone 154 orbity wobec 164
        // wynikających ze wzoru Eulera.
        if !self.angle_ok(prop.from, from - end, prop.class) {
            self.stats.rejected_angle += 1;
            return None;
        }

        let node = match target {
            Some(t) => {
                if !self.angle_ok(t, end - from, prop.class) {
                    self.stats.rejected_angle += 1;
                    return None;
                }
                self.stats.snapped += 1;
                t
            }
            None => self.add_node_z(end, z_end, NodeFlags::NONE),
        };
        if node == prop.from {
            return None;
        }
        let sid = self.push_segment(prop.from, node, prop.class, structure, RoadFlags::NONE);
        if let Some(inny) = wiadukt {
            self.segments[sid.0 as usize].flags = self.segments[sid.0 as usize]
                .flags
                .with(RoadFlags::GRADE_SEPARATED);
            self.segments[inny.0 as usize].flags = self.segments[inny.0 as usize]
                .flags
                .with(RoadFlags::GRADE_SEPARATED);
        }
        self.stats.accepted += 1;
        Some((node, dir, !scalony))
    }
}

/// Niweleta w jednostkach `slope_at` (tan α × 64), z rzędnych w decymetrach.
fn grade_units(a: Vec2, za_dm: i32, b: Vec2, zb_dm: i32) -> u8 {
    let dh = (zb_dm - za_dm).abs() as f32 * 0.1;
    let len = (b - a).length().max(1.0);
    ((dh / len) * 64.0).min(255.0) as u8
}

/// Obrót wektora o kąt w stopniach. `det_math` zamiast `f64::sin`/`cos` — 00 §K-6.
fn rotate(d: Vec2, deg: f32) -> Vec2 {
    if deg == 0.0 {
        return d;
    }
    let r = f64::from(deg) * std::f64::consts::PI / 180.0;
    let (s, c) = (det_math::sin(r), det_math::cos(r));
    let (x, y) = (f64::from(d.x), f64::from(d.y));
    Vec2::new((x * c - y * s) as f32, (x * s + y * c) as f32)
}

/// Szum kierunku w zakresie ±`amp` stopni.
fn jitter(r: &mut Rng, amp: f32) -> f32 {
    let v = r.gen_range_u32(2001) as f32 / 1000.0 - 1.0;
    v * amp
}

/// Snapowanie do najbliższej z czterech osi siatki obróconej o `theta` stopni.
fn snap_to_grid(d: Vec2, theta: f32) -> Vec2 {
    let base = rotate(Vec2::new(1.0, 0.0), theta);
    let perp = Vec2::new(-base.y, base.x);
    let mut best = (f32::MIN, base);
    for c in [base, -base, perp, -perp] {
        let dot = d.dot(c);
        if dot > best.0 {
            best = (dot, c);
        }
    }
    best.1
}

/// Przecięcie dwóch odcinków w ich domknięciu. Zwraca punkt i parametr wzdłuż pierwszego.
///
/// Bez epsilonów — o tym, czy trafienie jest skrzyżowaniem, stykiem w węźle, czy
/// niczym, rozstrzyga wywołujący, bo tylko on wie, które węzły są jego własne.
fn segment_intersection(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> Option<(Vec2, f32)> {
    let r = a2 - a1;
    let s = b2 - b1;
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-6 {
        return None;
    }
    let q = b1 - a1;
    let t = (q.x * s.y - q.y * s.x) / denom;
    let u = (q.x * r.y - q.y * r.x) / denom;
    if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
        return None;
    }
    Some((a1 + r * t, t))
}

/// WP4 + WP5: rozwinięcie sieci z bram.
#[must_use]
pub fn grow_network(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
    gp: &GatePlan,
    rings: &RingTable,
) -> Grown {
    let mut b = Builder::new(plan, t, gp, rings);
    let mut heap: BinaryHeap<Reverse<(Key, u32)>> = BinaryHeap::new();
    let mut props: Vec<Proposal> = Vec::new();
    let mut seq: u32 = 0;
    let budget = plan.segment_budget();

    let push = |props: &mut Vec<Proposal>,
                heap: &mut BinaryHeap<Reverse<(Key, u32)>>,
                seq: &mut u32,
                mut p: Proposal| {
        p.seq = *seq;
        *seq += 1;
        heap.push(Reverse((Key(p.prio, p.seq), props.len() as u32)));
        props.push(p);
    };

    // Aksjomat ω: z każdej bramy drogowej propozycja w kierunku centrum.
    let mut gate_nodes = Vec::new();
    for g in &gp.gates {
        let (kind, dir) = (g.kind, g.dir_inward);
        if kind.is_rail() {
            // Bramy kolejowe czekają na M2c razem z torami. Węzła **nie** zakładamy:
            // byłby wierzchołkiem bez krawędzi, który i tak wypada przy domykaniu sieci.
            gate_nodes.push(None);
            continue;
        }
        let n = b.add_node(g.node_pos, NodeFlags::GATE);
        gate_nodes.push(Some(n));
        let class = match kind {
            super::gates::GateKind::Airport | super::gates::GateKind::Port => RoadClass::Arterial,
            _ => RoadClass::Highway,
        };
        // **Wachlarz** zamiast jednej propozycji. Wewnętrzne próby ratunkowe w `grow`
        // (obrót ±15°, ±30°, skrócenie) obracają ten sam kierunek; przy bramie stojącej
        // nad odnogą wody wszystkie pięć trafiało w tę samą taflę i brama zostawała
        // węzłem bez krawędzi, po czym wypadała przy domykaniu sieci. Kolejne ramiona
        // wachlarza odchylone o 26° są od siebie na tyle różne, że reguła minimalnego
        // kąta w węźle początkowym przepuści tylko pierwsze, które się uda.
        for odchylka in [0.0_f32, 26.0, -26.0, 52.0, -52.0] {
            push(
                &mut props,
                &mut heap,
                &mut seq,
                Proposal {
                    from: n,
                    dir: rotate(dir, odchylka),
                    class,
                    gen: 0,
                    prio: 0,
                    seq: 0,
                    target: None,
                    ring: false,
                    gate: true,
                },
            );
        }
    }

    // Obwodnica dla miast > 150 tys. — jeden zalążek, resztę domyka reguła P4.
    if plan.target_pop > 150_000 {
        let start = gp.center + Vec2::new(0.0, plan.urban_radius_m() * 0.8);
        let n = b.add_node(start, NodeFlags::NONE);
        push(
            &mut props,
            &mut heap,
            &mut seq,
            Proposal {
                from: n,
                dir: Vec2::new(1.0, 0.0),
                class: RoadClass::Arterial,
                gen: 0,
                prio: 0,
                seq: 0,
                target: None,
                ring: true,
                gate: false,
            },
        );
    }

    let mut near = Vec::new();
    while let Some(Reverse((_, idx))) = heap.pop() {
        if b.segments.len() >= budget {
            b.stats.rejected_budget += 1;
            continue;
        }
        let prop = props[idx as usize];
        b.stats.proposals += 1;
        let Some((node, dir, kontynuuj)) = b.grow(&prop, &mut near) else {
            continue;
        };
        // Scalenie z istniejącym węzłem wygasza **kontynuację** (P1), ale nie odcina
        // rozgałęzień: skrzyżowanie jest dokładnie tym miejscem, w którym odchodzą
        // ulice niższej klasy. Przy gaszeniu całej produkcji sieć kończyła wzrost
        // przy 1209 segmentach na budżecie 1600 i zostawała drzewem.
        let p_end = b.nodes[node.0 as usize].pos;
        let r_end = (p_end - gp.center).length();
        let spec = prop.class.spec();
        let limit_generacji = prop.gen + 1 > spec.max_gen;
        let mut r = rng(plan.seed, StreamId::RoadsL, prop.seq, Tick(1));

        // P1 — kontynuacja.
        let w_miescie = r_end < b.urban_r * 1.15;
        let kontynuuj_dalej = if matches!(prop.class, RoadClass::Highway) || prop.gate {
            // Trasa wylotowa i droga bramowa jadą do miasta nawet przez pustkę;
            // reszta rośnie tylko w obszarze zurbanizowanym.
            r_end > b.urban_r * 0.2
        } else {
            w_miescie
        };
        // **Trasa wylotowa wchodzi w miasto jako arteria.** Tak to działa w rzeczywistości
        // (droga krajowa staje się aleją) i tylko tak aksjomat z §5.2 rodzi szkielet
        // miasta: przy 15% szansy na zjazd dwie autostrady dają pół arterii na miasto,
        // czyli sieć, która po przycięciu wiszących końców znika w całości.
        // Odnotowane w „Korektach planu".
        let (klasa_dalej, gen_dalej) =
            if matches!(prop.class, RoadClass::Highway) && r_end < b.urban_r {
                (RoadClass::Arterial, 0)
            } else {
                (prop.class, prop.gen + 1)
            };
        if kontynuuj_dalej && kontynuuj && !limit_generacji {
            push(
                &mut props,
                &mut heap,
                &mut seq,
                Proposal {
                    from: node,
                    dir,
                    class: klasa_dalej,
                    gen: gen_dalej,
                    prio: prop.prio + 1,
                    seq: 0,
                    target: None,
                    ring: prop.ring,
                    gate: prop.gate,
                },
            );
        }

        // P0 — **łącznik domykający**. Uogólnienie reguły P4 z arterii na każdą klasę:
        // łańcuch, który się kończy (limit generacji, wyjście poza obszar, dojazd do
        // środka), próbuje trafić w najbliższą istniejącą ulicę zamiast zostać ślepym
        // zaułkiem. Bez tego 35% powstałych segmentów wypadało przy domykaniu sieci,
        // a graf zostawał drzewem — wbrew celowi reguły P4 z §5.2.
        if kontynuuj && (limit_generacji || !kontynuuj_dalej) {
            let sasiedzi: Vec<NodeId> = b
                .segments_at_node(node)
                .iter()
                .map(|&s| {
                    let seg = &b.segments[s as usize];
                    if seg.a == node {
                        seg.b
                    } else {
                        seg.a
                    }
                })
                .chain(std::iter::once(node))
                .collect();
            let (klasa, promien) = if matches!(prop.class, RoadClass::Highway) {
                (RoadClass::Arterial, 700.0)
            } else {
                (prop.class, f32::from(spec.seg_len_m) * 1.5)
            };
            if let Some(t) = b.find_node(p_end, promien, klasa.rank(), &sasiedzi) {
                push(
                    &mut props,
                    &mut heap,
                    &mut seq,
                    Proposal {
                        from: node,
                        dir: (b.nodes[t.0 as usize].pos - p_end).normalize_or_zero(),
                        class: klasa,
                        gen: prop.gen,
                        prio: prop.prio + 1,
                        seq: 0,
                        target: Some(t),
                        ring: false,
                        gate: false,
                    },
                );
            }
        }

        if limit_generacji {
            continue;
        }

        if !w_miescie {
            continue;
        }

        // P2 — rozgałęzienie boczne.
        let side = Vec2::new(-dir.y, dir.x);
        let (dziecko, p_branch, dprio) = match prop.class {
            RoadClass::Highway => (Some(RoadClass::Arterial), 150, 12),
            RoadClass::Arterial => (Some(RoadClass::Collector), 550, 6),
            RoadClass::Collector => (Some(RoadClass::Local), 700, 3),
            // `ponytail:` reguła P2 dopuszcza `Local → Service` tylko w kwartale > 2 ha,
            // a kwartały zna dopiero M2c (WP8). Przybliżenie: od drugiej generacji ulicy
            // lokalnej i poza superblokiem. Sufit: w gęstej starówce powstanie sięgacz,
            // który urbanistycznie należałby do podziału kwartału. Wyjście: przenieść
            // ten wariant do WP8, gdy `max_block_area` będzie już policzone.
            // Bez tego równoległe ulice lokalne nigdy się nie przecinają i miasto
            // wychodzi grzebieniem — widać to na podglądzie natychmiast.
            RoadClass::Local if prop.gen >= 2 => (Some(RoadClass::Service), 200, 1),
            _ => (None, 0, 0),
        };
        // W superbloku nie ma ulic wewnętrznych — to jest cała jego definicja.
        let dziecko = match (dziecko, b.pattern_at(p_end)) {
            (Some(RoadClass::Local), Pattern::Superblock(_)) => None,
            (d, _) => d,
        };
        if let Some(kl) = dziecko {
            let co_czwarty =
                !matches!(prop.class, RoadClass::Highway) || prop.gen.is_multiple_of(4);
            for znak in [1.0_f32, -1.0] {
                if !co_czwarty || !r.gen_bool_permille(p_branch) {
                    continue;
                }
                push(
                    &mut props,
                    &mut heap,
                    &mut seq,
                    Proposal {
                        from: node,
                        dir: side * znak,
                        class: kl,
                        gen: 0,
                        prio: prop.prio + dprio,
                        seq: 0,
                        target: None,
                        ring: false,
                        gate: false,
                    },
                );
            }
        }

        // P3 — rozwidlenie Y (tylko arterie).
        if matches!(prop.class, RoadClass::Arterial) && !prop.ring && r.gen_bool_permille(80) {
            for znak in [1.0_f32, -1.0] {
                let kat = (25 + r.gen_range_u32(16)) as f32 * znak;
                push(
                    &mut props,
                    &mut heap,
                    &mut seq,
                    Proposal {
                        from: node,
                        dir: rotate(dir, kat),
                        class: RoadClass::Arterial,
                        gen: prop.gen + 1,
                        prio: prop.prio + 2,
                        seq: 0,
                        target: None,
                        ring: false,
                        gate: false,
                    },
                );
            }
        }

        // P4 — domknięcie pierścienia: sieć arterii ma być grafem z cyklami, nie drzewem.
        if matches!(prop.class, RoadClass::Arterial) && prop.gen >= 3 {
            let sasiedzi: Vec<NodeId> = b
                .segments_at_node(node)
                .iter()
                .map(|&s| {
                    let seg = &b.segments[s as usize];
                    if seg.a == node {
                        seg.b
                    } else {
                        seg.a
                    }
                })
                .chain(std::iter::once(node))
                .collect();
            let promien = f32::from(spec.seg_len_m) * 1.5;
            // `ponytail:` pełna reguła mówi „węzeł nienależący do przodków"; tu wykluczamy
            // węzeł bieżący i jego bezpośrednich sąsiadów. Sufit: łącznik może zamknąć
            // trójkąt o boku 1,5·seg_len zamiast większego cyklu. Wyjście: pamiętać
            // identyfikator korzenia propozycji, gdyby trójkąty okazały się widoczne.
            if let Some(t) = b.find_node(p_end, promien, RoadClass::Arterial.rank(), &sasiedzi) {
                push(
                    &mut props,
                    &mut heap,
                    &mut seq,
                    Proposal {
                        from: node,
                        dir: (b.nodes[t.0 as usize].pos - p_end).normalize_or_zero(),
                        class: RoadClass::Arterial,
                        gen: prop.gen + 1,
                        prio: prop.prio + 2,
                        seq: 0,
                        target: Some(t),
                        ring: false,
                        gate: false,
                    },
                );
            }
        }
    }

    Grown {
        nodes: b.nodes,
        segments: b.segments,
        geom: b.geom,
        gate_nodes,
        stats: b.stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obrot_o_90_stopni_jest_dokladny() {
        let d = rotate(Vec2::new(1.0, 0.0), 90.0);
        assert!((d.x).abs() < 1e-6, "x = {}", d.x);
        assert!((d.y - 1.0).abs() < 1e-6, "y = {}", d.y);
    }

    #[test]
    fn snapowanie_bierze_najblizsza_os() {
        let d = snap_to_grid(Vec2::new(0.9, 0.1), 0.0);
        assert!((d - Vec2::new(1.0, 0.0)).length() < 1e-5);
        let d = snap_to_grid(Vec2::new(-0.1, -0.9), 0.0);
        assert!((d - Vec2::new(0.0, -1.0)).length() < 1e-5);
    }

    #[test]
    fn przeciecie_zwraca_punkt_na_obu_odcinkach() {
        let (p, t) = segment_intersection(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(5.0, -5.0),
            Vec2::new(5.0, 5.0),
        )
        .expect("odcinki się przecinają");
        assert!((p - Vec2::new(5.0, 0.0)).length() < 1e-5);
        assert!((t - 0.5).abs() < 1e-5);

        // Odcinki równoległe nie mają przecięcia.
        assert!(segment_intersection(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 5.0),
            Vec2::new(10.0, 5.0),
        )
        .is_none());

        // Styk w końcu **jest** zwracany: o tym, czy to skrzyżowanie, czy sąsiedztwo
        // w węźle, rozstrzyga wywołujący progiem metrowym (korekta B13).
        assert!(segment_intersection(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 9.0),
        )
        .is_some());
    }
}
