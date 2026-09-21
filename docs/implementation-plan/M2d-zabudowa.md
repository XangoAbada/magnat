# M2d — Zabudowa

Podfaza 4 z 6 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2c: `Parcel` (obrys, `frontage`, strefa, `block`, `district`), `Block.{zone, epoch_ring, neighborhood}`, `District.{style, income_tier, kind}`, `CityFields`, `ParcelSet.street_index`. M1: `engine/voxel` (`EditQueue`, `EditOp`, `MaterialRegistry`), `TerrainQuery`. |
| **Pakiety robocze** | WP10, WP11, WP12, **WP12b** (warstwa transportowa i podgląd — D2/D3), **WP15a** (wycena `pass_1` — D1, przeniesiona z M2e) |
| **Projekt techniczny** | §5.6, §5.6b, §5.7 w części `pass_1` (formuła w `M2e-gospodarka-bazowa-i-wycena.md`) |
| **Wynik do pokazania** | **Miasto w kliencie graficznym `magnat`**: przelot kamerą nad zabudową wygenerowaną z gramatyki, z nawierzchnią jezdni, nasypami i mostami; budynki mają piętra, lokale i stanowiska pracy. |
| **Kryterium zamknięcia** | Kryteria WP10–WP12, WP12b i WP15a; derywacja 50 tys. budynków równolegle daje wynik identyczny z jednowątkową; żaden voxel fundamentu nie graniczy z powietrzem od spodu (ryzyko R9). |
| **Poprzednia / następna** | `M2c-strefy-parcele-dzielnice.md` · `M2f-roznorodnosc-zabudowy.md` |

Etap 6: język gramatyki architektury w `data/grammar/` z walidatorem, silnik derywacji, kolejkowanie terminali do voxeli i wnętrza logiczne (`Unit`, `Workplace`). Dodatkowo — korekty D1–D3 — przebieg `pass_1` wyceny gruntu (wejście doboru gramatyki), warstwa transportowa w voxelach i podpięcie miasta do klienta graficznego.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| WP10 | Język gramatyki + parser + walidator | M0 (`io`) | schemat RON w `data/grammar/`, walidacja przy starcie | walidator odrzuca: brak `schema_version`, nieznany materiał, cykl `Ref`, przekroczenie budżetu węzłów; 100% plików z repo przechodzi |
| WP15a | Wycena gruntu, przebieg `pass_1` | WP8 (M2c) | czynniki dostępne po Etapie 5: klasa drogi frontowej, długość frontu, `noise`, pole i kształt działki, `epoch_ring`; zapis do `Parcel.land_value_per_m2` | każda parcela zabudowywalna ma wartość > 0; `avg(OldTown) > avg(Suburb)`; dwa przebiegi tego samego ziarna dają identyczne wartości. **Przeniesione z M2e (D1)**: `pass_1` jest wejściem WP11, a nie jego wynikiem |
| WP11 | Silnik derywacji gramatyki | WP10, WP8, **WP15a** | drzewo zakresów (scope tree), operacje `Split`/`Repeat`/`Extrude`/`Comp`/`Inset` | derywacja 50 tys. budynków równolegle daje identyczny wynik jak jednowątkowo |
| WP12 | Kolejkowanie voxeli + wnętrza logiczne | WP11, M1 (`voxel`) | terminale gramatyki → `EditOp` w `EditQueue` (**nie** zapis wprost — D4); niwelacja parceli przed posadowieniem (R9); generacja `Unit` i `Workplace` | brak budynku przecinającego pas drogowy, wodę lub inną parcelę; suma m² lokali ≤ powierzchni brutto budynku; żaden voxel fundamentu nie graniczy z powietrzem od spodu |
| WP12b | Warstwa transportowa w voxelach + podgląd miasta | WP12, M1 (`voxel`, `render`) | nawierzchnia jezdni, nasypy, mosty i tunele jako `EditOp` (§5.6b); podpięcie `generate_city` do `tools/magnat` | żaden segment jezdny nie wisi nad terenem ani nie jest w nim zatopiony poza tolerancją 1 voxela; `magnat --seed …` pokazuje miasto, `--no-city` wraca do widoku M1; 60 FPS w widoku dzielnicy |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.6 Etap 6 — gramatyka architektury voxelowej

#### Język opisu (`data/grammar/*.ron`)

Gramatyka kształtowa nad **drzewem zakresów**: zakres = zorientowany prostopadłościan
(początek, 3 osie, wymiary). Reguła przekształca zakres w listę zakresów potomnych.

**Terminale nie zapisują voxeli — kolejkują edycje** (korekta D4). M1 zamknął to inaczej,
niż zakładała decyzja 9.2/1 dokumentu fazy: nie ma `ChunkWriter::stage`/`commit_sorted`,
jest `EditQueue` z `EditOp::{Fill, Carve, Terrace}`, a porządek kanoniczny wyprowadzany
jest **z treści komendy**, nie z numeru nadanego przy kolejkowaniu. Konsekwencja dla WP11
i WP12 jest dobra: derywacja może iść równolegle bez żadnej dyscypliny numerowania,
bo to nie kolejność `push` decyduje o wyniku.

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
oraz `Fill(material)`.

`Instance(voxel_asset)` **wypada z M2** (korekta D5): `engine/voxel` nie ma operacji
stawiania prefabrykatu — `EditOp` zna `Fill`, `Carve` i `Terrace`, a wyeksportowany
`Rot90` nie ma dziś ani jednego konsumenta. Prefabrykaty to detal wizualny, czyli M11
(§2 dokumentu fazy), i to M11 zgłosi M1 operację `Place`, kiedy będzie jej potrzebował.
Limit `Instance ≤ 256 na budynek` zostaje zapisany na tę okoliczność, ale w M2 nie ma
czego ograniczać.

Twarde limity (egzekwowane przez walidator i przez runtime):
głębokość derywacji ≤ 12, węzłów na budynek ≤ 4 096. Przekroczenie = błąd generacji
zliczany w `GenerationReport`, nie ciche obcięcie. Limit `Instance ≤ 256 na budynek`
zostaje zapisany dla M11, ale w M2 nie ma czego ograniczać (D5).

#### Wejście z M2c — imiennie

Cztery kanały parametryzacji czytają dokładnie te pola i żadnych innych:

| Kanał | Pole źródłowe (M2c) |
|---|---|
| strefa | `Parcel.zone` (a nie `Block.zone` — działka wewnętrzna bywa `Green` w kwartale mieszkaniowym) |
| epoka | `Block.epoch_ring` → indeks w `ZoneResult::rings`, klucz epoki z `data/epochs/` |
| wartość gruntu | `Parcel.land_value_per_m2` — **zero do czasu WP15a**, stąd zależność WP11 → WP15a (D1) |
| styl dzielnicy | `District.style: StyleId` = `kind × 32 + founded_epoch`; `District.income_tier` wchodzi do `wage_band` |

Geometria: `Parcel.poly` (obrys w metrach, `PolyArena`), `Parcel.area_m2`,
`Parcel.frontage: Frontage { seg, t0, t1 }`. `snap_to_frontage: true` znaczy: wejście
ląduje na osi segmentu `frontage.seg` w parametrze z przedziału `[t0, t1]` — M2c
gwarantuje, że przedział jest niezerowy dla każdej parceli poza `Green`/`Water`/`Extraction`.

#### Parametryzacja (cztery kanały, zgodnie z PRD §4.2 Etap 6)

| Kanał | Wpływ |
|---|---|
| **strefa** | zbiór gramatyk kandydujących, typ wnętrza, obowiązkowe cofnięcie od granicy |
| **epoka** (`epoch_ring` kwartału) | zestaw reguł i paleta materiałów: XIX w. → cegła + tynk + dachówka, lata 70. → prefabrykat + papa, 2010 → szkło + tynk cienkowarstwowy |
| **wartość gruntu** | liczba kondygnacji (interpolacja w `count: Range`), poziom detalu (`Details` z progiem `land_value`), jakość materiału, powierzchnia lokalu |
| **styl dzielnicy** (`StyleId`) | filtr `applies.styles` + paleta kolorów elewacji + wybór kształtu dachu |

Wybór gramatyki dla parceli: filtr `applies`, następnie losowanie ważone
`weight × style_affinity(district.style)` z `rng(seed, StreamId::BuildingPick, parcel_idx, Tick(0))`
(czwarty argument `rng` jest typu `Tick` — 00 §3.1).
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
/// Indeks w globalnej tablicy lokali. Nie `Entity`: lokali jest ~190 tys., nie są
/// odpytywane przekrojowo po archetypach i żyją w ciągłym zakresie `Building.units`
/// (ta sama logika co `K-16` dla partii i ofert).
pub struct UnitIdx(pub u32);
/// Indeks gramatyki w katalogu `data/grammar/`, nadawany przy ładowaniu w kolejności
/// alfabetycznej klucza — tak jak `GoodId` (00 §5).
pub struct GrammarId(pub u16);

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

---

### 5.6b Warstwa transportowa w voxelach i podgląd miasta (WP12b)

Sekcja dopisana korektami D2 i D3: §6 dokumentu fazy obiecuje M1 zapis „brył budynków,
**nasypów, mostów**", ale żaden pakiet nie był tego właścicielem, a bez tego miasto w 3D
ma budynki wiszące nad nietkniętym terenem i autostradę przechodzącą przez wzgórze.

#### Co trafia do `EditQueue`

| Element | Operacja | Uwagi |
|---|---|---|
| korpus drogi | `Terrace { aabb, target_z: niweleta − 1 voxel, material }` | `Terrace` robi nasyp i wykop jednym wariantem — po to M1 go dodał („nasyp albo wykop drogowy (M2)"). Rzędna z `RoadNode.z_dm` interpolowanej wzdłuż segmentu (M2b, korekta B20) |
| nawierzchnia | `Fill { aabb, material: wg klasy i epoki }` | jeden voxel na wierzchu korpusu; `asphalt` / `cobble` / `gravel` z `data/materials/`, wybór po `RoadClass` × `epoch_ring` |
| nasyp jako budowla | — | mieści się w `Terrace`; osobna operacja nie jest potrzebna, a `RoadStructure::Embankment` niesie już wysokość |
| most | `Fill` pomostu na rzędnej niwelety + `Carve` prześwitu `clearance_dm` pod nim | przęsła, pylony i barierki to detal → M11 |
| tunel | `Carve { CarveShape::Tunnel { from, to, radius } }` | promień z `row_m / 2`; `CarveShape::Tunnel` istnieje w M1 dokładnie pod to |
| niwelacja parceli (R9) | `Terrace` obrysu do mediany wysokości + skarpa przy różnicy > 1,5 m | **przed** posadowieniem budynku; to jest mitygacja R9 i należy do WP12, nie tutaj |

**Kolejność warstw jest rozstrzygana geometrią, nie rangą operacji.** `EditOp::rank`
stosuje `Fill` (0) przed `Carve` (1) przed `Terrace` (2), więc korpus wyrównywany na końcu
zamazałby nawierzchnię położoną wcześniej. Dlatego `Terrace` celuje w **jeden voxel poniżej**
niwelety (podbudowa), a `Fill` kładzie nawierzchnię na niwelecie: zakresy się nie
przecinają i kolejność przestaje mieć znaczenie. To nie jest obejście rangi — to jest
przekrój drogi.

#### Budżet

Metropolia: ~640 km dróg × średnio 16 m pasa ≈ 10 km² nawierzchni. Przy voxelu 0,5 m
to ~41 mln voxeli powierzchni i tyleż komend `Fill`, gdyby kolejkować per voxel —
więc kolejkujemy **per segment**, prostopadłościanami w układzie osi segmentu:
~14 tys. segmentów × 2 operacje (`Terrace` + `Fill`) = ~28 tys. komend. Pomiar do
uzupełnienia po implementacji; limit zgłaszany do M1, bo to jego budżet pamięci (§7).

#### Podgląd w kliencie

`tools/magnat` istnieje od M1 i ma kamerę, streaming voxeli, cienie, HDR i nakładki `F3`.
Nie pokazuje miasta z jednego powodu: woła `generate()`, nie `generate_city()`.
WP12b dokłada:

1. wywołanie `generate_city` po wygenerowaniu terenu, za flagą `--no-city` wracającą
   do czystego widoku M1 (przydatne przy diagnozie regresji terenu);
2. zastosowanie `EditQueue` do `VoxelWorld` przed pierwszą klatką;
3. tryb kamery „nad miastem" ustawiony na `CityData.center`, zamiast na środku mapy;
4. wypis `GenerationReport` do konsoli klienta — te same linie co w headless.

Nakładka wartości gruntu i karta inspekcji parceli **nie tutaj** — to WP16 w M2e.

---

## Zmiany wpisane po M2c

Zgodnie z `K-18`: poprawki znalezione przy pracy nad wcześniejszą podfazą wędrują w przód
w tej samej zmianie. Gwiazdką oznaczone te, które zmieniają **zakres albo kryterium**.

| # | Zmiana | Dlaczego |
|---|---|---|
| D1 ★ | **`pass_1` wyceny gruntu przenosi się z M2e do M2d jako WP15a.** WP15 w M2e zostaje jako WP15b (`pass_2`) | §5.7 mówi wprost, że `pass_1` biegnie „po Etapie 5" i zasila „wybór gramatyki, liczbę kondygnacji, `rent_hint`" — czyli WP11, który jest tutaj. Przy pierwotnym przydziale WP11 czytałby `Parcel.land_value_per_m2` równe zeru, bo WP15 leży za nim na ścieżce krytycznej (`WP11 → WP12 → WP14 → WP15`). Kanał „wartość gruntu" z §5.6 byłby martwy, a liczba kondygnacji brałaby się z niczego |
| D2 ★ | **Nowy pakiet WP12b: warstwa transportowa w voxelach** (nawierzchnia, korpus drogi, mosty, tunele) | §6 dokumentu fazy wymienia w „konsumuję" zapis „brył budynków, **nasypów, mostów**", ale opis WP12 mówi wyłącznie o budynkach i żaden pakiet nie był właścicielem jezdni. Bez tego „wynik do pokazania" tej podfazy jest nieosiągalny: budynki stoją nad nieruszonym terenem, a droga po nasypie nie istnieje fizycznie |
| D3 ★ | **WP12b podpina `generate_city` do `tools/magnat`** | Klient graficzny woła dziś tylko `generate()`. „Miasto w voxelach" bez tego kroku jest widoczne wyłącznie w testach — a to jest **jedyna** podfaza M2, w której da się miasto obejrzeć przed M2e |
| D4 | Terminale gramatyki **kolejkują `EditOp`**, nie piszą do chunków. Decyzja 9.2/1 dokumentu fazy jest rozstrzygnięta i przechodzi do §9.1 | M1 zamknął to inaczej niż propozycja M2: nie `ChunkWriter::stage`/`commit_sorted`, tylko `EditQueue` z `EditOp::{Fill, Carve, Terrace}` i porządkiem kanonicznym wyprowadzanym **z treści komendy**. Dla WP11 to zmiana na lepsze — derywacja idzie równolegle bez dyscypliny numerowania — ale przykład w §5.6 opisywał API, którego nie ma |
| D5 | Operator `Instance(voxel_asset)` **wypada ze zbioru M2** | `engine/voxel` nie ma operacji stawiania prefabrykatu: `EditOp` zna `Fill`, `Carve` i `Terrace`, a wyeksportowany `Rot90` nie ma ani jednego konsumenta. Prefabrykaty są detalem wizualnym, czyli M11 wg §2 — i to M11 zgłosi M1 operację `Place`. Przy okazji zmniejsza to ryzyko R4 („gramatyka staje się drugim językiem programowania") |
| D6 ★ | **Semantyka `RoadFlags::NO_HEAVY` do poprawienia przed WP12** — flaga ma znaczyć „zakaz ruchu ciężkiego", a nie „istnieje jakikolwiek limit tonażu" | Kod M2b ustawia `NO_HEAVY` dla każdej klasy o `max_tonnage_t > 0`, czyli także dla `Collector` (40 t). Test T4 fazy wymaga, żeby 100 % budynków na `Logistics`/`IndustryHeavy` miało rampę **przy drodze bez `NO_HEAVY`** — przy obecnej regule jedynymi takimi drogami są `Highway` i `Arterial`, a strefy przemysłowe frontują zwykle do kolektora. Kryterium jest więc dziś niespełnialne nie z powodu urbanistyki, tylko z powodu progu w jednej linii. **Propozycja domyślna:** `NO_HEAVY` tylko dla `Pedestrian` oraz klas o `max_tonnage_t < 24` (`Local` 18 t, `Service` 8 t); `Collector` traci flagę. Zmiana dotyka też M4 (K-14), więc idzie razem z regeneracją hashy — teraz jest na to najtaniej, bo budynków jeszcze nie ma |
| D7 | Rampa wybierana spośród **wszystkich dróg przylegających** do parceli, nie tylko z `Parcel.frontage` | `Frontage` wskazuje najbliższy odcinek jezdni, który dla działki przemysłowej bywa ulicą dojazdową. Brak drogi bez `NO_HEAVY` w sąsiedztwie → wpis do `GenerationReport`, nie cicha rampa przy zakazie |
| D8 | `UnitIdx` i `GrammarId` zdefiniowane wprost | Oba były używane w sygnaturach §5.6 bez definicji. `UnitIdx` to indeks, nie `Entity` — tą samą logiką co `K-16`: lokali jest ~190 tys., żyją w ciągłym zakresie `Building.units` i nikt nie odpytuje ich przekrojowo po archetypach |
| D9 | Czwarty argument `rng` to `Tick`, nie liczba | Przykład `rng(seed, StreamId::BuildingPick, parcel_idx, 0)` nie kompiluje się wobec `00` §3.1 |

**Do potwierdzenia przy starcie M2d, nie rozstrzygnięte tutaj:** czy `data/grammar/`
potrzebuje osobnego zestawu dla kwartałów, które M2c oznaczył jako pozamiejskie
(`Agriculture` o działkach 3–20 ha) — zabudowa zagrodowa rządzi się inną skalą niż
miejska i pierwotny szkic gramatyki jej nie przewiduje.

**Rozstrzygnięte przy starcie:** zabudowa zagrodowa **nie potrzebuje osobnego zestawu
gramatyk, tylko własnych cofnięć** — `data/grammar/zagroda.ron` wyraża skalę przez
`massing` (dom przy drodze, reszta działki zostaje polem), a nie przez nowe operatory.
Gdyby bryła skalowała się z działką, gospodarstwo na 20 ha dostałoby budynek o boku 400 m.

---

## Zmiany wpisane po M2d

Numeracja `E-n`; odwołania z kodu (`korekta E3`) wskazują na tę tabelę. Gwiazdką
oznaczone te, które zmieniają **zakres albo kryterium**, a nie tylko sposób liczenia.

| # | Korekta | Dlaczego |
|---|---|---|
| E1 ★ | **`engine/voxel` dostaje bryłę zorientowaną**: `Obb3`, `EditOp::Prism` i `CarveShape::Prism`. §5.6b zakłada `Terrace`/`Fill` na `IAabb3` | Ulica biegnie pod dowolnym kątem, a budynek stoi równolegle do swojej ulicy. Prostopadłościan osiowy obejmuje przy 45° √2 razy szerszy pas — czyli sąsiednią działkę — a rozbicie bryły na komendy-wiersze daje przy 50 tys. budynków **miliony** komend zamiast tysięcy. Obrót wyłącznie wokół pionu; test przynależności to same porównania, więc determinizm zostaje |
| E2 | Klucz porządku kanonicznego `EditQueue` domknięty **odciskiem treści operacji** | Dwie komendy o tym samym źródle, zasięgu i randze (dwa okna w jednej ścianie, dwa `Fill` różnym materiałem) miały dotąd **równy** klucz, a `sort_by_key` jest stabilny — o kolejności decydowała kolejność `push`, czyli harmonogram wątków. Dokładnie to, czego zakazuje nagłówek modułu `edit` |
| E3 ★ | Komendy miasta stosowane **leniwie**, przez nowy `EditIndex` (komendy + kubełki chunków), a nie przez `EditOverlay` | `EditOverlay` trzyma **każdy zmieniony voxel**. Miasto to ~50 tys. budynków po kilkanaście tysięcy voxeli — setki milionów wpisów, dziesiątki gigabajtów. Indeks trzyma komendy (metropolia: 600 tys.) i rasteryzuje je przy materializacji chunka. Ta sama rasteryzacja, ten sam wynik. Nakładka zostaje dla edycji **runtime'owych** (M6 drążący kopalnię w trakcie gry) |
| E4 | `EditOverlay::apply_to` i rasteryzacja przyjmują **`lod`** | Nakładka jest indeksowana w LOD0. Bez rzutowania na grubszą siatkę miasto znikało poza pierścieniem LOD0 (192 m), czyli w każdym widoku dzielnicy. Błąd był w M1 od początku, ale nie miał konsumenta |
| E5 ★ | **`Workplace` powstaje w M2d** z rodzaju lokalu i `data/jobs/roles.ron`, z `site: Option<SiteId> = None`. Liczbę stanowisk i przypisanie do zakładu nadpisuje M2e (WP13) | §5.6 mówi „liczba `Workplace` wynika z archetypu zakładu (Etap 7)", ale archetypy (`data/buildings/`) powstają dopiero w M2e, a „wynik do pokazania" tej podfazy brzmi „budynki mają piętra, lokale **i stanowiska pracy**". Bez przelicznika w `data/jobs/` kontrakt `wage_band` dla M3 (§6, decyzja 9.1/8) też nie miałby wartości. Przelicznik `m2_per_workplace` stoi tymczasowo przy roli, nie przy archetypie |
| E6 ★ | **`Green` i `Extraction` nie dostają zabudowy w M2d.** Budżet §7 fazy przypisuje im 300 i 120 budynków — te obiekty należą do Etapu 7 (M2e §5.8 pkt 5: „jedna parcela = jedno gospodarstwo/kopalnia") | Park, w którym każda działka dostaje budynek awaryjny, przestaje być parkiem — a właśnie tak wyglądał pierwszy przebieg. Kopalnia nie jest bryłą z gramatyki, tylko zakładem na złożu |
| E7 ★ | Parcela o froncie **< 6 m** nie dostaje gramatyki, także awaryjnej: jest odpadem podziału pasowego i zostaje pusta | Bez tej bramki udział gramatyki awaryjnej mierzył **ziarnistość podziału na parcele** (6,4 % na mieście 4 km), a nie luki w katalogu gramatyk — czyli kryterium „< 2 %" badało coś innego, niż nazywa. Po bramce: 0,4 % (4 km) i 1,1 % (8 km, metropolia) |
| E8 | Filtry `applies` rozluźniane **etapami w ustalonej kolejności** (epoka → styl dzielnicy → wartość gruntu), z osobnym licznikiem `relaxed` w raporcie. Strefa i wymiary działki nie są pomijane nigdy | Miasto z 1990 ma kwartały z pięciu epok, a katalog nie pokrywa każdej kombinacji (strefa × epoka × styl). Bez rozluźniania 46 % budynków szło na gramatykę awaryjną. Licznik jest po to, żeby luka w danych była widoczna, zamiast chować się pod „działa" |
| E9 | Nowe pole `Applies.coverage`: udział pasujących działek, które ta gramatyka faktycznie zabudowuje | Filtr `applies` mówi „czy wolno", nie „jak często". Bez pokrycia każde pole rolne dostawało zagrodę. To jedyny sposób, w jaki M2 tworzy **świadomie** pustą działkę |
| E10 | Cele operatora `Ref(id)` mieszkają w polu `refs:` pliku gramatyki | §5.6 używa `Ref("parter_uslugowy")`, nie mówiąc, gdzie leży cel. Lista par, nie mapa — kolejność w danych ma być kolejnością w pamięci (00 §3.2) |
| E11 | `massing` to **jedna struktura** (cofnięcia, próg dziedzińca, maksymalna głębokość traktu), nie enum `Perimeter / Freestanding / Hall` | Warianty różniłyby się wyłącznie wartościami tych samych pól. Trzy warianty o identycznym kształcie to abstrakcja bez drugiego konsumenta (00, „Dobre praktyki": YAGNI przed SOLID) |
| E12 ★ | Dach spadzisty jest **schodkowany bryłami zagnieżdżonymi**, a stopień ma co najmniej **2,5 m wysięgu poziomego** (≤ 6 stopni) | §5.6b twierdzi, że „przy voxelu 0,5 m stopnie są poniżej progu widoczności". Voxel ma **1 m w poziomie** i 0,5 m w pionie (`CHUNK_SPAN_M`/`CHUNK_DIM` wobec `VOXEL_HEIGHT_DM`), więc dwie zagnieżdżone bryły obrócone pod kątem rasteryzują się z własnym schodkiem i schodki się mijają — dach wychodzi dziurawy jak wafel. Widać to **tylko** na podglądzie; żaden test tego nie łapie. Bryły zagnieżdżone (każda od podstawy połaci, nie jedna na drugiej) wykluczają szczelinę z definicji |
| E13 ★ | Kryterium WP15a `avg(OldTown) > avg(Suburb)` zastąpione porównaniem **grup**: rdzeń (`OldTown` + `InnerCity`) wobec obrzeża (`Suburb` + `Village`) | `DistrictKind::OldTown` dostaje wyłącznie dzielnica, której **dominującym** pierścieniem jest najstarsza epoka, a ta ma z `data/epochs/` 3 % powierzchni. Na większości ziaren nie ma ani jednej takiej dzielnicy i porównanie wypadało „0 vs 0" — czyli kryterium spełnione tożsamościowo, czego zakazuje `K-18` pkt 3. To samo dotyczy testu T12 fazy |
| E14 | `RoadFlags::NO_HEAVY` poprawione zgodnie z **D6**: flaga dla `Pedestrian` i klas o tonażu < 24 t (`Local` 18 t, `Service` 8 t); `Collector` (40 t) ją traci. Reguła w jednym miejscu — `RoadClass::forbids_heavy()` | Zaplanowane w D6, wykonane tutaj: bez tego test T4 („rampa przy drodze bez `NO_HEAVY`") był niespełnialny, bo strefy przemysłowe frontują zwykle do kolektora. Przy okazji zniknął duplikat progu w dwóch miejscach (L-system i ulice lokalne) |
| E15 | `data/jobs/roles.ron` powstaje **w M2d** | §6 fazy wymienia `data/jobs/` w „konsumuję", ale żadna podfaza nie była jego właścicielem. Katalog jest minimalny z rozmysłem — jedna rola na rodzaj lokalu; pełny katalog ról to M7 |
| E16 | `data/materials/building.ron` — mury, dachy i nawierzchnie dołożone do rejestru M1 | `data/materials/` miało wyłącznie materiały terenowe. Rejestr scala katalog i nadaje `MaterialId` po posortowanym kluczu, więc dopisanie pliku przenumerowuje identyfikatory — i właśnie dlatego w zapisie gry trzyma się klucz tekstowy (00 §5) |
| E17 | `generate_city` przyjmuje `&MaterialRegistry` i `&JobPool` | Gramatyka odwołuje się do materiałów po kluczu, a derywacja 50 tys. budynków jest jedynym zrównoleglonym krokiem fazy. Sygnatura z §6 nie przewidywała ani jednego, ani drugiego |
| E18 | `Building.aabb` wymagał typu, którego nie było: `Aabb3` dopisany do `engine/spatial` | `core::IAabb3` żyje w voxelach i jest całkowitoliczbowy, a selekcja do kadru (M11) porównuje bryłę z piramidą widzenia **w metrach** |
| E19 | Niwelacja parceli i pasa drogowego to **para brył** (wypełnienie pod niweletą + wykop nad nią), a nie `EditOp::Terrace` | Konsekwencja E1: `Terrace` przyjmuje wyłącznie `IAabb3`. Kolejność warstw rozstrzyga wtedy `EditSource` — źródło jest **pierwszym** kluczem porządku kanonicznego, więc numery źródeł są kolejnością robót na budowie: ziemia, nawierzchnia, bryły, otwory, tunele. §5.6b rozstrzygał tę kolejność geometrią („podbudowa jeden voxel pod niweletą"), co przestało wystarczać, gdy doszły otwory okienne |
| E20 ★ | **Blok `Details` z §5.6 nie wszedł do enuma `Rule` i nie jest długiem tej podfazy — jest zakresem nowej, `M2f-roznorodnosc-zabudowy.md`** (WP18–WP20). Katalog został na 12 plikach, po jednym przedstawicielu na rodzaj zabudowy | Kryterium M2d („budynki mają piętra, lokale i stanowiska pracy") jest spełnione bez detalu bryły, więc podfaza domyka się uczciwie — ale §5.6 obiecywał `Cornice`, `Balcony`, `Chimney` i lukarny, a żaden pakiet nie był ich właścicielem. Wpisanie tego jako `TODO` w `Rule` byłoby złamaniem `K-18` pkt (d). Przy okazji wyszło, że brakuje **jednego** operatora, nie pięciu: `Offset` poszerza zakres z każdej strony, więc balkonu, wykusza i ryzalitu nie da się zapisać wcale, a gzyms, komin i pas okienny już się dają. Rozstrzygnięcie granicy z M11 — decyzja 21 w §9.2 fazy: detal poniżej 1 voxela nie jest w M2 wyrażalny z powodu rastra, nie z powodu zakresu |

### Zmierzone

| Miara | Metropolia 16 km (400 tys.) | Miasto 8 km (120 tys.) | Miasto 4 km (40 tys.) |
|---|---|---|---|
| wycena `pass_1` | 8 ms | 3 ms | 0,5 ms |
| gramatyka + derywacja + wnętrza (8 wątków) | 131 ms | 41 ms | 10 ms |
| warstwa transportowa w voxelach | 805 ms | 295 ms | 43 ms |
| indeks edycji (kubełkowanie po chunkach) | 722 ms | 216 ms | 41 ms |
| **razem M2d** | **~1,7 s** | ~0,55 s | ~95 ms |
| budynki | 14 510 | 5 146 | 1 135 |
| lokale / mieszkania | 157 360 / 117 557 | 55 619 / 40 810 | 13 306 / 10 131 |
| stanowiska pracy | 263 603 | 98 085 | 24 875 |
| gramatyka awaryjna | 1,1 % | 1,1 % | 0,4 % |
| rozluźnień filtru `applies` | 1 978 | 295 | 402 |
| działki bez zabudowy (odpad podziału) | 3 094 | 1 148 | 186 |
| komendy voxelowe (zabudowa + drogi) | 553 197 + 46 196 | 193 727 + 17 741 | 50 tys. + 3,3 tys. |
| FPS w widoku dzielnicy (RTX 4070 Ti SUPER) | — | mediana 1 249, 1 % low 198 | — |

Budżet §7 dla metropolii: „derywacja gramatyki + zapis voxeli (równolegle) ≤ 28 s" —
zmierzone 1,7 s, z czego sama derywacja 0,13 s. Zapas jest tak duży, bo voxele **nie są**
materializowane w generacji: powstają komendy, a rasteryzacja dzieje się dopiero przy
wczytaniu chunka (korekta E3).

Dwie liczby **nie** trzymają budżetów §7 i nie jest to wada tej podfazy: budynków
w metropolii jest 14,5 tys. wobec ~49,5 tys., a mieszkań 117 tys. wobec 167 tys.
Idą one w ślad za liczbą parcel (18,3 tys. wobec ~42 tys.), którą M2c zgłosił już jako
rozjazd gęstości. Bilans mieszkań i stanowisk (test T10) zamyka się w M2e — tam, gdzie
jest czym kalibrować.

### Kontrola wzrokowa

Podgląd w kliencie jest tu przyrządem, nie ilustracją. Trzy rzeczy wyszły **wyłącznie**
z obejrzenia miasta i żaden test ich nie widział:

1. **Dachów nie było w ogóle.** Reguła `Roof` dostaje zakres bryły o zerowej wysokości
   (leży na rzędnej posadowienia) i wypadała na wspólnym warunku „zakres zdegenerowany".
   W liczbach nie widać tego niczym: budynki mają wysokość, lokale i stanowiska.
2. **Dach spadzisty wychodził kratownicą** — korekta E12. Powód leży w rasteryzacji,
   nie w geometrii: przy voxelu 1 m w poziomie stopnie cieńsze niż ~2 m mijają się.
3. **Okna czytały się jako sztruks.** Otwór co 3 m przy voxelu 1 m to dwa voxele muru
   na dwa voxele otworu — z odległości dzielnicy jednolita pionowa prążkowanica.
   Rozstaw 4,5–5 m i otwór 1,2–1,4 m wysokości dają ścianę, na której widać okna.

## Czego ta podfaza nie zostawia następnej

- `Parcel.land_value_per_m2` jest wypełnione (`pass_1`) i **jest wejściem** doboru
  gramatyki, nie jej wynikiem. M2e liczy `pass_2` na tym samym `ValueCtx`.
- `Parcel.status` = `Built` i `Parcel.building` wskazują budynek wszędzie tam, gdzie coś
  stanęło. Działki `Vacant` to albo odpad podziału, albo świadome pokrycie gramatyki.
- `Workplace` istnieje i ma widełki, ale **nie ma zakładu** (`site: None`) — to jest
  jedyna rzecz, którą M2e musi w tych rekordach dopisać (albo nadpisać ich liczbę).
- `CityData.edits` niesie komplet komend voxelowych z indeksem chunkowym; konsument
  stosuje je przy materializacji. Nakładka per voxel nie powstaje nigdzie w generacji.
