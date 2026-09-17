//! Przetargi miejskie: miasto kupuje usługę zamiast ją prowadzić (M8e WP9, PRD §10.3).
//!
//! **Odbiór odpadów jest tu pierwszym i jedynym przedmiotem z prawdziwym
//! wolumenem, i nie jest to przypadek.** `SpendCategory::Waste` ma udział w planie
//! wydatków od M8a, a `ServiceKind::Waste` nie ma ani jednego archetypu budynku
//! w `data/buildings/` — miasto płaci więc co miesiąc za usługę, której w mieście
//! nie ma (pozycja 46 wykazu `R2`). Przetarg zamyka tę dziurę od strony, z której
//! da się ją zamknąć teraz: pieniądz idzie do **firmy**, która to robi, a nie na
//! konto reszty świata.
//!
//! I to jest cała nowość w przepływie: **pierwszy wydatek publiczny, który ląduje
//! na koncie kogoś z tego miasta.** Reszta planu wydatków dalej wychodzi poza
//! model (`ponytail:` sufit z M8a) i czeka na miasto jako pracodawcę.
//!
//! Punktacja jest całkowitoliczbowa i jawna: cztery kryteria, wagi w punktach
//! bazowych, wynik w punktach bazowych. Przegrany ma się dowiedzieć, ilu punktów
//! mu zabrakło — a nie że „wybrano korzystniejszą ofertę" (PRD §14.1).

use std::collections::BTreeMap;

use magnat_core::{
    DecisionReason, DistrictId, Money, SiteId, StateHasher, TenderKind, Tick, Q,
};
use magnat_firms::FirmKey;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TenderId(pub u32);

/// Przedmiot przetargu: rodzaj plus identyfikator (dzielnica albo linia).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct TenderSubject {
    pub kind: TenderKind,
    pub id: u16,
}

/// Wagi kryteriów w punktach bazowych. Suma **nie musi** dawać 10 000 —
/// normalizuje je [`score_bp`], bo rada zmieniająca jedną wagę nie ma obowiązku
/// przeliczać pozostałych trzech.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BidCriteria {
    pub price_bp: u32,
    pub quality_bp: u32,
    pub delivery_bp: u32,
    pub local_bp: u32,
}

impl Default for BidCriteria {
    /// Cena waży najwięcej, ale nie wszystko — przetarg rozstrzygany wyłącznie
    /// ceną nie miałby po co mieć kryteriów.
    fn default() -> BidCriteria {
        BidCriteria {
            price_bp: 5_000,
            quality_bp: 2_500,
            delivery_bp: 1_500,
            local_bp: 1_000,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bid {
    pub bidder: FirmKey,
    pub site: SiteId,
    /// Cena za miesiąc usługi.
    pub price: Money,
    pub quality_promise: Q,
    pub delivery_days: u16,
    /// Czy oferent ma zakład w dzielnicy, której dotyczy przedmiot.
    pub local: bool,
    /// Nielegalne. Podnosi punktację **u skorumpowanego urzędnika**, a razem z nią
    /// hazard sprawy — do M9 nikt go nie składa i pole czeka na gracza.
    pub kickback: Option<Money>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TenderOutcome {
    pub winner: FirmKey,
    pub site: SiteId,
    pub price: Money,
    pub score_bp: u32,
    pub runner_up_bp: u32,
}

#[derive(Clone, Debug)]
pub struct Tender {
    pub id: TenderId,
    pub subject: TenderSubject,
    pub published_at: Tick,
    pub bid_deadline: Tick,
    /// Ile miasto zamierza wydać miesięcznie. Oferta droższa odpada z progu.
    pub budget: Money,
    pub criteria: BidCriteria,
    pub bids: Vec<Bid>,
    pub outcome: Option<TenderOutcome>,
    /// Postępowanie zamknięte bez rozstrzygnięcia — nikt nie złożył oferty.
    ///
    /// Osobne pole od `outcome`, bo to są dwie różne odpowiedzi: „wygrał ten"
    /// i „nie przyszedł nikt". Pierwsza wersja nie miała tego pola i przetarg
    /// bez ofert **blokował przedmiot na zawsze** — `publish` widział wiszące
    /// postępowanie, a `close_due` co trzydzieści dób wypisywało do dziennika
    /// ten sam powód bez końca.
    pub closed: bool,
}

/// Umowa zawarta w wyniku przetargu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ServiceContract {
    pub winner: FirmKey,
    pub site: SiteId,
    pub price: Money,
    pub until: Tick,
}

/// Rejestr przetargów i umów miasta.
#[derive(Clone, Default, Debug)]
pub struct TenderRegistry {
    tenders: Vec<Tender>,
    contracts: BTreeMap<(u8, u16), ServiceContract>,
    next: u32,
}

impl TenderRegistry {
    #[must_use]
    pub fn new() -> TenderRegistry {
        TenderRegistry::default()
    }

    #[must_use]
    pub fn all(&self) -> &[Tender] {
        &self.tenders
    }

    #[must_use]
    pub fn get(&self, id: TenderId) -> Option<&Tender> {
        self.tenders.iter().find(|t| t.id == id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tenders.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tenders.is_empty()
    }

    /// Umowa obowiązująca dla danego przedmiotu w chwili `t`.
    #[must_use]
    pub fn contract(&self, subject: TenderSubject, t: Tick) -> Option<&ServiceContract> {
        self.contracts
            .get(&(subject.kind.as_index() as u8, subject.id))
            .filter(|c| c.until.0 > t.0)
    }

    /// Wszystkie obowiązujące umowy — treść zakładki „Przetargi" karty rady.
    pub fn contracts_in_force(&self, t: Tick) -> impl Iterator<Item = (TenderSubject, &ServiceContract)> {
        self.contracts.iter().filter_map(move |((k, id), c)| {
            if c.until.0 <= t.0 {
                return None;
            }
            TenderKind::from_index(usize::from(*k)).map(|kind| (TenderSubject { kind, id: *id }, c))
        })
    }

    /// Ogłoszenie przetargu. Zwraca `None`, jeśli na ten przedmiot już trwa
    /// postępowanie albo obowiązuje umowa.
    pub fn publish(
        &mut self,
        subject: TenderSubject,
        budget: Money,
        criteria: BidCriteria,
        deadline_days: u16,
        t: Tick,
    ) -> Option<(TenderId, DecisionReason)> {
        if self.contract(subject, t).is_some() {
            return None;
        }
        if self
            .tenders
            .iter()
            .any(|x| x.subject == subject && x.outcome.is_none() && !x.closed)
        {
            return None;
        }
        let id = TenderId(self.next);
        self.next += 1;
        self.tenders.push(Tender {
            id,
            subject,
            published_at: t,
            bid_deadline: Tick(t.0 + u64::from(deadline_days) * 1_440),
            budget,
            criteria,
            bids: Vec::new(),
            outcome: None,
            closed: false,
        });
        Some((
            id,
            DecisionReason::TenderPublished {
                subject: subject.kind,
                subject_id: subject.id,
                budget,
            },
        ))
    }

    /// Złożenie oferty. Odrzuca po terminie, po rozstrzygnięciu i powyżej budżetu.
    pub fn submit_bid(&mut self, id: TenderId, bid: Bid, t: Tick) -> bool {
        let Some(x) = self.tenders.iter_mut().find(|x| x.id == id) else {
            return false;
        };
        if x.outcome.is_some()
            || x.closed
            || t.0 > x.bid_deadline.0
            || bid.price.get() > x.budget.get()
        {
            return false;
        }
        // Ta sama firma nie licytuje dwa razy — druga oferta zastępuje pierwszą.
        if let Some(stara) = x.bids.iter_mut().find(|b| b.bidder == bid.bidder) {
            *stara = bid;
        } else {
            x.bids.push(bid);
        }
        true
    }

    /// Rozstrzygnięcie przetargów, którym minął termin (`EveryDay`).
    ///
    /// Zwraca powody do dziennika. Przetarg **bez ofert też się rozstrzyga** —
    /// wynikiem „nikt nie przyszedł", bo postępowanie wiszące w nieskończoność
    /// wygląda w raporcie tak samo jak postępowanie, którego nikt nie ogłosił.
    pub fn close_due(&mut self, contract_months: u16, t: Tick) -> Vec<DecisionReason> {
        let mut powody = Vec::new();
        for x in &mut self.tenders {
            if x.outcome.is_some() || x.closed || t.0 < x.bid_deadline.0 {
                continue;
            }
            let mut punkty: Vec<(u32, usize)> = x
                .bids
                .iter()
                .enumerate()
                .map(|(i, b)| (score_bp(b, x, &x.criteria), i))
                .collect();
            // Malejąco po punktach, rosnąco po kluczu firmy — remis jawny.
            punkty.sort_unstable_by(|a, b| {
                b.0.cmp(&a.0)
                    .then(x.bids[a.1].bidder.0.cmp(&x.bids[b.1].bidder.0))
            });
            let Some((score, i)) = punkty.first().copied() else {
                // Brak ofert: postępowanie **kończy się** wynikiem „nie przyszedł
                // nikt", a przedmiot wraca na rynek. Miasto robi tę usługę samo
                // i płaci za nią plan, dopóki ktoś nie stanie do następnego.
                x.closed = true;
                powody.push(DecisionReason::TenderAwarded {
                    subject: x.subject.kind,
                    price: Money::ZERO,
                    score_bp: 0,
                    runner_up_bp: 0,
                    bids: 0,
                });
                continue;
            };
            let drugi = punkty.get(1).map_or(0, |(s, _)| *s);
            let b = x.bids[i];
            x.outcome = Some(TenderOutcome {
                winner: b.bidder,
                site: b.site,
                price: b.price,
                score_bp: score,
                runner_up_bp: drugi,
            });
            self.contracts.insert(
                (x.subject.kind.as_index() as u8, x.subject.id),
                ServiceContract {
                    winner: b.bidder,
                    site: b.site,
                    price: b.price,
                    until: Tick(t.0 + u64::from(contract_months) * 43_200),
                },
            );
            powody.push(DecisionReason::TenderAwarded {
                subject: x.subject.kind,
                price: b.price,
                score_bp: u16::try_from(score).unwrap_or(u16::MAX),
                runner_up_bp: u16::try_from(drugi).unwrap_or(u16::MAX),
                bids: u8::try_from(x.bids.len()).unwrap_or(u8::MAX),
            });
        }
        powody
    }

    /// Zrywa umowę, której miasto nie zapłaciło. Wykonawca przestaje mieć
    /// zobowiązanie, a przedmiot wraca na rynek przy najbliższym przetargu.
    pub fn terminate(&mut self, site: SiteId, t: Tick) {
        for c in self.contracts.values_mut() {
            if c.site == site && c.until.0 > t.0 {
                c.until = t;
            }
        }
    }

    pub fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.tenders.len() as u64);
        h.write_u32(self.next);
        for x in &self.tenders {
            h.write_u32(x.id.0);
            h.write_u8(x.subject.kind.as_index() as u8);
            h.write_u16(x.subject.id);
            h.write_u64(x.bid_deadline.0);
            h.write_i64(x.budget.get());
            h.write_u64(x.bids.len() as u64);
            for b in &x.bids {
                h.write_u64(b.bidder.0);
                h.write_i64(b.price.get());
            }
            h.write_u8(u8::from(x.closed));
            match &x.outcome {
                None => h.write_u8(0),
                Some(o) => {
                    h.write_u8(1);
                    h.write_u64(o.winner.0);
                    h.write_i64(o.price.get());
                    h.write_u32(o.score_bp);
                }
            }
        }
        for ((k, id), c) in &self.contracts {
            h.write_u8(*k);
            h.write_u16(*id);
            h.write_u64(c.winner.0);
            h.write_i64(c.price.get());
            h.write_u64(c.until.0);
        }
    }
}

/// Punktacja oferty w punktach bazowych, 0..=10 000.
///
/// Cena liczy się **odwrotnie i wobec budżetu**: oferta za darmo daje pełne
/// 10 000, oferta na cały budżet — zero. To jest jedyna postać, w której da się
/// porównać cenę z jakością bez wprowadzania kursu wymiany między złotówką
/// a punktem `Q`, a każda inna postać takiego kursu byłaby ukryta.
#[must_use]
pub fn score_bp(b: &Bid, t: &Tender, c: &BidCriteria) -> u32 {
    let suma_wag = i64::from(c.price_bp + c.quality_bp + c.delivery_bp + c.local_bp).max(1);
    let budzet = t.budget.get().max(1);
    let cena = ((budzet - b.price.get()).max(0) * 10_000 / budzet).min(10_000);
    let jakosc = i64::from(b.quality_promise.get()) * 100;
    // Termin: natychmiast = 10 000, sześćdziesiąt dób i dłużej = 0.
    let termin = ((60i64 - i64::from(b.delivery_days)).max(0) * 10_000 / 60).min(10_000);
    let lokalnosc = i64::from(u8::from(b.local)) * 10_000;
    let razem = cena * i64::from(c.price_bp)
        + jakosc * i64::from(c.quality_bp)
        + termin * i64::from(c.delivery_bp)
        + lokalnosc * i64::from(c.local_bp);
    u32::try_from((razem / suma_wag).clamp(0, 10_000)).unwrap_or(0)
}

/// Miesięczna kwota do zapłacenia z obowiązujących umów, z rozbiciem na odbiorców.
///
/// Osobna funkcja, bo przelew robi krok miesięczny miasta, który ma `&mut Books`,
/// a rejestr przetargów go nie ma i mieć nie powinien.
#[must_use]
pub fn due_this_month(reg: &TenderRegistry, t: Tick) -> Vec<(SiteId, Money)> {
    reg.contracts_in_force(t)
        .map(|(_, c)| (c.site, c.price))
        .collect()
}

/// Dzielnica przedmiotu przetargu, jeśli przedmiot jest dzielnicowy.
#[must_use]
pub fn subject_district(s: TenderSubject) -> Option<DistrictId> {
    match s.kind {
        TenderKind::WasteCollection | TenderKind::RoadMaintenance => Some(DistrictId(s.id)),
        TenderKind::TransitLine | TenderKind::Construction => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oferta(klucz: u64, cena: i64, jakosc: u8, dni: u16, lokalna: bool) -> Bid {
        Bid {
            bidder: FirmKey(klucz),
            site: SiteId(magnat_core::Entity::new(
                u32::try_from(klucz).unwrap_or(0),
                std::num::NonZeroU32::new(1).expect("generacja"),
            )),
            price: Money(cena),
            quality_promise: Q::new(jakosc),
            delivery_days: dni,
            local: lokalna,
            kickback: None,
        }
    }

    fn rejestr() -> (TenderRegistry, TenderId) {
        let mut r = TenderRegistry::new();
        let (id, _) = r
            .publish(
                TenderSubject {
                    kind: TenderKind::WasteCollection,
                    id: 3,
                },
                Money(1_000_000),
                BidCriteria::default(),
                14,
                Tick(0),
            )
            .expect("ogłoszenie");
        (r, id)
    }

    #[test]
    fn tanszy_i_lepszy_wygrywa_a_przegrany_zna_roznice() {
        let (mut r, id) = rejestr();
        assert!(r.submit_bid(id, oferta(1, 900_000, 60, 30, false), Tick(1)));
        assert!(r.submit_bid(id, oferta(2, 700_000, 80, 10, true), Tick(1)));
        let powody = r.close_due(24, Tick(14 * 1_440));
        assert_eq!(powody.len(), 1);
        let o = r.get(id).expect("przetarg").outcome.expect("rozstrzygnięcie");
        assert_eq!(o.winner, FirmKey(2));
        assert!(o.score_bp > o.runner_up_bp);
        assert!(o.runner_up_bp > 0, "przegrany ma mieć punktację, nie zero");
    }

    #[test]
    fn oferta_ponad_budzet_odpada_a_po_terminie_nie_wchodzi() {
        let (mut r, id) = rejestr();
        assert!(!r.submit_bid(id, oferta(1, 1_200_000, 90, 1, true), Tick(1)));
        assert!(!r.submit_bid(id, oferta(2, 500_000, 90, 1, true), Tick(30 * 1_440)));
        assert!(r.get(id).expect("przetarg").bids.is_empty());
    }

    #[test]
    fn przetarg_bez_ofert_konczy_sie_odpowiedzia_a_nie_cisza() {
        let (mut r, _) = rejestr();
        let powody = r.close_due(24, Tick(14 * 1_440));
        assert_eq!(powody.len(), 1);
        assert!(matches!(
            powody[0],
            DecisionReason::TenderAwarded { bids: 0, .. }
        ));
        // Postępowanie jest zamknięte, więc następna doba nie liczy go od nowa…
        assert!(r.close_due(24, Tick(14 * 1_440 + 1_440)).is_empty());
        // …a przedmiot wraca na rynek, zamiast zostać zablokowany na zawsze.
        assert!(r
            .publish(
                TenderSubject {
                    kind: TenderKind::WasteCollection,
                    id: 3,
                },
                Money(1_000_000),
                BidCriteria::default(),
                14,
                Tick(20 * 1_440),
            )
            .is_some());
    }

    #[test]
    fn umowa_obowiazuje_do_terminu_i_blokuje_kolejny_przetarg() {
        let (mut r, id) = rejestr();
        let s = TenderSubject {
            kind: TenderKind::WasteCollection,
            id: 3,
        };
        r.submit_bid(id, oferta(7, 400_000, 70, 5, true), Tick(1));
        r.close_due(24, Tick(14 * 1_440));
        let t = Tick(20 * 1_440);
        assert_eq!(r.contract(s, t).map(|c| c.price), Some(Money(400_000)));
        assert!(r
            .publish(s, Money(1_000_000), BidCriteria::default(), 14, t)
            .is_none());
        assert_eq!(due_this_month(&r, t).len(), 1);
        // Po wygaśnięciu umowy przedmiot wraca na rynek.
        let potem = Tick(t.0 + 25 * 43_200);
        assert!(r.contract(s, potem).is_none());
        assert!(r
            .publish(s, Money(1_000_000), BidCriteria::default(), 14, potem)
            .is_some());
    }
}
