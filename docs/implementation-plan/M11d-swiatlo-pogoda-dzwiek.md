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

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP6 | Oświetlenie nocne i blackout | WP2, M1, M8 | M |
| WP7 | Pogoda wizualna i dym | WP2, M8 | M |
| WP8 | `engine/audio` | WP2 (tylko `SiteRenderRec`) | L |

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
