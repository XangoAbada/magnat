//! Router: zapytania o trasę, profile wag i spięcie CCH, ALT, cache i macierzy czasów
//! (M4a §5.1, WP2).
//!
//! Tu mieszka jedyny punkt, w którym reszta gry pyta o trasę. Warstwy poniżej
//! (`cch`, `alt`, `cache`, `matrix`) są niezależne i testowalne osobno; ten moduł
//! wybiera, która z nich odpowiada na dane zapytanie, i pilnuje **kontraktu z M6**:
//! niewykonalność trasy jest rozstrzygana **na etapie planowania**, nigdy w trakcie
//! przejazdu.

use std::sync::Arc;

use magnat_core::{Mass, Money, TransportMode};
use smallvec::SmallVec;

use crate::alt::{AltScratch, Landmarks};
use crate::cache::{CacheStats, RouteCache, RouteKey};
use crate::cch::{self, ChGraph, ChScratch, ContractionOrder};
use crate::graph::{EdgeId, Modality, NavGraphs, NodeId, RoadGraph, TransferLink};
use crate::matrix::TravelTimeMatrix;

/// Zestaw wag na **tej samej** kolejności kontrakcji. Dołożenie profilu kosztuje
/// jedną kustomizację, nie rekontrakcję — to jest powód wyboru CCH zamiast CH.
///
/// Zbiór jest **mały i zamknięty** (`D11`): regulacje M8 przełączają profil albo
/// zmieniają maskę wyłączonych krawędzi wewnątrz profilu, nigdy nie dokładają nowego.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(u8)]
pub enum RouteProfile {
    #[default]
    Passenger,
    /// Ciężarowy w godzinach obowiązywania stref i zakazów.
    HeavyDay,
    /// Ciężarowy poza godzinami zakazów (okno dostaw nocnych).
    HeavyNight,
}

impl RouteProfile {
    pub const ALL: &'static [RouteProfile] = &[
        RouteProfile::Passenger,
        RouteProfile::HeavyDay,
        RouteProfile::HeavyNight,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            RouteProfile::Passenger => "passenger",
            RouteProfile::HeavyDay => "heavy_day",
            RouteProfile::HeavyNight => "heavy_night",
        }
    }

    #[must_use]
    pub const fn is_heavy(self) -> bool {
        matches!(self, RouteProfile::HeavyDay | RouteProfile::HeavyNight)
    }
}

/// Masa odniesienia profilu ciężarowego: 24 t, czyli masa całkowita trzyosiowej
/// ciężarówki — ten sam próg, którym M2 nadaje flagę `NO_HEAVY`.
///
/// Zestaw wag profilu musi być **jeden** dla wszystkich ciężarówek (inaczej każda masa
/// byłaby osobną kustomizacją, patrz `D11`), więc wyłącza krawędzie nieprzejezdne dla
/// pojazdu odniesienia. Zapytanie o masę większą niż odniesienia jest dodatkowo
/// sprawdzane na zwróconej trasie — patrz [`NavRouter::route`].
pub const HEAVY_REFERENCE_MASS: Mass = Mass(24_000_000);

/// Waga oznaczająca „krawędź wyłączona z tego profilu". Nigdy nie jest relaksowana.
pub const EXCLUDED: u32 = u32::MAX;

/// Wagi krawędzi dla profilu. `banned` to maska regulacji miejskich (M8) — w M4
/// zaślepka: `None` znaczy „brak stref zakazu".
///
/// Krawędź jest wyłączona, gdy: most jest zamknięty (remont), tonaż krawędzi albo
/// mostu nie wystarcza pojazdowi odniesienia profilu, klasa zabrania ruchu ciężkiego,
/// albo maska regulacji ją wyłącza.
#[must_use]
pub fn profile_weights(g: &RoadGraph, profile: RouteProfile, banned: Option<&[bool]>) -> Vec<u32> {
    let heavy = profile.is_heavy();
    g.edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            if banned.is_some_and(|m| m.get(i).copied().unwrap_or(false)) {
                return EXCLUDED;
            }
            if let Some(b) = e.bridge {
                let br = g.bridges[b.0 as usize];
                if br.closed {
                    return EXCLUDED;
                }
                if heavy && br.max_mass.0 > 0 && br.max_mass.0 < HEAVY_REFERENCE_MASS.0 {
                    return EXCLUDED;
                }
            }
            if heavy {
                if e.max_mass.0 > 0 && e.max_mass.0 < HEAVY_REFERENCE_MASS.0 {
                    return EXCLUDED;
                }
                if e.class == magnat_core::RoadClass::Pedestrian {
                    return EXCLUDED;
                }
            }
            e.free_flow_cs(g.modality)
        })
        .collect()
}

/// Zapytanie o trasę.
///
/// `vehicle_class` z planu §5.1 tu nie ma: `VehicleClassId` jest typem `sim/traffic`
/// (M4 §6), którego `engine/nav` nie może znać, a w M4a nikt go nie czyta. Dokłada go
/// M4b razem z pojazdem jako encją.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RouteQuery {
    pub origin: NodeId,
    pub dest: NodeId,
    pub mode: TransportMode,
    /// 0..24 — wybiera profil czasowy i klucz cache.
    pub depart_hour_bucket: u8,
    pub profile: RouteProfile,
    /// Masa całkowita z ładunkiem — kontrola tonażu mostów.
    pub gross_mass: Mass,
}

impl RouteQuery {
    /// Zwykłe zapytanie osobowe: profil `Passenger`, masa bez znaczenia.
    #[must_use]
    pub fn passenger(
        origin: NodeId,
        dest: NodeId,
        mode: TransportMode,
        depart_hour_bucket: u8,
    ) -> RouteQuery {
        RouteQuery {
            origin,
            dest,
            mode,
            depart_hour_bucket,
            profile: RouteProfile::Passenger,
            gross_mass: Mass::ZERO,
        }
    }

    /// Zapytanie towarowe: profil ciężki, masa całkowita z ładunkiem.
    ///
    /// Osobny konstruktor, a nie literał struktury, z tego samego powodu co
    /// [`RouteQuery::passenger`]: profil i masa muszą iść **razem**. Profil ciężki
    /// bez masy przepuściłby most o za małym tonażu, a masa bez profilu pojechałaby
    /// wagami ruchu osobowego, czyli przez strefę zakazu ruchu ciężkiego.
    #[must_use]
    pub fn freight(
        origin: NodeId,
        dest: NodeId,
        gross_mass: Mass,
        depart_hour_bucket: u8,
        night: bool,
    ) -> RouteQuery {
        RouteQuery {
            origin,
            dest,
            mode: TransportMode::Freight,
            depart_hour_bucket,
            profile: if night {
                RouteProfile::HeavyNight
            } else {
                RouteProfile::HeavyDay
            },
            gross_mass,
        }
    }

    #[must_use]
    fn cache_key(&self) -> RouteKey {
        RouteKey {
            origin: self.origin,
            dest: self.dest,
            mode: self.mode,
            hour_bucket: self.depart_hour_bucket,
            profile: self.profile,
            // W górę: kubełek ma **obejmować** masę zapytania, a nie ją przycinać.
            gross_t: ((self.gross_mass.0.max(0) + 999_999) / 1_000_000).min(65_535) as u16,
        }
    }
}

/// Odcinek jednomodalny trasy. Przesiadki rozdzielają odcinki.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RouteLeg {
    pub mode: TransportMode,
    pub modality: Modality,
    pub edges: Vec<EdgeId>,
    pub entry_transfer: Option<TransferLink>,
}

/// Trasa gotowa do przejechania.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Route {
    pub mode: TransportMode,
    pub legs: SmallVec<[RouteLeg; 4]>,
    /// Suma z profilu godzinowego w chwili planowania.
    pub planned_minutes: u16,
    /// Paliwo, bilet, parking wg profilu. W M4a zawsze `Money::ZERO` — cennik
    /// wchodzi z paliwem (M4b/WP5) i taryfą (M4c/WP10), a zerowa kwota jest tu
    /// uczciwsza niż zmyślona.
    pub planned_cost: Money,
    /// Koszt w setnych sekundy — to, co naprawdę minimalizował router.
    pub cost_cs: u32,
}

impl Route {
    /// Liczba krawędzi we wszystkich odcinkach.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.legs.iter().map(|l| l.edges.len()).sum()
    }
}

/// Zaokrąglenie setnych sekundy do pełnych minut, w górę od połowy.
#[must_use]
fn cs_to_minutes(cs: u32) -> u16 {
    let m = (u64::from(cs) + 3_000) / 6_000;
    u16::try_from(m).unwrap_or(u16::MAX)
}

/// Wynik pojedynczego zapytania — do inspekcji i do testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RouteStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cch_queries: u64,
    pub alt_queries: u64,
    /// Zapytania, które spadły na Dijkstrę z predykatem masy (patrz `D11`).
    pub mass_fallbacks: u64,
    /// Zapytania bez trasy.
    pub infeasible: u64,
}

/// Router odpowiada na zapytania trasowe.
///
/// **Sygnatura odbiega od §5.1 i to jest świadome.** Plan mówi `fn route(&self, req)`,
/// ale odpowiedź wymaga `&mut`: bufory wyszukiwania są wielokrotnego użytku (bez tego
/// każde zapytanie zerowałoby tablicę wielkości grafu i budżet z §7.3 by przepadł),
/// a cache tras z definicji się zmienia przy trafieniu. Mutex na tej ścieżce zjadłby
/// dokładnie ten budżet, którego broni. API równoległe (sesja per wątek z osobnym
/// zestawem buforów) zaprojektuje M4b, kiedy będzie miał konsumenta, który go potrzebuje.
pub trait Router {
    /// `None` = brak trasy spełniającej ograniczenia. Zwracane **na etapie planowania**,
    /// nigdy jako porażka w trakcie przejazdu (kontrakt z M6).
    fn route(&mut self, req: &RouteQuery) -> Option<Arc<Route>>;
}

/// Komplet struktur routingu miasta.
pub struct NavRouter {
    graphs: NavGraphs,
    order: ContractionOrder,
    /// Po jednym zestawie wag na profil, na tej samej kolejności kontrakcji.
    ch: [ChGraph; 3],
    weights: [Vec<u32>; 3],
    /// Wagi swobodne warstw niedrogowych, policzone raz. Liczenie ich przy każdym
    /// zapytaniu byłoby alokacją wielkości grafu na jedno dojście do sklepu.
    layer_weights: [Vec<u32>; 4],
    landmarks: [Option<Landmarks>; 4],
    cache: RouteCache<Arc<Route>>,
    matrix: TravelTimeMatrix,
    ch_scratch: ChScratch,
    alt_scratch: AltScratch,
    stats: RouteStats,
}

/// Liczba punktów orientacyjnych ALT na warstwę pieszą i rowerową.
///
/// Osiem to standardowy kompromis: heurystyka jest wyraźnie ostrzejsza od euklidesowej,
/// a koszt to `8 × 2 × n` słów pamięci. Trasy piesze są krótkie (M3 tnie je na 5 km),
/// więc większa liczba nie ma czego przyspieszyć.
pub const LANDMARK_COUNT: usize = 8;

impl NavRouter {
    /// Buduje kolejność kontrakcji, trzy zestawy wag i punkty orientacyjne warstw
    /// pieszej i rowerowej.
    ///
    /// `cache_capacity` zaokrągla się w górę do potęgi dwójki razy liczba dróg
    /// w kubełku — patrz [`RouteCache`].
    #[must_use]
    pub fn build(graphs: NavGraphs, n_districts: u16, cache_capacity: usize) -> NavRouter {
        let road = graphs.layer(Modality::Road);
        let order = cch::build_order(road);
        let weights: [Vec<u32>; 3] =
            std::array::from_fn(|i| profile_weights(road, RouteProfile::ALL[i], None));
        let ch: [ChGraph; 3] = std::array::from_fn(|i| {
            let mut c = cch::contract(road, &order);
            c.customize(road, &weights[i], road.weight_version);
            c
        });
        let layer_weights: [Vec<u32>; 4] =
            std::array::from_fn(|m| graphs.layers[m].free_flow_weights());
        let landmarks = std::array::from_fn(|m| {
            let layer = &graphs.layers[m];
            if matches!(Modality::ALL[m], Modality::Foot | Modality::Bike) && layer.edge_count() > 0
            {
                Some(Landmarks::build(layer, &layer_weights[m], LANDMARK_COUNT))
            } else {
                None
            }
        });
        let topology = road.topology_version;
        let mut cache = RouteCache::new(cache_capacity);
        cache.invalidate(topology);
        NavRouter {
            graphs,
            order,
            ch,
            weights,
            layer_weights,
            landmarks,
            cache,
            matrix: TravelTimeMatrix::new(n_districts),
            ch_scratch: ChScratch::new(),
            alt_scratch: AltScratch::new(),
            stats: RouteStats::default(),
        }
    }

    #[must_use]
    pub fn graphs(&self) -> &NavGraphs {
        &self.graphs
    }

    #[must_use]
    pub fn order(&self) -> &ContractionOrder {
        &self.order
    }

    #[must_use]
    pub fn matrix(&self) -> &TravelTimeMatrix {
        &self.matrix
    }

    pub fn matrix_mut(&mut self) -> &mut TravelTimeMatrix {
        &mut self.matrix
    }

    #[must_use]
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    #[must_use]
    pub fn stats(&self) -> RouteStats {
        self.stats
    }

    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.ch.iter().map(ChGraph::memory_bytes).sum::<usize>()
            + self
                .landmarks
                .iter()
                .filter_map(|l| l.as_ref().map(Landmarks::memory_bytes))
                .sum::<usize>()
            + self.matrix.memory_bytes()
    }

    /// Warstwa, po której jedzie dany środek transportu.
    #[must_use]
    pub const fn modality_of(mode: TransportMode) -> Modality {
        match mode {
            TransportMode::Walk => Modality::Foot,
            TransportMode::Bicycle => Modality::Bike,
            TransportMode::Rail | TransportMode::Tram => Modality::Rail,
            // Autobus jedzie jezdnią razem z resztą ruchu.
            TransportMode::Car | TransportMode::Bus | TransportMode::Freight => Modality::Road,
        }
    }

    /// Przelicza wagi profilu od nowa i przepuszcza je przez kustomizację.
    /// Kolejność kontrakcji się nie zmienia — to jest **tania** ścieżka reakcji
    /// na remont mostu, zmianę limitu prędkości albo nową strefę zakazu.
    pub fn customize(
        &mut self,
        profile: RouteProfile,
        banned: Option<&[bool]>,
        weight_version: u32,
    ) {
        let road = &self.graphs.layers[Modality::Road.as_index()];
        let i = profile.as_index();
        self.weights[i] = profile_weights(road, profile, banned);
        self.ch[i].customize(road, &self.weights[i], weight_version);
        self.cache.clear();
    }

    /// Pełna rekontrakcja po zmianie topologii: nowa kolejność, nowe skróty,
    /// wszystkie trzy profile przeliczone na nowo. **Droga** ścieżka — patrz
    /// [`crate::rebuild`], który rozkłada ją na ticki z deterministycznym budżetem.
    pub fn recontract(&mut self) {
        self.layer_weights = std::array::from_fn(|m| self.graphs.layers[m].free_flow_weights());
        let road = &self.graphs.layers[Modality::Road.as_index()];
        self.order = cch::build_order(road);
        for i in 0..3 {
            self.weights[i] = profile_weights(road, RouteProfile::ALL[i], None);
            let mut c = cch::contract(road, &self.order);
            c.customize(road, &self.weights[i], road.weight_version);
            self.ch[i] = c;
        }
        self.cache.invalidate(road.topology_version);
    }

    /// Obserwacja realnego czasu przejazdu — wejście do [`TravelTimeMatrix`].
    /// W M4a wywołuje ją test i podgląd; w M4b robi to rozliczenie podróży.
    pub fn observe_trip(
        &mut self,
        from: magnat_core::DistrictId,
        to: magnat_core::DistrictId,
        hour: u8,
        mode: TransportMode,
        minutes: u16,
    ) {
        self.matrix.observe(from, to, hour, mode, minutes);
    }

    /// Trasa bez cache — ścieżka, którą idzie każde nietrafione zapytanie.
    fn compute(&mut self, req: &RouteQuery) -> Option<Route> {
        let modality = NavRouter::modality_of(req.mode);
        let layer = &self.graphs.layers[modality.as_index()];
        if req.origin.0 as usize >= layer.node_count() || req.dest.0 as usize >= layer.node_count()
        {
            return None;
        }

        let (cost, edges) = if modality == Modality::Road {
            let p = req.profile.as_index();
            self.stats.cch_queries += 1;
            let found = self.ch[p].query_path(req.origin, req.dest, &mut self.ch_scratch);
            match found {
                None => return None,
                Some((c, e)) if self.mass_ok(layer, &e, req.gross_mass) => (c, e),
                Some(_) => {
                    // Trasa profilu nie unosi tego ładunku. Rzadki przypadek:
                    // wagi profilu są liczone dla pojazdu odniesienia, a ta ciężarówka
                    // jest cięższa. Idziemy Dijkstrą z predykatem — wolniej, ale bez
                    // kosztu stałego czwartego profilu (`D11`).
                    self.stats.mass_fallbacks += 1;
                    let w = self.mass_filtered_weights(modality, req);
                    cch::dijkstra(layer, &w, req.origin, req.dest)?
                }
            }
        } else {
            self.stats.alt_queries += 1;
            let m = modality.as_index();
            let lm = self.landmarks[m].as_ref();
            crate::alt::route(
                layer,
                &self.layer_weights[m],
                lm,
                req.origin,
                req.dest,
                &mut self.alt_scratch,
            )?
        };

        let leg = RouteLeg {
            mode: req.mode,
            modality,
            edges,
            entry_transfer: None,
        };
        Some(Route {
            mode: req.mode,
            legs: SmallVec::from_iter([leg]),
            planned_minutes: cs_to_minutes(cost),
            planned_cost: Money::ZERO,
            cost_cs: cost,
        })
    }

    /// Czy trasa unosi ładunek? Sprawdza tonaż krawędzi i mostów.
    fn mass_ok(&self, layer: &RoadGraph, edges: &[EdgeId], gross: Mass) -> bool {
        if gross.0 <= 0 {
            return true;
        }
        edges.iter().all(|&e| {
            let edge = layer.edge(e);
            if edge.max_mass.0 > 0 && edge.max_mass.0 < gross.0 {
                return false;
            }
            match edge.bridge {
                None => true,
                Some(b) => {
                    let br = layer.bridges[b.0 as usize];
                    !br.closed && (br.max_mass.0 <= 0 || br.max_mass.0 >= gross.0)
                }
            }
        })
    }

    /// Wagi profilu z dodatkowym wyłączeniem krawędzi, które nie unoszą tej masy.
    fn mass_filtered_weights(&self, modality: Modality, req: &RouteQuery) -> Vec<u32> {
        let layer = &self.graphs.layers[modality.as_index()];
        let base = &self.weights[req.profile.as_index()];
        base.iter()
            .enumerate()
            .map(|(i, &w)| {
                if w == EXCLUDED {
                    return EXCLUDED;
                }
                if self.mass_ok(layer, &[EdgeId(i as u32)], req.gross_mass) {
                    w
                } else {
                    EXCLUDED
                }
            })
            .collect()
    }
}

impl Router for NavRouter {
    fn route(&mut self, req: &RouteQuery) -> Option<Arc<Route>> {
        let key = req.cache_key();
        if let Some(hit) = self.cache.get(&key) {
            self.stats.cache_hits += 1;
            return Some(Arc::clone(hit));
        }
        self.stats.cache_misses += 1;
        match self.compute(req) {
            None => {
                self.stats.infeasible += 1;
                None
            }
            Some(r) => {
                let a = Arc::new(r);
                self.cache.insert(key, Arc::clone(&a));
                Some(a)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{synthetic_grid, RoadGraphBuilder};

    fn graphs_from(road: RoadGraph) -> NavGraphs {
        let pusty = |m: Modality| RoadGraphBuilder::new(m).with_turns(false).finish();
        NavGraphs::new(
            [
                road,
                pusty(Modality::Foot),
                pusty(Modality::Bike),
                pusty(Modality::Rail),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn trasa_z_cache_jest_identyczna_z_policzona() {
        let g = synthetic_grid(15, 15, 10_000);
        let mut r = NavRouter::build(graphs_from(g), 1, 256);
        let q = RouteQuery::passenger(NodeId(0), NodeId(224), TransportMode::Car, 7);
        let a = r.route(&q).expect("trasa istnieje");
        let b = r.route(&q).expect("trafienie w cache");
        assert_eq!(a, b);
        assert_eq!(r.stats().cache_hits, 1);
        assert_eq!(r.stats().cache_misses, 1);
    }

    #[test]
    fn koszt_trasy_zgadza_sie_z_suma_wag_krawedzi() {
        let g = synthetic_grid(12, 12, 10_000);
        let w = g.free_flow_weights();
        let mut r = NavRouter::build(graphs_from(g), 1, 64);
        for cel in [17u32, 61, 143] {
            let q = RouteQuery::passenger(NodeId(3), NodeId(cel), TransportMode::Car, 8);
            let t = r.route(&q).expect("trasa istnieje");
            let suma: u32 = t.legs[0].edges.iter().map(|e| w[e.0 as usize]).sum();
            assert_eq!(suma, t.cost_cs);
            assert_eq!(t.planned_minutes, cs_to_minutes(t.cost_cs));
        }
    }

    #[test]
    fn profil_ciezki_omija_most_o_za_malej_nosnosci() {
        // Dwie równoległe drogi z A do B: krótsza przez most 10 t, dłuższa bez mostu.
        use crate::graph::{EdgeSpec, GeomRef};
        use magnat_core::{DistrictId, RoadClass};
        let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
        let a = b.add_node(magnat_core::IVec2::new(0, 0), 0);
        let m = b.add_node(magnat_core::IVec2::new(10_000, 0), 0);
        let z = b.add_node(magnat_core::IVec2::new(20_000, 0), 0);
        let objazd = b.add_node(magnat_core::IVec2::new(10_000, 40_000), 0);
        let most = b.add_bridge(Mass(10_000_000)); // 10 t
        let spec = |from, to, len, bridge| EdgeSpec {
            from,
            to,
            geometry_ref: GeomRef(0),
            length_cm: len,
            lanes: 1,
            class: RoadClass::Collector,
            speed_limit_dkmh: 500,
            max_mass: Mass::ZERO,
            grade_permille: 0,
            bridge,
            curb_parking: 0,
            district: DistrictId(0),
        };
        b.add_edge_pair(spec(a, m, 10_000, Some(most)));
        b.add_edge_pair(spec(m, z, 10_000, None));
        b.add_edge_pair(spec(a, objazd, 41_000, None));
        b.add_edge_pair(spec(objazd, z, 41_000, None));
        let mut r = NavRouter::build(graphs_from(b.finish()), 1, 64);

        let osobowy = r
            .route(&RouteQuery::passenger(a, z, TransportMode::Car, 9))
            .expect("osobowy przejedzie mostem");
        assert_eq!(osobowy.edge_count(), 2, "osobowy jedzie krótszą trasą");

        let ciezki = r
            .route(&RouteQuery {
                origin: a,
                dest: z,
                mode: TransportMode::Freight,
                depart_hour_bucket: 9,
                profile: RouteProfile::HeavyDay,
                gross_mass: Mass(30_000_000), // 30 t
            })
            .expect("ciężki ma objazd");
        assert_eq!(ciezki.edge_count(), 2, "objazd też ma dwie krawędzie");
        assert!(
            ciezki.cost_cs > osobowy.cost_cs,
            "objazd musi być dłuższy: {} vs {}",
            ciezki.cost_cs,
            osobowy.cost_cs
        );
    }

    #[test]
    fn zamkniety_most_odbiera_trase_po_kustomizacji() {
        use crate::graph::{EdgeSpec, GeomRef};
        use magnat_core::{DistrictId, RoadClass};
        let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
        let a = b.add_node(magnat_core::IVec2::new(0, 0), 0);
        let z = b.add_node(magnat_core::IVec2::new(10_000, 0), 0);
        let most = b.add_bridge(Mass(40_000_000));
        b.add_edge_pair(EdgeSpec {
            from: a,
            to: z,
            geometry_ref: GeomRef(0),
            length_cm: 10_000,
            lanes: 1,
            class: RoadClass::Local,
            speed_limit_dkmh: 300,
            max_mass: Mass::ZERO,
            grade_permille: 0,
            bridge: Some(most),
            curb_parking: 0,
            district: DistrictId(0),
        });
        let mut graphs = graphs_from(b.finish());
        let mut r = NavRouter::build(graphs.clone(), 1, 64);
        assert!(r
            .route(&RouteQuery::passenger(a, z, TransportMode::Car, 7))
            .is_some());

        // Remont: most zamknięty → kustomizacja, bez rekontrakcji.
        graphs.layers[Modality::Road.as_index()].bridges[0].closed = true;
        graphs.layers[Modality::Road.as_index()].weight_version += 1;
        let mut r = NavRouter::build(graphs, 1, 64);
        assert!(
            r.route(&RouteQuery::passenger(a, z, TransportMode::Car, 7))
                .is_none(),
            "zamknięty most nie ma trasy"
        );
    }

    #[test]
    fn brak_trasy_gdy_cel_poza_grafem() {
        let g = synthetic_grid(5, 5, 10_000);
        let mut r = NavRouter::build(graphs_from(g), 1, 16);
        assert!(r
            .route(&RouteQuery::passenger(
                NodeId(0),
                NodeId(9_999),
                TransportMode::Car,
                7
            ))
            .is_none());
        assert_eq!(r.stats().infeasible, 1);
    }
}
