# M2 — Miasto statyczne

Status: plan wykonawczy fazy.
Dokument nadrzędny: `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, LOD, dane, testy).
Ten dokument **nie redefiniuje** niczego z dokumentu 00. Rozbieżności → sekcja 9.

---

## 1. Cel fazy i artefakt końcowy

Po zakończeniu M2 z samego `world_seed` + `CityPlan` powstaje **kompletne, puste miasto**:
sieć dróg z mostami i tunelami, kolej towarowa, dzielnice o własnej tożsamości, kwartały,
parcele z właścicielem i wyceną, budynki voxelowe wygenerowane z gramatyki, wnętrza logiczne
(piętra → lokale → stanowiska pracy) oraz obsada budynków firmami-danymi, w której każdy
konsumowany produkt ma udokumentowane źródło.

Artefakt uruchamialny:

1. `tools/headless generate --seed 0xC0FFEE --size metropolis --profile mixed --epoch 1990`
   → `GenerationReport` (JSON): liczniki, czasy etapów, wynik domknięcia łańcuchów,
   wynik testów spójności, `world_hash_m2`.
2. Klient graficzny: kamera swobodna nad miastem, przełączalna **nakładka wartości gruntu**
   (§14.2), kliknięcie parceli/budynku → karta inspekcji z rozbiciem wyceny na czynniki
   (`LandValueBreakdown`, wymóg wyjaśnialności — dok. 00 §7).
3. `cargo test -p magnat-world --test consistency` — zielone testy Etapu 10 w zakresie
   dostępnym bez populacji.

Miasto **nie żyje**: nie ma mieszkańców, ruchu, transakcji. Firmy to wyłącznie rekordy danych.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zawartość |
|---|---|
| `engine/spatial` (nowy crate, właściciel M2) | siatka chunków, CSR-grid statyczny, grid dynamiczny, quadtree parcel, pola skalarne, zapytania promieniowe/kNN/prostokątne, kolejność Mortona |
| `sim/world` — Etap 3 | punkty wejścia do miasta, arterie L-systemem ograniczonym terenem, mosty, tunele, kolej towarowa |
| `sim/world` — Etap 4 | strefowanie wszystkich klas stref, pierścienie epok budowy |
| `sim/world` — Etap 5 | sieć lokalna, kwartały, parcele (właściciel, wartość, status) |
| `sim/world` — Etap 6 | gramatyka architektury voxelowej, wnętrza logiczne (`Unit`, `Workplace`) |
| `sim/world` — Etap 7 | obsada budynków firmami-danymi + algorytm domknięcia łańcuchów produktowych |
| `sim/world` — §4.3 | hierarchia Świat→Dzielnice→Kwartały→Parcele→Budynki→Lokale→Stanowiska, tożsamość dzielnic |
| `sim/world` — §6.7 (część statyczna) | statyczna wycena gruntu w 3 przebiegach |
| `engine/render` (rozszerzenie) | nakładka skalarna wartości gruntu + legenda |
| `sim/world` — Etap 10 | testy spójności generacji bez populacji |
| `data/` | `data/grammar/`, `data/zoning/`, `data/chains/`, `data/districts/`, minimalne `data/goods/`, `data/recipes/`, `data/buildings/` |

### Nie wchodzi

| Obszar | Faza |
|---|---|
| Generator terenu, hydrologia, geologia, klimat, biomy (Etapy 1–2) | M1 |
| Chunki voxelowe, paleta, meshing, streaming, LOD wizualne | M1 |
| Populacja, gospodarstwa domowe, Etap 8, klasy społeczne | M3 |
| Graf nawigacyjny, CH, pathfinding, symulacja ruchu, parkingi | M4 |
| Dynamiczna wartość gruntu z transakcji, rynek nieruchomości, deweloperzy, czynsze | M5 / M10 |
| Zachowania firm: produkcja, HR, ceny, AI, bankructwa | M6 / M7 |
| Usługi publiczne, podatek od nieruchomości, sieci przesyłowe | M8 |
| Historia „na sucho" (Etap 9) | M10 |
| Detale wizualne, animacje, renderowane wnętrza, „ścięcie" widoku | M11 |
| Metropolia jako cel wydajnościowy produkcyjnej jakości, tryb 50× | M12 |

Granica wobec M1 (**K-13**): M2 czyta teren **wyłącznie przez `TerrainQuery`** i pisze voxele
przez API M1, nigdy nie dotyka wewnętrznej reprezentacji chunka ani surowych danych
wysokościowych. Czego w `TerrainQuery` brakuje — zgłaszamy M1 jako rozszerzenie kontraktu,
nie liczymy sami (lista w sekcji 6).

Granica wobec M4 (**K-14**): *dana potrzebna, żeby miasto narysować — należy do M2;
dana potrzebna, żeby nim przejechać — należy do M4.*
M2 zapisuje: oś drogi, klasę, liczbę pasów, dopuszczalny tonaż, struktury (most/tunel/nasyp).
M4 wyprowadza z tego: geometrię pasów, skrzyżowania, kierunki ruchu, `RoadGraph`.
`RoadSegment` **nie** jest przeprojektowywany pod pasy.

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M2 |
|---|---|
| §4.1 | `CityPlan`: seed, wielkość, epoka startowa, profil gospodarczy, region — jako wejście generatora |
| §4.2 Etap 3 | WP3–WP5 (bramy, arterie, mosty/tunele, kolej) |
| §4.2 Etap 4 | WP7 (strefowanie) |
| §4.2 Etap 5 | WP6, WP8 (sieć lokalna, parcele) |
| §4.2 Etap 6 | WP10–WP12 (gramatyka, voxele, wnętrza logiczne) |
| §4.2 Etap 7 | WP13–WP14 (obsada firmami, domknięcie łańcuchów) |
| §4.2 Etap 10 | WP17 — podzbiór bez populacji |
| §4.3 | WP9 (dzielnice + tożsamość), hierarchia w całym projekcie danych |
| §6.7 | tylko „wartość gruntu = f(dostęp, hałas, prestiż…)" w wersji statycznej; transakcje → M5/M10 |
| §7.2 | katalog archetypów zakładów jako wymaganie na `SiteArchetypeId` w `data/buildings/` |
| §7.3 | z modelu fizycznego zakładu M2 realizuje wyłącznie: budynek na parceli, pojemność m², liczba stanowisk, rampa/parking jako atrybut pojemności. Maszyny, magazyny, media, emisje → M6–M8 |
| §14.2 | nakładka wartości gruntu (jedna z listy map cieplnych) |
| §16.2 | `engine/spatial` zgodnie z opisem crate'a |
| §17.5 | `CategoryGrid` jako indeks przestrzenny per kategoria — dostarczany teraz, używany od M5 |
| §17.7 | budżet pamięci warstwy M2 (sekcja 7) |
| §19 | kamień milowy „M2 — Miasto statyczne" |

---

## 4. Pakiety robocze

Kolejność jest istotna: etapy generacji tworzą łańcuch zależności zamknięty **trzema przebiegami
wyceny** (patrz WP15 i sekcja 5.7).

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| WP1 | `spatial`: siatka i indeksy statyczne | M0 | `GridSpec`, `CellId`, `morton2`, `CsrGrid`, `CategoryGrid`, zapytania `query_radius` / `query_rect` / `k_nearest` / `query_radius_batch` | benchmark: 1 mln zapytań promieniowych R=250 m po 300 tys. encji ≤ 180 ms na 8 wątkach; test własnościowy: wynik gridu == wynik brute-force dla 10 tys. losowych zapytań |
| WP2 | `spatial`: quadtree, grid dynamiczny, pola skalarne | WP1 | `ParcelTree` (quadtree AABB), `DynamicGrid` (przebudowa sortowaniem zliczającym), `ScalarField`, `multi_source_dijkstra` | zapytanie punkt→parcela ≤ 2 µs; przebudowa 400 tys. encji ≤ 3 ms na 8 wątkach i **bit-identyczna** przy 2 przebiegach; Dijkstra 1 mln komórek ≤ 400 ms |
| WP3 | Punkty wejścia do miasta | M1 (`Terrain`) | `CityGate`, wybór typu i miejsca wg profilu i regionu | dla 20 seedów × 5 profili: każdy wymagany typ bramy istnieje, leży na terenie zgodnym z typem (port na wodzie żeglownej, lotnisko na terenie o nachyleniu < 3%) |
| WP4 | L-system arterii | WP3 | reguły globalne + ograniczenia lokalne, snapowanie, klasy dróg | sieć bez wiszących końców klasy ≥ Collector; 2 przebiegi tego samego seeda → identyczny bajt w bajt `RoadNetwork` |
| WP5 | Mosty, tunele, kolej towarowa | WP4 | kryteria mostu/tunelu, koszt, A* najtańszej ścieżki dla torów, łączenie odnóg | każda strefa przemysłowa/logistyczna ma bocznicę ≤ 1,2 km od kwartału; brak toru o nachyleniu > 2% |
| WP6 | Kwartały z grafu dróg | WP5 | wyznaczanie ścian planarnych (faces) obchodem półkrawędzi | suma pól kwartałów + pas drogowy = pole obszaru zurbanizowanego (tolerancja 0,5%) |
| WP7 | Strefowanie z kwotami + pierścienie epok | WP2, WP6 | pola punktowe, przydział kwotowy per kwartał, pierścienie wieku zabudowy | udział każdej strefy w granicach ±3 pp. wobec profilu; brak strefy przemysłowej ciężkiej z nawietrznej względem R1–R3 przy dominującym wietrze (test miękki, próg 90%) |
| WP8 | Sieć lokalna + podział na parcele | WP7 | podział kwartału (rekurencyjny OBB + ulice), pasowy podział na działki wg wymiarów strefy | 100% parcel ma niezerową frontę drogową (poza `Green`/`Water`); zero nakładek wielokątów > 1 m² |
| WP9 | Dzielnice i ich tożsamość | WP7, WP8 | Voronoi po kwartałach + doginanie granic do arterii/rzek, nazwy, reputacja, `income_tier` | 10–40 dzielnic, pokrycie obszaru zurbanizowanego bez dziur i nakładek; nazwy unikalne |
| WP10 | Język gramatyki + parser + walidator | M0 (`io`) | schemat RON w `data/grammar/`, walidacja przy starcie | walidator odrzuca: brak `schema_version`, nieznany materiał, cykl `Ref`, przekroczenie budżetu węzłów; 100% plików z repo przechodzi |
| WP11 | Silnik derywacji gramatyki | WP10, WP8 | drzewo zakresów (scope tree), operacje `Split`/`Repeat`/`Extrude`/`Comp`/`Inset`/`Instance` | derywacja 50 tys. budynków równolegle daje identyczny wynik jak jednowątkowo |
| WP12 | Zapis voxeli + wnętrza logiczne | WP11, M1 (`voxel`) | terminale gramatyki → materiały; generacja `Unit` i `Workplace` | brak budynku przecinającego pas drogowy, wodę lub inną parcelę; suma m² lokali ≤ powierzchni brutto budynku |
| WP13 | Archetypy zakładów i szablony łańcuchów | WP12 | `data/buildings/` (typy z PRD §7.2), `data/chains/`, obsada stref | każda zabudowana parcela niemieszkalna ma przypisany `SiteSeed` lub jest `Institutional`/`Green` |
| WP14 | Domknięcie łańcuchów produktowych | WP13 | `supply_closure_check` + naprawa (import / dostawienie zakładu); walidator CI grafu produktów | `missing == []`; podaż/popyt w [0,85; 1,30] dla każdego towaru; **test negatywny: katalog, w którym towar jest osiągalny wyłącznie z `initial_stock.ron`, musi oblać walidację** (zapas startowy nie jest źródłem — 5.8) |
| WP15 | Wycena gruntu (3 przebiegi) | WP2, WP7, WP14 | `land_value_pass_0/1/2`, `LandValueBreakdown` | monotoniczność: średnia wartość w `OldTown` > `Suburb`; wartość przy przemyśle ciężkim < średniej dzielnicy; przebiegi deterministyczne |
| WP16 | Nakładka UI + karta inspekcji | WP15, M1 (`render`) | tekstura pola skalarnego na terenie, legenda, karta parceli/budynku | klik na parcelę pokazuje wartość i rozbicie na ≥ 8 czynników; 60 FPS z włączoną nakładką |
| WP17 | Testy spójności + raport generacji | WP14, WP15 | 12 testów z sekcji 7, `GenerationReport`, `world_hash_m2` | wszystkie testy zielone dla 32 seedów × 4 profile w CI (miasto małe) i 4 seedów (metropolia, nocne CI) |

Ścieżka krytyczna: WP1 → WP2 → WP4 → WP6 → WP7 → WP8 → WP11 → WP12 → WP14 → WP15 → WP17.
WP3, WP9, WP10, WP13, WP16 można prowadzić równolegle.

---

## 5. Projekt techniczny

### 5.1 `engine/spatial` — indeksy przestrzenne

Jeden crate, cztery struktury, zero dziedziczenia. Wszystko SoA, wszystko deterministyczne.

```rust
/// Definicja układu siatki. Wspólna dla wszystkich indeksów w świecie.
pub struct GridSpec { pub origin: Vec2, pub cell_m: u16, pub cols: u16, pub rows: u16 }
pub struct CellId(pub u32);          // row-major; morton2() dla porządku cache'owego

impl GridSpec {
    pub fn cell_of(&self, p: Vec2) -> CellId;
    pub fn cells_in_aabb(&self, a: Aabb2) -> CellRange;   // iterator, bez alokacji
    pub fn cell_center(&self, c: CellId) -> Vec2;
}
pub fn morton2(x: u16, y: u16) -> u32;   // przeplot bitów, dla kolejności encji w ECS

/// Indeks statyczny: budowany raz, tylko do odczytu. CSR (compressed sparse row).
pub struct CsrGrid<T: Copy> { spec: GridSpec, starts: Vec<u32>, items: Vec<T> }
impl<T: Copy> CsrGrid<T> {
    pub fn build(spec: GridSpec, it: impl Iterator<Item = (Vec2, T)>) -> Self;  // O(n)
    pub fn for_each_in_radius(&self, c: Vec2, r: f32, f: impl FnMut(Vec2, T));
    pub fn query_rect(&self, a: Aabb2, out: &mut Vec<T>);
}

/// §17.5: jeden CsrGrid per klucz kategorii (np. kategoria oferty, SiteArchetypeId).
pub struct CategoryGrid<K: Ord + Copy, T: Copy> { keys: Vec<K>, grids: Vec<CsrGrid<T>> }

/// Indeks dynamiczny: przebudowa co tick sortowaniem zliczającym (deterministyczna,
/// równoległa, niezależna od kolejności ukończenia jobów — dok. 00 §3.3).
pub struct DynamicGrid<T: Copy> { spec: GridSpec, front: CsrGrid<T>, back: Buffers }
impl<T: Copy> DynamicGrid<T> {
    pub fn rebuild(&mut self, pos: &[Vec2], ids: &[T], jobs: &JobScope);  // O(n + cells)
    pub fn for_each_in_radius(&self, c: Vec2, r: f32, f: impl FnMut(Vec2, T));
    pub fn k_nearest(&self, c: Vec2, k: usize, out: &mut Vec<(f32, T)>);  // ekspansja pierścieniowa
}

/// Quadtree nad AABB parcel. Parcele różnią się powierzchnią o 3 rzędy wielkości
/// (działka R2 ~400 m², pole rolne ~20 ha) — siatka jednorodna tu nie działa.
pub struct ParcelTree { nodes: Vec<QNode>, aabbs: Vec<Aabb2>, ids: Vec<ParcelId> }
impl ParcelTree {
    pub fn build(items: &[(Aabb2, ParcelId)]) -> Self;   // bulk, bottom-up po kluczu Mortona
    pub fn at_point(&self, p: Vec2, poly: &PolyArena) -> Option<ParcelId>;
    pub fn query_rect(&self, a: Aabb2, out: &mut Vec<ParcelId>);
    pub fn query_segment(&self, a: Vec2, b: Vec2, out: &mut Vec<ParcelId>);
}
// Parametry: max_depth = 12, bucket = 16, brak wstawiania w M2 (parcele statyczne);
// insert/remove dodaje M5 (podziały i scalenia działek).

/// Pole skalarne na GridSpec — nośnik wszystkich „map wpływu" generatora i nakładek UI.
pub struct ScalarField { spec: GridSpec, data: Vec<u16> }    // znormalizowane 0..=65535
impl ScalarField {
    pub fn sample(&self, p: Vec2) -> f32;                    // dwuliniowo, O(1)
    pub fn multi_source_dijkstra(spec: GridSpec, sources: &[CellId],
                                 cost: impl Fn(CellId, CellId) -> u32) -> Self;
    pub fn decay_from(sources: &[(CellId, f32)], half_life_m: f32) -> Self;
    pub fn combine(inputs: &[(&ScalarField, f32)]) -> Self;  // suma ważona w stałej kolejności
}
```

**Zapytania wsadowe.** Interfejs, z którego korzystają M3–M5 przy setkach tysięcy encji:
`query_radius_batch(&self, centers: &[Vec2], r: f32, out: &mut Csr<T>)` — zapytania są
wstępnie sortowane po `morton2(cell_of(center))`, dzielone na porcje po 256 i wykonywane
na job systemie; składanie wyniku po indeksie porcji, nie po kolejności zakończenia.

**Złożoności i budżety** (`k` = liczba komórek w AABB zapytania, `m` = liczba kandydatów,
`b` = pojemność liścia = 16):

| Zapytanie | Struktura | Złożoność | Budżet docelowy |
|---|---|---|---|
| punkt → parcela | `ParcelTree` | O(log n + b) | ≤ 2 µs |
| promień R, encje statyczne | `CsrGrid`, `cell_m = 64` | O(k + m), k = (2R/64 + 1)² | R = 250 m → k = 81; 1 mln zapytań ≤ 180 ms / 8 wątków |
| promień R, encje dynamiczne | `DynamicGrid` | O(k + m) + przebudowa O(n) | przebudowa 400 tys. ≤ 3 ms / 8 wątków |
| k najbliższych | ekspansja pierścieniowa | O(k·b) oczekiwana, najgorsza O(n) | k = 15 → ≤ 8 µs |
| prostokąt → parcele | `ParcelTree` | O(log n + wynik) | — |
| przecięcie odcinka z parcelami | `ParcelTree` + test slab | O(log n + wynik) | — |
| wartość pola w punkcie | `ScalarField` | O(1) | ≤ 30 ns |
| odległość „po drogach" do klasy X | `multi_source_dijkstra`, 16 m | prekomputacja O(N log N), odczyt O(1) | 1 mln komórek ≤ 400 ms |
| oferty kategorii K w promieniu R | `CategoryGrid` | O(k + m_K) | §17.5: typowo 3–15 kandydatów |

**Dobór `cell_m`.** Reguła: `cell_m ≈ mediana promienia zapytania / 4`. Ustalone:
budynki i parcele 64 m, agenci 32 m, pojazdy 32 m, oferty per kategoria 128 m
(kategorii jest dużo, encji w każdej mało), pola skalarne 16 m.

**Czego tu nie ma i dlaczego.** Brak R-tree (quadtree wystarcza dla statycznych AABB),
brak BVH (to nie jest raytracer — pikowanie myszą robi render przez bufor identyfikatorów),
brak rastra parcel 2 m (256 MB przy mapie 16×16 km; `ParcelTree::at_point` w 2 µs jest
wystarczające). Raster per chunk to zapisana ścieżka rozbudowy, gdyby M4 zaczął robić
miliony zapytań punkt→parcela na tick.

### 5.2 Etap 3 — szkielet transportu

#### Punkty wejścia

```rust
pub enum GateKind { Highway, RailFreight, RailPassenger, Port, Airport }
pub struct CityGate {
    pub kind: GateKind,
    pub pos: Vec2,                  // punkt na krawędzi mapy (Airport: wewnątrz)
    pub dir_inward: Vec2,
    pub capacity: Qty,              // przepustowość dobowa — używana od M6 (limit importu)
    pub node: NodeId,
}
```
Liczba i typy bram z profilu (`data/zoning/profile_*.ron`): każdy profil ma listę
`required: [GateKind]` i `optional: [(GateKind, prob)]`. Umiejscowienie: dla każdej krawędzi
mapy liczony jest koszt wprowadzenia drogi klasy `Highway` w głąb 800 m
(nachylenie, woda, przekroczenie rzeki) — wybierany argmin, z kwantyzacją do 200 m i
losowaniem `rng(seed, StreamId::Gates, gate_index, 0)` wśród 3 najlepszych, by uniknąć
stałego „zawsze południowy wschód". Port wymaga wody o głębokości ≥ 6 m i ciągłym
połączeniu do krawędzi mapy. Lotnisko: prostokąt 2600×600 m o nachyleniu < 3%,
≥ 4 km od centrum, poza pierścieniem gęstej zabudowy.

#### Klasy dróg

```rust
pub enum RoadClass { Highway, Arterial, Collector, Local, Service, Pedestrian, RailFreight, RailPassenger }
```

| Klasa | pasy | pas drogowy (ROW) | dł. segmentu | v [km/h] | max nachylenie | most: max rozpiętość | próg tunelu (przewyższenie) |
|---|---|---|---|---|---|---|---|
| Highway | 2×2 | 30 m | 400 m | 120 | 5% | 800 m | 20 m |
| Arterial | 2×2 | 24 m | 200 m | 60 | 7% | 300 m | 25 m |
| Collector | 1×2 | 16 m | 120 m | 50 | 9% | 120 m | 35 m |
| Local | 1×2 | 12 m | 60 m | 30 | 12% | 40 m | — |
| Service | 1×1 | 8 m | 40 m | 20 | 15% | 20 m | — |
| Pedestrian | — | 4 m | 30 m | — | 20% (schody) | 30 m | — |
| RailFreight | 1–2 tory | 20 m | 500 m | 80 | 2,0% | 400 m | 15 m |
| RailPassenger | 2 tory | 16 m | 500 m | 120 | 2,5% | 400 m | 15 m |

#### L-system arterii

Wariant parametryczny, sterowany kolejką priorytetową (schemat Parish–Müller), rozwijany
**jednowątkowo** — całość to ~3 s dla metropolii, równoległość nie jest warta ryzyka
determinizmu.

Stan: `Proposal { pos: Vec2, dir: Vec2, class: RoadClass, gen: u16, prio: u32, seq: u32 }`.
Kolejka: `BinaryHeap` po kluczu `(prio, seq)` — `seq` to monotoniczny licznik nadawany przy
wpychaniu, więc porządek jest **totalny**, bez remisów. To jest cały mechanizm determinizmu
L-systemu; RNG dla propozycji: `rng(world_seed, StreamId::RoadsL, seq, 0)`.

Aksjomat ω: dla każdej bramy `Highway` — propozycja w kierunku centrum, `class = Highway`,
`gen = 0`, `prio = 0`. Dodatkowo 1 propozycja obwodnicowa dla miast > 150 tys.

Reguły produkcji (po zaakceptowaniu segmentu `s` o końcu `p'` i kierunku `d'`):

```
P1  KONTYNUACJA
    Highway|Arterial|Collector → Proposal(p', rotate(d', θ_c), ta_sama_klasa, gen+1, prio+1)
    θ_c wg wzorca globalnego (niżej). Stop gdy gen > max_gen[class] lub p' poza obszarem.

P2  ROZGAŁĘZIENIE BOCZNE   (prawdopodobieństwo p_branch[class], sprawdzane co segment)
    Highway  → Arterial   w ±90°,  prio += 12,  p_branch = 0.15  (zjazdy, tylko co 4. segment)
    Arterial → Collector  w ±90°,  prio += 6,   p_branch = 0.55
    Collector→ Local      w ±90°,  prio += 3,   p_branch = 0.70   (obie strony niezależnie)
    Local    → Service    w ±90°,  prio += 1,   p_branch = 0.20   (tylko kwartały > 2 ha)

P3  ROZWIDLENIE (Y)        tylko Arterial, p = 0.08, dwaj potomkowie w ±(25°..40°)

P4  DOMKNIĘCIE PIERŚCIENIA (tylko Arterial, gen ≥ 3): jeśli w promieniu 1,5·seg_len istnieje
    węzeł Arterial nienależący do przodków — propozycja łącznika prosto do niego, prio += 2.
    Cel: sieć arterii ma być grafem z cyklami, nie drzewem (wymóg Etapu 10: spójność
    i brak pojedynczych punktów odcięcia dzielnicy).
```

**Wzorce globalne** (`θ_c` i preferencja kierunku) — wybierane per dzielnica-zalążek
z `data/districts/*.ron`, ważone epoką pierścienia:

| Wzorzec | Reguła kierunku | Typowo dla |
|---|---|---|
| `Radial` | `d'` = kierunek na/od centrum ± szum 8° | rdzeń staromiejski, promieniste wyloty |
| `Grid(θ)` | `d'` snapowane do najbliższej z {θ, θ+90°, θ+180°, θ+270°}, szum 3° | XIX-wieczne śródmieście, przedmieścia planowane |
| `Organic` | `d'` = `d` ± U(−25°, 25°), preferencja izolinii wysokości | średniowieczny rdzeń, wchłonięta wieś |
| `Contour` | `d'` minimalizuje nachylenie na odcinku (5 kandydatów co 12°) | teren górski, skarpy |
| `Superblock` | `Grid(θ)` z `seg_len ×= 3` dla Collector, brak `Local` wewnątrz | blokowisko lat 60.–80. |

**Ograniczenia lokalne** — stosowane w tej kolejności; pierwsze, które odrzuci, kończy próbę:

1. **Nachylenie.** `TerrainQuery::slope_at` co 8 m wzdłuż kandydata (nachylenie jako `u8`).
   Jeśli `max_slope > class.max_slope`: rotacja o ±15° (próby w kolejności: +15, −15, +30, −30),
   potem skrócenie do 50% długości, potem odrzucenie.
2. **Przeszkoda i wybór struktury.** `TerrainQuery::crossing_cost(a, b)` zwraca kwalifikację
   przeszkody na odcinku wraz z jej długością i różnicą wysokości. M2 **nie liczy tego sam** —
   decyduje tylko, czy dana klasa drogi stać na taką przeprawę:
   - `Crossing::Water { span, bank_delta }` → `Bridge`, jeśli `span ≤ class.bridge_max`
     **i** `bank_delta ≤ 0,05·span`; koszt ×`bridge_cost_mult[class]`, `prio += 4`.
     Most zawsze prostopadle do osi cieku ±20°, inaczej szuka obrotu.
   - `Crossing::Ridge { len, rise }` → `Tunnel { cover_dm }`, jeśli `rise > class.tunnel_trigger`
     **i** `len ≤ class.tunnel_max` (Highway 1200 m, Arterial 600 m, Collector 250 m);
     `prio += 6`. Dla `Local`/`Service` tunele zabronione.
   - `Crossing::Depression { len, drop }` → `Embankment` (nasyp), jeśli `drop ≤ 8 m`.
   - Brak dopuszczalnego wariantu → odrzucenie propozycji.
4. **Snapowanie węzła.** Jeśli `p'` leży w promieniu `0,25·seg_len` od istniejącego węzła
   o klasie ≥ `class` → scalenie (`p'` := ten węzeł, propozycja potomna wygasa).
5. **Przecięcie.** Jeśli segment przecina istniejący: oba dzielone w punkcie przecięcia,
   powstaje węzeł. Jeśli przecinane są dwie różne struktury (`Bridge` × `AtGrade`) →
   bezkolizyjnie, bez węzła (wiadukt).
6. **Minimalny kąt.** Odrzucenie, jeśli kąt do dowolnej krawędzi incydentnej w węźle
   docelowym < 30° (`Local`: < 22°).
7. **Strefa zakazana.** Odrzucenie wewnątrz pasa drogowego innej drogi, na wodzie
   bez mostu, na złożu oznaczonym jako `Extraction` oraz w obrysie lotniska.

Warunek stopu: pusta kolejka albo osiągnięty budżet segmentów wyliczony z `target_pop`
(sekcja 7, budżety). Budżet jest twardy — zabezpiecza przed rozbieganiem się generatora.

#### Kolej towarowa

Nie L-systemem. Dla każdego klastra stref `IndustryHeavy` / `Logistics` liczona jest
najtańsza ścieżka A* na siatce 32 m od bramy `RailFreight`, z kosztem
`base + 40·nachylenie% + 900·(woda) + 25·(przecięcie drogi klasy ≥ Collector)`
i karą nieskończoną za nachylenie > 2%. Ścieżki są następnie **scalane**:
wspólne prefiksy (odległość < 40 m na długości > 300 m) łączone w jeden tor z rozjazdem
w punkcie rozejścia. Kolejność przetwarzania klastrów: malejąca powierzchnia, remisy po
`BlockId` — deterministycznie.

```rust
pub struct RoadSegment {
    pub a: NodeId, pub b: NodeId,
    pub class: RoadClass,
    pub geom: PolyRef,            // OŚ drogi (centrolinia) — indeks do wspólnej areny punktów
    pub structure: RoadStructure, // AtGrade | Bridge { clearance_dm } | Tunnel { cover_dm }
                                  // | Embankment { height_dm }
    pub lanes_fwd: u8, pub lanes_bwd: u8,
    pub row_m: u8,                // szerokość pasa drogowego
    pub speed_kph: u8,
    pub max_tonnage_t: u8,        // dopuszczalny tonaż; 0 = bez ograniczenia
    pub length_dm: u32,
    pub district: DistrictId,
    pub flags: RoadFlags,         // ONEWAY | SIDEWALK | NO_HEAVY | TRAM_READY | RAIL | GRADE_SEPARATED
}
pub struct RoadNetwork {
    pub nodes: Vec<RoadNode>,     // pos, stopień, flagi (gate, junction, dead_end)
    pub segments: Vec<RoadSegment>,
    pub adj_start: Vec<u32>,      // CSR: sąsiedztwo węzeł → segmenty
    pub adj_items: Vec<SegmentId>,
    pub geom: PolyArena,
    pub gates: Vec<CityGate>,
    pub furniture: Vec<StreetFurniture>,   // latarnie, ławki, szpalery — wejście dla M11
}
pub struct StreetFurniture { pub seg: SegmentId, pub t: f32, pub pos: Vec3, pub kind: FurnitureKind }
pub enum FurnitureKind { StreetLamp, Bench, TreeRow, BusStopPad }
```

### 5.3 Etap 4 — strefowanie

#### Klasy stref

```rust
pub enum ZoneKind {
    Residential(ResDensity),   // 5 klas gęstości wg PRD §4.2
    Commercial,                // handlowa
    Office,                    // usługowa/biurowa
    IndustryLight,
    IndustryHeavy,
    Logistics,
    Agriculture,
    Institutional,             // publiczna
    Green,                     // zielona: park, las miejski, cmentarz, skwer
    Extraction,                // wydobywcza
    Water,
    Undevelopable,             // nachylenie, tereny zalewowe, pas ochronny
}
pub enum ResDensity { R1, R2, R3, R4, R5 }
// R1 domy wolnostojące · R2 szeregowa/bliźniacza · R3 kamienice/niska zwarta
// R4 bloki 4–11 kondygnacji · R5 wieżowce mieszkalne
```

#### Pola wejściowe (wszystkie `ScalarField`, siatka 16 m)

| Pole | Źródło | Uwaga |
|---|---|---|
| `d_center` | Dijkstra po sieci dróg od centroidu centrum | **nie** euklidesowo — rzeka bez mostu ma być barierą |
| `access_road` | Dijkstra ważony klasą drogi | wjazd z Highway ≠ wjazd z Service |
| `d_gate_rail`, `d_gate_road`, `d_port` | Dijkstra od bram | wejście dla logistyki i przemysłu |
| `noise` | `decay_from` po segmentach Highway/Arterial/Rail + strefach przemysłowych | half-life 120 m |
| `amenity` | `decay_from` po wodzie, lesie, parkach + bonus za widok (różnica wysokości) | half-life 300 m |
| `slope`, `flood_risk` | M1 (teren, hydrologia) | `flood_risk` > próg → `Undevelopable` |
| `deposit` | M1 (geologia) | koncentracja złoża → kandydat na `Extraction` |
| `soil_quality` | M1 (biomy, gleba) | `Agriculture` |
| `epoch_ring` | patrz niżej | oś „starówka ↔ blokowisko ↔ przedmieścia" |

#### Pierścienie epok

Zamiast losowania „stylu" — **symulowany wzrost footprintu**. Dla `n_epok` epok z
`data/epochs/` (np. średniowiecze, XIX w., międzywojnie, powojenne bloki, transformacja,
współczesność) footprint miasta jest rozszerzany o powierzchnię wynikającą z krzywej
populacji epoki, priorytetowo na komórki o najwyższym `access_road + amenity − slope`.
Każdy kwartał dostaje `epoch_ring: u8` = epoka, w której został objęty footprintem.
To jeden przebieg BFS po polu priorytetu, O(N log N), w pełni deterministyczny.
`epoch_ring` parametryzuje potem: wzorzec L-systemu, wielkość kwartału, wymiary działki,
zestaw gramatyk i paletę materiałów. Epoka startowa gry to ostatni pierścień —
miasto startujące w 1990 **ma** starówkę, jeśli krzywa epok ją przewiduje.

#### Przydział z kwotami

Strefy przypisujemy **kwartałom**, nie komórkom (inaczej powstaje szum i kwartał
z czterema strefami). Procedura:

```
1. Dla każdego kwartału b policz wektor punktacji:
       score[b][z] = Σ_f  w[z][f] · field_f(centroid_ważony(b))      // w[] z data/zoning/
   Twarde weta (score = −∞): Water, flood_risk > próg, slope > max[z],
   IndustryHeavy bliżej niż 400 m od jakiejkolwiek Residential, Extraction bez złoża.
2. Kwoty: target_area[z] = urban_area × mix[z][profile][epoch].
3. Przydział zachłanny z marginesem: kwartały sortowane malejąco po
       margin[b] = max_z score[b][z] − drugi_max_z score[b][z]
   (remisy po BlockId rosnąco). Kwartał trafia do najlepszej strefy, w której
   pozostała kwota ≥ jego powierzchni; jeśli brak — do następnej w kolejności punktacji.
4. Wygładzanie: 2 przebiegi. Kwartał, którego ≥ 3 z 4 sąsiadów mają tę samą inną strefę
   i którego margin < 0,15, przechodzi do strefy sąsiadów, jeśli kwoty na to pozwalają.
   Iteracja po BlockId rosnąco, zmiany aplikowane po przebiegu (nie w trakcie).
```
Wynik: udział każdej strefy w ±3 pp. wobec profilu, bez plam pikselowych, deterministycznie.
Złożoność O(B·Z + B log B), B ≈ 6 000 kwartałów, Z = 16 → pomijalne.

### 5.4 Etap 5 — sieć lokalna i parcele

#### Kwartały

`RoadNetwork` jest grafem planarnym (po kroku 5 ograniczeń lokalnych każde przecięcie ma
węzeł; mosty i tunele są wyłączone z planaryzacji i oznaczane `RoadFlags::GRADE_SEPARATED`).
Ściany wyznaczamy obchodem półkrawędzi: w każdym węźle krawędzie posortowane po kącie,
następna półkrawędź = „najbardziej w prawo". O(E). Ściana zewnętrzna (o ujemnym polu)
odrzucana. Kwartały o powierzchni < 300 m² scalane z sąsiadem o najdłuższej wspólnej
granicy.

```rust
pub struct Block {
    pub id: BlockId,
    pub district: DistrictId,
    pub neighborhood: u16,         // „osiedle" — grupa kwartałów, tożsamość drobniejsza niż dzielnica
    pub poly: PolyRef,
    pub area_m2: u32,
    pub zone: ZoneKind,
    pub epoch_ring: u8,
    pub parcels: Range<u32>,       // ciągły zakres w globalnej tablicy parcel
    pub bounding: Range<u32>,      // CSR → SegmentId otaczających dróg
}
```

#### Sieć lokalna

Kwartał większy niż `max_block_area[zone]` jest dzielony rekurencyjnie:
oblicz prostokąt minimalnego obwodu (OBB), przetnij w poprzek dłuższej osi w punkcie
`0,5 ± U(−0,12, 0,12)` i wstaw w cięciu ulicę klasy `Local` (lub `Service`, gdy obie połówki
< 0,6 ha). Stop gdy obie połówki ≤ `max_block_area` **lub** gdy krótszy bok < 2·`depth[zone]`.
Ślepe zakończenia dopuszczalne tylko dla R1/R2 (`cul-de-sac` z zawrotką o R = 9 m) i tylko
gdy `epoch_ring` ≥ epoki przedmieść — w pierścieniu staromiejskim ulica musi się domykać.

`max_block_area`: R1 4 ha · R2 2,5 ha · R3 1,2 ha · R4 6 ha · R5 4 ha · Commercial 3 ha ·
Office 2,5 ha · IndustryLight 8 ha · IndustryHeavy 30 ha · Logistics 20 ha ·
Institutional 6 ha · Agriculture i Green bez limitu.

#### Podział na parcele

Podział pasowy od frontu ulicy. Dla każdej krawędzi kwartału leżącej przy drodze
(krawędź frontowa), w kolejności malejącej klasy drogi, potem po `SegmentId`:

```
1. Odsuń krawędź do wewnątrz o depth[zone] → pas zabudowy.
2. Podziel długość frontu na odcinki o szerokości losowanej z rozkładu frontage[zone]
   (trójkątny: min/mode/max), rng(seed, StreamId::Parcels, block_index, i).
   Reszta < min → doklejana do ostatniej działki (nigdy nie powstaje działka poniżej min).
3. Prostuj boczne granice: prostopadle do frontu (Grid/Superblock) lub promieniście
   do centroidu kwartału (Radial/Organic).
4. Wnętrze kwartału pozostałe po pasach: jeśli pole > 0,4 ha i zone ∈ {R4,R5,Commercial}
   → jedna parcela wewnętrzna z dostępem przez bramę/przejazd; w przeciwnym razie
   → parcela typu Green (podwórko) należąca do miasta lub scalona z najgłębszą działką.
```

| Strefa | front [m] min/mode/max | głębokość [m] | pole [m²] |
|---|---|---|---|
| R1 | 18 / 22 / 30 | 32 | 600–1 200 |
| R2 | 10 / 14 / 18 | 30 | 320–600 |
| R3 | 9 / 14 / 22 | 38 | 340–900 |
| R4 | 40 / 60 / 95 | 55 | 2 200–7 000 |
| R5 | 30 / 45 / 65 | 50 | 1 500–4 000 |
| Commercial | 20 / 35 / 60 | 55 | 1 000–5 000 |
| Office | 25 / 40 / 60 | 50 | 1 200–4 000 |
| IndustryLight | 60 / 85 / 120 | 110 | 5 000–18 000 |
| IndustryHeavy | 120 / 200 / 300 | 260 | 2–12 ha |
| Logistics | 100 / 140 / 200 | 180 | 1,5–5 ha |
| Institutional | 40 / 70 / 140 | 80 | 3 000–20 000 |
| Agriculture | 120 / 200 / 400 | 380 | 3–20 ha |
| Extraction | — (kształt złoża) | — | 5–60 ha |

```rust
pub struct Frontage { pub seg: SegmentId, pub t0: f32, pub t1: f32 }  // parametryzacja wzdłuż osi drogi
pub enum ParcelOwner { Unowned, City, Citizen(CitizenId), Firm(FirmId), Developer(FirmId) }
pub enum ParcelStatus { Vacant, Built, UnderConstruction { done_at: SimMinute }, Derelict, Reserved }

pub struct Parcel {
    pub block: BlockId,
    pub district: DistrictId,
    pub poly: PolyRef,
    pub area_m2: u32,
    pub frontage: Frontage,
    pub zone: ZoneKind,
    pub owner: ParcelOwner,
    pub status: ParcelStatus,
    pub land_value_per_m2: Money,      // grosze; statyczna w M2
    pub building: Option<BuildingId>,
}
```
**Właściciele w M2.** Nie ma jeszcze mieszkańców ani prawdziwych firm, więc M2 ustawia:
`City` (drogi, zieleń, publiczne, ~12% parcel), `Firm(FirmId)` dla parcel z `SiteSeed`,
`Unowned` dla pustych, a parcele mieszkaniowe — `City` ze statusem `Reserved`.
**M3 (Etap 8) przepisuje właścicieli parcel mieszkaniowych na gospodarstwa domowe.**
To jedyny punkt, w którym M3 mutuje strukturę własności z M2 — kontrakt w sekcji 6.

### 5.5 §4.3 — dzielnice i hierarchia

Hierarchia jest **tablicowa, nie wskaźnikowa**: każdy poziom to `Vec` posortowany tak, że
dzieci każdego rodzica leżą w ciągłym zakresie (`Range<u32>`). Zaleta: iteracja po dzielnicy
jest ciągła w pamięci, zapytanie „lokale w budynku" to slice, a hash stanu (dok. 00 §3.6)
liczy się po tablicach w naturalnej kolejności.

```
World → districts[0..D] → blocks[d.blocks] → parcels[b.parcels] → building
      → buildings[..]   → units[bld.units] → workplaces[unit.workplaces]
```

```rust
pub struct District {
    pub name: String,                  // z data/names/, unikalna, z kontrolą kolizji
    pub kind: DistrictKind,            // OldTown | InnerCity | BlockEstate | Suburb
                                       // | IndustrialBelt | PortQuarter | Village | Campus | GreenBelt
    pub founded_epoch: EpochId,
    pub style: StyleId,                // klucz do palety materiałów i zestawu gramatyk
    pub boundary: PolyRef,
    pub blocks: Range<u32>,
    pub centroid: Vec2,
    pub reputation: Q,                 // start; M8/M10 zmieniają
    pub crime: Q,                      // start
    pub income_tier: u8,               // 0..=4 — M3 mapuje na klasy społeczne
    pub avg_land_value: Money,         // za m²; liczone po WP15
    pub pop_capacity: u32,             // suma mieszkań × wielkość GD epoki — wejście dla M3
}
```

Wyznaczanie granic:

1. **Zalążki.** Obowiązkowe: centrum, każda brama, centroid każdego klastra przemysłowego
   ≥ 30 ha. Uzupełnienie próbkowaniem Poissona o `d_min = sqrt(urban_area / target_count)`,
   `target_count` = clamp(10, 40, `target_pop` / 12 000).
2. **Voronoi po kwartałach** z metryką: odległość po sieci dróg (`ScalarField` od zalążka)
   + kara 0,35 za różnicę `epoch_ring` + kara 0,5 za różnicę klasy strefy.
3. **Doginanie do barier.** Dwa przebiegi po kwartałach rosnąco po `BlockId`: jeśli granica
   dzielnicy przecina kwartał, którego ≥ 60% obwodu styka się z arterią, koleją lub rzeką,
   kwartał przechodzi w całości na stronę wskazaną przez tę barierę. Zmiany aplikowane
   po przebiegu. Efekt: granice dzielnic biegną ulicami i rzeką, nie po przekątnej kwartału.
4. **Tożsamość.** `kind` z dominującej pary (`epoch_ring`, `zone`); `style` z `kind` + epoki;
   `income_tier` z kwantyla `avg_land_value` (5 kwantyli); `reputation` = f(`income_tier`,
   udział `Green`, hałas); `crime` = f(−`income_tier`, gęstość, odległość od centrum).
   Nazwa: `data/names/districts_pl.ron` — szablony (`{przymiotnik} {rdzeń}`, toponimy
   od cech terenu: „Zarzecze" przy rzece, „Podgórze" przy skarpie), unikalność wymuszana
   sufiksem kierunkowym.

### 5.6 Etap 6 — gramatyka architektury voxelowej

#### Język opisu (`data/grammar/*.ron`)

Gramatyka kształtowa nad **drzewem zakresów**: zakres = zorientowany prostopadłościan
(początek, 3 osie, wymiary). Reguła przekształca zakres w listę zakresów potomnych.
Terminale zapisują materiały do voxeli.

```ron
BuildingGrammar(
  schema_version: 1,
  id: "kamienica_secesyjna",
  applies: Applies(
    zones:        [Residential(R3), Commercial],
    epochs:       ["1890-1918"],
    styles:       ["staromiejski", "srodmiejski"],
    land_value:   (min: 18000, max: 90000),        // grosze za m²
    frontage_m:   (min: 9.0, max: 24.0),
    depth_m:      (min: 18.0, max: 50.0),
    weight:       30,
  ),
  massing: Perimeter( setback_front_m: 0.0, setback_side_m: 0.0, courtyard_min_m2: 60.0 ),
  rules: [
    Foundation( depth_m: 2.5, material: "brick_red", basements: 1 ),
    Floors(
      count:    Range(3, 6),
      height_m: 3.6,
      ground:   Ref("parter_uslugowy"),
      typical:  Ref("pietro_mieszkalne"),
      top:      Ref("pietro_poddasze"),
    ),
    Roof( Gable( pitch_deg: 40, material: "roof_tile_red",
                 dormers: Poisson(lambda: 0.35, per_m: 6.0) ) ),
    Details([
      Cornice( at: [FloorTop(0), RoofEave], depth_m: 0.4, material: "stucco" ),
      WindowBand( split: Repeat(width_m: 2.2, pad_m: 0.6), sill_m: 0.9, h_m: 1.9 ),
      Balcony( prob: 0.4, faces: [Front], from_floor: 2, depth_m: 1.1 ),
      Chimney( count: Range(2, 5) ),
      Entrance( faces: [Front], width_m: 1.6, snap_to_frontage: true ),
    ]),
  ],
  interior: Interior(
    ground:  Commercial( unit_area_m2: Range(45, 120) ),
    typical: Dwelling( unit_area_m2: Range(38, 95), per_floor: Range(2, 4) ),
    top:     Dwelling( unit_area_m2: Range(30, 60), per_floor: Range(2, 3) ),
    circulation_share: 0.14,          // klatki, korytarze — odejmowane od powierzchni brutto
  ),
)
```

Operatory dostępne w regułach (zamknięty zbiór, bez skryptów — modding gramatyk to M12):
`Split(axis, [Abs(m) | Rel(w) | Repeat(m)])`, `Inset(m)`, `Offset(m)`, `Extrude(m)`,
`Comp(faces: [Front|Back|Side|Top|Bottom])`, `Repeat(width, pad)`, `Ref(id)`,
`Choice([(weight, rule)])`, `If(cond, rule)` — gdzie `cond` czyta wyłącznie parametry
wejściowe (strefa, epoka, wartość gruntu, styl, wymiary zakresu, numer kondygnacji) —
`Instance(voxel_asset)`, `Fill(material)`.

Twarde limity (egzekwowane przez walidator i przez runtime):
głębokość derywacji ≤ 12, węzłów na budynek ≤ 4 096, `Instance` na budynek ≤ 256.
Przekroczenie = błąd generacji zliczany w `GenerationReport`, nie ciche obcięcie.

#### Parametryzacja (cztery kanały, zgodnie z PRD §4.2 Etap 6)

| Kanał | Wpływ |
|---|---|
| **strefa** | zbiór gramatyk kandydujących, typ wnętrza, obowiązkowe cofnięcie od granicy |
| **epoka** (`epoch_ring` kwartału) | zestaw reguł i paleta materiałów: XIX w. → cegła + tynk + dachówka, lata 70. → prefabrykat + papa, 2010 → szkło + tynk cienkowarstwowy |
| **wartość gruntu** | liczba kondygnacji (interpolacja w `count: Range`), poziom detalu (`Details` z progiem `land_value`), jakość materiału, powierzchnia lokalu |
| **styl dzielnicy** (`StyleId`) | filtr `applies.styles` + paleta kolorów elewacji + wybór kształtu dachu |

Wybór gramatyki dla parceli: filtr `applies`, następnie losowanie ważone
`weight × style_affinity(district.style)` z `rng(seed, StreamId::BuildingPick, parcel_idx, 0)`.
Brak kandydata → `_fallback_<zone>.ron` + inkrementacja licznika w raporcie.
Test akceptacyjny: udział fallbacku < 2%.

#### Determinizm i równoległość

RNG derywacji: `rng(world_seed, StreamId::BuildingGrammar, building_index, node_index)`,
gdzie `node_index` to numer węzła w porządku DFS. Brak licznika współdzielonego między
budynkami ⇒ **budynki można generować równolegle**, każdy w swoim buforze, a zapis do
voxeli jest składany po `building_index`. To najdroższy etap fazy i jedyny zrównoleglony.

#### Wnętrza logiczne

```rust
pub struct Building {
    pub parcel: ParcelId,
    pub grammar: GrammarId,
    pub epoch: EpochId,
    pub footprint: PolyRef,
    pub floors: u8,                 // nadziemne
    pub basements: u8,
    pub floor_heights_dm: SmallVec<[u16; 8]>,  // wysokość każdej kondygnacji — M11 tnie po stropach
    pub height_dm: u16,
    pub gross_area_m2: u32,
    pub units: Range<u32>,
    pub condition: Q,               // stan techniczny: f(epoka, wartość gruntu, szum)
    pub aabb: Aabb3,
    pub entrances: SmallVec<[Entrance; 4]>,
}
pub struct Entrance { pub pos: Vec3, pub seg: SegmentId, pub t: f32, pub kind: EntranceKind }
// EntranceKind: Main | Service | Ramp | Garage — Ramp wymagane dla Logistics/Industry

pub enum UnitKind { Dwelling { rooms: u8 }, Retail, Office, Workshop, Storage, Common }
pub enum UnitOccupant { Vacant, Household(HouseholdId), Site(SiteId) }
pub struct Unit {
    pub building: BuildingId,
    pub floor: i8,                  // ujemne = kondygnacja podziemna
    pub kind: UnitKind,
    pub area_m2: u16,
    pub occupant: UnitOccupant,     // w M2 zawsze Vacant lub Site(..)
    pub rent_hint: Money,           // miesięcznie; podpowiedź startowa dla M5
    pub workplaces: Range<u32>,
}
pub struct Workplace {
    pub unit: UnitIdx,
    pub site: SiteId,
    pub role: JobRoleId,
    pub shift: ShiftId,              // I | II | III | Flexible
    pub wage_band: WageBand,         // widełki płacowe — kontrakt dla M3 (patrz sekcja 6)
    pub occupant: Option<CitizenId>, // w M2 zawsze None
}
/// Widełki, nie pensja. Pensje emergentne to M7 — M2 daje tylko przedział startowy,
/// żeby M3 (Etap 8) mógł dopasować dochody rodzin do wartości mieszkań.
pub struct WageBand { pub min: Money, pub median: Money, pub max: Money }  // miesięcznie, brutto
```
Generacja lokali: powierzchnia użytkowa kondygnacji = pole obrysu × (1 − `circulation_share`);
dzielona na lokale wg `Interior` gramatyki; reszta < 0,6·min → doklejana do ostatniego lokalu.
Liczba `Workplace` w lokalu wynika z archetypu zakładu (Etap 7), nie z gramatyki —
gramatyka daje **pojemność** (m²), archetyp przelicza ją na stanowiska
(`m2_per_workplace` w `data/buildings/*.ron`).
`wage_band` pochodzi z `data/jobs/*.ron`: `JobRoleId` ma widełki bazowe dla epoki,
mnożone przez `wage_mult` archetypu zakładu i przez `income_tier` dzielnicy.
Nigdy nie jest wyliczane z populacji (jej jeszcze nie ma) — to wejście dla M3, nie wyjście.

Wizualnie: bryła + stropy + otwory okienne. **Renderowane wnętrza i „ścięcie" widoku to M11.**

### 5.7 Statyczna wycena gruntu (§6.7, część statyczna)

Problem kolejności: strefowanie potrzebuje wartości, wartość potrzebuje miejsc pracy,
miejsca pracy potrzebują budynków, budynki potrzebują wartości. Rozwiązanie: **trzy
przebiegi o ustalonym zakresie, bez iteracji do zbieżności** (determinizm + budżet czasu).

| Przebieg | Kiedy | Czynniki | Odbiorca |
|---|---|---|---|
| `pass_0` | przed Etapem 4 | `d_center`, `d_gate_*`, `amenity`, `slope`, `flood_risk` | punktacja stref (5.3) |
| `pass_1` | po Etapie 5 | + klasa drogi frontowej, długość frontu, `noise`, powierzchnia i kształt działki, `epoch_ring` | wybór gramatyki, liczba kondygnacji, `rent_hint` |
| `pass_2` | po Etapie 7 | + `job_access`, `retail_access`, `service_access`, prestiż dzielnicy, kara za sąsiedztwo `IndustryHeavy`/`Extraction` | UI, karta inspekcji, wejście dla M5/M8/M10 |

```
V = base[zone]
  × A_job × A_retail × A_transit × A_amenity
  ÷ (P_noise × P_pollution × P_flood)
  × prestige[district] × epoch_factor × frontage_factor
```
gdzie np. `A_job = 1 + k_job · Σ_b jobs_b · exp(−t(parcel, b) / 12 min)`, a `t` czytane
z `ScalarField` odległości po drogach (16 m). Wszystkie czynniki liczone w `f32`
(dozwolone — dok. 00 §2: geometria i funkcje użyteczności), ale:
**sumowanie w stałej kolejności enumeracji czynników**, jedno zaokrąglenie na końcu
(`div_round_half_up` → `Money` w groszach za m²). Nigdy nie kumulujemy `Money` z floata
w pętli.

```rust
pub struct LandValueBreakdown {           // wymóg wyjaśnialności — dok. 00 §7
    pub base: Money,
    pub factors: [(LandValueFactor, i16); 12],   // wpływ w punktach procentowych
    pub result: Money,
}
pub enum LandValueFactor { ZoneBase, JobAccess, RetailAccess, TransitAccess, Amenity,
    Noise, Pollution, FloodRisk, DistrictPrestige, Epoch, Frontage, Shape }
pub fn land_value_at(w: &World, p: ParcelId) -> (Money, LandValueBreakdown);
```

**Nakładka UI.** `pass_2` zapisany jako `ScalarField` → tekstura R16 rzutowana na teren
przez pipeline nakładek M1/`render`; paleta i progi z `data/ui/overlays.ron`; legenda
z wartościami w zł/m². Klik → karta parceli z tabelą `LandValueBreakdown` posortowaną
po wielkości wpływu. To jedyna nakładka danych wymagana w M2 — reszta listy z §14.2
powstaje wraz z fazami, które produkują dane.

### 5.8 Etap 7 — gospodarka bazowa (firmy jako obiekty danych)

W M2 firma **nie ma zachowań**: nie produkuje, nie zatrudnia, nie wycenia. Jest rekordem,
który mówi „w tym budynku będzie huta o skali 1,4, z 320 stanowiskami tych ról".

```rust
pub struct FirmSeed { pub name: String, pub sector: SectorId, pub sites: SmallVec<[SiteId; 4]> }
pub struct SiteSeed {
    pub firm: FirmId,
    pub building: BuildingId,
    pub units: Range<u32>,
    pub archetype: SiteArchetypeId,   // katalog z PRD §7.2, dane w data/buildings/
    pub recipes: Vec<RecipeId>,       // z archetypu; puste dla handlu i usług
    pub capacity_scale: u16,          // 1000 = skala bazowa archetypu
    pub workplaces: Range<u32>,
}
```

#### Obsada

1. **Przemysł i logistyka.** Dla każdego klastra przemysłowego losowany szablon łańcucha
   z `data/chains/*.ron` (np. `stal_podstawowa: [koksownia, huta, walcownia, wytwornia_konstrukcji]`),
   ważony profilem gospodarczym i epoką. Zakłady szablonu sadzone na parcelach klastra
   w kolejności szablonu, każdy na najbliższej wolnej parceli o wystarczającej powierzchni
   (kolejność przeglądania: rosnąco po odległości do poprzedniego ogniwa łańcucha,
   remisy po `ParcelId`).
2. **Handel detaliczny.** Normatywy na `target_pop` (wejście do walidacji, nie sztywna prawda):
   sklep osiedlowy 1/1 500 · supermarket 1/12 000 · dyskont 1/10 000 · hipermarket 1/60 000 ·
   stacja paliw 1/8 000 · apteka 1/7 000 · piekarnia 1/6 000 · sklep specjalistyczny 1/9 000 ·
   targ 1/50 000 · salon samochodowy 1/45 000. Rozmieszczenie: wybór parcel `Commercial`
   maksymalizujący pokrycie ludności (zachłanny k-center po `pop_capacity` kwartałów,
   deterministyczny, k = liczba placówek).
3. **Usługi i biura.** Mix z profilu: udział biur w powierzchni `Office`; typy z `data/buildings/`
   ważone epoką (1990: mało IT, dużo biur projektowych; 2020: odwrotnie).
4. **Publiczne.** Szkoła 1/1 200 · przedszkole 1/2 000 · przychodnia 1/8 000 · szpital 1/120 000 ·
   komisariat 1/40 000 · remiza 1/35 000 · urząd 1/miasto. Zakłady `Institutional`
   należą do `City`; ich prawdziwe działanie to M8.
5. **Rolnictwo i wydobycie.** Jedna parcela = jedno gospodarstwo/kopalnia;
   `capacity_scale` z jakości gleby / koncentracji złoża (M1).

#### Algorytm domknięcia łańcuchów produktowych

`supply_closure_check` — domknięcie osiągalności na hipergrafie receptur
(schemat Dowlinga–Galliera), O(V + E), w pełni deterministyczny.

```
WEJŚCIE: zbiór SiteSeed, katalog towarów G, katalog receptur R, bramy, target_pop.

def(1)  produced(g)   := istnieje r ∈ R zainstancjonowane w mieście, g ∈ outputs(r)
def(2)  importable(g) := Good::external_base_price.is_some()  ORAZ istnieje brama zgodnego typu
                         // jedno pole, nie „flaga + cena" — nie da się mieć stanu
                         // „importowalny bez ceny". Epoki dostępności importu należą do M10
                         // (pole na TradeGood, nie na Good) i M2 ich nie wypełnia.
def(3)  demanded      := (towary z koszyka potrzeb epoki × target_pop)
                       ∪ (inputs(r) dla każdego r zainstancjonowanego)
                       ∪ (media i materiały eksploatacyjne archetypów)

KROK 1  seed := { g : importable(g) }
              ∪ { g : ∃ r zainstancjonowane, g ∈ outputs(r),
                      r.source ∈ { Extraction(_), Agriculture },
                      site(r) stoi na parceli ze złożem / glebą }
        reachable := seed
        kolejka   := receptury o inputs ⊆ reachable   (FIFO, wkładane rosnąco po RecipeId)

        // Punkt wejścia rozpoznajemy po JAWNEJ fladze Recipe::source, nie po „inputs puste".
        // ZAOSTRZENIE (wymóg M6): ZAPAS STARTOWY NIE JEST ŹRÓDŁEM.
        // data/scenarios/initial_stock.ron pokrywa pierwsze ~14 dni świata, ale walidator
        // NIE MOŻE traktować go jako punktu wejścia domknięcia — inaczej przejdzie w CI
        // łańcuch, który raz wystartuje i nigdy się nie odtworzy po wyczerpaniu zapasu,
        // czyli dokładnie ta klasa błędu, którą walidator ma łapać.
        // Punkty wejścia: WYŁĄCZNIE RecipeSource::{Extraction, Agriculture}
        //                 oraz Good::external_base_price.is_some().

KROK 2  while kolejka niepusta:
            r := pop
            for g in outputs(r) rosnąco po GoodId:
                if g ∉ reachable:
                    reachable += g
                    dla każdej receptury r' z g ∈ inputs(r'): dec(licznik_brakow[r'])
                    if licznik_brakow[r'] == 0: push(r')

KROK 3  missing := demanded \ reachable

KROK 4  NAPRAWA — dla g ∈ missing rosnąco po GoodId:
          a) jeśli importable(g) i brama ma wolną przepustowość:
                oznacz g jako import, reachable += g, wróć do KROKU 2
          b) w przeciwnym razie wybierz receptę r produkującą g:
                min (liczba brakujących wejść), remis → min RecipeId
             znajdź parcelę: strefa wymagana przez archetyp r, area ≥ min, status Vacant,
                min koszt transportu do największego odbiorcy g; remis → min ParcelId
             utwórz SiteSeed + Building (gramatyka archetypu), demanded += inputs(r),
             reachable += outputs(r), wróć do KROKU 2
          c) brak wolnej parceli → podnieś kwotę strefy o 1 kwartał (przestrefowanie kwartału
             o najniższym marginesie z 5.3 krok 3) i powtórz b)
          d) nadal brak → BŁĄD GENERACJI: pozycja w GenerationReport.errors, test CI czerwony

KROK 5  BILANS PRZEPUSTOWOŚCI — dla każdego g ∈ demanded:
          // yield NIE jest osobnym polem — liczy się z receptury, żeby nie rozjechał się z masami:
          daily_yield(r, g) := r.outputs[g].mass * 1440 / r.duration_minutes
          supply(g) := Σ capacity_scale(site)/1000 · daily_yield(r, g) po zakładach produkujących g
                     + import_cap(g)
          demand(g) := popyt ludności (target_pop × koszyk epoki)
                     + Σ inputs(r, g) · capacity(site) po odbiorcach
          ratio := supply/demand
          if ratio < 0.85: capacity_scale zakładów ×= min(2.5, 0.9/ratio); jeśli nie wystarcza
                           → dostaw kolejny zakład (krok 4b)
          if ratio > 1.30: capacity_scale ×= 1.15/ratio (nie poniżej 0.4 skali bazowej)

WARUNEK AKCEPTACJI: missing == [] ORAZ ∀g: 0.85 ≤ ratio(g) ≤ 1.30
```

**Cykle.** Receptury bywają wzajemnie zależne i to jest **normalne oraz pożądane**
(rafineria ↔ elektrownia, huta ↔ fabryka maszyn). Domknięcie z KROKU 2 z definicji nie
wchodzi w cykl bez punktu wejścia — taki towar ląduje w `missing`, a KROK 4a przecina cykl
importem. Stąd **twardy wymóg na dane, potwierdzony przez M6**: każdy cykl w grafie receptur
musi mieć co najmniej jedno wejście z importu lub z wydobycia. M6 zadeklarował, że będzie
tej reguły bronić w danych, więc **zostaje proste domknięcie Dowlinga–Galliera — bez
przechodzenia na rozwiązywanie punktu stałego.**
Weryfikuje to walidator danych uruchamiany przy ładowaniu i w CI. Dok. 00 §5 został
**zmieniony**: walidator domknięcia grafu produktów wchodzi do CI już w M2, bo bez niego
Etap 7 nie ma jak się domknąć. M2 tworzy walidator i minimalny katalog; **właścicielem
docelowego schematu i pełnego katalogu (~400 towarów) pozostaje M6** — kształt schematu
uzgadniany z M6 (9.2/1), żeby nie musiał go przepisywać.

W M6 algorytm zostanie rozwinięty: `capacity_scale` zastąpi realna zdolność produkcyjna
maszyn, `import_cap` stanie się dynamiczne, a bilans z KROKU 5 — wejściem do balansatora.

#### Granica z M6 — co czyję, a czego nie dotykam

Schemat `Good` i `Recipe` należy do M6 (`M6-lancuch-dostaw.md` §5.1 i §5.4); M2 wypełnia
go wcześniej minimalnym katalogiem i pisze walidator. Trzy pary pól wyglądają podobnie
i **nie wolno ich zlewać**:

| Pole M6 | Znaczenie | Odpowiednik po stronie M2 | Dlaczego osobno |
|---|---|---|---|
| `Recipe::labour: Vec<(JobRoleId, u32)>` | **pracochłonność szarży w osobominutach** — wielkość kosztowa | `Workplace` + `data/buildings/*.ron::m2_per_workplace` — **obsada etatowa** | to dwie różne liczby o osobnych właścicielach (obsada: M2 i M7). Muszą się zgadzać w balansie, ale nie są tym samym i nie wyprowadzam jednej z drugiej |
| `Recipe::machine_class: MachineClassId` | abstrakcyjna zdolność („potrzebuję młyna walcowego") | archetyp w `data/buildings/` deklaruje, ile slotów której klasy mieści hala | receptura nie zna obiektu fizycznego; render i generator czytają **archetyp**, nigdy receptury |
| `Recipe::source: RecipeSource` | `Manufacturing` / `Extraction(ResourceKind)` / `Agriculture` | — | jawny punkt wejścia mojego domknięcia; M6 dodał to pole właśnie na potrzeby M2 |

Pozostałe ustalenia przyjęte od M6 bez zmian:

- **`GoodUnit { Grams, Milliunits }` — dwie jednostki, nie trzy.** `Volume` nigdy nie jest
  jednostką natywną: jest pochodną masy przez `density_g_per_l`. `Grams` dla Bulk/Liquid/Gas,
  `Milliunits` (milisztuki, zawsze wielokrotność 1000) dla Piece/Palletized.
  Pole publiczne i stabilne — czyta je też `sim/macro::lift()` w M10.
- **`schema_version` per plik**, nie wspólny dla katalogu (~60 plików towarów; wspólna wersja
  zmuszałaby do bumpowania wszystkiego przy dodaniu jednej kategorii).
- **Bilans masy receptury** `Σ inputs == Σ outputs + process_loss` — reguła egzekwowana
  przez mój walidator w CI od M2. Z niej właśnie wynika, że `yield` nie może być osobnym polem.
- **Kalendarz: rok ma 360 dni (12 × 30), K-1.** `1440` w `daily_yield` to minuty na dobę
  i się nie zmienia, ale **roczne agregaty w KROKU 5 liczę przez 360, nie 365.**

---

## 6. Kontrakty międzyfazowe

### Konsumuję

| Od | Typ / funkcja | Użycie |
|---|---|---|
| M0 `engine/core` | `Money`, `SimMinute`, `Tick`, `Q`, `Qty`, `DistrictId`, `ParcelId`, `BuildingId`, `SiteId`, `FirmId`, `GoodId`, `RecipeId`, `JobRoleId`, `StreamId`, `rng()`, `div_round_half_up` | wszędzie |
| M0 `engine/ecs` | `World`, `Entity`, bufory komend, `SystemId` | encje parceli, budynków, zakładów |
| M0 `engine/jobs` | deterministyczny fork-join | pola skalarne, derywacja gramatyki, `DynamicGrid::rebuild` |
| M0 `engine/io` | snapshot ECS, hash stanu | `world_hash_m2` |
| M1 `TerrainQuery` (**K-13, jedyne źródło prawdy o terenie**) | `height_at` → `i32` w jednostkach **0,5 m**; `slope_at` → `u8`; `buildability_at`; `obstacle_mask`; `crossing_cost` (most / tunel / nasyp wraz z długością i różnicą wysokości). Współrzędne w metrach | ograniczenia lokalne L-systemu (5.2), strefowanie (5.3), niwelacja parceli (R9) |
| M1 `engine/voxel` | `ChunkWriter`, `MaterialPalette`, `VoxelId` | zapis brył budynków, nasypów, mostów |
| M1 `engine/render` | pipeline nakładek skalarnych na terenie | nakładka wartości gruntu |
| dane | `data/epochs/`, `data/names/`, `data/jobs/` | pierścienie epok, nazwy dzielnic, `wage_band` |

**Zgłoszone M1 rozszerzenia `TerrainQuery`** (M2 ich nie liczy sam — K-13):

| Potrzeba | Zapytanie | Kto tego jeszcze używa |
|---|---|---|
| umiejscowienie portu | `water_depth_at`, `is_navigable_to_edge` | M8 (transport wodny), M6 (import) |
| strefowanie wydobywcze | `deposit_at` → rodzaj, koncentracja, objętość | M6 (wyczerpywanie złoża) |
| strefowanie rolnicze | `soil_quality_at` | M6 (plon/ha) |
| weto dla `IndustryHeavy` z nawietrznej | `prevailing_wind` | M8 (zanieczyszczenie) |
| weto `Undevelopable` | `flood_risk_at` → pole ciągłe 0..1 na siatce 16 m | M8 (powodzie), M10 (ubezpieczenia) |

Jeśli któreś z nich M1 już pokrywa pod inną nazwą — używamy jego nazwy. Jeśli M1 odmówi
któregoś, odpowiadające mu weto lub strefa wypada z generatora i trzeba to odnotować
w `GenerationReport`; **nie** dopisujemy własnego liczenia z surowych danych wysokościowych.

### Dostarczam

**`engine/spatial`** (właściciel — nikt inny tego nie projektuje):
`GridSpec`, `CellId`, `morton2`, `CsrGrid<T>`, `CategoryGrid<K,T>`, `DynamicGrid<T>`,
`ParcelTree`, `ScalarField`, `multi_source_dijkstra`, `query_radius`, `query_radius_batch`,
`query_rect`, `query_segment`, `k_nearest`, `SpatialStats`.

**`sim/world`** (rozszerzenie):
`CityPlan`, `GenerationReport`, `world_hash_m2()`,
`generate_city(&CityPlan, &Terrain, &mut World) -> GenerationReport`;
`RoadNetwork`, `RoadSegment`, `RoadNode`, `RoadClass`, `RoadStructure`, `RoadFlags`, `CityGate`, `GateKind`;
`District`, `DistrictKind`, `StyleId`, `EpochId`, `Block`, `BlockId`;
`ZoneKind`, `ResDensity`; `Parcel`, `ParcelOwner`, `ParcelStatus`, `Frontage`, `PolyArena`, `PolyRef`;
`Building`, `Entrance`, `EntranceKind`, `Unit`, `UnitKind`, `UnitOccupant`, `Workplace`, `ShiftId`;
`FirmSeed`, `SiteSeed`, `SiteArchetypeId`, `SectorId`;
`land_value_at(..) -> (Money, LandValueBreakdown)`, `LandValueFactor`,
`supply_closure_check(..) -> ClosureReport`.

**`data/`**: `data/grammar/*.ron` (schemat + zestaw startowy), `data/zoning/profile_*.ron`,
`data/chains/*.ron`, `data/districts/*.ron`, `data/buildings/*.ron` (archetypy z PRD §7.2),
minimalne `data/goods/` i `data/recipes/` + walidator domknięcia (schemat uzgadniany z M6,
9.2/1), `data/ui/overlays.ron`.

**`StreamId`** — warianty dopisane przez M2. Wg **K-4** M2 dostaje blok **120–139**
(blok nadmiarowy, gdyby zabrakło: **1120–1219**). Wartości poniżej są **niezmienne** —
zmiana numeru strumienia zmienia każdy świat wygenerowany wcześniej z tego samego seeda,
więc numer raz nadany nie podlega refaktorowi ani przenumerowaniu przy dodawaniu wariantów.

| Wartość | Wariant | Użycie |
|---|---|---|
| 120 | `Gates` | wybór miejsca bram wśród 3 najlepszych kandydatów (5.2) |
| 121 | `RoadsL` | L-system arterii, klucz `seq` propozycji (5.2) |
| 122 | `RoadRail` | scalanie i rozjazdy kolei towarowej (5.2) |
| 123 | `Blocks` | podział kwartału OBB, punkt cięcia (5.4) |
| 124 | `Zoning` | szum punktacji i remisy przydziału kwotowego (5.3) |
| 125 | `EpochRings` | rozrost footprintu per epoka (5.3) |
| 126 | `Districts` | próbkowanie Poissona zalążków dzielnic (5.5) |
| 127 | `Naming` | nazwy dzielnic z szablonów (5.5) |
| 128 | `Parcels` | szerokości frontów w podziale pasowym (5.4) |
| 129 | `BuildingPick` | wybór gramatyki dla parceli (5.6) |
| 130 | `BuildingGrammar` | derywacja gramatyki, klucz `(building_index, node_index)` (5.6) |
| 131 | `Interiors` | podział kondygnacji na lokale, liczba pokoi (5.6) |
| 132 | `FirmSeed` | wybór szablonu łańcucha, nazwy firm (5.8) |
| 133 | `SitePlacement` | remisy przy sadzeniu zakładów i placówek handlu (5.8) |
| 134–139 | — | rezerwa M2 |

### Kto co bierze

| Faza | Bierze |
|---|---|
| M3 | `Unit` (mieszkania → GD), `Workplace` (→ rynek pracy), `District.{income_tier, pop_capacity, reputation}`, `Block.neighborhood`, `DynamicGrid` dla agentów. **Przepisuje `Parcel.owner` parcel mieszkaniowych na `Citizen`** |
| M4 | `RoadNetwork` (+ `RoadStructure`, `RoadFlags`) jako źródło grafu nawigacyjnego; `Entrance` jako punkty wejścia/wyjazdu; `DynamicGrid` dla pojazdów |
| M5 | `CategoryGrid` jako indeks ofert (§17.5); `land_value_at` i `Unit.rent_hint` jako ceny startowe; `SiteSeed` sklepów |
| M6 | `SiteSeed.{archetype, recipes, capacity_scale}`, `ClosureReport`, `CityGate.capacity` jako limit importu |
| M7 | `FirmSeed` → pełna `Firm`; `Workplace` → etaty |
| M8 | `Block`, `District` jako jednostki podatkowe i obszary usług publicznych; `Parcel.land_value_per_m2` jako podstawa podatku od nieruchomości |
| M10 | `land_value_per_m2` jako wartość startowa modelu dynamicznego; `District.{reputation, crime}` jako stan początkowy |
| M11 | `BuildingGrammar` — rozszerzenie o `Details` wizualne i wnętrza renderowane |

### Kontrakt dla M3 (odpowiedź na zapytanie, blokuje mu start pakietów)

**1. `Workplace` niesie `wage_band`** — tak, dodane do struktury (5.6):
`WageBand { min, median, max }` w `Money` (grosze, miesięcznie, brutto). Źródło:
`data/jobs/*.ron` (widełki bazowe roli dla epoki) × `wage_mult` archetypu zakładu ×
`income_tier` dzielnicy. To **widełki, nie pensja** — pensje emergentne z licytacji
to M7; M2 daje przedział startowy, żeby krok Etapu 8 dopasowujący dochody rodzin do
wartości mieszkań miał z czego liczyć. Para do zestawienia po stronie M3:
`WageBand.median` ↔ `Unit.rent_hint` ↔ `Parcel.land_value_per_m2`.

**2. Geometria ulic — minimalna forma, bez grafu** (graf należy do M4 wg **K-2**).
M3 dostaje z `RoadNetwork` wyłącznie odcinki centrolinii i ich długości:

```rust
pub struct StreetLine { pub seg: SegmentId, pub pts: &[Vec2], pub length_dm: u32, pub class: RoadClass }
pub fn street_lines(net: &RoadNetwork) -> impl Iterator<Item = StreetLine>;
```
Plus `Entrance { pos, seg, t }` jako punkt przywiązania budynku do ulicy oraz
`CsrGrid<SegmentId>` do zapytania „najbliższy odcinek ulicy". To wystarcza do
rozmieszczania ludzi, szacowania odległości i rysowania — i **nie** daje ani osiągalności,
ani czasu przejazdu, bo to domena M4. Dopóki M4 nie istnieje, M3 może użyć
`ScalarField` odległości po drogach (5.3) jako przybliżenia czasu dojazdu w Etapie 8.

### Kontrakt dla M11 (z jego zapytania)

- **Stropy do cięcia widoku poziomami** (PRD §15.2): `Building.{floors, basements, height_dm}`
  + `floor_heights_dm: SmallVec<[u16; 8]>` (wysokość każdej kondygnacji — nie zawsze równa,
  parter usługowy jest wyższy) + `Unit.floor`. Cięcie idzie po kondygnacjach, nie po
  dowolnej wysokości. `BuildingGrammar` pozostaje czytelna dla M11 jako wejście
  do `generate_interior(...)`.
- **Latarnie:** L-system emituje `StreetFurniture { seg, t, kind, pos }`
  (`FurnitureKind::{StreetLamp, Bench, TreeRow, BusStopPad}`) co `lamp_spacing_m[class]`
  wzdłuż osi, po stronie chodnika. To jedno pole w wyjściu WP4 — tanio i M11 tego potrzebuje.
- **Selekcja do kadru:** `CsrGrid<BuildingId>::query_rect` w 2D (rzut piramidy widzenia
  na płaszczyznę) + `Building.aabb: Aabb3` do dokładnego odrzucenia. Osobnego indeksu 3D
  M2 **nie** buduje — miasto jest płaskie w sensie indeksowania, a `Aabb3` per budynek
  wystarcza. Jeśli pomiar w M11 pokaże inaczej, dokładamy trzeci wymiar do `GridSpec`.
- **Wymagania przestrzenne zakładu** (ile stanowisk w hali, jakie wyposażenie) zostają
  w `data/buildings/*.ron` przy archetypie, **nie** w recepturze — zgodnie z sugestią M11,
  żeby render nie zależał od katalogu ekonomicznego. Do potwierdzenia przez M6 (9.2/1).

---

## 7. Testy i kryteria akceptacji

### Testy spójności generacji (Etap 10, zakres bez populacji)

| # | Test | Kryterium |
|---|---|---|
| T1 | Spójność grafu dróg | jedna składowa spójna po segmentach jezdnych (bez `Pedestrian`); każda brama osiągalna z centroidu |
| T2 | Odporność na odcięcie | żadna dzielnica nie jest połączona z resztą miasta pojedynczym segmentem (test mostów grafu: `bridges(G) ∩ granice_dzielnic = ∅`) |
| T3 | Dostępność budynków | BFS po grafie pieszym od bram dosięga 100% `Entrance`; każde `Entrance` ma drogę w promieniu 50 m |
| T4 | Rampy | 100% budynków na `Logistics`/`IndustryHeavy` ma `EntranceKind::Ramp` przy drodze bez `NO_HEAVY` |
| T5 | Brak nakładek parcel | pole przecięcia dowolnej pary parcel ≤ 1 m² (testowane przez `ParcelTree::query_rect`) |
| T6 | Budynek w parceli | obrys budynku ⊆ wielokąt parceli, tolerancja 0,1 m; brak kolizji z pasem drogowym, wodą, torem |
| T7 | Fronta drogowa | 100% parcel poza `Green`/`Water`/`Extraction` ma `Frontage` o długości > 0 |
| T8 | Pokrycie dzielnicami | dzielnice pokrywają obszar zurbanizowany bez dziur i nakładek; każdy kwartał w dokładnie jednej dzielnicy; 10 ≤ D ≤ 40 |
| T9 | Kwoty stref | udział każdej strefy w ±3 pp. wobec profilu |
| T10 | Bilans lokali i stanowisk | Σ mieszkań × wielkość GD epoki ∈ [0,97; 1,08] × `target_pop`; Σ stanowisk ∈ [0,95; 1,12] × oczekiwanych etatów |
| T11 | Domknięcie łańcuchów | `missing == []`; ∀g: `0,85 ≤ supply/demand ≤ 1,30` |
| T12 | Sanity wyceny | `avg_land_value(OldTown) > avg_land_value(Suburb)`; parcela sąsiadująca z `IndustryHeavy` poniżej mediany dzielnicy; brak wartości ≤ 0 |

### Determinizm (dok. 00 §3.6)

- D1: dwa pełne przebiegi `generate_city` z tym samym seedem → identyczny `world_hash_m2`
  **oraz** bit-identyczne bufory `RoadNetwork`, `parcels`, `buildings`, `units`, `workplaces`.
- D2: generacja budynków jednowątkowa == wielowątkowa (ten sam hash) — pilnuje, by
  równoległość WP11 nie wprowadziła zależności od kolejności ukończenia jobów.
- D3: `DynamicGrid::rebuild` daje identyczny `CsrGrid` przy 1, 4 i 8 wątkach.
- D4: `ScalarField::combine` i wycena — identyczne przy zmianie liczby wątków
  (test wykrywa sumowanie floatów w kolejności ukończenia).
- D5: hash M2 dopisany do funkcji haszującej ECS z dok. 00 §3.6 — Definition of Done.

### Wydajność

| Miara | Budżet (metropolia 400 tys., 8 rdzeni) |
|---|---|
| pola skalarne + Dijkstra (6 pól) | ≤ 4 s |
| L-system dróg (1 wątek) + kolej | ≤ 4 s |
| kwartały + sieć lokalna + parcele | ≤ 6 s |
| strefowanie + pierścienie + dzielnice | ≤ 3 s |
| wycena (3 przebiegi) | ≤ 3 s |
| derywacja gramatyki + zapis voxeli (równolegle) | ≤ 28 s |
| Etap 7 + domknięcie | ≤ 4 s |
| testy spójności | ≤ 8 s |
| **razem** | **≤ 60 s** |
| miasto małe (40 tys.) — używane w CI | ≤ 12 s |
| nakładka wartości gruntu włączona | bez spadku poniżej 60 FPS w widoku dzielnicy |

### Budżety wielkości i pamięci (metropolia 400 tys., profil mieszany)

**Sieć transportowa:** ~16 000 segmentów, ~13 000 węzłów, ~1 200 km dróg, ~45 km torów,
40–120 mostów, 0–15 tuneli (zależnie od regionu).

**Parcele, budynki, lokale, stanowiska:**

| Strefa | parceli | budynków | lokali | stanowisk |
|---|---|---|---|---|
| R1 | 11 000 | 11 000 | 12 000 | — |
| R2 | 9 500 | 9 500 | 13 000 | — |
| R3 | 7 000 | 7 000 | 42 000 | 6 000 |
| R4 | 2 200 | 6 500 | 78 000 | 2 500 |
| R5 | 400 | 900 | 22 000 | 1 500 |
| Commercial | 3 800 | 3 600 | 6 500 | 32 000 |
| Office | 2 400 | 2 300 | 7 000 | 54 000 |
| IndustryLight | 1 500 | 2 400 | 2 400 | 33 000 |
| IndustryHeavy | 260 | 900 | 900 | 21 000 |
| Logistics | 420 | 700 | 700 | 12 000 |
| Institutional | 1 300 | 1 900 | 3 000 | 40 000 |
| Green | 900 | 300 | 300 | 3 000 |
| Agriculture | 1 500 | 2 400 | 1 200 | 5 000 |
| Extraction | 40 | 120 | 120 | 2 500 |
| **razem** | **~42 000** | **~49 500** | **~189 000** | **~212 500** |

Kontrola: 167 000 mieszkań × 2,4 os. = 400 tys. ✓
212 500 stanowisk wobec ~196 tys. aktywnych zawodowo przy 7% bezrobocia = ~86% obsadzenia;
reszta to wakaty i stanowiska zmianowe — mieści się w widełkach T10.
Obszar zurbanizowany ~70 km², kwartałów ~6 000, dzielnic 32.

**Pamięć warstwy M2** (bez voxeli, które należą do M1):

| Element | Rozmiar |
|---|---|
| parcele 42 tys. × 96 B + arena wielokątów | 6,1 MB |
| budynki 49,5 tys. × 80 B + obrysy | 5,0 MB |
| lokale 189 tys. × 24 B | 4,5 MB |
| stanowiska 212 tys. × 16 B | 3,4 MB |
| drogi: segmenty + węzły + CSR + geometria | 1,6 MB |
| kwartały 6 tys., dzielnice 32 | 0,5 MB |
| `SiteSeed` / `FirmSeed` ~7 tys. | 0,8 MB |
| `ScalarField` × 6 trwałych (1 mln komórek × 2 B) | 12,0 MB |
| indeksy: `ParcelTree` + `CsrGrid` budynków + `CategoryGrid` | 4,0 MB |
| `DynamicGrid` dla 400 tys. encji (rezerwacja dla M3/M4) | 4,0 MB |
| **razem** | **~42 MB** |

Wobec celu 6 GB z §17.7 warstwa statyczna miasta to ~0,7% budżetu — pole na późniejsze
komponenty jest zachowane.

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Skutek | Mitygacja |
|---|---|---|---|
| R1 | L-system rozbiega się: sieć samoprzecinająca, wiszące końce, pajęczyna w górach | miasto nie do przejścia, T1/T2 czerwone | twardy budżet segmentów + reguła P4 (domykanie pierścieni) + `SpatialStats` z liczbą odrzuceń per ograniczenie; przy odrzuceniach > 40% generator loguje ostrzeżenie i zmniejsza `seg_len` klasy o 25% |
| R2 | Geometria: parcele nakładające się, budynki poza działką, dziury w kwartałach — klasyczna zmora podziału wielokątów na floatach | T5/T6 losowo czerwone, trudne do debugowania | cała geometria parcel w **stałoprzecinkowym** układzie milimetrowym (`i32`); operacje podziału na liczbach całkowitych; tolerancje jawne (1 m² dla nakładek, 0,1 m dla obrysu) |
| R3 | Zależność cykliczna wycena ↔ strefowanie ↔ budynki ↔ firmy zamienia się w iterację do zbieżności | niedeterminizm i nieprzewidywalny czas generacji | **trzy przebiegi o stałym zakresie** (5.7), zakaz pętli zbieżnościowej; test D1 pilnuje |
| R4 | `data/grammar/` staje się drugim językiem programowania (warunki, pętle, skrypty) | koszt utrzymania większy niż silnika | zamknięty zbiór operatorów, `If` czyta tylko parametry wejściowe, twarde limity głębokości i liczby węzłów, brak zmiennych użytkownika; modding gramatyk odłożony do M12 |
| R5 | Etap 7 nie domyka się, bo `data/goods` i `data/recipes` są w M2 szczątkowe | generacja wywala się na własnym teście | minimalny katalog (~60 towarów, ~45 receptur) pokrywający koszyk potrzeb epoki startowej, pisany **pod schemat M6** (§5.1 i §5.4 jego dokumentu), + walidator w CI od M2; M6 rozszerza katalog, nie algorytm — potwierdził, że reguły „każdy cykl ma wejście z importu lub wydobycia" będzie bronić w danych |
| R6 | Derywacja gramatyki na 50 tys. budynków przekracza budżet czasu | generacja metropolii > 2 min | profilowanie od WP11 (criterion na 1 000 budynków), cache derywacji dla identycznych krotek (gramatyka, wymiary zaokrąglone do 0,5 m, epoka, tier wartości) — trafienia rzędu 60% w blokowiskach |
| R7 | `engine/spatial` zaprojektowany pod M2, a M3–M5 będą potrzebowały czegoś innego | przeprojektowanie crate'a w trakcie M4 | API zapytań wsadowych i `CategoryGrid` (§17.5) dostarczone **teraz**, mimo że M2 ich nie używa — jedyne wyprzedzenie, na jakie ta faza sobie pozwala; uzasadnienie: zmiana układu indeksu po M4 dotknie 4 crate'y |
| R8 | Granice dzielnic po Voronoi biegną w poprzek kwartałów i wyglądają sztucznie | „mieszkańcy identyfikują się z dzielnicą" (§4.3) nie działa w UI | krok 3 z 5.5 (doginanie do arterii i rzek) + test T8 + wizualna kontrola w narzędziu podglądu |
| R9 | Kolizja budynków z voxelami terenu M1 (budynek w zboczu, wiszący w powietrzu) | wizualne artefakty w całym mieście | niwelacja terenu per parcela: wyrównanie do mediany wysokości obrysu przed zapisem, ze skarpą lub murem oporowym przy różnicy > 1,5 m; test: żaden voxel fundamentu nie graniczy z powietrzem od spodu |
| R10 | Nazwy dzielnic powtarzają się lub brzmią absurdalnie | psuje immersję, widoczne natychmiast | pula szablonów + toponimy z cech terenu + wymuszona unikalność; lista odrzuceń w `data/names/` |

---

## 9. Decyzje otwarte

### 9.1 Rozstrzygnięte — wiążące, wpisane do projektu

| # | Decyzja | Rozstrzygnięcie | Gdzie w dokumencie |
|---|---|---|---|
| 1 | Kontrakt terenu z M1 | **K-13**: `TerrainQuery` jest jedynym źródłem prawdy. Wysokość `i32` w jednostkach **0,5 m** (nie decymetry — moja pierwotna propozycja odrzucona), nachylenie `u8`, współrzędne w metrach. `slope_at`, `buildability_at`, `obstacle_mask`, `crossing_cost` pokrywają WP3/WP4. Brakujące zapytania zgłaszane jako rozszerzenie `TerrainQuery`, nie liczone samodzielnie | 2, 5.2 (ogr. lokalne 1–2), 6 (tabela rozszerzeń) |
| 2 | Granica z M4 | **K-14**: dana do narysowania miasta — M2; dana do przejechania nim — M4. M2 daje oś, klasę, liczbę pasów, tonaż, struktury; M4 wyprowadza pasy, skrzyżowania, kierunki. `RoadSegment` nieprzeprojektowywany | 2, 5.2 (`RoadSegment`) |
| 3 | Bloki `StreamId` | **K-4**: bloki po 20; **M2 = 120–139**, nadmiarowy 1120–1219. Moja propozycja 64–96 odrzucona (kolizja z rozdanymi już zakresami). Wartości wpisane imiennie i niezmienne | 6 (tabela `StreamId`) |
| 4 | Walidator domknięcia grafu produktów | Wchodzi do CI **już w M2** — dok. 00 §5 zmieniony. M2 tworzy minimalny `data/goods/` (~60) i `data/recipes/` (~45) oraz walidator; **właścicielem docelowego schematu i katalogu ~400 towarów jest M6** | 5.8, 6, R5 |
| 5 | `SocialClass` | Należy do M3. M2 daje `income_tier: u8 (0..=4)` + `pop_capacity` | 5.5 (`District`) |
| 6 | Liczba dzielnic | 32 + poziom „osiedle" jako `Block.neighborhood: u16` | 5.4 (`Block`), 5.5 |
| 7 | Mieszkania vs. `target_pop` | M2 gwarantuje pojemność w widełkach T10; M3 skaluje populację i **nie** dostawia budynków | 7 (T10) |
| 8 | `wage_band` na stanowisku | Tak. `WageBand { min, median, max }` z `data/jobs/` × archetyp × `income_tier` | 5.6 (`Workplace`), 6 |
| 9 | Geometria ulic dla M3 | Odcinki centrolinii + długości (`street_lines`), **nie** graf (**K-2**) | 6 (kontrakt dla M3) |
| 10 | Składnia gramatyki wobec M11 | M11 potwierdził: dokłada **wyłącznie nowe warianty terminali** (szyldy, propy wnętrz, maszyny) w zdefiniowanym przeze mnie punkcie rozszerzenia. Zbiór operatorów, bryła i otwory zostają w M2. **Bump `schema_version` niepotrzebny** | 5.6, 6 (kontrakt dla M11) |
| 11 | Czy `GridSpec` potrzebuje trzeciego wymiaru | Nie. M11 potwierdził, że `CsrGrid::query_rect` (2D) + `Building.aabb: Aabb3` wystarczą: rzut piramidy widzenia i tak jest prostokątem, a cięcie poziomami dotyczy **jednego** budynku, więc indeks przestrzenny się do tego nie miesza. Elastyczność, za którą nie płacimy z góry — zmiana `GridSpec` po M4 dotknęłaby 4 crate'ów | 5.1, 6 (kontrakt dla M11) |
| 12 | Czy `floor_heights_dm` jest potrzebne obok liczby pięter | Tak — M11 potwierdził, że to różnica między działającym a popsutym cięciem poziomami: przy stałej wysokości cięcia przekrój przechodzi przez witrynę parteru usługowego, czyli tam, gdzie gracz patrzy najczęściej. `Unit.floor: i8` daje przy okazji piwnice | 5.6 (`Building`, `Unit`) |
| 13 | Podział `data/recipes/` ↔ `data/buildings/` | **M6: zgoda.** Receptura czysto ekonomiczna; `SiteArchetypeId`, strefa, min. parcela, `m2_per_workplace`, obsada `JobRoleId` + `wage_band` → `data/buildings/`. Po stronie M6 zostają `Recipe::machine_class` (abstrakcyjna zdolność, nie obiekt) i `Recipe::labour` (**osobominuty szarży, nie obsada etatowa**) — nie zlewać z moimi polami | 5.8 („Granica z M6") |
| 14 | Cykle bez wejścia z importu lub wydobycia | **M6: nie potrzebuje takiego przypadku.** Zostaje proste domknięcie Dowlinga–Galliera, **bez** przechodzenia na punkt stały. Cykle są u M6 normalne i pożądane, ale każdy daje się rozciąć importem albo złożem. Punkt wejścia rozpoznawany po jawnej fladze `Recipe::source`, nie po „inputs puste" | 5.8 (KROK 1, „Cykle") |
| 15 | Czy zapas startowy może być punktem wejścia domknięcia | **Nie — zaostrzenie wymuszone przez M6.** `data/scenarios/initial_stock.ron` pokrywa ~14 pierwszych dni, ale walidator nie może go traktować jako źródła: przeszedłby w CI łańcuch, który raz wystartuje i nigdy się nie odtworzy po wyczerpaniu zapasu — czyli dokładnie ta klasa błędu, którą walidator ma łapać | 5.8 (KROK 1) |
| 16 | Kształt pól katalogu towarów | `GoodUnit { Grams, Milliunits }` — dwie jednostki, `Volume` zawsze pochodna masy przez `density_g_per_l`. Importowalność jako `external_base_price: Option<Money>` (jedno pole, brak stanu „importowalny bez ceny"). Epoki dostępności importu **nie** są moim polem — właścicielem jest M10 (`TradeGood`). `yield` nie jest przechowywany, tylko liczony z mas i `duration_minutes`. `schema_version` per plik | 5.8 („Granica z M6") |
| 17 | Kalendarz w bilansie przepustowości | **K-1: rok = 360 dni (12 × 30).** `1440` w `daily_yield` to minuty na dobę i zostaje; roczne agregaty w KROKU 5 liczone przez 360, nie 365 | 5.8 („Granica z M6") |

### 9.2 Nadal otwarte — do rozstrzygnięcia przed startem wskazanego WP

| # | Decyzja | Blokuje | Propozycja M2 | Stan |
|---|---|---|---|---|
| 1 | Czy `engine/voxel` daje `ChunkWriter` bezpieczny dla równoległego zapisu, gdy budynek przecina granicę chunka | WP12 | `ChunkWriter::stage(building_index, runs)` + `commit_sorted()`; składanie po `building_index` | **czeka na M1** |
| 2 | Które z pięciu zgłoszonych rozszerzeń `TerrainQuery` M1 przyjmuje (`water_depth_at`, `deposit_at`, `soil_quality_at`, `prevailing_wind`, `flood_risk_at`) | WP3, WP7 | wszystkie pięć — każde ma co najmniej dwóch odbiorców (tabela w sekcji 6). Odrzucone = odpowiednie weto lub strefa wypada z generatora i ląduje w `GenerationReport` | **czeka na M1** |
| 3 | Moment, w którym wartość gruntu przestaje być statyczna (M5 transakcje / M10 pełny model) | WP15 | pole `Parcel.land_value_per_m2` zostaje, M5/M10 je nadpisują; **bez** traita `LandValueSource` — jedna implementacja nie potrzebuje abstrakcji | propozycja bez sprzeciwu |
| 4 | Czy epoka startowa dopuszcza pierścienie starsze od niej (starówka w mieście startującym w 1990) | WP7 | tak — pierścienie epok to historia zabudowy, nie stan techniki dostępnej graczowi | propozycja bez sprzeciwu |
| 5 | Czy `Institutional` (szkoły, szpitale) obsadza M2 jako `SiteSeed` należący do `City`, czy czeka na M8 | WP13 | M2 obsadza i daje stanowiska (M3 potrzebuje nauczycieli i lekarzy jako miejsc pracy); M8 dokłada budżet, politykę i jakość usługi | propozycja bez sprzeciwu |

---

## 10. Szacunek wielkości

| WP | Nazwa | Rozmiar |
|---|---|---|
| WP1 | `spatial`: siatka i indeksy statyczne | M |
| WP2 | `spatial`: quadtree, grid dynamiczny, pola skalarne | M |
| WP3 | Punkty wejścia do miasta | S |
| WP4 | L-system arterii | L |
| WP5 | Mosty, tunele, kolej towarowa | M |
| WP6 | Kwartały z grafu dróg | M |
| WP7 | Strefowanie z kwotami + pierścienie epok | M |
| WP8 | Sieć lokalna + podział na parcele | L |
| WP9 | Dzielnice i ich tożsamość | M |
| WP10 | Język gramatyki + parser + walidator | M |
| WP11 | Silnik derywacji gramatyki | L |
| WP12 | Zapis voxeli + wnętrza logiczne | L |
| WP13 | Archetypy zakładów i szablony łańcuchów | M |
| WP14 | Domknięcie łańcuchów produktowych | M |
| WP15 | Wycena gruntu (3 przebiegi) | M |
| WP16 | Nakładka UI + karta inspekcji | S |
| WP17 | Testy spójności + raport generacji | M |

Rozkład: 4 × L, 11 × M, 2 × S. Ciężar fazy leży w geometrii podziału na parcele (WP8)
i w gramatyce budynków (WP11 + WP12) — te trzy WP to ok. 45% pracy fazy i tam należy
zaplanować rezerwę.
