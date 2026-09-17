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
    NavigateBack, NavigateForward,           // stos kart inspekcji, M9c §5.7
    SelectCardTab { subject: Subject, tab: u8 },
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

---

## Zmiany wpisane po decyzji właściciela produktu (2026-09-17)

Zgodnie z `K-18`.

| # | Zmiana | Dlaczego |
|---|---|---|
| | **`ViewCommand` dostaje `NavigateBack`, `NavigateForward` i `SelectCardTab`** (§5.5) | Karta inspekcji przechodzi na zakładki i odnośniki (`M9c` §5.7). Skok po odnośniku i powrót są zmianą **widoku**, nie stanu świata — więc idą tym strumieniem, nie `PlayerCommand`, i nie wchodzą do hasha. Wchodzą za to do dziennika widoku, a to jest realna wartość przy zgłoszeniu błędu: „kliknąłem w to, potem w to, potem się wywaliło" jest odtwarzalne |
| | **`Subject` w tych wariantach pochodzi z `engine/core` (`K-62`), nie z `engine/ui`** | `ViewCommand` już miał `SelectEntity(Subject)` i `OpenInspection(Subject)`, więc ta zmiana nic tu nie łamie — przenosi tylko miejsce, w którym typ jest zdefiniowany. Warto to odnotować, bo `game/` importowałby go inaczej |

---

## Zmiany wpisane po M9a

Zgodnie z `K-18`. To są rzeczy, o których wiemy **na pewno** po zamknięciu podfazy.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| DA-1 ★ | **Mosty stawiające świat wyprowadziły się z `tools/headless` do `game/src/world/`** (`population`, `retail`, `plants`, `firms`, `labor`, `city`, `grid`, `events`, `full`). `magnat-headless` reeksportuje je pod starymi nazwami, więc scenariusze, testy integracyjne i balansator nie drgnęły. Nowy kontrakt **`K-68`** w `00` §4a | Kryterium WP1 żąda, żeby `tools/headless` **uruchamiał sesję**, a kryterium WP13 — żeby robił to podpoleceniem. Jedno i drugie znaczy `headless → game`. Gdyby `game` zależał od `headless` (po mosty), Cargo miałby cykl i nie zbudowałby workspace'u. Wybór był więc między przeprowadzką mostów a drugą kopią — a druga kopia rozjeżdża się z pierwszą zawsze (`K-8`, `K-32`) |
| DA-2 ★ | **`PlayerCommand` ma dwa warianty, nie siedemdziesiąt:** `StartGame` i `SetPrice`. Lista z §5.5 zostaje **projektem języka komend**; wariant wchodzi do enuma razem ze swoim wykonawcą. Dopisywać wolno wyłącznie na końcu (jak `TxKind`, `K-66`) | Z ~25 typów ładunku wypisanych w §5.5 **żaden nie istnieje w repozytorium** (`LoanRequest`, `BuildSpec`, `ContractTerms`, `WeekSchedule`, `PriceMode`, …). Zadeklarowanie ich teraz znaczyłoby siedemdziesiąt ramion „to jeszcze nie działa", czyli siedemdziesiąt `TODO` w typie — czego zabrania `K-18` pkt 4 — i siedemdziesiąt wariantów nieodróżnialnych od działających (`K-67`, ryzyko `R2`). Wykonawca dla `SetPrice` istnieje (`Market::set_price` + `set_policy`), więc ten wariant jest prawdziwy: przestawia cenę, zmienia hash stanu i widzi go test |
| DA-3 ★ | **Walidacja dwustopniowa jest jedną funkcją z jednym wołającym.** `precheck` i `apply` biorą `CommandView` (dziś: rynek), a nie `&Snapshot`; drugie wywołanie — po stronie panelu — dokłada `M9b` razem z migawką | Podwójnie buforowany `Snapshot` z §6 („konsumuję: M0") **nie istnieje**: `magnat-sim-snapshot` niesie POD-y renderu (piesi, światła), nie stan dla paneli. `CommandView` jest szwem, w który on wejdzie — funkcja walidująca nie zagląda do `&World`, więc podmiana źródła jej nie dotknie |
| DA-4 | **`GameState` ma cztery warianty, nie siedem:** `Shell`, `Generating`, `WorldReady`, `Playing`. `CharacterSelect` przychodzi z WP4 (`M9c`), `Succession` i `ScenarioEnd` z WP12 (`M9e`) — razem ze swoją treścią | Ten sam powód co `DA-2`: stan, w który nie da się wejść, wygląda tak samo jak działający |
| DA-5 | **`WorldGenJob` chodzi na `std::thread`, nie na `engine/jobs`.** §5.13 zapowiadał `JobHandle` z puli | `engine/jobs` wystawia trzy funkcje i **wszystkie są blokujące** (`scope`, `map_reduce_indexed`, `for_each_chunk_mut`); uchwytu do zadania w tle nie ma. Zagnieżdżenie puli w pulę byłoby gorsze niż brak: generacja **sama** bierze `&JobPool` do zrównoleglenia passów. Ścieżka wyjścia nazwana w kodzie: `JobPool::spawn` zwracające uchwyt, właściciel M0 |
| DA-6 | **Postęp generacji ma 13 kroków: dwanaście passów terenu plus „miasto" jako jeden.** `GenProgress::pass_name` czyta nazwy z `PASSES`, a nie z listy przepisanej w `game/` | `generate_city` nie ma kanału, którym mógłby raportować etapy w trakcie (`stage_millis` powstaje na końcu). Dołożenie go to zmiana w siedmiu miejscach `sim/world` dla paska, który i tak rusza się raz na kilka sekund |
| DA-7 ★ | **Zapisem gry w M9a jest ziarno i wejścia, nie migawka stanu.** Slot to nagłówek (`slot-N.meta.ron`, kilkaset bajtów) plus dziennik (`slot-N.replay.ron`). Wczytanie slotu kosztuje tyle, ile kosztowała rozgrywka | `engine/io::save_world` zapisuje **kolumny komponentów**, a stan gospodarki siedzi w zasobach (`Market`, `Books`, `City`, `Firms`) trzymanych za `Arc<Mutex<…>>` i w ogóle nieserializowalnych; `CityData` nie ma `Serialize`. Migawka wymaga więc pracy, którą M9 §6 adresuje do **M12** („pełne wersjonowanie zapisu"). Dziennik daje tymczasem dokładnie to, co obiecuje §1 pkt 9 dokumentu fazy: replay, który u kogoś innego odtworzy tę samą grę |
| DA-8 | **`NewGameParams` ma czwarte pole `opts: SessionOpts`** (liczba mieszkańców, liczba zamian mieszkań, gospodarka, warstwa Mikro) i jedzie ono w **nagłówku dziennika**, nie w kopercie `StartGame` | `WorldGenParams` nie zawiera liczby mieszkańców — ta wynika z pojemności miasta albo z przełącznika scenariusza. Bez zapisania jej odtworzenie przebiegu testowego dawałoby inny świat, a kryterium WP2 byłoby spełniane przypadkiem. W grze wszystkie cztery wartości są domyślne, więc koperta dalej wystarcza |
| DA-9 | **`sim/world::generate` dostało brata: `generate_observed(params, pool, on_pass)`** — wykonanie `Z-4`. Anulowanie zwraca `Ok(None)`, a nie nowy wariant błędu; `generate` jest odtąd jednolinijkowym opakowaniem | Ośmiu istniejących wołających `generate` nie ma powodu przepisywać, a anulowanie **nie jest błędem parametrów** — wciśnięcie go do `ParamError` byłoby kłamstwem typu |
| DA-10 | Dwie drobne zmiany w cudzych crate'ach: `magnat_ui::Locale` jest `Serialize` (profil gracza to plik RON), a `Market::good_key(GoodId) -> Option<String>` domyka drogę powrotną do `good_of_key` | Komenda niesie towar **kluczem tekstowym, nie indeksem** (00 §5: „w zapisie gry trzymamy klucz"), a panel ma z czego ten klucz wziąć |
| DA-11 | **Klient graficzny nie stawia już świata u siebie.** `tools/magnat` woła `game::world::population::zbuduj_z_params` i `game::Session::begin`; `Citizens` trzyma sesję zamiast własnego `App` i własnego `Market` | To jest wykonanie kryterium „jedna droga do świata" (M9 §7). Do M8e klient składał świat **inaczej niż scenariusz**: bez firm, bez strony publicznej, bez sieci i bez zdarzeń — więc okno pokazywało inne miasto niż to, które mierzył balansator, a różnicy nie widział nikt, bo nikt nie porównywał. Skutek uboczny jest zamierzony: start klienta kosztuje teraz tyle, co pełne miasto M8 |
