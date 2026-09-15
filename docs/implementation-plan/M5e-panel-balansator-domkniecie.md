# M5e — Panel, balansator, domknięcie

Podfaza 5 z 5 fazy **M5 — Gospodarka detaliczna** (`M5-gospodarka-detaliczna.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M5b–M5d, M3 (`engine/ui`, `tools/magnat`). WP14 rośnie równolegle od M5a — tutaj jest tylko domykany. |
| **Pakiety robocze** | WP12, WP13, WP14 |
| **Projekt techniczny** | §5.11, §5.12 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: **w oknie z miastem** mieszkańcy wchodzą do konkretnych sklepów, kliknięcie w sklep otwiera panel, ceny da się ustawić i zobaczyć skutek; balansator w CI. |
| **Kryterium zamknięcia** | Kryteria WP12–WP14 oraz bramki 1–7 fazy M5 w `00-postep.md`. |
| **Poprzednia / następna** | `M5d-budzety-banki-inflacja.md` · — (ostatnia w fazie) |

Panel sklepu w UI, balansator z bramkami CI oraz komplet testów własnościowych, determinizmu i benchmarków.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP12 | Panel sklepu w UI **i gospodarka w kliencie** | WP3–WP7, M3 (`engine/ui`, `tools/magnat`) | L |
| WP13 | Balansator i bramki CI | WP1–WP10 | L |
| WP14 | Testy własnościowe, determinizm, benchmarki | równolegle od WP1 | M |

### WP12 — Panel sklepu w UI i gospodarka w kliencie
Pakiet ma **dwie połowy** i to jest korekta wpisana po M5d (`AB-1`): sam panel nie pokaże niczego,
dopóki klient graficzny nie uruchamia gospodarki.

**(a) Gospodarka w `tools/magnat`.** Klient robi to, co dziś robi wyłącznie scenariusz `m5shop`:
buduje `EconomyData`, `GoodTable`, `Books` z kontem `RestOfWorld`, `Market`, obsadza sklepy
z zakładów Etapu 7, otwiera bank, rejestruje `Books` i `Market` w haszu stanu, dokłada
`MarketSystem` do harmonogramu i **podmienia `Sources.places` z `InfinitePlaces` na `Market`**
(`Z-1`, `tools/magnat/src/citizens.rs`). Nowe przełączniki w duchu istniejących: `--no-economy`
wraca do zachowania M3.

**(b) Panel.** Trzy zakładki: Półki / Klienci / Konkurencja. Dane wyłącznie przez
`ShopPanelSnapshot` (podwójnie buforowany, bez dostępu UI do ECS symulacji), otwierany
kliknięciem w sklep przez istniejący bufor identyfikatorów z M3d.

Kryterium: `magnat --seed …` pokazuje mieszkańców wchodzących do **konkretnych** sklepów
(licznik transakcji rośnie, półki schodzą), kliknięcie w sklep otwiera panel, a pytanie
„dlaczego Anna nie kupiła u mnie?" ma w nim odpowiedź dla ≥95% mieszkańców, którzy w ostatnich
7 dniach byli kandydatami i nie kupili. Osobno, bo to jest ryzyko, a nie ozdoba: przy `X10`
klatka nie schodzi poniżej budżetu z §7.3 albo `--no-economy` jest udokumentowaną drogą wyjścia.

### WP13 — Balansator i bramki CI
Sekcja 7.4. Kryterium: bramki działają w CI na PR (macierz zredukowana) i nocnie (pełna);
sztucznie wprowadzony błąd (usunięcie dolnego ogranicznika ceny) **czerwieni** bramkę deflacji.

### WP14 — Testy własnościowe, determinizm, benchmarki
Rośnie od WP1, nie na końcu. Dopisanie komponentów `sim/economy` do funkcji haszującej stan ECS
jest warunkiem Definition of Done fazy (dok. 00 §3 pkt 6).

**Uzupełnienie po M4 (`T-6`):** hasza się nie tylko komponenty. **Arena ofert wchodzi do funkcji
haszującej jako osobna sekcja**, w kolejności indeksów — tego wymaga `K-16` wprost, a mechanizm
jest gotowy (`World::register_arena_hash::<T>(ArenaKind)` w `engine/ecs`, `Arena::hash_state`
w `engine/core`). Do dziś **żadna faza nie zarejestrowała ani jednej areny**, więc M5 jest
pierwszym konsumentem tej ścieżki i pierwszym, który się dowie, czy działa.

**Rozstrzygnięte w M5b przez `K-29` i tak zostaje.** `register_arena_hash::<T>(kind)` czyta
`Arena<T>` **stojącą jako osobny zasób świata**, a arena ofert musi mieszkać wewnątrz
`Market`, bo `PlaceProvider` nie dostaje `&World`. Wchodzi więc do hasha przez
`impl HashState for Market` — w kolejności indeksów, ze slotami i generacjami, czyli
co do treści dokładnie tak, jak żąda `K-16`. Zmienia się wyłącznie sekcja: „zasoby wg
nazwy typu" zamiast „areny wg `ArenaKind`". `ArenaKind::Offers` zostaje zarezerwowane
i niezmienne, a M6 (partie) decyduje o swojej drodze sam.

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.11 Systemy ECS i częstotliwości

| System | Częstotliwość | Odczyt | Zapis | Równoległość |
|---|---|---|---|---|
| `offer_index_rebuild` | EveryHour | `Shelf`, pozycje `SiteId` | `OfferIndex` | po komórkach dirty |
| `restock_shelf` | EveryHour | `ShopInventory`, `AssortmentPolicy` | `Shelf`, `Offer.available` | po sklepach |
| `purchase_decision` | zdarzeniowo (DES §17.3) | `OfferIndex`, `Offer`, `Personality`, `ExperienceMemory`, `HouseholdBudget` | bufor `PurchaseIntent` | po chunkach mieszkańców |
| `settle_transactions` | EveryMinute | `PurchaseIntent` | `Books`, `Ledger`, `Shelf`, pamięć agenta | **sekwencyjny**, sort `(Site, Good, arrived, Citizen)` |
| `observe_competitors` | EveryDay (offset `firm.index() % 1440`) | `OfferIndex`, `Offer` | `CompetitorSnapshot` | po firmach |
| `reprice` | EveryDay | `CompetitorSnapshot`, `ShopInventory`, `ObservedElasticity` | `Offer.unit_price`, `PriceController` | po sklepach |
| `price_experiments` | EveryDay | historia sprzedaży | `PriceExperiment`, `ObservedElasticity` | po sklepach |
| `markdown_perishables` | EveryDay | `StockLine.expires` | `Offer.unit_price`, `Ledger` (WriteOff) | po sklepach |
| `replenish_from_wholesale` | EveryDay | `ShopInventory`, `ReorderPolicy` | `Books`, `Ledger`, zamówienia | po sklepach |
| `receive_deliveries` | EveryHour | zamówienia | `ShopInventory`, `Ledger` | po sklepach |
| `household_budget_plan` | EveryMonth | dochody, koszty stałe, `Personality` | `HouseholdBudget` | po GD |
| `loan_servicing` | EveryDay | `Loan` | `Books`, `Ledger`, `MoneySupplyLedger` | sekwencyjny po `LoanId` |
| `bank_underwriting` | zdarzeniowo | `LoanApplication` | `Loan`, `Books` | sekwencyjny |
| `cpi_and_base_rate` | EveryMonth | `TransactionStats` | `CpiIndex`, `BaseRate` | 1 wątek |
| `ledger_close` | EveryMonth | `Ledger.journal` | `PeriodClose` | po sklepach |
| `shop_panel_snapshot` | EveryHour / na żądanie UI | wszystko powyżej (RO) | `ShopPanelSnapshot` | 1 wątek |

Zgodność LOD (dok. 00 §4): `purchase_decision` i `settle_transactions` działają **identycznie**
w mikro i mezo — LOD zmienia tylko sposób, w jaki agent dociera do sklepu (M4), nie cenę,
nie wybór, nie zapis księgowy. Test: ten sam scenariusz w mikro i mezo → identyczne salda, tolerancja 0.

### 5.12 Panel sklepu w UI (§14.3)

Kształt dostarczony (`sim/economy/src/panel.rs`) — różnice wobec pierwotnego szkicu
są wypisane pod blokiem, bo każda ma powód, którego szkic nie mógł znać.

```rust
pub struct ShopPanelSnapshot {
    pub site: SiteId, pub firm: FirmId, pub kind: PlaceKind,
    pub at: Tick,                         // chwila migawki — panel pokazuje, jak stara jest
    pub tracking: LostSaleTracking,
    pub shelves: Vec<ShelfRow>,
    pub customers: CustomerStats,
    pub lost_sales: LostSalesView,
    pub competition: Vec<CompetitorRow>,
    pub finance: FinanceSummary,          // RZiS bieżącego miesiąca + bilans + przepływy + zapas
    pub reprices: Vec<DecisionReason>,    // „dlaczego wczoraj potaniało"
    pub good_keys: Vec<(GoodId, String)>, // nazwy towarów jadą z migawką
}

pub struct ShelfRow { pub good: GoodId, pub price: Money, pub unit_cost: Money, pub margin_bp: i32,
                      pub on_shelf: Qty, pub backroom: Qty, pub days_of_cover: u16,
                      pub turnover_7d: Qty, pub expires_at: Option<SimMinute>,
                      pub policy: PricePolicy, pub delegated: bool }

pub struct CustomerStats { pub by_district: Vec<(DistrictId, u32)>,   // „skąd"
                           pub by_class: Vec<(SocialClass, u32)>,     // „kto"
                           pub by_driver: Vec<(UtilityKind, u32)>,    // „dlaczego" — dominujący człon U
                           pub daily: [u32; 7],                       // spadek liczby klientów po podwyżce
                           pub total: u32 }

pub struct LostSalesView { pub histogram: LostSaleHistogram,   // suma **siedmiu dób**, nie doby
                           pub recent: Vec<LostSale> }         // 256 wpisów, tylko Full

pub struct CompetitorRow { pub site: SiteId, pub distance_m: u32,
                           pub prices: Vec<(GoodId, Money)>, pub observed_age_days: u8 }
```

Cztery różnice wobec szkicu, wszystkie wymuszone przez kryterium, a nie przez wygodę:

1. **`expires_at`, nie `expires_in`.** Migawka niesie chwilę bezwzględną; „za ile" liczy
   interfejs z `at`. Różnica przestaje być kosmetyczna w chwili, gdy panel zostaje
   otwarty przez godzinę: pole „za ile" starzałoby się w kieszeni panelu.
2. **`customers.daily`.** Kryterium z §1 fazy brzmi „po 3 dniach widzi spadek liczby
   klientów", a rozkłady po dzielnicy i klasie są kumulatywne i **spadku nie pokażą**.
   Siedem liczb, `daily[6]` to doba migawki; obrót pierścienia robi migawka, żeby
   interfejs nie musiał znać jego kotwicy.
3. **`lost_sales.histogram` liczy tydzień, nie dobę.** Tak brzmi kryterium WP12, a
   `ShopLostSales.today` prowadził wyłącznie dobę bieżącą.
4. **`good_keys` w migawce.** Bez nich interfejs musiałby trzymać `GoodTable` obok
   i przestałby być odcięty od gospodarki — a pierwsza rozbieżność katalogów wyszłaby
   jako pusta nazwa na ekranie.

`observed_age_days` jest pokazywane wprost — gracz ma widzieć, że patrzy na dane sprzed N dni,
tak samo jak AI. Nakładka mapy cieplnej „zasięg sklepu" (§14.2) rysowana z `customers.by_district`.

---

---

## Zmiany wpisane po M5c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5c —
podfaza nie jest tu przeprojektowywana.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Meta-test bramki G3 jest zmianą jednej linii w `kernel::next_price_full`.** „Usunięcie dolnego ogranicznika ceny musi zaczerwienić bramkę deflacji" ma teraz konkretny adres: `let floor = mul_bp(cost, BP + min_margin_bp + adj_spoil)` | Kryterium WP13 mówiło „sztucznie wprowadzony błąd", nie mówiąc gdzie. Po wydzieleniu rdzenia (D20) ogranicznik jest jedną linią w jednej funkcji bez stanu, więc meta-test da się napisać jako test rdzenia z podmienionym wejściem, a nie jako przebieg z pokiereszowanym kodem produkcyjnym |
| ★ | **`MarketStats` urosło o pięć pozycji, które balansator ma zbierać**: `reprices`, `observations`, `write_offs`, `write_off_value`, `expired_qty`. Lista metryk w §7.4 dokumentu fazy wymienia rozkład cen, CPI, marże, bankructwa, `deferral_rate`, `stockout_rate` i HHI — odpisy towaru przeterminowanego nie były tam wymienione, a są **pierwszym** objawem złej kalibracji zamówień | Zmierzone w scenariuszu `m5shop` (8 dób, 60 sklepów): 230 odpisów za 2,3 mln zł przy obrocie 1,2 mln zł. To nie jest szum — to sygnał, że `ReorderPolicy` zamawia na 8 dni sprzedaży towar o 3-dniowym terminie. Bramka na stosunek `write_off_value` do obrotu jest tańsza niż wypatrzenie tego w rozkładzie marż |
| ★ | **Panel sklepu ma gotowe wejścia i nie potrzebuje nowych struktur po stronie `sim/economy`.** `FinanceSummary` z §5.12 wypełnia się z `Market::{income_statement, balance_sheet, cash_flow}`, `ShelfRow.policy` z `Market::policy_of`, `ShelfRow.unit_cost`/`margin_bp` z `LedgerAccount::{Cogs, Revenue}`, `CompetitorRow.observed_age_days` z `CompetitorEntry.seen_at`, a „dlaczego potaniało" z `Market::reprice_log` | WP12 jest przez to pracą po stronie `engine/ui`, a nie po obu stronach. Jedyne, czego nie ma, to `ShelfRow.turnover_7d` — obrót tygodniowy wymaga okna, którego `PriceController` nie prowadzi (ma dobę bieżącą i poprzednią, na potrzeby eksperymentu) |
| ★ | **Podgląd „co by się stało z ceną dziś" istnieje jako `Market::preview_policy`** i woła **to samo** składanie co `reprice` | To jest połowa WP11, która należała do UI. Osobna arytmetyka podglądu rozjechałaby się z wykonaniem przy pierwszej zmianie wzoru, a gracz zobaczyłby to dopiero po zatwierdzeniu polityki |
| | **`cash_flow` dla zakładu **nieśledzonego** zwraca `complete: false`.** Pierścień dziennika prowadzą wyłącznie zakłady oznaczone (`W-7` w dokumencie M5c) | Panel musi to pokazać, a nie przemilczeć: liczba obcięta oknem dziennika wygląda jak liczba prawdziwa. Zakład gracza jest zawsze śledzony, więc dla niego problem nie występuje — ale karta konkurenta już tak |
| | **Kolejność kroków doby sklepu jest kontraktem** (`W-13`): odpis → obserwacja → przecena → zaopatrzenie, a miesiąc domyka się po zaopatrzeniu. Tabela systemów w §5.11 wymienia je bez kolejności | Przy odwrotnej kolejności `observe`/`reprice` reakcja na przecenę konkurenta mieści się w 2..=8 dobach zamiast 1..=7 i kryterium WP6 pęka **na rozkładzie**, a nie na pojedynczym przypadku. Tabela §5.11 opisuje częstotliwości, więc wystarczy dopisać tam, że te cztery systemy mają ustaloną kolejność wewnątrz doby |
| | **`observe_competitors` dostało budżet w §7.3**: pełne odświeżenie 2 tys. sklepów < 40 ms, zmierzone 27,4 ms przy promieniu 1 200 m. §7.3 tej ścieżki nie miało, bo powstała dopiero w M5c | Bramka benchmarkowa WP14 musi ją mierzyć osobno od `reprice` (1,11 ms) — obie są dobowe, ale rosną z czego innego: `reprice` z liczby sterowników, `observe` z **kwadratu** gęstości sklepów w promieniu |

---

## Zmiany wpisane po M5d

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M5d.

| # | Zmiana | Dlaczego |
|---|---|---|
| AB-1 ★ | **WP12 rośnie o drugą połowę: podpięcie gospodarki do `tools/magnat`, i rośnie z `M` do `L`.** Klient wstawia dziś do `AgentSources` atrapę `InfinitePlaces` z M3 (`citizens.rs:107`), a `tools/magnat/Cargo.toml` nie ma `magnat-economy` wśród zależności — więc cała faza M5 jest niewidoczna w oknie z miastem | **§1 dokumentu fazy obiecuje sesję w GUI** („gracz otwiera sklep osiedlowy przy ulicy X, podnosi cenę mleka o 15 %, po 3 dniach widzi w panelu spadek liczby klientów"), a **żaden pakiet nie był właścicielem drogi, którą gospodarka trafia do klienta**: `AgentSources` nie pojawia się w dokumentach M5e, M6, M7, M8 ani M9, a `ShopPanelSnapshot` nie pojawia się w M9 ani M11. To jest przypadek (5) z `K-18` — ten sam, który M9a złapał u siebie przy `GameState::MainMenu`. Panel bez tej połowy byłby widgetem z testem w CI i bez ani jednego użytkownika |
| AB-2 | **Podpięcie wyciągnie na wierzch koszt, który dziś płaci tylko headless.** `m5shop` liczy dobę miasta 28,5 tys. w ~15 s przy 8 wątkach; przy `X1` doba ma 1440 s realnych, więc mieści się z zapasem, ale przy `X10` już nie | `U-24` mówi wprost, skąd to idzie: **nie z decyzji zakupowej** (`utility_of_offer` 14,7 ns wobec budżetu 120 ns), tylko z routera M4 — każdy zakup to dwa wywołania `begin_trip`. Lepiej, żeby wyszło w M5e z balansatorem i benchmarkami pod ręką, niż w M9 pod panelami. Bramka benchmarkowa WP14 ma to rozdzielać (`U-24`), więc liczba będzie znana, zanim ktoś zobaczy spadek FPS |
| ★ | **WP13 nie liczy CPI ani inflacji sam — czyta je z rynku.** `Market::{cpi_index_bp, cpi_mom_bp, cpi_yoy_bp, base_rate, loan_count, credit_outstanding}` istnieją od M5d, a `cpi_mom_bp`/`cpi_yoy_bp` zwracają `Option<i32>` | Bramki G1–G3 mówią wprost o inflacji r/r i m/m. Policzenie ich w balansatorze z surowych transakcji znaczyłoby **drugą implementację CPI**, a ta rozjeżdża się z pierwszą przy pierwszej zmianie koszyka — to jest ten sam argument, którym D20 wywalczył `kernel`. `Option` ma przy tym znaczenie dla samej bramki: „nie ma jeszcze roku historii" to co innego niż „inflacja zero", a G1 mierzy od 12. miesiąca właśnie dlatego |
| ★ | **Metryki M5d do zbierania per dzień są gotowe i nazwane**: indeks CPI, inflacja m/m i r/r, stopa bazowa, liczba kredytów, niespłacony kapitał oraz `HouseholdMonthReport` (zaplanowane budżety, zapłacone koszty stałe, oszczędności, wnioski i przyznania kredytu, niedopłaty, dopisane zaległości) | §7.4 dokumentu fazy wymienia „podaż pieniądza, sumę kredytów, stopę bazową" wśród metryk balansatora, ale nie mówił, skąd je wziąć. Teraz mówi: `HouseholdMonthReport` wraca z `settle_household_month` raz na miesiąc gry, a reszta jest odczytem z `Market` |
| | **WP12 dostaje `Market::budget_log`** — pierścień 256 ostatnich decyzji budżetowych i kredytowych `(indeks gospodarstwa, DecisionReason)`, poza hashem stanu | To jest odpowiedź na „czemu tej rodzinie nie starczyło" i ta sama mechanika co `ShopLostSales`: okno podglądu, nie historia. Trzy nowe powody (`CreditApproved`, `CreditRejected`, `BudgetShortfall`) mają już teksty w `pl.ron` i `en.ron` oraz ramiona w `engine/ui::describe`, więc panel ma co renderować bez dokładania ani jednego klucza |
| | **Scenariusz `m5shop` wypisuje sekcję „budżety, banki, inflacja"** i liczy niezmiennik świata **po odjęciu kreacji kredytowej netto** (`AA-2`, `Y-8`) | WP14 buduje na tym złoty plik hashy fazy. Warto wiedzieć, że różnica sumy świata nie jest już zerem z definicji: kredyt tworzy pieniądz, a rejestry ruchu M4 dalej nie mają kont (decyzja otwarta nr 16) |

---

## Korekty wpisane w trakcie M5e

Tabela korekt podfazy. Gwiazdka = zmiana zakresu albo kryterium.

| # | Korekta | Dlaczego |
|---|---|---|
| AD-1 ★ | **Kryterium WP12 spełniało się tożsamościowo i to była największa pomyłka tej podfazy.** „Odpowiedź dla ≥ 95 % mieszkańców, którzy w ostatnich 7 dniach byli kandydatami i nie kupili" mierzyło zbiór, w którym licznik **był** mianownikiem: pierścień `ShopLostSales` zapisywał wyłącznie wizyty, które doszły do sklepu i tam się nie udały, a mieszkaniec, który porównał ceny i poszedł do konkurenta, nie zostawiał śladu. Naprawione w `candidates`: każdy kandydat **przegrany** w sklepie śledzonym dostaje wpis z powodem i z `went_to` | Scena z §1 dokumentu fazy („cena o 12 % wyższa niż w *Dobry Koszyk*, 700 m dalej") jest **dokładnie tym przypadkiem**, więc kryterium omijało to, co obiecuje artefakt. Przy okazji pole `LostSale.went_to` przestało być zawsze `None` — istniało od M5b i nikt go nie wypełniał. Test `kazdy_kto_rozwazyl_sklep_i_nie_kupil_ma_powod` liczy teraz oba zbiory osobno: 400 wyborów, mianownik z przebiegu, próg 950 ‰ |
| AD-2 ★ | **Odpisy towaru przeterminowanego wynosiły 15,8-krotność obrotu.** Balansator zgłosił to pierwszym przebiegiem. Trzy przyczyny, wszystkie naprawione: (a) zapas startowy brał **pełny** cel polityki zamiast jednego wyłożenia, (b) sklep dojrzały zamawiał towar, którego u niego nikt nie kupuje, (c) pusta linia zapasu dziedziczyła termin ważności towaru, którego już nie ma, więc świeża dostawa szła na odpis w dniu przyjęcia. Po naprawie: **1,8-krotność**, a marża brutto śledzonego sklepu przeszła z −181 zł na +13 530 zł | Korekta ★ po M5c przewidziała objaw („`ReorderPolicy` zamawia na 8 dni sprzedaży towar o 3-dniowym terminie") i przewidziała go **za słabo** — prawdziwy mnożnik był o rząd wielkości większy. To jest zarazem odpowiedź na pytanie, po co balansator w ogóle powstaje: żadna z tych trzech rzeczy nie łamała ani jednego testu jednostkowego i żadna nie była widoczna w przebiegu ośmiodobowym |
| AD-3 ★ | **Wiązanie celu zamówienia z popytem odwraca mechanizm inflacji emergentnej z WP10 — i dlatego nie zostało wprowadzone dla sklepów sprzedających.** Zmierzone: czterokrotna akcja kredytowa dawała CPI 8 239 zamiast 9 793 wobec bazy 10 000, czyli **obniżała** ceny | Presja zapasu jest w M5 **jedynym** kanałem, którym pieniądz dochodzi do cen: dostawca zewnętrzny ma nieskończoną podaż po stałej cenie, więc sklep, który zawsze domawia do celu, nigdy nie podnosi ceny. Sterownik zapasu z kosztem braku należy do M6 razem z realnym dostawcą — wtedy będzie też **czym** podnieść cenę hurtową. Granica poprawki biegnie więc po „sklep, który nie sprzedaje, nie zamawia", i tylko po niej |
| AD-4 ★ | **Bramka G9 mierzyła transakcje, a PRD §14.1 mówi o decyzjach.** 21 821 wpisów „bez powodu" na 295 385 to były czynsz, media, płace, rata kredytu, wypłata dochodu i kapitał założycielski — **zobowiązania wykonane, nie wybrane**. Bramka liczy teraz wyłącznie rodzaje transakcji, przy których ktoś wybierał, a lista jest wyczerpującym `match` bez gałęzi `_` | Dwie naprawy po stronie `sim/economy`, bo dwie ścieżki **były** decyzjami i nie miały powodu: zamówienie u dostawcy (`StockBelowThreshold` — ta sama reguła co po stronie gospodarstwa) i uruchomienie kredytu (`Books::create_credit` dostało parametr `reason`). Po obu: **0 na 282 099**. Milcząca gałąź `_ => false` zrobiłaby z G9 bramkę, która przestaje mierzyć przy pierwszej nowej ścieżce pieniądza |
| AD-5 ★ | **`stockout_rate` z PRD §20.1 to odsetek odmów „brak towaru", nie odsetek pustych półek.** G5 czerwieniło się na 298 ‰ przy progu 150 ‰, licząc oferty z pustą półką | Różnica przestała być akademicka razem z `AD-2`: od tej podfazy dojrzały sklep **świadomie** nie zamawia towaru, którego nikt u niego nie kupuje, a jego oferta zostaje widoczna z zerowym stanem („znam, nie ma" — §5.3). Bramka liczona po półkach czerwieniłaby się na **decyzji asortymentowej**, czyli na zdrowym zachowaniu rynku. Metryka po półkach zostaje w raporcie jako `stockout_permille`, bo mówi coś prawdziwego — tylko nie to, o co pyta G5 |
| AD-6 ★ | **Profil `ci` nie mieści się w 10 minutach przy 8 ziarnach × 365 dób i CI bierze 4 × 120.** Zmierzone: doba miasta 3 tys. mieszkańców to ~1,6 s, czyli 365 dób to ~9,7 min **na ziarno**. Pełną macierz puszcza bieg nocny | Źródło kosztu jest nazwane w `U-24` i nie leży w gospodarce: `utility_of_offer` mierzy 15 ns, cała decyzja zakupowa 0,59 µs, a każdy zakup to **dwa wywołania routera M4**. Lepiej mieć bramkę, która biegnie przy każdym pull requeście, niż zgodną z §7.4 liczbę, którą ktoś wyłączy po trzecim przekroczeniu limitu |
| AD-7 | **Panel sklepu otwiera się raycastem w teren, nie buforem identyfikatorów.** Kryterium WP12 mówiło „przez istniejący bufor identyfikatorów z M3d" — bufor istnieje, ale niesie **wyłącznie pieszych** (korekta H-15 do M3d) | Przypadek (2) z `K-18`: API, którego kryterium używa w opisie, nie robi tego, co kryterium zakłada. Sufit jest nazwany w kodzie klienta: dwa sklepy bliżej siebie niż 25 m są nierozróżnialne kliknięciem. Zgłoszone do M11 jako zadanie dla warstwy rysującej, bo bufor identyfikatorów należy do niej |
| AD-8 | **`m5shop --days` domyślnie 6, nie 2.** Przy dwóch dobach gospodarstwo nie schodzi z zapasu startowego (`purchase_days = 4`), więc scenariusz kończył się **własną** bramką „przez 2 dób nikt nic nie kupił" przy domyślnym wywołaniu | Ta sama klasa błędu co kryterium spełnione tożsamościowo, tylko odwrotna: domyślne wywołanie mierzyło pustą pętlę i zgłaszało to jako porażkę. Bramka CI determinizmu M5 bierze sześć dób z tego samego powodu |
| AD-9 | **`Market::set_price` ustawia teraz także sterownik ceny.** Wcześniej zmieniał wyłącznie ofertę, więc panel (czytający sterownik) pokazywał inną cenę niż ta, którą płacił klient | Złapane przez test `migawka_panelu_pokazuje_to_samo_co_rynek`, i to jest cały powód, dla którego ten test porównuje migawkę z akcesorami rynku zamiast sprawdzać, że pola są niepuste. Migawka bierze cenę **z oferty** (`K-7`: oferta jest jedynym nośnikiem ceny), a sterownik niesie politykę i obrót |
| AD-10 | **CI nie uruchamiało się na `push`**: wyzwalacz stał na gałęzi `main`, a gałąź główna nazywa się `master` (CLAUDE.md). Bramki działały wyłącznie na pull requestach, których w tym projekcie nie ma | Znalezione przy dokładaniu jobu balansatora. Poprawka jednoliniowa, ale konsekwencja była taka, że **żaden** commit od M0 nie przeszedł przez CI inaczej niż lokalnie |
