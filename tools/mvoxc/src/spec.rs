//! Specyfikacja importu obok pliku `.vox` (M11a, WP1).
//!
//! `.vox` niesie kształty i barwy. Nie niesie **niczego z tego, co jest modelem gry**:
//! która bryła jest ramieniem, do czego jest przyczepiona, gdzie ma staw, w którym
//! poziomie detalu istnieje i który indeks barwy jest którą rolą palety. Tego nie da się
//! zgadnąć z pliku i nie da się tego wpisać w edytorze — więc stoi w `<model>.ron`
//! leżącym obok `<model>.vox`, w RON-ie jak reszta danych (00 §5).
//!
//! Kolejność `parts` w specyfikacji odpowiada kolejności par `SIZE`/`XYZI` w `.vox` —
//! to jedyny porządek, który autor modelu widzi w edytorze.

use magnat_voxel::{ModelFlags, ModelKind, PaletteSlot, PartName, SlotRole};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const SPEC_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
pub struct ImportSpec {
    pub schema_version: u32,
    /// `character` | `vehicle` | `prop` | `sign` | `machine`.
    pub kind: String,
    #[serde(default)]
    pub flags: Vec<String>,
    /// Deklaracja slotów: indeks 1..15 → rola palety.
    pub slots: BTreeMap<u8, String>,
    /// Indeks barwy MagicaVoxela → indeks slotu. Barwa spoza tej mapy jest **błędem**,
    /// nie pustką: voxel pomalowany kolorem, którego nikt nie przypisał, znikałby
    /// z modelu po cichu.
    pub colors: BTreeMap<u8, u8>,
    pub parts: Vec<PartSpec>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PartSpec {
    pub name: String,
    /// Nazwa części nadrzędnej; pusto = korzeń. Rodzic musi stać **przed** dzieckiem.
    ///
    /// Pusty łańcuch zamiast `Option`, bo RON bez rozszerzenia `implicit_some` wymagałby
    /// w danych `parent: Some("torso")` — a specyfikację pisze człowiek obok modelu.
    #[serde(default)]
    pub parent: String,
    /// Staw w ćwiartkach voxela, względem układu rodzica.
    pub pivot: (i16, i16, i16),
    /// Poziomy detalu, w których część istnieje.
    pub lods: Vec<u8>,
}

impl ImportSpec {
    pub fn parse(text: &str) -> Result<ImportSpec, String> {
        let s: ImportSpec = ron::from_str(text).map_err(|e| e.to_string())?;
        if s.schema_version != SPEC_SCHEMA_VERSION {
            return Err(format!(
                "schema_version {}, oczekiwano {SPEC_SCHEMA_VERSION}",
                s.schema_version
            ));
        }
        Ok(s)
    }

    pub fn kind(&self) -> Result<ModelKind, String> {
        Ok(match self.kind.as_str() {
            "character" => ModelKind::Character,
            "vehicle" => ModelKind::Vehicle,
            "prop" => ModelKind::Prop,
            "sign" => ModelKind::Sign,
            "machine" => ModelKind::Machine,
            other => return Err(format!("nieznany rodzaj modelu `{other}`")),
        })
    }

    pub fn flags(&self) -> Result<ModelFlags, String> {
        let mut f = ModelFlags::default();
        for n in &self.flags {
            f = f.union(match n.as_str() {
                "HasDoors" => ModelFlags::HAS_DOORS,
                "HasWheels" => ModelFlags::HAS_WHEELS,
                "Emissive" => ModelFlags::EMISSIVE,
                "TwoSided" => ModelFlags::TWO_SIDED,
                other => return Err(format!("nieznana flaga `{other}`")),
            });
        }
        Ok(f)
    }

    pub fn slots(&self) -> Result<Vec<PaletteSlot>, String> {
        self.slots
            .iter()
            .map(|(slot, rola)| {
                SlotRole::from_key(rola)
                    .map(|role| PaletteSlot { slot: *slot, role })
                    .ok_or_else(|| format!("nieznana rola slotu `{rola}`"))
            })
            .collect()
    }

    pub fn part_name(&self, i: usize) -> Result<PartName, String> {
        let n = &self.parts[i].name;
        PartName::from_key(n).ok_or_else(|| {
            format!("nieznana nazwa części `{n}` — dopisz ją do `PartName` albo użyj `part_<n>`")
        })
    }

    /// Indeks rodzica w tablicy części albo [`magnat_voxel::Part::NO_PARENT`].
    pub fn parent_index(&self, i: usize) -> Result<u8, String> {
        let rodzic = &self.parts[i].parent;
        if rodzic.is_empty() {
            return Ok(magnat_voxel::Part::NO_PARENT);
        }
        let j = self.parts[..i]
            .iter()
            .position(|p| &p.name == rodzic)
            .ok_or_else(|| {
                format!(
                    "część `{}`: rodzic `{rodzic}` nie stoi przed nią na liście",
                    self.parts[i].name
                )
            })?;
        Ok(j as u8)
    }

    /// Maska poziomów detalu. Numer spoza `0..LOD_COUNT` jest **błędem specyfikacji**:
    /// `1 << 8` to przepełnienie przesunięcia, a `1 << 3` ustawiłoby po cichu bit,
    /// którego nikt nie czyta — czyli część znikałaby z modelu bez śladu.
    pub fn lod_mask(&self, i: usize) -> Result<u8, String> {
        let mut m = 0u8;
        for l in &self.parts[i].lods {
            if *l >= magnat_voxel::LOD_COUNT {
                return Err(format!(
                    "część `{}`: poziom detalu {l}, format ma {}",
                    self.parts[i].name,
                    magnat_voxel::LOD_COUNT
                ));
            }
            m |= 1 << l;
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"(
        schema_version: 1,
        kind: "character",
        slots: {1: "skin", 2: "outfit_main"},
        colors: {1: 1, 2: 2},
        parts: [
            (name: "torso", pivot: (0,0,16), lods: [0,1]),
            (name: "head", parent: "torso", pivot: (0,0,8), lods: [0,1]),
        ],
    )"#;

    #[test]
    fn czyta_hierarchie_maske_i_sloty() {
        let s = ImportSpec::parse(SPEC).expect("parse");
        assert_eq!(s.kind().unwrap(), ModelKind::Character);
        assert_eq!(s.part_name(1).unwrap(), PartName::HEAD);
        assert_eq!(s.parent_index(0).unwrap(), magnat_voxel::Part::NO_PARENT);
        assert_eq!(s.parent_index(1).unwrap(), 0);
        assert_eq!(s.lod_mask(0).unwrap(), 0b011);
        assert_eq!(s.slots().unwrap().len(), 2);
    }

    #[test]
    fn poziom_detalu_spoza_formatu_jest_bledem() {
        let zly = SPEC.replace("lods: [0,1]", "lods: [0,8]");
        let s = ImportSpec::parse(&zly).expect("parse");
        assert!(s.lod_mask(0).is_err(), "numer 8 przeszedł");
    }

    #[test]
    fn rodzic_za_dzieckiem_jest_bledem_specyfikacji() {
        let zly = SPEC
            .replace(r#"(name: "torso", pivot: (0,0,16), lods: [0,1]),"#, "")
            .replace(
                r#"parts: ["#,
                r#"parts: [(name: "torso", parent: "head", pivot: (0,0,16), lods: [0,1]),"#,
            );
        let s = ImportSpec::parse(&zly).expect("parse");
        assert!(s.parent_index(0).is_err());
    }
}
