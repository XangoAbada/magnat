# M4c — Wybór środka, parkingi, komunikacja

Podfaza 3 z 4 fazy **M4 — Ruch** (`M4-ruch.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M4b (podróże, paliwo). |
| **Pakiety robocze** | **WP14 (pierwszy)**, WP6, WP7, WP10 |
| **Projekt techniczny** | §5.12, §5.3, §5.5, §5.6 |
| **Wynik do pokazania** | Rozkład udziału środków transportu w widełkach z PRD §20.1; linia autobusowa wozi ludzi wg rozkładu; przepełniony parking odbiera opcję „samochód”. |
| **Kryterium zamknięcia** | Kryteria WP14, WP6, WP7 i WP10; 100 % decyzji transportowych ma uzasadnienie. |
| **Poprzednia / następna** | `M4b-mezo-i-podroze.md` · `M4d-mikro-i-dowod-spojnosci.md` |

Koszt uogólniony i wybór środka transportu z `TripDecisionReason`, parkingi z rezerwacją i cennikiem, komunikacja miejska z rozkładem, taborem i kierowcami-mieszkańcami. **Podfaza zaczyna się od WP14** — spłaty kosztu warstwy Mikro, który wyszedł w trakcie M4b (§5.12).

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP14** | **Koszt warstwy Mikro** — *robiony pierwszy, przed WP6* | M4b | Trzy niezależne przyczyny spadku klatek przy dużej liczbie pieszych, wszystkie w kodzie warstwy Mikro napisanym w M4b: krokowanie bezstanowego bufora 600 razy na minutę, brak bramki okna Mikro po migracji `walk` → `sim/traffic` (regresja wobec `Z-6`) i podwójna kopia zrzutu na klatkę. Diagnoza, podział własności i **lista do weryfikacji na starcie** w §5.12. | Krok Mikro dla 5 tys. pieszych ≤ 2 ms **na minutę świata** (nie na wywołanie — patrz §5.12); `enter_micro` poza oknem nie tworzy encji (test); zrzut do renderera kopiowany raz |
| **WP6** | Wybór środka transportu (§9.4) | WP5, WP7 | Koszt uogólniony w `Money`; wykonalność opcji (dostępność auta w GD, parking u celu, zasięg baku, rozkład komunikacji); wybór + `TripDecisionReason` z kosztami wszystkich kandydatów. Rozszerza `sim/agents`. | Rozkład udziału środków transportu w scenariuszu referencyjnym w zakresach z §20.1; 100 % decyzji ma uzasadnienie; sklep bez parkingu traci klientów zmotoryzowanych (mierzalne) |
| **WP7** | Parkingi | WP1 | `ParkingLot` z pojemnością, rezerwacją na okno czasowe, cennikiem, promieniem dojścia. Parking przyuliczny jako pojemność krawędzi. Szukanie miejsca = czas + ryzyko porażki. | Przepełniony parking blokuje opcję „samochód"; nakładka obłożenia; brak „pojazdów widmo" — każdy zaparkowany pojazd zajmuje miejsce |
| **WP10** | Komunikacja miejska | WP4, WP6 | `TransitLine`, `TransitStop`, rozkład, tabor, kierowcy jako mieszkańcy z grafikiem, wsiadanie z limitem pojemności, przesiadki, przepełnienie → pasażer zostaje. Routing multimodalny: dojście + oczekiwanie + przejazd + przesiadka. | Linia autobusowa wozi ludzi wg rozkładu; przepełnienie w szczycie generuje spóźnienia; kierowca-mieszkaniec ma tę pracę w planie dnia |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść. Wyjątkiem jest **§5.12**,
która jest nowa: przyszła z pakietem WP14 w trakcie M4b, a nie z podziału zakresu fazy.

### 5.12 Koszt warstwy Mikro (WP14)

Sekcja **nowa** — nie ma odpowiednika w pierwotnej numeracji fazy. Powstała z obserwacji zgłoszonej
w trakcie implementacji M4b („spadki wydajności, gdy na ekranie pojawia się dużo przechodniów")
i z prześledzenia ścieżki pieszego od bufora do kadru.

**Dlaczego pakiet stoi tutaj, a nie w M4b, skoro kod jest tamtejszy.** Bo M4b był w trakcie
implementacji, kiedy to wyszło, a pakiet dopisany do trwającej podfazy ginie — czyta ją ktoś,
kto ma tabelę WP w głowie sprzed poprawki. Podfaza następna jest pierwszym dokumentem, który ktoś
przeczyta **od początku**. Stąd też WP14 jest pierwszy w kolejności M4c: to spłata długu, a nie
nowa funkcja, i im dłużej czeka, tym więcej kodu na nim stoi.

**Zanim zaczniesz: zweryfikuj, co M4b naprawił po drodze.** Poniższa diagnoza jest stanem z chwili
zgłoszenia, czyli sprzed końca M4b. Trzy punkty są niezależne i każdy mógł zostać po drodze
zamknięty — sprawdź każdy osobno i odhacz, zamiast poprawiać coś, co już działa. Punkt, który
okaże się naprawiony, zostaje w tej sekcji jako zapis, dlaczego go szukano.

#### 1. Bufor jest krokowany 600 razy na minutę, a jest bezstanowy

`TravelMicroSystem` (`sim/agents/src/systems.rs`) woła `micro_step` w pętli
`0..MICRO_STEPS_PER_TICK`, czyli **600 razy na minutę świata**. Tymczasem `Micro::step`
(`sim/traffic/src/micro.rs`) jest **czystą funkcją czasu**: `progress` i `pos` liczą się
z `now_ms`, `depart_min` i `arrive_min`, nic się między krokami nie akumuluje. Każdy przebieg
nadpisuje poprzedni, więc **599 z 600 przebiegów jest wyrzucanych**. Do tego `punkt_na_lamanej`
skanuje segmenty trasy liniowo, więc koszt jednej minuty świata to
`600 × pieszych × segmentów`, a pętla `advance()` przewija wiele minut na klatkę.

Te 600 kroków nie kupuje nawet płynności: renderer czyta `micro_snapshot` **raz na klatkę**,
czyli widzi wyłącznie stan po ostatnim kroku. Piesi i tak przeskakują co minutę świata.
Płynność wymagałaby kroku sterowanego czasem klatki, a nie tickiem symulacji — to osobna
sprawa i należy do M11b (animacja), nie tutaj.

**Naprawa:** jedno wywołanie na minutę, na końcu minuty. Pętla znika.

**Dlaczego kryterium M3b tego nie złapało.** M3b §7.5 mierzył „krok Mikro dla 5 tys. pieszych
**47 µs**" przy progu **2 ms/klatkę** — i to jest pomiar **jednego wywołania** `micro_step`.
System robi ich 600, więc realny koszt tej samej sceny to `600 × 47 µs ≈ 28 ms` na minutę świata,
czyli **czternastokrotność progu**, który raportowano jako spełniony z zapasem rzędu wielkości.
Bench mierzył coś innego niż to, co robi kod. To ta sama nauka, którą dziennik zapisał po M2e —
kryterium, którego nikt nie puścił na realnej ścieżce wywołań, jest hipotezą, a nie kryterium —
więc kryterium WP14 mówi wprost **„na minutę świata"**, nie „na wywołanie".

#### 2. Bramka okna Mikro wypadła przy migracji (regresja wobec `Z-6`)

`Z-6` w dokumencie fazy jest jednoznaczne: *„Warstwa Mikro ma okno… Pieszy wchodzi w nią
w chwili, gdy zaczyna podróż, i tylko jeśli któryś koniec trasy mieści się w oknie.
M4 przejmuje ten kontrakt razem z buforem."* Stary `WalkOracle::enter_micro` odcinał przez
`if !w_oknie(from) && !w_oknie(to) { return; }`.

W chwili zgłoszenia bramki **nie było**: `enter_micro` w `sim/traffic/src/oracle.rs` wpuszczało
każdą podróż pieszą w mieście, a `set_micro_window` zniknęło z traitu `TravelOracle` (zostało
w trzech wywołaniach w `tools/magnat/src/citizens.rs` i w teście `budgets.rs`, które bez niego
się nie kompilowały — migracja była wtedy w połowie). Bez bramki „5 tys. pieszych **w kadrze**"
z kryterium M3b przestaje być liczbą pieszych w kadrze i staje się liczbą pieszych w mieście:
przy 274 tys. mieszkańców w szczycie porannym to rząd wielkości więcej encji, z których każda
jest krokowana zgodnie z punktem 1.

**Naprawa:** okno po stronie `sim/traffic`, z bramką w `enter_micro`, nie u wołającego.
To jest wymóg `Z-6`, nie optymalizacja: `DayLoopSystem` ma wołać bezwarunkowo i nic nie wiedzieć
o kamerze, a headless ma nie płacić nic. Razem z oknem wraca zastrzeżenie z `Z-6`/`H-28` — okno
musi być otwarte, **zanim** ruszy doba, którą chce się oglądać.

#### 3. Zrzut do renderera kopiowany dwa razy na klatkę

`micro_snapshot` kopiuje cały bufor pod mutexem do wektora pośredniego, a wołający
(`tools/magnat/src/citizens.rs`) przepisuje go natychmiast drugi raz, tylko po to, żeby odrzucić
pole `progress`. Dwie pełne kopie `O(n)` na klatkę zamiast jednej.

**Naprawa:** jeden bufor. Renderer bierze to, co dostaje, albo `micro_snapshot` wypełnia od razu
strukturę docelową. Selekcja kadru (promień + frustum) po stronie renderera jest w porządku
i zostaje bez zmian — problemem jest wejście, które punkt 2 rozdmuchuje, a nie samo odcinanie.

#### Co do WP14 **nie** należy

| Znalezisko | Dlaczego nie tutaj | Adresat |
|---|---|---|
| Pass `pick_id` rysuje wszystkich pieszych **drugi raz w każdej klatce** (pełna geometria + czyszczenie tekstury ID wielkości okna), niezależnie od tego, czy kursor cokolwiek wskazuje — a pozycja kursora jest znana przed nagraniem passa | To `engine/render`, nie `sim/traffic`; pass powstał w M3d razem z pickingiem pieszych i jest kosztem renderu, nie symulacji | **M11e/WP10** — dopisane tam jako pomiar z terminem |
| Ten sam pass **nie jest mierzony**: `PASS_NAMES` ma sześć pozycji, a `pick_id` jest siódmy i nie ma znaczników czasu | jw. — dopóki nie jest mierzony, żaden budżet klatki go nie widzi | **M11e/WP10** |

#### Pomiar

Bramką regresji jest istniejąca grupa benchmarków ruchu (`m3b-2 ruch` w `agents_bench.rs`),
ale mierzona **na pełnej ścieżce systemu**, nie na pojedynczym `micro_step` — inaczej WP14
powtórzyłby błąd, który naprawia. Scenariusz: 5 tys. pieszych, okno otwarte, jedna minuta świata.

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
