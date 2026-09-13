# M2b — Szkielet transportu

Podfaza 2 z 5 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2a (indeksy), M1 (`TerrainQuery`, K-13). |
| **Pakiety robocze** | WP3, WP4, WP5, WP6 |
| **Projekt techniczny** | §5.2 |
| **Wynik do pokazania** | `headless generate --preview roads` → PNG z bramami, arteriami, strukturami inżynierskimi, torami i konturami kwartałów. |
| **Kryterium zamknięcia** | Kryteria WP3–WP6; dodatkowo dwa przebiegi tego samego seeda dają bajt w bajt identyczny `RoadNetwork`. |
| **Poprzednia / następna** | `M2a-indeksy-przestrzenne.md` · `M2c-strefy-parcele-dzielnice.md` |

Etap 3 generacji: bramy miasta, arterie L-systemem ograniczonym terenem, mosty i tunele, kolej towarowa, a z powstałego grafu — kwartały.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| WP3 | Punkty wejścia do miasta | M1 (`Terrain`) | `CityGate`, wybór typu i miejsca wg profilu i regionu | dla 20 seedów × 5 profili: każdy wymagany typ bramy istnieje, leży na terenie zgodnym z typem (port na wodzie żeglownej, lotnisko na terenie o nachyleniu < 3%) |
| WP4 | L-system arterii | WP3 | reguły globalne + ograniczenia lokalne, snapowanie, klasy dróg | sieć bez wiszących końców klasy ≥ Collector; 2 przebiegi tego samego seeda → identyczny bajt w bajt `RoadNetwork` |
| WP5 | Mosty, tunele, kolej towarowa | WP4 | kryteria mostu/tunelu, koszt, A* najtańszej ścieżki dla torów, łączenie odnóg | każda strefa przemysłowa/logistyczna ma bocznicę ≤ 1,2 km od kwartału; brak toru o nachyleniu > 2% |
| WP6 | Kwartały z grafu dróg | WP5 | wyznaczanie ścian planarnych (faces) obchodem półkrawędzi | suma pól kwartałów + pas drogowy = pole obszaru zurbanizowanego (tolerancja 0,5%) |

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

Aksjomat ω: dla każdej bramy `Highway` — propozycja w kierunku centrum, `class = Highway`,
`gen = 0`, `prio = 0`. Dodatkowo 1 propozycja obwodnicowa dla miast > 150 tys.

Reguły produkcji (po zaakceptowaniu segmentu `s` o końcu `p'` i kierunku `d'`):

```
P1  KONTYNUACJA
    Highway|Arterial|Collector → Proposal(p', rotate(d', θ_c), ta_sama_klasa, gen+1, prio+1)
    θ_c wg wzorca globalnego (niżej). Stop gdy gen > max_gen[class] lub p' poza obszarem.

P2  ROZGAŁĘZIENIE BOCZNE   (prawdopodobieństwo p_branch[class], sprawdzane co segment)
    Highway  → Arterial   w ±90°,  prio += 12,  p_branch = 0.15  (zjazdy, tylko co 4. segment)
    Arterial → Collector  w ±90°,  prio += 6,   p_branch = 0.55
    Collector→ Local      w ±90°,  prio += 3,   p_branch = 0.70   (obie strony niezależnie)
    Local    → Service    w ±90°,  prio += 1,   p_branch = 0.20   (tylko kwartały > 2 ha)

P3  ROZWIDLENIE (Y)        tylko Arterial, p = 0.08, dwaj potomkowie w ±(25°..40°)

P4  DOMKNIĘCIE PIERŚCIENIA (tylko Arterial, gen ≥ 3): jeśli w promieniu 1,5·seg_len istnieje
    węzeł Arterial nienależący do przodków — propozycja łącznika prosto do niego, prio += 2.
    Cel: sieć arterii ma być grafem z cyklami, nie drzewem (wymóg Etapu 10: spójność
    i brak pojedynczych punktów odcięcia dzielnicy).
```

**Wzorce globalne** (`θ_c` i preferencja kierunku) — wybierane per dzielnica-zalążek
z `data/districts/*.ron`, ważone epoką pierścienia:

| Wzorzec | Reguła kierunku | Typowo dla |
|---|---|---|
| `Radial` | `d'` = kierunek na/od centrum ± szum 8° | rdzeń staromiejski, promieniste wyloty |
| `Grid(θ)` | `d'` snapowane do najbliższej z {θ, θ+90°, θ+180°, θ+270°}, szum 3° | XIX-wieczne śródmieście, przedmieścia planowane |
| `Organic` | `d'` = `d` ± U(−25°, 25°), preferencja izolinii wysokości | średniowieczny rdzeń, wchłonięta wieś |
| `Contour` | `d'` minimalizuje nachylenie na odcinku (5 kandydatów co 12°) | teren górski, skarpy |
| `Superblock` | `Grid(θ)` z `seg_len ×= 3` dla Collector, brak `Local` wewnątrz | blokowisko lat 60.–80. |

**Ograniczenia lokalne** — stosowane w tej kolejności; pierwsze, które odrzuci, kończy próbę:

1. **Nachylenie.** `TerrainQuery::slope_at` co 8 m wzdłuż kandydata (nachylenie jako `u8`).
   Jeśli `max_slope > class.max_slope`: rotacja o ±15° (próby w kolejności: +15, −15, +30, −30),
   potem skrócenie do 50% długości, potem odrzucenie.
2. **Przeszkoda i wybór struktury.** `TerrainQuery::crossing_cost(a, b)` zwraca kwalifikację
   przeszkody na odcinku wraz z jej długością i różnicą wysokości. M2 **nie liczy tego sam** —
   decyduje tylko, czy dana klasa drogi stać na taką przeprawę:
   - `Crossing::Water { span, bank_delta }` → `Bridge`, jeśli `span ≤ class.bridge_max`
     **i** `bank_delta ≤ 0,05·span`; koszt ×`bridge_cost_mult[class]`, `prio += 4`.
     Most zawsze prostopadle do osi cieku ±20°, inaczej szuka obrotu.
   - `Crossing::Ridge { len, rise }` → `Tunnel { cover_dm }`, jeśli `rise > class.tunnel_trigger`
     **i** `len ≤ class.tunnel_max` (Highway 1200 m, Arterial 600 m, Collector 250 m);
     `prio += 6`. Dla `Local`/`Service` tunele zabronione.
   - `Crossing::Depression { len, drop }` → `Embankment` (nasyp), jeśli `drop ≤ 8 m`.
   - Brak dopuszczalnego wariantu → odrzucenie propozycji.
4. **Snapowanie węzła.** Jeśli `p'` leży w promieniu `0,25·seg_len` od istniejącego węzła
   o klasie ≥ `class` → scalenie (`p'` := ten węzeł, propozycja potomna wygasa).
5. **Przecięcie.** Jeśli segment przecina istniejący: oba dzielone w punkcie przecięcia,
   powstaje węzeł. Jeśli przecinane są dwie różne struktury (`Bridge` × `AtGrade`) →
   bezkolizyjnie, bez węzła (wiadukt).
6. **Minimalny kąt.** Odrzucenie, jeśli kąt do dowolnej krawędzi incydentnej w węźle
   docelowym < 30° (`Local`: < 22°).
7. **Strefa zakazana.** Odrzucenie wewnątrz pasa drogowego innej drogi, na wodzie
   bez mostu, na złożu oznaczonym jako `Extraction` oraz w obrysie lotniska.

Warunek stopu: pusta kolejka albo osiągnięty budżet segmentów wyliczony z `target_pop`
(sekcja 7, budżety). Budżet jest twardy — zabezpiecza przed rozbieganiem się generatora.

#### Kolej towarowa

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
