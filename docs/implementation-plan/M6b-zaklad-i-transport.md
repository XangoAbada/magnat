# M6b — Zakład i transport

Podfaza 2 z 5 fazy **M6 — Łańcuch dostaw** (`M6-lancuch-dostaw.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M6a (partia, receptury), M4 (`nav`, `traffic`). |
| **Pakiety robocze** | WP4, WP5, WP6 |
| **Projekt techniczny** | §5.5, §5.6, §5.7 |
| **Wynik do pokazania** | Linia bez prądu stoi; partia zmienia lokację wyłącznie przez zlecenie transportowe. |
| **Kryterium zamknięcia** | Kryteria WP4–WP6; `prop_no_teleport` zielony, test spójności LOD produkcji z tolerancją 0. |
| **Poprzednia / następna** | `M6a-katalog-i-partia.md` · `M6c-rynek-b2b.md` |

Zakład jako model fizyczny z jedną funkcją `advance_production` dla wszystkich LOD, zlecenia transportowe z wymaganiami pojazdu, polityki zapasów i kaskada niedoboru.

---

## Pakiety robocze

### WP4 — Zakład jako model fizyczny
**Zależy od:** WP3.
**Opis.** `ProductionLine` (wydajność, wiek, `condition`, MTBF, `LineState`), `ProductionSchedule` (zmiany, plan szarż, przezbrojenia, konserwacja), `UtilityMeter` (prąd/gaz/woda/ciepło z fakturą miesięczną), `Dock` (rampa jako kolejka M/D/c z godzinami dostaw), `Emissions`. Jedna funkcja `advance_production(site, minutes)` używana przez wszystkie trzy poziomy LOD.
**Kryterium ukończenia:** linia bez prądu stoi; awaria wymaga części z magazynu lub zamówienia; przezbrojenie kosztuje czas i masę; test spójności LOD (mikro vs mezo) daje identyczne salda.
**Rozmiar: XL**

### WP5 — Zlecenia transportowe
**Zależy od:** WP2, M4 (`nav`, `traffic`).
**Opis.** `TransportOrder` i jego maszyna stanów, `VehicleRequirements` (nadwozie, chłodnia, cysterna, ADR, tonaż), flota własna vs. przewoźnik wynajęty, załadunek i rozładunek przez rampę, zdarzenia przybycia z M4, obsługa nieudanej dostawy. Rurociąg jako tryb bez pojazdu (przepustowość dobowa + awarie).
**Kryterium ukończenia:** `prop_no_teleport` zielony — partia zmienia lokację wyłącznie przez zlecenie albo ruch wewnątrz tego samego `SiteId`.
**Rozmiar: L**

### WP6 — Polityki zapasów i kaskada niedoboru
**Zależy od:** WP2, WP5.
**Opis.** `InventoryPolicy` (min/max, JIT, sezonowy), przegląd ciągły i okresowy, `ShortageCascade` jako maszyna stanów per `(SiteId, GoodId)` z `DecisionReason` na każdym przejściu.
**Kryterium ukończenia:** scenariusz „młyn stoi 5 dni" przechodzi przez wszystkie stopnie kaskady w udokumentowanej kolejności; każde przejście ma zapisany powód czytelny w karcie inspekcji.
**Rozmiar: M**

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.5 Zakład fizyczny

```rust
pub struct ProductionLine {
    pub id: LineId, pub site: SiteId,
    pub machine_class: MachineClassId,
    pub recipe: Option<RecipeId>,          // aktualnie ustawiona
    pub nominal_throughput: Mass,          // masa wsadu na godzinę przy 100%
    pub age_minutes: u64,
    pub condition: Q,                      // spada z pracą, rośnie z konserwacją
    pub mtbf_hours: u32,                   // bazowe; efektywne = mtbf * condition/100
    pub power_draw: Energy,                // Wh/h przy pracy
    pub spare_part: GoodId,                // część potrzebna do naprawy — wpięcie w łańcuch dostaw
    pub state: LineState,
}

pub enum LineState {
    Idle,
    Setup      { until: SimMinute, to_recipe: RecipeId },
    Running    { recipe: RecipeId, started: SimMinute, ends: SimMinute, inputs: SmallVec<[BatchId;8]> },
    Broken     { since: SimMinute, cause: BreakCause },
    Maintenance{ until: SimMinute },
    Starved    { missing: GoodId },        // brak wejścia
    Blocked    { full: GoodId },           // pełny magazyn wyjściowy
}
pub enum BreakCause { Wear, NoPower, NoWater, NoStaff, Strike }
```

`Starved` i `Blocked` są osobnymi stanami, nie `Idle` — bo to one lądują w alertach pulpitu firmy i w wyjaśnieniu „dlaczego moja fabryka nie produkuje".

```rust
pub struct ProductionSchedule {
    pub shifts: [Option<Shift>; 3],
    pub plan: VecDeque<PlannedRun>,             // recipe, target_mass, earliest_start, priority
    pub maintenance_interval_hours: u32,
    pub next_maintenance: SimMinute,
}
pub struct Shift { pub from: SimMinute, pub to: SimMinute, pub headcount: u16, pub wage_multiplier_pct: u16 }
```

Przezbrojenie odpala się automatycznie, gdy `plan.front().recipe != line.recipe`: `LineState::Setup` na `recipe.setup.minutes` plus odpisanie `recipe.setup.scrap_mass` jako `LossKind::Setup`. Zmiana nocna kosztuje więcej (`wage_multiplier_pct`) — decyzję o jej uruchomieniu podejmuje M7, M6 tylko ją wykonuje.

```rust
pub struct Warehouse { pub site: SiteId, pub role: WarehouseRole, pub slots: Vec<SlotId> }
pub enum WarehouseRole { Input, Output, Distribution, Shelf, Backroom, Tank }

pub struct StorageSlot {
    pub id: SlotId, pub site: SiteId,
    pub class: StorageClass, pub hazard_mask: HazardMask,
    pub cap_mass: Mass, pub cap_volume: Volume,
    pub used_mass: Mass, pub used_volume: Volume,
    pub batches: Vec<BatchId>,             // utrzymywane posortowane po (expires_at, BatchId) — FEFO
}
```

Pojemność slotu wynika z powierzchni budynku (m² z M2) i klasy: `cap_volume = area_m2 * height_m * fill_factor(class)`. Silos i zbiornik mają `cap_volume` wprost z danych budynku.

```rust
pub struct Dock {                          // rampa — realne wąskie gardło (§7.3)
    pub site: SiteId,
    pub bays: u8,                          // stanowisk równolegle
    pub fixed_minutes: u16,                // podjazd, dokumenty
    pub minutes_per_tonne: u16,
    pub hours: OpeningHours,               // godziny dostaw (ograniczenie z §8.3)
    pub yard_capacity: u8,                 // ile pojazdów mieści plac poza stanowiskami
    pub queue: VecDeque<(SimMinute, u32)>, // klucz TOTALNY: (minuta przybycia, vehicle_entity_index)
    pub busy_until: [SimMinute; MAX_BAYS],
}
```

**Kontrakt z M4 — zakład trzyma kolejkę, M4 dowozi i odbiera czas zwolnienia.** M4 odmówił budowania osobnego modelu ruchu dla ciężarówek (drugi model to drugie miejsce, w którym może pęknąć tolerancja 0 mikro↔mezo) i słusznie — kolejka rampy jest moja, przejazd jest jego.

```rust
// M4 -> M6 przy dojeździe pojazdu na teren zakładu
pub struct VehicleArrivedAtSite { pub vehicle: VehicleId, pub site: SiteId,
                                  pub order: TransportOrderId, pub at: SimMinute }
// M6 -> M4 w odpowiedzi; M4 księguje oczekiwanie jako pozycję w rejestrze przejazdu
pub struct SiteDwellResponse { pub release_at: SimMinute, pub idle_fuel: Mass }
```

`release_at` wchodzi do księgi, czyli **dotyka pieniądza**, więc obowiązują cztery warunki brzegowe:

1. **Deterministyczne i niezależne od LOD.** Mikro odgrywa stojącą ciężarówkę wizualnie, ale **nie decyduje, kiedy odjedzie** — decyduje kolejka, identycznie w mikro i w mezo.
2. **Klucz kolejki musi być totalny:** `(minuta_przybycia, vehicle_entity_index)`. Wcześniejszy szkic używał `TransportOrderId`, co jest błędem — jeden pojazd może wieźć kilka zleceń (milk-run, §5.6), więc porządek po zleceniu nie jest funkcją i przy konsolidacji dałby remis rozstrzygany przypadkowo.
3. **Przepełnienie placu wylewa się na ulicę.** Pojazdy ponad `yard_capacity` zajmują pojemność przyuliczną krawędzi dostępowej i **biorą udział w zatorze**. To jedyny punkt, w którym moja kolejka dotyka sieci drogowej, i jest pożądany: zastawiona ulica pod źle zaprojektowaną hurtownią to czytelny sygnał dla gracza, że rampa jest za mała — dokładnie ta emergencja, o którą chodzi w §8.3.
4. **Strażnikiem jest test M4 `ramp_wait_lod_invariant`** (300 ciężarówek, 12 ramp, mikro vs mezo, tolerancja 0 na czas zwolnienia, paliwo postojowe i koszt). Niedeterministyczny model rampy po stronie M6 wywali CI, zanim rozejdzie się po saldach — projektujemy pod to od pierwszej wersji, nie dokładamy determinizmu później.

```rust

pub struct UtilityMeter {
    pub kind: UtilityKind,                 // Power | Gas | Water | Heat
    pub consumed: i64,                     // Wh lub ml, kumulacja od ostatniej faktury
    pub supplier: FirmId,
    pub tariff: Money,                     // za kWh / m³
    pub billed_until: SimMinute,
    pub cut_off: bool,                     // brak płatności / awaria sieci (M8) -> Broken{NoPower}
}
```

Faktura miesięczna trafia do `economy::book`. Emisje są akumulowane per `SiteId` i wystawiane przez `site_emissions()` — M8 je konsumuje, M6 nie liczy ich skutków.

### 5.6 Logistyka

```rust
pub struct TransportOrder {
    pub id: TransportOrderId,
    pub from: SiteId, pub to: SiteId,
    pub cargo: SmallVec<[BatchId; 8]>,
    pub mass: Mass, pub volume: Volume,
    pub requires: VehicleRequirements,
    pub ready_at: SimMinute, pub due_at: SimMinute,     // "kiedy najpóźniej"
    pub carrier: Carrier,
    pub price: Money,
    pub state: TransportOrderState,
    pub reason: DecisionReason,
}

pub struct VehicleRequirements {
    pub body: BodyType,                    // Box | Reefer | Tanker | Tipper | Container | Flatbed
    pub min_payload: Mass, pub min_volume: Volume,
    pub adr: bool,                         // wymaga kierowcy z uprawnieniami
    pub storage_class: StorageClass,       // chłodnia w drodze
}

pub enum Carrier { OwnFleet(FirmId), Hired { firm: FirmId, quote: Money }, Pipeline(PipelineId), Unassigned }

pub enum TransportOrderState {
    Draft, Tendered { closes: SimMinute },
    Assigned { vehicle: VehicleId, pickup_eta: SimMinute },
    LoadingQueue, Loading { until: SimMinute },
    EnRoute { eta: SimMinute },
    UnloadingQueue, Unloading { until: SimMinute },
    Done, Failed(FailReason),
}
pub enum FailReason { NoCarrier, NoVehicle, NoDriver, Expired, Refused, RouteBlocked, Accident }
```

Cykl życia: `ReplenishmentSystem` albo `ContractDeliverySystem` tworzy `Draft` → `TransportDispatchSystem` konsoliduje i przypisuje flotę albo ogłasza przetarg (`Tendered`) → `Assigned` → kolejka rampy nadawcy → `EnRoute` (M4 liczy trasę i zwraca zdarzenie przybycia) → kolejka rampy odbiorcy → `Done`. **Zmiana `Batch::location` wolna jest wyłącznie w obrębie tego cyklu** — tego pilnuje test `prop_no_teleport`.

**Konsolidacja (milk-run).** Zlecenia z tego samego nadawcy, w tym samym oknie czasowym, do celów w promieniu 4 km, mieszczące się w jednym pojeździe, łączone są heurystyką zachłanną Clarke–Wright z twardym limitem 12 punktów na trasę. To wystarcza, żeby centrum dystrybucyjne opłacało się samo z siebie, i nie kosztuje zauważalnego CPU. Optymalizator VRP jest niepotrzebny — a gdyby kiedyś pomiar wykazał inaczej, trasa jest jedną funkcją do podmiany.

### 5.7 Polityki zapasów i kaskada niedoboru

```rust
pub enum InventoryPolicy {
    MinMax    { reorder_point: Mass, target: Mass },
    Jit       { lead_minutes: u32, safety_minutes: u32 },
    Seasonal  { base: MinMaxParams, curve: SeasonCurveId },
}
pub struct InventoryRule { pub good: GoodId, pub policy: InventoryPolicy,
                           pub review: Review, pub preferred: PreferredSource }
pub enum Review { Continuous, Periodic { every_minutes: u32, at_minute: u32 } }
pub enum PreferredSource { Contract(ContractId), Spot, Import, Any }
```

Sklepy używają `Periodic` dobowego (jedna dostawa dziennie jest tańsza niż pięć); zakłady ciągłe — `Continuous`. `Seasonal` istnieje dla żniw i zimy: krzywa mnoży `reorder_point` w oknie roku.

**Kalendarz (K-1, K-15) — wiążący.** Rok ma **360 dni (12 × 30)**, tydzień ma **7 dni i dryfuje względem miesiąca**; `DayOfWeek` pochodzi z `SimCalendar` w `engine/core`, nie jest liczony lokalnie z `SimMinute`. Dotyka to M6 w czterech miejscach i w każdym trzeba to uwzględnić od pierwszej linii kodu, bo późniejsza poprawka to migracja danych:

- **`SeasonCurveId`** — krzywa ma 12 punktów po 30 dni, nie 365 dni. Żniwa wypadają na stałym dniu roku.
- **`expires_at`** — liczone w minutach od `produced_at`, więc kalendarz nie wpływa na arytmetykę, ale wpływa na prezentację daty w UI i na `expires_at / 1440` w kluczu agregacji (§7.4).
- **Okno 30 dni** w `TradeGood::window_reference` to dokładnie jeden miesiąc kalendarzowy — wygodny zbieg, który warto zachować.
- **Dryf tygodnia** jest jedynym powodem, dla którego harmonogram dostaw `DeliverySchedule` musi trzymać okres w minutach, a nie „w dniach miesiąca": dostawa „w każdy wtorek" i dostawa „1. i 15. dnia miesiąca" to dwa różne rytmy, które się rozjeżdżają. Sklepy zamawiają tygodniowo, faktury za media idą miesięcznie — i te dwa cykle mają się nie synchronizować.

**Kaskada niedoboru (§8.4)** — maszyna stanów per `(SiteId, GoodId)`, przeliczana `EveryHour`, sterowana **pokryciem** `coverage = stock / consumption_rate` w godzinach:

```rust
pub enum ShortageStage {
    Ok,
    Buffer      { coverage_min: u32 },        // < 8 h: zużywaj rezerwę, alert w pulpicie
    Throttled   { pct: u8 },                  // < 4 h: produkcja proporcjonalnie obniżona
    SpotSearch  { rfq: RfqId },               // równolegle z Throttled: zapytanie ofertowe, drożej
    Importing   { eta: SimMinute },           // < 2 h i spot bez wyniku: import, wolniej
    Substituted { alt: GoodId, quality_loss: u8 },
    Halted      { since: SimMinute },         // 0: linia Starved, koszty stałe lecą dalej
}
```

Kolejność prób jest dokładnie ta z PRD: bufor → obniżenie produkcji → spot → import → substytut → postój. Substytut jest przed postojem, a nie przed importem, bo psuje jakość produktu i powinien być przedostatnią deską ratunku. Każde przejście zapisuje `DecisionReason::Shortage { good, from, to, coverage_minutes }` — widoczny w karcie inspekcji zakładu (dok. 00 §7).

Po stronie sklepu (M5) pusta półka to `OutOfStock` — mieszkaniec kupuje substytut, idzie do konkurencji albo rezygnuje, i **zapamiętuje** („u nich nie było"). M6 tylko dostarcza sygnał; pamięć jest w M3/M5.
