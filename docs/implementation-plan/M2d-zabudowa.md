# M2d — Zabudowa

Podfaza 4 z 5 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
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
| **Poprzednia / następna** | `M2c-strefy-parcele-dzielnice.md` · `M2e-gospodarka-bazowa-i-wycena.md` |

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
