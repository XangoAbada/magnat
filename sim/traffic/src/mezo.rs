//! Warstwa mezo — **jedyne źródło prawdy ekonomicznej ruchu** (M4b/WP3, M4d §5.4).
//!
//! Mezo działa zawsze i dla każdej krawędzi, niezależnie od tego, gdzie patrzy kamera.
//! Mikro (M4d) niczego tu nie zastąpi — dołoży się dla krawędzi widocznych i nie będzie
//! miało prawa zapisu. Argument jest w §5.4 i sprowadza się do jednego zdania: zbiór
//! krawędzi w LOD Mikro zależy od kamery, kamera nie wchodzi do hasha stanu, więc gdyby
//! mikro wpływało na wynik, obrót kamerą zmieniałby saldo gospodarstwa domowego.
//!
//! **We wzorach tego modułu nie ma ani jednego floata.** `mean_speed_dkmh` jest
//! skwantowane, zanim gdziekolwiek wejdzie, długości są w centymetrach, paliwo
//! w mikrolitrach, pieniądz w groszach. Granicą jest sygnatura [`settle_edge`]
//! i jest to granica, której da się pilnować w przeglądzie kodu (ryzyko R9).
//!
//! ## Dlaczego paliwo w mikrolitrach
//!
//! Krawędź metropolii M2 ma średnio ~40 m. Pojazd palący 6,2 l/100 km zużywa na niej
//! 2,5 ml. W mililitrach ta liczba zaokrągla się do 2 albo 3 i flota pali kilkanaście
//! procent obok — a `settle_edge` jest wołane ~8 razy na pojazd na minutę, więc błąd
//! kumuluje się w każdej podróży. Jednostką wewnętrzną baku i ledgera jest więc
//! **mikrolitr**; `Volume` (mililitry, 00 §2) pojawia się dopiero na granicy modułu.

use crate::spec::{VdfTable, VehicleCatalog, VehicleClassId};
use magnat_core::{HashState, Mass, Money, SimMinute, StateHasher};
use magnat_nav::{EdgeId, NodeControl, RoadEdge, RoadGraph, TurnPriority};

/// Ile mikrolitrów ma mililitr.
pub const UL_PER_ML: i64 = 1_000;

/// Setne sekundy w minucie.
pub const CS_PER_MINUTE: u64 = 6_000;

/// Długość cyklu sygnalizacji stałoczasowej (D10: sygnalizacja jest stałoczasowa,
/// plany siedzą w `data/`; adaptacyjna jest polityką miejską i należy do M8).
pub const SIGNAL_CYCLE_S: u32 = 90;

/// Udział zielonego dla jednego wlotu przy sygnalizacji dwufazowej.
pub const SIGNAL_GREEN_S: u32 = 40;

/// Odstęp między pojazdami odjeżdżającymi z kolejki, w setnych sekundy.
pub const HEADWAY_CS: u32 = 200;

/// Maksymalne opóźnienie węzłowe, jakie model zwróci — bez tego kolejka na wlocie
/// podporządkowanym potrafiłaby rosnąć w nieskończoność i podróż nigdy by się nie skończyła.
pub const MAX_NODE_DELAY_CS: u32 = 20 * 100;

/// Stan krawędzi w bieżącej minucie. Zamrażany na początku minuty i **nie zmieniany
/// w jej trakcie** — dzięki temu kolejność przetwarzania pojazdów w obrębie minuty
/// nie wpływa na czas przejazdu żadnego z nich.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LinkState {
    pub inflow_last_min: u16,
    /// **Jedyna** prędkość wchodząca do wzorów. Skwantowana do dekakm/h.
    pub mean_speed_dkmh: u16,
    pub free_flow_dkmh: u16,
    pub capacity_vpm: u16,
    pub queue_len_cm: u32,
}

/// Stan kolejki na krawędzi. `occupancy` to liczba pojazdów fizycznie na niej
/// obecnych; gdy dobije do `storage_capacity`, wjazd jest wstrzymany i kolejka
/// propaguje się w górę (spillback).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EdgeQueue {
    pub occupancy: u16,
    pub storage_capacity: u16,
    /// Ile wjazdów odrzucono w tej minucie — wejście nakładki korków i licznik ryzyka R4.
    pub rejected_last_min: u16,
    /// Ile pojazdów zjechało z krawędzi w tej minucie.
    pub departures_this_min: u16,
    /// Czy krawędź w ogóle wstrzymuje wjazd po zapełnieniu.
    ///
    /// Sieć M2 ma setki odcinków krótszych od długości jednego pojazdu — to kikuty
    /// przy skrzyżowaniach, a nie ulice. Kolejka na sześciometrowym odcinku jest
    /// **fikcją geometryczną**: fizycznie stoi ona na ulicy przed nim. Wstrzymywanie
    /// na nich wjazdu tworzy przewężenia, których w mieście nie ma, i to one — a nie
    /// realne arterie — okazały się najwęższym gardłem pierwszego pomiaru.
    /// Takie krawędzie zliczają obłożenie (bilans pojazdów zostaje domknięty),
    /// ale nigdy nie odmawiają wjazdu.
    pub blocks: bool,
    /// Ile kolejnych minut krawędź jest pełna i **nic** z niej nie zjechało.
    ///
    /// To, a nie czas oczekiwania pojedynczego pojazdu, jest sygnałem zakleszczenia
    /// (ryzyko R4). Zwykła kolejka przed przewężeniem potrafi trwać kwadrans i jest
    /// zjawiskiem pożądanym — rozplątywanie jej „bo długo czeka" skasowałoby korek,
    /// czyli dokładnie to, co faza ma pokazać.
    pub stuck_minutes: u16,
}

impl EdgeQueue {
    /// Czy krawędź jest zapełniona **i** jest na tyle długa, żeby to znaczyło korek.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.blocks && self.occupancy >= self.storage_capacity
    }
}

/// Stan węzła: ile pojazdów przez niego przejechało w poprzedniej minucie.
/// Z tego bierze się opóźnienie na wlotach podporządkowanych.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct NodeState {
    pub arrivals_last_min: u16,
    pub arrivals_this_min: u16,
}

/// Jeden wiersz rejestru przejazdu: co się stało na jednej krawędzi.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LedgerEntry {
    pub edge: EdgeId,
    pub entry: SimMinute,
    pub exit: SimMinute,
    /// Dokładny czas przejazdu w setnych sekundy. `entry`/`exit` są jego zaokrągleniem
    /// do minut; akumuluje się **ten** czas, nie zaokrąglony, inaczej 166 krawędzi
    /// podróży dokładałoby do 83 minut błędu zaokrąglenia.
    pub travel_cs: u32,
    pub mean_speed_dkmh: u16,
    /// Zatrzymania na węźle wejściowym.
    pub stops: u8,
    /// Paliwo w **mikrolitrach** (patrz nagłówek modułu).
    pub fuel_ul: i64,
    /// Opłata: bilet, parking, myto. Dla zwykłej krawędzi 0 — paliwo płaci się
    /// na stacji, nie na drodze.
    pub money: Money,
    /// Opóźnienie na węźle **wjazdowym** tej krawędzi: sygnalizacja, pierwszeństwo,
    /// kolejka ruchu skrętnego. Wypełnia je `advance`, bo dopiero ono zna manewr.
    pub node_delay_cs: u32,
    /// Czas spędzony w kolejce oczekujących na **wjazd** na tę krawędź (spillback).
    ///
    /// Te dwa pola istnieją, bo bez nich karta inspekcji liczy czas kolejki jako resztę
    /// z odejmowania i wrzuca do niej opóźnienie sygnalizacji — a to są dwie różne
    /// naprawy (`N-6`). Agregat w `TrafficStats` ma ten rozbiór od M4b; tutaj jest ten
    /// sam rozbiór **na jedną podróż**, czyli w rozdzielczości, w której patrzy gracz.
    pub blocked_cs: u32,
}

/// Pojazd tak, jak widzi go wzór kosztu: klasa z katalogu plus katalog dla mnożników.
#[derive(Clone, Copy)]
pub struct VehicleSpecRef<'a> {
    pub cat: &'a VehicleCatalog,
    pub class: VehicleClassId,
}

/// **Jedyne** miejsce, w którym powstaje czas przejazdu i zużycie paliwa (§5.4).
///
/// Funkcja czysta: te same argumenty → ten sam wynik, zawsze i w każdym LOD.
/// `entry_cs` to absolutny czas wjazdu w setnych sekundy; minuty w wyniku są jego
/// zaokrągleniem, ale sumuje się `travel_cs`.
#[must_use]
pub fn settle_edge(
    edge: &RoadEdge,
    link: &LinkState,
    veh: &VehicleSpecRef<'_>,
    load: Mass,
    entry_cs: u64,
    stops_at_entry_node: u8,
    cold_start: bool,
) -> LedgerEntry {
    let spec = veh.cat.spec(veh.class);
    let v = link.mean_speed_dkmh.max(1);
    let dist_cm = u64::from(edge.length_cm.max(1));

    // Czas: cm × 36 / dekakm/h = setne sekundy. Ta sama arytmetyka, której używa
    // `RoadEdge::free_flow_cs` na wagi routera — inna dałaby plan niezgodny z przejazdem.
    let travel_cs = (dist_cm * 36 / u64::from(v))
        .max(1)
        .min(u64::from(u32::MAX)) as u32;
    let exit_cs =
        entry_cs + u64::from(travel_cs) + u64::from(stops_at_entry_node) * u64::from(HEADWAY_CS);

    // Paliwo. Baza w mikrolitrach: ml/100 km × cm / 10 000 = µl.
    let mut fuel_ul = i64::from(spec.base_ml_per_100km) * (dist_cm as i64) / 10_000;
    fuel_ul = mul_permille(fuel_ul, veh.cat.speed_factor(link.mean_speed_dkmh));
    fuel_ul = mul_permille(
        fuel_ul,
        load_factor_permille(veh.cat, spec.kerb_mass_g, load),
    );
    fuel_ul = mul_permille(fuel_ul, grade_factor_permille(veh.cat, edge.grade_permille));
    fuel_ul += i64::from(stops_at_entry_node) * i64::from(spec.idle_ml_per_stop) * UL_PER_ML;
    if cold_start {
        fuel_ul += i64::from(spec.cold_start_ml) * UL_PER_ML;
    }

    LedgerEntry {
        edge: EdgeId(0),
        entry: SimMinute(entry_cs / CS_PER_MINUTE),
        exit: SimMinute(exit_cs / CS_PER_MINUTE),
        travel_cs,
        mean_speed_dkmh: link.mean_speed_dkmh,
        stops: stops_at_entry_node,
        fuel_ul,
        money: Money::ZERO,
        // Wypełnia je warstwa wyżej: `settle_edge` widzi jedną krawędź, a opóźnienie
        // węzła i czekanie na wjazd są własnością **przejścia** między krawędziami.
        node_delay_cs: 0,
        blocked_cs: 0,
    }
}

/// Opóźnienie i liczba zatrzymań na węźle wejściowym krawędzi (§5.4 punkt 2).
///
/// Model jest prosty i taki ma być w M4b: sygnalizacja stałoczasowa daje średnie
/// oczekiwanie równe połowie czerwonego, wlot podporządkowany — karę rosnącą
/// z natężeniem na wlotach głównych. Gap acceptance, fazy i ronda należą do WP8.
///
/// `ponytail:` sufit nazwany — opóźnienie nie zależy od nasycenia ruchu skrętnego,
/// tylko od liczby pojazdów przez węzeł. Ścieżka wyjścia: `TurnMovement.saturation_flow_vph`
/// jest już w grafie i wzór Webstera wchodzi bez zmiany sygnatury, gdy WP8 zmierzy,
/// że to widać.
#[must_use]
pub fn settle_node(
    control: NodeControl,
    priority: TurnPriority,
    node: &NodeState,
    entry_cs: u64,
) -> (u64, u8) {
    let ruch = u32::from(node.arrivals_last_min);
    let (delay_cs, stops) = match (control, priority) {
        (NodeControl::Uncontrolled, _) => (0, 0),
        (NodeControl::Signal(_), _) => {
            // Opóźnienie równomierne (Webster, człon pierwszy): pojazd przyjeżdża
            // w losowej chwili cyklu, więc średnia strata to `czerwone² / (2 × cykl)`,
            // a nie połowa czerwonego — połowę czeka tylko ten, kto trafił na sam
            // początek czerwonego. Dla cyklu 90 s i zielonego 40 s daje to 13,9 s.
            let red = SIGNAL_CYCLE_S.saturating_sub(SIGNAL_GREEN_S);
            let base = red * red * 100 / (2 * SIGNAL_CYCLE_S.max(1));
            (base + ruch.min(20) * HEADWAY_CS / 8, 1)
        }
        (NodeControl::PrioritySigns { .. }, TurnPriority::Major) => (0, 0),
        (NodeControl::PrioritySigns { .. }, _) => {
            // Wlot podporządkowany: czeka tym dłużej, im gęściej na głównej.
            // Trzy dziesiąte sekundy na pojazd, który przejechał tędy w poprzedniej
            // minucie — przy pustej głównej wjazd jest natychmiastowy.
            (ruch.min(20) * 30, u8::from(ruch > 0))
        }
    };
    let delay_cs = delay_cs.min(MAX_NODE_DELAY_CS);
    (entry_cs + u64::from(delay_cs), stops)
}

/// Mnożnik obciążenia: 100 % masy własnej dokłada `load_slope_permille` promili.
#[must_use]
pub fn load_factor_permille(cat: &VehicleCatalog, kerb_mass_g: i64, load: Mass) -> u32 {
    if kerb_mass_g <= 0 || load.0 <= 0 {
        return 1_000;
    }
    let extra = (load.0.saturating_mul(i64::from(cat.load_slope_permille()))) / kerb_mass_g;
    (1_000 + extra.clamp(0, 4_000)) as u32
}

/// Mnożnik nachylenia, przycięty z dołu — zjazd nie zwraca paliwa bez końca.
#[must_use]
pub fn grade_factor_permille(cat: &VehicleCatalog, grade_permille: i16) -> u32 {
    let d = i64::from(grade_permille) * i64::from(cat.grade_slope_permille());
    let v = 1_000 + d;
    v.clamp(i64::from(cat.grade_floor_permille()), 8_000) as u32
}

/// Mnożenie przez promile z jawnym zaokrąglaniem (00 §2 — nigdy niejawne obcinanie).
#[inline]
#[must_use]
pub fn mul_permille(v: i64, permille: u32) -> i64 {
    Money(v).mul_ratio(i64::from(permille), 1_000).0
}

/// Stan sieci mezo: po jednym wpisie na krawędź i na węzeł warstwy drogowej.
///
/// To jest **stan trwały** symulacji ruchu i wchodzi do hasha (§5.9 punkt 7).
/// Tablice są gęste i indeksowane `EdgeId`/`NodeId` — iteracja idzie po indeksie
/// rosnąco, więc zakaz iterowania po mapach (00 §3.2) jest spełniony strukturalnie.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MezoState {
    pub links: Vec<LinkState>,
    pub queues: Vec<EdgeQueue>,
    pub nodes: Vec<NodeState>,
    /// Krawędzie, których obłożenie zmieniło się w tej minucie — tylko dla nich
    /// przelicza się prędkość. Bez tego każda minuta gry kosztowałaby przejście
    /// po wszystkich 116 tys. krawędzi metropolii.
    dirty: Vec<u32>,
    dirty_flag: Vec<bool>,
    /// Kolejka oczekujacych na wjazd, po jednej na krawedz. Lista jednokierunkowa
    /// przepleciona przez podroze (`wait_next`): cztery bajty na krawedz i cztery
    /// na podroz zamiast wektora na kazdej ze 116 tys. krawedzi metropolii.
    ///
    /// Bez niej pojazd zablokowany spillbackiem musialby probowac ponownie co jakis
    /// czas -- a kazda taka proba to operacja na kopcu. Tu budzi go **zdarzenie**:
    /// zwolnienie miejsca na krawedzi, na ktora czeka.
    pub(crate) wait_head: Vec<u32>,
    pub(crate) wait_tail: Vec<u32>,
}

/// Wartownik pustej kolejki oczekujacych.
pub const NO_WAIT: u32 = u32::MAX;

impl MezoState {
    /// Buduje stan dla warstwy drogowej grafu.
    #[must_use]
    pub fn new(road: &RoadGraph, vdf: &VdfTable) -> MezoState {
        let mut links = Vec::with_capacity(road.edge_count());
        let mut queues = Vec::with_capacity(road.edge_count());
        for e in &road.edges {
            let free = e.free_speed_dkmh(road.modality).max(1);
            let storage = vdf.storage_capacity(e.length_cm, e.lanes);
            links.push(LinkState {
                inflow_last_min: 0,
                mean_speed_dkmh: free,
                free_flow_dkmh: free,
                capacity_vpm: vdf.capacity_vpm(e.class, e.lanes),
                queue_len_cm: 0,
            });
            queues.push(EdgeQueue {
                occupancy: 0,
                storage_capacity: storage,
                blocks: vdf.holds_queue(e.length_cm, e.lanes),
                rejected_last_min: 0,
                departures_this_min: 0,
                stuck_minutes: 0,
            });
        }
        MezoState {
            links,
            queues,
            nodes: vec![NodeState::default(); road.node_count()],
            dirty: Vec::new(),
            dirty_flag: vec![false; road.edge_count()],
            wait_head: vec![NO_WAIT; road.edge_count()],
            wait_tail: vec![NO_WAIT; road.edge_count()],
        }
    }

    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.links.len()
    }

    /// Ile pojazdów jest w tej chwili na sieci. Podstawa testu zachowania
    /// „w systemie = wjazdy − wyjazdy".
    #[must_use]
    pub fn vehicles_on_network(&self) -> u64 {
        self.queues.iter().map(|q| u64::from(q.occupancy)).sum()
    }

    pub fn enter(&mut self, edge: EdgeId) {
        let i = edge.0 as usize;
        self.queues[i].occupancy = self.queues[i].occupancy.saturating_add(1);
        self.links[i].inflow_last_min = self.links[i].inflow_last_min.saturating_add(1);
        self.mark(edge);
    }

    pub fn leave(&mut self, edge: EdgeId) {
        let i = edge.0 as usize;
        debug_assert!(self.queues[i].occupancy > 0, "wyjazd z pustej krawędzi");
        self.queues[i].occupancy = self.queues[i].occupancy.saturating_sub(1);
        self.queues[i].departures_this_min = self.queues[i].departures_this_min.saturating_add(1);
        self.mark(edge);
    }

    pub fn reject(&mut self, edge: EdgeId) {
        let i = edge.0 as usize;
        self.queues[i].rejected_last_min = self.queues[i].rejected_last_min.saturating_add(1);
    }

    fn mark(&mut self, edge: EdgeId) {
        let i = edge.0 as usize;
        if !self.dirty_flag[i] {
            self.dirty_flag[i] = true;
            self.dirty.push(edge.0);
        }
    }

    /// Zamraża prędkości na nową minutę. Wołane **raz**, na początku kroku minutowego,
    /// zanim ruszy jakikolwiek pojazd — stąd bierze się niezależność wyniku od
    /// kolejności przetwarzania w obrębie minuty.
    pub fn freeze_minute(&mut self, road: &RoadGraph, vdf: &VdfTable) {
        for idx in std::mem::take(&mut self.dirty) {
            let i = idx as usize;
            self.dirty_flag[i] = false;
            let e = &road.edges[i];
            let q = self.queues[i];
            let link = &mut self.links[i];
            link.mean_speed_dkmh = vdf.speed_dkmh(
                e.class,
                link.free_flow_dkmh,
                q.occupancy,
                q.storage_capacity,
            );
            link.queue_len_cm = u32::from(q.occupancy) * vdf.jam_spacing_cm();
        }
        for q in &mut self.queues {
            q.stuck_minutes = if q.occupancy >= q.storage_capacity && q.departures_this_min == 0 {
                q.stuck_minutes.saturating_add(1)
            } else {
                0
            };
            q.rejected_last_min = 0;
            q.departures_this_min = 0;
        }
        for l in &mut self.links {
            l.inflow_last_min = 0;
        }
        for n in &mut self.nodes {
            n.arrivals_last_min = n.arrivals_this_min;
            n.arrivals_this_min = 0;
        }
    }

    /// Liczba krawędzi, na których stoi korek: prędkość spadła poniżej połowy swobodnej.
    #[must_use]
    pub fn congested_edges(&self) -> u32 {
        self.links
            .iter()
            .filter(|l| l.mean_speed_dkmh * 2 < l.free_flow_dkmh)
            .count() as u32
    }
}

impl HashState for MezoState {
    /// Do hasha wchodzi to, co jest stanem: obłożenie i skwantowana prędkość
    /// (§5.9 punkt 7). `dirty` jest buforem roboczym i do hasha nie wchodzi.
    fn hash_state(&self, h: &mut StateHasher) {
        for (q, l) in self.queues.iter().zip(&self.links) {
            h.write_u16(q.occupancy);
            h.write_u16(l.mean_speed_dkmh);
        }
        for n in &self.nodes {
            h.write_u16(n.arrivals_last_min);
        }
    }
}

/// Manewr skrętny między dwiema krawędziami: priorytet dla modelu węzła i to,
/// czy manewr jest oznaczony jako zabroniony.
///
/// **Zakaz nie kończy podróży.** Routing jest węzłowy i nie zna manewrów (`J-8`),
/// więc trasa może zawierać zawracanie — najczęściej na przystanku pośrednim, jak
/// stacja paliw przy ślepej uliczce, gdzie zawrócić trzeba i wolno. Odrzucanie takiej
/// podróży w połowie łamałoby M4 §7.1: niewykonalność ma się objawiać **przy
/// planowaniu**, a nie wtedy, gdy pojazd stoi już na sieci. Zawracanie kosztuje więc
/// czas i zatrzymanie, a nie porażkę.
///
/// Brak wpisu w tabeli znaczy „nie ma zakazu i nie ma pierwszeństwa", czyli `Major`.
#[must_use]
pub fn turn_priority(road: &RoadGraph, in_edge: EdgeId, out_edge: EdgeId) -> (TurnPriority, bool) {
    for t in road.turns_from(in_edge) {
        if t.out_edge == out_edge {
            return if t.banned {
                (TurnPriority::Yield, true)
            } else {
                (t.priority, false)
            };
        }
    }
    (TurnPriority::Major, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{DistrictId, IVec2, RoadClass};
    use magnat_nav::{EdgeSpec, GeomRef, Modality, RoadGraphBuilder};

    fn graf() -> RoadGraph {
        let mut b = RoadGraphBuilder::new(Modality::Road);
        let a = b.add_node(IVec2::new(0, 0), 0);
        let z = b.add_node(IVec2::new(100_000, 0), 0);
        b.add_edge_pair(EdgeSpec {
            from: a,
            to: z,
            geometry_ref: GeomRef(0),
            length_cm: 100_000,
            lanes: 1,
            class: RoadClass::Local,
            speed_limit_dkmh: 300,
            max_mass: Mass::ZERO,
            grade_permille: 0,
            bridge: None,
            curb_parking: 0,
            district: DistrictId(0),
        });
        b.finish()
    }

    #[test]
    fn korek_powstaje_i_sie_rozladowuje() {
        let vdf = VdfTable::load_default().expect("data/roads/");
        let road = graf();
        let mut st = MezoState::new(&road, &vdf);
        let e = EdgeId(0);
        let swobodna = st.links[0].mean_speed_dkmh;

        // Zapychamy krawędź do pełna.
        let cap = st.queues[0].storage_capacity;
        for _ in 0..cap {
            st.enter(e);
        }
        st.freeze_minute(&road, &vdf);
        assert!(st.queues[0].is_full());
        assert!(
            st.links[0].mean_speed_dkmh < swobodna,
            "pełna krawędź nie zwolniła"
        );

        // I rozładowujemy.
        for _ in 0..cap {
            st.leave(e);
        }
        st.freeze_minute(&road, &vdf);
        assert_eq!(st.links[0].mean_speed_dkmh, swobodna, "korek nie zszedł");
        assert_eq!(st.vehicles_on_network(), 0);
    }

    #[test]
    fn czas_przejazdu_rosnie_gdy_predkosc_spada() {
        let cat = VehicleCatalog::load_default().expect("data/vehicles/");
        let vdf = VdfTable::load_default().expect("data/roads/");
        let road = graf();
        let st = MezoState::new(&road, &vdf);
        let veh = VehicleSpecRef {
            cat: &cat,
            class: cat.by_key("car_small").expect("car_small"),
        };

        let szybko = settle_edge(&road.edges[0], &st.links[0], &veh, Mass::ZERO, 0, 0, false);
        let wolny = LinkState {
            mean_speed_dkmh: st.links[0].mean_speed_dkmh / 3,
            ..st.links[0]
        };
        let wolno = settle_edge(&road.edges[0], &wolny, &veh, Mass::ZERO, 0, 0, false);

        assert!(wolno.travel_cs > szybko.travel_cs);
        // I kosztuje więcej paliwa — o to chodzi w korku.
        assert!(
            wolno.fuel_ul > szybko.fuel_ul,
            "{} !> {}",
            wolno.fuel_ul,
            szybko.fuel_ul
        );
    }

    #[test]
    fn wzor_paliwowy_jest_powtarzalny_co_do_mikrolitra() {
        let cat = VehicleCatalog::load_default().expect("data/vehicles/");
        let vdf = VdfTable::load_default().expect("data/roads/");
        let road = graf();
        let st = MezoState::new(&road, &vdf);
        let veh = VehicleSpecRef {
            cat: &cat,
            class: cat.by_key("car_medium").expect("car_medium"),
        };
        let a = settle_edge(
            &road.edges[0],
            &st.links[0],
            &veh,
            Mass(80_000),
            123,
            2,
            true,
        );
        let b = settle_edge(
            &road.edges[0],
            &st.links[0],
            &veh,
            Mass(80_000),
            123,
            2,
            true,
        );
        assert_eq!(a, b);
        // Kilometr przy 6,2–7,6 l/100 km to dziesiątki mililitrów, nie zero.
        assert!(a.fuel_ul > 50_000, "{} µl na 1 km", a.fuel_ul);
    }

    #[test]
    fn sygnalizacja_kosztuje_czas_a_droga_glowna_nie() {
        let n = NodeState {
            arrivals_last_min: 8,
            arrivals_this_min: 0,
        };
        let (t_sygnal, s_sygnal) = settle_node(
            NodeControl::Signal(magnat_nav::SignalPlanId(0)),
            TurnPriority::Major,
            &n,
            0,
        );
        assert!(t_sygnal > 0 && s_sygnal == 1);

        let (t_glowna, s_glowna) = settle_node(
            NodeControl::PrioritySigns {
                major: [EdgeId(0), EdgeId(1)],
            },
            TurnPriority::Major,
            &n,
            0,
        );
        assert_eq!((t_glowna, s_glowna), (0, 0));

        let (t_podp, _) = settle_node(
            NodeControl::PrioritySigns {
                major: [EdgeId(0), EdgeId(1)],
            },
            TurnPriority::Yield,
            &n,
            0,
        );
        assert!(t_podp > 0, "wlot podporządkowany nie czeka");
    }
}
