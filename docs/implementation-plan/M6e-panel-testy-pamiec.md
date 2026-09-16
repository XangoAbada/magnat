# M6e — Panel, testy, pamięć

Podfaza 5 z 5 fazy **M6 — Łańcuch dostaw** (`M6-lancuch-dostaw.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M6c, M6d, `engine/ui`. WP14 rośnie równolegle od M6a — tutaj jest domykany. |
| **Pakiety robocze** | WP13, WP14, WP15 |
| **Projekt techniczny** | §5.11 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: paliwo od złoża do baku, sklepy przestają być nieskończone. |
| **Kryterium zamknięcia** | Kryteria WP13–WP15 oraz bramki 1–7 fazy M6 w `00-postep.md`. |
| **Poprzednia / następna** | `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md` · — (ostatnia w fazie) |

Panel łańcucha dostaw ze śledzeniem partii, komplet testów E2E, własnościowych i wydajnościowych oraz agregacja partii pod budżet pamięci.

---

## Pakiety robocze

### WP13 — Panel łańcucha dostaw i śledzenie partii
**Zależy od:** WP8, WP12, `engine/ui`.
**Opis.** Graf dostawców i odbiorców z przepływami oraz oznaczeniem ryzyka, lista kontraktów z pokryciem, Gantt dostaw per zakład, tryb „śledź partię" z osią czasu i kosztem narastającym, nakładka „przepływ towaru Y" (dane do renderu z M1/M11).
**Kryterium ukończenia:** `trace_batch` na bochenku chleba zwraca ≥ 5 etapów z czasem, masą, jakością i kosztem; na litrze diesla ≥ 6 etapów aż do `Deposit`.
**Rozmiar: M**

### WP14 — Testy E2E, własnościowe, wydajnościowe, determinizm
**Zależy od:** rośnie od WP2.
**Opis.** Dwa łańcuchy referencyjne z liczbami (§7), osiem testów własnościowych, benchmarki criterion, dopisanie komponentów M6 do funkcji haszującej stan ECS.
**Kryterium ukończenia:** wszystko zielone przy budżecie wydajności z §7.4.
**Rozmiar: L**

### WP15 — Agregacja partii i budżet pamięci
**Zależy od:** WP2, WP14.
**Opis.** `BatchCoalesceSystem`, kubełkowanie jakości i daty przydatności, wyłączenie scalania dla partii śledzonych, kompaktowanie areny, twardy limit liczby partii z trybem awaryjnym.
**Kryterium ukończenia:** miasto 400 tys. mieści się w ≤ 600 tys. aktywnych partii i ≤ 64 MB pamięci gorącej.
**Rozmiar: M**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.11 Systemy ECS i częstotliwości

| System | Częstotliwość | Zadanie |
|---|---|---|
| `ProductionTickSystem` | EveryMinute | postęp szarż, pobór mediów, losowanie awarii, zakończenie szarży → nowe partie |
| `DockQueueSystem` | EveryMinute | kolejki ramp, załadunek i rozładunek |
| `TransportArrivalSystem` | EveryMinute | odbiór zdarzeń przybycia z `sim/traffic`, przejście stanu zlecenia |
| `SpoilageSystem` | EveryMinute (kopiec) | wygaszanie partii wg `expires_at`, przeceny, `LossKind::Expired` |
| `ShortageCascadeSystem` | EveryHour (staggered) | pokrycie zapasu, przejścia `ShortageStage` |
| `ReplenishmentSystem` | EveryHour (staggered) | polityki zapasów → `TransportOrder::Draft` |
| `RfqSystem` | EveryHour | otwieranie RFQ, zbieranie i rozstrzyganie ofert |
| `ContractDeliverySystem` | EveryHour | harmonogram dostaw kontraktowych, naliczanie kar |
| `TransportDispatchSystem` | EveryHour | konsolidacja milk-run, przydział flot, przetargi przewozowe |
| `ScheduleSystem` | EveryHour | zmiany, kolejka szarż, przezbrojenia |
| `MaintenanceSystem` | EveryDay | konserwacja, zużycie maszyn, zamawianie części |
| `ImportExportSystem` | EveryDay | przepustowość węzłów, ceny zewnętrzne, decyzje eksportowe |
| `DepositDepletionSystem` | EveryDay | wyczerpywanie złóż, przeliczenie kosztu wydobycia |
| `UtilityBillingSystem` | EveryMonth | faktury za media → `economy::book` |
| `BatchCoalesceSystem` | EveryDay (staggered) | scalanie partii, kompaktowanie areny |

**Rozpraszanie obciążenia (staggering):** systemy godzinowe i dobowe nie liczą wszystkiego naraz — zakład o indeksie `i` obsługiwany jest w minucie `i % 60` (godzinowe) i `i % 1440` (dobowe). Deterministyczne, bo po indeksie encji, a nie po zegarze. To spłaszcza szczyt CPU z ~12 ms raz na godzinę do ~0,2 ms co minutę.

---

---

## Zmiany wpisane po M6b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6b.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AG-1 ★ | **`sim/supply` nie zależy od `magnat-ecs` i po M6b nadal nie zależy.** Cała faza jest do tej pory biblioteką czystych funkcji nad trzema zasobami: `Store` (partie i sloty), `Plant` (linie, harmonogram, liczniki, rampa) i `Transport` (zlecenia). **Wpięcie ich w harmonogram systemów jest robotą §5.11, czyli tej podfazy** — i dopiero wtedy `sim/supply` dostanie `magnat-ecs` w zależnościach | Odłożenie tego nie było lenistwem tylko kolejnością: kryteria WP4–WP6 są sprawdzalne bez świata ECS, a system wpięty przed czasem trzeba by przepisywać przy każdej zmianie kształtu zasobu. **Co konkretnie zostaje do zrobienia:** trzy zasoby przez `World::register_resource_hash` (nie `register_arena_hash` — arena partii siedzi wewnątrz `Store`, `K-29`), system produkcji jako **wyłączny** (`SystemDesc::exclusive()`, `K-21`) albo z jawnym dostępem do trzech zasobów, przegląd kaskady na `EveryHour`, przegląd zapasów wg `Review` reguły, faktura za media na `EveryMonth` (`Plant::bill_utilities` zwraca listę `(zakład, dostawca, kwota)` do zaksięgowania przez wołającego) i **`SpoilageSystem` przed `RetailSystem`** — to ostatnie jest kontraktem międzyfazowym `D12`, na którym stoi `prop_no_expired_on_shelf` |
| AG-2 ★ | **`advance_production` chodzi pętlą minuta po minucie, O(minutes)** — sufit nazwany w kodzie komentarzem `ponytail:`. Przy kroku minutowym to jedna iteracja; przy makro z krokiem dobowym to 1 440 obrotów na zakład. Zmierzenie tego należy do WP14 (`bench_production_2400_lines`) | To jest cena zapłacona **świadomie za tolerancję 0 spójności LOD** (§7.6): ta sama funkcja wywołana sześćdziesiąt razy po minucie i raz po godzinie musi dać ten sam wynik co do grama i grosza, łącznie z ciągiem losowań awarii — a strumień RNG jest kluczowany absolutną minutą. Droga wyjścia (skok do najbliższego zdarzenia: koniec szarży, koniec przezbrojenia, przegląd) wymaga **dowodu, że rozkład awarii się nie zmienił**, i tego dowodu nie da się napisać bez benchmarku. Test `spojnosc_lod_ma_tolerancje_zero` jest strażnikiem tego, czego nie wolno przy tym zepsuć |
| AG-3 | **Trzy liczby do snapshotu renderu (§6.4.3) są policzone i wystawione:** `PlantSite::activity()`, `PlantSite::is_fault()` (bit `SITE_FAULT` — awaria linii **albo** odcięty licznik) oraz `plant::stock_fill(store, site)` (`max` ze stosunku masy i objętości, z wyłączeniem półki). Skalę emisji daje `Tuning::emission_scale(pm_g_per_min)` z saturacją, a `PlantSite::emissions.pm_g_last_minute` jest jej wejściem | WP13 rysuje panel, a M11 czyta snapshot — obie strony mają te liczby zastane, nie do policzenia. `smoke_kind` bierze się z `Emissions::plume` receptury **aktualnie uruchomionej** na linii i jest deklarowany w danych, nie wyliczany ze stosunku `pm_g` do `co2_g`; to kontrakt z M11 i M6a go już dotrzymał |
| AG-4 | **Powody decyzji zakładu są w pierścieniu ośmiu wpisów per zakład** (`PlantSite::reasons()`), a nie w globalnym dzienniku. Karta inspekcji zakładu czyta stamtąd | Pierścień, bo pamięć zakładu nie ma rosnąć przez sto lat gry, a karta pokazuje ostatnie kilka zdarzeń. Osiem wpisów × 9 000 zakładów to ~2,3 MB — mieści się w budżecie §7.4 bez zabiegów. Zapisane, bo WP13 jest pierwszym konsumentem i mógłby zakładać, że historia jest pełna |
| AG-5 ★ | **Trzy warianty `DecisionReason` fazy M6 są nadane i wieczne:** `Shortage = 400`, `ProductionHalted = 401`, `SubstituteUsed = 402`. Mają ramiona w `engine/ui` i klucze w `pl.ron` oraz `en.ron`. Pozostałe trzy z §6.1 (`SupplierChosen`, `ContractSigned`, `ExportChosen`) dopisuje **M6c** na końcu bloku 403–499 | Numer dyskryminanty wchodzi do zapisu gry i do kronik M9, więc raz nadany nie zmienia się nigdy. Zapisane tutaj, bo bramka 5 fazy („każda decyzja zapisuje `DecisionReason` i renderuje się w karcie inspekcji") zamyka się na poziomie fazy, czyli po tej podfazie, a nie po M6b |

---

## Zmiany wpisane po M6c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6c.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AJ-1 ★ | **Zasobów jest cztery, nie trzy.** `AG-1` wylicza `Store`, `Plant` i `Transport`; od M6c dochodzi **`B2b`** (zapytania, oferty, kontrakty, węzły graniczne, indeks dostawców i siedmiodobowe okno cen spot). Wszystkie cztery wchodzą do hasha przez `World::register_resource_hash`, żaden przez `register_arena_hash` (`K-29`, `AD-7`, `AI-2`) | Rynek trafił do jednego zasobu z tego samego powodu co partie i sloty: rozstrzygnięcie zapytania podpisuje kontrakt, wystawia zlecenie transportowe i sięga do areny partii w jednym kroku, a `World::resource_mut` pożycza **cały** świat. Liczba „trzy" w `AG-1` jest po M6c po prostu nieaktualna, a to jest ta klasa liczby, którą się czyta jako listę do odhaczenia |
| AJ-2 ★ | **Lista „co konkretnie zostaje do zrobienia" z `AG-1` pomija cały rynek B2B.** Dochodzą trzy systemy, wszystkie już wypisane w §5.11 i wszystkie mające gotową logikę po stronie czystych funkcji: `RfqSystem` (`B2b::open_rfq` + `B2b::resolve_due`, `EveryHour`), `ContractDeliverySystem` (`B2b::run_contracts`, `EveryHour`) i `ImportExportSystem` (`B2b::poll_imports`, `B2b::try_export`, `B2b::absorb_exports`, `B2b::roll_day`, `EveryDay`) | `AG-1` pisano po M6b, czyli zanim rynek istniał, więc lista była wtedy kompletna. Zapisane, bo `AG-1` czyta się jak listę kontrolną startu M6e, a pominięty system nie daje o sobie znać kompilacją — daje o sobie znać tym, że zapytania ofertowe nigdy się nie rozstrzygają i kaskada stoi na `SpotSearch` w nieskończoność |
| AJ-3 ★ | **Kolejność w DAG, której §5.11 nie deklaruje, a `B2b` wymaga:** `ShortageCascadeSystem` → `RfqSystem` (rynek konsumuje akcje wyprodukowane przez kaskadę w tej samej godzinie, `AG-3` z M6c) oraz `TransportArrivalSystem` → `ImportExportSystem` (`absorb_exports` wypuszcza poza miasto **to, co dojechało** do magazynu węzła, więc musi biec po rozładunku) | To jest ta sama klasa kontraktu co `SpoilageSystem` przed `RetailSystem` z `D12`: zależność nie jest widoczna w typach, tylko w treści, a odwrócona kolejność nie wywala się — daje o godzinę starsze dane. Przy pierwszej parze skutkiem byłoby `SpotSearch` z `RfqId` z poprzedniej godziny; przy drugiej eksport wypuszczany dobę po rozładunku, czyli zawyżony stan magazynu węzła w każdej migawce |
| AJ-4 | **`AG-5` jest wykonane:** `SupplierChosen = 403`, `ContractSigned = 404` i `ExportChosen = 405` **istnieją** — mają dyskryminanty, ramiona w `engine/ui/src/inspect/reason.rs`, klucze w `pl.ron` i `en.ron` oraz wpisy w `wszystkie()`, więc bramka 5 fazy jest po stronie M6 domknięta co do treści. Blok M6 `DecisionReason` jest **zamknięty**: 400–405, nic więcej nie dochodzi | `AG-5` mówiło „dopisuje M6c" w czasie przyszłym. Zapisane, bo rejestr długu R1 (pozycja 37) wiąże z tym rozmiar `fn describe`: 319 przed M6b, 355 po M6b, **397 po M6c** — i to jest liczba, którą `struct_guard.py` sprawdza dosłownie, więc zmiana w `describe` bez zmiany rejestru zapali błąd |
| AJ-5 | **Benchmark `bench_rfq_600_open` z §7.4 dokumentu fazy nie jest nigdzie w M6e nazwany**, choć `AG-7` przypisał do WP14 trzy pozostałe. Dochodzi do WP14 razem z nimi, z tego samego powodu: dopiero tu jest miasto, na którym da się go uruchomić | Sufit, który trzeba będzie zmierzyć, jest przy tym już znany i nazwany w kodzie: `SellerIndex` **przebudowuje się w całości** (`EveryDay`), a `collect_quotes` przechodzi liniowo po dostawcach towaru z sufitem `max_candidates`. Przy 9 000 zakładów i ~600 otwartych zapytaniach oba są rzędu 10⁴ operacji na dobę gry i nie ma czego optymalizować — ale to jest teza do zmierzenia, nie do przyjęcia na słowo |

---

## Zmiany wpisane po M6d

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6d.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AO-1 ★ | **Zasób jest **jeden**, nie cztery: `Chain` (`magnat_supply::ChainHandle`).** `AJ-1` wyliczało `Store`, `Plant`, `Transport` i `B2b` jako cztery osobne wpięcia przez `register_resource_hash`; wszystkie cztery siedzą teraz w jednym zasobie i haszują się w tej kolejności jako jedna sekcja | `World::resource_mut` pożycza **cały** świat, a rozstrzygnięcie zapytania ofertowego podpisuje kontrakt, wystawia zlecenie i ładuje partię w jednym kroku. Cztery zasoby znaczyłyby cztery bufory komend i rozbicie tego kroku na cztery ticki. To jest rozszerzenie `AD-7` i `AG-2` o piętro wyżej; `K-29` dopuszcza je wprost (`AL-11`) |
| AO-2 ★ | **Kadencja łańcucha istnieje, ale jako rusztowanie, nie jako osiem systemów z §5.11.** `Chain::step_minute`, `step_hour` i `step_day` robią to, co §5.11 opisuje, i są wołane z `MarketSystem` (M5) — bo bez nich WP11 nie miał jak się wydarzyć: półka bierze towar z magazynu, a do magazynu nic nie dojedzie, dopóki ktoś nie ruszy zlecenia | **To jest praca, która została dla WP14, i warto wiedzieć, na czym stoi.** Do zrobienia: rozbicie na systemy z własnymi częstotliwościami, rozproszenie po indeksie encji (`i % 60`, `i % 1440` — §7.4), własny `SystemId` i jawne krawędzie DAG. Kolejności **już zagwarantowane** i niepodlegające zmianie: kaskada przed rynkiem (`AJ-3`), psucie przed detalem (`D12`), przybycia przed eksportem. Trzy funkcje kadencji zostają jako ciało systemów — M6e ma je opakować, a nie napisać drugi raz |
| AO-3 ★ | **Zakłady produkcyjne wciąż nie powstają z danych miasta i to jest największa pozycja wejścia M6e.** Etap 7 stawia `SiteSeed` z recepturami, ale nikt nie buduje z nich `PlantSite`: nie ma linii, obsady, liczników mediów ani zapasu startowego. Sklepy biorą do WP11 towar **przez bramę graniczną** | Bilans masy domyka się bez ani jednego grama z powietrza, więc kryterium WP11 jest spełnione — ale łańcuch referencyjny z §7.1 (pole → elewator → młyn → piekarnia → sklep) nie ma dziś ani jednego ogniwa produkcyjnego w mieście. Do zrobienia razem z `data/scenarios/initial_stock.ron` (`R2` w §8). Dwie pułapki, obie zmierzone: **(a)** `SiteSeed` nie niesie `DepositId`, więc kopalnia nie wie, z czego kopie (`AL-17`); **(b)** `ProductionLine::new` zostawia `next_maintenance` na „nigdy", więc linia pracująca bez przerwy staje po 166 dobach na awarii, której nie ma czym naprawić (`AL-16`) |
| AO-4 ★ | **`RoadFreight` czeka na wpięcie, a most go nie dostaje.** Wtyczka towarowa M4 istnieje (`magnat_traffic::freight`), liczy prawdziwe kilometry po grafie na profilu `HeavyDay` i ma testy; `magnat_headless::retail::setup` nadal wstrzykuje atrapę `FlatRateFreight`, bo dostaje `Box<dyn TravelOracle>`, a nie `Arc<TrafficOracle>` | Rozszerzenie sygnatury mostu to zmiana w miejscu, w którym świat składa się w całość — czyli dokładnie ta robota, którą `AJ-2` przypisuje M6e. Do wpięcia potrzebna jest jeszcze mapa `SiteId → WorldCoord` dla ramp; `RoadFreight::new` bierze ją jako argument właśnie dlatego, że składający świat ją ma, a `sim/supply` nie |
| AO-5 | **Benchmarki z `AG-7` i `AJ-5` mają pierwsze liczby odniesienia, choć nie są zmierzone.** `Chain::step_minute` przy 60 sklepach i zerowej produkcji nie jest widoczny w przebiegu `m5shop` (6 dób, 3 tys. mieszkańców), a `SellerIndex::rebuild` przy zerowej liczbie dostawców jest darmowy | Zapisane, bo obie liczby urosną **skokowo** razem z `AO-3`: 800 zakładów produkcyjnych to 800 wywołań `advance_production` na minutę i przebudowa indeksu dostawców po 2 400 liniach. Dzisiejszy brak sygnału nie jest pomiarem i nie wolno go czytać jako „mieści się w budżecie" |
| AO-6 ★ | **Złoża wchodzą do produkcji parametrem, nie zasobem — i to jest do rozstrzygnięcia w M6e.** `Chain::step_minute` bierze `&dyn Deposits` i to działa; `ChainHandle::deposits` wymaga natomiast `Arc<dyn Deposits + Send + Sync + 'static>`, a wtyczka M1 (`magnat_world::DepositLedger`) jest adapterem **pożyczającym** (`RefCell<&mut [Deposit]>`) i żadnego z tych trzech warunków nie spełnia | Pożyczka jest poprawna w miejscu, w którym się jej używa: system wyłączny (`K-21`) ma `&mut World`, więc `&mut [Deposit]` jest w zasięgu ręki i nic się nie ściga. Wpięcie złóż **do uchwytu** wymaga natomiast odpowiedzi na pytanie, gdzie mieszka `WorldData::deposits`, kiedy sięga po nie dwóch właścicieli — a to jest pytanie o M1, nie o M6. Do czasu rozstrzygnięcia `ChainHandle::deposits` stoi na `NoDeposits` i kopalnie stoją razem z nim; zakłady przetwórcze nie są tym dotknięte, bo złoża nie mają |
