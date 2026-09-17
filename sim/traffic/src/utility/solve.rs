//! Solver przepływu: bilans wyspy, zrzut obciążenia, przepływy na mostach
//! i kaskada wyłączeń (M8b §5.4, kroki 1–6).
//!
//! Wszystko całkowitoliczbowe. Straty składają się mnożnikiem `gross` liczonym
//! w dziesięciotysięcznych — dwa odcinki po 2 % dają 4,04 %, a nie 4 %, bo strata
//! na drugim odcinku dotyczy mocy, którą trzeba było przez pierwszy przepchnąć.
//!
//! Koszt rundy to O(N+E) plus sortowanie odbiorców **wyłącznie w wyspie, która
//! nie domyka bilansu**. Wyspa zbilansowana nie sortuje niczego, a to jest
//! przypadek typowy: sieć stoi w równowadze prawie każdą minutę doby.

use magnat_core::{rng, DecisionReason, StreamId, Tick};

use super::topo::NO_PARENT;
use super::{
    EdgeState, LoadProfile, NodeRole, SupplyState, UtilityEdge, UtilityNetwork, UtilityNode,
    MAX_CASCADE_ROUNDS,
};

/// Skala mnożnika strat i profili. Punkty bazowe, tak samo jak każda stawka
/// w tej grze — moc przekłada się na pieniądz przez licznik, więc zakaz floatów
/// z 00 §2 obowiązuje i tutaj.
const BPS: i64 = 10_000;

/// To samo 10 000 w `u32` — wartość neutralna mnożnika pogodowego sieci.
pub(crate) const BPS_U32: u32 = 10_000;

/// Widełki czasu naprawy krawędzi, w minutach gry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RepairWindow {
    pub min_minutes: u32,
    pub max_minutes: u32,
}

impl Default for RepairWindow {
    fn default() -> RepairWindow {
        RepairWindow {
            min_minutes: 60,
            max_minutes: 420,
        }
    }
}

impl RepairWindow {
    /// Losuje czas naprawy dla konkretnej krawędzi w konkretnym ticku.
    ///
    /// Klucz to `(indeks krawędzi, tick)`, więc ta sama awaria w tym samym
    /// przebiegu wraca po tym samym czasie niezależnie od kolejności, w jakiej
    /// solver doszedł do tej krawędzi. Losowany jest **czas naprawy, nie samo
    /// zadziałanie**: przeciążenie wynika z bilansu i jest funkcją stanu.
    #[must_use]
    pub fn draw(self, seed: u64, edge: u32, t: Tick) -> u32 {
        // Naprawa **nigdy nie trwa zera minut**: krawędź wracająca w tym samym
        // ticku, w którym wypadła, wypadałaby co tick i zalewała pierścień powodów,
        // a kaskada nie miałaby jak się zatrzymać.
        let dol = self.min_minutes.max(1);
        let rozpietosc = self.max_minutes.saturating_sub(dol);
        if rozpietosc == 0 {
            return dol;
        }
        let mut r = rng(seed, StreamId::GridFault, edge, t);
        dol.saturating_add(r.gen_range_u32(rozpietosc.saturating_add(1)))
    }
}

/// Mnożniki dobowego kształtu popytu, po jednym na godzinę doby, w punktach
/// bazowych. Dane, nie kod — `data/tuning/grid.ron`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ProfileTable {
    pub household: [u32; 24],
    pub industry: [u32; 24],
}

impl Default for ProfileTable {
    /// Płasko: sieć bez tabeli profili zachowuje się jak sieć o stałym popycie.
    fn default() -> ProfileTable {
        ProfileTable {
            household: [10_000; 24],
            industry: [10_000; 24],
        }
    }
}

impl ProfileTable {
    fn mnoznik(&self, p: LoadProfile, hour: u32) -> i64 {
        let h = (hour % 24) as usize;
        match p {
            LoadProfile::Flat => BPS,
            LoadProfile::Household => i64::from(self.household[h]),
            LoadProfile::Industry => i64::from(self.industry[h]),
            // Odbiór grzewczy ma ten sam kształt doby co gospodarstwo — różni się
            // tym, że jest czuły na pogodę, a to jest osobny mnożnik.
            LoadProfile::Heating => i64::from(self.household[h]),
        }
    }
}

/// Popyt węzła w tej godzinie: moc przyłączeniowa × kształt doby × pogoda.
///
/// Mnożnik pogodowy dotyczy wyłącznie odbioru bytowego i grzewczego. Przemysł
/// bierze tyle, ile bierze linia produkcyjna, i mróz tego nie zmienia.
fn demand_at(node: &UtilityNode, hour: u32, p: &ProfileTable, weather_bps: u32) -> i64 {
    let podstawa = node.base_demand * p.mnoznik(node.profile, hour) / BPS;
    match node.profile {
        LoadProfile::Household | LoadProfile::Heating => podstawa * i64::from(weather_bps) / BPS,
        LoadProfile::Flat | LoadProfile::Industry => podstawa,
    }
}

/// Co zrobiła jedna minuta sieci.
#[derive(Clone, Debug, Default)]
pub struct SolveReport {
    pub shed_nodes: Vec<u32>,
    pub tripped_edges: Vec<u32>,
    /// Moc, której odbiorcy nie dostali: suma popytu węzłów w stanie innym niż
    /// [`SupplyState::Ok`]. Zero znaczy, że sieć domknęła bilans bez ofiar.
    pub unserved: i64,
    /// Odcisk **zbioru** odbiorców bez zasilania, nie jego sumy mocy.
    ///
    /// Sama `unserved` do rozpoznania zmiany nie wystarcza: tick, w którym jeden
    /// zakład gaśnie, a drugi o równym poborze wraca, ma tę samą sumę i wyglądałby
    /// jak tick, w którym nic się nie stało — a liczniki zakładów zostałyby wtedy
    /// z odcięciem odwrotnym do prawdy aż do najbliższej zmiany sumy.
    pub outage_key: u64,
    pub cascade_rounds: u8,
    /// Moc osiągalna i popyt po ostatniej rundzie, oba w skali u źródła
    /// (czyli z doliczoną stratą przesyłu). Wychodzą z solvera, bo bilans wyspy
    /// żyje w buforach roboczych i po kroku znika — a to jedyne liczby, z których
    /// generator zdarzeń M8c widzi, że elektrownia chodzi na 98 % mocy.
    pub supply_total: i64,
    pub demand_total: i64,
    /// Powody do karty inspekcji (00 §7). Wyjaśnienie jest częścią wyniku, a nie
    /// polem sieci — sieć nie ma prawa zapisać go sama, bo nie zna ticku.
    pub reasons: Vec<DecisionReason>,
}

/// Bufory robocze solvera. Żyją w sieci, żeby krok minutowy nie alokował —
/// 1440 alokacji na dobę razy pięć sieci to koszt widoczny w budżecie 0,3 ms.
#[derive(Clone, Debug, Default)]
pub struct Scratch {
    /// Mnożnik strat od źródła do węzła, w punktach bazowych.
    gross: Vec<i64>,
    /// Popyt węzła przeliczony na moc **u źródła** (`popyt × gross`).
    w: Vec<i64>,
    /// Suma poddrzewa w tej samej skali — po niej liczy się przepływ krawędzi.
    sub: Vec<i64>,
    /// Moc osiągalna wyspy, indeksowana korzeniem.
    supply: Vec<i64>,
    /// Popyt wyspy w skali `w`, indeksowany korzeniem.
    demand: Vec<i64>,
    /// Kolejność zrzutu, spakowana w jedno `u64`:
    /// `wyspa(24 b) | 255 − priorytet(8 b) | indeks węzła(32 b)`.
    ///
    /// Pakowanie zamiast krotki `(u32, Reverse<u8>, u32)` nie jest sprytem dla
    /// sprytu: sortowanie jest **najdroższą częścią kaskady** (zmierzone: 2,7 ms
    /// z 6,3 ms w scenie patologicznej), a porównanie jednego `u64` jest dla
    /// procesora jedną instrukcją zamiast trzech gałęzi.
    kolejka2: Vec<u64>,
    active: Vec<bool>,
    pary: Vec<(u32, u32)>,
    zrodla: Vec<bool>,
}

impl UtilityNetwork {
    /// Jedna minuta sieci: bilans, zrzut, przepływy, zabezpieczenia i kaskada.
    ///
    /// `hour` to godzina doby 0..=23 — po niej idzie kształt popytu
    /// ([`LoadProfile`]).
    #[must_use]
    pub fn solve(
        &mut self,
        seed: u64,
        t: Tick,
        hour: u32,
        profile: &ProfileTable,
        repair: RepairWindow,
    ) -> SolveReport {
        let mut raport = SolveReport::default();
        if self.nodes.is_empty() {
            return raport;
        }
        // Bufory wychodzą z sieci na czas kroku: inaczej `&mut self.scratch`
        // pożyczałby całą strukturę i ani węzły, ani krawędzie nie byłyby
        // dostępne. Wracają na końcu, więc alokacji dalej nie ma.
        let mut s = std::mem::take(&mut self.scratch);

        // Krawędzie, którym minął czas naprawy, wracają **przed** pierwszą rundą:
        // sieć ma się zregenerować sama, a nie czekać na kolejną awarię.
        for e in &mut self.edges {
            if let EdgeState::Tripped { until } = e.state {
                if t.0 >= until.0 {
                    e.state = EdgeState::Ok;
                    self.dirty = true;
                }
            }
        }

        let mut wypadlo = 0;
        for runda in 0..MAX_CASCADE_ROUNDS {
            raport.cascade_rounds = runda + 1;
            // Zrzut rozstrzyga się od nowa w każdej rundzie, bo zmieniła się
            // topologia — więc lista z poprzedniej rundy jest nieaktualna, a nie
            // uzupełniana. Raport ma mówić, **kto jest** bez prądu po kroku,
            // a nie kogo po drodze rozważano; w patologicznej kaskadzie różnica
            // to ośmiokrotność listy o pięciu tysiącach pozycji.
            raport.shed_nodes.clear();
            self.przygotuj_topologie(&mut s);
            self.rozlicz_wyspy(&mut s, hour, profile, &mut raport);
            self.policz_przeplywy(&mut s);
            wypadlo = self.zadzialaj_zabezpieczenia(&s, seed, t, repair, &mut raport);
            if wypadlo == 0 {
                break;
            }
        }

        // Ostatnia runda mogła wywalić krawędź i na tym się skończyć — a wtedy
        // węzły odcięte przez tę krawędź zostałyby ze stanem sprzed jej
        // wypadnięcia, czyli z prądem, którego nie mają. Limit rund ma ograniczyć
        // **koszt**, a nie pozwolić sieci skłamać: domykamy bilansem bez
        // zabezpieczeń, więc reszta niezbilansowanych węzłów idzie w zrzut
        // dokładnie tak, jak obiecuje komentarz przy `MAX_CASCADE_ROUNDS`.
        if wypadlo > 0 {
            raport.shed_nodes.clear();
            self.przygotuj_topologie(&mut s);
            self.rozlicz_wyspy(&mut s, hour, profile, &mut raport);
        }

        for (i, n) in self.nodes.iter().enumerate() {
            if !matches!(n.role, NodeRole::Connection { .. }) || n.state == SupplyState::Ok {
                continue;
            }
            raport.unserved += demand_at(n, hour, profile, self.weather_bps);
            // Mieszalnik, nie suma: dwa węzły zamienione miejscami mają dać inny
            // odcisk, a suma indeksów ich nie odróżnia.
            raport.outage_key = raport
                .outage_key
                .rotate_left(7)
                .wrapping_mul(0x100_0000_01B3)
                ^ u64::from(i as u32)
                ^ u64::from(n.state.tag());
        }
        self.scratch = s;
        raport
    }

    /// Krok 1: wyspy, las rozpinający i mosty, a po nich mnożnik strat.
    fn przygotuj_topologie(&mut self, s: &mut Scratch) {
        s.active.clear();
        s.active
            .extend(self.edges.iter().map(UtilityEdge::is_active));
        if self.dirty {
            s.pary.clear();
            s.pary.extend(self.edges.iter().map(|e| (e.a, e.b)));
            s.zrodla.clear();
            s.zrodla.extend(
                self.nodes
                    .iter()
                    .map(|n| matches!(n.role, NodeRole::Source { .. })),
            );
            self.topo
                .rebuild(self.nodes.len(), &s.pary, &s.active, &s.zrodla);
            self.dirty = false;
        }
        // Rodzic przed dzieckiem, czyli postorder od tyłu.
        s.gross.clear();
        s.gross.resize(self.nodes.len(), BPS);
        for &v in self.topo.post_order.iter().rev() {
            let p = self.topo.parent[v as usize];
            if p == NO_PARENT {
                continue;
            }
            let e = &self.edges[self.topo.parent_edge[v as usize] as usize];
            s.gross[v as usize] = s.gross[p as usize] * (BPS + i64::from(e.loss_bps)) / BPS;
        }
    }

    /// Kroki 2 i 3: bilans wyspy i zrzut obciążenia.
    fn rozlicz_wyspy(
        &mut self,
        s: &mut Scratch,
        hour: u32,
        profile: &ProfileTable,
        raport: &mut SolveReport,
    ) {
        let n = self.nodes.len();
        let pogoda = self.weather_bps;
        s.supply.clear();
        s.supply.resize(n, 0);
        s.demand.clear();
        s.demand.resize(n, 0);
        s.w.clear();
        s.w.resize(n, 0);

        for (i, node) in self.nodes.iter_mut().enumerate() {
            let wyspa = self.topo.island[i] as usize;
            match node.role {
                NodeRole::Source { capacity, online } => {
                    node.supplied = 0;
                    node.state = if online {
                        s.supply[wyspa] += capacity;
                        SupplyState::Ok
                    } else {
                        SupplyState::Faulted
                    };
                }
                NodeRole::Hub => {
                    node.state = SupplyState::Ok;
                    node.supplied = 0;
                }
                NodeRole::Connection { .. } => {
                    node.state = SupplyState::Ok;
                    let d = demand_at(node, hour, profile, pogoda);
                    node.supplied = d;
                    s.w[i] = d * s.gross[i];
                    s.demand[wyspa] += s.w[i];
                }
            }
        }

        // Bilans całej sieci, zanim zrzut zetnie popyt do możliwości źródeł.
        // Kolejność sumowania idzie po `roots`, czyli po indeksach węzłów —
        // nie po `HashMap` (00 §3.2).
        raport.supply_total = 0;
        raport.demand_total = 0;
        for &korzen in &self.topo.roots {
            let k = korzen as usize;
            raport.supply_total += s.supply[k];
            raport.demand_total += s.demand[k] / BPS;
        }

        // Wyspy, które nie domykają bilansu. Bez tego kroku zrzut przechodziłby
        // po **wszystkich** węzłach dla **każdej** wyspy — a kaskada rozbija sieć
        // metropolii na tysiące wysp, więc iloczyn byłby kwadratem i budżet
        // 2 ms z testu T8b pękłby o rząd wielkości.
        let mut jest = false;
        for &korzen in &self.topo.roots {
            let k = korzen as usize;
            if s.demand[k] > s.supply[k] * BPS {
                jest = true;
            }
        }
        if !jest {
            return;
        }

        // Jedno przejście po węzłach, jedno sortowanie, potem spacer po
        // segmentach: `(wyspa, malejąco priorytet, rosnąco indeks)`.
        //
        // Klucz mieści wyspę w 24 bitach, więc sieć powyżej 16,7 mln węzłów
        // mieszałaby wyspy po cichu. Metropolia ma ich pięć tysięcy, ale cicha
        // granica jest gorsza od głośnej.
        debug_assert!(n < 1 << 24, "sieć powyżej 16,7 mln węzłów: klucz zrzutu");
        s.kolejka2.clear();
        for (i, node) in self.nodes.iter().enumerate() {
            let k = self.topo.island[i] as usize;
            if s.w[i] == 0 || s.demand[k] <= s.supply[k] * BPS {
                continue;
            }
            if let NodeRole::Connection { priority } = node.role {
                // **Malejąco po priorytecie**, rosnąco po indeksie: przemysł (3)
                // gaśnie przed gospodarstwami (2), a szpital i wodociąg (0) na
                // samym końcu. §5.4 mówił „rosnąco po priorytecie" i było to
                // odwrócone — przy tamtej kolejności pierwszy zrzut gasiłby szpital.
                s.kolejka2
                    .push(((k as u64) << 40) | (u64::from(255 - priority) << 32) | i as u64);
            }
        }
        s.kolejka2.sort_unstable();

        let mut poz = 0;
        while poz < s.kolejka2.len() {
            let k = (s.kolejka2[poz] >> 40) as usize;
            let mut koniec = poz;
            while koniec < s.kolejka2.len() && (s.kolejka2[koniec] >> 40) as usize == k {
                koniec += 1;
            }
            let moc = s.supply[k] * BPS;
            // Wyspa bez ani jednego czynnego źródła nie jest przeciążona —
            // ona po prostu nie ma prądu, i to są dwa różne zdania o sieci.
            let odciety = if s.supply[k] == 0 {
                SupplyState::Isolated
            } else {
                SupplyState::Shed
            };
            let brak = u32::try_from((s.demand[k] - moc) / BPS).unwrap_or(u32::MAX);
            let mut zostalo = s.demand[k] - moc;
            let mut ostatni = 0u8;
            let mut ile = 0usize;
            for &klucz in &s.kolejka2[poz..koniec] {
                if zostalo <= 0 {
                    break;
                }
                let p = 255 - ((klucz >> 32) & 0xFF) as u8;
                let i = (klucz & 0xFFFF_FFFF) as u32;
                zostalo -= s.w[i as usize];
                s.w[i as usize] = 0;
                self.nodes[i as usize].state = odciety;
                self.nodes[i as usize].supplied = 0;
                raport.shed_nodes.push(i);
                ostatni = p;
                ile += 1;
            }
            // Powód zapisuje się wyłącznie dla **decyzji**, czyli dla zrzutu.
            // Izolacja decyzją nie jest: nie było czego dzielić, a tłumaczy ją
            // `GridTripped` na krawędzi, która wyspę odcięła.
            if ile > 0 && odciety == SupplyState::Shed {
                raport.reasons.push(DecisionReason::LoadShed {
                    service: self.service,
                    priority: ostatni,
                    shortfall_w: brak,
                });
            }
            poz = koniec;
        }
    }

    /// Krok 4: przepływ krawędzi = suma popytu jej poddrzewa, jednym przejściem
    /// po postorderze.
    fn policz_przeplywy(&self, s: &mut Scratch) {
        s.sub.clone_from(&s.w);
        for &v in &self.topo.post_order {
            let p = self.topo.parent[v as usize];
            if p != NO_PARENT {
                s.sub[p as usize] += s.sub[v as usize];
            }
        }
    }

    /// Krok 5: zadziałanie zabezpieczeń i wejście w kolejną rundę kaskady.
    ///
    /// Sprawdzamy **tylko mosty** — krawędź w pierścieniu ma obok siebie drogę
    /// objazdową, a przepływ policzony dla niej na drzewie jest artefaktem wyboru
    /// drzewa, nie stanem sieci (patrz [`super::topo::TopologyCache`]).
    fn zadzialaj_zabezpieczenia(
        &mut self,
        s: &Scratch,
        seed: u64,
        t: Tick,
        repair: RepairWindow,
        raport: &mut SolveReport,
    ) -> u32 {
        let mut ile = 0;
        for v in 0..self.nodes.len() as u32 {
            if self.topo.parent[v as usize] == NO_PARENT {
                continue;
            }
            let e = self.topo.parent_edge[v as usize];
            if !self.topo.is_bridge[e as usize] || !self.edges[e as usize].is_active() {
                continue;
            }
            let przeplyw = s.sub[v as usize] / s.gross[v as usize];
            if przeplyw <= self.edges[e as usize].capacity {
                continue;
            }
            let minuty = repair.draw(seed, e, t);
            self.edges[e as usize].state = EdgeState::Tripped {
                until: Tick(t.0 + u64::from(minuty)),
            };
            self.dirty = true;
            raport.tripped_edges.push(e);
            raport.reasons.push(DecisionReason::GridTripped {
                service: self.service,
                repair_minutes: u16::try_from(minuty).unwrap_or(u16::MAX),
            });
            ile += 1;
        }
        ile
    }
}
