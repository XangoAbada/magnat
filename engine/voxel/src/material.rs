//! Rejestr materiałów voxelowych (M1 §5.2).
//!
//! Materiały są **danymi** (`data/materials/*.ron`), nie enumem w kodzie: dołożenie skały
//! ma być zmianą w pliku, nie rekompilacją (00 §5, zasada *O* z SOLID). `MaterialId` jest
//! indeksem do tego rejestru i jest stabilny **w obrębie wersji danych** — w zapisie gry
//! trzymamy klucz tekstowy, nie indeks (00 §5).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// Indeks materiału w rejestrze globalnym.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct MaterialId(pub u16);

impl MaterialId {
    /// Powietrze ma **zagwarantowany** indeks 0. To nie jest przypadek porządku alfabetycznego,
    /// tylko rezerwacja: pusty chunk i wyzerowana paleta muszą znaczyć „powietrze" bez zaglądania
    /// do rejestru, a meshing sprawdza pustkę miliony razy na klatkę.
    pub const AIR: MaterialId = MaterialId(0);

    #[inline]
    #[must_use]
    pub const fn is_air(self) -> bool {
        self.0 == 0
    }

    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Indeks w palecie **lokalnej** chunka (M1 §5.2, PRD §16.3: ≤ 256 materiałów na chunk).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct LocalIdx(pub u8);

impl LocalIdx {
    /// Pozycja 0 palety jest zawsze powietrzem — patrz [`MaterialId::AIR`].
    pub const AIR: LocalIdx = LocalIdx(0);
}

/// Własności materiału istotne dla logiki, nie dla wyglądu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct MaterialFlags(pub u16);

impl MaterialFlags {
    pub const SOLID: MaterialFlags = MaterialFlags(1 << 0);
    pub const LIQUID: MaterialFlags = MaterialFlags(1 << 1);
    pub const TRANSPARENT: MaterialFlags = MaterialFlags(1 << 2);
    pub const DIGGABLE: MaterialFlags = MaterialFlags(1 << 3);
    pub const BUILDABLE: MaterialFlags = MaterialFlags(1 << 4);
    pub const SUPPORTS_VEGETATION: MaterialFlags = MaterialFlags(1 << 5);

    #[inline]
    #[must_use]
    pub const fn contains(self, f: MaterialFlags) -> bool {
        self.0 & f.0 == f.0
    }

    #[inline]
    #[must_use]
    pub const fn union(self, f: MaterialFlags) -> MaterialFlags {
        MaterialFlags(self.0 | f.0)
    }
}

/// Materiał voxelowy. Pola poza `albedo`/`roughness`/`emissive` są konsumowane przez
/// symulację, nie przez render: `hardness` przez M6 (koszt wydobycia), `bearing_capacity`
/// przez M2 (koszt fundamentów), `density_kg_m3` przez M1 (bilans masy złoża, §5.5).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VoxelMaterial {
    pub key: Box<str>,
    pub albedo: [u8; 3],
    pub roughness: u8,
    pub emissive: u8,
    pub flags: MaterialFlags,
    pub hardness: u8,
    pub density_kg_m3: u16,
    pub bearing_capacity: u8,
}

impl VoxelMaterial {
    #[inline]
    #[must_use]
    pub fn is_solid(&self) -> bool {
        self.flags.contains(MaterialFlags::SOLID)
    }

    #[inline]
    #[must_use]
    pub fn is_opaque(&self) -> bool {
        self.flags.contains(MaterialFlags::SOLID)
            && !self.flags.contains(MaterialFlags::TRANSPARENT)
    }
}

/// Postać materiału w pliku RON. Osobna od [`VoxelMaterial`], bo w danych flagi są listą
/// nazw (czytelną i diffowalną), a w pamięci maską bitową (tanią).
#[derive(Clone, Debug, Deserialize)]
struct MaterialDef {
    key: String,
    albedo: (u8, u8, u8),
    #[serde(default = "default_roughness")]
    roughness: u8,
    #[serde(default)]
    emissive: u8,
    #[serde(default)]
    flags: Vec<String>,
    #[serde(default)]
    hardness: u8,
    #[serde(default)]
    density_kg_m3: u16,
    #[serde(default)]
    bearing_capacity: u8,
}

const fn default_roughness() -> u8 {
    200
}

#[derive(Clone, Debug, Deserialize)]
struct MaterialFile {
    schema_version: u32,
    materials: Vec<MaterialDef>,
}

/// Wersja schematu, którą rozumie ten kod. Plik z inną wersją jest błędem, nie ostrzeżeniem —
/// cicha zgodność wsteczna z danymi kończy się światem wygenerowanym z połowy parametrów.
pub const MATERIALS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum MaterialError {
    Io(std::io::Error),
    Parse { file: String, msg: String },
    SchemaVersion { file: String, got: u32 },
    DuplicateKey(String),
    UnknownFlag { key: String, flag: String },
    MissingAir,
    TooMany(usize),
}

impl fmt::Display for MaterialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MaterialError::Io(e) => write!(f, "odczyt materiałów: {e}"),
            MaterialError::Parse { file, msg } => write!(f, "{file}: {msg}"),
            MaterialError::SchemaVersion { file, got } => write!(
                f,
                "{file}: schema_version {got}, oczekiwano {MATERIALS_SCHEMA_VERSION}"
            ),
            MaterialError::DuplicateKey(k) => write!(f, "materiał `{k}` zdefiniowany dwukrotnie"),
            MaterialError::UnknownFlag { key, flag } => {
                write!(f, "materiał `{key}`: nieznana flaga `{flag}`")
            }
            MaterialError::MissingAir => {
                write!(
                    f,
                    "brak materiału `air` — indeks 0 jest dla niego zarezerwowany"
                )
            }
            MaterialError::TooMany(n) => write!(f, "{n} materiałów, limit to 65536"),
        }
    }
}

impl std::error::Error for MaterialError {}

impl From<std::io::Error> for MaterialError {
    fn from(e: std::io::Error) -> Self {
        MaterialError::Io(e)
    }
}

/// Rejestr globalny materiałów.
#[derive(Clone, Debug, Default)]
pub struct MaterialRegistry {
    materials: Vec<VoxelMaterial>,
    by_key: BTreeMap<Box<str>, MaterialId>,
}

impl MaterialRegistry {
    /// Ładuje wszystkie `*.ron` z katalogu. Kolejność plików w systemie plików nie ma
    /// znaczenia: identyfikatory nadaje **posortowany klucz tekstowy** (00 §5), więc
    /// ten sam zestaw danych daje te same `MaterialId` na każdej maszynie.
    pub fn load_dir(dir: &Path) -> Result<MaterialRegistry, MaterialError> {
        let mut defs: Vec<MaterialDef> = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "ron"))
            .collect();
        files.sort();

        for path in files {
            let name = path.display().to_string();
            let text = std::fs::read_to_string(&path)?;
            let file: MaterialFile = ron::from_str(&text).map_err(|e| MaterialError::Parse {
                file: name.clone(),
                msg: e.to_string(),
            })?;
            if file.schema_version != MATERIALS_SCHEMA_VERSION {
                return Err(MaterialError::SchemaVersion {
                    file: name,
                    got: file.schema_version,
                });
            }
            defs.extend(file.materials);
        }
        MaterialRegistry::from_defs(defs)
    }

    fn from_defs(mut defs: Vec<MaterialDef>) -> Result<MaterialRegistry, MaterialError> {
        // Powietrze na pozycję 0, reszta alfabetycznie. Sortowanie jest jedynym źródłem
        // kolejności — plik może wymieniać materiały w dowolnym porządku.
        defs.sort_by(|a, b| {
            let rank = |k: &str| u8::from(k != "air");
            (rank(&a.key), &a.key).cmp(&(rank(&b.key), &b.key))
        });
        if defs.first().map(|d| d.key.as_str()) != Some("air") {
            return Err(MaterialError::MissingAir);
        }
        if defs.len() > u16::MAX as usize + 1 {
            return Err(MaterialError::TooMany(defs.len()));
        }

        let mut materials = Vec::with_capacity(defs.len());
        let mut by_key = BTreeMap::new();
        for (i, d) in defs.into_iter().enumerate() {
            let mut flags = MaterialFlags(0);
            for f in &d.flags {
                let bit = match f.as_str() {
                    "SOLID" => MaterialFlags::SOLID,
                    "LIQUID" => MaterialFlags::LIQUID,
                    "TRANSPARENT" => MaterialFlags::TRANSPARENT,
                    "DIGGABLE" => MaterialFlags::DIGGABLE,
                    "BUILDABLE" => MaterialFlags::BUILDABLE,
                    "SUPPORTS_VEGETATION" => MaterialFlags::SUPPORTS_VEGETATION,
                    other => {
                        return Err(MaterialError::UnknownFlag {
                            key: d.key,
                            flag: other.to_string(),
                        })
                    }
                };
                flags = flags.union(bit);
            }
            let key: Box<str> = d.key.into_boxed_str();
            if by_key.insert(key.clone(), MaterialId(i as u16)).is_some() {
                return Err(MaterialError::DuplicateKey(key.into_string()));
            }
            materials.push(VoxelMaterial {
                key,
                albedo: [d.albedo.0, d.albedo.1, d.albedo.2],
                roughness: d.roughness,
                emissive: d.emissive,
                flags,
                hardness: d.hardness,
                density_kg_m3: d.density_kg_m3,
                bearing_capacity: d.bearing_capacity,
            });
        }
        Ok(MaterialRegistry { materials, by_key })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    /// Materiał po indeksie. Panika przy indeksie spoza rejestru: `MaterialId` pochodzi
    /// zawsze z tego rejestru, więc zły indeks to błąd programu, nie danych.
    #[inline]
    #[must_use]
    pub fn get(&self, id: MaterialId) -> &VoxelMaterial {
        &self.materials[id.index()]
    }

    #[must_use]
    pub fn id_of(&self, key: &str) -> Option<MaterialId> {
        self.by_key.get(key).copied()
    }

    /// Klucz → id, z paniką i czytelnym komunikatem. Do użycia tam, gdzie brak materiału
    /// oznacza niespójne dane, a nie sytuację do obsłużenia (np. warstwa geologiczna).
    #[must_use]
    pub fn expect_id(&self, key: &str) -> MaterialId {
        self.id_of(key)
            .unwrap_or_else(|| panic!("brak materiału `{key}` w data/materials"))
    }

    #[must_use]
    pub fn all(&self) -> &[VoxelMaterial] {
        &self.materials
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejestr_testowy() -> MaterialRegistry {
        MaterialRegistry::from_defs(vec![
            MaterialDef {
                key: "granite".into(),
                albedo: (120, 120, 120),
                roughness: 200,
                emissive: 0,
                flags: vec!["SOLID".into(), "DIGGABLE".into()],
                hardness: 200,
                density_kg_m3: 2700,
                bearing_capacity: 250,
            },
            MaterialDef {
                key: "air".into(),
                albedo: (0, 0, 0),
                roughness: 0,
                emissive: 0,
                flags: vec![],
                hardness: 0,
                density_kg_m3: 0,
                bearing_capacity: 0,
            },
            MaterialDef {
                key: "soil".into(),
                albedo: (90, 70, 50),
                roughness: 220,
                emissive: 0,
                flags: vec![
                    "SOLID".into(),
                    "DIGGABLE".into(),
                    "SUPPORTS_VEGETATION".into(),
                ],
                hardness: 20,
                density_kg_m3: 1500,
                bearing_capacity: 80,
            },
        ])
        .unwrap()
    }

    #[test]
    fn powietrze_dostaje_indeks_zero_niezaleznie_od_alfabetu() {
        let r = rejestr_testowy();
        assert_eq!(r.id_of("air"), Some(MaterialId::AIR));
        // „granite" < „soil" alfabetycznie, a oba są za powietrzem.
        assert_eq!(r.id_of("granite"), Some(MaterialId(1)));
        assert_eq!(r.id_of("soil"), Some(MaterialId(2)));
        assert!(!r.get(MaterialId::AIR).is_solid());
        assert!(r.get(r.expect_id("granite")).is_opaque());
    }

    #[test]
    fn brak_powietrza_to_blad_danych() {
        let e = MaterialRegistry::from_defs(vec![MaterialDef {
            key: "rock".into(),
            albedo: (1, 1, 1),
            roughness: 0,
            emissive: 0,
            flags: vec![],
            hardness: 0,
            density_kg_m3: 0,
            bearing_capacity: 0,
        }])
        .unwrap_err();
        assert!(matches!(e, MaterialError::MissingAir));
    }

    #[test]
    fn nieznana_flaga_lamie_ladowanie() {
        let e = MaterialRegistry::from_defs(vec![
            MaterialDef {
                key: "air".into(),
                albedo: (0, 0, 0),
                roughness: 0,
                emissive: 0,
                flags: vec![],
                hardness: 0,
                density_kg_m3: 0,
                bearing_capacity: 0,
            },
            MaterialDef {
                key: "x".into(),
                albedo: (0, 0, 0),
                roughness: 0,
                emissive: 0,
                flags: vec!["FLOATS".into()],
                hardness: 0,
                density_kg_m3: 0,
                bearing_capacity: 0,
            },
        ])
        .unwrap_err();
        assert!(matches!(e, MaterialError::UnknownFlag { .. }), "{e}");
    }
}
