//! Sesja gry: stojący świat, kolejność klatki i granica czytania stanu
//! (M9a §5.1, §5.2).
//!
//! # Kolejność klatki jest kontraktem determinizmu
//!
//! ```text
//! 1. wejście surowe            → intencje
//! 2. intencje                  → komendy (autorytatywne) i widok (nieautorytatywny)
//! 3. dziennik replayu          — PRZED wykonaniem
//! 4. ile ticków wg TimeScale   → Session::step
//!      └─ komendy w punkcie synchronizacji na początku ticku
//! 5. podmiana bufora migawki
//! 6. przebudowa brudnych poddrzew UI
//! 7. render
//! ```
//!
//! Kroki 1–2 i 5–7 należą do klienta i do `M9b`; tutaj jest krok 3 i 4, czyli
//! wszystko, co da się uruchomić bez GPU. `game/` **nie zależy od `wgpu` ani od
//! `winit`** i to nie jest przypadek: gdyby zależał, bramka R-5 (zakaz GPU
//! w przebiegu bezgłowym) przestałaby cokolwiek chronić.
//!
//! **Zasada nienegocjowalna:** kod paneli nie widzi `&World`. Do czasu, aż `M9b`
//! postawi podwójnie buforowaną migawkę, jedynym wejściem komend jest
//! [`CommandView`] — i ono `&World` też nie widzi.

use magnat_core::{StateHash, Tick};
use magnat_economy::Market;
use magnat_ecs::App;
use magnat_jobs::JobPool;

use crate::command::{
    precheck, CommandEnvelope, CommandError, CommandView, PlayerCommand, PlayerId,
    ViewCommand, ViewRecord,
};
use crate::replay::{Rejected, ReplayLog};
use crate::shell::{NewGameParams, ShellScreen, WorldGenJob, WorldPreview};
use crate::world::{stand_up, BuiltCity, StandingReport};

/// Stan gry jako całości.
///
/// Pauza **nie jest** stanem gry — to `SimSpeed::Paused`. Jeden mechanizm, nie dwa.
/// Menu pauzy jest za to zwykłym `ShellScreen::Pause`: stan się zmienia, ale sesja
/// zostaje w pamięci i zegar stoi, więc powrót do gry nie wczytuje niczego.
///
/// Wariantów jest siedem. `Succession` i `ScenarioEnd` doszły w `M9e` razem ze swoją
/// **treścią** — sukcesja ma komendy i dziedzica, domknięcie scenariusza ma rozliczenie
/// celów — ale **nie ze swoimi ekranami i nie z przejściem**: nikt ich dziś nie
/// konstruuje, a `legacy::check` nie ma wołającego. To jest `DI-33` i `DI-34`
/// w `M9e-panele-czas-kariera.md`, wpisane jako brak, a nie przemilczane.
pub enum GameState {
    /// Wszystko poza rozgrywką — §5.13.
    Shell(ShellScreen),
    /// Generacja świata w tle, z postępem i anulowaniem.
    Generating(WorldGenJob),
    /// Podgląd: „gram tutaj" / „losuj ponownie" / „zmień parametry".
    WorldReady {
        preview: Box<WorldPreview>,
        built: Box<BuiltCity>,
    },
    /// Świat stoi, ale gracz nie ma jeszcze ciała: wybór postaci (WP4).
    ///
    /// Sesja jest już tutaj — kandydaci powstają **z postawionego świata**, a nie
    /// z parametrów, bo predykat wariantu pyta o wiek, pracę i oszczędności.
    CharacterSelect(Box<Session>),
    Playing(Box<Session>),
    /// Postać zmarła. Świat tyka dalej — gracz wybiera dziedzica albo nową dynastię.
    /// Sesja zostaje: sukcesja jest komendą w tym samym świecie, a nie nową grą.
    Succession {
        session: Box<Session>,
        /// Kogo proponuje gra. `None` = nie ma dziedzica i zostaje ekran spuścizny.
        heir: Option<magnat_core::CitizenId>,
    },
    /// Scenariusz się domknął: cele rozliczone, gra czeka na decyzję gracza.
    ScenarioEnd {
        session: Box<Session>,
        outcome: crate::scenario::ScenarioOutcome,
    },
}

impl GameState {
    #[must_use]
    pub fn is_playing(&self) -> bool {
        matches!(self, GameState::Playing(_))
    }

    /// Sesja, jeśli świat stoi — także w trakcie wyboru postaci, bo ekran kandydatów
    /// czyta z niej populację.
    #[must_use]
    pub fn session(&self) -> Option<&Session> {
        match self {
            GameState::Playing(s)
            | GameState::CharacterSelect(s)
            | GameState::Succession { session: s, .. }
            | GameState::ScenarioEnd { session: s, .. } => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        match self {
            GameState::Playing(s)
            | GameState::CharacterSelect(s)
            | GameState::Succession { session: s, .. }
            | GameState::ScenarioEnd { session: s, .. } => Some(s),
            _ => None,
        }
    }
}

/// Stojący świat pod kontrolą gry.
pub struct Session {
    pub app: App,
    /// Rynek, jeśli gospodarka jest włączona.
    pub market: Option<Market>,
    /// Teren i miasto — czyta je render i panele, nie symulacja.
    pub built: BuiltCity,
    pub report: StandingReport,
    log: ReplayLog,
    /// Koperty czekające na swój tick.
    pending: Vec<CommandEnvelope>,
    next_seq: u64,
    /// Postać gracza. `None` do czasu `SetCharacter` — świat wtedy stoi i tyka,
    /// ale gracz nie ma jeszcze ciała.
    player: Option<crate::PlayerCharacter>,
    /// Historia metryk gracza do wykresów (`DF-4`). Strona widoku: nie wchodzi
    /// do hasha stanu i nie zmienia wyniku symulacji.
    metrics: crate::metrics::MetricsRecorder,
    /// Kronika: dziennik świata i gracza. Widok pochodny — zbiera raz na dobę to,
    /// co i tak już leży w dziennikach `sim/*` (`game::chronicle`).
    chronicle: crate::chronicle::Chronicle,
    /// Scenariusz i postęp jego celów.
    ///
    /// Stoi **w sesji**, a nie w kliencie, bo rozstrzyga o tym, czy gra jest wygrana,
    /// a to nie jest sprawa okna: przebieg bezgłowy musi dostać tę samą odpowiedź.
    /// Wynika w całości z koperty `StartGame` i ze stanu świata, więc replay odtwarza
    /// go bez zapisywania czegokolwiek osobno.
    scenario: Option<crate::scenario::Scenario>,
    scenario_state: crate::scenario::ScenarioState,
    /// Cele domknięte w ostatniej dobie — wejście warunku „cel osiągnięty" i kroniki.
    fresh_objectives: Vec<crate::scenario::ObjectiveId>,
    /// Czas realny spędzony w sesji. Liczony z `dt` podawanego przez wołającego,
    /// **nigdy z `Instant::now()`** — w kodzie sesji zegar ścienny jest zakazany
    /// (00 §3.5), a ta liczba i tak jest metadaną slotu, nie stanem.
    played_ms: u64,
}

impl Session {
    /// Etap B zakładania gry: zaludnienie, gospodarka i harmonogram na gotowym
    /// mieście. Pierwszą kopertą dziennika jest `StartGame` z kompletem parametrów.
    ///
    /// # Errors
    /// Jak [`stand_up`].
    pub fn begin(
        built: BuiltCity,
        params: NewGameParams,
        pool: &JobPool,
    ) -> Result<Session, Box<dyn std::error::Error>> {
        let standing = stand_up(&built, params.world.seed, params.opts, pool)?;
        let mut s = Session {
            app: standing.app,
            market: standing.market,
            built,
            report: standing.report,
            log: ReplayLog::new(params),
            pending: Vec::new(),
            next_seq: 0,
            player: None,
            played_ms: 0,
            metrics: crate::metrics::MetricsRecorder::default(),
            chronicle: crate::chronicle::Chronicle::default(),
            scenario: None,
            scenario_state: crate::scenario::ScenarioState::default(),
            fresh_objectives: Vec::new(),
        };
        s.load_scenario(params.scenario);
        // Świat już stoi, więc ta koperta niczego nie wykonuje — niesie za to
        // wszystko, czego trzeba, żeby go odtworzyć (§5.13).
        s.submit_at(
            Tick(0),
            PlayerCommand::StartGame {
                world: params.world,
                scenario: params.scenario,
                variant: params.variant,
                pick: None,
            },
        );
        Ok(s)
    }

    #[must_use]
    pub fn tick(&self) -> Tick {
        self.app.world.tick
    }

    /// Ziarno świata — karta mieszkańca odtwarza z niego plan dnia. Jedno źródło:
    /// nagłówek dziennika wejść, czyli to samo, z czego odtwarza się cała sesja.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.log.header.params.world.seed
    }

    #[must_use]
    pub fn state_hash(&self) -> StateHash {
        magnat_io::world_state_hash(&self.app.world)
    }

    #[must_use]
    pub fn log(&self) -> &ReplayLog {
        &self.log
    }

    #[must_use]
    pub fn played_ms(&self) -> u64 {
        self.played_ms
    }

    /// Widok, na którym wykonuje się walidacja i komendy.
    #[must_use]
    pub fn view(&self) -> CommandView<'_> {
        CommandView {
            market: self.market.as_ref(),
            world: Some(&self.app.world),
            has_character: self.player.is_some(),
        }
    }

    /// Historia metryk gracza — wejście wykresów w panelach.
    #[must_use]
    pub fn metrics(&self) -> &crate::metrics::MetricsRecorder {
        &self.metrics
    }

    /// Wczytuje scenariusz z katalogu i stosuje jego łatki.
    ///
    /// Łatki idą **przed pierwszym tickiem**, więc hash zostaje funkcją
    /// `(ziarno, lista łatek)` — dokładnie tak, jak zapowiadała decyzja otwarta
    /// nr 9 dokumentu fazy. Brak katalogu nie jest błędem gry: scenariusz jest
    /// warstwą nad światem, a świat stoi i bez niego.
    fn load_scenario(&mut self, id: crate::ScenarioId) {
        let Ok(katalog) = crate::scenario::ScenarioCatalog::load() else {
            return;
        };
        let Some(sc) = katalog.by_id(id).cloned() else {
            return;
        };
        crate::scenario::apply_patches(self, &sc.patches);
        self.scenario = Some(sc);
    }

    /// Scenariusz tej gry, jeśli jakiś jest.
    #[must_use]
    pub fn scenario(&self) -> Option<&crate::scenario::Scenario> {
        self.scenario.as_ref()
    }

    #[must_use]
    pub fn scenario_state(&self) -> &crate::scenario::ScenarioState {
        &self.scenario_state
    }

    /// Jak stoi scenariusz. `Running` także wtedy, gdy scenariusza nie ma — tryb
    /// otwarty nie da się wygrać ani przegrać i to jest jego treść.
    #[must_use]
    pub fn scenario_outcome(&self) -> crate::scenario::ScenarioOutcome {
        self.scenario.as_ref().map_or(
            crate::scenario::ScenarioOutcome::Running,
            |sc| self.scenario_state.outcome(self, sc),
        )
    }

    /// Cele domknięte w ostatniej przeliczonej dobie.
    #[must_use]
    pub fn fresh_objectives(&self) -> &[crate::scenario::ObjectiveId] {
        &self.fresh_objectives
    }

    /// Kronika — wejście panelu Kronika i osiągnięć emergentnych.
    #[must_use]
    pub fn chronicle(&self) -> &crate::chronicle::Chronicle {
        &self.chronicle
    }

    /// Postać gracza, jeśli została wybrana.
    #[must_use]
    pub fn player(&self) -> Option<&crate::PlayerCharacter> {
        self.player.as_ref()
    }

    /// Zgłasza komendę gracza na **najbliższy** tick.
    ///
    /// Zwrócony `Err` jest odpowiedzią dla UI („czego brakuje"), a nie decyzją —
    /// decyzja zapada przy stosowaniu, w punkcie synchronizacji. Koperta trafia
    /// do dziennika **w obu przypadkach**: replay ma odrzucić ją tak samo.
    pub fn submit(&mut self, cmd: PlayerCommand) -> Result<u64, CommandError> {
        let t = Tick(self.app.world.tick.get() + 1);
        let odpowiedz = precheck(&self.view(), &cmd);
        let seq = self.submit_at(t, cmd);
        odpowiedz.map(|()| seq)
    }

    fn submit_at(&mut self, tick: Tick, cmd: PlayerCommand) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let env = CommandEnvelope {
            seq,
            tick,
            actor: PlayerId(0),
            cmd,
        };
        self.log.commands.push(env.clone());
        self.pending.push(env);
        seq
    }

    /// Zapisuje komendę widoku. Nie dotyka symulacji — stąd znacznik czasu
    /// rzeczywistego, którego w strumieniu autorytatywnym być nie może.
    pub fn record_view(&mut self, wall_ms: u64, cmd: ViewCommand) {
        let tick = self.app.world.tick;
        self.log.view.push(ViewRecord { wall_ms, tick, cmd });
    }

    /// Wykonuje `minutes` ticków; `dt_ms` dolicza się do czasu gry w slocie.
    ///
    /// Po każdej minucie zapisuje metryki doby, jeśli właśnie minęła. Metryki są
    /// **stroną widoku** — nie dotykają świata i nie wchodzą do hasha (00 §3.6) —
    /// ale prowadzi je krok sesji, a nie klient: inaczej wykres w oknie pokazywałby
    /// inną historię niż ta, którą mierzy przebieg bezgłowy.
    pub fn step(&mut self, minutes: u32, dt_ms: u64) {
        self.played_ms += dt_ms;
        for _ in 0..minutes {
            let t = Tick(self.app.world.tick.get() + 1);
            self.apply_due(t);
            self.app.tick();
            let mut m = std::mem::take(&mut self.metrics);
            m.maybe_record(self);
            self.metrics = m;
            let mut k = std::mem::take(&mut self.chronicle);
            k.harvest(self);
            self.chronicle = k;
            if let Some(sc) = self.scenario.clone() {
                let mut st = std::mem::take(&mut self.scenario_state);
                let swieze = st.step(self, &sc);
                self.scenario_state = st;
                if !swieze.is_empty() {
                    self.fresh_objectives = swieze;
                }
            }
        }
    }

    /// Stosuje koperty należne tickowi `t`, w kolejności `(actor, seq)` (00 §3.4).
    fn apply_due(&mut self, t: Tick) {
        if self.pending.is_empty() {
            return;
        }
        let mut due: Vec<CommandEnvelope> = Vec::new();
        self.pending.retain(|e| {
            if e.tick.get() <= t.get() {
                due.push(e.clone());
                false
            } else {
                true
            }
        });
        due.sort_by_key(|e| (e.actor, e.seq));
        for e in due {
            if let Err(err) = self.wykonaj(t, &e.cmd) {
                self.log.rejected.push(Rejected {
                    seq: e.seq,
                    error: err,
                });
            }
        }
    }

    /// Wykonanie jednej komendy.
    ///
    /// Komendy, którym wystarczy rynek, idą przez [`apply`] — tę samą funkcję, którą
    /// woła panel przy wygaszaniu przycisku. Reszta zmienia **świat i sesję** naraz
    /// (znacznik gracza w `Identity`, kapitał w księgach, rejestr firm, rynek pracy),
    /// więc mieszka w [`crate::command::exec`]: `apply` dostaje widok, a nie
    /// `&mut World`, i tak ma zostać.
    fn wykonaj(&mut self, t: Tick, cmd: &PlayerCommand) -> Result<(), CommandError> {
        crate::command::exec::run(self, t, cmd)
    }

    /// Wariant startu z nagłówka dziennika — łatkę kapitału stosuje `SetCharacter`.
    pub(crate) fn variant(&self) -> crate::StartVariant {
        self.log.header.params.variant
    }

    /// Postać gracza na zapis. `pub(crate)`, bo zmienia ją **wyłącznie** wykonawca
    /// komendy — panel dostaje `&Session` i zmienić jej nie może.
    pub(crate) fn player_mut(&mut self) -> &mut Option<crate::PlayerCharacter> {
        &mut self.player
    }

    /// Przewija `ticks` ticków, zbierając hash stanu co `hash_every` (0 = nie liczyć).
    ///
    /// Jedno miejsce zamiast ósmej kopii tej pętli w scenariuszach: łańcuch hashy
    /// jest kryterium WP2, więc należy do sesji, a nie do narzędzia.
    pub fn run_hashing(&mut self, ticks: u64, hash_every: u64) -> Vec<(u64, StateHash)> {
        let mut out = Vec::new();
        for _ in 0..ticks {
            self.step(1, 0);
            let t = self.app.world.tick.get();
            if hash_every > 0 && t.is_multiple_of(hash_every) {
                out.push((t, self.state_hash()));
            }
        }
        out
    }
}

/// Co poszło nie tak przy odtwarzaniu.
#[derive(Debug)]
pub enum ReplayMismatch {
    /// Dziennik nie zaczyna się od `StartGame`.
    NoStartGame,
    /// Koperta `StartGame` opisuje inny świat niż nagłówek — dziennik jest niespójny.
    ParamsDiffer,
    /// Świat odrzucił komendę inaczej niż przy nagraniu. To jest **ten** test:
    /// odtworzenie ma odrzucać identycznie (§5.5).
    RejectionDiffers {
        seq: u64,
        recorded: Option<CommandError>,
        replayed: Option<CommandError>,
    },
    Build(String),
}

impl std::fmt::Display for ReplayMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayMismatch::NoStartGame => write!(f, "dziennik nie zaczyna się od StartGame"),
            ReplayMismatch::ParamsDiffer => {
                write!(f, "koperta StartGame opisuje inny świat niż nagłówek")
            }
            ReplayMismatch::RejectionDiffers {
                seq,
                recorded,
                replayed,
            } => write!(
                f,
                "komenda {seq}: nagrano {recorded:?}, odtworzono {replayed:?}"
            ),
            ReplayMismatch::Build(e) => write!(f, "nie udało się odtworzyć świata: {e}"),
        }
    }
}

impl std::error::Error for ReplayMismatch {}

/// Odtwarza sesję z dziennika: buduje świat z parametrów i przewija `ticks`
/// ticków, stosując koperty na ich własnych tickach.
///
/// Zwraca sesję i łańcuch hashy co `hash_every` — porównanie z nagranym łańcuchem
/// należy do wołającego, bo to on wie, z czym porównuje.
///
/// # Errors
/// [`ReplayMismatch`] — niespójny dziennik, błąd budowy świata albo **inne
/// odrzucenie komendy niż przy nagraniu**.
pub fn replay(
    log: &ReplayLog,
    ticks: u64,
    hash_every: u64,
    pool: &JobPool,
) -> Result<(Session, Vec<(u64, StateHash)>), ReplayMismatch> {
    let params = log.header.params;
    let Some(pierwsza) = log.commands.first() else {
        return Err(ReplayMismatch::NoStartGame);
    };
    match &pierwsza.cmd {
        PlayerCommand::StartGame { world, .. } if *world == params.world => {}
        PlayerCommand::StartGame { .. } => return Err(ReplayMismatch::ParamsDiffer),
        _ => return Err(ReplayMismatch::NoStartGame),
    }

    let built = crate::world::population::zbuduj_z_params(
        params.world,
        pool,
        &crate::shell::GenWatch::none(),
    )
    .map_err(|e| ReplayMismatch::Build(e.to_string()))?
    .ok_or_else(|| ReplayMismatch::Build("generacja anulowana".to_string()))?;

    let standing = stand_up(&built, params.world.seed, params.opts, pool)
        .map_err(|e| ReplayMismatch::Build(e.to_string()))?;
    let mut s = Session {
        app: standing.app,
        market: standing.market,
        built,
        report: standing.report,
        log: ReplayLog::new(params),
        pending: log.commands.clone(),
        next_seq: log.commands.len() as u64,
        played_ms: 0,
        player: None,
        metrics: crate::metrics::MetricsRecorder::default(),
        chronicle: crate::chronicle::Chronicle::default(),
        scenario: None,
        scenario_state: crate::scenario::ScenarioState::default(),
        fresh_objectives: Vec::new(),
    };
    s.log.commands = log.commands.clone();
    s.load_scenario(params.scenario);

    let hashe = s.run_hashing(ticks, hash_every);

    // Odrzucenia muszą się zgadzać co do sztuki i co do powodu.
    let nagrane =
        |seq: u64, v: &[Rejected]| v.iter().find(|r| r.seq == seq).map(|r| r.error.clone());
    for e in &log.commands {
        let a = nagrane(e.seq, &log.rejected);
        let b = nagrane(e.seq, &s.log.rejected);
        if a != b {
            return Err(ReplayMismatch::RejectionDiffers {
                seq: e.seq,
                recorded: a,
                replayed: b,
            });
        }
    }
    Ok((s, hashe))
}
