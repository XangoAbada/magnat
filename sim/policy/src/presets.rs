//! Presety polityk z `data/policies/` (M7 §6, 00 §5).
//!
//! **Polityka jest daną, nie kodem.** To jest wykonanie `O` z SOLID w tym projekcie
//! (CLAUDE.md: rozszerzanie przez `data/*.ron`) i droga do moddowalnej AI w M12:
//! nowa strategia firmy to nowy wpis w pliku, a nie nowa gałąź w cudzym `match`.
//!
//! Presety są **wzorcami**, nie instancjami: mają numer w katalogu, a `PolicyId`
//! nadaje im dopiero przypisanie do zakładu — tak samo jak `GoodId` powstaje przy
//! ładowaniu katalogu towarów, a nie stoi w pliku.

use serde::Deserialize;
use std::path::Path;

use crate::ast::{Policy, PolicyDomain};
use crate::validate::{validate, PolicyError};

/// Wersja schematu `data/policies/*.ron`.
pub const POLICIES_SCHEMA_VERSION: u32 = 1;

/// Wzorzec polityki z katalogu.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct PolicyPreset {
    /// Klucz tekstowy — to on, a nie numer, stoi w zapisie gry (00 §5).
    pub key: String,
    pub policy: Policy,
}

#[derive(Deserialize)]
struct Plik {
    schema_version: u32,
    presets: Vec<PolicyPreset>,
}

/// Katalog presetów. `Vec` posortowany po kluczu — numer presetu jest jego pozycją
/// i jest stabilny w obrębie wersji danych.
#[derive(Clone, Debug, Default)]
pub struct PolicyCatalog {
    presets: Vec<PolicyPreset>,
}

#[derive(Debug)]
pub enum CatalogError {
    Io(String),
    Parse(String),
    Schema {
        found: u32,
        want: u32,
    },
    /// Preset, który nie przeszedł walidatora. Ładowanie się wtedy **nie udaje** —
    /// polityka w danych jest kontraktem tak samo jak katalog towarów, a wadliwy
    /// preset objawiłby się jako sklep, który nic nie robi.
    Invalid {
        key: String,
        err: PolicyError,
    },
    DuplicateKey(String),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::Io(e) | CatalogError::Parse(e) => write!(f, "data/policies: {e}"),
            CatalogError::Schema { found, want } => {
                write!(
                    f,
                    "data/policies: schema_version {found}, oczekiwano {want}"
                )
            }
            CatalogError::Invalid { key, err } => {
                write!(f, "data/policies: preset „{key}” — {err:?}")
            }
            CatalogError::DuplicateKey(k) => write!(f, "data/policies: klucz „{k}” dwa razy"),
        }
    }
}

impl std::error::Error for CatalogError {}

impl PolicyCatalog {
    pub fn load_default() -> Result<PolicyCatalog, CatalogError> {
        PolicyCatalog::load(&magnat_core::data_path("policies/presets.ron"))
    }

    pub fn load(path: &Path) -> Result<PolicyCatalog, CatalogError> {
        let txt = std::fs::read_to_string(path).map_err(|e| CatalogError::Io(e.to_string()))?;
        PolicyCatalog::parse(&txt)
    }

    pub fn parse(txt: &str) -> Result<PolicyCatalog, CatalogError> {
        let p: Plik = ron::from_str(txt).map_err(|e| CatalogError::Parse(e.to_string()))?;
        if p.schema_version != POLICIES_SCHEMA_VERSION {
            return Err(CatalogError::Schema {
                found: p.schema_version,
                want: POLICIES_SCHEMA_VERSION,
            });
        }
        let mut presets = p.presets;
        presets.sort_by(|a, b| a.key.cmp(&b.key));
        for para in presets.windows(2) {
            if para[0].key == para[1].key {
                return Err(CatalogError::DuplicateKey(para[0].key.clone()));
            }
        }
        for pr in &presets {
            validate(&pr.policy).map_err(|err| CatalogError::Invalid {
                key: pr.key.clone(),
                err,
            })?;
        }
        Ok(PolicyCatalog { presets })
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Policy> {
        self.presets
            .binary_search_by(|p| p.key.as_str().cmp(key))
            .ok()
            .map(|i| &self.presets[i].policy)
    }

    /// Kopia presetu z nadanym numerem — tak preset staje się polityką zakładu.
    #[must_use]
    pub fn instantiate(&self, key: &str, id: magnat_core::PolicyId) -> Option<Policy> {
        let mut p = self.get(key)?.clone();
        p.id = id;
        Some(p)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.presets.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PolicyPreset> {
        self.presets.iter()
    }

    /// Pierwszy preset o tej dziedzinie — skrót dla mostu stawiającego miasto,
    /// który nie zna nazw presetów, a musi czymś obsadzić zakład.
    #[must_use]
    pub fn first_of(&self, domain: PolicyDomain) -> Option<&PolicyPreset> {
        self.presets.iter().find(|p| p.policy.domain == domain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katalog_z_repozytorium_sie_laduje_i_jest_poprawny() {
        let c = PolicyCatalog::load_default().expect("data/policies/presets.ron");
        assert!(c.len() >= 3, "presetów jest {}", c.len());
        // Każdy preset przeszedł walidator — `load` by inaczej nie wrócił.
        // Sprawdzamy jeszcze, że katalog odpowiada na pytanie, po które istnieje.
        assert!(c.first_of(PolicyDomain::Pricing).is_some());
        assert!(c.first_of(PolicyDomain::Stock).is_some());
    }

    #[test]
    fn preset_niepoprawny_nie_wchodzi_do_katalogu() {
        // Ta sama polityka co „dyskont", ale z ceną netto po jednej stronie —
        // `PriceBasisMismatch` ma zatrzymać ładowanie, a nie wjechać do miasta.
        let zly = r#"(
            schema_version: 1,
            presets: [(
                key: "zly",
                policy: (
                    id: (0),
                    name: "zły",
                    domain: Pricing,
                    rules: [(
                        when: Cmp(
                            lhs: Metric(Price(good: This, basis: GrossRetail)),
                            op: Gt,
                            rhs: Metric(UnitCost(This)),
                        ),
                        then: [SetPrice(good: This, to: Metric(UnitCost(This)))],
                    )],
                    cooldown_h: 12,
                ),
            )],
        )"#;
        assert!(matches!(
            PolicyCatalog::parse(zly),
            Err(CatalogError::Invalid { .. })
        ));
    }
}
