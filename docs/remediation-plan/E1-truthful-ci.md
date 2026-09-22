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

- [ ] **N1.1** strażnik `#[ignore]` — `M2#1`, `R2#1`, wzorzec „`#[ignore]` bez joba"
  - *Naprawa:* dopisać do `scripts/plan_guard.py` (już parsuje `ci.yml`) sprawdzenie:
    każdy `#[ignore]` w `*/tests/*.rs` i `*/src/*.rs` jest uruchamiany przez któryś krok
    `ci.yml` (`--include-ignored` / `--ignored` na tym pliku testów albo filtr po nazwie)
    **albo** jego powód zawiera `N<e>.<n>` otwartego punktu z `docs/remediation-plan/`.
  - *Test:* self-test strażnika z trzema przypadkami: test z jobem, test z otwartym punktem,
    test bez niczego (błąd).

- [ ] **N1.2** dopiąć do CI testy, które twierdzą, że w nim są — `M2#1`, `R2#1`, `M10` WP10.16, `M11` R2-WP17
  - `sim/world/tests/consistency.rs` (T1–T13, D1, D2, D5, budżet, WP13, WP16 — 7 testów),
  - `sim/world/tests/lsystem_split.rs` (dowód `R2-WP21`),
  - `tools/headless/tests/m10_domkniecie.rs` (dwa przebiegi = ten sam ciąg hashy),
  - `tools/headless/tests/full_city.rs:162` (kopalnia na złożu).
  - *Gdzie:* job `determinism` (nocny), tak jak pozostałe testy `sim/world` w release.
  - *Dowód:* zielony przebieg nocny albo jawne `#[ignore = "N…"]` według uwagi wyżej.

- [ ] **N1.3** `m3_century` w nocnym CI i progi, które wymusza — `M3#4`
  - Runner `tools/headless/src/century.rs:167-169,236-241` drukuje populację, ale nie
    sprawdza §7.2 („nigdy < 1000", „nigdy > 3×"). Wersja testowa `wp7_…` sprawdza `min > 0`.
  - *Naprawa:* kod wyjścia ≠ 0 przy przekroczeniu progu; krok w nocnym jobie.

- [ ] **N1.4** `bench_guard`: zniknięty benchmark to błąd — `R2#2`
  - `scripts/bench_guard.py:75-77`: wiersz `BRAK … zniknął` nie ustawia `kod = 1`.
  - *Test:* przypadek w `--self-test` sprawdzający **kod wyjścia**, nie tylko wiersz.

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

- [ ] **N1.7** `struct_guard` — dziury w bramce — `R1#3`, `R1#5`, `R1#6`, `R1#8`, `R1#9`, `R1#10`, `R2#5`
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

- [ ] **N1.8** `plan_guard`: kod korekty porównywany jako podciąg — `R2#6`
  - `plan_guard.py:145`: `D-1` pasuje do `D-10`. Porównanie po granicy słowa.

- [ ] **N1.9** hook strukturalny łapie tylko `Bash` — `R1#11`
  - Matcher w `.claude/settings.json` ma objąć też `PowerShell`.

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
