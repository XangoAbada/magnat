# M11e — Budżet klatki

Podfaza 5 z 5 fazy **M11 — Prezentacja** (`M11-prezentacja.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M11b–M11d. |
| **Pakiety robocze** | WP10 |
| **Projekt techniczny** | §5.10, §5.11 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: cele FPS z PRD §20.2 dotrzymane na maszynie referencyjnej. |
| **Kryterium zamknięcia** | Kryterium WP10 oraz bramki 1–7 fazy M11 w `00-postep.md`. **Decyzja właściciela produktu**: maszyna referencyjna dla „GPU klasy średniej (2024)” musi być nazwana przed pomiarem. |
| **Poprzednia / następna** | `M11d-swiatlo-pogoda-dzwiek.md` · — (ostatnia w fazie) |

`RenderBudget`, adaptacyjne LOD i benchmarki klatkowe.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP10 | `RenderBudget`, adaptacyjne LOD, benchmarki klatkowe | WP4, WP5, WP6, WP7 | M |

### WP10 — `RenderBudget`, adaptacyjne LOD, benchmarki

**Opis.** Pomiar czasu GPU przez timestamp queries, `RenderStats` z podziałem na warstwy,
adaptacyjna skala progów LOD z histerezą. Harness benchmarków na scenach referencyjnych,
zapis do `bench/frames/*.json`, porównanie z baseline w CI.

**Kryterium ukończenia.** Cele §20.2 osiągnięte na sprzęcie referencyjnym; regresja p95 > 8%
zatrzymuje build.

**Dodatkowy pomiar z terminem — selekcja kadru (zobowiązanie wobec M2).** WP10 mierzy osobno koszt
`CsrGrid::query_rect` + odrzucenia po `Building.aabb` w scenach `bench_district` i `bench_city`
(licznik `snapshot_select_ms` w `RenderStats`). M2 świadomie zostawił `GridSpec` w 2D na podstawie
mojego argumentu i poprosił o sygnał, gdyby pomiar pokazał inaczej — **z zastrzeżeniem, że zmiana
`GridSpec` po M4 dotyka czterech crate'ów, więc zgłoszenie ma wartość tylko wcześnie.**
Próg alarmowy: `snapshot_select_ms > 0,3 ms` w `bench_city`. Po przekroczeniu WP10 **natychmiast**
zgłasza to M2, nie czeka na koniec fazy. Jeśli próg nie zostanie przekroczony — zamykamy temat
pisemnie, żeby nikt nie wracał do trzeciego wymiaru bez danych.

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.10 `RenderBudget` i adaptacja

```rust
pub struct RenderBudget {
    pub target_ms: f32,             // 16,6 lub 33,3 wg trybu kamery
    pub lod_scale: f32,             // 0,5..1,0 — globalny mnożnik progów odległości
    history:       [f32; 8],        // timestamp queries GPU
    cooldown:      u32,
}
pub struct RenderStats {            // eksportowane do devtools i do M12
    pub gpu_ms: f32, pub cpu_ms: f32,
    pub draw_calls: u32, pub triangles: u64,
    pub instances_by_lod: [u32; 4],
    pub impostor_regen: u8, pub impostor_resident: u16,
    pub voices_active: u8, pub chunk_remesh_count: u32,
    pub snapshot_select_ms: f32,    // koszt query_rect + odrzucenia po Aabb3 (zob. WP10)
}
```

Jeśli p95 z 8 klatek > `target_ms`, `lod_scale` spada o 10% (dolny limit 0,5). Jeśli p95 < 80%
celu przez 60 klatek — rośnie o 5%. **Cooldown 30 klatek** między zmianami; bez niego system
oscyluje i pop LOD-u jest widoczny jako pulsowanie.

**Reguła (rozstrzygnięta, nie rekomendacja — decyzja 9.11):** `RenderBudget` zmienia wyłącznie
odległości progowe i capy instancji po stronie GPU. **Nie dotyka `ViewQuery`, `SnapshotCaps`
ani niczego, co idzie do sim.** Obniżanie capu snapshotu pod presją wydajności byłoby sprzężeniem
zwrotnym render → symulacja — czyli dokładnie tym, czemu zapobiega wydzielenie `sim-snapshot`.
Cap snapshotu jest stałą konfiguracji, nie zmienną runtime'u.

### 5.11 Systemy i ich częstotliwość

Systemy renderu i audio **nie są systemami ECS** — nie ma ich w DAG-u schedulera symulacji.
Żyją w pętli renderu (PRD §17.1: „tick renderu niezależny").

| System | Miejsce | Częstotliwość | Dostęp |
|---|---|---|---|
| `fill_render_snapshot` | wątek sim, job | ≤ 30 Hz, ograniczane | `&World` → `&mut Snapshot` (bufor tylny) |
| `InstanceCompaction` | wątek renderu | per klatka | `&Snapshot` |
| `LodClassify` + `RenderBudget` | wątek renderu | per klatka | `&Snapshot`, `&mut RenderBudget` |
| `ImpostorRegen` | wątek renderu + job | ≤ 4 bloki/klatkę | `&VoxelWorld` (M1) |
| `InteriorGenerate` | job | on-demand, cache LRU | `&Snapshot`, `&BuildingGrammar` |
| `LightListBuild` | wątek renderu | per klatka | `&Snapshot` |
| `WeatherUpdate` | wątek renderu | per klatka | `&Snapshot.weather` |
| `AudioUpdate` | wątek audio | 60 Hz | `&Snapshot` |
| `OcclusionRaycast` | wątek audio, job | 4 Hz | `&VoxelWorld` |
| `MusicDirector` | wątek audio | 4 Hz + granica taktu | `&Snapshot.player` |

**Ani jeden z nich nie ma `&mut World`.** To jest egzekwowane sygnaturami.

---
