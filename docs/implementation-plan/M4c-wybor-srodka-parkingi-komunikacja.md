# M4c — Wybór środka, parkingi, komunikacja

Podfaza 3 z 4 fazy **M4 — Ruch** (`M4-ruch.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M4b (podróże, paliwo). |
| **Pakiety robocze** | WP6, WP7, WP10 |
| **Projekt techniczny** | §5.3, §5.5, §5.6 |
| **Wynik do pokazania** | Rozkład udziału środków transportu w widełkach z PRD §20.1; linia autobusowa wozi ludzi wg rozkładu; przepełniony parking odbiera opcję „samochód”. |
| **Kryterium zamknięcia** | Kryteria WP6, WP7 i WP10; 100 % decyzji transportowych ma uzasadnienie. |
| **Poprzednia / następna** | `M4b-mezo-i-podroze.md` · `M4d-mikro-i-dowod-spojnosci.md` |

Koszt uogólniony i wybór środka transportu z `TripDecisionReason`, parkingi z rezerwacją i cennikiem, komunikacja miejska z rozkładem, taborem i kierowcami-mieszkańcami.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP6** | Wybór środka transportu (§9.4) | WP5, WP7 | Koszt uogólniony w `Money`; wykonalność opcji (dostępność auta w GD, parking u celu, zasięg baku, rozkład komunikacji); wybór + `TripDecisionReason` z kosztami wszystkich kandydatów. Rozszerza `sim/agents`. | Rozkład udziału środków transportu w scenariuszu referencyjnym w zakresach z §20.1; 100 % decyzji ma uzasadnienie; sklep bez parkingu traci klientów zmotoryzowanych (mierzalne) |
| **WP7** | Parkingi | WP1 | `ParkingLot` z pojemnością, rezerwacją na okno czasowe, cennikiem, promieniem dojścia. Parking przyuliczny jako pojemność krawędzi. Szukanie miejsca = czas + ryzyko porażki. | Przepełniony parking blokuje opcję „samochód"; nakładka obłożenia; brak „pojazdów widmo" — każdy zaparkowany pojazd zajmuje miejsce |
| **WP10** | Komunikacja miejska | WP4, WP6 | `TransitLine`, `TransitStop`, rozkład, tabor, kierowcy jako mieszkańcy z grafikiem, wsiadanie z limitem pojemności, przesiadki, przepełnienie → pasażer zostaje. Routing multimodalny: dojście + oczekiwanie + przejazd + przesiadka. | Linia autobusowa wozi ludzi wg rozkładu; przepełnienie w szczycie generuje spóźnienia; kierowca-mieszkaniec ma tę pracę w planie dnia |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.3 Wybór środka transportu (§9.4)

Wszystko liczone w `Money` (grosze, i64 — dok. 00 §2). Wygoda jest **zmonetyzowana**, nie jest
osobną skalą — to eliminuje wagi bez jednostki i czyni decyzję wyjaśnialną w karcie inspekcji.

```rust
pub struct GeneralizedCost {
    pub time_minutes: u16,
    pub vot_gr_per_min: i64,          // wartość czasu: f(dochód godzinowy GD, purpose)
    pub time_cost: Money,             // time_minutes * vot_gr_per_min
    pub money_cost: Money,            // paliwo + bilet + parking + amortyzacja + myto
    pub discomfort_cost: Money,       // suma składników poniżej
    pub total: Money,
}

pub struct DiscomfortBreakdown {
    pub weather: Money,               // deszcz/mróz × ekspozycja środka (pieszo/rower boli, auto nie)
    pub luggage: Money,               // masa bagażu × ekspozycja
    pub crowding: Money,              // obłożenie pojazdu komunikacji / (pojemność)
    pub transfers: Money,             // stała kara per przesiadka
    pub status: Money,                // dysonans statusu (§5.4) — ujemny dla auta u osoby o wysokim statusie
    pub walk_access: Money,           // dojście do przystanku/parkingu ponad próg
}

pub fn evaluate_modes(
    ctx: &ModeContext,                // mieszkaniec, GD, pogoda, godzina, kalendarz
    req: &TripRequest,
    nav: &NavServices,
) -> ModeDecision;

pub struct ModeDecision {
    pub chosen: TravelMode,
    pub candidates: SmallVec<[(TravelMode, GeneralizedCost, Option<Infeasible>); 8]>,
    pub reason: TripDecisionReason,   // dok. 00 §7 — enum z parametrami, nie string
}

pub enum Infeasible {
    NoCarInHousehold,
    CarInUseBy(CitizenId),
    NoParkingWithinRadius { lot_searched: u16 },
    InsufficientFuelRange { range_m: u32, needed_m: u32 },
    NoTransitConnection,
    DistanceOverPersonalLimit { mode: TravelMode },
    BelowMinimumAge,
    VehicleBroken,
}
```

Kandydaci (§9.4): `Walk`, `Bike`, `Transit`, `CarOwn`, `CarHousehold`, `Taxi`, `Carpool(driver)`.
Procedura: wyznacz zbiór wykonalnych → policz `GeneralizedCost` każdego → **wybierz minimum**
z bonusem nawyku (`habit_bonus` = stała × świeżość poprzedniego wyboru dla tej pary
origin–dest–purpose). Nie stosujemy modelu logitowego — argmin z nawykiem daje stabilny,
wyjaśnialny wybór i jedną deterministyczną ścieżkę. Rozstrzyganie remisów: kolejność w enumie
`TravelMode`. Losowość wchodzi wyłącznie tam, gdzie modeluje niewiedzę: wybór stacji paliw
i wybór miejsca parkingowego, przez `rng(world_seed, StreamId::ModeChoice, citizen_index, tick)`.

`vot_gr_per_min` = `hourly_net_income_gr / 60 × purpose_multiplier`, z podłogą (czas ma wartość
także dla bezrobotnego) i sufitem. Mnożniki per `TripPurpose` w `data/` — to główna gałka
balansująca udział środków transportu.

### 5.5 Parkingi

```rust
pub struct ParkingLot {
    pub building: Option<BuildingId>, // parking przy sklepie/biurze; None = samodzielny
    pub capacity: u16,
    pub occupied: u16,
    pub reservations: BinaryHeap<Reverse<(SimMinute /*zwolnienie*/, VehicleId)>>,
    pub price_gr_per_hour: Money,     // 0 = darmowy; taryfy miejskie dopiero M8
    pub access_node: NodeId,
    pub walk_radius_m: u16,           // ile mieszkaniec zaakceptuje dojścia
    pub kind: ParkingKind,            // Surface | Underground | Curb | Private(FirmId)
}

/// Rezerwacja na okno czasowe — pojazd „widmo" nie istnieje, każde auto zajmuje miejsce.
pub fn try_reserve(lot: &mut ParkingLot, from: SimMinute, to: SimMinute, v: VehicleId)
    -> Result<ParkingSlotRef, ParkingDenied>;
```

Wyszukiwanie: kandydaci = parkingi w promieniu dojścia od celu, posortowani po
`(koszt_dojścia_gr + opłata_gr, lot_index)` — deterministycznie. Brak wolnego miejsca w całym
promieniu → `Infeasible::NoParkingWithinRadius` → opcja `CarOwn` odpada w §5.3. To realizuje wprost
zdanie z PRD §9.4: *brak parkingu przy sklepie zmniejsza jego zasięg dla kierowców* — i wprost
uzasadnienie z §14.1: *„dlaczego Anna nie kupiła u mnie?" → „brak parkingu"*.

Parking przyuliczny (`curb_parking` na krawędzi) to `ParkingLot` syntetyczny per krawędź —
ta sama ścieżka kodu, żadnego drugiego mechanizmu.

### 5.6 Komunikacja miejska (§9.3)

```rust
pub struct TransitLine {
    pub id: LineId,
    pub mode: TransitMode,            // Bus | Tram | SuburbanRail | Metro
    pub stops: Vec<TransitStop>,      // w kolejności, z czasem przejazdu międzyprzystankowego
    pub path: Vec<EdgeId>,            // dla autobusu — krawędzie drogowe (dzieli korki!)
    pub timetable: Timetable,         // odjazdy z pierwszego przystanku per typ dnia
    pub fleet: Vec<VehicleId>,
    pub operator: OperatorRef,        // City | Firm(FirmId) — przetargi i dotacje: M8
    pub fare: Money,
}

pub struct TransitStop {
    pub node: NodeId,                 // węzeł grafu pieszego + TransferLink do drogi
    pub dwell_base_s: u16,
    pub waiting: Vec<CitizenId>,      // deterministyczna kolejka — FIFO po minucie przybycia,
                                      // remisy po entity_index
}

pub struct TransitRun {                  // kurs = instancja rozkładu
    pub line: LineId,
    pub vehicle: VehicleId,
    pub driver: CitizenId,               // mieszkaniec z grafikiem pracy (M3)
    pub occupancy: u16,
    pub capacity: u16,
    pub delay_minutes: i16,              // narastające z LinkState krawędzi trasy
}
```

Kluczowe zachowania:
- **Autobus jedzie tą samą siecią** i podlega temu samemu `settle_edge` — korek opóźnia autobus.
  Tramwaj/metro mają własne krawędzie z wydzielonym torowiskiem (brak wpływu ruchu drogowego).
- **Przepełnienie**: gdy `occupancy == capacity`, wsiadanie jest odrzucane. Pasażer zostaje na
  przystanku, dostaje `TripDecisionReason::LeftBehind { line, run }`, jego zdarzenie przybycia jest
  przeplanowane na następny kurs. Spóźnienie do pracy → hook `LatenessRecorded` dla M5/M7.
- **Kierowca** to mieszkaniec: jego zmiana jest zobowiązaniem stałym w planerze M3; brak kierowcy
  (choroba, brak rekrutacji) = kurs odwołany.
- **Routing multimodalny**: dojście pieszo (A\* na grafie pieszym) → oczekiwanie (z rozkładu, znanego
  mieszkańcowi z jego przystanku — §9.3) → przejazd (z rozkładu + `delay_minutes`) → przesiadka →
  dojście. Wszystkie składowe wchodzą do `GeneralizedCost` z §5.3.
