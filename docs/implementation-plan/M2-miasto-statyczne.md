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

## 4. Pakiety robocze i podfazy

Kolejność jest istotna: etapy generacji tworzą łańcuch zależności zamknięty **trzema przebiegami
wyceny** (patrz WP15a/WP15b i sekcja 5.7).

Faza jest rozbita na **5 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M2a — Indeksy przestrzenne** | WP1, WP2 | 5.1 | `cargo bench -p magnat-spatial` na danych syntetycznych: cztery struktury odpowiadają na zapytania w budżecie, bez generatora miasta. | `M2a-indeksy-przestrzenne.md` |
| **M2b — Szkielet transportu** | WP3, WP4, WP5, WP6 | 5.2 | `headless preview --field roads` → PNG z bramami, arteriami, strukturami inżynierskimi i konturami kwartałów. | `M2b-szkielet-transportu.md` |
| **M2c — Strefy, parcele, dzielnice** | WP7, WP5b, WP8, WP9 | 5.3, 5.4, 5.5 | Podgląd: mapa stref i parcel z granicami dzielnic; kliknięcie parceli daje strefę, właściciela, frontę drogową i dzielnicę. | `M2c-strefy-parcele-dzielnice.md` |
| **M2d — Zabudowa** | WP10, WP15a, WP11, WP12, WP12b | 5.6, 5.6b, 5.7 (`pass_1`) | **Miasto w kliencie graficznym `magnat`**: zabudowa z gramatyki, nawierzchnia jezdni, nasypy i mosty; budynki z piętrami, lokalami i stanowiskami pracy. | `M2d-zabudowa.md` |
| **M2e — Gospodarka bazowa i wycena** | WP13, WP14, WP15b, WP16, WP17 | 5.7 (`pass_2`), 5.8 | Pełny artefakt fazy z §1 dokumentu fazy: `GenerationReport`, nakładka wartości gruntu z rozbiciem na czynniki, zielone `--test consistency`. | `M2e-gospodarka-bazowa-i-wycena.md` |

Ścieżka krytyczna: WP1 → WP2 → WP4 → WP6 → WP7 → WP9 → WP8 → WP15a → WP11 → WP12 → WP14 → WP15b → WP17.
WP3, WP9, WP10, WP13, WP16 można prowadzić równolegle.

**Korekta po M2b:** kolej towarowa (druga połowa WP5) przenosi się do M2c jako **WP5b**,
bo jej trasy prowadzą do klastrów stref, a strefy powstają dopiero w WP7. Ścieżka krytyczna
się nie zmienia — już zakładała `WP4 → WP6 → WP7`. Szczegóły: korekta B1 w dokumencie M2b.

**Korekta po M2c:** wewnątrz podfazy **WP9 wykonuje się przed WP8**. Powód: §5.5 wymaga
hierarchii tablicowej, czyli przenumerowania kwartałów po przypisaniu dzielnic — zrobione
po powstaniu parcel wymagałoby przestawiania dwóch sprzężonych tablic zamiast jednej.
WP9 nie potrzebuje z WP8 niczego. Szczegóły: korekta C8 w dokumencie M2c.

**Korekta po M2d:** tabela `E-1`–`E-19` w `M2d-zabudowa.md`. Osiem pozycji zmienia zakres
albo kryterium; trzy dotykają kontraktu z M1, bo `engine/voxel` dostał **bryłę zorientowaną**
(`Obb3`, `EditOp::Prism`, `CarveShape::Prism`) i **leniwy `EditIndex`** zamiast nakładki
per voxel — bez pierwszego miasto musiałoby stać w układzie Manhattan, bez drugiego zapis
zabudowy metropolii to dziesiątki gigabajtów. Zmiany wpisane w przód do M2e: tabela
`F1`–`F8` w `M2e-gospodarka-bazowa-i-wycena.md`.

**Korekta po M2c, wpisana w przód (`K-18`):** trzy zmiany w podziale pakietów między M2d
a M2e, uzasadnione w tabeli „Zmiany wpisane po M2c" dokumentu `M2d-zabudowa.md`:

- **WP15 rozcięty na WP15a i WP15b.** `pass_1` wyceny („po Etapie 5") zasila dobór
  gramatyki, liczbę kondygnacji i `rent_hint`, czyli WP11 — leży więc w M2d, a nie za nim.
  W M2e zostaje `pass_2` jako WP15b. `pass_0` zrealizowany w M2c jako punktacja stref.
- **Nowy WP12b:** warstwa transportowa w voxelach (nawierzchnia, korpus drogi, mosty,
  tunele) plus podpięcie `generate_city` do klienta `tools/magnat`. §6 obiecuje M1 zapis
  „brył budynków, nasypów, mostów", ale żaden pakiet nie był ich właścicielem.
- **Ryzyko R9** (kolizja budynków z terenem) jest mitygowane jawnie w WP12 i wchodzi do
  kryterium zamknięcia M2d, zamiast żyć wyłącznie w tabeli ryzyk.

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | `engine/spatial` — indeksy przestrzenne | `M2a-indeksy-przestrzenne.md` |
| 5.2 | Etap 3 — szkielet transportu | `M2b-szkielet-transportu.md` |
| 5.3 | Etap 4 — strefowanie | `M2c-strefy-parcele-dzielnice.md` |
| 5.4 | Etap 5 — sieć lokalna i parcele | `M2c-strefy-parcele-dzielnice.md` |
| 5.5 | §4.3 — dzielnice i hierarchia | `M2c-strefy-parcele-dzielnice.md` |
| 5.6 | Etap 6 — gramatyka architektury voxelowej | `M2d-zabudowa.md` |
| 5.7 | Statyczna wycena gruntu (§6.7, część statyczna) | `M2e-gospodarka-bazowa-i-wycena.md` |
| 5.8 | Etap 7 — gospodarka bazowa (firmy jako obiekty danych) | `M2e-gospodarka-bazowa-i-wycena.md` |

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
`RoadNetwork`, `RoadSegment`, `RoadNode` (z `z_dm` — rzędną niwelety, korekta B20 w M2b),
`RoadClass`, `ClassSpec`, `RoadStructure`, `RoadFlags`, `NodeFlags`, `CityGate`, `GateKind`, `GateProfile`;
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
| 18 | Bezpieczny zapis voxeli przy budynku przecinającym granicę chunka (blokowało WP12) | **Rozstrzygnięte przez M1 inaczej, niż proponował M2.** Nie ma `ChunkWriter::stage`/`commit_sorted`; jest `EditQueue` z `EditOp::{Fill, Carve, Terrace}`, a **porządek kanoniczny wyprowadzany jest z treści komendy**, nie z numeru nadanego przy kolejkowaniu — bo numer nadany przy równoległym `push` nie jest deterministyczny. M2 **nie pisze do chunków, tylko kolejkuje edycje** (00 §3.4). Dla WP11 to zmiana na lepsze: derywacja idzie równolegle bez żadnej dyscypliny numerowania | 5.6 (M2d), korekta D4 |
| 19 | Które z pięciu zgłoszonych rozszerzeń `TerrainQuery` M1 przyjmuje (blokowało WP3, WP7) | **Wszystkie pięć przyjęte.** `water_depth_at`, `deposit_at`, `soil_quality_at`, `prevailing_wind` i `flood_risk_at` są w `TerrainQuery` z domyślnymi implementacjami nad danymi, które już istnieją. M2b i M2c używają wszystkich pięciu | 6 (tabela rozszerzeń) |
| 20 | Czy epoka startowa dopuszcza pierścienie starsze od niej (blokowało WP7) | **Tak, propozycja przyjęta i zrealizowana w M2c.** `EpochTable::rings_for` zachowuje wszystkie epoki zamknięte przed rokiem startu plus tę, w której gra się zaczyna; udziały są renormalizowane. Miasto z 1990 ma starówkę, bo pierścienie są historią zabudowy, a nie stanem techniki dostępnej graczowi | 5.3 (M2c) |

### 9.2 Nadal otwarte — do rozstrzygnięcia przed startem wskazanego WP

| # | Decyzja | Blokuje | Propozycja M2 | Stan |
|---|---|---|---|---|
| 3 | Moment, w którym wartość gruntu przestaje być statyczna (M5 transakcje / M10 pełny model) | WP15b | pole `Parcel.land_value_per_m2` zostaje, M5/M10 je nadpisują; **bez** traita `LandValueSource` — jedna implementacja nie potrzebuje abstrakcji | propozycja bez sprzeciwu |
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
| WP12 | Kolejkowanie voxeli + wnętrza logiczne | L |
| WP12b | Warstwa transportowa w voxelach + podgląd w kliencie | M |
| WP13 | Archetypy zakładów i szablony łańcuchów | M |
| WP14 | Domknięcie łańcuchów produktowych | M |
| WP15a | Wycena gruntu, `pass_1` (M2d) | S |
| WP15b | Wycena gruntu, `pass_2` + rozbicie (M2e) | M |
| WP16 | Nakładka UI + karta inspekcji | S |
| WP17 | Testy spójności + raport generacji | M |

Rozkład (po korektach B1, C8 i D1–D3): 4 × L, 13 × M, 3 × S. Ciężar fazy leży w geometrii podziału na parcele (WP8)
i w gramatyce budynków (WP11 + WP12) — te trzy WP to ok. 45% pracy fazy i tam należy
zaplanować rezerwę.
