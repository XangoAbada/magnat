# M8a — Pieniądz publiczny

Podfaza 1 z 5 fazy **M8 — Miasto jako aktor** (`M8-miasto-jako-aktor.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M5 (pieniądz, księgowość), M6 (import — cła), M7 (wynik firmy — CIT). |
| **Pakiety robocze** | WP1, WP2 |
| **Projekt techniczny** | §5.0, §5.1 |
| **Wynik do pokazania** | Test T1 (domknięcie podatkowe) zielony na scenariuszu rocznym; każde obciążenie widoczne w karcie inspekcji firmy z nazwą, stawką i podstawą. |
| **Kryterium zamknięcia** | Kryteria WP1 i WP2; suma pieniądza w świecie razem z `CityBudget.cash` niezmienna z tolerancją 0 gr. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M8b-sieci-przesylowe.md` |

Szkielet `sim/city` z `CityBudget` i księgą publiczną oraz `TaxCode` z siedmioma daninami i trójstopniowym cyklem życia należności.

---

## Pakiety robocze

### WP1 — Szkielet `sim/city`, `CityBudget`, księga publiczna
**Zależy od:** M5 (pieniądz, księgowość), M0 (ECS, RNG).
Encja `City` jako singleton ECS z komponentami `CityBudget`, `TaxCode`, `PolicySet`, `Government`.
Księga publiczna: przychody per `TaxKind`, wydatki per `SpendCategory`, saldo gotówki,
dług miejski (obligacje u banków M5), zobowiązania i należności z terminami.
Domknięcie miesiąca i roku budżetowego. Awaryjne domknięcie deficytu (cięcia albo dług).
**Kryterium ukończenia:** headless z ręcznie wstrzykniętymi wpłatami i wypłatami przechodzi
test zachowania pieniądza: `Σ firmy + Σ mieszkańcy + Σ banki + CityBudget.cash = const`,
tolerancja 0 gr, 100 tys. ticków.
**Rozmiar:** M

### WP2 — `TaxCode` i naliczanie danin
**Zależy od:** WP1, M5 (transakcje, księga), M6 (import — cła), M7 (wynik firmy — CIT).
Siedem danin z PRD §6.8, każda jako czysta funkcja naliczająca + system, który ją wywołuje
w odpowiednim momencie cyklu. Trójstopniowy cykl życia należności:
**naliczenie → zaksięgowanie zobowiązania → zapłata → wpływ do budżetu**, z jawnym stanem
zaległości i umorzenia. Zmiana stawki nigdy nie działa wstecz (`effective_from`).
**Kryterium ukończenia:** test domknięcia podatkowego (sekcja 7, T1) zielony na scenariuszu
rocznym; każde obciążenie widoczne w karcie inspekcji firmy z nazwą, stawką i podstawą.
**Rozmiar:** L

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.0 Konwencje fazy

- **Zero floatów w podatkach, budżecie, taryfach i hazardzie.** Stawki w punktach bazowych
  (`u32`, 10000 bps = 100%), krzywe hazardu jako łamane na `i64`, prawdopodobieństwa w ppm (`u32`).
  Uzasadnienie: wynik losowania zdarzenia musi być identyczny bitowo na każdej platformie.
  To wymóg **silniejszy** niż K-6 (zakaz `f64::exp`/`powf`) — M8 nie potrzebuje nawet
  `core::det_math` w ścieżce hazardu, bo cała ocena jest całkowitoliczbowa.
- **Kalendarz: K-1 — 360 dni, 12 × 30, bez lat przestępnych.** Wszystkie okresy podatkowe,
  kadencje, sezony i harmonogramy konserwacji liczą w tej jednostce. Miesiąc = 43 200 ticków
  ekonomicznych, rok = 518 400. Dzięki temu `annual/12` nie ma reszty kalendarzowej,
  a rozliczenie miesięczne podatku od nieruchomości jest dokładne z definicji.
- **Dwie siatki czasu, nigdy zmieszane (K-15).** Tydzień jest 7-dniowy i **dryfuje** względem
  miesiąca (30 nie dzieli się przez 7), więc M8 trzyma je rozdzielnie:
  - **siatka tygodniowa** (`DayOfWeek` z `SimCalendar` w `engine/core` — M8 nie liczy go sam,
    ani z własnego modulo): godziny handlu, zakaz handlu w niedziele, zakaz ruchu ciężkiego
    w wybrane dni, grafiki usług publicznych (urząd i szkoła nie pracują w weekend,
    szpital i policja tak), weekendowy profil poboru mediów;
  - **siatka miesięczna/kwartalna** (30 / 90 / 360 dni, dzielą się bez reszty): okresy
    rozliczeniowe wszystkich danin, terminy płatności, domknięcia budżetu, kadencje i wybory,
    finansowanie usług, taryfy.

  Żaden okres rozliczeniowy nie jest definiowany w tygodniach. Żadna regulacja handlowa nie
  jest definiowana w miesiącach. Test T1 zależy od tego wprost: podatek liczony „co tydzień"
  nie domknąłby się do roku.
- **Typy wspólne bierzemy z `engine/core`, nie definiujemy własnych (K-8, K-12):**
  `UtilityKind`, `TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind`, `DecisionReason`.
  `UtilityKind` jest tam, mimo że właścicielem sieci jest M8 — bo używa go też `TxKind::Utility`
  w M5 i wybór środka transportu w M4.
- **`sim/city` nie mutuje cudzych komponentów.** Wszystko wychodzi buforem komend:
  `TaxCharge`, `ParamPatch`, `ServiceEffect`, `PermitDecision`. Właściciel komponentu aplikuje.
- **Pieniądz porusza się wyłącznie przez `Books::transfer` (M5).** `sim/city` nie ma dostępu
  do sald; `sys_settle_tax_charges` emituje transfer i odczytuje jego wynik. Miasto jest
  zwykłym posiadaczem konta (`AccountOwner::City`), więc niezmiennik `MoneySupplyLedger`
  obejmuje budżet miasta bez wyjątków.
- **Blok `StreamId` dla M8 (K-4): 240–259.** Przydział stały:
  `240 EventHazard`, `241 EventRoll`, `242 EventSeverity`, `243 EventDuration`,
  `244 Weather`, `245 Election`, `246 Audit`, `247 PermitProcessing`,
  `248 GridFault`, `249 CityPolicy`, `250 TenderScoring`, `251 Demography`,
  252–259 wolne. Wartości raz nadane są niezmienne.

### 5.1 Budżet miasta i podatki

```rust
// ---------- budżet ----------

pub struct CityBudget {
    pub cash: Money,                                  // konto miasta w banku (M5)
    pub revenue_ytd:  [Money; TAX_KIND_COUNT],        // wpływy zrealizowane, per danina
    pub revenue_other: [Money; OTHER_REVENUE_COUNT],  // opłaty, mandaty, dywidendy ze spółek miejskich
    pub spend_ytd:    [Money; SPEND_CATEGORY_COUNT],
    pub receivables:  Vec<TaxChargeId>,               // wystawione, niezapłacone
    pub debt: Vec<MunicipalBond>,                     // obligacje: nominał, kupon bps, termin
    pub capital_projects: Vec<CapitalProject>,        // inwestycje w toku, harmonogram płatności
    pub fiscal_year: u16,
    pub reserve_target_months: u8,                    // polityka rezerwy
}

pub enum SpendCategory {
    Education, Health, Police, Fire, Waste, Parks, Administration,
    TransitSubsidy, RoadMaintenance, CapitalInvestment, Subsidies, DebtService,
}

// ---------- kodeks podatkowy ----------

pub enum TaxKind {
    Cit, Pit, Vat, Property,
    Excise(ExciseClass),        // Fuel | Alcohol | Tobacco | Energy
    Duty(TariffClass),          // klasa celna towaru
    License(LicenseClass),      // Alcohol | Fuel | Gambling | Waste | Transport | FoodService
}

pub struct TaxCode {
    pub schema_version: u16,
    pub cit_bps: u32,
    pub pit_brackets: Vec<PitBracket>,          // { upper: Money, bps: u32 }, ostatni bez górnej granicy
    pub pit_free_allowance: Money,              // roczna kwota wolna
    pub vat_bps_by_class: Vec<u32>,             // indeks = VatClass towaru (z data/goods/)
    pub property_bps_per_year: u32,
    pub excise: Vec<ExciseRate>,                // { class, per_unit: Money, unit: ExciseUnit }
    pub duty_bps_by_class: Vec<u32>,
    pub license_fee_per_year: Vec<Money>,
    pub late_interest_bps_per_year: u32,
    pub effective_from: Tick,                   // zmiana stawek nigdy wstecz
}
```

**Funkcje naliczające — czyste, bez dostępu do świata, testowalne jednostkowo:**

```rust
/// VAT z ceny brutto (cena detaliczna jest brutto — mieszkaniec widzi to, co płaci).
/// vat = round_half_up(gross * bps / (10000 + bps))
pub fn vat_from_gross(gross: Money, bps: u32) -> Money;
pub fn vat_add_to_net(net: Money, bps: u32) -> Money;

/// CIT od dochodu rocznego z uwzględnieniem strat z lat ubiegłych.
/// Zwraca (należność, nowy stan straty do rozliczenia).
pub fn cit_due(taxable_profit: Money, loss_carry: Money, bps: u32) -> (Money, Money);

/// PIT pobierany u źródła przy wypłacie, narastająco od początku roku
/// (żeby suma zaliczek = podatek roczny co do grosza).
pub fn pit_withheld(gross_wage: Money, ytd_gross: Money, ytd_withheld: Money,
                    code: &TaxCode) -> Money;

/// Podatek od nieruchomości rozliczany miesięcznie, ale sumujący się dokładnie do rocznego:
/// month_amount = annual*m/12 - annual*(m-1)/12  (dzielenie całkowite — reszta ląduje w grudniu)
pub fn property_tax_month(cadastral_value: Money, bps_per_year: u32, month: u8) -> Money;

pub fn excise_due(qty: Qty, rate: &ExciseRate) -> Money;       // stawka kwotowa, nie procentowa
pub fn duty_due(customs_value: Money, bps: u32) -> Money;
pub fn license_fee(class: LicenseClass, code: &TaxCode) -> Money;
```

**Należność i jej cykl życia:**

```rust
pub struct TaxCharge {
    pub id: TaxChargeId,
    pub payer: TaxPayer,              // Firm(FirmId) | Citizen(CitizenId) | Household(HouseholdId)
    pub kind: TaxKind,
    pub period: FiscalPeriod,         // Month(y, m) | Year(y) | Instant(tick)  — na akcyzę i cło
    pub base: Money,                  // podstawa (dla akcyzy: wartość ekwiwalentna; ilość w `base_qty`)
    pub base_qty: Qty,
    pub rate_snapshot: u32,           // stawka użyta — dla wyjaśnialności i audytu
    pub amount: Money,
    pub assessed_at: Tick,
    pub due_at: Tick,
    pub state: ChargeState,
    pub reason: TaxReason,            // enum z parametrami (§7 dokumentu 00), nie string
}

pub enum ChargeState {
    Assessed,                         // naliczony, zaksięgowany jako zobowiązanie u płatnika
    Settled { at: Tick },             // zapłacony → wpływ do budżetu
    Overdue { since: Tick, interest_accrued: Money },
    Abated  { at: Tick, why: AbateReason },   // umorzony (upadłość, decyzja rady, przedawnienie)
}
```

**Miejsce w księgowości firmy.** `sim/city` **nie księguje sam** — emituje `TaxCharge`
do bufora komend; księgę (`Ledger`, `JournalEntry`, `post`) prowadzi M5. M8 dostarcza
implementację haka, który M5 zostawił:

```rust
/// M5 definiuje `trait TaxEngine` i domyślną implementację `NoTax`.
/// M8 dostarcza jedyną prawdziwą implementację — nic więcej w M5 się nie zmienia.
pub struct CityTaxEngine { code: TaxCode, charges: ChargeRegistry }

impl TaxEngine for CityTaxEngine {
    /// Wywoływane przez M5 w `settle_transactions` PRZED `Books::transfer`.
    /// Zwraca część podatkową kwoty brutto; M5 wypełnia nią `Transaction.tax`.
    fn tax_on(&mut self, kind: &TxKind, gross: Money, t: Tick) -> Money;
    /// Wywoływane po zaksięgowaniu — rejestruje zobowiązanie do rozliczenia okresowego.
    fn record(&mut self, tx: TxId, payer: TaxPayer, kind: TaxKind, base: Money, amount: Money, t: Tick);
}
```

**Odpowiedź na pytanie otwarte M5 nr 8** (`Transaction.tax` czy osobny rejestr):
**oba, z podziałem ról.** Pole `Transaction.tax` zostaje i wystarcza dla VAT-u — transakcja
detaliczna dotyczy jednego towaru, więc ma dokładnie jedną stawkę i jedną kwotę; nie potrzeba
rozbicia na pozycje. Wszystko, co nie jest naliczane na transakcji (CIT, podatek od
nieruchomości, koncesje, domiary), żyje w `ChargeRegistry` M8 i nigdy nie dotyka dziennika
transakcji. Dzięki temu M5 nie migruje dziennika, a M8 nie wciska siedmiu danin w jedno pole.

**Rozszerzenia, o które M8 prosi M5** (dopisanie wariantów, bez zmiany istniejących):
`AccountOwner::City`, `TxKind::TaxPayment { charge: TaxChargeId }`,
`TxKind::PublicSpend { category: SpendCategory }`, oraz konta w `LedgerAccount`:
`VatPayable`, `PitPayable`, `CitPayable`, `PropertyTaxPayable`, `ExcisePayable`
(pasywa — `TaxExpense` już istnieje jako hak).

Kontrakt księgowania:

| Danina | Moment naliczenia | Zapis u płatnika | Rozliczenie |
|---|---|---|---|
| VAT | każda transakcja detaliczna (M5) | przychód = brutto − VAT; `Cr VatPayable` | 20. dnia następnego miesiąca |
| PIT | każda wypłata (M7, payroll) | `Dr WageExpense(brutto)`, `Cr Cash(netto)`, `Cr PitPayable` | 20. dnia następnego miesiąca |
| CIT | domknięcie roku obrotowego firmy | `Dr TaxExpense`, `Cr CitPayable` | do końca marca |
| Nieruchomości | 1. dnia miesiąca, właściciel parceli | `Dr TaxExpense`, `Cr PropertyTaxPayable` | 15. dnia miesiąca |
| Akcyza | opuszczenie zakładu produkcyjnego / odprawa importowa | `Dr Inventory` (wchodzi w koszt towaru!), `Cr ExcisePayable` | miesięcznie |
| Cło | odprawa w węźle importowym (M6) | `Dr Inventory`, `Cr Cash` | natychmiast |
| Koncesja | rozpoczęcie roku licencyjnego | `Dr TaxExpense`, `Cr Cash` | natychmiast |

Akcyza i cło wchodzą **w koszt zapasu**, nie w koszt okresu — dlatego podnoszą cenę
emergentnie (przez politykę marżową firmy z M7), a nie przez zadanie ceny.

**Rozliczenie i wpływ do budżetu — jeden system, jedno miejsce:**

```rust
/// EveryDay. Jedyne miejsce, w którym pieniądz przechodzi od płatnika do miasta.
/// Iteracja po TaxChargeId rosnąco; brak środków → ChargeState::Overdue, nie panika.
fn sys_settle_tax_charges(
    charges: &mut ChargeRegistry,
    budget:  &mut CityBudget,
    books:   &mut Books,                        // M5 — Books::transfer(payer_acct, city_acct, ..)
    tick: Tick,
);
```

Niezmiennik (test T1): dla każdej pary `(kind, period)`
`Σ Assessed = Σ Settled + Σ Overdue + Σ Abated`, oraz
`Σ Settled == Δ CityBudget.revenue_ytd[kind]`, tolerancja **0 groszy**.
