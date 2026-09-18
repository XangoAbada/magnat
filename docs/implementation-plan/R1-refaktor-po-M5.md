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
| **Pakiety robocze** | R-WP1…R-WP12 |
| **Wynik do pokazania** | `master` z identycznymi hashami świata i zerem plików produkcyjnych powyżej 1200 linii kodu poza jawną listą wyjątków; skrypt `scripts/struct_guard.py` w CI i reguła w `CLAUDE.md`, która każe agentowi sprawdzić własną pracę, zanim ją zacommituje. |
| **Kryterium zamknięcia** | Kryteria R-WP1…R-WP12 (§4) plus siedem kryteriów akceptacji z §7. Twarde: **żaden hash nie zmienił się o bit**. |
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

- Podział jedenastu plików produkcyjnych na moduły wzdłuż szwów wypisanych w §4.
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
| R-WP1 | Reguła i skrypt kontroli strukturalnej | — | M | `[x]` |
| R-WP2 | `tools/magnat/src/main.rs` | R-WP1 | S | `[x]` |
| R-WP3 | `engine/render/src/renderer.rs` | R-WP1 | L | `[x]` |
| R-WP4 | `sim/world/src/population/mod.rs` | R-WP1 | L | `[x]` |
| R-WP5 | `sim/world/src/city/sites.rs` | R-WP1 | M | `[x]` |
| R-WP6 | `sim/traffic/src/oracle.rs`, `trip.rs` | R-WP1 | M | `[x]` |
| R-WP7 | `sim/agents/src/demography.rs` | R-WP1 | M | `[x]` |
| R-WP8 | `sim/world/src/city/build.rs` | R-WP5 | M | `[x]` |
| R-WP9 | `sim/world/src/city/mod.rs` | R-WP5, R-WP8 | S | `[x]` |
| R-WP11 | `sim/agents/src/planner.rs` | R-WP1 | M | `[x]` |
| R-WP12 | `sim/economy/src/market.rs` | R-WP1 | L | `[x]` |
| R-WP10 | `transit.rs`, `micro.rs`, usunięcie rusztowań | R-WP2…R-WP9, R-WP11, R-WP12 | M | `[ ]` |

R-WP2…R-WP9, R-WP11 i R-WP12 są wzajemnie niezależne poza wypisanymi zależnościami i można je
przestawiać. Kolejność w tabeli jest kolejnością rosnącego ryzyka: R-WP2 nie dotyka symulacji wcale,
R-WP3 nie dotyka determinizmu, a R-WP4, R-WP7, R-WP9 i R-WP12 dotykają obu. R-WP10 zostaje ostatni,
bo prostuje importy po **wszystkich** podziałach — stąd numer niezgodny z pozycją w tabeli.

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

### R-WP11 — `sim/agents/src/planner.rs` (1818 → 5 plików)

Pakiet dopisany po R-WP1 (`D-1` w tabeli zmian na końcu dokumentu): `planner.rs` ma **1772 linie kodu produkcyjnego**
i nie było go na liście, mimo że przekracza próg błędu tak samo jak dziewięć pozostałych.

Szew biegnie wzdłuż czterech faz planera doby: faza 1 (zobowiązania) podróżuje i jest
niewywłaszczalna, fazy 2 i 4 wypełniają luki tam, gdzie mieszkaniec już jest, a faza 3 jako jedyna
pyta `PlaceProvider` i `TravelOracle` o wybór miejsca. Kanwa i uzasadnienia są wspólne dla czterech.

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `planner/canvas.rs` | `Gap`, `DayCanvas`, `PlanStats`, `insert`, `remove`, `gaps`, `place_at`, `origin_of`, `slot_starting_at`, `required_place_at`, `first_start`, `load`, `wstaw`, `podsumuj`, `render_day_debug` | 375 |
| `planner/commitments.rs` | faza 1: `faza1_zobowiazania`, `absencja`, `poprzedni_dzien`, `wstaw_zobowiazanie`, `dojazd_przed`, `powrot`, `dojazd_po`, `minuty`, `wstaw_dojazd`, `odprowadzenie_osobne`, `SCHOOL_OPEN`, `SCHOOL_CLOSE`, `ESCORT_HANDOVER_MIN` | 380 |
| `planner/rhythm.rs` | fazy 2 i 4: `faza2_potrzeby`, `chronotyp`, `ostatni_koniec`, `najdluzsza_luka`, `wstaw_w_oknie`, `faza4_czas_wolny`, `wybierz_zajecie`, `PREP_MIN`, `HYGIENE_TRIGGER`, `LEISURE_MIN_GAP` | 330 |
| `planner/tasks.rs` | faza 3: `Zadanie`, `lista_zadan`, `Wybor`, `Okno`, `faza3_zadania`, `najlepsza_realizacja`, `rozwaz`, `aktywnosc_zadania`, `W_DETOUR`, `STOCK_THRESHOLD_DAYS`, `HEALTH_TRIGGER`, `CLOTHING_TRIGGER` | 430 |
| `planner/mod.rs` | `MAX_SLOTS`, `HouseholdView`, `PlanCtx`, `ReasonEntry`, `ReasonLog`, `Log`, `plan_day`, `plan_day_explained`, `uloz`, `replan*`, `request_replan`, `tick_replan_cooldown`, `store_plan`, `load_plan` | 410 |

`Log` zostaje w `mod.rs`, a nie w osobnym `reasons.rs`: dotyka go każda z czterech faz, więc osobny
plik miałby 160 linii i sześć krawędzi zależności zamiast jednej. To samo rozstrzygnięcie co przy
kluczach strumieni w `demography/mod.rs` (R-WP7). Nazwa `planner/rhythm.rs`, a nie `needs.rs`, bo
`needs` jest w tym crate'cie zajęte przez `crate::needs::NeedTable`.

**Ostrzeżenie o determinizmie.** Losowania są dwa i oba idą przez `PlanCtx::rng_for`, czyli
`rng(seed, StreamId::DayPlan, citizen_idx, Tick(day*1440 + minute))` — strumień jest wyprowadzany
z minuty, nie sekwencyjny, więc samo przeniesienie kodu nie przesuwa stanu RNG. Zmienia go
natomiast każde dotknięcie argumentu `minute`. Poza tym w hash wchodzą cztery kolejności:
iteracja po `NeedKind::ALL` i `StockCat::ALL` (remisy rozstrzyga pierwszy w tablicy), kolejność
zadań w `faza3_zadania` (każde wstawienie przesuwa indeksy slotów), kolejność dwóch przebiegów
w `najlepsza_realizacja` (luki przed slotami `Commute` — `rozwaz` rozstrzyga remis przez „pierwszy
wygrywa") i dwa przebiegi `for dlugie in [true, false]` w `faza4_czas_wolny`.

Jedyna niemechaniczna część podziału to widoczności: `Gap`, `Log`, `wstaw`, `podsumuj`, `uloz`,
`PlanCtx::rng_for`, `minuty`, cztery `faza*` i prywatne metody `DayCanvas` muszą stać się
`pub(super)`. Nic z tego nie wypływa poza `planner/`, bo `mod.rs` re-eksportuje dzisiejsze 16
symboli bez zmiany.

**Kryterium:** hash świata po 360 dobach niezmieniony; `det_plan_pure` i `plan_explain_matches`
bez zmian; złoty wydruk `render_day_debug` bajt w bajt; `lib.rs` i `systems.rs` bez zmiany ani
jednej linii; żaden nowy plik powyżej 500 linii.

---

### R-WP12 — `sim/economy/src/market.rs` (3228 → katalog `market/`, 9 plików)

Jedenasty plik listy, zapowiedziany w trzech tabelach „Zmiany wpisane po M5c/M5d/M5e" na końcu
tego dokumentu, ale bez własnego wiersza w §4 — pakiet dopisany po R-WP1 (`D-1`). Największy plik
produkcyjny w repo i jedyny z **trzema** blokami `impl` powyżej 500 linii (860, 821, 510).
M6 dopisuje do niego rynek B2B, więc termin „przed M6a" obowiązuje tak samo jak przy R-WP5.

**Kierunek: `market.rs` → katalog `market/`, nie moduły siostrzane.** To warunek techniczny,
nie estetyka: Rust udostępnia elementy prywatne modułowi definiującemu **i jego potomkom**, więc
`market/fulfil.rs` widzi wszystkie pola `MarketInner` przez `use super::*`. Wariant siostrzany
(`market_fulfil.rs`) wymagałby `pub(crate)` na trzydziestu polach i jest z tego powodu odrzucony.
Wpis po M5c mówił „przez `pub(crate)` na polach albo przez `impl Market` w modułach potomnych" —
druga droga jest darmowa, pierwsza nie jest potrzebna wcale.

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `market/mod.rs` | nagłówek, importy, stałe półki, `PurchaseIntent`, `PlannedPurchase`, `MarketStats`, `HouseholdSnapshot`, `ShopSeed`, `Bank`, `HouseholdMonth`, `HouseholdMonthReport`, `MarketInner`, `Market`, `new`, `lock`, `set_tick`, `tick`, `rebuild_index`, `refresh_households`, `snapshot`, `has_home_stock`, `vot_for`, `buyer_state`, `post_purchase`, `post_receipt`, `wholesale_memo`, `impl HashState for Market` | 470 |
| `market/api.rs` | szew (d) i gettery: `stats`, `shop_count`, `offer_count`, `price_at`, `set_price`, `shelf_qty`, `backroom_qty`, `inventory_value`, `offer_of`, `set_supply_shock`, `good_of_key`, `shop_pos`, `account_of`, `rest_of_world`, `sites`, `set_tracking`, `lost_sales`, `lost_histogram`, `policy_of`, `reprice_log`, `observed_of`, `observe_delay`, `elasticity_of`, `income_statement`, `balance_sheet`, `cash_flow`, `ledger_balance` | 250 |
| `market/lifecycle.rs` | szew (a): `open_shop`, `record_capital`, `stock_initial`, `deliver_now` | 270 |
| `market/restock.rs` | szew (b): `restock_shelves`, `reorder_and_receive`, `expire_goods`, `docelowy_zapas` | 250 |
| `market/price_day.rs` | szew (c): `observe_competitors`, `reprice_all`, `set_policy`, `preview_policy`, `price_of`, `REPRICE_LOG` | 290 |
| `market/close.rs` | szew (f′), kapitał obrotowy firmy: `close_month`, `service_working_capital`, `maybe_borrow_working_capital` | 250 |
| `market/household.rs` | szew (f), gospodarstwo i kredyt konsumencki: `household_month`, `open_bank`, `bank`, `budget_of`, `loan`, `loan_count`, `credit_outstanding`, `budget_log`, `cpi_*`, `base_rate`, `roll_cpi_day`, `close_cpi_month`, `log_budget`, `take_from_household`, `pay_installment`, `apply_for_credit`, `overspend_bp` | 380 |
| `market/fulfil.rs` | szewy (e) i (h): `impl PlaceProvider for Market`, `Market::fulfil`, `take_intents`, `return_goods`, `record_sale`, `cats_of`, `shop_coord`, `who_key`, `MAX_LINES` | 620 |
| `market/readout.rs` | szew (g): `balance_sample`, `shop_panel` | 310 |

**Kolejność wewnątrz pakietu, od najbezpieczniejszego:** `readout.rs` → `api.rs` → `household.rs`
→ `lifecycle.rs` → `restock.rs` → `price_day.rs` → `close.rs` → `fulfil.rs`. Pierwsze dwa nie mogą
zmienić hasha nawet przy błędzie implementacji: `hash_state` haszuje `offers`, `shops`, `intents`,
`budgets`, `loans`, `bank` i `cpi`, a te dwa pliki biorą `self.lock()` bez `mut`. Ostatni jest
jednocześnie największy i jedyny dotykający `K-6`.

**Ostrzeżenie o determinizmie.** W pliku nie ma stanowego RNG — pięć losowań jest licznikowych,
wyprowadzonych z `MarketInner.seed`, więc przeniesienie metody jest neutralne pod warunkiem, że
argumenty kluczujące (`firm.entity().index()`, `t`, `world_seed`) nie zmienią się ani o jeden.
Ryzyko leży w pokusie „przy okazji" uporządkowania parametrów. Poza tym trzy porządki iteracji
wchodzą do wyniku i **nie wolno ich ujednolicać**: `by_site` (`BTreeMap<SiteId, u32>`, w hashu),
`shops` po indeksie (kolejność zakładania sklepów — inna niż `by_site`; tak chodzą `expire_goods`,
`restock_shelves`, `reorder_and_receive` i `close_month`) oraz `controllers` po `GoodId`. Sortowanie
w `candidates` jest jedynym miejscem, w którym ustala się kolejność sumowania w softmaksie (`K-6`):
sort → obliczenie → `choose_offer` zostaje w tej kolejności. Bufory (`offer_buf`, `cand_buf`,
`util_buf`, `order_buf`, `deliv_buf`, `obs_buf`, `entry_buf`) nie wchodzą do hasha, ale karmią
te sortowania — zmiana miejsca `clear()` jest niewidoczna w diffie i widoczna w hashu.

**Kryterium:** `run_mezo.hashes` niezmienione; `MarketStats`, `BalanceSample` i `ShopPanelSnapshot`
identyczne co do pola na przebiegu bramek G1–G9 balansatora; żaden blok `impl` powyżej 500 linii;
`market/mod.rs` poniżej 500.

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

**Sprzątanie:** usunięcie wszystkich `pub use` rusztowań dodanych w R-WP2…R-WP9, R-WP11 i R-WP12 oraz poprawienie
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
5. **Żaden blok `impl` powyżej 500 linii i żadna funkcja powyżej 250 — poza pozycjami
   rejestru długu strukturalnego na końcu tego dokumentu.** Klauzula wyjątków dopisana przy
   domknięciu R1 (`D-31`), symetrycznie do kryterium nr 4. Wyjątek nie jest darmowy: każdy
   wymaga wiersza w rejestrze z nazwanym sufitem, powodem, dla którego podział nie jest
   przeniesieniem bloku, i fazą-właścicielem, która będzie miała powód inny niż metryka.
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

**`D-R6` — czy `sim/traffic/src/trip.rs` wchodzi do R-WP6. — ROZSTRZYGNIĘTE: tak, przyjęte domyślnie i wykonane w R-WP6.** Plik ma 964 linie i blok `impl`
na 590. Propozycja: tak, w tym samym pakiecie co `oracle.rs`, bo te dwa pliki dzielą typy
(`TripHandle`, `PendingTrip`) i dzielenie ich osobno oznacza dwa razy tę samą analizę zależności.

**`D-R8` — `benches/baseline.json` nie był aktualizowany od M3 i `bench_guard` nie mierzy
jednej trzeciej benchmarków.** Znalezione przy domknięciu R1, przy pierwszym w całym R1
przebiegu kryterium akceptacji nr 3. Z 41 pozycji **13 zgłasza `NOWY — brak w linii bazowej`**:
cały planer (`plan_day`, `plan_day_explained`, `replan`), mikro pieszych, `estimate` z cache,
trzy pozycje demografii i społeczeństwa, Etap 8 (`m3d-1`), cztery pozycje indeksu parcel
i jeden shard potrzeb. Dla nich `bench_guard` **nie mierzy niczego** — i robi to cicho, bo
brak wpisu jest informacją, nie błędem. To ta sama klasa problemu co `D-R7`: bramka, która
raportuje zielono, nie sprawdzając tego, co myśli, że sprawdza.

Propozycja: **odnowić linię bazową w pierwszym commicie po R1**, na sprzęcie odniesienia
i **nie w tym samym commicie co refaktor** — zapisanie liczb po R1 jako odniesienia jest
uprawnione (pomiar wyżej pokazuje, że R1 niczego nie spowolnił: 28 pozycji `OK`, maksymalne
odchylenie `+8,3 %` przy progu ostrzeżenia 10 %), ale zrobione **razem** z refaktorem
zamieniłoby dowód w założenie. Do rozstrzygnięcia: czy linię bazową odnawia się odtąd
przy zamknięciu każdej fazy, czy tylko przy świadomej zmianie wydajności (`D-8` z M0 mówi
to drugie, a praktyka sześciu faz pokazała, że wtedy nikt tego nie robi).
*Nieblokująca dla niczego w R1.*

**`D-R7` — test akceptacyjny M3d pada na `master` i nie ma go w CI.** Znalezione przy R-WP4.
**Cztery testy akceptacyjne M2d i M3d padają na `master` i żaden z nich nie biegnie w CI.**
Znalezione przy R-WP4 i R-WP5, potwierdzone niezależnie na czystym `HEAD` (`git stash`):

| Plik | W CI z `--include-ignored`? | Stan | Co pada |
|---|---|---|---|
| `tests/determinism.rs` | tak | zielony | — |
| `tests/city.rs` | tak | zielony | — |
| `tests/city_m2c.rs` | tak | zielony | — |
| `tests/city_m2d.rs` | **nie** | **3 z 16** | `rozbicie_wyceny_sumuje_sie_do_wyniku` (`Money(17523)` ≠ `Money(15939)`), `zaden_budynek_nie_wisi_nad_terenem` (parcela 4993, dziura na indeksie 43 przy terenie 55), `park_i_kopalnia_zostaja_bez_zabudowy` („budynek w strefie green") |
| `tests/population.rs` | **nie** | **1 z 7** | `doba_przez_systemy_ecs_planuje_dowozi_i_zaspokaja` (`31971` ≠ `44188` zakończonych podróży) |

Wszystkie cztery są `#[ignore]`, więc `cargo test --workspace` ich nie widzi, a `ci.yml` woła
z `--include-ignored` wyłącznie trzy pierwsze pliki. Bramka, którą §7 pkt 2 nazywa „zielony
`master`", jest więc **spełniona i pusta naraz**: cztery testy akceptacyjne M2d i M3d nie biegły
nigdzie od nieznanej liczby faz. Identyczność liczb przed podziałem i po nim jest przy okazji
dodatkowym dowodem, że R1 niczego nie przesunął.

Propozycja: **nie naprawiać tego w R1** — rozstrzygnięcie, czy pada test, czy pada kod, jest
zmianą zachowania (§2 „nie wchodzi"). Dopisać brakujące pliki do joba `determinism` w `ci.yml`
**dopiero razem z diagnozą**, bo job czerwony od pierwszego dnia zostanie wyłączony (ryzyko R-5).
Dwa z trzech przypadków `city_m2d` leżą w plikach, które R-WP8 i R-WP9 i tak otworzą — jeśli
podział cokolwiek o nich powie, trafi to tutaj. *Nieblokująca dla żadnego pakietu R1.*

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
| R-WP11 | `sim/agents/src/planner.rs` | 1 818 | 5 | M |
| R-WP12 | `sim/economy/src/market.rs` | 3 228 | 9 | L |
| R-WP10 | `transit.rs`, `micro.rs` + prostowanie importów | 2 897 + rozsiane | 6 | M |
| | **Razem** | **~23 600 z 74 600 linii kodu produkcyjnego (32 %)** | **60 z 11** | |

Żaden wiersz nie wnosi ani nie usuwa funkcjonalności. Liczba linii po podziale ma być większa
o narzut nagłówków modułów i deklaracji `use` — szacunkowo 3–5 %, i to jest cena, nie oszczędność.

---

## Rejestr długu strukturalnego

Tabela zasilana przez regułę z §6, kiedy odpowiedź na pytanie 3 brzmi „nie dzielę teraz".
Pusta na start. Wiersz wpisuje się w tym samym commicie, w którym próg został przekroczony.

| # | Plik | Metryka i wartość | Świadomie zostawione, bo | Ścieżka wyjścia |
|---|---|---|---|---|
| 1 | `tools/magnat/src/app.rs` | blok `impl App` — 460 linii (ostrzeżenie od 300) | Dziesięć metod pętli okna, każda potrzebuje `&mut self` do tych samych pól stanu klienta. Podział przez `impl App` w modułach potomnych jest możliwy, ale dopiero wtedy, gdy będzie wiadomo, wzdłuż czego — dziś nie ma drugiego tematu, jest jeden temat o dziesięciu krokach | M11 (prezentacja) dokłada tu animacje, wnętrza i budżet klatki. Wtedy szew się pokaże sam i próg błędu 500 padnie w tej samej fazie |
| 2 | `tools/magnat/src/app.rs` | `App::klatka` — 249 linii (błąd od 250) | Robi w jednym ciągu osiem rzeczy (czas gry, clamp kamery, scena pomiarowa, strumieniowanie, zrzut PNG, próbka benchmarku, klatka UI, tytuł okna) i **każdy podział przestawia kolejność wywołań**, czego R1 §3 reguła 2 zakazuje bezwarunkowo | M11b (animacja i LOD) albo M11e (budżet klatki) — obie i tak muszą tę funkcję rozłożyć, żeby zmierzyć koszt per etap |
| 3 | `tools/magnat/src/main.rs` | `main` — 154 linie (ostrzeżenie od 150), z czego ~80 to literał `App { … }` o 46 polach | Ten literał jest jedynym powodem, dla którego 46 pól `App` musi być `pub(crate)`. Zamiana na `App::from_args(...)` kasuje jedno i drugie, ale to **nowy symbol i zmiana kształtu wywołania** — poza zakresem R1 (§2 „nie wchodzi") | Konstruktor `App::from_args` przy pierwszej fazie, która i tak dotyka konstrukcji klienta (M9b rdzeń UI albo M11) |
| 4 | `engine/render/src/renderer/pipelines.rs` | plik — 1122 linie (ostrzeżenie od 800) | Plik jest jednym ciągiem **kolejności tworzenia zasobów GPU**, a nie zbiorem tematów. Granicą jest tu kolejność, nie temat: dalszy podział przenosi wycinek dawnego `new` do drugiego pliku i utrudnia sprawdzenie, że kolejność się zgadza — czyli psuje jedyną własność, która w tym pliku ma znaczenie | M11 dokłada tu potoki. Gdy plik przekroczy 1200, naturalny podział to `pipelines/scene.rs` (voxel, water, sky) i `pipelines/screen.rs` (far, post, cluster) — dwa łańcuchy bez wspólnego `BindGroupLayout` poza `layout` i `overlay_layout` |
| 5 | `engine/render/src/renderer/pipelines.rs` | `fn scene_pipelines` — 190 linii (ostrzeżenie od 150) | Pięć `create_render_pipeline` w ustalonej kolejności plus trzy moduły shaderów i dwa układy wierzchołków. Rozbicie na pięć funkcji przestawiłoby `create_shader_module` względem `create_render_pipeline` — dokładnie to, czego §3 reguła 2 zakazuje | Ten sam podział `scene` / `screen` co w pozycji 4 rozwiązuje oba naraz |
| 6 | `engine/render/src/renderer/passes.rs` | `impl Renderer` — 471 linii (ostrzeżenie od 300) | Dziesięć metod nagrywania klatki sięgających tych samych pól potoków i buforów. Szew „nagrywanie" vs „wywoływanie" istnieje, ale `render_with_ui` i `record_passes` dzielą `FrameLists` i licznik trójkątów, więc rozdzielenie przepuściłoby je przez sygnaturę | M11e (budżet klatki) i tak musi rozłożyć `record_passes` na etapy, żeby zmierzyć koszt każdego |
| 7 | `engine/render/src/renderer/passes.rs` | `fn record_passes` — 183 linie (ostrzeżenie od 150) | Sześć przebiegów renderowania w jednym `CommandEncoder`; każdy domyka `RenderPass` przed otwarciem następnego. Kolejność jest kontraktem sterownika, a `pass` nie przeżywa granicy funkcji bez przepisania czasów życia | M11e |
| 8 | `engine/render/src/renderer/frame.rs` | `fn prepare_frame` — 169 linii (ostrzeżenie od 150) | Cztery zapisy uniformów, budowa bufora per-chunk i trzy przebiegi cullingu w jednej sekwencji. Podział na `write_uniforms` + `cull` jest wykonalny, ale wektor widocznych chunków żyłby między nimi jako typ pośredni istniejący tylko po to, żeby zaraz się rozpakować | M11b (animacja i LOD) dotyka cullingu — wtedy `cull` dostanie własny powód istnienia |
| 9 | `engine/render/src/renderer/frame.rs` | `impl Renderer` — 305 linii (ostrzeżenie od 300) | Pięć linii ponad progiem. Dzielenie pliku o tyle jest kosztem bez zysku | Zniknie samo, gdy pozycja 8 zostanie kiedyś podzielona |
| 10 | `engine/render/src/renderer.rs` | `impl Renderer` — 386 linii (ostrzeżenie od 300) | `new` (94) plus `resize` i siedem setterów, z czego `set_overlay` (92) i `upload_far_terrain` (99) to dwie trzecie reszty. Obie wgrywają teksturę i przebudowują grupę wiązań; `renderer/upload.rs` byłby dziś plikiem o dwóch funkcjach bez wspólnego stanu | M2 podpina własne pola pod nakładkę (§6.1). Gdy ścieżek wgrywania będzie więcej niż jedna, `upload.rs` dostanie temat |
| 11 | `sim/world/src/population/homes.rs` | `fn dopasuj_mieszkania` — 227 linii (ostrzeżenie od 150) | Lustro stanu, rangi dochód↔wartość, pomiar przedziału median i **pętla poprawkowa 200 tys. prób zamiany** to jeden ciąg: `adres`, `zaloga`, `hist`, `krotszych` i `odchylenie` żyją przez wszystkie cztery etapy i są aktualizowane przyrostowo. Każdy podział wypycha ten stan przez sygnaturę albo przestawia kolejność prób — a ta kolejność **jest częścią hasha** (§3 reguła 1 i ostrzeżenie w §4 R-WP4) | Faza, która zamieni zachłanną wymianę na wyżarzanie. Sufit jest już nazwany komentarzem `ponytail:` nad funkcją; wtedy „pomiar przedziału" i „pętla" rozejdą się same |
| 12 | `sim/world/src/population/mod.rs` | `fn generate_population` — 211 linii (ostrzeżenie od 150; **było 280, czyli nad progiem błędu**) | To jest lista dziesięciu kroków i ma prawo być długa — jedyne miejsce, z którego widać całą sekwencję Etapu 8. Dalsze skracanie wymaga wyniesienia kroku 11 (flota, 35 linii), a ten sięga `crate::traffic_build` w ośmiu miejscach i nie ma właściciela w `population/` | Faza dotykająca floty: gdy `traffic_build` dostanie własny „krok 11" jako jedną funkcję, `generate_population` schodzi poniżej 180 |
| 13 | `sim/world/src/population/mod.rs` | metryka `mod.rs` — 365 linii kodu poza deklaracjami (ostrzeżenie od 300; **było 2138**) | Poza `generate_population` (211) zostają wyłącznie typy wejścia i wyjścia Etapu 8 plus cztery drobiazgi mostu. `population/api.rs` byłby plikiem o czterech typach bez zachowania | Zniknie razem z pozycją 12 |
| 14 | `sim/world/src/city/sites/place.rs` | plik — 949 linii (ostrzeżenie od 800; §4 szacowało 750) | Jeden ciąg pięciu kroków obsady, w którym stan idzie przez cztery zmienne lokalne `populate`: `kandydaci`, `przydzialy`, `grupa`, `rep`. Kroki 1–4 mutują wszystkie cztery, krok 5 czyta wynik. Podział wypchnąłby czwórkę przez sygnaturę i przestawił moment, w którym rośnie `grupa` — a numer grupy firmy wchodzi do `rng(seed, StreamId::FirmSeed, …)`, czyli do nazw firm i **do hasha** | M6 dokłada tu magazyny i zakłady. Wtedy `place/workplaces.rs` (korekta F1: `rebind_workplaces` + `licz_stanowiska`, ~130 linii, nie dotykają ani `Kand`, ani `Przydzial`) wychodzi za darmo, a reszta schodzi do ~810 |
| 15 | `sim/world/src/city/sites/closure.rs` | `fn supply_closure_check` — 194 linie (ostrzeżenie od 150; **wartość niezmieniona od dawnego pliku**) | Pięć kroków §5.8 dzielących pięć wektorów stanu (`import`, `demanded`, `osiagalne`, `importowalny`, `budzet_importu`) i sześć przebiegów o stałym budżecie. Wyniesienie KROKU 5 przepuściłoby przez sygnaturę cztery wektory i `&mut [SiteSeed]`, a dowieziony import liczy się **od stanu końcowego**, nie narastająco — rozdzielenie kusi do zwrócenia importu z pętli, czyli do błędu, który komentarz w środku opisuje jako już raz naprawiony | M6d przejmuje funkcję w całości i zamienia stały budżet bram na dynamiczny (wpisane do `M6d-*.md` przez `K-18`). Wtedy „naprawa" i „bilans" rozejdą się same, bo przestaną być jednorazowe |
| 16 | `sim/world/src/city/sites/place.rs` | `fn posadz_produkcje` — 168 linii (ostrzeżenie od 150; **wartość niezmieniona**) | Trzy podkroki (3a klastry wg szablonów, 3b reszta, 3c rozluźnienie strefy) czytające i zmniejszające ten sam wektor `zostalo` oraz ten sam `kand`. Kolejność 3a→3b→3c jest kolejnością zajmowania działek, czyli wprost hashem | Ta sama faza co pozycja 14 |
| 17 | `sim/traffic/src/trip/minute.rs` | `impl TrafficNetwork` — 432 linie (ostrzeżenie od 300; **było 596 w jednym pliku**) | Jeden ciąg życia minuty: `step_minute` → `dispatch` → `advance` → `finish`, w którym `ActiveTrip`, kopiec odjazdów i `stats` są aktualizowane przyrostowo przez wszystkie cztery. Rozcięcie między `dispatch` a `advance` przecina pipeline w połowie, a nie wzdłuż tematu, a kolejność wywołań jest wprost hashem (§3 reguła 2) | M6 dokłada ruch towarowy i rampy. Gdy `dispatch` przestanie być jedną ścieżką (osobne wpuszczanie ciężarówek), `minute/freight.rs` wyjdzie z własnym powodem istnienia |
| 18 | `sim/traffic/src/oracle.rs` | `impl TrafficOracle` — 381 linii (ostrzeżenie od 300; **było 774**) | Po wyjęciu ofert i podróży zostaje `new` plus ~25 getterów i setterów oraz cztery zapytania trasujące, wszystkie sięgające tych samych zamków (`parking`, `transit`, `router`). `oracle/routing.rs` byłby dziś plikiem o czterech funkcjach dzielących stan z resztą przez `self` | M4/M6 przy `TravelTimeMatrix` per dzielnica × godzina: wtedy trasowanie dostanie własny stan i własny temat |
| 19 | `sim/traffic/src/oracle/offers.rs` | `impl TrafficOracle` — 363 linie (ostrzeżenie od 300) | Pięć wariantów jednego kontraktu plus `plan`, które je porównuje. Dalszy podział albo rozdziela warianty po plikach — niszcząc możliwość porównania ich wzrokiem, czyli **cel** podziału z §4 — albo sprowadza je do wspólnej abstrakcji, czego §4 zakazuje wprost | M4c+/M6: gdy dojdzie szósty i siódmy środek (rower miejski, dostawa), podział `offers/individual.rs` i `offers/shared.rs` wyjdzie sam |
| 20 | `sim/traffic/src/oracle/journey.rs` | `fn start_trip` — 167 linii (ostrzeżenie od 150; **wartość niezmieniona**) | Jeden ciąg: decyzja → rezerwacja pojazdu → trasa (z ewentualnym postojem na stacji) → `PendingTrip` → wpis do kolejki. Każdy wyniesiony fragment przepuszcza `ModeDecision`, `TripRequest` i `RouteQuery` przez sygnaturę | Faza, która rozdzieli tankowanie od wyruszenia — M6, gdy stacja paliw stanie się zakładem z zapasem |
| 21 | `sim/agents/src/demography/mod.rs` | metryka `mod.rs` — 495 linii kodu poza deklaracjami (ostrzeżenie od 300; **było 1871 w jednym pliku**) | Poza typami wejścia i wyjścia doby oraz miesiąca (`Population` 140 linii, `LifeQueue` 99, dwa raporty, osiem kluczy `K_*`) zostaje tu piątka relacyjna (98 linii), która **nie ma dokąd pójść** — powód w `D-16`. Osobny `relations.rs` zamieniłby jedną krawędź zależności na pięć i zostawiłby `mod.rs` na 397 liniach, czyli nadal nad progiem ostrzeżenia | Faza, w której narodziny i zgon przestaną same pisać relacje rodzinne. Wtedy `demography` przestaje wołać `powiaz`, kierunek `social → demography` zostaje jednostronny **także po przeniesieniu**, piątka jedzie do `social.rs` tak, jak §4 R-WP7 zakładał, a `Population` i `LifeQueue` dostają przy okazji powód, żeby wyjść do `demography/registry.rs` |
| 22 | `sim/world/src/city/build.rs` | `fn plan_building_inner` — 160 linii (ostrzeżenie od 150; **wartość niezmieniona od dawnego pliku**) | Jeden ciąg: filtr strefy → oś frontu i pierzeja → rozpiętość działki → `PickCtx` → `pick_grammar` → `footprint_for` → niwelacja → trzy pomiary `zapas` → `derive`. Każdy wyniesiony fragment przepuszcza przez sygnaturę `PickCtx`, `Scope`, parę `(lo, hi)` i marginesy, a kolejność losowań ze strumienia `StreamId::BuildingPick` **jest hashem** — `pick_grammar` losuje raz na cały dobór właśnie po to, żeby działka po drugim podejściu nie przesunęła strumienia sąsiadce | M6 dokłada tu zakłady i magazyny. Jeśli wtedy powstanie drugi **temat**, a nie drugi krok tego samego, szew będzie widać |
| 23 | `sim/world/src/city/build/capacity.rs` + `build/interiors.rs` | cztery liczby kalibracyjne bez nazwy (`1.025` — środek okna T10, czyli **cel kalibracji**; `12.0` — faktyczny twardy sufit `GESTOSC_MAX`; `0.02` — próg „nie ma czego kalibrować") i **jedna zduplikowana**: `28.0` m² na pokój żyje w `rescale_dwellings` **i** w `rodzaj` | §2 R1: ani jednej zmiany zachowania. Nazwanie stałej jest darmowe, ale scalenie duplikatu `28.0` zmienia liczbę miejsc, w których reguła domenowa żyje, i jest pracą właściciela kalibracji, nie refaktoru. Podział to **uwidocznił** — obie kopie wylądowały w różnych plikach | M5e (balansator): to on ma powód, żeby te liczby ruszyć, i on pierwszy zobaczy, że jedna jest w dwóch egzemplarzach. Jeśli ruszy tylko jedną, sylwetka mieszkań rozjedzie się z metrażem |
| 24 | `sim/world/src/city/mod.rs` | `fn generate_city` — **368** linii (błąd od 250; 365 do M6d, +3 w M6e za bilans otwarcia złóż `AP-1`) | To jest lista etapów 3–7 i ma prawo być długa, jak `generate_population` (pozycja 12) — **ale 147 z 365 linii to nie etapy**: 76 linii składania `warnings` i 71 linii literału `GenerationReport { … }`. Wyniesienie pierwszego przepuszcza przez sygnaturę siedem struktur plus `bez_frontu`, czytane też przez drugie; wyniesienie drugiego to konstruktor o ~20 parametrach. To jest **zmiana kształtu, nie przeniesienie bloku**, a konstruktor powołany po to, żeby zbić metrykę, jest gorszy od metryki | **M6a**: ta faza dokłada do `GenerationReport` sekcję łańcucha dostaw, więc `report::zbierz(…)` dostaje wtedy powód istnienia inny niż liczba linii. `generate_city` schodzi do ~290, a wyniesienie `warnings` w tej samej fazie — poniżej 220 |
| 25 | `sim/agents/src/planner/mod.rs` | metryka `mod.rs` — **366** linii kodu poza deklaracjami (ostrzeżenie od 300; **było 1772 w jednym pliku**) | Poza czterema typami kontraktu i dwoma orkiestratorami zostaje tu `Log` (41 linii), który **nie ma dokąd pójść**: pisze do niego każda z czterech faz, więc `planner/reasons.rs` zamieniłby jedną krawędź zależności na sześć i zostawiłby `mod.rs` na 325 liniach, czyli nadal nad progiem ostrzeżenia. Dokładnie ten sam układ co piątka relacyjna w `demography/mod.rs` (pozycja 21) | Faza, w której uzasadnienia przestaną być odtwarzane synchronicznie z planem. Wtedy `Log` przestaje być cieniem `DayCanvas`, trójka `ReasonLog` + `Log` + `ReasonEntry` (90 linii) dostaje własny powód istnienia, a `mod.rs` schodzi do ~276 |
| 26 | `sim/economy/src/market/fulfil.rs` | `fn PlaceProvider::candidates` — **258** linii (błąd od 250; **wartość niezmieniona od dawnego pliku**) | **Stan zastany, nie skutek podziału** (na `master` ten sam błąd, linie 2696–2953). Zejście poniżej 250 wymaga wycięcia bloku, a każdy sensowny blok przecina ciąg `offer_buf.clear()/extend` → sortowanie `cand_buf` → softmax `util_buf` → `choose_offer`, czyli **jedyne miejsce, w którym ustala się kolejność sumowania w softmaksie (`K-6`)** — wprost objęte zakazem z §4 R-WP12 | Faza, która rozstrzygnie, czy zbieranie ofert w zasięgu (pierwsze ~60 linii, przed pierwszym sortowaniem) da się wyjąć jako osobną funkcję **bez przesunięcia `clear()`**. Dopóki nie wiadomo, że tak, cięcie jest ryzykiem bez zysku | **Wykonane w M6d/WP11.** Odczyt półki z magazynu (`shelf_units`, `shelf_pick`) przepchnął funkcję z 254 na ponad twardy próg, więc pytanie „czy da się wyjąć zbieranie ofert bez przesunięcia `clear()`" trzeba było zadać na serio. Odpowiedź: **da się**, i to bez dotykania ciągu, o który chodziło. `zbierz_kandydatow` bierze oferty w zasięgu, odsiewa nieznane i za małe i zamienia je w kandydatów; `clear()` buforów, sortowanie, softmax i `choose_offer` zostają w `candidates` w niezmienionej kolejności. 254 → 226 linii, zero zmian zachowania (hashe `m5shop` zmieniły się z powodu migracji WP11, nie z powodu podziału)
| 27 | `sim/economy/src/market/fulfil.rs` | `fn Market::fulfil` — 218 linii (ostrzeżenie od 150; wartość niezmieniona) | Jedna ścieżka decyzyjna z ośmioma powodami odmowy. Rozcięcie jej znaczy rozcięcie `FulfilOutcome`, czyli zmianę kontraktu, a nie przeniesienie bloku | Ta sama faza co pozycja 26 |
| 28 | `sim/economy/src/market/readout.rs` | `impl Market` — 313 linii (ostrzeżenie od 300) | Dwie metody **czysto odczytowe**, po ~150 linii każda (`balance_sample`, `shop_panel`). Podział na dwa pliki po 160 linii dokłada krawędź importową bez zysku — a to jest jedyny szew `market.rs`, o którym wiadomo, że nie może zmienić hasha nawet przy błędzie implementacji | M5e/M9: gdy panel dostanie drugą zakładkę czytającą inne pola, `readout/` dostanie temat |
| 29 | `sim/economy/src/market.rs` | plik — **571** linii rodzica (poniżej progu ostrzeżenia 800, ale powyżej liczby 500 z kryterium §4 R-WP12) | Rodzic jest właścicielem stanu: `MarketInner` z trzydziestoma polami, `new`, `lock`, akcesory ticka, migawka kupującego i `impl HashState`. Sam nagłówek modułu (39), blok `use` (43), definicje typów (216) i `HashState` (48) to 346 linii, zanim dojdzie cokolwiek wykonawczego — liczba 500 z §4 była przy tym przydziale treści **nieosiągalna** | M6 dopisuje tu rynek B2B. Jeśli plik przekroczy 800, następnym szwem jest wyniesienie definicji typów (`PurchaseIntent`, `ShopSeed`, `Bank`, `HouseholdMonth*`, `MarketStats` — 216 linii) do `market/types.rs`, co zostawia rodzicowi ~355 linii samego stanu |
| 30 | `sim/traffic/src/transit/sim.rs` | `impl TransitNetwork` — 360 linii (ostrzeżenie od 300; **było 689 w jednym pliku**) | Jeden ciąg życia minuty `step_minute` → `dispatch` → `advance` → `give_up`, ten sam kształt co pozycja 17 (`trip/minute.rs`). `przesiadka` i `give_up` dzielą z `advance` kolejkę oczekujących i licznik `stats` | Zniknie razem z pozycją 31 |
| 31 | `sim/traffic/src/transit/sim.rs` | `fn advance` — 176 linii (ostrzeżenie od 150; **wartość niezmieniona od dawnego pliku**) | Cztery etapy przystanku w jednej pętli po kursach, z reborrowami `&mut self.runs[ri]` trzymanymi **przez granice etapów**, `mem::take` na kolejce w środku etapu 2 i `swap_remove` po pętli w odwrotnej kolejności. Wyjęcie etapu ponownie pobiera każdy reborrow w innym punkcie — to **przepisanie struktury pożyczek, nie przeniesienie bloku** (§3 reguła 2). §4 R-WP10 dopuszcza podział tylko wtedy, gdy kolejność zdarzeń przeżyje; nie przeżywa | M6 dokłada ruch towarowy i rampy. Jeśli etap *wsiadka* doczeka się drugiego wołającego, wyjęcie go stanie się przeniesieniem, a nie przepisaniem |
| 32 | `sim/world/src/city/build.rs` | martwy re-eksport `pub use model::{… ShiftKey …}` — zero konsumentów w workspace | `clippy::upper_case_acronyms` na wariancie `III` milczy **wyłącznie dla API eksportowanego** (`avoid-breaking-exported-api = true` w `clippy.toml`). Usunięcie re-eksportu budzi lint, a uciszenie go wymaga `#[allow]`, czyli zmiany kodu — zakazanej w R1 §2. Wybór był między dwoma złami i padł na mniejsze | Dowolny commit spoza R1: `#[allow(clippy::upper_case_acronyms)]` nad `ShiftKey` albo przemianowanie wariantu `III`. Wpisane, bo bez tego wyjaśnienia wygląda na przeoczenie R-WP10 |
| 33 ★ | `sim/world/src/city/lsystem.rs` | `impl<'a> Builder<'a>` — **605** linii (błąd od 500) | **Jedyna z ośmiu pozycji, którą da się podzielić dziś i za darmo.** Szew po linii 422: do niej indeks i reguły terenowe (`cell`…`segments_at_node`, `forbidden`…`angle_ok`), od niej wzrost (`pattern_at`, `steer`, `grow`). Obie strony to przeniesienie bloku — potomek widzi prywatne pola `Builder` za darmo (`D-8`), więc zero `pub(crate)`, zero zmian sygnatur, zero zmian kolejności. Brakowało tylko pakietu w §4: plik ma 1154 linie, czyli **poniżej progu błędu dla metryki *plik***, i przez to nie trafił na listę ośmiu z §1 | Pakiet w `R2`, albo commit spoza R1. 257 z 605 linii to `grow` (pozycja 35) — samo jego wyniesienie zbija blok do 348, czyli do ostrzeżenia |
| 34 | `sim/world/src/city/lsystem.rs` | `fn grow_network` — **317** linii (błąd od 250) | Cała pętla L-systemu: aksjomat z bram, zalążek obwodnicy, kopiec propozycji i pięć reguł produkcji P0–P4 na wspólnym `props`/`heap`/`seq`/`b`. Szew po ~868 (zasiew / pętla) wymaga przepuszczenia przez sygnaturę czwórki `&mut` i zamiany domknięcia `push` na funkcję wolną; wewnątrz pętli szwu nie ma wcale. **`seq` jest licznikiem kolejności wepchnięć i zarazem kluczem losowania** `rng(seed, StreamId::RoadsL, prop.seq, …)` — każde przestawienie `push` przesuwa strumień wszystkim następnym propozycjom | Faza, która otworzy generator dróg. **Dziś nie planuje go żadna z M6–M12** — sprawdzone: M8e trzyma politykę drogową na krawędziach grafu M4, M10a zakłada drogi Etapów 3–5 jako gotowe, M12d wymienia tylko `city/grammar.rs` |
| 35 | `sim/world/src/city/lsystem.rs` | `fn Builder::grow` — **257** linii (błąd od 250) | Próba wyprowadzenia jednego segmentu, cztery etapy na sześciu zmiennych lokalnych, które każdy etap nadpisuje. Komentarz w środku mówi, dlaczego etap 3 istnieje: *snapowanie, przedłużenie i podział — każde z nich zmienia koniec odcinka*. Do tego `self.stats.rejected_*` przeplecione z siedmioma `return None`, więc wyniesienie fragmentu przesuwa moment, w którym licznik rośnie | Ta sama faza co pozycja 34. **Wyniesienie `grow` kasuje pozycję 33, ale nie kasuje tej** — 257 linii zostaje 257 liniami w nowym pliku |
| 36 | `sim/world/src/city/zoning.rs` | `fn assign_zones` — **322** linie (błąd od 250) | Cały WP7 strefowania w sześciu ponumerowanych krokach na wspólnej macierzy `score[n][16]`. Najczystszy jest krok 6 (bufor przemysłu ciężkiego, ~1010–1080) — przeniesienie bloku plus sygnatura na pięć argumentów; kroki 1–4 są splecione (krok 4 czyta macierz z 1 i kwoty z 3). **Rng nie ma tu wcale**; determinizm stoi na `sort_by(total_cmp)` z jawnymi remisami po `BlockId` i na kolejności kroków | ~~**M8e**~~ → **bez właściciela fazowego (sprostowanie po M8e, `CI-1`)**. Wyzwalacz nie zadziałał i tym razem nie dlatego, że przebudowa była za droga, tylko dlatego, że **`Policy::Zoning` nie powstał**: `Parcel.zone` czyta w całym repozytorium wyłącznie generator miasta (`city/build.rs`, `city/districts.rs`), a generator biegnie raz, przed pierwszym tickiem. Uchwała o przekwalifikowaniu parceli zmieniałaby pole, którego nikt już nie przeczyta — czyli byłaby wariantem bez skutku (`R2`). Ścieżki *przekwalifikuj jedną parcelę* potrzebuje dopiero faza, w której zabudowa reaguje na strefę **w trakcie gry**: rynek nieruchomości (M10) albo przebudowa miasta (M12). Do tego czasu pozycja zostaje w rejestrze jako dług bez wyzwalacza, a nie jako zadanie z adresem, który się nie sprawdził |
| 37 | `engine/ui/src/inspect/reason.rs` | `fn describe` — **618** linii (błąd od 250; 319 przed M6b, 355 po M6b, 451 po M7b, 566 po M7d) | Tabela `match`: jedno ramię na wariant `DecisionReason`, zero logiki, zero stanu, ma już `#[allow(clippy::too_many_lines)]`. Szew tematyczny istnieje, ale **`DecisionReason` jest enumem płaskim**, więc podział daje albo powtórzenie listy wariantów we wzorcach (dwa miejsca do zapomnienia zamiast jednego), albo pomocnicze funkcje zwracające `Option<String>` — a to **kasuje wyczerpującość sprawdzaną przez kompilator**, czyli jedyną rzecz, która gwarantuje, że nowy wariant nie przejdzie bez tekstu | ~~**M7c**~~ → **osobna zmiana** (patrz sprostowanie w opisie). Przebudowa `DecisionReason` na `Citizen \| Firm \| City` z `M7c-polityki-i-menedzerowie.md` §5.11 zostaje jedyną drogą wyjścia; po tym `describe` rozpada się **z konstrukcji**, a każdy podzbiór zachowuje własną wyczerpująceść. Uwaga: **M6 dopisuje tu wcześniej sześć wariantów** (`Shortage`, `SupplierChosen`, `ContractSigned`, `ExportChosen`, `SubstituteUsed`, `ProductionHalted`), więc funkcja najpierw rośnie o ~80 linii. **Wykonane:** M6b dołożył trzy (355), M6c pozostałe trzy (397) — prognoza się sprawdziła, blok M6 jest zamknięty. **M7b dołożył blok M7** (`Hired`, `WageRaise`, `JobLeft`) i **451**: pięć ramion na trzy warianty, bo `WageRaise` z przyrostem zerowym jest innym zdaniem („stawka bez zmian, sufit marży") niż podwyżka o zero, a `Hired` bez drugiego kandydata innym niż `Hired` z remisem. **Sprostowanie po M7c: wyzwalacz nie zadziałał i pozycja zostaje bez właściciela fazowego.** M7c dołożył `PolicyApplied` i `ManagerAssigned` (znów trzy ramiona na dwa warianty, bo reguła zapasowa jest innym zdaniem niż reguła numer 255) i funkcja ma **499** linii — ale **przebudowy `DecisionReason` nie zrobił**. Powód jest zmierzony, nie estetyczny: przebudowa dotyka **370 miejsc w dziesięciu crate'ach**, zmienia reprezentację w zapisie gry i pakowanie `PlanSlot.reason` do bajtu, a do żadnego z dwóch kryteriów M7c nie wnosi nic. Wpuszczona do commita podfazy zatopiłaby jego recenzję dokładnie tak, jak `cargo fmt` całego repo zatopiłby recenzję M7a. **Rozstrzygnięcie stoi:** podział `describe` nadal wymaga tamtej przebudowy i nadal jest właściwym rozwiązaniem; zmienia się wyłącznie to, kto ją niesie — **osobna zmiana, nie podfaza**, bo przebudowa kontraktu z 00 §K-12 nie jest pakietem roboczym żadnej fazy. Do tego czasu funkcja rośnie o ~20 linii na podfazę M7 i jest zamrożona w `struct_guard.py` na aktualnej wartości. **Po M7d: 566.** Blok finansowy (505–510) dołożył siedem ramion na sześć wariantów — `BankruptcyOpened` ma dwa, bo brak płynności mierzy się dobami („nie płaci od trzech miesięcy"), a ujemny kapitał nie („ma więcej długów niż majątku"), i to są dwa różne zdania o firmie. Prognoza „~20 linii na podfazę" okazała się zaniżona **dwukrotnie i konsekwentnie**: M7b +54, M7c +48, M7d +67. Powód jest ten sam za każdym razem i warto go zapisać, bo zmienia szacunek dla M8–M10: wariant z ładunkiem złożonym rzadko ma jedno ramię, a formatowanie argumentów zajmuje pięć linii na ramię, nie jedną. **Po M7e: 618.** Pięć wariantów AI firm (511–515), pięć ramion — pierwszy blok M7, w którym żaden wariant nie rozgałęził się na dwa klucze lokalizacji, i pierwszy, w którym przyrost (+52) trafił w prognozę. **Po M7f: 660** i blok M7 jest zamknięty (516–519: `SiteOpened`, `VoluntaryClosure`, `FirmFounded`, `ChainEntered`). Przyrost +42 jest najmniejszy w całej fazie i pokazuje, gdzie naprawdę leży koszt tej funkcji: `SiteOpened` niesie `Trend` o trzech wartościach i ma **jedno** ramię, bo kierunek wchodzi podstawieniem do zdania, a nie wyborem klucza. Ramię kosztuje pięć linii; **rozgałęzienie na drugi klucz kosztuje kolejne pięć** — i to ono, a nie liczba wariantów, decydowało o przyrostach M7b (+54) i M7d (+67). Adres podziału zostaje bez zmian: osobna zmiana przebudowująca `DecisionReason` na `Citizen \| Firm \| City`. **Po M8a: 730.** Blok M8 (600–606) dołożył siedem ramion na siedem wariantów — znów żadnego rozgałęzienia na drugi klucz, ale przyrost wyszedł **+70**, największy od M7d. Powód doprecyzowuje wcześniejszy wniosek: nie liczy się liczba ramion ani liczba kluczy, tylko **liczba podstawień w ramieniu**. Każdy powód podatkowy niesie trzy (danina, kwota, stawka albo przyczyna), a trzy argumenty to dziesięć linii formatowania, nie pięć. Gdyby siedem danin dostało po własnym zdaniu zamiast jednego zdania z podstawieniem `{danina}`, ta sama treść zajęłaby ~250 linii — i to jest miara tego, ile ten plik zawdzięcza podstawieniom **Po M8b: 754** (607–608, sieci przesyłowe, +24 — przyrost wraca do prognozy, bo rodzaj medium wchodzi podstawieniem `{medium}`, tak samo jak danina). **Po M8c: 780** (609–610, zdarzenia świata, +26 — sześć kategorii wchodzi jednym `{kategoria}`). **Po M8d: 855**, największy przyrost bloku M8 (+75) z dwóch powodów naraz: `ServiceQuality` ma **sześć** podstawień, bo jakość placówki bez rozbicia na czynniki jest liczbą bez odpowiedzi na „dlaczego tyle”, a `RemedyImposed` jest **pierwszym ramieniem bloku M8 rozgałęzionym na dwa klucze lokalizacji** — kara z kwotą i kara bez kwoty to dwa różne zdania. **Po M8e: 972 (+117) i po raz pierwszy przekroczony próg pliku (1329 > 1200).** Siedem wariantów władzy i wyborów (616–622), z czego **trzy rozgałęziają się na dwa klucze**: podwyżka stawki i obniżka, przetarg rozstrzygnięty i przetarg bez ofert, wybory utrzymujące burmistrza i wybory zmieniające władzę. To domyka regułę zbierającą całą historię tej pozycji: **koszt bierze się z rozgałęzień, nie z wariantów** — M7f (+42, zero rozgałęzień) i M8e (+117, trzy) są jej dwoma końcami. Przekroczenie progu pliku niczego nie zmienia w adresie podziału: plik ma **jeden temat** (powód → zdanie), więc rozcięcie go dałoby dwa pliki o jednym temacie. Rozcina go dopiero `K-58` z `R2e` — rozbicie `DecisionReason` na trzy enumy po aktorze — i to jest ten sam podział widziany z drugiej strony |
| 38 | `sim/economy/src/data.rs` | `fn EconomyData::load` — **269** linii (błąd od 250) | Siedmiokrotnie powtórzony ten sam rytuał (odczyt → `schema_version` → duplikaty → kompletność) zakończony literalem o ~20 polach. **Najtańszy szew z całej ósemki**: pięć z siedmiu bloków zwraca typ, który **już istnieje** (`RetailTable`, `(PricingParams, ShopCosts)`, `BudgetParams`, `BankParams`, `CpiSpec`), więc ich wyniesienie to przeniesienie bloku z sygnaturą `fn(dir: &Path) -> Result<T, EconomyDataError>`. Zostają dwa: `weights.ron` i `choice.ron`, który rozsypuje dziewięć skalarów wprost do literału — ten wymagałby typu pośredniego. Po wyniesieniu piątki `load` schodzi do ~110 | **M6a** (`M6a-katalog-i-partia.md` WP1): wprowadza `data/goods/`, `GoodId` alfabetyczny i `pack: Option<RetailPack>` opisany jako kontrakt z M5 — czyli moment, w którym `retail.ron` dostaje ósmy czytnik. Kolejność `goods` w `RetailTable` jest kontraktem danych (ranga substytutu), więc blok musi pojechać z zachowaniem kolejności wierszy **Sprostowanie po M6a:** wyzwalacz nie zadziałał i pozycja przechodzi do **M6d/WP11**. M6a nie dołożył `pack: Option<RetailPack>` do `Good` — pole nie ma w M6 ani jednego konsumenta i zostało przeniesione do WP11 razem z podłączeniem półki (`AD-4` w dokumencie fazy M6). `retail.ron` dostał w M6a wyłącznie przemianowane klucze, czyli ani jednego nowego czytnika, więc `load` ma te same 268 linii co przed fazą | **Sprostowanie po M6d:** wyzwalacz nie zadziałał **drugi raz** i pozycja przechodzi do **M6e**. WP11 zmienił, **gdzie leży towar**, a nie schemat danych: `retail.ron` nie dostał ani jednego nowego czytnika, `data.rs` nie jest w diffie tej podfazy, a `Good::pack` z `AD-4` okazał się niepotrzebny (`AL-10`). Dzielenie pliku, którego zmiana nie dotyka, w commicie o pięćdziesięciu plikach, jest dokładnie tym rodzajem przejazdu przy okazji, przez który commit przestaje dać się przeczytać — a `load` ma te same 268 linii co przed fazą. Adres na przyszłość bez zmian: pierwsza faza, która dopisze czytnik do `retail.ron` (M6e panel, M7 asortyment). **Sprostowanie po M7d: wyzwalacz nie zadziałał trzeci raz, a funkcja urosła o jedną linię (269).** M7d dopisał do `bank.ron` sekcję `finance` (leasing, faktoring, obligacja) i produkt inwestycyjny, ale to jest **nowe pole w istniejącym bloku**, a nie nowy czytnik: `BankParams` buduje się dokładnie tam, gdzie się budował, o jeden wiersz dłużej. Podział pliku, którego zmiana dotyczy w jednej linii, w commicie zamykającym podfazę, jest tym samym przejazdem przy okazji, który odrzuciło M6d
| 39 | `sim/economy/src/market/lifecycle.rs` | `impl Market` — 361 linii (ostrzeżenie od 300), w tym `fn open_shop` 192 (ostrzeżenie od 150) | **Urosło w M6d/WP11 i urosło we właściwym miejscu.** Otwarcie sklepu zakłada teraz dwa sloty magazynowe, rampę i zakład w `Plant` — to jest ta sama czynność co przedtem (postawienie sklepu), tylko wykonana do końca. Szew tematyczny **istnieje**: `open_shop` + `stock_initial` (założenie i zatowarowanie) kontra `record_capital` + `deliver_now` (pieniądz i dostawa z zewnątrz), i przecina plik mniej więcej na pół. Nie jest jednak mechaniczny: obie połowy sięgają do `ChainHandle` i do `Books`, więc podział w tym samym commicie co migracja znaczyłby dwie zmiany naraz w pliku, którego cały sens się właśnie zmienił | **M7a** (`M7a-firma-jako-dane.md`): zakładanie firmy przestaje być czynnością scenariusza i staje się decyzją AI, więc `open_shop` i tak dostaje drugiego wołającego z innymi wymaganiami — a wtedy podział ma po co się wydarzyć, zamiast być przestawieniem linii |
| 40 | `engine/core/src/vocab.rs` | plik **802** linie (ostrzeżenie od 800, błąd od 1200) | Słownik wspólny wszystkich faz i **z definicji rosnący**: każde wykonanie `K-8` dokłada tu enum. Po M7f przekroczył próg ostrzeżenia dopisaniem `Trend` (`K-52`). Podział jest oczywisty tematycznie — słowniki agenta, gospodarki, terenu i czasu — ale kosztuje **przenumerowanie nazw w `use` w dziesięciu crate'ach** i nic nie kupuje, dopóki plik mieści się w progu błędu. Ścieżka wyjścia: `vocab/` jako katalog z re-eksportem w `mod.rs`, czyli zmiana bez zmiany ani jednej nazwy publicznej | **M8** — pierwsza faza, która dopisze tu więcej niż jeden słownik naraz (podatki, prawo pracy, usługi miejskie), i pierwsza, przy której próg błędu przestanie być odległy |
| 41 | `tools/magnat/src/citizens.rs` | blok `impl Citizens` — **448** linii (ostrzeżenie od 300; 450 przed M9a, 405 po M9b) | Po przeniesieniu stawiania świata do `game::Session` (`K-68`) zostaje tu jeden temat: spięcie sesji z oknem — wejście `egui`, klatka UI, warstwa Mikro dla renderera i dwa panele. Wszystkie metody sięgają `self.session` i `self.ui`, więc podział wypchnąłby obie przez sygnaturę. Blok **zmalał** w tym commicie i zmaleje dalej, gdy `M9b` przeniesie panele do `game::panels` | **Sprostowanie po M9c: prognoza się nie sprawdziła i warto wiedzieć dlaczego.** `M9b` miał wyprowadzić panele, a `M9c` zamiast tego **zastąpił dwa panele jednym** (karta inspekcji zamiast karty mieszkańca i karty sklepu) i dołożył do klienta nawigację po kartach, filtr encji i legendę nakładki. Netto +43 linie: treść kart przeniosła się do `game::inspect`, ale ich **obsługa** została tutaj i tu zostanie. Adres podziału zmienia się na `M9e`: panele biznesowe dostaną własny rejestr (`PanelRegistry`), a wtedy z tego bloku wychodzi „wejście i warstwa Mikro" osobno od „panele i karty" — czyli wreszcie dwa tematy |
| 42 | `sim/economy/src/market/api.rs` | blok `impl Market` — **498** linii (ostrzeżenie od 300, **dwie linie od progu błędu 500**; 449 przed M9a, 459 po M9a) | Trzydzieści kilka odczytów i trzy zapisy nad jednym zamkiem rynku — to jest szew (g) z tabeli „zmiany po M5e": odczyty dla prezentacji i pomiaru. M9a dołożył jeden odczyt (`good_key`, 4 linie), bo komenda gracza niesie **klucz towaru, nie indeks** (00 §5). Podział wzdłuż „odczyt / zapis" jest naturalny, ale przestawia sąsiedztwo metod, które panel woła parami | **M9c dołożył dwa odczyty** (`lost_sales_of_citizen` i `is_customer`, razem ~39 linii): karta mieszkańca pyta o utracone sprzedaże **jego**, a filtr encji o to, czy kupił u gracza. Próg błędu 500 padnie przy pierwszym następnym odczycie, więc **podział jest teraz zaplanowany na `M9e`, a nie „kiedyś"**: panele biznesowe dołożą ich kilkanaście, a szew („odczyt dla prezentacji" → `api/readout.rs`) jest ten sam, który był naturalny już w R1 |
| 43 | `engine/core/src/decision.rs` | plik **814** linii (ostrzeżenie od 800) | To jest jedna lista: centralny `DecisionReason` ze wszystkimi wariantami wszystkich faz plus jawny `discriminant`. Podział pliku rozbiłby listę, której jedyną wartością jest to, że **widać ją w całości** — numer wolny znajduje się wzrokiem, a nie wyszukiwarką po trzech plikach. Wartość wyszła na jaw przy `cargo fmt --all` w M9a (formatowanie skróciło plik, kontrola strukturalna patrzy na pliki zmienione); przed formatowaniem plik był dłuższy i nigdy nie był zarejestrowany | `R2e` dzieli `DecisionReason` na trzy enumy po aktorze (`Citizen` \| `Firm` \| `City`, `K-58`, rejestr poz. 37). Wtedy podział pliku wychodzi za darmo i wzdłuż tematu, a nie wzdłuż numeru linii |
| 41 ✅ | `tools/balansator/src/gates.rs` | `fn evaluate` — **257** linii (błąd od 250) | **Podzielone od razu, bo podział okazał się mechaniczny.** Prognoza z tej samej pozycji sprzed godziny („dwanaście linii od progu, następna bramka go przekroczy") sprawdziła się w tym samym commicie: dopisanie ogona do G11 przesunęło funkcję na 257. Szew nie był estetyczny, tylko **tematyczny**: `detal` (G1–G6) pyta o ceny i rynek, `rzetelnosc` (G7–G9) o to, czy przebieg w ogóle wolno czytać — czerwień którejkolwiek z tych trzech unieważnia pozostałe bramki — a `firmy` (G10–G11) o populację firm i rynek pracy. `evaluate` zostaje składaniem trzech list i ma dziewięć linii. Zero stanu dzielonego między blokami, więc przeniesienie było przeniesieniem symboli bez zmiany zachowania: ten sam zestaw run-files daje tę samą tabelę | **M7f** |

---

## Zmiany wpisane po R1

Zgodnie z `K-18`. Tabela wypełnia się w trakcie wykonywania tego dokumentu — poprawki do planów
faz M6–M12, które wyjdą przy podziale plików (np. moduł, który M6 miał przejąć, okaże się leżeć
gdzie indziej, niż zakłada `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md`).

| # | Zmiana | Dlaczego |
|---|---|---|
| D-1 ★ | **Dochodzą dwa pakiety: R-WP11 (`sim/agents/src/planner.rs`, 1 772 linie kodu) i R-WP12 (`sim/economy/src/market.rs`, 3 228).** Oba przekraczają próg błędu i żaden nie miał wiersza w §4. R-WP10 zależy teraz także od nich, bo prostuje importy po wszystkich podziałach | Wpisane przy zamknięciu R-WP1, znalezione pierwszym przebiegiem `scripts/struct_guard.py --all` — czyli dokładnie przez narzędzie, które R-WP1 miał dostarczyć. Bez tych dwóch wierszy R1 **nie mógł spełnić własnego kryterium akceptacji nr 4** („żaden plik produkcyjny powyżej 1200 linii poza listą wyjątków z §5"), bo dwa pliki powyżej progu nie należały do żadnego pakietu. `market.rs` był zresztą zapowiedziany w trzech tabelach „Zmiany wpisane po M5c/M5d/M5e" jako „jedenasty plik" — zabrakło tylko przełożenia tego na §4; `planner.rs` nie był wymieniony nigdzie |
| D-2 | **Hook jest typu `PreToolUse` na `Bash` i wchodzi tuż przed `git commit`, a nie typu `Stop`**, jak mówi §6 wariant B. Logika siedzi w `scripts/struct_guard.py --hook` (czyta zdarzenie z wejścia, oddaje `additionalContext`), a `.claude/settings.json` ma jedną linię polecenia | Ten sam paragraf §6 wymaga, żeby kontrola działała **„przed commitem, nie po"**. Hook typu `Stop` odpala się po zakończeniu tury agenta, czyli po commicie — sprzeczność wewnątrz jednej sekcji. `PreToolUse` trafia w moment dosłownie opisany w regule i nie potrzebuje zabezpieczenia przed pętlą (`stop_hook_active`). Hook **nie oddaje `permissionDecision`**, więc zwykła zgoda na `git commit` przebiega bez zmian — informuje, nie blokuje, zgodnie z „czego kontrola nie robi" |
| D-3 | **`market.rs` ma 3 228 linii, nie „~3 000", i trzy bloki `impl` powyżej 500** (860, 821, 510), a nie jeden. Szew (f) rozpada się na dwa: `service_working_capital` i `maybe_borrow_working_capital` sięgają `self.shops` czternaście razy i `self.budgets` ani razu — to kapitał obrotowy **firmy**, wołany z `close_month`, a nie miesiąc gospodarstwa. Dochodzi ósmy szew (h): rozliczenie transakcji (`take_intents`, `return_goods`, `record_sale`, ~98 linii), nierozdzielne od (e). `close_month` (73 linie) nie należał do żadnego szwu i idzie z (f′) | Wpis po M5d nazywał (f) „najczystszym szwem z całej szóstki" i to było nieprawdą — pomiar odwołań pokazał, że połowa (f) dotyka sklepów. Teza o (g) jako jedynym szwie bez zapisu **potwierdza się**: `balance_sample` i `shop_panel` biorą `self.lock()` bez `mut`, a `hash_state` haszuje wyłącznie pola, których one nie ruszają |
| D-4 | **`MarketInner` nie potrzebuje `pub(crate)` na polach**, wbrew ograniczeniu z wpisu po M5c. Warunkiem jest katalog `market/` (moduły potomne), nie moduły siostrzane `market_*.rs` | Rust udostępnia elementy prywatne modułowi definiującemu **i wszystkim jego potomkom**, więc `use super::*` wystarcza. Ograniczenie z M5c dotyczyło wariantu siostrzanego, który z tego właśnie powodu odpada |
| D-5 | **Kolumna „dziś" w tabeli progów §6 jest nieaktualna** (mierzona przy pisaniu dokumentu, w trakcie M5c). Pomiar `struct_guard.py --all` na `master` po M5e: 206 plików produkcyjnych, plik 15 ostrzeżeń / 10 błędów (+3 wyjątki), `impl` 7 / 8, funkcja 22 / 12, `mod.rs` 0 / 2 | Nie jest to korekta progów — `D-R1` zostaje bez zmian. Liczby urosły, bo M5d i M5e dopisały ~1 700 linii do `market.rs`, a to jest właśnie zjawisko, które reguła z §6 ma łapać. Kolumna zostaje w dokumencie jako zapis stanu z chwili pisania; stan bieżący daje skrypt |
| D-8 ★ | **Zdanie z §4 R-WP3 „pola trzeba podnieść do `pub(crate)`" jest nieprawdziwe** przy wariancie katalogowym, czyli tym, który R-WP3 sam zaleca. Pola `Renderer` **nie zostały podniesione** i nie musiały. Podniesienia poszły w drugą stronę: 37 symboli, które wyszły z `renderer`, dostało `pub(super)` — żaden `pub` ani `pub(crate)` | Ten sam mechanizm co `D-4` dla `market.rs`, potwierdzony niezależnie: moduł udostępnia swoje prywatne elementy **wszystkim potomkom**, więc `impl Renderer` w `renderer/frame.rs` czyta `self.chunks` bez żadnej zmiany widoczności. Uogólnienie wiążące dla R-WP4…R-WP12: **podział do katalogu nie wymaga otwierania pól, podział na moduły siostrzane wymaga** — i to jest argument za katalogiem, a nie kwestia gustu |
| D-9 | **Szew w `pipelines.rs` nie biegnie „po jednym pipelinie z jego layoutem", jak zakłada tabela §4 R-WP3.** W dawnym `new` **wszystkie osiem układów grup wiązań powstaje przed pierwszym potokiem**, a układy potoków jeszcze później; funkcja „potok razem ze swoim layoutem" musiałaby przepleść `create_bind_group_layout` z `create_*_pipeline`. Zamiast tego jest dziesięć funkcji, z których każda jest **ciągłym wycinkiem dawnego ciała `new`**, wołanym w dawnej kolejności. Osobno: `render`, `render_with_ui`, `render_to_image`, `render_to_image_with_ui` idą do `passes.rs`, a nie do `renderer.rs`, bo inaczej `impl Renderer` w `renderer.rs` ma ~556 linii, czyli powyżej progu błędu z §7 pkt 5 | Tabela §4 opisywała szew tematyczny, a plik ma szew **kolejnościowy** — i to kolejność jest tu kontraktem sterownika, nie temat. Zachowany jest duch tabeli (`pipelines.rs` nie dotyka `self`, jest czystą kompozycją `&Device` → obiekty GPU), nie jej litera. Wpisane, bo ta sama pułapka czeka M11, gdy dołoży własne potoki |
| D-29 ★ | **Scenariusze `tools/headless` przestają być mierzone — to poprawka w liście wykluczeń `struct_guard.py`, czyli w dostawie R-WP1.** Skrypt pomijał `tests/`, `benches/` i `target/`, a powinien pomijać także moduły binarki scenariuszy. Granicę rysuje sam crate: `tools/headless/src/lib.rs` wystawia **dokładnie to, co ma więcej niż jednego konsumenta** (`population` 287 linii, `retail` 256 — oba poniżej każdego progu), a scenariusze zostają modułami binarki. Lista mierzonych plików powstaje z **odczytu `lib.rs`**, nie z wypisania, więc nie może się zestarzeć. Efekt: dwa przekroczenia progu błędu (`m5shop::run` 362, `worldgen::preview_city` 262) i ostrzeżenie `m3day.rs` 893 znikają | Scenariusze **są bramkami CI**, tylko z CLI zamiast `#[test]` — `ci.yml` uruchamia `m3day` i `m5shop` z `--out`/`--expect` i porównuje ciągi hashy. Obowiązuje więc ten sam powód, którym §2 zwolniło `tests/`: w `m5shop::run` kolejność wydruku **jest** raportem, a abstrakcja nad nią pogarsza jedyną własność, jaką ten kod ma. **Nie dotyczy `tools/magnat`** (klient graficzny to kod produktu — dzielił go R-WP2) **ani `tools/balansator`**. Wykluczenie ma własną bramkę w `--self-test`, bo regres w filtrze byłby **cichy**: skrypt świeciłby na zielono na mniejszej liczbie plików |
| D-30 | **`D-13` było w jednej trzeciej nieprawdziwe.** Z trzynastu symboli, o których twierdziło, że nie mają konsumenta poza `sites/`, dziesięć naprawdę go nie miało i zostało zwężone; trzy — `ArchetypeSpec`, `ChainTemplate`, `FirmNames` — mają i zostają `pub`. Są typami **pól publicznych** (`Archetype::spec`, `SiteCatalog::chains`, `SiteCatalog::names`), czytanymi przez `population/table.rs`, `population/catalog.rs` i `traffic_build.rs`; kompilator odrzucił zwężenie trzema błędami `type is private` i lintem `private_interfaces` | `D-13` policzył trafienia grepem **po nazwie typu**, a te trzy podróżują przez **pole, nie przez ścieżkę**. Reguła na przyszłość: przed zwężeniem widoczności typu sprawdź, czy nie jest typem pola publicznego — grep po nazwie tego nie zobaczy, a kompilator zobaczy dopiero przy próbie |
| D-31 ★ | **Kryterium akceptacji nr 5 dostaje klauzulę wyjątków, symetryczną do nr 4.** Nr 4 od początku brzmiało *żaden plik produkcyjny powyżej 1200 linii **poza listą wyjątków z §5***, a nr 5 — *żaden blok `impl` powyżej 500 i żadna funkcja powyżej 250* — klauzuli nie miało. Po R1 zostaje **osiem przekroczeń**, wszystkie w plikach, których **żaden pakiet R1 nie miał na liście**: `lsystem.rs` (jeden `impl` i trzy funkcje), `zoning.rs`, `inspect/reason.rs`, `economy/data.rs` oraz dwa znane z rejestru (`generate_city`, `candidates`). Każde ma teraz wiersz w rejestrze długu z nazwanym sufitem i fazą-właścicielem | Asymetria między nr 4 a nr 5 była przeoczeniem redakcyjnym, nie zasadą: R1 **mierzył ogon plików**, a nie ogon funkcji — §1 wypisuje osiem plików powyżej 1200 linii i buduje wokół nich pakiety, nigdzie nie licząc, ile funkcji przekracza 250. Większość tych ośmiu to tabele `match`, wydruki albo pętle o wspólnym stanie, których podział jest **zmianą kształtu, nie przeniesieniem bloku** — czyli jawnie poza §2. Decyzja właściciela produktu, podjęta przy domknięciu R1 |
| D-32 | **Zwężenia widoczności są zmianą publicznego API skrzyń i kryterium nr 7 wymaga ich wypisania.** `sim/world`: `SCALE_BASE`, `SCALE_MIN`, `SCALE_MAX`, `RATIO_MIN`, `RATIO_MAX`, `firm_id`, `niemieszkalna` → `pub(super)`; `ARCHETYPE_SCHEMA_VERSION`, `CHAIN_SCHEMA_VERSION`, `FIRM_NAMES_SCHEMA_VERSION`, `GESTOSC_MIN`, `GESTOSC_MAX` → prywatne. Dodatkowo przestały być re-eksportowane (deklaracji nie zmieniono): `capacity::{ETATY_SCALE_MIN, ETATY_SCALE_MAX, recompute_pop_capacity, rescale_dwellings}`, `footprint::{footprint_for, rozpietosc}`, `model::{wage_band, JOBS_SCHEMA_VERSION}`, `sites::place::populate` | żaden z tych symboli nie miał konsumenta poza swoją skrzynią — sprawdzone grepem po całym workspace **łącznie z `tests/`, `benches/` i `tools/`**, potwierdzone przez `cargo build --workspace --all-targets`. To jest wykonanie `D-R3` w duchu: ścieżka importu ma mówić prawdę o tym, gdzie kod leży, a `pub` bez konsumenta kłamie o zasięgu |
| D-33 ★ | **Wejście do decyzji o `R2`: został dokładnie jeden plik, który wygląda jak to, co R1 dzielił.** `sim/world/src/city/lsystem.rs` — blok `impl` na 605 linii i trzy funkcje między 257 a 317, przy 1154 liniach pliku. Pozostałe siedem przekroczeń to pojedyncze funkcje w plikach zdrowej długości, każda z nazwaną fazą-właścicielem. Nagłówek tego dokumentu mówi: *jeśli reguła z §6 pokaże, że `R2` jest potrzebny — powstanie; jeśli nie pokaże, `R2` nie powstanie i to będzie dowód, że reguła działa* | Odpowiedź po R1 brzmi: **na jeden plik nie opłaca się dokument**. `lsystem.rs` (pozycja 33) dzieli się przeniesieniem bloku, za darmo i bez `pub(crate)` — to jest praca na jeden commit, nie na fazę. Reguła z §6 działa od teraz: hook zapali się przy pierwszym pakiecie M6, który dopisze do pliku nad progiem, i wtedy decyzja będzie podejmowana z pomiarem w ręku, a nie z pamięci |
| D-25 ★ | **Rodzic to `market.rs` obok katalogu, nie `market/mod.rs`, a kryterium *poniżej 500 linii* z §4 R-WP12 było nieosiągalne przy przydziale treści z tej samej tabeli.** Nagłówek modułu (39) + blok `use` (43) + definicje typów (216) + `impl HashState` (48) to 346 linii, zanim dojdzie `new` i piątka metod stanu; wyszło 571. `D-14` rozstrzyga pisownię: `mod.rs` dla rodzica, który tylko deklaruje — a ten trzyma `MarketInner` z trzydziestoma polami i jest właścicielem stanu w najczystszej postaci w tym repo | Trzeci raz ten sam wzorzec po `renderer.rs` (R-WP3) i `oracle.rs` (R-WP6), i za każdym razem z tego samego powodu: **tabela §4 rozdziela zachowanie, a nie stan**, więc rodzicowi zostaje to, czego tabela nie wymienia. Przy `foo.rs` + `foo/` metryka `mod.rs` nie ma zastosowania, a metryka pliku (571 przy progu 800) zostawia M6 realny zapas |
| D-26 | **Osiem szwów (a)–(h) z `D-3` zgadza się z kodem co do metody — w tym rozstrzygnięcie, że (f) rozpada się na dwa.** `service_working_capital` i `maybe_borrow_working_capital` to metody `MarketInner` wołane z `close_month`, więc wyszły razem z nim do `market/close.rs`, a `market/household.rs` nie ma po nich śladu. Szew (h) okazał się nierozdzielny od (e), jak zapowiadał `D-3`: `take_intents`, `return_goods` i `record_sale` dzielą z `fulfil` pole `intents` i dostęp do półki | `D-3` powstało z pomiaru odwołań (`self.shops` czternaście razy w rzekomym miesiącu gospodarstwa), a nie z czytania nazw — i to jest jedyny wpis korygujący plan, który po wykonaniu nie wymagał **żadnej** dalszej poprawki. Metoda warta powtórzenia: przy wątpliwym szwie policzyć odwołania do pól, zanim się uwierzy nazwie metody |
| D-27 | **Diagnoza z §1 o czterech najdłuższych plikach bez testów jednostkowych okazała się trafna po raz pierwszy — przy piątym sprawdzeniu.** `market.rs` naprawdę ma zero bloków `#[cfg(test)]`. Wcześniej: arena renderera miała dwa (`D-10`), obrys sześć (`D-18`), planer własny plus osiemnaście integracyjnych (`D-23`), `city/mod.rs` naprawdę zero. Pokrycie `market.rs` stoi w `sim/economy/tests/` — 50 testów integracyjnych w siedmiu plikach | Bilans po całym R1: z pięciu plików, o których §1 twierdziło, że nie mają testów, **dwa naprawdę ich nie miały**. To nie unieważnia §1 jako diagnozy struktury — unieważnia wnioskowanie o pokryciu z długości pliku. Wniosek dla przyszłych dokumentów: liczbę testów mierzy się grepem, nie intuicją |
| D-28 | **Dwa testy `sim/economy` chodzą po `src/` rekurencyjnie i objęły nowy katalog `market/` bez żadnej poprawki** — `single_entry_point.rs` (M5a: jedyne przypisanie do salda ma być w `books.rs`) i `budget_credit_cpi.rs`. To odwrotność `D-24`, gdzie `contract.rs` czytał `src/planner.rs` po ścieżce i wymagał wzmocnienia | Różnica jest jedną linią przy pisaniu testu (`if p.is_dir() { walk(&p, out) }`) i decyduje o tym, czy bramka przeżyje pierwszy podział pliku, który obserwuje. Warte zapisania jako wzorzec: **test chodzący po źródłach ma chodzić po katalogu, nie po liście plików** |
| D-23 | **Zdania §4 „trzeba podnieść X do `pub(crate)`" przeszacowują — trzeci raz z rzędu.** §4 R-WP11 wymieniało 22 kandydatów na `pub(super)`; realnie potrzebnych było **17**, a pięć odpadło: `Log` z czterema metodami i `PlanCtx::rng_for` to prywatne elementy przodka, które potomkowie widzą za darmo (`D-8`), a `uloz` i `minuty` w ogóle nie przecinają granicy pliku. Wcześniej to samo w R-WP3 (`D-8`: pola `Renderer` w ogóle nie musiały) i w planie R-WP12 (`D-4`: `MarketInner` nie potrzebuje `pub(crate)`) | Wzorzec jest stały i wynika z jednej pomyłki w modelu mentalnym: przy wariancie katalogowym **widoczność podnosi się tylko dla ruchu poprzecznego** (rodzeństwo → rodzeństwo, dziecko → rodzic), nigdy dla ruchu w dół. Podnoszenie „z listy" zamiast „z komunikatu kompilatora" otwiera symbole, które nie muszą być otwarte — a każdy z nich to powierzchnia, którą R-WP10 musiałby potem zwężać |
| D-24 | **`sim/agents/tests/contract.rs` czytał `src/planner.rs` po ścieżce i musiał zostać poprawiony.** Skanuje teraz cały katalog `src/planner/` przez `read_dir` + `sort`, zamiast listy plików. To **wzmocnienie testu, nie osłabienie**: plik dopisany do `planner/` w przyszłej fazie wchodzi pod regułę „kontrakt nie wspomina o gospodarce ani o grafie" bez dopisywania go tam ręcznie. Zakazane listy i sama reguła nietknięte | Jedyny przypadek w całym R1, w którym podział wymusił zmianę **w pliku testowym**. Wpisane, bo pozostałe pakiety mogą trafić na to samo: test, który czyta źródło po ścieżce, jest zależnością od układu plików ukrytą przed kompilatorem. Grep po `include_str!` i `read_to_string` z `CARGO_MANIFEST_DIR` przed podziałem kosztuje sekundę i oszczędza czerwony przebieg |
| D-20 ★ | **Tabela §4 R-WP9 wymienia cztery pliki, powstało pięć — i `GenerationReport::lines` rozcięte, czego tabela nie przewiduje.** `city/finalize.rs` musi istnieć, bo `finalize` to 182 linie algorytmu (przycinanie do punktu stałego, największa składowa, kompaktowanie, CSR, bramy), a nie orkiestracji: zostawienie go w `mod.rs` trzymało metrykę `mod.rs` na 758 liniach (próg błędu 600) i **łamało zasadę, którą R-WP9 sam formułuje** — „`mod.rs` deklaruje i orkiestruje, nie implementuje". `lines` (283) rozcięte na szwie, który już miało (`if self.parcels > 0 \|\| self.districts > 0`), na `lines` 72 + `lines_m2` 214 | **§7 jest nadrzędne wobec §4** — precedens `D-11`. Bez obu ruchów pakiet zostawiał cztery przekroczenia progu błędu zamiast jednego. Efekt: `mod.rs` 1410 → 575 linii pliku i 534 poza deklaracjami, czyli z ciasnego marginesu czterech linii przed progiem ostrzeżenia robi się **225 linii zapasu przed M6** |
| D-21 | **`D-12` dotyczy podziału pliku-modułu na podkatalog (`sites.rs` → `sites/`), a nie podziału istniejącego `mod.rs` na rodzeństwo.** R-WP9 nie schodzi o poziom: `report.rs`, `hash.rs`, `checks.rs` i `finalize.rs` leżą w `city/` jako rodzeństwo `build.rs` i `road.rs`, więc `super` nadal znaczy `city`. **Zero przeliterowanych ścieżek.** Blok `use` rodzica też pozostał nietknięty — rustc liczy import rodzica jako używany, gdy dziecko sięga po niego przez `use super::*` | Drugi przypadek jest tańszy o całą kategorię zmian i o cały churn importowy. Przy wyborze kształtu podziału warto to ważyć: rodzeństwo tam, gdzie dzielony jest `mod.rs`; podkatalog tam, gdzie dzielony jest plik-moduł |
| D-22 | **Trzecia awaria `city_m2d` ma adres i też wskazuje na test** (uzupełnienie `D-19`). `rozbicie_wyceny_sumuje_sie_do_wyniku` buduje własny `ValueCtx` z **`access: None`**, czyli semantyką `pass_1`, i porównuje wynik z zapisanym `land_value_per_m2`. Ale `value.rs:473` — `pass_2` — nadpisuje to pole dla **wszystkich** parcel z `access: Some(&access)` i ma na to `debug_assert!`, a `generate_city` woła `pass_2` po Etapie 7 i nazywa to komentarzem dwadzieścia linii wyżej. **Pierwszy assert testu, ten od jego nazwy, przechodzi**; pada drugi, bo porównuje wyliczenie `pass_1` z zapisem `pass_2`. Test pochodzi z M2d, sprzed istnienia `pass_2` | `D-R7` ma komplet trzech adresów dla `city_m2d`: dwa z `D-19`, trzeci tutaj. **Dwa z trzech to przeterminowane testy M2d**, nie zepsuty kod; trzeci (`zaden_budynek_nie_wisi_nad_terenem`) zostaje hipotezą wskazującą na `derive.rs`. Naprawa tego jest jednym tokenem w pliku testowym (`access: Some(&c.access)`) i nie rusza hasha — ale rozstrzygnięcie „pada test czy pada kod" należy do `D-R7`, nie do R1 §2 |
| D-17 | **`build/footprint.rs` nie jest „czystą geometrią 2D bez świata", wbrew zdaniu z §4 R-WP8.** Ta sama tabela przypisuje temu plikowi `bryla(p: &Planned)` i `wejscia(input: &BuildInput, parcels: &ParcelSet, …)`, które biorą świat wprost. Czysta geometria to sześć z dziewięciu funkcji pliku: `footprint_for`, `zapas`, `rozpietosc`, `miesci_sie`, `rogi`, `wyjscie_z_bryly` — i dokładnie na nich stoją testy. Wykonana została tabela, nie zdanie | Zdanie obiecywało więcej, niż tabela dawała, i gdyby ktoś je wykonał dosłownie, `bryla` i `wejscia` musiałyby wyjść do trzeciego pliku albo zmienić sygnaturę — czyli zatrzymać pakiet (§3). Wpisane, bo R-WP12 i R-WP11 mają w §4 podobne zdania opisowe obok tabel, a **tabela jest wiążąca** |
| D-18 ★ | **Obietnica „testu, którego dziś nie ma" okazała się nieprawdziwa po raz drugi.** §4 R-WP8 nazywa `footprint.rs` „pierwszym kandydatem na test jednostkowy, którego dziś nie ma" — było ich **sześć**, w module `#[cfg(test)]` na końcu `build.rs`, i wszystkie przeniosły się razem z kodem. Naprawdę brakowało testu **`wyjscie_z_bryly`** — jedynej funkcji geometrycznej w pliku, której doc-komentarz nazywa przebyty błąd (`half.y` niezależnie od kierunku: dla hali 200 × 40 m drzwi kilkadziesiąt metrów wewnątrz bryły) i której nic nie pokrywało. Dopisany test obchodzi halę 72 kierunkami co 5° i sprawdza, że punkt trafia w **lico**, nie w okrąg; przywrócenie dawnej linii go wywraca | Po `D-10` (arena renderera) to drugi przypadek tej samej pomyłki w §4, a `D-10` wprowadziło regułę „sprawdź, zanim uwierzysz" właśnie po to. Reguła zadziałała. Wniosek ogólniejszy: diagnoza z §1 („cztery najdłuższe pliki mają zero albo prawie zero testów") jest trafna co do kierunku i **niesprawdzona co do pliku** — do R-WP11 i R-WP12 należy podchodzić z tym samym założeniem |
| D-19 | **Diagnoza dwóch z trzech awarii `city_m2d` (`D-R7`) — obie wskazują poza `build.rs`.** (1) `park_i_kopalnia_zostaja_bez_zabudowy`: strażnik strefy w `plan_building_inner` brzmi `wymus.is_none() && matches!(… Green \| Extraction)`, a jego komentarz nazywa powód — **korekta F2 z M2e** pozwala Etapowi 7 wskazać gramatykę imiennie (pawilon w parku, nadszybie na kopalni). Test pochodzi z M2d, iteruje po **wszystkich** budynkach łącznie z Etapem 7 i sprawdza regułę, którą M2e świadomie zniosło — **nieaktualny jest test, nie kod**. (2) `zaden_budynek_nie_wisi_nad_terenem`: `base_z_m` liczy się w jednym miejscu, a `level_strip` wypełnia od `lo − 1 m`, czyli zawsze poniżej najniższego terenu pod obrysem, na pasie szerszym o pół metra — **z niwelacji dziura wyjść nie może**. Może z części `carve: true` kolejkowanej po podsypce: `SRC_BUILDING_VOID` to `EditSource(31)` przeciw `EditSource(20)` podsypki, więc void wygrywa wszędzie, gdzie się z nią przecina. Kandydat: `Rule::Void`, którego `scope` sięga poniżej `base_z_m` — **szukać w `derive.rs`** | Jedyna wartość, jaką R1 mógł wnieść do `D-R7`: nie naprawę (to zmiana zachowania, §2), tylko adres. `D-R7` przestaje brzmieć „coś pada" i staje się dwiema hipotezami, z których każda ma plik i linię. Informacja o głębokości fundamentu jest już w `Planned` (`bryla` liczy `aabb` od `base_z_m − foundation_depth_m`), więc dolne obcięcie pryzmy karwującej byłoby jedną linią — ale zmienia hash, więc należy do fazy, która to rozstrzygnie |
| D-16 ★ | **Piątka relacyjna (`powiaz`, `wpisz_relacje`, `odwrotna`, `relations_ref`, `knowledge_ref`) NIE przenosi się do `social.rs`,** wbrew propozycji z §4 R-WP7. Sprawdzenie kierunku zależności, którego §4 słusznie zażądał, wypadło **jednostronnie, ale w stronę odwrotną do przeniesienia**: `social.rs` sięga do `demography` w 20 miejscach, `demography` do `social.rs` w zerze — a mimo to całą piątkę woła sama demografia (`uroda`, `smierc`, `usun_relacje`, `zwolnij_slaby`, `dobierz_partnerow`, `sluby`), a poza nią `migration.rs` i **`sim/world/src/population/seeding.rs`**. Zostają w `demography/mod.rs`, nie w osobnym `relations.rs` | Przeniesienie zamieniłoby zależność jednostronną na dwustronną i **zmieniłoby ścieżkę importu poza skrzynią** (`sim/world` woła `demography::powiaz`), czego R1 §2 zabrania. Osobny plik byłby gorszy od obu wariantów: jedna krawędź zamieniona na pięć, a `mod.rs` i tak nad progiem. Warunek, pod którym §4 miało rację, jest nazwany w pozycji 21 rejestru długu — dopiero gdy narodziny i zgon przestaną same pisać relacje rodzinne |
| D-14 | **Rodzic zostaje plikiem obok katalogu (`foo.rs` + `foo/`), a nie `foo/mod.rs`, gdy jego własny kod przekracza 600 linii.** W R-WP6 `oracle/mod.rs` miałby ~750 linii kodu poza deklaracjami, czyli **nad progiem błędu metryki `mod.rs`**; zejście poniżej wymagałoby trzeciego, sztucznego podziału ściany getterów, która nie ma drugiego tematu. Przy pisowni `oracle.rs` + `oracle/` metryka `mod.rs` nie ma zastosowania, a metryka pliku to 747 (czysto). Precedens: R-WP3 (`renderer.rs` obok `renderer/`). Ten sam wybór dla `trip.rs` + `trip/`, dla spójności w skrzyni | To nie jest obejście progu, tylko konsekwencja zasady z §4 R-WP9: **„`mod.rs` deklaruje i orkiestruje, nie implementuje"**. Plik, który trzyma stan i jego akcesory, nie jest orkiestratorem — i nie powinien nazywać się `mod.rs`. Reguła dla pozostałych pakietów: `mod.rs` wtedy, gdy rodzic naprawdę tylko deklaruje i re-eksportuje (R-WP4, R-WP5); `foo.rs` + `foo/` wtedy, gdy rodzic zostaje właścicielem stanu (R-WP3, R-WP6) |
| D-15 | **`oracle/trip.rs` z tabeli §4 nazywa się `oracle/journey.rs`.** `crate::trip` jest w tej skrzyni zajęte przez sieć podróży w toku — czyli przez część B tego samego pakietu — a `crate::oracle::trip` obok niej to pomyłka nie tylko do przeczytania, ale i do popełnienia, bo `oracle.rs` ma `use crate::trip::{PendingTrip, …}`, a dziecko `use super::*`. Osobno: **`trip.rs` ma dwa tematy, nie jeden**, wbrew „podział analogiczny, ustalić przy pakiecie" z §4 — krok minutowy jest jednym ciągiem i się go nie dzieli, ale kolejka oczekujących na krawędź (`enqueue_wait`, `wake_one`, `unjam`) to mechanizm **krawędzi**, nie podróży: własny niezmiennik FIFO, własna lista jednokierunkowa w `ActiveTrip.wait_next` i jedyne miejsce, w którym obłożenie wolno przekroczyć pojemność | Trzeci raz ten sam wzorzec, po `planner/rhythm.rs` (R-WP11) i `market/*` (R-WP12): **nazwa modułu potomnego nie może kolidować z modułem najwyższego poziomu tej samej skrzyni**, bo obie ścieżki są wtedy poprawne i obie wyglądają tak samo. Wycięcie kolejki nie było estetyką: bez niego `impl TrafficNetwork` w `minute.rs` ma 506 linii, czyli **nad twardym progiem 500 z §7 pkt 5** |
| D-12 | **Podział na katalog przeliterowuje ścieżki `super::` — to jest jedyny koszt zejścia o poziom.** W R-WP5 dotyczyło to czterech linii: po zejściu do `city/sites/` `super` znaczy `sites`, nie `city`, więc `super::blocks::block_adjacency`, `super::parcels::parcel_id`, `super::grammar::zone_z_klucza` i `&super::road::RoadNetwork` musiały zostać zapisane jako `crate::city::…`. Wariant `super::super::` odrzucony jako nieczytelny | Reguła dla pozostałych pakietów: **policz odwołania `super::` przed podziałem** — to jedyna kategoria zmian, która wychodzi poza „przeniesienie bloku", i jedyna, którą porównanie wielozbiorów pokaże jako różnicę. Kompilator sprawdza je wszystkie naraz, więc ryzyko jest zerowe, ale w diffie trzeba je umieć nazwać |
| D-13 | **Dziewięć symboli `pub` w `sites/` nie ma ani jednego konsumenta poza `sites/`** (`SCALE_BASE`, `SCALE_MIN/MAX`, `RATIO_MIN/MAX`, trzy `*_SCHEMA_VERSION`, `ChainTemplate`, `FirmNames`, `ArchetypeSpec`, `firm_id`, `niemieszkalna` — sprawdzone grepem po całym workspace). Zostały `pub`, bo kryterium akceptacji nr 7 mówi „publiczne API niezmienione" | Zwężenie do `pub(super)` należy do **R-WP10**, razem z prostowaniem importów: to ta sama operacja (ścieżka ma mówić prawdę o tym, gdzie kod leży) i ten sam argument (`D-R3`). Wpisane, żeby R-WP10 wiedział, że ma tu do zrobienia coś ponad usuwanie `pub use` |
| D-11 | **Trzy odstępstwa od tabeli §4 R-WP4, wszystkie wymuszone przez kryterium „`mod.rs` poniżej 400 linii".** `docelowa_populacja` idzie do `pyramid.rs`, choć tabela jej nie wymienia (czyta pasma piramidy — jest wejściem kroku 1, nie orkiestracją), a dwa ciągłe bloki wychodzą z `generate_population` jako nowe funkcje: `pyramid::zasiedl` (krok 2b, pętla spawnu) i `traits::nadaj_cechy` (kroki 3–4). Oba ciała przeniesione co do znaku; zmieniły się wyłącznie trzy tokeny zamieniające zmienne lokalne w parametry | Bez tych trzech ruchów `mod.rs` miałby ~470 linii własnego kodu, a `generate_population` zostałoby na 280 — czyli **nad progiem błędu 250 z kryterium akceptacji nr 5**, którego stan sprzed pakietu nie spełniał. Tabela §4 rozdzielała kroki między pliki, ale nie przewidziała, że dwa z nich siedzą w ciele orkiestratora, a nie w osobnych funkcjach |
| D-10 | **Arena miała już swój test.** §4 R-WP3 obiecuje suballokatorowi „test jednostkowy, którego dziś nie ma, bo nie ma jak" — były dwa, w module `#[cfg(test)]` na końcu `renderer.rs`. Przeniosły się razem z kodem. Dopisany jest trzeci, pokrywający to, czego naprawdę brakowało: ponowne użycie dziury po zwolnionym bloku ze środka, brak nakładania się żywych bloków i zachowanie `alloc(0)` | Diagnoza z §1 („cztery najdłuższe pliki mają zero albo prawie zero testów") była trafna co do kierunku i nieprecyzyjna co do tego pliku. Trzeci test pilnuje milczącej umowy: `Arena::alloc(0)` zwraca `Block { offset: 0 }`, czyli przesunięcie kolidujące z pierwszym prawdziwym blokiem — nieszkodliwe dziś, ale nie jest to własność typu, tylko zwyczaj |
| D-7 | **Rusztowania `pub use` z procedury §3 pkt 2 nie powstają w crate'ach binarnych.** R-WP2 ich nie potrzebował: `magnat` jest binarką, więc wszystkie wywołania są wewnątrz crate'u i ścieżki poprawia się od razu jawnymi `use`, a kompilator weryfikuje je w całości. R-WP10 nie ma po tym pakiecie nic do posprzątania | Powód, dla którego rusztowanie w ogóle istnieje (§3 pkt 5, ryzyko R-6), znika, gdy zbiór wywołań jest zamknięty w jednym crate'cie i mieści się w jednym diffie. To samo będzie dotyczyć każdego pakietu, którego podział nie wychodzi poza crate — wpisane, żeby R-WP10 nie szukał czegoś, czego nie ma |
| D-6 | **Definicja „linii kodu" jest rozstrzygnięta i zmierzona:** wszystkie linie pliku poza blokiem `#[cfg(test)]`, razem z pustymi i komentarzami | §1 podawał liczby bez definicji. Kalibracja zgadza się co do jednej linii z §5 dla całej trójki wyjątków (`cch.rs` 942, `det_math.rs` 876, `graph.rs` 1064) i dla `renderer.rs` (2693), więc to jest ta definicja, którą liczono ręcznie — skrypt ją tylko utrwala |

---

## Zmiany wpisane po M5c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5c.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Dochodzi jedenasty plik: `sim/economy/src/market.rs`** — 1 253 linie przy pisaniu tego dokumentu, **1 975** po M5c. Szwy są widoczne z nazw metod: (a) cykl życia sklepu (`open_shop`, `record_capital`, `stock_initial`, `deliver_now`), (b) zaopatrzenie i półka (`restock_shelves`, `reorder_and_receive`, `expire_goods`), (c) doba cenowa (`observe_competitors`, `reprice_all`, `set_policy`, `preview_policy`), (d) raporty księgowe (cztery cienkie opakowania na `ledger::*`), (e) `PlaceProvider` (`candidates`, `fulfil`) — i to ostatnie jest największym pojedynczym kawałkiem | Plik przekroczył próg 1 200 linii w tej samej podfazie, w której R1 powstał, i z tego samego powodu co reszta listy: dokłada się do niego każda faza, a nikt nie pyta o rozmiar. Ograniczenie jest twardsze niż przy innych plikach: `MarketInner` jest prywatny, a wszystkie te metody sięgają do jego pól, więc podział musi iść przez `pub(crate)` na polach albo przez `impl Market` w modułach potomnych — **nie** przez wyniesienie funkcji wolnych. M6 dopisuje do tego pliku rynek B2B, więc termin „przed M6a" obowiązuje tak samo |
| | **`sim/economy` ma po M5c 7 555 linii w 13 plikach** (było 4 tys. w 8). Rozkład jest zdrowy poza `market.rs`: `ledger.rs` 782, `pricing.rs` 866, `books.rs` 917, reszta poniżej 600 | Nowe moduły M5c (`kernel`, `ledger`, `pricing`, `tax`) powstały od razu podzielone, więc R1 nie ma tam nic do roboty. To jest przy okazji argument za regułą z R-WP1: plik, który rodzi się z granicą, nie rozlewa się później |

---

## Zmiany wpisane po M5d

Zgodnie z `K-18`.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`market.rs` ma po M5d 2 655 linii** (1 253 przy pisaniu tego dokumentu, 1 975 po M5c), a `sim/economy` **10 169 linii w 16 plikach**. Do listy szwów z wpisu po M5c dochodzi szósty: **(f) miesiąc gospodarstwa i kredyt** (`household_month`, `pay_installment`, `apply_for_credit`, `service_working_capital`, `maybe_borrow_working_capital`) | Przyrost 680 linii w jednej podfazie jest największy z dotychczasowych i potwierdza diagnozę, a nie ją zmienia: do `market.rs` dokłada każda faza, bo `MarketInner` jest jedynym miejscem, z którego widać naraz sklepy, gospodarstwa, kredyty i koszyk CPI. Ograniczenie z wpisu po M5c obowiązuje bez zmian — podział musi iść przez `impl Market` w modułach potomnych, nie przez funkcje wolne. **Nowa obserwacja dla R-WP1:** szew (f) jest najczystszy z całej szóstki, bo dotyka wyłącznie `budgets`, `loans`, `bank` i `cpi`, a tych pól nie rusza nikt poza nim — jeśli podział ma się zacząć od jednego kawałka, to od tego |
| | **Nowe moduły M5d (`budget`, `credit`, `cpi`) powstały od razu podzielone**: 416, 566 i 404 linii, każdy z własnym `#[cfg(test)]` | Ten sam argument, co po M5c: plik, który rodzi się z granicą, nie rozlewa się później. R1 nie ma tam nic do roboty, a stan `budgets`/`loans`/`cpi` mieszka w `market.rs` wyłącznie dlatego, że `PlaceProvider` nie dostaje `&World` (`U-18`) — to jest ograniczenie architektury, nie brak podziału |

---

## Zmiany wpisane po M5e

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5e.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`market.rs` ma po M5e ~3 000 linii**, a `sim/economy` **17 plików**. Do szóstki szwów dochodzi **(g) odczyty dla prezentacji i pomiaru** — `shop_panel` i `balance_sample`. Szew jest **czystszy niż (f)**: obie funkcje są *tylko do odczytu*, nie dotykają ani jednego pola mutowalnie i nie mają żadnego stanu | To jest pierwszy kawałek `market.rs`, który da się przenieść **bez ryzyka**: funkcja czysto odczytowa przeniesiona do `impl Market` w module potomnym nie może zmienić ani jednego hasha, bo niczego nie zapisuje. Jeśli R-WP1 potrzebuje kawałka, na którym sprawdzi się skrypt kontroli strukturalnej i reguła „żaden hash nie zmienia się o bit", to jest ten kawałek — a nie (f) |
| ★ | **`tools/headless` ma od M5e target biblioteczny** (`src/lib.rs` z `pub mod population; pub mod retail;`) i **trzech konsumentów**: własną binarkę, `tools/magnat` i `tools/balansator`. Most „zakłady Etapu 7 → rynek detaliczny" (`retail::setup`) mieszka w crate'cie, którego nazwa mówi „headless", a który nie jest już wyłącznie headlessem | Wykonanie decyzji otwartej nr 10 fazy M5 (balansator jako biblioteka, nie proces). **Nazwa jest długiem strukturalnym i należy do R1**, nie do M5: klient graficzny zależny od `magnat-headless` czyta się jak pomyłka, a nie jak decyzja. Dwie drogi wyjścia, obie tanie, bo chodzi o `git mv` i jedną linię w `Cargo.toml` każdego konsumenta: (a) przemianowanie crate'u na `tools/harness`, (b) wydzielenie samego mostu do `tools/retail-bridge`. **Wariantu „przenieść most do `sim/economy`" nie ma**: `sim/economy` nie zna `CityData` i znać go nie może — most z definicji potrzebuje obu stron |
| | **`tools/balansator` powstał i od razu ma dwóch mieszkańców**: bramki G1–G9 oraz przeniesiony z headlessa `calibrate-vdf` (`T-1`) | R1 dostaje go jako crate **nowy**, czyli taki, który rodzi się z granicami — tak samo jak moduły M5c i M5d. Nie ma tam nic do podziału i to jest obserwacja, nie zaniedbanie |
| | **`engine/ui` zależy od `magnat-economy`** od M5e (panel sklepu czyta `ShopPanelSnapshot`) | Kierunek jest jednostronny i taki zostaje: `sim/economy` nie widzi interfejsu. Precedens `magnat-traffic` z M4d (`S-15`). R1 nie ma tu nic do zrobienia — zapisane, żeby skrypt kontroli strukturalnej nie zgłosił tego jako nowej krawędzi bez uzasadnienia |
