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

### WP1 — Szkielet `sim/city`, `CityBudget`, księga publiczna ✅
**Zależy od:** M5 (pieniądz, księgowość), M0 (ECS, RNG).
Encja `City` jako singleton ECS z komponentami `CityBudget`, `TaxCode`, `PolicySet`, `Government`.
Księga publiczna: przychody per `TaxKind`, wydatki per `SpendCategory`, saldo gotówki,
dług miejski (obligacje u banków M5), zobowiązania i należności z terminami.
Domknięcie miesiąca i roku budżetowego. Awaryjne domknięcie deficytu (cięcia albo dług).
**Kryterium ukończenia:** headless z ręcznie wstrzykniętymi wpłatami i wypłatami przechodzi
test zachowania pieniądza: `Σ firmy + Σ mieszkańcy + Σ banki + CityBudget.cash = const`,
tolerancja 0 gr, 100 tys. ticków.
**Rozmiar:** M

### WP2 — `TaxCode` i naliczanie danin ✅
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


---

## Zmiany wpisane po M8a

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| `CA-1` ★ | **`CityBudget` nie ma pola `cash`. Ma `account: AccountId`.** §5.1 zapisywał `pub cash: Money` z komentarzem „konto miasta w banku (M5)" | Dwa źródła prawdy o jednym saldzie rozjeżdżają się przy pierwszym przelewie, a testu, który by to złapał, nie ma po żadnej ze stron. Saldo czyta się z `Books`. Zysk, którego plan nie obiecywał: kryterium ukończenia WP1 („`Σ firmy + Σ mieszkańcy + Σ banki + miasto = const`") nie wymaga po stronie M8 **ani jednej linii kodu** — jest to `Books::check_conservation`, którego miasto nie umie złamać, bo nie ma innego sposobu ruszenia pieniądza niż `transfer` |
| `CA-2` ★ | **`trait TaxEngine` nie wygląda jak w §5.1 i nie mógł wyglądać.** Plan rysował `fn tax_on(&mut self, …)` i `fn record(…)`, czyli silnik zapisujący do rejestru. Rzeczywisty hak M5 to `gross_from_net`/`net_from_gross` na `&self`; M8 dokłada trzy metody z ciałem domyślnym (`vat_on_gross`, `excise_on`, `withhold`) i **nie rusza istniejących** (`K-57`) | Hak siedzi w `Market` jako `Box<dyn TaxEngine>` za `Arc<Mutex<…>>`, więc dostaje `&self`. Rejestr należności mieszka w zasobie `City` i napełnia go system miasta — rynek nie ma prawa o nim wiedzieć. Przypadek (2) z `K-18`: API, którego plan używa w przykładzie, nazywa się inaczej |
| `CA-3` ★ | **Siedem danin to płaski słownik w `core`, a nie enum z ładunkiem.** §5.1 zapisywał `Excise(ExciseClass)`, `Duty(TariffClass)`, `License(LicenseClass)` | `vocab_enum!` daje słownik, nie enum z parametrem, a `TaxKind` musi być ładunkiem `DecisionReason` (`K-55`). Klasa idzie osobnym polem należności — i tak jest lepiej, bo histogram wpływów ma liczyć daniny, a nie pary (danina, klasa). Ta sama korekta co przy `BankruptcyTrigger` (`K-48`) |
| `CA-4` ★ | **Płatnikiem jest zakład (`SiteId`), a nie firma.** §5.1 zapisywał `TaxPayer::Firm(FirmId)` | Księga, konto i rachunek wyniku są w tej symulacji **per zakład** (`Ledger::new(site, firm, …)`), a danina bez konta, z którego da się ją ściągnąć, byłaby liczbą w raporcie, a nie pieniądzem w budżecie. Firma widzi swoje obciążenia jako sumę po swoich zakładach; rejestr firm zna tę listę. `TaxPayer::Household` zostaje w enumie i nie ma w M8a ani jednego użycia — PIT pobiera się u źródła, a nie od gospodarstwa |
| `CA-5` ★ | **Ziarnistość należności to (płatnik, danina, okres), nie transakcja.** §5.1 tego nie przesądzał, a tabela kontraktu księgowania sugerowała naliczenie na transakcji | Wykonanie `R8` wprost: siedem danin naliczanych per transakcja daje graczowi ścianę liczb, a rejestr rosnący jak dziennik transakcji przestałby się mieścić w pamięci po pierwszym roku. Piekarnia widzi jeden wiersz „VAT, marzec", a nie cztery tysiące wierszy po jednej bułce. Konsekwencja: `ChargeRegistry::accrue` **dolicza** do otwartej należności zamiast zakładać nową |
| `CA-6` ★ | **Zaliczka PIT liczy się w skali roku, a nie od zera.** Kwota wolna dzieli się na miesiące (`pit_withheld` bierze numer wypłaty w roku) | Przy czystej metodzie narastającej pierwsze pół roku byłoby bez podatku, a od sierpnia potrójne — bo kwota wolna zjadałaby całą dotychczasową podstawę. Pomiar: przy skali z `tax.ron` i medianie dochodu gospodarstwa w mieście 3 tys. mieszkańców pierwsza zaliczka wypadała **zero** przez cztery miesiące. Własność, na której stoi `R5`, zostaje bez zmian: przy dwunastej wypłacie skalowanie jest tożsamością, więc suma zaliczek równa się podatkowi rocznemu co do grosza |
| `CA-7` | **Cła nie liczy miasto i liczyć nie będzie.** `sim/supply` dolicza je przy wycenie importu (`ImportQuote.duty`) i wsadza w koszt nabycia partii (`K-36`); miasto dostaje stąd fakt i zabiera pieniądz **z kanału importowego**, a nie drugi raz od importera | Importer zapłacił już przy odprawie (`payable() = net + duty`). Rozdzielenie tej zapłaty na dwa przelewy w `absorb_settlements` dołożyłoby stan pośredni „zapłacone cło, niezapłacony towar" w miejscu, które już dziś ma gałąź „przyjęcie zawsze, zapłata gdy jest z czego". Stawki pozostają w `data/trade/tariffs.ron` (`K-37`) i od tej chwili są **polityką miasta**, mimo że plik ma cudzy adres |
| `CA-8` ★ | **Akcyza nalicza się przy zmianie właściciela wyrobu, a nie przy opuszczeniu zakładu.** Tabela kontraktu księgowania mówiła „opuszczenie zakładu produkcyjnego / odprawa importowa" i „`Dr Inventory` — wchodzi w koszt towaru" | Zdarzenia „wyrób opuszcza linię" nie ma po stronie M6 jako punktu, w którym da się coś zaksięgować; są za to dwa punkty, w których wyrób zmienia właściciela: sprzedaż detaliczna (`record_sale`) i rozliczenie hurtowe (`absorb_settlements`). Akcyza idzie w koszt **okresu** (`Dr TaxExpense`), a nie w koszt zapasu. Skutek dla gracza jest ten sam i ten sam jest wymóg `T7`: cenę podnosi polityka marżowa firmy, a nie zdarzenie podatkowe |
| `CA-9` ★ | **Akcyza w tym mieście nie ma czego obłożyć i to jest `R2` nazwany wprost.** Obłożone są paliwa, piwo i papierosy; z tych trzech na półce stoi tylko piwo — i przegrywa z sokiem, bo w kategorii `Drink` sok jest pierwszy w rangach substytutu (`data/economy/retail.ron`). Paliwo kupują pojazdy przez `FuelLedger` (M4), który nie ma jeszcze konta w księgach; papierosów nie ma w asortymencie detalicznym | Mechanizm jest wpięty w dwa prawdziwe punkty i pokryty testem jednostkowym; **wolumenu nie ma**. Droga wyjścia jest jedna i ma adres: **akcyza od energii na rachunku za media (M8b)**, gdzie wolumen jest codzienny i dotyczy każdego zakładu. `ExciseClass::Energy` jest w PRD §6.8 od początku — brakowało jej sieci, a nie stawki |
| `CA-10` | **Miasto jest zasobem świata, nie encją-singletonem** (rozstrzygnięcie `D4` fazy odwrotnie do propozycji) | Uzasadnienie w `D4` brzmiało „żeby mieściła się w haszu stanu i snapshotcie bez wyjątków", a `World::register_resource_hash` (`K-29`) daje dokładnie to samo i robi to już dla `Books`, `Market`, `Firms` i `CorpFinance`. Encja z jednym egzemplarzem kosztowałaby archetyp, bufor komend i pytanie „którą", nie kupując niczego |
| `CA-11` | **Podatek od nieruchomości płaci zakład stojący na parceli, a nie właściciel gruntu** | Generator oddaje całą mieszkaniówkę i wszystkie instytucje **miastu** (`ParcelOwner::City`), a miasto nie opodatkowuje samo siebie; realnym właścicielem prywatnym jest w Etapie 7 wyłącznie firma stojąca na parceli przemysłowej albo handlowej. Podatek od mieszkania czeka na rynek nieruchomości, którego w tej fazie nie ma i który do niej nie należy. `ponytail:` sufit w `city::world`: podstawą jest sama ziemia (`land_value_per_m2 × area_m2`), bez wartości budynku |
| `CA-16` | **Ściągalność rozkłada się nierówno i to jest wynik, nie usterka.** Przebieg roczny: PIT i cło **100 %**, VAT **99,5 %**, podatek od nieruchomości **44 %**, koncesje **56 %** | Pierwsze trzy płaci się **cudzą gotówką**: PIT potrąca się u źródła, cło jest zapłacone przy odprawie, VAT przechodzi przez kasę sklepu. Dwie ostatnie firma płaci z własnej i tam zaczynają rosnąć zaległości z odsetkami — czyli dokładnie tam, gdzie mechanizm zaległości ma się objawić. **Czego to nie rozstrzyga:** czy 25 punktów bazowych od wartości katastralnej to za dużo dla tej gospodarki. To jest pytanie do balansatora (**M8e**), a nie do tej podfazy |
| `CA-15` | **Zakładka danin w karcie firmy stoi w `engine/ui`, a nie w migawce `sim/firms`.** `FirmCard::with_taxes(c, l, &[TaxCharge])` dokłada piątą zakładkę osobnym wywołaniem | `FirmPanelSnapshot` składa `sim/firms`, a ten crate **nie widzi miasta** i widzieć nie może: zależność idzie `city → economy → firms`, a odwrócenie zamknęłoby cykl. Kartę składa więc ten, kto widzi obie strony — warstwa prezentacji. Przy okazji `sim/city` **przestaje zależeć od `sim/world`**: budowa katastru z parceli generatora przeniosła się do mostu w `tools/headless`, bo wiąże dwie strony, których żadna nie widzi drugiej, a most widzi obie |
| `CA-13` | **Cło ściąga się z kanału importowego, a nie z konta importera — i to była pomyłka do złapania dopiero przebiegiem.** Pierwsza wersja rozliczenia brała cło z konta zakładu, bo płatnikiem w rejestrze jest zakład | Importer płaci cło **przy odprawie**, razem z zapłatą za towar (`payable() = net + duty`), i kwota siedzi w koszcie nabycia partii. Ściągnięcie go drugi raz obciążało firmę podwójnie. Płatnik i źródło pieniądza rozchodzą się w tej jednej daninie i nigdzie indziej: karta inspekcji pokazuje importera, bo to on poniósł koszt, a przelew idzie z „reszty świata". Z tego samego powodu cło **nie** przechodzi przez `post_tax_payment`: nie ma zobowiązania do zdjęcia z `TaxPayable`, bo nigdy tam nie stanęło |
| `CA-14` | **CIT w przebiegu rocznym wyszedł zerem i to jest odpowiedź, a nie brak odpowiedzi.** Wszystkie 73 zakłady handlowe zamknęły rok stratą (razem 31,2 mln zł), więc podatku nie ma, a strata idzie do rozliczenia w latach następnych. Raport scenariusza wypisuje tę liczbę wprost | Danina zerowa i danina niewpięta wyglądają w raporcie identycznie (`R2`), więc scenariusz musi umieć pokazać, **z czego** wyszło zero. **Czego to nie rozstrzyga:** czy strata bierze się z podatków, czy z gospodarki sprzed M8a. Skala jest zbyt duża na podatki (428 tys. zł straty na zakład rocznie wobec ~7 tys. zł podatku od nieruchomości), ale VAT podnosi cenę półkową i zbija wolumen, więc część na pewno stąd jest. Pomiar różnicowy (`m7miasto` wobec `m8miasto` na tym samym ziarnie) należy do panelu balansatora, czyli do **M8e** |
| `CA-12` | **Scenariusz `m8miasto` stoi obok `m7miasto`, a nie zamiast niego**; `full::setup` nie stawia miasta jako aktora | Włączenie siedmiu danin zmienia świat, na którym skalibrowano bramki G1–G9 — a kalibracja należy do panelu balansatora, czyli do M8e. Ta sama kolejność co przy `m7labor` obok `m5shop`: zszycie przychodzi z ostatnią podfazą fazy |
