//! WP5b — kolej towarowa (M2 §5.2, przeniesione z M2b korektą B1).
//!
//! Tory prowadzą od bramy `RailFreight` do klastrów stref `IndustryHeavy` i `Logistics`,
//! więc nie dało się ich wyznaczyć przed Etapem 4 — cel powstaje dopiero w WP7.
//!
//! Zamiast osobnego A* na każdy klaster idzie **jedna Dijkstra od bramy** po siatce 16 m,
//! a trasy powstają przez cofanie się po drzewie poprzedników (korekta C11). Scalanie
//! wspólnych prefiksów wychodzi wtedy za darmo: dwie bocznice do sąsiednich zakładów
//! dzielą odcinek do rozjazdu, bo dzielą ścieżkę w drzewie. Rozjazd to punkt, w którym
//! cofanie trafia na komórkę już zajętą przez wcześniejszy tor.
//!
//! Ograniczenie 2 % nachylenia dotyczy **niwelety**, nie terenu (jak w M2b, korekta B3):
//! profil podłużny jest przycinany do 2 % w dwóch przebiegach, a różnicę wobec gruntu
//! pokrywa nasyp albo wykop.

use super::blocks::BlockSet;
use super::gates::{CityGate, GatePlan, GateSpot};
use super::road::{
    structure_for, NodeFlags, NodeId, PolyArena, RoadClass, RoadFlags, RoadNetwork, RoadNode,
    RoadSegment, RoadStructure, UNASSIGNED_DISTRICT,
};
use super::zoning::{CityFields, ZoneKind, ZoneResult};
use crate::query::TerrainQuery;
use magnat_spatial::{CellId, Vec2};

/// Odległość między węzłami toru w metrach — `seg_len_m` klasy `RailFreight`.
const NODE_SPACING_M: f32 = 500.0;
/// Promień obsługi bocznicy: klaster bliżej niż to od istniejącego toru **nie dostaje
/// własnej odnogi** (kryterium WP5b: „bocznica ≤ 1,2 km od kwartału").
const SIDING_RADIUS_M: f32 = 1_200.0;
/// Dopuszczalne nachylenie niwelety w jednostkach `slope_at` (2 % · 64 / 100 ≈ 1).
const MAX_GRADE_PCT: f32 = 2.0;

#[derive(Clone, PartialEq, Debug, Default)]
pub struct RailReport {
    pub track_km: f64,
    /// Klastry przemysłowe, do których dociągnięto bocznicę.
    pub sidings: u32,
    /// Rozjazdy: miejsca, w których bocznica odchodzi od istniejącego toru.
    pub junctions: u32,
    /// Klastry obsłużone bocznicą, która już istniała (w zasięgu 1,2 km).
    pub covered: u32,
    /// Klastry bez połączenia (teren nie pozwolił).
    pub unreachable: u32,
    /// Najbardziej stromy odcinek toru, w procentach.
    pub max_grade_pct: f32,
}

/// WP5b w całości: tory od bram kolejowych do klastrów przemysłowych.
///
/// Bramy kolejowe siedzą w planie bram, a nie w gotowej sieci — M2b ich nie podłączył,
/// bo nie miały dokąd prowadzić (korekta B1). Tutaj dostają węzeł i tor.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_rail(
    t: &dyn TerrainQuery,
    gp: &GatePlan,
    net: &mut RoadNetwork,
    geom: &mut PolyArena,
    blocks: &BlockSet,
    zones: &ZoneResult,
    f: &CityFields,
    centroid: &[Vec2],
    segments_before: usize,
) -> RailReport {
    let mut rep = RailReport::default();
    let spec = super::road::spec(RoadClass::RailFreight);
    let cele = klastry(blocks, zones, centroid, segments_before, f);
    let bramy: Vec<&GateSpot> = gp.gates.iter().filter(|g| g.kind.is_rail()).collect();
    if bramy.is_empty() || cele.is_empty() {
        rep.unreachable = cele.len() as u32;
        return rep;
    }

    // Jedna Dijkstra na bramę; klaster obsługuje ta brama, która jest do niego bliżej.
    let mut drzewa: Vec<(CellId, Vec<u32>, Vec<u32>)> = Vec::new();
    for g in &bramy {
        let start = f.spec.cell_of(g.node_pos);
        let (dist, prev) = dijkstra(f, t, start);
        drzewa.push((start, dist, prev));
    }

    // Komórka zajęta przez tor — rozpoznanie rozjazdu i scalanie wspólnych prefiksów.
    let mut zajete = vec![false; f.spec.cell_count()];
    // Węzeł sieci przypisany komórce, żeby rozjazd trafiał w istniejący węzeł.
    let mut wezel = vec![u32::MAX; f.spec.cell_count()];

    for (i, g) in bramy.iter().enumerate() {
        let n = dodaj_wezel(net, t, g.node_pos, NodeFlags::GATE);
        let c = drzewa[i].0;
        zajete[c.0 as usize] = true;
        wezel[c.0 as usize] = n.0;
        net.gates.push(CityGate {
            kind: g.kind,
            pos: g.pos,
            dir_inward: g.dir_inward,
            capacity: g.kind.capacity(),
            node: n,
        });
    }

    // Węzły położonego toru — po nich sprawdzamy, czy klaster jest już obsłużony.
    let mut tor: Vec<Vec2> = bramy.iter().map(|g| g.node_pos).collect();

    for cel in cele {
        // Klaster w zasięgu istniejącej bocznicy nie potrzebuje własnej odnogi.
        // Bez tego warunku każda kępa hal dostawała własny tor i metropolia kończyła
        // z 197 km torów wobec 45 km z budżetu M2 §7 (korekta C14).
        let p = f.spec.cell_center(cel);
        if tor.iter().any(|q| (*q - p).length() < SIDING_RADIUS_M) {
            rep.covered += 1;
            continue;
        }

        // Brama o najkrótszej trasie; remis po indeksie bramy.
        let Some((bi, _)) = drzewa
            .iter()
            .enumerate()
            .map(|(i, (_, d, _))| (i, d[cel.0 as usize]))
            .filter(|(_, d)| *d != u32::MAX)
            .min_by_key(|(i, d)| (*d, *i))
        else {
            rep.unreachable += 1;
            continue;
        };
        let prev = &drzewa[bi].2;

        // Cofanie po drzewie do pierwszej komórki już zajętej przez tor.
        let mut sciezka = vec![cel.0];
        let mut c = cel.0;
        loop {
            let p = prev[c as usize];
            if p == u32::MAX {
                break;
            }
            c = p;
            sciezka.push(c);
            if zajete[c as usize] {
                break;
            }
        }
        if sciezka.len() < 2 {
            rep.unreachable += 1;
            continue;
        }
        let rozjazd = zajete[c as usize] && c != drzewa[bi].0 .0;
        sciezka.reverse(); // od strony sieci ku zakładowi

        let (dodane, dlugosc, grade) =
            poloz_tor(t, net, geom, f, &sciezka, &mut zajete, &mut wezel, &spec);
        if dodane == 0 {
            rep.unreachable += 1;
            continue;
        }
        for &c in &sciezka {
            tor.push(f.spec.cell_center(CellId(c)));
        }
        rep.sidings += 1;
        rep.track_km += dlugosc / 1000.0;
        rep.max_grade_pct = rep.max_grade_pct.max(grade);
        if rozjazd {
            rep.junctions += 1;
        }
    }
    rep
}

/// Docelowe komórki: po jednej na spójny klaster stref przemysłowych i logistycznych.
/// Cel klastra to centroid kwartału o najniższym `BlockId` — deterministycznie,
/// a klastry są zwarte, więc to i tak jest blisko środka.
fn klastry(
    blocks: &BlockSet,
    zones: &ZoneResult,
    centroid: &[Vec2],
    segments: usize,
    f: &CityFields,
) -> Vec<CellId> {
    let przemysl = |z: ZoneKind| {
        matches!(
            z,
            ZoneKind::IndustryHeavy | ZoneKind::IndustryLight | ZoneKind::Logistics
        )
    };
    let n = blocks.blocks.len();
    let (start, items) = super::blocks::block_adjacency(blocks, segments);
    let mut seen = vec![false; n];
    let mut out: Vec<(f64, CellId)> = Vec::new();
    for s in 0..n {
        if seen[s] || !przemysl(zones.zone[s]) {
            continue;
        }
        let mut stos = vec![s];
        seen[s] = true;
        let mut min_id = s;
        let mut pole = 0.0f64;
        while let Some(v) = stos.pop() {
            min_id = min_id.min(v);
            pole += f64::from(blocks.blocks[v].area_m2);
            for k in start[v]..start[v + 1] {
                let j = items[k as usize] as usize;
                if !seen[j] && przemysl(zones.zone[j]) {
                    seen[j] = true;
                    stos.push(j);
                }
            }
        }
        out.push((pole, f.spec.cell_of(centroid[min_id])));
    }
    // Od największego klastra: magistrala powstaje pierwsza, a kępy hal doczepiają się
    // do niej rozjazdem albo mieszczą się w promieniu obsługi.
    out.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1 .0.cmp(&b.1 .0)));
    out.into_iter().map(|(_, c)| c).collect()
}

/// Dijkstra po siatce pól: koszt rośnie z nachyleniem i wodą, maleje na istniejącym torze.
fn dijkstra(f: &CityFields, t: &dyn TerrainQuery, start: CellId) -> (Vec<u32>, Vec<u32>) {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let spec = f.spec;
    let n = spec.cell_count();
    let (cols, rows) = (i64::from(spec.cols), i64::from(spec.rows));
    let mut dist = vec![u32::MAX; n];
    let mut prev = vec![u32::MAX; n];
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    dist[start.0 as usize] = 0;
    heap.push(Reverse((0, start.0)));

    while let Some(Reverse((d, c))) = heap.pop() {
        if d > dist[c as usize] {
            continue;
        }
        let (x, y) = (i64::from(c) % cols, i64::from(c) / cols);
        for (dx, dy) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= cols || ny >= rows {
                continue;
            }
            let nc = (ny * cols + nx) as u32;
            let p = spec.cell_center(CellId(nc));
            let m = if dx != 0 && dy != 0 { 23u32 } else { 16 };
            // Nachylenie terenu: kolej go nie lubi, ale nie jest zakazane — profil
            // przycinany jest później, a różnicę pokrywa nasyp.
            let slope = u32::from(t.slope_at(p.x as i32, p.y as i32));
            let mut w = m * (4 + slope.min(40));
            if f.blocked[nc as usize] {
                w *= 6; // most
            }
            if f.road_rank[nc as usize] > 0 {
                w += m * 8; // przecięcie z ulicą kosztuje wiadukt albo przejazd
            }
            let nd = d.saturating_add(w);
            if nd < dist[nc as usize] {
                dist[nc as usize] = nd;
                prev[nc as usize] = c;
                heap.push(Reverse((nd, nc)));
            }
        }
    }
    (dist, prev)
}

/// Zamiana ścieżki komórek na segmenty toru: węzły co ~500 m, profil przycięty do 2 %.
#[allow(clippy::too_many_arguments)]
fn poloz_tor(
    t: &dyn TerrainQuery,
    net: &mut RoadNetwork,
    geom: &mut PolyArena,
    f: &CityFields,
    sciezka: &[u32],
    zajete: &mut [bool],
    wezel: &mut [u32],
    spec: &super::road::ClassSpec,
) -> (u32, f64, f32) {
    let krok = (NODE_SPACING_M / f.spec.cell_size()).max(1.0) as usize;
    let mut punkty: Vec<(u32, Vec2)> = Vec::new();
    for (i, &c) in sciezka.iter().enumerate() {
        if i == 0 || i + 1 == sciezka.len() || i % krok == 0 {
            punkty.push((c, f.spec.cell_center(CellId(c))));
        }
    }
    if punkty.len() < 2 {
        return (0, 0.0, 0.0);
    }

    // Profil: rzędna terenu przycięta do 2 % w obie strony (dwa przebiegi).
    let mut z: Vec<f32> = punkty
        .iter()
        .map(|(_, p)| t.height_at(p.x as i32, p.y as i32) as f32 * 5.0)
        .collect();
    for _ in 0..2 {
        for i in 1..z.len() {
            let d = (punkty[i].1 - punkty[i - 1].1).length() * 10.0;
            let limit = d * MAX_GRADE_PCT / 100.0;
            z[i] = z[i].clamp(z[i - 1] - limit, z[i - 1] + limit);
        }
        for i in (0..z.len() - 1).rev() {
            let d = (punkty[i + 1].1 - punkty[i].1).length() * 10.0;
            let limit = d * MAX_GRADE_PCT / 100.0;
            z[i] = z[i].clamp(z[i + 1] - limit, z[i + 1] + limit);
        }
    }

    let mut wezly: Vec<NodeId> = Vec::with_capacity(punkty.len());
    for (i, (c, p)) in punkty.iter().enumerate() {
        let n = if wezel[*c as usize] != u32::MAX {
            NodeId(wezel[*c as usize])
        } else {
            let n = NodeId(net.nodes.len() as u32);
            net.nodes.push(RoadNode {
                pos: *p,
                z_dm: z[i] as i32,
                degree: 0,
                flags: NodeFlags::NONE,
            });
            wezel[*c as usize] = n.0;
            n
        };
        wezly.push(n);
    }
    for &c in sciezka {
        zajete[c as usize] = true;
    }

    let mut dodane = 0u32;
    let mut dlugosc = 0.0f64;
    let mut max_grade = 0.0f32;
    for i in 1..wezly.len() {
        let (a, b) = (wezly[i - 1], wezly[i]);
        if a == b {
            continue;
        }
        let (pa, pb) = (net.nodes[a.0 as usize].pos, net.nodes[b.0 as usize].pos);
        let len = (pb - pa).length();
        if len < 1.0 {
            continue;
        }
        let dz = (z[i] - z[i - 1]).abs();
        max_grade = max_grade.max(dz / 10.0 / len * 100.0);
        let structure = structure_for(t, pa, pb, spec).unwrap_or(RoadStructure::AtGrade);
        net.segments.push(RoadSegment {
            a,
            b,
            class: RoadClass::RailFreight,
            geom: geom.push(&[pa, pb]),
            structure,
            lanes_fwd: spec.lanes_fwd,
            lanes_bwd: spec.lanes_bwd,
            row_m: spec.row_m,
            speed_kph: spec.speed_kph,
            max_tonnage_t: spec.max_tonnage_t,
            length_dm: (len * 10.0) as u32,
            district: UNASSIGNED_DISTRICT,
            flags: RoadFlags::RAIL.with(RoadFlags::GRADE_SEPARATED),
        });
        net.nodes[a.0 as usize].degree = net.nodes[a.0 as usize].degree.saturating_add(1);
        net.nodes[b.0 as usize].degree = net.nodes[b.0 as usize].degree.saturating_add(1);
        dodane += 1;
        dlugosc += f64::from(len);
    }
    (dodane, dlugosc, max_grade)
}

fn dodaj_wezel(net: &mut RoadNetwork, t: &dyn TerrainQuery, p: Vec2, flags: NodeFlags) -> NodeId {
    let n = NodeId(net.nodes.len() as u32);
    net.nodes.push(RoadNode {
        pos: p,
        z_dm: t.height_at(p.x as i32, p.y as i32) * 5,
        degree: 0,
        flags,
    });
    n
}
