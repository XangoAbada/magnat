# E2 — Pieniądz i księgowość

**Po co.** Bramki pieniądza (K1, `check_conservation`) sprawdzają tylko sumę sald. Wszystkie
błędy z tego etapu tę sumę zachowują, więc przechodzą. Zaległości nie da się spłacić, raty
liczą się podwójnie, PIT płaci „reszta świata", emisja daje udziały za darmo. Najpewniej
stąd 715 postępowań upadłościowych na 242 firmy (`BF-10`).

**Wejście.** E1 zamknięty.

**Pomiar zamknięcia.**
1. Liczba postępowań upadłościowych na liczbę firm w przebiegu `BF-10` — przed i po.
2. Niezmiennik pieniądza świata (poz. 78 wykazu R2: −49 539 zł na 31 dób) — przed i po.
3. Przyrząd z `N2.1` zielony na pięcioletnim przebiegu miasta.

**Wzorzec do wytępienia: efekt przed płatnością.** Najpierw wydaje się udziały, licencję
albo koszt, potem pobiera pieniądz; gdy przelew się nie uda, efekt zostaje. Poprawka jest
zawsze ta sama: przelew jest warunkiem efektu, w jednej operacji. Punkty `N2.3`, `N2.13`,
`N2.14` mają ten kształt. Przy każdej naprawie w tym etapie sprawdź, czy wołający obok nie
ma tego samego.

## Punkty

- [ ] **N2.1** drugi przyrząd: uzgodnienie dwustronne — wzorzec „przyrządy ślepe"
  - Suma sald nie wystarczy. Potrzebny przyrząd, który dla każdego podmiotu uzgadnia dwa
    niezależne zapisy tej samej rzeczy:
    - `Ledger` firmy (RZiS, `TaxPayable`) z przelewami w `Books` na jej kontach;
    - suma `TaxCharge` z księgi miasta z `TaxPayable` w księgach firm;
    - `outstanding` kredytów ≥ 0 i równe sumie przyszłych rat harmonogramu;
    - zaległości w `Arrears` z ratami, które ich nie spłaciły (bez podwójnego liczenia);
    - dług firm przed `lift` = dług po `lower` (używa go `N5.2`).
  - *Gdzie:* jedna funkcja w `sim/economy` wołana z testów przebiegowych i z balansatora
    jako bramka.
  - *Dowód:* przyrząd pada na dzisiejszym `master` (co najmniej na `N2.2`, `N2.3`, `N2.6`).
    Ten fakt, z liczbami, idzie do komunikatu commita.

- [ ] **N2.2** zaległość da się spłacić — `M7#1`
  - `Arrears::settle` (`sim/economy/src/corpfin/arrears.rs:246`) woła wyłącznie podział masy
    upadłościowej. Zaległości powstają w `market/close.rs:103,215,242`, `corpfin/ops.rs:132`,
    `systems.rs:780`, a `check_insolvency` patrzy na wiek najstarszej. Jeden brak na czynsz
    po 90 dobach kończy się upadłością, nawet gdy firma dawno ma gotówkę.
  - *Naprawa:* spłata zaległości (najstarsza pierwsza) przy zamknięciu miesiąca, zanim
    `check_insolvency` sprawdzi wiek.
  - *Test:* sklep raz nie płaci czynszu, potem ma gotówkę → po 120 dobach nie ma postępowania.

- [ ] **N2.3** rata kredytu atomowa — `M7#2`, `M5#3`
  - Nieudana rata trafia do zaległości, ale `outstanding` nie maleje, a `paid_months` nie
    rośnie — w następnym miesiącu ta sama rata idzie drugi raz (`market/close.rs:241-246`,
    `credit.rs:61`). W upadłości bank zgłasza zaległości **i** pełne `outstanding`
    (`proceedings.rs:87-105`).
  - Kredyt obrotowy sklepu (`close.rs:204-262`): odsetki przelane, potem `destroy_credit` na
    kapitał się nie udaje i funkcja wychodzi przed `paid_months += 1` — odsetki podwójnie.
  - Rata z `principal == 0` (`credit.rs:124`) blokuje `paid_months` na zawsze.
  - *Naprawa:* rata jest albo zapłacona w całości (odsetki + kapitał, harmonogram idzie dalej),
    albo w całości przechodzi do zaległości (harmonogram też idzie dalej). Nigdy oba naraz.
  - *Test:* proptest — ciąg udanych i nieudanych rat; suma zapłacona + zaległa = suma
    harmonogramu; roszczenie banku w upadłości = `outstanding` + zaległe, bez dubla.

- [ ] **N2.4** jedno postępowanie na firmę — `M7#4`
  - `corpfin/system.rs:100-119`, `proceedings.rs:114,424`: każdy sklep otwiera postępowanie
    ze wszystkimi zaległościami firmy; `open_case.insert` nadpisuje identyfikator, pierwsza
    sprawa zostaje w `Filed` na zawsze z zamrożoną gotówką.
  - *Test:* firma z trzema sklepami i zaległością → dokładnie jedno postępowanie.

- [ ] **N2.5** zakład produkcyjny może zbankrutować — `M7#7`
  - `otwieraj_postepowania` (`corpfin/system.rs:100`) iteruje tylko `shop_accounts()`.
  - *Test:* zakład produkcyjny z zaległością > 90 dób → postępowanie.

- [ ] **N2.6** PIT płaci pracodawca — `M8#5`, `M8#6`, `M8#10`
  - `assess.rs:103` nalicza PIT na `TaxPayer::External`, `settle.rs:55` ściąga z
    `rest_of_world`. Od R2b zakład wypłaca netto (`payroll.rs:87-93`), więc potrącona zaliczka
    zostaje na koncie firmy na zawsze. Regresja po R2b.
  - `engine.rs:78-86`: `ytd_months` rośnie przy każdym `withhold`, a wypłat w miesiącu bywa
    kilka → zaliczki za niskie.
  - `systems.rs:196`: rok podatkowy PIT przesunięty o dobę (PRAWDOPODOBNE).
  - *Test:* miesiąc pracy jednej osoby → zaliczka schodzi z konta pracodawcy, `rest_of_world`
    się nie rusza, `ytd_months == 1`.

- [ ] **N2.7** domiar, kary i odsetki za zwłokę w księdze — `M8#2`, `M8#3`, `M8#4`
  - `law.rs:634,671,702`: `accrue` bez `market.post_tax_accrual`, a zapłata
    (`settle.rs:89-91`) woła `post_tax_payment` → `TaxPayable` ujemne, kara bez kosztu w RZiS.
  - `charge.rs:317-322`: `accrue` na należności zaległej nie podnosi `totals.overdue`,
    `mark_settled` (`:402-404`) odejmuje całość → licznik ujemny.
  - `settle_due` przelewa tylko `c.amount` (`settle.rs:75`) — odsetki za zwłokę liczą się,
    ale nikt ich nie płaci; naliczane podwójnie w `oznacz_zalegla` (`:106`) i `age_overdue`
    (`:124`).
  - *Test:* zaległy VAT + domiar na tym samym kluczu → księga firmy, licznik zaległości
    i zapłata zgadzają się co do grosza, odsetki raz.

- [ ] **N2.8** prawdziwa kontrola domknięcia T1 — `M8#1`
  - `sim/city/src/charge.rs:475`: prawa strona to `settled + abated + open()`, a
    `open() = assessed − settled − abated` (`:215`). Tożsamość.
  - *Naprawa:* porównać `settled` z sumą przelewów podatkowych w `Books`, a `assessed` z sumą
    wymiarów z silnika podatkowego — z dwóch niezależnych źródeł. Najprościej jako część `N2.1`.
  - *Test:* wstrzyknięta wpłata bez `mark_settled` → kontrola pada.

- [ ] **N2.9** budżet gospodarstwa kluczem z generacją — `M5#1`
  - `MarketInner.budgets` indeksowane `Entity::index()` bez generacji
    (`sim/economy/src/market.rs:504`, `market/household.rs:43-80,188`); ECS oddaje zwolnione
    indeksy. Nowa rodzina dziedziczy cudzy kredyt i `arrears_months`; `outstanding` spada
    poniżej zera. `settle_household_loan` nie czyści `budgets[hh].loan`. `borrower` dostaje
    zawsze generację 1 (`household.rs:290`).
  - *Naprawa:* klucz `Entity` (z generacją) i usunięcie wpisu przy rozwiązaniu gospodarstwa.
  - *Test:* gospodarstwo z kredytem umiera, indeks przejmuje nowe → nowe nie płaci raty.

- [ ] **N2.10** upadłość: zabezpieczenie i reszta masy — `M7#5`, `M7#10`
  - `Secured` nie działa w grze: wszystkie ścieżki podają `None`, `credit::Loan` nie ma
    `collateral`. Proptest (`corpfin.rs:369`) mapuje `Secured → Unsecured`, `Owners → Trade`.
  - `distribute` (`proceedings.rs:323-411`) nie oddaje resztki masy właścicielom.
  - **Decyzja N2.10-a:** zabezpieczenie kredytu. *Domyślnie:* usunąć wariant `Secured` i
    decyzję D7 z kodu — nikt go nie produkuje. Wrócić, gdy kredyt dostanie zastaw.
  - *Test:* proptest bez mapowania wariantów; reszta masy trafia do właścicieli.

- [ ] **N2.11** towar nie znika bez zaksięgowanej straty — `M6#1`, `M6#8`, `M6#9`
  - `sim/supply/src/plant/produce.rs:160-205` (`krok_linii`): awaria albo odcięcie medium
    w trakcie szarży nadpisuje `Running{charge}` przez `Broken` — wsad (albo masa ze złoża,
    `:710`) znika bez `LossKind` i wpisu w `losses`.
  - `:498` `brak_miejsca` sprawdza miejsce tylko przy starcie; błąd `put` przy zamknięciu
    (`:826`) jest połykany — wyrób znika.
  - `zamknij_szarze` (`~:807`): `continue` przed `kolejka.next()` przesuwa alokację kosztu.
  - *Test:* awaria w połowie szarży → strata w `losses` równa wsadowi; bilans złoża się domyka.

- [ ] **N2.12** odrzucony ładunek wraca albo jest stratą — `M6#2`
  - `sim/supply/src/transport.rs:464-490` (`deliver`), `:494` (`prune`): pełny slot odbiorcy
    daje `Failed(Refused)` z niepustym `cargo`, którego nikt nie obsługuje. Kupujący zapłacił
    przy wysyłce, partie są `InTransit` na zawsze i się nie psują.
  - **Decyzja N2.12-a:** co z odrzuconym ładunkiem. *Domyślnie:* wraca do nadawcy
    (zlecenie powrotne), a kupujący dostaje zwrot zapłaty — to jedyny wariant, który nie
    wymyśla nowej reguły handlowej.
  - *Test:* odbiorca z pełnym slotem → po powrocie towar u nadawcy, pieniądz u kupującego.

- [ ] **N2.13** giełda: emisja i ubezpieczenia — `M10#2`, `M10#3`, `M10#8`
  - Emisja obronna (`sim/economy/src/equity/month.rs:175-182`): `corp::issue` przed `zaplac`,
    nieudana zapłata nie cofa emisji; `zaplac` zawsze `false` dla `Owner::City` i
    `Owner::External` (`pay.rs:110`). Udziały za darmo.
  - Składka ok. 12× za niska: `note_exposure` sumuje polisomiesiące (`insurance/mod.rs:118`,
    `:301-310`), `premium` (`:164`) traktuje to jak roczną i dzieli przez 12.
  - `floor(n·59/60)` dla n < 60 daje n−1 (`:326`) — ekspozycja nie rośnie przy małym n.
  - *Test:* emisja do `External` bez pokrycia → brak nowych udziałów; składka dla znanej
    ekspozycji równa ręcznemu rachunkowi.

- [ ] **N2.14** media i licencje: koszt tylko z przelewem — `M10#6`, `M10#7`
  - `sim/media/src/system.rs:865-878`: zapis kosztu zostaje, gdy `Books::transfer` odmówi.
  - `sim/firms/src/rnd/progress.rs:484-503` przyznaje licencję od razu,
    `sim/economy/src/rnd.rs:187-188` pobiera `min(kwota, saldo)` — licencja za 0 zł.
  - *Test:* firma z zerowym saldem → brak licencji i brak zapisu kosztu.

- [ ] **N2.15** spadek z długiem — `M3#5`
  - `demography/day.rs:644-665,701-703`: ujemna masa spadkowa obcinana `.max(0)`, gotówka
    zmarłego = 0 — dług znika, niezmiennik pęka. `checked_add(...).unwrap_or(cash)` gubi
    składnik po cichu.
  - **Decyzja N2.15-a:** kto przejmuje dług spadkowy. *Domyślnie:* dług zostaje przy
    spadkobiercach do wysokości masy (reszta jest umorzeniem zapisanym w księdze jako strata
    wierzyciela) — umorzenie jest zdarzeniem, nie znikaniem.
  - *Test:* proptest dziedziczenia z ujemną masą.

- [ ] **N2.16** drobne księgowe — `M5#4`, `M5#5`, `M5#6`, `M5#7`, `M5#8`
  - `ledger.rs:567-574`: `cash_flow` ma `complete = true`, choć dziennik zaczął się w środku
    okresu.
  - DSTI (`credit.rs:256-260`) liczy ratę bez premii ryzyka, kredyt (`:302`) z premią.
  - `let _ = ledger::post(...)` w `market/fulfil.rs:123,153`, `close.rs` (5 miejsc),
    `lifecycle.rs:205`; `let _ = books.destroy_credit(...)` w `household.rs:73,223` —
    błąd zapisu rozjeżdża `Ledger` z `Books`. Błąd ma wrócić do wołającego.
  - `emit`/`absorb` (`books.rs:766-803`) nie sprawdzają `memo.tax` → panika zamiast
    `TxError::InvalidTax`.
  - `ledger_post` (`kernel.rs:308-316`) sprawdza przepełnienie linia po linii.

- [ ] **N2.17** paliwo i zaokrąglanie pieniądza — `M4#4`, `M4#7`
  - `systems.rs:678-689`: przy `cost == 0` albo bez `Wealth` paliwo idzie do baku, a
    `FuelLedger.volume_ul` nie rośnie.
  - Taksówka `km = manhattan_cm / 100_000` (`offers.rs:347`) — 1,9 km kosztuje jak 1 km;
    obcięcia w `settle_edge` (`mezo.rs:170`). Wszędzie `div_round_half_up`.

- [ ] **N2.18** scenariusz `IndebtSite` nie zadłuża — `M9#9`
  - `game/src/scenario.rs:430-453` wyprowadza gotówkę przez `TxKind::Endowment` i
    `DecisionReason::Unspecified`. Ma zaciągać kredyt.

- [ ] **N2.19** martwe pola i CIT — `M8#8`, `M8#9`
  - `TaxCode.loss_carry_years` nikt nie czyta (strata przechodzi bez limitu lat);
    `TaxPayer::Household` bez pisarza.
  - CIT działa wstecz (`assess.rs:160-195`, PRAWDOPODOBNE).
  - *Domyślnie:* limit lat straty wdrożyć (pole już jest w danych), `Household` usunąć.

## Znalezione po drodze
