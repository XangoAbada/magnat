//! Generator miasta (faza M2). Podfaza **M2b** dostarcza Etap 3: bramy, szkielet
//! transportu i kwartały wyznaczone z powstałego grafu.
//!
//! Wejściem jest `CityPlan` i `TerrainQuery` (**K-13** — teren wyłącznie przez kontrakt
//! M1, nigdy przez surowe dane). Wyjściem `CityData`: sieć dróg, kwartały i raport.
//! Strefy, parcele, dzielnice, zabudowa i gospodarka bazowa dokładają kolejne podfazy.

pub mod blocks;
pub mod gates;
pub mod lsystem;
pub mod pattern;
pub mod road;

use crate::assets::data_path;
use crate::params::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};
use crate::query::TerrainQuery;
use blocks::BlockSet;
use gates::{CityGate, GateKind, GateProfile};
use magnat_core::StateHash;
use magnat_spatial::Vec2;
use pattern::RingTable;
use road::{
    FurnitureKind, NodeFlags, NodeId, PolyArena, RoadClass, RoadFlags, RoadNetwork, RoadNode,
    RoadSegment, SegmentId, StreetFurniture,
};

/// Wejście generatora miasta (PRD §4.1). Wszystko, czego trzeba, żeby z samego ziarna
/// odtworzyć miasto — i nic ponadto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CityPlan {
    pub seed: u64,
    pub size: WorldSize,
    pub region: Region,
    pub epoch: Epoch,
    pub profile: EconomyProfile,
    pub difficulty: Difficulty,
    /// Docelowa liczba mieszkańców. Nie jest parametrem wejściowym gracza — wynika
    /// z rozmiaru mapy (PRD §4.1) — ale jest **jedyną** wielkością, z której generator
    /// liczy budżety, więc stoi w planie jawnie, a nie w trzech miejscach osobno.
    pub target_pop: u32,
}

/// Gęstość zaludnienia obszaru zurbanizowanego, os./km².
/// Z budżetów M2 §7: 400 tys. mieszkańców na ~70 km².
const URBAN_DENSITY_PER_KM2: f32 = 5_714.0;
/// Segmentów drogowych na mieszkańca⁻¹. Z budżetów M2 §7: ~16 tys. segmentów na 400 tys.
const POP_PER_SEGMENT: u32 = 25;

impl CityPlan {
    #[must_use]
    pub fn from_world(p: &WorldGenParams) -> CityPlan {
        CityPlan {
            seed: p.seed,
            size: p.size,
            region: p.region,
            epoch: p.epoch,
            profile: p.profile,
            difficulty: p.difficulty,
            target_pop: target_pop(p.size),
        }
    }

    #[must_use]
    pub fn map_size_m(&self) -> i32 {
        self.size.meters() as i32
    }

    /// Promień obszaru zurbanizowanego. Przycięty do mapy — miasto ma się na niej
    /// zmieścić, nawet jeśli ludności starczyłoby na większe.
    #[must_use]
    pub fn urban_radius_m(&self) -> f32 {
        let area_km2 = self.target_pop as f32 / URBAN_DENSITY_PER_KM2;
        let r =
            magnat_core::det_math::sqrt(f64::from(area_km2) / std::f64::consts::PI) as f32 * 1000.0;
        r.min(self.map_size_m() as f32 * 0.4)
    }

    /// Twardy budżet segmentów — zabezpieczenie przed rozbieganiem się L-systemu (R1).
    #[must_use]
    pub fn segment_budget(&self) -> usize {
        (self.target_pop / POP_PER_SEGMENT) as usize
    }

    fn profile_path(&self) -> String {
        format!("zoning/profile_{}.ron", self.profile.key())
    }
}

#[must_use]
pub const fn target_pop(size: WorldSize) -> u32 {
    match size {
        WorldSize::Small4km => 40_000,
        WorldSize::Medium8km => 120_000,
        WorldSize::Large12km => 250_000,
        WorldSize::Metropolis16km => 400_000,
    }
}

/// Raport generacji miasta (M2 §1, artefakt 1). M2b wypełnia część transportową;
/// kolejne podfazy dokładają swoje sekcje do tej samej struktury.
#[derive(Clone, PartialEq, Debug)]
pub struct GenerationReport {
    pub center: Vec2,
    pub gates: Vec<(GateKind, Vec2)>,
    pub missing_gates: Vec<GateKind>,
    /// Bramy postawione, ale bez połączenia z siecią w tej podfazie: kolejowe czekają
    /// na tory z M2c. Wypisywane, bo brama, o której raport milczy, znika bez śladu.
    pub deferred_gates: Vec<(GateKind, Vec2)>,
    pub nodes: u32,
    pub segments: u32,
    pub road_km: f64,
    pub bridges: u32,
    pub tunnels: u32,
    pub embankments: u32,
    pub blocks: u32,
    pub dropped_faces: u32,
    pub swallowed_faces: u32,
    pub faces_debug: String,
    pub urban_area_km2: f64,
    pub road_area_km2: f64,
    pub stats: lsystem::LStats,
    pub stage_millis: Vec<(&'static str, f64)>,
    pub road_hash: StateHash,
    /// Ostrzeżenia — R1 fazy: generator, który odrzuca ponad 40% propozycji,
    /// buduje co innego, niż planowano, i ma o tym powiedzieć.
    pub warnings: Vec<String>,
}

impl GenerationReport {
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![
            format!("centrum: {:.0}, {:.0}", self.center.x, self.center.y),
            format!(
                "bramy: {}",
                self.gates
                    .iter()
                    .map(|(k, p)| format!("{}@{:.0},{:.0}", k.key(), p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            format!(
                "sieć: {} węzłów, {} segmentów, {:.1} km",
                self.nodes, self.segments, self.road_km
            ),
            format!(
                "struktury: {} mostów, {} tuneli, {} nasypów",
                self.bridges, self.tunnels, self.embankments
            ),
            format!(
                "kwartały: {} · obszar {:.2} km² · pas drogowy {:.2} km²",
                self.blocks, self.urban_area_km2, self.road_area_km2
            ),
            format!(
                "propozycje: {} · przyjęte {} · odrzucone {}% (nachylenie {}, przeprawa {}, kąt {}, strefa {})",
                self.stats.proposals,
                self.stats.accepted,
                self.stats.rejection_pct(),
                self.stats.rejected_slope,
                self.stats.rejected_crossing,
                self.stats.rejected_angle,
                self.stats.rejected_zone,
            ),
            format!(
                "scalenia {} · podziały {} · przycięte {} · kwartały odrzucone {} / pochłonięte {}",
                self.stats.snapped, self.stats.split, self.stats.pruned,
                self.dropped_faces, self.swallowed_faces,
            ),
            self.faces_debug.clone(),
            format!("hash sieci: {:032x}", self.road_hash.0),
        ];
        for (n, ms) in &self.stage_millis {
            v.push(format!("  {n}: {ms:.1} ms"));
        }
        if !self.deferred_gates.is_empty() {
            v.push(format!(
                "bramy odłożone do M2c (tory): {}",
                self.deferred_gates
                    .iter()
                    .map(|(k, p)| format!("{}@{:.0},{:.0}", k.key(), p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        if !self.missing_gates.is_empty() {
            v.push(format!(
                "BRAK BRAM WYMAGANYCH: {}",
                self.missing_gates
                    .iter()
                    .map(|k| k.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for w in &self.warnings {
            v.push(format!("OSTRZEŻENIE: {w}"));
        }
        v
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct CityData {
    pub plan: CityPlan,
    pub center: Vec2,
    pub roads: RoadNetwork,
    pub blocks: BlockSet,
    pub report: GenerationReport,
}

#[derive(Debug)]
pub enum CityGenError {
    Profile(String),
    Rings(pattern::RingError),
}

impl std::fmt::Display for CityGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CityGenError::Profile(s) => write!(f, "{s}"),
            CityGenError::Rings(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CityGenError {}

pub const PROFILE_SCHEMA_VERSION: u32 = 1;

fn load_profile(plan: &CityPlan) -> Result<GateProfile, CityGenError> {
    let path = data_path(&plan.profile_path());
    let txt = std::fs::read_to_string(&path)
        .map_err(|e| CityGenError::Profile(format!("{}: {e}", path.display())))?;
    let p: GateProfile = ron::from_str(&txt)
        .map_err(|e| CityGenError::Profile(format!("{}: {e}", path.display())))?;
    if p.schema_version != PROFILE_SCHEMA_VERSION {
        return Err(CityGenError::Profile(format!(
            "{}: schema_version {}, oczekiwano {PROFILE_SCHEMA_VERSION}",
            path.display(),
            p.schema_version
        )));
    }
    Ok(p)
}

/// Etap 3 generacji miasta w całości (M2b).
pub fn generate_city(plan: &CityPlan, t: &dyn TerrainQuery) -> Result<CityData, CityGenError> {
    let profile = load_profile(plan)?;
    let rings = RingTable::load().map_err(CityGenError::Rings)?;
    let mut stage = Vec::new();
    let mut zegar = std::time::Instant::now();
    let mut tik = |stage: &mut Vec<(&'static str, f64)>, n: &'static str| {
        stage.push((n, zegar.elapsed().as_secs_f64() * 1000.0));
        zegar = std::time::Instant::now();
    };

    let gp = gates::place_gates(plan, t, &profile);
    tik(&mut stage, "bramy");

    let grown = lsystem::grow_network(plan, t, &gp, &rings);
    tik(&mut stage, "l-system");

    let (mut roads, stats) = finalize(grown, &gp, t);
    tik(&mut stage, "domknięcie sieci");

    // Arena wyjęta na czas budowy kwartałów: `build_blocks` czyta sieć i **dopisuje**
    // do areny obrysy, a to dwa różne pożyczenia tej samej struktury.
    let mut geom = std::mem::take(&mut roads.geom);
    let blocks = blocks::build_blocks(&roads, &mut geom);
    roads.geom = geom;
    tik(&mut stage, "kwartały");

    let mut warnings = Vec::new();
    if stats.rejection_pct() > 40 {
        warnings.push(format!(
            "L-system odrzucił {}% propozycji (R1: teren wymusza inny układ niż wzorzec)",
            stats.rejection_pct()
        ));
    }
    for g in gp.gates.iter().filter(|g| !g.kind.is_rail()) {
        if !roads
            .gates
            .iter()
            .any(|x| x.kind == g.kind && x.pos == g.pos)
        {
            warnings.push(format!(
                "brama {} przy {:.0},{:.0} nie połączyła się z siecią i wypadła przy domykaniu",
                g.kind.key(),
                g.pos.x,
                g.pos.y
            ));
        }
    }
    if blocks.blocks.is_empty() {
        warnings.push("graf dróg nie zamknął ani jednego kwartału".to_string());
    }

    let report = GenerationReport {
        center: gp.center,
        gates: roads.gates.iter().map(|g| (g.kind, g.pos)).collect(),
        missing_gates: gp.missing.clone(),
        deferred_gates: gp
            .gates
            .iter()
            .filter(|g| g.kind.is_rail())
            .map(|g| (g.kind, g.pos))
            .collect(),
        nodes: roads.nodes.len() as u32,
        segments: roads.segments.len() as u32,
        road_km: roads.length_m(|s| s.class.is_driveable()) / 1000.0,
        bridges: policz(&roads, |s| {
            matches!(s.structure, road::RoadStructure::Bridge { .. })
        }),
        tunnels: policz(&roads, |s| {
            matches!(s.structure, road::RoadStructure::Tunnel { .. })
        }),
        embankments: policz(&roads, |s| {
            matches!(s.structure, road::RoadStructure::Embankment { .. })
        }),
        blocks: blocks.blocks.len() as u32,
        dropped_faces: blocks.dropped,
        swallowed_faces: blocks.swallowed,
        faces_debug: format!(
            "orbity {} · zewnętrzne {} · zdegenerowane {} · wiadukty {}",
            blocks.orbits, blocks.outer, blocks.degenerate, blocks.excluded_edges
        ),
        urban_area_km2: blocks.urban_area_m2 / 1e6,
        road_area_km2: blocks.road_area_m2 / 1e6,
        stats,
        stage_millis: stage,
        road_hash: roads.hash(),
        warnings,
    };

    Ok(CityData {
        plan: *plan,
        center: gp.center,
        roads,
        blocks,
        report,
    })
}

/// Liczba segmentów spełniających warunek — raport mówi o **gotowej** sieci,
/// a nie o tym, co powstało przed przycięciem wiszących końców.
fn policz(net: &RoadNetwork, f: impl Fn(&RoadSegment) -> bool) -> u32 {
    net.segments.iter().filter(|s| f(s)).count() as u32
}

/// Domknięcie sieci: przycięcie wiszących końców, wybór głównej składowej, kompaktowanie
/// tablic, CSR sąsiedztwa i mała architektura.
///
/// **Przycinamy wszystkie** wiszące końce, nie tylko klasy ≥ Collector z kryterium WP4.
/// Powód jest geometryczny: wisząca krawędź wewnątrz ściany grafu robi w niej szczelinę
/// o zerowej szerokości, której odsunięcie kwartału (WP6) nie ma jak obsłużyć. Sięgacze
/// (`cul-de-sac`) wracają w M2c, gdzie powstają świadomie przy podziale kwartału.
fn finalize(
    grown: lsystem::Grown,
    gp: &gates::GatePlan,
    t: &dyn TerrainQuery,
) -> (RoadNetwork, lsystem::LStats) {
    let lsystem::Grown {
        nodes,
        segments,
        geom,
        gate_nodes,
        mut stats,
    } = grown;

    let mut alive = vec![true; segments.len()];
    let is_gate =
        |n: NodeId, nodes: &[RoadNode]| nodes[n.0 as usize].flags.contains(NodeFlags::GATE);

    // 1. Przycinanie wiszących końców — do punktu stałego.
    loop {
        let mut deg = vec![0u32; nodes.len()];
        for (i, s) in segments.iter().enumerate() {
            if alive[i] {
                deg[s.a.0 as usize] += 1;
                deg[s.b.0 as usize] += 1;
            }
        }
        let mut zmiana = false;
        for (i, s) in segments.iter().enumerate() {
            if !alive[i] {
                continue;
            }
            let a_leaf = deg[s.a.0 as usize] == 1 && !is_gate(s.a, &nodes);
            let b_leaf = deg[s.b.0 as usize] == 1 && !is_gate(s.b, &nodes);
            if a_leaf || b_leaf {
                alive[i] = false;
                stats.pruned += 1;
                zmiana = true;
            }
        }
        if !zmiana {
            break;
        }
    }

    // 2. Największa składowa spójna — reszta to wyspy, do których nie da się dojechać.
    let mut comp = vec![u32::MAX; nodes.len()];
    let mut sizes: Vec<u32> = Vec::new();
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); nodes.len()];
    for (i, s) in segments.iter().enumerate() {
        if alive[i] {
            adj[s.a.0 as usize].push(i as u32);
            adj[s.b.0 as usize].push(i as u32);
        }
    }
    let mut stos = Vec::new();
    for start in 0..nodes.len() {
        if comp[start] != u32::MAX || adj[start].is_empty() {
            continue;
        }
        let id = sizes.len() as u32;
        let mut n = 0u32;
        comp[start] = id;
        stos.push(start);
        while let Some(v) = stos.pop() {
            n += 1;
            for &si in &adj[v] {
                let s = &segments[si as usize];
                let o = if s.a.0 as usize == v {
                    s.b.0 as usize
                } else {
                    s.a.0 as usize
                };
                if comp[o] == u32::MAX {
                    comp[o] = id;
                    stos.push(o);
                }
            }
        }
        sizes.push(n);
    }
    let glowna = sizes
        .iter()
        .enumerate()
        .max_by_key(|(i, n)| (**n, std::cmp::Reverse(*i)))
        .map_or(u32::MAX, |(i, _)| i as u32);
    for (i, s) in segments.iter().enumerate() {
        if alive[i] && comp[s.a.0 as usize] != glowna {
            alive[i] = false;
            stats.pruned += 1;
        }
    }

    // 3. Kompaktowanie: nowe indeksy węzłów i segmentów, świeża arena geometrii.
    let mut node_map = vec![u32::MAX; nodes.len()];
    let mut new_nodes: Vec<RoadNode> = Vec::new();
    let mut new_segments: Vec<RoadSegment> = Vec::new();
    let mut new_geom = PolyArena::new();
    for (i, s) in segments.iter().enumerate() {
        if !alive[i] {
            continue;
        }
        let mut przepisz = |n: NodeId, new_nodes: &mut Vec<RoadNode>| -> NodeId {
            let slot = &mut node_map[n.0 as usize];
            if *slot == u32::MAX {
                *slot = new_nodes.len() as u32;
                let mut nn = nodes[n.0 as usize];
                nn.degree = 0;
                nn.flags = NodeFlags(nn.flags.0 & NodeFlags::GATE.0);
                new_nodes.push(nn);
            }
            NodeId(*slot)
        };
        let a = przepisz(s.a, &mut new_nodes);
        let b = przepisz(s.b, &mut new_nodes);
        let mut ns = *s;
        ns.a = a;
        ns.b = b;
        ns.geom = new_geom.push(geom.get(s.geom));
        new_nodes[a.0 as usize].degree = new_nodes[a.0 as usize].degree.saturating_add(1);
        new_nodes[b.0 as usize].degree = new_nodes[b.0 as usize].degree.saturating_add(1);
        new_segments.push(ns);
    }

    // 4. Flagi węzłów i CSR sąsiedztwa.
    for n in &mut new_nodes {
        if n.degree >= 3 {
            n.flags = n.flags.with(NodeFlags::JUNCTION);
        } else if n.degree == 1 {
            n.flags = n.flags.with(NodeFlags::DEAD_END);
        }
    }
    let mut adj_start = vec![0u32; new_nodes.len() + 1];
    for s in &new_segments {
        adj_start[s.a.0 as usize + 1] += 1;
        adj_start[s.b.0 as usize + 1] += 1;
    }
    for i in 1..adj_start.len() {
        adj_start[i] += adj_start[i - 1];
    }
    let mut kursor = adj_start.clone();
    let mut adj_items = vec![SegmentId(0); adj_start[new_nodes.len()] as usize];
    for (i, s) in new_segments.iter().enumerate() {
        for end in [s.a, s.b] {
            let k = &mut kursor[end.0 as usize];
            adj_items[*k as usize] = SegmentId(i as u32);
            *k += 1;
        }
    }

    // 5. Bramy: uchwyt do węzła po przenumerowaniu. Brama, której węzeł nie przetrwał
    //    domknięcia, wypada z sieci — i widać to w raporcie, bo lista jest krótsza.
    let mut city_gates = Vec::new();
    for (g, n) in gp.gates.iter().zip(gate_nodes.iter()) {
        let Some(n) = n else { continue };
        let new = node_map[n.0 as usize];
        if new == u32::MAX {
            continue;
        }
        city_gates.push(CityGate {
            kind: g.kind,
            pos: g.pos,
            dir_inward: g.dir_inward,
            capacity: g.kind.capacity(),
            node: NodeId(new),
        });
    }

    let furniture = place_furniture(&new_nodes, &new_segments, t);

    (
        RoadNetwork {
            nodes: new_nodes,
            segments: new_segments,
            adj_start,
            adj_items,
            geom: new_geom,
            gates: city_gates,
            furniture,
        },
        stats,
    )
}

/// Latarnie wzdłuż osi, naprzemiennie po obu stronach — wejście dla M11 (M2 §6).
fn place_furniture(
    nodes: &[RoadNode],
    segments: &[RoadSegment],
    t: &dyn TerrainQuery,
) -> Vec<StreetFurniture> {
    let mut out = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        let spacing = f32::from(s.class.spec().lamp_spacing_m);
        if spacing <= 0.0 || !matches!(s.structure, road::RoadStructure::AtGrade) {
            continue;
        }
        let (a, b) = (nodes[s.a.0 as usize].pos, nodes[s.b.0 as usize].pos);
        let len = (b - a).length();
        let d = (b - a).normalize_or_zero();
        let bok = Vec2::new(-d.y, d.x) * (f32::from(s.row_m) * 0.5 - 1.5);
        let n = (len / spacing) as u32;
        for k in 0..n {
            let t_param = (k as f32 + 0.5) * spacing / len;
            let strona = if k % 2 == 0 { 1.0 } else { -1.0 };
            let p = a + (b - a) * t_param + bok * strona;
            // `height_at` jest w jednostkach 0,5 m (K-13).
            let z = t.height_at(p.x as i32, p.y as i32) as f32 * 0.5;
            out.push(StreetFurniture {
                seg: SegmentId(i as u32),
                t: t_param,
                pos: glam::Vec3::new(p.x, p.y, z),
                kind: FurnitureKind::StreetLamp,
            });
        }
    }
    out
}

/// Sanity-check klas dróg w jednym miejscu — używany przez testy spójności.
#[must_use]
pub fn dangling_high_class(net: &RoadNetwork) -> Vec<NodeId> {
    net.nodes
        .iter()
        .enumerate()
        .filter(|(i, n)| {
            n.degree == 1
                && !n.flags.contains(NodeFlags::GATE)
                && net
                    .segments_at(NodeId(*i as u32))
                    .iter()
                    .any(|s| net.segments[s.0 as usize].class.rank() >= RoadClass::Collector.rank())
        })
        .map(|(i, _)| NodeId(i as u32))
        .collect()
}

/// Czy sieć jezdna jest jedną składową spójną (wstęp do testu T1 z M2 §7).
#[must_use]
pub fn is_connected(net: &RoadNetwork) -> bool {
    let start = net
        .segments
        .iter()
        .position(|s| s.class.is_driveable() && !s.flags.contains(RoadFlags::RAIL));
    let Some(s0) = start else { return true };
    let mut seen = vec![false; net.nodes.len()];
    let mut stos = vec![net.segments[s0].a];
    seen[net.segments[s0].a.0 as usize] = true;
    let mut n = 1;
    while let Some(v) = stos.pop() {
        for &s in net.segments_at(v) {
            let o = net.other_end(s, v);
            if !seen[o.0 as usize] {
                seen[o.0 as usize] = true;
                n += 1;
                stos.push(o);
            }
        }
    }
    n == net.nodes.len()
}
