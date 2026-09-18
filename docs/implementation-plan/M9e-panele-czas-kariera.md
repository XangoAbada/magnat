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
| **WP10** | Panele biznesowe | WP5–WP9 | 9 paneli z §14.3: pulpit, zakład (Gantt), sklep, łańcuch dostaw (graf), rynek, ludzie, finanse, miasto, kronika + `PanelRegistry` | Każdy panel spełnia budżet klatki; z każdego panelu da się wydać co najmniej jedną `PlayerCommand` |
| **WP11** | Czas, śledzenie, kronika | WP5, WP10 | `TimeScale`, `StopCondition` z gotowym zestawem, tryb „śledź" (mieszkaniec/pojazd/partia), oś czasu dnia plan-vs-realizacja, magazyn kroniki z indeksem i wyszukiwaniem | Śledzenie partii od pola do półki z czasem i kosztem etapów; „zatrzymaj gdy brak towaru" działa przy 10× |
| **WP12** | Kariera, scenariusze, porażka, onboarding | WP4, WP10, WP11 | `CareerTier`, `Scenario`/`Objective`, 5 scenariuszy z §13.3, bankructwo osobiste, sukcesja, samouczek i pomiar metryk §20.3 | Scenariusz „Zbuduj sieć 50 sklepów" przechodzi do końca; bankructwo i śmierć nie kończą sesji; scenariusz samouczka mieści się w budżecie 12 interakcji |

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
