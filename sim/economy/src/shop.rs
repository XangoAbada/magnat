//! Sklep: zaplecze, półka, asortyment (M5b §5.3, PRD §7.1, §7.3).
//!
//! **Zaplecze i półka to dwa różne stany.** `Offer.available` odzwierciedla wyłącznie
//! półkę — towar w zapleczu nie jest na sprzedaż. Przy wyczerpaniu półki oferta
//! zostaje (z `available == 0`), bo sklep ma pozostać widoczny w inspekcji jako
//! „znany, ale bez towaru": to jest odpowiedź na pytanie z PRD §14.1, a nie
//! przeoczenie w sprzątaniu ofert.

use std::collections::BTreeMap;

use magnat_core::{
    FirmId, GoodId, HashState, Money, Qty, RejectCause, SimMinute, SiteId, StateHasher, Tick,
    REJECT_CAUSE_COUNT,
};

/// Linia zapasu. `expires` to **jedna data na linię** — M5 nie ma partii, więc
/// dostawa dokłada się do istniejącej linii i przesuwa datę na wcześniejszą z dwóch
/// (ostrożniej, nie „średnio"). M6 zastępuje to `BatchId` i wyceną FIFO.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StockLine {
    /// NIGDY < 0 — niezmiennik P3, testowany.
    pub qty: Qty,
    /// Łączny koszt nabycia tej linii → wycena średnią ważoną.
    pub cost_total: Money,
    pub expires: Option<SimMinute>,
}

impl StockLine {
    /// Dokłada dostawę: ilości się sumują, koszty się sumują, data ważności
    /// bierze **wcześniejszą** z dwóch.
    pub fn receive(&mut self, qty: Qty, cost: Money, expires: Option<SimMinute>) {
        self.qty = Qty(self.qty.get().saturating_add(qty.get()));
        self.cost_total = self
            .cost_total
            .checked_add(cost)
            .expect("StockLine: przepełnienie kosztu linii zapasu");
        self.expires = match (self.expires, expires) {
            (Some(a), Some(b)) => Some(SimMinute(a.get().min(b.get()))),
            (a, b) => a.or(b),
        };
    }

    /// Zdejmuje `take` sztuk i oddaje przypadający na nie koszt.
    pub fn take(&mut self, take: Qty) -> Money {
        take_units(&mut self.qty, &mut self.cost_total, take)
    }
}

/// Zdejmuje ilość z pary (ilość, koszt) i oddaje koszt przypadający na zdjęte.
///
/// Gałąź **zmiatania reszty** (ryzyko R5 z dokumentu fazy): kiedy ilość schodzi
/// do zera, wychodzi **cały** pozostały koszt, a nie wynik proporcji. Bez tego
/// po tysiącach transakcji zostaje osad groszy, którego nikt nie zobaczy, dopóki
/// bilans nie przestanie się zamykać.
///
/// Jedna funkcja dla zaplecza i dla półki, bo to jest jedna reguła domenowa
/// (DRY dotyczy wiedzy): wycena średnią ważoną schodzi tak samo po obu stronach.
/// M5c/WP7 przenosi ją do `kernel` jako `take_cogs`, razem z księgowaniem.
pub fn take_units(qty: &mut Qty, cost_total: &mut Money, take: Qty) -> Money {
    debug_assert!(take.get() >= 0, "ilość zdejmowana musi być nieujemna");
    let have = qty.get();
    let take = take.get().min(have);
    if take <= 0 {
        return Money::ZERO;
    }
    if take == have {
        let all = *cost_total;
        *qty = Qty::ZERO;
        *cost_total = Money::ZERO;
        return all;
    }
    let cost = cost_total.mul_ratio(take, have);
    *qty = Qty(have - take);
    *cost_total = cost_total
        .checked_sub(cost)
        .expect("zapas: koszt nie może zejść poniżej zera");
    cost
}

/// Polityka zapasu: poniżej `point` zamawiamy do `target`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReorderPolicy {
    pub point: Qty,
    pub target: Qty,
    pub lead_time_days: u8,
}

/// Kto decyduje o asortymencie.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AssortmentPolicy {
    /// Gracz wybiera towary z listy.
    Manual { goods: Vec<GoodId> },
    /// AI: tyle linii, ile mieści półka, w kolejności rangi substytutu kategorii.
    ///
    /// Plan §5.3 mówi „top-N wg marża × popyt w dzielnicy". `ponytail:` sufit nazwany:
    /// popytu w dzielnicy w M5b jeszcze nie ma — mierzy go dopiero balansator (M5e),
    /// a marża jest dziś stałą z danych, więc ranking po marży × popyt byłby
    /// rankingiem po jednej znanej liczbie. Ścieżka wyjścia: ranking wchodzi razem
    /// z `ObservedElasticity` w M5c, kiedy będzie z czego go policzyć.
    Auto { max_lines: u16, min_margin_bp: i32 },
}

/// Linia na półce. `offer` jest uchwytem do areny ofert — cena czyta się z areny
/// na żywo, więc przecena nie rusza ani półki, ani indeksu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ShelfLine {
    pub good: GoodId,
    pub qty: Qty,
    /// Koszt nabycia towaru leżącego na półce — druga połowa wyceny zapasu.
    pub cost_total: Money,
    /// Ile miejsc na półce; limit ekspozycji, czyli sufit `qty`.
    pub facings: u16,
    pub offer: crate::offer::OfferId,
}

impl ShelfLine {
    /// Zdejmuje towar z półki razem z przypadającym na niego kosztem nabycia.
    pub fn take(&mut self, take: Qty) -> Money {
        take_units(&mut self.qty, &mut self.cost_total, take)
    }
}

/// Półka sklepu. `lines` jest **posortowane po `GoodId`** — to ona ustala kolejność
/// iteracji, więc żadna pętla sklepu nie zależy od kolejności wstawiania.
#[derive(Clone, Default)]
pub struct Shelf {
    pub slots: u16,
    pub lines: Vec<ShelfLine>,
}

impl Shelf {
    #[must_use]
    pub fn line(&self, good: GoodId) -> Option<&ShelfLine> {
        self.lines
            .binary_search_by_key(&good, |l| l.good)
            .ok()
            .map(|i| &self.lines[i])
    }

    pub fn line_mut(&mut self, good: GoodId) -> Option<&mut ShelfLine> {
        self.lines
            .binary_search_by_key(&good, |l| l.good)
            .ok()
            .map(|i| &mut self.lines[i])
    }

    /// Wstawia linię, zachowując porządek po `GoodId`. Zwraca `false`, gdy półka pełna.
    pub fn insert(&mut self, line: ShelfLine) -> bool {
        match self.lines.binary_search_by_key(&line.good, |l| l.good) {
            Ok(_) => false,
            Err(i) => {
                if self.lines.len() >= usize::from(self.slots) {
                    return false;
                }
                self.lines.insert(i, line);
                true
            }
        }
    }
}

/// Zaplecze sklepu (§7.3: pojemność wynika z budynku).
#[derive(Clone, Default)]
pub struct ShopInventory {
    pub backroom: BTreeMap<GoodId, StockLine>,
    pub capacity_m3: i64,
    pub reorder: BTreeMap<GoodId, ReorderPolicy>,
}

// ── utracone sprzedaże (kontrakt uzgodniony z M9) ────────────────────────────────

/// Trójstopniowe śledzenie: pełny bufor na wszystkich sklepach AI byłby kosztem
/// bez odbiorcy (§5.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LostSaleTracking {
    #[default]
    None,
    Histogram,
    Full,
}

/// Jedna utracona sprzedaż — 24 B, pierścień 256 wpisów na sklep.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LostSale {
    pub citizen: magnat_core::CitizenId,
    pub good: GoodId,
    pub when: Tick,
    pub cause: RejectCause,
    pub went_to: Option<SiteId>,
}

pub const LOST_SALE_RING: usize = 256;

/// Histogram doby: ile razy który powód, bez pamiętania kto.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LostSaleHistogram {
    pub day: u32,
    pub by_cause: [u32; REJECT_CAUSE_COUNT],
}

/// Zapis utraconych sprzedaży jednego sklepu.
#[derive(Clone, Default)]
pub struct ShopLostSales {
    pub today: LostSaleHistogram,
    ring: Vec<LostSale>,
    head: usize,
}

impl ShopLostSales {
    /// Sprawdzenie poziomu jest jednym odczytem bitu, więc rynek obsadzony wyłącznie
    /// przez AI nie płaci za ten mechanizm nic (§5.4).
    pub fn record(&mut self, level: LostSaleTracking, day: u32, sale: LostSale) {
        if level == LostSaleTracking::None {
            return;
        }
        if self.today.day != day {
            self.today = LostSaleHistogram {
                day,
                by_cause: [0; REJECT_CAUSE_COUNT],
            };
        }
        self.today.by_cause[sale.cause.as_index()] += 1;
        if level != LostSaleTracking::Full {
            return;
        }
        if self.ring.len() < LOST_SALE_RING {
            self.ring.push(sale);
        } else {
            self.ring[self.head] = sale;
            self.head = (self.head + 1) % LOST_SALE_RING;
        }
    }

    /// Od najstarszej do najnowszej.
    pub fn iter(&self) -> impl Iterator<Item = &LostSale> {
        self.ring[self.head..]
            .iter()
            .chain(self.ring[..self.head].iter())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

// ── sklep ────────────────────────────────────────────────────────────────────────

/// Zakład handlowy: budynek, magazyn, półka, oferta.
pub struct Shop {
    pub site: SiteId,
    pub firm: FirmId,
    /// Rachunek bieżący sklepu w [`crate::Books`].
    pub account: crate::books::AccountId,
    /// Pozycja w metrach — z budynku, w którym stoi zakład.
    pub pos: magnat_spatial::Vec2,
    pub kind: magnat_core::PlaceKind,
    pub hours: magnat_agents::OpenHours,
    pub inventory: ShopInventory,
    pub shelf: Shelf,
    pub assortment: AssortmentPolicy,
    pub tracking: LostSaleTracking,
    pub lost: ShopLostSales,
    /// Licznik sprzedanych sztuk od początku świata — wejście do metryk balansatora.
    pub sold_qty: i64,
    pub revenue: Money,
}

impl HashState for StockLine {
    fn hash_state(&self, h: &mut StateHasher) {
        self.qty.hash_state(h);
        self.cost_total.hash_state(h);
        match self.expires {
            Some(m) => {
                h.write_u8(1);
                m.hash_state(h);
            }
            None => h.write_u8(0),
        }
    }
}

impl HashState for Shop {
    fn hash_state(&self, h: &mut StateHasher) {
        self.site.entity().hash_state(h);
        self.firm.entity().hash_state(h);
        // Zaplecze: `BTreeMap` iteruje po kluczu, więc kolejność jest stanem,
        // a nie przypadkiem (00 §3.2).
        h.write_u32(self.inventory.backroom.len() as u32);
        for (g, l) in &self.inventory.backroom {
            g.hash_state(h);
            l.hash_state(h);
        }
        h.write_u32(self.shelf.lines.len() as u32);
        for l in &self.shelf.lines {
            l.good.hash_state(h);
            l.qty.hash_state(h);
            l.cost_total.hash_state(h);
            h.write_u16(l.facings);
        }
        h.write_i64(self.sold_qty);
        self.revenue.hash_state(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zdjecie_calej_linii_zmiata_reszte_groszy() {
        // R5: 1000 sztuk za 333 gr. Zdejmowanie po jednej sztuce zaokrągla w dół
        // 999 razy; ostatnie zdjęcie musi wynieść z linii cały osad, inaczej
        // `cost_total` zostaje dodatni przy zerowej ilości i bilans przestaje się
        // zamykać (P5).
        let mut l = StockLine {
            qty: Qty(1_000),
            cost_total: Money(333),
            expires: None,
        };
        let mut suma = 0i64;
        for _ in 0..1_000 {
            suma += l.take(Qty(1)).get();
        }
        assert_eq!(l.qty, Qty::ZERO);
        assert_eq!(l.cost_total, Money::ZERO);
        assert_eq!(suma, 333, "koszt nie może się zgubić ani rozmnożyć");
    }

    #[test]
    fn dostawa_bierze_wczesniejsza_date_waznosci() {
        let mut l = StockLine::default();
        l.receive(Qty(10), Money(100), Some(SimMinute(5_000)));
        l.receive(Qty(10), Money(120), Some(SimMinute(3_000)));
        assert_eq!(l.qty, Qty(20));
        assert_eq!(l.cost_total, Money(220));
        assert_eq!(l.expires, Some(SimMinute(3_000)));
    }

    #[test]
    fn polka_trzyma_porzadek_po_good_id_i_pilnuje_slotow() {
        let mut s = Shelf {
            slots: 2,
            lines: Vec::new(),
        };
        let l = |g: u16| ShelfLine {
            good: GoodId(g),
            qty: Qty::ZERO,
            cost_total: Money::ZERO,
            facings: 1,
            offer: crate::offer::OfferId::from_bits(1 << 32).unwrap(),
        };
        assert!(s.insert(l(7)));
        assert!(s.insert(l(3)));
        assert!(!s.insert(l(5)), "półka pełna");
        assert!(!s.insert(l(3)), "towar już jest");
        assert_eq!(
            s.lines.iter().map(|x| x.good.get()).collect::<Vec<_>>(),
            vec![3, 7]
        );
    }

    #[test]
    fn pierscien_utraconych_sprzedazy_nie_rosnie_w_nieskonczonosc() {
        let mut l = ShopLostSales::default();
        let sale = |i: u64| LostSale {
            citizen: magnat_core::CitizenId(magnat_core::Entity::new(
                i as u32,
                std::num::NonZeroU32::MIN,
            )),
            good: GoodId(1),
            when: Tick(i),
            cause: RejectCause::OutOfStock,
            went_to: None,
        };
        for i in 0..(LOST_SALE_RING as u64 + 10) {
            l.record(LostSaleTracking::Full, 0, sale(i));
        }
        assert_eq!(l.len(), LOST_SALE_RING);
        assert_eq!(
            l.today.by_cause[RejectCause::OutOfStock.as_index()],
            LOST_SALE_RING as u32 + 10
        );
        // Najstarszy w pierścieniu to ten, który nie został jeszcze nadpisany.
        assert_eq!(l.iter().next().unwrap().when, Tick(10));
        // Poziom `None` nie kosztuje nic i nic nie zapisuje.
        let mut cichy = ShopLostSales::default();
        cichy.record(LostSaleTracking::None, 0, sale(0));
        assert!(cichy.is_empty());
        assert_eq!(cichy.today.by_cause[RejectCause::OutOfStock.as_index()], 0);
    }
}
