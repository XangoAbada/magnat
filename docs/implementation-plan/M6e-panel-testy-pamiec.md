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
