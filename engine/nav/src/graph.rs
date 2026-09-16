//! Graf sieci transportowej (M4a §5.1, WP1).
//!
//! Granica **K-14**: M2 zapisuje oś drogi, klasę, liczbę pasów, tonaż i strukturę
//! inżynierską — czyli to, co wynika z planowania miasta. Tu wyprowadza się z tego
//! **krawędzie kierunkowe, manewry skrętne i kierunki ruchu**, czyli to, co jest
//! potrzebne, żeby przez miasto przejechać.
//!
//! Crate nie wie nic o mieście: nie ma tu `RoadNetwork`, parcel ani budynków.
//! Wejście wchodzi przez [`RoadGraphBuilder`], a adapter z geometrii M2 mieszka
//! w `sim/world` (`Z-3`) — bo `engine/nav` **nie może** zależeć od `sim/world`:
//! `sim/world` zależy od `sim/agents`, a `sim/agents` od M4b zależy od `engine/nav`.

use magnat_core::{DistrictId, HashState, IVec2, Mass, RoadClass, StateHasher};

/// Węzeł grafu — indeks w tablicy, nie `Entity`. Graf jest tablicą, nie ECS-em.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct NodeId(pub u32);

/// Krawędź **kierunkowa**. Jezdnia dwukierunkowa to dwie krawędzie.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct EdgeId(pub u32);

/// Obiekt mostowy — nośność i stan remontowy zmienia M8, graf tylko je nosi.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct BridgeId(pub u32);

/// Nieprzezroczysty uchwyt do geometrii po stronie M2 (`PolyRef`).
///
/// Celowo `u32`, a nie `magnat_world::PolyRef`: gdyby graf trzymał typ z `sim/world`,
/// `engine/nav` musiałby od niego zależeć i powstałby cykl opisany w nagłówku modułu.
/// Do narysowania krawędzi i tak trzeba areny M2, a ta jest po stronie, która ten
/// uchwyt nadała.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct GeomRef(pub u32);

/// Plan sygnalizacji z `data/` (M4d/WP8 nadaje mu treść; tu jest tylko klucz).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct SignalPlanId(pub u16);

/// Warstwa grafu. Każda jest **osobną instancją** [`RoadGraph`], nie wspólnym grafem
/// z maską: graf pieszy ma inną gęstość i inny algorytm (A\* wystarcza, bo trasy są
/// krótkie), a rozdzielenie usuwa filtrowanie krawędzi z pętli wewnętrznej.
/// Przesiadki między warstwami to jawna tabela [`TransferLink`].
///
/// Tramwaju nie ma, bo M2 go nie generuje (jest tylko `RoadFlags::TRAM_READY`);
/// warstwę dokłada faza, która postawi pierwszy tor tramwajowy.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u8)]
pub enum Modality {
    Road,
    Foot,
    Bike,
    Rail,
}

impl Modality {
    pub const ALL: &'static [Modality] = &[
        Modality::Road,
        Modality::Foot,
        Modality::Bike,
        Modality::Rail,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn mask(self) -> ModalityMask {
        ModalityMask(1 << (self as u8))
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Modality::Road => "road",
            Modality::Foot => "foot",
            Modality::Bike => "bike",
            Modality::Rail => "rail",
        }
    }

    /// Prędkość swobodna warstwy, gdy krawędź nie niesie własnego limitu
    /// (pieszy i rower nie mają limitu drogowego). Decykilometry na godzinę.
    ///
    /// Pieszy: 81 m/min z M3 (`walk::BASE_SPEED_M_PER_MIN`) = 4,86 km/h — ta sama
    /// liczba, żeby czasy dojścia nie drgnęły przy podmianie `TravelOracle` w M4b.
    #[must_use]
    pub const fn free_speed_dkmh(self) -> u16 {
        match self {
            Modality::Foot => 48,
            Modality::Bike => 150,
            Modality::Road | Modality::Rail => 0, // z limitu krawędzi
        }
    }
}

/// Maska warstw, które używają krawędzi.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct ModalityMask(pub u8);

impl ModalityMask {
    pub const NONE: ModalityMask = ModalityMask(0);

    #[must_use]
    pub const fn with(self, m: Modality) -> ModalityMask {
        ModalityMask(self.0 | m.mask().0)
    }

    #[must_use]
    pub const fn contains(self, m: Modality) -> bool {
        self.0 & m.mask().0 != 0
    }
}

/// Sterowanie w węźle.
///
/// Wariantu `Roundabout` **nie ma**: M2 nie generuje rond, a wariant z `Vec` odebrałby
/// `NodeControl` `Copy` i dołożył 20 B na węzeł w grafie, który rond nie zawiera.
/// Dokłada go WP8 (M4d) razem z modelem pierwszeństwa na pierścieniu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NodeControl {
    #[default]
    Uncontrolled,
    /// Dwie krawędzie o najwyższej randze klasy są drogą z pierwszeństwem.
    PrioritySigns {
        major: [EdgeId; 2],
    },
    Signal(SignalPlanId),
}

/// Pierwszeństwo manewru skrętnego.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TurnPriority {
    #[default]
    Major,
    Yield,
    Signal(u8),
}

/// Manewr skrętny: (wjazd, wyjazd). To **on**, nie węzeł, ma przepustowość.
///
/// Węzła nie ma w strukturze, bo wynika z `in_edge` (`edges[in_edge].to`), a tablica
/// ma 16 pozycji na skrzyżowanie czterowlotowe — pole na węzeł byłoby 4 B kłamstwa
/// powielonego 16 razy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TurnMovement {
    pub in_edge: EdgeId,
    pub out_edge: EdgeId,
    pub banned: bool,
    pub priority: TurnPriority,
    /// Pojazdów na godzinę przy zielonym ciągłym. Konsument: model węzła w M4b.
    pub saturation_flow_vph: u16,
}

/// Most: nośność i stan. M8 zmienia `closed` przy remoncie, graf wtedy idzie
/// kustomizacją, nie rekontrakcją — geometria się nie zmienia.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bridge {
    pub max_mass: Mass,
    pub closed: bool,
}

/// Węzeł: położenie w centymetrach (ta sama jednostka co `WorldCoord` z M3)
/// i rzędna niwelety, też w centymetrach.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RoadNode {
    pub pos_cm: IVec2,
    pub z_cm: i32,
}

/// Krawędź kierunkowa.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RoadEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub geometry_ref: GeomRef,
    /// Długość wzdłuż osi, całkowita — wchodzi do wzorów pieniężnych (00 §2, R9).
    pub length_cm: u32,
    pub lanes: u8,
    pub class: RoadClass,
    /// Decykilometry na godzinę: 500 = 50,0 km/h. 0 = bierz z warstwy.
    pub speed_limit_dkmh: u16,
    /// Tonaż dopuszczalny. `Mass::ZERO` = bez ograniczeń.
    pub max_mass: Mass,
    /// Nachylenie w promilach; wpływa na zużycie paliwa (M4b §5.7).
    pub grade_permille: i16,
    pub modalities: ModalityMask,
    pub bridge: Option<BridgeId>,
    /// Miejsc postojowych przyulicznych. Konsument: parkingi (M4c/WP7),
    /// przelewanie się placu manewrowego na ulicę (M6).
    pub curb_parking: u16,
    pub district: DistrictId,
    /// Przeciwny kierunek tej samej jezdni — `None` dla jednokierunkowej.
    /// Nosi go krawędź, bo inaczej zakazu zawracania nie da się wystawić bez
    /// szukania po geometrii.
    pub twin: Option<EdgeId>,
}

impl RoadEdge {
    /// Prędkość swobodna w decykilometrach na godzinę dla danej warstwy.
    #[must_use]
    pub const fn free_speed_dkmh(&self, m: Modality) -> u16 {
        let layer = m.free_speed_dkmh();
        if layer != 0 {
            layer
        } else {
            self.speed_limit_dkmh
        }
    }

    /// Czas przejazdu swobodnego w setnych sekundy — **wyłącznie arytmetyka
    /// całkowita**, żeby float nie wszedł tylnymi drzwiami do ścieżki pieniężnej (R9).
    ///
    /// `cm / (dkmh × 25/9 cm/s) × 100 = cm × 36 / dkmh`.
    #[must_use]
    pub const fn free_flow_cs(&self, m: Modality) -> u32 {
        let v = self.free_speed_dkmh(m);
        if v == 0 {
            return u32::MAX;
        }
        let num = self.length_cm as u64 * 36;
        let t = num / v as u64;
        if t > u32::MAX as u64 {
            u32::MAX
        } else {
            t as u32
        }
    }
}

// ---- CSR --------------------------------------------------------------------

/// Indeks „klucz → zakres pozycji" w płaskiej tablicy.
///
/// To **nie** jest `magnat_spatial::CsrGrid`: tamten jest indeksem przestrzennym
/// (komórka siatki → obiekty w niej), ten jest sąsiedztwem grafu (węzeł → krawędzie
/// wychodzące). Wspólna byłaby tylko para `starts`/`items`, a to za mało na abstrakcję.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Csr {
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl Csr {
    /// Buduje indeks z par `(klucz, wartość)` sortowaniem przez zliczanie:
    /// O(n + k), kolejność w obrębie klucza = kolejność wystąpienia w wejściu,
    /// czyli deterministyczna bez sortowania porównawczego.
    #[must_use]
    pub fn from_pairs(keys: usize, pairs: &[(u32, u32)]) -> Csr {
        let mut starts = vec![0u32; keys + 1];
        for &(k, _) in pairs {
            starts[k as usize + 1] += 1;
        }
        for i in 0..keys {
            starts[i + 1] += starts[i];
        }
        let mut cursor = starts.clone();
        let mut items = vec![0u32; pairs.len()];
        for &(k, v) in pairs {
            let slot = &mut cursor[k as usize];
            items[*slot as usize] = v;
            *slot += 1;
        }
        Csr { starts, items }
    }

    #[must_use]
    pub fn range(&self, key: usize) -> &[u32] {
        let a = self.starts[key] as usize;
        let b = self.starts[key + 1] as usize;
        &self.items[a..b]
    }

    #[must_use]
    pub fn keys(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    #[must_use]
    pub fn items(&self) -> &[u32] {
        &self.items
    }
}

// ---- Graf -------------------------------------------------------------------

/// Graf jednej warstwy.
#[derive(Clone, PartialEq, Debug)]
pub struct RoadGraph {
    pub modality: Modality,
    pub nodes: Vec<RoadNode>,
    pub edges: Vec<RoadEdge>,
    /// Węzeł → krawędzie wychodzące.
    pub out_edges: Csr,
    /// Węzeł → krawędzie wchodzące. Potrzebny wstecznej połowie wyszukiwania
    /// dwukierunkowego i referencyjnej Dijkstrze z testu `route_optimality`.
    pub in_edges: Csr,
    pub turns: Vec<TurnMovement>,
    /// Krawędź wjazdowa → zakres manewrów z niej wychodzących. Pusty, gdy graf
    /// zbudowano bez tabeli manewrów (`RoadGraphBuilder::with_turns(false)`).
    pub turn_index: Csr,
    pub controls: Vec<NodeControl>,
    pub bridges: Vec<Bridge>,
    /// ++ przy zmianie struktury → wyzwala rekontrakcję.
    pub topology_version: u32,
    /// ++ przy zmianie wag → wyzwala kustomizację.
    pub weight_version: u32,
}

impl RoadGraph {
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    #[must_use]
    pub fn edge(&self, e: EdgeId) -> &RoadEdge {
        &self.edges[e.0 as usize]
    }

    /// Krawędzie wychodzące z węzła.
    #[must_use]
    pub fn out(&self, n: NodeId) -> &[u32] {
        self.out_edges.range(n.0 as usize)
    }

    /// Krawędzie wchodzące do węzła.
    #[must_use]
    pub fn inc(&self, n: NodeId) -> &[u32] {
        self.in_edges.range(n.0 as usize)
    }

    /// Manewry skrętne z krawędzi wjazdowej. Pusty wycinek, gdy tabeli nie ma.
    #[must_use]
    pub fn turns_from(&self, e: EdgeId) -> &[TurnMovement] {
        if self.turn_index.keys() == 0 {
            return &[];
        }
        let r = self.turn_index.range(e.0 as usize);
        if r.is_empty() {
            return &[];
        }
        let a = r[0] as usize;
        &self.turns[a..a + r.len()]
    }

    /// Czasy przejazdu swobodnego wszystkich krawędzi, w setnych sekundy.
    #[must_use]
    pub fn free_flow_weights(&self) -> Vec<u32> {
        self.edges
            .iter()
            .map(|e| e.free_flow_cs(self.modality))
            .collect()
    }

    /// Odległość euklidesowa między węzłami w centymetrach, zaokrąglona w dół.
    /// Bez `f64` — `isqrt` na `u64` jest dokładny i identyczny na każdej platformie.
    #[must_use]
    pub fn straight_cm(&self, a: NodeId, b: NodeId) -> u32 {
        let p = self.nodes[a.0 as usize].pos_cm;
        let q = self.nodes[b.0 as usize].pos_cm;
        let dx = i64::from(p.x - q.x);
        let dy = i64::from(p.y - q.y);
        let d2 = (dx * dx + dy * dy) as u64;
        let d = d2.isqrt();
        u32::try_from(d).unwrap_or(u32::MAX)
    }
}

impl HashState for RoadGraph {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.modality as u8);
        h.write_u32(self.topology_version);
        h.write_u32(self.weight_version);
        h.write_u32(self.nodes.len() as u32);
        for n in &self.nodes {
            h.write_u32(n.pos_cm.x as u32);
            h.write_u32(n.pos_cm.y as u32);
            h.write_u32(n.z_cm as u32);
        }
        h.write_u32(self.edges.len() as u32);
        for e in &self.edges {
            h.write_u32(e.from.0);
            h.write_u32(e.to.0);
            h.write_u32(e.geometry_ref.0);
            h.write_u32(e.length_cm);
            h.write_u8(e.lanes);
            h.write_u8(e.class as u8);
            h.write_u16(e.speed_limit_dkmh);
            h.write_i64(e.max_mass.0);
            h.write_u16(e.grade_permille as u16);
            h.write_u8(e.modalities.0);
            h.write_u32(e.bridge.map_or(u32::MAX, |b| b.0));
            h.write_u16(e.curb_parking);
            h.write_u16(e.district.0);
            h.write_u32(e.twin.map_or(u32::MAX, |t| t.0));
        }
        h.write_u32(self.bridges.len() as u32);
        for b in &self.bridges {
            h.write_i64(b.max_mass.0);
            h.write_u8(u8::from(b.closed));
        }
        h.write_u32(self.turns.len() as u32);
        for t in &self.turns {
            h.write_u32(t.in_edge.0);
            h.write_u32(t.out_edge.0);
            h.write_u8(u8::from(t.banned));
            h.write_u16(t.saturation_flow_vph);
        }
    }
}

// ---- Budowa -----------------------------------------------------------------

/// Opis krawędzi przekazywany budowniczemu. Osobny typ od [`RoadEdge`], bo
/// `twin` i `modalities` wynikają z budowy, a nie z wejścia.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EdgeSpec {
    pub from: NodeId,
    pub to: NodeId,
    pub geometry_ref: GeomRef,
    pub length_cm: u32,
    pub lanes: u8,
    pub class: RoadClass,
    pub speed_limit_dkmh: u16,
    pub max_mass: Mass,
    pub grade_permille: i16,
    pub bridge: Option<BridgeId>,
    pub curb_parking: u16,
    pub district: DistrictId,
}

/// Budowniczy grafu warstwy.
///
/// Wejście jest **ogólne** (węzły i krawędzie), żeby crate nie musiał znać `RoadNetwork`:
/// adapter z geometrii M2 siedzi w `sim/world::nav_build`, a generator siatki syntetycznej
/// niżej w tym module karmi ten sam interfejs (R8 w M4 §8).
pub struct RoadGraphBuilder {
    modality: Modality,
    nodes: Vec<RoadNode>,
    edges: Vec<RoadEdge>,
    controls: Vec<NodeControl>,
    bridges: Vec<Bridge>,
    with_turns: bool,
}

impl RoadGraphBuilder {
    #[must_use]
    pub fn new(modality: Modality) -> RoadGraphBuilder {
        RoadGraphBuilder {
            modality,
            nodes: Vec::new(),
            edges: Vec::new(),
            controls: Vec::new(),
            bridges: Vec::new(),
            with_turns: true,
        }
    }

    /// Tabela manewrów kosztuje `Σ deg(v)²` pozycji — na siatce czterowlotowej
    /// 16 na węzeł. Routing jej **nie używa** (jest węzłowy, patrz `cch`), więc
    /// graf syntetyczny do benchmarków buduje się bez niej.
    #[must_use]
    pub fn with_turns(mut self, yes: bool) -> RoadGraphBuilder {
        self.with_turns = yes;
        self
    }

    pub fn reserve(&mut self, nodes: usize, edges: usize) {
        self.nodes.reserve(nodes);
        self.controls.reserve(nodes);
        self.edges.reserve(edges);
    }

    pub fn add_node(&mut self, pos_cm: IVec2, z_cm: i32) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(RoadNode { pos_cm, z_cm });
        self.controls.push(NodeControl::Uncontrolled);
        id
    }

    pub fn add_bridge(&mut self, max_mass: Mass) -> BridgeId {
        let id = BridgeId(self.bridges.len() as u32);
        self.bridges.push(Bridge {
            max_mass,
            closed: false,
        });
        id
    }

    /// Dokłada krawędź kierunkową. `twin` uzupełnia [`RoadGraphBuilder::finish`].
    pub fn add_edge(&mut self, s: EdgeSpec) -> EdgeId {
        let id = EdgeId(self.edges.len() as u32);
        self.edges.push(RoadEdge {
            from: s.from,
            to: s.to,
            geometry_ref: s.geometry_ref,
            length_cm: s.length_cm,
            lanes: s.lanes,
            class: s.class,
            speed_limit_dkmh: s.speed_limit_dkmh,
            max_mass: s.max_mass,
            grade_permille: s.grade_permille,
            modalities: self.modality.mask(),
            bridge: s.bridge,
            curb_parking: s.curb_parking,
            district: s.district,
            twin: None,
        });
        id
    }

    /// Dokłada parę krawędzi przeciwnych kierunków tej samej jezdni i wiąże je w `twin`.
    pub fn add_edge_pair(&mut self, s: EdgeSpec) -> (EdgeId, EdgeId) {
        let fwd = self.add_edge(s);
        let bwd = self.add_edge(EdgeSpec {
            from: s.to,
            to: s.from,
            grade_permille: -s.grade_permille,
            ..s
        });
        self.edges[fwd.0 as usize].twin = Some(bwd);
        self.edges[bwd.0 as usize].twin = Some(fwd);
        (fwd, bwd)
    }

    pub fn set_control(&mut self, n: NodeId, c: NodeControl) {
        self.controls[n.0 as usize] = c;
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Położenie już dodanego węzła — generatorom siatek wygodniej zapytać
    /// budowniczego niż prowadzić własną kopię współrzędnych.
    ///
    /// # Panics
    /// Gdy węzeł nie został dodany.
    #[must_use]
    pub fn node_pos(&self, n: NodeId) -> IVec2 {
        self.nodes[n.0 as usize].pos_cm
    }

    #[must_use]
    pub fn finish(self) -> RoadGraph {
        let n = self.nodes.len();
        let out_pairs: Vec<(u32, u32)> = self
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| (e.from.0, i as u32))
            .collect();
        let in_pairs: Vec<(u32, u32)> = self
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| (e.to.0, i as u32))
            .collect();
        let out_edges = Csr::from_pairs(n, &out_pairs);
        let in_edges = Csr::from_pairs(n, &in_pairs);

        let (turns, turn_index) = if self.with_turns {
            build_turns(&self.edges, &out_edges, &self.controls)
        } else {
            (Vec::new(), Csr::default())
        };

        RoadGraph {
            modality: self.modality,
            nodes: self.nodes,
            edges: self.edges,
            out_edges,
            in_edges,
            turns,
            turn_index,
            controls: self.controls,
            bridges: self.bridges,
            topology_version: 1,
            weight_version: 1,
        }
    }
}

/// Przepustowość nasycenia pasa w pojazdach na godzinę — stała podręcznikowa
/// (HCM: ~1900 poj./h/pas dla ruchu prostego). Skręt w lewo bez pasa wydzielonego
/// przepuszcza mniej, bo czeka na lukę w ruchu przeciwnym.
const SATURATION_STRAIGHT_VPH: u16 = 1900;
const SATURATION_TURN_VPH: u16 = 1400;

fn build_turns(
    edges: &[RoadEdge],
    out_edges: &Csr,
    controls: &[NodeControl],
) -> (Vec<TurnMovement>, Csr) {
    let mut turns: Vec<TurnMovement> = Vec::new();
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    for (i, e) in edges.iter().enumerate() {
        let start = turns.len() as u32;
        let node = e.to.0 as usize;
        for &o in out_edges.range(node) {
            let out = &edges[o as usize];
            // Zawracanie: wyjazd tą samą jezdnią, którą się wjechało.
            let u_turn = e.twin == Some(EdgeId(o)) || out.to == e.from;
            let straight = !u_turn && out.class.rank() >= e.class.rank();
            turns.push(TurnMovement {
                in_edge: EdgeId(i as u32),
                out_edge: EdgeId(o),
                banned: u_turn,
                priority: turn_priority(controls[node], EdgeId(i as u32)),
                saturation_flow_vph: if straight {
                    SATURATION_STRAIGHT_VPH
                } else {
                    SATURATION_TURN_VPH
                },
            });
            pairs.push((i as u32, start));
        }
    }
    (turns, Csr::from_pairs(edges.len(), &pairs))
}

fn turn_priority(c: NodeControl, in_edge: EdgeId) -> TurnPriority {
    match c {
        NodeControl::Uncontrolled => TurnPriority::Major,
        NodeControl::Signal(p) => TurnPriority::Signal((p.0 & 0xff) as u8),
        NodeControl::PrioritySigns { major } => {
            if major.contains(&in_edge) {
                TurnPriority::Major
            } else {
                TurnPriority::Yield
            }
        }
    }
}

// ---- Przesiadki między warstwami --------------------------------------------

/// Rodzaj powiązania międzymodalnego.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u8)]
pub enum TransferKind {
    /// Chodnik ↔ przystanek.
    Stop,
    /// Chodnik ↔ parking (dojście od miejsca postojowego).
    Parking,
    /// Chodnik ↔ jezdnia (wsiadanie do auta zaparkowanego przy krawężniku).
    Curb,
}

/// Jawne powiązanie węzła jednej warstwy z węzłem drugiej. Tabela, nie graf z maską —
/// dzięki temu koszt przesiadki jest w jednym miejscu i widać go w karcie podróży.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TransferLink {
    pub from: Modality,
    pub from_node: NodeId,
    pub to: Modality,
    pub to_node: NodeId,
    /// Czas przesiadki w sekundach (dojście, wsiadanie).
    pub seconds: u16,
    pub kind: TransferKind,
}

/// Komplet warstw miasta wraz z tabelą przesiadek.
#[derive(Clone, PartialEq, Debug)]
pub struct NavGraphs {
    pub layers: [RoadGraph; 4],
    pub transfers: Vec<TransferLink>,
    /// Węzeł warstwy → zakres przesiadek z niego. Indeksowany
    /// `modality.as_index() * nodes + node`, po jednym CSR na warstwę.
    pub transfer_index: [Csr; 4],
}

impl NavGraphs {
    /// Długość trasy w centymetrach — suma krawędzi wszystkich odcinków.
    ///
    /// `Route` nie niesie dystansu, bo router minimalizuje **czas** i dystans nie jest
    /// mu do niczego potrzebny. Potrzebny jest za to temu, kto płaci za tonokilometry
    /// (M6d), więc liczy się go tutaj, przy grafie, a nie kopiuje do trasy — kopia
    /// rosłaby w każdej trasie w cache'u, a używa jej jeden konsument na tysiąc.
    #[must_use]
    pub fn route_length_cm(&self, r: &crate::router::Route) -> u64 {
        r.legs
            .iter()
            .map(|l| {
                let layer = &self.layers[l.modality.as_index()];
                l.edges
                    .iter()
                    .map(|&e| u64::from(layer.edge(e).length_cm))
                    .sum::<u64>()
            })
            .sum()
    }

    #[must_use]
    pub fn new(layers: [RoadGraph; 4], transfers: Vec<TransferLink>) -> NavGraphs {
        let transfer_index = std::array::from_fn(|m| {
            let pairs: Vec<(u32, u32)> = transfers
                .iter()
                .enumerate()
                .filter(|(_, t)| t.from.as_index() == m)
                .map(|(i, t)| (t.from_node.0, i as u32))
                .collect();
            Csr::from_pairs(layers[m].node_count(), &pairs)
        });
        NavGraphs {
            layers,
            transfers,
            transfer_index,
        }
    }

    #[must_use]
    pub fn layer(&self, m: Modality) -> &RoadGraph {
        &self.layers[m.as_index()]
    }

    pub fn transfers_from(&self, m: Modality, n: NodeId) -> impl Iterator<Item = &TransferLink> {
        self.transfer_index[m.as_index()]
            .range(n.0 as usize)
            .iter()
            .map(|&i| &self.transfers[i as usize])
    }
}

impl HashState for NavGraphs {
    fn hash_state(&self, h: &mut StateHasher) {
        for l in &self.layers {
            l.hash_state(h);
        }
        h.write_u32(self.transfers.len() as u32);
        for t in &self.transfers {
            h.write_u8(t.from as u8);
            h.write_u32(t.from_node.0);
            h.write_u8(t.to as u8);
            h.write_u32(t.to_node.0);
            h.write_u16(t.seconds);
            h.write_u8(t.kind as u8);
        }
    }
}

// ---- Walidacja --------------------------------------------------------------

/// Co poszło nie tak przy budowie grafu. Błąd, nie `panic`, bo wejściem są dane
/// generatora miasta — chcemy wiedzieć **które** ziarno je złamało.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GraphError {
    /// Krawędź wskazuje węzeł spoza tablicy.
    DanglingEdge { edge: EdgeId, node: NodeId },
    /// Pętla własna — droga z węzła do niego samego.
    SelfLoop { edge: EdgeId },
    /// Krawędź jezdna bez pasa ruchu.
    NoLanes { edge: EdgeId },
    /// Most bez nośności: nie da się rozstrzygnąć przejazdu ciężarówki.
    BridgeWithoutCapacity { bridge: BridgeId },
    /// Krawędź wskazuje most spoza tablicy mostów.
    DanglingBridge { edge: EdgeId, bridge: BridgeId },
    /// Krawędź o zerowej długości — dzielnik w `free_flow_cs`.
    ZeroLength { edge: EdgeId },
}

/// Wynik walidacji — także wtedy, gdy wszystko jest w porządku: te liczby idą
/// do inspektora grafu i do dziennika.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GraphReport {
    pub nodes: u32,
    pub edges: u32,
    pub turns: u32,
    pub bridges: u32,
    /// Węzły bez żadnej krawędzi (w warstwie ich być nie musi — kolej nie dotyka
    /// większości skrzyżowań — ale liczba jest sygnałem).
    pub isolated: u32,
    /// Węzły, z których się nie wyjedzie, choć się do nich wjeżdża.
    pub sinks: u32,
    pub largest_component: u32,
    /// Największa **silnie** spójna składowa: zbiór węzłów, z których da się dojechać
    /// do każdego innego z tego zbioru **i wrócić**.
    ///
    /// To ona, a nie składowa słaba, mówi, ile par origin–cel ma w ogóle trasę.
    /// Różnica bierze się z ulic jednokierunkowych: węzeł na końcu jednokierunkowej
    /// ślepej uliczki jest słabo spójny z miastem i nieosiągalny naprawdę. Liczba
    /// mniejsza od `largest_component` jest sygnałem dla generatora sieci (M2),
    /// nie dla routera — router może tylko zwrócić `None`.
    pub largest_scc: u32,
    pub total_length_km: u32,
}

/// Sprawdza spójność grafu i zwraca metryki. Nie sprawdza dostępności parcel —
/// tego nie da się zrobić bez wiedzy o mieście, więc robi to `sim/world::nav_build`.
///
/// # Errors
/// Zwraca pierwszy napotkany [`GraphError`] w kolejności indeksów krawędzi —
/// deterministycznie, żeby dwa przebiegi tego samego ziarna raportowały to samo.
pub fn validate(g: &RoadGraph) -> Result<GraphReport, GraphError> {
    let n = g.nodes.len() as u32;
    for (i, e) in g.edges.iter().enumerate() {
        let id = EdgeId(i as u32);
        if e.from.0 >= n {
            return Err(GraphError::DanglingEdge {
                edge: id,
                node: e.from,
            });
        }
        if e.to.0 >= n {
            return Err(GraphError::DanglingEdge {
                edge: id,
                node: e.to,
            });
        }
        if e.from == e.to {
            return Err(GraphError::SelfLoop { edge: id });
        }
        if e.length_cm == 0 {
            return Err(GraphError::ZeroLength { edge: id });
        }
        if g.modality == Modality::Road && e.lanes == 0 {
            return Err(GraphError::NoLanes { edge: id });
        }
        if let Some(b) = e.bridge {
            let Some(br) = g.bridges.get(b.0 as usize) else {
                return Err(GraphError::DanglingBridge {
                    edge: id,
                    bridge: b,
                });
            };
            if br.max_mass.0 <= 0 {
                return Err(GraphError::BridgeWithoutCapacity { bridge: b });
            }
        }
    }

    let mut isolated = 0u32;
    let mut sinks = 0u32;
    for v in 0..g.nodes.len() {
        let o = g.out_edges.range(v).len();
        let i = g.in_edges.range(v).len();
        if o == 0 && i == 0 {
            isolated += 1;
        } else if o == 0 {
            sinks += 1;
        }
    }

    let total_cm: u64 = g.edges.iter().map(|e| u64::from(e.length_cm)).sum();

    Ok(GraphReport {
        nodes: n,
        edges: g.edges.len() as u32,
        turns: g.turns.len() as u32,
        bridges: g.bridges.len() as u32,
        isolated,
        sinks,
        largest_component: largest_component(g),
        largest_scc: largest_scc(g),
        total_length_km: (total_cm / 100_000) as u32,
    })
}

/// Największa **słabo** spójna składowa: BFS po krawędziach w obie strony.
/// Słabo, bo jednokierunkowa ulica nie wyklucza dojazdu — wyklucza go dopiero
/// brak trasy, a to sprawdza router, nie walidator.
fn largest_component(g: &RoadGraph) -> u32 {
    let n = g.nodes.len();
    let mut seen = vec![false; n];
    let mut best = 0u32;
    let mut queue: Vec<u32> = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        queue.clear();
        queue.push(start as u32);
        let mut size = 0u32;
        let mut head = 0usize;
        while head < queue.len() {
            let v = queue[head] as usize;
            head += 1;
            size += 1;
            for &e in g.out_edges.range(v) {
                let w = g.edges[e as usize].to.0 as usize;
                if !seen[w] {
                    seen[w] = true;
                    queue.push(w as u32);
                }
            }
            for &e in g.in_edges.range(v) {
                let w = g.edges[e as usize].from.0 as usize;
                if !seen[w] {
                    seen[w] = true;
                    queue.push(w as u32);
                }
            }
        }
        best = best.max(size);
    }
    best
}

/// Największa silnie spójna składowa — Kosaraju na iteracyjnym DFS.
///
/// Iteracyjnym, bo rekurencja na grafie 200 tys. węzłów z długimi łańcuchami
/// przepełniłaby stos; łańcuchy węzłów stopnia 2 są w sieci drogowej regułą,
/// nie wyjątkiem.
fn largest_scc(g: &RoadGraph) -> u32 {
    let n = g.nodes.len();
    if n == 0 {
        return 0;
    }
    // Faza 1: kolejność zakończenia DFS po krawędziach wychodzących.
    let mut seen = vec![false; n];
    let mut order: Vec<u32> = Vec::with_capacity(n);
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push((start as u32, 0));
        while let Some(&mut (v, ref mut i)) = stack.last_mut() {
            let out = g.out_edges.range(v as usize);
            if (*i as usize) < out.len() {
                let e = out[*i as usize];
                *i += 1;
                let w = g.edges[e as usize].to.0 as usize;
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w as u32, 0));
                }
            } else {
                order.push(v);
                stack.pop();
            }
        }
    }
    // Faza 2: DFS po krawędziach wchodzących, w odwrotnej kolejności zakończenia.
    let mut done = vec![false; n];
    let mut best = 0u32;
    let mut queue: Vec<u32> = Vec::new();
    for &root in order.iter().rev() {
        if done[root as usize] {
            continue;
        }
        done[root as usize] = true;
        queue.clear();
        queue.push(root);
        let mut size = 0u32;
        let mut head = 0usize;
        while head < queue.len() {
            let v = queue[head] as usize;
            head += 1;
            size += 1;
            for &e in g.in_edges.range(v) {
                let w = g.edges[e as usize].from.0 as usize;
                if !done[w] {
                    done[w] = true;
                    queue.push(w as u32);
                }
            }
        }
        best = best.max(size);
    }
    best
}

// ---- Siatka syntetyczna ------------------------------------------------------

/// Regularna siatka `cols × rows` skrzyżowań jako graf testowy (R8).
///
/// Istnieje, żeby testy routingu nie zależały od generatora miasta. Prędkości rosną
/// co piątą ulicę, więc trasa optymalna nie jest trywialnie geometryczna.
///
/// **Do pomiaru budżetu WP2 jej nie używać** — patrz [`synthetic_road_network`].
#[must_use]
pub fn synthetic_grid(cols: u32, rows: u32, spacing_cm: u32) -> RoadGraph {
    synthetic_road_network(cols, rows, 1, spacing_cm)
}

/// Siatka `cols × rows` skrzyżowań, w której **każda ulica jest podzielona na
/// `per_street` odcinków** — czyli między skrzyżowaniami leżą węzły stopnia 2.
///
/// To nie jest ozdoba, tylko warunek sensowności pomiaru. Koszt zapytania w hierarchii
/// kontrakcji rządzi się **szerokością drzewową** grafu, a siatka jednorodna `k × k`
/// ma ją równą `k`: separator na szczycie dysekcji zagnieżdżonej jest kliką o `k`
/// wierzchołkach. Sieć drogowa ma tę samą liczbę węzłów przy nieporównanie mniejszym
/// separatorze, bo większość jej węzłów to punkty **wzdłuż** ulicy, a nie skrzyżowania:
/// łańcuch węzłów stopnia 2 kontrahuje się bez wypełnienia. Mierzenie budżetu routingu
/// na czystej siatce mierzy więc najgorszy przypadek algorytmu, a nie przypadek, który
/// w grze wystąpi — instancja `100 × 100 × 10` ma tyle samo węzłów co `450 × 450`
/// i rdzeń mniejszy dwudziestokrotnie.
///
/// `per_street = 1` daje z powrotem czystą siatkę.
///
/// # Panics
/// Gdy `per_street == 0` albo `spacing_cm == 0` — długość odcinka musi być dodatnia.
#[must_use]
pub fn synthetic_road_network(cols: u32, rows: u32, per_street: u32, spacing_cm: u32) -> RoadGraph {
    assert!(per_street > 0, "ulica musi mieć co najmniej jeden odcinek");
    assert!(spacing_cm > 0, "rozstaw skrzyżowań musi być dodatni");
    let streets = cols * rows.saturating_sub(1) + rows * cols.saturating_sub(1);
    let sub = spacing_cm / per_street;
    let sub_len = sub.max(1);

    let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
    b.reserve(
        (cols * rows + streets * (per_street - 1)) as usize,
        (2 * streets * per_street) as usize,
    );
    for y in 0..rows {
        for x in 0..cols {
            b.add_node(
                IVec2::new((x * spacing_cm) as i32, (y * spacing_cm) as i32),
                0,
            );
        }
    }
    let junction = |x: u32, y: u32| NodeId(y * cols + x);
    let spec = |from, to, arterial: bool| EdgeSpec {
        from,
        to,
        geometry_ref: GeomRef(0),
        length_cm: sub_len,
        lanes: if arterial { 2 } else { 1 },
        class: if arterial {
            RoadClass::Arterial
        } else {
            RoadClass::Local
        },
        speed_limit_dkmh: if arterial { 600 } else { 300 },
        max_mass: Mass::ZERO,
        grade_permille: 0,
        bridge: None,
        curb_parking: 0,
        district: DistrictId(0),
    };

    // Ulica: `per_street` odcinków przez `per_street - 1` węzłów pośrednich.
    // Węzły pośrednie powstają w kolejności ulic, więc numeracja jest deterministyczna.
    let street = |b: &mut RoadGraphBuilder, a: NodeId, z: NodeId, arterial: bool| {
        let pa = b.node_pos(a);
        let pz = b.node_pos(z);
        let mut prev = a;
        for k in 1..per_street {
            let t = k as i64;
            let n = per_street as i64;
            let pos = IVec2::new(
                (i64::from(pa.x) + (i64::from(pz.x) - i64::from(pa.x)) * t / n) as i32,
                (i64::from(pa.y) + (i64::from(pz.y) - i64::from(pa.y)) * t / n) as i32,
            );
            let mid = b.add_node(pos, 0);
            b.add_edge_pair(spec(prev, mid, arterial));
            prev = mid;
        }
        b.add_edge_pair(spec(prev, z, arterial));
    };

    for y in 0..rows {
        for x in 0..cols {
            if x + 1 < cols {
                street(&mut b, junction(x, y), junction(x + 1, y), y % 5 == 0);
            }
            if y + 1 < rows {
                street(&mut b, junction(x, y), junction(x, y + 1), x % 5 == 0);
            }
        }
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csr_zachowuje_kolejnosc_wystapien() {
        let c = Csr::from_pairs(3, &[(2, 10), (0, 20), (2, 11), (0, 21)]);
        assert_eq!(c.range(0), &[20, 21]);
        assert_eq!(c.range(1), &[] as &[u32]);
        assert_eq!(c.range(2), &[10, 11]);
    }

    #[test]
    fn czas_swobodny_zgadza_sie_z_arytmetyka() {
        let g = synthetic_grid(2, 6, 10_000); // odcinki po 100 m
                                              // Rząd 0 jest arterią (600 dkmh): 100 m przy 60,0 km/h = 6,0 s = 600 setnych.
        let arteria = g.edge(EdgeId(0));
        assert_eq!(arteria.speed_limit_dkmh, 600);
        assert_eq!(arteria.free_flow_cs(Modality::Road), 600);
        // Ulica lokalna (300 dkmh): 100 m przy 30,0 km/h = 12,0 s = 1200 setnych.
        let lokalna = g
            .edges
            .iter()
            .find(|e| e.speed_limit_dkmh == 300)
            .expect("siatka ma ulice lokalne");
        assert_eq!(lokalna.free_flow_cs(Modality::Road), 1200);
        // Ta sama krawędź w warstwie pieszej idzie prędkością warstwy, nie limitem drogi.
        assert_eq!(lokalna.free_flow_cs(Modality::Foot), 10_000 * 36 / 48);
    }

    #[test]
    fn para_krawedzi_ma_przeciwne_nachylenie_i_wzajemny_twin() {
        let mut b = RoadGraphBuilder::new(Modality::Road);
        let a = b.add_node(IVec2::new(0, 0), 0);
        let c = b.add_node(IVec2::new(10_000, 0), 500);
        let (f, r) = b.add_edge_pair(EdgeSpec {
            from: a,
            to: c,
            geometry_ref: GeomRef(0),
            length_cm: 10_000,
            lanes: 1,
            class: RoadClass::Local,
            speed_limit_dkmh: 300,
            max_mass: Mass::ZERO,
            grade_permille: 50,
            bridge: None,
            curb_parking: 0,
            district: DistrictId(0),
        });
        let g = b.finish();
        assert_eq!(g.edge(f).grade_permille, 50);
        assert_eq!(g.edge(r).grade_permille, -50);
        assert_eq!(g.edge(f).twin, Some(r));
        assert_eq!(g.edge(r).twin, Some(f));
    }

    #[test]
    fn zawracanie_jest_zabronione_w_tabeli_manewrow() {
        let g = {
            let mut b = RoadGraphBuilder::new(Modality::Road);
            let n: Vec<NodeId> = (0..3)
                .map(|i| b.add_node(IVec2::new(i * 10_000, 0), 0))
                .collect();
            for i in 0..2 {
                b.add_edge_pair(EdgeSpec {
                    from: n[i],
                    to: n[i + 1],
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
                });
            }
            b.finish()
        };
        // Krawędź 0 wjeżdża do węzła 1; jedyny zakazany wyjazd to jej własny bliźniak.
        let t = g.turns_from(EdgeId(0));
        assert!(!t.is_empty());
        let zabronione: Vec<EdgeId> = t.iter().filter(|m| m.banned).map(|m| m.out_edge).collect();
        assert_eq!(zabronione, vec![g.edge(EdgeId(0)).twin.unwrap()]);
    }

    #[test]
    fn walidator_lapie_wiszaca_krawedz_i_zerowa_dlugosc() {
        let mut g = synthetic_grid(3, 3, 10_000);
        g.edges[0].length_cm = 0;
        assert_eq!(
            validate(&g),
            Err(GraphError::ZeroLength { edge: EdgeId(0) })
        );

        let mut g = synthetic_grid(3, 3, 10_000);
        g.edges[0].to = NodeId(999);
        assert!(matches!(validate(&g), Err(GraphError::DanglingEdge { .. })));
    }

    #[test]
    fn skladowa_silna_odrozniona_od_slabej_przy_jednokierunkowej() {
        // Trzy węzły: 0↔1 dwukierunkowo, 1→2 jednokierunkowo.
        // Słabo spójne są wszystkie trzy; silnie — tylko {0, 1}.
        let mut b = RoadGraphBuilder::new(Modality::Road);
        let n: Vec<NodeId> = (0..3)
            .map(|i| b.add_node(IVec2::new(i * 10_000, 0), 0))
            .collect();
        let spec = |from, to| EdgeSpec {
            from,
            to,
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
        b.add_edge_pair(spec(n[0], n[1]));
        b.add_edge(spec(n[1], n[2]));
        let r = validate(&b.finish()).expect("graf poprawny");
        assert_eq!(r.largest_component, 3, "słabo spójne są wszystkie trzy");
        assert_eq!(r.largest_scc, 2, "z węzła 2 nie da się wrócić");
        assert_eq!(r.sinks, 1);
    }

    #[test]
    fn siec_drogowa_ma_lancuchy_wezlow_stopnia_dwa() {
        let g = synthetic_road_network(10, 10, 5, 10_000);
        // 100 skrzyżowań + 180 ulic × 4 węzły pośrednie.
        assert_eq!(g.node_count(), 100 + 180 * 4);
        let r = validate(&g).expect("sieć ma być poprawna");
        assert_eq!(r.isolated, 0);
        assert_eq!(r.largest_component, g.node_count() as u32);
        // Węzeł pośredni ma dokładnie dwie krawędzie wychodzące (tam i z powrotem).
        let stopien_dwa = (0..g.node_count())
            .filter(|&v| g.out(NodeId(v as u32)).len() == 2)
            .count();
        assert!(
            stopien_dwa >= 180 * 4,
            "łańcuchów jest za mało: {stopien_dwa}"
        );
        // Długość całkowita nie zmienia się przy podziale ulicy.
        let siatka = synthetic_grid(10, 10, 10_000);
        assert_eq!(
            validate(&siatka).unwrap().total_length_km,
            r.total_length_km
        );
    }

    #[test]
    fn siatka_syntetyczna_jest_spojna_i_bez_sierot() {
        let g = synthetic_grid(20, 20, 10_000);
        let r = validate(&g).expect("siatka ma być poprawna");
        assert_eq!(r.nodes, 400);
        assert_eq!(r.isolated, 0);
        assert_eq!(r.sinks, 0);
        assert_eq!(r.largest_component, 400);
    }
}
