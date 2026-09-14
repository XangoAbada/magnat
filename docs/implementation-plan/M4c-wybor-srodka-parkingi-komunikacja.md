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

---

## Zmiany wpisane po M4b

Zgodnie z `K-18`. To są rzeczy, o których M4c wie **na pewno** po zamknięciu M4b;
M4c nie jest tu przeprojektowywany. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| M-1 ★ | **Wybór środka transportu już istnieje — w postaci reguły zastępczej, którą WP6 ma wymienić.** Dziś decyduje `TrafficOracle::plan`: kto ma przypisany pojazd i jedzie dalej niż `MIN_CAR_DISTANCE_CM` (800 m), ten jedzie; reszta idzie pieszo. Trzy stałe do zastąpienia mają nazwy i komentarze `ponytail:` w kodzie: `MIN_CAR_DISTANCE_CM`, `PLANNING_MARGIN_PERMILLE`, `FUEL_RESERVE_FACTOR` | Punkt podmiany jest jedną funkcją, a nie rozsypanym warunkiem: `plan()` zwraca parę `(środek, minuty)` i jest wołana z `estimate` **i** z `begin_trip`, więc plan i przejazd nie mogą się rozjechać. `evaluate_modes` z §5.3 wchodzi dokładnie w to miejsce |
| M-2 ★ | **`TravelTimeMatrix` jest pusta i to M4c ma rozstrzygnąć, gdzie ona mieszka** (`L-8` w M4b). Dziś siedzi w `NavRouter`, czyli wewnątrz `AgentSources` — a ten zasób jest **jawnie wyłączony z hasha stanu** jako „dane wejściowe miasta, nie stan" | Macierz karmiona obserwacjami jest stanem symulacji: wpływa na plan, a plan na świat. Zostawienie jej w zasobie, który deklaruje, że stanu nie ma, przepuściłoby rozjazd przez test determinizmu. Jej **pierwszym konsumentem jest WP6**, więc decyzja należy do tej podfazy: albo macierz przenosi się do osobnego zasobu z hakiem hasha, albo zostaje tam, gdzie jest, i wtedy nie wolno jej karmić |
| M-3 ★ | **Dostępność pojazdu jest dziś binarna i pilnuje jej `TrafficOracle`**: pojazd w podróży nie jest opcją dla drugiej podróży tego samego mieszkańca (pole `busy`, zwalniane przy `Arrived`/`Failed`). Nie ma modelu „auto zostało pod pracą" ani „auto wziął ktoś inny z gospodarstwa" | Bez tej blokady mieszkaniec stojący w korku zgłaszał przy każdym przeplanowaniu kolejną podróż tym samym autem, a każda wjeżdżała na sieć osobno — flota rozmnażała się dokładnie wtedy, kiedy było najciaśniej. WP6 potrzebuje pełnego modelu (auto podąża za kierowcą, parking u celu), a `busy` jest miejscem, w które on wchodzi |
| M-4 ★ | **Zlecenie, którego sieć nie przyjmie, musi dostać zastępcze `Arrive`** — inaczej mieszkaniec stoi do końca gry. Ścieżka jest w `TrafficSystem` (`odrzucone`) i WP6 z WP7 muszą ją uszanować: brak wolnego miejsca parkingowego odbiera **opcję przy planowaniu**, a nie podróż w trakcie | To ta sama reguła co `no_mid_trip_restriction_failure == 0` z M4 §7.1, tylko widziana od strony agenta. Wypuszczenie podróży, która nie ma jak się skończyć, jest cichym zawieszeniem mieszkańca — najgorszym rodzajem błędu, bo nie wywala się, tylko zatrzymuje miasto |
| M-5 | **Objazd na stację jest gotowym wzorcem dla objazdu na parking.** Trasa dzieli się na odcinki (`PendingTrip.legs`), postój jest stałą (`REFUEL_DWELL_MIN`), a wybór celu idzie przez `station_on_route` — kandydat o najmniejszym nadłożeniu w linii prostej, remisy po indeksie | WP7 ma dokładnie ten sam kształt problemu: wybrać punkt w korytarzu trasy, dojechać, postać, jechać dalej. Typ `Station` i funkcja wyboru są w `oracle.rs` i nazwane tak, że parking wejdzie obok, a nie zamiast |
| M-6 | **Opłaty mają miejsce w ledgerze i nikt ich jeszcze nie wystawia.** `LedgerEntry.money` jest zerowe dla zwykłej krawędzi — bilet, parking i myto to jedyne rzeczy, które mają je wypełnić. Druga strona bilansu jest w `FuelLedger` i jest **haszowana** | Paliwo płaci się na stacji, nie na drodze, więc `settle_edge` nie ma czego księgować. Test własnościowy pieniądza działa dziś na parze „kierowca ↔ stacja" i WP7 z WP10 mają go rozszerzyć o parę „kierowca ↔ parking/operator", a nie zbudować drugi licznik |
| M-7 | **Warstwa szynowa metropolii ma 20 węzłów w składowej** (`Y-6`) i nic się nie zmieniło: `RoadClass::RailPassenger` nadal nie ma ani jednej krawędzi. Warstwa autobusowa nie istnieje jako osobny graf — autobus pojedzie po `Modality::Road` | `NavRouter::modality_of` mapuje `Bus` na `Road`, więc routing autobusu działa od razu; czego nie ma, to pasów autobusowych i przystanków jako węzłów (`TransferKind::Stop` jest w enumie i nie ma ani jednej instancji). WP10 zaczyna od przystanków, nie od grafu |
| M-8 | **Postój przyuliczny jest w grafie i nikt go nie czyta.** `RoadEdge.curb_parking` wylicza `nav_build` z klasy i długości (`J-10`), warstwa mezo go nie dotyka | Pole czeka na WP7 dokładnie w tym kształcie, w jakim je zostawił M4a. Jeśli WP7 uzna oszacowanie za za grube, ścieżka wyjścia jest tam opisana: realne dane krawężnika z M2 |

---

## Korekty planu wpisane po implementacji M4c

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Korekta | Dlaczego |
|---|---|---|
| P-1 ★ | **Z trzech punktów WP14 realny był jeden.** Bramka okna Mikro (punkt 2) była już przywrócona przed końcem M4b: `MicroLayer::enter` odcina trasę, której żaden koniec nie mieści się w oknie, a `set_micro_window` jest w traicie `TravelOracle`. Zostały punkt 1 (pętla 600 podkroków) i punkt 3 (dwie kopie zrzutu). | Sekcja §5.12 mówiła wprost „zweryfikuj, co M4b naprawił po drodze" i to jest wynik tej weryfikacji. Punkt 2 zostaje w dokumencie jako zapis, dlaczego go szukano, i dostał **test** — `mikro_utrzymuje_piec_tysiecy_pieszych_w_kadrze` sprawdza teraz także, że bez kadru warstwa nie tworzy encji. Regresja, która raz weszła, ma mieć bramkę |
| P-2 ★ | **Bramką WP14 nie jest benchmark, tylko test liczący wywołania.** `agents_bench.rs` mierzy `StraightLineTravel`, który od M4b ma **puste** ciała metod warstwy Mikro — grupa „m3b-2 ruch" nie mierzy więc niczego od czasu migracji. Kryterium pilnują dwa testy: `warstwa_mikro_jest_krokowana_raz_na_minute` (`sim/agents/tests/contract.rs`, atrapa z licznikiem) i `mikro_utrzymuje_piec_tysiecy_pieszych_w_kadrze` (`sim/traffic/tests/mezo.rs`, 5 tys. pieszych × 60 minut świata, próg 2 ms w release i 30 ms w debugu). | Benchmark w `sim/agents` nie ma jak zobaczyć `MicroLayer`, bo ten mieszka w `sim/traffic`, a zależność idzie w drugą stronę. Co ważniejsze: **liczba wywołań jest własnością, którą da się sprawdzić dokładnie**, a pomiar czasu jest tylko jej przybliżeniem. Pętla 600 podkroków wróciłaby niezauważona przez benchmark na dość szybkiej maszynie; test z licznikiem wywali się zawsze |
| P-3 ★ | **Wybór środka transportu mieszka w `sim/traffic`, nie w `sim/agents`.** M4 §2 i §6 przypisują go do `sim/agents`, ale po M4b punktem podmiany jest `TrafficOracle::plan` (`M-1`), a ten jest w `sim/traffic`. Moduł `mode` z `GeneralizedCost`, `ModeDecision` i `evaluate_modes` powstał więc tam. | Wycena potrzebuje routera, rejestru parkingów i rozkładu komunikacji — wszystkiego, czego `sim/agents` nie widzi i widzieć nie może, bo zależność idzie w drugą stronę. Kontrakt dla M5 i M9 się przez to nie zmienia: czytają `DecisionReason` z `core` i `TripLedger` z `sim/traffic`, nie `ModeDecision` bezpośrednio |
| P-4 ★ | **`TripDecisionReason` nie powstaje jako osobny enum.** `K-12` wymaga **jednego** centralnego `DecisionReason` w `engine/core`, a §5.3 zapowiadał typ obok niego. Uzasadnienie niosą trzy nowe warianty centralnego enuma: `ModeCompared` (205), `NoParkingAtDestination` (206), `LeftBehind` (207); pełna lista kandydatów z rozbiciem kosztu żyje w `ModeDecision.candidates`. | Drugi enum powodów łamałby mechanizm, który `K-12` kupuje: brak ramienia w `engine/ui` ma **nie kompilować** gry. Komentarz w `decision.rs` przewidywał zresztą `ModeCompared` pod kolejnym numerem już w M4b |
| P-5 ★ | **`Infeasible` ma dwa warianty więcej, niż zapowiadał §5.3**: `NoRoute` (2,8 % par metropolii nie ma połączenia — `Y-1`) i `BeyondBudget { fare_gr }` (kurs droższy niż dopuszczalny ułamek dziennego dochodu). | `BeyondBudget` wyszedł z pomiaru: taksówka bez ograniczenia budżetowego zbierała **15,2 % podróży metropolii**, bo dla mieszkańca bez auta i bez zasięgu komunikacji była jedyną szybką opcją. Sama cena jej nie hamuje — koszt uogólniony wybiera ją także wtedy, gdy kurs kosztuje dniówkę. Próg budżetowy jest tu tym, czym parking dla samochodu: **warunkiem wykonalności, nie składnikiem kosztu**. Po jego wprowadzeniu udział spadł do 2,9 % |
| P-6 ★ | **`DiscomfortBreakdown` ma siódmy składnik: `mode_penalty`** — stałą niedogodność środka z `data/roads/mode_choice.ron`. | Bez niego **rower zbierał 92,5 % podróży**: jest darmowy i trzykrotnie szybszy od marszu, a model czasu i pieniądza nie widzi ani kradzieży, ani jazdy po jezdni bez ścieżki, ani tego, że do pracy nie przychodzi się spoconym. To nie jest gałka do dostrajania wyniku, tylko nazwanie składnika, którego brakowało — i dlatego siedzi w danych, z komentarzem, co reprezentuje |
| P-7 ★ | **Widełki rozkładu udziałów powstają w `data/roads/mode_choice.ron` i obowiązują dopiero od `reference_population`.** PRD §20.1 mówi „rozkłady w zakresach referencyjnych **dla epoki**" i liczb nie podaje — kryterium WP6 było niemierzalne, dopóki ich nie było. | Rozkład zależy od **długości podróży**, a ta od wielkości miasta: ten sam model daje 70,6 % marszu w mieście 4 km i 37,8 % w metropolii 16 km. Jedne widełki dla obu nie byłyby bramką, tylko przedziałem „coś między 9 a 71 %". Bramka stoi więc przy scenariuszu odniesienia z §7.3 (miasto 150 tys.); dla mniejszych miast raport wypisuje rozkład i mówi wprost, że go nie ocenia |
| P-8 ★ | **Zasób `TrafficServices` wchodzi do hasha stanu.** Do M4b był jawnie pomijany jako „dane wejściowe miasta" — a już wtedy trzymał `busy`, czyli informację, które auto jest w podróży, i ta wpływa na plan mieszkańca. M4c dokłada parkingi, komunikację, nawyk i `TravelTimeMatrix`. | To rozstrzyga też `M-2`: macierz czasów przejazdu **jest** stanem symulacji (karmiona obserwacjami, wpływa na plan, a plan na świat), więc albo wchodzi do hasha, albo nie wolno jej karmić. Wchodzi. Graf, katalog pojazdów i tabela VDF zostają poza — są wejściem, nie wynikiem |
| P-9 ★ | **`VehicleCatalog::fuel_cost` liczyło tysiąckrotnie za dużo.** Funkcja powstała w M4b przed rozstrzygnięciem `K-25` i dzieliła przez `1_000` (mililitry), a jednostką wewnętrzną ruchu jest mikrolitr. **Nie miała wtedy żadnego wołającego** — tankowanie liczyło się inline w `trip::refuel`, już poprawnie. | Pierwszy konsument (paliwo taboru komunikacji) pokazał błąd natychmiast: 100 litrów oleju napędowego kosztowało 523 tys. zł. To jest argument za tym, żeby funkcja bez wołającego albo miała test, albo nie istniała — martwy kod nie jest neutralny, bo pierwszy użytkownik dziedziczy po nim błąd, którego nikt nie szukał |
| P-10 | **Parking domowy jest warunkiem posiadania auta.** Gospodarstwo, dla którego nie ma miejsca w promieniu 400 m od domu, nie dostaje pojazdu; co czwarte dostaje zamiast tego podjazd (pojemność 1). | Inaczej cała flota startowa byłaby widmami: stałaby pod domem, nie zajmując żadnego miejsca, i `parking_no_ghosts` nie miałby czego pilnować. Skutek jest przy okazji realistyczny — na osiedlu bez krawężnika motoryzacja jest niższa. Zmierzone na metropolii: 37 783 pojazdy zamiast ~47 tys. wynikających z `MOTORISATION_PER_MILLE` |
| P-11 | **Autobus nie zajmuje miejsca w `EdgeQueue`** — cierpi od korka, ale go nie tworzy. Czas przejazdu międzyprzystankowego liczy ten sam `settle_edge`, co dla samochodów, z tego samego `LinkState`. | `ponytail:` sufit nazwany i policzony: ~200 kursów wobec 13 500 pojazdów w szczycie to 1,5 % floty na sieci, czyli mniej niż szerokość widełek kalibracji VDF. Ścieżka wyjścia jest w M4d/WP8: pojazd komunikacji wchodzi do warstwy mikro razem z samochodami i wtedy też do kolejki krawędzi |
| P-12 | **Kierowca autobusu ma etat, który już miał.** `TrafficServices.transit_drivers` to lista mieszkańców; kurs bierze pierwszego, który dziś pracuje (`Employment::works_on`, brak `FLAG_SICK_LEAVE`). Zajezdnia **nie jest** miejscem pracy. | Dodanie zajezdni jako `PlaceRef` wymagałoby ruszenia `PlaceTable` i `Vacancies`, czyli generatora populacji M3d. Kryterium WP10 („kierowca-mieszkaniec ma tę pracę w planie dnia") jest spełnione co do litery: kierowca jest mieszkańcem, jego zmiana jest w planie dnia i nieobecność odwołuje kurs. `ponytail:` ścieżka wyjścia to M7 — rynek pracy daje zajezdni własne wakaty |
| P-13 | **Przesiadka jest realizowana, nie tylko wyceniana.** Pasażer z przesiadką nie dostaje `Arrive` przy wysiadce na przystanku przesiadkowym, tylko wraca do kolejki drugiej linii (`Waiting.next`). | Gdyby dostał, teleportowałby się przez drugą nogę: koszt uogólniony liczyłby ją, a symulacja nie. To ta sama klasa błędu co `M-4` — podróż, która kończy się inaczej, niż obiecał plan |
| P-14 | **Rezerwacja miejsca postojowego powstaje przy wycenie i jest zwalniana, gdy opcja przegra.** Drugim zaworem jest `ParkingRegistry::expire`: blokada ma termin i wygasa, gdy podróż nigdy nie wyruszy. | Rezerwacja przy wycenie jest konieczna: bez niej nie wiadomo ani czy opcja jest wykonalna, ani ile kosztuje dojście od parkingu. Bez zwalniania każda rozważona i odrzucona jazda zabierałaby centrum jedno miejsce do końca gry |
| P-15 | **Pogoda powstaje jako `core::weather_at`** (rozstrzygnięcie `D4` zgodnie z propozycją): trójkąt roczny temperatury plus opad o sezonowym prawdopodobieństwie, całkowitoliczbowo i bez funkcji przestępnych (`K-6`). M8 podmienia ciało, `sim/traffic` nie zmienia ani linii. | `DiscomfortBreakdown.weather` rozstrzyga o udziale roweru i pieszych, więc zaślepka musiała powstać razem z **pierwszym konsumentem**, a nie razem z właścicielem. Ta sama zasada, co przy cenie paliwa (`D9`) |
| P-16 | **`left_behind` liczy ludzi, `boarding_refusals` liczy zdarzenia.** Pierwszy pomiar dał 84 743 „nie zmieściło się" przy 15 265 wsiadających — bo ten sam pasażer był pomijany przez każdy kolejny pełny kurs. | Licznik zdarzeń jest przydatny (mówi, jak długo linia jest przepełniona), ale jako miara przepełnienia kłamie o rząd wielkości. Oba są w `TransitStats` i oba są w raporcie |
| P-17 ★ | **Rezygnacja z czekania jest marszem, nie teleportacją.** `TransitEvent::GaveUp` niesie `walk_min` — czas dojścia pieszo policzony przez oracle **przed** wejściem do kolejki i przechowywany w `Waiting.walk_fallback_min`. | Pierwsza wersja dawała `Arrive` w następnej minucie: mieszkaniec stał 45 minut na przystanku i **docierał do celu natychmiast**, czyli dostawał teleportację w nagrodę za cierpliwość. Na metropolii dotyczyło to 96 tys. podróży na dobę i nie widać tego w żadnym bilansie — ani pieniądz, ani paliwo, ani liczba pojazdów się przez to nie rozjeżdżają. Widać dopiero w profilu doby, i to jest cały powód, dla którego `Commute` jest w nim osobną pozycją |
