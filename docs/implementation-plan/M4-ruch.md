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

## 4. Pakiety robocze

Kolejność jest kolejnością wykonania. `→` oznacza twardą zależność.

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP1** | `RoadGraph` i grafy modalne | M2 (`sim/world` drogi) | Budowa grafu z geometrii dróg M2: węzły, krawędzie kierunkowe, pasy, klasa, limit, tonaż, gradient, most. Warstwy: `Road`, `Foot`, `Bike`, `Rail`. Połączenia międzymodalne (przystanek↔chodnik, parking↔chodnik). Walidacja: spójność, brak krawędzi wiszących, każda parcela ma dostęp pieszy. | Graf miasta 150 tys. buduje się < 400 ms; walidator zielony; inspektor grafu w devtools rysuje krawędzie z atrybutami |
| **WP2** | Routing: CCH + A\* + cache | WP1 | Kolejność kontrakcji z zagnieżdżonej dysekcji (raz), kustomizacja wag przy zmianie prędkości/przepustowości, pełna rekontrakcja przy zmianie topologii — obie w tle z **deterministycznym budżetem pracy na tick**. A\* z heurystyką landmarkową dla grafu pieszego/rowerowego. `RouteCache` (klucz: `(origin_node, dest_node, mode, profile_hour_bucket)`). `TravelTimeMatrix` dzielnica × godzina × środek, aktualizowana z obserwacji EWMA. | Zapytanie CCH ≤ 60 µs (p95) na grafie 200 tys. węzłów; trafialność cache ≥ 90 % w scenariuszu doby; rekontrakcja kończy się w **tym samym ticku** w dwóch przebiegach tego samego seeda |
| **WP3** | Warstwa mezo | WP1 | `LinkState` per krawędź (przepływ, gęstość, prędkość średnia, przepustowość), `EdgeQueue` (kopiec `(exit_minute, vehicle)`), funkcja przepustowości VDF, model węzła: przepustowość per ruch skrętny z kolejką. **`settle_edge` / `settle_node` — jedyne źródło prawdy ekonomicznej.** | Pojazdy przejeżdżają miasto bez wizualizacji; korek na przewężeniu powstaje i rozładowuje się; test zachowania: liczba pojazdów w systemie = wjazdy − wyjazdy |
| **WP4** | `TripRequest` i integracja z DES M3 | WP2, WP3 | API podróży: agent zgłasza `TripRequest`, dostaje `TripId` i zdarzenie `TripArrived` w kolejce czasu M3. Przerwanie i przeplanowanie (korek, zamknięta droga). `TripLedger` — rejestr przejazdu per krawędź. | Mieszkańcy z M3 dojeżdżają do pracy pojazdami zamiast teleportacji; oś czasu dnia w karcie inspekcji pokazuje realne czasy |
| **WP5** | Paliwo, energia, pojazd jako encja | WP4 | Komponenty pojazdu, `FuelTank`, zużycie per przejazd rozliczane w `settle_edge`, zużycie techniczne i przebieg. Tankowanie: próg → zadanie w planie dnia → wybór stacji wg użyteczności (cena, nadłożenie trasy, marka, kolejka). | Auto z pustym bakiem nie jest opcją transportową; Anna tankuje po drodze; suma paliwa = zatankowane − spalone (test własnościowy) |
| **WP6** | Wybór środka transportu (§9.4) | WP5, WP7 | Koszt uogólniony w `Money`; wykonalność opcji (dostępność auta w GD, parking u celu, zasięg baku, rozkład komunikacji); wybór + `TripDecisionReason` z kosztami wszystkich kandydatów. Rozszerza `sim/agents`. | Rozkład udziału środków transportu w scenariuszu referencyjnym w zakresach z §20.1; 100 % decyzji ma uzasadnienie; sklep bez parkingu traci klientów zmotoryzowanych (mierzalne) |
| **WP7** | Parkingi | WP1 | `ParkingLot` z pojemnością, rezerwacją na okno czasowe, cennikiem, promieniem dojścia. Parking przyuliczny jako pojemność krawędzi. Szukanie miejsca = czas + ryzyko porażki. | Przepełniony parking blokuje opcję „samochód"; nakładka obłożenia; brak „pojazdów widmo" — każdy zaparkowany pojazd zajmuje miejsce |
| **WP8** | Warstwa mikro | WP3 | IDM (car-following), MOBIL (zmiana pasa), sygnalizacja (fazy, cykl), ronda, pierwszeństwo. Aktywna tylko dla krawędzi w LOD Mikro. **Dostęp read-only do `LinkState` i `TripLedger`** — wymuszone deklaracją systemu w schedulerze. Serwo domykające czas przejazdu do wartości zaksięgowanej. | Ruch w kadrze wygląda poprawnie: kolejki przed światłem, wjazdy na rondo, wyprzedzanie; deklarowany zbiór zapisów systemu mikro nie zawiera żadnego komponentu ekonomicznego (test) |
| **WP9** | **Dowód spójności mikro↔mezo** | WP8 | Harness `micro_mezo_equivalence` + kalibrator offline parametrów IDM ↔ VDF w `tools/balansator`. Szczegóły w §5.4 i §7.2. | Twarde asercje (pieniądz, paliwo, minuta przybycia) z tolerancją 0; miernik dryfu kalibracji w normie; test „kamera nie zmienia świata" zielony |
| **WP10** | Komunikacja miejska | WP4, WP6 | `TransitLine`, `TransitStop`, rozkład, tabor, kierowcy jako mieszkańcy z grafikiem, wsiadanie z limitem pojemności, przesiadki, przepełnienie → pasażer zostaje. Routing multimodalny: dojście + oczekiwanie + przejazd + przesiadka. | Linia autobusowa wozi ludzi wg rozkładu; przepełnienie w szczycie generuje spóźnienia; kierowca-mieszkaniec ma tę pracę w planie dnia |
| **WP11** | Nakładki UI i inspekcja | WP3, WP7, WP10 | Nakładki: natężenie, korki, izochrony, parkingi, obciążenie linii. Karta inspekcji podróży i pojazdu. Filtr „pokaż tylko X" (§14.2). | Nakładki działają na snapshocie double-buffered, bez blokowania symulacji; przełączanie nakładki < 1 klatka |
| **WP12** | Wydajność i determinizm | wszystkie | Profilowanie, chunkowanie jobów, budżety, hash stanu ruchu w funkcji haszującej ECS, benchmarki criterion. | Budżety z §7.3 dotrzymane; dwa przebiegi tego samego seeda = identyczny ciąg hashy przez 100 dni gry |

Ścieżka krytyczna: WP1 → WP2 → WP3 → WP4 → WP6. WP8/WP9 mogą iść równolegle do WP10 po WP3.

---

## 5. Projekt techniczny

### 5.1 `engine/nav` — grafy i routing

```rust
// ---- Graf sieci --------------------------------------------------------

pub struct NodeId(pub u32);        // indeks w grafie, nie Entity — graf jest tablicą, nie ECS
pub struct EdgeId(pub u32);
pub struct LaneIdx(pub u8);

#[derive(Clone, Copy)]
pub enum RoadClass { Motorway, Arterial, Local, Residential, Dirt }

#[derive(Clone, Copy)]
pub enum Modality { Road, Foot, Bike, Rail, Tram }

/// Krawędź kierunkowa. Droga dwukierunkowa = dwie krawędzie ze wspólnym `geometry_ref`.
pub struct RoadEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub geometry_ref: PolylineId,     // z sim/world (M2)
    pub length_cm: u32,               // całkowite — wchodzi do wzorów pieniężnych
    pub lanes: u8,
    pub class: RoadClass,
    pub speed_limit_dkmh: u16,        // decykilometry/h — 500 = 50,0 km/h
    pub max_mass: Mass,               // tonaż; 0 = bez ograniczeń
    pub grade_permille: i16,          // nachylenie, wpływa na zużycie paliwa
    pub modalities: ModalityMask,     // które warstwy używają tej krawędzi
    pub bridge: Option<BridgeId>,     // przepustowość i remonty (M2/M8 zmieniają stan)
    pub curb_parking: u16,            // miejsc przyulicznych
    pub district: DistrictId,
}

/// Manewr skrętny w węźle: (wjazd, wyjazd). To on, nie węzeł, ma przepustowość.
pub struct TurnMovement {
    pub node: NodeId,
    pub in_edge: EdgeId,
    pub out_edge: EdgeId,
    pub banned: bool,
    pub priority: TurnPriority,       // Signal(phase) | Yield | Major | RoundaboutRing
    pub saturation_flow_vph: u16,     // pojazdów/h przy zielonym ciągłym
}

pub enum NodeControl {
    Uncontrolled,
    PrioritySigns { major: [EdgeId; 2] },
    Roundabout { ring_edges: Vec<EdgeId> },
    Signal(SignalPlanId),             // plan sygnalizacji z data/
}

pub struct RoadGraph {
    pub nodes: Vec<RoadNode>,
    pub edges: Vec<RoadEdge>,
    pub out_edges: CsrIndex,          // CSR: węzeł → zakres krawędzi wychodzących
    pub turns: Vec<TurnMovement>,
    pub turn_index: CsrIndex,         // (węzeł, wjazd) → zakres manewrów
    pub controls: Vec<NodeControl>,
    pub topology_version: u32,        // ++ przy zmianie struktury → wyzwala rekontrakcję
    pub weight_version: u32,          // ++ przy zmianie wag → wyzwala kustomizację
}
```

Grafy pieszy, rowerowy i szynowy to osobne instancje `RoadGraph` z własną `Modality`
i własnym indeksem — nie wspólny graf z maską. Uzasadnienie: graf pieszy ma inną gęstość
i inne algorytmy (A\* wystarcza, bo trasy są krótkie), a rozdzielenie eliminuje filtrowanie
krawędzi w pętli wewnętrznej. Przesiadki modalne to jawna tabela `TransferLink`.

```rust
// ---- Routing -----------------------------------------------------------

/// Customizable Contraction Hierarchies: kolejność kontrakcji liczona RAZ
/// (z zagnieżdżonej dysekcji na geometrii), wagi przeliczane tanio przy każdej zmianie.
pub struct ChGraph {
    pub order: Vec<NodeId>,           // kolejność kontrakcji — stabilna między kustomizacjami
    pub up: CsrIndex,                 // graf „w górę" hierarchii
    pub down: CsrIndex,
    pub weights: Vec<u32>,            // czas swobodny w setnych sekundy
    pub built_for_topology: u32,
    pub customized_for_weights: u32,
}

pub struct Route {
    pub mode: TravelMode,
    pub legs: SmallVec<[RouteLeg; 4]>,  // odcinki jednomodalne, rozdzielone przesiadkami
    pub planned_minutes: u16,           // suma z profilu godzinowego w chwili planowania
    pub planned_cost: Money,            // paliwo/bilet/parking wg profilu
}

pub struct RouteLeg {
    pub mode: TravelMode,
    pub edges: Vec<EdgeId>,             // dla legu transportowego: sekwencja przystanków
    pub entry_transfer: Option<TransferLink>,
}

pub trait Router {
    /// `None` = brak trasy spełniającej ograniczenia. Zwracane NA ETAPIE PLANOWANIA,
    /// nigdy jako porażka w trakcie przejazdu (kontrakt z M6).
    fn route(&self, req: &RouteQuery) -> Option<Route>;
}

pub struct RouteQuery {
    pub origin: NodeId,
    pub dest: NodeId,
    pub mode: TravelMode,
    pub depart_hour_bucket: u8,         // 0..24 — wybiera profil czasowy
    pub profile: RouteProfile,          // wybiera zestaw wag z ograniczeniami
    pub gross_mass: Mass,               // masa całkowita z ładunkiem — kontrola tonażu mostów
    pub vehicle_class: Option<VehicleClassId>,
}

/// Zestawy wag na TEJ SAMEJ kolejności kontrakcji CCH. Dodanie profilu kosztuje
/// jedną kustomizację (~150 ms), nie rekontrakcję — to jest powód wyboru CCH zamiast CH.
/// Zbiór jest MAŁY I ZAMKNIĘTY; regulacje M8 przełączają profil, nie tworzą nowego.
pub enum RouteProfile {
    Passenger,
    HeavyDay,       // ciężarowy w godzinach obowiązywania stref i zakazów
    HeavyNight,     // ciężarowy poza godzinami zakazów (okno dostaw nocnych)
}

/// Krawędź jest wyłączona z profilu ciężarowego, gdy: max_mass < gross_mass,
/// most w remoncie, albo krawędź leży w strefie zakazu ruchu ciężkiego aktywnej
/// w danym profilu. Wyłączenie = waga u32::MAX w zestawie wag tego profilu.

/// Cache tras stabilnych (§17.6). Klucz bez czasu odjazdu — trasa dom↔praca
/// zmienia się rzadko; unieważniana przez topology_version i przez replanning.
pub struct RouteCache {
    map: SeededMap<RouteKey, Arc<Route>>,   // deterministyczna kolejność iteracji
    lru: IntrusiveLru,
    pub topology_version: u32,
}

/// §17.6 — tabela czasów przejazdu dzielnica × godzina × środek.
/// Wejście dla: planowania mezo, filtrowania ofert pracy (§5.6), LOD makro (M12).
pub struct TravelTimeMatrix {
    pub minutes: Vec<u16>,              // [district_from][district_to][hour][mode], EWMA
    pub samples: Vec<u32>,
    pub n_districts: u16,
}

impl TravelTimeMatrix {
    /// Aktualizacja z obserwacji: EWMA na liczbach całkowitych, deterministyczna kolejność.
    pub fn observe(&mut self, from: DistrictId, to: DistrictId,
                   hour: u8, mode: TravelMode, minutes: u16);
}
```

**Przebudowa CCH.** Dwa poziomy reakcji na zmianę sieci:

| Zdarzenie | Reakcja | Koszt |
|---|---|---|
| Zmiana wagi (limit prędkości, remont mostu, zamknięcie pasa) | **kustomizacja** — ta sama kolejność kontrakcji, przeliczenie wag skrótów | ~80–150 ms dla 200 tys. węzłów, w pełni równoległa |
| Zmiana topologii (nowa droga, zburzona droga) | **rekontrakcja** — nowa kolejność + nowe skróty | ~3–8 s jednowątkowo, ~1 s na 8 wątkach |

Obie idą przez `engine/jobs` w tle, ale **punkt konsumpcji jest deterministyczny**: zadanie dostaje
budżet `W` jednostek pracy na tick ekonomiczny; liczba ticków do ukończenia jest funkcją
`total_work / W`, nie czasu zegarowego. Podmiana grafu (`Arc::swap`) następuje w punkcie
synchronizacji tego ticku. Domyślnie kustomizacja kończy się w ≤ 2 ticki, rekontrakcja w ≤ 30 ticków
(30 minut gry). W trakcie rekontrakcji zapytania idą starym `ChGraph`; zapytanie, którego trasa
dotyka krawędzi z listy zmian (`dirty_edges`), spada na dwukierunkowe A\* na `RoadGraph`
(1–3 ms — akceptowalne, bo takich zapytań jest kilkadziesiąt).

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

Sumarycznie faza jest **ciężka** — porównywalna z M2 lub M3 — a jej ciężar koncentruje się
w WP8 (mikro) i WP3 (mezo). Gdyby faza musiała zostać przycięta, jedyna bezpieczna redukcja to
**odłożenie części WP8** (zmiana pasa i ronda) do M11: warstwa mikro nie ma skutków ekonomicznych,
więc jej uproszczenie nie rusza żadnego kontraktu poza wizualnym. Odwrotna redukcja — przycięcie
WP3 lub WP9 — jest niedopuszczalna: to one niosą kontrakt §17.4.
