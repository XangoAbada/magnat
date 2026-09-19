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

| | WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|---|
| [x] | WP3 | Animacja: `PoseAtlas`, klipy, stany | WP1, WP2 | L |
| [x] | WP4a | LOD wizualne encji i atlas sylwetek | WP2, WP3 | L |
| [ ] | WP4b | Impostory dzielnic (runtime, LRU, 192 MB) | WP4a, M1 | M |

**Podział WP4 na dwa pakiety jest poprawką wpisaną po M11b** (`G-10`). Impostor **encji**
i impostor **dzielnicy** dzielą nazwę i nic poza nią: pierwszy jest wypalany z modelu
przy starcie i nie zależy od świata, drugi powstaje w locie z tego, co gracz zbudował,
i potrzebuje passa renderującego blok miasta do tekstury. WP4b ma adresata: **M11e**,
razem z `RenderBudget` i scenami referencyjnymi — patrz `G-10`.

### WP3 — Animacja

**Opis.** `AnimationClip` z klatkami póz (12 fps, styl stop-motion), wypalone w starcie do
`PoseAtlas` — statycznego bufora GPU. Instancja niesie tylko `(clip_id, phase)`; vertex shader
sam odczytuje macierze części. **Zero uploadu kości na klatkę.** Klipy: chodzenie, bieg,
stanie, siedzenie, wsiadanie/wysiadanie, niesienie zakupów, oraz zestaw `Work(WorkStyle)` —
lada, magazyn, linia, biuro, budowa, kierownica (`G-1`).

**Kryterium ukończenia.** 20 000 animowanych postaci przy < 0,3 ms dodatkowego czasu GPU względem
tych samych postaci w pozie bazowej; test `pose_atlas_fits_budget` (≤ 512 KB).

**Zmierzone (M11b).** 14 030 postaci na poziomie L1 w kadrze, RTX 4070 Ti SUPER, 1600×900:
pass `opaque` **0,39 ms z animacją i 0,39 ms bez niej** — różnica poniżej rozdzielczości
znacznika czasu GPU, czyli poniżej 0,01 ms wobec budżetu 0,3 ms. Scena: `magnat --crowd 20000
--crowd-step 0.55 --dist 60 --speed 0`, drugi przebieg z `--no-anim`. Atlas póz waży **56 KB**
przy dwóch modelach i 24 klipach (budżet 512 KB), a test `atlas_miesci_sie_w_budzecie`
liczy go z prawdziwego katalogu `data/models/`.

### WP4a — LOD wizualne encji i atlas sylwetek

**Opis.** Trzy poziomy dla encji (`G-9`) i atlas impostorów **encji**, wypalany przy starcie
z modelu i klipu chodu. Kafel przechowuje **rolę slotu palety**, nie kolor — dzięki temu
wariant wyglądu działa aż do najdalszego poziomu i tłum w oddali nie robi się szary.

**Kryterium ukończenia.** Przejście progu L1 → L2 nie zmienia ani wielkości, ani barwy encji:
billboard ma wymiary bryły modelu (test `billboard_ma_wielkosc_modelu`), a barwę liczy ten sam
`pick(wariant, rola)`, co pełna geometria. Encja na poziomie impostora jest **klikalna** —
pass `pick_id` rysuje kafle tą samą geometrią co pass sceny.

**Zmierzone (M11b).** Atlas sylwetek: **590 KB** (48 warstw 64×48 `Rgba8Uint`) wobec 13 MB
z rachunku §5.6 — wymiar `outfit_class` odpadł razem z kolorem w kaflu (`G-8`). Tłum 20 000
postaci rozciągnięty przez próg 120 m nie pokazuje w kadrze żadnej linii podziału
(`magnat --crowd 20000 --crowd-step 1.5 --dist 60`). Kliknięcie w encję rysowaną kaflem
zwraca `Citizen` z prawidłowym indeksem (`magnat --pick`).

### WP4b — Impostory dzielnic

**Opis.** Atlas kafli 128×128 m generowany w runtime z tego, co gracz zbudował: 8 azymutów,
LRU, budżet 192 MB, regeneracja ≤ 4 bloki na klatkę, inwalidacja dwustopniowa, degradacja
do chunków LOD 8×. **Przeniesione do M11e** (`G-10`).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 Animacja

> **Wykonane w M11b z korektami.** Struktury niżej są szkicem sprzed implementacji;
> co z nich zostało, a co się zmieniło i dlaczego, mówi tabela `G-n` na końcu tego
> dokumentu (`G-1`…`G-5`). Zasada („sim nie prowadzi animacji, faza jest funkcją czystą,
> zero uploadu kości na klatkę") nie drgnęła.

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

> **Wykonane w M11b z korektą `G-9`:** pasma L3 nie ma, a progi są osobne dla mieszkańca
> i pojazdu. Liczby w tabelach niżej są tymi, które weszły do `LodBands`.

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

> **Impostory encji wykonane w M11b** (`WP4a`) — z korektami `G-8` i `G-11`: kafel niesie
> **rolę slotu**, a nie indeks, wymiaru `outfit_class` nie ma, a kafle czyta `textureLoad`
> z tekstury `Rgba8Uint`. **Impostory dzielnic są w M11e** jako `WP4b` (`G-10`); ten opis
> jest ich projektem technicznym i nie zmienia się razem z adresem.

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

---

## Zmiany wpisane po M11b

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. Pełna lista jest tutaj;
dokument fazy niesie tylko to, co dotyczy całej fazy.

| # | Zmiana | Dlaczego |
|---|---|---|
| G-1 ★ | **`clip_for(activity, work)` bierze `WorkStyle`, nie `JobRoleId`**, a `ClipKind::Work` niesie jedną z sześciu klas: lada, magazyn, linia, biuro, budowa, kierownica | §5.4 zapisywał `Work(JobRoleId)`, a `JobRoleId` ma dziś 46 wartości i rośnie z katalogiem — czyli `ClipKind` miałby więcej wariantów, niż `ClipId` mieści (sufit 256, ryzyko `R4`). Odwzorowanie rola → styl wymaga przy tym **klucza tekstowego roli**, którego `engine/voxel` nie ma i mieć nie może: `data/jobs/roles.ron` czyta `sim/firms`, a zależność idzie od symulacji do silnika, nie odwrotnie. Stoi więc tam, gdzie widać obie strony: `WorkStyle::from_role_key` w `engine/voxel`, a tablicę „rola → styl" buduje raz wypełniacz snapshotu |
| G-2 | **`AnimationClip` nie ma pól `id`, `fps`, `loop_mode`, `pose_offset` ani `parts_mask`.** Zostaje `kind`, `frames` (potęga dwójki) i lista kanałów | `id` nadaje katalog (pozycja w liście), `pose_offset` należy do atlasu, a nie do klipu, `parts_mask` wynika z listy kanałów. `fps` jest **stałą modułu**, bo fazę liczy się raz na encję z jednego zegara — dwa różne `fps` w jednym snapshocie znaczyłyby, że jedna liczba opisuje dwa zegary. `loop_mode` nie ma czego opisywać: ruch sinusoidalny jest zapętlony z definicji, a klip **jednorazowy** potrzebowałby chwili startu zdarzenia, której rekord nie niesie — i to jest nazwany sufit, nie przeoczenie |
| G-3 ★ | **`PoseAtlas` wypala pozy per para (model, klip)**, a nie per klip. Indeks jest płaski: `model * MAX_CLIPS + clip` | Atlas trzyma obrót **złożony po hierarchii** i translację skorygowaną o pozę spoczynkową, a jedno i drugie zależy od pivotów **konkretnego modelu**. Klip wspólny dla wszystkich modeli musiałby albo składać hierarchię w shaderze (pętla po rodzicach na każdym wierzchołku), albo trzymać same kąty lokalne i liczyć resztę w locie. Para bez wspólnej części nie zajmuje ani jednego teksela, więc dwadzieścia klipów postaci nie mnoży się przez katalog pojazdów: atlas obu modeli waży 56 KB |
| G-4 | **Tablica wejść i teksele póz jadą jednym buforem storage**, a `ClipEntry::offset` jest absolutnym indeksem w tym buforze | `downlevel_defaults` daje **cztery** bufory storage na etap shadera, a render potrzebuje palety (barwy i zestawy), póz i wpisów atlasu sylwetek. Piąty wymagałby podniesienia limitu urządzenia, czyli odcięcia kart, które mają dokładnie ten limit — a to jest jedna tablica rozbita na nagłówek i dane |
| G-5 ★ | **Faza klipu liczy się z czasu animacji (`ViewQuery.anim_ms`), a nie z ticku symulacji.** Zegar rośnie czasem realnym, gdy gra idzie, i stoi przy pauzie | Tick to **minuta gry**, a minuta trwa sekundę realną przy prędkości ×1 — klip przesuwałby się o jedną klatkę na sekundę. Czas świata mnożony przez prędkość jest równie zły w drugą stronę: przy ×10 nogi przebierałyby dziesięć razy szybciej, niż da się zobaczyć. Decyzja 9.3 („liczy wypełniacz, funkcją czystą od chwili i indeksu encji") zostaje bez zmian — zmienia się to, **która chwila**. Do symulacji ta liczba nie wchodzi: jedzie do snapshotu wyłącznie jako faza |
| G-6 ★ | **`PedestrianRecord` dostaje `heading: f32`** (16 → 20 B), a `CitizenRenderRec.yaw` przestaje być zerem. Zamyka `F-5` | Warstwa Mikro liczyła kurs pieszego od M3d (`PathArena::at` zwraca kąt odcinka) i **wyrzucała go jedną linią**. Renderer nie ma jak go odtworzyć: różnica pozycji między publikacjami dałaby obrót zależny od częstotliwości klatek, czyli od kamery, czego zabrania 00 §4. Rekord ruchu nie wchodzi do budżetu snapshotu — idzie własnym kanałem |
| G-7 ★ | **Model zastępczy postaci stał bokiem i został obrócony o 90°**: oś barków i rozstaw nóg idą teraz wzdłuż `Y`, a postać patrzy w `+X` — tak samo jak auto | Do M11b `yaw` mieszkańca był zerem, więc orientacji modelu nikt nie widział. Pierwszy obrócony pieszy pokazałby ją natychmiast: wymach nogi w marszu jest obrotem wokół osi poprzecznej, a przy modelu stojącym bokiem szedłby on w bok. Test `postac_patrzy_wzdluz_osi_x` porównuje teraz głębokość z szerokością bryły |
| G-8 ★ | **Impostor encji nie ma wymiaru `outfit_class`**: 8 azymutów × 4 fazy = 32 kafle na model zamiast 1024, a atlas obu modeli waży 590 KB zamiast 13 MB | §5.6 liczył kafle po klasie ubrania, ale kafel **nie niesie koloru** — niesie rolę slotu, a barwę podstawia shader (`E-7`). Klasa ubrania nie zmienia kształtu sylwetki, więc wymiar był mnożnikiem bez treści. Przy okazji kafel ma jeden rozmiar dla wszystkich (64×48 px), bo warstwy `Texture2DArray` muszą go mieć wspólny |
| G-9 ★ | **`LodBands` ma trzy progi per rodzaj encji (`Bands { l1_m, l2_m, draw_m }`), a pasma L3 nie ma** | §5.5 dawał czwarty poziom („plamka cienia + sylwetka 4×6 px"), ale impostor rysowany z czterystu metrów **zajmuje dokładnie tyle pikseli sam z siebie** — osobny poziom kosztowałby drugi potok i drugi zestaw kafli po to, żeby narysować to samo. Zamiast trzeciego progu jest `draw_m`. Mieszkaniec i pojazd mają osobne progi, bo auto jest cztery razy dłuższe i na tej samej odległości zajmuje kilka razy więcej pikseli |
| G-10 ★ | **Impostory dzielnic przeniesione do M11e** jako `WP4b`; WP4 dzieli się na `WP4a` (encje, zamknięte) i `WP4b` | Dwie rzeczy o jednej nazwie i niczym więcej wspólnym. Impostor **encji** wypala się z modelu przy starcie, nie zależy od świata i sprawdza się testem bez GPU. Impostor **dzielnicy** powstaje w locie z tego, co gracz zbudował: potrzebuje passa renderującego blok 128×128 m do tekstury, kolejki regeneracji z budżetem czasu i LRU na 192 MB — czyli **budżetu klatki**, który jest własnością M11e (`RenderBudget`, §5.10). Dochodzi kryterium: „scena `bench_city` mieści się w budżecie VRAM" nie ma jak zapalić się na czerwono, dopóki sceny `bench_city` nie ma, a ta powstaje w M11e §7.2. Zysk jest przy tym **wydajnościowy, a nie wizualny**: chunki M1 sięgają 4 km i dalej rysuje clipmapa, więc bez impostorów dzielnic nie ma dziury w obrazie, jest wyższy koszt |
| G-11 | **Kafle czyta `textureLoad` z tekstury `Rgba8Uint`, a nie sampler** | Kanał `R` niesie **numer roli palety**, a `G` numer ściany — liczby, nie barwy. Filtrowanie liniowe zrobiłoby z roli 2 i 4 rolę 3, czyli cudzy kolor na krawędzi każdej sylwetki. Przy okazji w grupie wiązań nie ma samplera |
| G-12 | **Klient dostaje scenę pomiarową `--crowd N`, `--crowd-step M` i `--no-anim`** — ta sama konwencja co `--lights` | Kryteria WP3 i WP4a mówią o dwudziestu tysiącach postaci i o przejściu progu detalu, a warstwa Mikro w oknie 900 m oddaje ich kilkadziesiąt (przy seedzie odniesienia 24–46). Bez sceny kryterium nie jest ani zielone, ani czerwone — jest niemierzalne. `--no-anim` istnieje z tego samego powodu: „względem pozy bazowej" wymaga zmierzenia tej samej sceny dwa razy. Tłum wchodzi **po** wypełnieniu snapshotu i nie dotyka symulacji |
| G-13 ★ | **Poprawka spoza podfazy: `PlaceEntry.at` brał wysokość z `aabb.min.z`, czyli z dna fundamentu** (`sim/world/src/population/catalog.rs`). Teraz bierze ją z wejścia budynku | Mieszkaniec startował i kończył trasę od 0,8 do 10,1 m **pod gruntem** (`data/grammar/`: głębokość posadowienia plus 2,8 m na kondygnację podziemną), więc pieszy szedł pod terenem i był niewidoczny. Objaw wyszedł dopiero wtedy, gdy pieszy dostał sylwetkę i klip: przedtem był plamką kilku pikseli i nikt nie liczył, na jakiej jest wysokości. `Building.entrances[].pos.z` niesie rzędną posadowienia i jest w modelu od M2. **Zmiana nie dotyka ani jednej liczby wchodzącej do ekonomii** i to jest sprawdzone, nie założone: wszystkie odległości w wyborze miejsca i w routingu liczą się w płaszczyźnie (`walk_minutes` sumuje `|dx|+|dy|`, `distance_sq_xy` i `manhattan_cm` ignorują `z`), więc jedynym konsumentem tej współrzędnej jest polilinia trasy w warstwie Mikro, czyli prezentacja. Testy `sim/world` zielone po zmianie |
| G-15 ★ | **Modele zastępcze są teraz wyśrodkowane na origin w poziomie** (pivot korzenia niesie przesunięcie), a kafel impostora rzutuje bryłę prawą stroną ekranu, nie lewą | Dwie usterki z jednej przyczyny, obie znalezione w recenzji przed commitem. **(1)** `instance.wgsl` obraca model o `yaw` **wokół origin**, a bryła auta leżała w całości po jednej stronie: jadący samochód byłby rysowany dwa metry obok jezdni i zataczałby łuk przy skręcie. Do M11b nikt tego nie widział, bo `yaw` mieszkańca był zerem, a pojazdów w oknie gry nie ma w ogóle (pozycja 71 wykazu R2). **(2)** Ta sama nieśrodkowość ucinała sylwetkę w kaflu: rzut zakładał bryłę wokół zera, więc auto z boku traciło ćwierć kafla. **(3)** Przy okazji prawa strona kafla była liczona jako `(−sin a, cos a)`, a prawa strona ekranu to `patrz × góra = (sin a, −cos a)` — kafel wychodził lustrzany i postać przebierała nogami w drugą stronę, niż szła. Pilnują tego dwa testy: `bryla_jest_wysrodkowana_w_poziomie` (`mvoxc`) i `sylwetka_miesci_sie_w_kaflu` (`engine/voxel`) |
| G-16 | **Wpis atlasu sylwetek niesie `frame_shift`**: o ile bitów przesunąć fazę klipu, żeby trafić w kafel | Kafle wypala się z **co czwartej** klatki chodu, a faza z rekordu tyka w takt klipu — bez przesunięcia tłum na impostorach przebierał nogami cztery razy szybciej niż ten sam tłum metr bliżej, czyli dokładnie przeskok, którego zabrania kryterium WP4a |
| G-17 | **Zegar animacji nie gubi ułamka milisekundy**, a `PoseAtlas::entry` odrzuca klip spoza katalogu, zanim sięgnie do tablicy | Obcięcie `dt` przy 60 klatkach na sekundę zabierało 4 % czasu animacji, a powyżej tysiąca klatek zegar stanąłby zupełnie. `entry()` bez tej straży dla `NO_CLIP` (255) nie wychodziło poza wektor — trafiało w **wiersz innego modelu**, czyli zwróciłoby cudzy klip; shader miał to porównanie od początku, procesor nie |
| G-14 | **`FrameStats` dostaje `instances` i `instance_batches`**, a zrzut wypisuje obok nich stan snapshotu, kamerę i pierwszego pieszego | Przy pustym kadrze trzy różne przyczyny dają ten sam obraz: symulacja nic nie oddała, render tego nie narysował albo kamera patrzy gdzie indziej. Bez tych liczb szuka się ich po kolei i po omacku — tak właśnie wyszło, że piesi chodzili jedenaście metrów pod terenem (`G-13`). `RenderStats` z §5.10 dostaje przy okazji dwa pierwsze pola |

### Co zostaje otwarte po M11b

| Rzecz | Adres |
|---|---|
| Impostory dzielnic (`WP4b`): atlas runtime, LRU, budżet 192 MB, regeneracja ≤ 4 bloki na klatkę | **M11e**, razem z `RenderBudget` i scenami referencyjnymi (`G-10`) |
| **Trasa pieszego jest odcinkiem prostym między środkami budynków** — ignoruje ulice i teren, więc w połowie drogi pieszy bywa pod ziemią albo nad nią | **R2.** `journey.rs::enter_micro_inner` daje warstwie Mikro dwa punkty, a nie polilinię z grafu pieszego; węzły grafu mają poprawne `z_cm` i nikt ich nie czyta. `G-13` naprawił końce trasy, środek zostaje |
| Warstwa Mikro oddaje 24–46 pieszych i **zero pojazdów** w oknie 900 m w szczycie porannym | **R2** (pozycja 71) — przy 26 tys. mieszkańców wygląda to na zaniżone, a zero aut znaczy, że `VehicleRenderRec` nie ma w oknie gry ani jednego czytelnika. Klip pojazdu, lakier i model są gotowe i czekają |
| Koła pojazdu kręcą się ze stałą prędkością klipu, a `wheel_phase` z rekordu nie ma czytelnika | M11e, jeśli pomiar to uzasadni: faza per część wymaga drugiego wejścia do `apply_pose` |
| `carry` jest zerem, a klip `WalkCarry` czeka gotowy — model zastępczy nie ma części `carried` | M11c, razem z propami wnętrz albo z modelem artysty |
| Pojazd zawsze dostaje klip `Drive`; postoju, tankowania i rozładunku rekord nie odróżnia | M11c (parkingi) — klipy są w katalogu |
| `pitch` pojazdu nadal nie wchodzi do obrotu instancji | M11c, razem z rampami i pojazdami na wzniesieniu |
