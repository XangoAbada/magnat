# M11b — Animacja i LOD wizualne

Podfaza 2 z 5 fazy **M11 — Prezentacja** (`M11-prezentacja.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M11a (format, snapshot), M1 (teren). |
| **Pakiety robocze** | WP3, WP4 |
| **Projekt techniczny** | §5.4, §5.5, §5.6 |
| **Wynik do pokazania** | Postać chodzi, siada i pracuje; dalszy plan schodzi na impostory bez widocznego przeskoku. |
| **Kryterium zamknięcia** | Kryteria WP3 i WP4. |
| **Poprzednia / następna** | `M11a-format-i-snapshot.md` · `M11c-wnetrza-i-kamera.md` |

`PoseAtlas`, klipy i maszyna stanów animacji części sztywnych oraz progi LOD wizualnego z `ImpostorAtlas`.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP3 | Animacja: `PoseAtlas`, klipy, stany | WP1, WP2 | L |
| WP4 | LOD wizualne i `ImpostorAtlas` | WP2, WP3, M1 | L |

### WP3 — Animacja

**Opis.** `AnimationClip` z klatkami póz (12 fps, styl stop-motion), wypalone w starcie do
`PoseAtlas` — statycznego bufora GPU. Instancja niesie tylko `(clip_id, phase)`; vertex shader
sam odczytuje macierze części. **Zero uploadu kości na klatkę.** Klipy: chodzenie, bieg,
stanie, siedzenie, wsiadanie/wysiadanie, niesienie zakupów, oraz zestaw `Work(JobRoleId)` —
kasjer, magazynier, operator linii, biurowy, budowlaniec, kierowca.

**Kryterium ukończenia.** 20 000 animowanych postaci przy < 0,3 ms dodatkowego czasu GPU względem
tych samych postaci w pozie bazowej; test `pose_atlas_fits_budget` (≤ 512 KB).

### WP4 — LOD wizualne i `ImpostorAtlas`

**Opis.** Cztery poziomy dla encji, atlas impostorów dla dzielnic (runtime, LRU) i dla encji
(wypalany przy buildzie). Impostor encji przechowuje **indeks slotu palety**, nie kolor — dzięki
czemu wariant wyglądu działa aż do najdalszego poziomu i tłum w oddali nie robi się szary.

**Kryterium ukończenia.** Przelot kamerą z 2 km do poziomu ulicy nie pokazuje popu geometrii
większego niż 1 piksel przy 1080p (pomiar: różnica obrazu między klatkami przy przekroczeniu progu);
scena `bench_city` mieści się w budżecie VRAM impostorów (192 MB).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 Animacja

```rust
pub struct AnimationClip {
    pub id:          ClipId,       // u8 — max 256 klipów w grze
    pub kind:        ClipKind,
    pub frames:      u8,           // 8..32
    pub fps:         u8,           // 12 — świadomy „stop-motion", spójny ze stylem voxelowym
    pub loop_mode:   LoopMode,     // Loop | Once | PingPong
    pub pose_offset: u32,          // offset w PoseAtlas
    pub parts_mask:  u16,          // które części animowane; reszta z pozy bazowej
}

/// **Projekcja `core::ActivityKind` na animację, nie drugi słownik czynności (K-8).**
/// To, *co* agent robi, jest własnością `engine/core`; `ClipKind` mówi tylko,
/// *jak to narysować*. Odwzorowanie jest wiele-do-jednego i żyje w `engine/voxel::anim`.
pub enum ClipKind {
    Idle, Walk, WalkCarry, Run, Sit, Stand,
    EnterVehicle, ExitVehicle, Board,           // wsiadanie (§15.4)
    Work(JobRoleId),                            // animacja wg zawodu (§15.1, §15.4)
    Shop, Queue,                                // zakupy
    Drive, Park, Refuel, Unload,                // pojazdy: jadą, parkują, tankują
    Forklift, Crane, RampLoad,                  // wózki widłowe, dźwigi, towar na rampach
}

/// Jedyne miejsce, w którym czynność domenowa staje się animacją.
/// Funkcja czysta, bez stanu — nowa czynność w `core` bez ramienia tutaj
/// jest błędem kompilacji, a nie cichym brakiem animacji.
pub fn clip_for(activity: core::ActivityKind, role: JobRoleId) -> ClipKind;

/// Stan animacji instancji. **Nie istnieje jako komponent ECS** — jest wyprowadzany
/// przy wypełnianiu snapshotu i mieści się w 2 B rekordu.
pub struct AnimationState { pub clip: ClipId, pub phase: u8 }

/// Statyczny bufor GPU. Wypalany przy starcie z klipów, nigdy nie zmieniany.
/// 64 klipy × 32 klatki × 12 części × 16 B (quat u16×4 + trans i16×3 + pad) ≈ 393 KB.
pub struct PoseAtlas { buffer: wgpu::Buffer, index: Vec<ClipEntry> }
```

**Decyzja architektoniczna: symulacja nie prowadzi animacji.** Sim wie tylko, *co* agent robi
(`ClipKind` wynika z jego bieżącego zdarzenia DES — §17.3). Faza jest wyliczana przy wypełnianiu
snapshotu funkcją czystą:

```rust
phase = (((instant.0 / (1000 / clip.fps as u64)) + entity_index as u64) % clip.frames as u64) as u8
```

Konsekwencje: **zero bajtów stanu animacji w ECS**, zero wpływu na hash symulacji, brak wymogu
ciągłości bufora między klatkami, i naturalne rozsunięcie fazy między agentami (składnik
`entity_index`) — tłum nie maszeruje w takt.

Vertex shader czyta `(clip, phase)` z instancji i `part_index` z atrybutu wierzchołka, po czym
pobiera macierz z `PoseAtlas`. **Zero uploadu kości na klatkę** — to jest różnica między 2,9 MB
macierzy na klatkę a zerem.

**Wózki widłowe i dźwigi** to modele `kind = Machine` z 2–3 częściami ruchomymi. Dźwig na budowie
nie potrzebuje żadnego stanu symulacji — jego faza to `f(building_id, instant)`. Wózek widłowy
w magazynie gracza dostaje fazę z `SiteRenderRec.activity` (stoi, gdy `activity == 0`).

### 5.5 Progi LOD wizualnego — liczby

Odległości podane dla FOV 60° i 1080p. `RenderBudget` skaluje je globalnie w zakresie 0,5×–1,0×.

**Mieszkańcy** (model 0,25 m, wysokość ok. 7 voxeli):

| LOD | Odległość | Model | Animacja | Cap instancji | Draw calle |
|---|---|---|---|---:|---:|
| L0 | 0–40 m | 12 części + akcesoria + niesiony przedmiot | pełny klip, 12 fps | 800 | ≤ 6 |
| L1 | 40–120 m | 5 części | klip, `parts_mask` zredukowana | 4 000 | ≤ 4 |
| L2 | 120–350 m | impostor 32×48 px, 8 kierunków × 4 fazy chodu | faza chodu z atlasu | 20 000 | 1 |
| L3 | > 350 m | plamka cienia + sylwetka 4×6 px | brak | bez limitu | 1 |

**Pojazdy:**

| LOD | Odległość | Model | Cap | Draw calle |
|---|---|---|---:|---:|
| L0 | 0–60 m | pełny: koła, kabina, drzwi, ładunek, światła | 400 | ≤ 8 |
| L1 | 60–200 m | bryła + koła-walce | 2 000 | ≤ 4 |
| L2 | 200–600 m | impostor 64×32 px, 16 kierunków | 8 000 | 1 |
| L3 | > 600 m | pominięte — ruch reprezentują strumienie nakładki (M2) | — | 0 |

**Propy wnętrz (`InteriorKit`):** L0 tylko przy aktywnym `CutPlane` lub FPP, 0–50 m, cap 3 000,
≤ 6 draw calli. Poza tym nie istnieją — wnętrze niewidoczne nie jest generowane.

**Chunki (należą do M1 — tu tylko dla ciągłości progów):** L0 0–256 m, L1 (2×) 256–768 m,
L2 (4×) 768–2048 m, **impostor dzielnicy > 2048 m (M11)**.

**Budżety klatki na scenach referencyjnych:**

| Scena | Draw calle | Trójkąty | Instancje | GPU p95 | Cel |
|---|---:|---:|---:|---:|---|
| `bench_street` (FPP / zoom max) | ≤ 900 | ≤ 4,0 M | ≤ 6 000 | ≤ 14 ms | 60 FPS |
| `bench_district` | ≤ 1 500 | ≤ 9,0 M | ≤ 24 000 | ≤ 15 ms | **60 FPS** |
| `bench_city` | ≤ 600 | ≤ 6,0 M | ≤ 2 000 | ≤ 30 ms | **30 FPS** |
| `bench_night_rain` | ≤ 1 700 | ≤ 9,5 M | ≤ 24 000 | ≤ 15 ms | 60 FPS |
| `bench_interiors` | ≤ 1 400 | ≤ 7,0 M | ≤ 12 000 | ≤ 15 ms | 60 FPS |

CPU po stronie renderu: **≤ 4,0 ms** na przygotowanie klatki, na wątku innym niż sim.

### 5.6 `ImpostorAtlas`

```rust
pub struct ImpostorAtlas {
    color: wgpu::Texture,        // Texture2DArray, RGBA8 — R = palette_slot, GBA = AO/alfa/normal.z
    depth: wgpu::Texture,        // R16Unorm — do poprawnego przecięcia z geometrią
    tiles: Vec<ImpostorTile>,
    lru:   LruQueue<TileId>,
    budget_bytes: usize,
}
pub struct ImpostorTile {
    pub slot: u16, pub dirs: u8, pub frames: u8,
    pub size: [u16; 2], pub generation: u32, pub last_used: u32,
}
```

**Kluczowa decyzja: impostor przechowuje indeks slotu palety, nie kolor.** Kanał R to
`palette_slot`, kolor podstawia się w fragment shaderze z `PaletteTable` przez `palette_base`
instancji. Dzięki temu system wariantów działa aż do L2 — tłum na 300 m dalej ma zróżnicowane
ubrania, a auta mają swoje lakiery, zamiast zamienić się w jednolitą szarość.

**Impostory encji — wypalane przy buildzie** (nie zależą od pozycji w świecie):

| Typ | Kafle | Rozmiar kafla | VRAM |
|---|---:|---|---:|
| Mieszkaniec | 32 `outfit_class` × 8 kier. × 4 fazy = 1 024 | 32×48 RGBA8 | 6,3 MB |
| Pojazd | 48 modeli × 16 kier. = 768 | 64×32 RGBA8 | 6,3 MB |
| **Razem** | | | **≈ 13 MB** |

**Impostory dzielnic — generowane w runtime**, bo zależą od tego, co gracz zbudował:

- Blok 128×128 m, 8 azymutów, 1 pas elewacji (kamera w widoku miasta ma ograniczone pochylenie).
- Kafel 128×128 px, RGBA8 + R16 depth = 6 B/px → **786 KB na blok**.
- Budżet: **192 MB VRAM → 244 bloki rezydentne**, LRU. Miasto 4×4 km ma 1024 bloki, ale w jednym
  kadrze horyzontalnym nigdy nie widać więcej niż ok. 200.
- Regeneracja: **≤ 4 bloki na klatkę**, osobny pass z budżetem 1,5 ms, kolejka priorytetowa po
  odległości. Amortyzacja jest obowiązkowa — regeneracja 40 bloków naraz to widoczne zacięcie.
- Inwalidacja **dwustopniowa**: `RenderSnapshot.terrain_revision` (M1) jako tani filtr wstępny
  „czy w ogóle cokolwiek się zmieniło", a dopiero potem `generation` per blok 128×128 m.
  **Licznik M1 jest globalny** — bumpowany przy każdej aplikacji edycji gdziekolwiek na mapie —
  więc w mieście w trakcie budowy zmienia się niemal co klatkę i sam z siebie nie inwaliduje niczego
  sensownie. Własnego licznika per blok **nie zastępuje i nie wolno go wyrzucić** (ostrzeżenie M1).
- **Degradacja:** przy przekroczeniu budżetu blok bez kafla rysowany jest chunkiem LOD 8× z M1.
  Wolniej, ale poprawnie — nigdy nie rysujemy dziury.


---

## Zmiany wpisane po M11a

Zgodnie z `K-18`. To są rzeczy, o których wiadomo **na pewno** po zamknięciu M11a;
podfaza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.
Pełna tabela `E-n` jest w `M11a-format-i-snapshot.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| F-1 ★ | **`PaletteTable` nie jest tablicą wariantów, a wierzchołek modelu niesie `role` obok `slot`** (`E-7`). Barwę wybiera shader funkcją `pick(wariant, rola)` z zestawu palety dzielnicy | Tablica per wygląd nie mieści się w capie: 24 576 mieszkańców ma rzędu dwudziestu tysięcy różnych wyglądów wobec 4096 wpisów. Dla animacji znaczy to jedno: klip **nie ma prawa zmieniać barwy**, bo barwa nie jest stanem instancji, tylko funkcją wariantu — a wariant jest niezmienny |
| F-2 ★ | **Pozycje wierzchołków są wypieczone w pozie spoczynkowej**, z wliczonym łańcuchem pivotów. `ModelMesh::part_rest_qv` niesie te same przesunięcia po stronie procesora, a `ModelVertex.part` numer części | M11a rysuje model jednym wywołaniem, bez tablicy transformacji na GPU — i to jest poprawny obraz **w spoczynku**. Pozy dokładając, trzeba przesunięcie części **odjąć** przed obrotem wokół pivota, a nie liczyć od nowa: `part_rest_qv` jest po to, żeby nie było drugiego miejsca, które je wyprowadza |
| F-3 | **`GpuInstance` ma 36 B, a nie 32** (`E-8`): doszło `pick`, a `extra` zmieniło się w `variant`. Pole `anim` to `clip \| phase << 8 \| aux << 16 \| flags << 24` | `aux` jest tym, co §5.3 nazywało `extra`: `carry` u mieszkańca, `wheel_phase` u pojazdu. `ClipId` ma **jeden bajt**, nie dwa — 256 klipów, czyli sufit z ryzyka `R4`, i to jest jego jedyne miejsce |
| F-4 ★ | **L2 nie jest wybierany i `LodBands` ma dwa progi zamiast trzech** (`E-12`). Powyżej 60 m encje rysują się L1 aż do 600 m | Siatka L2 jest z założenia pusta, bo poziom impostora rysuje `ImpostorAtlas` — czyli WP4 tej podfazy. Do tego czasu wybieranie L2 znaczyłoby, że encja **znika** zamiast zmaleć. `LodBands` jest strukturą, więc trzeci próg jest dopisaniem pola, a nie przebudową ścieżki |
| F-5 | **Kierunek marszu mieszkańca nie istnieje: `CitizenRenderRec.yaw` jest zerem** | Rekord warstwy Mikro niesie pozycję, nie kurs. Wyprowadzenie kursu z różnicy pozycji między publikacjami dałoby obrót zależny od **częstotliwości publikacji**, czyli od klatki — a to jest dokładnie ta klasa sprzężenia, której zabrania 00 §4. Kurs ma policzyć warstwa ruchu albo klip lokomocji, i to jest wejście WP3 |
| F-6 | **Wszystkie pojazdy jadą jedną bryłą `car`.** `VehicleRenderRec.model` niesie `VehicleClassId` z `data/vehicles/classes.ron` i czeka na tablicę | `ModelTable` sortuje już teraz po `ModelId`, więc dołożenie modeli nie rusza ścieżki klatki — dokłada wiersze. Sufit jest nazwany w kodzie (`instancing::ModelTable`) |
| F-7 | **Ścieżka klatki bierze 322 µs przy 26 tys. encji** (`m11a-1` w `benches/baseline.json`) wobec budżetu 1,5 ms z §5.3 | Zapas jest czterokrotny, więc animacja i klasyfikacja trzech poziomów mają w czym rosnąć. Sortowanie jest porównaniami, nie radixem — wymiana na sortowanie kubełkowe jest w zapasie i należy do M11e, jeśli pomiar ją uzasadni |
