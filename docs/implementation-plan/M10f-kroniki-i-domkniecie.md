# M10f — Kroniki i domknięcie

Podfaza 6 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M10a–M10e. |
| **Pakiety robocze** | WP10.15, WP10.16 |
| **Projekt techniczny** | §5.10 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: marka z pamięci agentów, R&D, giełda, historia „na sucho”. |
| **Kryterium zamknięcia** | Kryteria WP10.15 i WP10.16 oraz bramki 1–7 fazy M10 w `00-postep.md`. |
| **Poprzednia / następna** | `M10e-relacje-i-zwiazki.md` · — (ostatnia w fazie) |

Kroniki i wyjaśnialność dla wszystkich nowych mechanik oraz dopisanie stanu M10 do funkcji haszującej.

---

## Pakiety robocze

### WP10.15 — Kroniki i wyjaśnialność

**Zależności:** M9 (podsystem kroniki), wszystkie WP tej fazy.
Przekrojowy. Każda nowa decyzja ma `DecisionReason` (dok. 00 §7). Nowe warianty `ChronicleEvent` z §5.9.
**Kryterium ukończenia:** 100% nowych decyzji ma czytelny powód w karcie inspekcji; audyt ręczny
20 losowych wpisów kronikarskich pod kątem zrozumiałości dla gracza.
**Rozmiar: S.**

---

### WP10.16 — Determinizm i hash stanu

Przekrojowy, Definition of Done fazy. Nowe warianty `StreamId`, dopisanie nowych komponentów do
funkcji haszującej, testy dwóch przebiegów. **Rozmiar: S.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.10 Systemy ECS i ich częstotliwość

| System | Częstotliwość | LOD | Odczyt/zapis |
|---|---|---|---|
| `brand_decay_compact` | `EveryMonth` | mezo | W: `BrandSlots` |
| `ad_expose_billboard` | `EveryHour` | mezo | R: przejazdy M4; W: `BrandSlots`, `CampaignMetrics` |
| `ad_expose_media` | `EveryDay` | mezo | R: `MediaOutlet`; W: `BrandSlots` |
| `ad_expose_leaflet` | `EveryDay` | mezo | R: indeks przestrzenny M2; W: `BrandSlots` |
| `ad_budget_burn` | `EveryDay` | — | W: `Ledger` przez `ledger_post` |
| `brand_strength_aggregate` | `EveryDay` | — | R: `BrandSlots`; W: agregat UI |
| `media_editorial` | `EveryDay` | — | R: `sim/events`; W: `Rumor` |
| `rnd_progress` | `EveryDay` | — | R: badacze, budżet; W: `ResearchProject` |
| `rnd_unlock` | `EveryDay` | — | W: `Patent`, efekty technologii |
| `epoch_advance` | `EveryMonth` | — | W: `EpochState`, koszyki potrzeb |
| `investor_decide_funds` | `EveryWeek` | — | W: `Order` |
| `investor_decide_citizens` | `EveryMonth` | — | W: `Order` |
| `stock_fixing` | `EveryDay` (17:00) | — | R/W: `OrderBook`, `Holding`, `Share` |
| `earnings_publish` | `EveryDay` (sprawdza harmonogram T+45) | — | W: `PublishedReport` |
| `takeover_check` | `EveryDay` | — | W: `FirmPersonality` (M7), `Rumor` |
| `dividend_pay` | `EveryMonth` | — | W: `Ledger` |
| `insurance_underwrite` | `EveryMonth` | — | R: `PerilStats`; W: `InsurancePolicy` |
| `insurance_claims` | `EveryDay` | — | R: `sim/events`; W: `Ledger`, `PerilStats` |
| `supplier_trust_update` | `EveryDay` | mezo+makro | W: `SupplierRelation` |
| `cartel_detect` | `EveryMonth` | — | W: kary, `BrandSlots`, kronika |
| `union_grievance` | `EveryDay` | — | R: księgi M7, graf relacji M3 |
| `union_negotiate` | `EveryWeek` | — | W: `Union`, płace |
| `strike_tick` | `EveryDay` | — | W: produkcja zakładu, `Ledger` |
| `macro_step` | `EveryDay` (tylko w trybie makro) | makro | R/W: `MacroState` |

---
