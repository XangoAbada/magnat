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
- [~] **6. Kontrakty** — dostarczone i używane wewnątrz fazy: `TerrainQuery` (wszystkie pięć
  grup z §6.1, konsumowane przez inspektor i nakładki), `ColumnSource`, `sim-snapshot`,
  `RenderGraph::register` + `GraphSlot`, `TerrainOverlay`, `ClusterOccupancy`, `EditReport`.
  Bramka domknie się dopiero wtedy, gdy **M2 ich faktycznie użyje** — tego M1 nie jest
  w stanie sprawdzić sam za siebie
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

- [ ] **M2a** Indeksy przestrzenne — `M2a-indeksy-przestrzenne.md` (WP1, WP2)
- [ ] **M2b** Szkielet transportu — `M2b-szkielet-transportu.md` (WP3, WP4, WP5, WP6)
- [ ] **M2c** Strefy, parcele, dzielnice — `M2c-strefy-parcele-dzielnice.md` (WP7, WP8, WP9)
- [ ] **M2d** Zabudowa — `M2d-zabudowa.md` (WP10, WP11, WP12)
- [ ] **M2e** Gospodarka bazowa i wycena — `M2e-gospodarka-bazowa-i-wycena.md` (WP13, WP14, WP15, WP16, WP17)

Bramki fazy:

- [ ] 1. Pakiety robocze · [ ] 2. Determinizm · [ ] 3. Własnościowe · [ ] 4. Budżety
- [ ] 5. Wyjaśnialność · [ ] 6. Kontrakty · [ ] 7. Decyzje otwarte
- [ ] **Faza ukończona** — artefakt: miasto do oglądania, nakładka wartości gruntu

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
