//! Sloty zapisu — nagłówek czytany bez wczytywania świata (M9a §5.13).
//!
//! Lista dziesięciu slotów **nie ma prawa wczytać dziesięciu światów**, więc
//! nagłówek jest osobnym plikiem obok właściwego zapisu: `slot-3.meta.ron`
//! (kilkaset bajtów) i `slot-3.replay.ron` (dziennik wejść). Slot w niezgodnej
//! wersji schematu zostaje na liście z opisanym błędem — ukryty zapis czyta się
//! jako utracona gra.
//!
//! Czym jest właściwy zapis w M9a, mówi [`crate::replay`]: ziarnem i wejściami.

use magnat_core::{Money, SimMinute};
use magnat_world::WorldGenParams;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::replay::{ReplayError, ReplayLog};

/// Wersja formatu nagłówka slotu.
pub const SAVE_SCHEMA_VERSION: u16 = 1;

/// Ile slotów pokazuje lista.
pub const SLOTS: u8 = 10;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SaveSlot {
    pub id: u8,
    pub city: String,
    /// Data gry — minuta świata.
    pub game_date: SimMinute,
    pub net_worth: Money,
    pub played_secs: u32,
    pub world: WorldGenParams,
    pub schema_version: u16,
    /// Czas rzeczywisty zapisu (sekundy uniksowe) — **metadana pliku**, nie stanu.
    pub saved_at_wall: u64,
}

#[derive(Debug)]
pub enum SaveError {
    Io(String),
    Parse(String),
    SchemaTooOld { found: u16, supported: u16 },
    SchemaTooNew { found: u16, supported: u16 },
    Replay(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "błąd wejścia/wyjścia: {e}"),
            SaveError::Parse(e) => write!(f, "nagłówek slotu nie daje się odczytać: {e}"),
            SaveError::SchemaTooOld { found, supported } => write!(
                f,
                "zapis w wersji {found}, obsługiwana jest {supported} — zapis jest starszy niż gra"
            ),
            SaveError::SchemaTooNew { found, supported } => write!(
                f,
                "zapis w wersji {found}, obsługiwana jest {supported} — zapis jest nowszy niż gra"
            ),
            SaveError::Replay(e) => write!(f, "dziennik slotu: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<ReplayError> for SaveError {
    fn from(e: ReplayError) -> SaveError {
        SaveError::Replay(e.to_string())
    }
}

#[derive(Deserialize)]
struct WersjaSondy {
    schema_version: u16,
}

#[must_use]
pub fn meta_path(dir: &Path, id: u8) -> PathBuf {
    dir.join(format!("slot-{id}.meta.ron"))
}

#[must_use]
pub fn replay_path(dir: &Path, id: u8) -> PathBuf {
    dir.join(format!("slot-{id}.replay.ron"))
}

/// Zapisuje slot: nagłówek i dziennik.
///
/// # Errors
/// Błąd zapisu któregokolwiek z dwóch plików.
pub fn write_slot(dir: &Path, slot: &SaveSlot, log: &ReplayLog) -> Result<(), SaveError> {
    std::fs::create_dir_all(dir).map_err(|e| SaveError::Io(e.to_string()))?;
    let s = ron::ser::to_string_pretty(slot, ron::ser::PrettyConfig::default())
        .map_err(|e| SaveError::Parse(e.to_string()))?;
    std::fs::write(meta_path(dir, slot.id), s).map_err(|e| SaveError::Io(e.to_string()))?;
    log.save(&replay_path(dir, slot.id))?;
    Ok(())
}

/// Czyta **wyłącznie nagłówek** slotu.
///
/// # Errors
/// Brak pliku, niezgodna wersja schematu albo błąd składni.
pub fn read_header(dir: &Path, id: u8) -> Result<SaveSlot, SaveError> {
    let s =
        std::fs::read_to_string(meta_path(dir, id)).map_err(|e| SaveError::Io(e.to_string()))?;
    let probe: WersjaSondy = ron::from_str(&s).map_err(|e| SaveError::Parse(e.to_string()))?;
    if probe.schema_version < SAVE_SCHEMA_VERSION {
        return Err(SaveError::SchemaTooOld {
            found: probe.schema_version,
            supported: SAVE_SCHEMA_VERSION,
        });
    }
    if probe.schema_version > SAVE_SCHEMA_VERSION {
        return Err(SaveError::SchemaTooNew {
            found: probe.schema_version,
            supported: SAVE_SCHEMA_VERSION,
        });
    }
    ron::from_str(&s).map_err(|e| SaveError::Parse(e.to_string()))
}

/// Lista slotów. Pusty slot to `None`, niezgodny — `Some(Err(..))`: zostaje
/// widoczny, bo zapis, który zniknął z listy, czyta się jako utracona gra.
#[must_use]
pub fn list_slots(dir: &Path) -> Vec<(u8, Option<Result<SaveSlot, SaveError>>)> {
    (0..SLOTS)
        .map(|id| {
            if meta_path(dir, id).exists() {
                (id, Some(read_header(dir, id)))
            } else {
                (id, None)
            }
        })
        .collect()
}

/// Czyta dziennik slotu — to jest właściwy zapis (patrz [`crate::replay`]).
///
/// # Errors
/// Brak pliku albo niezgodna wersja dziennika.
pub fn read_log(dir: &Path, id: u8) -> Result<ReplayLog, SaveError> {
    Ok(ReplayLog::load(&replay_path(dir, id))?)
}
