# M9d — Język reguł

Podfaza 4 z 5 fazy **M9 — Gracz: kariera** (`M9-gracz-kariera.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M9b (widgety), M9a (komendy), `sim/policy` z M7 (K-11). |
| **Pakiety robocze** | WP8, WP9 |
| **Projekt techniczny** | §5.6 |
| **Wynik do pokazania** | 6 przykładowych polityk zbudowanych wyłącznie klikaniem; dry-run „30 dni” zgodny z późniejszym wykonaniem. |
| **Kryterium zamknięcia** | Kryteria WP8 i WP9; 200 sklepów z politykami < 1 ms/dobę na wątku symulacji. |
| **Poprzednia / następna** | `M9c-gracz-inspekcja-nakladki.md` · `M9e-panele-czas-kariera.md` |

Projekt języka reguł jako specyfikacja wiążąca dla M7, serializator tekstowy, walidator, `RuleEditor` bez pisania kodu, dry-run oraz wykonanie polityk przez menedżerów.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP8** | Edytor reguł i diagnostyka | WP2, WP6, AST z `sim/policy` (M7, K-11) | Projekt języka jako spec dla M7; serializator tekstowy, walidator (w tym `PriceBasisMismatch`), `RuleEditor` (edycja slotów, zero pisania kodu), dry-run „30 dni" | 6 przykładowych polityk z §5.6 zbudowanych wyłącznie klikaniem; dry-run zgodny z późniejszym wykonaniem; podstawa ceny widoczna w każdym slocie cenowym |
| **WP9** | Wykonanie polityk przez menedżerów | WP8 | `ManagerExecution` z umiejętności, zapis `DecisionReason::PolicyApplied`, eskalacja do gracza; wymagania wydajnościowe na `PolicyRunner` przekazane M7 | 200 sklepów z politykami: < 1 ms/dobę na wątku symulacji, determinizm w dwóch przebiegach |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.6 Język reguł (§14.6)

> **Własność (K-11).** Język reguł — gramatyka, warunki, akcje, metryki i zakresy stosowania
> opisane poniżej — jest projektowany w tej fazie i **wiążący dla M7**. Sam AST i ewaluator
> mieszkają w crate'cie **`sim/policy`, którego właścicielem jest M7**, bo firmy AI potrzebują
> tego samego mechanizmu operacyjnie dla tysięcy firm (§6.3 PRD: gracz ma „ten sam zestaw
> narzędzi co AI"). W `game/` zostaje warstwa gracza: **edytor UI, diagnostyka, dry-run
> i podpowiedzi**, plus `ManagerExecution` — jakość wykonania zależna od menedżera.

Reprezentacja kanoniczna to **AST**, nie tekst. Edytor manipuluje AST przez sloty z listami
rozwijanymi — gracz nie pisze i nie może napisać składniowego błędu. Postać tekstowa istnieje
do: wyświetlania („co ta polityka robi" jednym zdaniem), dzielenia się politykami i do `data/`.
Parser tekstu nie jest używany w pętli — wykonuje się AST.

```rust
pub struct Policy {
    pub id: PolicyId,
    pub name: String,
    pub domain: PolicyDomain,           // Pricing | Stock | Hr | Production | Logistics
    pub rules: Vec<Rule>,               // pierwsza pasująca wygrywa; kolejność = priorytet
    pub fallback: Option<Action>,
    pub cadence: Cadence,               // Daily (domyślnie) | Hourly (droższe, dla nietrwałych)
    pub cooldown_h: u8,                 // martwa strefa czasowa — antyoscylacja
}

pub struct Rule {
    pub when:    ConditionExpr,
    pub then:    SmallVec<[Action; 2]>,
    pub enabled: bool,
    pub note:    String,                // notatka gracza, nie wpływa na wykonanie
}

pub enum ConditionExpr {
    And(Box<ConditionExpr>, Box<ConditionExpr>),
    Or(Box<ConditionExpr>, Box<ConditionExpr>),
    Not(Box<ConditionExpr>),
    Cmp { lhs: Expr, op: CmpOp, rhs: Expr },
    Always,
}

pub enum Expr {
    Lit(Value),                          // Money | Qty | Days | Bp | Count | Enum
    Metric(Metric),
    Bin { lhs: Box<Expr>, op: ArithOp, rhs: Box<Expr> },   // + − × ÷ oraz „% z"
}

pub enum Metric {
    // cena i koszt — każda metryka cenowa NIESIE podstawę (doc 00 §4a / K-7)
    Price      { good: GoodId, basis: PriceBasis },
    UnitCost(GoodId), Margin(GoodId),
    CheapestCompetitorPrice { good: GoodId, radius_m: u32, basis: PriceBasis },
    AvgCompetitorPrice      { good: GoodId, radius_m: u32, basis: PriceBasis },
    CompetitorCount         { radius_m: u32 },
    // zapas — okna kroczące (nie kalendarzowe), więc niezależne od podziału na tygodnie
    Stock(GoodId), StockDays(GoodId), Turnover7d(GoodId), Sales7d(GoodId),
    DaysToExpiry(GoodId), ShelfGap(GoodId),
    // produkcja i ludzie
    MachineUtilization, OpenPositions(JobRoleId), StaffTurnover12m,
    MedianMarketWage(JobRoleId), StaffMood, ManagerSkill,
    // finanse i kontekst
    CashBalance, Receivables, Season, DayOfWeek, DayOfMonth, HourOfDay,
    DaysSinceLastChange(GoodId),
}

pub enum Action {
    SetPrice     { good: GoodId, to: Expr },
    AdjustPrice  { good: GoodId, by: Expr },          // by: Bp albo Money
    SetMargin    { good: GoodId, bp: Bp },
    ClampPrice   { good: GoodId, min: Expr, max: Expr },
    OrderUpTo    { good: GoodId, days: Expr },
    OrderQty     { good: GoodId, qty: Expr, source: OrderSource },
    Markdown     { good: GoodId, bp: Bp },
    RemoveFromShelf(GoodId),
    Hire         { role: JobRoleId, count: u16, wage: Expr },
    RaiseWage    { role: JobRoleId, by: Bp, cap: Option<Expr> },
    PlanProduction { recipe: RecipeId, qty: Expr },
    Alert        { key: LocKey, severity: Severity },
    /// eskalacja: polityka się zatrzymuje i pyta gracza (alert w pulpicie firmy)
    AskPlayer    { key: LocKey },
}

pub enum PolicyScope {
    Firm(FirmId),
    Site(SiteId),
    Group(TagId),                                     // np. „sklepy w Śródmieściu"
    Product   { inner: Box<PolicyScope>, good: GoodId },
    Category  { inner: Box<PolicyScope>, cat: CategoryId },
}
```

Rozstrzyganie zakresu — **od najbardziej szczegółowego**:
`Product@Site > Category@Site > Site > Product@Group > Group > Product@Firm > Firm`.
Konflikt (dwa zakresy tej samej szczegółowości) jest błędem walidacji, nie losowaniem.

**Arytmetyka bez floatów.** Wszystkie wartości pieniężne to `Money(i64)` w groszach, procenty to
`Bp(i32)` w punktach bazowych (10000 = 100%). Mnożenie `Money × Bp` przez `i128` i
`div_round_half_up` (doc 00 §2). To jest wymóg, nie preferencja: polityka cenowa wykonana przez
menedżera zmienia stan trwały.

**Podstawa ceny jest jawna (doc 00 §4a, K-7).** `Offer.price_basis` rozróżnia cenę brutto
(detal — to, co faktycznie płaci klient) od netto (hurt). Skutki dla M9:

- Każda metryka cenowa niesie `PriceBasis`. Nie istnieje `cena(X)` bez podstawy.
- **Edytor reguł pokazuje podstawę w slocie, zawsze, nawet gdy jest tylko jedna sensowna** —
  gracz ma widzieć „cena brutto (detal)", a nie domyślać się. Domyślna podstawa wynika z rodzaju
  zakładu w zakresie polityki (`SiteKind::Shop` → brutto, zakład/hurt → netto), ale jest widoczna
  i przestawialna.
- **Walidator odrzuca mieszanie podstaw w jednym porównaniu i w jednej akcji** —
  `PriceBasisMismatch` jest błędem, nie ostrzeżeniem. `ustaw cenę brutto = cena_netto_konkurenta − 2%`
  po prostu nie da się złożyć.
- Konwersja jest dostępna jawnie jako funkcja wyrażenia (`brutto(x)` / `netto(x)` po stawce VAT
  towaru), więc reguła mieszana jest możliwa — ale gracz musi ją napisać świadomie.
- Porównanie z konkurencją w detalu to zawsze **brutto do brutto** — ta sama liczba, którą
  klient widzi na półce i którą zobaczy w karcie inspekcji jako powód `LostToCompetitor`.

```rust
pub enum PriceBasis { Gross, Net }   // z sim/economy: Offer.price_basis
```

#### Gramatyka postaci tekstowej

```ebnf
polityka   ::= "POLITYKA" nazwa "DLA" zakres [ "CO" kadencja ] regula+ [ "INACZEJ" akcja ]
regula     ::= "GDY" warunek "TO" akcja { "ORAZ" akcja }
warunek    ::= warunek "I" warunek | warunek "LUB" warunek | "NIE" warunek
             | "(" warunek ")" | porownanie | "ZAWSZE"
porownanie ::= wyrazenie ("<" | "<=" | "=" | "≠" | ">=" | ">") wyrazenie
wyrazenie  ::= liczba jednostka | metryka | wyrazenie ("+"|"-"|"*"|"/") wyrazenie
             | procent "Z" wyrazenie
jednostka  ::= "zł" | "gr" | "szt" | "dni" | "%" | "godz" | "os"
podstawa   ::= "brutto" | "netto"                      ; obowiązkowa przy metrykach cenowych (K-7)
metryka    ::= "cena" podstawa "(" towar ")" | "koszt" "(" towar ")" | "marża" "(" towar ")"
             | "zapas" "(" towar ")" | "zapas_dni" "(" towar ")"
             | "rotacja_7d" "(" towar ")" | "sprzedaż_7d" "(" towar ")"
             | "dni_do_przydatności" "(" towar ")"
             | "cena_najtańszego_konkurenta" podstawa "(" towar "," promień ")"
             | "cena_średnia_konkurencji" podstawa "(" towar "," promień ")"
             | "brutto" "(" wyrazenie ")" | "netto" "(" wyrazenie ")"   ; jawna konwersja VAT
             | "liczba_konkurentów" "(" promień ")"
             | "obłożenie_maszyn" | "wolne_etaty" "(" stanowisko ")"
             | "rotacja_kadry_12m" | "płaca_mediana" "(" stanowisko ")"
             | "nastrój_załogi" | "umiejętność_menedżera"
             | "saldo" | "należności" | "sezon" | "dzień_tygodnia" | "dzień_miesiąca" | "godzina"
             | "dni_od_zmiany" "(" towar ")"
akcja      ::= "ustaw cenę" wyrazenie
             | "zmień cenę o" wyrazenie
             | "ustaw marżę" procent
             | "ogranicz cenę do" wyrazenie ".." wyrazenie
             | "zamów do" wyrazenie "dni"
             | "zamów" wyrazenie źródło
             | "przeceń o" procent
             | "wycofaj z półki"
             | "zatrudnij" liczba stanowisko "za" wyrazenie
             | "podnieś płacę o" procent [ "do" wyrazenie ]
             | "zaplanuj produkcję" receptura wyrazenie
             | "wyślij alert" tekst
             | "zapytaj gracza" tekst
zakres     ::= "firmy" nazwa | "zakładu" nazwa | "sklepu" nazwa | "grupy" tag
             | "produktu" towar "w" zakres | "kategorii" kategoria "w" zakres
kadencja   ::= "dzień" | "godzinę"
```

Twarde ograniczenia języka (żeby nie wyhodować języka programowania — na to jest M12/modding):
**bez zmiennych, bez pętli, bez funkcji gracza, bez rekurencji.** Maksymalnie 8 reguł na
politykę, głębokość wyrażenia ≤ 3, promień ≤ 10 km, ≤ 2 metryki konkurencyjne na politykę.

#### Sześć przykładowych polityk (realne, z PRD)

```
POLITYKA "Dyskont dzielnicowy" DLA grupy "Sklepy osiedlowe" CO dzień
  GDY liczba_konkurentów(3 km) > 0
    TO ustaw cenę cena_najtańszego_konkurenta brutto(TEN_TOWAR, 3 km) - 2%
       ORAZ ogranicz cenę do brutto(koszt(TEN_TOWAR) * 105%) .. brutto(koszt(TEN_TOWAR) * 160%)
  INACZEJ ustaw marżę 25%
```
(§6.3 wprost: „−2% względem najtańszego konkurenta w promieniu 3 km" + „marża 25%" jako fallback,
z klauzulą chroniącą przed sprzedażą poniżej kosztu — czyli „cena dynamiczna z limitem".
Podstawy jawne: cena detaliczna i cena konkurenta to brutto, koszt jednostkowy to netto, więc
ogranicznik przechodzi przez jawne `brutto(...)` — K-7. Marża liczona jest zawsze na netto.)

```
POLITYKA "Nabiał — nie wyrzucamy" DLA kategorii "Nabiał" w firmy "Delikatesy Anny" CO godzinę
  GDY dni_do_przydatności < 2 dni I zapas_dni > 1 dni  TO przeceń o 40%
  GDY dni_do_przydatności < 1 dni                       TO przeceń o 70%
  GDY dni_do_przydatności = 0 dni                       TO wycofaj z półki ORAZ wyślij alert "straty"
```

```
POLITYKA "Zapas min-max" DLA grupy "Sklepy osiedlowe" CO dzień
  GDY zapas_dni < 3 dni        TO zamów do 10 dni
  GDY zapas_dni > 21 dni I rotacja_7d < 5 szt  TO wyślij alert "martwy zapas"
```

```
POLITYKA "Sezon grzewczy" DLA produktu "Węgiel opałowy" w firmy "Skład Magnat" CO dzień
  GDY sezon = jesień I zapas_dni < 30 dni  TO zamów do 60 dni
  GDY sezon = wiosna I zapas_dni > 20 dni  TO przeceń o 15%
```

```
POLITYKA "Zatrzymać ludzi" DLA firmy "Magnat Logistyka" CO dzień
  GDY rotacja_kadry_12m > 30% I nastrój_załogi < 40
    TO podnieś płacę o 5% do płaca_mediana(kierowca) * 115%
  GDY wolne_etaty(kierowca) > 2 os  TO zatrudnij 2 kierowca za płaca_mediana(kierowca) * 105%
```

```
POLITYKA "Nie przepłacaj za ropę" DLA zakładu "Rafineria Wschód" CO dzień
  GDY zapas_dni(ropa) < 5 dni I cena_średnia_konkurencji(ropa, 10 km) > koszt(ropa) * 130%
    TO zapytaj gracza "Ropa droga, a zapas na 5 dni — kupować czy przystopować produkcję?"
  GDY zapas_dni(ropa) < 5 dni  TO zamów do 20 dni
```

Ostatni przykład pokazuje najważniejszą akcję całego systemu: **`zapytaj gracza`**. Automatyzacja,
której nie da się przerwać, jest gorsza niż jej brak — gracz z 200 sklepami potrzebuje, żeby
polityka sama zgłaszała, kiedy sytuacja wyszła poza jej założenia.

#### Walidacja (`RuleEditor`)

```rust
pub struct RuleEditor { policy: Policy, diagnostics: Vec<Diagnostic>, dry_run: Option<DryRunResult> }
pub enum Diagnostic {
    UnitMismatch    { at: NodePath, expected: Unit, got: Unit },          // błąd
    PriceBasisMismatch { at: NodePath, lhs: PriceBasis, rhs: PriceBasis },// błąd (K-7)
    BelowCost       { good: GoodId },                                     // ostrzeżenie
    PossibleOscillation { rule: usize, metric: Metric },                  // ostrzeżenie
    UnreachableRule { rule: usize, shadowed_by: usize },                  // ostrzeżenie
    ScopeConflict   { with: PolicyId, scope: PolicyScope },               // błąd
    CostBudget      { estimated_us: u32, limit_us: u32 },                 // błąd powyżej limitu
    MissingClamp    { rule: usize },                                      // ostrzeżenie
}
```

1. **Typy jednostek** — każda metryka ma `Unit`; edytor oferuje w slocie wyłącznie wyrażenia
   zgodne jednostkowo, więc `UnitMismatch` może powstać tylko z importu tekstu.
2. **Podstawa ceny (K-7)** — porównanie lub akcja mieszająca brutto z netto to **błąd**.
   Edytor nie pozwala złożyć takiego wyrażenia; komunikat wskazuje jawną konwersję `brutto()`/`netto()`.
3. **Sprzedaż poniżej kosztu** — ostrzeżenie, nie błąd (może być świadomą wojną cenową).
   Porównanie robione po sprowadzeniu obu stron do netto.
4. **Oscylacja** — statycznie: reguła, której akcja zmienia metrykę czytaną w jej warunku
   z przeciwnym znakiem, bez `ClampPrice` i przy `cooldown_h == 0`. Wymuszamy martwą strefę.
5. **Reguła nieosiągalna** — warunek wcześniejszej reguły pokrywa późniejszą (sprawdzenie
   przez podstawienie przedziałów, nie SMT — tanio i wystarczy).
6. **Konflikt zakresów** — dwie polityki tej samej szczegółowości na tym samym celu.
7. **Budżet kosztu** — metryki promieniowe są drogie; edytor pokazuje szacowany koszt jednego
   wykonania i blokuje politykę powyżej limitu.
8. **Dry-run „przetestuj na ostatnich 30 dniach"** — polityka jest odtwarzana na zapisanych
   seriach metryk tego zakładu (te same `Series`, które zasilają wykresy), a wynik pokazany jako
   nakładka na wykresie ceny: „co by ustawiła". Kosztuje niemal nic, bo dane już są, a odpowiada
   na pytanie, którego gracz inaczej nie zada przed przypięciem polityki do 40 sklepów.

#### Wykonanie i menedżer (§14.6, §7 PRD: „jakość zależna od umiejętności")

```rust
pub struct ManagerExecution {
    pub skill:          Q,     // umiejętność „zarządzanie" (M7)
    pub info_lag_days:  u8,    // 7 → 1 wraz ze skill (§6.3: konkurent widziany z opóźnieniem 1–7 dni)
    pub reaction_delay_h: u8,  // 48 → 1
    pub exec_error_bp:  i32,   // ±400 bp → ±0 bp
    pub skip_chance_bp: i32,   // 1500 bp → 0 bp
}
impl ManagerExecution {
    pub fn from_skill(skill: Q) -> Self;   // jedno deterministyczne odwzorowanie, bez floatów
}
```

`PolicyRunner` jako system ECS, częstotliwość `EveryDay` (opcjonalnie `EveryHour` dla polityk
z `Cadence::Hourly`), z **rozłożeniem w dobie**: zakład o indeksie `i` liczony w minucie
`(i * 37) % 1440`. 200 sklepów to średnio 0,14 zakładu na minutę gry — koszt znika w tle.

Jakość wykonania wchodzi w trzech miejscach i wszystkie trzy są deterministyczne
(`rng(world_seed, StreamId::PolicyExecution /* = 260, blok M9: 260–279, K-4 */, manager.index(), tick)`):

- **Wejścia**: metryki konkurencji czytane z opóźnieniem `info_lag_days` — słaby menedżer
  reaguje na wczorajszy świat.
- **Opóźnienie**: reguła zostaje zastosowana `reaction_delay_h` godzin po spełnieniu warunku.
- **Wykonanie**: wartość wynikowa przesunięta o `exec_error_bp`, z szansą `skip_chance_bp`
  na pominięcie cyklu.

Brak menedżera → menedżerem jest gracz: `skill` = umiejętność zarządzania postaci, ale jeśli
postać nie ma zmiany w tym punkcie (`SetOwnShift`), obowiązuje podłoga „bez nadzoru".
To jest mechaniczna motywacja do zatrudnienia menedżera z §7 PRD — nie bramka, tylko koszt.

Każde zastosowanie polityki zapisuje powód:

```rust
DecisionReason::PolicyApplied {
    policy: PolicyId, rule: u8,
    inputs: SmallVec<[(Metric, Value); 4]>,
    target: Value, applied: Value,       // różnica = błąd menedżera
    manager: Option<CitizenId>, lag_days: u8,
}
```

Karta inspekcji półki renderuje to jako: *„Cena 6,38 zł ustawiona 3 dni temu przez politykę
»Dyskont dzielnicowy«, reguła 1: najtańszy konkurent w 3 km = 6,51 zł (dane sprzed 4 dni),
cel 6,38 zł, menedżer Marek Zdun (umiejętność 41) ustawił 6,44 zł."* To jest cała odpowiedź
na „dlaczego moja cena jest taka" — bez tego automatyzacja jest czarną skrzynką.

---

## Zmiany wpisane po M7c

Zgodnie z `K-18`. M7 zbudował ewaluator języka opisanego w §5.6 (`sim/policy`, `K-11`)
i przy okazji znalazł w tym opisie cztery rzeczy nieprawdziwe. Wpisane jest **tylko to,
co wiadomo na pewno** — projekt języka się nie zmienia i nadal należy do M9.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Metryki i akcje towarowe biorą `GoodRef { This, Id(GoodId) }`, a nie `GoodId`.** `This` to `TEN_TOWAR` z gramatyki | §5.6 wypisuje w AST `good: GoodId`, a **wszystkie sześć przykładowych polityk w tym samym paragrafie** używa `TEN_TOWAR` — „Dyskont dzielnicowy" i „Nabiał — nie wyrzucamy" wprost. Polityka o zakresie kategorii albo grupy wykonuje się raz na towar i nie zna jego identyfikatora w chwili zapisu, więc bez tego wariantu AST nie wyraża własnych przykładów. Podstawienie robi ewaluator na granicy odczytu metryki, czyli edytor nadal widzi jeden slot |
| ★ | **AST dostaje `Expr::Convert { to, of }` — jawne `brutto(x)` i `netto(x)`, które gramatyka §5.6 wypisuje, a lista wariantów `Expr` pomija** | Bez tego węzła `K-7` **zamyka język**: koszt własny jest netto, cena półkowa brutto, walidator odrzuca porównanie mieszające podstawy — i ogranicznika „105 % … 160 % kosztu" z polityki „Dyskont dzielnicowy" nie da się zapisać wcale. Obejście przez ogranicznik odniesiony do **własnej ceny** nie ogranicza niczego, bo przycina wartość do przedziału wyprowadzonego z niej samej; zmierzone: po dobie cena stała dokładnie tam, gdzie stała. Przelicznikiem jest stawka VAT towaru z widoku (`PolicyView::vat_bp`); do M8 wynosi zero, więc konwersja jest tożsamością — ale **typ** się zmienia i to jest cała jej treść przed M8 |
| ★ | **`Action::Alert` i `AskPlayer` niosą `msg: u16`, a nie `LocKey`** | `LocKey` mieszka w `engine/ui`, a `sim/policy` od interfejsu nie zależy i zależeć nie będzie — akcje wykonują się w symulacji tysiące razy na dobę, także wtedy, gdy żadnego okna nie ma. `msg` jest indeksem komunikatu polityki; **odwzorowanie na klucz lokalizacji należy do warstwy UI, czyli do M9**, i to jest jedyne miejsce, w którym ta zmiana coś kosztuje |
| | **`PriceBasis` ma warianty `GrossRetail` i `NetB2B`, nie `Gross`/`Net`** — i mieszka od M7c w `engine/core::vocab` (`K-47`), a nie w `sim/economy` | §5.6 podaje `pub enum PriceBasis { Gross, Net }` „z sim/economy"; w `Offer.price_basis` od M5a stoją dłuższe nazwy i to one są kontraktem areny 80 tys. ofert. Przenosiny do `core` były konieczne, bo `sim/policy` nie może zależeć od `sim/economy`; edytor M9 czyta ten sam typ co przed zmianą, tylko z innego crate'u |
| ★ | **`Policy.domain` jest dziś ograniczona do `Pricing` i `Stock`** — walidator zwraca `DomainNotAvailable` dla `Hr`, `Production` i `Logistics` | Wykonawca akcji istnieje dla ceny i zapasu (sklep, M7c); kadr, produkcji i logistyki nikt jeszcze nie wykonuje. Polityka bez wykonawcy wyglądałaby w edytorze jak działająca i nie robiłaby nic. **Dla M9 znaczy to jedno:** przykładowa polityka „Zatrzymać ludzi" z §5.6 nie da się dziś przypiąć **Po M7e adres odblokowania przesuwa się na M7f:** tier operacyjny M7e nie dostał akcji kadrowych z rozmysłu (publikacja ofert i licytacja płac dzieją się same od M7b, `BC-3`), ale wykonawca **polityki** kadrowej to inna rzecz niż decyzja tieru — i tego nadal nie napisał nikt |
| | **`Diagnostic` ma dziś osiem wariantów, ale nie te same.** Powstały `UnitMismatch`, `PriceBasisMismatch`, `UnreachableRule`, `PossibleOscillation`, a do tego `ActionOutOfDomain`, `DomainNotAvailable`, `TooDeep` i `BudgetExceeded` (promień, liczba reguł, liczba metryk konkurencyjnych) w miejsce `CostBudget` i `MissingClamp`. **`BelowCost` i `ScopeConflict` nie powstały** | `ScopeConflict` nie ma gdzie powstać: `PolicyScope` nie jest polem `Policy` (tak samo jak w §5.6), więc walidator jednej polityki nie widzi drugiej. Remis szczegółowości rozstrzyga `scope::resolve` niższym indeksem — deterministycznie i tak samo w każdym przebiegu — a wykryć konflikt można dopiero tam, gdzie leży **lista** polityk, czyli w edytorze. `CostBudget` w postaci „szacowany koszt w mikrosekundach" wymaga pomiaru, którego nie ma czym zrobić przed pierwszym wykonaniem; twarde limity z §5.6 (8 reguł, głębokość 3, promień 10 km, 2 metryki konkurencyjne) mierzą to samo i dają się sprawdzić statycznie. `BelowCost` wymaga sprowadzenia obu stron do netto, czyli stawki VAT towaru — a tej `sim/policy` nie widzi. Oba wracają w WP8 razem z edytorem, który ma dostęp i do jednego, i do drugiego |
| | **`MAX_COVER = 999`: `zapas_dni` towaru, który się nie sprzedaje, jest skończoną liczbą, a nie brakiem odpowiedzi** | Reguła „martwy zapas" z polityki „Zapas min-max" wyzwala się właśnie na tym przypadku — przy braku odpowiedzi nie wyzwalałaby się nigdy. Symetrycznie pusta półka daje zero dób pokrycia niezależnie od tego, czy cokolwiek się sprzedawało, bo inaczej nie wyzwoliłoby się zamówienie |
| | **`ManagerExecution` (§5.6, „Wykonanie i menedżer") zostaje w całości przy M9 WP9** i M7c go nie dotknął. M7 dostarcza `ManagementQuality` — jakość **zarządzania**, wchodzącą w produktywność, rotację i straty | Odnotowane, żeby WP9 nie zastał gotowego mechanizmu o tej samej nazwie i innym znaczeniu. Jakość wykonania polityki (opóźnienie informacji, zwłoka reakcji, błąd wykonania, pominięty cykl) to osobna warstwa nad ewaluatorem i nadal jest niczyja poza M9 |
