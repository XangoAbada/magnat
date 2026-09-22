# E8 — Dokumenty zgodne z kodem

**Po co.** Część pakietów i bramek w `docs/implementation-plan/` jest odhaczona bez
spełnionego kryterium, część dokumentów wskazuje pliki, których nie ma, a plany R3 i M12
opisują kod sprzed kilku faz. Rozjazd planu z kodem jest gorszy niż błąd w planie, bo nikt
go nie widzi.

**Wejście.** E1–E7 zamknięte — dopiero wtedy wiadomo, które kryteria są spełnione.
Wyjątek: `N8.1` i `N8.2` można robić w dowolnym momencie, bo tylko korygują odhaczenia.

**Pomiar zamknięcia.** `plan_guard.py`, `struct_guard.py --all` zielone. Każde `[x]` z listy
w `N8.1` jest albo zdjęte, albo ma korektę kryterium w dokumencie fazy.

**Reguła dla każdego rozjazdu.** Dwie drogi: dopisać test, który spełnia kryterium, albo
skorygować kryterium do tego, co naprawdę jest sprawdzane (wpis w tabeli korekt dokumentu
fazy). *Domyślnie* korekta kryterium; test tylko wtedy, gdy chroni pieniądz, determinizm
albo gracza.

## Punkty

- [ ] **N8.1** odhaczenia bez spełnionego kryterium
  - M0: WP-05 (panika zamiast trybuild), WP-07 (2000 ticków zamiast 10 tys., CI 100 tys. ×
    20 tys. zamiast 400 tys. × 200 tys.), WP-02 (22 przypadki brzegowe zamiast 40), brak testu
    `Opaque`, brak skanu regex T-D10, `cargo public-api` (§7.4 pkt 6), T-D9 na ARM,
    „`unsafe` wyłącznie w `chunk.rs`" w `00-postep.md:86`.
  - M1: skirt LOD (V3), fuzz chunków (V1), budżet upsamplingu (W5); korekty liczb W3, V2, V3,
    W5 i `terrain_hash_matrix` zapisane tylko w komentarzach testów — przenieść do planu.
  - M2: WP1 odhaczony mimo budżetu 317 ms wobec 180 ms; D1 porównuje hashe, nie bufory;
    WP17 (testy spójności) — po `N1.2` zgodne albo nie.
  - M3: nazwy testów z §7 nie istnieją (mapowanie na polskie nazwy niezapisane); macierz
    „10 × 4" tylko ręcznie; `prop_age_pyramid`; `det_plan_replay` niewykonalne bez zasobów
    w zapisie.
  - M4: bramka 6 (`LatenessRecorded`, `EnergyDrawn`, `access_edge_curb_occupancy` nie
    istnieją), WP9 B1–B4 i `ramp_wait_lod_invariant`, WP12 „100 dni" (CI: 2 doby),
    §7.1 nazwy testów.
  - M5: P4, P5, P7 nie są proptestami; brak licznika alokacji §7.3; macierz CI 4×120
    zamiast 8×365 (`AD-6`) bez zapisanego skutku dla G1; niezmiennik świata tylko ostrzeżeniem.
  - M6: bramka 3 (dwa z ośmiu testów pilnują martwych mechanizmów — po E7 zgodne albo nie);
    „400 tys. partii ≤ 64 MB" to rachunek, nie pomiar.
  - M7: §7.10 (20 ziaren × 5 lat) niewykonany; `BF-10` przypisuje 715 postępowań
    brakowi zamykania firm — po E2 poprawić przyczynę.
  - M8: dowód WP4/WP6 wskazuje `tools/headless/tests/events.rs`, którego nigdy nie było;
    T1 35 dób zamiast roku; T5 w gospodarce 150 dób zamiast 20 lat; T3 5-letnie, dynamiczne
    T7, T8a, T8c, T8d bez testów; benchmark `city.Tax` przekazany do R2f i zgubiony;
    R2-WP31 `[ ]`, choć `AgencyKind::Prosecution` istnieje.
  - M9: bramka 4 (graf i Gantt niezmierzone w chwili odhaczenia); bramka 2 opisuje „rok gry,
    hash co 1000", test ma 1440 ticków co 240; WP13 bez testu `--from params.ron`.
  - M10: WP10.4 (`[~]` w M10a, `[x]` w `00-postep.md:527`), WP10.15, WP10.16 (po `N1.2`),
    WP10.22; zdanie „`MacroLodPolicy` nie powstał" (`00-postep.md:655-658`) — istnieje.
  - M11: WP9 „zmiana nazwy → ≤ 1 klatka" bez kodu; §7.3 `lod_transition_is_subtle`,
    `cut_plane_has_no_holes`, „±10 %" w `interior_reflects_stock`; „cisza w ≤ 0,5 s".
  - R1: R-WP10 `[ ]` w dokumencie, `[x]` w `00-postep.md:285`; kryterium „żadnego `pub use`
    z R1" — zostało ~16; R-WP12 „mod poniżej 500" (dziś 840).
  - R2: tabela zbiorcza §4 nie zgadza się z podfazami w 11 wierszach
    (`R2-naprawy-po-M11.md:215,223-226,230,231,234,235,238,241`); kryterium 3 „master zielony"
    przy czerwonym `macro-kernel`; „Wynik do pokazania" nie istnieje; tabela „Zmiany wpisane
    po R2" pusta; `D-R7` przeczy `ci.yml`.

- [ ] **N8.2** rejestr długu strukturalnego zgodny z pomiarem — `R1#3`, `R1#4`, `R1#7`, `R1#12`
  - Wartości rozjechane: poz. 62 (1339 → 1265), 64 (634 → 641), 40 vocab.rs (802 → **1199**,
    linia od błędu), 29 (803 → 840), 54 (1088 → 1110), 4 (1122 → 1153), 14 (949 → 1030),
    18 (381 → 449), 26 (258 → 233, pozycja nadal otwarta).
  - Ścieżki nieistniejące: poz. 35, 37.
  - 46 ze 101 ostrzeżeń bez wiersza (parcels.rs 1121, demography/day.rs 1073, social.rs 1066,
    agents/systems.rs 974, components.rs 959, places.rs 947, zoning.rs 1108 …).
  - Brak `ponytail:` przy wyjątkach z §5 (cch.rs, det_math.rs, graph.rs) i zamrożonych
    funkcjach; `D-R5` (`synthetic_road_network` w kodzie produkcyjnym) bez wiersza.
  - *Dowód:* `struct_guard --all` po `N1.7` zielony.

- [ ] **N8.3** pozycje rejestru z adresatem „R3" — `R3` ocena, `R1` ryzyka
  - 54 pozycje wskazują R3, który nie ma dla nich pakietu. Po `N1.7` bramka to widzi.
  - Każda pozycja dostaje: realnego adresata (dokument, który ją zrobi), albo datę przeglądu,
    albo `✅`, jeśli E1–E7 ją przy okazji rozwiązały.

- [ ] **N8.4** przegląd planu implementacyjnego przed wznowieniem — `R2#4`, `R3#1`–`R3#11`, `M12#1`, `M12#8`–`M12#15`
  - **R3:** 22 pozycje wykazu R2 przeniesione do R3, pakiet mają 4 (13, 39, 45, 46, 56, 57,
    58, 60, 72, 73, 75, 76, 78, 79, 80, 82, 83, 84 bez wykonawcy — część zrobi ten plan:
    73 → `N4.1`, 78 → pomiar `E2`, 84 → `N1.5`). Kryterium WP1 zależy od poz. 78; z WP1 wypadły
    warunki „wolne etaty ≤ 15 %" i „G11 w paśmie 3–12 %"; WP2 porównuje gęstość z liczbą
    osób; `per_site`/`min` przy rozdrobnieniu; dwa źródła obsady; miara „3 209 z 4 186 lokali"
    liczy różne rzeczy; tabela §2.1 nieaktualna (CH-5, CH-6 zrobione); WP4 i WP5 bez opisu
    i kryterium w R3.
  - **M12:** dwa różne pakiety WP4 (M12a i M12b); D1, D2, D14 opisane jako otwarte, choć
    rozwiązane; nazwy API niezgodne z kodem (`LocaleCatalog`/`TextKey`/`t!` vs
    `Catalog`/`LocKey`; `travel_time(DistrictId, …)` vs `TravelTimeMatrix`; `CitizenHot`
    128 B vs 14 komponentów po 148 B; `ComponentSchemaId` `u64`); §5.8 M12e „brak klucza →
    en-US → klucz" łamie zakaz cichego fallbacku z `CLAUDE.md`; rozdział gorące/zimne
    niewykonalny w tym ECS bez slabu; próg regresji 10 % vs 25 % w `bench_guard`; kolizja
    podkomendy `century`; budżet §5.6 zakłada tick minutowy w makro (krok jest dobowy);
    WP4 M12b niedoszacowany — wejściem jest inwentarz z `N6.10`; `M12c:146` ma starą sygnaturę
    `MacroLodPolicy`.
  - **M10:** rozstrzygnięcie `FF-29`/`GG-3` na podstawie pomiaru z E5.
  - *Wynik:* poprawione dokumenty R3 i M12 (tabele „Zmiany wpisane po"), wpis w `00-postep.md`,
    że plan implementacyjny jest wznowiony.

## Znalezione po drodze
