# M11c — Wnętrza i kamera FPP

Podfaza 3 z 5 fazy **M11 — Prezentacja** (`M11-prezentacja.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M11a (snapshot), M2 (wnętrza logiczne), M9 (panele). |
| **Pakiety robocze** | WP5, WP9 |
| **Projekt techniczny** | §5.7 |
| **Wynik do pokazania** | Wejście do własnego sklepu z poziomu ulicy; szyld firmy gracza widoczny z zewnątrz. |
| **Kryterium zamknięcia** | Kryteria WP5 i WP9. |
| **Poprzednia / następna** | `M11b-animacja-i-lod.md` · `M11d-swiatlo-pogoda-dzwiek.md` |

Kamera FPP, `CutPlane` do cięcia poziomami, `InteriorKit` oraz szyldy i barwy firm gracza.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP5 | Kamera FPP, `CutPlane`, `InteriorKit` | WP2, M2, M9 | L |
| WP9 | Szyldy i barwy firm gracza | WP1, M9 | S |

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
