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
| Oferta jako jedyny nośnik ceny | `Offer` w **dedykowanej arenie** (`K-16`, nie encja ECS), indeks przestrzenny per kategoria, agregacja do UI | §6.1, §17.5 |
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
| **M5b — Sklep i zakup** `[x]` | WP3, WP4, WP5 | 5.3, 5.4, 5.5, 5.7 | Mieszkaniec wychodzi po chleb, wybiera ofertę i wraca; pieniądz i sztuki zgadzają się po obu stronach. | `M5b-sklep-i-zakup.md` |
| **M5c — Ceny i księgowość** `[x]` | WP6, WP7, WP11 | 5.6, 5.8 | Sklep AI podnosi cenę przy niedoborze i obniża przy zaleganiu; rachunek wyników i bilans domykają się co do grosza. | `M5c-ceny-i-ksiegowosc.md` |
| **M5d — Budżety, banki, inflacja** `[x]` | WP8, WP9, WP10 | 5.9, 5.10 | Inflacja emergentna: koszyk CPI liczony z transakcji świata, stopa bazowa reagująca na niego bez ręcznego sterowania. | `M5d-budzety-banki-inflacja.md` |
| **M5e — Panel, balansator, domknięcie** `[x]` | WP12, WP13, WP14 | 5.11, 5.12 | Pełny artefakt fazy z §1 dokumentu fazy: otwórz sklep, ustal ceny, obserwuj klientów; balansator w CI. | `M5e-panel-balansator-domkniecie.md` |

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
| `reprice` dla 2 tys. sklepów × 40 towarów | < 40 ms raz na dobę (**zmierzone 1,11 ms** przy 18 towarach katalogu M5) |
| `observe_competitors` — pełne odświeżenie obrazu konkurencji 2 tys. sklepów | < 40 ms (ścieżka dopisana po M5c; **zmierzone 30,1 ms** przy promieniu 1 200 m) |
| `candidates` — **cała decyzja zakupowa bez podróży**, 40 sklepów w promieniu (`U-24`) | **zmierzone 0,59 µs**; to jest liczba, którą §7.3 miał rozdzielić od kosztu routera M4 — sama decyzja mieści się w budżecie z ogromnym zapasem, a 10 s doby w scenariuszu idzie z dwóch `begin_trip` na zakup |
| `Market::shop_panel` — migawka panelu jednego sklepu (M5e §5.12) | **zmierzone 1,73 µs**; panel jest darmowy w skali klatki, kadencja `EveryHour` jest zapasem, nie koniecznością |
| `Market::balance_sample` — dobowa próbka balansatora, 2 tys. sklepów | **zmierzone 0,78 ms**; przy 365 dobach to 0,29 s na przebieg, czyli poniżej szumu wobec budżetu profilu `ci` |
| Pamięć: `Offer` | **≤ 56 B** (skorygowane po M5a, `U-5`); `ShopLostSales` ≤ 256 wpisów × 16 B na sklep |

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
| **G4 Reaktywność szoku podaży** | §20.1 „widoczny w 2–7 dni, wygaszony w 2–8 tygodni" | scenariusz `supply-shock` (cena hurtowa wskazanego towaru +80% w dniu 180): mediana ceny detalicznej pokrywa ≥ 50% szoku w przedziale **2–7 dni** (`t_response`) i stabilizuje się w ±10% nowego poziomu w przedziale **14–56 dni** (`t_settle`). Zbyt szybko = też fail (rynek bez tarcia, sprzeczne z opóźnieniem obserwacji 1–7 dni). **Bramka doradcza do czasu M6** — zmierzone w M5e: odpowiedź 2 doby (zielone), stabilizacja 5 dób wobec 14–56, bo tarcie wnosi rynek B2B, nie detal (decyzja otwarta nr 17) |
| **G5 Rynek nie wymiera** | §16.5 | ≥ 1 aktywna oferta w każdej kategorii w 100% dni; mediana `deferral_rate` < 25%; `stockout_rate` < 15% |
| **G6 Brak monopolizacji z kalibracji** | §6.4 (softmax, nie argmax) | HHI per kategoria per dzielnica < 0,6 w medianie seedów — chroni przed zjechaniem temperatury softmaxu do zera |
| **G7 Zachowanie pieniądza** | dok. 00 §6 | każdy przebieg kończy P1 zielono, 0 groszy |
| **G8 Determinizm** | dok. 00 §3 | dwa przebiegi seeda 0 → identyczny ciąg hashy |
| **G9 Wyjaśnialność** | §20.1 „100% decyzji" | 0 decyzji bez `DecisionReason` w próbce 10⁵ |

Profile: **ci** (PR) = bramki G1–G3, G5, G7–G9, budżet czasu < 10 min.
**nightly** = wszystkie bramki + cztery scenariusze + raport Markdown.

**Korekta po M5e (`AD-6`):** macierz profilu `ci` to **4 seedy × 120 dni**, nie 8 × 365.
Zmierzone na tym repozytorium: doba miasta 3 tys. mieszkańców to ~1,6 s, czyli rok gry
to ~9,7 min **na jeden seed** — osiem seedów nie zmieści się w dziesięciu minutach
niezależnie od równoległości. Źródło kosztu nazywa `U-24` i **nie leży w gospodarce**:
`utility_of_offer` mierzy 15 ns, cała decyzja zakupowa 0,59 µs, a każdy zakup to dwa
wywołania routera M4. Pełną macierz (8 × 365 × cztery scenariusze) puszcza bieg nocny.
Raport Markdown generuje **Rust, nie Python**: bez wykresów, tabelami — §20.4 dopuszcza
Pythona w narzędziach, ale dopóki raport jest tabelą, druga technologia w łańcuchu CI
kosztuje więcej, niż daje.

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
| R6 | ~~Zależność od M4 — człon `g` wymaga kosztu przejazdu w czasie **i** pieniądzu~~ → **ryzyko zmieniło naturę, nie zniknęło.** Kontrakt jest dostarczony (`TravelOracle::estimate` zwraca minuty **i** `Money`, sekcja 9 pkt 3), ale jest **drogi**: wycenia sześć opcji z routingiem i woła się już ponad milion razy na dobę metropolii bez udziału M5 | średnie / wysokie | wstępny ranking całkowitoliczbowy **przed** wyceną (ta sama mitygacja co R7, i to ona jest teraz właściwa); bramka benchmarkowa na `decide_purchase`; zaślepka z tabeli czasów dzielnica↔dzielnica zostaje jako awaryjna ścieżka rozwoju WP4 |
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

**Domknięcie po M4 (`K-18`).** Zamknęły się dwa kolejne punkty: **1** — przez `K-16`, i to
**odwrotnie** niż brzmiała propozycja M5 (oferta jest uchwytem do areny, nie encją ECS) — oraz **3**,
bo kontrakt kosztu przejazdu został dostarczony i nie trzeba go budować. Przy okazji wyszło, że
„wymagają rozstrzygnięcia przed startem fazy" jest zbyt szerokie: większość punktów dotyczy
podfaz, do których jeszcze daleko. **Przed pierwszym pakietem (WP1) trzeba rozstrzygnąć dokładnie
dwa: 14 i — w wąskiej części — 7.** Oba zamrażają kształt typów, które WP1 tworzy i którymi
niezmiennik P1/P1b jest testowany. Doszedł jeden nowy punkt, **15**, znaleziony w kodzie M3.

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

1. ~~**`Offer` jako encja ECS czy komponent półki?**~~ — **ZAMKNIĘTE przez `K-16`.**
   Rozstrzygnięcie zapadło **odwrotnie** niż brzmiała propozycja M5 i jest wiążące:
   **oferta nie jest encją ECS**, tylko uchwytem do dedykowanej areny,
   `OfferId { index: u32, generation: NonZeroU32 }`. Argument z dok. 00 §2 przestał obowiązywać,
   bo `K-16` jest jawną korektą tamtego paragrafu — `engine/core/src/ids.rs` mówi to wprost.
   Powód jest ten, który propozycja liczyła jako koszt i odrzuciła: ofert jest rzędu 10⁵,
   mają skrajnie wysoką rotację, a **żadna nie jest nigdy odpytywana przekrojowo po archetypach** —
   do oferty dociera się przez półkę albo przez indeks przestrzenny kategorii. Płacenie za nie
   mutacją strukturalną ECS to koszt bez korzyści.
   Wymagania nienegocjowalne z `K-16`: arena wchodzi do funkcji haszującej stan w kolejności
   indeksów, uchwyt po zwolnieniu **nigdy** nie jest ponownie ważny (stąd generacja), a snapshot
   i zapis traktują arenę jak sekcję ECS. Mechanizm jest gotowy: `engine/core/src/arena.rs`
   (`Arena<T>`, `ArenaHandle<T>`, `Arena::hash_state`), właściciel M0. M5 dostarcza instancję.

2. ~~**Skąd bierze się dochód GD w M5?**~~ — **ZAMKNIĘTE w M5b (`U-16`)** przez przyjęcie
   propozycji: `pay_incomes` wypłaca `Household.income_monthly` z konta `RestOfWorld`
   na granicy miesiąca, kwota pochodzi z generacji populacji M3, struktura wypłaty
   należy do M5, a M7 podmieni źródło. Punkt nie doczekał uzgodnienia, bo okazał się
   blokerem **wyniku podfazy**: gospodarstwa z Etapu 8 mają saldo zero, więc pierwszy
   przebieg scenariusza dał 170 tys. odmów „brak środków" i zero transakcji.
   Poprzednie brzmienie:
   M5 potrzebuje dochodu, żeby budżet miał sens; pensje emergentne to M7.
   *Propozycja M5: `HouseholdBudget.income_monthly` wypłacane z konta `RestOfWorld`, kwota
   z profilu zawodowego mieszkańca (M3). Kto jest właścicielem tej logiki — M3 czy M5? Propozycja: M5,
   bo dotyczy pieniądza; M7 podmienia źródło bez zmiany struktury.* **Do uzgodnienia z M3 i M7.**

3. ~~**Sygnatura `travel_cost` z M4.**~~ — **ZAMKNIĘTE przez M4.**
   Kontrakt istnieje pod inną nazwą i nie trzeba go budować:
   `TravelOracle::estimate(from, to, depart, who) -> TravelEstimate { minutes, cost: Money, mode, reason }`
   (`sim/agents/src/places.rs`). Od M4c `cost` jest realną kwotą — bilet, taryfa taksówki, paliwo
   i amortyzacja policzone w `GeneralizedCost.money_cost` — więc człon `g` dostaje czas **i** pieniądz
   z jednego wywołania. Wywołanie jest **bezskutkowe**: pod spodem idzie `plan(commit = false)`,
   które parking wyłącznie *sprawdza*, nie rezerwuje, nie wstawia pasażera do kolejki przystanku
   i nie zapisuje nawyku.

   **Otwarte zostaje co innego: koszt tego wywołania.** `estimate` wycenia wszystkie sześć opcji
   z routingiem, a komentarz `ponytail:` w `sim/traffic/src/oracle.rs` mówi, że woła się już
   **ponad milion razy na dobę metropolii** — zanim M5 dołoży 3–15 kandydatów na decyzję zakupową.
   To nie jest brak kontraktu, tylko budżet, i mieszka teraz w ryzykach R6/R7, a nie tutaj.
   *Do rozstrzygnięcia w WP4, nie przed startem fazy: albo wstępny ranking całkowitoliczbowy
   odcina kandydatów przed wyceną, albo `estimate` dostaje tańszy wariant „tylko czas i pieniądz
   dla środka z nawyku".*

4. ~~**Kto jest właścicielem `KnownPlaces` (znajomość sklepów, §5.7)?**~~ — **ZAMKNIĘTE
   w M5b (`U-20`)** zgodnie z propozycją: własność zostaje przy M3, M5 tylko czyta.
   Typ nazywa się inaczej, niż zakładał plan (`KnowledgeRef` + `KnowledgeSlab` +
   `KnowledgeView`, nie `KnownPlaces`), ale filtr kandydatów działa przez niego bez
   dopisywania ani jednego pola; zdarzenie „odwiedzono sklep" wnosi gotowe
   `social::learn_place`, a plotkę dokłada M10.

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
   **Wąska część ZAMKNIĘTA w M5a (`U-13`):** wariant `AccountOwner::Bank(FirmId)` został przyjęty
   domyślnie i zamrożony razem z enumem w WP1 — bank ma `FirmId`, konto własne i limit debetu
   jak każda firma.
   **Szeroka część ZAMKNIĘTA w M5d (`AA-1`) przez przyjęcie propozycji domyślnej:** bank ma
   `FirmId` i konto, a jego „AI" to funkcja `assess_credit`. Nie ma pracowników, produkcji
   ani własnej `Ledger` — w M5 robi dokładnie trzy rzeczy (ocenia wniosek, tworzy depozyt,
   niszczy go przy spłacie) i wszystkie trzy przechodzą przez jedno konto. M7 czyni go pełną
   firmą z załogą i osobowością, nie ruszając `AccountOwner::Bank`.

8. ~~**Hook VAT: pole `Transaction.tax` czy osobny rejestr podatkowy?**~~ — **ZAMKNIĘTE przez M8.**
   Rozgraniczenie: `Transaction.tax` zostaje **wyłącznie dla VAT-u** (nierozłączny od pojedynczej
   transakcji detalicznej), pozostałe daniny idą do `ChargeRegistry` po stronie M8 i lądują
   w dzienniku jako `TxKind::TaxPayment`. **Migracji dziennika w M8 nie będzie.**
   M8 buduje `CityTaxEngine` jako jedyną prawdziwą implementację mojego haka `TaxEngine`.

9. ~~**Próg odłożenia zakupu: jeden globalny czy per potrzeba?**~~ — **ZAMKNIĘTE w M5b
   (`U-21`)** zgodnie z propozycją: `thr0` per `NeedKind` w `data/economy/choice.ron`,
   przy normalizacji `Σ|w| = 1` dającej wspólną skalę. Liczby zostają do strojenia
   balansatorem (WP13); zamrożona jest struktura, nie kalibracja. Poprzednie brzmienie:
   *Propozycja M5: `thr0` per `NeedId` z `data/economy/` (bo odłożenie zakupu chleba i odłożenie
   zakupu butów to nie to samo), przy normalizacji wag `Σ|w| = 1` zapewniającej wspólną skalę U.*
   Rozstrzygnięcie zależy od kalibracji balansatorem — do domknięcia w WP13.

10. **ZAMKNIĘTE w M5e (`K-32`) zgodnie z propozycją domyślną: biblioteka.**
    `tools/headless` dostał target biblioteczny z dwoma modułami publicznymi
    (`population`, `retail`); scenariusze zostały modułami binarki, bo CLI i kod
    wyjścia nie należą do biblioteki. `--jobs N` w balansatorze steruje **liczbą
    seedów liczonych równolegle**, a nie liczbą procesów — izolacja awarii nie
    była warta drugiego mechanizmu, skoro przebieg, który panikuje, i tak
    czerwieni bramkę. Zmiana dotyczy crate'u należącego do M0, więc ma własny
    wpis `K-32` w dokumencie nadrzędnym, a nie rozstrzygnięcie lokalne w fazie.
    Skutek uboczny, którego punkt nie przewidywał i który okazał się ważniejszy
    od samego wyboru: **most „zakłady Etapu 7 → rynek" przestał być kopiowany**.
    Przed M5e stał wyłącznie w ciele scenariusza `m5shop`, a klient graficzny
    i balansator musiałyby go przepisać — teraz wszyscy trzej wołają
    `magnat_headless::retail::setup`. Poprzednie brzmienie:
    **Balansator używa `tools/headless` jako biblioteki czy uruchamia proces?**
    Biblioteka: szybciej, jeden proces na seed w puli wątków. Proces: izolacja awarii, łatwiejsza
    równoległość na poziomie CI.
    *Propozycja M5: biblioteka + `--jobs N` procesów na poziomie CLI (najlepsze z obu).*
    **Do uzgodnienia z M0 — wymaga, żeby `tools/headless` eksponował API, nie tylko `main`.**
    **Stan faktyczny po M4 (sprawdzony w kodzie):** `tools/headless` **nie ma targetu
    bibliotecznego** — w `Cargo.toml` nie ma sekcji `[lib]`, a w `src/` jest wyłącznie `main.rs`
    z modułami prywatnymi. Wariant „biblioteka" wymaga więc realnej zmiany w cudzym crate'cie
    (właściciel: M0), a nie samego uzgodnienia. Dotyczy WP13, czyli ostatniej podfazy — ale
    zgłoszone teraz, bo to jest ta klasa rzeczy, którą odkrywa się w tygodniu domknięcia fazy.

11. ~~**Kwantyzacja `ObservedElasticity` w stanie trwałym.**~~ — **ZAMKNIĘTE w M5c (`W-9`)**
    zgodnie z propozycją: `ObservedElasticity.e_bp: i32` w punktach bazowych, float wyłącznie
    jako wartość pośrednia w `det_math::ln`. Przy okazji z tego samego powodu
    `PriceController.last_experiment` jest `Option<Tick>`, a nie `Tick`: `Tick(0)` jako „nigdy"
    zablokowałby eksperymenty przez pierwsze 30 dób świata.
    Poprzednie brzmienie:
    *Elastyczność jest floatem, ale trafia do stanu trwałego (a więc do hasha i do zapisu).
    Propozycja M5: przechowywać jako `i32` w bp; float tylko jako wartość pośrednia w obliczeniu.*

12. ~~**Nazwa typu `PriceePolicy`**~~ — **ZAMKNIĘTE.** Potwierdzone: `PricePolicy`.

13. **ZAMKNIĘTE w M5b (`U-22`)** zgodnie z propozycją: flagę ustawia `game/`
    (`Market::set_tracking`), `sim/economy` ją tylko czyta, a poziom śledzenia
    **nie wchodzi do hasha stanu** — inaczej kliknięcie „śledź" zmieniałoby świat.
    Poprzednie brzmienie: **źródło `LostSaleTracking` dla zakładów gracza.**
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
    **Blokowało WP1**, i mocniej, niż wynikałoby z brzmienia punktu: `MoneySupplyLedger` powstaje
    w **pierwszym** pakiecie i jest częścią niezmiennika P1/P1b, czyli tego samego testu
    własnościowego, który jest kryterium zamknięcia M5a.
    **ZAMKNIĘTE w M5a przez przyjęcie propozycji domyślnej (`U-10`):** ziarnistość to „inwestor
    + kwota", obie funkcje są `pub`, zwracają `Result<TxId, TxError>` i biorą `Tick`. Transze
    i harmonogram wejścia, jeśli M7 ich potrzebuje, są **jego** stanem — `Books` widzi z nich
    pojedyncze wywołania, więc inna ziarnistość po stronie M7 nie rusza niezmiennika. Kanał jest
    pokryty testem P1b od pierwszego dnia, więc rozjazd wyjdzie u tego, kto go wprowadzi.

15. **ROZSTRZYGNIĘTE w M5b (`U-23`) — odwrotnie niż brzmiała propozycja: pola nie
    dopisujemy.** `candidates` nie dostaje `TravelOracle`, więc nie ma kto wypełnić
    kwoty; wołanie wyceny 3–15 razy na decyzję to dokładnie koszt, przed którym
    ostrzegają R6 i R7. Człon `g` liczy w M5b koszt czasu (`travel_min · vot`),
    a pieniężna część dojazdu jest zerem z sufitem nazwanym w kodzie przy
    `Candidate.travel_money`. Poprzednie brzmienie: **`PlaceCandidate` nie niesie
    kwoty, a człon `g` jej wymaga.**
    `sim/agents::PlaceCandidate` ma dziś `place`, `travel_min: u16`, `score: i32` i `reason` —
    **brak pola `Money`**. Komentarz przy `score` zapowiada wyłącznie podmianę znaczenia na
    „użyteczność §6.4 × 1000", więc kwota nie ma dziś gdzie usiąść. Bez niej M5 musi wołać wycenę
    przejazdu osobno dla każdego kandydata, co wraca prosto do kosztu z punktu 3.
    *Propozycja M5: `PlaceCandidate` dostaje `travel_cost: Money` wypełniane przez `PlaceProvider`,
    bo to ten sam wołający, który już zna `travel_min` — jedno wywołanie zamiast dwóch.
    Właścicielem struktury jest M3, więc zmiana należy do M5 jako rozszerzenie cudzego typu.*
    **Do uzgodnienia z M3.** Nie blokuje M5a; blokuje WP4 w M5b.

16. **Nowe, otwarte po M5c — kto daje konto stacji paliw, przewoźnikowi, taksówce i parkingowi.**
    Niezmiennik świata z `U-17` brzmi `society::total_money + Books::total_balance() == const`
    i **nie domyka się**, kiedy w scenariuszu jeździ ruch: mieszkaniec płaci za paliwo,
    bilet i taryfę z komponentu `Wealth`, a drugą stroną jest `FuelLedger`/`FareLedger`
    z `sim/traffic` — rejestr, nie konto. Po dopisaniu obu rejestrów do sumy zostaje
    **−5,6 tys. zł przez 31 dób i +63,2 tys. zł przez 40 dób** na 100 mln zł w mieście
    28 tys. mieszkańców — czyli rzędu 0,006–0,06 %, ale **ze zmianą znaku**, więc
    kanałów bez pary jest co najmniej dwa i działają w przeciwne strony. Taryfa
    taksówkowa (`FareLedger.taxi_revenue`) jest podejrzanym numer jeden: rośnie
    o 200 tys. zł przez 40 dób, a w `sim/traffic` nie ma odpowiadającego jej zapisu
    po stronie `Wealth`. Przy przebiegu 8-dobowym (M5b) nie było tego widać, bo obie
    strony były poza sumą.
    *Propozycja M5: konto dostaje stację paliw M5d razem z obrotem stacji (`T-2` mówi
    wprost „M5 podmienia ciało na ofertę w `sim/economy`"), a przewoźnika i parking
    M7/M8; do tego czasu bramką scenariusza jest niezmiennik P1 w księgach
    (tolerancja 0) i domknięcie księgowości zakładu, a suma świata jest **mierzona
    i wypisywana z rozbiciem**, nie bramkowana.* **Do uzgodnienia z M4 — kanał bez pary
    trzeba znaleźć przed wpięciem stacji, inaczej M5d odziedziczy go razem z kontem.**
    **Stan po M5d: punkt zostaje otwarty, świadomie.** Obrót stacji (`T-2`) nie należy
    do żadnego z pakietów M5d — §4 przypisuje tej podfazie WP8–WP10 — a sam punkt żąda
    uzgodnienia z M4 **przed** wpięciem konta. Wciągnięcie go tutaj znaczyłoby odziedziczenie
    kanału bez pary razem z kontem, czyli dokładnie to, przed czym ostrzega ostatnie zdanie.
    Bramka scenariusza zostaje taka, jak ustawiło ją `W-17`. **Reszta urosła i urosła
    z powodu, który warto zapisać:** po M5d jest to **+163,0 tys. zł przez 40 dób** wobec
    +63,2 tys. zł przed nią, i nie jest to regres M5d — pieniądz kredytowy zwiększył wydatki
    na dojazdy, więc kanał bez pary (taryfa taksówkowa) przepuszcza proporcjonalnie więcej.
    Wniosek dla tego, kto go domknie: **to jest błąd mnożnikowy, nie addytywny**, i będzie
    rósł z każdą fazą dokładającą pieniądza.


17. **Nowe, otwarte po M5e — dolna granica `t_settle` w bramce G4 mierzy tarcie,
    którego M5 nie modeluje.**
    Zmierzone na scenariuszu `supply-shock` (120 dób, +80 % ceny hurtowej chleba,
    miasto 3 tys. mieszkańców): **odpowiedź w 2 dobach** — w widełkach 2–7 —
    i **stabilizacja w 5** wobec widełek 14–56. Mechanizm jest zrozumiały i nie
    jest błędem: dostawca zewnętrzny to zaślepka o nieskończonej podaży, bez
    kontraktów, bez terminów płatności i bez zapasu w drodze, a sklep, któremu
    wzrósł koszt własny, przecenia **od razu** — opóźnienie 1–7 dób z §6.3 dotyczy
    **obserwacji konkurencji**, a nie własnego rachunku. Uzasadnienie dolnej
    granicy w §7.4 („zbyt szybko = też fail, rynek bez tarcia") jest więc trafne
    co do zasady i nietrafne co do kanału: tarcie ma wejść razem z rynkiem B2B.
    *Propozycja M5: G4 zostaje z niezmienionymi progami, ale jest **doradcza**
    (mierzy i raportuje, nie wywraca przebiegu) do czasu, aż M6 wniesie kontrakty
    i zapas w drodze; wtedy staje się bramką wiążącą bez zmiany ani jednej liczby.*
    Przyjęte domyślnie w M5e — `GateOutcome::advisory`, werdykt `DORADCZE`
    w tabeli i w raporcie nocnym, żeby doba, w której M6 to zmieni, była widoczna.
    **Do potwierdzenia z M6**: alternatywą jest przesunięcie dolnej granicy
    `t_settle` w dół dla M5 i podniesienie jej w M6 — odrzucone, bo próg, który
    wędruje za implementacją, przestaje być kryterium akceptacyjnym.
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
| WP12 | Panel sklepu w UI i gospodarka w kliencie | **L** | trzy zakładki + snapshot + nakładka zasięgu **plus podpięcie `Market` do `tools/magnat`** (`AA-9`) |
| WP13 | Balansator i bramki CI | **L** | CLI, scenariusze, 9 bramek, raport, wpięcie w CI, meta-test bramek |
| WP14 | Testy własnościowe, determinizm, benchmarki | **M** | rozłożone na całą fazę, nie blok na końcu |

`sim/economy::kernel` (D20) **nie jest osobnym WP** — `next_price` powstaje w WP6, a `take_cogs`
i `ledger_post` w WP7. Napisane od razu w rdzeniu kosztują tyle samo, co napisane obok,
a P4–P7 stają się tańsze. **Korekta po M5a (`U-11`): w WP1 rdzeń nie powstaje.** Plan
przypisywał tam „walidację zapisu", ale zapisu księgowego w WP1 nie ma — `Ledger` powstaje
dopiero w WP7, a `Σ lines == 0` dla dwuliniowego przelewu jest tożsamością. Jedyna arytmetyka
pieniądza w WP1 to podział kwoty, gotowy w `magnat_core::split_proportional` z M0. Moduł `labor` (D19) też nie jest tu wyceniony:
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

---

## Zmiany wpisane po M4

Zgodnie z `K-18`. To są rzeczy, o których wiemy **na pewno** po zamknięciu M4;
faza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| T-1 ★ | **`tools/balansator` ma na starcie jednego mieszkańca: `calibrate-vdf`.** Zadanie istnieje dziś jako `headless calibrate-vdf` (M4d §5.4) i M5, tworząc crate, **przenosi je**, a nie pisze od nowa | M4 §5.4 przypisywał kalibrator balansatorowi, ale właścicielem crate'u jest M5 (dok. 00 §1) i M4 nie miał prawa go założyć. Zadanie jest gotowe, zmierzone i ma bramkę w CI; przeniesienie to zmiana adresu, nie treści. **Zachowaj domyślne „nie zapisuj"**: przepisanie `min_speed_dkmh` zmienia czasy przejazdu całego miasta, czyli pieniądze — i to jest dokładnie ta klasa decyzji, dla której balansator w ogóle powstaje |
| T-2 | **Cena paliwa jest gotowa do podmiany w jednym miejscu** (`D9` rozstrzygnięte w M4b): ruch czyta ją przez `VehicleCatalog::price_gr`, nie z pliku wprost. M5 podmienia ciało na ofertę w `sim/economy` i **nie rusza ani jednej linii `sim/traffic`** | Zdarzenie `FuelPurchased { station, volume, unit_price }` i rejestr `FuelLedger` już istnieją i domykają bilans pieniądza; M5 dostaje więc obrót stacji jako wejście, a nie jako rzecz do zbudowania |
| T-3 | **Koszt dojazdu do budżetu gospodarstwa bierze się z `TripLedger.total_money`, a amortyzacja z przebiegu** (`D3`) | `wear_gr_per_100km` wchodzi **wyłącznie** do kosztu uogólnionego decyzji; realne obciążenie budżetu nalicza M5 z `VehicleCondition.odometer_cm`. Policzenie amortyzacji drugi raz z ledgera byłoby podwójnym kosztem — i jest to najłatwiejsza pomyłka w tym styku |
| T-4 | **Karta inspekcji podróży istnieje i pokazuje rozbicie kosztu uogólnionego wszystkich kandydatów w groszach** (`magnat_ui::TripCard`) | M5 pyta „dlaczego nie kupił u mnie" i ma na to gotowy ekran: `NoParkingAtDestination { lots_searched }` i `ModeCompared { chosen, runner_up, delta_gr }` są renderowane przez `engine/ui::describe` i widoczne w karcie. Dokładać trzeba **powód zakupowy**, a nie mechanizm wyjaśniania |
| T-5 | **`sim/agents` ładuje `data/names/` i wystawia `name_catalog()`**, a `magnat_ui::full_name(&Identity)` formatuje nazwę | Panel sklepu i karta klienta w M5 mają pokazywać osobę, nie numer. Formatera nie trzeba pisać — trzeba go zawołać. Nazwy **nie są lokalizacją UI** i nie mają `LocKey` |
| T-6 ★ | **Oferta jest uchwytem do areny, nie encją ECS** (`K-16`). Poprawione w trzech miejscach: §2 (tabela zakresu), §9 pkt 1 i `M5a` (opis WP2 oraz §5.2) | Dokument stał w sprzeczności z **wiążącym** rozstrzygnięciem dokumentu nadrzędnego i przegrywał z nim w najgorszy możliwy sposób: nie w dyskusji, tylko w kodzie, który ktoś by napisał. `K-16` jest jawną korektą dok. 00 §2, więc argument „`OfferId(Entity)` jest w §2" przestał obowiązywać w chwili jej wpisania. Mechanizm areny jest gotowy od M0 (`engine/core/src/arena.rs`), a `engine/core/src/ids.rs` mówi wprost, że `OfferId` **nie** jest tam, gdzie reszta identyfikatorów |
| T-7 ★ | **Kontrakt kosztu przejazdu jest dostarczony; ryzyko R6 zmieniło naturę, a nie zniknęło.** `TravelOracle::estimate` zwraca `TravelEstimate { minutes, cost: Money, mode, reason }`, a wywołanie jest bezskutkowe (`plan(commit = false)`: parking sprawdzany, nie rezerwowany) | §9 pkt 3 nazywał to „najtwardszą zależnością zewnętrzną fazy" i prosił M4 o nową sygnaturę. Nowej sygnatury nie będzie, bo istniejąca robi dokładnie to, o co prosi człon `g`. **Otwarte zostaje co innego i jest to sprawa budżetu, nie kontraktu**: `estimate` wycenia sześć opcji z routingiem i woła się już ponad milion razy na dobę metropolii **bez** udziału M5, a decyzja zakupowa chce 3–15 kandydatów na decyzję. Mitygacją jest wstępny ranking całkowitoliczbowy **przed** wyceną — ta sama, którą R7 ma od początku |
| T-8 ★ | **Blokery WP1 są dwa i oba są zapisane jako „do potwierdzenia później": punkt 14 i wariant `AccountOwner::Bank(FirmId)` z punktu 7** | `MoneySupplyLedger` z kanałem kapitału zewnętrznego powstaje w **pierwszym** pakiecie i jest częścią niezmiennika P1/P1b — czyli tego samego testu, który jest kryterium zamknięcia M5a. Zmiana ziarnistości kanału po WP1 to przepisanie niezmiennika, nie dołożenie pola. Bank jest z M5d, ale jego wariant w `AccountOwner` zamraża się razem z enumem |
| T-9 | **Dwie luki w cudzych crate'ach, obie sprawdzone w kodzie**: `sim/agents::PlaceCandidate` nie ma pola `Money` (nowy punkt 15), a `tools/headless` nie ma targetu bibliotecznego, którego wymaga wariant „balansator jako biblioteka" z punktu 10 | Żadna nie blokuje M5a, obie blokują coś później: pierwsza WP4, druga WP13. Zapisane teraz, bo to jest ta klasa rzeczy, którą inaczej odkrywa się w tygodniu domknięcia fazy — a wtedy zmiana w cudzym crate'cie jest już nie poprawką, tylko przeszkodą |

---

## Zmiany wpisane po M5b

Zgodnie z `K-18`. To są rzeczy, o których wiemy **na pewno** po zamknięciu M5b;
faza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| U-16 ★ | **Decyzja otwarta nr 2 zamknięta zgodnie z propozycją M5: dochód gospodarstwa wypłaca M5.** `sim/economy::pay_incomes` przelewa `Household.income_monthly` z konta `RestOfWorld` na granicy miesiąca; kwota pochodzi z generacji populacji (M3), struktura wypłaty należy do M5, źródło kwoty podmienia M7 | Punkt był opisany jako „do uzgodnienia z M3 i M7", ale okazał się blokerem **wyniku podfazy**, nie kwestią własności: gospodarstwa z Etapu 8 mają saldo zero, więc pierwszy przebieg scenariusza dał 170 tys. odmów „brak środków" i ani jednej transakcji. Uzgadniać nie było czego — bez wypłaty nie ma gospodarki detalicznej |
| U-17 ★ | **Saldo gospodarstwa domowego NIE jest kontem w `Books`.** Zostaje w komponencie `Household` (własność M3), a `Books` widzi je przez kanał `MoneySupplyLedger.household_sector_in/out`. Niezmiennik P1 obowiązuje bez zmian po stronie ksiąg; niezmiennik całego świata brzmi `society::total_money + Books::total_balance() == const` i jest sprawdzany testem end-to-end | Dwa źródła salda rozjeżdżają się przy pierwszej transakcji — to jest dokładnie ostrzeżenie z korekty ★ w `M5d`. Kanał sektora gospodarstw jest tym samym wzorcem co kanał kapitału zewnętrznego M7: strona spoza ksiąg z własną pozycją w ewidencji podaży. Konsekwencja dla M5d/WP8: `HouseholdBudget.account`/`cash` z §5.9 **nie powstaną jako `AccountId`** — koperty stoją na polach komponentu |
| U-18 ★ | **`sim/economy` zależy od `sim/agents`** (implementuje `PlaceProvider`), a stan rynku mieszka w `Market(Arc<Mutex<…>>)`, nie w osobnych zasobach świata | Punktem podmiany jest `Sources.places` (`Z-1`), a ten trait nie dostaje `&World`. Kierunek zależności jest jednostronny i taki zostaje: `sim/agents` nie widzi gospodarki i widzieć jej nie może (cykl z `K-8`) |
| U-19 ★ | **Kontrakt M3 rozszerzony w dwóch miejscach**: `PlaceProvider::candidates` dostaje `who: &CitizenView<'_>`, a `FulfilRequest` — `household_size: u8` i realny `budget_hint` | Obie zmiany są wykonaniem zapowiedzi, które M3 sam zapisał w komentarzach („M5: użyteczność §6.4 × 1000", „M5 wstawia tu realny budżet"), tylko bez kanału, którym dane miały dojść. Szczegóły i powody w tabeli korekt `M5b` (`V-1`, `V-2`) |
| U-20 | **Decyzja otwarta nr 4 zamknięta zgodnie z propozycją: `KnownPlaces` należy do M3**, a M5 tylko czyta. Realizacja: `KnowledgeView` w `candidates`, filtr `known.knows(PlaceRef::Site(...))` | Typ nazywa się inaczej, niż zakładał plan (`KnowledgeRef` + `KnowledgeSlab` + `KnowledgeView`, a nie `KnownPlaces`), ale własność jest ta sama i M5 nie dopisał tam ani jednego pola. Zdarzenie „odwiedzono sklep" wnosi `social::learn_place`, które już istnieje; M10 dokłada plotkę |
| U-21 | **Decyzja otwarta nr 9 zamknięta zgodnie z propozycją: próg odłożenia zakupu jest per potrzeba**, `thr0` z `data/economy/choice.ron`, przy normalizacji wag `Σ|w| = 1` dającej wspólną skalę | Kalibracja balansatorem (WP13) dostaje tabelę do strojenia, a nie jedną liczbę — i to ona zdecyduje o wartościach. Struktura jest zamrożona, liczby nie są |
| U-22 | **Decyzja otwarta nr 13 zamknięta zgodnie z propozycją: flagę `LostSaleTracking` ustawia `game/`**, `sim/economy` tylko czyta (`Market::set_tracking`). Poziom **nie wchodzi do hasha stanu** | „Kto jest graczem" nie jest pojęciem ekonomicznym. Gdyby poziom śledzenia wchodził do hasha, kliknięcie „śledź" zmieniałoby świat — to ta sama zasada, którą M4 zastosował do nakładek ruchu („pomiar nie jest stanem") |
| U-23 ★ | **Decyzja otwarta nr 15 rozstrzygnięta inaczej, niż brzmiała propozycja: `PlaceCandidate` NIE dostaje pola `Money`.** Człon `g` liczy w M5b wyłącznie koszt czasu (`travel_min · vot`), a pieniężna część dojazdu jest zerem z nazwanym sufitem | Propozycja zakładała, że `PlaceProvider` zna koszt przejazdu — nie zna: `candidates` nie dostaje `TravelOracle`, a wołanie go 3–15 razy na decyzję to dokładnie koszt, przed którym ostrzegają R6 i R7 (`estimate` wycenia sześć opcji z routingiem i woła się już ponad milion razy na dobę **bez** udziału M5). Dopisanie pola do cudzej struktury jest tanie; wypełnienie go nie jest. Ścieżka wyjścia zostaje zapisana w kodzie przy `Candidate.travel_money` i wraca, kiedy pomiar pokaże, że czas sam nie wystarcza |
| U-24 | **Budżet §7.3 dla `purchase_decision` mierzy się osobno od podróży, które ta decyzja generuje** | Scenariusz `m5shop` pokazał wzrost czasu doby z 0,6 s do ~10 s po uruchomieniu zakupów, a `utility_of_offer` mierzy 14,7 ns wobec budżetu 120 ns. Różnica jest w routerze M4: każdy zakup to dwa wywołania `begin_trip`. Bramka benchmarkowa fazy (WP14) musi to rozdzielać, inaczej zaczerwieni się na koszcie cudzego modułu |
| U-25 | **Katalog `data/economy/` powstał** i jest dopisany do listy w dokumencie 00 §5: `weights.ron` (wagi użyteczności per potrzeba), `choice.ron` (temperatura, szum, progi, promień, budżety odniesienia), `retail.ron` (kategoria, trwałość, dostawa, narzut per towar, asortyment per rodzaj sklepu) | Lista katalogów w §5 deklaruje się jako kompletna, więc brak wpisu byłby jej błędem, nie luką — ta sama sytuacja co `data/ui/` przy M2 (`K-19`) |

---

## Zmiany wpisane po M5c

Zgodnie z `K-18`. To są rzeczy, o których wiemy **na pewno** po zamknięciu M5c;
faza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| X-1 ★ | **Dolny ogranicznik ceny schodzi razem z przeceną psującego się towaru** (`W-2` w `M5c`). Podłoga to `unit_cost · (10 000 + min_margin_bp + adj_spoil)` | §5.6 opisywał podłogę bez tego członu, przez co `adj_spoil` nigdy nie mógł zadziałać: −60 % od ceny z marżą 30 % to 104 gr, a podłoga przy marży minimalnej 5 % stoi na 210 gr. Kryterium „przecena psującego się" spełniałoby się tożsamościowo — czwarty raz w projekcie, kiedy kryterium mierzyło co innego niż ścieżka wywołań (po M2e, WP14 M4 i `S-4`). Bramka G3 nie traci przez to nic: dla towaru nieulegającego zepsuciu `adj_spoil == 0` |
| X-2 ★ | **Kolejność kroków doby sklepu jest kontraktem**: odpis → obserwacja → przecena → zaopatrzenie, a domknięcie miesiąca po zaopatrzeniu (`W-13`). Warunek odświeżenia obrazu to `wiek >= delay_days`, nie `>` | Kryterium WP6 żąda reakcji na przecenę konkurenta w **1..=7 dobach**. Odwrotna kolejność `observe`/`reprice` daje 2..=8, a warunek `>` daje 1..=8. Obie pomyłki widać dopiero na rozkładzie ze 100 firm, nie na pojedynczym przypadku — i dlatego test kryterium jest zbudowany tak, jak jest |
| X-3 ★ | **Koszty stałe zakładu weszły do M5c razem z `data/economy/shop.ron`** (`W-10`): czynsz, media, płace i amortyzacja wyposażenia. §5.8 wymienia je wśród zdarzeń księgowych, ale §4 nie przypisywał ich żadnemu pakietowi | To jest przypadek (5) z `K-18` — „pakiet obiecuje coś, czego żaden pakiet nie jest właścicielem". Sklep bez kosztów stałych ma marżę zawyżoną o całą tę pozycję, więc bramki G1–G3 stroiłyby nie ten świat. Sufity są nazwane w danych razem ze ścieżkami wyjścia: czynsz M7/M10, media M8, płace M7 |
| X-4 ★ | **Niezmiennik świata sprawdza się przez `society::total_money`, nigdy przez własną sumę po gospodarstwach.** Scenariusz `m5shop` sumował `Household.{cash, bank, savings}` i pomijał `Population::escheat`, `Population::emigrated` oraz `Wealth` mieszkańca | Gospodarstwo znika ze świata przy zgonie, przy scaleniu po ślubie i przy wyprowadzce, a wszystkie trzy zdarzenia wypadają na **granicy miesiąca**. Przebieg 8-dobowy (M5b) pokazywał więc różnicę 0 gr i wyglądał na zielony; przy 31 dobach ta sama arytmetyka dawała −135 tys. zł i wyglądała jak wyciek pieniądza w M5c, którego nie ma. Wniosek jest ogólniejszy niż ta jedna linia: **niezmiennik pieniądza testuje się na przebiegu dłuższym niż miesiąc gry**, bo krótszy nie uruchamia demografii ani migracji |
| X-5 ★ | **Domyślny promień obserwacji konkurencji to 1 200 m** (`W-14`), a `MatchCompetitor` może zażądać większego i wtedy płaci za niego ten sklep. §7.3 dostało własną linię budżetu dla `observe_competitors` | Pomiar: 161 ms przy 3 000 m wobec 27,4 ms przy 1 200 m dla 2 tys. sklepów. Ścieżka rośnie z **kwadratu** gęstości sklepów w promieniu, więc jest jedyną dobową ścieżką fazy, która skaluje się gorzej niż liniowo — i dlatego ma osobną bramkę benchmarkową, a nie wspólną z `reprice` |
| | **`PurchaseIntent` niesie koszt własny sprzedanego towaru** (`W-4`). `fulfil` zdejmował z półki sztuki i odrzucał zwrócony koszt, a `return_goods` oddawał same sztuki | Bez tego `InventoryGoods` rozjeżdżał się z wyceną zapasu przy każdym nieudanym rozliczeniu, czyli P5 pękał w miejscu, którego M5b nie mógł zobaczyć — księgi jeszcze nie było. Kontrakt `LostSaleTracking` i `ShopLostSales` bez zmian |
| | **Dokument 00 rośnie o `K-30`** (słowniki, które M5 wnosi do `engine/core`: `PriceDriver`) oraz o `shop.ron` w liście katalogów §5 | `UtilityKind` opisuje wymiar oceny **kupującego**, nie człon korekty **sprzedawcy**; wciśnięcie tam `Stock` i `Spoilage` zepsułoby kartę mieszkańca po stronie M3. Ładunek centralnego enuma musi mieszkać w `core` (`K-12`, `K-20`) |


## Zmiany wpisane po M5d

Zgodnie z `K-18`. To są rzeczy, o których wiemy **na pewno** po zamknięciu M5d;
faza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.
Prefiks `AA-n`, bo pojedyncze litery skończyły się na `Z`, `T`, `U` i `X`.

| # | Zmiana | Dlaczego |
|---|---|---|
| AA-1 ★ | **Decyzja otwarta nr 7 zamknięta w szerokiej części zgodnie z propozycją: bank ma `FirmId`, konto i `assess_credit` zamiast AI.** Bez załogi, bez produkcji, bez własnej `Ledger` | W M5 bank robi trzy rzeczy: ocenia wniosek, tworzy depozyt i niszczy go przy spłacie. Plan kont zakładu ma 21 pozycji, z których dla banku niezerowe byłyby dwie — a księga, która nie księguje, jest kosztem bez czytelnika. M7 czyni go pełną firmą, nie ruszając `AccountOwner::Bank(FirmId)` zamrożonego w M5a |
| AA-2 ★ | **Decyzja otwarta nr 16 (konto stacji paliw) zostaje otwarta, mimo że propozycja przypisywała ją M5d.** Powód i pomiar w samym punkcie | Obrót stacji przychodzi z `T-2`, a `T-2` nie jest żadnym z pakietów WP8–WP10; sam punkt żąda przy tym uzgodnienia z M4 **przed** wpięciem konta. Ważniejsze jest to, co pomiar pokazał przy okazji: reszta urosła z +63,2 tys. zł do **+163,0 tys. zł** przez 40 dób, i urosła **dlatego, że przybyło pieniądza** — kanał bez pary przepuszcza proporcjonalnie więcej. To jest błąd mnożnikowy, nie addytywny, więc każda faza dokładająca pieniądza będzie go powiększać |
| AA-3 ★ | **`data/economy/` rośnie o trzy pliki**: `envelopes.ron` (koszty stałe gospodarstwa i wagi kopert per typ), `bank.ron` (ocena zdolności, produkty kredytowe, reguła stopy bazowej), `cpi.ron` (koszyk `q_0`). Dokument 00 §5 i **`K-31`** dopisane w tej samej zmianie | Lista katalogów w 00 §5 deklaruje się jako kompletna, a `K-31` jest wykonaniem `K-8` dla trzech słowników, które M5d wnosi do `core` (`LoanKind`, `RejectCredit`, `FixedCost`) — wszystkie są ładunkami `DecisionReason`, więc nie mogą mieszkać w crate'cie zależnym od `core`. **`cpi.ron` jest jedynym plikiem `data/economy/`, którego balansator nie stroi**: `q_0` jest bazą indeksu i jego zmiana przestawia całą historię CPI |
| AA-4 ★ | **Trzy kontrakty z §6 są dostarczone pod inną sygnaturą, niż zapowiadał plan**: `assess_credit` nie dostaje `&Books` ani `&CreditHistory` (`Y-3`), `cpi(&TransactionStats, …)` zastąpił `CpiTracker` (`Y-4`), a `HouseholdBudget.loans: Vec<LoanId>` to `loan: Option<LoanId>` (`Y-2`). Pozostałe — `plan_budget`, `budget_ref_for_need`, `Envelope`, `Loan`, `LoanKind`, `build_schedule`, `CreditDecision`, `BaseRate` — są takie, jak obiecane | Wszystkie trzy różnice wynikają z tej samej doktryny, którą faza przyjęła przy `kernel` (D20): **rozwiązanie encji i stanu na liczby robi wołający**. `CreditHistory` jako typ nie powstał, bo „zaległości z ostatnich 24 miesięcy" to jeden `u8`; `TransactionStats` nie mógł powstać, bo dziennik jest pierścieniem na dobę, a okno CPI ma trzydzieści. Odbiorcy z §6 (M6, M7, M10) dostają węższe typy, nie inne pojęcia |
| AA-5 ★ | **Nowe w kontrakcie, czego §6 nie wymieniał**: `settle_household_month` + `HouseholdMonth`/`HouseholdMonthReport` (M7 podmieni źródło dochodu, M8 dopisze PIT do kosztów stałych), `Bank`, `LoanBook`, `CpiTracker`, `CpiBasket`, `IndexBp`, `GoodTable::id_of_key`, `Market::{open_bank, budget_of, loan, loan_count, credit_outstanding, cpi_index_bp, cpi_mom_bp, cpi_yoy_bp, base_rate, budget_log}` | §6 wymieniał typy danych, ale nie **wejście**, którym faza następna ich użyje. Miesięczny krok gospodarstwa jest tym wejściem: M7 wstawia w niego pensję emergentną zamiast `income_monthly`, a M8 dopisuje PIT jako czwarty koszt stały. Bez nazwanego punktu wejścia obie fazy napisałyby własną pętlę po gospodarstwach — i wtedy kolejność „rata → wniosek → koszty stałe" (`Y-6`) przestałaby być kontraktem |
| AA-6 | **Blok M5 w `DecisionReason` zajmuje 300–306**; 307–399 zostaje wolne. `StreamId::CreditScoringJitter = 185` wszedł do użycia | Wartości są wieczne (`K-12`, `K-4`), więc warto mieć zapisane, gdzie faza skończyła. Trzy nowe powody: `CreditApproved`, `CreditRejected`, `BudgetShortfall` — ostatni jest ogniwem, bez którego ścieżka „debet → wniosek → odmowa → zaległość" urywa się na odmowie |
| AA-7 | **Balansator (WP13) dostaje metryki gotowe, a nie do wyprowadzenia**: `cpi_index_bp`, `cpi_mom_bp`, `cpi_yoy_bp`, `base_rate`, `loan_count`, `credit_outstanding`, `HouseholdMonthReport` | Bramki G1–G3 mówią wprost o inflacji r/r, m/m i o medianie marży; liczenie ich w balansatorze z surowych transakcji znaczyłoby **drugą implementację CPI**, a ta rozjechałaby się z pierwszą przy pierwszej zmianie koszyka. `mom_bp` i `yoy_bp` zwracają `Option`, więc bramka wie, kiedy nie ma czego mierzyć, zamiast mierzyć zero |
| AA-9 ★ | **Artefakt z §1 nie miał właściciela w części graficznej — WP12 go dostaje i rośnie z `M` do `L`.** Klient `tools/magnat` nie zależy od `magnat-economy` i wstawia do `AgentSources` atrapę `InfinitePlaces` z M3, więc **cała faza M5 jest dziś niewidoczna w oknie z miastem**. WP12 dostaje drugą połowę: zbudowanie `Books`/`Market`, obsadzenie sklepów, `MarketSystem` w harmonogramie i podmianę `Sources.places` (`AB-1` w `M5e`) | §1 obiecuje sesję w GUI, a sprawdzenie w dokumentach pokazało, że **`AgentSources` nie pojawia się ani razu w M5e, M6, M7, M8 i M9**, a `ShopPanelSnapshot` ani razu w M9 i M11 — czyli nikt tej drogi nie budował i nikt na nią nie czekał. Przypadek (5) z `K-18`. Konsekwencja, której nie widać z samego planu: gdyby zostało tak do M9, panel z M5e przez trzy fazy byłby widgetem z testem w CI i bez ani jednego użytkownika, a koszt gospodarki w klatce wyszedłby dopiero pod panelami gracza — najgorszym możliwym momencie |
| AA-8 | **Panel sklepu (WP12) dostaje `Market::budget_log`** — pierścień 256 ostatnich decyzji budżetowych i kredytowych, poza hashem | To jest odpowiedź na „czemu tej rodzinie nie starczyło" — ta sama klasa pytania co „dlaczego Anna nie kupiła u mnie" i ta sama mechanika co `ShopLostSales`: okno podglądu, nie historia. Poza hashem z tego samego powodu co poziom śledzenia (`U-22`): czytanie podglądu nie ma prawa zmieniać świata |
