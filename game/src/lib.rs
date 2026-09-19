//! `magnat-game` — gra: sesja, komendy gracza, dziennik replayu i powłoka
//! (M9, crate tworzony w M9a).
//!
//! # Co to jest
//!
//! Warstwa między symulacją a graczem. Trzyma stojący świat ([`session::Session`]),
//! przyjmuje od gracza komendy ([`command::PlayerCommand`]), zapisuje je do
//! dziennika, z którego da się odtworzyć sesję co do grosza ([`replay`]), i wie,
//! jak założyć nową grę bez ani jednego argumentu wiersza poleceń ([`shell`]).
//!
//! # Czego tu nie ma
//!
//! **GPU i okna.** Crate nie zależy od `wgpu` ani od `winit`, więc przebieg
//! bezgłowy stawia dokładnie tę samą grę co klient graficzny — a nie jej kopię.
//! Rysowanie należy do `engine/render` i do klienta; widgety do `engine/ui`.
//!
//! **Panele, karta inspekcji, edytor reguł, postać gracza.** To kolejne podfazy
//! M9 (`M9b`–`M9e`). Tu jest szkielet, na którym one staną.
//!
//! # Jedna droga do świata
//!
//! [`world`] jest jedynym miejscem, w którym świat wstaje: teren, miasto, ludzie,
//! gospodarka, firmy, strona publiczna, sieci i zdarzenia. Mosty przeprowadziły się
//! tu z `tools/headless` przy starcie M9a — narzędzie testowe nie ma prawa być
//! właścicielem drogi, którą powstaje gra. `magnat-headless` reeksportuje je pod
//! starymi nazwami, więc scenariusze i testy nie drgnęły.

#![forbid(unsafe_code)]

pub mod career;
pub mod chronicle;
pub mod command;
pub mod inspect;
pub mod legacy;
pub mod metrics;
pub mod panels;
pub mod onboarding;
pub mod overlays;
pub mod player;
pub mod policy;
pub mod replay;
pub mod scenario;
pub mod save;
pub mod screens;
pub mod session;
pub mod timectl;
pub mod tutorial;
pub mod shell;
pub mod view;
pub mod world;

pub use career::{CareerTier, Holdings};
pub use chronicle::{Chronicle, ChronicleEntry, ChronicleId, ChronicleKind, ChronicleScope};
pub use command::{
    Autonomy,
    apply, precheck, CommandEnvelope, CommandError, CommandView, PlayerCommand, PlayerId,
    ViewCommand, ViewRecord,
};
pub use metrics::{MetricId, MetricsRecorder};
pub use panels::{Layout, PanelAction, PanelCtx, PanelDesc, PanelId, PanelRegistry, Panels};
pub use onboarding::{measure as measure_onboarding, Onboarding};
pub use overlays::{EntityFilter, OverlayField, OverlayField2d};
pub use policy::{dry_run, DrySummary, Edit, EditError, GoodKeys, Note, RuleEditor};
pub use player::{
    AutonomyField, Candidate, Control, PlayerAutonomy, PlayerCharacter, StartVariant,
    CANDIDATES_SHOWN,
};
pub use legacy::LifeEvent;
pub use scenario::{
    Goal, Objective, ObjectiveId, Scenario, ScenarioCatalog, ScenarioOutcome, ScenarioState,
    WorldPatch,
};
pub use replay::{ReplayError, ReplayHeader, ReplayLog, REPLAY_SCHEMA_VERSION};
pub use save::{SaveError, SaveSlot, SAVE_SCHEMA_VERSION};
pub use screens::{Shell, ShellAction};
pub use tutorial::{Progress as TutorialProgress, Step as TutorialStep, Tutorial};
pub use timectl::{
    FollowTarget, StopCondition, StopConditionId, StopHit, StopWatch, TimeScale,
};
pub use session::{replay as replay_session, GameState, ReplayMismatch, Session};
pub use shell::{
    GenProgress, GenWatch, NewGameParams, ScenarioId, Settings, ShellScreen, WorldGenJob,
    WorldPreview,
};
pub use view::SnapshotFiller;
pub use world::{stand_up, BuiltCity, SessionOpts, Standing, StandingReport};
