# M5d — Budżety, banki, inflacja

Podfaza 4 z 5 fazy **M5 — Gospodarka detaliczna** (`M5-gospodarka-detaliczna.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M5a (księga), M5c (księgowość), M3 (gospodarstwa domowe). |
| **Pakiety robocze** | WP8, WP9, WP10 |
| **Projekt techniczny** | §5.9, §5.10 |
| **Wynik do pokazania** | Inflacja emergentna: koszyk CPI liczony z transakcji świata, stopa bazowa reagująca na niego bez ręcznego sterowania. |
| **Kryterium zamknięcia** | Kryteria WP8–WP10. |
| **Poprzednia / następna** | `M5c-ceny-i-ksiegowosc.md` · `M5e-panel-balansator-domkniecie.md` |

Budżety gospodarstw domowych, banki i kredyt, a na wierzchu CPI i stopa bazowa liczone z faktycznych transakcji.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP8 | Budżety gospodarstw domowych | WP1, M3 | M |
| WP9 | Banki i kredyt | WP1, WP7, WP8 | M |
| WP10 | CPI, stopa bazowa, inflacja emergentna | WP5, WP9 | S |

### WP8 — Budżety gospodarstw domowych
Budżetowanie kopertowe: miesięczny plan, koperta per potrzeba, `budget_ref` dla funkcji użyteczności
wyprowadzony z koperty (to jest mianownik członu ceny — bez niego `f(cena/budżet)` nie ma definicji).
Kryterium: GD o dochodzie poniżej kosztów stałych wchodzi w debet → wniosek kredytowy →
odmowa → zaległość i spadek zaspokojenia; ścieżka w pełni wyjaśnialna.

### WP9 — Banki i kredyt
Depozyt, kredyt konsumpcyjny (GD), kredyt obrotowy (sklep), ocena zdolności, harmonogram annuitetowy
sumujący się **dokładnie** do kapitału + odsetek.
Kryterium: udzielenie kredytu zwiększa podaż pieniądza o kwotę kapitału, spłata zmniejsza;
ewidencja `MoneySupplyLedger` zgadza się z sumą sald co do grosza w każdym ticku.

### WP10 — CPI, stopa bazowa, inflacja emergentna
CPI Laspeyresa z koszyka miejskiego liczony z **cen transakcyjnych ważonych wolumenem** (oferta,
której nikt nie kupuje, nie jest ceną). Bank centralny jako abstrakcja poza miastem, reguła typu
Taylora w arytmetyce całkowitej.
Kryterium: w scenariuszu „podwojenie akcji kredytowej przy stałej podaży dóbr" CPI rośnie —
bez żadnego parametru „inflacja" w kodzie (test negatywny: grep nie znajduje takiej zmiennej stanu).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.9 Budżet gospodarstwa domowego (§5.2)

```rust
pub struct HouseholdBudget {
    pub household: HouseholdId,
    pub account: AccountId,                   // rachunek bieżący w banku
    pub cash: AccountId,                      // gotówka
    pub income_monthly: Money,                // M5: z RestOfWorld (hook M7 — pensje emergentne)
    pub fixed: BTreeMap<FixedCost, Money>,    // czynsz/rata, media, ubezpieczenie, raty kredytów
    pub envelopes: BTreeMap<NeedId, Envelope>,
    pub savings_target_bp: i32,               // z cechy Oszczędność
    pub loans: Vec<LoanId>,
    pub arrears: Money,                       // zaległości; > 0 psuje scoring
}

pub struct Envelope { pub allocated: Money, pub spent: Money, pub expected_purchases: u16 }

pub fn plan_budget(b: &mut HouseholdBudget, p: &HouseholdProfile, d: &EconomyData, t: Tick);
pub fn budget_ref_for_need(b: &HouseholdBudget, need: NeedId) -> Money;   // mianownik w f(cena/budżet)
```

Miesięcznie: `disposable = income − Σ fixed − income · savings_target_bp / 10_000`.
Podział `disposable` na koperty wg wag z `data/economy/envelopes.ron` (per typ GD z §5.2)
modulowanych osobowością; **reszta z dzielenia trafia do pierwszej koperty wg `NeedId`** (dok. 00 §2).
Brak środków na koszty stałe → wniosek o kredyt konsumpcyjny → odmowa → `arrears += brak`
i zdarzenie do M3 (stres, §5.3). Nadwyżka → przelew na `Savings`.

### 5.10 Banki, kredyt, stopa bazowa (§6.5)

```rust
pub struct Loan {
    pub id: LoanId,
    pub borrower: AccountOwner, pub lender: FirmId,
    pub kind: LoanKind,                      // Consumer | WorkingCapital
    pub principal: Money, pub outstanding: Money,
    pub rate_bp_annual: i32,
    pub term_months: u16, pub paid_months: u16,
    pub schedule: Vec<Installment>,          // annuitet; Σ principal == principal DOKŁADNIE
    pub arrears_months: u8,
}
pub struct Installment { pub due: Tick, pub principal: Money, pub interest: Money }

pub fn build_schedule(principal: Money, rate_bp: i32, months: u16, first_due: Tick) -> Vec<Installment>;
// Ostatnia rata pochłania resztę z zaokrągleń — suma kapitałów == principal co do grosza.
// K-1: kalendarz 360 dni (12 × 30). Miesiąc odsetkowy == 30 dni ZAWSZE, więc stopa miesięczna
// = rate_bp_annual / 12 bez wyjątków lutowych i bez konwencji ACT/365. Terminy rat:
// first_due + k · 30 dni. Ta sama siatka obowiązuje okresy księgowe (ledger_close)
// i okno CPI — „30 dni" i „miesiąc" to w M5 jedno i to samo, nigdy dwa różne pojęcia.

pub fn assess_credit(app: &LoanApplication, b: &Books, hist: &CreditHistory, rate: BaseRate)
    -> CreditDecision;

pub enum CreditDecision {
    Approved { limit: Money, rate_bp: i32, reason: DecisionReason },
    Rejected { cause: RejectCredit, reason: DecisionReason },   // DstiTooHigh | NoIncome | Arrears | DscrTooLow
}
```

Ocena zdolności (§6.5: „historia, zabezpieczenie, przepływy") — prosta i wyjaśnialna:
- GD: `dsti_bp = (Σ istniejących rat + nowa rata) · 10_000 / dochód_netto` ≤ `dsti_limit_bp`;
  korekta o `credit_history_score` (zaległości z ostatnich 24 mies.).
- Sklep: `dscr = ebitda_12m / debt_service_12m` ≥ `dscr_min`; plus minimalny staż działalności.
- `rate_bp = base_rate_bp + risk_premium_bp(scoring) + product_spread_bp(kind)`.

**Kreacja pieniądza.** Uruchomienie kredytu: bank tworzy depozyt (`Books::create_credit`)
→ `MoneySupplyLedger.credit_created += principal`. Spłata kapitału niszczy pieniądz
(`destroy_credit`) → `credit_repaid += principal`. Odsetki to zwykły przelew do banku,
**nie** zmieniają podaży. To jest cały mechanizm — inflacja wychodzi z niego sama.

**CPI i stopa bazowa**

```rust
pub struct CpiBasket { pub items: Vec<(GoodId, Qty)> }     // koszyk miejski, data/economy/cpi.ron
pub fn cpi(stats: &TransactionStats, basket: &CpiBasket, window_days: u16) -> IndexBp;  // 10_000 = baza

pub struct BaseRate { pub bp: i32, pub set_at: Tick }
pub fn update_base_rate(cur: BaseRate, cpi_yoy_bp: i32, d: &EconomyData) -> BaseRate;
```

CPI = Laspeyres: `Σ p_t·q_0 / Σ p_0·q_0`, gdzie `p_t` to **średnia ważona wolumenem transakcji**
z ostatnich 30 dni, nie średnia ofert. Oferta, której nikt nie kupuje, nie jest ceną (§6.1).

Bank centralny (abstrakcja poza miastem, EveryMonth), reguła typu Taylora w arytmetyce całkowitej:
`base_bp = clamp(neutral_bp + a_bp · (cpi_yoy_bp − target_bp) / 10_000, floor_bp, ceil_bp)`,
z ograniczeniem zmiany do ±`max_step_bp` na miesiąc. Reakcja jest celowo powolna — bank centralny
ma stabilizować, nie sterować; sterowanie zabiłoby emergencję, którą balansator ma mierzyć.

**W kodzie nie istnieje zmienna „inflacja" jako wejście.** Pętla: kredyt → depozyty →
większe koperty GD → więcej zakupów powyżej progu → szybsze schodzenie zapasów →
`adj_stock` dodatni → wyższe ceny ofert → wyższe CPI. Test negatywny w CI (grep) pilnuje,
żeby nikt nie „poprawił" tego skrótem.

---

## Zmiany wpisane po M3c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3c —
podfaza nie jest tu przeprojektowywana.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`HouseholdBudget` z §5.9 stoi na komponencie `Household` (M3c §5.6), a nie obok niego.** `income_monthly` **już tam jest** i wypełnia je generacja populacji; `cash`, `bank`, `savings` i `debt` też. Budżet M5 dokłada koperty, koszty stałe, kredyty i zaległości, a pola pieniężne **czyta i pisze w komponencie** | Dwa źródła salda gospodarstwa rozjeżdżają się przy pierwszej transakcji, a testu, który by to złapał, nie ma po żadnej ze stron. `society::total_money` sumuje dziś pieniądz z `Wealth` mieszkańców i z `Household`; gdyby M5 trzymał saldo u siebie, test zachowania pieniądza (00 §6) przestałby cokolwiek znaczyć |
| ★ | **`NeedId` w sygnaturach §5.9 to `core::NeedKind`** (dwanaście wariantów, K-20), a klucz koperty zakupowej to `core::StockCat` (osiem kategorii). `StockCat::need()` mówi, którą potrzebę uzupełnia zakup w danej kategorii | Typ `NeedId` nie powstał i nie powstanie — słownik potrzeb mieszka w `core` od M3a. `Household.stock: [u8; STOCK_CAT_COUNT]` jest indeksowany `StockCat` i to on jest polem, które M5 zastępuje realnym towarem (zapowiedź z §6.2 dokumentu fazy M3) |
| ★ | **„Typ GD" w `data/economy/envelopes.ron` to `HouseholdKind`**, wyliczany ze **składu** gospodarstwa (`household::classify`), a nie przechowywany jako deklaracja | Rodzina po wyprowadzce dzieci przestaje być rodziną z dziećmi w tej samej minucie, w której ostatnie z nich wychodzi — bez osobnej mechaniki „przekwalifikowania". Wagi kopert per typ zmieniają się wtedy same |
| | **Katalog `data/economy/` nie jest w liście z `00` §5**, która deklaruje się jako kompletna. M5 dopisuje go tam razem z pierwszym plikiem | Ta sama sytuacja co `data/ui/` przy M2 (`K-19`): lista jest kompletna, więc brak wpisu jest jej błędem, nie luką |

---

## Zmiany wpisane po M5b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5b.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`HouseholdBudget.account` i `.cash` z §5.9 NIE są `AccountId`.** Saldo gospodarstwa mieszka w komponencie `Household`, a `Books` widzi je przez kanał `MoneySupplyLedger.household_sector_in/out` (`U-17` w dokumencie fazy). Koperty, koszty stałe, kredyty i zaległości stoją **na polach komponentu** | To jest wykonanie korekty ★ wpisanej tu po M3c: dwa źródła salda rozjeżdżają się przy pierwszej transakcji. M5b sprawdził to na żywo — zakup zdejmuje kwotę z komponentu i księguje ją w `Books` jednym wywołaniem, a niezmiennik `society::total_money + Books::total_balance()` jest zielony co do grosza na 226 tys. transakcji |
| ★ | **Wypłata dochodu już istnieje: `sim/economy::pay_incomes`** (decyzja otwarta nr 2 zamknięta w M5b, `U-16`). WP8 dokłada **podział** kwoty na koperty, a nie kanał wypłaty | Bez wypłaty M5b nie miał czego pokazać: gospodarstwa z Etapu 8 mają saldo zero, więc pierwszy przebieg dał 170 tys. odmów „brak środków". Kanał jest więc zbudowany i przetestowany; WP8 wchodzi na gotowe, a M7 podmienia wyłącznie źródło kwoty |
| ★ | **`budget_ref_for_need` podmienia jedno wywołanie, nie tabelę.** W M5b mianownik członu ceny bierze się ze stałej per `StockCat` z `data/economy/choice.ron` (`budget_ref_gr`); WP8 zastępuje `choice::budget_ref_for` obliczeniem z koperty | Rząd wielkości tej liczby **decyduje o tym, czy cena w ogóle waży**: mianownik miesięczny zamiast „na jedno wyjście po zakupy" spłaszcza człon ceny do zera i rynek staje się losowaniem (korekta `V-9`). WP8 musi utrzymać tę skalę, inaczej kalibracja softmaxu przestanie pasować |
| | **`Loan.borrower: AccountOwner` zostaje, ale dla gospodarstwa nie wskaże konta w `Books`** — wskaże `AccountOwner::Household(HouseholdId)` jako **tożsamość**, a spłata rusza polami komponentu i kanałem sektora gospodarstw | Kreacja i destrukcja pieniądza kredytowego (`create_credit`/`destroy_credit`) działa na kontach, więc kredyt dla gospodarstwa wpływa **najpierw na konto banku**, a stamtąd kanałem do komponentu. To jedno dodatkowe wywołanie, nie inna mechanika |
| | **CPI ma z czego liczyć od M5b**: `TxKind::RetailSale` niesie `good`, `qty` i kwotę, a dziennik-pierścień `TxJournal` ma 4096 wpisów | Okno CPI to 30 dni, a pierścień starcza na dobę ruchu jednego miasta — WP10 potrzebuje więc **własnego agregatu** narastającego (suma kwot i ilości per `GoodId` per dzień), a nie odczytu z dziennika. To jest różnica między „mam dane" a „mam je wtedy, kiedy ich potrzebuję" |
