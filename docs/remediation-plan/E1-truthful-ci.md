# E1 — CI i bramki mówią prawdę

**Po co.** Dziewięć testów w `sim/world/tests`, test domknięcia M10 i test kopalni nie biegną
nigdzie, choć ich `#[ignore]` twierdzi, że „CI uruchamia jawnie". `bench_guard` przepuszcza
benchmark, który zniknął. G1 jest zawsze zielona. `macro-kernel` jest czerwony na `master`.
Dopóki tak jest, żadna naprawa z E2–E7 niczego nie dowodzi.

**Wejście.** Nic.

**Pomiar zamknięcia.** Lista wszystkich `#[ignore]` w repozytorium z kolumną „job w `ci.yml`
albo otwarty punkt `N`" — pusta kolumna nie występuje. Wszystkie joby CI zielone na `master`.

**Uwaga o zieleni.** Test dopięty do CI może okazać się czerwony (np. `consistency.rs` nie
biegł od M2e). Wtedy nie wyłączamy go z powrotem: dostaje `#[ignore = "N<etap>.<nr>: …"]`
z identyfikatorem punktu, który go naprawi, a strażnik z `N1.1` pilnuje, że punkt jest otwarty.
Jeśli czerwień nie ma jeszcze punktu, zakładamy go w odpowiednim etapie („Znalezione po drodze").

## Punkty

- [x] **N1.1** strażnik `#[ignore]` — `M2#1`, `R2#1`, wzorzec „`#[ignore]` bez joba"
  - *Naprawa:* dopisać do `scripts/plan_guard.py` (już parsuje `ci.yml`) sprawdzenie:
    każdy `#[ignore]` w `*/tests/*.rs` i `*/src/*.rs` jest uruchamiany przez któryś krok
    `ci.yml` (`--include-ignored` / `--ignored` na tym pliku testów albo filtr po nazwie)
    **albo** jego powód zawiera `N<e>.<n>` otwartego punktu z `docs/remediation-plan/`.
  - *Test:* self-test strażnika z trzema przypadkami: test z jobem, test z otwartym punktem,
    test bez niczego (błąd).
  - *Zrobione:* reguła 6 `plan_guard`. Trzecia droga (decyzja właściciela z 22.09): powód
    zaczyna się od `narzędzie:` albo `pomiar:`, a ciało testu nie ma `assert` — narzędzie
    ręczne, które niczego nie obiecuje. Parser `ci.yml` zna `-p`, `--test`, `--lib`, `--skip`,
    filtr po nazwie, komentarze powłoki i potoki; osiem przypadków `--self-test`.
    `--list-ignored` wypisuje tabelę do pomiaru zamknięcia.
    Na kodzie sprzed zmiany: **36 ze 100** `#[ignore]` bez właściciela. Rozdzielone:
    33 → `N1.2:` (dopięcie do CI), `population.rs:288` → `N4.1:` (kolejka DES),
    `det_math_accuracy.rs:240` → `narzędzie:`, `reach.rs:380` → `pomiar:`.
    `reach.rs:342` i `cch.rs:1264` mają asercje budżetu czasu, więc są testami, nie
    narzędziami — idą do `N1.2`.

- [x] **N1.2** dopiąć do CI testy, które twierdzą, że w nim są — `M2#1`, `R2#1`, `M10` WP10.16, `M11` R2-WP17
  - `sim/world/tests/consistency.rs` (T1–T13, D1, D2, D5, budżet, WP13, WP16 — 7 testów),
  - `sim/world/tests/lsystem_split.rs` (dowód `R2-WP21`),
  - `tools/headless/tests/m10_domkniecie.rs` (dwa przebiegi = ten sam ciąg hashy),
  - `tools/headless/tests/full_city.rs:162` (kopalnia na złożu).
  - *Gdzie:* job `determinism` (nocny), tak jak pozostałe testy `sim/world` w release.
  - *Dowód:* zielony przebieg nocny albo jawne `#[ignore = "N…"]` według uwagi wyżej.
  - *Zrobione:* 33 testy z `N1.2:` (strażnik `N1.1`) — lista szersza niż w audycie:
    doszły `labor_city.rs` (5), `firms_city.rs` (4), `blackout.rs` (2), reszta `full_city.rs`,
    `nav_build.rs:709`, `school.rs:356`. Nowy krok `E1 — testy…` w `determinism`; budżety
    czasu (`cch.rs:1264`, `reach.rs:342`) w jobie `budzety`. Lokalnie w release (Windows):
    wszystkie zielone w 7,5 min poza **jednym**: `macierz_32_ziaren_x_4_profile` — 7 ze 128
    światów bez źródła `raw_water` (T11). Dostał `N4.15` w E4 i `--skip` po nazwie.
    Przy okazji reguła 6 dostała drugą stronę: test z otwartym punktem N, który job i tak
    uruchamia, jest błędem (zapomniany `--skip` = czerwone CI z powodu, który ma właściciela).
    Zielony bieg w CI — przy pomiarze zamknięcia etapu (Actions zablokowane od 21.09).

- [ ] **N1.3** `m3_century` w nocnym CI i progi, które wymusza — `M3#4`
  - Runner `tools/headless/src/century.rs:167-169,236-241` drukuje populację, ale nie
    sprawdza §7.2 („nigdy < 1000", „nigdy > 3×"). Wersja testowa `wp7_…` sprawdza `min > 0`.
  - *Naprawa:* kod wyjścia ≠ 0 przy przekroczeniu progu; krok w nocnym jobie.

- [x] **N1.4** `bench_guard`: zniknięty benchmark to błąd — `R2#2`
  - `scripts/bench_guard.py:75-77`: wiersz `BRAK … zniknął` nie ustawia `kod = 1`.
  - *Test:* przypadek w `--self-test` sprawdzający **kod wyjścia**, nie tylko wiersz.
  - *Zrobione:* `kod = 1` przy zniknięciu; przypadek „sam zniknięty benchmark wywraca bramkę"
    padał przed poprawką (`bench_guard --self-test: BŁĄD`, kod 1).

- [ ] **N1.5** `macro-kernel` zielony na `master` — `R2#3`
  - `scripts/macro_kernel_guard.py` zgłasza trzy trafienia w
    `sim/macro/src/step/labor.rs:110,190,229` (poz. 84 wykazu R2 mówi o dwóch).
  - *Naprawa:* przenieść te trzy obliczenia do wywołań `kernel`. Jeśli zmienia to wynik
    makro, to i tak jest poprawne (makro ma liczyć jak mezo) — wynik mierzy E5.

- [ ] **N1.6** bramki balansatora bez danych nie są zielone — `M5#2`, `R2` WP24
  - G1 (`tools/balansator/src/gates.rs:247-253`): `yoy_bp` wymaga 13 zamkniętych miesięcy,
    profile CI liczą 120 i 365 dób, `all()` na pustym zbiorze daje `true`. Ten sam kształt:
    „6 miesięcy deflacji" w G3 przy 120 dobach, G11 zawsze pominięta w profilu `ci`.
  - *Naprawa:* bramka bez danych ma trzeci stan `BRAK DANYCH`, widoczny w raporcie.
    W profilu nocnym `BRAK DANYCH` jest błędem; w profilu `ci` jest dopuszczalny tylko dla
    bramek wypisanych z nazwy w konfiguracji profilu.
  - **Decyzja N1.6-a:** wydłużyć nocny profil do ≥ 420 dób, żeby G1 miała dane?
    *Domyślnie:* tak, nocny 8 × 450 dób — o ile czas nocnego joba zostaje poniżej limitu
    GitHub Actions; jeśli nie, 4 × 450.

- [x] **N1.7** `struct_guard` — dziury w bramce — `R1#3`, `R1#5`, `R1#6`, `R1#8`, `R1#9`, `R1#10`, `R2#5`
  - `SyntaxWarning` od `"\|"` w `struct_guard.py:277-278` → `r"\|"`.
  - `WYJATKI_PLIK` zwalnia metrykę bezwarunkowo (`:524`); ma zamrażać liczbę jak `REJESTR`
    (graph.rs urósł 1064 → 1084 bez sygnału).
  - Metryka „`mod.rs` z własnym kodem" liczy tylko `name == "mod.rs"` (`:459`); ma liczyć
    też rodzica `foo.rs` obok katalogu `foo/` (market.rs 840, oracle.rs 878, build.rs).
  - `--all --json` zwraca kod przed `sprawdz_rejestr` (`:782-783`).
  - Kontrola duplikatów numerów pomija wiersze z ✅ (`:324-329`; numer 41 użyty dwa razy).
  - Ścieżka z kolumny „Plik" rejestru nie jest sprawdzana (poz. 35 i 37 wskazują pliki,
    których nie ma).
  - Data przeglądu nie wygasa (`:249,332-337`): data z przeszłości ma być błędem.
  - „Żywy adresat" to dowolna wzmianka otwartej fazy — pozycje 47, 49, 51 są żywe tylko
    dlatego, że M10a stoi na `[~]`. Adresat ma być wskazany w kolumnie adresata, nie w treści.
  - *Test:* po jednym przypadku `--self-test` na każdą kreskę.
  - *Skutek:* bramka zapali się na pozycjach, które dziś przechodzą — ich poprawienie to `N8.3`,
    nie ten punkt. Tu wystarczy, że bramka mówi prawdę; do czasu `N8.3` pozycje dostają
    datę przeglądu (dziś + 30 dni).
  - *Zrobione:* wszystkie osiem kresek, każda z przypadkiem `--self-test` (11 przypadków
    rejestru, 2 zamrożenia wyjątku, martwe wyjątki, `foo.rs` obok `foo/`, kompilacja bez
    ostrzeżeń, `--all --json`). Rejestr dostał kolumnę **„Adresat”**: faza, punkt `N`
    albo data przeglądu. Faza liczy się tylko wtedy, gdy któryś jej dokument wymienia plik
    pozycji — inaczej „R3” w prozie znów robiłby za adresata 54 pozycji.
    Migracja: 60 otwartych pozycji → `N8.3 · przegląd 2026-10-22`, nikt nie został przy
    fazie (jedyny kandydat, M10 przy poz. 47, żyje tylko przez `[~]` M10a). Poz. 35 i 37
    wskazują istniejące pliki (`lsystem/constrain.rs`, `reason/firm.rs`), zamknięta 41 →
    71. Nowa metryka zapaliła 10 plików (`oracle.rs` 864 … `renderer.rs` 614) — poz. 72–81
    rejestru i klucze `REJESTR`, adresat `N8.3`. `WYJATKI_PLIK` zamrożone na 942/876/1084.
    Wynik: `--all` 577 plików, 0 błędów, 109 ostrzeżeń, 0 pozycji bez żywego adresata.

- [x] **N1.8** `plan_guard`: kod korekty porównywany jako podciąg — `R2#6`
  - `plan_guard.py:145`: `D-1` pasuje do `D-10`. Porównanie po granicy słowa.
  - *Zrobione:* `kod_w_tresci` — kod jako całe słowo. Przypadki `D-1`/`D-10` i `E2`/`E20`
    padały przed poprawką. Bramka nadal zielona: żadna z 34 obietnic nie wisiała na podciągu.

- [x] **N1.9** hook strukturalny łapie tylko `Bash` — `R1#11`
  - Matcher w `.claude/settings.json` ma objąć też `PowerShell`.
  - *Zrobione:* matcher `Bash|PowerShell`. `struct_guard --self-test` sprawdza, że matcher
    hooka ze `struct_guard.py` pasuje do obu narzędzi — przed poprawką `BLAD … (brak: PowerShell)`.

- [ ] **N1.10** `clippy.toml` bez luk — `M0#11`
  - Zakazać odpowiedników `f32` (`ln`, `exp`, `powf`, `powi`, `mul_add`, `log*`, trygonometria)
    tak jak dla `f64`; zakazać `rayon` poza `engine/jobs` (00 §6.4).
  - Usunąć `#[allow(clippy::disallowed_methods)]` z `core_bench.rs:31,52` albo dać
    powód w komentarzu `ponytail:`.
  - *Dowód:* `cargo clippy --workspace -- -D warnings` zielony.

- [ ] **N1.11** runner headless: raport rozbieżności i T-D8 — `M0#8`, `M0#9`, `M0` WP-12
  - `tools/headless/src/main.rs:306-330`: raport bez diffu encji; hash ticku 0 liczony,
    ale nie porównywany z `--expect`. T-D8 sprawdza tylko niezerowy kod wyjścia.
  - `sim.step` w konsoli (`main.rs:334`) nic nie robi → usunąć komendę.
  - *Test:* przebieg z podmienionym `--expect` kończy się raportem wskazującym archetyp.

- [ ] **N1.12** pomiar klatki bez vsync — `M11#3`, `M11#5`, `M11#6`
  - `disable_vsync()` tylko przy `--bench`, nie przy `--bench-scene`
    (`tools/magnat/src/app.rs:192`, `args.rs:239-262`). Wszystkie raporty mają `frame_ms`
    p95 ≈ 27,4 ms, czyli poniżej 60 FPS; werdykt „z zapasem 2,5×" stoi tylko na GPU.
  - `RenderBudget.target_ms` przypisywany tylko w scenie pomiarowej (`app.rs:858-861`).
  - Scena pomiarowa ma zamrozić adaptację LOD (`bench_night_rain.json`: `lod_scale 0.900`).
  - *Dowód:* nowe raporty `bench/frames/*.json`, wpisane liczby w dokumencie etapu.
  - **Decyzja N1.12-a:** nocny bieg z GPU (`M11#4`) — runnera z GPU nie ma.
    *Domyślnie:* pomiar klatki zostaje ręczny, z raportem w repozytorium; usunąć z planów
    każde zdanie, że biegnie w CI.

- [x] **N1.13** fałszywe komentarze w `ci.yml` — `M0` WP-14, `R2#9`
  - `ci.yml:17` twierdzi, że ARM biegnie co noc — joba aarch64 nie ma.
  - `ci.yml:136-142` „szesnaście z szesnastu przechodzi" przeczy `D-R7`.
  - **Decyzja N1.13-a:** ARM. *Domyślnie:* usunąć komentarz; T-D9 na ARM zostaje
    niepotwierdzony i tak zapisany w `N8.1`.
  - *Zrobione:* przyjęta propozycja domyślna N1.13-a. Oba komentarze poprawione; przy okazji
    `workflow_dispatch`, żeby nocny zestaw dało się puścić ręcznie (`gh workflow run check`)
    — balansator traktuje go jak `schedule`. Dowód: `plan_guard` (reguły 4 i 5) zielony.

## Znalezione po drodze

- [x] **N1.14** kroki CI połykają błędy przez powłokę — `nowe`
  - Na Windows krok bez `shell:` biegnie w pwsh, który zwraca kod **ostatniej** komendy:
    z czterech `cargo test` kroku M1 w `determinism` (i dwóch w M9b) liczył się tylko
    ostatni. Na Linuksie domyślny `bash -e` nie ma `pipefail`, więc
    `cargo test … | tee raport-budzetow.txt` (`budzety`) i `export-drains | tee` były
    zielone przy każdym wyniku.
  - *Naprawa:* `defaults: run: shell: bash` w `ci.yml` — jawne `bash` to
    `bash -eo pipefail` na obu systemach.
  - *Test:* reguła 5 `plan_guard` (workflow bez domyślnej powłoki `bash` jest błędem),
    z trzema przypadkami w `--self-test`.
  - *Odrzucone po sprawdzeniu:* `f32::cos`/`sin` w `sim/world/src/city/build/footprint.rs:567`
    nie łamie determinizmu — to kod testu (`wyjscie_z_bryly_trafia_w_lico`), nie symulacji.
    Dostaje `#[allow]` z powodem w `N1.10`.

- [~] **N1.15** `bench-guard` porównuje dwie różne maszyny i dwa różne systemy plików — `nowe`
  - Ostatni nocny bieg, który się wykonał (20.09, `5239480`): 37 × `BŁĄD` (od +26 % do +432 %),
    2 × `OK`. Linia bazowa jest nagrana lokalnie (`b9a3e39`), a porównywana z runnerem
    `ubuntu-latest` — różnica sprzętu, nie regresja.
  - 37 × `BRAK` w parze z 42 × `NOWY` to te same benchmarki pod inną nazwą: na Windows
    katalog `target/criterion` gubi wielkość liter i kropki na końcu nazwy („B-1…" → „b-1…",
    „300 tys." → „300 tys"), więc klucze linii bazowej nie pasują do nazw z Linuksa.
    Od `N1.4` oba przypadki wywracają bramkę, więc job jest czerwony z trzech powodów naraz.
  - *Naprawa:* nazwy porównywane po normalizacji (małe litery, bez kropek na końcu) po obu
    stronach; linia bazowa i przebieg z tej samej klasy maszyny.
  - **Decyzja N1.15-a:** gdzie powstaje linia bazowa. *Domyślnie:* na runnerze CI — job
    wystawia `target/criterion` jako artefakt, `--update` czyta z pobranego artefaktu,
    a opis commita odnowienia podaje identyfikator biegu zamiast nazwy sprzętu.
  - *Test:* przypadek `--self-test`: „B-1 x." w przebiegu i „b-1 x" w bazie to ten sam wpis.
  - *W toku:* przyjęta propozycja domyślna N1.15-a. `klucz()` normalizuje nazwy po obu
    stronach (przypadek self-testu padał przed zmianą), job wystawia artefakt `criterion`,
    reguła odnowienia w `CLAUDE.md` mówi o biegu CI zamiast sprzętu. **Brakuje:** odnowienia
    `benches/baseline.json` z artefaktu — wymaga działającego CI (od 21.09 joby nie startują
    z powodu rozliczeń konta GitHub) — i zielonego joba po nim.

- [ ] **N1.16** nocny balansator: czerwony i na granicy limitu czasu — `nowe`
  - Bieg z 20.09 (`5239480`): 4 scenariusze × 8 ziaren × 365 dób trwały 5 h 31 min przy
    limicie joba 6 h. Czerwone: G2 (hiperinflacja, 28 ziaren), G5 (rynek wymiera, 24),
    G9 (24 akcje bez powodu na 42,7 mln), G10 (populacja firm, ziarno 7), G11 (bezrobocie
    1,0–1,9 % przy paśmie 3–12 %, 32/32).
  - Skutek dla `N1.6-a`: 8 × 450 dób w jednym jobie nie zmieści się w 6 h (≈ 6,8 h).
  - *Naprawa:* scenariusze jako macierz jobów (każdy w swoim limicie), bramka w osobnym
    jobie na zebranych artefaktach. Czerwone bramki — po przebiegu na bieżącym `master`
    — dostają punkty w etapach, których dotyczą (G2, G5, G11 → E2/E4, G9 → E4 `N4.11`),
    a profil nocny dopuszcza czerwień tylko bramki wskazującej otwarty punkt, tak jak
    strażnik `#[ignore]` z `N1.1`.
