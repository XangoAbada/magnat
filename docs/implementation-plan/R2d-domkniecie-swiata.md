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
| R2-WP17 → **M11c** | Kopalnia staje na złożu | `D-N13` (przyjęta) | M | `[ ]` |
| R2-WP18 → **M11c** | Gęstość firm i pasmo bezrobocia | R2-WP17 (WP12 zamknięty w M8c) | L | `[ ]` |
| R2-WP19 | Przechwytywanie rzek w erozji | — | M | `[ ]` |

---

### R2-WP17 — Kopalnia staje na złożu

> **Pakiet wykonuje M11c.** Decyzja właściciela produktu z 2026-09-19 przeniosła go do podfazy
> prezentacyjnej razem z trzema innymi, bo M11c ma pokazać ulicę, a ulica jest pusta i nie ma
> na niej szyldów. Zakres, kryterium i sufit czasu zostają **bez zmian** — zmienia się wyłącznie
> adres wykonania. Powód w tabeli `J-n` dokumentu `M11c-wnetrza-i-kamera.md`; status wraca do
> wykazu R2 po zamknięciu M11c.


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
> adres wykonania. Powód w tabeli `J-n` dokumentu `M11c-wnetrza-i-kamera.md`; status wraca do
> wykazu R2 po zamknięciu M11c.


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
| | *(tabela wypełnia się w trakcie R2d)* | |
