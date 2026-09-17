//! Dziennik wejść gracza i jego odtwarzacz (M9a, WP2; PRD §18.2).
//!
//! # Co jest zapisem gry w M9a
//!
//! **Ziarno i wejścia.** Świat powstaje z `WorldGenParams` deterministycznie
//! (M1 §6.1), a wszystko, co zrobił gracz, jest w tym dzienniku — więc para
//! „nagłówek + koperty" wystarcza, żeby odtworzyć sesję co do grosza. Dokładnie
//! to obiecuje §1 pkt 9 dokumentu fazy: „wyeksportować replay (seed + wejścia),
//! który u kogoś innego odtworzy tę samą grę".
//!
//! Cena jest jawna i trzeba ją nazwać: **wczytanie slotu kosztuje tyle, ile
//! kosztowała rozgrywka**, bo odtwarzanie przewija ticki. Migawka stanu, która
//! wczytuje się w sekundę, wymaga serializacji zasobów świata (rynek, księgi,
//! rejestr firm, miasto) — a tego `engine/io` dziś nie umie i jest to zakres M12
//! („pełne wersjonowanie zapisu", M9 §6). Do tego czasu slot jest dziennikiem.
//!
//! # Odrzucone komendy też są w dzienniku
//!
//! Replay musi odrzucić je identycznie, i to jest osobny test (§5.5). Wpis niesie
//! więc także powód odrzucenia — gdyby odtworzenie odrzuciło komendę z innego
//! powodu albo jej nie odrzuciło, wyszłoby to od razu, a nie dopiero jako inny
//! hash tysiąc ticków później.

use magnat_core::Tick;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::command::{CommandEnvelope, CommandError, ViewRecord};
use crate::shell::NewGameParams;

/// Wersja formatu dziennika. Zmiana kształtu koperty zmienia tę liczbę.
pub const REPLAY_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReplayHeader {
    pub schema_version: u16,
    /// Wersja gry, która nagrała dziennik — pierwsza rzecz, o którą pyta się
    /// przy zgłoszeniu błędu.
    pub engine_build: String,
    /// Komplet parametrów założenia gry, razem z nastawami przebiegu.
    pub params: NewGameParams,
}

/// Odrzucenie zapisane w dzienniku.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Rejected {
    pub seq: u64,
    pub error: CommandError,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReplayLog {
    pub header: ReplayHeader,
    /// Strumień autorytatywny — **wszystkie** koperty, także te, które świat
    /// odrzucił. Replay ma je odrzucić tak samo, więc musi je zobaczyć.
    pub commands: Vec<CommandEnvelope>,
    /// Które z nich odrzucono i dlaczego, po `seq` koperty.
    pub rejected: Vec<Rejected>,
    /// Strumień widoku — nie wchodzi do hasha i wolno go wyłączyć w ustawieniach
    /// (decyzja otwarta nr 7 fazy).
    pub view: Vec<ViewRecord>,
}

#[derive(Debug)]
pub enum ReplayError {
    Io(String),
    Parse(String),
    SchemaTooOld { found: u16, supported: u16 },
    SchemaTooNew { found: u16, supported: u16 },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::Io(e) => write!(f, "błąd wejścia/wyjścia: {e}"),
            ReplayError::Parse(e) => write!(f, "dziennik nie daje się odczytać: {e}"),
            ReplayError::SchemaTooOld { found, supported } => write!(
                f,
                "dziennik w wersji {found}, obsługiwana jest {supported} — zapis jest starszy niż gra"
            ),
            ReplayError::SchemaTooNew { found, supported } => write!(
                f,
                "dziennik w wersji {found}, obsługiwana jest {supported} — zapis jest nowszy niż gra"
            ),
        }
    }
}

impl std::error::Error for ReplayError {}

/// Sonda wersji: czyta **wyłącznie** numer schematu.
///
/// Osobny typ, bo pełny odczyt pękłby na zmienionym kształcie koperty, a wtedy
/// gracz zobaczyłby „błąd składni" zamiast „ten zapis jest z innej wersji gry".
#[derive(Deserialize)]
struct WersjaSondy {
    header: NaglowekSondy,
}

#[derive(Deserialize)]
struct NaglowekSondy {
    schema_version: u16,
}

impl ReplayLog {
    #[must_use]
    pub fn new(params: NewGameParams) -> ReplayLog {
        ReplayLog {
            header: ReplayHeader {
                schema_version: REPLAY_SCHEMA_VERSION,
                engine_build: env!("CARGO_PKG_VERSION").to_string(),
                params,
            },
            commands: Vec::new(),
            rejected: Vec::new(),
            view: Vec::new(),
        }
    }

    /// Ostatni tick, dla którego cokolwiek zapisano — meta dziennika, nie stanu.
    #[must_use]
    pub fn last_tick(&self) -> Tick {
        Tick(
            self.commands
                .iter()
                .map(|c| c.tick.get())
                .max()
                .unwrap_or(0),
        )
    }

    /// # Errors
    /// Błąd serializacji albo zapisu pliku.
    pub fn save(&self, path: &Path) -> Result<(), ReplayError> {
        let s = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| ReplayError::Parse(e.to_string()))?;
        std::fs::write(path, s).map_err(|e| ReplayError::Io(e.to_string()))
    }

    /// # Errors
    /// Brak pliku, niezgodna wersja schematu albo błąd składni.
    pub fn load(path: &Path) -> Result<ReplayLog, ReplayError> {
        let s = std::fs::read_to_string(path).map_err(|e| ReplayError::Io(e.to_string()))?;
        Self::parse_ron(&s)
    }

    /// # Errors
    /// Jak [`ReplayLog::load`], bez błędu wejścia/wyjścia.
    pub fn parse_ron(s: &str) -> Result<ReplayLog, ReplayError> {
        let probe: WersjaSondy = ron::from_str(s).map_err(|e| ReplayError::Parse(e.to_string()))?;
        let found = probe.header.schema_version;
        if found < REPLAY_SCHEMA_VERSION {
            return Err(ReplayError::SchemaTooOld {
                found,
                supported: REPLAY_SCHEMA_VERSION,
            });
        }
        if found > REPLAY_SCHEMA_VERSION {
            return Err(ReplayError::SchemaTooNew {
                found,
                supported: REPLAY_SCHEMA_VERSION,
            });
        }
        ron::from_str(s).map_err(|e| ReplayError::Parse(e.to_string()))
    }
}
