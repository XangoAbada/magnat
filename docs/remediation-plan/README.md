# Plan naprawczy

Źródło: audyt z 22 września 2026, `master` @ `b9a3e39` —
https://claude.ai/artifact/16m6sfFYaoqkKice2Lrcgb (16 raportów, ok. 170 unikalnych znalezisk,
34 wysokiej wagi).

Plan powstał z audytu, nie z planu implementacyjnego. Nie ma faz, podfaz ani bramek fazowych.
Ma **etapy**, a każdy etap ma listę **punktów**. Każdy punkt to jedno znalezisko albo zwarta
grupa znalezisk w tym samym miejscu kodu.

## Diagnoza w trzech zdaniach

Kod jest w większości napisany. Kłopot w tym, że część mechanizmów działa tylko w testach,
część testów nie może się nie udać, a część nie biegnie w CI. Dlatego przetrwały błędy
księgowe, które zachowują sumę pieniądza, więc bramka zachowania pieniądza ich nie widzi.

## Etapy

| Etap | Plik | O czym | Status |
|---|---|---|---|
| E1 | [E1-truthful-ci.md](E1-truthful-ci.md) | CI i bramki mówią prawdę | [ ] |
| E2 | [E2-accounting.md](E2-accounting.md) | Pieniądz i księgowość | [ ] |
| E3 | [E3-player.md](E3-player.md) | Gracz i równość reguł | [ ] |
| E4 | [E4-simulation.md](E4-simulation.md) | Symulacja: doba, ruch, złoża, zdarzenia | [ ] |
| E5 | [E5-macro.md](E5-macro.md) | Makro i spójność LOD | [ ] |
| E6 | [E6-engine-and-save.md](E6-engine-and-save.md) | Silnik ECS, hash stanu, zapis | [ ] |
| E7 | [E7-dead-mechanisms.md](E7-dead-mechanisms.md) | Mechanizmy, które żyją tylko w testach | [ ] |
| E8 | [E8-plan-reconciliation.md](E8-plan-reconciliation.md) | Dokumenty zgodne z kodem | [ ] |
| E9 | [E9-ui-polish.md](E9-ui-polish.md) | Oprawa interfejsu (nie z audytu) | [ ] |
| E10 | [E10-audio.md](E10-audio.md) | Dźwięk: nagrania i głośność (nie z audytu) | [ ] |

**Kolejność.** E1 najpierw i bez wyjątków — jest tani, a bez niego nie wiadomo, czy
naprawa w E2–E7 czegokolwiek dowodzi. E2 przed E3 i E5, bo gracz i makro stoją na tej samej
księdze. E4, E6 i E7 są od siebie niezależne i mogą iść w dowolnej kolejności po E2.
E8 zamyka plan.

**E9 stoi obok tej kolejności.** Nie pochodzi z audytu, tylko z makiet interfejsu
(`docs/ui-inspiracje/`), i nie dotyka niczego, co naprawiają E2–E7: to krój, ikony
i wygląd paneli oraz ekranów powłoki. Wymaga tylko E1, bo bez niego złote testy paneli
niczego nie dowodzą. Idzie równolegle z E2–E8 i nie blokuje zamknięcia planu.

**E10 stoi obok tak samo.** Nie pochodzi z audytu, tylko z prośby właściciela produktu:
nagrania z ComfyUI zamiast syntezy i suwaki głośności w ustawieniach. Dotyka `engine/audio`,
profilu gracza i ekranu ustawień, czyli niczego, co naprawiają E2–E7. Wymaga tylko E1,
idzie równolegle z resztą i nie blokuje zamknięcia planu. Z E9 dzieli ekran ustawień
(`N9.4` i `N10.2`) — kolejność opisuje nagłówek E10.

**Plan implementacyjny stoi.** Dopóki E1–E7 nie są zamknięte, nie zaczynamy nowych
podfaz z `docs/implementation-plan/` (R3, M12). Powód: M12b stoi na `generation` chunka
(E6), R3-WP1 na niezmienniku pieniądza (E2), a każda nowa faza dopisze kod na przyrządach,
które dziś kłamią.

## Punkt: jak wygląda i kiedy jest zamknięty

Każdy punkt ma identyfikator `N<etap>.<nr>` (np. `N2.3`) i odwołanie do audytu w formie
`<faza>#<nr znaleziska>` (np. `M7#2` = raport M7, znalezisko 2).

Punkt jest zamknięty (`[x]`), gdy:

1. **Jest test, który pada na kodzie sprzed naprawy.** Najpierw test, potem poprawka.
   Fakt, że test padał, odnotowuje komunikat commita (nazwa testu i komunikat błędu).
2. **Test przechodzi po naprawie i biegnie w CI.** `#[ignore]` bez joba nie zamyka
   niczego (patrz `N1.1`).
3. **Znalezisko się nie potwierdziło** — wtedy punkt dostaje `[-]` i jedno zdanie, dlaczego.
   Audytorzy czytali źródła, nie uruchamiali kodu; znacznik PRAWDOPODOBNE znaczy „nie
   odtworzone". Wynik audytu jest raportem, nie prawdą.

Punkt częściowo zrobiony ma `[~]` i zdanie, czego brakuje.

Etap jest zamknięty, gdy wszystkie jego punkty mają `[x]` albo `[-]` i wykonano
**pomiar zamknięcia** z nagłówka etapu. Pomiar wpisuje się do dokumentu etapu liczbami.

## Decyzje

Punkty oznaczone **Decyzja** mają propozycję domyślną. Przyjmij ją i powiedz o tym
w commicie, albo zapytaj właściciela, jeśli wybór zmienia kształt rozwiązania.
Nie zaczynaj punktu, którego decyzja jest nierozstrzygnięta.

## Znalezione po drodze

Naprawa regularnie pokaże błąd, którego audyt nie znalazł. Taki błąd trafia na koniec
dokumentu etapu, którego dotyczy, jako nowy punkt z odwołaniem `nowe` zamiast `<faza>#<nr>`.
Nigdy jako `TODO` w kodzie.

## Dziennik

Jedna linia na zamknięty punkt albo grupę: data, identyfikatory, co zrobiono, hash commita.

| Data | Punkty | Co | Commit |
|---|---|---|---|
| 2026-09-22 | N1.14 | `defaults: run: shell: bash` w `ci.yml`; reguła 5 `plan_guard` | `1e21c7b` |
| 2026-09-22 | N1.13 | fałszywe komentarze o ARM i „16 z 16” w `ci.yml`; `workflow_dispatch` jako bieg nocny na żądanie | `54163da` |
| 2026-09-22 | N1.4 | `bench_guard`: zniknięty benchmark ustawia kod 1 | `fdd4edb` |
| 2026-09-22 | N1.8 | `plan_guard`: kod korekty dopasowywany jako całe słowo | `0b02d07` |
| 2026-09-22 | N1.9 | hook strukturalny także na `PowerShell`; self-test pilnuje matchera | `e3df1c5` |
| 2026-09-22 | N1.1 | reguła 6 `plan_guard`: każdy `#[ignore]` ma job, otwarty punkt albo jest narzędziem; 36 ze 100 dostało właściciela; nowe N1.15, N1.16 | `189ccde` |
| 2026-09-22 | N1.7 | `struct_guard`: osiem dziur zamkniętych; rejestr z kolumną „Adresat”, 60 pozycji → `N8.3` z przeglądem 2026-10-22, 10 nowych (72–81) | `e15920d` |
| 2026-09-22 | N1.15 `[~]` | `bench_guard`: nazwy z Windows i Linuksa to jeden wpis; artefakt `criterion` z CI jako źródło linii bazowej | ten commit |
