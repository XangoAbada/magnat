# M4d — Mikro i dowód spójności

Podfaza 4 z 4 fazy **M4 — Ruch** (`M4-ruch.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M4b (mezo), M4c (wybór środka). |
| **Pakiety robocze** | WP8, WP9, WP11, WP12 |
| **Projekt techniczny** | §5.4, §5.8, §5.9 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: korki widoczne i mierzone, „kamera nie zmienia świata”. |
| **Kryterium zamknięcia** | Kryteria WP8, WP9, WP11 i WP12 oraz bramki 1–7 fazy M4 w `00-postep.md`. |
| **Poprzednia / następna** | `M4c-wybor-srodka-parkingi-komunikacja.md` · — (ostatnia w fazie) |

Warstwa mikro (IDM, MOBIL, sygnalizacja) z dostępem read-only do stanu ekonomicznego, harness równoważności mikro↔mezo z kalibratorem, nakładki UI i domknięcie wydajnościowo-determinizmowe.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP8** | Warstwa mikro | WP3 | IDM (car-following), MOBIL (zmiana pasa), sygnalizacja (fazy, cykl), ronda, pierwszeństwo. Aktywna tylko dla krawędzi w LOD Mikro. **Dostęp read-only do `LinkState` i `TripLedger`** — wymuszone deklaracją systemu w schedulerze. Serwo domykające czas przejazdu do wartości zaksięgowanej. | Ruch w kadrze wygląda poprawnie: kolejki przed światłem, wjazdy na rondo, wyprzedzanie; deklarowany zbiór zapisów systemu mikro nie zawiera żadnego komponentu ekonomicznego (test) |
| **WP9** | **Dowód spójności mikro↔mezo** | WP8 | Harness `micro_mezo_equivalence` + kalibrator offline parametrów IDM ↔ VDF w `tools/balansator`. Szczegóły w §5.4 i §7.2. | Twarde asercje (pieniądz, paliwo, minuta przybycia) z tolerancją 0; miernik dryfu kalibracji w normie; test „kamera nie zmienia świata" zielony |
| **WP11** | Nakładki UI i inspekcja | WP3, WP7, WP10 | Nakładki: natężenie, korki, izochrony, parkingi, obciążenie linii. Karta inspekcji podróży i pojazdu. Filtr „pokaż tylko X" (§14.2). | Nakładki działają na snapshocie double-buffered, bez blokowania symulacji; przełączanie nakładki < 1 klatka |
| **WP12** | Wydajność i determinizm | wszystkie | Profilowanie, chunkowanie jobów, budżety, hash stanu ruchu w funkcji haszującej ECS, benchmarki criterion. | Budżety z §7.3 dotrzymane; dwa przebiegi tego samego seeda = identyczny ciąg hashy przez 100 dni gry |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 **Spójność mikro ↔ mezo — architektura i dowód**

To jest kluczowa sekcja fazy. Kontrakt (dok. 00 §4, PRD §17.4) brzmi: *wynik ekonomiczny nie zależy
od poziomu LOD; test przebiegu mikro i mezo → identyczne salda pieniężne, tolerancja 0.*

#### Argument wymuszający architekturę

Zbiór krawędzi w LOD Mikro zależy od **kamery gracza**. Kamera nie jest częścią symulacji i nie
wchodzi do hasha stanu. Gdyby warstwa mikro wpływała na wynik ekonomiczny, obrócenie kamery
zmieniłoby salda gospodarstw domowych — determinizm (dok. 00 §3) przestałby obowiązywać.
Stąd jedyna architektura, która spełnia oba kontrakty naraz:

> **Warstwa mezo jest jedynym źródłem prawdy. Warstwa mikro jest ograniczonym wizualizatorem
> z zakazem zapisu do stanu symulacji.**

Mezo działa **zawsze i dla każdej krawędzi**, niezależnie od LOD. Mikro nie zastępuje mezo —
dokłada się do niej dla krawędzi widocznych.

#### Wspólny model kosztu przejazdu

Jedna funkcja czysta, wywoływana identycznie w obu LOD (w mikro — przez warstwę mezo pod spodem):

```rust
/// JEDYNE miejsce, w którym powstaje czas przejazdu, zużycie paliwa i koszt pieniężny.
/// Czysta funkcja: te same argumenty → ten sam wynik, zawsze.
pub fn settle_edge(
    edge: &RoadEdge,
    link: &LinkState,                 // stan krawędzi w minucie wjazdu
    veh: &VehicleSpec,
    load: Mass,
    entry: SimMinute,
    stops_at_entry_node: u8,
) -> LedgerEntry;

pub fn settle_node(
    turn: &TurnMovement,
    node: &NodeState,                 // długość kolejki ruchu skrętnego, faza sygnalizacji
    entry: SimMinute,
) -> (SimMinute /*wyjazd*/, u8 /*zatrzymania*/);
```

Model czasu (mezo i mikro identycznie):

1. **Krawędź** — prędkość z diagramu podstawowego (VDF) na skwantowanej gęstości:
   `mean_speed_dkmh = vdf(free_flow, capacity, occupancy)`, wynik zaokrąglany do decykm/h
   **przed** użyciem gdziekolwiek indziej. Czas = `length_cm / mean_speed` w setnych sekundy,
   zaokrąglany `div_round_half_up` do minut w ledgerze.
2. **Węzeł** — opóźnienie z modelu kolejkowego ruchu skrętnego: `delay = f(przepływ/przepustowość,
   udział zielonego, długość kolejki)`; liczba zatrzymań z tego samego modelu.
3. **Spillback** — gdy `EdgeQueue.occupancy == storage_capacity`, wjazd jest wstrzymany i kolejka
   propaguje się w górę; to samo zjawisko w mikro widać jako korek blokujący skrzyżowanie.

Model paliwa (§9.5), całkowitoliczbowy:

```
fuel_ml = base_ml_per_100km(class)
        * dist_cm / 100_000
        * speed_factor(mean_speed_dkmh)      // tabela z data/, indeksowana kubełkiem prędkości
        * load_factor(load, kerb_mass)
        * grade_factor(grade_permille)
        / 1_000_000                          // skala mnożników: promile × promile
        + stops * idle_ml_per_stop(class)
        + cold_start_ml (tylko pierwsza krawędź podróży)
```

Wszystkie mnożniki to `u32` w promilach, wszystkie dzielenia przez `div_round_half_up`.
`mean_speed_dkmh` jest już skwantowane, `dist_cm` całkowite — **we wzorze nie ma ani jednego floata**,
więc wynik jest identyczny na każdej platformie i w każdym LOD z definicji, nie z tolerancji.

Koszt pieniężny przejazdu: `fuel_ml × unit_price_gr_per_l / 1000` + opłaty (parking, bilet, myto)
+ amortyzacja (`wear_gr_per_100km × dist`). Podział kwoty na strony wg reguły z dok. 00 §2.

#### Co robi warstwa mikro

| Może | Nie może |
|---|---|
| czytać `LinkState`, `TripLedger`, `RoadGraph` | zapisywać czegokolwiek z `sim/*` poza własnym `VehicleState` |
| integrować pozycje, pasy, akceleracje (IDM/MOBIL) | zmieniać `booked_exit` |
| odgrywać cykl sygnalizacji i pierwszeństwo | zmieniać `EdgeQueue`, `occupancy`, `LinkState` |
| raportować zmierzony diagram podstawowy do devtools | karmić tym pomiarem symulację w czasie gry |

Zakaz jest **wymuszony przez scheduler, nie przez dyscyplinę**: system `micro_step` deklaruje
`Read<LinkState> + Read<TripLedger> + Write<VehicleState>`. Test WP9 asercją sprawdza deklarowany
zbiór zapisów systemu mikro — dopisanie tam komponentu ekonomicznego wywala CI.

**Serwo.** Mikro dostaje `booked_exit` i domyka do niego czas przejazdu przez korektę pożądanej
prędkości IDM (`v_desired *= clamp(remaining_booked / remaining_micro, 0.85, 1.15)`). Korekta jest
w praktyce bliska 1, bo parametry IDM są kalibrowane offline do VDF (niżej). Gdy pojazd wyjedzie
wcześniej — czeka na linii wyjazdowej; gdy się spóźni — krawędź zwalnia go dopiero wizualnie,
ledger pozostaje nietknięty. **Wielkość korekty jest metryką jakości, nie poprawności.**

#### Kalibracja offline (tu mieszka gałka strojenia)

Diagram podstawowy IDM ma postać zamkniętą: dla stanu równowagi odstęp
`s_e(v) = s0 + v·T / sqrt(1 − (v/v0)^4)`, więc gęstość `k(v) = 1/(s_e(v) + l)` i przepływ `q = k·v`.
VDF w `data/roads/vdf.ron` **jest wyprowadzony z tych parametrów**, nie dopasowany osobno.
`tools/balansator` ma zadanie `calibrate-vdf`: dla każdej klasy drogi przepuszcza mikro na
jednorodnym pierścieniu przy rosnącej gęstości, mierzy realny `q(k)` i przelicza tabelę VDF.
Uruchamiane ręcznie przy zmianie parametrów IDM, wynik commitowany do `data/`. To jest jedyny
kanał mikro → symulacja i jest **offline, powtarzalny i widoczny w diffie**.

#### Test spójności — `micro_mezo_equivalence` (WP9)

Scenariusz referencyjny: siatka 20×20 + jedna arteria + 3 skrzyżowania z sygnalizacją i 1 rondo;
5 000 pojazdów; skryptowane pary origin–dest i minuty odjazdu; 3 godziny gry; stały seed.

| # | Przebieg | Asercja | Tolerancja |
|---|---|---|---|
| A1 | `ForceMezo` vs `ForceMicro` | dla każdej podróży: `ledger.total_fuel` | **0** |
| A2 | jw. | dla każdej podróży: `ledger.total_money` | **0** |
| A3 | jw. | dla każdej podróży: minuta przybycia | **0** |
| A4 | jw. | suma gotówki wszystkich GD; hash komponentów pieniężnych i `FuelTank` | **0** |
| A5 | `ForceMezo` vs przebieg ze skryptowaną ścieżką kamery przełączającą LOD w trakcie podróży | ciąg hashy stanu ECS co 1000 ticków | **identyczny** |
| B1 | `ForceMicro` | **zmierzony** (nie zaksięgowany) czas przybycia vs zaksięgowany: średni błąd względny | ≤ 3 % |
| B2 | jw. | 95. percentyl błędu względnego | ≤ 12 % |
| B3 | jw. | błąd ze znakiem (systematyczne obciążenie) | ≤ 1 % |
| B4 | jw. | test Kołmogorowa–Smirnowa rozkładu czasów przejazdu mikro vs rozkład generowany przez VDF | p > 0,01 |

A1–A5 są **twarde i blokujące** — i prawdziwe *z konstrukcji*, nie przez szczęście: obie ścieżki
wołają tę samą `settle_edge` na tym samym `LinkState`. Ich rolą jest bycie strażnikiem regresji:
pierwsza osoba, która „dla realizmu" podepnie mikro pod ekonomię, zobaczy czerwony CI.

B1–B4 to **miernik dryfu kalibracji** — mierzy, czy animacja wciąż opowiada tę samą historię, co
księgowość. Blokujący w buildzie nocnym, ostrzegawczy w PR. Jego wywalenie się jest sygnałem
„uruchom `calibrate-vdf`", nie „popraw kod ekonomii".

**Świadomie przyjęty sufit.** Mikro nie odkrywa korków, których mezo nie widzi — mezo widzi
wszystkie krawędzie, więc nie ma czego odkrywać, ale wierność zjawisk lokalnych (fala zatrzymań,
blokowanie skrzyżowania przez pojazd stojący w korku) żyje tylko w mikro i **nie wpływa na koszt**.
Jeżeli w M12 profilowanie pokaże, że to zubaża rozgrywkę, ścieżką rozwoju jest podniesienie
wierności *mezo* (drobniejsze kubełki gęstości, jawny model blokowania węzła), nie oddanie
ekonomii w ręce mikro.

### 5.8 Systemy ECS i częstotliwości

| System | Crate | Częstotliwość | Odczyt / zapis |
|---|---|---|---|
| `nav::cch_customize_step` | nav | `EveryMinute` (budżet pracy) | R `RoadGraph` / W `ChGraph` (bufor) |
| `nav::cch_recontract_step` | nav | `EveryMinute` (budżet pracy) | jw. |
| `nav::route_service_drain` | nav | `EveryMinute` | R `ChGraph`, `RouteCache` / W `RouteCache` |
| `nav::travel_time_matrix_update` | nav | `EveryHour` | R `TripLedger` / W `TravelTimeMatrix` |
| `traffic::mezo_node_step` | traffic | `EveryMinute` | R/W `NodeState`, `EdgeQueue` |
| `traffic::mezo_edge_step` | traffic | `EveryMinute` | R/W `EdgeQueue`, `LinkState`; W `TripLedger`, `FuelTank` |
| `traffic::trip_dispatch` | traffic | `EveryMinute` | R `TripRequest` / W `EdgeQueue`, kolejka DES |
| `traffic::transit_dispatch` | traffic | `EveryMinute` | R `Timetable` / W `TransitRun` |
| `traffic::transit_boarding` | traffic | `EveryMinute` | R/W `TransitStop`, `TransitRun` |
| `traffic::parking_expire` | traffic | `EveryMinute` | R/W `ParkingLot` |
| `traffic::vehicle_wear` | traffic | `EveryDay` | R `TripLedger` / W `VehicleCondition` |
| `traffic::micro_step` | traffic | **`Every100ms`**, tylko LOD Mikro | **R** `LinkState`, `TripLedger`, `RoadGraph` / **W** `VehicleState` |
| `traffic::lod_select` | traffic | co klatkę renderu (poza symulacją) | R kamera, `LinkState` / W zbiór mikro |
| `agents::mode_choice` | agents (rozszerzenie) | zdarzeniowo (`TripRequest`) | R `NavServices`, GD / W `ModeDecision` |

`traffic::lod_select` celowo **nie jest systemem symulacji** — działa po stronie renderu i nie ma
prawa zapisu do `sim/*`. To formalne odzwierciedlenie §5.4.

### 5.9 Determinizm

1. **Kolejność aktualizacji mezo.** Iteracja po krawędziach po `EdgeId` rosnąco (`Vec`, nie mapa).
   Wewnątrz krawędzi kolejka `EdgeQueue` jest kopcem po kluczu `(exit_minute, vehicle_entity_index)` —
   klucz jest totalnym porządkiem, remisy niemożliwe.
2. **Kolejność aktualizacji mikro.** Cztery fazy na tick 100 ms:
   - **A (równolegle, read-only):** dla każdego pojazdu policz pożądane przyspieszenie (IDM) i chęć
     zmiany pasa (MOBIL), zapisz do scratchu per pojazd. Czyta poprzedni, zamrożony bufor pozycji.
   - **B (rozstrzyganie konfliktów, deterministyczne):** zgłoszenia zmiany pasa i wjazdu na
     skrzyżowanie sortowane kluczem
     `(node_index, turn_index, priority_class, distance_to_stopline_cm, vehicle_entity_index)`.
     Przetwarzane sekwencyjnie w tym porządku; równoległość po węzłach jest dozwolona, bo klucz
     zaczyna się od `node_index`, a węzły są rozłączne.
   - **C (równolegle):** integracja pozycji + serwo do `booked_exit`.
   - **D:** zamiana buforów; komendy strukturalne posortowane po `(SystemId, entity_index)` (dok. 00 §3.4).
3. **Pierwszeństwo bez wyścigu.** `priority_class` daje porządek częściowy z reguł ruchu:
   sygnalizacja (faza zielona wygrywa) → rondo (pojazd na pierścieniu wygrywa) → znaki
   (droga główna wygrywa) → równorzędne (reguła prawej ręki, wyprowadzona z kąta krawędzi).
   Remis w tej samej klasie rozstrzyga odległość do linii zatrzymania, potem `entity_index`.
   **W żadnym miejscu nie pytamy o zegar, o identyfikator wątku ani o kolejność ukończenia jobów.**
4. **Floaty.** Dozwolone w IDM/MOBIL/geometrii (dok. 00 §2 — fizyka ruchu). Zabronione na ścieżce
   ledgera: `mean_speed_dkmh` jest kwantowane przed wejściem do `settle_edge`, a `settle_edge`
   i wzór paliwowy są w całości całkowitoliczbowe. Granica jest jedna i jest nią sygnatura
   `settle_edge` — łatwa do pilnowania w review.
5. **Kolekcje.** `RouteCache` na `SeededMap`; `reservations` i `departures` na kopcach z totalnym
   kluczem; nigdzie iteracji po `HashMap`.
6. **RNG.** Nowe warianty `StreamId`: `ModeChoice`, `FuelStationChoice`, `ParkingSearch`,
   `TransitDwell`, `VehicleBreakdown`. Dopisywane na końcu enuma, istniejące wartości nietknięte.
7. **Hash stanu.** Do funkcji haszującej ECS dochodzą: `FuelTank`, `VehicleCondition`,
   `VehicleLocation`, `EdgeQueue.occupancy`, `LinkState.mean_speed_dkmh`, `ParkingLot.occupied`,
   `TransitRun.{occupancy, delay_minutes}`. **`VehicleState` (mikro) nie wchodzi do hasha** — to
   formalny zapis tego, że mikro jest wizualizacją.

---
