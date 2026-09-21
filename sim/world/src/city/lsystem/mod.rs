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

use super::gates::{AirportFootprint, GatePlan};
use super::pattern::{Pattern, RingTable};
use super::road::{
    ClassSpec, NodeFlags, PolyArena, RoadClass, RoadFlags, RoadNode, RoadSegment, RoadStructure,
    SegmentId, UNASSIGNED_DISTRICT,
};
use super::{road::NodeId, CityPlan};
use crate::query::TerrainQuery;
use magnat_core::{det_math, rng, Rng, StreamId, Tick};
use magnat_spatial::Vec2;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

mod constrain;
mod rules;

/// Bok komórki indeksu roboczego L-systemu, w metrach.
///
/// Własny indeks przyrostowy, a nie `CsrGrid` z `engine/spatial`: tamten jest z założenia
/// statyczny („budowany raz, tylko do odczytu", M2a §5.1), a tu co segment dokładamy
/// węzeł i krawędź. Przebudowa CSR przy każdym z 16 tys. segmentów byłaby O(n²).
const BUCKET_M: f32 = 64.0;

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
    pub(super) fn find_node(
        &self,
        p: Vec2,
        r: f32,
        min_rank: u8,
        exclude: &[NodeId],
    ) -> Option<NodeId> {
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
        let mut r = rng(plan.seed, StreamId::RoadsL, prop.seq, Tick(1));
        let krok = rules::Krok::nowy(&b, prop, node, dir, kontynuuj, gp.center);

        // Kolejność wołania reguł **jest** kolejnością wpychania do kopca, a ta
        // wchodzi w `seq`, czyli w klucz porządku totalnego, czyli w hash miasta.
        // Zamiana dwóch wierszy niżej miejscami przestawia każde miasto z każdego
        // ziarna — i dlatego wyglądają tak, a nie jak pętla po tablicy reguł.
        rules::p1_kontynuacja(&krok, &mut |p| push(&mut props, &mut heap, &mut seq, p));

        // Scalenie z istniejącym węzłem wygasza **kontynuację** (P1), ale nie odcina
        // rozgałęzień: skrzyżowanie jest dokładnie tym miejscem, w którym odchodzą
        // ulice niższej klasy. Przy gaszeniu całej produkcji sieć kończyła wzrost
        // przy 1209 segmentach na budżecie 1600 i zostawała drzewem.
        rules::p0_lacznik(&b, &krok, &mut |p| push(&mut props, &mut heap, &mut seq, p));

        if krok.limit_generacji {
            continue;
        }
        if !krok.w_miescie {
            continue;
        }

        rules::p2_odgalezienie(&b, &krok, &mut r, &mut |p| {
            push(&mut props, &mut heap, &mut seq, p);
        });
        rules::p3_rozwidlenie(&krok, &mut r, &mut |p| {
            push(&mut props, &mut heap, &mut seq, p);
        });
        rules::p4_pierscien(&b, &krok, &mut |p| push(&mut props, &mut heap, &mut seq, p));
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
}
