//! Strojenie sieci przesyłowych — `data/tuning/grid.ron` (`K-35`).
//!
//! Katalog `data/tuning/` jest wspólny dla faz i niesie liczby, **które wolno
//! przestawić bez zmiany znaczenia modelu**. Taryfa, kształt doby i czas naprawy
//! spełniają to kryterium: zmieniają, jak mocno sieć boli, a nie czym jest.

use std::path::Path;

use magnat_core::{assets::data_path, Money, UtilityService};
use serde::Deserialize;

use super::solve::{ProfileTable, RepairWindow};
use super::Tariff;

pub const GRID_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum GridDataError {
    Io(std::io::Error),
    Ron(String),
    Schema { found: u32, want: u32 },
    /// Nieznany klucz medium w sekcji `tariffs`. Cicha zaślepka byłaby gorsza:
    /// sieć z taryfą zero wygląda w raporcie tak samo jak sieć, której nikt
    /// nie fakturuje (`R2`).
    UnknownService(String),
    /// Profil doby nie ma 24 wpisów. Uzupełnienie brakujących zerami dałoby
    /// miasto, które gaśnie o godzinie, której nikt nie wpisał.
    BadProfile { name: &'static str, found: usize },
    /// Widełki naprawy bez sensu: górna poniżej dolnej albo dolna równa zeru.
    /// Naprawa w zero minut znaczy krawędź wracającą w tym samym ticku,
    /// w którym wypadła — czyli kaskadę, która nie ma jak się zatrzymać.
    BadRepair { min: u32, max: u32 },
}

impl std::fmt::Display for GridDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GridDataError::Io(e) => write!(f, "data/tuning/grid.ron: {e}"),
            GridDataError::Ron(e) => write!(f, "data/tuning/grid.ron: {e}"),
            GridDataError::Schema { found, want } => {
                write!(f, "data/tuning/grid.ron: schema_version {found}, oczekiwano {want}")
            }
            GridDataError::UnknownService(k) => {
                write!(f, "data/tuning/grid.ron: nieznane medium „{k}”")
            }
            GridDataError::BadProfile { name, found } => {
                write!(f, "data/tuning/grid.ron: {name} ma {found} wpisów zamiast 24")
            }
            GridDataError::BadRepair { min, max } => write!(
                f,
                "data/tuning/grid.ron: repair_minutes ({min}..{max}) — dolna musi być ≥ 1 i ≤ górnej"
            ),
        }
    }
}

impl std::error::Error for GridDataError {}

impl From<std::io::Error> for GridDataError {
    fn from(e: std::io::Error) -> GridDataError {
        GridDataError::Io(e)
    }
}

#[derive(Deserialize)]
struct RepairRow {
    min: u32,
    max: u32,
}

#[derive(Deserialize)]
struct TariffRow {
    service: String,
    per_unit_gr: i64,
    standing_gr_per_month: i64,
}

#[derive(Deserialize)]
struct GridFile {
    schema_version: u32,
    profile_household: Vec<u32>,
    profile_industry: Vec<u32>,
    repair_minutes: RepairRow,
    tariffs: Vec<TariffRow>,
    household_w: i64,
    household_water_ml_h: i64,
    plant_water_ml_h: i64,
    priority_critical: u8,
    priority_household: u8,
    priority_industry: u8,
    source_margin_bps: u32,
    spur_capacity_bps: u32,
    backbone_capacity_bps: u32,
    loss_bps_backbone: u32,
    loss_bps_spur: u32,
}

/// Wszystko, co sieć czyta z danych. Taryfy indeksowane wariantem
/// [`UtilityService`], więc medium bez wiersza w pliku ma taryfę zerową
/// i nie udaje, że ktoś je fakturuje.
#[derive(Clone, Copy, Debug)]
pub struct GridTuning {
    pub profiles: ProfileTable,
    pub repair: RepairWindow,
    tariffs: [Tariff; 7],
    pub household_w: i64,
    pub household_water_ml_h: i64,
    pub plant_water_ml_h: i64,
    pub priority_critical: u8,
    pub priority_household: u8,
    pub priority_industry: u8,
    pub source_margin_bps: u32,
    pub spur_capacity_bps: u32,
    pub backbone_capacity_bps: u32,
    pub loss_bps_backbone: u32,
    pub loss_bps_spur: u32,
}

const ZERO_TARIFF: Tariff = Tariff {
    standing_charge_per_month: Money::ZERO,
    per_unit: Money::ZERO,
};

impl Default for GridTuning {
    /// Sieć bez pliku: płaski popyt, zerowa taryfa, naprawa jak w [`RepairWindow`].
    /// Używają tego wyłącznie testy jednostkowe solvera, którym dane są obojętne.
    fn default() -> GridTuning {
        GridTuning {
            profiles: ProfileTable::default(),
            repair: RepairWindow::default(),
            tariffs: [ZERO_TARIFF; 7],
            household_w: 0,
            household_water_ml_h: 0,
            plant_water_ml_h: 0,
            priority_critical: 0,
            priority_household: 2,
            priority_industry: 3,
            source_margin_bps: 11_500,
            spur_capacity_bps: 12_500,
            backbone_capacity_bps: 30_000,
            loss_bps_backbone: 90,
            loss_bps_spur: 210,
        }
    }
}

impl GridTuning {
    /// # Errors
    /// Gdy pliku nie ma, nie parsuje się, ma inną wersję schematu albo wymienia
    /// medium spoza słownika `UtilityService`.
    pub fn load_default() -> Result<GridTuning, GridDataError> {
        GridTuning::load(&data_path("tuning/grid.ron"))
    }

    /// # Errors
    /// Jak wyżej.
    pub fn load(path: &Path) -> Result<GridTuning, GridDataError> {
        let txt = std::fs::read_to_string(path)?;
        let f: GridFile = ron::from_str(&txt).map_err(|e| GridDataError::Ron(e.to_string()))?;
        if f.schema_version != GRID_SCHEMA_VERSION {
            return Err(GridDataError::Schema {
                found: f.schema_version,
                want: GRID_SCHEMA_VERSION,
            });
        }
        if f.repair_minutes.min == 0 || f.repair_minutes.max < f.repair_minutes.min {
            return Err(GridDataError::BadRepair {
                min: f.repair_minutes.min,
                max: f.repair_minutes.max,
            });
        }
        let household = doba(&f.profile_household, "profile_household")?;
        let industry = doba(&f.profile_industry, "profile_industry")?;
        let mut tariffs = [ZERO_TARIFF; 7];
        for row in &f.tariffs {
            let s = UtilityService::ALL
                .iter()
                .find(|s| s.name() == row.service)
                .ok_or_else(|| GridDataError::UnknownService(row.service.clone()))?;
            tariffs[s.as_index()] = Tariff {
                standing_charge_per_month: Money(row.standing_gr_per_month),
                per_unit: Money(row.per_unit_gr),
            };
        }
        Ok(GridTuning {
            profiles: ProfileTable {
                household,
                industry,
            },
            repair: RepairWindow {
                min_minutes: f.repair_minutes.min,
                max_minutes: f.repair_minutes.max,
            },
            tariffs,
            household_w: f.household_w,
            household_water_ml_h: f.household_water_ml_h,
            plant_water_ml_h: f.plant_water_ml_h,
            priority_critical: f.priority_critical,
            priority_household: f.priority_household,
            priority_industry: f.priority_industry,
            source_margin_bps: f.source_margin_bps,
            spur_capacity_bps: f.spur_capacity_bps,
            backbone_capacity_bps: f.backbone_capacity_bps,
            loss_bps_backbone: f.loss_bps_backbone,
            loss_bps_spur: f.loss_bps_spur,
        })
    }

    /// Taryfa operatora danego medium.
    #[must_use]
    pub fn tariff(&self, service: UtilityService) -> Tariff {
        self.tariffs[service.as_index()]
    }

    /// Czy to medium ma w tym mieście operatora z cennikiem. Sieć bez taryfy
    /// nie ma po co powstawać — nikt by za nią nie zapłacił.
    #[must_use]
    pub fn has_tariff(&self, service: UtilityService) -> bool {
        self.tariff(service).per_unit.get() > 0
    }
}

fn doba(v: &[u32], name: &'static str) -> Result<[u32; 24], GridDataError> {
    <[u32; 24]>::try_from(v).map_err(|_| GridDataError::BadProfile {
        name,
        found: v.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plik z repozytorium ładuje się i niesie taryfy obu mediów, które mają
    /// w tej grze licznik. Sieć bez taryfy byłaby siecią, za którą nikt nie płaci.
    #[test]
    fn strojenie_laduje_sie_z_repozytorium() {
        let t = GridTuning::load_default().expect("data/tuning/grid.ron");
        assert!(t.has_tariff(UtilityService::Electricity));
        assert!(t.has_tariff(UtilityService::Water));
        assert!(t.tariff(UtilityService::Electricity).standing_charge_per_month > Money::ZERO);
        // Szczyt wieczorny gospodarstw jest wyższy od nocy — inaczej profil
        // nie miałby kształtu i nie byłoby po co go wczytywać.
        assert!(t.profiles.household[19] > t.profiles.household[3]);
        assert!(t.profiles.industry[9] > t.profiles.industry[2]);
        assert!(t.repair.max_minutes > t.repair.min_minutes);
    }
}
