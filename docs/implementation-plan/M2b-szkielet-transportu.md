# M2b — Szkielet transportu

Podfaza 2 z 5 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2a (indeksy), M1 (`TerrainQuery`, K-13). |
| **Pakiety robocze** | WP3, WP4, WP5, WP6 (WP5 bez kolei — patrz korekta B1) |
| **Projekt techniczny** | §5.2 |
| **Wynik do pokazania** | `headless preview --field roads` → PNG z bramami, arteriami, strukturami inżynierskimi i konturami kwartałów (tory: M2c, korekta B1). |
| **Kryterium zamknięcia** | Kryteria WP3–WP6; dodatkowo dwa przebiegi tego samego seeda dają bajt w bajt identyczny `RoadNetwork`. |
| **Poprzednia / następna** | `M2a-indeksy-przestrzenne.md` · `M2c-strefy-parcele-dzielnice.md` |

Etap 3 generacji: bramy miasta, arterie L-systemem ograniczonym terenem, mosty i tunele, kolej towarowa, a z powstałego grafu — kwartały.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| [x] WP3 | Punkty wejścia do miasta | M1 (`Terrain`) | `CityGate`, wybór typu i miejsca wg profilu i regionu | dla 20 seedów × 5 profili: każdy wymagany typ bramy istnieje, leży na terenie zgodnym z typem (port na wodzie żeglownej, lotnisko na terenie o nachyleniu < 3%) |
| [x] WP4 | L-system arterii | WP3 | reguły globalne + ograniczenia lokalne, snapowanie, klasy dróg | sieć bez wiszących końców klasy ≥ Collector; 2 przebiegi tego samego seeda → identyczny bajt w bajt `RoadNetwork` |
| [x] WP5 | Mosty, tunele, nasypy | WP4 | wybór struktury z `TerrainQuery::crossing_cost`, limity rozpiętości i przewyższenia per klasa | most ≤ `bridge_max[class]`, tunel ≤ `tunnel_max[class]` i tylko dla klas z `tunnel_trigger > 0`, nasyp ≤ 8 m; niweleta segmentu po gruncie ≤ `max_slope[class]` |
| [x] WP5b | Kolej towarowa | WP7 (M2c) | A* najtańszej ścieżki dla torów, łączenie odnóg, rozjazdy | każda strefa przemysłowa/logistyczna ma bocznicę ≤ 1,2 km od kwartału; brak toru o nachyleniu > 2% — **wykonane w M2c, korekta B1** |
| [x] WP6 | Kwartały z grafu dróg | WP5 | wyznaczanie ścian planarnych (faces) obchodem półkrawędzi | liczba orbit obchodu = `E − V + 2·C` (wzór Eulera); suma kwartałów + pas drogowy = suma ścian w tolerancji 0,5% — patrz korekta B17 |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.2 Etap 3 — szkielet transportu

#### Punkty wejścia

```rust
pub enum GateKind { Highway, RailFreight, RailPassenger, Port, Airport }
pub struct CityGate {
    pub kind: GateKind,
    pub pos: Vec2,                  // punkt na krawędzi mapy (Airport: wewnątrz)
    pub dir_inward: Vec2,
    pub capacity: Qty,              // przepustowość dobowa — używana od M6 (limit importu)
    pub node: NodeId,
}
```
Liczba i typy bram z profilu (`data/zoning/profile_*.ron`): każdy profil ma listę
`required: [GateKind]` i `optional: [(GateKind, prob)]`. Umiejscowienie: dla każdej krawędzi
mapy liczony jest koszt wprowadzenia drogi klasy `Highway` w głąb 800 m
(nachylenie, woda, przekroczenie rzeki) — wybierany argmin, z kwantyzacją do 200 m i
losowaniem `rng(seed, StreamId::Gates, gate_index, 0)` wśród 3 najlepszych, by uniknąć
stałego „zawsze południowy wschód". Port wymaga wody o głębokości ≥ 6 m i ciągłym
połączeniu do krawędzi mapy. Lotnisko: prostokąt 2600×600 m o nachyleniu < 3%,
≥ 4 km od centrum, poza pierścieniem gęstej zabudowy.

#### Klasy dróg

```rust
pub enum RoadClass { Highway, Arterial, Collector, Local, Service, Pedestrian, RailFreight, RailPassenger }
```

| Klasa | pasy | pas drogowy (ROW) | dł. segmentu | v [km/h] | max nachylenie | most: max rozpiętość | próg tunelu (przewyższenie) |
|---|---|---|---|---|---|---|---|
| Highway | 2×2 | 30 m | 400 m | 120 | 5% | 800 m | 20 m |
| Arterial | 2×2 | 24 m | 200 m | 60 | 7% | 300 m | 25 m |
| Collector | 1×2 | 16 m | 120 m | 50 | 9% | 120 m | 35 m |
| Local | 1×2 | 12 m | 60 m | 30 | 12% | 40 m | — |
| Service | 1×1 | 8 m | 40 m | 20 | 15% | 20 m | — |
| Pedestrian | — | 4 m | 30 m | — | 20% (schody) | 30 m | — |
| RailFreight | 1–2 tory | 20 m | 500 m | 80 | 2,0% | 400 m | 15 m |
| RailPassenger | 2 tory | 16 m | 500 m | 120 | 2,5% | 400 m | 15 m |

#### L-system arterii

Wariant parametryczny, sterowany kolejką priorytetową (schemat Parish–Müller), rozwijany
**jednowątkowo** — całość to ~3 s dla metropolii, równoległość nie jest warta ryzyka
determinizmu.

Stan: `Proposal { pos: Vec2, dir: Vec2, class: RoadClass, gen: u16, prio: u32, seq: u32 }`.
Kolejka: `BinaryHeap` po kluczu `(prio, seq)` — `seq` to monotoniczny licznik nadawany przy
wpychaniu, więc porządek jest **totalny**, bez remisów. To jest cały mechanizm determinizmu
L-systemu; RNG dla propozycji: `rng(world_seed, StreamId::RoadsL, seq, 0)`.

Aksjomat ω: dla każdej bramy drogowej — **wachlarz** pięciu propozycji w kierunku centrum
(odchylenia 0°, ±26°, ±52°), `class = Highway` (`Port`/`Airport`: `Arterial`), `gen = 0`,
`prio = 0`. Wachlarz, a nie jedna propozycja, bo próby ratunkowe z ograniczenia 2 obracają
ten sam kierunek i przy bramie nad odnogą wody wszystkie trafiały w tę samą taflę
(korekta B15b). Reguła minimalnego kąta w węźle początkowym przepuści tylko pierwsze ramię,
które się uda. Dodatkowo 1 propozycja obwodnicowa dla miast > 150 tys.

Trasa wylotowa po wejściu w obszar zurbanizowany **zmienia klasę na `Arterial`** (korekta B9)
— tak to działa w rzeczywistości i tylko tak aksjomat rodzi szkielet miasta.

Reguły produkcji (po zaakceptowaniu segmentu `s` o końcu `p'` i kierunku `d'`):

```
P1  KONTYNUACJA
    Highway|Arterial|Collector → Proposal(p', rotate(d', θ_c), ta_sama_klasa, gen+1, prio+1)
    θ_c wg wzorca globalnego (niżej). Stop gdy gen > max_gen[class] lub p' poza obszarem.

P2  ROZGAŁĘZIENIE BOCZNE   (prawdopodobieństwo p_branch[class], sprawdzane co segment)
    Highway  → Arterial   w ±90°,  prio += 12,  p_branch = 0.15  (zjazdy, tylko co 4. segment)
    Arterial → Collector  w ±90°,  prio += 6,   p_branch = 0.55
    Collector→ Local      w ±90°,  prio += 3,   p_branch = 0.70   (obie strony niezależnie)
    Local    → Service    w ±90°,  prio += 1,   p_branch = 0.20   (przybliżenie „kwartał
                                                                    > 2 ha": gen ≥ 2 i poza
                                                                    superblokiem — korekta B23)

P3  ROZWIDLENIE (Y)        tylko Arterial, p = 0.08, dwaj potomkowie w ±(25°..40°)

P4  DOMKNIĘCIE PIERŚCIENIA (tylko Arterial, gen ≥ 3): jeśli w promieniu 1,5·seg_len istnieje
    węzeł Arterial nienależący do przodków — propozycja łącznika prosto do niego, prio += 2.
    Cel: sieć arterii ma być grafem z cyklami, nie drzewem (wymóg Etapu 10: spójność
    i brak pojedynczych punktów odcięcia dzielnicy).

P0  ŁĄCZNIK DOMYKAJĄCY (korekta B10, uogólnienie P4 na każdą klasę): łańcuch, który się
    kończy — limit generacji, wyjście poza obszar, dojazd do środka — próbuje trafić
    w najbliższą istniejącą ulicę swojej klasy w promieniu 1,5·seg_len, zamiast zostać
    ślepym zaułkiem, który i tak wypadnie przy domykaniu sieci.
```

**Wzorce globalne** (`θ_c` i preferencja kierunku) — wybierane wg **względnej odległości
od środka miasta** z `data/districts/patterns.ron` (korekta B6: dzielnice i pierścienie epok
powstają dopiero w M2c, więc nie ma jeszcze czym ważyć; promień jest ich przybliżeniem,
bo miasto rosło koncentrycznie). Kąt siatki `θ` to jedna liczba na świat,
losowana z `rng(seed, StreamId::RoadsL, NO_ENTITY, 0)`:

| Wzorzec | Reguła kierunku | Typowo dla |
|---|---|---|
| `Radial` | `d'` = kierunek na/od centrum ± szum 8° | rdzeń staromiejski, promieniste wyloty |
| `Grid(θ)` | `d'` snapowane do najbliższej z {θ, θ+90°, θ+180°, θ+270°}, szum 3° | XIX-wieczne śródmieście, przedmieścia planowane |
| `Organic` | `d'` = `d` ± U(−25°, 25°), preferencja izolinii wysokości | średniowieczny rdzeń, wchłonięta wieś |
| `Contour` | `d'` minimalizuje nachylenie na odcinku (5 kandydatów co 12°) | teren górski, skarpy |
| `Superblock` | `Grid(θ)` z `seg_len ×= 3` dla Collector, brak `Local` wewnątrz | blokowisko lat 60.–80. |

**Ograniczenia lokalne** — stosowane w tej kolejności; pierwsze, które odrzuci, kończy próbę.
Numeracja **przenumerowana na 1–6** (pierwotna pomijała 3) i kolejność dwóch pierwszych
**zamieniona** — uzasadnienie w korektach B3 i B4.

1. **Przeszkoda i wybór struktury.** `TerrainQuery::crossing_cost(a, b)` zwraca kwalifikację
   przeszkody. M2 **nie liczy tego sam** (K-13) — decyduje tylko, czy daną klasę drogi stać
   na taką przeprawę. Warianty są takie, jakie **faktycznie zwraca M1** (korekta B2):
   - `Crossing::Bridge { span_m, clearance_m }` → `Bridge { clearance_dm }`,
     jeśli `span_m ≤ class.bridge_max`; inaczej odrzucenie.
   - `Crossing::Tunnel { len_m, rock }` → `Tunnel { cover_dm }`, jeśli klasa ma
     `tunnel_trigger > 0`, `len_m ≤ class.tunnel_max` i przewyższenie terenu nad cięciwą
     w połowie odcinka ≥ `tunnel_trigger`. Dla `Local`/`Service` tunele zabronione.
   - `Crossing::Embankment { .. }` → `Embankment { height_dm }`, gdy droga wznosi się ponad
     teren o co najmniej 1,5 m, a różnica (w obie strony: nasyp i wykop) nie przekracza 8 m;
     poniżej progu to zwykły `AtGrade` na robotach ziemnych, powyżej — odrzucenie.
   - `Crossing::Flat` → `AtGrade`.
2. **Niweleta.** Spadek między **rzędnymi końców** odcinka (`RoadNode::z_dm`) porównywany
   z `max_slope[class]`. Dotyczy wyłącznie przebiegu po gruncie (`AtGrade`, `Embankment`):
   most i tunel nie podążają za terenem. Przy przekroczeniu: rotacja o ±15° (próby w kolejności
   +15, −15, +30, −30), potem skrócenie do 50% długości, potem odrzucenie.
3. **Snapowanie węzła.** Jeśli `p'` leży w promieniu `0,25·seg_len` od istniejącego węzła
   o klasie ≥ `class` → scalenie (`p'` := ten węzeł, **kontynuacja** wygasa; rozgałęzienia
   nie — korekta B11).
4. **Przecięcie.** Sonda przedłużona o 25% długości, jeśli koniec nie trafił w węzeł
   (korekta B13). Trafienie klasyfikowane metrycznie: bliżej niż 1 m od węzła istniejącego
   segmentu → scalenie z tym węzłem; bliżej niż 1 m od początku kandydata → styk, ignorowane;
   w środku segmentu → podział i nowy węzeł. Jeśli którakolwiek ze stron jest mostem albo
   tunelem → bezkolizyjnie, bez węzła, obie krawędzie dostają `GRADE_SEPARATED` i wypadają
   z wyznaczania kwartałów. **Nasyp nie jest bezkolizyjny** — leży na gruncie.
5. **Minimalny kąt.** Odrzucenie, jeśli kąt do dowolnej krawędzi incydentnej wynosi < 30°
   (`Local`/`Service`: < 22°) — sprawdzane **w obu węzłach**, początkowym i docelowym
   (korekta B12). Porównanie przez iloczyn skalarny, bez `atan2` (00 §K-6).
6. **Strefa zakazana.** Odrzucenie poza mapą, w obrysie lotniska oraz **gdy węzeł miałby
   stanąć w wodzie**. Sam odcinek wolno poprowadzić nad wodą — od tego jest most — ale
   koniec musi wypaść na lądzie (korekta B14). Pas drogowy innej drogi i złoża `Extraction`
   pilnuje M2c, bo złoża wchodzą do strefowania (Etap 4).

Warunek stopu: pusta kolejka albo osiągnięty budżet segmentów wyliczony z `target_pop`
(sekcja 7, budżety). Budżet jest twardy — zabezpiecza przed rozbieganiem się generatora.

#### Kolej towarowa — **przeniesiona do M2c** (korekta B1)

Trasy torów prowadzą do klastrów stref `IndustryHeavy` / `Logistics`, a strefowanie
to Etap 4, czyli WP7 z `M2c-strefy-parcele-dzielnice.md`. W M2b nie ma czego łączyć:
opis zostaje tutaj jako projekt techniczny, wykonanie należy do WP5b w M2c. M2b stawia
za to bramę `RailFreight` (pozycja i przepustowość) i wypisuje ją w `GenerationReport`
jako bramę odłożoną — węzła sieci nie zakłada, bo byłby wierzchołkiem bez krawędzi.

Nie L-systemem. Dla każdego klastra stref `IndustryHeavy` / `Logistics` liczona jest
najtańsza ścieżka A* na siatce 32 m od bramy `RailFreight`, z kosztem
`base + 40·nachylenie% + 900·(woda) + 25·(przecięcie drogi klasy ≥ Collector)`
i karą nieskończoną za nachylenie > 2%. Ścieżki są następnie **scalane**:
wspólne prefiksy (odległość < 40 m na długości > 300 m) łączone w jeden tor z rozjazdem
w punkcie rozejścia. Kolejność przetwarzania klastrów: malejąca powierzchnia, remisy po
`BlockId` — deterministycznie.

```rust
pub struct RoadSegment {
    pub a: NodeId, pub b: NodeId,
    pub class: RoadClass,
    pub geom: PolyRef,            // OŚ drogi (centrolinia) — indeks do wspólnej areny punktów
    pub structure: RoadStructure, // AtGrade | Bridge { clearance_dm } | Tunnel { cover_dm }
                                  // | Embankment { height_dm }
    pub lanes_fwd: u8, pub lanes_bwd: u8,
    pub row_m: u8,                // szerokość pasa drogowego
    pub speed_kph: u8,
    pub max_tonnage_t: u8,        // dopuszczalny tonaż; 0 = bez ograniczenia
    pub length_dm: u32,
    pub district: DistrictId,
    pub flags: RoadFlags,         // ONEWAY | SIDEWALK | NO_HEAVY | TRAM_READY | RAIL | GRADE_SEPARATED
}
pub struct RoadNetwork {
    pub nodes: Vec<RoadNode>,     // pos, stopień, flagi (gate, junction, dead_end)
    pub segments: Vec<RoadSegment>,
    pub adj_start: Vec<u32>,      // CSR: sąsiedztwo węzeł → segmenty
    pub adj_items: Vec<SegmentId>,
    pub geom: PolyArena,
    pub gates: Vec<CityGate>,
    pub furniture: Vec<StreetFurniture>,   // latarnie, ławki, szpalery — wejście dla M11
}
pub struct StreetFurniture { pub seg: SegmentId, pub t: f32, pub pos: Vec3, pub kind: FurnitureKind }
pub enum FurnitureKind { StreetLamp, Bench, TreeRow, BusStopPad }
```

---

## Zmiany wpisane po M2b

Zgodnie z `CLAUDE.md`: rozjazd kodu z planem jest gorszy niż błąd w planie, bo nikt go nie widzi.
Poprawki z gwiazdką zmieniły **zakres** albo **kryterium**, reszta doprecyzowuje projekt.

| # | Co | Dlaczego |
|---|---|---|
| B1 ★ | **Kolej towarowa przeniesiona do M2c** jako WP5b. WP5 w M2b to mosty, tunele i nasypy | Trasy torów prowadzą do klastrów stref `IndustryHeavy`/`Logistics`, a strefy powstają w Etapie 4 (WP7, M2c). Kryterium WP5 („bocznica ≤ 1,2 km od kwartału") nie da się w M2b nawet sprawdzić — nie ma stref. Ścieżka krytyczna fazy (`WP4 → WP6 → WP7`) już to zakładała; tabela WP tego nie odzwierciedlała |
| B2 | `Crossing` ma warianty z M1, nie z planu: `Flat`, `Embankment{fill_m3}`, `Bridge{span_m, clearance_m}`, `Tunnel{len_m, rock}` | **K-13**: kontrakt `TerrainQuery` opublikowany przez M1 jest jedynym źródłem prawdy. Plan wypisywał `Water{span, bank_delta}`, `Ridge{len, rise}`, `Depression{len, drop}` — takich wariantów nie ma i nie M2 je definiuje |
| B3 ★ | Ograniczenie nachylenia mierzy **niweletę** (spadek między końcami odcinka), nie `slope_at` próbkowane co 8 m | To dwie różne wielkości: `slope_at` zwraca moduł gradientu **terenu**, więc droga biegnąca poziomo po zboczu ma niweletę 0%, a `slope_at` 20%. Przy kryterium z planu autostrada (limit 5%) była odrzucana już na pierwszym segmencie w **każdym** regionie — zmierzone 100% odrzuceń i sieć o zerowej długości. Za profil pośredni odpowiada ograniczenie 1, które ma na to gotową odpowiedź w `crossing_cost` |
| B4 | Kolejność ograniczeń: struktura **przed** niweletą; numeracja 1–6 zamiast 1, 2, 4, 5, 6, 7 | Most i tunel z definicji nie podążają za terenem, więc limit niwelety stosuje się tylko do przebiegu po gruncie. Przy kolejności z planu żadna przeprawa przez dolinę nie miałaby szansy powstać — odrzucałoby ją zbocze, którego most nie dotyka. Numeracja w planie pomijała 3 |
| B5 | `ClassSpec` dostaje `max_gen`, `max_tonnage_t`, `lamp_spacing_m` | Tabela klas ich nie ma, a wszystkie trzy są potrzebne: pierwsze do warunku stopu P1, drugie jest polem `RoadSegment` z planu, trzecie — wymaganiem M11 na `StreetFurniture`. `max_gen` jest **bezpiecznikiem, nie regulatorem gęstości**: o rozmiarze miasta decydują promień obszaru zurbanizowanego i budżet segmentów |
| B6 | Wzorzec globalny wybierany po względnym promieniu z `data/districts/patterns.ron`, nie po dzielnicy i pierścieniu epoki | Dzielnice (§5.5) i pierścienie epok (§5.3) powstają w M2c, czyli **po** drogach. Promień jest ich przybliżeniem i to nie jest proteza: miasto rosło koncentrycznie, więc odległość od środka **jest** wiekiem zabudowy. M2c wyprowadzi z tego pierścienie, nie odwrotnie |
| B7 | `CityPlan`, `target_pop`, `GenerationReport` zdefiniowane w tej podfazie | Plan wymienia je w §6 fazy („dostarczam"), ale nie mówi, która podfaza je tworzy. M2b jest pierwszym konsumentem. `target_pop` wynika z rozmiaru mapy (4 km → 40 tys., 8 km → 120 tys., 12 km → 250 tys., 16 km → 400 tys.) i jest jedyną wielkością, z której liczone są budżety generatora |
| B8 | **Wybór środka miasta** — algorytm dopisany (luka w planie) | Aksjomat L-systemu celuje „w kierunku centrum", a plan nigdzie nie mówi, skąd to centrum wziąć. Wybieramy jak ludzie: płaski i suchy grunt w środkowej części mapy, z premią za bliskość żeglownej wody. Bez RNG — rozstrzyga teren, remisy kolejność komórek |
| B9 ★ | Trasa wylotowa zmienia klasę na `Arterial` po wejściu w obszar zurbanizowany | Przy regule P2 (`Highway → Arterial`, `p = 0,15`, co czwarty segment) dwie autostrady dają **pół arterii na miasto**. Zmierzone: 49 segmentów, z których po przycięciu wiszących końców nie zostawał żaden. Droga krajowa stająca się aleją to zarazem to, co robi rzeczywistość |
| B10 | Reguła **P0** — łącznik domykający, uogólnienie P4 na każdą klasę | Cel P4 („graf z cyklami, nie drzewo") dotyczy całej sieci, nie tylko arterii. Bez P0 35% powstałych segmentów wypadało przy domykaniu jako ślepe zaułki |
| B11 | Scalenie z istniejącym węzłem wygasza **kontynuację**, ale nie rozgałęzienia | Skrzyżowanie jest dokładnie tym miejscem, w którym odchodzą ulice niższej klasy. Przy gaszeniu całej produkcji wzrost kończył się na 1209 segmentach przy budżecie 16 000 |
| B12 ★ | Minimalny kąt sprawdzany **także w węźle początkowym** | Dwie propozycje wychodzące z tego samego węzła w niemal tym samym kierunku dawały dwie **nakładające się** krawędzie. Geometrycznie się nie przecinają (wyznacznik zeruje się na współliniowości), więc test planarności ich nie widzi, ale porządek kątowy przestaje odpowiadać rysunkowi i obchód półkrawędzi scala sąsiednie ściany: 154 orbity wobec 164 ze wzoru Eulera |
| B13 | Przecięcie: sonda przedłużona o 25% i **metryczna** klasyfikacja trafienia (styk / węzeł / podział) zamiast progu ułamkowego | (a) Ulica kończąca się 20 m przed równoległą ma się do niej podłączyć — bez tego sieć zostaje drzewem (161 ścian na 923 segmenty). (b) Przy progu ułamkowym (1% długości) przecięcie 2 m od końca czterystumetrowej autostrady nie dostawało węzła i po cichu łamało planarność |
| B14 ★ | Ograniczenie 6 doprecyzowane: **węzeł** nie ma prawa stanąć w wodzie (odcinek nad wodą — wolno) | Bez tego miasto portowe rozrastało się mostami **w morze**: każdy 200-metrowy odcinek arterii mieścił się w limicie rozpiętości 300 m, więc L-system budował po tafli w nieskończoność. 224 mosty na mieście 40-tysięcznym |
| B15 | Port stoi na żeglownej wodzie **przy mieście**, nie na krawędzi mapy; węzeł drogowy ląduje na nabrzeżu | Plan pisze w komentarzu „punkt na krawędzi mapy", a warunek stawia inny: „woda o głębokości ≥ 6 m i **ciągłe połączenie** do krawędzi mapy". Drugie jest właściwe. Ciągłość wynika z klasy wody: `Sea` dochodzi do krawędzi z definicji, `River` w modelu M1 zawsze uchodzi; `Canal` (jezioro) odpada, bo bywa bezodpływowe |
| B15b | Aksjomat ω to **wachlarz** pięciu propozycji (0°, ±26°, ±52°), nie jedna | Próby ratunkowe z ograniczenia 2 obracają ten sam kierunek o ±15°/±30°; przy bramie stojącej nad odnogą wody wszystkie trafiały w tę samą taflę i brama zostawała węzłem bez krawędzi, po czym wypadała przy domykaniu sieci |
| B16 | `required` w profilu niesie **krotność**: `[Highway, Highway]` to dwa wjazdy | Miasto z jednym wjazdem nie ma jak domknąć sieci arterii w cykl — dwie autostrady z różnych krawędzi spotykają się przy środku i to one tworzą pierwszy pierścień |
| B17 ★ | Kryterium WP6 zastąpione **niezmiennikiem Eulera**: liczba orbit obchodu = `E − V + 2·C` | Sformułowanie z planu („suma kwartałów + pas drogowy = obszar zurbanizowany, tolerancja 0,5%") jest spełnione **tożsamościowo**, bo pas drogowy liczymy jako różnicę ściany i kwartału — test nie mógłby nigdy upaść. Liczba orbit jest wielkością czysto kombinatoryczną i wyłapuje każdy błąd obchodu. Bilans pól zostaje jako drugi, słabszy warunek |
| B18 | Przycinane są **wszystkie** wiszące końce, nie tylko klasy ≥ Collector | Wisząca krawędź wewnątrz ściany robi w niej szczelinę o zerowej szerokości, której odsunięcie kwartału nie ma jak obsłużyć. Sięgacze (`cul-de-sac`) wracają w M2c, gdzie powstają świadomie przy podziale kwartału. Bramy chroni flaga `gate` na łańcuchu, który rośnie aż do miasta |
| B19 | Ściany < 300 m² są **odrzucane**, nie scalane z sąsiadem | Plan każe scalać, co wymaga sumowania wielokątów. 300 m² między jezdniami to wysepka w skrzyżowaniu, nie kwartał; jej pole trafia do pasa drogowego i bilans się zgadza |
| B20 | `RoadNode` niesie `z_dm` — **rzędną niwelety** | Droga jest w przekroju podłużnym cięciwą między swoimi końcami, a różnicę wobec terenu pokrywa nasyp albo wykop. Bez tego pola węzeł powstały z **podziału** segmentu dostawał rzędną terenu i obie połówki miały nagle inne nachylenie niż odcinek, z którego powstały — limit klasy przestawał obowiązywać w miejscu, w którym nikt nic nie budował. Pole wchodzi do kontraktu dla M4 i M11 |
| B21 | CLI: `headless preview --field roads`, nie `generate --preview roads` | Podgląd pól generatora ma już swoje podpolecenie z M1 wraz z rusztowaniem (parsowanie parametrów, podpróbkowanie, zapis PNG). Dokładanie drugiej ścieżki do `generate` byłoby duplikatem |
| B22 | Testy podfazy oznaczone `#[ignore]`, CI uruchamia je jawnie w `release` | Konwencja z `budgets.rs` i `determinism.rs`: każdy z tych testów generuje świat (~2 s w `debug`). Krok dopisany do joba `determinism` w `ci.yml` |
| B23 | `Local → Service` dopuszczone od `gen ≥ 2` i poza superblokiem | Reguła P2 warunkuje to kwartałem > 2 ha, a kwartały zna dopiero M2c (WP8). Bez tego wariantu równoległe ulice lokalne nigdy się nie przecinają i miasto wychodzi **grzebieniem** — widać to na podglądzie natychmiast. Docelowo wariant należy przenieść do WP8 |

---

## Wynik pomiarów

`cargo run --release -p magnat-headless -- preview --field roads`, profil `release`,
region rzeczny, `seed 0xC0FFEE`.

| Miara | Miasto małe (4 km, 40 tys.) | Metropolia (16 km, 400 tys.) | Budżet §7 fazy |
|---|---|---|---|
| Etap 3 łącznie | 20 ms | **217 ms** | ≤ 4 s (L-system + kolej) |
| — bramy | 3 ms | 37 ms | — |
| — L-system | 15 ms | 170 ms | — |
| — domknięcie sieci | 0,5 ms | 8 ms | — |
| — kwartały | 0,2 ms | 1 ms | ≤ 6 s (razem z siecią lokalną i parcelami, M2c) |
| segmenty | 931 | 9 744 | ~16 000 |
| długość dróg | 56 km | 517 km | ~1 200 km |
| mosty / tunele / nasypy | 35 / 0 / 70 | 378 / 0 / 361 | 40–120 mostów, 0–15 tuneli |
| kwartały | 110 | 1 148 | ~6 000 **po** podziale w WP8 |
| obszar zurbanizowany | 2,4 km² | 35,9 km² | ~70 km² |
| odrzucenia propozycji | 25% | 12% | ostrzeżenie > 40% (R1) |

**Czego te liczby nie domykają.** Metropolia wychodzi około 60% planowanej gęstości:
9,7 tys. segmentów wobec 16 tys. i 36 km² obszaru zurbanizowanego wobec 70 km². Różnica
jest oczekiwana i nie jest do nadrobienia w tej podfazie: kwartał ma tu średnio 3 ha, czyli
znacznie powyżej `max_block_area` większości stref (R3 1,2 ha, Commercial 3 ha), więc
**WP8 z M2c podzieli go siecią lokalną** i dopiero to zbliży obie liczby do planu.
Kalibracja gęstości jest sprawdzalna dopiero testami T9/T10 z §7 fazy, czyli w M2e —
i tam należy ją domknąć, a nie tutaj na wyczucie.

Tuneli jest zero we wszystkich zmierzonych światach: `crossing_cost` kwalifikuje tunel
dopiero przy deniwelacji przekraczającej połowę długości odcinka **i** 30 m, a regiony inne
niż górski takich zboczy nie mają. To zachowanie M1, nie M2 — odnotowane, bo budżet §7 fazy
przewiduje „0–15 tuneli (zależnie od regionu)" i dolna granica jest tu osiągana.

---

## Czego ta podfaza nie zostawia następnej

- **Kolej towarowa** (WP5b) — czeka na strefy z WP7.
- **Ulice lokalne wewnątrz kwartałów** — WP8 dzieli kwartały większe niż `max_block_area`,
  i to ten krok zbliży gęstość sieci do budżetu z §7.
- **`RoadSegment::district`** ma wartość `UNASSIGNED_DISTRICT` (`DistrictId(u16::MAX)`);
  wypełnia ją WP9.
- **Sięgacze (`cul-de-sac`)** — w M2b przycinane jako wiszące końce, w M2c powstają
  świadomie przy podziale kwartału (tylko R1/R2 i tylko poza pierścieniem staromiejskim).
