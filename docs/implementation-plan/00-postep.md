# Postęp prac — „Magnat"

Śledzenie na poziomie faz i bramek. Szczegółowe pakiety robocze są w sekcji 4 dokumentu każdej fazy —
tam się je odhacza, tutaj tylko zbiorczo.

Legenda: `[ ]` nierozpoczęte · `[~]` w toku · `[x]` ukończone i zweryfikowane

---

## Decyzje właściciela produktu (przed startem)

Szczegóły i argumentacja: `00-raport-audytu.md` §4.

- [ ] Odstępstwo od PRD §9.2 — mikro tylko w kadrze kamery (wpływa na kryteria akceptacji M4)
- [ ] Tryb 50×: 17,0 s/dobę wobec celu ≤ 3 s — co poświęcamy (drabina cięć w M12)
- [ ] Kolejność zaspokajania w upadłości — pracownicy vs wierzyciel zabezpieczony (M7)
- [ ] Maszyna referencyjna dla „GPU klasy średniej (2024)" (M11, M12)
- [ ] Akceptacja osłabienia kontraktu LOD dla makro (`K-5`) albo powrót do projektu trybu 50×
- [ ] **Nowe po M0:** akceptacja korekt planu z `M0-fundament-silnika.md` §4a — w szczególności
  K0-2 (granica dokładności `pow` zależna od wykładnika) i K0-10 (kryterium skalowania B-1c
  z zastrzeżeniem o wielkości pracy). Obie zmieniają liczby w kryteriach akceptacji, więc
  wymagają świadomej zgody, a nie milczącego przyjęcia

---

## Bramki fazy

Każda faza jest ukończona dopiero, gdy przejdzie **wszystkie siedem bramek**. Bramki wynikają
z szablonu w `00-konwencje-i-kontrakty.md` §8 i są identyczne dla każdej fazy:

| # | Bramka | Źródło wymagania |
|---|---|---|
| 1 | Wszystkie pakiety robocze z §4 ukończone wg własnych kryteriów | dokument fazy §4 |
| 2 | Testy determinizmu zielone; komponenty fazy dopisane do funkcji haszującej | `00` §3.6 |
| 3 | Testy własnościowe pieniądza i masy przechodzą z tolerancją 0 | `00` §6 |
| 4 | Benchmarki w zadeklarowanych budżetach (ms/tick, pamięć, FPS) | dokument fazy §7 |
| 5 | Każda decyzja fazy zapisuje `DecisionReason` i renderuje się w karcie inspekcji | `00` §7 |
| 6 | Kontrakty „dostarczam" z §6 faktycznie dostarczone i użyte przez fazę zależną | dokument fazy §6 |
| 7 | Decyzje otwarte z §9 zamknięte albo świadomie przeniesione dalej z adresatem | dokument fazy §9 |

---

## Fazy

### M0 — Fundament silnika
`M0-fundament-silnika.md` · bez zależności wejściowych, może ruszyć od razu

- [x] 1. Pakiety robocze · [x] 2. Determinizm · [x] 3. Własnościowe · [~] 4. Budżety
- [x] 5. Wyjaśnialność · [x] 6. Kontrakty · [x] 7. Decyzje otwarte

Bramka 4 jest `[~]`, a nie `[x]`, z jednego powodu: **B-1c osiąga 5,2× zamiast 8×**
przy 16 wątkach. To nie jest wada implementacji (przy 70 µs pracy dominuje narzut
planowania zadań), tylko kryterium sformułowane bez zastrzeżenia o wielkości pracy —
korekta K0-10 czeka na akceptację. Pozostałe jedenaście benchmarków mieści się
w celach, część z wielokrotnym zapasem (M0 §4a).
Bramka 5 jest domknięta mechanizmem, nie treścią: M0 nie ma decyzji domenowych,
więc `DecisionReason` ma na razie jeden wariant — pilnuje go test `trybuild`,
który wywala kompilację `match` bez ramienia `_` (to jest cały sens K-12).
- [~] **Faza ukończona** — artefakt działa (headless runner + okno `wgpu` z chunkiem 32³,
  hashe identyczne przy 1/2/8/16 wątkach, wznowienie z zapisu nieodróżnialne od przebiegu
  ciągłego). **Do domknięcia przez właściciela repozytorium, bo wymaga środowisk,
  których nie ma na maszynie deweloperskiej:**
  1. przebieg joba `miri` (wymaga toolchaina nightly — job jest w `ci.yml`, kod ma
     `unsafe` wyłącznie w `engine/ecs/src/chunk.rs`),
  2. przebieg macierzy CI na GitHub Actions (Linux + Windows) przy pierwszym push,
  3. potwierdzenie ≥ 60 FPS w `magnat-voxelview` na docelowej maszynie (okno startuje
     na Vulkanie, licznik FPS jest w tytule),
  4. T-D9 na `aarch64` (QEMU, przebieg nocny) — złoty odcisk `det_math` jest zatwierdzony
     dla `x86_64-pc-windows-msvc`.

### M1 — Świat statyczny
`M1-swiat-statyczny.md` · wymaga: M0

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: oglądalny krajobraz z seeda, rzeki z ujściem, cykl dobowy

### M2 — Miasto statyczne
`M2-miasto-statyczne.md` · wymaga: M1

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: miasto do oglądania, nakładka wartości gruntu

### M3 — Ludzie i dzień
`M3-ludzie-i-dzien.md` · wymaga: M2

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: mieszkańcy chodzą do pracy i sklepu, karta inspekcji z osią dnia

### M4 — Ruch
`M4-ruch.md` · wymaga: M3

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: korki widoczne i mierzone, spójność mikro↔mezo z tolerancją 0

### M5 — Gospodarka detaliczna
`M5-gospodarka-detaliczna.md` · wymaga: M4 · **pierwsza pętla gracza (vertical slice)**

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: otwórz sklep, ustal ceny, obserwuj klientów; balansator w CI

### M6 — Łańcuch dostaw
`M6-lancuch-dostaw.md` · wymaga: M5

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: paliwo od złoża do baku, sklepy przestają być nieskończone

### M7 — Firmy AI i rynek pracy
`M7-firmy-ai-i-rynek-pracy.md` · wymaga: M6

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: konkurencja reaguje na gracza, pensje emergentne

### M8 — Miasto jako aktor
`M8-miasto-jako-aktor.md` · wymaga: M7

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: podatki, wybory, sieci przesyłowe, zdarzenia i pogoda

### M9 — Gracz: kariera
`M9-gracz-kariera.md` · wymaga: M8

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: pełna ścieżka kariery, panele biznesowe, automatyzacja polityk

### M10 — Głębia
`M10-glebia.md` · wymaga: M9

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: marka z pamięci agentów, R&D, giełda, historia „na sucho"

### M11 — Prezentacja
`M11-prezentacja.md` · wymaga: M10

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: detale, animacje, wnętrza, audio, cele FPS z §20.2

### M12 — Skala i jakość
`M12-skala-i-jakosc.md` · wymaga: M11

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: metropolia 400 tys. ≤ 6 GB, zapis w tle, modding, sesja 100-letnia

---

## Dziennik

Jedna linia na zamknięty pakiet roboczy lub bramkę. Najnowsze na górze.

| Data | Faza | Co zamknięto | Uwagi |
|---|---|---|---|
| 2026-09-13 | M0 | **Bramki 1–7 fazy M0.** Cały workspace: 7 crate'ów, 147 testów zielonych, `clippy -D warnings` bez wyjątków, benchmarki B-1…B-9 w budżetach (poza B-1c, patrz M0 §4a K0-10) | Do domknięcia bramki „faza ukończona": Miri, macierz CI, FPS w voxelview, ARM w T-D9 |
| 2026-09-13 | M0 | WP-15: bootstrap `wgpu`/`winit`, chunk 32³, kamera orbitalna; `wgpu` nie przenika do headless | wgpu 30 zmieniło API względem planu (prezentacja przez kolejkę, `immediate_size`, `CurrentSurfaceTexture`) — ryzyko R-5 zmaterializowane i obsłużone |
| 2026-09-13 | M0 | WP-13, WP-14: benchmarki, `benches/baseline.json`, `scripts/bench_guard.py`, macierz CI z jobami `check`/`miri`/`determinism`/`bench-guard` | Progi D-8: 10 % ostrzeżenie, 25 % błąd |
| 2026-09-13 | M0 | WP-12: runner headless ze światem syntetycznym (6 archetypów, 12 komponentów, 8 systemów) i testem negatywnym `--features chaos` | T-D8 potwierdzony: build chaos zawodzi `--expect` na ticku 500 |
| 2026-09-13 | M0 | WP-11: `Inspector`, `Console`, `scope!`, `MetricSink` z eksportem CSV | Konsument metryk: balansator M5 |
| 2026-09-13 | M0 | WP-09, WP-10: `world_state_hash` (T-D5 zielony), snapshot sekcyjny zstd (T-D4 i T-D13 zielone) | Korekty K0-5, K0-8, K0-9 w M0 §4a |
| 2026-09-13 | M0 | WP-07, WP-08: scheduler DAG i bufory komend; 2000 ticków identycznych przy 1/2/8/16 wątkach | Flush 100 tys. komend: 233 ms → 12,9 ms po zamianie wyszukiwania rezerwacji na binarne |
| 2026-09-13 | M0 | WP-04, WP-05: storage archetypowy w chunkach 64 KiB (kontrakt layoutu z M12), `Query`/`Access` wyliczane z typów | D-7 rozstrzygnięte odwrotnie niż domyślnie: od razu wersja z `unsafe` (K0-12) |
| 2026-09-13 | M0 | WP-06: `JobPool` nad rayon, `map_reduce_indexed` niezależny od liczby wątków | D-1 przyjęte domyślnie |
| 2026-09-13 | M0 | WP-02, WP-02b, WP-03, WP-03b: `engine/core` w całości — pieniądz, `Fx`, kalendarz 360-dniowy, `Entity`, `Arena<T>`, słowniki K-8, `DecisionReason`, RNG, `det_math` | Korekty K0-1…K0-4, K0-11 w M0 §4a; tabela referencyjna `det_math` liczona w 60 cyfrach |
| 2026-09-13 | M0 | WP-01: workspace, toolchain, `clippy.toml` z zakazem libm i `HashMap` | Zakaz z 00 §K-6 i §3.2 egzekwowany lintem od pierwszej linii kodu |
| 2026-09-13 | — | Plan implementacji: 13 faz, kontrakty `K-1`…`K-16` | `00-raport-audytu.md` |
