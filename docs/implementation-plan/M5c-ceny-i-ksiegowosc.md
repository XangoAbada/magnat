# M5c — Ceny i księgowość

Podfaza 3 z 5 fazy **M5 — Gospodarka detaliczna** (`M5-gospodarka-detaliczna.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M5b (transakcje). |
| **Pakiety robocze** | WP6, WP7, WP11 |
| **Projekt techniczny** | §5.6, §5.8 |
| **Wynik do pokazania** | Sklep AI podnosi cenę przy niedoborze i obniża przy zaleganiu; rachunek wyników i bilans domykają się co do grosza. |
| **Kryterium zamknięcia** | Kryteria WP6, WP7 i WP11. |
| **Poprzednia / następna** | `M5b-sklep-i-zakup.md` · `M5d-budzety-banki-inflacja.md` |

Polityki cenowe AI, pełna księgowość sklepu (rachunek wyników, bilans, przepływy) i delegowanie polityk cenowych graczowi.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP6 | Polityki cenowe AI | WP2, WP3, WP5 | L |
| WP7 | Księgowość sklepu | WP1, WP3, WP5 | L |
| WP11 | Polityki cenowe delegowane przez gracza | WP6 | S |

### WP6 — Polityki cenowe AI
Cztery mechanizmy korekty (magazyn, konkurencja z opóźnieniem, eksperyment, przecena psującego się)
składane w jedną cenę w arytmetyce punktów bazowych.
Kryterium: sklep z nadmiarem zapasu obniża cenę w ciągu 3 dni; sklep z brakami podnosi;
sklep obok konkurenta, który obniżył cenę, reaguje **nie wcześniej niż po 1 i nie później niż po 7 dniach**
(test mierzy opóźnienie na 100 firmach i weryfikuje rozkład).

### WP7 — Księgowość sklepu
Plan kont jako enum, dziennik z zapisami zbilansowanymi (Σ Wn = Σ Ma, tolerancja 0),
raporty wyprowadzane z dziennika, nie z liczników obok.
Kryterium: po roku symulacji bilans się zamyka co do grosza; suma `Revenue − Cogs − koszty`
z RZiS równa się zmianie `RetainedEarnings`; wartość `InventoryGoods` w bilansie równa się
`Σ StockLine.cost_total` co do grosza.

### WP11 — Polityki cenowe delegowane przez gracza
**Ten sam** typ `PricePolicy` co AI (§6.3 mówi wprost: „z tym samym zestawem narzędzi co AI").
Zero osobnej ścieżki kodu. UI: wybór wariantu + parametry + podgląd „co by się stało z ceną dziś".
Kryterium: polityka „−2% względem najtańszego konkurenta w promieniu 3 km" ustawiona przez gracza
i przez AI daje identyczną cenę przy identycznym stanie.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.6 Polityki cenowe (§6.3)

```rust
pub enum PricePolicy {
    Fixed          { price: Money },
    Markup         { target_margin_bp: i32 },
    MatchCompetitor{ delta_bp: i32, radius_m: u32, reference: CompetitorRef },  // „−2% vs najtańszy w 3 km"
    Dynamic        { target_margin_bp: i32, floor_margin_bp: i32, ceil_margin_bp: i32 },
}

pub enum CompetitorRef { Cheapest, Median, Named(SiteId) }

pub struct PriceController {            // stan per (SiteId, GoodId)
    pub policy: PricePolicy,
    pub current: Money,
    pub last_change: Tick,
    pub observed: CompetitorSnapshot,   // ceny konkurencji z opóźnieniem 1–7 dni
    pub elasticity: Option<ObservedElasticity>,
    pub experiment: Option<PriceExperiment>,
    pub delegated: bool,                // true = gracz oddał sterowanie polityce
}

pub fn reprice(pc: &mut PriceController, ctx: &PricingCtx, t: Tick) -> Option<DecisionReason>;
```

Składanie ceny — **cała arytmetyka w punktach bazowych i `i64`, zero floatów** (dok. 00 §2):

```
base_bp   = unit_cost · (10_000 + target_margin_bp) / 10_000

adj_stock = clamp(k_stock · (stock_bp_of_target − 10_000) / 10_000, −2000, +1500)
            // nadmiar zapasu → ujemne (obniżka); brak → dodatnie (podwyżka)
adj_comp  = clamp(k_comp · (observed_ref_price · 10_000 / base − 10_000) / 10_000, −1500, +1500)
adj_elast = z eksperymentu: krok w stronę p* = p · (1 + 1/(e + 1)), ograniczony do ±500 bp
adj_spoil = schodkowo wg pozostałego terminu ważności: −2000 / −4000 / −6000 bp

price = clamp(base_bp · (10_000 + adj_stock + adj_comp + adj_elast + adj_spoil) / 10_000,
              floor = unit_cost · (10_000 + min_margin_bp) / 10_000,
              ceil  = unit_cost · (10_000 + max_margin_bp) / 10_000)
```

**Podstawa ceny (K-7).** `unit_cost` jest kwotą netto, a cena detaliczna to brutto — w M5 pokrywają
się, bo VAT ≡ 0, ale `reprice` musi porównywać w jednej podstawie **od pierwszego dnia**, inaczej
w M8 marże skoczą o stawkę VAT-u bez zmiany żadnej polityki. Dlatego: całe składanie ceny
(`base_bp`, `floor`, `ceil`, `adj_comp`) dzieje się **netto**, a przeliczenie na `Offer.unit_price`
w podstawie `GrossRetail` jest ostatnim krokiem, przez `TaxEngine` (w M5 `NoTax`, mnożnik 1).
Cena konkurencji z `CompetitorSnapshot` przychodzi brutto i jest sprowadzana do netto tym samym
mechanizmem przed użyciem w `adj_comp`.

`floor` jest krytyczny: bez niego sklepy w nadmiarze zapasu wpadają w spiralę deflacyjną poniżej
kosztu. To dokładnie ten mechanizm, który bramka balansatora ma pilnować (sekcja 7.4).
`k_stock`, `k_comp`, `min_margin_bp`, `max_margin_bp` — z osobowości firmy (§12 PRD, w M5 z danych).

**Obserwacja konkurencji z opóźnieniem 1–7 dni (§6.3)**

```rust
pub struct CompetitorSnapshot {
    pub entries: Vec<CompetitorEntry>,   // posortowane po SiteId
    pub refreshed: Tick,
    pub delay_days: u8,                  // 1..=7, losowane raz per firma
}
pub struct CompetitorEntry { pub site: SiteId, pub good: GoodId, pub price: Money, pub seen_at: Tick }
```

`delay_days = 1 + rng(world_seed, StreamId::CompetitorDelay, firm.index(), 0) % 7`, stałe dla firmy
(to jest jej „czujność"). System `observe_competitors` (EveryDay) odświeża tylko te wpisy, których
wiek przekroczył `delay_days` — jeden przelot indeksu w promieniu na firmę, nie skan całego rynku.
Sklep **działa na starej cenie konkurenta** — to jest źródło realnych błędów decyzyjnych AI
i pole manewru dla gracza (przecena na 3 dni, zanim konkurencja zauważy).

**Eksperymenty cenowe (§6.3)**

```rust
pub struct PriceExperiment { pub direction: i8, pub magnitude_bp: i32,
                             pub started: Tick, pub len_days: u8, pub baseline_units: Qty }
pub struct ObservedElasticity { pub e: f32, pub measured_at: Tick, pub samples: u32 }
```

Uruchamiane gdy `personality.risk > próg`, nie częściej niż co 30 dni, magnitude ±3–10%.
Po zakończeniu okna: `e = ln(q1/q0) / ln(p1/p0)` (float — wynik steruje parametrem polityki,
nie kwotą pieniężną, więc dozwolone; wpis do stanu trwałego kwantyzowany do `i32` w bp).
Jeśli `|Δln(q)|` poniżej progu szumu — eksperyment nierozstrzygnięty, `e` bez zmian.
Gracz widzi wynik eksperymentu w raporcie (§6.4: „gracz może ją zmierzyć eksperymentem").

**Polityki gracza (WP11)** — ten sam `PricePolicy`, ta sama funkcja `reprice`. Różnica wyłącznie
w tym, kto ustawia wariant. `delegated == false` oznacza `PricePolicy::Fixed` ustawiony ręcznie.

### 5.8 Księgowość sklepu (§6.9)

```rust
pub struct Ledger {
    pub site: SiteId,
    pub firm: FirmId,
    balances: BTreeMap<LedgerAccount, Money>,
    journal: RingBuffer<JournalEntry>,          // ostatnie N; starsze na dysk (engine/io)
    pub periods: Vec<PeriodClose>,              // miesięczne domknięcia
}

pub enum LedgerAccount {
    // Aktywa
    Cash, BankCurrent, InventoryGoods, FixedAssets, AccumDepreciation,
    // Pasywa — rozbite na *Payable na wniosek M8 (zobowiązanie ma mieć wierzyciela i termin)
    TradePayable,        // dostawcy
    TaxPayable,          // VAT + daniny z ChargeRegistry (M8); w M5 zawsze 0
    WagePayable,         // naliczone, niewypłacone (hook M7)
    LoansShort, LoansLong, Equity, RetainedEarnings,
    // RZiS
    Revenue, Cogs, WagesExpense, RentExpense, UtilitiesExpense,
    DepreciationExpense, InterestExpense, WriteOffExpense,
    TaxExpense,          // HOOK M8 — w M5 zawsze 0
}

pub struct JournalEntry { pub tick: Tick, pub tx: Option<TxId>,
                          pub lines: SmallVec<[(LedgerAccount, Money); 4]>,  // Σ == 0
                          pub reason: DecisionReason }

pub fn post(ledger: &mut Ledger, e: JournalEntry) -> Result<(), LedgerError>; // odrzuca niezbilansowane

pub fn income_statement(l: &Ledger, from: Tick, to: Tick) -> IncomeStatement;
pub fn balance_sheet  (l: &Ledger, at: Tick)              -> BalanceSheet;
pub fn cash_flow      (l: &Ledger, from: Tick, to: Tick)  -> CashFlow;   // metoda bezpośrednia z dziennika
```

**Wycena zapasów — średnia ważona z zabezpieczeniem przed dryfem**

```rust
/// Koszt własny sprzedaży przy sprzedaży qty_sold z linii o (qty, cost_total).
pub fn take_cogs(line: &mut StockLine, qty_sold: Qty) -> Money {
    debug_assert!(qty_sold <= line.qty);
    let cogs = if qty_sold == line.qty {
        line.cost_total                       // zmiatamy resztę — linia zeruje się DOKŁADNIE
    } else {
        Money(mul_div_i128(line.cost_total.0, qty_sold.0, line.qty.0))  // zaokrąglenie half-up
    };
    line.qty -= qty_sold;
    line.cost_total -= cogs;
    cogs
}
```

Bez gałęzi „zmiatania reszty" `cost_total` nie schodzi do zera przy wyzerowanym `qty` i bilans
przestaje się zamykać po kilku tysiącach transakcji. To nie jest optymalizacja, to warunek poprawności.

Zdarzenia księgowe w M5: sprzedaż (Revenue/Cogs), zakup u dostawcy (InventoryGoods/TradePayable→Cash),
odpis towaru przeterminowanego (WriteOffExpense), czynsz, media, amortyzacja wyposażenia (liniowa,
miesięczna), odsetki i raty kredytu, wypłaty (M5: stała kwota, hook M7).

---

---

## Korekty projektu technicznego M5c

Zgodnie z regułą „popraw plan, zanim napiszesz kod". Gwiazdka = zmiana zakresu
albo kryterium.

| # | Korekta | Dlaczego |
|---|---|---|
| W-1 ★ | **Znak korekty zapasu w §5.6 jest odwrotny do zamierzonego.** Wzór `adj_stock = k_stock · (stock_bp_of_target − 10 000) / 10 000` przy dodatnim `k_stock` daje **podwyżkę** przy zaleganiu, a zdanie obok mówi „nadmiar zapasu → ujemne". Zaimplementowane jest `k_stock · (10 000 − stock_bp_of_target) / 10 000` | Kryterium WP6 („sklep z nadmiarem zapasu obniża cenę w ciągu 3 dni") sprawdza dokładnie ten znak, więc wzór z planu zaczerwieniłby własne kryterium. Przycięcie ⟨−2000, +1500⟩ bp zostaje bez zmian i jest asymetryczne z rozmysłu: zalegający towar ma więcej miejsca w dół niż braki w górę |
| W-2 ★ | **Dolny ogranicznik marży schodzi razem z przeceną psującego się towaru.** `floor = unit_cost · (10 000 + min_margin_bp + adj_spoil)`, a nie `(10 000 + min_margin_bp)` | Inaczej `adj_spoil` jest **martwym kodem przy każdej sensownej marży minimalnej**: −60 % od ceny z marżą 30 % to 104 gr, a podłoga przy marży minimalnej 5 % stoi na 210 gr. Kryterium „przecena psującego się" spełniałoby się wtedy tożsamościowo — a to jest ten sam błąd, który M4 złapał trzy razy (`S-4`). Uzasadnienie merytoryczne jest mocniejsze niż arytmetyczne: alternatywą dla wyprzedaży jest odpis 100 % (`WriteOffExpense`), więc towar w ostatniej dobie ważności **jest** wart mniej niż kosztował. Dla towaru, który się nie psuje, `adj_spoil == 0` i podłoga stoi dokładnie tam, gdzie pilnuje jej bramka G3 |
| W-3 ★ | **`CompetitorSnapshot` trzyma agregat per towar, a nie wpis per (sklep, towar).** `CompetitorEntry { good, cheapest, cheapest_site, median, named, offers, seen_at, rev }`, posortowane po `GoodId` | `CompetitorRef` pyta o najtańszego, o medianę albo o **jednego** wskazanego konkurenta — pełny cennik każdego sąsiada to 1,2 mln wpisów na metropolię za daną, z której czyta się trzy liczby. Wskazany konkurent rozwiązuje się w chwili obserwacji i ląduje w `named`. Przy okazji `observed` przenosi się z `PriceController` (per towar) na `Shop` (jeden obraz na sklep) — inaczej ten sam agregat leżałby w czterdziestu kopiach |
| W-4 ★ | **`PurchaseIntent` dostaje pole `cogs`.** `fulfil` zdejmuje towar z półki i **odrzucał** zwrócony koszt nabycia (`let _ = sl.take(take)`), a `return_goods` oddawał same sztuki | Bez tego pola koszt znikał przy każdym nieudanym rozliczeniu i `InventoryGoods` rozjeżdżał się z wyceną zapasu — czyli niezmiennik P5 pękał w miejscu, którego M5b nie mógł zobaczyć, bo księgi jeszcze nie było. To nie jest dług M5b, tylko pierwszy moment, w którym dało się to zmierzyć |
| W-5 | **`ShelfLine` dostaje `expires`.** Data ważności wędruje z zaplecza na półkę przy uzupełnianiu (wcześniejsza z dwóch, tak samo jak przy dostawie) | Odpis (§5.8) i przecena psującego się (§5.6) widziałyby wyłącznie zaplecze, a psuje się to, co leży na wierzchu. `deliver_now` dostaje z tego powodu argument `Tick` — przyjęcie towaru jest zdarzeniem księgowym i musi mieć datę |
| W-6 | **`Ledger.balances` to tablica indeksowana `LedgerAccount::as_index()`, nie `BTreeMap`.** Plan §5.8 mówił `BTreeMap<LedgerAccount, Money>` | Sygnatura `kernel::ledger_post(lines, acc: &mut [Money; LEDGER_ACCOUNT_COUNT])` z §6 dokumentu fazy wymaga tablicy — rdzeń **nie alokuje**, więc mapa nie wchodzi w grę. Tablica 21 kwot to 168 B na zakład wobec mapy z węzłami na stercie; kolejność wariantów `LedgerAccount` staje się przez to kontraktem zapisu gry, tak samo jak kolejność `StockCat` |
| W-7 ★ | **Pierścień dziennika prowadzą wyłącznie zakłady śledzone** (`LostSaleTracking != None`); salda i miesięczne domknięcia prowadzą wszystkie. Raporty czytają salda i `PeriodClose`, a okno zapisów jest podglądem dla panelu | Zapis per transakcja × 2 tys. sklepów × rok gry to gigabajty, a odbiorcą pełnej historii jest wyłącznie panel gracza. To ta sama flaga i ta sama zasada, którą M5b zastosował do utraconych sprzedaży. Wymaganie „raporty wyprowadzane z dziennika, nie z liczników obok" zostaje spełnione co do treści: salda **są** wynikiem księgowania i nie ma do nich drugiej drogi (`post` → `kernel::ledger_post`). Konsekwencja jawna: `cash_flow` dla zakładu nieśledzonego zwraca `complete: false` zamiast milcząco obciętej liczby |
| W-8 | **Domknięcie miesiąca idzie wieloma zapisami po dwie linie, po jednym na konto wynikowe**, a nie jednym zapisem wielolinijkowym | `JournalEntry` jest `Copy` i mieści cztery linie — bo alokacja na każdą sprzedaż to 450 tys. alokacji na dobę. Domknięcie rozbite na pary jest przy okazji czytelniejsze w dzienniku: widać, skąd wziął się wynik, zamiast jednej pozycji „domknięcie" |
| W-9 | **`ObservedElasticity.e` jest `i32` w punktach bazowych, nie `f32`** (wykonanie decyzji otwartej nr 11 zgodnie z propozycją). Float żyje wyłącznie jako wartość pośrednia w `det_math::ln` | Elastyczność wchodzi do stanu trwałego, czyli do hasha i do zapisu gry (00 §2). `PriceController.last_experiment` jest z podobnego powodu `Option<Tick>`, a nie `Tick`: `Tick(0)` jako „nigdy" zablokowałby eksperymenty przez pierwsze 30 dób świata, czyli dokładnie w okresie, w którym balansator mierzy rozbieg |
| W-10 ★ | **Koszty stałe zakładu są w M5c, w danych** (`data/economy/shop.ron`): czynsz, media, płace i amortyzacja wyposażenia per miejsce na półce. §5.8 wymienia je wśród zdarzeń księgowych, ale żaden pakiet nie był ich właścicielem | Sklep bez kosztów stałych ma marżę zawyżoną o całą tę pozycję, a bramki balansatora G1–G3 stroiłyby **nie ten świat**. Sufity są nazwane w pliku razem ze ścieżkami wyjścia (czynsz M7/M10, media M8, płace M7). Amortyzacja jest jedynym kosztem bezgotówkowym i dlatego nie idzie tą samą ścieżką co przelew |
| W-11 | **`Market::record_capital` jest osobnym wywołaniem.** `open_shop` nie widzi `Books`, a kapitał obrotowy wpływa przelewem, który robi wołający | Pieniądz ma jedno wejście (`Books::transfer`, kryterium WP1) i nie wolno go dublować w księdze zakładu. Bez tego wywołania `BankCurrent` od pierwszej minuty nie zgadza się z saldem rachunku — i to jest pierwsza rzecz, którą sprawdza test WP7 |
| W-12 | **`TradePayable` chodzi w M5 na saldzie Wn.** Dostawa zewnętrzna jest płatna przy zamówieniu, więc zapłata (Wn `TradePayable` / Ma `BankCurrent`) wyprzedza przyjęcie towaru (Wn `InventoryGoods` / Ma `TradePayable`) | `InventoryGoods` ma rosnąć **dopiero przy przyjęciu**, inaczej P5 pęka na każdym towarze będącym w drodze. Ujemne saldo zobowiązania to zaliczka u dostawcy i jest prawdziwe; M6 wnosi terminy płatności i saldo staje się tym, czym nazwa obiecuje |
| W-13 ★ | **Kolejność kroków doby sklepu jest kontraktem: odpis → obserwacja → przecena → zaopatrzenie.** `observe` **przed** `reprice`, a warunek odświeżenia to `wiek >= delay_days`, nie `>` | Przecena konkurenta z doby `D` ma wejść do obrazu najwcześniej w dobie `D+1`. Przy odwrotnej kolejności reakcja mieści się w 2..=8 dobach, a przy warunku `>` — w 1..=8. Kryterium WP6 żąda przedziału **1..=7** i test mierzy go na 100 firmach, więc obie te pomyłki zaczerwieniłyby go dopiero na rozkładzie, nie na pojedynczym przypadku |
| W-14 ★ | **Domyślny promień obserwacji konkurencji to 1 200 m, nie 3 000 m.** Polityka `MatchCompetitor` może zażądać większego („−2 % vs najtańszy w 3 km") i wtedy płaci za niego ten sklep, a nie cały rynek | Pomiar: dobowe odświeżenie obrazu dla 2 tys. sklepów metropolii kosztowało **161 ms** przy 3 000 m i **27 ms** przy 1 200 m. §7.3 nie ma linii dla `observe_competitors`, bo ta ścieżka powstaje dopiero teraz — liczba jest więc dopisana do §7.3 razem z budżetem (niżej). 1 200 m to ten sam promień, w którym `choice.ron` szuka ofert dla mieszkańca: sklep, którego klient nie rozważa, nie jest konkurentem |
| W-15 | **`DecisionReason::Repricing` niesie `PriceDriver`** — nowy słownik w `engine/core` (`K-30`), nie `UtilityKind` | `UtilityKind` opisuje wymiar oceny kupującego, a nie człon korekty cenowej sprzedawcy; wciskanie tam `Stock` i `Spoilage` zepsułoby kartę inspekcji po stronie M3. Ładunek centralnego enuma musi mieszkać w `core` (`K-12`, `K-20`) |
| W-16 ★ | **Cel zamówienia nie może przeżyć terminu ważności towaru.** `ReorderPolicy.target` to `min(BACKROOM_MULTIPLE, shelf_life_days)` wyłożeń, a nie zawsze osiem | Stała `BACKROOM_MULTIPLE = 8` z M5b była nieszkodliwa, dopóki nic się nie psuło. Od M5c sklep zamawiający osiem wyłożeń chleba o **trzydniowym** terminie odpisuje pięć z nich: scenariusz `m5shop` pokazał odpisy na **13,4 mln zł przy obrocie 9,5 mln zł** przez 40 dób i wynik sklepu −121 tys. zł. To nie jest błąd polityki cenowej ani księgowej — to interakcja, której przed odpisem nie dało się zobaczyć. Kalibracja docelowa należy do balansatora (M5e); tutaj wchodzi wyłącznie ogranicznik, który sprawia, że sklep nie zamawia towaru, którego nie zdąży sprzedać |
| W-17 ★ | **Niezmiennik świata z `U-17` nie domyka się, kiedy w scenariuszu jeździ ruch** — i nie jest to wina M5c. Bramką scenariusza `m5shop` jest od teraz niezmiennik P1 w **księgach** (tolerancja 0) plus domknięcie księgowości zakładu; suma całego świata jest **mierzona i wypisywana z rozbiciem na pięć składników**, a nie bramkowana | Mieszkaniec płaci za paliwo, bilet i taryfę z komponentu `Wealth`, a drugą stroną jest `FuelLedger`/`FareLedger` z `sim/traffic` — rejestr, nie konto. Scenariusz M5b sumował tylko `Household.{cash, bank, savings}`, więc **obie** strony tych przepływów były poza sumą i wynik wychodził 0 gr przez 8 dób. Po dopisaniu `society::total_money` (spadki, emigracja, `Wealth`) i obu rejestrów zostaje −5,6 tys. zł przez 31 dób i **+63,2 tys. zł** przez 40 — rzędu 0,006–0,06 %, ale **ze zmianą znaku**, więc kanałów bez pary jest co najmniej dwa i działają w przeciwne strony (pierwszy podejrzany: `FareLedger.taxi_revenue`, które rośnie bez zapisu po stronie `Wealth`). Wniosek ogólniejszy niż ta jedna liczba: **niezmiennik pieniądza testuje się na przebiegu dłuższym niż miesiąc gry**, bo krótszy nie uruchamia ani demografii, ani migracji. Domknięcie: decyzja otwarta nr 16 dokumentu fazy |
| W-18 | **`cargo fmt --all --check` jest czerwone na `master` od dawna** i M5c tego nie naprawia: rozjazd obejmuje 86 plików w dziewięciu crate'ach, z których M5c dotyka ośmiu | Formatowanie całego repozytorium utopiłoby zmianę merytoryczną w mechanicznym diffie na kilka tysięcy linii. To jest praca dla `R1` (refaktor po M5), gdzie i tak przenosi się symbole między plikami bez zmiany zachowania — i gdzie diff formatujący niczego nie zasłoni. Zgłoszone tutaj, bo bramka CI deklaruje `fmt` jako warunek, a warunek niespełniany od kilku faz przestaje być warunkiem |

### Uzupełnienie §7.3 — budżety ścieżek M5c

| Ścieżka | Budżet | Zmierzone (release, 2 tys. sklepów × 18 towarów) |
|---|---|---|
| `reprice` dla 2 tys. sklepów × asortyment, raz na dobę | < 40 ms | **1,11 ms** |
| `observe_competitors`, **pełne** odświeżenie 2 tys. sklepów | nowy — ustalony na < 40 ms | **27,4 ms**; w przebiegu realnym odświeża się ok. 1/4 sklepów na dobę (czujność 1–7 dni), więc koszt dobowy to rząd 7 ms |

Katalog detaliczny M5 ma 18 towarów, więc pomiar obejmuje 2 000 × 18 sterowników zamiast
2 000 × 40 z §7.3. Budżet skaluje się liniowo z liczbą sterowników; rozmiar katalogu podnosi M6.

---

## Zmiany wpisane po M5b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5b —
podfaza nie jest tu przeprojektowywana.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`reprice` zmienia `Offer` przez `Market::set_price`, a nie przez zasób `Arena<Offer>`.** Arena ofert mieszka wewnątrz `Market` (`K-29`, korekta `V-3` w M5b), więc polityka cenowa musi wejść tam, gdzie siedzi stan: albo metodą na `Market`, albo systemem wołającym `Market` — nie zapytaniem ECS po ofertach | `PlaceProvider` nie dostaje `&World`, więc wszystko, czego dotyka decyzja zakupowa, jest osiągalne wyłącznie z uchwytu rynku. Zmiana ceny **nie brudzi indeksu** (indeks trzyma uchwyty, cena czyta się z areny na żywo) — to zostaje z M5a bez zmian |
| ★ | **Obserwacja konkurencji ma gotowy licznik: `Offer.price_rev`.** Porównanie z zapamiętaną rewizją zastępuje kopię cennika konkurenta | Pole powstało w M5a właśnie na to i nie ma jeszcze wołającego. Kryterium WP6 („reakcja nie wcześniej niż po 1 i nie później niż po 7 dniach") mierzy się wtedy na dacie zapamiętania rewizji, a nie na różnicy cen |
| ★ | **Poślizg ceny (§5.5) dostaje w M5c swojego pierwszego konsumenta.** W M5b mechanizm jest kompletny, ale bezczynny: `PlannedPurchase` zapamiętuje cenę z chwili decyzji, `price_slippage_bp` stoi w `data/economy/choice.ron`, a `MarketStats.slippage_rechecks` liczy zera, bo ceny się nie ruszają | Pierwszy `reprice` uruchamia całą tę ścieżkę naraz. Warto sprawdzić licznik po włączeniu polityk cenowych — jeśli zostanie zerem, znaczy to, że ceny zmieniają się wyłącznie w nocy i nikt nigdy nie zastaje innej ceny, niż widział przy planowaniu |
| ★ | **`take_cogs` w `kernel` przejmuje gotową funkcję `shop::take_units`, a nie pisze jej od nowa.** Gałąź zmiatania reszty (R5) jest już zaimplementowana i ma własny test (`zdjecie_calej_linii_zmiata_reszte_groszy`) | D20 wymaga „zera zmian zachowania" tam, gdzie coś realnie się przenosi — a tu się przenosi. Złoty plik ciągu hashy przed przenosinami i po nich obowiązuje |
| | **Wycena zapasu jest już rozdzielona na dwa miejsca: zaplecze i półkę.** `Market::inventory_value(site)` sumuje oba i to jest lewa strona niezmiennika P5 | `InventoryGoods` z bilansu musi się równać sumie **obu**, nie samego zaplecza. Towar wyłożony na półkę nie przestaje być majątkiem sklepu, a łatwo o to potknięcie, bo `Offer.available` patrzy tylko na półkę |
| | **Konto sklepu istnieje od M5b** (`AccountOwner::Firm`, otwierane przy stawianiu sklepu) i jest już obciążane zakupami u dostawcy zewnętrznego | `Ledger` z WP7 stoi więc obok istniejącego rachunku, a nie zamiast niego: dziennik księgowy jest widokiem na te same przepływy, nie drugim saldem |
