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

## Zmiany wpisane po M5d

Rzeczy, które w trakcie pracy okazały się inne, niż zapisano wyżej. Gwiazdka = zmiana
zakresu albo kryterium. Prefiks `Y-n`, bo `V` należy do M5b, a `W` do M5c.

| # | Korekta | Dlaczego |
|---|---|---|
| Y-1 ★ | **`budget_ref` dzieli kopertę przez liczbę zakupów w kategorii, a nie przez liczbę wyjść po zakupy.** `expected_purchases(cat) = towary w kategorii × 30 / purchase_days`, czyli 63 dla `Food` przy osiemnastotowarowym katalogu M5 | To jest wykonanie ostrzeżenia z korekty ★ wpisanej tu po M5b i zarazem jedyna liczba w tej podfazie, której rząd wielkości **decyduje o działaniu rynku** (`V-9`). Gospodarstwo kupuje każdy towar kategorii osobno, więc jedno wyjście po zakupy to jedna pozycja, nie koszyk: dzielenie przez 7,5 wyjścia dałoby dla żywności 15 500 gr wobec ręcznie strojonej stałej 1 500 gr z `choice.ron` — dziesięciokrotnie za dużo, człon ceny spłaszczony do zera i rynek zamieniony w losowanie. Zmierzone po podmianie: żywność 1 850 gr, higiena 961 gr wobec 900 gr, leki 1 730 gr wobec 2 500 gr. Test `mianownik_ceny_zostaje_w_rzedzie_wielkosci_stalej_z_choice_ron` pilnuje tego czterokrotnym marginesem w obie strony |
| Y-2 ★ | **`HouseholdBudget.loans: Vec<LoanId>` zwęziło się do `loan: Option<LoanId>`** | Gospodarstwo ma w M5 jeden produkt kredytowy, a drugi kredyt pod to samo zabezpieczenie to refinansowanie, czyli mechanika M7. `Vec` per gospodarstwo znaczy alokację na stercie razy 11 tys. gospodarstw metropolii, żeby przechować zero albo jeden element — i to w strukturze, która wchodzi do hasha i do zapisu gry. `Vec` wraca razem z kredytem inwestycyjnym M7 |
| Y-3 ★ | **`assess_credit` nie dostaje `&Books` ani `&CreditHistory`.** Sygnatura to `(&LoanApplication, &BankParams, BaseRate, seed, Tick)`, a wszystko, co ocena czyta z ksiąg i z historii, wchodzi jako trzy pola wniosku: `existing_service`, `arrears_months`, `ebitda_12m` | Ta sama doktryna, którą `kernel` ma od M5c: **rozwiązanie encji na liczby robi wołający**. `CreditHistory` jako typ nigdy nie powstał i nie powinien: „zaległości z ostatnich 24 miesięcy" to jeden `u8`, a nie struktura. Zysk jest ten sam co przy rdzeniu — ocena zdolności daje się przetestować bez budowania świata |
| Y-4 ★ | **`cpi(stats: &TransactionStats, …)` nie powstało; powstał `CpiTracker`** z własnym pierścieniem dobowym per `GoodId`, historią miesięczną i stopą bazową | `TransactionStats` nie istnieje i nie mógł: `TxJournal` jest pierścieniem o 4096 wpisach i starcza na dobę ruchu jednego miasta, a okno CPI ma 30 dób (korekta wpisana tu po M5b). Konsekwencja, której plan nie zapisywał: **koszyk jest stanem i wchodzi do hasha** razem ze stopą bazową — stopa wpływa na oprocentowanie, oprocentowanie na ratę, rata na saldo gospodarstwa, więc pomiar zmienia świat |
| Y-5 ★ | **Bank centralny nie rusza stopy, dopóki nie ma dwunastu miesięcy historii.** `CpiTracker::yoy_bp()` zwraca `Option<i32>`, a `update_base_rate` woła się tylko dla `Some` | Zero czytałoby się jak „inflacja 0 %", czyli **poniżej celu 2,5 %**, i reguła Taylora zaczęłaby obniżać stopę, zanim cokolwiek zmierzyła. Zmierzone przed poprawką: stopa schodziła z 500 do 450 bp już w pierwszym domknięciu miesiąca świata. To jest ten sam gatunek błędu co „kryterium spełnione tożsamościowo" — reguła działała, tylko na danych, których nie było |
| Y-6 ★ | **Kolejność miesiąca gospodarstwa jest kontraktem: rata → wniosek kredytowy → koszty stałe → oszczędności.** Kredyt obrotowy zakładu wchodzi analogicznie do `Market::close_month`, **przed** `ledger::close_period` | Rata idzie pierwsza, bo bank jest wierzycielem uprzywilejowanym, a jej niezapłacenie ma inny skutek niż niezapłacenie czynszu (psuje scoring na 24 miesiące). Wniosek **przed** niedopłatą, bo kryterium WP8 wymaga ścieżki „debet → wniosek → odmowa → zaległość", a nie zaległości, której nikt nie próbował uniknąć. Oszczędności na końcu, bo odkłada się to, co zostaje. Po stronie zakładu powód jest księgowy i był zapisany w korekcie po M5c: odsetki po `close_period` wpadłyby do następnego okresu |
| Y-7 | **Kredyt gospodarstwa przechodzi przez konto banku w obie strony.** Uruchomienie: `Books::create_credit(konto banku)` → `household_receive` → pole komponentu. Spłata: `household_pay(konto banku)` → `destroy_credit(konto banku)`, a odsetki zostają na koncie banku jako zwykły przelew | `create_credit`/`destroy_credit` działają na kontach, a saldo gospodarstwa kontem **nie jest** (`U-17`). To jedno dodatkowe wywołanie po każdej stronie, nie inna mechanika — dokładnie tak, jak zapowiadała korekta wpisana tu po M5b |
| Y-8 ★ | **Niezmiennik pieniądza w scenariuszu zmienił kształt: suma świata ma od teraz prawo rosnąć.** Bramką jest różnica **po odjęciu kreacji kredytowej netto**, a nie sama różnica | Kredyt tworzy pieniądz i to jest cały mechanizm WP9 — niezmiennik „suma == const" byłby po nim fałszywy. `Books::check_conservation` (P1) nie zmienia się ani o linię, bo `MoneySupplyLedger.total()` liczył kreację od M5a. Scenariusz wypisuje obie liczby osobno: ile przybyło kredytu i ile zostało poza nim |
| Y-9 ★ | **Scenariusz planuje pierwszy budżet w ticku 0, obok `pay_incomes`** | Bez tego gospodarstwa przez trzydzieści dób nie mają kopert, mianownik członu ceny stoi na stałej z `choice.ron`, a przebieg krótszy niż miesiąc nie pokazuje **ani jednej** decyzji budżetowej — czyli mierzy M5b, a nie M5d. Zmierzone: 8 dób przed poprawką dawało 0 wpisów w oknie decyzji, po poprawce 11 407 gospodarstw, 3 481 wniosków kredytowych i 1 724 przyznane |
| Y-10 | **`GoodTable` dostaje `id_of_key`** (i `iter`), a `data/economy/cpi.ron` wskazuje towary kluczem tekstowym | Klucz, a nie indeks, bo `GoodId` nadaje katalog M2 przy ładowaniu, a w zapisie gry trzyma się klucz (00 §5). Złączenie klucza z identyfikatorem robi wyłącznie `GoodTable::build` i tylko on może je wystawić — trzecia kopia tej mapy w teście i w koszyku rozjechałaby się przy pierwszej zmianie katalogu |
| Y-11 | **`Shop` dostaje `loan: Option<LoanId>` i `opened: Tick`** | Kredyt obrotowy jest stanem zakładu (rata wchodzi do wyniku miesiąca), a staż działalności jest wejściem oceny zdolności. Oba wchodzą do hasha zakładu |
| Y-12 ★ | **Lista `wszystkie()` w `engine/ui` nie była kompletna i bramka wyjaśnialności nie była bramką.** Obejmowała 29 wariantów `DecisionReason`, pomijając `Repricing` (303, dopisany w M5c) oraz `ModeCompared`, `NoParkingAtDestination` i `LeftBehind` (205–207, dopisane w M4c/M4d) — wszystkie cztery miały już ramiona w `describe`. Po uzupełnieniu jest 36. Przy okazji `kazdy_slownik_domenowy_ma_nazwy` nie iterował po `PriceDriver::ALL` | „Bramka, która nigdy nie świeci na czerwono, nie jest bramką" (§7.4 dokumentu fazy). Test sprawdzał, że **wypisane** warianty mają tekst w obu językach — a nie, że wypisane są wszystkie. Nowy wariant fazy przechodził go milcząco, bo nikt go do listy nie dopisał; dokładnie tak przeszły cztery poprzednie |
| Y-13 | **Kopia budżetu zeszła z gorącej ścieżki.** `buyer_state` i próg odłożenia czytają `&HouseholdBudget` z wektora, nie kopię | `HouseholdBudget` ma ćwierć kilobajta (osiem kopert po 24 B plus koszty stałe), a `buyer_state` woła się raz na kandydata, czyli 3–15 razy na decyzję zakupową i rzędu miliona razy na dobę metropolii. To nie jest optymalizacja przedwczesna, tylko cofnięcie kosztu, który wprowadziła ta podfaza |

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

---

## Zmiany wpisane po M5c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5c —
podfaza nie jest tu przeprojektowywana.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Miesięczna pętla zakładu już istnieje: `Market::close_month`.** Odsetki i raty kredytu obrotowego (WP9) wchodzą **do niej**, między koszty stałe a domknięcie okresu, a nie jako osobny przelot po sklepach | Kolejność w miesiącu jest kontraktem księgowym: rata musi się zaksięgować **przed** `ledger::close_period`, inaczej odsetki wpadną do następnego okresu i RZiS miesiąca przestanie się zgadzać z przepływami. Konta `LoansShort`, `LoansLong` i `InterestExpense` istnieją w planie kont od M5c i są w M5c zawsze zerowe — WP9 dokłada wołającego, nie konto |
| ★ | **Każdy przelew zakładu musi mieć swój zapis w księdze.** `LedgerAccount::BankCurrent` jest lustrem salda rachunku w `Books` i test WP7 sprawdza tę równość co do grosza | Uruchomienie kredytu obrotowego rusza **trzy** rzeczy naraz: `Books::create_credit` (podaż pieniądza), saldo rachunku i księgę zakładu. Pominięcie trzeciej nie zaczerwieni P1 — zaczerwieni test M5c, i to jest jedyny moment, w którym rozjazd wyjdzie blisko przyczyny |
| ★ | **CPI ma liczyć z cen transakcyjnych, a od M5c ceny naprawdę się ruszają.** W M5b `Offer.unit_price` stała w miejscu przez cały przebieg; teraz zmienia się dobowo (2,7 tys. przecen na 8 dób miasta 28 tys. mieszkańców) | Koszyk liczony z `retail.ron` × `markup_bp` dałby w M5b tę samą liczbę co koszyk z transakcji i różnicy nie dałoby się zauważyć. Od M5c dałoby się — i byłaby to inflacja liczona z cennika, a nie z tego, co ludzie płacą (§6.1). Agregat narastający per `GoodId` per doba, o którym mowa wyżej, musi więc brać kwotę z `TxKind::RetailSale`, nie z oferty |
| | **`data/economy/shop.ron` jest plikiem, który stroi balansator razem z `choice.ron`.** `min_margin_bp` z tego pliku jest dolnym ogranicznikiem ceny, czyli mechanizmem, którego pilnuje bramka G3 | WP10 („podwojenie akcji kredytowej przy stałej podaży dóbr → CPI rośnie") działa przez człon `adj_stock`: więcej kredytu → większe koperty → szybciej schodzący zapas → wyższe ceny. Górny ogranicznik `max_margin_bp` jest sufitem tej ścieżki i przy zbyt niskiej wartości scenariusz WP10 przejdzie tożsamościowo zerem |
| | **Poślizg ceny (§5.5) zaczął być widoczny dopiero na przebiegu dłuższym niż miesiąc**: 0 przypadków przy 8 dobach, 948 przy 40 | Mechanizm jest sprawny, ale zapełnia się wolno — `PlannedPurchase` musi trafić na dobę, w której cena akurat drgnęła. Dla WP8 znaczy to tyle, że kalibracji progów kopert nie da się ocenić na przebiegu tygodniowym |
