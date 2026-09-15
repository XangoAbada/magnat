//! CPI i inflacja emergentna (M5d §5.10, PRD §6.5).
//!
//! Indeks liczy się **z cen transakcyjnych ważonych wolumenem**, nie ze średniej
//! ofert: oferta, której nikt nie kupuje, nie jest ceną (§6.1). Metoda to Laspeyres
//! z zamrożonym koszykiem `q_0` z `data/economy/cpi.ron`:
//!
//! ```text
//! CPI_t = 10 000 · Σ p_t·q_0 / Σ p_0·q_0
//! ```
//!
//! gdzie `p_t` to średnia ważona wolumenem z ostatnich 30 dób, a `p_0` — ta sama
//! średnia z pierwszego pełnego okna świata.
//!
//! **Dlaczego agregat, a nie odczyt z dziennika.** `TxJournal` jest pierścieniem
//! o 4096 wpisach i starcza na dobę ruchu jednego miasta; okno CPI ma 30 dób.
//! Licznik narastający per `GoodId` per doba jest więc jedyną drogą — i to jest
//! różnica między „mam dane" a „mam je wtedy, kiedy ich potrzebuję" (korekta
//! wpisana do M5d po M5b).
//!
//! **W tym module nie ma zmiennej „inflacja" jako wejścia.** Jest wyłącznie wynik:
//! `yoy_bp()` liczy się z dwóch odczytów indeksu, a bank centralny go czyta.
//! Pilnuje tego test statyczny `inflacja_nie_jest_zmienna_stanu`.

use magnat_core::{GoodId, HashState, Money, Qty, StateHasher, Tick};

use crate::credit::{update_base_rate, BaseRate};
use crate::data::EconomyData;
use crate::supply::{GoodTable, PRICE_UNIT};

/// Indeks w punktach bazowych: 10 000 = baza.
pub type IndexBp = i32;

/// Baza indeksu — wartość, od której zaczyna każdy świat.
pub const INDEX_BASE: IndexBp = 10_000;

/// Ile miesięcy historii indeksu trzymamy. Dwanaście wystarcza do inflacji r/r,
/// dwadzieścia cztery dają zapas na okno historii kredytowej z tego samego pliku.
const MONTHS_KEPT: usize = 24;

/// Obrót jednym towarem w jednej dobie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct DaySales {
    amount: Money,
    qty: Qty,
}

/// Koszyk miejski rozwiązany na identyfikatory towarów.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CpiBasket {
    pub items: Vec<(GoodId, Qty)>,
}

impl CpiBasket {
    /// Rozwiązuje klucze z `cpi.ron` na `GoodId` przez katalog detaliczny.
    ///
    /// Towar, którego katalog nie wyprodukował (bo nie ma go w `data/goods/`),
    /// **wypada z koszyka** — inaczej jego zerowy obrót ciągnąłby indeks w dół
    /// przez całe życie świata. Walidację „towar spoza `retail.ron`" robi loader
    /// danych, więc tutaj zostaje wyłącznie ta jedna, węższa sytuacja.
    #[must_use]
    pub fn resolve(d: &EconomyData, goods: &GoodTable) -> CpiBasket {
        let mut items = Vec::with_capacity(d.cpi.items.len());
        for (key, qty) in &d.cpi.items {
            if let Some(good) = goods.id_of_key(key) {
                items.push((good, *qty));
            }
        }
        CpiBasket { items }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Licznik CPI: pierścień dobowy, historia miesięczna i stopa bazowa.
///
/// Wchodzi do hasha stanu (00 §3.6) przez `Market`, bo stopa bazowa wpływa na
/// oprocentowanie kredytów, a te na pieniądz — pomiar, który zmienia świat, jest
/// stanem, a nie pomiarem.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CpiTracker {
    basket: CpiBasket,
    window_days: u16,
    /// `[doba][pozycja koszyka]` — pierścień o `window_days` dobach.
    ring: Vec<DaySales>,
    day_cursor: u16,
    days_filled: u16,
    /// `p_0` per pozycja koszyka; ustalane raz, przy pierwszym pełnym oknie.
    base: Vec<Money>,
    base_set: bool,
    index_bp: IndexBp,
    months: Vec<IndexBp>,
    months_filled: u32,
    rate: BaseRate,
}

impl CpiTracker {
    #[must_use]
    pub fn new(d: &EconomyData, goods: &GoodTable) -> CpiTracker {
        let basket = CpiBasket::resolve(d, goods);
        let n = basket.items.len();
        let window = d.cpi.window_days.max(1);
        CpiTracker {
            basket,
            window_days: window,
            ring: vec![DaySales::default(); n * window as usize],
            day_cursor: 0,
            days_filled: 0,
            base: vec![Money::ZERO; n],
            base_set: false,
            index_bp: INDEX_BASE,
            months: Vec::new(),
            months_filled: 0,
            rate: BaseRate::start(&d.bank),
        }
    }

    #[must_use]
    pub fn basket(&self) -> &CpiBasket {
        &self.basket
    }

    /// Bieżący indeks. `10 000` do czasu ustalenia bazy.
    #[must_use]
    pub fn index_bp(&self) -> IndexBp {
        self.index_bp
    }

    #[must_use]
    pub fn base_rate(&self) -> BaseRate {
        self.rate
    }

    /// Czy baza indeksu została już ustalona (minęło pierwsze pełne okno).
    #[must_use]
    pub fn has_base(&self) -> bool {
        self.base_set
    }

    /// Zmiana indeksu rok do roku w punktach bazowych.
    ///
    /// `None`, dopóki nie ma dwunastu miesięcy historii — i to jest różnica, która
    /// ma znaczenie: zero czytałoby się jak „inflacja 0 %", czyli **poniżej celu**,
    /// i bank centralny zacząłby obniżać stopę, zanim cokolwiek zmierzył.
    #[must_use]
    pub fn yoy_bp(&self) -> Option<i32> {
        if self.months.len() < 13 {
            return None;
        }
        let teraz = i64::from(self.months[self.months.len() - 1]);
        let rok_temu = i64::from(self.months[self.months.len() - 13]);
        if rok_temu <= 0 {
            return None;
        }
        Some(
            i32::try_from((teraz - rok_temu) * i64::from(INDEX_BASE) / rok_temu)
                .unwrap_or(i32::MAX),
        )
    }

    /// Zmiana indeksu miesiąc do miesiąca w punktach bazowych; `None` przed drugim
    /// domknięciem miesiąca. Czyta to balansator (bramka G2, brak hiperinflacji).
    #[must_use]
    pub fn mom_bp(&self) -> Option<i32> {
        if self.months.len() < 2 {
            return None;
        }
        let teraz = i64::from(self.months[self.months.len() - 1]);
        let poprzedni = i64::from(self.months[self.months.len() - 2]);
        if poprzedni <= 0 {
            return None;
        }
        Some(
            i32::try_from((teraz - poprzedni) * i64::from(INDEX_BASE) / poprzedni)
                .unwrap_or(i32::MAX),
        )
    }

    /// Dopisuje transakcję detaliczną do bieżącej doby.
    ///
    /// Woła to `record_sale`, czyli **rozliczenie**, a nie wystawienie oferty:
    /// do koszyka wchodzi to, co ktoś zapłacił.
    pub fn record(&mut self, good: GoodId, amount: Money, qty: Qty) {
        let Some(i) = self.basket.items.iter().position(|(g, _)| *g == good) else {
            return;
        };
        let slot = &mut self.ring[self.day_cursor as usize * self.basket.items.len() + i];
        slot.amount = Money(slot.amount.get().saturating_add(amount.get()));
        slot.qty = Qty(slot.qty.get().saturating_add(qty.get()));
    }

    /// Zamyka dobę: przelicza indeks z okna i przesuwa kursor pierścienia.
    pub fn roll_day(&mut self) {
        self.days_filled = (self.days_filled + 1).min(self.window_days);
        self.recompute();
        self.day_cursor = (self.day_cursor + 1) % self.window_days;
        let n = self.basket.items.len();
        let od = self.day_cursor as usize * n;
        for s in &mut self.ring[od..od + n] {
            *s = DaySales::default();
        }
    }

    /// Zamyka miesiąc: dopisuje indeks do historii i przestawia stopę bazową.
    pub fn close_month(&mut self, p: &crate::data::BankParams, t: Tick) -> BaseRate {
        if self.base_set {
            self.months.push(self.index_bp);
            if self.months.len() > MONTHS_KEPT {
                self.months.remove(0);
            }
            self.months_filled += 1;
        }
        // Bez roku historii bank centralny **nie rusza stopy**: reguła Taylora
        // potrzebuje odchylenia od celu, a nie da się go policzyć z niczego.
        if let Some(yoy) = self.yoy_bp() {
            self.rate = update_base_rate(self.rate, yoy, p, t);
            self.rate.set_at = t;
        }
        self.rate
    }

    /// Średnia cena transakcyjna pozycji koszyka w oknie, za `PRICE_UNIT` jednostek.
    /// `None`, jeśli w oknie nie było ani jednej transakcji tym towarem.
    #[must_use]
    fn window_price(&self, i: usize) -> Option<Money> {
        let n = self.basket.items.len();
        let mut kwota: i128 = 0;
        let mut ilosc: i128 = 0;
        for d in 0..self.window_days as usize {
            let s = self.ring[d * n + i];
            kwota += i128::from(s.amount.get());
            ilosc += i128::from(s.qty.get());
        }
        if ilosc <= 0 {
            return None;
        }
        i64::try_from(kwota * i128::from(PRICE_UNIT) / ilosc)
            .ok()
            .map(Money)
    }

    /// Przelicza indeks z bieżącego okna.
    ///
    /// Pozycja bez transakcji w oknie **wypada z licznika i z mianownika naraz** —
    /// dzięki temu chwilowy brak towaru w mieście nie wygląda jak spadek cen.
    fn recompute(&mut self) {
        if self.basket.is_empty() {
            return;
        }
        if !self.base_set {
            if self.days_filled < self.window_days {
                return;
            }
            // Pierwsze pełne okno zamraża `p_0`. Towar, którym w tym oknie nikt
            // nie handlował, dostaje bazę zerową i dołączy do indeksu dopiero,
            // kiedy pojawi się w obu odczytach.
            for i in 0..self.basket.items.len() {
                self.base[i] = self.window_price(i).unwrap_or(Money::ZERO);
            }
            self.base_set = true;
            self.index_bp = INDEX_BASE;
            return;
        }
        let mut licznik: i128 = 0;
        let mut mianownik: i128 = 0;
        for (i, (_, q0)) in self.basket.items.iter().enumerate() {
            let p0 = self.base[i];
            if p0.get() <= 0 {
                continue;
            }
            let Some(pt) = self.window_price(i) else {
                continue;
            };
            let q = i128::from(q0.get());
            licznik += i128::from(pt.get()) * q;
            mianownik += i128::from(p0.get()) * q;
        }
        if mianownik <= 0 {
            return;
        }
        self.index_bp =
            i32::try_from(licznik * i128::from(INDEX_BASE) / mianownik).unwrap_or(i32::MAX);
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.basket.items.len() as u64);
        for (g, q) in &self.basket.items {
            h.write_u16(g.get());
            q.hash_state(h);
        }
        for s in &self.ring {
            s.amount.hash_state(h);
            s.qty.hash_state(h);
        }
        h.write_u16(self.day_cursor);
        h.write_u16(self.days_filled);
        for m in &self.base {
            m.hash_state(h);
        }
        h.write_u8(u8::from(self.base_set));
        h.write_u32(self.index_bp as u32);
        h.write_u64(self.months.len() as u64);
        for m in &self.months {
            h.write_u32(*m as u32);
        }
        h.write_u32(self.months_filled);
        self.rate.hash_state(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Katalog detaliczny na potrzeby testu — ta sama droga co w `Market`.
    fn swiat() -> (EconomyData, GoodTable) {
        let d = EconomyData::load_default().expect("data/economy/");
        let goods = GoodTable::build(&d.retail, |key| {
            d.retail
                .goods
                .iter()
                .position(|g| g.key == key)
                .map(|i| (GoodId(i as u16), Money(20_000), Qty(200)))
        });
        (d, goods)
    }

    #[test]
    fn koszyk_rozwiazuje_sie_na_towary_katalogu() {
        let (d, goods) = swiat();
        let b = CpiBasket::resolve(&d, &goods);
        assert_eq!(b.items.len(), d.cpi.items.len());
    }

    #[test]
    fn indeks_stoi_na_bazie_dopoki_okno_sie_nie_zapelni() {
        let (d, goods) = swiat();
        let mut c = CpiTracker::new(&d, &goods);
        for _ in 0..d.cpi.window_days - 1 {
            c.record(GoodId(0), Money(100_000), Qty(1_000));
            c.roll_day();
        }
        assert!(!c.has_base());
        assert_eq!(c.index_bp(), INDEX_BASE);
    }

    #[test]
    fn podwojenie_cen_transakcyjnych_podwaja_indeks() {
        let (d, goods) = swiat();
        let mut c = CpiTracker::new(&d, &goods);
        let towary: Vec<GoodId> = c.basket().items.iter().map(|(g, _)| *g).collect();
        for _ in 0..d.cpi.window_days {
            for g in &towary {
                c.record(*g, Money(100_000), Qty(1_000));
            }
            c.roll_day();
        }
        assert!(c.has_base());
        assert_eq!(c.index_bp(), INDEX_BASE);
        for _ in 0..d.cpi.window_days {
            for g in &towary {
                c.record(*g, Money(200_000), Qty(1_000));
            }
            c.roll_day();
        }
        assert_eq!(c.index_bp(), 2 * INDEX_BASE);
    }

    #[test]
    fn oferta_ktorej_nikt_nie_kupuje_nie_jest_cena() {
        // Towar bez ani jednej transakcji w oknie wypada z licznika i z mianownika,
        // więc brak towaru w mieście nie wygląda jak spadek cen (§6.1).
        let (d, goods) = swiat();
        let mut c = CpiTracker::new(&d, &goods);
        let towary: Vec<GoodId> = c.basket().items.iter().map(|(g, _)| *g).collect();
        for _ in 0..d.cpi.window_days {
            for g in &towary {
                c.record(*g, Money(100_000), Qty(1_000));
            }
            c.roll_day();
        }
        // Drugie okno: połowa asortymentu znika ze sklepów, reszta drożeje o 10 %.
        for _ in 0..d.cpi.window_days {
            for g in towary.iter().take(towary.len() / 2) {
                c.record(*g, Money(110_000), Qty(1_000));
            }
            c.roll_day();
        }
        assert_eq!(c.index_bp(), 11_000);
    }

    #[test]
    fn stopa_bazowa_reaguje_dopiero_po_roku_historii() {
        let (d, goods) = swiat();
        let mut c = CpiTracker::new(&d, &goods);
        let start = c.base_rate().bp;
        for m in 0..6 {
            assert_eq!(c.close_month(&d.bank, Tick(m)).bp, start, "miesiąc {m}");
        }
        assert_eq!(c.yoy_bp(), None);
    }
}
