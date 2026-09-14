# M9a — Szkielet gry i komendy

Podfaza 1 z 5 fazy **M9 — Gracz: kariera** (`M9-gracz-kariera.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M0–M8 w całości. |
| **Pakiety robocze** | WP1, WP2, WP13 |
| **Projekt techniczny** | §5.1, §5.2, §5.5, §5.13 |
| **Wynik do pokazania** | Zapis sesji roku gry i jej odtworzenie z łańcuchem hashy zgodnym co 1000 ticków; nowa gra zakładana z `NewGameParams` bez jednego argumentu wiersza poleceń. |
| **Kryterium zamknięcia** | Kryteria WP1, WP2 i WP13; `tools/headless` uruchamia sesję bez GPU. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M9b-rdzen-ui.md` |

Crate `game/`: `Session`, `GameState`, kolejność pętli i granica czytania stanu, pełna lista `PlayerCommand` z walidacją dwustopniową, dziennik wejść i odtwarzacz, oraz powłoka sesji — zakładanie nowej gry z parametrów, generacja świata z raportowaniem postępu i sloty zapisu.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP1** | Szkielet `game/` i pętla | M0–M8 | `Session`, `GameState`, kolejność pętli (input → komendy → ticki → swap snapshotu → rebuild UI → render), integracja schedulera sim z pętlą okna | Headless i z oknem: świat z M8 chodzi pod kontrolą `game/`, `tools/headless` uruchamia sesję bez GPU |
| **WP2** | Komendy gracza i replay | WP1 | `PlayerCommand` (pełna lista), `CommandEnvelope`, walidacja dwustopniowa, dziennik wejść, odtwarzacz | Zapis sesji 1 roku gry, odtworzenie, zgodny łańcuch hashy co 1000 ticków |
| **WP13** | Powłoka sesji: nowa gra, generacja świata, sloty zapisu | WP1, WP2 | `ShellScreen` w `GameState`, `NewGameParams` (`WorldGenParams` + scenariusz + wariant startu), `WorldGenJob` — generacja w wątku `engine/jobs` z postępem per pass i anulowaniem, `SaveSlot` z metadanymi i listą slotów, `Settings` jako profil gracza poza zapisem świata | `tools/headless new-game --from params.ron` zakłada sesję **bez jednego argumentu opisującego świat**: hash świata identyczny jak z `magnat --seed … --size … --region …` dla tych samych wartości; anulowanie w połowie generacji nie zostawia sesji w stanie połowicznym; `SaveSlot` czytany z pliku starszej wersji zwraca opisany błąd, nie panikę |

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
    Shell(ShellScreen),                                     // wszystko poza rozgrywką — §5.13
    Generating(WorldGenJob),                                // generacja świata w tle, z postępem
    WorldReady { preview: WorldPreview },                   // podgląd: gram tutaj / losuj ponownie
    CharacterSelect { candidates: Vec<CitizenId>, variant: StartVariant },
    Playing,
    Succession { deceased: CitizenId, heir: Option<CitizenId> },
    ScenarioEnd { result: ScenarioResult },
}
```
Pauza **nie jest** stanem gry — to `TimeScale::Paused`. Jeden mechanizm, nie dwa. Menu pauzy jest
za to zwykłym `ShellScreen::Pause`: stan się zmienia, ale **sesja zostaje w pamięci** i zegar stoi,
więc powrót do gry nie wczytuje niczego. Rozróżnienie jest ważne dla replayu — wyjście do menu
głównego kończy sesję i zamyka dziennik, pauza nie robi ani jednego, ani drugiego.

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
    /// Komplet parametrów świata, nie samo ziarno: rozmiar, region, epoka, profil i trudność
    /// zmieniają wygenerowany świat tak samo jak ziarno, więc replay bez nich odtworzyłby
    /// inne miasto (§5.13).
    StartGame          { world: WorldGenParams, scenario: ScenarioId, variant: StartVariant, pick: CitizenId },
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

### 5.13 Powłoka sesji — nowa gra, generacja, sloty (PRD §14.7)

Tu mieszka **logika** ekranów poza rozgrywką; ich postać graficzna to WP14 w `M9b`. Podział
przebiega dokładnie tam, gdzie zwykle: wszystko poniżej da się uruchomić i przetestować bez GPU,
więc `tools/headless` zakłada nową grę tą samą drogą co klient graficzny — a nie drugą, równoległą.

**Po co ta podfaza w ogóle to dostaje.** Dziś jedyną drogą do świata jest `magnat --seed … --size …
--region … --epoch … --profile …` (`tools/magnat::Args`). To jest droga dla nas, nie dla gracza,
i nie jest to kwestia wygody: dopóki parametry żyją wyłącznie w `clap`, **żaden ekran nie ma czego
pokazać**, a zapis gry nie ma czego odtworzyć poza ziarnem.

```rust
pub enum ShellScreen {
    MainMenu,
    NewGame  { draft: NewGameParams },   // kreator; `draft` przeżywa wejście w podgląd i powrót
    Load     { slots: Vec<SaveSlot>, selected: Option<u8> },
    Settings { tab: SettingsTab },
    Pause,                               // sesja żyje, zegar stoi (§5.2)
}

/// Wejście do założenia nowej gry. Serializowalne — `tools/headless` bierze je z pliku RON,
/// klient z kreatora, test z literału. Trzy drogi, jedna struktura.
pub struct NewGameParams {
    pub world:    WorldGenParams,        // sim/world: seed, size, region, epoch, profile, difficulty
    pub scenario: ScenarioId,            // data/scenarios/*.ron; „tryb otwarty" to też scenariusz
    pub variant:  StartVariant,          // §13.1 PRD
}
```

`WorldGenParams` **nie jest kopiowany ani opakowywany** — to ten sam typ, który generator dostaje
dziś z wiersza poleceń i który już jest w zapisie gry (M1 §5.5). Kreator jest edytorem tej
struktury, nic więcej; `params.validate()` jest tą samą funkcją, która broni CLI.

```rust
pub struct WorldGenJob {
    handle:   JobHandle<Result<GeneratedWorld, ParamError>>,
    progress: Arc<GenProgress>,          // czytane przez UI co klatkę, pisane przez wątek generacji
    cancel:   Arc<AtomicBool>,
}

pub struct GenProgress { pub done: AtomicU8, pub total: u8, pub pass: AtomicU8 }  // pass → PASSES[i].name
```

Postęp nie jest zmyślony: `sim/world::pipeline::PASSES` ma nazwy etapów i już dziś mierzy ich czas
do `WorldGenReport`. WP13 dokłada do `generate()` **opcjonalny obserwator** (`&mut dyn FnMut(usize)`
wołany po każdym passie) i sprawdzenie flagi anulowania między passami — kilkanaście linii
w `sim/world`, a nie nowy mechanizm. Ziarnistość „między passami" jest świadoma: pass trwa
ułamek sekundy do ~2 s, a przerywanie w środku erozji wymagałoby wstrzykiwania testów do pętli
liczących i kosztowałoby więcej niż jest warte.

**Kolejność zakładania gry** (jedna funkcja, `game::session::new_game`) jest **dwuetapowa**, i to
nie z elegancji, tylko z pomiarów M1 i M3d:

```
NewGameParams
  └─ etap A: generate() (teren, M1) + generate_city() (miasto, M2)      metropolia ≈ 20 s
       └─ WorldPreview  →  gracz: „gram tutaj" / „losuj ponownie" / „zmień parametry"
            └─ etap B: Etap 8 (populacja, M3)                           metropolia 26,9 s
                 └─ wybór postaci (WP4) → StartGame w ticku 0 → Playing
```

Podgląd stoi **przed** zaludnieniem, bo zaludnienie metropolii kosztuje 26,9 s (M3d §7), a świat
odrzucony po obejrzeniu mapy nie ma powodu być zaludniony. Konsekwencja, którą trzeba przyjąć
świadomie: `WorldPreview` pokazuje **pojemność** miasta z Etapów 6–7 (mieszkania, miejsca pracy,
firmy startowe), a nie faktyczną populację — ta powstaje dopiero w etapie B. Podawanie w podglądzie
„118 400 mieszkańców", zanim ktokolwiek został wygenerowany, byłoby liczbą wziętą z sufitu.

Świat powstaje **przed** sesją, więc dziennik replayu zaczyna się od `StartGame` niosącej komplet
parametrów — odtworzenie nie potrzebuje pliku świata, tylko tej jednej koperty (M1 §6.1).

```rust
pub struct WorldPreview {                // ekran „świat gotowy" — §6.3 docs/ui-design.md
    pub map:        Vec<u8>,             // miniatura 512×512 RGBA, ta sama mapa co w podglądzie headless
    pub homes:      u32, pub jobs: u32, pub firms: u32,   // pojemność z Etapów 6–7, nie populacja
    pub city_area_km2: u32, pub districts: u16,
    pub top_industries: [GoodId; 3],
    pub deposits:   Vec<(DepositKind, Mass)>,
    pub report:     WorldGenReport,      // czasy etapów i hash terenu — do zgłoszeń błędów
}

pub struct SaveSlot {
    pub id: u8, pub city: String, pub game_date: SimMinute, pub net_worth: Money,
    pub played_secs: u32, pub world: WorldGenParams, pub schema_version: u16, pub saved_at_wall: u64,
}
```

`SaveSlot` czyta **wyłącznie nagłówek** pliku zapisu — lista dziesięciu slotów nie ma prawa wczytać
dziesięciu światów. Slot w niezgodnej wersji schematu zostaje na liście z opisanym błędem
(`SaveError::SchemaTooOld { found, supported }`), bo ukryty zapis czyta się jako utracona gra.

**Ustawienia** (`Settings`: język, `ui_scale`, grafika, dźwięk, sterowanie, zapis dziennika widoku)
siedzą w profilu gracza obok układu paneli — **nie w zapisie świata i nie w hashu stanu**. Zmiana
języka i skali UI działa natychmiast; to jest test, nie życzenie (`M9b` WP14).

**Czego tu nie ma:** rysowania (WP14 w `M9b`), ekranu wyboru postaci (WP4 w `M9c` — tam jest
`StartVariant` i kandydaci), ekranów domknięcia scenariusza i spuścizny (WP12 w `M9e`).

---

## Zmiany wpisane po M3d

Zgodnie z `K-18`. Źródło: decyzja właściciela produktu z 2026-09-14 („gra ma generować miasto
z poziomu gry, nie z wiersza poleceń") oraz stan kodu po M3d.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **Nowy pakiet WP13** — powłoka sesji: `ShellScreen`, `NewGameParams`, `WorldGenJob` z postępem i anulowaniem, `SaveSlot`, `Settings` (§5.13). Kryterium: nowa gra bez jednego argumentu CLI, z hashem świata identycznym jak z `magnat --seed …` | PRD §14.7. Żaden pakiet M9 nie był właścicielem drogi „od uruchomienia do grającego świata" — `GameState::MainMenu` istniał jako wariant enuma, którego nikt nie wypełniał. To przypadek (5) z `K-18`: pakiet obiecuje coś, czego nikt nie jest właścicielem |
| Z-2 ★ | **`GameState` przebudowany:** `MainMenu` → `Shell(ShellScreen)`, `Loading { progress: u8 }` → `Generating(WorldGenJob)` + `WorldReady { preview }` | `u8` postępu nie ma jak powiedzieć, **co** się dzieje ani pozwolić anulować, a generacja 16 km to kilkanaście sekund. Podgląd świata przed grą jest osobnym stanem, bo gracz może go odrzucić i wrócić do kreatora |
| Z-3 ★ | **`PlayerCommand::StartGame` niesie `WorldGenParams`, nie `seed: u64`** | Rozmiar, region, epoka, profil i trudność zmieniają świat tak samo jak ziarno. Replay z samym ziarnem odtwarzałby inne miasto — a to jest kontrakt determinizmu z §7, nie szczegół |
| Z-4 | **`sim/world::generate` dostaje obserwatora postępu i flagę anulowania** (wołane między passami). Robi to WP13, bo M1 jest zamknięty, a M9 rozszerza wszystkie `sim/*` | `PASSES` mają nazwy i czasy, ale raportują je dopiero po zakończeniu. Ekran ładowania potrzebuje ich w trakcie — inaczej pokazuje animowany pasek, czyli kłamstwo |
