# M8 — Miasto jako aktor

Status: plan fazy. Podlega `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, LOD, dane, testy).
Crate'y tworzone: `sim/city`, `sim/events`. Crate rozszerzany: `sim/traffic` (sieci przesyłowe).

---

## 1. Cel fazy i artefakt końcowy

Po M8 miasto przestaje być scenografią i staje się **drugim graczem**: ma własny budżet,
własne cele, własne decyzje i własne narzędzia nacisku. Świat przestaje być gładki:
pada deszcz albo go nie ma, elektrownia się psuje, ludzie strajkują, a urząd ma kolejkę.

Artefakt uruchamialny (headless + nakładki debug):

1. **Miasto pobiera podatki i wydaje pieniądze.** Każda transakcja detaliczna nalicza VAT,
   każda wypłata nalicza PIT, każdy rok obrotowy firmy nalicza CIT, każda parcela płaci
   podatek od nieruchomości. Gracz widzi każde obciążenie w swojej księdze z nazwą, stawką
   i okresem. Budżet miasta domyka się do grosza.
2. **Miasto rządzi.** Burmistrz AI z preferencjami (prorozwojowy / socjalny / ekologiczny /
   populistyczny) co miesiąc podejmuje decyzje: zmienia stawki, ogłasza przetarg, wydaje
   pozwolenia, wprowadza regulację, przyznaje dotację — każda decyzja z `DecisionReason`.
3. **Wybory działają.** Co kadencję mieszkańcy głosują (frekwencja i preferencje zależne od
   statusu, nastroju, dzielnicy i jakości usług w ich okolicy). Wynik zmienia politykę.
   Gracz może wspierać kandydata — legalnie i nielegalnie, z ryzykiem.
4. **Usługi publiczne mają skutki.** Szkoła podnosi umiejętności dzieci w obwodzie, szpital
   skraca absencję, policja obniża straty w sklepach (widoczne w rachunku wyników gracza),
   urząd wydaje pozwolenie po **realnym czasie** wynikającym z obsady i kolejki.
5. **Sieci przesyłowe żyją.** Elektrownia → linie → transformatory → odbiorcy. Awaria źródła
   albo przeciążenie linii prowadzi do **blackoutu**: zakład bez prądu stoi, a przestój widać
   w kosztach i w niedostarczonych kontraktach. Woda, gaz, ciepło, telekomunikacja — ten sam
   solver, inne jednostki, każda sieć jako firma z taryfą.
6. **Zdarzenia wynikają ze stanu świata.** Susza nie jest wpisem w kalendarzu — jest skutkiem
   deficytu opadów. Awaria elektrowni jest skutkiem wieku bloku, zaległej konserwacji i
   miesięcy pracy na 98% mocy. Strajk jest skutkiem rozjazdu płac i zysku. Zdarzenie zmienia
   **parametry**; ceny zmieniają się same, w M5/M6/M7.
7. **Pogoda i pory roku pracują cały czas** — na popyt na ciepło, na plony, na prędkość ruchu,
   na obciążenie sieci.

**Demo fazy (`tools/headless --scenario m8-demo`):** rok gry na mieście 20 tys. mieszkańców.
Log pokazuje: mroźny tydzień → skok poboru ciepła i prądu → przeciążenie linii do dzielnicy
przemysłowej → wyłączenie → dwie fabryki stoją 4 godziny → kara umowna u odbiorcy → wzrost
taryfy w kolejnym okresie rozliczeniowym → spadek poparcia dla burmistrza w tej dzielnicy →
przegrana w wyborach w tym obwodzie. Ani jeden z tych kroków nie jest zaprogramowany jako skutek.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zawartość | PRD |
|---|---|---|
| Władza miejska AI | burmistrz i rada, cele, preferencje, pętla decyzyjna, regulacje, dotacje, inwestycje | §10.1 |
| Budżet i przepływy publiczne | `CityBudget`, księga publiczna, dług miejski, inwestycje kapitałowe | §6.8, §10.1 |
| Podatki | CIT, PIT, VAT (detal), od nieruchomości, akcyza, cła, koncesje — z pełnym śladem w księgowości | §6.8, §6.9 |
| Pozwolenia i przetargi | `Permit` z kolejką urzędu, `Tender` z kryteriami i ofertami | §10.1, §10.3 |
| Wybory | frekwencja i preferencje per mieszkaniec, kandydaci, wsparcie gracza (legalne i nie) | §10.2 |
| Usługi publiczne | szkoły, szpitale, policja, straż, odpady, parki, urzędy — z mierzalnym skutkiem | §10.3 |
| Prawo i egzekucja | antymonopol, inspekcja pracy, sanepid, ochrona środowiska, skarbówka, szara strefa | §10.4, §7.3 |
| Sieci przesyłowe | prąd (przepływ, przeciążenie, blackout, kaskada), gaz, woda/kanalizacja, ciepło, telekom; taryfy | §9.6, §7.3 |
| Generator zdarzeń | hazard zależny od stanu świata, `ParamPatch`, katalog 6 kategorii jako dane | §11.1, §11.2 |
| Systemy dynamiczne stałe | pory roku i pogoda, zegar epoki, wskaźniki cyklu koniunkturalnego, parametry demografii | §11.3 |

### Nie wchodzi

| Czego nie robię | Kto robi | Co zamiast tego |
|---|---|---|
| Reakcje firm na podatki, ceny, regulacje, blackout | **M7** | emituję parametry i obciążenia, firma decyduje sama |
| Księga główna firmy (konta, RZiS, bilans) | **M5** (`sim/economy`) / **M7** | emituję `TaxCharge` do bufora; księgowanie robi właściciel księgi |
| Media jako firmy, kształtowanie opinii | **M10** | `MediaExposure` to pole wejściowe wyborcy; do M10 zasilane stubem (zasięg = f(dzielnica, nakład publiczny)) |
| Giełda, przejęcia, R&D, marka | **M10** | `EpochClock` publikuje dostępność technologii, nic więcej |
| Pełne panele UI miasta | **M9** | tylko inspektory `devtools` (tabela budżetu, graf sieci, log hazardów) |
| Symulacja ruchu drogowego | **M4** | konsumuję czasy przejazdu i krawędzie; modyfikuję ich prędkość parametrem |
| Model makro / historia „na sucho" | **M10** | publikuję `MacroIndicators` jako agregat obserwowalny, nie modeluję cyklu |
| Bankructwo firmy z tytułu zaległości | **M7** | wystawiam `Remedy::BackTax` i wniosek egzekucyjny; skutek majątkowy — M7 |
| Generacja terenu, klimat bazowy | **M1** | pogoda M8 to odchyłka od norm klimatycznych M1 |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Pokrycie w M8 |
|---|---|
| §6.8 Podatki i przepływy publiczne | WP2 w całości (7 danin), WP1 (budżet) |
| §6.9 Księgowość — ślad obciążeń | WP2, kontrakt `TaxCharge` → księga firmy |
| §6.7 Podatek od nieruchomości | WP2 (`property_tax_month`), podstawa z wyceny M5 |
| §7.3 Media zakładu, emisje | WP3 (przyłącza, zużycie, faktury), WP8 (kary za emisje) |
| §9.6 Sieci przesyłowe | WP3 w całości |
| §10.1 Władza miejska | WP9 (pętla decyzyjna), WP7 (pozwolenia), WP9 (przetargi, dotacje, regulacje) |
| §10.2 Wybory | WP10 |
| §10.3 Usługi publiczne | WP7 |
| §10.4 Prawo i egzekucja | WP8 |
| §11.1 Zasady zdarzeń | WP4 (hazard ze stanu, `ParamPatch`, zakaz zadawania cen) |
| §11.2 Katalog kategorii | WP5 (`data/events/`, 6 kategorii) |
| §11.3 Systemy dynamiczne | WP6 (pogoda, sezon, epoka, demografia, wskaźniki cyklu) |
| §14.3 Panel „Miasto", Kronika | WP11 (model danych + inspektor; pełny panel — M9) |
| §17.4 LOD | WP2, WP3 — mezo-odpowiedniki podatków i mediów, test salda 0 |
| §18.2 Determinizm | WP4 (arytmetyka całkowita hazardu), WP11 (hash, replay) |
| §19 M8 | zakres kamienia milowego w całości |

---

## 4. Pakiety robocze

Kolejność wynika z zależności: pieniądz publiczny → media → zdarzenia → reszta.

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

### WP3 — Sieci przesyłowe w `sim/traffic`
**Zależy od:** WP1 (taryfy jako polityka), M2 (parcele, dzielnice), M7 (operator jako firma).
Graf sieci, solver przepływu z ograniczeniami, wykrywanie przeciążeń i wysp, zrzut obciążenia
wg priorytetu, kaskada wyłączeń z limitem rund, `SupplyState` per przyłącze. Pomiar zużycia,
taryfa, faktura miesięczna jako zwykła transakcja B2B/B2C. Pięć rodzajów sieci na jednym solverze.
**Kryterium ukończenia:** test T2 (blackout zatrzymuje produkcję i jest widoczny w kosztach);
benchmark `criterion` mieści się w budżecie z sekcji 5.4.
**Rozmiar:** L

### WP4 — Rdzeń `sim/events`: hazard, `ParamPatch`, rejestr zdarzeń
**Zależy od:** WP3 (sondy stanu sieci), M1 (klimat), M3 (nastroje), M7 (stan firm).
`EventDef` ładowany z `data/events/`, `EventTrigger` z krzywymi całkowitoliczbowymi,
`Probe` z cache per tick, losowanie ze strumienia `StreamId::EventRoll`, `ParamOverlay`
nakładający modyfikatory na parametry bazowe. Zakaz zadawania cen wpisany w typ `SimParam`.
**Kryterium ukończenia:** test T3 (pełny determinizm) + test T7 (żaden wariant `SimParam` nie
dotyka oferty, ceny ani marży) zielone.
**Rozmiar:** L

### WP5 — Katalog zdarzeń jako dane
**Zależy od:** WP4.
Sześć kategorii z §11.2 w `data/events/{natural,infrastructure,external,social,firm,political}.ron`.
Minimum 8 definicji na kategorię, w tym trzy wzorcowe z sekcji 5.5 (susza, awaria elektrowni, strajk).
Walidator danych w CI: każda sonda istnieje, każdy `SimParam` jest osiągalny, krzywe monotoniczne
tam, gdzie deklarowane, `max_concurrent` i `cooldown` sensowne.
**Kryterium ukończenia:** walidator zielony; przebieg 5 lat gry generuje zdarzenia ze wszystkich
sześciu kategorii, a rozkład liczby zdarzeń na rok mieści się w widełkach z balansatora.
**Rozmiar:** M

### WP6 — Systemy dynamiczne stałe
**Zależy od:** WP4 (sondy), M1 (normy klimatyczne).
Pogoda (proces seedowany, zakotwiczony w normach M1), pory roku i kalendarz rolniczy,
`EpochClock` z `data/epochs/`, `MacroIndicators` (koszyk CPI, bezrobocie, przyrost kredytu,
średni nastrój) liczone miesięcznie jako agregat obserwowalny, `DemographyParams` dla M3.
**Kryterium ukończenia:** pogoda deterministyczna i statystycznie zgodna z normami M1
(średnia roczna temperatura ±0,5 °C, suma opadów ±10%); `MacroIndicators` publikowane
i czytelne jako sondy.
**Rozmiar:** M

### WP7 — Usługi publiczne i urzędy
**Zależy od:** WP1 (finansowanie), WP2 (koszty osobowe → PIT), M3 (mieszkańcy), M7 (rynek pracy).
Placówki jako encje z obsadą rekrutowaną **na zwykłym rynku pracy** (nie magiczny etat),
jakość z finansowania i obsady, zasięg obwodowy, skutki w M3/M5/M7. Urząd jako kolejka:
`Permit` czeka, czas oczekiwania jest emergentny.
**Kryterium ukończenia:** test T4 (skutki usług mierzalne) + test kolejki: podwojenie obsady
urzędu skraca medianę czasu wydania pozwolenia co najmniej o 40%.
**Rozmiar:** L

### WP8 — Prawo i egzekucja, szara strefa
**Zależy od:** WP2 (podstawa opodatkowania), WP4 (mechanizm hazardu — **reużyty**, nie drugi system).
Pięć urzędów, sprawy, dowody, kary i środki zaradcze. Szara strefa jako parametr firmy
(`UnreportedShareBps`) podnoszący hazard kontroli. Kontrola skarbowa domykająca zaległość
z odsetkami. Antymonopol z progiem udziału rynkowego i przymusowym podziałem.
**Kryterium ukończenia:** scenariusz „firma ukrywa 30% obrotu" kończy się kontrolą w medianie
< 3 lat gry, a domiar trafia do budżetu bez naruszenia testu T1.
**Rozmiar:** M

### WP9 — Władza miejska AI
**Zależy od:** WP1–WP3, WP7.
Cele (poparcie, saldo budżetu, rozwój), preferencje jako wagi, menu działań z heurystyką
oczekiwanego skutku, pętla miesięczna z histerezą (ochrona przed oscylacją stawek).
Inwestycje (drogi, szkoły, komunikacja), strefowanie, przetargi z kryteriami i rozstrzygnięciem,
regulacje, dotacje z limitem budżetowym.
**Kryterium ukończenia:** test T5 (brak oscylacji): w 20 latach gry bez zdarzeń zewnętrznych
żadna stawka podatkowa nie zmienia kierunku częściej niż raz na 24 miesiące.
Każda decyzja ma `DecisionReason` czytelny w inspektorze.
**Rozmiar:** L

### WP10 — Wybory
**Zależy od:** WP9, WP7 (jakość usług jako wejście), M3 (status, nastrój, dzielnica).
Kalendarz kadencji, kandydaci z programem, frekwencja i głos per mieszkaniec, podział mandatów,
wpływ na politykę. Wsparcie kandydata przez gracza: legalne (darowizna, wykup reklamy)
i nielegalne (łapówka) z ryzykiem ujawnienia jako zdarzenia politycznego.
**Kryterium ukończenia:** test T6 (determinizm wyborów) + test wrażliwości: pogorszenie jakości
usług w jednej dzielnicy o 30% obniża tam poparcie inkumbenta o co najmniej 5 pkt proc.
**Rozmiar:** M

### WP11 — Inspektory, kronika, domknięcie fazy
**Zależy od:** wszystkie.
Inspektor budżetu, inspektor sieci (graf z przepływami i przeciążeniami), log hazardów
(„dlaczego jeszcze nie ma suszy" — wartości sond i wynikowy hazard), wpisy kroniki dla zdarzeń.
Dopisanie komponentów M8 do funkcji hasza stanu ECS. Scenariusz `m8-demo`.
**Kryterium ukończenia:** Definition of Done fazy: wszystkie testy T1–T8 zielone,
`clippy -D warnings`, benchmarki w budżecie, scenariusz demo odtwarzalny z seeda.
**Rozmiar:** M

---

## 5. Projekt techniczny

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

### 5.2 Polityka, pozwolenia, przetargi, władza

```rust
pub struct Government {
    pub mayor: CitizenId,
    pub mayor_pref: Preference,
    pub council: Vec<(CitizenId, Preference)>,     // mandaty
    pub term_start: Tick,
    pub term_ticks: u64,
    pub approval_bps_by_district: Vec<u32>,        // aktualizowane miesięcznie
    pub goals: Goals,                              // { approval_w, balance_w, growth_w } — wagi z preferencji
}

pub struct Preference {                            // 4 osie z §10.1, sumują się do 10000 bps
    pub growth_bps: u32, pub social_bps: u32,
    pub green_bps: u32,  pub populist_bps: u32,
}

pub enum Policy {
    TaxRate { kind: TaxKind, bps: u32 },
    Zoning { parcel: ParcelId, use_: ZoneUse },
    TrafficRestriction(EdgeRestriction),          // patrz niżej — JEDEN wariant na wszystkie
                                                  // ograniczenia ruchowe, nie jeden na rodzaj
    EmissionLimit { pollutant: Pollutant, per_day: i64, penalty_per_unit: Money },
    TradingHours { district: DistrictId, hours: WeekMask },   // w tym zakaz handlu w niedziele
    Subsidy { target: SubsidyTarget, per_period: Money, cap: Money, sunset: Tick },
    TariffCap { kind: UtilityKind, max_per_unit: Money },
    MinWage(Money),
    ParkingFee { district: DistrictId, per_hour: Money },
    ServiceFunding { kind: ServiceKind, per_month: Money },
}

/// Siatka tygodniowa (K-15): 7 dni × 24 godziny = 168 bitów.
/// Indeksowane `DayOfWeek` z `SimCalendar` (engine/core) — M8 nie liczy dnia tygodnia sam.
pub struct WeekMask([u64; 3]);
impl WeekMask { pub fn allows(&self, d: DayOfWeek, hour: u8) -> bool; }
```

**Regulacje ruchowe — predykat na krawędziach, nie nowy profil trasy.**
Ograniczenie M4: trasy liczą się na Contraction Hierarchies z ustaloną, małą liczbą profili
(`Passenger`, `HeavyDay`, `HeavyNight`) — każdy profil to osobny zestaw wag, ~450 ms
kustomizacji i do 30 MB. **Liczba profili jest zasobem limitowanym i M8 go nie powiększa.**
Gdyby każda uchwała rady tworzyła własny profil, koszt routingu rósłby liniowo z aktywnością
polityczną miasta — czyli rósłby dokładnie wtedy, gdy rozgrywka robi się ciekawa.

Dlatego **wszystkie** ograniczenia ruchowe — zakaz ruchu ciężkiego w centrum, tonaż na moście,
strefa czystego transportu, okna dostaw w śródmieściu — to jeden typ, wyrażony jako zbiór
wyłączonych krawędzi plus okno czasowe:

```rust
pub struct EdgeRestriction {
    pub edges: EdgeSet,                     // krawędzie grafu M4, nie geometria ani dzielnica
    pub applies_to: VehicleClass,           // Heavy | Delivery | Combustion | Transit | All
    pub window: WeekMask,                   // K-15 — kiedy obowiązuje
    pub exemption: Option<PermitKind>,      // np. OversizeTransport zwalnia z zakazu
    pub fine: Money,                        // egzekucja: mandat, nie bariera fizyczna
}
```

Kompilacja do maski (`EveryHour`, tylko przy zmianie zbioru obowiązujących regulacji):

```rust
/// Wszystkie aktywne regulacje dla danej klasy pojazdu i godziny składają się
/// SUMĄ BITOWĄ w JEDNĄ maskę na istniejący profil. N regulacji = 1 maska,
/// więc koszt jest stały w liczbie uchwał, nie liniowy.
pub fn compile_edge_mask(policies: &PolicySet, class: VehicleClass,
                         day: DayOfWeek, hour: u8) -> EdgeMask;

pub struct PolicyRecord {
    pub policy: Policy,
    pub enacted_at: Tick,
    pub effective_from: Tick,        // zawsze ≥ enacted_at + okres vacatio legis
    pub sunset: Option<Tick>,
    pub vote: CouncilVote,
    pub reason: DecisionReason,      // wyjaśnialność (§7 dokumentu 00)
}

// ---------- pozwolenia: czas jest kosztem ----------

pub struct Permit {
    pub id: PermitId,
    pub applicant: Applicant,
    pub kind: PermitKind,            // Build | ChangeOfUse | Demolition | EnvClearance
                                     // | AlcoholLicense | FoodService | RoadAccess | OversizeTransport
    pub parcel: Option<ParcelId>,
    pub filed_at: Tick,
    pub status: PermitStatus,        // Queued | UnderReview | RequestForInfo{deadline}
                                     // | Approved{at} | Rejected(RejectReason) | Expired
    pub fee: Money,
    pub office: OfficeId,
    pub priority: PermitPriority,    // Standard | Expedited(opłata) | Political(interwencja)
}
```

Czas wydania pozwolenia **nie jest parametrem** — wynika z obsady i kolejki:

```rust
/// EveryDay. throughput = Σ po urzędnikach floor(skill/10) * digitization_mul_bps/10000,
/// pomnożone przez 0 w dni wolne urzędu (K-15: `PermitOffice.open_days: WeekMask`).
/// Kolejka FIFO w obrębie klasy priorytetu; sprawy złożone (EnvClearance) kosztują 5 jednostek.
/// Skutek uboczny zamierzony: wniosek złożony w piątek leży dwa dni — czas gracza to koszt.
fn sys_process_permit_queue(offices: &mut [PermitOffice], permits: &mut PermitRegistry,
                            day: DayOfWeek, tick: Tick);
```

```rust
pub struct Tender {
    pub id: TenderId,
    pub subject: TenderSubject,      // BusLine(LineId) | WasteCollection(DistrictId)
                                     // | Construction(ProjectId) | Maintenance(AssetId)
    pub published_at: Tick,
    pub bid_deadline: Tick,
    pub criteria: BidCriteria,       // wagi bps: cena / jakość / termin / lokalność
    pub bids: Vec<Bid>,
    pub outcome: Option<TenderOutcome>,   // { winner: FirmId, contract: ContractId }
}

pub struct Bid {
    pub bidder: FirmId, pub price: Money, pub quality_promise: Q,
    pub delivery_ticks: u64,
    pub kickback: Option<Money>,     // nielegalne; podnosi hazard kontroli i skandalu
}
```

Pętla decyzyjna burmistrza (`EveryMonth`): ocena celów → wybór działania z menu
z heurystyką oczekiwanego skutku (nie optymalizacja, nie solver) → uchwalenie z terminem
wejścia w życie. **Histereza obowiązkowa:** zmiana stawki wymaga odchylenia od celu
powyżej progu utrzymującego się 3 miesiące — inaczej AI oscyluje.

### 5.3 Usługi publiczne i egzekucja

```rust
pub struct PublicService {
    pub kind: ServiceKind,           // School(Level) | Hospital | Clinic | Police | Fire
                                     // | WasteCollection | Park | Office(OfficeKind) | Transit
    pub site: SiteId,                // zwykły budynek na parceli (M2)
    pub capacity: u32,               // uczniów / łóżek / rewirów / ton na tydzień
    pub staff_target: u32,
    pub staff: Vec<CitizenId>,       // rekrutowani na ZWYKŁYM rynku pracy (M7)
    pub funding_per_month: Money,
    pub condition: Q,                // stan budynku, degraduje bez konserwacji
    pub quality: Q,                  // emergentna, aktualizowana EveryMonth
    pub utilization_bps: u32,        // przeciążenie obniża jakość
    pub catchment: DistrictId,
    pub open_days: WeekMask,         // K-15: urząd i szkoła mają weekend, szpital i policja nie
}

/// EveryMonth. quality = f(funding/obsługiwaną osobę, obsada/etaty, condition, przeciążenie)
fn sys_update_service_quality(services: &mut [PublicService], budget: &CityBudget);

/// EveryMonth. Publikuje odczyt dla innych faz: zanik z odległością po czasach przejazdu (M4).
fn sys_publish_service_coverage(services: &[PublicService], out: &mut ServiceCoverage);

pub struct ServiceCoverage {                 // per dzielnica, czytane przez M3/M5/M7
    pub education: Vec<Q>, pub health: Vec<Q>, pub safety: Vec<Q>,
    pub fire_response: Vec<Q>, pub sanitation: Vec<Q>, pub leisure: Vec<Q>,
}
```

Kanały skutków (emitowane jako parametry, nie jako liczby narzucone innym fazom):

| Usługa | Parametr wyjściowy | Kto konsumuje |
|---|---|---|
| Szkoła | `SkillGrowthMulBps{district, age_band}` | M3 (rozwój dzieci) |
| Szpital / przychodnia | `RecoveryRateMulBps`, `AbsenteeismBps{district}` | M3, M7 (produktywność) |
| Policja | `ShrinkageBps{district}` → straty w sklepach | M5/M7 (koszt w RZiS!) |
| Straż | `FireHazardMulBps{district}` → mnożnik hazardu zdarzenia pożaru | M8 (`sim/events`) |
| Odpady | `PollutionDelta{district}` przy zaległościach w wywozie | M8, M5 (wartość gruntu) |
| Parki | `LeisureSatisfaction`, `LandValueMulBps` | M3, M5 |
| Urząd | czas wydania pozwolenia (emergentny) | gracz, M7 |

```rust
pub struct Agency {
    pub kind: AgencyKind,            // Antitrust | LaborInspection | Sanitary | Environment | TaxOffice
    pub budget: Money, pub inspectors: u32,
    pub open_cases: Vec<CaseId>,
}

pub struct Case {
    pub id: CaseId, pub subject: FirmId, pub agency: AgencyKind,
    pub opened_at: Tick, pub evidence: Q,
    pub finding: Option<Finding>, pub remedy: Option<Remedy>,
}

pub enum Remedy {
    Fine(Money),
    Closure { until: Tick },                         // sanepid zamyka restaurację
    LicenseRevoked(LicenseClass),
    ForcedDivestiture { share_bps: u32 },            // antymonopol
    BackTax { amount: Money, interest: Money },      // skarbówka → nowy TaxCharge
    Injunction(Policy),                              // nakaz dostosowania (np. filtr)
}
```

**Szara strefa** to parametr firmy `UnreportedShareBps` (ustawiany przez gracza albo przez AI
firmy z M7). Obniża podstawę VAT/CIT/PIT i **podnosi hazard kontroli skarbowej** przez sondę
`DeclaredVsExpectedGapBps`. Kontrola używa **tego samego mechanizmu hazardu co zdarzenia**
(WP4) — nie budujemy drugiego losowania.

### 5.4 Sieci przesyłowe (`sim/traffic`)

```rust
pub enum UtilityKind { Power, Gas, Water, Sewage, Heat, Telecom }

pub struct UtilityNetwork {
    pub kind: UtilityKind,
    pub operator: FirmId,                 // sieć JEST firmą (§9.6) — z taryfą i księgowością
    pub nodes: Vec<UtilityNode>,
    pub edges: Vec<UtilityEdge>,
    pub topo: TopologyCache,              // przebudowa tylko przy zmianie topologii
    pub tariff: Tariff,
}

pub struct UtilityNode {
    pub site: Option<SiteId>,
    pub role: NodeRole,                   // Source{capacity, min_stable, online}
                                          // | Transformer{capacity} | Connection{priority}
    pub demand: i64,                      // W / ml·h⁻¹ / Wh·h⁻¹ / Mb·s⁻¹ — jednostki całkowite
    pub supplied: i64,
    pub state: SupplyState,               // Ok | Shed | Isolated | Faulted
    pub metered: i64,                     // licznik narastająco do faktury
}

pub struct UtilityEdge {
    pub a: u32, pub b: u32,
    pub capacity: i64, pub loss_bps: u32,
    pub state: EdgeState,                 // Ok | Tripped{until} | UnderMaintenance
}

pub struct TopologyCache {                // budowany przy TopologyDirty, nie co tick
    pub islands: UnionFind,
    pub spanning_forest: Vec<u32>,        // rodzic każdego węzła
    pub post_order: Vec<u32>,             // kolejność sumowania poddrzew
    pub loop_groups: Vec<LoopGroup>,      // składowe 2-spójne skolapsowane do superwęzłów
}

pub struct Tariff {
    pub standing_charge_per_month: Money,
    pub per_unit: Money,                  // za kWh / m³ / GJ / GB — cena za 1000 jednostek bazowych
    pub peak_multiplier_bps: u32,
    pub connection_fee: Money,
}
```

**Model przepływu — wystarczający do blackoutu, nie więcej.**

```rust
pub fn solve_network(net: &mut UtilityNetwork, rng: RngKey) -> SolveReport;

pub struct SolveReport {
    pub shed_nodes: Vec<u32>, pub tripped_edges: Vec<u32>,
    pub unserved: i64, pub cascade_rounds: u8,
}
```

Algorytm, jedna runda:
1. **Wyspy.** Union-find z cache; przelicz tylko gdy `TopologyDirty`.
2. **Bilans wyspy.** `supply = Σ źródeł online`, `demand = Σ węzłów odbiorczych`
   (strata na krawędziach doliczana jako `demand * loss_bps`).
3. **Zrzut obciążenia.** Gdy `demand > supply`: odłączaj węzły w porządku
   `(priority, node_index)` — rosnąco po priorytecie — aż do bilansu. Szpital i wodociąg
   mają priorytet 0, gospodarstwa domowe 2, przemysł 3. Porządek jest w pełni deterministyczny.
4. **Przepływy na krawędziach.** Na drzewie rozpinającym: przepływ krawędzi = suma popytu
   w jej poddrzewie, liczona jednym przejściem po `post_order` — **O(E)**.
   Pętle (składowe 2-spójne) kolapsowane do superwęzła o przepustowości = suma jego krawędzi.
5. **Zadziałanie zabezpieczeń.** `flow > capacity` → krawędź `Tripped` (czas naprawy
   losowany ze `StreamId::GridFault`). To zmienia topologię → **runda kolejna** = kaskada.
6. **Limit 8 rund/tick.** Po wyczerpaniu: reszta niezbilansowanych węzłów → `Shed`.
   Limit chroni przed pętlą i daje twardy górny koszt.

> `ponytail:` przepływ po drzewie rozpinającym, pętle jako superwęzły. Sufit: nie modeluje
> rozpływu mocy w oczku ani jałowej. Ścieżka wyjścia, gdyby przeciążenia okazały się
> nierealistyczne: liniowy rozpływ DC (macierz B, rozkład LU cache'owany na topologię) —
> ten sam interfejs `solve_network`, dziesięciokrotnie droższy.

**Częstotliwość i koszt.**

| Sieć | Tick | Uzasadnienie |
|---|---|---|
| Power | `EveryMinute` | blackout musi być ostry; bufora nie ma |
| Heat, Gas | `EveryHour` | bufor w rurociągu i bezwładność cieplna budynku |
| Water, Sewage | `EveryHour` | zbiorniki wyrównawcze |
| Telecom | `EveryHour` | brak przepustowości = degradacja, nie zatrzymanie |

Budżet wydajności dla metropolii 400 tys. (cel z §17.7): ~5000 węzłów i ~6000 krawędzi
w sieci energetycznej. Runda = O(N+E) ≈ 11 tys. operacji całkowitych.
Typowo 1 runda, w kaskadzie ≤ 8. **Cel: < 0,3 ms na tick dla wszystkich pięciu sieci łącznie**,
mierzony `criterion`. Union-find i drzewo przebudowywane wyłącznie przy zmianie topologii
(nowe przyłącze, awaria) — kilka razy na dobę gry, nie 1440 razy.

**Skutek dla zakładu (kontrakt dla M7):**

```rust
pub fn supply_state(site: SiteId, kind: UtilityKind) -> SupplyState;
pub fn power_available(site: SiteId) -> bool;     // skrót — najczęstsze pytanie
```

Zakład bez prądu: linie produkcyjne stają (`SiteHalted { reason: NoUtility(Power) }`),
koszty stałe biegną dalej, płace biegną dalej, chłodnia przestaje chłodzić (M6: psucie),
kasa w sklepie nie działa. **To M7 decyduje, co firma z tym zrobi** — M8 tylko wystawia stan.

**Rozliczenie mediów:** `metered` narasta co tick; `EveryMonth` operator wystawia fakturę
jako zwykłą transakcję (M5), z pełnym śladem w księdze odbiorcy. Miasto może ograniczyć
taryfę przez `Policy::TariffCap` — z emergentnym skutkiem (operator tnie konserwację →
rośnie sonda `MaintenanceBacklogDays` → rośnie hazard awarii; to pętla, nie skrypt).

### 5.5 Generator zdarzeń (`sim/events`)

**Zasada nienegocjowalna (§11.1):** zdarzenie zmienia **parametry symulacji**.
Zdarzenie nie zna słowa „cena".

```rust
pub struct EventDef {
    pub key: String,                       // stabilny klucz tekstowy (zapis gry trzyma klucz)
    pub category: EventCategory,           // Natural | Infrastructure | External | Social | Firm | Political
    pub scope: EventScope,                 // World | District | Parcel | Site | Firm | Network | RoadEdge
    pub trigger: EventTrigger,
    pub severity: SeveritySpec,            // zakres losowania siły, w bps
    pub duration: DurationSpec,            // Fixed | RangeDays | UntilRepaired | UntilResolved
    pub effects: Vec<PatchSpec>,           // skala efektu ∝ severity
    pub cooldown_days: u32,
    pub max_concurrent: u8,
    pub chronicle: ChronicleTemplate,      // wpis do Kroniki (§14.3)
}

pub struct EventTrigger {
    pub base_hazard_ppm_per_day: u32,      // szansa bazowa: na milion, na dobę, na instancję zakresu
    pub gate: Vec<Precondition>,           // twarde warunki: sezon, epoka, typ terenu, istnienie obiektu
    pub factors: Vec<HazardFactor>,
}

pub struct HazardFactor {
    pub probe: Probe,
    pub curve: Curve,                      // łamana: Vec<(x: i64, mul_bps: i64)>, interpolacja liniowa na i64
}
```

**Ocena hazardu — wyłącznie arytmetyka całkowita:**

```rust
/// hazard = base * Π curve_i(probe_i) / 10000^n, clamp 0..=1_000_000
pub fn hazard_ppm(def: &EventDef, ctx: &ProbeCtx, scope: ScopeInstance) -> u32;

/// Losowanie: bez stanu, bez globalnego RNG, bez f64.
pub fn roll(seed: u64, def_idx: u16, scope_idx: u32, tick: Tick, hazard_ppm: u32) -> bool {
    // rng(seed, StreamId::EventRoll, mix(def_idx, scope_idx), tick.0) % 1_000_000 < hazard_ppm
}
```

**Sondy (`Probe`)** — jedyne wejście generatora do świata, tylko do odczytu, kwantowane do `i64`:

```rust
pub enum Probe {
    // klimat i teren (M1 + WP6)
    RainfallDeficit30d(DistrictId), RainfallDeficit90d(DistrictId), SoilMoistureBps(ParcelId),
    AirTempTenthsC, WindSpeed, SnowCoverMm, SeasonIndex, RiverLevelBps(RiverId),
    // infrastruktura (WP3)
    GeneratorLoadFactorBps(SiteId), MaintenanceBacklogDays(SiteId), EquipmentAgeMonths(SiteId),
    SparePartsCoverDays(SiteId), NetworkReserveMarginBps(UtilityKind), EdgeUtilisationBps(UtilityKind, u32),
    // firma i praca (M7)
    WageGapBps(FirmId), WorkerMoodMean(FirmId), UnionDensityBps(FirmId),
    RecentLayoffs90d(FirmId), SafetyInvestmentBps(SiteId), DeclaredVsExpectedGapBps(FirmId),
    // społeczeństwo i miasto (M3 + WP6 + WP7)
    UnemploymentBps(DistrictId), CrimeIndex(DistrictId), PollutionIndex(DistrictId),
    FoodShareOfIncomeBps(DistrictId), MoodMean(DistrictId), ApprovalBps(DistrictId),
    ServiceQuality(ServiceKind, DistrictId),
    // świat zewnętrzny (WP6 + M6)
    ImportDependencyBps(GoodId), EpochYear, MarketHeatBps, CreditGrowthBps,
}
```

`ProbeCtx` buduje się raz na partię oceny i **cache'uje wartość per (Probe, tick)** w `BTreeMap` —
sonda `UnemploymentBps(d)` używana przez 12 definicji liczy się raz.

**Efekty — `SimParam` i jego twarde ograniczenie:**

```rust
pub struct ParamPatch { pub param: SimParam, pub op: PatchOp, pub value: i64 }
pub enum PatchOp { MulBps, AddAbs, SetBool, Clamp }

pub enum SimParam {
    // wydajność i dostępność
    CropYieldMulBps { crop: GoodId, scope: Scope },
    MachineOutputMulBps { site: SiteId },
    GeneratorOnline { site: SiteId },
    EdgeCapacityMulBps { net: UtilityKind, edge: u32 },
    StorageIntegrityBps { site: SiteId },
    // praca i ludzie
    LaborAvailableBps { site: SiteId },
    AbsenteeismBps { district: DistrictId },
    SkillGrowthMulBps { district: DistrictId, age_band: u8 },
    // ruch i logistyka
    TravelSpeedMulBps { edge: RoadEdgeId },
    EdgeClosed { edge: RoadEdgeId },
    ImportNodeThroughputMulBps { node: ImportNodeId },
    // potrzeby i preferencje (moda, epidemia, festyn)
    NeedWeightMulBps { need: NeedId, scope: Scope },
    BrandTrustDelta { firm: FirmId, delta: i16 },
    // świat zewnętrzny — TU wolno dotknąć ceny, bo to nie jest cena w mieście
    ExternalPriceMulBps { good: GoodId },
    DutyBpsOverride { class: TariffClass },
}
```

> **Niezmiennik wpisany w typ:** `SimParam` nie ma i nie będzie miał wariantu ustawiającego
> cenę oferty w mieście, marżę firmy, wolumen sprzedaży ani popyt w sztukach.
> Wolno zmienić plon, przepustowość, dostępność, wagę potrzeby, prędkość i cenę **importu**.
> Test T7 pilnuje tego mechanicznie.

**Rejestr aktywnych zdarzeń i nakładka parametrów:**

```rust
pub struct WorldEvent {
    pub id: EventId, pub def: u16, pub scope: ScopeInstance,
    pub started_at: Tick, pub ends_at: Option<Tick>,
    pub severity_bps: u32,
    pub cause: EventCause,             // Hazard | Chained{parent: EventId} | Policy | Player
    pub patches: Vec<PatchHandle>,
}

/// Wartość efektywna = wartość bazowa ∘ iloczyn aktywnych patchy.
/// Przeliczane przy ZMIANIE zbioru aktywnych zdarzeń, nie co tick.
/// Kolejność składania: sortowanie po (SimParam, EventId) — nigdy po HashMap.
pub struct ParamOverlay { /* ... */ }
pub fn effective<T: Param>(overlay: &ParamOverlay, key: T::Key, base: T::Value) -> T::Value;
```

**Harmonogram oceny:** zdarzenia wolne (naturalne, społeczne, polityczne, zewnętrzne) —
`EveryDay`. Zdarzenia szybkie (awaria maszyny, awaria sieci, wypadek na drodze) — `EveryHour`.
Zakres `Site`/`Firm` oceniany tylko dla obiektów, które przeszły `gate` — typowo 2–5%
populacji obiektów.

---

#### 5.5.1 Trzy przykłady mechanizmu „prawdopodobieństwo zależne od stanu"

**A. Susza** (`natural/drought`, zakres: `District`, ocena `EveryDay`)

| | |
|---|---|
| **Gate** | `SeasonIndex ∈ {wiosna, lato}`, dzielnica zawiera parcele rolne |
| **Base** | 60 ppm/dobę |
| **Sondy → krzywe** | `RainfallDeficit30d`: 0 mm → ×1,0; 40 mm → ×3,0; 80 mm → ×9,0; 120 mm → ×20,0 · `AirTempTenthsC`: 200 → ×1,0; 300 → ×2,5 · `SoilMoistureBps` (mediana parcel rolnych): 5000 → ×1,0; 1500 → ×4,0 · `RiverLevelBps`: 10000 → ×1,0; 4000 → ×2,0 |
| **Severity** | 2000–9000 bps, rosnąco z deficytem opadów |
| **Zmieniane parametry** | `CropYieldMulBps{crop, district} ×= (10000 − severity·k)` per uprawa (zboża wrażliwsze niż okopowe) · `SoilMoistureBps` w dół · `WaterSourceYieldMulBps` dla ujęć · `FireHazardMulBps{district}` w górę (sprzężenie z pożarem lasu) |
| **Czas trwania** | `UntilResolved` — kończy się, gdy `RainfallDeficit30d < 20 mm` |

**Oczekiwana kaskada (żaden krok nie jest zaprogramowany w M8):** niższy plon → gospodarstwo
zbiera mniej ton (M6, receptury) → młyn nie domyka kontraktu, kupuje na spocie (M6) →
licytacja pozostałego zboża podnosi cenę hurtową → piekarnia widzi wyższy koszt jednostkowy →
polityka marżowa (M7 §6.3) podnosi cenę bułki → mieszkaniec liczy użyteczność (M5 §6.4),
część przechodzi na substytut albo ogranicza zakup → rośnie zapotrzebowanie na import zboża →
węzeł importowy nasyca przepustowość i cena zewnętrzna rośnie (M6 §6.2) → jeśli utrzyma się
kwartał, `FoodShareOfIncomeBps` rośnie → podnosi hazard zdarzenia `social/protest`
i obniża `ApprovalBps` → burmistrz rozważa dotację do żywności albo obniżkę VAT na chleb (WP9).
**M8 nie zapisał ani jednej ceny.**

**B. Awaria elektrowni** (`infrastructure/power_plant_failure`, zakres: `Site`, ocena `EveryHour`)

| | |
|---|---|
| **Gate** | zakład jest źródłem w `UtilityNetwork{Power}` i `online == true` |
| **Base** | 4 ppm/godzinę |
| **Sondy → krzywe** | `EquipmentAgeMonths`: 0 → ×1,0; 240 → ×2,0; 480 → ×5,0 · `MaintenanceBacklogDays`: 0 → ×1,0; 30 → ×2,5; 180 → ×8,0 · `GeneratorLoadFactorBps`: 8000 → ×1,0; 9500 → ×2,0; 10000 → ×4,0 · `AirTempTenthsC`: 250 → ×1,0; 350 → ×1,8 (chłodzenie) · `SafetyInvestmentBps`: wysokie → ×0,6 |
| **Severity** | 3000–10000 bps (awaria jednego bloku vs. całego zakładu) |
| **Zmieniane parametry** | `GeneratorOnline{site} = false` (albo `SourceCapacityMulBps` przy severity < 7000) |
| **Czas trwania** | `UntilRepaired` — **naprawa wymaga części z łańcucha dostaw (M6) i ekipy (M7)**; brak części w mieście = dłuższy postój |

**Oczekiwana kaskada:** źródło znika z bilansu wyspy → `solve_network` w tym samym ticku
stwierdza `demand > supply` → zrzut obciążenia wg priorytetu → dzielnica przemysłowa `Shed` →
`power_available(site) == false` → linie produkcyjne stają (M7), koszty stałe i płace biegną →
niezrealizowane kontrakty → kary umowne (M6) → jeśli zrzut nie wystarczy, sąsiednia linia
przejmuje przepływ i **przekracza przepustowość** → `Tripped` → kolejna runda → rozpad wyspy =
blackout obszarowy. Równolegle: operator kupuje prąd z importu drożej → rośnie koszt →
taryfa w kolejnym okresie w górę (M7, polityka cenowa operatora) → koszt energii rośnie
**wszystkim** firmom → część podnosi ceny → `ApprovalBps` spada → temat wyborczy.
Dodatkowo: `MaintenanceBacklogDays` po awarii rośnie, więc hazard **kolejnej** awarii jest wyższy —
pętla dodatnia, którą przerywa dopiero inwestycja operatora.

**C. Strajk** (`social/strike`, zakres: `Firm`, ocena `EveryDay`)

| | |
|---|---|
| **Gate** | firma ma ≥ 15 pracowników i istnieje od ≥ 180 dni |
| **Base** | 20 ppm/dobę |
| **Sondy → krzywe** | `WageGapBps` (zysk na pracownika vs. mediana płacy w zawodzie): 0 → ×1,0; 3000 → ×3,0; 8000 → ×10,0 · `WorkerMoodMean`: +20 → ×0,5; 0 → ×1,0; −40 → ×4,0 · `UnionDensityBps`: 0 → ×0,2; 5000 → ×1,0; 9000 → ×2,5 · `RecentLayoffs90d`: 0 → ×1,0; 10 → ×2,0 · `FoodShareOfIncomeBps` w dzielnicy (inflacja zjada płacę): ×1,0–×2,2 |
| **Severity** | 3000–10000 bps = odsetek załogi przystępującej do strajku |
| **Zmieniane parametry** | `LaborAvailableBps{site} ×= (10000 − severity)` · `HiringBlocked{firm} = true` · `MoodContagion{district}` w górę (podnosi hazard protestu i strajku u sąsiadów) |
| **Czas trwania** | `UntilResolved` — kończy się, gdy `WageGapBps` spadnie poniżej progu (firma podniosła płace) albo po wyczerpaniu funduszu strajkowego |

**Oczekiwana kaskada:** dostępność pracy spada → produkcja spada proporcjonalnie (M7) →
firma nie domyka dostaw → półki się pustoszą → klienci przechodzą do konkurencji (M5, użyteczność),
część lojalności ginie trwale (M10: marka) → firma liczy koszt przestoju i porównuje z kosztem
podwyżki (M7, decyzja) → jeśli podnosi płace, `WageGapBps` spada i hazard wygasa; jeśli nie,
zaczyna się rotacja i odejścia (M7) → inspekcja pracy dostaje sygnał (WP8, ten sam mechanizm
hazardu) → ewentualna kara → temat medialny (M10) → wpływ na wybory (WP10).

Wspólny wzorzec wszystkich trzech: **stan świata → sonda → krzywa → hazard → losowanie →
zmiana parametru → reakcja innych faz → nowy stan świata → inna sonda.** Pętla jest zamknięta,
a M8 nie zna ani jednej ceny w mieście.

### 5.6 Systemy dynamiczne stałe

```rust
pub struct Weather {                          // EveryHour, StreamId::Weather
    pub temp_tenths_c: i32, pub precip_tenths_mm: u32,
    pub wind: u16, pub cloud_bps: u32, pub snow_cover_mm: u32,
}
```

Pogoda to **odchyłka od norm klimatycznych M1** dla danego dnia roku: proces autoregresyjny
z seedowanym szumem, kotwiczony tak, żeby średnia roczna i suma opadów zgadzały się z normą.
Zastosowania: popyt na ciepło i prąd (→ obciążenie sieci → przeciążenie w mróz), plony,
prędkość ruchu (`TravelSpeedMulBps`, M4), wydajność budownictwa, sondy zdarzeń.

```rust
pub struct SeasonCalendar { /* dzień roku → sezon, kalendarz agrotechniczny, wagi potrzeb */ }
pub struct EpochClock { pub year: u16, pub unlocked: BitSet<TechId> }   // z data/epochs/
pub struct MacroIndicators {                   // EveryMonth, TYLKO agregat obserwowalny
    pub cpi_basket: Money, pub unemployment_bps: u32,
    pub credit_growth_bps: i32, pub mood_mean: Mood, pub heat_bps: u32,
}
pub struct DemographyParams {                  // wyliczane, stosuje je M3
    pub fertility_mul_bps: u32, pub mortality_mul_bps: u32,
    pub immigration_per_month: u32, pub emigration_bps: u32,
}
```

Cykl koniunkturalny **nie jest symulowany** — jest mierzony. `MacroIndicators` to odczyt stanu,
który zdarzenia zewnętrzne wzmacniają przez sondy (`CreditGrowthBps`, `MarketHeatBps`).
Model makro należy do M10.

### 5.7 Wybory

```rust
pub struct Election {
    pub scheduled_at: Tick, pub term_ticks: u64,
    pub candidates: Vec<Candidate>,
    pub result: Option<ElectionResult>,
}

pub struct Candidate {
    pub citizen: CitizenId,
    pub platform: Preference,                       // 4 osie, te same co u burmistrza
    pub tax_stance: Vec<(TaxKind, i32)>,            // deklarowana zmiana w bps
    pub funding: Money,
    pub media_reach: Q,                             // do M10 stub: f(funding, dzielnica)
    pub backers: Vec<(Backer, Money, Legality)>,    // gracz tu wchodzi
}

pub struct ElectionResult {
    pub turnout_bps: u32,
    pub per_district: Vec<DistrictTally>,
    pub council: Vec<(u8 /*idx kandydata*/, u8 /*mandaty*/)>,
    pub mayor: u8,
}

/// EveryDay w dniu wyborów. Iteracja po CitizenId rosnąco — determinizm.
/// turnout   = f(status, wiek, nastrój, staż w dzielnicy, ekspozycja medialna) → próg z RNG
/// preferencja = softmax po użyteczności kandydata: zgodność programu z interesem wyborcy
///               (podatki wobec jego dochodu), ocena usług w JEGO dzielnicy za kadencję,
///               nastrój, ekspozycja medialna, więzi osobiste (graf relacji M3)
fn sys_run_election(e: &mut Election, citizens: &CitizenView, cov: &ServiceCoverage, seed: u64);
```

Wsparcie nielegalne (`Legality::Illegal`) tworzy ukryte powiązanie, które podnosi hazard
zdarzenia `political/scandal` przez sondę — a ujawnienie uderza w kandydata i w gracza
(sprawa w `Agency::Antitrust`/prokuratura, kara, utrata koncesji).

### 5.8 Systemy ECS i ich częstotliwość

| System | Crate | Częstotliwość | Odczyt / zapis |
|---|---|---|---|
| `sys_solve_power_grid` | `traffic` | EveryMinute | R: węzły, popyt / W: `SupplyState`, `EdgeState` |
| `sys_solve_other_grids` | `traffic` | EveryHour | jw. dla gazu, wody, ciepła, telekomu |
| `sys_meter_utilities` | `traffic` | EveryMinute | W: `metered` |
| `sys_assess_vat` | `city` | na transakcji (hook M5) | W: bufor `TaxCharge` |
| `sys_assess_payroll_pit` | `city` | na wypłacie (hook M7) | W: bufor `TaxCharge` |
| `sys_assess_property_tax` | `city` | EveryMonth | R: wycena parcel / W: bufor |
| `sys_assess_excise_duty` | `city` | na zdarzeniu produkcji/importu | W: bufor |
| `sys_assess_cit` | `city` | EveryMonth (domknięcie roku firmy) | R: RZiS / W: bufor |
| `sys_settle_tax_charges` | `city` | EveryDay | W: `CityBudget`, `PaymentCmd` |
| `sys_process_permit_queue` | `city` | EveryDay | W: `Permit.status` |
| `sys_update_service_quality` | `city` | EveryMonth | W: `PublicService.quality` |
| `sys_publish_service_coverage` | `city` | EveryMonth | W: `ServiceCoverage` |
| `sys_agency_inspections` | `city` | EveryDay | W: `Case`, bufor `TaxCharge` |
| `sys_government_decide` | `city` | EveryMonth | W: `PolicyRecord`, `Tender` |
| `sys_run_election` | `city` | EveryDay (gate: dzień wyborów) | W: `Government` |
| `sys_budget_close` | `city` | EveryMonth / EveryMonth (gate: grudzień) | W: `CityBudget` |
| `sys_weather_step` | `events` | EveryHour | W: `Weather` |
| `sys_eval_hazards_slow` | `events` | EveryDay | R: sondy / W: bufor `WorldEvent` |
| `sys_eval_hazards_fast` | `events` | EveryHour | jw. |
| `sys_apply_events` | `events` | EveryHour | W: `ParamOverlay`, Kronika |
| `sys_expire_events` | `events` | EveryHour | W: `ParamOverlay` |
| `sys_macro_indicators` | `events` | EveryMonth | W: `MacroIndicators`, `DemographyParams` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam

| Typ / funkcja | Crate | Konsument |
|---|---|---|
| `CityTaxEngine: TaxEngine` — jedyna prawdziwa implementacja haka M5 | `city` | **M5** (zastępuje `NoTax`) |
| `TaxCharge`, `TaxChargeId`, `ChargeRegistry`, `ChargeState`, `TaxReason` | `city` | M5 (księga), M7 (decyzje), M9 (UI) |
| `TaxKind`, `TaxCode`, `FiscalPeriod` | `city` | M5, M6 (cło), M7 |
| `vat_from_gross`, `vat_add_to_net`, `cit_due`, `pit_withheld`, `property_tax_month`, `excise_due`, `duty_due`, `license_fee` | `city` | M5, M6, M7 |
| `CityBudget`, `SpendCategory`, `MunicipalBond` | `city` | M9 (panel), M10 (makro) |
| `Policy`, `PolicyRecord`, `PolicySet`, `fn policy_in_force(&PolicySet, Tick) -> &[Policy]` | `city` | M2 (strefowanie), M7 (regulacje), M9 |
| `EdgeRestriction`, `VehicleClass`, `fn compile_edge_mask(..) -> EdgeMask` | `city` | **M4 — kontrakt o ustalonym kształcie, patrz niżej** |
| `Permit`, `PermitKind`, `PermitStatus`, `fn file_permit(..) -> PermitId` | `city` | M7 (budowa zakładu), M9 (gracz) |
| `Tender`, `Bid`, `BidCriteria`, `fn submit_bid(..)` | `city` | M7 (firmy AI), M9 (gracz) |
| `Election`, `Candidate`, `ElectionResult`, `fn back_candidate(..)` | `city` | M9, M10 (media) |
| `PublicService`, `ServiceKind`, `ServiceCoverage` | `city` | M3 (edukacja, zdrowie), M5 (straty, wartość gruntu), M7 (absencja) |
| `Agency`, `Case`, `Remedy`, `UnreportedShareBps` | `city` | M7 (ryzyko), M9 (decyzja gracza) |
| `UtilityKind`, `UtilityNetwork`, `Tariff`, `SupplyState` | `traffic` | M7 (operatorzy, zakłady), M9 |
| `fn power_available(SiteId) -> bool`, `fn supply_state(SiteId, UtilityKind) -> SupplyState` | `traffic` | **M7 — kontrakt krytyczny** |
| `WorldEvent`, `EventDef`, `EventCategory`, `EventId`, `EventCause` | `events` | wszystkie `sim/*`, M9 (Kronika) |
| `SimParam`, `ParamPatch`, `ParamOverlay`, `fn effective(..)` | `events` | M1, M3, M4, M5, M6, M7 |
| `Probe`, `ProbeCtx`, `EventTrigger`, `hazard_ppm` | `events` | M8 (WP8 — kontrole), M10 (zdarzenia rynkowe) |
| `Weather`, `SeasonCalendar`, `EpochClock` | `events` | M1/M11 (wizualizacja), M3 (potrzeby), M4 (prędkość), M6 (rolnictwo) |
| `MacroIndicators`, `DemographyParams` | `events` | M3 (demografia), M10 (makro) |
| `data/events/*.ron` — schemat i walidator | `events` | M12 (modding) |

#### Kontrakt szczególny z M4 — regulacje ruchowe

**Liczba profili tras (CH) jest zasobem limitowanym. M8 go nie powiększa.**
M4 utrzymuje ustalony, mały zestaw profili (`Passenger`, `HeavyDay`, `HeavyNight`) na jednej
kolejności kontrakcji. Każda regulacja ruchowa M8 wyraża się **wyłącznie** jako predykat na
krawędziach (`EdgeRestriction`: zbiór krawędzi + klasa pojazdu + okno `WeekMask`), który
M4 nakłada jako **maskę wyłączonych krawędzi na istniejący profil**. Zasady wiążące:

1. Żadna uchwała rady, żadna decyzja burmistrza i żadne działanie gracza **nie tworzy nowego
   profilu trasy.** Jeśli regulacja nie daje się wyrazić maską, nie wchodzi do `Policy`.
2. Regulacje składają się **sumą bitową w jedną maskę** per (klasa pojazdu, godzina).
   Dwadzieścia uchwał kosztuje tyle samo co jedna — koszt jest stały w liczbie regulacji.
3. Dla rzadkiego, nietypowego ograniczenia, którego maska nie obsłuży, M4 udostępnia
   **fallback: A\* z predykatem** — wolniejszy, ale bez kosztu stałego. M8 używa go świadomie
   i oznacza taką regulację jako kosztowną (widoczne w inspektorze).
4. `compile_edge_mask` przelicza się wyłącznie przy zmianie zbioru obowiązujących regulacji
   (uchwała, wejście w życie, wygaśnięcie), nie co tick.

Konsekwencja projektowa dla M8: regulacja jest zawsze **egzekwowana mandatem, nie barierą**.
Ciężarówka fizycznie może wjechać w zakazaną ulicę; router jej tam nie poprowadzi, a jeśli
kierowca (albo gracz) wybierze tę trasę mimo wszystko — jest mandat i sprawa dla inspekcji.
To utrzymuje regulację po stronie kosztu ekonomicznego, gdzie ma być, a nie po stronie fizyki.

### Konsumuję

| Czego potrzebuję | Skąd | Uwagi |
|---|---|---|
| `Money`, `Tick`, `SimMinute`, `Qty`, `Q`, `Mood`, `StreamId`, `rng()` | M0 `core` | dopisuję warianty `StreamId` |
| bufory komend, scheduler, hash stanu | M0 `ecs` | dopisuję komponenty M8 do hasza |
| normy klimatyczne per dzień roku i dzielnicę | M1 `world` | kotwica dla pogody |
| parcele, budynki, dzielnice, strefy | M2 `world` | podstawa podatku od nieruchomości, obwody |
| `CitizenId`, status, nastrój, dzielnica zamieszkania, graf relacji | M3 `agents` | wyborcy, obsada usług |
| czasy przejazdu, krawędzie drogowe | M4 `nav`/`traffic` | zasięg usług, `TravelSpeedMulBps` |
| `Books`, `Books::transfer`, `AccountId`, `AccountOwner`, `MoneySupplyLedger` | M5 `economy` | jedyna droga ruchu pieniądza; miasto jako `AccountOwner::City` |
| `trait TaxEngine` (+ `NoTax` jako baza), `Transaction.tax`, `TxKind`, `LedgerAccount::TaxExpense` | M5 `economy` | **haki zostawione dla M8** — M8 dostarcza `CityTaxEngine` |
| `Ledger`, `JournalEntry`, `post`, `income_statement` | M5 `economy` | podstawa CIT; ślad obciążeń w księdze gracza |
| transakcja detaliczna (`TxKind::RetailSale`), wycena parceli | M5 `economy` | punkt naliczenia VAT, podstawa podatku od nieruchomości |
| `SimCalendar` (K-1: 360 dni, 12 × 30), `DayOfWeek` (K-15) | M0 `core` | okresy podatkowe i kadencje na siatce miesięcznej; godziny handlu i grafiki urzędów na tygodniowej |
| `UtilityKind`, `DecisionReason`, `NeedKind`, `PlaceRef`, `TransportMode`, `ActivityKind` | M0 `core` (K-8, K-12) | M8 tylko używa, nie definiuje — `UtilityKind` dzielony z `TxKind::Utility` (M5) i M4 |
| węzeł importowy, wartość celna, partie towaru, receptury | M6 `supply` | cło, akcyza, naprawa (części) |
| `FirmId`, RZiS, payroll, decyzje firmy, rynek pracy | M7 `firms` | CIT, PIT, reakcje na blackout i podatki |
| `MediaExposure` wyborcy | M10 | **do M10 stub**: f(nakład publiczny kandydata, dzielnica) |

---

## 7. Testy i kryteria akceptacji

**T1 — Domknięcie podatkowe (tolerancja 0 groszy).** Scenariusz roczny, 5 tys. mieszkańców,
300 firm, wszystkie siedem danin aktywnych. Dla każdej pary `(TaxKind, FiscalPeriod)`:
```
Σ amount(Assessed) == Σ amount(Settled) + Σ amount(Overdue) + Σ amount(Abated)
Σ amount(Settled) == Δ CityBudget.revenue_ytd[kind]
```
oraz dla każdej firmy: suma obciążeń w jej księdze == suma `TaxCharge` wystawionych na nią.
Plus test zachowania pieniądza z dokumentu 00: `Σ firmy + Σ mieszkańcy + Σ banki + miasto = const`.
**Tolerancja: 0.** Test własnościowy (proptest) na 10⁶ losowych transakcji sprawdza dodatkowo,
że suma VAT-u naliczonego per transakcja równa się dokładnie VAT-owi w deklaracji miesięcznej
(nie ma dryfu zaokrągleń) i że `Σ property_tax_month(v, bps, 1..=12) == annual(v, bps)`.
Strażnik siatek czasu (K-15): test strukturalny sprawdza, że `FiscalPeriod` nie ma wariantu
tygodniowego i że żadna daninowa ścieżka nie czyta `DayOfWeek`. Przebieg 3-letni domyka się
do roku niezależnie od tego, w jaki dzień tygodnia wypadł 1 stycznia (dryf tygodnia nie
przesuwa ani jednego grosza).

**T2 — Blackout zatrzymuje produkcję i widać to w kosztach.** Scenariusz: jedna elektrownia,
jedna fabryka, 6 godzin gry. W ticku T wymuszona awaria źródła.
Asercje: (a) `power_available(factory) == false` w ticku T (Power liczony `EveryMinute`);
(b) wolumen produkcji w oknie awarii **dokładnie 0**; (c) w RZiS firmy za ten dzień koszty stałe
i płace **nie zerowe**, koszt energii zmiennej zerowy, wynik gorszy niż w przebiegu kontrolnym;
(d) karta inspekcji zakładu pokazuje `SiteHalted{ reason: NoUtility(Power) }`;
(e) po naprawie produkcja wraca do poziomu sprzed awarii w ≤ 2 ticki.
Wariant kaskadowy: sieć z 3 wyspami, wymuszone przeciążenie linii — asercja, że kaskada
kończy się w ≤ 8 rundach i że każda wyspa po ustabilizowaniu ma `supply ≥ demand`.

**T3 — Pełny determinizm zdarzeń.** Dwa przebiegi tego samego seeda, 5 lat gry:
identyczny, uporządkowany ciąg krotek `(tick, event_key, scope_instance, severity_bps, duration)`
oraz identyczny hash stanu ECS co 1000 ticków (z komponentami M8 w funkcji haszującej).
Trzeci przebieg: te same 5 lat z zapisem i wczytaniem stanu w losowym momencie — ciąg zdarzeń
po wznowieniu identyczny. Test statyczny: w module hazardu i losowania zdarzeń **nie występuje
żaden typ zmiennoprzecinkowy** (lint + test kompilacyjny na sygnaturach).

**T4 — Usługi mają mierzalny skutek.** Trzy pary przebiegów A/B, 10 lat gry:
(a) szkoła z pełnym finansowaniem vs. połowa — mediana umiejętności 18-latków w obwodzie
różni się o ≥ 8 punktów `Q`; (b) posterunek vs. brak — straty inwentaryzacyjne sklepów
w dzielnicy różnią się o ≥ 25% i widać to jako pozycję w RZiS; (c) szpital vs. brak —
absencja chorobowa w dzielnicy różni się o ≥ 15%.

**T5 — Władza nie oscyluje.** 20 lat gry bez zdarzeń zewnętrznych: żadna stawka podatkowa
nie zmienia kierunku częściej niż raz na 24 miesiące; budżet nie wchodzi w spiralę zadłużenia
(dług/przychody roczne < 200% w każdym punkcie). Każda decyzja ma niepusty `DecisionReason`.

**T6 — Wybory deterministyczne i wrażliwe.** Ten sam seed → identyczny wynik co do głosu.
Test wrażliwości: obniżenie `ServiceQuality` w jednej dzielnicy o 30% przez kadencję obniża
tam poparcie inkumbenta o ≥ 5 pkt proc. wobec przebiegu kontrolnego. Frekwencja mieści się
w widełkach 35–75% i jest wyższa wśród wyższego statusu i starszych roczników.

**T7 — Zdarzenia nie zadają cen.** Test strukturalny: pełne wyliczenie wariantów `SimParam`
przechodzi przez listę dozwoloną; żaden wariant nie odnosi się do `Offer`, `price`, `margin`
ani `demand_qty`. Test dynamiczny: przebieg 3 lat ze wszystkimi zdarzeniami — snapshot
wszystkich `Offer.price` przed i po zastosowaniu każdego patcha w tym samym ticku jest
identyczny (ceny zmieniają się dopiero w ticku decyzji cenowej firmy, nie w ticku zdarzenia).

**T8 — LOD i wydajność.**
(a) Ten sam scenariusz w mikro i w mezo → identyczne sumy podatków i identyczne salda
gotówkowe (tolerancja 0, wymóg z dokumentu 00 §4).
(b) `criterion`: `solve_network` dla wszystkich pięciu sieci metropolii 400 tys.
**< 0,3 ms/tick** (typowo, 1 runda) i **< 2 ms** w kaskadzie 8-rundowej.
(c) `sys_eval_hazards_slow` dla pełnego katalogu zdarzeń i 400 tys. mieszkańców
**< 3 ms/dobę gry**, z cache sond.
(d) `sys_settle_tax_charges` **< 1 ms/dobę gry** przy 20 tys. firm.

**Definition of Done fazy:** T1–T8 zielone, `clippy -D warnings`, komponenty M8 w haszu stanu,
walidator `data/events/` w CI, scenariusz `m8-demo` odtwarzalny z seeda, każda decyzja
władzy i każde zdarzenie z powodem widocznym w inspektorze (§7 dokumentu 00).

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Skutek | Mitygacja |
|---|---|---|---|
| R1 | **Burza zdarzeń** — hazardy mnożą się i świat staje się festiwalem katastrof | gra nieprzewidywalna, balans niemożliwy | `max_concurrent` i `cooldown_days` per definicja; globalny limit „budżetu chaosu" (suma severity aktywnych zdarzeń per kategoria); raport balansatora: liczba zdarzeń na rok per kategoria z widełkami w CI |
| R2 | **Martwe hazardy** — zdarzenie nigdy nie zachodzi, bo krzywa jest źle dobrana | katalog danych jest fikcją | inspektor hazardów pokazuje dla każdej definicji aktualny hazard i wartości sond; test CI: w 20-letnim przebiegu każda definicja z katalogu musi zajść ≥ 1 raz |
| R3 | **Model sieci zbyt prosty** — przeciążenia nie występują albo występują zawsze | blackout jest nudny albo gra nie działa | ścieżka wyjścia opisana w 5.4 (rozpływ DC za tym samym interfejsem); scenariusz testowy z wymuszonym przeciążeniem od WP3 |
| R4 | **Sprzężenie z M7** — firmy nie reagują na blackout ani na podatki | M8 wygląda jak dekoracja | kontrakt `power_available` i `TaxCharge` uzgodniony **przed** WP2/WP3; test integracyjny T2 wymaga rzeczywistej reakcji M7 — jeśli M7 nie gotowe, stub firmy w testach fazy |
| R5 | **Dryf zaokrągleń w podatkach** | test T1 pęka po miesiącach, trudny do zdiagnozowania | wszystkie funkcje podatkowe czyste i pokryte proptestami od pierwszego dnia; PIT liczony narastająco, podatek od nieruchomości metodą różnicy skumulowanej |
| R6 | **Koszt oceny hazardów** — sondy liczone per obiekt per definicja | O(defs × obiektów) co dobę | `gate` odsiewa przed policzeniem sond; cache sond per (Probe, tick); zakresy `Site`/`Firm` oceniane tylko dla kandydatów; budżet w T8c |
| R7 | **Oscylacja AI władzy** — stawki skaczą co miesiąc | miasto wygląda na szalone, gracz nie może planować | histereza + minimalny okres między zmianami + vacatio legis; test T5 |
| R8 | **Podatki przytłaczają gracza** | siedem danin w księdze = ściana liczb | jedna linia zbiorcza „podatki" z rozwinięciem; naliczenia grupowane per okres, nie per transakcja; UI — M9, ale model danych już to umożliwia (`FiscalPeriod` jako klucz agregacji) |
| R9 | **Determinizm złamany przez sondę** — sonda liczona z `HashMap` albo z floata | cichy rozjazd przebiegów | sondy wyłącznie `i64`, iteracja po `Vec`/`BTreeMap`; test T3 z zapisem i wczytaniem w środku |
| R10 | **Szara strefa zbyt opłacalna albo bezużyteczna** | albo wszyscy oszukują, albo nikt | hazard kontroli rośnie nadliniowo z `UnreportedShareBps`; kara = domiar + odsetki + mnożnik; balansator raportuje odsetek szarej strefy w populacji firm AI (cel: 5–20%) |
| R11 | **Kaskada sieci nie zbiega** | pętla w ticku | twardy limit 8 rund + fallback: zrzuć wszystko niezbilansowane; test kaskadowy w T2 |
| R12 | **Inflacja regulacji ruchowych** — aktywne politycznie miasto mnoży uchwały, a każda dokłada koszt routingu | routing przestaje się mieścić w budżecie M4 dokładnie wtedy, gdy gra robi się ciekawa | `EdgeRestriction` jako predykat składany sumą bitową w jedną maskę (kontrakt z M4 w §6) — koszt stały w liczbie uchwał; zakaz tworzenia nowych profili CH wpisany w kontrakt; test: 50 aktywnych regulacji daje ten sam czas routingu co 1 (±5%) |

---

## 9. Decyzje otwarte

Kanał uzgodnień międzyagentowych był w tej sesji niedostępny (`ListAgents` nieobecny), ale
dokumenty M0/M1/M4/M5 i rozstrzygnięcia `K-*` z dokumentu 00 pojawiły się w trakcie pisania
i zostały **uwzględnione**. Poniżej najpierw to, co już rozstrzygnięte, potem to, co otwarte.

### Rozstrzygnięte na podstawie dokumentów, które powstały równolegle

| # | Sprawa | Rozstrzygnięcie przyjęte przez M8 |
|---|---|---|
| Z1 | Kalendarz | **K-1**: 360 dni, 12 × 30. Okresy podatkowe, kadencje i sezony liczą w tej jednostce; `annual/12` bez reszty |
| Z2 | Zakres `StreamId` | **K-4**: blok 240–259, przydział wyliczony w 5.0 |
| Z3 | Funkcje przestępne | **K-6**: M8 spełnia z nadmiarem — hazard i podatki w całości całkowitoliczbowe, `det_math` niepotrzebny w tej ścieżce |
| Z4 | Właściciel księgi i planu kont | **M5**: `Ledger`, `LedgerAccount`, `JournalEntry`, `post`. M8 nie projektuje księgowości, tylko emituje `TaxCharge` i wypełnia hak `TaxEngine` |
| Z5 | Hak podatkowy (pytanie otwarte M5 nr 8) | **odpowiedź w 5.1**: `Transaction.tax` zostaje dla VAT (jedna stawka na transakcję), reszta danin w `ChargeRegistry` M8. M5 nie migruje dziennika |
| Z6 | Ruch pieniądza | **M5**: wyłącznie `Books::transfer`; miasto jako `AccountOwner::City`, objęte niezmiennikiem `MoneySupplyLedger` |
| Z7 | Gdzie mieszka `UtilityKind` (było D2b) | **K-8**: `engine/core`, zgodnie z propozycją M8. Razem z `TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind` i `DecisionReason` (K-12) |
| Z8 | Tydzień w kalendarzu 12×30 | **K-15**: tydzień 7-dniowy istnieje i dryfuje; `DayOfWeek` z `SimCalendar`. M8 rozdziela siatki: regulacje handlowe i grafiki usług na tygodniowej (`WeekMask`), podatki, budżet i kadencje na miesięcznej/kwartalnej. Żaden okres rozliczeniowy nie jest liczony w tygodniach |

### Otwarte — do rozstrzygnięcia przed startem fazy

| # | Decyzja | Założenie M8 | Z kim uzgodnić |
|---|---|---|---|
| D1 | Rozszerzenia, o które M8 prosi M5: `AccountOwner::City`, `TxKind::TaxPayment`, `TxKind::PublicSpend`, konta `*Payable` | dopisanie wariantów, zero zmian w istniejących; jeśli M5 odmówi — M8 trzyma zobowiązania wyłącznie w `ChargeRegistry`, kosztem czytelności bilansu firmy | **M5** |
| D2 | Czy cena detaliczna w ofercie jest **brutto** czy **netto** | brutto (mieszkaniec widzi to, co płaci; VAT wyłuskiwany `vat_from_gross`). M5 ma `Transaction.gross = net + tax`, co jest spójne z brutto po stronie oferty, ale `Offer.unit_price` wymaga jawnego potwierdzenia | **M5** |
| D3 | Kto nalicza cło w węźle importowym | M6 wywołuje `duty_due` z `TaxCode` i emituje `TaxCharge`; M8 dostarcza stawkę i odbiera wpływ | **M6** |
| D4 | Czy `City` jest encją ECS czy zasobem (resource) | encja-singleton z komponentami — żeby mieściła się w haszu stanu i snapshotcie bez wyjątków | **M0** |
| D5 | Bankructwo z tytułu zaległości podatkowych | M8 wystawia `Remedy::BackTax` i wniosek egzekucyjny; postępowanie upadłościowe i wyprzedaż majątku — M7 | **M7** |
| D6 | Granulacja zasięgu usług publicznych | per dzielnica (`DistrictId`), z zanikiem po czasie przejazdu między centroidami. `ponytail:` sufit — szkoła obwodowa może być zbyt zgrubna; wyjście: promień w metrach na siatce M2 | **M2, M3** |
| D7 | `MediaExposure` wyborcy przed M10 | stub `f(nakład publiczny kandydata, dzielnica)`; M10 podmienia na zasięg realnych tytułów bez zmiany sygnatury | **M10** |
| D8 | Płaca minimalna — polityka miasta czy parametr rynku pracy | `Policy::MinWage` uchwalana przez M8, egzekwowana przez M7 przy ustalaniu ofert pracy | **M7** |
| D10 | Czy operatorzy sieci są przejmowalni przez gracza w M8 | w M8 wyłącznie firmy AI z taryfą; przejęcie/przetarg na operatora — M9/M10 | **M9, M10** |
| D11 | Kto stosuje `DemographyParams` | M8 liczy parametry (dzietność, migracja), M3 je stosuje do populacji | **M3** |
| D12 | Podstawa podatku od nieruchomości | wycena katastralna aktualizowana z transakcji (M5 §6.7), zamrażana na 1 stycznia roku podatkowego | **M5** |
| D13 | Czy strajk jest zdarzeniem (`sim/events`) czy mechaniką związków (M10 §6.6) | M8 dostarcza zdarzenie `social/strike` z hazardem; M10 może podmienić wyzwalanie na model związków zawodowych, zachowując te same `SimParam` | **M10, M7** |
| D14 | Czy zdarzenia mogą wyzwalać zdarzenia (`EventCause::Chained`) | tak, ale wyłącznie przez zmianę sondy (pośrednio) — bezpośrednie łańcuchy tylko tam, gdzie to fizyka (awaria linii → rozpad wyspy) | — (decyzja M8, do rewizji po balansie) |
| D15 | Częstotliwość `sys_solve_power_grid` | `EveryMinute`; jeśli benchmark T8b nie zmieści się w budżecie — `EveryHour` z wyzwalaniem zdarzeniowym przy zmianie topologii lub popytu > 5% | **M12** (profilowanie) |

---

## 10. Szacunek wielkości

| WP | Zakres | Rozmiar |
|---|---|---|
| WP1 | Szkielet `sim/city`, `CityBudget`, księga publiczna, dług | **M** |
| WP2 | `TaxCode`, siedem danin, cykl należności, integracja z księgą | **L** |
| WP3 | Sieci przesyłowe: graf, solver, blackout, kaskada, taryfy, faktury | **L** |
| WP4 | Rdzeń `sim/events`: hazard, sondy, `ParamOverlay`, rejestr | **L** |
| WP5 | Katalog zdarzeń w `data/events/` + walidator CI | **M** |
| WP6 | Pogoda, sezony, epoka, wskaźniki makro, parametry demografii | **M** |
| WP7 | Usługi publiczne, obwody, urzędy z kolejką pozwoleń | **L** |
| WP8 | Pięć urzędów, kontrole, kary, szara strefa | **M** |
| WP9 | Władza miejska AI: cele, decyzje, przetargi, dotacje, regulacje | **L** |
| WP10 | Wybory: frekwencja, preferencje, mandaty, wsparcie gracza | **M** |
| WP11 | Inspektory, Kronika, hash stanu, scenariusz demo | **M** |

Rozkład: 4 × L, 6 × M, 1 × M (WP1). Najcięższe i najbardziej ryzykowne są WP2 (precyzja
do grosza), WP3 (jedyny nowy solver numeryczny w fazie) i WP4 (fundament, od którego zależą
WP5 i WP8). Kolejność startu: WP1 → WP2 równolegle z WP3 → WP4 → WP5/WP6 równolegle →
WP7 → WP8/WP9 → WP10 → WP11.
