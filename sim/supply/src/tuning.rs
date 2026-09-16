//! Strojenie łańcucha dostaw — `data/tuning/supply.ron` (M6 `D7`, przyjęte w M6b).
//!
//! Progi kaskady, taryfy mediów, zużycie maszyn i stawki transportowe są **kalibracją,
//! nie projektem**: mają się zmieniać przebiegiem balansatora i być widoczne w diffie,
//! a nie w rekompilacji. To ten sam argument, który wypchnął diagram podstawowy ruchu
//! do `data/roads/vdf.ron` (`K-24`).
//!
//! Wszystko całkowitoliczbowe: pieniądz w groszach, masa w gramach, objętość
//! w mililitrach, czas w minutach. Zakaz floatów z 00 §2 obowiązuje także w kalibracji —
//! wartość, którą stroi balansator, wchodzi wprost do kosztu szarży.

use magnat_core::{Energy, Mass, Money, UtilityService, Volume};
use serde::Deserialize;

pub const TUNING_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ShortageTuning {
    pub buffer_minutes: u32,
    pub throttled_minutes: u32,
    pub importing_minutes: u32,
    pub min_throughput_pct: u8,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct UtilityTuning {
    pub power_gr_per_kwh: i64,
    pub gas_gr_per_kwh: i64,
    pub heat_gr_per_kwh: i64,
    pub water_gr_per_m3: i64,
    pub sewage_gr_per_m3: i64,
}

impl UtilityTuning {
    /// Taryfa za jednostkę rozliczeniową medium: kWh dla energii, m³ dla cieczy.
    #[must_use]
    pub const fn tariff(&self, kind: UtilityService) -> Money {
        Money(match kind {
            UtilityService::Electricity => self.power_gr_per_kwh,
            UtilityService::Gas => self.gas_gr_per_kwh,
            UtilityService::Heat => self.heat_gr_per_kwh,
            UtilityService::Water => self.water_gr_per_m3,
            UtilityService::Sewage => self.sewage_gr_per_m3,
            // Wywóz odpadów i internet są licznikami dopiero w M8 razem z sieciami;
            // zero znaczy „nie fakturujemy", a nie „za darmo".
            UtilityService::Waste | UtilityService::Internet => 0,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct PlantTuning {
    pub wear_minutes_per_point: u32,
    pub maintenance_minutes: u32,
    pub repair_minutes: u32,
    pub condition_after_service: u8,
    pub wage_gr_per_person_minute: i64,
    pub overhead_gr_per_batch: i64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct DockTuning {
    pub idle_fuel_ml_per_minute: i64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct TransportTuning {
    pub cost_gr_per_tonne_km: i64,
    pub minutes_per_km: u32,
    pub load_fixed_minutes: u32,
    pub pipeline_gr_per_tonne: i64,
}

impl TransportTuning {
    /// Koszt przewozu masy na dystans. Na `i128`, bo tona razy kilometr razy grosz
    /// wychodzi poza `i64` przy masie silosu.
    #[must_use]
    pub fn haul_cost(&self, mass: Mass, km: u32) -> Money {
        let grosze = i128::from(mass.0) * i128::from(km) * i128::from(self.cost_gr_per_tonne_km)
            / 1_000_000;
        Money(grosze as i64)
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Tuning {
    pub schema_version: u32,
    pub emission_ref_pm_g_per_min: i64,
    pub shortage: ShortageTuning,
    pub utility: UtilityTuning,
    pub plant: PlantTuning,
    pub dock: DockTuning,
    pub transport: TransportTuning,
}

#[derive(Debug)]
pub enum TuningError {
    Io(String),
    Parse(String),
    Schema { found: u32, want: u32 },
}

impl std::fmt::Display for TuningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TuningError::Io(e) => write!(f, "data/tuning/supply.ron: {e}"),
            TuningError::Parse(e) => write!(f, "data/tuning/supply.ron: {e}"),
            TuningError::Schema { found, want } => {
                write!(f, "data/tuning/supply.ron: schema_version {found}, oczekiwano {want}")
            }
        }
    }
}

impl std::error::Error for TuningError {}

impl Tuning {
    pub fn load(path: &std::path::Path) -> Result<Tuning, TuningError> {
        let tekst =
            std::fs::read_to_string(path).map_err(|e| TuningError::Io(e.to_string()))?;
        let t: Tuning =
            ron::from_str(&tekst).map_err(|e| TuningError::Parse(e.to_string()))?;
        if t.schema_version != TUNING_SCHEMA_VERSION {
            return Err(TuningError::Schema {
                found: t.schema_version,
                want: TUNING_SCHEMA_VERSION,
            });
        }
        Ok(t)
    }

    pub fn load_default() -> Result<Tuning, TuningError> {
        Tuning::load(&magnat_core::data_path("tuning/supply.ron"))
    }

    /// Skala emisji dla snapshotu renderu (M6 §6.4.3): `u8` z saturacją, wspólna dla
    /// wszystkich zakładów. Normalizacja per typ zakładu jest odrzucona — mała kotłownia
    /// na pełnej mocy dymiłaby jak huta, a mapa zanieczyszczeń kłamałaby dokładnie tam,
    /// gdzie gracz decyduje, gdzie postawić dom.
    #[must_use]
    pub fn emission_scale(&self, pm_g_per_min: i64) -> u8 {
        if self.emission_ref_pm_g_per_min <= 0 {
            return 0;
        }
        let v = 255 * pm_g_per_min / self.emission_ref_pm_g_per_min;
        v.clamp(0, 255) as u8
    }
}

/// Energia w **watominutach**: `Energy` to watogodziny (00 §2), a licznik tyka co minutę.
///
/// Ten sam zabieg co z paliwem w M4 (`K-25`) i z tego samego powodu: linia o poborze
/// 4 200 kWh/h zjada 70 kWh na minutę, ale linia o poborze 30 kWh/h zjada pół — a przy
/// dzieleniu na minuty w watogodzinach ta druga zjadałaby **zero** przez całą dobę.
/// Błąd nie znosiłby się, tylko kumulował w jedną stronę: rachunek za prąd byłby zerowy.
/// To nie jest odstępstwo od 00 §2 — §2 mówi, w czym wyraża się `Energy`, a nie w czym
/// wolno liczyć pośrednio; zakaz dotyczy floata, nie drobniejszej działki całkowitej.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct WattMinutes(pub i64);

impl WattMinutes {
    /// Watogodziny na granicy modułu — faktura, karta inspekcji, snapshot.
    #[must_use]
    pub const fn energy(self) -> Energy {
        Energy(self.0 / 60)
    }

    /// Minuta pracy przy poborze `draw` watogodzin na godzinę.
    #[must_use]
    pub const fn per_minute(draw: Energy) -> WattMinutes {
        WattMinutes(draw.0)
    }

    /// Energia szarży podana w watogodzinach.
    #[must_use]
    pub const fn from_energy(e: Energy) -> WattMinutes {
        WattMinutes(e.0 * 60)
    }
}

/// Mililitry wody jako masa. Gęstość 1 g/ml, więc przeliczenie nie ma gdzie zgubić reszty —
/// i dlatego woda może być medium licznikowym, nie tracąc masy w bilansie receptury.
#[must_use]
pub const fn water_mass(v: Volume) -> Mass {
    Mass(v.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plik danych musi się ładować i mieć sensowne liczby — inaczej pierwszy przebieg
    /// scenariusza pada na taryfie zero i wygląda to na błąd kodu.
    #[test]
    fn strojenie_laduje_sie_z_repozytorium() {
        let t = Tuning::load_default().expect("data/tuning/supply.ron");
        assert_eq!(t.schema_version, TUNING_SCHEMA_VERSION);
        assert!(t.utility.power_gr_per_kwh > 0);
        assert!(t.shortage.buffer_minutes > t.shortage.throttled_minutes);
        assert!(t.shortage.throttled_minutes > t.shortage.importing_minutes);
        assert!(t.plant.wear_minutes_per_point > 0);
    }

    /// Watominuta istnieje po to, żeby mały pobór nie znikał w zaokrągleniu.
    /// Linia 30 Wh/h przez dobę to 720 Wh — a nie zero, jak wyszłoby przy dzieleniu
    /// poboru przez 60 w każdej minucie.
    #[test]
    fn maly_pobor_nie_znika_w_zaokragleniu() {
        let mut licznik = WattMinutes::default();
        for _ in 0..1440 {
            licznik.0 += WattMinutes::per_minute(Energy(30)).0;
        }
        assert_eq!(licznik.energy(), Energy(720));
    }

    #[test]
    fn skala_emisji_saturuje_sie_zamiast_zawijac() {
        let t = Tuning::load_default().expect("data/tuning/supply.ron");
        assert_eq!(t.emission_scale(0), 0);
        assert_eq!(t.emission_scale(1_000_000), 255);
    }
}
