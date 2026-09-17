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
    apply, precheck, CommandEnvelope, CommandError, CommandView, PlayerCommand, PlayerId,
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
/// Wariantów jest cztery, a nie siedem: `CharacterSelect` przychodzi z WP4 (`M9c`),
/// `Succession` i `ScenarioEnd` z WP12 (`M9e`) — razem ze swoją treścią. Wariant
/// stanu, w który nie da się wejść, wygląda tak samo jak działający (`K-67`).
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
    Playing(Box<Session>),
}

impl GameState {
    #[must_use]
    pub fn is_playing(&self) -> bool {
        matches!(self, GameState::Playing(_))
    }

    #[must_use]
    pub fn session(&self) -> Option<&Session> {
        match self {
            GameState::Playing(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        match self {
            GameState::Playing(s) => Some(s),
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
            played_ms: 0,
        };
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
        }
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
    pub fn step(&mut self, minutes: u32, dt_ms: u64) {
        self.played_ms += dt_ms;
        for _ in 0..minutes {
            let t = Tick(self.app.world.tick.get() + 1);
            self.apply_due(t);
            self.app.tick();
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
            let widok = CommandView {
                market: self.market.as_ref(),
            };
            if let Err(err) = apply(&widok, &e.cmd) {
                self.log.rejected.push(Rejected {
                    seq: e.seq,
                    error: err,
                });
            }
        }
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
    };
    s.log.commands = log.commands.clone();

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
