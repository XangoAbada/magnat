# M10a — Jądro, makro, historia na sucho

Podfaza 1 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M5 (rdzeń `kernel`), M6 i M7 (wypełnione sloty jądra). |
| **Pakiety robocze** | WP10.2, WP10.1, WP10.3, WP10.4 |
| **Projekt techniczny** | §5.6, §5.7, §5.8 |
| **Wynik do pokazania** | Świat startuje z 30-letnią historią policzoną w makro; test spójności makro↔mezo zielony. |
| **Kryterium zamknięcia** | Kryteria WP10.1–WP10.4. **Ta podfaza blokuje M12c (tryb 50×)** — nie odkładać jej za podfazy równoległe. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M10b-marka-i-media.md` |

Ścieżka krytyczna całej fazy: domknięcie wspólnego jądra ekonomicznego, `sim/macro` ze stanem i krokiem oraz `lift`/`lower`, historia „na sucho” (etapy D0–D5) i test spójności LOD.

---

## Pakiety robocze

### [x] WP10.2 — Wydzielenie wspólnego jądra ekonomicznego (`econ_kernel`)

**Zależności:** M5 (rdzeń), M6 i M7 (wypełnienie slotów).
**Charakter:** **domknięcie, nie refaktor** — i to jest zmiana na lepsze względem pierwszej wersji
tego planu. M5 pisze `kernel` od razu jako rdzeń, bo te funkcje w M5 dopiero powstają; nie ma więc
czego wyciągać po fakcie. M10 dokłada wyłącznie brakujące funkcje i ustanawia regułę.

Sedno spójności makro↔mezo nie leży w „starannym kalibrowaniu dwóch modeli" — takie kalibrowanie
zawsze rozjeżdża się po trzech sprintach. Leży w tym, że **model jest jeden**. Dlatego przed napisaniem
`sim/macro` wyciągamy z `sim/economy` i `sim/firms` funkcje czyste, wołane z obu poziomów:

```rust
// sim/economy/src/kernel.rs — nowy moduł, przeniesione ciała funkcji
pub fn purchase_score(ctx: &ScoreCtx, opt: &OptionView) -> Score;     // §6.4, bez losowania
pub fn softmax_shares(scores: &[Score], beta: Q, out: &mut [u32]);    // udziały w permille, suma == 1000_000
pub fn next_price(cur: Money, s: &PricingState, p: &PricingPolicy) -> Money;  // §6.3
pub fn wage_bid(role: JobRoleId, s: &LaborState, p: &WagePolicy) -> Money;    // §6.6
pub fn throughput(r: &Recipe, inputs: &[Qty], crew: &CrewStats, tech: TechLevel) -> Qty; // §7.4
pub fn interest_accrual(principal: Money, rate_bps: u32, days: u16) -> Money;
pub fn ledger_post(book: &mut Ledger, e: LedgerEntry);                // jedyna droga księgowania
```

**Reguła wiążąca, którą ten WP ustanawia:**
> `sim/macro` **nie zawiera żadnej logiki ekonomicznej.** Zawiera wyłącznie agregację, alokację
> i pętlę czasu. Każda decyzja o cenie, płacy, produkcji, użyteczności i każde zaksięgowanie pieniądza
> przechodzi przez `econ_kernel`. Różnica między mezo a makro to wyłącznie to, czy `softmax_shares`
> jest losowany (mezo: jeden agent wybiera jedną opcję) czy stosowany jako wagi (makro: komórka dzieli
> popyt proporcjonalnie).

To przenosi problem spójności z „testowania podobieństwa dwóch implementacji" na „testowania jednej
implementacji" — i to jest jedyny powód, dla którego test z WP10.4 ma szansę być zielony po roku.

**Podział pracy — uzgodniony i zamknięty.** `sim/economy` należy do M5 i to M5 buduje rdzeń
od pierwszego dnia:

| Funkcja | Kto pisze | Kiedy |
|---|---|---|
| `next_price`, `take_cogs`, `ledger_post`, `purchase_score`, `softmax_shares` | **M5** | w M5, jako rdzeń od razu |
| `wage_bid` | **M7** | wypełnia slot zadeklarowany przez M5 |
| `throughput` | **M6** | wypełnia slot zadeklarowany przez M5 |
| `interest_accrual`, `tax::apply` | M10 | WP10.2 |

**Zasada rdzenia, ustalona przez M5 i wiążąca dla wszystkich:** rdzeń **nie zna LOD, nie zna encji,
nie alokuje** — dostaje liczby i zwraca liczby, a identyfikatory wchodzą wyłącznie jako indeksy
katalogu danych. To jest mocniejsze sformułowanie mojej reguły i zastępuje ją.

Konsekwencja dla mnie jest dokładnie ta, po którą ten pakiet powstał: **makro woła ten sam kod co
mezo**, więc kryterium K2 (dok. 00 §4 / K-5) mierzy **błąd agregacji, a nie rozjazd dwóch
implementacji**. Bez tego cały §7.3 byłby testem podobieństwa dwóch modeli, czyli testem,
który zawsze można „naprawić" dostrajaniem.

**Kryterium ukończenia:** wszystkie testy M5–M7 zielone bez zmian; `cargo tree` pokazuje, że
`sim/macro` zależy od `sim/economy::kernel`, a nie odwrotnie; lint CI zabrania w `sim/macro`
literałów mnożników cenowych i stawek (skrypt grepowy — prymitywny, ale skuteczny).

**Rozmiar: L.**

---

### [x] WP10.1 — `sim/macro`: stan, krok, `lift`/`lower`

**Zależności:** WP10.2.

Cztery rzeczy:
1. `MacroState` / `MacroCell` / `MacroFirm` — struktury z §5.6.
2. `step()` — jeden dzień makro, osiem faz z §5.6.
3. `lift()` / `lower()` / `lower_cell()` — most mezo↔makro z gwarancją zachowania pieniądza co do grosza.
4. `MacroLodPolicy` — progi przełączania LOD, histereza i warunki blokady przejścia (M12 dostarcza
   wywołanie z pętli gry i `SpeedGovernor`, matematykę daje M10).

**Kryterium ukończenia:**
- `lift(lower(s)) == s` na wszystkich agregatach pieniężnych — **tolerancja 0 groszy**, test własnościowy
  na 1000 losowych stanów (proptest).
- `step()` zachowuje sumę pieniądza (emisja − destrukcja) — test własnościowy.
- Dwa przebiegi tego samego seeda → identyczny hash `MacroState` co 100 kroków.
- 7500 kroków makro dla metropolii ≤ 60 s jednowątkowo (criterion).
- `MacroState` metropolii ≤ 8 MB (test asercyjny na `size_of` + pojemnościach) — musi się dać
  sklonować kilkanaście razy dla równoległych „co jeśli" M7.

**Rozmiar: XL.**

---

### [~] WP10.3 — Historia „na sucho" (`DryRunConfig`, etapy D0–D5)

**Zależności:** WP10.1, generator M1–M2 (Etapy 1–8).

Pełny opis etapów w §5.7. Kryterium ukończenia:
- `--dry-run --years 80` na 64 seedach: 100% kończy się światem przechodzącym Etap 10
  (po ≤ 3 rundach `rebalance`), **> 80% bez żadnej rundy naprawczej**.
- Raport zawiera ≥ 50 wpisów kronikarskich z `provenance: DryRun` dla 80 lat.
- Rozwinięcie 400 tys. mieszkańców z makro do pełnego ECS ≤ 20 s.
- Determinizm: ten sam seed → identyczny hash świata po rozwinięciu.

**Rozmiar: L.**

---

### [~] WP10.4 — Test spójności LOD i rozszerzenie balansatora

**Zależności:** WP10.1–10.3.
Opis w §7.3. **Rozmiar: M.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.6 `sim/macro` — stan i krok

```rust
/// Komórka = (dzielnica × klasa społeczna). 40 dzielnic × 6 klas = 240 komórek.
pub struct MacroCell {
    pub key: (DistrictId, ClassId),
    pub citizens: Vec<CitizenSeed>,     // TOŻSAMOŚĆ — patrz §5.8
    pub age_hist: [u32; 18],            // kohorty 5-letnie
    pub cash: Money, pub deposits: Money, pub debt: Money,
    pub wealth_q: [Money; 4],           // min, q1, q3, max — do rozwinięcia rozkładu
    pub labor: [u32; N_ROLES],          // liczba zdolnych do roli
    pub skill_sum: [u32; N_ROLES],      // suma umiejętności (średnia = skill_sum/labor)
    pub employed: u32, pub unemployed: u32,
    pub need_sat: [Q; N_NEEDS],
    pub demand: MacroStock,             // popyt zrealizowany w ostatnim kroku
}

/// Firmy NIE są agregowane — zachowują tożsamość i są symulowane indywidualnie także w makro.
pub struct MacroFirm {
    pub id: FirmId, pub district: DistrictId, pub branch: BranchId,
    pub capital: Money, pub debt: Money,
    pub stock: MacroStock,              // jednostka natywna towaru — patrz niżej
    pub capacity_daily: Qty, pub utilization_bps: u16,
    pub employees: u32, pub wage_bill: Money,
    pub price: SparseVec<GoodId, Money>,
    pub brand_stock: i32,               // skalarny surogat marki (patrz niżej)
    pub tech: TechLevel,
    pub suppliers: SmallVec<[(GoodId, FirmId); 8]>,   // wynik dry-runu: „stali dostawcy"
}

pub struct MacroState {
    pub tick: Tick, pub day: u32,
    pub cells: Vec<MacroCell>,          // posortowane po kluczu — determinizm iteracji
    pub firms: Vec<MacroFirm>,          // posortowane po FirmId
    pub commute: CommuteMatrix,         // dzielnica × dzielnica × koszt (z §17.6, statyczna w makro)
    pub epoch: EpochState,
    pub ledger: Ledger,                 // suma pieniądza, ten sam typ co mezo
}
```

**Ziarno agregacji jest zamknięte z obu stron — i musi skalować się z populacją.**

Ziarna nie wolno **zagęścić**, bo poniżej ~500 osób na komórkę błąd zastąpienia losowania wartością
oczekiwaną przestaje się znosić i kryterium K2 (≤ 0,5%/miesiąc, §7.3) staje się nieosiągalne
ze statystyki, nie z implementacji. Nie wolno go też **zgrubić**, bo komórka przestaje odpowiadać
czemukolwiek, co gracz widzi na mapie. To jest ograniczenie, o którym łatwo zapomnieć przy pierwszej
prośbie o „większą rozdzielczość makro" — nie ma jej i nie będzie.

Stąd wniosek, którego stałe ziarno 40 × 6 nie spełnia. PRD §4.1 dopuszcza miasta od 20 tys.,
a §4.3 daje 10–40 dzielnic. Rachunek osób na komórkę przy 6 klasach:

| Rozmiar (§4.1) | Populacja | Dzielnic | Komórek (×6 klas) | Osób/komórkę | K2 osiągalne? |
|---|---|---|---|---|---|
| małe | 20 000 | 10 | 60 | 333 | **nie** |
| małe (górny) | 40 000 | 15 | 90 | 444 | **nie** |
| średnie | 60 000 | 20 | 120 | 500 | granica |
| średnie (górny) | 120 000 | 25 | 150 | 800 | tak |
| duże | 150 000 | 30 | 180 | 833 | tak |
| duże (górny) | 300 000 | 35 | 210 | 1 428 | tak |
| metropolia | 400 000 | 40 | 240 | 1 667 | tak |

Dwa pierwsze wiersze łamią własny kontrakt fazy. Dlatego ziarno **nie jest stałą**:

```rust
/// Liczba klas w ziarnie dobierana tak, by na komórkę przypadało ≥ MIN_CELL_POP osób.
pub const MIN_CELL_POP: u32 = 500;
pub fn cell_grain(pop: u32, districts: u16) -> ClassGrain;   // Classes6 | Classes3 | Classes2
```

Klasy łączymy parami po sąsiadujących przedziałach statusu (§5.4), nigdy losowo:
`Classes3` = (niższa + robotnicza), (niższa średnia + wyższa średnia), (wyższa + elita).
Dla miasta 20 tys. daje to 10 × 3 = 30 komórek i 666 osób na komórkę — kontrakt wraca do zakresu.
Odwzorowanie klasy na komórkę jest funkcją czystą, więc `lift()` i `lower()` działają bez zmian.

Test `macro_grain_meets_min_cell_pop` sprawdza to dla wszystkich siedmiu rozmiarów z §4.1 —
inaczej K2 przechodzi w CI na metropolii i cicho pada u gracza, który wybrał małe miasto.
> *ponytail: `ClassGrain` ma trzy warianty, nie parametr ciągły. Ziarno ciągłe wymagałoby
> przemapowania klas przy każdej zmianie populacji; trzy progi wystarczą na zakres 20 tys.–400 tys.*

**Jednostka towaru w makro: natywna, nigdy przeliczana.**

```rust
/// Ilość towaru w jednostce NATYWNEJ danego `GoodId`, zadeklarowanej w `data/goods/`.
/// Jeden i64, nie para — dwie liczby mogłyby się rozjechać, a wtedy zachowanie masy (K1) pada.
pub struct MacroStock(pub SparseVec<GoodId, i64>);
```

M6 (`sim/supply`) prowadzi towary masowe (ropa, zboże, cement) w `Mass` (gramy), a sztukowe w `Qty`
(milisztuki), i to `data/goods/` rozstrzyga, która jednostka jest wiodąca dla danego towaru.
`MacroStock` trzyma **dokładnie tę liczbę, którą trzyma M6**, i ani `lift()`, ani `lower()` jej nie
przelicza. Konsekwencja jest celowa: skoro nie ma konwersji, nie ma zaokrąglenia, a zachowanie masy
co do grama (K1, §7.3) wynika z tego konstrukcyjnie, a nie z ostrożnej arytmetyki.
> *ponytail: jedna liczba w jednostce z katalogu zamiast pary `(Mass, Qty)`. Para wymagałaby
> współczynnika przeliczeniowego per towar, utrzymywania go w zgodzie z M6 i testu na rozjazd —
> czyli trzech rzeczy zamiast zera.*

**Czego makro nie odtworzy: łańcucha pochodzenia partii.** `MacroStock` zna ilość, nie zna partii.
Gubi to, co M6 uważa w towarze za istotne: jakość, datę przydatności, koszt nabycia i markę.
Przy `lower()` partie **nie są odtwarzane — są generowane na nowo** z agregatu, z jakością i datą
przydatności wylosowanymi z rozkładu komórki (ten sam mechanizm kwantylowy co w §5.8).

Skutek dla „od pola do półki" (PRD §14.4): **ślad pochodzenia rwie się na granicy makro.**
Partia wygenerowana przez `lower()` dostaje `provenance: FromAggregate { district, last_supplier }`
i trace kończy się tam, a nie na złożu. Trzy rzeczy czynią to akceptowalnym:
1. `MacroFirm.suppliers` zachowuje **jeden skok wstecz** — a to jest właśnie ten wynik dry-runu,
   o który chodzi w §4.2 Etap 9 („relacje między firmami — stali dostawcy").
2. Partie z historii „na sucho" dostają `provenance: DryRun` i UI pokazuje je jako historię,
   a nie jako sfabrykowany łańcuch do złoża. Ten sam chwyt co przy wpisach kronikarskich —
   **lepiej przyznać się do braku danych niż zmyślić wiarygodny łańcuch.**
3. W trybie 50× (M12) ślad rwie się tylko dla towarów, które przeżyły całe okno przyspieszenia;
   wszystko wyprodukowane po powrocie do mezo ma pełny łańcuch.

To jest realna utrata funkcjonalności i jest wpisana jako D10 w §9 — nie da się jej naprawić
bez trzymania partii w makro, co przekreśla sens agregacji.

**Firmy zachowują tożsamość również w makro.** 3000 firm × ~400 B to 1,2 MB i kilkanaście operacji
dziennie każda — agregowanie ich byłoby oszczędnością bez pokrycia, a kosztowałoby dokładnie te
rzeczy, po które robimy dry-run: relacje dostawców, zadłużenie konkretnych firm, historię dzielnic.
Agregowani są **wyłącznie mieszkańcy**.

**Marka w makro.** Sloty per mieszkaniec nie istnieją w stanie uśpionym. `MacroFirm.brand_stock`
to skalarna wartość oczekiwana agregatu afinitetów; przy `lower` rozwijana do slotów proporcjonalnie
do znajomości w dzielnicy. Utrata informacji jest akceptowana i **jawnie udokumentowana**:
przejście mezo→makro→mezo gubi indywidualne historie marek, zachowuje rozkład.

**Krok makro — jeden dzień, osiem faz, wszystkie przez `econ_kernel`:**

| # | Faza | Częstotliwość | Co robi |
|---|---|---|---|
| 1 | Demografia | co 30 kroków | starzenie kohort, urodzenia, zgony, migracja z tablic zależnych od `need_sat` i bezrobocia |
| 2 | Rynek pracy | co krok | podaż roli w komórce × dostępność z `CommuteMatrix` vs popyt firm; płaca z `wage_kernel`, zmiana ograniczona do ±2%/dzień |
| 3 | Produkcja | co krok | `throughput()` per firma, ograniczone wejściami i załogą |
| 4 | Rynek dóbr | co krok | popyt komórki z potrzeb; alokacja do firm przez `softmax_shares(purchase_score(...))` — **ten sam kernel co §6.4** |
| 5 | Ceny | co krok | `next_price()` per firma per towar — **ten sam kernel co §6.3** |
| 6 | Finanse | co krok | odsetki, raty, podatki (stawki z M8), dywidendy, test wypłacalności, bankructwa |
| 7 | Relacje | co krok | dostawca wybrany w fazie 4 dla B2B podnosi `SupplierRelation.trust` |
| 8 | Zdarzenia | co krok | zredukowany katalog §11.2 z tymi samymi hazardami co w mezo |

`step()` jest jednowątkowy. 240 komórek × ~60 towarów + 3000 firm to ~10⁵ operacji na krok —
równoległość byłaby tu tylko źródłem niedeterminizmu bez zysku.

### 5.7 Historia „na sucho" — etapy

```rust
pub struct DryRunConfig {
    pub seed: u64,
    pub years: u16,                    // 30..=100
    pub start_year: i32,               // rok_startu_gry − years
    pub epoch_track: EpochTrackId,
    pub profile: EconProfile,          // §4.1
    pub target: PopTarget,             // §4.1 — docelowa wielkość NA KONIEC dry-runu
    pub step_days_early: u8,           // 6  — lata 1..N−5 (60 kroków/rok przy kalendarzu 360 dni)
    pub step_days_late: u8,            // 1  — ostatnie 5 lat
    pub max_rebalance_rounds: u8,      // 3
    pub chronicle: bool,
}
```

Tabela jest **poprawiona po M10a**: pierwsza wersja opisywała cztery etapy z sześciu,
bo D1 i D2 nie miały wiersza — a nazwa „etapy D0–D5" sugerowała, że mają. Kolumny
„co jest symulowane / agregowane" były we wszystkich wierszach puste i zostały usunięte:
w makrze **wszystko jest agregatem** poza firmami (`D21`), więc kolumna z jedną możliwą
odpowiedzią nie była pytaniem.

| Etap | Nazwa | Wynik |
|---|---|---|
| **D0** | Zasiew | `lift()` stojącego świata po Etapie 8 generatora. Miasto **nie jest zmniejszane** (`E-2`): populacja startowa jest populacją docelową, a historia zmienia strukturę wieku, rozmieszczenie, majątek, ceny, zapasy, zadłużenie i sieć dostawców. Parcele, drogi i teren nie są symulowane. |
| **D1** | Bieg | `years` lat kroku makro. Krok jest zmienny: `step_days_early` (domyślnie 6) w latach wczesnych, `step_days_late` (1) w ostatnich pięciu — bo to one ustawiają stan początkowy partii. Każda faza mnoży swój przepływ przez długość kroku (`E-7`). |
| **D2** | Epoki | **Nie powstaje w M10a** (`E-3`). Postęp technologiczny epoki to PRD §11.3, czyli WP10.9 i podfaza M10c; drugi mechanizm epok obok tamtego byłby dokładnie tym, przed czym broni `K-8`. |
| **D3** | Kronika | Zdarzenie o skali większej niż próg → `ChronicleEvent` z `provenance: DryRun`: wstrząs w skali miasta, dzielnica tracąca albo zyskująca > 40 ‰ ludności w ciągu roku, runda naprawcza. Próg jest **wyższy niż dla zdarzenia w partii** (`R9`): celem jest 50–200 wpisów na 80 lat, nie 50 000. |
| **D4** | Rozwinięcie (`lower`) | `MacroState` naniesiony na świat ECS: pieniądz gospodarstw w komponentach, salda firm w księgach, różnica sektora GD domknięta na rachunku reszty świata. Mechanizm w §5.8. Wykonuje się **po** naprawie, bo świat ma dostać stan, który przeszedł bramki. |
| **D5** | Weryfikacja i naprawa | Etap 10 PRD: osiem bramek i do trzech rund `rebalance`. Kryteria i procedura niżej. |

**D5 — weryfikacja (§4.2 Etap 10: „żaden rynek nie jest w stanie nierównowagi > 30%").**

Nierównowagę definiujemy jako **średnią z ostatnich 30 dni dry-runu**, nie z jednego dnia — pojedynczy
dzień jest zaszumiony i dawałby fałszywe alarmy:

```
imbalance(g) = |podaż_dzienna(g) − popyt_dzienny(g)| / max(podaż_dzienna(g), popyt_dzienny(g))
```

Bramki Etapu 10 (wszystkie muszą przejść):

| # | Kryterium | Próg |
|---|---|---|
| 1 | `imbalance(g)` dla każdego `GoodId` | ≤ 30% |
| 2 | Pokrycie zapasem każdego konsumowanego towaru | ≥ 3 dni |
| 3 | Każda firma: ≥ 1 pracownik i ≥ 1 dostawca dla każdego wejścia receptury | 100% |
| 4 | Bezrobocie | 3%–15% |
| 5 | Mediana `dług/aktywa` firm | 0,10–0,60 |
| 6 | Odsetek firm niewypłacalnych w dowolnej dzielnicy | < 40% |
| 7 | Gini majątku GD | 0,25–0,45 |
| 8 | Koszyk podstawowy / mediana dochodu GD | 0,25–0,55 |
| 9 | Każdy mieszkaniec ma dom; grafy dróg spójne | 100% (z Etapów 4–6, nie z makro) |

**Naprawa, nie odrzucenie seeda.** Odrzucenie 40% seedów byłoby porażką generatora i wściekłością
gracza, który wybrał seed. Do 3 rund `rebalance()`:

1. **Zapasy:** dosypanie do minimum pokrycia po koszcie z księgowaniem (nie z powietrza — jako import
   zaciągnięty w ostatnim miesiącu, z długiem).
2. **Ceny:** przesunięcie do punktu równowagi logitowej wyliczonego z `softmax_shares` — analitycznie,
   jednym krokiem Newtona, nie iteracyjnie.
3. **Brakujący dostawca:** dopisanie importera z §6.2 warstwa 3 zamiast tworzenia firmy z niczego.
4. **Zadłużenie:** redukcja poprzez zdarzenie restrukturyzacji, zapisane w kronice.

Po 3 nieudanych rundach: regeneracja z `seed + 1` i jawny komunikat w logu. Cel w CI (balansator,
64 seedy): **100% seedów przechodzi, > 80% bez żadnej rundy naprawczej.** Spadek poniżej 80% jest
sygnałem, że parametry gospodarki się rozjechały — to jest właściwy wczesny alarm dla całego projektu,
nie tylko dla M10.

### 5.8 Tożsamość w stanie uśpionym i rozwinięcie makro→mezo

To jest sedno techniczne fazy. §17.4 wymaga: „agregaty per dzielnica × klasa, **z zachowaniem
tożsamości** (tylko stan uśpiony)" oraz „przejście makro→mezo odtwarza indywidualne stany
z zachowanych tożsamości (deterministycznie z seedu + agregatu)".

**Czym jest uśpiony mieszkaniec.** Trzema rzeczami i niczym więcej:

```rust
pub struct CitizenSeed {
    pub birth_index: u32,   // globalnie unikalny, monotoniczny numer narodzin/przybycia
    pub cell: u16,          // indeks MacroCell, do której należy
}
```

8 bajtów. 400 tys. mieszkańców = **3,2 MB**. Czyli: tożsamość jest zachowana **dosłownie i w całości** —
wiemy dokładnie, kto istnieje i do której komórki należy. Nie jest zachowany **stan indywidualny**
(gotówka, umiejętność w roli, nastrój, pamięć, sloty marek) — on jest odtwarzany.

Trwały identyfikator i cała osobowość mieszkańca wywodzą się czysto:
`personality(world_seed, birth_index)` → wszystkie cechy stałe z §5.1 (ambicja, oszczędność, lojalność…),
imię, nazwisko, płeć, data urodzenia. **Cechy stałe nigdy nie muszą być przechowywane w makro** —
są funkcją seeda. To jest jedyne miejsce, gdzie determinizm z dok. 00 §3 daje nam oszczędność
zamiast kosztu.

**`lower()` — rozwinięcie agregatu do jednostek, krok po kroku.** Dla każdej komórki i każdej
wielkości X (gotówka, majątek, umiejętność w roli r, zaspokojenie potrzeby n):

1. Weź z komórki: `suma` (i64) oraz kwantyle `wealth_q` (min, q1, q3, max).
2. Ustal **porządek rangowy**: posortuj `citizens` po `hash(world_seed, StreamId::MacroLower,
   birth_index, quantity_id)`. Porządek jest funkcją czystą — niezależny od kolejności iteracji,
   od wątków, od historii wstawień do wektora.
3. Przypisz wartości z **kwantylowej funkcji odwrotnej** dopasowanej do (suma, kwantyle):
   log-normalna dla pieniądza i majątku, beta dla skal `Q`. Dopasowanie dwuparametrowe,
   rozwiązywane analitycznie z q1/q3 — bez iteracji.
4. **Korekta reszty:** po zaokrągleniu do i64 licz `r = suma − Σ przypisanych` i dodaj `r`
   do pierwszego wg posortowanego klucza (dok. 00 §2, zasada podziału kwoty).

Punkt 4 daje twardą gwarancję: **`lower()` zachowuje pieniądz co do grosza**. Nie „w przybliżeniu",
nie „z tolerancją" — dokładnie. Odwrotnie `lift()` sumuje po posortowanym kluczu i wyznacza kwantyle.
Stąd kontrakt testowalny z tolerancją 0:

```
lift(lower(s)) == s     na wszystkich polach Money
```

**Czego `lower` nie odtwarza i dlaczego to jest w porządku.** `lower` odtwarza **rozkład**, nie
konkretne życiorysy. Anna Wiśniewska po rozwinięciu ma tę samą tożsamość, osobowość, wiek i rodzinę,
ale jej saldo bankowe jest losowaniem z rozkładu jej komórki, a nie sumą jej faktycznych 80 lat
transakcji — bo tych transakcji nie było. To jest cena makro i jest zamierzona.

**Graf relacji w uśpieniu: wyrzucany, nie przechowywany.** Graf z §5.1 nie jest stanem niezależnym —
jest funkcją struktury: rodzina (jawna, mało encji), sąsiedztwo (parcela), współpraca (zakład),
plus „stare przyjaźnie" generowane z seeda pary. Przy `lower` odtwarzamy go z tych czterech
generatorów. Oszczędność w trybie 50× (M12) jest duża, a informacja utracona — nieobserwowalna,
bo w makro nikt po tym grafie nie chodzi.

**Rodziny i gospodarstwa domowe zachowują tożsamość jawnie.** GD jest jednostką ekonomiczną (§5.2),
jest ich ~150 tys. przy 400 tys. mieszkańców, a ich majątek i mieszkanie to rzeczy, których rozkład
nie wystarcza (mieszkanie jest konkretną parcelą). `HouseholdId`, skład, adres i majątek GD są trzymane
jawnie także w makro: 150 tys. × 48 B = 7,2 MB. **To jest jedyne odstępstwo od agregacji mieszkańców
i jest świadome** — bez niego „majątki rodzin" z §4.2 Etap 9 nie mają nośnika.

---

## Zmiany wpisane po M10a

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.
Rozstrzygnięcia dotykające kontraktu z dokumentu 00 mają wpis `K-78` w §4a.

| # | Zmiana | Dlaczego |
|---|---|---|
| E-1 ★ | **`SeedWorld` nie powstaje, a `dry_run` bierze `&mut World`.** Sygnatura z §6 dokumentu fazy brzmiała `dry_run(cfg, seed_world: &SeedWorld)`; jest `dry_run(cfg: &DryRunConfig, world: &mut World, params: &MacroParams) -> DryRunResult` | `CitizenSeed.birth_index` to indeks encji ECS — tak wypełnia go `lift` od M7f. Świat **jest** zasiewem, więc osobny typ opisywałby drugi raz to, co `World` już niesie (`K-78` pkt 2) |
| E-2 ★ | **Etap D0 nie zmniejsza miasta, a populacja jest w kroku makro zamknięta.** §5.7 zapowiadał start od 30–50 % docelowej ludności; historia startuje z pełnej populacji stojącego świata i tę populację ma na końcu. Zmienia się struktura wieku, rozmieszczenie między dzielnicami, majątek, ceny, zapasy, zadłużenie i sieć dostawców | Nowy mieszkaniec w makrze musiałby dostać indeks encji, której jeszcze nie ma, a zmarły — zostawić encję do usunięcia. Jedno i drugie czyni z `lower()` **drugi generator populacji** obok Etapu 8 (`K-8`). Wzrost ludności wymaga churnu encji i należy do M12c, gdzie i tak musi powstać |
| E-3 ★ | **Etapu D2 nie ma.** Tabela w §5.7 opisywała cztery wiersze z sześciu — D1 i D2 nie miały opisu. D1 jest zaimplementowany jako bieg historii; D2 (epoki i postęp technologiczny) **nie powstaje w M10a** | Postęp epoki to PRD §11.3, czyli WP10.9 i podfaza M10c. Wpisanie go tutaj znaczyłoby drugi mechanizm epok obok tamtego |
| E-4 | **`lower_cell` nie używa odwrotnej dystrybuanty log-normalnej.** §5.8 pkt 3 zapowiadał dopasowanie rozkładu dwuparametrowego; jest **profil kwantylowy w liczbach całkowitych** — interpolacja między `wealth_q` po pozycji w rankingu, a podział przez `core::split_proportional` | Kwantyle są tym, co komórka **zna**, a dopasowanie rozkładu do czterech kwantyli i tak sprowadza się do interpolacji między nimi. Float w drodze do kwoty pieniężnej jest zabroniony (00 §2) nawet przejściowo, a tutaj nie jest do niczego potrzebny |
| E-5 ★ | **`lift(lower(s)) == s` obowiązuje na sumach, nie na kwantylach.** Kryterium WP10.1 mówi „wszystkie agregaty pieniężne"; `wealth_q` jest **kształtem**, a nie agregatem, i rozwinięcie odtwarza go z dokładnością profilu, nie co do grosza | Suma jest niezmiennikiem księgowym i da się ją zagwarantować konstrukcyjnie. Kwantyl jest statystyką próby i jego równość wymagałaby, żeby profil był dokładną dystrybuantą świata — czyli żeby makro trzymało rozkład, a nie cztery liczby |
| E-6 | **Równość per komórka ma jeden warunek: gospodarstwo nie może mieć członków w dwóch komórkach.** `lift` dzieli pieniądz gospodarstwa **równo między komórki jego członków**, więc gospodarstwo o członkach w dwóch klasach społecznych wraca inaczej, niż wyszło. Suma jest ta sama; `LowerReport.straddling_households` **liczy** takie gospodarstwa zamiast o nich milczeć | Rozwiązaniem byłaby komórka po gospodarstwie, a nie po osobie — czyli inne ziarno agregacji i inny próg `MIN_CELL_POP`. Liczba w raporcie jest tańsza i mówi, ile dokładnie kosztuje przybliżenie |
| E-7 ★ | **Krok makro może reprezentować wiele dób** (`MacroParams::days_per_step`) i **każda faza mnoży przez tę liczbę swój przepływ**: listę płac, odsetki, minuty pracy linii, budżet zakupowy, tempo rekrutacji i hazard wstrząsu. Kadencje (przecena, przegląd dostawców, rocznica) pytają `step::przekroczono`, a nie resztę z dzielenia | Bez mnożnika osiemdziesiąt lat liczone krokiem sześciodobowym wypłaciłoby jedną szóstą należnych płac. Bez `przekroczono` krok sześciodobowy **mija granicę okresu bez trafienia w nią** i przegląd dostawców nie odbyłby się ani razu — a faza wyglądałaby na działającą, bo test z krokiem jednodobowym przechodzi |
| E-8 ★ | **Przecena wypada raz na `reprice_every_days` (domyślnie 4), a nie codziennie.** `data/economy/shop.ron` daje każdej firmie czujność 1–7 dób; makro nie ma osobowości firm, więc bierze jedną kadencję dla wszystkich — środek tamtego przedziału | Przecena codzienna, którą M7f tu zostawił, była modelem **agresywniejszym od mezo**: ceny zbiegały w „co jeśli" szybciej niż w przebiegu, który ten „co jeśli" przewidywał. Przy okazji krok zmieścił się w budżecie §7.5 |
| E-9 ★ | **Komórka ocenia najwyżej `candidates_per_good` (8) najtańszych ofert na towar**, a nie wszystkie oferty dzielnicy | Mieszkaniec w M5 pyta o `k_min..k_max` ofert (`data/economy/choice.ron`) i wybiera spośród nich. Komórka oceniająca każdą półkę w dzielnicy **wiedziałaby o rynku więcej niż mieszkaniec, którego zastępuje** — to jest rozjazd modeli, nie oszczędność. Rachunek „240 komórek × ~60 towarów" z §5.7 daje 10⁵ operacji na krok **tylko** przy takim zawężeniu |
| E-10 | **Faza 6 dostaje kanał kredytu obrotowego.** Firma, która nie ma z czego zapłacić ludziom, pożycza od **reszty świata** (przelew `RestOfWorld → Firms`, `debt` rośnie o tę samą kwotę), do sufitu `max_leverage_permille` | Bez tego `MacroFirm.debt` nigdy nie rośnie: `lift` odczytuje dług jako ujemne saldo rachunku, a świeżo postawione miasto ma wszystkie salda dodatnie. Historia obiecuje „zadłużenie firm" (PRD §4.2 Etap 9), a oddawałaby same zera — bramka 5 Etapu 10 mierzyłaby **brak mechanizmu**, nie stan świata. Sufit jest konieczny, bo makro nie ma postępowania upadłościowego (`K-10` przyznaje je M7): bez niego firma trwale nierentowna pożycza co dobę przez osiemdziesiąt lat i saldo reszty świata wychodzi poza zakres `i64` |
| E-11 ★ | **Naprawa „brakujący dostawca" nie powstaje.** §5.7 wymieniał cztery naprawy; są trzy (zapasy, ceny, restrukturyzacja zadłużenia) | Zatowarowanie w fazie 3 idzie z importu, więc **każda firma ma dostawcę z konstrukcji**. Naprawa opisywałaby stan, który nie zachodzi — a naprawa, której nikt nigdy nie uruchomi, przechodzi każdy test i wygląda tak samo jak działająca |
| E-12 ★ | **Kryterium WP10.3 „100 % ziaren przechodzi Etap 10" nie jest spełnione po M10a** i to jest stan zapisany, nie przemilczany. Mechanizm działa w całości: osiem bramek jest **zmierzonych**, trzy rundy naprawcze się wykonują, kronika powstaje, pieniądz domyka się co do grosza. Na świeżo wygenerowanym mieście 4 km cztery bramki świecą na czerwono: nierównowaga (1), firmy bez obsady (3), mediana dźwigni (5) i współczynnik Giniego (7) | Przyczyna bramek 1 i 3 jest jedna i nazwana w `step::labor`: **płaska `CommuteMatrix`** zamyka rekrutację w granicach dzielnicy, więc firmy w dzielnicach przemysłowych bez mieszkań nie dostają obsady, nie produkują, a rynek zjada zapas z Etapu 7. Otwarcie puli na całe miasto zostało spróbowane i **cofnięte** — bezrobocie schodzi wtedy do zera, a odsetek firm bez obsady rośnie z 291 ‰ do 342 ‰, bo bez kosztu dojazdu rekrutacja jest albo zakazana, albo darmowa. Rozstrzyga to wypełnienie macierzy przez M4 (kontrakt M10 §6), a nie zmiana progu w bramce. Bramka 7 mówi o **generatorze**: rozkład majątku gospodarstw po Etapie 8 ma Gini rzędu 0,50–0,84, a plan oczekuje 0,25–0,45 — to jest liczba do rozstrzygnięcia przez właściciela produktu (§9.2 dokumentu fazy) |
| E-13 ★ | **Rozszerzenia balansatora nie ma.** WP10.4 dostarcza test spójności LOD (`tools/headless/tests/macro_lod.rs`, pięć przypadków w CI) i przebieg `headless dry-run`; bramki G12+ w `tools/balansator` nie powstały | Bramka balansatora musiałaby porównywać przebieg mezo i makro tego samego scenariusza przez rok gry — a przebieg mezo roku gry kosztuje ~10 min na ziarno (`U-24`). To jest bieg nocny z własnym budżetem i własnym raportem, czyli praca wielkości pakietu, a nie dopisek. Adresat: M10f razem z domknięciem fazy |
| E-14 | **Zależność `sim/macro` → `magnat-supply` zostaje i ma czytelnika**: `step::produce` woła `magnat_supply::PlantSite::FULL_LABOR` przy ograniczaniu pokrycia etatowego | Zapis dla porządku: przegląd crate'u po M7f wskazywał tę zależność jako nieużywaną |
| E-15 | **Bramka 8 (koszyk do dochodu) potrzebuje koszyka od wołającego.** `DryRunConfig.basket: Vec<(GoodId, i64)>` — pusty koszyk znaczy „nie zmierzono" i raport mówi to wprost | `sim/macro` nie ma katalogu towarów i mieć nie powinien (`K-50`: rdzeń nie zna encji ani katalogu). Koszyk `data/economy/cpi.ron` składa scenariusz gry; przebieg `headless dry-run` bierze na razie dwanaście towarów o najszerszej dostępności i mówi o tym w kodzie |
