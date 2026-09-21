# M3 — Ludzie i dzień

Właściciel crate'ów: `sim/agents`, `engine/ui` (szkielet + karta inspekcji).
Rozszerza: `sim/world` (populacja).
Kontrakty bazowe: `00-konwencje-i-kontrakty.md` (typy, determinizm, LOD, K-1 kalendarz, K-2 własność `engine/nav`, K-4 bloki `StreamId`, K-8 słowniki domenowe w `core`, K-12 `DecisionReason` w `core`, K-15 tydzień). Nic z tamtego dokumentu nie jest tu redefiniowane.

**Kalendarz (K-1):** rok gry = 360 dni = 12 miesięcy × 30 dni, bez lat przestępnych. Wszystkie hazardy roczne, wieki, emerytury i shardowania w tym dokumencie liczą się w tej skali. 100 lat gry = 36 000 dni.

**Tydzień (K-15):** tydzień jest 7-dniowy i **dryfuje** względem miesiąca (30 ∤ 7) — pierwszy dzień roku wypada co roku w inny dzień tygodnia i to jest zamierzone. `DayOfWeek` pochodzi z `SimCalendar` w `engine/core` (absolutny numer doby mod 7); M3 **nie definiuje własnego** typu ani własnej arytmetyki tygodnia. Planer dnia (5.4), grafiki zmianowe (5.1 `ShiftKind`), godziny otwarcia miejsc (`OpenHours`) i generacja obsady (5.9 krok 5) stoją na `DayOfWeek`, nie na dniu miesiąca.

**Routing (K-2):** właścicielem `engine/nav`, w tym grafu pieszego, jest **M4**. M3 **nie projektuje własnego API routingu** — buduje minimalny estymator odległości i czasu dojścia **za interfejsem `TravelOracle`**, który M4 przejmuje i zastępuje.

**Słowniki domenowe (K-8, K-12):** `TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind` oraz `DecisionReason` mieszkają w `engine/core` (M0) — słownik i konwersje, zero logiki, bez `#[non_exhaustive]`. M3 **wnosi do nich warianty**, nie posiada tych typów. Powód: inaczej powstaje cykl `sim/agents` → `sim/traffic` → `sim/economy` → `sim/agents`.

---

## 1. Cel fazy i artefakt końcowy

Po M3 uruchamiamy miasto z M2 **zaludnione**: mieszkańcy mają tożsamość, osobowość, potrzeby, gospodarstwo domowe, pracę i plan dnia; chodzą pieszo do pracy i do sklepu; miasto żyje rytmem doby.

Artefakty, które da się uruchomić i zobaczyć:

1. **`tools/headless --scenario m3_day`** — 30 dni gry na 120 tys. mieszkańców bez GPU; na wyjściu: histogram wykorzystania czasu doby, rozkład czasu dojazdu, poziomy potrzeb, log zdarzeń DES, hash stanu co 1000 ticków.
2. **`tools/headless --scenario m3_century`** — 100 lat gry (36 000 dni) w trybie demograficznym; raport: piramida wieku co 10 lat, saldo narodzin/zgonów/migracji, populacja nie wymiera i nie eksploduje.
3. **Aplikacja z GPU** — miasto z M2 + poruszający się piesi, sterowanie czasem (pauza, 1×, 3×, 10×), kliknięcie w mieszkańca otwiera **kartę inspekcji z osią czasu dnia**.
4. **Wydruk planu dnia** (test akceptacyjny) — ten sam renderer, który rysuje oś czasu w UI, ma tryb tekstowy dający wydruk w formie z PRD §5.5 (wzorzec „Anna Wiśniewska"). Wydruk jest złotym testem w CI.

Definicja ukończenia fazy: wszystkie kryteria z sekcji 7 zielone, komponenty `sim/agents` dopisane do funkcji haszującej stan ECS.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zakres w M3 |
|---|---|
| Model mieszkańca | tożsamość, 8 cech osobowości, cechy dynamiczne, wykształcenie, umiejętności, **majątek jako pola danych** (bez operacji) |
| Gospodarstwa domowe | typy GD, skład, wspólne mieszkanie, zapasy GD (abstrakcyjne „dni zapasu"), podział ról (kto robi zakupy) |
| Cykl życia i demografia | narodziny, dobór partnera, związek, rozwód, starzenie, choroba, śmierć, **dziedziczenie jako mechanika** (przeniesienie `Money` i mieszkania) |
| Migracja | wprowadzanie/wyprowadzanie GD jako regulator populacji |
| Potrzeby | 12 potrzeb z §5.3, tempa spadku, skutki deprywacji, zaspokajanie przez `PlaceProvider` |
| Status i klasy | funkcja statusu, przedziały klas, mobilność społeczna |
| Relacje i pamięć | graf relacji (ważony, cappowany), magazyn 32 doświadczeń/wiedzy, plotka (§5.7) |
| Planer dnia | 4 fazy priorytetów, `DecisionReason` dla każdego wyboru, replanning |
| DES | koło czasu zdarzeń, deterministyczne rozstrzyganie remisów, dispatch |
| Generacja populacji | Etap 8 w całości (dopasowanie miejsc pracy, piramida wieku, dochód↔wartość mieszkania, rozkład czasu dojazdu) |
| Ruch pieszy | mezo (czas przybycia) + mikro (pozycja na polilinii), **za `TravelOracle`**, bez własnego API routingu |
| `engine/ui` | szkielet IMGUI, selekcja encji, sterowanie czasem, karta inspekcji mieszkańca z osią czasu |

### Nie wchodzi

| Czego nie robimy | Kto to robi |
|---|---|
| Funkcja użyteczności zakupu (§6.4), ceny, realne wydawanie budżetu, sklep z magazynem | **M5** — podmienia implementację `PlaceProvider` |
| **Graf nawigacyjny, CH, routing jako moduł** (`engine/nav`) | **M4** (K-2) — M3 ma tylko prywatny estymator za `TravelOracle` |
| Wybór środka transportu, samochód, paliwo, parking, korki | **M4** — podmienia implementację `TravelOracle`; M3 zostawia `TransportMode` w `PlanSlot` |
| Rynek pracy, aplikowanie do firm, pensje emergentne, rotacja | **M7** — w M3 przypisanie pracy jest **statyczne z generacji** i zmienia się tylko przez zdarzenia demograficzne |
| Marka, reklama, rekomendacje płatne | **M10** — M3 dostarcza tylko magazyn doświadczeń, na którym marka później stoi |
| Pełne panele biznesowe, automatyzacja polityk | **M9** |
| Dziedziczenie **firm**, giełda, aktywa | **M7 / M10** — M3 emituje hook `on_inheritance` i przenosi tylko `Money` + mieszkanie |
| Rynek nieruchomości, kredyt hipoteczny, kupno mieszkania | **M5 / M9** — M3 zna tylko przypisanie „GD ↔ lokal" i pustostany |
| LOD Makro (agregaty dzielnica × klasa) | **M10 / M12** — M3 implementuje Mikro i Mezo |
| Zapis/odczyt pełnego stanu, migracje schematu | **M0 (minimalny) / M12 (pełny)** — M3 tylko rejestruje swoje komponenty |
| Szkoły, przedszkola, służba zdrowia jako instytucje z pojemnością | **M8** — M3 traktuje je jako `PlaceProvider` (nieskończone) |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M3 |
|---|---|
| §5.1 Model mieszkańca | całość — komponenty `Identity`, `Personality`, `Vitals`, `Skills`, `Wealth`, `Relations`, `Knowledge` |
| §5.2 Rodziny i GD | całość poza budżetem wydawanym realnie (pola są, operacje w M5) |
| §5.3 Potrzeby | całość: tabela temp spadku i skutków deprywacji; koszyki dóbr **tylko jako `NeedKind → PlaceKind`**, produkty w M5 |
| §5.4 Status i klasy | całość |
| §5.5 Tryb dnia | całość planera; wybór miejsca przez `PlaceProvider` (M5 podmienia), transport przez `TravelOracle` (M4 podmienia) |
| §5.6 Decyzje długoterminowe | **nie w M3**, poza tym co wymusza demografia (zmiana GD → zmiana mieszkania) |
| §5.7 Informacja i plotka | całość — magazyn wiedzy, propagacja przez relacje, widoczność z trasy |
| §4.2 Etap 8 | całość — generator populacji |
| §4.2 Etap 10 | podzbiór dotyczący ludzi (każdy mieszkaniec ma dom, bezrobocie w celu) |
| §14.1 | `DecisionReason` w karcie inspekcji — DoD fazy |
| §14.4 | tryb „śledź": oś czasu dnia z planem i realizacją, panel potrzeb, ostatnie decyzje |
| §14.5 | pauza, 1×, 3×, 10× (50× / tryb makro — M12) |
| §17.3 | silnik DES, kolejka czasu, replanning |
| §17.4 | Mikro/Mezo dla agentów; zasada „wynik nie zależy od LOD" |
| §17.7 | budżet pamięci ~400 B/mieszkańca + osobny magazyn doświadczeń |
| §19 M3 | zakres kamienia milowego |

---

## 4. Pakiety robocze i podfazy

Kolejność topologiczna. „Zależy od" oznacza twardą zależność kompilacyjną lub testową.

Faza jest rozbita na **4 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| ✅ **M3a — Fundament agenta** | WP1, WP2, WP3, WP4 | 5.1, 5.2, 5.3, 5.5 | Headless: 400 tys. mieszkańców w pamięci, potrzeby spadają zgodnie z tabelą, koło czasu rozdaje zdarzenia. | `M3a-fundament-agenta.md` |
| ✅ **M3b — Dzień mieszkańca** | WP5, WP6 | 5.4, 5.10 | Wydruk dnia wzorcowego: pobudka → posiłek → dojazd → praca → zadanie po drodze → dom → czas wolny → sen; pieszy dociera na czas. | `M3b-dzien-mieszkanca.md` |
| ✅ **M3c — Demografia i społeczeństwo** | WP7, WP8, WP9 | 5.6, 5.7, 5.8 | `m3_century` — 100 lat gry headless bez wybuchu ani wygaszenia populacji. | `M3c-demografia-i-spoleczenstwo.md` |
| 🔶 **M3d — Populacja i UI** | WP10, WP11, WP12, WP13 | 5.9, 5.11, 5.12 | Pełny artefakt fazy z §1 dokumentu fazy: mieszkańcy chodzą do pracy i sklepu, karta inspekcji pokazuje plan obok realizacji. | `M3d-populacja-i-ui.md` |

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | Komponenty ECS (SoA) i budżet pamięci | `M3a-fundament-agenta.md` |
| 5.2 | Silnik DES (§17.3) | `M3a-fundament-agenta.md` |
| 5.3 | Punkty rozszerzenia dla M4 i M5 (kontrakt) | `M3a-fundament-agenta.md` |
| 5.4 | Planer dnia (§5.5) | `M3b-dzien-mieszkanca.md` |
| 5.5 | Potrzeby (§5.3) — tabela parametrów | `M3a-fundament-agenta.md` |
| 5.6 | GD, cykl życia, demografia (§5.2) | `M3c-demografia-i-spoleczenstwo.md` |
| 5.7 | Migracja i regulacja populacji (§5.2) | `M3c-demografia-i-spoleczenstwo.md` |
| 5.8 | Status, relacje, plotka (§5.4, §5.7) | `M3c-demografia-i-spoleczenstwo.md` |
| 5.9 | Generacja populacji — Etap 8 (§4.2) | `M3d-populacja-i-ui.md` |
| 5.10 | Ruch pieszy (za `TravelOracle`, K-2) | `M3b-dzien-mieszkanca.md` |
| 5.11 | `engine/ui` — szkielet i karta inspekcji | `M3d-populacja-i-ui.md` |
| 5.12 | Systemy ECS i ich częstotliwość | `M3d-populacja-i-ui.md` |

---

## 6. Kontrakty międzyfazowe

### 6.1 Dostarczam — trwale

| Nazwa | Rodzaj | Odbiorcy | Opis |
|---|---|---|---|
| `sim::agents::{Identity, Personality, Vitals, Needs, Skills, Wealth, Employment, Residence, PlanRef, AgentState, KnowledgeRef, RelationsRef, Lifecycle}` | komponenty ECS | M4–M12 | stan mieszkańca |
| `sim::agents::Household` | komponent ECS | M5, M7, M8, M9 | GD z polami budżetu i zapasów |
| `sim::agents::PlaceProvider` | **trait** | **M5** (podmienia), M6, M8 | źródło miejsc zaspokajających potrzeby |
| `sim::agents::{PlaceCandidate, FulfilRequest, FulfilOutcome, OpenHours}` | typy traita | M5, M8 | |
| `sim::agents::TravelOracle` | **trait** | **M4** (podmienia), M8 | czas / koszt / tryb podróży; jedyne wejście do routingu w M3 |
| `sim::agents::{TravelEstimate, TripRequest, TripHandle}` | typy traita | M4 | |
| `sim::agents::planner::{plan_day, plan_day_explained, replan, PlanCtx, DayCanvas, PlanSlot, PlanStats, ReplanCause}` | API | M4, M5, M7, M9 | planer dnia |
| warianty `core::DecisionReason` wniesione przez M3 (lista w 5.4) | warianty enum | wszystkie | **enum należy do `core` (K-12)**, M3 go nie posiada — wnosi warianty i zamraża ich dyskryminatory |
| warianty `core::{NeedKind, ActivityKind}` + `TransportMode::Walk` | warianty enum | M4, M5, M8 | **słowniki należą do `core` (K-8)**; M3 wnosi 12 potrzeb i komplet aktywności planera |
| `sim::agents::des::{EventQueue, SimEvent, EventKind, order_key}` | API | M4, M6, M7, M8 | kolejka zdarzeń wielokrotnego użytku |
| `sim::agents::needs::{NeedKind, NeedTable, need_decay}` | API + dane | M5, M8 | |
| `sim::agents::social::{status_of, SocialClass, KnowledgeView, awareness_of}` | API | M5, M7, M10 | status, klasa, znajomość miejsca |
| `sim::agents::InheritanceHook` | **trait** | M5, M7, M10 | rejestracja dziedziczenia aktywów |
| `sim::world::population::{generate_population, PopulationParams, PopulationReport}` | API | M8, M9 (scenariusze), M10 (Etap 9) | Etap 8; używany też przez `MigrationSystem` |
| `engine::ui::{UiContext, InspectorPanel, Selection, TimeControlsWidget}` | API | **M9**, M11 | szkielet UI |
| `engine::ui::inspect::timeline::{DayTimelineWidget, render_day_text}` | API | M9, M4 | oś czasu + wydruk tekstowy |
| `engine::ui::inspect::reason::describe` | fn | wszystkie | jedyne miejsce zamiany `DecisionReason` na tekst |
| `data/needs/`, `data/demography/`, `data/names/` | katalogi danych | M5, M8, M10 | schematy RON ze `schema_version` |
| `StreamId::{DayPlan, Demography, Gossip, PopGen, PersonalityGen, Migration, Relations}` | warianty enum | — | blok M3 = **140–159** (K-4), rezerwa 1140–1159; wartości przypisane raz i zamrożone |

### 6.2 Dostarczam **tymczasowo — faza zastępuje**

| Nazwa | Kto zastępuje | Kiedy znika | Uwaga |
|---|---|---|---|
| `sim::agents::WalkOracle` — **minimalny estymator odległości i czasu dojścia**, implementacja `TravelOracle` | **M4** (`engine/nav`, K-2) | M4 usuwa moduł `sim/agents::walk` w całości | `pub(crate)`, bez publicznego API routingu; test architektoniczny pilnuje, że nic z `walk` nie wycieka do API crate'a. M4 **nie migruje** tego kodu — pisze swój i kasuje ten |
| `sim::agents::InfinitePlaces` — nieskończone źródła miejsc/sklepów, implementacja `PlaceProvider` | **M5** | M5 usuwa | wybór = najkrótszy dojazd; `fulfil` zawsze `Done`, `spent = Money(0)` |
| `sim::agents::Employment` — **statyczne** przypisanie pracy z generacji | **M7** | M7 przejmuje zarządzanie polem (komponent zostaje, decyzja 9.5) | w M3 zmienia się tylko przez zdarzenia demograficzne (emerytura, śmierć, migracja) |
| `Household.stock: [u8; 8]` — abstrakcyjne „dni zapasu" | **M5** | M5 zastępuje realnymi towarami z partiami | próg wyzwalający zakupy w planerze pozostaje ten sam |
| `sim::agents::walk::PedestrianBuffer` — pozycje pieszych Mikro | **M4** (jeden bufor dla wszystkich uczestników ruchu) | M4 | |
| atrapy testowe `FlakyPlaces`, `PanickingPlaces` | — | zostają jako testy kontraktowe | M5 i M4 powinny je nadal przechodzić |

### 6.3 Konsumuję

| Od kogo | Co | Wymaganie |
|---|---|---|
| M0 `engine/core` | `Money`, `SimMinute`, `Tick`, `Q`, `Mood`, `CitizenId`, `HouseholdId`, `SiteId`, `BuildingId`, `DistrictId`, `JobRoleId`, `StreamId`, `rng()` | doc 00 §2, §3 |
| M0 `engine/core` | `SimClock`, `SimSpeed` | decyzja 9.1 |
| M0 `engine/core` | `SimCalendar` + **`DayOfWeek`** (rok 12×30, tydzień 7-dniowy dryfujący) | K-1, K-15 — M3 nie liczy dnia tygodnia samodzielnie |
| M0 `engine/core` | `TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind` — słowniki domenowe | **K-8, rozstrzygnięte** |
| M0 `engine/core` | `DecisionReason` — centralny enum bez `#[non_exhaustive]` | **K-12, rozstrzygnięte** |
| M0 `engine/core` | blok `StreamId` **140–159** (rezerwa 1140–1159) | **K-4, rozstrzygnięte** |
| M0 `engine/ecs` | archetypy, `CommandBuffer`, deklaracja R/W systemów, hash stanu | doc 00 §3 |
| M0 `engine/jobs` | deterministyczny fork-join po chunkach | doc 00 §3.3 |
| M0 `engine/io` | rejestracja komponentów do snapshotu | |
| M1 `engine/render` | wpięcie warstwy UI, bufor ID do selekcji, rysowanie voxeli pieszych | decyzja 9.3 |
| M2 `sim/world` | `Parcel`, `Building`, lokale z powierzchnią i wartością, dzielnice z wartością gruntu i przestępczością | |
| M2 `sim/world` | **geometria ulic (centrolinie odcinków + długości)** — nie graf nawigacyjny | K-2: graf nawigacyjny buduje M4; M3 potrzebuje tylko odległości |
| M2 `engine/spatial` | indeks przestrzenny „miejsca w promieniu R" | używany przez `InfinitePlaces` i zasiew wiedzy |
| M1/M2 Etap 6–7 | `HousingUnit[]`, `JobSlot[]` z `wage_band` | wejście Etapu 8; decyzja 9.12 |

### 6.4 Zamrożone (nie wolno zmieniać po M3)

- Dyskryminatory wariantów wniesionych przez M3 do `core::{DecisionReason, NeedKind, ActivityKind}` (K-8, K-12) oraz do własnych `EventKind`, `CommitmentKind`, `ShiftKind` — wolno **dopisywać na końcu**, nie wolno przestawiać ani usuwać (doc 00 §3.1: wpływ na strumienie RNG i na zapis stanu).
- Wartości bloku `StreamId` 140–159 raz przypisane funkcjom M3 (K-4) — zmiana łamie determinizm zapisów.
- Sygnatury `PlaceProvider` i `TravelOracle` — M4 i M5 implementują, nie zmieniają. Rozszerzenie wyłącznie przez dodanie metody z domyślną implementacją.
- Klucz porządkujący `order_key` w DES — zmiana łamie determinizm wszystkich istniejących zapisów.

---

## 7. Testy i kryteria akceptacji

### 7.1 Determinizm (doc 00 §3.6)

| Test | Kryterium |
|---|---|
| `det_agents_hash` | dwa przebiegi `m3_day` z tym samym seedem → identyczny ciąg hashy stanu co 1000 ticków |
| `det_thread_count` | ten sam scenariusz na 1, 4 i 16 wątkach → identyczne hashe |
| `det_plan_pure` | `plan_day` wywołany 1000× na tych samych danych → identyczne bajty `DayCanvas` |
| `det_plan_replay` | plan odtworzony po zapisie i odczycie stanu w losowym momencie doby = plan oryginalny |
| `det_event_order` | 100 tys. zdarzeń wstawionych w 20 losowych kolejnościach w tę samą minutę → identyczna kolejność obsługi |
| `det_speed_invariance` | doba przy 1× i przy 10× → identyczny hash (wymóg §14.5) |
| `det_lod_consistency` | ten sam scenariusz w LOD Mikro i Mezo → identyczne czasy przybycia i identyczny hash `Needs` (tolerancja 0, doc 00 §4) |
| `det_demography_independence` | usunięcie jednego mieszkańca nie zmienia żadnego losowania demograficznego u pozostałych (porównanie strumieni) |

### 7.2 Testy własnościowe demografii

| Test | Kryterium |
|---|---|
| `prop_population_identity` | dla każdej doby: `pop(t+1) = pop(t) + narodziny − zgony + napływ − odpływ`, dokładnie co do osoby, przez 36 000 dni |
| `prop_century_survival` | `m3_century`, 5 seedów × 4 rozmiary miasta: populacja po 100 latach ∈ [0,5×, 2,0×] startowej; **żaden przebieg nie schodzi poniżej 1 000 mieszkańców ani nie przekracza 3× startowej w żadnym momencie** |
| `prop_age_pyramid` | udziały grup 0–17 / 18–64 / 65+ nie wychodzą poza koperty epoki (±8 pp) w żadnej dekadzie |
| `prop_no_orphan_household` | brak GD bez członków, brak mieszkańca bez GD, brak GD bez lokalu |
| `prop_household_membership` | każdy mieszkaniec jest w dokładnie jednym GD; `Household.members` i `Identity.household` zawsze zgodne |
| `prop_inheritance_conservation` | suma `Money` przed śmiercią = suma po podziale (tolerancja 0 groszy) |
| `prop_no_immortals` | brak żywego mieszkańca o wieku > 120 lat (= 43 200 dni) w przebiegu 100-letnim |
| `prop_relation_symmetry` | relacja rodzinna jest obustronna; nieżyjący nie występuje w żadnym slabie relacji |
| `prop_knowledge_bound` | `KnowledgeRef.len ≤ 32` zawsze; slaby bez wycieków (liczba zajętych bloków = liczba żywych mieszkańców z wiedzą) |
| `prop_calendar` | każdy miesiąc ma 30 dni, rok 360; żadna data w symulacji nie odwołuje się do kalendarza gregoriańskiego (K-1) |
| `prop_week_drift` | `DayOfWeek` pochodzi wyłącznie z `SimCalendar`; przez 100 lat dzień tygodnia 1. dnia roku przechodzi przez wszystkie 7 wartości (360 mod 7 = 3), a liczba dni roboczych w miesiącu waha się między 20 a 23 — **i żaden test ani dane nie zakładają stałej** (K-15) |
| `prop_weekly_rhythm` | w przebiegu 30-dniowym profil dobowy sobót i niedziel różni się od dni roboczych (mniej slotów `Work`, więcej `Leisure`/`Social`), a obsada zmian weekendowych jest niepusta |

`prop_century_survival` jest testem długim — w CI wersja skrócona (30 lat, 3 seedy, miasto małe); pełna wersja w nocnym przebiegu. Scenariusz `m3_century` jest gotowym wejściem dla balansatora w M5.

### 7.3 Planer

| Test | Kryterium |
|---|---|
| `plan_golden_anna` | **test akceptacyjny fazy**: ustalony seed i mieszkaniec (kobieta 34 l., księgowa, zmiana 8–16, rodzina z dziećmi, zapas pieczywa 1 dzień) → wydruk `render_day_text` zgodny bajt w bajt z zatwierdzonym plikiem, zawierający: pobudkę z poziomem snu, śniadanie z informacją o zapasach, wyjście z domu, dojazd, pracę z godzinami zmiany, **zadanie po drodze z uzasadnieniem wyboru miejsca i wskazaną alternatywą**, powrót, kolację, czas wolny z uzasadnieniem, sen |
| `plan_every_slot_has_reason` | dla 10 tys. losowych mieszkańców: każdy slot ma `reason != ReasonTag::None` |
| `plan_explain_matches` | `plan_day_explained` daje identyczne sloty co `plan_day` dla 10 tys. mieszkańców |
| `plan_commitments_never_dropped` | praca i szkoła nigdy nie są usuwane przez fazę 3 ani 4, także przy przepełnieniu 24 slotów |
| `plan_sleep_and_meal_survive` | przy przepełnieniu slotów plan zawiera sen i ≥ 1 posiłek |
| `plan_unknown_place` | mieszkaniec bez wiedzy o żadnym miejscu danej potrzeby → zadanie pominięte z `PlaceUnknown`, **bez paniki i bez teleportacji** |
| `plan_refusal_path` | atrapa `FlakyPlaces` (odmowa co 3. wizycie) → `ReplanCause::PlaceRefused` obsłużone, plan spójny (ścieżka przygotowana dla M5) |
| `plan_no_overlap` | żadne dwa sloty się nie nakładają; suma `dur_min` + luki = 1440 |
| `plan_replan_debounce` | 1000 zgłoszeń przeplanowania w 1 minucie dla tego samego agenta → dokładnie 1 przeplanowanie |
| `plan_no_economy_types` | test architektoniczny: `planner.rs` nie importuje `GoodId`, `Offer`, ani żadnego typu z modułu `walk` |

### 7.4 Generacja populacji (Etap 8 + podzbiór Etapu 10)

Dla 10 seedów × 4 rozmiary miasta:

| Test | Kryterium |
|---|---|
| `gen_everyone_has_home` | 100% mieszkańców ma `Residence` wskazujący istniejący lokal; żaden lokal nie jest przeludniony ponad limit typu |
| `gen_unemployment` | \|bezrobocie − `unemployment_target`\| ≤ 1 pp |
| `gen_jobs_filled` | \|JobSlot\| ≈ \|aktywni\| × (1 + target), odchylenie ≤ 2% |
| `gen_age_pyramid` | test χ² względem piramidy epoki, p > 0,05 |
| `gen_income_housing` | korelacja rang Spearmana (dochód GD, wartość lokalu) ≥ 0,60 |
| `gen_commute_hist` | χ² histogramu czasu dojazdu vs. cel lognormalny; odchylenie mediany ≤ 5% |
| `gen_skill_fit` | ≥ 70% zatrudnionych ma `skill_match(role) ≥ 40` |
| `gen_determinism` | ten sam seed → identyczna populacja co do bajtu |
| `gen_perf` | 400 tys. mieszkańców ≤ 30 s; 120 tys. ≤ 8 s |

### 7.5 Wydajność (`criterion`, progi regresji w CI)

| Benchmark | Cel |
|---|---|
| `bench_plan_day` | mediana ≤ 10 µs, p99 ≤ 30 µs na plan (1 wątek), z atrapą `candidates` o realistycznym koszcie |
| `bench_replan` | mediana ≤ 5 µs |
| `bench_des_dispatch` | 2 778 zdarzeń/tick (średnia dla 200 tys. agentów) ≤ 60 µs; szczyt 11 tys. zdarzeń ≤ 250 µs |
| `bench_des_day` | pełna doba dla 200 tys. agentów (4 mln zdarzeń) ≤ 250 ms na 1 wątku |
| `bench_need_decay` | shard 1/60 przy 400 tys. ≤ 100 µs/tick na 8 wątkach (korekta A-8; zmierzone 63 µs) |
| `bench_gossip_day` | dobowa plotka dla 400 tys. ≤ 20 ms |
| `bench_walk_estimate` | mediana ≤ 80 µs; trafienie cache ≤ 0,2 µs |
| `bench_population_gen` | jak `gen_perf` |
| `mem_population_400k` | stan gorący ≤ 400 B/mieszkańca; całość `sim/agents` ≤ 300 MB |

Cel zbiorczy fazy: **pełna doba gry dla 120 tys. mieszkańców w trybie headless ≤ 3 s wall-clock** na 8 wątkach (≥ 480× czasu rzeczywistego) — z zapasem na M4–M8.

### 7.6 Kryteria akceptacyjne (przegląd fazy)

1. Klik w pieszego w widoku 3D otwiera kartę z osią czasu dnia; każdy blok ma czytelne uzasadnienie po polsku.
2. Wydruk planu dnia wzorcowego mieszkańca odpowiada strukturą przykładowi z PRD §5.5.
3. Pauza / 1× / 3× / 10× działają, kalendarz 12×30 idzie, wynik nie zależy od prędkości.
4. Headless 30 dni bez paniki, bez wycieku slabów, z hashami zgodnymi między przebiegami.
5. Headless 100 lat: populacja żyje, piramida wieku sensowna, brak eksplozji.
6. Rozkład czasu dojazdu i obłożenie miejsc pracy zgodne z celami Etapu 8.
7. **Wymiana `InfinitePlaces` na atrapę nie wymaga zmiany ani jednej linii w `planner.rs`** — dowód, że kontrakt dla M5 trzyma.
8. **Publiczne API `sim/agents` nie eksportuje niczego z modułu `walk`** — dowód, że M4 może zbudować `engine/nav` bez rozbierania M3 (K-2).

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Skutek | Mitygacja |
|---|---|---|---|
| R1 | Przeciek abstrakcji ekonomii do planera — ktoś „na chwilę" wstawia cenę do wyboru miejsca | M5 przepisuje planer, kontrakt fazy pada | Test `plan_no_economy_types` + atrapa `PanickingPlaces` + kryterium akceptacyjne nr 7 |
| R2 | Równoległe API routingu w `sim/agents` mimo K-2 | M4 musi utrzymywać dwa grafy albo rozbierać M3 | Moduł `walk` jest `pub(crate)`; test architektoniczny (kryterium nr 8); w sekcji 6.2 wprost: „M4 nie migruje, tylko kasuje" |
| R3 | Lawina przeplanowań (jedno zdarzenie miejskie uderza w setki tysięcy agentów) | spajk klatki, zacięcie symulacji | Debouncing 15 min + twardy limit 20 tys. przeplanowań/tick z przelewem FIFO (5.2) |
| R4 | Demografia oscyluje lub degeneruje w długim przebiegu | świat nie nadaje się do 100-letnich sesji (cel M12) | `m3_century` w CI od pierwszego dnia WP7; migracja jako jedyny regulator z ujemnym sprzężeniem; scenariusz gotowy dla balansatora M5 |
| R5 | Budżet 400 B zjedzony przez późniejsze fazy | metropolia nie mieści się w 6 GB | 16 B rezerwy w `Household`, test `mem_population_400k` jako bramka CI, jawny podział „majątek GD vs osobisty" w sekcji 6 |
| R6 | Planer nie mieści się w 10 µs po dołożeniu użyteczności §6.4 w M5 | M5 traci wydajność, wina przypisana M3 | `candidates` ma twardy limit 16 kandydatów i `max_travel_min`; kosztowna część jest po stronie M5; benchmark planera mierzony z atrapą symulującą 8 µs w `candidates` |
| R7 | M2 nie dostarcza geometrii ulic w formie nadającej się do liczenia odległości | WP6 blokuje się | Wymaganie zapisane w 6.3 jako minimalne (centrolinie + długości), nie graf; fallback: odległość Manhattan z korektą 1,25× — gorsza, ale odblokowuje WP5 i WP10, a i tak znika w M4 |
| R8 | Selekcja encji wymaga bufora ID w renderze, którego M1 nie planował | karta inspekcji bez sposobu otwarcia | Decyzja 9.3; fallback: wyszukiwarka mieszkańców w UI (bez klikania w świat) — nie blokuje testu akceptacyjnego, bo złoty test jest tekstowy |
| R9 | Determinizm łamie się przy mutacjach strukturalnych (narodziny/śmierć przesuwają indeksy) | cała zasada §18.2 pada | Strumienie RNG parametryzowane `entity_index`, nie licznikiem; bufory komend sortowane; testy `det_plan_replay` i `det_demography_independence` |
| R10 | 24 sloty planu za mało dla rodziny wielodzietnej z dwiema zmianami | plany obcinane, dziwne zachowania | `plan_commitments_never_dropped` i `plan_sleep_and_meal_survive` są bramkami; podniesienie limitu do 32 kosztuje 96 B/os. — mieści się w zapasie, ale wymaga ponownego rachunku pamięci |
| R11 | Plotka (§5.7) rozprowadza wiedzę za szybko lub za wolno → w M10 marka nie działa | mechanika marki bez podstawy | Test WP9 z twardymi progami (≥60% w 1 km po 30 dniach, <5% powyżej 5 km) — kalibrowany już w M3, zanim M10 na tym zbuduje |
| R13 | Ktoś zakłada stałą liczbę dni roboczych w miesiącu albo liczy dzień tygodnia jako `day % 30` | dane i bilanse rozjeżdżają się co kilka miesięcy, w sposób trudny do wykrycia | K-15 wbudowane w `Employment.work_days` i `PlanCtx.dow`; test `prop_week_drift` sprawdza wahanie 20–23 dni roboczych w miesiącu; `DayOfWeek` wyłącznie z `SimCalendar`, zakaz lokalnej arytmetyki tygodnia |
| R12 | Sharding systemów (1/60, 1/7, 1/30, 1/360) wprowadza subtelny błąd fazowy | wyniki zależne od momentu startu | Test referencyjny „bez shardingu" dla `NeedDecaySystem` i `GossipSystem` z tolerancją 0; sharding po `entity_index`, nigdy po pozycji w archetypie |

---

## 9. Decyzje otwarte

Do rozstrzygnięcia przed startem wskazanych WP. Rozstrzygnięcia lądują w `00-konwencje-i-kontrakty.md`, nie tutaj.

**Rozstrzygnięte przez koordynatora w trakcie planowania — wbudowane w dokument, nie są już otwarte:**

| Rozstrzygnięcie | Treść | Gdzie naniesione |
|---|---|---|
| **K-1** | kalendarz 360 dni (12 × 30), bez lat przestępnych | 5.1, 5.6, 5.12, 7.2 |
| **K-2** | `engine/nav` (w tym graf pieszy) należy do M4 | 5.10, 6.2, 6.3, WP6, kryterium akcept. nr 8; dawne założenie „graf pieszy od M2" wycofane |
| **K-4** | blok `StreamId` dla M3 = **140–159**, rezerwa 1140–1159 | 6.1, 6.3, 6.4 — **zamyka 9.7** |
| **K-8** | `TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind` → `engine/core` jako słowniki domenowe | nagłówek, 5.4, 6.1, 6.3 — **zamyka 9.6** |
| **K-12** | `DecisionReason` → jeden centralny enum w `core`, bez `#[non_exhaustive]`; M3 wnosi warianty | 5.4, 6.1, 6.3, 6.4 |
| **K-15** | tydzień 7-dniowy **dryfuje** względem miesiąca; `DayOfWeek` z `SimCalendar` | nagłówek, `Employment.work_days` (5.1), faza 1 i 4 planera (5.4), krok 5 generacji (5.9), testy `prop_week_drift`, `prop_weekly_rhythm` |
| — | `data/needs/`, `data/demography/` dopisane do doc 00 §5 | **zamyka 9.13** |
| — | 9.4 (geometria ulic) i 9.12 (`wage_band` w `JobSlot`) przekazane do **M2** z poleceniem odpowiedzi jako jawny kontrakt w jego sekcji 6 | pozostają w tabeli poniżej jako **oczekujące na odpowiedź M2**, nie jako decyzje M3 |

| # | Decyzja | Z kim | Blokuje | Propozycja M3 |
|---|---|---|---|---|
| 9.1 | Czyje są `SimClock` / `SimSpeed` i jakie jest mapowanie prędkości na minuty gry | **M0**, M9, M11 | WP11 | `engine/core` (M0) jest właścicielem, M3 dostarcza tylko widget. 1× = 1 minuta gry / sekundę realną; 50× dodaje M12 |
| 9.2 | Biblioteka UI: `egui` vs własny IMGUI | **M1**, M9, M11 | WP11 | `egui` + `egui-wgpu`. Nie piszemy toolkitu — M9 ma zbudować kilkanaście paneli, nie framework |
| 9.3 | Mechanizm selekcji encji w widoku 3D (bufor ID w renderze vs raycast po CPU) | **M1** | WP11, WP12 | bufor ID renderowany razem z geometrią; M1 wystawia `pick(x, y) -> Option<EntityId>` |
| 9.4 | Minimalna forma geometrii ulic, jakiej M3 potrzebuje od M2, żeby nie budować grafu (K-2) | **M2** (odpowiada w swojej sekcji 6) | WP6, WP10 | **przekazane do M2, oczekuje odpowiedzi.** Potrzebne minimum: lista odcinków `(node_a, node_b, length_m)` + pozycje węzłów. M3 liczy z tego odległość sieciową i nic nie eksportuje |
| 9.5 | Czy `Employment` należy do `sim/agents` (M3) czy `sim/firms` (M7) | **M7** | WP1 | komponent mieszkańca zostaje w `sim/agents`; M7 dokłada `JobPosting`, `Application` i historię zatrudnienia po swojej stronie |
| 9.8 | Czy `PlaceProvider::fulfil` pobiera pieniądze, czy M5 wprowadzi osobny `PurchaseService` | **M5** | WP4 | `fulfil` zwraca `spent: Money`, ale **nie modyfikuje sald** — saldami zarządza M5 w swoim systemie; M3 pole ignoruje |
| 9.9 | Czy dopuszczamy kalibrowany człon tłumiący w demografii, jeśli sama migracja nie ustabilizuje 100 lat | **M10** (balansator, historia „na sucho"), M12 | WP8 | **nie wprowadzamy na zapas**; jeśli `prop_century_survival` oblewa, człon dodaje M10 razem z kalibracją |
| 9.10 | Zakres dziedziczenia w M3 i kształt hooka | **M7**, M5, M10 | WP7 | M3 dzieli tylko `Money` i mieszkanie; `InheritanceHook` z permilami; M7 rejestruje firmy, M5 długi |
| 9.11 | Czy generacja populacji (Etap 8) to moduł w `sim/world` czy w `sim/agents` | **M2**, M10 | WP10 | `sim/world::population` (doc 00: M3 „rozszerza `sim/world` (populacja)"), zależny od `sim/agents`; M10 użyje go do Etapu 9 |
| 9.12 | Czy `JobSlot` z Etapu 7 ma `wage_band` i kto jest jego właścicielem | **M2** (odpowiada w swojej sekcji 6), **M7** | WP10 | **przekazane do M2, oczekuje odpowiedzi.** Typ w `sim/world`, wypełniany w Etapie 7, czytany przez Etap 8, przejmowany przez M7 bez zmiany kształtu. **Bez `wage_band` krok 6 generacji nie ma czym operować** |
| 9.14 | Czy magazyn doświadczeń ma być kompresowany już w M3 | **M12** (pamięć) | WP1 | nie — klasy rozmiaru slabu dają ~2×; kompresja dopiero gdy pomiar przy 400 tys. przekroczy 80 MB. Ścieżka upgrade'u opisana w 5.1 |
| 9.15 | Instytucje publiczne (szkoła, przychodnia) w M3: nieskończony `PlaceProvider` czy encje z pojemnością | **M8** | WP4, WP10 | w M3 nieskończone; M8 podmienia implementację i dokłada pojemność oraz kolejki. Planer bez zmian |
| 9.16 | Format i zakres bufora „śledzenia" (§14.4) — ile decyzji trzymamy dla obserwowanego mieszkańca | **M9** | WP12 | 64 ostatnie wpisy, tylko dla ≤ 8 jednocześnie śledzonych encji; reszta odtwarzana z seeda |
| 9.17 | Czy `PedestrianBuffer` (pozycje Mikro) ma od razu mieć kształt docelowy M4 (wszyscy uczestnicy ruchu), czy M3 robi własny i M4 go zastępuje | **M4** | WP6 | M3 robi własny, minimalny, `pub(crate)`; M4 zastępuje jednym buforem dla pieszych, pojazdów i pasażerów |

---

## 10. Szacunek wielkości

| WP | Nazwa | Rozmiar | Uwaga |
|---|---|---|---|
| WP1 | Komponenty i magazyny | **M** | dużo definicji, mało logiki; slab alokatora to jedyna nietrywialna część |
| WP2 | Potrzeby i deprywacja | **S** | tabela w danych + jeden system shardowany |
| WP3 | Silnik DES | **M** | koło czasu jest proste; cała trudność w determinizmie i testach |
| WP4 | Punkty rozszerzenia + implementacje tymczasowe | **S** | traity + `InfinitePlaces` (kilkadziesiąt linii) + atrapy testowe |
| WP5 | Planer dnia | **L** | rdzeń fazy: 4 fazy, `DayCanvas`, tryb explain, replanning |
| WP6 | Ruch pieszy za `TravelOracle` | **M** | mniejszy niż pierwotnie dzięki K-2 — bez grafu nawigacyjnego, bez CH, bez publicznego API |
| WP7 | GD, cykl życia, demografia | **L** | dużo przejść stanu i mutacji strukturalnych; testy własnościowe są tu najdroższe |
| WP8 | Migracja i regulacja | **M** | mało kodu, dużo strojenia i przebiegów 100-letnich |
| WP9 | Status, relacje, plotka | **M** | trzy systemy shardowane + kalibracja progów zasięgu |
| WP10 | Generacja populacji (Etap 8) | **L** | 10 kroków, 4 testy dopasowania statystycznego, wymóg wydajności |
| WP11 | `engine/ui` szkielet | **M** | ryzyko w integracji z renderem M1, nie w samym UI |
| WP12 | Karta inspekcji z osią czasu | **M** | widget osi czasu + renderer tekstowy + złoty test |
| WP13 | Testy, determinizm, benchmarki | **M** | rozłożone na całą fazę, nie doklejone na końcu |

**Sumarycznie faza: XL** — trzy pakiety L (WP5, WP7, WP10), reszta M/S.

**Ścieżka krytyczna:** WP1 → WP3 → WP4 → **WP5** → WP12.
**Ścieżka równoległa:** WP7 → WP8 → WP10 (wymaga tylko WP1 i danych z M2) — idzie niezależnie od planera, spina się dopiero w WP13.
**Największe ryzyko harmonogramowe:** WP10 zależy od jakości wyjścia Etapu 6–7 z M2 — decyzja 9.12 (`wage_band` w `JobSlot`) musi zapaść **przed** startem WP10, inaczej krok 6 generacji nie ma na czym pracować.

---

## Zmiany wpisane po M3a

Zgodnie z `K-18`. Pełne uzasadnienie w tabeli „Zmiany wpisane po M3a"
dokumentu `M3a-fundament-agenta.md` (numeracja `D-n`) — tu tylko to, co dotyczy kontraktów
fazy, czyli sekcji §6.

| # | Zmiana | Dlaczego |
|---|---|---|
| A-1 ★ | **§6.1: `sim::agents::des::{EventQueue, SimEvent, EventKind, order_key}` dostaje piąty wariant `EventKind::EndActivity`** (dyskryminatory: `Arrive` 0, `NeedTick` 1, `StartActivity` 2, `EndActivity` 3, `PlanDay` 4) | Lazy scheduling z §5.2 wymaga zdarzenia kończącego czynność, inaczej łańcuch „obsługa harmonogramuje następne" rwie się po `StartActivity`. Kolejność wariantów jest kolejnością obsługi przy remisie czasowym i od teraz jest zamrożona (§6.4) |
| A-2 ★ | **§6.1: `TravelOracle::begin_trip` ma trzy parametry** — `(trip, who: &CitizenView, q)` | Prędkość marszu zależy od wieku, zdrowia i energii. Sygnatury traitów są zamrożone od M3, a M4 ma je implementować, nie zmieniać — parametr wchodzi teraz, zanim ktokolwiek je implementuje. Szczegóły: korekta D-3 |
| A-3 ★ | **§6.1: warianty `core::DecisionReason` wniesione przez M3 to lista z M3b §5.4 plus `ModeWalkOnly` (113) i `NeedSatisfied` (114)** | Lista z M3b nie miała powodu dla `TravelEstimate.reason` ani dla `FulfilOutcome::Done`, a oba pola są w kontrakcie §5.3 i oba wymagają wyjaśnienia (00 §7). Przypadek 5 z `K-18` |
| A-4 ★ | **§6.3: M3 konsumuje z `core` cztery nowe słowniki** — `PlaceKind`, `TraitId`, `DeprivationEffect`, `MinuteOfDay` — i wnosi do `NeedKind` dwanaście wariantów z M3a §5.5 w miejsce dziesięciu roboczych z M0 | Wpisane jako **`K-20`** w dokumencie 00. `DeprivationEffect` musi być w `core`, bo jest ładunkiem `DecisionReason`; reszta spełnia kryterium K-8. Nic nie używało tamtych dziesięciu wariantów |
| A-5 | **§6.1: `sim::world::population` nadal jest właścicielem generacji, ale `sim/agents` nie zależy od `sim/world`.** Katalog miejsc (`PlaceTable`) wsypuje się z zewnątrz | Zależność idzie w jedną stronę: `sim/world` → `sim/agents`. Etap 8 (M3d) wypełnia `PlaceTable` budynkami i zakładami M2, a `InfinitePlaces` i `WalkOracle` czytają gotowy katalog. Bez tego crate agentów ciągnąłby za sobą generator miasta |
| A-6 | **§6.1: `register` rozbite na `register_components` i `register_resources`** | Minimalny snapshot z M0 nie niesie zasobów, a `world_state_hash` je hashuje — świat z hakami zasobów nie przechodzi round-tripu. Poprawka do `engine/io` poszła do M12b; do tego czasu round-trip testuje się na samych komponentach (korekta D-4) |
| A-8 ★ | **§7.5: `bench_need_decay` ≤ 100 µs na 8 wątkach zamiast ≤ 30 µs** (zmierzone 63 µs przy 400 tys.) | Próg 30 µs liczył koszt dotknięcia shardu (107 KB), a nie koszt jego znalezienia: archetypowy ECS musi przejść wszystkie wiersze i odrzucić 59/60, bo nie umie zaadresować „co sześćdziesiątej encji" bez bocznego indeksu, który trzeba by unieważniać przy każdych narodzinach. 63 µs to 0,007 % ticku przy 1× — szczegóły w korekcie D-13 dokumentu M3a |
| A-7 | **§7.5: `bench_alloc_population` nazywa się `mem_population_400k`** i jest testem, nie benchmarkiem; próg to **430 B** zamiast 400 B | Budżet mierzy się raz i porównuje z progiem — to jest test. Criterion mierzy tempo zmian i pilnuje regresji, więc benchmarkiem zostaje `m3a-3 populacja/spawn 400 tys.`. Podniesienie progu: korekta D-1 |

---

## Zmiany wpisane po M3b

Zgodnie z `K-18`. Pełne uzasadnienie w tabeli „Zmiany wpisane po M3b"
dokumentu `M3b-dzien-mieszkanca.md` (numeracja `F-n`) — tu tylko to, co dotyczy kontraktów
fazy, czyli sekcji §6.

| # | Zmiana | Dlaczego |
|---|---|---|
| A-9 ★ | **§6.1: `sim::agents::planner` eksportuje `plan_day`, `plan_day_explained`, `replan`, `replan_explained_into`, `request_replan`, `tick_replan_cooldown`, `store_plan`, `load_plan`, `render_day_debug`, `PlanCtx`, `DayCanvas`, `PlanStats`, `ReasonLog`, `ReasonEntry`, `HouseholdView`, `MAX_SLOTS`** | Lista z §6.1 wymieniała `PlanSlot` (jest w `store`, M3a) i nie wymieniała ani debouncingu, ani zapisu planu do slabu — a bez obu planer nie da się wpiąć w systemy ECS w M3d. `HouseholdView` dochodzi jako widok, nie komponent (korekta F-4) |
| A-10 ★ | **§6.3: M3 konsumuje z `core` dwa kolejne słowniki — `CommitmentKind` i `StockCat`** (z `STOCK_CAT_COUNT` i `StockCat::need()`) | Oba są ładunkami `DecisionReason`, więc muszą być w `core` (K-12). Dopisane do `K-20` w dokumencie 00 — korekta F-3 |
| A-11 ★ | **§6.4: blok `DecisionReason` fazy M3 jest kompletny i zamrożony: 100–114.** `SlotBudgetExhausted` niesie `NeedKind`, nie `TaskKind`; `Replanned` niesie `cause_tag: u8` (B-1) | `TaskKind` nie powstał — w M3 zadanie jest tożsame z potrzebą (korekta F-2). Numery 115–199 zostają wolne dla M3c i M3d |
| A-12 ★ | **§6.1: `sim::agents::des` — zegar `EventQueue` stoi na minucie właśnie rozdawanej.** `schedule` odrzuca zdarzenie na minutę już rozdaną | Korekta `D-14` do M3a §5.2, znaleziona przy wykonaniu planu: przy poprzedniej semantyce każda podróż trwała o minutę dłużej niż w planie. Dotyczy każdego konsumenta kolejki — M4, M6, M7 i M8 — więc jest zmianą kontraktu, nie szczegółem M3b (korekta F-5) |
| A-13 | **§6.2: `WalkOracle` dostaje konstruktor `with_streets(places, nodes, segments)` i pięć metod `micro_*`.** Wszystkie znikają razem z modułem, gdy M4 dostarczy `engine/nav` | Płaskie tablice zamiast typu grafu (F-6) i zrzut pozycji zamiast bufora (F-7) — publiczne API crate'a nadal nie zna ani węzła, ani odcinka, ani trasy |

---

## Zmiany wpisane po M3c

Zgodnie z `K-18`. Pełne uzasadnienie w tabeli „Zmiany wpisane po M3c"
dokumentu `M3c-demografia-i-spoleczenstwo.md` (numeracja `G-n`) — tu tylko to, co dotyczy
kontraktów fazy, czyli sekcji §6.

| # | Zmiana | Dlaczego |
|---|---|---|
| A-14 ★ | **§6.1: `sim::agents` eksportuje warstwę społeczeństwa** — `household::{Household, HouseholdKind, HouseholdOverflow, HouseholdRoles, MemberView, members_of, add_member, remove_member, classify, roles}`, `demography::{DemographyTable, Population, LifeQueue, InheritanceHook, NoInheritance, step_day, step_month, stream_key, powiaz, citizen_by_index, household_by_index}`, `migration::{Vacancies, HomeSlot, JobSlot, Unsettled, spawn_household, spawn_household_aged, seed_population, zaloz_gospodarstwo, shock_retire_jobs, attractiveness, step_month}`, `social::{SocialClass, status_of, StatusInput, StatusBreakdown, StatusDistribution, CityFacts, SocialIndex, learn_place, knows_place, awareness_of, step_day, step_month}`, `society::{register_society, step_day, population, households, total_money, is_month_start}` | §6.1 zapowiadało `Household`, `InheritanceHook` i `social::{status_of, SocialClass, KnowledgeView, awareness_of}` — czyli wierzchołek. Reszta to rzeczy, bez których M3d nie wpnie tego w systemy ECS §5.12 ani w Etap 8: spis populacji, terminarz zdarzeń życiowych, pula wakatów i pustostanów, generator gospodarstw |
| A-15 ★ | **§6.3: M3 konsumuje z `core` dwa kolejne słowniki — `MigrationKind` i `LifeEventKind`** — a blok `DecisionReason` fazy zamyka się na **100–119** | Oba są ładunkami `DecisionReason` (`MigrationDecision`, `LifeEvent`), więc muszą być w `core` (K-12). Dopisane do `K-20` w dokumencie 00 — korekta G-11 |
| A-16 ★ | **§6.2: `sim::agents::Vacancies` dochodzi do listy „dostarczam tymczasowo".** Pojemność miasta wchodzi do `sim/agents` dwiema płaskimi listami (`HomeSlot`, `JobSlot`); wypełnia je Etap 8 z `Unit` i `Workplace` M2, a **M7 przejmuje pulę etatów** razem z rynkiem pracy | Ten sam kierunek zależności co przy `PlaceTable` (A-5): `sim/world` zależy od `sim/agents`, nigdy odwrotnie. Dopóki M7 nie ma rynku pracy, pula wakatów jest jedynym miejscem, w którym „miasto ma pracę" cokolwiek znaczy — i jedynym wejściem regulatora populacji z §5.7 |
| A-17 ★ | **§6.1: `sim::agents::planner` — `HouseholdView` ma piąte pole `pickups`**, a faza 1 planera układa powrót z pracy przez szkołę | Domknięcie korekty C-8: podział ról w gospodarstwie wyznacza odbierającego dziecko, ale bez pola w widoku planer nie miał jak tego wykonać. M4 czyta te sloty jak każdy inny `Commute` — po parze miejsc, nie po rodzaju (korekta G-12) |
| A-18 | **§6.4: rytm doby i miesiąca dostarcza `society::step_day`, nie systemy ECS.** Tabela systemów §5.12 należy do M3d i to ona je opakuje | Ta sama decyzja co w M3a (pętla zdarzeń) i M3b (planer). Powód dodatkowy: tryb demograficzny `m3_century` przeskakuje dobę na krok, a `App::tick` idzie minuta po minucie (korekta G-5) |
| A-20 ★ | **§7.5: `bench_gossip_day` ≤ 120 ms na jednym wątku zamiast ≤ 20 ms** (zmierzone 63,6 ms przy 421 tys. mieszkańców). Dochodzi próg dla zasiewu populacji: `seed_population` 200 tys. gospodarstw = 19,5 s, więc Etapowi 8 zostaje 10 s z budżetu 30 s | Próg 20 ms nie miał za sobą rachunku: doba plotki dotyka 1/7 populacji, a na osobę przypada dwanaście przejść po slabie relacji i dwa przeszukania slabu wiedzy słuchacza. 63,6 ms na **dobę gry** to 0,004 % czasu przy 1× i 0,22 % w trybie 50×; system jest przy tym shardowany i nadaje się do `par_for_each`. Szczegóły: korekty G-16 i G-17 |
| A-19 | **§7.2: `prop_century_survival` w wersji pełnej jest artefaktem runnera `headless century`, a nie testem `cargo`.** W CI stoi wersja skrócona `wp7_populacja_przezywa_trzydziesci_lat` (30 lat × 3 ziarna) | Sto lat gry × pięć ziaren × cztery rozmiary miasta to kilkanaście minut — nocny przebieg, dokładnie jak zapowiada §7.2. Runner zwraca kod wyjścia 1, gdy populacja wyjdzie poza [0,5×, 2,0×], więc nadaje się do nocnego joba bez obudowy |

## Zmiany wpisane po M3d

Zgodnie z `K-18`. Pełne uzasadnienie w tabeli „Zmiany wpisane po M3d"
dokumentu `M3d-populacja-i-ui.md` (numeracja `H-n`) — tu tylko to, co dotyczy kontraktów
fazy, czyli sekcji §6.

| # | Zmiana | Dlaczego |
|---|---|---|
| A-21 ★ | **§6.1: `sim::agents` eksportuje warstwę systemów ECS** — `systems::{register_day, bootstrap_day, AgentSources, Sources, CitizenSnapshot, DayLoopSystem, DayStats, ReplanCooldownSystem, SkillDriftSystem, HouseholdStockSystem, SocietySystem, WalkMicroSystem, Trace, TraceEntry, micro_count, set_lod, MAX_TASK_TRAVEL_MIN, MAX_WATCHED, TRACE_LEN}` oraz `places::{home_of, site_of, place_from_key, SITE_KEY_BASE}` | §6.1 zapowiadało kontrakty planera i traitów, a nie to, czym się je napędza. `AgentSources` jest **punktem podmiany dla M4 i M5**: M5 zamienia `places` na indeks ofert, M4 `travel` na `engine/nav`. `CitizenSnapshot` jest wejściem zarówno pętli doby, jak i karty inspekcji — plan odtwarza się z niej tym samym kodem, którym powstał |
| A-22 ★ | **§6.1: konwencja kluczy miejsc jest kontraktem.** Budynki numerują się od zera, zakłady od `SITE_KEY_BASE = 1 << 24`; `Residence.building` i `Employment.site` to klucze w tej samej przestrzeni co `Knowledge.target` | Bez przesunięcia budynek nr 7 i zakład nr 7 byłyby dla magazynu wiedzy tym samym miejscem. `sim/agents` zna **konwencję** i umie ją odwrócić (`place_from_key`); skąd biorą się numery, wie generator populacji, bo tylko on widzi jednocześnie budynki M2 i mieszkańców (E-1, E-6) |
| A-23 ★ | **§6.1: `sim::world::population` dostarcza `generate_population`, `PopulationParams`, `PopulationReport`, `Populated`, `home_place`, `site_place`, `SITE_KEY_BASE`, `COMMUTE_BINS`** — a `Populated` niesie `PlaceTable`, sieć pieszą i raport | §6.1 zapowiadało `{generate_population, PopulationParams, PopulationReport}`. `Populated` dochodzi, bo katalog miejsc i sieć piesza to **wynik** Etapu 8, a nie jego efekt uboczny: bez nich `InfinitePlaces` i `WalkOracle` nie mają z czego powstać (korekty E-1 i E-7) |
| A-24 ★ | **§6.3: M3 konsumuje z `core` `SimClock` i `SimSpeed`** (K-22) oraz z `ecs` **system wyłączny** (K-21) | Wykonanie decyzji 9.1 i warunek konieczny §5.12: krok, który jest funkcją nad całym światem, musi dostać świat na wyłączność. Oba wpisy są w dokumencie 00, bo zmieniają kontrakt crate'ów M0 |
| A-25 ★ | **§6.1: `engine/ui` dostarcza `Catalog`, `LocKey`, `Locale`, `TimeControlsWidget`, `Selection`, `Picker`, `ListPicker`, `UiContext`, `InspectorPanel`, `CitizenCard`, `DayTimeline`, `render_day_text`, `inspect::reason::describe`** oraz katalogi `data/locale/{pl,en}.ron` | §6.1 zapowiadało `{UiContext, InspectorPanel, Selection, TimeControlsWidget}` i `inspect::timeline::{DayTimelineWidget, render_day_text}`. `DayTimelineWidget` nie powstał pod tą nazwą: widget rysujący należy do warstwy graficznej, której M3d nie dostarcza (H-15), a **model** osi dnia nazywa się `DayTimeline` i jest wspólny dla widgetu i wydruku. `Catalog`/`LocKey`/`Locale` wchodzą do §6.1 wcześniej, niż zapowiadał M9 — bo tekst dla gracza musiał powstać już tutaj (korekta E-8) |
| A-26 | **§6.2: `sim::agents::Sources.travel` jest typem konkretnym (`WalkOracle`), nie `dyn TravelOracle`** | Moduł `walk` jest `pub(crate)` i **znika w całości** razem z M4 (§6.2), więc typ konkretny nie jest tu zobowiązaniem na przyszłość, tylko zapisem tego, że jedyna implementacja żyje w tym samym crate'cie. Planer nadal kompiluje się przeciwko `&dyn TravelOracle` — to on jest kryterium akceptacyjnym nr 7, nie systemy |
| A-27 ★ | **§7.4: `gen_perf` mówi o 400 tys. mieszkańców, których M2 nie ma z czego zrobić.** Metropolia 16 km daje 175 tys. lokali i 189 tys. etatów, czyli **274 tys. mieszkańców** — i tyle zajmuje 26,9 s | Liczba 400 tys. pochodzi z `CityPlan.target_pop` (PRD §4.1), a nie z pojemności, którą Etap 6–7 faktycznie stawia; M2e zgłosił ten rozjazd jako T10 (rozrzut 0,61–1,08). Próg zostaje, ale mierzy się go na **największym mieście, jakie istnieje** |
