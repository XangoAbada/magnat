# M6d — Złoża i koniec dostawcy zewnętrznego

Podfaza 4 z 5 fazy **M6 — Łańcuch dostaw** (`M6-lancuch-dostaw.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M6b (zakład, transport), M6c (rynek), M1 (geologia). |
| **Pakiety robocze** | WP10, WP11, WP12 |
| **Projekt techniczny** | §5.10 |
| **Wynik do pokazania** | Paliwo od złoża do baku bez ani jednego punktu, w którym towar bierze się znikąd; sklepy przestają być nieskończone. |
| **Kryterium zamknięcia** | Kryteria WP10–WP12. |
| **Poprzednia / następna** | `M6c-rynek-b2b.md` · `M6e-panel-testy-pamiec.md` |

Wydobycie z wyczerpywaniem złóż, migracja zdejmująca `ExternalSupplier` z M5 oraz magazyny i centra dystrybucyjne.

---

## Pakiety robocze

### WP10 — Wydobycie i wyczerpywanie złóż
**Zależy od:** WP4, M1 (geologia).
**Opis.** `Deposit` (masa pozostała, koncentracja, głębokość), rosnący koszt wydobycia, spadająca koncentracja (najlepsze żyły najpierw), sygnał wyczerpania do M7.
**Kryterium ukończenia:** `prop_deposit_monotone`; złoże wyczerpane w scenariuszu 50-letnim, zakład zamknięty, kaskada niedoboru odpala się w dół łańcucha.
**Rozmiar: S**

### WP11 — Migracja: koniec „zewnętrznego dostawcy" z M5
**Zależy od:** WP2, WP5, WP6. Szczegóły w §6.3.
**Kryterium ukończenia:** feature `infinite_supply` domyślnie wyłączony; test porównawczy przed/po migracji zielony; test zachowania masy włączony globalnie (do M5 sklepowy stub był jawnym wyjątkiem na liście).
**Rozmiar: M**

### WP12 — Magazyny, centra dystrybucyjne, konsolidacja
**Zależy od:** WP5, WP6.
**Opis.** `WarehouseRole::Distribution`, cross-docking, konsolidacja zleceń w trasę milk-run (heurystyka zachłanna Clarke–Wright ograniczona do 12 punktów — nie optymalizacja), koszt t·km i przepustowość rampy jako naturalna nagroda za DC.
**Kryterium ukończenia:** scenariusz „10 sklepów, 1 DC" ma niższy koszt dostawy na tonę niż „10 sklepów, dostawa bezpośrednia od producenta" — bez żadnej reguły to wymuszającej, wyłącznie z kosztów i kolejek.
**Rozmiar: M**

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.10 Wydobycie

**K-13 — `TerrainQuery` z M1 jest jedynym źródłem prawdy o terenie.** M6 **nie trzyma własnej kopii geologii** i nie odejmuje voxeli. Złoże jest własnością M1; M6 konsumuje trzy metody i wnosi jedną rzecz własną — mapowanie `ResourceKind → GoodId`.

```rust
// Kontrakt M1 (K-13). Bilans złoża prowadzony jest w GRAMACH, w arytmetyce całkowitej.
// Voxele są POCHODNĄ WIZUALNĄ — nigdy nie są źródłem prawdy o pozostałej masie.
impl Deposit {
    pub fn extract(&mut self, want: Mass) -> Mass;   // zwraca faktycznie wydobyte (może być < want)
    pub fn remaining(&self) -> Mass;
    pub fn voxels_for(&self, m: Mass) -> VoxelDelta; // do renderu wyrobiska, M1/M11
}

// Własność M6: co znaczy dany surowiec w katalogu towarów.
pub fn good_for(kind: ResourceKind) -> GoodId;       // ResourceKind::Oil -> raw_crude_oil
```

Strona M6 to wyłącznie ekonomia wydobycia — koszt, koncentracja, decyzja o eksploatacji:

```rust
pub struct MiningSite {
    pub site: SiteId, pub deposit: DepositId, pub good: GoodId,
    pub concentration_pct: u8,             // ile masy użytecznej na tonę urobku
    pub depth_m: u16,
    pub daily_capacity: Mass,
}
```

`concentration_pct` i `depth_m` pochodzą z `TerrainQuery` przy założeniu zakładu i są kopiowane raz — to parametry ekonomiczne, nie stan terenu. Stan terenu żyje wyłącznie po stronie M1 i jest odpytywany przez `remaining()`.

Koszt wydobycia, przeliczany `EveryDay`:

```
depletion  = 1000 − 1000*remaining/initial                      // promile wyczerpania
conc_eff   = concentration_pct * cbrt_lut(remaining/initial)    // najlepsze żyły najpierw
cost/tonę  = base * (100 + depth_m/20) / 100 * 100/conc_eff * (1000 + 2*depletion²/1000) / 1000
```

`cbrt_lut` to tablica 64 wartości — deterministyczna i tania, bez floatów w ścieżce kosztowej. **K-6** zakazuje `f64::ln` / `exp` / `powf` ze std w kodzie symulacji; gdyby gdziekolwiek w `sim/supply` potrzebna była funkcja przestępna (krzywa sezonowa, wygładzanie prognozy), pochodzi z `core::det_math`, nigdy ze `std`. W ścieżkach pieniądza, masy i stanów magazynowych i tak obowiązuje arytmetyka całkowita (dok. 00 §2).

Gdy `remaining() == 0`: zakład dostaje `DepositExhausted`, linie przechodzą w `Starved`, a decyzję (zamknięcie, przebranżowienie, bankructwo) podejmuje M7.
