# M8e — Władza i wybory

Podfaza 5 z 5 fazy **M8 — Miasto jako aktor** (`M8-miasto-jako-aktor.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M8a–M8d w całości. |
| **Pakiety robocze** | WP9, WP10, WP11 |
| **Projekt techniczny** | §5.2, §5.7, §5.8 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: podatki, wybory, sieci przesyłowe, zdarzenia i pogoda. |
| **Kryterium zamknięcia** | Kryteria WP9–WP11, testy T1–T8 zielone oraz bramki 1–7 fazy M8 w `00-postep.md`. |
| **Poprzednia / następna** | `M8d-uslugi-i-prawo.md` · — (ostatnia w fazie) |

Władza miejska AI z celami, histerezą i menu działań, wybory z kadencjami i programami, inspektory, kronika i domknięcie fazy.

---

## Pakiety robocze

### WP9 — Władza miejska AI
**Zależy od:** WP1–WP3, WP7.
Cele (poparcie, saldo budżetu, rozwój), preferencje jako wagi, menu działań z heurystyką
oczekiwanego skutku, pętla miesięczna z histerezą (ochrona przed oscylacją stawek).
Inwestycje (drogi, szkoły, komunikacja), strefowanie, przetargi z kryteriami i rozstrzygnięciem,
regulacje, dotacje z limitem budżetowym.
**Kryterium ukończenia:** test T5 (brak oscylacji): w 20 latach gry bez zdarzeń zewnętrznych
żadna stawka podatkowa nie zmienia kierunku częściej niż raz na 24 miesiące.
Każda decyzja ma `DecisionReason` czytelny w inspektorze.
**Rozmiar:** L

### WP10 — Wybory
**Zależy od:** WP9, WP7 (jakość usług jako wejście), M3 (status, nastrój, dzielnica).
Kalendarz kadencji, kandydaci z programem, frekwencja i głos per mieszkaniec, podział mandatów,
wpływ na politykę. Wsparcie kandydata przez gracza: legalne (darowizna, wykup reklamy)
i nielegalne (łapówka) z ryzykiem ujawnienia jako zdarzenia politycznego.
**Kryterium ukończenia:** test T6 (determinizm wyborów) + test wrażliwości: pogorszenie jakości
usług w jednej dzielnicy o 30% obniża tam poparcie inkumbenta o co najmniej 5 pkt proc.
**Rozmiar:** M

### WP11 — Inspektory, kronika, domknięcie fazy
**Zależy od:** wszystkie.
Inspektor budżetu, inspektor sieci (graf z przepływami i przeciążeniami), log hazardów
(„dlaczego jeszcze nie ma suszy" — wartości sond i wynikowy hazard), wpisy kroniki dla zdarzeń.
Dopisanie komponentów M8 do funkcji hasza stanu ECS. Scenariusz `m8-demo`.
**Kryterium ukończenia:** Definition of Done fazy: wszystkie testy T1–T8 zielone,
`clippy -D warnings`, benchmarki w budżecie, scenariusz demo odtwarzalny z seeda.
**Rozmiar:** M

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.2 Polityka, pozwolenia, przetargi, władza

```rust
pub struct Government {
    pub mayor: CitizenId,
    pub mayor_pref: Preference,
    pub council: Vec<(CitizenId, Preference)>,     // mandaty
    pub term_start: Tick,
    pub term_ticks: u64,
    pub approval_bps_by_district: Vec<u32>,        // aktualizowane miesięcznie
    pub goals: Goals,                              // { approval_w, balance_w, growth_w } — wagi z preferencji
}

pub struct Preference {                            // 4 osie z §10.1, sumują się do 10000 bps
    pub growth_bps: u32, pub social_bps: u32,
    pub green_bps: u32,  pub populist_bps: u32,
}

pub enum Policy {
    TaxRate { kind: TaxKind, bps: u32 },
    Zoning { parcel: ParcelId, use_: ZoneUse },
    TrafficRestriction(EdgeRestriction),          // patrz niżej — JEDEN wariant na wszystkie
                                                  // ograniczenia ruchowe, nie jeden na rodzaj
    EmissionLimit { pollutant: Pollutant, per_day: i64, penalty_per_unit: Money },
    TradingHours { district: DistrictId, hours: WeekMask },   // w tym zakaz handlu w niedziele
    Subsidy { target: SubsidyTarget, per_period: Money, cap: Money, sunset: Tick },
    TariffCap { kind: UtilityKind, max_per_unit: Money },
    MinWage(Money),
    ParkingFee { district: DistrictId, per_hour: Money },
    ServiceFunding { kind: ServiceKind, per_month: Money },
}

/// Siatka tygodniowa (K-15): 7 dni × 24 godziny = 168 bitów.
/// Indeksowane `DayOfWeek` z `SimCalendar` (engine/core) — M8 nie liczy dnia tygodnia sam.
pub struct WeekMask([u64; 3]);
impl WeekMask { pub fn allows(&self, d: DayOfWeek, hour: u8) -> bool; }
```

**Regulacje ruchowe — predykat na krawędziach, nie nowy profil trasy.**
Ograniczenie M4: trasy liczą się na Contraction Hierarchies z ustaloną, małą liczbą profili
(`Passenger`, `HeavyDay`, `HeavyNight`) — każdy profil to osobny zestaw wag, ~450 ms
kustomizacji i do 30 MB. **Liczba profili jest zasobem limitowanym i M8 go nie powiększa.**
Gdyby każda uchwała rady tworzyła własny profil, koszt routingu rósłby liniowo z aktywnością
polityczną miasta — czyli rósłby dokładnie wtedy, gdy rozgrywka robi się ciekawa.

Dlatego **wszystkie** ograniczenia ruchowe — zakaz ruchu ciężkiego w centrum, tonaż na moście,
strefa czystego transportu, okna dostaw w śródmieściu — to jeden typ, wyrażony jako zbiór
wyłączonych krawędzi plus okno czasowe:

```rust
pub struct EdgeRestriction {
    pub edges: EdgeSet,                     // krawędzie grafu M4, nie geometria ani dzielnica
    pub applies_to: VehicleClass,           // Heavy | Delivery | Combustion | Transit | All
    pub window: WeekMask,                   // K-15 — kiedy obowiązuje
    pub exemption: Option<PermitKind>,      // np. OversizeTransport zwalnia z zakazu
    pub fine: Money,                        // egzekucja: mandat, nie bariera fizyczna
}
```

Kompilacja do maski (`EveryHour`, tylko przy zmianie zbioru obowiązujących regulacji):

```rust
/// Wszystkie aktywne regulacje dla danej klasy pojazdu i godziny składają się
/// SUMĄ BITOWĄ w JEDNĄ maskę na istniejący profil. N regulacji = 1 maska,
/// więc koszt jest stały w liczbie uchwał, nie liniowy.
pub fn compile_edge_mask(policies: &PolicySet, class: VehicleClass,
                         day: DayOfWeek, hour: u8) -> EdgeMask;

pub struct PolicyRecord {
    pub policy: Policy,
    pub enacted_at: Tick,
    pub effective_from: Tick,        // zawsze ≥ enacted_at + okres vacatio legis
    pub sunset: Option<Tick>,
    pub vote: CouncilVote,
    pub reason: DecisionReason,      // wyjaśnialność (§7 dokumentu 00)
}

// ---------- pozwolenia: czas jest kosztem ----------

pub struct Permit {
    pub id: PermitId,
    pub applicant: Applicant,
    pub kind: PermitKind,            // Build | ChangeOfUse | Demolition | EnvClearance
                                     // | AlcoholLicense | FoodService | RoadAccess | OversizeTransport
    pub parcel: Option<ParcelId>,
    pub filed_at: Tick,
    pub status: PermitStatus,        // Queued | UnderReview | RequestForInfo{deadline}
                                     // | Approved{at} | Rejected(RejectReason) | Expired
    pub fee: Money,
    pub office: OfficeId,
    pub priority: PermitPriority,    // Standard | Expedited(opłata) | Political(interwencja)
}
```

Czas wydania pozwolenia **nie jest parametrem** — wynika z obsady i kolejki:

```rust
/// EveryDay. throughput = Σ po urzędnikach floor(skill/10) * digitization_mul_bps/10000,
/// pomnożone przez 0 w dni wolne urzędu (K-15: `PermitOffice.open_days: WeekMask`).
/// Kolejka FIFO w obrębie klasy priorytetu; sprawy złożone (EnvClearance) kosztują 5 jednostek.
/// Skutek uboczny zamierzony: wniosek złożony w piątek leży dwa dni — czas gracza to koszt.
fn sys_process_permit_queue(offices: &mut [PermitOffice], permits: &mut PermitRegistry,
                            day: DayOfWeek, tick: Tick);
```

```rust
pub struct Tender {
    pub id: TenderId,
    pub subject: TenderSubject,      // BusLine(LineId) | WasteCollection(DistrictId)
                                     // | Construction(ProjectId) | Maintenance(AssetId)
    pub published_at: Tick,
    pub bid_deadline: Tick,
    pub criteria: BidCriteria,       // wagi bps: cena / jakość / termin / lokalność
    pub bids: Vec<Bid>,
    pub outcome: Option<TenderOutcome>,   // { winner: FirmId, contract: ContractId }
}

pub struct Bid {
    pub bidder: FirmId, pub price: Money, pub quality_promise: Q,
    pub delivery_ticks: u64,
    pub kickback: Option<Money>,     // nielegalne; podnosi hazard kontroli i skandalu
}
```

Pętla decyzyjna burmistrza (`EveryMonth`): ocena celów → wybór działania z menu
z heurystyką oczekiwanego skutku (nie optymalizacja, nie solver) → uchwalenie z terminem
wejścia w życie. **Histereza obowiązkowa:** zmiana stawki wymaga odchylenia od celu
powyżej progu utrzymującego się 3 miesiące — inaczej AI oscyluje.

### 5.7 Wybory

```rust
pub struct Election {
    pub scheduled_at: Tick, pub term_ticks: u64,
    pub candidates: Vec<Candidate>,
    pub result: Option<ElectionResult>,
}

pub struct Candidate {
    pub citizen: CitizenId,
    pub platform: Preference,                       // 4 osie, te same co u burmistrza
    pub tax_stance: Vec<(TaxKind, i32)>,            // deklarowana zmiana w bps
    pub funding: Money,
    pub media_reach: Q,                             // do M10 stub: f(funding, dzielnica)
    pub backers: Vec<(Backer, Money, Legality)>,    // gracz tu wchodzi
}

pub struct ElectionResult {
    pub turnout_bps: u32,
    pub per_district: Vec<DistrictTally>,
    pub council: Vec<(u8 /*idx kandydata*/, u8 /*mandaty*/)>,
    pub mayor: u8,
}

/// EveryDay w dniu wyborów. Iteracja po CitizenId rosnąco — determinizm.
/// turnout   = f(status, wiek, nastrój, staż w dzielnicy, ekspozycja medialna) → próg z RNG
/// preferencja = softmax po użyteczności kandydata: zgodność programu z interesem wyborcy
///               (podatki wobec jego dochodu), ocena usług w JEGO dzielnicy za kadencję,
///               nastrój, ekspozycja medialna, więzi osobiste (graf relacji M3)
fn sys_run_election(e: &mut Election, citizens: &CitizenView, cov: &ServiceCoverage, seed: u64);
```

Wsparcie nielegalne (`Legality::Illegal`) tworzy ukryte powiązanie, które podnosi hazard
zdarzenia `political/scandal` przez sondę — a ujawnienie uderza w kandydata i w gracza
(sprawa w `Agency::Antitrust`/prokuratura, kara, utrata koncesji).

### 5.8 Systemy ECS i ich częstotliwość

| System | Crate | Częstotliwość | Odczyt / zapis |
|---|---|---|---|
| `sys_solve_power_grid` | `traffic` | EveryMinute | R: węzły, popyt / W: `SupplyState`, `EdgeState` |
| `sys_solve_other_grids` | `traffic` | EveryHour | jw. dla gazu, wody, ciepła, telekomu |
| `sys_meter_utilities` | `traffic` | EveryMinute | W: `metered` |
| `sys_assess_vat` | `city` | na transakcji (hook M5) | W: bufor `TaxCharge` |
| `sys_assess_payroll_pit` | `city` | na wypłacie (hook M7) | W: bufor `TaxCharge` |
| `sys_assess_property_tax` | `city` | EveryMonth | R: wycena parcel / W: bufor |
| `sys_assess_excise_duty` | `city` | na zdarzeniu produkcji/importu | W: bufor |
| `sys_assess_cit` | `city` | EveryMonth (domknięcie roku firmy) | R: RZiS / W: bufor |
| `sys_settle_tax_charges` | `city` | EveryDay | W: `CityBudget`, `PaymentCmd` |
| `sys_process_permit_queue` | `city` | EveryDay | W: `Permit.status` |
| `sys_update_service_quality` | `city` | EveryMonth | W: `PublicService.quality` |
| `sys_publish_service_coverage` | `city` | EveryMonth | W: `ServiceCoverage` |
| `sys_agency_inspections` | `city` | EveryDay | W: `Case`, bufor `TaxCharge` |
| `sys_government_decide` | `city` | EveryMonth | W: `PolicyRecord`, `Tender` |
| `sys_run_election` | `city` | EveryDay (gate: dzień wyborów) | W: `Government` |
| `sys_budget_close` | `city` | EveryMonth / EveryMonth (gate: grudzień) | W: `CityBudget` |
| `sys_weather_step` | `events` | EveryHour | W: `Weather` |
| `sys_eval_hazards_slow` | `events` | EveryDay | R: sondy / W: bufor `WorldEvent` |
| `sys_eval_hazards_fast` | `events` | EveryHour | jw. |
| `sys_apply_events` | `events` | EveryHour | W: `ParamOverlay`, Kronika |
| `sys_expire_events` | `events` | EveryHour | W: `ParamOverlay` |
| `sys_macro_indicators` | `events` | EveryMonth | W: `MacroIndicators`, `DemographyParams` |

---
