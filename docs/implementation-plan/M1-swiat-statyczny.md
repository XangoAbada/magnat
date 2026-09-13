# M1 — Świat statyczny

Status: plan wykonawczy.
Nadrzędny: `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, LOD, dane, testy — **nie redefiniujemy**).
Źródło wymagań: `PRD_Magnat.md` §4.1, §4.2 (Etap 1–2), §15.1–15.3, §16.2, §16.3, §17.1, §17.7, §19 (M1), §20.2.

---

## 1. Cel fazy i artefakt końcowy

**Cel:** z 64-bitowego seeda i zestawu parametrów powstaje deterministyczny, oglądalny krajobraz —
teren uformowany erozją rzeczną, warstwy geologiczne ze złożami, klimat i biomy — renderowany
własnym pipeline'em voxelowym z cyklem dobowym, w budżetach FPS z §20.2.

**Artefakt końcowy (co da się uruchomić i zobaczyć):**

1. `magnat --seed 0xC0FFEE --size 8km --region rzeczny --epoch 1990 --profile przemyslowe`
   otwiera okno: krajobraz voxelowy 8×8 km, swobodna kamera od orbity nad mapą do poziomu gruntu,
   płynne przewijanie bez zacięć, słońce przesuwające się po niebie (skrót: przyspieszenie czasu ×1000),
   cienie kaskadowe, rzeki z ujściem, lasy/pola/mokradła rozłożone wg klimatu.
2. Klawisz `F3` → nakładki debug na terenie: wysokość, akumulacja spływu, klasa wody,
   biom, temperatura stycznia/lipca, opady, warstwa geologiczna na zadanej głębokości, mapa złóż.
3. `tools/headless generate --seed X --size 16km --out world.mgw` — pełna generacja bez GPU,
   raport czasów per przebieg, hash terenu, statystyki (długość rzek, powierzchnia biomów,
   bilans złóż w gramach).
4. `tools/headless verify --seed X --runs 2` — dwa przebiegi tego samego seeda dają identyczny
   hash terenu, klimatu i listy złóż (bit w bit).
5. Inspektor devtools: klik w teren → kolumna geologiczna (materiał + miąższość każdej warstwy),
   głębokość wody gruntowej, biom, `ClimateCell`, złoża przecinające kolumnę z objętością,
   koncentracją i pozostałymi zasobami.

**Czego artefakt jeszcze nie ma:** dróg, budynków, ludzi, żadnej ekonomii. To jest pusty, żywy
krajobraz — podkład, na którym M2 postawi miasto.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zawartość |
|---|---|
| `engine/voxel` (właściciel) | Chunk 32³, paleta ≤256 materiałów/chunk, składowanie Uniform/RLE/Dense, greedy meshing z voxelowym AO na job systemie, agregacja LOD 2×/4×/8×, streaming i rezydencja chunków, API edycji (dla M2) |
| `engine/render` (właściciel) | Bootstrap `wgpu` (device/surface/frame graph), pipeline voxelowy z arena-bufferem i multi-draw indirect, cienie kaskadowe, clustered shading, słońce i cykl dobowy, niebo, woda, clipmap dalekiego terenu, post-processing (tonemap + FXAA), hook nakładek danych |
| Kamera (§15.2) | Rig orbitalno-swobodny (obrót, pochylenie, zoom orbita→ulica), tryb pierwszoosobowy, płaszczyzna cięcia poziomego jako uniform (M2/M11 z niej korzystają) |
| `sim/world` (właściciel, część terenowa) | `WorldGenParams` (§4.1), Etap 1 (teren, hydrologia, geologia, złoża), Etap 2 (klimat, biomy), kontrakt `TerrainQuery` dla pozostałych faz |
| Rozszerzenia | `core`: warianty `StreamId` 100–119; `ecs`: rejestracja zasobu świata i komponentu `Deposit`; `devtools`: inspektor terenu i nakładki |

### Nie wchodzi

| Element | Faza |
|---|---|
| Punkty wejścia do miasta, arterie, L-system dróg, mosty/tunele jako obiekty (Etap 3) | M2 |
| Strefy, parcele, dzielnice, gramatyka budynków (Etap 4–6) | M2 |
| Populacja, gospodarka bazowa, historia „na sucho" (Etap 7–9) | M3, M7, M10 |
| Pogoda jako **zdarzenie** (burza, susza, powódź) i sezonowość popytu | M8 — M1 daje tylko statyczny roczny cykl klimatu jako dane wejściowe |
| Rolnictwo: co rośnie, plon/ha | M5/M6 — M1 daje `soil_fertility` + biom + klimat, nie model agronomiczny |
| Wydobycie i wyczerpywanie złóż (kopalnia) | M6 — M1 daje model złoża zdolny to udźwignąć (patrz §5.5) |
| Detale voxelowe 0,25 m, animacje, wnętrza, impostory dzielnic, pogoda wizualna (śnieg/deszcz/mgła/dym), audio, TAA | M11 |
| Profilowanie do celów docelowych, metropolia 400 tys., zapis w tle | M11/M12 — M1 pilnuje własnych budżetów, nie całościowych |
| Instancing encji dynamicznych (ludzie, pojazdy) | M3/M4 — M1 buduje pipeline tak, by się dopiął (osobny pass, wspólny frame graph) |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizujemy w M1 |
|---|---|
| §4.1 Parametry generacji | Pełne: seed, wielkość, epoka, profil, region, trudność → `WorldGenParams`. W M1 konsumujemy **region** i **wielkość** wprost; **profil** i **epoka** wpływają na wagi typów złóż i pokrycia terenu; **trudność** tylko przechowujemy i przekazujemy dalej |
| §4.2 Etap 1 — Teren | Całość: szum wielooktawowy, symulacja hydrologiczna, warstwy geologiczne, złoża z objętością i koncentracją |
| §4.2 Etap 2 — Klimat i biomy | Całość: mapa temperatury i opadów z rocznym cyklem, lasy/pola/mokradła |
| §4.2 Etap 3–10 | Poza zakresem (M2+) |
| §4.3 Struktura przestrzenna | Tylko poziom „Świat"; dzielnice i niżej — M2 |
| §15.1 Styl | Voxel 1 m dla terenu; paleta ograniczona per region/epoka. Detal 0,25 m — M11 |
| §15.2 Kamera | Całość |
| §15.3 Oświetlenie | Cykl dobowy, cienie, przygotowanie pod światła punktowe (clustered). Pory roku wizualnie i pogoda — M11 |
| §16.2 Moduły | Tworzymy `engine/voxel`, `engine/render`, terenową część `sim/world` |
| §16.3 Renderer voxelowy | Wszystko poza impostorami dzielnic (M11) i instancingiem encji (M3/M4) |
| §17.1 Czas | Render jako niezależny tick z interpolacją ze snapshotu; słońce funkcją `SimMinute` |
| §17.7 Pamięć | M1 deklaruje swój wycinek budżetu (§5.9) |
| §20.2 Metryki techniczne | 60 FPS widok dzielnicy / 30 FPS widok miasta — kryterium akceptacji §7 |

---

## 4. Pakiety robocze (WP)

Kolejność jest ścieżką krytyczną od góry do dołu; pakiety w tej samej grupie można prowadzić równolegle.

### Grupa W — generator (`sim/world`), headless, bez GPU

| WP | Opis | Zależy od | Kryterium ukończenia | Status |
|---|---|---|---|---|
| **W1 — Parametry i szkielet potoku** | `WorldGenParams` (§4.1) + walidacja; `GenPipeline` jako lista nazwanych przebiegów; rejestracja `StreamId` 100–119; profil czasu per przebieg; `WorldGenReport` | M0 (`core`, `jobs`) | `headless generate` przechodzi przez wszystkie przebiegi-zaślepki, raportuje czasy, wynik jest pusty ale deterministyczny | [x] |
| **W2 — Etap 1a: baza wysokości** | Maska lądu, domain-warped fBm, ridged multifractal dla regionu górskiego, poziom morza, profil brzegu dla regionu nadmorskiego; wszystko na siatce roboczej 4 m | W1 | Mapa wysokości 4096² dla 16 km w ≤ 400 ms; wizualny sanity-check w `headless preview --png`; hash stabilny między przebiegami | [x] |
| **W3 — Etap 1b: hydrologia** | Priority-flood + ε, kierunki D8 z deterministycznym tie-breakiem, akumulacja spływu, erozja stream-power metodą niejawną Brauna–Willetta, dyfuzja zboczowa, erozja termiczna (kąt usypu), klasyfikacja wód (morze/jezioro/rzeka z rzędem), wektorowa sieć koryt, głębokość i szerokość koryta z geometrii hydraulicznej | W2 | Rzeki płyną z gór do morza bez pętli i bez lokalnych zagłębień; brak „wiszących" cieków; budżet czasu §5.9; hash stabilny | [x] |
| **W4 — Etap 1c: geologia i złoża** | Analityczny stos `TerrainLayer` (gleba→ił/piasek→skała osadowa→podłoże), modulacja miąższości szumem i nachyleniem; złoża: Poisson-disk rozmieszczenie zarodków, typ z wag region×profil×epoka, kształt (elipsoida / pokład / warstwa wodonośna), przycięcie do formacji nośnej, objętość + koncentracja + zasoby w gramach | W3 (podłoże zależy od wypiętrzenia i erozji) | Inspektor pokazuje kolumnę geologiczną w dowolnym punkcie; bilans: `reserves == f(volume, concentration, density)` w arytmetyce całkowitej; test własnościowy sumy mas | [ ] |
| **W5 — Etap 1d: materializacja 1 m** | Deterministyczny upsampling 4 m → 1 m (bicubic + szum detalu), wcięcie koryt z sieci wektorowej, `ColumnSource` zamieniający kolumnę na runy voxelowe dla `engine/voxel` | W4 | Upsampling kafla 256×256 m w ≤ 0,5 ms; brak schodków i artefaktów na stykach kafli; koryto rzeki na 1 m pokrywa się z wektorem ±1 m | [ ] |
| **W6 — Etap 2: klimat i biomy** | Temperatura bazowa z szerokości geograficznej regionu + gradient pionowy + kontynentalność + miesięczna sinusoida; wiatr dominujący, adwekcja wilgoci, opad orograficzny i cień opadowy; klasyfikacja biomu (Whittaker + nadpisania lokalne: mokradło przy wysokiej akumulacji i małym spadku, woda, skała, śnieg); żyzność gleby; głębokość wód gruntowych | W3 (potrzebuje akumulacji i wysokości) | Mapy miesięczne 12×; region górski daje widoczny cień opadowy; region pustynny nie generuje lasu; hash stabilny | [ ] |
| **W7 — Kontrakt `TerrainQuery` + devtools** | Publiczne API dla M2+ (§6), nakładki debug, inspektor terenu, serializacja świata do `.mgw` | W5, W6 | M2 może odpytać wysokość, spławność, przeszkody, złoża i biom bez znajomości wnętrza generatora; `.mgw` ≤ 25 MB dla 16 km | [ ] |

### Grupa V — `engine/voxel`

| WP | Opis | Zależy od | Kryterium ukończenia | Status |
|---|---|---|---|---|
| **V1 — Chunk, paleta, składowanie** | `Chunk`, `Palette`, `VoxelMaterial`, rejestr materiałów z `data/materials/*.ron`, warianty `Uniform`/`Rle`/`Dense`, konwersje między nimi, dostęp `get`/`set` | M0 | Round-trip Dense→Rle→Dense bit w bit; test fuzz na losowych chunkach; `Uniform` zajmuje ≤ 32 B | [x] |
| **V2 — Greedy meshing + AO** | Meshing 6 kierunków, scalanie prostokątów po (materiał, AO, światło), voxelowe AO per wierzchołek liczone z 3 sąsiadów rogu, pakowany format wierzchołka 8 B, uruchomienie na job systemie | V1 | Chunk terenowy: ≤ 900 quadów, ≤ 0,8 ms na rdzeń; test „mesh nie przecieka" (brak dziur między chunkami — sąsiedztwo brane z 1-voxelowej otoczki) | [ ] |
| **V3 — LOD 2×/4×/8×** | Agregacja **bezpośrednio z `ColumnSource`**, nie z voxeli LOD0; materiał przez głosowanie większościowe z tie-breakiem po najniższym `MaterialId`; pusty gdy >50% powietrza | V2, W5 | LOD3 chunk generuje się ≤ 1,5 ms; brak szwów geometrycznych na granicy pierścieni LOD (skirt) | [ ] |
| **V4 — Streaming i edycja** | Rezydencja sterowana kamerą, pierścienie LOD z histerezą, kolejka priorytetowa żądań, pula chunków z twardym budżetem pamięci, sub-alokator bufora GPU z listą wolnych bloków, `edit_region`/`stamp_prefab` + `revision` unieważniający mesh | V3, R2 | Przelot kamery 2 km wzdłuż mapy: brak spadku poniżej 55 FPS, brak przekroczenia budżetu pamięci, brak widocznego „wyskakiwania" chunków | [ ] |

### Grupa R — `engine/render`

| WP | Opis | Zależy od | Kryterium ukończenia | Status |
|---|---|---|---|---|
| **R1 — Bootstrap i frame graph** | Przejęcie od M0 inicjalizacji `wgpu` (device, surface, konfiguracja), frame graph z jawną deklaracją zasobów i barier, podwójnie buforowany snapshot widocznych danych (§17.1), pomiar czasu GPU (timestamp queries) | M0, V1 | Pusty frame graph renderuje niebo i mierzy czas każdego passu; zmiana rozmiaru okna bez wycieków | [ ] |
| **R2 — Pipeline voxelowy** | Jeden arena-buffer wierzchołków/indeksów, per-chunk dane w SSBO, tabela materiałów w SSBO, culling frustum na CPU + `multi_draw_indexed_indirect` (fallback: draw per chunk), depth prepass | R1, V2 | 3000 chunków LOD0 rysowanych ≤ 6 ms GPU; fallback działa na backendzie bez multi-draw | [ ] |
| **R3 — Cienie kaskadowe** | 4 kaskady, stabilizacja (snap do texela), dobór podziałów logarytmiczno-liniowy, osobna lista widoczności per kaskada, PCF 3×3 | R2 | Brak migotania cieni przy obrocie kamery; ≤ 2 ms GPU; cienie poprawne od orbity do poziomu ulicy | [ ] |
| **R4 — Clustered shading, słońce, niebo** | Froxele 16×9×24, przypisanie świateł w compute, kierunek i barwa słońca z `SimCalendar` (dzień roku 0..359, K-1) + szerokość geograficzna regionu, LUT nieba per wysokość słońca, ambient dwustrefowy (niebo/grunt) po normalnej, licznik `ClusterOccupancy` w devtools | R3 | Doba w 30 s realnych: ciągła zmiana oświetlenia bez skoków; **4096** świateł testowych ≤ 0,4 ms na przypisanie; `ClusterOccupancy` pokazuje histogram i maksimum na klaster w czasie rzeczywistym (przyrząd kalibracyjny dla M2/M11 — patrz §6.1) | [ ] |
| **R5 — Woda i daleki teren** | Forward pass wody (fresnel, mgła głębinowa, animowane normalne, brzeg miękki po depth), geometry clipmap terenu poza zasięgiem LOD3 z mapy 4 m | R4, W3 | Widok miasta z orbity: horyzont wypełniony, spójny z terenem voxelowym w miejscu przejścia | [ ] |
| **R6 — Post-processing i nakładki** | HDR, ekspozycja z krzywej pory dnia, tonemap ACES-fitted, FXAA, mgła atmosferyczna, `TerrainOverlay` (tekstura pola skalarnego + paleta) — hook dla M2 | R5 | Wszystkie nakładki debug z §1 działają przez ten sam mechanizm, którego użyje M2 | [ ] |
| **C1 — Kamera (§15.2)** | Rig: orbita (obrót/pochylenie/zoom), swobodny lot, tryb pierwszoosobowy, ograniczenia (kolizja z terenem, clamp pochylenia), interpolacja zoomu z FOV, `ClipPlane` jako uniform | R1 | Przejście orbita → poziom ulicy → pierwsza osoba płynne, bez przenikania przez teren | [ ] |

### Grupa X — jakość

| WP | Opis | Zależy od | Kryterium ukończenia | Status |
|---|---|---|---|---|
| **X1 — Testy, benchmarki, CI** | Test determinizmu terenu, testy własnościowe złóż, benchmarki `criterion` (meshing, LOD, hydrologia), scena benchmarkowa FPS z ustalonym przelotem kamery, budżety jako asercje | wszystkie | CI zielone; raport budżetów (pamięć, czas generacji, FPS) publikowany jako artefakt builda | [ ] |

---

## 5. Projekt techniczny

### 5.1 Geometria świata — ustalenia liczbowe

| Wielkość | Wartość | Uwaga |
|---|---|---|
| Rozdzielczość pozioma | 1 m | §4.2 |
| Rozdzielczość pionowa | 0,5 m | §4.2 |
| Chunk | 32³ voxeli = **32 × 32 m poziomo × 16 m w pionie** | Chunk jest niesymetryczny w metrach — to celowe |
| Zakres pionowy świata | −64 m … +192 m n.p.m. = 512 voxeli = **16 warstw chunków** | Poziom morza = voxel Z 128 |
| Mapa 4×4 km | 128 × 128 × 16 chunków = 262 144 slotów | |
| Mapa 16×16 km | 512 × 512 × 16 chunków = **4 194 304 sloty** | Gęsto = 134 GB → **świat nie jest składowany jako voxele** |
| Siatka robocza generatora | **4 m** (4096² dla 16 km) | Hydrologia i erozja liczone tu; 1 m to deterministyczna pochodna |
| Siatka klimatu | **256 m** (64² dla 16 km) | Klimat nie ma struktury poniżej tej skali |

**Decyzja fundamentalna:** voxele są **materializowane na żądanie** z reprezentacji kolumnowej,
nigdy składowane w całości. Trwałe są tylko: mapa wysokości 4 m po erozji, klasa i głębokość wody,
wektorowa sieć koryt, lista złóż, siatka klimatu i **rzadka nakładka edycji gracza**.
Geologia jest funkcją analityczną — nie zajmuje pamięci wcale.

### 5.2 `engine/voxel` — struktury

```rust
pub const CHUNK_DIM: u32 = 32;
pub const CHUNK_VOXELS: usize = 32 * 32 * 32; // 32768

/// Indeksowanie liniowe: idx = (y * 32 + x) * 32 + z  — Z najszybsze.
/// Powód: w terenie najdłuższe ciągi identycznych voxeli są pionowe
/// (kolumna powietrza, kolumna skały) → RLE po Z daje najlepszą kompresję.
#[inline] pub fn lin(x: u32, y: u32, z: u32) -> usize { ((y * 32 + x) * 32 + z) as usize }

pub struct MaterialId(pub u16);   // rejestr globalny, ≤ 65535
pub struct LocalIdx(pub u8);      // indeks w palecie chunka, ≤ 256 (PRD §16.3)

pub struct VoxelMaterial {
    pub key: Box<str>,            // "soil", "granite", "coal_ore", "water" — klucz w zapisie gry
    pub albedo: [u8; 3],
    pub roughness: u8,
    pub emissive: u8,
    pub flags: MaterialFlags,     // SOLID|LIQUID|TRANSPARENT|DIGGABLE|BUILDABLE|SUPPORTS_VEGETATION
    pub hardness: u8,             // 0..=255 — koszt wydobycia, konsumuje M6
    pub density_kg_m3: u16,       // potrzebne do przeliczenia objętość ↔ masa
    pub bearing_capacity: u8,     // nośność gruntu, konsumuje M2 (koszt fundamentów)
}

/// Paleta lokalna chunka. Terenowy chunk ma typowo 2–6 pozycji.
pub struct Palette {
    entries: smallvec::SmallVec<[MaterialId; 8]>,
}
impl Palette {
    pub fn intern(&mut self, m: MaterialId) -> Option<LocalIdx>; // None gdy przekroczono 256
    pub fn resolve(&self, i: LocalIdx) -> MaterialId;
}

#[derive(Clone, Copy)]
pub struct Run { pub len: u16, pub idx: LocalIdx }   // 4 B z wyrównaniem

pub enum ChunkStorage {
    /// Cały chunk jednym materiałem — powietrze, lita skała, głębia wody.
    /// ~95% slotów świata. Koszt: 2 B.
    Uniform(MaterialId),
    /// Terenowa powierzchnia. Typowo 1500–4000 runów ≈ 6–16 KB.
    Rle(Box<[Run]>),
    /// Chunk edytowany (wykop kopalni M6, fundament M2). 32 KB.
    Dense(Box<[LocalIdx; CHUNK_VOXELS]>),
}

pub struct ChunkCoord { pub x: i32, pub y: i32, pub z: i16 }  // w jednostkach chunków

pub enum ChunkState { Unloaded, Queued, Generating, Ready, Meshing, Resident }

pub struct Chunk {
    pub coord: ChunkCoord,
    pub lod: u8,                  // 0 = 1 m, 1 = 2 m, 2 = 4 m, 3 = 8 m
    pub palette: Palette,
    pub storage: ChunkStorage,
    pub revision: u32,            // ++ przy edycji → unieważnia mesh i LOD-y rodziców
    pub state: ChunkState,
    pub mesh: Option<MeshHandle>, // blok w arenie GPU
}
```

**API:**

```rust
pub trait ColumnSource: Send + Sync {
    /// Jedyne wejście voxeli do świata. Implementuje sim/world (WP-W5).
    /// Musi być czyste i deterministyczne — wołane z dowolnego workera.
    fn fill_chunk(&self, coord: ChunkCoord, lod: u8, out: &mut ChunkBuilder);
}

pub struct VoxelWorld { /* pula chunków, indeks przestrzenny, budżet */ }
impl VoxelWorld {
    pub fn new(src: Arc<dyn ColumnSource>, budget: VoxelBudget) -> Self;
    pub fn update_residency(&mut self, cam: &CameraState, jobs: &JobScope);
    pub fn get(&self, pos: IVec3) -> MaterialId;
    pub fn edit_region(&mut self, cmd: VoxelEditCmd) -> EditResult;   // dla M2, M6
    pub fn stamp_prefab(&mut self, origin: IVec3, p: &VoxelPrefab);   // dla M2 (budynki)
    pub fn stats(&self) -> VoxelStats;                                 // dla devtools
}
```

### 5.3 Greedy meshing i voxelowe AO

Algorytm klasyczny, sześć przebiegów (3 osie × 2 zwroty). Dla każdego z 32 przekrojów:

1. Zbuduj maskę 32×32 widocznych ścian — ściana widoczna, gdy voxel jest `SOLID`,
   a sąsiad w kierunku przebiegu nie jest.
2. Dla każdej ściany policz **AO per róg**: wartość 0..3 z trzech sąsiadów rogu
   (dwa boczne + narożny; reguła: `if side1 && side2 { 0 } else { 3 - side1 - side2 - corner }`).
3. Scalaj prostokąty **tylko przy identycznej czwórce** `(MaterialId, [ao;4], sun_light, block_light)`.
   To fragmentuje quady na zboczach — świadomy koszt, zmierzony w WP-V2.
4. Emituj 4 wierzchołki + 6 indeksów; przekątną quada wybierz po AO (unikanie artefaktu „anisotropii").

**Sąsiedztwo na granicy chunka:** `ChunkBuilder` materializuje otoczkę 1 voxela z tego samego
`ColumnSource` (nie z sąsiedniego chunka) — dzięki temu meshing chunka jest niezależny od
kolejności ładowania sąsiadów i nie wymaga rememeshingu przy ich pojawieniu się.
Koszt: 34³ zamiast 32³ = +20% pracy generatora. Warte tego.

**Format wierzchołka — 8 B:**

```
lo: u32   x:6 | y:6 | z:6 | ao:2 | normal:3 | _:9      (pozycja 0..32 w chunku)
hi: u32   material:16 | sun:4 | block:4 | _:8
```

Pozycja bazowa chunka i skala LOD idą w per-chunk SSBO, nie w wierzchołku.

**Koszt (mierzony w WP-V2, budżet):** chunk terenowy 32³ → 200–900 quadów, 0,3–0,8 ms na rdzeń.
Meshing biegnie na `engine/jobs` w **puli niskiego priorytetu** — nigdy nie może zagłodzić symulacji.

### 5.4 LOD i streaming

Agregacja LOD **z `ColumnSource`, nie z voxeli LOD0** — LOD3 nie wymaga rezydentnego LOD0,
co jest warunkiem widoku całego miasta w budżecie pamięci.

| LOD | Skala | Promień od kamery | Chunk pokrywa |
|---|---|---|---|
| 0 | 1 m | ≤ 192 m | 32 × 32 × 16 m |
| 1 | 2 m | ≤ 512 m | 64 × 64 × 32 m |
| 2 | 4 m | ≤ 1400 m | 128 × 128 × 64 m |
| 3 | 8 m | ≤ 4000 m | 256 × 256 × 128 m |
| clipmap | 4 m heightmap | > 4000 m | — |

- Materiał agregatu: głosowanie większościowe po `2^lod` sześcianie, remis → najniższy `MaterialId`.
  Pusty, gdy >50% powietrza (inaczej zbocza puchną).
- **Histereza ±15%** na promieniach — chunk zmienia LOD dopiero po przekroczeniu progu
  powiększonego/pomniejszonego o 15%, żeby kamera stojąca na granicy nie miotała się w kółko.
- Szwy między pierścieniami: „skirt" — pionowa ramka o wysokości jednego voxela wyższego LOD
  po obwodzie chunka. Tanie, wystarczające; geomorphing dopiero gdyby było widać (nie planujemy).
- Kolejka żądań: `BinaryHeap` po `(lod, dystans_kwadrat, coord)` — klucz zawiera `coord`,
  więc kolejność jest **całkowicie deterministyczna**, niezależna od kolejności kończenia jobów.
- Twardy limit: ≤ 64 chunków „w locie" jednocześnie; ≤ 2 ms/klatkę na upload do GPU.
- Zwolnienie: LRU po budżecie pamięci; chunk z `revision > 0` (edytowany) idzie do nakładki edycji,
  nie jest odrzucany.

### 5.4a Zapis voxeli — kontrakt dla M2, M6 i M11

Pytanie M2 brzmi „czy mogę pisać do chunków równolegle". **Odpowiedź: nie piszesz do chunków
w ogóle — kolejkujesz edycje.** To nie jest ograniczenie wymyślone dla wygody `engine/voxel`,
tylko zastosowanie kontraktu §3.4 dokumentu 00 (mutacje strukturalne wyłącznie przez bufory komend,
aplikowane w punktach synchronizacji, w ustalonym porządku). Voxele są stanem trwałym symulacji
(kopalnia M6 je drąży, zapis gry je przechowuje), więc podlegają tej samej regule co encje.

**Model dwufazowy:**

```rust
pub struct EditSeq(pub u64);           // globalna kolejność, nadawana przy push

pub enum EditOp {
    Stamp  { prefab: PrefabId, origin: IVec3, rot: Rot90, palette_map: PaletteMap },
    Fill   { aabb: IAabb3, material: MaterialId },
    Carve  { shape: CarveShape },      // wykop kopalni (M6), wykop pod fundament (M2)
    Terrace{ aabb: IAabb3, target_z: i32, material: MaterialId },  // nasyp/wykop drogowy (M2)
}

pub struct VoxelEditCmd {
    pub seq: EditSeq,
    pub source: SystemId,
    pub op: EditOp,
    pub aabb: IAabb3,       // zasięg wyliczany przy push — bucketing bez interpretacji op
}

/// FAZA A — równoległa, po stronie M2/M6. Bez zamków: bufor thread-local per worker.
pub struct EditQueue { /* Vec per worker */ }
impl EditQueue {
    /// &self, nie &mut self — wolno wołać z dowolnego joba, także dla budynku
    /// przecinającego granicę chunka. Kolejność nadaje faza B, nie kolejność wołania.
    pub fn push(&self, source: SystemId, op: EditOp) -> EditSeq;
}

/// FAZA B — punkt synchronizacji. JEDYNE miejsce, w którym voxele się zmieniają.
impl VoxelWorld {
    pub fn apply_edits(&mut self, q: EditQueue, jobs: &JobScope) -> EditReport;
}

/// Token wyłącznego zapisu do JEDNEGO chunka. M2 nigdy go nie widzi i nie potrafi utworzyć —
/// wydaje go wyłącznie faza B. Wyłączność jest gwarantowana typem (&mut), nie dyscypliną.
pub struct ChunkWriter<'a> { chunk: &'a mut Chunk, coord: ChunkCoord }
impl<'a> ChunkWriter<'a> {
    pub fn set(&mut self, local: UVec3, m: MaterialId);
    pub fn fill(&mut self, local: IAabb3, m: MaterialId);
}
```

**Jak faza B daje równoległość mimo serializacji zapisu:**

1. Scalenie buforów workerów i sortowanie po `(source, seq)` — deterministyczne, niezależne od
   kolejności kończenia jobów (§3.3).
2. **Bucketing po `ChunkCoord`** z `aabb` każdej komendy. Budynek przecinający granicę chunka
   trafia do listy **każdego** chunka, który dotyka — to jest dokładnie to miejsce, w którym
   przypadek graniczny M2 znika. M2 nie musi nic wiedzieć o granicach chunków.
3. Zastosowanie **równolegle po chunkach**: jeden worker = jeden chunk na wyłączność, więc dwa
   wątki nigdy nie dotykają tego samego chunka. Wewnątrz chunka kolejność jest posortowaną
   kolejnością globalną → wynik identyczny przy 1 i 8 wątkach.
4. Konflikt (dwie komendy na ten sam voxel w tym samym punkcie synchronizacji): **wygrywa wyższy
   `seq`**, deterministycznie. Nakładające się zapisy trafiają do `EditReport::overlaps` — M2 używa
   tego do wykrycia kolidujących budynków, zamiast sprawdzać to samodzielnie.

**Edycja chunka nierezydentnego** (M2 stawia budynek 3 km od kamery): komenda trafia do
**rzadkiej nakładki edycji**, kluczowanej po `ChunkCoord`, która jest stanem trwałym i jest
konsultowana przy każdej materializacji chunka z `ColumnSource`. Rezydentne chunki są dodatkowo
unieważniane przez `revision++`. Dzięki temu edycja nie wymaga streamingu i działa headless, bez GPU.

`ChunkWriter` nie jest publiczny w sensie „M2 go woła" — jest publiczny w sensie „M2 wie, że istnieje
i że tylko faza B go dostaje". Gdyby kiedyś okazało się, że jakaś faza potrzebuje synchronicznego
zapisu poza punktem synchronizacji, jest to zmiana kontraktu 00 §3.4, nie decyzja `engine/voxel`.

### 5.5 `sim/world` — parametry i model terenu

```rust
pub struct WorldGenParams {              // §4.1 — w całości w zapisie gry
    pub seed: u64,
    pub size: WorldSize,                 // Small4km | Medium8km | Large12km | Metropolis16km
    pub epoch: Epoch,                    // Y1950 | Y1970 | Y1990 | Y2010 | Y2020
    pub profile: EconomyProfile,         // Industrial|Port|University|Tourist|Agricultural|Mixed
    pub region: Region,                  // Coastal|Mountain|Lowland|River|Desert
    pub difficulty: Difficulty,          // M1 tylko przenosi dalej
}
```

Wielkość mapy wiąże się z celem populacyjnym z §4.1 (małe 20–40 tys. → 4 km; metropolia 400 tys.+ → 16 km).
`region` steruje wypiętrzeniem, poziomem morza, wilgotnością bazową i wagami złóż;
`profile` i `epoch` przesuwają wagi typów złóż (np. `Industrial` + `Y1950` → więcej węgla i rudy;
`Y2020` → złoża te istnieją, ale częściej oznaczone jako wyeksploatowane historycznie).

```rust
/// Warstwa geologiczna — definiowana w data/geology/*.ron, nie hardkodowana.
pub struct TerrainLayer {
    pub material: MaterialId,
    pub thickness_base_cm: u32,     // bazowa miąższość
    pub thickness_noise: NoiseSpec, // modulacja przestrzenna (oktawy, częstotliwość, amplituda)
    pub slope_falloff: f32,         // 0..1 — gleba nie utrzymuje się na stromiźnie
    pub elev_range_m: RangeInclusive<i16>,
    pub water_affinity: i8,         // torf/muł tylko blisko wody; -100..100
}

/// Kolumna geologiczna to wynik, nie dane — liczona analitycznie na żądanie.
pub struct ColumnStack {
    pub surface_z: i32,             // w jednostkach 0,5 m
    pub layers: smallvec::SmallVec<[(MaterialId, i32); 8]>,  // (materiał, dolna granica Z)
    pub water_table_z: i32,
}
pub fn column_at(&self, x: i32, y: i32) -> ColumnStack;   // ~150 ns, bezalokacyjne
```

**Złoże — model, który musi udźwignąć wyczerpanie przez kopalnię M6:**

```rust
pub struct DepositId(pub u32);

pub enum ResourceKind { Coal, IronOre, Oil, Gas, Aggregate, Groundwater, ClayDeposit }

pub enum DepositShape {
    Ellipsoid { center: IVec3, radii: IVec3, yaw_deg: u16 },     // ruda, kruszywo
    Seam { polyline: Vec<IVec2>, top_z: i32, thickness_dm: u16 },// pokład węgla
    Trap  { center: IVec3, radii: IVec3, cap_layer: MaterialId },// ropa/gaz w pułapce
    Aquifer { poly: Vec<IVec2>, top_z: i32, bottom_z: i32 },     // wody gruntowe
}

pub struct Deposit {
    pub id: DepositId,
    pub resource: ResourceKind,
    pub shape: DepositShape,
    pub volume_m3: i64,          // objętość geometryczna formacji
    pub concentration: Q,        // 0..=100 — udział surowca w skale (kontrakt §2)
    pub reserves: Mass,          // gramy surowca — WARTOŚĆ PIERWOTNA, całkowitoliczbowa
    pub extracted: Mass,         // 0 na starcie; M6 wyłącznie inkrementuje
    pub depth_top_m: i16,        // poniżej powierzchni terenu w najpłytszym punkcie
    pub depth_bottom_m: i16,
    pub discovered: bool,        // badania geologiczne (M6) odkrywają
    pub quality: Q,              // wpływa na cenę/wydajność przerobu (M6)
}

impl Deposit {
    pub fn remaining(&self) -> Mass { Mass(self.reserves.0 - self.extracted.0) }
    pub fn depletion(&self) -> Q;                       // 0..=100
    /// Kopalnia (M6) woła to; zwraca faktycznie wydobytą masę (≤ żądanej).
    pub fn extract(&mut self, want: Mass) -> Mass;
    /// Ile voxeli usunąć, by wizualizacja odpowiadała wydobytej masie.
    pub fn voxels_for(&self, m: Mass, mat: &VoxelMaterial) -> u64;
}
```

**Kluczowa decyzja modelowa:** ekonomiczną prawdą o złożu jest **całkowitoliczbowy bilans masy**
(`reserves`, `extracted` w gramach, kontrakt §2 — żadnych floatów), a nie liczba voxeli.
Voxele złoża są **pochodną wizualną**: kopalnia zgłasza wydobytą masę, `voxels_for` przelicza to
na objętość wykopu przez gęstość materiału, a `VoxelWorld::edit_region` drąży. Dzięki temu:

- wyczerpanie jest dokładne i testowalne własnościowo (`Σ extracted ≤ Σ reserves`, nigdy naruszone),
- wyczerpanie działa **bez rezydentnych voxeli** (kopalnia poza kadrem, LOD mezo/makro),
- w zapisie gry trzymamy tylko `(DepositId, extracted)` — dwa u64 na złoże.

`reserves` liczymy raz przy generacji, całkowitoliczbowo:
`reserves_g = volume_m3 × density_kg_m3 × concentration / 100 × 1000`, z `checked_mul`.

```rust
pub struct ClimateCell {                 // siatka 256 m; kalendarz 360 dni = 12 × 30 (K-1)
    pub temp_monthly_dc: [i16; 12],      // dziesiąte części °C — bez floatów w stanie trwałym
    pub precip_monthly_mm: [u16; 12],    // miesiące równe (30 dni) → interpolacja bez wyjątków
    pub biome: Biome,
    pub soil_fertility: Q,               // 0..=100 — M5/M6 przeliczy na plon
    pub water_table_depth_dm: u16,
    pub prevailing_wind_deg: u16,
    pub heating_degree_days: u16,        // suma stopniodni — M8 (zapotrzebowanie na ciepło)
}

pub enum Biome { Sea, Lake, River, Marsh, BroadleafForest, ConiferForest, MixedForest,
                 Grassland, Cropland, Scrub, Rock, Sand, Snow }
```

### 5.6 Przebiegi generatora i determinizm

Potok jest listą nazwanych przebiegów; każdy ma **własny strumień RNG**, więc dodanie przebiegu
nie przesuwa losowań pozostałych. Wszystko wg kontraktu §3: `rng(world_seed, StreamId, index, 0)`.

```rust
// engine/core — M1 dopisuje warianty, nigdy nie zmienia istniejących wartości.
pub enum StreamId {
    // ...M0...
    WorldLandmask   = 100,
    WorldHeightBase = 101,
    WorldDomainWarp = 102,
    WorldUplift     = 103,
    WorldErodibility= 104,
    WorldGeology    = 105,
    WorldDeposits   = 106,
    WorldClimate    = 107,
    WorldWind       = 108,
    WorldBiome      = 109,
    WorldDetail     = 110,
    // 111–119 zarezerwowane dla M1
}
```

| # | Przebieg | Strumień | Rozdzielczość | Wynik |
|---|---|---|---|---|
| P1 | Maska lądu i poziom morza | `WorldLandmask` | 4 m | maska ląd/morze, linia brzegowa dla `Coastal` |
| P2 | Baza wysokości | `WorldHeightBase` + `WorldDomainWarp` | 4 m | fBm 8 oktaw z domain warpingiem; `Mountain` → ridged multifractal; `Lowland` → niska amplituda + duża długość fali |
| P3 | Wypiętrzenie i podatność na erozję | `WorldUplift`, `WorldErodibility` | 4 m | pola `U` i `K` dla stream-power (twarde intruzje = niska `K` → ostańce) |
| P4 | Wypełnienie zagłębień | — (deterministyczny) | 4 m | priority-flood + ε (Barnes) — gwarantuje brak lokalnych minimów |
| P5 | Kierunki spływu i akumulacja | — | 4 m | D8 z tie-breakiem po indeksie sąsiada; akumulacja topologiczna |
| P6 | **Erozja** | — | 4 m | stream-power niejawnie (Braun–Willett) + dyfuzja zboczowa + erozja termiczna; N iteracji |
| P7 | Klasyfikacja wód | — | 4 m + wektor | morze/jezioro/rzeka, rząd Strahlera, szerokość i głębokość koryta z geometrii hydraulicznej; wektorowa sieć koryt |
| P8 | Geologia | `WorldGeology` | analitycznie | `TerrainLayer` stack — 0 B pamięci |
| P9 | Złoża | `WorldDeposits` | wektor | lista `Deposit` |
| P10 | Temperatura | `WorldClimate` | 256 m | 12 map miesięcznych |
| P11 | Wiatr i opady | `WorldWind` | 256 m | adwekcja wilgoci, opad orograficzny, cień opadowy |
| P12 | Biomy i żyzność | `WorldBiome` | 256 m | `ClimateCell::biome`, `soil_fertility`, `water_table_depth_dm` |
| P13 | Materializacja 1 m | `WorldDetail` | **leniwie, per kafel** | upsampling + szum detalu + wcięcie koryt |

**Determinizm float w erozji** (to jest realne ryzyko, nie formalność):

- Iteracja P4–P6 wyłącznie w porządku indeksu komórki (row-major) albo po posortowanym stosie
  odbiorników — nigdy po kolejności zakończenia jobów.
- Równoległość P6: po **niezależnych zlewniach** (typowo 50–500 na mapie 16 km), redukcja
  składana po indeksie zlewni. Wynik identyczny niezależnie od liczby wątków — test w CI
  uruchamia generację z `RAYON_NUM_THREADS=1` i `=8` i porównuje hash.
- Kompilacja z ustalonym `target-feature`, bez `fast-math`; brak `f32::mul_add` tam, gdzie
  kompilator mógłby zmienić kontrakcję FMA między platformami.
- Hash terenu (`blake3` po mapie wysokości u16 + lista złóż + klimat) jest testem kontraktowym §3.

### 5.7 Hydrologia — algorytm, parametry, koszt

**P4 — Priority-flood + ε** (Barnes, Lehman, Mulla 2014). Kolejka priorytetowa od brzegów mapy
do środka; komórka niższa od bieżącego progu podnoszona do `próg + ε` (ε = 1 jednostka = 1 mm
w reprezentacji roboczej). Gwarantuje, że **każda komórka ma spływ do morza** — bez tego erozja
tworzy nieciągłe rzeki i jeziora bez odpływu.
Koszt: O(n log n), 16,8 M komórek, kopiec → ~1,0–1,5 s jednowątkowo (słabo się zrównolegla; akceptujemy).

**P5 — D8 + akumulacja.** Odbiornik = sąsiad o największym spadku `(h_i − h_j)/dist`;
remis → najniższy indeks sąsiada (kolejność N, NE, E, SE, S, SW, W, NW).
Akumulacja: przejście po stosie donorów w kolejności odwrotnej do priority-flood, O(n).
Koszt: ~150 ms.

**P6 — Stream-power niejawnie (Braun & Willett 2013):**

```
∂h/∂t = U − K · A^m · S^n          (erozja rzeczna)
∂h/∂t += D · ∇²h                    (dyfuzja zboczowa)
h skorygowane do kąta usypu φ        (erozja termiczna)
```

Parametry domyślne (w `data/geology/erosion.ron`, strojone per region):

| Parametr | Wartość | Znaczenie |
|---|---|---|
| `m` | 0,5 | wykładnik pola zlewni |
| `n` | 1,0 | wykładnik spadku (n=1 → schemat **liniowy, rozwiązywalny wprost**) |
| `K` | 3·10⁻⁶ … 3·10⁻⁵ m^(1−2m)/rok | modulowane polem `WorldErodibility` |
| `U` | 0,2 … 2,0 mm/rok | `Mountain` górna granica, `Lowland` dolna |
| `D` | 0,01 … 0,1 m²/rok | dyfuzja zboczowa |
| `φ` | 33° (grunt), 55° (skała) | kąt usypu |
| `dt` | 5 000 lat | schemat niejawny → bezwarunkowo stabilny |
| iteracje | 40 (mapa ≤ 8 km) / 80 (16 km) | 200 tys. – 400 tys. lat modelowanych |

Dla `n = 1` rekurencja Brauna–Willetta jest **jawnym wzorem** przechodzącym po stosie odbiorników
w jednym przebiegu O(n) — stąd wybór `n = 1`: bezwarunkowa stabilność przy dużym `dt`
i brak iteracji Newtona wewnątrz kroku. To jest powód, dla którego 80 kroków starcza.

**Koszt P6 dla 16×16 km @ 4 m (16,8 M komórek):**
80 iteracji × (przejście stosu O(n) ~10 ns/komórkę + dyfuzja ~4 ns/komórkę)
= 80 × 16,8 M × 14 ns ≈ 19 s jednowątkowo.
Zrównoleglone po zlewniach (efektywne przyspieszenie ~3,5× na 8 rdzeniach, bo zlewnie są nierówne)
→ **5–6 s**. To jest największa pojedyncza pozycja w budżecie generacji.

**Dlaczego 4 m, a nie 1 m:** erozja na 1 m to 268 M komórek — 16× więcej pracy (≈ 90 s) i 16×
więcej pamięci roboczej (≈ 4,8 GB). Rzeki miejskie mają 20–100 m szerokości, czyli 5–25 komórek
przy 4 m — w zupełności dość. Cieki węższe niż 4 m są poniżej rozdzielczości modelu i powstają
w P13 przez wcięcie z **wektorowej** sieci koryt, która zachowuje ciągłość niezależnie od siatki.

**P7 — klasyfikacja wód.** Rzeka, gdy akumulacja `A > A_min` (domyślnie 0,25 km² zlewni).
Szerokość z geometrii hydraulicznej `w = a · Q^b` (a ≈ 2,5; b ≈ 0,5; `Q ≈ A × opad_roczny × współczynnik odpływu`),
głębokość `d = c · Q^f` (c ≈ 0,25; f ≈ 0,4). Rząd Strahlera liczony po drzewie odbiorników.
Jeziora: komórki podniesione przez priority-flood o więcej niż `ε × 4` tworzą misę — wypełniamy do progu odpływu.

### 5.8 `engine/render` — frame graph

| Pass | Typ | Zawartość | Budżet GPU (widok dzielnicy, 1440p) |
|---|---|---|---|
| `shadow` | depth-only ×4 | 4 kaskady 2048², stabilizowane (snap do texela), osobna lista widoczności | 1,8 ms |
| `depth_prepass` | depth | opaque terenu, wypełnia bufor głębi dla clusteringu i wody | 0,8 ms |
| `cluster_assign` | compute | froxele 16×9×24, ≤ 256 świateł/klaster | 0,3 ms |
| `opaque` | color HDR | pipeline voxelowy, multi-draw indirect, materiały z SSBO, AO z wierzchołka, PCF 3×3 z kaskad | 5,5 ms |
| `water` | forward | fresnel, mgła głębinowa, animowane normalne, miękki brzeg z depth | 0,9 ms |
| `far_terrain` | color | geometry clipmap z mapy 4 m (> 4 km) | 0,6 ms |
| `overlay` | color | `TerrainOverlay` — tekstura pola skalarnego + paleta, rzutowana na teren | 0,4 ms |
| `post` | fullscreen | ekspozycja z krzywej pory dnia, ACES-fitted tonemap, mgła atmosferyczna, FXAA | 0,7 ms |
| rezerwa (UI M3+, encje M3/M4) | | | 5,6 ms |
| **suma** | | | **16,6 ms = 60 FPS** |

**Rejestracja passów — punkt rozszerzenia dla M3/M4/M11.** `RenderGraph` nie jest zamkniętą listą;
passy rejestruje się jawnie, z deklaracją zasobów wejściowych i wyjściowych, a graf sam wyznacza
kolejność i bariery. M1 buduje ten mechanizm i rejestruje własne passy; kolejne fazy **dodają passy,
nie przepisują istniejących**.

```rust
pub struct PassId(pub u16);
pub struct RenderGraph { /* ... */ }

pub trait RenderPass: Send + Sync {
    fn id(&self) -> PassId;
    fn declare(&self, d: &mut PassDecl);                       // czyta/pisze jakie zasoby
    fn record(&mut self, ctx: &FrameCtx, enc: &mut wgpu::CommandEncoder);
}
pub struct PassDecl {
    pub reads:  Vec<ResourceRef>,      // HdrColor, Depth, ShadowCascades, ClusterGrid, TerrainOverlay…
    pub writes: Vec<ResourceRef>,
    pub order:  PassOrder,             // After(PassId) | Before(PassId) | InSlot(GraphSlot)
}
/// Sloty to nazwane punkty wpięcia; graf gwarantuje ich kolejność względem passów M1.
pub enum GraphSlot { PreDepth, PostDepth, PreOpaque, PostOpaque, PreWater, PostWater,
                     PreOverlay, PrePost, PostPost, Offscreen }

impl RenderGraph {
    pub fn register(&mut self, pass: Box<dyn RenderPass>) -> PassId;
    pub fn resource(&mut self, desc: ResourceDesc) -> ResourceRef;   // własne RT/bufory passa
}
```

Cykle i zapisy do zasobu bez deklaracji są błędem wykrywanym przy budowie grafu, nie w runtime.

**Budżet świateł (clustered shading).** Właścicielem passa `cluster_assign` jest M1; fazy dalsze
wyłącznie **wypełniają listę świateł** ze snapshotu. Limity:

| Wielkość | Wartość |
|---|---|
| Froxele | 16 × 9 × 24 = 3456 klastrów |
| Świateł w klastrze | **≤ 256** |
| **Świateł aktywnych w klatce (globalnie)** | **≤ 4096** |
| Koszt przypisania przy 4096 świateł | ≤ 0,4 ms GPU |

Redukcja z dziesiątek tysięcy źródeł do 4096 jest **po stronie producenta snapshotu**, nie passa:
źródło bliżej niż promień progowy → światło punktowe, dalej → materiał emisyjny + wkład do ambientu.
M1 nie narzuca progu (150 m dla latarni jest rozsądnym punktem startowym); narzuca **twardy limit
4096** — przepełnienie bufora to błąd asercji w buildzie debug i obcięcie po priorytecie
(dystans do kamery, malejąco) w release.

**Nie robimy w M1** (świadomie): TAA i wektory ruchu (M11), SSR na wodzie (M11), bindless
(nie wszędzie dostępne w `wgpu` — zamiast tego jeden arena-buffer + indeks w SSBO), auto-ekspozycja
(krzywa czasu dnia jest przewidywalna i nie miga).

**Oświetlenie dobowe (§15.3):** kierunek słońca z `SimMinute` → dzień roku + minuta doby +
szerokość geograficzna z `Region` → deklinacja i kąt godzinny → azymut/wysokość.
Barwa i natężenie słońca oraz nieba z LUT 64 wpisów po wysokości słońca (autorowane w RON),
interpolowanej liniowo — brak modelu Preethama/Hoseka, bo LUT wygląda dobrze i jest darmowy.
Ambient: dwa kolory (niebo/grunt) mieszane po `normal.y`.

**Kamera (§15.2):**

```rust
pub enum CameraMode {
    Orbit { target: DVec3, dist: f32, yaw: f32, pitch: f32 },  // orbita nad miastem → ulica
    Free  { pos: DVec3, yaw: f32, pitch: f32 },
    FirstPerson { anchor: EntityId, eye_height_m: f32 },        // §15.2; postać gracza = M9
}
pub struct CameraState {
    pub mode: CameraMode,
    pub fov_deg: f32,          // 20° przy orbicie (quasi-izometria) → 60° przy ulicy
    pub clip_plane_z: Option<i32>,  // cięcie poziomem — uniform, używa M2/M11
    pub near: f32, pub far: f32,
}
```

Pozycja kamery w `f64` (mapa 16 km × 1 m → `f32` traci precyzję na krawędziach);
do shaderów przekazujemy **pozycje względem kamery**, nie absolutne.
Zoom miesza `dist` i `fov` po krzywej — od dalekiej orbity z wąskim FOV (wrażenie izometrii,
§15.2) do bliskiej perspektywy. Pochylenie clampowane do 5°…89°; kolizja z terenem przez
`height_at` + margines.

### 5.9 Budżety

**Pamięć (M1, poza budżetem symulacji z §17.7):**

| Pozycja | 4×4 km | 16×16 km | Uwaga |
|---|---|---|---|
| Mapa wysokości 4 m (u16) | 2,1 MB | 33,5 MB | trwała |
| Klasa wody + głębokość | 1,1 MB | 18,5 MB | trwała |
| Sieć koryt (wektor) | 0,1 MB | 1,0 MB | trwała |
| Złoża | 0,05 MB | 0,5 MB | ~500 / ~6000 sztuk |
| Klimat (256 m) | 13 KB | 213 KB | trwała |
| **Trwały stan świata** | **~3,5 MB** | **~54 MB** | zapis po zstd: ≤ 8 / ≤ 20 MB |
| Pula chunków voxelowych | — | **≤ 384 MB** | twardy limit; ~8192 chunków rezydentnych |
| Arena buforów GPU (mesh) | — | **≤ 768 MB** | ~32 KB/chunk średnio |
| Cienie + G-bufory + LUT | — | **≤ 192 MB** | 4×2048² D32 = 64 MB + reszta |
| Nakładka edycji gracza | — | rośnie z rozgrywką | tylko chunki `Dense` z `revision > 0` |
| **Suma M1 na metropolii** | | **≤ 1,4 GB** | z 6 GB celu §17.7 |

**Czas generacji (8 rdzeni, desktop 2024, headless bez GPU):**

| Przebieg | 4×4 km | 16×16 km |
|---|---|---|
| P1–P3 (szum, wypiętrzenie) | 30 ms | 400 ms |
| P4 (priority-flood) | 90 ms | 1,4 s |
| P5 (D8 + akumulacja) | 12 ms | 160 ms |
| **P6 (erozja)** | **0,4 s** | **5–6 s** |
| P7 (wody) | 25 ms | 350 ms |
| P8–P9 (geologia, złoża) | 20 ms | 250 ms |
| P10–P12 (klimat, biomy) | 45 ms | 600 ms |
| **Suma (cel)** | **≤ 1,0 s** | **≤ 10 s** |
| **Suma (twardy limit CI)** | **≤ 2,5 s** | **≤ 20 s** |

P13 (materializacja 1 m) nie wchodzi do tego budżetu — jest leniwe, ~0,4 ms na kafel 256×256 m.

**FPS (§20.2, GPU średniej klasy 2024 — RTX 4060 / RX 7600, 1440p):**

| Scenariusz | Cel | Warunki |
|---|---|---|
| Widok dzielnicy (kamera 50–300 m nad gruntem, promień ~600 m) | **≥ 60 FPS** | LOD0 dominuje, ~2500–3500 chunków |
| Widok miasta (orbita 1,5–4 km) | **≥ 30 FPS** | LOD2/3 dominuje, clipmap na horyzoncie |
| Poziom ulicy | ≥ 60 FPS | mały frustum, LOD0 |

**CPU na klatkę (wątek renderu, 16,6 ms):** culling ≤ 1,0 ms, upload meshy ≤ 2,0 ms,
zapis komend ≤ 1,5 ms. Symulacja i meshing — osobne wątki, pula niskiego priorytetu.

---

## 6. Kontrakty międzyfazowe

### 6.1 Dostarczam

**Dla M2 (miasto statyczne) — pięć rzeczy wprost wymaganych: wysokość, spławność, przeszkody, złoża, biomy:**

```rust
/// sim/world::TerrainQuery — jedyny interfejs do terenu dla wszystkich faz.
/// Czysty, bezalokacyjny, wołalny z wielu wątków. Współrzędne w metrach (1 m siatka).
pub trait TerrainQuery: Send + Sync {
    // --- 1. MAPA WYSOKOŚCI ---
    fn height_at(&self, x: i32, y: i32) -> i32;           // w jednostkach 0,5 m
    fn height_tile(&self, tile: TileCoord, out: &mut [i32; 256 * 256]);  // wsadowo dla L-systemu
    fn slope_at(&self, x: i32, y: i32) -> u8;             // 0..=255 ≈ tan(α) × 64
    fn surface_material_at(&self, x: i32, y: i32) -> MaterialId;
    fn column_at(&self, x: i32, y: i32) -> ColumnStack;   // pełna kolumna geologiczna

    // --- 2. SPŁAWNOŚĆ I WODA ---
    fn water_at(&self, x: i32, y: i32) -> WaterCell;
    fn river_network(&self) -> &RiverNetwork;             // wektor: odcinki, rząd, szerokość, spławność
    fn navigable(&self, x: i32, y: i32) -> Option<NavigableClass>;  // Sea | Canal | River{min_draft_dm}

    // --- 3. PRZESZKODY I PRZYDATNOŚĆ POD ZABUDOWĘ ---
    fn buildability_at(&self, x: i32, y: i32) -> Buildability;
    fn obstacle_mask(&self, rect: IRect) -> ObstacleBitset;  // 1 bit/m²: woda, stromizna, skała
    fn crossing_cost(&self, a: IVec2, b: IVec2) -> Crossing; // most/tunel/nasyp + długość + różnica wysokości

    // --- 4. ZŁOŻA ---
    fn deposits_in(&self, rect: IRect) -> DepositIter<'_>;
    fn deposit(&self, id: DepositId) -> &Deposit;
    fn deposits_at_column(&self, x: i32, y: i32) -> DepositIter<'_>;

    // --- 5. KLIMAT I BIOMY ---
    fn climate_at(&self, x: i32, y: i32) -> &ClimateCell;
    fn biome_at(&self, x: i32, y: i32) -> Biome;
    fn soil_fertility_at(&self, x: i32, y: i32) -> Q;

    // --- 6. SKRÓTY ZGŁOSZONE PRZEZ M2 (K-13) ---
    // Wszystkie PRZYJĘTE. Cztery z pięciu to jednolinijkowe akcesory nad danymi, które już
    // istnieją — nie dokładają ani obliczeń, ani pamięci. Piąty domyka lukę semantyczną.
    fn water_depth_at(&self, x: i32, y: i32) -> u16;          // = water_at().depth_dm
    fn deposit_at(&self, x: i32, y: i32) -> Option<DepositId>;// najpłytsze złoże w kolumnie
    fn soil_quality_at(&self, x: i32, y: i32) -> Q;           // = soil_fertility_at (patrz niżej)
    fn prevailing_wind(&self, x: i32, y: i32) -> u16;         // stopnie, = climate_at().prevailing_wind_deg
    fn flood_risk_at(&self, x: i32, y: i32) -> Q;             // = Buildability::flood_risk (to samo)
}

pub struct WaterCell { pub depth_dm: u16, pub class: WaterClass, pub flow_dir: u8, pub strahler: u8 }
pub struct Buildability {
    pub slope_ok: bool,
    pub flood_risk: Q,              // z akumulacji i wysokości nad korytem
    pub bearing_capacity: u8,       // z materiału powierzchniowego
    pub earthwork_cost_index: u16,  // mnożnik kosztu robót ziemnych; M2 przelicza na Money
}
pub enum Crossing { Flat, Embankment{ fill_m3: i64 }, Bridge{ span_m: u32, clearance_m: u16 }, Tunnel{ len_m: u32, rock: MaterialId } }
```

**Rozstrzygnięcie zgłoszeń M2 (tryb K-13 — fazy zgłaszają braki, nie liczą po swojemu):**

| Zgłoszenie | Werdykt | Uzasadnienie |
|---|---|---|
| `water_depth_at` | **przyjęte jako skrót** | To `water_at(x,y).depth_dm`. Dodaję akcesor, żeby M2 nie budowało całego `WaterCell` dla jednego pola. Zero nowych danych |
| `deposit_at` | **przyjęte jako skrót** | Zwraca najpłytsze złoże przecinające kolumnę (`deposits_at_column().min_by(depth_top_m)`). Do strefowania wydobywczego (Etap 4) tyle wystarcza; pełna lista nadal przez `deposits_at_column` |
| `soil_quality_at` | **przyjęte, ale to alias — uwaga na dwuznaczność** | „Jakość gleby" znaczy dwie różne rzeczy. Do **rolnictwa** to `soil_fertility_at` i `soil_quality_at` jest jego aliasem. Do **budowy** (czy grunt uniesie fundament) to `Buildability::bearing_capacity`, zupełnie inna wielkość. M2 użyje jednego albo drugiego zależnie od tego, czy strefuje rolę czy liczy koszt fundamentu — nigdy tego samego pola do obu |
| `prevailing_wind` | **przyjęte jako skrót** | `climate_at().prevailing_wind_deg`. Uzasadnione: M2 potrzebuje tego do strefowania (przemysł z podwiatru od mieszkaniówki), a to realne kryterium urbanistyczne, nie ozdobnik |
| `flood_risk_at` | **przyjęte — TAK, to dokładnie to samo** co `Buildability::flood_risk` | Jedna wartość, jedno źródło, dwie drogi dostępu. Semantyka doprecyzowana: `Q` 0..=100 z wysokości punktu nad poziomem wody miarodajnej najbliższego koryta, wyliczonym w P7 z rzędu Strahlera i geometrii hydraulicznej. **Nie ma i nie będzie drugiego modelu ryzyka powodziowego** — gdyby M8 (powódź jako zdarzenie) potrzebował innego, nakłada odchylenie, nie liczy od nowa |

`crossing_cost` pozostaje po stronie M1 zgodnie z ustaleniem — M2 nie liczy mostów i tuneli
samodzielnie. Konsekwencja, którą M2 musi znać: `Crossing` zwraca **geometrię i objętość robót**
(rozpiętość, długość, `fill_m3`, materiał skały), nie `Money`. Przeliczenie na koszt jest po
stronie M2, bo to cena epoki i technologii, a nie własność terenu.

**Dla M2 — edycja i render:**

```rust
// engine/voxel
pub fn VoxelWorld::edit_region(&mut self, cmd: VoxelEditCmd) -> EditResult;
pub fn VoxelWorld::stamp_prefab(&mut self, origin: IVec3, p: &VoxelPrefab);
pub struct VoxelPrefab { pub dims: UVec3, pub palette: Palette, pub data: ChunkStorage }

// engine/render — nakładki danych (wartość gruntu, hałas, strefy…)
pub enum OverlaySlot { Primary, Secondary }
pub trait TerrainOverlay: Send + Sync {
    fn field(&self) -> &ScalarField;          // R16 lub R8, dowolna rozdzielczość
    fn palette(&self) -> OverlayPalette;      // rampa + zakres + legenda
    fn opacity(&self) -> f32;
}
pub fn Renderer::set_overlay(&mut self, slot: OverlaySlot, ov: Option<Arc<dyn TerrainOverlay>>);
pub fn Renderer::add_line_layer(&mut self, layer: LineLayerHandle);  // drogi, strumienie przepływów
```

**Dla M3/M4 (encje dynamiczne):** frame graph z wolnym passem `entities` między `depth_prepass`
a `opaque`, dostęp do bufora klastrów świateł i do kaskad cieni; `CameraState` i macierze
jako zasób współdzielony.

**Dla M6 (łańcuch dostaw / wydobycie):** `Deposit::extract(Mass) -> Mass`, `remaining()`,
`voxels_for()`. Prawdą jest bilans masy w gramach; voxele są pochodne (§5.5).
`ResourceKind → GoodId` mapuje M6, nie M1.

**Dla M8 (miasto jako aktor):** `ClimateCell` z rocznym cyklem jako **podkład, nie zdarzenie** —
M8 nakłada odchylenia pogodowe na ten cykl, nie modyfikuje go.

**Dla M11 (uzgodnione bezpośrednio z planem M11):**

| Co | Właściciel | Uwaga |
|---|---|---|
| `RenderGraph::register` + `GraphSlot` | **M1 buduje** | M11 wpina swoje sześć passów (instancing encji, cap przekroju, regeneracja impostorów, cząstki pogody, dym, tłum agregatowy) przez `PassDecl`, bez dotykania passów M1 |
| `CameraState` jako osobny zasób ramki (macierze + frustum + `clip_plane_z`) | **M1** | Odczytywalny przez dowolny pass i przez `engine/audio` |
| `CameraMode::FirstPerson` | **M1 dostarcza wariant** (PRD §15.2 jest w zakresie M1) | M11 **nie dopisuje go ponownie** — wiąże istniejący wariant z encją postaci gracza (M9). Anchor jest `EntityId`, więc do M9 może wskazywać encję atrapę |
| `clip_plane_z` (cięcie poziomami) | **M1: uniform + clamp geometrii** | M11 dokłada **cap pass** domykający ściany — to nowy pass, nie zmiana pipeline'u chunków |
| Pass `cluster_assign` | **M1** | M11 tylko wypełnia listę świateł ze snapshotu; limity: ≤ 4096 świateł/klatkę, ≤ 256/klaster (§5.8) |
| `ClusterOccupancy` — licznik zajętości klastrów w devtools | **M1 dostarcza** | Histogram świateł na klaster + maksimum + liczba klastrów powyżej progu, odczytywalny w czasie rzeczywistym. **Powód, dla którego to jest kontrakt, a nie ciekawostka:** budżet globalny 4096 nie chroni przed niczym, bo jest ślepy na gęstość — 2400 latarni rozrzuconych równomiernie i te same 2400 wzdłuż jednej alei dają identyczną sumę globalną i zupełnie inny koszt passa `opaque`. Progiem, który faktycznie wiąże, jest ≤ 256 **na klaster**, więc przyrządem do kalibracji redukcji po stronie producenta snapshotu musi być ten licznik, nie suma. Objaw przekroczenia: skok czasu `opaque` przy obrocie kamery **wzdłuż** arterii przy niezmienionej liczbie świateł globalnie |
| Pipeline chunków 32³, greedy meshing, voxel AO, LOD 2×/4×/8×, streaming, `init_gpu()`, `TerrainQuery` | **M1, zamknięte** | M11 ich nie modyfikuje |
| `render::instancing`, `impostor`, `interiors`, `weather`, `signs`, `budget`, `voxel::model` (.mvox 0,25 m), `voxel::anim` | **M11** | Nowe moduły w crate'ach M1; M1 nie planuje ich zawartości |
| Impostory za LOD3 | **M11** | M1 kończy na `far_terrain` (clipmap) — M11 go zastępuje lub uzupełnia |

**`sim-snapshot` — crate typów POD między symulacją a renderem. Właścicielem jest M11**
(rozstrzygnięcie koordynatora); M1 jest **konsumentem typu ładunku** i właścicielem wyłącznie
**mechanizmu** po stronie `engine/render` (podwójne buforowanie, synchronizacja, interpolacja,
`FrameCtx`). Ponieważ `engine/render` nie skompiluje się bez tego crate'u, M1 zakłada go w WP-R1
w minimalnej postaci i **oddaje schemat M11 bez negocjacji** — poniższe cztery pola to potrzeba
M1, nie propozycja kształtu całości:

```rust
// sim-snapshot — WYŁĄCZNIE typy POD, zero zależności od sim/* i engine/render
pub struct RenderSnapshot {
    pub tick: Tick,
    pub sim_minute: SimMinute,          // pozycja słońca, cykl dobowy
    pub camera_hint: Option<DVec3>,     // cel śledzenia
    pub terrain_revision: u32,          // unieważnia cache chunków po edycji
    pub lights: SoaSlice<LightRecord, 4096>,  // stały cap = globalny budżet świateł §5.8
    // rekordy encji (mieszkaniec 32 B, pojazd 40 B, zakład 24 B) — dokłada M11
}
```

`lights` jest **stałym capem, nie `Vec`-iem** — na wniosek M11, przyjęty bez zastrzeżeń: snapshot
publikuje się do 30×/s, więc `Vec` w ładunku znaczy alokację na publikację, czyli dokładnie to,
czemu podwójne buforowanie ma zapobiegać. Cap 4096 = globalny budżet świateł z §5.8, 20 B na rekord,
80 KB. Przepełnienie obsługuje producent (obcięcie po malejącym dystansie), nie pass.

**M1 przyjmuje regułę egzekucyjną M11:** `engine/render`, `engine/voxel` i `engine/audio` nie mogą
mieć w `Cargo.toml` żadnego `sim/*` poza `sim-snapshot`; pilnuje tego `cargo tree` w CI.
Wpisane do kryteriów akceptacji §7.
Uwaga do M11: `sim/world` **nie** jest zależnością `engine/render` ani `engine/voxel`. Trait
`ColumnSource` jest zdefiniowany w `engine/voxel`, a implementowany w `sim/world` — zależność idzie
w jedną stronę, od symulacji do silnika. `TerrainQuery` żyje w `sim/world` i konsumują go wyłącznie
`sim/*` oraz M2; render dostaje mapę wysokości dla clipmapy jako zwykły bufor w snapshocie, nie
przez trait. Reguła `cargo tree` jest spełniona bez wyjątków.

**Dla M12:** stan trwały świata to `WorldGenParams` + hash terenu + lista `(DepositId, extracted)`
+ nakładka edycji. Zapis świata 16 km ≤ 20 MB — cel §20.2 („zapis < 5 s") nie jest zagrożony
przez teren.

### 6.2 Konsumuję

**Od M0 (`engine/core`):**
- `SimMinute`, `Tick`, `Q`, `Mass`, `Volume`, `Money` — kontrakt §2.
- `rng(world_seed, StreamId, entity_index, tick)` — kontrakt §3.1.
- `StreamId` jako `enum` z możliwością dopisania wariantów — **rezerwuję zakres 100–119**.
- Pomocniki fixed-point i `div_round_half_up`.
- `blake3`/hasher stanu do testu kontraktowego §3.6.

**Od M0 (`engine/jobs`):**
- `JobScope` z deterministycznym fork-join i redukcją składaną po indeksie (§3.3).
- `par_for_each_indexed` gwarantujące wynik niezależny od kolejności zakończenia.
- **Pula niskiego priorytetu / tło** dla meshingu i generacji chunków, tak by praca renderu
  nigdy nie zagłodziła symulacji. Jeśli M0 tego nie ma — zgłoszone w §9.

**Od M0 (`engine/ecs`):** rejestracja zasobu `World` (teren jako zasób, nie encje), komponent
`Deposit` jako encja ECS (M6 będzie go mutował przez bufor komend), bufory komend.

**Od M0 (`engine/io`):** zapis/odczyt snapshotu; potrzebuję serializacji `WorldGenParams`,
listy złóż i rzadkiej nakładki edycji. Reszta terenu **regeneruje się z seeda** — nie zapisujemy jej.

**Od M0 (`engine/devtools`):** rejestracja panelu inspektora, strefy profilera, wejście
`tools/headless` pozwalające wygenerować świat bez GPU (§6 konwencji: headless-first).

**Od M0 (okno i `wgpu`):** M0 buduje okno z `wgpu` renderujące jeden chunk (§19). Ponieważ
właścicielem `engine/render` jest M1, **kod inicjalizacji device/surface migruje z M0 do
`engine/render` w WP-R1** — M0 traktuje swój renderer jako rusztowanie do wyrzucenia. Zgłoszone w §9.

**Od `data/`:** nowe katalogi `data/materials/`, `data/geology/`, `data/climate/`, `data/sky/`
w formacie RON z `schema_version` (kontrakt §5).

**Od M11:** crate `sim-snapshot` (patrz §6.1) — jedyna dozwolona zależność `sim/*` w `engine/render`.

### 6.3 Wpływ rozstrzygnięć wiążących 00 §4a na M1

| Decyzja | Wpływ na M1 | Co zmieniam |
|---|---|---|
| **K-1, K-15** — kalendarz 360 dni (12 × 30), tydzień 7-dniowy dryfujący | **Realny wpływ na cykl roczny.** Dzień roku i pora roku biorę z `SimCalendar`, nie liczę z `SimMinute` samodzielnie | `ClimateCell::temp_monthly_dc[12]` / `precip_monthly_mm[12]` **pasują wprost** — 12 miesięcy × 30 dni = 360, interpolacja między miesiącami jest równomierna (po 30 dni), bez przypadków szczególnych na luty i lata przestępne. Deklinacja słońca: `δ = 23,44° · sin(2π · (d − 80) / 360)`, `d ∈ 0..359` z `SimCalendar::day_of_year()`. `DayOfWeek` M1 nie używa (nie ma jeszcze nic, co zależałoby od dnia tygodnia) — będzie potrzebny dopiero M3 |
| **K-6** — `core::det_math` zamiast libm w kodzie symulacji; render bez ograniczeń | **Dotyczy mnie mocniej, niż sugeruje treść:** `sim/world` to kod symulacji, więc **cały generator (P1–P13) jest objęty**, mimo że produkuje dane wizualne. Passy `engine/render` i meshing w `engine/voxel` — poza | Szum, erozja i model klimatu przechodzą na `core::det_math`. **Dobra wiadomość:** wybór `m = 0,5` i `n = 1` w stream-power (§5.7) był podyktowany stabilnością, ale ma skutek uboczny — `A^0,5` to `sqrt`, które jest w IEEE-754 **dokładnie zaokrąglane i identyczne na każdej platformie**. Generator nie potrzebuje więc `powf` ani funkcji przestępnych w gorącej pętli; `det_math` obsłuży `sin`/`cos` w modelu klimatu i pozycji słońca, wołane 4096 razy, nie 16 mln |
| **K-8, K-12** — wspólne słowniki domenowe i `DecisionReason` w `engine/core` | Moje `ResourceKind` i `Biome` są słownikami domenowymi używanymi przez M5/M6/M8 | Przenoszę oba do `engine/core` przy WP-W4/W6. `VoxelMaterial`, `TerrainLayer`, `Deposit`, `ClimateCell` zostają w `sim/world` (to modele, nie słowniki). M1 nie produkuje `DecisionReason` — generator nie podejmuje decyzji agenta; kontrakt 00 §7 dotyczy mnie pusto |
| **K-13** — `TerrainQuery` jedynym źródłem prawdy o terenie | Przyjmuję jako **normalny tryb pracy**, nie wyjątek. Zgłoszenia braków od faz są tańsze niż pięć niezależnych implementacji ryzyka powodziowego | Konsekwencja operacyjna: `TerrainQuery` będzie rosnąć po zamknięciu M1. Dlatego jest **traitem**, a nie strukturą — dopisanie metody z domyślną implementacją nie łamie niczego u konsumentów. Każde zgłoszenie rozstrzygam tak jak pięć zgłoszeń M2 w §6.1: skrót nad istniejącą daną albo jawne wskazanie, że danej nie ma i skąd ją wziąć |
| **K-16** — `BatchId`/`OfferId` dostają dedykowane areny; `Deposit` **zostaje** encją ECS | Potwierdzam: kontrakt `Deposit::extract(Mass) -> Mass` **bez zmian** | Uzasadnienie zgodne z rozstrzygnięciem: złóż jest ~500 (4 km) do ~6000 (16 km), są długowieczne i mutowane rzadko (M6, przez bufor komend). Arena nic by tu nie dała |

---

## 7. Testy i kryteria akceptacji

### 7.1 Determinizm (warunek Definition of Done, kontrakt §3.6)

| Test | Treść | Próg |
|---|---|---|
| `terrain_hash_stable` | Dwa przebiegi tego samego seeda → identyczny hash (wysokość u16 + wody + złoża + klimat) | bit w bit |
| `terrain_thread_invariant` | Generacja przy 1 i 8 wątkach → identyczny hash | bit w bit |
| `terrain_hash_matrix` | 32 seedy × 5 regionów × 4 rozmiary → tabela hashy zapisana w repo; zmiana wymaga świadomego zatwierdzenia | 0 nieoczekiwanych zmian |
| `column_matches_voxels` | `column_at(x,y)` zgodne z materiałami zmaterializowanego chunka | 100% próbek |
| `lod_consistency` | Agregat LOD_n zgodny z większościowym głosowaniem na LOD0 w tym samym obszarze | 100% |
| `deposit_ledger` | Suma `reserves` i `extracted` po serii `extract()` — brak przepełnienia, brak ujemnych, `extracted ≤ reserves` | własnościowy, 10 tys. losowań |
| `edits_order_invariant` | Ten sam zbiór `VoxelEditCmd` zakolejkowany w losowej kolejności przez 1 i przez 8 workerów → identyczny stan voxeli i identyczny `EditReport` (§5.4a) | bit w bit, 1000 losowań |
| `edits_cross_chunk` | Stempel przecinający granicę chunka daje ten sam wynik niezależnie od tego, które z dotykanych chunków były rezydentne w chwili aplikacji | 100% |

Hash terenu dopisany do funkcji haszującej stanu ECS (wymóg kontraktu §3.6).

### 7.2 Poprawność generatora

| Test | Próg |
|---|---|
| `no_local_minima` — po P4 żadna komórka lądowa nie jest lokalnym minimum | 0 komórek |
| `rivers_reach_sea` — każdy odcinek sieci koryt ma ścieżkę do morza/jeziora odpływowego | 100% |
| `no_negative_water` — głębokość wody ≥ 0 wszędzie | 100% |
| `soil_on_slopes` — brak gleby przy nachyleniu > `slope_falloff` | 100% |
| `deposits_in_host_rock` — każde złoże leży w dopuszczalnej formacji i poniżej terenu | 100% |
| `climate_monotone` — temperatura maleje z wysokością w każdym miesiącu | 100% |
| `rain_shadow` — region `Mountain`: opad zawietrzny < 60% nawietrznego | dla 90% seedów |
| `biome_plausible` — region `Desert` nie generuje `BroadleafForest`; `Coastal` ma linię brzegową | 100% |

### 7.3 Wydajność (asercje w CI, maszyna referencyjna)

| Test | Próg |
|---|---|
| `bench_generate_4km` | ≤ 2,5 s |
| `bench_generate_16km` | ≤ 20 s |
| `bench_mesh_chunk` | ≤ 0,8 ms/chunk terenowy, 1 rdzeń |
| `bench_lod3_chunk` | ≤ 1,5 ms |
| `bench_priority_flood_16km` | ≤ 1,5 s |
| `mem_voxel_budget` | pula chunków ≤ 384 MB, arena GPU ≤ 768 MB — przekroczenie = błąd, nie ostrzeżenie |
| `mem_world_persistent_16km` | ≤ 60 MB w RAM, ≤ 20 MB w zapisie |
| `dep_isolation` (`cargo tree`) | `engine/render`, `engine/voxel`, `engine/audio` nie mają w drzewie zależności żadnego `sim/*` poza `sim-snapshot` — reguła uzgodniona z M11, egzekwowana buildem, nie dyscypliną |
| `render_graph_acyclic` | Rejestracja passa z cyklem lub z zapisem do niezadeklarowanego zasobu → błąd przy budowie grafu, nie w runtime |

### 7.4 Wydajność renderu (poza CI — maszyna z GPU, raport ręczny per milestone)

Scena benchmarkowa: ustalony seed, ustalony przelot kamery 60 s (orbita miasta → dzielnica →
poziom ulicy → przelot 2 km → powrót na orbitę), zapis czasu klatki.

| Metryka | Próg (§20.2) |
|---|---|
| Widok dzielnicy — mediana | ≥ 60 FPS |
| Widok dzielnicy — 1% low | ≥ 45 FPS |
| Widok miasta — mediana | ≥ 30 FPS |
| Widok miasta — 1% low | ≥ 24 FPS |
| Zacięcia > 33 ms podczas przelotu | ≤ 2 na 60 s |

### 7.5 Headless-first (kontrakt §6)

Cała `sim/world` musi generować i być testowana **bez GPU**. `tools/headless generate`
działa na maszynie CI bez karty graficznej. Testy renderu są oznaczone `#[ignore]` w CI
bez GPU i uruchamiane na maszynie z GPU.

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Waga | Mitygacja |
|---|---|---|---|
| R1 | **Niedeterminizm float w erozji** — inny wynik między maszynami/liczbą wątków; łamie kontrakt §3 i psuje M2, bo parcele przestają pasować | wysoka | Ustalona kolejność redukcji, równoległość po zlewniach (nie po blokach), pinned `target-feature`, brak `fast-math`, test `terrain_thread_invariant` w CI od pierwszego dnia WP-W3. Plan awaryjny: przeniesienie wysokości na fixed-point i32 (kosztem ~15% wydajności) |
| R2 | **Erozja na 4 m, teren na 1 m** — rzeka na 1 m nie trafia w koryto wyerodowane na 4 m; schodki po upsamplingu | wysoka | Koryta jako **wektory**, nie rastry; wcięcie po upsamplingu; bicubic + szum detalu o amplitudzie malejącej z nachyleniem; test `rivers_reach_sea` na obu rozdzielczościach |
| R3 | **Budżet erozji** — 5–6 s dla 16 km to optymistyczna prognoza; zlewnie są nierówne i skalowanie może wyjść 2× zamiast 3,5× | średnia | Zmierzyć wcześnie (WP-W3 ma budżet jako kryterium ukończenia). Dźwignie: mniej iteracji z większym `dt`, erozja na 8 m dla map 16 km, progress bar z podglądem (generacja raz na sesję — 20 s jest do przyjęcia) |
| R4 | **AO łamie scalanie w greedy meshingu** — różne czwórki AO na zboczach fragmentują quady; liczba wierzchołków rośnie 2–4× względem prognozy, arena GPU przepełniona | średnia | Zmierzyć realny rozkład quadów na terenie erodowanym (nie na płaskim placu testowym) w WP-V2. Plan awaryjny: AO per voxel w teksturze 3D (scalanie pełne, koszt: dodatkowa tekstura ~1 B/voxel na rezydentnych chunkach) |
| R5 | **Budżet pamięci przy kamerze na dnie doliny** — widok daleki wzdłuż doliny wciąga ogromny frustum przy LOD0 | średnia | Twardy limit puli z wywłaszczaniem: przekroczenie budżetu obniża promień LOD0 zanim zabraknie pamięci; nigdy odwrotnie. Budżet jest asercją, nie sugestią |
| R6 | **Multi-draw indirect niedostępne** na części backendów `wgpu` (WebGPU, część sterowników) | niska | Ścieżka zapasowa: draw per chunk z sortowaniem po pipeline; ≤ 4000 draw calli mieści się w budżecie CPU. Obie ścieżki w CI |
| R7 | **Granica M0/M1 przy `wgpu`** — dwa renderery, nikt nie wie, który jest prawdziwy | średnia | WP-R1 jawnie usuwa renderer M0; M0 od początku traktuje go jako rusztowanie. Zgłoszone w §9 |
| R8 | **Krzywa uczenia Rusta i WGSL** (§20.4) — M1 to pierwsza faza z nietrywialną grafiką i nietrywialną równoległością | wysoka | Kolejność WP: `engine/voxel` (czysty Rust, headless, testowalny) przed `engine/render`; generator w całości headless; każda grupa WP ma artefakt do obejrzenia |
| R9 | **Nadprojektowanie renderu** — clustered shading, cienie kaskadowe i post-processing zanim jest cokolwiek do oświetlenia poza terenem | średnia | M1 buduje tylko to, co obsługuje słońce i teren. Światła punktowe: infrastruktura + kilka testowych, bez systemu latarni (M2) i okien (M2). Zakaz TAA, SSR, GI |
| R10 | **Złoża bez konsumenta aż do M6** — projektujemy model w ciemno i za 5 faz okazuje się nie pasować | średnia | Model minimalny, ale z całkowitoliczbowym bilansem masy i `extract()` od razu — to jedyna część, której później nie da się dołożyć bez migracji zapisów. Reszta (metody wydobycia, koszty, jakość przerobu) jest w M6 |

---

## 9. Decyzje otwarte

Pozycje D1–D4 **muszą** być rozstrzygnięte przed startem odpowiednich WP; reszta przed końcem fazy.
Nie udało się ich uzgodnić w trakcie planowania, bo M0 i M2 są planowane równolegle.

| # | Decyzja | Z kim | Blokuje | Stanowisko M1 |
|---|---|---|---|---|
| **D1** | Czy `engine/jobs` ma **pule priorytetów** (foreground symulacji / background renderu)? Bez tego meshing chunków będzie okresowo zjadał rdzenie symulacji | **M0** | WP-V2, WP-V4 | Potrzebna co najmniej dwupoziomowa pula albo limit współbieżności per kategoria zadania. Obejście: własny limiter w `engine/voxel` (≤ N_rdzeni−2 jobów meshingu) — gorsze, ale wykonalne |
| **D2** | Kto tworzy `wgpu::Device`/`Surface` — M0 (i M1 to przejmuje) czy od razu `engine/render`? | **M0** | WP-R1 | M1 twierdzi: M0 buduje rusztowanie do wyrzucenia, `engine/render` przejmuje własność w WP-R1 i kod M0 znika. Wymaga zgody, żeby M0 nie budował trwałego API |
| **D3** | Rezerwacja `StreamId` 100–119 dla M1 | **M0** | WP-W1 | Zakres przyjęty jednostronnie; do potwierdzenia w dokumencie 00 |
| **D4** | Format i **właściciel** maski przeszkód/spławności dla L-systemu dróg: bitset 1 m (32 MB dla 16 km) czy 2 m (8 MB)? Kto go trzyma — `sim/world` czy `engine/spatial` (M2)? | **M2** | WP-W7 | M1 proponuje: `TerrainQuery::obstacle_mask(rect)` generuje maskę **na żądanie dla prostokąta**, nikt jej nie trzyma w całości. Jeśli M2 potrzebuje pełnej rezydentnej maski — niech ją posiada `engine/spatial` |
| **D5** | Czy drogi, mosty i fundamenty **edytują voxele terenu destrukcyjnie**, czy są osobną warstwą nad terenem? Wpływa na rozmiar zapisu (nakładka edycji) i na to, czy `height_at` zwraca teren naturalny czy zastany | **M2** | WP-V4, WP-W7 | M1 proponuje: `height_at` = teren **naturalny** (deterministyczny z seeda, zawsze regenerowalny); nasypy/wykopy M2 żyją w nakładce edycji i mają osobne `height_built_at`. Inaczej tracimy regenerowalność i zapis rośnie o rzędy wielkości |
| **D6** | Jednostka wyczerpywania złoża i kto przelicza masę na voxele | **M6** | WP-W4 | M1 proponuje: prawdą jest `Mass` w gramach; `Deposit::voxels_for()` po stronie M1, wywołanie `edit_region` po stronie M6. `ResourceKind → GoodId` mapuje M6 |
| **D7** | Czy pogoda M8 **modyfikuje** `ClimateCell`, czy nakłada warstwę odchyleń? | **M8** | WP-W6 | M1 proponuje: `ClimateCell` jest **niezmienny** (klimatologia, roczny cykl); M8 trzyma `WeatherState` jako odchylenie od niego. Inaczej klimat przestaje być regenerowalny z seeda |
| **D8** | Domyślny rozmiar świata dla metropolii: 16×16 km w pełnej rozdzielczości, czy 16 km terenu z **obszarem zabudowy ograniczonym do 8×8 km**? Pierwsze jest 4× droższe w generacji i pamięci | **M2, M12** | WP-W1 | M1 proponuje wariant drugi — teren 16 km (widoki dalekie, złoża, rolnictwo), zabudowa 8 km. 400 tys. mieszkańców mieści się w 8×8 km przy realistycznej gęstości |
| ~~D9~~ | ~~Granica własności `engine/render` między M1 a M11~~ | **M11** | — | **ROZSTRZYGNIĘTE** — uzgodnione bezpośrednio z planem M11, zapisane w §6.1. M1 buduje `RenderGraph::register` + `GraphSlot`; M11 dodaje passy, nie przepisuje istniejących. `CameraMode::FirstPerson` i `clip_plane_z` dostarcza M1 (PRD §15.2 jest w zakresie M1), cap pass przekroju należy do M11. Właścicielem passa `cluster_assign` jest M1, limity ≤ 4096 świateł/klatkę i ≤ 256/klaster; redukcja z ~20 tys. latarni po stronie producenta snapshotu. Crate `sim-snapshot`: M1 właścicielem mechanizmu, M11 właścicielem schematu ładunku; reguła `cargo tree` przyjęta |
| **D10** | Czy wysokości terenu przechodzą na fixed-point i32 (plan awaryjny R1) już teraz, czy dopiero gdy test determinizmu padnie na innej platformie? | wewnętrzna | WP-W3 | **Ryzyko zmalało po K-6.** `core::det_math` obowiązuje w `sim/world`, a dobór `m = 0,5` / `n = 1` sprowadza gorącą pętlę erozji do `sqrt` i czterech działań — wszystkie dokładnie zaokrąglane w IEEE-754, identyczne na każdej platformie. Stanowisko M1: `f32` + test `terrain_thread_invariant` w CI; migracja na fixed-point tylko na dowodach z realnej rozbieżności między platformami |
| **D11** | Czy `EditReport::overlaps` wystarcza M2 do wykrywania kolidujących budynków, czy M2 potrzebuje **odrzucenia** kolidującej komendy zamiast rozstrzygnięcia „wyższy `seq` wygrywa"? | **M2** | WP-V4 | M1 proponuje: `apply_edits` zawsze stosuje wszystko i tylko raportuje nakładki — walidacja przed kolejkowaniem jest tańsza i należy do M2, bo tylko M2 wie, czy nakładka to błąd (dwa budynki) czy zamiar (budynek na nasypie) |

---

## 10. Szacunek wielkości

| WP | Rozmiar | Uzasadnienie |
|---|---|---|
| W1 — Parametry i szkielet potoku | **S** | Struktury + walidacja + raportowanie; brak algorytmiki |
| W2 — Baza wysokości | **M** | Szum jest znany, ale strojenie 5 regionów × 4 rozmiary to iteracje |
| **W3 — Hydrologia** | **XL** | Priority-flood + D8 + Braun–Willett + dyfuzja + klasyfikacja + wektoryzacja koryt; do tego determinizm równoległy i budżet czasu. Najtrudniejszy pakiet fazy |
| W4 — Geologia i złoża | **M** | Stos analityczny prosty; złoża to rozmieszczenie + kształty + bilans masy |
| W5 — Materializacja 1 m | **M** | Upsampling + wcięcie koryt + `ColumnSource`; pułapki na stykach kafli |
| W6 — Klimat i biomy | **M** | Model orograficzny + klasyfikacja; dużo strojenia, mało algorytmiki |
| W7 — Kontrakt + devtools | **S** | Głównie fasada nad tym, co już istnieje |
| V1 — Chunk, paleta, składowanie | **M** | Trzy warianty składowania + konwersje + fuzz |
| **V2 — Greedy meshing + AO** | **L** | Klasyczny algorytm, ale sześć przebiegów, AO per róg, otoczka, pakowanie, job system, benchmark |
| V3 — LOD | **M** | Agregacja z `ColumnSource` + skirt |
| **V4 — Streaming i edycja** | **L** | Rezydencja, priorytety, histereza, sub-alokator GPU, budżet z wywłaszczaniem, edycja z unieważnianiem |
| R1 — Bootstrap i frame graph | **M** | Frame graph to inwestycja, która spłaca się w M3/M4/M11 |
| **R2 — Pipeline voxelowy** | **L** | Arena, SSBO, culling, multi-draw + ścieżka zapasowa, depth prepass |
| R3 — Cienie kaskadowe | **M** | Znany problem, ale stabilizacja kaskad zawsze zjada czas |
| R4 — Clustered + słońce + niebo | **L** | Compute klastrów + pozycja słońca + LUT + ambient |
| R5 — Woda i daleki teren | **M** | Woda uproszczona (bez SSR), clipmap prosty |
| R6 — Post-processing i nakładki | **M** | Tonemap i FXAA tanie; `TerrainOverlay` to kontrakt dla M2, wart staranności |
| C1 — Kamera | **S** | Rig + tryby + clamp; `f64` względem kamery to jedyna subtelność |
| X1 — Testy, benchmarki, CI | **M** | Rozłożone po całej fazie, nie na końcu |

**Rozkład wysiłku:** `sim/world` ≈ 40%, `engine/render` ≈ 35%, `engine/voxel` ≈ 20%, reszta ≈ 5%.
Ścieżka krytyczna: **W1 → W2 → W3 → W5 → V1 → V2 → V3 → V4**, równolegle **R1 → R2 → R3 → R4**.
WP-W3 (hydrologia) i WP-V2 (meshing) to dwa pakiety, których niedoszacowanie przesunie całą fazę —
oba mają budżet wydajnościowy jako kryterium ukończenia, żeby problem wyszedł wcześnie, a nie w M11.
