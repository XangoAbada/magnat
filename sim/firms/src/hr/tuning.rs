//! Strojenie rynku pracy — widok na `data/tuning/labor.ron` (M7b, `K-35`).
//!
//! Plik jest **wspólny dla dwóch crate'ów**: `sim/firms` czyta z niego reguły firmy
//! (krok licytacji, premie, odprawy), `sim/economy::labor` reguły rynku (ile ofert
//! obejrzy kandydat, jak schodzi płaca progowa). Ładowarka stoi tutaj, bo zależność
//! idzie `economy → firms` i odwrócić się nie da.
//!
//! **Czego w tym pliku nie ma: tabeli płac.** Widełki roli są w `data/jobs/roles.ron`
//! i należą do M2; tutaj są wyłącznie parametry licytacji o człowieka.

use serde::Deserialize;
use std::path::Path;

/// Wersja schematu `data/tuning/labor.ron`.
pub const LABOR_TUNING_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct SearchTuning {
    pub offers_min: u8,
    pub offers_max: u8,
    pub offer_days_valid: u16,
    pub escalate_after_days: u16,
    pub on_the_job_every_days: u16,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct WageTuning {
    pub reservation_start_bp: i32,
    pub reservation_newcomer_bp: i32,
    pub reservation_floor_bp: i32,
    pub reservation_decay_bp_per_day: i32,
    pub base_step_bp: i32,
    pub headhunt_shortage: u16,
    pub headhunt_premium_bp: i32,
    pub switch_threshold_bp: i32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct HrTuning {
    pub training_cost_bp: i32,
    pub training_gain: u8,
    pub training_every_months: u16,
    pub bonus_max_bp: i32,
    pub bonus_perf_threshold: u16,
    pub severance_days_per_year: u16,
    pub severance_days_max: u16,
    pub dismiss_perf: u16,
    pub dismiss_warnings: u8,
    pub quit_base_per_10k: u16,
    pub talent_base: u8,
    pub talent_step: u8,
}

/// Menedżerowie i delegowanie (M7c WP7).
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ManagerTuning {
    pub span_optimum: u16,
    pub span_penalty: u8,
    pub morale_weight_bp: i32,
    pub replacement_drop: u8,
    pub turnover_mult_worst: i32,
    pub turnover_mult_best: i32,
    pub headhunt_premium_bp: i32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct BenefitTuning {
    pub health_gain: u8,
    pub meals_gain: u8,
    pub car_gain: u8,
    pub health_cost_bp: i32,
    pub meals_cost_bp: i32,
    pub car_cost_bp: i32,
    pub training_cost_bp: i32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct LaborTuning {
    pub schema_version: u32,
    pub search: SearchTuning,
    pub wage: WageTuning,
    pub hr: HrTuning,
    pub manager: ManagerTuning,
    pub benefits: BenefitTuning,
}

#[derive(Debug)]
pub enum TuningError {
    Io(String),
    Parse(String),
    Schema {
        found: u32,
        want: u32,
    },
    /// Widełki, w których dolny kraniec stoi nad górnym — plik da się sparsować,
    /// ale kod losujący z takiego zakresu panikuje, więc łapiemy to przy ładowaniu.
    BadRange(&'static str),
}

impl std::fmt::Display for TuningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TuningError::Io(e) | TuningError::Parse(e) => write!(f, "data/tuning/labor.ron: {e}"),
            TuningError::Schema { found, want } => write!(
                f,
                "data/tuning/labor.ron: schema_version {found}, oczekiwano {want}"
            ),
            TuningError::BadRange(co) => {
                write!(f, "data/tuning/labor.ron: pusty przedział „{co}”")
            }
        }
    }
}

impl std::error::Error for TuningError {}

impl LaborTuning {
    pub fn load(path: &Path) -> Result<LaborTuning, TuningError> {
        let txt = std::fs::read_to_string(path).map_err(|e| TuningError::Io(e.to_string()))?;
        LaborTuning::parse(&txt)
    }

    pub fn load_default() -> Result<LaborTuning, TuningError> {
        LaborTuning::load(&magnat_core::data_path("tuning/labor.ron"))
    }

    pub fn parse(txt: &str) -> Result<LaborTuning, TuningError> {
        let t: LaborTuning = ron::from_str(txt).map_err(|e| TuningError::Parse(e.to_string()))?;
        if t.schema_version != LABOR_TUNING_SCHEMA_VERSION {
            return Err(TuningError::Schema {
                found: t.schema_version,
                want: LABOR_TUNING_SCHEMA_VERSION,
            });
        }
        if t.search.offers_min == 0 || t.search.offers_min > t.search.offers_max {
            return Err(TuningError::BadRange("search.offers"));
        }
        if t.wage.reservation_floor_bp > t.wage.reservation_start_bp {
            return Err(TuningError::BadRange("wage.reservation"));
        }
        // Mnożnik rotacji idzie **w dół** wraz z jakością zarządzania: gorszy menedżer
        // ma mieć wyższą rotację. Odwrócona para przeszłaby przez parser i dałaby
        // symulację, w której lepszy menedżer traci więcej ludzi — a to jest dokładnie
        // ten błąd, którego szuka test monotoniczności z §7.4, tylko znaleziony przy
        // ładowaniu zamiast po pięciu latach przebiegu.
        if t.manager.turnover_mult_best > t.manager.turnover_mult_worst {
            return Err(TuningError::BadRange("manager.turnover_mult"));
        }
        if t.manager.span_optimum == 0 {
            return Err(TuningError::BadRange("manager.span_optimum"));
        }
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plik_z_repozytorium_sie_laduje() {
        let t = LaborTuning::load_default().expect("data/tuning/labor.ron");
        assert!(
            t.search.offers_min >= 3,
            "PRD §17.5: co najmniej trzy oferty"
        );
        assert!(t.search.offers_max <= 15, "PRD §17.5: najwyżej piętnaście");
    }

    #[test]
    fn puste_widelki_ofert_sa_bledem() {
        let zly = std::fs::read_to_string(magnat_core::data_path("tuning/labor.ron"))
            .expect("data/tuning/labor.ron")
            .replace("offers_min: 3", "offers_min: 30");
        assert!(matches!(
            LaborTuning::parse(&zly),
            Err(TuningError::BadRange("search.offers"))
        ));
    }
}
