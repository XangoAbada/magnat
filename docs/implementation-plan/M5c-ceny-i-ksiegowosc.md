# M5c — Ceny i księgowość

Podfaza 3 z 5 fazy **M5 — Gospodarka detaliczna** (`M5-gospodarka-detaliczna.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M5b (transakcje). |
| **Pakiety robocze** | WP6, WP7, WP11 |
| **Projekt techniczny** | §5.6, §5.8 |
| **Wynik do pokazania** | Sklep AI podnosi cenę przy niedoborze i obniża przy zaleganiu; rachunek wyników i bilans domykają się co do grosza. |
| **Kryterium zamknięcia** | Kryteria WP6, WP7 i WP11. |
| **Poprzednia / następna** | `M5b-sklep-i-zakup.md` · `M5d-budzety-banki-inflacja.md` |

Polityki cenowe AI, pełna księgowość sklepu (rachunek wyników, bilans, przepływy) i delegowanie polityk cenowych graczowi.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP6 | Polityki cenowe AI | WP2, WP3, WP5 | L |
| WP7 | Księgowość sklepu | WP1, WP3, WP5 | L |
| WP11 | Polityki cenowe delegowane przez gracza | WP6 | S |

### WP6 — Polityki cenowe AI
Cztery mechanizmy korekty (magazyn, konkurencja z opóźnieniem, eksperyment, przecena psującego się)
składane w jedną cenę w arytmetyce punktów bazowych.
Kryterium: sklep z nadmiarem zapasu obniża cenę w ciągu 3 dni; sklep z brakami podnosi;
sklep obok konkurenta, który obniżył cenę, reaguje **nie wcześniej niż po 1 i nie później niż po 7 dniach**
(test mierzy opóźnienie na 100 firmach i weryfikuje rozkład).

### WP7 — Księgowość sklepu
Plan kont jako enum, dziennik z zapisami zbilansowanymi (Σ Wn = Σ Ma, tolerancja 0),
raporty wyprowadzane z dziennika, nie z liczników obok.
Kryterium: po roku symulacji bilans się zamyka co do grosza; suma `Revenue − Cogs − koszty`
z RZiS równa się zmianie `RetainedEarnings`; wartość `InventoryGoods` w bilansie równa się
`Σ StockLine.cost_total` co do grosza.

### WP11 — Polityki cenowe delegowane przez gracza
**Ten sam** typ `PricePolicy` co AI (§6.3 mówi wprost: „z tym samym zestawem narzędzi co AI").
Zero osobnej ścieżki kodu. UI: wybór wariantu + parametry + podgląd „co by się stało z ceną dziś".
Kryterium: polityka „−2% względem najtańszego konkurenta w promieniu 3 km" ustawiona przez gracza
i przez AI daje identyczną cenę przy identycznym stanie.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.6 Polityki cenowe (§6.3)

```rust
pub enum PricePolicy {
    Fixed          { price: Money },
    Markup         { target_margin_bp: i32 },
    MatchCompetitor{ delta_bp: i32, radius_m: u32, reference: CompetitorRef },  // „−2% vs najtańszy w 3 km"
    Dynamic        { target_margin_bp: i32, floor_margin_bp: i32, ceil_margin_bp: i32 },
}

pub enum CompetitorRef { Cheapest, Median, Named(SiteId) }

pub struct PriceController {            // stan per (SiteId, GoodId)
    pub policy: PricePolicy,
    pub current: Money,
    pub last_change: Tick,
    pub observed: CompetitorSnapshot,   // ceny konkurencji z opóźnieniem 1–7 dni
    pub elasticity: Option<ObservedElasticity>,
    pub experiment: Option<PriceExperiment>,
    pub delegated: bool,                // true = gracz oddał sterowanie polityce
}

pub fn reprice(pc: &mut PriceController, ctx: &PricingCtx, t: Tick) -> Option<DecisionReason>;
```

Składanie ceny — **cała arytmetyka w punktach bazowych i `i64`, zero floatów** (dok. 00 §2):

```
base_bp   = unit_cost · (10_000 + target_margin_bp) / 10_000

adj_stock = clamp(k_stock · (stock_bp_of_target − 10_000) / 10_000, −2000, +1500)
            // nadmiar zapasu → ujemne (obniżka); brak → dodatnie (podwyżka)
adj_comp  = clamp(k_comp · (observed_ref_price · 10_000 / base − 10_000) / 10_000, −1500, +1500)
adj_elast = z eksperymentu: krok w stronę p* = p · (1 + 1/(e + 1)), ograniczony do ±500 bp
adj_spoil = schodkowo wg pozostałego terminu ważności: −2000 / −4000 / −6000 bp

price = clamp(base_bp · (10_000 + adj_stock + adj_comp + adj_elast + adj_spoil) / 10_000,
              floor = unit_cost · (10_000 + min_margin_bp) / 10_000,
              ceil  = unit_cost · (10_000 + max_margin_bp) / 10_000)
```

**Podstawa ceny (K-7).** `unit_cost` jest kwotą netto, a cena detaliczna to brutto — w M5 pokrywają
się, bo VAT ≡ 0, ale `reprice` musi porównywać w jednej podstawie **od pierwszego dnia**, inaczej
w M8 marże skoczą o stawkę VAT-u bez zmiany żadnej polityki. Dlatego: całe składanie ceny
(`base_bp`, `floor`, `ceil`, `adj_comp`) dzieje się **netto**, a przeliczenie na `Offer.unit_price`
w podstawie `GrossRetail` jest ostatnim krokiem, przez `TaxEngine` (w M5 `NoTax`, mnożnik 1).
Cena konkurencji z `CompetitorSnapshot` przychodzi brutto i jest sprowadzana do netto tym samym
mechanizmem przed użyciem w `adj_comp`.

`floor` jest krytyczny: bez niego sklepy w nadmiarze zapasu wpadają w spiralę deflacyjną poniżej
kosztu. To dokładnie ten mechanizm, który bramka balansatora ma pilnować (sekcja 7.4).
`k_stock`, `k_comp`, `min_margin_bp`, `max_margin_bp` — z osobowości firmy (§12 PRD, w M5 z danych).

**Obserwacja konkurencji z opóźnieniem 1–7 dni (§6.3)**

```rust
pub struct CompetitorSnapshot {
    pub entries: Vec<CompetitorEntry>,   // posortowane po SiteId
    pub refreshed: Tick,
    pub delay_days: u8,                  // 1..=7, losowane raz per firma
}
pub struct CompetitorEntry { pub site: SiteId, pub good: GoodId, pub price: Money, pub seen_at: Tick }
```

`delay_days = 1 + rng(world_seed, StreamId::CompetitorDelay, firm.index(), 0) % 7`, stałe dla firmy
(to jest jej „czujność"). System `observe_competitors` (EveryDay) odświeża tylko te wpisy, których
wiek przekroczył `delay_days` — jeden przelot indeksu w promieniu na firmę, nie skan całego rynku.
Sklep **działa na starej cenie konkurenta** — to jest źródło realnych błędów decyzyjnych AI
i pole manewru dla gracza (przecena na 3 dni, zanim konkurencja zauważy).

**Eksperymenty cenowe (§6.3)**

```rust
pub struct PriceExperiment { pub direction: i8, pub magnitude_bp: i32,
                             pub started: Tick, pub len_days: u8, pub baseline_units: Qty }
pub struct ObservedElasticity { pub e: f32, pub measured_at: Tick, pub samples: u32 }
```

Uruchamiane gdy `personality.risk > próg`, nie częściej niż co 30 dni, magnitude ±3–10%.
Po zakończeniu okna: `e = ln(q1/q0) / ln(p1/p0)` (float — wynik steruje parametrem polityki,
nie kwotą pieniężną, więc dozwolone; wpis do stanu trwałego kwantyzowany do `i32` w bp).
Jeśli `|Δln(q)|` poniżej progu szumu — eksperyment nierozstrzygnięty, `e` bez zmian.
Gracz widzi wynik eksperymentu w raporcie (§6.4: „gracz może ją zmierzyć eksperymentem").

**Polityki gracza (WP11)** — ten sam `PricePolicy`, ta sama funkcja `reprice`. Różnica wyłącznie
w tym, kto ustawia wariant. `delegated == false` oznacza `PricePolicy::Fixed` ustawiony ręcznie.

### 5.8 Księgowość sklepu (§6.9)

```rust
pub struct Ledger {
    pub site: SiteId,
    pub firm: FirmId,
    balances: BTreeMap<LedgerAccount, Money>,
    journal: RingBuffer<JournalEntry>,          // ostatnie N; starsze na dysk (engine/io)
    pub periods: Vec<PeriodClose>,              // miesięczne domknięcia
}

pub enum LedgerAccount {
    // Aktywa
    Cash, BankCurrent, InventoryGoods, FixedAssets, AccumDepreciation,
    // Pasywa — rozbite na *Payable na wniosek M8 (zobowiązanie ma mieć wierzyciela i termin)
    TradePayable,        // dostawcy
    TaxPayable,          // VAT + daniny z ChargeRegistry (M8); w M5 zawsze 0
    WagePayable,         // naliczone, niewypłacone (hook M7)
    LoansShort, LoansLong, Equity, RetainedEarnings,
    // RZiS
    Revenue, Cogs, WagesExpense, RentExpense, UtilitiesExpense,
    DepreciationExpense, InterestExpense, WriteOffExpense,
    TaxExpense,          // HOOK M8 — w M5 zawsze 0
}

pub struct JournalEntry { pub tick: Tick, pub tx: Option<TxId>,
                          pub lines: SmallVec<[(LedgerAccount, Money); 4]>,  // Σ == 0
                          pub reason: DecisionReason }

pub fn post(ledger: &mut Ledger, e: JournalEntry) -> Result<(), LedgerError>; // odrzuca niezbilansowane

pub fn income_statement(l: &Ledger, from: Tick, to: Tick) -> IncomeStatement;
pub fn balance_sheet  (l: &Ledger, at: Tick)              -> BalanceSheet;
pub fn cash_flow      (l: &Ledger, from: Tick, to: Tick)  -> CashFlow;   // metoda bezpośrednia z dziennika
```

**Wycena zapasów — średnia ważona z zabezpieczeniem przed dryfem**

```rust
/// Koszt własny sprzedaży przy sprzedaży qty_sold z linii o (qty, cost_total).
pub fn take_cogs(line: &mut StockLine, qty_sold: Qty) -> Money {
    debug_assert!(qty_sold <= line.qty);
    let cogs = if qty_sold == line.qty {
        line.cost_total                       // zmiatamy resztę — linia zeruje się DOKŁADNIE
    } else {
        Money(mul_div_i128(line.cost_total.0, qty_sold.0, line.qty.0))  // zaokrąglenie half-up
    };
    line.qty -= qty_sold;
    line.cost_total -= cogs;
    cogs
}
```

Bez gałęzi „zmiatania reszty" `cost_total` nie schodzi do zera przy wyzerowanym `qty` i bilans
przestaje się zamykać po kilku tysiącach transakcji. To nie jest optymalizacja, to warunek poprawności.

Zdarzenia księgowe w M5: sprzedaż (Revenue/Cogs), zakup u dostawcy (InventoryGoods/TradePayable→Cash),
odpis towaru przeterminowanego (WriteOffExpense), czynsz, media, amortyzacja wyposażenia (liniowa,
miesięczna), odsetki i raty kredytu, wypłaty (M5: stała kwota, hook M7).

---

## Zmiany wpisane po M5b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5b —
podfaza nie jest tu przeprojektowywana.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`reprice` zmienia `Offer` przez `Market::set_price`, a nie przez zasób `Arena<Offer>`.** Arena ofert mieszka wewnątrz `Market` (`K-29`, korekta `V-3` w M5b), więc polityka cenowa musi wejść tam, gdzie siedzi stan: albo metodą na `Market`, albo systemem wołającym `Market` — nie zapytaniem ECS po ofertach | `PlaceProvider` nie dostaje `&World`, więc wszystko, czego dotyka decyzja zakupowa, jest osiągalne wyłącznie z uchwytu rynku. Zmiana ceny **nie brudzi indeksu** (indeks trzyma uchwyty, cena czyta się z areny na żywo) — to zostaje z M5a bez zmian |
| ★ | **Obserwacja konkurencji ma gotowy licznik: `Offer.price_rev`.** Porównanie z zapamiętaną rewizją zastępuje kopię cennika konkurenta | Pole powstało w M5a właśnie na to i nie ma jeszcze wołającego. Kryterium WP6 („reakcja nie wcześniej niż po 1 i nie później niż po 7 dniach") mierzy się wtedy na dacie zapamiętania rewizji, a nie na różnicy cen |
| ★ | **Poślizg ceny (§5.5) dostaje w M5c swojego pierwszego konsumenta.** W M5b mechanizm jest kompletny, ale bezczynny: `PlannedPurchase` zapamiętuje cenę z chwili decyzji, `price_slippage_bp` stoi w `data/economy/choice.ron`, a `MarketStats.slippage_rechecks` liczy zera, bo ceny się nie ruszają | Pierwszy `reprice` uruchamia całą tę ścieżkę naraz. Warto sprawdzić licznik po włączeniu polityk cenowych — jeśli zostanie zerem, znaczy to, że ceny zmieniają się wyłącznie w nocy i nikt nigdy nie zastaje innej ceny, niż widział przy planowaniu |
| ★ | **`take_cogs` w `kernel` przejmuje gotową funkcję `shop::take_units`, a nie pisze jej od nowa.** Gałąź zmiatania reszty (R5) jest już zaimplementowana i ma własny test (`zdjecie_calej_linii_zmiata_reszte_groszy`) | D20 wymaga „zera zmian zachowania" tam, gdzie coś realnie się przenosi — a tu się przenosi. Złoty plik ciągu hashy przed przenosinami i po nich obowiązuje |
| | **Wycena zapasu jest już rozdzielona na dwa miejsca: zaplecze i półkę.** `Market::inventory_value(site)` sumuje oba i to jest lewa strona niezmiennika P5 | `InventoryGoods` z bilansu musi się równać sumie **obu**, nie samego zaplecza. Towar wyłożony na półkę nie przestaje być majątkiem sklepu, a łatwo o to potknięcie, bo `Offer.available` patrzy tylko na półkę |
| | **Konto sklepu istnieje od M5b** (`AccountOwner::Firm`, otwierane przy stawianiu sklepu) i jest już obciążane zakupami u dostawcy zewnętrznego | `Ledger` z WP7 stoi więc obok istniejącego rachunku, a nie zamiast niego: dziennik księgowy jest widokiem na te same przepływy, nie drugim saldem |
