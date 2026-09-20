//! Strojenie badań i rozwoju: `data/tuning/rnd.ron` i jego walidacja (M10c §5.4, `K-35`).
//!
//! Osobny plik od drzewa technologii z rozmysłu i granica jest ta sama, którą `K-56`
//! postawił między `data/city/` a `data/tuning/city.ron`: **tutaj są liczby, które
//! wolno przestawić bez zmiany znaczenia modelu**, tam kształt. Zmiana
//! `mrp_per_full_time_day` przyspiesza wszystkie badania; dopisanie węzła do
//! `data/tech/tech.ron` zmienia to, co w ogóle da się odkryć.

use magnat_core::{FirmStrategy, Money};
use serde::Deserialize;
use std::path::Path;

/// Wersja schematu `data/tuning/rnd.ron` (00 §5).
pub const RND_SCHEMA_VERSION: u32 = 1;

/// Liczba kursów firmy — szerokość tablicy mnożników budżetu (`K-49`).
pub const FIRM_STRATEGY_COUNT: usize = FirmStrategy::ALL.len();

/// Kalibracja R&D.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RndTuning {
    /// Milipunkty badawcze z jednego milietatu pracy badacza przez dobę.
    pub mrp_per_full_time_day: u32,
    /// Miesięczny budżet materiałowy na jednego badacza, w groszach.
    pub material_budget_per_researcher_gr: i64,
    /// Mnożnik budżetu per kurs firmy, w promilach. Kolejność `FirmStrategy::ALL`.
    pub budget_by_strategy_permille: [u16; FIRM_STRATEGY_COUNT],
    /// Szansa przełomu na dobę, w dziesięciotysięcznych.
    pub breakthrough_per_10k_day: u16,
    /// O ile przełom skraca **pozostały** koszt, w punktach bazowych: (min, max).
    pub breakthrough_cut_bp: (u16, u16),
    /// Zniżka kosztu węzła, którego rok „światowy" już minął, w punktach bazowych.
    pub world_known_discount_bp: u16,
    /// Długość ochrony patentowej w latach gry.
    pub patent_years: u16,
    /// Opłata wstępna licencji za jeden punkt badawczy kosztu węzła, w groszach.
    pub upfront_per_rp_gr: i64,
    /// Widełki royalty w punktach bazowych przychodu: (min, max).
    pub royalty_bp: (u16, u16),
}

impl RndTuning {
    /// Mnożnik budżetu materiałowego dla kursu firmy, w promilach.
    #[must_use]
    pub fn budget_permille(&self, s: FirmStrategy) -> u32 {
        u32::from(self.budget_by_strategy_permille[s.as_index()])
    }

    /// Docelowy miesięczny budżet materiałowy zakładu o `n` badaczach.
    #[must_use]
    pub fn target_budget(&self, n: u32, s: FirmStrategy) -> Money {
        let base = self
            .material_budget_per_researcher_gr
            .saturating_mul(i64::from(n));
        Money(base.saturating_mul(i64::from(self.budget_permille(s))) / 1000)
    }
}

#[derive(Deserialize)]
struct RndFile {
    schema_version: u32,
    mrp_per_full_time_day: u32,
    material_budget_per_researcher_gr: i64,
    budget_by_strategy_permille: [u16; FIRM_STRATEGY_COUNT],
    breakthrough_per_10k_day: u16,
    breakthrough_cut_bp: (u16, u16),
    world_known_discount_bp: u16,
    patent_years: u16,
    upfront_per_rp_gr: i64,
    royalty_bp: (u16, u16),
}

/// Błąd wczytania `data/tuning/rnd.ron`.
#[derive(Debug)]
pub enum RndTuningError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    Schema {
        found: u32,
        want: u32,
    },
    /// Widełki odwrócone. Odejmowanie `max - min` na typie bez znaku zawinęłoby się
    /// i przełom skracałby koszt o sześćset procent — to ta sama klasa błędu,
    /// co `stories_max < stories_min` w `brand.ron` (`F-21` w M10b).
    Range {
        pole: &'static str,
        min: u32,
        max: u32,
    },
    /// Tempo zerowe znaczy badania, które nigdy się nie kończą. Plik, który to
    /// zapisuje, opisuje wyłączony mechanizm, a nie inne strojenie — i wyglądałby
    /// dokładnie tak samo jak działający.
    Zero(&'static str),
}

impl std::fmt::Display for RndTuningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RndTuningError::Io(e) => write!(f, "rnd.ron: {e}"),
            RndTuningError::Ron(e) => write!(f, "rnd.ron: {e}"),
            RndTuningError::Schema { found, want } => {
                write!(f, "rnd.ron: schema_version {found}, oczekiwano {want}")
            }
            RndTuningError::Range { pole, min, max } => {
                write!(
                    f,
                    "rnd.ron: {pole} — górna granica {max} poniżej dolnej {min}"
                )
            }
            RndTuningError::Zero(pole) => {
                write!(
                    f,
                    "rnd.ron: {pole} jest zerem, czyli badania nigdy się nie kończą"
                )
            }
        }
    }
}

impl std::error::Error for RndTuningError {}

impl RndTuning {
    /// Wczytuje i waliduje plik strojenia R&D.
    pub fn load(path: &Path) -> Result<RndTuning, RndTuningError> {
        let tekst = std::fs::read_to_string(path).map_err(RndTuningError::Io)?;
        let f: RndFile = ron::from_str(&tekst).map_err(RndTuningError::Ron)?;
        if f.schema_version != RND_SCHEMA_VERSION {
            return Err(RndTuningError::Schema {
                found: f.schema_version,
                want: RND_SCHEMA_VERSION,
            });
        }
        if f.mrp_per_full_time_day == 0 {
            return Err(RndTuningError::Zero("mrp_per_full_time_day"));
        }
        if f.breakthrough_cut_bp.1 < f.breakthrough_cut_bp.0 {
            return Err(RndTuningError::Range {
                pole: "breakthrough_cut_bp",
                min: u32::from(f.breakthrough_cut_bp.0),
                max: u32::from(f.breakthrough_cut_bp.1),
            });
        }
        if f.royalty_bp.1 < f.royalty_bp.0 {
            return Err(RndTuningError::Range {
                pole: "royalty_bp",
                min: u32::from(f.royalty_bp.0),
                max: u32::from(f.royalty_bp.1),
            });
        }
        Ok(RndTuning {
            mrp_per_full_time_day: f.mrp_per_full_time_day,
            material_budget_per_researcher_gr: f.material_budget_per_researcher_gr,
            budget_by_strategy_permille: f.budget_by_strategy_permille,
            breakthrough_per_10k_day: f.breakthrough_per_10k_day,
            breakthrough_cut_bp: f.breakthrough_cut_bp,
            world_known_discount_bp: f.world_known_discount_bp,
            patent_years: f.patent_years,
            upfront_per_rp_gr: f.upfront_per_rp_gr,
            royalty_bp: f.royalty_bp,
        })
    }

    /// Wczytuje z katalogu danych gry.
    pub fn load_default() -> Result<RndTuning, RndTuningError> {
        RndTuning::load(&magnat_core::data_path("tuning/rnd.ron"))
    }
}

impl Default for RndTuning {
    /// Kalibracja awaryjna dla testów jednostkowych — te same liczby, co w pliku.
    ///
    /// Istnieje z tego samego powodu co `Default for BrandData`: test stawiający
    /// jeden zakład nie ma powodu czytać katalogu danych. **Świat produkcyjny czyta
    /// plik** (`game::world::full`), bo martwa ładowarka ukrywa niepoprawne dane —
    /// to jest cała nauka z `F-22` w M10b.
    fn default() -> RndTuning {
        RndTuning {
            mrp_per_full_time_day: 1590,
            material_budget_per_researcher_gr: 1_250_000,
            budget_by_strategy_permille: [1000, 700, 1100, 1000, 1500, 800],
            breakthrough_per_10k_day: 5,
            breakthrough_cut_bp: (1000, 4000),
            world_known_discount_bp: 6000,
            patent_years: 20,
            upfront_per_rp_gr: 4_000,
            royalty_bp: (300, 700),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plik_strojenia_sie_wczytuje() {
        // Ten test istnieje, bo w M10b ładowarka `brand.ron` była martwa przez całą
        // podfazę i ukryła plik, który się nie parsował (`F-22`).
        let t = RndTuning::load_default().expect("data/tuning/rnd.ron");
        assert_eq!(t, RndTuning::default());
    }

    #[test]
    fn kurs_firmy_przestawia_budzet() {
        let t = RndTuning::default();
        let ostrozna = t.target_budget(4, FirmStrategy::Cautious);
        let innowacyjna = t.target_budget(4, FirmStrategy::Innovative);
        assert_eq!(ostrozna, Money(5_000_000));
        assert_eq!(innowacyjna, Money(7_500_000));
    }
}
