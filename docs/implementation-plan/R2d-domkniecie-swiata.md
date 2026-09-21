# R2d — Domknięcie świata: gęstość, złoża, rzeki

Podfaza dokumentu naprawczego `R2-naprawy-po-M11.md`. Numeracja sekcji `5.x` jest numeracją
dokumentu R2.

| | |
|---|---|
| **Wejście** | R2-WP12 zamknięte (wiek produkcyjny z danych) — bez niego mianownik stopy bezrobocia jest inny w rynku pracy i w modelu makro, więc R2-WP18 nie ma czego mierzyć. R2-WP17 przed R2-WP18. |
| **Pakiety robocze** | R2-WP17…R2-WP19 |
| **Wynik do pokazania** | `headless generate --size 4km` z rozszerzonym `GenerationReport`: liczba zakładów wydobywczych stojących na złożu, stosunek mieszkańców do firm, liczba przechwyceń rzecznych w erozji. Dziś pierwsza z tych liczb jest zerem, druga wynosi ~130, trzeciej nikt nie liczy. |
| **Kryterium zamknięcia** | Kryteria R2-WP17…R2-WP19 plus: przebieg pięcioletni na mieście 4 km kończy się **stopą bezrobocia w paśmie bramki G11 (3–12 %)** albo pomiarem i decyzją, że nie da się tego osiągnąć bez przeprojektowania Etapu 7 (`D-N6`). |
| **Poprzednia / następna** | `R2c-rozjazdy-danych-i-kodu.md` / `R2e-dlug-i-martwy-kod.md` |
| **Zamknięta** | 2026-09-21, **drugą gałęzią kryterium w obu pozycjach, które ją mają.** Pasmo bramki G11 nie zostało osiągnięte: `R2-WP18` skończył się pomiarem i decyzją `D-N20` (zakład bierze cały budynek — to przeprojektowanie Etapu 7, adres R3), a `R2-WP19` zakończeniem drugim (przechwytywanie działa, ale nie mieści się w budżecie 16 km — `D-8`). `R2-WP17` zamknięty w M11c z testem. Wszystkie trzy pozycje wykazu (5, 6, 42) mają status, a to jest twarde kryterium całego R2. |

---

## 5.10 Najdroższa podfaza i jedyna z sufitem pracy

Pozostałe podfazy R2 naprawiają rzeczy, w których wiadomo, co jest zepsute i gdzie. Ta jedna
zawiera pozycję, przy której **nie wiadomo, czy naprawa jest domknięciem, czy przeprojektowaniem** —
i dlatego jako jedyna w całym R2 ma jawny sufit pracy zapisany w decyzji otwartej.

Pozycja 6 (gęstość firm i bezrobocie) jest najpoważniejszą rzeczą na całym wykazie, bo **psuje
pomiar wszystkiego innego**. Bramki kalibracyjne, hazardy zdarzeń M8c, warunki uzwiązkowienia
M10e, model makro — wszystko mierzy się na świecie, w którym na jednego pracodawcę przypada
sto trzydzieści osób, a dwanaście tysięcy etatów stoi pustych przy bezrobociu 0,2 %. Te dwie
liczby razem znaczą, że rynek pracy nie ma z czym się równoważyć.

Obie liczby są zapisane w `00-postep.md` jako `BF-4` i `BF-10`, z adresem **M2** — fazy zamkniętej
przed pięcioma fazami. Geneza sięga `M7a` `AS-6`.

Trzecia pozycja tej podfazy (przechwytywanie rzek) jest świadomym skrótem z wyliczonym kosztem
naprawy i wchodzi tu tylko dlatego, że dotyka tego samego generatora i tej samej bramki czasu.

---

## 5.11 Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R2-WP17 → **M11c** | Kopalnia staje na złożu | `D-N13` (przyjęta) | M | `[x]` **zamknięte w M11c** |
| R2-WP18 → **M11c** → **R3** | Gęstość firm i pasmo bezrobocia | R2-WP17 (WP12 zamknięty w M8c) | L | `[~]` **zmierzone, sufit `D-N6` zadziałał** (`D-N20`) |
| R2-WP19 | Przechwytywanie rzek w erozji | — | M | `[x]` **zakończenie drugie** (`D-8`) |

---

### R2-WP17 — Kopalnia staje na złożu

> **Pakiet wykonuje M11c.** Decyzja właściciela produktu z 2026-09-19 przeniosła go do podfazy
> prezentacyjnej razem z trzema innymi, bo M11c ma pokazać ulicę, a ulica jest pusta i nie ma
> na niej szyldów. Zakres, kryterium i sufit czasu zostają **bez zmian** — zmienia się wyłącznie
> adres wykonania. Powód w tabeli `J-n` dokumentu `M11c-wnetrza-i-kamera.md`.
>
> **Zamknięty w M11c** (2026-09-19) — z testem; szczegóły w tabeli `J-n` tamtego dokumentu.


**Pozycja wykazu:** 5. Zapisana jako `AQ-8` w `M6-lancuch-dostaw.md` z adresem M7; M7 zamknięte bez niej.

**Przyczyna.** Etap 7 generatora rozstawia zakłady dwiema drogami. Pierwsza — klastry przemysłowe
z szablonami łańcuchów (`data/chains/templates.ron`) — sprawdza dopasowanie do złoża. Druga —
wypełniacz stref nieprodukcyjnych — **nie sprawdza niczego poza strefą**. Archetypy kopalń mają
`needs_deposit` w `data/buildings/primary.ron`, ale trafiają do wypełniacza, a ten pola nie czyta.

Skutek: w mieście 4 km stoi 39 zakładów produkcyjnych i **zero wydobywczych ze złożem**. Cały
model wyczerpywania złoża — koncentracja malejąca z pierwiastkiem sześciennym pozostałej masy,
koszt z członem kwadratowym, samoistne zamykanie się szybu, gdy import staje się tańszy — jest
napisany, przetestowany (`sim/supply/tests/mining.rs`: złoże wyczerpuje się w pięćdziesiąt lat
i szyb staje) i **nie uruchamia się w żadnym wygenerowanym mieście** tej wielkości.

**Szew.** Wypełniacz nie musi umieć szukać złóż — musi umieć **odmówić**. Archetyp z `needs_deposit`
jest z wypełniacza wykluczony, a jego rozstawienie idzie wyłącznie ścieżką klastrową, która
dopasowanie już robi. To jest odwrócenie problemu: zamiast uczyć drugi mechanizm tego, co umie
pierwszy, odbieramy mu prawo do decyzji, której nie umie podjąć.

Pozostaje pytanie, co zrobić, gdy w mieście nie ma złoża odpowiedniego typu. Dziś odpowiedzią jest
milczące postawienie kopalni w próżni. Po naprawie: **zakład nie powstaje**, a domknięcie łańcuchów
(`supply_closure_check`) widzi brak surowca i uruchamia import przez bramę — mechanizm, który już
istnieje i ma przepustowość, elastyczność ceny i cło.

**Zakres.**

| Co | Gdzie |
|---|---|
| Wypełniacz stref pomija archetypy z `needs_deposit` | `sim/world/src/city/sites/place.rs` |
| Rozstawienie przy złożu jako jedyna droga dla tych archetypów | tamże, ścieżka klastrowa |
| `GenerationReport`: liczba zakładów wydobywczych i liczba tych, które stoją na złożu | `sim/world/src/city/report.rs` |
| Ostrzeżenie raportu, gdy profil wymaga wydobycia, a region nie ma złóż | tamże |

**Kryterium:** test odtwarzający — świat 4 km, profil `industrial`, każdy zakład wydobywczy
w `SiteSet` ma niepuste `MiningSite` wskazujące na istniejące złoże. Przed naprawą test pada,
bo zbiór zakładów wydobywczych jest pusty albo żaden nie ma złoża. Drugi test: przebieg
pięćdziesięcioletni na takim świecie kończy się co najmniej jednym szybem zamkniętym z powodu
wyczerpania — czyli mechanizm, który dziś nie chodzi, faktycznie chodzi.

---

### R2-WP18 — Gęstość firm i pasmo bezrobocia

> **Pakiet wykonuje M11c.** Decyzja właściciela produktu z 2026-09-19 przeniosła go do podfazy
> prezentacyjnej razem z trzema innymi, bo M11c ma pokazać ulicę, a ulica jest pusta i nie ma
> na niej szyldów. Zakres, kryterium i sufit czasu zostają **bez zmian** — zmienia się wyłącznie
> adres wykonania. Powód w tabeli `J-n` dokumentu `M11c-wnetrza-i-kamera.md`.
>
> **Skończony w M11c pomiarem, nie kodem** (2026-09-19): sufit `D-N6` zadziałał, a wynik
> pomiaru stoi w `tools/headless/src/population.rs` (`gestosc_firm`). Zakład bierze **cały
> budynek** — 4 186 lokali użytkowych obsługuje 202 zakłady, a `it_office` ma 183 etaty.
> Naprawa jest przeprojektowaniem Etapu 7 i ma adres `D-N20`; pakiet przechodzi do **R3**.


**Pozycja wykazu:** 6. Rozmiar `L`, sufit pracy w `D-N6`.

**Przyczyna — dwie i trzeba je rozdzielić.**

*Gęstość.* Etap 7 liczy liczbę zakładów wytwórczych z propagacji popytu wstecz po hipergrafie
receptur (dwanaście rund) i rozstawia handel oraz usługi normatywnie, zachłannym maksymalnym
pokryciem z zanikiem promienia. Wynik: 215 firm na miasto, czyli jedna na ~130 mieszkańców wobec
obiecanych 6–10 tysięcy firm na 150 tysięcy ludzi, czyli jedna na 15–25. Rozbieżność jest
dziesięciokrotna i nie jest błędem arytmetycznym — normatyw daje tyle, ile daje, bo liczy
**zapotrzebowanie na moc produkcyjną**, a nie **liczbę podmiotów gospodarczych**. Jeden duży
zakład i dziesięć małych zaspokajają ten sam popyt.

*Bezrobocie.* Pięcioletni przebieg kończy się stopą 0,2 % przy 12 032 nieobsadzonych etatach.
To nie jest rynek pracy w równowadze — to rynek, w którym podaż pracy jest wielokrotnie mniejsza
niż popyt, więc każdy, kto chce pracować, pracuje, a etaty stoją puste, bo nie ma ich kim obsadzić.
Przyczyną jest **ta sama liczba z drugiej strony**: normatyw obsady (`per_10000m2` × powierzchnia)
wygenerował więcej stanowisk, niż Etap 8 zasiedlił mieszkańców.

**Szew.** Nie ruszamy normatywu mocy produkcyjnej — on jest poprawny i domyka łańcuchy. Zmienia
się **rozdrobnienie**: ta sama moc rozkłada się na więcej podmiotów, wg rozkładu wielkości firm
z danych (potęgowy, jak w rzeczywistości: dużo mikro, mało dużych). To dokłada firm bez dokładania
etatów.

Osobno i **przed** tym: liczba etatów musi zejść do liczby mieszkańców, a nie odwrotnie. Etap 8
liczy populację z liczby etatów podzielonej przez `(1 + bezrobocie)` — czyli populacja idzie za
etatami. Jeśli etatów jest 12 tysięcy za dużo, to albo normatyw obsady jest za gęsty, albo miasto
4 km jest za małe na tyle zakładów. Pakiet zaczyna się od **pomiaru, która z tych dwóch rzeczy
zachodzi**, i dopiero potem dobiera naprawę.

**Zakres.**

| Co | Gdzie |
|---|---|
| Pomiar: rozkład etatów per typ zakładu wobec populacji, przed jakąkolwiek zmianą | `tools/headless/src/population.rs` |
| Rozkład wielkości firm w `data/site_types/` — pole `size_distribution` | katalog typów zakładu |
| Rozdrobnienie mocy na podmioty wg rozkładu | `sim/world/src/city/sites/place.rs` |
| Bilans etatów wobec populacji jako **ostrzeżenie raportu**, nie cicha rozbieżność | `sim/world/src/city/report.rs` |

**Sufit pracy (`D-N6`).** Dwa dni, próg akceptacji 1 : 40 mieszkańców na firmę wobec obiecanych
1 : 15…1 : 25. Po przekroczeniu sufitu pakiet kończy się **pomiarem i decyzją otwartą**, nie kodem,
i przenosi się do R3 z adresatem. Powód: Etap 7 jest jedynym miejscem w projekcie, w którym
naprawa może wymagać przeprojektowania, a nie domknięcia — a wtedy należy do własnej fazy.

**Kryterium:** test odtwarzający — świat 4 km ma stosunek mieszkańców do firm poniżej 1 : 40,
a liczba nieobsadzonych etatów po pięciu latach nie przekracza 15 % wszystkich etatów. Przed
naprawą pierwsza liczba wynosi ~130, druga ~40 %. Bramka G11 balansatora (bezrobocie 3–12 %)
przechodzi w biegu nocnym — dziś nie przechodzi albo jest odfiltrowana (patrz R2-WP24).

---

### R2-WP19 — Przechwytywanie rzek w erozji

**Pozycja wykazu:** 42.

**Przyczyna.** Przebieg P6 ustala topologię sieci odwodnienia **raz, przed pętlą** czterdziestu
albo osiemdziesięciu iteracji stream-power. W trakcie erozji wysokości się zmieniają, ale
odbiorcy D8 już nie — więc rzeka nie może przechwycić sąsiedniej zlewni. Przechwytywanie rzeczne
kształtuje w rzeczywistości większość dużych dorzeczy; bez niego sieć rzeczna jest tą, którą
wyznaczył szum przed erozją, tylko głębiej wciętą.

Skrót jest świadomy i ma wyliczony koszt w komentarzu: 1,4 sekundy na powtórzenie routingu.

**Szew.** Nie każda iteracja potrzebuje nowej topologii. Routing powtarzany **co N iteracji**
(N z danych) kosztuje `40/N × 1,4 s` zamiast `40 × 1,4 s`. Budżet jest ciasny: generacja 16 km
zajmuje dziś 8,3–8,9 s wobec celu 10 s, więc do dyspozycji jest **około sekundy**. Przy N = 10
dochodzi 5,6 s i nie mieści się; przy N = 40 — jedno powtórzenie w połowie przebiegu — dochodzi
1,4 s i mieści się na styk.

Stąd dwa możliwe zakończenia pakietu, obydwa pełnoprawne:

1. **Jedno przeliczenie topologii w połowie erozji wystarcza**, żeby przechwycenia zachodziły,
   i mieści się w budżecie. Zostaje włączone.
2. **Jedno nie wystarcza, a więcej nie mieści się w budżecie** — pakiet kończy się pomiarem,
   wierszem w tabeli korekt i **podniesieniem komentarza `ponytail:` do rangi decyzji zapisanej
   w `M1`**, z liczbą zamiast oszacowania.

Drugie zakończenie jest pełnoprawne. Ten pakiet jest w R2 nie po to, żeby przechwytywanie
koniecznie powstało, tylko po to, żeby przestało być skrótem bez pomiaru.

**Zakres.**

| Co | Gdzie |
|---|---|
| `erosion.reroute_every: u8` w danych, `0` = wyłączone | `data/geology/erosion.ron` |
| Powtórzenie `flow::route` co N iteracji | `sim/world/src/gen/erosion.rs` |
| Pomiar: liczba komórek, które zmieniły zlewnię, i czas generacji per N | `tools/headless/src/worldgen.rs` |

**Ostrzeżenie o determinizmie.** Zmiana N zmienia **każdy świat z każdego ziarna**. Macierz hashy
terenu (32 ziarna × 5 regionów) wymaga przeliczenia i zatwierdzenia — to jest część kryterium,
nie skutek uboczny.

**Kryterium:** test odtwarzający — na ziarnie, na którym dwie zlewnie sąsiadują przez niski dział
wodny, po erozji z `reroute_every = 40` co najmniej jedna komórka zmieniła zlewnię. Przy
`reroute_every = 0` nie zmienia żadna. Budżet: generacja 16 km poniżej 10 s przy wybranym N,
albo pakiet kończy się zakończeniem drugim — z zatwierdzoną macierzą hashy w obu przypadkach,
bo zakończenie drugie też jest decyzją, a nie brakiem zmiany.

#### Wykonanie — zakończenie drugie (2026-09-21)

Mechanizm powstał i działa; **budżet go nie przyjął**. Pomiar, 8 wątków, `mountain`, ziarno 17:

| | bez przetrasowania | z jednym | cel M1 §10 |
|---|---|---|---|
| 16 km, razem | 8,43 s | **10,27 s** | 10 s |
| 16 km, P6 erozja | 6,27 s | 8,11 s | 5–6 s |
| 4 km, razem | 0,33 s | 0,41 s | 1 s |

Zakończenie pierwsze („jedno przeliczenie wystarcza i mieści się") rozpada się na dwie połowy
i tylko jedna wyszła: przechwytywanie **zachodzi** — 478 395 komórek zmienia ujście (2,8 % mapy),
koryta rosną z 414 km do 441 km, powierzchnia jezior spada o 14 % — ale przetrasowanie kosztuje
**1,38 s** (P4 1,10 s + P5 0,28 s) wobec zapasu 1,2 s w regionie górskim i 1,0 s na nizinie.
Rzadziej się nie da: jedno przetrasowanie na przebieg to już minimum.

Dlatego wartość w danych to **`reroutes: 0`** i jest to decyzja, nie brak zmiany. Zapisana
w `M1-swiat-statyczny.md` §5.7a razem z pomiarem, a komentarz `ponytail:` z oszacowaniem
„1,4 s za powtórzenie" zniknął z `erosion.rs` — bo oszacowanie było zaniżone o 27 %.

**Ścieżka wyjścia prowadzi przez tańsze P4, nie przez rzadsze przetrasowanie.** Priority-flood
chodzi na kopcu binarnym, a wysokości są w milimetrach całkowitych: klucz jest ograniczonym
intem, więc kolejka kubełkowa jest wprost stosowalna i nie zmienia wyniku. Adresata nie ma —
pozycja stoi jako **80** w wykazie §11 dokumentu R2, kandydat do R3 albo do M12 (profilowanie).

**Macierz hashy zatwierdzona bez zmian.** Przy `reroutes = 0` ścieżka jest arytmetycznie ta sama
co przed pakietem — wcięcie progu odpływowego przeniosło się z przygotowania stanu do jego
rozsypania, ale wyrażenie zostało to samo. 160 hashy `terrain_hash_matrix` przechodzi bez
przeliczania i to jest zatwierdzenie, którego wymaga kryterium.

---

## 5.12 Decyzje otwarte tej podfazy

**`D-N13` — Co zrobić, gdy profil gospodarczy wymaga wydobycia, a region nie ma złóż.**
**ROZSTRZYGNIĘTA wg propozycji domyślnej** — właściciel produktu, 2026-09-19, przy przeniesieniu
`R2-WP17` do M11c (`J-5`).
Propozycja: zakład nie powstaje, a domknięcie łańcuchów uruchamia import przez bramę — mechanizm
istnieje i ma przepustowość, elastyczność ceny i cło. Wariant „przesuń profil na inny region"
oznaczałby, że `--region desert --profile industrial` daje inny świat niż deklaruje; wariant
„dosiej złoże" łamie `K-13` (jedno źródło prawdy o terenie). *Blokująca dla R2-WP17.*

**`D-N14` — Czy rozkład wielkości firm jest danymi per typ zakładu, czy jedną krzywą globalną.**
Propozycja: jedną krzywą globalną w `data/economy/`, z mnożnikiem per kategoria typu. Rozkład
per typ to 96 wierszy do skalibrowania i żaden konsument nie odróżni ich od krzywej z mnożnikiem.
*Nieblokująca.*

---

## Zmiany wpisane po R2d

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2d.

| # | Zmiana | Dlaczego |
|---|---|---|
| `D-8`* | `R2-WP19` kończy się **zakończeniem drugim**: mechanizm przetrasowania jest w kodzie i ma test, ale w danych stoi `0`. Kryterium budżetowe („16 km poniżej 10 s") nie zostało spełnione — 10,27 s przy jednym przetrasowaniu | Przetrasowanie kosztuje 1,38 s wobec 1,2 s zapasu. Plan zakładał 1,4 s i „mieści się na styk"; pomiar mówi, że nie mieści się w żadnym regionie. Obie połowy zakończenia pierwszego musiały wyjść, wyszła jedna |
| `D-9`* | Parametr nazywa się **`reroutes`** (ile przetrasowań na przebieg), nie `reroute_every` (co ile iteracji) | Iteracji erozji jest 40 na mapie 4 i 8 km, a 80 na 12 i 16 km. Zapisane w planie `reroute_every = 40` dawałoby **zero** przetrasowań na mapie 4 km i jedno na 16 km — czyli przechwycenia tylko w metropolii, a to dokładnie odwrotnie, niż każe budżet czasu. Jednostka była zła, nie wartość |
| `D-10` | Liczba przechwyconych komórek jest pozycją `GenerationReport` (`WorldStats::basin_captures`, wiersz „przechwycenia rzeczne"), a nie osobnym pomiarem w `tools/headless/src/worldgen.rs` | Zakres pakietu kierował pomiar do scenariusza. Raport już wypisuje czasy per przebieg, więc „czas generacji per N" jest w nim bez dopisywania czegokolwiek — brakowało tylko drugiej liczby. Osobny pomiar w scenariuszu byłby drugą kopią tej samej wiedzy |
| `D-11` | Wcięcie progu odpływowego (`OUTLET_INCISION_M`, `SEDIMENT_INFILL`) przeniosło się z przygotowania stanu erozji do jego rozsypania i nakłada je **tylko ostatni** rozsyp | Przy przetrasowaniu w trakcie erozji misa jest zdejmowana i nakładana z powrotem. Gdyby ścięcie szło razem z nią, każde przetrasowanie ścinałoby jeziora o kolejne sześć metrów i liczba przetrasowań zmieniałaby powierzchnię jezior mocniej niż sama erozja. Przy `reroutes = 0` wyrażenie jest to samo, więc macierz hashy nie drgnęła |
| `D-12`* | Tańsze P4 (kolejka kubełkowa zamiast kopca binarnego w priority-flood) jest **warunkiem** włączenia przechwyceń i dostaje pozycję **80** w wykazie §11 dokumentu R2 — nie w rejestrze długu strukturalnego, który mierzy długość plików, a nie koszt algorytmu | Bez tego `reroutes` nigdy nie będzie większe od zera, bo brakuje 0,2–0,4 s i nie ma innego miejsca, z którego je wziąć. Pozycja bez adresata to dokładnie to, co łapie `R2-WP26` |

`*` = zmiana zakresu albo kryterium.
