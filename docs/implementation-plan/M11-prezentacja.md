# M11 — Prezentacja

Status: plan fazy. Dokument nadrzędny: `00-konwencje-i-kontrakty.md` (sekcje 1, 3, 4, 6).
Źródło: `PRD_Magnat.md` §15 (całość), §14.2, §16.1, §16.3, §17.1, §17.4, §19, §20.2.

Właściciel crate'ów: **`engine/audio`** (nowy) i **`sim-snapshot`** (nowy — rozstrzygnięcie koordynatora
do decyzji 9.5; konsumenci: M1 i M9).
Rozszerzane: `engine/render`, `engine/voxel` (właściciel: M1 — **rozszerzamy, nie projektujemy od nowa**).

> **Uwaga do dok. 00 §1:** macierz własności modułów wymienia dla M11 wyłącznie `engine/audio`.
> Doszedł `sim-snapshot` — wymaga dopisania do tabeli w dokumencie nadrzędnym.

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

## 4. Pakiety robocze (WP)

Kolejność topologiczna. `→` = zależność twarda.

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP1 | Format modelu voxelowego i sloty palety | M1 (`engine/voxel`) | M |
| WP11 | Palety per dzielnica × epoka (`data/palettes/`) | WP1 | S |
| WP2 | Kontrakt snapshotu i instancing encji dynamicznych | WP1, M3, M4 | L |
| WP3 | Animacja: `PoseAtlas`, klipy, stany | WP1, WP2 | L |
| WP4 | LOD wizualne i `ImpostorAtlas` | WP2, WP3, M1 | L |
| WP5 | Kamera FPP, `CutPlane`, `InteriorKit` | WP2, M2, M9 | L |
| WP6 | Oświetlenie nocne i blackout | WP2, M1, M8 | M |
| WP7 | Pogoda wizualna i dym | WP2, M8 | M |
| WP8 | `engine/audio` | WP2 (tylko `SiteRenderRec`) | L |
| WP9 | Szyldy i barwy firm gracza | WP1, M9 | S |
| WP10 | `RenderBudget`, adaptacyjne LOD, benchmarki klatkowe | WP4, WP5, WP6, WP7 | M |

WP8 jest niezależny od WP3–WP7 i może iść równolegle od momentu zamknięcia WP2.

---

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

### WP5 — Kamera FPP, cięcie poziomami, wnętrza

**Opis.** `CameraMode::FirstPerson(CitizenId)` śledzący postać gracza wyłącznie przez odczyt
pozycji ze snapshotu. `CutPlane` jako clip w shaderze + pass domykający przekrój (`cap pass`),
żeby przecięta ściana nie była pustą skorupą. `InteriorKit` generuje wyposażenie proceduralnie
z gramatyki budynku (M2) i stanu zakładu ze snapshotu — regały z wypełnieniem wg stanu magazynu,
linie produkcyjne, biurka.

**Kryterium ukończenia.** Cięcie 4-piętrowego centrum handlowego na poziomie 2 przy ≤ 16,6 ms;
regały odzwierciedlają stan magazynu (test: zmiana zapasu o 50% zmienia liczbę widocznych propów);
spacer FPP po sklepie gracza bez artefaktów near-plane.

### WP6 — Oświetlenie nocne i blackout

**Opis.** Wypełnianie listy świateł punktowych dla clustered shadingu M1 z danych snapshotu:
okna budynków (`SiteRenderRec.lights`), latarnie, reflektory pojazdów. Latarnie dalej niż 150 m
przestają być światłami i stają się materiałem emisyjnym + wkładem do ambientu — inaczej
20 tys. latarni zabija cluster. Blackout: `PowerState.supply_ratio` poniżej progu wygasza dzielnicę
rampą 1,5 s (skok wygląda jak błąd renderu, nie jak awaria sieci).

**Kryterium ukończenia.** Scena `bench_night_rain` w budżecie; `bench_blackout` nie jest wolniejszy
niż `bench_night_rain` (czyli ścieżka „brak świateł" nie ma patologii); wizualnie widać granicę
zasilanej i niezasilanej dzielnicy.

### WP7 — Pogoda wizualna i dym

**Opis.** Deszcz/śnieg jako GPU particle w boxie wokół kamery (jeden draw call, wrapping —
nie symulujemy pogody nad całym miastem, tylko tam, gdzie kamera). Mgła wykładnicza wysokościowa
w shaderze. **Pory roku i śnieg na ziemi nie dotykają geometrii voxeli** — modyfikują paletę
materiału w shaderze. To jest świadoma decyzja: remeshing 4× w roku dla całego miasta byłby
absurdalnym kosztem za zmianę koloru. Dym z kominów: emiter cząstek, gęstość z `SiteRenderRec.emission`.

**Kryterium ukończenia.** Przejście przez cztery pory roku bez ani jednego remeshingu chunka
(licznik `chunk_remesh_count` = 0); deszcz przy 200 tys. cząstek ≤ 0,8 ms GPU; dym z 256 kominów
w jednym draw callu.

### WP8 — `engine/audio`

**Opis.** Nowy crate. Mikser czterech magistral (Ambient / World / UI / Music) z limiterem.
`AmbientZone` per dzielnica z łożem dźwiękowym zależnym od typu i pory dnia. `SoundEmitter`
przestrzenny z okluzją z raycasta voxelowego. **Klastrowanie głosów** — bez niego żywe miasto
to 5000 emiterów; z nim to 32 głosy. `MusicDirector` czytający stan finansów gracza.
„Linia stoi = cisza" to wprost `gain = f(SiteRenderRec.activity)`, gdzie `activity == 0 → gain == 0`.

**Kryterium ukończenia.** Testy z sekcji 7.4; odsłuch trzech dzielnic (przemysł, park, śródmieście)
daje rozpoznawalnie różne łoża; zatrzymanie linii produkcyjnej gracza słychać w ≤ 0,5 s.

### WP9 — Szyldy i barwy firm gracza

**Opis.** `SignAtlas` — nazwy firm gracza renderowane przez font atlas z `engine/ui` (M9) do
tekstury R8 (kanał alfa; kolor z palety marki), tile 256×64. Szyld jako prop `.mvox` z jednym
slotem `Sign` mapowanym na tile atlasu. To samo dla `livery` pojazdów — oklejenie firmowe.

**Kryterium ukończenia.** Zmiana nazwy firmy w UI aktualizuje szyld w świecie w ≤ 1 klatce;
100 różnych szyldów w kadrze w jednym draw callu.

### WP10 — `RenderBudget`, adaptacyjne LOD, benchmarki

**Opis.** Pomiar czasu GPU przez timestamp queries, `RenderStats` z podziałem na warstwy,
adaptacyjna skala progów LOD z histerezą. Harness benchmarków na scenach referencyjnych,
zapis do `bench/frames/*.json`, porównanie z baseline w CI.

**Kryterium ukończenia.** Cele §20.2 osiągnięte na sprzęcie referencyjnym; regresja p95 > 8%
zatrzymuje build.

**Dodatkowy pomiar z terminem — selekcja kadru (zobowiązanie wobec M2).** WP10 mierzy osobno koszt
`CsrGrid::query_rect` + odrzucenia po `Building.aabb` w scenach `bench_district` i `bench_city`
(licznik `snapshot_select_ms` w `RenderStats`). M2 świadomie zostawił `GridSpec` w 2D na podstawie
mojego argumentu i poprosił o sygnał, gdyby pomiar pokazał inaczej — **z zastrzeżeniem, że zmiana
`GridSpec` po M4 dotyka czterech crate'ów, więc zgłoszenie ma wartość tylko wcześnie.**
Próg alarmowy: `snapshot_select_ms > 0,3 ms` w `bench_city`. Po przekroczeniu WP10 **natychmiast**
zgłasza to M2, nie czeka na koniec fazy. Jeśli próg nie zostanie przekroczony — zamykamy temat
pisemnie, żeby nikt nie wracał do trzeciego wymiaru bez danych.

---

## 5. Projekt techniczny

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

### 5.7 Kamera, cięcie poziomami, wnętrza

**Korekta po uzgodnieniu z M1: kamera w całości należy do M1, łącznie z widokiem
pierwszoosobowym.** PRD §15.2 jest w zakresie M1, więc `CameraMode::{Orbit, Free,
FirstPerson { anchor: EntityId, eye_height_m: f32 }}` **dostarcza M1** — M11 nie definiuje
drugiego wariantu, tylko **wiąże istniejący `anchor` z encją postaci gracza** (M9; do czasu M9
`anchor` wskazuje encję atrapę). `clip_plane_z` jest uniformem M1 i to M1 klampuje geometrię
chunków. Moja jest wyłącznie **warstwa przekroju**: `CutPlane` (wyliczenie `world_y` ze stropów)
i cap pass domykający ściany.

```rust
pub struct CutPlane {
    pub mode: CutMode,              // Off | Level(u8) | Box(Aabb) | FocusBuilding(u32)
    pub world_y: f32,               // wyprowadzane z floor_heights_dm, nie zgadywane
    pub fade: f32,                  // 0,5 m miękkiej granicy — twarda krawędź wygląda jak błąd
}
```

**`Level(n)` tnie po realnym stropie, nie po równych odstępach.** M2 dostarcza
`Building.floor_heights_dm` — wysokości poszczególnych kondygnacji, bo parter usługowy jest wyższy
niż piętro mieszkalne. `world_y = base + suma(floor_heights_dm[0..n])`. Gdyby ciąć co stałą
wysokość, przekrój przechodziłby przez środek witryny na parterze każdej kamienicy.
`Unit.floor: i8` (ujemne = podziemie) pozwala ciąć również piwnice i magazyny podziemne.

**Pion liczy się w jednostkach 0,5 m, nie w metrach — to najłatwiejszy błąd o czynnik 2 w tej fazie.**
Voxel M1 ma 1 × 1 × **0,5 m** (PRD §15.1 mówi o rozdzielczości 1 m i to prawda w poziomie; §4.2
dokłada 0,5 m w pionie i to §4.2 jest wiążące). Czyli kondygnacja 3 m to **6 voxeli, nie 3**,
a `clip_plane_z` M1 przyjmuje jednostki 0,5 m. `floor_heights_dm` z M2 jest w decymetrach,
więc konwersja to `half_meters = dm / 5` — jedno miejsce w kodzie, obudowane testem
`cut_plane_hits_slab`: dla budynku o kondygnacjach 4,5 m / 3,0 m / 3,0 m cięcie `Level(1)`
i `Level(2)` musi trafić dokładnie w strop, nie 1,5 m obok.

**Implementacja cięcia:** clip w fragment shaderze voxelowym (`discard` powyżej `world_y`)
+ **cap pass** domykający przekrój pełnym kolorem materiału. Bez cap passa przecięta ściana jest
pustą skorupą i widać wnętrze geometrii — wygląda jak dziura, nie jak przekrój architektoniczny.

**`InteriorKit` — wnętrza generowane, nie modelowane:**

```rust
pub struct InteriorKit { pub props: Vec<PropPlacement> }
pub struct PropPlacement { pub model: ModelId, pub pos: [f32; 3], pub yaw: u16, pub fill: u8 }

pub fn generate_interior(
    building: &BuildingGrammar,   // M2 — kondygnacje, siatka, typ użytkowania
    site: &SiteRenderRec,         // M11 — stock_fill, activity
    seed: u64,                    // deterministyczne rozmieszczenie (StreamId::Interior)
) -> InteriorKit;
```

Regały, linie produkcyjne, biurka, palety na rampie to **propy instancjonowane**, nie ręczne modele.
`stock_fill` steruje liczbą widocznych skrzynek na regale — gracz widzi pustkę w magazynie, zanim
otworzy panel. Cache: `InteriorCache` LRU 64 budynki, inwalidacja przy zmianie `stock_fill`
lub `activity` o > 10% (histereza — inaczej regenerujemy co klatkę).

**FPP nie ma własnej fizyki.** Postać gracza porusza się jak każdy inny agent; wejście gracza idzie
do sim jako komenda (M9), render tylko odczytuje `PlayerViewRec.eye` ze snapshotu i interpoluje.
To jest jedyny sposób, żeby tryb FPP nie stał się drugą, rozbieżną ścieżką ruchu.

W FPP: near plane 0,05 m, FOV 70°, model własnej postaci ukryty, LOD wymuszony na L0 w promieniu
15 m, `CutPlane::Box` wokół kamery, żeby ściana za plecami nie zasłaniała.

### 5.8 Oświetlenie, pogoda, dym

**Światła.** M1 jest właścicielem passa clustered shading; M11 **wypełnia listę świateł** ze snapshotu:

| Źródło | Dane | Reguła |
|---|---|---|
| Okna | `SiteRenderRec.lights` | budynek = 1 światło obszarowe, nie N okien; ~1 500 w widoku dzielnicy |
| Latarnie | `RoadNetwork.furniture`, `FurnitureKind::StreetLamp` (M2) | **< 400 m: światło punktowe. ≥ 400 m: materiał emisyjny + wkład do ambientu dzielnicy** |
| Reflektory | `VehicleRenderRec.flags` | tylko L0 i L1, cap 128 |

**Korekta budżetu po uzgodnieniu z M1: 256 to limit NA KLASTER, nie globalny.** Froxele 16×9×24 =
3 456 klastrów, globalnie **≤ 4 096 aktywnych świateł na klatkę** (≤ 0,4 ms GPU na przypisanie).
Pierwotny próg 150 m wyliczyłem przy błędnym założeniu 256 świateł globalnie i był o rząd
wielkości zbyt ostrożny — nocne miasto byłoby ciemniejsze, niż musi.

Przeliczenie na realnej gęstości: latarnie co ~30 m po obu stronach jezdni to ~66 sztuk na kilometr
drogi. Przy budżecie 4 096, z czego ~1 500 zabierają okna i ~128 reflektory, na latarnie zostaje
~2 400 → ok. **36 km widocznej drogi**, co pokrywa widok dzielnicy z zapasem. Stąd **próg 400 m**.
Dokładna wartość jest kalibrowana pomiarem w WP6 na `bench_night_rain` — gęstość latarni zależy od
`lamp_spacing_m[class]` z M2, więc to jest pokrętło do strojenia, nie stała z tabeli.

**Redukcja zostaje po stronie producenta listy (M11), nie w passie M1.** Przepełnienie: `assert`
w debug, w release obcięcie po malejącym dystansie — nigdy losowe, żeby nocna scena nie migotała
przy obrocie kamery.

**Prawdziwym ograniczeniem jest zajętość klastra, nie budżet globalny (ostrzeżenie M1).** Koszt passa
opaque rośnie z liczbą świateł **na klaster**, a nie z sumą. 2 400 latarni rozrzuconych po scenie jest
tanie; te same 2 400 wzdłuż jednej prostej arterii skupia się w kilkunastu klastrach i limit 256
wyczerpuje się tam **wcześniej niż globalne 4 096**. Wniosek: sam próg odległości nie wystarcza,
bo jest ślepy na gęstość.

Dlatego reguła latarni ma drugi stopień: przy przekroczeniu **192 świateł w klastrze** latarnie w tym
klastrze przechodzą na materiał emisyjny **co drugą**, potem co czwartą — przerzedzanie zamiast
obcinania ogona, bo równomiernie rzadszy rząd latarni wzdłuż alei czyta się jak rzadsze latarnie,
a ucięty ogon jak ciemna dziura w połowie ulicy. Objaw do rozpoznania w WP6: skok czasu passa opaque
przy obrocie kamery **wzdłuż** arterii przy niezmienionej liczbie globalnej — to jest zajętość
klastra, nie budżet. **M1 wystawia licznik zajętości klastrów w devtools** i to on, a nie suma świateł,
jest przyrządem do kalibracji progu 400 m na `bench_night_rain`.

**Blackout.** `PowerRec.supply_ratio < 128` → wygaszenie dzielnicy rampą **1,5 s**
(`gain *= smoothstep`). Skokowe zgaszenie czyta się jako błąd renderu; rampa czyta się jako awaria.

**Pogoda:**

| Efekt | Implementacja | Koszt |
|---|---|---|
| Deszcz / śnieg | GPU particle w boxie 60×40×60 m wokół kamery, wrapping, 200 tys. cząstek, 1 draw call | ≤ 0,8 ms |
| Mgła | wykładnicza wysokościowa w shaderze, fullscreen; dodatkowo nocna nad wodą | ≤ 0,2 ms |
| Śnieg na ziemi | **blend palety materiału w shaderze wg `normal.y` i `snow_cover`** | 0 ms |
| Pory roku (liście) | **przesunięcie palety materiału `Foliage` wg `season`** | 0 ms |
| Kałuże / błoto | maska wilgotności z `precipitation`, wygaszana po opadzie | 0 ms |
| Dym z kominów | emiter cząstek, gęstość = `emission`, 64 cząstki/emiter, cap 256 emiterów, 1 draw call | ≤ 0,3 ms |

**Pory roku nie dotykają geometrii voxeli.** Remeshing całego miasta cztery razy w roku gry za
zmianę koloru liści byłby kosztem bez pokrycia. Sezon to przesunięcie palety — i wygląda tak samo.

**Sezon liczy się z `SimCalendar` (K-1), nie z własnej arytmetyki dat:** rok ma 360 dni,
12 miesięcy × 30, więc pora roku to dokładnie 90 dni, a granice sezonu są ostre i identyczne
w każdym roku gry. Brak lat przestępnych oznacza, że krzywa `day_curve` w `AmbientZone`
i krzywa wysokości słońca (M1) mogą być tablicowane raz na 360 dni bez przypadków szczególnych.

**Render nie odpytuje terenu i nie liczy o nim niczego (K-13).** `TerrainQuery` żyje w `sim/world`,
a `engine/render` nie może od niego zależeć — wołanie go stąd wywróciłoby test `dep_isolation`
uzgodniony z M1. W praktyce okazało się, że nie jest potrzebny nigdzie w tej fazie:

| Co wyglądało na potrzebę terenu | Skąd naprawdę pochodzi |
|---|---|
| Śnieg i błoto na ziemi | Shader terenu, blend palety wg `normal.y` już zrasteryzowanej geometrii |
| Osadzenie propów `InteriorKit` | `Building.aabb` + `floor_heights_dm` (M2) — wnętrza są w budynku, nie na gruncie |
| Kamera FPP | `CameraMode::FirstPerson{anchor}` (M1), pozycja encji ze snapshotu |
| Bloki impostorów dzielnic | Rzeczywista geometria chunków renderowana do atlasu |
| Daleki teren za 4 km | Heightmapa jako zwykły bufor w snapshocie + pass `far_terrain` (M1) |

Gdyby cokolwiek w przyszłości potrzebowało wysokości terenu per encja, **wnosi to
`fill_render_snapshot`** — ono biegnie po stronie sim i `TerrainQuery` ma pod ręką.

### 5.9 `engine/audio`

```rust
pub struct AudioEngine {
    buses:    [Bus; 4],              // Ambient | World | Ui | Music, każdy z gain + limiter
    voices:   VoicePool,             // 32 głosy światowe + 8 UI + 4 stemy muzyki
    zones:    Vec<AmbientZone>,
    emitters: Vec<SoundEmitter>,
    music:    MusicDirector,
    listener: Listener,              // pozycja = kamera; w FPP = głowa postaci
}

pub struct SoundEmitter {
    pub pos:       [f32; 3],
    pub source:    SoundSourceId,    // klucz do data/audio/
    pub gain:      f32,
    pub pitch:     f32,
    pub radius:    (f32, f32),       // (pełna głośność, cisza)
    pub occlusion: f32,              // raycast voxelowy, odświeżany 4 Hz — nie 60 Hz
    pub looping:   bool,
    pub owner:     Option<u32>,      // entity_lo zakładu — do „linia stoi = cisza"
}

pub struct AmbientZone {
    pub district:  DistrictId,
    pub bed:       AmbientBedId,     // Industry | Traffic | Park | Residential | Retail | Port
    pub weight:    f32,              // udział w mixie z odległości kamery do centroidu
    pub day_curve: CurveId,          // to samo miejsce brzmi inaczej o 3:00 i o 17:00
}
```

**Klastrowanie głosów — bez tego nie ma żywego miasta.** Emitery o tym samym `SoundSourceId`
w promieniu **15 m** łączą się w jeden wirtualny emiter w centroidzie, z gainem
`sum.min(single * 2.5)` (nie liniowo — 20 ciężarówek nie jest 20× głośniejsze niż jedna).
Bez tego 500 pojazdów na skrzyżowaniu to 500 głosów; z tym to 3.

Priorytet głosu: `gain × (1 − occlusion) / (1 + dist²)`. Przy wyczerpaniu puli głos o najniższym
priorytecie jest wygaszany rampą 120 ms (nie ucinany — trzask jest gorszy niż brak dźwięku).

**„Linia stoi = cisza" (§15.5)** to wprost:

```rust
emitter.gain = base_gain * (site.activity as f32 / 255.0);   // activity == 0 → gain == 0
```

Zatrzymanie linii słychać, zanim gracz otworzy panel. To jest kanał informacyjny, nie ozdoba.

**Muzyka adaptacyjna:**

```rust
pub enum MusicMood { Rozwoj, Stabilnosc, Napiecie, Kryzys }
```

Wyprowadzana z `PlayerViewRec.liquidity_ratio` (płynność / 30-dniowe koszty stałe) i `profit_trend`.
Warstwy (stemy) crossfadowane **wyłącznie na granicy taktu**, przejście 2 takty, **histereza ±15%** —
inaczej przy płynności oscylującej wokół progu muzyka miga. Muzyka nigdy nie przeskakuje w środku frazy.

**Audio nie czyta ECS.** `AudioEngine::update(&RenderSnapshot, &CameraState)` — ta sama zasada
co render, ten sam mechanizm egzekwowania (sekcja 6.3).

### 5.10 `RenderBudget` i adaptacja

```rust
pub struct RenderBudget {
    pub target_ms: f32,             // 16,6 lub 33,3 wg trybu kamery
    pub lod_scale: f32,             // 0,5..1,0 — globalny mnożnik progów odległości
    history:       [f32; 8],        // timestamp queries GPU
    cooldown:      u32,
}
pub struct RenderStats {            // eksportowane do devtools i do M12
    pub gpu_ms: f32, pub cpu_ms: f32,
    pub draw_calls: u32, pub triangles: u64,
    pub instances_by_lod: [u32; 4],
    pub impostor_regen: u8, pub impostor_resident: u16,
    pub voices_active: u8, pub chunk_remesh_count: u32,
    pub snapshot_select_ms: f32,    // koszt query_rect + odrzucenia po Aabb3 (zob. WP10)
}
```

Jeśli p95 z 8 klatek > `target_ms`, `lod_scale` spada o 10% (dolny limit 0,5). Jeśli p95 < 80%
celu przez 60 klatek — rośnie o 5%. **Cooldown 30 klatek** między zmianami; bez niego system
oscyluje i pop LOD-u jest widoczny jako pulsowanie.

**Reguła (rozstrzygnięta, nie rekomendacja — decyzja 9.11):** `RenderBudget` zmienia wyłącznie
odległości progowe i capy instancji po stronie GPU. **Nie dotyka `ViewQuery`, `SnapshotCaps`
ani niczego, co idzie do sim.** Obniżanie capu snapshotu pod presją wydajności byłoby sprzężeniem
zwrotnym render → symulacja — czyli dokładnie tym, czemu zapobiega wydzielenie `sim-snapshot`.
Cap snapshotu jest stałą konfiguracji, nie zmienną runtime'u.

### 5.11 Systemy i ich częstotliwość

Systemy renderu i audio **nie są systemami ECS** — nie ma ich w DAG-u schedulera symulacji.
Żyją w pętli renderu (PRD §17.1: „tick renderu niezależny").

| System | Miejsce | Częstotliwość | Dostęp |
|---|---|---|---|
| `fill_render_snapshot` | wątek sim, job | ≤ 30 Hz, ograniczane | `&World` → `&mut Snapshot` (bufor tylny) |
| `InstanceCompaction` | wątek renderu | per klatka | `&Snapshot` |
| `LodClassify` + `RenderBudget` | wątek renderu | per klatka | `&Snapshot`, `&mut RenderBudget` |
| `ImpostorRegen` | wątek renderu + job | ≤ 4 bloki/klatkę | `&VoxelWorld` (M1) |
| `InteriorGenerate` | job | on-demand, cache LRU | `&Snapshot`, `&BuildingGrammar` |
| `LightListBuild` | wątek renderu | per klatka | `&Snapshot` |
| `WeatherUpdate` | wątek renderu | per klatka | `&Snapshot.weather` |
| `AudioUpdate` | wątek audio | 60 Hz | `&Snapshot` |
| `OcclusionRaycast` | wątek audio, job | 4 Hz | `&VoxelWorld` |
| `MusicDirector` | wątek audio | 4 Hz + granica taktu | `&Snapshot.player` |

**Ani jeden z nich nie ma `&mut World`.** To jest egzekwowane sygnaturami.

---

## 6. Kontrakty międzyfazowe

### 6.1 Dostarczam

| Typ / funkcja | Crate | Dla kogo |
|---|---|---|
| `RenderSnapshot`, `CitizenRenderRec`, `VehicleRenderRec`, `SiteRenderRec`, `CrowdDensityRec`, `WeatherState`, `PowerRec`, `PlayerViewRec`, `SnapshotCaps` | `sim-snapshot` | M3, M4, M6, M7, M8, M9 — **wypełniają**; M12 — mierzy |
| `ViewQuery`, `fill_render_snapshot(&World, &ViewQuery, &mut RenderSnapshot)` | `sim-snapshot` | M3, M4 — implementują wypełnianie |
| `VoxModel`, `PaletteSlot`, `PaletteTable`, `VariantKey`, `ModelId` | `engine/voxel::model` | M2 (propy), M9 (podgląd w UI), M12 (modding) |
| `AnimationClip`, `AnimationState`, `ClipKind`, `ClipId`, `PoseAtlas` | `engine/voxel::anim` | M12 (modding — własne klipy) |
| `InstanceRenderer`, `GpuInstance` | `engine/render::instancing` | M2 (strumienie nakładek mogą użyć tej samej ścieżki) |
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
| 9.2 | **Cap snapshotu 24 576 mieszkańców / 8 192 pojazdów.** Czy sim akceptuje koszt selekcji top-K (≈ 0,8 ms na publikację) i czy `engine/spatial` (M2) daje zapytanie po AABB wystarczająco tanio | M2, M3, M4 | Akceptowany; selekcja w jobie równoległym do ticku | Otwarte |
| 9.3 | **Kto wylicza `anim_phase`.** Przyjęto: sim przy wypełnianiu snapshotu, funkcją czystą z `(instant, entity_index)`. Alternatywa: render z `entity_lo` — wtedy `anim_phase` znika z rekordu (32 B → 31 B, czyli bez zysku po wyrównaniu) | M3 | Sim wylicza; zero stanu animacji w ECS | Otwarte, niskie ryzyko |
| 9.4 | ~~Skala `activity`, `emission`, `stock_fill`~~ | M6, M7 | — | **ZAMKNIĘTE przez M6** (§6.4.3 ich dokumentu). Wzory w tabeli 5.2. WP7 i WP8 odblokowane. M6 dołożył `SITE_FAULT` (awaria ≠ bezczynność) i `smoke_kind`; oba zmieściłem w bajcie, który był wyrównaniem — `SiteRenderRec` nadal 24 B |
| 9.5 | ~~Czy powstaje crate `sim-snapshot`~~ | M0, M1 | — | **ZAMKNIĘTE przez koordynatora: tak, powstaje, a właścicielem jest M11** (nie M1, jak pierwotnie zakładałem). Konsumenci: M1 (podwójne buforowanie) i M9. Uzasadnienie przyjęte w całości: bez osobnego crate'a zasady „render nie mutuje symulacji" nie da się egzekwować grafem zależności, a reguła pilnowana samą dyscypliną zostanie złamana przy pierwszym pośpiechu. **Konsekwencja: dok. 00 §1 wymaga dopisania `sim-snapshot` do crate'ów tworzonych przez M11** |
| 9.6 | **`kira` czy `cpal`** dla `engine/audio` (PRD §16.1 dopuszcza oba). `kira` daje gotowy mikser, magistrale i crossfade — mniej kodu, ale własny model czasu. `cpal` to goły strumień — pełna kontrola, więcej pracy | — (decyzja M11) | `kira`: mikser, magistrale i przejścia muzyczne to dokładnie to, czego potrzebujemy, a nie mamy powodu ich pisać. Ambient przestrzenny i klastrowanie budujemy sami nad nim | Otwarte — **do zamknięcia prototypem w WP8, przed napisaniem miksera** |
| 9.7 | **Czy postać gracza w FPP jest zwykłym agentem ECS.** Plan zakłada, że tak, i że wejście gracza idzie przez komendy do sim. Jeśli M9 zaprojektuje postać gracza jako byt poza ECS, tryb FPP potrzebuje osobnej ścieżki pozycji | M9 | Zwykły agent; render tylko odczytuje `PlayerViewRec.eye` | Otwarte |
| 9.8 | ~~Trzy nowe katalogi `data/`~~ | dok. 00 | — | **ZATWIERDZONE przez koordynatora:** `data/palettes/`, `data/models/`, `data/audio/`, każdy z `schema_version` per plik. **Do weryfikacji:** w chwili pisania lista w dok. 00 §5 nadal ich nie zawiera (sprawdzone `grep`) — zgłoszone koordynatorowi |
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

**Rozkład:** 5 × L, 4 × M, 2 × S. Ciężar leży w WP2 (kontrakt danych), WP3 (animacja),
WP4 (LOD), WP5 (wnętrza) i WP8 (audio).

**Ścieżka krytyczna:** WP1 → WP2 → {WP3, WP4} → WP10.
WP8 można prowadzić równolegle od zamknięcia WP2 — potrzebuje wyłącznie `SiteRenderRec`
i `PlayerViewRec`, nie czeka na nic z grafiki.

**Największa niepewność:** WP2 zależy od rozstrzygnięcia decyzji 9.1 (granica z M1) i 9.5
(crate `sim-snapshot`). Bez nich WP2 nie powinien startować — inaczej grozi przepisanie
kontraktu danych po fakcie, a on jest fundamentem wszystkich pozostałych pakietów.
