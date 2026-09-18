# R2f — Pomiar i bramki

Podfaza dokumentu naprawczego `R2-naprawy-po-M11.md`. Numeracja sekcji `5.x` jest numeracją
dokumentu R2. Ostatnia podfaza — zamyka R2 i wystawia rachunek.

| | |
|---|---|
| **Wejście** | R2a…R2e zamknięte. R2-WP24 mierzy skutek R2-WP18, więc nie ma sensu wcześniej; R2-WP26 egzekwuje regułę na rejestrze, który R2e właśnie posprzątał. |
| **Pakiety robocze** | R2-WP24…R2-WP26 |
| **Wynik do pokazania** | Raport balansatora z biegu nocnego, w którym **wszystkie jedenaście bramek ma werdykt** — dziś dwie są odfiltrowane, a jedna jest doradcza. Plus wyjście `struct_guard --all` z listą pozycji rejestru bez adresata: ma być pusta. |
| **Kryterium zamknięcia** | Kryteria R2-WP24…R2-WP26 plus siedem kryteriów akceptacji z `R2` §7. To jest moment, w którym R2 się zamyka. |
| **Poprzednia / następna** | `R2e-dlug-i-martwy-kod.md` / `M12a-pamiec.md` |

---

## 5.16 Bramka, która nie może zaświecić na czerwono, nie jest bramką

To zdanie stoi już w R1 przy teście wykrywacza przypisań do salda i jest tam prawdziwe: drugi test
sprawdza sam wykrywacz, bo bramka, która nigdy nie pada, niczego nie chroni.

Ta podfaza stosuje tę samą regułę do trzech rzeczy, które dziś jej nie spełniają:

- **Bramka G11** (bezrobocie 3–12 %) chodzi wyłącznie w biegu nocnym i ma dwa filtry, przez które
  może nie zostać w ogóle policzona. Na świecie z bezrobociem 0,2 % powinna świecić czerwono —
  a jeśli nie świeci, to filtry ją wyłączają i trzeba je nazwać.
- **Bramka G4** (reaktywność szoku) jest doradcza, bo zmierzona stabilizacja wynosi 5 dób wobec
  widełek 14–56. Bramka, której werdykt niczego nie blokuje, jest wykresem.
- **Testy miasta** — jedenaście testów w `sim/world/tests/city.rs`, przeważnie `#[ignore]`, bo
  każdy stawia pełny świat. Bramy, L-system, domknięcie sieci i kwartały są weryfikowane ręcznie
  albo wcale; automatycznie chodzi tylko hash determinizmu, a ten mówi „tak samo jak wczoraj",
  nie „poprawnie".

Czwarta rzecz jest o piętro wyżej i dotyczy samego procesu: **reguła „pozycja rejestru ma
adresata" nie ma egzekutora** (§5.13 w `R2e`). Cztery pozycje przeżyły sześć faz z adresatem
skreślonym, nieistniejącym albo takim, który nie zadziałał.

---

## 5.17 Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R2-WP24 | Bramka bezrobocia naprawdę mierzy bezrobocie | R2-WP18 | M | `[ ]` |
| R2-WP25 | Testy miasta wychodzą z `#[ignore]` | — | M | `[ ]` |
| R2-WP26 | Egzekutor rejestru długu | R2e | S | `[ ]` |

---

### R2-WP24 — Bramka bezrobocia naprawdę mierzy bezrobocie

**Pozycje wykazu:** 38b, 39. Pozycja 38 jest zamknięta jako **nie będąca usterką** — patrz niżej.

**Czego tu nie ma i dlaczego.** Przegląd zgłosił, że profil `ci` balansatora nie mieści się
w dziesięciu minutach: osiem ziaren po 365 dób to ~9,7 minuty **na ziarno**. To jest prawda i jest
**rozstrzygnięte** — `M5e` `AD-6 ★` zapisało pomiar, nazwało źródło kosztu (`U-24`: każdy zakup to
dwa wywołania routera M4) i przyjęło rozwiązanie: bramka pull requesta bierze krótszy przebieg
(4 ziarna × 120 dób), a pełną macierz na czterech scenariuszach puszcza bieg nocny. `ci.yml`
implementuje dokładnie to, z komentarzem wyjaśniającym odstępstwo od §7.4. Uzasadnienie w korekcie
jest lepsze niż moje zgłoszenie: *lepiej mieć bramkę, która biegnie przy każdym pull requeście,
niż zgodną z planem, którą ktoś wyłączy po trzecim przekroczeniu limitu.*

Pozycja 38 idzie więc do wykazu ze statusem `nie dotyczy` i to jest jedyny taki wiersz w R2.

**Co zostaje.** Przy weryfikacji tamtego zgłoszenia wyszło coś ostrzejszego. Profile bramek są
rozdzielone tak: `Nightly` bierze wszystko, `Ci` pomija `G4`, `G6` i `G11`. Bramka **G11 —
bezrobocie w paśmie 3–12 %** — jest więc jedyną, która mogłaby złapać pozycję 6 wykazu (0,2 %
przy 12 032 pustych etatach), i chodzi wyłącznie nocą. Do tego przechodzi przez dwa filtry
(`G11_MIN_DAYS`, warunek ciasnego rynku pracy i filtr ziarna), z których każdy może ją wykluczyć
z oceny w ogóle — a wykluczona bramka nie zwraca werdyktu „czerwony", tylko nie zwraca żadnego.

Czyli: świat z bezrobociem 0,2 % mógł przechodzić przez bieg nocny bez jednej czerwonej lampki
przez całe M7. Tego nie sprawdzono i to jest pierwszy krok pakietu.

`G4` jest osobnym przypadkiem tej samej choroby: ma werdykt, ale werdykt nie blokuje, bo bramka
jest doradcza. Zmierzone 5 dób wobec widełek 14–56 zapisano w `00-postep.md`; liczby nie ma
w żadnym dokumencie fazy, a adresem miało być tarcie rynku B2B z M6 — M6 zamknięte.

**Zakres.**

| Co | Gdzie |
|---|---|
| Pomiar: ile razy w ostatnich stu biegach nocnych G11 zwróciła werdykt, a ile razy została odfiltrowana | `tools/balansator/src/gates.rs` + raport |
| Bramka odfiltrowana zwraca `Skipped { gate, reason }`, nie znika z raportu | `gates.rs::evaluate` |
| Raport nocny wypisuje wszystkie jedenaście bramek z werdyktem albo powodem pominięcia | `tools/balansator/src/report.rs` |
| G4: nowe widełki z pomiaru po R2c i R2d albo jawna decyzja, że zostaje doradcza z powodem | `gates.rs`, `data/tuning/` |
| Test bramki: świat z bezrobociem 0,2 % **musi** dać czerwoną G11 | `tools/balansator/tests/meta_gate.rs` |

**Kryterium:** test odtwarzający — syntetyczny zestaw metryk z bezrobociem 2 ‰ daje werdykt
czerwony dla G11, a nie pominięcie. Przed naprawą daje pominięcie albo nic. Raport nocny zawiera
jedenaście wierszy, nie dziewięć. G4 ma werdykt blokujący albo wiersz w tabeli korekt z powodem,
dla którego zostaje doradcza — trzecia możliwość („jest doradcza i nikt nie wie dlaczego") przestaje
istnieć.

---

### R2-WP25 — Testy miasta wychodzą z `#[ignore]`

**Pozycja wykazu:** 40. Rejestr `R1` `D-R7`; konwencja `#[ignore]` pochodzi z `M2b` `B22`
i `M3d` `H-24`.

**Przyczyna.** Jedenaście testów w `sim/world/tests/city.rs` weryfikuje bramy, L-system, domknięcie
sieci i kwartały. Przeważnie są `#[ignore]`, bo każdy stawia pełny świat 4 km, a to jest kilkanaście
sekund na test. `D-R7` w R1 ma tabelę czterech plików w tej sytuacji — `city_m2d` (3 z 16 pada
poza CI) i `population` (1 z 7) — i odkłada naprawę świadomie, **bez fazy-właściciela**.

Skutek po sześciu fazach: jedynym mechanizmem, który chodzi automatycznie na generatorze miasta,
jest hash determinizmu. Hash mówi „tak samo jak wczoraj". Nie mówi „poprawnie" — a różnicę widać
dokładnie wtedy, gdy ktoś zmieni generator i wszystkie hashe się przeliczy, bo tak trzeba.

**Szew.** Nie skracamy testów — skracamy świat. Miasto 2 km ma tę samą strukturę co 4 km
(bramy, arterie, pierścień, kwartały, strefy, dzielnice), a generuje się w ułamku czasu. Testy
strukturalne nie potrzebują metropolii; potrzebują miasta, które ma wszystkie elementy.

To, czego 2 km nie pokryje — budżety czasu i pamięci na 16 km, macierz hashy — zostaje w biegu
nocnym, gdzie już jest reszta pomiarów.

**Zakres.**

| Co | Gdzie |
|---|---|
| `WorldSize::Km2` jako rozmiar testowy, wyłącznie dla testów | `sim/world/src/params.rs` |
| Jedenaście testów `city.rs` przechodzi na 2 km i schodzi z `#[ignore]` | `sim/world/tests/city.rs` |
| `city_m2c`, `city_m2d`, `population` — te, które da się przenieść, przechodzą | tamże |
| Testy, których nie da się (budżety, macierz hashy) — jawny job nocny, nie `#[ignore]` | `.github/workflows/ci.yml` |
| Tabela w `D-R7` uzupełniona o stan po R2 | `R1-refaktor-po-M5.md` |

**Ostrzeżenie o determinizmie.** Dodanie rozmiaru świata do enumu zmienia jego reprezentację
w hashu parametrów generacji. Wariant idzie **na koniec** enumu; macierz hashy dla istniejących
rozmiarów musi wyjść identyczna, i to jest część kryterium.

**Kryterium:** żaden test w `sim/world/tests/` nie ma `#[ignore]` bez wiersza w `D-R7` mówiącego,
do którego joba nocnego należy. Czas pełnego `cargo test -p magnat-world` poniżej pięciu minut
na maszynie CI. Macierz hashy dla 4 km i 16 km identyczna bit w bit przed i po.

---

### R2-WP26 — Egzekutor rejestru długu

**Pozycja wykazu:** 35.

**Przyczyna.** Reguła z R1 brzmi: pozycja rejestru ma adresata — fazę, która ją otworzy z powodu
innego niż liczba linii. Reguła jest dobra i w trzydziestu przypadkach zadziałała. W czterech nie,
i każdy zawiódł inaczej: adresat skreślony bez zastąpienia (poz. 37), adresat, którego nigdy nie
było, i to zapisane wprost (poz. 33–35), wyzwalacz, który nie strzelił — raz (poz. 24, adres M6a)
i **trzy razy** (poz. 38, adres „pierwsza faza dopisująca czytnik do `retail.ron`", z dwoma
sprostowaniami w samym rejestrze).

Wspólne dla wszystkich czterech: **nic nie sprawdza, czy adresat istnieje i czy jeszcze nie minął.**
Rejestr jest tabelą w pliku markdown, a `struct_guard` mierzy linie w kodzie. Te dwie rzeczy nigdy
się nie spotykają.

**Szew.** `struct_guard` czyta rejestr. Skrypt już wczytuje listę zamrożonych wartości z pliku
(`REJESTR`), więc umie znaleźć tabelę; dokłada się parsowanie kolumny „Ścieżka wyjścia" i sprawdzenie
dwóch rzeczy: czy wskazany dokument istnieje w `docs/implementation-plan/`, i czy faza, którą nazywa,
nie jest już odhaczona w `00-postep.md`.

Pozycja, której adresat zamknął się bez niej, jest **błędem bramki**, nie wpisem w tabeli. To jest
dokładnie ta klasa, która przeżyła sześć faz.

**Zakres.**

| Co | Gdzie |
|---|---|
| Parsowanie rejestru długu z `R1-refaktor-po-M5.md` | `scripts/struct_guard.py` |
| Sprawdzenie: adresat istnieje jako plik `M*.md`/`R*.md` | tamże |
| Sprawdzenie: faza adresata nie jest odhaczona w `00-postep.md` | tamże |
| Pozycja bez adresata albo z adresatem przeterminowanym → kod wyjścia 1 w trybie `--all` | tamże |
| Ten sam test dla pozycji oznaczonych „dziś nie planuje go żadna z M6–M12" — taki wpis jest dopuszczalny **tylko z datą przeglądu** | tamże |
| Wiersz w `CLAUDE.md` przy regule przeglądu strukturalnego | `CLAUDE.md` |

**Kryterium:** test odtwarzający — rejestr z pozycją wskazującą na fazę odhaczoną w `00-postep.md`
daje kod wyjścia 1 z nazwą pozycji. Przed naprawą skrypt jej nie widzi. `python scripts/struct_guard.py
--all` na repozytorium po R2e przechodzi, czyli wszystkie czterdzieści jeden pozycji ma adresata,
który istnieje i jeszcze nie minął. Self-test skryptu rozszerzony o ten przypadek — bo bramka,
która nigdy nie świeci na czerwono, nie jest bramką.

---

## 5.18 Rachunek zamknięcia R2

Ostatni krok podfazy i całego dokumentu. Nie jest pakietem, bo nie wytwarza kodu — jest procedurą.

1. Wykaz `R2` §11 wypełniony do końca: każdy wiersz ma status `zamknięta` z numerem testu,
   `przeniesiona` z imiennym adresatem i powodem, `odrzucona` z powodem albo `nie dotyczy`.
2. Rejestr długu w `R1`: pozycje zamknięte przez R2 przekreślone, numeracja nowych ciągła od **42**.
3. Cztery wpisy `K-58`…`K-61` w `00-konwencje-i-kontrakty.md` §4a — te, które faktycznie powstały.
4. Sześć wpisów w dokumentach faz wcześniejszych wg `R2` §6.
5. `00-postep.md`: sekcja R2 odhaczona, linia w dzienniku z datą, zakresem i tym, co wyszło poza
   plan.
6. Tabela „Zmiany wpisane po R2" w każdym dokumencie podfazy, wypełniona.

Punkt 1 jest kryterium twardym: **R2 nie zamyka się z pozycją bez statusu**. Pozycja bez statusu
jest dokładnie tym, co R2 miało naprawić — usterką, o której wiadomo i której nikt nie przypisał.

---

## 5.19 Decyzje otwarte tej podfazy

**`D-N17` — Co się dzieje z bramką, która zostaje odfiltrowana w każdym biegu.** Propozycja:
po dziesięciu kolejnych biegach nocnych bez werdyktu bramka zgłasza się jako błąd konfiguracji,
nie jako pominięcie. Filtr, który wyklucza bramkę zawsze, jest wyłączeniem bramki napisanym
okrężnie. Wariant łagodniejszy — sam raport bez błędu — jest tym, co mamy dziś, i to właśnie
naprawiamy. *Blokująca dla R2-WP24.*

**`D-N18` — Czy rozmiar świata 2 km wchodzi do gry, czy zostaje tylko w testach.** Propozycja:
tylko w testach, z konstruktorem niedostępnym z CLI. Miasto 2 km ma ~10 tys. mieszkańców, czyli
poniżej progu, przy którym którykolwiek mechanizm ekonomiczny ma sens, a jego obecność w kreatorze
świata (M9a) obiecywałaby graczowi rozgrywkę, której nie ma. *Nieblokująca.*

---

## Zmiany wpisane po R2f

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2f.

| # | Zmiana | Dlaczego |
|---|---|---|
| | *(tabela wypełnia się w trakcie R2f)* | |


### R2-WP29 — Budżety grafu, Gantta i panelu [S]

**Zależności:** M9e (`GraphView`, `GanttView`, `Panels`).

§7 dokumentu M9 podaje trzy budżety, których **nikt nie zmierzył**: graf łańcucha
dostaw 500 węzłów / 2000 krawędzi rysowany w ≤ 0,8 ms i układany w ≤ 50 ms poza
klatką, Gantt 500 pasków w ≤ 0,4 ms, przebudowa pojedynczego panelu w ≤ 2,0 ms.
M9e zdał bramkę „klatka bez zmian danych nie przebudowuje modelu" testem licznika
przebudów — i to jest inna rzecz niż czas.

Benchmark idzie do `engine/ui/benches/ui_bench.rs`, obok pomiarów `Table`, `Series`
i `HeatmapThumb` z M9b, i na tej samej uprzęży bezgłowej.

**Kryterium ukończenia:** trzy pomiary w raporcie `criterion`, każdy z medianą pod
swoim budżetem; graf i Gantt zasilone **danymi z przebiegu**, a nie zbudowanymi na
potrzeby pomiaru (`DE-5` — dokładnie dlatego nie powstały w M9b).
