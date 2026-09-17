//! Kalibracja usług publicznych, urzędów i egzekucji (M8d, `data/tuning/city.ron`).
//!
//! Osobny plik danych od `data/city/tax.ron` i osobny loader, bo to są dwie różne
//! klasy liczb (`K-35`): tam siedzi stawka, która zmienia cenę widzianą przez gracza,
//! tutaj próg, przy którym urząd otwiera sprawę. Pierwsze jest kształtem modelu,
//! drugie jego kalibracją — i tylko drugie wolno przestawić balansatorem.
//!
//! Kształt loadera jest ten sam co przy [`crate::code::TaxCode`]: stała
//! `*_SCHEMA_VERSION`, jawny `enum` błędu i walidacja **kompletności przed użyciem**.
//! Brak wpisu dla rodzaju usługi jest błędem ładowania, nie cichym zerem — placówka
//! bez normy finansowania miałaby jakość zero i wyglądałaby jak zamknięta.

use magnat_core::{Money, PermitKind, ServiceKind, SERVICE_KIND_COUNT};
use serde::Deserialize;

pub const CITY_TUNING_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum TuningError {
    Io(String),
    Parse(String),
    Schema {
        found: u32,
        want: u32,
    },
    /// Rodzaj usługi bez wpisu w tabeli — nazwa mówi, w której.
    Missing {
        table: &'static str,
        kind: &'static str,
    },
    /// Wagi jakości nie sumują się do stu.
    BadWeights(u32),
}

impl std::fmt::Display for TuningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TuningError::Io(e) => write!(f, "tuning/city.ron: {e}"),
            TuningError::Parse(e) => write!(f, "tuning/city.ron: {e}"),
            TuningError::Schema { found, want } => {
                write!(
                    f,
                    "tuning/city.ron: schema_version {found}, oczekiwano {want}"
                )
            }
            TuningError::Missing { table, kind } => {
                write!(f, "tuning/city.ron: brak wpisu `{kind}` w tabeli `{table}`")
            }
            TuningError::BadWeights(s) => {
                write!(
                    f,
                    "tuning/city.ron: wagi jakości sumują się do {s}, a mają do 100"
                )
            }
        }
    }
}

impl std::error::Error for TuningError {}

#[derive(Clone, Copy, Debug, Deserialize)]
struct KindGr {
    kind: ServiceKind,
    gr: i64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct KindBp {
    kind: ServiceKind,
    bp: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct QualityParams {
    pub w_funding: u32,
    pub w_staff: u32,
    pub w_condition: u32,
    pub w_load: u32,
    funding_ref_gr_per_capita: Vec<KindGr>,
    pub condition_decay_per_year: u8,
    pub condition_repair_per_year: u8,
    pub load_zero_bp: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PermitParams {
    pub units_per_clerk_day: u32,
    pub cost_units: Vec<u32>,
    pub fee_gr: Vec<i64>,
    pub expire_days: u32,
}

impl PermitParams {
    /// Koszt wniosku w jednostkach przerobu urzędu. Rodzaj spoza tabeli kosztuje
    /// jedną jednostkę — tabela krótsza od enuma jest błędem danych, którego pilnuje
    /// walidator, więc ten wariant w symulacji nie występuje.
    #[must_use]
    pub fn cost(&self, kind: PermitKind) -> u32 {
        self.cost_units.get(kind.as_index()).copied().unwrap_or(1)
    }

    #[must_use]
    pub fn fee(&self, kind: PermitKind) -> Money {
        Money(self.fee_gr.get(kind.as_index()).copied().unwrap_or(0))
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ShadowParams {
    pub step_bp: u16,
    pub relief_bp: u16,
    pub ceiling_bp: u16,
    pub distress_bp: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AgencyParams {
    pub evidence_to_close: u8,
    pub evidence_per_inspector_day: u8,
    pub case_expire_days: u32,
    pub fine_bp: u32,
    pub back_tax_penalty_bp: u32,
    pub sanitary_expired_g: i64,
    pub sanitary_closure_days: u32,
    pub wage_floor_bp: u16,
    pub antitrust_share_bp: u32,
    pub emission_permille: u32,
}

/// Cała kalibracja M8d w jednej strukturze — jeden plik, jeden typ, jeden loader.
#[derive(Clone, Debug, Deserialize)]
pub struct CityTuning {
    pub schema_version: u32,
    pub quality: QualityParams,
    spillover_bp: Vec<KindBp>,
    pub permits: PermitParams,
    pub shadow: ShadowParams,
    pub agencies: AgencyParams,
    pub shrinkage_base_bp: u32,

    /// Rozwinięte tabele per rodzaj usługi — walidator wypełnia je przy ładowaniu,
    /// żeby odczyt w pętli miesięcznej był indeksem, a nie wyszukiwaniem.
    #[serde(skip)]
    funding_ref: [i64; SERVICE_KIND_COUNT],
    #[serde(skip)]
    spillover: [u32; SERVICE_KIND_COUNT],
}

impl CityTuning {
    pub fn load(path: &std::path::Path) -> Result<CityTuning, TuningError> {
        let tekst = std::fs::read_to_string(path).map_err(|e| TuningError::Io(e.to_string()))?;
        let mut t: CityTuning =
            ron::from_str(&tekst).map_err(|e| TuningError::Parse(e.to_string()))?;
        if t.schema_version != CITY_TUNING_SCHEMA_VERSION {
            return Err(TuningError::Schema {
                found: t.schema_version,
                want: CITY_TUNING_SCHEMA_VERSION,
            });
        }
        let q = &t.quality;
        let suma = q.w_funding + q.w_staff + q.w_condition + q.w_load;
        if suma != 100 {
            return Err(TuningError::BadWeights(suma));
        }
        let mut funding = [0i64; SERVICE_KIND_COUNT];
        let mut spill = [u32::MAX; SERVICE_KIND_COUNT];
        for w in &t.quality.funding_ref_gr_per_capita {
            funding[w.kind.as_index()] = w.gr;
        }
        for w in &t.spillover_bp {
            spill[w.kind.as_index()] = w.bp;
        }
        for k in ServiceKind::ALL {
            if funding[k.as_index()] <= 0 {
                return Err(TuningError::Missing {
                    table: "funding_ref_gr_per_capita",
                    kind: k.name(),
                });
            }
            // Brak wpisu **i** wpis spoza skali idą tą samą ścieżką: zanik powyżej
            // stu procent nie ma znaczenia, a po przemnożeniu przez jakość i rzucie
            // na `u8` dałby zawinięcie — czyli losową jakość pokrycia zamiast błędu.
            if spill[k.as_index()] == u32::MAX || spill[k.as_index()] > 10_000 {
                return Err(TuningError::Missing {
                    table: "spillover_bp",
                    kind: k.name(),
                });
            }
        }
        t.funding_ref = funding;
        t.spillover = spill;
        Ok(t)
    }

    pub fn load_default() -> Result<CityTuning, TuningError> {
        CityTuning::load(&magnat_core::data_path("tuning/city.ron"))
    }

    /// Pełne finansowanie tej usługi w groszach na obsługiwaną osobę i miesiąc.
    #[must_use]
    pub fn funding_ref(&self, kind: ServiceKind) -> i64 {
        self.funding_ref[kind.as_index()]
    }

    /// Ile jakości placówki dociera poza jej dzielnicę, w punktach bazowych.
    #[must_use]
    pub fn spillover(&self, kind: ServiceKind) -> u32 {
        self.spillover[kind.as_index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plik_domyslny_sie_laduje_i_jest_kompletny() {
        // Kompletność jest tu warunkiem działania, nie estetyką: rodzaj usługi bez
        // normy finansowania miałby jakość zero i wyglądałby jak placówka zamknięta.
        let t = CityTuning::load_default().expect("data/tuning/city.ron");
        for k in ServiceKind::ALL {
            assert!(t.funding_ref(*k) > 0, "{}", k.name());
            assert!(t.spillover(*k) <= 10_000, "{}", k.name());
        }
        assert_eq!(t.permits.cost_units.len(), PermitKind::ALL.len());
        assert_eq!(t.permits.fee_gr.len(), PermitKind::ALL.len());
    }
}
