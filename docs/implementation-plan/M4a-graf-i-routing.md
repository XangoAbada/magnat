# M4a — Graf i routing

Podfaza 1 z 4 fazy **M4 — Ruch** (`M4-ruch.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2 (`RoadNetwork`, K-14), M0 (`jobs`). |
| **Pakiety robocze** | WP1, WP2 |
| **Projekt techniczny** | §5.1 |
| **Wynik do pokazania** | Devtools: inspektor grafu rysuje krawędzie z atrybutami; zapytanie CCH odpowiada w budżecie na grafie 200 tys. węzłów. |
| **Kryterium zamknięcia** | Kryteria WP1 i WP2; rekontrakcja kończy się w tym samym ticku w dwóch przebiegach tego samego seeda. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M4b-mezo-i-podroze.md` |

Crate `engine/nav`: `RoadGraph` i grafy modalne zbudowane z geometrii M2, CCH z kustomizacją i rekontrakcją w tle, A* landmarkowy, `RouteCache`, `TravelTimeMatrix`.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP1** | `RoadGraph` i grafy modalne | M2 (`sim/world` drogi) | Budowa grafu z geometrii dróg M2: węzły, krawędzie kierunkowe, pasy, klasa, limit, tonaż, gradient, most. Warstwy: `Road`, `Foot`, `Bike`, `Rail`. Połączenia międzymodalne (przystanek↔chodnik, parking↔chodnik). Walidacja: spójność, brak krawędzi wiszących, każda parcela ma dostęp pieszy. | Graf miasta 150 tys. buduje się < 400 ms; walidator zielony; inspektor grafu w devtools rysuje krawędzie z atrybutami |
| **WP2** | Routing: CCH + A\* + cache | WP1 | Kolejność kontrakcji z zagnieżdżonej dysekcji (raz), kustomizacja wag przy zmianie prędkości/przepustowości, pełna rekontrakcja przy zmianie topologii — obie w tle z **deterministycznym budżetem pracy na tick**. A\* z heurystyką landmarkową dla grafu pieszego/rowerowego. `RouteCache` (klucz: `(origin_node, dest_node, mode, profile_hour_bucket)`). `TravelTimeMatrix` dzielnica × godzina × środek, aktualizowana z obserwacji EWMA. | Zapytanie CCH ≤ 60 µs (p95) na grafie 200 tys. węzłów; trafialność cache ≥ 90 % w scenariuszu doby; rekontrakcja kończy się w **tym samym ticku** w dwóch przebiegach tego samego seeda |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

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
