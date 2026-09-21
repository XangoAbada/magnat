# R2c — Rozjazdy danych i kodu

Podfaza dokumentu naprawczego `R2-naprawy-po-M11.md`. Numeracja sekcji `5.x` jest numeracją
dokumentu R2.

| | |
|---|---|
| **Wejście** | R2 otwarte. Pakiety R2c nie dotykają plików, które rusza `R2a` — obie podfazy można prowadzić równolegle. |
| **Pakiety robocze** | R2-WP12…R2-WP16 |
| **Wynik do pokazania** | `headless m3day` z nową sekcją „wartość czasu": rozkład stawek VoT w populacji w dobie 1 i w dobie 3600. Dziś te dwa rozkłady są identyczne, bo VoT nie jest odświeżana ani razu. |
| **Kryterium zamknięcia** | Kryteria R2-WP12…R2-WP16 plus: **żadna wielkość z tej podfazy nie ma dwóch źródeł** — test statyczny szuka stałych liczbowych w kodzie symulacji tam, gdzie odpowiednik stoi w `data/`. |
| **Poprzednia / następna** | `R2b-pieniadz-gospodarstwa.md` / `R2d-domkniecie-swiata.md` |

---

## 5.7 Wspólny wzorzec: wielkość, która ma dwa źródła albo zero konsumentów

Pięć pakietów tej podfazy wygląda na pięć niezwiązanych drobiazgów. Łączy je jedna rzecz: każdy
jest miejscem, w którym **deklaracja i zachowanie rozjechały się po cichu**, bo nie ma testu,
który by je porównał.

Trzy warianty tego samego błędu:

- **Dwa źródła.** Wiek produkcyjny jest w danych i jako stała w kodzie, w dwóch miejscach, i te
  liczby są różne (R2-WP12).
- **Źródło, które przestało być czytane.** Dochód gospodarstwa jest aktualizowany poprawnie, ale
  wartość czasu czyta jego migawkę z generacji świata i nigdy jej nie odświeża (R2-WP13).
- **Konsument, który nic nie robi.** Kaskada niedoboru produkuje akcję podmiany, a odbiorca ma dla
  niej puste ramię `match` (R2-WP14). Potrzeba ma tempo i próg, ale zero miejsc zaspokojenia
  (R2-WP16). Warstwa piesza ma flagę chodnika w projekcie i nie ma jej w kodzie (R2-WP15).

Wniosek dla kryterium zamknięcia: **nie wystarczy naprawić pięciu miejsc**. Podfaza kończy się
testem statycznym, który szuka tej klasy błędu w całym kodzie symulacji, a nie tylko w tych
pięciu.

---

## 5.8 Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R2-WP12 ⇧ | Wiek produkcyjny w jednym miejscu | — | S | `[x]` **wykonane w M8c** (2026-09-17) jako warunek wejścia podfazy, zgodnie z propozycją domyślną `D-N1` |
| R2-WP13 | Wartość czasu idzie za dochodem | — | M | `[x]` (`K-98`; motoryzacja wydzielona do `R2-WP38`, `D-N12`) |
| R2-WP14 | Szczebel substytucji dostaje wykonawcę | — | M | `[x]` (`K-97`) |
| R2-WP15 → **M11c** | Chodniki: warstwa piesza bez dróg szybkiego ruchu | — | S | `[x]` **zamknięte w M11c** |
| R2-WP16 | Potrzeby bez martwych slotów | — | M | `[x]` (`K-96`; razem z pozycją 43 — odbudowa nastroju) |
| R2-WP38 | Motoryzacja idzie za dochodem | R2-WP13 | M | `[ ]` — patrz `D-N12` |

---

### R2-WP12 — Wiek produkcyjny w jednym miejscu

**Pozycja wykazu:** 7.

**Przyczyna.** `data/demography/demography.ron` deklaruje `ages.work_start = 18`
i `ages.retirement = 65`. Kod ma niezależnie od tego `const WORKING_AGE: RangeInclusive<i32> =
16..=74` w **dwóch miejscach**: `sim/economy/src/labor/system.rs` i `sim/macro/src/lift.rs`.
Komentarz przy stałej broni się rozsądnie — statystyka publiczna liczy siłę roboczą inaczej niż
kadry liczą etaty — i to jest prawda. Problemem nie jest istnienie dwóch pojęć, tylko to, że
**jedno z nich jest stałą w kodzie, powieloną, i nikt nie wie, że jest powielona**.

Skutek praktyczny łączy się z pozycją 1: skoro dziecko urodzone w grze nie dostaje flagi ucznia,
to rynek pracy M7 widzi je jako kandydata od szesnastego roku życia, mimo że dane mówią osiemnaście.

**Szew.** Wiek produkcyjny staje się daną (`K-60`). Pole `ages.labour_force: (u8, u8)` w tym samym
pliku, co pozostałe progi, walidowane razem z nimi. Obie stałe znikają, obaj wołający czytają
`DemographyTable`. `sim/macro` zależy od `sim/agents`, więc dostęp ma.

**Zakres.**

| Co | Gdzie |
|---|---|
| `ages.labour_force: (16, 74)` z walidatorem (dolna ≥ `school_start`, górna ≤ `ages.max`) | `data/demography/demography.ron`, `sim/agents/src/demography/table.rs` |
| Usunięcie obu stałych, przejście na dane | `sim/economy/src/labor/system.rs`, `sim/macro/src/lift.rs` |
| Wpis `K-60` w `00` §4a | — |

**Wykonane przed R2, w M8c.** Pakiet był pozycją 7 wykazu i jedyną z §2b, która blokowała
**kalibrację hazardów** M8c: sonda `UnemploymentPermille` generatora zdarzeń stoi na stopie
bezrobocia, a ta miała trzy niezgodne definicje (18–65 w danych, 16–74 dwa razy w kodzie,
w dwóch różnych typach całkowitych). Krzywe protestu i fali przestępczości mierzyłyby wtedy
własny rozjazd. Stan po wykonaniu: pole `ages.labour_force: (min: 16, max: 74)` z walidatorem,
`DEMOGRAPHY_SCHEMA_VERSION` 1 → 2, obie stałe usunięte, test `sim/agents/tests/labour_force.rs`
(dwa przypadki: granice są w danych i są szersze niż wiek pracy; odwrócone granice są błędem
ładowania). Wpis `K-60` w `00` §4a powstał razem z commitem M8c.

**Kryterium:** test odtwarzający — zmiana `ages.labour_force` w pliku danych zmienia mianownik
stopy bezrobocia **i** liczbę kohort w modelu makro. Przed naprawą nie zmienia żadnego z dwóch.
Test statyczny: w `sim/economy` i `sim/macro` nie ma stałej będącej przedziałem wieku.

---

### R2-WP13 — Wartość czasu idzie za dochodem

**Pozycje wykazu:** 4, 13.

**Przyczyna.** `TrafficOracle::set_incomes` jest wołane dokładnie raz, z
`sim/world/src/population/mod.rs`, przy zasiedlaniu świata. Tablica dochodów na godzinę jest od
tej chwili zamrożona. Mieszkaniec, który po pięciu latach awansował, stracił pracę albo ją
znalazł, wycenia swoją godzinę tak, jak w dniu powstania świata.

To nie jest kosmetyka. Wartość czasu jest mnożnikiem **największego składnika** kosztu uogólnionego
w wyborze środka transportu; od niej zależy podział między marsz, rower, komunikację i auto
w każdej podróży każdego mieszkańca. `M5` `U-23` zapisało, że część pieniężna kosztu jest zerem
z nazwanym sufitem — czyli człon czasowy jest dziś praktycznie całym kosztem.

Druga pozycja jest tą samą chorobą w innym miejscu: motoryzacja to płaska stawka 430 ‰,
losowana raz przy zasiedleniu, bez związku z dochodem. `M4b` `L-12` zapisało to świadomie
z uzasadnieniem „zależność od dochodu odpada, dopóki nie ma budżetów gospodarstw (M5)".
**M5 jest zamknięte od trzech faz.**

**Szew.** Odświeżanie idzie tam, gdzie dochód już się zmienia: funkcja przesuwająca
`income_monthly` (trzech wołających: zatrudnienie, zwolnienie, podwyżka) przesuwa też wpis
w tablicy oracle'a. Nie ma przebiegu okresowego, nie ma przeliczania całej tablicy — jest ta sama
operacja przyrostowa, co dla dochodu, bo źródło jest to samo.

Motoryzacja przestaje być rzutem przy zasiedleniu i staje się przeglądem miesięcznym: gospodarstwo
bez auta, którego dochód przekroczył próg z danych, kupuje auto, jeśli jest gdzie je zaparkować.
Gospodarstwo z autem i dochodem poniżej progu wyprzedaży — sprzedaje. Próg i histereza idą
z `data/roads/mode_choice.ron`.

**Zakres.**

| Co | Gdzie |
|---|---|
| `TrafficOracle::bump_income(household, delta)` obok `set_incomes` | `sim/traffic/src/oracle.rs` |
| Wołanie z funkcji przesuwającej dochód | `sim/economy/src/labor/system.rs` |
| Przegląd motoryzacji raz na miesiąc, próg z histerezą | `sim/traffic/src/systems.rs`, `data/roads/mode_choice.ron` |
| Sekcja „wartość czasu" w raporcie `m3day` | `tools/headless/src/m3day.rs` |

**Ostrzeżenie o determinizmie.** Zmiana rozkładu VoT zmienia udziały środków transportu, czyli
obciążenie sieci, czyli czasy przejazdu, czyli plany dnia. To jest najszerzej promieniujący pakiet
w całym R2 i dlatego ma własne kryterium pomiarowe, nie tylko test jednostkowy.

**Wykonane inaczej, niż zapowiadał szew — dwie korekty (`C-1`, `C-2`).** Po pierwsze,
`bump_income(household, delta)` wołane z rynku pracy **nie powstaje**: `sim/economy` nie zależy
od `magnat-traffic`, a nowa krawędź grafu kupowałaby dokładnie ten wzorzec, który `K-93` właśnie
naprawiał — przyrostową denormalizację dochodu, w której nikt nie odpowiada za to, żeby każde
`+wage` miało parę `−wage`. Zamiast tego `TrafficSystem` przelicza **całą tablicę raz na dobę**
(`household_incomes`, `K-98`): jedna reguła, dwóch wołających, zero rozproszonych delt i zero
nowych zależności. Po drugie, motoryzacja wychodzi z tego pakietu do własnego (`R2-WP38`) —
uzasadnienie w `D-N12`.

**Kryterium:** test odtwarzający — mieszkaniec zatrudniony w ticku 100 ma po dobie wyższą wartość
czasu niż przed zatrudnieniem, a jego wybór środka transportu na tej samej trasie przesuwa się
w stronę szybszego. Przed naprawą wartość czasu jest identyczna. Przebieg `m3day --days 3600`:
rozkład VoT w dobie 3600 różni się od rozkładu w dobie 1, a **udziały środków transportu
pozostają w widełkach metropolii** z `data/roads/mode_choice.ron`. Wyjście poza widełki jest
zadaniem kalibracyjnym z własnym wierszem, nie powodem do cofnięcia naprawy (ryzyko `N-6`).

**Zmierzone (2026-09-21), z jawną luką.** `m3day --size 4km --seed 1`: doba 1 — 11 433
gospodarstwa, mediana 48 gr/min, średnia 49, Q1 24, Q3 72; doba 30 — 11 960 gospodarstw,
mediana 46, średnia 47, Q1 23, Q3 71. Rozkłady się różnią, a przed naprawą były identyczne
co do grosza. **Druga połowa kryterium została niezmierzona i to jest jej status, a nie
przeoczenie:** bramka widełek wyłącza się sama poniżej populacji odniesienia (150 tys.
z `mode_choice.ron`), a świat 4 km ma 28,6 tys. mieszkańców i wypisuje wprost „miasto
mniejsze od odniesienia — bramka nie działa”. Sprawdzenie udziałów wymaga przebiegu na
mieście wielkości metropolii i należy do `R2f` razem z resztą bramek — wiersz w `R2-WP24`.

---

### R2-WP14 — Szczebel substytucji dostaje wykonawcę

**Pozycja wykazu:** 3.

**Przyczyna.** Kaskada niedoboru ma siedem szczebli i szósty z nich nazywa się `Substituted`.
`shortage::review` produkuje dla niego akcję — pyta najpierw o substytut receptury, potem
o substytut towaru — a `B2b::serve` ma dla tej akcji **puste ramię `match`**. Szósty szczebel
jest więc przejściem do siódmego: zakład idzie z „szukam zamiennika" prosto w `Halted`.

Dane są gotowe i ładowane: `Substitute` istnieje w katalogu dóbr, a `M6` `AG-6` i `M6b` `AF-9`
zapisały, że szczebel był nieosiągalny z braku substytutów i że jeden dopisano. Tego, że wykonawcy
nie ma, nie zapisał nikt — korekty opisują dane, nie ramię `match`.

**Szew.** Podmiana jest **zmianą receptury na linii**, a nie zmianą towaru w zamówieniu. Linia
z ustawioną recepturą alternatywną przezbraja się (koszt czasu i odpadu z `Setup`, mechanizm już
istnieje) i produkuje ten sam wyrób z innego wsadu. To jest najmniejsza zmiana, która daje temu
szczeblowi treść, i korzysta wyłącznie z mechanizmów, które są.

Jakość wyrobu z substytutu spada przez istniejący `cap_by_worst_input` — nie trzeba nowego
parametru.

**Zakres.**

| Co | Gdzie |
|---|---|
| `ShortageAction::Substitute` wybiera recepturę alternatywną i ustawia ją na linii | `sim/supply/src/b2b.rs::serve` |
| Powrót do receptury pierwotnej, gdy pierwotny wsad wraca ponad próg | `sim/supply/src/shortage.rs` |
| `DecisionReason::RecipeSubstituted { from, to, cause }` | `engine/core/src/decision.rs` + `engine/ui` |
| Substytuty dla łańcucha referencyjnego nr 1 w katalogu | `data/recipes/food.ron` |

**Wykonane inaczej, niż zapowiadał szew — trzy korekty (`C-3`…`C-5`).** Szew opisywał mechanizm,
którego w repozytorium nie ma i nigdy nie było. **(1) „Receptura alternatywna" nie istnieje.**
Substytucja jest w tym katalogu podmianą **jednego wejścia w obrębie tej samej receptury**
(`RecipeInput.substitutes` z `mass_ratio_permille` i `quality_penalty`), a nie drugą recepturą
na linii — pola `alternatives` nie ma ani w `Recipe`, ani w `data/recipes/`. Wykonawcą jest więc
`plant::produce`: brakującą resztę wejścia dobiera z zamiennika, jeśli kaskada doszła do szczebla
`Substituted`. Przezbrojenie zostaje tam, gdzie było — służy zmianie wyrobu, nie wsadu.
**(2) `DecisionReason::RecipeSubstituted` nie powstaje.** `SubstituteUsed = 402` istnieje od M6b,
`review` już go zapisuje i ma ramię renderu w obu językach — drugi wariant opisywałby to samo
zdarzenie drugi raz. **(3) `ShortageAction::Substitute` znika zamiast dostać wykonawcę.** Wszystko,
co niósł, niesie stopień `ShortageStage::Substituted`; `B2b::serve` miał dla niego puste ramię,
bo podmiany się **nie kupuje**. Test statyczny z kryterium jest po tym spełniony wprost: enum
akcji ma dwa warianty i oba mają treść.

**Czwarta rzecz, której szew nie przewidywał: kaskada potrzebowała sufitu.** Pokrycie liczy się
z **pierwotnego** wsadu, więc zostaje zerem także wtedy, gdy zamiennik faktycznie karmi linię —
a `szczebel_progowy(0)` to `Halted`. Zakład jadący na otrębach wchodziłby więc na postój przy
pełnym magazynie otrąb. `review` clampuje cel do `Substituted`, dopóki zamiennika starcza; sufit
działa **tylko w górę**, więc pierwotna dostawa nadal ściąga drabinę w dół, a wyczerpanie
zamiennika zdejmuje sufit i postój przychodzi w następnej godzinie.

**Kryterium:** test odtwarzający — piekarnia bez mąki, z otrębami w magazynie (jedyny substytut
w katalogu fali A, przy wejściu receptury `bakery_bread_wheat`), piecze dalej i **nie zatrzymuje
się**; kaskada zatrzymuje się na szczeblu `Substituted`, nie dochodzi do `Halted`. Przed naprawą
dochodzi do `Halted` w tej samej dobie. Jakość: wsad szarży na zamienniku ma niższe `q_in`
niż wsad na mące — kara z danych schodzi z jakości **wejścia**, więc `cap_by_worst_input` tnie
wyrób sam, bez ani jednego nowego parametru. Test statyczny: w `sim/supply` nie ma pustego
ramienia `match` dla żadnego wariantu `ShortageAction`.

---

### R2-WP15 — Chodniki: warstwa piesza bez dróg szybkiego ruchu

> **Pakiet wykonuje M11c.** Decyzja właściciela produktu z 2026-09-19 przeniosła go do podfazy
> prezentacyjnej razem z trzema innymi, bo M11c ma pokazać ulicę, a ulica jest pusta i nie ma
> na niej szyldów. Zakres, kryterium i sufit czasu zostają **bez zmian** — zmienia się wyłącznie
> adres wykonania. Powód w tabeli `J-n` dokumentu `M11c-wnetrza-i-kamera.md`.
>
> **Zamknięty w M11c** (2026-09-19) — z testem; szczegóły w tabeli `J-n` tamtego dokumentu.


**Pozycja wykazu:** 12.

**Przyczyna.** `nav_build.rs` buduje warstwę `Modality::Foot` z wszystkich segmentów poza koleją —
łącznie z drogami szybkiego ruchu. Komentarz `ponytail:` nazywa sufit i wskazuje ścieżkę wyjścia:
`RoadFlags::SIDEWALK` w M4c. **M4 zamknęło się bez niej.** Flaga jest w projekcie `M2b` §5
i nie ma jej w kodzie.

Skutek widać w wyborze środka transportu: marsz wzdłuż obwodnicy jest wykonalny i tani, więc
bywa wybierany — a jest to jedyna opcja bez składnika pieniężnego, więc przy niskiej wartości
czasu wygrywa.

**Szew.** Flaga nadawana przy budowie sieci, z klasy drogi — nie jako osobne dane. Klasy
`highway` i `expressway` nie dostają chodnika, reszta dostaje. Warstwa piesza filtruje po fladze.
Przejścia dla pieszych przez drogi bez chodnika zostają tam, gdzie są dziś — w węzłach — więc
sieć się nie rozpada.

**Zakres.**

| Co | Gdzie |
|---|---|
| `RoadFlags::SIDEWALK` nadawana z `RoadClass` przy budowie segmentu | `sim/world/src/city/road.rs` |
| Warstwa `Foot` filtruje po fladze | `sim/world/src/nav_build.rs` |
| Walidacja: każda parcela z frontem drogowym nadal ma dojście pieszo | tamże — istniejące `NavBuildError::ParcelUnreachable` |

**Kryterium:** test odtwarzający — trasa piesza między dwoma punktami po obu stronach obwodnicy
nie prowadzi po obwodnicy, tylko przez najbliższe przejście. Przed naprawą prowadzi po obwodnicy.
Test istniejący `parcel_unreachable` musi nadal przechodzić na wszystkich pięciu regionach —
to jest zabezpieczenie przed odcięciem dzielnicy od sieci pieszej.

---

### R2-WP16 — Potrzeby bez martwych slotów

**Pozycje wykazu:** 14, 15.

**Przyczyna.** Dwanaście potrzeb, z czego trzy mają tempo zero i pustą listę miejsc zaspokojenia:
`Safety`, `Housing` i `Status`. Dla dwóch pierwszych to jest w porządku i zapisane — właścicielem
`Safety` jest M8 (`M8` `Z-1`), `Housing` należy do M9. **`Status` miał właściciela w M5** i M5
jest zamknięte od trzech faz. `M3a` `D-15`, `M3d` `H-3` i `M5` `Z-3` opisują tempo zero jako
świadome; tego, że po M5 nadal nikt go nie stosuje, nie zapisał nikt.

Ten sam wzorzec w efektach deprywacji: `ProductivityLoss` i `AmbitionGain` są zadeklarowane
w `data/needs/needs.ron` i **jawnie pominięte** w systemie stosującym skutki. `M7b` zapisało,
że czekają na M7. M7 jest zamknięte.

Koszt nie jest zerowy: `Status` zajmuje slot w szesnastobajtowej tablicy przeliczanej dla każdego
mieszkańca w każdym ticku shardu, a `DeprivationEffectsSystem` iteruje po wszystkich efektach.

**Szew.** Trzy możliwe zakończenia i każda potrzeba dostaje jedno z nich, jawnie:

1. **Ma właściciela i zostaje** — `Safety` (M8), `Housing` (M9). Wiersz w danych z adresem w komentarzu.
2. **Dostaje treść w R2** — `ProductivityLoss` i `AmbitionGain` mają konsumentów, którzy już
   istnieją: produktywność czyta `sim/firms/src/hr/productivity.rs`, ambicja wchodzi do progu
   zmiany pracy w `labor_policy.rs`. Podpięcie jest jednolinijkowe po każdej stronie.
3. **Znika** — `Status` jako potrzeba nie ma konsumenta i nie będzie miała: status społeczny jest
   liczony w `social.rs` z siedmiu czynników i **jest wielkością wyprowadzaną, nie potrzebą**.
   Dwa modele tej samej rzeczy to jeden za dużo (`D-N11`).

**Zakres.**

| Co | Gdzie |
|---|---|
| `NeedKind::Status` usunięty z enumu i z danych; tablica potrzeb schodzi z 12 do 11 | `engine/core/src/vocab.rs`, `data/needs/needs.ron` |
| `ProductivityLoss` stosowany: mnożnik do `effective_labor` | `sim/firms/src/hr/productivity.rs` |
| `AmbitionGain` stosowany: przesunięcie progu zmiany pracy | `sim/firms/src/labor_policy.rs` |
| `Safety` i `Housing` dostają w danych komentarz z adresem fazy | `data/needs/needs.ron` |
| Walidator: potrzeba z tempem zero **musi** mieć w danych pole `owner_phase` | `sim/agents/src/needs.rs` |

**Ostrzeżenie o determinizmie.** Usunięcie wariantu z `NeedKind` zmienia rozmiar komponentu
`Needs` i jego reprezentację w hashu. To jest zmiana formatu zapisu gry — dopuszczalna tu, bo
R2 stoi przed M12b, które format zapisu i tak przebudowuje.

**Wykonane z trzema korektami (`C-6`…`C-8`).** **(1) `AmbitionGain` nie dostaje parametru
w `switch_threshold_bp`, tylko wchodzi do `PersonFacts.ambition`.** Skutek nazywa się „ambicja
rośnie", a `switch_threshold_bp(ambition, loyalty, t)` **już** umie ambicję czytać — drugi
parametr opisywałby tę samą rzecz dwa razy i rozjechałby się z pierwszym przy pierwszej zmianie
wagi. `PersonFacts.ambition` znaczy od tej chwili „ambicja skuteczna": cecha osobowości plus
deprywacja rozwoju, przelicznik 100 promili = 10 punktów skali `Q`. Reguła zostaje jedna i ma
swój test strażnik od M7b. **(2) `ProductivityLoss` wchodzi do `effective_labor` jako szósty
argument**, a domknięcie `Fn(CitizenId) -> Option<(Vitals, Q)>` rośnie o `DeprivationPressure`.
Nacisk liczy `sim/agents` (`needs::pressure`), bo tylko on widzi poziomy potrzeb; sufit strat
(70 %) jest w kodzie z nazwanym powodem, a nie w danych: człowiek w skrajnej deprywacji pracuje
**gorzej**, ale przychodzi — bez sufitu wystarczyłoby dosypać skutków w danych, żeby zakład
stanął, nie tracąc ani jednego pracownika. **(3) Pozycja 43 wykazu wchodzi do tego pakietu
naprawdę, a nie tylko adresem.** `MoodLoss` był **jedynym** pisarzem `Vitals.mood` w całym
repozytorium, więc po roku gry całe miasto siedziało na −100 — a nastrój wchodzi do
`effective_labor` przez `mood01`, czyli do liczby, którą ten pakiet właśnie zaczął ważyć.
Odbudowa jest bezwarunkowa i konkuruje z karami (`mood_recovery_centi_per_hour` w `needs.ron`);
zero jest sufitem i to jest jawne ograniczenie modelu — **nic nie podnosi nastroju ponad
neutralny**, bo żadna faza do niego nie pisze.

**Kryterium:** test odtwarzający — mieszkaniec z głodem poniżej progu krytycznego ma niższą
produktywność niż najedzony rówieśnik o tych samych umiejętnościach; mieszkaniec z długotrwałą
deprywacją ma wyższą ambicję, czyli niższy próg zmiany pracy. Przed naprawą oba są identyczne.
Test walidatora: potrzeba z tempem zero, bez miejsc zaspokojenia i bez `owner_phase` **przerywa
ładowanie** danych. Test nastroju: mieszkaniec z zaspokojonymi potrzebami wychodzi z −100 do zera
i tam staje, a dwie zdeprywowane potrzeby nastrojowe wypychają go poniżej.

---

## 5.9 Decyzje otwarte tej podfazy

**`D-N11` — Czy `NeedKind::Status` znika, czy dostaje treść.** **Przyjęta propozycja domyślna
(2026-09-21).** Znika. Status społeczny
jest już liczony w `social.rs` z siedmiu czynników z wagami z danych i jest wielkością
wyprowadzaną — mieszkaniec nie „zaspokaja potrzeby statusu", tylko ma status wynikający z dochodu,
majątku, wykształcenia, zawodu, adresu, konsumpcji i rodziny. Utrzymywanie drugiego modelu tej
samej rzeczy kosztuje slot w tablicy i mylącą pozycję na karcie inspekcji. Wariant odwrotny
(status jako potrzeba z miejscami zaspokojenia — restauracje, kluby, sklepy marek) jest mechaniką
z zakresu M10b (marka) i tam powinien powstać. *Blokująca dla R2-WP16.*

**`D-N12` — Czy motoryzacja przegląda się miesięcznie, czy przy zmianie dochodu.** Propozycja:
miesięcznie. Przy zmianie dochodu byłoby taniej obliczeniowo, ale dawałoby kupno auta w dobie
podwyżki, co wygląda jak błąd, a nie jak decyzja. Miesięczny przegląd z histerezą rozkłada zakupy
i pozwala na warunek „dochód powyżej progu przez trzy miesiące". *Nieblokująca.*

**Rozstrzygnięte 2026-09-21 co do kadencji, przeniesione co do wykonania: pakiet `R2-WP38`.**
Miesięcznie — propozycja domyślna zostaje. Samo wykonanie wychodzi jednak z `R2-WP13`, i to jest
rozstrzygnięcie z pomiarem, a nie odłożenie. Powód: przegląd motoryzacji wymaga **cyklu życia
pojazdu, którego w repozytorium nie ma**. M4 obsadza flotę raz, przy zasiedlaniu, i od tej chwili
nikt pojazdu nie tworzy ani nie kasuje. Zbudowanie tego znaczy: `drivers` i `by_household` pod
zamek (pięć miejsc odczytu na ścieżce planowania podróży), lista wolnych slotów floty, przenosiny
`losuj_klase` i `bak` z `sim/world` do `sim/traffic` razem z wołaczem w `seed_fleet` (wzorzec
`K-92`), zwalnianie miejsca postojowego przy sprzedaży i despawn encji. To jest **mechanika**,
a nie domknięcie rozjazdu między deklaracją a zachowaniem — czyli dokładnie klasa, której `R2` §2
zabrania, ta sama co przy pozycji 76 („miasto jako pracodawca"). Zostaje przy tym rozdzielone
od przyczyny, którą `R2-WP13` naprawia: pozycja 4 to **zamrożona migawka**, pozycja 13 to
**brakujący mechanizm**. `R2` §3 reguła 2 mówi wprost, co wtedy — własny wiersz i własny pakiet,
nie doklejanie do trwającego. Adresat jest nazwany, kryterium i szew zostają te, które stoją
w szwie `R2-WP13` (próg z histerezą z `data/roads/mode_choice.ron`, przegląd miesięczny).

---

## Zmiany wpisane po R2c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2c.

Numeracja `C-n`, gwiazdka `★` przy numerze = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| C-1 | `R2-WP13`: `TrafficOracle::bump_income(household, delta)` **nie powstaje**. Odświeżenie idzie całą tablicą, raz na dobę, z `TrafficSystem` (`household_incomes`, `K-98`) | Szew kazał wołać oracle z `sim/economy::labor::system`, a `sim/economy` nie zależy od `magnat-traffic`. Nowa krawędź grafu kupowałaby przy okazji wzorzec, który `K-93` właśnie naprawiał: przyrostową denormalizację dochodu, w której nikt nie odpowiada za parowanie `+wage` z `−wage` |
| C-2 ★ | `R2-WP13`: motoryzacja wychodzi do własnego pakietu `R2-WP38`; pozycja 13 wykazu dostaje jego adres | Przegląd motoryzacji wymaga cyklu życia pojazdu, którego w repozytorium nie ma — M4 obsadza flotę raz i nikt jej potem nie rusza. To jest mechanika, nie domknięcie rozjazdu (`R2` §2), i druga przyczyna obok pozycji 4, więc `R2` §3 reguła 2 każe dać jej własny pakiet. Pomiar i pełne uzasadnienie w `D-N12` |
| C-3 | `R2-WP14`: podmiana jest **zmianą wejścia w tej samej recepturze**, a nie zmianą receptury na linii | „Receptura alternatywna" nie istnieje ani w `Recipe`, ani w `data/recipes/`. Substytut jest polem `RecipeInput.substitutes` z proporcją masy i karą jakości — i tak opisuje go katalog od M6a. Przezbrojenie służy zmianie wyrobu, nie wsadu |
| C-4 | `R2-WP14`: `DecisionReason::RecipeSubstituted` **nie powstaje** | `SubstituteUsed = 402` istnieje od M6b, `review` już go zapisuje, a `engine/ui` ma dla niego ramię w obu językach. Drugi wariant opisywałby to samo zdarzenie dwa razy |
| C-5 | `R2-WP14`: `ShortageAction::Substitute` **znika** zamiast dostać wykonawcę; dochodzi sufit kaskady na szczeblu `Substituted` | Wszystko, co niósł wariant, niesie stopień `ShortageStage::Substituted`; podmiany się nie kupuje. Sufit jest wymuszony arytmetyką: pokrycie liczy się z pierwotnego wsadu, więc zostaje zerem także wtedy, gdy zamiennik karmi linię, a `szczebel_progowy(0)` to `Halted` |
| C-6 | `R2-WP16`: `AmbitionGain` wchodzi do `PersonFacts.ambition`, a nie do parametru `switch_threshold_bp` | Skutek nazywa się „ambicja rośnie", a `switch_threshold_bp` już umie ambicję czytać. Drugi parametr opisywałby tę samą rzecz dwa razy i rozjechał się z pierwszym przy pierwszej zmianie wagi |
| C-7 | `R2-WP16`: walidator `owner_phase` wymaga pola dla potrzeby z tempem zero **i** bez miejsc zaspokojenia | Sama reguła „tempo zero" objęłaby `Health`, która jest zdarzeniowa z rozmysłu i ma przychodnię, szpital i aptekę. Pusta lista miejsc jest drugą połową pytania „czy tę potrzebę cokolwiek rusza" |
| C-8 ★ | `R2-WP16`: pozycja 43 wykazu (odbudowa nastroju) wchodzi do zakresu pakietu | `§11` przypisywał ją temu pakietowi, a `§5.8` nie mówił o niej ani słowem — czyli adres bez treści. Po dołożeniu mnożnika deprywacji do `effective_labor` nastrój waży w produkcji więcej niż przedtem, a `MoodLoss` był jego jedynym pisarzem: miasto na stałym −100 mierzyłoby stałą, nie stan |
