# M5 — Gospodarka detaliczna

Status: plan fazy. Nadrzędny: `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, LOD, wyjaśnialność).
Właściciel crate'ów: `sim/economy`, `tools/balansator`. Rozszerza: `sim/agents`, `sim/firms` (szkic sklepu), `engine/ui`.
Źródło: `PRD_Magnat.md` §6 (całość), §5.3–5.7, §7.1, §7.3, §14.1–14.3, §16.5, §17.5, §19, §20.1.

---

## 1. Cel fazy i artefakt końcowy

M5 to **pierwszy vertical slice** — pierwsza faza, w której istnieje pętla gracza. Po zakończeniu M5 da się:

1. Uruchomić miasto (z M1–M4) z działającą gospodarką detaliczną: mieszkańcy mają pieniądze, budżety
   gospodarstw domowych, potrzeby i chodzą do **konkretnych sklepów z konkretnym magazynem**,
   płacąc **konkretną cenę z konkretnej oferty**. Sklep, w którym nie ma chleba, nie sprzedaje chleba.
2. Otworzyć sklep jako gracz, wybrać asortyment, ustawić cenę ręcznie lub delegować politykę cenową,
   i **obserwować skutek**: kto przyszedł, skąd, dlaczego — i kto nie przyszedł i dlaczego nie.
3. Zobaczyć pełną księgowość tego sklepu: RZiS, bilans, przepływy pieniężne, wycenę zapasów.
4. Zobaczyć konkurencję AI reagującą na ruch gracza — z opóźnieniem 1–7 dni, nie natychmiast.
5. Uruchomić `tools/balansator` na N miastach i dostać raport rozkładu cen, inflacji, marż, bankructw
   oraz **werdykt bramek CI** (stabilność, reaktywność szoku, zachowanie pieniądza, determinizm).

Artefakt końcowy (demo „do pokazania"): headless przebieg 2 lat gry × 32 seedy z zielonym raportem
balansatora **plus** sesja w GUI: gracz otwiera sklep osiedlowy przy ulicy X, podnosi cenę mleka o 15%,
po 3 dniach widzi w panelu spadek liczby klientów i listę utraconych sprzedaży z uzasadnieniem
„cena o 12% wyższa niż w *Dobry Koszyk*, 700 m dalej".

Wprost: **nie istnieje globalna cena rynkowa** (§6.1). Wszystko, co UI pokazuje jako „cenę mleka",
jest agregatem po ofertach — i musi być tak zaimplementowane, nie zasymulowane skrótem.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zakres w M5 | PRD |
|---|---|---|
| Oferta jako jedyny nośnik ceny | `Offer` jako encja ECS, indeks przestrzenny per kategoria, agregacja do UI | §6.1, §17.5 |
| Rynek detaliczny B2C | dopasowanie kupujący↔oferta, rozliczenie, kolejność deterministyczna | §6.2 pkt 1 |
| Decyzja zakupowa | funkcja użyteczności, softmax, próg odłożenia zakupu, wagi z osobowości i statusu | §6.4, §5.4 |
| Polityki cenowe AI | marża, korekta wg magazynu, obserwacja konkurencji z opóźnieniem 1–7 dni, eksperymenty cenowe, przeceny psującego się | §6.3 |
| Polityki delegowane gracza | te same typy co AI, ustawiane z UI | §6.3, §14.6 (podstawa) |
| Sklep | magazyn zaplecza, półka, asortyment, rotacja, braki, pojemność z budynku | §7.1, §7.3 |
| Pieniądz i konta | `Account`, podwójny zapis, jeden punkt przelewu, ewidencja podaży pieniądza | §6.5 |
| Budżety GD | dochody, wydatki stałe, koperty wydatków zmiennych, oszczędności/dług | §5.2 |
| Banki i kredyt (podstawowy) | depozyty, kredyt konsumpcyjny, kredyt obrotowy, ocena zdolności, harmonogram spłat | §6.5 |
| Stopa bazowa i inflacja | CPI z koszyka miejskiego, bank centralny jako abstrakcja, inflacja **emergentna** | §6.5 |
| Księgowość sklepu | plan kont, dziennik, RZiS, bilans, przepływy, wycena zapasów | §6.9 |
| Panel sklepu w UI | półki, klienci („skąd/kto/dlaczego"), utracone sprzedaże, konkurencja w zasięgu | §14.3, §14.1 |
| Balansator | N miast, rozkłady cen, wykrywanie spirali, bramki CI | §16.5, §20.1, §20.4 |

### Nie wchodzi (i kto to robi)

| Pominięte | Faza | Co M5 zostawia w zamian |
|---|---|---|
| Rynek B2B (spot, kontrakty), import | M6 | `trait Wholesale` + `ExternalSupplier` — jawny punkt wymiany (§5.7 tego dokumentu) |
| Partie towaru (`BatchId`), receptury, produkcja | M6 | `StockLine` z jedną datą ważności; wycena średnią ważoną zamiast FIFO |
| Zaopatrzenie sklepu od realnego dostawcy | M6 | zakup u „zewnętrznego dostawcy" o cenie z `data/goods/` |
| Rynek pracy, pensje emergentne | M7 | `HouseholdBudget.income_monthly` z abstrakcyjnego pracodawcy (konto `RestOfWorld`) |
| Firmy AI: zakładanie, bankructwo, osobowości pełne | M7 | sklepy AI zasiedlone przez generator M2; osobowość cenowa = 4 parametry z danych |
| Podatki, VAT, akcyza | M8 | `trait TaxEngine` + impl `NoTax`; pole `Transaction.tax` istnieje i jest zawsze 0 |
| Giełda, ubezpieczenia, obligacje, leasing, faktoring | M10 / M7 | brak |
| Marka, reklama, afinitet do marki | M10 | człon `w_marka · afinitet` istnieje w funkcji użyteczności, `afinitet ≡ 0.0` |
| Plotka jako kanał informacji | M10 | znajomość sklepu = odwiedzony ∪ w zasięgu domu/pracy/trasy (§5.7 uproszczone) |
| Pełne panele gracza, edytor reguł | M9 | panel sklepu + wybór polityki z listy (bez edytora reguł) |
| Nieruchomości, czynsze emergentne | M7/M10 | czynsz sklepu = stała z parcelą, wydatek stały GD = stała |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M5 |
|---|---|
| §6.1 | `Offer` jako jedyny nośnik ceny; brak zmiennej „cena rynkowa" w kodzie — test statyczny |
| §6.2 pkt 1 | rynek detaliczny w całości |
| §6.3 | polityki cenowe AI i gracza, bez surowców pierwotnych (M6) |
| §6.4 | funkcja użyteczności, softmax, próg, elastyczność jako własność emergentna |
| §6.5 | pieniądz, banki, depozyty, kredyt konsumpcyjny i obrotowy, stopa bazowa, inflacja emergentna. Bez giełdy i ubezpieczeń |
| §6.9 | księgowość sklepu w całości (bez należności z terminami — te mają sens dopiero w B2B, M6) |
| §5.2 | budżet GD: dochody − wydatki stałe − zmienne → oszczędności lub dług |
| §5.3 | koszyk dóbr per potrzeba i substytuty niższego rzędu (mapowanie z `data/goods/`) |
| §5.4 | status jako źródło wag `w_status`, `w_jakość`, `w_cena` |
| §5.5 | zadanie „zakupy" z planera dnia wywołuje `purchase_decision` |
| §5.7 | znajomość sklepu jako filtr kandydatów (bez plotki) |
| §7.1, §7.3 | sklep = zakład firmy: budynek, pojemność magazynu i półki, media jako wydatek stały |
| §14.1 | `DecisionReason` dla każdej decyzji zakupowej, cenowej i kredytowej |
| §14.3 | panel „Sklep" |
| §16.5 | balansator |
| §17.5 | indeks przestrzenny ofert, 3–15 kandydatów, cache agregatów per dzielnica |
| §20.1 | bramki balansatora (sekcja 7 tego dokumentu) |
| §20.4 | balansator w CI **od tej fazy** jako główna mitygacja ryzyka spirali |

---

## 4. Pakiety robocze i podfazy

Kolejność ma jedną twardą zasadę: **pieniądz przed wszystkim innym**. Test zachowania pieniądza musi
być zielony zanim powstanie pierwsza transakcja detaliczna, bo później nie da się go już wprowadzić
bez przepisywania.

Faza jest rozbita na **5 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M5a — Pieniądz i oferta** | WP1, WP2 | 5.1, 5.2 | Test zachowania pieniądza zielony **zanim** powstanie pierwsza transakcja detaliczna — później nie da się go wprowadzić bez przepisywania. | `M5a-pieniadz-i-oferta.md` |
| **M5b — Sklep i zakup** | WP3, WP4, WP5 | 5.3, 5.4, 5.5, 5.7 | Mieszkaniec wychodzi po chleb, wybiera ofertę i wraca; pieniądz i sztuki zgadzają się po obu stronach. | `M5b-sklep-i-zakup.md` |
| **M5c — Ceny i księgowość** | WP6, WP7, WP11 | 5.6, 5.8 | Sklep AI podnosi cenę przy niedoborze i obniża przy zaleganiu; rachunek wyników i bilans domykają się co do grosza. | `M5c-ceny-i-ksiegowosc.md` |
| **M5d — Budżety, banki, inflacja** | WP8, WP9, WP10 | 5.9, 5.10 | Inflacja emergentna: koszyk CPI liczony z transakcji świata, stopa bazowa reagująca na niego bez ręcznego sterowania. | `M5d-budzety-banki-inflacja.md` |
| **M5e — Panel, balansator, domknięcie** | WP12, WP13, WP14 | 5.11, 5.12 | Pełny artefakt fazy z §1 dokumentu fazy: otwórz sklep, ustal ceny, obserwuj klientów; balansator w CI. | `M5e-panel-balansator-domkniecie.md` |

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | Pieniądz, konta, transakcje | `M5a-pieniadz-i-oferta.md` |
| 5.2 | Oferta i indeks przestrzenny (§6.1, §17.5) | `M5a-pieniadz-i-oferta.md` |
| 5.3 | Sklep: magazyn, półka, asortyment | `M5b-sklep-i-zakup.md` |
| 5.4 | Funkcja użyteczności zakupu (§6.4) — pełna specyfikacja | `M5b-sklep-i-zakup.md` |
| 5.5 | Rozliczanie transakcji — wyścig o ostatnią sztukę | `M5b-sklep-i-zakup.md` |
| 5.6 | Polityki cenowe (§6.3) | `M5c-ceny-i-ksiegowosc.md` |
| 5.7 | Punkt wymiany M5 ↔ M6: zewnętrzny dostawca | `M5b-sklep-i-zakup.md` |
| 5.8 | Księgowość sklepu (§6.9) | `M5c-ceny-i-ksiegowosc.md` |
| 5.9 | Budżet gospodarstwa domowego (§5.2) | `M5d-budzety-banki-inflacja.md` |
| 5.10 | Banki, kredyt, stopa bazowa (§6.5) | `M5d-budzety-banki-inflacja.md` |
| 5.11 | Systemy ECS i częstotliwości | `M5e-panel-balansator-domkniecie.md` |
| 5.12 | Panel sklepu w UI (§14.3) | `M5e-panel-balansator-domkniecie.md` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam (`sim/economy`, `tools/balansator`)

| Kontrakt | Odbiorcy |
|---|---|
| `Offer`, `OfferId`, `OfferIndex`, `query_offers`, `PriceStats`, `PriceBasis` | M6 (B2B na tym samym mechanizmie, `NetB2B`), M7, M9, M10, `engine/ui` |
| `Books`, `Account`, `AccountId`, `AccountOwner` (z wariantem `City`), `Books::transfer` | wszystkie fazy dotykające pieniądza; `City` na wniosek M8 |
| `MoneySupplyLedger` z kanałem `external_capital_in/out` + `inject_external_capital`, `repatriate_external_capital` | **M7** — jedyny punkt emisji pieniądza spoza `RestOfWorld` (kapitał sieci zewnętrznej) |
| `Transaction`, `TxKind` (z `TaxPayment`, `PublicSpend`, `ExternalCapital`), `TxJournal`; `Transaction.tax` = **wyłącznie VAT** | M8 (pozostałe daniny przez własny `ChargeRegistry`, bez migracji dziennika), M7, M9 (kronika), M10 |
| `LostSaleTracking`, `LostSaleHistogram`, `LostSale` | **M9** (karta inspekcji „dlaczego Anna nie kupiła u mnie", §14.1) |
| `Ledger`, `LedgerAccount`, `JournalEntry`, `post`, `income_statement`, `balance_sheet`, `cash_flow` | M6, M7 (księgowość zakładu produkcyjnego), M9 (panel Finanse), M10 (wycena giełdowa) |
| `PricePolicy`, `PriceController`, `reprice`, `CompetitorSnapshot`, `ObservedElasticity` | M7 (firmy AI), M9 (delegowanie), M10 |
| `ShopInventory`, `Shelf`, `StockLine`, `ReorderPolicy`, `AssortmentPolicy`, `take_cogs` | M6 (partie zastąpią `StockLine.expires`), M7 |
| `utility_of_offer`, `weights_for`, `gather_candidates`, `choose_offer`, `purchase_threshold` | M3 (planer dnia woła to dla zadania „zakupy"), M10 (dopisze człon marki) |
| `sim/economy::labor`: oferty pracy (`CategoryId::JobRole`), indeks, dopasowanie po stronie szukającego, `Vec<Application>`, `LaborMarketStats` | **M7** (`sim/firms::labor_policy` domyka stronę pracodawcy — granica niżej) |
| **`sim/economy::kernel`**: `next_price`, `take_cogs`, `ledger_post` (M5); sloty `wage_bid` (M7), `throughput` (M6) | **M10** (model makro woła ten sam kod co mezo — warunek K-5), M6, M7 |
| `HouseholdBudget`, `plan_budget`, `budget_ref_for_need`, `Envelope` | M3, M7 (dochód z pensji), M8 (PIT), M9 |
| `Loan`, `LoanKind`, `build_schedule`, `assess_credit`, `CreditDecision`, `BaseRate`, `cpi` | M6 (kredyt obrotowy pod zapasy), M7 (inwestycyjny), M10 (hipoteczny, giełda) |
| `trait Wholesale`, `ExternalSupplier`, `PurchaseQuote`, `Delivery` | **M6 podmienia implementację, nie interfejs** |
| `trait TaxEngine` + `NoTax`, pole `Transaction.tax`, konto `TaxExpense` | **M8** |
| `DecisionReason::{Purchase, OfferRejected, PurchaseDeferred, Repricing, CreditDecision}` | `engine/devtools`, `engine/ui`, M9 |
| `StreamId` 180–199 (K-4): `PurchaseNoise`, `PurchaseChoice`, `PriceExperiment`, `CompetitorDelay`, `ExternalPriceDrift`, `CreditScoringJitter`; 186–199 wolne dla M7 | M0 (właściciel enuma), M7 |
| `ShopPanelSnapshot` i podtypy (w tym `LostSalesView`) | `engine/ui` (M3 właściciel szkieletu, M9 pełne panele) |
| `BalansatorMetrics` (schemat JSON) + bramki CI | M6–M12 (każda faza dopisuje metryki, nie zmienia istniejących pól) |

### Moduł `labor` — granica z `sim/firms` (D19 od M7)

Przyjmuję przeniesienie: `sim/economy::labor` (oferty pracy, indeks, dopasowanie, `LaborMarketStats`)
należy do mnie, `sim/firms::labor_policy` (scoring kandydata, eskalacja stawki) do M7.
Rynek pracy jest ofertą jak każda inna (§6.6: „każde stanowisko to oferta"), więc M5 **nie buduje
drugiego mechanizmu dopasowania** — `gather_candidates`/`choose_offer` są sparametryzowane kategorią
i zestawem wag, a M7 dokłada wariant `CategoryId::JobRole` i własne `UtilityWeights`
(pensja netto − koszt dojazdu, dopasowanie umiejętności, reputacja pracodawcy — §5.6).

**Granica, jednym zdaniem:** `sim/economy::labor` odpowiada za **stronę szukającego** — kto widzi
jaką ofertę i którą wybiera (mieszkaniec → oferta pracy, ten sam kod co mieszkaniec → oferta sklepu),
a `sim/firms::labor_policy` za **stronę ogłaszającego** — ile oferta ma obiecywać i którego
z aplikujących przyjąć.

Szwu nie ma, bo granica biegnie po danych, nie po module: ja wytwarzam **uporządkowany zbiór
aplikacji**, M7 zwraca **decyzję**. Rozpisane, żeby nie było wątpliwości przy pierwszym konflikcie:

| Należy do mnie (`sim/economy::labor`) | Należy do M7 (`sim/firms::labor_policy`) |
|---|---|
| `Offer` z `CategoryId::JobRole` jako nośnik stawki i warunków | **jaka stawka** trafia do tej oferty (wywołuje `kernel::wage_bid`) |
| indeks przestrzenny ofert pracy, zasięg dojazdu, filtr znajomości | — |
| użyteczność i wybór po stronie mieszkańca, próg zmiany pracy (§5.6) | — |
| uporządkowany `Vec<Application>` przekazany firmie | **scoring i wybór** spośród tych aplikacji |
| `LaborMarketStats` (rozkład pensji per zawód, wakaty, bezrobocie, rotacja) do balansatora | kiedy eskalować stawkę i o ile (polityka, nie mechanika) |
| `TxKind::Wage`, konto `WagePayable`, `StreamId` 186–199 | — |

Reguła rozstrzygająca spory graniczne: **jeśli coś zależy od osobowości konkretnej firmy — to M7;
jeśli jest takie samo dla każdego pracodawcy — to ja.** To ta sama linia, która w detalu oddziela
`reprice` (mój mechanizm) od `PricePolicy` (parametry właściciela).

### Podmoduł `sim/economy::kernel` (D20 od M10 przez M7) — zgoda

**Zgoda.** Rekomendacja jest trafna, a powód poważniejszy niż koszt refaktoru: dwa modele ekonomii
zawsze się rozjeżdżają, a wtedy K-5 (odchylenie agregatów ≤ 0,5%, brak dryfu) staje się nie do
utrzymania, bo dryf pochodziłby z **rozjazdu implementacji**, nie z agregacji — czyli mierzylibyśmy
własny błąd zamiast błędu przybliżenia. Model makro M10 musi wołać ten sam kod co mezo.

Jedna poprawka do sposobu: **nie wyciągam `kernel` po fakcie — piszę te funkcje od razu w `kernel`.**
Ekstrakcja post factum to koszt bez powodu, skoro w M5 dopiero powstają. „Zero zmian zachowania"
zostaje wymogiem tam, gdzie realnie coś przenoszę (`take_cogs`, walidacja zapisu księgowego).

```rust
/// Rdzeń liczbowy sim/economy. ZASADA: nie zna LOD, nie zna encji, nie alokuje.
/// Dostaje liczby, zwraca liczby. Brak &World, brak Entity, brak I/O, brak stanu globalnego.
/// Identyfikatory wchodzą wyłącznie jako indeksy katalogu danych (GoodId, JobRoleId),
/// nigdy jako id rozwiązywane przez świat (SiteId, FirmId, CitizenId).
pub mod kernel {
    pub fn next_price(i: PriceInput) -> Money;                 // M5 — rdzeń reprice (5.6)
    pub fn take_cogs(line: StockValue, sold: Qty) -> (Money, StockValue);  // M5 — wycena zapasu (5.8)
    pub fn ledger_post(lines: &[(LedgerAccount, Money)],
                       acc: &mut [Money; LEDGER_ACCOUNT_COUNT]) -> Result<(), LedgerError>;  // M5
    pub fn wage_bid(i: WageInput) -> Money;                    // slot — wypełnia M7
    pub fn throughput(i: ThroughputInput) -> Qty;              // slot — wypełnia M6
}
```

Wszystkie `*Input` są `Copy`-owymi strukturami POD; rozwiązanie encji na liczby robi **wołający**,
nie rdzeń. `ledger_post` pisze do bufora wołającego, dlatego nie alokuje mimo zmiennej liczby linii.

Trzy konsekwencje, dla których zgadzam się chętnie, a nie tylko bez sprzeciwu:

1. **Testy własnościowe przenoszą się na poziom, na którym są tanie.** P5 (brak dryfu wyceny),
   P6 (harmonogram sumuje się do kapitału), P7 (podział kwoty) i P4 (zbilansowanie zapisu) dają się
   uruchomić `proptest`-em **bez budowania świata** — milion przypadków zamiast stu scenariuszy.
2. **Zakaz floatów w pieniądzu (dok. 00 §2) staje się sprawdzalny mechanicznie.** Sygnatury
   `next_price`, `take_cogs` i `ledger_post` są w całości całkowitoliczbowe, więc test statyczny
   „w `kernel` nie występuje `f32`/`f64` w ścieżce pieniężnej" wystarcza zamiast przeglądu kodu.
3. **K-5 zostaje testowalne wprost:** makro i mezo wołają tę samą funkcję, więc różnica agregatów
   pochodzi wyłącznie z agregacji wejść. To jest test dla M10, ale warunek jego istnienia
   dostarczam ja.

Bramka „zero zmian zachowania" (wymóg, nie intencja): przed przeniesieniem `take_cogs` i walidacji
zapisu do `kernel` zapisuję złoty plik ciągu hashy z testu determinizmu (7.2); po przeniesieniu ciąg
musi być identyczny **bit w bit**. Refaktor, który zmienia choć jeden hash, jest z definicji
nieudany i wraca.

### Konsumuję

| Od kogo | Co | Uwaga |
|---|---|---|
| M0 | `Money`, `Qty`, `Tick`, `SimMinute`, `Q`, id-ki, RNG ze strumieniami, ECS, scheduler, job system, `tools/headless` | `tools/headless` musi dać się użyć **jako biblioteka** przez balansator — sekcja 9 pkt 10 |
| M0 | **`core::det_math`** (`ln`, `ln1p`, `log2`, `exp`, `exp_m1`, `exp2`, `pow`, `sqrt`, `softmax`), ≤ 2 ULP, lint zakazujący libm w symulacji | K-6 — **jedyne** źródło funkcji przestępnych w `sim/economy`; `softmax` sumuje po indeksach, bez redukcji parami |
| M0 | `DecisionReason` jako jeden centralny enum bez `#[non_exhaustive]` (K-12); `UtilityKind` i wspólne słowniki w `engine/core` (K-8) | M5 dopisuje warianty, nie tworzy własnego enuma |
| M0 | kalendarz 360 dni, 12 × 30 (K-1) | miesiąc odsetkowy i okres księgowy == 30 dni; brak konwencji ACT/365 |
| M8 | `CityTaxEngine` jako implementacja `TaxEngine`; `ChargeRegistry` dla danin innych niż VAT | zamyka moją decyzję otwartą nr 8 — bez migracji dziennika |
| M1/M2 | `engine/spatial` (grid, zapytanie promieniowe), `DistrictId`, `ParcelId`, pozycje budynków, pojemność budynku (m²→sloty półki) | |
| M3 | `Personality` (wrażliwość cenowa, lojalność, oszczędność, otwartość, ambicja, ryzyko), `Needs` + tempo spadku, `ExperienceMemory`, `KnownPlaces`, `HouseholdId` + typ GD, kolejka zdarzeń DES, zadanie „zakupy" z planera dnia | `KnownPlaces` — czyja własność? sekcja 9 pkt 4 |
| M4 | `travel_cost(from, to, mode, at) -> (SimMinute, Money)` | **twarda zależność** członu `g`; potrzebny jest też koszt pieniężny, nie tylko czas — sekcja 9 pkt 3 |
| M6 | zastąpi `ExternalSupplier` implementacją B2B | |
| M7 | zastąpi `income_monthly` pensją emergentną; uczyni bank pełną firmą | |
| M8 | dostarczy `TaxEngine` z VAT | |
| M10 | dostarczy `brand_affinity` (w M5 ≡ 0) i plotkę rozszerzającą `KnownPlaces` | |

---

## 7. Testy i kryteria akceptacji

### 7.1 Testy własnościowe (obowiązkowe, dok. 00 §6)

| # | Własność | Tolerancja | Jak |
|---|---|---|---|
| P1 | `Σ sald == endowment + credit_created − credit_repaid + external_capital_in − external_capital_out` | **0 groszy** | co tick w debug, co 1000 ticków w release; `RestOfWorld` jest zwykłym kontem, więc handel zewnętrzny nic nie psuje |
| P1b | Kanał kapitału zewnętrznego (M7) nie łamie P1 | **0 groszy** | **test pisany w M5, mimo że kanał jest w M5 nieużywany**: scenariusz proptest wplata `inject_external_capital` / `repatriate_external_capital` w losowy strumień przelewów i kredytów. Bez tego test pęknie dopiero w M7 — daleko od przyczyny (zgłoszenie M7) |
| P2 | Żaden sklep nie sprzedaje towaru, którego nie ma | 0 sztuk | po każdej fazie rozliczenia: `Σ sprzedanych z linii ≤ stan półki przed fazą`; test wyścigu: 10 tys. agentów, 10 sztuk → dokładnie 10 transakcji |
| P3 | Brak ujemnych stanów | — | `StockLine.qty ≥ 0`, `Shelf qty ≥ 0`, `Offer.available ≥ 0`, `Loan.outstanding ≥ 0`, saldo ≥ `−overdraft_limit` |
| P4 | Każdy zapis w dzienniku jest zbilansowany | 0 groszy | `Σ lines == 0` wymuszone w `post` |
| P5 | `InventoryGoods` w bilansie == `Σ StockLine.cost_total` | 0 groszy | po roku symulacji; chroni gałąź „zmiatania reszty" w `take_cogs` |
| P6 | Harmonogram kredytu sumuje się do kapitału | 0 groszy | `Σ installment.principal == principal` dla 10⁴ losowych (kwota, stopa, okres) |
| P7 | Podział kwoty między N stron sumuje się do oryginału | 0 groszy | test kopert budżetowych i rozbicia raty |
| P8 | 100% decyzji ma `DecisionReason` | 100% | test przechodzi po dzienniku decyzji z 30 dni symulacji |

| P9 | `kernel` nie ma floatów w ścieżce pieniężnej | — | test statyczny: w sygnaturach i ciałach `next_price`, `take_cogs`, `ledger_post` nie występuje `f32`/`f64`. Możliwy tylko dlatego, że rdzeń jest wydzielony (D20) |

Narzędzie: `proptest` dla P1, P1b, P3, P6, P7; scenariusze headless dla P2, P8.

**P4, P5, P6, P7 uruchamiane są na `kernel`, nie na świecie** — czyste sygnatury pozwalają
`proptest`-owi wygenerować milion przypadków zamiast stu scenariuszy. To jest główny zysk z D20
po mojej stronie i powód, dla którego wydzielenie rdzenia jest częścią WP1/WP6/WP7, a nie
osobnym refaktorem na końcu.

### 7.2 Determinizm (dok. 00 §3)

- Dopisanie wszystkich komponentów `sim/economy` do funkcji haszującej stan ECS — **warunek DoD fazy**.
- Dwa przebiegi tego samego seeda (365 dni, 50 tys. mieszkańców) → identyczny ciąg hashy co 1000 ticków.
- Przebieg jednowątkowy vs. `RAYON_NUM_THREADS=1,2,8,16` → identyczny ciąg hashy.
- Przebieg na Windows vs. Linux → identyczny ciąg hashy (**to jest test, który wyłapie obejście
  `core::det_math`** — patrz 5.4/K-6). Uzupełniająco lint CI z M0 zakazujący libm w `sim/*`.
- Mikro vs. mezo: ten sam scenariusz → identyczne salda i identyczna liczba transakcji (dok. 00 §4).

### 7.3 Wydajność (criterion, cel dla miasta 150 tys.)

| Ścieżka | Cel |
|---|---|
| `query_offers` (5 tys. ofert, promień 3 km) | < 20 µs |
| `utility_of_offer` | < 120 ns na kandydata |
| `purchase_decision` — cała doba (≈450 tys. decyzji × ≤15 kandydatów) | < 900 ms CPU łącznie, ≤ 3 ms na tick minutowy w szczycie |
| `settle_transactions` (tick szczytowy, ≈2 tys. intencji) | < 1,5 ms (sekwencyjny — to jest górna granica, patrz ryzyko R3) |
| `reprice` dla 2 tys. sklepów × 40 towarów | < 40 ms raz na dobę |
| Pamięć: `Offer` | ≤ 48 B; `ShopLostSales` ≤ 256 wpisów × 16 B na sklep |

Zero alokacji w `gather_candidates` i `utility_of_offer` (bufory wielokrotnego użytku) — weryfikowane
licznikiem alokacji w teście.

### 7.4 Balansator — kryteria akceptacji z PRD §20.1 przetłumaczone na testy

```
balansator run  --seeds 32 --days 730 --scenario base --out runs/
balansator run  --seeds 32 --days 730 --scenario supply-shock
balansator run  --seeds 32 --days 730 --scenario player-price-war
balansator gate runs/ --profile ci|nightly
```

Metryki zbierane per przebieg per dzień: rozkład cen per `GoodId` (p10/p50/p90/min/max, liczba ofert),
CPI i inflacja m/m oraz r/r, mediana marży sklepów, liczba sklepów i bankructw, `deferral_rate`
(udział decyzji zakończonych odłożeniem), `stockout_rate`, HHI koncentracji per kategoria,
podaż pieniądza, suma kredytów, stopa bazowa, rozkład zaspokojenia potrzeb.

| Bramka | Źródło PRD | Warunek (fail = czerwone CI) |
|---|---|---|
| **G1 Stabilność cen** | §20.1 „inflacja roczna −5…+15% w 95% seedów" | w ≥ 95% seedów inflacja r/r ∈ ⟨−5%, +15%⟩ w każdym miesiącu od 12. miesiąca (rozbieg wyłączony) |
| **G2 Brak hiperinflacji** | §20.1, §16.5 | żaden seed: brak miesiąca z inflacją m/m > 10%; brak `CPI_t / CPI_{t−90d} > 1,5` |
| **G3 Brak spirali deflacji** | §16.5 | żaden seed: brak 6 kolejnych miesięcy ze spadkiem CPI; mediana marży sklepów nie schodzi poniżej `min_margin_bp` w > 5% dni |
| **G4 Reaktywność szoku podaży** | §20.1 „widoczny w 2–7 dni, wygaszony w 2–8 tygodni" | scenariusz `supply-shock` (cena hurtowa wskazanego towaru +80% w dniu 180): mediana ceny detalicznej pokrywa ≥ 50% szoku w przedziale **2–7 dni** (`t_response`) i stabilizuje się w ±10% nowego poziomu w przedziale **14–56 dni** (`t_settle`). Zbyt szybko = też fail (rynek bez tarcia, sprzeczne z opóźnieniem obserwacji 1–7 dni) |
| **G5 Rynek nie wymiera** | §16.5 | ≥ 1 aktywna oferta w każdej kategorii w 100% dni; mediana `deferral_rate` < 25%; `stockout_rate` < 15% |
| **G6 Brak monopolizacji z kalibracji** | §6.4 (softmax, nie argmax) | HHI per kategoria per dzielnica < 0,6 w medianie seedów — chroni przed zjechaniem temperatury softmaxu do zera |
| **G7 Zachowanie pieniądza** | dok. 00 §6 | każdy przebieg kończy P1 zielono, 0 groszy |
| **G8 Determinizm** | dok. 00 §3 | dwa przebiegi seeda 0 → identyczny ciąg hashy |
| **G9 Wyjaśnialność** | §20.1 „100% decyzji" | 0 decyzji bez `DecisionReason` w próbce 10⁵ |

Profile: **ci** (PR) = 8 seedów × 365 dni, bramki G1–G3, G5, G7–G9, budżet czasu < 10 min.
**nightly** = 32 seedy × 730 dni, wszystkie bramki + scenariusze szokowe + raport Markdown z wykresami
(analiza w Pythonie, zgodnie z §20.4 — Python zostaje w narzędziach).

Test bramki (meta-test, uruchamiany raz): celowe usunięcie dolnego ogranicznika ceny w `reprice`
musi zaczerwienić G3. Bramka, która nigdy nie świeci na czerwono, nie jest bramką.

### 7.5 Definition of Done fazy

Wszystko z 7.1–7.4 zielone, plus: komponenty `sim/economy` w funkcji haszującej stanu,
`clippy -D warnings`, panel sklepu odpowiada na „dlaczego Anna nie kupiła u mnie" dla ≥ 95% przypadków,
balansator wpięty w CI (to jest osobne kryterium — §20.4 wskazuje go jako **główną mitygację ryzyka**).

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Prawdopodobieństwo / skutek | Mitygacja |
|---|---|---|---|
| R1 | **Spirala cenowa** (deflacja przy nadmiarze zapasów lub hiperinflacja przy pętli kredytowej) | wysokie / krytyczne | dolny i górny ogranicznik marży w `reprice` (5.6); bramki G1–G3 w CI **od pierwszego dnia fazy**, nie na końcu; balansator budowany w WP13, ale metryki zbierane od WP5 |
| R2 | **Determinizm zmiennoprzecinkowy** — funkcja użyteczności używa `ln` i `exp` | **zamknięte (K-6)** / było krytyczne | `core::det_math` z M0 (≤ 2 ULP, `softmax` sumujący po indeksach) + lint CI zakazujący libm w symulacji + test hashy Windows vs. Linux. Pozostała resztka ryzyka: obejście `det_math` przez nieuwagę — wyłapuje je lint i test międzyplatformowy |
| R3 | **`settle_transactions` jest sekwencyjny** — przy dużym mieście może stać się wąskim gardłem | średnie / średnie | sekwencyjność dotyczy tylko intencji z bieżącego ticku (≈2 tys.), nie populacji. Ścieżka podniesienia: partycjonowanie po `SiteId` (sklepy są rozłączne, więc równoległość jest legalna) — wdrożyć dopiero gdy benchmark z 7.3 czerwieni się przy 400 tys. (M12) |
| R4 | **Degeneracja wyboru do monopolu** — wszyscy do najtańszego, konkurencja pada, potem monopolista windzi ceny | średnie / wysokie | softmax zamiast argmax (§6.4), szum, waga odległości, filtr znajomości (§5.7), premia za nowość; bramka G6 (HHI) |
| R5 | **Dryf zaokrągleń w wycenie zapasów** — bilans przestaje się zamykać po tysiącach transakcji | wysokie (jeśli nieprzewidziane) / wysokie | gałąź „zmiatania reszty" w `take_cogs` (5.8) + test własnościowy P5 |
| R6 | **Zależność od M4** — człon `g` wymaga kosztu przejazdu w czasie **i** pieniądzu; jeśli M4 daje tylko czas, człon traci sens | średnie / wysokie | zaślepka `travel_cost` z tabeli czasów dzielnica↔dzielnica (§17.6) pozwala rozwijać WP4 równolegle; rozbieżność zapisana w sekcji 9 pkt 3 |
| R7 | **Koszt gorącej ścieżki** — 450 tys. decyzji/dobę × 15 kandydatów × 8 członów | średnie / średnie | bufory wielokrotnego użytku, obcięcie K=15 z wstępnym rankingiem całkowitoliczbowym, cache agregatów per dzielnica (§17.5), benchmark jako bramka |
| R8 | **Zmiana wyceny zapasów WAC → FIFO w M6** zmienia historyczne COGS i psuje porównywalność raportów | średnie / niskie | jawne udokumentowanie w 5.7 pkt 2; decyzja o sposobie migracji — sekcja 9 pkt 5 |
| R9 | **Brak pętli dochodowej** — w M5 dochód GD jest egzogeniczny (RestOfWorld), więc balansator może „potwierdzać" stabilność, której w M7 zabraknie | wysokie / średnie | scenariusz balansatora `income-shock` (−20% dochodów) już w M5; jawne oznaczenie wyników M5 jako **warunkowych** do czasu M7; ponowne przejście bramek jest kryterium akceptacji M7, nie M5 |
| R10 | **Przeciążenie zakresu fazy** — M5 to 14 pakietów i pierwszy vertical slice | wysokie / średnie | twarda kolejność: WP1 (pieniądz) → WP2–WP5 (pętla) → reszta. Po WP5 istnieje grywalne demo; WP6–WP12 podnoszą jakość, WP13 zabezpiecza. Jeśli faza się przeciąga, WP11 i część WP6 (eksperymenty cenowe) są kandydatami do przesunięcia do M7 |

---

## 9. Decyzje otwarte

Runda uzgodnień międzyfazowych zamknęła punkty **6, 8 i 12** oraz wniosła rozstrzygnięcia wiążące
K-1, K-4, K-7, K-8, K-12 (wprowadzone do sekcji 5 i 6). Poniższe punkty pozostają otwarte
i wymagają rozstrzygnięcia **przed startem fazy**; przy każdym jest propozycja M5, którą przyjmuję
jako domyślną, jeśli nikt nie zgłosi sprzeciwu.

Runda zamykająca M7 dołożyła dwa rozstrzygnięcia, oba **przyjęte** (sekcja 6):
**D19** — moduł `labor` wchodzi do `sim/economy`, granica z `sim/firms::labor_policy` biegnie
między stroną szukającego (moja) a stroną ogłaszającego (M7); reguła rozstrzygająca spory:
zależne od osobowości firmy → M7, jednakowe dla każdego pracodawcy → ja.
**D20** — zgoda na `sim/economy::kernel`, z jedną poprawką: funkcje powstają **od razu** w rdzeniu,
nie są wyciągane po fakcie; „zero zmian zachowania" obowiązuje tam, gdzie realnie coś przenoszę,
i jest egzekwowane złotym plikiem hashy (bit w bit).

Zmiany przyjęte we wcześniejszej rundzie bez zastrzeżeń: `AccountOwner::City`, `TxKind::{TaxPayment,
PublicSpend}`, rozbicie pasywów na `TradePayable`/`TaxPayable`/`WagePayable` (M8); kanał
`external_capital_in/out` w `MoneySupplyLedger` wraz z testem P1b (M7); trójstopniowy
`LostSaleTracking` (M9); miejsce na `LaborMarketStats` przez reużycie istniejącego mechanizmu
dopasowania zamiast drugiego (M7).

1. **`Offer` jako encja ECS czy komponent półki?**
   Dok. 00 §2 definiuje `OfferId(Entity)`, a §17.2 PRD wymienia ofertę wśród encji — więc encja.
   Koszt: 1 encja na (sklep × towar), przy 2 tys. sklepów × 40 towarów = 80 tys. encji.
   *Propozycja M5: encja. Do potwierdzenia z M0 (budżet encji) i M6 (oferty B2B mnożą tę liczbę).*

2. **Skąd bierze się dochód GD w M5?**
   M5 potrzebuje dochodu, żeby budżet miał sens; pensje emergentne to M7.
   *Propozycja M5: `HouseholdBudget.income_monthly` wypłacane z konta `RestOfWorld`, kwota
   z profilu zawodowego mieszkańca (M3). Kto jest właścicielem tej logiki — M3 czy M5? Propozycja: M5,
   bo dotyczy pieniądza; M7 podmienia źródło bez zmiany struktury.* **Do uzgodnienia z M3 i M7.**

3. **Sygnatura `travel_cost` z M4.**
   Człon `g` wymaga `(SimMinute, Money)` — czasu **i** kosztu pieniężnego (paliwo, bilet, parking).
   Jeśli M4 zwraca sam czas, M5 musiałby duplikować model kosztu pojazdu.
   *Propozycja M5: `fn travel_cost(from, to, mode, at) -> TravelCost { time: SimMinute, money: Money }`
   jako kontrakt M4.* **Do uzgodnienia z M4 — to najtwardsza zależność zewnętrzna tej fazy.**

4. **Kto jest właścicielem `KnownPlaces` (znajomość sklepów, §5.7)?**
   *Propozycja M5: M3 (to pamięć agenta, obok `ExperienceMemory`); M5 tylko czyta i zgłasza
   zdarzenie „odwiedzono sklep". M10 dopisze plotkę.* **Do uzgodnienia z M3 i M10.**

5. **Migracja wyceny zapasów WAC (M5) → FIFO per partia (M6).**
   Warianty: (a) przełącznik od daty wejścia M6, historyczne linie wyceniane WAC do wyczerpania;
   (b) jednorazowa konwersja `StockLine` na sztuczne partie o koszcie średnim.
   *Propozycja M5: wariant (a) — prostszy i nie fałszuje historii.* **Do uzgodnienia z M6.**

6. ~~**Czy crate `libm` mieści się w granicy „własnego silnika"?**~~ — **ZAMKNIĘTE (K-6).**
   M0 dostarczył `core::det_math` (`ln`, `ln1p`, `log2`, `exp`, `exp_m1`, `exp2`, `pow`, `sqrt`,
   `softmax`, ≤ 2 ULP) plus lint zakazujący libm w symulacji. M5 używa `det_math`, nie `libm`.
   Konsekwencja projektowa: `softmax` sumuje w kolejności indeksów bez redukcji parami, więc
   sortowanie kandydatów **przed** wywołaniem jest jedynym miejscem ustalającym kolejność
   sumowania (5.4).

7. **Bank: firma (`FirmId`) czy abstrakcja?**
   `sim/firms` jest własnością M7. W M5 bank musi mieć konto, kapitał i politykę, ale nie potrzebuje
   pracowników ani produkcji.
   *Propozycja M5: bank ma `FirmId` i `Ledger` (jak sklep), ale jego „AI" to funkcja `assess_credit`;
   M7 czyni go pełną firmą z załogą i osobowością.* **Do uzgodnienia z M7.**

8. ~~**Hook VAT: pole `Transaction.tax` czy osobny rejestr podatkowy?**~~ — **ZAMKNIĘTE przez M8.**
   Rozgraniczenie: `Transaction.tax` zostaje **wyłącznie dla VAT-u** (nierozłączny od pojedynczej
   transakcji detalicznej), pozostałe daniny idą do `ChargeRegistry` po stronie M8 i lądują
   w dzienniku jako `TxKind::TaxPayment`. **Migracji dziennika w M8 nie będzie.**
   M8 buduje `CityTaxEngine` jako jedyną prawdziwą implementację mojego haka `TaxEngine`.

9. **Próg odłożenia zakupu: jeden globalny czy per potrzeba?**
   *Propozycja M5: `thr0` per `NeedId` z `data/economy/` (bo odłożenie zakupu chleba i odłożenie
   zakupu butów to nie to samo), przy normalizacji wag `Σ|w| = 1` zapewniającej wspólną skalę U.*
   Rozstrzygnięcie zależy od kalibracji balansatorem — do domknięcia w WP13.

10. **Balansator używa `tools/headless` jako biblioteki czy uruchamia proces?**
    Biblioteka: szybciej, jeden proces na seed w puli wątków. Proces: izolacja awarii, łatwiejsza
    równoległość na poziomie CI.
    *Propozycja M5: biblioteka + `--jobs N` procesów na poziomie CLI (najlepsze z obu).*
    **Do uzgodnienia z M0 — wymaga, żeby `tools/headless` eksponował API, nie tylko `main`.**

11. **Kwantyzacja `ObservedElasticity` w stanie trwałym.**
    Elastyczność jest floatem, ale trafia do stanu trwałego (a więc do hasha i do zapisu).
    *Propozycja M5: przechowywać jako `i32` w bp; float tylko jako wartość pośrednia w obliczeniu.*
    Decyzja wewnętrzna M5, zgłoszona dla spójności z dok. 00 §2.

12. ~~**Nazwa typu `PriceePolicy`**~~ — **ZAMKNIĘTE.** Potwierdzone: `PricePolicy`.

13. **Nowe, otwarte — źródło `LostSaleTracking` dla zakładów gracza.**
    Przyjąłem propozycję M9 (histogram zawsze dla zakładów gracza + pierścień 256 dla oznaczonych,
    zero kosztu dla AI, ≈ 2,5 MB przy 200 sklepach). Otwarte zostaje **kto ustawia flagę**:
    `sim/economy` czytając własność zakładu, czy `game/` przy przejęciu sklepu przez gracza.
    *Propozycja M5: `game/` ustawia, `sim/economy` tylko czyta — bo „kto jest graczem" nie jest
    pojęciem ekonomicznym.* **Do potwierdzenia z M9.**

14. **Nowe, otwarte — kto woła kanał kapitału zewnętrznego.**
    `MoneySupplyLedger.external_capital_in/out` i obie funkcje istnieją w M5, ale żaden system ich
    nie używa (pokryte wyłącznie testem P1b). *Propozycja M5: pozostają `pub`, wołane wyłącznie
    z `sim/firms` w M7.* Jeśli M7 potrzebuje innej ziarnistości niż „inwestor + kwota"
    (np. transzy, harmonogramu wejścia) — trzeba to wiedzieć przed zamrożeniem struktury.
    **Do potwierdzenia z M7.**

---

## 10. Szacunek wielkości

| WP | Nazwa | Rozmiar | Uzasadnienie |
|---|---|---|---|
| WP1 | Pieniądz, konta, podwójny zapis | **M** | mało kodu, dużo rygoru; testy własnościowe dominują nakład |
| WP2 | Oferta i indeks przestrzenny | **M** | opiera się na gotowym `engine/spatial` z M2 |
| WP3 | Sklep: magazyn, półka, asortyment, zewnętrzny dostawca | **L** | trzy stany (zaplecze/półka/oferta) + polityka zapasu + punkt wymiany z M6 |
| WP4 | Funkcja użyteczności i wybór oferty | **L** | rdzeń fazy; koszt leży w kalibracji i determinizmie, nie w liczbie linii |
| WP5 | Rozliczanie transakcji | **M** | mechanizm prosty, ale każdy błąd łamie niezmiennik |
| WP6 | Polityki cenowe AI | **L** | cztery niezależne mechanizmy korekty + obserwacja z opóźnieniem + eksperymenty |
| WP7 | Księgowość sklepu | **L** | plan kont, dziennik, trzy raporty, wycena zapasów bez dryfu |
| WP8 | Budżety GD | **M** | budżetowanie kopertowe + ścieżka niedoboru |
| WP9 | Banki i kredyt | **M** | zakres podstawowy; harmonogram i kreacja pieniądza to najdelikatniejsze punkty |
| WP10 | CPI, stopa bazowa, inflacja emergentna | **S** | dwie funkcje i koszyk w danych; trudność jest w kalibracji, nie w kodzie |
| WP11 | Polityki cenowe gracza | **S** | zero nowej logiki — tylko UI i walidacja nad typami z WP6 |
| WP12 | Panel sklepu w UI | **M** | trzy zakładki + snapshot + nakładka zasięgu |
| WP13 | Balansator i bramki CI | **L** | CLI, scenariusze, 9 bramek, raport, wpięcie w CI, meta-test bramek |
| WP14 | Testy własnościowe, determinizm, benchmarki | **M** | rozłożone na całą fazę, nie blok na końcu |

`sim/economy::kernel` (D20) **nie jest osobnym WP** — `next_price` powstaje w WP6, `take_cogs`
i `ledger_post` w WP7, walidacja zapisu w WP1. Napisane od razu w rdzeniu kosztują tyle samo,
co napisane obok, a P4–P7 stają się tańsze. Moduł `labor` (D19) też nie jest tu wyceniony:
należy do zakresu M7, M5 dostarcza wyłącznie sygnatury, które i tak buduje dla detalu.

Sumarycznie: 4 × L, 8 × M, 2 × S. Faza jest **największa z dotychczasowych** — to konsekwencja
bycia pierwszym vertical slice'em. Punkt kontrolny po WP5: istnieje grywalna pętla
(otwórz sklep → ustal cenę → obserwuj klientów). Jeśli do tego momentu budżet fazy jest przekroczony,
kandydatami do przesunięcia do M7 są WP11 i eksperymenty cenowe z WP6 — **nie** WP13,
bo balansator w CI jest wskazany w §20.4 jako główna mitygacja ryzyka projektu.

## Zmiany wpisane po M3

Zgodnie z `K-18`. To są rzeczy, o których M5 wie **na pewno** po zamknięciu M3d.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **Punktem podmiany jest `sim::agents::Sources.places: Box<dyn PlaceProvider>`** w zasobie `AgentSources`. M5 wstawia tam indeks ofert i nie zmienia ani jednej linii w `planner.rs` ani w `DayLoopSystem` | Kryterium akceptacyjne nr 7 fazy M3 mówi o planerze; pętla doby wywołuje `fulfil` przez ten sam trait. Atrapy `FlakyPlaces` i `PanickingPlaces` zostają jako testy kontraktowe i M5 ma je nadal przechodzić |
| Z-2 ★ | **`FulfilOutcome::Done.satisfaction` jest przyrostem i stosuje się go na końcu wizyty**, a czas wizyty nie liczy się do spadku tej potrzeby (korekta H-4) | M5 zastępuje stałą z `data/needs/needs.ron` wartością zależną od kupionego dobra. Semantyka („przyrost", „na końcu") jest kontraktem pętli doby, a nie szczegółem atrapy |
| Z-3 ★ | **`Safety`, `Housing` i `Status` mają dziś tempo spadku 0** (korekta H-3); `Status` i `Housing` czekają na M5/M9 | Potrzeba, która spada, a której nic nie podnosi, dochodzi do zera u wszystkich i wypycha z miasta każdego po kolei przez progi migracji. Tempo wnosi faza, która wnosi mechanizm — dla statusu jest nim konsumpcja statusowa z §6.4 |
| Z-4 | **Kalibracja potrzeb jest zadaniem balansatora, nie M3.** Zmierzone po 30 dobach gry (81 tys. mieszkańców): głód 16/100, sen 15/100, higiena 26/100, wypoczynek 36/100, kontakty 33/100 — mierzone o północy | Tabela §5.5 nie była nigdy puszczana przez wielodobowy przebieg razem z planerem. Liczby są punktem wyjścia dla `tools/balansator`, a nie wynikiem do przyjęcia: scenariusz `headless m3day` wypisuje je po każdym przebiegu |
| Z-5 | **`Household.stock` zużywa się o jeden dzień na dobę** (`HouseholdStockSystem`, `Cadence::EveryDay`) i to on wyzwala zakupy przez próg w fazie 3 planera | M5 zastępuje ten system realną konsumpcją towarów z partiami. Próg wyzwalający zakupy zostaje ten sam — tak jak zapowiada M3 §6.2 |
