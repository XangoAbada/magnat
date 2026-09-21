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

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| WP1 | Pieniądz, konta, podwójny zapis | M0 | M | `[x]` |
| WP2 | Oferta i indeks przestrzenny | M2 (`engine/spatial`), WP1 | M | `[x]` |

### WP1 — Pieniądz, konta, podwójny zapis
Jedno wejście do zmiany stanu pieniężnego: `Books::transfer`. Brak innego sposobu na zmianę salda
w całym `sim/economy` — wymuszone prywatnością pola `balance` i testem statycznym (grep w CI).
Świat zewnętrzny (`RestOfWorld`) jest **normalnym kontem w systemie**, nie ujściem — dzięki temu
zakup u zewnętrznego dostawcy i wypłata od abstrakcyjnego pracodawcy zachowują sumę pieniądza
bez osobnej ewidencji ujść.
Kryterium ukończenia: test własnościowy „suma pieniądza = emisja − destrukcja" zielony na losowym
strumieniu 10⁶ przelewów, tolerancja **0 groszy**; próba przelewu ujemnej kwoty i przekroczenia
limitu debetu zwraca błąd, nie panikuje.

**Zamknięte.** `sim/economy/tests/money_conservation.rs`: strumień 10⁶ operacji (przelewy,
kreacja i destrukcja kredytu, oba kierunki kanału kapitału zewnętrznego, emisja) kończy się
z tolerancją 0 gr, a niezmiennik jest sprawdzany co 50 tys. kroków, nie tylko na końcu.
Ścieżki błędu są w strumieniu obecne (≈ 380 tys. odrzuceń) i osobny test dowodzi, że nieudana
operacja nie zostawia śladu ani w saldach, ani w podaży, ani w dzienniku. Jedno wejście
do salda pilnuje `tests/single_entry_point.rs` — test tekstowy z własnym testem wykrywacza,
bo bramka, która nigdy nie świeci na czerwono, nie jest bramką.

### WP2 — Oferta i indeks przestrzenny
`Offer` w **dedykowanej arenie**, nie jako encja ECS (`K-16` — korekta dok. 00 §2; mechanizm
`Arena<T>` jest gotowy w `engine/core/src/arena.rs`). Indeks: grid komórek z `engine/spatial`,
osobna warstwa per kategoria potrzeby. Zapytanie zwraca **uchwyty**, cena czytana na żywo z areny —
zmiana ceny nie wymaga przebudowy indeksu.
Kryterium: `query_offers` dla 5 tys. ofert i promienia 3 km zwraca wynik w < 20 µs (criterion),
kolejność wyniku identyczna przy dwóch przebiegach i niezależna od kolejności wstawiania.

**Zamknięte.** Zmierzone **767 ns** przy 554 trafieniach z 5 tys. ofert na mapie 16 km —
z dwudziestokrotnym zapasem wobec budżetu. Kolejność wyniku jest rosnąca po `(CellId, OfferId)`
i wychodzi ze struktury, a nie z sortowania na końcu: przebudowa idzie po indeksach areny,
a zapytanie skanuje wiersze siatki rosnąco. Testy: niezależność od kolejności wstawiania,
identyczność przy 1 i 8 wątkach, uchwyt zdjętej oferty nie wraca z zapytania.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

**Kod poniżej jest stanem faktycznym po implementacji**, nie szkicem: rozjazdy wobec
pierwotnego planu są wypisane w tabeli korekt na końcu dokumentu.

### 5.1 Pieniądz, konta, transakcje

```rust
// --- konta -------------------------------------------------------------
pub struct AccountId(pub u32);

impl AccountId {
    /// Druga strona zapisu, która NIE jest kontem: emisja, kreacja i destrukcja
    /// pieniądza kredytowego, kapitał spoza systemu. Sentinel zamiast Option,
    /// bo Transaction ma być rozmiaru stałego (U-6).
    pub const OUTSIDE: AccountId = AccountId(u32::MAX);
}

pub enum AccountOwner {
    Household(HouseholdId),
    Citizen(CitizenId),          // gotówka w portfelu mieszkańca
    Firm(FirmId),
    Bank(FirmId),                // konto własne banku (rezerwy + kapitał) — decyzja 7
    City,                        // budżet miasta — wariant zamówiony przez M8, pusty w M5
    RestOfWorld,                 // zewnętrzny dostawca, abstrakcyjny pracodawca, import
    CentralBank,
}

pub enum AccountKind { Cash, Current, Savings, LoanLiability }

pub struct Account {
    pub owner: AccountOwner,
    pub kind: AccountKind,
    pub bank: Option<FirmId>,        // None dla Cash
    balance: Money,                  // PRYWATNE — zmiana wyłącznie przez metody Books
    pub overdraft_limit: Money,      // >= 0; domyślnie 0
}
impl Account { pub fn balance(&self) -> Money; }

// --- księga przelewów --------------------------------------------------
pub struct Books {
    accounts: Vec<Account>,                 // indeksowane AccountId
    supply: MoneySupplyLedger,              // PRYWATNE (U-9) — dostęp przez Books::supply()
    journal: TxJournal,                     // pierścień 4096 wpisów
    next_tx: u64,
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
impl MoneySupplyLedger { pub fn total(&self) -> Money; }
// Niezmiennik (test P1, tolerancja 0 groszy):
// Σ balance == endowment
//            + credit_created     − credit_repaid
//            + external_capital_in − external_capital_out

impl Books {
    /// JEDYNY sposób przeniesienia pieniądza między kontami.
    pub fn transfer(
        &mut self,
        from: AccountId,
        to: AccountId,
        amount: Money,              // musi być > 0
        memo: TxMemo,
        t: Tick,
    ) -> Result<TxId, TxError>;     // NonPositive | InsufficientFunds | SameAccount
                                    // | Unknown | Overflow | InvalidTax

    /// Emisja przy inicjalizacji świata. Wołana przez generator, nie przez system.
    pub fn endow(&mut self, to: AccountId, amount: Money, t: Tick) -> Result<TxId, TxError>;

    /// Kreacja i destrukcja pieniądza kredytowego — wołać wolno wyłącznie bankowi
    /// przy uruchomieniu i spłacie kredytu (M5d). `pub`, nie `pub(crate)` — patrz U-9.
    pub fn create_credit(&mut self, to: AccountId, amount: Money, loan: LoanId, t: Tick)
        -> Result<TxId, TxError>;
    pub fn destroy_credit(&mut self, from: AccountId, amount: Money, loan: LoanId, t: Tick)
        -> Result<TxId, TxError>;

    /// Jedyne wejście kanału M7. W M5 nie wołane przez żaden system —
    /// pokryte wyłącznie testem własnościowym P1b.
    pub fn inject_external_capital(&mut self, to: AccountId, amount: Money,
                                   investor: ExternalInvestorId, t: Tick) -> Result<TxId, TxError>;
    pub fn repatriate_external_capital(&mut self, from: AccountId, amount: Money,
                                       investor: ExternalInvestorId, t: Tick) -> Result<TxId, TxError>;

    pub fn supply(&self) -> &MoneySupplyLedger;
    pub fn journal(&self) -> &TxJournal;
    pub fn total_balance(&self) -> Money;
    /// Niezmiennik P1. Err niesie obie liczby, bo przy czerwonym teście obie są potrzebne.
    pub fn check_conservation(&self) -> Result<(), (Money, Money)>;
}
```

```rust
pub struct Transaction {
    pub id: TxId,
    pub tick: Tick,
    pub kind: TxKind,
    pub debit: AccountId,        // AccountId::OUTSIDE przy emisji i destrukcji
    pub credit: AccountId,
    pub net: Money,              // kwota bez VAT
    pub tax: Money,              // WYŁĄCZNIE VAT (rozstrzygnięcie z M8). W M5 zawsze Money(0)
    pub gross: Money,            // net + tax; to jest kwota realnie przelana
    pub reason: DecisionReason,  // enum z engine/core (K-12)
}

/// Opis zapisu podawany przez wołającego. `tax` siedzi tutaj, a nie w sygnaturze
/// `transfer`, żeby CityTaxEngine z M8 dopisywał wartość, a nie zmieniał API.
pub struct TxMemo { pub kind: TxKind, pub reason: DecisionReason, pub tax: Money }

pub enum TxKind {
    RetailSale   { offer: OfferId, good: GoodId, qty: Qty, buyer: HouseholdId },
    WholesalePurchase { good: GoodId, qty: Qty, supplier: SupplierRef },   // SupplierRef::External w M5
    Wage         { site: SiteId },        // w M5 z konta RestOfWorld do GD
    Rent         { site: SiteId },
    Utility      { site: SiteId, kind: UtilityService },  // UtilityService mieszka w engine/core (U-8)
    LoanDraw     { loan: LoanId },
    LoanPayment  { loan: LoanId, principal: Money, interest: Money },
    Deposit      { from_cash: bool },
    Withdrawal,
    Endowment,                            // emisja przy inicjalizacji świata (U-8)
    // --- warianty zamówione przez M8; w M5 nieużywane, ale obecne w enumie i w dzienniku ---
    TaxPayment   { charge: ChargeKind },  // ChargeKind z ChargeRegistry (własność M8)
    PublicSpend  { program: ProgramId },  // wydatek miasta
    ExternalCapital { investor: ExternalInvestorId, inflow: bool },  // kanał M7 (5.1)
}
```

Reguły arytmetyczne (dok. 00 §2): `checked_add`/`checked_mul`, `div_round_half_up`, podział kwoty
między N stron sumuje się do oryginału (reszta do pierwszego wg porządku `AccountId`).
Podziału nie implementujemy u siebie — `magnat_core::split_proportional` z M0 spełnia ten kontrakt
i ma własny test własnościowy.

**Rozgraniczenie z M8 (rozstrzygnięcie wiążące).** `Transaction.tax` jest **tylko dla VAT-u** —
bo VAT jest nierozłączny od pojedynczej transakcji detalicznej i musi się z niej wyodrębnić
w momencie rozliczenia. Wszystkie pozostałe daniny (CIT, PIT, podatek od nieruchomości, akcyza,
cła, opłaty koncesyjne) idą do `ChargeRegistry` po stronie M8 i trafiają do dziennika jako
`TxKind::TaxPayment`. **M5 nie przewiduje migracji dziennika w M8** — decyzja otwarta nr 8
jest tym samym zamknięta.

**Dziennik nie wchodzi do hasha treścią, tylko licznikiem.** `TxJournal` jest pierścieniem
o stałej pojemności, więc hashowanie zawartości uzależniłoby stan świata od rozmiaru bufora
diagnostycznego — a rozjazd treści zapisu i tak wychodzi na saldach. Do hasha idą konta
(właściciel, rodzaj, bank, saldo, limit), wszystkie pięć pozycji `MoneySupplyLedger` i `next_tx`.

### 5.2 Oferta i indeks przestrzenny (§6.1, §17.5)

Oferta siedzi w **arenie**, nie w ECS (`K-16`). `OfferId` to `ArenaHandle<Offer>` z `engine/core`,
a nie `Entity`: uchwyt po zwolnieniu nigdy nie jest ponownie ważny, arena wchodzi do funkcji
haszującej w kolejności indeksów, a snapshot i zapis traktują ją jak sekcję ECS.

```rust
pub type OfferId = ArenaHandle<Offer>;

/// Wpis areny ofert. Jedyny nośnik ceny w grze — nie istnieje żadna globalna cena towaru.
///
/// Pozycji tu nie ma z rozmysłu (U-4): oferta stoi tam, gdzie zakład, więc pozycja jest
/// własnością SiteId i podaje ją wołający przy przebudowie indeksu.
pub struct Offer {
    pub seller: FirmId,
    pub site: SiteId,            // sklep = nośnik lokalizacji i dostępności
    pub good: GoodId,
    pub unit_price: Money,       // za 1 sztukę (Qty(1000)); ZAWSZE kwota płacona przez kupującego
    pub price_basis: PriceBasis, // K-7: w M5 zawsze GrossRetail
    pub available: Qty,          // == Shelf line qty; 0 oznacza „znany sklep, ale brak towaru"
    pub quality: Q,
    pub category: CategoryId,    // warstwa indeksu
    pub since: Tick,
    pub price_rev: u32,          // licznik zmian ceny — do obserwacji konkurencji z opóźnieniem
}
impl Offer {
    /// Zmiana ceny NIE rusza indeksu: indeks trzyma uchwyty, cena czytana jest z areny.
    pub fn set_price(&mut self, p: Money);
}

/// K-7: cena w ofercie to kwota, którą płaci kupujący — brutto w detalu, netto w hurcie.
/// VAT wyodrębnia się dopiero przy rozliczeniu (net/tax/gross w Transaction), nie w ofercie.
pub enum PriceBasis { GrossRetail, NetB2B }

/// Warstwa indeksu. W M5 kategorią jest kategoria zapasu gospodarstwa (`StockCat` z `core`),
/// M7 dokłada `JobRole(JobRoleId)` — rynek pracy jest ofertą jak każda inna (U-3).
pub enum CategoryId { Stock(StockCat) }
impl CategoryId {
    pub const LAYER_COUNT: usize = STOCK_CAT_COUNT;
    pub const fn layer(self) -> usize;
}

/// Indeks przestrzenny: osobna warstwa per kategoria. Warstwą jest `DynamicGrid`
/// z `engine/spatial` (U-1) — ta sama struktura, którą M2 zbudował dla encji miasta.
pub struct OfferIndex {
    spec: GridSpec,
    layers: Vec<DynamicGrid<OfferId>>,   // indeksowane CategoryId::layer()
    dirty: Vec<bool>,
    pos_buf: Vec<Vec2>,                  // bufory robocze przebudowy
    id_buf: Vec<OfferId>,
}
impl OfferIndex {
    pub fn new(spec: GridSpec) -> OfferIndex;
    /// Dodanie albo zdjęcie oferty brudzi warstwę; zmiana ceny NIE.
    pub fn mark_dirty(&mut self, category: CategoryId);
    pub fn mark_all_dirty(&mut self);
    pub fn is_dirty(&self) -> bool;
    /// `pos_of` daje pozycję zakładu w metrach. Kolejność wejścia to kolejność indeksów
    /// areny, więc w komórce oferty leżą rosnąco po OfferId.
    pub fn rebuild(&mut self, offers: &Arena<Offer>,
                   pos_of: impl Fn(SiteId) -> Vec2, pool: &JobPool);
}

pub struct PriceStats { pub min: Money, pub p50: Money, pub max: Money, pub offers: u32, pub qty: Qty }

/// Agregat po ofertach — jedyna postać, w jakiej „cena towaru" w ogóle istnieje.
pub fn price_stats(offers: &Arena<Offer>, ids: &[OfferId], buf: &mut Vec<Money>) -> Option<PriceStats>;

/// Bufor wielokrotnego użytku — zero alokacji w gorącej ścieżce.
pub fn query_offers(
    index: &OfferIndex,
    category: CategoryId,
    origin: Vec2,                // metry; WorldPos nie istnieje — patrz U-2
    radius_m: u32,
    out: &mut Vec<OfferId>,
);
```

Kolejność wyniku: rosnąco po `(CellId, OfferId)` — deterministyczna i niezależna od kolejności
wstawiania. Zakaz iteracji po `HashMap` (dok. 00 §3 pkt 2).

**Wpięcie do świata.** `register_economy(&mut World, GridSpec)` wstawia `Books`, `Arena<Offer>`
i `OfferIndex` jako zasoby oraz rejestruje haki hasha: `Books` przez `register_resource_hash`,
arena przez `register_arena_hash::<Offer>(ArenaKind::Offers)`. **`OfferIndex` nie jest hashowany** —
jest pochodną areny i pozycji zakładów, więc hashowanie go dokładałoby do stanu świata coś,
co z tego stanu wynika (ta sama zasada, którą M4 zastosował do `TrafficOverlay`).

---

## Zmiany wpisane po M5a

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Korekta | Dlaczego |
|---|---|---|
| U-1 ★ | **`OfferIndex` nie jest `Vec<BTreeMap<CellId, Vec<OfferId>>>` z `dirty: BTreeSet`.** Warstwą jest `DynamicGrid<OfferId>` z `engine/spatial`, a brud to `Vec<bool>` per warstwa. **Cache `district_stats` nie powstaje** — `PriceStats` liczy się na żądanie funkcją `price_stats` | `DynamicGrid` daje **dokładnie** to, czego §5.2 żąda: przebudowę sortowaniem zliczającym w O(n + komórek), kolejność wyniku niezależną od liczby wątków (test D3 z M2) i zapytanie promieniowe do bufora bez alokacji. Własne `BTreeMap`-y byłyby przepisaniem gotowego kodu — i gorszym, bo `BTreeMap` na gorącej ścieżce zapytania kosztuje skok po wskaźniku na komórkę. Cache agregatów odpada z innego powodu: unieważnia go „tick cenowy", którego w M5a nie ma (powstaje w M5c), więc dziś zestarzałby się w ciszy. Wraca razem ze swoim konsumentem i swoim unieważnianiem |
| U-2 | **`WorldPos` nie istnieje w kodzie.** Zapytania przestrzenne pracują na `magnat_spatial::Vec2` (metry, `f32`), a deterministyczną pozycją świata jest `magnat_core::WorldCoord` (centymetry, `i32`) | Nazwa z planu nie miała desygnatu. Rozdział jest celowy i wprowadzony przez M2: float wolno w geometrii (00 §2), więc indeks liczy odległości w `f32`, a konwersja z `WorldCoord` należy do wołającego. Pieniądza to nie dotyka w żadnym punkcie |
| U-3 | **`CategoryId` nie istniało i nie powstaje w `core`** — to enum w `sim/economy` z jednym wariantem `Stock(StockCat)`; M7 dokłada `JobRole(JobRoleId)` | `K-8` każe przenosić do `core` słowniki, których używa więcej niż jedna faza i które nie mają jak sobie ich podać. Tu drugi konsument (M7) zależy od `sim/economy` wprost, więc cyklu nie ma i przenosiny nic nie kupują. Sama kategoria zapasu jest już w `core` jako `StockCat` — i to jej dokumentacja mówi wprost, że **to M5 przypisuje `GoodId` do kategorii** |
| U-4 | **`Offer` nie trzyma pozycji.** Podaje ją wołający przez domknięcie `pos_of: Fn(SiteId) -> Vec2` przy `OfferIndex::rebuild` | Pozycja oferty to pozycja zakładu — kopia, która nie ma własnego życia. Trzymanie jej w ofercie kosztowałoby 8 B × 10⁵ ofert i dokładało stan, który mógłby się rozjechać z prawdą o budynku. Przebudowa jest rzadka (dodanie albo zdjęcie asortymentu, nie zmiana ceny), więc rozwiązanie `SiteId → Vec2` w jej trakcie jest tanie |
| U-5 ★ | **`Offer` ma 56 B, a nie ≤ 48 B z §7.3 dokumentu fazy.** Budżet skorygowany do **56 B** | Rachunek: `FirmId` 8 + `SiteId` 8 + `GoodId` 2 + `Money` 8 + `PriceBasis` 1 + `Qty` 8 + `Q` 1 + `CategoryId` 2 + `Tick` 8 + `price_rev` 4 = 50 B, wyrównane do 56. Zejście do 48 wymagałoby usunięcia `seller` (wyprowadzalnego z `site`, ale za cenę wyszukania **w każdej transakcji**) albo `price_rev` (którego M5c potrzebuje, a dołożenie go później jest zmianą formatu areny, czyli zapisu gry). Różnica to 0,8 MB przy 10⁵ ofert — cena kontraktu, nie rozrzutność. Pilnuje jej test `oferta_miesci_sie_w_budzecie_pamieci` |
| U-6 | **`Transaction` potrzebuje drugiej strony spoza zbioru kont: `AccountId::OUTSIDE`** | Emisja, kreacja i destrukcja kredytu oraz kanał M7 są z definicji jednostronne wobec `Σ balance` — gdyby miały prawdziwe konto przeciwstawne, suma sald byłaby zawsze zerem i niezmiennik P1 nie miałby czego mierzyć. Sentinel zamiast `Option<AccountId>`, żeby `Transaction` został rozmiaru stałego i bez gałęzi |
| U-7 | **`TxError` dostaje dwa warianty ponad cztery z planu: `Overflow` i `InvalidTax`** | Kryterium WP1 mówi „zwraca błąd, **nie panikuje**", a są dwie drogi do paniki w ścieżce pieniężnej. `Overflow`: saldo i pozycje podaży są `i64`, więc bez tego wariantu przepełnienie musiałoby albo panikować, albo udawać `InsufficientFunds`. `InvalidTax`: rozbicie kwoty na netto i VAT dzieje się przy zapisie do dziennika, czyli **po** zmianie sald — VAT większy od kwoty zostawiłby przelew wykonany bez transakcji. Sprawdzenie jest więc na wejściu, razem z pozostałymi, i to jest jedyne miejsce, w którym M5 dotyka pola, które wypełni dopiero `CityTaxEngine` z M8 |
| U-8 | **Dwie poprawki w `TxKind`: `Utility.kind` to `UtilityService`, nie `UtilityKind`; dochodzi wariant `Endowment`** | `UtilityKind` w `core` **istnieje**, ale znaczy co innego: to wymiary funkcji użyteczności zakupu z PRD §6.4 (`Price, Quality, Distance…`). Mediami jest `UtilityService` (`Electricity, Water, Sewage…`) i jego dokumentacja mówi wprost, że powstał po to, żeby M5 mógł napisać `TxKind::Utility`. Pomyłka planu, nie zmiana kontraktu. `Endowment` dochodzi, bo emisja początkowa musi mieć swój rodzaj w dzienniku — inaczej byłaby nieodróżnialna od kredytu |
| U-9 | **`Books.supply` jest prywatne (dostęp przez `supply()`), a `create_credit`/`destroy_credit` są `pub`, nie `pub(crate)`** | Odwrotnie niż w planie, i w obu przypadkach z tego samego powodu — żeby widoczność mówiła prawdę. Publiczne `supply` pozwalałoby złamać P1 bez jednego przelewu, więc jest prywatne. `pub(crate)` przy kreacji kredytu nie ogranicza **niczego**, bo cała faza M5 (w tym bank z M5d) mieszka w tym samym crate'cie — ograniczał za to widoczność w teście własnościowym, który ma sprawdzać wszystkie kanały podaży, a nie te wygodne. Reguła „wolno wyłącznie bankowi" zostaje w dokumentacji metody, gdzie jest sprawdzalna przez człowieka, a nie udawana przez kompilator |
| U-10 | **`inject_external_capital` zwraca `Result<TxId, TxError>` i bierze `Tick`** | Plan dawał `-> TxId` bez ticku. Kwota bywa niedodatnia i saldo bywa bliskie granicy `i64`, więc funkcja bez `Result` musiałaby panikować; tick jest potrzebny, bo zapis idzie do dziennika tak samo jak każdy inny. Kanał zostaje przy ziarnistości „inwestor + kwota" (decyzja otwarta nr 14, propozycja M5 przyjęta domyślnie) |
| U-11 ★ | **Podmoduł `kernel` nie powstaje w M5a.** §10 dokumentu fazy przypisywał do WP1 „walidację zapisu" — w WP1 nie ma zapisu księgowego do walidowania | `Ledger`, plan kont i `JournalEntry` powstają w WP7 (M5c) i dopiero wtedy `ledger_post` ma co sprawdzać; `Σ lines == 0` dla dwuliniowego przelewu jest tożsamością, nie regułą. Jedyna arytmetyka pieniądza w WP1 to podział kwoty — a ten jest gotowy w `magnat_core::split_proportional` z własnym testem własnościowym, więc kopiowanie go do `kernel` łamałoby DRY zamiast realizować D20. **Cały `kernel` powstaje w M5c**, razem z `next_price`, `take_cogs` i `ledger_post`; wymóg „funkcje piszemy od razu w rdzeniu" zostaje nienaruszony, bo żadna z nich jeszcze nie istnieje |
| U-12 | **M5a nie zużywa ani jednej wartości `StreamId`** — blok 180–199 zostaje w całości wolny dla M5b | WP1 i WP2 nie losują: przelew jest funkcją argumentów, a indeks przebudową. Pierwszym kandydatem jest `PurchaseNoise` w decyzji zakupowej (WP4). Wartości `StreamId` są wieczne (`K-4`), więc rezerwowanie ich „na zapas" przed pierwszym zapisem, który je poniesie, jest kosztem bez korzyści |
| U-13 | **Decyzja otwarta nr 7 w wąskiej części przyjęta domyślnie: `AccountOwner::Bank(FirmId)` zostaje** | `T-8` wskazywał ten wariant jako bloker WP1, bo zamraża się razem z enumem. Propozycja M5 (bank ma `FirmId` i księgę jak sklep, a M7 czyni go pełną firmą) nie spotkała się ze sprzeciwem i nie zmienia kształtu konta — M7 dokłada firmie załogę i osobowość, nie przestawia właściciela rachunku. Szeroka część punktu 7 (czym bank jest jako firma) zostaje otwarta do M5d |
| U-14 | **Blok `DecisionReason` 300–399 zostaje pusty; transakcje M5a niosą `Unspecified`** | Wyjaśnialność (00 §7) dotyczy **decyzji**, a M5a żadnej nie podejmuje: przelew jest wykonaniem cudzej decyzji, emisja jest ustawieniem świata, a kanał M7 nie ma w M5 wołającego. Pierwszy wariant M5 (`ShopChosen`, zapowiedziany komentarzem w `engine/core/src/decision.rs`) dopisuje WP4 razem z decyzją zakupową, i wtedy dochodzi do niego ramię w `engine/ui::describe` oraz klucze `ui.reason.*` w **obu** plikach `data/locale/`. Bramka 5 zamyka się na poziomie fazy, nie podfazy (`K-17`) |
| U-15 | **`TxJournal` to sam pierścień (4096 wpisów), bez „zrzutu na dysk" z §5.1** | Zrzut potrzebuje formatu, wersjonowania i konsumenta — konsumentem jest kronika M9, a formatem `engine/io` w wersji pełnej z M12. Do tego czasu plik, którego nikt nie czyta, byłby kodem, który nie ma jak być nieprawdziwy. Pierścień wystarcza obu dzisiejszym odbiorcom: panelowi sklepu (M5e) i testom, które sprawdzają, że nieudana operacja nie zostawia wpisu |
