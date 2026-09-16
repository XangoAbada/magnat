//! Parametry niewypłacalności z `data/tuning/insolvency.ron` (M7d §5.13).
//!
//! Osobno od rejestru, bo to jest **kalibracja, nie model** (`K-35`): progi, dyskonta
//! rund i wynagrodzenie syndyka wolno przestawić bez zmiany znaczenia postępowania,
//! a kolejność zaspokojenia — nie, i dlatego ona siedzi w `ClaimPriority` w `core`.
//!
//! Walidacja jest tu ostra z rozmysłu: plik, w którym rund jest więcej niż dyskont,
//! **zatrzymuje ładowanie**. Postępowanie, w którym trzecia runda ma po cichu cenę
//! drugiej, wygląda na działające i zmienia wynik każdej upadłości w mieście.

use std::fmt;
use std::path::Path;

use serde::Deserialize;

use magnat_core::Money;

use crate::corpfin::instruments::AssetKind;

/// Wersja schematu `data/tuning/insolvency.ron`.
pub const INSOLVENCY_SCHEMA_VERSION: u32 = 1;

// ── parametry z danych ───────────────────────────────────────────────────────────

/// Kiedy firma jest niewypłacalna (`data/tuning/insolvency.ron`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct TriggerParams {
    pub illiquid_days: u16,
    pub illiquid_min_gr: i64,
    pub negative_equity_arrears_months: u8,
}

/// Jak przebiega wyprzedaż masy.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct EstateParams {
    pub trustee_fee_bp: u16,
    pub rounds: u8,
    pub round_days: u16,
    pub round_discount_bp: Vec<i64>,
    pub scrap_bp: i64,
}

impl EstateParams {
    /// Dyskonto rundy. Runda spoza listy dostaje ostatnie dyskonto — lista jest
    /// walidowana przy ładowaniu, więc to jest gałąź obronna, a nie zachowanie.
    #[must_use]
    pub fn discount(&self, round: u8) -> i64 {
        self.round_discount_bp
            .get(round as usize)
            .copied()
            .or_else(|| self.round_discount_bp.last().copied())
            .unwrap_or(0)
    }
}

/// Wycena masy.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct ValuationParams {
    pub equipment_bp: i64,
    pub inventory_bp: i64,
}

impl ValuationParams {
    #[must_use]
    pub fn of(&self, kind: AssetKind, book_value: Money) -> Money {
        let bp = match kind {
            AssetKind::Equipment => self.equipment_bp,
            AssetKind::Inventory => self.inventory_bp,
        };
        book_value.mul_ratio(bp, 10_000)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct InsolvencyParams {
    pub trigger: TriggerParams,
    pub estate: EstateParams,
    pub valuation: ValuationParams,
}

#[derive(Deserialize)]
struct InsolvencyFile {
    schema_version: u32,
    trigger: TriggerParams,
    estate: EstateParams,
    valuation: ValuationParams,
}

#[derive(Debug)]
pub enum InsolvencyError {
    Io(std::io::Error),
    Parse(ron::error::SpannedError),
    Schema {
        found: u32,
        want: u32,
    },
    /// Lista dyskont nie pokrywa zadeklarowanej liczby rund. To **zatrzymuje
    /// ładowanie**: postępowanie, w którym trzecia runda ma cenę drugiej, wygląda
    /// na działające i cicho zmienia wynik każdej upadłości w mieście.
    RoundsMismatch {
        rounds: u8,
        discounts: usize,
    },
    /// Dyskonto poza zakresem albo nierosnące — druga runda nie może być droższa
    /// od pierwszej, bo wtedy wyprzedaż przestaje być wyprzedażą.
    BadDiscounts,
}

impl fmt::Display for InsolvencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InsolvencyError::Io(e) => write!(f, "insolvency.ron: {e}"),
            InsolvencyError::Parse(e) => write!(f, "insolvency.ron: {e}"),
            InsolvencyError::Schema { found, want } => {
                write!(f, "insolvency.ron: schema_version {found}, oczekiwano {want}")
            }
            InsolvencyError::RoundsMismatch { rounds, discounts } => write!(
                f,
                "insolvency.ron: {rounds} rund, ale {discounts} dyskont — lista musi mieć tyle wpisów, ile jest rund"
            ),
            InsolvencyError::BadDiscounts => write!(
                f,
                "insolvency.ron: dyskonta rund muszą rosnąć i mieścić się w 0..=10000"
            ),
        }
    }
}

impl std::error::Error for InsolvencyError {}

impl InsolvencyParams {
    /// Ładuje `data/tuning/insolvency.ron`.
    pub fn load_default() -> Result<InsolvencyParams, InsolvencyError> {
        InsolvencyParams::load(&magnat_core::data_path("tuning/insolvency.ron"))
    }

    pub fn load(path: &Path) -> Result<InsolvencyParams, InsolvencyError> {
        let txt = std::fs::read_to_string(path).map_err(InsolvencyError::Io)?;
        InsolvencyParams::parse(&txt)
    }

    pub fn parse(txt: &str) -> Result<InsolvencyParams, InsolvencyError> {
        let f: InsolvencyFile = ron::from_str(txt).map_err(InsolvencyError::Parse)?;
        if f.schema_version != INSOLVENCY_SCHEMA_VERSION {
            return Err(InsolvencyError::Schema {
                found: f.schema_version,
                want: INSOLVENCY_SCHEMA_VERSION,
            });
        }
        if f.estate.round_discount_bp.len() != f.estate.rounds as usize {
            return Err(InsolvencyError::RoundsMismatch {
                rounds: f.estate.rounds,
                discounts: f.estate.round_discount_bp.len(),
            });
        }
        let d = &f.estate.round_discount_bp;
        if d.iter().any(|x| !(0..=10_000).contains(x)) || d.windows(2).any(|w| w[0] > w[1]) {
            return Err(InsolvencyError::BadDiscounts);
        }
        Ok(InsolvencyParams {
            trigger: f.trigger,
            estate: f.estate,
            valuation: f.valuation,
        })
    }
}
