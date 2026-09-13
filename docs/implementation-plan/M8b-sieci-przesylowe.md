# M8b — Sieci przesyłowe

Podfaza 2 z 5 fazy **M8 — Miasto jako aktor** (`M8-miasto-jako-aktor.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M8a (taryfy jako polityka), M2 (parcele, dzielnice), M7 (operator jako firma). |
| **Pakiety robocze** | WP3 |
| **Projekt techniczny** | §5.4 |
| **Wynik do pokazania** | Test T2: blackout zatrzymuje produkcję i jest widoczny w kosztach. |
| **Kryterium zamknięcia** | Kryterium WP3; benchmark solvera w budżecie z §5.4. |
| **Poprzednia / następna** | `M8a-pieniadz-publiczny.md` · `M8c-zdarzenia.md` |

Pięć rodzajów sieci na jednym solverze przepływu: przeciążenia, wyspy, zrzut obciążenia wg priorytetu, kaskada wyłączeń, pomiar i faktura.

---

## Pakiety robocze

### WP3 — Sieci przesyłowe w `sim/traffic`
**Zależy od:** WP1 (taryfy jako polityka), M2 (parcele, dzielnice), M7 (operator jako firma).
Graf sieci, solver przepływu z ograniczeniami, wykrywanie przeciążeń i wysp, zrzut obciążenia
wg priorytetu, kaskada wyłączeń z limitem rund, `SupplyState` per przyłącze. Pomiar zużycia,
taryfa, faktura miesięczna jako zwykła transakcja B2B/B2C. Pięć rodzajów sieci na jednym solverze.
**Kryterium ukończenia:** test T2 (blackout zatrzymuje produkcję i jest widoczny w kosztach);
benchmark `criterion` mieści się w budżecie z sekcji 5.4.
**Rozmiar:** L

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 Sieci przesyłowe (`sim/traffic`)

```rust
pub enum UtilityKind { Power, Gas, Water, Sewage, Heat, Telecom }

pub struct UtilityNetwork {
    pub kind: UtilityKind,
    pub operator: FirmId,                 // sieć JEST firmą (§9.6) — z taryfą i księgowością
    pub nodes: Vec<UtilityNode>,
    pub edges: Vec<UtilityEdge>,
    pub topo: TopologyCache,              // przebudowa tylko przy zmianie topologii
    pub tariff: Tariff,
}

pub struct UtilityNode {
    pub site: Option<SiteId>,
    pub role: NodeRole,                   // Source{capacity, min_stable, online}
                                          // | Transformer{capacity} | Connection{priority}
    pub demand: i64,                      // W / ml·h⁻¹ / Wh·h⁻¹ / Mb·s⁻¹ — jednostki całkowite
    pub supplied: i64,
    pub state: SupplyState,               // Ok | Shed | Isolated | Faulted
    pub metered: i64,                     // licznik narastająco do faktury
}

pub struct UtilityEdge {
    pub a: u32, pub b: u32,
    pub capacity: i64, pub loss_bps: u32,
    pub state: EdgeState,                 // Ok | Tripped{until} | UnderMaintenance
}

pub struct TopologyCache {                // budowany przy TopologyDirty, nie co tick
    pub islands: UnionFind,
    pub spanning_forest: Vec<u32>,        // rodzic każdego węzła
    pub post_order: Vec<u32>,             // kolejność sumowania poddrzew
    pub loop_groups: Vec<LoopGroup>,      // składowe 2-spójne skolapsowane do superwęzłów
}

pub struct Tariff {
    pub standing_charge_per_month: Money,
    pub per_unit: Money,                  // za kWh / m³ / GJ / GB — cena za 1000 jednostek bazowych
    pub peak_multiplier_bps: u32,
    pub connection_fee: Money,
}
```

**Model przepływu — wystarczający do blackoutu, nie więcej.**

```rust
pub fn solve_network(net: &mut UtilityNetwork, rng: RngKey) -> SolveReport;

pub struct SolveReport {
    pub shed_nodes: Vec<u32>, pub tripped_edges: Vec<u32>,
    pub unserved: i64, pub cascade_rounds: u8,
}
```

Algorytm, jedna runda:
1. **Wyspy.** Union-find z cache; przelicz tylko gdy `TopologyDirty`.
2. **Bilans wyspy.** `supply = Σ źródeł online`, `demand = Σ węzłów odbiorczych`
   (strata na krawędziach doliczana jako `demand * loss_bps`).
3. **Zrzut obciążenia.** Gdy `demand > supply`: odłączaj węzły w porządku
   `(priority, node_index)` — rosnąco po priorytecie — aż do bilansu. Szpital i wodociąg
   mają priorytet 0, gospodarstwa domowe 2, przemysł 3. Porządek jest w pełni deterministyczny.
4. **Przepływy na krawędziach.** Na drzewie rozpinającym: przepływ krawędzi = suma popytu
   w jej poddrzewie, liczona jednym przejściem po `post_order` — **O(E)**.
   Pętle (składowe 2-spójne) kolapsowane do superwęzła o przepustowości = suma jego krawędzi.
5. **Zadziałanie zabezpieczeń.** `flow > capacity` → krawędź `Tripped` (czas naprawy
   losowany ze `StreamId::GridFault`). To zmienia topologię → **runda kolejna** = kaskada.
6. **Limit 8 rund/tick.** Po wyczerpaniu: reszta niezbilansowanych węzłów → `Shed`.
   Limit chroni przed pętlą i daje twardy górny koszt.

> `ponytail:` przepływ po drzewie rozpinającym, pętle jako superwęzły. Sufit: nie modeluje
> rozpływu mocy w oczku ani jałowej. Ścieżka wyjścia, gdyby przeciążenia okazały się
> nierealistyczne: liniowy rozpływ DC (macierz B, rozkład LU cache'owany na topologię) —
> ten sam interfejs `solve_network`, dziesięciokrotnie droższy.

**Częstotliwość i koszt.**

| Sieć | Tick | Uzasadnienie |
|---|---|---|
| Power | `EveryMinute` | blackout musi być ostry; bufora nie ma |
| Heat, Gas | `EveryHour` | bufor w rurociągu i bezwładność cieplna budynku |
| Water, Sewage | `EveryHour` | zbiorniki wyrównawcze |
| Telecom | `EveryHour` | brak przepustowości = degradacja, nie zatrzymanie |

Budżet wydajności dla metropolii 400 tys. (cel z §17.7): ~5000 węzłów i ~6000 krawędzi
w sieci energetycznej. Runda = O(N+E) ≈ 11 tys. operacji całkowitych.
Typowo 1 runda, w kaskadzie ≤ 8. **Cel: < 0,3 ms na tick dla wszystkich pięciu sieci łącznie**,
mierzony `criterion`. Union-find i drzewo przebudowywane wyłącznie przy zmianie topologii
(nowe przyłącze, awaria) — kilka razy na dobę gry, nie 1440 razy.

**Skutek dla zakładu (kontrakt dla M7):**

```rust
pub fn supply_state(site: SiteId, kind: UtilityKind) -> SupplyState;
pub fn power_available(site: SiteId) -> bool;     // skrót — najczęstsze pytanie
```

Zakład bez prądu: linie produkcyjne stają (`SiteHalted { reason: NoUtility(Power) }`),
koszty stałe biegną dalej, płace biegną dalej, chłodnia przestaje chłodzić (M6: psucie),
kasa w sklepie nie działa. **To M7 decyduje, co firma z tym zrobi** — M8 tylko wystawia stan.

**Rozliczenie mediów:** `metered` narasta co tick; `EveryMonth` operator wystawia fakturę
jako zwykłą transakcję (M5), z pełnym śladem w księdze odbiorcy. Miasto może ograniczyć
taryfę przez `Policy::TariffCap` — z emergentnym skutkiem (operator tnie konserwację →
rośnie sonda `MaintenanceBacklogDays` → rośnie hazard awarii; to pętla, nie skrypt).
