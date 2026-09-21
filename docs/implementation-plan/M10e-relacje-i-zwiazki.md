# M10e — Relacje i związki

Podfaza 5 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7 (firmy, płace), M8 (wyzwalacz zdarzeniowy — K-9). |
| **Pakiety robocze** | WP10.13, WP10.14 |
| **Projekt techniczny** | §5.9 |
| **Wynik do pokazania** | Strajk powstaje z żądania płacowego wyliczonego z danych M7 i skutkuje po stronie miasta przez wyzwalacz M8. |
| **Kryterium zamknięcia** | Kryteria WP10.13 i WP10.14. |
| **Poprzednia / następna** | `M10d-gielda-przejecia-ubezpieczenia.md` · `M10f-kroniki-i-domkniecie.md` |

Relacje międzyfirmowe oraz związki zawodowe i strajki — mechanika negocjacji należy tutaj, nie do M7 ani M8.

---

## Pakiety robocze

### WP10.13 — Relacje międzyfirmowe

**Zależności:** M6 (kontrakty), M7 (firmy AI), M8 (regulator).
**Kryterium ukończenia:** `SupplierRelation.trust` rośnie z historii terminowych dostaw i realnie
zmienia wybór dostawcy (firma płaci +3% stałemu dostawcy zamiast szukać taniej — widoczne w
`DecisionReason`). Kartel obniża wolumen i podnosi cenę; hazard wykrycia rośnie z odchyleniem ceny
od benchmarku; po wykryciu kara i uderzenie w markę wszystkich członków.
**Rozmiar: L.**

---

### WP10.14 — Związki zawodowe i strajki

**Zależności:** M7 (HR, płace, księgowość), M3 (graf relacji).
**Kryterium ukończenia:** zakład o marży 35% płacący 20% poniżej mediany miejskiej dla roli, z gęstą
siecią relacji między pracownikami, formuje związek w ciągu 3–9 miesięcy gry. Strajk zatrzymuje
produkcję zakładu (nie firmy), uruchamia kary z kontraktów M6 u odbiorców i jest widoczny w mediach.
Fundusz strajkowy wyczerpuje się i wymusza rozstrzygnięcie — brak strajków wiecznych (test: 100
strajków w balansatorze, mediana czasu trwania 4–21 dni, maksimum < 90 dni).
**Rozmiar: M.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.9 Relacje międzyfirmowe, związki — struktury

```rust
pub struct SupplierRelation {
    pub buyer: FirmId, pub supplier: FirmId, pub good: GoodId,
    pub since: SimMinute, pub volume_cum: Qty,
    pub trust: Q,                       // z historii terminowości i jakości
    pub discount_bps: u16, pub priority: u8,
    pub exclusivity: Option<Exclusivity>,
}

pub struct Cartel {
    pub id: CartelId, pub members: SmallVec<[FirmId; 8]>, pub good: GoodId,
    pub floor_price: Money,
    pub quota: SmallVec<[(FirmId, Qty); 8]>,
    pub formed: SimMinute, pub secrecy: Q,
}

pub struct Union {
    pub id: UnionId, pub scope: UnionScope,   // Site | Firm | Branch
    pub members: Vec<CitizenId>, pub density: Q,
    pub militancy: Q, pub strike_fund: Money,
    pub leader: CitizenId,
    pub demand: Option<UnionDemand>,
    pub state: UnionState,                    // Uśpiony | Żądanie | Negocjacje{runda} | Strajk{od} | Ugoda
}
```

**`SupplierRelation` jest mój, ale liczby karmiące `trust` są M6.** M6 jawnie odmówił własności
tego typu (relacja to §7.9, czyli mój zakres) i konsumuje z niego **dokładnie dwa pola**:
`discount_bps` (korekta ceny przy rozstrzyganiu RFQ) i `priority` (kolejność przydziału masy,
gdy dostawca nie ma dość dla wszystkich — to jest mechanizm „stały dostawca ma priorytet
w niedoborze" z §7.9). `trust`, `since`, `volume_cum` i `exclusivity` są wyłącznie moje.

**Nie buduję własnego licznika opóźnień.** `trust` liczę z `SupplyContract` M6:
`late_deliveries` i `missed_mass`. Uwaga, która kosztowałaby inaczej dzień debugowania: kontrakt
M6 ma `grace_minutes`, więc „spóźnione" i „spóźnione **ponad tolerancję**" to dwie różne liczby —
`trust` karze za tę drugą. Własny licznik obok licznika M6 rozjechałby się przy karach umownych.

Degradacja jest łagodna w obie strony: dopóki `SupplierRelation` nie istnieje, M6 liczy RFQ bez
rabatu i bez priorytetu, więc M10 nie blokuje M6, a M6 nie blokuje M10.

**Hazard wykrycia kartelu (miesięczny).** Kartel nie ginie od rzutu kostką, tylko od własnej chciwości:

```
h = 0,5%                                       // baza
  + 0,3% × (liczba_członków − 3).max(0)        // każdy dodatkowy członek to dodatkowe usta
  + 2,0% × (odchylenie_ceny_od_benchmarku% / 10)   // im więcej kradniesz, tym bardziej widać
  + 0,2% × liczba_niezadowolonych_wtajemniczonych  // menedżer z nastrojem < −40 lub zwolniony
  × aktywność_regulatora                       // z M8, 0,5..2,0
```

Kara: 10% obrotu 12-miesięcznego per członek (ograniczone wypłacalnością), rozwiązanie kartelu,
**uderzenie w markę: afinitet −20 u każdego mieszkańca posiadającego slot tej marki** (to jest
miejsce, gdzie dwa systemy tej fazy spotykają się i dają emergencję), 24 miesiące karencji.
Kartel trafia do kroniki **dopiero po wykryciu** — inaczej kronika spoileruje graczowi tajemnicę.

**Formowanie związku.** Trzy warunki naraz, wszystkie z §6.6:

```
grievance = clamp( w1 × (zysk_na_pracownika vs udział_płac_docelowy)
                 + w2 × (mediana_płacy_miejskiej_dla_roli − płaca_tutaj) / mediana
                 + w3 × średni_stres
                 + w4 × wypadki_12m, 0, 100)
```
1. `grievance ≥ 55` przez ≥ 60 kolejnych dni,
2. największa spójna składowa grafu relacji **wśród pracowników zakładu** ≥ max(8, 25% załogi)
   (przeszukiwanie ograniczone do załogi — kilkadziesiąt wierzchołków, koszt pomijalny),
3. gęstość potencjalnego członkostwa ≥ 30%.

Żądanie jest zakotwiczone na **znanych** płacach porównywalnych firm — czyli na tym, co pracownicy
wiedzą z grafu relacji i plotki (§5.7), a nie na prawdziwej medianie. Związek może żądać za dużo albo
za mało, bo ma niepełną informację. To jest realizm, który wychodzi z systemu, nie z parametru.

**Negocjacje i strajk.** Rundy tygodniowe; firma kontruje na podstawie swojej sytuacji finansowej
z ksiąg M7; próg akceptacji związku maleje wraz z wyczerpywaniem funduszu strajkowego. Strajk
zatrzymuje produkcję **zakładu** (nie całej firmy), uruchamia kary z kontraktów B2B M6 u odbiorców
(kaskada!), jest publikowany przez media, uderza w markę pracodawcy. Fundusz się kończy — nie ma
strajków wiecznych.


---

## Zmiany wpisane po M10e

Zgodnie z `K-18` i regułą „popraw plan, zanim napiszesz kod". Gwiazdka = zmiana
zakresu albo kryterium. Numeracja `GE-n`.

| # | Co było w planie | Co jest i dlaczego |
|---|---|---|
| GE-1 ★ | „`SupplierRelation` jest **mój**" — typ po stronie M10 (§5.9) | **Reguła jest M10, adres jest M6:** typ mieszka w `sim/supply::b2b::relation`, obok `Exclusives`, z którym jest strukturalnie tożsamy. To ta sama korekta, którą `K-50` zrobił jądru ekonomii. Wszystkie trzy wejścia relacji są w `sim/supply`: terminowość w `SupplyContract`, obrót w `Settlement`, a jedyny czytelnik rabatu i priorytetu to rozstrzygnięcie przetargu. Tabela piętro wyżej znaczyłaby port `Arc<dyn …>` wyłącznie po to, żeby wrócić po liczbę, którą ten crate właśnie sam policzył. Rozstrzygnięcie w 00 §4a jako `K-88` |
| GE-2 ★ | „`discount_bps` — **korekta ceny** przy rozstrzyganiu RFQ" (§5.9) | **Preferencja w funkcji celu, nie obniżka.** Kupujący płaci pełną kwotę z oferty; rabat mówi tylko tyle, że oferta stałego dostawcy wygrywa, mimo że jest droższa. Kryterium WP10.13 brzmi „firma **płaci +3 %** stałemu dostawcy zamiast szukać taniej", czyli opisuje przepłacenie, a nie zniżkę — a gdyby rabat schodził z ceny, sprzedawca płaciłby za własną rzetelność. Pole siedzi na `Quote`, obok premii za wyłączność i w tym samym miejscu kodu: obie są wiedzą o relacji, jedna podnosi cenę, druga obniża ocenę |
| GE-3 ★ | „`trust` liczę z `late_deliveries` i `missed_mass`" (§5.9) | **`late_deliveries` nie rosło nigdy.** `run_contracts` wołało `record_fulfilled(masa, 0)` z twardym zerem, więc `Penalty::grace_minutes` było polem, którego nikt nie czytał, a zaufanie stałoby wyłącznie na masie niedostarczonej. Naprawione: spóźnienie liczy się od terminu harmonogramu. `ponytail:` **to jest spóźnienie wysyłki, nie dostawy** — opóźnienie rozładunku wymaga haka przy przyjęciu partii i należy do M6 (`R2`); dziś mierzy się to, co widać z tego miejsca, i to jest więcej niż zero |
| GE-4 | „`volume_cum: Qty`" (§5.9) | **`Mass`.** Cały łańcuch M6 liczy w gramach, `Qty` (milisztuki) nie ma w nim ani jednego nośnika, a druga jednostka w tej strukturze znaczyłaby przeliczenie przy każdym zapisie |
| GE-5 ★ | „hazard wykrycia … `+ 0,2 % × liczba_niezadowolonych_wtajemniczonych` (menedżer z nastrojem < −40 lub zwolniony)" (§5.9) | **Wtajemniczonym niezadowolonym jest załoga w sporze z pracodawcą.** Nastroju menedżera nie ma dziś gdzie przeczytać: `ManagementQuality` jest umiejętnością, nie samopoczuciem, a menedżer zwolniony nie zostawia śladu, po którym dałoby się go policzyć. Załoga w sporze jest tym samym sygnałem i **ma nośnik** — to są ludzie, którzy mają powód mówić o firmie źle. Przy okazji jest to drugie miejsce, w którym dwa systemy tej podfazy się spotykają, obok uderzenia w markę |
| GE-6 ★ | „Kara: 10 % obrotu 12-miesięcznego per członek" — bez wskazania drogi (§5.9) | **Karę nakłada urząd antymonopolowy M8 i idzie ona przez `ChargeRegistry`**, czyli tą samą drogą co każda inna należność (`CB-3`, niezmiennik `Σ Assessed = Σ Settled + Σ Overdue + Σ Abated`). Wymagało to jednej zmiany w `sim/city`: `Case` dostaje `origin: CaseOrigin::{Threshold, Cartel}`, bo urząd prowadzi od tej chwili **dwie różne sprawy** — za dominację (przymusowy podział) i za zmowę (kara pieniężna). Bez tego rozróżnienia wykrycie kartelu zamykałoby sklepy wszystkich członków naraz, czyli karałoby dzielnicę mocniej niż zmowę |
| GE-7 ★ | „`strike_fund: Money`" jako pole związku (§5.9) | **Fundusz jest miarą wytrzymałości, a nie saldem**: sumą oszczędności gospodarstw strajkujących **ponad miesiąc utrzymania**, zdjętą w chwili wyjścia z pracy i topniejącą o utracone zarobki. Składek związkowych nikt w tej grze nie płaci, więc konto w `Books` byłoby puste. Rezerwa jednego miesiąca jest istotą rzeczy: rodzina wydająca cały dochód na czynsz i jedzenie nie ma z czego strajkować, choć coś jej na koncie leży. **Sufit trzeba nazwać wprost i jest w nagłówku `union.rs`:** salda gospodarstw w czasie strajku **nie maleją**, bo dochód gospodarstwa jest egzogeniczny (decyzja nr 2 fazy M5, a `PayrollOutbox` nie ma konsumenta od M7b). Realny pieniądz strajk zabiera **firmie** — lista płac nie płaci za dni postoju i widać to w rachunku wyniku zakładu; po stronie załogi zostaje liczba mówiąca, jak długo wytrzyma. Kiedy lista płac dojdzie do gospodarstw, fundusz przestanie być modelem i stanie się odczytem (`FF-29`) |
| GE-8 ★ | „`members: Vec<CitizenId>`" w `Union` (§5.9) | **Listy członków nie ma.** Tą listą jest `SocialIndex::coworkers(site)` po odsianiu uczniów (`K-74`); druga kopia musiałaby przeżyć każde zwolnienie, śmierć i przeprowadzkę, więc rozjechałaby się z pierwszą przy pierwszym odejściu. Związek trzyma **gęstość**, a skład odtwarza z indeksu. Z tego samego powodu nie powstaje `UnionScope::{Firm, Branch}`: decyzja `D6` fazy rozstrzygnęła, że związek jest zakładowy, a wariant bez ścieżki powstania wygląda w kodzie tak samo jak działający (`K-67`) |
| GE-9 ★ | „`w4 × wypadki_12m`" — czwarty człon żalu (§5.9) | **Nie powstaje.** Licznika wypadków przy pracy nie ma dziś nigdzie w projekcie: `DeprivationEffect::AccidentRisk` istnieje w słowniku i wpada w pustą gałąź `sim/agents::needs`. Człon nad liczbą, której nikt nie liczy, byłby zerem udającym wagę. Wraca razem z wypadkami, kiedy te dostaną źródło; trzy pozostałe wagi sumują się do stu i walidator danych tego pilnuje |
| GE-10 ★ | „Strajk … jest publikowany przez media, **uderza w markę pracodawcy**" (§5.9) | **Brakowało ostatniego ogniwa i M10e je dokłada.** Do M10d publikacja budowała wyłącznie znajomość **tytułu**, który pisze; o kim jest tekst, nie docierało nigdzie. Teraz zdarzenie społeczne i firmowe o podmiocie z marką zabiera czytelnikom sympatię do tej marki — nowy wariant `Touch::Scandal` w pamięci mieszkańca, jedyny kontakt ruszający afinitet bez zakupu. **Dociera tylko do tych, którzy markę znają**: kto o firmie nie słyszał, ten po przeczytaniu o cudzym strajku nadal o niej nie słyszał, a guard stoi w `magnat_agents::touch`, w jedynym wejściu do pamięci. `ponytail:` siła uderzenia prasowego jest stałą w `sim/media`, a nie w `data/tuning/brand.ron` — sufit nazwany, droga wyjścia to pole `scandal_drop`, kiedy balansator zmierzy, ile marek rocznie obrywa od prasy |
| GE-11 ★ | Franczyza, spółka celowa (JV) i integracja pionowa/pozioma (§2 pkt 10 dokumentu fazy) | **Nie wymagają ani jednej linii kodu i to jest cały wpis.** JV to firma, której właścicielami są dwie inne firmy — `Owner::Firm` istnieje od M7a, a `Firm::owners_sum_ok` pilnuje sumy 10 000 bp (`FE-7`, `K-85`). Franczyza i licencja to `ContractId` z M6 („bez nowego typu umowy", M10 §6); licencję technologiczną zbudował tą drogą M10c. Integracja pionowa i pozioma to przejęcie, a przejęcia zbudował M10d — czy przejmowana firma jest dostawcą, czy konkurentem, jest pytaniem do katalogu towarów, a nie osobnym mechanizmem |
| GE-12 ★ | Decyzja `D10`: „wypełnić `CommuteMatrix` przy okazji M10e" (§9.2 dokumentu fazy) | **Wykonana, ale z innego źródła, niż zapowiadał kontrakt.** M10 §6 wskazywał snapshot `TravelTimeMatrix` — a ta macierz w świecie świeżo postawionym jest **pusta**: wypełniają ją obserwacje realnych przejazdów mezo, a historia „na sucho" żadnego nie wykonuje. Zdjęcie pustej macierzy dałoby tę samą płaską stałą, którą miało zastąpić. Macierz wypełnia się więc z **geometrii miasta**: odległość między środkami ciężkości dzielnic przez nominalną prędkość miejską, przy czym środek liczy się z domów mieszkańców (a dla dzielnicy przemysłowej — z jej zakładów), bo dojeżdża się do ludzi, a nie do środka wielokąta. Korekta kontraktu: nie „M4 wypełnia", tylko „M10 wypełnia z geometrii, M4 uściśla obserwacjami, kiedy są". Rekrutacja w makro przestaje być zamknięta w dzielnicy i staje się **ważona gotowością do dojazdu** — liniowo od pełnej przy dojeździe wewnątrzdzielnicowym do zera przy godzinie w jedną stronę |
| GE-13 | Plan nie mówił, co się dzieje, gdy ugoda zapada **w trakcie** strajku | **Zakład wraca do pracy tą samą drogą co przy kapitulacji.** Pierwsza wersja zdejmowała przestój tylko przy wyczerpanym funduszu, więc po ugodzie hala stała dalej, lista płac nie płaciła, a zdarzenie `social/strike` nie gasło nigdy — bo jego sonda czyta właśnie to pole. Złapał to test „strajk zatrzymuje zakład i zabiera załogę z listy płac" i to jest dokładnie ta klasa błędu, dla której on istnieje |
| GE-14 | Plan nie mówił, skąd bierze się rozrzut długości strajku | **Z rzutu `StreamId::StrikeResolve` na wielkość ustępstwa** w granicach, które firma sama sobie wyznaczyła (sufit z marży, podłoga jego połowa). Bez niego każdy spór o tej samej marży kończyłby się tej samej doby, a mediana z kryterium WP10.14 byłaby jedną liczbą bez rozrzutu — kryterium mówi zaś o **paśmie** 4–21 dni, czyli rozrzut zakłada. Czy strony się dogadają, rzut nie rozstrzyga: to jest porównanie dwóch progów |

---

## Zmiany wpisane po M10b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M10b.
Szczegóły — tabela `F-n` w `M10b-marka-i-media.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FE-1 | **Graf relacji ma już wyjście na zewnątrz `sim/agents`: `social::relations_of(world, citizen) -> Vec<(Entity, u8)>`.** Zwraca drugą stronę relacji i jej wagę, w kolejności slabu | Powstało dla kampanii PR, która przechodzi po relacjach z góry (`magnat_media`). Związki zawodowe robią to samo z tego samego miejsca — **nie ma potrzeby pisać drugiego przejścia po slabie**, a dwa przejścia o tej samej regule rozjechałyby się przy pierwszej zmianie wagi relacji |
| FE-2 | **`SocialIndex::coworkers(site)` mierzy wreszcie to, co obiecuje** — ale to zasługa `K-74`, nie M10b; tutaj tylko potwierdzenie, że nic tego nie cofnęło | §5.9 liczy warunek powstania związku na spójnej składowej grafu relacji **wśród pracowników zakładu**. Uczeń wypadł z `by_site` i ma własny indeks (`DK-8`) |
| FE-3 | **Strajk ma już nośnik po stronie opinii: marka firmy.** Kampania PR, publikacja o strajku i rozczarowanie klienta piszą do tego samego slotu (`BrandAffinity`) | M10 §1 obiecuje kaskadę „strajk → gazeta pisze → marka gracza traci afinitet". Dwa z trzech ogniw są gotowe: publikacja (`Story`, M10b) i afinitet (`Touch::Media`). Brakuje wyłącznie zdarzenia strajku jako wejścia do redakcji — czyli tego, co robi ta podfaza |
| FE-4 | **Blok `StreamId` M10: zajęte 280–284 i 292–295; wolne 285–291 i 296–299.** `CartelDetection = 289`, `UnionFormation = 290`, `StrikeResolve = 291` są nadal wolne i zarezerwowane imiennie | — |

---

## Zmiany wpisane po M10c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10c. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `FD-n`
w `M10c-rd-i-nowe-produkty.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FE-5 | **Blok `DecisionReason` M10: zajęte 800–807.** Wolne: **808–899**. `StreamId` M10: wolne **286–291** i **296–299** | M10c wziął `RnDBreakthrough = 285` i powody 804–807 |
| FE-6 | **`StockCat` ma od M10c dziewięć wariantów**, a `Household.stock` dziewięć bajtów (`K-83`). Kolejność jest kontraktem zapisu gry i dopisywać wolno wyłącznie na końcu | Żądanie płacowe związku liczy się z budżetu gospodarstwa, a ten dzieli się na koperty indeksowane `StockCat`. Wpis, który zakłada osiem kopert, policzy o jedną za mało |

---

## Zmiany wpisane po M10d

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10d. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `GD-n`
w `M10d-gielda-przejecia-ubezpieczenia.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FE-7 ★ | **Udziału w firmie nie liczy się w akcjach, tylko w punktach bazowych `Firm.owners`** (`GD-1`, `K-85`). Kartel, franczyza i spółka celowa (`JV`) dzielą tę samą tablicę: „firma A ma 40 % w B" to `OwnerShare { owner: Owner::Firm(A), bp: 4_000 }` | Drugiej tablicy własności nie ma i nie będzie. Wpis, który zakłada `Holding` albo `total_shares`, opisuje strukturę, której nie ma — a `Firm::owners_sum_ok` pilnuje sumy 10 000 od M7a |
| FE-8 | **`Equity::take_disclosures()` jest gotową skrzynką zdarzeń korporacyjnych.** Niesie `(firma, posiadacz jako `Subject`, bp, czy to kontrola, doba) i opróżnia się przy odbiorze | Kartel wykryty przez regulatora M8 i przejęcie to dla kroniki ta sama klasa zdarzenia. Drugi strumień o tym samym kształcie rozjechałby się z pierwszym |
| FE-9 ★ | **Strajk ma teraz drugi nośnik obok marki: kurs.** Notowana firma, w której stanęła produkcja, publikuje gorszy wynik w dobie `(m + 1) × 30 + 45`, a przekonanie inwestorów rusza się dopiero wtedy — chyba że wcześniej pójdzie plotka | To jest mierzalna konsekwencja strajku, której M10 §1 nie obiecywało wprost, a która wychodzi za darmo z `GD-3`. Uwaga przy pisaniu testu: między strajkiem a reakcją kursu mija **półtora miesiąca gry**, więc przebieg krótszy niż 75 dób nie zobaczy niczego |
| FE-10 | **Blok `StreamId` M10: zajęte 280–287 i 292–295. Wolne: 288–291 i 296–299.** `CartelDetection = 289`, `UnionFormation = 290`, `StrikeResolve = 291` są nadal wolne i zarezerwowane imiennie; **`PerilDraw = 288` zostaje zarezerwowany i niezajęty** (`K-85`) | M10d wziął `InvestorNoise = 286` i `EarningsNoise = 287` |
| FE-11 | **Blok `DecisionReason` M10: zajęte 800–816. Wolne: 817–899** | M10d dołożył dziewięć powodów giełdy i ubezpieczeń |
| FE-12 | **`Owner` ma pięć wariantów i to jest komplet dla własności.** Kartel nie jest własnością, więc nie dokłada wariantu; JV **jest** i nie potrzebuje niczego nowego — spółka celowa to firma, której właścicielami są dwie inne firmy | Ta sama reguła, którą `GD-8` zastosował do funduszy inwestycyjnych: nowy rodzaj właściciela wchodzi razem ze swoim kontem w `Books`, a nie jako wyjątek w jednym module |
