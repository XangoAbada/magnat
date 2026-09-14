# M3c — Demografia i społeczeństwo

Podfaza 3 z 4 fazy **M3 — Ludzie i dzień** (`M3-ludzie-i-dzien.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M3a (komponenty, DES). |
| **Pakiety robocze** | WP7, WP8, WP9 |
| **Projekt techniczny** | §5.6, §5.7, §5.8 |
| **Wynik do pokazania** | `m3_century` — 100 lat gry headless bez wybuchu ani wygaszenia populacji. |
| **Kryterium zamknięcia** | Kryteria WP7–WP9; tożsamość księgowa populacji zachowana co do jednostki. |
| **Poprzednia / następna** | `M3b-dzien-mieszkanca.md` · `M3d-populacja-i-ui.md` |

Gospodarstwa domowe i cykl życia z hazardami rocznymi, migracja jako jedyny regulator populacji, status społeczny, relacje i propagacja wiedzy o miejscach.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP7 | GD, cykl życia, demografia | WP1 | L |
| WP8 | Migracja i regulacja populacji | WP7 | M |
| WP9 | Status, relacje, plotka | WP1, WP7 | M |

### WP7 — GD, cykl życia, demografia

Typy GD, przejścia stanów, hazardy (płodność, umieralność, formowanie związku, rozwód), dziedziczenie. Wszystkie hazardy roczne w skali roku 360-dniowego (K-1).

**Kryterium ukończenia:** `m3_century` — populacja po 36 000 dniach w przedziale [0,5×, 2,0×] startowej dla 5 seedów; tożsamość księgowa populacji zachowana co do jednostki (sekcja 7).

### WP8 — Migracja i regulacja populacji

Napływ/odpływ GD sterowany wakatami pracy i pustostanami mieszkaniowymi. **To jest jedyny regulator populacji** — nie ma sztucznego „spawnowania do celu".

**Kryterium ukończenia:** eksperyment szokowy — usunięcie 20% miejsc pracy powoduje odpływ i stabilizację bezrobocia w ≤ 5 latach gry, bez oscylacji o amplitudzie > 30%.

### WP9 — Status, relacje, plotka

Funkcja statusu, przedziały klas, budowa i zanik relacji, propagacja wiedzy o miejscach.

**Kryterium ukończenia:** nowe miejsce dodane do świata jest znane ≥ 60% mieszkańców w promieniu 1 km po 30 dniach gry i < 5% mieszkańców powyżej 5 km (test §5.7 — fundament pod markę w M10).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

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

---

## Zmiany wpisane po M3a

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3a.

| # | Zmiana | Dlaczego |
|---|---|---|
| C-1 | **`Household` powstaje w tej podfazie, nie w M3a.** M3a definiuje trzynaście komponentów mieszkańca i zostawia `Identity.household` jako indeks encji | §5.1 (M3a) nie opisuje `Household` — opisuje go §5.6, czyli ta podfaza. Budżet §17.7 liczy GD osobno (≈167 tys. × 128 B), poza stanem gorącym mieszkańca |
| C-2 | **Slaby relacji i wiedzy są gotowe** (`RelationSlab`, `KnowledgeSlab`, klasy 4/8/12/16/24/32, wolna lista, `occupied_blocks()` do wykrywania wycieków). Wypychanie 33. wpisu bierze indeks ofiary z domknięcia wywołującego, a nie z traitu | Dwa magazyny, dwie różne reguły rangowania (`Knowledge::rank` jest w `store.rs`), obie jednolinijkowe u wołającego — trait na to nie zarabia. `prop_knowledge_bound` ma gotowe `occupied_blocks()` |
| C-3 | **`StatusLoss` stosuje ta podfaza**, przy funkcji statusu z §5.8. M3a go nie odejmuje, tylko wystawia przez `deprivation_of` | Rozstrzygnięcie D-6: status jest tu **liczony**, a nie odejmowany — odejmowanie go w M3a rozjechałoby się z tą funkcją przy pierwszym uruchomieniu obu naraz. To samo dotyczy `AmbitionGain` (M7) |
| C-4 | **`ReplanCause::HouseholdEvent { kind: HhEventKind }` istnieje** z wariantami `Birth`, `ChildIll`, `Death`, `MemberJoined`, `MemberLeft`, `Moved`; `is_full_replan()` zwraca `true` dla `Death` | Zdarzenia demograficzne mają już punkt zaczepienia w planerze — WP7 wypełnia je treścią, nie projektuje od nowa |
| C-5 | **Strumienie RNG tej podfazy są przypisane i zamrożone:** `Demography` 141, `Gossip` 142, `Migration` 145, `Relations` 146 (`K-4`, blok M3 = 140–159) | Wartości raz nadane są niezmienne — zmiana numeru strumienia zmienia każdy świat wygenerowany wcześniej z tego samego ziarna |
