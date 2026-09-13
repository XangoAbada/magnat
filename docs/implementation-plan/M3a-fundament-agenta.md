# M3a — Fundament agenta

Podfaza 1 z 4 fazy **M3 — Ludzie i dzień** (`M3-ludzie-i-dzien.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M0 (`ecs`, `core`), M2 (parcele, budynki, lokale). |
| **Pakiety robocze** | WP1, WP2, WP3, WP4 |
| **Projekt techniczny** | §5.1, §5.2, §5.3, §5.5 |
| **Wynik do pokazania** | Headless: 400 tys. mieszkańców w pamięci, potrzeby spadają zgodnie z tabelą, koło czasu rozdaje zdarzenia. |
| **Kryterium zamknięcia** | Kryteria WP1–WP4; budżet ≤ 400 B stanu gorącego na mieszkańca i test architektoniczny na atrapie `PanickingPlaces`. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M3b-dzien-mieszkanca.md` |

Warstwa, na której stoi cała reszta fazy: komponenty SoA i magazyny o zmiennej długości, potrzeby z deprywacją, silnik DES i dwa punkty rozszerzenia (`PlaceProvider`, `TravelOracle`) z implementacjami tymczasowymi.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP1 | Komponenty i magazyny | M0 (`ecs`, `core`) | M |
| WP2 | Potrzeby i deprywacja | WP1 | S |
| WP3 | Silnik DES | WP1 | M |
| WP4 | Punkty rozszerzenia M4/M5 + implementacje tymczasowe | WP1, M2 (parcele, budynki) | S |

### WP1 — Komponenty i magazyny

Definicja wszystkich komponentów SoA z sekcji 5.1, trzech magazynów o zmiennej długości (arena planu, slab relacji, slab wiedzy), rejestracja w funkcji haszującej stan ECS.

**Kryterium ukończenia:** test `size_of` dla każdego komponentu zgodny z tabelą budżetu; `bench_alloc_population(400_000)` pokazuje zużycie ≤ 400 B/mieszkańca stanu gorącego (bez magazynu wiedzy); hash stanu stabilny po serializacji/deserializacji.

### WP2 — Potrzeby i deprywacja

Tabela temp spadku (dane RON), `NeedDecaySystem` shardowany 1/60 na tick minutowy, `DeprivationEffectsSystem` przeliczający wpływ na `Vitals`.

**Kryterium ukończenia:** mieszkaniec bez żadnego zaspokojenia osiąga stan krytyczny w czasie zgodnym z tabelą (±2%); sharding nie zmienia wyniku względem wersji referencyjnej „wszyscy co godzinę" (tolerancja 0).

### WP3 — Silnik DES

Koło czasu (`TimingWheel`) o 2880 kubełkach minutowych + `BTreeMap` przelewowy na zdarzenia dalekie (urodziny, śmierć, rocznice). Deterministyczne sortowanie kubełka, dispatch do handlerów.

**Kryterium ukończenia:** `bench_des_dispatch` — 4 mln zdarzeń/dobę gry przetworzone ≤ 250 ms sumarycznie na 1 wątku; test: losowa kolejność wstawiania 100 tys. zdarzeń w tę samą minutę → identyczna kolejność obsługi.

### WP4 — Punkty rozszerzenia M4/M5

Traity `PlaceProvider` i `TravelOracle` plus implementacje tymczasowe `InfinitePlaces` i `WalkOracle`. **To jest kontrakt, nie szkic** — M5 i M4 podmieniają wyłącznie implementację, planer się nie zmienia.

**Kryterium ukończenia:** planer kompiluje się przeciwko traitom, nie przeciwko implementacjom (test: atrapa `PanickingPlaces` panikująca w każdej metodzie, podmieniona w teście, potwierdza brak zależności typów); żadna sygnatura w `planner.rs` nie zawiera `Money` w roli ceny, `GoodId` ani typu z grafu nawigacyjnego.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.1 Komponenty ECS (SoA) i budżet pamięci

Wszystkie komponenty `#[repr(C)]`, bez paddingu niejawnego (test `size_of` + `offset_of`).

```rust
// ---------- stan gorący, tablice SoA ----------

#[repr(C)] pub struct Identity {        // 16 B
    pub first_name: u16,                // indeks w puli imion (data/names/)
    pub last_name:  u16,
    pub birth_day:  i32,                // dni od dnia 0 świata (rok = 360 dni, K-1); ujemne = przed startem
    pub birth_district: u16,            // DistrictId; 0xFFFF = przyjezdny
    pub flags:      u8,                 // bit0 płeć, bit1 żywy, bit2 gracz, bit3..7 rezerwa
    pub _pad:       u8,
    pub household:  u32,                // indeks encji GD
}

#[repr(C)] pub struct Personality(pub [u8; 8]);   // 8 B — kolejność wg TraitId
// TraitId: Ambition, Thrift, PriceSensitivity, Loyalty, Sociability, Openness, Risk, Conscientiousness

#[repr(C)] pub struct Vitals {          // 8 B
    pub health: u8, pub energy: u8,     // Q 0..=100
    pub mood:   i8,                     // Mood -100..=100
    pub stress: u8,
    pub edu_level: u8,                  // EduLevel: Podstawowe..Wyższe
    pub edu_field: u8,                  // EduField: Techniczny, Humanistyczny, Medyczny, Ekonomiczny, Artystyczny
    pub status:  u8,                    // Q 0..=100; klasa = przedział, nie jest przechowywana
    pub _pad:    u8,
}

#[repr(C)] pub struct Needs {           // 16 B
    pub level: [u8; 12],                // Q 0..=100, indeks = NeedKind
    pub updated_at: u32,                // SimMinute obcięte do u32
}

#[repr(C)] pub struct Skills(pub [SkillSlot; 4]);                         // 16 B
#[repr(C)] pub struct SkillSlot { pub role: u16, pub level: u8, pub decay: u8 }  // 4 B
// >4 umiejętności: przelew do bocznej BTreeMap<CitizenIdx, Vec<SkillSlot>> (< 3% populacji)

#[repr(C)] pub struct Wealth {          // 16 B — POLA, operacje w M5
    pub cash: Money,                    // portfel osobisty
    pub personal_assets: Money,         // wycena aktywów osobistych; resztę majątku trzyma GD
}

#[repr(C)] pub struct Employment {      // 12 B — statyczne w M3, M7 przejmuje
    pub site: u32,                      // SiteId; u32::MAX = brak pracy
    pub role: u16,                      // JobRoleId
    pub shift: u8,                      // ShiftKind: 6-14, 8-16, 14-22, 22-6, Flex, Weekend
    pub flags: u8,                      // uczeń/student/emeryt/bezrobotny/na zwolnieniu
    pub commute_baseline_min: u16,      // z generacji, do walidacji histogramu
    pub work_days: u8,                  // bitmaska DayOfWeek (K-15): bit N = pracuje w dniu N
    pub _pad: u8,
}
// ShiftKind mówi O KTÓREJ, work_days mówi W KTÓRE DNI — rozdzielone, bo zmiana weekendowa
// (§5.5) to inna maska przy tej samej porze. Maska jest wyprowadzana z DayOfWeek z SimCalendar,
// nigdy z dnia miesiąca: przy roku 360-dniowym tydzień dryfuje względem miesiąca (K-15).

#[repr(C)] pub struct Residence {       // 8 B
    pub building: u32,                  // BuildingId
    pub unit: u16,                      // numer lokalu w budynku
    pub district: u16,                  // DistrictId
}

#[repr(C)] pub struct PlanRef {         // 8 B
    pub offset: u32,                    // offset w DayPlanArena
    pub len: u8,                        // liczba slotów (≤ 24)
    pub cursor: u8,                     // indeks bieżącego slotu
    pub plan_day: u16,                  // dzień świata mod 65536 — wykrywa nieaktualny plan
}

#[repr(C)] pub struct AgentState {      // 8 B
    pub activity: u8,                   // ActivityKind (bieżąca)
    pub lod: u8,                        // Micro / Meso
    pub replan_cooldown: u8,            // minuty do końca debouncingu
    pub flags: u8,
    pub position: u32,                  // Mezo: indeks odcinka ulicy; Mikro: indeks w PedestrianBuffer
}

#[repr(C)] pub struct KnowledgeRef { pub handle: u32, pub len: u8, pub class: u8, pub _pad: u16 } // 8 B
#[repr(C)] pub struct RelationsRef { pub handle: u32, pub len: u8, pub class: u8, pub _pad: u16 } // 8 B

#[repr(C)] pub struct Lifecycle {       // 8 B
    pub partner: u32,                   // indeks encji; u32::MAX = brak
    pub next_event_day: u16,            // najbliższe zaplanowane zdarzenie życiowe
    pub flags: u16,                     // w związku / w ciąży / chory / na emeryturze
}
```

**Magazyny o zmiennej długości**

```rust
// Arena planu dnia — wspólny Vec<PlanSlot>, podwójnie buforowana (dziś / jutro), kompaktowana co dobę
#[repr(C)] pub struct PlanSlot {        // 12 B
    pub start_min: u16,                 // minuta doby 0..1439
    pub dur_min:   u16,
    pub target:    u32,                 // encja miejsca (budynek / site / dom); u32::MAX = brak
    pub kind:      u8,                  // ActivityKind
    pub mode:      u8,                  // TransportMode — M3 pisze Walk; M4 pisze resztę
    pub reason:    u16,                 // ReasonTag(u8) | ReasonParam(u8) — skrót powodu
}

#[repr(C)] pub struct Relation {        // 8 B
    pub other: u32, pub kind: u8, pub weight: u8, pub last_contact_day: u16,
}

#[repr(C)] pub struct Knowledge {       // 8 B — JEDEN magazyn na doświadczenie i na plotkę
    pub target: u32,                    // miejsce / pracodawca / (M5: produkt)
    pub day:    u16,                    // dzień świata mod 65536 ≈ 182 lata gry przy roku 360-dniowym
    pub score:  u8,                     // ocena 0..=100
    pub kind:   u8,                     // Visited | Heard | SeenOnRoute | (M10: Ad)
}
```

Slaby relacji i wiedzy: klasy rozmiaru **4 / 8 / 16 / 24 / 32** wpisów, alokator blokowy bez fragmentacji (bloki stałej wielkości, wolna lista per klasa). Wpis 33. wypycha najstarszy o najniższej wadze (`score × decay(recency)`).

`ponytail:` jeden magazyn na „byłem" i „słyszałem" zamiast dwóch — §5.1 (pamięć) i §5.7 (wiedza) to ten sam byt z innym polem `kind`. Rozdzielenie dopiero gdyby M10 potrzebował innego cyklu życia dla reklamy.

**Rachunek budżetu §17.7 (metropolia 400 tys.)**

Stan gorący na mieszkańca:

| Pozycja | B |
|---|---|
| `Identity` | 16 |
| `Personality` | 8 |
| `Vitals` | 8 |
| `Needs` | 16 |
| `Skills` | 16 |
| `Wealth` | 16 |
| `Employment` | 12 |
| `Residence` | 8 |
| `PlanRef` | 8 |
| `AgentState` | 8 |
| `KnowledgeRef` | 8 |
| `RelationsRef` | 8 |
| `Lifecycle` | 8 |
| **suma komponentów** | **140** |
| arena planu: śr. 12 slotów × 12 B | 144 |
| slab relacji: śr. 10 wpisów × 8 B | 80 |
| kolejka DES: 1 zdarzenie w locie × 16 B | 16 |
| metadane encji ECS (generacja, indeks archetypu) | 16 |
| **stan gorący razem** | **396 B** |

396 B ≤ 400 B — budżet §17.7 dotrzymany. **400 000 × 396 B = 158 MB** stanu gorącego.

Poza budżetem gorącym (§17.7 wprost: „pamięć doświadczeń w osobnym, kompresowanym magazynie"):

| Pozycja | Rachunek | MB |
|---|---|---|
| magazyn wiedzy/doświadczeń | 400 k × śr. 16 wpisów × 8 B | 51 |
| gospodarstwa domowe | ≈167 tys. × 128 B | 21 |
| koło czasu DES (2880 kubełków + przelew) | | 8 |
| cache czasów dojścia dom↔praca | 400 k × 2 uchwyty × 8 B + pula polilinii | 24 |
| bufor pozycji Mikro (tylko encje w kadrze, ≤ 20 tys.) | 20 k × 32 B | 1 |
| **razem `sim/agents` przy 400 tys.** | | **≈ 263 MB** |

Mieści się z dużym zapasem w celu 6 GB, w którym główną pozycją są voxele i render (M1/M11). Zapas świadomie zostawiony fazom M5–M10, które dołożą mieszkańcowi koszyk, pojazd i historię zatrudnienia.

**Decyzja oszczędnościowa:** gotówka / konto / oszczędności / dług siedzą w GD, nie w mieszkańcu (§5.2: „GD to jednostka ekonomiczna — wspólny budżet"). Mieszkaniec trzyma tylko portfel osobisty. Oszczędza 24 B/mieszkańca i zgadza się z modelem domenowym.

**Kompresja magazynu doświadczeń:** na razie **bez kompresji entropijnej** — klasy rozmiaru slabu dają już ~2× względem stałych 32 wpisów. `ponytail:` sufit znany — jeśli przy 400 tys. magazyn przekroczy 80 MB, pakujemy wpis do 5 B (target jako indeks lokalny w dzielnicy u16 + delta dnia u8 + score u8 + kind w nibble) i kompresujemy zstd bloki zimnych mieszkańców. Nie robimy tego, zanim pomiar tego nie wymusi (decyzja 9.14).

### 5.2 Silnik DES (§17.3)

```rust
#[repr(C)] pub struct SimEvent {        // 16 B
    pub time:  u32,                     // SimMinute obcięte do u32 (≈ 8100 lat gry)
    pub actor: u32,                     // indeks encji
    pub kind:  u8,                      // EventKind
    pub slot:  u8,                      // indeks slotu planu, 0xFF = nie dotyczy
    pub payload: u16,
    pub _pad:  u32,
}

pub struct EventQueue {
    wheel: [Vec<SimEvent>; 2880],            // 2 doby minut; indeks = time % 2880
    overflow: BTreeMap<u32, Vec<SimEvent>>,  // zdarzenia dalsze niż 2 doby (urodziny, śmierć)
    now: u32,
}

impl EventQueue {
    pub fn schedule(&mut self, e: SimEvent);
    /// Zwraca zdarzenia bieżącej minuty w porządku totalnym, deterministycznie.
    pub fn drain_minute(&mut self, out: &mut Vec<SimEvent>);
}
```

**Rozstrzyganie remisów czasowych.** Zdarzenia tej samej minuty sortowane `sort_unstable_by_key` po kluczu:

```rust
fn order_key(e: &SimEvent) -> u64 {
    ((e.kind as u64) << 48) | ((e.actor as u64) << 16) | (e.slot as u64)
}
```

Klucz jest **totalny**: dwa zdarzenia o identycznym kluczu są zabronione (`debug_assert` — jeden aktor nie może mieć dwóch zdarzeń tego samego rodzaju i slotu w tej samej minucie). Wynik nie zależy od kolejności wstawiania ani od liczby wątków. `kind` z przodu, bo kolejność semantyczna ma znaczenie: `Arrive` przed `NeedTick` przed `StartActivity` przed `PlanDay`.

**Lazy scheduling.** Planer **nie** wypycha 12 zdarzeń naraz. Do kolejki trafia wyłącznie **następne** zdarzenie mieszkańca; jego obsługa harmonogramuje kolejne. Efekt: kolejka trzyma ~1 zdarzenie na mieszkańca (400 k × 16 B = 6,4 MB) zamiast 12, a przeplanowanie nie musi usuwać zdarzeń z kolejki — nieaktualne wykrywa się po `(plan_day, slot)` przy obsłudze.

**Rachunek kosztu (§17.3: 200 tys. agentów × ~20 zdarzeń/dobę).**

- 200 000 × 20 = 4 000 000 zdarzeń/dobę; 4 000 000 / 1440 = **2 778 zdarzeń na tick minutowy** średnio. Szczyty poranny i popołudniowy do ~4× średniej ≈ 11 000 zdarzeń/tick.
- Koszt ticku = `drain_minute` (kopia kubełka) + sort 2 778 elementów po kluczu `u64` (~32 tys. porównań) + dispatch.
- Cele: **≤ 60 µs/tick** na 1 wątku dla średniej, **≤ 250 µs** w szczycie. Przy 400 tys. mieszkańców dwukrotność — nadal poniżej 1 ms/tick, przy ticku trwającym (1×) 1000 ms realnego czasu.
- Dispatch zrównoleglony po chunkach posortowanej listy **tylko dla zdarzeń bezkonfliktowych** (piszą wyłącznie do komponentów własnego aktora). Zdarzenia mutujące strukturę lub GD idą przez bufor komend (doc 00 §3.4).

**Strategia replanningu.**

```rust
pub enum ReplanCause {
    Late { delay_min: u16 },              // przybycie później niż ETA (M4: korek)
    PlaceClosed { place: PlaceRef },
    PlaceRefused { place: PlaceRef },     // M5: brak towaru / za drogo
    NeedCritical { need: NeedKind },      // potrzeba spadła poniżej progu awaryjnego
    TripBlocked,
    HouseholdEvent { kind: HhEventKind }, // narodziny, choroba dziecka, śmierć
    ShiftChanged,
    WeatherChanged,                       // M8 wypełnia; M3 zostawia wariant
}
```

1. **Inkrementalnie, nie od zera.** `replan(from_minute, cause)` zachowuje sloty już rozpoczęte i wszystkie **zobowiązania** (faza 1). Przelicza tylko fazy 3 i 4 od `from_minute` do końca doby. Pełne przeplanowanie wyłącznie gdy padło samo zobowiązanie (`ShiftChanged`, `HouseholdEvent::Death`).
2. **Debouncing.** `AgentState.replan_cooldown` blokuje kolejne przeplanowanie przez 15 minut gry. `ponytail:` twardy limit zamiast kolejki priorytetowej przeplanowań — sufit znany: przy dużym ruchu ważne przeplanowanie może przepaść na 15 minut; kolejkę priorytetową wprowadzamy dopiero, gdy pomiar w M4 pokaże, że to boli.
3. **Budżet.** Oczekiwane ≤ 2 przeplanowania/mieszkańca/dobę → 800 tys./dobę przy 400 tys. → 555/tick × 5 µs = **2,8 ms/tick** na 1 wątku; zrównoleglone po chunkach ≈ 0,4 ms. Twardy limit globalny: powyżej 20 tys. zgłoszeń w ticku nadmiar przechodzi FIFO na kolejny tick — zamiast spajku klatki.
4. **Determinizm.** `replan` używa `rng(seed, StreamId::DayPlan, citizen_idx, day * 1440 + from_minute)` — ten sam powód w tej samej minucie daje ten sam wynik niezależnie od tego, ile razy stan był zapisany i wczytany.

### 5.3 Punkty rozszerzenia dla M4 i M5 (kontrakt)

Planer zależy **wyłącznie** od tych dwóch traitów. Żadna sygnatura w planerze nie wspomina o cenie, `GoodId`, magazynie, pojeździe ani grafie nawigacyjnym — dlatego M5 i M4 podmieniają implementacje bez przepisywania planera.

```rust
/// Źródło miejsc zaspokajających potrzeby.
/// M3: `InfinitePlaces` — nieskończone, zawsze udane, wybór = najkrótszy dojazd.
/// M5: indeks ofert sklepów z magazynem, cenami i użytecznością §6.4.
pub trait PlaceProvider: Send + Sync {
    /// Kandydaci zaspokajający potrzebę, wyłącznie **znani** mieszkańcowi (§5.7).
    /// Zwracani w porządku deterministycznym, malejąco po `score`.
    fn candidates(
        &self,
        need: NeedKind,
        from: PlaceRef,
        max_travel_min: u16,
        known: &KnowledgeView<'_>,
        out: &mut ArrayVec<PlaceCandidate, 16>,
    );

    fn opening_hours(&self, place: PlaceRef) -> OpenHours;

    /// Realizacja wizyty w chwili zdarzenia `StartActivity`.
    /// M3: zawsze `Done { spent: Money(0) }`. M5: sprawdza półkę i pobiera pieniądze.
    fn fulfil(&mut self, req: &FulfilRequest) -> FulfilOutcome;
}

pub struct PlaceCandidate {
    pub place: PlaceRef,
    pub travel_min: u16,
    pub score: i32,              // M3: -(travel_min as i32); M5: użyteczność §6.4 × 1000
    pub reason: DecisionReason,  // dlaczego ten kandydat ma taki wynik
}

pub struct FulfilRequest {
    pub citizen: CitizenId,
    pub household: HouseholdId,
    pub need: NeedKind,
    pub place: PlaceRef,
    pub at: SimMinute,
    pub budget_hint: Money,      // M3: Money(0) = bez limitu; M5: realny budżet GD
}

pub enum FulfilOutcome {
    Done { satisfaction: Q, spent: Money, duration_min: u16, reason: DecisionReason },
    Refused(DecisionReason),     // M3 nigdy nie zwraca; M5 zwraca (brak towaru, za drogo)
}

/// Czas, koszt i tryb podróży + jej rozpoczęcie.
/// M3: `WalkOracle` — prywatny estymator po centroliniach ulic M2, tryb zawsze Walk.
/// M4: `engine/nav` — CH, wybór środka transportu §9.4, pojazdy, parking (K-2).
pub trait TravelOracle: Send + Sync {
    fn estimate(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        depart: MinuteOfDay,
        who: &CitizenView<'_>,
    ) -> TravelEstimate;

    /// Rozpoczyna podróż; implementacja sama harmonogramuje zdarzenie `Arrive`.
    fn begin_trip(&mut self, trip: TripRequest, q: &mut EventQueue) -> TripHandle;

    /// Miejsca widoczne z trasy (§5.7). M3: bufor wokół odcinków ulic na drodze dojścia.
    fn places_on_route(&self, trip: &TripHandle, out: &mut ArrayVec<PlaceRef, 8>);
}

pub struct TravelEstimate {
    pub minutes: u16,
    pub cost: Money,             // M3: Money(0)
    pub mode: TransportMode,     // M3: Walk
    pub reason: DecisionReason,
}
```

`TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind` mieszkają w `engine/core` (K-8), żeby M3, M4 i M5 dzieliły definicję bez cyklu `sim/agents` → `sim/traffic` → `sim/economy` → `sim/agents`. `DecisionReason` tak samo (K-12).

**Punkt zaczepienia dla M4 (K-2).** `TravelOracle` jest **jedynym** wejściem do routingu w całej fazie M3. `PlanSlot.mode` jest już w strukturze i zapisywany przez `estimate`. M4 tworzy `engine/nav`, implementuje `TravelOracle` po swojej stronie, dokłada warianty do `TransportMode` i **kasuje moduł `walk` z `sim/agents`**. Planer nie wie o samochodach ani o grafach; wie tylko, że podróż ma czas, koszt i tryb.

**Punkt zaczepienia dla M5.** `fulfil` zwracające `Refused` powoduje `ReplanCause::PlaceRefused`. Ścieżka jest zbudowana i przetestowana już w M3 atrapą `FlakyPlaces` (odmawia co trzeciej wizycie), mimo że produkcyjna `InfinitePlaces` nigdy nie odmawia. Dzięki temu M5 nie dokłada nowej ścieżki sterowania, tylko włącza istniejącą.

### 5.5 Potrzeby (§5.3) — tabela parametrów

Wartości bazowe w `data/needs/needs.ron`, skalowane wiekiem, zdrowiem i osobowością. Poziom `Q` 0..=100.

| Potrzeba | Tempo spadku | Pełna → 0 | Zaspokajana przez | Skutek deprywacji (poziom < 25) |
|---|---|---|---|---|
| Hunger | 6,0 pkt/h | ~17 h | Meal (dom / lokal) | energia −2/h, zdrowie −0,2/h, produktywność ×0,7 |
| Sleep | 4,2 pkt/h na jawie, +25 pkt/h we śnie | ~24 h | Sleep (dom, hotel) | energia −3/h, ryzyko wypadku ×2, p(absencja) = 0,15 |
| Hygiene | 4,2 pkt/h | ~24 h | Hygiene (dom) | zdrowie −0,1/h, status −5 |
| Health | zdarzeniowe (choroba −10..−60) | — | Doctor, Pharmacy, Hospital | absencja, hazard śmierci × k(wiek) |
| Safety | 0,15 pkt/h, modyfikowane przestępczością dzielnicy | ~28 dni | dzielnica, policja (M8) | stres +2/dobę, p(migracji GD) ↑ |
| Housing | 0,05 pkt/h + skok przy zmianie wielkości GD | ~80 dni | odpowiedni lokal | stres +1/dobę, znacznik „szuka mieszkania" (M5/M9) |
| Mobility | 4,2 pkt/h | ~24 h | dostęp do trasy (M3 pieszo; M4 reszta) | brak dostępu do pracy → absencja |
| Clothing | 0,6 pkt/h, ×2 przy zmianie sezonu | ~7 dni | Clothing | status −3, komfort termiczny ↓ |
| Leisure | 3,0 pkt/h | ~33 h | Leisure (kino, park, restauracja, dom) | nastrój −1/h |
| Social | 2,5 pkt/h, ×0,5 gdy w tym samym lokalu co relacja | ~40 h | Social (rodzina, lokale) | nastrój −1/h |
| Status | 0,1 pkt/h | ~40 dni | konsumpcja statusowa (M5), adres, auto (M4) | ambicja ↑, satysfakcja ↓ |
| Development | 0,05 pkt/h | ~80 dni | Education, kursy | ambicja ↑ → wyzwalacz zmiany pracy w M7 |

**System spadku.** `NeedDecaySystem` działa `EveryMinute`, ale przetwarza **1/60 populacji na tick** (shard = `entity_index % 60`); shard, na który przypada kolej, dostaje spadek za pełne 60 minut. To jest **dokładne** (nie przybliżone), deterministyczne i równomiernie rozkłada koszt: 400 k / 60 = 6 667 mieszkańców × 16 B = 107 KB odczytu i zapisu na tick, poniżej 30 µs.

**Zaspokojenie.** Wykonanie slotu `Meal`, `Sleep`, `Leisure`, `Shopping`… wywołuje `PlaceProvider::fulfil`, którego `satisfaction` dodaje się do poziomu z saturacją na 100. W M3 `InfinitePlaces` zwraca stałe wartości z `data/needs/needs.ron`; w M5 wartość zależy od kupionego dobra — planer się nie zmienia.
