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
| **Wynik do pokazania** | `headless preview --field zones` → PNG z parcelami w barwach stref, obrysami działek, granicami dzielnic i siecią; `--crop x,y,bok` kadruje wycinek (bez tego działka o froncie 14 m ma na metropolii pół piksela); `--inspect x,y` wypisuje kartę parceli: strefa, właściciel, front drogowy, kwartał, osiedle, pierścień epoki i dzielnica. |
| **Kryterium zamknięcia** | Kryteria WP7–WP9; zero nakładek wielokątów > 1 m², pokrycie dzielnicami bez dziur. |
| **Poprzednia / następna** | `M2b-szkielet-transportu.md` · `M2d-zabudowa.md` |

Etapy 4 i 5: strefowanie kwotowe z pierścieniami epok, sieć ulic lokalnych, podział kwartałów na parcele, a na wierzchu hierarchia dzielnic z §4.3 i ich tożsamość.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| [x] WP7 | Strefowanie z kwotami + pierścienie epok | WP2, WP6 | pola punktowe, przydział kwotowy per kwartał, pierścienie wieku zabudowy | udział każdej strefy w granicach ±3 pp. wobec profilu; brak strefy przemysłowej ciężkiej z nawietrznej względem R1–R3 przy dominującym wietrze (test miękki, próg 90%) |
| [x] WP8 | Sieć lokalna + podział na parcele | WP7 | podział kwartału (rekurencyjny OBB + ulice), pasowy podział na działki wg wymiarów strefy | 100% parcel ma niezerową frontę drogową (poza `Green`/`Water`); zero nakładek wielokątów > 1 m² |
| [x] WP5b | Kolej towarowa | WP7 | A* najtańszej ścieżki dla torów od bramy `RailFreight` do klastrów `IndustryHeavy`/`Logistics`, scalanie wspólnych prefiksów, rozjazdy; opis w §5.2 (`M2b-szkielet-transportu.md`) | każda strefa przemysłowa/logistyczna ma bocznicę ≤ 1,2 km od kwartału; brak toru o nachyleniu > 2%. **Przeniesione z M2b**: trasy prowadzą do stref, więc nie da się ich wyznaczyć przed Etapem 4 |
| [x] WP9 | Dzielnice i ich tożsamość | WP7, WP8 | Voronoi po kwartałach + doginanie granic do arterii/rzek, nazwy, reputacja, `income_tier` | 10–40 dzielnic, pokrycie obszaru zurbanizowanego bez dziur i nakładek; nazwy unikalne |

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

---

## Korekty planu wpisane po implementacji

Numeracja `C-n`; odwołania z kodu (`korekta C7`) wskazują na tę tabelę. Gwiazdką
oznaczone te, które zmieniają **zakres albo kryterium**, a nie tylko sposób liczenia.

| # | Korekta | Dlaczego |
|---|---|---|
| C1 | Pola `noise` i `amenity` liczone jako **zanik z odległości** (Dijkstra po siatce + `exp2`), a nie sumą `ScalarField::decay_from` po źródłach | `decay_from` kosztuje zasięg² **na źródło**, a źródła są tu obszarowe: komórek drogi i wody są w metropolii dziesiątki tysięcy, przy zasięgu 6 okresów połowicznego zaniku daje to miliardy operacji na jedno pole. Wynik jest ten sam co do kształtu (monotoniczny zanik od najbliższego źródła), koszt spada do O(N log N). `decay_from` zostaje dla źródeł punktowych |
| C2 | `flood_risk`, `deposit` i `soil_quality` **nie są** `ScalarField` — są próbkowane po pięć punktów na kwartał | Plan wymienia je wśród pól siatki 16 m, ale jedynym konsumentem punktacji jest kwartał (6 tys. sztuk), a nie komórka (1 mln). Trzy pola po milionie wywołań `buildability_at` kosztowałyby więcej niż cały Etap 4 |
| C3 | Pierścienie epok rosną po **grafie sąsiedztwa kwartałów**, nie po komórkach siatki | Jedynym konsumentem `epoch_ring` jest kwartał, a rasteryzacja 6 tys. wielokątów na milion komórek kosztuje więcej niż cały etap. Rozrost po kwartałach daje to samo uporządkowanie („miasto rosło od środka ku najlepszemu terenowi") przy O(B log B) |
| C4 | Geometria parcel na `f32` w metrach, **nie** w stałoprzecinkowym układzie milimetrowym z ryzyka R2 | R2 zakłada ogólne dzielenie i sumowanie wielokątów. Podział pasowy niczego takiego nie robi: każde cięcie półpłaszczyzną **konstruuje** dwie części o wspólnej krawędzi, więc nakładka nie ma skąd powstać. `f32` przy mapie 16 km ma rozdzielczość ~2 mm wobec tolerancji 1 m² z testu T5, a osobny układ współrzędnych kosztowałby drugą arenę i konwersję w każdym konsumencie |
| C5 ★ | Weta terenowe skalibrowane: `flood_max` 55 → **92**, nachylenie przemysłu 6 → **10** jednostek | Przy progach z pierwszej kalibracji generator wykreślał **92 ze 96** kwartałów miasta nad rzeką jako `Undevelopable` (miasta nadrzeczne stoją w terenie zalewowym z definicji), a przemysł ciężki i logistyka nie miały ani jednego kandydata — ich kwota 9 % wsiąkała w R1. Weto zostaje dla terenu praktycznie w wodzie i dla stoków nie do zabudowania; resztę załatwiają ujemne wagi `Flood` i `Slope` w punktacji |
| C6 ★ | Ściana grafu > 30 ha **leżąca poza promieniem obszaru zurbanizowanego** jest kwartałem pozamiejskim: dostaje strefę otwartą wprost z punktacji, poza kwotami i poza udziałami. Rolnictwo i zieleń dostają limit kwartału (40 i 20 ha), którego tabela §5.4 nie przewiduje | Dwie drogi wylotowe i rzeka zamykają setki hektarów pola — to normalna ściana grafu, ale nie kwartał. Bez tego jedna taka ściana bywa warta 84 % powierzchni miasta czterdziestotysięcznego i żadna kwota nie ma prawa się zgodzić. **Warunek położenia dołożony po obejrzeniu podglądu:** sam próg powierzchni wyrzucał z miasta także ściany wciśnięte między arterie w środku (23 % powierzchni metropolii w 16 ścianach), przez co centrum wypełniały parki wielkości dzielnicy. Taka ściana jest kwartałem, tylko nienaturalnie dużym — dzieli ją sieć lokalna WP8. Limit kwartału dla terenów otwartych jest dopełnieniem: droga polna dzieli pole tak samo jak ulica dzieli kwartał, inaczej 340 ha byłoby **jedną działką** |
| C7 | WP8 idzie w **trzech przebiegach**: geometria (bez dotykania sieci) → podziały segmentów i ulice lokalne → fronty z `CsrGrid<SegmentId>` | Ten sam segment brzegowy bywa zaczepiony z obu sąsiadujących kwartałów. Dzielony pojedynczo, w trakcie liczenia geometrii, rozjeżdżał `SegmentId` drugiemu z nich. Przy okazji przebieg 3 daje **za darmo** indeks „najbliższy odcinek ulicy", który i tak jest kontraktem dla M3 (M2 §6) |
| C8 ★ | **WP9 wykonuje się przed WP8.** Ścieżka krytyczna §4 się nie zmienia (`WP7 → WP9 → WP8`), zmienia się kolejność w kodzie | §5.5 wymaga hierarchii tablicowej: kwartały jednej dzielnicy muszą leżeć w ciągłym zakresie, co znaczy przenumerowanie kwartałów. Po powstaniu parcel trzeba by przestawiać dwie sprzężone tablice zamiast jednej i przepisywać `Parcel.block` oraz `Block.parcels`. WP9 nie potrzebuje z WP8 niczego — Voronoi idzie po kwartałach |
| C9 | `District.pop_capacity` liczone z gęstości strefy (os./ha), nie z sumy mieszkań | Lokale powstają dopiero w M2d (WP12). Pole jest w kontrakcie dla M3, więc musi mieć wartość teraz; M2d je przeliczy z `Unit`. Test pilnuje tylko rzędu wielkości wobec `target_pop` |
| C10 | `District.income_tier` z punktacji zastępczej (dostęp, hałas, atrakcyjność, odległość od centrum), nie z kwantyla `avg_land_value` | Wartość gruntu powstaje w WP15 (M2e), a `income_tier` jest potrzebny już tutaj — wchodzi do `WageBand` i do reputacji. M2e nadpisze go kwantylem, gdy będzie z czego liczyć |
| C11 | Kolej: **jedna Dijkstra od bramy** i cofanie po drzewie poprzedników zamiast A* na każdy klaster | Scalanie wspólnych prefiksów (wymóg planu) wychodzi wtedy za darmo: dwie bocznice do sąsiednich zakładów dzielą odcinek do rozjazdu, bo dzielą ścieżkę w drzewie. Rozjazd to punkt, w którym cofanie trafia na komórkę już zajętą. Osobne A* trzeba by dopiero uczyć scalania |
| C12 ★ | Bufor 400 m i kryterium wiatrowe realizowane przebiegiem naprawczym, który **zamienia** kwartały (kandydat o zbliżonej powierzchni), a nie przenosi | Weto „`IndustryHeavy` bliżej niż 400 m od jakiejkolwiek `Residential`" jest z natury cykliczne: mieszkaniówka powstaje w kroku 3, więc w kroku 1 nie ma czego pilnować. Przeniesienie kwartału na kolejną strefę z rankingu oddaje kwotę przemysłu, której nikt już nie odbiera — w mieście 40-tysięcznym **każdy** kwartał leży w promieniu 400 m od jakiegoś mieszkania, więc weto kasowało całą strefę. Zamiana z kwartałem podobnej wielkości honoruje regułę tam, gdzie geometria pozwala, i zachowuje kwoty tam, gdzie nie pozwala; przypadki nierozwiązywalne idą do `GenerationReport` |
| C13 ★ | Kryterium T9/WP7 („±3 pp.") obowiązuje **od miasta 120-tysięcznego**. Poniżej limitem jest udział największego kwartału (`ZoneResult::largest_block_share`), liczony i raportowany | Kwartał trafia do strefy w całości, więc odchylenia mniejszego niż udział największego kwartału nie da się osiągnąć **żadnym** przydziałem. Zmierzone: metropolia 0,4 pp. przy progu ziarnistości 1,2; miasto 120-tysięczne 0,3 pp. przy progu 3,4; miasto 40-tysięczne 6,4 pp. przy progu 10,4. To jest ograniczenie ziarnistości, nie kalibracji — test honoruje `max(3 pp., 1,5 × próg)` |
| C14 | Bocznica powstaje tylko do klastra **dalszego niż 1,2 km** od istniejącego toru; klastry przetwarzane od największego | Kryterium WP5b mówi o promieniu obsługi, nie o odnodze na klaster. Bez tego warunku każda kępa hal dostawała własny tor i metropolia kończyła z **197 km** torów wobec ~45 km z budżetu §7; po korekcie 73 km przy 15 bocznicach i 99 klastrach obsłużonych z istniejących. Reszta rozjazdu między 73 a 45 km to trasa od bramy na krawędzi mapy do miasta — do kalibracji w M2e |
| C15 | Ulica lokalna **nie powstaje**, gdy spadek między ulicami, które miała połączyć, przekracza limit jej klasy. `is_connected` liczy tylko sieć jezdną, a `BlockSet` zapamiętuje `(V, E, C)` z chwili budowy kwartałów | Trzy konsekwencje dołożenia czegokolwiek do `RoadNetwork` po M2b: (1) rzędne ulicy lokalnej są narzucone przez ulice, które łączy, więc nie ma czego negocjować — kwartał zostaje niepodzielony, a działki dostają front od strony istniejącej drogi; (2) tory są osobną składową spójności z definicji, bo pociąg nie skręca w ulicę; (3) niezmiennik Eulera dotyczy grafu, **z którego kwartały powstały**, a nie tego, co jest w sieci po WP5b i WP8 |
| C16 | Kryterium wiatrowe WP7 jest **miejskie, nie sąsiedzkie**: przemysł ciężki ma leżeć po zawietrznej względem środka ciężkości zabudowy R1–R3 | Wersja lokalna („żaden kwartał R1–R3 w stożku 1 km pod wiatr") jest w zwartym mieście nie do spełnienia przez żaden przydział — przedmieścia otaczają zakład ze wszystkich stron — i mierzyłaby gęstość miasta, nie urbanistykę. Sterujący tym `ZoneField::Downwind` jest z tej samej skali: liczy się względem środka miasta |
| C17 | Strefy bez kandydata są raportowane (`ZoneResult::candidates`) i testowane | Kwota, której nikt nie może dostać, wygląda w udziałach identycznie jak kwota, której zabrakło miejsca, a jest czymś zupełnie innym: błędem kalibracji wag, nie generacji. Bez tej liczby diagnoza „Logistics: 0 %" zajmowała pół godziny |
| C18 | Oś ulicy lokalnej biegnie między **sąsiednią parą** przecięć z brzegiem podkwartału, a nie między pierwszym a ostatnim | Przy podkwartale niewypukłym prosta cięcia wychodzi z niego i wraca, więc odcinek od pierwszego do ostatniego przecięcia biegłby kawałkiem na zewnątrz — przez cudzy kwartał i w poprzek otaczającej drogi. Łamie to planarność sieci, a widać dopiero w teście WP6 (jedno przecięcie bez węzła na metropolię), nie na podglądzie |

### Zmierzone

| Miara | Metropolia 16 km (400 tys.) | Miasto 8 km (120 tys.) | Miasto 4 km (40 tys.) |
|---|---|---|---|
| pola wpływu (7 przebiegów Dijkstry) | 504 ms | 60 ms | 31 ms |
| strefy + pierścienie epok | 134 ms | 13 ms | 1,4 ms |
| dzielnice | 2,6 ms | 0,9 ms | 0,2 ms |
| kolej towarowa | 349 ms | 90 ms | 20 ms |
| sieć lokalna + parcele | 14 ms | 3,5 ms | 0,9 ms |
| **razem miasto (z M2b)** | **~1,3 s** | 336 ms | 65 ms |
| odchylenie kwot stref | 0,2 pp. (próg 5,8) | 0,3 pp. (próg 3,4) | 6,4 pp. (próg 10,4) |
| kwartały / parcele | 1 086 / 20 354 | 400 / 4 999 | 96 / 1 217 |
| ulice lokalne / podziały segmentów | 733 / 1 283 | 129 / 220 | 38 / 71 |
| dzielnice | 33 | 10 | 10 |
| tory | 68,2 km, 12 bocznic, 9 rozjazdów | 52,8 km, 37 / 34 | 5,3 km, 4 / 1 |
| parcele bez frontu drogowego | 0 | 0 | 0 |

Budżety §7 dla metropolii: strefowanie + pierścienie + dzielnice ≤ 3 s (zmierzone 0,64 s),
kwartały + sieć lokalna + parcele ≤ 6 s (0,01 s), pola skalarne + Dijkstra ≤ 4 s (0,50 s).
Liczba kwartałów (1 086) jest wyraźnie poniżej budżetu §7 (~6 000) — to ten sam rozjazd
gęstości, który M2b zgłosił w korekcie do §7 i który domyka kalibracja w M2e. Liczba parcel
(20,4 tys. wobec ~42 tys.) idzie w ślad za nią, ale rozjazd jest mniejszy, bo na kwartał
przypada 19 działek zamiast planowanych 7.

### Kontrola wzrokowa

Podgląd jest tu przyrządem, nie ilustracją — dwie rzeczy wyszły **wyłącznie** z obejrzenia
obrazka i żaden test ich nie widział:

1. **Parcele rysowane samym wypełnieniem zlewały się w plamę.** Sąsiednie działki tej samej
   strefy mają tę samą barwę, więc kwartał podzielony na dwadzieścia działek wyglądał
   identycznie jak niepodzielony. Stąd obrysy działek w trybie `zones` — bez nich podgląd
   nie dowodzi niczego o podziale, a właśnie on jest wynikiem tej podfazy.
2. **Parki wielkości dzielnicy w środku miasta** — próg 30 ha z korekty C6 wyrzucał z kwot
   także ściany leżące w centrum. W liczbach wyglądało to niewinnie („pozamiejskich 16"),
   na mapie — jak dziura w mieście. Stąd warunek położenia.
