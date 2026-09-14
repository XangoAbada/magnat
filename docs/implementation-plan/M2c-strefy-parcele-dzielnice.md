# M2c — Strefy, parcele, dzielnice

Podfaza 3 z 5 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2b (kwartały, bramy, `RoadNetwork`), M2a (pola skalarne, `multi_source_dijkstra`). |
| **Pakiety robocze** | WP7, WP8, WP9, **WP5b** (kolej towarowa — przeniesiona z M2b, korekta B1) |
| **Projekt techniczny** | §5.3, §5.4, §5.5 |
| **Wynik do pokazania** | Podgląd: mapa stref i parcel z granicami dzielnic; kliknięcie parceli daje strefę, właściciela, frontę drogową i dzielnicę. |
| **Kryterium zamknięcia** | Kryteria WP7–WP9; zero nakładek wielokątów > 1 m², pokrycie dzielnicami bez dziur. |
| **Poprzednia / następna** | `M2b-szkielet-transportu.md` · `M2d-zabudowa.md` |

Etapy 4 i 5: strefowanie kwotowe z pierścieniami epok, sieć ulic lokalnych, podział kwartałów na parcele, a na wierzchu hierarchia dzielnic z §4.3 i ich tożsamość.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| WP7 | Strefowanie z kwotami + pierścienie epok | WP2, WP6 | pola punktowe, przydział kwotowy per kwartał, pierścienie wieku zabudowy | udział każdej strefy w granicach ±3 pp. wobec profilu; brak strefy przemysłowej ciężkiej z nawietrznej względem R1–R3 przy dominującym wietrze (test miękki, próg 90%) |
| WP8 | Sieć lokalna + podział na parcele | WP7 | podział kwartału (rekurencyjny OBB + ulice), pasowy podział na działki wg wymiarów strefy | 100% parcel ma niezerową frontę drogową (poza `Green`/`Water`); zero nakładek wielokątów > 1 m² |
| WP5b | Kolej towarowa | WP7 | A* najtańszej ścieżki dla torów od bramy `RailFreight` do klastrów `IndustryHeavy`/`Logistics`, scalanie wspólnych prefiksów, rozjazdy; opis w §5.2 (`M2b-szkielet-transportu.md`) | każda strefa przemysłowa/logistyczna ma bocznicę ≤ 1,2 km od kwartału; brak toru o nachyleniu > 2%. **Przeniesione z M2b**: trasy prowadzą do stref, więc nie da się ich wyznaczyć przed Etapem 4 |
| WP9 | Dzielnice i ich tożsamość | WP7, WP8 | Voronoi po kwartałach + doginanie granic do arterii/rzek, nazwy, reputacja, `income_tier` | 10–40 dzielnic, pokrycie obszaru zurbanizowanego bez dziur i nakładek; nazwy unikalne |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.3 Etap 4 — strefowanie

#### Klasy stref

```rust
pub enum ZoneKind {
    Residential(ResDensity),   // 5 klas gęstości wg PRD §4.2
    Commercial,                // handlowa
    Office,                    // usługowa/biurowa
    IndustryLight,
    IndustryHeavy,
    Logistics,
    Agriculture,
    Institutional,             // publiczna
    Green,                     // zielona: park, las miejski, cmentarz, skwer
    Extraction,                // wydobywcza
    Water,
    Undevelopable,             // nachylenie, tereny zalewowe, pas ochronny
}
pub enum ResDensity { R1, R2, R3, R4, R5 }
// R1 domy wolnostojące · R2 szeregowa/bliźniacza · R3 kamienice/niska zwarta
// R4 bloki 4–11 kondygnacji · R5 wieżowce mieszkalne
```

#### Pola wejściowe (wszystkie `ScalarField`, siatka 16 m)

| Pole | Źródło | Uwaga |
|---|---|---|
| `d_center` | Dijkstra po sieci dróg od centroidu centrum | **nie** euklidesowo — rzeka bez mostu ma być barierą |
| `access_road` | Dijkstra ważony klasą drogi | wjazd z Highway ≠ wjazd z Service |
| `d_gate_rail`, `d_gate_road`, `d_port` | Dijkstra od bram | wejście dla logistyki i przemysłu |
| `noise` | `decay_from` po segmentach Highway/Arterial/Rail + strefach przemysłowych | half-life 120 m |
| `amenity` | `decay_from` po wodzie, lesie, parkach + bonus za widok (różnica wysokości) | half-life 300 m |
| `slope`, `flood_risk` | M1 (teren, hydrologia) | `flood_risk` > próg → `Undevelopable` |
| `deposit` | M1 (geologia) | koncentracja złoża → kandydat na `Extraction` |
| `soil_quality` | M1 (biomy, gleba) | `Agriculture` |
| `epoch_ring` | patrz niżej | oś „starówka ↔ blokowisko ↔ przedmieścia" |

#### Pierścienie epok

Zamiast losowania „stylu" — **symulowany wzrost footprintu**. Dla `n_epok` epok z
`data/epochs/` (np. średniowiecze, XIX w., międzywojnie, powojenne bloki, transformacja,
współczesność) footprint miasta jest rozszerzany o powierzchnię wynikającą z krzywej
populacji epoki, priorytetowo na komórki o najwyższym `access_road + amenity − slope`.
Każdy kwartał dostaje `epoch_ring: u8` = epoka, w której został objęty footprintem.
To jeden przebieg BFS po polu priorytetu, O(N log N), w pełni deterministyczny.
`epoch_ring` parametryzuje potem: wzorzec L-systemu, wielkość kwartału, wymiary działki,
zestaw gramatyk i paletę materiałów. Epoka startowa gry to ostatni pierścień —
miasto startujące w 1990 **ma** starówkę, jeśli krzywa epok ją przewiduje.

#### Przydział z kwotami

Strefy przypisujemy **kwartałom**, nie komórkom (inaczej powstaje szum i kwartał
z czterema strefami). Procedura:

```
1. Dla każdego kwartału b policz wektor punktacji:
       score[b][z] = Σ_f  w[z][f] · field_f(centroid_ważony(b))      // w[] z data/zoning/
   Twarde weta (score = −∞): Water, flood_risk > próg, slope > max[z],
   IndustryHeavy bliżej niż 400 m od jakiejkolwiek Residential, Extraction bez złoża.
2. Kwoty: target_area[z] = urban_area × mix[z][profile][epoch].
3. Przydział zachłanny z marginesem: kwartały sortowane malejąco po
       margin[b] = max_z score[b][z] − drugi_max_z score[b][z]
   (remisy po BlockId rosnąco). Kwartał trafia do najlepszej strefy, w której
   pozostała kwota ≥ jego powierzchni; jeśli brak — do następnej w kolejności punktacji.
4. Wygładzanie: 2 przebiegi. Kwartał, którego ≥ 3 z 4 sąsiadów mają tę samą inną strefę
   i którego margin < 0,15, przechodzi do strefy sąsiadów, jeśli kwoty na to pozwalają.
   Iteracja po BlockId rosnąco, zmiany aplikowane po przebiegu (nie w trakcie).
```
Wynik: udział każdej strefy w ±3 pp. wobec profilu, bez plam pikselowych, deterministycznie.
Złożoność O(B·Z + B log B), B ≈ 6 000 kwartałów, Z = 16 → pomijalne.

### 5.4 Etap 5 — sieć lokalna i parcele

#### Kwartały

**Zrobione w M2b (WP6)** — tu zostaje opis, bo numeracja §5.x jest z dokumentu fazy.

`RoadNetwork` jest grafem planarnym (po ograniczeniu lokalnym 4 każde przecięcie ma węzeł).
Z planaryzacji wypadają **tylko te** krawędzie, które faktycznie krzyżują się z inną bez
węzła — most albo tunel nad drogą po gruncie; obie dostają wtedy `RoadFlags::GRADE_SEPARATED`.
Nasyp bezkolizyjny **nie jest** (korekta B13/B14 w M2b). Ściany wyznaczamy obchodem
półkrawędzi: w każdym węźle krawędzie posortowane po kącie (pseudokątem, bo `atan2` jest
zakazane w kodzie symulacji — 00 §K-6), następna półkrawędź = „najbardziej w prawo". O(E).
Ściana zewnętrzna (o polu niedodatnim) odrzucana. Kwartały o powierzchni < 300 m² są
**odrzucane**, nie scalane — patrz korekta B19 w M2b. Obrys użytkowy kwartału powstaje przez
odsunięcie ściany do wewnątrz o połowę pasa drogowego każdej krawędzi, z ogranicznikiem ostrza.

```rust
pub struct Block {
    pub id: BlockId,
    pub district: DistrictId,
    pub neighborhood: u16,         // „osiedle" — grupa kwartałów, tożsamość drobniejsza niż dzielnica
    pub poly: PolyRef,
    pub area_m2: u32,
    pub zone: ZoneKind,
    pub epoch_ring: u8,
    pub parcels: Range<u32>,       // ciągły zakres w globalnej tablicy parcel
    pub bounding: Range<u32>,      // CSR → SegmentId otaczających dróg
}
```

#### Sieć lokalna

Kwartał większy niż `max_block_area[zone]` jest dzielony rekurencyjnie:
oblicz prostokąt minimalnego obwodu (OBB), przetnij w poprzek dłuższej osi w punkcie
`0,5 ± U(−0,12, 0,12)` i wstaw w cięciu ulicę klasy `Local` (lub `Service`, gdy obie połówki
< 0,6 ha). Stop gdy obie połówki ≤ `max_block_area` **lub** gdy krótszy bok < 2·`depth[zone]`.
Ślepe zakończenia dopuszczalne tylko dla R1/R2 (`cul-de-sac` z zawrotką o R = 9 m) i tylko
gdy `epoch_ring` ≥ epoki przedmieść — w pierścieniu staromiejskim ulica musi się domykać.

`max_block_area`: R1 4 ha · R2 2,5 ha · R3 1,2 ha · R4 6 ha · R5 4 ha · Commercial 3 ha ·
Office 2,5 ha · IndustryLight 8 ha · IndustryHeavy 30 ha · Logistics 20 ha ·
Institutional 6 ha · Agriculture i Green bez limitu.

#### Podział na parcele

Podział pasowy od frontu ulicy. Dla każdej krawędzi kwartału leżącej przy drodze
(krawędź frontowa), w kolejności malejącej klasy drogi, potem po `SegmentId`:

```
1. Odsuń krawędź do wewnątrz o depth[zone] → pas zabudowy.
2. Podziel długość frontu na odcinki o szerokości losowanej z rozkładu frontage[zone]
   (trójkątny: min/mode/max), rng(seed, StreamId::Parcels, block_index, i).
   Reszta < min → doklejana do ostatniej działki (nigdy nie powstaje działka poniżej min).
3. Prostuj boczne granice: prostopadle do frontu (Grid/Superblock) lub promieniście
   do centroidu kwartału (Radial/Organic).
4. Wnętrze kwartału pozostałe po pasach: jeśli pole > 0,4 ha i zone ∈ {R4,R5,Commercial}
   → jedna parcela wewnętrzna z dostępem przez bramę/przejazd; w przeciwnym razie
   → parcela typu Green (podwórko) należąca do miasta lub scalona z najgłębszą działką.
```

| Strefa | front [m] min/mode/max | głębokość [m] | pole [m²] |
|---|---|---|---|
| R1 | 18 / 22 / 30 | 32 | 600–1 200 |
| R2 | 10 / 14 / 18 | 30 | 320–600 |
| R3 | 9 / 14 / 22 | 38 | 340–900 |
| R4 | 40 / 60 / 95 | 55 | 2 200–7 000 |
| R5 | 30 / 45 / 65 | 50 | 1 500–4 000 |
| Commercial | 20 / 35 / 60 | 55 | 1 000–5 000 |
| Office | 25 / 40 / 60 | 50 | 1 200–4 000 |
| IndustryLight | 60 / 85 / 120 | 110 | 5 000–18 000 |
| IndustryHeavy | 120 / 200 / 300 | 260 | 2–12 ha |
| Logistics | 100 / 140 / 200 | 180 | 1,5–5 ha |
| Institutional | 40 / 70 / 140 | 80 | 3 000–20 000 |
| Agriculture | 120 / 200 / 400 | 380 | 3–20 ha |
| Extraction | — (kształt złoża) | — | 5–60 ha |

```rust
pub struct Frontage { pub seg: SegmentId, pub t0: f32, pub t1: f32 }  // parametryzacja wzdłuż osi drogi
pub enum ParcelOwner { Unowned, City, Citizen(CitizenId), Firm(FirmId), Developer(FirmId) }
pub enum ParcelStatus { Vacant, Built, UnderConstruction { done_at: SimMinute }, Derelict, Reserved }

pub struct Parcel {
    pub block: BlockId,
    pub district: DistrictId,
    pub poly: PolyRef,
    pub area_m2: u32,
    pub frontage: Frontage,
    pub zone: ZoneKind,
    pub owner: ParcelOwner,
    pub status: ParcelStatus,
    pub land_value_per_m2: Money,      // grosze; statyczna w M2
    pub building: Option<BuildingId>,
}
```
**Właściciele w M2.** Nie ma jeszcze mieszkańców ani prawdziwych firm, więc M2 ustawia:
`City` (drogi, zieleń, publiczne, ~12% parcel), `Firm(FirmId)` dla parcel z `SiteSeed`,
`Unowned` dla pustych, a parcele mieszkaniowe — `City` ze statusem `Reserved`.
**M3 (Etap 8) przepisuje właścicieli parcel mieszkaniowych na gospodarstwa domowe.**
To jedyny punkt, w którym M3 mutuje strukturę własności z M2 — kontrakt w sekcji 6.

### 5.5 §4.3 — dzielnice i hierarchia

Hierarchia jest **tablicowa, nie wskaźnikowa**: każdy poziom to `Vec` posortowany tak, że
dzieci każdego rodzica leżą w ciągłym zakresie (`Range<u32>`). Zaleta: iteracja po dzielnicy
jest ciągła w pamięci, zapytanie „lokale w budynku" to slice, a hash stanu (dok. 00 §3.6)
liczy się po tablicach w naturalnej kolejności.

```
World → districts[0..D] → blocks[d.blocks] → parcels[b.parcels] → building
      → buildings[..]   → units[bld.units] → workplaces[unit.workplaces]
```

```rust
pub struct District {
    pub name: String,                  // z data/names/, unikalna, z kontrolą kolizji
    pub kind: DistrictKind,            // OldTown | InnerCity | BlockEstate | Suburb
                                       // | IndustrialBelt | PortQuarter | Village | Campus | GreenBelt
    pub founded_epoch: EpochId,
    pub style: StyleId,                // klucz do palety materiałów i zestawu gramatyk
    pub boundary: PolyRef,
    pub blocks: Range<u32>,
    pub centroid: Vec2,
    pub reputation: Q,                 // start; M8/M10 zmieniają
    pub crime: Q,                      // start
    pub income_tier: u8,               // 0..=4 — M3 mapuje na klasy społeczne
    pub avg_land_value: Money,         // za m²; liczone po WP15
    pub pop_capacity: u32,             // suma mieszkań × wielkość GD epoki — wejście dla M3
}
```

Wyznaczanie granic:

1. **Zalążki.** Obowiązkowe: centrum, każda brama, centroid każdego klastra przemysłowego
   ≥ 30 ha. Uzupełnienie próbkowaniem Poissona o `d_min = sqrt(urban_area / target_count)`,
   `target_count` = clamp(10, 40, `target_pop` / 12 000).
2. **Voronoi po kwartałach** z metryką: odległość po sieci dróg (`ScalarField` od zalążka)
   + kara 0,35 za różnicę `epoch_ring` + kara 0,5 za różnicę klasy strefy.
3. **Doginanie do barier.** Dwa przebiegi po kwartałach rosnąco po `BlockId`: jeśli granica
   dzielnicy przecina kwartał, którego ≥ 60% obwodu styka się z arterią, koleją lub rzeką,
   kwartał przechodzi w całości na stronę wskazaną przez tę barierę. Zmiany aplikowane
   po przebiegu. Efekt: granice dzielnic biegną ulicami i rzeką, nie po przekątnej kwartału.
4. **Tożsamość.** `kind` z dominującej pary (`epoch_ring`, `zone`); `style` z `kind` + epoki;
   `income_tier` z kwantyla `avg_land_value` (5 kwantyli); `reputation` = f(`income_tier`,
   udział `Green`, hałas); `crime` = f(−`income_tier`, gęstość, odległość od centrum).
   Nazwa: `data/names/districts_pl.ron` — szablony (`{przymiotnik} {rdzeń}`, toponimy
   od cech terenu: „Zarzecze" przy rzece, „Podgórze" przy skarpie), unikalność wymuszana
   sufiksem kierunkowym.
