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

- [x] **1. Pakiety robocze** — WP-01…WP-15 zamknięte, każdy wg własnego kryterium
  (statusy przy pakietach w `M0-fundament-silnika.md` §4)
- [x] **2. Determinizm** — T-D1…T-D8 i T-D11…T-D13 zielone; hashe bajt w bajt identyczne
  przy 1/2/8/16 wątkach, wznowienie z zapisu nieodróżnialne od przebiegu ciągłego,
  test negatywny `--features chaos` zawodzi na właściwym ticku
- [x] **3. Własnościowe** — `split_proportional` sumuje się do kwoty dzielonej dla 10⁵
  losowych przypadków (w tym kwot ujemnych i wag zerowych); masy M0 jeszcze nie ma,
  bo nie ma towarów
- [~] **4. Budżety** — 11 z 12 benchmarków w celu, część z wielokrotnym zapasem (M0 §4a).
  Poza celem **B-1c: 5,2× zamiast 8×** przy 16 wątkach — to nie wada implementacji
  (przy 70 µs pracy dominuje narzut planowania zadań), tylko kryterium sformułowane
  bez zastrzeżenia o wielkości pracy. Bramka domknie się wraz z akceptacją korekty K0-10
- [x] **5. Wyjaśnialność** — mechanizm gotowy i egzekwowany przez kompilator: `match`
  po `DecisionReason` bez ramienia `_` nie kompiluje się (test `trybuild`). Treści nie ma,
  bo M0 nie podejmuje decyzji domenowych — pierwszy wariant dopisze M3
- [x] **6. Kontrakty** — całe §6 dokumentu fazy ma pokrycie w publicznym API; konsumentem
  wewnątrz M0 są `tools/headless` (ECS, jobs, io, devtools) i `engine/io` (kontrakt chunków)
- [x] **7. Decyzje otwarte** — D-1, D-4, D-8, D-12 przyjęte domyślnie; D-6 i D-2 były już
  zamknięte; **D-7 rozstrzygnięte odwrotnie niż domyślnie** (K0-12); D-3, D-5, D-9, D-11
  przeniesione dalej z adresatem (M1, M2, M4), zgodnie z treścią bramki
- [~] **Faza ukończona** — artefakt działa: runner headless, okno `wgpu` z chunkiem 32³,
  147 testów zielonych, `clippy -D warnings` bez wyjątków. **Cztery pozycje do domknięcia
  wymagają środowisk, których nie ma na maszynie deweloperskiej:**
  1. przebieg joba `miri` (potrzebny toolchain nightly; job jest w `ci.yml`, `unsafe`
     siedzi wyłącznie w `engine/ecs/src/chunk.rs`),
  2. przebieg macierzy CI na GitHub Actions (Linux + Windows) przy pierwszym push,
  3. potwierdzenie ≥ 60 FPS w `magnat-voxelview` na maszynie docelowej (okno startuje
     na Vulkanie, licznik FPS jest w tytule),
  4. T-D9 na `aarch64` (QEMU, przebieg nocny) — złoty odcisk `det_math` jest na razie
     zatwierdzony dla `x86_64-pc-windows-msvc`.

### M1 — Świat statyczny
`M1-swiat-statyczny.md` · wymaga: M0

- [x] **1. Pakiety robocze** — W1–W7, V1–V4, R1–R6, C1, X1 zamknięte wg własnych kryteriów
  (statusy przy pakietach w `M1-swiat-statyczny.md` §4)
- [x] **2. Determinizm** — `terrain_hash_stable`, `terrain_thread_invariant` (1/2/8/16 wątków)
  i `terrain_hash_matrix` (32 seedy × 5 regionów, tabela zatwierdzona w repo) zielone;
  `tools/headless verify --runs 2` daje identyczny hash terenu. Hash obejmuje wysokość,
  wody, sieć rzeczną, złoża, klimat i odległość od wody — czyli wszystko, co faza dokłada
  do stanu trwałego
- [x] **3. Własnościowe** — `deposit_ledger`: 10 tys. losowań ciągów `extract()`, zero ubytku
  masy, `extracted ≤ reserves`, brak przepełnienia przy zasobach do 2⁶³ g. Pieniądza M1 nie
  dotyka, bo nie ma jeszcze transakcji
- [x] **4. Budżety** — wszystkie progi §7.3 zmierzone i spełnione: generacja 16 km ≤ 20 s,
  P4 1,0–1,2 s wobec 1,5 s, stan trwały 53 MB wobec 60 MB, meshing 0,54–0,59 ms wobec 0,8 ms,
  agregat LOD3 poniżej 1,5 ms, pula chunków i arena GPU w zadeklarowanych limitach.
  Render (§7.4, RTX 4070 Ti): mediana 860–1537 FPS na etap, 1 % low 114–974, **0 zacięć**
  wobec progu ≤ 2 na 60 s
- [x] **5. Wyjaśnialność** — M1 nie podejmuje decyzji agentowych (generacja jest funkcją
  seeda, nie wyborem), więc `DecisionReason` nie ma tu czego opisywać. Odpowiednikiem karty
  inspekcji jest **karta terenu**: klik prawym w oknie albo `magnat --inspect x,y` wypisuje
  kolumnę geologiczną, wody gruntowe, klimat, przydatność pod zabudowę i złoża — i robi to
  wyłącznie przez `TerrainQuery`, czyli przez ten sam interfejs, który dostanie M2
- [x] **6. Kontrakty** — dostarczone i używane wewnątrz fazy: `TerrainQuery` (wszystkie pięć
  grup z §6.1, konsumowane przez inspektor i nakładki), `ColumnSource`, `sim-snapshot`,
  `RenderGraph::register` + `GraphSlot`, `TerrainOverlay`, `ClusterOccupancy`, `EditReport`.
  **Domknięta przez M2 (2026-09-14).** Generator miasta czyta teren wyłącznie przez
  `TerrainQuery` — łącznie ze wszystkimi pięcioma rozszerzeniami z 9.1/19 (`water_depth_at`,
  `deposit_at`, `soil_quality_at`, `prevailing_wind`, `flood_risk_at`) — pisze bryły przez
  `EditQueue`/`EditIndex`, a nakładka wartości gruntu jedzie przez `TerrainOverlay` tym samym
  mechanizmem, co osiem podglądów M1. Ani razu nie sięgnął do wnętrza chunka
- [~] **7. Decyzje otwarte** — D1–D11 przyjęte zgodnie ze stanowiskiem M1 (D9 rozstrzygnięte
  wcześniej z M11). Wymagają potwierdzenia adresatów: D1 (pule priorytetów w `engine/jobs`)
  i D2 (własność `wgpu`) — M0; D4, D5, D8, D11 — M2; D6 — M6; D7 — M8. W M1 żadna z nich
  nie zablokowała pakietu, bo faza nie ma jeszcze drugiego konsumenta
- [~] **Faza ukończona** — artefakt działa: `magnat --seed … --region …` pokazuje krajobraz
  z seeda, rzeki z ujściem, jeziora, cykl dobowy z cieniami kaskadowymi i nakładki `F3`.
  Zostaje potwierdzenie bramek 6 i 7 przez fazy zależne

### M2 — Miasto statyczne
`M2-miasto-statyczne.md` · wymaga: M1

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [x] **M2a** Indeksy przestrzenne — `M2a-indeksy-przestrzenne.md` (WP1, WP2)
- [x] **M2b** Szkielet transportu — `M2b-szkielet-transportu.md` (WP3, WP4, WP5, WP6; kolej → WP5b w M2c)
- [x] **M2c** Strefy, parcele, dzielnice — `M2c-strefy-parcele-dzielnice.md` (WP7, WP5b, WP8, WP9)
- [x] **M2d** Zabudowa — `M2d-zabudowa.md` (WP10, WP15a, WP11, WP12, WP12b)
- [x] **M2f** Różnorodność zabudowy — `M2f-roznorodnosc-zabudowy.md` (WP18, WP19, WP20) — wykonywana **przed** M2e
- [x] **M2e** Gospodarka bazowa i wycena — `M2e-gospodarka-bazowa-i-wycena.md` (WP13, WP14, WP15b, WP16, WP17)

Bramki fazy:

- [x] **1. Pakiety robocze** — WP1–WP20 zamknięte wg własnych kryteriów; kryteria zmienione
  po pomiarze są wypisane imiennie w tabelach korekt `B`–`I` dokumentów podfaz
- [x] **2. Determinizm** — D1 (`world_hash_m2` identyczny w dwóch przebiegach), D2 (1 wątek ==
  8 wątków, razem z Etapem 7 i domknięciem), D3/D4 z M2a i M2c, D5 (hash obejmuje warstwę M2e —
  **z testem negatywnym**: zmiana `capacity_scale` jednego zakładu musi go ruszyć)
- [x] **3. Własnościowe** — bilans masy każdej receptury `Manufacturing`
  (`Σ inputs == Σ outputs + process_loss`) egzekwowany przy ładowaniu katalogu i testem
  jednostkowym; pieniądza M2 nie przepływa — wycena liczy się `div_round_half_up`
  z jednym zaokrągleniem na końcu i nie kumuluje `Money` z floata
- [~] **4. Budżety** — czas zmierzony i mieści się z zapasem: metropolia 3,5 s generacji
  miasta wobec 60 s budżetu (Etap 7 + domknięcie 80 ms wobec 4 s, wycena 22 ms wobec 3 s),
  miasto małe poniżej 1 s wobec 12 s. **Zostaje jedna pozycja: 60 FPS z włączoną nakładką
  wartości gruntu** — wymaga maszyny z GPU, tak samo jak niedomknięte pozycje wydajności
  renderu z M1
- [x] **5. Wyjaśnialność** — `LandValueBreakdown` na dwanaście czynników, liczony na żądanie
  i pokazywany w karcie inspekcji **tą samą funkcją** w kliencie graficznym i w `headless
  preview --inspect`; karta niesie też zakład, firmę, receptury i skalę. `DecisionReason`
  nie ma tu czego opisywać: generacja jest funkcją ziarna, nie wyborem agenta —
  pierwszy wariant dopisze M3
- [~] **6. Kontrakty** — całe §6 dostarczone i używane wewnątrz fazy: `engine/spatial`,
  `RoadNetwork`, `Parcel`, `Building`/`Unit`/`Workplace`, `FirmSeed`/`SiteSeed`,
  `land_value_at`, `supply_closure_check`, katalogi `data/`. Bramka domknie się, gdy
  **M3 ich faktycznie użyje** — tego M2 nie sprawdzi sam za siebie, tak samo jak M1 nie
  sprawdzał swojego. W drugą stronę: **M2 domyka bramkę 6 fazy M1** (patrz wyżej)
- [x] **7. Decyzje otwarte** — §9.1 (1–20) rozstrzygnięte; §9.2/21 i 9.2/22 zamknięte w M2f,
  §9.2/3 i 9.2/5 w M2e. Nic nie zostaje otwarte
- [~] **Faza ukończona** — artefakt działa: `magnat --seed …` pokazuje miasto z firmami
  i nakładką wartości gruntu (`F3`), `headless preview --field value --inspect x,y` daje
  ten sam obraz i tę samą kartę bez GPU, `--test consistency` zielony na 32 ziarnach
  × 4 profile. Zostaje potwierdzenie 60 FPS z nakładką na maszynie z GPU i bramka 6
  po stronie M3

### M3 — Ludzie i dzień
`M3-ludzie-i-dzien.md` · wymaga: M2

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M3a** Fundament agenta — `M3a-fundament-agenta.md` (WP1, WP2, WP3, WP4)
- [ ] **M3b** Dzień mieszkańca — `M3b-dzien-mieszkanca.md` (WP5, WP6)
- [ ] **M3c** Demografia i społeczeństwo — `M3c-demografia-i-spoleczenstwo.md` (WP7, WP8, WP9)
- [ ] **M3d** Populacja i UI — `M3d-populacja-i-ui.md` (WP10, WP11, WP12, WP13)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: mieszkańcy chodzą do pracy i sklepu, karta inspekcji z osią dnia

### M4 — Ruch
`M4-ruch.md` · wymaga: M3

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M4a** Graf i routing — `M4a-graf-i-routing.md` (WP1, WP2)
- [ ] **M4b** Mezo i podróże — `M4b-mezo-i-podroze.md` (WP3, WP4, WP5)
- [ ] **M4c** Wybór środka, parkingi, komunikacja — `M4c-wybor-srodka-parkingi-komunikacja.md` (WP6, WP7, WP10)
- [ ] **M4d** Mikro i dowód spójności — `M4d-mikro-i-dowod-spojnosci.md` (WP8, WP9, WP11, WP12)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: korki widoczne i mierzone, spójność mikro↔mezo z tolerancją 0

### M5 — Gospodarka detaliczna
`M5-gospodarka-detaliczna.md` · wymaga: M4 · **pierwsza pętla gracza (vertical slice)**

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M5a** Pieniądz i oferta — `M5a-pieniadz-i-oferta.md` (WP1, WP2)
- [ ] **M5b** Sklep i zakup — `M5b-sklep-i-zakup.md` (WP3, WP4, WP5)
- [ ] **M5c** Ceny i księgowość — `M5c-ceny-i-ksiegowosc.md` (WP6, WP7, WP11)
- [ ] **M5d** Budżety, banki, inflacja — `M5d-budzety-banki-inflacja.md` (WP8, WP9, WP10)
- [ ] **M5e** Panel, balansator, domknięcie — `M5e-panel-balansator-domkniecie.md` (WP12, WP13, WP14)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: otwórz sklep, ustal ceny, obserwuj klientów; balansator w CI

### M6 — Łańcuch dostaw
`M6-lancuch-dostaw.md` · wymaga: M5

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M6a** Katalog i partia — `M6a-katalog-i-partia.md` (WP1, WP2, WP3)
- [ ] **M6b** Zakład i transport — `M6b-zaklad-i-transport.md` (WP4, WP5, WP6)
- [ ] **M6c** Rynek B2B — `M6c-rynek-b2b.md` (WP7, WP8, WP9)
- [ ] **M6d** Złoża i koniec dostawcy zewnętrznego — `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md` (WP10, WP11, WP12)
- [ ] **M6e** Panel, testy, pamięć — `M6e-panel-testy-pamiec.md` (WP13, WP14, WP15)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: paliwo od złoża do baku, sklepy przestają być nieskończone

### M7 — Firmy AI i rynek pracy
`M7-firmy-ai-i-rynek-pracy.md` · wymaga: M6

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M7a** Firma jako dane — `M7a-firma-jako-dane.md` (WP1, WP2, WP3)
- [ ] **M7b** Rynek pracy — `M7b-rynek-pracy.md` (WP4, WP5, WP6)
- [ ] **M7c** Polityki i menedżerowie — `M7c-polityki-i-menedzerowie.md` (WP6b, WP7)
- [ ] **M7d** Finanse i upadłość — `M7d-finanse-i-upadlosc.md` (WP8, WP9)
- [ ] **M7e** AI firm — `M7e-ai-firm.md` (WP10, WP11, WP12, WP12b, WP14)
- [ ] **M7f** Makro i domknięcie — `M7f-makro-i-domkniecie.md` (WP13, WP15, WP16, WP17)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: konkurencja reaguje na gracza, pensje emergentne

### M8 — Miasto jako aktor
`M8-miasto-jako-aktor.md` · wymaga: M7

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M8a** Pieniądz publiczny — `M8a-pieniadz-publiczny.md` (WP1, WP2)
- [ ] **M8b** Sieci przesyłowe — `M8b-sieci-przesylowe.md` (WP3)
- [ ] **M8c** Zdarzenia — `M8c-zdarzenia.md` (WP4, WP5, WP6)
- [ ] **M8d** Usługi publiczne i prawo — `M8d-uslugi-i-prawo.md` (WP7, WP8)
- [ ] **M8e** Władza i wybory — `M8e-wladza-i-wybory.md` (WP9, WP10, WP11)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: podatki, wybory, sieci przesyłowe, zdarzenia i pogoda

### M9 — Gracz: kariera
`M9-gracz-kariera.md` · wymaga: M8

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M9a** Szkielet gry i komendy — `M9a-szkielet-gry-i-komendy.md` (WP1, WP2)
- [ ] **M9b** Rdzeń UI — `M9b-rdzen-ui.md` (WP3, WP6)
- [ ] **M9c** Gracz, inspekcja, nakładki — `M9c-gracz-inspekcja-nakladki.md` (WP4, WP5, WP7)
- [ ] **M9d** Język reguł — `M9d-jezyk-regul.md` (WP8, WP9)
- [ ] **M9e** Panele, czas, kariera — `M9e-panele-czas-kariera.md` (WP10, WP11, WP12)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: pełna ścieżka kariery, panele biznesowe, automatyzacja polityk

### M10 — Głębia
`M10-glebia.md` · wymaga: M9

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M10a** Jądro, makro, historia na sucho — `M10a-jadro-makro-historia.md` (WP10.2, WP10.1, WP10.3, WP10.4)
- [ ] **M10b** Marka i media — `M10b-marka-i-media.md` (WP10.5, WP10.6, WP10.7)
- [ ] **M10c** R&D i nowe produkty — `M10c-rd-i-nowe-produkty.md` (WP10.8, WP10.9)
- [ ] **M10d** Giełda, przejęcia, ubezpieczenia — `M10d-gielda-przejecia-ubezpieczenia.md` (WP10.10, WP10.11, WP10.12)
- [ ] **M10e** Relacje i związki — `M10e-relacje-i-zwiazki.md` (WP10.13, WP10.14)
- [ ] **M10f** Kroniki i domknięcie — `M10f-kroniki-i-domkniecie.md` (WP10.15, WP10.16)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: marka z pamięci agentów, R&D, giełda, historia „na sucho"

### M11 — Prezentacja
`M11-prezentacja.md` · wymaga: M10

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M11a** Format i kontrakt snapshotu — `M11a-format-i-snapshot.md` (WP1, WP11, WP2)
- [ ] **M11b** Animacja i LOD wizualne — `M11b-animacja-i-lod.md` (WP3, WP4)
- [ ] **M11c** Wnętrza i kamera FPP — `M11c-wnetrza-i-kamera.md` (WP5, WP9)
- [ ] **M11d** Światło, pogoda, dźwięk — `M11d-swiatlo-pogoda-dzwiek.md` (WP6, WP7, WP8)
- [ ] **M11e** Budżet klatki — `M11e-budzet-klatki.md` (WP10)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: detale, animacje, wnętrza, audio, cele FPS z §20.2

### M12 — Skala i jakość
`M12-skala-i-jakosc.md` · wymaga: M11

Podfazy — porcje wykonawcze; kryterium zamknięcia każdej jest w jej dokumencie:

- [ ] **M12a** Pamięć — `M12a-pamiec.md` (WP1, WP2, WP3)
- [ ] **M12b** Zapis i replay — `M12b-zapis-i-replay.md` (WP4, WP5, WP6, WP7, WP8)
- [ ] **M12c** Tryb 50× i skala 400 tys. — `M12c-tryb-50x-i-skala.md` (WP9, WP10)
- [ ] **M12d** Modding — `M12d-modding.md` (WP11, WP12)
- [ ] **M12e** Lokalizacja i domknięcie — `M12e-lokalizacja-i-domkniecie.md` (WP13, WP14)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: metropolia 400 tys. ≤ 6 GB, zapis w tle, modding, sesja 100-letnia

---

## Dziennik

Jedna linia na zamknięty pakiet roboczy lub bramkę. Najnowsze na górze.

| Data | Faza | Co zamknięto | Uwagi |
|---|---|---|---|
| 2026-09-14 | M2 | **M2e — podfaza zamknięta, faza M2 domknięta w zakresie, który da się domknąć bez GPU i bez M3.** Etap 7: katalog 72 towarów i 62 receptur (`data/goods/`, `data/recipes/`), 79 archetypów zakładów (`data/buildings/`), 11 szablonów łańcuchów, domknięcie Dowlinga–Galliera z naprawą importem i bilansem przepustowości, `FirmSeed`/`SiteSeed` z nazwami proceduralnymi. `pass_2` wyceny z czterema czynnikami dostępności liczonymi separowalnym rozmyciem wykładniczym. Nakładka `land_value` z `data/ui/overlays.ron` — ta sama paleta w kliencie (`F3`) i w `headless preview --field value`. Karta inspekcji wspólna dla obu ścieżek. **13 testów spójności zielonych na macierzy 32 ziarna × 4 profile** (128 światów, 78 s). Metropolia: 19 152 budynki, 170 000 mieszkań, 218 410 stanowisk, 3 908 zakładów, 3,5 s | **Siedemnaście korekt (I-1…I-17), dziewięć zmienia zakres albo kryterium — najwięcej w całej fazie i to jest treścią tego wpisu.** Macierz 32 × 4 uruchomiona po raz pierwszy dała **256 naruszeń na 128 światach**; z tego tylko część dotyczyła M2e, reszta to rzeczy, których nikt wcześniej nie zmierzył. (I-6) **T10 okazał się niespełnialny w danych.** F6 wskazywał trzy dźwignie — gęstość parcel, `massing.max_depth_m`, `m2_per_workplace` — przestawiłem wszystkie trzy i nie wystarczyło: rozrzut pojemności mieszkaniowej wobec `target_pop` wynosi **0,61–1,08** i bierze się z tego, ile płaskiego terenu dał M1, a nie z parametrów. Okna szerokiego na 11 pp. nie trafia się stałą, więc obie połówki T10 są teraz **kalibrowane konstrukcyjnie** po Etapie 6, a zastosowany współczynnik stoi w raporcie. (I-2, I-4) **Domknięcie łańcuchów dostało dwie dźwignie, których §5.8 nie ma:** liczba zakładów wyprowadzana z popytu (bez tego 2 400 działek rolnych metropolii dawało nadwyżkę kilkunastokrotną, której limit 0,4 skali nie zje) i gaszenie nadmiarowych zakładów. Górna granica `ratio ≤ 1,30` **nie obowiązuje produktu ubocznego**: rzeźnia skalowana mięsem daje 3,6 × zapotrzebowania na skórę i żadne skalowanie tego nie zmieni bez zepsucia mięsa. (I-9) **Zakaz ruchu ciężkiego nadaje L-system po tonażu klasy, kiedy stref jeszcze nie ma** — przez co ulica obsługująca halę magazynową była ulicą osiedlową i 39 światów ze 128 oblewało T4, miejscami po 58 % budynków. (I-12) **T12 mierzył korelację, nie model:** w mieście ciasnym przemysł ciężki zostaje przy śródmieściu, więc „sąsiedztwo huty podnosi wartość” wychodziło w 78 ze 128 światów; teraz mierzy się sam czynnik `Pollution` i jego gradient. (I-13) Luka w katalogu gramatyk: **strefa R4 w pierścieniu średniowiecznym i przemysłowym nie miała ani jednej gramatyki** i 15,5 % budynków szło na rozluźnionym filtrze epoki — gęsta zabudowa z 1890 roku jest kamienicą, a nie blokiem, którego wtedy nie było. **Dwa znaleziska są o cudzych podfazach i zostają M2c:** osierocony segment ulicy lokalnej w 2 światach ze 128 (I-14) i `Block.area_m2` mniejsze od sumy działek kwartału, do 1,85 × (I-15) — oba mają teraz ostrzeżenie w raporcie albo test i nie znikną po cichu. Nauka ogólniejsza, ta sama co po M2f i mocniejsza: **kryterium, którego nikt nie puścił na macierzy, jest hipotezą, a nie kryterium** — siedem z trzynastu testów z §7 zmieniło brzmienie w dniu, w którym pierwszy raz zobaczyły 128 światów zamiast jednego |
| 2026-09-14 | M2 | **M2f/WP20 — podfaza M2f zamknięta.** `BuildingSignature` (gramatyka, kondygnacje, materiał ściany, kształt dachu) liczona w generacji i nietrzymana w `Building`; w `GenerationReport` histogram gramatyk, powtórki sygnatur w promieniu 60 m ze współrzędnymi najgorszego miejsca, entropia sygnatur w mieście i najniższa w dzielnicy wraz z gramatyką, która ją zdominowała. Test **T13** na 32 ziarnach × 4 profile: powtórki < 15 % (najgorsze 10,5 %), entropia dzielnicy ≥ 1,8 bita (najgorsza 3,45), entropia miasta ≥ 4,0 bita (najgorsza 5,48) | **Pięć korekt (H12–H16), trzy zmieniają kryterium.** (H12) **Entropia liczona po sygnaturach, nie po `GrammarId`** — §5.6c mierzył złą rzecz: dzielnica „Bór" dostała 0,29 bita, bo 108 ze 112 budynków to `chalupa`, tyle że te chałupy różnią się materiałem, dachem i wysokością, więc z ulicy **są** różne; po zamianie ta sama dzielnica ma 3,45 bita. Doszedł drugi próg — entropia całego miasta — bo dzielnica ma prawo być jednorodna, a miasto nie ma. (H13) **Reguły rozdzielające (`Seq`, `Choice`, `If`, `Ref`) zwolnione ze strażnika zdegenerowanego zakresu**: bryła w momencie wyboru dachu ma zerową wysokość, więc `Choice([Roof, Roof])` wypadał w całości i **trzeci kanał wariancji z §5.6c był martwy od M2d**. (H14) Kształt dachu jako trzeci kanał `Choice` w dziewięciu gramatykach — powtórki sygnatury **14,2 % → 8,6 %** bez dokładania ani jednego pliku. (H15, H16) **Miara znalazła trzy monokultury niewidoczne w pokryciu**, wszystkie tego samego rodzaju: próg `land_value` w `applies` wycinał jedynego konkurenta, więc jedna gramatyka brała 93 % losowań w dzielnicy przy pełnej macierzy i zerowym fallbacku. Reguła wyprowadzona z tego: **próg wartości gruntu wolno postawić typowi, poniżej którego ktoś inny przejmuje** (wieżowiec ma pod sobą blok, willa ma pod sobą kostkę) — nigdy typowi, który jest dla swojej kombinacji jedynym kandydatem. Osobna nauka o samym testowaniu: 16-punktowa próbka, na której skalibrowałem progi, **przepuściła przypadek o 4,6 pp. gorszy** od wszystkiego, co widziała (ziarno 1, profil przemysłowy, 16,5 % przy limicie 15 %) — kalibrować wolno wyłącznie na tej macierzy, na której test potem chodzi |
| 2026-09-14 | M2 | **M2f/WP20 — podfaza M2f zamknięta.** `BuildingSignature` (gramatyka, kondygnacje, materiał ściany, kształt dachu) liczona w generacji i nietrzymana w `Building`; w `GenerationReport` histogram gramatyk, powtórki sygnatur w promieniu 60 m z współrzędnymi najgorszego miejsca, entropia sygnatur w mieście i najniższa w dzielnicy wraz z gramatyką, która ją zdominowała. Test **T13** na 32 ziarnach × 4 profile. Zmierzone progi: powtórki < 15 % (najgorsze 11,9 %), entropia dzielnicy ≥ 1,8 bita (najgorsza 3,45), entropia miasta ≥ 4,0 bita (najgorsza 5,91) | **Cztery korekty (H12–H15), dwie zmieniają kryterium.** (H12) **Entropia liczona po sygnaturach, nie po `GrammarId`** — §5.6c mierzył złą rzecz: dzielnica „Bór" dostała 0,29 bita, bo 108 ze 112 budynków to `chalupa`, tyle że te chałupy różnią się materiałem, dachem i wysokością, więc z ulicy **są** różne; po zamianie ta sama dzielnica ma 3,45 bita. Doszedł drugi próg — entropia całego miasta — bo dzielnica ma prawo być jednorodna, a miasto nie ma. (H13) **Reguły rozdzielające (`Seq`, `Choice`, `If`, `Ref`) zwolnione ze strażnika zdegenerowanego zakresu**: bryła w momencie wyboru dachu ma zerową wysokość, więc `Choice([Roof, Roof])` wypadał w całości i trzeci kanał wariancji z §5.6c był **martwy od M2d**. (H14) Kształt dachu jako trzeci kanał `Choice` w dziewięciu gramatykach — powtórki sygnatury **14,2 % → 8,6 %** bez dokładania ani jednego pliku. (H15) **Miara od razu znalazła dwie monokultury niewidoczne w pokryciu**: `kamienica_modernistyczna` miała próg wartości gruntu, przez co na taniej ziemi nie kandydowała i `kamienica_plombowa` brała 441 z 474 budynków dzielnicy; `blok_wspolczesny` wymagał 14 m frontu. Macierz była wtedy pełna, a fallback poniżej progu — dokładnie po to jest druga miara. **Progi są podłogami przeciw regresji, nie celami**, skalibrowane 16-punktowym pomiarem po WP19; wymyślanie ich przed pomiarem raz już się nie udało (H12) |
| 2026-09-14 | M2 | **M2f/WP19** — katalog gramatyk z 12 do **32 plików**, macierz pokrycia (strefa × epoka × styl) bez luk, `Applies::covers` jako jedno źródło prawdy o dopasowaniu, rozbicie licznika rozluźnień na epokę/styl/wartość gruntu. Miasto 8 km, ziarno 7: gramatyka awaryjna **55 → 8 (1,07 % → 0,15 %)**, dobór po rozluźnieniu filtru **295 → 35 (5,7 % → 0,67 %)**, luki macierzy **9 → 0**, mieszkań 40 810 → 47 462, etap zabudowy 40,6 ms | Nowe typy: **bliźniak** (nie było go wcale), chałupa, kostka, willa międzywojenna, szeregówka ceglana, trzy warianty kamienicy (secesyjna, modernistyczna, plombowa), kamienica biurowa, pasaż, market z parkingiem, punktowiec, galeriowiec, apartamentowiec, wieżowiec mieszkalny, skład ceglany, hala lekka, magazyn wysokiego składowania, szkoła, kościół z wieżą. **Korekta H7 odwołana w całości** — diagnoza „połać dachu faluje przez trzy nachodzące schodkowania" była błędna: derywacja daje dokładnie tyle brył, ile opisuje plan, zrzut sprzed zmiany wygląda identycznie, a falowanie widać **tak samo na zieleni terenu i na jezdni**. To rasteryzacja obróconych brył przy voxelu 1 m, czyli wygląd całego silnika — nie zadanie dla M2, tylko dla M11 (profil ciągły z prefabrykatu) albo dla rozmowy o rozmiarze voxela. Trzy zmiany zrobione pod tą hipotezą zostały cofnięte. **Drugą rzecz złapał dopiero pomiar:** pierwsza wersja `Applies::covers` przekazywała pusty klucz jako „filtr pominięty", co go **zaostrzało** zamiast pomijać — udział awaryjnych skoczył z 1,1 % na 7,2 % i wyszło to wyłącznie dlatego, że licznik już istniał; stąd `Option<&str>` w sygnaturze. **Do WP20:** rdzeń miasta nadal wygląda jednorodnie mimo pięciu kandydatów zamiast jednego — wagi dają `kamienica` większość losowań, a reszta różni się detalem, nie bryłą. Jeśli entropia w T13 upadnie, upadnie właśnie tam, a nie na przedmieściach |
| 2026-09-14 | M2 | **M2f/WP18** — operator `Rule::Protrude { face, m, rule }` (balkon, wykusz, ryzalit), lukarny jako `Roof.dormers`, przycinanie wysunięć do działki z wysięgiem nad chodnikiem powyżej skrajni 3,5 m, liczniki wysunięć w `GenerationReport`. Katalog używa obu: `blok.ron` (balkony frontowe), `kamienica.ron` (balkony od podwórza + lukarny). Miasto 8 km, ziarno 7: **8 283 wysunięcia, 19 przyciętych, 154 odrzucone**, zabudowa nadal 41,8 ms | **Siedem korekt (H1–H7), dwie zmieniają zakres albo kryterium.** (H2) **Błąd zastany z M2d: `Face::Front` nie był licem od ulicy.** `footprint_for` brało oś `u` z kierunku odcinka drogi, a kierunek odcinka nie mówi, po której jego stronie leży działka — witryna parteru usługowego patrzyła w oficynę mniej więcej co drugi budynek, i żaden test tego nie widział, bo okna **były**, tylko nie tam. Teraz ulica jest zawsze po stronie `−v`; przy okazji znika rozgałęzienie `front_na_minusie` w cofnięciach. Dla WP18 to warunek konieczny: bez tego `Protrude(Front)` wychodziłby w głąb kwartału. (H1) kryterium WP18 w tabeli przeczyło §5.6c — zakaz bezwarunkowy wysięgu nad pas drogowy odbiera balkon **każdej** kamienicy w pierzei, bo tam `setback_front_m` wynosi 0 z definicji; obowiązuje §5.6c, a granica 1,5 m jest wyprowadzona z geometrii (jezdnia to 60 % pasa, najwęższa klasa `Service` ma 8 m, więc pobocze 1,6 m). (H4) zapas na wysunięcie liczy się od lica **bryły**, nie bieżącego zakresu — inaczej `Inset` na poddaszu zjadałby balkon, choć działka się nie zwęziła. (H5) `Building.aabb` obejmuje wysunięcia, bo inaczej selekcja do kadru w M11 ucinałaby balkony przy krawędzi ekranu. **(H7) Wyszło wyłącznie z obejrzenia miasta:** połać dachu czyta się z odległości dzielnicy jak tektura falista — nie z powodu derywacji (3 zagnieżdżone bryły, zgodnie z planem), tylko z rasteryzacji trzech brył stojących pod kątem do siatki, z których każda ma własne schodkowanie; ta sama klasa co E12, w skali, której E12 nie usunęła. Zrzut z commitu poprzedzającego wygląda tak samo, więc to nie regresja — zadanie i rytm okien (otwór 2,0 m przy filarze 2,6 m) przechodzą do WP19 |
| 2026-09-14 | — | Plan: nowa podfaza **M2f — Różnorodność zabudowy** (`M2f-roznorodnosc-zabudowy.md`, WP18–WP20), wykonywana **między M2d a M2e**; test **T13** w §7 fazy; decyzje **21** i **22** w §9.2 fazy; korekta **E20** w M2d; wpisy **G1–G4** w `M12d-modding.md` (K-18) | Powód: Etap 6 nie był domknięty — blok `Details` z §5.6 (gzyms, balkon, komin, lukarna) nigdy nie wszedł do enuma `Rule` i żaden pakiet nie był jego właścicielem, czyli sytuacja (5) z `K-18`. Trzy ustalenia zmieniają zakres: **(1) brakuje jednego operatora, nie pięciu** — `Comp`, `Offset`, `Extrude` i `Repeat` zapisują dziś pas okienny, gzyms, komin i wejście, a balkonu, wykusza i ryzalitu nie da się wcale, bo `Offset` poszerza zakres z **każdej** strony; stąd `Protrude { face, m, rule }` i `dormers` w `Roof`. **(2) Granicą detalu między M2 a M11 jest rozmiar voxela, nie rodzaj detalu** (decyzja 21): przy voxelu 1 m w poziomie gzyms 0,4 m nie ma się gdzie zrasteryzować, więc detal subwoxelowy należy do M11 jako prefab `Place`, a detal od ~1 m w górę zostaje w gramatyce — uściślenie rozstrzygnięcia 9.1/10. **(3) M2f leży przed M2e**, bo T10 jest wg korekty F6 zadaniem kalibracyjnym, a jedną z jego trzech dźwigni jest `massing` gramatyk: katalog rozszerzony po kalibracji znaczy kalibrację drugi raz. M2e zostaje ostatnia i nadal zamyka bramki 1–7. Różnorodność dostaje próg zamiast opinii (T13: identyczne sygnatury w promieniu 60 m < 15 %, entropia gramatyk w dzielnicy mieszkaniowej ≥ 1,8 bita), bo `K-18` pkt 3 zabrania kryteriów niemierzalnych. W przód do M12d: `data/grammar/` wchodzi do `ModManifest.data_dirs` imiennie, a `GrammarSet::load_dir` bierze dziś jedną ścieżkę i jest to jedyna rzecz dzieląca ten katalog od moddowalności |
| 2026-09-14 | M2 | **Podfaza M2d** — Etap 6 w całości: język gramatyki architektury w `data/grammar/` (12 plików, zamknięty zbiór operatorów, walidator: materiały, cykl `Ref`, budżet węzłów), silnik derywacji na drzewie zakresów (`Split`/`Repeat`/`Inset`/`Offset`/`Extrude`/`Comp`/`Choice`/`If`/`Ref` + `Foundation`/`Floors`/`Roof`), posadowienie z niwelacją parceli (R9), wnętrza logiczne (`Unit`, `Workplace`, `WageBand`), wycena gruntu `pass_1` (WP15a, przeniesiona z M2e korektą D1), warstwa transportowa w voxelach i **miasto w kliencie `magnat`** (`--no-city` wraca do widoku M1). Metropolia: 14,5 tys. budynków, 157 tys. lokali, 264 tys. stanowisk, 600 tys. komend voxelowych w **1,7 s** wobec budżetu 28 s; widok dzielnicy 1 249 FPS przy progu 60 | **Dziewiętnaście korekt planu (E1–E19), osiem zmienia zakres albo kryterium.** Najważniejsze: (E1) **`engine/voxel` dostaje bryłę zorientowaną** (`Obb3`, `EditOp::Prism`, `CarveShape::Prism`) — §5.6b zakładał `Terrace` na `IAabb3`, a ulica biegnie pod dowolnym kątem: prostopadłościan osiowy obejmuje przy 45° √2 razy szerszy pas, czyli sąsiednią działkę, a rozbicie bryły na komendy-wiersze daje przy 50 tys. budynków miliony komend zamiast tysięcy; (E3) **komendy stosowane leniwie przez nowy `EditIndex`, nie przez `EditOverlay`** — nakładka trzyma każdy zmieniony voxel, a miasto to setki milionów voxeli wobec 600 tys. komend; (E7) **działka o froncie < 6 m nie dostaje gramatyki, także awaryjnej** — bez tej bramki udział fallbacku mierzył ziarnistość podziału pasowego (6,4 %), a nie luki w katalogu, czyli kryterium „< 2 %" badało co innego, niż nazywa (po bramce 0,4–1,1 %); (E5) **`Workplace` powstaje już tutaj** z `data/jobs/roles.ron`, z `site: None` do dowiązania w M2e — inaczej „budynki mają stanowiska pracy" z kryterium podfazy i kontrakt `wage_band` dla M3 byłyby puste; (E6) **zieleń i wydobycie zostają bez zabudowy** — park, w którym każda działka dostaje budynek awaryjny, przestaje być parkiem; te obiekty to `SiteSeed` Etapu 7; (E13) kryterium `avg(OldTown) > avg(Suburb)` zastąpione porównaniem grup (rdzeń wobec obrzeża), bo `OldTown` wymaga dominacji najstarszego pierścienia, a ten ma 3 % powierzchni i na większości ziaren nie ma ani jednej takiej dzielnicy — porównanie „0 vs 0" jest spełnione tożsamościowo. Przy okazji zamknięta obietnica korekty C9 z M2c: `District.pop_capacity` liczone z faktycznych mieszkań. **Trzy rzeczy wyszły wyłącznie z obejrzenia miasta i żaden test ich nie widział:** dachów nie było w ogóle (reguła `Roof` dostaje zakres o zerowej wysokości i wypadała na wspólnym warunku „zakres zdegenerowany", a w liczbach wszystko się zgadzało), dach spadzisty wychodził kratownicą (przy voxelu **1 m w poziomie** — §5.6b mówi o 0,5 m, co jest wymiarem pionowym — dwie zagnieżdżone bryły pod kątem rasteryzują się z własnym schodkiem i schodki się mijają), a okna co 3 m czytały się jako sztruks. Budżety §7 dla liczby budynków (14,5 tys. wobec 49,5 tys.) i mieszkań (117 tys. wobec 167 tys.) nie są trzymane — idą w ślad za liczbą parcel, którą M2c zgłosił jako rozjazd gęstości; T10 jest zadaniem kalibracyjnym M2e (F6) |
| 2026-09-14 | — | Plan: kontrakt **`K-18` — „poprawki wędrują w przód"** w dokumencie 00 i reguła operacyjna w `CLAUDE.md`. Na jego podstawie poprawione dokumenty **M2d** (dziewięć zmian D1–D9) i **M2e** (D1, D10) oraz §4, §9.1, §9.2 i §10 dokumentu fazy | Reguła: jeśli praca nad fazą X pokazuje, że dokument fazy następnej jest błędny, nieścisły albo niedopowiedziany, poprawiamy go **w tej samej zmianie**, w tabeli „Zmiany wpisane po MX" — bo plan jest wart tyle, ile jest prawdziwy w chwili, gdy ktoś go otwiera, a wiedzę ma ten, kto ją właśnie zdobył. Pięć znalezisk zmieniających zakres albo kryterium: (D1) **`pass_1` wyceny gruntu przeniesiony z M2e do M2d jako WP15a** — zasila dobór gramatyki i liczbę kondygnacji, a leżał za nimi na ścieżce krytycznej, więc gramatyka czytałaby wartość gruntu równą zeru; (D2) **nowy WP12b: warstwa transportowa w voxelach** — §6 fazy obiecuje zapis „brył budynków, nasypów, mostów", ale żaden pakiet nie był właścicielem jezdni, przez co „miasto w voxelach" miałoby budynki nad nietkniętym terenem; (D3) **WP12b podpina `generate_city` do `tools/magnat`** — klient graficzny woła dziś tylko `generate()`, a M2d jest jedyną podfazą, w której da się miasto obejrzeć przed M2e; (D6) **semantyka `RoadFlags::NO_HEAVY` do poprawienia** — flaga jest dziś ustawiana dla każdej klasy z jakimkolwiek limitem tonażu, także dla kolektora (40 t), przez co test T4 („rampa przy drodze bez `NO_HEAVY`") jest niespełnialny z powodu progu w jednej linii, a nie z powodu urbanistyki; (D5) operator `Instance` wypada z M2, bo `engine/voxel` nie ma operacji stawiania prefabrykatu. Przy okazji rozstrzygnięte dwie decyzje z §9.2, obie „czekające na M1": zapis voxeli idzie przez `EditQueue`, nie przez `ChunkWriter::stage` (M1 zamknął to inaczej, niż proponował M2 — i lepiej), a wszystkie pięć zgłoszonych rozszerzeń `TerrainQuery` zostało przyjętych i jest używanych |
| 2026-09-14 | M2 | **Podfaza M2c** — Etapy 4 i 5: osiem pól wpływu na siatce 16 m, pierścienie epok z `data/epochs/`, strefowanie kwotowe z wagami z `data/zoning/weights.ron` i kwotami z profili, kolej towarowa (WP5b) jedną Dijkstrą od bramy ze scalaniem prefiksów, dzielnice Voronoi po kwartałach z doginaniem do arterii i nazwami z `data/names/districts_pl.ron`, sieć ulic lokalnych i podział pasowy na parcele. `headless preview --field zones` + `--inspect x,y`. Metropolia: 1 086 kwartałów, 15 681 parcel, 33 dzielnice, 73 km torów w **1,25 s** wobec budżetów 4 + 3 + 6 s | **Siedemnaście korekt planu (C1–C17) w dokumencie podfazy, pięć zmienia zakres albo kryterium.** Najważniejsze: (C13) **kryterium T9/WP7 „±3 pp." obowiązuje od miasta 120-tysięcznego** — kwartał trafia do strefy w całości, więc odchylenia mniejszego niż udział największego kwartału nie da się osiągnąć żadnym przydziałem (zmierzone: metropolia 0,4 pp. przy progu ziarnistości 1,2; miasto 40-tysięczne 6,4 przy progu 10,4); (C6) **ściana grafu > 30 ha nie jest kwartałem** — dwie drogi wylotowe i rzeka zamykają setki hektarów pola, a jedna taka ściana bywa warta 84 % powierzchni małego miasta i sama unieważnia wszystkie kwoty; (C5) weta terenowe przekalibrowane — przy progu `flood_max` 55 generator wykreślał **92 ze 96** kwartałów miasta nad rzeką jako `Undevelopable`, bo miasta nadrzeczne stoją w terenie zalewowym z definicji; (C12) bufor 400 m wokół przemysłu ciężkiego realizowany **zamianą** kwartałów, nie przeniesieniem — przeniesienie oddawało kwotę strefy, której nikt już nie odbierał, i 5 % wsiąkało w R1; (C8) **WP9 przed WP8**, bo przypisanie dzielnic przenumerowuje kwartały, a po powstaniu parcel trzeba by przestawiać dwie tablice zamiast jednej. Przy okazji wyszło, że przydział kwotowy nie może rozstrzygać ogona punktacją: ostatnie kwartały są większe od każdej reszty kwoty i trafiały wszystkie do jednej strefy (R1 21,7 % przy kwocie 11,3 %), podczas gdy strefy o niskiej punktacji nie dostawały **nigdy nic** (Logistics: 86 kandydatów, 0 kwartałów) — rozstrzyga największa niedobrana kwota. Kolej po korekcie C14 skróciła się ze 197 km do 68 km wobec ~45 km z budżetu §7; reszta to trasa od bramy na krawędzi mapy, do kalibracji w M2e. **Dwie rzeczy wyszły wyłącznie z podglądu, nie z testów:** parcele rysowane samym wypełnieniem zlewały się w plamę (kwartał podzielony na dwadzieścia działek wyglądał jak niepodzielony — stąd obrysy działek i kadrowanie `--crop`), a próg 30 ha z C6 wyrzucał z kwot także ściany leżące w **środku** miasta, przez co centrum wypełniały parki wielkości dzielnicy; w liczbach wyglądało to niewinnie |
| 2026-09-14 | M2 | **Podfaza M2b** — Etap 3 bez kolei: `CityPlan` i `GenerationReport`, wybór środka miasta, bramy z profili `data/zoning/profile_*.ron`, L-system arterii z wzorcami z `data/districts/patterns.ron`, mosty/tunele/nasypy z `crossing_cost`, domknięcie sieci (przycinanie, największa składowa, CSR, latarnie) i kwartały obchodem półkrawędzi. `headless preview --field roads`. Metropolia: 9,7 tys. segmentów, 517 km, 1148 kwartałów w **217 ms** wobec budżetu 4 s | **Dwadzieścia cztery korekty planu (B1–B23) w dokumencie podfazy, pięć zmienia zakres albo kryterium.** Najważniejsze: (B1) **kolej towarowa przeniesiona do M2c jako WP5b** — jej trasy prowadzą do klastrów stref, a strefy powstają w WP7, więc kryterium „bocznica ≤ 1,2 km" nie dało się w M2b nawet sprawdzić; (B3) ograniczenie nachylenia mierzy **niweletę**, nie `slope_at` — to dwie różne wielkości i przy kryterium z planu autostrada była odrzucana na pierwszym segmencie w każdym regionie (100 % odrzuceń, sieć o zerowej długości); (B17) kryterium WP6 zastąpione **niezmiennikiem Eulera** (`orbity = E − V + 2·C`), bo sformułowanie z planu jest spełnione tożsamościowo — pas drogowy liczymy jako różnicę ściany i kwartału, więc test nie mógłby nigdy upaść; (B12) minimalny kąt sprawdzany także w węźle **początkowym** — bez tego dwie propozycje z tego samego węzła dawały nakładające się krawędzie, niewidoczne dla testu przecięć (wyznacznik zeruje się na współliniowości), a scalające sąsiednie ściany: 154 orbity wobec 164; (B14) węzeł nie ma prawa stanąć w wodzie — bez tego miasto portowe rozrastało się mostami w morze, 224 mosty na mieście 40-tysięcznym. Gęstość metropolii wychodzi ~60 % planu (36 km² wobec 70) — kwartał ma średnio 3 ha, czyli powyżej `max_block_area` większości stref, więc różnicę dobierze podział siecią lokalną w WP8; kalibracja należy do T9/T10 w M2e. **Zastane, nie moje:** `bench_generate_4km` z M1 przewraca się pod `cargo test --workspace` w profilu `debug` (zegarowy budżet 2,5 s przy 170-sekundowej sąsiedniej suicie); osobno przechodzi |
| 2026-09-14 | M2 | **Podfaza M2a** — crate `engine/spatial` w całości: `GridSpec`/`CellId`/`morton2`, `CsrGrid`, `CategoryGrid`, `DynamicGrid`, `ParcelTree`, `ScalarField` z `multi_source_dijkstra`, zapytania wsadowe. Parity z przeglądem zupełnym na 10 tys. zapytań (promień, prostokąt, kNN, wsad, kategorie), przebudowa gridu bit-identyczna przy 1/2/4/8 wątkach i w dwóch przebiegach (D3) | **Siedem korekt planu w §5.1 dokumentu podfazy (A1–A7), jedna istotna:** budżet „1 mln zapytań R = 250 m po 300 tys. encji ≤ 180 ms" wycenia liczbę komórek, a przemilcza liczbę trafień — przy R = 250 m zapytanie zwraca 230 encji na mapie 16 km i 860 w obszarze zurbanizowanym. Zmierzone 111 ms (✓) i 275 ms (✗) tą samą strukturą; drugi przypadek leży na przepustowości L3, nie na arytmetyce (zawężenie skanu do cięciwy koła ścięło 40 % kandydatów i nie zmieniło czasu). Wniosek dla M3–M5: do sąsiadów `k_nearest` (1,09 µs), do ofert `CategoryGrid` (0,28 µs), nie promień po wszystkich encjach. Pozostałe budżety z zapasem: punkt→parcela 0,32 µs (limit 2), przebudowa 400 tys. 1,86 ms (limit 3), Dijkstra 1 mln komórek 35 ms (limit 400), próbka pola 7,7 ns (limit 30). `ParcelTree::at_point` bierze predykat zamiast `&PolyArena` — `engine/spatial` nie może zobaczyć typu z `sim/world` |
| 2026-09-14 | M1 | Domknięcie bramek fazy: macierz 32 seedów × 5 regionów zatwierdzona w repo, `deposit_ledger` na 10 tys. losowań, karta inspekcji terenu (`--inspect x,y` i prawy przycisk w oknie) | **Korekta wobec §7.1:** macierz hashy obejmuje jeden rozmiar (4 km), a nie cztery — 640 generacji to dwie godziny na przebieg i tabela, której nikt nie zatwierdzi świadomie; wpływ rozmiaru pilnuje `seed_i_region_zmieniaja_swiat`. Przy okazji wyszła regresja budżetu: agregat LOD3 urósł do 1,79 ms, bo materializacja liczyła szum detalu i rozmycie biomu na każdą próbkę — detal pomijany od LOD2 (amplituda 0,35 m wobec voxela 2 m), biom próbkowany na siatce 8 m jak geologia; z powrotem poniżej 1,5 ms przy niezmienionym błędzie agregatu (1 voxel LOD0) |
| 2026-09-14 | M1 | X1: benchmarki `criterion` dla generacji, meshingu i agregacji LOD (klucze `m1-1`…`m1-4` w linii bazowej), scena pomiarowa §7.4 z pięcioma etapami i metryką 1 % low, asercje `bench_priority_flood_16km` i `mem_voxel_budget`, `dep_isolation` i raport budżetów jako artefakt w CI | Przelot 60 s (RTX 4070 Ti, 1600×900, region rzeczny): widok miasta 860 FPS / 1 % low 114, dzielnica 1272 / 816, ulica 1537 / 974, przelot 2 km 1081 / 259, **0 zacięć > 33 ms** wobec progu ≤ 2 na 60 s. P4 na 16 km: 1008–1234 ms wobec 1500 ms. Przy okazji naprawione `cargo bench --workspace`, które od M0 przewracało się na bibliotekach uruchamianych jako bench target — bramka regresji D-8 nie działała |
| 2026-09-14 | M1 | R5, R6: przebieg wody (fresnel, mgła głębinowa z bufora głębi, fale, miękki brzeg), siatka dalekiego terenu z mapy 4 m, bufor HDR z ekspozycją i ACES w post-processingu, FXAA, `TerrainOverlay` i dziewięć nakładek pod `F3` | **Korekta wobec §1:** nakładka „akumulacja spływu" pokazuje rząd Strahlera, bo sama akumulacja nie jest stanem trwałym — cztery bajty na komórkę to 64 MB na mapie 16 km wobec całego budżetu 60 MB z §5.9. Przy okazji wyszedł błąd, którego nie widać w testach: `struct Frame` w `sky.wgsl` został przy układzie sprzed kaskad i czytał barwę nieba z macierzy światła (niebo wychodziło białawe) — teraz strzeże tego test porównujący deklaracje we wszystkich shaderach. Przelot 2 km z pełnym potokiem: 183 FPS średnio, GPU 0,29 ms na wszystkie przebiegi (p95 0,63 ms) |
| 2026-09-14 | M1 | R3, R4: cienie kaskadowe (4 kaskady, sfera otaczająca + zatrzask do texela, PCF 3×3 na sprzętowym porównaniu), przypisanie świateł do froxeli 16×9×24 w compute, `ClusterOccupancy` w devtools | Cienie 0,18 ms (p95 0,29) w scenie z pierścieniami LOD wobec limitu 2 ms; 4096 świateł: przypisanie 0,26 ms (p95 0,52) wobec 0,4 ms — mediana w budżecie, ogon poza nim, bo pomiar obejmuje klatki z materializacją chunków. Trzy błędy wyłapane przez podgląd, nie przez testy: pass cienia nie może widzieć własnej mapy w grupie wiązań, kafel klastra we fragmencie liczy się po **odbitej** osi Y, a warstwa klastra po **głębokości widoku**, nie po odległości od oka |
| 2026-09-13 | M1 | R1, R2, C1: pomiar czasu GPU per pass (timestamp queries), rysowanie pośrednie `multi_draw_indexed_indirect` ze ścieżką zapasową, tryby kamery z przejściami bez przeskoku | 3380 chunków LOD0: GPU p95 2,62 + 1,59 ms (limit 6 ms); obie ścieżki rysowania dają zrzut **identyczny bit w bit** (SHA-256); przelot 2 km bez zacięć > 33 ms; `tools/voxelview` usunięte zgodnie z K-3 |
| 2026-09-13 | M1 | Poprawki ujawnione przez podgląd: tafla wody nie powstawała w meshu, chunki różnych LOD nadpisywały się w rendererze, granica biomu biegła po siatce klimatu 256 m, `column_at` pomijał pokrywę biomu | Komórki jezior przeszły na RLE (6 B/komórkę → 8 B/odcinek): stan trwały 16 km z 61,4 MB na 53,0 MB (region górski, 1,54 mln komórek jezior), znów pod limitem 60 MB z §5.9 |
| 2026-09-13 | M1 | V2, V3, V4: greedy meshing z voxelowym AO i formatem wierzchołka 8 B, agregacja LOD, `VoxelWorld` z pierścieniami LOD, histerezą, budżetem pamięci i dwufazową kolejką edycji | **Trzy korekty planu, wszystkie z pomiarem:** (1) budżet 900 quadów na chunk dotyczy średniej (zmierzone 477–664), najgorszy chunk sięga 2564 — i wypada na **nizinie**, nie w górach, bo chunk przy jeziorze ma dwie niezależne powierzchnie; arena GPU nadal w budżecie (250 MB z 768 MB); (2) agregację LOD wykonuje **źródło**, nie `engine/voxel` — nadpróbkowanie `8^lod` po stronie silnika dało 305 ms na chunk LOD3 wobec budżetu 1,5 ms, próbkowanie po stronie `sim/world` mieści się w budżecie przy błędzie powierzchni 1 voxela LOD0; (3) porządek stosowania edycji wyprowadzany jest **z treści komendy**, nie z `EditSeq` — numer nadawany przy równoległym `push` nie jest deterministyczny, więc sortowanie po nim łamałoby `edits_order_invariant`. Geologia próbkowana co 8 m (i tak samo w `column_at`, żeby inspektor i wykop pokazywały to samo): materializacja chunka 0,39–0,50 ms, meshing 0,64–0,68 ms |
| 2026-09-13 | M1 | W5 i W7: materializacja 1 m (Catmull–Rom + szum detalu + wcięcie koryta z wektora), `Terrain` jako implementacja `ColumnSource` i **całego** kontraktu `TerrainQuery` z §6.1 wraz z pięcioma skrótami M2 | `column_matches_voxels` zielony po poprawce granic warstw: przedziały voxeli muszą pokrywać się co do jednostki z regułą `ColumnStack::material_at`. Szum detalu jest wygaszany w korycie — przy amplitudzie 3,5 dm i korycie 3 dm zjadał całe wcięcie |
| 2026-09-13 | M1 | W6: przebiegi P10–P12 — temperatura z szerokości, wysokości i kontynentalności, adwekcja wilgoci z opadem orograficznym i cieniem opadowym, biomy Whittakera z nadpisaniami terenowymi, żyzność i poziom wód gruntowych | Cień opadowy zdaje w 8/8 światach po dwóch poprawkach modelu: pełna podstawa regionalna zamiast 55 % (region rzeczny wychodził stepem) i ubytek wilgoci z wysokością bezwzględną. Wiatr w regionie górskim zmieniony na południowy, bo pasmo biegnie z południa na północ |
| 2026-09-13 | M1 | W4: geologia analityczna (`data/geology/layers.ron`, 0 B pamięci), pole odległości od wody 16 m, złoża z wagami `region × profil × epoka` i całkowitoliczbowym bilansem masy | Test własnościowy masy złóż zielony (00 §6). `data/` odnajdywane przez `assets::data_dir()` zamiast ścieżek względnych w każdym wywołaniu |
| 2026-09-13 | — | Plan: fazy M2–M12 rozbite na **55 podfaz** we własnych plikach `MX<litera>-*.md`; §4 i §5 dokumentów faz stały się tabelami kierującymi, numeracja `5.x` zachowana. Kontrakt `K-17` w dokumencie 00, procedura startu podfazy w `CLAUDE.md` | Zakres, kontrakty (§6), testy (§7), ryzyka i decyzje otwarte zostają na poziomie fazy — dzielony jest tylko plan wykonawczy. Bramki 1–7 nadal zamykają się na poziomie fazy. M0 i M1 nierozbite: M0 zamknięty, M1 w trakcie |
| 2026-09-13 | M1 | W3: hydrologia w całości — priority-flood + ε, D8 z porządkiem topologicznym, erozja stream-power (Braun–Willett, n = 1) z równoległością po zlewniach, dyfuzja zboczowa, erozja termiczna, klasyfikacja wód, rząd Strahlera, geometria hydrauliczna, wektorowa sieć koryt | **Pięć odstępstw od liczb w planie, każde z uzasadnieniem w kodzie lub w `data/geology/erosion.ron`:** (1) `dt` 150 lat zamiast 5000 i `K` rząd wyżej — przy planowych wartościach nachylenie koryta w stanie ustalonym wychodzi 20 %, czyli 800 m różnicy na rzece w świecie o zakresie 256 m; (2) dyfuzja 0,001 zamiast 0,01 — zasięg wygładzania musi zostać poniżej komórki 4 m, inaczej kasuje sieć dolin wyciętą przez stream-power; (3) próg rzeki 0,1 km² zamiast 0,25 — przy planowym mapa 4 km ma sieć rzędu 2, czyli bez rozgałęzień; (4) erozja pracuje na powierzchni **wypełnionej**, a zagłębienia wracają pomniejszone o wcięcie progu odpływowego — bez tego misy bezodpływowe w ogóle nie były rzeźbione; (5) regiony śródlądowe dostały regionalne nachylenie `tilt_m`, bez którego ridged multifractal daje jeziora na jednej trzeciej mapy. Budżety §5.9 dla 16 km: generacja 8,3–8,9 s (cel ≤ 10 s), P4 1,2 s (cel ≤ 1,5 s), stan trwały 50–59 MB (limit 60 MB), `.mgw` 14–21 MB (limit 25 MB). P6 7,5 s wobec prognozy 5–6 s — ryzyko R3 zmaterializowane w przewidzianej skali, mieści się w limicie CI |
| 2026-09-13 | M0 | `core::det_math` rozszerzony o `sin`/`cos`/`tan` (redukcja Cody'ego–Waite'a + jądra fdlibm), tabela referencyjna 60-cyfrowa i osobny złoty odcisk T-D9 dla trygonometrii | Dopisane przez M1 wg procedury z 00 §K-6: roczny cykl `ClimateCell` i kąt usypu wchodzą do stanu trwałego, więc nie mogły zostać po stronie renderu. Odcisk M0 celowo **nietknięty** |
| 2026-09-13 | M1 | W2: przebiegi P1–P3 (maska lądu i profil brzegowy, baza wysokości z domain warpingiem, pola `U` i `K`); podgląd PNG w headless | Budżet §5.9 dla 16 km: P1+P2 = 178 ms wobec celu 400 ms. Dwie poprawki wyszły z podglądu, nie z testów: waga oktaw w ridged zapadała się po pierwszej oktawie, a szum wartościowy zostawiał kratownicę — stąd przejście na szum gradientowy |
| 2026-09-13 | M1 | W1: `WorldGenParams` z walidacją, `Grid2`, potok 12 nazwanych przebiegów z profilem czasu, `WorldGenReport`, `StreamId` 100–110, `.mgw`, `headless generate/verify/preview` | Decyzje D1–D11 przyjęte wg propozycji domyślnych M1 (D3 potwierdza rezerwację strumieni w 00 §K-4) |
| 2026-09-13 | M1 | V1: `engine/voxel` — chunk 32³, paleta lokalna, `Uniform`/`Rle`/`Dense`, `ColumnSource`, rejestr materiałów z `data/materials/` | Round-trip Dense→Rle→Dense bit w bit; `Uniform` w 32 B |
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
