# M11e — Budżet klatki

Podfaza 5 z 5 fazy **M11 — Prezentacja** (`M11-prezentacja.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M11b–M11d. |
| **Pakiety robocze** | WP10. **`WP4b` nie wchodzi** — przeniesiony do `M12a` decyzją właściciela produktu z 2026-09-19, popartą pomiarem (`H-10`) |
| **Projekt techniczny** | §5.10, §5.11 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: cele FPS z PRD §20.2 dotrzymane na maszynie referencyjnej. |
| **Kryterium zamknięcia** | Kryterium WP10 oraz bramki 1–7 fazy M11 w `00-postep.md`. **Decyzja właściciela produktu (zamknięta 2026-09-19)**: maszyną referencyjną jest **RTX 4070 Ti SUPER @ 1080p**, a progi są zaostrzone mnożnikiem 0,55 wobec celu §20.2, bo obietnica dotyczy RTX 4060 (`H-9`). |
| **Poprzednia / następna** | `M11d-swiatlo-pogoda-dzwiek.md` · — (ostatnia w fazie) |

`RenderBudget`, adaptacyjne LOD i benchmarki klatkowe.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Stan |
|---|---|---|---|---|
| WP10 | `RenderBudget`, adaptacyjne LOD, benchmarki klatkowe | WP5, WP6, WP7 | M | [x] |
| ~~WP4b~~ | ~~Impostory dzielnic~~ → **`M12a`/WP4** | — | — | przeniesiony (`H-10`) |

> **Kolejność z pierwotnego planu była niewykonalna i to jest osobna korekta (`H-13`).**
> Tabela mówiła „WP10 zależy od WP4b", a kryterium WP4b brzmiało „scena `bench_city`
> mieści się w budżecie 192 MB" — czyli odwoływało się do sceny, którą stawia dopiero
> WP10 w §7.2. Dwa pakiety czekały na siebie nawzajem. Rozstrzygnięcie jest takie,
> jakie w tej sytuacji jedyne ma sens: **najpierw przyrząd, potem optymalizacja**,
> bo bez pomiaru nie wiadomo, czy optymalizacja ma co optymalizować. Okazało się,
> że nie ma (`H-10`).

### WP4b — Impostory dzielnic (przeniesiony do `M12a`)

**Opis.** Atlas kafli 128×128 m generowany w runtime z tego, co gracz zbudował: 8 azymutów,
kafel 128×128 px (RGBA8 + R16 depth), budżet **192 MB VRAM → 244 bloki rezydentne** z LRU,
regeneracja **≤ 4 bloki na klatkę** w osobnym passie z budżetem 1,5 ms i kolejką po odległości,
inwalidacja dwustopniowa (`RenderSnapshot.terrain_revision` jako tani filtr wstępny, potem
`generation` per blok), degradacja do chunków LOD 8× przy przekroczeniu budżetu. Projekt
techniczny w całości: `M11b-animacja-i-lod.md` §5.6 — pakiet zmienił adres, nie treść (`H-1`).

**Kryterium ukończenia.** Scena `bench_city` mieści się w budżecie 192 MB, `impostor_resident ≤ 244`
i `impostor_regen ≤ 4` w każdej klatce (§7.3 dokumentu fazy), a przy sztucznie obniżonym budżecie
nie powstaje **ani jedna dziura** — blok bez kafla rysuje się chunkiem LOD 8×.

**Dlaczego nie tutaj.** Pomiar z WP10 odpowiedział na pytanie, którego przed nim nie dało się
zadać: scena `bench_city` bierze **5,76 ms p95** wobec progu 18,32 ms, a na mapie 16 km
z 272 tys. mieszkańców — 6,79 ms. Cały koszt rysowania chunków w tej scenie to
`depth_prepass` 1,64 + `opaque` 1,87 + `shadows` 4,51 ≈ 8 ms, a impostory dzielnic
zdjęłyby z tego **wycinek pasma 2–4 km**; dominujące kaskady cieni są bliskiego planu
i nie dotyczą ich wcale. Sam plan zapisał przy tym (`G-10` w `M11b`), że zysk jest
wydajnościowy, a nie wizualny: chunki M1 sięgają 4 km, dalej rysuje clipmapa, więc
**bez impostorów nie ma dziury w obrazie**. Pakiet kosztowałby 192 MB VRAM zajętych
na stałe i ok. 1,2 tys. linii nowego kodu po to, żeby przyspieszyć scenę mającą
trzykrotny zapas. Warunek powrotu jest zapisany w `M12a` i jest **liczbą, nie wrażeniem**:
`bench_city` przekraczające 60 % progu na maszynie docelowej albo w trybie 50×.

### WP10 — `RenderBudget`, adaptacyjne LOD, benchmarki

**Opis.** Pomiar czasu GPU przez timestamp queries, `RenderStats` z podziałem na warstwy,
adaptacyjna skala progów LOD z histerezą. Harness benchmarków na scenach referencyjnych,
zapis do `bench/frames/*.json`, porównanie z baseline przy ręcznym pomiarze
(w CI nie ma GPU — `N1.12-a`).

**Kryterium ukończenia.** Cele §20.2 osiągnięte na sprzęcie referencyjnym; regresja zatrzymuje
build. **Wykonane** — siedem scen odniesienia, p95 czasu GPU wobec progu zaostrzonego
mnożnikiem maszyny (`H-9`):

| Scena | GPU p50 | GPU p95 | Próg | Cel §20.2 |
|---|---:|---:|---:|---|
| `bench_street` | 4,77 ms | **6,84 ms** | 9,13 ms | 60 FPS |
| `bench_district` | 4,28 ms | **6,51 ms** | 9,13 ms | 60 FPS |
| `bench_city` | 2,77 ms | **6,02 ms** | 18,32 ms | 30 FPS |
| `bench_night_rain` | 4,50 ms | **6,92 ms** | 9,13 ms | 60 FPS |
| `bench_blackout` | 4,63 ms | **7,02 ms** | 9,13 ms | ≤ `night_rain` |
| `bench_interiors` | 4,08 ms | **6,42 ms** | 9,13 ms | 60 FPS |
| `bench_winter` | 1,11 ms | **5,28 ms** | 9,13 ms | 60 FPS |

Czas procesora na przygotowanie klatki w rendererze: **0,60 ms p95** wobec budżetu 4,0 ms
z §5.5. Złożenie danych klatki po stronie klienta (wypełnienie snapshotu, przekrój, szyldy):
**0,31 ms** w scenie bez tłumu, 3,85 ms w scenie z dwudziestoma czterema tysiącami
syntetycznych pieszych — ten drugi koszt jest **sceną pomiarową, a nie grą**: tłum powstaje
od nowa w każdej klatce i w rozgrywce go nie ma.

Bramka: `python scripts/frame_guard.py bench/frames`, linia bazowa w `bench/frames/baseline.json`,
instrukcja w `bench/frames/README.md`.

**Zamknięty pomiar — pass `pick_id` (zgłoszenie z M4c/WP14).** Pass ma znaczniki czasu jako
**ósmą** pozycję `PASS_NAMES` (`H-4`) i kosztuje **0,088 ms p95** przy 13,8 tys. encji w kadrze
(`bench_night_rain`). Przy okazji naprawiona przyczyna, a nie tylko pomiar: pass **nie otwiera
się bez kursora**. Do M11e rysował pełną geometrię wszystkich encji drugi raz i czyścił teksturę
identyfikatorów wielkości okna także wtedy, gdy nikt tych bajtów nie czytał — każdy zrzut
offscreen, każdy przebieg z CI i każda klatka z kursorem poza oknem. Poprawka ma jedną gałąź
(`PickBuffer::aktywny`), bo pozycja kursora była znana **przed** nagraniem passa; brakowało
wyłącznie pomiaru, który by powiedział, czy warto.

**Zamknięty pomiar — selekcja kadru (zobowiązanie wobec M2).** Licznik mierzy koszt
`CsrGrid::query_rect` po indeksie budynków plus odrzucenia i sortowanie po `Building.aabb`.
Miarodajne są sceny z **aktywnym cięciem poziomami**, bo tylko one tę ścieżkę otwierają:
`bench_interiors` **0,047 ms** i `bench_street` (widok pierwszoosobowy) **0,045 ms**, wobec
progu alarmowego 0,3 ms — **sześciokrotny zapas**. **Temat zamykamy pisemnie, tak jak
obiecywał plan:** `GridSpec` zostaje w 2D, M2 nie dostaje zgłoszenia, a wracać do trzeciego
wymiaru bez nowego pomiaru nie ma po co.

Pierwsza wersja tego pomiaru **mierzyła co innego, niż deklarowała**, i jest to warte zapisania,
bo klasa błędu wraca: zegar startował **po** publikacji snapshotu, a `query_rect` woła wyłącznie
generator wnętrz — który przy wyłączonym cięciu kończy się wczesnym powrotem. W scenach
orbitalnych licznik mierzył więc dwa wczesne powroty i pokazywał 0,016 ms, czyli liczbę
mieszczącą się w progu z powodu, który z progiem nie miał nic wspólnego. Znalazła to recenzja
przed commitem, nie test. Liczniki są od tej chwili **dwa**: `snapshot_select_ms` (całe
składanie danych klatki po stronie klienta) i `building_query_ms` (sam indeks budynków) —
bo jedna liczba na dwa pytania odpowiada tylko na jedno i nie mówi, na które.

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.10 `RenderBudget` i adaptacja

> **Wykonane z jedną korektą kształtu (`H-14`).** `RenderStats` **nie jest osobną
> strukturą o własnych `gpu_ms`, `draw_calls` i `triangles`, tylko obwódką na
> `FrameStats`**, którą renderer i tak wypełnia od M11b (`H-2`). Dwie struktury
> o wspólnych polach rozjeżdżają się przy pierwszej zmianie, więc wspólne pola są
> jedne, a doklejone jest wyłącznie to, czego renderer z definicji nie widzi:
> mikser dźwięku, strumieniowanie chunków klienta i koszt selekcji kadru.

```rust
// engine/render/src/budget.rs
pub struct RenderBudget {
    pub target_ms: f32,             // 16,6 lub 33,3 wg trybu kamery
    lod_scale: f32,                 // 0,5..1,0 — globalny mnożnik progów odległości
    historia: [f32; 8],             // timestamp queries GPU
    karencja: u32,
    z_zapasem: u32,
}
pub struct RenderStats {            // eksportowane do devtools i do M12
    pub frame: FrameStats,          // czasy passów, wywołania, trójkąty, instancje per LOD
    pub lod_scale: f32,
    pub voices_active: u8,          // mikser — `engine/audio` nie widzi renderu (§6.3)
    pub chunk_remesh_count: u32,    // strumieniowanie klienta (`I-14`)
    pub snapshot_select_ms: f32,    // query_rect + odrzucenia po Building.aabb
    pub lights: u32,                // dowód, że blackout w ogóle zaszedł
    pub impostor_regen: u8, pub impostor_resident: u16,   // zera do czasu `M12a`/WP4
}
```

**`RenderStats` w `engine/devtools` (DoD §7.5 pkt 6) idzie przez `MetricSink`, a nie przez
typ w tamtym crate'cie** — i to nie jest skrót: `engine/render` **zależy** od
`engine/devtools` (`ClusterOccupancy`, zrzut PNG), więc zależność w drugą stronę zamknęłaby
cykl, którego Cargo nie zbuduje. `RenderStats::record_into(&mut MetricSink, Tick)` zapisuje
siedemnaście serii (czasy ośmiu passów w mikrosekundach plus liczniki), a `MetricSink`
eksportuje CSV — czyli M12 startuje od gotowego szeregu, a nie od pisania drugiego licznika.

**Skala budżetu wchodzi w jednym miejscu:** `LodBands::scaled(k)` w `Renderer::set_entities`.
Pętla sprzężenia zwrotnego domyka się w `finish_stats`, a nie u klienta — każdy konsument
renderu ma dostać tę samą adaptację, a jedyne wejście (czas GPU) jest po stronie renderu.
Do okna wchodzą **wyłącznie świeże pomiary**: znaczniki czasu wychodzą co drugą klatkę,
więc klatka bez odczytu nie jest pomiarem zera, tylko brakiem pomiaru.

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


---

## Zmiany wpisane po M11a

Zgodnie z `K-18`. To są rzeczy, o których wiadomo **na pewno** po zamknięciu M11a;
podfaza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.
Pełna tabela `E-n` jest w `M11a-format-i-snapshot.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| I-1 | **Ścieżka klatki po stronie procesora bierze 322 µs przy 26 tys. encji** — klucz `m11a-1` w `benches/baseline.json`, wobec budżetu 1,5 ms z §5.3 | To jest pierwsza zmierzona pozycja budżetu klatki i jedyna, która po M11a istnieje. Mierzy filtr stożka, klasyfikację poziomu detalu, sortowanie i zapis instancji **razem**, bo razem stoją w budżecie i razem się je przekracza |
| I-2 | **Sortowanie instancji idzie porównaniami, nie radixem z §5.3** | Różnych kluczy `(model, poziom)` jest rzędu dziesiątek, więc sortowanie kubełkowe byłoby szybsze — ale dopiero wtedy, gdy 0,2 ms z budżetu zacznie być widoczne w pomiarze. Sufit jest nazwany w kodzie komentarzem `ponytail:` i wymiana dotyczy jednej linii |
| I-3 ★ | **Cap snapshotu jest stałą konfiguracji i `RenderBudget` nie ma prawa go ruszać** — decyzja 9.11, wykonana kształtem typu | `SnapshotCaps` jest `Copy`, nie zawiera referencji i wchodzi do `ViewQuery`, czyli do jedynego kanału render → sim. Obniżanie capu byłoby sprzężeniem render → symulacja, czyli dokładnie tym, czemu `sim-snapshot` zapobiega |
| I-4 | **Snapshot waży 1,31 MB rezydentnie na bufor, 2,62 MB przy podwójnym buforowaniu** — i nie zależy od wielkości miasta | Rozmiar jest sumą pojemności, nie zapełnienia: `RenderSnapshot::resident_bytes()` zwraca tę samą liczbę dla miasta pustego i pełnego, czego pilnuje test. To jest liczba, którą M12 wpisuje do budżetu pamięci |
| I-5 | **`GpuInstance` ma 36 B**, więc upload to 720 KB na klatkę przy 20 tys. encji (43 MB/s po PCIe), a nie 640 KB (`E-8`) | §5.3 wyliczał 32 B, nie wymieniając w nich pickingu. Budżet pasma tego nie ogranicza, ale liczba w raporcie ma się zgadzać z liczbą w kodzie |
| I-6 | **Pojazdy w trybie 50× nie istnieją i kliknięcie w nie też nie** — warstwa Mikro jest tam wyłączona z konstrukcji (`M12c`) | Zapis z 2026‑09‑17 żądał, żeby pass `pick_id` wypełniał bufor także przy wyłączonym LOD mikro **albo** żeby ta podfaza powiedziała wprost, że pojazdów wtedy nie ma. Mówi wprost: bez warstwy Mikro `vehicle_snapshot` zwraca pustą listę, więc do snapshotu nie trafia ani jeden pojazd i nie rysuje się ani jedna instancja bez identyfikatora. „Klikalne jest wszystko, co widoczne" trzyma się, bo niewidoczne jest jedno i drugie naraz — i to ma sprawdzić test tej podfazy, a nie założenie |

---

## Zmiany wpisane po M11b

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| H-1 ★ | **M11e przejmuje `WP4b` — impostory dzielnic** (atlas kafli 128×128 m generowany w runtime, 8 azymutów, LRU, budżet 192 MB, regeneracja ≤ 4 bloki na klatkę z budżetem 1,5 ms, inwalidacja dwustopniowa `terrain_revision` + `generation` per blok, degradacja do chunków LOD 8×). Projekt techniczny stoi w `M11b` §5.6 i **się nie zmienia** — zmienia się wyłącznie adres wykonania | Trzy powody, wszystkie z tej samej strony. **(1)** Ten pakiet jest budżetem klatki, a nie obrazem: kolejka regeneracji z limitem czasu i LRU na 192 MB to dokładnie mechanizm, który opisuje §5.10 (`RenderBudget`), więc rozbicie go na dwie podfazy znaczyłoby dwa liczniki tego samego. **(2)** Kryterium WP4 („scena `bench_city` mieści się w budżecie VRAM impostorów") odwołuje się do sceny referencyjnej, która powstaje **tutaj**, w §7.2 dokumentu fazy — w M11b nie ma go jak zapalić ani na zielono, ani na czerwono. **(3)** Bez impostorów dzielnic **nie ma dziury w obrazie**: chunki M1 sięgają 4 km (`LOD_RADII_M`), a dalej rysuje clipmapa terenu, więc brak kafli kosztuje czas klatki, a nie widok. Impostory **encji** są zamknięte w M11b i nie wchodzą tu ponownie |
| H-2 | **`FrameStats` ma już `instances` i `instance_batches`** — dwa pierwsze pola `RenderStats` z §5.10 powstały w M11b (`G-14`) | Liczba encji i liczba wsadów odpowiadają na pierwsze pytanie przy pustym albo wolnym kadrze. WP10 dokłada do nich podział na warstwy i czasy passów, a nie zaczyna od zera |
| H-3 | **Scena pomiarowa tłumu jest w kliencie**: `--crowd N`, `--crowd-step M` i `--no-anim` (`G-12`) | `bench_street` i `bench_district` wymagają 6 000 i 24 000 encji w kadrze, a warstwa Mikro w oknie 900 m oddaje ich kilkadziesiąt. Zamrożone zapisy z §7.2 tego nie zmienią, dopóki gęstość Mikro jest taka, jaka jest — a scena syntetyczna mierzy **rysowanie**, czyli to, czego dotyczy budżet klatki |

## Zmiany wpisane po M11d

Zgodnie z `K-18`. Pełna tabela `I-n` jest w `M11d-swiatlo-pogoda-dzwiek.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| H-4 ★ | **`PASS_NAMES` ma już siedem pozycji, a siódma to `weather`, nie `pick_id`.** Pomiar passa bufora ID zostaje w zakresie WP10 i wchodzi jako **ósma** | Cząstki pogody i dymu potrzebowały własnego przebiegu z mieszaniem alfa i głębią tylko do odczytu, więc dostały własną pozycję w tablicy — dopisaną **na końcu**, bo indeksy znaczników czasu są pozycyjne i wstawienie w środku przesunęłoby każdy wcześniejszy pomiar. Zgłoszenie o `pick_id` z M4c/WP14 zostaje bez zmian co do treści; zmienia się wyłącznie numer, pod którym wejdzie |
| H-5 ★ | **Sceny `bench_night_rain`, `bench_blackout` i `bench_winter` mają czym się ustawić**: klient dostał `--precip`, `--snow` i `--blackout` (`I-15`) | Model pogody M8c losuje opad i nie da się go poprosić o ulewę, a zamrożony zapis z §7.2 utrwaliłby pogodę tej jednej doby. Wymuszenie dotyczy **wyłącznie snapshotu**, więc scena nie zmienia stanu świata i nie psuje `render_off_equals_render_on` |
| H-6 | **`bench_blackout ≤ bench_night_rain` ma mechanizm, a nie tylko nadzieję**: pass pogody jest **pomijany w całości**, gdy nie ma ani opadu, ani dymu, a lista świateł przy zgaszonej dzielnicy jest krótsza, nie pełna zer | Kryterium WP6 mówi „ścieżka brak świateł nie ma patologii". Rysowanie zerowej liczby cząstek kosztuje przełączenie celu renderowania i tyle samo czyszczenia co pełna scena — dlatego pusty pass się nie otwiera |
| H-7 | **`RenderStats` dostaje dwie liczby z M11d**: `weather_particles` (cząstki tej klatki) i `chunk_remesh_count` po stronie strumieniowania klienta | Pierwsza pilnuje progu 0,8 ms z WP7, druga jest całym dowodem na „pory roku nie dotykają geometrii" — a bez licznika to kryterium jest deklaracją, nie pomiarem (`I-14`) |
| H-8 | **Warstwa dźwiękowa ma własny budżet i własny licznik** (`AudioStats`: głosy czynne, odrzucone, nastrój, takt), a `--no-audio` wyłącza ją w całości | `bench_street` i `bench_district` mierzą klatkę, a mikser chodzi na własnym wątku — bez przełącznika nie da się rozdzielić kosztu rysowania od kosztu dźwięku. Pula 32 głosów jest twarda, więc koszt jest ograniczony z góry niezależnie od gęstości miasta |

---

## Zmiany wpisane po M11e

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. To jest ostatnia podfaza
fazy M11, więc tabela zbiera też poprawki, które wędrują do M12 i do R2.

| # | Zmiana | Dlaczego |
|---|---|---|
| H-9 ★ | **Maszyną referencyjną jest RTX 4070 Ti SUPER @ 1080p, a progi są zaostrzone mnożnikiem `ZAPAS = 0,55`** wobec celów z PRD §20.2 (16,6 → 9,13 ms; 33,3 → 18,32 ms). Decyzja właściciela produktu z 2026-09-19; zamyka decyzję 9.10 dokumentu fazy i pozycję „Maszyna referencyjna" w `00-postep.md` | Obietnica §20.2 dotyczy „GPU średniej klasy 2024", a mierzyć da się wyłącznie na karcie wyraźnie szybszej. Próg wzięty wprost z §20.2 byłby wtedy obietnicą niepokrytą niczym: scena mieszcząca się w 16,6 ms tutaj nie mówi nic o RTX 4060. Mnożnik jest **jawną stałą w `tools/magnat/src/scenes.rs`, a nie liczbą wtopioną w progi** — kiedy ktoś zmierzy te same sceny na maszynie docelowej, zmienia się jedna linia. Sufit jest nazwany: 0,55 to stosunek przepustowości obu kart z materiałów producenta, nie pomiar tej gry |
| H-10 ★ | **`WP4b` (impostory dzielnic) nie wchodzi do M11 i przenosi się do `M12a` jako WP4**, z warunkiem powrotu wyrażonym liczbą: `bench_city` powyżej 60 % progu na maszynie docelowej albo w trybie 50× | Pomiar z WP10: `bench_city` bierze 5,76 ms p95 wobec progu 18,32 ms, a na mapie 16 km z 272 tys. mieszkańców 6,79 ms. Cały koszt rysowania chunków to ≈ 8 ms (`depth_prepass` 1,64 + `opaque` 1,87 + `shadows` 4,51), a impostory zdjęłyby z tego wycinek pasma 2–4 km; dominujące kaskady cieni są bliskiego planu i nie dotyczą ich wcale. `G-10` zapisało przy tym samo, że zysk jest wydajnościowy, a nie wizualny — bez impostorów **nie ma dziury w obrazie**, bo chunki sięgają 4 km i dalej rysuje clipmapa. 192 MB VRAM na stałe i ok. 1,2 tys. linii kodu za przyspieszenie sceny z trzykrotnym zapasem to koszt bez odbiorcy |
| H-11 ★ | **Regresja mierzy się na p50, nie na p95, z progiem 10 % zamiast 8 %.** Próg bezwzględny (czy scena mieści się w celu §20.2) zostaje na p95 i się nie zmienia | Cztery przebiegi tej samej sceny na tej samej maszynie i tym samym kodzie: p50 4,04–4,27 ms (rozrzut 5,7 %), p95 5,88–6,49 ms (10,3 %), p99 6,58–7,67 ms (16,7 %). Bramka z progiem 8 % na p95 zapalałaby się na samym rozrzucie zegarów karty, czyli na niczym — a bramka zapalająca się losowo uczy ludzi ją ignorować. Dwie metryki mają tu dwie różne role i mieszanie ich było błędem planu, nie pomiaru |
| H-12 ★ | **Kryterium „`bench_blackout` ≤ `bench_night_rain`" porównuje liczbę świateł i pass `clusters`, a nie czas całej klatki** | Obie sceny to dwa osobne procesy, a między nimi karta stoi na innym zegarze: passy, których blackout nie dotyka z konstrukcji (`depth_prepass`, `water`, `post`, `pick_id`), różniły się w pomiarze o 40–70 % w tę samą stronę. Porównanie całej klatki mierzyło stan sprzętu. Passem zależnym od listy świateł jest `clusters` i tylko on — zmierzone 0,240 ms wobec 0,356 ms przy 554 światłach wobec 1 255 |
| H-13 ★ | **Kolejność „WP10 zależy od WP4b" była niewykonalna** i pakiety czekały na siebie nawzajem: kryterium WP4b odwoływało się do sceny `bench_city`, którą stawia dopiero WP10 w §7.2 | To jest przypadek (4) z `K-18` — „kolejność pakietów jest niewykonalna, bo któryś potrzebuje danych, które powstają po nim". Rozstrzygnięcie: najpierw przyrząd, potem optymalizacja. Bez pomiaru nie wiadomo, czy optymalizacja ma co optymalizować, i tym razem okazało się, że nie ma |
| H-14 | **`RenderStats` jest obwódką na `FrameStats`, a nie osobną strukturą z własnymi `gpu_ms`, `draw_calls` i `triangles`** | Renderer liczy te pola od M11b (`H-2`) i wpisuje je do `FrameStats`. Dwie struktury o wspólnych polach rozjeżdżają się przy pierwszej zmianie — a rozjazd w statystykach widać dopiero jako liczbę, której nikt nie umie powiązać z przyczyną. Doklejone jest wyłącznie to, czego renderer z definicji nie widzi: mikser dźwięku, strumieniowanie chunków klienta i koszt selekcji kadru po stronie symulacji |
| H-15 ★ | **Sceny odniesienia są presetami argumentów nad deterministycznym generatorem, a nie zamrożonymi zapisami `bench/scenes/*.mgsave`** z §7.2 | Formatu `.mgsave` nie ma i nigdy nie powstał: zapis gry to ziarno plus dziennik wejść (`game/src/save.rs`), a zrzutu świata do pliku nie ma nigdzie w repozytorium. Zamrożony zrzut wymagałby schematu zapisu, którego właścicielem jest **M12b**, więc zamrożenie go tutaj przesądzałoby cudzą decyzję przed czasem. Ten sam seed daje ten sam świat — tego pilnuje macierz hashy terenu z M1 |
| H-16 ★ | **Pass `pick_id` nie otwiera się bez kursora** i ma znaczniki czasu jako ósma pozycja `PASS_NAMES`. Zmierzone: 0,088 ms p95 przy 13,8 tys. encji | Zgłoszenie z M4c/WP14 domknięte przyczyną, nie tylko pomiarem. Pass rysował pełną geometrię wszystkich encji **drugi raz w każdej klatce** i czyścił teksturę identyfikatorów wielkości okna także wtedy, gdy nikt tych bajtów nie czytał — każdy zrzut offscreen i każda klatka z kursorem poza oknem. Poprawka ma jedną gałąź, bo pozycja kursora była znana przed nagraniem passa; brakowało wyłącznie pomiaru, który by powiedział, czy warto |
| H-17 | **Zobowiązanie wobec M2 zamknięte pisemnie: `GridSpec` zostaje w 2D.** `snapshot_select_ms` w `bench_city` to 0,0165 ms wobec progu alarmowego 0,3 ms | Plan wymagał zamknięcia tematu w jedną albo drugą stronę („żeby nikt nie wracał do trzeciego wymiaru bez danych"). Osiemnastokrotny zapas jest odpowiedzią. Licznik zostaje w `RenderStats` i w raporcie każdej sceny, więc gdyby indeks budynków kiedyś urósł, liczba jest pod ręką i nie trzeba budować przyrządu od nowa |
| H-18 ★ | **Zegar prezentacji idzie w scenie odniesienia, choć symulacja stoi na pauzie** | Sześćset klatek ma mierzyć ten sam świat — stąd pauza. Ale od zegara prezentacji zależą faza klipu, ruch cząstek pogody i rampa wygaszenia dzielnicy, więc z nim zatrzymanym `bench_blackout` **nigdy nie gasił ani jednej latarni** i porównywał scenę samą ze sobą. To jest właściwe użycie rozdziału z `K-22` i `G-5`, a nie obejście pauzy: zegar gry i zegar prezentacji są dwiema różnymi rzeczami i dokładnie po to |
| H-19 ★ | **`--blackout N` gasi dzielnice *widoczne w kadrze*, a nie N pierwszych po indeksie** | Do pierwszego pomiaru wymuszenie gasiło dzielnice 0..N, a kamera scen stoi nad centrum — lista świateł miała wtedy tyle samo pozycji z blackoutem i bez niego (1 255 w obu przebiegach). Wybór idzie po liczbie latarni w zasięgu oka, bo to ona jest kosztem: dzielnica bez ani jednej latarni w kadrze zgaszona nie zmienia ani jednej klatki. Remis rozstrzyga numer dzielnicy, więc wynik jest ten sam w każdym przebiegu |
| H-20 ★ | **`snapshot_instancing.rs` z M11a nie był uruchamiany w CI przez ani jeden krok** i jest tam od M11e razem z nowym testem §7.1 | Test `#[ignore]` bez kroku `--include-ignored` jest testem, którego nie ma. M11a napisała go jako dowód, że droga sim → snapshot → bufor instancji cokolwiek oddaje, i przez trzy podfazy nikt go nie odpalał poza autorem. Krok w zadaniu `determinism` obejmuje teraz cały pakiet `magnat` |
| H-22 | **Pass wody też zeruje swoją pozycję w `pass_ms`, gdy się nie odbył** | Ta sama klasa usterki co `I-25` w M11d, tylko o passie, którego tamta poprawka nie objęła: `water` otwiera się wyłącznie przy niepustej liście chunków z taflą, a `resolve_timer` rozwiązuje cały zakres znaczników. Kadr bez wody pokazywałby czas z klatki, w której woda była — czyli raport mierzyłby pracę, której nie wykonano. Trzy passy klatki są warunkowe (`water`, `weather`, `pick_id`) i od tej chwili wszystkie trzy zachowują się tak samo |
| H-21 | **Czas procesora klatki mierzy się bez czekania na synchronizację pionową** | Pierwszy pomiar dawał `cpu_ms` p95 25,7 ms na scenie, której GPU zajmowało 5,5 — bo `get_current_texture` blokuje do synchronizacji pionowej i był w środku mierzonego odcinka. Liczba mierzyła monitor, nie kod. Po rozdzieleniu: 0,60 ms p95 wobec budżetu 4,0 ms z §5.5 |

### Znalezione w recenzji przed commitem

Jedenaście poprawek z przeglądu tej samej zmiany. Żadna nie zmienia zakresu; wszystkie
dotyczą rzeczy, które **mierzyłyby nieprawdę** — a przyrząd pokazujący nieprawdę jest
gorszy od jego braku, bo braku nikt nie weźmie za pomiar.

| # | Zmiana | Dlaczego |
|---|---|---|
| H-23 ★ | **`snapshot_select_ms` mierzył dwa wczesne powroty, a nie indeks budynków.** Zegar startował **po** publikacji snapshotu, a `CsrGrid::query_rect` woła wyłącznie generator wnętrz — który przy wyłączonym cięciu kończy się natychmiast. Liczniki są teraz **dwa**: `snapshot_select_ms` (całe składanie danych klatki) i `building_query_ms` (sam indeks budynków) | To jest najpoważniejsze znalezisko recenzji, bo unieważniało **wniosek**, nie tylko liczbę: 0,0165 ms mieściło się w progu 0,3 ms z powodu, który z progiem nie miał nic wspólnego, a na tej podstawie zamykaliśmy zobowiązanie wobec M2 o `GridSpec`. Po poprawce miarodajne są sceny z aktywnym cięciem — `bench_interiors` 0,047 ms i `bench_street` 0,045 ms — i zobowiązanie zamyka się **naprawdę**, z sześciokrotnym zapasem. Jedna liczba na dwa pytania odpowiada tylko na jedno i nie mówi, na które |
| H-24 ★ | **Kryterium `seasons_do_not_remesh` nie mogło zapalić się na czerwono.** Scena stoi na pauzie, sezon liczy się z ticku świata, więc w oknie pomiaru nigdy się nie zmieniał i przyrost licznika był zerem **z konstrukcji**. `bench_winter` przewija teraz cztery pory roku w oknie pomiaru (`Ambience::force_season`) | §7.3 mówi wprost „przejście przez 4 pory roku", a zielone kryterium bez przejścia jest zielone z tego samego powodu co kryterium spełnione — i nie da się ich odróżnić. Test `scena_zimowa_przewija_wszystkie_cztery_pory_roku` pilnuje, że przewijanie oddaje cztery różne wartości, a nie trzy albo jedną |
| H-25 | **Maska pominiętych passów nakładała się na pomiar z innej klatki.** Znaczniki czasu wracają o klatkę później, a flagi „pass się odbył" opisywały klatkę bieżącą. Maska jedzie teraz **razem z odczytem** (`PassTimer::maska`, ustawiana w `resolve_timer`) | Kursor wychodzący poza okno między dwiema klatkami zerował czas passa, który się odbył i został zmierzony — a suma zaniżona o ten czas szła prosto do `RenderBudget`. Kanał, którym do pętli sprzężenia zwrotnego wchodzi liczba niezwiązana z żadną klatką, jest gorszy od braku pętli |
| H-26 | **`gpu_samples` w raporcie było zawyżone dwukrotnie.** `pass_ms` odbudowuje się co klatkę z ostatniego udanego odczytu, więc warunek „większe od zera" przepuszczał każdą klatkę. `FrameStats` dostaje `gpu_fresh` i do próbki wchodzą wyłącznie klatki świeże | Wszystkie siedem raportów meldowało 600 próbek z 600 klatek, choć odczyt wychodzi co drugą. Percentylom to nie szkodziło (każdy pomiar liczył się dwa razy), ale liczba próbek jest jedyną rzeczą w raporcie, która mówi, **czy pomiar w ogóle szedł** — i akurat ona kłamała. Teraz raporty pokazują 300 z 600 |
| H-27 ★ | **Bramka regresji przepuszczała przerwany przebieg.** Raporty scen są zacommitowane, a klient przerwany przed końcem nie zapisywał nic — więc `frame_guard.py` porównywał wczorajszy plik z linią bazową wygenerowaną z tego samego pliku. Trzy poprawki: klient **kasuje raport na starcie przebiegu**, skrypt sprawdza obecność wszystkich siedmiu scen wobec **kanonu**, a werdykt progu liczy sam zamiast ufać polu `verdict` z pliku | Bramka mówiąca „brak regresji" o scenie, która się nie uruchomiła, jest gorsza od braku bramki: uczy ufać wynikowi, którego nie ma. Przy okazji `--update` odmawia zapisania linii bazowej z niepełnego przebiegu, a wartość bazowa `0.0` przestała udawać brak wpisu — na maszynie bez `TIMESTAMP_QUERY` wyłączała kontrolę regresji dla sceny na zawsze |
| H-28 | **`--blackout N` wracał po cichu do numerowania po indeksie**, gdy w zasięgu oka nie było ani jednej latarni (kamera wysoko, dzielnica bez oświetlenia): wszystkie liczniki zerowe, sortowanie stabilne, wynik 0, 1, 2… Ranking spada wtedy na **całkowitą** liczbę latarni w dzielnicy, a dzielnice bez ani jednej wypadają z wyniku | Dokładnie to zachowanie ta funkcja miała zastąpić (`H-19`) — wróciłoby w innym kadrze i nikt by tego nie zauważył, bo `bench_blackout` działa. Gaszenie dzielnicy bez latarni zajmuje przy tym miejsce dzielnicy, która je ma |
| H-29 | **Seria `render.lod_scale_permille` była ciągiem zer.** `us(0.9)` daje 900, a dzielenie przez tysiąc — zero. Przy okazji cała rodzina serii zaokrągla teraz zamiast obcinać | Komentarz nad tą linią mówił „0,9 i 0,95 muszą się różnić", a nie różniły się: jedyną wartością dającą coś innego niż zero było 1,0. Seria, po której M12 miało zacząć profilowanie adaptacji detalu, nie niosła jej wcale. Obcięcie gubiło też do jednej mikrosekundy na każdym czasie passa, zawsze w tę samą stronę |
| H-30 | **Cap snapshotu 4 096 w teście §7.1 mógł niczego nie obcinać.** Miasto testu ma osiem tysięcy mieszkańców w oknie 900 m, więc wariant „cap 4 096" bywał bit w bit powtórką wariantu bez capu. Doszedł wariant z capem 256 i **asercja, że cap faktycznie obciął** | Kryterium „cap jest wizualny, nie ekonomiczny" trzymałoby się zielone bez mierzenia czegokolwiek — ta sama klasa błędu, którą łapie bramka `szczyt_instancji > 0` przy kamerach. Test sprawdza teraz najpierw, ile mieszkańców kadr oddaje bez obcięcia, i dopiero potem porównuje |
| H-31 | **`draw_calls` liczyło wsady, a nie wywołania.** Wsad wskazujący na model bez wypieczonej siatki jest pomijany w `draw_with`, a licznik dodawał go dwa razy (pass nieprzezroczysty i bufor identyfikatorów). Liczbę zwracają teraz `InstanceRenderer::draw` i `draw_ids` | Ta sama reguła, którą ta zmiana postawiła przy chunkach: liczba wywołań powstaje **tam, gdzie wywołania powstają**, a nie jest odtwarzana z drugiej strony. Odtworzenie rozjeżdża się przy pierwszej zmianie warunku pomijania. Po poprawce `bench_street` pokazuje 25 wywołań zamiast 16 — i to jest liczba prawdziwa, nie gorsza |
| H-33 | **`PassTimer::zbierz` wychodził wcześniej bez odmapowania bufora** przy nieudanym odczycie, zostawiając go zajętym na zawsze | Kod przeniesiony jeden do jednego z M1, więc to nie jest regresja — ale wcześniej kosztowało to raport, a od M11e zatrzymywałoby **pętlę sprzężenia zwrotnego sterującą poziomem detalu**, i to po cichu do końca sesji. Rangę usterki podnosi konsument, a nie jej wiek |
"""

### Co zostaje otwarte po M11e

| Rzecz | Adres |
|---|---|
| Impostory dzielnic (`WP4b`): atlas runtime, LRU, budżet 192 MB, regeneracja ≤ 4 bloki na klatkę | **`M12a`/WP4**, z warunkiem powrotu wyrażonym liczbą (`H-10`) |
| `terrain_revision` w `RenderSnapshot` jest polem, którego **nikt nie inkrementuje** | **`M12a`**, razem z `WP4b` — to jego pierwszy stopień inwalidacji i nie ma drugiego czytelnika |
| Progi klatkowe nie są w CI: wspólne runnery nie mają karty graficznej | Ręczny pomiar na maszynie z GPU, raport w repozytorium (`N1.12-a`: nocnego biegu z GPU nie ma). CI sprawdza poprawność (§7.1) i mikrobenchmarki procesora |
| `bench_street` stoi kamerą pierwszoosobową **bez postaci gracza** (`--observe`) | M12, jeśli scena z postacią miałaby dać inny kadr. Koszt rysowania jest ten sam, a scena z postacią wymagałaby wyboru wariantu startu w skrypcie |
| Koła pojazdu kręcą się ze stałą prędkością klipu, a `wheel_phase` z rekordu nie ma czytelnika | **R2** — pomiar nie dał powodu, żeby robić to w M11: pojazdów w oknie gry i tak jest zero (pozycja 71 wykazu) |
