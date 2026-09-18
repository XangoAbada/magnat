# R2f — Pomiar i bramki

Podfaza dokumentu naprawczego `R2-naprawy-po-M11.md`. Numeracja sekcji `5.x` jest numeracją
dokumentu R2. Ostatnia podfaza — zamyka R2 i wystawia rachunek.

| | |
|---|---|
| **Wejście** | R2a…R2e zamknięte. R2-WP24 mierzy skutek R2-WP18, więc nie ma sensu wcześniej; R2-WP26 egzekwuje regułę na rejestrze, który R2e właśnie posprzątał. |
| **Pakiety robocze** | R2-WP24…R2-WP26, R2-WP29, R2-WP33, R2-WP34 |
| **Wynik do pokazania** | Raport balansatora z biegu nocnego, w którym **wszystkie jedenaście bramek ma werdykt** — dziś dwie są odfiltrowane, a jedna jest doradcza. Plus wyjście `struct_guard --all` z listą pozycji rejestru bez adresata: ma być pusta. |
| **Kryterium zamknięcia** | Kryteria R2-WP24…R2-WP26, R2-WP29, R2-WP33 i R2-WP34 plus siedem kryteriów akceptacji z `R2` §7. To jest moment, w którym R2 się zamyka. |
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

Piąta jest tą samą chorobą w drugim miejscu: **poprawka wędrująca w przód też nie ma egzekutora**.
Tabela korekt potrafi wskazać dokument docelowy, a rzeczy w nim nie ma — i tak zniknęły obie
pozycje, które `M8` `CJ-9` obiecał temu dokumentowi. Obie połówki naprawia R2-WP26, bo to jedna
przyczyna i jeden wzorzec skryptu.

---

## 5.17 Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R2-WP24 | Bramka bezrobocia naprawdę mierzy bezrobocie | R2-WP18 | M | `[ ]` |
| R2-WP25 | Testy miasta wychodzą z `#[ignore]` | — | M | `[ ]` |
| R2-WP26 | Egzekutor rejestru długu i poprawek wędrujących w przód | R2e | M | `[ ]` |
| R2-WP29 | Budżety grafu, Gantta i panelu zmierzone | M9e | S | `[ ]` |
| R2-WP33 | Linia bazowa benchmarków mierzy wszystkie | — | S | `[ ]` |
| R2-WP34 | Scenariusz `export_drains` | R2-WP32 | M | `[ ]` |

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

### R2-WP26 — Egzekutor rejestru długu i poprawek wędrujących w przód

**Pozycje wykazu:** 35, 64.

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

**Druga połowa pakietu, dopisana po przeglądzie sesji (poz. 64).** Rejestr długu ma przynajmniej
regułę, którą da się egzekwować. **Tabele korekt nie mają żadnej** — a niosą dokładnie ten sam
rodzaj zobowiązania: wiersz mówi „to idzie do `M10-glebia.md`" albo „dwie pozycje do wykazu R2",
i nikt nigdy nie sprawdza, czy dojechało. Sprawdzenie dwudziestu takich wierszy wskazujących inny
plik dało **osiem bez pokrycia pod wskazanym adresem**. Większość okazała się nieszkodliwa — rzecz
istniała w kodzie albo w dokumencie sąsiednim — ale dwa przypadki były prawdziwą stratą i oba
dotyczyły tego dokumentu: `M8` `CJ-9` obiecał wykazowi R2 dwie pozycje, których w nim nie było
(dziś 58 i 59).

Mechanizm psucia jest ten sam co przy rejestrze: **wiedzę ma ten, kto ją właśnie zdobył**, a wpis
w cudzym dokumencie jest jedyną rzeczą, która ją przenosi. Jeśli wpis nie powstanie albo powstanie
pod adresem, którego nikt nie odwiedzi, K-18 działa na papierze.

**Szew dla drugiej połowy.** Osobny skrypt, nie rozbudowa `struct_guard` — bo to jest sprawdzenie
dokumentów, a nie kodu, i nie ma powodu, żeby bramka strukturalna padała przez markdown. Parsowanie
jest proste, bo tabele mają ustalony kształt po ujednoliceniu nagłówków w R2-WP23: wiersz z nazwą
pliku `.md` w drugiej kolumnie deklaruje adresata.

Sprawdzenie jest **słabe i takie ma być**: czy plik docelowy istnieje i czy zawiera kod korekty
albo charakterystyczny identyfikator z jej treści. Silniejsze sprawdzenie (czy rzecz naprawdę
została opisana) wymagałoby czytania ze zrozumieniem i skończyłoby się wyłączeniem bramki po
trzecim fałszywym alarmie — dokładnie tak, jak `AD-6` opisuje los bramki, która przeszkadza.

**Zakres.**

| Co | Gdzie |
|---|---|
| Parsowanie rejestru długu z `R1-refaktor-po-M5.md` | `scripts/struct_guard.py` |
| Sprawdzenie: adresat istnieje jako plik `M*.md`/`R*.md` | tamże |
| Sprawdzenie: faza adresata nie jest odhaczona w `00-postep.md` | tamże |
| Pozycja bez adresata albo z adresatem przeterminowanym → kod wyjścia 1 w trybie `--all` | tamże |
| Ten sam test dla pozycji oznaczonych „dziś nie planuje go żadna z M6–M12" — taki wpis jest dopuszczalny **tylko z datą przeglądu** | tamże |
| Wiersz w `CLAUDE.md` przy regule przeglądu strukturalnego | `CLAUDE.md` |
| `scripts/plan_guard.py`: wiersz tabeli korekt wskazujący plik `.md` musi mieć w nim pokrycie — kod korekty albo identyfikator z treści | nowy skrypt |
| Wiersz wskazujący dokument **odhaczony** w `00-postep.md` bez pokrycia → kod wyjścia 1 | tamże |
| Job w CI obok `struct-guard`, bez `continue-on-error` | `.github/workflows/ci.yml` |

**Kryterium:** test odtwarzający dla obu połówek. Pierwsza — rejestr z pozycją wskazującą na fazę
odhaczoną w `00-postep.md` daje kod wyjścia 1 z nazwą pozycji; przed naprawą skrypt jej nie widzi.
`python scripts/struct_guard.py --all` na repozytorium po R2e przechodzi, czyli wszystkie
czterdzieści jeden pozycji ma adresata, który istnieje i jeszcze nie minął. Druga — `plan_guard`
uruchomiony na **stanie sprzed R2** znajduje `CJ-9` i wypisuje go jako niespełnioną obietnicę;
uruchomiony na stanie po R2 przechodzi. Self-test obu skryptów rozszerzony o te przypadki — bo
bramka, która nigdy nie świeci na czerwono, nie jest bramką.

---

### R2-WP33 — Linia bazowa benchmarków mierzy wszystkie

**Pozycja wykazu:** 61. To jest `D-R8` z R1 — decyzja otwarta z propozycją domyślną, opisana
w `00-postep.md` jako „otwarta świadomie i z adresem". Adresu nie wpisano nigdzie i R1 zamknęło
się bez niej.

**Przyczyna.** `benches/baseline.json` nie był aktualizowany od M3. Z czterdziestu jeden pozycji
**trzynaście zgłasza `NOWY — brak w linii bazowej`**: cały planer (`plan_day`,
`plan_day_explained`, `replan`), mikro pieszych, `estimate` z cache, trzy pozycje demografii
i społeczeństwa, Etap 8 (`m3d-1`), cztery pozycje indeksu parcel i jeden shard potrzeb.

Dla nich `bench_guard` **nie mierzy niczego — i robi to cicho**, bo brak wpisu jest informacją,
nie błędem. To jest ta sama klasa co pozycja 38b: bramka raportuje zielono, nie sprawdzając tego,
co myśli, że sprawdza. Różnica jest taka, że G11 miała przynajmniej filtr, który dało się nazwać;
tutaj nie ma nawet tego.

Znalezione przy domknięciu R1, przy **pierwszym w całym R1** przebiegu kryterium akceptacji nr 3 —
czyli sam fakt, że bramka chodziła raz na dwanaście pakietów, jest częścią usterki.

**Szew.** Odnowienie linii bazowej idzie **osobnym commitem**, bez żadnej innej zmiany — tak samo
jak `cargo fmt` całego repozytorium i z tego samego powodu: zapisanie liczb razem ze zmianą, która
na nie wpływa, zamienia dowód w założenie. Commit powstaje **po R2e**, bo wcześniejsze pakiety R2
zmieniają wydajność w miejscach, które właśnie mierzymy.

Drugi krok jest ważniejszy od pierwszego: **brak wpisu przestaje być informacją i staje się
błędem**. Benchmark bez linii bazowej to benchmark, którego nikt nie ogląda.

Trzeci krok zamyka pytanie, którego `D-R8` nie rozstrzygnął: kiedy linię bazową się odnawia.
Propozycja jest w `D-N21`.

**Zakres.**

| Co | Gdzie |
|---|---|
| Odnowienie `benches/baseline.json` na sprzęcie odniesienia, osobnym commitem | `benches/baseline.json` |
| `bench_guard`: pozycja bez wpisu w linii bazowej → kod wyjścia 1, nie komunikat `NOWY` | `scripts/bench_guard.py` |
| Wyjątek dla benchmarku **nowego w tym commicie**: dopuszczalny tylko razem z dopisaniem go do linii bazowej w tym samym commicie | tamże |
| Reguła odnawiania linii bazowej zapisana przy `D-8` | `M0-fundament-silnika.md`, `CLAUDE.md` |
| Tabela w `D-R8` uzupełniona o stan po R2 | `R1-refaktor-po-M5.md` |

**Kryterium:** test odtwarzający — `bench_guard` uruchomiony na linii bazowej z usuniętą jedną
pozycją zwraca kod wyjścia 1 i nazwę brakującego benchmarku. Przed naprawą zwraca zero i słowo
`NOWY`. Po odnowieniu wszystkie czterdzieści jeden pozycji ma wpis, a `--all` przechodzi.
Self-test skryptu rozszerzony o ten przypadek.

---

### R2-WP34 — Scenariusz `export_drains`

**Pozycja wykazu:** 62. Zależny od R2-WP32, bo mierzy ceny w przebiegu z ruchem, a do R2-WP32
niezmiennik świata w takim przebiegu się nie domyka.

**Przyczyna.** `M6` §7.7 wymienia `export_drains` jako test kryterium WP9: „wzrost ceny
zewnętrznej o 40 % → mierzalny odpływ masy **i wzrost cen lokalnych**, bez zaprogramowanej
reguły". Kryterium ma dwie połowy i tylko pierwsza została zmierzona.

`AH-12` w M6c zawęziło kryterium do samego drenażu masy — słusznie, bo ceny nie drgną, dopóki
półka nie kupuje z rynku B2B — a `AI-8` przeniosło pomiar cen **za WP11**, czyli do M6e. M6e
zamknęło się bez niego. Scenariusza nie ma ani w `data/scenarios/`, ani nigdzie w kodzie: nazwa
`export_drains` występuje wyłącznie w dwóch dokumentach planu.

Skutek: **druga połowa kryterium fazy M6 nie została zmierzona i nic tego nie pilnuje**. Faza jest
odhaczona, kryterium jest zawężone, a zawężenie miało być tymczasowe.

To jest ten sam wzorzec co poz. 64: zawężenie z adresatem jest dobrą praktyką dokładnie tak długo,
jak długo ktoś sprawdza adresata.

**Szew.** Scenariusz idzie do `tools/headless` obok pozostałych, na tej samej uprzęży co `m6rynek`.
Szok jest jednorazowy i deterministyczny: cena zewnętrzna towaru z wysokim udziałem eksportu rośnie
o 40 % w ustalonej dobie, reszta świata bez zmian. Mierzy się dwie krzywe — masa wychodząca
z miasta i cena półkowa tego samego towaru — i pyta o **kierunek i opóźnienie**, nie o wartość.

Żadnej reguły „eksport podnosi ceny" się nie pisze i to jest sedno testu: jeśli cena nie drgnie,
to znaczy, że kanał między rynkiem B2B a półką nie istnieje, i to jest wynik, a nie porażka
pomiaru.

**Zakres.**

| Co | Gdzie |
|---|---|
| Scenariusz `export_drains` z szokiem ceny zewnętrznej +40 % w ustalonej dobie | `tools/headless/src/export_drains.rs`, `data/scenarios/` |
| Pomiar: masa wychodząca z miasta i mediana ceny półkowej towaru, doba po dobie | tamże |
| Wpięcie do biegu nocnego obok pozostałych scenariuszy | `.github/workflows/ci.yml` |
| Kryterium WP9 w `M6` §7.7 wraca do pełnego brzmienia albo dostaje wiersz z powodem zawężenia **na stałe** | `M6-lancuch-dostaw.md` |

**Kryterium:** przebieg scenariusza pokazuje odpływ masy (to działa dziś) **oraz** wzrost mediany
ceny półkowej tego samego towaru w ciągu 14 dób od szoku. Jeśli cena nie drgnie, pakiet zamyka się
pomiarem i wierszem w tabeli korekt `M6` nazywającym brakujące ogniwo — to jest dopuszczalne
zamknięcie i lepsze niż dzisiejszy brak pomiaru, bo zostawia liczbę zamiast ciszy.

---

## 5.18 Rachunek zamknięcia R2

Ostatni krok podfazy i całego dokumentu. Nie jest pakietem, bo nie wytwarza kodu — jest procedurą.

1. Wykaz `R2` §11 wypełniony do końca: każdy wiersz ma status `zamknięta` z numerem testu,
   `przeniesiona` z imiennym adresatem i powodem, `odrzucona` z powodem albo `nie dotyczy`.
2. Rejestr długu w `R1`: pozycje zamknięte przez R2 przekreślone, numeracja nowych ciągła od **42**.
3. Sześć wpisów `K-58`…`K-63` w `00-konwencje-i-kontrakty.md` §4a — te, które faktycznie powstały.
4. Jedenaście wpisów w dokumentach faz wcześniejszych wg `R2` §6.
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

**`D-N21` — Kiedy odnawia się linia bazowa benchmarków.** Pytanie zostawione otwarte przez `D-R8`
w R1. `D-8` z M0 mówi „przy świadomej zmianie wydajności", a praktyka sześciu faz pokazała, że
wtedy nie robi tego nikt. Propozycja: **przy zamknięciu każdej fazy**, jednym commitem bez innych
zmian, z zapisem sprzętu odniesienia. Wariant z M0 jest czystszy teoretycznie i przegrał
empirycznie. *Blokująca dla R2-WP33.*

**`D-N18` — Czy rozmiar świata 2 km wchodzi do gry, czy zostaje tylko w testach.** Propozycja:
tylko w testach, z konstruktorem niedostępnym z CLI. Miasto 2 km ma ~10 tys. mieszkańców, czyli
poniżej progu, przy którym którykolwiek mechanizm ekonomiczny ma sens, a jego obecność w kreatorze
świata (M9a) obiecywałaby graczowi rozgrywkę, której nie ma. *Nieblokująca.*

---

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

---

## Zmiany wpisane po R2f

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2f.

| # | Zmiana | Dlaczego |
|---|---|---|
| | *(tabela wypełnia się w trakcie R2f)* | |
