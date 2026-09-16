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

---

## Zmiany wpisane po R1

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po podziale
`sim/world/src/city/sites.rs` w pakiecie R-WP5 dokumentu `R1-refaktor-po-M5.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`supply_closure_check` nie jest już fragmentem cudzego pliku — jest plikiem.** Adres to `sim/world/src/city/sites/closure.rs` (360 linii kodu + 141 testów) i mieszka w nim **całe** domknięcie: `supply_closure_check` (KROKI 1–5), `zgas_nadmiarowe`, `bilans`, `wiodace_dla`, `wiodace_wyjscia`, `pusty`, `ClosureReport` oraz `brama_t_na_dobe` — budżet importu przez bramy, dziś stały. **To jest dosłownie punkt, w którym WP11 wchodzi.** Plik ma od R-WP5 trzy testy bilansu masy, które nie potrzebują świata (dwutowarowy katalog: 500 g mąki → 400 g chleba + 100 g ubytku) | WP11 („koniec zewnętrznego dostawcy") zakładał przejęcie funkcji rozsianej po pliku o 1862 liniach. Po R-WP5 przejmuje jeden plik, a testy bilansu są bramką, która powie, czy zamiana stałego budżetu bram na dynamiczny nie zgubiła masy — wcześniej takiej bramki nie było **nigdzie**: `consistency.rs` (T10/T11) tylko ogląda `report.closure` po pełnej generacji, więc nie odróżni pomylonych wejść z wyjściami od poprawnych, bo oba przebiegi pomyliłyby je tak samo |
| ★ | **Strona popytu została po drugiej stronie szwu i WP11 musi o tym wiedzieć.** `popyt_bazowy` (koszyk + `consumes` archetypów) i `zapotrzebowanie_zakladow` (propagacja wstecz po hipergrafie receptur, dwanaście rund — to ona wyznacza, ile zakładów w ogóle powstaje) są w `sites/place.rs`, nie w `closure.rs`. `populate` woła `supply_closure_check` dopiero w kroku 6, z popytem policzonym na komplecie przydziałów | Koniec zewnętrznego dostawcy zmienia bilans z **jednorazowego w ciągły**, a dziś popyt jest liczony raz, w generatorze miasta. Granica „popyt w generatorze / domknięcie w `closure.rs`" jest miejscem, w którym WP11 będzie musiał podjąć decyzję, i lepiej, żeby była widoczna teraz niż odkryta w trakcie |
| | **Co jeszcze WP11 rusza poza `closure.rs`:** `sites/mod.rs` (`SiteSeed.capacity_scale` i `SiteSeed.recipes` są mutowane przez KROK 5 i przez gaszenie; `SiteSet.closure` trzyma raport), `sites/data.rs` (`RATIO_MIN`/`RATIO_MAX` — warunek akceptacji T11; `SCALE_MIN`/`SCALE_MAX`/`SCALE_BASE` — widełki KROKU 5), `city/mod.rs` (`GenerationReport.closure` i jego wydruk), `city/catalog.rs` (`Catalog::reachable`, `Good::has_external_price`, `Good::gates`) oraz `sim/world/tests/consistency.rs` (T10/T11) | Lista jest kompletna wobec stanu po R-WP5 i sprawdzona grepem po całym workspace. Wpisana, bo „przejmij `supply_closure_check`" brzmi jak jedna zmiana, a jest sześcioma plikami |
| | **Kod martwy do usunięcia przy okazji WP11, nie wcześniej:** w `supply_closure_check` `let mut import = vec![0i64; n];` stoi **dwa razy** (`closure.rs` linie 92 i 138). Pierwszy jest zapisywany w KROKU 4a (`import[g] = import[g].max(1)` dla towaru nieosiągalnego, ale importowalnego) i zaraz przesłonięty przez drugi, więc ten zapis nie dochodzi nigdzie. Zachowanie dzisiejsze jest poprawne — realny przywóz liczy blok po pętli, od zera — ale KROK 4a jest kodem martwym, a nie regułą | R1 §2 zakazuje zmiany zachowania, więc R-WP5 tego nie ruszył. WP11 i tak przepisuje tę pętlę na budżet dynamiczny; usunięcie martwego zapisu razem z nią kosztuje zero, a osobno kosztowałoby własny dowód, że nic od niego nie zależy |

---

## Zmiany wpisane po M6a

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AF-1 ★ | **Adres `Catalog::reachable`, `Good::has_external_price` i `Good::gates` zmienił się z `sim/world/src/city/catalog.rs` na `sim/supply/src/catalog/`.** W `sim/world` zostaje re-eksport, więc wołania z `closure.rs` nie drgnęły — ale plik, który WP11 ma przepisywać, jest teraz gdzie indziej | Katalogu potrzebuje `sim/economy` (od WP11 półka bierze towar stąd), a `sim/economy` nie zależy i nie może zależeć od `sim/world`. Lista „co jeszcze WP11 rusza" z tabeli po R1 jest w tym jednym wierszu nieaktualna i ten wiersz ją poprawia |
| AF-2 ★ | **`Good::pack` (`RetailPack`) nie istnieje — projektuje go WP11.** M6a pola nie dodał, bo §5.1 nie definiuje go ani jednym zdaniem, a wie, czego potrzebuje, dopiero ten, kto łączy półkę z magazynem | Pole dodane „na zapas" o zgadniętym kształcie kosztuje migrację ~60 plików danych, kiedy okaże się, że M5 potrzebuje czegoś innego. Dodanie go w WP11, razem z pierwszym konsumentem, kosztuje jeden przebieg po danych i ani jednej decyzji podjętej w ciemno (`AD-4` w dokumencie fazy) |
| AF-3 | **Klucze towarów są inne niż w M5.** `bread` to teraz `food_bread_wheat`, `flour` to `food_flour_t550`, `crude_oil` to `raw_crude_oil` (`AD-12`). `data/economy/retail.ron`, `data/locale/{pl,en}.ron` i literały w kodzie zostały przemianowane razem z katalogiem | WP11 łączy `GoodTable` M5 z katalogiem M6 i będzie porównywać klucze. Rozjazd nazw ujawniłby się jako **cicho pominięty towar** — `GoodTable::build` pomija klucze, których nie umie rozwiązać, i nie jest to błąd ładowania |
| AF-4 | **`Store::shelf_pick` i `backroom_to_shelf` z §6.1 jeszcze nie istnieją, ale stoi pod nie wszystko:** `WarehouseRole::{Backroom, Shelf}`, FEFO w slocie, `Store::take` zwracające `BatchSlice { mass, quality, cost_total, brand, expires_at }` i `Store::spoil` księgujące `LossKind::Expired` | `SoldUnits` z §6.3 kroku 5 to `BatchSlice` pod inną nazwą. WP11 ma je opakować, a nie zaprojektować od nowa — i ma sprawdzić, czy `trait Wholesale` z M5 nie potrzebuje rozszerzenia o jakość i markę (§6.4.4 przewidywał, że będzie potrzebować; `BatchSlice` to pokrywa) |
