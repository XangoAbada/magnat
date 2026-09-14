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
