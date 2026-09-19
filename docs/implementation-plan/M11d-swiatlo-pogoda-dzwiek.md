# M11d — Światło, pogoda, dźwięk

Podfaza 4 z 5 fazy **M11 — Prezentacja** (`M11-prezentacja.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M11a (snapshot), M1 (klimat), M8 (pogoda, blackout). |
| **Pakiety robocze** | WP6, WP7, WP8 |
| **Projekt techniczny** | §5.8, §5.9 |
| **Wynik do pokazania** | Noc, deszcz, dym z komina i warstwa dźwiękowa reagująca na stan świata, nie na skrypt. |
| **Kryterium zamknięcia** | Kryteria WP6–WP8. |
| **Poprzednia / następna** | `M11c-wnetrza-i-kamera.md` · `M11e-budzet-klatki.md` |

Oświetlenie nocne z blackoutem, pogoda wizualna i dym oraz crate `engine/audio`.

---

## Pakiety robocze

| | WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|---|
| [x] | WP6 | Oświetlenie nocne i blackout | WP2, M1, M8 | M |
| [x] | WP7 | Pogoda wizualna i dym | WP2, M8 | M |
| [x] | WP8 | `engine/audio` | WP2 (tylko `SiteRenderRec`) | L |

### WP6 — Oświetlenie nocne i blackout

**Opis.** Wypełnianie listy świateł punktowych dla clustered shadingu M1 z danych snapshotu:
okna budynków (`SiteRenderRec.lights`), latarnie, reflektory pojazdów. Latarnie dalej niż 150 m
przestają być światłami i stają się materiałem emisyjnym + wkładem do ambientu — inaczej
20 tys. latarni zabija cluster. Blackout: `PowerState.supply_ratio` poniżej progu wygasza dzielnicę
rampą 1,5 s (skok wygląda jak błąd renderu, nie jak awaria sieci).

**Kryterium ukończenia.** Scena `bench_night_rain` w budżecie; `bench_blackout` nie jest wolniejszy
niż `bench_night_rain` (czyli ścieżka „brak świateł" nie ma patologii); wizualnie widać granicę
zasilanej i niezasilanej dzielnicy.

### WP7 — Pogoda wizualna i dym

**Opis.** Deszcz/śnieg jako GPU particle w boxie wokół kamery (jeden draw call, wrapping —
nie symulujemy pogody nad całym miastem, tylko tam, gdzie kamera). Mgła wykładnicza wysokościowa
w shaderze. **Pory roku i śnieg na ziemi nie dotykają geometrii voxeli** — modyfikują paletę
materiału w shaderze. To jest świadoma decyzja: remeshing 4× w roku dla całego miasta byłby
absurdalnym kosztem za zmianę koloru. Dym z kominów: emiter cząstek, gęstość z `SiteRenderRec.emission`.

**Kryterium ukończenia.** Przejście przez cztery pory roku bez ani jednego remeshingu chunka
(licznik `chunk_remesh_count` = 0); deszcz przy 200 tys. cząstek ≤ 0,8 ms GPU; dym z 256 kominów
w jednym draw callu.

### WP8 — `engine/audio`

**Opis.** Nowy crate. Mikser czterech magistral (Ambient / World / UI / Music) z limiterem.
`AmbientZone` per dzielnica z łożem dźwiękowym zależnym od typu i pory dnia. `SoundEmitter`
przestrzenny z okluzją z raycasta voxelowego. **Klastrowanie głosów** — bez niego żywe miasto
to 5000 emiterów; z nim to 32 głosy. `MusicDirector` czytający stan finansów gracza.
„Linia stoi = cisza" to wprost `gain = f(SiteRenderRec.activity)`, gdzie `activity == 0 → gain == 0`.

**Kryterium ukończenia.** Testy z sekcji 7.4; odsłuch trzech dzielnic (przemysł, park, śródmieście)
daje rozpoznawalnie różne łoża; zatrzymanie linii produkcyjnej gracza słychać w ≤ 0,5 s.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.8 Oświetlenie, pogoda, dym

**Światła.** M1 jest właścicielem passa clustered shading; M11 **wypełnia listę świateł** ze snapshotu:

| Źródło | Dane | Reguła |
|---|---|---|
| Okna | `SiteRenderRec.lights` | budynek = 1 światło obszarowe, nie N okien; ~1 500 w widoku dzielnicy |
| Latarnie | `RoadNetwork.furniture`, `FurnitureKind::StreetLamp` (M2) | **< 400 m: światło punktowe. ≥ 400 m: materiał emisyjny + wkład do ambientu dzielnicy** |
| Reflektory | `VehicleRenderRec.flags` | tylko L0 i L1, cap 128 |

**Korekta budżetu po uzgodnieniu z M1: 256 to limit NA KLASTER, nie globalny.** Froxele 16×9×24 =
3 456 klastrów, globalnie **≤ 4 096 aktywnych świateł na klatkę** (≤ 0,4 ms GPU na przypisanie).
Pierwotny próg 150 m wyliczyłem przy błędnym założeniu 256 świateł globalnie i był o rząd
wielkości zbyt ostrożny — nocne miasto byłoby ciemniejsze, niż musi.

Przeliczenie na realnej gęstości: latarnie co ~30 m po obu stronach jezdni to ~66 sztuk na kilometr
drogi. Przy budżecie 4 096, z czego ~1 500 zabierają okna i ~128 reflektory, na latarnie zostaje
~2 400 → ok. **36 km widocznej drogi**, co pokrywa widok dzielnicy z zapasem. Stąd **próg 400 m**.
Dokładna wartość jest kalibrowana pomiarem w WP6 na `bench_night_rain` — gęstość latarni zależy od
`lamp_spacing_m[class]` z M2, więc to jest pokrętło do strojenia, nie stała z tabeli.

**Redukcja zostaje po stronie producenta listy (M11), nie w passie M1.** Przepełnienie: `assert`
w debug, w release obcięcie po malejącym dystansie — nigdy losowe, żeby nocna scena nie migotała
przy obrocie kamery.

**Prawdziwym ograniczeniem jest zajętość klastra, nie budżet globalny (ostrzeżenie M1).** Koszt passa
opaque rośnie z liczbą świateł **na klaster**, a nie z sumą. 2 400 latarni rozrzuconych po scenie jest
tanie; te same 2 400 wzdłuż jednej prostej arterii skupia się w kilkunastu klastrach i limit 256
wyczerpuje się tam **wcześniej niż globalne 4 096**. Wniosek: sam próg odległości nie wystarcza,
bo jest ślepy na gęstość.

Dlatego reguła latarni ma drugi stopień: przy przekroczeniu **192 świateł w klastrze** latarnie w tym
klastrze przechodzą na materiał emisyjny **co drugą**, potem co czwartą — przerzedzanie zamiast
obcinania ogona, bo równomiernie rzadszy rząd latarni wzdłuż alei czyta się jak rzadsze latarnie,
a ucięty ogon jak ciemna dziura w połowie ulicy. Objaw do rozpoznania w WP6: skok czasu passa opaque
przy obrocie kamery **wzdłuż** arterii przy niezmienionej liczbie globalnej — to jest zajętość
klastra, nie budżet. **M1 wystawia licznik zajętości klastrów w devtools** i to on, a nie suma świateł,
jest przyrządem do kalibracji progu 400 m na `bench_night_rain`.

**Blackout.** `PowerRec.supply_ratio < 128` → wygaszenie dzielnicy rampą **1,5 s**
(`gain *= smoothstep`). Skokowe zgaszenie czyta się jako błąd renderu; rampa czyta się jako awaria.

**Pogoda:**

| Efekt | Implementacja | Koszt |
|---|---|---|
| Deszcz / śnieg | GPU particle w boxie 60×40×60 m wokół kamery, wrapping, 200 tys. cząstek, 1 draw call | ≤ 0,8 ms |
| Mgła | wykładnicza wysokościowa w shaderze, fullscreen; dodatkowo nocna nad wodą | ≤ 0,2 ms |
| Śnieg na ziemi | **blend palety materiału w shaderze wg `normal.y` i `snow_cover`** | 0 ms |
| Pory roku (liście) | **przesunięcie palety materiału `Foliage` wg `season`** | 0 ms |
| Kałuże / błoto | maska wilgotności z `precipitation`, wygaszana po opadzie | 0 ms |
| Dym z kominów | emiter cząstek, gęstość = `emission`, 64 cząstki/emiter, cap 256 emiterów, 1 draw call | ≤ 0,3 ms |

**Pory roku nie dotykają geometrii voxeli.** Remeshing całego miasta cztery razy w roku gry za
zmianę koloru liści byłby kosztem bez pokrycia. Sezon to przesunięcie palety — i wygląda tak samo.

**Sezon liczy się z `SimCalendar` (K-1), nie z własnej arytmetyki dat:** rok ma 360 dni,
12 miesięcy × 30, więc pora roku to dokładnie 90 dni, a granice sezonu są ostre i identyczne
w każdym roku gry. Brak lat przestępnych oznacza, że krzywa `day_curve` w `AmbientZone`
i krzywa wysokości słońca (M1) mogą być tablicowane raz na 360 dni bez przypadków szczególnych.

**Render nie odpytuje terenu i nie liczy o nim niczego (K-13).** `TerrainQuery` żyje w `sim/world`,
a `engine/render` nie może od niego zależeć — wołanie go stąd wywróciłoby test `dep_isolation`
uzgodniony z M1. W praktyce okazało się, że nie jest potrzebny nigdzie w tej fazie:

| Co wyglądało na potrzebę terenu | Skąd naprawdę pochodzi |
|---|---|
| Śnieg i błoto na ziemi | Shader terenu, blend palety wg `normal.y` już zrasteryzowanej geometrii |
| Osadzenie propów `InteriorKit` | `Building.aabb` + `floor_heights_dm` (M2) — wnętrza są w budynku, nie na gruncie |
| Kamera FPP | `CameraMode::FirstPerson{anchor}` (M1), pozycja encji ze snapshotu |
| Bloki impostorów dzielnic | Rzeczywista geometria chunków renderowana do atlasu |
| Daleki teren za 4 km | Heightmapa jako zwykły bufor w snapshocie + pass `far_terrain` (M1) |

Gdyby cokolwiek w przyszłości potrzebowało wysokości terenu per encja, **wnosi to
`fill_render_snapshot`** — ono biegnie po stronie sim i `TerrainQuery` ma pod ręką.

### 5.9 `engine/audio`

```rust
pub struct AudioEngine {
    buses:    [Bus; 4],              // Ambient | World | Ui | Music, każdy z gain + limiter
    voices:   VoicePool,             // 32 głosy światowe + 8 UI + 4 stemy muzyki
    zones:    Vec<AmbientZone>,
    emitters: Vec<SoundEmitter>,
    music:    MusicDirector,
    listener: Listener,              // pozycja = kamera; w FPP = głowa postaci
}

pub struct SoundEmitter {
    pub pos:       [f32; 3],
    pub source:    SoundSourceId,    // klucz do data/audio/
    pub gain:      f32,
    pub pitch:     f32,
    pub radius:    (f32, f32),       // (pełna głośność, cisza)
    pub occlusion: f32,              // raycast voxelowy, odświeżany 4 Hz — nie 60 Hz
    pub looping:   bool,
    pub owner:     Option<u32>,      // entity_lo zakładu — do „linia stoi = cisza"
}

pub struct AmbientZone {
    pub district:  DistrictId,
    pub bed:       AmbientBedId,     // Industry | Traffic | Park | Residential | Retail | Port
    pub weight:    f32,              // udział w mixie z odległości kamery do centroidu
    pub day_curve: CurveId,          // to samo miejsce brzmi inaczej o 3:00 i o 17:00
}
```

**Klastrowanie głosów — bez tego nie ma żywego miasta.** Emitery o tym samym `SoundSourceId`
w promieniu **15 m** łączą się w jeden wirtualny emiter w centroidzie, z gainem
`sum.min(single * 2.5)` (nie liniowo — 20 ciężarówek nie jest 20× głośniejsze niż jedna).
Bez tego 500 pojazdów na skrzyżowaniu to 500 głosów; z tym to 3.

Priorytet głosu: `gain × (1 − occlusion) / (1 + dist²)`. Przy wyczerpaniu puli głos o najniższym
priorytecie jest wygaszany rampą 120 ms (nie ucinany — trzask jest gorszy niż brak dźwięku).

**„Linia stoi = cisza" (§15.5)** to wprost:

```rust
emitter.gain = base_gain * (site.activity as f32 / 255.0);   // activity == 0 → gain == 0
```

Zatrzymanie linii słychać, zanim gracz otworzy panel. To jest kanał informacyjny, nie ozdoba.

**Muzyka adaptacyjna:**

```rust
pub enum MusicMood { Rozwoj, Stabilnosc, Napiecie, Kryzys }
```

Wyprowadzana z `PlayerViewRec.liquidity_ratio` (płynność / 30-dniowe koszty stałe) i `profit_trend`.
Warstwy (stemy) crossfadowane **wyłącznie na granicy taktu**, przejście 2 takty, **histereza ±15%** —
inaczej przy płynności oscylującej wokół progu muzyka miga. Muzyka nigdy nie przeskakuje w środku frazy.

**Audio nie czyta ECS.** `AudioEngine::update(&RenderSnapshot, &CameraState)` — ta sama zasada
co render, ten sam mechanizm egzekwowania (sekcja 6.3).


---

## Zmiany wpisane po M11a

Zgodnie z `K-18`. To są rzeczy, o których wiadomo **na pewno** po zamknięciu M11a;
podfaza nie jest tu przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.
Pełna tabela `E-n` jest w `M11a-format-i-snapshot.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| H-1 ★ | **`weather`, `power` i `sites` w snapshocie są wyzerowane i ta podfaza je wypełnia.** Typy istnieją i mają ustalone rozmiary; brakuje wyłącznie wypełniacza | Pierwszym czytelnikiem każdego z nich jest coś, co rysuje albo gra: dym, okna, blackout, łoże dźwiękowe. Pole wypełnione, którego nikt nie czyta, wygląda w danych tak samo jak działające — stąd kolejność. Wypełniacz stoi w `magnat_game::view::SnapshotFiller` (`E-2`) i dokłada się do niego metodą na sekcję, a nie drugą funkcją |
| H-2 | **`LightRecord` zostaje w kształcie M1** (`pos`, `range`, `color_rgbe`), bez pól `kind` i `flags` z §5.2 (`E-6`) | Ten sam rozmiar 20 B, a wersja M1 ma działającego konsumenta (`clusters::to_gpu`). Rozróżnienie okno / latarnia / reflektor dokłada ta podfaza razem z pierwszym miejscem, które je czyta — i wtedy dopiero zajmuje bajty |
| H-3 | **`PowerRec` ma 1 B i nie niesie numeru dzielnicy** (`E-5`): tablica jest indeksowana dzielnicą, więc indeks jest tożsamością | Pole `district` mogło mieć wyłącznie wartość własnego indeksu, a `u16` obok `u8` dawał 4 B rekordu i 256 B tablicy zamiast obiecanych 192 |
| H-4 | **`SiteRenderRec.flags` niesie `SITE_FAULT` na bicie 0 i rodzaj pióropusza na bitach 1–2**, z akcesorami `is_faulted()` i `plume_kind()` | Kodowanie jest już w typie, więc wypełniacz i shader nie wyprowadzają go osobno. Pięć bitów zostaje wolnych |
| H-5 | **Barwy z palet są w sRGB i shader instancji przeliczy je na liniowe** (`pow 2,2`) przed oświetleniem | Bufor sceny jest HDR i liniowy, bo ekspozycja i tonemap dzieją się w post-processingu (M1). Emitery świateł mają wejść **tą samą drogą**, inaczej latarnia i okno tego samego budynku będą miały dwie różne barwy przy identycznym wpisie w danych |
| H-6 | **Rola palety `Emissive` ma numer 11 i shader traktuje ją osobno** — element nią oznaczony nie gaśnie w cieniu | To jest cała różnica między lampą a blachą i jest już w kodzie. Blackout ma więc gdzie zadziałać: wygaszenie jest mnożnikiem tej jednej roli, a nie osobnym passem |

---

## Zmiany wpisane po M11d

Zgodnie z `K-18`. To są rzeczy, o których wiadomo **na pewno** po zamknięciu M11d.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| I-1 ★ | **Pojemność klastra świateł to 64, nie 256** (`MAX_LIGHTS_PER_CLUSTER` w `engine/devtools`), więc próg przerzedzania 192 z §5.8 leżał **powyżej** sufitu i nie mógł się nigdy zapalić. Próg jest teraz 48 (trzy czwarte pojemności), z powrotem przy 32 | Korekta budżetu z §5.8 mówiła „256 na klaster" na podstawie decyzji 9.1, a kod M1 stanął na 64 i nikt tych dwóch liczb nie porównał. Globalne 4 096 zgadza się bez zmian, więc próg 400 m zostaje |
| I-2 ★ | **Przerzedzanie latarni jest globalne, sterowane szczytem zajętości klastrów z poprzedniej klatki**, a nie liczone per klaster | §5.8 opisuje przerzedzanie „w tym klastrze", co wymagałoby **czwartej** kopii arytmetyki froxeli po stronie producenta listy — a nagłówek `clusters.rs` ostrzega wprost, że trzy kopie już są i rozjeżdżają się przy pierwszej zmianie wymiarów siatki. Przyrządem jest `ClusterOccupancy`, czyli licznik, który M1 wystawił dokładnie w tym celu (§5.8 zdanie ostatnie). Efekt wizualny jest ten sam — równomiernie rzadszy rząd latarni — tylko obejmuje cały kadr, a nie jeden froxel |
| I-3 | **`LightRecord` nadal nie ma pól `kind` i `flags`**, które `H-2` zapowiadało „razem z pierwszym czytelnikiem" | Czytelnik się nie pojawił i to jest wynik, nie zaniechanie: blackout gasi dzielnicę po stronie **producenta** listy, bo tylko on zna dzielnicę, a przerzedzanie idzie krokiem po liście, nie po rodzaju światła. Rekord zostaje 20-bajtowy, a snapshot w budżecie |
| I-4 | **`SnapshotFiller.sites` niesie dzielnicę** — docstring obiecywał ją od M11a, a krotka miała dwa pola | Bez dzielnicy okno zakładu nie wie, czy ma prąd, więc blackout nie miałby czego wygasić |
| I-5 ★ | **Usterka sprzed tej podfazy: wypełniacz szukał zakładu pod numeracją generatora, a `Plant` jest kluczowany kluczem przesuniętym** (`K-46`). `plant.get` nie trafiał **nigdy**, więc każdy zakład od M11c miał `activity = 128` i `stock_fill = 255` | To są wartości domyślne `fill_sites` i wyglądają dokładnie jak prawdziwe — dlatego nikt tego nie zobaczył. Objawiło się dopiero tutaj, bo dym z komina i „linia stoi = cisza" stoją na tych liczbach. Poprawione w `game::view`, `game::ambience` oraz po stronie klienta w `signs.rs` i `interiors.rs`; po naprawie 183 z 254 zakładów pracuje, a nie 254 |
| I-6 ★ | **`SiteRenderRec.emission` liczy się z receptury pracującej linii, a nie z `EmissionTotals::pm_g_last_minute`** | Tamto pole zeruje się co minutę i napełnia dopiero w minucie **zakończenia szarży**, więc snapshot próbkujący je co klatkę trafia w zero prawie zawsze: komin dymiłby przez jedną klatkę raz na pół godziny gry. Kontrakt §5.2 mówi „gęstość", czyli tempo — i tempo liczymy z `emissions.pm_g / duration_minutes` receptury. Sufit nazwany w kodzie: tempo jest nominalne, obniżona szarża dymi tak samo jak pełna |
| I-7 ★ | **Widoczność pióropusza zależy od `plume_kind` i `activity`, nie od samego `emission`** | Chłodnia kominowa i mleczarnia mają `PlumeKind::Steam` i **zerowy pył**, bo para nie jest zanieczyszczeniem. Reguła: rodzaj mówi, co leci, `activity` — czy cokolwiek leci, `emission` — jak gęsto. Bez tego połowa kominów w mieście byłaby niewidoczna mimo pracy |
| I-8 ★ | **Waga łoża dzielnicy bierze się z encji wokół słuchacza, a nie z odległości do centroidu dzielnicy**, jak zapisywało §5.9 | Centroidu dzielnicy w snapshocie nie ma i nie będzie — kanał sim → render niesie encje i tablice per dzielnica, a nie geometrię miasta. Wychodzi na to samo i jest uczciwsze: dzielnica przemysłowa brzmi maszynami dlatego, że stoją w niej maszyny |
| I-9 ★ | **`AudioEngine::update` bierze `&Listener`, a nie `&CameraState`**, jak zapisywało §5.9 | `CameraState` mieszka w `engine/render`, a §6.3 pkt 1 zabrania `engine/audio` zależeć od czegokolwiek poza `engine-core` i `sim-snapshot`. Ten sam ruch, którym `Y-4` przestawiło `generate_interior` na `InteriorSpec`. Słuchacza składa klient, bo to on ma kamerę |
| I-10 | **Okluzja jest terenowa, nie voxelowa** — sufit nazwany w kodzie komentarzem `ponytail:` | Prawdziwy raycast po voxelach wymaga zmaterializowanych chunków, których klient trzyma tylko wokół kamery; emiter poza tym podzbiorem dostałby zero i tak. Ścieżka wyjścia: przecięcie odcinka z `Building.aabb` przez `CsrGrid::query_rect`, czyli indeks, który miasto już ma |
| I-11 ★ | **`data/audio/` nie ma nagrań: `audio.ron` opisuje syntezę** (decyzja właściciela produktu z 2026-09-19) | Ten sam wzorzec co `mvoxc gen` w M11a i `Y-6` w M11c. Kryterium „odsłuch trzech dzielnic daje rozpoznawalnie różne łoża" jest wtedy **mierzalne bez ucha**: test porównuje energię w pasmach, bo to w nich siedzi różnica między rumorem hali a szelestem liści. Szczegóły kontraktu — `K-77` |
| I-12 | **`PASS_NAMES` rośnie do siedmiu, a `weather` dopisany jest na końcu**, choć pass rysuje się przed `post` | Indeksy znaczników czasu są pozycyjne, więc wstawienie w środku przesunęłoby każdy wcześniejszy pomiar — a wtedy porównanie z baseline'em mierzyłoby przenumerowanie, nie zmianę kodu. Wiążące dla M11e, które z tych liczb robi raport |
| I-13 | **`FrameUniform` dostaje `weather: vec4`** (śnieg, wilgoć, sezon, zachmurzenie) i **wszystkie osiem kopii `struct Frame` w WGSL** dostaje to pole razem z testem, który tego pilnuje | Kopia, która zgubi pole, czyta cudze bajty i jest przy tym poprawnym shaderem — walidator `naga` tego nie widzi. Test `kazda_kopia_uniformu_ramki_ma_te_same_pola` porównuje listę pól we wszystkich ośmiu |
| I-14 | **`chunk_remesh_count` jest licznikiem w strumieniowaniu klienta i mierzy się go w oknie, nie w CI** | Kryterium WP7 mówi o przejściu przez cztery pory roku, a to wymaga uruchomionej gry z GPU — §7.2 dopuszcza to wprost („progi klatkowe weryfikuje nocny bieg"). W CI stoi za to test, że pogoda w ogóle nie ma drogi do geometrii: wchodzi wyłącznie do uniformu ramki |
| I-15 | **Klient dostaje cztery przełączniki scen pomiarowych: `--precip`, `--snow`, `--blackout`, `--no-audio`** | Sceny odniesienia §7.2 nazywają się `bench_night_rain`, `bench_winter` i `bench_blackout`, a model pogody losuje opad i nie da się go poprosić o ulewę. Ta sama konwencja co `--lights` i `--crowd`: wymuszenie dotyczy **wyłącznie snapshotu**, więc scena nie zmienia ani grosza w świecie |
| I-16 | **Sezon w snapshocie liczy się z `world.tick`, a `--day` przestawia samo słońce** — `--snow` wymusza przy okazji zimę, żeby scena była spójna | Rozjazd jest sprzed tej podfazy i udokumentowany w kliencie („Dzień roku dla słońca. Symulacja liczy własną dobę od zera"), ale do M11d nie było go widać: dopiero paleta sezonowa stawia zieleń obok zimowego słońca. Przewinięcie świata o 170 dób kosztuje 245 tys. minut symulacji, więc scena wymusza sezon zamiast go przeżywać |
| I-17 ★ | **Słuchacz stoi w celu kamery, a nie w jej oku** | Ta sama poprawka, którą `X-3` zrobiło dla okna warstwy Mikro, i z tego samego powodu: przy orbicie z 900 m oko wisi nad miastem, więc emiter w promieniu 220 m od oka nie istnieje i widok dzielnicy byłby niemy mimo tysiąca ludzi w kadrze |
| I-18 ★ | **Pojazd ma zapalony silnik, a po zmroku światła** — do tej pory `VehicleRenderRec.flags` było twardym zerem | Reflektory z WP6 i dźwięk silnika z WP8 nie miały na czym stanąć: obie reguły czytają flagi, których nikt nie pisał. Warstwa Mikro trzyma wyłącznie pojazdy w podróży, więc „silnik pracuje” jest prawdą o każdym z nich; próg zapalenia świateł jest ten sam co dla latarni |

### Znalezione w recenzji przed commitem

Dziesięć poprawek z przeglądu tej samej zmiany. Żadna nie zmienia zakresu; wszystkie
dotyczą rzeczy, które **działałyby po cichu źle**.

| # | Zmiana | Dlaczego |
|---|---|---|
| I-19 ★ | **Mieszkaniec nie wchodzi do mieszanki łóż**; robią to zakłady (przez `ambient_kind`) i pojazdy (zawsze jako `Traffic`) | `CitizenRenderRec.district` niesie dzielnicę **zameldowania**, a nie tę, w której człowiek stoi — ważenie nią odległości znaczyło, że pas przemysłowy pełen dojeżdżających brzmi osiedlem. To jest dokładnie odwrotność kryterium WP8, a złapać dało się to tylko czytając, skąd bierze się `district` |
| I-20 ★ | **Bramka `dep_isolation` przepuszczała siedem z jedenastu crate'ów symulacji** i jest teraz listą **dozwolonych**, nie zakazanych | Wzorzec `magnat-(world\|city\|econ\|events\|sim)` nie łapał `magnat-economy` (po „econ" jest „omy"), `sim` nie łapał niczego, a `supply`, `agents`, `traffic`, `firms`, `macro` i `policy` nie były wymienione. Przy liście dozwolonych **nowy crate symulacji zapala bramkę sam** |
| I-21 | **`AudioEngine::new` nie panikuje już po utworzeniu miksera** — każdy `expect` zamieniony na propagację błędu | Konstruktor zwraca `Result` po to, żeby brak urządzenia dawał cichą grę. Panika za `AudioManager::new` łamała własny kontrakt crate'u dokładnie w sytuacji, do której ten `Result` był |
| I-22 ★ | **Tempo emisji liczy się w miligramach na minutę, nie w gramach** | `pm_g / duration_minutes` obcina w dół, więc **każda receptura emitująca mniej niż gram na minutę dawała zero** i jej komin był czysty. Ta sama pułapka co przy rampie blackoutu i przy paliwie (`K-25`): zaokrąglenie w jedną stronę nie znosi się |
| I-23 ★ | **Mgła sięga pełnej skali**: próg połowy zachmurzenia zamiast dzielenia przez trzy | Poprzedni wzór nie przekraczał 85 z 255, więc górne dwie trzecie skali były martwe, a kontrakt renderu obiecywał przy 255 widoczność 200 m — stan nieosiągalny. Mgła jest tak samo rzadka, ale kiedy jest, znaczy to, co obiecuje |
| I-24 | **`WeatherUniform.params.w` przestaje udawać daną**: było tam `snow_cover`, którego shader cząstek nie czytał | Śnieg na ziemi idzie **uniformem ramki** do shaderów terenu; cząstki opadu nic o nim nie wiedzą. Pole ustawiane i nieczytane wygląda w kodzie tak samo jak działające, więc lepiej, żeby jawnie było wyrównaniem |
| I-25 | **Pominięty pass pogody zeruje swoją pozycję w `pass_ms`** | Pass, który się nie odbył, nie stempluje pary znaczników, a `resolve_timer` rozwiązuje cały zakres — więc raport pokazywałby czas z klatki, w której pass był. `bench_blackout` mierzyłby wtedy pracę, której nie wykonano |
| I-26 | **Test kopii `struct Frame` porównuje kolejność pól, nie ich obecność** (i jest ich dziewięć, nie osiem) | O tym, które bajty trafiają do którego pola, decyduje wyłącznie pozycja. Kopia z przestawionymi polami przechodziła poprzednią wersję testu i czytała cudze bajty — czyli awaria, przed którą `I-13` miał chronić |
| I-27 | **Reguła śniegu i sezonu w `voxel.wgsl` i `far_terrain.wgsl` ma test porównujący obie kopie** | WGSL nie ma dołączania plików, więc duplikat jest nieunikniony — ale rozjazd byłby widoczny dokładnie na granicy pierścienia LOD, czyli tam, gdzie patrzy się najczęściej |
| I-28 | **Walidator katalogu dźwięku sprawdza klucze źródeł i warstwy stemów** | Przestawienie dwóch wierszy w `sources` zamieniłoby cicho dźwig na deszcz, a stem bez warstw wczytywał się i grał ciszę. Oba stany są nie do odróżnienia od brakującego pliku — czyli dokładnie to, przed czym walidator ma bronić |

Przy okazji poprawione bez osobnego wiersza: pętla szwu w syntezatorze omijała **każdy**
głos z warstwą szumu, czyli wszystkie siedem łóż, i nie dochodziła do ani jednej asercji
(sprawdza teraz stemy muzyki, które są czystymi tonami); zerowe ziarno xorshiftu jest
punktem stałym i uciszało prawy kanał; `wybierz_kominy` alokowało co klatkę mimo pola
`scratch` opisanego jako „klatka nie alokuje"; indeksowanie katalogu archetypów wywracało
wiązanie z miastem zamiast dać domyślne łoże; próg przerzedzania latarni dostał test
wiążący go z `CLUSTER_CAPACITY` po stronie klienta — jedynego crate'u, który widzi obie
liczby naraz.
