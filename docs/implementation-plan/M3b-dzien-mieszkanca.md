# M3b — Dzień mieszkańca

Podfaza 2 z 4 fazy **M3 — Ludzie i dzień** (`M3-ludzie-i-dzien.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M3a (DES, potrzeby, traity), M2 (geometria ulic). |
| **Pakiety robocze** | WP5, WP6 |
| **Projekt techniczny** | §5.4, §5.10 |
| **Wynik do pokazania** | Wydruk dnia wzorcowego: pobudka → posiłek → dojazd → praca → zadanie po drodze → dom → czas wolny → sen; pieszy dociera na czas. |
| **Kryterium zamknięcia** | Kryteria WP5 i WP6; spójność mikro↔mezo w czasach przybycia z tolerancją 0. |
| **Poprzednia / następna** | `M3a-fundament-agenta.md` · `M3c-demografia-i-spoleczenstwo.md` |

Planer dnia w czterech fazach z `DayCanvas` i `DecisionReason` per slot oraz ruch pieszy za `TravelOracle` (mezo + mikro).

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP5 | Planer dnia | WP2, WP3, WP4 | L |
| WP6 | Ruch pieszy za `TravelOracle` (mezo + mikro) | WP3, WP4, M2 (geometria ulic) | M |

### WP5 — Planer dnia

Algorytm 4-fazowy, `DayCanvas`, `DecisionReason` per slot, tryb `explain`, replanning z debouncingiem.

**Kryterium ukończenia:** wydruk dnia dla scenariusza wzorcowego zgodny strukturalnie z §5.5 (pobudka → posiłek → dojazd → praca → zadanie po drodze → dom → czas wolny → sen); `plan_day` czysta funkcja (1000 wywołań = identyczne bajty); `plan_day_explained` daje identyczne sloty co `plan_day`; benchmark ≤ 10 µs mediana.

### WP6 — Ruch pieszy za `TravelOracle`

Mezo: `WalkOracle` liczy czas dojścia i emituje zdarzenie `Arrive`. Mikro: interpolacja pozycji po polilinii na ticku 100 ms dla encji w LOD Mikro.

**Zgodnie z K-2 M3 nie tworzy modułu routingu.** `WalkOracle` jest prywatną implementacją `TravelOracle` w `sim/agents`, opartą na **odległości sieciowej po centroliniach ulic z M2** (BFS po ważonym grafie odcinków ulic, budowanym jednorazowo przy starcie i trzymanym prywatnie). Nie eksportuje typów grafu, nie ma publicznego API `route()`, `path()` ani `graph()`. M4, tworząc `engine/nav`, dostarcza własną implementację `TravelOracle` i **usuwa `WalkOracle` w całości** — nie ma nic do zmigrowania.

**Kryterium ukończenia:** test spójności LOD — ten sam scenariusz w Mikro i Mezo daje identyczne czasy przybycia i identyczny hash stanu potrzeb (tolerancja 0); 5 tys. pieszych w kadrze w LOD Mikro ≤ 2 ms/klatkę; test architektoniczny: `pub use` crate'a `sim/agents` nie eksportuje żadnego typu z modułu `walk`.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 Planer dnia (§5.5)

```rust
pub struct PlanCtx<'a> {
    pub day: WorldDay,                     // dzień świata; rok = 360 dni (K-1)
    pub dow: DayOfWeek,                    // z engine/core::SimCalendar (K-15) — NIE liczony lokalnie
    pub rng: StreamRng,                    // rng(seed, StreamId::DayPlan, citizen_idx, day)
    pub citizen: CitizenView<'a>,
    pub household: HouseholdView<'a>,
    pub places: &'a dyn PlaceProvider,
    pub travel: &'a dyn TravelOracle,
}

pub struct DayCanvas { slots: ArrayVec<PlanSlot, 24> }  // zawsze posortowane po start_min

/// Czysta funkcja: te same wejścia → identyczne wyjście, bez stanu globalnego.
pub fn plan_day(ctx: &PlanCtx<'_>, out: &mut DayCanvas) -> PlanStats;

/// Ten sam algorytm, dodatkowo zapisujący pełne uzasadnienia. Używane przez UI i testy.
pub fn plan_day_explained(ctx: &PlanCtx<'_>, out: &mut DayCanvas, log: &mut ReasonLog) -> PlanStats;

pub fn replan(ctx: &PlanCtx<'_>, from: MinuteOfDay, cause: ReplanCause,
              canvas: &mut DayCanvas) -> PlanStats;
```

**Algorytm — cztery fazy; żadna faza nie usuwa slotu wstawionego przez fazę wyższą.**

```
plan_day(ctx):
  canvas ← pusty
  // FAZA 1 — zobowiązania stałe (niewywłaszczalne)
  // dzień tygodnia BIERZEMY z ctx.dow (SimCalendar, K-15) — nigdy z day % 30 ani z dnia miesiąca
  jeśli Employment.site != BRAK i Employment.work_days & (1 << ctx.dow) != 0:
      okno_pracy ← ShiftKind → (start, koniec)
      wstaw Work[site, okno_pracy]                       reason = Commitment{Work}
      wstaw Commute[dom→site] tuż przed; czas = travel.estimate(dom, site, ...)
      wstaw Commute[site→dom] tuż po                     reason = Commitment{Commute}
  jeśli flags.uczeń i ctx.dow ∈ dni_szkolne(epoka): analogicznie School
  dla każdego dziecka przypisanego temu mieszkańcowi w HouseholdRoles:
      wstaw Escort[dom→szkoła], Escort[szkoła→dom]       reason = Commitment{Childcare}

  // FAZA 2 — potrzeby krytyczne jako okna
  okno_snu ← chronotyp(Personality, wiek) przycięty do luk wokół zobowiązań
  wstaw Sleep[dom, okno_snu]                             reason = NeedCritical{Sleep, level}
  dla posiłku in [śniadanie, obiad, kolacja]:
      miejsce ← dom; jeśli luka < 30 min i mieszkaniec jest w pracy → Meal[near(site)]
      wstaw Meal                                         reason = NeedCritical{Hunger, level}
  jeśli Needs[Hygiene] < 40: wstaw Hygiene[dom] po śnie  reason = NeedCritical{Hygiene, level}

  // FAZA 3 — zadania (wg pilności, potem wg możliwości połączenia z trasą)
  zadania ← []
  jeśli min(Household.stock) ≤ próg: zadania += Shopping{kategoria}
  jeśli Needs[Health] < 30 lub Lifecycle.flags.chory: zadania += Doctor
  jeśli Needs[Clothing] < 30: zadania += Clothing
  // M4 dopisze tu Refuel (paliwo < próg) — punkt zaczepienia jest w tej pętli
  posortuj malejąco po pilności; remisy po dyskryminatorze TaskKind (deterministycznie)
  dla każdego zadania:
      luki ← wolne przedziały canvas ≥ potrzebny_czas
      dla każdej luki:
          kotwica ← miejsce, w którym mieszkaniec JEST na początku luki
          places.candidates(need, kotwica, zasięg_osobisty, known) → ≤16 kandydatów
          jeśli luka przylega do Commute A→B:
              detour(P) = t(A→P) + t(P→B) − t(A→B)      // premia „po drodze"
      wybierz (luka, kandydat) maksymalizujące  score − w_detour · detour
      jeśli brak kandydata: NIE wstawiaj        reason = PlaceUnknown{need} | NoTimeWindow{need}
      wstaw Task + Travel wokół                 reason = kandydat.reason
      jeśli detour > 0: reason = ChosenOnRoute{detour_min, direct_min}

  // FAZA 4 — czas wolny
  dla każdej pozostałej luki ≥ 30 min:
      wagi ← f(Personality[Sociability, Openness], Needs[Leisure|Social|Status],
               pora dnia, ctx.dow)        // profil weekendowy ≠ profil dnia roboczego (K-15)
      los ← softmax(wagi, ctx.rng)              // seedowany → powtarzalny
      wstaw Leisure[wybrany rodzaj]             reason = FreeTimePreference{trait, weight}
  luki < 30 min i luki nocne → Home             reason = FreeTimePreference{Default, 0}

  zwróć canvas
```

**Struktura `DayCanvas`.** `ArrayVec<PlanSlot, 24>` trzymany posortowany; wstawienie to liniowe szukanie luki (`O(n)`), całość `O(n²)` przy n ≤ 24 = najwyżej ~300 porównań na kilku liniach cache. `ponytail:` żadnego drzewa przedziałów — dopiero gdyby limit slotów przekroczył ~64.

Limit 24 slotów jest twardy. Po jego wyczerpaniu faza 4 przestaje wstawiać, a faza 3 pomija zadania o najniższej pilności z powodem `SlotBudgetExhausted`. Test pilnuje, że przy pełnym limicie plan nadal zawiera sen, pracę i co najmniej jeden posiłek.

**Zapis `DecisionReason` — kluczowa decyzja.** Każdy slot ma pole `reason: u16` = `tag(u8) | param(u8)`, co wystarcza na etykietę osi czasu. **Pełne uzasadnienie nie jest przechowywane — jest odtwarzane.** Karta inspekcji woła `plan_day_explained` z tym samym seedem i dniem; determinizm gwarantuje, że plan odtworzony jest identyczny co do bajtu, a `ReasonLog` zawiera pełne parametry: odrzucone alternatywy, różnice czasów, powód pominięcia zadania.

Uzasadnienie: pełne logi decyzji dla 400 tys. mieszkańców to ~100 B/os./dobę = 40 MB/dobę gry, czyli ~14 GB na rok gry — nie do utrzymania. Odtworzenie kosztuje 10 µs i jest dokładne. Test `plan_explain_matches` pilnuje, żeby obie ścieżki się nie rozjechały.

Enum mieszka w `engine/core` (K-12) — jeden centralny słownik dla wszystkich faz, bez `#[non_exhaustive]`. M3 **wnosi poniższe warianty**; kolejne fazy dopisują swoje na końcu.

```rust
// engine/core::DecisionReason — warianty wniesione przez M3
pub enum DecisionReason {
    // planer — fazy
    Commitment          { kind: CommitmentKind },
    NeedCritical        { need: NeedKind, level: Q },
    StockBelowThreshold { cat: StockCat, days_left: u8 },
    FreeTimePreference  { trait_id: TraitId, weight: u8 },
    NoTimeWindow        { need: NeedKind, needed_min: u16, longest_gap_min: u16 },
    SlotBudgetExhausted { dropped: TaskKind },
    // wybór miejsca
    ChosenNearest       { travel_min: u16, runner_up_min: u16 },
    ChosenOnRoute       { detour_min: u16, direct_min: u16 },
    PlaceUnknown        { need: NeedKind, known_count: u8 },   // §5.7
    PlaceClosed         { place: PlaceRef, opens_at: MinuteOfDay },
    // realizacja
    Arrived             { planned: MinuteOfDay, actual: MinuteOfDay },
    Replanned           { cause: ReplanCause, slots_changed: u8 },
    Deprivation         { need: NeedKind, effect: DeprivationEffect },
    // M4 dopisze warianty transportu, M5 — cenowe, M7 — pracy.
    // Istniejące warianty i ich dyskryminatory są zamrożone (sekcja 6.4).
}
```

`PlaceRef`, `NeedKind`, `ActivityKind`, `TransportMode` — analogicznie: słowniki w `core` (K-8), warianty wnoszone przez fazy. M3 wnosi 12 wariantów `NeedKind` (5.5), komplet `ActivityKind` planera i `TransportMode::Walk`.

### 5.10 Ruch pieszy (za `TravelOracle`, K-2)

- **Mezo (domyślnie).** `WalkOracle::estimate` liczy czas dojścia po **odległości sieciowej** na ważonym grafie odcinków ulic z M2 (budowanym raz przy starcie, trzymanym **prywatnie** w module `sim/agents::walk`). Cache LRU + **przypięta** para tras dom↔praca per mieszkaniec. Prędkość bazowa 1,35 m/s, modyfikowana wiekiem, zdrowiem i energią. `begin_trip` harmonogramuje `Arrive` w `depart + minutes` i nic więcej nie robi — brak pozycji, brak ticku 100 ms.
- **Mikro (tylko encje w kadrze / śledzone).** Pozycja interpolowana po polilinii na ticku 100 ms, wpis w `PedestrianBuffer` (32 B: pozycja f32×3, postęp f32, indeks trasy, kierunek). **Bez unikania kolizji i bez steeringu** — przy voxelu 1 m tłum czyta się dobrze bez tego; steering należy do M11 razem z animacjami.
- **Spójność LOD (doc 00 §4).** Czas przybycia liczy **wyłącznie** warstwa mezo. Mikro jest interpolacją między wyliczonym startem a wyliczonym przybyciem i nie może go zmienić. Dlatego test spójności LOD przechodzi z tolerancją 0 z definicji, a nie przez kalibrację.
- **Granica z M4 (K-2).** Moduł `walk` jest `pub(crate)`. Zero publicznych typów grafu, zero funkcji `route`/`path`/`graph` w API crate'a. Test architektoniczny sprawdza, że publiczne API `sim/agents` nie eksportuje niczego z `walk`. Gdy M4 dostarczy `engine/nav`, moduł znika w całości — nie ma nic do zmigrowania ani do utrzymywania równolegle.

`ponytail:` odległość sieciowa zamiast pełnego A* z heurystyką — przy dystansach pieszych (≤ 3 km) BFS po ważonym grafie z wcześniejszym przerwaniem wystarcza, a i tak cała ta implementacja jest do wyrzucenia w M4.

---

## Zmiany wpisane po M3a

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3a —
podfaza nie jest tu przeprojektowywana.

| # | Zmiana | Dlaczego |
|---|---|---|
| B-1 ★ | **`DecisionReason::Replanned` nie może nieść `ReplanCause`** — niech niesie `cause_tag: u8` (jest `ReplanCause::tag()`) i `slots_changed: u8` | `ReplanCause` mieszka w `sim/agents`, a `DecisionReason` w `core` (K-12): ładunek centralnego enuma nie może pochodzić z crate'u, który od `core` zależy. Poza tym `ReplanCause::PlaceClosed` niesie `PlaceRef` (16 B), więc `Replanned` przekroczyłby limit `size_of::<DecisionReason>() <= 24` z K-12 |
| B-2 ★ | **`begin_trip` ma sygnaturę `(trip, who: &CitizenView, q)`** (korekta D-3 w M3a) | Planer i tak trzyma `CitizenView` w `PlanCtx`, więc wywołanie nic nie kosztuje |
| B-3 | **`DayCanvas` stoi na `magnat_agents::ArrayVec<PlanSlot, 24>`** — typ jest w crate'cie, przetestowany, `Copy`, bez `unsafe` | `ArrayVec` powstał w M3a na potrzeby `PlaceProvider::candidates`; drugi konsument był znany z góry (§5.4). Zapis planu do slabu: `PlanSlab` + `PlanRef::set_slab_ref` — arena już nie jest buforowana podwójnie (korekta D-1) |
| B-4 | **`WalkOracle` istnieje i liczy odległość manhattanowo z korektą 1,25×** (fallback z ryzyka R7). WP6 podmienia **wnętrze** `estimate`, `begin_trip` i `places_on_route` na odległość sieciową po centroliniach ulic z M2 | Kontrakt, testy i korytarz „widoczne z trasy" są gotowe; do zrobienia zostaje graf odcinków, cache tras dom↔praca i warstwa Mikro. Sygnatury się nie zmieniają — to jest cały sens `TravelOracle` |
| B-5 | **`plan_no_economy_types` ma już odpowiednik w M3a** (`tests/contract.rs::kontrakt_nie_wspomina_o_gospodarce_ani_o_grafie`, skanuje `places.rs`). WP5 dopisuje do niego `planner.rs` | Test jest tani i łapie dokładnie to, przed czym broni ryzyko R1 — wystarczy dołożyć drugi plik do listy skanowanych |
| B-6 | **Faza 3 planera woła `places::choose_place`**, a nie `PlaceProvider::candidates` wprost | `choose_place` jest już jedynym miejscem produkującym `PlaceUnknown { need, known_count }` i ma test na ścieżkę „mieszkaniec bez wiedzy" (korekta D-5). Planer wywoła je z kotwicą i zasięgiem osobistym |
| B-7 | **Absencja (`DeprivationEffect::AbsenceRisk`) należy do planera** — M3a jej nie stosuje, tylko wystawia przez `deprivation_of` | Rozstrzygnięcie D-6: skutki progowe stosuje faza-właściciel, bo tylko ona wie, co znaczy „nie poszedł do pracy". Wartości (promile) są w `data/needs/needs.ron` |
