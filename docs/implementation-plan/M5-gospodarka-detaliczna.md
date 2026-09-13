# M5 — Gospodarka detaliczna

Status: plan fazy. Nadrzędny: `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, LOD, wyjaśnialność).
Właściciel crate'ów: `sim/economy`, `tools/balansator`. Rozszerza: `sim/agents`, `sim/firms` (szkic sklepu), `engine/ui`.
Źródło: `PRD_Magnat.md` §6 (całość), §5.3–5.7, §7.1, §7.3, §14.1–14.3, §16.5, §17.5, §19, §20.1.

---

## 1. Cel fazy i artefakt końcowy

M5 to **pierwszy vertical slice** — pierwsza faza, w której istnieje pętla gracza. Po zakończeniu M5 da się:

1. Uruchomić miasto (z M1–M4) z działającą gospodarką detaliczną: mieszkańcy mają pieniądze, budżety
   gospodarstw domowych, potrzeby i chodzą do **konkretnych sklepów z konkretnym magazynem**,
   płacąc **konkretną cenę z konkretnej oferty**. Sklep, w którym nie ma chleba, nie sprzedaje chleba.
2. Otworzyć sklep jako gracz, wybrać asortyment, ustawić cenę ręcznie lub delegować politykę cenową,
   i **obserwować skutek**: kto przyszedł, skąd, dlaczego — i kto nie przyszedł i dlaczego nie.
3. Zobaczyć pełną księgowość tego sklepu: RZiS, bilans, przepływy pieniężne, wycenę zapasów.
4. Zobaczyć konkurencję AI reagującą na ruch gracza — z opóźnieniem 1–7 dni, nie natychmiast.
5. Uruchomić `tools/balansator` na N miastach i dostać raport rozkładu cen, inflacji, marż, bankructw
   oraz **werdykt bramek CI** (stabilność, reaktywność szoku, zachowanie pieniądza, determinizm).

Artefakt końcowy (demo „do pokazania"): headless przebieg 2 lat gry × 32 seedy z zielonym raportem
balansatora **plus** sesja w GUI: gracz otwiera sklep osiedlowy przy ulicy X, podnosi cenę mleka o 15%,
po 3 dniach widzi w panelu spadek liczby klientów i listę utraconych sprzedaży z uzasadnieniem
„cena o 12% wyższa niż w *Dobry Koszyk*, 700 m dalej".

Wprost: **nie istnieje globalna cena rynkowa** (§6.1). Wszystko, co UI pokazuje jako „cenę mleka",
jest agregatem po ofertach — i musi być tak zaimplementowane, nie zasymulowane skrótem.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zakres w M5 | PRD |
|---|---|---|
| Oferta jako jedyny nośnik ceny | `Offer` jako encja ECS, indeks przestrzenny per kategoria, agregacja do UI | §6.1, §17.5 |
| Rynek detaliczny B2C | dopasowanie kupujący↔oferta, rozliczenie, kolejność deterministyczna | §6.2 pkt 1 |
| Decyzja zakupowa | funkcja użyteczności, softmax, próg odłożenia zakupu, wagi z osobowości i statusu | §6.4, §5.4 |
| Polityki cenowe AI | marża, korekta wg magazynu, obserwacja konkurencji z opóźnieniem 1–7 dni, eksperymenty cenowe, przeceny psującego się | §6.3 |
| Polityki delegowane gracza | te same typy co AI, ustawiane z UI | §6.3, §14.6 (podstawa) |
| Sklep | magazyn zaplecza, półka, asortyment, rotacja, braki, pojemność z budynku | §7.1, §7.3 |
| Pieniądz i konta | `Account`, podwójny zapis, jeden punkt przelewu, ewidencja podaży pieniądza | §6.5 |
| Budżety GD | dochody, wydatki stałe, koperty wydatków zmiennych, oszczędności/dług | §5.2 |
| Banki i kredyt (podstawowy) | depozyty, kredyt konsumpcyjny, kredyt obrotowy, ocena zdolności, harmonogram spłat | §6.5 |
| Stopa bazowa i inflacja | CPI z koszyka miejskiego, bank centralny jako abstrakcja, inflacja **emergentna** | §6.5 |
| Księgowość sklepu | plan kont, dziennik, RZiS, bilans, przepływy, wycena zapasów | §6.9 |
| Panel sklepu w UI | półki, klienci („skąd/kto/dlaczego"), utracone sprzedaże, konkurencja w zasięgu | §14.3, §14.1 |
| Balansator | N miast, rozkłady cen, wykrywanie spirali, bramki CI | §16.5, §20.1, §20.4 |

### Nie wchodzi (i kto to robi)

| Pominięte | Faza | Co M5 zostawia w zamian |
|---|---|---|
| Rynek B2B (spot, kontrakty), import | M6 | `trait Wholesale` + `ExternalSupplier` — jawny punkt wymiany (§5.7 tego dokumentu) |
| Partie towaru (`BatchId`), receptury, produkcja | M6 | `StockLine` z jedną datą ważności; wycena średnią ważoną zamiast FIFO |
| Zaopatrzenie sklepu od realnego dostawcy | M6 | zakup u „zewnętrznego dostawcy" o cenie z `data/goods/` |
| Rynek pracy, pensje emergentne | M7 | `HouseholdBudget.income_monthly` z abstrakcyjnego pracodawcy (konto `RestOfWorld`) |
| Firmy AI: zakładanie, bankructwo, osobowości pełne | M7 | sklepy AI zasiedlone przez generator M2; osobowość cenowa = 4 parametry z danych |
| Podatki, VAT, akcyza | M8 | `trait TaxEngine` + impl `NoTax`; pole `Transaction.tax` istnieje i jest zawsze 0 |
| Giełda, ubezpieczenia, obligacje, leasing, faktoring | M10 / M7 | brak |
| Marka, reklama, afinitet do marki | M10 | człon `w_marka · afinitet` istnieje w funkcji użyteczności, `afinitet ≡ 0.0` |
| Plotka jako kanał informacji | M10 | znajomość sklepu = odwiedzony ∪ w zasięgu domu/pracy/trasy (§5.7 uproszczone) |
| Pełne panele gracza, edytor reguł | M9 | panel sklepu + wybór polityki z listy (bez edytora reguł) |
| Nieruchomości, czynsze emergentne | M7/M10 | czynsz sklepu = stała z parcelą, wydatek stały GD = stała |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M5 |
|---|---|
| §6.1 | `Offer` jako jedyny nośnik ceny; brak zmiennej „cena rynkowa" w kodzie — test statyczny |
| §6.2 pkt 1 | rynek detaliczny w całości |
| §6.3 | polityki cenowe AI i gracza, bez surowców pierwotnych (M6) |
| §6.4 | funkcja użyteczności, softmax, próg, elastyczność jako własność emergentna |
| §6.5 | pieniądz, banki, depozyty, kredyt konsumpcyjny i obrotowy, stopa bazowa, inflacja emergentna. Bez giełdy i ubezpieczeń |
| §6.9 | księgowość sklepu w całości (bez należności z terminami — te mają sens dopiero w B2B, M6) |
| §5.2 | budżet GD: dochody − wydatki stałe − zmienne → oszczędności lub dług |
| §5.3 | koszyk dóbr per potrzeba i substytuty niższego rzędu (mapowanie z `data/goods/`) |
| §5.4 | status jako źródło wag `w_status`, `w_jakość`, `w_cena` |
| §5.5 | zadanie „zakupy" z planera dnia wywołuje `purchase_decision` |
| §5.7 | znajomość sklepu jako filtr kandydatów (bez plotki) |
| §7.1, §7.3 | sklep = zakład firmy: budynek, pojemność magazynu i półki, media jako wydatek stały |
| §14.1 | `DecisionReason` dla każdej decyzji zakupowej, cenowej i kredytowej |
| §14.3 | panel „Sklep" |
| §16.5 | balansator |
| §17.5 | indeks przestrzenny ofert, 3–15 kandydatów, cache agregatów per dzielnica |
| §20.1 | bramki balansatora (sekcja 7 tego dokumentu) |
| §20.4 | balansator w CI **od tej fazy** jako główna mitygacja ryzyka spirali |

---

## 4. Pakiety robocze

Kolejność ma jedną twardą zasadę: **pieniądz przed wszystkim innym**. Test zachowania pieniądza musi
być zielony zanim powstanie pierwsza transakcja detaliczna, bo później nie da się go już wprowadzić
bez przepisywania.

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP1 | Pieniądz, konta, podwójny zapis | M0 | M |
| WP2 | Oferta i indeks przestrzenny | M2 (`engine/spatial`), WP1 | M |
| WP3 | Sklep: magazyn, półka, asortyment, zewnętrzny dostawca | WP1, WP2 | L |
| WP4 | Funkcja użyteczności i wybór oferty | M3, M4, WP2 | L |
| WP5 | Rozliczanie transakcji | WP1, WP3, WP4 | M |
| WP6 | Polityki cenowe AI | WP2, WP3, WP5 | L |
| WP7 | Księgowość sklepu | WP1, WP3, WP5 | L |
| WP8 | Budżety gospodarstw domowych | WP1, M3 | M |
| WP9 | Banki i kredyt | WP1, WP7, WP8 | M |
| WP10 | CPI, stopa bazowa, inflacja emergentna | WP5, WP9 | S |
| WP11 | Polityki cenowe delegowane przez gracza | WP6 | S |
| WP12 | Panel sklepu w UI | WP3–WP7, M3 (`engine/ui`) | M |
| WP13 | Balansator i bramki CI | WP1–WP10 | L |
| WP14 | Testy własnościowe, determinizm, benchmarki | równolegle od WP1 | M |

### WP1 — Pieniądz, konta, podwójny zapis
Jedno wejście do zmiany stanu pieniężnego: `Books::transfer`. Brak innego sposobu na zmianę salda
w całym `sim/economy` — wymuszone prywatnością pola `balance` i testem statycznym (grep w CI).
Świat zewnętrzny (`RestOfWorld`) jest **normalnym kontem w systemie**, nie ujściem — dzięki temu
zakup u zewnętrznego dostawcy i wypłata od abstrakcyjnego pracodawcy zachowują sumę pieniądza
bez osobnej ewidencji ujść.
Kryterium ukończenia: test własnościowy „suma pieniądza = emisja − destrukcja" zielony na losowym
strumieniu 10⁶ przelewów, tolerancja **0 groszy**; próba przelewu ujemnej kwoty i przekroczenia
limitu debetu zwraca błąd, nie panikuje.

### WP2 — Oferta i indeks przestrzenny
`Offer` jako encja ECS (zgodnie z `OfferId(Entity)` w dok. 00 §2 i §17.2 PRD). Indeks: grid komórek
z `engine/spatial`, osobna warstwa per kategoria potrzeby. Zapytanie zwraca **identyfikatory**,
cena czytana na żywo z komponentu — zmiana ceny nie wymaga przebudowy indeksu.
Kryterium: `query_offers` dla 5 tys. ofert i promienia 3 km zwraca wynik w < 20 µs (criterion),
kolejność wyniku identyczna przy dwóch przebiegach i niezależna od kolejności wstawiania.

### WP3 — Sklep: magazyn, półka, asortyment, zewnętrzny dostawca
Zaplecze (`ShopInventory`) i półka (`Shelf`) to **dwa różne stany**; oferta odzwierciedla wyłącznie
półkę. Uzupełnianie półki z zaplecza co godzinę, zamówienie u zewnętrznego dostawcy wg polityki
zapasu (punkt ponownego zamówienia + czas dostawy z danych).
Kryterium: test własnościowy „brak ujemnych stanów" i „sklep nie sprzedaje towaru, którego nie ma"
zielony; scenariusz zerwania dostaw → półka pustoszeje w tempie sprzedaży, oferta znika z kandydatów,
ale **sklep pozostaje widoczny w inspekcji z powodem „brak towaru"**.

### WP4 — Funkcja użyteczności i wybór oferty
Pełna treść w sekcji 5.4. Kryterium: dwa przebiegi tego samego seeda dają identyczny ciąg wyborów;
test wrażliwości — podniesienie ceny w jednym sklepie o 10% przesuwa udział rynkowy monotonicznie
w dół dla każdej z 20 losowych populacji; 100% decyzji ma zapisany `DecisionReason`.

### WP5 — Rozliczanie transakcji
Rozdzielenie na fazę decyzji (równoległą, bez mutacji) i fazę rozliczenia (sekwencyjną,
deterministyczną). Rozwiązuje wyścig o ostatnią sztukę bez blokad.
Kryterium: 10 tys. agentów kierujących się do sklepu z 10 sztukami na półce → dokładnie 10 transakcji,
9990 zdarzeń `Stockout` z przeplanowaniem, suma pieniądza bez zmian.

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

### WP11 — Polityki cenowe delegowane przez gracza
**Ten sam** typ `PricePolicy` co AI (§6.3 mówi wprost: „z tym samym zestawem narzędzi co AI").
Zero osobnej ścieżki kodu. UI: wybór wariantu + parametry + podgląd „co by się stało z ceną dziś".
Kryterium: polityka „−2% względem najtańszego konkurenta w promieniu 3 km" ustawiona przez gracza
i przez AI daje identyczną cenę przy identycznym stanie.

### WP12 — Panel sklepu w UI
Trzy zakładki: Półki / Klienci / Konkurencja. Dane wyłącznie przez `ShopPanelSnapshot` (podwójnie
buforowany, bez dostępu UI do ECS symulacji).
Kryterium: pytanie „dlaczego Anna nie kupiła u mnie?" ma odpowiedź w panelu dla ≥95% mieszkańców,
którzy w ostatnich 7 dniach byli kandydatami i nie kupili.

### WP13 — Balansator i bramki CI
Sekcja 7.4. Kryterium: bramki działają w CI na PR (macierz zredukowana) i nocnie (pełna);
sztucznie wprowadzony błąd (usunięcie dolnego ogranicznika ceny) **czerwieni** bramkę deflacji.

### WP14 — Testy własnościowe, determinizm, benchmarki
Rośnie od WP1, nie na końcu. Dopisanie komponentów `sim/economy` do funkcji haszującej stan ECS
jest warunkiem Definition of Done fazy (dok. 00 §3 pkt 6).

---

## 5. Projekt techniczny

### 5.1 Pieniądz, konta, transakcje

```rust
// --- konta -------------------------------------------------------------
pub struct AccountId(pub u32);

pub enum AccountOwner {
    Household(HouseholdId),
    Citizen(CitizenId),          // gotówka w portfelu mieszkańca
    Firm(FirmId),
    Bank(FirmId),                // konto własne banku (rezerwy + kapitał)
    City,                        // budżet miasta — wariant zamówiony przez M8, pusty w M5
    RestOfWorld,                 // zewnętrzny dostawca, abstrakcyjny pracodawca, import
    CentralBank,
}

pub enum AccountKind { Cash, Current, Savings, LoanLiability }

pub struct Account {
    pub owner: AccountOwner,
    pub kind: AccountKind,
    pub bank: Option<FirmId>,        // None dla Cash
    balance: Money,                  // PRYWATNE — zmiana wyłącznie przez Books::transfer
    pub overdraft_limit: Money,      // >= 0; domyślnie 0
}

// --- księga przelewów --------------------------------------------------
pub struct Books {
    accounts: Vec<Account>,                 // indeksowane AccountId
    pub supply: MoneySupplyLedger,
    journal: TxJournal,                     // pierścień + zrzut na dysk
}

pub struct MoneySupplyLedger {
    pub endowment: Money,          // pieniądz wykreowany przy inicjalizacji świata
    pub credit_created: Money,     // suma kapitałów udzielonych kredytów
    pub credit_repaid: Money,      // suma spłaconych kapitałów
    /// Kanał zamówiony przez M7: kapitał sieci zewnętrznej wchodzącej do miasta.
    /// To NIE jest przepływ z RestOfWorld — to emisja spoza systemu, więc musi mieć
    /// własną pozycję, inaczej test P1 pęknie dopiero w M7, daleko od przyczyny.
    /// W M5 obie pozycje są zawsze 0 (żaden kanał ich nie rusza), ale istnieją w strukturze,
    /// w niezmienniku i w teście — po to, żeby M7 dopisał tylko wywołanie.
    pub external_capital_in: Money,   // wejście kapitału inwestora zewnętrznego
    pub external_capital_out: Money,  // repatriacja zysku / wyjście z rynku
}
// Niezmiennik (test P1, tolerancja 0 groszy):
// Σ balance == endowment
//            + credit_created     − credit_repaid
//            + external_capital_in − external_capital_out

impl Books {
    /// Jedyne wejście dla kanału M7. W M5 nie wołane przez żaden system —
    /// pokryte wyłącznie testem własnościowym.
    pub fn inject_external_capital(&mut self, to: AccountId, amount: Money, src: ExternalInvestorId) -> TxId;
    pub fn repatriate_external_capital(&mut self, from: AccountId, amount: Money, src: ExternalInvestorId)
        -> Result<TxId, TxError>;
}

impl Books {
    /// JEDYNY sposób zmiany salda w całym sim/economy.
    pub fn transfer(
        &mut self,
        from: AccountId,
        to: AccountId,
        amount: Money,              // musi być > 0
        memo: TxMemo,
        t: Tick,
    ) -> Result<TxId, TxError>;     // TxError: NonPositive | InsufficientFunds | SameAccount | Unknown

    /// Kreacja/destrukcja pieniądza — WYŁĄCZNIE bank przy uruchomieniu i spłacie kredytu.
    pub(crate) fn create_credit(&mut self, to: AccountId, amount: Money, loan: LoanId) -> TxId;
    pub(crate) fn destroy_credit(&mut self, from: AccountId, amount: Money, loan: LoanId) -> Result<TxId, TxError>;
}
```

```rust
pub struct Transaction {
    pub id: TxId,
    pub tick: Tick,
    pub kind: TxKind,
    pub debit: AccountId,
    pub credit: AccountId,
    pub net: Money,              // kwota bez VAT
    pub tax: Money,              // WYŁĄCZNIE VAT (rozstrzygnięcie z M8). W M5 zawsze Money(0)
    pub gross: Money,            // net + tax; to jest kwota realnie przelana
    pub reason: DecisionReason,  // enum z engine/core (K-12)
}

pub enum TxKind {
    RetailSale   { offer: OfferId, good: GoodId, qty: Qty, buyer: HouseholdId },
    WholesalePurchase { good: GoodId, qty: Qty, supplier: SupplierRef },   // SupplierRef::External w M5
    Wage         { site: SiteId },        // w M5 z konta RestOfWorld do GD
    Rent         { site: SiteId },
    Utility      { site: SiteId, kind: UtilityKind },   // UtilityKind mieszka w engine/core (K-8)
    LoanDraw     { loan: LoanId },
    LoanPayment  { loan: LoanId, principal: Money, interest: Money },
    Deposit      { from_cash: bool },
    Withdrawal,
    // --- warianty zamówione przez M8; w M5 nieużywane, ale obecne w enumie i w dzienniku ---
    TaxPayment   { charge: ChargeKind },  // ChargeKind z ChargeRegistry (własność M8)
    PublicSpend  { program: ProgramId },  // wydatek miasta
    ExternalCapital { investor: ExternalInvestorId, inflow: bool },  // kanał M7 (5.1)
}
```

Reguły arytmetyczne (dok. 00 §2): `checked_add`/`checked_mul`, `div_round_half_up`, podział kwoty
między N stron sumuje się do oryginału (reszta do pierwszego wg porządku `AccountId`).

**Rozgraniczenie z M8 (rozstrzygnięcie wiążące).** `Transaction.tax` jest **tylko dla VAT-u** —
bo VAT jest nierozłączny od pojedynczej transakcji detalicznej i musi się z niej wyodrębnić
w momencie rozliczenia. Wszystkie pozostałe daniny (CIT, PIT, podatek od nieruchomości, akcyza,
cła, opłaty koncesyjne) idą do `ChargeRegistry` po stronie M8 i trafiają do dziennika jako
`TxKind::TaxPayment`. **M5 nie przewiduje migracji dziennika w M8** — decyzja otwarta nr 8
jest tym samym zamknięta.

### 5.2 Oferta i indeks przestrzenny (§6.1, §17.5)

```rust
/// Encja ECS. Jedyny nośnik ceny w grze — nie istnieje żadna globalna cena towaru.
pub struct Offer {
    pub seller: FirmId,
    pub site: SiteId,            // sklep = nośnik lokalizacji i dostępności
    pub good: GoodId,
    pub unit_price: Money,       // za 1 sztukę (Qty(1000)); ZAWSZE kwota płacona przez kupującego
    pub price_basis: PriceBasis, // K-7: w M5 zawsze GrossRetail
    pub available: Qty,          // == Shelf line qty; 0 oznacza „znany sklep, ale brak towaru"
    pub quality: Q,
    pub category: CategoryId,    // z data/goods — warstwa indeksu
    pub since: Tick,
    pub price_rev: u32,          // licznik zmian ceny — do obserwacji konkurencji z opóźnieniem
}

/// K-7: cena w ofercie to kwota, którą płaci kupujący — brutto w detalu, netto w hurcie.
/// VAT wyodrębnia się dopiero przy rozliczeniu (net/tax/gross w Transaction), nie w ofercie.
pub enum PriceBasis { GrossRetail, NetB2B }

pub struct OfferIndex {
    /// warstwa per kategoria → komórka gridu dzielnicowego → posortowany Vec<OfferId>
    layers: Vec<BTreeMap<CellId, Vec<OfferId>>>,
    dirty: BTreeSet<(CategoryId, CellId)>,
    /// cache agregatów per dzielnica per towar (§17.5), unieważniany co tick cenowy
    district_stats: BTreeMap<(DistrictId, GoodId), PriceStats>,
}

pub struct PriceStats { pub min: Money, pub p50: Money, pub max: Money, pub offers: u32, pub qty: Qty }

pub fn query_offers(
    index: &OfferIndex,
    category: CategoryId,
    origin: WorldPos,
    radius_m: u32,
    out: &mut Vec<OfferId>,      // bufor wielokrotnego użytku — zero alokacji w gorącej ścieżce
);
```

Kolejność wyniku: rosnąco po `(CellId, OfferId)` — deterministyczna i niezależna od kolejności
wstawiania. Zakaz iteracji po `HashMap` (dok. 00 §3 pkt 2).

### 5.3 Sklep: magazyn, półka, asortyment

```rust
pub struct ShopInventory {
    pub site: SiteId,
    pub backroom: BTreeMap<GoodId, StockLine>,
    pub capacity_m3: i64,                 // z budynku (§7.3)
    pub reorder: BTreeMap<GoodId, ReorderPolicy>,
}

pub struct StockLine {
    pub qty: Qty,                         // NIGDY < 0 — niezmiennik testowany
    pub cost_total: Money,                // łączny koszt nabycia tej linii → wycena średnią ważoną
    pub expires: Option<SimMinute>,       // M5: jedna data na linię. M6: zastąpione przez BatchId
}

pub struct Shelf {
    pub site: SiteId,
    pub slots: u16,                       // z pojemności budynku
    pub lines: Vec<ShelfLine>,            // len <= slots, posortowane po GoodId
}

pub struct ShelfLine {
    pub good: GoodId,
    pub qty: Qty,
    pub facings: u16,                     // ile miejsc na półce — limit ekspozycji
    pub offer: OfferId,
}

pub struct ReorderPolicy { pub point: Qty, pub target: Qty, pub lead_time_days: u8 }

pub enum AssortmentPolicy {
    Manual { goods: Vec<GoodId> },                    // gracz
    Auto   { max_lines: u16, min_margin_bp: i32 },    // AI: top-N wg marża × popyt w dzielnicy
}
```

Półka a zaplecze to dwa stany. `Offer.available` odzwierciedla **wyłącznie półkę** — towar
w zapleczu nie jest na sprzedaż. Przy wyczerpaniu półki oferta zostaje (z `available == 0`),
bo sklep ma pozostać widoczny w inspekcji jako „znany, ale bez towaru" — to jest odpowiedź
na pytanie z §14.1.

### 5.4 Funkcja użyteczności zakupu (§6.4) — pełna specyfikacja

#### Kandydaci (§17.5: 3–15, nie O(n·m))

```rust
pub struct Candidate {
    pub offer: OfferId,
    pub site: SiteId,
    pub good: GoodId,
    pub qty: Qty,                 // ile agent chce kupić
    pub price_total: Money,       // unit_price × qty
    pub travel: TravelCost,       // z M4: (SimMinute, Money)
    pub quality: Q,
    pub known: bool,
}

pub fn gather_candidates(
    ctx: &MarketCtx,
    buyer: CitizenId,
    need: NeedId,
    budget_ref: Money,
    out: &mut CandidateBuf,
) -> usize;
```

Algorytm (deterministyczny, bez alokacji):
1. `need → &[GoodId]` — koszyk substytutów z `data/goods/` (§5.3), posortowany po randze substytutu.
2. `query_offers` dla kategorii potrzeby, promień = `max_travel_m(mode, personality)` z M4.
3. Filtr twardy: `available >= qty` **i** `known(buyer, site)` (§5.7: odwiedzony ∪ komórka domu/pracy/trasy).
4. Wstępny ranking po całkowitoliczbowym `prescore = −(price_total + travel_money + travel_min·vot)`
   kwantyzowanym do `i32`; sortowanie po `(−prescore, SiteId, GoodId)` — remisy rozstrzygane po id.
5. Obcięcie do `K_MAX = 15`. Jeśli `< K_MIN = 3` → jednokrotne rozszerzenie promienia o jeden pierścień
   komórek. Jeśli nadal `0` → `DecisionReason::NoCandidates { cause }`.

#### Użyteczność

```rust
pub fn utility_of_offer(
    c: &Candidate,
    w: &UtilityWeights,
    st: &BuyerState,
    noise: f32,
) -> f32;
```

```
U = w_price   · f(price_total / budget_ref)
  + w_quality · (quality / 100)
  + w_brand   · brand_affinity                 // M5: ≡ 0.0, hook M10
  + w_dist    · f(travel_cost / budget_ref)
  + w_loyalty · loyalty(site)
  + w_status  · status_fit(quality, status)
  + w_novelty · novelty(site) · openness
  + noise
```

Konkretne postaci członów:

| Człon | Postać | Uzasadnienie |
|---|---|---|
| `f(x)` | `f(x) = −ln(1 + x)`, `x ≥ 0` | jedna funkcja dla ceny **i** odległości — obie wyrażone jako ułamek budżetu, więc porównywalne. `f(0)=0`, monotonicznie malejąca, malejąca wrażliwość przy dużym `x` (kto już wydał pół budżetu, nie rozróżnia drobnych różnic) |
| `g` (odległość) | `g ≡ f`, argumentem jest `travel_cost` | świadomie ta sama funkcja: PRD wymaga „kosztu dojazdu w czasie i pieniądzu" sprowadzonego do jednej wielkości |
| `travel_cost` | `travel_money + travel_min · vot`, gdzie `vot = dochód_netto_GD_miesięczny / (22·8·60) · vot_factor(status)` | wartość czasu wyprowadzona z dochodu, nie zgadnięta; bogatszy mieszkaniec realnie omija tani sklep na drugim końcu miasta |
| `budget_ref` | `envelopes[need] / max(1, oczekiwane_pozostałe_zakupy_w_miesiącu)` | mianownik ma definicję operacyjną z WP8, nie jest stałą |
| `loyalty(site)` | `(avg_rating(site) − 50)/50 · visits/(visits + 3)` ∈ ⟨−1, 1⟩ | z pamięci doświadczeń M3 (§5.1); skurcz `n/(n+3)` — jedna wizyta nie tworzy lojalności |
| `status_fit` | `1 − |tier(quality) − tier(status)| / 4`, tiery 0..4 | elita nie kupuje najtańszego, GD o niskim statusie nie kupuje luksusu — nawet gdy stać |
| `novelty(site)` | `1.0` jeśli `visits == 0`, inaczej `0.0` | jednorazowa premia za spróbowanie nowego sklepu — to jest mechanizm, dzięki któremu sklep gracza w ogóle ma pierwszego klienta (§5.7) |
| `openness` | `Personality.otwartość / 100` | |
| `noise` | `sigma · u`, `u ~ U(−1, 1)` | patrz niżej |

#### Wagi (§6.4: „wynikają z osobowości i statusu")

```rust
pub struct UtilityWeights { pub price: f32, pub quality: f32, pub brand: f32, pub dist: f32,
                            pub loyalty: f32, pub status: f32, pub novelty: f32 }

pub fn weights_for(p: &Personality, status: Q, need: NeedId, data: &EconomyData) -> UtilityWeights;
```

Konstrukcja: baza per potrzeba z `data/economy/weights.ron` (np. dla „głód" cena waży więcej niż
dla „status"), modulowana **multiplikatywnie** przez osobowość i status, potem normalizowana
tak, by `Σ|w| = 1` (żeby próg `U_threshold` miał wspólną skalę dla wszystkich potrzeb):

```
price   = base.price   · (0.5 + 1.0 · wrażliwość_cenowa/100) · (1.4 − 0.8 · status/100)
quality = base.quality · (0.6 + 0.8 · status/100)
dist    = base.dist    · (0.6 + 0.8 · (1 − mobilność_GD))
loyalty = base.loyalty · (lojalność/100)
novelty = base.novelty · (otwartość/100)
status  = base.status  · (status/100) · (ambicja/100 + 0.5)
brand   = base.brand                                     // M5: mnożone przez afinitet ≡ 0
```

Brak nowych parametrów osobowości — wszystkie z §5.1 (własność M3).
**Elastyczność cenowa nie jest tu parametrem** — jest własnością rozkładu `price` w populacji
i dostępności alternatyw (§6.4). Balansator ją mierzy, nie ustawia.

#### Szum — powtarzalny (dok. 00 §3 pkt 1)

```rust
// Szum musi być STAŁY dla danej trójki (kupujący, oferta, decyzja) w obrębie jednej decyzji,
// inaczej ponowna ewaluacja da inny wynik.
let key = mix64(citizen.index() as u64, offer.index() as u64);
let noise = rng_uniform_f32(world_seed, StreamId::PurchaseNoise, key, tick) * 2.0 - 1.0;
let noise = noise * sigma;     // sigma z data/economy, kalibrowana balansatorem
```

Żadnego globalnego stanu RNG. **K-4: blok `StreamId` przydzielony M5 to 180–199.**
Warianty dopisywane do enuma w `core`, nigdy nie zmieniamy istniejących wartości:

| Wartość | Wariant |
|---|---|
| 180 | `PurchaseNoise` |
| 181 | `PurchaseChoice` |
| 182 | `PriceExperiment` |
| 183 | `CompetitorDelay` |
| 184 | `ExternalPriceDrift` |
| 185 | `CreditScoringJitter` |
| 186–199 | wolne (rezerwa na rozszerzenia `sim/economy`, m.in. rynek pracy od M7) |

#### Wybór: softmax (§6.4 — nie argmax)

```rust
pub fn choose_offer(
    cands: &[Candidate],          // posortowane deterministycznie (krok 4 wyżej)
    utils: &[f32],
    temperature: f32,
    seed: RngKey,
) -> Choice;

pub enum Choice { Buy { idx: usize }, Defer { reason: DeferReason } }
```

1. `core::det_math::softmax(&utils, T, &mut probs)` — odejmuje maksimum (stabilność numeryczna)
   i sumuje **w kolejności indeksów wejściowych**, bez redukcji parami (K-6).
2. Dlatego `utils` musi już być w kolejności posortowanej tablicy kandydatów (krok 4 wyżej);
   nigdy po mapie (dok. 00 §2).
3. Losowanie: `r = rng_uniform_f32(seed, StreamId::PurchaseChoice, citizen.index(), tick)`,
   przejście po sumie skumulowanej `probs` w tej samej kolejności.
4. `T` (temperatura) z `data/economy/choice.ron`, jeden parametr globalny; kalibrowany balansatorem.
   `T → 0` degeneruje do argmax i produkuje monopole — bramka balansatora na koncentrację rynku
   (HHI) pilnuje, żeby kalibracja tam nie zjechała.

#### Próg odłożenia zakupu (§6.4)

```rust
pub fn purchase_threshold(need: NeedId, satisfaction: Q, budget: &HouseholdBudget, d: &EconomyData) -> f32;
```

```
U_threshold = thr0(need)
            − k_urgency · (100 − satisfaction)/100        // głodny kupi drożej
            + k_envelope · max(0, overspend_bp) / 10_000  // wyczerpana koperta podnosi próg
```

Ścieżka gdy `U_best < U_threshold` (kolejność z PRD §6.4):
1. **Substytut niższego rzędu** — następna ranga w koszyku potrzeby; jednokrotna ponowna ewaluacja
   z tym samym seedem decyzji (bez rekurencji).
2. Jeśli dalej poniżej progu — **odłożenie**: `Choice::Defer`, agent wraca do zadania później tego dnia.
3. Jeśli dzień się kończy — **ograniczenie konsumpcji**: zaspokojenie potrzeby spada,
   skutki obsługuje M3 (§5.3). `sim/economy` tylko raportuje zdarzenie.

#### Wyjaśnialność (§14.1, dok. 00 §7)

```rust
// K-12: JEDEN centralny enum w engine/core, bez #[non_exhaustive].
// M5 dopisuje poniższe warianty do istniejącego enuma — nie tworzy własnego.
pub enum DecisionReason {
    // ... warianty z innych faz
    Purchase {
        chosen: OfferId, runner_up: Option<OfferId>,
        price_delta_bp: i32,            // vs runner-up
        dominant_term: UtilityTerm,     // który człon przeważył
        u_chosen: f32, u_threshold: f32,
    },
    OfferRejected { site: SiteId, cause: RejectCause },
    PurchaseDeferred { need: NeedId, best_u: f32, threshold: f32, cause: DeferReason },
    Repricing { site: SiteId, good: GoodId, from: Money, to: Money, driver: PriceDriver },
    CreditDecision { applicant: AccountOwner, outcome: CreditOutcome, dsti_bp: i32 },
}

pub enum RejectCause {
    NotKnown, OutOfStock, TooFar { extra_min: u16 },
    PriceHigherBy { bp: i32 }, QualityBelowStatus, BudgetExhausted,
}
```

**Zapis utraconej sprzedaży (`LostSale`) — kontrakt uzgodniony z M9.** To jedyny sposób, żeby
odpowiedzieć na pytanie, które PRD §14.1 stawia jako sztandarowy przykład karty inspekcji.
Przyjmuję propozycję M9 w całości — trójstopniowa, bo pełny bufor na wszystkich sklepach AI
byłby kosztem bez odbiorcy:

```rust
pub enum LostSaleTracking { None, Histogram, Full }   // wybierane per SiteId

pub struct LostSaleHistogram {                 // ~120 B na sklep na dobę
    pub day: SimDay,
    pub by_cause: [u32; RejectCause::COUNT],   // ile razy który powód
    pub by_good:  SmallVec<[(GoodId, u32); 8]>,
}

pub struct LostSale {                          // 16 B, pierścień 256 wpisów
    pub citizen: CitizenId, pub good: GoodId,
    pub when: Tick, pub cause: RejectCause, pub went_to: Option<SiteId>,
}
```

| Zakład | Poziom | Koszt |
|---|---|---|
| zakłady gracza | `Histogram` **zawsze** + pierścień `Full` 256 wpisów | ~12,8 kB/sklep |
| zakłady oznaczone przez gracza („śledź") | `Full` | jw. |
| pozostałe zakłady AI | `None` | **0** |

Przy 200 sklepach gracza z pełnym śledzeniem i rokiem histogramów: ≈ 2,5 MB — mieści się
w budżecie pamięci §17.7. Zapis następuje w fazie decyzji, gdy kandydat został odrzucony
**i** jego `LostSaleTracking != None` — sprawdzenie to jeden odczyt bitu, więc gorąca ścieżka
dla rynku obsadzonego wyłącznie przez AI nie płaci nic.

#### Determinizm zmiennoprzecinkowy — rozstrzygnięte (K-6)

`ln`/`exp` z systemowego libm **nie są bit-identyczne między platformami**, a funkcja użyteczności
używa obu. M0 dostarczył `core::det_math` (`ln`, `ln1p`, `log2`, `exp`, `exp_m1`, `exp2`, `pow`,
`sqrt`, `softmax`) o dokładności ≤ 2 ULP, z lintem zakazującym libm w kodzie symulacji.

**`sim/economy` używa wyłącznie `core::det_math` — nie `libm`, nie `f32::ln`/`f32::exp`.**
Konkretnie: `f(x) = −det_math::ln1p(x)`, a wybór oferty woła `det_math::softmax`, nie własną pętlę.
`det_math::softmax` sumuje w kolejności indeksów wejściowych i nie stosuje redukcji parami —
dlatego tablica kandydatów musi być posortowana **przed** wywołaniem (krok 4 doboru kandydatów),
i to sortowanie jest jedynym miejscem, gdzie ustala się kolejność sumowania.

### 5.5 Rozliczanie transakcji — wyścig o ostatnią sztukę

Faza decyzji jest równoległa po chunkach mieszkańców i **nie mutuje** magazynów. Produkuje intencje:

```rust
pub struct PurchaseIntent {
    pub buyer: CitizenId, pub household: HouseholdId,
    pub offer: OfferId, pub good: GoodId, pub qty: Qty,
    pub agreed_price: Money,          // cena z momentu decyzji
    pub arrived: Tick,
    pub reason: DecisionReason,
}
```

Faza rozliczenia (`settle_transactions`, EveryMinute, sekwencyjna) sortuje intencje po
`(SiteId, GoodId, arrived, CitizenId)` i obsługuje do wyczerpania półki:

- towar jest → `Books::transfer(GD → sklep)`, `Shelf.qty -= qty`, zapis w dzienniku sklepu
  (Revenue + Cogs), wpis do pamięci doświadczeń agenta (M3), aktualizacja `Offer.available`;
- towaru brak → `RejectCause::OutOfStock`, zdarzenie DES „przeplanuj zakup" (§17.3);
- cena się zmieniła między decyzją a rozliczeniem o więcej niż `price_slippage_bp` →
  agent ponownie ocenia próg (jednokrotnie), inaczej rezygnuje.

To rozwiązanie daje niezmiennik „żaden sklep nie sprzedaje towaru, którego nie ma"
**konstrukcyjnie**, bez blokad i bez zależności od kolejności ukończenia jobów (dok. 00 §3 pkt 3).

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

### 5.7 Punkt wymiany M5 ↔ M6: zewnętrzny dostawca

To jedyne źródło towaru w M5 i **jedyne miejsce, które M6 musi wymienić**. Sygnatura i księgowanie
są zaprojektowane tak, żeby M6 podmienił implementację, nie interfejs.

```rust
/// Kontrakt zaopatrzenia sklepu. W M5: jedna implementacja (ExternalSupplier).
/// W M6: zastąpiona przez rynek B2B (spot + kontrakty) — sygnatura bez zmian.
pub trait Wholesale {
    fn quote(&self, good: GoodId, qty: Qty, at: SiteId, t: Tick) -> Option<PurchaseQuote>;
    fn place_order(&mut self, q: &PurchaseQuote, buyer: FirmId, t: Tick) -> Result<OrderId, SupplyError>;
    fn poll_deliveries(&mut self, t: Tick, out: &mut Vec<Delivery>);
}

pub struct PurchaseQuote {
    pub good: GoodId, pub qty: Qty,
    pub unit_price: Money,          // cena hurtowa
    pub delivery_at: Tick,          // teraz + lead_time_days z data/goods
    pub quality: Q,
    pub shelf_life: Option<SimMinute>,
}

/// M5: cena hurtowa = wholesale_base(good) · sezonowość(month) · dryf(t) — z data/goods/*.ron.
/// Dryf deterministyczny: rng(world_seed, StreamId::ExternalPriceDrift, good, day).
/// Pieniądz idzie na konto AccountOwner::RestOfWorld — czyli NIE WYPADA z systemu.
pub struct ExternalSupplier { /* ... */ }
```

Trzy jawne konsekwencje, które M6 musi znać:
1. `RestOfWorld` jest kontem w `Books`, nie ujściem — dzięki temu niezmiennik pieniądza jest
   sprawdzany bez ewidencji przepływów zewnętrznych.
2. Zapas jest wyceniany **średnią ważoną** (`StockLine.cost_total / qty`), bo nie ma partii.
   M6 wprowadza `BatchId` i FIFO — to **zmienia COGS**; migracja opisana w sekcji 9 pkt 5.
3. Dostawa zewnętrzna nie zajmuje pojazdu ani rampy (§7.3) — tylko czas. M6 to urealnia.

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

### 5.11 Systemy ECS i częstotliwości

| System | Częstotliwość | Odczyt | Zapis | Równoległość |
|---|---|---|---|---|
| `offer_index_rebuild` | EveryHour | `Shelf`, pozycje `SiteId` | `OfferIndex` | po komórkach dirty |
| `restock_shelf` | EveryHour | `ShopInventory`, `AssortmentPolicy` | `Shelf`, `Offer.available` | po sklepach |
| `purchase_decision` | zdarzeniowo (DES §17.3) | `OfferIndex`, `Offer`, `Personality`, `ExperienceMemory`, `HouseholdBudget` | bufor `PurchaseIntent` | po chunkach mieszkańców |
| `settle_transactions` | EveryMinute | `PurchaseIntent` | `Books`, `Ledger`, `Shelf`, pamięć agenta | **sekwencyjny**, sort `(Site, Good, arrived, Citizen)` |
| `observe_competitors` | EveryDay (offset `firm.index() % 1440`) | `OfferIndex`, `Offer` | `CompetitorSnapshot` | po firmach |
| `reprice` | EveryDay | `CompetitorSnapshot`, `ShopInventory`, `ObservedElasticity` | `Offer.unit_price`, `PriceController` | po sklepach |
| `price_experiments` | EveryDay | historia sprzedaży | `PriceExperiment`, `ObservedElasticity` | po sklepach |
| `markdown_perishables` | EveryDay | `StockLine.expires` | `Offer.unit_price`, `Ledger` (WriteOff) | po sklepach |
| `replenish_from_wholesale` | EveryDay | `ShopInventory`, `ReorderPolicy` | `Books`, `Ledger`, zamówienia | po sklepach |
| `receive_deliveries` | EveryHour | zamówienia | `ShopInventory`, `Ledger` | po sklepach |
| `household_budget_plan` | EveryMonth | dochody, koszty stałe, `Personality` | `HouseholdBudget` | po GD |
| `loan_servicing` | EveryDay | `Loan` | `Books`, `Ledger`, `MoneySupplyLedger` | sekwencyjny po `LoanId` |
| `bank_underwriting` | zdarzeniowo | `LoanApplication` | `Loan`, `Books` | sekwencyjny |
| `cpi_and_base_rate` | EveryMonth | `TransactionStats` | `CpiIndex`, `BaseRate` | 1 wątek |
| `ledger_close` | EveryMonth | `Ledger.journal` | `PeriodClose` | po sklepach |
| `shop_panel_snapshot` | EveryHour / na żądanie UI | wszystko powyżej (RO) | `ShopPanelSnapshot` | 1 wątek |

Zgodność LOD (dok. 00 §4): `purchase_decision` i `settle_transactions` działają **identycznie**
w mikro i mezo — LOD zmienia tylko sposób, w jaki agent dociera do sklepu (M4), nie cenę,
nie wybór, nie zapis księgowy. Test: ten sam scenariusz w mikro i mezo → identyczne salda, tolerancja 0.

### 5.12 Panel sklepu w UI (§14.3)

```rust
pub struct ShopPanelSnapshot {
    pub shelves: Vec<ShelfRow>,
    pub customers: CustomerStats,
    pub lost_sales: LostSalesView,
    pub competition: Vec<CompetitorRow>,
    pub finance: FinanceSummary,          // RZiS bieżącego miesiąca + przepływy + stan zapasów
}

pub struct ShelfRow { pub good: GoodId, pub price: Money, pub unit_cost: Money, pub margin_bp: i32,
                      pub on_shelf: Qty, pub backroom: Qty, pub days_of_cover: u16,
                      pub turnover_7d: Qty, pub expires_in: Option<SimMinute>, pub policy: PricePolicy }

pub struct CustomerStats { pub by_district: Vec<(DistrictId, u32)>,   // „skąd"
                           pub by_class: Vec<(SocialClass, u32)>,     // „kto"
                           pub by_driver: Vec<(UtilityTerm, u32)> }   // „dlaczego" — dominujący człon U

pub struct LostSalesView { pub histogram: LostSaleHistogram,   // zawsze dla zakładów gracza
                           pub recent: Vec<LostSale> }         // 256 wpisów, tylko Full

pub struct CompetitorRow { pub site: SiteId, pub distance_m: u32,
                           pub prices: Vec<(GoodId, Money)>, pub observed_age_days: u8 }
```

`observed_age_days` jest pokazywane wprost — gracz ma widzieć, że patrzy na dane sprzed N dni,
tak samo jak AI. Nakładka mapy cieplnej „zasięg sklepu" (§14.2) rysowana z `customers.by_district`.

---

## 6. Kontrakty międzyfazowe

### Dostarczam (`sim/economy`, `tools/balansator`)

| Kontrakt | Odbiorcy |
|---|---|
| `Offer`, `OfferId`, `OfferIndex`, `query_offers`, `PriceStats`, `PriceBasis` | M6 (B2B na tym samym mechanizmie, `NetB2B`), M7, M9, M10, `engine/ui` |
| `Books`, `Account`, `AccountId`, `AccountOwner` (z wariantem `City`), `Books::transfer` | wszystkie fazy dotykające pieniądza; `City` na wniosek M8 |
| `MoneySupplyLedger` z kanałem `external_capital_in/out` + `inject_external_capital`, `repatriate_external_capital` | **M7** — jedyny punkt emisji pieniądza spoza `RestOfWorld` (kapitał sieci zewnętrznej) |
| `Transaction`, `TxKind` (z `TaxPayment`, `PublicSpend`, `ExternalCapital`), `TxJournal`; `Transaction.tax` = **wyłącznie VAT** | M8 (pozostałe daniny przez własny `ChargeRegistry`, bez migracji dziennika), M7, M9 (kronika), M10 |
| `LostSaleTracking`, `LostSaleHistogram`, `LostSale` | **M9** (karta inspekcji „dlaczego Anna nie kupiła u mnie", §14.1) |
| `Ledger`, `LedgerAccount`, `JournalEntry`, `post`, `income_statement`, `balance_sheet`, `cash_flow` | M6, M7 (księgowość zakładu produkcyjnego), M9 (panel Finanse), M10 (wycena giełdowa) |
| `PricePolicy`, `PriceController`, `reprice`, `CompetitorSnapshot`, `ObservedElasticity` | M7 (firmy AI), M9 (delegowanie), M10 |
| `ShopInventory`, `Shelf`, `StockLine`, `ReorderPolicy`, `AssortmentPolicy`, `take_cogs` | M6 (partie zastąpią `StockLine.expires`), M7 |
| `utility_of_offer`, `weights_for`, `gather_candidates`, `choose_offer`, `purchase_threshold` | M3 (planer dnia woła to dla zadania „zakupy"), M10 (dopisze człon marki) |
| `sim/economy::labor`: oferty pracy (`CategoryId::JobRole`), indeks, dopasowanie po stronie szukającego, `Vec<Application>`, `LaborMarketStats` | **M7** (`sim/firms::labor_policy` domyka stronę pracodawcy — granica niżej) |
| **`sim/economy::kernel`**: `next_price`, `take_cogs`, `ledger_post` (M5); sloty `wage_bid` (M7), `throughput` (M6) | **M10** (model makro woła ten sam kod co mezo — warunek K-5), M6, M7 |
| `HouseholdBudget`, `plan_budget`, `budget_ref_for_need`, `Envelope` | M3, M7 (dochód z pensji), M8 (PIT), M9 |
| `Loan`, `LoanKind`, `build_schedule`, `assess_credit`, `CreditDecision`, `BaseRate`, `cpi` | M6 (kredyt obrotowy pod zapasy), M7 (inwestycyjny), M10 (hipoteczny, giełda) |
| `trait Wholesale`, `ExternalSupplier`, `PurchaseQuote`, `Delivery` | **M6 podmienia implementację, nie interfejs** |
| `trait TaxEngine` + `NoTax`, pole `Transaction.tax`, konto `TaxExpense` | **M8** |
| `DecisionReason::{Purchase, OfferRejected, PurchaseDeferred, Repricing, CreditDecision}` | `engine/devtools`, `engine/ui`, M9 |
| `StreamId` 180–199 (K-4): `PurchaseNoise`, `PurchaseChoice`, `PriceExperiment`, `CompetitorDelay`, `ExternalPriceDrift`, `CreditScoringJitter`; 186–199 wolne dla M7 | M0 (właściciel enuma), M7 |
| `ShopPanelSnapshot` i podtypy (w tym `LostSalesView`) | `engine/ui` (M3 właściciel szkieletu, M9 pełne panele) |
| `BalansatorMetrics` (schemat JSON) + bramki CI | M6–M12 (każda faza dopisuje metryki, nie zmienia istniejących pól) |

### Moduł `labor` — granica z `sim/firms` (D19 od M7)

Przyjmuję przeniesienie: `sim/economy::labor` (oferty pracy, indeks, dopasowanie, `LaborMarketStats`)
należy do mnie, `sim/firms::labor_policy` (scoring kandydata, eskalacja stawki) do M7.
Rynek pracy jest ofertą jak każda inna (§6.6: „każde stanowisko to oferta"), więc M5 **nie buduje
drugiego mechanizmu dopasowania** — `gather_candidates`/`choose_offer` są sparametryzowane kategorią
i zestawem wag, a M7 dokłada wariant `CategoryId::JobRole` i własne `UtilityWeights`
(pensja netto − koszt dojazdu, dopasowanie umiejętności, reputacja pracodawcy — §5.6).

**Granica, jednym zdaniem:** `sim/economy::labor` odpowiada za **stronę szukającego** — kto widzi
jaką ofertę i którą wybiera (mieszkaniec → oferta pracy, ten sam kod co mieszkaniec → oferta sklepu),
a `sim/firms::labor_policy` za **stronę ogłaszającego** — ile oferta ma obiecywać i którego
z aplikujących przyjąć.

Szwu nie ma, bo granica biegnie po danych, nie po module: ja wytwarzam **uporządkowany zbiór
aplikacji**, M7 zwraca **decyzję**. Rozpisane, żeby nie było wątpliwości przy pierwszym konflikcie:

| Należy do mnie (`sim/economy::labor`) | Należy do M7 (`sim/firms::labor_policy`) |
|---|---|
| `Offer` z `CategoryId::JobRole` jako nośnik stawki i warunków | **jaka stawka** trafia do tej oferty (wywołuje `kernel::wage_bid`) |
| indeks przestrzenny ofert pracy, zasięg dojazdu, filtr znajomości | — |
| użyteczność i wybór po stronie mieszkańca, próg zmiany pracy (§5.6) | — |
| uporządkowany `Vec<Application>` przekazany firmie | **scoring i wybór** spośród tych aplikacji |
| `LaborMarketStats` (rozkład pensji per zawód, wakaty, bezrobocie, rotacja) do balansatora | kiedy eskalować stawkę i o ile (polityka, nie mechanika) |
| `TxKind::Wage`, konto `WagePayable`, `StreamId` 186–199 | — |

Reguła rozstrzygająca spory graniczne: **jeśli coś zależy od osobowości konkretnej firmy — to M7;
jeśli jest takie samo dla każdego pracodawcy — to ja.** To ta sama linia, która w detalu oddziela
`reprice` (mój mechanizm) od `PricePolicy` (parametry właściciela).

### Podmoduł `sim/economy::kernel` (D20 od M10 przez M7) — zgoda

**Zgoda.** Rekomendacja jest trafna, a powód poważniejszy niż koszt refaktoru: dwa modele ekonomii
zawsze się rozjeżdżają, a wtedy K-5 (odchylenie agregatów ≤ 0,5%, brak dryfu) staje się nie do
utrzymania, bo dryf pochodziłby z **rozjazdu implementacji**, nie z agregacji — czyli mierzylibyśmy
własny błąd zamiast błędu przybliżenia. Model makro M10 musi wołać ten sam kod co mezo.

Jedna poprawka do sposobu: **nie wyciągam `kernel` po fakcie — piszę te funkcje od razu w `kernel`.**
Ekstrakcja post factum to koszt bez powodu, skoro w M5 dopiero powstają. „Zero zmian zachowania"
zostaje wymogiem tam, gdzie realnie coś przenoszę (`take_cogs`, walidacja zapisu księgowego).

```rust
/// Rdzeń liczbowy sim/economy. ZASADA: nie zna LOD, nie zna encji, nie alokuje.
/// Dostaje liczby, zwraca liczby. Brak &World, brak Entity, brak I/O, brak stanu globalnego.
/// Identyfikatory wchodzą wyłącznie jako indeksy katalogu danych (GoodId, JobRoleId),
/// nigdy jako id rozwiązywane przez świat (SiteId, FirmId, CitizenId).
pub mod kernel {
    pub fn next_price(i: PriceInput) -> Money;                 // M5 — rdzeń reprice (5.6)
    pub fn take_cogs(line: StockValue, sold: Qty) -> (Money, StockValue);  // M5 — wycena zapasu (5.8)
    pub fn ledger_post(lines: &[(LedgerAccount, Money)],
                       acc: &mut [Money; LEDGER_ACCOUNT_COUNT]) -> Result<(), LedgerError>;  // M5
    pub fn wage_bid(i: WageInput) -> Money;                    // slot — wypełnia M7
    pub fn throughput(i: ThroughputInput) -> Qty;              // slot — wypełnia M6
}
```

Wszystkie `*Input` są `Copy`-owymi strukturami POD; rozwiązanie encji na liczby robi **wołający**,
nie rdzeń. `ledger_post` pisze do bufora wołającego, dlatego nie alokuje mimo zmiennej liczby linii.

Trzy konsekwencje, dla których zgadzam się chętnie, a nie tylko bez sprzeciwu:

1. **Testy własnościowe przenoszą się na poziom, na którym są tanie.** P5 (brak dryfu wyceny),
   P6 (harmonogram sumuje się do kapitału), P7 (podział kwoty) i P4 (zbilansowanie zapisu) dają się
   uruchomić `proptest`-em **bez budowania świata** — milion przypadków zamiast stu scenariuszy.
2. **Zakaz floatów w pieniądzu (dok. 00 §2) staje się sprawdzalny mechanicznie.** Sygnatury
   `next_price`, `take_cogs` i `ledger_post` są w całości całkowitoliczbowe, więc test statyczny
   „w `kernel` nie występuje `f32`/`f64` w ścieżce pieniężnej" wystarcza zamiast przeglądu kodu.
3. **K-5 zostaje testowalne wprost:** makro i mezo wołają tę samą funkcję, więc różnica agregatów
   pochodzi wyłącznie z agregacji wejść. To jest test dla M10, ale warunek jego istnienia
   dostarczam ja.

Bramka „zero zmian zachowania" (wymóg, nie intencja): przed przeniesieniem `take_cogs` i walidacji
zapisu do `kernel` zapisuję złoty plik ciągu hashy z testu determinizmu (7.2); po przeniesieniu ciąg
musi być identyczny **bit w bit**. Refaktor, który zmienia choć jeden hash, jest z definicji
nieudany i wraca.

### Konsumuję

| Od kogo | Co | Uwaga |
|---|---|---|
| M0 | `Money`, `Qty`, `Tick`, `SimMinute`, `Q`, id-ki, RNG ze strumieniami, ECS, scheduler, job system, `tools/headless` | `tools/headless` musi dać się użyć **jako biblioteka** przez balansator — sekcja 9 pkt 10 |
| M0 | **`core::det_math`** (`ln`, `ln1p`, `log2`, `exp`, `exp_m1`, `exp2`, `pow`, `sqrt`, `softmax`), ≤ 2 ULP, lint zakazujący libm w symulacji | K-6 — **jedyne** źródło funkcji przestępnych w `sim/economy`; `softmax` sumuje po indeksach, bez redukcji parami |
| M0 | `DecisionReason` jako jeden centralny enum bez `#[non_exhaustive]` (K-12); `UtilityKind` i wspólne słowniki w `engine/core` (K-8) | M5 dopisuje warianty, nie tworzy własnego enuma |
| M0 | kalendarz 360 dni, 12 × 30 (K-1) | miesiąc odsetkowy i okres księgowy == 30 dni; brak konwencji ACT/365 |
| M8 | `CityTaxEngine` jako implementacja `TaxEngine`; `ChargeRegistry` dla danin innych niż VAT | zamyka moją decyzję otwartą nr 8 — bez migracji dziennika |
| M1/M2 | `engine/spatial` (grid, zapytanie promieniowe), `DistrictId`, `ParcelId`, pozycje budynków, pojemność budynku (m²→sloty półki) | |
| M3 | `Personality` (wrażliwość cenowa, lojalność, oszczędność, otwartość, ambicja, ryzyko), `Needs` + tempo spadku, `ExperienceMemory`, `KnownPlaces`, `HouseholdId` + typ GD, kolejka zdarzeń DES, zadanie „zakupy" z planera dnia | `KnownPlaces` — czyja własność? sekcja 9 pkt 4 |
| M4 | `travel_cost(from, to, mode, at) -> (SimMinute, Money)` | **twarda zależność** członu `g`; potrzebny jest też koszt pieniężny, nie tylko czas — sekcja 9 pkt 3 |
| M6 | zastąpi `ExternalSupplier` implementacją B2B | |
| M7 | zastąpi `income_monthly` pensją emergentną; uczyni bank pełną firmą | |
| M8 | dostarczy `TaxEngine` z VAT | |
| M10 | dostarczy `brand_affinity` (w M5 ≡ 0) i plotkę rozszerzającą `KnownPlaces` | |

---

## 7. Testy i kryteria akceptacji

### 7.1 Testy własnościowe (obowiązkowe, dok. 00 §6)

| # | Własność | Tolerancja | Jak |
|---|---|---|---|
| P1 | `Σ sald == endowment + credit_created − credit_repaid + external_capital_in − external_capital_out` | **0 groszy** | co tick w debug, co 1000 ticków w release; `RestOfWorld` jest zwykłym kontem, więc handel zewnętrzny nic nie psuje |
| P1b | Kanał kapitału zewnętrznego (M7) nie łamie P1 | **0 groszy** | **test pisany w M5, mimo że kanał jest w M5 nieużywany**: scenariusz proptest wplata `inject_external_capital` / `repatriate_external_capital` w losowy strumień przelewów i kredytów. Bez tego test pęknie dopiero w M7 — daleko od przyczyny (zgłoszenie M7) |
| P2 | Żaden sklep nie sprzedaje towaru, którego nie ma | 0 sztuk | po każdej fazie rozliczenia: `Σ sprzedanych z linii ≤ stan półki przed fazą`; test wyścigu: 10 tys. agentów, 10 sztuk → dokładnie 10 transakcji |
| P3 | Brak ujemnych stanów | — | `StockLine.qty ≥ 0`, `Shelf qty ≥ 0`, `Offer.available ≥ 0`, `Loan.outstanding ≥ 0`, saldo ≥ `−overdraft_limit` |
| P4 | Każdy zapis w dzienniku jest zbilansowany | 0 groszy | `Σ lines == 0` wymuszone w `post` |
| P5 | `InventoryGoods` w bilansie == `Σ StockLine.cost_total` | 0 groszy | po roku symulacji; chroni gałąź „zmiatania reszty" w `take_cogs` |
| P6 | Harmonogram kredytu sumuje się do kapitału | 0 groszy | `Σ installment.principal == principal` dla 10⁴ losowych (kwota, stopa, okres) |
| P7 | Podział kwoty między N stron sumuje się do oryginału | 0 groszy | test kopert budżetowych i rozbicia raty |
| P8 | 100% decyzji ma `DecisionReason` | 100% | test przechodzi po dzienniku decyzji z 30 dni symulacji |

| P9 | `kernel` nie ma floatów w ścieżce pieniężnej | — | test statyczny: w sygnaturach i ciałach `next_price`, `take_cogs`, `ledger_post` nie występuje `f32`/`f64`. Możliwy tylko dlatego, że rdzeń jest wydzielony (D20) |

Narzędzie: `proptest` dla P1, P1b, P3, P6, P7; scenariusze headless dla P2, P8.

**P4, P5, P6, P7 uruchamiane są na `kernel`, nie na świecie** — czyste sygnatury pozwalają
`proptest`-owi wygenerować milion przypadków zamiast stu scenariuszy. To jest główny zysk z D20
po mojej stronie i powód, dla którego wydzielenie rdzenia jest częścią WP1/WP6/WP7, a nie
osobnym refaktorem na końcu.

### 7.2 Determinizm (dok. 00 §3)

- Dopisanie wszystkich komponentów `sim/economy` do funkcji haszującej stan ECS — **warunek DoD fazy**.
- Dwa przebiegi tego samego seeda (365 dni, 50 tys. mieszkańców) → identyczny ciąg hashy co 1000 ticków.
- Przebieg jednowątkowy vs. `RAYON_NUM_THREADS=1,2,8,16` → identyczny ciąg hashy.
- Przebieg na Windows vs. Linux → identyczny ciąg hashy (**to jest test, który wyłapie obejście
  `core::det_math`** — patrz 5.4/K-6). Uzupełniająco lint CI z M0 zakazujący libm w `sim/*`.
- Mikro vs. mezo: ten sam scenariusz → identyczne salda i identyczna liczba transakcji (dok. 00 §4).

### 7.3 Wydajność (criterion, cel dla miasta 150 tys.)

| Ścieżka | Cel |
|---|---|
| `query_offers` (5 tys. ofert, promień 3 km) | < 20 µs |
| `utility_of_offer` | < 120 ns na kandydata |
| `purchase_decision` — cała doba (≈450 tys. decyzji × ≤15 kandydatów) | < 900 ms CPU łącznie, ≤ 3 ms na tick minutowy w szczycie |
| `settle_transactions` (tick szczytowy, ≈2 tys. intencji) | < 1,5 ms (sekwencyjny — to jest górna granica, patrz ryzyko R3) |
| `reprice` dla 2 tys. sklepów × 40 towarów | < 40 ms raz na dobę |
| Pamięć: `Offer` | ≤ 48 B; `ShopLostSales` ≤ 256 wpisów × 16 B na sklep |

Zero alokacji w `gather_candidates` i `utility_of_offer` (bufory wielokrotnego użytku) — weryfikowane
licznikiem alokacji w teście.

### 7.4 Balansator — kryteria akceptacji z PRD §20.1 przetłumaczone na testy

```
balansator run  --seeds 32 --days 730 --scenario base --out runs/
balansator run  --seeds 32 --days 730 --scenario supply-shock
balansator run  --seeds 32 --days 730 --scenario player-price-war
balansator gate runs/ --profile ci|nightly
```

Metryki zbierane per przebieg per dzień: rozkład cen per `GoodId` (p10/p50/p90/min/max, liczba ofert),
CPI i inflacja m/m oraz r/r, mediana marży sklepów, liczba sklepów i bankructw, `deferral_rate`
(udział decyzji zakończonych odłożeniem), `stockout_rate`, HHI koncentracji per kategoria,
podaż pieniądza, suma kredytów, stopa bazowa, rozkład zaspokojenia potrzeb.

| Bramka | Źródło PRD | Warunek (fail = czerwone CI) |
|---|---|---|
| **G1 Stabilność cen** | §20.1 „inflacja roczna −5…+15% w 95% seedów" | w ≥ 95% seedów inflacja r/r ∈ ⟨−5%, +15%⟩ w każdym miesiącu od 12. miesiąca (rozbieg wyłączony) |
| **G2 Brak hiperinflacji** | §20.1, §16.5 | żaden seed: brak miesiąca z inflacją m/m > 10%; brak `CPI_t / CPI_{t−90d} > 1,5` |
| **G3 Brak spirali deflacji** | §16.5 | żaden seed: brak 6 kolejnych miesięcy ze spadkiem CPI; mediana marży sklepów nie schodzi poniżej `min_margin_bp` w > 5% dni |
| **G4 Reaktywność szoku podaży** | §20.1 „widoczny w 2–7 dni, wygaszony w 2–8 tygodni" | scenariusz `supply-shock` (cena hurtowa wskazanego towaru +80% w dniu 180): mediana ceny detalicznej pokrywa ≥ 50% szoku w przedziale **2–7 dni** (`t_response`) i stabilizuje się w ±10% nowego poziomu w przedziale **14–56 dni** (`t_settle`). Zbyt szybko = też fail (rynek bez tarcia, sprzeczne z opóźnieniem obserwacji 1–7 dni) |
| **G5 Rynek nie wymiera** | §16.5 | ≥ 1 aktywna oferta w każdej kategorii w 100% dni; mediana `deferral_rate` < 25%; `stockout_rate` < 15% |
| **G6 Brak monopolizacji z kalibracji** | §6.4 (softmax, nie argmax) | HHI per kategoria per dzielnica < 0,6 w medianie seedów — chroni przed zjechaniem temperatury softmaxu do zera |
| **G7 Zachowanie pieniądza** | dok. 00 §6 | każdy przebieg kończy P1 zielono, 0 groszy |
| **G8 Determinizm** | dok. 00 §3 | dwa przebiegi seeda 0 → identyczny ciąg hashy |
| **G9 Wyjaśnialność** | §20.1 „100% decyzji" | 0 decyzji bez `DecisionReason` w próbce 10⁵ |

Profile: **ci** (PR) = 8 seedów × 365 dni, bramki G1–G3, G5, G7–G9, budżet czasu < 10 min.
**nightly** = 32 seedy × 730 dni, wszystkie bramki + scenariusze szokowe + raport Markdown z wykresami
(analiza w Pythonie, zgodnie z §20.4 — Python zostaje w narzędziach).

Test bramki (meta-test, uruchamiany raz): celowe usunięcie dolnego ogranicznika ceny w `reprice`
musi zaczerwienić G3. Bramka, która nigdy nie świeci na czerwono, nie jest bramką.

### 7.5 Definition of Done fazy

Wszystko z 7.1–7.4 zielone, plus: komponenty `sim/economy` w funkcji haszującej stanu,
`clippy -D warnings`, panel sklepu odpowiada na „dlaczego Anna nie kupiła u mnie" dla ≥ 95% przypadków,
balansator wpięty w CI (to jest osobne kryterium — §20.4 wskazuje go jako **główną mitygację ryzyka**).

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Prawdopodobieństwo / skutek | Mitygacja |
|---|---|---|---|
| R1 | **Spirala cenowa** (deflacja przy nadmiarze zapasów lub hiperinflacja przy pętli kredytowej) | wysokie / krytyczne | dolny i górny ogranicznik marży w `reprice` (5.6); bramki G1–G3 w CI **od pierwszego dnia fazy**, nie na końcu; balansator budowany w WP13, ale metryki zbierane od WP5 |
| R2 | **Determinizm zmiennoprzecinkowy** — funkcja użyteczności używa `ln` i `exp` | **zamknięte (K-6)** / było krytyczne | `core::det_math` z M0 (≤ 2 ULP, `softmax` sumujący po indeksach) + lint CI zakazujący libm w symulacji + test hashy Windows vs. Linux. Pozostała resztka ryzyka: obejście `det_math` przez nieuwagę — wyłapuje je lint i test międzyplatformowy |
| R3 | **`settle_transactions` jest sekwencyjny** — przy dużym mieście może stać się wąskim gardłem | średnie / średnie | sekwencyjność dotyczy tylko intencji z bieżącego ticku (≈2 tys.), nie populacji. Ścieżka podniesienia: partycjonowanie po `SiteId` (sklepy są rozłączne, więc równoległość jest legalna) — wdrożyć dopiero gdy benchmark z 7.3 czerwieni się przy 400 tys. (M12) |
| R4 | **Degeneracja wyboru do monopolu** — wszyscy do najtańszego, konkurencja pada, potem monopolista windzi ceny | średnie / wysokie | softmax zamiast argmax (§6.4), szum, waga odległości, filtr znajomości (§5.7), premia za nowość; bramka G6 (HHI) |
| R5 | **Dryf zaokrągleń w wycenie zapasów** — bilans przestaje się zamykać po tysiącach transakcji | wysokie (jeśli nieprzewidziane) / wysokie | gałąź „zmiatania reszty" w `take_cogs` (5.8) + test własnościowy P5 |
| R6 | **Zależność od M4** — człon `g` wymaga kosztu przejazdu w czasie **i** pieniądzu; jeśli M4 daje tylko czas, człon traci sens | średnie / wysokie | zaślepka `travel_cost` z tabeli czasów dzielnica↔dzielnica (§17.6) pozwala rozwijać WP4 równolegle; rozbieżność zapisana w sekcji 9 pkt 3 |
| R7 | **Koszt gorącej ścieżki** — 450 tys. decyzji/dobę × 15 kandydatów × 8 członów | średnie / średnie | bufory wielokrotnego użytku, obcięcie K=15 z wstępnym rankingiem całkowitoliczbowym, cache agregatów per dzielnica (§17.5), benchmark jako bramka |
| R8 | **Zmiana wyceny zapasów WAC → FIFO w M6** zmienia historyczne COGS i psuje porównywalność raportów | średnie / niskie | jawne udokumentowanie w 5.7 pkt 2; decyzja o sposobie migracji — sekcja 9 pkt 5 |
| R9 | **Brak pętli dochodowej** — w M5 dochód GD jest egzogeniczny (RestOfWorld), więc balansator może „potwierdzać" stabilność, której w M7 zabraknie | wysokie / średnie | scenariusz balansatora `income-shock` (−20% dochodów) już w M5; jawne oznaczenie wyników M5 jako **warunkowych** do czasu M7; ponowne przejście bramek jest kryterium akceptacji M7, nie M5 |
| R10 | **Przeciążenie zakresu fazy** — M5 to 14 pakietów i pierwszy vertical slice | wysokie / średnie | twarda kolejność: WP1 (pieniądz) → WP2–WP5 (pętla) → reszta. Po WP5 istnieje grywalne demo; WP6–WP12 podnoszą jakość, WP13 zabezpiecza. Jeśli faza się przeciąga, WP11 i część WP6 (eksperymenty cenowe) są kandydatami do przesunięcia do M7 |

---

## 9. Decyzje otwarte

Runda uzgodnień międzyfazowych zamknęła punkty **6, 8 i 12** oraz wniosła rozstrzygnięcia wiążące
K-1, K-4, K-7, K-8, K-12 (wprowadzone do sekcji 5 i 6). Poniższe punkty pozostają otwarte
i wymagają rozstrzygnięcia **przed startem fazy**; przy każdym jest propozycja M5, którą przyjmuję
jako domyślną, jeśli nikt nie zgłosi sprzeciwu.

Runda zamykająca M7 dołożyła dwa rozstrzygnięcia, oba **przyjęte** (sekcja 6):
**D19** — moduł `labor` wchodzi do `sim/economy`, granica z `sim/firms::labor_policy` biegnie
między stroną szukającego (moja) a stroną ogłaszającego (M7); reguła rozstrzygająca spory:
zależne od osobowości firmy → M7, jednakowe dla każdego pracodawcy → ja.
**D20** — zgoda na `sim/economy::kernel`, z jedną poprawką: funkcje powstają **od razu** w rdzeniu,
nie są wyciągane po fakcie; „zero zmian zachowania" obowiązuje tam, gdzie realnie coś przenoszę,
i jest egzekwowane złotym plikiem hashy (bit w bit).

Zmiany przyjęte we wcześniejszej rundzie bez zastrzeżeń: `AccountOwner::City`, `TxKind::{TaxPayment,
PublicSpend}`, rozbicie pasywów na `TradePayable`/`TaxPayable`/`WagePayable` (M8); kanał
`external_capital_in/out` w `MoneySupplyLedger` wraz z testem P1b (M7); trójstopniowy
`LostSaleTracking` (M9); miejsce na `LaborMarketStats` przez reużycie istniejącego mechanizmu
dopasowania zamiast drugiego (M7).

1. **`Offer` jako encja ECS czy komponent półki?**
   Dok. 00 §2 definiuje `OfferId(Entity)`, a §17.2 PRD wymienia ofertę wśród encji — więc encja.
   Koszt: 1 encja na (sklep × towar), przy 2 tys. sklepów × 40 towarów = 80 tys. encji.
   *Propozycja M5: encja. Do potwierdzenia z M0 (budżet encji) i M6 (oferty B2B mnożą tę liczbę).*

2. **Skąd bierze się dochód GD w M5?**
   M5 potrzebuje dochodu, żeby budżet miał sens; pensje emergentne to M7.
   *Propozycja M5: `HouseholdBudget.income_monthly` wypłacane z konta `RestOfWorld`, kwota
   z profilu zawodowego mieszkańca (M3). Kto jest właścicielem tej logiki — M3 czy M5? Propozycja: M5,
   bo dotyczy pieniądza; M7 podmienia źródło bez zmiany struktury.* **Do uzgodnienia z M3 i M7.**

3. **Sygnatura `travel_cost` z M4.**
   Człon `g` wymaga `(SimMinute, Money)` — czasu **i** kosztu pieniężnego (paliwo, bilet, parking).
   Jeśli M4 zwraca sam czas, M5 musiałby duplikować model kosztu pojazdu.
   *Propozycja M5: `fn travel_cost(from, to, mode, at) -> TravelCost { time: SimMinute, money: Money }`
   jako kontrakt M4.* **Do uzgodnienia z M4 — to najtwardsza zależność zewnętrzna tej fazy.**

4. **Kto jest właścicielem `KnownPlaces` (znajomość sklepów, §5.7)?**
   *Propozycja M5: M3 (to pamięć agenta, obok `ExperienceMemory`); M5 tylko czyta i zgłasza
   zdarzenie „odwiedzono sklep". M10 dopisze plotkę.* **Do uzgodnienia z M3 i M10.**

5. **Migracja wyceny zapasów WAC (M5) → FIFO per partia (M6).**
   Warianty: (a) przełącznik od daty wejścia M6, historyczne linie wyceniane WAC do wyczerpania;
   (b) jednorazowa konwersja `StockLine` na sztuczne partie o koszcie średnim.
   *Propozycja M5: wariant (a) — prostszy i nie fałszuje historii.* **Do uzgodnienia z M6.**

6. ~~**Czy crate `libm` mieści się w granicy „własnego silnika"?**~~ — **ZAMKNIĘTE (K-6).**
   M0 dostarczył `core::det_math` (`ln`, `ln1p`, `log2`, `exp`, `exp_m1`, `exp2`, `pow`, `sqrt`,
   `softmax`, ≤ 2 ULP) plus lint zakazujący libm w symulacji. M5 używa `det_math`, nie `libm`.
   Konsekwencja projektowa: `softmax` sumuje w kolejności indeksów bez redukcji parami, więc
   sortowanie kandydatów **przed** wywołaniem jest jedynym miejscem ustalającym kolejność
   sumowania (5.4).

7. **Bank: firma (`FirmId`) czy abstrakcja?**
   `sim/firms` jest własnością M7. W M5 bank musi mieć konto, kapitał i politykę, ale nie potrzebuje
   pracowników ani produkcji.
   *Propozycja M5: bank ma `FirmId` i `Ledger` (jak sklep), ale jego „AI" to funkcja `assess_credit`;
   M7 czyni go pełną firmą z załogą i osobowością.* **Do uzgodnienia z M7.**

8. ~~**Hook VAT: pole `Transaction.tax` czy osobny rejestr podatkowy?**~~ — **ZAMKNIĘTE przez M8.**
   Rozgraniczenie: `Transaction.tax` zostaje **wyłącznie dla VAT-u** (nierozłączny od pojedynczej
   transakcji detalicznej), pozostałe daniny idą do `ChargeRegistry` po stronie M8 i lądują
   w dzienniku jako `TxKind::TaxPayment`. **Migracji dziennika w M8 nie będzie.**
   M8 buduje `CityTaxEngine` jako jedyną prawdziwą implementację mojego haka `TaxEngine`.

9. **Próg odłożenia zakupu: jeden globalny czy per potrzeba?**
   *Propozycja M5: `thr0` per `NeedId` z `data/economy/` (bo odłożenie zakupu chleba i odłożenie
   zakupu butów to nie to samo), przy normalizacji wag `Σ|w| = 1` zapewniającej wspólną skalę U.*
   Rozstrzygnięcie zależy od kalibracji balansatorem — do domknięcia w WP13.

10. **Balansator używa `tools/headless` jako biblioteki czy uruchamia proces?**
    Biblioteka: szybciej, jeden proces na seed w puli wątków. Proces: izolacja awarii, łatwiejsza
    równoległość na poziomie CI.
    *Propozycja M5: biblioteka + `--jobs N` procesów na poziomie CLI (najlepsze z obu).*
    **Do uzgodnienia z M0 — wymaga, żeby `tools/headless` eksponował API, nie tylko `main`.**

11. **Kwantyzacja `ObservedElasticity` w stanie trwałym.**
    Elastyczność jest floatem, ale trafia do stanu trwałego (a więc do hasha i do zapisu).
    *Propozycja M5: przechowywać jako `i32` w bp; float tylko jako wartość pośrednia w obliczeniu.*
    Decyzja wewnętrzna M5, zgłoszona dla spójności z dok. 00 §2.

12. ~~**Nazwa typu `PriceePolicy`**~~ — **ZAMKNIĘTE.** Potwierdzone: `PricePolicy`.

13. **Nowe, otwarte — źródło `LostSaleTracking` dla zakładów gracza.**
    Przyjąłem propozycję M9 (histogram zawsze dla zakładów gracza + pierścień 256 dla oznaczonych,
    zero kosztu dla AI, ≈ 2,5 MB przy 200 sklepach). Otwarte zostaje **kto ustawia flagę**:
    `sim/economy` czytając własność zakładu, czy `game/` przy przejęciu sklepu przez gracza.
    *Propozycja M5: `game/` ustawia, `sim/economy` tylko czyta — bo „kto jest graczem" nie jest
    pojęciem ekonomicznym.* **Do potwierdzenia z M9.**

14. **Nowe, otwarte — kto woła kanał kapitału zewnętrznego.**
    `MoneySupplyLedger.external_capital_in/out` i obie funkcje istnieją w M5, ale żaden system ich
    nie używa (pokryte wyłącznie testem P1b). *Propozycja M5: pozostają `pub`, wołane wyłącznie
    z `sim/firms` w M7.* Jeśli M7 potrzebuje innej ziarnistości niż „inwestor + kwota"
    (np. transzy, harmonogramu wejścia) — trzeba to wiedzieć przed zamrożeniem struktury.
    **Do potwierdzenia z M7.**

---

## 10. Szacunek wielkości

| WP | Nazwa | Rozmiar | Uzasadnienie |
|---|---|---|---|
| WP1 | Pieniądz, konta, podwójny zapis | **M** | mało kodu, dużo rygoru; testy własnościowe dominują nakład |
| WP2 | Oferta i indeks przestrzenny | **M** | opiera się na gotowym `engine/spatial` z M2 |
| WP3 | Sklep: magazyn, półka, asortyment, zewnętrzny dostawca | **L** | trzy stany (zaplecze/półka/oferta) + polityka zapasu + punkt wymiany z M6 |
| WP4 | Funkcja użyteczności i wybór oferty | **L** | rdzeń fazy; koszt leży w kalibracji i determinizmie, nie w liczbie linii |
| WP5 | Rozliczanie transakcji | **M** | mechanizm prosty, ale każdy błąd łamie niezmiennik |
| WP6 | Polityki cenowe AI | **L** | cztery niezależne mechanizmy korekty + obserwacja z opóźnieniem + eksperymenty |
| WP7 | Księgowość sklepu | **L** | plan kont, dziennik, trzy raporty, wycena zapasów bez dryfu |
| WP8 | Budżety GD | **M** | budżetowanie kopertowe + ścieżka niedoboru |
| WP9 | Banki i kredyt | **M** | zakres podstawowy; harmonogram i kreacja pieniądza to najdelikatniejsze punkty |
| WP10 | CPI, stopa bazowa, inflacja emergentna | **S** | dwie funkcje i koszyk w danych; trudność jest w kalibracji, nie w kodzie |
| WP11 | Polityki cenowe gracza | **S** | zero nowej logiki — tylko UI i walidacja nad typami z WP6 |
| WP12 | Panel sklepu w UI | **M** | trzy zakładki + snapshot + nakładka zasięgu |
| WP13 | Balansator i bramki CI | **L** | CLI, scenariusze, 9 bramek, raport, wpięcie w CI, meta-test bramek |
| WP14 | Testy własnościowe, determinizm, benchmarki | **M** | rozłożone na całą fazę, nie blok na końcu |

`sim/economy::kernel` (D20) **nie jest osobnym WP** — `next_price` powstaje w WP6, `take_cogs`
i `ledger_post` w WP7, walidacja zapisu w WP1. Napisane od razu w rdzeniu kosztują tyle samo,
co napisane obok, a P4–P7 stają się tańsze. Moduł `labor` (D19) też nie jest tu wyceniony:
należy do zakresu M7, M5 dostarcza wyłącznie sygnatury, które i tak buduje dla detalu.

Sumarycznie: 4 × L, 8 × M, 2 × S. Faza jest **największa z dotychczasowych** — to konsekwencja
bycia pierwszym vertical slice'em. Punkt kontrolny po WP5: istnieje grywalna pętla
(otwórz sklep → ustal cenę → obserwuj klientów). Jeśli do tego momentu budżet fazy jest przekroczony,
kandydatami do przesunięcia do M7 są WP11 i eksperymenty cenowe z WP6 — **nie** WP13,
bo balansator w CI jest wskazany w §20.4 jako główna mitygacja ryzyka projektu.
