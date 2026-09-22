# M12b — Zapis i replay

Podfaza 2 z 6 fazy **M12 — Skala i jakość** (`M12-skala-i-jakosc.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M0 (`Snapshotable`, `state_hash`), granularność chunka w `engine/ecs` (§9 D1). |
| **Pakiety robocze** | WP4, WP5, WP6, WP7, WP8 |
| **Projekt techniczny** | §5.4, §5.5 |
| **Wynik do pokazania** | `Ctrl+S` w trakcie gry nie zatrzymuje symulacji ani na jeden tick; `--replay bug_4711.mgr` odtwarza cudze zgłoszenie klatka w klatkę. |
| **Kryterium zamknięcia** | Kryteria WP4–WP8; hash zapisanego pliku równy hashowi stanu w chwili bariery. |
| **Poprzednia / następna** | `M12a-pamiec.md` · `M12c-tryb-50x-i-skala.md` |

`engine/io` w pełnej postaci, zapis w tle przez copy-on-write chunków ECS, migracje schematu, `EventJournal` z autozapisami oraz replay z bramką determinizmu w CI.

---

## Pakiety robocze

### WP4 — `engine/io` w pełnej postaci [L]
Zależności: M0 (`Snapshotable`, `state_hash`).

`SaveHeader`, `SectionEntry`, `SectionId`, kompresja zstd per sekcja, klasyfikacja sekcji `Persisted` / `Derived`, integralność (xxh3 per sekcja).

**Kryterium ukończenia:** round-trip 400 tys. miasta: zapis → wczytanie → `state_hash` identyczny; rozmiar pliku w przedziale 50–300 MB; **z pauzą** czas zapisu < 5 s (WP5 usuwa pauzę, nie przyspiesza).

### WP5 — Zapis w tle (copy-on-write chunków ECS) [L]
Zależności: WP4, dostęp do granularności chunka w `engine/ecs` (patrz §9 D1).

Bariera zapisu na punkcie synchronizacji, atomowa podmiana wskaźnika chunka, arena cieni z twardym limitem, tryb „dogoń" przy wysyceniu.

**Kryterium ukończenia:** w trakcie zapisu 400 tys. miasta żaden tick symulacji nie przekracza 1.3× swojego mediany czasu sprzed zapisu; hash stanu zapisanego pliku == hash stanu w chwili bariery (a nie w chwili zakończenia zapisu); zapis pod obciążeniem 50× nie przekracza limitu areny cieni.

### WP6 — Migracje schematu [M]
Zależności: WP4.

`Migration`, `MigrationRegistry`, łańcuchowanie N→N+1, korpus zapisów „złotych".

**Kryterium ukończenia:** test `migrates_from_every_historical_schema()` — dla każdego zapisu w `tests/fixtures/saves/v*.mgs` wczytanie kończy się sukcesem i daje oczekiwany `state_hash` po migracji. Korpus rośnie z każdym bumpem `schema_version` — dodanie migracji bez dodania zapisu złotego jest błędem CI.

### WP7 — `EventJournal` i autozapisy [M]
Zależności: WP4, WP5.

Dziennik wejść gracza od snapshotu bazowego, autozapis hybrydowy (snapshot rzadko + dziennik często), rotacja slotów, integracja z „zatrzymaj przy zdarzeniu X" (§14.5).

**Kryterium ukończenia:** autozapis przyrostowy < 200 ms i < 2 MB; odtworzenie stanu ze snapshotu bazowego + dziennika daje `state_hash` identyczny ze stanem w chwili autozapisu; crash-safety — przerwanie procesu w dowolnym momencie zapisu zostawia poprzedni zapis nienaruszony (zapis do pliku tymczasowego + `rename`).

### WP8 — Replay i bramka determinizmu w CI [M]
Zależności: WP7.

`ReplayFile`, `tools/headless --replay`, checkpointy hashy, pipeline zgłoszeń błędów (przycisk „zgłoś błąd" pakuje replay + log + manifest modów).

**Kryterium ukończenia:** zadanie CI `determinism` na każdy PR: 2 przebiegi × 3 seedy × 50 tys. ticków → identyczne ciągi hashy; replay z korpusu regresyjnego (10 nagrań) odtwarza się bit-w-bit. Rozjazd w replayu raportuje **pierwszy** rozbieżny tick i nazwę systemu (bisekcja po hashach cząstkowych per archetyp).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 Format zapisu

```rust
pub struct SaveFile;   // wyłącznie format na dysku; w pamięci nigdy nie istnieje w całości

#[repr(C)]
pub struct SaveHeader {
    magic: [u8; 8],              // b"MAGNATSV"
    format_version: u16,         // wersja kontenera (nagłówek, sekcje) — zmienia się rzadko
    schema_version: u32,         // wersja schematu świata, monotoniczna; sterują nią migracje
    engine_build: [u8; 20],      // git hash builda — diagnostyka, nie kompatybilność
    world_seed: u64,
    tick: Tick,
    sim_minute: SimMinute,
    state_hash: u64,             // hash stanu w chwili BARIERY zapisu, nie zakończenia
    data_manifest_hash: u64,     // hash zawartości data/ — zmiana danych unieważnia replay
    mods: Vec<ModStamp>,         // id + wersja + checksum każdego aktywnego moda
    flags: SaveFlags,            // COMPRESSED_ZSTD | HAS_JOURNAL | AUTOSAVE | MODS_CHANGED
    section_count: u16,
    created_utc_unix: i64,       // METADANA — celowo NIE wchodzi do state_hash
}

pub struct SectionEntry {
    id: SectionId,
    schema_version: u16,         // wersja schematu TEJ sekcji; migracje działają per sekcja
    offset: u64,
    len_compressed: u64,
    len_raw: u64,
    checksum: u64,               // xxh3 po dekompresji
}

pub enum SectionId {
    EntityAllocator, Archetype(ComponentSchemaId), ExperienceStore, DesQueue,
    ChronicleIndex, PlayerState, CityState, MarketState, ModState, UiState,
}
```

**Klasyfikacja sekcji.** Każda sekcja jest `Persisted` albo `Derived`. `Derived` (CH, graf pieszy, cache tras, indeksy przestrzenne, indeksy rynkowe, meshe voxela) **nie jest zapisywana** — odtwarza się po wczytaniu. To ścina ~200 MB z pliku, ~1 s z czasu zapisu i, co ważniejsze, całą klasę migracji: struktura pochodna może zmienić się dowolnie między wersjami, bo nigdy nie trafia na dysk. Odtworzenie po wczytaniu: ~2 s dla 400 tys. na czterech rdzeniach, pokazywane jako część paska ładowania.

**Rozmiar i czas.** ECS 440 MB + doświadczenia 102 MB + DES 48 MB + stan miasta/rynków ~40 MB ≈ 630 MB surowego. zstd-3 na danych SoA (kolumny liczb całkowitych o niskiej entropii, świetnie się pakują): ~4–5× → **130–160 MB**. Mieści się w widełkach PRD §18.1 (50–300 MB): małe miasto 50 tys. daje ~25 MB, metropolia z kroniką w pliku ~280 MB.

Czas zapisu 400 tys.: kompresja 630 MB przy ~400 MB/s/rdzeń na 4 wątkach = **0.4 s**; zapis 150 MB na NVMe (500 MB/s) = **0.3 s**; serializacja i CoW = **0.5 s**. Razem **~1.2 s**, na dysku talerzowym (100 MB/s) **~2.4 s**. Cel §20.2 (< 5 s) spełniony z zapasem 2×.

### 5.5 Zapis w tle — copy-on-write chunków ECS

To jest najbardziej ryzykowny mechanizm w fazie, więc opisany jest dokładnie.

**Jednostka CoW:** chunk archetypu ECS — blok 64 KB kolumn SoA jednego archetypu (liczba encji = 64 KB / rozmiar wiersza; dla `CitizenHot` przy 128 B to 512 encji/chunk). Przy 400 tys. miasta całe gorące ECS to ~7 000 chunków.

**Kierunek kopiowania: sim kopiuje, wątek zapisu czyta oryginał.** Odwrotny kierunek (zapis czyta „żywą" pamięć, sim kopiuje stare wartości do cienia) wymagałby synchronizacji na każdym odczycie wątku zapisu. W wybranym wariancie wątek zapisu pracuje wyłącznie na pamięci, która od momentu bariery jest **niemutowalna** — zero wyścigów, zero blokad na ścieżce odczytu.

**Przebieg:**

1. **Bariera** (systemem `SaveBarrierSystem`, częstotliwość `EveryMinute`, wykonywany w punkcie synchronizacji po zaaplikowaniu buforów komend — czyli tam, gdzie dok. 00 §3.4 już gwarantuje spójność stanu). Bariera: (a) zwiększa `save_epoch`, (b) przechodzi po liście chunków i zapamiętuje ich bieżące wskaźniki w `SaveManifest`, (c) liczy `state_hash`. Koszt: ~7 000 wpisów po 16 B = jedno przejście liniowe, **< 1 ms**. To jedyny moment, w którym symulacja czeka — mieści się poniżej szumu jednego ticka.
2. **Okno zapisu.** Każdy zapis do chunka sprawdza `chunk.epoch != save_epoch`. Jeśli chunk nie był jeszcze dotknięty w tej epoce: alokacja nowego bloku 64 KB, `memcpy` starej zawartości, ustawienie `epoch`, **atomowa podmiana wskaźnika** w tablicy chunków świata (`AtomicPtr`, `Release`). Stary blok jest trzymany przez `Arc` w `SaveManifest`, więc nie znika pod wątkiem zapisu. Sim od tej chwili pisze do nowego bloku.
3. **Mutacje strukturalne.** Tworzenie encji po barierze alokuje nowe chunki — nie ma ich w `SaveManifest`, więc nie trafiają do zapisu (poprawnie: zapis reprezentuje stan z chwili bariery). Usuwanie (swap-remove w chunku) to zwykła mutacja → zwykły CoW. Żadnej dodatkowej ścieżki.
4. **Wątek zapisu** iteruje po `SaveManifest` w kolejności `SectionId`, kompresuje chunk po chunku (4 wątki `rayon`, składanie deterministyczne po indeksie), zapisuje do pliku tymczasowego. Po wysłaniu chunka zwalnia jego `Arc` — cień znika natychmiast, nie po zakończeniu całego zapisu.
5. **Zamknięcie:** `fsync`, zapis nagłówka z tabelą sekcji, `rename` na docelową nazwę (atomowe na NTFS i ext4 — crash w trakcie zostawia poprzedni zapis nienaruszony). Wyczyszczenie `save_epoch`.

**Narzut czasu.** Kopia jednego chunka 64 KB przy ~15 GB/s: **~4 µs**. Pesymistycznie w oknie 1.2 s dotknięte zostaną wszystkie 7 000 chunków → **28 ms łącznego CPU rozłożone na ~600 ticków** przy prędkości 1× (0.05 ms/tick, poniżej 3% budżetu ticka) i na ~10 ticków przy 50× (2.8 ms/tick — **to boli**, dlatego `SpeedGovernor` obniża prędkość do 10× na czas zapisu w tle; alternatywą byłoby przekroczenie budżetu 2 ms/tick z §5.6). Narzut jest jednorazowy per chunk per epoka — drugi i kolejne zapisy do tego samego chunka są darmowe.

**Narzut pamięci.** Szczyt = suma cieni chunków dotkniętych, ale jeszcze nieprzetworzonych przez wątek zapisu. Ponieważ wątek zapisu zwalnia cienie na bieżąco, a kompresja (0.4 s) jest szybsza niż pesymistyczne dotknięcie wszystkich chunków, realny szczyt to **80–150 MB**. Budżet: **280 MB twardego limitu** areny cieni.

**Wysycenie limitu — tryb „dogoń":** przy 80% limitu `SpeedGovernor` obniża prędkość gry o jeden szczebel i wątek zapisu dostaje wyższy priorytet. Przy 100% sim **zatrzymuje się na najbliższej barierze** do zwolnienia 20% areny. To degradacja (gracz widzi krótką pauzę), nie awaria — i jest metryką alarmową w testach (§7.4). Jedyny scenariusz, w którym to wystąpi, to zapis podczas 50× na dysku talerzowym.

---

## Zmiany wpisane po M3a

Zgodnie z `K-18`.

| # | Zmiana | Dlaczego |
|---|---|---|
| — ★ | **Zapis musi nieść zasoby świata, nie tylko archetypy ECS.** Dziś `world_state_hash` hashuje zasoby przez haki (`World::register_resource_hash`), a `save_world`/`load_world` ich nie serializują — świat z zarejestrowanym hakiem **nie przechodzi własnego round-tripu**: `load_world` odrzuca plik z `HashMismatch`, bo po wczytaniu zasobu nie ma. Pierwszy przypadek: M3 rejestruje `RelationSlab`, `KnowledgeSlab`, `PlanSlab` i `EventQueue` (M3a, korekta D-4). Kolejka zdarzeń jest stanem trwałym tak samo jak komponenty — to, co ma się wydarzyć jutro, jest częścią świata | Sekcja `SectionKind::Arena` z K-16 jest wzorem: zasób dostaje sekcję z własnym zakresem bajtów i sumą kontrolną. Do czasu tej zmiany M3 obchodzi problem, rozbijając rejestrację na `register_components` i `register_resources` — ale to jest obejście, nie rozwiązanie: zapis gry z M3 nie odtworzy planów dnia ani zaplanowanych zdarzeń demograficznych |
