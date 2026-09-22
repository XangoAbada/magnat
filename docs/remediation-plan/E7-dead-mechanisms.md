# E7 — Mechanizmy, które żyją tylko w testach

**Po co.** Kontrakty B2B z karami, rampa, milk-run oraz rozwiązywanie związków i zmów mają
testy i bramki — a wywołania wyłącznie z testów. Bramki są zielone, a w mieście nie dzieje się
nic. Każdy taki mechanizm albo wchodzi do świata, albo przestaje udawać, że w nim jest.

**Wejście.** E1 i E2 zamknięte (kontrakty i kary płyną przez księgę).

**Pomiar zamknięcia.** Dla każdego punktu: wywołanie z kodu produkcyjnego i licznik w
przebiegu `m7-miasto` większy od zera — albo mechanizm usunięty razem z jego bramkami
i wzmiankami w dokumentach.

**Reguła.** Zanim wybierzesz „wpiąć", sprawdź w `PRD_Magnat.md`, czy PRD tego wymaga.
Kod bez konsumenta i bez wymagania się usuwa (YAGNI); da się go odzyskać z historii gita.

## Punkty

- [ ] **N7.1** kontrakty B2B i kary — `M6#4`, `M6#11`
  - `sign_contract`, `pay_penalty`, `terminate_contract`, `SupplyContractDraft::from_quote`
    nie mają wywołań poza testami; `run_contracts` chodzi co godzinę po pustej mapie.
    Panel kontraktów zawsze pusty. `prop_contract_penalty` (`tests/properties.rs:326`)
    jest tautologiczny.
  - **Decyzja N7.1-a:** *Domyślnie:* wpiąć — AI zakładu produkcyjnego podpisuje kontrakt
    z dostawcą po N kolejnych zakupach spot u tego samego dostawcy. Najprostsza reguła,
    która daje kontrakty w mieście; dostrojenie później.
  - *Test:* przebieg miasta → liczba aktywnych kontraktów > 0, co najmniej jedna kara.

- [ ] **N7.2** rampa (`Dock`) — `M6#5`, `M6` WP14
  - `Dock::arrive` (`plant/dock.rs:115`) wołają tylko testy i benchmarki;
    `Chain::odbierz_przybyle` rozładowuje w minucie `eta` bez kolejki, `bays` i godzin.
    `prop_dock_capacity` pilnuje rampy, której świat nie używa.
  - **Decyzja N7.2-a:** *Domyślnie:* usunąć ze świata bramek (bramka nie twierdzi, że rampa
    działa w mieście); kod rampy zostaje w bibliotece z komentarzem `ponytail:` wskazującym
    `RoadFreight` jako moment wpięcia. Usunąć, jeśli PRD jej nie wymaga.

- [ ] **N7.3** milk-run — `M6#6`
  - `consolidate` i `Transport::dispatch_run` woła tylko `freight.rs`. `dispatch_run` nie
    jest atomowe: błąd na przystanku `i` zostawia wysłane 0…i−1 i gubi udział kosztu.
  - **Decyzja N7.3-a:** jak `N7.2-a`. Niezależnie od decyzji `dispatch_run` ma być atomowe,
    jeśli zostaje.

- [ ] **N7.4** związki i zmowy sprzątają po zamkniętych zakładach — `M10#10`
  - `Unions::dissolve`, `Cartels::drop_member` wołane tylko z testów. Wpiąć w zamknięcie
    zakładu i upadłość — tu nie ma decyzji, to brakujące wywołanie.
  - *Test:* zakład ze związkiem bankrutuje → związek rozwiązany.

- [ ] **N7.5** `infinite_supply` i pusty test migracji — `M6#7`
  - `migracja_m6.rs:180` robi `assert!(true)`; CI nigdy nie buduje z
    `--features infinite_supply`. Test porównawczy z §6.3 M6 nie istnieje.
  - *Domyślnie:* usunąć feature i test — migracja się dokonała, porównanie „przed/po"
    nie ma już „przed".

- [ ] **N7.6** LOD wokseli: głosowanie i skirt — `M1` V3
  - `lod::majority` bez wywołań produkcyjnych, `sim/world` bierze średnią z 4 próbek, a
    `lod.rs:62-73` twierdzi co innego. Skirta nie ma.
  - *Domyślnie:* usunąć `majority` i poprawić komentarz; skirt dopisać tylko, jeśli widać
    szpary między poziomami LOD (zrzut ekranu w commicie).

- [ ] **N7.7** psucie w transporcie — `M6` odstępstwa
  - Mnożnik ×8 dla partii w drodze (R9 M6) nie istnieje; partie w drodze się nie psują.
  - *Domyślnie:* wdrożyć, jeśli partie w drodze da się objąć istniejącą pętlą psucia
    z mnożnikiem; jeśli wymaga to nowego mechanizmu — zapytać. Bez tego transport jest
    lodówką.

## Znalezione po drodze

- [ ] **N7.8** poziom trudności nic nie zmienia — `nowe`
  - Gracz wybiera `Difficulty` w kreatorze (`game/src/screens/newgame.rs:206-210`), trafia
    ona do `WorldGenParams`, `CityPlan` i dziennika — i nikt jej nie czyta. Poza parserem
    i kopiowaniem pól nie ma ani jednego odczytu w `sim/`, `game/` ani `engine/`.
    `magnat-headless` nie ma nawet flagi (na sztywno `Normal`, `worldgen.rs:91`, `nav.rs:99`).
  - PRD §4.1 (`PRD_Magnat.md:108`) wymaga wpływu na cztery rzeczy: kapitał startowy,
    agresywność konkurentów, częstość zdarzeń, stopy procentowe. Reguła etapu: PRD wymaga
    → wpiąć, nie usuwać.
  - **Decyzja N7.8-a:** *Domyślnie:* jedna tabela `data/tuning/difficulty.ron` z czterema
    mnożnikami w `bp` na poziom, czytana w czterech istniejących miejscach (kapitał wariantu
    startu, marża docelowa AI, `trigger.base_ppm` zdarzeń, premia za ryzyko w `bank.ron`).
    `normal` = 10 000 wszędzie, więc złote hashe dla `Normal` się nie zmieniają.
    Flaga `--difficulty` w `magnat-headless` w tym samym commicie.
  - *Test:* ten sam świat na `easy` i `brutal` → różny kapitał startowy gracza i różna
    liczba zdarzeń po 90 dobach; na `normal` hash równy dzisiejszemu.
