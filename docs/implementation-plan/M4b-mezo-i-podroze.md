# M4b — Mezo i podróże

Podfaza 2 z 4 fazy **M4 — Ruch** (`M4-ruch.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M4a (graf, routing), M3 (DES). |
| **Pakiety robocze** | WP3, WP4, WP5 |
| **Projekt techniczny** | §5.2, §5.7 |
| **Wynik do pokazania** | Mieszkańcy z M3 dojeżdżają do pracy pojazdami zamiast teleportacji; korek powstaje na przewężeniu i rozładowuje się. |
| **Kryterium zamknięcia** | Kryteria WP3–WP5; bilans pojazdów (wjazdy − wyjazdy) i bilans paliwa (zatankowane − spalone) z tolerancją 0. |
| **Poprzednia / następna** | `M4a-graf-i-routing.md` · `M4c-wybor-srodka-parkingi-komunikacja.md` |

Warstwa mezo jako jedyne źródło prawdy ekonomicznej (`settle_edge` / `settle_node`), API `TripRequest` wpięte w DES M3, pojazd jako encja z bakiem i tankowaniem.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP3** | Warstwa mezo | WP1 | `LinkState` per krawędź (przepływ, gęstość, prędkość średnia, przepustowość), `EdgeQueue` (kopiec `(exit_minute, vehicle)`), funkcja przepustowości VDF, model węzła: przepustowość per ruch skrętny z kolejką. **`settle_edge` / `settle_node` — jedyne źródło prawdy ekonomicznej.** | Pojazdy przejeżdżają miasto bez wizualizacji; korek na przewężeniu powstaje i rozładowuje się; test zachowania: liczba pojazdów w systemie = wjazdy − wyjazdy |
| **WP4** | `TripRequest` i integracja z DES M3 | WP2, WP3 | API podróży: agent zgłasza `TripRequest`, dostaje `TripId` i zdarzenie `TripArrived` w kolejce czasu M3. Przerwanie i przeplanowanie (korek, zamknięta droga). `TripLedger` — rejestr przejazdu per krawędź. | Mieszkańcy z M3 dojeżdżają do pracy pojazdami zamiast teleportacji; oś czasu dnia w karcie inspekcji pokazuje realne czasy |
| **WP5** | Paliwo, energia, pojazd jako encja | WP4 | Komponenty pojazdu, `FuelTank`, zużycie per przejazd rozliczane w `settle_edge`, zużycie techniczne i przebieg. Tankowanie: próg → zadanie w planie dnia → wybór stacji wg użyteczności (cena, nadłożenie trasy, marka, kolejka). | Auto z pustym bakiem nie jest opcją transportową; Anna tankuje po drodze; suma paliwa = zatankowane − spalone (test własnościowy) |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.2 `sim/traffic` — pojazd jako encja

```rust
// Komponenty ECS pojazdu (VehicleId = newtype nad Entity, dok. 00 §2)

pub struct VehicleClassId(pub u16);   // indeks do data/vehicles/ — spec współdzielony

pub struct VehicleOwner {
    pub kind: OwnerKind,              // Household(HouseholdId) | Firm(FirmId) | Transit(LineId)
    pub primary_driver: Option<CitizenId>,
}

pub struct VehicleCondition {
    pub wear: Q,                      // 0..=100, 100 = wrak
    pub odometer_cm: u64,
    pub next_service_cm: u64,
    pub broken_until: Option<SimMinute>,
}

pub struct FuelTank {
    pub kind: FuelKind,               // Petrol | Diesel | Lpg | Electric
    pub capacity: Volume,             // ml (dla EV: Energy przeliczone na ml-ekwiwalent? NIE —
                                      // patrz decyzja otwarta D6)
    pub level: Volume,
    pub refuel_threshold: Volume,     // próg wyzwalający zadanie tankowania w planie dnia
}

pub enum VehicleLocation {
    Parked { lot: ParkingSlotRef },
    OnEdge { edge: EdgeId, trip: TripId },
    Depot { site: SiteId },
}

/// Stan MIKRO — osobna, gorąca tablica SoA, tylko dla pojazdów na krawędziach w LOD Mikro.
/// Alokowany z puli, niszczony przy zejściu do mezo. NIE jest zapisywany w snapshocie gry.
pub struct VehicleState {
    pub vehicle: VehicleId,
    pub edge: EdgeId,
    pub lane: LaneIdx,
    pub pos_cm: u32,                  // wzdłuż krawędzi
    pub speed_cms: u16,               // cm/s
    pub accel_cmss: i16,
    pub leader: Option<MicroIdx>,
    pub booked_exit: SimInstant,      // TWARDE — z TripLedger; serwo domyka do tej wartości
}

/// Stan MEZO — jedyny stan trwały ruchu. Istnieje dla KAŻDEJ krawędzi, zawsze.
pub struct EdgeQueue {
    pub edge: EdgeId,
    pub departures: BinaryHeap<Reverse<(SimMinute, u32 /*vehicle_index*/)>>,
    pub occupancy: u16,               // pojazdów aktualnie na krawędzi
    pub storage_capacity: u16,        // length_cm * lanes / (dł. pojazdu + odstęp)
    pub spillback_to: SmallVec<[EdgeId; 4]>,  // gdy pełna, blokuje dopływy
}

pub struct LinkState {
    pub inflow_last_min: u16,
    pub mean_speed_dkmh: u16,         // SKWANTOWANA — jedyna prędkość wchodząca do wzorów
    pub free_flow_dkmh: u16,
    pub capacity_vpm: u16,            // pojazdów/minutę
    pub queue_len_cm: u32,
}

/// Rejestr przejazdu — oś czasu podróży w karcie inspekcji (§14.4) i podstawa księgowania.
pub struct TripLedger {
    pub trip: TripId,
    pub entries: SmallVec<[LedgerEntry; 16]>,
    pub total_fuel: Volume,
    pub total_money: Money,
}

pub struct LedgerEntry {
    pub edge: EdgeId,
    pub entry: SimMinute,
    pub exit: SimMinute,
    pub mean_speed_dkmh: u16,
    pub stops: u8,                    // zatrzymania na węźle wejściowym
    pub fuel: Volume,
    pub money: Money,                 // opłata (bilet/parking/myto), 0 dla zwykłej krawędzi
}
```

```rust
// ---- Podróż ------------------------------------------------------------

pub struct TripRequest {
    pub traveler: CitizenId,
    pub origin: PlaceRef,             // budynek / parcela / przystanek
    pub dest: PlaceRef,
    pub depart_at: SimMinute,
    pub arrive_by: Option<SimMinute>, // twarde okno (zmiana w pracy, seans)
    pub purpose: TripPurpose,         // Work | School | Shopping | Refuel | Leisure | Medical | Escort
    pub luggage: Mass,                // wpływa na wygodę roweru/komunikacji
    pub party: SmallVec<[CitizenId; 4]>, // carpooling / odwożenie dzieci
}

pub enum TripOutcome {
    Arrived { at: SimMinute, ledger: TripLedger },
    Failed { reason: TripFailure },   // BrakParkingu | BrakPaliwa | AwariaPojazdu | BrakPolaczenia
}
```

### 5.7 Paliwo i tankowanie (§9.5)

Zużycie rozliczane w `settle_edge` (wzór w §5.4) i odejmowane od `FuelTank.level` przy księgowaniu
wpisu ledgera. Spadek poniżej `refuel_threshold` publikuje `RefuelNeeded { vehicle, household }`,
co planer dnia M3 wstawia jako **zadanie** (§5.5 PRD) — nie jako osobny system.

Wybór stacji: kandydaci w korytarzu trasy (bufor od `Route`), użyteczność w groszach:
```
koszt = szacowany_wolumen × cena_gr_za_l
      + nadłożenie_trasy_min × vot_gr_per_min
      + oczekiwanie_w_kolejce_min × vot_gr_per_min
      − premia_lojalnościowa(marka, pamięć mieszkańca)   // §5.1 pamięć, §5.7 plotka
```
Remisy — po `station_index`. Kolejka do dystrybutora: model kolejkowy M/M/c na liczbie stanowisk.
**Stacja w M4 nie ma zbiornika**: sprzedaje dowolny wolumen, emituje
`FuelPurchased { station, volume, unit_price }`. M6 podpina pod to zbiornik, dostawy cysterną
i ekonomię; kontrakt zdarzenia się nie zmieni.

EV: ta sama ścieżka, `FuelKind::Electric`, ładowarka jako `FuelStation` o długim czasie obsługi.
Obciążenie sieci energetycznej — M8 (M4 emituje tylko `Energy` pobraną).

---

## Zmiany wpisane po M4a

Zgodnie z `K-18`. To są rzeczy, o których M4b wie **na pewno** po zamknięciu M4a;
M4b nie jest tu przeprojektowywany. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| Y-1 ★ | **3,5 % węzłów drogowych metropolii leży poza największą składową silnie spójną** (11 603 z 12 031 słabo spójnych) i **2,8 % losowych par origin–cel nie ma trasy**. M4b musi to rozstrzygnąć **w wyborze środka transportu**, nie w przejeździe: `Router::route → None` dla samochodu znaczy „ten mieszkaniec nie dojedzie autem", a nie „podróż się nie udała" | Zmierzone w `headless nav --size 16km` po dołożeniu do walidatora składowej silnej (`J-18` w M4a). Różnica bierze się z ulic jednokierunkowych: węzeł na końcu jednokierunkowej ślepej uliczki jest **słabo** spójny z miastem i nieosiągalny naprawdę. Konsekwencja jest twarda, bo M4 §7.1 wymaga `no_mid_trip_restriction_failure == 0`: niewykonalność ma się objawiać przy planowaniu. Router już to robi — brakuje ramienia po stronie mieszkańca |
| Y-2 ★ | **Punktem podmiany jest `Sources.travel`, a moduł `walk` kasuje się w całości** (`Z-1`, `Z-3`). M4a **nie ruszył** `sim/agents` ani `population::siec_piesza` — oba czekają na M4b. Warstwa pieszo-rowerowa w `engine/nav` jest gotowa: 12 145 węzłów, 29 170 krawędzi, składowa silna == słaba, A\* landmarkowy **5,8× szybszy** od czystej Dijkstry | M4a dostarczał graf i router, nie podróże. Podmiana `TravelOracle` wymaga `TripRequest` i kolejki DES, czyli WP4 — robienie jej wcześniej rozdzieliłoby jedną zmianę na dwa commity wbrew regule „podfaza = jeden commit" |
| Y-3 | **`Router::route` bierze `&mut self` i zwraca `Option<Arc<Route>>`** (`J-13` w M4a). Routing **równoległy** nie istnieje i to M4b ma go zaprojektować, jeśli go potrzebuje — budżet §7.3 zakłada rozłożenie zapytań na 8 wątków | Bufory wyszukiwania i cache wymagają `&mut`, a `Mutex` na tej ścieżce zjadłby budżet, którego broni. Zmierzone dziś: metropolia, p95 **32,8 µs** na zapytanie z rozpakowaniem trasy, trafialność cache w scenariuszu dwuprzebiegowym. 100 zapytań na minutę gry to **3,3 ms jednowątkowo** wobec budżetu 1,5 ms — więc albo sesja per wątek, albo mniej zapytań dzięki cache'owi. Liczba jest znana, decyzja należy do M4b |
| Y-4 | **Pojemność `RouteCache` jest wymaganiem wdrożeniowym, nie parametrem do strojenia** (`J-12`): ≥ 2× liczba odrębnych par origin–cel, a klucz niesie kubełek godzinowy, więc par jest ~2× liczba dojeżdżających | Trafialność ≥ 90 % osiąga się przy pojemności 16 384 na 5 000 par (918 ‰); przy 8 192 spada do 763 ‰. M4b zna liczbę mieszkańców i to on musi wyliczyć pojemność z niej, a nie przyjąć stałą |
| Y-5 | **`Route.planned_cost` jest dziś `Money::ZERO`** i to WP5 ma go wypełnić. `Route.cost_cs` niesie koszt w setnych sekundy — to, co router naprawdę minimalizował | Zerowa kwota jest uczciwsza niż zmyślona: cennik paliwa wchodzi z WP5, taryfa z M4c/WP10. Pole istnieje, żeby M4b nie musiał zmieniać kształtu `Route` |
| Y-6 | **Warstwa kolejowa metropolii ma 20 węzłów w składowej i 12 032 izolowane** — to bocznice towarowe M2, nie sieć pasażerska | `RoadClass::RailPassenger` istnieje jako klasa i rodzaj bramy, ale **żaden generator jej nie zapisuje** (M2 `rail.rs` stawia wyłącznie `RailFreight`). Kolej pasażerska jako środek transportu nie ma więc na czym jeździć — dotyczy to `TransportMode::Rail` w wyborze środka (M4c/WP6) i linii szynowych w WP10 |

---

## Korekty planu wpisane po implementacji M4b

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Korekta | Dlaczego |
|---|---|---|
| L-1 ★ | **`TripRequest` z §5.2 nie powstaje w `sim/traffic`.** Typ o tej nazwie istnieje od M3 w `sim/agents::places` i to on jest argumentem `TravelOracle::begin_trip`; ruch konsumuje go, a nie definiuje drugi raz. Zlecenie gotowe do wpuszczenia na sieć nazywa się `PendingTrip` i niesie to, czego M3 nie zna: trasę, klasę pojazdu i stan zbiornika. Kontrakt M4 §6 („dostarczam `TripRequest`") wskazuje więc na `sim/agents`, nie na `sim/traffic` | Dwa typy o tej samej nazwie na to samo pojęcie to dokładnie to, przed czym broni `K-8`. `TripRequest` M3 jest w publicznym API od M3b i jest w sygnaturze traitu — zmiana jego adresu kosztowałaby przepisanie planera, którego kryterium tej podfazy zabrania ruszać |
| L-2 ★ | **Tankowanie jest objazdem w trakcie podróży, nie zadaniem w planie dnia.** §5.7 zakładał `RefuelNeeded` → zadanie planera M3. Realizacja: gdy przy wyruszeniu zbiornik jest poniżej progu **albo** poniżej czterokrotności szacowanego zużycia, zlecenie dostaje drugi odcinek trasy przez stację, sześć minut postoju i zapis `StationChosen` w karcie | Dwa powody, oba twarde. Po pierwsze wstawienie zadania do planu wymaga otwarcia `planner.rs`, a kryterium akceptacyjne fazy M3 nr 7 mówi wprost: „planer — zero zmian; jeśli musisz go dotknąć, coś poszło nie tak". Po drugie zdanie testowe fazy M4 §1 brzmi *„tankuje **po drodze**"* — objazd jest bliżej tego, co faza obiecuje, niż osobna wyprawa. `DecisionReason::RefuelNeeded` istnieje i jest pokazywany; zmienił się tylko wykonawca |
| L-3 ★ | **Paliwo liczy się w mikrolitrach** (`K-25`), a nie w mililitrach. `FuelTank.level`, `LedgerEntry.fuel_ul` i liczniki bilansu są w µl; `Volume` wychodzi dopiero przez `FuelTank::volume()` | Zmierzone: krawędź metropolii ma ~40 m, zużycie na niej ~2,5 ml, a `settle_edge` woła się ~8 razy na pojazd na minutę. Zaokrąglenie do mililitra idzie **w jedną stronę** i kumuluje się w każdej podróży — flota paliłaby kilkanaście procent za mało, a test własnościowy paliwa i tak by przeszedł, bo obie strony bilansu liczyłyby ten sam błąd. To jest przypadek, w którym zielony test nie znaczy poprawnego modelu |
| L-4 ★ | **Zdarzenie `Arrive` dla przejazdu samochodem wstawia warstwa mezo, a nie `begin_trip`.** `TripHandle.minutes` niesie czas **planowany** — ten sam, który zwróciło `estimate` — a minutę faktycznego przybycia wyznacza sieć. Dla podróży pieszych ścieżka M3 zostaje bez zmian | Bez tego korek nie miałby jak dotrzeć do mieszkańca: `begin_trip` jest wołane w chwili wyruszenia i nie wie, co się wydarzy po drodze. Równość „slot `Commute` w planie == czas z `TravelOracle`", na której stoi planer M3b, zostaje nienaruszona, bo `estimate` i `begin_trip` liczą tę samą liczbę — różnica między nią a rzeczywistością jest spóźnieniem i idzie przez `ReplanCause::Late`, który M3b już zbudował |
| L-5 ★ | **`settle_edge` ma inną sygnaturę niż §5.4**: przyjmuje czas wjazdu w setnych sekundy (nie `SimMinute`), znacznik zimnego startu i katalog pojazdów zamiast `VehicleSpec`; mnożniki wzoru paliwowego nakłada **sekwencyjnie**, z zaokrągleniem po każdym, zamiast jednego dzielenia przez 1 000 000 | Minuta jest za grubą działką na akumulację: podróż dotyka do 65 krawędzi, a zaokrąglanie czasu przejazdu każdej z nich do pełnych minut dokładałoby do pół godziny błędu. `LedgerEntry` niesie więc `travel_cs` obok `entry`/`exit` w minutach — minuty są dla karty inspekcji, setne sekundy dla arytmetyki. Mnożniki sekwencyjne dają ten sam wynik co iloczyn przy trzech czynnikach promilowych, ale bez ryzyka przepełnienia i z jawnym zaokrągleniem w każdym kroku (00 §2) |
| L-6 ★ | **Zabroniony manewr skrętny nie kończy podróży** — kosztuje zatrzymanie i czas. Pierwsza implementacja zwracała `TripFailure::NoRoute` i **to był najczęstszy powód porażki: 5 510 podróży na 5 769** | Routing jest węzłowy i nie zna manewrów (`J-8`), więc trasa może zawierać zawracanie. Najczęściej zawiera je legalnie: na przystanku pośrednim, jak stacja paliw przy ślepej uliczce, gdzie zawrócić trzeba i wolno. Odrzucanie takiej podróży w połowie łamie M4 §7.1 (`no_mid_trip_restriction_failure == 0`): niewykonalność ma się objawiać **przy planowaniu**. Nowy test `trasy_samochodowe_sa_ciagle_i_bez_zakazanych_manewrow` (`sim/world/tests/traffic.rs`) pilnuje od drugiej strony, że trasy z routera są ciągłe i bez zakazów — 199 tras na 200 par, zero naruszeń |
| L-7 ★ | **Spillback: pojazd czeka na zdarzenie, nie ponawia próby**, a krawędzie krótsze niż cztery pojazdy **nie wstrzymują wjazdu w ogóle**. `EdgeQueue` dostaje kolejkę oczekujących (lista jednokierunkowa przez pole `wait_next` w podróży), pojemność minimalną `MIN_STORAGE = 4` i znacznik `blocks` | Trzy kolejne pomiary na mieście 4 km. (a) Ponawianie próby co minutę obcina przepustowość każdego przewężenia do `storage_capacity` pojazdów na minutę i zamienia zwykłą kolejkę w zakleszczenie całej sieci: **6 833 porażek**. (b) Ponawianie co odstęp (2 s) działa, ale kosztuje miliony operacji na kopcu — test korka spowolnił z 0,03 s do 36 s. (c) Po wprowadzeniu kolejki zdarzeniowej **najwęższym gardłem miasta okazał się sześciometrowy kikut przy skrzyżowaniu** z 1 166 odmowami wjazdu. Kolejka na odcinku krótszym od jednego pojazdu jest fikcją geometryczną — fizycznie stoi ona na ulicy przed nim. Po wyłączeniu blokowania na takich odcinkach gardłem stał się realny kolektor 116 m, rozplątania spadły z 12 664 do 1 310, najdłuższe czekanie z 61 do 18 minut, a na metropolii porażki z 524 do 13 |
| L-8 ★ | **`TravelTimeMatrix` nie jest w M4b karmiona obserwacjami** i to jest decyzja, nie przeoczenie. Zostaje pusta do M4c | Macierz jest stanem symulacji: wpływa na plan, a plan na świat. Mieszka w `NavRouter`, czyli w `AgentSources`, a ten zasób jest **jawnie wyłączony z hasha stanu** (M3, `register_day`) jako „dane wejściowe miasta, nie stan". Karmienie jej teraz wsunęłoby stan do zasobu, który deklaruje, że go nie ma — i rozjazd przeszedłby przez test determinizmu niezauważony. Jej konsumentem jest wybór środka transportu (M4c/WP6), więc to M4c ma rozstrzygnąć, gdzie ona mieszka i czy wchodzi do hasha |
| L-9 | **Na sieć mezo wjeżdżają wyłącznie pojazdy.** Podróż pieszo zostaje przy ścieżce M3: szacunek czasu i `Arrive` wprost z oracle | Pieszy nie tworzy korka, a przepuszczenie 274 tys. mieszkańców przez kolejki krawędzi kosztowałoby budżet, którego broni §7.3. Zmierzone: 44 tys. podróży na dobę w mieście 4 km, z czego **5,8 tys. samochodem** — reszta nie ma po co dotykać sieci |
| L-10 | **Kolejka odjazdów jest jedna, globalna, z kluczem w setnych sekundy** `(exit_cs, indeks encji pojazdu, slot)`, a nie `BinaryHeap` per krawędź z kluczem minutowym, jak w §5.2 | Kopiec per krawędź wymagałby przejścia po wszystkich 116 tys. krawędzi w każdej minucie gry. Rozdzielczość minutowa jest natomiast **błędem modelu, nie optymalizacji**: przy takim kluczu pojazdy z jednej minuty idą w kolejności indeksu, a każdy przejeżdża całą swoją trasę, zanim ruszy następny. Krawędź nigdy nie ma wtedy dwóch pojazdów naraz i **korek nie może powstać**, bo obłożenie mierzy obecność równoczesną. Pierwsza wersja testu przewężenia pokazała dokładnie to: zero pojazdów na moście przy 120 wjeżdżających |
| L-11 | **`OwnerKind` nie jest enumem z ładunkiem**, tylko parą (znacznik, indeks encji) w `VehicleOwner`; `NodeState` nie ma kolejki per ruch skrętny | Dwa z trzech wariantów `OwnerKind` nie mają dziś właściciela: firm nie ma do M7, linii komunikacyjnych do M4c. Indeks encji jest we wszystkich trzech ten sam, więc enum kupowałby wyłącznie nazwę. Model węzła liczy opóźnienie z liczby pojazdów przez węzeł, a nie z nasycenia ruchu skrętnego — `TurnMovement.saturation_flow_vph` jest w grafie i wzór Webstera wejdzie bez zmiany sygnatury, gdy WP8 zmierzy, że to widać |
| L-12 | **Flotę obsadza `sim/world::traffic_build`, nie `sim/traffic`** — tak samo jak adapter grafu (`J-5`). Motoryzacja jest jedną liczbą (`MOTORISATION_PER_MILLE = 430` gospodarstw na tysiąc), kierowcą zostaje pierwszy pracujący dorosły gospodarstwa, a zbiornik startowy jest wypełniony w 20–100 % | `sim/traffic` nie może zależeć od `sim/world`. Zależność od dochodu odpada, dopóki nie ma budżetów gospodarstw (M5) — byłaby zmyślona. Częściowe napełnienie zbiorników jest **warunkiem zobaczenia tankowania w ogóle**: pełny bak starczy na kilkaset kilometrów, czyli na kilka tygodni dojazdów, więc świat startujący „wszyscy na pełnym" pokazałby stacje dopiero w czwartym tygodniu gry |
| L-13 ★ | **Moduł `walk` skasowany w całości** (`Z-1`, `Y-2`). `Sources.travel` to `Box<dyn TravelOracle>`, `Populated` traci pola `nodes` i `segments` (`Z-3`), a `sim/agents` dostaje **`StraightLineTravel`** — dubler testowy z szacunkiem manhattanowym, dla własnych testów i benchmarków crate'u | `sim/agents` nie może zależeć od `sim/traffic` (zależność idzie w drugą stronę), a jego testy planera potrzebują jakiegokolwiek czasu dojścia. Dwie konsekwencje wpisane od razu: test architektury `walk_nie_wycieka` zamienia się w `trasa_nie_wycieka` i pilnuje teraz, że z `sim/agents` nie wychodzi **żaden** typ trasy; test wydajności `walk_estimate` traci kryterium „cache przyspiesza dziesięciokrotnie", bo szacunek nie ma już cache'u po tej stronie. **Złote wydruki zostały zachowane bez zmian** — patrz `L-19` |
| L-19 ★ | **Ranking kandydatów traci mnożnik nadłożenia 1,25 i zaokrągla czas w górę.** `places::walk_minutes` liczy teraz czystą odległość manhattanowa podzieloną przez prędkość marszu, z zaokrągleniem do góry | Pierwsza wersja dublera przepisała mnożnik z fallbacku `WalkOracle` i **złote wydruki planu przestały pokazywać przypadek `ChosenOnRoute`** z PRD §5.5 (zakupy po drodze z pracy) — Anna zaczęła kupować wieczorem przy domu. Diagnoza jest pouczająca: odległość manhattanowa **jest** odległością po siatce ulic wszędzie tam, gdzie ulice biegną wzdłuż osi, więc w scenie testowej (i w większości miasta M2) mnożnik był czystym błędem, a nie korektą. Korekta za nadłożenie należy do estymatora, który wie, że nie ma sieci — czyli do ścieżki zapasowej w `magnat_traffic`, a nie do rankingu. Zaokrąglenie w górę odtwarza zachowanie `WalkOracle`: dojście trwające 3,7 minuty kończy się w czwartej. Po obu poprawkach **wszystkie złote wydruki M3 — planera i karty inspekcji — przechodzą bez przeliczania**, co jest jedynym dowodem, że podmiana warstwy transportu nie zmieniła doby mieszkańca w niczym poza ruchem |
| L-14 | **`TravelOracle` rośnie o sześć metod warstwy Mikro** (`enter_micro`, `micro_step`, `micro_retire`, `micro_len`, `set_micro_window`, `micro_snapshot`), wszystkie po `&self` i wszystkie z pustą implementacją domyślną | Do M4b te metody były inherentne na `WalkOracle` i wołał je `WalkMicroSystem` (teraz `TravelMicroSystem`) oraz klient graficzny. Po podmianie pola na `Box<dyn TravelOracle>` nie było ich jak zawołać. `&self` jest wymuszone przez system, który deklaruje **odczyt** zasobu `AgentSources`; puste ciała domyślne są wymuszone przez to, czym Mikro jest: implementacja bez warstwy wizualnej nadal spełnia kontrakt ekonomiczny w całości (00 §4) |
| L-15 ★ | **Zmierzone, doba gry.** Miasto 4 km (28,5 tys. mieszkańców, 3 927 pojazdów, 5 stacji): 5 782 przejazdy, **wszystkie zakończone**, zero porażek. Metropolia 16 km (274 tys. mieszkańców, 38 tys. pojazdów, 50 stacji): 64 737 przejazdów, **13 porażek (0,02 %)**, 4,79 mln rozliczeń krawędzi, doba 21,3 s. W obu bilans pojazdów i bilans paliwa domknięte co do jednostki i zero podróży bez uzasadnienia. Rozbiór czasu przejazdu (metropolia): **jazda 9,9 min, skrzyżowania 3,6 min, kolejki 25,5 min**; przejazd trwa **1,8×** dłużej niż plan z narzutem 3,0 | Kolejki są trzy razy droższe od samej jazdy i to jest **znalezisko o routingu, nie o modelu ruchu**: router liczy wagi z prędkości swobodnych i nie ma sprzężenia zwrotnego z obciążeniem, więc wszyscy dostają tę samą najkrótszą trasę i zjeżdżają się na te same kolektory. Mechanizm, który to rozwiązuje, jest zaprojektowany i przypisany — `TravelTimeMatrix` plus kustomizacja wag CCH — ale jego konsumentem jest M4c/WP6. Do tego czasu narzut planistyczny (3,0 × czas swobodny) zabiera połowę błędu, a reszta zostaje spóźnieniem; spóźnienie z korka jest treścią tej fazy, nie jej wadą |
| L-16 ★ | **Prędkość przy zapełnieniu w `data/roads/vdf.ron` wyznacza tempo rozładowania korka i pierwsza kalibracja miała ją o połowę za niską.** Przepływ przy gęstości korkowej to `pasy × v / odstęp`, więc 70 dkmh na jednym pasie przy odstępie 7,5 m daje 15 pojazdów na minutę, a realna kolejka zjeżdża po zielonym w tempie ~30. Po podniesieniu wartości do 110–160 dkmh najdłuższe czekanie spadło z 30 do 15 minut, a rozplątania z 2 589 do 1 186 | Wniosek szerszy niż M4: w diagramie podstawowym **to prędkość minimalna, a nie zadeklarowana przepustowość pasa, jest parametrem przepustowości sieci**. `capacity_vph_per_lane` nie wchodzi do wzoru prędkości w ogóle — konsumuje ją model węzła i nakładka natężenia. Kto będzie kalibrował VDF w M4d, ma zaczynać od `min_speed_dkmh` |
| L-17 | **Test wydajności warstwy Mikro przeniósł się do `sim/traffic/tests/mezo.rs`** razem z warstwą (`Z-4`), a `sim/agents` przestał go mieć | Bufor pieszych mieszka teraz w `magnat_traffic::MicroLayer`, a `sim/agents` nie zależy od crate'u ruchu. Kryterium (5 tys. pieszych ≤ 2 ms na klatkę) jest to samo, mierzone na tym samym buforze |
| L-20 | **Bloki `DecisionReason` i `StreamId` fazy otwarte i częściowo zajęte.** `DecisionReason` 200–204: `ModeChosen`, `NoRouteForMode`, `RefuelNeeded`, `StationChosen`, `TripDelayed`. `StreamId` 160–162: `ModeChoice`, `FuelStationChoice`, `VehicleSeed`. Wolne dla M4c i M4d: powody 205–299, strumienie 163–179 | `VehicleSeed` nie był w planie — obsadzenie floty przy generacji świata potrzebuje własnego strumienia, bo losuje per gospodarstwo, a nie per decyzja. `ParkingSearch`, `TransitDwell` i `VehicleBreakdown` z kontraktu M4 §6 **nie zostały dopisane**: strumień bez ani jednego wywołania to numer zarezerwowany na wyrost, a numery raz nadane są wieczne. Dopisze je podfaza, która ich użyje — blok jest jej i nikt inny go nie ruszy. Wszystkie pięć powodów jest **faktycznie produkowanych** (bramka 5): wybór środka i brak trasy przy planowaniu, tankowanie i wybór stacji przy objeździe, spóźnienie przy przybyciu; licznik `TrafficStats.reasons` liczy je per podróż, a test własnościowy paliwa sprawdza, że żadna podróż nie kończy się bez powodu. Przy okazji trzeba było przeliczyć oczekiwany komunikat kompilatora w `engine/core/tests/ui/` — mechanizm `K-12` działa przez **treść błędu**, więc dopisanie wariantu zmienia też wzorzec testu `trybuild` |
| L-18 | **Rozstrzygnięcia decyzji otwartych §9 fazy:** `D3` (amortyzacja) przyjęta jak w propozycji — `wear_gr_per_100km` jest w katalogu i wchodzi **tylko** do kosztu decyzji, a obciążenie budżetu nalicza M5 z przebiegu; `D6` (bateria EV) **rozstrzygnięta odwrotnie niż domyślnie** — jednostka zbiornika zależy od `FuelKind`, a nie od typu (`enum EnergyStore`), bo żadna klasa w `data/` nie jest elektryczna i drugi wariant nie miałby ani jednej instancji; `D9` (cena paliwa) przyjęta — cena jest w `data/vehicles/classes.ron` za `FuelKind`, a M4 czyta ją przez `VehicleCatalog::price_gr`, więc podmiana na ofertę `sim/economy` w M5 nie rusza kodu ruchu; `D10` (sygnalizacja) przyjęta — stałoczasowa, cykl 90 s, opóźnienie z członu równomiernego Webstera | `D6` jest jedyną zmianą wobec propozycji i jest odwracalna w jednym typie: `FuelTank` ma pięć pól i jednego konstruktora. `D1`, `D2`, `D4`, `D5`, `D7`, `D8`, `D11` nie dotyczą tej podfazy i zostają otwarte z dotychczasowymi adresatami |
