//! Związek zawodowy jako stan zakładu (M10e WP10.14, PRD §6.6, `K-9`).
//!
//! ## Czego tu nie ma i dlaczego
//!
//! **Listy członków nie ma.** Plan §5.9 zapisywał `members: Vec<CitizenId>`;
//! tą listą jest już `SocialIndex::coworkers(site)` po odsianiu uczniów (`K-74`),
//! a druga kopia musiałaby przeżyć każde zwolnienie, każdą śmierć i każdą
//! przeprowadzkę — czyli rozjechałaby się z pierwszą przy pierwszym odejściu.
//! Związek trzyma **gęstość**, a skład odtwarza z indeksu, kiedy go potrzebuje.
//!
//! **`strike_fund` nie jest osobnym kontem w `Books` i nie jest saldem.** Jest
//! **miarą wytrzymałości** załogi: sumą oszczędności jej gospodarstw ponad miesiąc
//! utrzymania, zdjętą w chwili wyjścia z pracy, topniejącą o utracone zarobki.
//! Składek związkowych nikt w tej grze nie płaci, więc konto byłoby puste.
//!
//! `ponytail:` **i to jest sufit, który trzeba nazwać wprost** — salda gospodarstw
//! w czasie strajku **nie maleją**, bo dochód gospodarstwa jest w tej grze
//! egzogeniczny (decyzja nr 2 fazy M5: płaci go abstrakcyjny pracodawca spoza
//! miasta, a `PayrollOutbox` nie ma konsumenta od M7b). Strajk zabiera więc
//! realny pieniądz **firmie** — lista płac nie płaci za dni postoju i widać to
//! w rachunku wyniku zakładu — a po stronie załogi zostaje liczbą, która mówi,
//! jak długo wytrzyma. Kiedy lista płac dojdzie do gospodarstw, ten fundusz
//! przestaje być modelem i staje się odczytem; do tego czasu jest jawnym
//! przybliżeniem, a nie księgą.

use std::collections::BTreeMap;

use magnat_core::{FirmId, HashState, Money, SiteId, StateHasher, Q};
use magnat_firms::FirmKey;

/// Numer związku — monotoniczny w obrębie gry i nigdy nie wracający.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UnionId(pub u32);

/// Żądanie płacowe.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UnionDemand {
    /// O ile procent (w punktach bazowych) załoga chce więcej.
    pub raise_bp: u16,
    /// Płaca, na którą się powołuje — **znana z grafu relacji**, nie prawdziwa
    /// mediana miejska (§5.9).
    pub anchor: Money,
}

/// Gdzie jest związek w cyklu sporu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnionState {
    /// Istnieje, ale nic nie żąda — po ugodzie albo tuż po zawiązaniu.
    Dormant { until_day: u32 },
    /// Żądanie przedstawione, firma jeszcze nie odpowiedziała.
    Demand { since_day: u32 },
    /// Negocjacje: runda i doba jej rozpoczęcia.
    Talks { round: u8, since_day: u32 },
    /// Strajk: doba rozpoczęcia i odsetek załogi, który wyszedł.
    Strike {
        since_day: u32,
        participation_bp: u16,
    },
}

impl UnionState {
    #[must_use]
    pub fn is_striking(self) -> bool {
        matches!(self, UnionState::Strike { .. })
    }

    /// Czy załoga jest w sporze — czyli czy jest wtajemniczonym niezadowolonym
    /// z punktu widzenia hazardu wykrycia zmowy (§5.9).
    #[must_use]
    pub fn in_dispute(self) -> bool {
        !matches!(self, UnionState::Dormant { .. })
    }

    fn tag(self) -> u8 {
        match self {
            UnionState::Dormant { .. } => 0,
            UnionState::Demand { .. } => 1,
            UnionState::Talks { .. } => 2,
            UnionState::Strike { .. } => 3,
        }
    }
}

/// Związek jednego zakładu.
///
/// `UnionScope::Firm` i `Branch` z planu §5.9 **nie powstają**: decyzja `D6` fazy
/// rozstrzygnęła, że związek formuje się na poziomie zakładu, bo graf relacji jest
/// lokalny — ludzie znają współpracowników, nie całą korporację. Wariant zakresu
/// bez ani jednej ścieżki, którą mógłby powstać, wygląda w kodzie tak samo jak
/// działający (`K-67`), więc czeka na eskalację razem z nią.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Union {
    pub id: UnionId,
    pub site: SiteId,
    pub firm: FirmKey,
    /// Odsetek załogi gotowej wejść w spór, w punktach bazowych.
    pub density: Q,
    /// Wojowniczość — rośnie z żalu, spada po przegranym strajku.
    pub militancy: Q,
    pub formed_day: u32,
    pub demand: Option<UnionDemand>,
    pub state: UnionState,
    /// Ile pieniądza załoga ma jeszcze na przeżycie strajku. Poza strajkiem zero.
    pub strike_fund: Money,
    /// Ile pieniądza znika z funduszu każdej doby strajku.
    pub daily_burn: Money,
}

impl Union {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        self.site.entity().hash_state(h);
        self.firm.hash_state(h);
        h.write_u8(self.density.get());
        h.write_u8(self.militancy.get());
        h.write_u32(self.formed_day);
        match self.demand {
            None => h.write_u8(0),
            Some(d) => {
                h.write_u8(1);
                h.write_u16(d.raise_bp);
                d.anchor.hash_state(h);
            }
        }
        h.write_u8(self.state.tag());
        match self.state {
            UnionState::Dormant { until_day } => h.write_u32(until_day),
            UnionState::Demand { since_day } => h.write_u32(since_day),
            UnionState::Talks { round, since_day } => {
                h.write_u8(round);
                h.write_u32(since_day);
            }
            UnionState::Strike {
                since_day,
                participation_bp,
            } => {
                h.write_u32(since_day);
                h.write_u16(participation_bp);
            }
        }
        self.strike_fund.hash_state(h);
        self.daily_burn.hash_state(h);
    }
}

/// Stan zakładu widziany przez warunki powstania związku (§5.9).
///
/// Istnieje **zanim** powstanie związek, więc nie może mieszkać w [`Union`].
/// Trzyma wszystkie trzy warunki naraz, a nie samą sumę, i to jest jego istota:
/// §6 pkt 5 dokumentu fazy żąda „`grievance` per zakład jako ostrzeżenia
/// wyprzedzającego", a suma na to nie odpowiada. Zakład, w którym żal sięga
/// progu, ale załoga się nie zna, i zakład, w którym załoga się zna, ale nie ma
/// o co walczyć, wyglądają w jednej liczbie tak samo — a są dwiema różnymi
/// sytuacjami i wymagają dwóch różnych ruchów pracodawcy.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Grievance {
    pub level: u8,
    pub days_above: u16,
    /// Największa spójna składowa grafu relacji wśród pracowników zakładu.
    pub component: u16,
    /// Gęstość potencjalnego członkostwa w punktach bazowych.
    pub density_bp: u16,
}

/// Wezwanie do strajku przekazywane w górę, do `sim/events` (§5.9).
///
/// Kierunek jest wymuszony grafem: `sim/events` zależy od `sim/economy`, nigdy
/// odwrotnie, więc związek **nie może** otworzyć zdarzenia sam. Zostawia fakt,
/// a rejestr zdarzeń go odbiera — ta sama droga, którą `Insurers::report_peril`
/// odbiera szkodę, tylko w drugą stronę.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StrikeCall {
    pub site: SiteId,
    pub firm: FirmId,
    pub participation_bp: u16,
}

/// Zmowa wykryta przez model, czekająca na urząd (`K-10`: sprawę prowadzi M8).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DetectedCartel {
    pub firm: FirmId,
    pub site: SiteId,
    pub evidence: Q,
}

/// Związki zawodowe miasta — zasób świata.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Unions {
    unions: BTreeMap<u32, Union>,
    watch: BTreeMap<u32, Grievance>,
    next_id: u32,
    calls: Vec<StrikeCall>,
    /// Ustępstwo **podstawione przez gracza** w miejsce wyliczonego z marży,
    /// per zakład, w punktach bazowych (`FF-23`, M10g WP10.22).
    ///
    /// To jest ta sama reguła, którą `PricePolicy::Fixed` stosuje do ceny (M5c):
    /// gracz podstawia własną liczbę, a nie dostaje drugiego silnika. Próg
    /// akceptacji załogi, ugoda i wyjście do strajku liczą się dalej tym samym
    /// kodem, bo `rundy` czyta **gotową ofertę**, a nie to, skąd pochodzi.
    ///
    /// Wpis zużywa się w rundzie, w której padł: gracz odpowiada na **tę** rundę,
    /// a nie ustawia politykę na całe negocjacje. Nieodebrany wchodzi do hasha
    /// stanu, bo jest tym, co świat ma do przekazania w najbliższej rundzie —
    /// ta sama zasada, co przy nieodebranych wezwaniach do strajku.
    player_offers: BTreeMap<u32, u16>,
}

impl Unions {
    #[must_use]
    pub fn new() -> Unions {
        Unions {
            next_id: 1,
            ..Unions::default()
        }
    }

    #[must_use]
    pub fn get(&self, site: SiteId) -> Option<&Union> {
        self.unions.get(&site.entity().index())
    }

    pub fn get_mut(&mut self, site: SiteId) -> Option<&mut Union> {
        self.unions.get_mut(&site.entity().index())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Union> {
        self.unions.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.unions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.unions.is_empty()
    }

    /// Gracz podstawia ustępstwo swojego zakładu na najbliższą rundę.
    ///
    /// `false` = w tym zakładzie nie toczy się spór, więc nie ma na co odpowiadać.
    /// Liczba jest **żądaniem odpowiedzi**, a nie jej skutkiem: czy załoga ją
    /// przyjmie, rozstrzyga jej próg akceptacji, ten sam co przy firmie AI.
    pub fn set_player_offer(&mut self, site: SiteId, bp: u16) -> bool {
        if !self.in_dispute(site) {
            return false;
        }
        self.player_offers.insert(site.entity().index(), bp);
        true
    }

    /// Ustępstwo podstawione przez gracza, jeśli jakieś czeka. Nie zdejmuje wpisu.
    #[must_use]
    pub fn player_offer(&self, site: SiteId) -> Option<u16> {
        self.player_offers.get(&site.entity().index()).copied()
    }

    /// Zdejmuje wpisy zużyte w tej rundzie.
    pub fn take_player_offers(&mut self) -> BTreeMap<u32, u16> {
        std::mem::take(&mut self.player_offers)
    }

    /// Czy załoga tego zakładu jest w sporze z pracodawcą — wejście hazardu
    /// wykrycia zmowy (§5.9).
    #[must_use]
    pub fn in_dispute(&self, site: SiteId) -> bool {
        self.get(site).is_some_and(|u| u.state.in_dispute())
    }

    /// Żal zakładu i licznik dób ponad progiem.
    #[must_use]
    pub fn grievance(&self, site: SiteId) -> Grievance {
        self.watch
            .get(&site.entity().index())
            .copied()
            .unwrap_or_default()
    }

    /// Wszystkie zmierzone zakłady razem z ich żalem, w kolejności indeksu.
    ///
    /// Potrzebne do pomiaru `FF-24`: ile zakładów w mieście z generatora stoi nad
    /// progiem. Sama liczba związków tego nie mówi — związek powstaje dopiero po
    /// dwóch pomiarach z rzędu i przy dość gęstej składowej grafu relacji, więc
    /// miasto z setką rozżalonych załóg i zerem związków wygląda w raporcie tak
    /// samo jak miasto zadowolone.
    pub fn watched(&self) -> impl Iterator<Item = (u32, Grievance)> + '_ {
        self.watch.iter().map(|(i, g)| (*i, *g))
    }

    /// Zapisuje pomiar żalu i przesuwa licznik o `days` dób.
    ///
    /// Pomiar jest **miesięczny**, a nie dobowy, więc krok wynosi trzydzieści.
    /// Powód jest mierzalny: średni stres załogi to przejście po wszystkich
    /// pracownikach każdego zakładu w mieście, a marża zakładu i tak zmienia się
    /// raz na miesiąc, bo tyle ma rachunek wyniku. Warunek „≥ 60 kolejnych dób"
    /// z §5.9 znaczy więc „dwa pomiary z rzędu" i to jest ta sama rzecz.
    pub fn note_grievance(
        &mut self,
        site: SiteId,
        level: u8,
        threshold: u8,
        days: u16,
        component: u32,
        density_bp: u32,
    ) -> Grievance {
        let g = self.watch.entry(site.entity().index()).or_default();
        g.level = level;
        g.component = u16::try_from(component).unwrap_or(u16::MAX);
        g.density_bp = u16::try_from(density_bp).unwrap_or(u16::MAX);
        g.days_above = if level >= threshold {
            g.days_above.saturating_add(days)
        } else {
            0
        };
        *g
    }

    /// Zakłada związek. Zwraca jego numer.
    pub fn form(&mut self, site: SiteId, firm: FirmKey, density: Q, day: u32) -> UnionId {
        let id = UnionId(self.next_id);
        self.next_id += 1;
        self.unions.insert(
            site.entity().index(),
            Union {
                id,
                site,
                firm,
                density,
                // Nowy związek jest tak wojowniczy, jak liczna jest jego baza —
                // i to jest jedyne wejście, jakie ma na starcie.
                militancy: density,
                formed_day: day,
                demand: None,
                state: UnionState::Dormant { until_day: day },
                strike_fund: Money::ZERO,
                daily_burn: Money::ZERO,
            },
        );
        id
    }

    /// Rozwiązuje związek — zakład zamknięty albo załoga rozeszła się do zera.
    pub fn dissolve(&mut self, site: SiteId) {
        self.unions.remove(&site.entity().index());
        self.watch.remove(&site.entity().index());
    }

    pub fn call_strike(&mut self, call: StrikeCall) {
        self.calls.push(call);
    }

    /// Odbiera wezwania do strajku. Opróżnia skrzynkę — wywołanie odebrane dwa razy
    /// otworzyłoby dwa zdarzenia na jeden strajk.
    pub fn take_calls(&mut self) -> Vec<StrikeCall> {
        std::mem::take(&mut self.calls)
    }
}

impl HashState for Unions {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.unions.len() as u32);
        for (site, u) in &self.unions {
            h.write_u32(*site);
            u.hash_state(h);
        }
        // Licznik żalu wchodzi do hasha, bo to on decyduje, kiedy powstanie związek —
        // stan, nie pomiar pomocniczy.
        h.write_u32(self.watch.len() as u32);
        for (site, g) in &self.watch {
            h.write_u32(*site);
            h.write_u8(g.level);
            h.write_u16(g.days_above);
            h.write_u16(g.component);
            h.write_u16(g.density_bp);
        }
        h.write_u32(self.next_id);
        // Nieodebrane wezwania są tym, co świat ma do przekazania w następnym kroku —
        // ta sama zasada, co przy `b2b_outbox` (`K-75`).
        h.write_u32(self.calls.len() as u32);
        for c in &self.calls {
            c.site.entity().hash_state(h);
            h.write_u16(c.participation_bp);
        }
        // Ustępstwo gracza czekające na rundę — tak samo jak wezwanie: stan, który
        // świat ma do przekazania, a nie pomiar pomocniczy.
        h.write_u32(self.player_offers.len() as u32);
        for (site, bp) in &self.player_offers {
            h.write_u32(*site);
            h.write_u16(*bp);
        }
    }
}
