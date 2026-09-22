# E3 — Gracz i równość reguł

**Po co.** Gracz może ustawić cenę w cudzym sklepie i przejąć żyjącego obcego bogacza.
AI zamyka sklep gracza. Po wczytaniu gry odpala się stare zdarzenie życiowe — sukcesja dla
żyjącego dziedzica albo samoczynna upadłość. Firma założona w grze nie ma szyldu ani wnętrza.
To są błędy, które gracz zobaczy w pierwszej godzinie.

**Wejście.** E2 zamknięty (część punktów zmienia przepływy pieniądza gracza).

**Pomiar zamknięcia.** Sesja `m9session` z komendami: załóż firmę, otwórz sklep, zapisz,
wczytaj, zmień cenę → szyld i wnętrze widoczne, hash po wczytaniu równy hashowi przed zapisem.

## Punkty

- [ ] **N3.1** AI nie zarządza firmą gracza — `M7#3`
  - `sim/firms/src/registry.rs:111,231`, `sim/economy/src/ai_run/mod.rs:123-165`: tier
    operacyjny zmienia cel marży sklepu gracza, tier taktyczny wystawia `CloseSite` po
    stratnych miesiącach (`tactical.rs:62`), a `systems.rs:695` sklep zamyka. Wyłączenie
    zależy od wołającego (`game/src/command/exec.rs:465`); test `firm_ai.rs:90` wyklucza
    gracza ręcznie, więc błąd maskuje.
  - *Naprawa:* firma gracza wypada z harmonogramu AI w rejestrze, nie u wołającego.
  - *Test:* firma gracza ze stratą przez 6 miesięcy, przebieg przez zwykłe systemy (bez
    ręcznego wykluczenia) → sklep otwarty, marża niezmieniona.

- [ ] **N3.2** `SetPrice` sprawdza własność — `M9#1`
  - `game/src/command/check.rs:36-46`: każda komenda na zakładzie idzie przez `moj_zaklad`,
    `SetPrice` nie. `apply` (`command/mod.rs:402-414`) nadpisuje też politykę na
    `PricePolicy::Fixed`. Testy `replay.rs:62,119` ustawiają cenę w sklepie AI — błąd
    utrwalony w teście; przepisać je na sklep gracza.
  - *Test:* `SetPrice` na zakładzie AI → odrzucone z powodem.

- [ ] **N3.3** stan życia postaci po wczytaniu — `M9#2`
  - `game/src/session.rs:646-711` (`replay`), `:490-499`: `life_event` i `ending_seen` czyści
    tylko `GameState::settle`, a `replay()` przewija bez niego. Po wczytaniu: sukcesja dla
    żyjącego dziedzica albo automatyczne `DeclarePersonalBankruptcy`.
  - `life_event` i `ending_seen` wchodzą do hasha stanu gry.
  - *Test:* zgon → sukcesja → zapis → wczytanie → brak drugiej sukcesji; to samo dla
    niewypłacalności, po której gracz odrobił straty.

- [ ] **N3.4** sukcesja tylko po śmierci — `M9#5`, `M9#7`
  - `Succeed` i `ContinueAsNewCitizen` nie wymagają śmierci postaci (`check.rs:216-221`,
    `exec.rs:176-191`); `SetHeir` przyjmuje dowolnego mieszkańca bez więzi (`check.rs:210`).
  - Po likwidacji firma dalej należy do gracza (`exec.rs:385-395`,
    `sim/firms/src/registry.rs:391`) — `FoundFirm` odpowiada `AlreadyHasFirm` (PRAWDOPODOBNE).
  - *Test:* żywy gracz: `SetHeir{obcy}` odrzucone, `Succeed` odrzucone; po likwidacji
    `FoundFirm` przechodzi.

- [ ] **N3.5** szyld i wnętrze dla firm z gry — `M11#1`, `M11#2`, `M11#8`, `M11#9`
  - `game/src/view.rs:147-169` (`bind_city`), `:288-293` (`fill_sites`),
    `tools/magnat/src/signs.rs:18-44`: lista zakładów i szyldy powstają raz na ziarno.
    `FoundFirm`/`OpenSite` nadają `SiteId`, którego tam nie ma. Firma zbankrutowana
    zachowuje szyld.
  - `stock_fill: 255` na sztywno (`view.rs:282,313-314`, `tools/magnat/src/interiors.rs:135-139`);
    komentarz odsyła do „pozycji w R3", której nie ma. Komentarze `ponytail:` w
    `interiors.rs:24-28`, `signs.rs:13-16` wskazują zamkniętą M11e.
  - *Naprawa:* wypełniacz i szyldy odświeżają się przy zmianie zbioru zakładów (wersja
    rejestru), nie przy zmianie ziarna. Zapas na regale z półki sklepu.
  - *Test:* `OpenSite` → w następnym snapshocie jest szyld; pusta półka → pusty regał;
    bankructwo → szyld znika.

- [ ] **N3.6** negocjacje gracza jak AI — `M10#12`, `M10#13`
  - `talks.rs:83,142`: stały `concession_cap_bp = 1200` dla gracza zamiast sufitu AI
    w tej samej sytuacji.
  - Test `liczba_gracza_nie_omija_sufitu_ustepstwa` (`relations.rs:810-834`) jest pusty:
    krok w dobie 300, runda co 7 dni (300 % 7 = 6).
  - *Test:* ta sama sytuacja dla AI i gracza → ten sam sufit; test trafia w dzień rundy.

- [ ] **N3.7** drugi silnik cen poza `sim/policy` — `M7#8`, `M7#9`
  - Tier operacyjny AI (`sim/firms/src/ai/ops.rs`) ustawia ceny logiką, której gracz nie
    może wyrazić regułą. Test równoważności `rownowaznosc.rs:146-151` jest tautologiczny.
  - **Decyzja N3.7-a:** *Domyślnie:* przenieść logikę tieru operacyjnego do `sim/policy`
    jako wbudowaną politykę, którą gracz może przypiąć. Jeśli nie da się tego wyrazić
    obecnym językiem reguł — zapytać właściciela, zanim język zostanie rozszerzony.
  - *Test:* równoważność: ta sama polityka przez `sim/policy` dla gracza i AI daje te same
    decyzje na 1000 losowych stanach.

- [ ] **N3.8** testy i metryki gracza — `M9#4`, `M9#6`, `M9#8`, `M9#10`
  - Lint „brak `CareerTier` w walidacji" (`game/tests/panels.rs:253-265`) skanuje
    `command/mod.rs` i `exec.rs`, a `precheck` jest w `command/check.rs`.
  - Onboarding (`game/src/onboarding.rs:88-127`) liczy koperty `PlayerCommand`, nie
    interakcje; pierwsza decyzja liczy się także odrzucona.
    **Decyzja N3.8-a:** *Domyślnie:* zmienić nazwę metryki na „komendy do pierwszej
    decyzji" i próg zostawić — liczenie kliknięć wymaga instrumentacji UI, której nie ma.
  - `game/src/legacy.rs:132-138` porównuje stałą z literałem → usunąć test.
  - `engine/ui/src/loc.rs:381-383`: −2 wychodzi jako „2 sklepy”.

- [ ] **N3.9** klucze UI z `Debug` — `M9` odstępstwa
  - `ui.variant.{:?}` (`screens/newgame.rs:254,262`, `screens/character.rs:32`) — zmiana nazwy
    wariantu po cichu gubi tekst. Jawna funkcja `wariant → LocKey`.

## Znalezione po drodze
