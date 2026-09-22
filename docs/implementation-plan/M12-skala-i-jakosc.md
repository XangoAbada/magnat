# M12 — Skala i jakość

Status: plan wykonawczy fazy domykającej.
Nadrzędny: `00-konwencje-i-kontrakty.md` (typy `core`, determinizm, LOD, dane RON — nie redefiniujemy tu niczego stamtąd).
Źródło wymagań: `PRD_Magnat.md` §18 (całość), §17.4, §17.7, §17.8, §16.1, §16.4, §16.5, §14.5, §19 (M12), §20.2, §20.4.

---

## 1. Cel fazy i artefakt końcowy

Po M12 gra z M0–M11 przestaje być demem na 100 tys. mieszkańców i staje się produktem, który wytrzymuje metropolię, długą sesję, cudze mody i cudzy komputer.

**Artefakt końcowy — pięć rzeczy, które da się uruchomić i zobaczyć:**

1. **Metropolia 400 tys.** ładuje się i chodzi w ≤ 6 GB RAM. W grze: panel `F3 → Pamięć` pokazuje rozbicie per podsystem z porównaniem do budżetu; przekroczenie dowolnej pozycji świeci na czerwono.
2. **Suwak prędkości 50×** działa: 1 doba gry ≤ 3 s dla miasta 150 tys., ruch schodzi do **makro** (mezo nie wystarcza — §5.6), grafika upraszcza, a pieniądz i masa po 30 dniach zgadzają się **co do grosza i grama** z przebiegiem 1×, przy odchyleniu agregatów dzielnicowych ≤ 0,5% miesięcznie i bez dryfu. Obietnica jest skalowana świadomie: 50× przewiduje stan dzielnicy i miasta, **nie los pojedynczej firmy** (D14).
3. **Zapis w tle bez pauzy:** `Ctrl+S` w trakcie gry — symulacja nie zatrzymuje się ani na jeden tick, pasek postępu dobiega do końca w < 5 s, plik ma 50–300 MB. Wczytanie zapisu zrobionego dwie wersje schematu temu działa (migracja w locie z komunikatem).
4. **Replay:** `tools/headless --replay bug_4711.mgr` odtwarza cudze zgłoszenie błędu klatka w klatkę na maszynie dewelopera. CI ma bramkę „determinizm 100%".
5. **Mod z warsztatu:** menedżer modów w grze, instalacja przykładowego moda (`example-mod/` — nowy towar, nowy typ zakładu, nowa polityka AI firmy, jeden panel UI, jedno zdarzenie), uruchomienie kariery z tym modem i **przejście walidatora determinizmu**. Cały interfejs w PL i EN z poprawną pluralizacją i odmianą.

Wszystko to jest mierzone, nie deklarowane: każdy z pięciu punktów ma odpowiadający test w CI (§7).

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Co konkretnie |
|---|---|
| Pamięć i skala | Rachunek budżetu jako **test CI**, redukcja rozmiaru komponentów (gorące/zimne), areny i pule, magazyn doświadczeń, wyniesienie kronik na dysk z indeksem, drabina cięć przy przekroczeniu |
| Tryb 50× | **Budżet 2,08 ms/tick rozpisany na systemy** (§5.6), przełączanie ruchu i agentów w makro, degradacja grafiki, `SpeedGovernor` (watchdog budżetu ticka), dowód spójności LOD |
| Zapis | `engine/io` w pełnej postaci: `SaveFile` z sekcjami, zstd, `schema_version`, `Migration`, `EventJournal`, autozapisy, **zapis w tle przez copy-on-write chunków ECS** |
| Determinizm | `ReplayFile`, bramka CI, pipeline zgłoszeń błędów, fundament pod lockstep (bez implementacji sieci) |
| Modding | `engine/script`: host Lua (mlua), sandbox, `ScriptHook`, `ModManifest`, menedżer modów, walidator determinizmu moda |
| Lokalizacja | `LocaleCatalog`, pluralizacja CLDR, przypadki gramatyczne PL, rodzaj gramatyczny w kronikach, testy pokrycia kluczy |
| Oprawa UI | Własny krój, ikony jako dane, pasek ikon paneli, menu główne na żywej scenie — **M12f**, dopisane 2026-09-22, bo ikony i krój nie miały wykonawcy (`docs/ui-design.md` §8) |
| Jakość | Sesja 100 lat headless, detekcja wycieków i dryfu ekonomicznego, rozbudowa balansatora o analizę trendu, benchmarki `criterion` w CI |

### Nie wchodzi

| Czego nie robimy | Kto to robi |
|---|---|
| Mechaniki symulacji (potrzeby, rynki, produkcja, HR, podatki) | M3–M10 — M12 je **konsumuje i optymalizuje**, nie zmienia semantyki |
| Renderer, greedy meshing, LOD wizualne, impostory, animacje | M1 / M11 — M12 tylko **wywołuje** przełącznik degradacji w trybie 50× i narzuca budżet pamięci na chunki |
| Panele UI, widgety, wykresy, edytor reguł | M9 / M11 — M12 dodaje panel pamięci (devtools), menedżer modów, warstwę i18n **pod** istniejącymi widgetami oraz oprawę (M12f: krój, ikony, nagłówki, tło menu) — **treść** paneli się nie zmienia |
| Minimalny snapshot ECS + hash stanu | M0 — M12 **rozszerza** ten sam kod, nie pisze drugiej ścieżki serializacji |
| Model makro, w tym **makro LOD ruchu** (agregaty per dzielnica × klasa, „historia na sucho") | M10 (`sim/macro`) — M12 **podłącza** go pod prędkość 50×, rozpisuje mu budżet czasu i dowodzi spójności z mezo. Własność makro ruchu uzgadniana z M10 — D11 |
| Multiplayer (sieć, lockstep, NAT, rozjazd) | Poza projektem. M12 dostarcza wyłącznie fundament: determinizm + dziennik wejść |
| CH i grafy nawigacji | M4 — M12 narzuca budżet pamięci i klasyfikuje je jako sekcje `Derived` (nie zapisywane) |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M12 |
|---|---|
| §18.1 Zapis | `SaveFile`, zstd, `EventJournal`, autozapisy, wersjonowanie + migracje, zapis w tle CoW — WP4–WP7 |
| §18.2 Determinizm | `ReplayFile`, bramka CI, odtwarzanie zgłoszeń, fundament lockstep — WP8 |
| §18.3 Modding | `engine/script`, sandbox, `ModManifest`, menedżer modów — WP11–WP12 |
| §17.7 Pamięć | Budżet 6 GB / 400 tys., 400 B gorącego stanu mieszkańca, magazyn doświadczeń (32 wpisy), kroniki na dysk z indeksem — WP1–WP3, WP10 |
| §17.4 LOD | Makro ruchu i agentów przy 50×; gwarancja spójności po rozdzieleniu tolerancji (mikro↔mezo 0; makro: agregaty ≤ 0,5%/mies. bez dryfu, pieniądz i masa dokładnie) — WP9 |
| §17.8 Testy | Testy własnościowe, determinizm co 1000 ticków, regresja balansu, `criterion` — WP14 |
| §16.1 | Granica bibliotek: `zstd`, `mlua`, `tracy-client`/`puffin`, `serde` — bez nowych zależności poza listą |
| §16.4 | Lokalizacja PL/EN z pluralizacją — WP13 |
| §16.5 | Headless runner, replay, balansator — M12 rozbudowuje o tryb 100-letni i analizę trendu |
| §14.5 | Prędkości 1×/3×/10×/50×, „zatrzymaj przy zdarzeniu X" jako punkt zaczepienia autozapisu |
| §19 M12 | Cały zakres fazy |
| §20.2 | Cele: 1 doba ≤ 3 s w 50× dla 150 tys., determinizm 100% w CI, zapis < 5 s bez pauzy |
| §20.4 | Ryzyka: spirale ekonomiczne → testy 100-letnie; koszt własnego silnika → budżet jako test, nie jako dokument |

---

## 4. Pakiety robocze i podfazy

Kolejność jest wiążąca w obrębie łańcuchów zależności; łańcuchy A, B, C, D są względem siebie niezależne i mogą iść równolegle.

```
Łańcuch A (pamięć):   WP1 → WP2 → WP3 ──────────────┐
Łańcuch B (zapis):    WP4 → WP5 → WP6 → WP7 → WP8   ├→ WP10 → WP14
Łańcuch C (modding):  WP11 → WP12 ──────────────────┤
Łańcuch D (reszta):   WP9, WP13 ────────────────────┘
```

Faza jest rozbita na **6 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M12a — Pamięć** | WP1, WP2, WP3 | 5.1, 5.2, 5.3 | Panel `F3 → Pamięć` pokazuje rozbicie per podsystem z porównaniem do budżetu; przekroczenie świeci na czerwono. | `M12a-pamiec.md` |
| **M12b — Zapis i replay** | WP4, WP5, WP6, WP7, WP8 | 5.4, 5.5 | `Ctrl+S` w trakcie gry nie zatrzymuje symulacji ani na jeden tick; `--replay bug_4711.mgr` odtwarza cudze zgłoszenie klatka w klatkę. | `M12b-zapis-i-replay.md` |
| **M12c — Tryb 50× i skala 400 tys.** | WP9, WP10 | 5.6 | Doba gry ≤ 3 s dla miasta 150 tys.; metropolia 400 tys. ładuje się i chodzi w ≤ 6 GB. | `M12c-tryb-50x-i-skala.md` |
| **M12d — Modding** | WP11, WP12 | 5.7 | `example-mod/` — nowy towar, typ zakładu, polityka AI, panel UI i zdarzenie — instaluje się i przechodzi walidator determinizmu. | `M12d-modding.md` |
| **M12e — Lokalizacja i domknięcie** | WP13, WP14 | 5.8, 5.9 | Pełny artefakt fazy z §1 dokumentu fazy — wszystkie pięć punktów zmierzone testem w CI. | `M12e-lokalizacja-i-domkniecie.md` |
| **M12f — Oprawa interfejsu** | WP15, WP16, WP17, WP18 | 5.10 | Gra wygląda jak makiety wybrane w `docs/ui-inspiracje/`: własny krój, ikony, pasek ikon paneli, menu na żywej scenie. Niezależna od M12a–e. | `M12f-oprawa-ui.md` |

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | Rozszerzenia `engine/core` i `engine/ecs` | `M12a-pamiec.md` |
| 5.2 | Rachunek pamięci — metropolia 400 tys., budżet 6 GB (6144 MB) | `M12a-pamiec.md` |
| 5.3 | Redukcja rozmiaru komponentów — techniki (WP2) | `M12a-pamiec.md` |
| 5.4 | Format zapisu | `M12b-zapis-i-replay.md` |
| 5.5 | Zapis w tle — copy-on-write chunków ECS | `M12b-zapis-i-replay.md` |
| 5.6 | Tryb 50×: budżet czasu ticka rozpisany na systemy | `M12c-tryb-50x-i-skala.md` |
| 5.7 | Modding: `engine/script` | `M12d-modding.md` |
| 5.8 | Lokalizacja | `M12e-lokalizacja-i-domkniecie.md` |
| 5.9 | Systemy ECS dodawane przez M12 | `M12e-lokalizacja-i-domkniecie.md` |
| 5.10 | Oprawa interfejsu | `M12f-oprawa-ui.md` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam

**`engine/io`** (rozszerzenie własności M0):
- `SaveHeader`, `SectionEntry`, `SectionId`, `SaveFlags`, `ModStamp`
- `save_world_background(&World, &Path, SavePolicy) -> SaveHandle` — nie blokuje symulacji
- `save_world_blocking(&World, &Path) -> Result<SaveStats>` — dla headless i CI
- `load_world(&Path) -> Result<(World, SaveHeader)>` — z automatyczną migracją
- `trait Migration { fn from_version(&self) -> u32; fn to_version(&self) -> u32; fn section(&self) -> SectionId; fn migrate(&self, input: &SectionBytes, ctx: &MigrationCtx) -> Result<SectionBytes>; }`
- `MigrationRegistry::apply_chain(from, to, sections) -> Result<Sections>`
- `EventJournal`, `JournalEntry { tick, order: u16, input: PlayerInput }`, `journal_append`, `journal_replay`
- `ReplayFile`, `ReplayOrigin { FromSeed { seed, worldgen }, FromSave }`, `replay_verify(&ReplayFile) -> ReplayVerdict`

**`engine/script`** (nowy crate, własność M12):
- `ModManifest`, `ModId`, `ModPermissions`, `ScriptEntry`, `ScriptHook`, `PanelSlot`
- `ScriptHost::load(&[ModManifest]) -> Result<ScriptHost>`
- `ScriptHost::dispatch(&mut self, hook: ScriptHook, ctx: &ScriptCtx) -> Vec<Command>` — zwraca komendy, nigdy nie mutuje bezpośrednio
- `validate_determinism(&ModManifest, seed: u64, ticks: u64) -> DeterminismVerdict`

**`engine/ui`** (rozszerzenie własności M9/M11):
- `LocaleCatalog`, `TextKey`, `PluralCategory`, `Case`, `Gender`
- `t!(key, args...)` — makro rozwijane do `TextKey` w czasie kompilacji (umożliwia skan pokrycia w CI)
- `set_locale(LocaleId)` — bez restartu

**`engine/devtools`** (rozszerzenie własności M0):
- `TrackingAllocator`, `memory_report() -> MemoryReport` (rozbicie per tag vs budżet)
- `component_size_budget()` — tabela oczekiwań dla testu CI
- Panel `F3 → Pamięć`

**Budżet czasu (M12 jest właścicielem pomiaru, nie realizacji):**
- `TickBudget` — tabela budżetów czasu per system per prędkość (§5.6) wraz z bramkami `criterion` i raportem winowajcy. Realizacja każdego wiersza należy do fazy, która napisała dany system; M12 mierzy, raportuje i eskaluje. Budżet ruchu makro (≤ 0,55 ms/tick, zero iteracji per pojazd i per agent) przyjęty przez M10 do jego własnych kryteriów wydajności
- `SpeedGovernor` — watchdog i degradacja prędkości; decyzję o przejściu LOD deleguje do `MacroLodPolicy` (M10), własnej histerezy nie ma

**`tools/balansator`** (rozszerzenie własności M5):
- `run_century(seeds: &[u64], config: CenturyConfig) -> CenturyReport`
- `DegenerationMetric`, `AlarmThreshold`, analiza trendu w oknie kroczącym

**`tools/headless`** (rozszerzenie własności M0):
- `--replay <plik>`, `--century`, `--memory-report`, `--bisect-hash`

### Konsumuję

| Od | Co |
|---|---|
| M0 | `engine/core`: `Money`, `SimMinute`, `Tick`, `Q`, `Mood`, wszystkie `*Id`, `StreamId`, `rng()`. `engine/ecs`: `World`, `Entity`, archetypy, **granularność chunka** (D1), bufory komend, punkty synchronizacji. `engine/io`: `Snapshotable`, `state_hash`. `tools/headless` |
| M1 | `engine/voxel`: budżet rezydencji chunków, streaming (M12 ustawia limit, nie implementuje) |
| M2 | `data/`: katalogi towarów/budynków — M12 dokłada pola przypadków i rodzaju |
| M3 | `sim/agents`: `CitizenHot`/`CitizenCold` do rozdzielenia, magazyn doświadczeń, kolejka DES |
| M4 | `engine/nav`: CH i graf pieszy jako sekcje `Derived` |
| M5 | `tools/balansator` do rozbudowy; metryki §20.1 |
| M9 | `game/`: `PlayerInput` (wejścia gracza do dziennika i replayu), kroniki strukturalne |
| M4 | `sim/traffic`: **`travel_time(from: DistrictId, to: DistrictId, hour: u8) -> Duration`** i jego kalibracja z obserwacji mezo (§17.6) — jedyny model czasu przejazdu w grze, także dla makro |
| M10 | `sim/macro`: `MacroCell`, `MacroState.commute: CommuteMatrix` (snapshot tablic M4), pętla makro, **`lower_cell(&MacroCell, seed, tick) -> CellExpansion`** (czysta), `MacroLodPolicy::{target_lod, may_transition}` z histerezą |
| M11 | `engine/render`: przełączniki degradacji (LOD, kaskady cieni, cząstki, animacje) |

---

## 7. Testy i kryteria akceptacji

### 7.0 Zasada doboru scenariuszy

Dwie pomyłki popełnione w trakcie uzgodnień z M10 okazały się instancjami tej samej klasy błędu i obie dotyczą testów, nie kodu. Zapisane, bo są tanie do powtórzenia:

1. **Scenariusz testowy musi sam spełniać kontrakt, który weryfikuje.** M10 miał scenariusz referencyjny 1 dzielnica × 2 000 osób do walidacji tolerancji agregatów — przy 6 klasach dawał 333 osoby na komórkę, czyli **poniżej progu, którego ten scenariusz miał pilnować**. Test stał pod progiem własnego kontraktu i świeciłby na zielono, nie mierząc niczego.
2. **Scenariusz wygodny nie jest scenariuszem brzegowym.** M12 kalibruje wszystko od górnego końca zakresu (`metropolis_400k.ron`, 150 tys. przy 50×), bo faza nazywa się „skala". Tam jest najwięcej bajtów i najwięcej ticków — ale niekoniecznie najciaśniej: ziarno makro, tolerancja agregatów i statystyka małych prób są najciaśniejsze na **20 tys.**

Stąd audyt korpusu M12 i wymagany rozrzut:

| Korpus / scenariusz | Stan | Wymagany rozrzut |
|---|---|---|
| `metropolis_400k.ron` (odniesienie wydajności) | górny koniec | Zostaje jako odniesienie **pamięci**, nie jako jedyny scenariusz kontraktowy |
| Smoke degeneracji na PR (1 rok, 50 tys.) | 50 tys. leży dokładnie w punkcie przełączenia `cell_grain` | Zostaje — świadomie, bo punkt przełączenia jest wart pokrycia. Dodatkowo 20 tys. i 400 tys. w biegu nocnym |
| Testy kontraktowe LOD (§7.2) | były na jednym rozmiarze | **Pełny zakres §4.1**, 20 tys. → 400 tys. |
| Korpus zapisów złotych (miasto 5 tys.) | mały, świadomie — migracje nie mają progu populacyjnego | Zostaje. Ale co najmniej jeden zapis złoty z metropolii, żeby migracja była testowana też na rozmiarze sekcji, nie tylko na schemacie |
| Walidator determinizmu moda (5 000 ticków, małe miasto) | mały | Zostaje — determinizm nie ma progu populacyjnego |

Reguła ogólna: **jeśli test kontraktowy M12 ma jeden rozmiar miasta, to jest to błąd projektu testu** — chyba że da się wskazać powód, dla którego weryfikowany kontrakt jest niewrażliwy na rozmiar (jak migracje i determinizm powyżej).

### 7.1 Zapis i migracje

| Test | Kryterium |
|---|---|
| `save_load_roundtrip_400k` | `state_hash` przed zapisem == po wczytaniu; plik 50–300 MB |
| `save_time_under_budget` | Zapis 400 tys. < 5 s (§20.2), na dysku talerzowym też |
| `background_save_no_stall` | W oknie zapisu żaden tick > 1.3× mediany sprzed zapisu; **0 ticków pominiętych** |
| `background_save_hash_is_barrier_hash` | Hash w nagłówku == hash z chwili bariery, nie z chwili zakończenia |
| `cow_arena_within_budget` | Szczyt areny cieni < 280 MB przy 400 tys. i prędkości 50× |
| `crash_during_save_preserves_previous` | Zabicie procesu w 20 losowych punktach zapisu → poprzedni zapis wczytywalny |
| `migrates_from_every_historical_schema` | Każdy `tests/fixtures/saves/v*.mgs` migruje do bieżącej wersji i daje oczekiwany hash |
| `migration_chain_is_complete` | Dla każdej pary kolejnych wersji istnieje migracja; brak luk i skoków |
| `derived_sections_not_persisted` | Żadna sekcja `Derived` nie występuje w pliku; odtworzenie po wczytaniu < 3 s |
| `autosave_incremental` | < 200 ms, < 2 MB; snapshot + dziennik → hash identyczny ze stanem |

**Korpus zapisów złotych.** Przy każdym bumpie `schema_version` do `tests/fixtures/saves/` trafia zapis wygenerowany bieżącą wersją, wraz z oczekiwanym hashem po migracji do przyszłych wersji (przeliczanym przy każdym kolejnym bumpie). Dodanie migracji bez dodania zapisu = czerwone CI. Zapisy są małe (miasto 5 tys., ~2 MB), więc korpus po 50 wersjach schematu to 100 MB w repo — akceptowalne, alternatywą jest migracja bez testu. Migracje nie mają progu populacyjnego, więc mały rozmiar jest tu uzasadniony (§7.0) — ale **co najmniej jeden zapis złoty pochodzi z metropolii**, żeby ścieżka migracji była testowana także na realnym rozmiarze sekcji, nie tylko na kształcie schematu.

### 7.2 Determinizm

| Test | Kryterium |
|---|---|
| `determinism_gate` (na każdy PR) | 3 seedy × 50 tys. ticków × 2 przebiegi → identyczne ciągi hashy co 1000 ticków |
| `determinism_parallel_invariance` | Ten sam seed przy 1, 2, 4, 16 wątkach job systemu → identyczny hash |
| `replay_corpus` | 10 nagrań z korpusu regresyjnego odtwarza się bit-w-bit |
| `replay_bisect_reports_first_divergence` | Sztucznie wstrzyknięty niedeterminizm → raport wskazuje właściwy tick i archetyp |
| `lod_micro_meso_identity` | 30 dni mikro vs mezo → **bit w bit, tolerancja 0** (dok. 00 §4) |
| `lod_macro_conservation` | 30 dni 1× vs 50× → suma pieniądza **co do grosza**, suma masy każdego towaru **co do grama** |
| `lod_macro_aggregate_accuracy` | Agregaty **komórkowe i wyżej** (n ≥ `MIN_CELL_POP` = 500 osób lub ≥ 10 firm) odchylone ≤ 0,5%/mies. Test **nie** obejmuje przychodu pojedynczej firmy, ceny przy < 3 dostawcach i udziału pojedynczej marki (§5.6, D14). Uruchamiany na **wszystkich** rozmiarach miast z §4.1, nie tylko na dużym |
| `lod_macro_no_drift` | 12 miesięcy 1× vs 50× → **nachylenie regresji różnicy nieodróżnialne od zera** (test istotności) **oraz autokorelacja znaku różnicy lag 1 ≤ 0,3** — sam próg na nachylenie da się przypadkiem przejść na krótkiej próbce |
| `macro_lod_transition_blocked` | Wejście w makro w trakcie fixingu giełdowego / negocjacji związkowych / strajku → `may_transition` zwraca `BlockReason`, governor nie przełącza i pokazuje powód |
| `mod_determinism_validator` | Mod niedeterministyczny odrzucony, poprawny przyjęty (oba w korpusie) |

### 7.3 Wydajność i pamięć

| Test | Kryterium |
|---|---|
| `component_sizes_are_budgeted` | `size_of` każdego komponentu ≤ tabela oczekiwań |
| `memory_budget_400k` | RSS ≤ 6 144 MB po 5 latach gry; każda pozycja §5.2 w budżecie |
| `speed_50x_day_budget` | 1 doba ≤ 3 s dla 150 tys. na maszynie referencyjnej CI (§20.2) |
| `speed_50x_per_system_budget` | **Każdy** wiersz tabeli §5.6 w swoim budżecie — czerwone także wtedy, gdy suma się mieści |
| `macro_work_unit_is_not_entity` | Licznik iteracji per agent/pojazd w systemach przy 50× == 0. To jest test założenia, na którym stoi cały budżet |
| `no_hardcoded_cell_count` | Żaden budżet, licznik ani asercja M12 nie zawiera stałej liczby komórek — wszystko przez `state.cells.len()`. Scenariusz na mieście 20 tys. (30 komórek) i 400 tys. (240) przechodzi tym samym kodem |
| `speed_50x_across_city_sizes` | Budżet ticka mierzony na wszystkich rozmiarach z §4.1, nie tylko na 150 tys. — koszt makro musi skalować się **w dół** wraz z liczbą komórek |
| `no_alloc_in_hot_systems` | Licznik alokacji == 0 dla systemów `#[no_alloc]` (profil `bench`) |
| `criterion_regression` | Regresja > 10% w benchmarku gorącym → czerwone CI |
| `chronicle_query_latency` | „Historia firmy X przez 100 lat" < 50 ms z dysku |

### 7.4 Sesja 100 lat — metryki degeneracji i progi

100 lat = 52.5 mln ticków ≈ 30 h przy 480 ticków/s. Za dużo na każdy PR, więc harmonogram trójstopniowy:

- **Na każdy PR:** 1 rok gry, miasto 50 tys. (~20 min) — smoke.
- **Nocny:** 10 lat × 3 seedy, miasto 150 tys.
- **Tygodniowy:** 100 lat × 5 seedów, miasto rosnące 150 → 400 tys., maszyna dedykowana.

| Metryka | Pomiar | Ostrzeżenie | **Alarm (fail)** |
|---|---|---|---|
| RSS procesu | co dobę gry | > 2 MB/rok gry po ustabilizowaniu populacji | > 6 GB lub trend liniowy > 10 MB/rok |
| Fragmentacja (`RSS / live_bytes`) | co dobę | > 1.5 | > 2.0 |
| Liczba żywych encji per typ | co dobę | odchylenie > 20% od trendu populacji | monotoniczny wzrost bez sufitu przez 5 lat (wyciek encji) |
| Encje „zombie" (bez właściciela i referencji) | rocznie | > 0 | > 100 |
| Suma pieniądza = emisja − destrukcja | próbkowane co godzinę gry | — | **jakakolwiek różnica ≠ 0** |
| Suma masy per towar = produkcja − konsumpcja − straty | co dobę | — | **jakakolwiek różnica ≠ 0** |
| Inflacja roczna (koszyk CPI) | rocznie | poza −5…+15% (§20.1) | poza −20…+50%, lub 3 lata z rzędu w ostrzeżeniu |
| Bezrobocie | rocznie | poza 2–20% | > 40% lub < 0.5% przez 3 lata |
| Populacja | rocznie | zmiana > ±8%/rok | spadek > 50% od szczytu lub wzrost > 15%/rok przez 5 lat |
| Rotacja firm (bankructwa/powstania) | rocznie | > 40%/rok | 0 firm w kategorii przez 2 lata (wymarcie branży) |
| Mediana majątku GD / mediana ceny koszyka | rocznie | spadek > 30% od roku 5 | spadek > 60% (zubożenie) lub wzrost > 400% (hiperinflacja majątku) |
| Gini dochodu | rocznie | > 0.55 | > 0.75 (całe miasto w jednych rękach) |
| Czas ticka p50 | ciągle | p99 > 2× p50 | p50 w roku 100 > 1.3× p50 w roku 10 przy tej samej populacji |
| Długość kolejki DES | co dobę | > 8 zdarzeń/agenta | monotoniczny wzrost przez 5 lat (zdarzenia bez konsumpcji) |
| Rozmiar kroniki na dysku | rocznie | > 200 MB/rok gry | brak rotacji po 30 latach |
| Wysycenie areny cieni zapisu | per zapis | > 60% | 100% (pauza symulacji) |
| Dryf makro vs mezo: nachylenie regresji różnicy agregatu (okno 12 mies.) | rocznie | istotne statystycznie | > 0,5%/rok w jedną stronę |
| Dryf makro vs mezo: **autokorelacja znaku różnicy (lag 1)** | rocznie | > 0,2 | **> 0,3** — systematyczne przesunięcie, po 100 latach czynnik ~120× |
| Determinizm | hash co 1000 ticków, 2 przebiegi | — | **jakakolwiek rozbieżność** |

**Wykrywanie wycieków pamięci:** `TrackingAllocator` z tagiem per podsystem; migawka liczników co rok gry; test — liczniki w roku 100 ≤ 1.2× liczniki w roku 20 przy porównywalnej populacji. Raport top-20 rosnących tagów jako artefakt CI.

**Wykrywanie dryfu ekonomicznego:** balansator porównuje nie wartości, lecz **nachylenie regresji liniowej w oknie kroczącym 20 lat**. Alarm, gdy |nachylenie| przekracza próg, nawet jeśli wartość bezwzględna jest jeszcze w widełkach. To jedyny sposób na wychwycenie powolnej spirali — takiej, która w roku 40 wygląda niewinnie, a w roku 90 zabija miasto. Roczna migawka tego nie widzi.

### 7.5 Modding i lokalizacja

| Test | Kryterium |
|---|---|
| `sandbox_escapes` | 30 prób ucieczki (IO poza katalogiem, sieć, `os.time`, `require`, `debug`, FFI, float w `Money`, mutacja z hooka UI, `pairs` po hash-mapie hosta…) → kontrolowany błąd, zero efektów ubocznych |
| `instruction_budget_is_deterministic` | Hook z pętlą nieskończoną przerywany po tej samej liczbie instrukcji na każdej maszynie i w obu przebiegach |
| `mod_commands_are_ordered` | Dwa mody piszące do tej samej encji → wynik zależy tylko od `load_order`, nie od kolejności ładowania z FS |
| `example_mod_all_hook_classes` | Przykładowy mod realizuje wszystkie 4 klasy hooków z §18.3 i przechodzi walidator |
| `save_with_changed_mods_warns` | Wczytanie przy innej liście modów → flaga `MODS_CHANGED`, ostrzeżenie, unieważnienie replayu |
| `all_keys_present_in_all_locales` | Każdy `t!()` w kodzie ma wpis w pl-PL i en-US |
| `no_unused_keys` | Brak wpisów nieużywanych (ostrzeżenie, nie fail — klucze modów) |
| `cldr_plural_vectors` | Wektory CLDR dla pl (one/few/many/other) i en przechodzą |
| `pseudo_locale_no_clipping` | Zrzuty UI w `x-long` (×1.4 długości + diakrytyki) bez przycięć |

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Dlaczego realne | Mitygacja |
|---|---|---|---|
| R1 | **Cel 3 s/dobę nieosiągalny** mimo zejścia do makro | M4 zmierzył 11 ms/tick dla mezo — 5× ponad cały budżet. Budżet §5.6 stoi na jednym założeniu: że przy 50× **żaden** system nie iteruje per encja. Wystarczy jeden, który to robi, żeby cel przepadł | Test `macro_work_unit_is_not_entity` sprawdza to założenie wprost, a nie przez pomiar czasu. Budżet per system z osobną bramką CI wskazuje winowajcę. Drabina cięć §5.6 daje 0,7 ms (34%) bezboleśnie. Przy dalszej porażce — D12 |
| R1b | **Makro dryfuje ekonomicznie** wobec mezo | 0,4% w losową stronę co miesiąc jest nieszkodliwe; 0,4% w tę samą stronę to po 100 latach czynnik ~120×. Sam próg na nachylenie regresji da się przypadkiem przejść na krótkiej próbce | `lod_macro_no_drift` testuje nachylenie **i** autokorelację znaku różnicy (lag 1 ≤ 0,3) — za M10. Zachowanie pieniądza i masy egzekwowane dokładnie przez `ledger_post()` na każdym LOD. Przy porażce: bisekcja po systemach (N w makro, reszta w mezo) |
| R1d | **Regresja widoczna tylko na małym mieście; test, który sam nie spełnia weryfikowanego kontraktu** | M12 to faza o skali, więc jej scenariusze są z natury duże, a „wygodny" myli się z „brzegowym". Ziarno makro było tego instancją: stała 240 przeszłaby każdy test M12 i pękła dopiero u gracza z miastem 20 tys. M10 znalazł u siebie wariant gorszy — scenariusz walidujący próg stał **poniżej tego progu**, więc świecił na zielono, nie mierząc niczego. Klasa systemowa, nie przypadek | Audyt korpusu i reguła doboru scenariuszy w **§7.0**, z wymaganym rozrzutem per korpus. Testy kontraktowe LOD na pełnym zakresie §4.1: `lod_macro_aggregate_accuracy`, `speed_50x_across_city_sizes`, `no_hardcoded_cell_count`. Jeden zapis złoty z metropolii mimo małego korpusu migracji |
| R1c | **Ktoś w M12 oprze się na dokładności, której makro nie ma** | D14: przychód pojedynczej firmy odchyla się o 3–12%, nie 0,5%. Pokusa jest realna w „co jeśli" AI i w autozapisie przed decyzją — tam wynik wygląda jak prognoza | Zastrzeżenie wpisane do kontraktu LOD (D14) i do obietnicy produktu: 50× przewiduje dzielnicę i miasto, nie firmę. Test `lod_macro_aggregate_accuracy` **celowo nie obejmuje** wielkości o małym n, żeby zielone CI nie sugerowało gwarancji, której nie ma |
| R2 | **ECS nie eksponuje chunków** → zapis w tle bez pauzy niewykonalny | Właścicielem ECS jest M0; jeśli chunk jest szczegółem implementacyjnym, CoW nie ma się o co zaczepić | Zgłoszone jako D1 **przed** startem WP5. Plan B: zapis z pauzą 1.2 s przy autozapisie i jawną pauzą przy ręcznym — porażka §20.2, ale nie fazy |
| R3 | **Budżet 6 GB nie domyka się** po WP10 | Liczby w §5.2 są zaprojektowane, nie zmierzone; voxel (2.1 GB) to 43% budżetu i należy do M1/M11 | Drabina cięć §5.2 z ustaloną kolejnością i zyskiem. Szczeble 1–3 (990 MB) są bezbolesne i wystarczają na błąd projektowy rzędu 20% |
| R4 | **Mod łamie determinizm mimo sandboxa** | Powierzchnia jest duża; `pairs` w Lua to jedna z co najmniej pięciu subtelnych pułapek | Pięć niezależnych mechanizmów (§5.7) + walidator jako bramka + odcisk modów w zapisie. Przy wykryciu w terenie: `MODS_CHANGED` chroni replay i ranking, a raport idzie do autora moda |
| R5 | **Test 100-letni jest za drogi, żeby go uruchamiać** | 30 h per seed — po trzeciej czerwonej nocy zespół przestanie patrzeć | Harmonogram trójstopniowy (§7.4): 20-minutowy smoke na PR łapie 80% regresji. 100-letni tygodniowo, z raportem HTML zamiast ściany logów |
| R6 | **Migracje gniją** — dopisywane bez testu, przestają działać po roku | Klasyczny los migracji w każdym projekcie | Korpus zapisów złotych jako **twardy wymóg CI**: migracja bez zapisu testowego nie wchodzi do main |
| R7 | **Dryf ekonomiczny widoczny dopiero po 60 latach** | Wolne spirale są niewidoczne w migawkach rocznych | Analiza nachylenia w oknie 20-letnim (§7.4), nie wartości bezwzględnych |
| R8 | **M12 zależy od 11 faz naraz** — dowolne opóźnienie blokuje | Faza domykająca z natury | Łańcuchy A–D są niezależne. Łańcuch B (zapis) i C (modding) nie potrzebują M9–M11 i mogą ruszyć wcześniej. Tylko WP9 i WP10 wymagają pełnego stosu |
| R9 | **Fragmentacja alokatora rośnie przez 100 lat** | Miliony krótkożyjących encji; domyślny alokator systemowy nie jest projektowany pod taki wzorzec | Areny dla encji o wysokiej rotacji (WP2). Metryka `RSS/live_bytes` z progiem 2.0. Plan B: `mimalloc` jako alokator globalny — zmiana jednej linii, mierzalna |
| R10 | **Zapis w tle przy 50× wysyca arenę cieni** | 2.8 ms/tick narzutu CoW przy 2 ms budżetu | `SpeedGovernor` obniża prędkość na czas zapisu. Gracz widzi spowolnienie, nie zacięcie. Metryka alarmowa |

---

## 9. Decyzje otwarte

Próba uzgodnienia z agentem planującym M0 nie powiodła się — kanał nie był dostępny w trakcie pisania tego dokumentu, więc D1–D4 pozostają jednostronnymi propozycjami M12. Uzgodnienie z M10 przebiegło w dwóch rundach i **zamknęło D5, D11, D11b i D13**; otwartą pozostawiło D14. Poniższe **wymagają rozstrzygnięcia z właścicielami odpowiednich crate'ów przed startem odpowiednich WP**, nie przed startem fazy jako całości.

Jedna pozycja jest twardą blokadą, nie tematem do dyskusji: **D1** — bez publicznej granularności chunka ECS zapis w tle bez pauzy jest niewykonalny i §20.2 („zapis < 5 s bez pauzy") przepada.

**Ścieżka eskalacji D1.** Kontakt z agentem planującym M0 był niedostępny przez cały czas pracy nad tym dokumentem, więc D1 nie jest uzgodnieniem, tylko **jednostronną propozycją M12 do rozstrzygnięcia przez koordynatora** — razem z D2 i D3, które dotyczą tego samego crate'a. Rozstrzygnięcie musi zapaść **przed zamknięciem planu M0**, nie przed startem M12: wszystkie trzy wymagają, żeby M0 coś zbudował lub wystawił od początku, a dobudowanie granularności chunka do gotowego ECS jest przepisaniem, nie rozszerzeniem. Jeśli rozstrzygnięcie wypadnie na „nie", plan B (zapis z pauzą ~1,2 s) jest wykonalny i mieści się w reszcie fazy — przepada tylko jeden cel z §20.2, nie faza.

Dwie pozycje czekają na fazę, z którą M12 nie rozmawiał: **D11** wymaga jeszcze potwierdzenia od M4 (czy zatrzymuje u siebie `travel_time`), a **D14** wymaga decyzji właściciela dok. 00 co do zapisu zastrzeżenia w kontrakcie LOD.

| # | Decyzja | Kontekst | Propozycja M12 | Blokuje |
|---|---|---|---|---|
| **D1** | **Czy chunk archetypu ECS jest publiczną jednostką API?** | Copy-on-write potrzebuje `ChunkId`, iteracji o deterministycznej kolejności i dostępu do surowych kolumn. Jeśli chunk jest szczegółem implementacyjnym `engine/ecs`, zapis w tle bez pauzy jest niewykonalny | `World::chunks()`, `World::chunk_bytes(ChunkId)`, `AtomicPtr` na wskaźniku chunka w tablicy świata. Rozmiar chunka 64 KB | WP5 (R2) |
| **D2** | **Kształt `RawSnapshot` w M0** | Jeśli M0 zserializuje świat do jednego płaskiego bufora, M12 przepisuje serializację od zera zamiast ją rozszerzać | `RawSnapshot` jako lista nazwanych bloków `(SectionId, Vec<u8>)` już w M0. Jedna ścieżka serializacji dla snapshotu i dla `state_hash` — nie dwie | WP4 |
| **D3** | **`ComponentSchemaId` i `SchemaRegistry` w `engine/core`** | Migracje są własnością M12, ale rejestr musi mieszkać tam, gdzie typy. Wymaga, by każdy komponent deklarował stabilny id i `schema_version` **od M0**, nawet gdy M0 nie migruje niczego | Dopisek do dok. 00 §2. Id nigdy nierecyklowane | WP6 |
| **D4** | **`MoneySmall(i32)` w komponentach gorących** | Dok. 00 §2 mówi „pieniądz zawsze i64". Budżet dobowy GD w `i32` oszczędza 4 B × 560 tys. encji = 2 MB — mało. Ale cena jednostkowa w ofertach i partiach to kolejne ~5 MB | Dopuścić `MoneySmall(i32)` **wyłącznie** w komponentach o zakresie udowodnionym testem własnościowym; arytmetyka zawsze w `i64`, konwersja jawna i sprawdzana. Alternatywa: odrzucić — zysk 7 MB nie jest wart wyjątku w kontrakcie. **M12 nie ma silnego zdania; domyślnie odrzucamy** | WP2 |
| **D5** | ~~`StreamId::Script` — numer wariantu~~ **ROZSTRZYGNIĘTE** | — | Koordynator przydzielił M12 zakres **320–339** (K-4). `Script = 320`, `SaveJitter = 321`, reszta rezerwa. M12 nie wychodzi poza swój zakres | — |
| **D6** | **Czy kroniki wchodzą do pliku zapisu?** | Kronika 100-letniego miasta to ~2 GB na dysku. Wpakowanie jej do zapisu robi z 150 MB pliku 2 GB. Niewpakowanie oznacza, że skopiowanie zapisu na inny komputer gubi historię świata | Zapis zawiera **indeks** kroniki i referencję do pliku logu; „eksport pełny" (osobna komenda) pakuje oba w jedno archiwum. Wymaga zgody M9 (właściciel panelu Kronika) | WP3 |
| **D7** | **Rozmiar voxela i wynikający z niego budżet rezydencji** | 1 400 MB na chunki rezydentne to 23% budżetu i największa pojedyncza pozycja. Liczba jest zgadywana, bo PRD nie definiuje rozmiaru voxela w metrach | Ustalić z M1/M11 rozmiar voxela i promień pełnej rezydencji, a potem przeliczyć §5.2. Do tego czasu 1 400 MB jest placeholderem, nie zobowiązaniem | WP10 |
| **D8** | **Rozszerzenie schematu `data/` o przypadki i rodzaj gramatyczny** | Lokalizacja PL wymaga odmiany nazw towarów i budynków. To zmiana schematu katalogów, których właścicielami są M2/M6 | Pole `name` w `data/goods/`, `data/buildings/` przechodzi z `String` na strukturę z formami przypadków; EN używa `nom` + `plural`. Bump `schema_version` tych katalogów | WP13 |
| **D9** | **Czy WASM (`wasmtime`) wchodzi do M12?** | PRD §16.1 i §18.3 dopuszczają Lua **lub** WASM. Dwa hosty = dwa sandboxy do audytu i dwa dowody determinizmu | **M12 robi tylko Lua.** WASM jako osobny, późniejszy WP, jeśli pojawi się realna potrzeba wydajnościowa. API kontekstu projektowane tak, by dołożenie backendu nie zmieniało kontraktu z modderem | — (decyzja M12, do zatwierdzenia) |
| **D10** | **Maszyna referencyjna dla celów z §20.2** | „1 doba ≤ 3 s dla 150 tys." nie znaczy nic bez sprzętu. Ten sam problem dotyczy „60 FPS na GPU średniej klasy". Cała tabela budżetów §5.6 jest bez tego nieweryfikowalna | Zdefiniować jedną konfigurację referencyjną (CPU 8 rdzeni ~2022, 16 GB RAM, NVMe) i pinować na niej runner CI. **Priorytet podniesiony** — blokuje trzy WP naraz | WP9, WP10, WP14 |
| **D11** | ~~Właściciel makro LOD ruchu~~ **ROZSTRZYGNIĘTE z M10** | — | Własność **podzielona**: `sim/macro` (M10) bierze jednostkę pracy i pętlę (`MacroCell`, alokacja podróży do komórek, agregaty); `sim/traffic` (M4) zachowuje `travel_time(from, to, hour)` i jego kalibrację. `MacroState.commute: CommuteMatrix` to snapshot tablic M4 — M10 trzyma, M4 wypełnia. Powód M10: `sim/macro` nie zawiera logiki dziedzinowej, żeby nie istniały dwa modele tej samej rzeczy. **Wymaga jeszcze potwierdzenia od M4** (u M10 jako jego D2) | WP9 |
| **D11b** | ~~Ziarno agregacji `MacroCell`~~ **ROZSTRZYGNIĘTE z M10** | M12 zakładał ~2 400 komórek (zgadnięte, 62 osoby/komórkę — łamało tolerancję ze statystyki). M10 podał 240, co jest poprawne dla metropolii, ale dla miasta 20 tys. daje 333 osoby/komórkę i **też** łamie tolerancję | **Ziarno nie jest stałą:** `cell_grain(pop, districts) -> ClassGrain` z progiem `MIN_CELL_POP = 500`; klasy łączą się parami po sąsiadujących przedziałach statusu. Zakres 30–240 komórek w całym §4.1. **Zobowiązanie M12:** nigdzie żadnej stałej liczby komórek — wyłącznie `state.cells.len()`; pilnuje tego `no_hardcoded_cell_count`. Budżet §5.6 przeliczony na 180 komórek (150 tys.) | WP9 |
| **D12** | **Co ciąć, jeśli 3 s/dobę nie domknie się mimo makro?** | Budżet §5.6 domyka się z 34% marginesem, ale każda z jego liczb jest projektowana, nie zmierzona; M4 pokazał, że pomyłka rzędu 5× jest realna | Drabina z §5.6: (1) kohorty 12 → 6 klas, (2) ekonomia na ticku godzinowym w makro — razem 0,7 ms, bez widocznych konsekwencji. Dopiero potem (3) rozluźnienie celu do **≤ 4 s/dobę** lub (4) definicja celu dla 100 tys. — **oba są zmianą PRD §20.2 i decyzją projektanta, nie inżyniera.** M12 rekomenduje (3) przed (4): 4 s dla 150 tys. jest uczciwsze niż 3 s dla miasta, którego nikt nie obiecywał | WP9 |
| **D13** | ~~Czy makro utrzyma zachowanie pieniądza i masy?~~ **ROZSTRZYGNIĘTE z M10 — TAK, to nie jest blokada** | — | Każdy przepływ w makro idzie przez `ledger_post()` (zapis dwustronny, ten sam co w mezo): pieniądz przechodzi między kontami, nie powstaje przy alokacji. Podział agregatu domyka się korektą reszty do pierwszego wg posortowanego klucza (dok. 00 §2); to samo dla `Qty`. Testy własnościowe po każdym kroku makro, tolerancja 0. Sformułowanie M10, które warto zapamiętać: *dokładność mówi, komu przypadł pieniądz; zachowanie mówi, ile go jest* | — |
| **D14** | **Zakres obietnicy trybu 50×** | M10 wykazał, że próg 0,5% jest **fizycznie nieosiągalny** dla przychodu pojedynczej firmy (3–12%), ceny przy < 3 dostawcach (2–8%) i udziału pojedynczej marki (2–5%) — to wariancja rozkładu wielomianowego przy małym n, nie usterka | **Tryb 50× przewiduje stan dzielnicy i miasta, nie los pojedynczej firmy.** M12 musi zdjąć to założenie wszędzie, gdzie mogłoby się zakraść: „co jeśli" AI, autozapis przed decyzją, porównywanie przebiegów, komunikaty w UI. Trafia do dok. 00 §4 jako zastrzeżenie do kontraktu LOD — uzgodnienie u właściciela dok. 00; równolegle zgłoszone przez M10 jako jego D1 | WP9, WP14 |
| **D15** | **Portret mieszkańca w karcie inspekcji** | Makieta `hud-b.png` ma portret. Prawdziwy portret to model `.mvox` wyrenderowany do tekstury — nowa ścieżka w rendererze (M11), której dziś nie ma | **Nie w M12f.** Karta dostaje ikony przy potrzebach; portret wraca, gdy renderer będzie miał render do tekstury z innego powodu (np. miniatury zapisów) | — |
| **D16** | **Dźwięk interfejsu bez wykonawcy** | `ui-design.md` §8 przypisywał go M11 razem z ikonami; M11 zamknął się bez niego, ikony przejęła M12f | Zostaje bez wykonawcy świadomie; wraca przy pierwszej fazie, która dołoży mikser dźwięku. Nie dopisujemy go do M12f, bo nie ma czym grać | — |

---

## 10. Szacunek wielkości

| WP | Zakres | Rozmiar |
|---|---|---|
| WP1 | Audyt i budżet pamięci jako test | **M** |
| WP2 | Redukcja komponentów, areny, pule | **L** |
| WP3 | Magazyn doświadczeń i kroniki na dysk | **M** |
| WP4 | `engine/io` w pełnej postaci | **L** |
| WP5 | Zapis w tle (copy-on-write) | **L** |
| WP6 | Migracje schematu + korpus złoty | **M** |
| WP7 | `EventJournal` i autozapisy | **M** |
| WP8 | Replay i bramka determinizmu w CI | **M** |
| WP9 | Tryb 50× (z dowodem równoważności LOD) | **L** |
| WP10 | Skala 400 tys.: profilowanie i domknięcie budżetu | **XL** |
| WP11 | `engine/script`: host Lua i sandbox | **XL** |
| WP12 | `ModManifest` i menedżer modów | **L** |
| WP13 | Lokalizacja PL/EN | **M** |
| WP14 | Testy długich sesji, balansator, benchmarki | **L** |

Rozkład: 2 × XL, 5 × L, 6 × M, 0 × S — to faza bez drobnicy.

Dwa XL dominują i są nimi z różnych powodów. **WP10** jest XL, bo to praca otwarta: pętla profil → optymalizacja → pomiar nie ma z góry znanego dna, a zależy od jakości kodu jedenastu poprzednich faz. **WP11** jest XL, bo sandbox to praca bezpieczeństwa — powierzchnia ataku jest duża, a „prawie szczelny" nie ma wartości. Obu nie da się rozbić na mniejsze kawałki dające samodzielny artefakt.

**WP9 zostaje L.** Sam suwak prędkości to godzina pracy; L bierze się stąd, że M4 zmierzył ruch mezo na 11 ms/tick wobec 2,08 ms całego budżetu, więc WP9 nie jest przełącznikiem, tylko wymuszaniem budżetu w czterech cudzych crate'ach naraz. Ryzyko urośnięcia do XL **odpadło** wraz z D11: makro ruchu pisze M10 (pętla) i M4 (`travel_time`), M12 dostarcza budżet, pomiar i `SpeedGovernor`. Faza ma dwa XL, nie trzy.

**WP6** (M) to ~300 linii kodu; cała reszta tego pakietu to dyscyplina korpusu testowego. **WP13** (M) jest M, a nie S, bo polska pluralizacja i odmiana przez przypadki to realna praca, a nie podmiana stringów.

---

## Zmiany wpisane po M4

Zgodnie z `K-18`. To są rzeczy, o których wiemy **na pewno** po zamknięciu M4;
faza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| T-1 ★ | **Kolejność regionów w `data/names/regions.ron` i kolejność wpisów w plikach pul są kontraktem zapisu gry** (`K-28`). Modding pul nazw **nie może** przestawiać ani usuwać wpisów, tylko dopisywać na końcu | `Identity.first_name`/`last_name` to indeksy w tablicy sklejonej z regionów w tej kolejności; przestawienie przenumerowuje wszystkich mieszkańców zapisanego świata. Walidator modów z WP12 ma to sprawdzać tak samo, jak sprawdza kolejność klas pojazdów |
| T-2 ★ | **Warstwa Mikro jest wyłączona przy zamkniętym oknie i kosztuje wtedy jedno sprawdzenie atomika na pojazd** — tryb 50× nie musi jej gasić osobno | §7.3 M4 zakłada, że w 50× mikro jest wyłączone „w całości". Jest — z konstrukcji, bo warstwa ma promień 0 jako stan domyślny i headless nigdy go nie otwiera. Zostaje realny problem z `D2`: sam **mezo** nie mieści się w metryce §20.2 i to on wymaga LOD makro |
| T-3 | **Rozbiór czasu przejazdu jest dostępny per podróż** (`LedgerEntry.node_delay_cs`, `blocked_cs`), a nie tylko globalnie | LOD makro ruchu musi odtworzyć **wszystkie trzy** składniki, nie tylko sumę: przy mieście, w którym kolejki bywają trzy razy droższe od jazdy (`L-15`), agregat zbudowany z samego czasu całkowitego rozjedzie się z mezo przy pierwszej zmianie sieci |
| T-4 | **Test spójności LOD istnieje jako `sim/world/tests/lod.rs`** i porównuje dwa przebiegi tego samego świata: ciąg hashy, sumę gotówki, paliwo i taryfy, z tolerancją 0 | Tryb 50× ma **osobny, jawnie słabszy kontrakt** (`D2`), ale jego test ma wyglądać tak samo: dwa przebiegi, ta sama lista wielkości, inny próg. Nie pisz drugiego harnessu — rozszerz ten o trzeci przebieg i tolerancję z dok. 00 §4 |
| T-5 | **Bramka wydajnościowa `m3day` biegnie już w CI** (dwie doby, cztery przebiegi: wątki, prędkość, okno Mikro) | Sto dób z kryterium WP12 M4 zostaje biegiem nocnym, a nie bramką pull requesta — cztery przebiegi po sto dób to kilkadziesiąt minut. M12 jest właścicielem testu długich sesji i to on ma zdecydować, czy sto dób wchodzi do CI, czy do osobnego harmonogramu |


## Zmiany wpisane po M9e

Zgodnie z `K-18`. Szczegóły — tabela `DI-n` w `M9e-panele-czas-kariera.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| DM-1 ★ | **Decyzja `D6` („czy kroniki wchodzą do pliku zapisu?") zmienia przedmiot.** Kronika gracza jest **widokiem pochodnym**: powstaje z `Events::chronicle()`, z pierścieni decyzji firm i z dziennika wejść, a wszystkie trzy i tak są w zapisie. Pytanie nie brzmi więc „czy zapisać kronikę", tylko **„czy te trzy dzienniki wolno rotować"** — bo to one rosną, a kronika jest ich odczytem | Dwa gigabajty z `D6` liczono dla kroniki zbieranej ze wszystkich zdarzeń świata. Zbierana i zawężona do gracza ma rzędu setek tysięcy wpisów przez sto lat (`DI-4`). `ChronicleStore` z `M12a` WP3 nadal jest potrzebny — ale dla **dzienników źródłowych**, nie dla widoku |
| DM-2 ★ | **`MetricsRecorder` też nie ma pliku i nie potrzebuje go w tym kształcie.** Serie odtwarzają się z przewinięcia dziennika wejść. Do rozstrzygnięcia w M12: czy przewinięcie stuletniej gry przy wczytaniu jest dopuszczalnym kosztem, czy serie mimo wszystko idą na dysk | Wykonanie decyzji otwartej nr 6 dokumentu M9 w części, którą dało się wykonać teraz. Trzy pliki poboczne skurczyły się do jednego (dziennik replayu), który format już ma |
| DM-3 | **Panele z modów dokłada się przez `PanelRegistry::register(PanelDesc)`**, a `PanelDesc` jest **daną**: klucz tytułu, lista źródeł danych, funkcja budująca model i funkcja rysująca. Sufit jest jeden i nazwany: `PanelModel` to enum, więc panel spoza kodu gry potrzebuje albo wariantu, albo zatarcia typu | Zapowiedziane w §6 dokumentu M9; wpisane tutaj, żeby `engine/script` wiedział, czego dotknie |
| DM-4 | **`Layout` (przypięte panele i aktywny) zapisuje się w profilu gracza, nie w zapisie świata**, i nie wchodzi do hasha — pilnuje tego test `uklad_paneli_nie_zmienia_hasha_stanu` | `docs/ui-design.md` §5. Wpisane, bo `M12b` §5 wymienia `UiState` jako sekcję zapisu — układ doku **nie jest** jej częścią |
| DM-5 | **Scenariusze są moddowalne od M9e**: `data/scenarios/scenarios.ron` z `schema_version`, kolejność wpisów jako kontrakt zapisu (`K-71`). Walidator modów sprawdza ją tak samo jak kolejność klas pojazdów (`K-24`) | Katalog powstał razem z WP12 i jest daną od pierwszego dnia — nie ma czego przenosić do modów później |

## Zmiany wpisane po M10a

Zgodnie z `K-18`. Szczegóły — tabela `E-n` w `M10a-jadro-makro-historia.md`,
rozstrzygnięcia kontraktowe — `K-78` w dokumencie 00.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| DN-1 ★ | **`MacroLodPolicy` istnieje i ma inną sygnaturę, niż zapowiadał kontrakt M10 §6.** Jest `target_lod(&self, from: CityLod, load: &LoadStats) -> CityLod` (a nie `target_lod(speed, load)`) oraz `may_transition(&self, from, to, state: &MacroState) -> Option<BlockReason>` (a nie `(…, world: &World)`). Poziom miasta to **`CityLod::{Mezo, Makro}`** z `magnat_macro::lod`, nie `Lod` mieszkańca z `sim/agents` | Histereza jest po stronie **obecnego** poziomu: bez `from` funkcja nie odróżnia wejścia od wyjścia i dwa progi nie znaczą nic. `&MacroState` zamiast `&World`, bo `sim/macro` widzi `World` wyłącznie w `lift`/`lower` — i to jest cała obrona przed prognozą, która coś w świecie zmienia |
| DN-2 ★ | **`fast_forward()` nie powstał i M12c buduje go sam** na `step()` + `MacroParams::days_per_step`. Wszystko, czego potrzebuje, już jest: krok może reprezentować wiele dób i każda faza mnoży przez tę liczbę swój przepływ (`E-7`), a `przekroczono` pilnuje kadencji rzadszych niż dobowa | Historia „na sucho" i tryb 50× to ten sam silnik z innym wejściem (M10 §1 pkt 1). `dry_run` jest dziś jedynym wołającym tej pętli; drugi wołający dopisuje się w M12c bez nowego mechanizmu |
| DN-3 ★ | **`FastForwardConfig::keep_identities` dostaje twarde znaczenie i to M12c musi je udźwignąć.** Krok makro ma **populację zamkniętą**: ludzie się przeprowadzają i starzeją, ale nikt nie powstaje i nikt nie znika, bo `CitizenSeed.birth_index` jest indeksem encji ECS (`K-78` pkt 2). Wzrost i spadek liczby mieszkańców w trybie 50× wymaga **tworzenia i usuwania encji przy `lower()`** — czyli tego, czego M10a świadomie nie zrobił | Nowy mieszkaniec w makrze musiałby dostać indeks encji, której jeszcze nie ma. Zrobienie tego w M10a znaczyłoby drugi generator populacji obok Etapu 8 (`K-8`); w M12c churn encji i tak musi powstać, więc tam jest tańszy i ma jednego właściciela |
| DN-4 | **Budżet kroku makro jest zmierzony, nie oszacowany**: metropolia (240 komórek, 400 tys. mieszkańców, 3 tys. firm, 60 towarów w obrocie) liczy dobę w **6,3 ms**, czyli 7500 kroków w ~47 s wobec sufitu 60 s. Klucz w linii bazowej: `m10-macro/m10-1-miesiac-metropolia` (mierzy **miesiąc**, bo trzy fazy nie chodzą codziennie) | M12 §5 planuje tryb 50× na tym silniku i potrzebuje liczby, a nie widełek. Przy okazji: zapas nie jest wielki — dołożenie marki, R&D i giełdy do kroku (M10b–M10d) będzie go zjadać |
| DN-5 | **Zapis gry nie zyskuje nowej sekcji.** `MacroState` nie jest stanem trwałym: powstaje z `lift()` i ginie po `lower()`, a `MacroHandle` w ECS trzyma go wyłącznie jako zdjęcie kwartalne dla „co jeśli" (M7f). Historia „na sucho" zostawia po sobie **świat**, a nie model | M12b projektuje sekcje zapisu; ta pozycja mówi, że `sim/macro` żadnej nie potrzebuje i potrzebować nie będzie, dopóki `MacroHandle` jest zdjęciem |

---

## Zmiany wpisane po M10g

Zgodnie z `K-18`. Szczegóły — tabela `GG-n` w `M10g-panele-i-komendy.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| DO-1 ★ | **Format zapisu urósł o cztery komendy gracza, siedemnasty wariant `Subject` i dwa pola stanu.** `PlayerCommand` dostał `OpenCampaign`, `StartResearch`, `PlaceStockOrder` i `AnswerUnion` — **na końcu enuma**, jak każe nagłówek `game::command` — `Subject` wariant `Cover(CoverId)` (`K-90`), a do hasha stanu weszły `CampaignMetrics::by_district` i `Unions::player_offers` | Migracje zapisu z M12b muszą to unieść, a dziennik wejść replayu dostał cztery nowe kształty koperty. Kolejność wariantów jest w obu wypadkach liczbą w pliku, więc dopisywanie w środku pozostaje zakazane |
| DO-2 | **Panel z moda zrobi trzy z czterech zmian, których wymaga panel gry.** `PanelRegistry::register` przyjmuje `PanelDesc` z zewnątrz, plik i klucz tytułu są danymi — ale **wariant `PanelModel` jest enumem w `game/`** i mod do niego nie dopisze | Wyszło przy dokładaniu trzech paneli M10 tą samą drogą (`GG-4`). Adres rozstrzygnięcia jest w M12d razem z resztą moddingu: albo `PanelModel` dostaje wariant `Custom(Box<dyn Any>)`, albo panele z modów rysują się przez własny, węższy kontrakt. Dziś nie ma czym tego rozstrzygnąć, bo nie ma ani jednego moda |
| DO-3 ★ | **Tryb 50× dziedziczy naprawę licytacji płacowej z `GG-2` i to jest dla niego istotniejsze niż dla M10.** Sufit stawki liczony jako `stawka × 3` przesuwał się razem ze stawką, więc firma z trwale otwartym wakatem podnosiła płacę o dwa procent na dobę **bez końca**. W trzydziestu latach historii „na sucho" kosztowało to stukrotność obrotu miasta; w trybie 50× ta sama pętla chodzi po świecie, który gracz potem **rozgrywa dalej** | Historia „na sucho" i tryb 50× to ten sam silnik (`DN-2`), więc każdy defekt akumulacyjny w `step::labor` jest defektem M12c. Test, który by to złapał, jest jeden i tani: suma pieniądza przed i po, tolerancja 0 (K1) — `dry-run` ma go od M10a, `fast_forward` ma go dostać razem z sobą |
| DO-4 | **`macro::step` losuje od M10g i `StreamId::MacroStep = 292` jest zajęty** (`K-90` pkt 2): odejścia dobrowolne zaokrąglają się losowo, kluczem `(indeks firmy, tick)` | `K-78` zapisał ten numer jako „zarezerwowany i niezajęty", a M12c buduje na `step()` swój `fast_forward`. Determinizm nie zmienia się w niczym — klucz nie zależy od kolejności wywołań — ale przebieg, który do tej pory nie losował w tej fazie, teraz losuje, i replay musi to unieść tak samo jak każde inne losowanie |
