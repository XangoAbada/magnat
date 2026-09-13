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

## 4. Pakiety robocze (WP)

Kolejność topologiczna. „Zależy od" oznacza twardą zależność kompilacyjną lub testową.

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP1 | Komponenty i magazyny | M0 (`ecs`, `core`) | M |
| WP2 | Potrzeby i deprywacja | WP1 | S |
| WP3 | Silnik DES | WP1 | M |
| WP4 | Punkty rozszerzenia M4/M5 + implementacje tymczasowe | WP1, M2 (parcele, budynki) | S |
| WP5 | Planer dnia | WP2, WP3, WP4 | L |
| WP6 | Ruch pieszy za `TravelOracle` (mezo + mikro) | WP3, WP4, M2 (geometria ulic) | M |
| WP7 | GD, cykl życia, demografia | WP1 | L |
| WP8 | Migracja i regulacja populacji | WP7 | M |
| WP9 | Status, relacje, plotka | WP1, WP7 | M |
| WP10 | Generacja populacji (Etap 8) | WP7, WP9, M2 (lokale), M1/M2 Etap 7 (miejsca pracy) | L |
| WP11 | `engine/ui` — szkielet, selekcja, sterowanie czasem | M1 (`render`) | M |
| WP12 | Karta inspekcji z osią czasu dnia | WP5, WP11 | M |
| WP13 | Testy własnościowe, determinizm, benchmarki | wszystkie | M |

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

### WP5 — Planer dnia

Algorytm 4-fazowy, `DayCanvas`, `DecisionReason` per slot, tryb `explain`, replanning z debouncingiem.

**Kryterium ukończenia:** wydruk dnia dla scenariusza wzorcowego zgodny strukturalnie z §5.5 (pobudka → posiłek → dojazd → praca → zadanie po drodze → dom → czas wolny → sen); `plan_day` czysta funkcja (1000 wywołań = identyczne bajty); `plan_day_explained` daje identyczne sloty co `plan_day`; benchmark ≤ 10 µs mediana.

### WP6 — Ruch pieszy za `TravelOracle`

Mezo: `WalkOracle` liczy czas dojścia i emituje zdarzenie `Arrive`. Mikro: interpolacja pozycji po polilinii na ticku 100 ms dla encji w LOD Mikro.

**Zgodnie z K-2 M3 nie tworzy modułu routingu.** `WalkOracle` jest prywatną implementacją `TravelOracle` w `sim/agents`, opartą na **odległości sieciowej po centroliniach ulic z M2** (BFS po ważonym grafie odcinków ulic, budowanym jednorazowo przy starcie i trzymanym prywatnie). Nie eksportuje typów grafu, nie ma publicznego API `route()`, `path()` ani `graph()`. M4, tworząc `engine/nav`, dostarcza własną implementację `TravelOracle` i **usuwa `WalkOracle` w całości** — nie ma nic do zmigrowania.

**Kryterium ukończenia:** test spójności LOD — ten sam scenariusz w Mikro i Mezo daje identyczne czasy przybycia i identyczny hash stanu potrzeb (tolerancja 0); 5 tys. pieszych w kadrze w LOD Mikro ≤ 2 ms/klatkę; test architektoniczny: `pub use` crate'a `sim/agents` nie eksportuje żadnego typu z modułu `walk`.

### WP7 — GD, cykl życia, demografia

Typy GD, przejścia stanów, hazardy (płodność, umieralność, formowanie związku, rozwód), dziedziczenie. Wszystkie hazardy roczne w skali roku 360-dniowego (K-1).

**Kryterium ukończenia:** `m3_century` — populacja po 36 000 dniach w przedziale [0,5×, 2,0×] startowej dla 5 seedów; tożsamość księgowa populacji zachowana co do jednostki (sekcja 7).

### WP8 — Migracja i regulacja populacji

Napływ/odpływ GD sterowany wakatami pracy i pustostanami mieszkaniowymi. **To jest jedyny regulator populacji** — nie ma sztucznego „spawnowania do celu".

**Kryterium ukończenia:** eksperyment szokowy — usunięcie 20% miejsc pracy powoduje odpływ i stabilizację bezrobocia w ≤ 5 latach gry, bez oscylacji o amplitudzie > 30%.

### WP9 — Status, relacje, plotka

Funkcja statusu, przedziały klas, budowa i zanik relacji, propagacja wiedzy o miejscach.

**Kryterium ukończenia:** nowe miejsce dodane do świata jest znane ≥ 60% mieszkańców w promieniu 1 km po 30 dniach gry i < 5% mieszkańców powyżej 5 km (test §5.7 — fundament pod markę w M10).

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

## 5. Projekt techniczny

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

### 5.6 GD, cykl życia, demografia (§5.2)

```rust
#[repr(C)] pub struct Household {          // 128 B
    pub kind: u8,          // Single, Couple, FamilyWithKids, MultiGen, Roommates, Dorm, LoneSenior
    pub size: u8,
    pub flags: u16,
    pub district: u16,
    pub _pad: u16,
    pub members: [u32; 6],                // >6 osób: przelew do bocznej mapy (rzadkie)
    pub building: u32, pub unit: u16, pub _pad2: u16,
    pub cash: Money, pub bank: Money, pub savings: Money, pub debt: Money,  // POLA; M5 operuje
    pub income_monthly: Money,            // z generacji; M5/M7 aktualizują
    pub stock: [u8; 8],                   // dni zapasu per StockCat — M5 zastąpi realnymi towarami
    pub shopper_rotation: u8,             // indeks członka robiącego zakupy (rotacja wg grafików)
    pub vehicle_slots: [u32; 2],          // M4 wypełnia
    pub _reserved: [u8; 16],              // rezerwa dla M5/M9, żeby nie przebudowywać archetypu
}
```

**Hazardy** (dane w `data/demography/` — katalog dopisany do doc 00 §5 — per epoka i klasa). Wszystkie stopy roczne odnoszą się do roku **360-dniowego** (K-1).

| Przejście | Model | Wyzwalacz |
|---|---|---|
| Narodziny | hazard roczny f(wiek matki, liczba dzieci, klasa, `Housing`, `income_monthly`) | `DemographySystem`, `EveryDay`, sharding **1/360** |
| Dobór partnera | kandydaci z grafu relacji (`Friend`, `Coworker`, `Neighbour`) + kompatybilność statusu \|Δstatus\| ≤ 20, wiek ±8 lat | `EveryMonth` (30 dni) |
| Ślub / wspólne GD | po N miesiącach relacji o wadze > 70 | `EveryMonth` |
| Rozwód | hazard f(stres obojga, `debt/income`, czas trwania związku) | `EveryMonth` |
| Starzenie | wiek liczony jako `(day − birth_day) / 360`, nie przechowywany | — |
| Emerytura | wiek ≥ próg epoki → `Employment.flags = Retired` | zdarzenie DES na urodziny |
| Choroba | hazard f(wiek, `health`, `hygiene`, sezon) | `EveryDay` |
| Śmierć | hazard f(wiek, `health`) + zdarzenia (M8) | `EveryDay` |
| Dziedziczenie | `Money` GD + lokal → współmałżonek, dalej dzieci po równo (reszta do pierwszego wg `entity_index`, doc 00 §2) | zdarzenie `Death` |

**Dziedziczenie w M3** przenosi wyłącznie pieniądz i mieszkanie. Firmy, aktywa i długi hipoteczne wchodzą hookiem:

```rust
pub trait InheritanceHook: Send + Sync {
    /// Wołany po podziale Money i mieszkania, przed usunięciem encji zmarłego.
    fn on_inheritance(&mut self, deceased: CitizenId,
                      heirs: &[(CitizenId, u16 /* permil */)],
                      cmd: &mut CommandBuffer);
}
```
M7 rejestruje tu przekazanie udziałów w firmach, M5 — długów i rachunków. Permile sumują się do 1000, reszta do pierwszego dziedzica wg `entity_index` — ta sama reguła co podział kwoty w doc 00 §2.

**Warunek determinizmu:** żaden hazard nie jest losowany zbiorczo dla całej populacji. Każde zdarzenie życiowe ma własny strumień `rng(seed, StreamId::Demography, citizen_idx, day)`, więc dodanie lub usunięcie mieszkańca nie przesuwa losowań pozostałych. Bez tego zapis/odczyt i przejścia LOD łamią hash stanu.

### 5.7 Migracja i regulacja populacji (§5.2)

Populacja **nie jest sterowana do celu**. Regulatorem jest migracja GD, reagująca na dwa obserwowalne sygnały miasta:

```
wakaty         = liczba JobSlot bez pracownika
pustostany     = liczba lokali bez GD
napływ/mies.   = k_in  · min(wakaty, pustostany) · atrakcyjność_miasta
odpływ/mies.   = k_out · Σ_GD p_wyjazdu(GD)
p_wyjazdu(GD)  = f(miesiące bez pracy któregokolwiek dorosłego,
                   Needs[Housing], Needs[Safety], zadłużenie/dochód)
atrakcyjność   = g(średnia płaca, bezrobocie, bezpieczeństwo, wartość gruntu)
```

Napływające GD generuje **ten sam generator co Etap 8** (ten sam kod, mniejsze N) i od razu dostają pracę oraz mieszkanie; GD, które nie znajdzie żadnego w ciągu 3 miesięcy (90 dni), wyjeżdża. To zamyka pętlę ujemnego sprzężenia bez sztucznego spawnowania.

Testy sekcji 7 weryfikują, że sama migracja wystarcza do stabilności w 100 lat gry. Jeśli nie wystarczy — decyzja 9.9 (człon tłumiący kalibrowany balansatorem, świadomie **nie** wprowadzany na zapas).

### 5.8 Status, relacje, plotka (§5.4, §5.7)

**Status** (`StatusSystem`, `EveryMonth`, sharding 1/30):

```
status = clamp_0_100(
     w_i · percentyl(dochód_GD_per_capita)
   + w_w · percentyl(majątek_GD)
   + w_e · edu_level_score
   + w_o · prestiż_zawodu(JobRoleId)          // data/jobs/
   + w_a · percentyl(wartość gruntu dzielnicy)
   + w_c · konsumpcja_statusowa                // M3: 0; M5 wypełnia
   + w_r · reputacja_rodziny                   // średnia statusu rodziców, zanikająca z wiekiem
)
```

Klasy = przedziały: niższa 0–15, robotnicza 16–33, niższa średnia 34–52, wyższa średnia 53–72, wyższa 73–89, elita 90–100. Klasa jest **wyliczana, nie przechowywana** — mobilność społeczna (§5.4) wychodzi sama, bez osobnej mechaniki.

**Relacje.** Tworzenie: rodzina i współlokatorzy (z generacji/narodzin, waga 80–100); współpracownicy (wspólny `site`, +1/dobę wspólnej zmiany, max 70); sąsiedzi (wspólny budynek/kwartał, max 50); znajomi z miejsca (spotkanie w tym samym slocie `Leisure`/`Social`, max 60). Zanik: `RelationDecaySystem`, `EveryDay`, sharding 1/7 — waga −1 za każdy pełny tydzień bez kontaktu, usunięcie przy wadze 0 (poza rodziną).

**Plotka.** `GossipSystem`, `EveryDay`, sharding 1/7:

```
dla mieszkańca M (w shardzie dnia):
  wybierz ≤2 relacje losowane z wagą ∝ Relation.weight, tylko weight ≥ 40
  dla każdej relacji R:
     wybierz z wiedzy M 1 wpis o najwyższym (score × świeżość), którego R jeszcze nie ma
     wstaw do R: Knowledge{ target, day: dziś, score: score − 10, kind: Heard }
```

**Widoczność z trasy.** Przy zmianie trasy dom↔praca (a w M3 tylko wtedy — trasa jest cache'owana) `TravelOracle::places_on_route` zwraca miejsca w buforze 50 m od przebiegu; trafiają do wiedzy jako `SeenOnRoute` ze `score = 35`.

To jest dokładnie mechanika z §5.7: nowy sklep zna początkowo tylko ten, kto go mija lub w nim był; zasięg buduje się przez relacje. M10 dopisze `kind = Ad` i niczego innego nie musi zmieniać.

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

### 5.10 Ruch pieszy (za `TravelOracle`, K-2)

- **Mezo (domyślnie).** `WalkOracle::estimate` liczy czas dojścia po **odległości sieciowej** na ważonym grafie odcinków ulic z M2 (budowanym raz przy starcie, trzymanym **prywatnie** w module `sim/agents::walk`). Cache LRU + **przypięta** para tras dom↔praca per mieszkaniec. Prędkość bazowa 1,35 m/s, modyfikowana wiekiem, zdrowiem i energią. `begin_trip` harmonogramuje `Arrive` w `depart + minutes` i nic więcej nie robi — brak pozycji, brak ticku 100 ms.
- **Mikro (tylko encje w kadrze / śledzone).** Pozycja interpolowana po polilinii na ticku 100 ms, wpis w `PedestrianBuffer` (32 B: pozycja f32×3, postęp f32, indeks trasy, kierunek). **Bez unikania kolizji i bez steeringu** — przy voxelu 1 m tłum czyta się dobrze bez tego; steering należy do M11 razem z animacjami.
- **Spójność LOD (doc 00 §4).** Czas przybycia liczy **wyłącznie** warstwa mezo. Mikro jest interpolacją między wyliczonym startem a wyliczonym przybyciem i nie może go zmienić. Dlatego test spójności LOD przechodzi z tolerancją 0 z definicji, a nie przez kalibrację.
- **Granica z M4 (K-2).** Moduł `walk` jest `pub(crate)`. Zero publicznych typów grafu, zero funkcji `route`/`path`/`graph` w API crate'a. Test architektoniczny sprawdza, że publiczne API `sim/agents` nie eksportuje niczego z `walk`. Gdy M4 dostarczy `engine/nav`, moduł znika w całości — nie ma nic do zmigrowania ani do utrzymywania równolegle.

`ponytail:` odległość sieciowa zamiast pełnego A* z heurystyką — przy dystansach pieszych (≤ 3 km) BFS po ważonym grafie z wcześniejszym przerwaniem wystarcza, a i tak cała ta implementacja jest do wyrzucenia w M4.

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
| `bench_need_decay` | shard 1/60 przy 400 tys. ≤ 30 µs/tick |
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
