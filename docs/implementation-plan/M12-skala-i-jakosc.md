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
| Jakość | Sesja 100 lat headless, detekcja wycieków i dryfu ekonomicznego, rozbudowa balansatora o analizę trendu, benchmarki `criterion` w CI |

### Nie wchodzi

| Czego nie robimy | Kto to robi |
|---|---|
| Mechaniki symulacji (potrzeby, rynki, produkcja, HR, podatki) | M3–M10 — M12 je **konsumuje i optymalizuje**, nie zmienia semantyki |
| Renderer, greedy meshing, LOD wizualne, impostory, animacje | M1 / M11 — M12 tylko **wywołuje** przełącznik degradacji w trybie 50× i narzuca budżet pamięci na chunki |
| Panele UI, widgety, wykresy, edytor reguł | M9 / M11 — M12 dodaje wyłącznie panel pamięci (devtools), menedżer modów i warstwę i18n **pod** istniejącymi widgetami |
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

## 4. Pakiety robocze

Kolejność jest wiążąca w obrębie łańcuchów zależności; łańcuchy A, B, C, D są względem siebie niezależne i mogą iść równolegle.

```
Łańcuch A (pamięć):   WP1 → WP2 → WP3 ──────────────┐
Łańcuch B (zapis):    WP4 → WP5 → WP6 → WP7 → WP8   ├→ WP10 → WP14
Łańcuch C (modding):  WP11 → WP12 ──────────────────┤
Łańcuch D (reszta):   WP9, WP13 ────────────────────┘
```

### WP1 — Audyt i budżet pamięci jako test [M]
Zależności: brak (wymaga działającego M10).

Pomiar stanu faktycznego: `TrackingAllocator` z tagowaniem per podsystem (kompilowany w profilach `dev` i `bench`, wyłączony w `release`), zrzut rozbicia co dobę gry. Rejestr rozmiarów komponentów: tabela `(ComponentName, expected_size, expected_align)` w `engine/devtools/src/memory_budget.rs`, sprawdzana testem `component_sizes_are_budgeted()`.

**Kryterium ukończenia:** test CI zawodzi, gdy dowolny komponent urośnie ponad zadeklarowany rozmiar lub gdy dowolna pozycja budżetu z §5.2 zostanie przekroczona w scenariuszu 400 tys. To jest jedyny mechanizm, który nie pozwala budżetowi zgnić — reszta fazy zakłada, że on istnieje.

### WP2 — Redukcja komponentów, areny, pule [L]
Zależności: WP1 (bez pomiaru nie ma czego redukować).

Rozdzielenie gorące/zimne dla `Citizen`, `Household`, `Firm`, `Site`; usunięcie `String` i `Vec` z komponentów; areny dla encji krótkożyjących; `ScratchPool` per wątek.

**Kryterium ukończenia:** gorący wiersz mieszkańca ≤ 128 B (§5.3 punkt 5), ECS gorący ≤ 440 MB przy 400 tys., zero alokacji per tick w systemach oznaczonych `#[no_alloc]`, hash stanu niezmieniony względem wersji przed refaktorem (test regresji na 3 seedach × 100 tys. ticków).

### WP3 — Magazyn doświadczeń i kroniki na dysk [M]
Zależności: WP2.

`ExperienceStore` (ring 32 wpisów × 8 B per mieszkaniec, w arenie kolumnowej), `ChronicleStore` (append-only log zstd-framed + indeks odwrócony w RAM), rotacja kronik starszych niż 30 lat gry do rocznych podsumowań.

**Kryterium ukończenia:** zapytanie „pełna historia firmy X przez 100 lat" zwraca wynik < 50 ms z dysku; indeks kronik ≤ 64 MB po 100 latach gry; karta inspekcji mieszkańca (M3) i Kronika (M9) działają bez zmian w API.

### WP4 — `engine/io` w pełnej postaci [L]
Zależności: M0 (`Snapshotable`, `state_hash`).

`SaveHeader`, `SectionEntry`, `SectionId`, kompresja zstd per sekcja, klasyfikacja sekcji `Persisted` / `Derived`, integralność (xxh3 per sekcja).

**Kryterium ukończenia:** round-trip 400 tys. miasta: zapis → wczytanie → `state_hash` identyczny; rozmiar pliku w przedziale 50–300 MB; **z pauzą** czas zapisu < 5 s (WP5 usuwa pauzę, nie przyspiesza).

### WP5 — Zapis w tle (copy-on-write chunków ECS) [L]
Zależności: WP4, dostęp do granularności chunka w `engine/ecs` (patrz §9 D1).

Bariera zapisu na punkcie synchronizacji, atomowa podmiana wskaźnika chunka, arena cieni z twardym limitem, tryb „dogoń" przy wysyceniu.

**Kryterium ukończenia:** w trakcie zapisu 400 tys. miasta żaden tick symulacji nie przekracza 1.3× swojego mediany czasu sprzed zapisu; hash stanu zapisanego pliku == hash stanu w chwili bariery (a nie w chwili zakończenia zapisu); zapis pod obciążeniem 50× nie przekracza limitu areny cieni.

### WP6 — Migracje schematu [M]
Zależności: WP4.

`Migration`, `MigrationRegistry`, łańcuchowanie N→N+1, korpus zapisów „złotych".

**Kryterium ukończenia:** test `migrates_from_every_historical_schema()` — dla każdego zapisu w `tests/fixtures/saves/v*.mgs` wczytanie kończy się sukcesem i daje oczekiwany `state_hash` po migracji. Korpus rośnie z każdym bumpem `schema_version` — dodanie migracji bez dodania zapisu złotego jest błędem CI.

### WP7 — `EventJournal` i autozapisy [M]
Zależności: WP4, WP5.

Dziennik wejść gracza od snapshotu bazowego, autozapis hybrydowy (snapshot rzadko + dziennik często), rotacja slotów, integracja z „zatrzymaj przy zdarzeniu X" (§14.5).

**Kryterium ukończenia:** autozapis przyrostowy < 200 ms i < 2 MB; odtworzenie stanu ze snapshotu bazowego + dziennika daje `state_hash` identyczny ze stanem w chwili autozapisu; crash-safety — przerwanie procesu w dowolnym momencie zapisu zostawia poprzedni zapis nienaruszony (zapis do pliku tymczasowego + `rename`).

### WP8 — Replay i bramka determinizmu w CI [M]
Zależności: WP7.

`ReplayFile`, `tools/headless --replay`, checkpointy hashy, pipeline zgłoszeń błędów (przycisk „zgłoś błąd" pakuje replay + log + manifest modów).

**Kryterium ukończenia:** zadanie CI `determinism` na każdy PR: 2 przebiegi × 3 seedy × 50 tys. ticków → identyczne ciągi hashy; replay z korpusu regresyjnego (10 nagrań) odtwarza się bit-w-bit. Rozjazd w replayu raportuje **pierwszy** rozbieżny tick i nazwę systemu (bisekcja po hashach cząstkowych per archetyp).

### WP9 — Tryb 50×: budżet czasu ticka [L]
Zależności: M10 (`sim/macro`), M4 (`sim/traffic` — kalibracja tablic czasów przejazdu), M11 (przełączniki degradacji renderu).

**To jest pakiet budżetowy, nie pakiet przełącznikowy.** Sam suwak prędkości to godzina pracy; L wynika z tego, że M4 zmierzył 11 ms/tick dla ruchu mezo (15,8 s/dobę — 5× ponad cel §20.2), więc mezo nie wystarcza i cały budżet trzeba rozpisać na systemy i wyegzekwować (§5.6). Zakres: rozpisanie budżetu 2,08 ms/tick, przełączanie ruchu i agentów w makro na progu 50×, degradacja grafiki, `SpeedGovernor`, dowód spójności LOD.

**Kryteria ukończenia (trzy niezależne):**
1. **Budżet:** 1 doba gry ≤ 3 s dla 150 tys. na maszynie referencyjnej CI (D10), z rozbiciem per system mieszczącym się w tabeli §5.6. Każdy system ma własną bramkę `criterion` — przekroczenie budżetu przez jeden system to czerwone CI, nawet gdy suma jeszcze się mieści.
2. **Zachowanie:** 30 dni w 1× vs 50× — suma pieniądza **co do grosza**, suma masy każdego towaru **co do grama** (dok. 00 §4 po rozdzieleniu tolerancji).
3. **Brak dryfu:** agregaty **komórkowe i wyżej** (ceny koszyka, bezrobocie, produkcja per branża) odchylone ≤ 0,5% miesięcznie; nachylenie regresji różnicy w oknie 12 miesięcy nieodróżnialne od zera **i** autokorelacja znaku różnicy (lag 1) ≤ 0,3. Odchylenie ograniczone jest tolerowalne, dryf **nie** — bo się kumuluje przez 100 lat. Wielkości o małym n (pojedyncza firma, cienki rynek, pojedyncza marka) są **poza** tym kryterium — D14.

### WP10 — Skala 400 tys.: profilowanie i domknięcie budżetu [XL]
Zależności: WP2, WP3, WP5, WP9.

Iteracyjna pętla: profil (`tracy`/`puffin`) → najgrubsza pozycja → optymalizacja → pomiar. Realizacja drabiny cięć (§5.2) w zakresie potrzebnym do zmieszczenia się w 6 GB.

**Kryterium ukończenia:** miasto 400 tys. działa przez 5 lat gry ≤ 6 GB RSS bez trendu wzrostowego; scenariusz `metropolis_400k.ron` w repo jako punkt odniesienia dla wszystkich późniejszych benchmarków.

### WP11 — `engine/script`: host Lua i sandbox [XL]
Zależności: brak twardych; wymaga stabilnego API `sim/*` (po M10).

Host `mlua` (Lua 5.4), budowa `_ENV` z białej listy, typowane API kontekstu, bufor komend moda, `ScriptHook`, budżet instrukcji.

**Kryterium ukończenia:** przykładowy mod (`example-mod/`) realizuje wszystkie cztery klasy hooków z §18.3 PRD (nowe zdarzenie, polityka AI, nowy typ zakładu, panel UI); test `sandbox_escapes()` — 30 prób ucieczki z piaskownicy (lista w §5.7) kończy się kontrolowanym błędem, nie efektem ubocznym.

### WP12 — `ModManifest` i menedżer modów [L]
Zależności: WP11, WP4 (`ModStamp` w nagłówku zapisu).

Manifest, rozwiązywanie zależności i konfliktów, deterministyczna kolejność ładowania, menedżer w grze, **walidator determinizmu moda**.

**Kryterium ukończenia:** mod celowo niedeterministyczny (w korpusie testowym: wywołuje `pairs` po tablicy z kluczami-stringami i zapisuje kolejność do stanu) zostaje **odrzucony** przez walidator; mod poprawny przechodzi; wczytanie zapisu z inną listą modów daje jawne ostrzeżenie i oznaczenie sesji `ModsChanged`.

### WP13 — Lokalizacja PL/EN [M]
Zależności: M9/M11 (istniejące UI).

`LocaleCatalog`, pluralizacja CLDR (pl: one/few/many/other), przypadki gramatyczne dla nazw z `data/`, rodzaj gramatyczny w kronikach, przełączanie języka bez restartu.

**Kryterium ukończenia:** test `all_keys_present_in_all_locales()` i `no_unused_keys()` zielone; wektor pluralizacyjny CLDR dla pl i en przechodzi; zrzuty UI w pseudo-locale `x-long` bez przycięć tekstu.

### WP14 — Testy długich sesji, balansator, benchmarki [L]
Zależności: WP10.

Harness sesji 100-letniej, metryki degeneracji z progami (§7.4), balansator rozszerzony o regresję trendu w oknie 20-letnim, benchmarki `criterion` w CI z detekcją regresji.

**Kryterium ukończenia:** 5 seedów × 100 lat × miasto rosnące 150 tys. → 400 tys. bez przekroczenia żadnego progu alarmowego; raport HTML z przebiegu jako artefakt CI.

---

## 5. Projekt techniczny

### 5.1 Rozszerzenia `engine/core` i `engine/ecs`

M12 nie projektuje tych crate'ów (właściciel: M0), ale potrzebuje trzech dopisków — zgłoszonych jako decyzje otwarte D1–D3 w §9:

```rust
// engine/core — dopisek do StreamId.
// Zakres przydzielony M12 przez koordynatora: 320–339 (K-4).
// Nigdy nie zmieniamy istniejących wartości i nie wychodzimy poza swój zakres.
pub enum StreamId {
    // ... warianty M0–M11 ...
    Script       = 320,  // hooki modów; slot moda wchodzi jako entity_index
    SaveJitter   = 321,  // rozrzut terminów autozapisu (nie wpływa na stan symulacji)
    // 322–339 — rezerwa M12
}

// engine/core — rejestr schematów, potrzebny migracjom w M12,
// ale mieszkający tam, gdzie mieszkają typy
pub struct ComponentSchemaId(pub u16);      // stabilny, nigdy nie recyklowany
pub struct SchemaRegistry { /* ComponentSchemaId -> (nazwa, schema_version) */ }

// engine/ecs — M12 wymaga publicznej granularności chunka pod CoW
pub struct ChunkId(pub u32);
impl World {
    pub fn chunks(&self) -> impl Iterator<Item = ChunkId>;   // kolejność deterministyczna
    pub fn chunk_bytes(&self, id: ChunkId) -> &[u8];         // surowe kolumny SoA
}
```

### 5.2 Rachunek pamięci — metropolia 400 tys., budżet 6 GB (6144 MB)

**Encje ECS (stan gorący).** Liczności dla miasta 400 tys. przy strukturze demograficznej i gospodarczej z M3/M7:

| Typ encji | Liczba | B/encja (gorące) | Suma |
|---|---:|---:|---:|
| Mieszkaniec (`CitizenHot`) | 400 000 | 128 | 51 MB |
| Mieszkaniec (`CitizenCold`, w osobnym archetypie) | 400 000 | 272 | 109 MB |
| Gospodarstwo domowe | 160 000 | 256 | 41 MB |
| Firma | 20 000 | 1 024 | 20 MB |
| Zakład / sklep | 30 000 | 2 048 | 61 MB |
| Budynek | 120 000 | 192 | 23 MB |
| Parcela | 150 000 | 128 | 19 MB |
| Pojazd | 120 000 | 128 | 15 MB |
| Partia towaru | 400 000 | 64 | 26 MB |
| Oferta | 200 000 | 48 | 10 MB |
| Kontrakt | 50 000 | 128 | 6 MB |
| Linia komunikacji | 200 | 4 096 | 1 MB |
| Zdarzenie aktywne | 5 000 | 256 | 1 MB |
| **Suma komponentów** | | | **383 MB** |
| Narzut ECS (tablica encji, generacje, mapy archetypów, luzy w chunkach ~15%) | | | 57 MB |
| **ECS razem** | | | **440 MB** |

Mieszkaniec: 128 + 272 = 400 B, dokładnie budżet z PRD §17.7 — ale z kluczową różnicą, że gorąca część (czytana co tick przez planer dnia i decyzję zakupową) to 128 B, czyli dwie linie cache. `CitizenCold` (biografia, wykształcenie, relacje, identyfikatory imienia) jest czytana przy inspekcji i rzadkich decyzjach życiowych.

**Budżet całkowity procesu:**

| Pozycja | Budżet | Uwagi |
|---|---:|---|
| ECS gorący (tabela wyżej) | 440 MB | Twardy limit z testu WP1 |
| Magazyn doświadczeń (32 × 8 B × 400 tys.) | 102 MB | Nieskompresowany w RAM — patrz §5.3 |
| Kolejka zdarzeń DES (~4 oczekujące/agenta × 24 B) | 48 MB | |
| Nawigacja: CH dróg, graf pieszy, cache tras, tabele dzielnic | 96 MB | Sekcja `Derived`, nie zapisywana |
| Indeksy przestrzenne (grid agentów/pojazdów, quadtree parcel) | 64 MB | `Derived` |
| Indeksy rynkowe (oferty per kategoria × dzielnica) | 48 MB | `Derived` |
| Voxel: chunki rezydentne + palety | 1 400 MB | Limit rezydencji, reszta streamowana (M1) |
| Voxel: meshe CPU-side + bufory staging | 700 MB | M11 |
| Render: bufory instancji, tekstury nakładek po stronie RAM | 300 MB | |
| Snapshot podwójnie buforowany dla renderu | 120 MB | §17.1 |
| Arena cieni zapisu w tle (szczyt) | 280 MB | Twardy limit, §5.5 |
| Kroniki: indeks w RAM (treść na dysku) | 64 MB | Po 100 latach gry |
| UI: wirtualizowane tabele, wykresy, cache atlasu tekstu | 120 MB | |
| Dane statyczne (RON po załadowaniu, katalogi, katalogi lokalizacji) | 48 MB | |
| Skrypty modów (heap Lua, limit wymuszany) | 128 MB | §5.7 |
| **Suma podsystemów** | **3 958 MB** | |
| Fragmentacja alokatora (+12%) | 475 MB | Mierzona jako `RSS / live_bytes` |
| Rezerwa na szczyty (klęska, wybory, migracja ludności) | 500 MB | |
| **RAZEM** | **4 933 MB** | **Zapas do 6 144 MB: 1 211 MB (20%)** |

Zapas 20% nie jest luksusem — jest buforem na to, że żadna z powyższych liczb nie jest jeszcze zmierzona, tylko zaprojektowana. WP1 zamienia je w pomiar; WP10 zamyka różnicę.

**Drabina cięć — gdy budżet się nie mieści.** Kolejność jest wiążąca: ucinamy z góry, każdy szczebel z podanym zyskiem i kosztem. Nie negocjujemy kolejności w trakcie fazy.

| # | Cięcie | Zysk | Koszt |
|---|---|---:|---|
| 1 | Rezydencja voxela 1400 → 900 MB (agresywniejszy streaming, mniejszy promień pełnego LOD) | 500 MB | Dłuższe doładowania przy szybkim przelocie kamery. Bezbolesne — pierwsze w kolejce |
| 2 | Meshe CPU-side: zwolnienie po uploadzie na GPU, rekonstrukcja przy edycji | 400 MB | Edycja terenu wymaga remeshingu chunka (~5 ms). Akceptowalne |
| 3 | `CitizenCold` na dysk (mmap, LRU 100 MB) — czytana rzadko | 90 MB | Inspekcja mieszkańca +1 ms. Akceptowalne |
| 4 | Magazyn doświadczeń: 32 → 16 wpisów | 51 MB | Krótsza pamięć marki (§10 PRD). **Zmiana mechaniki — wymaga zgody M10** |
| 5 | Partie towaru: scalanie partii tego samego towaru, dostawcy i tygodnia | 15 MB | Zgrubniejsze śledzenie „od pola do półki" (§14.4). Widoczne dla gracza |
| 6 | Cache tras dom↔praca: 250 tys. → 100 tys. najczęstszych, reszta liczona na żądanie | 20 MB | +0.4 ms per przeliczenie trasy. Ryzyko dla budżetu ticka |
| 7 | Populacja docelowa 400 → 300 tys. | ~25% wszystkiego | **Porażka celu PRD §17.7.** Ostateczność, decyzja projektanta, nie inżyniera |

### 5.3 Redukcja rozmiaru komponentów — techniki (WP2)

1. **Rozdział gorące/zimne.** `CitizenHot` (128 B): pozycja LOD, wektor potrzeb `[Q; 8]`, budżet dobowy, miejsce pracy, GD, stan planu dnia, flagi. `CitizenCold` (272 B): biografia, wykształcenie, relacje, `(NameId, SurnameId)`, historia zatrudnienia. Osobny archetyp, wspólny `Entity` — dostęp przez ten sam id.
2. **Zero `String` w komponentach.** Imiona to `(u16, u16)` do katalogu; nazwy własne generowane z gramatyki na żądanie (M2) i cache'owane w UI, nie w ECS.
3. **Zero `Vec` per encja.** Listy (półki sklepu, załoga zakładu, pozycje kontraktu) jako `(offset: u32, len: u16)` do wspólnej areny per typ. `Vec` to 24 B nagłówka + osobna alokacja — przy 400 tys. encji to 10 MB nagłówków i 400 tys. alokacji do fragmentacji.
4. **Pola opcjonalne jako osobne komponenty.** `Option<Entity>` w gorącym wierszu to 8 B płaconych przez wszystkich; komponent-znacznik kosztuje 0 B w chunkach, w których go nie ma.
5. **Enumy `#[repr(u8)]`, flagi w bitfieldach.** 16 osobnych `bool` → jeden `u16`. `Q(u8)` i `Mood(i8)` z dok. 00 są już minimalne — pilnuje tego test rozmiarów.
6. **`MoneySmall(i32)`** dla kwot o udowodnionym zakresie (budżet dobowy GD, cena jednostkowa) — zakres ±21 mln zł. Konwersja do `Money(i64)` jawna, arytmetyka zawsze w `i64`. **Wymaga dopisku do dok. 00 §2 — decyzja otwarta D4.**
7. **Areny i pule.** `Arena<T>` (bump + free list) dla encji o wysokiej rotacji: partie, oferty, zdarzenia DES. `ScratchPool` per wątek dla buforów tymczasowych w systemach, resetowany na końcu ticka — zawartość nigdy nie wypływa poza system, więc per-wątkowość nie łamie determinizmu.
8. **Test rozmiarów jako bramka.** `#[test] fn component_sizes_are_budgeted()` porównuje `size_of::<T>()` z tabelą oczekiwań. Wzrost rozmiaru komponentu bez świadomej aktualizacji tabeli = czerwone CI. To jest jedyny powód, dla którego ten budżet przeżyje M12.

**Magazyn doświadczeń — decyzja: nie kompresujemy w RAM.**

```rust
#[repr(C)]
struct Experience { kind: u8, valence: i8, sim_day: u16, subject: u32 }  // 8 B
// ring 32 wpisów per mieszkaniec, w kolumnowej arenie po 1024 osoby
```

32 × 8 B × 400 tys. = 102 MB. PRD §17.7 mówi o „osobnym, kompresowanym magazynie" — realizujemy to **jako osobny magazyn**, ale kompresja zstd stosowana jest wyłącznie w dwóch miejscach, w których jest darmowa:

- **w zapisie** (sekcja `ExperienceStore` → zstd-3, ~25 MB na dysku),
- **dla agentów w LOD Makro** (uśpionych; ring zamrożony, niedostępny dla decyzji) — tam zstd daje ~4× i nie kosztuje nic, bo nikt tego nie czyta.

Kompresowanie magazynu dla agentów aktywnych byłoby błędem: pamięć doświadczeń jest czytana przy **każdej** decyzji zakupowej (M5) i przy ocenie marki (M10), więc dekompresja chunka trafiałaby w najgorętszą pętlę w grze. 102 MB to 1.7% budżetu — nie warto. Redukcja idzie przez rozmiar wpisu (8 B), nie przez entropię.

### 5.4 Format zapisu

```rust
pub struct SaveFile;   // wyłącznie format na dysku; w pamięci nigdy nie istnieje w całości

#[repr(C)]
pub struct SaveHeader {
    magic: [u8; 8],              // b"MAGNATSV"
    format_version: u16,         // wersja kontenera (nagłówek, sekcje) — zmienia się rzadko
    schema_version: u32,         // wersja schematu świata, monotoniczna; sterują nią migracje
    engine_build: [u8; 20],      // git hash builda — diagnostyka, nie kompatybilność
    world_seed: u64,
    tick: Tick,
    sim_minute: SimMinute,
    state_hash: u64,             // hash stanu w chwili BARIERY zapisu, nie zakończenia
    data_manifest_hash: u64,     // hash zawartości data/ — zmiana danych unieważnia replay
    mods: Vec<ModStamp>,         // id + wersja + checksum każdego aktywnego moda
    flags: SaveFlags,            // COMPRESSED_ZSTD | HAS_JOURNAL | AUTOSAVE | MODS_CHANGED
    section_count: u16,
    created_utc_unix: i64,       // METADANA — celowo NIE wchodzi do state_hash
}

pub struct SectionEntry {
    id: SectionId,
    schema_version: u16,         // wersja schematu TEJ sekcji; migracje działają per sekcja
    offset: u64,
    len_compressed: u64,
    len_raw: u64,
    checksum: u64,               // xxh3 po dekompresji
}

pub enum SectionId {
    EntityAllocator, Archetype(ComponentSchemaId), ExperienceStore, DesQueue,
    ChronicleIndex, PlayerState, CityState, MarketState, ModState, UiState,
}
```

**Klasyfikacja sekcji.** Każda sekcja jest `Persisted` albo `Derived`. `Derived` (CH, graf pieszy, cache tras, indeksy przestrzenne, indeksy rynkowe, meshe voxela) **nie jest zapisywana** — odtwarza się po wczytaniu. To ścina ~200 MB z pliku, ~1 s z czasu zapisu i, co ważniejsze, całą klasę migracji: struktura pochodna może zmienić się dowolnie między wersjami, bo nigdy nie trafia na dysk. Odtworzenie po wczytaniu: ~2 s dla 400 tys. na czterech rdzeniach, pokazywane jako część paska ładowania.

**Rozmiar i czas.** ECS 440 MB + doświadczenia 102 MB + DES 48 MB + stan miasta/rynków ~40 MB ≈ 630 MB surowego. zstd-3 na danych SoA (kolumny liczb całkowitych o niskiej entropii, świetnie się pakują): ~4–5× → **130–160 MB**. Mieści się w widełkach PRD §18.1 (50–300 MB): małe miasto 50 tys. daje ~25 MB, metropolia z kroniką w pliku ~280 MB.

Czas zapisu 400 tys.: kompresja 630 MB przy ~400 MB/s/rdzeń na 4 wątkach = **0.4 s**; zapis 150 MB na NVMe (500 MB/s) = **0.3 s**; serializacja i CoW = **0.5 s**. Razem **~1.2 s**, na dysku talerzowym (100 MB/s) **~2.4 s**. Cel §20.2 (< 5 s) spełniony z zapasem 2×.

### 5.5 Zapis w tle — copy-on-write chunków ECS

To jest najbardziej ryzykowny mechanizm w fazie, więc opisany jest dokładnie.

**Jednostka CoW:** chunk archetypu ECS — blok 64 KB kolumn SoA jednego archetypu (liczba encji = 64 KB / rozmiar wiersza; dla `CitizenHot` przy 128 B to 512 encji/chunk). Przy 400 tys. miasta całe gorące ECS to ~7 000 chunków.

**Kierunek kopiowania: sim kopiuje, wątek zapisu czyta oryginał.** Odwrotny kierunek (zapis czyta „żywą" pamięć, sim kopiuje stare wartości do cienia) wymagałby synchronizacji na każdym odczycie wątku zapisu. W wybranym wariancie wątek zapisu pracuje wyłącznie na pamięci, która od momentu bariery jest **niemutowalna** — zero wyścigów, zero blokad na ścieżce odczytu.

**Przebieg:**

1. **Bariera** (systemem `SaveBarrierSystem`, częstotliwość `EveryMinute`, wykonywany w punkcie synchronizacji po zaaplikowaniu buforów komend — czyli tam, gdzie dok. 00 §3.4 już gwarantuje spójność stanu). Bariera: (a) zwiększa `save_epoch`, (b) przechodzi po liście chunków i zapamiętuje ich bieżące wskaźniki w `SaveManifest`, (c) liczy `state_hash`. Koszt: ~7 000 wpisów po 16 B = jedno przejście liniowe, **< 1 ms**. To jedyny moment, w którym symulacja czeka — mieści się poniżej szumu jednego ticka.
2. **Okno zapisu.** Każdy zapis do chunka sprawdza `chunk.epoch != save_epoch`. Jeśli chunk nie był jeszcze dotknięty w tej epoce: alokacja nowego bloku 64 KB, `memcpy` starej zawartości, ustawienie `epoch`, **atomowa podmiana wskaźnika** w tablicy chunków świata (`AtomicPtr`, `Release`). Stary blok jest trzymany przez `Arc` w `SaveManifest`, więc nie znika pod wątkiem zapisu. Sim od tej chwili pisze do nowego bloku.
3. **Mutacje strukturalne.** Tworzenie encji po barierze alokuje nowe chunki — nie ma ich w `SaveManifest`, więc nie trafiają do zapisu (poprawnie: zapis reprezentuje stan z chwili bariery). Usuwanie (swap-remove w chunku) to zwykła mutacja → zwykły CoW. Żadnej dodatkowej ścieżki.
4. **Wątek zapisu** iteruje po `SaveManifest` w kolejności `SectionId`, kompresuje chunk po chunku (4 wątki `rayon`, składanie deterministyczne po indeksie), zapisuje do pliku tymczasowego. Po wysłaniu chunka zwalnia jego `Arc` — cień znika natychmiast, nie po zakończeniu całego zapisu.
5. **Zamknięcie:** `fsync`, zapis nagłówka z tabelą sekcji, `rename` na docelową nazwę (atomowe na NTFS i ext4 — crash w trakcie zostawia poprzedni zapis nienaruszony). Wyczyszczenie `save_epoch`.

**Narzut czasu.** Kopia jednego chunka 64 KB przy ~15 GB/s: **~4 µs**. Pesymistycznie w oknie 1.2 s dotknięte zostaną wszystkie 7 000 chunków → **28 ms łącznego CPU rozłożone na ~600 ticków** przy prędkości 1× (0.05 ms/tick, poniżej 3% budżetu ticka) i na ~10 ticków przy 50× (2.8 ms/tick — **to boli**, dlatego `SpeedGovernor` obniża prędkość do 10× na czas zapisu w tle; alternatywą byłoby przekroczenie budżetu 2 ms/tick z §5.6). Narzut jest jednorazowy per chunk per epoka — drugi i kolejne zapisy do tego samego chunka są darmowe.

**Narzut pamięci.** Szczyt = suma cieni chunków dotkniętych, ale jeszcze nieprzetworzonych przez wątek zapisu. Ponieważ wątek zapisu zwalnia cienie na bieżąco, a kompresja (0.4 s) jest szybsza niż pesymistyczne dotknięcie wszystkich chunków, realny szczyt to **80–150 MB**. Budżet: **280 MB twardego limitu** areny cieni.

**Wysycenie limitu — tryb „dogoń":** przy 80% limitu `SpeedGovernor` obniża prędkość gry o jeden szczebel i wątek zapisu dostaje wyższy priorytet. Przy 100% sim **zatrzymuje się na najbliższej barierze** do zwolnienia 20% areny. To degradacja (gracz widzi krótką pauzę), nie awaria — i jest metryką alarmową w testach (§7.4). Jedyny scenariusz, w którym to wystąpi, to zapis podczas 50× na dysku talerzowym.

### 5.6 Tryb 50×: budżet czasu ticka rozpisany na systemy

**Budżet.** Doba = 1440 ticków ekonomicznych. Cel §20.2: 1 doba ≤ 3 s dla 150 tys. → **480 ticków/s → 2,08 ms/tick na wszystko**.

**Punkt wyjścia: mezo nie wystarcza.** M4 zmierzył ruch mezo na **11 ms/tick**, czyli 15,8 s na dobę gry — sam ruch przekracza cały budżet 5×. To nie jest problem do rozwiązania optymalizacją stałego czynnika; to problem jednostki pracy. Przy 150 tys. mieszkańców każdy system, który przy 50× **iteruje per agent lub per pojazd**, kosztuje 1,5 ms nawet przy nierealistycznych 10 ns na encję — czyli 72% budżetu na jeden system. Wniosek jest jednoznaczny: **przy 50× jednostką pracy musi przestać być encja.**

Jednostki pracy przy 50× dla miasta 150 tys.:

| Jednostka | Liczność (150 tys.) | Zastępuje |
|---|---:|---|
| `MacroCell` (dzielnica × klasa) | **180** (30 dzielnic × 6 klas); zakres **30–240** w całym §4.1 | 150 000 mieszkańców |
| Podróż rozpoczęta w tym ticku | ~420 | ~60 000 pojazdów w ruchu |
| Zakład | ~11 000, ale na ticku `EveryHour` → ~180/tick amortyzowanych | ~120 000 maszyn |
| Para (kategoria × dzielnica) na rynku | ~4 000 | ~200 000 ofert |

**Ziarno agregacji jest podporządkowane tolerancji, nie budżetowi czasu — i nie jest stałą.** Droga do tej liczby jest pouczająca, więc zostaje udokumentowana. Pierwsza wersja tego dokumentu zakładała ~2 400 kohort (200 dzielnic × 12 klas) — obie liczby zgadywane. Było to błędne o dwa rzędy: przy 2 400 komórkach na komórkę przypada 62 osoby, a wariancja rozkładu wielomianowego przy n = 62 daje odchylenie kilku procent, więc kontrakt tolerancji 0,5% pękał ze statystyki, niezależnie od jakości implementacji.

M10 podał 240 (40 × 6) i miał rację dla metropolii. Po przyłożeniu tej samej reguły do **wszystkich** rozmiarów miast z PRD §4.1 okazało się jednak, że stałe ziarno `dzielnica × 6 klas` przechodzi na dużym mieście i **cicho pada na małym**:

| Rozmiar | Populacja | Dzielnic | Komórek (×6) | Osób/komórkę | Tolerancja 0,5%? |
|---|---:|---:|---:|---:|---|
| małe | 20 000 | 10 | 60 | **333** | **nie** |
| małe (górny) | 40 000 | 15 | 90 | **444** | **nie** |
| średnie | 60 000 | 20 | 120 | 500 | granica |
| duże | 150 000 | 30 | 180 | 833 | tak |
| metropolia | 400 000 | 40 | 240 | 1 667 | tak |

Kontrakt łamał się więc dokładnie tam, gdzie nikt nie patrzył — u gracza, który wybrał małe miasto. Rozwiązanie (M10): ziarno klas jest funkcją rozmiaru, z twardym progiem `MIN_CELL_POP = 500`:

```rust
pub const MIN_CELL_POP: u32 = 500;
pub fn cell_grain(pop: u32, districts: u16) -> ClassGrain;   // Classes6 | Classes3 | Classes2
```

Klasy łączą się parami po **sąsiadujących** przedziałach statusu, nigdy losowo (`Classes3` = niższa+robotnicza, niższa średnia+wyższa średnia, wyższa+elita). Dla 20 tys.: 10 × 3 = 30 komórek, 666 osób/komórkę — kontrakt wraca do zakresu. Odwzorowanie klasy na komórkę jest funkcją czystą, więc `lift()` i `lower_cell()` nie zmieniają się wcale.

**Konsekwencja dla M12, i jest to jedyna rzecz, o którą M10 poprosił:** liczba komórek **nigdzie nie może być stałą**. Wszystkie budżety, liczniki i asercje odwołują się do `state.cells.len()`, nie do 180 ani 240. Pułapka jest cicha — stała 240 działałaby poprawnie na wszystkich scenariuszach testowych M12 (które są duże) i złamałaby się dopiero przy pierwszym profilu na małym mieście. Pilnuje tego test `no_hardcoded_cell_count` (§7.3).

Spadek ze 150 000 do 180 jednostek pracy (833×) jest tym, co czyni cel osiągalnym z zapasem. Ruch: czas przejazdu z tablic dzielnica×dzielnica×godzina (PRD §17.6) — **wzór i kalibracja należą do M4 (`sim/traffic::travel_time`)**, a `sim/macro` trzyma tylko ich snapshot (`MacroState.commute: CommuteMatrix`) z chwili przejścia w makro. Nie ma kolejek na krawędziach, car-following ani pojazdów.

**Budżet per system (150 tys., prędkość 50×, maszyna referencyjna D10):**

| System | Budżet | Jednostka pracy przy 50× | Uzasadnienie liczby |
|---|---:|---|---|
| Ruch makro | 0,55 ms | ~420 podróży/tick | Lookup w `CommuteMatrix` + losowanie z rozkładu ≈ 1 µs/podróż → 0,42 ms, zapas 30%. Budżet uzgodniony z M10 |
| Agenci (DES makro) | 0,15 ms | **`cells.len()` = 180** | Zdarzenia per komórka, nie per agent; ~830 ns/komórka |
| Ekonomia (rynki, transakcje, ceny) | 0,45 ms | ~4 000 par kategoria × dzielnica | Transakcje agregowane per komórka × kategoria |
| Firmy i produkcja | 0,25 ms | ~180 zakładów/tick | Produkcja per zakład (§17.4), na ticku godzinowym |
| Miasto, zdarzenia, kroniki | 0,10 ms | dzielnice, zdarzenia aktywne | Częstotliwości `EveryDay`/`EveryMonth` |
| Scheduler, bufory komend, punkty synchronizacji | 0,10 ms | ~180 wpisów komend | Sortowanie po `(SystemId, entity_index)` |
| **Rezerwa** | **0,48 ms** | — | 23% budżetu |
| **RAZEM** | **2,08 ms** | | **= 3,0 s/dobę** |

Budżet jest liczony dla **180 komórek** (150 tys. — rozmiar, dla którego PRD §20.2 definiuje cel). Na metropolii komórek jest 240, więc wiersz „agenci" rośnie o 33% (0,15 → 0,20 ms); mieści się w rezerwie i nie dotyczy celu 3 s/dobę, który jest zdefiniowany dla 150 tys. Na małym mieście komórek jest 30 i koszt spada sześciokrotnie — skalowanie idzie **wyłącznie w dół**, nigdy w górę.

Rezerwa urosła z 0,08 do 0,48 ms wyłącznie dzięki korekcie ziarna. To nie jest zysk z optymalizacji — to zysk z tego, że poprzednia liczba była zgadnięta źle. Traktujemy ją jako margines na to, że pozostałe wiersze też są zgadnięte.

**Egzekwowanie.** Każdy wiersz tej tabeli dostaje własny benchmark `criterion` i własną bramkę CI. Przekroczenie budżetu przez pojedynczy system jest czerwone **nawet wtedy, gdy suma jeszcze się mieści** — inaczej pierwszy system, który przekroczy, zje rezerwę kolejnych i winowajcy nie da się wskazać. Budżety są własnością M12, ale realizacja każdego wiersza jest własnością fazy, która ten system napisała; M12 mierzy, raportuje i eskaluje.

**Przełączniki na progach prędkości:**

| Prędkość | Ruch | Agenci | Render |
|---|---|---|---|
| 1× / 3× | mikro w kadrze, mezo poza | DES pełny | pełny |
| 10× | mikro tylko dla śledzonych, reszta mezo | DES pełny | bez wnętrz, LOD +1 |
| **50×** | **makro wszędzie — tablice czasów przejazdu, zero pojazdów** | **makro: agregaty per dzielnica × klasa, tożsamości zachowane (stan uśpiony)** | **bez animacji ludzi, LOD +2, 1 kaskada cienia, bez cząstek, nakładki odświeżane co 1 s realnego** |

Przejście makro→mezo odtwarza indywidualne stany z zachowanych tożsamości, deterministycznie z seeda i tożsamości (PRD §17.4). Kontrakt uzgodniony z M10 (D11 zamknięte):

```rust
pub fn lower_cell(cell: &MacroCell, seed: u64, tick: Tick) -> CellExpansion;
```

Funkcja czysta: bez dostępu do `World`, bez stanu ukrytego. `MacroCell` niesie `Vec<CitizenSeed>` — nośnik tożsamości, bez którego „deterministycznie z seeda i tożsamości" ma tylko jeden człon. `lower()` jest wyłącznie sterownikiem wołającym ją per komórka. M12 nalegał na czystość tej sygnatury, bo to jedyny punkt, w którym replay i zapis mogą się rozjechać niezauważalnie — po obu stronach test jest osobny (K4 u M10, `replay_corpus` u M12).

**Kontrakt spójności po rozdzieleniu tolerancji (dok. 00 §4).** Tolerancja nie jest już jedna — i nie jest też jedna w obrębie makro:

| Przejście / wielkość | Tolerancja |
|---|---|
| mikro ↔ mezo | **0** — bit w bit, bez wyjątków |
| Suma pieniądza w systemie (każdy LOD) | **0 — co do grosza** |
| Suma masy każdego towaru (każdy LOD) | **0 — co do grama** |
| Agregat komórkowy i wyżej (`MIN_CELL_POP` ≥ 500 osób lub ≥ 10 firm) | ≤ 0,5% miesięcznie, **bez dryfu** |
| Przychód pojedynczej firmy | **3–12%** — poza kontraktem 0,5% |
| Cena towaru przy < 3 dostawcach | **2–8%** — poza kontraktem 0,5% |
| Udział rynkowy pojedynczej marki | **2–5%** — poza kontraktem 0,5% |

Trzy ostatnie wiersze to ustalenie M10 i **nie są usterką do naprawienia**: to wariancja rozkładu wielomianowego przy małym n (przy 200 klientach odchylenie udziału to ~3,5%), której żaden wspólny kernel ekonomiczny nie zdejmie. Konsekwencja dla M12 jest konkretna i wchodzi do obietnicy produktu: **tryb 50× przewiduje stan dzielnicy i miasta, nie los pojedynczej firmy.** Nigdzie w M12 nie wolno oprzeć się na założeniu, że po przebiegu w 50× konkretna firma jest w konkretnym stanie z dokładnością 0,5% — dotyczy to w szczególności „co jeśli" AI, autozapisu przed decyzją i wszelkich porównań przebiegów. Zapisane jako D14.

Rozróżnienie dokładności od zachowania nie jest złagodzeniem wymagania, tylko jego doprecyzowaniem, i M10 sformułował je czyściej, niż robił to ten dokument: **dokładność mówi, komu przypadł pieniądz; zachowanie mówi, ile go jest.** Makro zgaduje pierwsze, nie zgaduje drugiego — każdy przepływ idzie przez `ledger_post()` (zapis dwustronny, ten sam co w mezo), a podział agregatu domyka się korektą reszty do pierwszego wg posortowanego klucza (dok. 00 §2). To, a nie równość sald, jest prawdziwym kontraktem.

**Dryf jest groźniejszy od odchylenia.** 0,4% w losową stronę co miesiąc jest nieszkodliwe; 0,4% w *tę samą* stronę to po 100 latach czynnik ~120× i świat, który się rozpadł. Dlatego sam próg na nachylenie regresji nie wystarcza — da się go przypadkiem przejść na krótkiej próbce. Za M10 dokładamy **kryterium autokorelacji znaku różnicy (lag 1 ≤ 0,3)** do testu 100-letniego (§7.4).

**`SpeedGovernor`** (poza tickiem symulacji, w pętli gry): mierzy czas ticka w oknie kroczącym 30 ticków. Jeśli p50 przekroczy budżet prędkości, **automatycznie obniża prędkość o szczebel** i wyświetla komunikat („Miasto jest za duże na 50× — 10×"). Spójny wolniejszy bieg jest lepszy od szarpanego szybszego. Governor jest też mechanizmem degradacji przy wysyceniu areny cieni (§5.5). Wyłączalny w headless (tam liczymy czas, nie płynność).

Governor **nie decyduje sam o przejściu LOD** — pyta `MacroLodPolicy` (M10):

```rust
pub fn target_lod(&self, speed: GameSpeed, load: &LoadStats) -> Lod;
pub fn may_transition(&self, from: Lod, to: Lod, world: &World) -> Option<BlockReason>;
```

`may_transition` jest wiążące: wejście w makro musi być zablokowane w trakcie fixingu giełdowego, rundy negocjacji związkowych i trwającego strajku — te stany zginęłyby przy agregacji. Governor pokazuje wtedy graczowi powód („50× dostępne po zakończeniu fixingu"), zamiast cicho zignorować suwak. **Histereza progów jest po stronie M10**, żeby governor nie oscylował — M12 nie implementuje własnej.

**Jeśli budżet nie domknie się mimo makro** — drabina cięć celu 3 s/dobę (szczegóły: D12):

1. Ekonomia na ticku godzinowym zamiast minutowego w trybie makro. Zysk ~0,35 ms. Koszt: ceny reagują w oknie godziny; §20.1 wymaga reakcji na szok w 2–7 **dni**, więc mieści się.
2. Cel rozluźniony do **≤ 4 s/dobę** — zmiana §20.2, decyzja projektanta. 1 rok gry w 50× to wtedy 24 min zamiast 18.
3. Cel definiowany dla 100 tys., nie 150 tys. — najgorsza opcja, bo zmienia obietnicę PRD w miejscu, w którym jest ona mierzalna i konkretna.

**Ziarno jako dźwignia — ograniczona i asymetryczna.** Szczebel „kohorty grubsze", obecny w pierwszej wersji tej drabiny jako 12 → 6 klas, nie zniknął, ale skurczył się i zmienił charakter. Przy `MIN_CELL_POP = 500` miasto 150 tys. ma zapas: `Classes3` daje 90 komórek po 1 666 osób, więc wciąż spełnia tolerancję. Zysk jest jednak **marginalny (~0,08 ms)**, bo ziarno klas wpływa tylko na wiersz „agenci" (0,15 ms) — ekonomia liczy się per kategoria × dzielnica i jest na nie obojętna. To dźwignia warta odnotowania, nie warta planowania.

**W drugą stronę drogi nie ma wcale.** Zagęszczanie komórek w celu poprawy rozdzielczości łamie tolerancję, i to najpierw na małych miastach — tam zapasu nad progiem 500 nie ma żadnego. Ziarno jest ograniczone z dołu przez statystykę (`MIN_CELL_POP`) i z góry przez sens; między tymi granicami zapas zależy od rozmiaru miasta i na 20 tys. wynosi zero.

Realny margines: rezerwa 0,48 ms (23%) plus szczebel 1 — razem ~0,83 ms, czyli 40% budżetu.

### 5.7 Modding: `engine/script`

**Wybór hosta: Lua 5.4 przez `mlua`.** WASM/`wasmtime` jest **odroczony** — dwa hosty to dwie powierzchnie sandboxa do audytu i dwa zestawy dowodów determinizmu, przy jednej korzyści (wydajność modów, której nikt jeszcze nie potrzebuje). API kontekstu jest projektowane jako czysty zestaw funkcji hosta, więc dołożenie backendu `wasmtime` później nie wymaga zmiany kontraktu z modderem. Lua wygrywa progiem wejścia, a modding bez modderów jest martwy.

```rust
pub struct ModManifest {
    id: ModId,                     // odwrotna domena, np. "pl.kowalski.piekarnie"
    name: LocalizedText,
    version: Version,              // semver
    engine_api: VersionReq,        // kompatybilność z API skryptowym
    data_schema: u32,              // wersja schematu data/
    requires: Vec<(ModId, VersionReq)>,
    conflicts: Vec<ModId>,
    load_order_hint: i32,          // rozstrzyganie remisów; ostateczna kolejność jest deterministyczna
    data_dirs: Vec<RelPath>,       // wyłącznie względne do katalogu moda
    scripts: Vec<ScriptEntry>,     // plik + deklarowane hooki
    permissions: ModPermissions,   // deklaratywne, zawsze podzbiór sandboxa
    checksum: u64,                 // xxh3 wszystkich plików moda — trafia do SaveHeader.mods
    determinism_verified: bool,    // ustawiane przez walidator, nie przez autora
}

pub enum ScriptHook {
    OnTick(Frequency),              // EveryDay/EveryMonth domyślnie; EveryHour/EveryMinute za permisją
    OnEventFired(EventKindId),
    OnFirmPolicyDecision,           // zwraca DecisionOverride + DecisionReason (dok. 00 §7)
    OnSiteProduce,
    OnAgentPurchaseScore,           // modyfikator użyteczności zakupu
    OnCityPolicyVote,
    OnWorldgenPostPass,
    OnUiPanel(PanelSlot),           // wyłącznie wątek UI; NIE MOŻE dotknąć stanu symulacji
}
```

**Sandbox — operacje dozwolone:**

- odczyt plików **wyłącznie** z `mods/<id>/` przez wirtualny FS montowany read-only (ścieżki kanonizowane, `..` i ścieżki absolutne odrzucane przed dotknięciem systemu plików);
- zapis **wyłącznie** do `mods/<id>/state/` (ustawienia moda), limit 1 MB, format RON, jeden plik — i **nigdy z wnętrza hooka symulacyjnego**, tylko z hooka UI;
- odczyt stanu symulacji przez typowane API kontekstu (`ctx:firm(id):cash()`, `ctx:good_price(good, district)`), zawsze zwracające dane posortowane po id;
- mutacje **wyłącznie** przez bufor komend hosta (`ctx:cmd():set_price(site, good, money)`) — ten sam mechanizm, którym mutują systemy silnika, sortowany po `(SystemId, entity_index)` zgodnie z dok. 00 §3.4; mod dostaje własny, stały `SystemId` z pozycją wynikającą z kolejności ładowania;
- losowość **wyłącznie** przez `ctx:rng()` → `rng(world_seed, StreamId::Script, mod_slot << 24 | entity_index, tick)` — slot moda wchodzi do indeksu, żeby dwa mody nigdy nie dzieliły strumienia;
- czas **wyłącznie** gry: `ctx:sim_minute()`, `ctx:tick()`, `ctx:date()`;
- logowanie do konsoli gry, teksty przez `ctx:text(key, args)` (pełna lokalizacja, §5.8);
- alokacja w obrębie limitu heapu moda (domyślnie 16 MB, twardy sufit sumaryczny 128 MB).

**Sandbox — operacje zakazane.** Zakaz jest egzekwowany przez **brak funkcji w środowisku**, nie przez konwencję ani skanowanie źródeł:

| Zakazane | Mechanizm |
|---|---|
| Jakiekolwiek IO poza `mods/<id>/` | `io` i `os` nieobecne w `_ENV`; wirtualny FS kanonizuje i odrzuca |
| Sieć | Brak jakiegokolwiek API sieciowego w hoście. Kropka |
| Zegar rzeczywisty (`os.time`, `os.clock`, `os.date`) | `os` nieobecne; odpowiedniki w `ctx` zwracają czas **gry** |
| `os.execute`, `io.popen` | `os`, `io` nieobecne |
| `require`, `dofile`, `loadfile`, `load`, `loadstring` | Usunięte z `_ENV`; ładowanie modułów wyłącznie przez host, z listy w manifeście |
| `package.loadlib`, FFI, biblioteki natywne | `package` nieobecne |
| `debug.*` | Usunięte z `_ENV` **po** zainstalowaniu przez host licznika instrukcji |
| Wątki, wyjście z hooka przez `coroutine.yield` | `coroutine` dostępne wewnątrz hooka, ale yield przez granicę hosta = błąd |
| Iteracja po niedeterministycznej kolekcji | API nie eksponuje żadnej mapy hash; `pairs` podmienione na wariant sortujący klucze |
| Float na ścieżce pieniądza | API `Money`/`Qty` przyjmuje wyłącznie `math.type(x) == "integer"`, resztę odrzuca błędem |
| Mutacja stanu z hooka UI | `OnUiPanel` dostaje kontekst tylko do odczytu — `ctx:cmd()` nie istnieje w tym wariancie |

**Wymuszanie determinizmu — pięć mechanizmów.** Wymóg „mod nie może złamać determinizmu" nie jest spełniany jednym trikiem:

1. **Środowisko bez źródeł niedeterminizmu.** `_ENV` budowane z białej listy, nie przez usuwanie z domyślnego. `math.random` i `math.randomseed` zastąpione przekierowaniem do `ctx:rng()`. `pairs` zastąpione wariantem sortującym klucze — kolejność `pairs` w Lua zależy od hashowania stringów i **nie jest stabilna między uruchomieniami**; to najczęstsze źródło niedeterminizmu w modach i najłatwiejsze do przeoczenia. `table.sort` udokumentowany jako niestabilny — API dostarcza `ctx:sort_stable()`.
2. **Budżet instrukcji, nie czasu.** Limit to N wykonanych instrukcji VM (Lua: licznikowy `debug.sethook` instalowany przez host **zanim** `debug` zniknie z `_ENV`). Przekroczenie → hook przerwany, jego bufor komend odrzucony w całości, mod oznaczony `Faulted`, komunikat w konsoli. Kluczowe: liczba instrukcji jest **deterministyczna**, czas ścienny nie — limit czasowy złamałby determinizm dokładnie tam, gdzie miał go chronić. Domyślnie 200 tys. instrukcji na hook, 2 mln na tick sumarycznie.
3. **Pieniądz i ilości tylko całkowitoliczbowo.** Lua 5.4 ma podtyp integer; API pieniądza waliduje `math.type` i odrzuca float. Zgodne z dok. 00 §2 („nigdy f32/f64 w pieniądzu").
4. **Odcisk konfiguracji modów w zapisie.** `SaveHeader.mods: Vec<ModStamp { id, version, checksum }>`. Wczytanie zapisu przy innej liście lub innych checksumach → flaga `MODS_CHANGED`, ostrzeżenie dla gracza, unieważnienie replayu i wyników rankingowych. Bez tego determinizm jest nieweryfikowalny, bo nie wiadomo, co właściwie liczyło.
5. **Walidator determinizmu jako bramka instalacji.** Menedżer modów uruchamia headless: 2 przebiegi × 5 000 ticków na małym mieście z tym samym seedem → porównanie ciągów hashy. Wynik zapisuje `determinism_verified`. Mod, który nie przejdzie, **nie jest blokowany** — jest degradowany: działa w piaskownicy, jest niedostępny w kariery i w rankingach, i mówi o tym wprost w menedżerze. Blokada zniechęciłaby modderów; degradacja chroni graczy.

Kolejność wykonania hooków: sortowana po `(HookKind, load_order, mod_id)` — nigdy po kolejności zwróconej przez system plików.

**Menedżer modów w grze:** lista zainstalowanych z ich statusem determinizmu, rozwiązywanie zależności i konfliktów, przeciąganie kolejności ładowania (zapisywanej jawnie, nie wyliczanej), przycisk „sprawdź determinizm" uruchamiający walidator, ostrzeżenie przed wczytaniem zapisu z inną konfiguracją. Warsztat online — poza zakresem M12 (to infrastruktura, nie gra).

### 5.8 Lokalizacja

```rust
pub struct LocaleCatalog {
    locale: LocaleId,                       // "pl-PL" | "en-US"
    plural_rule: fn(i64) -> PluralCategory, // reguła CLDR
    entries: BTreeMap<TextKey, MessageTemplate>,
    fallback: Option<Arc<LocaleCatalog>>,   // brakujący klucz → en-US → sam klucz (widoczny w UI)
}

pub struct TextKey(pub u32);                // internowany, stabilny hash klucza tekstowego
pub enum PluralCategory { One, Few, Many, Other }   // pl używa wszystkich czterech, en dwóch
pub enum Case { Nom, Gen, Dat, Acc, Inst, Loc }     // ignorowane w en
pub enum Gender { Masc, Fem, Neut }
```

Trzy rzeczy, które odróżniają to od „przetłumaczymy stringi":

1. **Pluralizacja CLDR, nie `if n == 1`.** PL: `1 sklep` (one), `2–4 sklepy` (few), `5+ sklepów` (many), `1,5 sklepu` (other). EN: one/other. Szablon: `{count, plural, one {# sklep} few {# sklepy} many {# sklepów} other {# sklepu}}`.
2. **Przypadki dla nazw z `data/`.** PL wymaga odmiany: „Brak: chleb" vs „Kupiono chleba". Katalogi (`data/goods/*.ron`, `data/buildings/*.ron`) dostają pole `name` z formami przypadków; szablon deklaruje żądany: `{good:gen}`. Dla EN pola przypadków są ignorowane — jedna forma plus liczba mnoga. To rozszerzenie schematu `data/` i wymaga bumpa `schema_version` tych katalogów.
3. **Rodzaj gramatyczny w kronikach.** „Anna otworzyła piekarnię" vs „Jan otworzył piekarnię". `Gender` w katalogu imion (M2/M3), szablon: `{actor:gender, select, fem {otworzyła} other {otworzył}}`.

Przełączanie języka bez restartu: `LocaleCatalog` za `Arc`, zmiana unieważnia cache tekstu w UI (dirty-flag całego drzewa widgetów). Kroniki przechowują `TextKey` + argumenty, nie gotowy tekst — dlatego zapis z polskiej sesji otwarty po angielsku ma angielską kronikę. Wymaga, by M9 zapisywało kroniki strukturalnie, co i tak wynika z dok. 00 §7 (`DecisionReason` jako enum, nie string).

### 5.9 Systemy ECS dodawane przez M12

| System | Częstotliwość | Odczyt / zapis | Rola |
|---|---|---|---|
| `SaveBarrierSystem` | `EveryMinute` | cały świat (R), `save_epoch` (W) | Sprawdza żądanie zapisu, stawia barierę, buduje `SaveManifest` |
| `JournalAppendSystem` | `EveryMinute` | bufor wejść gracza (R), dziennik (W) | Dopisuje `JournalEntry` po aplikacji komend |
| `AutosaveTriggerSystem` | `EveryHour` | zegar gry, polityka autozapisu | Decyduje: snapshot bazowy czy przyrost dziennika |
| `ChronicleFlushSystem` | `EveryHour` | bufor kroniki (R/W) | Wypycha blok 4 MB na dysk, aktualizuje indeks |
| `ScriptHookDispatchSystem` | `EveryDay` (domyślnie) | wg deklaracji hooka | Wykonuje hooki modów w kolejności `(HookKind, load_order, mod_id)`, zbiera komendy |
| `MemoryProbeSystem` | `EveryDay` | liczniki alokatora, liczby encji | Metryki degeneracji; w `release` tylko RSS i liczby encji |
| `SpeedGovernorSystem` | poza tickiem (pętla gry) | historia czasów ticka | Watchdog budżetu, degradacja prędkości |

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
