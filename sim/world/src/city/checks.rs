//! Kontrole spójności sieci drogowej — czysta teoria grafów na `RoadNetwork`.
//!
//! Wydzielone z `city/mod.rs` w R-WP9 bez zmiany zachowania.

use super::*;

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

/// Składowe spójne sieci jezdnej: ile ich jest, ile segmentów ma największa i ile
/// jest segmentów jezdnych razem. Diagnostyka T1 — sama odpowiedź „nie jest spójna"
/// nie mówi, czy odpadł kwartał, czy pojedynczy ślepy zaułek.
#[must_use]
pub fn skladowe_jezdne(net: &RoadNetwork) -> (u32, u32, u32) {
    let jezdny = |s: &RoadSegment| s.class.is_driveable() && !s.flags.contains(RoadFlags::RAIL);
    let ile_jezdnych = net.segments.iter().filter(|s| jezdny(s)).count() as u32;
    let mut seen = vec![false; net.nodes.len()];
    let mut skladowych = 0u32;
    let mut najwieksza = 0u32;
    for start in 0..net.nodes.len() {
        if seen[start]
            || !net
                .segments_at(NodeId(start as u32))
                .iter()
                .any(|s| jezdny(&net.segments[s.0 as usize]))
        {
            continue;
        }
        skladowych += 1;
        let mut n = 0u32;
        let mut stos = vec![NodeId(start as u32)];
        seen[start] = true;
        while let Some(v) = stos.pop() {
            for &s in net.segments_at(v) {
                if !jezdny(&net.segments[s.0 as usize]) {
                    continue;
                }
                n += 1;
                let o = net.other_end(s, v);
                if !seen[o.0 as usize] {
                    seen[o.0 as usize] = true;
                    stos.push(o);
                }
            }
        }
        najwieksza = najwieksza.max(n / 2);
    }
    (skladowych, najwieksza, ile_jezdnych)
}

/// Czy sieć jezdna jest jedną składową spójną (wstęp do testu T1 z M2 §7).
///
/// Liczone po węzłach **dotkniętych przez drogi jezdne**: od M2c w sieci siedzą też
/// tory, a te są osobną składową z definicji — pociąg nie skręca w ulicę.
#[must_use]
pub fn is_connected(net: &RoadNetwork) -> bool {
    let jezdny = |s: &RoadSegment| s.class.is_driveable() && !s.flags.contains(RoadFlags::RAIL);
    let mut w_sieci = vec![false; net.nodes.len()];
    for s in net.segments.iter().filter(|s| jezdny(s)) {
        w_sieci[s.a.0 as usize] = true;
        w_sieci[s.b.0 as usize] = true;
    }
    let ile = w_sieci.iter().filter(|x| **x).count();
    let Some(start) = w_sieci.iter().position(|x| *x) else {
        return true;
    };
    let mut seen = vec![false; net.nodes.len()];
    let mut stos = vec![NodeId(start as u32)];
    seen[start] = true;
    let mut n = 1;
    while let Some(v) = stos.pop() {
        for &s in net.segments_at(v) {
            if !jezdny(&net.segments[s.0 as usize]) {
                continue;
            }
            let o = net.other_end(s, v);
            if !seen[o.0 as usize] {
                seen[o.0 as usize] = true;
                n += 1;
                stos.push(o);
            }
        }
    }
    n == ile
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOKALNA: RoadClass = RoadClass::Local;
    const KOLEKTOR: RoadClass = RoadClass::Collector;
    const BEZ: RoadFlags = RoadFlags::NONE;

    /// Minimalna sieć z listy krawędzi. Te trzy funkcje czytają wyłącznie graf —
    /// klasy, flagi i CSR sąsiedztwa — więc test nie potrzebuje ani terenu, ani
    /// geometrii, ani generacji miasta. Podział na `checks.rs` to właśnie uwidocznił.
    fn siec(
        wezlow: usize,
        krawedzie: &[(u32, u32, RoadClass, RoadFlags)],
        bramy: &[u32],
    ) -> RoadNetwork {
        let mut geom = PolyArena::new();
        let mut nodes: Vec<RoadNode> = (0..wezlow)
            .map(|_| RoadNode {
                pos: Vec2::new(0.0, 0.0),
                z_dm: 0,
                degree: 0,
                flags: NodeFlags::NONE,
            })
            .collect();
        for g in bramy {
            nodes[*g as usize].flags = nodes[*g as usize].flags.with(NodeFlags::GATE);
        }
        let mut segments: Vec<RoadSegment> = Vec::new();
        for (a, b, class, flags) in krawedzie {
            nodes[*a as usize].degree += 1;
            nodes[*b as usize].degree += 1;
            segments.push(RoadSegment {
                a: NodeId(*a),
                b: NodeId(*b),
                class: *class,
                geom: geom.push(&[]),
                structure: road::RoadStructure::AtGrade,
                lanes_fwd: 1,
                lanes_bwd: 1,
                row_m: 10,
                speed_kph: 50,
                max_tonnage_t: 0,
                length_dm: 100,
                district: magnat_core::DistrictId(0),
                flags: *flags,
            });
        }
        let mut adj_start = vec![0u32; nodes.len() + 1];
        for s in &segments {
            adj_start[s.a.0 as usize + 1] += 1;
            adj_start[s.b.0 as usize + 1] += 1;
        }
        for i in 1..adj_start.len() {
            adj_start[i] += adj_start[i - 1];
        }
        let mut kursor = adj_start.clone();
        let mut adj_items = vec![SegmentId(0); adj_start[nodes.len()] as usize];
        for (i, s) in segments.iter().enumerate() {
            for end in [s.a, s.b] {
                let k = &mut kursor[end.0 as usize];
                adj_items[*k as usize] = SegmentId(i as u32);
                *k += 1;
            }
        }
        RoadNetwork {
            nodes,
            segments,
            adj_start,
            adj_items,
            geom,
            gates: Vec::new(),
            furniture: Vec::new(),
        }
    }

    #[test]
    fn tor_nie_skleja_skladowych_jezdnych() {
        // 0—1 i 2—3 to dwie ulice, 1—2 to tor: pociąg nie skręca w ulicę.
        let net = siec(
            4,
            &[
                (0, 1, LOKALNA, BEZ),
                (2, 3, LOKALNA, BEZ),
                (1, 2, RoadClass::RailFreight, RoadFlags::RAIL),
            ],
            &[],
        );
        assert!(!is_connected(&net));
        assert_eq!(skladowe_jezdne(&net), (2, 1, 2));

        // ta sama krawędź jako ulica — jedna składowa, trzy segmenty jezdne.
        let net = siec(
            4,
            &[
                (0, 1, LOKALNA, BEZ),
                (2, 3, LOKALNA, BEZ),
                (1, 2, LOKALNA, BEZ),
            ],
            &[],
        );
        assert!(is_connected(&net));
        assert_eq!(skladowe_jezdne(&net), (1, 3, 3));
    }

    #[test]
    fn wiszacy_koniec_liczy_sie_od_kolektora_i_omija_bramy() {
        // Wiszą oba końce, ale tylko 0 siedzi na kolektorze; 2 jest na ulicy lokalnej.
        let net = siec(3, &[(0, 1, KOLEKTOR, BEZ), (1, 2, LOKALNA, BEZ)], &[]);
        assert_eq!(dangling_high_class(&net), vec![NodeId(0)]);

        // Ten sam graf z bramą w węźle 0 — brama ma prawo być końcem sieci.
        let net = siec(3, &[(0, 1, KOLEKTOR, BEZ), (1, 2, LOKALNA, BEZ)], &[0]);
        assert!(dangling_high_class(&net).is_empty());
    }
}
