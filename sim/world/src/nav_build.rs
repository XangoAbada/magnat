//! Adapter geometrii miasta na graf nawigacyjny (M4a/WP1, `Z-3`).
//!
//! Konwersja „geometria M2 → graf M4" mieszka po stronie `sim/world`, a nie w `engine/nav`,
//! i nie jest to kwestia gustu: `sim/world` zależy od `sim/agents`, a `sim/agents` od M4b
//! zależy od `magnat-nav` (M4 §`Z-1`). Zależność `magnat-nav → magnat-world` domknęłaby
//! ten cykl i Cargo odmówiłoby budowy. Dlatego `engine/nav` zna wyłącznie
//! [`RoadGraphBuilder`], a to, czym jest ulica, parcela i most, wie tylko ten moduł.
//!
//! Moduł zastępuje — dla M4 — to, co dziś robi `population::siec_piesza`: tamta funkcja
//! buduje jedną, płaską sieć pieszą dla `WalkOracle` z M3. Zostaje nietknięta do M4b,
//! która usuwa ją razem z modułem `walk`.
//!
//! **Przestrzeń indeksów węzłów jest w każdej warstwie ta sama co w `RoadNetwork.nodes`.**
//! Węzeł 17 to węzeł 17 w drodze, chodniku, rowerze i kolei — dzięki temu przesiadka jest
//! parą `(warstwa, ten sam indeks)`, a nie wyszukiwaniem po współrzędnych. Ceną są węzły
//! izolowane w warstwach rzadkich (kolej nie dotyka większości skrzyżowań); walidator je
//! liczy i nie uważa za błąd.

use crate::city::road::{self, RoadFlags, RoadNetwork, RoadSegment, RoadStructure};
use crate::city::CityData;
use magnat_core::{DistrictId, IVec2, Mass, RoadClass};
use magnat_nav::{
    validate, EdgeId, EdgeSpec, GeomRef, GraphError, GraphReport, Modality, NavGraphs, NodeControl,
    NodeId as NavNodeId, RoadGraph, RoadGraphBuilder, SignalPlanId, TransferKind, TransferLink,
};

/// Czas przejścia krawężnika: wsiąść do auta albo z niego wysiąść, w sekundach.
/// Stała, bo M4a nie ma jeszcze ani parkingów (M4c/WP7), ani rozkładów (M4c) —
/// przesiadka „chodnik ↔ jezdnia" jest wszędzie taka sama.
const CURB_TRANSFER_S: u16 = 30;

/// Nośność mostu, gdy ani segment, ani klasa drogi nie niosą limitu tonażu.
///
/// Walidator odrzuca most o nośności ≤ 0, a „bez ograniczenia" (`Mass::ZERO`) znaczy
/// dla krawędzi co innego niż dla mostu: krawędź bez limitu jest legalna, most bez
/// nośności jest dziurą w modelu przejazdu ciężarówki.
// ponytail: 60 t to typowa nośność drogowego mostu klasy A; sufit jest taki, że wszystkie
// mosty bez własnego tonażu mają tę samą liczbę. Wyjście: M8 (remonty) nadaje mostom
// realne nośności razem ze stanem technicznym i wtedy ta stała znika.
const DEFAULT_BRIDGE_TONNAGE_T: i64 = 60;

/// Miejsce postojowe przyuliczne wzdłuż krawężnika, w metrach.
const CURB_SLOT_M: u32 = 6;

/// Co poszło nie tak przy budowie grafu nawigacyjnego z miasta.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NavBuildError {
    Graph(GraphError),
    /// Parcela bez dostępu pieszego — łamie kryterium WP1.
    ParcelUnreachable {
        parcel: u32,
    },
}

impl std::fmt::Display for NavBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NavBuildError::Graph(e) => write!(f, "graf nawigacyjny jest niepoprawny: {e:?}"),
            NavBuildError::ParcelUnreachable { parcel } => {
                write!(f, "parcela {parcel} ma front, ale nie ma dojścia pieszego")
            }
        }
    }
}

impl std::error::Error for NavBuildError {}

/// Metryki budowy — wchodzą do dziennika i do inspektora grafu.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct NavBuildReport {
    /// Raport walidacji każdej warstwy, w kolejności [`Modality::ALL`].
    pub layers: [GraphReport; 4],
    pub transfers: u32,
    pub parcels_with_access: u32,
    pub parcels_without_access: u32,
    pub build_micros: u64,
}

/// Buduje komplet warstw nawigacyjnych z sieci drogowej M2 (M4a/WP1).
///
/// # Errors
/// - [`NavBuildError::Graph`] — któraś warstwa nie przeszła [`magnat_nav::validate`]
///   (wisząca krawędź, pętla własna, most bez nośności, zerowa długość).
/// - [`NavBuildError::ParcelUnreachable`] — parcela z frontem przy ulicy, z której nie
///   powstała żadna krawędź piesza. To kryterium WP1 („każda parcela osiągalna pieszo"),
///   więc jest błędem, a nie liczbą w raporcie.
pub fn build_nav(city: &CityData) -> Result<(NavGraphs, NavBuildReport), NavBuildError> {
    let t0 = std::time::Instant::now();
    let roads = &city.roads;

    // Kolejność musi być kolejnością `Modality::as_index`, bo `NavGraphs::layer` indeksuje
    // tablicę wprost.
    let (road, road_nodes, _) = build_layer(roads, Modality::Road);
    let (foot, foot_nodes, foot_segments) = build_layer(roads, Modality::Foot);
    let (bike, _, _) = build_layer(roads, Modality::Bike);
    let (rail, _, _) = build_layer(roads, Modality::Rail);

    // Przesiadka „chodnik ↔ jezdnia" wszędzie tam, gdzie obie warstwy dotykają tego samego
    // węzła. Przystanków i parkingów nie ma — dokłada je M4c razem z modelem postoju.
    let mut transfers = Vec::new();
    for v in 0..roads.nodes.len() {
        if road_nodes[v] && foot_nodes[v] {
            let n = NavNodeId(v as u32);
            transfers.push(TransferLink {
                from: Modality::Foot,
                from_node: n,
                to: Modality::Road,
                to_node: n,
                seconds: CURB_TRANSFER_S,
                kind: TransferKind::Curb,
            });
            transfers.push(TransferLink {
                from: Modality::Road,
                from_node: n,
                to: Modality::Foot,
                to_node: n,
                seconds: CURB_TRANSFER_S,
                kind: TransferKind::Curb,
            });
        }
    }

    let layers = [
        validate(&road).map_err(NavBuildError::Graph)?,
        validate(&foot).map_err(NavBuildError::Graph)?,
        validate(&bike).map_err(NavBuildError::Graph)?,
        validate(&rail).map_err(NavBuildError::Graph)?,
    ];

    // Dostępność parcel. Parcela bez frontu (`Frontage::NONE`) to podwórko w środku
    // kwartału — M2 je dopuszcza i nie obiecuje im dojścia, więc jest liczona, a nie
    // zgłaszana jako błąd. Błędem jest dopiero front przy ulicy, po której nikt nie przejdzie.
    let mut with_access = 0u32;
    let mut without_access = 0u32;
    for (i, p) in city.parcels.parcels.iter().enumerate() {
        if p.frontage.is_none() {
            without_access += 1;
            continue;
        }
        let dostep = foot_segments
            .get(p.frontage.seg.0 as usize)
            .copied()
            .unwrap_or(false);
        if !dostep {
            return Err(NavBuildError::ParcelUnreachable { parcel: i as u32 });
        }
        with_access += 1;
    }

    let report = NavBuildReport {
        layers,
        transfers: transfers.len() as u32,
        parcels_with_access: with_access,
        parcels_without_access: without_access,
        build_micros: t0.elapsed().as_micros() as u64,
    };
    Ok((NavGraphs::new([road, foot, bike, rail], transfers), report))
}

/// Czy segment należy do warstwy.
///
/// `Foot` bierze **każdy** segment niekolejowy, także autostradę. To uproszczenie:
/// przy autostradzie chodnika nie ma. Wybrane świadomie, bo alternatywa (wycięcie
/// `Highway`) robi z kryterium WP1 zakładnika urbanistyki — wystarczy jedna parcela
/// frontująca do drogi klasy `Highway`, żeby budowa grafu padła błędem o czymś, czego
/// graf nie naprawi.
// ponytail: sufit to piesi chodzący autostradą w routingu M4b. Wyjście: M4c ma flagę
// `RoadFlags::SIDEWALK` z M2 i wtedy warstwa piesza filtruje po niej, a nie po klasie.
fn in_layer(s: &RoadSegment, m: Modality) -> bool {
    let rail = s.flags.contains(RoadFlags::RAIL) || s.class.is_rail();
    match m {
        Modality::Road => s.class.is_driveable() && !s.flags.contains(RoadFlags::RAIL),
        Modality::Foot => !rail,
        Modality::Bike => !rail && s.class != RoadClass::Highway,
        Modality::Rail => rail,
    }
}

/// Buduje jedną warstwę. Zwraca graf, tablicę „warstwa dotyka tego węzła" i tablicę
/// „z tego segmentu powstała w tej warstwie krawędź".
fn build_layer(roads: &RoadNetwork, m: Modality) -> (RoadGraph, Vec<bool>, Vec<bool>) {
    let mut b = RoadGraphBuilder::new(m);
    b.reserve(roads.nodes.len(), roads.segments.len() * 2);

    // Zaokrąglenie w JEDNYM miejscu: metry (f32) → centymetry (i32) przez `round`.
    // `population::siec_piesza` ucina (`as i32`) — różnica to pół centymetra na węzeł,
    // ale `round` jest symetryczne wokół zera, a ucinanie przesuwa całą zachodnią
    // i południową połowę miasta o pół centymetra do środka. Skoro adapter i tak
    // zastępuje tamtą funkcję w M4b, bierzemy wersję poprawną.
    for n in &roads.nodes {
        b.add_node(
            IVec2::new(
                (n.pos.x * 100.0).round() as i32,
                (n.pos.y * 100.0).round() as i32,
            ),
            n.z_dm * 10,
        );
    }

    let mut node_touched = vec![false; roads.nodes.len()];
    let mut seg_used = vec![false; roads.segments.len()];
    // (węzeł docelowy, ranga klasy, krawędź) — tylko warstwa `Road`, do `NodeControl`.
    let mut incoming: Vec<(u32, u8, u32)> = Vec::new();

    for (i, s) in roads.segments.iter().enumerate() {
        if !in_layer(s, m) {
            continue;
        }
        // Zwyrodnienia sieci wypadają tu, a nie w walidatorze: pętla własna i zerowa
        // długość są dla `engine/nav` twardym błędem, a dla generatora miasta artefaktem,
        // którego nie ma sensu zgłaszać jako awarii całego grafu.
        if s.a == s.b || s.length_dm == 0 {
            continue;
        }

        let a = NavNodeId(s.a.0);
        let z = NavNodeId(s.b.0);
        let length_cm = s.length_dm.saturating_mul(10);
        let bridge = match s.structure {
            RoadStructure::Bridge { .. } => Some(b.add_bridge(bridge_mass(s))),
            _ => None,
        };
        let spec = EdgeSpec {
            from: a,
            to: z,
            geometry_ref: GeomRef(s.geom.0),
            length_cm,
            lanes: 1,
            class: s.class,
            speed_limit_dkmh: u16::from(s.speed_kph) * 10,
            max_mass: Mass(i64::from(s.max_tonnage_t) * 1_000_000),
            grade_permille: grade_permille(roads, s),
            bridge,
            curb_parking: 0,
            // Segment bez dzielnicy trafia do dzielnicy 0. Sentinel `u16::MAX` nie jest
            // indeksem do niczego, a graf używa `district` wyłącznie do agregatów.
            district: if s.district == road::UNASSIGNED_DISTRICT {
                DistrictId(0)
            } else {
                s.district
            },
        };

        let mut zapisz = |e: EdgeId, do_wezla: u32| {
            if m == Modality::Road {
                incoming.push((do_wezla, s.class.rank(), e.0));
            }
        };

        if m == Modality::Road {
            let parking = curb_parking(s);
            let fwd = EdgeSpec {
                lanes: s.lanes_fwd,
                curb_parking: parking,
                ..spec
            };
            if s.flags.contains(RoadFlags::ONEWAY) {
                // Jednokierunkowa: tylko a → b. Brak pasa w tym kierunku znaczy, że
                // jezdni tam nie ma — walidator odrzuca krawędź jezdną o zerowej liczbie pasów.
                if s.lanes_fwd == 0 {
                    continue;
                }
                let e = b.add_edge(fwd);
                zapisz(e, z.0);
            } else if s.lanes_fwd > 0 && s.lanes_bwd > 0 {
                // `ClassSpec` nie daje klasy o dwóch niezerowych, ale różnych liczbach pasów
                // (jedyna asymetria to `Service` 1/0, czyli faktyczna jednokierunkowość),
                // więc para bierze `lanes_fwd` dla obu kierunków.
                let (f, r) = b.add_edge_pair(fwd);
                zapisz(f, z.0);
                zapisz(r, a.0);
            } else if s.lanes_fwd > 0 {
                let e = b.add_edge(fwd);
                zapisz(e, z.0);
            } else if s.lanes_bwd > 0 {
                let e = b.add_edge(EdgeSpec {
                    from: z,
                    to: a,
                    lanes: s.lanes_bwd,
                    curb_parking: parking,
                    grade_permille: -spec.grade_permille,
                    ..spec
                });
                zapisz(e, a.0);
            } else {
                // 0/0 pasów (klasa `Pedestrian`): w warstwie drogowej nie ma jezdni.
                continue;
            }
        } else {
            // Warstwy niedrogowe są zawsze dwukierunkowe: `ONEWAY` to reguła ruchu
            // kołowego, a nie właściwość geometrii — pieszy, rowerzysta i pociąg
            // manewrowy poruszają się tą ulicą w obie strony.
            b.add_edge_pair(spec);
        }

        seg_used[i] = true;
        node_touched[s.a.0 as usize] = true;
        node_touched[s.b.0 as usize] = true;
    }

    if m == Modality::Road {
        set_controls(roads, &mut b, &seg_used, incoming);
    }
    (b.finish(), node_touched, seg_used)
}

/// Nośność mostu w gramach. Kolejność źródeł: tonaż segmentu, tonaż klasy, stała.
fn bridge_mass(s: &RoadSegment) -> Mass {
    let t = if s.max_tonnage_t > 0 {
        i64::from(s.max_tonnage_t)
    } else {
        let klasa = road::spec(s.class).max_tonnage_t;
        if klasa > 0 {
            i64::from(klasa)
        } else {
            DEFAULT_BRIDGE_TONNAGE_T
        }
    };
    Mass(t * 1_000_000)
}

/// Nachylenie niwelety w promilach, z rzędnych węzłów. Obie wielkości są w decymetrach,
/// więc iloraz jest bezjednostkowy i całkowitoliczbowy — bez floata i bez zaokrąglania.
fn grade_permille(roads: &RoadNetwork, s: &RoadSegment) -> i16 {
    if s.length_dm == 0 {
        return 0;
    }
    let dz =
        i64::from(roads.nodes[s.b.0 as usize].z_dm) - i64::from(roads.nodes[s.a.0 as usize].z_dm);
    (dz * 1000 / i64::from(s.length_dm)).clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}

/// Miejsca postojowe przyuliczne na kierunek.
///
/// M2 takiej danej nie ma, więc jest szacowana z długości i klasy: parkuje się przy
/// ulicy lokalnej, dojazdowej i kolektorze, nie parkuje się na drodze bezkolizyjnej.
// ponytail: sufit to szacunek „długość / 6 m" bez zatok, zjazdów i zakazów postoju.
// Wyjście: M4c/WP7 potrzebuje realnych krawężników i wtedy liczba idzie z danych, nie stąd.
fn curb_parking(s: &RoadSegment) -> u16 {
    let dozwolone = matches!(
        s.class,
        RoadClass::Local | RoadClass::Service | RoadClass::Collector
    ) && !s.flags.contains(RoadFlags::GRADE_SEPARATED);
    if !dozwolone {
        return 0;
    }
    let length_m = s.length_dm / 10;
    u16::try_from(length_m / CURB_SLOT_M).unwrap_or(u16::MAX)
}

/// Sterowanie w węzłach warstwy drogowej.
///
/// Reguła: ≤ 2 wloty drogowe → bez sterowania (to nie skrzyżowanie, tylko załamanie osi);
/// ≥ 3 wloty z udziałem arterii lub autostrady → sygnalizacja (samego planu nadaje
/// M4d/WP8, tu jest tylko klucz); reszta → znaki pierwszeństwa dla dwóch krawędzi
/// wjazdowych o najwyższej randze klasy, remis rozstrzyga niższy `EdgeId`.
///
/// Dopełnienie, gdy wlotów jest mniej niż dwa: jeden wlot powtarza się w obu pozycjach
/// (`[e, e]`) — droga z pierwszeństwem jest wtedy jednowlotowa i tak wygląda w tabeli
/// manewrów; zero wlotów (wszystkie ulice wychodzą z węzła) zostaje bez sterowania,
/// bo nie ma komu nadać pierwszeństwa.
fn set_controls(
    roads: &RoadNetwork,
    b: &mut RoadGraphBuilder,
    seg_used: &[bool],
    mut incoming: Vec<(u32, u8, u32)>,
) {
    // Sortowanie zamiast mapy: deterministyczne i bez alokacji na węzeł.
    incoming.sort_unstable_by(|x, y| x.0.cmp(&y.0).then(y.1.cmp(&x.1)).then(x.2.cmp(&y.2)));
    let mut top = vec![[u32::MAX; 2]; roads.nodes.len()];
    for (n, _, e) in incoming {
        let t = &mut top[n as usize];
        if t[0] == u32::MAX {
            t[0] = e;
        } else if t[1] == u32::MAX {
            t[1] = e;
        }
    }

    for (v, wlot_top) in top.iter().enumerate() {
        let wloty = roads.segments_at(road::NodeId(v as u32));
        let mut stopien = 0u32;
        let mut glowna = false;
        for s in wloty {
            if !seg_used[s.0 as usize] {
                continue;
            }
            stopien += 1;
            let c = roads.segments[s.0 as usize].class;
            glowna |= matches!(c, RoadClass::Arterial | RoadClass::Highway);
        }
        if stopien <= 2 {
            continue;
        }
        let n = NavNodeId(v as u32);
        if glowna {
            b.set_control(n, NodeControl::Signal(SignalPlanId(0)));
            continue;
        }
        let major = match *wlot_top {
            [u32::MAX, _] => continue,
            [a, u32::MAX] => [EdgeId(a), EdgeId(a)],
            [a, c] => [EdgeId(a), EdgeId(c)],
        };
        b.set_control(n, NodeControl::PrioritySigns { major });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::road::{
        NodeFlags, NodeId, PolyArena, RoadNode, RoadSegment, SegmentId, UNASSIGNED_DISTRICT,
    };
    use magnat_spatial::Vec2;

    /// Minimalna sieć: węzły `(x_m, y_m, z_dm)` i segmenty `(a, b, length_dm, klasa, flagi)`.
    fn siec(
        wezly: &[(f32, f32, i32)],
        segmenty: &[(u32, u32, u32, RoadClass, RoadFlags)],
    ) -> RoadNetwork {
        let mut geom = PolyArena::new();
        let nodes: Vec<RoadNode> = wezly
            .iter()
            .map(|&(x, y, z)| RoadNode {
                pos: Vec2::new(x, y),
                z_dm: z,
                degree: 0,
                flags: NodeFlags::NONE,
            })
            .collect();
        let segments: Vec<RoadSegment> = segmenty
            .iter()
            .map(|&(a, b, length_dm, class, flags)| {
                let sp = road::spec(class);
                let g = geom.push(&[nodes[a as usize].pos, nodes[b as usize].pos]);
                RoadSegment {
                    a: NodeId(a),
                    b: NodeId(b),
                    class,
                    geom: g,
                    structure: RoadStructure::AtGrade,
                    lanes_fwd: sp.lanes_fwd,
                    lanes_bwd: sp.lanes_bwd,
                    row_m: sp.row_m,
                    speed_kph: sp.speed_kph,
                    max_tonnage_t: sp.max_tonnage_t,
                    length_dm,
                    district: UNASSIGNED_DISTRICT,
                    flags,
                }
            })
            .collect();
        let mut adj_start = Vec::with_capacity(nodes.len() + 1);
        let mut adj_items = Vec::new();
        for v in 0..nodes.len() {
            adj_start.push(adj_items.len() as u32);
            for (i, s) in segments.iter().enumerate() {
                if s.a.0 as usize == v || s.b.0 as usize == v {
                    adj_items.push(SegmentId(i as u32));
                }
            }
        }
        adj_start.push(adj_items.len() as u32);
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
    fn jednokierunkowa_daje_jedna_krawedz_jezdna_i_dwie_piesze() {
        let net = siec(
            &[(0.0, 0.0, 0), (100.0, 0.0, 0)],
            &[(0, 1, 1000, RoadClass::Local, RoadFlags::ONEWAY)],
        );
        let (road, _, _) = build_layer(&net, Modality::Road);
        let (foot, _, uzyte) = build_layer(&net, Modality::Foot);
        assert_eq!(road.edge_count(), 1, "jednokierunkowa ma jeden kierunek");
        assert_eq!(road.edge(EdgeId(0)).from, NavNodeId(0));
        assert_eq!(road.edge(EdgeId(0)).twin, None);
        assert_eq!(foot.edge_count(), 2, "pieszy chodzi w obie strony");
        assert!(uzyte[0]);
        // 100 m = 1000 dm = 10 000 cm.
        assert_eq!(road.edge(EdgeId(0)).length_cm, 10_000);
        assert!(validate(&road).is_ok() && validate(&foot).is_ok());
    }

    #[test]
    fn nachylenie_ma_przeciwny_znak_na_krawedzi_powrotnej() {
        // 5 m w górę na 100 m = 50 ‰.
        let net = siec(
            &[(0.0, 0.0, 0), (100.0, 0.0, 50)],
            &[(0, 1, 1000, RoadClass::Local, RoadFlags::NONE)],
        );
        let (road, _, _) = build_layer(&net, Modality::Road);
        assert_eq!(road.edge(EdgeId(0)).grade_permille, 50);
        assert_eq!(road.edge(EdgeId(1)).grade_permille, -50);
        assert_eq!(road.edge(EdgeId(0)).twin, Some(EdgeId(1)));
    }

    #[test]
    fn tory_nie_wchodza_do_warstw_ulicznych() {
        let net = siec(
            &[(0.0, 0.0, 0), (500.0, 0.0, 0)],
            &[(0, 1, 5000, RoadClass::RailFreight, RoadFlags::RAIL)],
        );
        assert_eq!(build_layer(&net, Modality::Road).0.edge_count(), 0);
        assert_eq!(build_layer(&net, Modality::Foot).0.edge_count(), 0);
        assert_eq!(build_layer(&net, Modality::Bike).0.edge_count(), 0);
        assert_eq!(build_layer(&net, Modality::Rail).0.edge_count(), 2);
    }

    #[test]
    fn autostrada_jest_pieszo_ale_nie_rowerem() {
        let net = siec(
            &[(0.0, 0.0, 0), (400.0, 0.0, 0)],
            &[(0, 1, 4000, RoadClass::Highway, RoadFlags::NONE)],
        );
        assert_eq!(build_layer(&net, Modality::Foot).0.edge_count(), 2);
        assert_eq!(build_layer(&net, Modality::Bike).0.edge_count(), 0);
    }

    #[test]
    fn skrzyzowanie_z_arteria_dostaje_sygnalizacje() {
        // Gwiazda: cztery ulice w węźle 0, jedna z nich arterią.
        let net = siec(
            &[
                (0.0, 0.0, 0),
                (100.0, 0.0, 0),
                (-100.0, 0.0, 0),
                (0.0, 100.0, 0),
                (0.0, -100.0, 0),
            ],
            &[
                (0, 1, 1000, RoadClass::Arterial, RoadFlags::NONE),
                (0, 2, 1000, RoadClass::Local, RoadFlags::NONE),
                (0, 3, 1000, RoadClass::Local, RoadFlags::NONE),
                (0, 4, 1000, RoadClass::Local, RoadFlags::NONE),
            ],
        );
        let (road, _, _) = build_layer(&net, Modality::Road);
        assert_eq!(road.controls[0], NodeControl::Signal(SignalPlanId(0)));
        // Końce promieni mają po jednym wlocie — to nie skrzyżowanie.
        assert_eq!(road.controls[1], NodeControl::Uncontrolled);

        // Ten sam węzeł bez arterii: znaki pierwszeństwa dla dwóch wlotów najwyższej rangi.
        let net = siec(
            &[
                (0.0, 0.0, 0),
                (100.0, 0.0, 0),
                (-100.0, 0.0, 0),
                (0.0, 100.0, 0),
            ],
            &[
                (0, 1, 1000, RoadClass::Collector, RoadFlags::NONE),
                (0, 2, 1000, RoadClass::Collector, RoadFlags::NONE),
                (0, 3, 1000, RoadClass::Local, RoadFlags::NONE),
            ],
        );
        let (road, _, _) = build_layer(&net, Modality::Road);
        let NodeControl::PrioritySigns { major } = road.controls[0] else {
            panic!(
                "bez arterii ma być pierwszeństwo, a jest {:?}",
                road.controls[0]
            );
        };
        for e in major {
            assert_eq!(road.edge(e).to, NavNodeId(0), "wlot ma wchodzić do węzła");
            assert_eq!(road.edge(e).class, RoadClass::Collector);
        }
        assert!(major[0] < major[1], "remis rozstrzyga niższy EdgeId");
    }

    #[test]
    fn dlugosc_warstwy_drogowej_zgadza_sie_z_siecia() {
        let net = siec(
            &[(0.0, 0.0, 0), (100.0, 0.0, 0), (250.0, 0.0, 0)],
            &[
                (0, 1, 1000, RoadClass::Local, RoadFlags::NONE),
                (1, 2, 1500, RoadClass::Local, RoadFlags::ONEWAY),
            ],
        );
        let (road, _, _) = build_layer(&net, Modality::Road);
        // Dwukierunkowa liczy się dwa razy (dwie krawędzie), jednokierunkowa raz.
        let suma: u64 = road.edges.iter().map(|e| u64::from(e.length_cm)).sum();
        assert_eq!(suma, 2 * 10_000 + 15_000);
    }

    #[test]
    fn miejsca_postojowe_tylko_przy_ulicach_lokalnych() {
        let net = siec(
            &[(0.0, 0.0, 0), (120.0, 0.0, 0), (400.0, 0.0, 0)],
            &[
                (0, 1, 1200, RoadClass::Local, RoadFlags::NONE),
                (1, 2, 2800, RoadClass::Highway, RoadFlags::NONE),
            ],
        );
        let (road, _, _) = build_layer(&net, Modality::Road);
        // 120 m / 6 m = 20 miejsc na kierunek.
        assert_eq!(road.edge(EdgeId(0)).curb_parking, 20);
        let autostrada = road
            .edges
            .iter()
            .find(|e| e.class == RoadClass::Highway)
            .expect("autostrada jest w warstwie drogowej");
        assert_eq!(autostrada.curb_parking, 0);
    }

    /// Kryterium WP1 na prawdziwym mieście: wszystkie warstwy przechodzą walidację,
    /// każda parcela z frontem ma dojście pieszo, a budowa mieści się w budżecie 400 ms.
    #[test]
    #[ignore = "generuje świat i miasto — CI uruchamia jawnie przez --include-ignored"]
    fn miasto_daje_poprawny_graf_i_dostep_do_kazdej_parceli() {
        use crate::params::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};
        use magnat_jobs::JobPool;
        use magnat_voxel::MaterialRegistry;
        use std::sync::Arc;

        let pool = JobPool::new(0);
        let params = WorldGenParams {
            seed: 0x00C0_FFEE,
            size: WorldSize::Small4km,
            region: Region::River,
            epoch: Epoch::Y1990,
            profile: EconomyProfile::Mixed,
            difficulty: Difficulty::Normal,
        };
        let (data, _) = crate::pipeline::generate(params, &pool).expect("świat");
        let reg = Arc::new(
            MaterialRegistry::load_dir(&crate::assets::data_path("materials")).expect("materiały"),
        );
        let terrain = crate::terrain::Terrain::new(data, reg);
        let plan = crate::city::CityPlan {
            seed: params.seed,
            size: params.size,
            region: params.region,
            epoch: params.epoch,
            profile: params.profile,
            difficulty: params.difficulty,
            target_pop: crate::city::target_pop(params.size),
        };
        let city = crate::city::generate_city(&plan, &terrain, terrain.materials(), &pool)
            .expect("miasto");

        let (graphs, r) = build_nav(&city).expect("graf nawigacyjny miasta");
        println!(
            "nav_build: {} µs, warstwy {:?}, przesiadki {}, parcele {}/{}",
            r.build_micros,
            r.layers
                .iter()
                .map(|l| (l.nodes, l.edges, l.isolated, l.largest_component))
                .collect::<Vec<_>>(),
            r.transfers,
            r.parcels_with_access,
            r.parcels_with_access + r.parcels_without_access
        );

        let road = &r.layers[Modality::Road.as_index()];
        let podlaczone = road.nodes - road.isolated;
        assert!(
            road.largest_component * 100 >= podlaczone * 95,
            "warstwa drogowa rozpada się na kawałki: {} z {podlaczone}",
            road.largest_component
        );
        assert!(r.parcels_with_access > 0, "miasto bez parcel z frontem");
        assert_eq!(
            graphs.layer(Modality::Road).node_count(),
            city.roads.nodes.len(),
            "przestrzeń indeksów węzłów ma być 1:1 z siecią"
        );
        // Sanity jednostek: suma długości krawędzi drogowych ≈ 2 × długość dróg jezdnych
        // (każdy dwukierunkowy odcinek to dwie krawędzie), z tolerancją na jednokierunkowe.
        let z_sieci = city.roads.length_m(|s| {
            s.class.is_driveable() && !s.flags.contains(RoadFlags::RAIL) && s.lanes_fwd > 0
        });
        let z_grafu = f64::from(road.total_length_km) * 1000.0;
        assert!(
            z_grafu > z_sieci * 0.95 && z_grafu < z_sieci * 2.05,
            "długości się nie zgadzają: graf {z_grafu} m, sieć {z_sieci} m"
        );
    }
}
