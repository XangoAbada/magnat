# M12a — Pamięć

Podfaza 1 z 5 fazy **M12 — Skala i jakość** (`M12-skala-i-jakosc.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | Działający M10 (i M11 dla renderowej części budżetu). |
| **Pakiety robocze** | WP1, WP2, WP3, **WP4 (przejęty z `M11e`, warunkowy — `DM-1`)** |
| **Projekt techniczny** | §5.1, §5.2, §5.3 |
| **Wynik do pokazania** | Panel `F3 → Pamięć` pokazuje rozbicie per podsystem z porównaniem do budżetu; przekroczenie świeci na czerwono. |
| **Kryterium zamknięcia** | Kryteria WP1–WP3; gorący wiersz mieszkańca ≤ 128 B, hash stanu niezmieniony po refaktorze. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M12b-zapis-i-replay.md` |

Audyt i budżet pamięci jako test CI, redukcja rozmiaru komponentów (gorące/zimne, areny, pule) oraz wyniesienie doświadczeń i kronik na dysk.

---

## Pakiety robocze

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

### WP4 — Impostory dzielnic [M] — **warunkowy**
Zależności: WP1 (budżet pamięci musi istnieć, zanim dołożymy do niego 192 MB VRAM).

Przejęty z `M11e` (`H-10`). Atlas kafli 128×128 m generowany w runtime z tego, co gracz zbudował:
8 azymutów, kafel 128×128 px (RGBA8 + R16 depth), budżet **192 MB VRAM → 244 bloki rezydentne**
z LRU, regeneracja **≤ 4 bloki na klatkę** w osobnym passie z budżetem 1,5 ms i kolejką po
odległości, inwalidacja dwustopniowa, degradacja do chunków LOD 8×. Projekt techniczny w całości:
`M11b-animacja-i-lod.md` §5.6 — pakiet zmienił adres, nie treść.

**Warunek wejścia — liczba, nie wrażenie.** Pakiet powstaje **wyłącznie wtedy**, gdy scena
`bench_city` przekroczy **60 % swojego progu** na maszynie docelowej albo w trybie 50× (`M12c`).
Stan po M11e: 5,76 ms p95 wobec progu 18,32 ms, czyli **31 %** — na mapie 16 km z 272 tys.
mieszkańców 6,79 ms, czyli 37 %. Impostory zdjęłyby wycinek pasma 2–4 km z ośmiu milisekund
rysowania chunków, a dominujące w tej scenie kaskady cieni są bliskiego planu i nie dotyczą ich
wcale. Zysk jest przy tym **wydajnościowy, a nie wizualny** (`G-10` w `M11b`): chunki M1 sięgają
4 km, dalej rysuje clipmapa, więc bez impostorów nie ma dziury w obrazie.

**Przy okazji, i to jest warunek konieczny, a nie dodatek:** `RenderSnapshot.terrain_revision`
jest dziś polem, którego **nikt nie inkrementuje**. To pierwszy stopień inwalidacji tego pakietu
i jego jedyny czytelnik — jeśli WP4 nie wejdzie, pole zostaje martwe i należy je usunąć albo
podpiąć razem z `M12b`.

**Kryterium ukończenia:** scena `bench_city` mieści się w budżecie 192 MB, `impostor_resident ≤ 244`
i `impostor_regen ≤ 4` w każdej klatce (§7.3 dokumentu M11), a przy sztucznie obniżonym budżecie
nie powstaje **ani jedna dziura** — blok bez kafla rysuje się chunkiem LOD 8×. Wejście do rachunku
pamięci z §5.2 jako osobna pozycja: 192 MB VRAM nie jest pamięcią systemową, ale budżet 6 GB
z PRD §20.2 mówi o obu i rozdzielenie ich jest częścią tego pakietu.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

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


---

## Zmiany wpisane po M11e

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| DM-1 ★ | **M12a przejmuje `WP4b` z `M11e` jako WP4 — warunkowo.** Projekt techniczny stoi w `M11b` §5.6 i się nie zmienia; zmienia się adres i dochodzi warunek wejścia wyrażony liczbą | Pomiar z M11e/WP10 pokazał, że scena, którą ten pakiet miał przyspieszyć, ma trzykrotny zapas: 5,76 ms p95 wobec progu 18,32 ms (na mapie 16 km z 272 tys. mieszkańców — 6,79 ms). 192 MB VRAM zajętych na stałe i ok. 1,2 tys. linii kodu za przyspieszenie czegoś, co mieści się w budżecie trzykrotnie, to koszt bez odbiorcy. M12a jest właściwym adresem, bo VRAM jest pamięcią, a budżet pamięci jest tej podfazy |
| DM-2 | **`RenderStats` jest gotowe i eksportuje CSV** — `RenderStats::record_into(&mut MetricSink, Tick)` zapisuje siedemnaście serii (czasy ośmiu passów w mikrosekundach, wywołania, trójkąty, instancje, remeshing chunków, głosy, selekcja kadru, skala detalu) | DoD fazy M11 obiecywał M12 „gotowy pomiar" i to jest on. Profilowanie długiej sesji zaczyna się od szeregu czasowego, a nie od pisania drugiego licznika. Wejściem jest `MetricSink` z `engine/devtools`, czyli licznik, który M0 postawił dokładnie w tym celu |
| DM-3 | **Budżet klatki (`RenderBudget`) istnieje i sam obniża progi detalu** przy przekroczeniu celu, z podłogą 0,5 i karencją 30 klatek | Tryb 50× z `M12c` ma gotowy mechanizm degradacji obrazu pod presją czasu. Wiążące pozostaje to, co rozstrzygnęła decyzja 9.11 fazy M11: budżet **nie dotyka** `ViewQuery` ani `SnapshotCaps`, bo obniżanie capu snapshotu byłoby sprzężeniem render → symulacja |
| DM-4 | **`terrain_revision` w `RenderSnapshot` nie ma ani jednego pisarza i ani jednego czytelnika** | Pole powstało w M11a jako pierwszy stopień inwalidacji impostorów dzielnic, a ten pakiet właśnie tu przyszedł. Jeśli WP4 wejdzie — pole dostaje sens; jeśli nie wejdzie — należy je usunąć razem z rewizją schematu snapshotu w `M12b`, a nie zostawiać jako pole, które wygląda na działające |
| DM-5 | **Progi klatkowe mają bramkę, ale nie mają jej w CI**: `python scripts/frame_guard.py bench/frames` wymaga karty graficznej | M12 jest właścicielem profilowania i to on rozstrzyga, czy nocny bieg na maszynie referencyjnej dostanie własne zadanie w CI, czy zostaje ręczny. Linia bazowa jest związana z maszyną i plik zapisuje jej nazwę — porównanie raportu z jednej karty do linii bazowej z drugiej mierzy sprzęt, nie kod |
