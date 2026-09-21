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
| **Kryterium zamknięcia** | ✅ Kryteria WP1–WP4; budżet ≤ **430 B** stanu gorącego na mieszkańca (korekta D-1; zmierzone 421 B) i test architektoniczny na atrapie `PanickingPlaces`. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M3b-dzien-mieszkanca.md` |

Warstwa, na której stoi cała reszta fazy: komponenty SoA i magazyny o zmiennej długości, potrzeby z deprywacją, silnik DES i dwa punkty rozszerzenia (`PlaceProvider`, `TravelOracle`) z implementacjami tymczasowymi.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| ✅ WP1 | Komponenty i magazyny | M0 (`ecs`, `core`) | M |
| ✅ WP2 | Potrzeby i deprywacja | WP1 | S |
| ✅ WP3 | Silnik DES | WP1 | M |
| ✅ WP4 | Punkty rozszerzenia M4/M5 + implementacje tymczasowe | WP1, M2 (parcele, budynki) | S |

### WP1 — Komponenty i magazyny

Definicja wszystkich komponentów SoA z sekcji 5.1, trzech magazynów o zmiennej długości (arena planu, slab relacji, slab wiedzy), rejestracja w funkcji haszującej stan ECS.

**Kryterium ukończenia:** test `size_of` dla każdego komponentu zgodny z tabelą budżetu; `mem_population_400k` pokazuje zużycie ≤ **430 B**/mieszkańca stanu gorącego (bez magazynu wiedzy — korekta D-1); hash stanu stabilny po serializacji/deserializacji **komponentów** (zasoby wejdą do snapshotu w M12 — korekta D-4).

**Stan:** ✅ zamknięty. `components.rs` (13 komponentów, 140 B), `store.rs` (slab z klasami 4/8/12/16/24/32, trzy magazyny), `arrayvec.rs`. Zmierzone przy 400 tys.: ECS 148 B, relacje 97 B, plany 157 B, kolejka 19 B → **421 B na mieszkańca**.

### WP2 — Potrzeby i deprywacja

Tabela temp spadku (dane RON), `NeedDecaySystem` shardowany 1/60 na tick minutowy, `DeprivationEffectsSystem` przeliczający wpływ na `Vitals`.

**Kryterium ukończenia:** mieszkaniec bez żadnego zaspokojenia osiąga stan krytyczny w czasie zgodnym z tabelą (±2%); sharding nie zmienia wyniku względem wersji referencyjnej „wszyscy co godzinę" (tolerancja 0).

**Stan:** ✅ zamknięty. Tolerancja 0 nie jest tu wynikiem kalibracji, tylko konstrukcji: spadek liczy się jako **różnica funkcji czasu absolutnego** (`needs::decay_between`), więc przeliczenie co minutę, co godzinę i shardowane 1/60 dają identyczny wynik. Testy: `shardowanie_nie_zmienia_wyniku`, `shard_daje_ten_sam_wynik_co_zamknieta_formula`, `czas_do_zera_zgadza_sie_z_tabela_paragrafu_5_5`.

### WP3 — Silnik DES

Koło czasu (`TimingWheel`) o 2880 kubełkach minutowych + `BTreeMap` przelewowy na zdarzenia dalekie (urodziny, śmierć, rocznice). Deterministyczne sortowanie kubełka, dispatch do handlerów.

**Kryterium ukończenia:** `bench_des_dispatch` — 4 mln zdarzeń/dobę gry przetworzone ≤ 250 ms sumarycznie na 1 wątku; test: losowa kolejność wstawiania 100 tys. zdarzeń w tę samą minutę → identyczna kolejność obsługi.

**Stan:** ✅ zamknięty. 4 mln zdarzeń doby w **50 ms** wobec 250 ms budżetu (szczyt 2 804 zdarzenia na tick); 100 tys. zdarzeń w jednej minucie wstawionych w dwóch przeciwnych kolejnościach daje identyczny ciąg.

### WP4 — Punkty rozszerzenia M4/M5

Traity `PlaceProvider` i `TravelOracle` plus implementacje tymczasowe `InfinitePlaces` i `WalkOracle`. **To jest kontrakt, nie szkic** — M5 i M4 podmieniają wyłącznie implementację, planer się nie zmienia.

**Kryterium ukończenia:** planer kompiluje się przeciwko traitom, nie przeciwko implementacjom (test: atrapa `PanickingPlaces` panikująca w każdej metodzie, podmieniona w teście, potwierdza brak zależności typów); żadna sygnatura w `planner.rs` nie zawiera `Money` w roli ceny, `GoodId` ani typu z grafu nawigacyjnego.

**Stan:** ✅ zamknięty. Planera jeszcze nie ma (M3b), więc rolę wywołującego pełni `places::choose_place` — wycinek fazy 3, który **będzie** wołany z planera i już teraz zna wyłącznie `&dyn PlaceProvider` (korekta D-5). `tests/contract.rs`: podstawienie `EmptyPlaces` i `PanickingPlaces` bez zmiany ani jednej linii u wołającego, `FlakyPlaces` przechodzi ścieżkę odmowy, dwa testy architektoniczne pilnują modułu `walk` i braku typów gospodarczych w kontrakcie.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.1 Komponenty ECS (SoA) i budżet pamięci

Wszystkie komponenty `#[repr(C)]`, bez paddingu niejawnego (test `size_of` + `offset_of`).

```rust
// ---------- stan gorący, tablice SoA ----------

#[repr(C)] pub struct Identity {        // 16 B
    pub first_name: u16,                // indeks w puli imion (data/names/) — pula powstaje w M4d/WP13
    pub last_name:  u16,                //   (patrz Z-7 w M4-ruch.md); do tego czasu to surowa liczba
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
// Plan dnia — blok w slabie (patrz korekta D-1; pierwotnie: wspólny Vec podwójnie buforowany)
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

Slaby relacji, wiedzy **i planów dnia**: klasy rozmiaru **4 / 8 / 12 / 16 / 24 / 32** wpisów, alokator blokowy bez fragmentacji (bloki stałej wielkości, wolna lista per klasa). Wpis 33. wypycha najstarszy o najniższej wadze (`score × decay(recency)`). Klasa 12 i trzeci magazyn to korekta D-1 — uzasadnienie w tabeli na końcu dokumentu.

Plan dnia dostaje blok w tym samym slabie, a nie własną arenę, bo mieszkaniec ma dokładnie jeden aktualny plan i zastępuje go w całości: `Slab::store` dobiera klasę od razu, zwalnia poprzedni blok i nie wymaga ani kompaktowania, ani dobowego przebiegu przepisującego `PlanRef.offset` wszystkim mieszkańcom. `PlanRef.offset` jest **uchwytem do slabu**: klasa w bitach 29..31, numer bloku niżej — komponent zostaje ośmiobajtowy.

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

Do tego magazyny i narzut. Kolumna „plan" to rachunek z pierwszego projektu, kolumna
„pomiar" — to, co pokazuje `mem_population_400k` przy 400 tys. mieszkańców (korekta D-1):

| Pozycja | plan | pomiar | skąd różnica |
|---|---|---|---|
| komponenty + `Entity` + chunkowanie ECS | 156 | **148** | chunk mieści wiersz bez odpadu; 13 komponentów po 140 B + 8 B uchwytu |
| plan dnia: śr. 12 slotów × 12 B | 144 | **157** | klasa rozmiaru zaokrągla 14 i 16 slotów do bloku 16 |
| slab relacji: śr. 10 wpisów × 8 B | 80 | **97** | klasa rozmiaru zaokrągla 18 wpisów do 24, a 26 do 32 |
| kolejka DES: 1 zdarzenie w locie × 16 B | 16 | **19** | nagłówki 2880 kubełków i zapas wzrostu porcjami po 64 |
| **stan gorący razem** | **396** | **421** | |

**Pierwotny rachunek nie uwzględniał narzutu alokacji w ogóle** — a slab z klasami
rozmiaru, który ten sam paragraf zaleca, z definicji zaokrągla w górę. Budżet kryterium
podnosi się więc do **430 B** (zmierzone 421, zapas na drobne zmiany kształtu), co przy
400 tys. mieszkańców daje **161 MB zamiast 151 MB** stanu gorącego. Wniosek z §17.7
(„metropolia mieści się w 6 GB") stoi z tym samym zapasem — zmieniła się liczba
w kryterium, nie wniosek z niej.

Dwie rzeczy, które przy okazji **zeszły** z rachunku, bo były realnym marnotrawstwem,
a nie ceną projektu: podwójne buforowanie areny planów (plan wczorajszy żyje obok
dzisiejszego przez całą dobę, czyli 288 B zamiast 144 — stąd slab) i geometryczny wzrost
`Vec` w slabach i kubełkach koła czasu (drugie tyle pamięci, której nikt nie zapisze —
stąd `reserve_exact` porcjami).

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
    /// `who` dołożone w M3a (korekta D-3): prędkość marszu zależy od wieku, zdrowia
    /// i energii, a M4 i tak będzie potrzebował podróżnika, żeby dobrać mu pojazd.
    fn begin_trip(&mut self, trip: TripRequest, who: &CitizenView<'_>, q: &mut EventQueue)
        -> TripHandle;

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
| Hunger | 6,0 pkt/h | 16,7 h | Meal (dom / lokal) | energia −2/h, zdrowie −0,2/h, produktywność ×0,7 |
| Sleep | 4,2 pkt/h na jawie, +25 pkt/h we śnie | 23,8 h | Sleep (dom, hotel) | energia −3/h, ryzyko wypadku ×2, p(absencja) = 0,15 |
| Hygiene | 4,2 pkt/h | 23,8 h | Hygiene (dom) | zdrowie −0,1/h, status −5 |
| Health | zdarzeniowe (choroba −10..−60) | — | Doctor, Pharmacy, Hospital | absencja, hazard śmierci × k(wiek) |
| Safety | 0,15 pkt/h, modyfikowane przestępczością dzielnicy | 27,8 dnia | dzielnica, policja (M8) | stres +2/dobę, p(migracji GD) ↑ |
| Housing | 0,05 pkt/h + skok przy zmianie wielkości GD | 83,3 dnia | odpowiedni lokal | stres +1/dobę, znacznik „szuka mieszkania" (M5/M9) |
| Mobility | 4,2 pkt/h | 23,8 h | dostęp do trasy (M3 pieszo; M4 reszta) | brak dostępu do pracy → absencja |
| Clothing | 0,6 pkt/h, ×2 przy zmianie sezonu | 6,9 dnia | Clothing | status −3, komfort termiczny ↓ |
| Leisure | 3,0 pkt/h | 33,3 h | Leisure (kino, park, restauracja, dom) | nastrój −1/h |
| Social | 2,5 pkt/h, ×0,5 gdy w tym samym lokalu co relacja | 40,0 h | Social (rodzina, lokale) | nastrój −1/h |
| Status | 0,1 pkt/h | 41,7 dnia | konsumpcja statusowa (M5), adres, auto (M4) | ambicja ↑, satysfakcja ↓ |
| Development | 0,05 pkt/h | 83,3 dnia | Education, kursy | ambicja ↑ → wyzwalacz zmiany pracy w M7 |

Kolumna „pełna → 0" jest **konsekwencją tempa** (100 · 6000 / tempo minut), nie drugim
parametrem — po korekcie D-2 podaje wartość dokładną zamiast zaokrąglonej do ładnej liczby.
Test `czas_do_zera_zgadza_sie_z_tabela_paragrafu_5_5` porównuje z nią dane z ±2 %.

**System spadku.** `NeedDecaySystem` działa `EveryMinute`, ale przetwarza **1/60 populacji na tick** (shard = `entity_index % 60`); shard, na który przypada kolej, dostaje spadek za pełne 60 minut. To jest **dokładne** (nie przybliżone), deterministyczne i równomiernie rozkłada koszt: 400 k / 60 = 6 667 mieszkańców × 16 B = 107 KB odczytu i zapisu na tick, poniżej 30 µs.

**Zaspokojenie.** Wykonanie slotu `Meal`, `Sleep`, `Leisure`, `Shopping`… wywołuje `PlaceProvider::fulfil`, którego `satisfaction` dodaje się do poziomu z saturacją na 100. W M3 `InfinitePlaces` zwraca stałe wartości z `data/needs/needs.ron`; w M5 wartość zależy od kupionego dobra — planer się nie zmienia.

**Skutki deprywacji — kto je stosuje.** M3a stosuje wyłącznie skutki **rate'owe dotykające
`Vitals`**: `EnergyLoss`, `HealthLoss`, `MoodLoss`, `StressGain`. Pozostałe są w danych
i wychodzą przez `deprivation_of`, ale stosuje je faza będąca ich właścicielem:
`StatusLoss` → M3c (tam status jest **liczony**, a nie odejmowany, §5.8), `AbsenceRisk`
→ M3b (planer), `ProductivityLoss` → M7, `AmbitionGain` → M7. Rozstrzygnięcie D-6.

---

## Zmiany wpisane po M3a

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| D-1 ★ | **Budżet stanu gorącego: 430 B zamiast 400 B** (zmierzone 421). Plan dnia przenosi się z podwójnie buforowanej areny do slabu, slab dostaje klasę 12, a slaby i kubełki koła czasu rosną `reserve_exact` porcjami zamiast geometrycznie | Pierwotny rachunek §5.1 (396 B) **nie uwzględniał narzutu alokacji w ogóle**, a slab z klasami rozmiaru, który ten sam paragraf zaleca, z definicji zaokrągla w górę: 10 relacji to blok 12, 14 slotów planu to blok 16. Pierwszy pomiar dał 513 B. Dwie pozycje były realnym marnotrawstwem i zeszły: podwójne buforowanie areny (plan wczorajszy żyje obok dzisiejszego przez całą dobę, bo mieszkaniec wykonuje go do pobudki — 288 B zamiast 144) oraz podwajanie pojemności `Vec` (drugie tyle pamięci, której nikt nie zapisze). Reszta, 25 B, jest ceną projektu, nie błędem implementacji. Skutek dla §17.7: 161 MB zamiast 151 MB przy 400 tys. mieszkańców — wniosek „metropolia w 6 GB" stoi z tym samym zapasem |
| D-2 ★ | **Kolumna „pełna → 0" w §5.5 podaje wartości dokładne**, nie zaokrąglone: 16,7 h zamiast ~17 h, 83,3 dnia zamiast ~80 dni, 41,7 dnia zamiast ~40 dni | Kolumna jest konsekwencją tempa (100 · 6000 / tempo), a nie drugim parametrem. Przy zaokrągleniach kryterium WP2 („±2 %") było niespełnialne dla trzech potrzeb, mimo że dane zgadzały się z temperami co do jednej setnej punktu. Tempo jest parametrem i zostaje bez zmian |
| D-3 ★ | **`TravelOracle::begin_trip` dostaje `who: &CitizenView`** | Prędkość marszu zależy od wieku, zdrowia i energii (§5.10), więc bez podróżnika `begin_trip` musiałby albo ufać czasowi policzonemu wcześniej przez `estimate`, albo liczyć dla przeciętnego mieszkańca. Sygnatury traitów są zamrożone od M3 (§6.4) i M4 ma ich **nie zmieniać** — lepiej dołożyć parametr teraz, gdy nikt jeszcze nie implementuje, niż zmuszać M4 do złamania tamtej reguły. M4 i tak potrzebuje podróżnika, żeby dobrać mu pojazd |
| D-4 ★ | **Kryterium WP1 „hash stabilny po serializacji/deserializacji" dotyczy komponentów, nie zasobów.** `register` rozbite na `register_components` + `register_resources` | Minimalny snapshot z M0 niesie archetypy ECS; zasobów (slaby, kolejka zdarzeń) nie serializuje, a `world_state_hash` je hashuje przez haki. Świat z zarejestrowanymi hakami nie przechodzi więc round-tripu i `load_world` odrzuca własny zapis z `HashMismatch`. To jest luka w `engine/io`, nie w M3 — poprawka poszła do `M12b-zapis-i-replay.md`. Do tego czasu test round-tripu używa `register_components`, a scenariusze `register` |
| D-5 | **Rolę „wołającego przez trait" w kryterium WP4 pełni `places::choose_place`**, a nie planer | Planer powstaje w M3b, więc kryterium „planer kompiluje się przeciwko traitom" nie dawało się w M3a sprawdzić inaczej niż tożsamościowo. `choose_place` to wycinek fazy 3 planera (wybór miejsca dla zadania), który M3b wywoła wprost — i który już teraz zna wyłącznie `&dyn PlaceProvider`. Przy okazji jest jedynym miejscem produkującym `PlaceUnknown` |
| D-6 | **M3a stosuje tylko skutki deprywacji dotykające `Vitals`**; `StatusLoss`, `AbsenceRisk`, `ProductivityLoss` i `AmbitionGain` są w danych, ale stosuje je faza-właściciel (M3c, M3b, M7) | Status jest w M3c **liczony** funkcją, a nie odejmowany (§5.8) — odejmowanie go tutaj rozjechałoby się z tamtą funkcją przy pierwszym uruchomieniu obu naraz. Absencja jest decyzją planera, produktywność wejściem do M7. Wartości zostają w `data/needs/needs.ron`, żeby faza, która je przejmie, nie wymyślała ich od nowa |
| D-7 | **`WalkOracle` w M3a liczy odległość manhattanowo z korektą 1,25×**, nie po centroliniach ulic | To jest fallback zapisany wprost w ryzyku R7 fazy. Odległość sieciowa wymaga geometrii ulic z M2, a więc zależności `sim/agents` → `sim/world`, której w M3a nie ma i mieć nie powinna. WP6 (M3b) zastępuje implementację nie zmieniając ani sygnatury, ani niczego u wywołujących — to jest cały sens `TravelOracle` |
| D-8 | **`InfinitePlaces` i `WalkOracle` stoją na `PlaceTable`** — katalogu `(PlaceRef, PlaceKind, WorldCoord)` z indeksem przestrzennym per rodzaj, wsypywanym z zewnątrz | Zależność WP4 od „M2 (parcele, budynki)" z tabeli pakietów jest w tej podfazie **pośrednia**: `sim/agents` nie zależy od `sim/world` (decyzja 9.11 mówi, że to `sim/world` rozszerza się o populację). Katalog wypełnia generator populacji w M3d, a w testach — ręcznie. Bez tego rozdzielenia crate agentów ciągnąłby za sobą cały generator miasta |
| D-9 | **Cztery słowniki wędrują do `engine/core`**: `PlaceKind`, `TraitId`, `DeprivationEffect`, `MinuteOfDay`; `NeedKind` dostaje dwanaście wariantów z §5.5 w miejsce dziesięciu roboczych z M0. Do `core` przenosi się też `assets::{data_dir, data_path}` | Wpisane jako `K-20` w dokumencie 00. `DeprivationEffect` **musi** być w `core`, bo jest ładunkiem `DecisionReason`; reszta spełnia kryterium K-8 (więcej niż jedna faza). `data_dir` przenosi się, bo tabelę potrzeb ładuje `sim/agents`, który o `sim/world` nie wie |
| D-10 | **`DecisionReason` zyskuje dwa warianty spoza listy z M3b §5.4**: `ModeWalkOnly` (113) i `NeedSatisfied` (114) | Lista z M3b nie miała powodu dla `TravelEstimate.reason` ani dla `FulfilOutcome::Done`, a oba pola są w kontrakcie §5.3 i oba wymagają wyjaśnienia (00 §7). To jest przypadek 5 z `K-18`: pakiet obiecywał pole, którego nikt nie był właścicielem. `discriminant()` liczy się teraz jawnym `match`, bo `self as u16` nie działa dla enuma z ładunkiem |
| D-11 | **`EventKind` ma pięć wariantów, nie cztery** — doszedł `EndActivity` (3); `PlanDay` przesuwa się na 4 | Lazy scheduling z §5.2 („obsługa zdarzenia harmonogramuje kolejne") wymaga zdarzenia kończącego czynność — bez niego łańcuch się rwie po `StartActivity`. Kolejność wariantów jest kolejnością obsługi przy remisie czasowym, więc `EndActivity` musi stać przed `PlanDay` |
| D-13 ★ | **`bench_need_decay`: ≤ 100 µs na 8 wątkach zamiast ≤ 30 µs.** Zmierzone: **63 µs** przy 400 tys. mieszkańców (342 µs na jednym wątku); skutki deprywacji 236 µs raz na godzinę, czyli 4 µs na tick po rozłożeniu. Oba systemy idą `par_for_each` po chunkach (00 §3.3) | Próg 30 µs pochodził z rachunku „6 667 mieszkańców × 16 B = 107 KB odczytu i zapisu" — czyli z kosztu **dotknięcia** shardu, z pominięciem kosztu jego **znalezienia**. Archetypowy ECS nie umie zaadresować „co sześćdziesiątej encji": trzeba przejść wszystkie 400 tys. wierszy i odrzucić 59/60. Boczny indeks encja → wiersz per shard kosztowałby pamięć i unieważnianie przy każdych narodzinach i każdym zgonie, czyli znacznie więcej niż te 63 µs, które są **0,007 % ticku** przy prędkości 1×. Sharding zostaje po `entity_index`, nigdy po pozycji w archetypie (ryzyko R12) |
| D-12 | **`sim/agents` dostaje własny `ArrayVec<T, N>`** (`T: Copy + Default`, bez `unsafe`) zamiast zależności `arrayvec` | Limity w kontraktach fazy są twarde: 16 kandydatów (R6), 24 sloty planu, 8 miejsc z trasy. `SmallVec`, który jest już w projekcie, przy przepełnieniu cicho przechodzi na stertę — czyli robi dokładnie to, przed czym limit ma bronić. Sześćdziesiąt linii zamiast nowej zależności; `arrayvec` z crates.io dopiero gdyby trzeba było trzymać tam typ z destruktorem |

## Zmiany wpisane po M3d

Zgodnie z `K-18`. Dotyczą **tabeli potrzeb z §5.5**, której M3a jest właścicielem;
uzasadnienia są w tabeli `H-n` dokumentu `M3d-populacja-i-ui.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| D-15 ★ | **Potrzeba bez sposobu zaspokojenia ma tempo spadku 0.** `Safety`, `Housing`, `Mobility` i `Status` dostają 0 w `data/needs/needs.ron`; niezmiennik pilnuje test `potrzeba_bez_sposobu_zaspokojenia_nie_spada` (korekta H-3) | Tabela §5.5 opisywała spadek dla wszystkich dwunastu potrzeb, ale miejsca zaspokojenia ma sześć. Potrzeba, która spada i której nic nie podnosi, dochodzi do zera u **wszystkich** — i wtedy jej skutki przestają cokolwiek różnicować. Mobilność z `AbsenceRisk` 1000 zatrzymała w drugiej dobie całe miasto w pracy. Tempo wpisze faza, która wnosi mechanizm: M4, M8, M5/M9 |
| D-16 ★ | **`satisfaction` jest przyrostem z wizyty i stosuje się go na jej końcu**, a **czas wizyty nie liczy się do spadku tej potrzeby** (korekta H-4) | §5.5 nie mówiło, kiedy przyrost się dzieje; §5.12 zgadywało, że przy `StartActivity`. Komentarz w danych mówi o tempie snu „na jawie", a sen trwa osiem godzin — bez oddania spadku z czasu wizyty żadna potrzeba nie utrzymywała się powyżej zera. Rozwiązanie nie wymaga, żeby `NeedDecaySystem` wiedział, co mieszkaniec robi |
| D-18 | **Test `czas_do_zera_zgadza_sie_z_tabela_paragrafu_5_5` traci cztery wiersze i zyskuje listę potrzeb zdarzeniowych** | Tabela „pełna → 0" z §5.5 opisywała jedenaście potrzeb; po D-15 spada siedem. Cztery pozostałe dostają **własnego** strażnika — test sprawdza teraz, że nie spadają wcale. Bez tego zmiana danych zostawiłaby po sobie dziurę w teście zamiast asercji |
| D-17 | `Hygiene.satisfaction` 35 → **100** (korekta H-5) | Kąpiel przywraca higienę w całości. Przy jednej wizycie dziennie i spadku 100 pkt na dobę wartość 35 trzymała higienę stale przy zerze |
