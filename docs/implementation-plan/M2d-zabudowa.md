# M2d — Zabudowa

Podfaza 4 z 5 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2c (parcele), M1 (`engine/voxel`). |
| **Pakiety robocze** | WP10, WP11, WP12 |
| **Projekt techniczny** | §5.6 |
| **Wynik do pokazania** | Miasto w voxelach: budynki wygenerowane z gramatyki, z piętrami, lokalami i stanowiskami pracy. |
| **Kryterium zamknięcia** | Kryteria WP10–WP12; derywacja 50 tys. budynków równolegle daje wynik identyczny z jednowątkową. |
| **Poprzednia / następna** | `M2c-strefy-parcele-dzielnice.md` · `M2e-gospodarka-bazowa-i-wycena.md` |

Etap 6: język gramatyki architektury w `data/grammar/` z walidatorem, silnik derywacji, zapis terminali do voxeli i wnętrza logiczne (`Unit`, `Workplace`).

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| WP10 | Język gramatyki + parser + walidator | M0 (`io`) | schemat RON w `data/grammar/`, walidacja przy starcie | walidator odrzuca: brak `schema_version`, nieznany materiał, cykl `Ref`, przekroczenie budżetu węzłów; 100% plików z repo przechodzi |
| WP11 | Silnik derywacji gramatyki | WP10, WP8 | drzewo zakresów (scope tree), operacje `Split`/`Repeat`/`Extrude`/`Comp`/`Inset`/`Instance` | derywacja 50 tys. budynków równolegle daje identyczny wynik jak jednowątkowo |
| WP12 | Zapis voxeli + wnętrza logiczne | WP11, M1 (`voxel`) | terminale gramatyki → materiały; generacja `Unit` i `Workplace` | brak budynku przecinającego pas drogowy, wodę lub inną parcelę; suma m² lokali ≤ powierzchni brutto budynku |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

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
