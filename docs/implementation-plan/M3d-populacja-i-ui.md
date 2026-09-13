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
| WP10 | Generacja populacji (Etap 8) | WP7, WP9, M2 (lokale), M1/M2 Etap 7 (miejsca pracy) | L |
| WP11 | `engine/ui` — szkielet, selekcja, sterowanie czasem | M1 (`render`) | M |
| WP12 | Karta inspekcji z osią czasu dnia | WP5, WP11 | M |
| WP13 | Testy własnościowe, determinizm, benchmarki | wszystkie | M |

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
