# M11e — Budżet klatki

Podfaza 5 z 5 fazy **M11 — Prezentacja** (`M11-prezentacja.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M11b–M11d. |
| **Pakiety robocze** | WP10, WP4b (przejęte z M11b — `H-1`) |
| **Projekt techniczny** | §5.10, §5.11 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: cele FPS z PRD §20.2 dotrzymane na maszynie referencyjnej. |
| **Kryterium zamknięcia** | Kryterium WP10 oraz bramki 1–7 fazy M11 w `00-postep.md`. **Decyzja właściciela produktu**: maszyna referencyjna dla „GPU klasy średniej (2024)” musi być nazwana przed pomiarem. |
| **Poprzednia / następna** | `M11d-swiatlo-pogoda-dzwiek.md` · — (ostatnia w fazie) |

`RenderBudget`, adaptacyjne LOD i benchmarki klatkowe.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP4b | Impostory dzielnic: atlas runtime, LRU, regeneracja amortyzowana | WP4a (M11b), M1 | M |
| WP10 | `RenderBudget`, adaptacyjne LOD, benchmarki klatkowe | WP4b, WP5, WP6, WP7 | M |

### WP4b — Impostory dzielnic

**Opis.** Atlas kafli 128×128 m generowany w runtime z tego, co gracz zbudował: 8 azymutów,
kafel 128×128 px (RGBA8 + R16 depth), budżet **192 MB VRAM → 244 bloki rezydentne** z LRU,
regeneracja **≤ 4 bloki na klatkę** w osobnym passie z budżetem 1,5 ms i kolejką po odległości,
inwalidacja dwustopniowa (`RenderSnapshot.terrain_revision` jako tani filtr wstępny, potem
`generation` per blok), degradacja do chunków LOD 8× przy przekroczeniu budżetu. Projekt
techniczny w całości: `M11b-animacja-i-lod.md` §5.6 — pakiet zmienił adres, nie treść (`H-1`).

**Kryterium ukończenia.** Scena `bench_city` mieści się w budżecie 192 MB, `impostor_resident ≤ 244`
i `impostor_regen ≤ 4` w każdej klatce (§7.3 dokumentu fazy), a przy sztucznie obniżonym budżecie
nie powstaje **ani jedna dziura** — blok bez kafla rysuje się chunkiem LOD 8×.

### WP10 — `RenderBudget`, adaptacyjne LOD, benchmarki

**Opis.** Pomiar czasu GPU przez timestamp queries, `RenderStats` z podziałem na warstwy,
adaptacyjna skala progów LOD z histerezą. Harness benchmarków na scenach referencyjnych,
zapis do `bench/frames/*.json`, porównanie z baseline w CI.

**Kryterium ukończenia.** Cele §20.2 osiągnięte na sprzęcie referencyjnym; regresja p95 > 8%
zatrzymuje build.

**Dodatkowy pomiar z terminem — pass `pick_id` (zgłoszenie z M4c/WP14).** WP10 mierzy pass bufora ID
pieszych jako **siódmą pozycję** w `PASS_NAMES`. Dziś tablica ma sześć pozycji, a `pick_id` nie ma
znaczników czasu — więc jest jedynym passem, którego żaden budżet klatki nie widzi, choć rysuje
**pełną geometrię wszystkich pieszych drugi raz w każdej klatce** i czyści teksturę ID wielkości
okna, niezależnie od tego, czy kursor cokolwiek wskazuje. Pozycja kursora jest znana **przed**
nagraniem passa (`pick.rs`, pole `cursor`), więc wyjście wcześniej przy `None` jest poprawką
o jednej gałęzi — ale dopóki nie ma pomiaru, nie wiadomo, ile to warte. M4c/WP14 naprawia trzy
przyczyny tego samego objawu po stronie symulacji i **świadomie zostawia tę tutaj**, bo to
`engine/render`, nie `sim/traffic`. Zgłoszenie ma wartość wcześnie: pass powstał w M3d razem
z pickingiem pieszych i od tamtej pory jest w każdej klatce z mieszkańcami na ekranie.

**Dodatkowy pomiar z terminem — selekcja kadru (zobowiązanie wobec M2).** WP10 mierzy osobno koszt
`CsrGrid::query_rect` + odrzucenia po `Building.aabb` w scenach `bench_district` i `bench_city`
(licznik `snapshot_select_ms` w `RenderStats`). M2 świadomie zostawił `GridSpec` w 2D na podstawie
mojego argumentu i poprosił o sygnał, gdyby pomiar pokazał inaczej — **z zastrzeżeniem, że zmiana
`GridSpec` po M4 dotyka czterech crate'ów, więc zgłoszenie ma wartość tylko wcześnie.**
Próg alarmowy: `snapshot_select_ms > 0,3 ms` w `bench_city`. Po przekroczeniu WP10 **natychmiast**
zgłasza to M2, nie czeka na koniec fazy. Jeśli próg nie zostanie przekroczony — zamykamy temat
pisemnie, żeby nikt nie wracał do trzeciego wymiaru bez danych.

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

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
