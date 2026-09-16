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

---

## Zmiany wpisane po M6e

Zgodnie z `K-18`. Podfaza domyka fazę, więc ta tabela jest jednocześnie listą rzeczy,
których plan M6e nie przewidział, a które okazały się warunkiem jego kryteriów.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AP-1 ★ | **`AO-6` rozstrzygnięte podziałem liczby, nie tablicy.** `DepositLedger` przestaje pożyczać `&mut [Deposit]` i **posiada** własny bilans wydobycia (`mined`, `at_start`, `reserves`), a `CityData` niesie go jako `Arc`. `Deposit::extracted` zostaje tam, gdzie był, i znaczy **wyczerpanie sprzed startu świata** — parametr generacji, który po niej nie drga. Port `Deposits` dostaje supertrait `HashState`, a `ChainHandle::hash_state` haszuje łańcuch **i** złoża | `AO-6` pytało, gdzie mieszka `WorldData::deposits`, kiedy sięga po nie dwóch właścicieli. Odpowiedź: nigdzie się nie przeprowadza, bo to **dwie różne liczby**. Historia świata jest geologią i wchodzi do hasha terenu razem z kształtem formacji; wydobycie w tej grze jest stanem symulacji i wchodzi do hasha stanu. Gdyby wydobycie zostało w `Deposit`, ten sam `Vec` byłby czytany przez `TerrainQuery::deposit(id) -> &Deposit` (referencja, więc bez zamka) i pisany przez kopalnię w każdej minucie — a to jest wybór między `RwLock` na gorącej ścieżce zapytań terenowych a drugą kopią tablicy. `DepositLedger` w `CityData` stoi tam z tego samego powodu co `catalog`: czyta go most stawiający gospodarkę, a most widzi miasto, nie teren |
| AP-2 ★ | **Zakład produkcyjny jest stroną rozliczeń: `Market` dostaje rejestr `plants` (`SiteId → (FirmId, AccountId)`), a `absorb_settlements` umie zapłacić także z jego konta.** Zakład **nie** dostaje przy tym półki, ceny ani księgi zakładowej — te należą do M7 | Do M6d `absorb_settlements` znajdowało konto wyłącznie wśród sklepów, więc dostawa mąki do piekarni była po cichu pomijana: towar wjeżdżał do magazynu, a pieniądz nie wychodził. Dopóki jedynym kupującym był sklep, a jedynym sprzedawcą brama graniczna, ta luka nie miała jak się wydarzyć — `AO-3` otworzył ją w tej samej zmianie, w której postawił zakłady. Wspólna ścieżka „zakład jako sklep" odpada: dałaby osiemset sklepów bez klientów, które bramka **G6** policzyłaby jako koncentrację handlu |
| AP-3 | **Emisja startowa przestaje być stałą i skaluje się z liczbą zakładów Etapu 7** (`retail::emisja(sites)`) | Kapitał obrotowy dostaje teraz także zakład, a metropolia ma ich rząd tysiąca. Objaw był myląco odległy od przyczyny: `brak środków w granicach limitu debetu` przy zakładaniu **losowego** zakładu — tego, na którym konto reszty świata akurat się skończyło |
| AP-4 ★ | **Jeden system, nie piętnaście — i to jest korekta §5.11, nie skrót.** Powstaje `supply.Chain` (`Cadence::EveryMinute`, wyłączny, `before(economy.Market)`), a `Chain::step` robi w nim to, co §5.11 rozpisuje na piętnaście systemów. **Rozpraszanie po indeksie encji jest zrobione i jest istotą tej zmiany**: zakład `i` przegląda się w minucie `i % 60`, slot `i` scala się w minucie `i % 1440`, a `step_hour` i `step_day` są od tej chwili wołane **co minutę**. Fakty wychodzą z kroku skrzynką (`ChainTick`: odpisy, rozliczenia, faktury) i księguje je `MarketSystem` | Piętnaście systemów nad **jednym** zasobem `Chain` (`AO-1`) byłoby piętnastoma systemami **wyłącznymi** (`K-21`), bo system wyłączny stoi sam na swoim poziomie: piętnaście poziomów zamiast jednego, zero krawędzi DAG do wykorzystania, zero zrównoleglenia. To, co z §5.11 naprawdę decyduje o budżecie §7.4, to **częstotliwości i rozpraszanie** — i to jest zrobione. Bez rozpraszania dziewięć tysięcy przeglądów wypadało w jednej minucie na sześćdziesiąt, czyli 1,7 % ticków ponad budżet, a budżet mówi **p99**: nie domykałby się niezależnie od tego, jak szybki jest kod. **Pułapka, której plan nie widział:** rozproszenie samo w sobie psuje `Review::Periodic` — zakład o fazie 5 pytałby regułę „przegląd o 4:00" w minucie 245 i nigdy nie trafił, więc pięćdziesiąt dziewięć zakładów na sześćdziesiąt nie zamówiłoby **nigdy niczego**. Zegar reguły przesuwa się o fazę wstecz i dlatego każdy widzi ten sam 4:00 |
| AP-5 | **`Plant::bill_utilities` oddaje `Vec<UtilityBill>` zamiast trójek `(zakład, dostawca, kwota)`** (§6.1) | Księgujący potrzebuje **rodzaju medium**, bo `TxKind::Utility` go niesie. Bez niego rachunek zakładu pokazywałby prąd i wodę jako jedną pozycję „media" — czyli dokładnie tę informację, po którą gracz otwiera kartę zakładu, gdy rachunek urósł. Przy okazji: faktura idzie na konto **reszty świata**, bo sieci przesyłowe i ich właściciel to zakres M8; do tego czasu media przychodzą spoza miasta tak samo jak towar importowany i P1 domyka się bez zmian |
| AP-6 ★ | **`engine/ecs`: jawne `before`/`after` mają pierwszeństwo przed krawędzią z konfliktu dostępów.** Kolejność kroków w `ScheduleBuilder::build` odwrócona, krawędź konfliktu pomijana tam, gdzie deklaracja ustawiła parę w drugą stronę. Wchodzi do `00` jako **`K-42`** | Utajony błąd, nie moja niewygoda. Konflikt dostępów mówi „tych dwóch nie wolno puścić równolegle" i **nie ma zdania o kierunku** — kierunek brał się z kolejności kanonicznej, czyli z hasha nazwy. Dla pary systemów wyłącznych konflikt zachodzi **zawsze**, więc jawne `before` między nimi dawało cykl zawsze wtedy, gdy hash trafił odwrotnie — a trafiał losowo. Skutek: kontraktu `D12` (psucie przed detalem) **nie dało się wyrazić**, choć `before` istnieje dokładnie po to. Istniejące `economy.Market.before(agents.DayLoop)` działało wyłącznie dlatego, że hashe trafiły zgodnie |
| AP-7 ★ | **`Store::resell`: zmiana właściciela przeszacowuje koszt własny partii na cenę zapłaconą.** Wołane po `dispatch` w sprzedaży spotowej i kontraktowej; `cogs` rośnie o koszt sprzedawcy, `paid_in` o cenę kupującego | Do M6d kupujący dziedziczył **koszt wytworzenia sprzedawcy**, płacąc za towar cenę z marżą. Różnica to dokładnie marża i znikała z bilansu — `check_cost` przechodził, bo `paid_in` też jej nie widział, ale księga kupującego (`InventoryGoods`) rozjeżdżała się z wyceną magazynu o wartość **każdej lokalnej dostawy**. Znalazł to `P5` w przebiegu `m5shop`: 9 329 gr na jednym sklepie po ośmiu dobach. Ścieżka importowa robiła to poprawnie od M6c (`cost: paid + duty`), bo import **tworzy** partię; sprzedaż lokalna ją **przenosi** — i była jedyną drogą, którą do M6e nikt nie przeszedł, bo zakłady nie produkowały |
| AP-8 ★ | **Dziennik partii zapisuje wszystkie etapy, a nie jeden.** Do M6d `BatchLedger` znał wyłącznie `Produced`; pozostałe jedenaście wariantów `TraceKind` było martwe. Dochodzą: zapisy przy `put`/`take`/`split`/`load`/`unload`/`move_within_site`/`shelf_pick`/`write_off`/`spoil`, **krawędzie pokrewieństwa** (`BatchLedger::link`), dziedziczenie flagi `TRACED` w dół łańcucha oraz `Store::mark_traced` jako punkt wejścia gracza | Bez krawędzi pokrewieństwa ślad urywał się na pierwszym przetworzeniu: bochenek zna swoją historię od wyjęcia z pieca, a „od pola do półki" zaczyna się na polu, czyli w partii, której ten bochenek jest **wnukiem**. Trzy rzeczy, których plan nie przewidział, a bez których kryterium WP13 jest nieosiągalne: **(a)** etap `Consumed` musi nieść masę **wydaną**, a nie resztę zostającą w magazynie — inaczej oś czasu mówi „zużyto 0 kg mąki"; **(b)** stempel miejsca to slot, w którym etap się wydarzył, a nie `origin.site` — inaczej rozładunek w sklepie pokazuje piekarnię; **(c)** rodzice nie mogą znikać po pierwszym wyjściu receptury, bo rafineria ma ich osiem i ślad diesla urywał się na benzynie, czyli na wyjściu, które akurat stało pierwsze w pliku |
| AP-9 | **`SupplyContract::pay_penalty` jest metodą kontraktu, a `B2b::pay_penalty` wyłącznie odnajduje kontrakt** | Niezmiennik `prop_contract_penalty` („suma kar naliczonych równa sumie zapłaconych") jest własnością kontraktu i ma mieszkać przy danych, których dotyczy. Przy okazji da się go sprawdzić bez budowania rynku |
| AP-10 | **Zmierzone, a nie przyjęte na słowo** (`AG-7`, `AJ-5`, `AO-5`). Produkcja 2 400 linii: **117 µs** wobec 0,5 ms. Doba liczona minuta po minucie (sufit `AG-7`): **3,78 ms na 100 zakładów** — cena tolerancji 0 spójności LOD jest realna i widoczna. Rampy przy tempie z §7.4: **52 ns/tick** wobec 0,2 ms. Przegląd zapasów 9 000 zakładów, jedna faza: **42 µs** wobec 200 µs. Przebudowa indeksu dostawców: **1,85 ms** raz na dobę wobec 40 ms. Scalanie, jedna faza: **624 ns**. Ślad o dwunastu przodkach: **470 ns**. Pamięć: **600 tys. partii × 104 B = 59,5 MB** wobec budżetu 64 MB | Wszystko w budżecie, ale dwie liczby są warte zapamiętania. **104 B na partię, nie 88** z szacunku §7.4 — zapas do 64 MB wynosi siedem bajtów na partię, więc dopisanie jednego pola `u64` do `Batch` **wychodzi poza budżet**; pilnuje tego test `szescset_tysiecy_partii_miesci_sie_w_budzecie`. I drugie: pierwsza wersja benchmarku ramp dawała 4,6 ms wobec 0,2 ms, bo mierzyła jeden przyjazd na rampę **co minutę** — obciążenie nieosiągalne w grze (rampa przeciążona czterdziestokrotnie). Liczba była prawdziwa i nieistotna, czyli najgorszy rodzaj liczby w benchmarku. Sufit został zmierzony osobno (`dock_queue_saturated_100`): kolejka rampy trwale przeciążonej rośnie bez ograniczenia i koszt staje się kwadratowy |
| AP-11 ★ | **Zapas startowy zakładów to cztery liczby w `data/scenarios/initial_stock.ron`, a nie tabela czterystu towarów** (`R2`) | Proporcje wejść i wyjść są już rozstrzygnięte recepturą i skalą zakładu, którą KROK 5 domknięcia łańcuchów wyrównał do popytu. Jedynym wolnym parametrem jest **na ile dób bufor wystarcza**, a to jest jedna liczba na świat. Tabela per towar byłaby czterystoma okazjami do rozjechania się z recepturą i zerem nowej informacji. Zapas **nie jest źródłem w grafie** (§6.4.1) i walidator dalej go nie widzi |
| AP-12 | **`AL-17` wykonane: `SiteSeed` niesie `DepositId`.** Wynik sprawdzenia `zloze_ok` przestaje być wyrzucany — `zloze_pod` zwraca identyfikator, a `Przydzial` przenosi go do `SiteSeed::deposit`. Zakład wydobywczy postawiony przez **wypełniacz strefy** złoża nie dostaje i stoi na `Starved`; raport mostu liczy takie osobno (`PlantsReport::mines_without_deposit`) | Wypełniacz nie sprawdza `zloze_ok`, bo wybiera archetyp po strefie, a nie po łańcuchu — i to jest zachowanie zamierzone: „wypełniacz strefy przemysłowej naprawdę nie ma czego kopać" (§5.10). Różnica wobec M6d jest taka, że teraz **widać**, ilu takich jest, zamiast domyślać się z braku wydobycia |
| AP-13 ★ | **Panel łańcucha (WP13) jest kartą tekstową ze złotym testem, a nie widokiem w kliencie.** `SupplyCard` + `SupplyView` w `engine/ui`, widget `widgets::supply_card`, 46 kluczy `ui.supply.*` i `ui.loss.*` w `pl.ron` **i** `en.ron`. Do klienta graficznego **nie jest wpięty** i to jest świadome | `Selection` nie ma wariantu dla zakładu i dołożenie go jest zadaniem M9 — zapisała to już tabela po M5e w dokumencie M9 (`Y-2`). Panel przychodzi do M9 gotowy: model, wydruk, widget i test w obu językach; brakuje wyłącznie kliknięcia, które go otworzy. Wpięcie go tutaj znaczyłoby zaprojektowanie zaznaczenia zakładu w fazie, która nie jest jego właścicielem |
