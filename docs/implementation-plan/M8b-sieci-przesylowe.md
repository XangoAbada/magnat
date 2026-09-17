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
**Stan:** `[x]` zamknięty. `tools/headless/tests/blackout.rs` (T2, dwa testy) i
`sim/traffic/tests/utility.rs` (12 testów solvera) zielone; `cargo bench -p magnat-traffic
--bench utility_bench` daje 186 µs na tick pięciu sieci metropolii wobec budżetu 300 µs
i 891 µs na ośmiorundową kaskadę jednej sieci wobec budżetu 2 ms (patrz `CC-11`).

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


---

## Zmiany wpisane po M8a

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po domknięciu M8a.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`UtilityKind` z §5.4 nie powstaje — rodzajem mediów jest `UtilityService` i on już jest w `engine/core`.** Nazwa `UtilityKind` jest **zajęta** i znaczy co innego: to wymiar oceny w funkcji użyteczności zakupu (`Price`, `Quality`, `Distance`, `Time`, `Variety`, `Brand`, `Habit`, `Convenience`, `Risk`), wniesiony przez M5. Media nazywają się `UtilityService` i mają siedem wariantów: `Electricity`, `Water`, `Sewage`, `Gas`, `Heat`, `Waste`, `Internet` — czyli `Power` to `Electricity`, `Telecom` to `Internet`, a `Waste` dochodzi. Enum siedzi w `core::vocab` od M5 i jest ładunkiem `TxKind::Utility`, więc **już dziś** niesie każdą fakturę za media w księgach zakładu | Przypadek (1) i (2) z `K-18` naraz. Drugi enum o tej samej treści rozjechałby się przy pierwszej zmianie, a pierwsza zmiana jest tu pewna, bo to M8b dokłada sieci. Do wykrycia było przy pierwszym `use magnat_core::UtilityKind` — czyli po napisaniu połowy solvera |
| ★ | **Akcyza od energii należy do tej podfazy i jest jedyną, która ma w tej grze wolumen.** M8a wpięła akcyzę w dwa prawdziwe punkty (sprzedaż detaliczna, rozliczenie hurtowe) i zmierzyła, że **nie ma czego obłożyć**: paliwo kupują pojazdy przez `FuelLedger` (M4), który nie ma konta w księgach; piwo stoi na półce, ale przegrywa z sokiem w rangach substytutu `data/economy/retail.ron`; papierosów nie ma w asortymencie detalicznym. `ExciseClass::Energy` z PRD §6.8 przechodzi dokładnie przez to, co ta podfaza buduje — rachunek za media, wystawiany co miesiąc każdemu zakładowi. Stawka dopisuje się do `data/city/tax.ron` (sekcja `excise`), naliczenie idzie tam, gdzie `absorb_utility_bills` księguje fakturę | `R2` („martwe hazardy") zastosowane do daniny: danina, której nikt nigdy nie naliczył, przechodzi **każdy** test domknięcia i w raporcie wygląda tak samo jak danina, której nikt nie zapłacił |
| | **Miasto jest zasobem świata (`magnat_city::City`), nie encją.** `Policy::TariffCap` i cała reszta polityki taryfowej wpina się przez `world.get_resource::<City>()`; rejestr należności (`ChargeRegistry`) i budżet (`CityBudget`) są jego polami. Wejście dla operatora sieci: `ChargeRegistry::accrue(payer, kind, period, base, mass, rate_bp, amount, at, due_at)` — jedyna droga, którą danina powstaje | Rozstrzygnięcie `D4` fazy zamknięte w M8a odwrotnie do propozycji (`CA-10`); §5.4 nie odwołuje się do tego wprost, ale pierwszy kod M8b, który zechce naliczyć opłatę, odwoła się na pewno |


---

## Zmiany wpisane po M8b

Zgodnie z `K-18`. Korekty §5.4 tej podfazy — czyli tego, co projekt techniczny
obiecywał, a czego implementacja nie zrobiła albo zrobiła inaczej.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| `CC-1` | **`TopologyCache.loop_groups` nie powstaje — pętle rozstrzygają się mostami.** §5.4 krok 4 zapowiadał kolapsowanie składowych 2-spójnych do superwęzłów o przepustowości równej sumie ich krawędzi. Kontrakcja nie jest potrzebna, bo pytanie, na które odpowiada, brzmi „czy tej krawędzi wolno wierzyć w przepływ z poddrzewa" — a to jest definicja **mostu**. Krawędź będąca mostem niesie sumę popytu swojego poddrzewa co do wata; krawędź niebędąca mostem leży w pierścieniu i ma obok siebie objazd. `TopologyCache` dostaje więc `is_bridge: Vec<bool>` i jedno przejście Tarjana zamiast kontrakcji grafu | Ta sama odpowiedź, mniej kodu i mniej struktur. Cena jest nazwana wprost w `utility::topo` i w teście `pierscien_nie_wypada_mimo_ciasnej_przepustowosci`: **pierścień nie przeciąża się nigdy**. Kaskada wchodzi przez otwarcie pierścienia (awaria N-1), a nie przez przeciążenie jego odcinka — i tak kaskadują prawdziwe sieci |
| `CC-2` ★ | **Kolejność zrzutu obciążenia jest odwrotna do zapisanej w §5.4.** Krok 3 mówił „odłączaj węzły w porządku `(priority, node_index)` — **rosnąco po priorytecie**", a w tym samym akapicie „szpital i wodociąg mają priorytet 0, gospodarstwa domowe 2, przemysł 3". Przy kolejności rosnącej pierwszy zrzut w mieście gasiłby szpital. Zrzut idzie **malejąco po priorytecie**, rosnąco po indeksie węzła | Sprzeczność wewnątrz jednego akapitu, rozstrzygnięta na korzyść zdania o szpitalu — to ono niesie intencję. Test `zrzut_gasi_przemysl_przed_szpitalem` pilnuje obu końców: przemysł schodzi pierwszy, a szpital zostaje nawet przy mocy wystarczającej dla jednego odbiorcy |
| `CC-3` | **`NodeRole::Transformer { capacity }` nie powstaje.** Przepustowość stacji jest przepustowością krawędzi, która do niej dochodzi, a dwie liczby o jednym znaczeniu rozjeżdżają się przy pierwszej zmianie. Węzeł pośredni jest `NodeRole::Hub` — bez popytu i bez prawa do zrzutu | `DRY` dotyczy wiedzy, nie kształtu: „ile przez to przejdzie" jest jedną regułą i ma mieć jedną liczbę |
| `CC-4` | **`EdgeState::UnderMaintenance` nie powstaje.** W M8b nikt nie planuje remontu linii, a wariant, którego nic nie ustawia, przechodzi każdy test i wygląda w raporcie tak samo jak wariant działający. Planowy remont dokłada **M8e** razem z polityką taryfową — wtedy będzie miał kto go ustawić | `R2` („martwe hazardy") zastosowane do wariantu enuma. Ta sama reguła, którą M8a zastosowała do akcyzy |
| `CC-5` | **`Tariff` ma dwa pola zamiast czterech: `peak_multiplier_bps` i `connection_fee` nie powstają.** Licznik zakładu rozlicza się raz na miesiąc **jedną stawką** (M6b), więc mnożnik szczytowy zapisany w chwili wystawienia faktury obłożyłby nim cały miesiąc; taryfa strefowa wymaga rozliczenia godzinowego, czyli zmiany kształtu licznika po stronie M6. Opłaty przyłączeniowej nie ma kto naliczyć, bo w M8b wszystkie przyłącza istnieją od minuty zero | Szczyt **jest** w modelu i ma skutek — tylko po stronie popytu (`LoadProfile` plus `profile_household`/`profile_industry` w `data/tuning/grid.ron`), gdzie działa fizycznie zamiast księgowo. Różnicę pokazuje test `szczyt_wieczorny_zrzuca_to_czego_noc_nie_zrzuca` |
| `CC-6` | **`UtilityNode.metered` nie powstaje.** Zużycie liczy `magnat_supply::UtilityMeter` od M6b i drugi licznik po stronie sieci byłby drugą prawdą o jednej liczbie. Sieć ustawia licznikowi **dwie** rzeczy, których ten sam o sobie nie wie: `cut_off` (czy medium jest) i `tariff`/`standing` (po ile) | Wykonanie zapowiedzi z `data/tuning/supply.ron`: „M8 podmieni je polityką miasta i sieciami przesyłowymi, **nie ruszając kształtu licznika**" |
| `CC-7` ★ | **`power_available` i `supply_state` są metodami `UtilityGrids`, a nie funkcjami wolnymi.** §5.4 i §6 dokumentu fazy zapisywały `pub fn power_available(site: SiteId) -> bool` — taka sygnatura wymagałaby stanu globalnego, czego 00 §3.1 zabrania. Zasób świata woła się `world.resource::<UtilityGrids>().power_available(site)` | Kontrakt dla M7 nie zmienia treści, tylko adres. Zakład **bez** przyłącza dostaje `SupplyState::Ok`, a nie `Isolated`: brak przyłącza znaczy „nie potrzebuje", tak samo jak brak licznika po stronie M6 — odwrotna odpowiedź zatrzymałaby każdy zakład, który prądu nie bierze |
| `CC-8` ★ | **Powstają dwie sieci, nie pięć: prąd i woda.** Solver jest jeden i obojętny na medium (benchmark mierzy pięć sieci, tak jak wymaga T8b), ale most stawia tylko te, które mają w tym mieście **odbiorcę z licznikiem**. Gaz i ciepło dostaną sieci razem z popytem grzewczym, czyli razem z pogodą w **M8c**; odpady i telekomunikacja razem z usługami miejskimi w **M8d** | `R2` jeszcze raz: sieć bez ani jednego odbiorcy przechodzi każdy test i wygląda w raporcie tak samo jak sieć, która działa. Dokładnie ten błąd M8a popełniła przy akcyzie i nazwała go po fakcie |
| `CC-9` ★ | **`CityTaxEngine::excise_on` nie było nadpisane i akcyza towarowa była martwa — poprawka u źródła.** M8a zamknęła się z wnioskiem „akcyza jest wpięta w dwa prawdziwe punkty, ale nie ma czego obłożyć". Obrót nie miał z tym nic wspólnego: M5 woła `m.tax.excise_on(...)` w `market/fulfil.rs` i `market/restock.rs`, a `CityTaxEngine` tej metody **nie implementował**, więc ciało domyślne z traitu zwracało zero i cała sekcja `excise` w `data/city/tax.ron` razem z `VatTable::excise_per_kg` była nieużywana. Po czterolinijkowej poprawce ten sam scenariusz `m8miasto --days 35` nalicza **149 708,88 zł akcyzy** zamiast zera | To jest `R2` w postaci, w której najtrudniej go zobaczyć: danina, której **nikt nigdy nie zapytał o stawkę**, przechodzi każdy test domknięcia (`Σ Assessed = Σ Settled + …` zgadza się na zerach) i w raporcie wygląda identycznie jak danina, na którą nie ma podstawy. Wniosek diagnostyczny M8a był prawdziwy co do faktów o obrocie i fałszywy co do przyczyny |
| `CC-10` | **Akcyza od energii ma własny hak i własną sekcję danych.** `TaxEngine` dostaje `excise_on_utility(service, units) -> Money` z ciałem domyślnym (`K-57` przewiduje dokładnie taką drogę), `data/city/tax.ron` dostaje sekcję `excise_energy` (grosze za kWh), a `TAX_SCHEMA_VERSION` idzie z 1 na 2. Naliczenie wchodzi w `Market::absorb_utility_bills` i jedzie do miasta **tą samą kolejką co akcyza hurtowa** (`b2b_outbox` → `assess::clo_i_akcyza`), więc nie powstaje ani jedna nowa ścieżka księgowa | Osobno od `excise_on`, bo tamta liczy **od masy wyrobu**, a kilowatogodzina masy nie ma. Wciśnięcie energii do tamtej metody wymagałoby udawanej masy. Przy okazji `UtilityMeter::bill` zwraca teraz `(kwota, jednostki)`: wyliczanie jednostek z kwoty przez dzielenie przez taryfę byłoby dzieleniem liczby zaokrąglonej przez liczbę, którą zmienia uchwała rady |
| `CC-11` ★ | **Budżet T8b zmierzony, z rozbiciem, którego test nie przewidywał.** Tick pięciu sieci metropolii (po ~5 000 węzłów i ~5 000 krawędzi): **201 µs** wobec budżetu 300 µs. Kaskada ośmiorundowa **jednej** sieci: **1,02 ms** wobec budżetu 2 ms. Kaskada ośmiorundowa **pięciu sieci naraz**: **5,36 ms** — powyżej budżetu i wpisane wprost | Kaskada jest z definicji zdarzeniem **jednej** sieci: wypadnięcie linii zmienia topologię tej sieci i niczyją inną. Do tego z tabeli częstotliwości w §5.4 wynika, że tylko prąd liczy się co minutę, więc pięć sieci spotyka się w jednym ticku raz na godzinę, a osiem rund w każdej z nich w tej samej minucie nie jest stanem, który model umie wyprodukować. Wiersz zostaje w benchmarku mimo to, bo górny koszt ticku wolno **znać**, a nie zakładać. Dwie optymalizacje po drodze, obie zmierzone: pętla zrzutu była kwadratowa względem liczby wysp (6,3 → 4,5 ms po zbudowaniu kolejki jednym przejściem), a klucz sortowania krotką zamiast spakowanego `u64` kosztował 1,8 ms z tych 4,5. Domknięcie bilansu po wyczerpaniu limitu rund (`CC-19`) dokłada do kaskady ~0,5 ms i jest tego warte |
| `CC-12` | **Odcięcie prądu zatrzymuje linię w toku, a nie dopiero przy następnej szarży** (`sim/supply/src/plant/produce.rs`). Do M8b `cut_off` sprawdzało się wyłącznie w `sprobuj_start`, więc szarża rozpoczęta przed blackoutem dochodziła do końca i pobierała prąd, którego nie było. Wsad przepada, tak samo jak przy awarii mechanicznej — ten sam zapis stanu i ten sam wpływ na bilans masy | T2 punkt (b) mierzy „wolumen produkcji w oknie awarii **dokładnie 0**", a nie „po zakończeniu bieżącej szarży". Bez tej poprawki test pokazywał 136 MWh pobrane przez zakłady bez prądu. Pisarzem jest M8 w cudzym crate'cie, czyli ta sama konstrukcja co `labor_pct` z `K-44` |
| `CC-13` | **`UtilityMeter` dostaje `standing: Money` — opłatę stałą okresu.** Zero do M8b (wartość neutralna, żaden test M6 nie drgnął); od M8b wpisuje ją operator razem z taryfą. To ona sprawia, że **blackout widać w kosztach**: licznik stoi na zerze, a rachunek przychodzi | T2 punkt (c) wymaga, żeby koszty stałe biegły, a koszt energii zmiennej był zerowy. Bez opłaty stałej zakład bez prądu miałby rachunek za media równy zero i blackout byłby dla niego **oszczędnością** |
| `CC-14` | **Operator sieci jest identyfikatorem bez księgi.** `UtilityNetwork.operator` niesie `FirmId`, ale pieniądz za media wychodzi na konto reszty świata — tak samo jak od M6b. Taryfa **jest** już własnością sieci i to jest realna zmiana; brakuje rachunku wyników operatora | Decyzja `D10` fazy: „w M8 wyłącznie firmy AI z taryfą; przejęcie i przetarg na operatora — M9/M10". Droga wyjścia opisana w `utility.rs`: założyć firmę w rejestrze M7 i skierować `absorb_utility_bills` na jej konto — reszta modelu się nie zmienia |
| `CC-15` | **Model jest promieniowy i jednoźródłowy per wyspa.** Przepływ krawędzi to suma popytu jej poddrzewa, a ta liczba jest przepływem tylko wtedy, gdy korzeniem wyspy jest jej źródło — dlatego `TopologyCache::rebuild` bierze `prefer_root` i zaczyna przejście od źródeł. Wyspa z **dwoma** czynnymi źródłami dostanie przepływy przypisane w całości do ścieżki jednego z nich | Sufit nazwany w `utility.rs` obok tego z §5.4. Ścieżka wyjścia jest ta sama: liniowy rozpływ DC za tym samym interfejsem `solve`. Miasto M8b ma jedną elektrownię i jedno ujęcie, więc ograniczenie nie jest dziś widoczne — ale będzie, gdy M8e pozwoli graczowi postawić drugi blok |
| `CC-16` | **Bramka scenariusza `m8miasto` („≥ 3 daniny z niezerowymi wpływami") była czerwona przed M8b.** Zmierzone na `HEAD` w osobnym drzewie roboczym: `--days 35` daje **1** daninę z wpływami (cło). Po poprawce `CC-9` i akcyzie od energii — **2**. Do trzech brakuje VAT-u, PIT-u i podatku od nieruchomości, a te mają termin **20. dnia miesiąca następnego**, czyli wymagają przebiegu ≥ 51 dób. Przebieg 51-dobowy daje **5** danin z wpływami i bramka jest zielona, więc `ci.yml` idzie z 35 na 51 dób | Bramka nie jest własnością M8b, ale `CLAUDE.md` wymaga zielonego `master` po każdym commicie, a różnica między „mechanizm nie działa" a „przebieg jest za krótki o dwadzieścia dób" jest różnicą między błędem a parametrem. Przebieg wydłuża się o ~15 s CI |
| `CC-17` | **Sklep nie jest węzłem sieci, a „kasa w sklepie nie działa" z §5.4 nie powstaje.** Węzły popytu bez licznika to **gospodarstwa domowe, jeden węzeł na dzielnicę**. Sklep ma ten sam priorytet zrzutu co gospodarstwo, więc osobny węzeł rozbijałby tę samą liczbę na dwie i nie zmieniałby ani chwili, w której zapala się zrzut. Sama **kasa bez prądu** jest zmianą zachowania sklepu po stronie M5 (sprzedaż odrzucona z własnym `RejectCause`), a nie stanem sieci — i należy do **M8e** razem z resztą skutków taryfowych | `R2` zastosowane do parametru danych: `shop_w_per_slot` stał w `data/tuning/grid.ron` przez jeden przebieg i nikt go nie czytał, więc wyleciał. Parametr, którego nikt nie czyta, wygląda w diffie balansatora dokładnie tak samo jak parametr, który stroi model |
| `CC-18` ★ | **Sieci powstają raz, przy stawianiu miasta.** Zakład postawiony później — przez cykl życia firm M7 albo przez gracza w M9 — **nie ma węzła w sieci**: `supply_state` odpowiada mu `Ok` (bo brak przyłącza znaczy „nie potrzebuje", `CC-7`), a jego licznik zostaje przy stawce z `data/tuning/supply.ron` zamiast przy taryfie operatora. Przyłączenie nowego odbiorcy wymaga dołożenia węzła i krawędzi plus `UtilityNetwork::mark_topology_dirty` — mechanizm jest, brakuje wołającego | Przypadek (5) z `K-18` znaleziony recenzją: żaden pakiet M8b nie jest właścicielem przyłączania w trakcie gry. Adres jest **M8e**, razem z przetargiem na operatora i pozwoleniem na budowę — bo to tam powstaje moment, w którym ktoś prosi o przyłącze. Do tego czasu blackout dotyka zakładów Etapu 7 i wyłącznie ich |
| `CC-19` | **Kaskada domyka bilans po wyczerpaniu limitu rund.** Ósma runda mogła wywalić krawędź i na tym się skończyć — a wtedy węzły odcięte przez tę świeżo wypadłą linię zostawały w stanie sprzed jej wypadnięcia, czyli z prądem, którego nie miały, a `unserved` je pomijał. Po pętli idzie więc jeszcze jedno `przygotuj_topologie` + `rozlicz_wyspy` **bez** kroku zabezpieczeń | Limit rund ma ograniczać **koszt**, a nie pozwalać sieci skłamać — a komentarz przy `MAX_CASCADE_ROUNDS` obiecywał wprost, że reszta niezbilansowanych węzłów idzie w zrzut. Koszt domknięcia to ~0,5 ms w scenie patologicznej i zero w każdej innej, bo wykonuje się wyłącznie wtedy, gdy ostatnia runda coś wywaliła |
| `CC-20` | **Rozpoznanie zmiany idzie po odcisku zbioru odciętych, nie po sumie mocy.** Pierwsza wersja porównywała `unserved` — tick, w którym jeden zakład gaśnie, a drugi o równym poborze wraca, miał tę samą sumę i nie przepisywał `cut_off` do liczników, więc oba zakłady zostawały ze stanem odwrotnym do prawdy aż do najbliższej zmiany sumy. W sieci budowanej przez most jest to sytuacja **prawdopodobna**, bo węzły dzielnicowe mają identyczny `base_demand` | Znalezione recenzją przed commitem. Optymalizacja „nie chodź po arenie zakładów 1440 razy na dobę" jest słuszna, ale jej warunek musi rozpoznawać **zbiór**, a nie jego sumę — suma jest w tej roli funkcją stratną |
| `CC-21` | **Cache list sąsiedztwa unieważnia się przez `mark_topology_dirty`.** CSR jest kluczowany parą `(węzły, krawędzie)`, bo w obrębie kroku zmienia się wyłącznie to, **które** krawędzie są czynne. Przepięcie istniejącej krawędzi liczby nie zmienia, więc klucz by tego nie zauważył i solver liczyłby wyspy, mosty i przepływy dla nieistniejącego grafu — po cichu | Znalezione recenzją. Podział jest teraz jawny: wypadnięcie krawędzi z przeciążenia idzie tanią drogą (tylko `dirty`), a zmiana **kształtu** grafu publiczną (`mark_topology_dirty`, czyli `dirty` + unieważnienie CSR) |
| `CC-22` | **Podstawa akcyzy od energii idzie w tysięcznych jednostki, nie w całych.** Pierwsza wersja zwracała z `UtilityMeter::bill` całe kWh, więc rachunek za 1,5 kWh miał podstawę jednej kilowatogodziny — **zawsze w tę samą stronę i przy każdej fakturze**. Przy okazji: brak wody zatrzymuje linię w toku tak samo jak brak prądu, bo receptura pobiera wodę przy **zamknięciu** szarży, a `sprobuj_start` blokował wyłącznie start | Znalezione recenzją i jest to ta sama klasa błędu, którą doktekst `bill()` nazywa dwa akapity wyżej przy reszcie groszowej: reszta jest pieniądzem. Dzielimy na końcu, tak samo jak przy taryfie |
