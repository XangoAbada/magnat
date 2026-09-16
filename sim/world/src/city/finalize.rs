//! Domknięcie sieci drogowej po L-systemie i mała architektura wzdłuż osi.
//!
//! Wydzielone z `city/mod.rs` w R-WP9 bez zmiany zachowania: to jest algorytm,
//! nie orkiestracja, a zasada pakietu brzmi „`mod.rs` deklaruje i orkiestruje,
//! nie implementuje".

use super::*;

/// Domknięcie sieci: przycięcie wiszących końców, wybór głównej składowej, kompaktowanie
/// tablic, CSR sąsiedztwa i mała architektura.
///
/// **Przycinamy wszystkie** wiszące końce, nie tylko klasy ≥ Collector z kryterium WP4.
/// Powód jest geometryczny: wisząca krawędź wewnątrz ściany grafu robi w niej szczelinę
/// o zerowej szerokości, której odsunięcie kwartału (WP6) nie ma jak obsłużyć. Sięgacze
/// (`cul-de-sac`) wracają w M2c, gdzie powstają świadomie przy podziale kwartału.
pub(super) fn finalize(
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
        let spacing = f32::from(road::spec(s.class).lamp_spacing_m);
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
