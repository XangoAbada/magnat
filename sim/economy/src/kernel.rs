//! Rdzeń liczbowy `sim/economy` (D20 z §6 dokumentu fazy).
//!
//! **Zasada: nie zna LOD, nie zna encji, nie alokuje.** Dostaje liczby, zwraca liczby.
//! Brak `&World`, brak `Entity`, brak I/O, brak stanu globalnego. Identyfikatory
//! wchodzą wyłącznie jako indeksy katalogu danych, nigdy jako id rozwiązywane
//! przez świat.
//!
//! Powód nie jest estetyczny. Model makro M10 ma wołać **ten sam kod** co mezo,
//! inaczej `K-5` (odchylenie agregatów ≤ 0,5 %, brak dryfu) staje się nie do
//! utrzymania: dryf pochodziłby z rozjazdu dwóch implementacji, a nie z agregacji,
//! czyli mierzylibyśmy własny błąd zamiast błędu przybliżenia.
//!
//! Drugi zysk jest dzisiejszy: własności P4–P7 dają się uruchomić `proptest`-em
//! **bez budowania świata**, a zakaz floatów w pieniądzu (00 §2) przestaje być
//! sprawą przeglądu kodu — cały moduł jest całkowitoliczbowy i pilnuje tego
//! test statyczny (P9).
//!
//! Sloty na przyszłość: `wage_bid` wypełnia M7, `throughput` M6. Nie ma ich tutaj
//! jako zaślepek, bo zaślepka z jedną implementacją to koszt bez konsumenta —
//! są w kontrakcie §6 dokumentu fazy i wchodzą razem ze swoim wołającym.

use magnat_core::{Money, Qty};

use crate::ledger::{LedgerAccount, LEDGER_ACCOUNT_COUNT};

/// Jeden punkt bazowy to 1/10 000, czyli 0,01 %. Cała arytmetyka cenowa liczy w tej
/// jednostce, bo jest całkowitoliczbowa i wystarczająco drobna: 1 bp na cenie 200 zł
/// to 2 grosze.
pub const BP: i64 = 10_000;

// ── cena (§5.6) ──────────────────────────────────────────────────────────────────

/// Wejście do złożenia ceny. Wszystko w **podstawie netto** (`K-7`) — przeliczenie
/// na brutto robi `TaxEngine` u wołającego, jako ostatni krok.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PriceInput {
    /// Koszt własny za [`crate::supply::PRICE_UNIT`] jednostek, netto.
    pub unit_cost: Money,
    pub target_margin_bp: i32,
    pub min_margin_bp: i32,
    pub max_margin_bp: i32,
    /// Zapas wyrażony w punktach bazowych celu: 10 000 = dokładnie cel,
    /// 20 000 = dwa razy tyle, 0 = pusto.
    pub stock_bp_of_target: i32,
    /// Czułość na zapas — z osobowości cenowej firmy.
    pub k_stock: i32,
    /// Obserwowana cena odniesienia konkurencji, netto. `None` = nikogo nie widać.
    pub competitor_net: Option<Money>,
    pub k_comp: i32,
    /// Korekta z eksperymentu albo ze zmierzonej elastyczności, bp.
    pub adj_elast_bp: i32,
    /// Przecena psującego się towaru, bp (ujemna albo zero).
    pub adj_spoil_bp: i32,
}

/// Rozbicie złożonej ceny — to z niego bierze się `DecisionReason::Repricing`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PriceBreakdown {
    pub base: Money,
    pub adj_stock_bp: i32,
    pub adj_comp_bp: i32,
    pub adj_elast_bp: i32,
    pub adj_spoil_bp: i32,
    pub floor: Money,
    pub ceil: Money,
    pub price: Money,
    /// `true`, gdy złożona cena oparła się o dolny ogranicznik marży.
    pub at_floor: bool,
    pub at_ceil: bool,
}

/// Największa korekta co do modułu — ta, którą karta inspekcji ma nazwać.
impl PriceBreakdown {
    #[must_use]
    pub fn dominant_adj_bp(&self) -> (i32, u8) {
        let adjs = [
            (self.adj_stock_bp, 0u8),
            (self.adj_comp_bp, 1),
            (self.adj_elast_bp, 2),
            (self.adj_spoil_bp, 3),
        ];
        let mut best = (0i32, u8::MAX);
        for (v, k) in adjs {
            if v.abs() > best.0.abs() {
                best = (v, k);
            }
        }
        best
    }
}

/// Składanie ceny w arytmetyce punktów bazowych (§5.6). Zero floatów — to jest
/// warunek P9 i powód, dla którego ta funkcja jest tutaj, a nie w `pricing`.
///
/// Dolny ogranicznik jest **krytyczny**: bez niego sklep z nadmiarem zapasu wpada
/// w spiralę deflacyjną poniżej kosztu własnego. Dokładnie tego pilnuje bramka G3
/// balansatora, a meta-test fazy sprawdza, że usunięcie tej linii ją zaczerwienia.
#[must_use]
pub fn next_price_full(i: PriceInput) -> PriceBreakdown {
    let cost = i.unit_cost.get().max(0);
    let base = mul_bp(cost, BP + i64::from(i.target_margin_bp));

    // Nadmiar zapasu ciągnie cenę w dół, brak — w górę. Znak jest odwrotny do
    // odchylenia zapasu, stąd `10 000 − stock_bp`, a nie `stock_bp − 10 000`
    // (korekta W-1 wobec §5.6: wzór w planie dawał podwyżkę przy zaleganiu).
    let odchylenie = BP - i64::from(i.stock_bp_of_target);
    let adj_stock = clamp_i32(i64::from(i.k_stock) * odchylenie / BP, -2_000, 1_500);

    let adj_comp = match i.competitor_net {
        Some(c) if base > 0 && c.get() > 0 => {
            let stosunek = c.get().saturating_mul(BP) / base - BP;
            clamp_i32(i64::from(i.k_comp) * stosunek / BP, -1_500, 1_500)
        }
        _ => 0,
    };
    let adj_elast = i.adj_elast_bp.clamp(-500, 500);
    let adj_spoil = i.adj_spoil_bp.clamp(-6_000, 0);

    let suma = i64::from(adj_stock) + i64::from(adj_comp) + i64::from(adj_elast) + i64::from(adj_spoil);
    let surowa = mul_bp(base, BP + suma);

    // Podłoga chroni przed **spiralą deflacyjną**, nie przed wyprzedażą towaru,
    // który i tak pójdzie do odpisu: alternatywą dla sprzedaży za pół ceny jest
    // strata 100 % (§5.8, `WriteOffExpense`). Dlatego przecena psującego się obniża
    // także dolny ogranicznik — bez tego `adj_spoil` byłby martwy przy każdej
    // sensownej marży minimalnej i kryterium „przecena psującego się" spełniałoby
    // się tożsamościowo. Dla towaru, który się nie psuje, `adj_spoil == 0`
    // i podłoga jest dokładnie tam, gdzie pilnuje jej bramka G3.
    let floor = mul_bp(cost, BP + i64::from(i.min_margin_bp) + i64::from(adj_spoil));
    let ceil = mul_bp(cost, BP + i64::from(i.max_margin_bp)).max(floor);
    let price = surowa.clamp(floor, ceil);

    PriceBreakdown {
        base: Money(base),
        adj_stock_bp: adj_stock,
        adj_comp_bp: adj_comp,
        adj_elast_bp: adj_elast,
        adj_spoil_bp: adj_spoil,
        floor: Money(floor),
        ceil: Money(ceil),
        price: Money(price),
        at_floor: surowa < floor,
        at_ceil: surowa > ceil,
    }
}

/// Sama kwota — sygnatura z kontraktu §6 dokumentu fazy.
#[must_use]
pub fn next_price(i: PriceInput) -> Money {
    next_price_full(i).price
}

/// Cena wprost z polityki (ręczna, dopasowanie do konkurenta) przycięta do widełek
/// marży. Ogranicznik obowiązuje **każdą** ścieżkę, nie tylko `Dynamic` — inaczej
/// polityka „−2 % od najtańszego" schodziłaby poniżej kosztu razem z konkurentem.
#[must_use]
pub fn clamp_to_margin(price: Money, unit_cost: Money, min_bp: i32, max_bp: i32) -> (Money, bool, bool) {
    let cost = unit_cost.get().max(0);
    let floor = mul_bp(cost, BP + i64::from(min_bp));
    let ceil = mul_bp(cost, BP + i64::from(max_bp)).max(floor);
    let p = price.get().clamp(floor, ceil);
    (Money(p), price.get() < floor, price.get() > ceil)
}

/// `v · bp / 10 000` w `i128`, z zaokrągleniem połówek od zera.
fn mul_bp(v: i64, bp: i64) -> i64 {
    Money(v).mul_ratio(bp, BP).get()
}

fn clamp_i32(v: i64, lo: i32, hi: i32) -> i32 {
    v.clamp(i64::from(lo), i64::from(hi)) as i32
}

// ── wycena zapasu (§5.8) ─────────────────────────────────────────────────────────

/// Para (ilość, koszt nabycia) — wycena średnią ważoną bez partii. M6 zastąpi
/// to FIFO per `BatchId`, wariantem (a) z decyzji otwartej nr 5: przełącznik od
/// daty wejścia M6, linie historyczne dojeżdżają na średniej.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StockValue {
    pub qty: Qty,
    pub cost_total: Money,
}

/// Koszt własny sprzedaży przy zdjęciu `sold` z linii.
///
/// Gałąź **zmiatania reszty** (ryzyko R5): kiedy ilość schodzi do zera, wychodzi
/// **cały** pozostały koszt, a nie wynik proporcji. Bez niej po tysiącach transakcji
/// zostaje osad groszy, którego nikt nie zobaczy, dopóki bilans nie przestanie się
/// zamykać (P5). To nie jest optymalizacja, to warunek poprawności.
///
/// Funkcja przyszła tu z `shop::take_units` bez zmiany zachowania — złoty plik ciągu
/// hashy przed przenosinami i po nich jest identyczny (wymóg D20).
#[must_use]
pub fn take_cogs(line: StockValue, sold: Qty) -> (Money, StockValue) {
    debug_assert!(sold.get() >= 0, "ilość zdejmowana musi być nieujemna");
    let have = line.qty.get();
    let take = sold.get().min(have);
    if take <= 0 {
        return (Money::ZERO, line);
    }
    if take == have {
        return (line.cost_total, StockValue::default());
    }
    let cost = line.cost_total.mul_ratio(take, have);
    let reszta = StockValue {
        qty: Qty(have - take),
        cost_total: line
            .cost_total
            .checked_sub(cost)
            .expect("zapas: koszt nie może zejść poniżej zera"),
    };
    (cost, reszta)
}

// ── zapis księgowy (§5.8) ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LedgerError {
    /// `Σ lines != 0` — zapis niezbilansowany, tolerancja 0 groszy (P4).
    Unbalanced(Money),
    /// Zapis bez linii albo z pojedynczą linią zerową.
    Empty,
    Overflow,
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LedgerError::Unbalanced(d) => write!(f, "zapis niezbilansowany o {} gr", d.get()),
            LedgerError::Empty => write!(f, "zapis bez linii"),
            LedgerError::Overflow => write!(f, "przepełnienie salda konta"),
        }
    }
}

impl std::error::Error for LedgerError {}

/// Nanosi zbilansowany zapis na tablicę sald. **Odrzuca niezbilansowane** — to jest
/// jedyne miejsce, w którym niezmiennik P4 jest egzekwowany, i dlatego nie ma drugiej
/// drogi do sald.
///
/// Konwencja znaku: **dodatnia kwota to Wn (debet), ujemna to Ma (kredyt)**, dla
/// każdego konta tak samo. Konto pasywne z saldem Ma ma więc saldo ujemne, a bilans
/// zamyka się wtedy jako `Σ wszystkich sald == 0`. Alternatywa (znak zależny od typu
/// konta) wymagałaby tablicy stron i dawała drugie miejsce, w którym da się pomylić
/// kierunek.
///
/// Pisze do bufora wołającego, więc nie alokuje mimo zmiennej liczby linii.
pub fn ledger_post(
    lines: &[(LedgerAccount, Money)],
    acc: &mut [Money; LEDGER_ACCOUNT_COUNT],
) -> Result<(), LedgerError> {
    if lines.is_empty() {
        return Err(LedgerError::Empty);
    }
    let mut suma: i64 = 0;
    for (_, m) in lines {
        suma = suma.checked_add(m.get()).ok_or(LedgerError::Overflow)?;
    }
    if suma != 0 {
        return Err(LedgerError::Unbalanced(Money(suma)));
    }
    // Sprawdzenie przed mutacją: zapis albo wchodzi w całości, albo nie zostawia śladu.
    for (a, m) in lines {
        acc[a.as_index()]
            .checked_add(*m)
            .ok_or(LedgerError::Overflow)?;
    }
    for (a, m) in lines {
        let i = a.as_index();
        acc[i] = acc[i].checked_add(*m).expect("sprawdzone wyżej");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wejscie(koszt: i64) -> PriceInput {
        PriceInput {
            unit_cost: Money(koszt),
            target_margin_bp: 3_000,
            min_margin_bp: 500,
            max_margin_bp: 12_000,
            stock_bp_of_target: 10_000,
            k_stock: 4_000,
            competitor_net: None,
            k_comp: 5_000,
            adj_elast_bp: 0,
            adj_spoil_bp: 0,
        }
    }

    #[test]
    fn zapas_w_celu_daje_czysta_marze() {
        let p = next_price(wejscie(200));
        assert_eq!(p, Money(260), "200 gr + 30 % = 260 gr");
    }

    #[test]
    fn nadmiar_obniza_a_brak_podnosi() {
        let mut nadmiar = wejscie(200);
        nadmiar.stock_bp_of_target = 20_000;
        let mut brak = wejscie(200);
        brak.stock_bp_of_target = 2_000;
        let tanio = next_price(nadmiar);
        let drogo = next_price(brak);
        assert!(tanio < Money(260), "zalegający towar ma tanieć: {tanio:?}");
        assert!(drogo > Money(260), "braki mają podnosić cenę: {drogo:?}");
    }

    #[test]
    fn dolny_ogranicznik_nie_pozwala_zejsc_pod_koszt() {
        // Wszystkie korekty na maksa w dół: −20 % zapas, −15 % konkurencja,
        // −5 % eksperyment. Bez podłogi cena zeszłaby pod koszt — to jest ryzyko R1
        // i bramka G3.
        let mut i = wejscie(200);
        i.stock_bp_of_target = 100_000;
        i.competitor_net = Some(Money(1));
        i.adj_elast_bp = -5_000;
        let b = next_price_full(i);
        assert!(b.at_floor);
        assert_eq!(b.price, Money(210), "podłoga = koszt + min_margin 5 %");
        assert!(b.price.get() > i.unit_cost.get());
    }

    #[test]
    fn przecena_psujacego_sie_obniza_takze_podloge() {
        // Inaczej `adj_spoil` byłby martwy: −60 % od ceny z marżą 30 % to 104 gr,
        // a podłoga przy marży minimalnej 5 % stoi na 210 gr.
        let mut i = wejscie(200);
        i.adj_spoil_bp = -6_000;
        let b = next_price_full(i);
        assert!(!b.at_floor, "wyprzedaż ma nie opierać się o podłogę: {b:?}");
        assert!(
            b.price.get() < i.unit_cost.get(),
            "towar w dniu ważności schodzi poniżej kosztu: {:?}",
            b.price
        );
    }

    #[test]
    fn zdjecie_calej_linii_zmiata_reszte_groszy() {
        // R5 — ta sama własność, którą miał `shop::take_units`, teraz w rdzeniu.
        let mut l = StockValue {
            qty: Qty(1_000),
            cost_total: Money(333),
        };
        let mut suma = 0i64;
        for _ in 0..1_000 {
            let (c, reszta) = take_cogs(l, Qty(1));
            suma += c.get();
            l = reszta;
        }
        assert_eq!(l, StockValue::default());
        assert_eq!(suma, 333);
    }

    #[test]
    fn niezbilansowany_zapis_nie_rusza_zadnego_salda() {
        let mut acc = [Money::ZERO; LEDGER_ACCOUNT_COUNT];
        let e = ledger_post(
            &[
                (LedgerAccount::Cash, Money(100)),
                (LedgerAccount::Revenue, Money(-99)),
            ],
            &mut acc,
        );
        assert_eq!(e, Err(LedgerError::Unbalanced(Money(1))));
        assert!(acc.iter().all(|m| *m == Money::ZERO));

        ledger_post(
            &[
                (LedgerAccount::Cash, Money(100)),
                (LedgerAccount::Revenue, Money(-100)),
            ],
            &mut acc,
        )
        .unwrap();
        assert_eq!(acc[LedgerAccount::Cash.as_index()], Money(100));
        assert_eq!(acc[LedgerAccount::Revenue.as_index()], Money(-100));
        assert_eq!(acc.iter().map(|m| m.get()).sum::<i64>(), 0);
    }
}
