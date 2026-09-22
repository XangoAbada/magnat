# E4 — Symulacja: doba, ruch, złoża, zdarzenia

**Po co.** Po przeplanowaniu reszta doby mieszkańca wykonuje się dwa razy (dwa zakupy).
Parking przestaje wiązać godzinę po przyjeździe. Podróż przerwana limitem czasu potrafi
zawiesić kolejkę na zawsze. Zapytania o złoża gubią pokłady poza punktem zarodka.

**Wejście.** E1 zamknięty. Niezależny od E2, E5, E6.

**Pomiar zamknięcia.** Test pętli doby `doba_przez_systemy_ecs_planuje_dowozi_i_zaspokaja`
(`sim/world/tests/population.rs:289`) biegnie w CI i przechodzi, a `arrivals == trips`
(dziś `31971 ≠ 44188`, `D-R7`).

## Punkty

- [ ] **N4.1** łańcuch zdarzeń DES się nie podwaja — `M3#1`, `M3#2`, `M3#3`
  - Po `ReplanCause::Late` funkcja `zakotwicz` (`sim/agents/src/systems.rs:367`, `:512-539`)
    wstawia `StartActivity(i, t)`, a stare zdarzenia łańcucha zostają (`des.rs:120-122,245-250`).
    Dla zachowanych slotów stare i nowe są identyczne bit w bit (`payload` = 0), więc test
    klucza obcego z H-23 ich nie odróżnia. W release (bez `debug_assert`) reszta doby idzie
    w dwóch kopiach: dwa `begin_trip`, podwójne zakupy.
  - Gałąź `Refused` (`systems.rs:312-330`) przeplanowuje bez `zakotwicz` — mieszkaniec stoi
    do następnego `PlanDay` (PRAWDOPODOBNE).
  - *Naprawa:* wersja planu w `payload`; zdarzenie ze starszą wersją odpada.
  - *Test:* przeplanowanie w środku doby → każda aktywność startuje raz. Potem zdjąć
    `#[ignore]`/`--skip` z testu pętli doby i dopiąć go do CI (release i debug).
  - *Odtworzone* 22.09.2026 poza audytem: `magnat-headless m8miasto --citizens 3000` w buildzie
    `debug` pada na `debug_assert` z `des.rs:246` („dwa zdarzenia o identycznym kluczu")
    **pierwszej doby** — ziarno 7, region rzeczny: minuta 476; parametry domyślne: minuta 840.
    Czyli żaden przebieg pełnego miasta w `debug` nie przeżywa doby, a w `release` każdy
    idzie dalej z podwojonym łańcuchem. Ten przebieg jest gotowym testem regresji:
    po naprawie `m8miasto --days 2` w `debug` kończy się kodem 0.

- [ ] **N4.2** parking bez pojazdów widm — `M4#1`, `M4#5`
  - `sim/traffic/src/parking.rs:425-437` (`expire`), `oracle/offers.rs:289-292`: blokada
    wygasa po `depart + minuty_jazdy + 1` i zwalnia miejsce auta, które już stoi. Przy
    przyjeździe nikt blokady nie zamienia na zajęcie. Bramka `parking_no_ghosts`
    (`m3day.rs:471`) porównuje dwie liczby z tego samego rejestru.
  - `ParkingRegistry::holds` i `wait_head`/`wait_tail` w `MezoState` poza hashem stanu.
  - *Naprawa:* przyjazd zamienia blokadę na zajęcie; `expire` zwalnia tylko blokady aut,
    które nie dojechały.
  - *Test:* bramka liczy auta w `VehicleLocation::Parked` (z komponentów pojazdów) i porównuje
    z `occupied` w rejestrze — z dwóch źródeł.

- [ ] **N4.3** przerwana podróż budzi czekających — `M4#2`, `M4#8`
  - `sim/traffic/src/trip/minute.rs:214-234`: `TripFailure::Gridlock` woła `mezo.leave(edge)`
    bez `wake_one(edge, …)` (robią to `finish` i `advance`).
  - Komentarz `trip/queue.rs:7,63` („`unjam` to jedyne miejsce przekroczenia pojemności")
    jest nieprawdziwy — `dispatch` (`minute.rs:135`) i `advance` też.
  - *Test:* kolejka na krawędzi, pierwsza podróż kończy się `Gridlock` → następna rusza.

- [ ] **N4.4** testy ruchu, które nie sprawdzają tego, co mówią — `M4#3`, `M4#6`
  - `micro_writes_nothing` (`sim/agents/tests/micro_lod.rs:27`) sprawdza deklarację dostępu,
    a Mikro zapisuje przez `&self` i `Mutex` (`oracle.rs:399`). Albo test hasha stanu
    z Mikro i bez, albo zmiana nazwy na to, co sprawdza.
  - `zamkniety_most_odbiera_trase_po_kustomizacji` (`router.rs:643-679`) nie woła
    `NavRouter::customize`.

- [ ] **N4.5** złoża: zapytania kolumnowe — `M1#1`, `M1#2`, `M1#4`, `M1#6`, `M1#7`
  - `deposits_at_column` (`sim/world/src/terrain.rs:540-553`) liczy głębokość od lokalnej
    powierzchni, a strop kształtu (`top_z`, `gen/deposits.rs:184`) od powierzchni zarodka.
    Różnica terenu ≥ 0,8–4,5 m gubi pokład. Czytają to strefowanie (`zoning.rs:649`),
    `sites/place.rs:779` i inspektor.
  - `DepositShape::contains` (`deposit.rs:68-71`) przepełnia `i64` przy |dx| ≳ 13,7 km —
    osiągalne na mapie 16 km. Test bounding boxa przed rachunkiem albo `i128`.
  - `yaw_deg` losowany (`gen/deposits.rs:337`), ignorowany w `contains` → usunąć albo użyć.
    *Domyślnie:* usunąć (zmienia strumień RNG — nowy złoty hash w tym samym commicie).
  - Walidacja danych: `historic_depletion_permille ≤ 1000` (inaczej panika w `clamp`,
    `deposit.rs:217`), `size_max_m ≥ size_min_m`.
  - Opis bramki 3 („do 2⁶³ g”) vs proptest `1..u32::MAX` — poprawić opis albo zakres.
  - *Test:* kolumna na skraju pokładu na zboczu → złoże znalezione; proptest `contains` na
    pełnym zakresie mapy 16 km bez paniki.

- [ ] **N4.6** R9 na świecie 8 km — `M2#2`
  - R2f zmienił świat testu `zaden_budynek_nie_wisi_nad_terenem`
    (`sim/world/tests/city_m2d.rs:533-534`) z 8 km na 2 km; na 8 km padał („parcela 4993,
    dziura na indeksie 43"). Hipoteza audytu: test porównuje z pierwotną wysokością terenu,
    nie z niwelacją, więc wykop na zboczu daje legalne powietrze.
  - *Najpierw* sprawdzić hipotezę na parceli 4993. Jeśli wadliwy jest test — poprawić test
    i wrócić do 8 km w nocnym. Jeśli kod — naprawić niwelację.

- [ ] **N4.7** miasto statyczne: drobne — `M2#3`, `M2#6`, `M2#7`, `M2#8`, `M2#9`, `M2#10`, `M2#5`
  - T8 (`consistency.rs:390-419`) nie sprawdza dolnej granicy 10 dzielnic ani dziur/nakładek.
  - `rent_hint` z `pass_1`, nieprzeliczany po `pass_2` (`build/interiors.rs:142`).
  - Karta wycenia z bieżących dzielnic, nie z zapisu (`value.rs:495-509`, PRAWDOPODOBNE).
  - Martwy zapis importu w domknięciu (`sites/closure.rs:92,123` vs `:138`).
  - Zapytanie promieniowe gubi encje spoza siatki (`engine/spatial/src/csr.rs:144-147`,
    PRAWDOPODOBNE).
  - Karta inspekcji z polskimi literałami (`city/inspect.rs:30-138`) → `LocKey`.
  - Test karty inspekcji tautologiczny (`consistency.rs:876`).

- [ ] **N4.8** pożar nie wyłącza pożarów w całym mieście — `M8#7`
  - `registry.rs:386` `in_cooldown` per definicja zdarzenia: jeden pożar wyłącza pożary
    wszędzie na 180 dób. Cooldown per (definicja, cel).

- [ ] **N4.9** B2B: indeks i partie — `M6#3`, `M6#10`, `M6#12`
  - Kontrakt `Indexed` bez obrotu spot wycenia się na indeksie 0
    (`.and_then(spot_index).unwrap_or(Money::ZERO)`), cena może zejść do zera; cena
    kontraktowa wraca do okna spot (`contract.rs:602`) — indeks karmi się sam sobą.
    Brak indeksu = cena bazowa; ceny kontraktowe poza oknem spot.
  - Scalanie partii ignoruje `QUARANTINED`/`RESERVED` (`store/aging.rs`,
    `Batch::coalesce_key`).
  - `Batch::unit_cost` (`batch.rs:222`) rzutuje `as i64` bez nasycenia.

- [ ] **N4.10** relacje, marka, giełda — `M10#4`, `M10#9`, `M10#15`, `M10#17`, `M10#18`, `M10#19`, `M10#20`
  - Pierwsza sesja po debiucie daje fałszywe przejęcie: `debut`/`list` nie ustawiają
    `known_stakes` (`equity/system.rs:290`, `mod.rs:214`, `corp.rs:53`); to samo dla
    spadkobiercy (`inherit.rs:139`).
  - Hazard wykrycia zmowy stały (`relations/cartel.rs:67,219`) — odchylenie porównuje dwie
    zamrożone liczby.
  - Płace idą do dzielnicy zakładu, nie mieszkania (`step/finance.rs:789-805`).
  - Paniki z danych: `max_members: 0` (`data.rs:155` → `talks.rs:480`),
    `shock_span_days = 0` (`events.rs:56`) → walidacja przy wczytaniu.
  - Strajk może trwać 90 dni przy kryterium „< 90" (`talks.rs:381`); strajk zerodniowy przy
    funduszu 0.
  - Marka: przypięcie po 9 zakupach zamiast 8 (`brand.rs:451`), wypychanie bez zaniku
    (`:352-357`), przypięcie wieczne i ciche odrzucanie przy 16 slotach.
  - Racjonowanie w arkuszu po identyfikatorze, nie po cenie (`book.rs:162-182`, PRAWDOPODOBNE).

- [ ] **N4.11** wyjaśnialność AI — `M7#6`, `M7#12`
  - `full_city.rs:93-105`: `reasons_logged >= ai.actions()` przy 1,06 mln powodów wobec
    28 tys. akcji. §7.8 wymaga: każda akcja ma dokładnie swój powód → równość per akcja.
  - `labor_policy.rs:107`: `as i32` przed `clamp` (w `labor/hr.rs:343` już naprawione).

- [ ] **N4.12** drobne w testach i danych — `M3#6`, `M0#10`
  - Pomocnik testu WP8 liczy wiek na dzień 0 (`sim/agents/tests/demography.rs:446-447,457`);
    martwe `let _ = wiek;`.
  - `Fx::from_ratio` i `Fx::div` przy ujemnym mianowniku nie zaokrąglają ku −∞
    (`fixed.rs:48,88`); `to_int_round` (`:65`) przepełnia się blisko `MAX`.

## Znalezione po drodze

- [ ] **N4.13** erozja czyta swój plik obok reszty danych — `nowe`
  - `sim/world/src/gen/erosion.rs:116`: `ErosionParams::load(Path::new("data/geology/erosion.ron"))
    .unwrap_or_default()`. Ścieżka względna wobec katalogu roboczego zamiast
    `crate::data_path` (jak w `geology.rs:31-33`, `deposits.rs:114-115` i w teście tego
    samego pliku, `erosion.rs:505`), a błąd odczytu i walidacji jest połykany.
  - Skutki: `MAGNAT_DATA` nie dociera do erozji; uruchomienie spoza korzenia repo, literówka
    w pliku albo wartość odrzucona przez walidację (`D·dt/dx² > 0,25`, `n ≠ 1`) dają
    po cichu wartości domyślne. Dziś domyślne są równe plikowi, więc nic tego nie zdradza.
  - *Naprawa:* `crate::data_path` i błąd przerywający generację (jak w przebiegach P8, P9).
  - *Test:* katalog danych z `erosion.ron` o innym `dt_years` → inny hash terenu; plik
    z `stream_power_n: 2.0` → generacja kończy się błędem, nie światem.

- [ ] **N4.14** `tuning/supply.ron` bez kontroli zakresów — `nowe`
  - `Tuning::load` (`sim/supply/src/tuning.rs:165-177`) sprawdza tylko składnię
    i `schema_version`. Ujemny koszt transportu, marża B2B albo spread eksportu przechodzą
    do symulacji. Pozostałe pliki `tuning/` i `economy/` mają walidację (`city.ron`
    `BadWeights`, `relations.ron` `Zero`/`Weights`); ten jest wyjątkiem.
  - *Naprawa:* `TuningError::Range { pole, wartość }` dla pól, które są kosztem, marżą,
    stawką albo liczbą dni (≥ 0; udziały w `bp` ≤ 10 000), w tym samym stylu co `N4.5`
    i `N4.10`.
  - *Test:* `transport.cost_gr_per_tonne_km: -1` → `Err` z nazwą pola.

- [ ] **N4.15** woda surowa bez źródła w 7 ze 128 światów — `nowe` (z `N1.2`)
  - `sim/world/tests/consistency.rs` `macierz_32_ziaren_x_4_profile` nie biegł nigdzie od M2e.
    Dopięty do CI w `N1.2` pada: T11 („domknięcie łańcuchów") zgłasza „brak źródła dla:
    raw_water · podaż/popyt 0,00, poza [0,85; 1,3]" dla ziaren 2, 5, 16, 25, 27 (profil
    `university`) i 10, 12 (`agricultural`). Pozostałe 6 testów pliku przechodzi.
  - *Do ustalenia przy naprawie:* czy te profile nie stawiają ujęcia wody, czy teren tych
    ziaren go nie dopuszcza — i czy wtedy miasto ma dostać źródło zewnętrzne, czy test ma
    wyłączyć T11 z uzasadnieniem.
  - *Test:* ten sam, bez `#[ignore = "N4.15: …"]` — wraca do kroku `E1 — testy…` joba
    `determinism`.
