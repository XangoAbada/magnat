# M9a — Szkielet gry i komendy

Podfaza 1 z 5 fazy **M9 — Gracz: kariera** (`M9-gracz-kariera.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M0–M8 w całości. |
| **Pakiety robocze** | WP1, WP2 |
| **Projekt techniczny** | §5.1, §5.2, §5.5 |
| **Wynik do pokazania** | Zapis sesji roku gry i jej odtworzenie z łańcuchem hashy zgodnym co 1000 ticków. |
| **Kryterium zamknięcia** | Kryteria WP1 i WP2; `tools/headless` uruchamia sesję bez GPU. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M9b-rdzen-ui.md` |

Crate `game/`: `Session`, `GameState`, kolejność pętli i granica czytania stanu, pełna lista `PlayerCommand` z walidacją dwustopniową, dziennik wejść i odtwarzacz.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP1** | Szkielet `game/` i pętla | M0–M8 | `Session`, `GameState`, kolejność pętli (input → komendy → ticki → swap snapshotu → rebuild UI → render), integracja schedulera sim z pętlą okna | Headless i z oknem: świat z M8 chodzi pod kontrolą `game/`, `tools/headless` uruchamia sesję bez GPU |
| **WP2** | Komendy gracza i replay | WP1 | `PlayerCommand` (pełna lista), `CommandEnvelope`, walidacja dwustopniowa, dziennik wejść, odtwarzacz | Zapis sesji 1 roku gry, odtworzenie, zgodny łańcuch hashy co 1000 ticków |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.1 Struktura `game/`

```
game/
├── app.rs        — pętla główna, kolejność faz klatki
├── session.rs    — Session, GameState, zapis/wczytanie
├── command.rs    — PlayerCommand, CommandEnvelope, walidacja, dziennik replay
├── player.rs     — PlayerCharacter, autonomia, CareerTier, sukcesja, bankructwo
├── scenario.rs   — Scenario, Objective, Goal, ładowanie z data/scenarios/*.ron
├── chronicle.rs  — zapis, indeks, zapytania, osiągnięcia
├── policy/       — ast.rs, text.rs, validate.rs, runner.rs, manager.rs
├── panels/       — dashboard, site, shop, supply, market, people, finance, city, chronicle
├── overlays.rs   — OverlaySpec, EntityFilter, legenda
├── follow.rs     — tryb śledzenia
├── timectl.rs    — TimeScale, StopCondition
├── inspect.rs    — InspectionCard, render DecisionReason
├── metrics.rs    — MetricsRecorder → Series (do wykresów i dry-runu polityk)
└── onboarding.rs — skrypt samouczka, sondy metryk §20.3
```

### 5.2 Pętla gry i granica czytania stanu

`app.rs` ustala kolejność klatki i jest to kontrakt determinizmu:

```rust
fn frame(&mut self, real_dt: Duration) {
    let intents   = self.window.pump_input();               // 1. wejście surowe
    let (cmds, view) = self.ui.dispatch(intents);           // 2. drzewo widgetów → intencje
    self.log.append(&cmds, &view, self.next_tick);          // 3. dziennik replay (przed wykonaniem)
    let ticks = self.timectl.ticks_for(real_dt);            // 4. ile ticków wg TimeScale
    self.sim.run(ticks, &cmds);                             //    komendy w punkcie sync tick.start
    self.snapshots.swap();                                  // 5. podmiana bufora + bump DataVersion
    self.ui.rebuild_dirty(&self.snapshots.front());         // 6. tylko brudne poddrzewa
    self.render.draw(&self.snapshots.front(), self.ui.draw_list());
}
```

**Zasada nienegocjowalna:** kod paneli nie widzi `&World`. Widzi wyłącznie `&Snapshot`
(podwójnie buforowany, spójny na granicy ticku — doc 00 §4). Egzekwowane typem: funkcje
budujące widgety przyjmują `&Snapshot`, a `Snapshot` nie udostępnia mutacji ani surowych
komponentów zapisywalnych. To eliminuje klasę błędów „UI czyta stan w połowie ticku" i
gwarantuje, że to, co gracz widzi, jest tym, na podstawie czego wydaje komendę.

```rust
pub enum GameState {
    MainMenu,
    Loading { progress: u8 },
    CharacterSelect { candidates: Vec<CitizenId>, variant: StartVariant },
    Playing,
    Succession { deceased: CitizenId, heir: Option<CitizenId> },
    ScenarioEnd { result: ScenarioResult },
}
```
Pauza **nie jest** stanem gry — to `TimeScale::Paused`. Jeden mechanizm, nie dwa.

### 5.5 Komendy gracza — wejścia replayu

Podział na dwa strumienie, bo to rozstrzyga determinizm:

- **`PlayerCommand`** — autorytatywne, wpływają na stan symulacji, wchodzą do hasha, są
  odtwarzane przy replayu.
- **`ViewCommand`** — nieautorytatywne (kamera, panele, skala czasu, sortowanie tabeli).
  Logowane w osobnym strumieniu z rzeczywistym znacznikiem czasu (`wall_ms`) — do odtworzenia
  *widoku* w zgłoszeniu błędu i do pomiaru metryk §20.3. Nie wpływają na symulację, więc
  znacznik czasu rzeczywistego ich nie psuje.

Pauza i skala czasu są w `ViewCommand` świadomie: symulacja daje ten sam wynik niezależnie od
tego, kiedy gracz ją zatrzymał — zatrzymanie zmienia tylko to, na którym ticku przestały
napływać komendy, a to jest już zapisane w `tick` każdej koperty.

```rust
pub struct CommandEnvelope {
    pub seq:   u64,       // monotoniczny numer w sesji
    pub tick:  Tick,      // tick, na którego początku komenda jest stosowana
    pub actor: PlayerId,  // 0 dla gry jednoosobowej; miejsce na lockstep (§18.2)
    pub cmd:   PlayerCommand,
}
```

Aplikacja: wszystkie koperty dla ticku T sortowane po `(actor, seq)` i stosowane w punkcie
synchronizacji przed systemami (doc 00 §3.4). Komenda odrzucona **też trafia do dziennika** —
replay musi ją odrzucić identycznie, i to jest osobny test.

```rust
pub enum PlayerCommand {
    // ——— sesja i postać ———
    StartGame          { seed: u64, scenario: ScenarioId, variant: StartVariant, pick: CitizenId },
    SetAutonomy        { field: AutonomyField, control: Control },
    SetHeir            { heir: CitizenId },
    DeclarePersonalBankruptcy,
    ContinueAsHeir     { heir: CitizenId },            // wydawana ze stanu Succession
    ContinueAsNewCitizen { pick: CitizenId },          // brak dziedzica → nowa dynastia

    // ——— życie mieszkańca (§13.2 etap 1) ———
    ApplyForJob        { site: SiteId, role: JobRoleId },
    AcceptJobOffer     { offer: OfferId },
    QuitJob            { notice_days: u8 },
    EnrollCourse       { course: CourseId, budget: Money },
    RentHome           { building: BuildingId },
    BuyHome            { building: BuildingId, mortgage: Option<LoanRequest> },
    SellHome           { building: BuildingId, min_price: Money },
    BuyPersonalVehicle { spec: VehicleSpecId, financing: Option<LoanRequest> },
    SellPersonalVehicle{ vehicle: VehicleId },
    TakePersonalLoan   { bank: FirmId, req: LoanRequest },
    RepayLoan          { debt: DebtId, amount: Money },

    // ——— firma ———
    FoundFirm          { name: String, legal_form: LegalForm, seed_capital: Money },
    RenameFirm         { firm: FirmId, name: String },
    CloseFirm          { firm: FirmId },
    OpenSite           { firm: FirmId, parcel: ParcelId, kind: SiteKind, build: BuildSpec },
    CloseSite          { site: SiteId },
    SellSite           { site: SiteId, min_price: Money },
    SetSiteTags        { site: SiteId, tags: Vec<TagId> },   // grupowanie dla polityk
    /// gracz-postać pracuje osobiście w swoim punkcie — realne godziny (§13.2)
    SetOwnShift        { site: SiteId, role: JobRoleId, schedule: WeekSchedule },

    // ——— sklep ———
    SetShelfAssortment { site: SiteId, good: GoodId, facings: u16 },
    RemoveFromShelf    { site: SiteId, good: GoodId },
    SetPrice           { site: SiteId, good: GoodId, price: Money },
    SetPriceMode       { site: SiteId, good: GoodId, mode: PriceMode },  // Manual | Policy(id)
    MarkdownBatch      { batch: BatchId, pct: Bp },
    ScrapBatch         { batch: BatchId },
    SetOpeningHours    { site: SiteId, schedule: WeekSchedule },

    // ——— zaopatrzenie i produkcja ———
    OrderStock         { site: SiteId, good: GoodId, qty: Qty, source: OrderSource },
    CancelOrder        { order: OrderId },
    SetProductionPlan  { site: SiteId, recipe: RecipeId, qty: Qty, priority: u8, window: TimeWindow },
    CancelProduction   { job: ProductionJobId },
    BuyMachine         { site: SiteId, machine: MachineSpecId, financing: Option<LoanRequest> },
    SellMachine        { machine: MachineId },
    ScheduleMaintenance{ machine: MachineId, window: TimeWindow },
    SetShiftPattern    { site: SiteId, pattern: ShiftPatternId },

    // ——— rynek i kontrakty ———
    PostOffer          { site: SiteId, good: GoodId, qty: Qty, price: Money, valid_until: SimMinute },
    CancelOffer        { offer: OfferId },
    RequestQuotes      { site: SiteId, good: GoodId, qty: Qty, radius_m: u32 },
    AcceptQuote        { offer: OfferId, qty: Qty },
    ProposeContract    { counterparty: FirmId, terms: ContractTerms },
    RespondToContract  { contract: ContractId, accept: bool },
    TerminateContract  { contract: ContractId, penalty_ack: Money },

    // ——— ludzie ———
    PostJobOpening     { site: SiteId, role: JobRoleId, wage: Money, count: u16 },
    ClosePosting       { posting: PostingId },
    HireCandidate      { citizen: CitizenId, site: SiteId, role: JobRoleId, wage: Money },
    FireEmployee       { citizen: CitizenId, severance: Money },
    SetWage            { target: WageTarget, wage: Money },     // osoba | stanowisko@zakład
    AssignManager      { site: SiteId, manager: CitizenId },
    UnassignManager    { site: SiteId },
    SetTrainingBudget  { site: SiteId, monthly: Money },
    SetBonusPolicy     { site: SiteId, policy: PolicyId },

    // ——— logistyka ———
    BuyVehicle         { firm: FirmId, spec: VehicleSpecId, financing: Option<LoanRequest> },
    SellVehicle        { vehicle: VehicleId },
    CreateRoute        { firm: FirmId, from: SiteId, stops: Vec<SiteId>, cadence: Cadence },
    AssignVehicleToRoute { vehicle: VehicleId, route: RouteId },
    DeleteRoute        { route: RouteId },

    // ——— finanse ———
    TakeBusinessLoan   { firm: FirmId, bank: FirmId, req: LoanRequest },
    OpenDeposit        { account: AccountId, amount: Money, term_days: u16 },
    TransferFunds      { from: AccountId, to: AccountId, amount: Money, purpose: TransferPurpose },
    PayDividend        { firm: FirmId, amount: Money },
    InjectCapital      { firm: FirmId, amount: Money },
    PayTaxArrears      { firm: Option<FirmId>, amount: Money },

    // ——— miasto (§8, mechanika w M8) ———
    SubmitPermit       { parcel: ParcelId, use_kind: LandUse },
    BidOnTender        { tender: TenderId, amount: Money },
    SupportCandidate   { candidate: CitizenId, amount: Money, channel: SupportChannel },
    FileComplaint      { subject: ComplaintSubject },

    // ——— automatyzacja (§14.6) ———
    CreatePolicy       { policy: Policy },       // cały AST w komendzie — replay musi mieć reguły
    UpdatePolicy       { id: PolicyId, policy: Policy },
    DeletePolicy       { id: PolicyId },
    AttachPolicy       { id: PolicyId, scope: PolicyScope },
    DetachPolicy       { id: PolicyId, scope: PolicyScope },
    SetPolicyEnabled   { id: PolicyId, enabled: bool },
    /// odpowiedź gracza na eskalację z akcji „wstrzymaj politykę i zapytaj gracza"
    ResolveEscalation  { alert: AlertId, decision: EscalationDecision },
}

pub enum ViewCommand {
    SetTimeScale(TimeScale), SetStopCondition { expr: ConditionExpr, once: bool },
    ClearStopCondition(StopConditionId),
    OpenPanel(PanelId), ClosePanel(PanelId), FocusPanel(PanelId), SetLayout(LayoutId),
    SetOverlay(Option<OverlaySpec>), SetEntityFilter(Option<EntityFilter>),
    SelectEntity(Subject), Follow(FollowTarget), StopFollow,
    SetCamera(CameraPose), SetLocale(Locale), SetUiScale(u16),
    TableSort { table: WidgetId, col: ColumnId, dir: SortDir },
    TableFilter { table: WidgetId, expr: ConditionExpr },
    SaveGame { slot: u8 }, OpenInspection(Subject),
}
```

**Walidacja dwustopniowa.** UI wygasza przycisk i pokazuje powód *zanim* gracz kliknie
(`fn precheck(&Snapshot, &PlayerCommand) -> Result<(), CommandError>`); autorytatywnie to samo
sprawdza `game/` w punkcie synchronizacji. Ta sama funkcja, dwa wywołania — jedno na snapshocie,
drugie na świecie. `CommandError` jest enumem z parametrami (nie stringiem), renderowanym
przez i18n: `InsufficientFunds { have, need }`, `NoPermit { parcel }`, `ParcelOccupied`,
`CandidateRejectedOffer { expected_wage }`, `SiteNotOwned`, `PolicyConflict { with }`.
