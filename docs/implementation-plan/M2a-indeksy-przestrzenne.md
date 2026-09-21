# M2a — Indeksy przestrzenne

Podfaza 1 z 5 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M0 (`ecs`, `core`, `jobs`). |
| **Pakiety robocze** | WP1, WP2 |
| **Projekt techniczny** | §5.1 |
| **Wynik do pokazania** | `cargo bench -p magnat-spatial` na danych syntetycznych: cztery struktury odpowiadają na zapytania w budżecie, bez generatora miasta. |
| **Kryterium zamknięcia** | Kryteria WP1 i WP2: parity z brute-force dla 10 tys. zapytań, przebudowa gridu bit-identyczna w dwóch przebiegach. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M2b-szkielet-transportu.md` |

Crate `engine/spatial` w całości: siatka chunków, CSR-grid statyczny, grid dynamiczny, quadtree parcel, pola skalarne. Bez ani jednej linii o mieście — to jest biblioteka, z której korzysta cała reszta fazy oraz fazy M4–M10.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| [x] WP1 | `spatial`: siatka i indeksy statyczne | M0 | `GridSpec`, `CellId`, `morton2`, `CsrGrid`, `CategoryGrid`, zapytania `query_radius` / `query_rect` / `k_nearest` / `query_radius_batch` | benchmark: 1 mln zapytań promieniowych R=250 m po 300 tys. encji ≤ 180 ms na 8 wątkach; test własnościowy: wynik gridu == wynik brute-force dla 10 tys. losowych zapytań |
| [x] WP2 | `spatial`: quadtree, grid dynamiczny, pola skalarne | WP1 | `ParcelTree` (quadtree AABB), `DynamicGrid` (przebudowa sortowaniem zliczającym), `ScalarField`, `multi_source_dijkstra` | zapytanie punkt→parcela ≤ 2 µs; przebudowa 400 tys. encji ≤ 3 ms na 8 wątkach i **bit-identyczna** przy 2 przebiegach; Dijkstra 1 mln komórek ≤ 400 ms |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

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
pub struct CsrGrid<T: Copy> { spec: GridSpec, starts: Vec<u32>, pts: Vec<Vec2>, items: Vec<T> }
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
    pub fn rebuild(&mut self, pos: &[Vec2], ids: &[T], pool: &JobPool);   // O(n + cells)
    pub fn for_each_in_radius(&self, c: Vec2, r: f32, f: impl FnMut(Vec2, T));
    pub fn k_nearest(&self, c: Vec2, k: usize, out: &mut Vec<(f32, T)>);  // ekspansja pierścieniowa
}

/// Quadtree nad AABB parcel. Parcele różnią się powierzchnią o 3 rzędy wielkości
/// (działka R2 ~400 m², pole rolne ~20 ha) — siatka jednorodna tu nie działa.
pub struct ParcelTree { nodes: Vec<QNode>, aabbs: Vec<Aabb2>, ids: Vec<ParcelId> }
impl ParcelTree {
    pub fn build(items: &[(Aabb2, ParcelId)]) -> Self;   // bulk, bottom-up po kluczu Mortona
    pub fn at_point(&self, p: Vec2, contains: impl Fn(ParcelId) -> bool) -> Option<ParcelId>;
    pub fn query_rect(&self, a: Aabb2, out: &mut Vec<ParcelId>);
    pub fn query_segment(&self, a: Vec2, b: Vec2, out: &mut Vec<ParcelId>);
}
// Parametry: max_depth = 12, bucket = 16, brak wstawiania w M2 (parcele statyczne);
// insert/remove dodaje M5 (podziały i scalenia działek).

/// Pole skalarne na GridSpec — nośnik wszystkich „map wpływu" generatora i nakładek UI.
pub struct ScalarField { spec: GridSpec, data: Vec<u16>, full_scale: f32 }  // znorm. 0..=65535
impl ScalarField {
    pub fn sample(&self, p: Vec2) -> f32;                    // dwuliniowo, O(1)
    pub fn sample_scaled(&self, p: Vec2) -> f32;             // sample * full_scale
    pub fn multi_source_dijkstra(spec: GridSpec, sources: &[CellId],
                                 cost: impl Fn(CellId, CellId) -> u32) -> Self;
    pub fn decay_from(spec: GridSpec, sources: &[(CellId, f32)], half_life_m: f32) -> Self;
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

**Zmiany wpisane po M2a** (CLAUDE.md: rozjazd kodu z planem jest gorszy
niż błąd w planie).

| # | Co | Dlaczego |
|---|---|---|
| A1 | `ParcelTree::at_point` bierze **predykat** `impl Fn(ParcelId) -> bool`, nie `&PolyArena` | `PolyArena` jest typem `sim/world` (M2b §5.2), a `engine/spatial` leży pod nim w grafie zależności. AABB zawęża kandydatów do garstki, o wielokącie rozstrzyga wywołujący — indeks nie musi znać geometrii parceli, żeby ją znaleźć |
| A2 | `ParcelTree::build` buduje **z góry na dół, ćwiartkami, ze straddlerami** (element na granicy zostaje w rodzicu), nie bottom-up po kluczu Mortona | Klucz Mortona porządkuje punkty, a nie prostokąty; przy rozpiętości trzech rzędów wielkości (o którą w tej strukturze chodzi) pole rolne i tak nie zeszłoby do liścia. Zmierzone: 42 tys. parcel → 4916 węzłów, `at_point` 0,32 µs |
| A3 | `DynamicGrid::rebuild` bierze `&JobPool`, nie `&JobScope` | `engine/jobs` (M0) nie eksportuje `JobScope` jako typu wejściowego systemu; pula jest tym, co dostaje każdy system symulacji |
| A4 | `ScalarField` niesie `full_scale: f32` (wartość fizyczna odpowiadająca 65535) | Bez tego pole odległości po drogach traci skalę przy kwantyzacji do `u16` i M2c musiałby ją liczyć drugi raz. `decay_from` dostaje `GridSpec` — bez niego nie ma na czym rysować |
| A5 | `CsrGrid` trzyma **pozycje** obok elementów (`pts`) | Filtr odległości bez skoku do pamięci encji. Koszt: 8 B na encję; zysk: zapytanie promieniowe jest jednym przebiegiem strumieniowym |
| A6 | `CellId` jest **row-major**, `morton2` zostaje do porządkowania encji i sortowania zapytań wsadowych | Przy row-major komórki jednego wiersza leżą w CSR obok siebie: 81 komórek zapytania to 9 ciągłych odczytów, a nie 81 skoków. Szerokość wiersza liczona z cięciwy koła, nie z kwadratu opisanego |
| A7 | **Budżet zapytania promieniowego uzupełniony o gęstość** — patrz niżej | Budżet „1 mln zapytań R = 250 m po 300 tys. encji ≤ 180 ms" wycenia `k` (liczbę komórek), a przemilcza `m` (liczbę trafień), które przy tym promieniu dominuje pięciokrotnie |

**Korekta budżetu (A7).** Koszt zapytania promieniowego to O(k + m), a przy R = 250 m składnik
`m` jest kilkakrotnie większy od `k`: zapytanie zwraca **tyle encji, ile ich jest w kole
o powierzchni 0,196 km²**. Ta sama struktura, te same 300 tys. encji i ten sam milion zapytań
dają więc dwa różne wyniki, zależnie od tego, na jakim obszarze te encje leżą:

| Scenariusz | Gęstość | Trafień na zapytanie | 1 mln zapytań / 8 wątków |
|---|---|---|---|
| cała mapa 16 × 16 km (256 km²) | 1,2 tys./km² | ~230 | **111 ms** ✓ wobec 180 ms |
| obszar zurbanizowany 70 km² | 4,3 tys./km² | ~860 | **275 ms** ✗ wobec 180 ms |

Drugi wiersz nie jest wadą implementacji: 860 trafień × 12 B to 10 KB przepisane na zapytanie,
czyli przy 1 mln zapytań ~10 GB ruchu z L3 — pomiar leży na przepustowości pamięci, nie na
arytmetyce. Zawężenie wiersza do cięciwy koła (A6) ścięło liczbę kandydatów o ~40 % i **nie
zmieniło czasu**, co tę diagnozę potwierdza.

Wniosek wiążący dla faz zależnych: **zapytanie promieniowe o dużym R po wszystkich encjach nie
jest ścieżką, którą gra ma chodzić**. Do „najbliższe obiekty" służy `k_nearest` (k = 15 → 1,09 µs),
do „oferty kategorii K w okolicy" — `CategoryGrid` (0,28 µs, PRD §17.5 przewiduje 3–15
kandydatów). Budżet 180 ms obowiązuje dla gęstości ≤ ~1,5 tys. encji/km² w jednym indeksie;
indeks obejmujący cały obszar zurbanizowany dzieli się na kategorie, zamiast rosnąć.

**Wynik pomiarów** (`cargo bench -p magnat-spatial`, 8 rdzeni, profil `bench`):

| Benchmark | Zmierzone | Budżet §5.1 | |
|---|---|---|---|
| `csr/1M zapytań R=250, 8 wątków (mapa 256 km²)` | 111 ms | ≤ 180 ms | ✓ |
| `csr/1M zapytań R=250, 8 wątków (miasto 70 km²)` | 275 ms | ≤ 180 ms | ✗ (A7) |
| `csr/pojedyncze zapytanie R=250` | 2,09 µs | — | |
| `csr/k=15 najbliższych` | 1,09 µs | ≤ 8 µs | ✓ |
| `csr/query_rect 200×200 m` | 0,72 µs | — | |
| `csr/budowa indeksu 300 tys.` | 1,86 ms | — | |
| `dynamic/przebudowa 400 tys., 8 wątków` | 1,86 ms | ≤ 3 ms | ✓ |
| `dynamic/przebudowa 400 tys., 1 wątek` | 2,02 ms | — | |
| `tree/punkt → parcela` | 0,32 µs | ≤ 2 µs | ✓ |
| `tree/prostokąt 300×300 m` | 2,69 µs | — | |
| `tree/odcinek 500 m` | 4,04 µs | — | |
| `field/próbka dwuliniowa` | 7,7 ns | ≤ 30 ns | ✓ |
| `field/dijkstra 1 mln komórek` | 35,2 ms | ≤ 400 ms | ✓ |
| `category/oferty kategorii w R=600` | 0,28 µs | 3–15 kandydatów | ✓ |

Przebudowa `DynamicGrid` skaluje się słabo (2,02 → 1,86 ms przy ośmiu wątkach), bo histogram
i suma prefiksowa po 70 tys. komórek są sekwencyjne i zjadają połowę czasu. Zostaje tak, bo
budżet jest spełniony z zapasem, a równoległy histogram to trzy razy więcej kodu w miejscu,
które nie jest wąskim gardłem.

**Czego tu nie ma i dlaczego.** Brak R-tree (quadtree wystarcza dla statycznych AABB),
brak BVH (to nie jest raytracer — pikowanie myszą robi render przez bufor identyfikatorów),
brak rastra parcel 2 m (256 MB przy mapie 16×16 km; `ParcelTree::at_point` w 2 µs jest
wystarczające). Raster per chunk to zapisana ścieżka rozbudowy, gdyby M4 zaczął robić
miliony zapytań punkt→parcela na tick.
