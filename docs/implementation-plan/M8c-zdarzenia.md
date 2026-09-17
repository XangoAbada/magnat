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
| **Kryterium zamknięcia** | Kryteria WP4–WP6; testy T3 i T7 zielone. |
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

### WP5 — Katalog zdarzeń jako dane
**Zależy od:** WP4.
Sześć kategorii z §11.2 w `data/events/{natural,infrastructure,external,social,firm,political}.ron`.
Minimum 8 definicji na kategorię, w tym trzy wzorcowe z sekcji 5.5 (susza, awaria elektrowni, strajk).
Walidator danych w CI: każda sonda istnieje, każdy `SimParam` jest osiągalny, krzywe monotoniczne
tam, gdzie deklarowane, `max_concurrent` i `cooldown` sensowne.
**Kryterium ukończenia:** walidator zielony; przebieg 5 lat gry generuje zdarzenia ze wszystkich
sześciu kategorii, a rozkład liczby zdarzeń na rok mieści się w widełkach z balansatora.
**Rozmiar:** M

### WP6 — Systemy dynamiczne stałe
**Zależy od:** WP4 (sondy), M1 (normy klimatyczne).
Pogoda (proces seedowany, zakotwiczony w normach M1), pory roku i kalendarz rolniczy,
`EpochClock` z `data/epochs/`, `MacroIndicators` (koszyk CPI, bezrobocie, przyrost kredytu,
średni nastrój) liczone miesięcznie jako agregat obserwowalny, `DemographyParams` dla M3.
**Kryterium ukończenia:** pogoda deterministyczna i statystycznie zgodna z normami M1
(średnia roczna temperatura ±0,5 °C, suma opadów ±10%); `MacroIndicators` publikowane
i czytelne jako sondy.
**Rozmiar:** M

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
