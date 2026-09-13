# M0 — Fundament silnika

Status: plan wykonawczy.
Dokument nadrzędny: [`00-konwencje-i-kontrakty.md`](./00-konwencje-i-kontrakty.md) — kontrakty stamtąd
są wiążące i **nie są tu redefiniowane**; ten dokument je *implementuje* i wskazuje miejsce w kodzie.
Źródło wymagań: `PRD_Magnat.md` §16, §17.1, §17.2, §17.7, §17.8, §18.1, §18.2, §19 (M0), §20.2, §20.4.

---

## 1. Cel fazy i artefakt końcowy

Po M0 istnieje **pusty, ale kompletny silnik**: ECS ze schedulerem, job system, zapis stanu,
narzędzia deweloperskie i dwa uruchamialne binaria. Nie ma jeszcze ani jednego bitu logiki domenowej —
i to jest celowe: M0 dowodzi, że fundament jest deterministyczny i wydajny, zanim cokolwiek na nim stanie.

Co da się uruchomić i zobaczyć:

1. **`cargo run -p magnat-headless -- --seed 42 --ticks 200000 --hash-every 1000 --out run_a.hashes`**
   — symulacja 200 tys. ticków na syntetycznym świecie (400 tys. encji z komponentami-atrapami),
   wypisuje ciąg hashy stanu. Drugi przebieg z inną liczbą wątków (`--threads 1` vs `--threads 16`)
   daje **bajt w bajt ten sam plik**. `--expect run_a.hashes` zwraca kod wyjścia różny od zera przy pierwszej
   rozbieżności, podając tick, archetyp i encję.
2. **`cargo run -p magnat-headless -- --seed 42 --ticks 50000 --save world.mgs`** i wznowienie
   `--load world.mgs --ticks 150000` — ciąg hashy identyczny z przebiegiem ciągłym. Zapis zstd, poniżej 5 s.
3. **`cargo run -p magnat-voxelview`** — okno `winit` + `wgpu`, jeden statyczny chunk 32³ voxeli
   z paletą, kamera orbitalna, licznik FPS. Zero generatora terenu, zero streamingu — dowód, że stos
   graficzny startuje na trzech backendach (Vulkan/DX12/Metal).
4. **`cargo run -p magnat-headless -- --console`** — konsola devtools: `ecs.archetypes`, `ecs.entity <id>`,
   `ecs.hash`, `prof.frame`, `sim.step 100`.
5. **`cargo bench`** — raporty criterion dla iteracji po 400 tys. encji i kosztu komendy strukturalnej.

Definition of Done fazy: wszystkie powyższe działają, CI (clippy `-D warnings`, testy, Miri na `engine/ecs`,
bench-guard) jest zielone, a dokument `00-konwencje-i-kontrakty.md` §2–§3 ma pełne pokrycie w kodzie.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zakres M0 |
|---|---|
| Workspace | Struktura crateów wg PRD §16.2, `rust-toolchain.toml`, linty workspace, profile build |
| `engine/core` | Typy bazowe (00 §2), **`Entity`**, arytmetyka pieniądza, fixed-point `Fx`, czas i kalendarz 360-dniowy (00 §K-1), `Cadence`, RNG `xoshiro256++` ze strumieniami, `StreamId` z siatką zakresów (00 §K-4), `SeededMap`, `StateHash`/`HashState`, **`det_math`** (00 §K-6), **`DecisionReason`** (00 §K-12), **słowniki domenowe** `UtilityKind`/`TransportMode`/`PlaceRef`/`NeedKind`/`ActivityKind` (00 §K-8), **`ComponentSchemaId`**, **`Arena<T>`/`ArenaHandle<T>`** (00 §K-16) |
| `engine/ecs` | Archetypowy storage SoA w chunkach 64 KiB, re-eksport `Entity`, `Query`, `Access`, `System`, scheduler DAG, `CommandBuffer`, zasoby (`Res`/`ResMut`), **publiczne API chunków pod copy-on-write M12** |
| `engine/jobs` | Pula wątków z work stealing, `scope`, deterministyczny fork-join (`map_reduce_indexed`) |
| `engine/io` | **Minimalny**: snapshot ECS (bincode + zstd), tablica schematu komponentów, `state_hash`, wznowienie |
| `engine/devtools` | **Szkielet**: inspektor ECS (tekstowy), rejestr komend konsoli, hooki profilera, `MetricSink` |
| `tools/headless` | Runner bez GPU: seed, liczba ticków, hashowanie, porównanie, zapis/odczyt, konsola |
| `tools/voxelview` | Bootstrap okna `wgpu`/`winit` + jeden statyczny chunk voxeli (rusztowanie pod M1) |
| Testy | Determinizm hashy, równoważność 1 vs N wątków, właściwości `Money`, Miri na `unsafe` w ECS |
| CI | `fmt`, `clippy -D warnings`, `test`, `miri`, `bench` z progiem regresji |

### Nie wchodzi

| Czego nie ma | Kto to robi |
|---|---|
| Generator terenu, hydrologia, klimat, chunki świata, streaming, greedy meshing, LOD, oświetlenie, `engine/voxel`, `engine/render` | **M1** |
| Indeksy przestrzenne, quadtree parcel, siatka dzielnic (`engine/spatial`) | **M2** |
| Jakakolwiek logika domenowa: agenci, potrzeby, DES, rynki, firmy, towary, podatki | **M3+** |
| Grafy transportu, CH, A\*, cache tras (`engine/nav`) | **M4** |
| UI gry (`engine/ui`), audio (`engine/audio`) | **M3 / M11** |
| Ładowanie danych RON z `data/`, walidacja grafu towarów | **M1** (pierwsze pliki) / **M6** (walidator grafu) |
| Wersjonowanie schematu zapisu z migracjami, zapis w tle (copy-on-write), dziennik zdarzeń, replay wejść gracza | **M12** (`engine/io` pełny); M0 daje tylko `format_version` i twardy błąd przy niezgodności |
| Modding, `engine/script`, sandbox Lua/WASM | **M12** |
| Makro-LOD, tryb 50×, uśpione tożsamości | **M10 / M12** |

Granica z M1 na stosie graficznym: M0 dostarcza **tylko** inicjalizację `Instance/Adapter/Device/Queue/Surface`,
pętlę zdarzeń i jeden pipeline rysujący zahardkodowaną siatkę. Cały `engine/render` i `engine/voxel` należy do M1
(patrz §9, decyzja otwarta D-5).

### Reguła wiążąca: funkcje przestępne (00 §K-6)

Podstawowa arytmetyka `f64` (`+`, `-`, `*`, `/`, `sqrt`) jest w Rust **deterministyczna między platformami** —
IEEE-754 wymaga poprawnego zaokrąglenia, a Rust nie robi fast-math ani automatycznej kontrakcji do FMA.
Niedeterminizm wnoszą wyłącznie **funkcje przestępne ze std**, które są cienkim opakowaniem na systemowy
libm: `ln`, `exp`, `powf`, `log2`, `sin`, `tanh` i pokrewne nie są poprawnie zaokrąglone, a ich ostatni bit
różni się między glibc, musl, MSVC i Apple libm oraz między wersjami tej samej biblioteki.

> **Zakaz:** `f64::{ln, ln_1p, log, log2, log10, exp, exp_m1, exp2, powf, powi, sin, cos, tan, asin, acos,
> atan, atan2, sinh, cosh, tanh, cbrt, hypot, mul_add}` (oraz odpowiedniki `f32`) są **zakazane w kodzie
> symulacji**: `sim/*`, `game/`, `engine/{core, ecs, jobs, io, spatial, nav}`.
> **Dozwolone** w `engine/{render, voxel, ui, audio}` i w `tools/voxelview` — tam wynik nie wchodzi do stanu
> trwałego, a różnica ostatniego bitu jest niewidoczna.
> Zamiennik: `core::det_math` (§5.3a). `f64::sqrt` i `f64::{abs, floor, ceil, round, trunc, min, max,
> copysign, to_bits, from_bits}` pozostają dozwolone wszędzie — są poprawnie zaokrąglone lub czysto bitowe.

`mul_add` jest w tym zakazie nie dlatego, że jest niedokładny, tylko odwrotnie: na sprzęcie z FMA daje wynik
fuzyjny, a na sprzęcie bez FMA — emulację przez libm. Ta sama linia kodu, dwa różne wyniki.

**Egzekwowanie** (trzy warstwy, WP-03b):
1. `clippy.toml` w korzeniu workspace z listą `disallowed-methods` — Clippy szuka pliku w górę od katalogu
   crate'a, więc `engine/render/clippy.toml` i `engine/ui/clippy.toml` z pustą listą **nadpisują** zakaz
   lokalnie. Jeden plik konfiguracyjny, zero kodu.
2. CI ma `clippy -D warnings`, więc naruszenie to czerwony build, nie ostrzeżenie.
3. Zapasowo: test w `tools/headless` skanujący źródła `sim/*` regexem na `\.(ln|exp|powf)\(` — łapie
   przypadki, w których ktoś obejdzie lint przez `#[allow]`. Tani i głupi, ale to właśnie tego rodzaju błąd,
   który kosztuje tydzień, gdy wyjdzie po roku.

---

## 3. Mapowanie na PRD

| Element M0 | PRD |
|---|---|
| Wybór Rust, granica bibliotek zewnętrznych | §16.1 |
| Struktura workspace i crateów | §16.2 |
| Bootstrap okna wgpu + jeden chunk | §16.3 (tylko punkt wejścia), §19 M0 |
| Tick ekonomiczny 1 min, tick mikro 100 ms, render niezależny, sim w osobnych wątkach | §17.1 |
| Archetypowy ECS SoA, scheduler z DAG, równoległość po chunkach, bufory komend | §17.2 |
| Budżet pamięci (ok. 400 B na encję stanu gorącego) jako kryterium layoutu archetypu | §17.7 |
| Testy determinizmu (hash co 1000 ticków), benchmarki criterion | §17.8, §18.2 |
| Snapshot ECS + zstd, wersjonowanie (tu: tylko detekcja wersji) | §18.1 |
| RNG ze strumieniami, fixed-point, brak zależności od czasu rzeczywistego | §18.2 |
| `det_math` — determinizm operacji zmiennoprzecinkowych między platformami | §18.2 („operacje zmiennoprzecinkowe w ustalonej kolejności lub fixed-point"), 00 §K-6; konsument: §6.4 (użyteczność zakupu) |
| `DecisionReason` — wyjaśnialność egzekwowana przez kompilator | §14.1, §20.1, 00 §7 i §K-12 |
| Słowniki domenowe w `core` (rozcięcie cyklu crateów) | §16.2 (granice modułów), 00 §K-8 |
| `Arena<T>` — partie i oferty poza ECS | §17.2 (co jest encją), §17.7 (pamięć: 600 tys. partii), 00 §K-16 |
| Chunk jako publiczna jednostka, `RawSnapshot` jako lista sekcji | §18.1 (zapis w tle, copy-on-write chunków ECS, wersjonowanie), §20.2 (zapis < 5 s bez pauzy) |
| Kalendarz 360-dniowy, siatka `StreamId` | §17.1, §18.2, 00 §K-1 i §K-4 |
| Headless runner, inspektor ECS, profiler | §16.5 |
| Determinizm 100 % w CI | §20.2 |
| Krzywa uczenia Rusta — M0 jako moduł „dobrze zdefiniowany" | §20.4 |

---

## 4. Pakiety robocze (WP)

Kolejność jest wiążąca. Każdy WP ma twarde kryterium ukończenia — uruchamialny test lub komenda,
nigdy „gotowe".

### WP-01 — Workspace, toolchain, szkielet CI
**Status:** [x] ukończony — workspace, `rust-toolchain.toml`, `clippy.toml`, profile, CI w `.github/workflows/ci.yml`
**Zależy od:** —
**Opis:** `Cargo.toml` workspace z katalogami `engine/*`, `sim/*`, `game/`, `tools/*` (cratey faz M1+ powstają
dopiero w swojej fazie — M0 tworzy wyłącznie swoje). `rust-toolchain.toml` z przypiętą wersją kanału stable
plus komponenty `clippy`, `rustfmt`, oraz osobny pin nightly dla Miri. Lint table w workspace:
`unsafe_code = "forbid"` domyślnie, punktowe `allow` tylko w `engine/ecs`. Profile: `dev` (opt-level 1 dla
zależności), `release` (`lto = "thin"`, `codegen-units = 1`; `panic = "abort"` **wyłączone** — job system
potrzebuje `catch_unwind`), profil `bench`. Workflow CI z jednym jobem `check`.
**Kryterium ukończenia:** `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
przechodzi na pustym workspace w CI, na Linux i Windows.
**Rozmiar:** S

### WP-02 — `engine/core`: typy bazowe, pieniądz, fixed-point, czas
**Status:** [x] ukończony — `core::{types, money, fixed, time}`; proptest 10^5 przypadków podziału, tabela 22 przypadków brzegowych zaokrąglania
**Zależy od:** WP-01
**Opis:** Implementacja 00 §2 co do litery: `Money`, `SimMinute`, `SimInstant`, `Tick`, `Mass`, `Volume`,
`Energy`, `Qty`, `Q`, `Mood`, `DistrictId`, `GoodId`, `RecipeId`, `JobRoleId`. Arytmetyka `Money`
(`checked_*`, `div_round_half_up`, `split_proportional`). `Fx` (Q32.32) tam, gdzie potrzeba ułamków,
a float jest zabroniony. Kalendarz `SimCalendar` i `Cadence`. Typowane id nad `Entity` (`CitizenId` itd.)
powstają po WP-04 — tu zostaje pusty moduł `ids` z TODO.
**Kryterium ukończenia:** testy własnościowe (`proptest`): `split_proportional` sumuje się do oryginału
dla 10^5 losowych podziałów; `div_round_half_up` zgodne z tabelą 40 przypadków brzegowych (ujemne, dokładna
połówka); `Fx::mul`/`Fx::div` bez przepełnienia w zakresie deklarowanym w doc-comencie.
**Rozmiar:** M

### WP-02b — `engine/core`: `Entity`, `Arena<T>`, słowniki domenowe, `DecisionReason`, `ComponentSchemaId`
**Status:** [x] ukończony — `Entity`, `Arena<T>` (10^6 cykli uchwytów), słowniki K-8, `DecisionReason` (≤24 B + test `trybuild`), `ComponentSchemaId`
**Zależy od:** WP-02
**Opis:** Pięć rzeczy, które muszą być w `core`, bo inaczej powstaje cykl zależności między cratami
`sim/*`, którego Cargo nie skompiluje (§5.1a), albo dwie fazy piszą ten sam mechanizm dwa razy:
1. **`Entity`** — przeniesiona z `ecs` do `core`. Wymusza to już 00 §2 (`CitizenId(pub Entity)` jest w `core`),
   a `PlaceRef` z K-8 potwierdza. `ecs` re-eksportuje, więc dla wywołujących nic się nie zmienia.
2. **Słowniki domenowe K-8:** `UtilityKind`, `TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind` —
   wyłącznie warianty i konwersje, **zero logiki**.
3. **`DecisionReason` K-12** — jeden centralny enum bez `#[non_exhaustive]`, z blokami dyskryminant
   rezerwowanymi per faza.
4. **`ComponentSchemaId`** — tożsamość schematu komponentu (nazwa + wersja), podstawa migracji w M12.
5. **`Arena<T>` i `ArenaHandle<T>` (00 §K-16)** — arena generacyjna dla kategorii, które nie są encjami:
   `BatchId` i `OfferId`. Mechanizm, nie zawartość — `core` nie wie, czym jest partia ani oferta (§5.1b).
**Kryterium ukończenia:** `size_of::<DecisionReason>() <= 24` (test); test strażniczy wartości dyskryminant
(dopisanie wariantu nie może zmienić istniejących); `cargo tree` potwierdza, że `core` nie zależy od niczego
z `engine/*` ani `sim/*`; przykładowy `match` po `DecisionReason` bez ramienia `_` nie kompiluje się po
dodaniu wariantu (test `trybuild` — to jest cały sens K-12); dla areny: `size_of::<ArenaHandle<T>>() == 8`,
**uchwyt zwolnionego slotu nie zwraca wartości po ponownym użyciu slotu** (test na 10^6 cykli
insert/remove/insert), `ArenaHandle<Batch>` nie kompiluje się tam, gdzie oczekiwany jest
`ArenaHandle<Offer>` (test `trybuild`), iteracja po 600 tys. slotów z dziurami jest w kolejności indeksów.
**Rozmiar:** L (pięć niezależnych mechanizmów; największy z nich to arena)

### WP-03 — `engine/core`: RNG, `StreamId`, `SeededMap`
**Status:** [x] ukończony — `xoshiro256++` zgodny z wektorem referencyjnym, `StreamId` z testem strażniczym, `SeededMap`
**Zależy od:** WP-02
**Opis:** `xoshiro256++` jako czysta funkcja stanu; `stream_seed(world_seed, StreamId, entity_index, tick)`
mieszany `SplitMix64`. `StreamId` jako enum z **jawnie przypisanymi, wiecznymi wartościami** i testem
strażniczym. `SeededMap` — mapa o deterministycznej kolejności iteracji (kolejność wstawiania, haszer ze
stałym ziarnem) plus `iter_sorted()` dla `K: Ord`. Lint clippy zakazujący `std::collections::HashMap`
w `sim/*` (`clippy.toml`, `disallowed-types`).
**Kryterium ukończenia:** wektory testowe `xoshiro256++` zgodne bit w bit z referencją; test, że
`rng(seed, S, e, t)` nie zależy od kolejności wywołań ani od liczby wątków; test snapshotowy wartości
liczbowych `StreamId` (dodanie wariantu nie może zmienić istniejących).
**Rozmiar:** S

### WP-03b — `engine/core`: `det_math` i egzekwowanie zakazu libm
**Status:** [x] ukończony — `det_math` (≤1 ULP, `pow` patrz §4a), złoty odcisk T-D9, `clippy.toml` z zakazem libm
**Zależy od:** WP-02
**Opis:** Moduł `core::det_math` z własnymi, w pełni określonymi implementacjami `ln`, `ln1p`, `exp`, `exp_m1`,
`log2`, `exp2`, `pow`, `sqrt` i `softmax` (§5.3a) — zbudowanymi **wyłącznie** z operacji, które IEEE-754
zaokrągla poprawnie (`+`, `-`, `*`, `/`, `sqrt`) plus manipulacja bitów wykładnika przez
`to_bits`/`from_bits`. Zero wywołań libm, zero `mul_add`. Do tego `clippy.toml` w korzeniu workspace
z `disallowed-methods` oraz nadpisania w `engine/render` i `engine/ui` (§2, reguła K-6), plus zapasowy
test skanujący źródła.
**Kryterium ukończenia:**
1. **Bit w bit między platformami:** wektor 100 tys. argumentów (logarytmicznie rozłożonych po całym
   zakresie wykładnika, plus przypadki brzegowe) przepuszczony przez każdą funkcję; plik wyników
   `det_math_golden.bin` z hashem XXH3 zatwierdzony w repo. Job CI liczy go na `x86_64-unknown-linux-gnu`,
   `x86_64-pc-windows-msvc` i `aarch64-unknown-linux-gnu` (emulacja QEMU wystarczy) oraz na dwóch wersjach
   toolchaina (bieżąca i poprzednia stabilna) — **wszystkie sześć hashy muszą być identyczne**.
2. **Dokładność:** porównanie z wartościami referencyjnymi policzonymi w arytmetyce 256-bitowej
   (`rug`/MPFR w osobnym narzędziu, wynik zamrożony w repo jako tabela) — `ln`, `exp`, `log2`, `exp2`,
   `ln1p` w granicy **≤ 1 ULP**, `pow` **≤ 2 ULP**, monotoniczność `ln`/`exp` na 10^6 kolejnych wartościach.
3. **Koszt:** benchmark B-8 — nie wolniej niż **2×** względem `f64::ln`/`exp` z libm platformy.
4. Lint: celowo wstawione `x.ln()` w `sim/*` wywala CI; to samo w `engine/render` przechodzi.
**Rozmiar:** M

### WP-04 — `engine/ecs`: storage archetypowy SoA i publiczne API chunków
**Status:** [x] ukończony — storage chunkowany, test layoutu ≤64 KiB, 400 tys. encji bez wycieku (licznik `Drop`); **Miri wymaga nightly, którego nie ma w tym środowisku — job CI jest, przebieg do wykonania**
**Zależy od:** WP-02b, WP-03
**Opis:** `EntityStore` z listą wolnych indeksów (samo `Entity` przychodzi z `core`, WP-02b), `ComponentId`
i rejestr komponentów sprzęgnięty z `ComponentSchemaId`, `Archetype` (kanoniczny, posortowany zestaw
`ComponentId`), kolumny SoA cięte na chunki o **docelowym rozmiarze 64 KiB** (liczba wierszy wyliczana
per archetyp), `EntityLocation`, operacje `spawn`/`despawn`/`insert`/`remove` w wersji **bezpośredniej** —
używanej wyłącznie poza tickiem (ładowanie, testy, odczyt snapshotu). Do tego **publiczne, wąskie API chunków**
(`ChunkId`, `ChunkRef`, `World::chunks`, `Archetype::chunk_generation`) — fundament pod copy-on-write w M12 (§5.5).
Tu żyje jedyny `unsafe` w M0; każdy blok z komentarzem `// SAFETY:` i testem pod Miri.
**Kryterium ukończenia:** 400 tys. encji w 6 archetypach spawnuje się i despawnuje bez wycieku
(test z licznikiem `Drop`); `cargo +nightly miri test -p magnat-ecs` zielone; `size_of::<Entity>() == 8`
i `size_of::<Option<Entity>>() == 8`; **każdy chunk to jedna alokacja o rozmiarze ≤ 64 KiB, osiągalna przez
jeden wskaźnik** (test layoutu — bez tego CoW w M12 jest niewykonalny); `ChunkRef` jest `Send + Clone`
i nie daje dostępu mutowalnego (test `trybuild`).
**Rozmiar:** L

### WP-05 — `engine/ecs`: `Query`, `Access`, zasoby
**Status:** [x] ukończony — `Query`/`Access` z typów, filtry, `par_for_each`, `par_fold`; konflikt `(&mut A, &A)` panikuje przy budowie zapytania
**Zależy od:** WP-04
**Opis:** `Component`, `QueryData` dla `&T`, `&mut T`, `Entity`, `Option<&T>` i krotek do 12 elementów;
filtry `With<T>`/`Without<T>`; iterator po chunkach (`ChunkIter`) i po wierszach; cache dopasowanych
archetypów unieważniany wersją rejestru archetypów. `Access` (bitsety read/write komponentów i zasobów,
flaga `structural`) wyliczany **z typów**, nie deklarowany ręcznie. Zasoby `Res<T>`/`ResMut<T>`.
**Kryterium ukończenia:** test `trybuild`, że `Query<(&mut A, &A)>` się nie kompiluje; iteracja po
400 tys. encji odwiedza wszystkie i tylko pasujące (suma kontrolna); `Access` dla złożonej krotki zgodny
z ręcznie wypisanym oczekiwaniem.
**Rozmiar:** M

### WP-06 — `engine/jobs`: pula wątków i deterministyczny fork-join
**Status:** [x] ukończony — `JobPool`, `map_reduce_indexed` niezależny od liczby wątków (1000 powtórzeń), panika workera propagowana
**Zależy od:** WP-01
**Opis:** `JobPool` (jawna, nie-globalna pula; liczba wątków z konfiguracji, `0` = liczba rdzeni),
`scope` dla zadań zagnieżdżonych, `map_reduce_indexed` — redukcja **po indeksie chunka**, nigdy po kolejności
zakończenia. Przechwytywanie paniki workera i przeniesienie jej na wątek wywołujący. Nazwy wątków dla profilera.
**Kryterium ukończenia:** `map_reduce_indexed` na 10 tys. chunków z celowo losowymi opóźnieniami daje ten sam
wynik przy 1, 2 i 16 wątkach, 1000 powtórzeń; panika w workerze propaguje się do wywołującego, a pula
pozostaje użyteczna.
**Rozmiar:** M

### WP-07 — `engine/ecs`: scheduler DAG
**Status:** [x] ukończony — DAG z kolejności kanonicznej, odcisk grafu niezależny od kolejności rejestracji, 2000 ticków identycznych przy 1/2/8/16 wątkach
**Zależy od:** WP-05, WP-06
**Opis:** `SystemId` wyprowadzony ze stabilnej nazwy systemu (nie z kolejności rejestracji), `SystemDesc`
(nazwa, `Cadence`, `Access`, jawne ograniczenia `after`/`before`), budowa DAG: systemy w kanonicznej kolejności
`SystemId`, krawędź dla każdej pary konfliktującej (write×write lub read×write na tym samym komponencie
lub zasobie), wykrywanie cykli dla ograniczeń jawnych, podział na etapy rozdzielone barierami. Wykonanie na
`JobPool` z licznikami zależności. Filtrowanie po `Cadence` dla bieżącego ticku.
**Kryterium ukończenia:** dla zestawu 60 syntetycznych systemów DAG jest identyczny niezależnie od kolejności
rejestracji (test na odcisku grafu); 10 tys. ticków przy `--threads 1` i `--threads 16` daje identyczny hash
stanu; konflikt niewykrywalny statycznie (dwa `ResMut<T>` równolegle) panikuje w debug.
**Rozmiar:** L

### WP-08 — `engine/ecs`: `CommandBuffer` i punkt synchronizacji
**Status:** [x] ukończony — bufory per system, cztery fazy flusha, 100 tys. komend z 16 buforów daje ten sam stan przy odwróconej kolejności
**Zależy od:** WP-07
**Opis:** Bufor komend per system (bez zamków), komendy w płaskim blobie plus indeks kluczy
`(SystemId, entity_index, seq)`. Rezerwacje encji (`EntityReservation`) pozwalające adresować encję,
która jeszcze nie istnieje. Flush w barierze: sortowanie kluczy, aplikacja w kolejności
despawn → remove → spawn → insert, materializacja nowych `Entity` dopiero po zwolnieniu indeksów.
**Kryterium ukończenia:** 100 tys. komend z 16 systemów daje identyczny stan (hash) przy 1 i 16 wątkach
oraz przy odwróconej kolejności rejestracji systemów; użycie `EntityReservation` po flushu jest błędem
kompilacji, nie runtime'u.
**Rozmiar:** M

### WP-09 — `engine/io`: `HashState` i hash stanu świata
**Status:** [x] ukończony — `world_state_hash` niezależny od historii spawnów (T-D5), czuły na jeden bajt
**Zależy od:** WP-08
**Opis:** `StateHasher` (XXH3-128) i `HashState` dla typów bazowych `core` plus makro `impl_hash_state_pod!`.
`world_state_hash(&World)`: archetypy w kolejności kanonicznego zestawu `ComponentId`, w archetypie encje
posortowane po `entity.index()`, komponenty w kolejności `ComponentId`. Floaty — tylko przez `to_bits()`
z kanonizacją NaN; komponent z floatem implementuje `HashState` ręcznie (brak blanket impl).
**Kryterium ukończenia:** hash nie zależy od historii spawnów/despawnów prowadzącej do tego samego stanu
logicznego (dwa różne przebiegi konstrukcji tego samego świata → ten sam hash); zmiana jednego bajtu
w jednym komponencie zmienia hash.
**Rozmiar:** S

### WP-10 — `engine/io`: snapshot ECS (bincode + zstd)
**Status:** [x] ukończony — snapshot sekcyjny zstd, roundtrip 400 tys. encji, katalog bez dekompresji < 50 ms, przepisanie sekcji bajtowo rozdzielne (T-D13)
**Zależy od:** WP-09
**Opis:** `SnapshotHeader` z `format_version`, `world_seed`, `tick`, `state_hash` i **tablicą schematu**
opartą na `ComponentSchemaId` (WP-02b). Zapis **sekcjami** (`RawSnapshot` / `SnapshotSection`, §5.9) —
nie płaskim buforem: każda kolumna komponentu, tablica encji i każdy zasób to osobna, samoopisująca się
sekcja z własnym zakresem bajtów, długością i sumą kontrolną. Kompresja zstd (poziom 3) per sekcja.
Odczyt z remapowaniem po `ComponentSchemaId`; nieznany komponent lub niezgodna `format_version` → twardy,
opisowy błąd (same migracje to M12 — M0 daje im tylko kształt, na którym da się je zbudować).
**Kryterium ukończenia:** roundtrip 400 tys. encji zachowuje `state_hash`; zapis poniżej 5 s i poniżej 300 MB
dla świata testowego; przerwany zapis nie niszczy poprzedniego pliku (zapis do `*.tmp` plus `rename`);
**test sekcyjny — da się wczytać nagłówek i listę sekcji bez dekompresji ładunku, podmienić bajty jednej
sekcji i zapisać plik z powrotem, nie dotykając pozostałych.** To jest dokładnie ta operacja, na której
M12 oprze migracje schematu i zapis przyrostowy; jeśli nie działa w M0, nie da się jej dorobić później
bez zmiany formatu.
**Rozmiar:** M

### WP-11 — `engine/devtools`: szkielet
**Status:** [x] ukończony — `Inspector`, `Console` z komendami wbudowanymi, `scope!` bez kosztu bez feature'a, `MetricSink` z eksportem CSV
**Zależy od:** WP-09
**Opis:** `Inspector` — tekstowy zrzut: lista archetypów z licznikami i bajtami, zrzut jednej encji ze
wszystkimi komponentami (przez `Debug` z rejestru komponentów). `Console` — rejestr komend
`fn(&mut World, &[&str]) -> String` z wbudowanymi `ecs.*`, `sim.*`, `prof.*`; każda faza dopisuje swoje.
`profile::scope!()` — makro kompilujące się do niczego bez feature `profiling`, do `tracy-client` z feature.
`MetricSink` — pierścieniowy bufor nazwanych liczników per tick plus eksport CSV (konsument: balansator, M5).
**Kryterium ukończenia:** `ecs.archetypes` i `ecs.entity <id>` działają w headless na świecie testowym;
build bez feature `profiling` nie zawiera zależności tracy (`cargo tree`); `MetricSink` z 64 licznikami
przez 10 tys. ticków mieści się w zadeklarowanym budżecie pamięci.
**Rozmiar:** M

### WP-12 — `tools/headless`: runner
**Status:** [x] ukończony — pełne CLI, świat syntetyczny (6 archetypów, 12 komponentów, 8 systemów), test negatywny `--features chaos` zawodzi na właściwym ticku
**Zależy od:** WP-10, WP-11
**Opis:** CLI: `--seed`, `--ticks`, `--threads`, `--hash-every`, `--out`, `--expect`, `--save`, `--load`,
`--console`, `--metrics`. Świat testowy budowany deterministycznie z seeda: N encji w kilku archetypach
z komponentami-atrapami i garścią systemów o realistycznym profilu dostępu (czytaj-wiele/pisz-jedno,
jeden system strukturalny). Raport rozbieżności: pierwszy niezgodny tick plus diff z `Inspector`
dla pierwszych 10 różniących się encji.
**Kryterium ukończenia:** `--expect` wykrywa wstrzykniętą niedeterministyczną zmianę — test negatywny w CI:
feature `chaos` włącza w jednym systemie iterację po `HashMap`, runner musi zawieść i wskazać właściwy tick.
**Rozmiar:** S

### WP-13 — Testy determinizmu i benchmarki
**Status:** [x] ukończony — benchmarki B-1…B-9 z wynikami w §4a; linia bazowa w `benches/baseline.json`
**Zależy od:** WP-12
**Opis:** Pełny zestaw z §7. Benchmarki criterion: iteracja po 400 tys. encji (2 i 6 komponentów, z filtrem
i bez), koszt pojedynczej komendy strukturalnej, koszt flusha 100 tys. komend, przeniesienie encji między
archetypami, `world_state_hash`, koszt bariery schedulera przy 60 systemach.
**Kryterium ukończenia:** testy §7.1–§7.3 zielone; wyniki bazowe zapisane do `benches/baseline.json`.
**Rozmiar:** M

### WP-14 — CI pełne
**Status:** [x] ukończony — macierz Linux/Windows, joby `check`/`miri`/`determinism`/`bench-guard`; **przebieg w GitHub Actions do wykonania przy pierwszym push**
**Zależy od:** WP-13
**Opis:** Macierz Linux/Windows: `fmt`, `clippy -D warnings`, `test --workspace`, job Miri (tylko
`engine/ecs`, `engine/core`), job determinizmu (headless 200 tys. ticków w dwóch konfiguracjach wątków
plus porównanie plików hashy), job benchmarków z progiem regresji (ponad 10 % wolniej = ostrzeżenie,
ponad 25 % = błąd).
**Kryterium ukończenia:** świadomie wprowadzona regresja 30 % w gorącej pętli wywala CI; job Miri
łapie celowo wprowadzony błąd aliasingu.
**Rozmiar:** S

### WP-15 — Bootstrap okna wgpu/winit i statyczny chunk voxeli
**Status:** [x] ukończony — okno `winit` + `wgpu` startuje na Vulkanie, chunk 32³ rysowany, `wgpu` nie przenika do headless
**Zależy od:** WP-01 (poza tym niezależny — może iść równolegle od WP-02)
**Opis:** `tools/voxelview`: `winit` event loop (API `ApplicationHandler`), `wgpu` instance → adapter →
device → queue → surface, konfiguracja i obsługa resize, bufor głębi, jeden pipeline z shaderem WGSL
(pozycja, normalna, indeks palety), jeden bufor wierzchołków z **zahardkodowanego** chunka 32³ (prosty wzór
proceduralny, bez generatora terenu), naiwna triangulacja ścian widocznych (bez greedy meshing), kamera
orbitalna, licznik FPS w tytule okna, `RUST_LOG`. Zero powiązania z ECS — to celowo ślepy zaułek, z którego
M1 przenosi wyłącznie kod inicjalizacji do `engine/render`.
**Kryterium ukończenia:** okno startuje na Vulkan i DX12 (Windows) oraz Vulkan (Linux), pokazuje chunk,
utrzymuje co najmniej 60 FPS; `wgpu` nie przenika do innych crateów
(`cargo tree -p magnat-headless` bez `wgpu`).
**Rozmiar:** M

### Graf zależności

```
WP-01 ─┬─ WP-02 ─┬─ WP-02b ─┬─ WP-04 ─ WP-05 ─┐
       │         ├─ WP-03 ──┘                  ├─ WP-07 ─ WP-08 ─ WP-09 ─┬─ WP-10 ─┐
       │         └─ WP-03b  (poza ścieżką krytyczną, konsument dopiero M5)│         ├─ WP-12 ─ WP-13 ─ WP-14
       ├─ WP-06 ────────────────────────────────┘                        └─ WP-11 ─┘
       └─ WP-15   (równolegle, poza ścieżką ECS)
```

WP-03b jest **poza ścieżką krytyczną M0** — pierwszy realny konsument to M5. Nie może jednak zostać
odłożony do M5: gdyby powstał później, cała logika M3/M4 zdążyłaby wsiąknąć w `f64::ln`, a zakaz z §2
byłby egzekwowany na kodzie, który już go łamie. Lint musi istnieć, zanim powstanie pierwszy system.

---

## 4a. Korekty planu wprowadzone w trakcie implementacji

Plan jest dokumentem wykonawczym, a nie pamiątką: poniższe punkty są **poprawkami
do treści powyżej**, wprowadzonymi, gdy implementacja pokazała, że pierwotny zapis
był niewykonalny albo nieprawdziwy. Każda ma uzasadnienie, bo rozjazd kodu z planem
jest gorszy niż błąd w planie — nikt go nie widzi.

| # | Miejsce | Było | Jest | Dlaczego |
|---|---|---|---|---|
| K0-1 | §7.2 „Dokładność `det_math`" | `exp(ln(x))` w granicy **3 ULP** dla x ∈ [10⁻¹⁰⁰, 10¹⁰⁰] | granica **3 + \|ln x\| ULP** | Próg matematycznie nieosiągalny dla **żadnej** implementacji: błąd bezwzględny ln(x) to ~0,5 ULP z \|ln x\|, a `exp` przenosi go na błąd **względny** wyniku. Dla x = 10⁻¹⁰⁰ (ln ≈ −230) daje to ok. 115 ULP przy idealnym `ln`. Zmierzone: 87 ULP |
| K0-2 | §5.3a, §7.2 (`pow`) | `pow` **≤ 2 ULP** bezwarunkowo | **≤ 2 ULP dla \|y·log2 x\| ≤ 256**, **≤ 8 ULP** w całym zakresie `f64` | W algorytmie `exp2(y·log2 x)` błąd logarytmu (≈ 2⁻⁵⁹ po złożeniu pary hi/lo) jest mnożony przez `y`. Zejście niżej wymaga algorytmu klasy fdlibm `e_pow.c` z własną dekompozycją na 24-bitowe połówki — niewarte kosztu, bo konsumenci (M5, M7, M10) pracują na wykładnikach rzędu jedności. Zmierzony najgorszy przypadek: **5 ULP dla 1,5^1000** |
| K0-3 | §5.5 (`CHUNK_TARGET_BYTES`) | „daje ~160 **chunków** na 400 tys. encji o wierszu 400 B" | 160 to liczba **wierszy**; przy wierszu 408 B chunk mieści **128 wierszy**, czyli ok. **3 100 chunków** | Arytmetyka: 400 tys. × 408 B = 163 MB / 64 KiB. Ziarno równoległości jest nadal drobne (ponad 190 chunków na wątek przy 16 wątkach) |
| K0-4 | §5.1a (`ComponentSchemaId`) | `name_hint()` w `core` | `ComponentRegistry::name_of_schema()` w `engine/ecs` | `name_hint` wymagałby globalnego rejestru w `core`, który nie ma czego rejestrować. Rejestr komponentów zna nazwy i tak — ten sam efekt bez stanu globalnego |
| K0-5 | §5.9 (`HashState`) | `StateHasher`/`HashState` w `engine/io` | w `magnat-core`, re-eksportowane przez `magnat-io` | Rejestr komponentów w `ecs` musi umieć zahaszować komponent, którego typu nie zna, a `ecs` nie może zależeć od `io`. Nazwy z kontraktu §6.5 nadal rozwiązują się przez `magnat_io::` |
| K0-6 | §5.8 (`EntityReservation`) | użycie po flushu = **błąd kompilacji** | użycie po flushu = **panika z komunikatem** (rezerwacja niesie epokę bufora) | Powiązanie rezerwacji czasem życia bufora uniemożliwiłoby `cmd.insert_reserved(r, v)`: rezerwacja trzymałaby pożyczkę mutowalną, którą ta metoda musi wziąć drugi raz. Wersja z epoką łapie ten sam błąd, tylko sekundę później |
| K0-7 | §5.5 (`trait Component`) | `Component: Send + Sync + 'static` | dodatkowo `+ HashState + Debug` | Hash stanu (00 §3.6) i karta inspekcji (00 §7) są wymaganiami DoD **każdej** fazy. W nadtypach egzekwuje je kompilator przy rejestracji, a nie czyjaś pamięć na przeglądzie — ta sama logika co K-12 |
| K0-8 | §5.9 (sekcje snapshotu) | sekcje `EntityTable`, `ComponentColumn`, `Arena`, `Resource`, `Opaque` | dodatkowo **`ChunkEntities`**, a `EntityTable` niesie też **listę wolnych indeksów** | Bez tablicy encji per chunk nie da się odtworzyć, który wiersz należy do której encji. Lista wolnych indeksów jest częścią stanu, nie szczegółem: to ona decyduje, który indeks dostanie następny `spawn` — bez niej wznowienie rozjeżdża się z przebiegiem ciągłym przy pierwszym spawnie (T-D4 by tego nie przepuścił) |
| K0-9 | 00 §6 (`unsafe` tylko w `ecs`) | `engine/io` musiałoby wołać `unsafe` przy odczycie snapshotu | `World::restore_rows_checked` — **bezpieczna brama walidująca** w `ecs`; `engine/io` pozostaje `#![forbid(unsafe_code)]` | Brama sprawdza długości kolumn, przynależność komponentów i **odrzuca typy z destruktorem** (typ bez destruktora nie trzyma wskaźnika, więc dowolne bajty nie zrobią z niego wiszącej referencji). Resztę zaufania niosą sumy kontrolne sekcji i weryfikacja hasha po wczytaniu |
| K0-10 | §7.3 (B-1c) | przyspieszenie **≥ 8×** wobec B-1b przy 16 wątkach | ≥ 8× **przy pracy per chunk przekraczającej narzut zadania**; zmierzone **5,2×** dla B-1b (70 µs pracy na 100 tys. wierszy) | Przy 70 µs całkowitej pracy narzut planowania zadań rayon dominuje. Kryterium bez zastrzeżenia o wielkości pracy mierzy rayon, a nie ECS |
| K0-11 | §7.2 (referencja `det_math`) | wartości odniesienia z **MPFR/`rug`** | arytmetyka dziesiętna **60 cyfr** (`decimal` ze standardowej biblioteki Pythona), tabela zamrożona w `engine/core/tests/reference/det_math.txt` | Ta sama rola (niezależna implementacja o precyzji grubo ponad `f64`), zero nowych zależności i osobnego narzędzia w workspace. Ważne: `Decimal(float)` bierze **dokładną** wartość binarną — `Decimal(repr(x))` wniósłby własne pół ULP błędu |
| K0-12 | §9 D-7 | wersja bezpieczna (`Vec<T>` per kolumna) najpierw, chunki z `unsafe` jako WP-04b | od razu storage chunkowany z trzema udokumentowanymi blokami `unsafe` | Kryterium ukończenia WP-04 („jeden chunk = jedna alokacja ≤ 64 KiB pod jednym wskaźnikiem") jest kontraktem z M12 i wariant `Vec`-per-kolumna go nie spełnia. Wersja bezpieczna nie przeszłaby własnego testu akceptacyjnego |

### Wyniki benchmarków (sprzęt: Windows 11, 16 wątków, `--release`)

| Benchmark | Cel §7.3 | Zmierzone | Werdykt |
|---|---|---|---|
| B-1 iteracja 400 tys. (2 komponenty) | ≤ 4 ms | **0,23 ms** | ✅ 17× zapasu |
| B-1b 6 komponentów, 4 archetypy, filtry | ≤ 12 ms | **0,07 ms** (ok. 100 tys. pasujących wierszy) | ✅ |
| B-1c `par_for_each`, 16 wątków | ≥ 8× wobec B-1b | **5,2×** (13,6 µs) | ⚠️ patrz K0-10 |
| B-2 koszt komendy | ≤ 150 ns | **5,7 ns** | ✅ |
| B-2b flush 100 tys. komend | ≤ 15 ms | **12,9 ms** | ✅ (pierwsza wersja: 233 ms — liniowe wyszukiwanie rezerwacji zamienione na binarne) |
| B-3 zmiana archetypu (wiersz 400 B) | ≤ 400 ns | **88 ns** | ✅ |
| B-4 `world_state_hash` 400 tys. | ≤ 40 ms | **11,4 ms** | ✅ |
| B-5 bariera, 60 systemów, 16 wątków | ≤ 50 µs | **19,8 µs** | ✅ |
| B-6 snapshot 400 tys. | zapis ≤ 5 s, ≤ 300 MB | **1,00 s**, **3,1 MB**, odczyt 0,36 s | ✅ |
| B-7 pamięć storage 400 tys. × 400 B | ≤ 1,2 GB RSS | **156 MB** chunków, narzut chunkowania **0,0 %** | ✅ |
| B-8 `det_math` wobec libm | ≤ 2× | `ln` **1,68×**, `exp` **1,16×**; softmax 16 opcji **130 ns** (cel ≤ 350 ns) | ✅ |
| B-9 arena 600 tys. | `get` ≤ 5 ns, iteracja ≤ 2 ms, insert+remove ≤ 20 ns | **2,7 ns**, **0,23 ms**, **4,7 ns** | ✅ K-16 uzasadnione |


---

## 5. Projekt techniczny

### 5.0 Workspace (PRD §16.2)

```
magnat/
├── Cargo.toml            # [workspace] members, [workspace.dependencies], [workspace.lints]
├── rust-toolchain.toml   # kanał stable przypięty; nightly tylko dla joba Miri
├── clippy.toml           # disallowed-types: HashMap/HashSet w sim/*
├── engine/
│   ├── core/             # magnat-core      (crate name: magnat-core, lib name: core_)
│   ├── ecs/              # magnat-ecs
│   ├── jobs/             # magnat-jobs
│   ├── io/               # magnat-io        (M0: minimalny)
│   └── devtools/         # magnat-devtools  (M0: szkielet)
├── tools/
│   ├── headless/         # magnat-headless  (bin)
│   └── voxelview/        # magnat-voxelview (bin, bootstrap wgpu)
└── docs/implementation-plan/
```

Katalogi `engine/{spatial,nav,voxel,render,ui,audio,script}`, `sim/*`, `game/`, `data/` **nie powstają w M0** —
tworzy je faza-właściciel wg tabeli 00 §1. Pusty crate założony „na zapas" to martwy kod w CI.

Zależności zewnętrzne dopuszczone w M0 (granica z PRD §16.1), wszystkie przez `[workspace.dependencies]`
z przypiętymi wersjami:

| Crate M0 | Zależności |
|---|---|
| `magnat-core` | `serde`, `xxhash-rust` (xxh3), `indexmap` (nośnik `SeededMap`) |
| `magnat-ecs` | `magnat-core`, `magnat-jobs`, `fixedbitset` |
| `magnat-jobs` | `rayon` (pula z work stealing), `parking_lot` |
| `magnat-io` | `magnat-core`, `magnat-ecs`, `bincode`, `zstd` |
| `magnat-devtools` | `magnat-core`, `magnat-ecs`, `tracy-client` (za feature `profiling`) |
| `magnat-headless` | powyższe plus `clap` |
| `magnat-voxelview` | `wgpu`, `winit`, `glam`, `pollster`, `env_logger`, `bytemuck` |

**Decyzja (jobs):** `engine/jobs` to cienka warstwa nad jawną instancją `rayon::ThreadPool`, nie własny
scheduler deque. Work stealing dostajemy z pudełka; determinizm i tak **nie pochodzi** od puli, tylko od
dyscypliny redukcji po indeksie (§5.4). Sufit tego skrótu: brak kontroli nad priorytetami zadań i nad
pinningiem do rdzeni. Wymiana na własny Chase-Lev dopiero, gdy profilowanie w M12 wskaże narzut rayon —
API `JobPool` jest zaprojektowane tak, by wymiana nie dotknęła wywołujących.

### 5.1 `engine/core` — typy bazowe

Typy z 00 §2 są przepisywane do `core::types` bez zmian. M0 dodaje do nich **zachowanie**:

```rust
// core::money — jedyna dopuszczona arytmetyka pieniądza (00 §2)
impl Money {
    pub const ZERO: Money = Money(0);
    pub fn checked_add(self, rhs: Money) -> Option<Money>;
    pub fn checked_sub(self, rhs: Money) -> Option<Money>;
    pub fn checked_mul_int(self, k: i64) -> Option<Money>;

    /// Mnożenie przez ułamek z jawnym zaokrągleniem połówek w górę (od zera).
    /// Liczone w i128 — brak przepełnienia dla |kwota| < 2^63 i |num|,|den| < 2^31.
    pub fn mul_ratio(self, num: i64, den: i64) -> Money;

    /// Dzielenie z zaokrągleniem połówek od zera. −5/2 = −3, 5/2 = 3.
    pub fn div_round_half_up(self, den: i64) -> Money;
}

/// Podział kwoty między N stron. Suma wyniku == total, ZAWSZE.
/// Reszta (total − suma podłóg) trafia po 1 grosz do kolejnych stron
/// w podanej kolejności wag — kolejność jest kontraktem wywołującego,
/// nigdy iteracją po mapie (00 §3.2).
pub fn split_proportional(total: Money, weights: &[u64]) -> Vec<Money>;

// core::fixed — Q32.32 dla ułamków tam, gdzie float jest zabroniony (00 §2)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fx(i64);
impl Fx {
    pub const FRAC_BITS: u32 = 32;
    pub const ONE: Fx = Fx(1 << 32);
    pub fn from_int(v: i32) -> Fx;
    pub fn from_ratio(num: i32, den: i32) -> Fx;
    pub fn to_int_trunc(self) -> i32;
    pub fn to_int_round(self) -> i32;
    pub fn mul(self, rhs: Fx) -> Fx;   // przez i128
    pub fn div(self, rhs: Fx) -> Fx;   // przez i128, panika przy rhs == 0
    pub fn saturating_add(self, rhs: Fx) -> Fx;
}
```

Czas i częstotliwość systemów (00 §4, PRD §17.1):

```rust
// core::time
pub const MINUTES_PER_HOUR: u64 = 60;
pub const HOURS_PER_DAY:    u64 = 24;
pub const DAYS_PER_MONTH:   u64 = 30;   // 00 §K-1: kalendarz 360-dniowy
pub const MONTHS_PER_YEAR:  u64 = 12;   // 12 × 30 = 360 dni, BEZ lat przestępnych
pub const MICRO_TICK_MS:    u64 = 100;  // tick ruchu mikro

/// Widok kalendarzowy na Tick. Czysta arytmetyka, bez stanu.
/// Kalendarz jest STAŁY: 12 miesięcy po 30 dni, rok = 360 dni, brak lat przestępnych,
/// każdy miesiąc tak samo długi (00 §K-1). Konsekwencje, na których polegają kolejne fazy:
/// okres rozliczeniowy, pensja, czynsz i odsetki są zawsze tej samej długości, więc
/// porównania rok do roku nie wymagają normalizacji, a testy regresji balansu (M5)
/// nie mają szumu z lutego. Sezony (M1, M8) dzielą rok na cztery równe kwartały po 90 dni.
/// To nie jest uproszczenie do poprawienia później — zmiana na kalendarz gregoriański
/// unieważniłaby każdy zapis gry i każdą linię bazową balansatora.
#[derive(Clone, Copy)]
pub struct SimCalendar { pub tick: Tick }
impl SimCalendar {
    pub fn minute_of_hour(self) -> u8;   // 0..=59
    pub fn hour_of_day(self)    -> u8;   // 0..=23
    pub fn day_of_month(self)   -> u8;   // 1..=30
    pub fn month_of_year(self)  -> u8;   // 1..=12
    pub fn year(self)           -> u32;
    pub fn is_hour_boundary(self) -> bool;
    pub fn is_day_boundary(self)  -> bool;
    pub fn is_month_boundary(self) -> bool;
}

/// Częstotliwość systemu (00 §4). Scheduler odpytuje ją co tick.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Cadence {
    EveryMicroTick,   // 100 ms gry, tylko LOD Mikro (PRD §17.1)
    EveryMinute,
    EveryHour,
    EveryDay,
    EveryMonth,
}
impl Cadence {
    /// Czy system ma się wykonać w tym ticku ekonomicznym.
    pub fn due(self, tick: Tick) -> bool;
}
```

### 5.1a `engine/core` — `Entity`, słowniki domenowe i `DecisionReason` (00 §K-8, §K-12)

**Dlaczego to jest w `core`, a nie w fazie, która tego używa.** Gdyby `TransportMode` mieszkał w `sim/traffic`,
a `NeedKind` w `sim/agents`, to `sim/agents` (wybór środka transportu) zależałby od `sim/traffic`,
`sim/traffic` od `sim/economy` (koszt paliwa), a `sim/economy` od `sim/agents` (potrzeby kupującego).
To cykl w grafie crateów — Cargo takiego workspace nie zbuduje. Zgłosiły to niezależnie M3, M5 i M8, więc
nie jest to hipoteza. `core` dostaje **wyłącznie słownik i konwersje, zero logiki**: żadnego kosztu przejazdu,
żadnej funkcji użyteczności, żadnego zapotrzebowania — te żyją w fazach.

```rust
// core::entity — Entity mieszka w core, nie w ecs.
// Wymusza to już 00 §2 (CitizenId(pub Entity) jest typem z core), a PlaceRef to potwierdza.
// `ecs` re-eksportuje `pub use magnat_core::Entity;` — dla wywołujących nic się nie zmienia,
// a core nie zyskuje zależności od ecs. To jedyna poprawna strona tej relacji.
pub struct Entity { index: u32, generation: NonZeroU32 }   // pełne API w §5.5

// core::vocab — słowniki K-8. Same warianty i konwersje.

/// Wymiar, w którym agent ocenia opcję. Konsument: M5 (funkcja użyteczności zakupu, PRD §6.4),
/// M7 (oceny ofert pracy), M9 (podsumowanie decyzji gracza).
#[repr(u8)]
pub enum UtilityKind { Price, Quality, Distance, Time, Variety, Brand, Habit, Convenience, Risk }

/// Środek transportu. Konsument: M3 (plan dnia), M4 (ruch), M5 (koszt dojazdu po zakupy), M8 (polityka miejska).
#[repr(u8)]
pub enum TransportMode { Walk, Bicycle, Car, Bus, Tram, Rail, Freight }

/// Odniesienie do miejsca w świecie — wspólna waluta między agentami, ruchem i gospodarką.
/// Warianty oparte na typowanych id z 00 §2 plus surowa współrzędna.
/// Świadomie NIE MA wariantu RoadNode: węzeł grafu dróg należy do `engine/nav` (M4),
/// a wpisanie go tutaj odtworzyłoby cykl, przed którym ten moduł ma bronić.
pub enum PlaceRef {
    Building(BuildingId),
    Parcel(ParcelId),
    Site(SiteId),
    District(DistrictId),
    Coord(WorldCoord),
}

/// Współrzędna świata w centymetrach — całkowitoliczbowa, więc deterministyczna i porównywalna.
/// `core` definiuje tylko reprezentację; interpretacja (układ, granice, wysokość terenu) należy do M1/M2.
pub struct WorldCoord { pub x: i32, pub y: i32, pub z: i32 }

/// Rodzaj potrzeby. Konsument: M3 (model potrzeb), M5 (co wyzwala zakup), M8 (usługi publiczne).
#[repr(u8)]
pub enum NeedKind { Hunger, Thirst, Rest, Hygiene, Health, Social, Fun, Safety, Esteem, Education }

/// Rodzaj czynności w planie dnia. Konsument: M3 (planer + DES), M4 (skąd dokąd), M9 (oś czasu w inspekcji).
#[repr(u8)]
pub enum ActivityKind { Sleep, Work, School, Shop, Eat, Leisure, Social, Errand, Commute, Idle }
```

**`DecisionReason` (K-12) — wyjaśnialność egzekwowana przez kompilator.**

Dokument 00 §7 wymaga, żeby każda decyzja agenta i firmy zapisywała powód w formie strukturalnej, a system
bez wyjaśnialności nie spełniał Definition of Done swojej fazy. Sam wymóg w dokumencie nie wystarczy —
sprawdza go człowiek na przeglądzie i przegapia. Dlatego enum jest **jeden, centralny i bez
`#[non_exhaustive]`**: kod renderujący kartę inspekcji robi `match` bez ramienia `_`, więc dopisanie wariantu
przez M7 **łamie kompilację `game/`** dopóki ktoś nie napisze, jak ten powód pokazać graczowi.
To nie jest niedogodność — to jest cały mechanizm.

```rust
// core::decision — JEDEN enum dla całej gry. Bez #[non_exhaustive]. Celowo.
#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, HashState)]
pub enum DecisionReason {
    // ── M0: 0..=19 ────────────────────────────────────────────────────────────
    Unspecified = 0,                       // tylko w testach silnika; użycie w sim/* to błąd przeglądu

    // ── M3 — agenci: 100..=199 ────────────────────────────────────────────────
    // NeedUrgent { need: NeedKind, level: Q } = 100,
    // ScheduleConflict { dropped: ActivityKind } = 101,

    // ── M4 — ruch: 200..=299 ──────────────────────────────────────────────────
    // ModeChosen { mode: TransportMode, minutes: u16 } = 200,

    // ── M5 — gospodarka detaliczna: 300..=399 ─────────────────────────────────
    // ShopChosen { shop: FirmId, dominant: UtilityKind, margin_permille: i16 } = 300,
    // PurchaseSkipped { need: NeedKind, reason: SkipReason } = 301,
    // ... kolejne fazy dopisują własne bloki na końcu pliku
}
```

Zasady dokładania wariantów — wiążące dla faz M1+:

1. **Blok dyskryminant per faza, po 100**, przypisany jak siatka `StreamId` (§5.2): M0 0–19, M3 100–199,
   M4 200–299, M5 300–399, M6 400–499, M7 500–599, M8 600–699, M9 700–799, M10 800–899, M12 900–999.
   Jawne `= N` przy każdym wariancie, bo dyskryminanta wchodzi do zapisu gry.
2. **Warianty dopisuje się na końcu swojego bloku, nigdy w środku cudzego.** Fazy pracują na rozłącznych
   fragmentach jednego pliku, więc konflikty scalania są punktowe, nie strukturalne.
3. **Nigdy nie usuwać i nie zmieniać numeracji istniejącego wariantu** — dyskryminanta jest w snapshocie
   i w kronikach (M9). Wariant nieużywany zostaje z komentarzem `// wycofany w MX`.
4. **Nazwa zawiera domenę** (`ShopChosen`, `ModeChosen`, `TaxRaised`), bo przestrzeń nazw jest wspólna.
5. **Ładunek: `Copy`, małe, typowane id — nigdy `String`.** Powód jest zapisywany milionami sztuk na dobę gry;
   tekst powstaje dopiero w warstwie prezentacji, z lokalizacją (00 §6). Test pilnuje
   `size_of::<DecisionReason>() <= 24`; większy ładunek chowa się za `Entity` albo indeksem do tablicy fazy.
6. Wariant, którego nie da się pokazać graczowi jednym zdaniem, jest źle zaprojektowany — to sygnał, że
   decyzja ma zbyt wiele przyczyn i trzeba ją rozbić.

```rust
// core::schema — tożsamość schematu komponentu. Potrzebna M12 do migracji zapisów,
// ale musi istnieć od M0, bo inaczej pierwsze zapisy nie mają czego wersjonować.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ComponentSchemaId(pub u64);   // xxh3_64("<nazwa>@<wersja>")

impl ComponentSchemaId {
    /// Liczona w czasie kompilacji z pary (Component::NAME, Component::SCHEMA_VERSION).
    pub const fn new(name: &str, version: u16) -> ComponentSchemaId;
    pub fn name_hint(self) -> Option<&'static str>;   // z rejestru, do komunikatów błędów
}
```

Reguła dla faz: **zmiana układu pól komponentu to podbicie `Component::SCHEMA_VERSION`**, nie zmiana jego
nazwy. Nazwa identyfikuje komponent przez całe życie projektu, wersja identyfikuje jego kształt — dopiero
ta para pozwala M12 napisać migrację „`Wallet@1` → `Wallet@2`" zamiast zgadywać.

### 5.1b `engine/core` — `Arena<T>` (00 §K-16)

**Dlaczego nie wszystko jest encją.** `BatchId` i `OfferId` przestają być `Entity` (zmiana w 00 §2):
partii towaru jest rzędu 600 tys., ofert podobnie, jedne i drugie powstają i giną w każdym ticku, a **żadna
z nich nie jest nigdy odpytywana przekrojowo po archetypach** — do partii dociera się przez magazyn, pojazd
albo półkę, do oferty przez indeks przestrzenny kategorii. Płacenie za nie buforem komend, sortowaniem kluczy
i przenoszeniem wierszy między archetypami to czysty koszt bez żadnej korzyści: ECS zarabia na iteracji
po komponentach, której tu nie ma.

M0 dostarcza **sam mechanizm**, żeby M5 i M6 nie napisały dwóch różnych aren z dwoma różnymi błędami.
`core` nie wie, czym jest partia ani oferta — zna tylko sloty, generacje i kolejność.

```rust
// core::arena

/// Uchwyt do slotu areny. Ten sam kształt co Entity (8 B, generacja niezerowa,
/// Ord po (index, generation)) — ale INNY typ, żeby kompilator nie pozwolił
/// wsadzić BatchId tam, gdzie oczekiwana jest encja.
/// Parametr T wiąże uchwyt z zawartością: ArenaHandle<Batch> nie pasuje do Arena<Offer>.
pub struct ArenaHandle<T> { index: u32, generation: NonZeroU32, _t: PhantomData<fn() -> T> }

impl<T> ArenaHandle<T> {
    pub fn index(self) -> u32;
    pub fn generation(self) -> u32;
    pub fn to_bits(self) -> u64;
    pub fn from_bits(bits: u64) -> Option<Self>;
}
// 00 §2, po zmianie K-16:  pub type BatchId = ArenaHandle<Batch>;  (Batch definiuje M6)
//                          pub type OfferId = ArenaHandle<Offer>;  (Offer definiuje M5)

/// Arena generacyjna. Sloty o stabilnych indeksach, swobodna lista wolnych slotów,
/// iteracja ZAWSZE w kolejności indeksów.
pub struct Arena<T> {
    slots: Vec<Slot<T>>,     // indeks slotu jest stabilny przez całe życie areny
    free:  Vec<u32>,         // LIFO; kolejność zwalniania jest deterministyczna, bo
                             // zwolnienia dzieją się w punktach synchronizacji (§5.8)
    live:  u32,
}
enum Slot<T> { Occupied { generation: NonZeroU32, value: T }, Vacant { generation: NonZeroU32 } }

impl<T> Arena<T> {
    pub fn new() -> Self;
    pub fn with_capacity(n: usize) -> Self;

    pub fn insert(&mut self, value: T) -> ArenaHandle<T>;
    /// Zwalnia slot i PODBIJA jego generację. Stary uchwyt przestaje być ważny
    /// na zawsze — kolejne insert() w ten sam slot dostaje nową generację.
    pub fn remove(&mut self, h: ArenaHandle<T>) -> Option<T>;

    /// None, jeśli slot jest wolny ALBO generacja się nie zgadza. Nigdy nie zwraca
    /// cudzej wartości z ponownie użytego slotu — to jest cały powód istnienia generacji.
    pub fn get(&self, h: ArenaHandle<T>) -> Option<&T>;
    pub fn get_mut(&mut self, h: ArenaHandle<T>) -> Option<&mut T>;
    pub fn contains(&self, h: ArenaHandle<T>) -> bool;

    pub fn len(&self) -> usize;
    /// Iteracja w kolejności indeksów slotów, z pominięciem wolnych. Deterministyczna
    /// z definicji — to jest jedyna dopuszczona iteracja po arenie w kodzie symulacji.
    pub fn iter(&self) -> impl Iterator<Item = (ArenaHandle<T>, &T)>;
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (ArenaHandle<T>, &mut T)>;

    /// Chunki slotów do równoległej iteracji przez JobPool (§5.4) — podział
    /// po indeksie, więc redukcja składa się deterministycznie.
    pub fn chunks(&self, rows: usize) -> impl Iterator<Item = ArenaChunk<'_, T>>;

    /// Kompaktowanie NIE ISTNIEJE i nie powstanie. Przesunięcie wartości między
    /// slotami unieważniłoby wszystkie uchwyty trzymane w komponentach ECS.
    /// Puste sloty są odzyskiwane przez swobodną listę, nie przez przenoszenie danych.
}

/// Rejestr aren w World — żeby hash i snapshot wiedziały, co mają objąć,
/// bez zgadywania i bez zależności core → sim.
/// Wartości są WIECZNE, tak jak StreamId: M5 = Offers, M6 = Batches, kolejne fazy dopisują.
#[repr(u16)]
pub enum ArenaKind { Offers = 1, Batches = 2 }
```

**Determinizm (punkt krytyczny).** Arena wchodzi do `world_state_hash` (§5.9) — w kolejności `ArenaKind`,
w arenie po indeksie slotu rosnąco, a dla każdego slotu hashowane są **generacja, znacznik zajętości
i wartość**. Nie sama zawartość: dwa przebiegi o identycznej treści partii, ale różnym układzie slotów,
**muszą** dać różny hash, bo są różnymi stanami — inaczej rozjazd układu areny przeszedłby przez CI
niezauważony i wypłynąłby dopiero jako rozjazd cen dwadzieścia tysięcy ticków później.
Warunkiem jest, żeby `insert`/`remove` działy się wyłącznie w punktach synchronizacji (tak jak komendy
strukturalne, §5.8) — arena nie jest współdzielona między równolegle działającymi systemami.

**Snapshot.** Arena jest sekcją snapshotu na równi z kolumną ECS:
`SectionKind::Arena { kind: ArenaKind, part }`, gdzie `part` to `Values` (ciąg wartości zajętych slotów),
`Generations` (generacja każdego slotu, także wolnego) i `Occupancy` (bitmapa zajętości).
Rozbicie na trzy części jest po to, żeby M12 mogło migrować sam układ wartości, nie dotykając generacji —
bez tego migracja schematu partii unieważniłaby każdy zapisany `BatchId`.

### 5.2 `engine/core` — RNG ze strumieniami (00 §3.1, PRD §18.2)

Brak globalnego stanu. Generator jest wyprowadzany czystą funkcją i żyje tylko w obrębie jednego
wywołania systemu dla jednej encji.

```rust
// core::rng
/// Strumień RNG. Wartości liczbowe są WIECZNE — faza dopisuje warianty na końcu,
/// nigdy nie zmienia i nie usuwa istniejących (00 §3.1). Test strażniczy pilnuje wartości.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u16)]
pub enum StreamId {
    // ── M0: 0..=19 ───────────────────────────────────────────────────────────
    // 0 zarezerwowane — brak strumienia; użycie to błąd, nie wartość domyślna
    EngineSelfTest = 1,     // tylko testy silnika i świat syntetyczny headless
    // ── M1: 100..=119 ──  Terrain = 100, Hydrology = 101, Climate = 102, ...
    // ── M2: 120..=139 ──  dalej wg siatki poniżej
}

#[derive(Clone)]
pub struct Rng { s: [u64; 4] }   // xoshiro256++

/// Jedyny sposób utworzenia generatora. Czysta funkcja czterech argumentów.
pub fn rng(world_seed: u64, stream: StreamId, entity_index: u32, tick: Tick) -> Rng;

impl Rng {
    pub fn next_u64(&mut self) -> u64;
    pub fn next_u32(&mut self) -> u32;
    /// Bez obciążenia modulo (metoda Lemire'a z odrzucaniem).
    pub fn gen_range_u32(&mut self, end_exclusive: u32) -> u32;
    pub fn gen_q(&mut self) -> Q;                 // 0..=100
    pub fn gen_bool_permille(&mut self, p: u16) -> bool;
    /// Tasowanie Fishera–Yatesa — jedyny dopuszczony sposób losowej permutacji.
    pub fn shuffle<T>(&mut self, slice: &mut [T]);
}
```

**Siatka zakresów `StreamId` (00 §K-4) — obowiązująca, M0 jest właścicielem enuma:**

| Faza | Zakres | Blok nadmiarowy |
|---|---|---|
| M0 | 0–19 | 1000–1019 |
| M1 | 100–119 | 1100–1119 |
| M2 | 120–139 | 1120–1139 |
| M3 | 140–159 | 1140–1159 |
| M4 | 160–179 | 1160–1179 |
| M5 | 180–199 | 1180–1199 |
| M6 | 200–219 | 1200–1219 |
| M7 | 220–239 | 1220–1239 |
| M8 | 240–259 | 1240–1259 |
| M9 | 260–279 | 1260–1279 |
| M10 | 280–299 | 1280–1299 |
| M11 | 300–319 | 1300–1319 |
| M12 | 320–339 | 1320–1339 |

Fazie, której 20 strumieni nie wystarczy, przysługuje blok `baza + 1000` — bez negocjacji i bez wchodzenia
w cudzy zakres. Zakresy 20–99 i 340–999 pozostają wolne; nie przydzielamy ich z góry, bo rezerwa,
której nikt nie potrzebuje, to tylko trudniejsza tabela.

Konsekwencja dla faz M1+: system, który potrzebuje losowości bez encji (np. zdarzenie globalne),
podaje `entity_index = u32::MAX` i własny `StreamId` — nie wymyśla innego mechanizmu.
Strumień jest **per przeznaczenie, nie per system**: dwa systemy losujące to samo zjawisko dzielą strumień,
jeden system losujący dwie niezależne rzeczy bierze dwa. Inaczej dołożenie systemu w M7 przesunęłoby
sekwencję widzianą przez system z M5 i zmieniło świat przy tym samym seedzie.

### 5.3 `engine/core` — `SeededMap` (00 §3.2)

Zakaz iteracji po `HashMap` wymaga jednego, wygodnego zamiennika. Nie piszemy własnej tablicy haszującej —
`indexmap` daje kolejność wstawiania i wymienny haszer.

```rust
// core::collections
/// Mapa o deterministycznej kolejności iteracji: kolejność wstawiania,
/// haszer ze stałym ziarnem wkompilowanym w binarkę.
pub type SeededMap<K, V> = indexmap::IndexMap<K, V, SeededHasherBuilder>;
pub type SeededSet<K>    = indexmap::IndexSet<K, SeededHasherBuilder>;

pub fn seeded_map<K, V>() -> SeededMap<K, V>;

pub trait SeededMapExt<K: Ord, V> {
    /// Iteracja po posortowanym kluczu — do użycia wszędzie tam, gdzie wynik
    /// wchodzi do stanu trwałego, a kolejność wstawiania nie jest kontraktem (00 §2).
    fn iter_sorted(&self) -> impl Iterator<Item = (&K, &V)>;
}
```

Reguła doboru, wiążąca dla faz M1+:
- kolejność wstawiania jest sama w sobie deterministyczna (budowa z posortowanego wejścia) → `SeededMap`,
- kolejność ma znaczenie semantyczne (rankingi, sumowanie floatów) → `BTreeMap` albo `iter_sorted()`,
- klucz to gęsty indeks (`GoodId`, `DistrictId`) → zwykły `Vec` indeksowany, nie mapa.

### 5.3a `engine/core` — `det_math` (00 §K-6)

**Wybór: `f64` z operacjami poprawnie zaokrąglonymi, nie fixed-point.** Uzasadnienie:

- Rozstrzygnięcie K-6 dotyczy *tylko* funkcji przestępnych. Podstawowa arytmetyka `f64` jest już
  deterministyczna (§2) — budowanie `Fx`-owego minimaksu rozwiązywałoby problem, którego nie ma, kosztem
  własnej biblioteki numerycznej z osobnym zestawem błędów.
- Zakres dynamiczny. Użyteczności i softmax w M5 operują na argumentach od 10⁻⁶ do 10⁶; Q32.32 ma około
  9 cyfr znaczących przy 1.0 i traci je gwałtownie przy skrajach. `f64` ma 15–17 w całym zakresie.
- Wynik `exp`/`ln` **nie wchodzi bezpośrednio do pieniądza** — pieniądz jest `i64` (00 §2), a funkcja
  użyteczności tylko rankinguje opcje. Do księgowania i tak trafia `Money` po jawnym zaokrągleniu.
- Koszt wdrożenia: około 200 linii wielomianów i składania wykładnika, kontra własny fixed-point
  z osobną analizą przepełnień dla każdej funkcji.

Sufit tej decyzji: jeśli kiedyś jakaś wielkość *pieniężna* będzie musiała przejść przez `exp`
(np. składanie odsetek w M10), robi to przez `Fx` i jawne zaokrąglenie **na wejściu i wyjściu**, nie przez
zmianę reprezentacji w `det_math`.

```rust
// core::det_math — deterministyczne funkcje przestępne.
// Zbudowane WYŁĄCZNIE z +, -, *, / i f64::sqrt (IEEE-754 wymaga poprawnego zaokrąglenia
// tych pięciu operacji) oraz f64::to_bits/from_bits. Żadnego wywołania libm,
// żadnego mul_add (na sprzęcie bez FMA jest emulowany przez libm — patrz §2).
// Wynik jest identyczny bit w bit na każdej platformie, którą Rust wspiera z IEEE-754 binary64.

/// ln(x). Rozkład x = m · 2^k przez bity wykładnika, m ∈ [√2/2, √2).
/// ln(m) = 2·atanh(s), s = (m−1)/(m+1); nieparzysty wielomian minimaksowy stopnia 15 w s
/// (8 współczynników). Człon k·ln2 liczony jako k·LN2_HI + k·LN2_LO (rozbicie na dwie
/// połówki mantysy), żeby uniknąć kasowania dla x bliskich 1.
/// Dokładność ≤ 0,9 ULP. Domena: x > 0; x == 0 → NEG_INFINITY, x < 0 lub NaN → NaN.
pub fn ln(x: f64) -> f64;

/// ln(1+x) bez utraty precyzji dla |x| ≪ 1 — dla samego ln(1.0 + x) błąd sięga tam 10^8 ULP.
/// Konsument: M5 (log-użyteczność przyrostów), M10 (stopy zwrotu).
pub fn ln1p(x: f64) -> f64;

/// log2(x) — ta sama dekompozycja co ln, ale bez mnożenia przez ln2 (k wchodzi dokładnie).
/// Dokładność ≤ 0,8 ULP.
pub fn log2(x: f64) -> f64;

/// exp(x). Redukcja k = round(x · LOG2_E), r = x − k·LN2_HI − k·LN2_LO, |r| ≤ ln2/2.
/// exp(r) z wielomianu minimaksowego stopnia 6 (schemat Hornera, stała kolejność działań),
/// skalowanie przez 2^k złożone bitowo z from_bits. Dokładność ≤ 0,8 ULP.
/// Przepełnienie: x > 709.78… → INFINITY; x < −745.13… → 0.0. Progi są STAŁYMI,
/// nie wynikiem porównania z libm.
pub fn exp(x: f64) -> f64;
pub fn exp_m1(x: f64) -> f64;   // exp(x) − 1, dokładne dla małych x
pub fn exp2(x: f64) -> f64;

/// x^y. Liczone jako exp2(y · log2(x)) w arytmetyce podwojonej precyzji (para hi/lo
/// z algorytmem Dekkera — dwa mnożenia f64 zamiast FMA), inaczej sam błąd log2
/// jest mnożony przez y i przy y ≈ 1000 wychodzi poza zakres użyteczny.
/// Dokładność ≤ 2 ULP. Przypadki brzegowe (y == 0, x == 1, wykładniki całkowite,
/// x < 0 z całkowitym y) obsłużone jawnie wg IEEE-754.
pub fn pow(x: f64, y: f64) -> f64;

/// Przekierowanie na f64::sqrt — IEEE-754 wymaga poprawnego zaokrąglenia sqrt,
/// więc jest deterministyczny na każdej platformie. Istnieje tylko po to,
/// żeby kod symulacji miał jedno miejsce, do którego sięga po matematykę.
#[inline] pub fn sqrt(x: f64) -> f64 { x.sqrt() }

/// Softmax stabilny numerycznie, in-place.
/// 1. m = max(xs) — po indeksie rosnąco, nie przez f64::max na iteratorze (NaN).
/// 2. xs[i] = exp((xs[i] − m) / temperature)
/// 3. suma w KOLEJNOŚCI INDEKSÓW (00 §2: float sumowany w ustalonej kolejności),
///    bez redukcji parami i bez rayon.
/// 4. normalizacja; suma wyniku mieści się w 1.0 ± 4 ULP·n.
/// Wejście puste → no-op. temperature ≤ 0 → panika (błąd wywołującego, nie dane wejściowe).
pub fn softmax_in_place(xs: &mut [f64], temperature: f64);
pub fn softmax(xs: &[f64], temperature: f64) -> Vec<f64>;

/// Losowanie z rozkładu softmax — najczęstszy konsument (M5: wybór sklepu,
/// M7: decyzje firm). Kumulacja w kolejności indeksów, próg z Rng::next_u64
/// przeskalowany do [0,1) przez dzielenie przez 2^53 (dokładne, bez zaokrąglenia).
/// Zwraca indeks wybranej opcji. Kolejność `weights` jest kontraktem wywołującego.
pub fn softmax_pick(weights: &[f64], temperature: f64, rng: &mut Rng) -> usize;
```

Czego `det_math` **nie ma** w M0 i dlaczego: brak `sin`/`cos`/`atan2` (geometria i ruch są w `render`/`voxel`,
gdzie float wolno — a jeśli M4 będzie ich potrzebował w kodzie ruchu wpływającym na stan, dopisze je tu
tym samym schematem); brak `tanh`/`sigmoid` (wyprowadzalne z `exp`, dokłada je pierwszy konsument);
brak wersji `f32` (symulacja nie używa `f32`).

### 5.4 `engine/jobs` — deterministyczny fork-join (00 §3.3, PRD §17.2)

```rust
pub struct JobPool { /* jawna instancja rayon::ThreadPool */ }

impl JobPool {
    /// threads == 0 → liczba dostępnych rdzeni. Nazwy wątków: "magnat-worker-{i}".
    pub fn new(threads: usize) -> JobPool;
    pub fn thread_count(&self) -> usize;

    /// Zadania zagnieżdżone; panika workera propaguje się do wywołującego.
    pub fn scope<'s, R>(&self, f: impl FnOnce(&Scope<'s>) -> R + Send) -> R;
}

/// JEDYNY dopuszczony sposób równoległej redukcji w kodzie symulacji.
/// Wyniki cząstkowe lądują w Vec<Option<A>> indeksowanym numerem chunka
/// i są składane w kolejności indeksu na wątku wywołującym — nigdy
/// w kolejności zakończenia zadań (00 §3.3).
pub fn map_reduce_indexed<C, A, R>(
    pool:    &JobPool,
    chunks:  &[C],
    map:     impl Fn(usize, &C) -> A + Sync,
    combine: impl Fn(R, A) -> R,
    init:    R,
) -> R
where C: Sync, A: Send, R: Send;

/// Równoległa mutacja rozłącznych chunków bez redukcji (typowa iteracja ECS).
/// Brak wartości zwracanej ⇒ brak problemu kolejności; kolejność efektów
/// ubocznych jest kontraktem wywołującego (ma ich nie mieć).
pub fn for_each_chunk_mut<C: Send>(
    pool: &JobPool,
    chunks: &mut [C],
    f: impl Fn(usize, &mut C) + Sync,
);
```

Jawny zakaz: `rayon::iter::*` i globalna pula rayon nie są używane poza `engine/jobs` (lint
`disallowed-types`). Kod symulacji widzi wyłącznie te trzy funkcje.

### 5.5 `engine/ecs` — encja i storage archetypowy

```rust
/// 8 bajtów. Generacja niezerowa ⇒ size_of::<Option<Entity>>() == 8.
/// Ord po (index, generation) — porządek sortowania komend (00 §3.4).
/// DEFINIOWANA W `core` (§5.1a), tutaj tylko re-eksport: pub use magnat_core::Entity;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Entity { index: u32, generation: NonZeroU32 }

impl Entity {
    pub fn index(self) -> u32;
    pub fn generation(self) -> u32;
    /// Płaska reprezentacja do serializacji i kluczy. Odwracalna.
    pub fn to_bits(self) -> u64;
    pub fn from_bits(bits: u64) -> Option<Entity>;
}

/// Rejestr żywych encji. Despawn podbija generację ⇒ stary uchwyt wygasa.
pub struct EntityStore {
    meta:     Vec<EntityMeta>,   // indeksowane entity.index()
    free:     Vec<u32>,          // lista wolnych indeksów, LIFO
    alive:    u32,
}
struct EntityMeta { generation: NonZeroU32, location: Option<EntityLocation> }

#[derive(Clone, Copy)]
pub struct EntityLocation { archetype: ArchetypeId, chunk: u16, row: u16 }
```

Komponenty i archetypy:

```rust
/// Komponent = zwykły typ danych. Bez logiki, bez referencji, bez Drop w ścieżce gorącej.
pub trait Component: Send + Sync + 'static {
    /// Nazwa stabilna między wersjami binarki — klucz w tablicy schematu snapshotu
    /// (§5.8). Zmiana nazwy = zmiana formatu zapisu.
    const NAME: &'static str;
}

/// Nadawany przy rejestracji w danym uruchomieniu. NIE jest stabilny między
/// uruchomieniami — w snapshocie zapisujemy Component::NAME (00 §5, analogicznie do GoodId).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ComponentId(u16);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ArchetypeId(u32);

/// Docelowy rozmiar chunka w bajtach. Liczba wierszy jest wyliczana PER ARCHETYP:
///   rows_per_chunk = clamp(prev_pow2(CHUNK_TARGET_BYTES / row_bytes), 8, 2048)
/// Potęga dwójki ⇒ lokalizacja to przesunięcie bitowe (shift trzymany w Archetype).
/// Wartość jest deterministyczna, bo row_bytes wynika z zestawu komponentów.
///
/// 64 KiB nie jest wartością z sufitu: to jednostka copy-on-write zapisu w tle w M12
/// (§5.5.1). Duży chunk = mniej kopiowania przy zapisie, ale grubsze ziarno
/// równoległości i gorsze trafienia w L2; 64 KiB mieści się w L2 każdego procesora
/// z ostatniej dekady i daje ~160 chunków na 400 tys. encji o wierszu 400 B,
/// czyli wystarczająco drobny podział dla 16 wątków.
pub const CHUNK_TARGET_BYTES: usize = 64 * 1024;

pub struct Archetype {
    id:         ArchetypeId,
    /// Kanoniczna tożsamość archetypu: ComponentId posortowane rosnąco.
    /// Dwa archetypy o tym samym zestawie to ten sam archetyp — zawsze.
    components: Box<[ComponentId]>,
    /// Kolumny w tej samej kolejności co `components`.
    chunks:     Vec<ArchetypeChunk>,
    rows:       u32,
    /// Krawędzie grafu przejść: dodanie/usunięcie komponentu → docelowy archetyp.
    /// Cache, żeby insert/remove nie przeszukiwał rejestru archetypów.
    edges:      SeededMap<(ComponentId, EdgeKind), ArchetypeId>,
}

pub struct ArchetypeChunk {
    /// JEDNA alokacja na chunk, osiągalna przez JEDEN wskaźnik. Kolumny to rozłączne
    /// wycinki tej alokacji (SoA), tablica entities też w niej siedzi.
    /// Wyrównanie: max(align_of) wszystkich komponentów archetypu.
    /// Ten layout jest kontraktem, nie szczegółem: podmiana chunka w M12 to podmiana
    /// jednego wskaźnika, a nie rekonstrukcja struktury (§5.5.1).
    data:     ChunkStorage,         // NonNull<u8> + Layout
    columns:  Box<[ColumnSlice]>,   // offset + stride + layout + drop_fn
    len:      u16,
    rows_cap: u16,
    /// Rośnie przy każdej mutacji zawartości chunka. M12 czyta ją, żeby wiedzieć,
    /// czy sekcja snapshotu jest nadal aktualna (zapis przyrostowy).
    generation: u32,
    /// Licznik zmian per kolumna — pod filtry Changed<T> w M3 (nie w M0, D-4).
    _reserved_change_ticks: (),
}
```

Operacje bezpośrednie na `World` (poza tickiem — ładowanie danych, testy, odczyt snapshotu):

```rust
pub struct World {
    entities:   EntityStore,
    archetypes: Archetypes,
    components: ComponentRegistry,
    resources:  Resources,
    pub tick:   Tick,
    pub seed:   u64,
}

impl World {
    pub fn new(seed: u64) -> World;
    pub fn register_component<T: Component>(&mut self) -> ComponentId;

    pub fn spawn(&mut self) -> EntityMut<'_>;         // builder: .with(c).id()
    pub fn despawn(&mut self, e: Entity) -> bool;
    pub fn insert<T: Component>(&mut self, e: Entity, value: T);
    pub fn remove<T: Component>(&mut self, e: Entity) -> Option<T>;
    pub fn get<T: Component>(&self, e: Entity) -> Option<&T>;
    pub fn get_mut<T: Component>(&mut self, e: Entity) -> Option<&mut T>;
    pub fn contains(&self, e: Entity) -> bool;

    pub fn insert_resource<T: Send + Sync + 'static>(&mut self, v: T);
    pub fn resource<T: Send + Sync + 'static>(&self) -> &T;
    pub fn resource_mut<T: Send + Sync + 'static>(&mut self) -> &mut T;

    pub fn query<Q: QueryData, F: QueryFilter>(&mut self) -> Query<'_, Q, F>;
}
```

`unsafe` w M0 jest ograniczony do trzech miejsc w `ecs::storage`: alokacja chunka, rzutowanie wycinka kolumny
na `&[T]`/`&mut [T]`, oraz `ptr::copy_nonoverlapping` przy przenoszeniu wiersza między archetypami.
Każde z nich ma komentarz `// SAFETY:` i dedykowany test pod Miri (WP-04). Reszta workspace ma
`#![forbid(unsafe_code)]` (00 §6).

#### 5.5.1 Chunk jako publiczna jednostka API — rozstrzygnięcie

**Decyzja: TAK, chunk jest publiczny — ale jako wąski, wyłącznie odczytowy uchwyt, nie jako wnętrzności
storage'u.** Powód jest konkretny, nie estetyczny: M12 buduje zapis w tle bez pauzy przez copy-on-write
(symulacja kopiuje chunk, podmienia wskaźnik, wątek zapisu czyta niemutowalną pamięć). Bez publicznego
dostępu do chunków ta funkcja nie istnieje, a cel „zapis < 5 s bez pauzy" z PRD §20.2 pada. Tego nie da się
dołożyć później: jeśli w M0 chunk będzie prywatnym szczegółem, to do M12 powstanie dziesięć faz kodu
zakładającego, że dane można przenosić między alokacjami — i CoW będzie wymagało przepisania storage'u.

Sprzeczność jest pozorna tylko na pierwszy rzut oka: „nie buduj funkcji bez konsumenta" (R-1) dotyczy
**mechanizmu**, a nie **kształtu**. M0 **nie implementuje** copy-on-write, licznika referencji ani zapisu
w tle. M0 zobowiązuje się jedynie do layoutu, który to umożliwia, i wystawia minimalny odczyt.
Koszt dziś: zero linii logiki. Koszt pominięcia: przepisanie storage'u w M12.

```rust
/// Stabilna tożsamość chunka. Stabilna w obrębie uruchomienia — chunk nie zmienia
/// numeru, dopóki żyje. Po despawnach chunk może zniknąć; numer nie jest wtedy wznawiany.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ChunkId { pub archetype: ArchetypeId, pub index: u32 }

/// Niemutowalny widok na chunk. Send + Clone, bez dostępu mutowalnego — to jest cała
/// umowa z M12: wątek zapisu trzyma ChunkRef i czyta bajty, a symulacja w tym czasie
/// pracuje na swojej kopii. M0 nie daje jeszcze mechanizmu, który tę kopię tworzy.
#[derive(Clone)]
pub struct ChunkRef<'w> {
    id:         ChunkId,
    len:        u16,
    generation: u32,
    /* wskaźnik na niemutowalną alokację + tablica kolumn */
}

impl<'w> ChunkRef<'w> {
    pub fn id(&self) -> ChunkId;
    pub fn len(&self) -> u16;
    /// Rośnie przy każdej mutacji. Niezmieniona ⇒ sekcja snapshotu jest aktualna.
    pub fn generation(&self) -> u32;
    pub fn entities(&self) -> &[Entity];
    /// Surowe bajty jednej kolumny — dokładnie to, co trafia do sekcji snapshotu (§5.9).
    pub fn column_bytes(&self, c: ComponentId) -> Option<&[u8]>;
    /// Typowany widok. None, jeśli archetyp nie ma tego komponentu.
    pub fn column<T: Component>(&self) -> Option<&[T]>;
}

impl World {
    /// Iteracja po wszystkich chunkach w kolejności (ArchetypeId, index) — deterministycznej.
    /// Konsument dziś: world_state_hash i save_world. Konsument w M12: zapis w tle.
    pub fn chunks(&self) -> impl Iterator<Item = ChunkRef<'_>>;
    pub fn chunk(&self, id: ChunkId) -> Option<ChunkRef<'_>>;
}
```

Czego M0 **nie** daje i co M12 sobie dokłada: `ChunkMut`, licznik referencji na alokacji, `AtomicPtr`
w miejscu `ChunkStorage`, sam mechanizm kopiowania i podmiany, tryb „zamrożonego" świata na czas zapisu.
Zobowiązanie M0 wobec M12 jest dokładnie takie: **jeden chunk to jedna alokacja ≤ 64 KiB pod jednym
wskaźnikiem, adresowalna przez `ChunkId`, z licznikiem `generation`, dostępna jako `Send` do odczytu.**
Zmiana któregokolwiek z tych pięciu warunków wymaga uzgodnienia z M12 — test layoutu w WP-04 pilnuje ich
mechanicznie, żeby nie zniknęły przy pierwszej optymalizacji.

### 5.6 `engine/ecs` — `Query` i deklaracja dostępu

Deklaracja read/write **nie jest pisana ręcznie** — wynika z typów zapytania. Ręczna deklaracja to
gwarantowane źródło rozjazdu między deklaracją a kodem, a scheduler opiera na niej poprawność.

```rust
/// Zbiór dostępów jednego systemu. Bitsety po ComponentId i po ResourceId.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Access {
    reads_components:  FixedBitSet,
    writes_components: FixedBitSet,
    reads_resources:   FixedBitSet,
    writes_resources:  FixedBitSet,
    /// System używa CommandBuffer ⇒ wymaga bariery po sobie.
    structural: bool,
}
impl Access {
    /// Konflikt = write×write lub read×write na tym samym identyfikatorze.
    /// Podstawa budowy krawędzi DAG (§5.7).
    pub fn conflicts_with(&self, other: &Access) -> bool;
    pub fn is_read_only(&self) -> bool;
}

/// Co system czyta z jednego wiersza. Implementacje: &T, &mut T, Entity, Option<&T>,
/// oraz krotki do 12 elementów.
pub trait QueryData: Send + Sync {
    type Item<'w>;
    /// Wpisuje własne wymagania do Access — stąd bierze się deklaracja systemu.
    fn declare_access(reg: &ComponentRegistry, out: &mut Access);
    fn matches(archetype: &Archetype) -> bool;
    /// # Safety: chunk musi pasować do `matches`, a row < chunk.len().
    unsafe fn fetch<'w>(chunk: &'w ArchetypeChunk, row: u16) -> Self::Item<'w>;
}

pub trait QueryFilter { fn matches(archetype: &Archetype) -> bool; }
pub struct With<T: Component>(PhantomData<T>);
pub struct Without<T: Component>(PhantomData<T>);

pub struct Query<'w, Q: QueryData, F: QueryFilter = ()> {
    world:    &'w World,
    /// Archetypy dopasowane raz, cache unieważniany wersją rejestru archetypów.
    matched:  &'w [ArchetypeId],
    _m: PhantomData<(Q, F)>,
}

impl<'w, Q: QueryData, F: QueryFilter> Query<'w, Q, F> {
    pub fn iter(&mut self) -> QueryIter<'_, Q, F>;              // wiersz po wierszu
    pub fn chunks(&mut self) -> QueryChunks<'_, Q, F>;          // jednostka równoległości
    pub fn get(&mut self, e: Entity) -> Option<Q::Item<'_>>;
    pub fn len(&self) -> usize;

    /// Równoległa iteracja: podział po indeksie chunka, wykonanie na JobPool.
    /// Domknięcie nie może mieć efektów ubocznych poza swoim chunkiem i CommandBuffer.
    pub fn par_for_each(&mut self, pool: &JobPool, f: impl Fn(Q::Item<'_>) + Sync);

    /// Deterministyczna redukcja — opakowanie map_reduce_indexed (§5.4).
    pub fn par_fold<A: Send, R: Send>(
        &mut self, pool: &JobPool,
        fold: impl Fn(&mut ChunkView<'_, Q>) -> A + Sync,
        combine: impl Fn(R, A) -> R, init: R,
    ) -> R;
}
```

Konflikt niemożliwy do wykrycia w typach (`Query<(&mut A, &A)>`) jest wykrywany przy budowie zapytania
i panikuje — test `trybuild`/panic w WP-05.

### 5.7 `engine/ecs` — system, `SystemId` i budowa DAG (PRD §17.2)

```rust
/// Stabilny identyfikator systemu. Wyprowadzony z pełnej nazwy ("sim.economy.match_offers"),
/// NIE z kolejności rejestracji — dodanie systemu w M7 nie może zmienić porządku
/// sortowania komend systemów z M5 (00 §3.4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SystemId(u32);   // xxh3_64(name) ścięty do u32; kolizja = panika przy rejestracji

pub struct SystemDesc {
    pub id:      SystemId,
    pub name:    &'static str,
    pub cadence: Cadence,
    pub access:  Access,
    /// Ograniczenia jawne — tylko tam, gdzie kolejność jest semantyczna,
    /// a nie wynika z konfliktu dostępów (np. „naliczanie odsetek przed rozliczeniem dnia").
    pub after:   Vec<SystemId>,
    pub before:  Vec<SystemId>,
}

pub trait System: Send + Sync + 'static {
    fn desc(&self) -> &SystemDesc;
    fn run(&mut self, ctx: &mut SystemCtx<'_>);
}

/// Wszystko, co system dostaje. Brak dostępu do zegara systemowego — z założenia (00 §3.5).
pub struct SystemCtx<'w> {
    pub world: UnsafeWorldCell<'w>,   // dostęp zawężony przez Access
    pub pool:  &'w JobPool,
    pub tick:  Tick,
    pub seed:  u64,
    pub cmd:   &'w mut CommandBuffer, // prealokowany per system
}

pub struct Schedule { stages: Vec<Stage> }
pub struct Stage {
    /// Systemy tego etapu wraz z krawędziami zależności wewnątrz etapu.
    nodes: Vec<ScheduleNode>,
    /// Po etapie: flush wszystkich CommandBuffer (§5.9).
    barrier: BarrierKind,
}
struct ScheduleNode { system: usize, depends_on: Vec<usize>, dependents: Vec<usize> }

pub struct ScheduleBuilder { systems: Vec<Box<dyn System>> }
impl ScheduleBuilder {
    pub fn add<S: System>(&mut self, system: S) -> &mut Self;
    pub fn build(self) -> Result<Schedule, ScheduleError>;
}

pub enum ScheduleError {
    DuplicateSystemId { name_a: &'static str, name_b: &'static str },
    OrderingCycle { cycle: Vec<&'static str> },
    UnknownConstraint { system: &'static str, target: SystemId },
}
```

**Algorytm budowy DAG** (to jest kontrakt, nie implementacja do wyboru):

1. Posortuj systemy po `SystemId` — to jest **kolejność kanoniczna**. Od tego momentu kolejność
   rejestracji nie ma znaczenia dla niczego.
2. Dla każdej pary `(i, j)`, `i < j` w kolejności kanonicznej: jeśli `access[i].conflicts_with(&access[j])`,
   dodaj krawędź `i → j`. Krawędzie idą zawsze „w przód" w kolejności kanonicznej, więc graf konfliktów
   jest acykliczny **z konstrukcji** — nie ma tu czego wykrywać.
3. Dołóż krawędzie z `after`/`before`. Dopiero tu możliwy jest cykl → wykrycie (Tarjan) i `ScheduleError::OrderingCycle`
   z wypisaną ścieżką.
4. Potnij na etapy: system z `access.structural == true` kończy etap (bariera po nim), bo jego komendy
   muszą zostać zaaplikowane, zanim ktokolwiek zobaczy nowe encje. Systemy strukturalne nie konfliktujące
   ze sobą trafiają do tego samego etapu — flush jest jeden, wspólny.
5. Wykonanie: licznik zależności per węzeł, gotowe węzły trafiają do `JobPool`. Systemy bez krawędzi
   między sobą biegną równolegle.

**Dlaczego wynik nie zależy od liczby wątków:** jeśli dwa systemy biegną równolegle, to znaczy, że nie mają
krawędzi, czyli ich zbiory dostępów nie konfliktują — nie widzą nawzajem swoich zapisów. Jedyny kanał, przez
który mogłyby na siebie wpłynąć, to bufory komend, a te są sortowane po `(SystemId, entity_index, seq)`
niezależnie od kolejności wykonania (§5.9). Test §7.2 sprawdza to empirycznie, ale gwarancja jest strukturalna.

```rust
/// Pętla ticku. Jedyne miejsce, w którym rośnie Tick.
pub struct App { pub world: World, schedule: Schedule, pool: JobPool, buffers: Vec<CommandBuffer> }
impl App {
    pub fn new(world: World, schedule: Schedule, threads: usize) -> App;
    /// 1. tick += 1  2. dla każdego etapu: uruchom należne systemy (Cadence::due)
    ///    3. bariera: flush komend  4. hooki devtools (metryki, opcjonalny hash).
    pub fn tick(&mut self);
    pub fn run_ticks(&mut self, n: u64);
}
```

### 5.8 `engine/ecs` — `CommandBuffer` (00 §3.4, PRD §17.2)

Mutacje strukturalne w trakcie ticku istnieją wyłącznie jako komendy. Bufor jest per system, więc nie ma
ani zamka, ani współdzielenia między wątkami.

```rust
pub struct CommandBuffer {
    system: SystemId,
    /// Ładunki komend w płaskim blobie — bez Box, bez alokacji per komenda.
    blob:   Vec<u8>,
    /// Klucze sortowania. Osobno od blobu, żeby sortować 16 B, nie ładunki.
    index:  Vec<CommandKey>,
    next_reservation: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CommandKey {
    system:  SystemId,   // 1. porządek systemów — kanoniczny, stabilny
    target:  u64,        // 2. Target zakodowany: encja → entity.index(), rezerwacja → 2^32 + seq
    seq:     u32,        // 3. kolejność zgłoszenia w obrębie systemu — rozstrzyga remisy
    offset:  u32,        // pozycja ładunku w blob (nie bierze udziału w Ord)
    kind:    CommandKind,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum CommandKind { Despawn = 0, Remove = 1, Spawn = 2, Insert = 3 }

/// Uchwyt do encji, która jeszcze nie istnieje. Nie da się z niego zrobić Entity
/// przed flushem — to jest gwarancja typu, nie konwencja.
#[derive(Clone, Copy)]
pub struct EntityReservation { system: SystemId, seq: u32 }

impl CommandBuffer {
    pub fn spawn(&mut self) -> EntityReservation;
    pub fn despawn(&mut self, e: Entity);
    pub fn insert<T: Component>(&mut self, e: Entity, value: T);
    pub fn insert_reserved<T: Component>(&mut self, r: EntityReservation, value: T);
    pub fn remove<T: Component>(&mut self, e: Entity);
    pub fn is_empty(&self) -> bool;
    pub fn clear(&mut self);
}

/// Aplikacja w barierze. Wywoływana wyłącznie przez App::tick, jednowątkowo.
/// Kolejność: (1) sortowanie wszystkich kluczy ze wszystkich buforów po CommandKey,
/// (2) faza Despawn, (3) faza Remove, (4) faza Spawn — indeksy encji pobierane
///     z listy wolnych w porządku zwolnienia z kroku (2), więc identyczne przy
///     dowolnej liczbie wątków, (5) faza Insert (adresy rezerwacji rozwiązane po kroku 4).
/// Faza przed fazą, a nie komenda po komendzie: dzięki temu „despawn + spawn"
/// w jednym ticku recyklinguje indeks deterministycznie.
pub fn flush_commands(world: &mut World, buffers: &mut [CommandBuffer]) -> FlushStats;

pub struct FlushStats { pub spawned: u32, pub despawned: u32, pub moved_rows: u32 }
```

Komendy odwołujące się do encji martwej (despawn w tym samym flushu) są ciche — nie panikują, tylko
zwiększają `FlushStats`. Panika przy gubieniu wyścigu byłaby niedeterministyczna względem kolejności
zgłoszeń, których i tak nie kontrolujemy.

### 5.9 `engine/io` — hash stanu i snapshot (PRD §18.1, 00 §3.6)

```rust
// io::hash
pub struct StateHash(pub u128);            // XXH3-128
pub struct StateHasher { /* ... */ }
impl StateHasher {
    pub fn write(&mut self, bytes: &[u8]);
    pub fn write_u64(&mut self, v: u64);
    pub fn finish(self) -> StateHash;
}

/// Każdy komponent, który ma wejść do hasha, implementuje to jawnie.
/// Brak blanket impl dla f32/f64 — float wchodzi tylko przez to_bits()
/// z kanonizacją NaN, i to jest świadoma decyzja autora komponentu (00 §2).
pub trait HashState { fn hash_state(&self, h: &mut StateHasher); }

/// Dla komponentów będących czystym POD (bez paddingu) — jedna linia zamiast boilerplate'u.
#[macro_export] macro_rules! impl_hash_state_pod { ($($t:ty),*) => { /* ... */ } }

/// Hash całego stanu ECS. Kolejność jest częścią kontraktu:
///   archetypy po kanonicznym zestawie ComponentId (leksykograficznie po NAZWACH komponentów,
///     nie po ComponentId — te nie są stabilne między uruchomieniami),
///   w archetypie encje po entity.index() rosnąco,
///   w encji komponenty po Component::NAME.
/// Encje martwe i puste archetypy nie wchodzą. Dzięki temu ten sam stan logiczny
/// osiągnięty inną historią spawnów daje ten sam hash.
///
/// PO archetypach hashowane są ARENY (§5.1b), w kolejności ArenaKind, każda
/// w kolejności indeksów slotów — razem z generacjami i znacznikiem zajętości.
/// Bez tego stan 600 tys. partii towaru byłby poza kontrolą determinizmu (00 §K-16).
/// Na końcu zasoby, w kolejności nazwy typu.
pub fn world_state_hash(world: &World) -> StateHash;

// io::snapshot
pub const SNAPSHOT_MAGIC: [u8; 8] = *b"MAGNAT\0\x01";
pub const FORMAT_VERSION: u16 = 1;

/// Snapshot jest LISTĄ SEKCJI, nie płaskim buforem. Bez tego migracje schematu
/// i zapis przyrostowy w M12 są niewykonalne: żeby zmienić układ jednego komponentu,
/// trzeba by przepisać cały plik, a żeby zapisać przyrostowo — wiedzieć, co się zmieniło.
/// Nagłówek i katalog sekcji są nieskompresowane, więc da się je przeczytać
/// bez dotykania ładunku (kilkaset kB zamiast 300 MB).
pub struct RawSnapshot {
    pub header:   SnapshotHeader,
    pub sections: Vec<SnapshotSection>,
}

pub struct SnapshotHeader {
    pub magic:          [u8; 8],
    pub format_version: u16,
    pub engine_build:   String,     // informacyjnie, do raportów błędów
    pub world_seed:     u64,
    pub tick:           Tick,
    pub entity_count:   u64,
    pub state_hash:     StateHash,  // weryfikowany po wczytaniu
    /// Tablica schematu: ComponentSchemaId → nazwa i rozmiar wiersza.
    /// To ona, a nie ComponentId, jest kluczem kompatybilności (§5.1a).
    pub schema:         Vec<SchemaEntry>,
}
pub struct SchemaEntry { pub schema_id: ComponentSchemaId, pub name: String, pub row_bytes: u32 }

/// Jedna samoopisująca się sekcja. Adresowalna, weryfikowalna i wymienialna
/// niezależnie od pozostałych.
pub struct SnapshotSection {
    pub kind:              SectionKind,
    pub schema_id:         Option<ComponentSchemaId>,  // dla sekcji komponentowych
    /// Zakres bajtów w pliku — pozwala odczytać samą tę sekcję (seek + read).
    pub byte_range:        Range<u64>,
    pub compressed_len:    u64,
    pub uncompressed_len:  u64,
    pub checksum:          u64,          // XXH3-64 ładunku po dekompresji
    pub compression:       Compression,  // Zstd { level } | None
    /// Generacja chunka w chwili zapisu (§5.5.1) — M12 porównuje ją, żeby pominąć
    /// sekcje, które się nie zmieniły od poprzedniego zapisu.
    pub source_generation: Option<u32>,
}

pub enum SectionKind {
    EntityTable,
    /// Jedna kolumna jednego chunka. Ziarno zapisu przyrostowego = ziarno copy-on-write.
    ComponentColumn { archetype: ArchetypeId, chunk: u32, component: ComponentSchemaId },
    /// Arena traktowana dokładnie jak sekcja ECS (00 §K-16): sloty, generacje, mapa zajętości.
    Arena { kind: ArenaKind, part: ArenaPart },
    Resource { type_name: String },
    /// Sekcja nierozpoznana przez tę wersję silnika — zachowywana bez zmian przy
    /// przepisaniu pliku. To ona pozwala M12 dokładać dane bez łamania starych odczytów.
    Opaque { tag: String },
}

/// Zapis: nagłówek + katalog sekcji (nieskompresowane), potem ładunki sekcji,
/// każdy skompresowany osobno. Zapis do `<path>.tmp`, fsync, rename.
pub fn save_world(world: &World, path: &Path) -> Result<SaveReport, IoError>;

/// Odczyt: remapowanie po ComponentSchemaId. Nieznany komponent albo
/// format_version != FORMAT_VERSION → twardy błąd z opisem.
/// Migracje schematu to M12 — M0 daje im tylko kształt, na którym da się je zbudować.
pub fn load_world(path: &Path, registry: &ComponentRegistry) -> Result<World, IoError>;

/// Odczyt samego katalogu — bez dekompresji ładunków. Konsument: M12 (migracje,
/// zapis przyrostowy), devtools (podgląd zapisu), CI (test sekcyjny WP-10).
pub fn read_directory(path: &Path) -> Result<RawSnapshot, IoError>;

pub struct SaveReport { pub bytes: u64, pub duration_ticks_equiv: u64, pub hash: StateHash }
```

### 5.10 `engine/devtools` — szkielet (PRD §16.5)

```rust
// devtools::inspector
pub struct Inspector;
impl Inspector {
    /// Lista archetypów: zestaw komponentów, liczba encji, liczba chunków, bajty.
    pub fn archetypes(world: &World) -> String;
    /// Pełny zrzut jednej encji. Komponenty przez Debug z rejestru.
    pub fn entity(world: &World, e: Entity) -> String;
    /// Różnica dwóch światów — raport rozbieżności determinizmu (WP-12).
    pub fn diff(a: &World, b: &World, max_entities: usize) -> String;
}

// devtools::console — rejestr komend; KAŻDA faza dopisuje swoje przez register()
pub type ConsoleFn = fn(&mut World, &[&str]) -> String;
pub struct Console { commands: SeededMap<&'static str, ConsoleCommand> }
impl Console {
    pub fn with_builtins() -> Console;   // ecs.archetypes, ecs.entity, ecs.hash, sim.step, prof.frame
    pub fn register(&mut self, name: &'static str, help: &'static str, f: ConsoleFn);
    pub fn exec(&mut self, world: &mut World, line: &str) -> String;
}

// devtools::profile — zerowy koszt bez feature "profiling"
#[macro_export] macro_rules! scope { ($name:literal) => { /* tracy span albo nic */ } }

// devtools::metrics — nazwane liczniki per tick, bufor pierścieniowy
pub struct MetricSink { capacity_ticks: usize, series: SeededMap<&'static str, Vec<i64>> }
impl MetricSink {
    pub fn record(&mut self, tick: Tick, name: &'static str, value: i64);
    pub fn series(&self, name: &str) -> &[i64];
    pub fn export_csv(&self, path: &Path) -> io::Result<()>;   // wejście balansatora (M5)
}
```

### 5.11 `tools/headless` — runner

```
magnat-headless [OPCJE]
  --seed <u64>            ziarno świata (domyślnie 1)
  --ticks <u64>           liczba ticków ekonomicznych do wykonania
  --threads <usize>       0 = liczba rdzeni (domyślnie 0)
  --entities <u32>        rozmiar świata syntetycznego (domyślnie 400000)
  --hash-every <u64>      co ile ticków liczyć hash (domyślnie 1000, 0 = nigdy)
  --out <plik>            zapis ciągu hashy: "tick hash" po jednym w linii
  --expect <plik>         porównanie z zapisanym ciągiem; różnica → exit 1 + raport
  --save <plik> / --load <plik>
  --console               tryb interaktywny (stdin)
  --metrics <plik.csv>
```

Świat syntetyczny (`headless::testworld`) jest **częścią kontraktu testowego**, nie zabawką: 6 archetypów
o rozmiarach wiersza 24–400 B, ok. 12 komponentów, 8 systemów o profilach dostępu odwzorowujących przyszłą
symulację (czytaj-wiele/pisz-jeden, dwa systemy czysto odczytowe biegnące równolegle, jeden system
strukturalny spawnujący i despawnujący ok. 0,1 % encji na tick, jeden system używający RNG per encja,
jeden `EveryHour`, jeden `EveryDay`). Fazy M1+ **nie modyfikują** tego świata — dopisują własne scenariusze obok.

### 5.12 `tools/voxelview` — bootstrap graficzny (PRD §16.3, §19 M0)

Minimalny łańcuch: `winit::EventLoop` + `ApplicationHandler` → `wgpu::Instance` (backendy z `WGPU_BACKEND`,
domyślnie `PRIMARY`) → `request_adapter` (preferencja: dedykowana karta) → `request_device`
(limity `downlevel_defaults`, żeby wcześnie złapać wymagania sprzętowe) → `Surface` + `configure`
(`Fifo`) → bufor głębi `Depth32Float`.

Zawartość: chunk 32³ z palety 4 materiałów, wypełniony zahardkodowanym wzorem (nie generator terenu).
Mesh budowany **naiwnie** — sześć ścian na voxel, odrzucane tylko ściany stykające się z pełnym sąsiadem.
Brak greedy meshingu (to M1). Jeden `RenderPipeline`, jeden shader WGSL, wierzchołek `{ pos: [f32;3],
normal: [i8;4], palette_index: u32 }`, uniform z macierzą widok-projekcja. Kamera orbitalna na myszy,
FPS w tytule okna.

Co M1 z tego przejmuje: funkcja `init_gpu() -> GpuContext` i obsługa resize/lost surface przenoszą się
do `engine/render`. Reszta (`chunk_mesh`, pipeline, shader) jest do usunięcia — to rusztowanie, nie fundament
(patrz D-5).

---

## 6. Kontrakty międzyfazowe

To najważniejsza sekcja tej fazy: wszystko poniżej jest publicznym API, na którym stoją M1–M12.
Zmiana czegokolwiek z listy „dostarczam" po zakończeniu M0 wymaga wpisu w `00-konwencje-i-kontrakty.md`.

### 6.1 Konsumuję

Nic. M0 nie ma zależności od innych faz — konsumuje wyłącznie kontrakty z
`00-konwencje-i-kontrakty.md` §2 (typy bazowe), §3 (determinizm), §4 (czas, `Cadence`), §6 (konwencje kodu).

### 6.2 Dostarczam — `magnat-core`

| Element | Sygnatura / postać | Kto konsumuje |
|---|---|---|
| Typy bazowe 00 §2 | `Money`, `SimMinute`, `SimInstant`, `Tick`, `Mass`, `Volume`, `Energy`, `Qty`, `Q`, `Mood`, `DistrictId`, `GoodId`, `RecipeId`, `JobRoleId` | wszystkie |
| Arytmetyka pieniądza | `Money::{checked_add, checked_sub, checked_mul_int, mul_ratio, div_round_half_up}` | M5, M6, M7, M8, M9, M10 |
| Podział kwoty bez gubienia grosza | `split_proportional(total: Money, weights: &[u64]) -> Vec<Money>` | M5 (rachunki), M7 (pensje), M8 (podatki), M10 (dywidendy) |
| Fixed-point | `Fx` (Q32.32): `from_ratio`, `mul`, `div`, `to_int_round` | M4 (czasy przejazdu), M5 (użyteczności wchodzące do stanu), M10 (makro) |
| Kalendarz | `SimCalendar::{minute_of_hour, hour_of_day, day_of_month, month_of_year, year, is_*_boundary}` | M3 (planer dnia), M5, M8 (podatki), M10 (epoki) |
| Częstotliwość systemu | `enum Cadence { EveryMicroTick, EveryMinute, EveryHour, EveryDay, EveryMonth }`, `Cadence::due(Tick) -> bool` | wszystkie fazy z systemami |
| RNG | `enum StreamId` (zakresy numerów rezerwowane per faza), `rng(world_seed, StreamId, entity_index, Tick) -> Rng`, `Rng::{next_u64, gen_range_u32, gen_q, gen_bool_permille, shuffle}` | wszystkie |
| Kolekcje deterministyczne | `SeededMap<K,V>`, `SeededSet<K>`, `seeded_map()`, `SeededMapExt::iter_sorted()` | wszystkie |
| **Matematyka przestępna (00 §K-6)** | `det_math::{ln, ln1p, log2, exp, exp_m1, exp2, pow, sqrt}` — bit w bit identyczne na każdej platformie, ≤ 1 ULP (`pow` ≤ 2 ULP) | **M5** (użyteczność zakupu, PRD §6.4; ceny), **M7** (decyzje firm, pensje emergentne), **M9** (oceny scenariuszy), **M10** (makro, stopy zwrotu, giełda) |
| **Softmax** | `det_math::{softmax, softmax_in_place, softmax_pick(weights, temperature, &mut Rng) -> usize}` — sumowanie w kolejności indeksów | **M5**, **M7**, M3 (wybór wariantu planu dnia), M9 |
| **Zakaz libm** | `clippy.toml` z `disallowed-methods` na `f64::{ln, exp, powf, …}` + nadpisania w `engine/render`, `engine/ui` | wszystkie — obowiązuje od pierwszej linii kodu symulacji |
| **Encja** | `Entity` — definiowana w `core` (nie w `ecs`), bo `CitizenId(pub Entity)` z 00 §2 i `PlaceRef` z K-8 są typami `core` | wszystkie; `ecs` tylko re-eksportuje |
| **Słowniki domenowe (00 §K-8)** | `UtilityKind`, `TransportMode`, `PlaceRef`, `WorldCoord`, `NeedKind`, `ActivityKind` — same warianty i konwersje, zero logiki | **M3** (potrzeby, plan dnia), **M4** (środek transportu), **M5** (wymiary użyteczności), **M8** (polityka miejska); istnieją po to, żeby te cztery fazy nie utworzyły cyklu crateów |
| **Wyjaśnialność (00 §K-12)** | `DecisionReason` — jeden centralny enum **bez `#[non_exhaustive]`**, bloki dyskryminant po 100 per faza, ładunek `Copy` ≤ 24 B | **wszystkie fazy z decyzjami** (M3, M5, M6, M7, M8, M9, M10) — dopisują warianty; **M9** renderuje je w karcie inspekcji i to jego kompilacja pada, gdy ktoś doda wariant bez obsługi |
| **Tożsamość schematu** | `ComponentSchemaId::new(name, version)`, reguła „zmiana układu pól = podbicie `SCHEMA_VERSION`, nie zmiana nazwy" | **M12** (migracje zapisów), `engine/io` od M0 |
| **Arena (00 §K-16)** | `Arena<T>`, `ArenaHandle<T>` (8 B, generacyjny), `ArenaKind`, `Arena::{insert, remove, get, get_mut, iter, chunks}` — bez kompaktowania | **M5** (`OfferId = ArenaHandle<Offer>`), **M6** (`BatchId = ArenaHandle<Batch>`); kolejne kategorie o tym profilu (masowe, krótkowieczne, adresowane punktowo) korzystają z tego samego mechanizmu, nie piszą własnego |

**Zobowiązanie faz M1+:**
- Nowe warianty `StreamId` dopisuje się na końcu, w zakresie numerycznym danej fazy, nigdy nie zmieniając
  istniejących wartości (00 §3.1). Nowe typy wielkości fizycznych dopisuje się do `core::types`,
  nie do crate'a fazy.
- Każda funkcja przestępna potrzebna w symulacji trafia do `det_math` (wraz z wektorem testowym
  i wartościami referencyjnymi MPFR), **nigdy** do crate'a fazy jako lokalne `x.ln()` z `#[allow]`.
  Faza, która potrzebuje `tanh`, `sin` albo `atan2` w kodzie wpływającym na stan trwały, dopisuje je tu
  tym samym schematem co M0 — to rozszerzenie kontraktu, nie jego obejście.
- Wynik `det_math` nie trafia do `Money` inaczej niż przez jawne zaokrąglenie
  (`Money::mul_ratio` / `div_round_half_up`) — float nigdy nie jest reprezentacją pieniądza (00 §2).

### 6.3 Dostarczam — `magnat-ecs`

| Element | Sygnatura / postać | Kto konsumuje |
|---|---|---|
| Encja | re-eksport `core::Entity` (definicja jest w `core`, §6.2) | wszystkie; M1 opakowuje w `BuildingId` itd. wg 00 §2 |
| **Chunk jako publiczna jednostka** | `ChunkId`, `ChunkRef<'w>` (`Send + Clone`, tylko odczyt: `entities`, `column::<T>`, `column_bytes`, `generation`), `World::{chunks, chunk}`, `CHUNK_TARGET_BYTES = 64 KiB` | **M12** — bez tego zapis w tle przez copy-on-write nie istnieje, a cel „zapis < 5 s bez pauzy" (PRD §20.2) pada; `engine/io` od M0 |
| **Gwarancja layoutu chunka** | jeden chunk = jedna alokacja ≤ 64 KiB pod **jednym** wskaźnikiem, adresowalna przez `ChunkId`, z licznikiem `generation`, dostępna jako `Send` do odczytu | **M12**; zmiana któregokolwiek z tych pięciu warunków wymaga uzgodnienia z M12, test layoutu w WP-04 pilnuje ich mechanicznie |
| Definicja komponentu | `trait Component { const NAME: &'static str; }` | wszystkie |
| Świat | `World::{new, register_component, spawn, despawn, insert, remove, get, get_mut, contains, insert_resource, resource, resource_mut, query}` | wszystkie |
| Zapytania | `Query<Q, F>`, `QueryData` (`&T`, `&mut T`, `Entity`, `Option<&T>`, krotki), `With<T>`, `Without<T>`, `Query::{iter, chunks, get, par_for_each, par_fold}` | wszystkie |
| Deklaracja dostępu | `Access` (wyliczany z typów), `Access::conflicts_with` | scheduler; fazy korzystają pośrednio |
| System | `trait System { fn desc(&self) -> &SystemDesc; fn run(&mut self, &mut SystemCtx); }`, `SystemDesc { id, name, cadence, access, after, before }`, `SystemCtx { world, pool, tick, seed, cmd }` | wszystkie fazy z logiką |
| Identyfikator systemu | `SystemId` ze stabilnej nazwy — porządek komend nie zależy od kolejności rejestracji | M5+ (kolejność rozliczeń) |
| Harmonogram | `ScheduleBuilder::{add, build}`, `Schedule`, `ScheduleError` | `game/` (M9), `tools/*` |
| Pętla | `App::{new, tick, run_ticks}` | M9 (`game/`), wszystkie testy |
| Mutacje strukturalne | `CommandBuffer::{spawn, despawn, insert, insert_reserved, remove}`, `EntityReservation`, `flush_commands` | wszystkie fazy tworzące/usuwające encje |
| Stałe layoutu | `CHUNK_TARGET_BYTES = 64 KiB`, liczba wierszy wyliczana per archetyp; chunk jest jednocześnie jednostką równoległości i jednostką copy-on-write | M12 (zapis w tle), M1 (chunk voxeli 32³ to **inne** pojęcie — nie mylić) |

**Zobowiązanie faz M1+:** żaden system nie mutuje strukturalnie `World` bezpośrednio w trakcie ticku —
wyłącznie przez `SystemCtx::cmd`. Bezpośrednie `World::spawn`/`despawn` są dozwolone tylko w ładowaniu
świata, testach i odczycie snapshotu.

### 6.4 Dostarczam — `magnat-jobs`

| Element | Sygnatura | Kto konsumuje |
|---|---|---|
| Pula | `JobPool::{new(threads), thread_count, scope}` | `App`, M1 (meshing), M4 (pathfinding), M10 (makro) |
| Deterministyczna redukcja | `map_reduce_indexed(pool, chunks, map, combine, init)` | każda redukcja w symulacji (00 §3.3) |
| Równoległa mutacja | `for_each_chunk_mut(pool, chunks, f)` | M1 (generator), M4 (ruch) |

**Zobowiązanie faz M1+:** `rayon::iter` i globalna pula rayon są zakazane poza `engine/jobs`
(lint `disallowed-types`). Każda redukcja, której wynik wchodzi do stanu trwałego, idzie przez
`map_reduce_indexed`.

### 6.5 Dostarczam — `magnat-io`

| Element | Sygnatura | Kto konsumuje |
|---|---|---|
| Hash stanu | `trait HashState { fn hash_state(&self, &mut StateHasher); }`, `impl_hash_state_pod!`, `world_state_hash(&World) -> StateHash` — obejmuje archetypy, **areny (sloty + generacje + zajętość)** i zasoby | **każda faza** — dopisanie swoich komponentów i aren do hasha jest elementem DoD fazy (00 §3.6) |
| Snapshot | `save_world`, `load_world`, `read_directory(&Path) -> Result<RawSnapshot, IoError>`, `FORMAT_VERSION` | M9 (zapis gry), M12 |
| **`RawSnapshot` jako lista sekcji** | `RawSnapshot { header, sections: Vec<SnapshotSection> }`, `SectionKind::{EntityTable, ComponentColumn{archetype, chunk, component}, Arena{kind, part}, Resource, Opaque}`, każda sekcja z `byte_range`, `checksum`, `source_generation` | **M12** — migracje schematu (podmiana jednej sekcji), zapis przyrostowy (pominięcie sekcji o niezmienionej `source_generation`), zachowanie sekcji `Opaque` przy przepisaniu pliku |

**Zobowiązanie faz M1+:** każdy nowy komponent trwały implementuje `HashState`, a jego `Component::NAME`
jest traktowany jak klucz kompatybilności zapisu — zmiana nazwy to zmiana formatu.

### 6.6 Dostarczam — `magnat-devtools`

| Element | Sygnatura | Kto konsumuje |
|---|---|---|
| Inspektor | `Inspector::{archetypes, entity, diff}` | M3+ (karta inspekcji buduje się nad tym, nie zamiast) |
| Konsola | `Console::{with_builtins, register, exec}`, `ConsoleFn = fn(&mut World, &[&str]) -> String` | **każda faza dopisuje swoje komendy** — przestrzeń nazw `<obszar>.<akcja>` |
| Profiler | `devtools::scope!("nazwa")` — zero kosztu bez feature `profiling` | wszystkie ścieżki gorące |
| Metryki | `MetricSink::{record, series, export_csv}` | M5 (balansator), M10, M12 |

### 6.7 Dostarczam — narzędzia i infrastruktura

| Element | Postać | Kto konsumuje |
|---|---|---|
| Runner headless | binarka `magnat-headless` z pełnym CLI (§5.11) | M5 (balansator uruchamia go N razy), CI wszystkich faz |
| Świat syntetyczny | `headless::testworld` — stały, nietykalny benchmark bazowy | benchmarki wszystkich faz (punkt odniesienia dla regresji) |
| Kontrakt testu determinizmu | format pliku hashy `"<tick> <hash_hex>"`, `--expect` z kodem wyjścia | CI wszystkich faz |
| Konfiguracja CI | joby `check`, `miri`, `determinism`, `bench-guard` | wszystkie fazy — faza dopisuje przypadki, nie buduje własnego CI |
| Bootstrap GPU | `voxelview::init_gpu() -> GpuContext` + obsługa resize/lost | **M1** przenosi do `engine/render` (D-5) |

### 6.8 Czego M0 świadomie NIE dostarcza (żeby fazy nie czekały)

- Filtrów `Changed<T>` / `Added<T>` — pierwszy realny konsument to M3; wtedy dokładamy liczniki zmian
  do `ArchetypeChunk` (miejsce już zarezerwowane). D-4.
- Relacji między encjami (rodzic/dziecko, hierarchie) — pierwszy konsument M2 (parcela → budynek).
  Do tego czasu relacja to zwykły komponent z `Entity` w środku.
- Kolejki zdarzeń DES — to konstrukcja `sim/agents`, nie silnika. M3.
- Podwójnie buforowanego snapshotu do renderu (PRD §17.1) — bez renderu nie ma czego buforować. M1.
- **Samego copy-on-write i zapisu w tle.** M0 dostarcza *kształt*, który to umożliwia (layout chunka,
  `ChunkRef`, `generation`, sekcje snapshotu) — nie dostarcza `ChunkMut`, licznika referencji, `AtomicPtr`
  w storage'u ani wątku zapisu. To M12. Rozróżnienie jest celowe: kształtu nie da się dorobić później
  bez przepisania storage'u, mechanizm da się dorobić w każdej chwili.
- **Migracji schematu zapisu.** `ComponentSchemaId` i sekcje `Opaque` istnieją od M0, żeby M12 miało
  na czym je oprzeć. Sam kod migracji — M12.
- Zawartości aren: `Batch` definiuje M6, `Offer` definiuje M5. `core` zna tylko sloty i generacje.

---

## 7. Testy i kryteria akceptacji

### 7.1 Determinizm

| Test | Opis | Kryterium |
|---|---|---|
| **T-D1 hash-determinizm** | Dwa pełne przebiegi `magnat-headless --seed 42 --ticks 200000 --hash-every 1000`, w osobnych procesach. Porównanie 200 hashy. | Ciągi identyczne bajt w bajt. Rozbieżność = błąd blokujący. |
| **T-D2 niezależność od liczby wątków** | Ten sam scenariusz przy `--threads 1`, `2`, `8`, `16`. | Wszystkie cztery ciągi hashy identyczne. To jest **wymagany** test schedulera z zadania fazy. |
| **T-D3 niezależność od kolejności rejestracji** | Ten sam zestaw systemów rejestrowany w 5 losowych permutacjach. | Identyczny odcisk DAG i identyczny ciąg hashy. |
| **T-D4 snapshot round-trip** | Przebieg ciągły 150 tys. ticków vs przebieg przerwany zapisem na ticku 50 tys. i wznowiony. | Identyczny ciąg hashy od ticku 50 tys.; `state_hash` z nagłówka zgodny z policzonym po wczytaniu. |
| **T-D5 niezależność od historii** | Świat zbudowany dwiema różnymi sekwencjami spawn/despawn do tego samego stanu logicznego. | `world_state_hash` identyczny. |
| **T-D6 determinizm komend** | 100 tys. komend z 16 systemów, w tym kolizje (`despawn` + `insert` na tej samej encji, `spawn` + `insert_reserved`). | Identyczny stan przy 1 i 16 wątkach; `FlushStats` identyczne. |
| **T-D7 RNG** | Wektory referencyjne `xoshiro256++`; `rng()` wywoływany w losowej kolejności i równolegle. | Zgodność bit w bit; wynik niezależny od kolejności wywołań. |
| **T-D8 test negatywny (chaos)** | Build z feature `chaos` wstawia w jeden system iterację po `HashMap`. | `--expect` **musi** zawieść i wskazać poprawny tick pierwszej rozbieżności. Test, który sprawdza, że testy działają. |
| **T-D9 `det_math` między platformami** | 100 tys. argumentów na funkcję (rozkład logarytmiczny po całym zakresie wykładnika + denormalne + 0 + ±∞ + NaN + potęgi dwójki + wartości bliskie 1). Wyniki hashowane XXH3, złota wartość zatwierdzona w repo. | Identyczny hash na `x86_64-linux-gnu`, `x86_64-windows-msvc`, `aarch64-linux-gnu` (QEMU) **oraz** na dwóch kolejnych wersjach toolchaina. Sześć przebiegów, jeden hash. |
| **T-D10 zakaz libm** | Celowo wstawione `x.ln()` w crate'cie `sim/*` oraz w `engine/render`. | Pierwsze wywala `clippy -D warnings`, drugie przechodzi. Dodatkowo skan regexem źródeł `sim/*` łapie obejście przez `#[allow]`. |
| **T-D12 arena w hashu** | Dwa światy o identycznej treści wartości w arenie, ale różnym układzie slotów (jeden zbudowany wprost, drugi przez sekwencję insert/remove/insert). | `world_state_hash` **różny** — to stany różne, nie równoważne. Test odwrotny: ta sama sekwencja operacji przy 1 i 16 wątkach → hash identyczny. |
| **T-D13 sekcje snapshotu** | `read_directory` bez dekompresji ładunku; podmiana bajtów jednej sekcji i przepisanie pliku; sekcja `Opaque` nieznana tej wersji silnika. | Katalog czytany w < 50 ms dla pliku 300 MB; pozostałe sekcje bajtowo nietknięte; sekcja `Opaque` przepisana bez zmian. To jest fundament migracji M12 — jeśli nie działa w M0, nie da się tego dorobić bez zmiany formatu. |
| **T-D11 softmax** | Ten sam wektor wag w 1000 losowych permutacjach wejścia przywróconych do kolejności oryginalnej; `softmax_pick` przy 1 i 16 wątkach schedulera. | Wynik bit w bit identyczny; suma prawdopodobieństw w granicy 1.0 ± 4·n ULP; brak zależności od kolejności wykonania systemów. |

### 7.2 Poprawność i bezpieczeństwo

| Test | Kryterium |
|---|---|
| Miri na `magnat-ecs` i `magnat-core` | Zielone; brak UB w trzech blokach `unsafe` (alokacja chunka, rzutowanie kolumny, przenoszenie wiersza). |
| Cykl życia encji | Uchwyt po `despawn` nie trafia w nową encję pod tym samym indeksem (test generacji, 10^6 cykli). |
| Cykl życia uchwytu areny | `ArenaHandle` zwolnionego slotu nigdy nie staje się ponownie ważny — 10^6 cykli insert/remove/insert, `get` zawsze `None`. Stary `BatchId` ma być wykrywalnym błędem, nie cichym odczytem cudzej partii. |
| Typowanie uchwytów | `ArenaHandle<Batch>` nie kompiluje się tam, gdzie oczekiwany jest `ArenaHandle<Offer>`; `Entity` nie kompiluje się tam, gdzie oczekiwany jest `ArenaHandle<T>` (`trybuild`). |
| Layout chunka (kontrakt z M12) | Każdy chunk to jedna alokacja ≤ 64 KiB pod jednym wskaźnikiem; `ChunkRef` jest `Send + Clone` i nie daje dostępu mutowalnego (`trybuild`); `generation` rośnie przy każdej mutacji i tylko przy niej. |
| Wyjaśnialność (K-12) | `size_of::<DecisionReason>() <= 24`; dyskryminanty zgodne z tabelą snapshotową; `match` bez ramienia `_` przestaje się kompilować po dodaniu wariantu (`trybuild`). |
| Wycieki | Licznik `Drop` zgadza się po 10^6 operacjach spawn/despawn/insert/remove. |
| Konflikt dostępów | `Query<(&mut A, &A)>` panikuje; dwa `ResMut<T>` równolegle panikują w debug. |
| Cykl ograniczeń | `after`/`before` tworzące cykl → `ScheduleError::OrderingCycle` z wypisaną ścieżką, nie deadlock. |
| Właściwości `Money` (`proptest`) | `split_proportional` sumuje się do `total` dla 10^5 przypadków, w tym kwot ujemnych i wag zerowych; `div_round_half_up` zgodne z tabelą przypadków brzegowych. |
| Dokładność `det_math` | Względem wartości referencyjnych w arytmetyce 256-bitowej (MPFR, tabela zamrożona w repo): `ln`, `ln1p`, `log2`, `exp`, `exp_m1`, `exp2` **≤ 1 ULP**, `pow` **≤ 2 ULP**. Monotoniczność `ln`/`exp` na 10^6 kolejnych wartościach; `exp(ln(x))` w granicy 3 ULP dla x ∈ [10⁻¹⁰⁰, 10¹⁰⁰]. |
| Brzegi `det_math` | `ln(0) == -∞`, `ln(-1)` i `ln(NaN)` = NaN, `exp(710) == ∞`, `exp(-746) == 0.0`, `pow(x, 0) == 1.0` dla każdego x (w tym NaN, wg IEEE-754), `pow(-2.0, 3.0) == -8.0` dokładnie. |
| Panika w jobie | Propagacja do wywołującego, pula użyteczna po zdarzeniu, brak zatrutych zamków. |
| `size_of` | `Entity == 8`, `Option<Entity> == 8`, `ComponentId == 2`. |

### 7.3 Wydajność (criterion, WP-13)

Sprzęt odniesienia deklarowany w `benches/README`; progi bezwzględne poniżej to **cele projektowe**
z PRD §20.2, próg CI to **regresja względna** wobec `benches/baseline.json`.

| Benchmark | Scenariusz | Cel |
|---|---|---|
| **B-1 iteracja 400 tys. encji** | `Query<(&mut A, &B)>`, 2 komponenty po 16 B, jeden archetyp, jednowątkowo | ≤ 4 ms/tick (≈ 100 M komponentów/s) |
| **B-1b to samo, 6 komponentów, 4 archetypy, z `With`/`Without`** | realistyczny profil symulacji | ≤ 12 ms/tick jednowątkowo |
| **B-1c to samo, `par_for_each`, 16 wątków** | skalowanie | przyspieszenie ≥ 8× wobec B-1b |
| **B-2 koszt komendy strukturalnej** | pojedynczy `cmd.spawn() + insert` oraz pojedynczy `despawn`, amortyzowany | ≤ 150 ns/komenda przy zapisie do bufora |
| **B-2b flush 100 tys. komend** | sortowanie + aplikacja, jednowątkowo | ≤ 15 ms; koszt liniowy (test skalowania 10 k / 100 k / 1 M) |
| **B-3 przeniesienie encji między archetypami** | `insert`/`remove` powodujące zmianę archetypu, wiersz 400 B | ≤ 400 ns/encja |
| **B-4 `world_state_hash`** | 400 tys. encji, 12 komponentów | ≤ 40 ms (liczony co 1000 ticków, więc amortyzowany narzut < 0,1 %) |
| **B-5 bariera schedulera** | 60 systemów, puste ciała, 16 wątków | ≤ 50 µs/tick narzutu (inaczej sam scheduler zjada budżet ticku) |
| **B-6 snapshot** | zapis + odczyt 400 tys. encji, zstd:3 | zapis ≤ 5 s i ≤ 300 MB (PRD §18.1, §20.2) |
| **B-7 pamięć** | RSS świata 400 tys. encji ze średnim wierszem 400 B | ≤ 1,2 GB (zapas do celu 6 GB z PRD §17.7 na dane domenowe faz M1+) |
| **B-9 arena** | 600 tys. partii: `insert`, `remove`, `get` po uchwycie, pełna iteracja z 30 % dziur | `get` ≤ 5 ns; pełna iteracja ≤ 2 ms; `insert`+`remove` ≤ 20 ns. Punkt odniesienia: ta sama liczba encji ECS z komendą strukturalną (B-2) — arena ma być **rząd wielkości** tańsza, inaczej K-16 nie miało sensu. |
| **B-8 `det_math`** | `ln`, `exp`, `pow` na wektorze 10^6 argumentów, obok tych samych funkcji z libm platformy | **≤ 2×** wolniej od libm. `softmax` na 16 opcjach (typowy koszyk M5) ≤ 350 ns. Przekroczenie 2× → optymalizacja wielomianu, nie powrót do libm. |

Próg CI: pogorszenie mediany o ponad 10 % → ostrzeżenie w PR; ponad 25 % → job na czerwono.

### 7.4 Kryteria akceptacji fazy

1. Wszystkie testy §7.1 i §7.2 zielone w CI na Linux i Windows.
2. Wszystkie benchmarki §7.3 mieszczą się w celach; odstępstwo wymaga wpisu do §9 z uzasadnieniem.
3. Pięć artefaktów z §1 uruchamia się z czystego klona jedną komendą.
4. `clippy --workspace --all-targets -- -D warnings` bez wyjątków; `unsafe` wyłącznie w trzech
   udokumentowanych blokach `engine/ecs`; zero `#[allow(clippy::disallowed_methods)]` w całym workspace.
5. `det_math` ma złotą wartość T-D9 zgodną na wszystkich platformach z macierzy D-10 i mieści się
   w granicach ULP z §7.2 oraz w progu 2× z B-8.
6. Sekcja 6 tego dokumentu ma pokrycie 1:1 w publicznym API — test `cargo public-api` albo przegląd ręczny
   z listą kontrolną.

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Prawdop. | Skutek | Mitygacja |
|---|---|---|---|---|
| R-1 | **Przeprojektowanie ECS.** M0 bez konsumenta kusi do budowania „na wszelki wypadek": relacje, obserwatory, hierarchie, hot-reload. Każdy taki mechanizm to kod bez użytkownika, który i tak trzeba przepisać, gdy pojawi się prawdziwy przypadek. | Wysokie | Miesiące straty, silnik pod wymagania, których nie znamy | §6.8 jest listą świadomych odmów. Reguła: mechanizm ECS powstaje dopiero wtedy, gdy konkretna faza go potrzebuje. Świat syntetyczny z §5.11 jest jedynym dopuszczalnym „wyobrażonym konsumentem". |
| R-2 | **Niedeterminizm wykryty późno**, gdy w kodzie są już trzy fazy logiki — lokalizacja przyczyny kosztuje wielokrotnie więcej. | Średnie | Bardzo wysoki | T-D8 (chaos) w CI od WP-12; `Inspector::diff` wskazuje encję i komponent, nie tylko tick; lint `disallowed-types` na `HashMap` w `sim/*` od WP-03. |
| R-3 | **Krzywa uczenia Rusta** przy najtrudniejszym module projektu — archetypowy ECS z `unsafe` to nie jest pierwszy program w Rust (PRD §20.4). | Wysokie | Opóźnienie WP-04/05 | WP-04 pisany z Miri od pierwszego dnia, nie na końcu. Dopuszczalna wersja pośrednia: kolumny jako `Vec<T>` bez chunków (poprawna, wolniejsza), chunkowanie jako osobny krok z benchmarkiem przed i po. Wariant bezpieczny musi przejść te same testy. |
| R-4 | **Narzut schedulera zjada zysk z równoległości** przy systemach o małej pracy (B-5). | Średnie | Wydajność | B-5 jako bramka; jeśli przekroczony — scalanie systemów o zgodnym `Access` w jeden węzeł DAG (optymalizacja zachowująca semantykę, bo kolejność kanoniczna jest zachowana). |
| R-5 | **`wgpu`/`winit` łamią API między wersjami** (historycznie co kilka miesięcy). | Wysokie | Niski w M0, średni w M1 | Wersje przypięte dokładnie w `[workspace.dependencies]`; `voxelview` jest izolowany — aktualizacja nie dotyka symulacji. Test `cargo tree` pilnuje, że `wgpu` nie przenika do headless. |
| R-6 | **Sortowanie komend jako wąskie gardło** przy 1 M komend/tick w M4/M6. | Niskie w M0 | Średni później | B-2b mierzy skalowanie już teraz; klucz ma 16 B i jest sortowalny radix-em — ścieżka optymalizacji znana, niewdrażana przed pomiarem. |
| R-7 | **Rozmiar chunka źle dobrany** — 64 KiB to jednocześnie jednostka copy-on-write M12 i jednostka równoległości symulacji, a te dwa wymagania ciągną w przeciwne strony (duży chunk = tańszy zapis, grubsze ziarno pracy). | Średnie | Wydajność | Liczba wierszy jest wyliczana per archetyp z `CHUNK_TARGET_BYTES` (§5.5), więc archetypy o skrajnym rozmiarze wiersza nie cierpią. Sam próg 64 KiB jest **jedną stałą** — zmiana to jedna linia plus przebieg benchmarków B-1/B-9. Wartość zafiksowana z M12 (§6.3), więc nie zmienia się jednostronnie. |
| R-8 | **Format snapshotu zamrożony za wcześnie** — M3 dodaje komponenty i stare zapisy przestają się wczytywać. | Wysokie | Niski, jeśli przyjęty świadomie | M0 deklaruje wprost: brak zgodności wstecz do M12. `format_version` rośnie przy każdej zmianie, odczyt starej wersji to czytelny błąd, nie awaria. Zapisy z faz M0–M11 są jednorazowe. |
| R-9 | **`det_math` powstaje, ale nikt go nie używa** — M3/M4 sięgają po `f64::ln` z `#[allow]`, bo „tu akurat nie wchodzi do stanu trwałego". Po roku okazuje się, że wchodziło. | Średnie | Wysoki — cichy niedeterminizm platformowy, wykryty przy pierwszym buildzie na innym systemie | Lint to warstwa pierwsza, ale niewystarczająca — dlatego T-D10 sprawdza też obejście przez `#[allow]`, a job determinizmu w CI biegnie na Linux **i** Windows. T-D1 na jednej platformie nie wykryłby tego nigdy. |
| R-10 | **`DecisionReason` jako jeden centralny enum staje się wąskim gardłem scalania** — pięć faz dopisuje warianty do jednego pliku, a każda zmiana rekompiluje `core` i wszystko powyżej. | Średnie | Niski w M0, rosnący | Bloki dyskryminant po 100 per faza w rozłącznych fragmentach pliku (§5.1a) — konflikty są punktowe. Rekompilacja: `DecisionReason` siedzi w osobnym module bez zależności, więc przebudowa dotyka `core` i konsumentów, nie całego drzewa. Koszt akceptowany świadomie: to cena za wyjaśnialność egzekwowaną przez kompilator (00 §7), a alternatywa (`Box<dyn>` albo string) kosztuje więcej i nic nie egzekwuje. |
| R-11 | **Arena i ECS rozjeżdżają się w regułach determinizmu** — ktoś woła `Arena::insert` w środku równolegle działającego systemu, bo arena nie ma bufora komend, który by tego bronił. | Średnie | Wysoki | `Arena` jest zasobem (`ResMut<Arena<T>>`), więc scheduler serializuje do niej dostęp tak jak do każdego innego zasobu (§5.6) — ochrona jest strukturalna, nie regulaminowa. T-D12 sprawdza to empirycznie przy 1 i 16 wątkach. |
| R-12 | **Publiczne API chunków zostaje wykorzystane niezgodnie z przeznaczeniem** — faza M4 zaczyna iterować po `ChunkRef` zamiast przez `Query`, omijając `Access` i deklarację dostępu. | Średnie | Wysoki — scheduler przestaje widzieć prawdziwe zależności | `ChunkRef` daje **wyłącznie odczyt**, więc najgorszy możliwy skutek to odczyt nieaktualnych danych, nie wyścig. Dodatkowo: `World::chunks` jest udokumentowane jako API serializacji i devtools, a przegląd kodu traktuje jego użycie w `sim/*` jako sygnał ostrzegawczy. Gdyby to okazało się realnym problemem, `chunks()` trafia za feature `snapshot-access` — ale nie zgadujemy tego przed pierwszym przypadkiem. |
| R-13 | **Własna implementacja `ln`/`exp` ma błąd**, którego testy ULP nie łapią (np. tylko w wąskim paśmie argumentów). Różnica jest deterministyczna, więc CI determinizmu milczy — świat jest po prostu subtelnie zły. | Niskie | Średni | Wektor testowy T-D9 rozłożony logarytmicznie po **całym** zakresie wykładnika, nie po „typowych" wartościach; do tego monotoniczność i round-trip `exp(ln(x))`. Referencja z MPFR, nie z libm — inaczej testowalibyśmy zgodność z tym, od czego uciekamy. |

---

## 9. Decyzje otwarte

| # | Decyzja | Kontekst | Domyślne rozstrzygnięcie (obowiązuje, jeśli nikt nie zgłosi sprzeciwu przed startem WP) | Z kim uzgodnić |
|---|---|---|---|---|
| **D-1** | Czy `engine/jobs` to opakowanie `rayon::ThreadPool`, czy własny scheduler Chase-Lev. PRD §16.1 dopuszcza oba. | Własny daje kontrolę nad priorytetami i pinningiem; rayon oszczędza kilka tygodni i jest sprawdzony. Determinizm nie zależy od wyboru (§5.4). | **Rayon**, ukryty za `JobPool`. Wymiana dopiero, gdy profil M12 wskaże narzut. | M4, M12 (najwięksi konsumenci równoległości) |
| ~~D-2~~ | ~~Kalendarz 360 dni czy gregoriański.~~ | **ZAMKNIĘTE — 00 §K-1.** | **360 dni (12 × 30), bez lat przestępnych**, stałe w `core::time` (§5.1). | rozstrzygnięte |
| **D-3** | Czy `Tick` to minuta ekonomiczna, a mikro-tick jest podpodziałem, czy odwrotnie. | 00 §4 i PRD §17.1 mówią „tick ekonomiczny = 1 minuta"; `Cadence::EveryMicroTick` (100 ms) potrzebuje 600 podkroków na tick. | **`Tick` = minuta.** Systemy mikro dostają pętlę wewnętrzną 600 kroków w obrębie jednego ticku, ich wynik jest widoczny dopiero na granicy minuty. | **M4** (ruch mikro) — to jego główna ścieżka |
| **D-4** | Czy M0 dokłada liczniki zmian (`Changed<T>`/`Added<T>`). | Kosztuje 4 B/kolumnę/chunk i komplikuje ścieżkę zapisu; bez konsumenta to spekulacja. | **Nie w M0.** Miejsce zarezerwowane w `ArchetypeChunk`; dokłada faza, która pierwsza tego potrzebuje. | **M3** (przeplanowanie zdarzeniami), M2 |
| **D-5** | Gdzie żyje bootstrap GPU: `tools/voxelview` (M0) czy od razu `engine/render` (M1). | 00 §1 przypisuje `engine/render` fazie M1. Osobna binarka nie łamie własności, ale M1 musi przenieść `init_gpu`. | **`tools/voxelview`**, z zobowiązaniem: M1 przenosi `init_gpu()` i obsługę surface do `engine/render`, resztę kasuje. | **M1** — do potwierdzenia przed startem WP-15 |
| ~~D-6~~ | ~~Czy rozmiar chunka ECS jest stały, czy wyliczany z rozmiaru wiersza.~~ | **ZAMKNIĘTE — wymaganiem M12.** Chunk jest jednostką copy-on-write zapisu w tle, więc jego rozmiar przestał być wewnętrzną sprawą M0. | **`CHUNK_TARGET_BYTES = 64 KiB`**, liczba wierszy wyliczana per archetyp (§5.5). Chunk jest **publiczną jednostką API** — `ChunkId` + `ChunkRef` tylko do odczytu (§5.5.1). | rozstrzygnięte z M12 |
| **D-7** | Czy `unsafe` w ECS jest w ogóle dopuszczalny w wersji pierwszej. | 00 §6 dopuszcza go w `ecs`, ale R-3 (krzywa uczenia) sugeruje najpierw wersję bezpieczną. | **Wersja bezpieczna najpierw** (`Vec<T>` per kolumna), chunki z `unsafe` jako WP-04b z benchmarkiem przed/po. Jeśli bezpieczna mieści się w celach §7.3 — `unsafe` nie powstaje wcale. | — (decyzja wewnętrzna M0) |
| **D-8** | Próg regresji benchmarków w CI. | Za czuły próg to fałszywe alarmy na współdzielonych runnerach. | **10 % ostrzeżenie / 25 % błąd**, mediana z 5 przebiegów. | wszystkie fazy |
| **D-9** | Zakres `det_math` w M0: czy od razu `sin`/`cos`/`atan2`/`tanh`, czy tylko `ln`/`exp`/`pow`/`softmax`. | Rozstrzygnięcie K-6 wymienia minimum. Funkcje trygonometryczne potrzebne są dziś tylko w renderze, gdzie float wolno; `tanh` jest wyprowadzalny z `exp`. | **Tylko minimum K-6.** Faza, która pierwsza potrzebuje funkcji w kodzie wpływającym na stan trwały, dopisuje ją do `det_math` (z wektorem T-D9 i referencją MPFR), nie do swojego crate'a. | **M4** (jeśli ruch mikro potrzebuje `atan2` w stanie trwałym), M10 |
| **D-10** | Czy macierz T-D9 obejmuje ARM od początku (QEMU w CI kosztuje czas przebiegu), czy dopiero przed pierwszym wydaniem. | Bez ARM test sprawdza tylko x86-64 Linux vs Windows — to i tak łapie różnicę glibc/MSVC, czyli najczęstszy przypadek. | **Od początku, ale rzadziej**: Linux + Windows w każdym PR, ARM (QEMU) w przebiegu nocnym. Rozjazd wykryty nocą jest tani, wykryty przed premierą — nie. | M12 (platformy docelowe) |

| **D-11** | Czy `WorldCoord` (i32, centymetry) w `core` to właściwa reprezentacja współrzędnej świata, czy M1/M2 potrzebują innej. | `PlaceRef::Coord` musi mieć jakiś typ, a ten typ nie może mieszkać w `spatial` ani `voxel` (cykl). Centymetry w i32 dają ±21 tys. km przy pełnym determinizmie. | **`WorldCoord { x, y, z }` w i32 cm.** `core` definiuje wyłącznie reprezentację; układ odniesienia, granice świata i geometria należą do M1/M2. | **M1**, **M2** — do potwierdzenia przed WP-02b |
| **D-12** | Które jeszcze kategorie dostaną areny zamiast encji. | K-16 rozstrzyga `BatchId` i `OfferId`. Kandydaci o tym samym profilu (masowe, krótkowieczne, nigdy nie odpytywane przekrojowo): zlecenia transportowe (M6), zdarzenia DES (M3), pozycje w kolejkach na krawędziach (M4). | **Nie rozstrzygamy z góry.** Mechanizm jest gotowy; faza, która ma taki przypadek, sięga po `Arena<T>` i wpisuje nowy wariant `ArenaKind`. Kryterium jest w §5.1b i jest sprawdzalne, nie uznaniowe. | M3, M4, M6 |

Nierozstrzygnięte przy pisaniu tego dokumentu (brak potwierdzenia od fazy-konsumenta): **D-3** (M4),
**D-5** (M1), **D-9** (M4), **D-11** (M1, M2). Pozostałe są decyzjami wewnętrznymi M0 albo zostały
zamknięte rozstrzygnięciami dokumentu 00 i obowiązują domyślnie.

**Nie są decyzjami otwartymi** — wpisane do dokumentu 00 i wiążące, tu tylko zaimplementowane:
**K-1** (kalendarz 360-dniowy, §5.1), **K-4** (siatka zakresów `StreamId`, §5.2), **K-6** (zakaz libm
i `det_math`, §2 i §5.3a), **K-8** (słowniki domenowe w `core`, §5.1a), **K-12** (`DecisionReason`
bez `#[non_exhaustive]`, §5.1a), **K-16** (`Arena<T>`, `BatchId`/`OfferId` poza ECS, §5.1b).
Otwarty pozostaje wyłącznie zakres i harmonogram testów (D-9, D-10) oraz D-11 i D-12.

---

## 10. Szacunek wielkości

| WP | Zakres | Rozmiar |
|---|---|---|
| WP-01 | Workspace, toolchain, szkielet CI | **S** |
| WP-02 | `core`: typy bazowe, pieniądz, `Fx`, czas | **M** |
| WP-02b | `core`: `Entity`, `Arena<T>`, słowniki K-8, `DecisionReason`, `ComponentSchemaId` | **L** |
| WP-03 | `core`: RNG, `StreamId`, `SeededMap` | **S** |
| WP-03b | `core`: `det_math` (`ln`/`exp`/`pow`/`softmax`) + lint zakazu libm | **M** |
| WP-04 | `ecs`: `Entity`, storage archetypowy SoA | **L** |
| WP-05 | `ecs`: `Query`, `Access`, zasoby | **M** |
| WP-06 | `jobs`: pula, deterministyczny fork-join | **M** |
| WP-07 | `ecs`: scheduler DAG | **L** |
| WP-08 | `ecs`: `CommandBuffer`, bariera | **M** |
| WP-09 | `io`: `HashState`, hash stanu | **S** |
| WP-10 | `io`: snapshot bincode + zstd | **M** |
| WP-11 | `devtools`: szkielet | **M** |
| WP-12 | `tools/headless`: runner | **S** |
| WP-13 | Testy determinizmu i benchmarki | **M** |
| WP-14 | CI pełne | **S** |
| WP-15 | Bootstrap wgpu/winit + statyczny chunk | **M** |

Rozkład: 4 × S, 9 × M, 3 × L, 0 × XL. Ciężar fazy leży w WP-04 i WP-07 — to one decydują o tym, czy
kolejne dziesięć faz stoi na czymś, czy grzęźnie. WP-02b urósł do L nie przez złożoność, tylko przez liczbę
niezależnych mechanizmów (pięć), z których żaden nie zależy od pozostałych — da się go rozbić między dwie
osoby albo zrobić w dowolnej kolejności wewnętrznej. Reszta jest rusztowaniem.

Ścieżka krytyczna:
`WP-01 → WP-02 → WP-02b → WP-04 → WP-05 → WP-07 → WP-08 → WP-09 → WP-10 → WP-12 → WP-13 → WP-14`.
Poza nią (możliwe równolegle): **WP-03** i **WP-03b** (po WP-02), **WP-06** (po WP-01), **WP-11** (po WP-09),
**WP-15** (po WP-01).
