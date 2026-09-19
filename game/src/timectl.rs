//! Czas, warunki zatrzymania i tryb „śledź" (M9e WP11, §5.10, PRD §14.5).
//!
//! # `TimeScale` to `SimSpeed`, a nie drugi enum
//!
//! §5.10 rysował `TimeScale { Paused, X1, X3, X10, X50 }`. Zegar gry mieszka jednak
//! w `engine/core` od `K-22` i to **on** jest właścicielem prędkości; drugi słownik
//! o tym samym znaczeniu rozjechałby się z pierwszym przy pierwszej zmianie, a jego
//! dyskryminanty wchodzą do zapisu. Dlatego [`TimeScale`] jest nazwą na `SimSpeed`,
//! a `X50` **nie powstaje tutaj**: `K-22` mówi wprost, że dopisuje go M12 na końcu
//! enuma, razem z trybem 50× po stronie symulacji.
//!
//! # Zatrzymanie jest stroną widoku i dlatego nic nie psuje
//!
//! Warunki sprawdzają się co godzinę gry na **stanie, który już jest** — nie liczą
//! niczego, czego symulacja nie policzyła, i niczego nie zapisują. Trafienie ustawia
//! pauzę na granicy klatki. Ponieważ pauza to `SimSpeed::Paused`, a prędkość nie
//! wchodzi do hasha (`K-22`), zatrzymanie z definicji nie może zmienić wyniku.
//!
//! # Czego w zestawie nie ma i dlaczego
//!
//! §5.10 wymienia jedenaście gotowych warunków. Trzy z nich nie mają dziś czego
//! obserwować i **nie powstają**, bo wariant, którego nikt nigdy nie zgłosi, przechodzi
//! każdy test i wygląda tak samo jak działający (`K-67`):
//!
//! - **strajk** — związki zawodowe i negocjacje należą do M10 (`K-9`);
//! - **nieudana dostawa** — `sim/supply` nie prowadzi dziennika niedowiezionych
//!   zleceń, a licznik kar mówi o pieniądzu, nie o zdarzeniu;
//! - **wojna cenowa na towarze X** wchodzi bez towaru: kampania konkurencyjna
//!   (`Firm::campaign`) niesie `ReactionKind::PriceWar` i towar, ale odpytanie o nią
//!   „po towarze" wymagałoby przejścia po wszystkich firmach miasta co godzinę —
//!   warunek pyta więc o **moich** konkurentów, a nie o cały rynek.
//!
//! Wrócą razem ze swoimi mechanikami i wtedy dostaną numery — tak samo jak wiersze
//! tabeli powodów z `DG-1`.

use magnat_core::{Money, SiteId, Subject};
use serde::{Deserialize, Serialize};

use crate::career::Holdings;
use crate::Session;

/// Prędkość upływu czasu gry. Nazwa na `SimSpeed` z `engine/core` (`K-22`).
pub type TimeScale = magnat_core::SimSpeed;

/// Numer warunku zatrzymania w sesji. Nadaje go [`StopWatch::arm`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct StopConditionId(pub u8);

/// Gotowy warunek zatrzymania — **zestaw, nie wyrażenie**.
///
/// §5.10 zapowiadał `StopCondition { expr: ConditionExpr }`. Tak się nie da i to
/// jest ten sam powód, dla którego `EntityFilter` przestał być wyrażeniem (`DG-4`):
/// metryki języka reguł opisują **jeden zakład** i nie mają kwantyfikatora, a każdy
/// warunek z tej listy pyta o „którykolwiek mój". Drzewo składniowe byłoby pustą ramą,
/// a gracz i tak wybiera z listy — §5.10 sam mówi „bez budowania wyrażenia".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum StopCondition {
    /// Któryś mój sklep ma pustą półkę.
    NoStock,
    /// Gotówka gospodarstwa gracza spadła poniżej progu.
    CashBelow { amount: Money },
    /// Polityka zgłosiła coś właścicielowi (`Market::policy_inbox`, `DH-1`).
    PolicyEscalation,
    /// Umowa dostawy kończy się za mniej niż `days` dób.
    ContractExpiring { days: u16 },
    /// Konkurent otworzył punkt w promieniu `radius_m` od któregoś mojego.
    CompetitorOpened { radius_m: u32 },
    /// Któraś moja linia produkcyjna stoi z powodu awarii.
    MachineBreakdown,
    /// Konkurent, z którym graniczę, ruszył wojnę cenową.
    PriceWar,
    /// W mieście zaczęło się zdarzenie.
    CityEvent,
    /// Cel scenariusza został osiągnięty (WP12).
    ObjectiveMet,
}

impl StopCondition {
    /// Klucz tekstu: `ui.stop.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            StopCondition::NoStock => "no_stock",
            StopCondition::CashBelow { .. } => "cash_below",
            StopCondition::PolicyEscalation => "policy_escalation",
            StopCondition::ContractExpiring { .. } => "contract_expiring",
            StopCondition::CompetitorOpened { .. } => "competitor_opened",
            StopCondition::MachineBreakdown => "machine_breakdown",
            StopCondition::PriceWar => "price_war",
            StopCondition::CityEvent => "city_event",
            StopCondition::ObjectiveMet => "objective_met",
        }
    }

    /// Zestaw domyślny — to, co gracz dostaje na liście bez budowania czegokolwiek.
    #[must_use]
    pub fn ready_set() -> Vec<StopCondition> {
        vec![
            StopCondition::NoStock,
            StopCondition::CashBelow {
                amount: Money(100_000),
            },
            StopCondition::PolicyEscalation,
            StopCondition::ContractExpiring { days: 7 },
            StopCondition::CompetitorOpened { radius_m: 1_000 },
            StopCondition::MachineBreakdown,
            StopCondition::PriceWar,
            StopCondition::CityEvent,
            StopCondition::ObjectiveMet,
        ]
    }
}

/// Trafienie warunku: co się stało i na co patrzeć.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StopHit {
    pub id: StopConditionId,
    pub cond: StopCondition,
    /// Podmiot, który to spowodował — kliknięcie w alert otwiera jego kartę.
    /// `None`, gdy warunek mówi o mieście, a nie o konkretnym bycie.
    pub subject: Option<Subject>,
}

/// Uzbrojone warunki i pamięć potrzebna do tych, które pytają o **zmianę**.
pub struct StopWatch {
    armed: Vec<(StopConditionId, StopCondition, bool)>,
    /// Warunki, które **już zatrzymały grę** i czekają, aż przestaną obowiązywać.
    ///
    /// Bez tego warunek poziomowy („saldo poniżej progu", „pusta półka") pauzowałby
    /// grę co godzinę bez końca: gracz wznawia, mija godzina, warunek dalej trzyma,
    /// pauza wraca. Zatrzask zdejmuje się sam, gdy warunek przestaje trafiać — więc
    /// drugie wpadnięcie w ten sam kłopot znowu zatrzyma.
    zatrzasniete: Vec<StopConditionId>,
    next_id: u8,
    /// Zakłady miasta widziane przy ostatnim sprawdzeniu — bez tego „konkurent
    /// otworzył punkt" nie ma z czym porównać. Pusty przed pierwszym sprawdzeniem,
    /// więc pierwsza godzina niczego nie zgłasza: świat zastany nie jest zmianą.
    znane_zaklady: Vec<SiteId>,
    /// Ostatnia godzina gry, o której sprawdzano — warunki chodzą `EveryHour`.
    ostatnia_godzina: u64,
    /// Ile wpisów skrzynki eskalacji widziano — pyta o przyrost, nie o stan.
    znane_eskalacje: usize,
    /// Ile wpisów kroniki zdarzeń widziano.
    znane_zdarzenia: usize,
    /// Czy cel scenariusza został już zgłoszony. Cel osiągnięty zostaje osiągnięty,
    /// więc bez tej pamięci zatrzymywałby grę co godzinę do końca świata.
    cel_zgloszony: bool,
}

impl Default for StopWatch {
    /// Zegar zaczyna od godziny, której nie ma.
    ///
    /// Zero byłoby **pierwszą godziną gry** i pierwsze sprawdzenie wypadłoby z niej
    /// jako „ta sama co ostatnio" — warunek trafiający w pierwszej dobie milczałby
    /// do drugiej. Wykryte testem progu gotówki (`DI-3`).
    fn default() -> StopWatch {
        StopWatch {
            armed: Vec::new(),
            zatrzasniete: Vec::new(),
            next_id: 0,
            znane_zaklady: Vec::new(),
            ostatnia_godzina: u64::MAX,
            znane_eskalacje: 0,
            znane_zdarzenia: 0,
            cel_zgloszony: false,
        }
    }
}

impl StopWatch {
    /// Uzbraja warunek i zwraca jego numer.
    pub fn arm(&mut self, c: StopCondition) -> StopConditionId {
        let id = StopConditionId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.armed.push((id, c, true));
        id
    }

    /// Włącza albo wyłącza warunek. Wyłączony zostaje na liście — gracz ma go
    /// znaleźć tam, gdzie go zostawił.
    pub fn set(&mut self, id: StopConditionId, on: bool) {
        for w in &mut self.armed {
            if w.0 == id {
                w.2 = on;
            }
        }
    }

    #[must_use]
    pub fn armed(&self) -> Vec<(StopConditionId, StopCondition, bool)> {
        self.armed.clone()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.armed.is_empty()
    }

    /// Sprawdza warunki, jeśli minęła godzina gry. Zwraca **pierwsze** trafienie
    /// w kolejności uzbrojenia — gracz zatrzymuje się raz, a nie dziewięć razy.
    ///
    /// `objective_met` przychodzi z zewnątrz, bo cele scenariusza liczy `game::scenario`
    /// i to on wie, czy któryś właśnie się domknął.
    pub fn check(&mut self, session: &Session, objective_met: bool) -> Option<StopHit> {
        let godzina = session.tick().get() / 60;
        if godzina == self.ostatnia_godzina {
            return None;
        }
        self.ostatnia_godzina = godzina;
        let h = Holdings::of(session);
        let nowe = self.nowe_zaklady(session);
        let mut trafienie = None;
        for (id, c, on) in self.armed.clone() {
            // Sprawdzamy **także warunki wyłączone**, bo trzy z nich pytają o przyrost
            // (eskalacje, zdarzenia miasta) i ich liczniki muszą iść dalej. Inaczej
            // włączenie warunku po godzinie przerwy zgłaszałoby przyrost, który
            // nastąpił, kiedy nikt nie patrzył.
            let hit = self.sprawdz(session, &h, &nowe, c, objective_met);
            let trafil = hit.is_some();
            let bylo = self.zatrzasniete.contains(&id);
            if !trafil && bylo {
                self.zatrzasniete.retain(|x| *x != id);
            }
            if !on || !trafil || bylo {
                continue;
            }
            self.zatrzasniete.push(id);
            if trafienie.is_none() {
                trafienie = Some(StopHit {
                    id,
                    cond: c,
                    subject: hit.flatten(),
                });
            }
        }
        trafienie
    }

    /// Zakłady, których godzinę temu w mieście nie było.
    fn nowe_zaklady(&mut self, session: &Session) -> Vec<SiteId> {
        let Some(m) = session.market.as_ref() else {
            return Vec::new();
        };
        let teraz = m.sites();
        if self.znane_zaklady.is_empty() {
            self.znane_zaklady = teraz;
            return Vec::new();
        }
        let nowe: Vec<SiteId> = teraz
            .iter()
            .filter(|s| !self.znane_zaklady.contains(s))
            .copied()
            .collect();
        self.znane_zaklady = teraz;
        nowe
    }

    /// `Some(subject)` = warunek trafił; wewnętrzne `None` znaczy „trafił, ale nie ma
    /// na co wskazać".
    #[allow(clippy::option_option)]
    fn sprawdz(
        &mut self,
        session: &Session,
        h: &Holdings,
        nowe: &[SiteId],
        c: StopCondition,
        objective_met: bool,
    ) -> Option<Option<Subject>> {
        match c {
            StopCondition::NoStock => pusta_polka(session, h).map(|s| Some(Subject::Site(s))),
            StopCondition::CashBelow { amount } => {
                (crate::metrics::gotowka_gracza(session).get() < amount.get()).then_some(None)
            }
            StopCondition::PolicyEscalation => {
                let m = session.market.as_ref()?;
                let ile = m.policy_inbox().len();
                let przyrost = ile > self.znane_eskalacje;
                self.znane_eskalacje = ile;
                przyrost.then_some(None)
            }
            StopCondition::ContractExpiring { days } => {
                konczaca_umowa(session, h, days).map(|s| Some(Subject::Site(s)))
            }
            StopCondition::CompetitorOpened { radius_m } => {
                blisko_nowy(session, h, nowe, radius_m).map(|s| Some(Subject::Site(s)))
            }
            StopCondition::MachineBreakdown => awaria(session, h).map(|s| Some(Subject::Site(s))),
            StopCondition::PriceWar => wojna_cenowa(session, h).map(|f| Some(Subject::Firm(f))),
            StopCondition::CityEvent => {
                let ev = session.app.world.get_resource::<magnat_events::Events>()?;
                let ile = ev.chronicle().len();
                let przyrost = ile > self.znane_zdarzenia;
                self.znane_zdarzenia = ile;
                przyrost.then_some(None)
            }
            StopCondition::ObjectiveMet => {
                if objective_met && !self.cel_zgloszony {
                    self.cel_zgloszony = true;
                    Some(None)
                } else {
                    None
                }
            }
        }
    }
}

/// Pierwszy zakład gracza z pustą półką.
fn pusta_polka(session: &Session, h: &Holdings) -> Option<SiteId> {
    let m = session.market.as_ref()?;
    h.sites.iter().copied().find(|s| {
        m.goods_of(*s)
            .into_iter()
            .any(|g| m.shelf_qty(*s, g).is_some_and(|q| q.get() <= 0))
    })
}

/// Pierwsza umowa dostawy gracza kończąca się w ciągu `days` dób.
fn konczaca_umowa(session: &Session, h: &Holdings, days: u16) -> Option<SiteId> {
    let m = session.market.as_ref()?;
    let prog = session.tick().get() + u64::from(days) * magnat_core::time::MINUTES_PER_DAY;
    let chain = m.chain();
    let c = chain.lock();
    let znaleziony = c.b2b.contracts().find_map(|k| {
        (h.sites.contains(&k.deliver_to) && k.valid_to.0 <= prog).then_some(k.deliver_to)
    });
    znaleziony
}

/// Nowy zakład w promieniu od któregoś zakładu gracza.
fn blisko_nowy(session: &Session, h: &Holdings, nowe: &[SiteId], radius_m: u32) -> Option<SiteId> {
    let m = session.market.as_ref()?;
    for n in nowe {
        if h.sites.contains(n) {
            continue;
        }
        let Some(pos) = m.shop_pos(*n) else { continue };
        for moj in &h.sites {
            let Some(p) = m.shop_pos(*moj) else { continue };
            if (pos - p).length() <= radius_m as f32 {
                return Some(*n);
            }
        }
    }
    None
}

/// Pierwszy zakład gracza, którego linia stoi z powodu awarii.
fn awaria(session: &Session, h: &Holdings) -> Option<SiteId> {
    let m = session.market.as_ref()?;
    let chain = m.chain();
    let c = chain.lock();
    let znaleziony = h.sites.iter().copied().find(|s| {
        c.plant.get(*s).is_some_and(|z| {
            z.lines
                .iter()
                .any(|l| matches!(l.state, magnat_supply::LineState::Broken { .. }))
        })
    });
    znaleziony
}

/// Firma, która ruszyła wojnę cenową i sąsiaduje z zakładem gracza.
///
/// Pyta o **moich** konkurentów, a nie o cały rynek: obraz konkurencji sklepu jest
/// już zbierany co dobę, a przejście po wszystkich firmach miasta co godzinę byłoby
/// kosztem płaconym przez każdą godzinę gry za zdarzenie rzadkie z definicji.
fn wojna_cenowa(session: &Session, h: &Holdings) -> Option<magnat_core::FirmId> {
    let m = session.market.as_ref()?;
    let firms = session.app.world.get_resource::<magnat_firms::Firms>()?;
    for moj in &h.sites {
        for g in m.goods_of(*moj) {
            let Some(k) = m.observed_of(*moj, g) else {
                continue;
            };
            let Some(f) = firms.site(k.cheapest_site).map(|z| z.firm) else {
                continue;
            };
            let wojna = firms
                .get(f)
                .and_then(|x| x.campaign)
                .is_some_and(|c| c.kind == magnat_core::ReactionKind::PriceWar);
            if wojna {
                return Some(magnat_firms::firm_id(f));
            }
        }
    }
    None
}

/// Kogo śledzi kamera (§5.10).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FollowTarget {
    Citizen(magnat_core::CitizenId),
    Vehicle(magnat_core::VehicleId),
    /// Partia towaru — „od pola do półki". Uchwyt areny z zatartym typem (`K-69`).
    Batch(magnat_core::ArenaRef),
}

impl FollowTarget {
    /// Podmiot, którego kartę otwiera kliknięcie w śledzony obiekt.
    #[must_use]
    pub const fn subject(self) -> Subject {
        match self {
            FollowTarget::Citizen(c) => Subject::Citizen(c),
            FollowTarget::Vehicle(v) => Subject::Vehicle(v),
            FollowTarget::Batch(b) => Subject::Batch(b),
        }
    }
}

/// Tryb „śledź": cel i przypięcie do warstwy Mikro.
///
/// Przypięcie robi `player::pin_micro` — mechanizm z `DG-8`, sufit `MAX_PINNED = 8`.
/// Tutaj jest wyłącznie wybór celu; kamera należy do klienta, a oś czasu doby
/// rysuje gotowy `magnat_ui::DayTimeline` z karty mieszkańca.
#[derive(Default)]
pub struct Follow {
    target: Option<FollowTarget>,
}

impl Follow {
    #[must_use]
    pub const fn target(&self) -> Option<FollowTarget> {
        self.target
    }

    /// Ustawia cel i przypina go do Mikro, jeśli to mieszkaniec.
    ///
    /// Pojazd i partia przypięcia nie dostają i to nie jest brak: warstwa Mikro
    /// pojazdów czeka na M11 (`R-2`), a partia nie jest encją ECS (`K-16`), więc
    /// nie ma czego przypinać — jedzie w skrzyni, a skrzynię wiezie pojazd.
    pub fn set(&mut self, session: &Session, target: Option<FollowTarget>) {
        self.target = target;
        let ludzie: Vec<magnat_core::CitizenId> = session
            .player()
            .map(|p| p.citizen)
            .into_iter()
            .chain(match target {
                Some(FollowTarget::Citizen(c)) => Some(c),
                _ => None,
            })
            .collect();
        crate::player::pin_micro(&session.app.world, &ludzie);
    }
}
