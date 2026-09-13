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
