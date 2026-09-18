# M9e — Panele, czas, kariera

Podfaza 5 z 5 fazy **M9 — Gracz: kariera** (`M9-gracz-kariera.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M9c (karta inspekcji, nakładki), M9d (polityki). Czas i śledzenie opisuje §5.10 — jest w `M9c`. |
| **Pakiety robocze** | WP10, WP11, WP12 |
| **Projekt techniczny** | §5.4, §5.9, §5.11, §5.12 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: pełna ścieżka kariery, panele biznesowe, automatyzacja polityk. |
| **Kryterium zamknięcia** | Kryteria WP10–WP12 oraz bramki 1–7 fazy M9 w `00-postep.md`. |
| **Poprzednia / następna** | `M9d-jezyk-regul.md` · — (ostatnia w fazie) |

Dziewięć paneli biznesowych z `PanelRegistry`, sterowanie czasem ze `StopCondition` i trybem „śledź”, kronika z wyszukiwaniem oraz kariera, scenariusze, porażka i onboarding.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| ✅ **WP10** | Panele biznesowe | WP5–WP9 | 9 paneli z §14.3: pulpit, zakład (Gantt), sklep, łańcuch dostaw (graf), rynek, ludzie, finanse, miasto, kronika + `PanelRegistry`; **komendy i wykonawcy** dla paneli operacyjnych (`DH-3`) | Każdy panel spełnia budżet klatki; z każdego panelu **operacyjnego** da się wydać co najmniej jedną `PlayerCommand` — lista w `PanelId::is_operational`, kronika jawnie pominięta (`DI-8`) |
| ✅ **WP11** | Czas, śledzenie, kronika | WP5, WP10 | `TimeScale` (= `SimSpeed`, `DI-2`), `StopCondition` jako **zestaw** (`DI-1`), tryb „śledź", karta partii z pełnym śladem, magazyn kroniki zbierający dzienniki `sim/*` (`DI-4`) | Śledzenie partii od pola do półki z czasem i kosztem etapów; „zatrzymaj gdy brak towaru" działa przy 10× |
| ✅ **WP12** | Kariera, scenariusze, porażka, onboarding | WP4, WP10, WP11 | `CareerTier`, `Scenario`/`Objective`/`Goal`, `WorldPatch`, 5 scenariuszy z §13.3 w `data/scenarios/scenarios.ron`, bankructwo osobiste, sukcesja, pomiar §20.3 z dziennika replayu | **Domknięte.** Bankructwo i śmierć nie kończą sesji i **obie mają drogę wejścia**: `GameState::settle` woła `legacy::check` raz na dobę, w kliencie i w przebiegu bezgłowym. Scenariusz sieci pięćdziesięciu przechodzi do końca i **prowadzi do ekranu domknięcia**; samouczek ma trzy kroki, każdy pomijalny; budżet dwunastu interakcji mierzony i dotrzymany (`DI-33`…`DI-38`) |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 Kariera bez blokad

```rust
pub enum CareerTier { Employee, FirstBusiness, Company, Group, Magnate }

impl CareerTier {
    /// WYPROWADZANY z obserwowalnych faktów, nigdy nie ustawiany i nigdy nie bramkujący.
    pub fn derive(snap: &Snapshot, p: &PlayerCharacter) -> CareerTier {
        // Employee:      brak firm
        // FirstBusiness: 1 zakład, ≤ 3 zatrudnionych
        // Company:       ≥ 2 zakłady lub ≥ 1 menedżer
        // Group:         ≥ 2 branże lub integracja pionowa (dostawca wewnętrzny)
        // Magnate:       udział > 25% w rynku ≥ 1 towaru lub ≥ 5% zatrudnienia miasta
    }
}
```

Tier służy **wyłącznie** kronice, tytułom i statystykom. Żadna komenda nie sprawdza tieru.
Jedynymi ograniczeniami są kapitał, ludzie, informacja i czas (§13.2). Test akceptacyjny w §7
sprawdza to wprost: w wariancie `Sandbox` gracz może wydać `FoundFirm` w ticku 1.

### 5.9 Panele biznesowe (§14.3)

`PanelRegistry` — punkt rozszerzenia dla M10 (marka, R&D, giełda, media) i M12 (mody):

```rust
pub struct PanelDesc {
    pub id: PanelId, pub title: LocKey, pub deps: &'static [DataSourceId],
    pub build: fn(&Snapshot, &PanelState) -> LayoutNode,
    pub min_tier: Option<CareerTier>,   // TYLKO do domyślnego układu i podpowiedzi, nie do blokowania
}
```
M10 rejestruje `PanelId::Brand`, `PanelId::Rnd`, `PanelId::Stock` bez dotykania kodu M9.
`min_tier` nie blokuje otwarcia panelu — decyduje tylko o tym, czy panel jest domyślnie
przypięty w układzie nowego gracza (onboarding, §5.12).

| Panel | Zawartość | Główne widgety | Konsumuje z | Odświeżanie |
|---|---|---|---|---|
| **Pulpit firmy** | przepływy pieniężne (30 dni / 12 mies.), alerty (niedobór, strajk, awaria, **eskalacja polityki — `Market::policy_inbox()`**), KPI: marża, **obrót netto** (bez VAT, K-7), zatrudnienie, płynność | wykres, lista alertów, kafelki KPI | M5 (księgowość, **skrzynka eskalacji polityk** — `DH-1`), M7 (HR), M8 (zdarzenia), M9 (eskalacje) | `EveryHour` |
| **Zakład** | produkcja (bieżące zlecenia), magazyn wg partii, załoga i zmiany, maszyny i ich stan, **dostawy jako Gantt**, koszty w rozbiciu | Gantt, `Table<Batch>`, `Table<Employee>`, wykres kosztów | M6 (produkcja, partie), M7 (załoga), M4 (dostawy) | `EveryHour`, Gantt `EveryMinute` przy otwartym |
| **Sklep** | półki (asortyment, ceny, rotacja, dni do przydatności), klienci (skąd, kto, dlaczego), **utracone wizyty z powodami**, konkurencja w zasięgu z ich cenami. **Wszystkie ceny detaliczne pokazywane brutto, porównanie z konkurentem zawsze brutto do brutto (K-7)**; kolumna netto opcjonalna i jawnie podpisana | `Table<ShelfRow>`, histogram powodów, minimapa zasięgu, lista `LostSale` | M5 (transakcje, użyteczność, `Offer.price_basis`), M9 (`LostSale`) | `EveryHour` |
| **Łańcuch dostaw** | graf dostawców i odbiorców z przepływami (grubość = wolumen), kontrakty, **ryzyka: jeden dostawca = czerwony węzeł**, czas i koszt na krawędzi | `GraphView`, `Table<Contract>` | M6 (zlecenia, kontrakty), M4 (czas transportu) | przy zmianie topologii; przepływy `EveryDay` |
| **Rynek** | dla produktu: wszystkie oferty w mieście (tabela), historia cen (wykres), wolumeny, udziały rynkowe. Oferty detaliczne i hurtowe **nigdy nie są mieszane w jednym szeregu** — przełącznik podstawy (brutto/netto) nad wykresem, podstawa w nagłówku kolumny (K-7) | `Table<Offer>` (do 100k), wykres, wykres udziałów | M5/M6 (oferty, `Offer.price_basis`) | `EveryHour` |
| **Ludzie** | pracownicy, kandydaci, menedżerowie; drzewo organizacyjne; umiejętności, płace vs mediana rynku, rotacja | `Table<Person>`, drzewo, wykres płac | M7 (HR, rynek pracy), M3 (mieszkańcy) | `EveryDay` |
| **Finanse** | księgi (RZiS, bilans, przepływy), kredyty i harmonogramy spłat, wycena; **miejsce na giełdę — M10**. **Przychód netto i VAT należny to dwa osobne wiersze, nigdy nie sumowane (K-7)**: VAT jest zobowiązaniem wobec miasta, nie przychodem — pokazywany w bilansie po stronie zobowiązań, z terminem rozliczenia, a nie w RZiS | `Table<LedgerEntry>`, wykresy, harmonogram | M5 (księgowość, banki), M8 (stawki, terminy) | `EveryDay` |
| **Miasto** | polityka i podatki, budżet miasta, przetargi, wybory i sondaże, media | tabele, wykres budżetu, oś wyborcza | M8 (`sim/city`) | `EveryDay` |
| **Kronika** | dziennik świata i gracza w stylu DF Legends, przeszukiwalny, filtry (aktor, typ, ważność, zakres dat) | `Table<ChronicleEntry>` z wyszukiwaniem, oś czasu | M9 + zdarzenia wszystkich faz | na żądanie |

Z każdego panelu da się wydać co najmniej jedną `PlayerCommand` — panel, z którego można tylko
patrzeć, jest raportem, a nie narzędziem, i nie spełnia kryterium ukończenia WP10.

### 5.11 Scenariusze, cele, kronika, porażka

```rust
pub struct Scenario {
    pub id: ScenarioId, pub title: LocKey, pub brief: LocKey,
    pub world_seed: u64, pub world_preset: WorldPresetId, pub start_epoch: EpochId,
    pub start: StartVariant,
    pub patches: Vec<WorldPatch>,           // „upadająca huta" = konkretny zakład z długiem
    pub objectives: Vec<Objective>,
    pub fail_conditions: Vec<ConditionExpr>,
    pub time_limit: Option<SimMinute>,
    pub tutorial: Option<TutorialScriptId>,
}
pub struct Objective {
    pub id: ObjectiveId, pub title: LocKey,
    pub goal: Goal, pub optional: bool,
    pub deadline: Option<SimMinute>, pub reveal_after: Option<ObjectiveId>,
}
pub enum Goal {
    MarketShare  { good: GoodId, area: AreaSpec, min_bp: u16 },   // „Zmonopolizuj paliwo w 10 lat"
    SiteCount    { kind: SiteKind, min: u32 },                    // „Zbuduj sieć 50 sklepów"
    Employment   { min_headcount: u32, rank: Option<u8> },        // „Zostań największym pracodawcą"
    FirmSolvent  { firm: FirmId, for_days: u32 },                 // „Uratuj upadającą hutę"
    ProductLaunched { recipe: RecipeId, units_sold: Qty },        // „własna marka samochodów" (M10)
    NetWorth     { min: Money },
    Custom(ConditionExpr),                                        // ta sama gramatyka co reguły
}
```
Ewaluacja celów: `EveryDay`, nigdy `EveryMinute`. Postęp `0..=10000 bp` pokazywany w panelu celów.
Scenariusze w `data/scenarios/*.ron` — moddowalne bez kodu od pierwszego dnia.

```rust
pub struct ChronicleEntry {
    pub id: ChronicleId, pub at: SimMinute,
    pub kind: ChronicleKind,
    pub actors: SmallVec<[ChronicleActor; 4]>,
    pub payload: ChroniclePayload,          // typowane dane, NIGDY gotowy string
    pub importance: u8,                     // 0..=100
    pub scope: ChronicleScope,              // World | Player | Firm(FirmId) | District(DistrictId)
}
```
Tekst powstaje dopiero przy wyświetleniu, z szablonu lokalizowanego — inaczej kronika nie da się
przetłumaczyć i puchnie. Magazyn: log dopisywany, obok zapisu gry, rekord ≤ 48 B + indeks
wtórny po `(tick, actor, kind)`. Skala: ~100–200 wpisów/dobę dużego miasta → 100 lat ≈ 5–7 mln
wpisów ≈ 300 MB bez czyszczenia. Dlatego **decymacja po ważności**: wpisy `importance < 30`
starsze niż 5 lat gry są agregowane do podsumowań rocznych. Wpisy dotyczące gracza i jego firm
nigdy nie są usuwane.

**Osiągnięcia emergentne = zapytania do kroniki.** „Twoja firma przetrwała 3 recesje" to zapytanie
`count(ChronicleKind::RecessionEnded) ≥ 3 AND firm_solvent_throughout`. Zero osobnego systemu.

**Porażka (§13.4).** `DeclarePersonalBankruptcy` (lub automatyczne przy niewypłacalności):
majątek firm likwidowany, reszta długu osobistego zostaje z harmonogramem spłat, `Reputation`
obrywa (banki M5/M7 muszą to widzieć przy ocenie zdolności — kontrakt), `PlayerAutonomy::job`
wraca na `Manual`, `CareerTier::derive` naturalnie zwraca `Employee`. **`GameState` pozostaje
`Playing`.** Gra się nie kończy i nie ma ekranu porażki — jest wpis w kronice o wysokiej ważności.

**Śmierć i dziedziczenie.** Zgon postaci przychodzi z demografii M3 — nie mamy osobnej śmierci
dla gracza. `GameState::Succession`: dziedzic z `SetHeir` albo automatycznie (dorosłe dziecko →
małżonek → rodzeństwo o najlepszej relacji). Przechodzi: własność firm (minus podatek spadkowy
wg prawa M8), zobowiązania, reputacja nazwiska. **Nie przechodzi: relacje osobiste i umiejętności** —
dziedzic ma własną sieć i własne kompetencje, i to jest prawdziwy koszt śmierci, znacznie
ciekawszy niż kara pieniężna. Brak dziedzica → ekran „spuścizna" z kroniką dynastii i wyborem
`ContinueAsNewCitizen` (nowa dynastia w tym samym świecie) albo zakończenie.

### 5.12 Onboarding — przełożenie §20.3 na wymagania

Metryka: **czas do pierwszej sensownej decyzji < 15 min**. Definicja operacyjna: pierwsza
`PlayerCommand` z zestawu `{SetPrice, AttachPolicy, OpenSite, AcceptJobOffer, HireCandidate}`.
Po M9d istnieją z tego **dwie pierwsze**; pozostałe trzy wchodzą w WP10 i WP12 razem
ze swoimi panelami (`DH-2`). Do tego czasu metryka jest mierzalna, ale mierzy węższy
zbiór decyzji, niż będzie mierzyła po domknięciu fazy.

Pomiar bez osobnej telemetrii: strumień `ViewCommand` niesie `wall_ms`, a strumień
`PlayerCommand` — `seq`. Czas do pierwszej sensownej decyzji liczymy **offline z dziennika
replayu dowolnego playtestu**. Jeden mechanizm (replay) obsługuje odtwarzanie błędów i metryki
gracza.

Wymagania wyprowadzone z budżetu 15 minut:

1. **Domyślny start to `ExperiencedWorker`, nie `Graduate`.** Absolwent bez kapitału jest
   ciekawszy fabularnie, ale odsuwa pierwszy biznes o godziny gry. Graduate zostaje jako wybór.
2. **Domyślny scenariusz samouczka: „Pierwszy sklep"**, mapa mała, populacja ~20 tys.,
   wyraźna nisza (dzielnica bez sklepu spożywczego) wygenerowana przez `WorldPatch`.
3. **Trzy kroki, ≤ 12 interakcji łącznie, ≤ 3 panele:**
   - (0–3 min) *Kim jesteś* — kamera na domu postaci, karta inspekcji własnej postaci,
     jedno kliknięcie „śledź siebie", doba w 3×.
   - (3–8 min) *Czego brakuje* — nakładka `ShopCatchment` z podświetloną dziurą, karta inspekcji
     sąsiada pokazująca `NotInChoiceSet { TooFar }` — gracz sam widzi popyt.
   - (8–15 min) *Otwórz i wyceń* — `OpenSite` na wskazanej parceli, `SetShelfAssortment`
     (domyślny koszyk jednym kliknięciem), `SetPrice`. Koniec samouczka.
4. **Każdy krok pomijalny**, żaden tekst dłuższy niż 40 słów, zero modalnych okien blokujących.
5. **Domyślny układ nowego gracza: 2 panele** (Sklep, Pulpit). Reszta dostępna, ale nie przypięta
   (`PanelDesc::min_tier`). Panel Finanse z pełną księgowością otwarty w 5. minucie zabija metrykę.
6. **Zwrot w ciągu jednej doby gry.** Po `SetPrice` samouczek przestawia czas na 3× i gwarantuje,
   że w ciągu doby gry panel Sklep pokaże ≥ 3 nazwanych klientów i ≥ 3 nazwane utracone wizyty
   z powodami. Pierwsza decyzja musi dostać wyjaśnioną odpowiedź, inaczej druga metryka z §20.3
   („rozumiem, dlaczego przegrałem", > 80%) nie ma szans.
7. **Test CI (nie ankieta):** skryptowy przebieg samouczka liczy interakcje i otwarte panele —
   regresja powyżej 12/3 wywala build. Samego czasu zegarowego nie testujemy w CI; mierzymy go
   z dzienników playtestów.

---


## Zmiany wpisane po M9b

Zgodnie z `K-18`. Pełne uzasadnienia — tabela `DE-n` w `M9b-rdzen-ui.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| DF-1 ★ | **`LayoutNode`, `Layout` i dokowanie przenoszą się tutaj** z `M9b` §5.8, razem z pierwszym panelem, który ma się gdzie zadokować | Ekrany powłoki są pełnoekranowe, a karta inspekcji jest jednym oknem — układ doków bez paneli to plik konfiguracyjny bez czytelnika (`DE-4`) |
| DF-2 ★ | **`GraphView` i `GanttView` powstają tutaj**, razem z panelem Łańcuch dostaw i panelem Zakład; ich budżety z §7 dokumentu fazy (graf 500 węzłów ≤ 0,8 ms i ≤ 50 ms układu poza klatką, Gantt 500 pasków ≤ 0,4 ms) zamykają się z nimi, a nie w `M9b` | Zmierzenie ich w WP6 znaczyłoby zmierzenie widgetu rysującego dane zbudowane na potrzeby benchmarku (`DE-5`) |
| DF-3 | **Rdzeń UI stoi i jest zmierzony**: `Table<T>` 100 tys. wierszy (rysowanie 145 µs, sortowanie 1,97 ms poza klatką, filtr 117 µs), `Series` z piramidą mip (wykres 8 serii × 10 lat: 80 µs), `HeatmapThumb` (78 µs). Panele biznesowe składają się z tych trzech plus `Rich`, `TabStrip` i `Theme` — nie budują własnych | Budżety §7 są spełnione z zapasem, więc panel, który ich nie dotrzyma, będzie miał winnego po swojej stronie |
| DF-4 | **`MetricsRecorder` nie powstał** — `Series` z `engine/ui` jest gotowym pojemnikiem (`push_day`, cztery poziomy, bezstratne sumy), ale nikt jeszcze nie zapisuje do niego metryk gracza. To jest pakiet tej podfazy | §6 dokumentu fazy obiecuje `Series` + `MetricsRecorder` jako parę; połowa jest gotowa i przetestowana, druga potrzebuje wiedzieć, **które** metryki ma zapisywać, a to wie dopiero panel |
| DF-5 | **Ekran ustawień ma dwie zakładki** („Gra", „Sterowanie"); grafika i dźwięk wchodzą z M11. Ekrany domknięcia scenariusza i spuścizny (`ui-design.md` §6.6) należą do WP12 razem z `GameState::{Succession, ScenarioEnd}` | Zakładka pusta jest gorsza od jej braku, bo obiecuje treść (`DE-11`) |


## Zmiany wpisane po M9c

Zgodnie z `K-18`. Szczegóły — tabela `DG-n` w `M9c-gracz-inspekcja-nakladki.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| DF-6 ★ | **Karta inspekcji stoi i jest jedna dla wszystkich szesnastu rodzajów podmiotów** (`game::inspect::card`, wyczerpujący `match`). Panel biznesowy, który dokłada byt z kartą, dokłada **wariant `Subject` i ramię tam** — inaczej `game/` się nie kompiluje (`K-69`) | To jest ta sama bramka, którą `K-12` postawił dla powodów decyzji. Panele `M9e` nie budują drugiej karty: „pokaż w inspekcji" znaczy `nav.go(Subject)` |
| DF-7 ★ | **Karta zakładu zastąpiła osobne okno sklepu**, a dok prawy ma jedną kartę i historię (`InspectionNav`, 32 pozycje, `ViewCommand::NavigateBack`/`Forward`). Panele biznesowe idą do doku **lewego** i nie mieszają się z inspekcją | `ui-design.md` §5: dok prawy należy do tego, co gracz kliknął, lewy do tego, co prowadzi. Różnica jest stała, bo zamiana miejscami psuje nawyk (`DG-14`) |
| DF-8 ★ | **`CardSection` nie powstał** — zakładka jest `Rich`. `M9e` dokłada dwie rzeczy, których karta dziś nie ma i które wymagają czegoś więcej niż tekst: **historię** (`Series` per encja, zakładka `History`) i **akcje** (`PlayerCommandTemplate`, przycisk „z karty da się działać"). Obie wchodzą jako nowe warianty `CardTabKind` plus ramię w widgecie | Z pięciu sekcji z §5.7 trzy renderują się do `Rich` i już to robią; dwie pozostałe nie mają dziś czym być wypełnione (`DG-3`) |
| DF-9 | **`PlayerCharacter` istnieje i niesie `owned_sites: Vec<SiteId>`**, ale własność jest **listą**, a nie udziałem w firmie. `FoundFirm`, `OpenSite` i przeniesienie udziałów należą do tej podfazy razem ze ścieżką kariery; `CareerTier::derive` też | `ponytail:` sufit nazwany w kodzie. Lista wystarcza do jedynej rzeczy, do której była potrzebna w M9c: oznaczenia zakładów gracza jako śledzonych |
| DF-10 | **Nakładki danych i filtry encji stoją** (`game::overlays`, dziewięć pól z §14.2 plus legenda i dwa filtry). Panel Rynek i panel Sklep mają z czego brać mapę zasięgu i cen — wołają `overlays::build`, a nie liczą własnej | Raster po działkach jest jeden i mieszka w `sim/world` (`DG-9`) |
| DF-11 | **Tryb śledzenia dostaje gotowe przypięcie LOD** (`player::pin_micro`, `MAX_PINNED = 8`). `FollowTarget` z §5.10 dokłada tylko kamerę i oś czasu doby | Decyzja §9 pkt 2 dokumentu fazy wykonana w M9c (`DG-8`) |


## Zmiany wpisane po M9d

Zgodnie z `K-18`. Szczegóły i uzasadnienia — tabela `DF-n` w `M9d-jezyk-regul.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| DH-1 ★ | **Skrzynka eskalacji polityk jest gotowa i należy do `sim/economy`, nie do M9.** `PolicyAlert` (tick, zakład, polityka, numer reguły, numer komunikatu, waga, `ask`) trafia do pierścienia 64 wpisów dla zakładów **śledzonych**; czyta się ją `Market::policy_inbox()`, czyści `clear_policy_inbox()`. Odwzorowanie numeru komunikatu na tekst jest po stronie UI: klucze `ui.policy.msg.<n>` | Wiersz „Pulpit firmy" wypisywał eskalację polityki jako byt M9. Nie jest: akcje `Alert` i `AskPlayer` wykonują się w symulacji tysiące razy na dobę, także wtedy, gdy żadnego okna nie ma (`AX-2`). Panel ma ją **pokazać**, a nie wyprodukować. `ask` odróżnia „wiedz o tym" od „zdecyduj" — przy `ask` polityka zatrzymuje się na tym towarze do najbliższego wykonania, więc wpis w pulpicie jest jedyną drogą, którą gracz się o tym dowie |
| DH-2 ★ | **Definicja metryki onboardingu wymienia komendy, których nie ma.** Po M9d `PlayerCommand` ma sześć wariantów: `StartGame`, `SetPrice`, `SetCharacter`, `SetAutonomy`, `AttachPolicy`, `DetachPolicy`. `OpenSite`, `AcceptJobOffer` i `HireCandidate` wchodzą dopiero z tą podfazą | `X-2`: wariant wchodzi razem ze swoim wykonawcą i ze swoim panelem. Metryka §20.3 jest dziś mierzalna po `SetPrice` i `AttachPolicy`; zbiór rośnie razem z WP10 i WP12 |
| DH-3 ★ | **Kryterium WP10 „z każdego panelu da się wydać co najmniej jedną `PlayerCommand`" jest dziś niespełnialne dla pięciu paneli** (Finanse, Miasto, Ludzie, Kronika, Łańcuch dostaw), bo dla żadnego z nich nie istnieje komenda, a opis WP10 nie wymienia ich dokładania. Kryterium zostaje, ale **opis pakietu rośnie o komendy**: panel bez komendy jest panelem tylko do oglądania i wtedy kryterium ma go jawnie pomijać, a nie udawać, że go obejmuje | Przypadek (3) z `K-18`: kryterium spełnione tożsamościowo albo niemierzalne. Rozstrzygnięcie na starcie M9e: albo każdy z pięciu dostaje komendę i wykonawcę, albo kryterium mówi „z każdego panelu **operacyjnego**" i wylicza, które to są. Drugiego nie da się wybrać po cichu — trzeba wypisać listę |
| DH-4 | **Edytor reguł istnieje i ma ekran** (`game::policy::RuleEditorView`, cztery zakładki: reguły, uwagi, próba na ostatnich dobach, zapis tekstowy). Klient otwiera go klawiszem `R` na zaznaczonym zakładzie. **Czego nie ma:** dokładania reguł i akcji z klawiatury — ekran pokazuje politykę, diagnozuje ją i pozwala przypiąć | Sufit nazwany w kodzie: wybór zakładu i towaru należy do panelu firmy, czyli do WP10. Dopisanie tego w M9d znaczyłoby budowanie panelu przed panelem |
| DH-5 | **Dry-run liczy się ze śladu doby** (`Market::dry_run` po `PolicyTrace` — 30 dób faktów zakładu śledzonego), a nie z `Series`/`MetricsRecorder`. `Series` istnieje od `M9b` i jest wykresem; `MetricsRecorder` z §6 dokumentu fazy **nie istnieje** i jest zadaniem tej podfazy | §5.6 pkt 8 mówił „na tych samych seriach, które zasilają wykresy". Ślad faktów jest bliższy prawdzie, bo niesie **dokładnie to**, co widzi reguła. Kiedy `MetricsRecorder` powstanie, dry-run go **nie potrzebuje** — potrzebuje go nakładka „co by ustawiła" na wykresie ceny |
| DH-7 | **`ScopeConflict` czeka na przypinanie polityki do grupy i do firmy.** Dziś polityka przypina się wyłącznie do zakładu, a zakład ma najwyżej jedną (`SiteDelegation`) — dwa zakresy nie mają gdzie się spotkać. Diagnoza wraca razem z `PolicyScope::Group`/`Firm`, czyli razem z panelem firmy, który pozwoli wybrać grupę | Wariant, którego nie da się wywołać, przechodzi każdy test i wygląda tak samo jak działający (`K-67`). Wpisane tutaj, bo to WP10 stawia panel, w którym grupa zakładów w ogóle powstaje |
| DH-6 | **`PolicyRunner` jako osobny system ECS nie powstał i rozłożenia `(i*37) % 1440` nie ma.** Budżet z §7 („≤ 1 ms na dobę, 200 zakładów") jest spełniony bez rozproszenia | Zmierzone: 200 zakładów z polityką dyskontową liczy się poniżej milisekundy w wydaniu optymalizowanym. Wpisane tutaj, żeby panel wydajności z WP10 nie szukał systemu, którego nie ma |


## Zmiany wpisane w M9e

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. To są rzeczy, które
wyszły przy pisaniu kodu obok §5.4, §5.9, §5.11 i §5.12 — każda zmienia coś,
co dokument obiecywał inaczej, niż dało się dotrzymać.

| # | Zmiana | Dlaczego |
|---|---|---|
| DI-1 ★ | **`StopCondition` jest gotowym zestawem, a nie `ConditionExpr`.** Dziewięć wariantów z listy §5.10; **trzy z niej nie powstają**: strajk (M10, `K-9`), nieudana dostawa (`sim/supply` nie prowadzi dziennika niedowiezionych zleceń) i wojna cenowa **na wskazanym towarze** — warunek pyta o moich konkurentów, a nie o cały rynek | Ten sam powód, dla którego wyrażeniem przestał być filtr encji (`DG-4`): metryki języka reguł opisują **jeden zakład** i nie mają kwantyfikatora, a każdy warunek z listy pyta o „którykolwiek mój". Drzewo składniowe byłoby pustą ramą — a §5.10 sam mówi „bez budowania wyrażenia" |
| DI-2 ★ | **`TimeScale` to nazwa na `SimSpeed`, nie drugi enum, a `X50` tu nie powstaje.** §5.10 rysował własne pięć wariantów | Zegar mieszka w `engine/core` od `K-22` i to on jest właścicielem prędkości; dyskryminanty wchodzą do zapisu. `K-22` mówi wprost, że `X50` dopisuje **M12 na końcu enuma**, razem z trybem 50× po stronie symulacji |
| DI-3 ★ | **Z siedmiu wariantów `Goal` powstaje pięć.** `ProductLaunched` odpada z M10 (marka i receptury), `Custom(ConditionExpr)` z powodu `DI-1`. `MarketShare` bierze **klucz tekstowy** towaru, nie `GoodId`, a `AreaSpec` zwęża się do „całe miasto" | Cel, którego nie da się osiągnąć, wygląda w panelu tak samo jak osiągalny (`K-67`). Klucz zamiast indeksu, bo `GoodId` przesuwa się przy każdym nowym towarze, a scenariusz jest **daną** (00 §5) |
| DI-4 ★ | **Kronika czyta dzienniki, zamiast zbierać zgłoszenia.** `chronicle::record()` z §6 dokumentu fazy **nie powstaje**; `Chronicle::harvest` przepisuje raz na dobę `Events::chronicle()`, pierścienie decyzji **firm gracza** i dziennik wejść. Rekord przestaje mieścić się w 48 B i nie musi | `game/` stoi **nad** wszystkimi `sim/*`, więc żaden system symulacji nie może do niego sięgnąć — zależność szłaby w drugą stronę i Cargo by tego nie zbudował. Budżet 48 B bronił przed 5–7 mln wpisów; kronika zbierana i zawężona do gracza ma ich rzędu setek tysięcy przez sto lat, więc broni przed zagrożeniem, którego nie ma. `ponytail:` sufit nazwany w kodzie: decyzji firm AI nie zbieramy, a kronika firmy otworzy się z jej karty i przeczyta jej pierścień wprost |
| DI-5 ★ | **`LayoutNode` nie powstaje.** Dok jest `egui::Panel::left` o stałej szerokości, a `Layout` to lista przypiętych paneli i jeden aktywny — preferencja widoku w profilu gracza, nie drzewo | To jest ta sama decyzja, którą `M9b` podjął dla drzewa retained (`DE-4`): własny system układu nad `egui` byłby drugim toolkitem. `DF-1` przeniósł go tutaj „razem z pierwszym panelem, który ma się gdzie zadokować" — pierwszy panel powstał i okazało się, że dokuje go `egui` |
| DI-6 ★ | **`PanelDesc` bierze `&Session`, nie `&Snapshot`.** Podwójnie buforowanej migawki nadal nie ma; granicą jest typ: panel dostaje `&Session` i `&mut` nie zobaczy | §5.9 pisał `build: fn(&Snapshot, &PanelState)`, ale `Snapshot` nie powstał ani w `M9b`, ani tutaj. Zasada z §5.2 („kod paneli nie widzi `&World`") jest **dotrzymana inaczej**: panel składa komendę i oddaje ją, a wykonuje ją sesja w punkcie synchronizacji. `ponytail:` sufit: dopóki UI i symulacja chodzą w jednym wątku, migawka kupuje wyłącznie spójność w połowie ticku — a tej pilnuje kolejność klatki z `M9a` §5.2 |
| DI-7 ★ | **`AssignManager` nie powstaje; zamiast niej `SetDelegationAutonomy`** | Menedżerem zostaje najlepszy człowiek na stanowisku kierowniczym i wybiera go `hr::reconcile_managers` **codziennie**. Komenda „postaw tego" byłaby nadpisana następnej doby, czyli byłaby wariantem bez skutku (`K-67`). Gracz stawia menedżera, **zatrudniając go** (`HireCandidate`), a steruje nim przez autonomię i politykę — i to jest ta decyzja, której `AttachPolicy` świadomie nie podejmowało |
| DI-8 ★ | **Rozstrzygnięcie `DH-3`: kryterium WP10 obejmuje panele *operacyjne*, a kronika jest jawnie pominięta.** Lista jest kodem (`PanelId::is_operational`), nie komentarzem. Pozostałe osiem paneli dostało komendy i wykonawców: `FoundFirm`, `OpenSite`, `CloseSite`, `SetShelfAssortment`, `SetRestockTarget`, `HireCandidate`, `SetDelegationAutonomy`, `ApplyForJob`, `TakeLoan`, `BackCandidate`, `ApplyForPermit` | Dziennik nie ma czego zmienić w świecie: przycisk „zrób coś" w kronice byłby przyciskiem bez skutku. Kryterium udające, że ją obejmuje, zostałoby spełnione tożsamościowo przez pierwszy taki przycisk — a `DH-3` żądał, żeby listy nie dało się wybrać po cichu |
| DI-9 ★ | **Panel Miasto dostał `ApplyForPermit`, bo samo `BackCandidate` nie wystarczyło.** Wybory są raz na kadencję, a urząd stoi zawsze | Bez tego panel byłby „operacyjny" przez trzy tygodnie na cztery lata, a test kryterium WP10 przechodziłby albo nie zależnie od tego, czy akurat trwa kampania. `Applicant::Player` czekał w `sim/city` od M8d na kogoś, kto go wyda |
| DI-10 ★ | **Spadkobierca dziedziczy firmę, a nie wskaźnik na zakład.** `take_role` przepisuje własność firmy zakładu na `Owner::Player` | `StartVariant::Heir` dawał `owned_sites` z pierwszego sklepu miasta, ale w rejestrze firm zakład należał do kogoś innego — więc `precheck` odmawiał graczowi **każdej** komendy dotyczącej jego własnego sklepu („to nie jest twój zakład"). Wyszło testem WP10; dwie prawdy o własności to ta sama klasa błędu co dwa identyfikatory zakładu (`K-46`) |
| DI-11 ★ | **`WorldPatch::ClearShopKind` zamyka sklepy *jednego rodzaju* w dzielnicy wskazanej porządkowo.** Pierwsza wersja („zamknij dzielnicę numer N") wygasiła w mieście testowym 83 sklepy z 83 | Nisza to **brakujący rodzaj sklepu**, a nie martwa dzielnica — i gracz musi mieć gdzie otworzyć własny, bo firma wchodzi do istniejącego lokalu. Numer dzielnicy zależy od ziarna, a scenariusz kładzie się na dowolnym świecie, więc adresowanie jest porządkowe. Rodzaj najrzadszy: dziura prawdziwa, miasto zostaje miastem |
| DI-12 | **`CardTabKind::History` powstaje, `Actions` nie.** Historię wypełnia karta partii (ślad „od pola do półki" z `trace_batch`) | `DF-8` zapowiadał obie. `Actions` potrzebuje `PlayerCommandTemplate`, czyli szablonu komendy oderwanego od panelu — a każda komenda ma dziś swój panel i swój kontekst (który zakład, który towar). Zakładka, której nie ma czym wypełnić, jest gorsza od jej braku, a `InspectionCard::tab` i tak by jej nie wpuścił |
| DI-13 | **`StopWatch` startuje od godziny, której nie ma** (`u64::MAX`, nie zero) | Zero jest **pierwszą godziną gry**, więc pierwsze sprawdzenie wypadało z niej jako „ta sama co ostatnio" i warunek trafiający w pierwszej dobie milczał do drugiej. Ta sama klasa błędu co licznik inicjowany zerem przy liczniku monotonicznym |
| DI-14 | **Panel Ludzie proponuje wyłącznie kandydatów, których da się zatrudnić** — z listy szukających pracy odpadają ci, którzy już gdzieś pracują | Szukający pracy **na etacie** jest poprawnym stanem rynku (`job_seekers` bierze dobową część zatrudnionych) i złym wierszem panelu: przycisk przy nim zawsze odmawiał |
| DI-15 | **Nakład na nowy punkt wolno mieć zerowy.** `OpenSite` odrzuca wyłącznie kwotę ujemną | `firmlife::expand` mówi wprost: firma, której nie stać na kapitał obrotowy, otwiera punkt bez niego i zobaczy to w pierwszym miesiącu. Walidacja „dodatnia" zabierała graczowi decyzję, którą AI ma |
| DI-16 | **Panele dostają własną przestrzeń kluczy `ui.panel.<panel>.*`** dla sklepu i łańcucha dostaw | `ui.shop.*` należy od M5e do **karty** zakładu, a `ui.supply.*` od M6e do karty łańcucha. Panel, który sięgnął po te same klucze, dostał cudzy tekst — i wyszło to dopiero jako niepodstawiony parametr na ekranie |
| DI-17 | **`MetricsRecorder` mieszka w `Session` i pisze z kroku doby**, nie w kliencie | Wykres w oknie ma pokazywać tę samą historię, którą mierzy przebieg bezgłowy. Metryki są stroną widoku: nie dotykają świata i **nie wchodzą do hasha** (00 §3.6) — test `uklad_paneli_nie_zmienia_hasha_stanu` pilnuje tego wprost |
| DI-18 | **Metryka onboardingu czyta dziennik replayu i ma w nim nośnik: `ViewCommand::OpenPanel`** | §5.12 obiecywał pomiar „bez osobnej telemetrii", ale otwarcia paneli nikt nie zapisywał — przyrząd byłby bez czujnika. `game::onboarding::measure` liczy interakcje i **różne** otwarte panele z tego samego dziennika, z którego odtwarza się sesję |
| DI-19 | **Reputacja gracza nie powstaje jako pole.** Po upadłości zakłady idą do likwidacji, a **zobowiązania zostają** — i to je widzi bank przy następnym wniosku | §5.11 pisał „`Reputation` obrywa (banki M5/M7 muszą to widzieć — kontrakt)". Takiego typu nie ma, a dodanie go bez czytelnika byłoby polem bez skutku. Ocena zdolności stoi na `arrears_months` kredytu, który po upadłości nie znika — mechanizm jest, tylko nazywa się inaczej |
| DI-20 | **Podatku spadkowego nie ma i sukcesja go nie nalicza.** Decyzja otwarta nr 8 dokumentu fazy **zostaje otwarta** | Kodeks podatkowy M8 zna siedem danin (`K-55`) i żadna z nich nie jest spadkowa. Naliczenie stawki, której nie ma w `data/city/`, byłoby wymyśleniem prawa po stronie gracza |


## Rzut z recenzji przed commitem M9e

Dwa niezależne przeglądy całego kodu podfazy, zgodnie z regułą „subagenci
oszczędzają kontekst" z `CLAUDE.md`. Dziewiętnaście usterek; **wszystkie naprawione
w tym samym commicie** poza jedną, która zmienia zachowanie firm AI i została
wpisana do `R2`. Gwiazdka = usterka, która czyniła obietnicę podfazy nieprawdziwą.

| # | Co było nie tak | Jak naprawione |
|---|---|---|
| DI-21 ★ | **Siedem z dziewięciu paneli było w grze nieosiągalnych.** `Layout::for_tier`, `pin` i `unpin` nie miały ani jednego wołającego poza testem; klient brał `Layout::default()` (Sklep + Pulpit) i nigdy go nie zmieniał. `PanelDesc::min_tier` — opisany jako **jedyne** miejsce, na które wpływa etap kariery — nie miał czytelnika | `Panels::sync` dopina panele, których próg gracz właśnie przekroczył (i nigdy nic nie odpina, bo to, co gracz przypiął sam, jest jego decyzją), a dok dostał wiersz „Więcej paneli" z resztą rejestru. `min_tier` dalej **nie blokuje**: każdy panel da się stąd otworzyć od pierwszej minuty |
| DI-22 ★ | **Komenda „złóż wniosek o pozwolenie" wywalała grę.** `ui.chronicle.act.apply_for_permit` nie było w katalogu, a `Catalog::fmt_key` panikuje na nieznanym kluczu — otwarcie panelu Kronika po tej komendzie kończyło sesję. Wystarczyła komenda **odrzucona**, bo dziennik zapisuje ją przed wykonaniem | Klucz dopisany. Druga panika tej samej klasy — `ui.policy.msg.<n>` dla komunikatu spoza katalogu — zamknięta osobnym wejściem `PanelCtx::fmt_or`, bo numer komunikatu pochodzi z **reguły gracza** i walidator go nie ogranicza |
| DI-23 ★ | **Trzy z siedmiu serii metryk zapisywały same zera.** `MetricsRecorder::add_revenue` i `add_lost_sales` nie były wołane znikąd — utarg, wynik i utracone wizyty były polami bez pisarza | Zbierane raz na dobę z rachunku zakładu i z histogramu utraconych wizyt — tą samą drogą co kronika (`DI-4`), bo `sim/*` nie ma jak sięgnąć do `game/` |
| DI-24 ★ | **Powód wygaszonego przycisku szedł do gracza jako `Display` `CommandError`**, czyli polski literał dla dewelopera z `SiteId(Entity { … })` w środku. W wersji angielskiej gry gracz dostawał polskie zdanie z identyfikatorem encji | `CommandError::text(&Catalog, Locale)` z wyczerpującym `match` i dwudziestoma ośmioma kluczami w obu językach. `Display` zostaje tam, gdzie był: w dzienniku i pod debuggerem |
| DI-25 | **Cel zapasu ustawiony przez gracza wyłączał zamawianie towaru.** Zapas dobowy liczy się jako `obrót/7` z podłogą **jednej milisztuki**, a towar dopiero co położony na półce ma obrót zero — więc „zapas na 7 dni" wychodziło siedem tysięcznych sztuki. Sklep przestawał ten towar zamawiać, a sterownik ceny widział magazyn przepełniony i schodził do podłogi marży | Towar **bez historii** dostaje cel domyślny `open_shop` (`restock_without_history`), a wzór dla towaru sprzedającego się jest jeden dla gracza i dla AI (`restock_from_days`). **Ta sama degeneracja zostaje w ścieżce AI** i jest w `R2`: jej naprawa zmienia zachowanie firm, więc należy do przebiegu z balansatorem, a nie do podfazy interfejsu — test `ale_widzi_cene_polkowa_gracza` pokazał to od razu |
| DI-26 | **Kredyt: trzy usterki naraz.** Zamknięty sklep dostawał kredyt, którego nigdy nie spłacał (dobowa pętla pomija zamknięte przy naliczaniu raty), nieudana kreacja depozytu zostawiała osierocony wpis w rejestrze kredytów, a odmowa „masz już kredyt" jechała do gracza jako „zaległości" | Strażnik `closed`, kolejność „najpierw pieniądz, potem wpis" i własny powód odmowy. Ścieżka publiczna z panelu ma inne wymagania niż cicha pętla dobowa — i to jest cała treść tej poprawki |
| DI-27 | **Zatrudniony ręcznie zostawał na liście bezrobotnych.** `owner_ops::hire` nie zdejmował człowieka z `seekers`, w odróżnieniu od doboru — więc panel Ludzie proponował go dalej jako kandydata, a statystyka rynku liczyła go dwa razy | `LaborMarket::drop_seeker` wołane po zatrudnieniu; przy okazji drugie wyszukanie stanowiska odzyskało warunek wolnego etatu, bez którego obsada mogła przekroczyć liczbę miejsc |
| DI-28 | **Gracz-właściciel mógł dostać drugą firmę od dobowego przeglądu niszy.** `wlasciciele()` dopasowywało wyłącznie `Owner::Citizen`, a firma gracza ma `Owner::Player` — więc mieszkaniec-gracz nadal wyglądał na kogoś bez firmy i przegląd zakładał mu kolejną za jego oszczędności | Ramię `Owner::Player` rozwiązuje się przez znacznik `Identity::FLAG_PLAYER`, czyli przez jedyne miejsce, po którym `sim/*` poznaje postać gracza |
| DI-29 | **Warunki zatrzymania pauzowały grę co godzinę bez końca.** „Saldo poniżej progu" i „pusta półka" są poziomowe: gracz wznawiał, mijała godzina, warunek dalej trzymał, pauza wracała | Zatrzask: warunek zgłasza się **raz**, a odblokowuje się sam, kiedy przestaje trafiać. Liczniki przyrostowe (eskalacje, zdarzenia miasta) idą teraz także przy warunku wyłączonym, żeby włączenie go nie zgłaszało zaległego przyrostu |
| DI-30 | **Marża „−0,50 %" wyświetlała się jako „0,50 %"** — znak brał się z części całkowitej, a ta dla wartości między −1 % a 0 jest zerem. Kolor obok liczył się poprawnie, więc tekst i kolor mówiły dwie różne rzeczy | Jedno wspólne `widgets::percent_bp`, znak z całości. Przy okazji marża pulpitu przestała jechać przez `as i32`, które przy sklepie o utargu rzędu złotówek zawijało stratę na plus |
| DI-31 | Drobne, w jednym worku: `ComboBox` z jednym identyfikatorem dla wszystkich list (dwie listy w panelu łańcucha rozwijały się razem); klucz towaru brany z **etykiety** zamiast z katalogu; `Debug` enuma roli magazynu na ekranie; przyrost kosztu w karcie partii liczony od zera zamiast od etapu sprzed okna; wpłata na kampanię wyprowadzająca pieniądz **przed** sprawdzeniem, czy kampania istnieje; `set_assortment` zwracające `Ok` mimo niewykonania; `PanelView::table` i `filter` bez czytelnika; `day_to_minute` i `firm_of_ordinal` bez wołającego; graf z cyklem rozkładający dwa węzły na pięć warstw | Wszystkie naprawione. Trzy martwe funkcje wyszły z kodu, `Chronicle::count_event` dostał czytelnika (osiągnięcia emergentne w panelu Kronika), a `PanelView::days` — pisarza (zakres wykresu na pulpicie) |
| DI-32 | **Kreator świata oferował jeden scenariusz.** Sześć scenariuszy z `data/scenarios/scenarios.ron` było nieosiągalnych z gry — wiersz „Scenariusz" miał na sztywno `[ScenarioId::SANDBOX]` | `Shell` czyta katalog przy starcie, a kreator pokazuje **opis wybranego** pod wierszem. Znalezione poza recenzją, przy sprawdzaniu, czy WP12 da się w ogóle zagrać |


## Czego WP12 nie domykało — i czym zostało domknięte

Wpisane po recenzji odhaczenia, **wykonane przy domknięciu `WP12`**. Kryterium pakietu
mówi „bankructwo **i śmierć** nie kończą sesji”, a śmierć w grającej sesji nie była
w ogóle wykrywana.

Reguła z `CLAUDE.md` brzmi „odhaczaj tylko to, co zweryfikowane”, a testy WP12
sprawdzały **komendy**, nie drogę, którą gracz do nich dochodzi. To jest ta sama
klasa błędu, którą recenzja znalazła przy panelach (`DI-21`): kod jest, wejścia nie ma.
Dlatego czwarta kolumna mówi o **drodze wejścia**, a nie o tym, że kod powstał.

| # | Czego brakowało | Co już było | Czym domknięte |
|---|---|---|---|
| DI-33 ★ | **Śmierć postaci nie przełączała gry w sukcesję.** `legacy::check` nie miał ani jednego wołającego, a `GameState::{Succession, ScenarioEnd}` nikogo, kto je konstruuje — dwa warianty stanu, w które nie dało się wejść (`K-67`) | `legacy::check` rozpoznaje zgon i niewypłacalność, `legacy::heir_of` wskazuje dziedzica, komendy `Succeed` i `ContinueAsNewCitizen` działają i mają test | **`Session::doba_gracza` woła `legacy::check` raz na dobę** i odkłada wynik; stan przełącza **`GameState::settle`** — jedna funkcja dla okna (`App::klatka`) i dla przebiegu bezgłowego (`headless new-game`). Sesja nie zmienia stanu gry sama, bo stan gry stoi **nad** nią. Niewypłacalność stanu **nie** przełącza: wydaje `DeclarePersonalBankruptcy` i gra idzie dalej (§13.4) — ale tylko wtedy, gdy jest co likwidować, bo po upadłości oba warunki `legacy::check` zostają prawdziwe i gracz z długiem ogłaszałby ją co dobę. Test: `smierc_postaci_przelacza_gre_w_sukcesje` |
| DI-34 ★ | **Ekranów domknięcia scenariusza i spuścizny nie było.** `DF-5` przypisywał je do WP12 razem z wariantami stanu; `ui-design.md` §6.6 opisuje, co mają rozliczać | `ScenarioOutcome` liczy się co dobę i ma test; `Chronicle` ma z czego złożyć kronikę dynastii | **`game::screens::ending`**: koniec scenariusza rozlicza **każdy** cel (osiągnięty / przepadł / o ile chybiony), spuścizna pokazuje kronikę dynastii i proponuje dziedzica albo nową dynastię. Żaden nie jest ślepym zaułkiem. Wynik scenariusza jest **zapamiętany raz na dobę** (`settled_outcome`), a nie liczony w każdej klatce — rozliczenie celów chodzi po zakładach gracza i po rynku. Ekran pokazuje się **raz**: wynik zostaje `Won` do końca gry (cel raz osiągnięty zostaje osiągnięty), więc bez pamięci o obejrzanym ekranie „graj dalej” wracałoby na niego przy najbliższej dobie. Test: `siec_piecdziesieciu_sklepow_domyka_scenariusz` jedzie teraz przez `settle` i `apply_end` |
| DI-35 ★ | **Samouczka nie było.** `Scenario::tutorial` był flagą bez czytelnika; trzech kroków z §5.12 pkt 3, pomijalności i limitu czterdziestu słów nie zaimplementowano | Scenariusz „Pierwszy sklep” stawia niszę łatką i ma cele; `onboarding::measure` liczy budżet z dziennika i test go pilnuje | **`game::tutorial`**: trzy kroki, każdy kończy się **faktem ze świata** (śledzę siebie / nakładka zasięgu włączona / mam sklep i ustawiłem cenę), a nie upływem czasu. Pasek u góry ekranu, **nie okno modalne**: świat pod nim tyka. Dwa przyciski — pomiń krok i pomiń samouczek. Limit czterdziestu słów jest testem w obu językach. **`TutorialScriptId` nie powstaje**: skrypt jest jeden, a identyfikator wskazujący zawsze tę samą pozycję byłby numerem bez zbioru — `Scenario::tutorial` zostaje `bool` i ma wreszcie czytelnika |
| DI-36 | **Panelu celów nie było.** §5.11 obiecuje postęp `0..=10000 bp` pokazywany w panelu celów, a `streak_progress_bp` nie miał czytelnika | `Goal::progress_bp`, `ScenarioState` i `fresh_objectives` liczą wszystko, czego panel potrzebuje | **Sekcja „Cele scenariusza” w pulpicie**, nie dziesiąty panel: gracz patrzy tam po to samo — jak mi idzie — a panel odwiedzany raz na dobę jest listą, nie narzędziem. Cele ukryte za `reveal_after` nie pokazują się przed czasem. Znak przed nazwą, bo kolor nigdy nie jest jedynym nośnikiem |
| DI-37 | **Gracz nie włączał i nie wyłączał warunków zatrzymania.** `StopWatch::set` i `ViewCommand::SetStopCondition` nie miały wołającego — zestaw był uzbrojony na sztywno przy wejściu do świata | Warunki działają, zatrzask działa, `armed()` zwraca listę ze stanem | Lista w doku lewym, zwinięta domyślnie. Przełączenie idzie do **dziennika wejść** przez `Panels::push_view` — jedna kolejka zdarzeń widoku, nie druga, bo z niej liczy się metryka onboardingu i odtwarza się zgłoszenie błędu |
| DI-38 | **Panel sklepu nie pokazywał bieżącego celu zapasu.** `Market::restock_days` nie miał czytelnika, więc gracz naciskał „7 dni”, nie wiedząc, co jest ustawione teraz | Odczyt istnieje i jest przycięty do `u16` | Wiersz „teraz: N dni zapasu” nad przyciskami; brak celu mówi „cel automatyczny”, a nie pokazuje zera |


## Zmiany wpisane po M9e

Zgodnie z `K-18`. Wyszło z gry uruchomionej po domknięciu WP12 — z jedynego miejsca,
w którym widać ekran rozgrywki złożony w całość, a nie każdy element osobno w teście.

| # | Zmiana | Dlaczego |
|---|---|---|
| DI-39 | **Pasek czasu jest panelem, nie pływającą warstwą.** `egui::Panel::top("magnat.czas")` rysowany przed dokiem; warstwy kotwiczone (pasek samouczka, pas alertów) dostają `constrain_to` z wolnym obszarem policzonym po doku, a karta inspekcji startuje obok doku, nie na nim | Pasek stał w lewym górnym rogu jako `Area` z pozycją `(12, 12)` wpisaną na sztywno, a dok lewy to `Panel::left` zaczynający się od `y = 0`. Warstwa pływająca **nie rezerwuje** miejsca, więc kolizja z pierwszą pozycją doku była pewna, a nie przypadkowa — i tak samo zaczynała karta inspekcji (okno 520 × 780 na pozycji `(12, 70)`, czyli w całości na doku). Kotwica `Align2` liczy się względem `Context::content_rect`, czyli ekranu minus wcięcia systemowe — panele jej nie obchodzą, stąd `constrain_to`. `ui-design.md` §5 rysował ten układ od początku; kod go nie realizował. Test `pasek_czasu_rezerwuje_pas_u_gory` w `tools/magnat` pyta o wolny prostokąt po narysowaniu paska i doku — przed naprawą pokazywał `y = 0` |

