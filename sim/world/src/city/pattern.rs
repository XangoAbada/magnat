//! Wzorce globalne L-systemu i pierścienie, w których obowiązują (M2 §5.2).
//!
//! **Korekta wobec planu.** Plan wybiera wzorzec „per dzielnica-zalążek z
//! `data/districts/*.ron`, ważony epoką pierścienia". Ani dzielnice (§5.5), ani pierścienie
//! epok (§5.3) jeszcze nie istnieją — powstają w M2c, czyli **po** drogach. Wzorzec jest
//! tu więc wybierany po **względnej odległości od środka miasta**, a to jest dokładnie ta
//! wielkość, którą pierścień epoki przybliża: miasto rosło koncentrycznie, więc promień
//! jest wiekiem zabudowy. M2c dostaje z tego pierścienie, nie odwrotnie.

use crate::assets::data_path;
use serde::{Deserialize, Serialize};

/// Wzorzec kierunku ulic. `Grid` i `Superblock` niosą kąt siatki w stopniach.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pattern {
    Radial,
    Grid(f32),
    Organic,
    Contour,
    Superblock(f32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PatternKind {
    Radial,
    Grid,
    Organic,
    Contour,
    Superblock,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Ring {
    /// Górna granica pierścienia jako ułamek promienia obszaru zurbanizowanego.
    pub max_radius_frac: f32,
    pub pattern: PatternKind,
    /// Kąt siatki dodawany do losowego kąta miasta (tylko `Grid`/`Superblock`).
    pub grid_angle_deg: f32,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct RingTable {
    pub schema_version: u32,
    pub rings: Vec<Ring>,
    /// Nachylenie, powyżej którego teren wygrywa z urbanistyką i wzorzec ustępuje
    /// `Contour`. W jednostkach `TerrainQuery::slope_at`.
    pub contour_slope: u8,
}

pub const RING_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum RingError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    Schema { found: u32 },
    Empty,
}

impl std::fmt::Display for RingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RingError::Io(e) => write!(f, "data/districts/patterns.ron: {e}"),
            RingError::Ron(e) => write!(f, "data/districts/patterns.ron: {e}"),
            RingError::Schema { found } => write!(
                f,
                "data/districts/patterns.ron: schema_version {found}, oczekiwano {RING_SCHEMA_VERSION}"
            ),
            RingError::Empty => write!(f, "data/districts/patterns.ron: pusta lista pierścieni"),
        }
    }
}

impl std::error::Error for RingError {}

impl RingTable {
    pub fn load() -> Result<RingTable, RingError> {
        let txt =
            std::fs::read_to_string(data_path("districts/patterns.ron")).map_err(RingError::Io)?;
        let t: RingTable = ron::from_str(&txt).map_err(RingError::Ron)?;
        if t.schema_version != RING_SCHEMA_VERSION {
            return Err(RingError::Schema {
                found: t.schema_version,
            });
        }
        if t.rings.is_empty() {
            return Err(RingError::Empty);
        }
        Ok(t)
    }

    /// Wzorzec dla punktu o względnym promieniu `r_frac` i nachyleniu `slope`.
    #[must_use]
    pub fn pattern_at(&self, r_frac: f32, slope: u8, city_theta: f32) -> Pattern {
        if slope > self.contour_slope {
            return Pattern::Contour;
        }
        let ring = self
            .rings
            .iter()
            .find(|r| r_frac <= r.max_radius_frac)
            .unwrap_or_else(|| self.rings.last().expect("rings niepuste"));
        match ring.pattern {
            PatternKind::Radial => Pattern::Radial,
            PatternKind::Organic => Pattern::Organic,
            PatternKind::Contour => Pattern::Contour,
            PatternKind::Grid => Pattern::Grid(city_theta + ring.grid_angle_deg),
            PatternKind::Superblock => Pattern::Superblock(city_theta + ring.grid_angle_deg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plik_pierscieni_laduje_sie_i_pokrywa_cale_miasto() {
        let t = RingTable::load().expect("data/districts/patterns.ron");
        // Ostatni pierścień musi sięgać poza obszar zurbanizowany — inaczej propozycja
        // na jego skraju nie miałaby wzorca.
        assert!(t.rings.last().unwrap().max_radius_frac >= 1.0);
        // Pierścienie są uporządkowane rosnąco, bo `find` bierze pierwszy pasujący.
        assert!(t
            .rings
            .windows(2)
            .all(|w| w[0].max_radius_frac < w[1].max_radius_frac));
    }

    #[test]
    fn stromizna_wygrywa_z_urbanistyka() {
        let t = RingTable::load().expect("data/districts/patterns.ron");
        assert_eq!(t.pattern_at(0.1, 200, 0.0), Pattern::Contour);
    }
}
