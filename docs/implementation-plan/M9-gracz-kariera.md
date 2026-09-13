# M9 — Gracz: kariera

Status: plan fazy. Podlega `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, wyjaśnialność).
Źródło: `PRD_Magnat.md` §13, §14, §16.4, §18.2, §19, §20.3.
Crate'y tworzone: `game/`. Crate'y rozszerzane: `engine/ui` (do pełnej postaci), `sim/policy`
(język reguł — właścicielem crate'a jest M7, K-11), wszystkie `sim/*` (hooki gracza).

---

## 1. Cel fazy i artefakt końcowy

Po M8 istnieje żywe miasto, które działa bez gracza. M9 wsadza w nie gracza — nie jako
bezcielesnego „inwestora", lecz jako **jednego z mieszkańców**, i daje mu narzędzia, którymi
da się prowadzić 200 sklepów bez mikrozarządzania.

**Artefakt końcowy:** uruchamialna gra. Konkretnie, w jednej sesji da się:

1. Wybrać (lub wylosować) mieszkańca jako postać — z jego domem, rodziną, pracą, oszczędnościami
   i znajomymi, wygenerowanymi przez M2/M3, nie doklejonymi.
2. Przeżyć dzień jako pracownik: iść do pracy, zrobić zakupy, obejrzeć własną kartę inspekcji
   i kartę inspekcji sąsiada — zrozumieć, jak działa miasto.
3. Otworzyć pierwszy biznes (kiosk / food truck / sklep / warsztat / furgonetka), ustalić ceny
   ręcznie, obsłużyć go osobiście (realne godziny postaci), zobaczyć klientów i — przede wszystkim —
   **tych, którzy nie kupili, i dlaczego**.
4. Urosnąć: zatrudnić ludzi, postawić menedżera, otworzyć drugi punkt, kupić dostawę własną,
   wejść w produkcję, zbudować grupę — bez żadnej blokady poza kapitałem, ludźmi, informacją i czasem.
5. **Zautomatyzować**: napisać politykę cenową w edytorze reguł (bez pisania kodu), przypiąć ją
   do 40 sklepów, zobaczyć w karcie inspekcji, którą regułę menedżer zastosował i jak bardzo ją
   spartaczył, bo ma niską umiejętność.
6. Zbankrutować osobiście — i grać dalej jako pracownik z długiem i popsutą reputacją.
7. Umrzeć — i grać dalej jako dziedzic, który odziedziczył firmy, ale nie odziedziczył znajomości.
8. Obejrzeć kronikę stulecia i wyeksportować replay (seed + wejścia), który u kogoś innego
   odtworzy tę samą grę co do grosza.

**Warunek grywalności fazy** (bez tego faza jest nieukończona): gracz z 200 sklepami nie musi
dotknąć ani jednej ceny ręcznie, a mimo to rozumie, dlaczego jego ceny są takie, jakie są.

**Twarde kryterium onboardingu (§20.3):** nowy gracz podejmuje pierwszą sensowną decyzję
(`SetPrice` / `OpenSite` / `AcceptJobOffer`) w **poniżej 15 minut** czasu rzeczywistego,
w ≤ 12 interakcjach i przy ≤ 3 otwartych panelach.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | PRD | Uwaga |
|---|---|---|
| `game/`: pętla gry, stany, sesja, zapis/wczytanie sesji, integracja sim+engine | §16.2 | crate tworzony w tej fazie |
| Postać gracza jako zwykły mieszkaniec, warianty startu | §13.1 | |
| Ścieżka kariery bez sztucznych blokad, tier jako etykieta | §13.2 | |
| Cele, scenariusze, osiągnięcia emergentne z kronik | §13.3 | |
| Bankructwo osobiste, śmierć postaci, dziedziczenie | §13.4 | |
| `PlayerCommand` — pełny zestaw wejść gracza + dziennik replay | §18.2 | |
| Karta inspekcji w pełnej postaci (renderowanie `DecisionReason`) | §14.1 | szkielet z M3, tu dopełnienie |
| Warstwy widoku, nakładki danych, filtrowanie | §14.2 | wybór i filtr — tu; rysowanie — M1/M2 |
| Komplet paneli biznesowych | §14.3 | 9 paneli |
| Tryb śledzenia mieszkańca / pojazdu / partii | §14.4 | |
| Sterowanie czasem + „zatrzymaj przy zdarzeniu X" | §14.5 | |
| **Język reguł: projektuję** (gramatyka, warunki, akcje, metryki, zakresy) | §14.6, §6.3 | K-11: AST i ewaluator mieszkają w `sim/policy`, którego **właścicielem jest M7** — firmy AI używają tego samego języka. Mój projekt języka jest dla M7 wiążący |
| **Edytor reguł, diagnostyka, dry-run „30 dni", podpowiedzi — mój zakres** | §14.6 | warstwa nad `sim/policy`, w `game/` |
| Wykonanie polityk przez menedżera gracza (`ManagerExecution`, eskalacje) | §14.6, §7 | odwzorowanie umiejętność → jakość wykonania |
| `engine/ui` w pełnej postaci: retained-mode, layout, `Table<T>` 100k, wykresy, grafy, Gantt, mapy cieplne mini, edytor reguł, DPI, i18n PL/EN z pluralizacją | §16.4 | |
| Onboarding i pomiar metryk gracza | §20.3 | |
| Punkt rozszerzenia dla paneli M10 (`PanelRegistry`) | — | tylko rejestr, bez treści |

### Nie wchodzi

| Obszar | Faza | Relacja M9 |
|---|---|---|
| Mechaniki symulacji, które panele wyświetlają: ceny, oferty, produkcja, HR, ruch, podatki | M3–M8 | konsumuję przez snapshot; wymagania na dane spisane w §6 |
| Marka, R&D, giełda, media, przejęcia + ich panele | M10 | rejestruję `PanelId`/`OverlayField`/`Goal` jako zarezerwowane, treści nie piszę |
| Detale graficzne, animacje, wnętrza, audio | M11 | UI rysuje się na własnych listach rysowania, niezależnie |
| Modding UI, panele z modów, pełne wersjonowanie zapisu | M12 | `PanelRegistry` jest projektowany tak, by M12 mógł go zasilić ze skryptu |
| Tryb 50× po stronie symulacji (mezo, makro) | M9 tylko sterowanie | przełącznik tu, implementacja mezo w M4/M9→`sim/macro` |
| Multiplayer lockstep | poza planem | koperta komendy ma pole `actor`, reszta nie |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M9 |
|---|---|
| §13.1 Start | `StartVariant`, wybór mieszkańca, `CharacterSelect` |
| §13.2 Ścieżka | `CareerTier` wyprowadzany, zero bram; `PlayerCommand::FoundFirm`/`OpenSite`/`AssignManager` |
| §13.3 Cele i scenariusze | `Scenario`, `Objective`, `Goal`, osiągnięcia jako zapytania do kroniki |
| §13.4 Porażka | `PersonalBankruptcy`, `Succession`, reputacja i dług przeżywające restart |
| §14.1 Karta inspekcji | `InspectionCard`, render `DecisionReason`, `LostSale` |
| §14.2 Warstwy widoku | `OverlaySpec`, `EntityFilter`, legenda |
| §14.3 Panele | 9 modułów w `game/panels/` |
| §14.4 Śledzenie | `FollowTarget`, oś czasu dnia, ślad partii |
| §14.5 Czas | `TimeScale`, `StopCondition` |
| §14.6 Automatyzacja | projekt języka (`Policy`, `Rule`, `ConditionExpr`, `Action`) → `sim/policy`/M7 (K-11); `RuleEditor`, diagnostyka, dry-run, `ManagerExecution` → `game/` |
| §6.3 Ceny gracza | polityki cenowe — ten sam zestaw narzędzi co AI (K-11); podstawa ceny jawna (K-7) |
| §16.4 UI gry | `engine/ui`: `Widget`, `Layout`, `Table<T>`, wykresy, grafy, Gantt, i18n, DPI |
| §18.2 Determinizm/replay | `CommandEnvelope`, dziennik wejść, test odtworzenia |
| §19 M9 | zakres kamienia milowego |
| §20.3 Metryki gracza | onboarding, pomiar z dziennika replay |

---

## 4. Pakiety robocze

Kolejność wymuszona jednym faktem: **panele są klientem widgetów, a nie odwrotnie**. Widgety
budujemy w takiej kolejności, w jakiej żąda ich pierwszy panel, który ich naprawdę potrzebuje.
Nie budujemy biblioteki widgetów „na zapas".

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP1** | Szkielet `game/` i pętla | M0–M8 | `Session`, `GameState`, kolejność pętli (input → komendy → ticki → swap snapshotu → rebuild UI → render), integracja schedulera sim z pętlą okna | Headless i z oknem: świat z M8 chodzi pod kontrolą `game/`, `tools/headless` uruchamia sesję bez GPU |
| **WP2** | Komendy gracza i replay | WP1 | `PlayerCommand` (pełna lista), `CommandEnvelope`, walidacja dwustopniowa, dziennik wejść, odtwarzacz | Zapis sesji 1 roku gry, odtworzenie, zgodny łańcuch hashy co 1000 ticków |
| **WP3** | Rdzeń `engine/ui` | WP1 | `Widget`, drzewo retained, `measure/arrange/paint/event`, dirty-flagging przez `DataVersion`, `Layout` z dokowaniem, skalowanie DPI, atlas fontów, i18n PL/EN z pluralizacją, listy rysowania | Panel testowy: brak zmian danych → 0 alokacji i 0 ms przebudowy; pseudo-lokalizacja ×1,4 nie rozwala układu |
| **WP4** | Postać gracza i start | WP2 | `PlayerCharacter`, `PlayerAutonomy`, `StartVariant` jako filtr+łatka na populacji, ekran wyboru postaci, przypięcie do LOD Mikro | Da się wybrać mieszkańca, zobaczyć jego rodzinę/pracę/znajomych i przeżyć dzień; 5 wariantów startu działa |
| **WP5** | Karta inspekcji w pełnej postaci | WP3, WP4 | `InspectionCard`, wyczerpujący render `DecisionReason`, `LostSale` (bufor cykliczny + histogram dobowy), głębokie linki między kartami | Odpowiedź na „dlaczego Anna nie kupiła u mnie?" w PL i EN, z nazwanym konkurentem i klikalnym odnośnikiem |
| **WP6** | `Table<T>`, wykresy, mapy cieplne mini | WP3 | Wirtualizowana tabela na 100k wierszy z sortowaniem/filtrem poza klatką, `Series` z piramidą mip (dzień/dekada/miesiąc/kwartał, kalendarz 12 × 30 wg K-1), miniatura mapy cieplnej | Budżety z §7 spełnione w criterion |
| **WP7** | Nakładki danych i filtry | WP3, WP6 | `OverlaySpec`, `EntityFilter`, legenda, przełącznik warstw, filtry „tylko moi klienci / moi pracownicy / cysterny z paliwem" | 9 nakładek z §14.2 działa (bez `BrandAwareness` — zarezerwowana dla M10) |
| **WP8** | Edytor reguł i diagnostyka | WP2, WP6, AST z `sim/policy` (M7, K-11) | Projekt języka jako spec dla M7; serializator tekstowy, walidator (w tym `PriceBasisMismatch`), `RuleEditor` (edycja slotów, zero pisania kodu), dry-run „30 dni" | 6 przykładowych polityk z §5.6 zbudowanych wyłącznie klikaniem; dry-run zgodny z późniejszym wykonaniem; podstawa ceny widoczna w każdym slocie cenowym |
| **WP9** | Wykonanie polityk przez menedżerów | WP8 | `ManagerExecution` z umiejętności, zapis `DecisionReason::PolicyApplied`, eskalacja do gracza; wymagania wydajnościowe na `PolicyRunner` przekazane M7 | 200 sklepów z politykami: < 1 ms/dobę na wątku symulacji, determinizm w dwóch przebiegach |
| **WP10** | Panele biznesowe | WP5–WP9 | 9 paneli z §14.3: pulpit, zakład (Gantt), sklep, łańcuch dostaw (graf), rynek, ludzie, finanse, miasto, kronika + `PanelRegistry` | Każdy panel spełnia budżet klatki; z każdego panelu da się wydać co najmniej jedną `PlayerCommand` |
| **WP11** | Czas, śledzenie, kronika | WP5, WP10 | `TimeScale`, `StopCondition` z gotowym zestawem, tryb „śledź" (mieszkaniec/pojazd/partia), oś czasu dnia plan-vs-realizacja, magazyn kroniki z indeksem i wyszukiwaniem | Śledzenie partii od pola do półki z czasem i kosztem etapów; „zatrzymaj gdy brak towaru" działa przy 10× |
| **WP12** | Kariera, scenariusze, porażka, onboarding | WP4, WP10, WP11 | `CareerTier`, `Scenario`/`Objective`, 5 scenariuszy z §13.3, bankructwo osobiste, sukcesja, samouczek i pomiar metryk §20.3 | Scenariusz „Zbuduj sieć 50 sklepów" przechodzi do końca; bankructwo i śmierć nie kończą sesji; scenariusz samouczka mieści się w budżecie 12 interakcji |

Ścieżka krytyczna: WP1 → WP2 → WP3 → WP6 → WP8 → WP9 → WP10 → WP12.
WP4/WP5/WP7/WP11 są równoległe względem siebie po WP3.

---

## 5. Projekt techniczny

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

### 5.3 Postać gracza

Kluczowa decyzja projektowa: **gracz nie jest osobnym bytem**. Gracz to `CitizenId` — ten sam,
którego obsługują systemy M3 (potrzeby, planer dnia, relacje, pamięć, zdrowie, demografia).
`PlayerCharacter` to cienki komponent sterujący nad zwykłym mieszkańcem.

```rust
pub struct PlayerCharacter {
    pub citizen: CitizenId,              // gracz JEST mieszkańcem
    pub household: HouseholdId,
    pub owned_firms: Vec<FirmId>,
    pub personal_account: AccountId,
    pub reputation: Reputation,          // przeżywa bankructwo (§13.4)
    pub debts: Vec<DebtId>,
    pub heir: Option<CitizenId>,
    pub autonomy: PlayerAutonomy,
    pub start_variant: StartVariant,
    pub dynasty: DynastyId,              // łączy kolejne postacie jednej gry
}

/// Które decyzje mieszkańca przejmuje gracz, a które dalej robi autopilot M3.
pub struct PlayerAutonomy {
    pub job:      Control,   // zmiana pracy
    pub shopping: Control,   // zakupy GD
    pub housing:  Control,   // mieszkanie
    pub vehicle:  Control,   // samochód
    pub leisure:  Control,   // czas wolny — domyślnie Auto, inaczej gra jest nudna
    pub schedule: Control,   // godziny pracy w swoim sklepie (§13.2)
}
pub enum Control { Manual, Auto }
```

Konsekwencja, którą trzeba zapewnić: **dom, rodzina, praca i znajomi przychodzą za darmo**
z generatora M2/M3. Nie tworzymy syntetycznego mieszkańca. Wariant startu to *predykat wyboru
+ łatka*:

```rust
pub enum StartVariant {
    /// absolwent bez kapitału, pożyczka od rodziny
    Graduate       { family_loan: Money },
    /// doświadczony pracownik z oszczędnościami
    ExperiencedWorker { savings: Money },
    /// spadkobierca małej firmy
    Heir           { firm_kind: SiteKind, condition: FirmCondition },
    /// inwestor z zewnątrz: kapitał jest, sieci relacji brak
    OutsideInvestor{ capital: Money, wipe_relations: bool },
    /// sandbox: dowolny kapitał
    Sandbox        { capital: Money },
}

impl StartVariant {
    /// Predykat na wygenerowanej populacji — kto w ogóle może być tą postacią.
    pub fn candidate_filter(&self) -> ConditionExpr;
    /// Łatka na wybranego mieszkańca, stosowana jako komenda w ticku 0.
    pub fn patch(&self, pick: CitizenId) -> Vec<PlayerCommand>;
}
```

`OutsideInvestor { wipe_relations: true }` realizuje „kapitał, brak sieci" przez wyzerowanie
relacji — a nie przez sztuczny modyfikator. Skutek jest emergentny: nikt o graczu nie wie,
więc pierwsza rekrutacja i pierwsi klienci są trudni (§5.7 PRD).

**Przypięcie LOD:** postać gracza i cele trybu „śledź" mają `LodPin` — muszą zostać w LOD Mikro
także przy 10× i 50×. To kontrakt do M3/M4 (§6, „konsumuję").

### 5.4 Kariera bez blokad

```rust
pub enum CareerTier { Employee, FirstBusiness, Company, Group, Magnate }

impl CareerTier {
    /// WYPROWADZANY z obserwowalnych faktów, nigdy nie ustawiany i nigdy nie bramkujący.
    pub fn derive(snap: &Snapshot, p: &PlayerCharacter) -> CareerTier {
        // Employee:      brak firm
        // FirstBusiness: 1 zakład, ≤ 3 zatrudnionych
        // Company:       ≥ 2 zakłady lub ≥ 1 menedżer
        // Group:         ≥ 2 branże lub integracja pionowa (dostawca wewnętrzny)
        // Magnate:       udział > 25% w rynku ≥ 1 towaru lub ≥ 5% zatrudnienia miasta
    }
}
```

Tier służy **wyłącznie** kronice, tytułom i statystykom. Żadna komenda nie sprawdza tieru.
Jedynymi ograniczeniami są kapitał, ludzie, informacja i czas (§13.2). Test akceptacyjny w §7
sprawdza to wprost: w wariancie `Sandbox` gracz może wydać `FoundFirm` w ticku 1.

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

### 5.6 Język reguł (§14.6)

> **Własność (K-11).** Język reguł — gramatyka, warunki, akcje, metryki i zakresy stosowania
> opisane poniżej — jest projektowany w tej fazie i **wiążący dla M7**. Sam AST i ewaluator
> mieszkają w crate'cie **`sim/policy`, którego właścicielem jest M7**, bo firmy AI potrzebują
> tego samego mechanizmu operacyjnie dla tysięcy firm (§6.3 PRD: gracz ma „ten sam zestaw
> narzędzi co AI"). W `game/` zostaje warstwa gracza: **edytor UI, diagnostyka, dry-run
> i podpowiedzi**, plus `ManagerExecution` — jakość wykonania zależna od menedżera.

Reprezentacja kanoniczna to **AST**, nie tekst. Edytor manipuluje AST przez sloty z listami
rozwijanymi — gracz nie pisze i nie może napisać składniowego błędu. Postać tekstowa istnieje
do: wyświetlania („co ta polityka robi" jednym zdaniem), dzielenia się politykami i do `data/`.
Parser tekstu nie jest używany w pętli — wykonuje się AST.

```rust
pub struct Policy {
    pub id: PolicyId,
    pub name: String,
    pub domain: PolicyDomain,           // Pricing | Stock | Hr | Production | Logistics
    pub rules: Vec<Rule>,               // pierwsza pasująca wygrywa; kolejność = priorytet
    pub fallback: Option<Action>,
    pub cadence: Cadence,               // Daily (domyślnie) | Hourly (droższe, dla nietrwałych)
    pub cooldown_h: u8,                 // martwa strefa czasowa — antyoscylacja
}

pub struct Rule {
    pub when:    ConditionExpr,
    pub then:    SmallVec<[Action; 2]>,
    pub enabled: bool,
    pub note:    String,                // notatka gracza, nie wpływa na wykonanie
}

pub enum ConditionExpr {
    And(Box<ConditionExpr>, Box<ConditionExpr>),
    Or(Box<ConditionExpr>, Box<ConditionExpr>),
    Not(Box<ConditionExpr>),
    Cmp { lhs: Expr, op: CmpOp, rhs: Expr },
    Always,
}

pub enum Expr {
    Lit(Value),                          // Money | Qty | Days | Bp | Count | Enum
    Metric(Metric),
    Bin { lhs: Box<Expr>, op: ArithOp, rhs: Box<Expr> },   // + − × ÷ oraz „% z"
}

pub enum Metric {
    // cena i koszt — każda metryka cenowa NIESIE podstawę (doc 00 §4a / K-7)
    Price      { good: GoodId, basis: PriceBasis },
    UnitCost(GoodId), Margin(GoodId),
    CheapestCompetitorPrice { good: GoodId, radius_m: u32, basis: PriceBasis },
    AvgCompetitorPrice      { good: GoodId, radius_m: u32, basis: PriceBasis },
    CompetitorCount         { radius_m: u32 },
    // zapas — okna kroczące (nie kalendarzowe), więc niezależne od podziału na tygodnie
    Stock(GoodId), StockDays(GoodId), Turnover7d(GoodId), Sales7d(GoodId),
    DaysToExpiry(GoodId), ShelfGap(GoodId),
    // produkcja i ludzie
    MachineUtilization, OpenPositions(JobRoleId), StaffTurnover12m,
    MedianMarketWage(JobRoleId), StaffMood, ManagerSkill,
    // finanse i kontekst
    CashBalance, Receivables, Season, DayOfWeek, DayOfMonth, HourOfDay,
    DaysSinceLastChange(GoodId),
}

pub enum Action {
    SetPrice     { good: GoodId, to: Expr },
    AdjustPrice  { good: GoodId, by: Expr },          // by: Bp albo Money
    SetMargin    { good: GoodId, bp: Bp },
    ClampPrice   { good: GoodId, min: Expr, max: Expr },
    OrderUpTo    { good: GoodId, days: Expr },
    OrderQty     { good: GoodId, qty: Expr, source: OrderSource },
    Markdown     { good: GoodId, bp: Bp },
    RemoveFromShelf(GoodId),
    Hire         { role: JobRoleId, count: u16, wage: Expr },
    RaiseWage    { role: JobRoleId, by: Bp, cap: Option<Expr> },
    PlanProduction { recipe: RecipeId, qty: Expr },
    Alert        { key: LocKey, severity: Severity },
    /// eskalacja: polityka się zatrzymuje i pyta gracza (alert w pulpicie firmy)
    AskPlayer    { key: LocKey },
}

pub enum PolicyScope {
    Firm(FirmId),
    Site(SiteId),
    Group(TagId),                                     // np. „sklepy w Śródmieściu"
    Product   { inner: Box<PolicyScope>, good: GoodId },
    Category  { inner: Box<PolicyScope>, cat: CategoryId },
}
```

Rozstrzyganie zakresu — **od najbardziej szczegółowego**:
`Product@Site > Category@Site > Site > Product@Group > Group > Product@Firm > Firm`.
Konflikt (dwa zakresy tej samej szczegółowości) jest błędem walidacji, nie losowaniem.

**Arytmetyka bez floatów.** Wszystkie wartości pieniężne to `Money(i64)` w groszach, procenty to
`Bp(i32)` w punktach bazowych (10000 = 100%). Mnożenie `Money × Bp` przez `i128` i
`div_round_half_up` (doc 00 §2). To jest wymóg, nie preferencja: polityka cenowa wykonana przez
menedżera zmienia stan trwały.

**Podstawa ceny jest jawna (doc 00 §4a, K-7).** `Offer.price_basis` rozróżnia cenę brutto
(detal — to, co faktycznie płaci klient) od netto (hurt). Skutki dla M9:

- Każda metryka cenowa niesie `PriceBasis`. Nie istnieje `cena(X)` bez podstawy.
- **Edytor reguł pokazuje podstawę w slocie, zawsze, nawet gdy jest tylko jedna sensowna** —
  gracz ma widzieć „cena brutto (detal)", a nie domyślać się. Domyślna podstawa wynika z rodzaju
  zakładu w zakresie polityki (`SiteKind::Shop` → brutto, zakład/hurt → netto), ale jest widoczna
  i przestawialna.
- **Walidator odrzuca mieszanie podstaw w jednym porównaniu i w jednej akcji** —
  `PriceBasisMismatch` jest błędem, nie ostrzeżeniem. `ustaw cenę brutto = cena_netto_konkurenta − 2%`
  po prostu nie da się złożyć.
- Konwersja jest dostępna jawnie jako funkcja wyrażenia (`brutto(x)` / `netto(x)` po stawce VAT
  towaru), więc reguła mieszana jest możliwa — ale gracz musi ją napisać świadomie.
- Porównanie z konkurencją w detalu to zawsze **brutto do brutto** — ta sama liczba, którą
  klient widzi na półce i którą zobaczy w karcie inspekcji jako powód `LostToCompetitor`.

```rust
pub enum PriceBasis { Gross, Net }   // z sim/economy: Offer.price_basis
```

#### Gramatyka postaci tekstowej

```ebnf
polityka   ::= "POLITYKA" nazwa "DLA" zakres [ "CO" kadencja ] regula+ [ "INACZEJ" akcja ]
regula     ::= "GDY" warunek "TO" akcja { "ORAZ" akcja }
warunek    ::= warunek "I" warunek | warunek "LUB" warunek | "NIE" warunek
             | "(" warunek ")" | porownanie | "ZAWSZE"
porownanie ::= wyrazenie ("<" | "<=" | "=" | "≠" | ">=" | ">") wyrazenie
wyrazenie  ::= liczba jednostka | metryka | wyrazenie ("+"|"-"|"*"|"/") wyrazenie
             | procent "Z" wyrazenie
jednostka  ::= "zł" | "gr" | "szt" | "dni" | "%" | "godz" | "os"
podstawa   ::= "brutto" | "netto"                      ; obowiązkowa przy metrykach cenowych (K-7)
metryka    ::= "cena" podstawa "(" towar ")" | "koszt" "(" towar ")" | "marża" "(" towar ")"
             | "zapas" "(" towar ")" | "zapas_dni" "(" towar ")"
             | "rotacja_7d" "(" towar ")" | "sprzedaż_7d" "(" towar ")"
             | "dni_do_przydatności" "(" towar ")"
             | "cena_najtańszego_konkurenta" podstawa "(" towar "," promień ")"
             | "cena_średnia_konkurencji" podstawa "(" towar "," promień ")"
             | "brutto" "(" wyrazenie ")" | "netto" "(" wyrazenie ")"   ; jawna konwersja VAT
             | "liczba_konkurentów" "(" promień ")"
             | "obłożenie_maszyn" | "wolne_etaty" "(" stanowisko ")"
             | "rotacja_kadry_12m" | "płaca_mediana" "(" stanowisko ")"
             | "nastrój_załogi" | "umiejętność_menedżera"
             | "saldo" | "należności" | "sezon" | "dzień_tygodnia" | "dzień_miesiąca" | "godzina"
             | "dni_od_zmiany" "(" towar ")"
akcja      ::= "ustaw cenę" wyrazenie
             | "zmień cenę o" wyrazenie
             | "ustaw marżę" procent
             | "ogranicz cenę do" wyrazenie ".." wyrazenie
             | "zamów do" wyrazenie "dni"
             | "zamów" wyrazenie źródło
             | "przeceń o" procent
             | "wycofaj z półki"
             | "zatrudnij" liczba stanowisko "za" wyrazenie
             | "podnieś płacę o" procent [ "do" wyrazenie ]
             | "zaplanuj produkcję" receptura wyrazenie
             | "wyślij alert" tekst
             | "zapytaj gracza" tekst
zakres     ::= "firmy" nazwa | "zakładu" nazwa | "sklepu" nazwa | "grupy" tag
             | "produktu" towar "w" zakres | "kategorii" kategoria "w" zakres
kadencja   ::= "dzień" | "godzinę"
```

Twarde ograniczenia języka (żeby nie wyhodować języka programowania — na to jest M12/modding):
**bez zmiennych, bez pętli, bez funkcji gracza, bez rekurencji.** Maksymalnie 8 reguł na
politykę, głębokość wyrażenia ≤ 3, promień ≤ 10 km, ≤ 2 metryki konkurencyjne na politykę.

#### Sześć przykładowych polityk (realne, z PRD)

```
POLITYKA "Dyskont dzielnicowy" DLA grupy "Sklepy osiedlowe" CO dzień
  GDY liczba_konkurentów(3 km) > 0
    TO ustaw cenę cena_najtańszego_konkurenta brutto(TEN_TOWAR, 3 km) - 2%
       ORAZ ogranicz cenę do brutto(koszt(TEN_TOWAR) * 105%) .. brutto(koszt(TEN_TOWAR) * 160%)
  INACZEJ ustaw marżę 25%
```
(§6.3 wprost: „−2% względem najtańszego konkurenta w promieniu 3 km" + „marża 25%" jako fallback,
z klauzulą chroniącą przed sprzedażą poniżej kosztu — czyli „cena dynamiczna z limitem".
Podstawy jawne: cena detaliczna i cena konkurenta to brutto, koszt jednostkowy to netto, więc
ogranicznik przechodzi przez jawne `brutto(...)` — K-7. Marża liczona jest zawsze na netto.)

```
POLITYKA "Nabiał — nie wyrzucamy" DLA kategorii "Nabiał" w firmy "Delikatesy Anny" CO godzinę
  GDY dni_do_przydatności < 2 dni I zapas_dni > 1 dni  TO przeceń o 40%
  GDY dni_do_przydatności < 1 dni                       TO przeceń o 70%
  GDY dni_do_przydatności = 0 dni                       TO wycofaj z półki ORAZ wyślij alert "straty"
```

```
POLITYKA "Zapas min-max" DLA grupy "Sklepy osiedlowe" CO dzień
  GDY zapas_dni < 3 dni        TO zamów do 10 dni
  GDY zapas_dni > 21 dni I rotacja_7d < 5 szt  TO wyślij alert "martwy zapas"
```

```
POLITYKA "Sezon grzewczy" DLA produktu "Węgiel opałowy" w firmy "Skład Magnat" CO dzień
  GDY sezon = jesień I zapas_dni < 30 dni  TO zamów do 60 dni
  GDY sezon = wiosna I zapas_dni > 20 dni  TO przeceń o 15%
```

```
POLITYKA "Zatrzymać ludzi" DLA firmy "Magnat Logistyka" CO dzień
  GDY rotacja_kadry_12m > 30% I nastrój_załogi < 40
    TO podnieś płacę o 5% do płaca_mediana(kierowca) * 115%
  GDY wolne_etaty(kierowca) > 2 os  TO zatrudnij 2 kierowca za płaca_mediana(kierowca) * 105%
```

```
POLITYKA "Nie przepłacaj za ropę" DLA zakładu "Rafineria Wschód" CO dzień
  GDY zapas_dni(ropa) < 5 dni I cena_średnia_konkurencji(ropa, 10 km) > koszt(ropa) * 130%
    TO zapytaj gracza "Ropa droga, a zapas na 5 dni — kupować czy przystopować produkcję?"
  GDY zapas_dni(ropa) < 5 dni  TO zamów do 20 dni
```

Ostatni przykład pokazuje najważniejszą akcję całego systemu: **`zapytaj gracza`**. Automatyzacja,
której nie da się przerwać, jest gorsza niż jej brak — gracz z 200 sklepami potrzebuje, żeby
polityka sama zgłaszała, kiedy sytuacja wyszła poza jej założenia.

#### Walidacja (`RuleEditor`)

```rust
pub struct RuleEditor { policy: Policy, diagnostics: Vec<Diagnostic>, dry_run: Option<DryRunResult> }
pub enum Diagnostic {
    UnitMismatch    { at: NodePath, expected: Unit, got: Unit },          // błąd
    PriceBasisMismatch { at: NodePath, lhs: PriceBasis, rhs: PriceBasis },// błąd (K-7)
    BelowCost       { good: GoodId },                                     // ostrzeżenie
    PossibleOscillation { rule: usize, metric: Metric },                  // ostrzeżenie
    UnreachableRule { rule: usize, shadowed_by: usize },                  // ostrzeżenie
    ScopeConflict   { with: PolicyId, scope: PolicyScope },               // błąd
    CostBudget      { estimated_us: u32, limit_us: u32 },                 // błąd powyżej limitu
    MissingClamp    { rule: usize },                                      // ostrzeżenie
}
```

1. **Typy jednostek** — każda metryka ma `Unit`; edytor oferuje w slocie wyłącznie wyrażenia
   zgodne jednostkowo, więc `UnitMismatch` może powstać tylko z importu tekstu.
2. **Podstawa ceny (K-7)** — porównanie lub akcja mieszająca brutto z netto to **błąd**.
   Edytor nie pozwala złożyć takiego wyrażenia; komunikat wskazuje jawną konwersję `brutto()`/`netto()`.
3. **Sprzedaż poniżej kosztu** — ostrzeżenie, nie błąd (może być świadomą wojną cenową).
   Porównanie robione po sprowadzeniu obu stron do netto.
4. **Oscylacja** — statycznie: reguła, której akcja zmienia metrykę czytaną w jej warunku
   z przeciwnym znakiem, bez `ClampPrice` i przy `cooldown_h == 0`. Wymuszamy martwą strefę.
5. **Reguła nieosiągalna** — warunek wcześniejszej reguły pokrywa późniejszą (sprawdzenie
   przez podstawienie przedziałów, nie SMT — tanio i wystarczy).
6. **Konflikt zakresów** — dwie polityki tej samej szczegółowości na tym samym celu.
7. **Budżet kosztu** — metryki promieniowe są drogie; edytor pokazuje szacowany koszt jednego
   wykonania i blokuje politykę powyżej limitu.
8. **Dry-run „przetestuj na ostatnich 30 dniach"** — polityka jest odtwarzana na zapisanych
   seriach metryk tego zakładu (te same `Series`, które zasilają wykresy), a wynik pokazany jako
   nakładka na wykresie ceny: „co by ustawiła". Kosztuje niemal nic, bo dane już są, a odpowiada
   na pytanie, którego gracz inaczej nie zada przed przypięciem polityki do 40 sklepów.

#### Wykonanie i menedżer (§14.6, §7 PRD: „jakość zależna od umiejętności")

```rust
pub struct ManagerExecution {
    pub skill:          Q,     // umiejętność „zarządzanie" (M7)
    pub info_lag_days:  u8,    // 7 → 1 wraz ze skill (§6.3: konkurent widziany z opóźnieniem 1–7 dni)
    pub reaction_delay_h: u8,  // 48 → 1
    pub exec_error_bp:  i32,   // ±400 bp → ±0 bp
    pub skip_chance_bp: i32,   // 1500 bp → 0 bp
}
impl ManagerExecution {
    pub fn from_skill(skill: Q) -> Self;   // jedno deterministyczne odwzorowanie, bez floatów
}
```

`PolicyRunner` jako system ECS, częstotliwość `EveryDay` (opcjonalnie `EveryHour` dla polityk
z `Cadence::Hourly`), z **rozłożeniem w dobie**: zakład o indeksie `i` liczony w minucie
`(i * 37) % 1440`. 200 sklepów to średnio 0,14 zakładu na minutę gry — koszt znika w tle.

Jakość wykonania wchodzi w trzech miejscach i wszystkie trzy są deterministyczne
(`rng(world_seed, StreamId::PolicyExecution /* = 260, blok M9: 260–279, K-4 */, manager.index(), tick)`):

- **Wejścia**: metryki konkurencji czytane z opóźnieniem `info_lag_days` — słaby menedżer
  reaguje na wczorajszy świat.
- **Opóźnienie**: reguła zostaje zastosowana `reaction_delay_h` godzin po spełnieniu warunku.
- **Wykonanie**: wartość wynikowa przesunięta o `exec_error_bp`, z szansą `skip_chance_bp`
  na pominięcie cyklu.

Brak menedżera → menedżerem jest gracz: `skill` = umiejętność zarządzania postaci, ale jeśli
postać nie ma zmiany w tym punkcie (`SetOwnShift`), obowiązuje podłoga „bez nadzoru".
To jest mechaniczna motywacja do zatrudnienia menedżera z §7 PRD — nie bramka, tylko koszt.

Każde zastosowanie polityki zapisuje powód:

```rust
DecisionReason::PolicyApplied {
    policy: PolicyId, rule: u8,
    inputs: SmallVec<[(Metric, Value); 4]>,
    target: Value, applied: Value,       // różnica = błąd menedżera
    manager: Option<CitizenId>, lag_days: u8,
}
```

Karta inspekcji półki renderuje to jako: *„Cena 6,38 zł ustawiona 3 dni temu przez politykę
»Dyskont dzielnicowy«, reguła 1: najtańszy konkurent w 3 km = 6,51 zł (dane sprzed 4 dni),
cel 6,38 zł, menedżer Marek Zdun (umiejętność 41) ustawił 6,44 zł."* To jest cała odpowiedź
na „dlaczego moja cena jest taka" — bez tego automatyzacja jest czarną skrzynką.

### 5.7 Karta inspekcji i „dlaczego Anna nie kupiła u mnie?"

```rust
pub struct InspectionCard { pub subject: Subject, pub sections: Vec<CardSection> }
pub enum Subject { Citizen(CitizenId), Household(HouseholdId), Firm(FirmId), Site(SiteId),
                   Building(BuildingId), Parcel(ParcelId), Vehicle(VehicleId), Batch(BatchId),
                   Offer(OfferId), Contract(ContractId), District(DistrictId) }
pub enum CardSection {
    State(Vec<(LocKey, Value)>),
    History(SeriesKey),
    Decisions(Vec<DecisionRecord>),
    Relations(Vec<(LocKey, Subject)>),
    Actions(Vec<PlayerCommandTemplate>),     // z karty da się działać, nie tylko patrzeć
}
pub struct DecisionRecord {
    pub at: SimMinute,
    pub reason: DecisionReason,
    pub alternatives: SmallVec<[Alternative; 3]>,   // co odrzucone i o ile przegrało
}
```

Render: `fn render_reason(&DecisionReason, &Locale) -> Vec<Span>` — **jedno ramię `match` na
wariant**, bez `_ =>`. `DecisionReason` to jeden centralny enum w `engine/core` i **nie może być
`#[non_exhaustive]`** (doc 00, K-12), żeby kompilator łapał
brakujące ramię. To jest egzekucja doc 00 §7: faza, która dodaje wariant, dodaje też render
i klucz lokalizacyjny, inaczej `game/` się nie kompiluje.

#### Pytanie „dlaczego Anna nie kupiła u mnie?" — warianty odpowiedzi i źródła

| Wariant `DecisionReason` | Renderowana odpowiedź | Źródło danych |
|---|---|---|
| `NotInChoiceSet { cause: Unknown }` | „Anna nie zna Twojego sklepu — nie była, nikt jej nie polecił, nie ma go na jej trasie" | M3, §5.7 (zbiór wiedzy mieszkańca) |
| `NotInChoiceSet { cause: TooFar { travel_cost, threshold } }` | „Za daleko: 18 min dojazdu przy jej progu 12 min" | M4 (trasa), M3 (próg osobisty) |
| `NotInChoiceSet { cause: Closed { at } }` | „O 16:40 Twój sklep był zamknięty" | M9 (`SetOpeningHours`) + M5 |
| `NotInChoiceSet { cause: OutOfStock { good } }` | „Brak chleba na półce o 16:40" | M5/M6 (stan półki) |
| `NotInChoiceSet { cause: NoAssortment { good } }` | „Nie masz w asortymencie mleka 3,2%" | M9 (`SetShelfAssortment`) |
| `LostToCompetitor { winner, u_gap_bp, dominant: Price }` | „Wybrała »Dobry Koszyk« — Twoja cena o 12% wyższa (18,90 vs 16,80)" | M5 §6.4 (rozkład użyteczności) |
| `LostToCompetitor { dominant: Distance }` | „»Dobry Koszyk« jest po jej drodze z pracy — Twój to 9 min objazdu" | M4 + M3 (planer dnia) |
| `LostToCompetitor { dominant: Loyalty }` | „Kupuje tam od 3 lat; przewaga lojalności wyceniona na 4,10 zł" | M3 (pamięć, historia zakupów) |
| `LostToCompetitor { dominant: Quality }` | „Ocenia jakość ich pieczywa na 78 vs Twoje 54" | M5/M6 (jakość partii) |
| `LostToCompetitor { dominant: Brand }` | „Zna ich markę, Twojej nie" (rezerwacja) | **M10** — do czasu M10 wariant nieaktywny |
| `LostToCompetitor { dominant: Status }` | „Twój sklep jest poniżej jej statusu — nie robi tam zakupów spożywczych" | M3 (status, osobowość) |
| `AccessBarrier { kind: NoParking }` | „Przyjechałaby autem — nie masz parkingu w promieniu 150 m" | M4 (parkingi), M2 (parcela) |
| `AccessBarrier { kind: Queue { minutes } }` | „Kolejka 11 min przy jednej kasie; jej budżet czasu to 6 min" | M5 (obsługa) + M9 (obsada) |
| `PurchaseDeferred { best_u, threshold }` | „Odłożyła zakup — budżet rozrywki wyczerpany do 20." | M3 (budżet GD), M5 (próg użyteczności) |
| `SubstituteChosen { wanted, taken }` | „Kupiła margarynę zamiast masła — masło przekroczyło jej próg ceny" | M5 §6.4 |
| `SoftmaxDraw { p_chosen_bp, p_yours_bp }` | „Twój sklep miał 31% szans, wylosowała inny — to nie błąd, to rozkład" | M5 (softmax, §6.4) |
| `PolicyApplied { .. }` | „Twoja cena wynika z polityki »X«, reguła 2" (kontekst do powyższych) | M9 |

Ostatni wiersz jest ważny mechanicznie: gracz pytający „dlaczego Anna nie kupiła" i widzący
„cena o 12% wyższa" musi mieć jednym kliknięciem odpowiedź, **skąd wzięła się jego cena** —
inaczej diagnoza nie prowadzi do decyzji.

#### Rejestracja utraconej sprzedaży (`LostSale`)

Karty inspekcji nie da się zbudować, jeśli nikt nie zapisał, że Anna *rozważała* mój sklep.
Zapisywanie tego dla 400 tys. agentów jest nie do przyjęcia kosztowo, więc:

```rust
pub struct LostSale {
    pub at: SimMinute, pub citizen: CitizenId, pub good: GoodId,
    pub reason: DecisionReason, pub competitor: Option<SiteId>, pub u_gap_bp: i32,
}
```

- **Zawsze, dla zakładów gracza**: dobowy histogram powodów (≤ 16 kubełków, 2 słowa kodu na
  kubełek) — to jest wersja *użyteczna operacyjnie*: „Utracone wizyty dziś: 312 — 41% nie zna
  sklepu, 28% cena, 19% za daleko, 12% brak towaru".
- **Bufor cykliczny 256 wpisów** z nazwiskami i szczegółami, wyłącznie dla zakładów oznaczonych
  `observed_by_player` — to jest wersja *narracyjna*, ta, w której pojawia się Anna.
- Dla zakładów AI: nic. Koszt przy graczu z 200 sklepami: 200 × 256 × ~48 B ≈ 2,5 MB. Do przyjęcia.

To jest wymaganie do M5 (`sim/economy`), zgłoszone w §6 i §9.

### 5.8 `engine/ui` — system UI (§16.4)

```rust
pub trait Widget {
    fn id(&self) -> WidgetId;                                  // stabilne: hierarchia + klucz danych
    fn deps(&self) -> &[DataSourceId];                         // co czytam — podstawa dirty-flagging
    fn measure(&mut self, ctx: &mut LayoutCtx, avail: Size) -> Size;
    fn arrange(&mut self, ctx: &mut LayoutCtx, rect: Rect);
    fn paint(&self, out: &mut DrawList);
    fn event(&mut self, ev: &InputEvent, out: &mut Vec<UiIntent>) -> EventFlow;
}
```

**Retained-mode z dirty-flagging.** Drzewo widgetów żyje między klatkami. Każdy widget deklaruje
`deps()`; po podmianie snapshotu `game/` podbija `DataVersion` tylko tych `DataSourceId`, które
faktycznie się zmieniły. Widget, którego zależności i wejście się nie zmieniły, **nie jest
dotykany** — zero pracy, zero alokacji. Deklaratywność (PRD §16.4 „drzewo budowane z danych")
realizujemy przez budowniczych paneli zwracających opis drzewa; różnicowanie jest po
`WidgetId`, więc stan widgetu (pozycja przewijania, zaznaczenie, rozwinięcie) przeżywa przebudowę.

```rust
pub enum LayoutNode {
    Split { axis: Axis, ratio: u16, a: Box<LayoutNode>, b: Box<LayoutNode> },
    Tabs(Vec<PanelId>),
    Panel(PanelId),
    Float { rect: Rect, panel: PanelId },
}
pub struct Layout { pub root: LayoutNode, pub docks: [Option<LayoutNode>; 4], pub name: String }
```
Układ jest preferencją widoku — zapisywany w profilu gracza, nie w zapisie świata (ale
`ViewCommand::SetLayout` idzie do strumienia widoku replayu).

```rust
pub struct Table<T: Row> {
    source:  Arc<dyn RowSource<T>>,   // widok na SoA snapshotu — brak kopiowania danych
    order:   Vec<u32>,                // permutacja po sort+filtr, liczona poza klatką
    order_version: u64,
    columns: Vec<Column<T>>,
    sort:    Option<(ColumnId, SortDir)>,
    filter:  ConditionExpr,           // ten sam AST co reguły
    viewport: Range<u32>,             // wirtualizacja
    selection: IndexSet,
}
```

Trzy razy ten sam AST, i to jest świadome: **warunki reguł, filtry tabel, warunki celów,
warunki „zatrzymaj przy zdarzeniu X" i filtry nakładek to jedna gramatyka**. Gracz uczy się
jednego mechanizmu, a my utrzymujemy jeden walidator i jeden ewaluator.

- Sortowanie 100k wierszy: wyciągnięcie tablicy kluczy `u64` i `pdqsort` w `engine/jobs`,
  poza klatką; tabela pokazuje poprzedni porządek do czasu gotowości. Przewijanie nigdy nie
  sortuje ponownie.
- Filtrowanie: przyrostowe, wynik jako bitset, przeliczane przy zmianie `DataVersion` źródła.
- Wysokość wiersza stała w obrębie tabeli → matematyka przewijania O(1).

```rust
pub struct Series { pub key: SeriesKey, pub mips: [Vec<Sample>; 4] }  // dzień, dekada, miesiąc, kwartał
pub struct Sample { pub min: i64, pub max: i64, pub sum: i64, pub n: u32 }
```
**Kalendarz: 360 dni, 12 × 30 (doc 00, K-1)** — i to jest powód, dla którego poziomy piramidy to
dzień / dekada (10 dni) / miesiąc (30) / kwartał (90), a nie tydzień: przy 12 × 30 każdy poziom
dzieli się bez reszty (30 = 3 × 10, 90 = 3 × 30, 360 = 4 × 90). Agregacja jest wtedy dokładna,
a oś czasu wykresu ma równe podziałki miesięczne bez dryfu — to samo dotyczy skali czasu
w `GanttView` dostaw i osi doby w trybie „śledź".

Wykres 10 lat danych dziennych = 3600 próbek na serię; przy 8 seriach ~29k próbek. Rysujemy
z poziomu mip dobranego do szerokości w pikselach — **nigdy więcej niż ~2000 odcinków**,
niezależnie od zakresu. `MetricsRecorder` (`EveryDay`, w `game/`, nie w `sim/`) zapisuje serie;
10 lat × 8 serii × 16 B ≈ 460 kB na poziomie dziennym (+ ~15% na pozostałe poziomy) — idzie do
zapisu gry, bo historia wykresów musi przeżyć wczytanie (i bo z niej działa dry-run polityk).

Formatowanie dat (`CalendarFmt` w `engine/ui`) zna wyłącznie kalendarz 12 × 30 — nie ma w nim
lat przestępnych ani miesięcy o różnej długości, więc arytmetyka osi czasu jest całkowitoliczbowa.

Pozostałe widgety: `GraphView` (układ warstwowy Sugiyama — łańcuch dostaw to przepływ, więc
warstwy są naturalne; liczony w jobie, cache'owany po hashu topologii), `GanttView` (wiersze =
maszyny/pojazdy, wirtualizacja po oknie czasu), `HeatmapThumb` (tekstura 256×256 z pola
skalarnego nakładki, odświeżana `EveryHour`), `RuleEditorView`.

**DPI:** jedna skala `ui_scale ∈ [0,75; 3,0]`, wszystkie rozmiary w pikselach logicznych,
przyciąganie do siatki pikseli fizycznych przy rysowaniu, font rastrowany per skala do atlasu.

**i18n:** każdy napis to `LocKey` (interned `u32`); katalogi `data/locale/{pl,en}.ron`.
Pluralizacja: PL ma cztery formy (`one` / `few` / `many` / `other`), EN dwie (`one` / `other`) —
wybór formy z reguł CLDR, zaszyty jako funkcja czysta na `(locale, n)`. Formatowanie liczb,
pieniądza i dat per locale (PL: przecinek dziesiętny, spacja jako separator tysięcy, „zł" po
liczbie). Test CI: żadnego literału tekstowego w konstruktorach widgetów.

### 5.9 Panele biznesowe (§14.3)

`PanelRegistry` — punkt rozszerzenia dla M10 (marka, R&D, giełda, media) i M12 (mody):

```rust
pub struct PanelDesc {
    pub id: PanelId, pub title: LocKey, pub deps: &'static [DataSourceId],
    pub build: fn(&Snapshot, &PanelState) -> LayoutNode,
    pub min_tier: Option<CareerTier>,   // TYLKO do domyślnego układu i podpowiedzi, nie do blokowania
}
```
M10 rejestruje `PanelId::Brand`, `PanelId::Rnd`, `PanelId::Stock` bez dotykania kodu M9.
`min_tier` nie blokuje otwarcia panelu — decyduje tylko o tym, czy panel jest domyślnie
przypięty w układzie nowego gracza (onboarding, §5.12).

| Panel | Zawartość | Główne widgety | Konsumuje z | Odświeżanie |
|---|---|---|---|---|
| **Pulpit firmy** | przepływy pieniężne (30 dni / 12 mies.), alerty (niedobór, strajk, awaria, eskalacja polityki), KPI: marża, **obrót netto** (bez VAT, K-7), zatrudnienie, płynność | wykres, lista alertów, kafelki KPI | M5 (księgowość), M7 (HR), M8 (zdarzenia), M9 (eskalacje) | `EveryHour` |
| **Zakład** | produkcja (bieżące zlecenia), magazyn wg partii, załoga i zmiany, maszyny i ich stan, **dostawy jako Gantt**, koszty w rozbiciu | Gantt, `Table<Batch>`, `Table<Employee>`, wykres kosztów | M6 (produkcja, partie), M7 (załoga), M4 (dostawy) | `EveryHour`, Gantt `EveryMinute` przy otwartym |
| **Sklep** | półki (asortyment, ceny, rotacja, dni do przydatności), klienci (skąd, kto, dlaczego), **utracone wizyty z powodami**, konkurencja w zasięgu z ich cenami. **Wszystkie ceny detaliczne pokazywane brutto, porównanie z konkurentem zawsze brutto do brutto (K-7)**; kolumna netto opcjonalna i jawnie podpisana | `Table<ShelfRow>`, histogram powodów, minimapa zasięgu, lista `LostSale` | M5 (transakcje, użyteczność, `Offer.price_basis`), M9 (`LostSale`) | `EveryHour` |
| **Łańcuch dostaw** | graf dostawców i odbiorców z przepływami (grubość = wolumen), kontrakty, **ryzyka: jeden dostawca = czerwony węzeł**, czas i koszt na krawędzi | `GraphView`, `Table<Contract>` | M6 (zlecenia, kontrakty), M4 (czas transportu) | przy zmianie topologii; przepływy `EveryDay` |
| **Rynek** | dla produktu: wszystkie oferty w mieście (tabela), historia cen (wykres), wolumeny, udziały rynkowe. Oferty detaliczne i hurtowe **nigdy nie są mieszane w jednym szeregu** — przełącznik podstawy (brutto/netto) nad wykresem, podstawa w nagłówku kolumny (K-7) | `Table<Offer>` (do 100k), wykres, wykres udziałów | M5/M6 (oferty, `Offer.price_basis`) | `EveryHour` |
| **Ludzie** | pracownicy, kandydaci, menedżerowie; drzewo organizacyjne; umiejętności, płace vs mediana rynku, rotacja | `Table<Person>`, drzewo, wykres płac | M7 (HR, rynek pracy), M3 (mieszkańcy) | `EveryDay` |
| **Finanse** | księgi (RZiS, bilans, przepływy), kredyty i harmonogramy spłat, wycena; **miejsce na giełdę — M10**. **Przychód netto i VAT należny to dwa osobne wiersze, nigdy nie sumowane (K-7)**: VAT jest zobowiązaniem wobec miasta, nie przychodem — pokazywany w bilansie po stronie zobowiązań, z terminem rozliczenia, a nie w RZiS | `Table<LedgerEntry>`, wykresy, harmonogram | M5 (księgowość, banki), M8 (stawki, terminy) | `EveryDay` |
| **Miasto** | polityka i podatki, budżet miasta, przetargi, wybory i sondaże, media | tabele, wykres budżetu, oś wyborcza | M8 (`sim/city`) | `EveryDay` |
| **Kronika** | dziennik świata i gracza w stylu DF Legends, przeszukiwalny, filtry (aktor, typ, ważność, zakres dat) | `Table<ChronicleEntry>` z wyszukiwaniem, oś czasu | M9 + zdarzenia wszystkich faz | na żądanie |

Z każdego panelu da się wydać co najmniej jedną `PlayerCommand` — panel, z którego można tylko
patrzeć, jest raportem, a nie narzędziem, i nie spełnia kryterium ukończenia WP10.

### 5.10 Nakładki, filtry, śledzenie, czas

```rust
pub struct OverlaySpec { pub field: OverlayField, pub filter: Option<EntityFilter>,
                         pub palette: PaletteId, pub range: RangeMode }
pub enum OverlayField {
    LandValue, HouseholdIncome, ShopCatchment { site: SiteId }, Traffic,
    ProductPrice { good: GoodId }, Unemployment, Health, Pollution,
    GoodFlow { good: GoodId },                 // animowane strumienie
    BrandAwareness,                            // zarezerwowane dla M10
}
pub type EntityFilter = ConditionExpr;         // „tylko klienci mojego sklepu" itd.
```
`game/` wybiera i filtruje; rysowanie pól skalarnych i strumieni należy do `engine/render`
(M1/M2). M9 dostarcza `OverlaySpec` + widget legendy + skalę.

```rust
pub enum FollowTarget { Citizen(CitizenId), Vehicle(VehicleId), Batch(BatchId) }
```
Tryb „śledź": kamera podąża, na dole oś czasu doby **plan vs realizacja** (plan z planera dnia
M3, realizacja z faktycznych zdarzeń), panel potrzeb, budżet, ostatnie decyzje z uzasadnieniem.
Śledzenie partii: łańcuch pochodzenia od pola do półki z czasem i kosztem każdego etapu —
wymaga `BatchProvenance` z M6 (§9, decyzja otwarta). Cel śledzenia dostaje `LodPin` na Mikro.

```rust
pub enum TimeScale { Paused, X1, X3, X10, X50 }
pub struct StopCondition { pub id: StopConditionId, pub expr: ConditionExpr, pub once: bool }
```
Gotowy zestaw warunków („zatrzymaj przy zdarzeniu X" bez budowania wyrażenia):
brak towaru w dowolnym moim sklepie · saldo poniżej progu · nieudana dostawa · strajk ·
awaria maszyny · konkurent otworzył punkt w promieniu R · kontrakt wygasa za N dni ·
wojna cenowa na towarze X · cel scenariusza osiągnięty · eskalacja z polityki ·
zdarzenie w mieście (podatki, przetarg, wybory).

Ewaluacja `EveryHour` po stronie `game/` na snapshocie; trafienie ustawia `TimeScale::Paused`
na granicy klatki, podnosi alert i podświetla źródło w kronice. Ponieważ to strona widoku,
pauzowanie z definicji nie może zmienić wyniku symulacji.

Przy `X50` (tryb makro, ruch w mezo): panele przechodzą na odświeżanie `EveryHour`, `GanttView`
i tryb śledzenia są wyłączone (albo wymuszają Mikro na jednej encji, co jest kosztem zgłoszonym
graczowi). Bez tego UI staje się wąskim gardłem trybu 50×.

### 5.11 Scenariusze, cele, kronika, porażka

```rust
pub struct Scenario {
    pub id: ScenarioId, pub title: LocKey, pub brief: LocKey,
    pub world_seed: u64, pub world_preset: WorldPresetId, pub start_epoch: EpochId,
    pub start: StartVariant,
    pub patches: Vec<WorldPatch>,           // „upadająca huta" = konkretny zakład z długiem
    pub objectives: Vec<Objective>,
    pub fail_conditions: Vec<ConditionExpr>,
    pub time_limit: Option<SimMinute>,
    pub tutorial: Option<TutorialScriptId>,
}
pub struct Objective {
    pub id: ObjectiveId, pub title: LocKey,
    pub goal: Goal, pub optional: bool,
    pub deadline: Option<SimMinute>, pub reveal_after: Option<ObjectiveId>,
}
pub enum Goal {
    MarketShare  { good: GoodId, area: AreaSpec, min_bp: u16 },   // „Zmonopolizuj paliwo w 10 lat"
    SiteCount    { kind: SiteKind, min: u32 },                    // „Zbuduj sieć 50 sklepów"
    Employment   { min_headcount: u32, rank: Option<u8> },        // „Zostań największym pracodawcą"
    FirmSolvent  { firm: FirmId, for_days: u32 },                 // „Uratuj upadającą hutę"
    ProductLaunched { recipe: RecipeId, units_sold: Qty },        // „własna marka samochodów" (M10)
    NetWorth     { min: Money },
    Custom(ConditionExpr),                                        // ta sama gramatyka co reguły
}
```
Ewaluacja celów: `EveryDay`, nigdy `EveryMinute`. Postęp `0..=10000 bp` pokazywany w panelu celów.
Scenariusze w `data/scenarios/*.ron` — moddowalne bez kodu od pierwszego dnia.

```rust
pub struct ChronicleEntry {
    pub id: ChronicleId, pub at: SimMinute,
    pub kind: ChronicleKind,
    pub actors: SmallVec<[ChronicleActor; 4]>,
    pub payload: ChroniclePayload,          // typowane dane, NIGDY gotowy string
    pub importance: u8,                     // 0..=100
    pub scope: ChronicleScope,              // World | Player | Firm(FirmId) | District(DistrictId)
}
```
Tekst powstaje dopiero przy wyświetleniu, z szablonu lokalizowanego — inaczej kronika nie da się
przetłumaczyć i puchnie. Magazyn: log dopisywany, obok zapisu gry, rekord ≤ 48 B + indeks
wtórny po `(tick, actor, kind)`. Skala: ~100–200 wpisów/dobę dużego miasta → 100 lat ≈ 5–7 mln
wpisów ≈ 300 MB bez czyszczenia. Dlatego **decymacja po ważności**: wpisy `importance < 30`
starsze niż 5 lat gry są agregowane do podsumowań rocznych. Wpisy dotyczące gracza i jego firm
nigdy nie są usuwane.

**Osiągnięcia emergentne = zapytania do kroniki.** „Twoja firma przetrwała 3 recesje" to zapytanie
`count(ChronicleKind::RecessionEnded) ≥ 3 AND firm_solvent_throughout`. Zero osobnego systemu.

**Porażka (§13.4).** `DeclarePersonalBankruptcy` (lub automatyczne przy niewypłacalności):
majątek firm likwidowany, reszta długu osobistego zostaje z harmonogramem spłat, `Reputation`
obrywa (banki M5/M7 muszą to widzieć przy ocenie zdolności — kontrakt), `PlayerAutonomy::job`
wraca na `Manual`, `CareerTier::derive` naturalnie zwraca `Employee`. **`GameState` pozostaje
`Playing`.** Gra się nie kończy i nie ma ekranu porażki — jest wpis w kronice o wysokiej ważności.

**Śmierć i dziedziczenie.** Zgon postaci przychodzi z demografii M3 — nie mamy osobnej śmierci
dla gracza. `GameState::Succession`: dziedzic z `SetHeir` albo automatycznie (dorosłe dziecko →
małżonek → rodzeństwo o najlepszej relacji). Przechodzi: własność firm (minus podatek spadkowy
wg prawa M8), zobowiązania, reputacja nazwiska. **Nie przechodzi: relacje osobiste i umiejętności** —
dziedzic ma własną sieć i własne kompetencje, i to jest prawdziwy koszt śmierci, znacznie
ciekawszy niż kara pieniężna. Brak dziedzica → ekran „spuścizna" z kroniką dynastii i wyborem
`ContinueAsNewCitizen` (nowa dynastia w tym samym świecie) albo zakończenie.

### 5.12 Onboarding — przełożenie §20.3 na wymagania

Metryka: **czas do pierwszej sensownej decyzji < 15 min**. Definicja operacyjna: pierwsza
`PlayerCommand` z zestawu `{SetPrice, OpenSite, AcceptJobOffer, HireCandidate}`.

Pomiar bez osobnej telemetrii: strumień `ViewCommand` niesie `wall_ms`, a strumień
`PlayerCommand` — `seq`. Czas do pierwszej sensownej decyzji liczymy **offline z dziennika
replayu dowolnego playtestu**. Jeden mechanizm (replay) obsługuje odtwarzanie błędów i metryki
gracza.

Wymagania wyprowadzone z budżetu 15 minut:

1. **Domyślny start to `ExperiencedWorker`, nie `Graduate`.** Absolwent bez kapitału jest
   ciekawszy fabularnie, ale odsuwa pierwszy biznes o godziny gry. Graduate zostaje jako wybór.
2. **Domyślny scenariusz samouczka: „Pierwszy sklep"**, mapa mała, populacja ~20 tys.,
   wyraźna nisza (dzielnica bez sklepu spożywczego) wygenerowana przez `WorldPatch`.
3. **Trzy kroki, ≤ 12 interakcji łącznie, ≤ 3 panele:**
   - (0–3 min) *Kim jesteś* — kamera na domu postaci, karta inspekcji własnej postaci,
     jedno kliknięcie „śledź siebie", doba w 3×.
   - (3–8 min) *Czego brakuje* — nakładka `ShopCatchment` z podświetloną dziurą, karta inspekcji
     sąsiada pokazująca `NotInChoiceSet { TooFar }` — gracz sam widzi popyt.
   - (8–15 min) *Otwórz i wyceń* — `OpenSite` na wskazanej parceli, `SetShelfAssortment`
     (domyślny koszyk jednym kliknięciem), `SetPrice`. Koniec samouczka.
4. **Każdy krok pomijalny**, żaden tekst dłuższy niż 40 słów, zero modalnych okien blokujących.
5. **Domyślny układ nowego gracza: 2 panele** (Sklep, Pulpit). Reszta dostępna, ale nie przypięta
   (`PanelDesc::min_tier`). Panel Finanse z pełną księgowością otwarty w 5. minucie zabija metrykę.
6. **Zwrot w ciągu jednej doby gry.** Po `SetPrice` samouczek przestawia czas na 3× i gwarantuje,
   że w ciągu doby gry panel Sklep pokaże ≥ 3 nazwanych klientów i ≥ 3 nazwane utracone wizyty
   z powodami. Pierwsza decyzja musi dostać wyjaśnioną odpowiedź, inaczej druga metryka z §20.3
   („rozumiem, dlaczego przegrałem", > 80%) nie ma szans.
7. **Test CI (nie ankieta):** skryptowy przebieg samouczka liczy interakcje i otwarte panele —
   regresja powyżej 12/3 wywala build. Samego czasu zegarowego nie testujemy w CI; mierzymy go
   z dzienników playtestów.

---

## 6. Kontrakty międzyfazowe

### Dostarczam

| Typ / funkcja | Crate | Dla kogo |
|---|---|---|
| `PlayerCommand`, `ViewCommand`, `CommandEnvelope`, `CommandError` | `game::command` | M10 (nowe komendy: marka, R&D, giełda), M12 (mody) |
| `fn precheck(&Snapshot, &PlayerCommand) -> Result<(), CommandError>` | `game::command` | wszystkie panele, M10 |
| `ReplayLog` (nagłówek, strumień autorytatywny, strumień widoku) + odtwarzacz | `game::session` | `engine/devtools` (§16.5), M12 (zgłoszenia błędów) |
| `PlayerCharacter`, `PlayerAutonomy`, `CareerTier::derive`, `StartVariant` | `game::player` | M10 (progresja), M12 |
| **Projekt** języka: `Policy`, `Rule`, `ConditionExpr`, `Expr`, `Metric`, `Action`, `PolicyScope`, `PriceBasis` w wyrażeniach | `sim/policy` (**właściciel crate'a: M7**, K-11; autor języka: M9) | M7, M10, M12 |
| `ManagerExecution::from_skill`, eskalacje, `DecisionReason::PolicyApplied` | `game::policy` | M7, M10 |
| Wymagania na `PolicyRunner` (kadencja, rozłożenie w dobie, budżet ms) | spec dla `sim/policy` | **M7** (implementuje) |
| `RuleEditor` + `Diagnostic` + dry-run „30 dni" + serializator tekstowy | `game::policy` | M10, M12 |
| `Scenario`, `Objective`, `Goal`, format `data/scenarios/*.ron` | `game::scenario` | M10 (cele marki/R&D), M12 |
| `ChronicleEntry`, `ChronicleKind`, `chronicle::record()`, `chronicle::query()` | `game::chronicle` | **wszystkie fazy** — każdy system zgłasza swoje zdarzenia |
| `InspectionCard`, `render_reason(&DecisionReason, &Locale)` | `game::inspect` | wszystkie fazy (każda dodaje ramię) |
| `PanelRegistry`, `PanelDesc`, `PanelId` | `game::panels` | **M10** (Brand/Rnd/Stock), M12 (panele z modów) |
| `OverlaySpec`, `OverlayField`, `EntityFilter` | `game::overlays` | M1/M2 (`engine/render` konsumuje spec), M10 |
| `Widget`, `LayoutNode`, `Layout`, `Table<T>`, `Series`, `GraphView`, `GanttView`, `HeatmapThumb`, `DrawList` | `engine/ui` | M10, M11, M12 |
| `LocKey`, katalogi `data/locale/*.ron`, `plural(locale, n)` | `engine/ui` | wszystkie fazy z tekstem |
| `TimeScale`, `StopCondition` | `game::timectl` | M11 (LOD wizualne wg skali), M12 (tryb 50×) |
| `Series` + `MetricsRecorder` (historia metryk do wykresów i dry-runu) | `game::metrics` | M10, `tools/balansator` |

### Konsumuję

| Czego potrzebuję | Od kogo | Uwaga / ryzyko |
|---|---|---|
| `Snapshot` podwójnie buforowany, spójny na granicy ticku, z widokami SoA | M0 (`ecs`, `io`) | bez tego panele czytają rwany stan |
| `DecisionReason` — **jeden centralny enum w `engine/core`, bez `#[non_exhaustive]`** (K-12); każda faza dopisuje wariant + ramię renderujące + `LocKey` | M0, M3–M8 | rozstrzygnięte; brak ramienia = `game/` się nie kompiluje |
| Mieszkaniec z domem, rodziną, pracą, relacjami, pamięcią, planerem dnia, demografią (zgon) | M3 | postać gracza to zwykły `CitizenId` |
| `LodPin` — gwarancja LOD Mikro dla wskazanych encji także przy 10×/50× | M3, M4 | postać gracza i cel trybu „śledź" |
| Zbiór wiedzy mieszkańca o sklepach (§5.7) i rozkład użyteczności zakupu (§6.4) z rozbiciem na składniki | M3, M5 | źródło większości wariantów „dlaczego Anna nie kupiła" |
| `LostSale`: histogram dobowy dla zakładów gracza + bufor 256 wpisów dla oznaczonych | **M5** | do uzgodnienia — §9 |
| Stan półki, transakcje, oferty, księgowość sklepu, banki i ocena zdolności z reputacją | M5 | panel Sklep, Rynek, Finanse, bankructwo |
| `Offer.price_basis` (brutto w detalu, netto w hurcie) oraz rozdzielone przychód netto / VAT należny w księgach | **M5** (K-7), M8 (stawki) | bez tego panele mieszają podstawy, a edytor reguł nie ma czego pokazać w slocie |
| Partie towaru + `BatchProvenance` (etapy z czasem i kosztem) | **M6** | śledzenie partii „od pola do półki" — §9 |
| Kontrakty B2B, zlecenia transportowe, topologia dostawca-odbiorca | M6 | panel Łańcuch dostaw (graf) |
| Zlecenia produkcyjne z oknami czasu, maszyny, przeglądy | M6 | Gantt w panelu Zakład |
| Umiejętności menedżera, HR, rynek pracy, mediana płac, rotacja, strajki | M7 | `ManagerExecution`, panel Ludzie |
| `sim/policy`: AST + ewaluator + `PolicyRunner` wg mojego projektu języka | **M7** (K-11) | rozstrzygnięte; M9 dokłada edytor, diagnostykę, dry-run i `ManagerExecution` |
| Trasy, czasy przejazdu, parkingi, pojazdy | M4 | odległość i bariera „brak parkingu" w karcie inspekcji |
| Podatki, pozwolenia, przetargi, wybory, prawo spadkowe | M8 | panel Miasto, sukcesja |
| Zdarzenia świata (awarie, pogoda, recesje) jako `ChronicleKind` | M8, M11 | kronika, warunki zatrzymania |
| Renderowanie pól skalarnych i strumieni na terenie | M1, M2 | nakładki danych — `game/` daje tylko spec |
| Zapis/wczytanie snapshotu + strumieni pobocznych (kronika, serie, dziennik replay) | M0 (min.), M12 (pełny) | §9 — format plików pobocznych |
| Blok `StreamId` **260–279** (K-4) | M0 | M9 używa `PolicyExecution = 260`; 261–279 wolne na przyszłe strumienie gracza |

---

## 7. Testy i kryteria akceptacji

### Determinizm i replay

| Test | Kryterium |
|---|---|
| Odtworzenie sesji | Nagrany dziennik 10 lat gry (headless, skryptowane komendy) odtworzony daje identyczny łańcuch hashy stanu co 1000 ticków |
| Komendy odrzucone | Dziennik z komendami nieprawidłowymi odtwarza się identycznie — odrzucenie z tym samym `CommandError` |
| Polityki | 200 sklepów z politykami, dwa przebiegi tego samego seeda → identyczne ceny i zamówienia |
| Menedżer | Błąd wykonania i pominięcia pochodzą wyłącznie z `rng(seed, StreamId::PolicyExecution, ..)` — brak `Instant::now()` w `game/` (test lintujący) |
| Strumień widoku | Zmiana `TimeScale`, pauzy, układu paneli i sortowań **nie zmienia** łańcucha hashy |
| Sukcesja i bankructwo | Deterministyczny wybór dziedzica; suma pieniądza zachowana (majątek = spadek + podatek spadkowy + spłacone długi) |

### Wydajność UI — budżety na klatkę

Budżet klatki 16,6 ms przy 60 FPS. **UI dostaje 4,0 ms CPU i 2,0 ms GPU**, gdy panele są otwarte.

| Scenariusz | Budżet | Jak osiągnięty |
|---|---|---|
| Klatka bez zmian danych i bez wejścia | **0,00 ms, 0 alokacji** | dirty-flagging po `DataVersion`; test z licznikiem alokacji |
| Przebudowa pojedynczego panelu (zmiana danych) | ≤ 2,0 ms | przebudowa tylko brudnego poddrzewa |
| `Table<T>` 100k wierszy — budowa i rysowanie widoku | ≤ 0,5 ms | wirtualizacja: ~60 wierszy + 8 overscan, stała wysokość |
| `Table<T>` 100k — przewijanie | ≤ 0,2 ms | brak ponownego sortowania i filtrowania |
| `Table<T>` 100k — zmiana sortowania | ≤ 12 ms **poza klatką** (`engine/jobs`) | klucze `u64` + pdqsort; do czasu gotowości widoczny stary porządek |
| `Table<T>` 100k — zmiana filtra | ≤ 8 ms poza klatką | wynik jako bitset, przyrostowo |
| Wykres 10 lat danych dziennych × 8 serii (3600 próbek/serię, K-1) | ≤ 0,3 ms CPU / 0,2 ms GPU | piramida mip (dzień/dekada/miesiąc/kwartał), ≤ 2000 odcinków niezależnie od zakresu |
| Graf łańcucha dostaw, 500 węzłów / 2000 krawędzi — rysowanie | ≤ 0,8 ms | instancjonowanie węzłów i krawędzi, cache geometrii |
| Ten sam graf — przeliczenie układu | ≤ 50 ms **poza klatką**, tylko przy zmianie topologii | Sugiyama warstwowy, cache po hashu topologii |
| Gantt, 500 pasków w oknie | ≤ 0,4 ms | wirtualizacja po oknie czasu |
| Miniatura mapy cieplnej 256×256 | ≤ 0,3 ms, odświeżanie `EveryHour` | pole skalarne z nakładki, nie przeliczane per klatka |
| `PolicyRunner`, 200 zakładów, doba gry | ≤ 1,0 ms sumarycznie na wątku symulacji | `EveryDay` rozłożone: zakład `i` w minucie `(i*37) % 1440` |
| Tryb 50× z otwartymi panelami | UI ≤ 1,0 ms | panele na `EveryHour`, Gantt i śledzenie wyłączone |

Wszystkie mierzone w `criterion` na bezgłowym uprzęży UI (bez GPU) — zgodnie z zasadą
headless-first z doc 00 §6.

### Wyjaśnialność

- Test wyczerpującego `match`: dodanie wariantu `DecisionReason` bez ramienia renderującego
  **nie kompiluje się**.
- Fuzz: 1000 losowych `DecisionReason` renderuje niepusty tekst w PL i EN; żaden nie zwraca
  klucza zamiast tłumaczenia.
- Scenariusz akceptacyjny „Anna": w sklepie gracza z celowo zawyżoną ceną, w ciągu 1 doby gry
  panel Sklep pokazuje ≥ 3 nazwane utracone wizyty, każda z powodem i — tam, gdzie dotyczy —
  z nazwanym konkurentem i różnicą ceny.
- Każde zastosowanie polityki ma `DecisionReason::PolicyApplied` z wejściami i odchyleniem
  menedżera (test: 100% zastosowań, nie próbka).

### Język reguł

- 6 polityk z §5.6 zbudowanych **wyłącznie przez interakcje edytora** (test skryptowy na
  `UiIntent`), bez wpisywania tekstu.
- Round-trip: AST → tekst → AST identyczne dla 10 tys. losowych poprawnych polityk.
- **Podstawa ceny (K-7):** nie da się złożyć w edytorze porównania ani akcji mieszającej brutto
  z netto; import tekstu z taką mieszanką zwraca `PriceBasisMismatch`. Reguła „−2% względem
  najtańszego konkurenta w promieniu 3 km" wykonana na sklepie daje cenę brutto równą
  `0,98 × cena_brutto_konkurenta` co do grosza.
- Walidator łapie: niezgodność jednostek, oscylację bez martwej strefy, regułę nieosiągalną,
  konflikt zakresów, przekroczony budżet kosztu.
- Własność: żadna polityka nie tworzy pieniądza — zamówienia ograniczone limitem kredytowym,
  ceny ≥ 0; przekroczenie jest przycinane i raportowane jako alert, nigdy nie panikuje.
- Dry-run na 30 dniach zgodny z późniejszym rzeczywistym wykonaniem przy `ManagerExecution`
  o `skill = 100` (tolerancja 0).

### Kariera i porażka

- **Brak sztucznych blokad**: w `Sandbox` gracz wydaje `FoundFirm` w ticku 1 i komenda przechodzi;
  test przeszukuje kod pod kątem sprawdzania `CareerTier` w ścieżce walidacji komend (musi być puste).
- Bankructwo osobiste: `GameState` pozostaje `Playing`, `CareerTier::derive == Employee`,
  harmonogram długu aktywny, ocena zdolności w banku pogorszona.
- Śmierć: sukcesja przenosi własność i zobowiązania, **nie przenosi relacji ani umiejętności**;
  brak dziedzica → ekran spuścizny z pełną kroniką dynastii.
- Scenariusz „Zbuduj sieć 50 sklepów" przechodzi w headless w skryptowanym przebiegu.

### UI, i18n, DPI

- Pseudo-lokalizacja (napisy ×1,4) — brak przepełnień układu w żadnym panelu.
- Każdy `LocKey` istnieje w `pl` i `en`; brak literałów tekstowych w konstruktorach widgetów.
- Pluralizacja PL: `1 sklep / 2 sklepy / 5 sklepów / 1,5 sklepu` — tablica przypadków w teście.
- `ui_scale` 0,75 / 1,0 / 1,5 / 2,0 / 3,0 — brak rozmyć i przycięć, przyciąganie do pikseli.
- **Kalendarz (K-1):** oś czasu wykresu na 10 latach daje dokładnie 120 podziałek miesięcznych
  i 40 kwartalnych; agregacja mip dzień→dekada→miesiąc→kwartał jest bezstratna dla sum
  (`sum` poziomu wyższego = suma poziomu niższego, tolerancja 0).
- **VAT (K-7):** w panelu Finanse suma wiersza „przychód netto" nigdy nie zawiera VAT-u;
  test na wygenerowanych księgach: `obrót_brutto = przychód_netto + VAT_należny` i VAT
  występuje wyłącznie po stronie zobowiązań.

### Onboarding

- Skryptowy przebieg samouczka: ≤ 12 interakcji i ≤ 3 panele do pierwszej `SetPrice` (test CI).
- Z dzienników playtestów liczony `t_first_meaningful_decision`; cel: mediana < 15 min,
  percentyl 90 < 25 min. Raport generowany z replayów, nie z osobnej telemetrii.

---

## 8. Ryzyka fazy i mitygacje

| Ryzyko | Skutek | Mitygacja |
|---|---|---|
| **`engine/ui` to największa pojedyncza masa kodu w projekcie i nie ma zapasowego planu** (egui wolno tylko w devtools, §16.1) | Faza się rozjeżdża, panele czekają na widgety | Widgety budowane wyłącznie pod konkretny panel, który ich żąda (kolejność WP). Dopuszczamy **tymczasowe** panele na egui za flagą `dev-panels` w buildach deweloperskich, usuwane do końca fazy — to pozwala testować mechaniki, zanim widget powstanie |
| **Język reguł puchnie w język programowania** | Nieskończona faza, nieuczalne UI | Twarde limity w §5.6 (bez zmiennych, pętli, funkcji; ≤ 8 reguł, głębokość ≤ 3). Czego zabraknie — idzie do M12/modding, nie do M9 |
| **Dług wyjaśnialności z M3–M8**: warianty `DecisionReason` okażą się ubogie i karta inspekcji nie odpowie na pytania gracza | Główna obietnica gry (§14.1) niespełniona, metryka §20.3 „rozumiem, dlaczego przegrałem" nie do osiągnięcia | Audyt na starcie fazy: lista pytań z §5.7 skonfrontowana z istniejącymi wariantami; braki zgłoszone jako wymagania do faz M3–M8 **przed** WP5, nie po |
| **`LostSale` zbyt drogie** przy 400 tys. agentów | Panel Sklep bez odpowiedzi „kto nie kupił" | Dwa poziomy: histogram (zawsze, zakłady gracza) + bufor 256 (tylko oznaczone). Dla zakładów AI zero kosztu |
| **Postać gracza i cele śledzenia przypięte do Mikro przy 50×** | Tryb makro traci wydajność | `LodPin` ograniczony do ≤ 8 encji; przy `X50` tryb śledzenia domyślnie wyłączony, włączenie jest świadomym kosztem pokazanym graczowi |
| **Kronika rośnie w nieskończoność** (100 lat gry, §19 M12) | Zapis puchnie, wyszukiwanie wolne | Rekord ≤ 48 B, dane typowane zamiast stringów, decymacja po ważności powyżej 5 lat, wpisy gracza nietykalne |
| **Panele czytają stan w połowie ticku** | Niedeterminizm tego, co gracz widzi; komenda wydana na nieaktualnym stanie | Typ: panele dostają wyłącznie `&Snapshot`; brak dostępu do `&World` w `game::panels` (egzekwowane widocznością modułów) |
| **Rozjazd języka reguł z M7** — `sim/policy` należy do M7, a język projektuję ja (K-11); M7 może dodać metrykę lub akcję poza gramatyką | Edytor nie umie pokazać czegoś, co AI już robi; gracz i AI przestają mieć „ten sam zestaw narzędzi" (§6.3) | Gramatyka z §5.6 jest wiążąca: rozszerzenie języka wymaga dopisania slotu w edytorze, więc każda nowa metryka/akcja M7 to zmiana uzgodniona. Test: enum `Metric` i `Action` mają wyczerpujące pokrycie w edytorze — nowy wariant bez slotu łamie build `game/` |
| **Onboarding przegrywa z bogactwem UI** | Metryka < 15 min nieosiągalna | Domyślny układ 2 paneli, `min_tier` sterujący przypinaniem, test CI na liczbę interakcji od pierwszego dnia WP12, nie na końcu |
| **Sortowanie/filtrowanie 100k wierszy w klatce** | Zacięcia przy każdym kliknięciu nagłówka | Cała praca poza klatką w `engine/jobs`; widoczny stary porządek do czasu gotowości; brak jakiejkolwiek pracy O(n) w ścieżce przewijania |
| **Eksplozja liczby komend** (`PlayerCommand` ma ~70 wariantów) | Trudny replay, trudna walidacja | Jedna funkcja `precheck` użyta w obu miejscach; test pokrycia: każdy wariant ma co najmniej jeden test walidacji i jeden wpis w dzienniku replayu |

---

## 9. Decyzje otwarte

Rozstrzygnięte przez koordynatora w trakcie planowania i **usunięte z tej listy**:
język reguł i własność `sim/policy` (**K-11** — M7 właścicielem crate'a, M9 autorem języka),
`DecisionReason` jako jeden centralny enum bez `#[non_exhaustive]` (**K-12**),
podstawa ceny `Offer.price_basis` i rozdział przychód netto / VAT (**K-7**),
blok `StreamId` 260–279 (**K-4**), kalendarz 360 dni = 12 × 30 (**K-1**).

| # | Decyzja | Kontekst | Propozycja M9 | Z kim | Status |
|---|---|---|---|---|---|
| 1 | **Kto zapisuje `LostSale`** i jakim kosztem | Bez tego nie ma odpowiedzi „dlaczego Anna nie kupiła" — sedno §14.1 | `sim/economy` zapisuje, ale wyłącznie dla zakładów z flagą `observed_by_player`: histogram dobowy zawsze, bufor 256 wpisów dla oznaczonych. Flagę ustawia `game/` przy zmianie własności | **M5** | przekazane właścicielowi (M5) |
| 2 | **`LodPin`** — czy M3/M4 gwarantują Mikro dla wskazanych encji przy 10× i 50× | Postać gracza i tryb „śledź" nie mają sensu w mezo | ≤ 8 przypiętych encji; przy `X50` przypięcie kosztuje i jest komunikowane | **M3, M4** | przekazane właścicielowi (M3/M4) |
| 3 | **Głębokość `BatchProvenance`** | §14.4: „od pola do półki, z czasem i kosztem na każdym etapie". Pełny łańcuch dla milionów partii jest drogi | Pełny łańcuch tylko dla partii dotkniętych przez zakłady gracza; dla reszty ostatnie 3 etapy. Jeśli M6 nie da rady — panel degraduje się do „ostatnie 3 etapy" i trzeba to przyznać w PRD | **M6** | przekazane właścicielowi (M6) |
| 4 | **Czy w kalendarzu 12 × 30 istnieje tydzień 7-dniowy** | K-1 daje 360 dni = 12 × 30, ale 30 nie dzieli się przez 7. Dotyczy `Metric::DayOfWeek`, `WeekSchedule` w `SetOpeningHours`/`SetOwnShift` i rytmu „weekendowego" popytu | Albo tydzień 7-dniowy dryfujący względem miesiąca (realizm, ale brzydka arytmetyka osi), albo dekada 10-dniowa z „wolnym" co 10. dzień. **Rekomendacja: tydzień 7-dniowy dryfujący** — rytm tygodniowy jest mocno widoczny w handlu detalicznym i szkoda go stracić; piramida mip wykresów i tak używa dekad, więc nic nie traci. Typ `DayOfWeek` musi pochodzić z kalendarza w `engine/core`, nie z `game/` | **M0**, M3, M8 | otwarte |
| 5 | **Odwzorowanie umiejętności menedżera na jakość wykonania** | Kto jest właścicielem krzywej `from_skill` | M7 jest właścicielem umiejętności, M9 odwzorowania. Krzywa w `data/` (moddowalna), nie w kodzie | M7 | otwarte |
| 6 | **Format plików pobocznych zapisu**: kronika, serie metryk, dziennik replay | M0 daje snapshot minimalny, pełne wersjonowanie dopiero M12 | Trzy pliki obok zapisu, każdy z `schema_version`; M12 wciąga je w migracje | M0, M12 | otwarte |
| 7 | **Czy strumień `ViewCommand` jest obowiązkową częścią zapisu** | Potrzebny do zgłoszeń błędów i metryk §20.3, ale to megabajty | Nie w zapisie gry; osobny plik, domyślnie włączony, wyłączalny w ustawieniach; zawsze dołączany do zgłoszenia błędu | M12 | otwarte |
| 8 | **Podatek spadkowy i prawo spadkowe** przy sukcesji | §13.4 wymaga przejścia majątku; stawka to prawo miejskie | M8 dostarcza stawkę i tryb; M9 wykonuje transfer i sprawdza własność pieniądza | **M8** | otwarte |
| 9 | **`WorldPatch` dla scenariuszy** („Uratuj upadającą hutę") | Scenariusz musi deterministycznie zmodyfikować wygenerowany świat, nie łamiąc kontraktu hasha | Łatki stosowane jako komendy w ticku 0, po generacji, przed pierwszym systemem — wtedy hash pozostaje funkcją `(seed, lista łatek)` | **M1, M2** | otwarte |
| 10 | **Czy `actor: PlayerId` zostaje w kopercie komendy** | §18.2 wskazuje lockstep jako możliwość; koszt 2 bajty na komendę | Zostaje. Dwa bajty teraz są tańsze niż migracja formatu replayu później | — | otwarte |
| 11 | **Rezerwacje dla M10** | `OverlayField::BrandAwareness`, `Goal::ProductLaunched`, `LostToCompetitor { dominant: Brand }`, `PanelId::{Brand, Rnd, Stock}` | Warianty istnieją od M9 jako nieaktywne, M10 je zasila bez zmiany typów | **M10** | otwarte |
| 12 | **Kto jest właścicielem `Series`/`MetricsRecorder`** | Balansator (M5) też chce historii metryk | `game/` zapisuje serie dla gracza; `tools/balansator` ma własny zapis headless. Wspólny jest tylko typ `Series` w `engine/ui` | M5, M12 | otwarte |

---

## 10. Szacunek wielkości

| WP | Zakres | Rozmiar |
|---|---|---|
| WP1 | Szkielet `game/`, pętla, stany, sesja, integracja sim+engine | **M** |
| WP2 | `PlayerCommand` (~70 wariantów), koperta, walidacja, dziennik i odtwarzacz replayu | **L** |
| WP3 | Rdzeń `engine/ui`: retained-mode, layout, dirty-flagging, DPI, atlas fontów, i18n z pluralizacją | **XL** |
| WP4 | Postać gracza, autonomia, 5 wariantów startu, ekran wyboru | **M** |
| WP5 | Karta inspekcji, render `DecisionReason`, `LostSale` | **M** |
| WP6 | `Table<T>` 100k, wykresy z piramidą mip, miniatury map cieplnych | **L** |
| WP7 | Nakładki danych i filtry encji | **M** |
| WP8 | AST języka reguł, serializator tekstowy, walidator, `RuleEditor`, dry-run | **L** |
| WP9 | `PolicyRunner`, `ManagerExecution`, eskalacje, powody decyzji | **M** |
| WP10 | 9 paneli biznesowych + `GraphView` + `GanttView` + `PanelRegistry` | **XL** |
| WP11 | Sterowanie czasem, warunki zatrzymania, tryb śledzenia, magazyn i wyszukiwarka kroniki | **L** |
| WP12 | Kariera, 5 scenariuszy, cele, bankructwo, sukcesja, samouczek, metryki | **L** |

Rozkład masy: **WP3 i WP10 to razem około połowy fazy.** To nie jest przypadek — M9 jest fazą,
w której powstaje całe UI gry, a nie tylko warstwa gracza. Jeśli faza ma się rozjechać, rozjedzie
się tam, dlatego WP3 startuje najwcześniej jak to możliwe (zaraz po WP1) i dlatego dopuszczamy
tymczasowe panele na egui za flagą deweloperską, żeby mechaniki z WP8/WP9/WP12 dało się testować
niezależnie od postępu widgetów.
