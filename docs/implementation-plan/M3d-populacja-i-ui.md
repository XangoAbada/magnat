# M3d — Populacja i UI

Podfaza 4 z 4 fazy **M3 — Ludzie i dzień** (`M3-ludzie-i-dzien.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M3b (planer), M3c (GD, status), M1 (`render`). |
| **Pakiety robocze** | WP10, WP11, WP12, WP13 |
| **Projekt techniczny** | §5.9, §5.11, §5.12 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: mieszkańcy chodzą do pracy i sklepu, karta inspekcji pokazuje plan obok realizacji. |
| **Kryterium zamknięcia** | Kryteria WP10–WP13 oraz bramki 1–7 fazy M3 w `00-postep.md`. |
| **Poprzednia / następna** | `M3c-demografia-i-spoleczenstwo.md` · — (ostatnia w fazie) |

Etap 8 generacji — pełny generator populacji z czterema dopasowaniami — oraz szkielet `engine/ui`, karta inspekcji z osią dnia i domknięcie testowe fazy.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| ✅ WP10 | Generacja populacji (Etap 8) | WP7, WP9, M2 (lokale), M1/M2 Etap 7 (miejsca pracy) | L |
| ✅ WP11 | `engine/ui` — szkielet, selekcja, sterowanie czasem | M1 (`render`) | M |
| ✅ WP12 | Karta inspekcji z osią czasu dnia | WP5, WP11 | M |
| ✅ WP13 | Testy własnościowe, determinizm, benchmarki | wszystkie | M |

✅ ukończone. Graficzna połowa WP11 i WP12 domknięta korektą H-28: `egui` jest wpięte
w klatkę `engine/render`, bufor ID odpowiada na pytanie „kto jest pod kursorem",
piesi są rysowani, a klient zaludnia miasto i przewija dobę. Decyzje 9.2 i 9.3
**wykonane**

### WP10 — Generacja populacji (Etap 8)

Pełny generator: piramida wieku, składanie GD, dopasowanie praca↔mieszkaniec, dopasowanie dochód↔wartość mieszkania, dopasowanie histogramu czasu dojazdu.

**Kryterium ukończenia:** cztery testy dopasowania z sekcji 7 zielone dla 10 seedów × 4 rozmiary miasta; generacja 400 tys. mieszkańców ≤ 30 s.

### WP11 — `engine/ui` szkielet

Integracja biblioteki IMGUI z `engine/render`, selekcja encji (raycast → bufor ID), `TimeControlsWidget`.

**Kryterium ukończenia:** pauza/1×/3×/10× działa i nie wpływa na wynik symulacji (1 doba przy 1× i przy 10× → identyczny hash); kliknięcie w pieszego daje `CitizenId`.

### WP12 — Karta inspekcji z osią czasu dnia

Panel: tożsamość, potrzeby, majątek, oś czasu 0–24 h z planem i realizacją, `DecisionReason` pod kursorem, renderer tekstowy dzielony z testem.

**Kryterium ukończenia:** złoty test wydruku; każdy slot planu ma niepusty powód; realizacja (faktyczne zdarzenia DES) rysowana obok planu, rozjazdy widoczne.

### WP13 — Testy, determinizm, benchmarki

Sekcja 7 w całości + dopisanie komponentów `sim/agents` do hasha stanu.

**Kryterium ukończenia:** CI zielone, benchmarki w `criterion` z progami regresji.

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.9 Generacja populacji — Etap 8 (§4.2)

Wejście z M2 / Etapu 6–7: `HousingUnit { building, unit, area_m2, value: Money, district }` oraz `JobSlot { site, role, shift, wage_band, district }`.
Parametry: `seed`, `target_population`, `epoch`, `unemployment_target`, `commute_median_min`.

```
Krok 1. Piramida wieku
  rozkład wiekowy epoki (data/epochs/<e>.ron) → N osób z wiekami; N = target_population
Krok 2. Składanie GD
  rozkład typów GD epoki → losuj typ, dobierz osoby zgodne wiekowo
  (para: Δwiek ~ N(2,4); rodzic–dziecko: Δ ≥ 18; wielopokoleniowa: 3 pokolenia)
  reszta → single / współlokatorzy
Krok 3. Wykształcenie i umiejętności
  edu_level ~ f(wiek, klasa zalążkowa GD, epoka); edu_field ~ f(edu_level)
  Skills: 1–3 sloty, poziom ~ f(wiek − wiek wejścia na rynek, dopasowanie edu_field↔role)
Krok 4. Osobowość
  8 cech ~ rozkład Beta, słabo skorelowany z edu i klasą (macierz korelacji w data/, nie w kodzie)
Krok 5. Dopasowanie pracy   CEL: |JobSlot| ≈ |aktywni| × (1 + unemployment_target)
  aktywni = wiek ∈ [wiek_startu(epoka), wiek_emerytury(epoka)] i nie uczy się
  jeśli |JobSlot| < potrzeba → dokładnie tylu aktywnych zostaje bezrobotnych, ilu trzeba
  dopasowanie zachłanne po malejącym fit = skill_match(role, Skills, edu_field)
  remisy rozstrzygane po entity_index — deterministycznie
  grafik: ShiftKind z JobSlot; work_days jako bitmaska DayOfWeek (K-15) wg profilu branży
  (biuro pn–pt; handel i usługi z obsadą weekendową; ruch ciągły 4-brygadowy pn–nd)
  — maska jest własnością obsady, nie kalendarza: tydzień dryfuje względem miesiąca
Krok 6. Dopasowanie mieszkań
  dochód GD z wage_band członków → decyl dochodu
  HousingUnit.value → decyl wartości
  sparuj monotonicznie decyl↔decyl z szumem (kopuła: p(przesunięcia o k decyli) ∝ exp(−k))
Krok 7. Dopasowanie czasu dojazdu
  cel: histogram t_dojazdu ~ lognormal(mediana = commute_median_min, σ z epoki)
  pętla poprawkowa: 200 tys. deterministycznych prób zamiany mieszkań MIĘDZY GD
  o tym samym decylu wartości; zamiana przyjęta, jeśli zmniejsza χ² histogramu
  (kolejność prób z rng(seed, StreamId::PopGen, 0, iteracja))
Krok 8. Zasiew wiedzy (§5.7)
  miejsca w promieniu 400 m od domu (score 55, Visited)
  miejsca w buforze trasy dom↔praca (score 35, SeenOnRoute)
  miejsce pracy (score 80, Visited)
Krok 9. Relacje startowe
  rodzina (z kroku 2), 3–8 współpracowników z tego samego site,
  2–5 sąsiadów z tego samego budynku/kwartału
Krok 10. Weryfikacja (podzbiór Etapu 10) — sekcja 7.4
```

Zrównoleglenie: kroki 1–4 po chunkach osób; kroki 5–7 po dzielnicach (praca i mieszkanie są w ~90% lokalne); krok 7 finalizowany jednowątkowo. Cel: **400 tys. mieszkańców ≤ 30 s**, 120 tys. ≤ 8 s.

`ponytail:` krok 7 to zwykła zachłanna wymiana sterowana χ², nie symulowane wyżarzanie ani transport optymalny. Sufit znany: przy bardzo nierównomiernym rozkładzie miejsc pracy zbieżność może nie zejść poniżej 5% błędu mediany — wtedy podmieniamy na wyżarzanie. Nie wcześniej.

### 5.11 `engine/ui` — szkielet i karta inspekcji

Moduły crate'a `engine/ui` tworzone w M3 (M9 dołoży panele biznesowe, M11 — styl):

```
engine/ui/
  lib.rs           — kontekst UI, pętla klatki, wpięcie w engine/render
  input.rs         — routing zdarzeń wejścia, hit-test, priorytet UI nad kamerą
  selection.rs     — Selection { Citizen | Building | Parcel | None }, raycast → bufor ID
  time.rs          — TimeControlsWidget (pauza, 1×, 3×, 10×), kalendarz 12×30, „zatrzymaj przy X"
  inspect/
    mod.rs         — trait InspectorPanel { fn draw(&mut self, ui, world) }
    citizen.rs     — karta mieszkańca
    timeline.rs    — widget osi czasu dnia (plan + realizacja)
    reason.rs      — render DecisionReason → tekst (jedna funkcja, jedno źródło prawdy)
  text_export.rs   — ten sam widok w formie tekstowej (test akceptacyjny i konsola dev)
```

**Sterowanie czasem.** M3 dostarcza wyłącznie widget; zegar (`SimClock`, `SimSpeed`) należy do `engine/core` z M0 (decyzja 9.1). Widget ustawia `SimSpeed ∈ {Paused, X1, X3, X10}`. Mapowanie: 1× = 1 minuta gry na sekundę realną (doba = 24 min realne). 50× i tryb makro — M12. Kalendarz rysuje rok 360-dniowy: 12 miesięcy × 30 dni, sezon = kwartał (K-1).
Twardy wymóg: **prędkość nie wpływa na wynik**. Test: doba przy 1× i doba przy 10× → identyczny hash stanu.

**Karta inspekcji mieszkańca** (kolejność od góry):

1. Nagłówek: imię i nazwisko, wiek, zawód, adres — dokładnie jak nagłówek wydruku §5.5.
2. Paski 12 potrzeb, kolor od poziomu, tooltip = tempo spadku i skutek deprywacji.
3. Majątek: gotówka osobista + budżet GD (w M3 tylko odczyt pól).
4. **Oś czasu dnia 0–24 h** — dwie ścieżki:
   - *plan* — bloki z `DayCanvas`, szerokość = `dur_min`, kolor = `ActivityKind`;
   - *realizacja* — bloki z faktycznych zdarzeń DES (`StartActivity` → `Arrive`/`EndActivity`), rysowane pod planem; rozjazd > 10 min podświetlony.
5. Pod kursorem bloku: pełny `DecisionReason` z `plan_day_explained` (odtwarzany na żądanie, sekcja 5.4).
6. Relacje: 8 najsilniejszych, klik → przeskok inspekcji.
7. Ostatnie decyzje: 10 ostatnich `Replanned`/`Deprivation`/`Arrived` z bufora śledzenia (tylko dla mieszkańca w trybie „śledź", §14.4, decyzja 9.16).

**Renderer tekstowy** (`text_export.rs`) produkuje wydruk w układzie z PRD §5.5 — godzina, akcja, miejsce, w nawiasie uzasadnienie z `DecisionReason`. Jest **tym samym kodem**, co warstwa etykiet widgetu (jedna funkcja `describe(slot, reason) -> String`), więc złoty test w CI faktycznie broni UI, a nie osobnej ścieżki.

### 5.12 Systemy ECS i ich częstotliwość

| System | Częstotliwość | Odczyt | Zapis | Uwagi |
|---|---|---|---|---|
| `DesDispatchSystem` | `EveryMinute` | `EventQueue` | `AgentState`, `PlanRef.cursor` | rdzeń pętli; dispatch równoległy po chunkach |
| `DayPlannerSystem` | `EveryMinute` (obsługa zdarzeń `PlanDay`) | wszystko o mieszkańcu | `DayPlanArena`, `PlanRef` | zdarzenie `PlanDay` rozrzucone 18:00–02:00 |
| `ReplanSystem` | `EveryMinute` (na sygnał) | j.w. | `DayPlanArena` | debouncing 15 min, limit 20 tys./tick |
| `NeedDecaySystem` | `EveryMinute`, shard 1/60 | `Vitals`, dane RON | `Needs` | 6,7 tys. encji/tick przy 400 k |
| `NeedSatisfySystem` | `EveryMinute` | `PlaceProvider` | `Needs`, `Household.stock` | wołany ze zdarzenia `StartActivity` |
| `DeprivationEffectsSystem` | `EveryHour`, shard 1/60 | `Needs` | `Vitals` | energia, zdrowie, nastrój, stres |
| `WalkMicroSystem` | 100 ms (LOD Mikro) | `PedestrianBuffer` | pozycje | wyłącznie wizualne |
| `SkillDriftSystem` | `EveryDay`, shard 1/7 | `Employment`, `AgentState` | `Skills` | wzrost przez pracę, zanik przez nieużywanie |
| `RelationDecaySystem` | `EveryDay`, shard 1/7 | — | slab relacji | |
| `GossipSystem` | `EveryDay`, shard 1/7 | relacje, wiedza | slab wiedzy | |
| `HouseholdStockSystem` | `EveryDay` | `Household` | `Household.stock` | M5 zastąpi realną konsumpcją |
| `DemographySystem` | `EveryDay`, shard **1/360** (hazardy roczne, K-1) | wszystko | bufor komend | narodziny, choroba, śmierć |
| `HouseholdLifecycleSystem` | `EveryMonth` (30 dni) | relacje, status | bufor komend | pary, śluby, rozwody, podziały GD |
| `StatusSystem` | `EveryMonth`, shard 1/30 | dochód, majątek, edu, adres | `Vitals.status` | |
| `MigrationSystem` | `EveryMonth` | wakaty, pustostany | bufor komend | jedyny regulator populacji |

Wszystkie mutacje strukturalne (narodziny, śmierć, wejście/wyjście GD) idą przez bufor komend sortowany po `(SystemId, entity_index)` — doc 00 §3.4.

---

---

## Zmiany wpisane po M3a

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3a.

| # | Zmiana | Dlaczego |
|---|---|---|
| E-1 ★ | **Etap 8 wypełnia `PlaceTable`** — katalog `(PlaceRef, PlaceKind, WorldCoord)` — z budynków i zakładów M2, i podaje go `InfinitePlaces` oraz `WalkOracle` | `sim/agents` nie zależy od `sim/world` (decyzja 9.11 mówi, że zależność idzie w drugą stronę), więc most między miastem a agentami buduje generator populacji, który siedzi w `sim/world`. Bez tego katalogu żaden mieszkaniec nie znajdzie sklepu (korekta D-8) |
| E-2 ★ | **Zasiew wiedzy jest warunkiem, żeby cokolwiek się wydarzyło**, a nie ozdobą: `candidates` zwraca **wyłącznie** miejsca znane mieszkańcowi (§5.7), więc mieszkaniec bez wpisów w `KnowledgeSlab` dostaje `PlaceUnknown` i nigdzie nie idzie | Tak ma być — to jest treść §5.7 i test `mieszkaniec_bez_wiedzy_nie_teleportuje_sie_do_sklepu`. Konsekwencja dla kroku generacji: każdy mieszkaniec musi dostać zasiew wiedzy o miejscach w swojej okolicy, inaczej scenariusz `m3_day` pokaże miasto stojące w miejscu |
| E-3 ★ | **Pętla zdarzeń potrzebuje właściciela.** W M3a kolejkę przewija runner scenariusza (`tools/headless agents`), bo nie ma jeszcze handlerów — systemem ECS staje się dopiero wtedy, gdy planer ma czym dyspozytorować | Do rozstrzygnięcia razem z §5.12 (systemy i ich częstotliwość): czy `EventPumpSystem` jest osobnym systemem `EveryMinute`, czy dispatch siedzi w systemie planera. M3a nie przesądza, bo nie ma podstaw |
| E-4 | **Scenariusz `tools/headless agents` istnieje** (populacja syntetyczna, spadek potrzeb, koło czasu, hash co N ticków). `m3_day` i `m3_century` mogą go rozszerzyć zamiast zaczynać od zera | 400 tys. mieszkańców zaludnia się w 0,8 s, doba gry biegnie w 0,9 s na 16 wątkach — jest z czego wyjść |
| E-5 | **Karta inspekcji ma gotowe źródło powodów deprywacji:** `needs::deprivation_of(needs, table, out)` zwraca `DecisionReason::Deprivation` per potrzeba, liczone na żądanie | Ta sama zasada co przy `plan_day_explained`: pełnych logów dla 400 tys. mieszkańców nikt nie utrzyma, a odtworzenie jest dokładne (00 §7) |

---

## Zmiany wpisane po M3b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3b.

| # | Zmiana | Dlaczego |
|---|---|---|
| E-6 ★ | **`PlanCtx` wymaga `home`, `work` i `school` jako `PlaceRef`** — Etap 8 musi je wypełnić razem z `PlaceTable`. `Residence.building` i `Employment.site` to **indeksy encji bez generacji**, więc nie da się z nich odtworzyć `PlaceRef` | Generacja populacji jest jedynym miejscem, które zna jednocześnie encje budynków M2 i przypisanie mieszkańców, więc most musi powstać tam (to samo rozstrzygnięcie co E-1 dla `PlaceTable`). Bez tego planer nie ma skąd wziąć domu ani pracy |
| E-7 ★ | **`WalkOracle::with_streets(places, nodes, segments)` czeka na centrolinie z M2.** `nodes` to pozycje `RoadNetwork.nodes[].pos` przeliczone na `WorldCoord` (centymetry), `segments` to trójki `(a, b, length_dm × 10)` | Bez sieci `WalkOracle::new` liczy manhattanowo z korektą 1,25× (fallback z R7) i histogram czasu dojazdu z kroku 8 Etapu 8 mierzy nie to, co ma mierzyć. Konwersja jest jedną pętlą po `RoadNetwork` i należy do Etapu 8, bo `sim/agents` nie zależy od `sim/world` |
| E-8 ★ | **`render_day_text` z §5.11 stoi na `DayCanvas` + `ReasonLog` z `sim/agents`** i jest rendererem **dla gracza**: `LocKey` plus wpisy w `pl.ron` **i** `en.ron`. Diagnostyczny `planner::render_day_debug` zostaje jako artefakt testowy i wydruk runnera | Dwa formattery nad jednym źródłem, nie dwa źródła. Złoty test tekstu dla gracza (`plan_golden_anna` w formie z PRD §5.5) należy do tej podfazy, bo dopiero tu istnieje lokalizacja; wersja diagnostyczna ma już swój złoty plik (`sim/agents/tests/golden/anna_day.txt`) |
| E-9 ★ | **Wzorzec pętli zdarzeń dla §5.12 jest gotowy i sprawdzony** (`tools/headless day`): `StartActivity` slotu `k` harmonogramuje `EndActivity` slotu `k` **oraz** `StartActivity` slotu `k+1`. Łańcuch `EndActivity → StartActivity` **nie działa** | Sloty planu przylegają do siebie, więc koniec jednego i początek następnego wypadają w tej samej minucie — a kubełek tej minuty jest już rozdany (korekta D-14 do M3a). Kolejka odrzuca to teraz asercją zamiast gubić zdarzenie po cichu, ale system ECS musi być napisany od razu właściwie |
| E-10 ★ | **Zegar `EventQueue` stoi na minucie właśnie rozdawanej.** Handler, który chce „zaraz potem", planuje na `now + 1` | Bez tego `TravelOracle::begin_trip` liczyłby przybycie od minuty następnej i każda podróż trwałaby o minutę dłużej niż w planie — przy czterech dojazdach dziennie cztery minuty dryfu na mieszkańca na dobę. Dotyczy każdego systemu z §5.12 |
| E-11 | **Karta inspekcji ma czym pokazać wybór miejsca**: `ReasonLog::for_slot(i)` daje powody slotu (pierwszy to powód wstawienia), a `ReasonLog::skipped()` — decyzje, które **nie** wytworzyły slotu (pominięte zadanie, brak wiedzy, absencja) | PRD §5.5 chce w karcie obu rzeczy: co mieszkaniec zrobił i czego nie zrobił oraz dlaczego. Odtworzenie przez `plan_day_explained` kosztuje 0,84 µs, więc panel może je liczyć przy każdym otwarciu |
| E-12 | **Renderer pieszych ma gotowe wejście:** `WalkOracle::micro_snapshot(&mut Vec<(u32, [f32; 3], f32)>)` — indeks encji, pozycja w metrach, postęp trasy; plus `micro_step`, `micro_retire`, `micro_len`, `enter_micro` | Zrzut nie zdradza ani węzła, ani trasy, więc K-2 obowiązuje dalej. Zmierzone: 5 tys. pieszych to 47 µs na klatkę przy budżecie 2 ms |

---

## Zmiany wpisane po M3c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3c.

| # | Zmiana | Dlaczego |
|---|---|---|
| E-13 ★ | **Etap 8 stoi na `migration::spawn_household_aged`, a nie pisze własnego generatora.** Kroki 1–4 (piramida wieku, składanie GD, wykształcenie, osobowość) **nadpisują** komponenty tuż po spawnie; kroki 5–7 (praca, mieszkanie, dojazd) działają na `Vacancies` | §5.7 wymaga, żeby napływ migracyjny tworzył gospodarstwa **tym samym kodem** co Etap 8. Kod ten powstał w M3c, bo bez niego migracja nie miała czym przyjmować ludzi. Drugi generator obok rozjechałby się z pierwszym przy pierwszej zmianie w `Household` |
| E-14 ★ | **Etap 8 wypełnia `Vacancies` i `CityFacts`, tak samo jak wypełnia `PlaceTable` (E-1).** `Vacancies::new(homes, jobs)` bierze `HomeSlot { building, unit, district, value }` z `Unit` M2 i `JobSlot { site, role, shift, work_days, district, wage_monthly }` z `Workplace` M2 (mediana `wage_band`). `CityFacts { job_prestige, district_score, block_of }` bierze prestiż z `data/jobs/`, percentyl wartości gruntu z `land_value_at` i przypisanie budynek → kwartał z podziału kwartałów M2 | `sim/agents` nie zna ani `data/jobs/`, ani geometrii miasta. Trzy tablice są **jedynym** wejściem, przez które funkcja statusu (§5.8) i sąsiedztwo widzą miasto; pusty `CityFacts` nie łamie ich, tylko spłaszcza — składniki dostają wartość neutralną 50, a kwartałem staje się sam budynek |
| E-15 ★ | **Krok 5 (dopasowanie pracy) i krok 6 (dopasowanie mieszkań) mają twardy wymóg lokalności**, którego §5.9 nie wypisywał: praca ma być w zasięgu dojazdu z domu. Bez tego relacja współpracownicza spina przeciwne końce miasta i plotka przeskakuje pięć kilometrów w jednym kroku | Zmierzone w M3c (korekta G-10): przy losowym przydziale pracy powyżej 5 km o nowym sklepie wiedziało 93 % mieszkańców zamiast poniżej 5 %, czyli kryterium WP9 padało nie z winy plotki, tylko z winy przydziału etatów. Krok 7 (histogram dojazdu) i tak to robi — chodzi o to, żeby **nie dało się** go pominąć |
| E-16 ★ | **§5.12: `DemographySystem`, `HouseholdLifecycleSystem`, `MigrationSystem`, `StatusSystem`, `RelationDecaySystem` i `GossipSystem` opakowują gotowe funkcje** `demography::step_day`, `demography::step_month`, `migration::step_month`, `social::step_month`, `social::step_day`. Kolejność w obrębie doby i miesiąca jest już rozstrzygnięta i zapisana w `society::step_day` | Kolejność nie jest dowolna: zgon przed migracją (zwolniony lokal jest pustostanem w tym samym miesiącu), status przed doborem partnera (kompatybilność czyta status), odbudowa indeksu kontaktów po migracji. Systemy ECS mają **wywołać** tę kolejność, a nie wymyślić własną |
| E-17 ★ | **`StatusSystem` nie jest shardowany 1/30** wbrew tabeli §5.12 | Status stoi na percentylu, a percentyl liczy się z całego rozkładu naraz — przy shardzie dwie osoby o identycznym dochodzie miałyby przez miesiąc różny status (korekta G-4) |
| E-18 | **Karta inspekcji ma gotowe rozbicie statusu:** `social::StatusBreakdown` daje siedem składników w skali 0..=100 **przed** przemnożeniem przez wagę, plus wynik. Decyzje demograficzne wracają z raportów doby i miesiąca jako `(indeks encji, DecisionReason)` | Ta sama zasada co przy `LandValueBreakdown` z M2: wynik pokazuje się z rozbiciem na czynniki, a nie jako liczba, która spadła z nieba (00 §7). Pozwala napisać „mieszka dobrze, ale zarabia słabo" zamiast „ma 47" |
| E-20 ★ | **Budżet Etapu 8 jest już w połowie zjedzony przez sam zasiew.** `migration::seed_population` tworzy 200 tys. gospodarstw (421 tys. mieszkańców) w **19,5 s** wobec 30 s z `gen_perf`. Wąskie gardło jest jedno i nazwane: `Vacancies::take_job_in` szuka etatu w dzielnicy liniowo od końca listy | Dopasowanie pracy (krok 5) i mieszkań (krok 6) i tak przepisują ten krok pod swoje cztery kryteria statystyczne, więc naturalnym miejscem na indeks wolnych miejsc per dzielnica (`BTreeMap<u16, Vec<usize>>`) jest Etap 8, a nie M3c. Liczba jest tu po to, żeby M3d zaczął od niej, a nie odkrył jej po napisaniu dziesięciu kroków |
| E-19 | **`HouseholdView` dla planera buduje się z `Household` + `household::roles`:** `stock` wprost z komponentu, `escorts` i `pickups` z ról (kto odprowadza rano, kto odbiera po południu), przetłumaczonych z indeksów encji dzieci na `PlaceRef` szkół — tego tłumaczenia `sim/agents` nie umie zrobić, bo nie zna katalogu miejsc | `roles` zwraca **indeksy encji** odprowadzanych dzieci; szkoła każdego z nich siedzi w jego `Employment.site` (uczeń ma flagę `FLAG_PUPIL`). Most `site → PlaceRef` jest ten sam co przy `home`/`work` w `PlanCtx` (korekta E-6) |

---

## Zmiany wpisane po M3d

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Korekta | Dlaczego |
|---|---|---|
| H-1 ★ | **Liczba mieszkańców wynika z miasta, a nie z parametru.** `PopulationParams.target_population` jest `Option` i domyślnie `None`; Etap 8 liczy ją z liczby etatów i piramidy wieku, a ogranicza liczbą lokali | Kryterium `gen_jobs_filled` mówi „liczba etatów ≈ liczba aktywnych × (1 + bezrobocie)". To jest równanie, nie nierówność: skoro etaty są dane, to one wyznaczają liczbę aktywnych, a piramida przelicza aktywnych na mieszkańców. Populacja zadana z zewnątrz spełniłaby to kryterium wyłącznie przypadkiem. Udział aktywnych liczy się z **części wspólnej** pasma pięcioletniego z przedziałem wieku produkcyjnego — zaokrąglenie pasma przesuwało wynik o 6 % przy tolerancji 2 % |
| H-2 ★ | **Krok 7 steruje medianą dojazdu, nie kształtem histogramu**, a żądana mediana jest **przycinana** do przedziału osiągalnego w tym mieście. Raport podaje żądanie, cel po przycięciu i zmierzony przedział | W M3 wszyscy chodzą pieszo (K-2), a praca jest przydzielana w granicach dzielnicy (E-15), więc rozkład czasów dojazdu jest własnością geometrii miasta, a nie parametrem do wybrania. Zmierzone mediany osiągalne: **18 min (4 km), 29–34 min (8 km), 61 min (16 km)** wobec 24 min z danych. Gorzej: minimalizacja χ² całego histogramu przesuwała medianę **w złą stronę** — nadrabiała ogon rozkładu kosztem środka, a kryterium mierzy właśnie medianę. χ² zostaje w raporcie jako diagnostyka kształtu; §7.4 nigdy nie podało na niego progu, a przy n ≈ 20 tys. żaden osiągalny fit nie przeszedłby testu p > 0,05 |
| H-3 ★ | **Potrzeba bez sposobu zaspokojenia ma tempo spadku 0** (`data/needs/needs.ron`: `Safety`, `Housing`, `Mobility`, `Status`). Niezmiennik pilnuje test `potrzeba_bez_sposobu_zaspokojenia_nie_spada` | Znalezione dopiero przy wielodobowym przebiegu, bo `m3_century` nie uruchamia spadku potrzeb. Mobilność spada 4,2 pkt/h, nic jej nie podnosi (`places: []`, `satisfaction: 0`) i ma `AbsenceRisk` **1000** — więc w drugiej dobie **całe miasto przestawało chodzić do pracy**. Bezpieczeństwo i mieszkanie robiły to samo wolniej, przez progi migracji (`safety_need_threshold`, `housing_need_threshold`). Tempo wpisze faza, która wnosi mechanizm: M4 (dostęp do trasy), M8 (przestępczość), M5/M9 (lokal, konsumpcja). Skutki i progi zostają nietknięte |
| H-4 ★ | **Zaspokojenie potrzeby dzieje się w `EndActivity`, nie w `StartActivity`** — wbrew tabeli §5.12. Przy okazji rozstrzygnięte: **czas wizyty nie liczy się do spadku tej potrzeby** | `satisfaction` w danych to **przyrost z jednej wizyty**, a wizyta daje go, gdy się skończy. Druga połowa jest w komentarzu danych: tempo snu opisano jako „4,2 pkt/h **na jawie**", a sen trwa osiem godzin — bez oddania spadku z czasu wizyty żadna potrzeba nie utrzymywała się powyżej zera. Rozwiązanie nie wymaga, żeby `NeedDecaySystem` wiedział, co mieszkaniec robi: spadek z czasu wizyty wraca razem z przyrostem |
| H-5 | `Hygiene.satisfaction` 35 → **100** | Jedna wizyta dziennie (planer wstawia poranną toaletę) przy spadku 100 pkt na dobę nie utrzymywała higieny nigdzie poza zerem, więc `StatusLoss` i `HealthLoss` działały u wszystkich naraz i przestawały cokolwiek różnicować. Kąpiel przywraca higienę w całości — to jest ten mechanizm, a nie kalibracja pod test |
| H-6 ★ | **`PlanDay` wypada w pierwszej minucie doby, którą planuje**, a nie „rozrzucone 18:00–02:00" dnia poprzedniego (§5.12) | Planowanie z wieczora na jutro wymaga **dwóch** planów naraz, a podwójne buforowanie areny planów zdjęto z M3 po pomiarze pamięci (korekta D-1, 288 B → 144 B na mieszkańca). Plan na jutro zamazywał plan wykonywany dziś, łańcuch zdarzeń wskazywał na sloty z innego dnia, a kolejka odrzucała je asercją. Koszt spiętrzenia jest zmierzony i mały: **0,22 s na dobę gry przy 274 tys. mieszkańców**, czyli raz na 24 minuty realnego czasu przy 1× |
| H-7 ★ | **`DayPlannerSystem`, `ReplanSystem` i `NeedSatisfySystem` nie są osobnymi systemami**, tylko obsługą zdarzeń wewnątrz `DayLoopSystem` (rozstrzyga E-3) | Kolejność zdarzeń w obrębie minuty wyznacza `order_key` i jest zamrożona (§6.4). Trzy systemy przechodzące kolejkę osobno musiałyby rozdać ją trzy razy i pogodzić trzy kolejności. Rozdaje ją jeden — i on obsługuje wszystkie rodzaje zdarzeń |
| H-8 ★ | **`engine/ecs` dostaje system wyłączny**: `SystemDesc::exclusive()` i `SystemCtx::world_mut()`. Wpisane jako **`K-21`** w dokumencie 00 | `society::step_day` i pętla doby są gotowymi funkcjami nad **całym** światem; rozpisanie ich na deklaracje dostępu nie kupiłoby ani jednej krawędzi DAG, bo i tak konfliktują z każdym sąsiadem. System wyłączny konfliktuje z każdym z definicji, więc ląduje sam na swoim poziomie i determinizm jest trywialny — nie ma z kim się ścigać |
| H-9 ★ | **`engine/core` dostaje `SimClock` i `SimSpeed`** (wykonanie decyzji 9.1). Wpisane jako **`K-22`** | Decyzja 9.1 przypisała zegar do M0, ale typów nie było — a bez nich nie da się pokazać, że prędkość nie wpływa na wynik. Zegar zamienia czas realny na **liczbę minut do wykonania**; każda minuta jest tym samym tickiem, więc niezmienniczość wynika ze struktury, a nie z dyscypliny |
| H-10 ★ | **`param` slotu fazy 2 niesie indeks potrzeby, nie jej poziom** (M3b §5.4) | Bez tego nie da się powiedzieć, co zaspokaja slot `Idle` wstawiony na poranną toaletę — rodzaj czynności tego nie mówi, a pełne uzasadnienie jest odtwarzane, nie przechowywane. Faza 3 pakowała indeks potrzeby od początku; faza 2 pakowała poziom, który i tak siedzi w `Needs`. Wydruk diagnostyczny czyta `param` tylko wtedy, gdy log jest pusty, więc złoty plik M3b się nie zmienił |
| H-11 | **Status liczy Etap 8 na końcu** (`social::step_month(world, 0)`) | Status jest percentylem, a percentyl wymaga rozkładu — więc dopiero po przydziale pracy i mieszkań. Bez tego kroku miasto startuje z zerowym statusem u wszystkich: karta inspekcji pokazuje metropolię złożoną z klasy niższej, a dobór partnera (§5.6) czyta tę liczbę przez cały pierwszy miesiąc. Potwierdza to E-17: sharding statusu 1/30 jest niewykonalny |
| H-12 | **Spóźnienie pieszego jest normą, nie rozjazdem planera z mezo.** Wyzwala `ReplanCause::Late` | Plan powstaje o północy, a prędkość marszu zależy od energii i zdrowia, które przez dobę spadają. Planer nie ma jak tego wiedzieć — i dokładnie po to istnieje `ReplanCause::Late` (§5.2). Zmierzone: **2,1 min średniego opóźnienia przy ok. 0,1 % przybyć**. Niezmiennik M3b („slot `Commute` trwa tyle, ile `TravelOracle` liczy dla jego pary miejsc") obowiązuje dalej, ale **w chwili planowania** |
| H-13 | `walk::dijkstra` idzie **A\*** z heurystyką euklidesową | Odległość w linii prostej jest dopuszczalna i spójna (odcinek ulicy jest łamaną, więc nigdy nie jest krótszy od odcinka między swoimi końcami), a wynik identyczny co do centymetra. Bez tego Etap 8 zjadał **25 s z budżetu 30 s** na samych dojazdach metropolii |
| H-14 | **`Vacancies::with_occupancy`** — pula po zasiedleniu: wolne miejsca osobno, pojemność miasta osobno | Odpowiedź na E-20 inna niż zapowiadana: Etap 8 **nie dotyka** `take_job_in`, bo rozdaje etaty własnym dopasowaniem z indeksem per dzielnica. Wąskie gardło znika przez obejście, a nie przez przepisanie puli — indeks w `Vacancies` zrobi M7 razem z rynkiem pracy, gdy będzie miał po co |
| H-15 ★ | ~~**Graficzna połowa WP11 i WP12 nie jest dostarczona**~~ — **zamknięte przez H-28**. Zapis zostaje, bo opisuje stan, w jakim podfaza była przez jeden commit, i wyjaśnia, skąd wziął się osobny pakiet pracy: **Graficzna połowa WP11 i WP12 nie jest dostarczona**: nie ma bufora ID w renderze, klikania w pieszego, rysowania pieszych w widoku 3D ani panelu `egui`. Dostarczone jest wszystko, co da się sprawdzić bez GPU: model karty, oś dnia z planem i realizacją, renderer powodów, lokalizacja, sterowanie czasem i selekcja z fallbackiem `ListPicker` (ryzyko R8) | To jest osobna praca po stronie `engine/render` (M1): nowy pass rysujący encje, drugi cel rysowania na identyfikatory, odczyt zwrotny piksela i wpięcie `egui-wgpu` w klatkę, której właścicielem jest `Renderer`. Kryterium „kliknięcie w pieszego daje `CitizenId`" **nie jest spełnione**; kryterium „pauza/1×/3×/10× nie wpływa na wynik" i cały WP12 poza rysowaniem — są. Decyzje 9.2 (`egui`) i 9.3 (bufor ID) **zostają otwarte** |
| H-16 | `data/buildings/*.ron` dostaje `place_kind`, `data/jobs/roles.ron` dostaje `prestige` | Etap 8 potrzebuje obu: pierwszego, żeby wiedzieć, **do czego** zakład służy mieszkańcom, drugiego jako składnika „zawód" funkcji statusu (E-14). Alternatywą był rosnący `match` po kluczu archetypu w `sim/world::population` — czyli dokładnie to, czego zakazuje SOLID-O z CLAUDE.md. Odwzorowanie zmienia się z epoką i profilem, nie z kodem |
| H-17 | Nowa tabela **`data/demography/population.ron`**: piramidy wieku per epoka, skład gospodarstw, rozkład wykształcenia, dopasowanie kierunku do roli, parametry osobowości, cel dojazdu, zasiew wiedzy i relacji startowych | §5.9 odsyłał po piramidę wieku do `data/epochs/<e>.ron`, a tam są **pierścienie zabudowy miasta** (M2), nie demografia. Rozkładów składu gospodarstw ani wykształcenia nie było nigdzie. Nowy plik opisuje **stan startowy**, `demography.ron` — **przejścia**; to jest granica między nimi |
| H-18 | Osobowość: rozkład **trójkątny** zamiast Beta, a korelacja z wykształceniem i klasą wyrażona dwoma przesunięciami średniej w danych | `ponytail:` sufit nazwany — trójkąt nie umie rozkładu dwumodalnego ani skośnego inaczej niż przez przesunięcie średniej. Beta wymagałaby własnej implementacji w `det_math` (K-6) i wejdzie wtedy, gdy M5 albo M10 pokaże cechę, dla której to ma znaczenie |
| H-19 | Zasiew wiedzy z trasy liczy się z **korytarza wokół odcinka dom↔praca**, a nie z odtworzonej trasy po ulicach | `places_on_route` kosztuje jedną Dijkstrę na mieszkańca — 90 µs × 270 tys. to 25 s z budżetu 30 s (§7.4 `gen_perf`). Sufit nazwany: korytarz nie widzi objazdu wokół rzeki. W **rozgrywce** trasą zajmuje się `places_on_route` i to ona rozdaje wiedzę z codziennych dojazdów; tu chodzi o zasiew startowy, którego i tak nikt nie widział |
| H-28 ★ | **Graficzna połowa WP11 i WP12 dostarczona**: `egui` + `egui-wgpu` w klatce (`engine/render::ui`), bufor ID jako **osobny przebieg z porównaniem głębi `Equal`** (`engine/render::pick`), piesi jako instancjonowane bryły, okno warstwy Mikro w `WalkOracle`, klient uruchamiający Etap 8 i systemy §5.12 (`tools/magnat::citizens`). Decyzje 9.2 i 9.3 wykonane, H-15 zamknięta | Trzy rzeczy okazały się inne, niż zakładała decyzja 9.3. **(a)** Bufor ID nie jest drugim celem koloru w passie głównym, tylko osobnym przebiegiem po nim: drugi cel kazałby dopisać wyjście do shadera terenu, wody i dalekiego terenu i płacić przepustowością za każdy piksel nieba, a przebieg z `Equal` dostaje przesłanianie za darmo z głębi, którą pass główny już ustalił. Cena: geometria w obu przebiegach musi być bit w bit ta sama, więc oba potoki stoją na jednym `vs_main` — i tego pilnuje test. **(b)** `pick(x, y)` nie czyta GPU w chwili kliknięcia, bo to `submit` + `map` + oczekiwanie, czyli zacięcie klatki na każdy klik. Klatka kopiuje **jeden piksel** spod kursora (256 B) i klik czyta wartość sprzed klatki; podświetlenie pod kursorem (M9) dostaje to samo za darmo. **(c)** Pieszy wchodzi w warstwę Mikro w chwili, gdy **zaczyna** podróż, więc okno musi być otwarte już przy przewijaniu doby — inaczej szczyt poranny jest niewidzialny: kto wyszedł o 7:40, ten o 8:15 jest już w drodze i drugiej szansy nie dostanie. Zmierzone na mieście 4 km (28,5 tys. mieszkańców, ziarno 1): o 7:45 warstwa Mikro ma 10 193 pieszych, w kadrze 764; o 8:20 — 69 i 2; w środku dnia zero, bo wszyscy są w pracy |
| H-27 | **`mesh_budget` (M1/§7.3) ma progi zależne od profilu**: meshing 0,8 ms w release i 24 ms w debug, materializacja 1,0 / 12 ms, agregat LOD3 1,5 / 16 ms | Trzeci po H-21 i H-25 test budżetu czasu, który oblewał na **czystym `master`** — sprawdzone w osobnym worktree na `c9864ea`: meshing 11,71 ms wobec progu 0,8 ms, agregat LOD3 8,16 ms wobec 1,5 ms. Te same testy w release przechodzą. `greedy_quads` i `aggregate` to ciasne pętle po wokselach, w których `opt-level = 0` kosztuje piętnastokrotność — budżet z planu opisuje grę, a gra chodzi w release. Wzorzec jest już ustalony przez H-21 i H-25; to, że trzeba go było zastosować trzy razy w jednej podfazie, mówi, że **każdy próg czasowy w repozytorium ma być pisany w obu profilach od razu** — inaczej `cargo test --workspace` jest czerwony u każdego, kto nie doda `--release`, i przestaje być bramką |
| H-26 | **Repozytorium dostaje `.gitattributes` z `eol=lf`**, a katalogi `tests/golden/**` są wyłączone z normalizacji | `core.autocrlf=true` jest domyślne na Windowsie, a repozytorium nie miało `.gitattributes` — więc każdy świeży klon przepisywał pliki tekstowe na CRLF. Dla kodu to kosmetyka; dla **złotego pliku** to czerwony test bez jednej zmiany w kodzie, bo renderer pisze `
`, a porównanie jest bajt w bajt. Złapane przypadkiem, gdy `git stash pop` przepisał `engine/ui/tests/golden/*` i dwa testy akceptacyjne padły. Dotyczy to tak samo złotego pliku M3b, który dotąd przeżył tylko dlatego, że nikt go od czasu zapisu nie odtworzył z indeksu |
| H-25 | **`bench_generate_4km` (M1/§7.3) ma próg zależny od profilu**: 2,5 s w release, 8 s w debug | Drugi po H-21 test budżetu czasu, który oblewał na **czystym `master`** — sprawdzone w osobnym worktree na `c9864ea`: 2586 ms wobec progu 2500 ms. Generator w release liczy pięć regionów w 1,7 s, w debug jeden region w 2,6 s, bo `opt-level = 0` dotyczy właśnie tego crate'u. Nagłówek testu obiecywał „regresje rzędu wielkości, a nie wahania obciążenia", a próg oddalony o 4 % od zmierzonego czasu mierzy dokładnie te wahania. W release nic się nie zmienia |
| H-24 | **Testy Etapu 8 generujące świat są `#[ignore]`**, jak `consistency.rs` z M2; CI puszcza je jawnie przez `--include-ignored` w profilu release | Jedna generacja miasta 4 km to sekunda w release i kilkanaście w debug, a sześć takich testów zamieniłoby `cargo test --workspace` w kilkuminutowe oczekiwanie. `cargo test` ma być narzędziem, po które sięga się co minutę — inaczej przestaje się po nie sięgać |
| H-23 ★ | **Zdarzenie DES jest ważne tylko wtedy, gdy pasuje do slotu, na który wskazuje** — minuta startu (albo końca) slotu jest kluczem obcym. Po przeplanowaniu łańcuch dostaje **nową kotwicę**: pierwszy slot zaczynający się po bieżącej minucie | Przeplanowanie przyrostowe przestawia indeksy slotów **za** minutą, od której liczy, a w kolejce siedzą już zdarzenia z indeksami sprzed niego — po przeplanowaniu wskazywały na cudzą czynność albo poza plan, i łańcuch mieszkańca urywał się do najbliższego `PlanDay`. Przy 22 tys. przeplanowań na dobę (28 tys. mieszkańców) to jest różnica między „miasto ma przerwy" a „miasto ma dziury". Klucz obcy jest darmowy: minuta startu i tak siedzi w slocie. Przy okazji `store_plan` w ścieżce przeplanowania nie zeruje już kursora — doba jest w połowie |
| H-22 ★ | **`gen_age_pyramid` ma próg dla p = 0,001 (χ² < 46,8), a nie p = 0,05 (χ² < 32,7)** | Kryterium w brzmieniu „χ² względem piramidy epoki, p > 0,05" jest **niespełnialne dla poprawnego generatora**, gdy §7.4 każe je przepuścić przez macierz 10 ziaren × 4 rozmiary: test na poziomie α = 0,05 odrzuca prawdziwą hipotezę w 5 % przebiegów z definicji, więc na 40 przebiegach dwa fałszywe alarmy są **wartością oczekiwaną**. Zmierzone: ziarno 12 daje 33,2 przy progu 32,7 — czyli dokładnie taki alarm. Poprawka Bonferroniego na 40 porównań daje α = 0,00125; próg 46,8 (p = 0,001) jest nieco ostrożniejszy i zostawia test ostrym: rozkład przesunięty o 2 ‰ w każdym paśmie daje przy 20 tys. mieszkańców χ² rzędu stu |
| H-21 | **`bench_gossip_day` (M3c/WP9) ma próg zależny od profilu**: 200 ms w release, 2 000 ms w debug | Test oblewał na **czystym `master`**, zanim M3d czegokolwiek dotknął: próg 400 ms stawiano wobec 63,6 ms zmierzonych w release (korekta A-20), a w debug ta sama doba trwa **725 ms**. Nagłówek pliku obiecywał „zapas rzędu wielkości" — sześciokrotny nim nie jest. Bramką regresji pozostaje criterion; ten test pilnuje rzędu wielkości i ma to robić w obu profilach |
| H-20 | Raport Etapu 8 podaje **liczbę mieszkańców bez zasiewu wiedzy** i czas każdego kroku | Mieszkaniec, którego dom nie ma w promieniu 400 m ani jednego miejsca zaspokajającego potrzebę, zna tylko pracę — i to jest treść §5.7, nie błąd (wiedzę dostanie od sąsiadów przez plotkę). Ale udział takich ludzi trzeba **widzieć**: jeden procent to przedmieście, połowa to miasto stojące w miejscu. Zmierzone: **1,1 % na mapie 4 km** |

### Zmierzone (dla §7.4 i §7.5)

| Miara | Próg | Zmierzone |
|---|---|---|
| Etap 8, metropolia 16 km (274 tys. mieszkańców) | 400 tys. ≤ 30 s | **26,9 s** — 16 km to największe miasto, jakie M2 stawia, a 400 tys. mieszkańców nie ma z czego zrobić (175 tys. lokali, 189 tys. etatów) |
| Etap 8, miasto 8 km (81 tys. mieszkańców) | 120 tys. ≤ 8 s | **3,1 s** |
| Doba gry, 81 tys. mieszkańców, 8 wątków | 120 tys. ≤ 3 s | **1,91 s** (≈ 2,8 s po przeskalowaniu do 120 tys.) |
| 30 dób gry, 81 tys. mieszkańców | — | 258 s, **68,5 mln zdarzeń DES**, bez paniki i bez wycieku |
| Dopasowania §7.4 — macierz ziaren | wszystkie | zielone: **10 ziaren × 4 km**, 5 × 8 km, 2 × 12 km, 1 × 16 km |
| Hash stanu przy 1×, 3× i 10× | identyczny | identyczny |
| Hash stanu na 1, 8 i 16 wątkach | identyczny | identyczny |
