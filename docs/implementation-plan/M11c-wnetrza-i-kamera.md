# M11c — Wnętrza i kamera FPP

Podfaza 3 z 5 fazy **M11 — Prezentacja** (`M11-prezentacja.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M11a (snapshot), M2 (wnętrza logiczne), M9 (panele). |
| **Pakiety robocze** | WP12, R2-WP15, R2-WP17, R2-WP18, WP5, WP9 |
| **Projekt techniczny** | §5.7 |
| **Wynik do pokazania** | Wejście do własnego sklepu z **ulicy, na której są ludzie i samochody**; szyld firmy gracza widoczny z zewnątrz, a firm w mieście tyle, żeby szyldów było co wieszać. |
| **Kryterium zamknięcia** | Kryteria WP12, R2-WP15, R2-WP17, R2-WP18, WP5 i WP9. |
| **Poprzednia / następna** | `M11b-animacja-i-lod.md` · `M11d-swiatlo-pogoda-dzwiek.md` |

Kamera FPP, `CutPlane` do cięcia poziomami, `InteriorKit` oraz szyldy i barwy firm gracza —
**a przed tym wszystkim ludzie i auta w kadrze oraz miasto, w którym jest ich dokąd posłać**
(cztery pakiety przejęte z R2 decyzją właściciela produktu, `J-1`).

---

## Pakiety robocze

| | WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|---|
| [x] | WP12 | Ulica ma ruch: okno Mikro, promień rysowania, trasa pieszego | WP2 (M11a) | M |
| [x] | R2-WP15 | Chodniki: warstwa piesza bez dróg szybkiego ruchu | — | S |
| [x] | R2-WP17 | Kopalnia staje na złożu | `D-N13` | M |
| [~] | R2-WP18 | Gęstość firm i pasmo bezrobocia — **pomiar, `D-N20`, → R3** | R2-WP17 | L |
| [x] | WP5 | Kamera FPP, `CutPlane`, `InteriorKit` | WP2, M2, M9 | L |
| [x] | WP9 | Szyldy i barwy firm gracza | WP1, M9 | S |

**Cztery pierwsze pakiety weszły do tej podfazy decyzją właściciela produktu (2026-09-19)**
i idą **przed** WP5 i WP9 — powód w tabeli `J-n` na końcu dokumentu. Najkrócej: M11c ma pokazać
wejście do własnego sklepu z poziomu ulicy, a ulica jest dziś pusta i nie ma na niej szyldów,
bo firm jest dziesięciokrotnie za mało. Kryterium WP5 („spacer FPP bez artefaktów") przeszłoby
w martwym mieście tak samo jak w żywym, więc kolejność jest treścią, nie porządkiem.

### WP12 — Ulica ma ruch

**Opis.** Trzy naprawy jednej rzeczy: tego, że w kadrze nie ma ludzi ani aut (pozycje 70 i 71
wykazu R2).

**(1) Okno warstwy Mikro i wycinek snapshotu idą za tym, na co gracz patrzy, nie za okiem
kamery.** Dziś `citizens.rs::okno_mikro` podaje `camera.eye()`, a przy orbicie z 900 m oko stoi
767 m w poziomie od celu — dysk o promieniu 900 m jest przesunięty o tyle samo i połowa leży
za plecami kamery. To samo dotyczy `ViewQuery.aabb`, liczonego wokół oka.

**(2) Promień rysowania encji przestaje być stałą mniejszą od dystansu orbity.** `DRAW_RADIUS_M`
to 600 m mierzone **od oka**, więc przy domyślnym `--dist 900` punkt, na który gracz patrzy,
jest poza promieniem — w widoku dzielnicy nie widać **żadnej** encji i to jest stan domyślny gry.
Próg ma wynikać z rozmiaru ekranowego encji, a nie z jednej liczby: encja mniejsza od piksela
odpada i tak, a encja w środku kadru nie ma prawa odpaść nigdy.

**(3) Pieszy idzie polilinią z grafu pieszego, nie prostą od środka budynku do środka budynku.**
`journey.rs::enter_micro_inner` podaje warstwie Mikro dwa punkty, więc mieszkaniec przenika przez
kwartały, a w połowie drogi bywa pod ziemią albo nad nią. Węzły grafu mają poprawne `z_cm`
(łańcuch `TerrainQuery::height_at` → `lsystem` → `nav_build`) i nikt ich w tej ścieżce nie czyta.
`G-13` z M11b wyprostowało **końce** trasy; ten pakiet prostuje środek. Przy okazji wejście do
warstwy przestaje być jednorazowe w minucie wyruszenia — inaczej po przeskoku kamery nowe okno
napełnia się kilkanaście minut symulacji.

**Czego ten pakiet nie robi.** Nie zmienia udziału podróży pieszych ani samochodowych: 70 % pieszo
i 10 % autem dla miasta 28 tys. to wynik modelu wyboru środka transportu (`data/roads/mode_choice.ron`),
zmierzony i przyjęty w M4c. Liczba pieszych w kadrze ma wynikać z tego udziału, a nie z bramki okna.

**Kryterium ukończenia** (godzina i tolerancja poprawione po pomiarze — `J-9`, `J-10`).
W widoku dzielnicy (`--dist 900`, seed odniesienia 7, **8:00**) w kadrze jest **co najmniej
200 mieszkańców i co najmniej 10 pojazdów**, a `snapshot:` w raporcie zrzutu pokazuje tę samą
liczbę co `stan renderu:` z dokładnością do odcięcia stożkiem. Test odtwarzający: pieszy idący
między dwoma budynkami po przeciwnych stronach kwartału **nie przechodzi przez obrys żadnego
budynku**, a jego odchylenie od terenu naturalnego mieści się w robotach ziemnych pod jezdnią
(≤ 5 m) i jest **wyraźnie mniejsze** niż odchylenie odcinka prostego między tymi samymi końcami.

### R2-WP15 — Chodniki: warstwa piesza bez dróg szybkiego ruchu

Przejęty z `R2c` bez zmiany zakresu. `nav_build.rs` buduje warstwę `Modality::Foot` ze wszystkich
segmentów poza koleją, więc marsz wzdłuż obwodnicy jest wykonalny, tani i przy niskiej wartości
czasu **wygrywa** — jest jedyną opcją bez składnika pieniężnego. Flaga `RoadFlags::SIDEWALK`
nadawana z klasy drogi przy budowie sieci: `highway` i `expressway` bez chodnika, reszta z chodnikiem.

**Kryterium ukończenia** (z `R2c`): trasa piesza między punktami po obu stronach obwodnicy prowadzi
przez najbliższe przejście, nie po obwodnicy; test `parcel_unreachable` przechodzi na wszystkich
pięciu regionach.

**Doprecyzowane po wykonaniu (`J-13`):** chodnik zostaje przy odcinku drogi szybkiego ruchu,
przy którym stoi parcela — inaczej fabryka przy szosie nie ma jak wypuścić pracownika
i `ParcelUnreachable` zapala się na czymś, czego graf nie naprawi. Front działki wybiera się
**wśród najbliższych odcinków z chodnikiem**, a nie z jednego najbliższego, więc takich
wyjątków zostaje dziewięć w `River` i zero w pozostałych czterech regionach.

### R2-WP17 — Kopalnia staje na złożu

Przejęty z `R2d` bez zmiany zakresu. Wypełniacz stref stawia zakłady wydobywcze bez sprawdzania
złoża, więc w mieście 4 km nie stoi **ani jedna** kopalnia na złożu, a napisany i przetestowany
model wyczerpywania złoża nigdy się nie uruchamia. Archetypy z `needs_deposit` wypadają z wypełniacza
stref i idą wyłącznie ścieżką klastrową, która złoże sprawdza.

**Decyzja `D-N13` przyjęta wg propozycji domyślnej** (właściciel produktu, 2026-09-19): profil
gospodarczy wymagający wydobycia w regionie bez złóż **nie dostaje zakładu**, a domknięcie łańcuchów
uruchamia import przez bramę — mechanizm istnieje i ma przepustowość, elastyczność ceny i cło.
Wariant „przesuń profil na inny region" dawałby świat inny, niż deklaruje wiersz poleceń; wariant
„dosiej złoże" łamie `K-13`.

**Wchodzi tutaj, bo blokuje WP18**: oba pakiety edytują `sim/world/src/city/sites/place.rs`
i `sim/world/src/city/report.rs`, a WP17 zmienia skład zakładów, więc zrobiony po WP18 unieważniłby
pomiar, na którym stoi kryterium WP18.

**Przyczyna okazała się inna niż w `R2d` (`J-14`).** Wypełniacz stref kopalń nigdy nie stawiał —
archetypy z `needs_deposit` mają `weight: 0`, a wypełniacz losuje wagą. Zakładów wydobywczych
nie było, bo **bilans popytu ich nie zamawiał**: szukały działki w strefie `Extraction`, a nie
na złożu, i odpadały na progu `SCALE_MIN` razem z każdym zakładem, którego wyrób da się sprowadzić.
Zamknięte trzema zmianami w `place.rs` — wykluczenie z wypełniacza (jak w planie), szukanie
działki **po złożu zamiast po strefie**, i zniesienie progu skali dla zakładu stojącego na złożu.

### R2-WP18 — Gęstość firm i pasmo bezrobocia

Przejęty z `R2d` bez zmiany zakresu, **razem z jego sufitem pracy**. Miasto ma jedną firmę na
~130 mieszkańców wobec obiecanych 1 : 15–25, a bezrobocie wynosi 0,2 % przy 12 032 nieobsadzonych
etatach. Pakiet **zaczyna się od pomiaru**, która z dwóch przyczyn zachodzi (normatyw liczy moc
produkcyjną zamiast liczby podmiotów; obsada normatywna wygenerowała więcej stanowisk, niż Etap 8
zasiedlił ludzi), i dopiero potem dobiera naprawę. Normatyw mocy produkcyjnej zostaje — zmienia się
**rozdrobnienie** mocy na podmioty.

**Sufit z `D-N6` obowiązuje bez zmian:** po dwóch dniach pakiet kończy się **pomiarem i decyzją
otwartą**, nie kodem, i przenosi się do R3. Etap 7 jest jedynym miejscem w projekcie, w którym
naprawa może wymagać przeprojektowania, a nie domknięcia — a wtedy należy do własnej fazy.

**Kryterium ukończenia** (z `R2d`): świat 4 km ma stosunek mieszkańców do firm poniżej 1 : 40,
a nieobsadzone etaty po pięciu latach nie przekraczają 15 % wszystkich; bramka G11 (bezrobocie
3–12 %) przechodzi w biegu nocnym.

**Zamknięte pomiarem, nie kodem — sufit `D-N6` zadziałał (`J-16`).** Pomiar stoi w kodzie
(`tools/headless/src/population.rs`, `gestosc_firm`) i mówi, która z dwóch przyczyn zachodzi:
**obie, i obie wychodzą z jednego założenia** — zakład bierze **cały budynek**. Lokali użytkowych
jest 4 186, zakładów 202; obsada liczy się z powierzchni całego budynku, więc `it_office` ma
183 etaty. Naprawa („zakład jest lokalem, nie budynkiem") dotyka `utworz_zaklady`,
`rebind_workplaces` i `by_building` z wszystkimi jego czytelnikami, i mnoży liczbę firm przez
pięć — to jest przeprojektowanie Etapu 7. Pakiet przechodzi do **R3** z decyzją otwartą `D-N20`.

### WP5 — Kamera FPP, cięcie poziomami, wnętrza

**Opis.** `CameraMode::FirstPerson(CitizenId)` śledzący postać gracza wyłącznie przez odczyt
pozycji ze snapshotu. `CutPlane` jako clip w shaderze + pass domykający przekrój (`cap pass`),
żeby przecięta ściana nie była pustą skorupą. `InteriorKit` generuje wyposażenie proceduralnie
z gramatyki budynku (M2) i stanu zakładu ze snapshotu — regały z wypełnieniem wg stanu magazynu,
linie produkcyjne, biurka.

**Kryterium ukończenia.** Cięcie 4-piętrowego centrum handlowego na poziomie 2 przy ≤ 16,6 ms;
regały odzwierciedlają stan magazynu (test: zmiana zapasu o 50% zmienia liczbę widocznych propów);
spacer FPP po sklepie gracza bez artefaktów near-plane.

### WP9 — Szyldy i barwy firm gracza

**Opis.** `SignAtlas` — nazwy firm gracza renderowane przez font atlas z `engine/ui` (M9) do
tekstury R8 (kanał alfa; kolor z palety marki), tile 256×64. Szyld jako prop `.mvox` z jednym
slotem `Sign` mapowanym na tile atlasu. To samo dla `livery` pojazdów — oklejenie firmowe.

**Kryterium ukończenia.** Zmiana nazwy firmy w UI aktualizuje szyld w świecie w ≤ 1 klatce;
100 różnych szyldów w kadrze w jednym draw callu.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.7 Kamera, cięcie poziomami, wnętrza

**Korekta po uzgodnieniu z M1: kamera w całości należy do M1, łącznie z widokiem
pierwszoosobowym.** PRD §15.2 jest w zakresie M1, więc `CameraMode::{Orbit, Free,
FirstPerson { anchor: EntityId, eye_height_m: f32 }}` **dostarcza M1** — M11 nie definiuje
drugiego wariantu, tylko **wiąże istniejący `anchor` z encją postaci gracza** (M9; do czasu M9
`anchor` wskazuje encję atrapę). `clip_plane_z` jest uniformem M1 i to M1 klampuje geometrię
chunków. Moja jest wyłącznie **warstwa przekroju**: `CutPlane` (wyliczenie `world_y` ze stropów)
i cap pass domykający ściany.

```rust
pub struct CutPlane {
    pub mode: CutMode,              // Off | Level(u8) | Box(Aabb) | FocusBuilding(u32)
    pub world_y: f32,               // wyprowadzane z floor_heights_dm, nie zgadywane
    pub fade: f32,                  // 0,5 m miękkiej granicy — twarda krawędź wygląda jak błąd
}
```

**`Level(n)` tnie po realnym stropie, nie po równych odstępach.** M2 dostarcza
`Building.floor_heights_dm` — wysokości poszczególnych kondygnacji, bo parter usługowy jest wyższy
niż piętro mieszkalne. `world_y = base + suma(floor_heights_dm[0..n])`. Gdyby ciąć co stałą
wysokość, przekrój przechodziłby przez środek witryny na parterze każdej kamienicy.
`Unit.floor: i8` (ujemne = podziemie) pozwala ciąć również piwnice i magazyny podziemne.

**Pion liczy się w jednostkach 0,5 m, nie w metrach — to najłatwiejszy błąd o czynnik 2 w tej fazie.**
Voxel M1 ma 1 × 1 × **0,5 m** (PRD §15.1 mówi o rozdzielczości 1 m i to prawda w poziomie; §4.2
dokłada 0,5 m w pionie i to §4.2 jest wiążące). Czyli kondygnacja 3 m to **6 voxeli, nie 3**,
a `clip_plane_z` M1 przyjmuje jednostki 0,5 m. `floor_heights_dm` z M2 jest w decymetrach,
więc konwersja to `half_meters = dm / 5` — jedno miejsce w kodzie, obudowane testem
`cut_plane_hits_slab`: dla budynku o kondygnacjach 4,5 m / 3,0 m / 3,0 m cięcie `Level(1)`
i `Level(2)` musi trafić dokładnie w strop, nie 1,5 m obok.

**Implementacja cięcia:** clip w fragment shaderze voxelowym (`discard` powyżej `world_y`)
+ **cap pass** domykający przekrój pełnym kolorem materiału. Bez cap passa przecięta ściana jest
pustą skorupą i widać wnętrze geometrii — wygląda jak dziura, nie jak przekrój architektoniczny.

**`InteriorKit` — wnętrza generowane, nie modelowane:**

```rust
pub struct InteriorKit { pub props: Vec<PropPlacement> }
pub struct PropPlacement { pub model: ModelId, pub pos: [f32; 3], pub yaw: u16, pub fill: u8 }

pub fn generate_interior(
    building: &BuildingGrammar,   // M2 — kondygnacje, siatka, typ użytkowania
    site: &SiteRenderRec,         // M11 — stock_fill, activity
    seed: u64,                    // deterministyczne rozmieszczenie (StreamId::Interior)
) -> InteriorKit;
```

Regały, linie produkcyjne, biurka, palety na rampie to **propy instancjonowane**, nie ręczne modele.
`stock_fill` steruje liczbą widocznych skrzynek na regale — gracz widzi pustkę w magazynie, zanim
otworzy panel. Cache: `InteriorCache` LRU 64 budynki, inwalidacja przy zmianie `stock_fill`
lub `activity` o > 10% (histereza — inaczej regenerujemy co klatkę).

**FPP nie ma własnej fizyki.** Postać gracza porusza się jak każdy inny agent; wejście gracza idzie
do sim jako komenda (M9), render tylko odczytuje `PlayerViewRec.eye` ze snapshotu i interpoluje.
To jest jedyny sposób, żeby tryb FPP nie stał się drugą, rozbieżną ścieżką ruchu.

W FPP: near plane 0,05 m, FOV 70°, model własnej postaci ukryty, LOD wymuszony na L0 w promieniu
15 m, `CutPlane::Box` wokół kamery, żeby ściana za plecami nie zasłaniała.


---

## Zmiany wpisane po M11a

Zgodnie z `K-18`. To są rzeczy, o których wiadomo **na pewno** po zamknięciu M11a;
podfaza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.
Pełna tabela `E-n` jest w `M11a-format-i-snapshot.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| G-1 ★ | **Budynek i zakład nadal nie są w buforze identyfikatorów i ta podfaza jest ich adresem.** Mieszkaniec i pojazd już są, a `Renderer::pick()` zwraca `PickHit { kind, entity }` z rodzajem w czterech starszych bitach | M11a wstawiła do bufora to, co rysuje instancingiem — a budynku i zakładu instancing nie rysuje: rysuje je pass chunków terenu. Wpisanie ich tam znaczy drugie wyjście koloru w shaderze chunka albo osobny przebieg po geometrii budynku, czyli pracę przy `CutPlane` i wnętrzach, czyli tę podfazę. Zapis „po M5e" dokumentu fazy wymienia `BuildingId` i `SiteId` imiennie i dopiero to go domknie; do tego czasu klient M5e szuka zakładu raycastem w teren w promieniu 25 m i dwa sklepy bliżej siebie są nierozróżnialne |
| G-2 | **`PickKind` ma dwa warianty (`Citizen`, `Vehicle`) i 28 bitów na indeks encji.** Rozszerzenie o budynek i zakład jest dopisaniem wariantu | Cztery bity rodzaju zostawiają czternaście wolnych wartości, a 28 bitów indeksu to 268 mln encji — trzy rzędy ponad metropolię. `decode_pick` odrzuca nieznany rodzaj zamiast zgadywać |
| G-3 ★ | **`PlayerViewRec` w snapshocie jest wyzerowany** i ta podfaza go wypełnia | To nie jest brak, tylko kolejność: pierwszym czytelnikiem `player.citizen` i `player.eye` jest tryb pierwszoosobowy, czyli WP5. Pole wypełnione, którego nikt nie czyta, wygląda w danych tak samo jak działające. Wypełniacz stoi w `magnat_game::view::SnapshotFiller` (`E-2`), a nie w `sim-snapshot` |
| G-4 | **`SiteRenderRec.sign_id` i `VehicleRenderRec.livery` są zerami** — atlas szyldów (WP9) jest ich pierwszym czytelnikiem | `sign_id` i `livery` przechodzą przez instancję jako `variant` i `anim`; kanał już działa, brakuje wyłącznie strony, która nada numery |
| G-5 | **`StreamId::Interior = 301` jest zarezerwowany imiennie** (`K-76` pkt 1) i to `generate_interior` go zajmie | Blok M11 (300–319) przydziela się fazie, a nie podfazie, więc numer został nadany razem z `Appearance = 300`. Wartość jest wieczna |

---

## Zmiany wpisane po decyzji właściciela produktu (2026-09-19)

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

Powód jest jeden i widać go na zrzucie z M11b: **miasto jest puste**. Podfaza M11b dała
postaciom klipy, sylwetki i poziomy detalu, a jedynym sposobem, żeby to zobaczyć, była scena
syntetyczna `--crowd`. W prawdziwym mieście w kadrze stoi kilkadziesiąt osób i zero aut,
a firm jest dziesięciokrotnie za mało, żeby ulica handlowa wyglądała jak ulica handlowa.
M11c miała pokazać „wejście do własnego sklepu z poziomu ulicy" — i pokazałaby je na pustej
ulicy, bo jej kryteria tego nie odróżniają.

| # | Zmiana | Dlaczego |
|---|---|---|
| J-1 ★ | **Podfaza przejmuje cztery pakiety: `WP12` (nowy), `R2-WP15`, `R2-WP17` i `R2-WP18`**, i idą one **przed** WP5 i WP9 | Kolejność jest treścią, nie porządkiem: kryterium WP5 („spacer FPP po sklepie bez artefaktów") przechodzi w martwym mieście tak samo jak w żywym, a kryterium WP9 („100 różnych szyldów w kadrze w jednym wywołaniu rysowania") jest przy obecnej gęstości firm niewykonalne z powodu, który nie ma nic wspólnego z rysowaniem — takiej liczby szyldów po prostu nie ma na czym powiesić |
| J-2 ★ | **`WP12` — nowy pakiet fazy M11**: okno warstwy Mikro i wycinek snapshotu idą za celem kamery, promień rysowania wynika z rozmiaru ekranowego encji, a pieszy idzie polilinią z grafu zamiast prostą | To są **trzy różne błędy dające ten sam objaw**, i wszystkie trzy leżą po stronie prezentacji, czyli tej fazy. Dwa pierwsze znalazły się dopiero wtedy, gdy ktoś policzył encje w kadrze zamiast patrzeć na obraz (`G-14` z M11b dołożyło ten licznik). Trzeci to pozycja 70 wykazu R2, której M11b naprawiła końce (`G-13`), a środek zostawiła |
| J-3 ★ | **`DRAW_RADIUS_M = 600` jest mniejszy od dystansu domyślnej orbity (900 m), a mierzy się od oka** — więc w domyślnym widoku gry nie widać **żadnej** encji, i to jest stan, w którym gra jest od M11a | Najpoważniejsza pojedyncza rzecz z całej czwórki: nie jest to „za mało ludzi", tylko „zero ludzi w widoku, od którego gra się zaczyna". Ujawniła to dopiero para liczb `snapshot:` i `stan renderu:` obok siebie — snapshot miał 32 rekordy, bufor instancji jeden |
| J-4 | **`R2-WP17` wchodzi razem z `R2-WP18`, choć sam nie jest widoczny w kadrze** | Nie da się ich rozdzielić: oba edytują `sim/world/src/city/sites/place.rs` i `sim/world/src/city/report.rs`, a WP17 zmienia skład zakładów w mieście — zrobiony po WP18 unieważniłby pomiar 1 : 40 i bilans etatów, czyli całe kryterium WP18. Przy okazji domyka rzecz, która sama w sobie jest martwa: **zero kopalń na złożu** znaczy, że model wyczerpywania złoża nigdy się nie uruchamia |
| J-5 | **Decyzja `D-N13` przyjęta wg propozycji domyślnej** (blokująca dla `R2-WP17`): profil wymagający wydobycia w regionie bez złóż nie dostaje zakładu, a łańcuch domyka import przez bramę | Pozostałe warianty łamią coś, co już stoi: „przesuń profil" daje świat inny, niż deklaruje wiersz poleceń, „dosiej złoże" łamie `K-13` (jedno źródło prawdy o terenie). Import przez bramę jest napisany, ma przepustowość, elastyczność ceny i cło |
| J-6 | **`R2-WP12` (wiek produkcyjny z danych) nie wchodzi — jest zamknięty od M8c** (`K-60`) | Druga zależność `R2-WP18` była już spełniona, zanim pytanie padło. Odnotowane, żeby nikt nie szukał go w tej podfazie |
| J-7 ★ | **Sufit `D-N6` pakietu `R2-WP18` obowiązuje bez zmian, również tutaj**: po dwóch dniach pakiet kończy się pomiarem i decyzją otwartą, a nie kodem, i przenosi się do R3 | Etap 7 generacji miasta jest jedynym miejscem w projekcie, w którym naprawa może wymagać przeprojektowania zamiast domknięcia. Przeniesienie pakietu do M11c nie jest powodem, żeby zdjąć mu sufit — jest powodem, żeby go przypomnieć, bo podfaza prezentacyjna jest gorszym miejscem na przebudowę generacji niż R2 |
| J-8 | **Wykaz R2 traci cztery pozycje z listy „do zrobienia w R2"** (5, 6, 12, 70, 71) i zyskuje przy nich adresata `M11c` | Reguła R2 mówi, że żadna pozycja nie kończy przeglądu bez statusu. „Przeniesiona z imiennym adresatem" jest statusem; „zrobiona gdzieś indziej po cichu" nie jest |
| J-9 ★ | **Godzina w kryterium WP12: 8:15 → 8:00.** Zmierzone na świecie odniesienia (`--seed 7 --size 4km --region lowland --epoch 1990 --profile mixed`): 7:50 → 4 644 mieszkańców i 552 pojazdy w snapshocie, 8:00 → 831 i 748, **8:15 → 74 i 209** | O 8:15 w całym mieście jest **114 podróży pieszych w toku**, bo praca zaczyna się o ósmej i szczyt trwa od 7:40 do 8:10. Przy takiej liczbie próg „200 mieszkańców w kadrze" jest nieosiągalny **niezależnie od renderu**, a WP12 ma jawnie nie ruszać ani udziału środków transportu, ani rozkładu wyruszeń. Kryterium mierzyło więc porę doby, a nie naprawę |
| J-10 ★ | **Tolerancja „0,5 m od terenu" zastąpiona odchyleniem od niwelety.** Zmierzone: trasa piesza odchyla się od terenu naturalnego o **3,90 m**, odcinek prosty między tymi samymi końcami o **11,90 m**; 93 % punktów trasy mieści się w 1 m | `RoadNode.z_dm` jest **rzędną niwelety**, a nie wysokością terenu, i to jest zamierzone: droga w przekroju podłużnym jest cięciwą między swoimi końcami, a różnicę pokrywa nasyp albo wykop. Pieszy idący chodnikiem po nasypie **ma** być trzy metry nad terenem naturalnym. Kryterium w pierwotnym brzmieniu nie mogło przejść przy poprawnej implementacji |
| J-11 | **Czwarta przyczyna pustego kadru, której `J-2` nie wymieniał: pojazdy w warstwie Mikro stały w punkcie `[0, 0, 0]` przez całą minutę.** `VehicleBuffer::end_feed` zamieniał bufory, ale nie wołał `place` — pozycje w metrach liczył dopiero `step`, a ten idzie **przed** zasileniem. Po poprawce: 748 pojazdów w snapshocie i 489 narysowanych tam, gdzie wcześniej było zero | To jest dokładnie ten kształt usterki, który opisuje `J-3`: rekord w snapshocie **był**, tylko wskazywał róg mapy. Bez pary liczb „snapshot" i „stan renderu" obok siebie wyglądało to jak brak danych z symulacji. Warstwa prezentacji ma być poprawna po każdej operacji, a nie tylko po kroku |
| J-22 ★ | **Szyld dostaje każda firma miasta, a nie tylko firma gracza, i wisi nad wejściem, nie nad dachem.** Kafel jest **per nazwa firmy**, więc sieć z trzema sklepami ma wszędzie ten sam napis; atlas mieści 255 nazw, a miasto 4 km ma ich około dwustu | Kryterium WP9 mówi „sto różnych szyldów w kadrze w jednym wywołaniu rysowania" — przy samych firmach gracza takiej liczby nie ma skąd wziąć w pierwszej godzinie gry, a ulica handlowa bez szyldów nie wygląda jak ulica handlowa (to jest dokładnie powód, dla którego `J-1` wpuścił tu cztery pakiety z `R2`). Sufit jest nazwany: po wyczerpaniu atlasu firma zostaje **bez szyldu**, a nie z cudzym |
| J-21 | **Krój szyldów jest bitmapowy 5 × 7 i mieszka w `engine/render`, a nie pochodzi z atlasu `egui`.** Polskie znaki mają własne glify, znak spoza tablicy rysuje się jako spacja | `engine/ui` nie ma własnego kroju — jest warstwą nad `egui`, a atlas `egui` żyje wewnątrz `egui_wgpu::Renderer` i wychodzi z niego wyłącznie jako `TexturesDelta`. Wiązanie cyklu życia szyldów w świecie z cyklem życia atlasu interfejsu kosztowałoby więcej niż czterdzieści pięć glifów po siedem bajtów. Przy szyldzie oglądanym z kilkunastu metrów, w świecie z voxela 0,25 m, krój wektorowy i tak byłby niewidoczny. Ścieżka wyjścia zapisana w kodzie: prawdziwy krój, gdy M12 doda drugi alfabet |
| J-20 ★ | **`PickKind` dostaje `Site`, ale zakład jest klikalny **przez swoje rzeczy, nie przez mury**: przez szyld i przez wyposażenie wnętrza.** `G-1` zapowiadał, że M11c jest adresem budynku i zakładu w buforze identyfikatorów — spełnione jest to dla zakładu, nie dla dowolnej ściany | Wierzchołek chunka ma **osiem bajtów** (`x/y/z/ao/normal` w jednym słowie, `material/sun` w drugim) i nie ma w nich miejsca na `BuildingId`; jedyny nośnik tożsamości w tym passie to szesnastobitowy numer materiału, wspólny dla wszystkich budynków z tej samej cegły. Dołożenie identyfikatora to zmiana **formatu siatki chunka**, czyli własności M1, i kosztuje pamięć wszystkich chunków terenu po to, żeby użył jej jeden pass. Szyld i regał są encjami instancjonowanymi, więc niosą identyfikator za darmo — a to jest ta sama odpowiedź, którą gracz uzna za poprawną: klika w sklep, nie w mur |
| J-19 ★ | **Prepass głębi musiał się nauczyć cięcia — bez tego przekrój jest czarną dziurą.** `voxel.wgsl` odrzucał fragmenty powyżej rzędnej od M1, ale prepass biegł **bez shadera fragmentu** i zapisywał głębię całej bryły. Niebo przegrywało wtedy z głębią nieistniejącego dachu, a czapka domykająca przegrywała porównaniem | To jest usterka M1, której nikt nie widział, bo **nikt nigdy nie ustawił `clip_plane_z`** — cięcie poziomami istniało jako uniform i jako `discard`, i nie miało ani jednego wołającego. Koszt naprawy: drugi potok prepassu z pustym shaderem fragmentu, używany **wyłącznie przy aktywnym przekroju**, żeby zwykła klatka nie straciła tego, na czym polega cały prepass |
| J-18 ★ | **Czapka przekroju jest pierścieniem ściany, a nie płytą na całym obrysie.** Szerokość pierścienia to pół metra — voxel M1 ma metr w poziomie, a cieńszy pierścień znikałby między voxelami | Płyta na całym obrysie domyka sylwetkę (czyli spełnia `cut_plane_has_no_holes` z §7.3) i **zakrywa wnętrze**, czyli to, po co gracz w ogóle tnie budynek. §5.7 mówi „cap pass domykający przekrój pełnym kolorem materiału" i nie rozstrzyga, ile tego przekroju jest — rozstrzyga to obraz: przekrój architektoniczny pokazuje mur w przecięciu, a nie strop nad piętrem |
| J-17 ★ | **`generate_interior` nie bierze `BuildingGrammar`, tylko `InteriorSpec` — płaski opis kondygnacji składany przez klienta.** Sposób użytkowania kondygnacji bierze się z lokali M2 (`Unit.kind`), a nie z gramatyki | Sygnatura z §5.7 jest **niewykonalna**: `BuildingGrammar` mieszka w `sim/world`, a `engine/render` nie ma prawa zależeć od żadnego `sim/*` poza `sim-snapshot` (§6.3 pkt 1) — pilnuje tego `cargo tree` w CI. Treść się nie zmienia: te same kondygnacje, ten sam obrys, ten sam podział na klatkę i powierzchnię użytkową. Zmienia się to, **kto składa opis** — i jest to ten sam ruch, którym `E-2` przeniosło wypełniacz snapshotu do `magnat_game::view` |
| J-16 ★ | **`R2-WP18` kończy się pomiarem i decyzją otwartą `D-N20`, a nie kodem — sufit `D-N6` zadziałał.** Zmierzone (4 km, `industrial`, ziarno 1): 24 800 mieszkańców, 199 firm (1 : 125), **4 186 lokali użytkowych wobec 202 zakładów**, 17 116 etatów przy 24 800 mieszkańcach, `it_office` 183 etaty na zakład | Obie przyczyny z `R2d` zachodzą i obie wychodzą z jednego założenia: **zakład bierze cały budynek**. Premises w mieście są, brakuje mechanizmu, który wsadzi do nich osobne firmy — a jego dołożenie to nowy pass Etapu 7 po `Unit` zamiast po parceli, zmiana `SiteSet.by_building` z odwzorowania jeden-do-jednego i pięciokrotny wzrost liczby firm, czyli wydajność i bilans pieniądza. `D-N6` przewidywał dokładnie ten wynik i dlatego istnieje |
| J-15 ★ | **Drugie kryterium `R2-WP17` („przebieg pięćdziesięcioletni kończy się szybem zamkniętym z powodu wyczerpania") zastąpione pomiarem bilansu wydobycia.** Test `kopalnia_zuzywa_zloze` sprawdza, że po trzech dobach `DepositLedger` pokazuje niezerowe wydobycie na złożach, na których stoją szyby; zmierzone: 2 szyby, 47,5 t | Kopalnia w mieście 4 km stoi w skali minimalnej, bo taki jest lokalny popyt, a złoże wystarcza wtedy na stulecia — wyczerpanie w pięćdziesiąt lat jest nieosiągalne **przy poprawnej implementacji**, więc kryterium mierzyłoby wielkość miasta, a nie naprawę. Istotą zarzutu było „model wyczerpywania złoża nigdy się nie uruchamia", i dokładnie to ten pomiar rozstrzyga |
| J-14 ★ | **Przyczyna zera kopalń jest inna, niż zapisał `R2d`.** Wypełniacz stref nigdy ich nie stawiał (archetypy z `needs_deposit` mają `weight: 0`, a wypełniacz losuje wagą). Bilans popytu **zamawiał** szyb gazowy i wiertnię ropy, ale `znajdz` szukało działki w strefie `Extraction`, a złoża pod miastem były inne (kruszywo, glina, węgiel); reszta kopalń odpadała na progu `SCALE_MIN`. Po poprawce: `Coastal` 1, `Mountain` 4, `Lowland` 2, `River` 3, `Desert` 3 zakłady wydobywcze, wszystkie na złożu | Wykluczenie z wypełniacza zostaje, bo reguła ma być jawna, ale samo niczego nie naprawiało — kryterium byłoby spełnione **zbiorem pustym**, czyli dokładnie tak, jak `K-18` pkt 3 zabrania. Zniesienie progu skali dla zakładu **stojącego na złożu** jest decyzją: złoże jest zasobem lokalnym, więc szyb na nim opłaca się nawet wtedy, gdy import byłby możliwy — a nadwyżka ma dokąd pójść, bo eksport przez bramę istnieje od M6d. Zmierzona cena: domknięcie łańcuchów bez zmian, import z 32 towarów spada do 31 |
| J-13 ★ | **Front działki wybiera się wśród najbliższych odcinków *z chodnikiem*, a chodnik zostaje przy odcinku, przy którym stoi parcela.** Bez pierwszej połowy kwartał przylegający do drogi szybkiego ruchu adresuje się do niej; bez drugiej `build_nav` kończy się `ParcelUnreachable` i miasto się nie stawia. Zmierzone po poprawce: `Coastal`, `Mountain`, `Lowland`, `Desert` — **zero** odcinków szybkiego ruchu z chodnikiem; `River` — dziewięć, same wielkopowierzchniowe działki rolne i przemysłowe na obrzeżu | `R2c` obiecywał, że „przejścia zostają w węzłach, więc sieć się nie rozpada", i to jest prawda o **sieci**, ale nie o **parceli**: działka, której jedynym sąsiadem jest autostrada, traci dojście w ogóle. Kryterium WP1 („każda parcela z frontem ma dojście pieszo") jest twardsze niż czystość klasyfikacji, a odcinek z zabudową u frontu nie jest obwodnicą w żadnym sensie poza etykietą. Flaga siedzi na odcinku, więc jedna fabryka udrażnia całe czterysta metrów — rozdrabnianie odcinka pod działkę to urbanistyka M2, nie graf, i dlatego kryterium liczy **odcinki**, a nie metry |
| J-12 | **Poza kwadransami szczytu ulica jest pusta i to nie należy do tej fazy.** Zmierzone na świecie odniesienia: 7:50 → 4 644 pieszych, 8:00 → 831, 8:15 → 74, **10:00 → 0**, 14:00 → 2 031, 16:00 → 1 919, 16:30 → 129, **17:00 → 0**, 20:00 → 0 | Plan doby wysyła wszystkich w tej samej minucie, więc miasto ma trzy piki i dwadzieścia godzin ciszy. To jest kształt modelu z M3, a nie renderu — WP12 jawnie nie rusza rozkładu wyruszeń (§„Czego ten pakiet nie robi"). Nowa pozycja wykazu `R2` z adresatem, bo „żywe miasto" z §1 dokumentu fazy będzie jej potrzebowało |

### Co to zmienia w rozmiarze podfazy

Było: `L + S`. Jest: `M + S + M + L + L + S`. Podfaza rośnie mniej więcej dwukrotnie i staje się
najdłuższą w fazie M11 — to jest cena, którą właściciel produktu przyjął świadomie, wybierając
wariant „pełne żywe miasto". Pakiet `R2-WP18` ma przy tym własny sufit czasu (`J-7`), więc
w najgorszym razie kończy się pomiarem, a nie rozlaniem podfazy.
