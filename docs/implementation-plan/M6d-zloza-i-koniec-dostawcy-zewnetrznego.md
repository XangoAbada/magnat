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

### WP10 — Wydobycie i wyczerpywanie złóż `[x]`
**Zależy od:** WP4, M1 (geologia).
**Opis.** `Deposit` (masa pozostała, koncentracja, głębokość), rosnący koszt wydobycia, spadająca koncentracja (najlepsze żyły najpierw), sygnał wyczerpania do M7.
**Kryterium ukończenia:** `prop_deposit_monotone`; złoże wyczerpane w scenariuszu 50-letnim, zakład zamknięty, kaskada niedoboru odpala się w dół łańcucha.
**Rozmiar: S**

### WP11 — Migracja: koniec „zewnętrznego dostawcy" z M5 `[x]`
**Zależy od:** WP2, WP5, WP6. Szczegóły w §6.3.
**Kryterium ukończenia:** feature `infinite_supply` domyślnie wyłączony; test porównawczy przed/po migracji zielony; test zachowania masy włączony globalnie (do M5 sklepowy stub był jawnym wyjątkiem na liście).
**Rozmiar: M**

### WP12 — Magazyny, centra dystrybucyjne, konsolidacja `[x]`
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

---

## Zmiany wpisane po M6b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6b.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AG-1 ★ | **Podłączenie prawdziwego routera M4 należy do tej podfazy, a nie do M6b.** M6b zastał po stronie M4 **wyłącznie ruch pasażerski**: nie ma `traffic::dispatch(order)`, nie ma `VehicleArrivedAtSite`, nie ma ciężarówki w `data/vehicles/classes.ron` (pięć klas: trzy osobowe, dostawczak, autobus), nie ma `BodyType`, nie ma `Licence::C` ani `Licence::ADR`. Gniazdo stoi gotowe: `trait FreightOracle` w `magnat_supply::transport`, jedna metoda `quote(from, to, mass, req) -> Option<FreightQuote>`, atrapa `FlatRateFreight` w crate'cie | Wzorzec jest ten sam co `TravelOracle` (`Z-1`): definicja i atrapa w crate'cie, który pyta, implementacja produkcyjna w tym, który umie odpowiedzieć, wpięcie w tym, który składa świat. **Co trzeba dołożyć po stronie M4, zanim wtyczka wejdzie:** klasy ciężarówek w `data/vehicles/classes.ron` (kolejność wpisów jest kontraktem zapisu gry, `K-24` — dopisywać wyłącznie na końcu), ładowność i nadwozie w `VehicleClassSpec` (dziś ma masę własną, zbiornik i zużycie, ale **nie ma ładowności**), uprawnienia kierowcy w `DriverEntry` oraz wejście do routera na profilu `HeavyDay`/`HeavyNight` — sam profil w `engine/nav` **jest**, tylko `TrafficOracle` nigdy go nie używa. **Dlaczego tutaj, a nie w M6b:** to WP11 i WP12 pierwsze potrzebują prawdziwych kilometrów. `dc_beats_direct` porównuje koszt na tonę i przy stawce ryczałtowej rozstrzygnąłby się arytmetyką zamiast geografią, czyli spełniłby się tożsamościowo |
| AG-2 ★ | **Konsolidacja milk-run jest opisana w `M6b-zaklad-i-transport.md` §5.6, ale wykonawcą jest WP12.** Numeracja `5.x` jest kontraktem (`K-17`), więc opis zostaje tam, gdzie był; robota jest tutaj | M6b świadomie jej nie zbudował: heurystyka Clarke–Wrighta bez centrum dystrybucyjnego nie ma przypadku, na którym widać, że działa, a `dc_beats_direct` jest jedynym testem mówiącym, czy była warta napisania. Zapisane, żeby nie wyglądało to na zaległość M6b |
| AG-3 ★ | **`WarehouseRole` ma cztery warianty, nie sześć**: `Input`, `Output`, `Backroom`, `Shelf`. `Distribution` i `Tank` z §5.5 **nie powstały**. WP12 dokłada `Distribution` **na końcu enuma** | Kolejność wariantów jest kontraktem zapisu gry — `role` siedzi w slocie magazynowym, a slot wchodzi do hasha stanu. Wstawienie `Distribution` w środku przenumerowałoby każdy magazyn w każdym zapisanym świecie. `Tank` prawdopodobnie nie powstanie wcale i to jest właściwe: zbiornik jest **klasą przechowywania** (`StorageClass::Tank`, istnieje), a nie rolą magazynu — rola odpowiada na pytanie „po co ten magazyn stoi", a nie „co w nim trzymamy" |
| AG-4 | **`Good::pack` nadal nie istnieje** (`AD-4` z dokumentu fazy przypisał je WP11) i M6b tego nie zmienił. Zastane jest natomiast wszystko, na czym półka ma stanąć: `Store::load`/`unload` (ruch przez zlecenie), `Store::available`, `Store::write_off` i `InventoryRule::daily_shop` z przeglądem dobowym o 4:00 | Bez zmian wobec `AD-4`; zapisane, bo WP11 jest pierwszym pakietem, który tego pola dotyka, i warto wiedzieć, ile z reszty zaplecza już stoi |
| AG-5 ★ | **Awaria linii wymaga części z magazynu i od M6b ma w mieście producenta.** `data/buildings/industry.ron` dostało osiem archetypów (`bearing_works`, `seal_works`, `electric_motor_plant`, `carton_plant`, `pallet_works`, `pet_bottle_plant`, `seed_plant`, `precast_plant`), więc `part_bearing_6204` i `part_pump_seal` nie muszą już przyjeżdżać zza granicy | `AE-6` nazwało to jako cenę do zapłacenia; zapłacona jest. Dla WP10 to ma znaczenie bezpośrednie: szyb z §7.2 psuje się na `part_pump_seal` co ~1 400 godzin pracy, a scenariusz `deposit_50y` ma pokazać kaskadę **w dół własnego łańcucha** — nie ścieżkę „brama graniczna dowozi wszystko" |

---

## Zmiany wpisane po M6c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6c.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AK-1 ★ | **Import ma od M6c drugie, dynamiczne źródło prawdy i WP11 musi wiedzieć, które wygrywa.** `brama_t_na_dobe` w `sites/closure.rs` to budżet **generatora miasta** — stała używana przy domykaniu Etapu 7. Obrót w grze idzie przez `TradeNode` (przepustowość dobowa, zaległość, `lead_minutes`, cena rosnąca z wolumenem) i `TariffTable` z `data/trade/tariffs.ron`. Te dwie liczby **nie są tym samym** i nie wolno ich zlać: pierwsza odpowiada na „czy to miasto da się w ogóle uruchomić", druga na „ile kosztuje dziś sprowadzić tonę" | Wiersz po R1 mówi „to jest dosłownie punkt, w którym WP11 wchodzi" — i to nadal prawda, ale od M6c wejście ma dokąd prowadzić, a przedtem nie miało. Ryzyko przy pominięciu jest ciche i kosztowne: WP11 przepisuje pętlę na budżet dynamiczny i bierze stałą generatora zamiast węzła, przez co import w grze jest znów płaski, a cała elastyczność ceny z WP9 nie ma ani jednego czytelnika |
| AK-2 ★ | **Feature `infinite_supply` z kryterium WP11 nie istnieje w workspace** — ani jednego `#[cfg(feature)]`, ani jednego wpisu w `Cargo.toml`. Nie istnieje też `supply_stub::refill_shelf` z §6.3 dokumentu fazy. Realnym „tworzeniem masy z niczego" jest `ExternalSupplier` w `sim/economy/src/supply.rs`, podpięty **konkretnym typem**, a nie przez `dyn Wholesale`: `MarketInner.supplier: ExternalSupplier` | Kryterium WP11 brzmi „feature domyślnie wyłączony", czyli mówi o wyłączeniu czegoś, czego nie ma — a to jest kryterium spełnione tożsamościowo, dokładnie ta klasa błędu, którą `K-18` wymienia jako przypadek (3). Do zrobienia są **dwie** rzeczy, nie jedna: założyć feature i **odwrócić podpięcie na `dyn Wholesale`**, bo bez tego nie ma jak podmienić implementacji. `AC-1` fazy zostaje wiążące: `Market::set_supply_shock` musi przeżyć podmianę, inaczej bramka G4 balansatora nie ma czym szokować |
| AK-3 ★ | **`Settlement` i `SellerRef` z M6c są gotowym wejściem księgowania dla WP11 i nie trzeba dla nich projektować niczego nowego.** Niosą strony, towar, masę, kwotę **netto**, cło osobno i `DecisionReason`. Po stronie `sim/economy` brakuje dokładnie jednej rzeczy: `SupplierRef` (`books.rs`) ma **jeden** wariant, `External`, i WP11 musi dołożyć `Firm(FirmId)` | Wzorzec jest ten sam, którym `Plant::bill_utilities` oddaje faktury za media, i jest już opisany w M6e §5.11 — ale **wyłącznie dla mediów**, więc przy zakupach B2B ktoś może go nie rozpoznać i zacząć od projektowania drugiego kanału. Kierunek zależności zamyka sprawę: `sim/supply` księgi nie widzi i widzieć nie może, więc wszystko, co ma trafić do `Books`, wychodzi z rynku listą faktów |
| AK-4 | **`Store::export` już istnieje i jest trzecim wyjściem masy z magazynu**, obok `take` (zużycie) i `write_off` (strata). `AF-4` wylicza, co „stoi pod" półkę sklepu, i tej pozycji nie zna | Zapisane, bo WP11 dotyka dokładnie tego szwu, a pole `exported` w bilansie ma od M6c pierwszego pisarza. Konsekwencja praktyczna dla testu migracyjnego z §6.3: bilans masy chleba domyka się teraz przez **trzy** kategorie wyjścia, a nie dwie, i przebieg z eksportem w tle da inną prawą stronę niż przebieg bez niego |
| AK-5 | **Część instalacji WP10 jest zastana.** `DepositId` i `BatchOrigin::deposit` istnieją od M6a (`batch.rs`), a `BatchOrigin::imported()` — od M6c i **świadomie urywa ślad**: świata zewnętrznego nie symulujemy, więc panel „od pola do półki" ma powiedzieć „import", a nie domalować łańcuch do złoża, którego nie ma. Nie istnieje natomiast `good_for(kind: ResourceKind) -> GoodId` zapowiedziane w §5.10 | Ta sama zasada, którą M6 i M10 uzgodniły dla partii odtworzonej z agregatu (§6.4.2 dokumentu fazy): przyznanie się do braku danych jest tańsze i uczciwsze od fabrykowania. WP10, podłączając złoża, dokłada **drugi** koniec tego samego śladu i ma go nie mylić z pierwszym |
| AK-6 | **`AF-1` wskazuje dobry adres, ale złe nazwy:** `reachable` jest metodą grafu w `catalog/graph.rs`, a nie metodą `Catalog`; `has_external_price` i `gates` są metodami `Good`, a nie `Catalog` | Drobiazg, który kosztuje dziesięć minut szukania w crate'cie, w którym się nie pracowało — i dokładnie ten rodzaj nieścisłości, dla którego `K-18` każe poprawiać dokument fazy następnej od razu |

---

## Zmiany wpisane po M6d

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AL-1 ★ | **`MiningSite` ma trzy pola, nie sześć**: `deposit`, `concentration_pct`, `depth_m`. `daily_capacity` i `good` z §5.10 **nie powstają** | `daily_capacity` to ta sama liczba co `ProductionLine::nominal_throughput` — dwa pola o jednej treści rozjeżdżają się przy pierwszej zmianie, a linia i tak jest tym, co ogranicza przerób. `good` mówi wyjście receptury `Extraction(kind)`, czyli **dane**; trzymanie go drugi raz w kodzie znaczyłoby, że dopisanie towaru do `data/recipes/extraction.ron` wymaga rekompilacji |
| AL-2 ★ | **`good_for(kind: ResourceKind) -> GoodId` z §5.10 nie powstaje.** Odwzorowanie surowca na towar **już jest** i mieszka w danych: `RecipeSource::Extraction(ResourceKind)` plus `outputs` receptury | Funkcja w Ruście byłaby **drugim źródłem prawdy** o tym samym odwzorowaniu — i tym gorszym, bo niewidocznym w diffie danych. To ten sam argument, którym `K-8` zabrania duplikatu enuma: dwie kopie rozjeżdżają się przy pierwszej zmianie, a tu „pierwsza zmiana" znaczy dopisanie ósmego surowca |
| AL-3 ★ | **`DepositId` był **dwoma** typami i żaden nie prowadził do drugiego.** `sim/world::DepositId(u32)` i `sim/supply::DepositId(u16)`, bez konwersji. Przeniesiony do `engine/core` jako **`K-39`** | Panel „od pola do półki" (PRD §14.4) obiecuje ślad do **konkretnej żyły**, a `BatchOrigin::deposit` prowadził do liczby, która przypadkiem wyglądała podobnie. Nic tego nie łapało, bo oba typy kompilowały się osobno — i to jest dokładnie ta klasa błędu, którą widać dopiero na ekranie gracza |
| AL-4 ★ | **Receptury `Extraction` nigdy się nie uruchamiały.** `Recipe::batch_mass` liczyło się jako suma **wejść**, a wydobycie wejść nie ma, więc `sprobuj_start` odrzucało je warunkiem `batch_mass <= 0`. Poprawione: receptura bez wejść bierze skalę z **wyjść** | Dobra wiadomość jest taka, że masy z niczego nie tworzyły — zła, że cała gałąź `Extraction` katalogu (osiem receptur, ~25 % punktów wejścia grafu) stała bezczynnie od M6b i **żaden test tego nie widział**, bo wszystkie stawiały młyn albo piekarnię. Zapisane, bo to jest wzorzec do sprawdzenia w M6e: gałąź katalogu bez testu jest gałęzią, o której nie wiadomo, czy działa |
| AL-5 ★ | **Bilans złoża wchodzi do M6 portem `Deposits` z `&self`, a nie `&mut`.** Trzy metody, wewnętrzna mutowalność po stronie implementacji (`magnat_world::DepositLedger`) | Wydobycie dzieje się w środku `advance_production`, gdzie `&mut Store` i `&mut Plant` są już pożyczone; przeciśnięcie czwartej pożyczki mutowalnej przez wszystkie sygnatury pętli minutowej kosztowałoby więcej niż `RefCell` w jednym miejscu. Wzorzec jest zastany, nie wymyślony: `TrafficOracle` trzyma router pod `Mutex` za `&self` z dokładnie tego samego powodu. Determinizm nie cierpi — kolejność zakładów w pętli produkcji jest ustalona |
| AL-6 | **Wzór kosztu wydobycia z §5.10 jest kontraktem, liczba „~2 480 zł/t po 60 % wyczerpania" z §7.2 — nie.** Ten sam wzór na tych samych parametrach szybu (koncentracja 82 %, głębokość 1 800 m) daje przy 60 % wyczerpania ~3 780 zł/t | Koszt startowy **zgadza się** (1 620 zł/t, test `koszt_startowy_szybu_zgadza_sie_z_lancuchem_referencyjnym`, tolerancja ±2 % jak dla wszystkich kosztów łańcuchów referencyjnych). Rozbieżność jest w członie kwadratowym wyczerpania i jest **celem kalibracji balansatora**, a nie błędem: §7.1 mówi wprost, że liczby łańcuchów referencyjnych są „wartościami startowymi do kalibracji". Test pilnuje monotoniczności i punktu startowego, bo to są własności wzoru; wartość w połowie drogi jest własnością strojenia |
| AL-7 ★ | **Stawka przewozowa przeniosła się z tonokilometra do katalogu pojazdów jako grosze za kilometr** (`VehicleClassSpec::haul_gr_per_km`). `TransportTuning::cost_gr_per_tonne_km` zostaje dla atrapy `FlatRateFreight` | To nie jest zmiana jednostki, tylko **warunek istnienia kryterium `dc_beats_direct`**. Przy stawce tonokilometrowej koszt jest liniowy w masie niezależnie od liczby kursów, więc konsolidacja dostaw **nie oszczędza nic** i porównanie „centrum dystrybucyjne kontra dostawa bezpośrednia" rozstrzyga się arytmetyką zamiast geografią, czyli spełnia się tożsamościowo. Liczby są przy tym te z §7.1 i §7.2 (4,20 zł/km wywrotka, 2,10 zł/km furgonetka, 6,80 zł/km cysterna), które M6b sprowadził do jednej stawki, bo nie miał katalogu ciężarówek |
| AL-8 ★ | **`FreightQuote` dostaje `call_out`** — część kosztu płaconą raz za pojazd, nie za kilometr | Bez wyodrębnienia tej części milk-run po dziesięciu sklepach płaciłby dziesięć razy za podstawienie tej samej ciężarówki, czyli dokładnie tyle, ile dziesięć osobnych kursów. Druga połowa mechanizmu z `AL-7` |
| AL-9 ★ | **`WarehouseRole::Distribution` dopisany na końcu enuma; `Tank` nie powstaje** (potwierdzenie `AG-3` z M6b) | Zbiornik jest **klasą przechowywania**, nie rolą magazynu: rola odpowiada na „po co ten magazyn stoi", a nie „co w nim trzymamy". Kolejność wariantów jest kontraktem zapisu gry, bo `role` siedzi w slocie, a slot wchodzi do hasha |
| AL-10 ★ | **`Good::pack` (`RetailPack`) nie powstaje** — wbrew `AD-4` i `AF-2`, które przypisały je WP11. Przelicznik sztuk detalicznych na masę wynika z dwóch pól, które **już są**: `GoodForm` (przez `GoodUnit`) i `unit_mass` | To jest odpowiedź na pytanie, które `AF-2` zadało poprawnie („wie, czego potrzebuje, dopiero ten, kto łączy półkę z magazynem") — i odpowiedź brzmi „niczego nowego". Dla postaci sypkich i ciekłych ilość **jest** masą w gramach, dla sztukowych przelicza się przez masę sztuki. Trzecie pole obok dwóch, które się zgadzają, byłoby trzecią liczbą do rozjechania i migracją ~60 plików danych bez powodu. Metody: `Good::mass_of_units` i `Good::units_of_mass` |
| AL-11 ★ | **Łańcuch jest jednym zasobem `Chain`, a nie czterema** (`Store`, `Plant`, `Transport`, `B2b` — `AJ-1`). Wchodzi do hasha przez `register_resource_hash` jako jedna sekcja, w tej kolejności | Rozszerzenie `AD-7` i `AG-2` o piętro wyżej: `World::resource_mut` pożycza **cały** świat, a rozstrzygnięcie zapytania ofertowego podpisuje kontrakt (`B2b`), wystawia zlecenie (`Transport`) i ładuje partię (`Store`) w **jednym** kroku. Cztery zasoby znaczyłyby cztery bufory komend i rozbicie tego kroku na cztery ticki. `K-29` dopuszcza to wprost i nie wymaga nowego rozstrzygnięcia |
| AL-12 ★ | **Sklep nie dostaje `InventoryRule`** — wbrew krokowi 1 migracji z §6.3. Zamówienia idą wyłącznie przez `Wholesale` i politykę M5 (`docelowy_zapas`) | Dwie polityki nad jednym magazynem nie są nadmiarem, tylko **sprzecznością**: obie zamawiają, żadna nie widzi zamówień drugiej i zaplecze rośnie do sumy obu celów (zmierzone: 2 t chleba na zapleczu osiedlowego sklepu wobec celu 250 kg). Wygrywa polityka M5, bo wiąże cel z obrotem tygodniowym i z terminem ważności i jest kalibrowana balansatorem od M5e. Reguły łańcucha zostają dla zakładów produkcyjnych, gdzie polityki detalicznej nie ma |
| AL-13 ★ | **Sklep płaci przy odbiorze, nie przy zamówieniu.** Krok 4 migracji z §6.3 zakładał kolejność M5: przelew przy złożeniu zamówienia, towar po `lead_time_days` | Zapytanie ofertowe **może nie znaleźć dostawcy** — i to jest cała różnica między dostawcą zewnętrznym a rynkiem. Sklep, który zapłacił za towar, którego nikt nie przywiózł, ma dziurę w kasie bez zdarzenia, które by ją tłumaczyło. Przy zamówieniu sprawdza się **pokrycie**, płaci się przy rozładunku. Konsekwencja, której nie było w planie: dostawa, na którą sklepowi zabrakło środków, i tak wchodzi na stan — jako **zobowiązanie** (`TradePayable` schodzi na minus), bo towar fizycznie stoi w magazynie, a pominięcie przyjęcia rozjechałoby `InventoryGoods` z wyceną zapasu (P5) |
| AL-14 ★ | **Cło wchodzi do kosztu nabycia partii importowanej.** `K-7` mówi, że cło nie modyfikuje **ceny w ofercie** — i to zostaje bez zmian: sprzedawca podaje netto, obciążenie jest rozpisane osobno | Kupujący, który zapłacił netto **i** cło, ma towar o takiej wartości i tyle wchodzi mu na stan. Rozdzielenie tych dwóch liczb po stronie zapasu rozjeżdżało `InventoryGoods` z wyceną magazynu o sumę ceł — zmierzone na przebiegu rocznym (`po_roku_bilans_zamyka_sie_co_do_grosza`, 138 620 zł na jednym sklepie) |
| AL-15 ★ | **Szok podaży (`Market::set_supply_shock`) mnoży cenę węzła granicznego, a nie wycenę u dostawcy.** `AC-1` wymagało, żeby nazwa wejścia przeżyła podmianę dostawcy — przeżyła, ale musiała zmienić adres | Cena z `Wholesale::quote` służy sklepowi do decyzji „stać mnie czy nie", a płaci się to, co wyjdzie z rozliczenia rynku. Szok postawiony na wycenie podniósłby liczbę w decyzji i nie ruszył kwoty w księdze — czyli bramka **G4** mierzyłaby własny parametr zamiast skutku. Po przenosinach szok jest tym, czym ma być: świat zewnętrzny podrożał |
| AL-16 | **`ProductionLine::new` nie nadaje pierwszego terminu przeglądu** (`next_maintenance == u64::MAX`, czyli „nigdy"). Linia pracująca bez przerwy schodzi z `condition` do zera po **166 dobach** i staje na awarii, której nie ma czym naprawić | Pułapka dla tego, kto składa zakład: objaw jest odległy o pół roku gry od przyczyny i wygląda na problem z częściami zamiennymi. Zapisane tutaj i w tabeli dla M6e, bo to M6e stawia zakłady z generatora. Sygnatury nie zmieniono — `new` nie widzi harmonogramu zakładu, a zgadywanie okresu przeglądu w konstruktorze linii byłoby gorsze od jawnego pola |
| AL-17 | **Złoże nie jest przypisane do zakładu w danych miasta.** `SiteSeed` nie niesie `DepositId`; generator sprawdza wyłącznie, **czy** pod parcelą leży złoże wymaganego rodzaju (`zloze_ok`), i wynik sprawdzenia wyrzuca | WP10 tego nie potrzebował — testy stawiają szyb wprost — więc pole nie powstało (YAGNI). Potrzebuje go M6e, kiedy zakłady Etapu 7 zaczną powstawać jako `PlantSite`: bez niego kopalnia nie wie, z czego kopie. Adres poprawki: `Przydzial` w `sites/place.rs` albo `utworz_zaklady` z dostępem do `BuildInput` |
| AL-18 ★ | **`supply_closure_check` zostaje w generatorze i zostaje statyczne.** Tabela „Zmiany wpisane po R1" zapowiadała, że WP11 przejmie tę funkcję i zamieni stały budżet bram na dynamiczny; nie przejął i **nie powinien był** | `AK-1` postawiło pytanie właściwie: to są **dwie różne liczby**. `brama_t_na_dobe` odpowiada na „czy to miasto da się w ogóle uruchomić" i jest pytaniem generatora, zadawanym raz, przed pierwszym tickiem. `TradeNode` odpowiada na „ile kosztuje dziś sprowadzić tonę" i jest pytaniem gry, zadawanym co dobę. Zlanie ich znaczyłoby, że domknięcie Etapu 7 zależy od stanu rynku, którego w chwili domykania jeszcze nie ma. Import w grze jedzie przez `TradeNode` — z przepustowością, zaległością i ceną rosnącą z wolumenem — i to jest ta elastyczność, o którą chodziło. Martwy zapis w KROKU 4a (`import[g].max(1)` przesłonięty drugą deklaracją) zostaje **nietknięty**, bo pętli nikt nie przepisuje; przechodzi do M6e razem z resztą Etapu 7 |
| AL-19 ★ | **Prawdziwy router M4 pod `FreightOracle` powstał, ale świat go jeszcze nie składa.** `magnat_traffic::freight::RoadFreight` istnieje, ma testy i liczy prawdziwe kilometry po grafie drogowym na profilu `HeavyDay`; most `magnat_headless::retail` nadal wstrzykuje atrapę `FlatRateFreight` | Most dostaje `Box<dyn TravelOracle>`, a nie `Arc<TrafficOracle>` — czyli **nie ma czego podłączyć**, bo router jest własnością tego drugiego. Rozszerzenie sygnatury mostu to zmiana w miejscu, w którym świat składa się w całość, a to jest robota M6e (`AJ-2`: wpięcie zasobów i kadencji). Cena jest nazwana i ograniczona: w scenariuszach headless koszt dostawy nie zależy od geografii miasta. Kryterium `dc_beats_direct` na tym nie cierpi, bo jego test stoi po stronie `sim/traffic` i używa prawdziwej siatki ulic |
