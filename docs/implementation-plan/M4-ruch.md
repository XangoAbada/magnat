# M4 — Ruch

Status: plan wykonawczy fazy.
Dokument nadrzędny: `00-konwencje-i-kontrakty.md` — typy bazowe, determinizm, LOD i szablon
pochodzą stamtąd i **nie są tu redefiniowane**.
Źródło wymagań: `PRD_Magnat.md` §9 (całość), §5.5–5.6, §14.2, §17.4, §17.6, §19, §20.2.

---

## 1. Cel fazy i artefakt końcowy

Po M4 miasto z M2, zaludnione przez agentów z M3, **żyje ruchem**: mieszkańcy nie teleportują się
do pracy, lecz wybierają środek transportu, jadą realną siecią, stoją w korkach, szukają miejsca
parkingowego, tankują i spóźniają się, gdy autobus jest pełny.

Artefakt uruchamialny (`tools/headless` + widok 3D):

1. **Pojazdy na drogach.** Ruch poranny i popołudniowy szczyt widoczny w kadrze jako pojedyncze
   pojazdy (car-following, zmiana pasa, sygnalizacja), poza kadrem jako przepływy na krawędziach.
2. **Nakładki danych** (§14.2): natężenie ruchu, korki (prędkość / prędkość swobodna), izochrony
   czasu dojazdu z wybranego punktu, obłożenie parkingów, obciążenie linii komunikacji.
3. **Karta inspekcji podróży** (§14.1/§14.4): dla dowolnego mieszkańca — którą opcję transportu
   wybrał i **dlaczego** (koszt uogólniony wszystkich rozważanych opcji w groszach), trasa,
   realny czas, spalone paliwo, koszt pieniężny podróży.
4. **Komunikacja miejska**: linie z rozkładem, taborem i kierowcami-mieszkańcami; przepełnione
   pojazdy zostawiają pasażerów na przystanku.
5. **Dowód spójności LOD** jako zielony test w CI: ten sam scenariusz w mikro i w mezo daje
   **bit w bit identyczne** salda pieniężne i zużycie paliwa (§17.4, dok. 00 §4).
6. **Benchmark**: 50 000 pojazdów w ruchu, doba gry w trybie 50× mieści się w budżecie z §20.2.

Zdanie testowe fazy: *„Anna wyjeżdża o 07:20, tankuje po drodze, stoi 6 minut w korku na Wołoskiej,
parkuje 180 m od biura i wchodzi do pracy o 07:58 — a gdy odwrócę kamerę, przyjeżdża o tej samej
minucie, za te same pieniądze."*

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi (crate'y `engine/nav` i `sim/traffic` — właściciel: M4)

| Obszar | Zakres |
|---|---|
| Grafy transportu | `RoadGraph` (pasy, klasy, limity, tonaż), graf pieszy, rowerowy, szynowy; mosty i ich przepustowość |
| Routing | CCH (Customizable Contraction Hierarchies) + przebudowa w tle, A\* hierarchiczny (pieszy/rower), `RouteCache` dom↔praca, `TravelTimeMatrix` dzielnica × godzina |
| Ruch mezo | `LinkState`, `EdgeQueue`, model węzła (przepustowość ruchów skrętnych), rozliczanie przejazdu |
| Ruch mikro | car-following (IDM), zmiana pasa (MOBIL), sygnalizacja, ronda, pierwszeństwo — **warstwa wizualna** (patrz §5.4) |
| Parkingi | `ParkingLot` z pojemnością, rezerwacja, promień dojścia, wpływ na wykonalność podróży autem |
| Komunikacja miejska | `TransitLine`: trasa, przystanki, rozkład, tabor, kierowcy-mieszkańcy, pojemność i przepełnienie |
| Wybór środka transportu | §9.4 — koszt uogólniony w `Money`, rozszerzenie `sim/agents` |
| Paliwo i energia | §9.5 — bak, zużycie per przejazd, tankowanie jako zadanie w planie dnia |
| Pojazdy jako encje | własność GD/firmy, stan techniczny, zużycie, amortyzacja |
| UI | nakładki: ruch, korki, czasy przejazdu, obłożenie parkingów; karta inspekcji podróży |

### Nie wchodzi

| Obszar | Faza |
|---|---|
| Sieci przesyłowe prądu, gazu, wody, ciepła, telekomunikacji (§9.6) | M8 |
| Zlecenia transportowe towarów, flota firm, rynek przewozów, ciężarówki z ładunkiem | M6 |
| Realne zbiorniki stacji paliw, dostawy cysterną, ekonomia i **ceny** paliwa | M5 (cena jako oferta) / M6 (zbiorniki, łańcuch) |
| Przetargi na obsługę linii, dotacje, regulacje ruchu, strefy płatnego parkowania jako polityka | M8 |
| Animacje pojazdów, modele voxelowe, dźwięk, LOD wizualne | M11 |
| Ładowarki EV jako odbiorniki sieci energetycznej (samo `FuelKind::Electric` i bateria — tak) | M8 |
| Tryb makro ruchu (50×, model statystyczny z §17.4) — M4 dostarcza tylko `TravelTimeMatrix` jako jego wejście | M12 |

**Stacje paliw w M4** są nieskończonymi źródłami: mają lokalizację, markę, kolejkę i **cenę
odczytywaną z danych** (`data/goods/fuel.ron`), nie mają zbiorników. M4 definiuje zdarzenie
`FuelPurchased { station, volume: Volume, unit_price: Money }`, M6 podpina pod nie zbiornik.

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co realizuje |
|---|---|
| §9.1 Sieć | WP1 — `RoadGraph`, klasy, tonaż, mosty; grafy pieszy/rowerowy/szynowy |
| §9.2 Symulacja ruchu (mikro/mezo, spójność) | WP3, WP8, **WP9 (dowód spójności)** |
| §9.3 Komunikacja miejska | WP10 |
| §9.4 Wybór środka transportu | WP6 |
| §9.5 Paliwo i energia | WP5 |
| §9.6 Sieci przesyłowe | **poza zakresem — M8** |
| §5.1 Model mieszkańca — tożsamość | WP13 — imię i nazwisko „z puli regionalnej"; pula powstaje dopiero tutaj (`Z-7`) |
| §5.5 Tryb dnia | WP4, WP5 — podróż i tankowanie jako zadania planera M3 |
| §5.6 Decyzje długoterminowe | WP2 — `TravelTimeMatrix` zasila zasięg poszukiwania pracy i mieszkania; próg „koszt komunikacji + czas > koszt posiadania auta" |
| §14.1 Wyjaśnialność | `TripDecisionReason` w każdej decyzji transportowej |
| §14.2 Warstwy widoku | WP11 — nakładki ruchu, korków, czasów przejazdu |
| §14.4 Śledzenie | `TripLedger` jako oś czasu podróży w karcie inspekcji |
| §17.1 Czas | tick mikro 100 ms, tick mezo = tick ekonomiczny 1 min |
| §17.3 DES | podróż = zdarzenie „przybycie" w kolejce czasu M3 |
| §17.4 LOD | WP9 — architektura gwarantująca niezależność wyniku od LOD |
| §17.6 Pathfinding | WP2 — CCH, A\*, cache tras, macierz dzielnica × godzina |
| §19 M4 | zakres kamienia milowego |
| §20.2 Metryki techniczne | WP12 — budżety ms/tick, 50× |

---

## 4. Pakiety robocze i podfazy

Kolejność jest kolejnością wykonania. `→` oznacza twardą zależność.

Faza jest rozbita na **4 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M4a — Graf i routing** | WP1, WP2 | 5.1 | Devtools: inspektor grafu rysuje krawędzie z atrybutami; zapytanie CCH odpowiada w budżecie na grafie 200 tys. węzłów. | `M4a-graf-i-routing.md` |
| **M4b — Mezo i podróże** | WP3, WP4, WP5 | 5.2, 5.7 | Mieszkańcy z M3 dojeżdżają do pracy pojazdami zamiast teleportacji; korek powstaje na przewężeniu i rozładowuje się. | `M4b-mezo-i-podroze.md` |
| **M4c — Wybór środka, parkingi, komunikacja** | **WP14**, WP6, WP7, WP10 | 5.12, 5.3, 5.5, 5.6 | Rozkład udziału środków transportu w widełkach z PRD §20.1; linia autobusowa wozi ludzi wg rozkładu; przepełniony parking odbiera opcję „samochód”. | `M4c-wybor-srodka-parkingi-komunikacja.md` |
| **M4d — Mikro i dowód spójności** | WP8, WP9, WP11, WP12, WP13 | 5.4, 5.8, 5.9, 5.10 | Pełny artefakt fazy z §1 dokumentu fazy: korki widoczne i mierzone, „kamera nie zmienia świata”. | `M4d-mikro-i-dowod-spojnosci.md` |

Ścieżka krytyczna: WP1 → WP2 → WP3 → WP4 → WP6. WP8/WP9 mogą iść równolegle do WP10 po WP3.
WP13 jest poza ścieżką krytyczną i **nie blokuje niczego** — zależy tylko od WP11, bo to karta
inspekcji podróży jest pierwszym ekranem, na którym brak imion widać (`Z-7`, §5.10).
**WP14 idzie pierwszy w M4c, przed WP6** — to spłata kosztu warstwy Mikro, który wyszedł w trakcie
M4b; pakiet stoi w M4c, a nie w M4b, bo do podfazy będącej w trakcie implementacji nie dopisuje się
pakietów: czyta ją ktoś, kto ma jej tabelę w głowie sprzed poprawki (§5.12).

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | `engine/nav` — grafy i routing | `M4a-graf-i-routing.md` |
| 5.2 | `sim/traffic` — pojazd jako encja | `M4b-mezo-i-podroze.md` |
| 5.3 | Wybór środka transportu (§9.4) | `M4c-wybor-srodka-parkingi-komunikacja.md` |
| 5.4 | **Spójność mikro ↔ mezo — architektura i dowód** | `M4d-mikro-i-dowod-spojnosci.md` |
| 5.5 | Parkingi | `M4c-wybor-srodka-parkingi-komunikacja.md` |
| 5.6 | Komunikacja miejska (§9.3) | `M4c-wybor-srodka-parkingi-komunikacja.md` |
| 5.7 | Paliwo i tankowanie (§9.5) | `M4b-mezo-i-podroze.md` |
| 5.8 | Systemy ECS i częstotliwości | `M4d-mikro-i-dowod-spojnosci.md` |
| 5.9 | Determinizm | `M4d-mikro-i-dowod-spojnosci.md` |
| 5.10 | **Generator imion i nazwisk (WP13)** — sekcja nowa, dopisana po M3 | `M4d-mikro-i-dowod-spojnosci.md` |
| 5.12 | **Koszt warstwy Mikro (WP14)** — sekcja nowa, dopisana w trakcie M4b | `M4c-wybor-srodka-parkingi-komunikacja.md` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam

| Typ / funkcja | Crate | Konsument |
|---|---|---|
| `RoadGraph`, `NodeId`, `EdgeId`, `RoadClass`, `TurnMovement` | `engine/nav` | M6 (trasy dostaw), M8 (inwestycje drogowe, remonty), M11 (render sieci) |
| `Router::route`, `RouteQuery`, `Route`, `RouteLeg` | `engine/nav` | M5 (zasięg sklepu), M6 (planowanie kursów), M7 (dojazd do pracy w ocenie oferty) |
| `RouteCache`, `invalidate(topology_version)` | `engine/nav` | M8 (po zmianie sieci) |
| `TravelTimeMatrix::lookup(from, to, hour, mode) -> u16` | `engine/nav` | M5 (zasięg), M7 (rynek pracy, §5.6), M10/M12 (LOD makro) |
| `TripRequest`, `TripId`, `TripOutcome`, zdarzenie `TripArrived` | `sim/traffic` | M3 (planer dnia), M5, M6, M7 |
| `TripLedger`, `LedgerEntry` | `sim/traffic` | M5 (koszt dojazdu w budżecie GD), M9 (karta inspekcji), M10 |
| `RouteProfile::{Passenger, HeavyDay, HeavyNight}`, `RouteQuery.gross_mass` | `engine/nav` | **M6** (planowanie zleceń z wyprzedzeniem), M8 (strefy zakazu ruchu ciężkiego) |
| `Router::route → None` jako **rozstrzygnięcie na etapie planowania** | `engine/nav` | **M6** — trasa niewykonalna dla tonażu/zakazu jest odrzucana przy planowaniu zamówienia, nigdy w trakcie przejazdu |
| `VehicleArrivedAtSite { vehicle, trip, site, at: SimMinute }` | `sim/traffic` | **M6** (kolejka do rampy), M7 |
| `SiteDwellResponse { release_at: SimMinute, idle_fuel: bool }` — odpowiedź konsumowana z powrotem do ledgera | `sim/traffic` | **M6** |
| `access_edge_curb_occupancy(site) -> u16` (przepełnienie placu manewrowego na ulicę) | `sim/traffic` | **M6**, M8 |
| `settle_edge`, `settle_node` | `sim/traffic` | **nikt nie wywołuje poza `sim/traffic`** — kontrakt zamknięty |
| `ModeDecision`, `GeneralizedCost`, `Infeasible`, `TripDecisionReason` | `sim/agents` | M5 (dlaczego nie kupił), M9 (UI), M14.1 |
| `VehicleId` + komponenty `VehicleOwner/Condition/Location`, `FuelTank`, `VehicleClassId` | `sim/traffic` | M6 (flota firm — **rozszerza, nie redefiniuje**), M7 (majątek), M8 (podatki od pojazdów) |
| `ParkingLot`, `try_reserve`, `ParkingDenied` | `sim/traffic` | M5 (atrakcyjność sklepu), M8 (polityka parkingowa), M9 |
| `TransitLine`, `TransitRun`, `TransitStop`, `OperatorRef` | `sim/traffic` | M8 (przetargi, dotacje, regulacje), M7 (kierowcy jako miejsca pracy) |
| Zdarzenie `FuelPurchased { station, volume: Volume, unit_price: Money }` | `sim/traffic` | **M6** (zbiorniki, dostawy), M5 (obrót stacji) |
| Zdarzenie `LatenessRecorded { citizen, minutes, cause }` | `sim/traffic` | M7 (produktywność, absencja) |
| Zdarzenie `EnergyDrawn { node, energy: Energy }` (ładowarki) | `sim/traffic` | **M8** (obciążenie sieci) |
| `TrafficOverlaySnapshot` (double-buffered) | `sim/traffic` | M11 / `engine/render` |
| `StreamId::{ModeChoice, FuelStationChoice, ParkingSearch, TransitDwell, VehicleBreakdown}` | `engine/core` | — (dopisek do enuma) |

### Konsumuję

| Typ / funkcja | Od | Uwagi |
|---|---|---|
| Geometria dróg, `PolylineId`, klasy i limity | **M2** (`sim/world`) | M4 buduje z tego `RoadGraph`; potrzebuję **jawnego zdarzenia `RoadNetworkChanged { dirty_edges }`** |
| `ParcelId`, `BuildingId`, punkty dostępu budynku | **M2** | wejście/wyjazd, lokalizacja parkingu |
| `DistrictId` i podział na dzielnice | **M2** | klucz `TravelTimeMatrix` |
| `CitizenId`, `HouseholdId`, planer dnia, kolejka DES | **M3** (`sim/agents`) | podróż jako zadanie; tankowanie jako zadanie |
| Graf pieszy / chodniki, ruch pieszy | **M3** | M3 tworzy ruch pieszy — M4 przejmuje graf pieszy do routingu multimodalnego (patrz D1) |
| Cechy osobowości, dochód GD, status (§5.1, §5.4) | **M3** | wejście do `vot_gr_per_min` i `DiscomfortBreakdown` |
| Pogoda / pora roku | **M8** (docelowo) | w M4 zaślepka `WeatherStub` — patrz D4 |
| `Money`, `Volume`, `Mass`, `Energy`, `Q`, `SimMinute`, `SimInstant`, `div_round_half_up`, `StreamId` | **M0** (`engine/core`) | dok. 00 §2 |
| Job system, bufory komend, scheduler z deklaracją dostępu | **M0** (`engine/jobs`, `engine/ecs`) | deklaracja dostępu jest **wymogiem** dla testu z §5.4 |
| Snapshot double-buffered, nakładki, hash stanu | **M0/M1** (`engine/render`, `engine/io`) | |
| Cena paliwa jako liczba w `data/` | **M5/M6** | w M4 stała z danych |
| `yard_capacity(site) -> u16` oraz `dock_throughput(site) -> u16` (ciężarówek/h) | **M6** | M4 nie modeluje rampy; potrzebuje tylko pojemności placu, by wiedzieć, od kiedy kolejka wylewa się na ulicę |
| `SiteDwellResponse.release_at` — deterministyczny czas zwolnienia pojazdu z rampy | **M6** | **wymóg twardy: niezależny od LOD i od kolejności wątków** (patrz niżej) |
| Strefy zakazu ruchu ciężkiego i godziny dostaw jako maska krawędzi per `RouteProfile` | **M8** | w M4 zaślepka z `data/`; M8 czyni z tego politykę miejską |

### 6.1 Ustalenia z M6 — ruch towarowy (odpowiedź na zapytanie fazy M6)

**1. ~1 500 pojazdów dostawczych mieści się w budżecie. Nie wymagają osobnej ścieżki.**
Wchodzą do tych samych `EdgeQueue`, `settle_edge` i `TripLedger`, co ruch osobowy — inny jest tylko
`VehicleClassId` (masa, spalanie, tonaż) i `RouteProfile`. Osobna ścieżka kodu dla ciężarówek
byłaby drugim modelem ruchu do utrzymania i drugim miejscem, w którym może pęknąć tolerancja 0.
Konsekwencje liczbowe są w §7.3 (wiersze „z ruchem towarowym"): szczyt rośnie z 12 000 do
**13 500 pojazdów**, budżet mezo z 6,0 do **6,8 ms**, suma mezo z 11 do **11,8 ms**.
Trzy zastrzeżenia:
- **Pojazd czekający na rampie nie jest w ruchu.** Nie zajmuje slotu `EdgeQueue` ani budżetu mezo —
  jest w `VehicleLocation::Depot`. 1 500 ciężarówek w obiegu to znacznie mniej niż 1 500 jadących.
- **W mikro ciężarówka liczy się podwójnie** względem capa 3 000 pojazdów (dłuższy pojazd, blokuje
  pas i skrzyżowanie). Cap jest budżetem rysowania, nie symulacji — przekroczenie zostawia pojazd
  w mezo bez skutku ekonomicznego (§5.4).
- **Koszt jest w routingu, nie w ruchu.** Trasy B2B mają zmienne pary origin–dest, więc nie korzystają
  z `RouteCache` dom↔praca. Przy ~6 000 przejazdów/dobę to ~15 zapytań/min w szczycie — mieści się
  w budżecie routingu (§7.3), ale to one, a nie sam przejazd, są realnym obciążeniem.

**2. Rampa: zgoda z propozycją M6 — zakład jest właścicielem kolejki.** M4 dowozi pojazd do węzła
dostępowego, emituje `VehicleArrivedAtSite`, M6 zwraca `SiteDwellResponse { release_at, idle_fuel }`,
a M4 księguje oczekiwanie jako `LedgerEntry` (zerowe paliwo, chyba że `idle_fuel` — chłodnia, cysterna
z pompą). Cztery warunki brzegowe, bez których to nie zadziała:
- `release_at` musi być **funkcją deterministyczną stanu zakładu i minuty przybycia** — nie może
  zależeć od LOD ani od kolejności wątków, bo wchodzi do ledgera, czyli do pieniądza (dok. 00 §3, §4).
- Kolejność obsługi w kolejce M6 musi mieć **klucz totalny** (propozycja: `(minuta_przybycia,
  vehicle_entity_index)`), tak samo jak wszystkie kolejki M4.
- **Przepełnienie placu wylewa się na ulicę:** pojazdy ponad `yard_capacity(site)` zajmują pojemność
  przyuliczną krawędzi dostępowej i biorą udział w spillbacku. To jedyne miejsce, gdzie kolejka M6
  dotyka sieci M4 — i jest to zachowanie pożądane (zastawiona ulica pod źle zaprojektowaną hurtownią).
- Mikro **odgrywa** stojącą ciężarówkę, ale nie decyduje, kiedy odjedzie; obowiązuje `release_at`.

**3. Ograniczenia przejazdu są rozstrzygane przy planowaniu, nie przy przejeździe.** `RouteQuery`
dostaje `gross_mass` i `RouteProfile`; krawędzie niespełniające ograniczenia mają w zestawie wag
tego profilu wagę `u32::MAX`, więc CCH ich nie zwróci. `Router::route` zwraca `None`, gdy trasy
nie ma — M6 dowiaduje się o tym **przy planowaniu zamówienia**, nie gdy cysterna stoi przed mostem.
To kosztuje dwa dodatkowe zestawy wag na tej samej kolejności kontrakcji (~150 ms kustomizacji
i ~4–8 MB każdy) — i jest to dokładnie powód, dla którego §5.1 wybiera CCH, a nie klasyczne CH.
Liczba profili jest **zamknięta i mała**; regulacje M8 przełączają profil albo zmieniają maskę
krawędzi, nigdy nie dokładają nowego profilu per regulacja (patrz D11).

---

## 7. Testy i kryteria akceptacji

### 7.1 Poprawność funkcjonalna

| Test | Kryterium |
|---|---|
| `graph_build_integrity` | każda parcela osiągalna pieszo; brak krawędzi bez węzła; suma pasów ≥ 1; most ma przepustowość |
| `route_optimality` | 1000 losowych par: CCH == Dijkstra na `RoadGraph` (ten sam koszt), 100 % |
| `heavy_route_respects_constraints` (własnościowy) | 1000 losowych zapytań `HeavyDay`/`HeavyNight` o losowej `gross_mass`: **żadna zwrócona trasa nie zawiera krawędzi z `max_mass < gross_mass`, mostu w remoncie ani krawędzi w aktywnej strefie zakazu** — 100 %, bez wyjątków |
| `no_mid_trip_restriction_failure` | 5 000 przejazdów towarowych: liczba `TripFailure` z powodu tonażu/zakazu == **0**; niewykonalność zawsze objawia się jako `route() == None` przy planowaniu |
| `heavy_route_completeness` | jeśli Dijkstra na grafie odfiltrowanym po ograniczeniach znajduje trasę, `Router` też ją znajduje (brak fałszywych `None`) |
| `yard_overflow_spillback` | plac zapełniony → nadmiarowe ciężarówki zajmują pojemność przyuliczną krawędzi dostępowej; brak zakleszczenia, brak pojazdów-widm; po rozładowaniu kolejka schodzi z ulicy |
| `route_cache_correctness` | trafienie cache zwraca trasę identyczną z przeliczoną na świeżo |
| `cch_rebuild_correctness` | po zmianie topologii trasy nie używają usuniętych krawędzi ani w trakcie, ani po przebudowie |
| `spillback_conservation` | liczba pojazdów w systemie = wjazdy − wyjazdy, na każdym ticku, przy nasyconej sieci |
| `parking_no_ghosts` | `sum(lot.occupied) + pojazdy_w_ruchu + pojazdy_w_zajezdni == liczba_pojazdów`, każdy tick |
| `transit_capacity` | pasażerów w pojeździe nigdy > `capacity`; pozostawieni trafiają na następny kurs |
| `mode_choice_explainability` | 100 % decyzji ma `TripDecisionReason` z pełną listą kandydatów i kosztów (dok. 00 §7) |
| `fuel_conservation` (własnościowy) | `Σ zatankowane − Σ spalone == Σ poziomów baków − stan początkowy`, tolerancja 0 ml |
| `money_conservation` (własnościowy) | wydatki na paliwo/bilety/parking == przychody stacji/operatorów/parkingów, tolerancja 0 gr |
| `indeks_zawsze_ma_wpis_w_puli` (własnościowy, WP13) | 100 tys. wylosowanych tożsamości: każdy `first_name`/`last_name` ma wpis w puli — zakres losowania pochodzi z długości pliku, nie ze stałej |
| `imie_zgadza_sie_z_plcia` (WP13) | imię wylosowane dla `FLAG_MALE` pochodzi z męskiego podzbioru puli i odwrotnie, 100 % |
| `nazwisko_dziedziczone_w_formie_wlasnej_plci` (WP13) | rodzina Kowalskich: ojciec „Kowalski", córka „Kowalska", ten sam indeks nazwiska; nazwisko nieodmienne („Nowak") identyczne w obu formach |

### 7.2 Spójność LOD i determinizm

- **`micro_mezo_equivalence`** — pełna tabela w §5.4 (A1–A5 blokujące z tolerancją 0, B1–B4 jako
  miernik dryfu kalibracji).
- **`ramp_wait_lod_invariant`** — rozszerzenie A1–A5 o scenariusz towarowy: 300 ciężarówek, 12 zakładów
  z rampami, przebiegi `ForceMezo` vs `ForceMicro`. Dla każdego przejazdu identyczne (tolerancja 0):
  `release_at`, czas oczekiwania w ledgerze, paliwo (w tym postojowe dla chłodni) i koszt.
  **Ten test jest strażnikiem kontraktu M6**: niedeterministyczny albo LOD-zależny model rampy
  po stronie M6 wywali go natychmiast, zanim rozejdzie się po saldach.
- **`micro_writes_nothing`** — asercja na deklarowanym zbiorze dostępu systemu `micro_step`:
  zbiór zapisów == `{VehicleState}`. Test jest tańszy i mocniejszy niż jakikolwiek test przebiegowy.
- **`camera_does_not_change_world`** (A5) — skryptowana ścieżka kamery wymuszająca przełączenia LOD
  w środku podróży; ciąg hashy stanu ECS identyczny z przebiegiem bez kamery.
- **`determinism_100_days`** — dwa przebiegi tego samego seeda, 100 dni gry, pełny ruch
  i komunikacja: identyczny ciąg hashy co 1000 ticków (dok. 00 §3.6).
- **`thread_count_invariance`** — ten sam seed na 1, 4 i 16 wątkach → identyczny hash.
- **`cch_rebuild_tick_determinism`** — moment podmiany `ChGraph` (numer ticku) identyczny
  w obu przebiegach, niezależnie od obciążenia maszyny.

### 7.3 Wydajność — budżety

Scenariusz odniesienia: miasto 150 tys. mieszkańców, ~50 000 pojazdów zarejestrowanych,
~12 000 osobowych jednocześnie w ruchu w szczycie **plus ~1 500 pojazdów dostawczych w obiegu M6**
(z czego ~900 na sieci, reszta na rampach i placach). Maszyna odniesienia: 8 rdzeni.

| Ścieżka | Częstotliwość | Budżet | Uzasadnienie arytmetyczne |
|---|---|---|---|
| `mezo_edge_step` + `mezo_node_step` | 1 / minutę gry | **6,8 ms** (suma na wątkach) | 13 500 pojazdów (12 000 osobowych + ~900 towarowych na sieci + zapas) × ~0,3 µs, równolegle po krawędziach. Towarowy dokłada ~12 % |
| `trip_dispatch` + `mode_choice` | 1 / minutę gry | **2,5 ms** | ~1 000 nowych podróży/min w szczycie (z 2 800 zdarzeń/min, §17.3) × ~8 kandydatów × ~0,3 µs. Przejazd towarowy pomija `mode_choice` — środek transportu wybiera M6 |
| Routing osobowy (po odjęciu cache) | 1 / minutę gry | **1,5 ms** | trafialność cache ≥ 90 % → ~100 zapytań CCH/min × 60 µs = 6 ms, rozdzielone na 8 wątków |
| **Routing towarowy** (bez cache) | 1 / minutę gry | **0,3 ms** | ~6 000 przejazdów/dobę → ~15 zapytań/min w szczycie × ~80 µs (profil z ograniczeniami jest nieco droższy) / 8 wątków |
| Obsługa ramp (`VehicleArrivedAtSite` ↔ `SiteDwellResponse`) | 1 / minutę gry | **0,2 ms** | ~30 zdarzeń przyjazd/odjazd na minutę; sama kolejka jest po stronie M6 i nie obciąża tego budżetu |
| `transit_*` + `parking_*` | 1 / minutę gry | **1,0 ms** | ~200 kursów, ~2 000 parkingów |
| **Razem mezo** | 1 / minutę gry | **≤ 11,8 ms** | wzrost z 11 ms po dołożeniu ruchu towarowego |
| `micro_step` | 1 / 100 ms gry | **4,0 ms** | cap 3 000 **jednostek** w mikro × ~1,3 µs (IDM+MOBIL+integracja), 4 fazy; ciężarówka = 2 jednostki |
| Warstwa Mikro pieszych (WP14) | **1 / minutę gry** | **2,0 ms** | 5 tys. pieszych w oknie kadru; budżet liczony **na minutę świata**, nie na wywołanie — patrz §5.12 M4c, gdzie ta różnica jest właśnie tym, co kryterium M3b przeoczyło |
| Budowa snapshotu nakładek | 1 / klatkę | **0,8 ms** | ~80 tys. krawędzi, kopia 4 pól |
| Kustomizacja CCH | zdarzeniowo, w tle | **≤ 450 ms** łącznie, rozłożone na ≤ 4 ticki | 3 profile (`Passenger`, `HeavyDay`, `HeavyNight`) × ~150 ms, równolegle |
| Rekontrakcja CCH | zdarzeniowo, w tle | **≤ 8 s** łącznie, rozłożone na ≤ 30 ticków gry | pełna kontrakcja; kolejność wspólna dla wszystkich profili; w międzyczasie fallback A\* |
| Pamięć `ChGraph` | — | **≤ 30 MB** | kolejność + topologia raz, wagi ×3 profile po ~4–8 MB |

Weryfikacja metryki §20.2 (*doba gry ≤ 3 s w trybie 50× dla miasta 150 tys.*): 1 440 ticków
minutowych × 11,8 ms = **17,0 s** — **budżet z §20.2 jest przekroczony ponad 5×**, jeśli ruch liczyć
wprost w trybie 50×. Dlatego:

- W trybie 50× LOD mikro jest **wyłączony w całości** (§14.5 mówi to wprost), co odejmuje 0 ms —
  i to nie wystarcza.
- Tryb 50× używa **LOD makro ruchu**: czasy przejazdu z `TravelTimeMatrix` zamiast rozliczania
  krawędź po krawędzi, `settle_edge` wywoływane raz na podróż na trasie zagregowanej.
  Koszt spada do ~1,2 ms/tick → 1,7 s/dobę. **Właścicielem trybu makro jest M12**; M4 dostarcza
  mu `TravelTimeMatrix` i musi już teraz zapewnić, że agregat jest wyprowadzalny z `settle_edge`.
- **To jest realne zagrożenie dla kontraktu tolerancji 0 w trzecim LOD** i jest zgłoszone jako
  decyzja otwarta **D2**.

Benchmarki criterion: `bench_cch_query`, `bench_cch_query_heavy` (profil z ograniczeniami),
`bench_mezo_tick_13k5` (z ruchem towarowym), `bench_micro_tick_3k`, `bench_mode_choice`,
`bench_cch_customize_3profiles`.

### 7.4 Kryteria akceptacji fazy (Definition of Done)

1. Wszystkie testy z §7.1–7.2 zielone; `clippy -D warnings`; `#![forbid(unsafe_code)]`.
2. `micro_mezo_equivalence` A1–A5 z **tolerancją 0**.
3. Budżety z §7.3 dotrzymane w trybie 1× i 10× (50× — patrz D2).
4. Komponenty M4 dopisane do funkcji haszującej ECS; `determinism_100_days` zielony.
5. Każda decyzja transportowa ma `TripDecisionReason` widoczny w karcie inspekcji (dok. 00 §7).
6. Headless: cały ruch i komunikacja uruchamialne bez GPU.
7. Nakładki §14.2 (ruch, korki, czasy przejazdu) działają i są czytelne.

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Skutek | Mitygacja |
|---|---|---|---|
| R1 | **Pokusa uczynienia mikro autorytatywnym** („prawdziwy korek powinien kosztować") | Śmierć determinizmu — kamera gracza zmienia salda GD | Zakaz wymuszony deklaracją dostępu w schedulerze + test `micro_writes_nothing`; uzasadnienie zapisane w §5.4 |
| R2 | **Rozjazd kalibracji IDM ↔ VDF** | Animacja przestaje pasować do księgowości; gracz widzi płynny ruch i dostaje rachunek za korek | Miernik dryfu B1–B4 w buildzie nocnym; `balansator calibrate-vdf` jako powtarzalna procedura; VDF wyprowadzany z parametrów IDM, nie dopasowywany osobno |
| R3 | **Koszt rekontrakcji CCH** przy częstych zmianach sieci (M8: inwestycje miejskie) | Zacięcia, niedeterminizm momentu podmiany | Podział: kustomizacja (tania) vs rekontrakcja (rzadka); budżet pracy na tick zamiast czasu zegarowego; fallback A\* na `dirty_edges` |
| R4 | **Spillback → zakleszczenie** (gridlock na pierścieniu krawędzi) | Symulacja staje, ruch zamiera na stałe | Detektor cyklu blokad per minutę; wymuszone „rozplątanie" (pojazd czekający > N minut dostaje priorytet) z deterministyczną regułą; test `gridlock_recovery` na scenariuszu przesyconej siatki |
| R5 | **Budżet 50× nieosiągalny** (patrz §7.3) | Niespełniona metryka §20.2 | LOD makro ruchu jako wymaganie M12; M4 dostarcza `TravelTimeMatrix` i gwarancję wyprowadzalności; ryzyko zgłoszone jako D2 |
| R6 | **Pojazdy-widma** (auto w dwóch miejscach, auto zaparkowane i jadące) | Niespójność majątku GD, błędy ekonomiczne w M5 | `VehicleLocation` jest jedynym źródłem prawdy; test `parking_no_ghosts` co tick; przejście stanów tylko przez jedną funkcję |
| R7 | **Eksplozja kandydatów w wyborze środka transportu** (carpooling z grafu relacji) | O(n²) na rozmiarze sieci znajomych | Twardy limit: carpooling rozważany tylko dla relacji o wadze > próg i tej samej godzinie odjazdu ±15 min, maks. 3 kandydatów; wstępne filtrowanie po `TravelTimeMatrix`, nie po pełnym routingu |
| R8 | **Zależność od M2 i M3, które powstają równolegle** | Blokada startu WP1/WP4 | `RoadGraph` budowany z minimalnego kontraktu (polilinia + klasa + limit); adapter i generator syntetycznej siatki do testów pozwalają rozwijać WP2–WP9 bez M2 |
| R9 | **Float w ścieżce pieniężnej wchodzi tylnymi drzwiami** (np. przez czas przejazdu) | Rozjazd platform, złamanie tolerancji 0 | Jedna granica: sygnatura `settle_edge` przyjmuje wyłącznie typy całkowite; kwantyzacja prędkości przed wywołaniem; lint w review |
| R10 | **Kolejka mikro rośnie przy szybkim ruchu kamery** (masowa alokacja `VehicleState`) | Skoki klatek | Pula obiektów o stałym rozmiarze; przy przekroczeniu limitu krawędzie poza priorytetem zostają w mezo (bez skutku ekonomicznego — patrz §5.4) |

---

## 9. Decyzje otwarte

W tej sesji nie był dostępny mechanizm odpytania agentów planujących pozostałe fazy
(brak narzędzia `ListAgents`), więc wszystkie punkty styku wymagające uzgodnienia trafiają tutaj.

| # | Decyzja | Kontekst i propozycja M4 | Z kim uzgodnić |
|---|---|---|---|
| **D1** | **Kto jest właścicielem grafu pieszego?** | §19 przypisuje „pieszy ruch" do M3, ale `engine/nav` jest crate'em M4. Propozycja M4: **M3 tworzy graf pieszy jako minimalną strukturę w `sim/world`; M4 przenosi go do `engine/nav` jako jedną z instancji `RoadGraph`** i dokłada A\*, `TransferLink` i routing multimodalny. M3 konsumuje nowe API bez zmiany semantyki. | **M3**, M2 |
| **D2** | **Tryb 50× nie mieści się w metryce §20.2 przy ruchu mezo.** | Arytmetyka w §7.3: 15,8 s/dobę wobec wymaganych 3 s. Rozwiązaniem jest LOD makro ruchu (czasy z `TravelTimeMatrix` zamiast krawędź po krawędzi). **Ale makro łamie tolerancję 0** z dok. 00 §4, chyba że kontrakt zostanie doprecyzowany: propozycja M4 — *tolerancja 0 obowiązuje między mikro a mezo; makro ma osobny, jawnie słabszy kontrakt (błąd średni salda ≤ 0,5 % w skali miesiąca, bez dryfu systematycznego), a przejście makro→mezo nie może tworzyć ani niszczyć pieniądza.* Wymaga zmiany w dok. 00 §4. | **M12**, właściciel dok. 00, M5 |
| **D3** | **Amortyzacja pojazdu: koszt podróży czy koszt okresowy?** | §9.4 wymienia amortyzację w koszcie uogólnionym podróży, ale księgowość GD (M5) najpewniej chce kosztu miesięcznego. Propozycja M4: **w koszcie uogólnionym amortyzacja występuje jako stawka za km (wpływa tylko na decyzję); realne obciążenie budżetu GD to koszt okresowy naliczany przez M5 z przebiegu.** Podwójne liczenie jest tu realnym zagrożeniem. | **M5**, M7 |
| **D4** | **Pogoda przed M8.** | `DiscomfortBreakdown.weather` jest istotny dla udziału roweru i pieszych. M4 potrzebuje w M4 zaślepki. Propozycja: **`WeatherStub` w `engine/core` z deterministycznym sezonowym modelem (temperatura + opad z seeda i dnia roku)**, którą M8 zastępuje realnym systemem bez zmiany sygnatury. | **M8**, M1 (klimat) |
| **D5** | **Czy taxi jest firmą już w M4?** | §9.4 wymienia taxi jako opcję. Firmy powstają w M7. Propozycja M4: **taxi w M4 jest opcją transportową z ceną z danych i zerowym czasem podstawienia powyżej progu gęstości**, bez encji firmy i bez floty; M7 podmienia na realnego operatora. Ryzyko: jeśli M7 zaprojektuje taxi jako pełny rynek przewozów, kontrakt `TravelMode::Taxi` może wymagać zmiany. | **M7**, M6 |
| **D6** | **Reprezentacja baterii EV.** | `FuelTank` używa `Volume` (ml). Dla EV naturalną jednostką jest `Energy` (Wh, dok. 00 §2). Dwa warianty: (a) `FuelTank { capacity: Volume }` + sztuczny przelicznik dla EV (brzydkie, ale jedna ścieżka kodu), (b) `enum EnergyStore { Liquid(Volume), Electric(Energy) }` (czyste, ale dwie ścieżki w `settle_edge`). Propozycja M4: **(b)** — wzór paliwowy i tak ma inne stałe, a `Energy` jest potrzebna M8 do obciążenia sieci. | **M8**, właściciel dok. 00 |
| **D7** | **Czy mikro ma być w ogóle włączane „na drogach o wysokim obciążeniu" (§9.2)?** | W architekturze z §5.4 mikro nie wpływa na wynik, więc uruchamianie go poza kadrem nie daje nic poza kosztem. Propozycja M4: **kryterium LOD Mikro to wyłącznie kadr kamery**; kryterium obciążenia zostaje jako tryb devtools/walidacji. To jest **jawne odstępstwo od litery §9.2 PRD** wymuszone przez §17.4 i §18.2 — wymaga akceptacji. | właściciel PRD |
| **D8** | **Zdarzenie `RoadNetworkChanged { dirty_edges }` z M2.** | M4 potrzebuje jawnego, ziarnistego powiadomienia o zmianie sieci z rozróżnieniem „zmiana wagi" vs „zmiana topologii" — od tego zależy, czy idzie tania kustomizacja czy droga rekontrakcja (§5.1). Bez tego M4 musi przebudowywać wszystko przy każdej zmianie. | **M2**, M8 |
| **D9** | **Kto wystawia cenę paliwa w M4?** | M4 czyta stałą z `data/`. M5 wprowadza oferty, M6 — łańcuch. Propozycja: **`FuelPrice` jako trywialna oferta w `sim/economy` już od M5**, a M4 czyta przez interfejs, nie bezpośrednio z pliku — żeby podmiana w M5 nie ruszała kodu M4. | **M5**, M6 |
| **D10** | **Sygnalizacja adaptacyjna.** | PRD nie rozstrzyga, czy sygnalizacja jest stałoczasowa czy adaptacyjna. M4 planuje **stałoczasową z planów w `data/`** (deterministyczna, prosta, kalibrowalna). Sygnalizacja adaptacyjna jest naturalną polityką miejską — jeśli M8 chce ją jako narzędzie gracza/miasta, potrzebuje hooka `SignalPlanId → plan` już teraz. | **M8** |
| **D11** | **Liczba profili routingu musi zostać mała.** | M4 utrzymuje 3 zestawy wag CCH (`Passenger`, `HeavyDay`, `HeavyNight`) — każdy kosztuje ~150 ms kustomizacji i ~4–8 MB. Jeśli M8 zaprojektuje regulacje ruchu jako dowolnie parametryzowalne strefy (godziny, klasy pojazdów, dni tygodnia per dzielnica), liczba profili eksploduje i routing przestaje się mieścić w budżecie. Propozycja M4: **regulacje M8 zmieniają maskę wyłączonych krawędzi wewnątrz istniejącego profilu; utworzenie nowego profilu wymaga zgody M4.** Fallbackiem dla rzadkiego, nietypowego ograniczenia jest A\* na `RoadGraph` z predykatem — wolniejszy, ale bez kosztu stałego. | **M8**, M6 |
| **D12** | **Czy ulice mają nazwy?** | Wyszło przy dopisywaniu WP13: plan **nie ma nazw ulic w żadnej fazie**, a adres mieszkańca to dziś `budynek / lokal / dzielnica` (M3d §5.4). Nie ma też nigdzie zapisu, że to decyzja — więc jest to przeoczenie, nie wybór. M4 jest fazą, która ulice indeksuje (`EdgeId`, `RoadGraph`), więc gdyby nazwy miały powstać, to jest najtańszy moment: generator stałby obok dzielnicowego z M2c i brał od niego toponimy. Propozycja M4: **nie w M4** — nazwa ulicy jest widoczna dopiero, gdy jest gdzie ją pokazać (tabliczka w M11c, adres w karcie M9c), a M4 ma już WP13 jako dług z poprzedniej fazy i nie ma powodu brać drugiego. Jeśli właściciel produktu uzna inaczej, WP13 rozszerza się o trzeci plik `data/names/streets_pl.ron` i pole `name: u16` w `RoadSegment` — koszt jest wtedy mały, bo pula i formater już będą. | właściciel produktu, **M9**, M11 |

---

## 10. Szacunek wielkości

| WP | Nazwa | Rozmiar | Uwaga |
|---|---|---|---|
| WP1 | `RoadGraph` i grafy modalne | **M** | Dużo atrybutów, mało algorytmiki; ryzyko w kontrakcie z M2 (D8) |
| WP2 | Routing: CCH + A\* + cache | **L** | CCH to najtrudniejszy algorytmicznie element fazy; kustomizacja + rekontrakcja + budżetowanie determinizmu |
| WP3 | Warstwa mezo | **L** | Rdzeń ekonomiczny fazy; spillback i model węzła są tu najtrudniejsze |
| WP4 | `TripRequest` i integracja z DES M3 | **M** | Głównie kontrakt i przeplanowanie |
| WP5 | Paliwo, energia, pojazd jako encja | **M** | Wzory proste, ale integracja z planerem dnia i budżetem GD dotyka wielu miejsc |
| WP6 | Wybór środka transportu | **M** | Prosty argmin; objętość jest w liczbie kandydatów, wykonalności i wyjaśnialności |
| WP7 | Parkingi | **S** | Rezerwacja na oknie czasowym, jedna ścieżka kodu dla wszystkich typów |
| WP8 | Warstwa mikro | **XL** | IDM + MOBIL + sygnalizacja + ronda + pierwszeństwo + 4-fazowy tick deterministyczny; największy pojedynczy pakiet fazy |
| WP9 | Dowód spójności mikro↔mezo | **L** | Harness, scenariusz referencyjny, kalibrator w balansatorze, miernik dryfu; wartość tego pakietu jest nieproporcjonalnie wysoka do jego rozmiaru |
| WP10 | Komunikacja miejska | **L** | Rozkłady, tabor, kierowcy, routing multimodalny, przepełnienie |
| WP11 | Nakładki UI i inspekcja | **M** | Trzy nakładki + karta podróży + karta pojazdu |
| WP12 | Wydajność i determinizm | **M** | Profilowanie, chunkowanie, benchmarki, hash |
| WP13 | Generator imion i nazwisk | **S** | Dwa pliki danych, jeden formater, trzy testy; objętość jest w treści plików, nie w kodzie. Dług z M3 (`Z-7`), nie zakres M4 |
| WP14 | Koszt warstwy Mikro | **S** | Trzy poprawki w kodzie warstwy Mikro napisanym w M4b: pętla 600 → jedno wywołanie, przywrócenie bramki okna (`Z-6`), jedna kopia zrzutu zamiast dwóch. Objętość jest w pomiarze, nie w kodzie — bench musi iść **pełną ścieżką systemu** (§5.12) |

Sumarycznie faza jest **ciężka** — porównywalna z M2 lub M3 — a jej ciężar koncentruje się
w WP8 (mikro) i WP3 (mezo). Gdyby faza musiała zostać przycięta, jedyna bezpieczna redukcja to
**odłożenie części WP8** (zmiana pasa i ronda) do M11: warstwa mikro nie ma skutków ekonomicznych,
więc jej uproszczenie nie rusza żadnego kontraktu poza wizualnym. Odwrotna redukcja — przycięcie
WP3 lub WP9 — jest niedopuszczalna: to one niosą kontrakt §17.4.

## Zmiany wpisane po M3

Zgodnie z `K-18`. To są rzeczy, o których M4 wie **na pewno** po zamknięciu M3d;
M4 nie jest tu przeprojektowywany.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **Punktem podmiany jest `sim::agents::Sources.travel`**, nie wywołania w planerze. M4 wymienia jedno pole zasobu `AgentSources` i **kasuje moduł `walk` w całości** (M3 §6.2) | Planer i systemy doby wołają `&dyn TravelOracle`; `Sources` trzyma dziś `WalkOracle` konkretnie, bo jedyna implementacja żyje w tym samym crate'cie. Po wymianie typ pola staje się `Box<dyn TravelOracle>` albo typem M4 — żadne wywołanie się nie zmienia |
| Z-2 ★ | **`Mobility` ma dziś tempo spadku 0 i to M4 je wpisuje** (`data/needs/needs.ron`, korekta H-3) | Potrzeba mobilności ma `AbsenceRisk` 1000 i żadnego sposobu zaspokojenia, dopóki nie ma środków transportu. Zostawiona ze spadkiem zatrzymywała całe miasto w pracy w drugiej dobie. M4 wnosi mechanizm (dostęp do trasy, pojazd, komunikacja) i razem z nim tempo oraz `satisfaction` |
| Z-3 ★ | **Etap 8 dostarcza sieć pieszą jako dwie płaskie tablice** (`Populated.nodes`, `Populated.segments`), przeliczone z `RoadNetwork` M2 na centymetry. M4 buduje `RoadGraph` z `RoadNetwork` bezpośrednio i te dwie tablice znikają razem z modułem `walk` | Konwersja jest jedną pętlą i należy do `sim/world`, bo `sim/agents` nie zależy od `sim/world` (korekta E-7). M4 nie ma powodu jej przejmować |
| Z-6 ★ | **Warstwa Mikro ma okno**: `WalkOracle::set_micro_window(środek, promień)`, domyślnie wyłączone. Pieszy wchodzi w nią w chwili, gdy **zaczyna** podróż, i tylko jeśli któryś koniec trasy mieści się w oknie. M4 przejmuje ten kontrakt razem z buforem | Bez okna warstwa trzyma polilinie dla wszystkich 274 tys. mieszkańców, których nikt nie ogląda. Bramka jest po stronie `WalkOracle`, nie wołającego: `DayLoopSystem` woła `enter_micro` bezwarunkowo i nic nie wie o kamerze, a headless nie płaci nic. **Konsekwencja, o której trzeba pamiętać**: okno musi być otwarte, zanim ruszy doba, którą chce się oglądać — pieszy, który wyszedł przed jego otwarciem, drugiej szansy nie dostanie (H-28) |
| Z-4 | **Warstwa Mikro jest już wpięta jako system `WalkMicroSystem`** (`Cadence::EveryMicroTick`, 600 podkroków po 100 ms) i **nie zapisuje niczego do stanu ekonomicznego** | M4 zastępuje `PedestrianBuffer` jednym buforem dla pieszych, pojazdów i pasażerów (decyzja 9.17). Częstotliwość i kontrakt „mikro nie ma prawa zapisu" są już zadeklarowane w kodzie, więc M4 podmienia treść, a nie miejsce |
| Z-5 | **Spóźnienia wobec planu już istnieją i mają obsługę**: `ReplanCause::Late { delay_min }` wyzwala przeplanowanie przyrostowe z debouncingiem 15 minut | M4 dokłada drugą przyczynę spóźnienia (korek), a nie ścieżkę sterowania — ta jest zbudowana i zmierzona (0,1 % przybyć, 2,1 min średnio) |
| Z-7 ★ | **M4 przejmuje dług M3: puli imion i nazwisk nie ma.** Nowy pakiet **WP13** w M4d (§5.10) buduje `data/names/first_names_pl.ron` i `surnames_pl.ron`, przestawia zakres losowania ze stałych `256`/`512` na długość puli i dokłada formater w `engine/ui` | `Identity.first_name`/`last_name` istnieją od M3a **jako indeksy w puli, której nikt nie zbudował** — komentarz typu w M3a §5.1 i M12e §66 odsyłają do niej jako do rzeczy istniejącej, a żaden pakiet w M2, M3a, M3c ani M3d jej nie tworzy. Dziś `demography.rs` i `migration.rs` losują surowe liczby, a karta mieszkańca z M3d wypisuje je dosłownie: `#84213 (45/321)`. M4 jest fazą, w której zaczyna to boleć widocznie, bo zdanie testowe fazy z §1 brzmi *„Anna wyjeżdża o 07:20"*, a karta inspekcji podróży (WP11) jest pierwszym ekranem pokazującym mieszkańca w zdaniu, nie w wierszu tabeli. **Konsekwencja, o której trzeba pamiętać:** zmiana zakresu losowania zmienia wylosowane wartości, a oba pola wchodzą do funkcji haszującej — hash świata przestawia się jednorazowo (§5.10) |
