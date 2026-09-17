# R2b — Pieniądz gospodarstwa i utarg zakładu

Podfaza dokumentu naprawczego `R2-naprawy-po-M11.md`. Numeracja sekcji `5.x` jest numeracją
dokumentu R2.

| | |
|---|---|
| **Wejście** | R2a zamknięte — R2-WP8 i R2-WP10 dotykają rozwiązania gospodarstwa, które R2-WP4 właśnie przedefiniował (gospodarstwo z opiekunem nie rozwiązuje się przy śmierci ostatniego dorosłego). |
| **Pakiety robocze** | R2-WP7…R2-WP11 |
| **Wynik do pokazania** | `headless m5shop --days 3600` z rozszerzoną sekcją „pieniądz": osobny wiersz dla gospodarstw rozwiązanych, dla spadków i dla kwot, które dziś znikają. Dziś ta sekcja domyka się co do grosza **tylko dlatego, że gospodarstwa nie są kontami w `Books`**. |
| **Kryterium zamknięcia** | Kryteria R2-WP7…R2-WP11 plus: w przebiegu dziesięcioletnim **suma sald gospodarstw plus rejestr emisji zgadza się co do grosza po tysiącu rozwiązanych gospodarstw**, a liczba zakładów produkcyjnych z niezerowym utargiem równa się liczbie zakładów produkcyjnych. |
| **Poprzednia / następna** | `R2a-rodzina-i-cykl-zycia.md` / `R2c-rozjazdy-danych-i-kodu.md` |

---

## 5.4 Dlaczego niezmiennik P1 tego nie łapie

To jest pytanie, które trzeba zadać od razu, bo `P1` — „suma sald równa się podaży pieniądza,
tolerancja zero groszy" — jest najmocniejszym testem w projekcie i chodzi na milionie operacji.

Odpowiedź: **saldo gospodarstwa domowego nie jest kontem w `Books`.** Mieszka w komponencie
`Household` w `sim/agents`, bo gospodarstwo jest własnością M3, a księgi powstały w M5. Przepływ
między nimi idzie przez dwa kanały ewidencji (`household_sector_in` / `household_sector_out`),
które **odnotowują ruch, a nie saldo po drugiej stronie**. Kiedy encja gospodarstwa jest
despawnowana razem z gotówką, konta miasta nic nie tracą, rejestr emisji nic nie widzi i `P1`
przechodzi.

Ten sam mechanizm ukrywa trzy dalsze pozycje tej podfazy. Wniosek dla R2: **niezmiennik musi
objąć gospodarstwa** (`K-61`), a dopiero potem naprawy mają czym być sprawdzone.

Kolejność pakietów wynika z tego wprost: R2-WP8 najpierw rozszerza niezmiennik i dopiero potem
naprawia rozwiązanie gospodarstwa — bo test, który nie umie zobaczyć usterki, nie jest testem.

---

## 5.5 Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R2-WP7 ⇧ | Utarg zakładu produkcyjnego | — | M | `[ ]` |
| R2-WP8 | Majątek gospodarstwa przy rozwiązaniu i podziale | — | M | `[ ]` |
| R2-WP9 | Dochód gospodarstwa po zdarzeniu życiowym | — | S | `[ ]` |
| R2-WP10 | Dziedziczenie ponad gotówkę osobistą | R2-WP8 | M | `[ ]` |
| R2-WP11 | Skala ekwiwalentna gospodarstwa | — | S | `[ ]` |

---

### R2-WP7 — Utarg zakładu produkcyjnego

**Pozycja wykazu:** 2. Zapisana pięć razy w tabelach korekt M7 (`AR-7` → `AV-2` → `BB-7` →
`BD-6` → `BF-7`, plus `M7e` `BC-8` i `M7c` `AX-13`) i przenoszona za każdym razem. Docelowy adres
brzmiał „M8" — i w dokumentach M8a…M8e nie ma dla niej pakietu.

**Przyczyna.** `Firms::post_revenue` ma w całym repozytorium **jednego wołającego**:
`sim/economy/src/systems.rs`, w ścieżce domknięcia miesiąca. Wyniki, które tam wchodzą, pochodzą
z `Market::close_month_with`, a ta funkcja iteruje po `m.shops` — czyli po sklepach. Zakład
produkcyjny bez półki nigdy nie dostaje wpisu, więc `SitePnlMonth.revenue` zostaje zerem.

Skutek jest cichy i dokładnie odwrotny do zamierzonego. `SitePnlMonth::margin_bp()` zwraca
**`None`** przy zerowym utargu — i to jest dobra decyzja, zapisana świadomie: „nie wiem" i „strata"
to dwa różne zdania. Tier taktyczny AI liczy miesiące straty wstecz i **przerywa liczenie na
pierwszym miesiącu bez pomiaru**. Fabryka nie ma pomiaru nigdy, więc jest **strukturalnie odporna
na zamknięcie**, niezależnie od tego, jak długo przynosi straty.

Głębsza przyczyna jest w kształcie typu: `supply::Settlement` niesie sprzedawcę jako `FirmId`,
nie `SiteId`. Firma z trzema zakładami dostaje jedną kwotę i nie ma jak jej rozdzielić na linie.

**Szew.** Rozliczenie B2B wie, **z którego slotu magazynowego** towar wyjechał, a slot zna swój
zakład. Zamiast rozszerzać `Settlement` o `SiteId` (co dotyka rynku B2B, kontraktów i importu),
utarg przypisuje się po stronie odbierającej: `absorb_settlements` w `sim/economy` ma już slot
i firmę, więc ma też zakład.

To jest mniejsza zmiana niż wygląda, ale ma jeden nietrywialny przypadek — sprzedaż z magazynu
centralnego firmy, który nie jest zakładem produkcyjnym. Taki utarg idzie na zakład, który partię
**wyprodukował** (ślad partii to wie), a gdy partia jest importowana — na `RestOfWorld`.

**Zakres.**

| Co | Gdzie |
|---|---|
| `Settlement` niesie `seller_site: Option<SiteId>` obok `seller` | `sim/supply/src/b2b/rfq.rs` (rozstrzygnięcie zna zakład sprzedawcy) |
| `absorb_settlements` woła `post_revenue` dla zakładu sprzedającego | `sim/economy/src/market/restock.rs` |
| Koszt własny sprzedaży hurtowej z partii, nie ze średniej | tamże, przez istniejące `take_cogs` |
| `SitePnlMonth` zakładu produkcyjnego wchodzi do karty firmy | `sim/firms/src/panel.rs` — pole jest, dziś zawsze zerowe |

**Ostrzeżenie o determinizmie.** Ten pakiet zmienia hash stanu: `SitePnlMonth` wchodzi do
funkcji haszującej `sim/firms`, a jego zawartość przestaje być zerem. Zmiana jest zamierzona
i musi być odnotowana w commicie.

**Kryterium:** test odtwarzający — młyn sprzedaje mąkę piekarni przez rynek B2B, po domknięciu
miesiąca `SitePnlMonth.revenue` młyna jest niezerowy, a `margin_bp()` zwraca `Some`. Przed naprawą
zwraca `None`. Drugi test: zakład produkcyjny z trwałą stratą zostaje zamknięty przez tier
taktyczny najpóźniej po trzech miesiącach — dziś nie zostaje zamknięty nigdy. Przebieg
dziesięcioletni: liczba zakładów produkcyjnych z niezerowym utargiem = liczba zakładów
produkcyjnych, które w tym miesiącu cokolwiek wysłały.

---

### R2-WP8 — Majątek gospodarstwa przy rozwiązaniu i podziale

**Pozycje wykazu:** 16, 19.

**Przyczyna.** `rozwiaz_gospodarstwo` w `demography/day.rs` oddaje lokal do puli pustostanów,
czyści przelew i despawnuje encję. W całej funkcji **nie ma ani jednej linii dotykającej**
`cash`, `bank`, `savings` ani `debt` — salda znikają razem z encją. `M3c` `G-7` domknęło lokal
i etat; pieniędzy w tym wpisie nie ma.

Dla mieszkańca odpowiednik istnieje i działa: gotówka bez spadkobiercy idzie na konto techniczne
(`escheat`), żeby test zachowania pieniądza widział adres. Dla gospodarstwa nie ma nic.

Druga połowa: `zaloz_gospodarstwo` — wołane przy usamodzielnieniu i przy rozstaniu — tworzy
gospodarstwo z saldami zerowymi. Dwudziestopięciolatek wyprowadzający się z domu i partner
wyprowadzający się po rozstaniu zabierają wyłącznie swój prywatny portfel; wspólne oszczędności
i wspólny dług zostają po drugiej stronie w całości.

**Szew.** Najpierw niezmiennik, potem naprawa (§5.4). `Books::check_conservation` rozszerza się
o sumę sald gospodarstw, co wymaga, żeby `sim/economy` umiało je policzyć — port istnieje
(`WorldWorkforce` czyta komponent `Household`), więc nowego nie trzeba.

Podział przy rozstaniu idzie przez `split_proportional` z `engine/core`, tak samo jak podział
spadku: suma części równa się kwocie dzielonej co do grosza, reszta do pierwszej pozycji
w ustalonym porządku. Dług dzieli się tą samą funkcją i tą samą wagą — R2 nie wprowadza reguł
podziału majątku przy rozstaniu jako mechaniki, tylko odmawia ich gubienia.

**Zakres.**

| Co | Gdzie |
|---|---|
| Niezmiennik P1 obejmuje gospodarstwa (`K-61`) | `sim/economy/src/books.rs` + `tests/money_conservation.rs` |
| Rozwiązanie gospodarstwa: salda do spadkobierców, brak spadkobierców → `escheat` | `demography/day.rs::rozwiaz_gospodarstwo` przez `InheritanceHook` |
| Usamodzielnienie: udział `1/n` w gotówce i oszczędnościach, dług **nie** idzie za wyprowadzającym się | `migration.rs::usamodzielnienie` |
| Rozstanie: podział gotówki, oszczędności i długu na dwie równe części | `demography/month.rs` |

**Kryterium:** test odtwarzający — gospodarstwo z saldem 100 000 gr, wszyscy członkowie umierają
w jednej dobie, po dobie suma sald plus rejestr emisji jest niezmieniona. Przed naprawą brakuje
100 000 gr i **niezmiennik tego nie widzi**, więc test najpierw rozszerza niezmiennik, a dopiero
potem sprawdza kwotę. Test własnościowy `proptest` na 10 000 konfiguracji: podział przy rozstaniu
i przy usamodzielnieniu sumuje się do kwoty wyjściowej z tolerancją zero.

---

### R2-WP9 — Dochód gospodarstwa po zdarzeniu życiowym

**Pozycje wykazu:** 17, 41.

**Przyczyna.** `Household.income_monthly` jest denormalizacją — źródłem prawdy o płacy zostaje
umowa po stronie firmy — i jest utrzymywana przyrostowo przez jedną funkcję z trzema wołającymi:
zatrudnienie `+wage`, zwolnienie `−wage`, podwyżka `to − from`. To działa.

Zdarzenia spoza rynku pracy — emerytura, śmierć, wyjazd z miasta — należą do `sim/agents`, które
nie widzi `sim/economy`, więc nie mogą wołać zwolnienia bezpośrednio. Łapie je `hr::reconcile`
na początku następnej doby, porównując obsadę w rejestrze firm z rzeczywistością.

I tu jest luka. `odejdz` woła zwolnienie także dla nieżyjących, ale funkcja przesuwająca dochód
szuka gospodarstwa **przez komponent `Identity` zmarłego**. Jeśli encja została już despawnowana,
funkcja wychodzi po cichu i płaca zostaje w dochodzie gospodarstwa na zawsze. Śmierć ustawia
flagę „nieżywy" przed despawnem, a `agents.Society` i `economy.Labor` są oba wyłączne i chodzą
raz na dobę — między nimi jest bariera, więc despawn zdąży się wykonać.

**Nie rozstrzygnięto tego czytaniem** i to jest część zakresu pakietu: pierwszy krok to test,
który odpowie, czy usterka zachodzi na dzisiejszym harmonogramie. `M7d` „po M7b" i `M7a` `AT-2`
zapisały bliźniaczy problem — `labor_pct` nieaktualizowane przy śmierci — więc precedens jest.

Niezależnie od wyniku: **żaden test nie sprawdza dziś `income_monthly` po żadnym zdarzeniu
życiowym.** Pole pojawia się w testach wyłącznie jako wartość ustawiana w atrapie.

**Szew.** Dochód przestaje zależeć od tego, czy encja jeszcze istnieje: `odejdz` przekazuje
`household` wprost, wyjęte z faktów **przed** rozstrzygnięciem, a nie odczytywane po fakcie.
To usuwa całą klasę problemu, a nie jeden przypadek.

**Zakres.**

| Co | Gdzie |
|---|---|
| Test rozstrzygający, czy usterka zachodzi na dzisiejszym harmonogramie | `sim/economy/tests/labor.rs` |
| `Leave` niesie `household: u32` wyjęty przed rozstrzygnięciem | `sim/economy/src/labor/hr.rs` |
| Jawna kolejność `agents.Society` przed `economy.Labor` | deklaracja systemu, `.after("agents.Society")` |
| Trzy testy zdarzeń życiowych: zgon, emerytura, wyjazd z miasta | `sim/economy/tests/labor.rs` |

**Kryterium:** test odtwarzający dla każdego z trzech zdarzeń osobno — po dobie od zdarzenia
`income_monthly` gospodarstwa spadł dokładnie o płacę odchodzącego. Co najmniej jeden z trzech
musi paść przed naprawą; jeśli **żaden nie pada**, pakiet kończy się samymi testami i wierszem
w tabeli korekt, że pozycja 17 była podejrzeniem, nie usterką. To jest dopuszczalne zamknięcie.

---

### R2-WP10 — Dziedziczenie ponad gotówkę osobistą

**Pozycja wykazu:** 18.

**Przyczyna.** `InheritanceHook` jest punktem wymiany zaprojektowanym dla M5/M7/M10 — udziały
w firmach, długi, majątek. Domyślną i jedyną implementacją jest `NoInheritance` z **pustym ciałem**.
Dziedziczona jest wyłącznie gotówka osobista zmarłego: `Wealth.cash + Wealth.personal_assets`,
dzielona przez `split_proportional`. `M3` §9.10 zostawiło zakres dziedziczenia jako decyzję
otwartą, która nigdy nie została zamknięta.

Skutek w M7: właściciel firmy umiera, firma zostaje bez udziałowca, a `Owner` nadal wskazuje na
nieżyjącego mieszkańca. Skutek w M5: kredyt zmarłego nie ma kto spłacić i zostaje w `LoanBook`
jako zobowiązanie encji, której nie ma.

**Szew.** Hak dostaje implementację po stronie `sim/economy`, bo tam są księgi, kredyty i rejestr
firm — `sim/agents` nadal nie wie, czym jest udział w firmie. Kolejność zaspokajania jest **ta
sama co w upadłości** (`ClaimPriority`, `K-48`): najpierw zobowiązania, potem podział reszty.
Reguła już istnieje i jest przetestowana; R2 jej nie powiela, tylko podpina.

**Zakres.**

| Co | Gdzie |
|---|---|
| `EconomyInheritance: InheritanceHook` | nowy moduł `sim/economy/src/inherit.rs` |
| Udziały w firmach przechodzą na spadkobierców wg tych samych wag co gotówka | tamże + `sim/firms/src/registry.rs` |
| Niespłacony kapitał kredytu obciąża masę spadkową przed podziałem | `sim/economy/src/credit.rs` |
| Firma bez żywego udziałowca po podziale → dobrowolne zwinięcie, nie upadłość | `sim/economy/src/firmlife/expand.rs` |
| Salda gospodarstwa z R2-WP8 wchodzą do masy spadkowej | `inherit.rs` |

**Kryterium:** test odtwarzający — właściciel firmy z kredytem umiera, zostawiając dwoje dzieci;
po dobie kredyt jest obciążeniem masy, udziały są podzielone po równo, a suma pieniądza się
zgadza. Przed naprawą udziały wskazują na nieżyjącego, a kredyt zostaje w księdze bez dłużnika.
Test własnościowy: w przebiegu dziesięcioletnim **żadna firma nie ma udziałowca, który nie żyje**.

---

### R2-WP11 — Skala ekwiwalentna gospodarstwa

**Pozycja wykazu:** 23.

**Przyczyna.** Konsumpcja liczy się jako `dzienne_na_osobę × rozmiar_gospodarstwa` w dwóch
miejscach `market/fulfil.rs`, a koszty stałe jako `9000 gr × rozmiar`. Niemowlę je tyle co
dorosły mężczyzna i zużywa tyle samo prądu. Skala ekwiwalentna gospodarstwa domowego nie
występuje w projekcie w żadnej postaci — ani w kodzie, ani w danych, ani w planie.

Skutek jest mierzalny: gospodarstwo `FamilyWithKids` z czwórką dzieci ma zapotrzebowanie sześciu
dorosłych, czyli koszyk o jedną trzecią za duży, co przekłada się na koperty budżetowe, na próg
odłożenia zakupu i na CPI.

**Szew.** Jedna funkcja w `sim/agents::household`, bo to gospodarstwo wie, kto ile ma lat, i jeden
wiersz w danych. Wagi idą za skalą zmodyfikowaną OECD, ale **liczbami z pliku**, nie stałymi:
pierwszy dorosły 1,0, każdy następny dorosły 0,5, dziecko poniżej `ages.adult` 0,3.

Nie jest to zmiana kosmetyczna — dotyka mianownika członu ceny w funkcji użyteczności, więc
przechodzi przez balansator. Dlatego rozmiar `S`, ale ryzyko `N-1`.

**Zakres.**

| Co | Gdzie |
|---|---|
| `Household::equivalent_size(&World) -> Fx` | `sim/agents/src/household.rs` |
| `data/economy/envelopes.ron`: `equivalence: (adult_first, adult_next, child)` | — |
| Konsumpcja i koszty stałe biorą skalę zamiast liczby osób | `sim/economy/src/market/fulfil.rs`, `budget.rs` |

**Kryterium:** test odtwarzający — gospodarstwo dwoje dorosłych + czworo dzieci ma zapotrzebowanie
równe 2,7 osoby ekwiwalentnej, nie 6; koszyk dobowy odpowiednio mniejszy. Przed naprawą oba są
sześciokrotnością. Przebieg balansatora: CPI po zmianie nie wychodzi poza pasmo G1, a jeśli
wychodzi — zadanie kalibracyjne z własnym wierszem, nie cofnięcie naprawy.

---

## 5.6 Decyzje otwarte tej podfazy

**`D-N9` — Czy dług idzie za wyprowadzającym się z gniazda.** Propozycja: nie. Dwudziestopięciolatek
zabiera udział w gotówce i oszczędnościach, dług zostaje po stronie gospodarstwa, które go
zaciągnęło. Wariant odwrotny wymaga śledzenia, kto był stroną umowy kredytowej, a `LoanBook`
wiąże kredyt z gospodarstwem, nie z osobą. *Blokująca dla R2-WP8.*

**`D-N10` — Czy skala ekwiwalentna dotyczy też mediów.** Propozycja: tak, tą samą skalą. Prąd
i woda zużywają się per osoba mniej niż liniowo (jedno oświetlenie, jedno ogrzewanie), więc
liniowość jest tu gorszym przybliżeniem niż w żywności. Wariant „skala tylko dla żywności"
jest łatwiejszy do obrony liczbowo, ale wymaga dwóch skal w danych. *Nieblokująca.*

---

## Zmiany wpisane po R2b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2b.

| # | Zmiana | Dlaczego |
|---|---|---|
| | *(tabela wypełnia się w trakcie R2b)* | |
