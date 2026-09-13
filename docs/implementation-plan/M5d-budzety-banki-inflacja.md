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
