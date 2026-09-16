//! Wagi produktywności per zawód — widok `sim/firms` na `data/jobs/roles.ron`
//! (M7a WP3, M7 §5.3, decyzja otwarta `D16` fazy).
//!
//! ## Dlaczego to jest drugi czytelnik tego samego pliku, a nie drugi plik
//!
//! `JobRoleId` jest **pozycją roli w `data/jobs/roles.ron`** — tak nadaje go M2
//! (`JobTable`) i tak siedzi w komponencie `Employment` każdego mieszkańca, czyli
//! w zapisie gry. Gdyby wagi M7 mieszkały w osobnym pliku, ten plik musiałby
//! powtarzać **kolejność** ról, a dwie listy o wymuszonej wspólnej kolejności
//! rozjeżdżają się przy pierwszej zmianie. Gdyby `sim/firms` przejął `JobTable`,
//! ciągnąłby za sobą `UnitClass` — czyli gramatykę budynków M2.
//!
//! Stąd `D16` rozstrzygnięte jako **rozszerzenie schematu**: jeden plik, dwa widoki.
//! M2 czyta z roli widełki, zmianę i klasę lokalu; M7 czyta klucz i wagi. Rozjazd
//! wymagałby przestawienia kolejności w pliku, a to psuje oba widoki jednakowo —
//! i pilnuje tego test `oba_widoki_daja_ten_sam_indeks` w `sim/world`.

use magnat_core::{JobRoleId, Q};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Wersja schematu `data/jobs/roles.ron`. Podniesiona z 1 do 2 przy dopisaniu
/// `weights` — pole jest **wymagane**, bo rola bez wag nie umie pracować,
/// a cicha wartość domyślna dałaby zawód, w którym umiejętność nic nie znaczy.
pub const JOBS_SCHEMA_VERSION: u32 = 2;

/// Wagi składników produktywności, w tysięcznych. **Suma musi wynosić 1000** —
/// inaczej zmiana wag zmieniałaby skalę wyniku, a nie tylko jego strukturę.
///
/// Dla `researcher` liczy się umiejętność, dla `truck_driver` energia i zdrowie.
/// Zmiana wag jest zmianą danych, nie kodu (M7 §5.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct RoleWeights {
    pub skill: u16,
    pub energy: u16,
    pub mood: u16,
    pub health: u16,
}

impl RoleWeights {
    pub const TOTAL: u16 = 1000;

    #[must_use]
    pub const fn sum(&self) -> u32 {
        self.skill as u32 + self.energy as u32 + self.mood as u32 + self.health as u32
    }

    /// Składowa bazowa produktywności w tysięcznych, składana w **ustalonej
    /// kolejności** pól (dokument 00 §2: każdy wynik wpływający na stan trwały
    /// sumuje się w kolejności, która nie zależy od niczego poza kodem).
    ///
    /// `mood01` to nastrój przeskalowany z −100..=100 do 0..=100 — zły nastrój
    /// obniża pracę, ale nigdy nie wytwarza pracy ujemnej.
    #[must_use]
    pub fn base(&self, skill: Q, energy: Q, mood01: Q, health: Q) -> i32 {
        let mut acc: i64 = 0;
        acc += i64::from(self.skill) * i64::from(skill.get());
        acc += i64::from(self.energy) * i64::from(energy.get());
        acc += i64::from(self.mood) * i64::from(mood01.get());
        acc += i64::from(self.health) * i64::from(health.get());
        // Skala: Σw = 1000, każdy składnik 0..=100, więc acc ∈ 0..=100_000.
        (acc / 1000) as i32
    }
}

/// Rola w widoku M7: klucz i wagi. Reszta rekordu (widełki, zmiana, klasa lokalu,
/// prestiż) należy do M2 i M3 — `serde` pomija ją, bo nie ma jej w tej strukturze.
#[derive(Clone, Debug, Deserialize)]
struct RoleSpec {
    key: String,
    weights: RoleWeights,
}

#[derive(Deserialize)]
struct RolesFile {
    schema_version: u32,
    roles: Vec<RoleSpec>,
}

#[derive(Debug)]
pub enum RoleError {
    Io(std::io::Error),
    Ron(String),
    Schema {
        found: u32,
        want: u32,
    },
    /// Wagi roli nie sumują się do 1000.
    BadWeights {
        role: String,
        sum: u32,
    },
    DuplicateKey(String),
}

impl std::fmt::Display for RoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoleError::Io(e) => write!(f, "błąd wejścia-wyjścia: {e}"),
            RoleError::Ron(m) => write!(f, "data/jobs/roles.ron: {m}"),
            RoleError::Schema { found, want } => {
                write!(
                    f,
                    "data/jobs/roles.ron: schema_version {found}, oczekiwano {want}"
                )
            }
            RoleError::BadWeights { role, sum } => write!(
                f,
                "rola „{role}”: wagi produktywności sumują się do {sum}, a mają do {}",
                RoleWeights::TOTAL
            ),
            RoleError::DuplicateKey(k) => write!(f, "rola „{k}” jest zdefiniowana dwa razy"),
        }
    }
}

impl std::error::Error for RoleError {}

/// Tablica ról w kolejności z pliku. Pozycja **jest** `JobRoleId` (kontrakt zapisu gry).
#[derive(Clone, Debug, Default)]
pub struct RoleTable {
    keys: Vec<String>,
    weights: Vec<RoleWeights>,
    index: BTreeMap<String, JobRoleId>,
}

impl RoleTable {
    #[must_use]
    pub fn id(&self, key: &str) -> Option<JobRoleId> {
        self.index.get(key).copied()
    }

    #[must_use]
    pub fn key(&self, id: JobRoleId) -> &str {
        &self.keys[id.0 as usize]
    }

    #[must_use]
    pub fn weights(&self, id: JobRoleId) -> RoleWeights {
        self.weights[id.0 as usize]
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn load(path: &Path) -> Result<RoleTable, RoleError> {
        let txt = std::fs::read_to_string(path).map_err(RoleError::Io)?;
        RoleTable::parse(&txt)
    }

    pub fn load_default() -> Result<RoleTable, RoleError> {
        RoleTable::load(&magnat_core::assets::data_path("jobs/roles.ron"))
    }

    pub fn parse(txt: &str) -> Result<RoleTable, RoleError> {
        let f: RolesFile = ron::from_str(txt).map_err(|e| RoleError::Ron(e.to_string()))?;
        if f.schema_version != JOBS_SCHEMA_VERSION {
            return Err(RoleError::Schema {
                found: f.schema_version,
                want: JOBS_SCHEMA_VERSION,
            });
        }
        let mut t = RoleTable::default();
        for (i, r) in f.roles.into_iter().enumerate() {
            let sum = r.weights.sum();
            if sum != u32::from(RoleWeights::TOTAL) {
                return Err(RoleError::BadWeights { role: r.key, sum });
            }
            if t.index.contains_key(&r.key) {
                return Err(RoleError::DuplicateKey(r.key));
            }
            t.index.insert(r.key.clone(), JobRoleId(i as u16));
            t.keys.push(r.key);
            t.weights.push(r.weights);
        }
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLIK: &str = r#"(
        schema_version: 2,
        roles: [
            ( key: "a", weights: (skill: 400, energy: 300, mood: 100, health: 200) ),
            ( key: "b", weights: (skill: 700, energy: 100, mood: 100, health: 100) ),
        ],
    )"#;

    #[test]
    fn indeks_jest_pozycja_w_pliku() {
        let t = RoleTable::parse(PLIK).expect("poprawny plik");
        assert_eq!(t.id("a"), Some(JobRoleId(0)));
        assert_eq!(t.id("b"), Some(JobRoleId(1)));
        assert_eq!(t.key(JobRoleId(1)), "b");
    }

    #[test]
    fn wagi_nie_sumujace_sie_sa_bledem() {
        let zly = PLIK.replace("skill: 400", "skill: 401");
        assert!(matches!(
            RoleTable::parse(&zly),
            Err(RoleError::BadWeights { .. })
        ));
    }

    #[test]
    fn baza_produktywnosci_jest_w_tysiecznych() {
        let w = RoleWeights {
            skill: 400,
            energy: 300,
            mood: 100,
            health: 200,
        };
        // Wszystko na 100 → 100 × Σw / 1000 = 100.
        assert_eq!(
            w.base(Q::new(100), Q::new(100), Q::new(100), Q::new(100)),
            100
        );
        assert_eq!(w.base(Q::new(0), Q::new(0), Q::new(0), Q::new(0)), 0);
    }
}
