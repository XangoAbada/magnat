# R1 — Refaktor strukturalny po M5

Dokument wykonawczy spoza numeracji faz. Wykonuje się go **po zamknięciu M5e, przed startem M6a**.
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.

Prefiks `R` zamiast `MX`, bo to nie jest faza: nie wnosi żadnej zdolności do gry, nie ma bramek 1–7
z `00-postep.md` i nie dostarcza kontraktu żadnej fazie następnej. Ma za to własne kryterium
zamknięcia, własne pakiety robocze i własny wpis w dzienniku — dokładnie jak podfaza.
Numer `R1` zostawia miejsce na `R2` w tym samym miejscu po M8, jeśli reguła z §6 pokaże, że jest
potrzebny; jeśli nie pokaże, `R2` nie powstanie i to będzie dowód, że reguła działa.

| | |
|---|---|
| **Wejście** | M5e zamknięte (wszystkie bramki fazy M5), `master` zielony. |
| **Pakiety robocze** | R-WP1…R-WP10 |
| **Wynik do pokazania** | `master` z identycznymi hashami świata i zerem plików produkcyjnych powyżej 1200 linii kodu poza jawną listą wyjątków; skrypt `scripts/struct_guard.py` w CI i reguła w `CLAUDE.md`, która każe agentowi sprawdzić własną pracę, zanim ją zacommituje. |
| **Kryterium zamknięcia** | Kryteria R-WP1…R-WP10 (§4) plus siedem kryteriów akceptacji z §7. Twarde: **żaden hash nie zmienił się o bit**. |
| **Poprzednia / następna** | `M5e-panel-balansator-domkniecie.md` · `M6a-katalog-i-partia.md` |

---

## 1. Po co to jest i dlaczego akurat tutaj

Sześć faz dopisywało do tych samych plików i żadna nie miała powodu ich podzielić — bo kryterium
ukończenia pakietu brzmi „test przechodzi", i słusznie. Nie brzmi „a plik, do którego dopisałeś
czterysta linii, nadal da się przeczytać". Stan na dziś, mierzony na 182 plikach produkcyjnych
(bez `tests/`, bez `benches/`, bez bloków `#[cfg(test)]`):

| Metryka | p50 | p90 | p95 | p99 | maksimum |
|---|---|---|---|---|---|
| Linie kodu w pliku | 263 | 910 | 1181 | 1879 | **2693** (`engine/render/src/renderer.rs`) |
| Linie w bloku `impl` | 18 | 117 | 181 | 421 | **1917** (`impl Renderer`) |
| Linie w funkcji | 14 | 51 | 77 | 158 | **780** (`Renderer::new`) |

Rozkład jest zdrowy do dziewięćdziesiątego percentyla i rozjeżdża się w ogonie. To nie jest
przypadek rozłożony równomiernie: **26 plików przekracza 800 linii, 8 przekracza 1200, a cztery
bloki `impl` przekraczają 500**. Te osiem plików to 22 % całego kodu produkcyjnego.

Drugi wynik pomiaru jest ciekawszy od pierwszego. Cztery najdłuższe pliki produkcyjne
(`renderer.rs`, `population/mod.rs`, `city/sites.rs`, `city/mod.rs`, razem 8113 linii) mają
**zero albo prawie zero testów jednostkowych w pliku**, podczas gdy pliki o połowę krótsze mają
ich po 200–400. To nie zbieg okoliczności i nie kwestia dyscypliny: plik, z którego nie da się
wyjąć kawałka do testu, rośnie, bo nie ma w nim miejsca, w którym coś by się kończyło.
Podział nie jest tu estetyką — jest warunkiem, żeby dało się dopisać test.

**Dlaczego przed M6, a nie „kiedyś".** M6 (łańcuch dostaw) dopisuje dokładnie do tych plików:
przejmuje katalog towarów i receptur, przejmuje `supply_closure_check` z `city/sites.rs`
(M6d — koniec dostawcy zewnętrznego), rozszerza `city/build.rs` o zakłady i magazyny, rozszerza
`sim/economy`. Wejście w te pliki teraz kosztuje raz. Wejście po M6 kosztuje dwa razy, bo M6
dołoży do nich swoje kilkaset linii, a potem trzeba będzie dzielić większy plik z większą liczbą
zależności. To jest ta sama arytmetyka, co w regule „poprawki wędrują w przód" (`K-18`),
tylko zastosowana do kodu zamiast do planu.

**Czego to nie jest.** Nie jest sprzątaniem ani spłatą długu technicznego w ogólnym sensie.
Kod tych plików jest poprawny, przetestowany i mieści się w budżetach — jedyne, co mu dolega,
to że wszystko leży w jednym kawałku. R1 przesuwa granice modułów i nie dotyka niczego innego.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

- Podział dziewięciu plików produkcyjnych na moduły wzdłuż szwów wypisanych w §4.
- Przeniesienie symboli między modułami tej samej skrzyni, z zachowaniem widoczności.
- Reguła w `CLAUDE.md` i skrypt `scripts/struct_guard.py` z §6 — jedyna trwała zmiana zachowania
  narzędzi, jaką ten dokument wnosi.
- Rejestr długu strukturalnego (tabela na końcu tego dokumentu) dla tego, co świadomie zostaje.

### Nie wchodzi

- **Zmiana zachowania czegokolwiek.** Ani jednej linii logiki. Kryterium jest w §3 i jest twarde.
- **Zmiana publicznego API skrzyń.** Symbol może zmienić moduł, nie może zmienić sygnatury ani
  zniknąć. Ścieżkę importu wolno zmienić, bo kompilator sprawdza wszystkie wywołania naraz.
- Optymalizacje, nawet oczywiste. Jeśli podział odsłoni coś do poprawy — pozycja w rejestrze
  długu, nie zmiana w tym commicie.
- Przepisywanie algorytmów. `engine/nav/src/cch.rs`, `engine/core/src/det_math.rs`
  i `engine/nav/src/graph.rs` zostają w całości — powody w §5.
- Nowe testy poza tymi, które podział **umożliwia** i które są darmowe (np. suballokator areny
  renderera, geometria obrysu budynku, bilans masy w domknięciu podaży).
- Pliki testowe. `sim/world/tests/consistency.rs` ma 817 linii i to nie jest problem: długi plik
  testów czyta się liniowo i nikt nie szuka w nim abstrakcji.
- Pliki generowane: `Cargo.lock`, `engine/core/tests/reference/det_math.txt` (3715 linii),
  `run_mezo.hashes`.

---

## 3. Zasada nadrzędna: refaktor bez zmiany zachowania

Trzy reguły, w kolejności ważności.

**1. Hash świata bit w bit.** `run_mezo.hashes`, `sim/world/tests/determinism.rs`, macierz
32 seedów × 5 regionów z M1, złote odciski `det_math` — wszystko zostaje niezmienione.
Podział, który zmienia kolejność iteracji albo przydział strumieni RNG, jest **błędem
implementacji, nie decyzją projektową**, i cofa się go bez dyskusji.

**2. Przenoszenie funkcji — tak. Zmiana kolejności wywołań — nie.** To jest najostrzejsza granica
tego dokumentu i dotyczy głównie R-WP4 (`generate_population`, dziesięć kroków w ustalonej
kolejności), R-WP7 (`step_day`/`step_month`, kolejność hazardów) i R-WP9 (`generate_city`).
Jeśli okaże się, że podział wymaga zmiany kolejności — **pakiet się zatrzymuje** i sprawa idzie
do §9 jako decyzja otwarta, a nie do kodu jako „przy okazji uporządkowane".

**3. Jeden pakiet = jeden commit, `master` zielony po każdym.** Reguła „jedna gałąź" z `CLAUDE.md`
obowiązuje bez zmian: `cargo test --workspace` i `clippy -D warnings` **przed** commitem.

### Procedura dla każdego pakietu

1. Zapisz hashe przed zmianą (przebieg, który wypełnia `run_mezo.hashes`, oraz testy determinizmu).
2. Przenieś symbole. W starym module zostaw `pub use nowy_modul::*;` — wywołania w całym
   workspace nie wymagają wtedy żadnej zmiany i diff jest minimalny.
3. `cargo test --workspace`, `clippy -D warnings`, porównanie hashy, `scripts/bench_guard.py`.
4. Commit.
5. **Dopiero osobnym krokiem** (ostatni pakiet, R-WP10) usuwa się re-eksporty i poprawia ścieżki
   importu w całym workspace. To zmiana czysto mechaniczna, którą kompilator weryfikuje w całości,
   więc ma prawo być duża — ale nie ma prawa iść razem z przenoszeniem kodu, bo wtedy nie widać,
   która z dwóch rzeczy coś zepsuła.

Punkt 5 jest przedmiotem decyzji `D-R3` w §9 — istnieje wariant, w którym re-eksporty zostają
na stałe. Nie polecam go, ale jest tańszy.

---

## 4. Pakiety robocze

Kolejność ma jedną twardą zasadę, tę samą co w M5: **narzędzie kontroli przed pracą, którą ma
kontrolować**. R-WP1 idzie pierwszy, bo dopiero on odpowiada na pytanie, czy kolejny taki dokument
będzie w ogóle potrzebny — a gdyby powstał po refaktorze, powstałby na oczyszczonym repo
i nikt by nie zobaczył, czy w ogóle cokolwiek wykrywa.

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R-WP1 | Reguła i skrypt kontroli strukturalnej | — | M | `[ ]` |
| R-WP2 | `tools/magnat/src/main.rs` | R-WP1 | S | `[ ]` |
| R-WP3 | `engine/render/src/renderer.rs` | R-WP1 | L | `[ ]` |
| R-WP4 | `sim/world/src/population/mod.rs` | R-WP1 | L | `[ ]` |
| R-WP5 | `sim/world/src/city/sites.rs` | R-WP1 | M | `[ ]` |
| R-WP6 | `sim/traffic/src/oracle.rs`, `trip.rs` | R-WP1 | M | `[ ]` |
| R-WP7 | `sim/agents/src/demography.rs` | R-WP1 | M | `[ ]` |
| R-WP8 | `sim/world/src/city/build.rs` | R-WP5 | M | `[ ]` |
| R-WP9 | `sim/world/src/city/mod.rs` | R-WP5, R-WP8 | S | `[ ]` |
| R-WP10 | `transit.rs`, `micro.rs`, usunięcie rusztowań | R-WP2…R-WP9 | M | `[ ]` |

R-WP2…R-WP9 są wzajemnie niezależne poza wypisanymi zależnościami i można je przestawiać.
Kolejność w tabeli jest kolejnością rosnącego ryzyka: R-WP2 nie dotyka symulacji wcale, R-WP3 nie
dotyka determinizmu, a R-WP4, R-WP7 i R-WP9 dotykają obu.

---

### R-WP1 — Reguła i skrypt kontroli strukturalnej

Treść w §6, bo to jedyny pakiet, który zostaje w repo na stałe. Tu tylko kryterium.

**Kryterium ukończenia:** `scripts/struct_guard.py` liczy cztery metryki z §6 na plikach
produkcyjnych, ma **własny test wykrywacza** (plik-atrapa przekraczający każdy próg musi zapalić
każdą z czterech metryk — bramka, która nigdy nie świeci na czerwono, nie jest bramką, ta sama
zasada co w `sim/economy/tests/single_entry_point.rs` z M5a), wchodzi do `ci.yml` jako job
`struct-guard` w trybie ostrzegawczym, a `CLAUDE.md` ma sekcję „Reguła: przegląd strukturalny
po zamkniętym pakiecie". Progi zatwierdzone (`D-R1`).

---

### R-WP2 — `tools/magnat/src/main.rs` (1200 → ~4 pliki)

Najtańszy podział w zestawie i celowo pierwszy po narzędziu: nie dotyka symulacji, więc jest
sprawdzianem procedury z §3, a nie sprawdzianem odwagi.

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `tools/magnat/src/args.rs` | `Args`, `parse_hour`, `parse_seed` | 120 |
| `tools/magnat/src/app.rs` | `App`, `impl ApplicationHandler`, `impl App` (445 linii) | 700 |
| `tools/magnat/src/preview.rs` | `mapa_dalekiego_terenu`, `barwa_biomu`, `swiatla_testowe`, `startowa_kamera`, `czasy_passow` | 230 |
| `tools/magnat/src/main.rs` | `main`, stałe interakcji | 150 |

**Kryterium:** `magnat` uruchamia się z tym samym zestawem argumentów, `--inspect` i podgląd
działają, żaden plik powyżej 800 linii.

---

### R-WP3 — `engine/render/src/renderer.rs` (2813 → 5 plików)

Najgorszy plik w repo i zarazem najbezpieczniejszy do podziału: nie wchodzi do hasha świata.
`Renderer::new` to **780 linii** tworzenia ośmiu układów grup wiązań, siedmiu modułów shaderów
i ośmiu pipeline'ów — kod bezstanowy, biorący `&Device` i zwracający obiekt GPU.

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `renderer/pipelines.rs` | po jednej funkcji wolnej na pipeline i jego layout: `terrain`, `depth`, `shadow`, `water`, `sky`, `far`, `post`, `cluster`; plus `uniform_entry`, `storage_entry`, `rw_storage_entry`, `create_*` | 900 |
| `renderer/arena.rs` | `Block`, `Arena`, `GpuChunk`, `VERTEX_CAPACITY`, `INDEX_CAPACITY`, `pojemnosc`, metody `upload_chunk` / `remove_chunk` / `has_chunk` | 300 |
| `renderer/frame.rs` | `FrameLists`, `VisibleChunk`, `DrawArgs`, `FrameUniform`, `ChunkUniform`, `prepare_frame`, `write_indirect`, `draw_list`, `write_frame_uniform`, `frustum_planes`, `sphere_in_frustum` | 600 |
| `renderer/passes.rs` | `record_passes`, `record_post`, `resolve_timer`, `PassTimer`, `PASS_NAMES`, `FrameStats`, `finish_stats`, `zbierz_occupancy` | 500 |
| `renderer.rs` | `struct Renderer`, `new` jako składanie, `render*`, `resize`, settery (`set_overlay`, `set_lights`, `set_pedestrians`, `set_cursor`, `upload_far_terrain`, `pick`) | 500 |

Rust pozwala rozłożyć `impl Renderer` na pliki w obrębie jednej skrzyni — pola trzeba podnieść
do `pub(crate)`. `renderer/pipelines.rs` nie dotyka `self` wcale i jest czystą kompozycją.

**Kryterium:** `Renderer::new` poniżej 100 linii; zrzut z `--screenshot` identyczny bajt w bajt
z zapisanym przed podziałem (ten sam test, którym M1 porównywał obie ścieżki rysowania — SHA-256);
`renderer/arena.rs` ma test jednostkowy suballokatora, który dziś nie istnieje, bo nie ma jak.

---

### R-WP4 — `sim/world/src/population/mod.rs` (2221 → 8 plików)

Katalog `population/` istnieje od M3d i trzyma obok siebie `table.rs` — i 2150 linii w `mod.rs`.
Podział jest wypisany w nagłówku samego pliku: dziesięć kroków z M3d §5.9.

| Nowy plik | Kroki §5.9 | Zawartość | ~linii |
|---|---|---|---|
| `population/catalog.rs` | most miasto→agenci | `katalog_miejsc`, `pustostany`, `wakaty`, `grafik`, `fakty_miasta`, `srodek`, `dzielnica_budynku` | 200 |
| `population/pyramid.rs` | 1–2 | `PulaWieku`, `piramida`, `Sklad`, `sklady` | 250 |
| `population/traits.rs` | 3–4 | `wyksztalcenie`, `osobowosc`, `umiejetnosci`, `skill_match` | 130 |
| `population/jobs.rs` | 5 | `Zatrudnienie`, `dopasuj_prace`, `wybierz_etat`, `wybierz_gdziekolwiek`, `przypisz_szkoly`, `PRZEGLAD_ETATOW` | 220 |
| `population/homes.rs` | 6–7 | `Pracownik`, `Mieszkania`, `dopasuj_mieszkania`, `przemieszaj_w_decylach`, `mediana_minutowa`, `histogram_docelowy`, `spearman`, `dojazdy_gospodarstwa`, `przeprowadz`, `Puste`, `widok` | 450 |
| `population/seeding.rs` | 8–9 | `zasiej_wiedze`, `korytarz`, `relacje_startowe` | 200 |
| `population/report.rs` | 10 | `PopulationReport`, `chi2`, `Liczby`, `raport` | 300 |
| `population/mod.rs` | — | `generate_population` jako lista dziesięciu kroków, `PopulationParams`, `Populated`, `PopulationError`, stałe | 350 |

To jest plik, który zyskuje najwięcej: `generate_population` ma dziś 290 linii i jest jedynym
miejscem, z którego widać całą sekwencję. Po podziale ma być listą wywołań.

**Ostrzeżenie o determinizmie.** Kroki 6–7 pracują na lustrze stanu i wykonują 200 tys. prób
zamiany w pętli poprawkowej. Kolejność tych prób jest częścią hasha. `population/homes.rs`
przenosi się w całości i bez zmiany ani jednej pętli.

**Kryterium:** hash populacji identyczny dla macierzy seedów z M3d; `mod.rs` poniżej 400 linii;
żaden z nowych plików powyżej 500.

---

### R-WP5 — `sim/world/src/city/sites.rs` (1859 → 4 pliki)

Trzy niezależne tematy w jednym pliku i zero testów jednostkowych. Pakiet krytyczny dla M6,
bo M6d przejmuje trzeci z nich w całości.

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `city/sites/data.rs` | `ArchetypeSpec`, `Archetype`, `ChainTemplate`, `FirmNames`, `SiteDataError`, `SiteCatalog`, stałe schematów i skali | 410 |
| `city/sites/place.rs` | `populate`, `zbierz_kandydatow`, `PoleLudnosci`, `rozmiesc_normatyw`, `wypelnij`, `pasuje_dzialka`, `popyt_bazowy`, `zapotrzebowanie_zakladow`, `klastry`, `posadz_produkcje`, `utworz_zaklady`, `rebind_workplaces`, `licz_stanowiska`, `rzymska` | 750 |
| `city/sites/closure.rs` | `supply_closure_check`, `zgas_nadmiarowe`, `wiodace_dla`, `wiodace_wyjscia`, `bilans`, `ClosureReport`, `brama_t_na_dobe`, `pusty` | 300 |
| `city/sites/mod.rs` | `SectorId`, `SiteSet`, `SiteSeed`, `FirmSeed`, `SiteReport`, `site_id`, `firm_id`, `niemieszkalna` | 250 |

**Nie do `city/catalog.rs`.** Tamten plik to katalog towarów i receptur, którego właścicielem
docelowym jest M6 — inny temat mimo mylącej zbieżności nazw. `sites/data.rs` to katalog
archetypów zakładów.

**Kryterium:** `city_hash` i `world_hash_m2` niezmienione dla macierzy z M2; `closure.rs` dostaje
test bilansu masy, który dziś nie istnieje; M6d ma jeden plik do przejęcia zamiast fragmentu.

---

### R-WP6 — `sim/traffic/src/oracle.rs` + `trip.rs` (2319 → ~6 plików)

Dwa sąsiadujące pliki z blokami `impl` na 761 i 590 linii. Szew w `oracle.rs` biegnie między
„wybierz środek transportu" a „prowadź podróż".

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `oracle/offers.rs` | `plan`, `offer_walk`, `offer_bike`, `offer_transit`, `offer_car`, `offer_taxi`, `policz_niewykonalne` | 400 |
| `oracle/trip.rs` | `start_trip`, `estimate_inner`, `places_on_route_inner`, `enter_micro_inner`, `zapamietaj_decyzje`, `last_decision`, `is_watched` | 350 |
| `oracle/mod.rs` | `TrafficOracle`, `DriverEntry`, `Station`, ~25 getterów i setterów, indeks przestrzenny, `OracleHandle`, `impl TravelOracle`, `walk_minutes_for`, `speed_pct` | 500 |
| `trip.rs` → podział analogiczny | ustalić przy pakiecie; blok `impl` na 590 linii jest tam jedyną strukturą (`D-R6`) | — |

Pięć funkcji `offer_*` to pięć wariantów jednego kontraktu. Dziś nie widać, że mają wspólny
kształt, bo leżą między setterami — po podziale różnica między nimi staje się treścią pliku.

**Kryterium:** `run_mezo.hashes` niezmienione; `mode_counts` i `infeasible_counts` identyczne
na przebiegu M4c; żaden blok `impl` powyżej 500 linii.

---

### R-WP7 — `sim/agents/src/demography.rs` (1998 → 4 pliki)

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `demography/table.rs` | `AgeRate`, `AgeChance`, `Ages`, `Range8`, `StatusWeights`, `MigrationParams`, `SocialParams`, `DemographyFile`, `DemographyTable`, `DemographyError`, `pokrycie`, `stopa` | 370 |
| `demography/day.rs` | `step_day`, `terminarz`, `hazardy`, `zachoruj`, `poczecie`, `uroda`, `liczba_dzieci`, `smierc`, `spadkobiercy`, `usun_relacje`, `zwolnij_slaby`, `opusc_gospodarstwo`, `rozwiaz_gospodarstwo` | 600 |
| `demography/month.rs` | `step_month`, `dobierz_partnerow`, `sluby`, `polacz_gospodarstwa`, `rozstania`, `compatibility`, `rodzinna`, `wyprowadz_mieszkanca`, `przeklasyfikuj` | 400 |
| `demography/mod.rs` | `Population`, `LifeQueue`, `LifeTask`, `InheritanceHook`, `DayReport`, `MonthReport`, klucze strumieni (`K_*`, `stream_key`), `citizen_by_index`, `household_by_index`, `encja` | 350 |

`powiaz`, `wpisz_relacje`, `odwrotna`, `relations_ref`, `knowledge_ref` (~110 linii) to kandydaci
do przeniesienia do istniejącego `social.rs` — **do sprawdzenia kierunku zależności przed
ruszeniem**, bo `social.rs` może już zależeć od `demography`.

**Ostrzeżenie o determinizmie.** Kolejność hazardów w `hazardy` i kolejność zdarzeń w `terminarz`
są częścią hasha przez przydział strumieni `K_*`. Przenosi się je bez zmiany kolejności `match`.

**Kryterium:** hash świata po 360 dobach identyczny; `DayReport` i `MonthReport` identyczne
co do pola na przebiegu M3c.

---

### R-WP8 — `sim/world/src/city/build.rs` (1877 → 5 plików)

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `build/model.rs` | `Building`, `Unit`, `UnitKind`, `UnitOccupant`, `Entrance`, `Workplace`, `WageBand`, `ShiftId`, `JobRole`, `JobTable`, `ShiftKey`, `wage_band`, `BuildingSet`, `BuildingSignature`, `building_id` | 350 |
| `build/footprint.rs` | `footprint_for`, `zapas`, `Axis2`, `rozpietosc`, `miesci_sie`, `rogi`, `bryla`, `wyjscie_z_bryly`, `wejscia`, stałe geometryczne | 350 |
| `build/interiors.rs` | `interiors`, `rodzaj`, `czynsz`, `stan_techniczny` | 180 |
| `build/capacity.rs` | `recompute_pop_capacity`, `rescale_dwellings` i stałe kalibracyjne (`OSOB_NA_MIESZKANIE`, `ETATOW_NA_MIESZKANCA`, `GESTOSC_*`, `ETATY_SCALE_*`) | 300 |
| `build/mod.rs` | `build_all`, `build_for_sites`, `emit`, `plan_building`, `plan_building_inner`, `queue_building`, `pick_grammar`, `pasuje`, `PickCtx`, `Relax`, `Planned`, `Skip`, `BuildInput`, `roznorodnosc`, `entropia_mbits`, `BuildReport` | 700 |

`build/footprint.rs` to czysta geometria 2D bez świata — pierwszy kandydat na test jednostkowy,
którego dziś nie ma. `build/capacity.rs` skupia stałe kalibracyjne w jednym miejscu, co jest
warunkiem, żeby balansator z M5e mógł je kiedykolwiek ruszyć.

**Kryterium:** `city_hash` niezmienione; liczba i rozmieszczenie budynków identyczne dla macierzy
z M2d; `mod.rs` poniżej 800 linii.

---

### R-WP9 — `sim/world/src/city/mod.rs` (1410 → 4 pliki)

`mod.rs` z dziewiętnastoma deklaracjami `pub mod` i 1380 liniami własnego kodu. Zasada:
**`mod.rs` deklaruje i orkiestruje, nie implementuje.**

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `city/report.rs` | `GenerationReport` i jego `impl` (290 linii) | 300 |
| `city/hash.rs` | `city_hash`, `world_hash_m2`, `policz`, `mediana_wartosci` | 180 |
| `city/checks.rs` | `dangling_high_class`, `skladowe_jezdne`, `is_connected` | 120 |
| `city/mod.rs` | deklaracje modułów, `CityPlan`, `CityData`, `CityGenError`, `PROFILE_SCHEMA_VERSION`, `load_profile`, `generate_city`, `finalize`, `place_furniture`, `target_pop` | 700 |

700 linii w `mod.rs` to wciąż dużo, ale to jest orkiestracja generacji miasta i ma prawo być
długa — `generate_city` czyta się wtedy jak spis etapów, czym jest.

**Ostrzeżenie o determinizmie.** `city_hash` przenosi się razem z całą listą komponentów
wchodzących do hasha. Pominięcie jednego pola przy przenoszeniu jest wykrywalne tylko przez
porównanie hashy — dlatego procedura z §3 wymaga zapisania ich **przed** zmianą.

**Kryterium:** macierz 32 seedów × 5 regionów identyczna; `mod.rs` poniżej 800 linii.

---

### R-WP10 — `transit.rs`, `micro.rs` i usunięcie rusztowań

Dwa ostatnie podziały o mniejszym zysku, plus sprzątanie po całym R1.

**`sim/traffic/src/micro.rs` (1437):** w pliku o pojazdach siedzą piesi. `Pedestrian`,
`PedestrianBuffer` i `PathArena` (~190 linii) nie mają z `VehicleBuffer` wspólnego nic poza tym,
że jedno i drugie się porusza → `micro/pedestrians.rs`. Osobno `micro/idm.rs`: `IdmParams`,
`przyspieszenie`, `pozadana`, `equilibrium_speed_cms`, `idm_speed_dkmh` — fizyka, która już dziś
ma funkcje wolne, więc wychodzi bez zmiany API. Zmiana pasów (`zmien_pasy`, `sasiedzi`, `zakres`,
`przed_w`, `przyspieszenie_w`, ~130 linii) → `micro/lanes.rs`.

**`sim/traffic/src/transit.rs` (1460):** szew między planowaniem (`plan_journey`,
`plan_with_transfer`, `ride_min`, `hop_cost`, `stops_near`) a symulacją minuty (`step_minute`,
`dispatch`, `advance`, `przesiadka`, `give_up`, `enqueue`) → `transit/plan.rs`, `transit/sim.rs`,
`transit/mod.rs`. `advance` ma 176 linii i jest kandydatem sam w sobie, ale dzieli się go tylko
wtedy, gdy wychodzi to bez zmiany kolejności zdarzeń.

**Sprzątanie:** usunięcie wszystkich `pub use` rusztowań dodanych w R-WP2…R-WP9 i poprawienie
ścieżek importu w całym workspace (`D-R3`). Zmiana mechaniczna, weryfikowana przez kompilator
w całości, ma prawo dotknąć stu plików.

**Kryterium:** żadnego `pub use` dodanego przez R1 nie zostało; `scripts/struct_guard.py` na
całym repo nie zgłasza błędu (ostrzeżenia dopuszczalne, jeśli mają wpis w rejestrze długu).

---

## 5. Czego nie ruszamy i dlaczego

Lista jest częścią kryterium akceptacji nr 4 — to są jawne wyjątki, nie przeoczenia.
Wchodzi do `scripts/struct_guard.py` jako lista wykluczeń z komentarzem, nie jako cichy wyjątek.

| Plik | Linie kodu | Dlaczego zostaje |
|---|---|---|
| `engine/nav/src/cch.rs` | 942 (+394 testów) | Jeden algorytm. `contract`, `customize` i `query` dzielą niezmienniki struktury łuków skrótowych; rozbicie po plikach rozerwie je bez żadnego zysku. Jedyny sensowny kawałek to `build_order` + `Dissector` (nested dissection to osobny algorytm od kontrakcji) — do rozważenia osobno, poza R1 |
| `engine/core/src/det_math.rs` | 876 (+237 testów) | Słownik funkcji deterministycznych. Podzielony słownik to dwa słowniki. Dodatkowo `K-6` wiąże go ze złotym odciskiem, więc każde dotknięcie jest kosztowne |
| `engine/nav/src/graph.rs` | 1064 (+166 testów) | Spójny: typy grafu, builder, walidacja. Jedyne, co odstaje, to `synthetic_grid` i `synthetic_road_network` (~100 linii fikstur testowych w kodzie produkcyjnym) — `D-R5` |
| `sim/world/tests/consistency.rs` i pozostałe `tests/` | — | Długi plik testów nie jest długiem. Czyta się go liniowo i nikt nie szuka w nim struktury |
| `engine/core/tests/reference/det_math.txt` | 3715 | Generowany |
| `Cargo.lock`, `run_mezo.hashes` | — | Generowane |

Reguła ogólna, której ta lista jest zastosowaniem: **dzieli się pliki, w których są dwa tematy,
a nie pliki, które są długie.** Plik długi, bo jeden algorytm jest długi, zostaje.

---

## 6. Reguła kontroli po zakończonej pracy — czego nam brakuje

To jest część, przez którą ten dokument w ogóle jest potrzebny, i jedyna, która zostaje w repo
po jego zamknięciu.

### Problem

Przez sześć faz nikt nie patrzył na rozmiar tego, co zostawia — nie z niedbalstwa, tylko dlatego,
że **nie ma momentu, w którym się to robi**. `CLAUDE.md` ma regułę odhaczania postępu, regułę
jednej gałęzi i regułę „poprawki wędrują w przód". Wszystkie trzy pytają o plan, dokumenty
i testy. Żadna nie pyta o kształt kodu, który właśnie powstał. Kryterium ukończenia pakietu brzmi
„test przechodzi" i to jest właściwe kryterium — ale jest niezupełne, bo przechodzący test nie
odróżnia czterystu linii dopisanych do modułu od czterystu linii dopisanych do worka.

Efekt jest mierzalny i jest w §1: rozkład zdrowy do p90 i rozjazd w ogonie. Ogon rośnie zawsze
w tych samych plikach, bo każda faza dopisuje tam, gdzie poprzednia zostawiła punkt zaczepienia.

### Czego potrzebujemy

Mechanizmu, który po zakończonej pracy — **przed commitem, nie po** — zadaje agentowi trzy pytania:

1. Czy plik, do którego dopisałem, przekroczył próg?
2. Jeśli tak: czy da się to podzielić **bez zmiany zachowania**, w tym samym commicie?
3. Jeśli nie da się: jaka pozycja idzie do rejestru długu strukturalnego i z jakim powodem?

Trzecie pytanie jest równie ważne jak drugie. Odpowiedź „nie dzielę, bo to jeden algorytm" jest
dobra — pod warunkiem, że jest zapisana. Nigdy jako `TODO` w kodzie (`K-18` pkt 4), tylko jako
wiersz w tabeli na końcu tego dokumentu albo komentarz `ponytail:` nazywający sufit i ścieżkę
wyjścia, zgodnie z konwencją z `CLAUDE.md`.

### Dwa warianty, nie wykluczają się

**Wariant A — reguła w `CLAUDE.md`.** Nowa sekcja „Reguła: przegląd strukturalny po zamkniętym
pakiecie", obok istniejących. Treść: przed commitem zamykającym pakiet agent liczy metryki na
plikach dotkniętych zmianą i odpowiada na trzy pytania wyżej.

- *Zaleta:* zero infrastruktury, działa od momentu dopisania.
- *Wada:* reguła w prompcie jest prośbą, nie bramką. Dokładnie ten sam problem, przez który M2a
  i M2b przeleżały niescalone przez trzy podfazy — a reguła jednej gałęzi też była zapisana.

**Wariant B — hook w `.claude/settings.json`.** Repo nie ma dziś katalogu `.claude/`, więc trzeba
go założyć. Hook typu `Stop` uruchamia `scripts/struct_guard.py --changed`, skrypt liczy metryki
na plikach zmienionych względem `HEAD` i **wypisuje** wynik do kontekstu agenta, gdy któryś próg
jest przekroczony.

- *Zaleta:* mechanizm, nie prośba. Nie da się zapomnieć, bo nie trzeba pamiętać.
- *Wada:* trzeba go napisać i utrzymać; hook, który krzyczy za często, zostanie wyłączony
  i wtedy jest gorszy od braku hooka, bo daje fałszywe poczucie kontroli.

**Propozycja domyślna (`D-R2`): oba, w tej kolejności.** Najpierw reguła, bo definiuje progi
i pytania. Potem hook, który tę regułę egzekwuje. Hook bez uzgodnionych progów byłby generatorem
szumu, a reguła bez hooka — pobożnym życzeniem. Do tego trzeci poziom: job `struct-guard`
w `ci.yml` obok istniejącego `bench-guard`, w trybie ostrzegawczym, jako siatka bezpieczeństwa
na wypadek pracy bez agenta.

### Progi

Nie są wzięte z sufitu — to kwartyle tego repo, zaokrąglone w górę do liczb, które da się
zapamiętać. Kolumna „dziś" mówi, ile pozycji zapala się na obecnym `master`, żeby było widać,
czy próg mierzy ogon, czy całe repo.

| Metryka | Ostrzeżenie | Błąd | Dziś (ostrz. / błąd) | Skąd próg |
|---|---|---|---|---|
| Linie kodu w pliku produkcyjnym (bez `#[cfg(test)]`) | 800 | 1200 | 26 / 8 | p90 = 910, p95 = 1181 |
| Linie w bloku `impl` | 300 | 500 | 11 / 4 | p95 = 181, p99 = 421 |
| Linie w funkcji | 150 | 250 | 33 / 9 | p95 = 77, p99 = 158 |
| `mod.rs` z własnym kodem poza deklaracjami | 300 | 600 | 2 / 2 | oba przypadki to R-WP4 i R-WP9 |

Po zamknięciu R1 kolumna „błąd" ma pokazywać zera poza jawnymi wyjątkami z §5. Jeśli po M6
pokaże cokolwiek innego, reguła zadziałała — bo wtedy wiadomo o tym w M6, a nie po M8.

### Czego kontrola **nie** robi

- Nie mierzy plików w `tests/`, `benches/` ani bloków `#[cfg(test)]`. Długi plik testów jest
  w porządku i ściganie go nauczyłoby pisania krótszych testów, nie krótszego kodu.
- Nie mierzy plików generowanych ani `data/*.ron`.
- Nie proponuje podziału pliku z listy wyjątków z §5.
- **Nie blokuje commita.** Wariant blokujący wraca do rozważenia dopiero, gdy będzie wiadomo,
  ile fałszywych alarmów daje wariant ostrzegawczy przez jedną fazę. Bramka, która blokuje
  refaktor nieistotny dla zadania, wymusza obejście, a obejście staje się nawykiem.

### Kształt skryptu

Wzorem jest istniejący `scripts/bench_guard.py`: Python, bez zależności, dwa progi
(ostrzeżenie / błąd), wynik czytelny w logu CI i w kontekście agenta.

```
scripts/struct_guard.py [--changed | --all] [--json]
  --changed   tylko pliki zmienione względem HEAD (tryb hooka)
  --all       całe repo (tryb CI)
  --json      wynik maszynowy
```

Wyjście: kod 0 przy czystym przebiegu i przy samych ostrzeżeniach, kod 1 przy przekroczeniu progu
błędu w trybie `--all` (CI). W trybie `--changed` zawsze 0 — hook informuje, nie blokuje.

Skrypt ma mieć **własny test wykrywacza**: plik-atrapa przekraczający każdy z czterech progów musi
zapalić każdą z czterech metryk. Bez tego testu skrypt, który przestanie cokolwiek wykrywać po
zmianie parsowania, będzie świecił na zielono i nikt się nie dowie — ta sama zasada, którą M5a
zastosował do `tests/single_entry_point.rs`.

### Proponowana treść reguły do `CLAUDE.md`

> ## Reguła: przegląd strukturalny po zamkniętym pakiecie
>
> Przed commitem zamykającym pakiet roboczy sprawdź pliki, które ta praca zmieniła, pod cztery
> progi (`scripts/struct_guard.py --changed`): plik 800/1200 linii kodu, blok `impl` 300/500,
> funkcja 150/250, `mod.rs` z własnym kodem 300/600.
>
> Przekroczony próg nie jest błędem i nie blokuje commita. Jest pytaniem, na które trzeba
> odpowiedzieć w jeden z trzech sposobów:
>
> 1. **Podziel teraz**, jeśli podział jest mechaniczny (przeniesienie symboli bez zmiany
>    zachowania) i mieści się w tym samym commicie.
> 2. **Zaplanuj podział**, jeśli wymaga decyzji albo dotyka determinizmu — wiersz w rejestrze
>    długu strukturalnego w `R1-refaktor-po-M5.md`, z powodem.
> 3. **Zostaw świadomie**, jeśli plik jest długi, bo jeden algorytm jest długi — komentarz
>    `ponytail:` nazywający sufit, plus wpis na liście wyjątków.
>
> Czego nie wolno: zostawić bez odpowiedzi i zostawić `TODO` w kodzie (`K-18` pkt 4).
> Dzieli się pliki, w których są dwa tematy, a nie pliki, które są długie.

---

## 7. Kryteria akceptacji

R1 nie ma bramek 1–7 z `00-postep.md`, bo nie jest fazą. Ma siedem własnych.

1. **Determinizm.** Żaden hash w `run_mezo.hashes`, `sim/world/tests/determinism.rs`, macierzy
   32 seedów × 5 regionów ani w złotych odciskach `det_math` nie zmienił się o bit.
2. **Zielony `master` po każdym pakiecie.** `cargo test --workspace` i `clippy -D warnings`
   przed każdym commitem, nie po ostatnim.
3. **Bez regresji wydajności.** `scripts/bench_guard.py` w progach `D-8` (10 % ostrzeżenie,
   25 % błąd) po każdym pakiecie. Podział pliku może zmienić decyzje inline'owania kompilatora —
   dlatego mierzymy, zamiast zakładać, że refaktor jest darmowy.
4. **Żaden plik produkcyjny powyżej 1200 linii kodu** poza listą wyjątków z §5.
5. **Żaden blok `impl` powyżej 500 linii** i żadna funkcja powyżej 250.
6. **Reguła i skrypt z §6 działają i mają test własny.** Job `struct-guard` w `ci.yml`,
   sekcja w `CLAUDE.md`, hook w `.claude/settings.json` (jeśli `D-R2` przyjęte domyślnie).
7. **Publiczne API skrzyń niezmienione**, albo każda zmiana wypisana w tabeli „Zmiany wpisane
   po R1" na końcu tego dokumentu.

---

## 8. Ryzyka i mitygacje

| # | Ryzyko | Mitygacja |
|---|---|---|
| R-1 | Podział zmienia kolejność iteracji albo przydział strumieni RNG → złamany hash, wykryty późno | Procedura z §3: hashe zapisane **przed** pakietem i porównane po nim, per pakiet, nie na końcu. Pakiety R-WP4, R-WP7 i R-WP9 mają osobne ostrzeżenie w treści |
| R-2 | Podział zmienia inline'owanie → regresja w gorących pętlach (`VehicleBuffer::substep`, `ChGraph::search`, `prepare_frame`) | `bench_guard` po każdym pakiecie. `#[inline]` dopisuje się tam, gdzie **pomiar** to pokaże, nie zapobiegawczo |
| R-3 | Refaktor rozlewa się w przeprojektowanie — „skoro już tu jestem" | §2 „nie wchodzi" plus twarda reguła: podział wymagający zmiany sygnatury publicznej zatrzymuje pakiet i idzie do §9 |
| R-4 | Konflikt z pracą M6 w tych samych plikach | Cały R1 przed M6a, nie równolegle (`D-R4`). R-WP5 i R-WP8 dotykają plików, które M6 rozszerza |
| R-5 | Hook z §6 generuje szum i zostaje wyłączony | Progi z pomiaru repo, nie z intuicji; tryb ostrzegawczy zanim ktokolwiek pomyśli o blokującym; kolumna „dziś" w tabeli progów pokazuje z góry, ile alarmów będzie |
| R-6 | Duży diff bez zmiany zachowania jest nieczytelny w recenzji, a commit jest jednostką recenzji | Re-eksporty `pub use` w kroku 2 procedury: diff pakietu to przeniesienie bloków, a nie przeniesienie plus setki poprawionych importów. Importy prostuje osobny, mechaniczny R-WP10 |
| R-7 | R1 przesuwa M6 o kilka dni i zostaje odłożony „na po M6" | Realne ryzyko bez mitygacji technicznej. Jedyna, jaka jest: §1 mówi, ile kosztuje odłożenie, a `D-R4` nie dopuszcza wykonania połowy |

---

## 9. Decyzje otwarte

Przy każdej jest propozycja, którą przyjmuję jako domyślną, jeśli nikt nie zgłosi sprzeciwu.
**Blokujące dla pierwszego pakietu: `D-R1` i `D-R2`.**

**`D-R1` — progi metryk.** Propozycja: tabela z §6 (800/1200, 300/500, 150/250, 300/600).
Zapalają dziś 26, 11, 33 i 2 pozycje — czyli mierzą ogon, nie repo. *Blokująca dla R-WP1.*

**`D-R2` — reguła, hook, czy oba.** Propozycja: oba plus job w CI, w kolejności reguła → hook → CI.
Reguła sama jest prośbą (dowód: M2a/M2b), hook sam byłby bez uzgodnionych progów szumem.
*Blokująca dla R-WP1.*

**`D-R3` — re-eksporty `pub use`: rusztowanie czy stan docelowy.** Propozycja: rusztowanie,
usuwane w R-WP10. Ścieżka importu ma mówić prawdę o tym, gdzie kod leży; `pub use` zostawione na
stałe daje dwie prawdziwe ścieżki do tego samego symbolu i po dwóch fazach nikt nie wie, która
jest ta właściwa. Wariant tańszy (zostawić) jest dopuszczalny, jeśli R-WP10 okaże się zbyt duży.

**`D-R4` — całość przed M6a, czy tylko pakiety dotykające plików M6.** Propozycja: całość.
Rozbicie na dwa terminy oznacza, że druga połowa nie nastąpi — to jest ta sama obserwacja,
z której wzięła się reguła „poprawki wędrują w przód" i reguła jednej gałęzi. Wariant minimalny
(R-WP5, R-WP8, R-WP9 przed M6, reszta po) jest dopuszczalny tylko wraz z terminem reszty.

**`D-R5` — `graph.rs::synthetic_grid` i `synthetic_road_network` za `#[cfg]`.** Propozycja: tak,
`#[cfg(any(test, feature = "synthetic"))]` — ale dopiero po sprawdzeniu, czy `tools/headless`
i `tools/magnat` nie używają ich w ścieżce produkcyjnej. Jeśli używają, zostają bez `#[cfg]`
i trafiają do rejestru długu.

**`D-R6` — czy `sim/traffic/src/trip.rs` wchodzi do R-WP6.** Plik ma 964 linie i blok `impl`
na 590. Propozycja: tak, w tym samym pakiecie co `oracle.rs`, bo te dwa pliki dzielą typy
(`TripHandle`, `PendingTrip`) i dzielenie ich osobno oznacza dwa razy tę samą analizę zależności.

---

## 10. Szacunek wielkości

| Pakiet | Plik(i) źródłowe | Linii dotkniętych | Plików po podziale | Rozmiar |
|---|---|---|---|---|
| R-WP1 | `scripts/`, `CLAUDE.md`, `ci.yml`, `.claude/` | ~250 nowych | +2 | M |
| R-WP2 | `tools/magnat/src/main.rs` | 1 200 | 4 | S |
| R-WP3 | `engine/render/src/renderer.rs` | 2 813 | 5 | L |
| R-WP4 | `sim/world/src/population/mod.rs` | 2 221 | 8 | L |
| R-WP5 | `sim/world/src/city/sites.rs` | 1 859 | 4 | M |
| R-WP6 | `sim/traffic/src/oracle.rs`, `trip.rs` | 2 319 | 6 | M |
| R-WP7 | `sim/agents/src/demography.rs` | 1 998 | 4 | M |
| R-WP8 | `sim/world/src/city/build.rs` | 1 877 | 5 | M |
| R-WP9 | `sim/world/src/city/mod.rs` | 1 410 | 4 | S |
| R-WP10 | `transit.rs`, `micro.rs` + prostowanie importów | 2 897 + rozsiane | 6 | M |
| | **Razem** | **~18 600 z 74 600 linii kodu produkcyjnego (25 %)** | **46 z 9** | |

Żaden wiersz nie wnosi ani nie usuwa funkcjonalności. Liczba linii po podziale ma być większa
o narzut nagłówków modułów i deklaracji `use` — szacunkowo 3–5 %, i to jest cena, nie oszczędność.

---

## Rejestr długu strukturalnego

Tabela zasilana przez regułę z §6, kiedy odpowiedź na pytanie 3 brzmi „nie dzielę teraz".
Pusta na start. Wiersz wpisuje się w tym samym commicie, w którym próg został przekroczony.

| # | Plik | Metryka i wartość | Świadomie zostawione, bo | Ścieżka wyjścia |
|---|---|---|---|---|
| | | | | |

---

## Zmiany wpisane po R1

Zgodnie z `K-18`. Tabela wypełnia się w trakcie wykonywania tego dokumentu — poprawki do planów
faz M6–M12, które wyjdą przy podziale plików (np. moduł, który M6 miał przejąć, okaże się leżeć
gdzie indziej, niż zakłada `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md`).

| # | Zmiana | Dlaczego |
|---|---|---|
| | | |

---

## Zmiany wpisane po M5c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5c.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Dochodzi jedenasty plik: `sim/economy/src/market.rs`** — 1 253 linie przy pisaniu tego dokumentu, **1 975** po M5c. Szwy są widoczne z nazw metod: (a) cykl życia sklepu (`open_shop`, `record_capital`, `stock_initial`, `deliver_now`), (b) zaopatrzenie i półka (`restock_shelves`, `reorder_and_receive`, `expire_goods`), (c) doba cenowa (`observe_competitors`, `reprice_all`, `set_policy`, `preview_policy`), (d) raporty księgowe (cztery cienkie opakowania na `ledger::*`), (e) `PlaceProvider` (`candidates`, `fulfil`) — i to ostatnie jest największym pojedynczym kawałkiem | Plik przekroczył próg 1 200 linii w tej samej podfazie, w której R1 powstał, i z tego samego powodu co reszta listy: dokłada się do niego każda faza, a nikt nie pyta o rozmiar. Ograniczenie jest twardsze niż przy innych plikach: `MarketInner` jest prywatny, a wszystkie te metody sięgają do jego pól, więc podział musi iść przez `pub(crate)` na polach albo przez `impl Market` w modułach potomnych — **nie** przez wyniesienie funkcji wolnych. M6 dopisuje do tego pliku rynek B2B, więc termin „przed M6a" obowiązuje tak samo |
| | **`sim/economy` ma po M5c 7 555 linii w 13 plikach** (było 4 tys. w 8). Rozkład jest zdrowy poza `market.rs`: `ledger.rs` 782, `pricing.rs` 866, `books.rs` 917, reszta poniżej 600 | Nowe moduły M5c (`kernel`, `ledger`, `pricing`, `tax`) powstały od razu podzielone, więc R1 nie ma tam nic do roboty. To jest przy okazji argument za regułą z R-WP1: plik, który rodzi się z granicą, nie rozlewa się później |

