# M11 — Prezentacja

Status: plan fazy. Dokument nadrzędny: `00-konwencje-i-kontrakty.md` (sekcje 1, 3, 4, 6).
Źródło: `PRD_Magnat.md` §15 (całość), §14.2, §16.1, §16.3, §17.1, §17.4, §19, §20.2.

Właściciel crate'ów: **`engine/audio`** (nowy) i **`sim-snapshot`** (nowy — rozstrzygnięcie koordynatora
do decyzji 9.5; konsumenci: M1 i M9).
Rozszerzane: `engine/render`, `engine/voxel` (właściciel: M1 — **rozszerzamy, nie projektujemy od nowa**).

> **~~Uwaga do dok. 00 §1~~ — załatwione.** `sim-snapshot` jest w macierzy własności
> dokumentu nadrzędnego jako crate tworzony przez M11. Sprawdzone przy starcie M11a.

Dwie zasady wiążące, którym podporządkowany jest cały ten plan:

1. **LOD wizualne nie zmienia wyniku ekonomicznego** (dok. 00 §4). Żaden próg odległości,
   budżet draw calli ani tryb kamery nie może wpłynąć na stan symulacji.
2. **Render czyta podwójnie buforowany snapshot i go nie mutuje** (dok. 00 §4, PRD §17.1).
   Egzekwowane typami (`&Snapshot`), grafem zależności cargo i testem hashy.

### Rozstrzygnięcia koordynacyjne wiążące tę fazę (dok. 00 §4a)

| # | Co z tego wynika dla M11 |
|---|---|
| **K-1** | Kalendarz 360 dni (12 × 30). `WeatherState.season`, krzywe dobowe `AmbientZone.day_curve` i wybór palety sezonowej liczą się z `SimCalendar` (M0), **nie z własnej arytmetyki dat**. Cztery pory roku = 4 × 90 dni, granice sezonu są dokładne, bez lat przestępnych |
| **K-3** | Od M1 właścicielem urządzenia GPU jest `engine/render` (`init_gpu()` przeniesione z `tools/voxelview`). M11 **nie inicjalizuje `wgpu`** — dostaje gotowe urządzenie i kolejkę |
| **K-4** | Blok `StreamId` fazy M11 to **300–319**. Nadajemy `Appearance = 300`, `Interior = 301`. Rezerwa 302–319. Nie „na końcu enuma" — wartości są przypisane z bloku i niezmienne (zmiana numeru strumienia zmienia każdy świat wygenerowany wcześniej z tego samego seeda) |
| **K-6** | Zakaz `f64::ln`/`exp`/`powf`/`atan2` dotyczy kodu symulacji; **„dozwolone bez ograniczeń w renderze, UI i audio"**. M11 używa zwykłej matematyki zmiennoprzecinkowej i `libm` swobodnie — w interpolacji, atenuacji dźwięku, krzywych oświetlenia i shaderach. Jedyny wyjątek: `fill_render_snapshot` biegnie po stronie sim i podlega `det_math` |
| **K-8** | Wspólne słowniki domenowe mieszkają w `engine/core`. `ClipKind` **nie jest drugim, równoległym słownikiem czynności** — jest projekcją `core::ActivityKind` na animację (5.4). M11 nie wprowadza własnego wokabularza tego, co agent robi |
| **K-13** | `TerrainQuery` jest jedynym źródłem prawdy o terenie — **ale żyje w `sim/world` i do renderu nie wchodzi wcale** (M1 potwierdził; inaczej złamałby test `dep_isolation`). Render dostaje teren dwiema drogami: jako geometrię chunków oraz jako heightmapę w snapshocie. **M11 nie odpytuje `TerrainQuery` i nie liczy niczego o terenie samodzielnie** — jeśli czegoś brakuje, wnosi to `fill_render_snapshot` po stronie sim |
| **K-12** | `DecisionReason` — M11 **nie dopisuje wariantów**. Render i audio nie podejmują decyzji domenowych; wizualizują cudze. Karta inspekcji należy do M3/M9 |

K-5, K-7, K-9, K-10, K-11, K-14 nie dotyczą tej fazy.

---

## 1. Cel fazy i artefakt końcowy

Po zakończeniu M11 da się uruchomić grę i **zobaczyć żywe miasto zamiast diagramu**:

- Zjazd kamerą z orbity nad miastem do poziomu ulicy bez przeskoków wizualnych — dzielnice
  na horyzoncie jako impostory, budynki w średnim planie jako agregowane chunki, ulica pod
  kamerą w pełnym detalu 0,25 m.
- Mieszkańcy chodzą chodnikami w ubraniach odpowiadających zawodowi i zamożności gospodarstwa
  domowego, wsiadają do samochodów o kolorze należącym do konkretnego GD, niosą zakupy,
  pracują animacją właściwą dla stanowiska.
- Nad sklepem gracza wisi szyld z nazwą jego firmy; ciężarówka z jego barwami podjeżdża pod
  rampę, wózek widłowy zdejmuje paletę.
- Klawisz cięcia poziomami zdejmuje dach — widać regały z towarem (wypełnienie odpowiada
  stanowi magazynu), linię produkcyjną i biura.
- `F` przełącza na widok pierwszoosobowy postaci gracza: spacer po własnym sklepie.
- Zapada zmierzch, zapalają się okna i latarnie; awaria sieci gasi dzielnicę w 1,5 s.
  Pada deszcz, z komina rafinerii idzie dym proporcjonalny do emisji, zimą leży śnieg.
- Słychać to: łoże dźwiękowe dzielnicy (przemysł / ruch / park), maszyny w zakładzie gracza —
  a kiedy linia stoi, zapada cisza. Muzyka reaguje na stan finansów gracza.
- `cargo bench -p engine-render` na trzech scenach referencyjnych raportuje p50/p95/p99 klatki
  i potwierdza cele z §20.2: **60 FPS widok dzielnicy, 30 FPS widok miasta**.

**Artefakt weryfikowalny:** `bench/frames/report.json` z zielonymi progami + test CI
`render_does_not_touch_sim` pokazujący, że ten sam seed daje identyczny ciąg hashy niezależnie
od tego, czy render działa, jaką trajektorią leci kamera i na jakim LOD stoi.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Co konkretnie |
|---|---|
| Styl (§15.1) | Format modelu voxelowego 0,25 m dla postaci/pojazdów/propów/szyldów, system slotów palety i wariantów, palety per dzielnica × epoka, atlas szyldów firm gracza |
| Kamera (§15.2) | Tryb pierwszoosobowy postaci gracza, cięcie budynków poziomami (`CutPlane`) + generator wnętrz `InteriorKit` |
| Światło i pogoda (§15.3) | Lista świateł punktowych (okna, latarnie, reflektory) zasilana ze snapshotu, blackout, deszcz/śnieg/mgła jako cząstki i shader, pory roku przez paletę materiału, dym z kominów |
| Animacja (§15.4) | `AnimationClip`/`AnimationState`, `PoseAtlas`, klipy pieszych/pracy/pojazdów/wózków/dźwigów, poziomy detalu modeli |
| Audio (§15.5) | Cały crate `engine/audio`: mikser, `SoundEmitter`, `AmbientZone`, klastrowanie głosów, `MusicDirector` |
| Wydajność (§20.2, §16.3) | Instancing encji dynamicznych, `ImpostorAtlas` dzielnic i encji, progi LOD wizualnego, `RenderBudget` z adaptacją, benchmarki klatkowe |
| Kontrakt danych | Rozszerzenie `RenderSnapshot` o encje dynamiczne, pogodę, zasilanie, aktywność zakładów |

### Nie wchodzi

| Obszar | Faza |
|---|---|
| Pipeline voxelowy chunków 32³, paleta chunka, greedy meshing, voxel AO, streaming, LOD 2×/4×/8× terenu, cienie kaskadowe, `RenderGraph`, kamera orbitalna, cykl dobowy słońca | **M1** — rozszerzamy istniejące, nie budujemy od nowa |
| Nakładki danych (mapy cieplne, animowane strumienie, filtry „pokaż tylko klientów mojego sklepu") | **M2** (geometria nakładek) / **M9** (UI sterujące) — konsumujemy, nie projektujemy |
| UI gry, panele biznesowe, widgety, wykresy, font atlas | **M9** / `engine/ui` — konsumujemy font atlas do szyldów |
| Profilowanie pamięci, budżet 6 GB, tryb 50×, zapis w tle, lokalizacja | **M12** — dostarczamy mu `RenderStats` jako wejście |
| Gramatyka budynków, rozkład parcel, sieć dróg | **M2** — konsumujemy jako wejście do `InteriorKit` |
| Jakakolwiek mechanika symulacji (decyzje agentów, ceny, produkcja, ruch) | **M3–M10** — wyłącznie odczyt ze snapshotu |
| `WeatherState` i `PowerState` jako model symulacyjny (co powoduje blackout, jak działa pogoda) | **M8** — konsumujemy wynik, wizualizujemy |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Pokrycie w tej fazie |
|---|---|
| §15.1 Styl | WP1 (format modelu, sloty palety), WP11 (palety dzielnica × epoka), WP9 (szyldy) |
| §15.2 Kamera | WP5 (FPP, `CutPlane`, `InteriorKit`) |
| §15.3 Oświetlenie i pogoda | WP6 (światła nocne, blackout), WP7 (deszcz, mgła, pory roku, dym) |
| §15.4 Animacja | WP3 (`AnimationClip`, `PoseAtlas`, klipy), WP4 (poziomy detalu) |
| §15.5 Dźwięk | WP8 (`engine/audio`) |
| §14.2 Warstwy widoku | WP2 (kanał `tint` w instancji — filtry i podświetlenia z M2/M9 działają na encjach dynamicznych) |
| §16.1 Fundamenty | `wgpu`/WGSL, `kira` **lub** `cpal` dla audio (decyzja 9.6), `glam`, `rayon`/jobs |
| §16.3 Renderer voxelowy | WP2 (instancing z wariantem i paletą), WP4 (impostory), WP6 (clustered lights — lista, nie pass) |
| §17.1 Czas | WP2 (kontrakt snapshotu, interpolacja renderu między tickami) |
| §17.4 LOD symulacji | WP2, WP4 — LOD wizualne **niezależne** od LOD symulacji; mikro/mezo/makro nie determinuje detalu wizualnego, tylko dostępność danych |
| §19 M11 | Cały zakres: „detale voxelowe, animacje, wnętrza, pogoda wizualna, audio, LOD wizualne, wydajność do celów" |
| §20.2 Metryki techniczne | WP10 (benchmarki, `RenderBudget`), cele 60/30 FPS |

---

## 4. Pakiety robocze i podfazy

Kolejność topologiczna. `→` = zależność twarda.

Faza jest rozbita na **5 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M11a — Format i kontrakt snapshotu** | WP1, WP11, WP2 | 5.1, 5.2, 5.3 | Jeden model postaci w trzech wariantach palety rysowany jednym draw callem; tłum z M3 rysowany z bufora instancji. | `M11a-format-i-snapshot.md` |
| **M11b — Animacja i LOD wizualne** | WP3, WP4a | 5.4, 5.5, 5.6 | Postać chodzi, siada i pracuje; dalszy plan schodzi na impostory bez widocznego przeskoku. | `M11b-animacja-i-lod.md` |
| **M11c — Wnętrza i kamera FPP** | WP12, R2-WP15, R2-WP17, R2-WP18, WP5, WP9 | 5.7 | Wejście do własnego sklepu z ulicy, na której są ludzie i samochody; szyld firmy gracza widoczny z zewnątrz. | `M11c-wnetrza-i-kamera.md` |
| **M11d — Światło, pogoda, dźwięk** | WP6, WP7, WP8 | 5.8, 5.9 | Noc, deszcz, dym z komina i warstwa dźwiękowa reagująca na stan świata, nie na skrypt. | `M11d-swiatlo-pogoda-dzwiek.md` |
| **M11e — Budżet klatki** | WP10, WP4b (`W-1`) | 5.10, 5.11 | Pełny artefakt fazy z §1 dokumentu fazy: cele FPS z PRD §20.2 dotrzymane na maszynie referencyjnej. | `M11e-budzet-klatki.md` |

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | Format modelu voxelowego — `.mvox` | `M11a-format-i-snapshot.md` |
| 5.2 | Kontrakt z symulacją — dokładnie co i ile | `M11a-format-i-snapshot.md` |
| 5.3 | Instancing — bufor GPU | `M11a-format-i-snapshot.md` |
| 5.4 | Animacja | `M11b-animacja-i-lod.md` |
| 5.5 | Progi LOD wizualnego — liczby | `M11b-animacja-i-lod.md` |
| 5.6 | `ImpostorAtlas` | `M11b-animacja-i-lod.md` |
| 5.7 | Kamera, cięcie poziomami, wnętrza | `M11c-wnetrza-i-kamera.md` |
| 5.8 | Oświetlenie, pogoda, dym | `M11d-swiatlo-pogoda-dzwiek.md` |
| 5.9 | `engine/audio` | `M11d-swiatlo-pogoda-dzwiek.md` |
| 5.10 | `RenderBudget` i adaptacja | `M11e-budzet-klatki.md` |
| 5.11 | Systemy i ich częstotliwość | `M11e-budzet-klatki.md` |

---

## 6. Kontrakty międzyfazowe

### 6.1 Dostarczam

| Typ / funkcja | Crate | Dla kogo |
|---|---|---|
| `RenderSnapshot`, `CitizenRenderRec`, `VehicleRenderRec`, `SiteRenderRec`, `CrowdDensityRec`, `WeatherState`, `PowerRec`, `PlayerViewRec`, `SnapshotCaps` | `sim-snapshot` | M3, M4, M6, M7, M8, M9 — **wypełniają**; M12 — mierzy |
| `ViewQuery`, `SnapshotCaps`, `Candidate`, `select_top_k`, `Aabb` | `sim-snapshot` | M3, M4 — wypełniają swoje sekcje |
| `SnapshotFiller::fill(&World, &ViewQuery, &mut RenderSnapshot)` — **adres poprawiony w M11a (`E-2`)**: wypełnianie mieszka w `magnat_game::view`, bo `sim-snapshot` nie może zależeć od trzech crate'ów `sim/*` naraz (§6.3 pkt 2) | `game/` | M11c, M11d — dokładają metodę na sekcję |
| `VoxModel`, `PaletteSlot`, `SlotRole`, `PartName`, `ModelId`, `ModelLibrary`, `build_model_mesh` | `engine/voxel::model` | M2 (propy), M9 (podgląd w UI), M12 (modding) |
| `PaletteLibrary`, `DistrictPaletteId`, `RampRef`, `pick(wariant, rola)` — **`PaletteTable` i `VariantKey` przeprojektowane w M11a (`E-7`)**: zestawy barw zamiast tablicy wariantów, a klucz wariantu jedzie w instancji dwoma polami, więc nie ma czego kluczować. **Osią jest epoka startowa gry, nie pierścień wieku zabudowy** (`E-17`) | `engine/voxel::palette` | M10 (epoki), M12 (modding) |
| `AnimationClip`, `AnimationState`, `ClipKind`, `ClipId`, `PoseAtlas` | `engine/voxel::anim` | M12 (modding — własne klipy) |
| `InstanceRenderer`, `GpuInstance` (36 B, `E-8`), `build_instances`, `ModelTable`, `LodBands`, `PickHit`/`PickKind` | `engine/render::instancing` | M2 (strumienie nakładek mogą użyć tej samej ścieżki), M9 (karta encji spod kursora) |
| `ImpostorAtlas`, `ImpostorTile` | `engine/render::impostor` | M12 (budżet VRAM) |
| `CutPlane`, `CutMode`, cap pass przekroju | `engine/render::interiors` | M9 (UI sterujące cięciem). **`CameraMode` należy do M1, nie do M11** |
| `InteriorKit`, `PropPlacement`, `generate_interior(...)` | `engine/render::interiors` | M9 (panel zakładu — ten sam podgląd) |
| `SignAtlas`, `SignId` | `engine/render::signs` | M9 (nazwa firmy → szyld), M10 (marka) |
| `RenderBudget`, `RenderStats` | `engine/render::budget` | **M12 — wejście do profilowania i trybu 50×**, devtools |
| `AudioEngine`, `SoundEmitter`, `AmbientZone`, `Bus`, `MusicDirector`, `MusicMood`, `SoundSourceId` | `engine/audio` | M8 (zdarzenia → dźwięk), M9 (UI), M12 (modding) |
| `data/palettes/` (schemat RON, `schema_version` per plik) | `data/` | M10 (epoki), M12 (modding) |
| `data/models/*.mvox` + `tools/mvoxc` | `tools/` | M12 (modding) |
| `data/audio/` (katalog źródeł dźwięku, łoża ambientu, stemy muzyki) | `data/` | M8 (dźwięk zdarzeń), M12 (modding) |
| Harness `bench/frames` + baseline | `engine/render` | M12 (testy długich sesji) |

### 6.2 Konsumuję

| Od | Co konkretnie |
|---|---|
| **M0** | `Entity`, `SimInstant`, `Tick`, `DistrictId`; **`SimCalendar`** (K-1 — sezon, pora dnia, granice miesiąca); **`core::ActivityKind`** jako źródło czynności odwzorowywanej na `ClipKind` (K-8); `StreamId` — **blok 300–319 (K-4)**, nadajemy `Appearance = 300`, `Interior = 301`; job system; `engine/devtools` (panel `RenderStats`) |
| **M1** | **Potwierdzone przez M1 (`M1-swiat-statyczny.md` §5.8 i §6.1 „Dla M11", decyzja D9 zamknięta):** `RenderPass`/`PassDecl`/`PassOrder`/`GraphSlot` — punkt rejestracji passa **buduje M1 w WP-R1**, nie ja; `CameraState` jako zasób ramki (macierze + frustum + `clip_plane_z`), **wraz z `CameraMode::{Orbit, Free, FirstPerson{anchor, eye_height_m}}`**; pass `cluster_assign` (256 świateł **na klaster**, 3456 klastrów, **≤ 4096 globalnie**); pipeline voxelowy chunków, greedy meshing, voxel AO, streaming, LOD 2×/4×/8×, cienie kaskadowe, słońce i cykl dobowy, depth prepass; mechanizm podwójnego buforowania, synchronizacja, interpolacja, `FrameCtx`; **`ClusterOccupancy`** — histogram świateł na klaster, maksimum i liczba klastrów powyżej progu, w czasie rzeczywistym (kontrakt w §6.1 M1, nie przysługa; ich WP-R4 zamyka się dopiero po pomiarze na 4 096 świateł testowych, czyli na mojej skali); pass `far_terrain` (clipmap > 4 km, **bez zabudowy — nie zastąpi moich impostorów w paśmie 2–4 km**); `init_gpu()` i urządzenie (K-3); `TerrainQuery` (K-13, żyje w `sim/world`, do renderu nie wchodzi); **geometria chunka: 32×32 m w poziomie × 16 m w pionie, voxel 1 × 1 × 0,5 m** |
| **M2** | **Uzgodnione i potwierdzone przez M2 (blok „Kontrakt dla M11" w `M2-miasto-statyczne.md` §6):** `Building.floor_heights_dm: SmallVec<[u16; 8]>` + `Unit.floor: i8` — **wysokości poszczególnych kondygnacji, nie sama ich liczba** (parter usługowy jest wyższy od piętra mieszkalnego, więc `CutPlane::Level(n)` musi ciąć po realnym stropie); `BuildingGrammar` czytelna wprost jako wejście `generate_interior(...)`; `RoadNetwork.furniture: Vec<StreetFurniture{seg, t, pos, kind}>` z `FurnitureKind::StreetLamp` — pozycje latarni do oświetlenia nocnego; `CsrGrid<BuildingId>::query_rect` (2D) + `Building.aabb: Aabb3` do selekcji kadru; geometria i shadery nakładek danych (nie projektujemy) |
| **M3** | `ClipKind` per agent (ze zdarzenia DES), `JobRoleId` → `outfit_class`, zamożność GD → `outfit_tier`, `carry`, pozycja i `yaw` |
| **M4** | Pozycje pojazdów, `yaw`/`pitch`, `VehicleModelId`, stan (jedzie/parkuje/tankuje/rozładunek), obłożenie, `HouseholdId` właściciela → `paint` |
| **M6 / M7** | **Skale potwierdzone przez M6 (`M6-lancuch-dostaw.md` §6.4.3) — patrz tabela w 5.2:** `activity` (normalizacja do własnej wydajności nominalnej, `Setup` = 0,4), `emission` (skala absolutna, `EMISSION_REF_PM_G_PER_MIN` w `data/tuning/supply.ron`), `stock_fill` (`max` z wolumenu i masy, bez `WarehouseRole::Shelf`), `flags.SITE_FAULT`, `flags.smoke_kind`; `FirmId` → `livery`, nazwa firmy → `sign_id` |
| **M8** | `WeatherState` (opad, sezon, mgła, pokrywa śnieżna), `PowerRec.supply_ratio` per dzielnica (blackout) |
| **M9** | Postać gracza (`PlayerViewRec.citizen`) i wejście do FPP jako komenda do sim; font atlas z `engine/ui` do `SignAtlas`; stan finansów → `liquidity_ratio`, `profit_trend`; UI sterujące `CutPlane`, trybem kamery, głośnością magistral |
| **M10** | Bieżąca epoka → wybór palety (`data/palettes/`) i zestawu modeli pojazdów; marka firmy → kolory szyldu i oklejenia |

### 6.3 Kontrakty egzekwowane mechanicznie

1. `engine/render` i `engine/audio` **nie mają w `Cargo.toml` zależności od żadnego `sim/*`**
   poza `sim-snapshot`. Test CI: `cargo tree -p engine-render -p engine-audio` nie zawiera
   `sim-agents|sim-economy|sim-firms|sim-traffic|sim-supply|sim-city|sim-macro`.
2. `sim-snapshot` zależy wyłącznie od `engine-core` i zawiera tylko typy `#[repr(C)]` POD
   bez metod mutujących świat.
3. Każda funkcja wejściowa renderu i audio przyjmuje `&RenderSnapshot`, nigdy `&mut World`.

To są trzy zdania, ale to one, a nie dobre intencje, gwarantują zasadę z dok. 00 §4.

---

## 7. Testy i kryteria akceptacji

### 7.1 Nienaruszalność symulacji (najważniejsze testy fazy)

| Test | Metoda | Kryterium |
|---|---|---|
| `render_off_equals_render_on` | Ten sam seed, 20 000 ticków. Przebieg A: `tools/headless`. Przebieg B: pełna gra z renderem offscreen (lavapipe). Hash stanu ECS co 1000 ticków. | **Identyczny ciąg 20 hashy.** Tolerancja zero. |
| `camera_path_does_not_matter` | Ten sam seed, 3 skrypty kamery: nieruchoma nad pustkowiem / przelot przez całe miasto / FPP w sklepie gracza. | Identyczne hashe. Testuje wprost dok. 00 §4 — LOD wizualne nie zmienia wyniku. |
| `lod_scale_does_not_matter` | Ten sam seed, `lod_scale` wymuszony na 0,5 / 0,75 / 1,0 oraz cap snapshotu 4 096 / 24 576. | Identyczne hashe. Cap jest wizualny, nie ekonomiczny. |
| `audio_off_equals_audio_on` | Jak wyżej, z `--no-audio`. | Identyczne hashe. |
| `no_sim_deps_in_render` | `cargo tree` (6.3) | Brak zależności. Test kompilacji, nie runtime'u. |
| `snapshot_fill_is_pure` | Typ: `fn(&World, &ViewQuery, &mut RenderSnapshot)`. Dodatkowo test mutacyjny: hash `World` przed i po 10 000 wywołań. | Hash bez zmian. |
| `snapshot_size_is_constant` | Miasto 40 tys. i 400 tys., ten sam kadr. | `size_of_val(snapshot)` identyczny; ≤ 1,35 MB. |
| `snapshot_selection_is_deterministic` | Ten sam tick, ten sam `ViewQuery`, 100 wywołań. | Identyczna lista `entity_lo` w identycznej kolejności (remisy po `entity_index`). |

### 7.2 Benchmarki klatkowe

**Sceny referencyjne** — zamrożone zapisy gry w repo (`bench/scenes/*.mgsave`), seed `0x4D41474E4154`,
miasto 150 tys., dzień 400:

| Scena | Kamera | Warunki | Cel p95 |
|---|---|---|---|
| `bench_street` | wys. 1,7 m, FPP, ul. Handlowa | 08:15, pogodnie, szczyt pieszy | ≤ 16,6 ms |
| `bench_district` | wys. 180 m, pochylenie 35°, Śródmieście | 08:15, ok. 20 tys. widocznych encji | **≤ 16,6 ms (60 FPS)** |
| `bench_city` | wys. 1 400 m, całe miasto | 12:00 | **≤ 33,3 ms (30 FPS)** |
| `bench_night_rain` | jak `district` | 23:00, deszcz, wszystkie latarnie | ≤ 16,6 ms |
| `bench_blackout` | jak `night_rain` | blackout 3 dzielnic | ≤ `night_rain` |
| `bench_interiors` | `CutPlane::Level(2)` | centrum handlowe + fabryka gracza | ≤ 16,6 ms |
| `bench_winter` | jak `district` | śnieg, `season = 3` | ≤ 16,6 ms **i** `chunk_remesh_count == 0` |

**Metoda:** 120 klatek rozgrzewki, 600 klatek pomiaru, GPU timestamp queries. Raport p50/p95/p99
+ pełny `RenderStats`. Zapis `bench/frames/<scena>.json`.

**Sprzęt referencyjny** („GPU średniej klasy 2024", §20.2): RTX 4060 / RX 7600 / Arc A750, 1080p,
sterowniki przypięte w opisie baseline'u.

**W CI (brak GPU):** uruchamiamy wyłącznie (a) benchmarki CPU-side — kompakcja instancji, radix sort,
`generate_interior` — z progami criterion, oraz (b) testy poprawności offscreen na lavapipe
**bez progów czasowych**. Progi klatkowe weryfikuje nocny bieg na dedykowanym runnerze z GPU.
Regresja p95 > 8% względem baseline'u = fail.

### 7.3 Poprawność wizualna

| Test | Kryterium |
|---|---|
| `lod_transition_is_subtle` | Przelot 2 km → 0 m; różnica obrazu między klatkami przy przekroczeniu każdego progu ≤ 1 px ekwiwalentu |
| `variant_survives_lod` | Ta sama encja na L0/L1/L2 ma ten sam kolor ubrania/lakieru (porównanie pikseli centralnych) |
| `cut_plane_has_no_holes` | Przekrój na 12 typach budynków — brak fragmentów o `alpha == 0` wewnątrz sylwetki (cap pass działa) |
| `interior_reflects_stock` | `stock_fill` 255 → 51: liczba propów skrzynek maleje proporcjonalnie ±10% |
| `blackout_ramps` | Wygaszenie trwa 1,5 s ±0,1 s; nie skokowo |
| `seasons_do_not_remesh` | Przejście przez 4 pory roku: `chunk_remesh_count == 0` |
| `impostor_budget` | `bench_city`: `impostor_resident ≤ 244`, `impostor_regen ≤ 4` w każdej klatce |
| `no_missing_geometry` | Przy sztucznie obniżonym budżecie impostorów: zero dziur (degradacja do chunków 8×) |

### 7.4 Audio

| Test | Kryterium |
|---|---|
| `stopped_line_is_silent` | `activity == 0` → sumaryczny gain emiterów zakładu == 0,0 (dokładnie) |
| `voice_clustering` | 500 emiterów `SoundSourceId::Truck` w promieniu 15 m → ≤ 1 głos |
| `voice_budget` | W żadnej klatce `voices_active > 32` |
| `no_voice_clicks` | Wywłaszczenie głosu zawsze przez rampę ≥ 120 ms |
| `music_hysteresis` | `liquidity_ratio` oscylujący ±10% wokół progu → zero zmian `MusicMood` w 60 s |
| `music_on_bar_boundary` | Każde przejście stemów na granicy taktu; brak przejść wewnątrz frazy |
| `audio_reads_only_snapshot` | Sygnatura `&RenderSnapshot`; test mutacyjny hasha jak 7.1 |

### 7.5 Definition of Done fazy

1. Wszystkie testy z 7.1 zielone — **bez wyjątku i bez tolerancji**.
2. `bench_district` ≤ 16,6 ms p95 i `bench_city` ≤ 33,3 ms p95 na sprzęcie referencyjnym (§20.2).
3. Snapshot ≤ 1,35 MB i niezależny od wielkości miasta.
4. Wszystkie nowe komponenty powstałe po stronie sim na potrzeby renderu (np. `paint`, `livery`,
   `outfit_class`) dopisane do funkcji haszującej stanu (dok. 00 §3.6).
5. `clippy -D warnings`, `#![forbid(unsafe_code)]` w `engine/audio`; `unsafe` w `engine/voxel`
   (bufory instancji) z uzasadnieniem i testem pod Miri (dok. 00 §6).
6. `RenderStats` widoczny w `engine/devtools` — M12 startuje z gotowym pomiarem.

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Skutek | Mitygacja |
|---|---|---|---|
| R1 | **Cap snapshotu 24 576 wygląda źle** w gęstej dzielnicy — widać, gdzie kończą się „prawdziwi" ludzie | Złamana iluzja żywego miasta | Warstwa L3 (tłum agregatowy) musi być dopracowana wizualnie, nie tylko tania. Test `lod_transition_is_subtle` obejmuje granicę L2/L3. Cap podnoszalny opcją dla mocnego sprzętu — **nigdy automatycznie**, bo to zmienia pasmo |
| R2 | **Impostory dzielnic powodują zacięcia** przy regeneracji po wyburzeniu kwartału | Widoczne szarpnięcia | Twardy limit 4 bloki/klatkę + budżet 1,5 ms + kolejka po odległości. Degradacja do chunków 8× zamiast czekania |
| R3 | **Cięcie poziomami odsłania puste pudełka** — wnętrz nie ma, bo nikt ich nie modelował | Funkcja sztandarowa §15.2 wygląda na niedokończoną | `InteriorKit` jest generatorem proceduralnym od pierwszego dnia WP5, nie „później dorobimy modele". Kryterium ukończenia WP5 wymaga wypełnienia zależnego od `stock_fill` |
| R4 | **Liczba klipów animacji rośnie z liczbą zawodów** (§15.4: „animacje wg zawodu") | Przekroczenie 256 `ClipId`, koszt produkcji assetów | `JobRoleId → ClipKind::Work(...)` jest **wiele-do-jednego**: kasjer, recepcjonista i urzędnik dzielą klip „praca przy ladzie". Docelowo 18–24 klipów pracy, nie 120 |
| R5 | **Budżet 32 głosów to mało** dla żywego miasta | Dźwięk brzmi ubogo mimo wizualnego bogactwa | Ciężar niosą `AmbientZone` (łoża), nie pojedyncze emitery. Klastrowanie 15 m. Jeśli odsłuch pokaże niedosyt — podniesienie do 48 jest zmianą jednej stałej |
| R6 | ~~Zależność od `RenderGraph` M1~~ | — | **ZDJĘTE.** M1 potwierdził, że buduje punkt rejestracji (`RenderPass`/`PassDecl`/`GraphSlot`) u siebie w WP-R1. Mój WP2 nie dotyka cudzego modułu. W zamian nowe, drobne ryzyko R11 |
| R7 | **Brak GPU w CI** | Regresje wydajności wykrywane późno | Rozdzielenie: CI weryfikuje poprawność (lavapipe) i benchmarki CPU-side; nocny bieg na runnerze z GPU weryfikuje progi klatkowe |
| R8 | **`activity`/`emission` nie mają zdefiniowanej skali** — M6/M7 mogą je liczyć inaczej, niż zakłada wizualizacja | Dym z komina lub głośność zakładu nie odpowiadają rzeczywistości | Decyzja 9.4. Do czasu rozstrzygnięcia kontrakt mówi: „0 = całkowity bezruch, 255 = pełne obłożenie nominalne", walidowane testem na scenie referencyjnej |
| R9 | **Animacja stop-motion 12 fps** może wyglądać tanio zamiast stylowo | Estetyka całej gry | Prototyp wizualny w WP3 przed wypaleniem pełnego zestawu klipów; `fps` jest polem `AnimationClip`, więc podniesienie do 24 dla wybranych klipów nie wymaga zmiany architektury (koszt: 2× `PoseAtlas`, wciąż < 1 MB) |
| R11 | **Rozjazd konwencji współrzędnych z M1** — M1 trzyma origin kamery w `f64` i przekazuje pozycje względem kamery; gdybym rzutował na `f32` przed odjęciem originu, encje zaczęłyby drgać przy współrzędnych rzędu kilku km | Pieszy skacze o pół metra między klatkami na krawędziach mapy — objaw wygląda na błąd symulacji, a jest błędem renderu, więc szuka się go w złym miejscu | Kolejność „odejmij, potem rzutuj" zapisana w 5.3 jako wymóg. Test `no_jitter_at_map_edge` w WP2: encja stojąca w miejscu przy współrzędnej 8 km ma zerową wariancję pozycji ekranowej przez 600 klatek |
| R10 | **Wyciek mutacji do sim przez `ViewQuery`** — ktoś w przyszłości „tymczasowo" doda pole zwrotne | Złamanie kontraktu nadrzędnego | Testy 7.1 (`snapshot_fill_is_pure`, `no_sim_deps_in_render`) są w CI jako blokujące. `ViewQuery` jest `Copy` i nie zawiera referencji |

---

## 9. Decyzje otwarte

| # | Decyzja | Z kim | Założenie robocze | Status |
|---|---|---|---|---|
| 9.1 | **Granica `engine/render` i `engine/voxel` między M1 a M11.** Trzy pytania: (a) czy `RenderGraph` M1 ma jawny punkt rejestracji passa z deklaracją zasobów in/out, (b) czy kamera eksponuje `CameraState` (macierze + frustum) jako osobny zasób, do którego M11 dopisze `FirstPerson` i `CutPlane`, (c) czy struktura snapshotu M1 jest rozszerzalna o nowe tablice SoA bez wersjonowania | **M1** | Tak / tak / rozszerzalna. M11 dokłada wyłącznie nowe moduły (`instancing`, `impostor`, `interiors`, `weather`, `budget`, `voxel::model`, `voxel::anim`) i nie modyfikuje pipeline'u chunków | **ZAMKNIĘTE — M1 odpowiedział „tak/tak/tak" z trzema korektami** (decyzja D9 M1 zamknięta). (a) `RenderGraph` dostaje punkt rejestracji i **buduje go M1 w WP-R1**, nie ja. (b) `CameraState` jest zasobem ramki, **ale `CameraMode::FirstPerson` należy do M1** — nie dopisuję drugiego wariantu, tylko wiążę `anchor` z postacią gracza. (c) 256 świateł to limit **na klaster**, globalnie 4 096 — mój próg latarni podniesiony ze 150 m na 400 m. Dodatkowo: konwencja `f64` origin i pozycje względem kamery (5.3) |
| 9.2 | ~~Cap snapshotu 24 576 mieszkańców / 8 192 pojazdów~~ | M2, M3, M4 | — | **ZAMKNIĘTE w M11a wg propozycji domyślnej.** Cap przyjęty i jest **stałą konfiguracji, nie zmienną runtime'u** (patrz 9.11). Zapytania po AABB do `engine/spatial` **nie ma i nie było potrzebne**: warstwa Mikro trzyma własne okno wokół kamery i zwraca wyłącznie encje z niego, więc selekcja idzie po kilku tysiącach kandydatów, a nie po mieście. Zmierzone: cała ścieżka procesora 322 µs przy 26 tys. encji (`m11a-1`) wobec budżetu 1,5 ms |
| 9.3 | ~~Kto wylicza `anim_phase`~~ | M3 | — | **ZAMKNIĘTE w M11a wg propozycji domyślnej:** liczy ją wypełniacz snapshotu, funkcją czystą od `(chwila, indeks encji)`, i **w ECS nie ma ani bajta stanu animacji**. Alternatywa różniłaby się wyłącznie adresem, bo rekord i tak ma bajt wyrównania. Przy okazji ta sama zasada objęła `appearance` (`E-4`, `K-76` pkt 4) |
| 9.4 | ~~Skala `activity`, `emission`, `stock_fill`~~ | M6, M7 | — | **ZAMKNIĘTE przez M6** (§6.4.3 ich dokumentu). Wzory w tabeli 5.2. WP7 i WP8 odblokowane. M6 dołożył `SITE_FAULT` (awaria ≠ bezczynność) i `smoke_kind`; oba zmieściłem w bajcie, który był wyrównaniem — `SiteRenderRec` nadal 24 B |
| 9.5 | ~~Czy powstaje crate `sim-snapshot`~~ | M0, M1 | — | **ZAMKNIĘTE przez koordynatora: tak, powstaje, a właścicielem jest M11** (nie M1, jak pierwotnie zakładałem). Konsumenci: M1 (podwójne buforowanie) i M9. Uzasadnienie przyjęte w całości: bez osobnego crate'a zasady „render nie mutuje symulacji" nie da się egzekwować grafem zależności, a reguła pilnowana samą dyscypliną zostanie złamana przy pierwszym pośpiechu. **Konsekwencja: dok. 00 §1 wymaga dopisania `sim-snapshot` do crate'ów tworzonych przez M11** |
| 9.6 | **`kira` czy `cpal`** dla `engine/audio` (PRD §16.1 dopuszcza oba). `kira` daje gotowy mikser, magistrale i crossfade — mniej kodu, ale własny model czasu. `cpal` to goły strumień — pełna kontrola, więcej pracy | — (decyzja M11) | `kira`: mikser, magistrale i przejścia muzyczne to dokładnie to, czego potrzebujemy, a nie mamy powodu ich pisać. Ambient przestrzenny i klastrowanie budujemy sami nad nim | Otwarte — **do zamknięcia prototypem w WP8, przed napisaniem miksera** |
| 9.7 | **Czy postać gracza w FPP jest zwykłym agentem ECS.** Plan zakłada, że tak, i że wejście gracza idzie przez komendy do sim. Jeśli M9 zaprojektuje postać gracza jako byt poza ECS, tryb FPP potrzebuje osobnej ścieżki pozycji | M9 | Zwykły agent; render tylko odczytuje `PlayerViewRec.eye` | Otwarte |
| 9.8 | ~~Trzy nowe katalogi `data/`~~ | dok. 00 | — | **ZAMKNIĘTE.** `data/palettes/`, `data/models/` i `data/audio/` są w §5 dokumentu nadrzędnego. M11a dołożyła im **regułę kolejności** (`K-76` pkt 2): osie `districts` i `epochs` w `palettes.ron` są kontraktem zapisu gry, bo indeks pary jedzie w buforze instancji; `ModelId` jest za to pozycją posortowanego klucza, więc w zapisie trzyma się klucz. `data/audio/` czeka na M11d |
| 9.9 | ~~Nowe warianty `StreamId`~~ | M0 | — | **ZAMKNIĘTE przez K-4.** Blok M11 to 300–319; nadajemy `Appearance = 300`, `Interior = 301`, rezerwa 302–319. Wartości niezmienne |
| 9.10 | **Sprzęt referencyjny dla §20.2.** „GPU średniej klasy 2024" wymaga konkretnego modelu, inaczej progi benchmarków są nieweryfikowalne | M12 | RTX 4060 / RX 7600 / Arc A750 @ 1080p | Otwarte — **M12 jest właścicielem profilowania, powinien to przypiąć** |
| 9.11 | ~~Czy `RenderBudget` może obniżać cap snapshotu~~ | M1, M12 | — | **ZAMKNIĘTE przez koordynatora: nie — i to jest reguła, nie rekomendacja.** Obniżanie capu snapshotu byłoby sprzężeniem render → symulacja, czyli dokładnie tym, czemu zapobiega `sim-snapshot`. Cap jest stałą konfiguracji, nie zmienną runtime'u |

---

## 10. Szacunek wielkości

| WP | Nazwa | Rozmiar | Uzasadnienie |
|---|---|---|---|
| WP1 | Format modelu voxelowego i sloty palety | **M** | Format + importer + walidator + 3 poziomy detalu. Nowy kod, ale dobrze ograniczony |
| WP11 | Palety per dzielnica × epoka | **S** | Dane + walidator + rozwiązywanie wariantu. Głównie `data/` |
| WP2 | Kontrakt snapshotu i instancing | **L** | Rdzeń fazy. Dotyka M1 (bufor), M2 (spatial), M3, M4. Ścieżka kompakcji musi mieścić się w 1,5 ms |
| WP3 | Animacja | **L** | `PoseAtlas`, shader z lookupem póz, ok. 24 klipy pracy + 12 lokomocji. Duża część to produkcja assetów |
| WP4 | LOD wizualne i `ImpostorAtlas` | **L** | Dwa systemy impostorów (build-time i runtime), LRU, regeneracja amortyzowana, degradacja |
| WP5 | Kamera FPP, `CutPlane`, wnętrza | **L** | `InteriorKit` to generator proceduralny z gramatyki — sam w sobie jest M/L |
| WP6 | Oświetlenie nocne i blackout | **M** | Wypełnianie listy świateł + reguła 150 m + rampa. Pass należy do M1 |
| WP7 | Pogoda wizualna i dym | **M** | Cząstki GPU + shadery mgły; sezony przez paletę są tanie z założenia |
| WP8 | `engine/audio` | **L** | Cały nowy crate: mikser, emitery przestrzenne, okluzja, klastrowanie, `MusicDirector` |
| WP9 | Szyldy i barwy firm gracza | **S** | Atlas tekstu + jeden slot palety. Zależy od font atlasu M9 |
| WP10 | `RenderBudget`, adaptacja, benchmarki | **M** | Timestamp queries + histereza + harness i baseline |
| WP12 | Ulica ma ruch: okno Mikro, promień rysowania, trasa pieszego | **M** | Trzy naprawy jednego objawu — pustego kadru (`X-1`). Dopisany po M11b |

**Rozkład:** 5 × L, 4 × M, 2 × S. Ciężar leży w WP2 (kontrakt danych), WP3 (animacja),
WP4 (LOD), WP5 (wnętrza) i WP8 (audio).

**Ścieżka krytyczna:** WP1 → WP2 → {WP3, WP4} → WP10.
WP8 można prowadzić równolegle od zamknięcia WP2 — potrzebuje wyłącznie `SiteRenderRec`
i `PlayerViewRec`, nie czeka na nic z grafiki.

**Największa niepewność:** WP2 zależy od rozstrzygnięcia decyzji 9.1 (granica z M1) i 9.5
(crate `sim-snapshot`). Bez nich WP2 nie powinien startować — inaczej grozi przepisanie
kontraktu danych po fakcie, a on jest fundamentem wszystkich pozostałych pakietów.

---

## Zmiany wpisane po M3d

Zgodnie z `K-18`. To są rzeczy, o których M11 wie **na pewno** po zamknięciu M3d;
M11 nie jest tu przeprojektowywany.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **`magnat-sim-snapshot` ma już jeden rekord encji: `PedestrianRecord`** (`pos: [f32;3]`, `entity: u32`). M11 dokłada wygląd, klip i pozę — **obok**, nie zamiast | Schemat ładunku należy do M11 i to się nie zmienia, ale M3 musiał narysować pieszego i odpowiedzieć, w kogo gracz kliknął. Rekord jest świadomie minimalny: ani `Appearance`, ani `AnimationClip` w nim nie ma. `f32` wystarcza — przy 16 km krok `f32` to ~1 mm, a bryła pieszego ma 0,5 m |
| Z-2 ★ | **Piesi nie idą przez `RenderSnapshot`, tylko slice'em do `Renderer::set_pedestrians`** — tak samo jak światła | `RenderSnapshot` jest `Copy` i bezalokacyjny (§ nagłówek crate'u), a pieszych bywa kilkaset tysięcy. `CappedSlice` na taką liczbę nie mieści się na stosie. Wzorzec „stan w snapshocie, tłum slice'em" jest już w kodzie dla `LightRecord` i M11 ma go zastać, a nie wymyślać |
| Z-3 ★ | **`engine/render` ma warstwę `egui` (`ui.rs`) i rysuje ją po post-processingu**, prosto na bufor ekranu. M11 stylizuje, nie wpina | Decyzja 9.2 M3 wybrała `egui` + `egui-wgpu`, a K-3 mówi, że urządzeniem GPU zarządza `engine/render`. Panel **nie może** iść przez HDR: kolory UI są w sRGB i mają takie wyjść, a FXAA na tekście wygląda jak wada sterownika |
| Z-4 | **Jest bufor ID (`pick.rs`) i `Renderer::pick(x, y)`** — osobny przebieg po passie nieprzezroczystym, z porównaniem głębi `Equal`, plus kopia jednego piksela spod kursora na klatkę | M11 rysuje pieszych z animacją i **musi trafić w tę samą geometrię**, bo bufor ID porównuje głębię przez `Equal`. Praktycznie znaczy to, że pass ID ma dostać ten sam punkt wejścia wierzchołka co pass sceny — tak jak dziś, gdzie oba stoją na jednym `vs_main` w `pedestrian.wgsl`, czego pilnuje test |
| Z-5 | **`engine/render` ma test walidacji WGSL bez GPU** (`tests/shaders.rs`, `naga`) | Shader jest sprawdzany dopiero przy tworzeniu urządzenia, czyli przy oknie, którego w CI nie ma. Każdy shader dokładany przez M11 ma trafić do listy w tym teście — lista jest jawna celowo, żeby nie rósł o pliki, których nikt nie kompiluje |
| Z-6 | **MSRV workspace'u to 1.95** (podniesione z 1.90 przez `egui 0.36`) | `egui-wgpu 0.36` jest jedyną wersją stojącą na `wgpu 30`; `0.33` wymaga `wgpu ^27`, czyli drugiego `wgpu` w drzewie zależności |

---

## Zmiany wpisane po M4

Zgodnie z `K-18`. To są rzeczy, o których wiemy **na pewno** po zamknięciu M4;
faza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| T-1 ★ | **`sim/snapshot` ma `VehicleRecord`** (`pos`, `heading`, `entity`, `class`, `lane`) obok `PedestrianRecord`, a `MicroLayer::vehicle_snapshot` go wypełnia. M11 **rozszerza ten rekord**, a nie zakłada drugiego kanału | Wykonanie `R-2`: `sim/snapshot` ma pozostać jedynym kanałem sim → render, którego pilnuje graf zależności. `heading` jest liczony po stronie ruchu, bo kierunek wynika z osi krawędzi, a tej renderer nie zna; `class` to indeks `data/vehicles/classes.ron` — dobór bryły i animacji należy do M11 |
| T-2 ★ | **Warstwa Mikro jest krokowana raz na minutę świata i całkuje IDM wewnątrz tego wywołania** (podkrok 0,5 s, limit 120, `data/roads/idm.ron`). Płynność sterowana **klatką** jest nadal do zrobienia i należy do M11b | To jest jawny sufit, nie przeoczenie: przy jednym kroku na minutę renderer widzi pozycje z końca minuty, więc pojazd i pieszy przeskakują raz na minutę świata niezależnie od FPS. M11b ma dać warstwie **własną kadencję** — `micro_step(now_cs)` przyjmuje dowolną chwilę i jest do tego gotowe, bufor domyka się do czasu zaksięgowanego przy każdej. Nie zmieniaj przy tym liczby wywołań **systemu** minutowego: jej pilnuje bramka WP14 z M4c |
| T-3 | **Mikro nie ma prawa zapisu do stanu symulacji i jest to egzekwowane deklaracją dostępu** (`micro_writes_nothing`), a `VehicleState` nie wchodzi do hasha | M11 dokłada animacje i wnętrza do **tej samej** warstwy. Dopisanie tam czegokolwiek, co zmienia stan ekonomiczny, wywali `camera_does_not_change_world` — i o to chodzi. Jeśli animacja potrzebuje czasu innego niż zaksięgowany, ma go **odegrać**, a nie wyznaczyć |
| T-4 | **Pasażer nie ma własnego rekordu w zrzucie** — jedzie w bryle kursu. Gracz nie zobaczy, ile osób siedzi w autobusie, dopóki M11 nie doda wnętrz | Sufit nazwany komentarzem `ponytail:` w `sim/traffic::micro`. Ścieżka wyjścia: pole obłożenia w `VehicleRecord` albo osobna tablica pasażerów, obie po stronie `sim/snapshot` |
| T-5 | **Nakładki ruchu są danymi, nie shaderem**: pięć wpisów w `data/ui/overlays.ron` (`traffic_flow`, `congestion`, `isochrone`, `parking_occupancy`, `transit_load`) plus podwójnie buforowany `TrafficOverlay` w `sim/traffic` | Klient i podgląd `headless m3day --overlay` czytają **tę samą** tabelę i tę samą funkcję rastrującą, więc nakładkę da się sprawdzić w CI bez GPU. M11 dokłada styl i legendę na ekranie, a nie drugi zestaw progów. Reguła palety obowiązuje każdą nową nakładkę i pilnuje jej test `kazda_nakladka_ma_palete_monotoniczna_w_jasnosci` |
| T-6 | **Zrzut 3 tys. pojazdów kosztuje 4,3 µs, 5 tys. pieszych 5,0 µs** (`m4d-2 zrzut` w `benches/baseline.json`) | Budżet §7.3 M4 dawał na to 0,8 ms/klatkę; zapas jest trzyrzędowy, więc M11 może w tym rekordzie **rosnąć**. Wąskim gardłem jest rysowanie, nie kopia |

---

## Zmiany wpisane po M5e

Zgodnie z `K-18`. Jedna rzecz, ale twarda.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Bufor identyfikatorów w `engine/render` niesie wyłącznie pieszych i to zaczyna blokować fazy.** M5e otwiera panel sklepu **raycastem w teren** i szuka zakładu w promieniu 25 m od punktu trafienia (`Citizens::select_shop`), bo `Renderer::pick()` nie ma czym zwrócić budynku ani zakładu. M9 potrzebuje tego samego dla każdego klikalnego obiektu paneli biznesowych | Dwa sklepy bliżej siebie niż 25 m są dziś nierozróżnialne kliknięciem — wygrywa bliższy, i to jest sufit nazwany w kodzie klienta. To nie jest problem M5 ani M9: **bufor identyfikatorów należy do warstwy rysującej**, czyli do M11 (właściciel `engine/render` po M1). Zapisane teraz, bo to jest ta klasa braku, którą inaczej odkrywa się jako „dlaczego kliknięcie w wieżowiec otwiera sklep spożywczy z parteru sąsiedniego budynku" — w tygodniu domknięcia cudzej fazy. Zakres: `BuildingId`, `SiteId` i **`VehicleId`** w buforze obok `CitizenId`, plus warianty w `magnat_ui::Selection` |

## Zmiany wpisane po decyzji właściciela produktu (2026-09-17)

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. Wymaganie produktowe brzmi:
**każdy obiekt widoczny w świecie jest klikalny i otwiera swoją kartę**. Przegląd planu pokazał,
że jedno ogniwo tego łańcucha nie ma właściciela, i jest nim pojazd.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Pojazd wchodzi do bufora identyfikatorów** — dopisany do zakresu wpisu „po M5e" wyżej, który wymieniał tylko `BuildingId` i `SiteId`. Pass `pick_id` rysuje pojazdy tą samą geometrią co pass sceny, tak jak robi to już dla pieszych (Z-4) | Rekord pojazdu w `M11a` §„Rekord pojazdu" **już niesie `entity_lo` z komentarzem „picking"**, a `M9c` §5.7 ma `Subject::Vehicle` i `FollowTarget::Vehicle` od początku. Brakowało wyłącznie zdania, które zobowiązuje warstwę rysującą do wpisania tego id do bufora — czyli łańcuch był cały poza jednym ogniwem, i to takim, którego nikt by nie szukał, bo z obu stron wygląda na gotowy. Karta pojazdu ma pokazać właściciela i stan (`VehicleOwner`, `VehicleCondition`, `VehicleLocation` istnieją w `sim/traffic` od M4) |
| | **Pass `pick_id` musi wypełniać bufor także przy wyłączonym LOD mikro dla pojazdów** — albo jawnie powiedzieć w `M11e`, że przy `X50` pojazdów nie ma i kliknięcie w nie nie istnieje | `M12c` wyłącza warstwę Mikro w trybie 50× z konstrukcji — pojazdów wtedy nie ma w ogóle, więc „klikalne jest wszystko, co widoczne" nadal się trzyma, ale tylko jeśli nie rysujemy pojazdu, który nie ma id. To ma być sprawdzone testem, nie założone |


## Zmiany wpisane po M9e

Zgodnie z `K-18`. Szczegóły — tabela `DI-n` w `M9e-panele-czas-kariera.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| DL-1 ★ | **Widok 3D dzieli ekran z dokiem lewym (320 px) i z kartą inspekcji.** Dok jest `egui::Panel::left` o stałej szerokości, więc kamera dostaje **węższy prostokąt**, a nie całe okno | `docs/ui-design.md` §5 wymaga, żeby widok świata był widoczny zawsze; panel pełnoekranowy istnieje wyłącznie dla kroniki i edytora reguł. Budżet klatki M11 liczy się od tej chwili dla kadru, który jest o 320 px węższy — i to jest liczba na korzyść, byle jej nie przeoczyć przy pomiarach |
| DL-2 ★ | **Tryb „śledź" ma cel i przypięcie, a nie ma kamery.** `game::timectl::Follow` trzyma `FollowTarget` i woła `player::pin_micro`; przesuwanie kamery za celem należy do klienta i do M11 | Przypięcie do warstwy Mikro jest **mechanizmem symulacji** (`DG-8`) i musiało powstać razem z warunkiem, który go używa. Kamera jest prezentacją i M11 jest jej właścicielem — dwie kamery rozjechałyby się przy pierwszym trybie orbitalnym |
| DL-3 | **Karta partii istnieje i pokazuje pełny ślad „od pola do półki"** (`game::inspect::batch`, `CardTabKind::History`). Wyrobisko i pojazd z §14.4 mają dokąd prowadzić | Decyzja otwarta nr 3 dokumentu M9 zamknięta: M6 dał pełny łańcuch, degradacji do „ostatnich trzech etapów" nie było |
| DL-4 | **`GanttView` i `GraphView` są w `engine/ui` i rysują się bez GPU** — tak samo jak reszta widgetów. M11 nie dokłada im renderera, tylko ewentualnie budżet | Oba powstały razem z pierwszym panelem, który ich zażądał (`DF-2`) |

## Zmiany wpisane po M11a

Zgodnie z `K-18`. Pełna tabela `E-n` z uzasadnieniami jest w `M11a-format-i-snapshot.md`;
tutaj wyłącznie to, co dotyczy **całej fazy**. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| U-1 ★ | **Kryterium `snapshot_size_is_constant` (§7.1) mierzy rozmiar *rezydentny*, nie `size_of_val`.** Snapshot waży 1,31 MB na bufor, 2,62 MB przy podwójnym buforowaniu; sufit 1,35 MB z §7.5 pkt 3 trzyma się | `size_of_val` przestało być właściwą miarą razem z `Copy` (`E-1`): ładunek leży na stercie w buforach o stałej pojemności, więc `size_of_val` mierzy nagłówek. Mierzona jest suma pojemności — liczba, która z definicji nie zależy od wielkości miasta, bo nie zależy nawet od zapełnienia |
| U-2 ★ | **`Z-2` (po M3d) przestaje obowiązywać.** Mieszkaniec idzie przez `RenderSnapshot` jako `CitizenRenderRec`; `PedestrianRecord` zostaje jako rekord **ruchu** | Przesłanką `Z-2` było `Copy` i stos. Odpadła razem z nim (`K-76` pkt 3). Kanał sim → render jest przez to **jeden**, tak jak obiecuje §6.3, a nie jeden plus slice obok |
| U-3 | **`§7.5` pkt 4 Definition of Done jest spełniony zbiorem pustym.** M11a nie wnosi ani jednego komponentu po stronie sim — wygląd, lakier i oklejenie są funkcjami czystymi (`E-4`) | Punkt mówił „nowe komponenty powstałe po stronie sim na potrzeby renderu dopisane do funkcji haszującej". Najtańszy sposób spełnienia go to nie tworzyć takich komponentów, a nie pamiętać o ich dopisaniu. Reguła jest wiążąca dla podfaz następnych i siedzi w `K-76` pkt 4 |
| U-4 | **Ryzyko `R11` (rozjazd konwencji współrzędnych) zamknięte, ale jego skala była zawyżona o dwa rzędy** (`E-11`): realny błąd złej kolejności to ok. 2 mm przy 16 km, a nie 0,5 m przy 8 km | Reguła „odejmij w `f64`, potem rzutuj" zostaje bez zmian — kosztuje jedno odejmowanie. Zmienia się wyłącznie liczba w opisie i test, który ją mierzy (`encja_przy_krawedzi_mapy_nie_drga`) |
| U-5 | **Ryzyko `R1` (cap 24 576 wygląda źle) jest nadal otwarte i nie ma dowodu w żadną stronę** | Warstwa L3 (tłum agregatowy) należy do M11b, a `crowd` w snapshocie jest po M11a wyzerowane. Do czasu jej powstania granica L2/L3 nie istnieje, więc nie ma czego oglądać — i to jest jedyna uczciwa odpowiedź, jaką M11a może dać |

## Zmiany wpisane po M11b

Zgodnie z `K-18`. Pełna tabela `G-n` z uzasadnieniami jest w `M11b-animacja-i-lod.md`;
tutaj wyłącznie to, co dotyczy **całej fazy**. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| W-1 ★ | **WP4 dzieli się na `WP4a` (impostory encji, zamknięte w M11b) i `WP4b` (impostory dzielnic), a `WP4b` przechodzi do M11e** | Impostor encji i impostor dzielnicy dzielą nazwę i nic poza nią: pierwszy wypala się z modelu przy starcie i sprawdza testem bez GPU, drugi powstaje w locie z tego, co gracz zbudował, i potrzebuje passa renderującego blok miasta do tekstury, kolejki regeneracji z budżetem czasu i LRU na 192 MB — czyli budżetu klatki, który jest własnością M11e. Kryterium „`bench_city` mieści się w budżecie VRAM" nie ma jak zapalić się na czerwono, dopóki sceny `bench_city` nie ma, a ta powstaje w M11e §7.2 (`G-10`) |
| W-2 ★ | **Pasma L3 z §5.5 nie ma.** Encja ma trzy poziomy (`L0`, `L1`, impostor) i odległość odcięcia; progi są osobne dla mieszkańca i pojazdu | Impostor rysowany z czterystu metrów zajmuje te 4×6 px sam z siebie, więc osobny poziom kosztowałby drugi potok po to, żeby narysować to samo (`G-9`) |
| W-3 ★ | **Faza animacji liczy się z czasu animacji, nie z ticku symulacji.** `ViewQuery` dostaje `anim_ms` — zegar prezentacji, który stoi przy pauzie i nie zależy od prędkości gry | Decyzja 9.3 mówi „funkcja czysta od chwili i indeksu encji" i zostaje w mocy; chwilą nie może być minuta gry, bo przy ×1 trwa sekundę realną (`G-5`) |
| W-4 ★ | **Klient dostaje scenę pomiarową `--crowd N`, `--crowd-step M` i `--no-anim`**, na wzór `--lights` z M1 | Kryteria M11b mówią o dwudziestu tysiącach postaci, a warstwa Mikro w oknie 900 m oddaje ich kilkadziesiąt. Bez sceny kryterium jest niemierzalne, a nie spełnione (`G-12`). Ta sama scena posłuży M11e do `bench_street` i `bench_district` |
| W-5 | **`FrameStats` niesie `instances` i `instance_batches`** — pierwsze dwa pola `RenderStats` z §5.10 | Przy pustym kadrze „symulacja nic nie oddała", „render tego nie narysował" i „kamera patrzy gdzie indziej" dają ten sam obraz. M12 dostaje te liczby przy okazji (`G-14`) |
| W-6 | **`§7.3` dostaje test `variant_survives_lod` w postaci wykonalnej bez GPU**: billboard ma wymiary bryły modelu, a barwę liczy ta sama funkcja `pick(wariant, rola)`, co pełna geometria | Porównanie pikseli centralnych z §7.3 wymaga karty graficznej i sceny referencyjnej; równość **wejść** obu ścieżek sprawdza się w CI i wyklucza tę samą klasę błędu (`G-8`, `G-11`) |
| W-7 ★ | **Poprawka spoza fazy: mieszkańcy zaczynali i kończyli trasę pod terenem** (`PlaceEntry.at` brał `aabb.min.z`, czyli dno fundamentu). Po poprawce biorą rzędną wejścia | To jest ta klasa usterki, którą widać dopiero wtedy, gdy encja przestaje być plamką: do M11b pieszy miał kilka pikseli i nikt nie liczył, na jakiej jest wysokości (`G-13`). Środek trasy zostaje otwarty i ma adres w R2 |

## Zmiany wpisane po decyzji właściciela produktu (2026-09-19, po M11b)

Zgodnie z `K-18`. Pełna tabela `J-n` stoi w `M11c-wnetrza-i-kamera.md`; tutaj to,
co dotyczy całej fazy. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| X-1 ★ | **Faza dostaje `WP12` — „Ulica ma ruch"** — i cztery pakiety wchodzą do M11c przed WP5 i WP9: `WP12`, `R2-WP15`, `R2-WP17`, `R2-WP18` | M11b pokazała, że żywe miasto z §1 („mieszkańcy chodzą chodnikami, wsiadają do samochodów, niosą zakupy") nie ma dziś jak zaistnieć: w kadrze stoi kilkadziesiąt osób, nie jedzie ani jeden samochód, a firm jest dziesięciokrotnie za mało. Artefakt fazy z §1 jest wtedy nieweryfikowalny — nie dlatego, że renderer czegoś nie umie, tylko dlatego, że nie ma czego narysować |
| X-2 ★ | **`DRAW_RADIUS_M` przestaje być stałą mniejszą od dystansu orbity.** Dziś to 600 m mierzone od oka, a domyślny widok gry stoi 900 m od celu — encje w środku kadru są poza promieniem | To jest stan gry od M11a i najpoważniejsza pojedyncza rzecz z całej czwórki: nie „za mało ludzi", tylko **zero ludzi w widoku, od którego gra się zaczyna**. §5.5 dokumentu M11b podaje progi poziomu detalu w metrach i one zostają; zmienia się wyłącznie odległość odcięcia, która ma wynikać z rozmiaru ekranowego encji |
| X-3 ★ | **Okno warstwy Mikro i `ViewQuery.aabb` idą za celem kamery, nie za jej okiem** | Przy orbicie z 900 m oko stoi 767 m w poziomie od celu, więc oba wycinki są przesunięte o tyle samo, a połowa okna leży za plecami kamery. Kadr i wycinek symulacji mają opisywać to samo miejsce |
| X-4 | **Decyzja `D-N13` z R2 przyjęta wg propozycji domyślnej** (profil bez złóż w regionie: zakład nie powstaje, łańcuch domyka import przez bramę) | Blokowała `R2-WP17`, który wchodzi do M11c razem z `R2-WP18`. Pozostałe warianty łamią kontrakt: „przesuń profil" daje świat inny, niż deklaruje wiersz poleceń, „dosiej złoże" łamie `K-13` |
| X-5 | **Wykaz R2 traci pięć pozycji na rzecz M11c** (5, 6, 12, 70, 71) i wszystkie mają tam imiennego adresata | R2 wymaga statusu dla każdej pozycji; „przeniesiona z adresatem" jest statusem, „zrobiona po drodze" nie jest |

## Zmiany wpisane po M11c

Zgodnie z `K-18`. Pełna tabela `J-n` z uzasadnieniami jest w `M11c-wnetrza-i-kamera.md`;
tutaj wyłącznie to, co dotyczy **całej fazy**. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| Y-1 ★ | **Cztery pakiety przejęte z `R2` są zamknięte, piąty (`R2-WP18`) kończy się pomiarem i przechodzi do `R3`** z decyzją otwartą `D-N20` | `D-N6` przewidywał, że naprawa gęstości firm może wymagać przeprojektowania Etapu 7, i tak wyszło: zakład bierze **cały budynek**, a lokali użytkowych jest dwadzieścia razy więcej niż zakładów. Reszta czwórki zamknięta z testami |
| Y-2 ★ | **Widok dzielnicy ma ludzi i auta.** Zmierzone na świecie odniesienia (`--seed 7 --size 4km`, `--dist 900`, 8:00): 858 mieszkańców i 798 pojazdów w snapshocie, 1 221 instancji narysowanych. Przed M11c: **zero** | Cztery przyczyny, wszystkie po stronie prezentacji: okno warstwy Mikro i wycinek snapshotu zaczepione w oku zamiast w celu kamery, próg rysowania mniejszy od dystansu orbity, jednorazowe wejście pieszego do kadru i pojazdy stojące w punkcie `[0, 0, 0]` przez całą minutę (`J-2`, `J-3`, `J-11`) |
| Y-3 ★ | **§7.3 dostaje `cut_plane_has_no_holes` w postaci, którą da się spełnić: czapka jest pierścieniem ściany, a nie płytą na obrysie** (`J-18`), a **prepass głębi musi znać cięcie** (`J-19`) | Płyta domyka sylwetkę i zakrywa wnętrze, czyli to, po co gracz tnie budynek. Prepass bez shadera fragmentu zapisywał głębię dachu, którego pass nieprzezroczysty już nie rysował — i wtedy przekrój był czarną dziurą niezależnie od czapki. Usterka M1, niewidoczna, bo do M11c **nikt nigdy nie ustawił `clip_plane_z`** |
| Y-4 ★ | **`generate_interior` z §6.1 przyjmuje `InteriorSpec`, a nie `BuildingGrammar`** (`J-17`) | Sygnatura z §5.7 łamie §6.3 pkt 1: `BuildingGrammar` mieszka w `sim/world`, a `engine/render` nie ma prawa zależeć od żadnego `sim/*` poza `sim-snapshot`. Ten sam ruch, którym `E-2` przeniosło wypełniacz snapshotu do `magnat_game::view` |
| Y-5 ★ | **Zapis „po M5e" o budynku i zakładzie w buforze identyfikatorów jest domknięty **w połowie**: zakład tak, budynek nie** (`J-20`) | Wierzchołek chunka ma osiem bajtów i nie ma w nich miejsca na `BuildingId`; dołożenie go jest zmianą formatu siatki, czyli własności M1, i kosztuje pamięć wszystkich chunków po to, żeby użył jej jeden pass. Zakład jest klikalny przez szyld i przez wyposażenie wnętrza — czyli przez rzeczy, które i tak są encjami instancjonowanymi |
| Y-6 | **Katalog `data/models/` rośnie z dwóch modeli do siedmiu**: dochodzą `shelf`, `crate`, `desk`, `machine` i `sign`, wszystkie generowane przez `mvoxc gen` | `InteriorKit` bez modeli propów nie ma czego postawić, a szyld bez tablicy nie ma na czym wisieć. Klucze są kontraktem z `PropModels` w `engine/render`: brak pliku nie jest błędem, tylko wnętrzem bez tego mebla |
