//! Tabela generacji populacji — `data/demography/population.ron` (M3d §5.9).
//!
//! Stan startowy miasta, nie przejścia: piramida wieku, skład gospodarstw, rozkład
//! wykształcenia i dopasowanie kierunku do roli. Hazardy (umieralność, płodność,
//! migracja) są w `DemographyTable` po stronie `sim/agents` i ten plik ich nie dubluje.
//!
//! Walidacja jest **twarda**: rozkład, który nie sumuje się do 1000, nie jest
//! rozkładem, tylko literówką — a piramidy zepsutej o 2 ‰ nie odróżni od błędu
//! generatora żaden test χ².

use magnat_core::PlaceKind;
use serde::Deserialize;

/// Ile pasm pięcioletnich ma piramida: 0–4 … 105–109.
pub const AGE_BANDS: usize = 22;
/// Szerokość pasma w latach.
pub const BAND_YEARS: u32 = 5;
/// Liczba poziomów `EduLevel`.
pub const EDU_LEVELS: usize = 5;
/// Liczba kierunków `EduField` łącznie z `None`.
pub const EDU_FIELDS: usize = 6;
/// Liczba cech `TraitId`.
pub const TRAITS: usize = 8;

pub const POPULATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
pub struct Pyramid {
    pub epoch: String,
    pub bands: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct HouseholdMix {
    pub adults: u8,
    pub children: u8,
    pub weight: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EducationMix {
    pub epoch: String,
    pub levels: Vec<u16>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SkillFit {
    pub role: String,
    pub fit: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct TraitSpec {
    pub mean: u8,
    pub spread: u8,
    pub edu_bias: i8,
    pub status_bias: i8,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct CommuteTarget {
    pub median_min: u16,
    pub sigma_centi: u16,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct KnowledgeSeed {
    pub home_radius_m: u16,
    pub home_score: u8,
    pub route_score: u8,
    pub work_score: u8,
    pub max_near_home: u8,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct StartingRelations {
    pub coworkers_min: u8,
    pub coworkers_max: u8,
    pub neighbours_min: u8,
    pub neighbours_max: u8,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PopulationTable {
    pub schema_version: u32,
    pub default_pyramid: String,
    pub pyramids: Vec<Pyramid>,
    pub households: Vec<HouseholdMix>,
    pub education: Vec<EducationMix>,
    pub fields: Vec<u16>,
    pub skill_fit: Vec<SkillFit>,
    pub traits: Vec<TraitSpec>,
    pub commute: CommuteTarget,
    pub unemployment_permille: u16,
    pub knowledge: KnowledgeSeed,
    pub starting_relations: StartingRelations,
}

#[derive(Debug)]
pub enum TableError {
    Io(std::io::Error),
    Ron(ron::de::SpannedError),
    Schema {
        found: u32,
    },
    /// Rozkład, który nie sumuje się do 1000 ‰.
    NotADistribution {
        what: String,
        sum: u32,
    },
    Missing(String),
    BadLength {
        what: String,
        want: usize,
        got: usize,
    },
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableError::Io(e) => write!(f, "data/demography/population.ron: {e}"),
            TableError::Ron(e) => write!(f, "data/demography/population.ron: {e}"),
            TableError::Schema { found } => write!(
                f,
                "population.ron: schema_version {found}, oczekiwano {POPULATION_SCHEMA_VERSION}"
            ),
            TableError::NotADistribution { what, sum } => {
                write!(
                    f,
                    "population.ron: {what} sumuje się do {sum} ‰ zamiast 1000"
                )
            }
            TableError::Missing(w) => write!(f, "population.ron: brak wpisu {w}"),
            TableError::BadLength { what, want, got } => {
                write!(
                    f,
                    "population.ron: {what} ma {got} pozycji, oczekiwano {want}"
                )
            }
        }
    }
}

impl std::error::Error for TableError {}

impl PopulationTable {
    pub fn load() -> Result<PopulationTable, TableError> {
        let path = crate::assets::data_path("demography/population.ron");
        let txt = std::fs::read_to_string(&path).map_err(TableError::Io)?;
        let t: PopulationTable = ron::from_str(&txt).map_err(TableError::Ron)?;
        t.validate()?;
        Ok(t)
    }

    fn validate(&self) -> Result<(), TableError> {
        if self.schema_version != POPULATION_SCHEMA_VERSION {
            return Err(TableError::Schema {
                found: self.schema_version,
            });
        }
        for p in &self.pyramids {
            if p.bands.len() != AGE_BANDS {
                return Err(TableError::BadLength {
                    what: format!("piramida {}", p.epoch),
                    want: AGE_BANDS,
                    got: p.bands.len(),
                });
            }
            suma(&p.bands, &format!("piramida {}", p.epoch))?;
        }
        if self
            .pyramids
            .iter()
            .all(|p| p.epoch != self.default_pyramid)
        {
            return Err(TableError::Missing(format!(
                "default_pyramid {}",
                self.default_pyramid
            )));
        }
        for e in &self.education {
            if e.levels.len() != EDU_LEVELS {
                return Err(TableError::BadLength {
                    what: format!("wykształcenie {}", e.epoch),
                    want: EDU_LEVELS,
                    got: e.levels.len(),
                });
            }
            suma(&e.levels, &format!("wykształcenie {}", e.epoch))?;
        }
        suma(&self.fields, "fields")?;
        suma(
            &self.households.iter().map(|h| h.weight).collect::<Vec<_>>(),
            "households",
        )?;
        if self.households.iter().all(|h| h.adults == 0) {
            return Err(TableError::Missing("gospodarstwo z dorosłym".into()));
        }
        for s in &self.skill_fit {
            if s.fit.len() != EDU_FIELDS {
                return Err(TableError::BadLength {
                    what: format!("skill_fit {}", s.role),
                    want: EDU_FIELDS,
                    got: s.fit.len(),
                });
            }
        }
        if self.traits.len() != TRAITS {
            return Err(TableError::BadLength {
                what: "traits".into(),
                want: TRAITS,
                got: self.traits.len(),
            });
        }
        Ok(())
    }

    /// Piramida epoki albo domyślna. Epoka bez własnego wpisu nie jest błędem —
    /// jest epoką, dla której nikt jeszcze nie policzył rozkładu.
    #[must_use]
    pub fn pyramid(&self, epoch: &str) -> &[u16] {
        self.pyramids
            .iter()
            .find(|p| p.epoch == epoch)
            .or_else(|| {
                self.pyramids
                    .iter()
                    .find(|p| p.epoch == self.default_pyramid)
            })
            .map_or(&[], |p| p.bands.as_slice())
    }

    #[must_use]
    pub fn education_of(&self, epoch: &str) -> &[u16] {
        self.education
            .iter()
            .find(|e| e.epoch == epoch)
            .or_else(|| self.education.last())
            .map_or(&[], |e| e.levels.as_slice())
    }

    /// Dopasowanie kierunku do roli, 0..=100. Rola bez wiersza dostaje 50 — „nie wiadomo",
    /// a nie „nie pasuje": inaczej dodanie roli w M7 zerowałoby zatrudnienie w niej.
    #[must_use]
    pub fn fit_of(&self, role_key: &str, field: u8) -> u8 {
        self.skill_fit
            .iter()
            .find(|s| s.role == role_key)
            .and_then(|s| s.fit.get(field as usize).copied())
            .unwrap_or(50)
    }
}

fn suma(xs: &[u16], what: &str) -> Result<(), TableError> {
    let s: u32 = xs.iter().map(|v| u32::from(*v)).sum();
    if s != 1000 {
        return Err(TableError::NotADistribution {
            what: what.to_string(),
            sum: s,
        });
    }
    Ok(())
}

/// Rodzaj miejsca dla archetypu zakładu — jedyne miejsce, w którym Etap 8 pyta
/// `data/buildings/` o to, **do czego** zakład służy mieszkańcom. Zakład bez wpisu
/// jest wyłącznie miejscem pracy.
#[must_use]
pub fn place_kind_of(a: &crate::city::sites::Archetype) -> Option<PlaceKind> {
    a.spec.place_kind
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabela_z_repozytorium_laduje_sie_i_jest_rozkladem() {
        let t = PopulationTable::load().expect("data/demography/population.ron");
        assert_eq!(t.pyramid("contemporary").len(), AGE_BANDS);
        // Epoka bez wpisu spada na domyślną, a nie na pustkę.
        assert_eq!(t.pyramid("nie-ma-takiej").len(), AGE_BANDS);
        assert_eq!(t.education_of("contemporary").len(), EDU_LEVELS);
        assert_eq!(t.fit_of("nie-ma-takiej-roli", 3), 50);
    }

    #[test]
    fn rozklad_niesumujacy_sie_do_1000_jest_bledem() {
        assert!(suma(&[500, 400], "test").is_err());
        assert!(suma(&[500, 500], "test").is_ok());
    }
}
