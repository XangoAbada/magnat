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
//! # Jądro jest regułą, a nie jednym plikiem (M7e WP12b)
//!
//! Plan M7e wymieniał `wage_bid` i `throughput` jako funkcje **do przeniesienia tutaj**.
//! Przenieść ich nie można i nie jest to kwestia wygody: stawkę licytacji liczy
//! `sim/firms`, przerób `sim/supply`, a **oba te crate'y stoją w grafie zależności
//! przed tym** (`economy → firms`, `economy → supply`). Zależność w drugą stronę
//! zamknęłaby cykl, którego Cargo nie zbuduje.
//!
//! Rozstrzygnięcie: jądro jest **zbiorem funkcji czystych o jawnych wejściach, z jednym
//! adresem**, a nie jednym plikiem. Implementacja mieszka w crate'cie, który ma dane;
//! adres nadaje ten moduł reeksportem. Dla `sim/macro` (M10) różnicy nie ma — woła
//! `kernel::wage_bid` i `kernel::throughput` i dostaje dokładnie ten kod, który liczy
//! mezo. Rozjazd dwóch implementacji jest niemożliwy, bo implementacja jest jedna.
//!
//! Tutaj mieszkają te funkcje jądra, których dane należą do `sim/economy`: składanie
//! ceny, wycena zapasu, zapis księgowy i rata annuitetowa.

/// Kolejna stawka w ofercie pracy — funkcja czysta niedoboru, agresji i sufitu marży
/// (M7 §5.5, §5.15). Implementacja: `magnat_firms::labor_policy::next_bid`.
///
/// Reeksport, a nie kopia: reguła „ile firma dokłada" jest **decyzją firmy** (`D19`)
/// i mieszka tam, gdzie widełki stanowiska. Tutaj jest jej adres dla modelu makro.
pub use magnat_firms::next_bid as wage_bid;

/// Przerób szarży z nominału linii, dławienia wsadem i pokrycia etatowego
/// (M7 §7.4, §5.15). Implementacja: `magnat_supply::kernel::throughput`.
///
/// Reeksport z tego samego powodu co [`wage_bid`]: liczbę zna magazyn i linia,
/// a `sim/supply` jest crate'em liściastym i o gospodarce nie wie nic.
pub use magnat_supply::kernel::throughput;

/// Podział popytu między oferty w proporcji `exp(u/T)` (§6.4, §5.15).
/// Implementacja: `core::det_math::softmax` — ta sama, z której `choose_offer`
/// losuje wybór pojedynczego kupującego.
///
/// **To jest cała różnica mezo vs. makro** (M10a §5.1): mezo losuje z tego rozkładu
/// jeden sklep dla jednego agenta, makro stosuje go jako wagi i dzieli popyt komórki
/// proporcjonalnie. Wzór jest jeden i dlatego dwa poziomy nie mogą się rozjechać.
pub use magnat_core::det_math::softmax as softmax_shares;

use magnat_core::{Money, Qty, Q};

use crate::data::UtilityWeights;
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

/// Rozbicie złożonej ceny — to z niego bierze się `DecisionReason::Firm(FirmReason::Repricing)`.
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

    let suma =
        i64::from(adj_stock) + i64::from(adj_comp) + i64::from(adj_elast) + i64::from(adj_spoil);
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
pub fn clamp_to_margin(
    price: Money,
    unit_cost: Money,
    min_bp: i32,
    max_bp: i32,
) -> (Money, bool, bool) {
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

// ── kredyt (§5.10) ───────────────────────────────────────────────────────────────

/// Skala ułamka w rachunku annuitetowym. `i128` × 10⁹ mieści `(1+i)^360` dla stóp
/// do kilkudziesięciu procent rocznie z zapasem rzędu dziesięciu rzędów wielkości.
const ANNUITY_SCALE: i128 = 1_000_000_000;

/// Rata annuitetowa: stała kwota, która przez `months` miesięcy spłaca kapitał
/// i odsetki naliczane od malejącego salda.
///
/// **Całkowitoliczbowo, bez `pow`.** Wzór `A = P·i·(1+i)ⁿ / ((1+i)ⁿ − 1)` liczy się
/// przez `n` mnożeń w `i128` zamiast przez potęgowanie zmiennoprzecinkowe — nie
/// dlatego, że `det_math::pow` jest niedostępne (jest, `K-6`), tylko dlatego, że
/// wynik jest **kwotą pieniężną**, a tam floata nie wolno użyć nawet przejściowo
/// (00 §2). `n ≤ 600`, więc pętla kosztuje mniej niż jedno wywołanie `pow`.
///
/// Stopa jest **miesięczna** w punktach bazowych: kalendarz ma 360 dni i miesiąc
/// odsetkowy zawsze równy 30 dobom (`K-1`), więc `rate_bp_month = rate_bp_annual/12`
/// bez wyjątków lutowych i bez konwencji ACT/365.
///
/// Stopa zerowa daje `P/n` — gałąź osobna, bo wzór ogólny dzieli wtedy przez zero.
#[must_use]
pub fn annuity_payment(principal: Money, rate_bp_month: i32, months: u16) -> Money {
    let n = months.max(1);
    if principal.get() <= 0 {
        return Money::ZERO;
    }
    if rate_bp_month <= 0 {
        return principal.div_round_half_up(i64::from(n));
    }
    let i = i128::from(rate_bp_month);
    let mut f = ANNUITY_SCALE;
    for _ in 0..n {
        f = f * (i128::from(BP) + i) / i128::from(BP);
    }
    let mianownik = f - ANNUITY_SCALE;
    if mianownik <= 0 {
        // Stopa tak mała, że po `n` miesiącach nie ruszyła dziewiątego miejsca
        // po przecinku — traktujemy jak zerową, zamiast dzielić przez zero.
        return principal.div_round_half_up(i64::from(n));
    }
    let licznik = i128::from(principal.get()) * i * f;
    let mianownik = mianownik * i128::from(BP);
    Money(div_round_half_away(licznik, mianownik))
}

/// Odsetki za jeden miesiąc od salda, połówki od zera (jak reszta pieniądza, 00 §2).
#[must_use]
pub fn monthly_interest(outstanding: Money, rate_bp_month: i32) -> Money {
    if outstanding.get() <= 0 || rate_bp_month <= 0 {
        return Money::ZERO;
    }
    outstanding.mul_ratio(i64::from(rate_bp_month), BP)
}

/// Zastosowanie stawki w punktach bazowych do kwoty: `v · bp / 10 000`,
/// połówki od zera (00 §2).
///
/// **Istnieje po to, żeby skala punktu bazowego miała jedno miejsce.** Dzielenie
/// przez 10 000 rozsiane po crate'ach to tyle samo miejsc, w których wolno pomylić
/// zaokrąglenie — a różnica jednego grosza na stawce widać dopiero jako rozjazd
/// wpływów budżetu po miesiącu. Bramka `scripts/macro_kernel_guard.py` egzekwuje
/// to w `sim/macro` wprost: literał `10_000` w działaniu jest tam błędem.
///
/// Stawka może być **ujemna** (przecena, korekta w dół) — stąd `i32`, a nie `u32`.
#[must_use]
pub fn apply_bp(v: Money, bp: i32) -> Money {
    if bp == 0 || v.get() == 0 {
        return Money::ZERO;
    }
    Money(div_round_half_away(
        i128::from(v.get()) * i128::from(bp),
        i128::from(BP),
    ))
}

/// Koszt własny odtworzony z ceny netto i marży: odwrotność bazy z [`next_price_full`]
/// (`base = cost · (1 + marża)`).
///
/// Potrzebna tam, gdzie znana jest cena, a nie koszt — model makro odtwarza tak
/// koszt półki przy `lift()`, bo zdjęcie widzi cenę na półce, a księga zakładu
/// zostaje w mezo. Reguła (którą stronę równania się odwraca) jest tutaj; liczba
/// (jaka marża) jest kalibracją wołającego.
///
/// Marża ≤ −10 000 bp znaczyłaby cenę ujemną albo zerową — zwracamy wtedy cenę,
/// czyli „koszt równy cenie, marża zero", zamiast dzielić przez zero.
#[must_use]
pub fn cost_from_price(price_net: Money, margin_bp: i32) -> Money {
    let m = i128::from(BP) + i128::from(margin_bp);
    if m <= 0 {
        return price_net;
    }
    Money(div_round_half_away(
        i128::from(price_net.get()) * i128::from(BP),
        m,
    ))
}

/// Odsetki naliczone za `days` dób od salda, przy stopie **miesięcznej** w bp.
///
/// Miesiąc odsetkowy ma zawsze 30 dób (`K-1`), więc dobowa część to `1/30` — i to
/// jest cała treść tej funkcji. Istnieje osobno od [`monthly_interest`], bo model
/// makro nalicza **co dobę**, a `monthly_interest(saldo) / 30` obcina grosze
/// w jedną stronę: przy 30 naliczeniach w miesiącu odsetki wychodzą systematycznie
/// niższe od miesięcznych, a błąd nie znosi się, tylko kumuluje (ten sam rachunek,
/// który wypchnął paliwo do mikrolitrów w `K-25`). Tutaj dzielenie jest **jedno**,
/// na końcu, i zaokrągla połówki od zera jak reszta pieniądza (00 §2).
#[must_use]
pub fn interest_accrual(outstanding: Money, rate_bp_month: i32, days: u16) -> Money {
    if outstanding.get() <= 0 || rate_bp_month <= 0 || days == 0 {
        return Money::ZERO;
    }
    let licznik = i128::from(outstanding.get()) * i128::from(rate_bp_month) * i128::from(days);
    Money(div_round_half_away(
        licznik,
        i128::from(BP) * i128::from(DAYS_PER_MONTH),
    ))
}

/// Doba w miesiącu odsetkowym (`K-1`: 12 × 30, bez wyjątków lutowych).
pub const DAYS_PER_MONTH: i64 = 30;

// ── daniny (§5.1 M8a, kontrakt `K-57`) ───────────────────────────────────────────

/// Arytmetyka daniny — **jedna dla miasta i dla modelu makro**.
///
/// Powód, dla którego te trzy linie mieszkają w jądrze, a nie w `sim/city`: krok
/// makro nalicza podatek dochodowy firmy tym samym wzorem, którym nalicza go miasto,
/// a `sim/city` stoi **nad** `sim/macro` w grafie i makro go nie widzi. Dwie kopie
/// mnożenia przez stawkę rozjechałyby się przy pierwszej zmianie zaokrąglenia —
/// i rozjazd byłby widoczny dopiero jako różnica wpływów budżetu między przebiegiem
/// mezo a makro, czyli w miejscu odległym od przyczyny (`K-50`).
///
/// Czego tu nie ma i nie będzie: **stawek**. Stawka jest polityką miasta
/// (`data/city/tax.ron`, `K-56`), a jądro nie zna danych — dostaje liczby
/// i oddaje liczbę.
pub mod tax {
    use super::{Money, BP};

    /// Danina liczona **od podstawy netto**: CIT, PIT, podatek od nieruchomości.
    #[must_use]
    pub fn apply(base: Money, rate_bp: u32) -> Money {
        if base.get() <= 0 {
            return Money::ZERO;
        }
        super::apply_bp(base, i32::try_from(rate_bp).unwrap_or(i32::MAX))
    }

    /// Danina **wyłuskana z kwoty brutto**: VAT (`K-7` — cena detaliczna jest tym,
    /// co płaci kupujący). Mianownik jest większy od licznika, więc wynik nigdy nie
    /// przekroczy kwoty — `Books::transfer` odrzuciłby `tax > amount`.
    #[must_use]
    pub fn from_gross(gross: Money, rate_bp: u32) -> Money {
        if rate_bp == 0 || gross.get() == 0 {
            return Money::ZERO;
        }
        gross.mul_ratio(i64::from(rate_bp), BP + i64::from(rate_bp))
    }

    /// Kwota brutto z netto — odwrotność [`from_gross`] z dokładnością do grosza
    /// zaokrąglenia, i to jest kierunek, w którym liczy sklep (`K-7`).
    #[must_use]
    pub fn add_to_net(net: Money, rate_bp: u32) -> Money {
        if rate_bp == 0 {
            return net;
        }
        Money(net.get() + apply(net, rate_bp).get())
    }
}

/// Dzielenie `i128` z zaokrągleniem połówek od zera — ta sama konwencja co
/// `Money::div_round_half_up`, tylko w szerszym typie.
fn div_round_half_away(a: i128, b: i128) -> i64 {
    let znak = if (a < 0) != (b < 0) { -1i128 } else { 1 };
    let (a, b) = (a.abs(), b.abs());
    let q = (a * 2 + b) / (b * 2);
    i64::try_from(znak * q).unwrap_or(if znak < 0 { i64::MIN } else { i64::MAX })
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

    // ── M10a/WP10.2: domknięcie jądra ────────────────────────────────────────

    #[test]
    fn odsetki_dobowe_sumuja_sie_do_miesiecznych() {
        // Trzydzieści naliczeń po jednej dobie ma dać tyle samo, co jedno
        // naliczenie miesięczne — z dokładnością do grosza zaokrąglenia.
        // To jest cały powód, dla którego `interest_accrual` istnieje obok
        // `monthly_interest`: `monthly_interest(saldo) / 30` gubi tu 9 groszy
        // na każdym miesiącu, zawsze w tę samą stronę.
        let saldo = Money(1_234_567);
        let miesiac = monthly_interest(saldo, 83);
        let doby: i64 = (0..30).map(|_| interest_accrual(saldo, 83, 1).get()).sum();
        assert!(
            (doby - miesiac.get()).abs() <= 30,
            "doby={doby} miesiac={}",
            miesiac.get()
        );
        // Jedno naliczenie za 30 dób jest równe miesięcznemu co do grosza.
        assert_eq!(interest_accrual(saldo, 83, 30), miesiac);
    }

    #[test]
    fn odsetki_od_zera_i_stopy_zerowej_sa_zerem() {
        assert_eq!(interest_accrual(Money(0), 100, 30), Money::ZERO);
        assert_eq!(interest_accrual(Money(-500), 100, 30), Money::ZERO);
        assert_eq!(interest_accrual(Money(1_000), 0, 30), Money::ZERO);
        assert_eq!(interest_accrual(Money(1_000), 100, 0), Money::ZERO);
    }

    #[test]
    fn danina_z_brutto_i_z_netto_opisuja_te_sama_transakcje() {
        // `add_to_net` i `from_gross` są dwiema stronami jednej kwoty: cena netto
        // podniesiona o stawkę, a potem wyłuskana z brutto, daje tę samą daninę
        // (z dokładnością do grosza zaokrąglenia).
        for netto in [100i64, 999, 12_345, 1_000_000] {
            let brutto = tax::add_to_net(Money(netto), 2_300);
            let danina = tax::from_gross(brutto, 2_300);
            assert!(
                (brutto.get() - netto - danina.get()).abs() <= 1,
                "netto={netto} brutto={} danina={}",
                brutto.get(),
                danina.get()
            );
            assert!(danina.get() < brutto.get(), "danina nie może zjeść kwoty");
        }
    }

    #[test]
    fn stawka_zerowa_nie_rusza_kwoty() {
        assert_eq!(tax::apply(Money(10_000), 0), Money::ZERO);
        assert_eq!(tax::from_gross(Money(10_000), 0), Money::ZERO);
        assert_eq!(tax::add_to_net(Money(10_000), 0), Money(10_000));
    }
}

// ── użyteczność zakupu (§6.4, `D20`) ─────────────────────────────────────────────

/// Wejście oceny oferty — **same liczby**, bez oferty, sklepu i mieszkańca.
///
/// Powód, dla którego ta struktura w ogóle powstała, jest w `D20`: `purchase_score`
/// miał wejść do jądra „razem ze swoim wołającym", a wołający pojawił się w M7f —
/// faza 4 kroku makro dzieli popyt komórki między firmy tym samym wzorem, którym
/// mieszkaniec wybiera sklep. `Candidate` się do tego nie nadaje, bo niesie `OfferId`,
/// czyli uchwyt do areny, której makro nie ma i mieć nie powinno.
#[derive(Clone, Copy, Debug)]
pub struct ScoreInput {
    /// Kwota, którą kupujący wyjmie z portfela za całą ilość (`K-7`).
    pub price_total: Money,
    /// Mianownik członów ceny i odległości — budżet odniesienia albo koperta.
    pub budget_ref: Money,
    /// Pieniężny koszt dojazdu razem z wyceną czasu.
    pub travel_cost: Money,
    pub quality: Q,
    pub status: Q,
    /// Człon lojalności, −1..=1: ocena z pamięci przeskalowana wagą wizyty.
    pub loyalty: f64,
    /// Człon nowości, 0..=1: zero dla miejsca odwiedzonego.
    pub novelty: f64,
    /// Człon marki, 0..=1. W M7 zawsze zero — afinitety wnosi M10 (§7.6).
    pub brand: f64,
}

/// Ocena jednej oferty (§6.4). `noise` przychodzi z zewnątrz, bo musi być **stały
/// dla trójki (kupujący, oferta, decyzja)**.
///
/// Zero zmian zachowania wobec `choice::utility_of_offer`, która od M7f jest cienkim
/// opakowaniem na tę funkcję: te same człony, ta sama kolejność sumowania, ten sam
/// mianownik. Kolejność ma tu znaczenie, bo suma `f64` nie jest łączna (00 §2).
#[must_use]
pub fn purchase_score(i: &ScoreInput, w: &UtilityWeights, noise: f64) -> f64 {
    let budget = i.budget_ref.get().max(1) as f64;
    w.price * cost_term(i.price_total.get() as f64 / budget)
        + w.quality * f64::from(i.quality.get()) / 100.0
        + w.brand * i.brand
        + w.dist * cost_term(i.travel_cost.get() as f64 / budget)
        + w.loyalty * i.loyalty
        + w.status * status_fit(i.quality, i.status)
        + w.novelty * i.novelty
        + noise
}

/// `f(x) = -ln(1 + x)` dla `x >= 0` (§5.4) — wspólny człon ceny i odległości.
///
/// Zero ma ścieżkę na skróty, bo `ln(1) = 0` i to nie jest przybliżenie, tylko
/// tożsamość. Opłaca się: człon odległości jest zerem przy **każdym** wywołaniu
/// z modelu makro (dojazd w obrębie dzielnicy nie różnicuje sklepów), a to jest
/// kilkaset tysięcy wywołań na krok. Wynik się nie zmienia, więc złoty odcisk
/// `det_math` i hashe stanu zostają bez zmian.
#[must_use]
pub fn cost_term(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    -magnat_core::det_math::ln1p(x)
}

/// Ćwiartka skali 0..100 — tier statusu i jakości (0..4).
fn tier(v: u8) -> i32 {
    i32::from(v) * 5 / 101
}

/// Dopasowanie jakości do statusu: elita nie kupuje najtańszego, a gospodarstwo
/// o niskim statusie nie kupuje luksusu — nawet gdy je stać (§5.4).
#[must_use]
pub fn status_fit(quality: Q, status: Q) -> f64 {
    1.0 - f64::from((tier(quality.get()) - tier(status.get())).abs()) / 4.0
}
