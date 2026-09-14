# M4d — Mikro i dowód spójności

Podfaza 4 z 4 fazy **M4 — Ruch** (`M4-ruch.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M4b (mezo), M4c (wybór środka). |
| **Pakiety robocze** | WP8, WP9, WP11, WP12, WP13 |
| **Projekt techniczny** | §5.4, §5.8, §5.9, §5.10 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: korki widoczne i mierzone, „kamera nie zmienia świata”. |
| **Kryterium zamknięcia** | Kryteria WP8, WP9, WP11, WP12 i WP13 oraz bramki 1–7 fazy M4 w `00-postep.md`. |
| **Poprzednia / następna** | `M4c-wybor-srodka-parkingi-komunikacja.md` · — (ostatnia w fazie) |

Warstwa mikro (IDM, MOBIL, sygnalizacja) z dostępem read-only do stanu ekonomicznego, harness równoważności mikro↔mezo z kalibratorem, nakładki UI, generator imion i nazwisk (dług z M3, §5.10) oraz domknięcie wydajnościowo-determinizmowe.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP8** | Warstwa mikro | WP3 | IDM (car-following), MOBIL (zmiana pasa), sygnalizacja (fazy, cykl), ronda, pierwszeństwo. Aktywna tylko dla krawędzi w LOD Mikro. **Dostęp read-only do `LinkState` i `TripLedger`** — wymuszone deklaracją systemu w schedulerze. Serwo domykające czas przejazdu do wartości zaksięgowanej. | Ruch w kadrze wygląda poprawnie: kolejki przed światłem, wjazdy na rondo, wyprzedzanie; deklarowany zbiór zapisów systemu mikro nie zawiera żadnego komponentu ekonomicznego (test) |
| **WP9** | **Dowód spójności mikro↔mezo** | WP8 | Harness `micro_mezo_equivalence` + kalibrator offline parametrów IDM ↔ VDF w `tools/balansator`. Szczegóły w §5.4 i §7.2. | Twarde asercje (pieniądz, paliwo, minuta przybycia) z tolerancją 0; miernik dryfu kalibracji w normie; test „kamera nie zmienia świata" zielony |
| **WP11** | Nakładki UI i inspekcja | WP3, WP7, WP10 | Nakładki: natężenie, korki, izochrony, parkingi, obciążenie linii. Karta inspekcji podróży i pojazdu. Filtr „pokaż tylko X" (§14.2). | Nakładki działają na snapshocie double-buffered, bez blokowania symulacji; przełączanie nakładki < 1 klatka |
| **WP12** | Wydajność i determinizm | wszystkie | Profilowanie, chunkowanie jobów, budżety, hash stanu ruchu w funkcji haszującej ECS, benchmarki criterion. | Budżety z §7.3 dotrzymane; dwa przebiegi tego samego seeda = identyczny ciąg hashy przez 100 dni gry |
| **WP13** | **Generator imion i nazwisk** | WP11 | Pule `data/names/first_names_pl.ron` i `surnames_pl.ron`, ładowane jak `districts_pl.ron`; rozmiar puli jako źródło zakresu losowania w miejsce stałych `256`/`512`; nazwisko w formie zgodnej z płcią; formatowanie w `engine/ui`. Szczegóły w §5.10. | Karta mieszkańca i karta podróży pokazują „Anna Kowalska", nie `#84213 (45/321)`; testy `imie_zgadza_sie_z_plcia`, `nazwisko_dziedziczone_w_formie_wlasnej_plci` i `indeks_zawsze_ma_wpis_w_puli` zielone |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść. Wyjątkiem jest **§5.10**,
która jest nowa: przyszła z pakietem WP13 po zamknięciu M3 (`Z-7`), a nie z podziału
zakresu fazy.

### 5.4 **Spójność mikro ↔ mezo — architektura i dowód**

To jest kluczowa sekcja fazy. Kontrakt (dok. 00 §4, PRD §17.4) brzmi: *wynik ekonomiczny nie zależy
od poziomu LOD; test przebiegu mikro i mezo → identyczne salda pieniężne, tolerancja 0.*

#### Argument wymuszający architekturę

Zbiór krawędzi w LOD Mikro zależy od **kamery gracza**. Kamera nie jest częścią symulacji i nie
wchodzi do hasha stanu. Gdyby warstwa mikro wpływała na wynik ekonomiczny, obrócenie kamery
zmieniłoby salda gospodarstw domowych — determinizm (dok. 00 §3) przestałby obowiązywać.
Stąd jedyna architektura, która spełnia oba kontrakty naraz:

> **Warstwa mezo jest jedynym źródłem prawdy. Warstwa mikro jest ograniczonym wizualizatorem
> z zakazem zapisu do stanu symulacji.**

Mezo działa **zawsze i dla każdej krawędzi**, niezależnie od LOD. Mikro nie zastępuje mezo —
dokłada się do niej dla krawędzi widocznych.

#### Wspólny model kosztu przejazdu

Jedna funkcja czysta, wywoływana identycznie w obu LOD (w mikro — przez warstwę mezo pod spodem):

```rust
/// JEDYNE miejsce, w którym powstaje czas przejazdu, zużycie paliwa i koszt pieniężny.
/// Czysta funkcja: te same argumenty → ten sam wynik, zawsze.
pub fn settle_edge(
    edge: &RoadEdge,
    link: &LinkState,                 // stan krawędzi w minucie wjazdu
    veh: &VehicleSpec,
    load: Mass,
    entry: SimMinute,
    stops_at_entry_node: u8,
) -> LedgerEntry;

pub fn settle_node(
    turn: &TurnMovement,
    node: &NodeState,                 // długość kolejki ruchu skrętnego, faza sygnalizacji
    entry: SimMinute,
) -> (SimMinute /*wyjazd*/, u8 /*zatrzymania*/);
```

Model czasu (mezo i mikro identycznie):

1. **Krawędź** — prędkość z diagramu podstawowego (VDF) na skwantowanej gęstości:
   `mean_speed_dkmh = vdf(free_flow, capacity, occupancy)`, wynik zaokrąglany do decykm/h
   **przed** użyciem gdziekolwiek indziej. Czas = `length_cm / mean_speed` w setnych sekundy,
   zaokrąglany `div_round_half_up` do minut w ledgerze.
2. **Węzeł** — opóźnienie z modelu kolejkowego ruchu skrętnego: `delay = f(przepływ/przepustowość,
   udział zielonego, długość kolejki)`; liczba zatrzymań z tego samego modelu.
3. **Spillback** — gdy `EdgeQueue.occupancy == storage_capacity`, wjazd jest wstrzymany i kolejka
   propaguje się w górę; to samo zjawisko w mikro widać jako korek blokujący skrzyżowanie.

Model paliwa (§9.5), całkowitoliczbowy:

```
fuel_ml = base_ml_per_100km(class)
        * dist_cm / 100_000
        * speed_factor(mean_speed_dkmh)      // tabela z data/, indeksowana kubełkiem prędkości
        * load_factor(load, kerb_mass)
        * grade_factor(grade_permille)
        / 1_000_000                          // skala mnożników: promile × promile
        + stops * idle_ml_per_stop(class)
        + cold_start_ml (tylko pierwsza krawędź podróży)
```

Wszystkie mnożniki to `u32` w promilach, wszystkie dzielenia przez `div_round_half_up`.
`mean_speed_dkmh` jest już skwantowane, `dist_cm` całkowite — **we wzorze nie ma ani jednego floata**,
więc wynik jest identyczny na każdej platformie i w każdym LOD z definicji, nie z tolerancji.

Koszt pieniężny przejazdu: `fuel_ml × unit_price_gr_per_l / 1000` + opłaty (parking, bilet, myto)
+ amortyzacja (`wear_gr_per_100km × dist`). Podział kwoty na strony wg reguły z dok. 00 §2.

#### Co robi warstwa mikro

| Może | Nie może |
|---|---|
| czytać `LinkState`, `TripLedger`, `RoadGraph` | zapisywać czegokolwiek z `sim/*` poza własnym `VehicleState` |
| integrować pozycje, pasy, akceleracje (IDM/MOBIL) | zmieniać `booked_exit` |
| odgrywać cykl sygnalizacji i pierwszeństwo | zmieniać `EdgeQueue`, `occupancy`, `LinkState` |
| raportować zmierzony diagram podstawowy do devtools | karmić tym pomiarem symulację w czasie gry |

Zakaz jest **wymuszony przez scheduler, nie przez dyscyplinę**: system `micro_step` deklaruje
`Read<LinkState> + Read<TripLedger> + Write<VehicleState>`. Test WP9 asercją sprawdza deklarowany
zbiór zapisów systemu mikro — dopisanie tam komponentu ekonomicznego wywala CI.

**Serwo.** Mikro dostaje `booked_exit` i domyka do niego czas przejazdu przez korektę pożądanej
prędkości IDM (`v_desired *= clamp(remaining_booked / remaining_micro, 0.85, 1.15)`). Korekta jest
w praktyce bliska 1, bo parametry IDM są kalibrowane offline do VDF (niżej). Gdy pojazd wyjedzie
wcześniej — czeka na linii wyjazdowej; gdy się spóźni — krawędź zwalnia go dopiero wizualnie,
ledger pozostaje nietknięty. **Wielkość korekty jest metryką jakości, nie poprawności.**

#### Kalibracja offline (tu mieszka gałka strojenia)

Diagram podstawowy IDM ma postać zamkniętą: dla stanu równowagi odstęp
`s_e(v) = s0 + v·T / sqrt(1 − (v/v0)^4)`, więc gęstość `k(v) = 1/(s_e(v) + l)` i przepływ `q = k·v`.
VDF w `data/roads/vdf.ron` **jest wyprowadzony z tych parametrów**, nie dopasowany osobno.
`tools/balansator` ma zadanie `calibrate-vdf`: dla każdej klasy drogi przepuszcza mikro na
jednorodnym pierścieniu przy rosnącej gęstości, mierzy realny `q(k)` i przelicza tabelę VDF.
Uruchamiane ręcznie przy zmianie parametrów IDM, wynik commitowany do `data/`. To jest jedyny
kanał mikro → symulacja i jest **offline, powtarzalny i widoczny w diffie**.

#### Test spójności — `micro_mezo_equivalence` (WP9)

Scenariusz referencyjny: siatka 20×20 + jedna arteria + 3 skrzyżowania z sygnalizacją i 1 rondo;
5 000 pojazdów; skryptowane pary origin–dest i minuty odjazdu; 3 godziny gry; stały seed.

| # | Przebieg | Asercja | Tolerancja |
|---|---|---|---|
| A1 | `ForceMezo` vs `ForceMicro` | dla każdej podróży: `ledger.total_fuel` | **0** |
| A2 | jw. | dla każdej podróży: `ledger.total_money` | **0** |
| A3 | jw. | dla każdej podróży: minuta przybycia | **0** |
| A4 | jw. | suma gotówki wszystkich GD; hash komponentów pieniężnych i `FuelTank` | **0** |
| A5 | `ForceMezo` vs przebieg ze skryptowaną ścieżką kamery przełączającą LOD w trakcie podróży | ciąg hashy stanu ECS co 1000 ticków | **identyczny** |
| B1 | `ForceMicro` | **zmierzony** (nie zaksięgowany) czas przybycia vs zaksięgowany: średni błąd względny | ≤ 3 % |
| B2 | jw. | 95. percentyl błędu względnego | ≤ 12 % |
| B3 | jw. | błąd ze znakiem (systematyczne obciążenie) | ≤ 1 % |
| B4 | jw. | test Kołmogorowa–Smirnowa rozkładu czasów przejazdu mikro vs rozkład generowany przez VDF | p > 0,01 |

A1–A5 są **twarde i blokujące** — i prawdziwe *z konstrukcji*, nie przez szczęście: obie ścieżki
wołają tę samą `settle_edge` na tym samym `LinkState`. Ich rolą jest bycie strażnikiem regresji:
pierwsza osoba, która „dla realizmu" podepnie mikro pod ekonomię, zobaczy czerwony CI.

B1–B4 to **miernik dryfu kalibracji** — mierzy, czy animacja wciąż opowiada tę samą historię, co
księgowość. Blokujący w buildzie nocnym, ostrzegawczy w PR. Jego wywalenie się jest sygnałem
„uruchom `calibrate-vdf`", nie „popraw kod ekonomii".

**Świadomie przyjęty sufit.** Mikro nie odkrywa korków, których mezo nie widzi — mezo widzi
wszystkie krawędzie, więc nie ma czego odkrywać, ale wierność zjawisk lokalnych (fala zatrzymań,
blokowanie skrzyżowania przez pojazd stojący w korku) żyje tylko w mikro i **nie wpływa na koszt**.
Jeżeli w M12 profilowanie pokaże, że to zubaża rozgrywkę, ścieżką rozwoju jest podniesienie
wierności *mezo* (drobniejsze kubełki gęstości, jawny model blokowania węzła), nie oddanie
ekonomii w ręce mikro.

### 5.8 Systemy ECS i częstotliwości

| System | Crate | Częstotliwość | Odczyt / zapis |
|---|---|---|---|
| `nav::cch_customize_step` | nav | `EveryMinute` (budżet pracy) | R `RoadGraph` / W `ChGraph` (bufor) |
| `nav::cch_recontract_step` | nav | `EveryMinute` (budżet pracy) | jw. |
| `nav::route_service_drain` | nav | `EveryMinute` | R `ChGraph`, `RouteCache` / W `RouteCache` |
| `nav::travel_time_matrix_update` | nav | `EveryHour` | R `TripLedger` / W `TravelTimeMatrix` |
| `traffic::mezo_node_step` | traffic | `EveryMinute` | R/W `NodeState`, `EdgeQueue` |
| `traffic::mezo_edge_step` | traffic | `EveryMinute` | R/W `EdgeQueue`, `LinkState`; W `TripLedger`, `FuelTank` |
| `traffic::trip_dispatch` | traffic | `EveryMinute` | R `TripRequest` / W `EdgeQueue`, kolejka DES |
| `traffic::transit_dispatch` | traffic | `EveryMinute` | R `Timetable` / W `TransitRun` |
| `traffic::transit_boarding` | traffic | `EveryMinute` | R/W `TransitStop`, `TransitRun` |
| `traffic::parking_expire` | traffic | `EveryMinute` | R/W `ParkingLot` |
| `traffic::vehicle_wear` | traffic | `EveryDay` | R `TripLedger` / W `VehicleCondition` |
| `traffic::micro_step` | traffic | **`Every100ms`**, tylko LOD Mikro | **R** `LinkState`, `TripLedger`, `RoadGraph` / **W** `VehicleState` |
| `traffic::lod_select` | traffic | co klatkę renderu (poza symulacją) | R kamera, `LinkState` / W zbiór mikro |
| `agents::mode_choice` | agents (rozszerzenie) | zdarzeniowo (`TripRequest`) | R `NavServices`, GD / W `ModeDecision` |

`traffic::lod_select` celowo **nie jest systemem symulacji** — działa po stronie renderu i nie ma
prawa zapisu do `sim/*`. To formalne odzwierciedlenie §5.4.

### 5.9 Determinizm

1. **Kolejność aktualizacji mezo.** Iteracja po krawędziach po `EdgeId` rosnąco (`Vec`, nie mapa).
   Wewnątrz krawędzi kolejka `EdgeQueue` jest kopcem po kluczu `(exit_minute, vehicle_entity_index)` —
   klucz jest totalnym porządkiem, remisy niemożliwe.
2. **Kolejność aktualizacji mikro.** Cztery fazy na tick 100 ms:
   - **A (równolegle, read-only):** dla każdego pojazdu policz pożądane przyspieszenie (IDM) i chęć
     zmiany pasa (MOBIL), zapisz do scratchu per pojazd. Czyta poprzedni, zamrożony bufor pozycji.
   - **B (rozstrzyganie konfliktów, deterministyczne):** zgłoszenia zmiany pasa i wjazdu na
     skrzyżowanie sortowane kluczem
     `(node_index, turn_index, priority_class, distance_to_stopline_cm, vehicle_entity_index)`.
     Przetwarzane sekwencyjnie w tym porządku; równoległość po węzłach jest dozwolona, bo klucz
     zaczyna się od `node_index`, a węzły są rozłączne.
   - **C (równolegle):** integracja pozycji + serwo do `booked_exit`.
   - **D:** zamiana buforów; komendy strukturalne posortowane po `(SystemId, entity_index)` (dok. 00 §3.4).
3. **Pierwszeństwo bez wyścigu.** `priority_class` daje porządek częściowy z reguł ruchu:
   sygnalizacja (faza zielona wygrywa) → rondo (pojazd na pierścieniu wygrywa) → znaki
   (droga główna wygrywa) → równorzędne (reguła prawej ręki, wyprowadzona z kąta krawędzi).
   Remis w tej samej klasie rozstrzyga odległość do linii zatrzymania, potem `entity_index`.
   **W żadnym miejscu nie pytamy o zegar, o identyfikator wątku ani o kolejność ukończenia jobów.**
4. **Floaty.** Dozwolone w IDM/MOBIL/geometrii (dok. 00 §2 — fizyka ruchu). Zabronione na ścieżce
   ledgera: `mean_speed_dkmh` jest kwantowane przed wejściem do `settle_edge`, a `settle_edge`
   i wzór paliwowy są w całości całkowitoliczbowe. Granica jest jedna i jest nią sygnatura
   `settle_edge` — łatwa do pilnowania w review.
5. **Kolekcje.** `RouteCache` na `SeededMap`; `reservations` i `departures` na kopcach z totalnym
   kluczem; nigdzie iteracji po `HashMap`.
6. **RNG.** Nowe warianty `StreamId`: `ModeChoice`, `FuelStationChoice`, `ParkingSearch`,
   `TransitDwell`, `VehicleBreakdown`. Dopisywane na końcu enuma, istniejące wartości nietknięte.
7. **Hash stanu.** Do funkcji haszującej ECS dochodzą: `FuelTank`, `VehicleCondition`,
   `VehicleLocation`, `EdgeQueue.occupancy`, `LinkState.mean_speed_dkmh`, `ParkingLot.occupied`,
   `TransitRun.{occupancy, delay_minutes}`. **`VehicleState` (mikro) nie wchodzi do hasha** — to
   formalny zapis tego, że mikro jest wizualizacją.

---

### 5.10 Generator imion i nazwisk (WP13)

Sekcja **nowa** — nie ma odpowiednika w pierwotnej numeracji fazy. Powstaje tutaj, bo pakiet
przyszedł z M3 jako dług (`Z-7` w dokumencie fazy), a nie z podziału zakresu M4.

#### Dlaczego to stoi w M4d, skoro dotyczy mieszkańca

`Identity.first_name` i `last_name` istnieją od M3a jako indeksy **w puli, której nikt nie zbudował**.
Kod losuje surowe liczby (`gen_range_u32(256)` w `demography.rs`, `gen_range_u32(512)`
w `migration.rs`), a karta mieszkańca z M3d wypisuje je dosłownie: gracz widzi `#84213 (45/321)`.
Dwa dokumenty odsyłają do tej puli jako do rzeczy istniejącej — komentarz typu w M3a §5.1
i M12e §66 (rodzaj gramatyczny w kronikach bierze `Gender` „z katalogu imion (M2/M3)").

M4 jest pierwszą fazą, w której to **boli widocznie**, a nie tylko wisi w komentarzu: zdanie testowe
fazy z §1 brzmi *„Anna wyjeżdża o 07:20, tankuje po drodze…"*, a karta inspekcji podróży (WP11)
jest pierwszym ekranem, na którym mieszkaniec pojawia się graczowi w zdaniu narracyjnym, nie jako
wiersz tabeli. Dlatego pakiet ląduje **po WP11** i w tej samej podfazie: bez puli WP11 pokazuje
identyfikator zamiast osoby i sam sobie zabiera połowę wartości.

#### Dane

Dwa pliki obok istniejących `districts_pl.ron` i `firms_pl.ron`, ładowane tą samą ścieżką
(`assets::data_path`, RON ze `schema_version`). **To nie jest lokalizacja UI** — nagłówek każdego
pliku powtarza tę notatkę, jak w dwóch poprzednich.

```
data/names/first_names_pl.ron    (Sex, "Anna")           — ~200 pozycji, podział po płci
data/names/surnames_pl.ron       ("Kowalski", "Kowalska") — ~600 pozycji, para form
```

**Nazwisko jest parą form, nie regułą sufiksową.** Polskie nazwisko odmienia się przez rodzaj
tylko czasem: `Kowalski → Kowalska`, ale `Nowak`, `Wróbel`, `Zaremba` i `Puchała` mają jedną formę
dla obu płci. Reguła „`-ski` → `-ska`" trafia w około jedną trzecią zbioru i myli się na reszcie,
a lista wyjątków do reguły jest dłuższa niż lista nazwisk. Para form w danych kosztuje drugą kolumnę
w pliku i zero kodu; dla nieodmiennych obie formy są identyczne i tak ma to wyglądać w diffie.

Wagi częstości **nie wchodzą** — losowanie jednostajne. Rozkład nazwisk w prawdziwym mieście ma długi
ogon, którego nikt nie zobaczy w karcie jednego mieszkańca, a waga to trzecia kolumna i drugi tryb
losowania. Gdyby kiedyś miało to znaczenie (kroniki M10f, ród gracza M9e), wagę dokłada faza, która
jej potrzebuje.

#### Co się zmienia w kodzie

1. **`Identity` nie zmienia się w ogóle** — nadal dwa `u16` i płeć w `flags` bit0
   (`Identity::FLAG_MALE`). Pakiet nie dotyka komponentu, jego rozmiaru ani układu.
2. **Zakres losowania pochodzi z długości puli**, nie ze stałej. Dziś `gen_range_u32(256)` przy puli
   200 imion trafiałby w 56 indeksów bez wpisu. Pula imion jest dzielona po płci, więc losuje się
   w obrębie właściwego podzbioru i indeks niesie od razu zgodność rodzaju.
3. **Dziedziczenie nazwiska zostaje, ale w formie własnej płci.** `demography.rs` już dziś kopiuje
   `last_name: m_id.last_name` — indeks pozostaje wspólny dla całej rodziny, forma jest wybierana
   z pary przy wypisywaniu. Dzięki temu córka Kowalskiego jest Kowalską, a syn Kowalskim, i nikt
   nie przechowuje dwóch indeksów.
4. **Formatowanie mieszka w `engine/ui`**, nie w `sim/agents` — nazwa jest prezentacją, a crate
   agentów nie ma powodu ładować katalogu nazw. `inspect/citizen.rs` i karta podróży WP11 dostają
   ten sam formater.

#### Determinizm i hash

Losowanie idzie istniejącym strumieniem RNG demografii i migracji — **żadnego nowego `StreamId`**.
Zmienia się natomiast zakres, więc wylosowane wartości `first_name`/`last_name` będą inne niż dziś,
a oba pola wchodzą do funkcji haszującej (`components.rs`). **Hash świata przestawia się jednorazowo.**
Testy determinizmu i złote testy M3 porównują przebieg z przebiegiem, nie z zapisaną liczbą, więc
przeżyją to bez zmian; gdyby gdzieś stał zapisany hash odniesienia, przeliczenie go należy do tego
pakietu, a nie do fazy następnej.

#### Świadomie przyjęty sufit

| Czego nie robimy | Dlaczego | Kto to podniesie, gdy będzie trzeba |
|---|---|---|
| unikalność imion | w mieście 274 tys. imiona **mają** się powtarzać — to odwrotność wymogu R10 dla dzielnic i firm | — |
| zależność imienia od epoki urodzenia | mieszkaniec urodzony w 1890 nazywałby się inaczej niż ten z 1985; dane są, bo `birth_day` istnieje, ale nikt tego nie zamawiał | M10f (kroniki), jeśli historia rodu zacznie obejmować pokolenia |
| drugie imię, zdrobnienia, formy adresatywne | jedna forma wystarcza karcie i kronice | M10b / M12e |
| pule per region | obie ścieżki są dziś zahardkodowane na `_pl`, bo drugiego regionu nie ma; wybór regionu to abstrakcja dla jednego konsumenta | faza, która wnosi drugi region |
| odmiana imienia przez przypadki | `{imię} wyjeżdża o 07:20` to mianownik; dopełniacz („karta Anny") wymagałby paradygmatu fleksyjnego | M12e, razem z rodzajem gramatycznym w szablonach |
| nazwy ulic | osobna luka, szersza od tej: adres mieszkańca to dziś `budynek / lokal / dzielnica`, a plan nie ma nazw ulic w żadnej fazie | decyzja właściciela produktu — patrz `D12` w §9 dokumentu fazy |
