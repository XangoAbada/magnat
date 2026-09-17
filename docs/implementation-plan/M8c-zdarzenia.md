# M8c — Zdarzenia

Podfaza 3 z 5 fazy **M8 — Miasto jako aktor** (`M8-miasto-jako-aktor.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M8b (sondy stanu sieci), M1 (klimat), M3 (nastroje), M7 (stan firm). |
| **Pakiety robocze** | WP4, WP5, WP6 |
| **Projekt techniczny** | §5.5, §5.6 |
| **Wynik do pokazania** | Susza, awaria elektrowni i strajk odpalają się z danych, deterministycznie, z czytelnym logiem „dlaczego jeszcze nie”. |
| **Kryterium zamknięcia** | Kryteria WP4–WP6; testy T3 i T7 zielone. **Zamknięte 2026-09-17**: przebieg 400 dób daje 17 zdarzeń ze wszystkich sześciu kategorii, T7 zielony strukturalnie i na katalogu, T3 w dwóch przebiegach z trzech — trzeci (zapis i wczytanie) jest nieosiągalny w tej fazie i ma adres M12b (`CE-15`). |
| **Poprzednia / następna** | `M8b-sieci-przesylowe.md` · `M8d-uslugi-i-prawo.md` |

Rdzeń `sim/events` z hazardem, `ParamOverlay` i rejestrem, katalog zdarzeń jako dane (6 kategorii × min. 8 definicji) oraz systemy dynamiczne stałe: pogoda, epoki, `MacroIndicators`.

---

## Pakiety robocze

### WP4 — Rdzeń `sim/events`: hazard, `ParamPatch`, rejestr zdarzeń
**Zależy od:** WP3 (sondy stanu sieci), M1 (klimat), M3 (nastroje), M7 (stan firm).
`EventDef` ładowany z `data/events/`, `EventTrigger` z krzywymi całkowitoliczbowymi,
`Probe` z cache per tick, losowanie ze strumienia `StreamId::EventRoll`, `ParamOverlay`
nakładający modyfikatory na parametry bazowe. Zakaz zadawania cen wpisany w typ `SimParam`.
**Kryterium ukończenia:** test T3 (pełny determinizm) + test T7 (żaden wariant `SimParam` nie
dotyka oferty, ceny ani marży) zielone.
**Rozmiar:** L
**Stan:** `[x]` zamknięty. `sim/events` (10 modułów), `sim/events/tests/no_prices.rs` (T7,
4 testy — w tym strażnik na źródle, że w module hazardu nie ma ani jednego floata)
i `tools/headless/tests/events.rs` (T3 i dynamiczna część T7) zielone.

### WP5 — Katalog zdarzeń jako dane
**Zależy od:** WP4.
Sześć kategorii z §11.2 w `data/events/{natural,infrastructure,external,social,firm,political}.ron`.
Minimum 8 definicji na kategorię, w tym trzy wzorcowe z sekcji 5.5 (susza, awaria elektrowni, strajk).
Walidator danych w CI: każda sonda istnieje, każdy `SimParam` jest osiągalny, krzywe monotoniczne
tam, gdzie deklarowane, `max_concurrent` i `cooldown` sensowne.
**Kryterium ukończenia:** walidator zielony; przebieg 5 lat gry generuje zdarzenia ze wszystkich
sześciu kategorii, a rozkład liczby zdarzeń na rok mieści się w widełkach z balansatora.
**Rozmiar:** M
**Stan:** `[x]` zamknięty — z korektą liczby definicji (`CE-9`): **31 definicji, 4–6 na kategorię**,
nie minimum osiem. Ogranicznikiem nie jest wyobraźnia, tylko liczba parametrów, które mają dziś
czytelnika. `sim/events/tests/catalog.rs` (4 testy) sprawdza m.in., że żadna definicja nie jest
martwa: w szczycie hazardu każda daje ≥ 1 wystąpienie na dwudziestolecie, a przy neutralnych
sondach ≤ 60 na rok.

### WP6 — Systemy dynamiczne stałe
**Zależy od:** WP4 (sondy), M1 (normy klimatyczne).
Pogoda (proces seedowany, zakotwiczony w normach M1), pory roku i kalendarz rolniczy,
`EpochClock` z `data/epochs/`, `MacroIndicators` (koszyk CPI, bezrobocie, przyrost kredytu,
średni nastrój) liczone miesięcznie jako agregat obserwowalny, `DemographyParams` dla M3.
**Kryterium ukończenia:** pogoda deterministyczna i statystycznie zgodna z normami M1
(średnia roczna temperatura ±0,5 °C, suma opadów ±10%); `MacroIndicators` publikowane
i czytelne jako sondy.
**Rozmiar:** M
**Stan:** `[x]` zamknięty. Pogoda jest procesem **z pamięcią** kotwiczonym w normach komórki
klimatu pod środkiem miasta; `tools/headless/tests/events.rs` mierzy zgodność z normą na
przebiegu rocznym. `MacroIndicators` nazywa się `CityIndicators` (`CE-8`) i jest czytane przez
cztery sondy. Razem z pogodą powstały **sieci gazu i ciepła** (`CD-1` wykonane): popyt grzewczy
jest funkcją stopniodni, więc mroźny tydzień podnosi pobór i może zapalić zrzut obciążenia.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.5 Generator zdarzeń (`sim/events`)

**Zasada nienegocjowalna (§11.1):** zdarzenie zmienia **parametry symulacji**.
Zdarzenie nie zna słowa „cena".

```rust
pub struct EventDef {
    pub key: String,                       // stabilny klucz tekstowy (zapis gry trzyma klucz)
    pub category: EventCategory,           // Natural | Infrastructure | External | Social | Firm | Political
    pub scope: EventScope,                 // World | District | Parcel | Site | Firm | Network | RoadEdge
    pub trigger: EventTrigger,
    pub severity: SeveritySpec,            // zakres losowania siły, w bps
    pub duration: DurationSpec,            // Fixed | RangeDays | UntilRepaired | UntilResolved
    pub effects: Vec<PatchSpec>,           // skala efektu ∝ severity
    pub cooldown_days: u32,
    pub max_concurrent: u8,
    pub chronicle: ChronicleTemplate,      // wpis do Kroniki (§14.3)
}

pub struct EventTrigger {
    pub base_hazard_ppm_per_day: u32,      // szansa bazowa: na milion, na dobę, na instancję zakresu
    pub gate: Vec<Precondition>,           // twarde warunki: sezon, epoka, typ terenu, istnienie obiektu
    pub factors: Vec<HazardFactor>,
}

pub struct HazardFactor {
    pub probe: Probe,
    pub curve: Curve,                      // łamana: Vec<(x: i64, mul_bps: i64)>, interpolacja liniowa na i64
}
```

**Ocena hazardu — wyłącznie arytmetyka całkowita:**

```rust
/// hazard = base * Π curve_i(probe_i) / 10000^n, clamp 0..=1_000_000
pub fn hazard_ppm(def: &EventDef, ctx: &ProbeCtx, scope: ScopeInstance) -> u32;

/// Losowanie: bez stanu, bez globalnego RNG, bez f64.
pub fn roll(seed: u64, def_idx: u16, scope_idx: u32, tick: Tick, hazard_ppm: u32) -> bool {
    // rng(seed, StreamId::EventRoll, mix(def_idx, scope_idx), tick.0) % 1_000_000 < hazard_ppm
}
```

**Sondy (`Probe`)** — jedyne wejście generatora do świata, tylko do odczytu, kwantowane do `i64`:

```rust
pub enum Probe {
    // klimat i teren (M1 + WP6)
    RainfallDeficit30d(DistrictId), RainfallDeficit90d(DistrictId), SoilMoistureBps(ParcelId),
    AirTempTenthsC, WindSpeed, SnowCoverMm, SeasonIndex, RiverLevelBps(RiverId),
    // infrastruktura (WP3)
    GeneratorLoadFactorBps(SiteId), MaintenanceBacklogDays(SiteId), EquipmentAgeMonths(SiteId),
    SparePartsCoverDays(SiteId), NetworkReserveMarginBps(UtilityKind), EdgeUtilisationBps(UtilityKind, u32),
    // firma i praca (M7)
    WageGapBps(FirmId), WorkerMoodMean(FirmId), UnionDensityBps(FirmId),
    RecentLayoffs90d(FirmId), SafetyInvestmentBps(SiteId), DeclaredVsExpectedGapBps(FirmId),
    // społeczeństwo i miasto (M3 + WP6 + WP7)
    UnemploymentBps(DistrictId), CrimeIndex(DistrictId), PollutionIndex(DistrictId),
    FoodShareOfIncomeBps(DistrictId), MoodMean(DistrictId), ApprovalBps(DistrictId),
    ServiceQuality(ServiceKind, DistrictId),
    // świat zewnętrzny (WP6 + M6)
    ImportDependencyBps(GoodId), EpochYear, MarketHeatBps, CreditGrowthBps,
}
```

`ProbeCtx` buduje się raz na partię oceny i **cache'uje wartość per (Probe, tick)** w `BTreeMap` —
sonda `UnemploymentBps(d)` używana przez 12 definicji liczy się raz.

**Efekty — `SimParam` i jego twarde ograniczenie:**

```rust
pub struct ParamPatch { pub param: SimParam, pub op: PatchOp, pub value: i64 }
pub enum PatchOp { MulBps, AddAbs, SetBool, Clamp }

pub enum SimParam {
    // wydajność i dostępność
    CropYieldMulBps { crop: GoodId, scope: Scope },
    MachineOutputMulBps { site: SiteId },
    GeneratorOnline { site: SiteId },
    EdgeCapacityMulBps { net: UtilityKind, edge: u32 },
    StorageIntegrityBps { site: SiteId },
    // praca i ludzie
    LaborAvailableBps { site: SiteId },
    AbsenteeismBps { district: DistrictId },
    SkillGrowthMulBps { district: DistrictId, age_band: u8 },
    // ruch i logistyka
    TravelSpeedMulBps { edge: RoadEdgeId },
    EdgeClosed { edge: RoadEdgeId },
    ImportNodeThroughputMulBps { node: ImportNodeId },
    // potrzeby i preferencje (moda, epidemia, festyn)
    NeedWeightMulBps { need: NeedId, scope: Scope },
    BrandTrustDelta { firm: FirmId, delta: i16 },
    // świat zewnętrzny — TU wolno dotknąć ceny, bo to nie jest cena w mieście
    ExternalPriceMulBps { good: GoodId },
    DutyBpsOverride { class: TariffClass },
}
```

> **Niezmiennik wpisany w typ:** `SimParam` nie ma i nie będzie miał wariantu ustawiającego
> cenę oferty w mieście, marżę firmy, wolumen sprzedaży ani popyt w sztukach.
> Wolno zmienić plon, przepustowość, dostępność, wagę potrzeby, prędkość i cenę **importu**.
> Test T7 pilnuje tego mechanicznie.

**Rejestr aktywnych zdarzeń i nakładka parametrów:**

```rust
pub struct WorldEvent {
    pub id: EventId, pub def: u16, pub scope: ScopeInstance,
    pub started_at: Tick, pub ends_at: Option<Tick>,
    pub severity_bps: u32,
    pub cause: EventCause,             // Hazard | Chained{parent: EventId} | Policy | Player
    pub patches: Vec<PatchHandle>,
}

/// Wartość efektywna = wartość bazowa ∘ iloczyn aktywnych patchy.
/// Przeliczane przy ZMIANIE zbioru aktywnych zdarzeń, nie co tick.
/// Kolejność składania: sortowanie po (SimParam, EventId) — nigdy po HashMap.
pub struct ParamOverlay { /* ... */ }
pub fn effective<T: Param>(overlay: &ParamOverlay, key: T::Key, base: T::Value) -> T::Value;
```

**Harmonogram oceny:** zdarzenia wolne (naturalne, społeczne, polityczne, zewnętrzne) —
`EveryDay`. Zdarzenia szybkie (awaria maszyny, awaria sieci, wypadek na drodze) — `EveryHour`.
Zakres `Site`/`Firm` oceniany tylko dla obiektów, które przeszły `gate` — typowo 2–5%
populacji obiektów.

---

#### 5.5.1 Trzy przykłady mechanizmu „prawdopodobieństwo zależne od stanu"

**A. Susza** (`natural/drought`, zakres: `District`, ocena `EveryDay`)

| | |
|---|---|
| **Gate** | `SeasonIndex ∈ {wiosna, lato}`, dzielnica zawiera parcele rolne |
| **Base** | 60 ppm/dobę |
| **Sondy → krzywe** | `RainfallDeficit30d`: 0 mm → ×1,0; 40 mm → ×3,0; 80 mm → ×9,0; 120 mm → ×20,0 · `AirTempTenthsC`: 200 → ×1,0; 300 → ×2,5 · `SoilMoistureBps` (mediana parcel rolnych): 5000 → ×1,0; 1500 → ×4,0 · `RiverLevelBps`: 10000 → ×1,0; 4000 → ×2,0 |
| **Severity** | 2000–9000 bps, rosnąco z deficytem opadów |
| **Zmieniane parametry** | `CropYieldMulBps{crop, district} ×= (10000 − severity·k)` per uprawa (zboża wrażliwsze niż okopowe) · `SoilMoistureBps` w dół · `WaterSourceYieldMulBps` dla ujęć · `FireHazardMulBps{district}` w górę (sprzężenie z pożarem lasu) |
| **Czas trwania** | `UntilResolved` — kończy się, gdy `RainfallDeficit30d < 20 mm` |

**Oczekiwana kaskada (żaden krok nie jest zaprogramowany w M8):** niższy plon → gospodarstwo
zbiera mniej ton (M6, receptury) → młyn nie domyka kontraktu, kupuje na spocie (M6) →
licytacja pozostałego zboża podnosi cenę hurtową → piekarnia widzi wyższy koszt jednostkowy →
polityka marżowa (M7 §6.3) podnosi cenę bułki → mieszkaniec liczy użyteczność (M5 §6.4),
część przechodzi na substytut albo ogranicza zakup → rośnie zapotrzebowanie na import zboża →
węzeł importowy nasyca przepustowość i cena zewnętrzna rośnie (M6 §6.2) → jeśli utrzyma się
kwartał, `FoodShareOfIncomeBps` rośnie → podnosi hazard zdarzenia `social/protest`
i obniża `ApprovalBps` → burmistrz rozważa dotację do żywności albo obniżkę VAT na chleb (WP9).
**M8 nie zapisał ani jednej ceny.**

**B. Awaria elektrowni** (`infrastructure/power_plant_failure`, zakres: `Site`, ocena `EveryHour`)

| | |
|---|---|
| **Gate** | zakład jest źródłem w `UtilityNetwork{Power}` i `online == true` |
| **Base** | 4 ppm/godzinę |
| **Sondy → krzywe** | `EquipmentAgeMonths`: 0 → ×1,0; 240 → ×2,0; 480 → ×5,0 · `MaintenanceBacklogDays`: 0 → ×1,0; 30 → ×2,5; 180 → ×8,0 · `GeneratorLoadFactorBps`: 8000 → ×1,0; 9500 → ×2,0; 10000 → ×4,0 · `AirTempTenthsC`: 250 → ×1,0; 350 → ×1,8 (chłodzenie) · `SafetyInvestmentBps`: wysokie → ×0,6 |
| **Severity** | 3000–10000 bps (awaria jednego bloku vs. całego zakładu) |
| **Zmieniane parametry** | `GeneratorOnline{site} = false` (albo `SourceCapacityMulBps` przy severity < 7000) |
| **Czas trwania** | `UntilRepaired` — **naprawa wymaga części z łańcucha dostaw (M6) i ekipy (M7)**; brak części w mieście = dłuższy postój |

**Oczekiwana kaskada:** źródło znika z bilansu wyspy → `solve_network` w tym samym ticku
stwierdza `demand > supply` → zrzut obciążenia wg priorytetu → dzielnica przemysłowa `Shed` →
`power_available(site) == false` → linie produkcyjne stają (M7), koszty stałe i płace biegną →
niezrealizowane kontrakty → kary umowne (M6) → jeśli zrzut nie wystarczy, sąsiednia linia
przejmuje przepływ i **przekracza przepustowość** → `Tripped` → kolejna runda → rozpad wyspy =
blackout obszarowy. Równolegle: operator kupuje prąd z importu drożej → rośnie koszt →
taryfa w kolejnym okresie w górę (M7, polityka cenowa operatora) → koszt energii rośnie
**wszystkim** firmom → część podnosi ceny → `ApprovalBps` spada → temat wyborczy.
Dodatkowo: `MaintenanceBacklogDays` po awarii rośnie, więc hazard **kolejnej** awarii jest wyższy —
pętla dodatnia, którą przerywa dopiero inwestycja operatora.

**C. Strajk** (`social/strike`, zakres: `Firm`, ocena `EveryDay`)

| | |
|---|---|
| **Gate** | firma ma ≥ 15 pracowników i istnieje od ≥ 180 dni |
| **Base** | 20 ppm/dobę |
| **Sondy → krzywe** | `WageGapBps` (zysk na pracownika vs. mediana płacy w zawodzie): 0 → ×1,0; 3000 → ×3,0; 8000 → ×10,0 · `WorkerMoodMean`: +20 → ×0,5; 0 → ×1,0; −40 → ×4,0 · `UnionDensityBps`: 0 → ×0,2; 5000 → ×1,0; 9000 → ×2,5 · `RecentLayoffs90d`: 0 → ×1,0; 10 → ×2,0 · `FoodShareOfIncomeBps` w dzielnicy (inflacja zjada płacę): ×1,0–×2,2 |
| **Severity** | 3000–10000 bps = odsetek załogi przystępującej do strajku |
| **Zmieniane parametry** | `LaborAvailableBps{site} ×= (10000 − severity)` · `HiringBlocked{firm} = true` · `MoodContagion{district}` w górę (podnosi hazard protestu i strajku u sąsiadów) |
| **Czas trwania** | `UntilResolved` — kończy się, gdy `WageGapBps` spadnie poniżej progu (firma podniosła płace) albo po wyczerpaniu funduszu strajkowego |

**Oczekiwana kaskada:** dostępność pracy spada → produkcja spada proporcjonalnie (M7) →
firma nie domyka dostaw → półki się pustoszą → klienci przechodzą do konkurencji (M5, użyteczność),
część lojalności ginie trwale (M10: marka) → firma liczy koszt przestoju i porównuje z kosztem
podwyżki (M7, decyzja) → jeśli podnosi płace, `WageGapBps` spada i hazard wygasa; jeśli nie,
zaczyna się rotacja i odejścia (M7) → inspekcja pracy dostaje sygnał (WP8, ten sam mechanizm
hazardu) → ewentualna kara → temat medialny (M10) → wpływ na wybory (WP10).

Wspólny wzorzec wszystkich trzech: **stan świata → sonda → krzywa → hazard → losowanie →
zmiana parametru → reakcja innych faz → nowy stan świata → inna sonda.** Pętla jest zamknięta,
a M8 nie zna ani jednej ceny w mieście.

### 5.6 Systemy dynamiczne stałe

```rust
pub struct Weather {                          // EveryHour, StreamId::Weather
    pub temp_tenths_c: i32, pub precip_tenths_mm: u32,
    pub wind: u16, pub cloud_bps: u32, pub snow_cover_mm: u32,
}
```

Pogoda to **odchyłka od norm klimatycznych M1** dla danego dnia roku: proces autoregresyjny
z seedowanym szumem, kotwiczony tak, żeby średnia roczna i suma opadów zgadzały się z normą.
Zastosowania: popyt na ciepło i prąd (→ obciążenie sieci → przeciążenie w mróz), plony,
prędkość ruchu (`TravelSpeedMulBps`, M4), wydajność budownictwa, sondy zdarzeń.

```rust
pub struct SeasonCalendar { /* dzień roku → sezon, kalendarz agrotechniczny, wagi potrzeb */ }
pub struct EpochClock { pub year: u16, pub unlocked: BitSet<TechId> }   // z data/epochs/
pub struct MacroIndicators {                   // EveryMonth, TYLKO agregat obserwowalny
    pub cpi_basket: Money, pub unemployment_bps: u32,
    pub credit_growth_bps: i32, pub mood_mean: Mood, pub heat_bps: u32,
}
pub struct DemographyParams {                  // wyliczane, stosuje je M3
    pub fertility_mul_bps: u32, pub mortality_mul_bps: u32,
    pub immigration_per_month: u32, pub emigration_bps: u32,
}
```

Cykl koniunkturalny **nie jest symulowany** — jest mierzony. `MacroIndicators` to odczyt stanu,
który zdarzenia zewnętrzne wzmacniają przez sondy (`CreditGrowthBps`, `MarketHeatBps`).
Model makro należy do M10.

## Zmiany wpisane po decyzji właściciela produktu (2026-09-17)

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. Wymaganie: **każdy obiekt
w świecie jest klikalny i ma kartę inspekcji z zakładkami, a każda nazwa w karcie jest
odnośnikiem**. Konsekwencja dla tej podfazy jest jedna i mała, ale nie było jej nigdzie.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`WorldEvent` wchodzi do `Subject` jako `Subject::Event(EventId)`** (`M9c` §5.7, `K-62`). Karta zdarzenia: co, gdzie, od kiedy, jak długo jeszcze, kogo dotyka — z odnośnikami do dotkniętych dzielnic, zakładów i mieszkańców. `ChronicleTemplate` w `EventDef` zostaje tym, czym jest: wpisem do kroniki, nie kartą | Przyczyna, dla której gracz pyta „dlaczego" o zdarzenie, jest dokładnie ta sama co przy sklepie: powód jest w `HazardFactor` i `Probe`, tylko nie miał gdzie się pokazać. `WorldEvent` ma już `id: EventId` (§5.x), więc kosztem jest wariant enuma i układ jednej zakładki |
| | **Wpis w kronice niesie `Subject`, nie sam tekst** — kliknięcie w wiersz kroniki otwiera kartę tego, czego wpis dotyczy | `ui-design.md` §4 żąda tego od alertu („kliknięcie otwiera podmiot"), a kronika jest tym samym strumieniem, tylko starszym. Jeśli `ChronicleEntry` powstaje tu bez pola na podmiot, M9 będzie go parsował z tekstu |


## Zmiany wpisane po M8c

Zgodnie z `K-18`. Korekty §5.5 i §5.6 tej podfazy — czyli tego, co projekt techniczny
obiecywał, a czego implementacja nie zrobiła albo zrobiła inaczej.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| `CE-1` ★ | **`Probe` ma 23 warianty, nie trzydzieści z §5.5.** Nie powstały sondy, których dzisiejszy świat nie umie policzyć: `RiverLevelBps` (rzeka ma geometrię, nie stan — przepływ brzegowy wyrzuca generator w `gen/water.rs`), `SoilMoistureBps` (wilgotność gleby jest półproduktem generacji i nie wchodzi do `WorldData`), `UnionDensityBps` (związków nie ma do M10, `K-9`), `SafetyInvestmentBps` (zero wystąpień w repozytorium — nie ma wymiaru bezpieczeństwa pracy), `ServiceQuality` (placówki są w M8d), `CrimeIndex` i `PollutionIndex` (istnieją, ale **nikt ich nie aktualizuje w trakcie gry**: pierwsza jest losowana przy generacji, druga jest polem z worldgenu), `FoodShareOfIncomeBps`, `RecentLayoffs90d`, `ImportDependencyBps` oraz `MarketHeatBps` — ta ostatnia zastąpiona `CityIndicators::heat_bps`, złożoną z dwóch **mierzonych** liczb | `R2` zastosowane do sondy. Wariant zwracający zawsze tę samą liczbę przechodzi każdy test, a krzywa nad nim jest ozdobą — i wygląda w katalogu tak samo jak sonda działająca. `EquipmentAgeMonths` i `MaintenanceBacklogDays` powstały pod nazwami `SourceAgeDays` i `SourceMaintenanceOverdueDays`, bo liczy je linia produkcyjna zakładu prowadzącego źródło, a nie węzeł sieci |
| `CE-2` ★ | **`SimParam` ma sześć wariantów, nie czternaście.** Są: `SiteOutputMulBps`, `SourceOnline`, `SourceCapacityMulBps`, `ExternalPriceMulBps`, `DutyBp`, `NeedDecayMulBps` — i **każdy ma dziś czytelnika widocznego w kodzie** (`sim/events/src/apply.rs`). Nie powstały: `EdgeCapacityMulBps` (przeciążenie krawędzi rozstrzyga solver z bilansu wyspy, więc drugi kanał byłby drugą prawdą o tej samej liczbie), `StorageIntegrityBps` (psucie ma twardą datę ważności, nie tempo), `SkillGrowthMulBps` i `AbsenteeismBps` (adres: M8d, razem ze szkołą i szpitalem), `BrandTrustDelta` (`Offer.brand` jest zawsze `None` do M10), `NeedWeightMulBps` (wagi pilności są stałymi w `planner/tasks.rs`, a nie tabelą — dopóki nie są daną, mnożnik nie ma czego mnożyć; powstał za to `NeedDecayMulBps`, bo **tempo** spadku jest daną), `TravelSpeedMulBps` i `EdgeClosed` (patrz `CE-3`) oraz `ImportNodeThroughputMulBps` (`ExternalPriceMulBps` pokrywa ten sam kanał jednym parametrem zamiast dwoma) | Ta sama reguła co przy sondach i ta sama, którą M8b zastosowała do `EdgeState::UnderMaintenance` (`CC-4`). Parametr, którego nikt nie stosuje, przechodzi test T7 najlepiej ze wszystkich |
| `CE-3` ★ | **`TravelSpeedMulBps` i `EdgeClosed` nie powstają w M8c — ich adresem jest M4 i M8e.** Czas przejazdu powstaje w **jednej** funkcji czystej (`mezo::settle_edge`) objętej kontraktem LOD o tolerancji zero (`K-5`): mnożnik tam wymaga przeliczenia dowodu spójności mikro ↔ mezo z M4d, czyli zmiany w kontrakcie M4, a nie w zakresie M8c. Regulacje ruchowe mają zresztą własny, uzgodniony kształt — `EdgeRestriction` i maskę krawędzi z §6 dokumentu fazy — i tam jest miejsce na oba | Przypadek (5) z `K-18` odwrócony: lepiej nazwać właściciela, zanim ktoś zbuduje drugi mechanizm obok istniejącego. Śnieg zalega w `Weather::snow_cover_mm` i czeka tam na czytelnika |
| `CE-4` | **`ParamOverlay` nie jest czytana — ona zapisuje.** §5.5 zapowiadał `fn effective(overlay, key, base) -> value`, wołane przez konsumentów. Taki kształt jest niewykonalny: konsumentami są `sim/economy`, `sim/supply`, `sim/traffic` i `sim/agents`, a każdy z nich musiałby wtedy zależeć od `sim/events` — przy istniejącym `economy → supply` zamyka to cykl, którego Cargo nie zbuduje. Nakładka **wpisuje** wartość do pola właściciela i pamięta, co tam zastała; wygaśnięcie zdarzenia przywraca zastane | Ten sam problem i to samo rozwiązanie co przy `StrategicOutlooks` (`K-54`): albo struktura idzie do wspólnego przodka, albo wartość idzie w drugą stronę. Zysk uboczny: po stronie M5, M6 i M7 nie ma ani jednego `if zdarzenie` |
| `CE-5` | **`SiteOutputMulBps` jest jednym parametrem na plon suszy, przestój maszyny i strajk.** Nie dlatego, że są tym samym, tylko dlatego, że jądro przepustowości M6 (`kernel::throughput`) ma na nie jedną liczbę: szarża zależy od wsadu i od obsady, a każda z tych trzech rzeczy zabiera zakładowi zdolność wyprodukowania partii. Czym się różnią, mówi powód zdarzenia — a ten gracz widzi w karcie | `DRY` dotyczy wiedzy, nie kształtu. Trzy pola o jednym miejscu zastosowania rozjechałyby się przy pierwszej zmianie |
| `CE-6` ★ | **`EventCause` ma dwa warianty: `Hazard` i `Forced`.** `Chained { parent }` nie powstaje — decyzja `D14` fazy mówi „łańcuchy wyłącznie przez sondę, pośrednio", a skoro tak, to zdarzenie potomne **jest** zwykłym zdarzeniem hazardowym i wariant niósłby informację, której nikt nie ustawia. `Policy` i `Player` dokłada M8e razem z radą miasta i komendami gracza | `R2` zastosowane do wariantu enuma, trzeci raz w tej fazie |
| `CE-7` | **`EpochClock` nie ma `unlocked: BitSet<TechId>`.** Nie dlatego, że to trudne — `TechId` **nie istnieje**: drzewa technologii mieszkają w `data/tech/`, którego właścicielem jest M10. Zegar wystawiałby pusty zbiór i wyglądałby na działający. Zostaje rok gry i lata od startu, oba z czytelnikami | Ta sama reguła, po raz czwarty. Wpisane wprost, żeby M10 nie budował drugiego zegara obok tego |
| `CE-8` | **`MacroIndicators` nazywa się `CityIndicators`.** W `sim/macro` stoją od M7f `MacroState` i `MacroCell`; trzecia nazwa z tym samym rdzeniem, oznaczająca **odczyt**, a nie model, myliłaby się z nimi przy pierwszym czytaniu. Treść bez zmian: CPI, inflacja rok do roku, bezrobocie, przyrost kredytu, nastrój i koniunktura, liczone raz na miesiąc gry | Nazwa jest częścią kontraktu tak samo jak pola. `MacroState` jest modelem, `CityIndicators` termometrem — i to jest cała różnica, którą nazwa ma nieść |
| `CE-9` ★ | **Katalog ma 31 definicji, 4–6 na kategorię, a nie minimum osiem.** Ogranicznikiem nie jest wyobraźnia, tylko liczba **parametrów z czytelnikiem** (`CE-2`): przy sześciu parametrach katalog czterdziestu ośmiu definicji składałby się w połowie z bliźniaków różniących się wyłącznie krzywą. Kryterium „min. 8 na kategorię" zamieniamy na dwa mierzalne, oba w `sim/events/tests/catalog.rs`: **żadna definicja nie jest martwa** (w szczycie hazardu daje ≥ 1 wystąpienie na dwudziestolecie) i **żadna nie zalewa** (przy neutralnych sondach ≤ 60 wystąpień na rok). Katalog rośnie razem z parametrami: M8d dokłada kontrole, M8e politykę rady, M10 związki i media | Liczba definicji była w planie **przybliżeniem** dla „katalog jest kompletny", a przybliżenie przestało mierzyć to, co miało, w chwili, gdy okazało się, że parametrów jest sześć. Ryzyko `R2` mierzy się wprost, a nie licznikiem wierszy |
| `CE-10` | **`DemographyParams` i `NeedModifiers` mieszkają w `sim/agents`, nie w `sim/events`.** Stosuje je M3 (decyzja `D11`: „M8 liczy parametry, M3 je stosuje"), a `sim/agents` nie może zależeć od `sim/events` — zależność idzie w drugą stronę. Struktura wędruje więc do wspólnego przodka, dokładnie tak, jak enum wędruje do `core` (`K-8`). Oba są **neutralne w świecie bez zdarzeń**: mnożniki stoją na 10 000, więc scenariusze M3 i M4 nie zmieniają się o ani jedno losowanie | Ten sam ruch co `CE-4` i z tej samej przyczyny. `DemographyParams` ma cztery mnożniki i **każdy ma mierzone wejście**: dzietność spada przy bezrobociu i drożyźnie, umieralność rośnie w mrozie, napływ idzie za pracą, odpływ za jej brakiem |
| `CE-11` | **System jest jeden (`events.Event`, `EveryHour`, wyłączny, otwiera tick), a nie dwa.** §5.5 dzielił ocenę na „wolną" (`EveryDay`) i „szybką" (`EveryHour`); to jest **pole definicji** (`hourly`), a nie osobny system. Dwa systemy wyłączne o wymuszonej kolejności to `K-53` bez potrzeby — ta sama korekta, którą M8a zrobiła tabeli systemów podatkowych (`CB-2`). System stoi **po** `traffic.Utility`, bo sonda obciążenia sieci ma opisywać ten tick, a nie poprzedni | Przypadek (5) z `K-18`. Pogoda, hazardy i wskaźniki dzielą ten sam zasób, ten sam moment w ticku i tę samą kolejność wobec reszty świata |
| `CE-12` | **Sieci gazu i ciepła powstały razem z popytem (`CD-1` wykonane), ale bez przyłączy przemysłowych.** Odbiorcą jest gospodarstwo domowe, jeden węzeł na dzielnicę, o profilu `LoadProfile::Heating` — czułym na pogodę przez `UtilityNetwork::weather_bps`. Zakład grzeje halę tak samo w lipcu i w styczniu, a jego gaz technologiczny liczy już licznik M6b; osobny węzeł rozbiłby tę samą liczbę na dwie | Ta sama odpowiedź, którą M8b dała sklepom (`CC-17`). `data/tuning/grid.ron` dostaje taryfy gazu i ciepła oraz moc grzewczą na mieszkańca, a `GRID_SCHEMA_VERSION` idzie z 1 na 2 |
| `CE-13` | **Źródło sieci dostaje `owner_site` — zakład, który je prowadzi.** Pole osobne od `site` (odbiorcy z licznikiem) i to rozróżnienie jest konieczne: elektrownia jest jednym i drugim naraz, a dwa wpisy w indeksie przyłączy zepsułyby `power_available`. Bez `owner_site` sondy „blok ma 32 lata" i „90 dób zaległej konserwacji" nie miałyby skąd wziąć liczby, bo źródło w grafie sieci jest węzłem, a nie maszyną | Wykonanie `CD-2`. Most wiąże źródło z zakładem, który **wytwarza** dane medium w recepturze — pierwszym po identyfikatorze, bo miasto z dwiema elektrowniami i tak ma w sieci jedno źródło (`CC-15`) |
| `CE-14` | **`UtilityGrids` wystawia bilans sieci: `load_factor_bps` i `reserve_margin_bps`.** Obie liczby żyły w prywatnych buforach solvera i znikały po kroku, więc sonda „elektrownia chodzi na 98 % mocy" nie miała skąd ich wziąć. `SolveReport` niesie je teraz obok `unserved`; **nie wchodzą do hasha stanu**, bo są przeliczane od zera w każdym kroku z węzłów, które do hasha wchodzą | Trzecia z trzech rzeczy, które M8b wymieniła jako brakujące. Przy okazji `UtilityGrids::unserved` dostał pierwszego czytelnika spoza testów |
| `CE-15` ★ | **Trzeci przebieg testu T3 — „zapis i wczytanie stanu w losowym momencie" — nie powstaje w M8c i jego adresem jest M12b.** `engine/io::save_world` zapisuje tablicę encji i archetypy, czyli **komponenty**; zasobów świata nie zapisuje żaden. Poza zapisem stoją więc nie tylko zdarzenia, ale też `City` (M8a), `Market` (M5), `Firms` (M7) i `UtilityGrids` (M8b) — pełne wersjonowanie schematu i zapis w tle są zakresem M12 od `00` §1. Dwa pierwsze przebiegi T3 (ten sam seed, różna liczba wątków) są w `ci.yml` i obejmują pogodę, rejestr zdarzeń i nakładkę, bo scenariusz `m8miasto` stawia je od doby zero | Przypadek (3) z `K-18`: kryterium jest niemierzalne w tej fazie, bo mechanizm, którego wymaga, nie istnieje — i nie jest to brak M8c ani M8a, tylko granica minimalnego `engine/io` z M0. Zapisane przy zdarzeniach, bo to one pierwsze mają stan, którego utrata byłaby widoczna gołym okiem: świat po wczytaniu nie pamiętałby ani suszy, ani tego, że elektrownia stoi |
