# E6 — Silnik ECS, hash stanu, zapis

**Po co.** Bezpieczne API `engine/ecs` pozwala wziąć dwie `&mut` na ten sam wiersz i pisać
do komponentu, którego system nie zadeklarował — to wyścig danych i cichy niedeterminizm.
Licznik `generation` chunka nie rośnie przy zapisie przez `Query`, czyli na głównej ścieżce
zapisu w ticku. Hash stanu pomija część stanu, więc niedeterminizm w tych polach przejdzie
przez bramki. Zapis wczytuje surowe bajty z pliku.

**Wejście.** E1 zamknięty. Niezależny od E2–E5.

**Pomiar zamknięcia.** Miri na `engine/ecs` (testy jednostkowe i integracyjne) zielony, a
celowo wprowadzony błąd z `N6.9` jest przez niego złapany. Hashe stanu z `m3day --expect`
i `m9session` po zmianach z `N6.8` zapisane jako nowe złote wartości w jednym commicie.

## Punkty

- [ ] **N6.1** `QueryIter` nie wydaje `&mut` dłużej niż pożyczka — `M0#1`
  - `engine/ecs/src/query.rs:385,539`: `iter(&mut self)` zwraca `QueryIter<'_, 'w>`, ale
    `Item = Q::Item<'w>`. `q.iter().next()` dwa razy → dwie `&mut` na ten sam wiersz.
  - *Test:* trybuild compile-fail dla dwóch `iter()` z żywymi elementami.

- [ ] **N6.2** `SystemCtx` sprawdza zadeklarowany `Access` — `M0#2`, `M0#3`
  - `system.rs:226-237`: `ctx.query::<&mut B>()` działa w systemie, który deklaruje odczyt A;
    scheduler puszcza go równolegle z innym piszącym do B.
  - `res_mut` (`system.rs:275`) tworzy `&mut World`, gdy równoległe systemy trzymają
    `&World` (PRAWDOPODOBNE — Stacked Borrows).
  - *Naprawa:* `query`/`world` sprawdzają żądany dostęp względem deklaracji (panika w debug
    i release — to błąd programisty, nie danych).
  - *Test:* system z niezadeklarowanym zapisem → panika z nazwą systemu i komponentu.

- [ ] **N6.3** `generation` rośnie przy każdym zapisie — `M0#4`, `M12#3`
  - `bump_generation` wołają tylko `World::get_mut`/`insert` i operacje strukturalne;
    `init_fetch` dla `&mut T` (`query.rs:220-237`), `par_for_each`, `par_fold` piszą bez
    podbicia. Test `world_lifecycle.rs:237` sprawdza tylko `get_mut`.
  - *Test:* zapis przez `Query<&mut T>`, `par_for_each` i `par_fold` → `generation` chunka
    większa.

- [ ] **N6.4** funkcje `pub`, które dają UB z bezpiecznego kodu — `M0#5`, `M0#6`, `M0#12`
  - `ChunkRef::column_bytes` (`chunk.rs:564` na `:314`) czyta padding `repr(Rust)`.
  - `World::restore_rows_checked` (`world.rs:516-550`) przyjmuje dowolne bajty; założenie
    K0-9 („typ bez destruktora nie trzyma wskaźnika") jest fałszywe (`&'static str`, `bool`,
    enumy, `NonZero*`).
  - `ArchetypeChunk::value_ptr` (`chunk.rs:298-305`) z samym `debug_assert`.
  - **Decyzja N6.4-a:** *Domyślnie:* wszystkie trzy `unsafe fn` z sekcją `# Safety`, a zapis
    (`engine/io`) serializuje komponenty przez jawny trait zamiast surowych bajtów kolumny
    tylko wtedy, gdy trzeba — do tego czasu `unsafe` przenosi odpowiedzialność tam, gdzie jest.
  - Wymaga zmiany K0-9 → wpis `K-n` w `00-konwencje-i-kontrakty.md` §4a.

- [ ] **N6.5** brak podwójnego drop przy panice — `M0#7`
  - `command.rs:320-360`: `apply_insert` przenosi przez `read_unaligned`, bufory resetowane
    na końcu; panika w `world.insert` → `CommandBuffer::drop` woła `drop_fn` na przeniesionych.
  - *Test:* komponent z licznikiem dropów, panika w środku flush (`catch_unwind`) → licznik = 1.

- [ ] **N6.6** odczyt zapisu nie ufa plikowi — `M0#13`, `M12#4`, `M12#5`, `M12#6`
  - `load_world` wczytuje surowe bajty, chroni go tylko xxh3 z tego samego pliku
    (`world.rs:545-546`).
  - `vec![0; n]` z długości z pliku, `zstd::decode_all` bez limitu (`snapshot.rs:378,393`);
    panika na złych danych (`:380,396`).
  - Odczyt kwadratowy: `.iter().find` per chunk i kolumna (`snapshot.rs:532-573`), plik
    otwierany per sekcja.
  - *Test:* fuzz/proptest na uszkodzonym pliku → `Err`, nigdy panika ani alokacja ponad limit.

- [ ] **N6.7** sloty odporne na przerwanie — `M12#7`
  - `std::fs::write` bez tmp+rename (`game/src/save.rs:94-100`, `replay.rs:137-140`).
    `engine/io` już robi tmp+rename — użyć tego samego.

- [ ] **N6.8** hash obejmuje cały stan — wzorzec „hash nie obejmuje całego stanu"
  - Geometria złóż (`Deposit::hash_state` bez `shape`, `deposit.rs:238-249`) — `M1#3`.
  - `Parcel.owner`, `Parcel.status`, `Workplace.site/occupant`, obrys, `RoadNetwork`,
    `CityGate`, pełny `WageBand`; nazwy bez prefiksu długości (`city/hash.rs:27-157`) — `M2#4`.
  - `ParkingRegistry::holds`, `wait_head`/`wait_tail` — `M4#5` (z `N4.2`).
  - `life_event`, `ending_seen` — `M9#2` (z `N3.3`).
  - Hash archetypu zależy od kolejności rejestracji komponentów (`io/hash.rs`), a
    `World::insert` rejestruje leniwie — `M0`.
  - *Test:* na każde pole: zmiana tylko tego pola → inny hash.

- [ ] **N6.9** Miri naprawdę biegnie — `M0` WP-04, WP-14
  - Miri nigdy nie został potwierdzony; biegnie tylko z `--lib`. `00-postep.md:86` twierdzi,
    że `unsafe` jest tylko w `chunk.rs` — jest w 7 plikach (~140 wystąpień).
  - *Dowód:* jednorazowo wprowadzony błąd (np. `N6.1` przed naprawą) → Miri czerwony;
    potem Miri na testach integracyjnych `engine/ecs` w nocnym jobie.
  - `[workspace.lints.rust] unsafe_code = "forbid"` z wyjątkiem `engine/ecs`
    (dziś atrybuty per crate, `sim/events` i `sim/macro` ich nie mają).

- [ ] **N6.10** zasoby świata w zapisie: luka jawna — `M12#2`
  - `save_world` (`snapshot.rs:210-275`) serializuje tylko archetypy; ok. 50 zasobów ma hak
    hasha, żaden nie jest serializowany. Każdy prawdziwy świat pada w `load_world`
    z `HashMismatch` (`:586`). Stan leży też poza `World` (`Session.market`, `player`,
    `scenario_state`, `legacy_day`).
  - Pełna serializacja to nowa funkcja, nie naprawa. Tu tylko: `load_world` na świecie
    z niezapisanymi zasobami zwraca błąd wymieniający je z nazwy (zamiast `HashMismatch`),
    a inwentarz zasobów i stanu poza `World` trafia do `N8.4` jako wejście dla zapisu gry.

- [ ] **N6.11** wersja schematu dziennika — `M12#16`, `M9` odstępstwa
  - `REPLAY_SCHEMA_VERSION = 1`, choć `PlayerCommand` urósł z 2 do 25 wariantów. Podbić;
    stary dziennik ma być odrzucony z komunikatem, nie odtworzony w inny świat.

- [ ] **N6.12** zawijanie generacji — `M0#14`
  - `wrapping_add` po 2³² cyklach slotu (`arena.rs:255`, `entity_store.rs:57-58`).
    Nieosiągalne w praktyce → komentarz `ponytail:` z sufitem, bez zmiany kodu.

## Znalezione po drodze

- [ ] **N6.13** zapis i dziennik znają dane, na których powstały — `nowe`
  - `ReplayHeader` (`game/src/replay.rs:35-42`) niesie `schema_version`, `engine_build`
    i `params` — nic o katalogu `data/`. Każda edycja pliku RON, a także podmiana katalogu
    przez `MAGNAT_DATA`, zmienia łańcuch hashy. Stary dziennik odtwarza się wtedy w inny
    świat, a złote pliki `--expect` / `*.hashes` rozjeżdżają się bez wskazania przyczyny.
  - Siostra `N6.11`: tam wersja kształtu komend, tu wersja treści danych.
  - **Decyzja N6.13-a:** *Domyślnie:* odcisk xxh3 po posortowanych ścieżkach i treści
    wszystkich plików `data/**/*.ron` poza `locale/` (tekst nie wchodzi do symulacji).
    Niezgodność przy odtwarzaniu → błąd wymieniający oba odciski, nie cichy rozjazd.
    Ten sam odcisk wypisuje `--out` przy hashach. Mody z M12d dopiszą do niego swoje
    katalogi — tego tu nie projektujemy.
  - *Test:* nagrać dziennik, zmienić jedną liczbę w `tuning/supply.ron`, odtworzyć → `Err`
    z odciskami; bez zmiany → odtworzenie jak dziś.
