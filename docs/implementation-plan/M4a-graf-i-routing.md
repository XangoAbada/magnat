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
| **WP2** | Routing: CCH + A\* + cache | WP1 | Kolejność kontrakcji z zagnieżdżonej dysekcji (raz), kustomizacja wag przy zmianie prędkości/przepustowości, pełna rekontrakcja przy zmianie topologii — obie w tle z **deterministycznym budżetem pracy na tick**. A\* z heurystyką landmarkową dla grafu pieszego/rowerowego. `RouteCache` (klucz: `(origin_node, dest_node, mode, profile_hour_bucket)`). `TravelTimeMatrix` dzielnica × godzina × środek, aktualizowana z obserwacji EWMA. | Zapytanie CCH ≤ 60 µs (p95) na grafie 200 tys. węzłów **sieci drogowej** (`J-11`); trafialność cache ≥ 90 % przy pojemności ≥ 2× liczba odrębnych par, w horyzoncie wielodobowym (`J-12`); rekontrakcja kończy się w **tym samym ticku** w dwóch przebiegach tego samego seeda |

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

---

## Korekty planu wpisane po implementacji M4a

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Korekta | Dlaczego |
|---|---|---|
| J-1 ★ | **`RoadClass` z §5.1 nie istnieje w tym kształcie.** Plan wypisywał `{ Motorway, Arterial, Local, Residential, Dirt }`; obowiązująca klasa drogi to ośmiowariantowy enum M2 (`Highway, Arterial, Collector, Local, Service, Pedestrian, RailFreight, RailPassenger`), przeniesiony do `engine/core` wpisem **`K-23`** | Enum powstał w dokumencie M4 zanim M2 zaprojektował sieć, i nigdy nie został z nią uzgodniony. Dwa słowniki na jedno pojęcie to dokładnie to, przed czym broni `K-8`, a tu dodatkowo kolejność wariantów jest kontraktem, bo indeksuje tablicę `SPECS` generatora. Kierunek przenosin jest wymuszony grafem zależności, nie gustem — uzasadnienie w `K-23` |
| J-2 ★ | **`TravelMode` nie powstaje. Jest `magnat_core::TransportMode`** (`Walk, Bicycle, Car, Bus, Tram, Rail, Freight`), w `core` od M0 | Ta sama reguła `K-8`, tylko że tym razem słownik już był — plan M4a wymyślał drugi. Dotyczy `Route.mode`, `RouteLeg.mode`, `RouteQuery.mode` i klucza `RouteCache`. `TravelMode::Taxi` z M4 §9/D5 wchodzi jako wariant `TransportMode`, gdy D5 zostanie rozstrzygnięte |
| J-3 | **`PolylineId` nazywa się `PolyRef`**, a `RoadEdge` trzyma go jako nieprzezroczysty `GeomRef(u32)`, nie jako typ z `sim/world` | Nazwa: pomyłka planu. Nieprzezroczystość: `engine/nav` nie może zależeć od `sim/world` (`K-23`), a do narysowania krawędzi i tak potrzeba areny M2, czyli tej samej strony, która uchwyt nadała |
| J-4 | **`CsrIndex` nie istniał — powstaje `nav::graph::Csr`** (klucz → zakres w płaskiej tablicy), osobny od `magnat_spatial::CsrGrid` | `CsrGrid` jest indeksem **przestrzennym** (komórka siatki → obiekty), a nie sąsiedztwem grafu. Wspólna byłaby tylko para `starts`/`items` — za mało na abstrakcję (`YAGNI` przed `DRY`) |
| J-5 ★ | **Adapter „geometria M2 → `RoadGraph`" mieszka w `sim/world::nav_build`, nie w `engine/nav`.** `engine/nav` wystawia ogólnego `RoadGraphBuilder` i nie zna `RoadNetwork` | Wykonanie `Z-3`. Zależność `magnat-nav → magnat-world` zamknęłaby cykl przez `sim/agents` (`Z-1`). Przy okazji `engine/nav` da się testować i benchmarkować bez generatora miasta — patrz J-9 |
| J-6 ★ | **Warstw jest cztery: `Road`, `Foot`, `Bike`, `Rail`. `Tram` odpada** | M2 nie generuje torowiska tramwajowego — jest wyłącznie flaga `RoadFlags::TRAM_READY`. Warstwa bez ani jednej krawędzi to kod, który nie ma jak być nieprawdziwy. Dokłada ją faza, która postawi pierwszy tor |
| J-7 ★ | **`NodeControl` nie dostaje wariantu `Roundabout`** | M2 nie generuje rond. Wariant z `Vec<EdgeId>` odebrałby `NodeControl` `Copy` i dołożył ~20 B na węzeł w grafie, w którym rond nie ma. Należy do WP8 (M4d) razem z modelem pierwszeństwa na pierścieniu |
| J-8 | **Routing jest węzłowy: manewry skrętne są w grafie, ale nie wchodzą do wag CCH.** Tabela manewrów niesie zakaz zawracania i przepustowość nasycenia — konsumentem jest model węzła w M4b i mikro w M4d | Routing z kosztem skrętu wymaga grafu rozwiniętego po krawędziach (rząd wielkości więcej węzłów i skrótów). Nie kupiłby dziś nic: **M2 nie dostarcza ani jednego zakazu skrętu**, a zawracanie i tak nie wchodzi do trasy najkrótszej. Ścieżka wyjścia jest znana i zapisana przy kodzie: graf rozwinięty po krawędziach, gdy M8 wprowadzi zakazy skrętu jako politykę. Kryterium `route_optimality` jest tym nietknięte — referencyjna Dijkstra jest węzłowa tak samo |
| J-9 | **`nav::graph::synthetic_grid` jako graf testowy i benchmarkowy** | Kryterium WP2 mówi o 200 tys. węzłów, a metropolia M2 ma ~16 tys. odcinków — bez siatki syntetycznej budżetu nie da się w ogóle zmierzyć. To jest też realizacja mitygacji **R8** z M4 §8 („generator syntetycznej siatki do testów") |
| J-10 | **`curb_parking` nie ma źródła w M2** — `nav_build` wylicza je z klasy i długości (6 m na miejsce postojowe wzdłuż jezdni) | Pole jest w kontrakcie §5.1 i konsumują je WP7 oraz M6 (przelewanie się placu na ulicę), ale M2 nie zapisuje krawężnika. Oszacowanie jest oznaczone `ponytail:` z nazwaną ścieżką wyjścia: realne dane krawężnika, gdy WP7 ich potrzebuje |
| J-11 ★ | **Kryterium „zapytanie CCH ≤ 60 µs (p95) na grafie 200 tys. węzłów" rozpada się na dwie liczby, bo jedna instancja nie umie ich obu unieść.** Bramką jest **metropolia M2** (16 km, 12 145 węzłów drogowych): zmierzone **p95 32,8 µs** z rozpakowaniem trasy i **34,2 µs** dla gołego zapytania, pamięć **13,7 MB** — obie z dwukrotnym zapasem. Instancją naprężeniową jest `synthetic_road_network(100, 100, 10)` (188 200 węzłów): **p95 86,8 µs**, pamięć 71,6 MB — **poza budżetem i tak zapisane** | Trzy rzeczy naraz, wszystkie zmierzone. **(a)** Siatka jednorodna `450 × 450` jest najgorszym możliwym przypadkiem dla dysekcji zagnieżdżonej: jej szerokość drzewowa wynosi 450, więc separator na szczycie jest kliką o 450 wierzchołkach. Zmierzone: 8,6 mln łuków, **265 MB**, p95 **1237 µs**. To nie jest wada implementacji (sprawdzone `LEAF ∈ {8,16,32,64}`, optimum przy 32) — prawdziwa sieć drogowa ma tę samą liczbę węzłów przy separatorze o rząd wielkości mniejszym, bo **większość jej węzłów to punkty wzdłuż ulicy, a nie skrzyżowania**: łańcuch węzłów stopnia 2 kontrahuje się bez wypełnienia, a siatka ma stopień 4 wszędzie. **(b)** Nawet instancja drogowa o 188 tys. węzłów nie mieści się w 30 MB. Pytanie brzmi więc: czy budżet dotyczy grafu, który w grze wystąpi, czy grafu piętnaście razy większego. Miasto M2 ma **12 145 węzłów drogowych** i żadna faza nie zapowiada większego — `WorldSize::Metropolis16km` jest największym rozmiarem mapy. Budżet §7.3 dotyczy metropolii i tam jest trafiony. **(c)** Instancja naprężeniowa zostaje w benchmarkach mimo przekroczenia, bo mierzy **skalowanie**, a nie zgodność: gdyby M12 chciał mapy 50 km, to ta liczba mówi, co się wtedy stanie. Siatka jednorodna zostaje w testach **poprawności** (`route_optimality`), gdzie jest akurat dobra, bo jest trudna |
| J-12 ★ | **Kryterium „trafialność cache ≥ 90 % w scenariuszu doby" jest arytmetycznie nieosiągalne i zmienia się na „≥ 90 % przy pojemności ≥ 2× liczba odrębnych par, w horyzoncie wielodobowym".** Zmierzone: **918 ‰** (5 000 par × 2 przejazdy/dobę × 10 dób, pojemność 16 384) wobec **763 ‰** przy pojemności 8 192 | Dwa niezależne powody, oba zmierzone. Po pierwsze **zimny start**: 5 000 chybień obowiązkowych na 10 000 zapytań pierwszej doby daje sufit 50 %, a na trzech dobach 83,3 % — niezależnie od jakości cache'u. Po drugie **tłuczenie kubełków**: 5 000 kluczy w 2 048 kubełkach po 4 drogi kolidują realnie. Cache tras dom↔praca ma sens dopiero jako struktura żyjąca wiele dób — i tak go używa gra. Pojemność jest wobec tego **wymaganiem wdrożeniowym, nie parametrem do strojenia**: M4b, który zna liczbę mieszkańców, musi ją z niej wyliczyć (klucz niesie też kubełek godzinowy, więc par jest ~2× liczba dojeżdżających) |
| J-13 | **`Router::route` bierze `&mut self` i zwraca `Option<Arc<Route>>`**, wbrew `fn route(&self, req) -> Option<Route>` z §5.1. Znika też `RouteQuery.vehicle_class` | `&mut` jest wymuszone dwukrotnie: bufory wyszukiwania są wielokrotnego użytku (bez tego każde zapytanie zerowałoby tablicę wielkości grafu i budżet z §7.3 by przepadł), a cache z definicji zmienia się przy trafieniu. `Mutex` na tej ścieżce zjadłby dokładnie ten budżet, którego broni. API równoległe (sesja per wątek) projektuje M4b, kiedy będzie miał konsumenta — dziś nikt nie woła routera z ośmiu wątków. `Arc` bierze się stąd, że trafienie w cache nie ma po co kopiować wektora krawędzi. `vehicle_class` to `VehicleClassId`, typ `sim/traffic` (M4 §6), którego `engine/nav` nie może znać; dokłada go M4b razem z pojazdem jako encją |
| J-14 | **`RouteQuery.gross_mass` nie wchodzi do zestawu wag profilu, tylko do sprawdzenia zwróconej trasy.** Profil ciężarowy ma **masę odniesienia 24 t** (ten sam próg, którym M2 nadaje `NO_HEAVY`); zapytanie o cięższy ładunek, którego trasa profilu nie unosi, spada na Dijkstrę z predykatem masy | Inaczej każda masa całkowita byłaby osobną kustomizacją i liczba profili eksplodowałaby — dokładnie to, przed czym broni `D11`. Kontrakt z M6 zostaje nienaruszony: niewykonalność nadal objawia się jako `route() == None` **przy planowaniu**, nigdy w trakcie przejazdu. Fallback jest tą samą ścieżką, którą `D11` przewiduje dla rzadkiego, nietypowego ograniczenia |
| J-15 | **Przebudowa jest budżetowana co do ticku podmiany, ale liczona jednorazowo w ticku podmiany**, a nie rozłożona na kawałki | Kontrakt, który ten mechanizm ma nieść, to **numer ticku** — bo to on wpływa na czas dojazdu, a przez niego na spóźnienie i na saldo. Ten jest spełniony co do ticku i przetestowany (`rekontrakcja_konczy_sie_w_tym_samym_ticku_w_dwoch_przebiegach`). Ceną jest zacięcie w tym jednym ticku. Ścieżka wyjścia jest wąska i zapisana przy kodzie: `ChGraph::customize_range(od_rangi, do_rangi)` i wznawialna kontrakcja. Robi się to wtedy, gdy M8 zacznie przebudowywać sieć często — **dziś nie zmienia jej nikt**, bo M2 stawia drogi raz i ich nie rusza |
| J-16 | **`D8` (zdarzenie `RoadNetworkChanged { dirty_edges }` z M2) nie jest realizowane w M4a.** Zamiast niego `RebuildQueue::detect` porównuje `topology_version`/`weight_version` grafu z ostatnio widzianymi | M2 nie zmienia sieci w trakcie gry — inwestycje drogowe to M8. Zdarzenie bez nadawcy byłoby kodem, który nie ma jak być nieprawdziwy. Faza, która pierwsza ruszy sieć w locie, podmienia `detect` na zdarzenie z listą brudnych krawędzi i nie dotyka reszty modułu; rozróżnienie „zmiana wagi" vs „zmiana topologii", od którego zależy wybór taniej i drogiej ścieżki, jest już zaimplementowane |
| J-17 ★ | **Zapytanie idzie drzewem eliminacji, nie dwukierunkową Dijkstrą.** Wersja z kopcem zostaje jako `query_dijkstra` — wyłącznie do testu różnicowego | Pierwsza implementacja chybiała budżet o 9 % (p95 68,3 µs wobec 60 µs na metropolii) i nie dało się tego odkręcić strojeniem. Przyczyna była strukturalna: graf po kontrakcji jest **cordalny**, więc zbiór węzłów osiągalnych w górę z dowolnego węzła to dokładnie ścieżka jego przodków w drzewie eliminacji — kopiec nie ma czego porządkować, bo porządek jest już w strukturze. Zmiana kosztuje **4 bajty na węzeł** (tablica `parent`, liczona w trakcie kontrakcji za darmo, bo lista w górę jest tam jeszcze posortowana po randze) i daje: metropolia p95 **68,3 → 31,2 µs**, instancja 188 tys. **133,5 → 86,8 µs**, benchmark osobowy **76,0 → 43,1 µs**, ciężarowy **79,6 → 37,8 µs**. Wniosek na przyszłość, nie tylko dla M4: budżet, który chybia o kilka procent, zwykle mówi, że algorytm jest nie ten, a nie że stała jest za duża |
| J-18 | **Walidator liczy też największą składową *silnie* spójną**, a nie tylko słabą. Zmierzone na metropolii: warstwa drogowa ma **11 603** węzłów silnie spójnych wobec 12 031 słabo — **428 węzłów (3,5 %) to pułapki jednokierunkowe**, z których się nie wraca. Na pieszej i rowerowej obie liczby są równe | Kryterium WP1 mówi o spójności, ale spójność słaba nie odpowiada na pytanie, które zadaje gra: *czy da się wrócić do domu*. Różnicę widać w pomiarze — **113 z 4 000 losowych par (2,8 %) nie ma trasy** i nie jest to błąd routera. To znalezisko o sieci M2, nie o `engine/nav`: router może tylko zwrócić `None`. Konsekwencja dla M4b jest realna i wpisana w przód do dokumentu tej podfazy: mieszkaniec, którego dom albo praca leży poza składową silną, musi mieć rozstrzygnięcie na poziomie wyboru środka transportu, a nie porażkę przejazdu |
