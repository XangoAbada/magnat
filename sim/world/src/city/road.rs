//! Sieć drogowa jako dane (M2 §5.2, granica **K-14**).
//!
//! Zapisujemy tu wyłącznie to, co jest potrzebne, żeby miasto **narysować**: oś drogi,
//! klasę, liczbę pasów, tonaż i strukturę inżynierską. Geometria pasów, skrzyżowania
//! i kierunki ruchu są wyprowadzane z tego przez M4 przy budowie `RoadGraph` — i dlatego
//! `RoadSegment` nie ma pola, które istniałoby tylko po to, żeby przez drogę przejechać.

use magnat_core::DistrictId;
use magnat_spatial::{Aabb2, Vec2};

/// Dzielnica jeszcze nieprzypisana — przypisanie robi M2c (WP9), nie ta podfaza.
/// Sentinel, a nie `Option`, bo pole jest wypełniane dla **każdego** segmentu
/// w jednym przebiegu M2c i `Option` kosztowałby bajt na 16 tys. segmentów
/// oraz gałąź w każdym konsumencie.
pub const UNASSIGNED_DISTRICT: DistrictId = DistrictId(u16::MAX);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SegmentId(pub u32);

/// Uchwyt do polilinii we wspólnej arenie punktów.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PolyRef(pub u32);

/// Wspólna arena polilinii: osie dróg, obrysy kwartałów, później parcele i budynki.
///
/// Jedna arena zamiast `Vec<Vec2>` w każdej strukturze — bo tych polilinii jest
/// kilkadziesiąt tysięcy, a osobny `Vec` na każdą to osobna alokacja i osobny skok
/// przy iteracji.
#[derive(Clone, PartialEq, Debug)]
pub struct PolyArena {
    pts: Vec<Vec2>,
    starts: Vec<u32>,
}

/// `Default` musi dawać to samo co `new`: pusta arena z `starts == []` nie ma nawet
/// wartownika zerowego i pierwszy `get` na niej pada. A `Default` bierze się tu z
/// `mem::take`, czyli z miejsca, w którym nikt o tym nie myśli.
impl Default for PolyArena {
    fn default() -> PolyArena {
        PolyArena::new()
    }
}

impl PolyArena {
    #[must_use]
    pub fn new() -> PolyArena {
        PolyArena {
            pts: Vec::new(),
            starts: vec![0],
        }
    }

    pub fn push(&mut self, pts: &[Vec2]) -> PolyRef {
        let r = PolyRef(self.starts.len() as u32 - 1);
        self.pts.extend_from_slice(pts);
        self.starts.push(self.pts.len() as u32);
        r
    }

    #[must_use]
    pub fn get(&self, r: PolyRef) -> &[Vec2] {
        let a = self.starts[r.0 as usize] as usize;
        let b = self.starts[r.0 as usize + 1] as usize;
        &self.pts[a..b]
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.starts.len() - 1
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Klasa drogi. Kolejność wariantów jest kolejnością **malejącej rangi** dla dróg
/// kołowych; do porównań służy jednak [`RoadClass::rank`], a nie `Ord` — inaczej
/// „klasa ≥ Collector" czytałoby się odwrotnie, niż znaczy.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RoadClass {
    Highway,
    Arterial,
    Collector,
    Local,
    Service,
    Pedestrian,
    RailFreight,
    RailPassenger,
}

impl RoadClass {
    /// Ranga: im wyżej w hierarchii ulicznej, tym większa. Tory mają rangę 0 —
    /// nie uczestniczą w hierarchii dróg kołowych i nigdy nie są celem snapowania.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            RoadClass::Highway => 6,
            RoadClass::Arterial => 5,
            RoadClass::Collector => 4,
            RoadClass::Local => 3,
            RoadClass::Service => 2,
            RoadClass::Pedestrian => 1,
            RoadClass::RailFreight | RoadClass::RailPassenger => 0,
        }
    }

    /// Czy klasa niesie ruch kołowy (wchodzi do testu spójności T1 i do wyznaczania
    /// kwartałów).
    #[must_use]
    pub const fn is_driveable(self) -> bool {
        matches!(
            self,
            RoadClass::Highway
                | RoadClass::Arterial
                | RoadClass::Collector
                | RoadClass::Local
                | RoadClass::Service
        )
    }

    #[must_use]
    pub const fn is_rail(self) -> bool {
        matches!(self, RoadClass::RailFreight | RoadClass::RailPassenger)
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            RoadClass::Highway => "highway",
            RoadClass::Arterial => "arterial",
            RoadClass::Collector => "collector",
            RoadClass::Local => "local",
            RoadClass::Service => "service",
            RoadClass::Pedestrian => "pedestrian",
            RoadClass::RailFreight => "rail_freight",
            RoadClass::RailPassenger => "rail_passenger",
        }
    }

    #[must_use]
    pub const fn spec(self) -> ClassSpec {
        SPECS[self as usize]
    }
}

/// Parametry klasy drogi — tabela z M2 §5.2, w kodzie w jednym miejscu.
///
/// Trzy pola nie mają odpowiednika w tabeli planu i są tu uzupełnione (odnotowane
/// w §„Korekty planu" dokumentu podfazy): `max_gen`, `max_tonnage_t` i `lamp_spacing_m`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClassSpec {
    pub lanes_fwd: u8,
    pub lanes_bwd: u8,
    /// Szerokość pasa drogowego (ROW) w metrach.
    pub row_m: u8,
    pub seg_len_m: u16,
    pub speed_kph: u8,
    /// Dopuszczalne nachylenie w procentach.
    pub max_slope_pct: u8,
    /// Maksymalna rozpiętość mostu w metrach.
    pub bridge_max_m: u16,
    /// Przewyższenie, powyżej którego opłaca się tunel, w metrach. 0 = tunel zabroniony.
    pub tunnel_trigger_m: u16,
    pub tunnel_max_m: u16,
    /// Maksymalna generacja L-systemu dla tej klasy. **Bezpiecznik, nie regulator
    /// gęstości**: o rozmiarze miasta decydują promień obszaru zurbanizowanego i twardy
    /// budżet segmentów. Limit istnieje po to, żeby pojedynczy łańcuch nie objechał mapy.
    pub max_gen: u16,
    /// Rozstaw latarni w metrach; 0 = brak oświetlenia (wejście dla M11).
    pub lamp_spacing_m: u16,
    /// Dopuszczalny tonaż; 0 = bez ograniczenia.
    pub max_tonnage_t: u8,
}

impl ClassSpec {
    /// Próg nachylenia w jednostkach `TerrainQuery::slope_at` (0..=255 ≈ tan α × 64).
    #[must_use]
    pub const fn max_slope_units(&self) -> u8 {
        // tan α = pct / 100, więc jednostka = pct · 64 / 100. Całkowitoliczbowo,
        // bo to próg porównania, a nie wielkość fizyczna.
        ((self.max_slope_pct as u32 * 64) / 100) as u8
    }
}

const SPECS: [ClassSpec; 8] = [
    // Highway
    ClassSpec {
        lanes_fwd: 2,
        lanes_bwd: 2,
        row_m: 30,
        seg_len_m: 400,
        speed_kph: 120,
        max_slope_pct: 5,
        bridge_max_m: 800,
        tunnel_trigger_m: 20,
        tunnel_max_m: 1200,
        max_gen: 60,
        lamp_spacing_m: 45,
        max_tonnage_t: 0,
    },
    // Arterial
    ClassSpec {
        lanes_fwd: 2,
        lanes_bwd: 2,
        row_m: 24,
        seg_len_m: 200,
        speed_kph: 60,
        max_slope_pct: 7,
        bridge_max_m: 300,
        tunnel_trigger_m: 25,
        tunnel_max_m: 600,
        max_gen: 90,
        lamp_spacing_m: 30,
        max_tonnage_t: 0,
    },
    // Collector
    ClassSpec {
        lanes_fwd: 1,
        lanes_bwd: 1,
        row_m: 16,
        seg_len_m: 120,
        speed_kph: 50,
        max_slope_pct: 9,
        bridge_max_m: 120,
        tunnel_trigger_m: 35,
        tunnel_max_m: 250,
        max_gen: 70,
        lamp_spacing_m: 30,
        max_tonnage_t: 40,
    },
    // Local
    ClassSpec {
        lanes_fwd: 1,
        lanes_bwd: 1,
        row_m: 12,
        seg_len_m: 60,
        speed_kph: 30,
        max_slope_pct: 12,
        bridge_max_m: 40,
        tunnel_trigger_m: 0,
        tunnel_max_m: 0,
        max_gen: 40,
        lamp_spacing_m: 35,
        max_tonnage_t: 18,
    },
    // Service — „1×1" z tabeli czytane dosłownie: jeden pas, jeden kierunek.
    ClassSpec {
        lanes_fwd: 1,
        lanes_bwd: 0,
        row_m: 8,
        seg_len_m: 40,
        speed_kph: 20,
        max_slope_pct: 15,
        bridge_max_m: 20,
        tunnel_trigger_m: 0,
        tunnel_max_m: 0,
        max_gen: 4,
        lamp_spacing_m: 0,
        max_tonnage_t: 8,
    },
    // Pedestrian
    ClassSpec {
        lanes_fwd: 0,
        lanes_bwd: 0,
        row_m: 4,
        seg_len_m: 30,
        speed_kph: 0,
        max_slope_pct: 20,
        bridge_max_m: 30,
        tunnel_trigger_m: 0,
        tunnel_max_m: 0,
        max_gen: 4,
        lamp_spacing_m: 25,
        max_tonnage_t: 0,
    },
    // RailFreight
    ClassSpec {
        lanes_fwd: 1,
        lanes_bwd: 1,
        row_m: 20,
        seg_len_m: 500,
        speed_kph: 80,
        max_slope_pct: 2,
        bridge_max_m: 400,
        tunnel_trigger_m: 15,
        tunnel_max_m: 900,
        max_gen: 0,
        lamp_spacing_m: 0,
        max_tonnage_t: 0,
    },
    // RailPassenger
    ClassSpec {
        lanes_fwd: 1,
        lanes_bwd: 1,
        row_m: 16,
        seg_len_m: 500,
        speed_kph: 120,
        max_slope_pct: 2,
        bridge_max_m: 400,
        tunnel_trigger_m: 15,
        tunnel_max_m: 900,
        max_gen: 0,
        lamp_spacing_m: 0,
        max_tonnage_t: 0,
    },
];

/// Nasyp poniżej tej wysokości jest zwykłą niwelacją, nie budowlą (M2b).
pub const EMBANKMENT_MIN_DM: i32 = 15;

/// Przewyższenie terenu nad cięciwą w połowie odcinka, w decymetrach.
/// Dodatnie = teren nad drogą (tunel), ujemne = droga nad terenem (nasyp).
#[must_use]
pub fn cover_dm(t: &dyn crate::query::TerrainQuery, a: Vec2, b: Vec2) -> i32 {
    let h = |p: Vec2| t.height_at(p.x as i32, p.y as i32);
    let mid = (a + b) * 0.5;
    // `height_at` jest w jednostkach 0,5 m (K-13) — stąd ×5 na decymetry.
    (h(mid) - (h(a) + h(b)) / 2) * 5
}

/// Wybór struktury inżynierskiej dla odcinka — **jedna implementacja** dla L-systemu
/// arterii (WP5) i dla torów (WP5b). `None` = przeszkody nie da się pokonać w limitach
/// klasy i odcinek trzeba odrzucić.
///
/// Kolejność: struktura **przed** niweletą. Most i tunel z definicji nie podążają
/// za terenem, więc test nachylenia stosuje się tylko do przebiegu po gruncie.
#[must_use]
pub fn structure_for(
    t: &dyn crate::query::TerrainQuery,
    a: Vec2,
    b: Vec2,
    spec: &ClassSpec,
) -> Option<RoadStructure> {
    use crate::query::Crossing;
    let iv = |p: Vec2| magnat_core::IVec2::new(p.x as i32, p.y as i32);
    match t.crossing_cost(iv(a), iv(b)) {
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
            let cover = cover_dm(t, a, b);
            (cover >= i32::from(spec.tunnel_trigger_m) * 10).then_some(RoadStructure::Tunnel {
                cover_dm: cover.clamp(0, i32::from(u16::MAX)) as u16,
            })
        }
        Crossing::Embankment { .. } => {
            // Znak mówi, po której stronie drogi jest teren: dodatni to wykop,
            // ujemny nasyp. Limit 8 m obowiązuje w obie strony.
            let d = cover_dm(t, a, b);
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

/// Struktura inżynierska segmentu. Jednostki w decymetrach, bo w tych jednostkach
/// pracuje `engine/voxel` (voxel = 5 dm).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoadStructure {
    AtGrade,
    Bridge { clearance_dm: u16 },
    Tunnel { cover_dm: u16 },
    Embankment { height_dm: u16 },
}

/// Flagi segmentu. Własny newtype zamiast crate'a `bitflags` — sześć bitów
/// nie jest powodem do dokładania zależności.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct RoadFlags(pub u16);

impl RoadFlags {
    pub const NONE: RoadFlags = RoadFlags(0);
    pub const ONEWAY: RoadFlags = RoadFlags(1 << 0);
    pub const SIDEWALK: RoadFlags = RoadFlags(1 << 1);
    pub const NO_HEAVY: RoadFlags = RoadFlags(1 << 2);
    pub const TRAM_READY: RoadFlags = RoadFlags(1 << 3);
    pub const RAIL: RoadFlags = RoadFlags(1 << 4);
    pub const GRADE_SEPARATED: RoadFlags = RoadFlags(1 << 5);

    #[must_use]
    pub const fn contains(self, f: RoadFlags) -> bool {
        self.0 & f.0 == f.0
    }

    #[must_use]
    pub const fn with(self, f: RoadFlags) -> RoadFlags {
        RoadFlags(self.0 | f.0)
    }
}

/// Flagi węzła.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct NodeFlags(pub u8);

impl NodeFlags {
    pub const NONE: NodeFlags = NodeFlags(0);
    pub const GATE: NodeFlags = NodeFlags(1 << 0);
    pub const JUNCTION: NodeFlags = NodeFlags(1 << 1);
    pub const DEAD_END: NodeFlags = NodeFlags(1 << 2);

    #[must_use]
    pub const fn contains(self, f: NodeFlags) -> bool {
        self.0 & f.0 == f.0
    }

    #[must_use]
    pub const fn with(self, f: NodeFlags) -> NodeFlags {
        NodeFlags(self.0 | f.0)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RoadNode {
    pub pos: Vec2,
    /// **Rzędna niwelety** — wysokość nawierzchni w decymetrach.
    ///
    /// Nie jest tym samym co `TerrainQuery::height_at` w tym punkcie i o to właśnie chodzi:
    /// droga jest w przekroju podłużnym cięciwą między swoimi końcami, a różnicę wobec
    /// terenu pokrywa nasyp albo wykop. Bez tego pola węzeł powstały z **podziału**
    /// segmentu dostawałby rzędną terenu, przez co obie połówki miałyby nagle inne
    /// nachylenie niż odcinek, z którego powstały — i limit klasy przestawałby
    /// obowiązywać w miejscu, w którym nikt nic nie budował.
    pub z_dm: i32,
    pub degree: u8,
    pub flags: NodeFlags,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RoadSegment {
    pub a: NodeId,
    pub b: NodeId,
    pub class: RoadClass,
    /// OŚ drogi (centrolinia) — indeks do wspólnej areny punktów.
    pub geom: PolyRef,
    pub structure: RoadStructure,
    pub lanes_fwd: u8,
    pub lanes_bwd: u8,
    pub row_m: u8,
    pub speed_kph: u8,
    /// Dopuszczalny tonaż; 0 = bez ograniczenia.
    pub max_tonnage_t: u8,
    pub length_dm: u32,
    pub district: DistrictId,
    pub flags: RoadFlags,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FurnitureKind {
    StreetLamp,
    Bench,
    TreeRow,
    BusStopPad,
}

/// Mała architektura wzdłuż osi — wejście dla M11 (kontrakt w M2 §6).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StreetFurniture {
    pub seg: SegmentId,
    /// Parametr wzdłuż osi segmentu, 0..=1.
    pub t: f32,
    pub pos: glam::Vec3,
    pub kind: FurnitureKind,
}

/// Sieć transportowa miasta.
#[derive(Clone, PartialEq, Debug)]
pub struct RoadNetwork {
    pub nodes: Vec<RoadNode>,
    pub segments: Vec<RoadSegment>,
    /// CSR: sąsiedztwo węzeł → segmenty. `adj_start` ma `nodes.len() + 1` pozycji.
    pub adj_start: Vec<u32>,
    pub adj_items: Vec<SegmentId>,
    pub geom: PolyArena,
    pub gates: Vec<super::gates::CityGate>,
    pub furniture: Vec<StreetFurniture>,
}

impl RoadNetwork {
    #[must_use]
    pub fn segments_at(&self, n: NodeId) -> &[SegmentId] {
        let a = self.adj_start[n.0 as usize] as usize;
        let b = self.adj_start[n.0 as usize + 1] as usize;
        &self.adj_items[a..b]
    }

    #[must_use]
    pub fn pos(&self, n: NodeId) -> Vec2 {
        self.nodes[n.0 as usize].pos
    }

    #[must_use]
    pub fn other_end(&self, s: SegmentId, n: NodeId) -> NodeId {
        let seg = &self.segments[s.0 as usize];
        if seg.a == n {
            seg.b
        } else {
            seg.a
        }
    }

    #[must_use]
    pub fn bounds(&self) -> Aabb2 {
        let mut a = Aabb2::new(Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for n in &self.nodes {
            a = a.union_point(n.pos);
        }
        a
    }

    /// Łączna długość dróg danej klasy w metrach.
    #[must_use]
    pub fn length_m(&self, pred: impl Fn(&RoadSegment) -> bool) -> f64 {
        self.segments
            .iter()
            .filter(|s| pred(s))
            .map(|s| f64::from(s.length_dm) / 10.0)
            .sum()
    }

    /// Hash sieci — wchodzi do `world_hash_m2` (00 §3.6). Kolejność jest kolejnością
    /// tablic, a te powstają deterministycznie, więc hash jest odciskiem całej generacji.
    #[must_use]
    pub fn hash(&self) -> magnat_core::StateHash {
        let mut h = magnat_core::StateHasher::new();
        h.write_u32(self.nodes.len() as u32);
        for n in &self.nodes {
            h.write_u32(n.pos.x.to_bits());
            h.write_u32(n.pos.y.to_bits());
            h.write_u32(n.z_dm as u32);
            h.write_u8(n.degree);
            h.write_u8(n.flags.0);
        }
        h.write_u32(self.segments.len() as u32);
        for s in &self.segments {
            h.write_u32(s.a.0);
            h.write_u32(s.b.0);
            h.write_u8(s.class as u8);
            h.write_u32(s.length_dm);
            h.write_u16(s.flags.0);
            h.write_u16(match s.structure {
                RoadStructure::AtGrade => 0,
                RoadStructure::Bridge { clearance_dm } => 1 ^ clearance_dm << 2,
                RoadStructure::Tunnel { cover_dm } => 2 ^ cover_dm << 2,
                RoadStructure::Embankment { height_dm } => 3 ^ height_dm << 2,
            });
            for p in self.geom.get(s.geom) {
                h.write_u32(p.x.to_bits());
                h.write_u32(p.y.to_bits());
            }
        }
        h.write_u32(self.gates.len() as u32);
        for g in &self.gates {
            h.write_u8(g.kind as u8);
            h.write_u32(g.pos.x.to_bits());
            h.write_u32(g.pos.y.to_bits());
            h.write_i64(g.capacity.0);
        }
        h.write_u32(self.furniture.len() as u32);
        h.finish()
    }
}

/// Odcinek centrolinii dla M3 (kontrakt w M2 §6) — bez grafu, bo graf należy do M4 (**K-2**).
pub struct StreetLine<'a> {
    pub seg: SegmentId,
    pub pts: &'a [Vec2],
    pub length_dm: u32,
    pub class: RoadClass,
}

pub fn street_lines(net: &RoadNetwork) -> impl Iterator<Item = StreetLine<'_>> {
    net.segments.iter().enumerate().map(|(i, s)| StreetLine {
        seg: SegmentId(i as u32),
        pts: net.geom.get(s.geom),
        length_dm: s.length_dm,
        class: s.class,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_zwraca_to_co_dostala() {
        let mut a = PolyArena::new();
        let r1 = a.push(&[Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0)]);
        let r2 = a.push(&[Vec2::new(5.0, 5.0)]);
        assert_eq!(a.get(r1), &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0)]);
        assert_eq!(a.get(r2), &[Vec2::new(5.0, 5.0)]);
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn prog_nachylenia_zgadza_sie_z_tabela() {
        // 5% → tan 0,05 → 0,05·64 = 3,2 → 3 jednostki `slope_at`.
        assert_eq!(RoadClass::Highway.spec().max_slope_units(), 3);
        assert_eq!(RoadClass::Service.spec().max_slope_units(), 9);
        assert_eq!(RoadClass::RailFreight.spec().max_slope_units(), 1);
    }

    #[test]
    fn ranga_rosnie_w_gore_hierarchii() {
        assert!(RoadClass::Highway.rank() > RoadClass::Local.rank());
        assert_eq!(RoadClass::RailFreight.rank(), 0);
    }
}
