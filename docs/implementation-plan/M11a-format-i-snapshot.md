# M11a — Format i kontrakt snapshotu

Podfaza 1 z 5 fazy **M11 — Prezentacja** (`M11-prezentacja.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M1 (`engine/voxel`), M3, M4. |
| **Pakiety robocze** | WP1, WP11, WP2 |
| **Projekt techniczny** | §5.1, §5.2, §5.3 |
| **Wynik do pokazania** | Jeden model postaci w trzech wariantach palety rysowany jednym draw callem; tłum z M3 rysowany z bufora instancji. |
| **Kryterium zamknięcia** | Kryteria WP1, WP11 i WP2; render czyta wyłącznie snapshot, nigdy stanu symulacji. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M11b-animacja-i-lod.md` |

Format `.mvox` z indeksami slotów palety zamiast kolorów, palety per dzielnica × epoka oraz kontrakt snapshotu symulacja→render z instancingiem encji dynamicznych.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP1 | Format modelu voxelowego i sloty palety | M1 (`engine/voxel`) | M |
| WP11 | Palety per dzielnica × epoka (`data/palettes/`) | WP1 | S |
| WP2 | Kontrakt snapshotu i instancing encji dynamicznych | WP1, M3, M4 | L |

### WP1 — Format modelu voxelowego i sloty palety

**Opis.** Jeden format binarny `.mvox` dla wszystkiego, co nie jest chunkiem terenu: postać,
pojazd, prop wnętrza, szyld, maszyna. Siatka 0,25 m. Model nie przechowuje kolorów — przechowuje
**indeksy slotów palety**; kolor podstawia się dopiero przy rysowaniu. To jest mechanizm, dzięki
któremu „auto należy do konkretnego GD i ma swój kolor" nie wymaga duplikowania modelu.

Model jest zbiorem **części sztywnych** z hierarchią i pivotami — nie ma skinningu wierzchołkowego.
Voxelowa postać animuje się jak figurka: obracamy części wokół pivotów. To 10× tańsze i stylistycznie
spójne z resztą świata.

**Kryterium ukończenia.** Importer z MagicaVoxel `.vox` + walidator (`tools/mvoxc`) produkują
`.mvox` z trzema poziomami detalu; wyświetlenie jednego modelu postaci i jednego pojazdu w scenie
testowej z trzema różnymi wariantami palety rysowanymi jednym draw callem.

### WP11 — Palety per dzielnica × epoka

**Opis.** `data/palettes/*.ron`: dla każdej pary (typ dzielnicy, epoka) ograniczony zestaw kolorów
per rola slotu. Paleta ogranicza, a nie definiuje: slot `Outfit` w dzielnicy robotniczej epoki
przemysłowej losuje z 6 barw, w śródmieściu epoki współczesnej z 14. Losowanie deterministyczne:
`rng(world_seed, StreamId::Appearance, entity_index, 0)` — **raz, przy generacji encji**, wynik ląduje
w polu `appearance` i jest niezmienny.

**Kryterium ukończenia.** Zrzut ekranu tej samej dzielnicy w trzech epokach pokazuje wyraźnie różną
tonację; walidator danych odrzuca paletę bez pokrycia wszystkich ról slotów.

### WP2 — Kontrakt snapshotu i instancing encji dynamicznych

**Opis.** Rdzeń fazy. Definiujemy dokładnie, jakie dane render dostaje od symulacji, w jakim
rozmiarze i z jakim capem (sekcja 5.2), oraz ścieżkę snapshot → bufory instancji → draw call.
Cap jest twardy i ustalony z góry: **niezależnie od tego, czy miasto ma 40 tys. czy 400 tys.
mieszkańców, snapshot ma stały rozmiar 1,30 MB.**

**Kryterium ukończenia.** 20 000 mieszkańców i 6 000 pojazdów w kadrze, rysowane ≤ 24 draw callami,
CPU ≤ 1,5 ms na kompakcję; test `snapshot_size_is_constant` przechodzi dla miasta 40 tys. i 400 tys.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.1 Format modelu voxelowego — `.mvox`

```
MVOX — model voxelowy encji i propów. Siatka 0,25 m. Katalog: data/models/
┌──────────────────────────────────────────────────────────────────────────┐
│ Header  (16 B)                                                           │
│   magic      [u8;4]  = b"MVOX"                                           │
│   version    u16                                                         │
│   kind       u8      Character | Vehicle | Prop | Sign | Machine         │
│   part_count u8                                                          │
│   lod_count  u8      zawsze 3 (L0 pełny, L1 uproszczony, L2 impostor)    │
│   flags      u8      HasDoors | HasWheels | Emissive | TwoSided          │
│   bbox_q     [u8;3]  wymiary w voxelach 0,25 m (max 255 → 63,75 m)       │
│   _pad       [u8;3]                                                      │
├──────────────────────────────────────────────────────────────────────────┤
│ Parts[part_count]                                                        │
│   name_id    u16     PartName (Head, Torso, ArmL, Mast, WheelFL, …)      │
│   parent     u8      0xFF = korzeń                                       │
│   pivot      [i16;3] w 1/4 voxela (0,0625 m) — precyzja stawu            │
│   dims       [u8;3]                                                      │
│   lod_mask   u8      w których LOD-ach ta część istnieje                 │
│   voxels     RLE: (palette_slot u8, run u8)*  — 0 = pustka               │
├──────────────────────────────────────────────────────────────────────────┤
│ PaletteSlots[≤16]                                                        │
│   slot  u8 ∈ 0..15                                                       │
│   role  u8  Skin | Hair | OutfitMain | OutfitTrim | Accent | Metal       │
│             | Glass | Paint | Livery | Sign | Rubber | Emissive          │
└──────────────────────────────────────────────────────────────────────────┘
```

**Model nie zawiera koloru.** Zawiera indeks slotu 0..15. Kolor wynika z wpisu w `PaletteTable`,
wskazanego przez `palette_base` w instancji:

```rust
/// Jeden wpis = 16 kolorów RGBA8 = 64 B. Bufor GPU storage, 4096 wpisów = 256 KB.
pub struct PaletteTable { entries: Vec<[[u8; 4]; 16]> }

/// Klucz wariantu — deterministycznie rozwiązywany do indeksu w PaletteTable.
pub struct VariantKey {
    pub model: ModelId,          // u16
    pub district_palette: u16,   // (typ dzielnicy × epoka) z data/palettes/
    pub appearance: u32,         // spakowany wariant — patrz 5.2
}
```

**Poziomy detalu w samym modelu:**

| LOD | Postać | Pojazd |
|---|---|---|
| L0 | 12 części (głowa, tors, 2× ramię, 2× przedramię+dłoń, 2× udo, 2× łydka+stopa, plecak/torba, nakrycie głowy) | pełny: nadwozie, 4 koła, kabina, drzwi, ładunek, światła |
| L1 | 5 części (głowa, tors, ramiona zbite, nogi zbite, niesiony przedmiot) | bryła + 4 koła jako walce, bez detali kabiny |
| L2 | impostor: kafel w `ImpostorAtlas` | impostor |

**Dlaczego części sztywne, nie skinning.** Voxelowa figurka nie ma się jak deformować — ma się
obracać w stawach. Skinning wierzchołkowy wymagałby wag per wierzchołek (greedy meshing produkuje
wierzchołki, które nie mają tożsamości), macierzy kości na instancję i ponownego meshingu.
Części sztywne dają tę samą wizualną jakość za `part_index` w atrybucie wierzchołka.

### 5.2 Kontrakt z symulacją — dokładnie co i ile

To jest najważniejsza sekcja tego dokumentu. Render **nie ma dostępu do ECS**. Ma dostęp wyłącznie
do struktury `RenderSnapshot` w crate'cie `sim-snapshot`, który zawiera **tylko typy POD**
i nie zależy od żadnego `sim/*`.

```rust
// crate: sim-snapshot  (zależności: tylko engine-core)
#[repr(C)]
pub struct RenderSnapshot {
    // --- pola zakładane przez M1 w WP-R1 (crate musi istnieć od M1) ---
    pub tick: Tick,
    pub sim_minute: SimMinute,
    pub camera_hint: CameraHint,        // origin kamery w f64 — patrz konwencja w 5.3
    pub terrain_revision: u32,          // inwalidacja impostorów dzielnic (5.6)
    pub lights: SoaSlice<LightRecord>,  // ≤ 4 096 — budżet clustered shadingu M1

    // --- rekordy encji dokładane przez M11 ---
    pub citizens: SoaSlice<CitizenRenderRec>,   // ≤ 24 576
    pub vehicles: SoaSlice<VehicleRenderRec>,   // ≤  8 192
    pub sites:    SoaSlice<SiteRenderRec>,      // ≤  4 096
    pub crowd:    SoaSlice<CrowdDensityRec>,    // ≤  2 048  (tłum poza capem)

    pub weather: WeatherState,          // 12 B, jeden rekord na świat
    pub power:   [PowerRec; 64],        // 192 B, per dzielnica
    pub player:  PlayerViewRec,         // 24 B — postać do FPP + stan finansów do muzyki
}

#[repr(C)]
pub struct LightRecord {                // 20 B — wypełnia M11, konsumuje pass cluster_assign M1
    pub pos: [f32; 3],                  // 12 B  względem origin kamery
    pub color_intensity: u32,           //  4 B  RGB8 + intensywność
    pub radius: f16,                    //  2 B
    pub kind: u8,                       //  1 B  Window | StreetLamp | Headlight | Emissive
    pub flags: u8,                      //  1 B
}
```

**Uwaga do kształtu pól M1:** `lights` musi być `SoaSlice` o stałym capie, nie `Vec` — snapshot jest
podwójnie buforowany i publikowany do 30 razy na sekundę, więc alokacja na publikację jest dokładnie
tym, czego ta struktura ma unikać. Poza tym przyjmuję propozycję M1 bez zmian; `camera_hint`
zastępuje moje wcześniejsze `camera_anchor: [i32;3]`, bo origin musi być w `f64` (5.3).

#### Rekord mieszkańca — 32 B

```rust
#[repr(C)]
pub struct CitizenRenderRec {
    pub pos:         [i32; 3],  // 12 B  pozycja w mm świata (fixed-point, zasięg ±2147 km)
    pub yaw:         u16,       //  2 B  1/65536 obrotu
    pub anim_state:  u8,        //  1 B  ClipId — co robi (patrz 5.4)
    pub anim_phase:  u8,        //  1 B  faza 0..255 w klipie
    pub appearance:  u32,       //  4 B  wariant wyglądu — patrz niżej
    pub carry:       u8,        //  1 B  Nic | Torba | Siatka | Skrzynka | Narzędzie | Teczka
    pub flags:       u8,        //  1 B  InVehicle | InBuilding | Highlighted | PlayerOwned
    pub district:    u16,       //  2 B  DistrictId — paleta i przynależność do AmbientZone
    pub entity_lo:   u32,       //  4 B  dolne 32 bity Entity — picking, stabilność instancji
    pub _pad:        u32,       //  4 B
}   // = 32 B
```

**Pole `appearance` (u32) — pełny rozkład bitów:**

| Bity | Pole | Znaczenie |
|---|---|---|
| 0–2 | `body` | sylwetka / wzrost (8 wariantów) |
| 3–6 | `head` | karnacja + rysy (16) |
| 7–10 | `hair` | fryzura + kolor (16) |
| 11–15 | `outfit_class` | **klasa ubrania wyprowadzona z `JobRoleId`** (32) |
| 16–17 | `outfit_tier` | **zamożność GD 0..3** — ten sam fartuch, lepszy materiał i odcień |
| 18–23 | `palette_seed` | losowanie odcienia **w obrębie palety dzielnicy/epoki** (64) |
| 24–26 | `age_band` | dziecko / nastolatek / dorosły / senior / … (8) |
| 27–31 | `accessory` | czapka, parasol (przy deszczu!), teczka, wózek (32) |

`appearance` jest **niezmienne** dla danej encji poza momentami, gdy zmienia się zawód lub
zamożność GD — wtedy sim przelicza pola 11–17. Render nigdy go nie modyfikuje.

#### Rekord pojazdu — 40 B

```rust
#[repr(C)]
pub struct VehicleRenderRec {
    pub pos:         [i32; 3],  // 12 B
    pub yaw:         u16,       //  2 B
    pub pitch:       i16,       //  2 B  nachylenie (wzniesienie, rampa)
    pub model:       u16,       //  2 B  VehicleModelId — marka + klasa (§15.1)
    pub paint:       u16,       //  2 B  **indeks lakieru należący do GD-właściciela**
    pub livery:      u16,       //  2 B  oklejenie firmowe → SignAtlas; 0 = brak
    pub anim_state:  u8,        //  1 B  Jedzie|Parkuje|Stoi|Tankuje|Rozładunek|Awaria
    pub anim_phase:  u8,        //  1 B
    pub wheel_phase: u8,        //  1 B
    pub load:        u8,        //  1 B  wypełnienie 0..255 → widoczny ładunek na pace
    pub flags:       u8,        //  1 B  Lights | Blinker | EngineOn | PlayerOwned
    pub occupants:   u8,        //  1 B  ile sylwetek w kabinie
    pub district:    u16,       //  2 B
    pub entity_lo:   u32,       //  4 B
    pub _pad:        [u8; 6],   //  6 B
}   // = 40 B
```

`paint` rozwiązuje wprost wymaganie PRD §16.3: *„auto należy do konkretnego GD i ma swój kolor"*.
Sim wypełnia `paint` z `HouseholdId` właściciela; render podstawia kolor pod slot `Paint`.
Zero duplikacji modeli — 48 modeli pojazdów × 64 lakiery = 3072 wizualnie różnych aut z 48 meshy.

#### Rekord zakładu / budynku aktywnego — 24 B

```rust
#[repr(C)]
pub struct SiteRenderRec {
    pub entity_lo:    u32,      //  4 B
    pub pos:          [i32; 3], // 12 B  punkt referencyjny (komin / rampa / wejście)
    pub activity:     u8,       //  1 B  0..255 — **0 = linia stoi (cisza + bezruch maszyn)**
    pub emission:     u8,       //  1 B  0..255 — gęstość dymu z komina (§15.3)
    pub lights:       u8,       //  1 B  0..255 — jasność okien; 0 przy blackoucie
    pub sign_id:      u16,      //  2 B  kafel w SignAtlas — nazwa firmy gracza
    pub ambient_kind: u8,       //  1 B  rodzaj łoża dźwiękowego emitera
    pub stock_fill:   u8,       //  1 B  0..255 — wypełnienie regałów dla InteriorKit
    pub flags:        u8,       //  1 B  bit 0: SITE_FAULT; bity 1–2: smoke_kind (PM/para/mieszany)
}   // = 24 B — dawne pole _pad zużyte na flags, rozmiar bez zmian
```

**Skale trzech pól ustalone z M6** (`M6-lancuch-dostaw.md` §6.4.3), wszystkie jednokierunkowe
i bez konsekwencji ekonomicznych:

| Pole | Wzór | Co z tego wynika wizualnie |
|---|---|---|
| `activity` | `255 · Σ(throughput_nom × utilization) / Σ throughput_nom`; `Running` = pełne, **`Setup` = 0,4** (maszyna pracuje, nie produkując — i słychać ją), `Idle`/`Starved`/`Blocked`/`Broken` = 0 | Prędkość animacji maszyn i wózków oraz `gain` emiterów. `activity == 0` znaczy „nic się nie rusza", nie „brak danych" — PRD §15.5 spełnione bez dodatkowego bitu |
| `emission` | `min(255, 255 · pm_g_per_min / EMISSION_REF_PM_G_PER_MIN)` — **skala absolutna, wspólna**, stała odniesienia w `data/tuning/supply.ron` | Gęstość dymu. Mała kotłownia **nie** dymi jak huta, więc mapa zanieczyszczeń nie kłamie tam, gdzie gracz wybiera miejsce pod dom |
| `stock_fill` | `255 · max(used_vol/cap_vol, used_mass/cap_mass)` po slotach **z wyłączeniem `WarehouseRole::Shelf`** | Liczba skrzynek na regałach zaplecza. `max` z dwóch stosunków, bo wąskie gardło zależy od towaru: chipsy wypełniają objętość przy śmiesznej masie, zboże odwrotnie |

**`SITE_FAULT` (bit 0) — awaria to nie to samo co bezczynność.** Oba stany dają `activity == 0`,
czyli bezruch i ciszę, ale gracz musi je odróżnić: zakład bez zmiany jest w porządku, zakład
zepsuty kosztuje pieniądze co minutę. Wizualnie awaria = brak dymu mimo pory pracy + zatrzymane
maszyny + cichy, powtarzalny sygnał alarmowy w `engine/audio`. Ikona w nakładce należy do M9.

**`PlumeKind` (bity 1–2)** — rodzaj pióropusza, zdefiniowany przez M6 (`M6-lancuch-dostaw.md` §5.4):

| Wartość | Źródło | Wygląd w `render::weather` |
|---|---|---|
| `None = 0` | montownia, szwalnia, magazyn | brak emitera — zero kosztu |
| `Steam = 1` | chłodnia kominowa, elektrownia, mleczarnia, browar | biały, **wznosi się i szybko znika**, wysoka przezroczystość |
| `Soot = 2` | huta, koksownia, cementownia, kotłownia węglowa | ciemnoszary, **opada i się rozlewa**, niska przezroczystość |
| `Chemical = 3` | rafineria, zakład chemiczny, papiernia | żółtawo-brunatny, **wolno dryfuje z wiatrem**, średnia przezroczystość, lekkie drgania |

**Wartość jest deklarowana w `data/recipes/`, nie wyliczana ze stosunku `pm_g`/`co2_g`** — M6
sprawdził, że uczciwie się nie da: chłodnia kominowa i kotłownia węglowa mają zbliżone `co2_g`
przy zupełnie różnym pióropuszu, bo to, co leci z komina, wynika z tego, co dzieje się w środku,
a nie ze stosunku dwóch wskaźników. Przy okazji jest moddowalne bez dotykania kodu.

Wartość pochodzi z receptury **aktualnie uruchomionej**, a przy stojącym zakładzie z domyślnej
receptury archetypu — **celowo nie z ostatnio uruchomionej**, bo wtedy komin zmieniałby barwę przy
każdym przezbrojeniu i czytałoby się to jak migotanie, a nie jak informacja.

Rozdzielenie `Chemical` od `Soot` ma konkretny powód po stronie czytelności: w łańcuchu referencyjnym
M6 rafineria stoi obok terminalu paliwowego i elektrowni, czyli **trzy duże kominy w jednym kadrze** —
gdyby dymiły identycznie, gracz nie odczytałby z widoku, który zakład właśnie stanął. Zostaje 5 bitów
rezerwy, gdyby podział okazał się za gruby.

#### Pozostałe rekordy

```rust
#[repr(C)] pub struct CrowdDensityRec {   // 4 B — tłum poza capem, per krawędź grafu pieszego
    pub edge: u16, pub count: u8, pub kind: u8,
}
#[repr(C)] pub struct WeatherState {      // 12 B
    pub precipitation: u8, pub kind: u8,      // deszcz / śnieg / grad
    pub temp_c: i8, pub wind: [i8; 2],
    pub fog_density: u8, pub cloud: u8,
    pub season: u8, pub snow_cover: u8, pub daylight: u8, pub _pad: u8,
}
#[repr(C)] pub struct PowerRec {          // 3 B → 64 dzielnic = 192 B
    pub district: u16, pub supply_ratio: u8,  // < 128 → blackout
}
#[repr(C)] pub struct PlayerViewRec {     // 24 B
    pub citizen: u32,                         // entity_lo postaci gracza (FPP)
    pub eye: [i32; 3], pub yaw: u16, pub pitch: i16,
    pub liquidity_ratio: u8,                  // płynność / 30-dniowe koszty stałe → MusicMood
    pub profit_trend: i8,
}
```

#### Rachunek bajtów i pasma

| Warstwa | Cap | Rekord | Rozmiar |
|---|---:|---:|---:|
| Mieszkańcy | 24 576 | 32 B | 786 KB |
| Pojazdy | 8 192 | 40 B | 328 KB |
| Zakłady | 4 096 | 24 B | 98 KB |
| Tłum agregatowy | 2 048 | 4 B | 8 KB |
| Światła (budżet clustered M1) | 4 096 | 20 B | 80 KB |
| Pogoda + zasilanie + gracz + nagłówek | — | — | < 1 KB |
| **Razem jeden bufor** | | | **≈ 1,30 MB** |
| **Rezydentnie (podwójne buforowanie)** | | | **≈ 2,60 MB** |

**Pasmo.** Publikacja snapshotu jest ograniczona do **30 Hz** (co drugi tick ruchu mikro przy
prędkości 1×; przy 3×/10×/50× co N-ty tick — render i tak nie wyświetli więcej niż 60 klatek/s,
a klatki pośrednie interpoluje). Stąd: **≤ 37 MB/s** ruchu w RAM.

Dla porównania: 400 tys. mieszkańców × 32 B = 12,8 MB **na publikację**, czyli 384 MB/s —
trzydziestokrotnie więcej przy zerowym zysku wizualnym, bo 95% tych encji jest poza kadrem
albo poniżej piksela.

**Kluczowa właściwość: rozmiar snapshotu nie zależy od wielkości miasta.** Miasto 40 tys.
i miasto 400 tys. dają identyczny 1,30 MB. To jest to, co pozwala M12 zamknąć metropolię
w budżecie pamięci bez dotykania renderu.

#### Jak sim wypełnia snapshot (i dlaczego to nie jest skan 400 tys.)

```rust
// wywoływane raz na publikację, w jobie równoległym do reszty ticku
pub fn fill_render_snapshot(world: &World, view: &ViewQuery, out: &mut RenderSnapshot);

pub struct ViewQuery {          // render → sim, jednokierunkowo, tylko odczyt po stronie sim
    pub aabb: Aabb,             // frustum kamery rozszerzony o 20% (histereza na obrót)
    pub eye: [i32; 3],
    pub caps: SnapshotCaps,
}
```

1. `engine/spatial` (M2) zwraca komórki gridu przecinające `aabb` → typowo 30–60 tys. kandydatów,
   nie 400 tys. Skan jest **lokalny**, nie globalny.
2. `partial_sort` po `dist²` do capu (24 576). Remis rozstrzygany po `entity_index` — **deterministycznie**.
3. Encje **poza** capem nie znikają ze świata — trafiają do `crowd` jako gęstość per krawędź.
4. Koszt: ≈ 0,8 ms na wątku sim, w jobie równoległym; nie na ścieżce krytycznej ticku.

**`ViewQuery` jest jedynym kanałem render → sim i nie może wpływać na stan.** Jest argumentem
funkcji czystej `fill_render_snapshot(&World, …)` — sygnatura `&World`, nie `&mut World`,
gwarantuje to na poziomie kompilatora.

#### Co robimy, gdy w kadrze jest 20 tys. widocznych encji

Kaskada czterech niezależnych mechanizmów, żaden nie wymaga udziału symulacji:

1. **Cap snapshotu (24 576 / 8 192).** Powyżej — encje w ogóle nie trafiają do snapshotu.
   Pasmo pozostaje stałe niezależnie od gęstości kadru.
2. **Twarde limity instancji per warstwa LOD** (5.5). Przy przekroczeniu limitu encje ponad limit
   **degradują o jeden poziom** — tablica jest już posortowana po odległości, więc degradują
   najdalsze. Przejście jest niewidoczne, bo dotyczy encji przy granicy progu.
3. **Warstwa L3 — tłum agregatowy.** Encje, które wypadły z L2, oraz cały `crowd` rysowane są jako
   instancjonowane plamki cienia + sylwetki 4×6 px na chodnikach, jeden draw call, koszt niezależny
   od liczby. Z 400 m tłum i tak jest plamą ruchu — i tak właśnie ma wyglądać.
4. **`RenderBudget`** (5.10) obniża globalną skalę progów, jeśli p95 klatki przekracza cel.

Wartości progowe: 20 000 widocznych encji to **scena projektowa, nie awaryjna** —
`bench_district` jest zbudowany właśnie na tej liczbie i musi mieścić się w 16,6 ms.

### 5.3 Instancing — bufor GPU

```rust
#[repr(C)]
pub struct GpuInstance {          // 32 B — to samo dla postaci, pojazdów i propów
    pub pos:          [f32; 3],   // 12 B  względem camera_anchor (f32 wystarcza: ±4 km @ 0,5 mm)
    pub yaw_pitch:    u32,        //  4 B  2× f16
    pub palette_base: u16,        //  2 B  offset w PaletteTable
    pub model_lod:    u16,        //  2 B  (ModelId << 2) | lod
    pub anim:         u32,        //  4 B  (ClipId << 16) | (phase << 8) | part_flags
    pub tint:         u32,        //  4 B  RGBA8 — filtry i podświetlenia z §14.2
    pub extra:        u32,        //  4 B  carry / load / livery / wheel_phase
}
```

20 000 instancji × 32 B = **640 KB uploadu na klatkę** → 38 MB/s po PCIe. Bufor persistent-mapped,
ring o trzech klatkach.

**Konwencja współrzędnych — obowiązkowo identyczna z M1, inaczej encje drgają.** M1 trzyma pozycję
kamery w `f64` (mapa 16 km; `f32` traci precyzję na krawędziach) i do shaderów przekazuje pozycje
**względem kamery**. `GpuInstance.pos` liczę tą samą drogą: `pos_mm` z rekordu (`i32`, milimetry)
minus origin kamery w `f64`, dopiero wynik różnicy rzutowany na `f32`. Odejmowanie **przed**
konwersją, nigdy po — inaczej przy współrzędnej rzędu 8 km `f32` ma krok ~0,5 m i pieszy skacze
między klatkami. To było ostrzeżenie M1 i jest tu zapisane jako wymóg, nie jako uwaga.

**Ścieżka klatki (CPU, wątek renderu — nigdy wątek sim):**

```
snapshot (read-only)
  → filtr frustum per encja            ~0,10 ms / 20k
  → klasyfikacja LOD wg dist²          ~0,08 ms
  → radix sort po (model_lod)          ~0,20 ms   [zbija draw calle]
  → zapis GpuInstance do ring buffera  ~0,35 ms
  → draw_indexed_indirect per (model, lod)
                                       ≈ 0,75 ms CPU, budżet 1,5 ms
```

Sortowanie jest tym, co sprowadza 20 000 encji do ≤ 24 draw calli: wszystkie instancje tego samego
`(model, lod)` leżą obok siebie.
