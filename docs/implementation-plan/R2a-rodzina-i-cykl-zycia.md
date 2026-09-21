# R2a — Rodzina i cykl życia

Podfaza dokumentu naprawczego `R2-naprawy-po-M11.md`. Numeracja sekcji `5.x` jest numeracją
dokumentu R2 — odesłania „patrz §5.4" nadal działają.

| | |
|---|---|
| **Wejście** | R2 otwarte; M11e zamknięte. Pakiety R2a nie zależą od żadnej innej podfazy R2 — można je prowadzić równolegle z `R2c`. |
| **Pakiety robocze** | R2-WP1…R2-WP6 |
| **Wynik do pokazania** | Przebieg `headless century --years 30` z nową tabelą: liczba uczniów, liczba gospodarstw osieroconych, mediana `edu_level` w kohorcie 25-latków, rozkład rodzajów relacji. Dziś trzy z tych czterech liczb są stałe albo zerowe. **Mediana `edu_level` zostaje zerem i to jest poprawny wynik tego scenariusza** — `century` stawia świat bez katalogu miejsc, więc uczeń dostaje flagę i nie dostaje placówki (`K-74`); tabela wypisuje obok liczbę uczniów **z placówką**, żeby zero nie wyglądało na usterkę. Skutek szkoły mierzy test T4a fazy M8. |
| **Kryterium zamknięcia** | Kryteria R2-WP1…R2-WP6 plus: w przebiegu trzydziestoletnim **udział uczniów w populacji 7–17 lat nie spada poniżej 90 %** i nie zależy od tego, ilu mieszkańców pochodzi z generatora, a ilu z porodów. |
| **Poprzednia / następna** | `R2-naprawy-po-M11.md` §4 / `R2b-pieniadz-gospodarstwa.md` |

---

## 5.1 Dlaczego ta podfaza jest największa

Bo dziura jest jednorodna. Z dwudziestu dziewięciu pozycji wykazu, których plan nie znał w żadnej
postaci, **osiemnaście dotyczy demografii, rodziny, cyklu życia i majątku gospodarstwa**. Nie jest
to zbieranina drobiazgów — to jeden obszar, który przeszedł przez M3a–M3d jako „fundament agenta,
dzień mieszkańca, demografia i społeczeństwo, populacja", a potem nikt do niego nie wrócił,
bo kolejne fazy budowały nad nim, a nie w nim.

Wspólny wzorzec wszystkich sześciu pakietów: **stan nadawany przy generacji świata nigdy nie jest
nadawany później**. Flaga ucznia, przypisanie szkoły, relacja rodzeństwa, wykształcenie — każde
z nich ma dokładnie jedno miejsce zapisu i jest nim Etap 8 generatora albo napływ migracyjny.
Mieszkaniec, który urodził się w grze, przechodzi przez życie bez żadnej z tych rzeczy, a im
dłużej trwa partia, tym większy jest jego udział w populacji.

To jest powód, dla którego pomiar musi iść przez przebieg trzydziestoletni, a nie przez test
jednostkowy: po pięciu latach różnica jest w granicach szumu, po trzydziestu wynosi połowę
populacji.

---

## 5.2 Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R2-WP1 ⇧ | Cykl szkolny w trakcie gry | — | M | `[x]` **wykonane przed R2 (2026-09-18)** jako warunek wejścia M8d, zgodnie z propozycją domyślną `D-N1` (`K-74`) |
| R2-WP2 | Graf rodziny: rodzeństwo, dziadkowie, ochrona wpisu | — | M | `[x]` (`K-59`) |
| R2-WP3 | Gospodarstwo bez cichego przepełnienia | — | S | `[x]` (`K-91`) |
| R2-WP4 | Opiekun prawny i gospodarstwo osierocone | R2-WP3 | M | `[x]` (`K-91`) |
| R2-WP5 | Wykształcenie jako stan zmienny | R2-WP1 | M | `[x]` **wykonane przed R2 (2026-09-18)** jako warunek wejścia M8d, zgodnie z propozycją domyślną `D-N1` (`K-74`) |
| R2-WP6 | Tożsamość rodzinna: nazwisko i cechy | R2-WP2 | S | `[x]` (`D-N4` — odrzucone z powodem) |
| R2-WP35 | Opieka nad dzieckiem poniżej wieku szkolnego | R2-WP1 | M | `[ ]` |
| R2-WP37 | Zmiana robocza idzie za rodzajem zakładu | — | M | `[x]` (`K-92`; poz. 72 wykazu, pierwsza połowa) |

`⇧` = kandydat do wyprzedzenia przed R2 zgodnie z `R2` §2b (`D-N1`).

---

### R2-WP1 — Cykl szkolny w trakcie gry

**Pozycje wykazu:** 1, 11.

**Przyczyna.** `Employment::FLAG_PUPIL` jest zapisywana w dwóch miejscach: `migration.rs` (napływ
migracyjny) i `population/pyramid.rs` (Etap 8 generatora). Placówkę przypisuje `przypisz_szkoly`
w `sim/world/src/population/jobs.rs`, wołane **jeden raz**, z `population/mod.rs`. Nie istnieje
system, który raz na dobę albo raz na rok sprawdza, czy mieszkaniec wszedł w przedział
`[ages.school_start, ages.school_end)` — w przeciwieństwie do emerytury, która taki przebieg ma
w `demography/day.rs`.

Druga połowa przyczyny jest w tym, **gdzie** trzymana jest szkoła. `E-19` w `M3d` zapisało
świadomie, że szkoła ucznia siedzi w `Employment.site`. Konsekwencji nikt nie zapisał:
`Employment::has_job()` to `site != NO_SITE`, więc uczeń wchodzi do indeksu miejsc pracy
w `social.rs` i dostaje relacje `Colleague` z kolegami z klasy. Dla `M10e` §5.9 (warunek
powstania związku zawodowego liczony na spójnej składowej grafu relacji **wśród pracowników
zakładu**) pierwszą kandydatką do uzwiązkowienia jest szkoła podstawowa.

**Szew.** Emerytura jest wzorcem, który już działa i którego nie trzeba wymyślać: hazard
shardowany 1/360 po indeksie encji, przejście stanu na najbliższej dobie shardu po przekroczeniu
progu, `DecisionReason::LifeEvent` z powodem. Cykl szkolny dostaje ten sam kształt, w tym samym
przebiegu dobowym, tuż po emeryturze.

Rozdzielenie ucznia od pracownika idzie przez **jawną metodę**, nie przez nową flagę:
`Employment::is_employed()` = `has_job() && !is_pupil()`, a wszystkie miejsca, które dziś pytają
`has_job()` w znaczeniu „pracuje", przechodzą na nią. Dziewięć miejsc czyta dziś `FLAG_PUPIL`
obok `has_job()` — to jest dokładnie ta lista.

**Zakres.**

| Co | Gdzie |
|---|---|
| Wejście do szkoły w wieku `school_start`, wyjście w `school_end` | `sim/agents/src/demography/day.rs`, obok emerytury |
| Przypisanie placówki dla nowego ucznia | port `SchoolProvider` w `sim/agents::places` — `sim/agents` nie zna katalogu miejsc, implementacja zostaje w `sim/world` |
| `Employment::is_employed()` i przejście dziewięciu wołających | `sim/agents/src/components.rs` + `sim/economy/src/labor`, `sim/world/src/population`, `social.rs` |
| Uczeń poza indeksem miejsc pracy | `sim/agents/src/social.rs` — `by_site` bierze `is_employed()` |
| Relacja klasowa jako `Acquaintance`, nie `Colleague` | `social.rs`; wariant `Classmate` **nie powstaje** (`D-N7`) |

**Ostrzeżenie o determinizmie — skorygowane przy wykonaniu.** Pakiet **nie zajmuje numeru
`StreamId`**: placówkę wybiera ta sama reguła, którą rozdaje je Etap 8 — najbliższa, a przy
równej odległości ta o niższym kluczu — czyli funkcja czysta bez losowania. Strumień
opisywałby mechanizm, którego nie ma. Reguła mieszka w `places::nearest_school` i ma dwóch
wołających: generator i dobowy cykl życia.

Pierwotne brzmienie tego akapitu zakładało, że wybór jest **losowaniem spośród wolnych miejsc
w obwodzie**, i rezerwowało pod nie wolny numer z bloku M3 (140–159). Pojemności placówki
reguła nie zna i nie sprawdza: normatyw obwodu liczy M8d (`ServiceCoverage`, `K-64`),
a przeciążona szkoła obniża jakość usługi, nie odsyła ucznia. Gdyby M8d zmieniło to
rozstrzygnięcie, numer strumienia trzeba będzie wtedy zająć — i wtedy hash każdego świata
z uczniami przestanie się zgadzać z wcześniejszym.

**Kryterium:** test odtwarzający — dziecko urodzone w ticku 0 ma w wieku 7 lat flagę ucznia
i niezerowe `Employment.site`, a w wieku 18 nie ma ani jednego, ani drugiego; przed naprawą test
pada na pierwszym asercie. Przebieg `century --years 30`: udział uczniów w populacji 7–17 lat
≥ 90 % i nie koreluje z udziałem mieszkańców pochodzących z generatora. Test statyczny: żadne
miejsce w `sim/economy/src/labor` nie pyta `has_job()` bez sprawdzenia ucznia.

---

### R2-WP2 — Graf rodziny: rodzeństwo, dziadkowie, ochrona wpisu

**Pozycje wykazu:** 8, 9, 24.

**Przyczyna.** Trzy osobne, ale wszystkie w jednym pliku i jednej funkcji wiążącej.

*Rodzeństwo.* `RelationKind::Sibling` powstaje w całym repozytorium w jednym miejscu — przy
porodzie, w `demography/day.rs`. `spawn_household_aged` w `migration.rs` wiąże **dorosłych
z dziećmi** relacją `Child` w podwójnej pętli i na tym kończy. Dwoje dzieci w gospodarstwie
zasiedlonym przez Etap 8 albo przyniesionym przez napływ migracyjny to dla silnika dwoje obcych
ludzi mieszkających pod jednym dachem.

*Dziadkowie.* Przy porodzie wszyscy domownicy poza matką i jej partnerem są wiązani jako
`Sibling`. W gospodarstwie typu `MultiGen` — a ten typ istnieje i jest rozpoznawany po wieku —
babcia dostaje z wnukiem relację rodzeństwa. Wariantu `Grandparent` w enumie nie ma.

*Ochrona wpisu.* Slab relacji mieści 32 wpisy. Po zapełnieniu `push` podmienia wpis wskazany
przez funkcję wyboru ofiary, a ta patrzy wyłącznie na wagę i datę ostatniego kontaktu. Waga
relacji rodzinnej zanika o 1 za każdy pełny tydzień bez kontaktu i jest podłogowana na 1 — więc
nieodwiedzany ojciec ma wagę 1 i jest **pierwszym kandydatem do usunięcia**, gdy przyjdzie
trzydziesta trzecia znajomość. `M3c` `C-2` opisało mechanizm wypychania; tego, że nie sprawdza
rodzaju relacji, nie zapisał nikt.

**Szew.** Wiązanie rodziny przy spawnie i przy porodzie to dziś dwie różne pętle w dwóch plikach
i to jest przyczyna rozjazdu. Powstaje jedna funkcja `demography::zwiaz_rodzine(world, &[Entity],
day)`, która dostaje skład gospodarstwa i wyprowadza **wszystkie** relacje rodzinne z wieku
i z pozycji w składzie. Oba wołające przechodzą na nią.

**Zakres.**

| Co | Gdzie |
|---|---|
| `RelationKind::Grandparent` **na końcu enumu** (`K-59`) | `sim/agents/src/store.rs`; `odwrotna()` mapuje go na `Child`, bo wnuk ma wobec babci tę samą asymetrię co dziecko wobec rodzica |
| Jedna funkcja wiążąca rodzinę, dwóch wołających | `demography/mod.rs`, wołana z `day.rs::uroda` i `migration.rs::spawn_household_aged` |
| Ofiara wypychania nie może być relacją rodzinną, dopóki jest wpis nierodzinny | `demography/mod.rs::wpisz_relacje` |
| Podłoga wagi rodzinnej podniesiona z 1 na wartość z danych | `data/demography/demography.ron`, pole `social.family_floor` |

**Ostrzeżenie o determinizmie.** Dopisanie wariantu do `RelationKind` przesuwa jego reprezentację
w hashu stanu. Wariant idzie **na koniec** enumu i jest to zapisane w `K-59` jako kontrakt zapisu
gry — tak samo jak `JobRoleId` jest pozycją w pliku ról.

**Kryterium:** test odtwarzający dla każdego z trzech tematów osobno — dwoje dzieci w gospodarstwie
z Etapu 8 ma relację `Sibling` w obie strony; babcia w gospodarstwie `MultiGen` ma z wnukiem
`Grandparent`, a nie `Sibling`; po wpisaniu 40 znajomości ojciec nadal jest w slabie, a wypchnięty
został znajomy. Wszystkie trzy padają przed naprawą. Test własnościowy `prop_relation_symmetry`
rozszerzony o `Grandparent`.

---

### R2-WP3 — Gospodarstwo bez cichego przepełnienia

**Pozycje wykazu:** 10, 27.

**Przyczyna.** Dwa miejsca, w których przekroczenie limitu kończy się **milczeniem**, a nie stanem.

Pierwsze: `uroda` w `demography/day.rs` woła `household::add_member` i **ignoruje zwrócone
`false`**. Dziecko urodzone w gospodarstwie dwunastoosobowym ma `Identity.household` wskazujące
na to gospodarstwo, ale nie ma go w liście członków — nie liczy się do rozmiaru, nie widzi go
klasyfikacja, nie widzą go role. Komentarz w kodzie nazywa to świadomym wyborem i jest to obrona
niezmiennika `prop_no_orphan_household`, a nie decyzja o zachowaniu. Skutek: mieszkaniec istnieje
i nie istnieje naraz, a karta inspekcji gospodarstwa pokazuje inny skład niż karta mieszkańca.

Drugie: `roles` przerywa zbieranie odprowadzanych dzieci na czwartym (`MAX_ESCORTED = 4`) przez
`break`. Piąte dziecko nie jest odprowadzane i **nie ma o tym śladu**.

**Szew.** Limit zostaje — jest zmierzony i wchodzi do budżetu pamięci. Zmienia się to, co się
dzieje po jego przekroczeniu: z „nic" na **stan nazwany i widoczny w karcie inspekcji**.

**Zakres.**

| Co | Gdzie |
|---|---|
| Poród do pełnego gospodarstwa: dziecko zakłada gospodarstwo pochodne w tym samym lokalu, typ `MultiGen` | `demography/day.rs::uroda` + `migration.rs::zaloz_gospodarstwo` |
| `HouseholdFlags::OVERCROWDED` z liczbą osób ponad limit | `sim/agents/src/household.rs` |
| Piąte i dalsze dziecko: `DecisionReason::EscortUnavailable { count }` | `household.rs::roles`, render w `engine/ui` |

**Kryterium:** test odtwarzający — gospodarstwo dwunastoosobowe, poród, po nim suma członków
wszystkich gospodarstw równa się liczbie żywych mieszkańców, a karta gospodarstwa i karta
mieszkańca pokazują ten sam skład. Przed naprawą pierwsza asercja pada. Drugi test: pięcioro
dzieci w wieku szkolnym poniżej progu odprowadzania daje **pięć** wpisów w karcie, z czego jeden
z powodem „brak dorosłego".

---

### R2-WP4 — Opiekun prawny i gospodarstwo osierocone

**Pozycje wykazu:** 20, 28.

**Przyczyna.** Gdy umiera ostatni dorosły, a w gospodarstwie zostają dzieci, **nie dzieje się
nic**. Gospodarstwo trwa, klasyfikacja daje `FamilyWithKids`, lista odprowadzanych jest czyszczona,
bo dorosłych jest zero. Nie ma opiekuna, kurateli ani przeniesienia do krewnych. Słowo „sierota"
nie występuje w kodzie symulacji w żadnym znaczeniu domenowym.

Lustrzany brak po drugiej stronie wieku: emeryt ma dokładnie trzy skutki — zwolniony etat, flagę
i wyższe hazardy zgonu oraz choroby. Nie ma opiekuna, nie ma wpływu na plan dnia domowników,
nie ma domu opieki.

**Szew.** Opiekun jest **polem gospodarstwa**, nie encją i nie instytucją (`D-N2`). Instytucja
opiekuńcza wymagałaby usługi publicznej z obsadą i finansowaniem, czyli należy do `M8d` i tam
powinna powstać. R2 domyka stan nieopisany, nie buduje mechaniki.

**Zakres.**

| Co | Gdzie |
|---|---|
| `Household.guardian: u32` — indeks encji dorosłego opiekuna, `NO_MEMBER` gdy niepotrzebny | `sim/agents/src/household.rs` |
| Wybór opiekuna przy śmierci ostatniego dorosłego: krewny w relacji `Parent`/`Sibling`/`Grandparent` z którymkolwiek dzieckiem, przy remisie najwyższa waga, dalej najniższy indeks encji | `demography/day.rs::smierc` |
| Brak krewnego → dorosły z dzielnicy o najwyższej wadze relacji z którymkolwiek dzieckiem; brak i takiego → dzieci przechodzą do gospodarstwa opiekuna wskazanego przez migrację | `demography/day.rs` |
| Opiekun wchodzi do `roles` jako `escort`/`pickup`/`shopper` mimo że mieszka gdzie indziej | `household.rs::roles` |
| Ten sam mechanizm dla seniora: `guardian` ustawiany przy `Vitals.health` poniżej progu z danych | `demography/day.rs`, próg w `data/demography/demography.ron` |

**Kryterium:** test odtwarzający — gospodarstwo z jednym rodzicem i dwojgiem dzieci, rodzic umiera,
po dobie gospodarstwo ma niepustego opiekuna, a dzieci mają w planie dnia dojazd do szkoły
z odprowadzeniem. Przed naprawą lista odprowadzanych jest pusta. Test własnościowy: w przebiegu
trzydziestoletnim **żadne gospodarstwo z dzieckiem nie ma jednocześnie zera dorosłych i pustego
opiekuna** — sprawdzane na każdej granicy miesiąca.

---

### R2-WP5 — Wykształcenie jako stan zmienny

**Pozycje wykazu:** 21, 22.

**Przyczyna.** `Vitals.edu_level` i `edu_field` ustawia wyłącznie Etap 8 generatora. `uroda` daje
`Vitals` z zerowym wykształceniem i **żaden system tego nie podnosi**. Ukończenie szkoły nie zmienia
niczego: uczeń kończy osiemnaście lat, traci flagę i wchodzi na rynek pracy z wykształceniem
zerowym, czyli gorszym niż ktokolwiek z generatora.

`M8d` §5.3 daje kanał `SkillGrowthMulBps` i `M8` §7 T4a mierzy medianę **umiejętności** 18-latków —
ale umiejętność i wykształcenie to dwa różne pola, a scoring kandydata w `sim/firms` czyta
umiejętność, natomiast sufit umiejętności (`skill_ceiling`) czyta wykształcenie. Bez tego pakietu
każdy rocznik urodzony w grze ma sufit umiejętności na poziomie „bez wykształcenia".

Osobno: `ages.school_start` wynosi 7, więc dziecko 0–6 lat nie ma żadnej instytucji, a rodzic
nie ma z tego powodu ograniczenia w planie dnia. `M3` §2 odesłało przedszkola do M8, `M8d` §5.3
ma `ServiceKind::School(Level)` z poziomem jako parametrem — czyli miejsce jest, treści nie ma.

**Szew.** Wykształcenie rośnie **skokowo, w dwóch momentach** (`D-N3`): ukończenie szkoły w wieku
`school_end` i ukończenie kursu wykupionego komendą gracza. Wariant ciągły wymagałby trzeciego
pola na mieszkańca i nie ma konsumenta, który odróżniłby go od skokowego.

Poziom szkoły nie powstaje w R2. **Tabela „ile lat w szkole daje jaki `edu_level`” weszła
do `data/demography/demography.ron` jako pole `education`, a nie do osobnego pliku
`education.ron`** — walidator musi sprawdzić ją wobec `ages` (próg wyższy niż pełny cykl
szkolny jest nieosiągalny), a osobny plik powtarzałby te same granice w drugim miejscu.
`M8d` podepnie pod nią poziomy placówek. To jest mniejszy
zakres niż wygląda: tabela ma tyle wierszy, ile `edu_level` ma wartości.

**Zakres.**

| Co | Gdzie |
|---|---|
| `edu_level` rośnie przy wyjściu ze szkoły, wg lat faktycznie przechodzonych — **i tylko temu, kto miał placówkę**: miasto bez szkoły w zasięgu zostawia dziecko z samą flagą wieku szkolnego | `demography/day.rs`, obok R2-WP1 |
| `data/demography/education.ron` — próg lat → poziom, z walidatorem pokrycia | nowy plik, ładowany przez `DemographyTable` |
| ~~Żłobek i przedszkole jako przedział wieku bez instytucji~~ — **wyszło z pakietu do `R2-WP35`** (pozycja 66 wykazu). Blokada slotu dorosłego zmienia podaż pracy całego miasta, więc jest własną naprawą z własnym przebiegiem balansatora, a nie polem przy okazji (`R2` §3 pkt 2) | `planner/commitments.rs`, `household::roles` |
| ~~`ages.childcare_end` jako osobne pole~~ — **nie powstaje przed swoim czytelnikiem**: liczba, której nikt nie czyta, wygląda w danych tak samo jak działająca | — |

**Ostrzeżenie o determinizmie.** Ten pakiet zmienia rozkład wykształcenia w populacji, a przez
niego rozkład płac i dochodów. Kryterium mierzy zbieżność, nie równość — patrz ryzyko `N-1`
w dokumencie R2.

**Kryterium:** test odtwarzający — mieszkaniec urodzony w ticku 0, przepuszczony przez pełen cykl
szkolny, ma w wieku 18 lat `edu_level` zgodny z tabelą, a jego sufit umiejętności jest wyższy niż
sufit rówieśnika, który szkoły nie skończył. Przed naprawą oba sufity są równe i minimalne.
Przebieg `century --years 30`: mediana `edu_level` w kohorcie 25-latków nie odbiega od mediany
tej samej kohorty z Etapu 8 o więcej niż jeden poziom.

---

### R2-WP6 — Tożsamość rodzinna: nazwisko i cechy

**Pozycje wykazu:** 25, 26 — **zamykane jako odrzucone**, nie naprawiane.

**Przyczyna, dla której to jest pakiet, a nie skreślenie.** Obie pozycje wyglądają na drobiazgi
do naprawienia w godzinę: osobowość noworodka to osiem niezależnych losowań zamiast mieszanki cech
rodziców, a dziecko zawsze dostaje nazwisko matki, bo zmiana nazwiska po ślubie nie istnieje
(komentarz w `migration.rs` odsyła to do „fazy, która ślub modeluje" — takiej fazy nie ma).

Czego nie widać z tej perspektywy: `M10a` §5.8 opiera **cały most makro↔mezo** na tym, że imię,
nazwisko, płeć i osobowość są funkcją `(world_seed, birth_index)` i nigdy nie są przechowywane.
`MacroCell` ma na mieszkańca osiem bajtów i nie ma gdzie trzymać wyniku mieszania cech rodziców
ani nazwiska zmienionego w połowie życia. Naprawa obu pozycji jest więc zmianą kontraktu M10,
a nie poprawką w `sim/agents`.

**Rozstrzygnięcie (`D-N4`).** Obie pozycje zostają zamknięte jako świadomie odrzucone. Zysk jest
kosmetyczny, koszt to osiem bajtów na mieszkańca w modelu makro plus migracja formatu zapisu.
Jeśli właściciel produktu zdecyduje odwrotnie, obie przenoszą się do `M10a` jako zmiana kontraktu
i wymagają wpisu `K-n`.

**Zakres pakietu** sprowadza się do trzech rzeczy i dlatego ma rozmiar `S`:

| Co | Gdzie |
|---|---|
| Komentarz w `migration.rs` przestaje odsyłać do nieistniejącej fazy i nazywa rozstrzygnięcie | `sim/agents/src/migration.rs` |
| Ten sam komentarz przy losowaniu osobowości noworodka | `demography/day.rs::uroda` |
| Wiersz w `M10a` „Zmiany wpisane po R2": kontrakt cech jako funkcji ziarna jest **wiążący**, a nie domyślny | `M10a-jadro-makro-historia.md` |

**Kryterium:** test statyczny — w `sim/agents` nie ma komentarza odsyłającego do fazy, której
nie ma w `docs/implementation-plan/`. Skaner bierze listę plików `M*.md` i `R*.md` i porównuje
z odesłaniami w komentarzach. To jest jedyny test, który ten pakiet zostawia, i jest celowo
mechaniczny: pakiet nie zmienia zachowania, więc nie ma czego odtwarzać.

---

### R2-WP37 — Zmiana robocza idzie za rodzajem zakładu

**Pozycja wykazu:** 72 (pierwsza połowa). Pakiet powstał przy R2a na polecenie
właściciela produktu; pozycja miała opis i pomiar, a nie miała wykonawcy.

**Przyczyna.** Grafik zmianowy **jest** w projekcie i działa — tylko w jednym miejscu.
`sim/world/src/population/catalog.rs::grafik` rozdaje przy zaludnianiu miasta sześć
wariantów dla handlu (z obsadą sobotnią i niedzielną), cztery brygady ruchu ciągłego
z nocną i jedną zmianę dla biura. Rynek pracy M7 nie wie o tym nic: `matching::post_offers`
wpisuje do **każdej** oferty `ShiftKind::Day`, `bidding::headhunt` to samo, a
`labor::system::hire` do **każdego** etatu `Employment::WEEKDAYS`.

Skutek jest cichy i nieodwracalny. Każda zmiana pracy — a rotacja kadrowa jest codzienna
— spłaszcza mieszkańca do godzin 8–16 od poniedziałku do piątku. Po jednym pokoleniu
całe miasto wychodzi z domu o tej samej minucie, huta stoi w nocy, a sklep w sobotę nie
ma kasjera. Żaden test tego nie łapał, bo każdy z nich patrzył na jedną stronę: testy
generatora sprawdzały `grafik`, testy rynku pracy nie pytały o zmianę wcale.

**Szew.** Reguła przeprowadza się tam, skąd widzą ją obie strony — do `sim/agents`, obok
`ShiftKind`. Wejściem jest `ShiftProfile` (`Office`, `Shop`, `Continuous`, `TwoShift`),
a odwzorowanie na niego robi każda strona z tego, co zna: generator z `SectorId` roli,
rynek pracy z `SiteTypeCategory` rodzaju zakładu. To nie są dwa duplikaty jednej wiedzy,
tylko dwie klasyfikacje tej samej rzeczy — reguła zostaje jedna.

**Zakres.**

| Co | Gdzie |
|---|---|
| `ShiftProfile` i `ShiftKind::schedule(profile, i)` — jedna reguła, dwóch wołających | `sim/agents/src/components.rs` |
| `grafik` znika; zostaje `profil(sector, base) -> ShiftProfile` | `sim/world/src/population/catalog.rs` |
| `SiteTypeCategory::shift_profile()`; `Site.shift_profile` | `sim/firms/src/{catalog,site}.rs` |
| `JobOffer.work_days`; `Workforce::hire` bierze maskę dni argumentem | `sim/economy/src/labor/{offer,mod,system}.rs` |
| Grafik liczony **przy obsadzeniu**, nie przy ogłoszeniu | `sim/economy/src/labor/matching.rs`, `bidding.rs`, `owner_ops.rs` |

**Dlaczego przy obsadzeniu, a nie przy ogłoszeniu.** Jedna oferta wisi na **wszystkie**
wolne etaty stanowiska naraz (`slots`), więc grafik wzięty z niej dałby całej ósemce tę
samą brygadę: huta miałaby ośmiu spawaczy na porannej i nikogo w nocy. Wyszło to
z testu, nie z rozumowania — pierwsza wersja liczyła grafik w `post_offers` i test
`zaklad_o_ruchu_ciaglym_obsadza_takze_noc` pokazał osiem razy `(Early, 31)`.

**Czego ten pakiet nie robi.** Nie rozrzuca wyruszeń **wewnątrz** zmiany. Pięć różnych
godzin startu to nie to samo co rozkład ciągły: o 10:00 ulica jest po tej naprawie
rzadsza, a nie pełna. Druga połowa pozycji 72 zostaje otwarta i jest osobną naprawą,
bo dotyka planera doby, a nie rynku pracy.

**Kryterium:** test odtwarzający — zakład o profilu `Continuous` obsadzony przez rynek
pracy ma wśród zatrudnionych **zmianę nocną** i co najmniej trzy różne pory; maski dni
nie są wszystkie takie same. Przed naprawą test pokazywał osiem razy `(Day, PN_PT)`.
Drugi test: każda z dziesięciu branż ma profil, a żaden z czterech profili nie jest pusty.

---

### R2-WP35 — Opieka nad dzieckiem poniżej wieku szkolnego

**Pozycja wykazu:** 66. Wyszła z `R2-WP5` przy jego wykonaniu, zgodnie z `R2` §3 pkt 2
(„jedna naprawa, jeden powód”).

**Przyczyna.** `ages.school_start` wynosi 7, więc dziecko 0–6 lat nie ma żadnej instytucji,
a rodzic nie ma z tego powodu żadnego ograniczenia w planie dnia. `M3` §2 odesłało żłobki
i przedszkola do M8, `M8d` §5.3 ma `ServiceKind::School(Level)` z poziomem jako parametrem —
miejsce jest, treści nie ma.

**Dlaczego osobno od `R2-WP5`.** Blokada slotu dorosłego **zmienia podaż pracy całego
miasta**: gospodarstwo z dwulatkiem i jednym dorosłym traci pracownika. To przechodzi
przez stopę bezrobocia, przez płace i przez bramkę G11, więc wymaga przebiegu
z balansatorem — a `R2-WP5` miał domknąć wykształcenie i domknął je.

**Zakres.** `ages.childcare_end` w `data/demography/demography.ron` (schemat 3 → 4),
rola opiekuńcza w `household::roles`, zobowiązanie całodobowe w `planner/commitments.rs`,
i **pomiar**: o ile spada podaż pracy i czy bramka G11 zostaje w paśmie.

**Kryterium:** test odtwarzający — gospodarstwo z jednym dorosłym i dzieckiem poniżej
`childcare_end` nie ma tego dorosłego w `job_seekers`, a z dwojgiem dorosłych ma jednego;
przebieg dziesięcioletni pokazuje stopę bezrobocia w paśmie G11 mimo ubytku podaży.

---

## 5.3 Decyzje otwarte tej podfazy

**`D-N7` — Czy powstaje `RelationKind::Classmate`.** Propozycja: nie. Relacja klasowa dostaje
istniejący wariant `Acquaintance` z wagą z danych. Osobny wariant kosztowałby miejsce w enumie,
który jest kontraktem zapisu gry, i nie ma konsumenta, który odróżniłby kolegę z klasy od
znajomego — `M10e` §5.9 pyta wyłącznie o relacje wśród pracowników zakładu, a uczeń po R2-WP1
pracownikiem nie jest. *Blokująca dla R2-WP1.*

**`D-N8` — Co się dzieje z dzieckiem, gdy nie ma żadnego kandydata na opiekuna.** Propozycja:
dziecko wyprowadza się z miasta razem z rejestracją w `Population::emigrated`, tak samo jak
dorosły bez pracy i bez lokalu. Wariant „gospodarstwo instytucjonalne" wymaga usługi publicznej
i należy do `M8d`. Wariant „dziecko zostaje samo" jest tym, co jest dziś, i to właśnie naprawiamy.
*Blokująca dla R2-WP4.*

---

## Zmiany wpisane po R2a

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2a.

| # | Zmiana | Dlaczego |
|---|---|---|
| A-1 ★ | **`RelationKind::Grandparent` jest symetryczny, a nie odwracany na `Child`.** §5.2 zapisywało „`odwrotna()` mapuje go na `Child`, bo wnuk ma wobec babci tę samą asymetrię co dziecko wobec rodzica" — i to zdanie było błędne. Wykonane wg nowego brzmienia, wpisane w `K-59` | `kind` znaczy „kim jest dla mnie ten drugi", więc wpis po stronie babci brzmiałby `Child` i byłby nieodróżnialny od wpisu o własnym dziecku. `spadkobiercy` filtruje dokładnie po `Child`, więc wnuk dziedziczyłby po równo z dziećmi — czyli graf rodziny zmieniłby podział spadku, który należy do `R2-WP10`. Kierunek i tak odczytuje się z wieku, którym obie strony dysponują |
| A-2 | **Reguła wiązania wychodzi z pokoleń, a nie z pozycji w składzie.** §5.2 zapowiadało funkcję, która „wyprowadza wszystkie relacje z wieku i z pozycji w składzie". Pozycja nie weszła: `zwiaz_rodzine` bierze sam wiek, a rodzeństwo wyprowadza ze **wspólnego rodzica**, nie z bliskości wieku | Pozycja w składzie znaczy co innego u każdego wołającego — w napływie migracyjnym dorośli stoją przed dziećmi, a przy porodzie skład jest dowolny. Bliskość wieku jako kryterium rodzeństwa zrobiłaby rodzinę z czworga współlokatorów w akademiku. Poród, który zna matkę i ojca, wiąże ich **jawnie przed** tą funkcją, a ona istniejącego wpisu nie nadpisuje |
| A-3 ★ | **Opiekun powstaje w dobowym przeglądzie, a nie w `day.rs::smierc`.** §5.2 wskazywało zgon jako miejsce wyboru opiekuna. Pierwsza wersja tak to zrobiła i test własnościowy złapał gospodarstwo osierocone **przez migrację**, nie przez zgon | Ostatniego dorosłego zabierają domowi cztery zdarzenia w czterech plikach: zgon, wyprowadzka do własnego lokalu, usamodzielnienie i wyjazd z miasta. Hak w każdym z nich to cztery kopie jednej reguły (`R2` §3 pkt 2). Jedno przejście po gospodarstwach na dobę kosztuje ułamek przejścia po mieszkańcach, które ta doba i tak wykonuje |
| A-4 ★ | **Trzecim krokiem wyboru opiekuna jest pierwszy dorosły z dzielnicy, a wyjazd z miasta zostaje przypadkiem zwyrodniałym.** `D-N8` proponowało wyjazd przy braku kandydata; przyjęte z zawężeniem | Pierwsza wersja deportowała dzieci, które nie zdążyły poznać nikogo spoza domu — czyli karała za brak znajomości, a nie za brak opieki. Objaw wyszedł natychmiast: trzy testy `school.rs` padły, bo sześciolatek wyjeżdżał z miasta, zanim zdążył pójść do szkoły. Po zawężeniu wyjazd wymaga dzielnicy bez ani jednego dorosłego |
| A-5 | **Skaner odesłań z `R2-WP6` widzi `M*`, a nie `M*` i `R*`.** §5.2 pisało „skaner bierze listę plików `M*.md` i `R*.md`" | `R<n>` w komentarzu znaczy „ryzyko R<n> z dokumentu tej fazy", a każdy dokument planu ma własną numerację ryzyk — `R1`…`R13` stoją w komentarzach `sim/agents` kilkanaście razy i żadne z nich nie jest adresem. `R2a` i dalsze są jednoznaczne, bo ryzyka nie mają litery podfazy, i te skaner widzi |
| A-6 | **Test własnościowy `R2-WP4` stawia świat pod niezmiennik.** Kryterium §5.2 mówi „w przebiegu trzydziestoletnim żadne gospodarstwo z dzieckiem…"; przebieg na świecie z zasiedlenia przechodził **także bez naprawy** | Zasiedlenie rozdaje rozmiary z `migration.arrival_sizes` i samotny rodzic z dziećmi nie wychodzi z niego ani razu, a przy naturalnej śmiertelności trzydziestolatka osierocenie nie zdarza się przez pięć lat w ogóle. Niezmiennik był spełniony tożsamościowo, czyli mierzył własny brak (`R2` §7 pkt 1). Test stawia więc dwadzieścia gospodarstw samotnego rodzica, postarza rodziców pod `ages.max` i **liczy osierocenia**, żeby to się nie powtórzyło |
| A-7 | **`R2-WP35` nie wchodzi w tę podfazę.** Tabela §5.2 wymienia go wśród pakietów R2a; kryterium zamknięcia podfazy wymienia tylko WP1–WP6 | Blokada slotu dorosłego zmienia podaż pracy całego miasta, więc wymaga przebiegu z balansatorem i własnego pomiaru bramki G11 — to jest własna naprawa z własnym powodem (`R2` §3 pkt 2), a nie pole przy okazji. Pakiet zostaje w R2a jako niezamknięty |
| A-9 | **Trzecia z czterech liczb „wyniku do pokazania” jest w tym scenariuszu zerem z definicji, nie z usterki.** Tabela wypisuje obok liczbę uczniów **z placówką** | `century` buduje świat z płaskich list lokali i etatów, bez `PlaceCatalog` — więc `places::nearest_school` nie ma czego znaleźć, uczeń zostaje z samą flagą wieku szkolnego, a `edu_level` przy wyjściu ze szkoły zostaje zerem. Tak `K-74` to rozstrzygnęło i tak ma być: miasto bez szkoły w zasięgu nie wykształca nikogo. Zero bez drugiej liczby obok wygląda jednak dokładnie tak samo jak zepsuty cykl szkolny, a to jest ten sam błąd pomiaru, przed którym broni `R2` §7 pkt 1 |
| A-8 ★ | **Pozycja 72 wykazu dostaje wykonawcę: `R2-WP37`, i zamyka się w połowie.** Pakiet nie był w planie R2a; powstał z polecenia właściciela produktu przy tej podfazie | „Plan doby wysyła wszystkich w tej samej minucie" miało dwie przyczyny, nie jedną. Pierwsza jest naprawiona: rynek pracy M7 wpisywał każdemu zmianę dzienną i dni robocze, więc grafik generatora znikał po pierwszej zmianie pracy (`K-92`). Druga zostaje: nawet w obrębie jednej zmiany wszyscy wyruszają co do minuty razem, bo plan doby nie ma rozrzutu. Druga połowa jest pomiarem, nie domysłem — wiersz 72 zostaje `[~]` |
