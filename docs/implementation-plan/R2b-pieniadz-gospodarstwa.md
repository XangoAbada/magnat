# R2b — Pieniądz gospodarstwa i utarg zakładu

Podfaza dokumentu naprawczego `R2-naprawy-po-M11.md`. Numeracja sekcji `5.x` jest numeracją
dokumentu R2.

| | |
|---|---|
| **Wejście** | R2a zamknięte — R2-WP8 i R2-WP10 dotykają rozwiązania gospodarstwa, które R2-WP4 właśnie przedefiniował (gospodarstwo z opiekunem nie rozwiązuje się przy śmierci ostatniego dorosłego). |
| **Pakiety robocze** | R2-WP7…R2-WP11, R2-WP30, R2-WP32 |
| **Wynik do pokazania** | `headless m5shop --days 3600` z rozszerzoną sekcją „pieniądz": osobny wiersz dla gospodarstw rozwiązanych, dla spadków i dla kwot, które dziś znikają. Dziś ta sekcja domyka się co do grosza **tylko dlatego, że gospodarstwa nie są kontami w `Books`**. |
| **Kryterium zamknięcia** | Kryteria R2-WP7…R2-WP11, R2-WP30 i R2-WP32 plus: w przebiegu dziesięcioletnim **suma sald gospodarstw plus rejestr emisji zgadza się co do grosza po tysiącu rozwiązanych gospodarstw**, a liczba zakładów produkcyjnych z niezerowym utargiem równa się liczbie zakładów produkcyjnych. Po R2-WP32 niezmiennik świata domyka się **z ruchem w scenariuszu**, a nie tylko bez niego. |
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
| R2-WP7 ⇧ | Utarg zakładu produkcyjnego | — | M | `[x]` **wykonane przed R2 (2026-09-18)** jako warunek wejścia M8d, zgodnie z propozycją domyślną `D-N1` (`K-75`) |
| R2-WP8 | Majątek gospodarstwa przy rozwiązaniu i podziale | — | M | `[x]` (`K-61`) |
| R2-WP9 | Dochód gospodarstwa po zdarzeniu życiowym | — | S | `[x]` (`K-93`) |
| R2-WP10 | Dziedziczenie ponad gotówkę osobistą | R2-WP8 | M | `[x]` (`K-95`) |
| R2-WP11 | Skala ekwiwalentna gospodarstwa | — | S | `[x]` (`K-94`) |
| R2-WP30 | Lista płac obciąża pracodawcę | R2-WP7 | M | `[~]` (`K-93`; miasto jako pracodawca zostaje otwarte — `B-7`) |
| R2-WP32 | Konta stacji, przewoźnika, taksówki i parkingu | — | L | `[~]` (`K-72`; kanały ruchu zamknięte, niezmiennik świata zostaje otwarty — poz. 78 wykazu) |
| R2-WP36 | Utarg eksportowy zakładu produkcyjnego | R2-WP7 | S | `[x]` |

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
| `absorb_settlements` zbiera utarg zakładu sprzedającego do licznika miesięcznego, a `close_month_with` wypuszcza go **tą samą listą**, którą oddaje sklepom — `post_revenue` zostaje przy jednym wołającym | `sim/economy/src/market/restock.rs`, `close.rs` |
| Koszt własny sprzedaży hurtowej z partii, nie ze średniej | **`Store::resell` zwraca koszt własny sprzedawcy** (tę samą liczbę, którą dopisuje do księgi kontrolnej magazynu), a `Settlement::seller_cogs` przenosi ją dalej — `take_cogs` nie było tu potrzebne |
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

### R2-WP36 — Utarg eksportowy zakładu produkcyjnego

**Pozycja wykazu:** 67. Wyszła z `R2-WP7` przy jego wykonaniu.

**Przyczyna.** Sprzedaż na eksport nie przechodzi przez `Store::resell`: masa schodzi
z bilansu dopiero po rozładunku w węźle granicznym (`B2b::absorb_exports`), więc w chwili
budowania `Settlement` nie ma czym zmierzyć kosztu własnego. `R2-WP7` postawił tam
`seller_site: None`, czyli **eksport nie wchodzi do rachunku wyniku zakładu**.

Alternatywa — policzyć utarg bez kosztu — byłaby gorsza od zera: fabryka pokazałaby sto
procent marży, a tier taktyczny trzymałby eksportera bez względu na rzeczywisty wynik.
Zero znaczy w `margin_bp()` „nie wiem” i to jest uczciwsza odpowiedź. Skutek zostaje
jednak ten sam co przed `R2-WP7`: **zakład produkujący wyłącznie na eksport jest
strukturalnie odporny na zamknięcie.**

**Szew.** `Store::export` **już zwraca koszt** (`absorb_exports` wyrzuca go do `_koszt`).
Brakuje wyłącznie pamięci, **który zakład** wysłał towar do węzła — partia to wie
(`BatchOrigin::site`), więc pytanie jest o to, czy brać ją z partii, czy zapamiętać
przy zleceniu transportowym.

**Kryterium:** test odtwarzający — zakład sprzedający wyłącznie na eksport ma po
domknięciu miesiąca niezerowy `SitePnlMonth.revenue` **i** niezerowy `cogs`, a jego
`margin_bp()` odpowiada różnicy ceny eksportowej i kosztu wytworzenia. Przed naprawą
utarg jest zerem.

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

### R2-WP30 — Lista płac obciąża pracodawcę

**Pozycja wykazu:** 58. Zapisana siedem razy: `M7b`, `M7f`, `M8a`, `M8d`, `M8e`, `M8` `CJ-9`
i `sim/city/src/rule.rs` w komentarzu nad regułą pensji nauczyciela. `CJ-9` kazał dopisać ją
do wykazu R2 i **to się nie stało** — pozycja weszła tu dopiero z przeglądu sesji.

**Przyczyna.** Są dwie ścieżki wypłaty i idzie nimi ten sam pieniądz dwa razy — a raczej ani razu,
bo druga nie ma końca.

`FirmSystem::run` woła `Firms::run_payroll` o północy i odkłada wynik w `PayrollOutbox`.
Skrzynka istnieje z dobrego powodu: `sim/firms` nie może sięgnąć po `Books`, bo zależność idzie
`economy → firms`, nie odwrotnie. Ten sam wzorzec działa dla rozliczeń B2B z M6 i dla decyzji
AI (`DecisionOutbox`, konsument w `sim/economy::ai_run`). Tyle że `PayrollOutbox::take()` nie ma
w repozytorium **ani jednego wołającego** — poza dwiema linijkami rejestrującymi sam zasób
(`game/src/world/full.rs`, `labor.rs`).

Gospodarstwo dostaje więc pieniądze zupełnie inną drogą: `pay_incomes` w `sim/economy/src/systems.rs`
przelewa `Household.income_monthly` z konta `market.rest_of_world()`. To jest **denormalizacja
dochodu**, nie lista płac zakładu: kwota bierze się z pola gospodarstwa, źródłem jest konto
techniczne „reszta świata", a `FirmBooks` pracodawcy nie widzi tej operacji w ogóle.

Trzy skutki, wszystkie zapisane wcześniej i żaden domknięty:

- **Rachunek wyniku zakładu nie zna kosztu pracy**, więc marża zakładu produkcyjnego jest fikcją
  nawet po R2-WP7 — stąd zależność tego pakietu od tamtego.
- **Miasto nie może być pracodawcą.** `CH-4` opisuje łańcuch „budżet → pensja nauczyciela →
  jakość szkoły"; `sim/city/src/rule.rs` mówi w komentarzu wprost, że wymaga `FirmKey` dla miasta
  **i konsumenta skrzynki**. Pierwsze da się zrobić w godzinę, drugiego nie ma.
- **Pieniądz wchodzi do gospodarstw z konta, które nie ma pokrycia w gospodarce.** Niezmiennik P1
  tego nie łapie, bo `rest_of_world` jest kontem emisyjnym i ma prawo schodzić poniżej zera.

**Szew.** Konsument skrzynki idzie do `sim/economy`, obok `absorb_settlements` — to jest ten sam
kształt (skrzynka z `sim/firms`, księgowanie po stronie ekonomii) i ta sama kadencja doby.
`pay_incomes` przestaje być źródłem wypłaty i zostaje **wyłącznie** dla dochodów spoza etatu
(emerytury, świadczenia), czyli dla tego, co naprawdę przychodzi z zewnątrz obiegu.

Potrącenie PIT zostaje tam, gdzie jest (`market.withhold`, hak M8) — zmienia się płatnik, nie
mechanizm. To jest ważne, bo M8a wpięła akcyzę i PIT w dwa **prawdziwe** punkty i R2 nie ma prawa
ich przestawić.

**Zakres.**

| Co | Gdzie |
|---|---|
| `absorb_payroll(world, market, t)` — konsument `PayrollOutbox::take()` | nowy moduł `sim/economy/src/payroll.rs` |
| Wypłata obciąża `FirmBooks` pracodawcy i uznaje gospodarstwo; potrącenie PIT bez zmian | tamże + `sim/economy/src/books.rs` |
| `pay_incomes` traci ramię „płaca" i zostaje przy dochodach spoza etatu | `sim/economy/src/systems.rs` |
| `hr_costs` ze skrzynki wchodzą do `SitePnlMonth` jako koszt pracy | `sim/firms/src/panel.rs` |
| `FirmKey` dla miasta i wypłaty placówek publicznych tą samą drogą (`CH-4`) | `sim/city/src/rule.rs`, `sim/firms/src/registry.rs` |

**Ostrzeżenie o determinizmie.** Pakiet zmienia hash: salda firm przestają być nietknięte przez
płace, a `SitePnlMonth` dostaje niezerowy koszt pracy. Zmiana jest zamierzona i idzie w commicie
razem z zapisem, o ile zmieniły się liczby w scenariuszu odniesienia.

**Kryterium:** test odtwarzający — zakład z trzema etatami po domknięciu doby ma saldo mniejsze
dokładnie o sumę płac brutto, a suma sald gospodarstw większa o sumę netto; różnica siedzi
w `PitPayable`. Przed naprawą saldo zakładu **nie zmienia się w ogóle**, a `PayrollOutbox.pending`
rośnie w nieskończoność — drugi test sprawdza właśnie to: po dziesięciu dobach skrzynka jest
pusta, dziś ma dziesięć dób wypłat. Trzeci test: nauczyciel zatrudniony przez miasto dostaje
pensję z budżetu miasta, a `SpendCategory::Education` maleje o tę kwotę.

---

### R2-WP32 — Konta stacji, przewoźnika, taksówki i parkingu

**Pozycja wykazu:** 60. To jest **decyzja otwarta nr 16 fazy M5**, która trzyma bramkę 7 tamtej
fazy od M5c i nie należała do żadnego pakietu M5…M9.

**Przyczyna.** Niezmiennik świata brzmi `society::total_money + Books::total_balance() == const`.
Mieszkaniec płaci za paliwo, bilet, taryfę i parking z komponentu `Wealth` — a druga strona tych
czterech płatności to `FuelLedger` i `FareLedger` w `sim/traffic`, czyli **rejestry, nie konta**.
Oba wchodzą do hasha (to jest pieniądz i M4 wiedział o tym, pisząc je), ale żaden nie ma konta
w `Books`, więc grosz wychodzi z jednej sumy i nie wchodzi do drugiej.

Pomiar z M5c po dopisaniu obu rejestrów do sumy: **−5,6 tys. zł przez 31 dób i +63,2 tys. zł
przez 40 dób** na 100 mln zł w mieście 28 tys. mieszkańców. Ze **zmianą znaku**, czyli kanałów
bez pary jest co najmniej dwa i działają w przeciwne strony. Po M5d ta sama liczba wynosi
**+163,0 tys. zł przez 40 dób** — i to nie jest regres M5d: pieniądz kredytowy zwiększył wydatki
na dojazdy, więc kanał bez pary przepuszcza proporcjonalnie więcej. **Błąd jest mnożnikowy, nie
addytywny**, i każda faza dokładająca wydatki na transport go powiększa.

Podejrzany numer jeden jest nazwany: `FareLedger.taxi_revenue` rośnie o 200 tys. zł przez 40 dób,
a w `sim/traffic` nie ma odpowiadającego mu zapisu po stronie `Wealth`.

**Dlaczego nie zrobiła tego M5d.** Bo punkt żądał uzgodnienia z M4 **przed** wpięciem konta:
wciągnięcie stacji do M5d oznaczałoby odziedziczenie kanału bez pary razem z kontem, czyli
zabetonowanie usterki pod nowym adresem. Warunek nadal obowiązuje i jest pierwszym krokiem
pakietu.

**Szew.** Najpierw **znaleźć kanał bez pary**, potem dawać konta — odwrotna kolejność zamienia
pomiar w zgadywanie. Krok pierwszy jest testem, nie kodem produkcyjnym: przebieg z rozbiciem
salda per kanał, dzień po dniu, aż znak się zmieni.

Konta idą tam, gdzie mają właściciela w fikcji świata: stacja paliw jest zakładem firmy
(`M5` `T-2` mówi wprost „M5 podmienia ciało na ofertę w `sim/economy`"), przewoźnik i parking
są jednostkami miasta po M8, a taksówka — zgodnie z `M4` `D5` — nie ma encji firmy i dostaje
konto techniczne z jawnie nazwanym właścicielem, dopóki jej nie dostanie.

**Zakres.**

| Co | Gdzie |
|---|---|
| Przebieg diagnostyczny: saldo świata z rozbiciem na kanały ruchu, doba po dobie | `tools/headless/src/m7_miasto.rs` (sekcja istnieje, brakuje rozbicia) |
| Stacja paliw jako zakład z księgą; `FuelLedger.revenue` staje się przychodem zakładu | `sim/traffic/src/systems.rs`, `sim/economy/src/market/` |
| Bilet i paliwo taboru: konto operatora komunikacji (jednostka miasta po M8b) | `sim/city`, `FareLedger.transit_revenue`, `transit_fuel_cost` |
| Taryfa taksówkowa: konto techniczne z właścicielem nazwanym w `TxKind` | `sim/economy/src/books.rs` |
| Opłata parkingowa: przychód miasta, nie rejestr | `FareLedger.parking_revenue` → `CityBudget` |
| Niezmiennik świata **bez wyłączeń** jako bramka scenariusza, nie pomiar obok (`K-72`) | `tools/headless`, `sim/economy/tests/` |

**Ostrzeżenie o determinizmie.** Pakiet zmienia hash dwa razy: raz przez przeniesienie kwot
z rejestrów do kont, raz przez zniknięcie pól rejestrów z funkcji haszującej `sim/traffic`.
Oba kroki idą w osobnych commitach, żeby dało się je rozdzielić przy porównaniu.

**Kryterium:** test odtwarzający — przebieg 40-dobowy scenariusza z ruchem domyka niezmiennik
świata z tolerancją **zero groszy**. Przed naprawą rozjazd wynosi +163,0 tys. zł i rośnie razem
z liczbą dób, co drugi test sprawdza wprost: rozjazd po 80 dobach jest ponad dwukrotnie większy
niż po 40, bo jest mnożnikowy. Bramka scenariusza `m5shop` przestaje być zawężona przez `W-17`
i wraca do pełnego brzmienia z `U-17`.

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

**`D-N22` — Czy taksówka dostaje encję przewoźnika, czy konto techniczne.** Propozycja: konto
techniczne z właścicielem nazwanym w `TxKind`. `M4` `D5` rozstrzygnęło, że taksówka jest opcją
transportową z taryfą z `data/roads/mode_choice.ron`, bez encji firmy i bez floty, i **kurs nie
wjeżdża na sieć** — encja przewoźnika wymagałaby floty, kierowców i etatów, czyli mechaniki,
której R2 nie wprowadza (§2 „czego to nie jest"). Wariant odwrotny jest lepszy docelowo i ma
naturalny adres: M10d (rynek kontroli nad firmą) albo osobna faza usług. Jeśli decyzja pójdzie
odwrotnie, R2-WP32 rośnie z `L` do `XL` i przestaje mieścić się w R2. *Blokująca dla R2-WP32.*

---

## Zmiany wpisane po R2b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2b.

| # | Zmiana | Dlaczego |
|---|---|---|
| B-1 | Decyzja otwarta o taksówce przenumerowana z `D-N20` na **`D-N22`** | `D-N20` zajęła decyzja „zakład budynkiem czy lokalem" otwarta w M11c (§9 dokumentu R2). Dwie decyzje o tym samym numerze w tym samym dokumencie naprawczym są dokładnie tą klasą rozjazdu, którą R2 mierzy. Numer taksówki jest cytowany wyłącznie w tym pliku, numer Etapu 7 — w §4 i w wykazie §11, więc przenumerowuje się tańszy. Pierwszy wolny numer to `D-N22` (`D-N21` zajęte w `R2f`) |
| B-2 ★ | `Household.split_off` dzieli **trzy** pozycje: gotówkę, oszczędności **i rachunek bieżący** | Zakres `R2-WP8` wymieniał „gotówkę i oszczędności". `bank` to konto, na które wpływa pensja i z którego płaci się rachunki — zostawienie go po stronie gniazda znaczyłoby, że dwudziestopięciolatek wychodzi z udziałem w skarbonce i zerem na koncie. Dług nie idzie (`D-N9`) i to jest jedyne wyłączenie |
| B-3 ★ | Ułamek podziału podaje **wołający** (`Czesci::Domownicy` / `Czesci::Polowa`), a nie sam `zaloz_gospodarstwo` | Plan mówi „udział `1/n`" przy wyprowadzce z gniazda i „na dwie równe części" przy rozstaniu. To są dwie różne prawdy o tym, kto jest stroną podziału — dzieci mieszkają w domu, ale nie są stroną umowy majątkowej — a jedna reguła dla obu byłaby liczbą wziętą znikąd |
| B-4 | Majątek gospodarstwa wchodzi do masy spadkowej w `smierc`, a nie w `rozwiaz_gospodarstwo` | Zakres `R2-WP8` wskazywał `rozwiaz_gospodarstwo` „przez `InheritanceHook`". Pod tamtym adresem lista spadkobierców jest **zawsze pusta**, bo `smierc` woła `usun_relacje` wcześniej — cały majątek szedłby na konto techniczne, także wtedy, gdy zmarły zostawia dzieci. `rozwiaz_gospodarstwo` dostaje siatkę bezpieczeństwa (reszta → `escheat`), a regułę podziału ma jedno miejsce |
| B-5 ★ | Niezmiennik `K-61` jest **osobną funkcją** (`Books::check_world_conservation(sektor)`), a nie rozszerzeniem `check_conservation` | Saldo gospodarstwa mieszka w komponencie i tam zostaje — `books.rs` wykłada, dlaczego dwa źródła tej samej liczby rozjechałyby się przy pierwszej transakcji. `P1` patrzy na same księgi i **musi** liczyć kanał sektora do podaży; niezmiennik świata widzi obie strony naraz i kanału **nie** liczy. Różnica jest cała w prawej stronie (`MoneySupplyLedger::core_total`) |
| B-6 ★ | `R2-WP9` ma **dwie** przyczyny, nie jedną | Plan przewidywał jedną: `release` szuka gospodarstwa przez `Identity` zmarłego, którego encji już nie ma. Druga wyszła z testu emerytury: `odejdz` pomijało `release` w ogóle, bo warunek `f.job == Some((site, role))` nie miał przypadku „ten człowiek nie ma już żadnej pracy" — a `sim/agents` czyści komponent przy przejściu na emeryturę. Obie naprawione w tym samym pakiecie, bo obie są tą samą usterką widzianą z dwóch stron: dochód zależał od tego, w jakim stanie jest encja |
| B-7 ★ | `R2-WP30` zamyka się **bez miasta jako pracodawcy** (`CH-4`) | Konsument skrzynki, obciążenie pracodawcy i przebudowa `pay_incomes` na dopłatę są zrobione. Trzeci wiersz zakresu — `FirmKey` dla miasta i wypłaty placówek publicznych — **nie**: most `game::world::firms` **pomija** zakłady municypalne (`skipped_municipal`, „ich firmy stawia M8"), więc nauczyciel nie jest dziś w rejestrze firm w ogóle. Zrobienie tego znaczy postawić firmę miasta, zakłady dla każdej placówki, obsadzić je z ECS i przestawić `wydaj` z planu wydatków na realną listę płac — czyli **wprowadzić mechanikę**, czego `R2` §2 zabrania. Pozycja dostaje wiersz w wykazie §11 z adresem |
| B-8 ★ | `pay_incomes` nie traci ramienia „płaca" — staje się **dopłatą** | Plan mówił „zostaje wyłącznie dla dochodów spoza etatu". Dochodów spoza etatu dziś nie ma: `income_monthly` prowadzą wyłącznie zatrudnienie, zwolnienie i podwyżka. Zwykłe usunięcie ramienia zostawiłoby bez grosza **każdy świat bez rejestru firm** — a takie są scenariusze M3 i M5, w których `income_monthly` pochodzi z generatora. `Market` dostaje licznik wypłat miesiąca, a `pay_incomes` dopłaca różnicę: świat z pracodawcami płaci raz, świat bez nich płaci tak jak przedtem |
| B-9 ★ | Skala ekwiwalentna jest w **promilach**, nie w `Fx` | Zakres `R2-WP11` pisał `Household::equivalent_size(&World) -> Fx`. Cała ścieżka zapasu i pieniądza jest całkowitoliczbowa (`Qty`, `Money`, bp), a `Fx` byłby w niej drugim układem liczbowym — kupionym za nic, bo trzy wagi z pliku mnożą się bez reszty. Skład liczy `sim/agents` (`Household.children`, bajt z rezerwy M5/M9), wagę nakłada `sim/economy`, bo to **dana gospodarki** |
| B-10 ★ | `R2-WP36`: `seller_site` wypełnia `try_export`, a koszt własny bierze się z **ładunku zlecenia**, nie z partii w węźle | Komentarz w `trade.rs` twierdził, że kosztu nie da się zmierzyć, bo „masa schodzi z bilansu dopiero po rozładunku". To pomyliło dwie rzeczy: `dispatch` zdejmuje partie ze slotu sprzedawcy **od razu** (`Store::load`), a z bilansu masy schodzą później. Ładunek zlecenia zna partie, więc `Store::resell` liczy koszt tą samą drogą co sprzedaż hurtowa (`rfq.rs`) — trzy linie zamiast atrybucji per partia w węźle, która i tak milczałaby dla każdej partii bez `origin.site` |
| B-11 ★ | `R2-WP32`: rejestry ruchu tracą **kwoty**, a nie całe pola; parking zostaje martwy | `K-72` mówi „rejestry stają się kontami". Wykonane dosłownie dla pieniądza: `FuelLedger` i `FareLedger` tracą wszystkie pola `Money`, zostają liczniki (bilety, kursy, tankowania, litry). Opłata parkingowa **nie dostaje płatnika**, bo `ParkingLot.price_gr_per_hour` jest w całym repozytorium zerem („taryfy miejskie dopiero M8") — kanał istnieje w słowniku i w dzienniku, a pisarza dostanie razem z taryfą. Licznik `parking_stays` bez pisarza należy do `R2-WP22` |
| B-12 ★ | `R2-WP32`: opłatę pobiera **jedna funkcja** (`pobierz`), a kanałem do ksiąg jest `MobilityDue` w `engine/core` | Diagnostyka pokazała, że kanałów bez pary jest więcej niż taksówka: rejestr rósł **bezwarunkowo**, a portfel schodził **warunkowo** (`if let Some(c) = citizen_by_index`), więc każdy podróżny, który zniknął w trakcie podróży, fundował przewoźnikowi bilet z niczego. To jest ta sama usterka w trzech kopiach, więc znika razem z kopiami (`R2` §3 pkt 2). `sim/traffic` i `sim/economy` nie widzą się nawzajem, więc kwota czeka jedną minutę w zasobie `core` — ten sam wzorzec, którym `K-64` postawił tam `ServiceCoverage` |
| B-14 ★ | `R2-WP32` zamyka **kanały ruchu**, a nie niezmiennik świata | Kryterium mówiło „przebieg 40-dobowy domyka niezmiennik z tolerancją zero groszy”. Kanały ruchu są zamknięte i mają testy, ale pomiar pokazał, że **rozjazd miał więcej niż jedno źródło** — dokładnie tak, jak przewidywał opis pakietu („kanałów bez pary jest co najmniej dwa i działają w przeciwne strony”). Po naprawie zostało −49 539 zł na 31 dób wobec +107 812 zł przed nią, a przebieg 3-dobowy domyka się co do grosza: reszta wchodzi w dobie 30. Zgodnie z `R2` §3 pkt 2 druga przyczyna dostaje **własny wiersz** (poz. 78), a nie doklejenie do trwającego pakietu. Bramka scenariusza `m5shop` zostaje ostrzeżeniem — ale ostrzeżeniem z liczbą i adresem, a nie z „wiadomo, że nie domyka się” |
| B-15 ★ | Pomiar `R2-WP32` odsłonił, że **release stawiał miasto widmo** | `debug_assert!` nie oblicza argumentu w release, a `spawn_household_aged` miało tam dopisanie członka do składu (`K-91` zrobiło `add_member` `#[must_use]`, a wołający opakował je w asercję). Naprawione w trzech miejscach `migration.rs` — to jest warunek, bez którego **żadnego** pomiaru z tego pakietu nie dałoby się zrobić, bo scenariusz w debug wywraca się na `debug_assert` kolejki zdarzeń (poz. 73), a w release mierzył pusty świat. Pozycja 77 wykazu |
| B-16 | Zapis zwrotny budżetu gospodarstwa idzie po **encji z generacją**, nie po indeksie | `settle_household_month` sklejał uchwyt z `Entity::new(row.index, MIN)`. Indeks zwolniony przez rozwiązane gospodarstwo wraca do puli z następną generacją, więc taki uchwyt przestaje wskazywać cokolwiek, a cicho pominięty zapis znaczy, że pieniądz zszedł z konta i nie zszedł z komponentu. Znalezione przy szukaniu reszty rozjazdu; **nie jest jego przyczyną** (pomiar bez zmian), ale jest usterką samą w sobie i kosztuje trzy linie. Testu odtwarzającego nie ma, bo ponowne użycie indeksu wymaga długiego przebiegu — i to jest jawny wyjątek od `R2` §3 pkt 1, a nie przeoczenie |
| B-13 | `InheritanceHook` dostaje `&mut World`, metodę `estate_charge` i jest wołany **zawsze** | `R2-WP10` zakładał implementację haka po stronie `sim/economy`. Hak nie dostawał świata, a `Firms`, `Books` i `LoanBook` są zasobami — bez tego nie ma do nich drogi. Drugie: hak stał **wewnątrz** gałęzi „są spadkobiercy", więc majątek bez spadkobiercy nie miał gdzie pójść. Trzecie: zobowiązania masy muszą zejść **przed** podziałem, a podział dzieje się przed hakiem — stąd osobna metoda z ciałem domyślnym |
