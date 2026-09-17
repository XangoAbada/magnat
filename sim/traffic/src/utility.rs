//! Sieci przesyłowe: graf, przepływ, przeciążenie, zrzut obciążenia i blackout
//! (M8b WP3, §5.4, PRD §9.6).
//!
//! # Dlaczego to stoi w `sim/traffic`, a nie w `sim/city`
//!
//! Sieć jest grafem z przepustowością i przepływem, czyli tą samą klasą rzeczy
//! co droga — i, co ważniejsze, **stoi w grafie crate'ów pod gospodarką**.
//! Odcięcie prądu zatrzymuje zakład (`sim/supply`), a `sim/traffic` od `sim/supply`
//! już zależy; gdyby sieć mieszkała w `sim/city`, zależność szłaby w drugą stronę
//! i zamknęłaby cykl, którego Cargo nie zbuduje (`city → economy → supply`).
//!
//! Rozliczenie mediów zostaje tam, gdzie było od M6b: licznik zakładu
//! (`magnat_supply::UtilityMeter`) nalicza zużycie co minutę, a faktura wychodzi
//! raz na miesiąc przez `Market::absorb_utility_bills`. **M8b nie stawia drugiego
//! licznika** — podmienia taryfę i dokłada opłatę stałą, dokładnie jak zapowiadał
//! komentarz w `data/tuning/supply.ron`: „M8 podmieni je polityką miasta i sieciami
//! przesyłowymi, nie ruszając kształtu licznika".
//!
//! # Czego ten model nie umie
//!
//! `ponytail:` przepływ liczy się na lesie rozpinającym, a przepustowość sprawdza
//! wyłącznie na mostach (patrz [`topo`]). Sufit: nie ma rozpływu mocy w oczku
//! sieci, nie ma mocy biernej, nie ma napięcia. Pierścień nie przeciąża się nigdy.
//! Ścieżka wyjścia, gdyby przeciążenia okazały się nierealistyczne: liniowy rozpływ
//! DC (macierz B, rozkład LU cache'owany na topologię) za tym samym interfejsem
//! [`UtilityNetwork::solve`], dziesięciokrotnie droższy.

pub mod solve;
pub mod system;
pub mod topo;
pub mod tuning;

use magnat_core::{
    DecisionReason, FirmId, HashState, Money, SiteId, StateHasher, Tick, UtilityService,
};

pub use solve::{ProfileTable, RepairWindow, SolveReport};
pub use system::{krok, period_minutes, register_grids, UtilitySystem};
pub use topo::TopologyCache;
pub use tuning::{GridDataError, GridTuning, GRID_SCHEMA_VERSION};

/// Twardy limit rund kaskady w jednym ticku (§5.4 krok 6).
///
/// Nie jest kalibracją i nie idzie do `data/`: chroni przed pętlą, a nie stroi
/// zachowania. Po jego wyczerpaniu reszta niezbilansowanych węzłów idzie w zrzut,
/// więc górny koszt ticku jest znany z góry, a nie „zwykle niski".
pub const MAX_CASCADE_ROUNDS: u8 = 8;

/// Stan zasilania węzła.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SupplyState {
    /// Zasilany w pełni.
    Ok,
    /// Odłączony zrzutem obciążenia — wyspa nie domykała bilansu.
    Shed,
    /// W wyspie bez ani jednego czynnego źródła. Nie ma kogo odłączać: prądu
    /// po prostu nie ma.
    Isolated,
    /// Węzeł jest źródłem i jest wyłączony (awaria bloku, remont, brak paliwa).
    Faulted,
}

impl SupplyState {
    /// Czy odbiorca dostaje medium. Dla źródła odpowiedź nie ma sensu i jest
    /// fałszem — źródło niczego nie odbiera.
    #[must_use]
    pub const fn is_supplied(self) -> bool {
        matches!(self, SupplyState::Ok)
    }

    const fn tag(self) -> u8 {
        match self {
            SupplyState::Ok => 0,
            SupplyState::Shed => 1,
            SupplyState::Isolated => 2,
            SupplyState::Faulted => 3,
        }
    }
}

/// Rola węzła w sieci.
///
/// Trzy warianty, a nie cztery z §5.4: **`Transformer { capacity }` nie powstaje**,
/// bo jego przepustowość jest przepustowością krawędzi, która do niego dochodzi,
/// i dwie liczby o jednym znaczeniu rozjechałyby się przy pierwszej zmianie.
/// Węzeł pośredni jest [`NodeRole::Hub`] — nie ma popytu i nie podlega zrzutowi.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeRole {
    /// Źródło: elektrownia, ujęcie wody, ciepłownia.
    Source { capacity: i64, online: bool },
    /// Stacja pośrednia — przepuszcza, nie odbiera i nie da się jej odłączyć.
    Hub,
    /// Przyłącze odbiorcy. `priority` niższy schodzi **później**: szpital
    /// i wodociąg mają 0, gospodarstwa domowe 2, przemysł 3.
    Connection { priority: u8 },
}

/// Kształt doby po stronie popytu. Mnożniki siedzą w `data/tuning/grid.ron`.
///
/// Istnieje, bo bez niego popyt jest stały, a sieć o stałym popycie nie przeciąża
/// się nigdy — blackout byłby wtedy wyłącznie skutkiem wymuszonej awarii, czyli
/// scenariuszem testowym udającym mechanizm (`R2`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadProfile {
    /// Bez dobowego kształtu — węzeł techniczny albo odbiór ciągły.
    Flat,
    /// Szczyt wieczorny.
    Household,
    /// Szczyt w godzinach zmiany.
    Industry,
    /// Odbiór grzewczy: ten sam kształt dobowy co gospodarstwo, ale **czuły
    /// na pogodę**. Dopisany w M8c razem z popytem na ciepło i gaz — sieć bez
    /// ani jednego odbiorcy przechodzi każdy test i wygląda w raporcie tak samo
    /// jak sieć, która działa (`CD-1`).
    Heating,
}

/// Węzeł sieci.
///
/// `metered` z §5.4 **nie powstaje**: zużycie liczy licznik zakładu
/// (`magnat_supply::UtilityMeter`) od M6b i drugi licznik po stronie sieci
/// byłby drugą prawdą o jednej liczbie.
#[derive(Clone, Copy, Debug)]
pub struct UtilityNode {
    /// Zakład za przyłączem, jeśli przyłącze prowadzi do zakładu. Gospodarstwa
    /// domowe wchodzą zbiorczo per dzielnica i nie mają `SiteId`.
    pub site: Option<SiteId>,
    /// Zakład, który ten węzeł **prowadzi** — dla źródeł: elektrownia, ciepłownia,
    /// ujęcie wody. Dopisane w M8c (`CD-2`) i celowo **osobne pole od `site`**:
    /// `site` znaczy „za tym przyłączem stoi odbiorca z licznikiem" i indeksuje
    /// `UtilityGrids`, a `owner_site` znaczy „to urządzenie ma właściciela, wiek
    /// i harmonogram konserwacji". Jeden zakład bywa i jednym, i drugim — elektrownia
    /// też bierze prąd — a dwa wpisy w indeksie przyłączy zepsułyby `power_available`.
    ///
    /// Bez tego pola sondy „blok ma 32 lata" i „90 dób zaległej konserwacji" nie
    /// miałyby skąd wziąć liczby: źródło w grafie sieci jest węzłem, a nie maszyną.
    pub owner_site: Option<SiteId>,
    pub role: NodeRole,
    /// Moc przyłączeniowa: waty dla energii, mililitry na godzinę dla cieczy.
    /// To jest **moc znamionowa, nie chwilowy pobór** — i to nie jest niedbałość.
    /// Popyt liczony z bieżącej pracy linii spadałby do zera w chwili odcięcia,
    /// wyspa natychmiast by się zbilansowała, prąd wróciłby w następnej minucie
    /// i sieć oscylowałaby w nieskończoność. Sieć planuje się na moc przyłączoną.
    pub base_demand: i64,
    pub profile: LoadProfile,
    pub supplied: i64,
    pub state: SupplyState,
}

impl UtilityNode {
    /// Odbiorca o zadanej mocy przyłączeniowej.
    #[must_use]
    pub const fn connection(
        site: Option<SiteId>,
        priority: u8,
        base_demand: i64,
        profile: LoadProfile,
    ) -> UtilityNode {
        UtilityNode {
            site,
            owner_site: None,
            role: NodeRole::Connection { priority },
            base_demand,
            profile,
            supplied: 0,
            state: SupplyState::Ok,
        }
    }

    /// Źródło o zadanej mocy osiągalnej.
    #[must_use]
    pub const fn source(capacity: i64) -> UtilityNode {
        UtilityNode {
            site: None,
            owner_site: None,
            role: NodeRole::Source {
                capacity,
                online: true,
            },
            base_demand: 0,
            profile: LoadProfile::Flat,
            supplied: 0,
            state: SupplyState::Ok,
        }
    }

    /// Źródło prowadzone przez konkretny zakład — elektrownia, ciepłownia, ujęcie.
    ///
    /// To jest wejście, którym hazard awarii M8c sięga po wiek bloku i zaległą
    /// konserwację: bez właściciela źródło jest liczbą watów bez historii.
    #[must_use]
    pub const fn source_owned(site: SiteId, capacity: i64) -> UtilityNode {
        UtilityNode {
            site: None,
            owner_site: Some(site),
            role: NodeRole::Source {
                capacity,
                online: true,
            },
            base_demand: 0,
            profile: LoadProfile::Flat,
            supplied: 0,
            state: SupplyState::Ok,
        }
    }

    /// Stacja pośrednia.
    #[must_use]
    pub const fn hub() -> UtilityNode {
        UtilityNode {
            site: None,
            owner_site: None,
            role: NodeRole::Hub,
            base_demand: 0,
            profile: LoadProfile::Flat,
            supplied: 0,
            state: SupplyState::Ok,
        }
    }
}

impl HashState for UtilityNode {
    fn hash_state(&self, h: &mut StateHasher) {
        match self.role {
            NodeRole::Source { capacity, online } => {
                h.write_u8(0);
                h.write_i64(capacity);
                h.write_u8(u8::from(online));
            }
            NodeRole::Hub => h.write_u8(1),
            NodeRole::Connection { priority } => {
                h.write_u8(2);
                h.write_u8(priority);
            }
        }
        match self.owner_site {
            Some(s) => {
                h.write_u8(1);
                s.0.hash_state(h);
            }
            None => h.write_u8(0),
        }
        h.write_i64(self.base_demand);
        h.write_i64(self.supplied);
        h.write_u8(self.state.tag());
    }
}

/// Stan krawędzi.
///
/// `UnderMaintenance` z §5.4 **nie powstaje**: w M8b nikt nie planuje konserwacji,
/// a wariant, którego nic nie ustawia, przechodzi każdy test i wygląda w raporcie
/// tak samo jak wariant, który działa (`R2`). Planowy remont linii dokłada M8e
/// razem z polityką taryfową — wtedy będzie miał kto go ustawić.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeState {
    Ok,
    /// Zadziałało zabezpieczenie; wraca sama w ticku `until`.
    Tripped {
        until: Tick,
    },
}

/// Krawędź sieci.
#[derive(Clone, Copy, Debug)]
pub struct UtilityEdge {
    pub a: u32,
    pub b: u32,
    pub capacity: i64,
    /// Strata na przesyle, w punktach bazowych. Składa się wzdłuż drogi od źródła
    /// — dwa odcinki po 2 % to 4,04 %, a nie 4 %.
    pub loss_bps: u32,
    pub state: EdgeState,
}

impl UtilityEdge {
    #[must_use]
    pub const fn new(a: u32, b: u32, capacity: i64, loss_bps: u32) -> UtilityEdge {
        UtilityEdge {
            a,
            b,
            capacity,
            loss_bps,
            state: EdgeState::Ok,
        }
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.state, EdgeState::Ok)
    }
}

impl HashState for UtilityEdge {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.a);
        h.write_u32(self.b);
        h.write_i64(self.capacity);
        h.write_u32(self.loss_bps);
        match self.state {
            EdgeState::Ok => h.write_u8(0),
            EdgeState::Tripped { until } => {
                h.write_u8(1);
                h.write_u64(until.0);
            }
        }
    }
}

/// Taryfa operatora sieci.
///
/// Dwie pozycje, a nie cztery z §5.4. **`peak_multiplier_bps` nie powstaje**:
/// licznik zakładu rozlicza się raz na miesiąc jedną stawką, więc mnożnik
/// szczytowy zapisany w chwili wystawienia faktury obłożyłby nim **cały** miesiąc.
/// Taryfa strefowa wymaga rozliczenia godzinowego, czyli zmiany kształtu licznika
/// po stronie M6 — a szczyt i tak jest w modelu, tylko po stronie popytu
/// ([`LoadProfile`]), gdzie ma skutek fizyczny zamiast księgowego.
/// **`connection_fee` nie powstaje** z tego samego powodu: w M8b wszystkie
/// przyłącza istnieją od minuty zero i nikt nowego nie zakłada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tariff {
    /// Opłata stała — należy się niezależnie od zużycia. To ona sprawia, że
    /// blackout **widać w kosztach**: licznik stoi, a rachunek nie znika.
    pub standing_charge_per_month: Money,
    /// Cena jednostki rozliczeniowej: kWh dla energii, m³ dla cieczy.
    pub per_unit: Money,
}

impl HashState for Tariff {
    fn hash_state(&self, h: &mut StateHasher) {
        self.standing_charge_per_month.hash_state(h);
        self.per_unit.hash_state(h);
    }
}

/// Jedna sieć jednego medium.
pub struct UtilityNetwork {
    pub service: UtilityService,
    /// Operator. `ponytail:` sufit — w M8b jest to **identyfikator bez księgi**:
    /// pieniądz za media wychodzi na konto reszty świata tak samo jak od M6b,
    /// bo operator jako firma z rachunkiem wyników jest decyzją `D10` odłożoną
    /// do M9/M10 („przejęcie operatora"). Ścieżka wyjścia: założyć firmę
    /// w rejestrze M7 i skierować `absorb_utility_bills` na jej konto — reszta
    /// modelu się nie zmienia, bo taryfa już dziś jest własnością sieci.
    pub operator: FirmId,
    pub nodes: Vec<UtilityNode>,
    pub edges: Vec<UtilityEdge>,
    pub tariff: Tariff,
    /// Mnożnik pogodowy popytu, w punktach bazowych (10 000 = norma sezonu).
    ///
    /// Pisze go **wyłącznie** krok pogody w `sim/events` (M8c §5.6), jeden na sieć,
    /// bo mróz podnosi pobór ciepła i prądu, a upał — wody; jedna liczba dla
    /// wszystkich mediów byłaby fałszem w trzech z nich. Dotyczy profilu
    /// `Household` i `Heating`; przemysł grzeje halę tak samo w lipcu i w styczniu.
    ///
    /// Wchodzi do hasha stanu, bo zmienia bilans wyspy, czyli to, czy zapala się zrzut.
    pub weather_bps: u32,
    topo: TopologyCache,
    dirty: bool,
    scratch: solve::Scratch,
}

impl UtilityNetwork {
    #[must_use]
    pub fn new(
        service: UtilityService,
        operator: FirmId,
        nodes: Vec<UtilityNode>,
        edges: Vec<UtilityEdge>,
        tariff: Tariff,
    ) -> UtilityNetwork {
        UtilityNetwork {
            service,
            operator,
            nodes,
            edges,
            tariff,
            weather_bps: solve::BPS_U32,
            topo: TopologyCache::default(),
            dirty: true,
            scratch: solve::Scratch::default(),
        }
    }

    /// Wymusza przebudowę topologii przy najbliższym kroku. Woła się po dołożeniu
    /// przyłącza albo po ręcznej zmianie krawędzi.
    ///
    /// Unieważnia **także listy sąsiedztwa**, bo zmiana, którą zgłasza się tędy,
    /// bywa przepięciem istniejącej krawędzi: liczba węzłów i krawędzi zostaje ta
    /// sama, a graf jest inny. Wypadnięcie krawędzi z przeciążenia idzie osobną,
    /// tańszą drogą — tam zmienia się wyłącznie to, które krawędzie są czynne.
    pub fn mark_topology_dirty(&mut self) {
        self.dirty = true;
        self.topo.invalidate();
    }

    /// Wyłącza albo włącza źródło. Zwraca `false`, gdy węzeł źródłem nie jest.
    ///
    /// To jest wejście, którym awaria bloku wchodzi do sieci — i jedyne, jakie
    /// M8b wystawia. Hazard, który je wywoła ze stanu świata (wiek bloku, zaległa
    /// konserwacja, miesiące pracy na 98 % mocy), należy do M8c.
    pub fn set_source_online(&mut self, node: u32, on: bool) -> bool {
        let Some(n) = self.nodes.get_mut(node as usize) else {
            return false;
        };
        let NodeRole::Source { online, .. } = &mut n.role else {
            return false;
        };
        *online = on;
        n.state = if on {
            SupplyState::Ok
        } else {
            SupplyState::Faulted
        };
        true
    }

    /// Czy sieć dostarcza medium do tego węzła.
    #[must_use]
    pub fn state_of(&self, node: u32) -> SupplyState {
        self.nodes
            .get(node as usize)
            .map_or(SupplyState::Isolated, |n| n.state)
    }

    /// Indeksy węzłów źródłowych — w kolejności rosnącej, bo po niej idą patche
    /// generatora zdarzeń, a kolejność patchy wchodzi do hasha stanu.
    #[must_use]
    pub fn sources(&self) -> Vec<u32> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.role, NodeRole::Source { .. }))
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// Moc osiągalna źródła i jego właściciel — wejście sond M8c.
    #[must_use]
    pub fn source_info(&self, node: u32) -> Option<(i64, bool, Option<SiteId>)> {
        let n = self.nodes.get(node as usize)?;
        let NodeRole::Source { capacity, online } = n.role else {
            return None;
        };
        Some((capacity, online, n.owner_site))
    }

    /// Ustawia moc osiągalną źródła. Zwraca `false`, gdy węzeł źródłem nie jest.
    ///
    /// Wejście dla `SimParam::SourceCapacityMulBps`: awaria **jednego bloku** nie gasi
    /// elektrowni, tylko zabiera jej część mocy — i to jest różnica między „zgasło
    /// pół dzielnicy" a „zgasło miasto".
    pub fn set_source_capacity(&mut self, node: u32, cap: i64) -> bool {
        let Some(n) = self.nodes.get_mut(node as usize) else {
            return false;
        };
        let NodeRole::Source { capacity, .. } = &mut n.role else {
            return false;
        };
        *capacity = cap.max(0);
        true
    }

    /// Topologia — do inspektora i do testów.
    #[must_use]
    pub fn topology(&self) -> &TopologyCache {
        &self.topo
    }
}

impl HashState for UtilityNetwork {
    /// Do hasha wchodzi **stan**, nie wejście: stany węzłów i krawędzi tak,
    /// bufory robocze solvera nie. Taryfa wchodzi, bo od M8e zmienia ją uchwała
    /// rady i rozjazd w niej byłby rozjazdem w rachunku każdego zakładu.
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.service as u8);
        self.operator.entity().hash_state(h);
        h.write_u32(self.nodes.len() as u32);
        for n in &self.nodes {
            n.hash_state(h);
        }
        h.write_u32(self.edges.len() as u32);
        for e in &self.edges {
            e.hash_state(h);
        }
        self.tariff.hash_state(h);
        h.write_u32(self.weather_bps);
        self.topo.hash_state(h);
    }
}

/// Ile wpisów historii powodów trzyma sieć na potrzeby karty inspekcji.
pub const REASON_RING: usize = 64;

/// Wszystkie sieci miasta jako jeden zasób świata.
///
/// Indeks `(zakład, medium) → węzeł` jest posortowanym `Vec`iem z wyszukiwaniem
/// binarnym, a nie `HashMap` — 00 §3.2 zabrania iterowania po `HashMap` w kodzie
/// symulacji, a tu i tak iterujemy przy każdym kroku, żeby przepisać odcięcia
/// do liczników.
#[derive(Default)]
pub struct UtilityGrids {
    nets: Vec<UtilityNetwork>,
    /// `(indeks encji zakładu, indeks medium, numer sieci, numer węzła)`,
    /// posortowane po dwóch pierwszych.
    index: Vec<(u32, u8, u32, u32)>,
    reasons: Vec<(Tick, DecisionReason)>,
    /// Odcisk obrazu awarii z poprzedniego kroku, per sieć. Służy jednej rzeczy:
    /// rozpoznaniu, że coś się zmieniło i trzeba przepisać odcięcia do liczników.
    /// Bez tego szew M8b → M6 chodziłby po arenie zakładów 1440 razy na dobę
    /// po to, żeby 1439 razy nie zmienić niczego.
    ///
    /// **Odcisk zbioru, nie suma mocy**: tick, w którym jeden zakład gaśnie,
    /// a drugi o równym poborze wraca, ma tę samą sumę i wyglądałby jak tick,
    /// w którym nic się nie stało.
    last_outage: Vec<u64>,
    /// Suma niedostarczonej mocy per sieć — wyłącznie do raportu i inspektora.
    last_unserved: Vec<i64>,
    /// Moc osiągalna i popyt z ostatniego kroku, per sieć. **Nie wchodzą do hasha**:
    /// są przeliczane od zera w każdym kroku z węzłów, które do hasha wchodzą.
    /// Są za to jedynym wejściem, którym generator zdarzeń M8c widzi obciążenie
    /// źródeł — bez nich sonda „elektrownia chodzi na 98 % mocy" nie miałaby skąd
    /// wziąć liczby, bo bilans wyspy żyje w buforach roboczych solvera.
    last_supply: Vec<i64>,
    last_demand: Vec<i64>,
    /// Zależności między sieciami: `(sieć, przyłącze, sieć zależna, jej źródło)`.
    /// Utrata zasilania przyłącza wyłącza źródło drugiej sieci.
    ///
    /// Jest dokładnie jedna taka para i to ona nadaje sens priorytetowi 0
    /// z §5.4 („szpital i wodociąg"): ujęcie wody jest odbiorcą prądu, więc
    /// blackout zabiera miastu najpierw prąd, a zaraz potem wodę. Bez tego
    /// pola każda sieć byłaby osobnym światem i awaria nie miałaby dokąd pójść.
    links: Vec<(u32, u32, u32, u32)>,
}

impl UtilityGrids {
    /// Dokłada sieć i przebudowuje indeks przyłączy.
    pub fn push(&mut self, net: UtilityNetwork) {
        let n = self.nets.len() as u32;
        let service = net.service.as_index() as u8;
        for (i, node) in net.nodes.iter().enumerate() {
            if let Some(s) = node.site {
                self.index.push((s.0.index(), service, n, i as u32));
            }
        }
        self.nets.push(net);
        // `u64::MAX` znaczy „jeszcze nie liczone" — pierwszy krok przepisze
        // liczniki zawsze, bo to on wpisuje im taryfę operatora.
        self.last_outage.push(u64::MAX);
        self.last_unserved.push(0);
        self.last_supply.push(0);
        self.last_demand.push(0);
        self.index.sort_unstable();
    }

    /// Odnotowuje wynik kroku sieci. Zwraca `true`, gdy obraz awarii się zmienił
    /// — czyli gdy odcięcia w licznikach zakładów są nieaktualne.
    pub(crate) fn mark_outage(&mut self, net: usize, key: u64, unserved: i64) -> bool {
        let zmiana = self.last_outage[net] != key;
        self.last_outage[net] = key;
        self.last_unserved[net] = unserved;
        zmiana
    }

    /// Odnotowuje bilans kroku: moc osiągalna i popyt, oba w skali u źródła.
    pub(crate) fn mark_balance(&mut self, net: usize, supply: i64, demand: i64) {
        self.last_supply[net] = supply;
        self.last_demand[net] = demand;
    }

    /// Obciążenie sieci w punktach bazowych: `popyt / moc osiągalna`.
    ///
    /// 10 000 znaczy „źródła oddają dokładnie tyle, ile miasto bierze", powyżej —
    /// wyspa nie domyka bilansu. Sieć bez czynnego źródła zwraca `u32::MAX`,
    /// bo „nieskończenie przeciążona" jest bliższe prawdy niż zero.
    #[must_use]
    pub fn load_factor_bps(&self, service: UtilityService) -> u32 {
        let Some(i) = self.nets.iter().position(|n| n.service == service) else {
            return 0;
        };
        let (s, d) = (self.last_supply[i], self.last_demand[i]);
        if s <= 0 {
            return if d > 0 { u32::MAX } else { 0 };
        }
        u32::try_from(d.max(0).saturating_mul(10_000) / s).unwrap_or(u32::MAX)
    }

    /// Zapas mocy sieci w punktach bazowych: `(moc − popyt) / moc`. Ujemny znaczy
    /// deficyt, czyli zrzut obciążenia w tym samym kroku.
    #[must_use]
    pub fn reserve_margin_bps(&self, service: UtilityService) -> i32 {
        let lf = self.load_factor_bps(service);
        if lf == u32::MAX {
            return i32::MIN;
        }
        10_000 - i32::try_from(lf).unwrap_or(i32::MAX)
    }

    /// Wiąże przyłącze jednej sieci ze źródłem drugiej: gdy przyłącze traci
    /// zasilanie, źródło gaśnie.
    pub fn link_source(&mut self, net: u32, node: u32, dependent: u32, source: u32) {
        self.links.push((net, node, dependent, source));
        self.links.sort_unstable();
    }

    /// Przenosi stan przyłączy na źródła sieci zależnych. Woła się **przed**
    /// krokiem sieci zależnej, żeby zobaczyła ona świeży stan tej, od której
    /// zależy — a nie sprzed minuty.
    pub(crate) fn apply_links_to(&mut self, target: u32) {
        for i in 0..self.links.len() {
            let (net, node, dependent, source) = self.links[i];
            if dependent != target {
                continue;
            }
            let zasilane = self.nets[net as usize].state_of(node).is_supplied();
            self.nets[dependent as usize].set_source_online(source, zasilane);
        }
    }

    /// Ile mocy nie dotarło do odbiorców w ostatnim kroku tej sieci.
    #[must_use]
    pub fn unserved(&self, net: usize) -> i64 {
        self.last_unserved.get(net).copied().unwrap_or(0).max(0)
    }

    #[must_use]
    pub fn nets(&self) -> &[UtilityNetwork] {
        &self.nets
    }

    pub fn nets_mut(&mut self) -> &mut [UtilityNetwork] {
        &mut self.nets
    }

    /// Sieć danego medium, jeśli miasto ją ma.
    #[must_use]
    pub fn net(&self, service: UtilityService) -> Option<&UtilityNetwork> {
        self.nets.iter().find(|n| n.service == service)
    }

    pub fn net_mut(&mut self, service: UtilityService) -> Option<&mut UtilityNetwork> {
        self.nets.iter_mut().find(|n| n.service == service)
    }

    fn locate(&self, site: SiteId, service: UtilityService) -> Option<(u32, u32)> {
        let klucz = (site.0.index(), service.as_index() as u8);
        self.index
            .binary_search_by_key(&klucz, |(s, k, _, _)| (*s, *k))
            .ok()
            .map(|i| (self.index[i].2, self.index[i].3))
    }

    /// Stan zasilania zakładu danym medium — kontrakt dla M7 z §6 dokumentu fazy.
    ///
    /// Zakład bez przyłącza dostaje [`SupplyState::Ok`], a nie `Isolated`: brak
    /// przyłącza znaczy „nie potrzebuje", tak samo jak brak licznika po stronie
    /// M6. Odwrotna odpowiedź zatrzymałaby każdy zakład, który nie bierze prądu.
    #[must_use]
    pub fn supply_state(&self, site: SiteId, service: UtilityService) -> SupplyState {
        match self.locate(site, service) {
            Some((n, v)) => self.nets[n as usize].state_of(v),
            None => SupplyState::Ok,
        }
    }

    /// Skrót na najczęstsze pytanie: czy ten zakład ma prąd.
    #[must_use]
    pub fn power_available(&self, site: SiteId) -> bool {
        self.supply_state(site, UtilityService::Electricity)
            .is_supplied()
    }

    /// Powody z ostatnich kroków — karta inspekcji sieci (00 §7).
    #[must_use]
    pub fn reasons(&self) -> &[(Tick, DecisionReason)] {
        &self.reasons
    }

    pub(crate) fn note(&mut self, t: Tick, r: DecisionReason) {
        if self.reasons.len() == REASON_RING {
            self.reasons.remove(0);
        }
        self.reasons.push((t, r));
    }
}

impl HashState for UtilityGrids {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.nets.len() as u32);
        for n in &self.nets {
            n.hash_state(h);
        }
        // Powody są wyjaśnieniem, nie stanem — ale ich **liczba i dyskryminanty**
        // wchodzą, bo rozjazd w tym, ile razy sieć zrzuciła obciążenie, jest
        // rozjazdem w symulacji, nawet gdy salda przypadkiem się zgadzają.
        h.write_u32(self.reasons.len() as u32);
        for (t, r) in &self.reasons {
            h.write_u64(t.0);
            h.write_u32(u32::from(r.discriminant()));
        }
    }
}
