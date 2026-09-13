# M5a — Pieniądz i oferta

Podfaza 1 z 5 fazy **M5 — Gospodarka detaliczna** (`M5-gospodarka-detaliczna.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M0 (`core`), M2 (`engine/spatial`). |
| **Pakiety robocze** | WP1, WP2 |
| **Projekt techniczny** | §5.1, §5.2 |
| **Wynik do pokazania** | Test zachowania pieniądza zielony **zanim** powstanie pierwsza transakcja detaliczna — później nie da się go wprowadzić bez przepisywania. |
| **Kryterium zamknięcia** | Kryteria WP1 i WP2; suma sald niezmienna z tolerancją 0 gr. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M5b-sklep-i-zakup.md` |

Jedno wejście do zmiany stanu pieniężnego (`Books::transfer`) z podwójnym zapisem oraz `Offer` z indeksem przestrzennym per kategoria.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP1 | Pieniądz, konta, podwójny zapis | M0 | M |
| WP2 | Oferta i indeks przestrzenny | M2 (`engine/spatial`), WP1 | M |

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

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

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
