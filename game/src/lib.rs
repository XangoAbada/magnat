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

pub mod command;
pub mod inspect;
pub mod overlays;
pub mod player;
pub mod policy;
pub mod replay;
pub mod save;
pub mod screens;
pub mod session;
pub mod shell;
pub mod world;

pub use command::{
    apply, precheck, CommandEnvelope, CommandError, CommandView, PlayerCommand, PlayerId,
    ViewCommand, ViewRecord,
};
pub use overlays::{EntityFilter, OverlayField, OverlayField2d};
pub use policy::{dry_run, DrySummary, Edit, EditError, GoodKeys, Note, RuleEditor};
pub use player::{
    AutonomyField, Candidate, Control, PlayerAutonomy, PlayerCharacter, StartVariant,
    CANDIDATES_SHOWN,
};
pub use replay::{ReplayError, ReplayHeader, ReplayLog, REPLAY_SCHEMA_VERSION};
pub use save::{SaveError, SaveSlot, SAVE_SCHEMA_VERSION};
pub use screens::{Shell, ShellAction};
pub use session::{replay as replay_session, GameState, ReplayMismatch, Session};
pub use shell::{
    GenProgress, GenWatch, NewGameParams, ScenarioId, Settings, ShellScreen, WorldGenJob,
    WorldPreview,
};
pub use world::{stand_up, BuiltCity, SessionOpts, Standing, StandingReport};
