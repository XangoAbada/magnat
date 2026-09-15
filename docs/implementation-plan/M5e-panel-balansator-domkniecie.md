# M5e — Panel, balansator, domknięcie

Podfaza 5 z 5 fazy **M5 — Gospodarka detaliczna** (`M5-gospodarka-detaliczna.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M5b–M5d, M3 (`engine/ui`). WP14 rośnie równolegle od M5a — tutaj jest tylko domykany. |
| **Pakiety robocze** | WP12, WP13, WP14 |
| **Projekt techniczny** | §5.11, §5.12 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: otwórz sklep, ustal ceny, obserwuj klientów; balansator w CI. |
| **Kryterium zamknięcia** | Kryteria WP12–WP14 oraz bramki 1–7 fazy M5 w `00-postep.md`. |
| **Poprzednia / następna** | `M5d-budzety-banki-inflacja.md` · — (ostatnia w fazie) |

Panel sklepu w UI, balansator z bramkami CI oraz komplet testów własnościowych, determinizmu i benchmarków.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP12 | Panel sklepu w UI | WP3–WP7, M3 (`engine/ui`) | M |
| WP13 | Balansator i bramki CI | WP1–WP10 | L |
| WP14 | Testy własnościowe, determinizm, benchmarki | równolegle od WP1 | M |

### WP12 — Panel sklepu w UI
Trzy zakładki: Półki / Klienci / Konkurencja. Dane wyłącznie przez `ShopPanelSnapshot` (podwójnie
buforowany, bez dostępu UI do ECS symulacji).
Kryterium: pytanie „dlaczego Anna nie kupiła u mnie?" ma odpowiedź w panelu dla ≥95% mieszkańców,
którzy w ostatnich 7 dniach byli kandydatami i nie kupili.

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

```rust
pub struct ShopPanelSnapshot {
    pub shelves: Vec<ShelfRow>,
    pub customers: CustomerStats,
    pub lost_sales: LostSalesView,
    pub competition: Vec<CompetitorRow>,
    pub finance: FinanceSummary,          // RZiS bieżącego miesiąca + przepływy + stan zapasów
}

pub struct ShelfRow { pub good: GoodId, pub price: Money, pub unit_cost: Money, pub margin_bp: i32,
                      pub on_shelf: Qty, pub backroom: Qty, pub days_of_cover: u16,
                      pub turnover_7d: Qty, pub expires_in: Option<SimMinute>, pub policy: PricePolicy }

pub struct CustomerStats { pub by_district: Vec<(DistrictId, u32)>,   // „skąd"
                           pub by_class: Vec<(SocialClass, u32)>,     // „kto"
                           pub by_driver: Vec<(UtilityTerm, u32)> }   // „dlaczego" — dominujący człon U

pub struct LostSalesView { pub histogram: LostSaleHistogram,   // zawsze dla zakładów gracza
                           pub recent: Vec<LostSale> }         // 256 wpisów, tylko Full

pub struct CompetitorRow { pub site: SiteId, pub distance_m: u32,
                           pub prices: Vec<(GoodId, Money)>, pub observed_age_days: u8 }
```

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
