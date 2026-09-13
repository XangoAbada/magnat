//! Kolumna geologiczna (M1 §5.5, przebieg P8).
//!
//! **Geologia jest funkcją, nie danymi.** Stos warstw liczy się analitycznie na żądanie
//! z wysokości, nachylenia, bliskości wody i szumu — i dlatego zajmuje 0 B pamięci
//! na mapie 16 km, gdzie składowanie warstw per komórka kosztowałoby kilkaset megabajtów.
//! Trwałe jest tylko ~2 KB definicji warstw z `data/geology/`.
//!
//! Kolejność warstw jest kolejnością z pliku, od powierzchni w dół. Ostatnia warstwa jest
//! podłożem i sięga do dna świata — inaczej kolumna miałaby dziurę, a kopalnia M6 wydrążyłaby
//! w niej pustkę bez materiału.

use crate::noise::{fbm, FbmSpec, NoiseField};
use magnat_voxel::{MaterialId, MaterialRegistry};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Wysokość voxela w decymetrach — jednostka `z` w kolumnie (0,5 m, PRD §4.2).
pub const VOXEL_DM: i32 = 5;

pub const GEOLOGY_SCHEMA_VERSION: u32 = 1;

/// Modulacja miąższości warstwy szumem przestrzennym.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct NoiseSpec {
    pub octaves: u8,
    pub wavelength_m: f32,
    /// Amplituda jako ułamek miąższości bazowej, 0..=1.
    pub amplitude: f32,
}

/// Warstwa geologiczna — definiowana w `data/geology/*.ron`, nie hardkodowana.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TerrainLayer {
    /// Klucz tekstowy materiału — **to** jest tożsamość warstwy w zapisie gry (00 §5).
    pub material_key: Box<str>,
    /// Indeks rozwiązany przy ładowaniu.
    pub material: MaterialId,
    pub thickness_base_cm: u32,
    pub thickness_noise: NoiseSpec,
    /// 0..1 — przy jakim nachyleniu warstwa znika. Gleba nie utrzymuje się na stromiźnie.
    pub slope_falloff: f32,
    pub elev_min_m: i16,
    pub elev_max_m: i16,
    /// −100..100 — torf i muł tylko blisko wody, less tylko daleko od niej.
    pub water_affinity: i8,
}

/// Kolumna geologiczna: wynik, nie dane.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ColumnStack {
    /// Powierzchnia terenu w jednostkach 0,5 m.
    pub surface_z: i32,
    /// `(materiał, dolna granica Z)`, od góry w dół.
    pub layers: SmallVec<[(MaterialId, i32); 8]>,
    pub water_table_z: i32,
}

impl ColumnStack {
    /// Materiał na zadanej wysokości. `None` powyżej powierzchni.
    #[must_use]
    pub fn material_at(&self, z: i32) -> Option<MaterialId> {
        if z > self.surface_z {
            return None;
        }
        for (m, bottom) in &self.layers {
            if z >= *bottom {
                return Some(*m);
            }
        }
        self.layers.last().map(|(m, _)| *m)
    }

    /// Miąższość warstwy przykrywającej dany materiał, w jednostkach 0,5 m.
    #[must_use]
    pub fn depth_to(&self, material: MaterialId) -> Option<i32> {
        let mut top = self.surface_z;
        for (m, bottom) in &self.layers {
            if *m == material {
                return Some(self.surface_z - top);
            }
            top = *bottom;
        }
        None
    }
}

#[derive(Clone, Debug, Deserialize)]
struct LayerDef {
    material: String,
    thickness_base_cm: u32,
    #[serde(default = "default_noise")]
    thickness_noise: NoiseSpec,
    #[serde(default)]
    slope_falloff: f32,
    #[serde(default = "default_elev_min")]
    elev_min_m: i16,
    #[serde(default = "default_elev_max")]
    elev_max_m: i16,
    #[serde(default)]
    water_affinity: i8,
}

const fn default_noise() -> NoiseSpec {
    NoiseSpec {
        octaves: 3,
        wavelength_m: 800.0,
        amplitude: 0.4,
    }
}
const fn default_elev_min() -> i16 {
    -1000
}
const fn default_elev_max() -> i16 {
    1000
}

#[derive(Clone, Debug, Deserialize)]
struct GeologyFile {
    schema_version: u32,
    layers: Vec<LayerDef>,
    bedrock: String,
}

/// Model geologiczny świata: warstwy od powierzchni w dół plus podłoże.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, Default)]
pub struct GeologyModel {
    pub layers: Vec<TerrainLayer>,
    pub bedrock_key: Box<str>,
    pub bedrock: MaterialId,
}

impl GeologyModel {
    pub fn load(path: &std::path::Path, reg: &MaterialRegistry) -> Result<GeologyModel, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file: GeologyFile = ron::from_str(&text).map_err(|e| e.to_string())?;
        if file.schema_version != GEOLOGY_SCHEMA_VERSION {
            return Err(format!(
                "{}: schema_version {}, oczekiwano {GEOLOGY_SCHEMA_VERSION}",
                path.display(),
                file.schema_version
            ));
        }
        let resolve = |k: &str| {
            reg.id_of(k)
                .ok_or_else(|| format!("nieznany materiał `{k}` w {}", path.display()))
        };

        let mut layers = Vec::with_capacity(file.layers.len());
        for d in file.layers {
            layers.push(TerrainLayer {
                material: resolve(&d.material)?,
                material_key: d.material.into_boxed_str(),
                thickness_base_cm: d.thickness_base_cm,
                thickness_noise: d.thickness_noise,
                slope_falloff: d.slope_falloff,
                elev_min_m: d.elev_min_m,
                elev_max_m: d.elev_max_m,
                water_affinity: d.water_affinity,
            });
        }
        Ok(GeologyModel {
            layers,
            bedrock: resolve(&file.bedrock)?,
            bedrock_key: file.bedrock.into_boxed_str(),
        })
    }

    /// Kolumna geologiczna w punkcie. Bezalokacyjna poza `SmallVec` mieszczącym 8 warstw.
    ///
    /// Argumenty są celowo prymitywne, a nie referencją do świata: dzięki temu funkcję można
    /// wołać z dowolnego workera przy materializacji chunków, bez pożyczania całego terenu.
    #[must_use]
    pub fn column_at(
        &self,
        noise: &NoiseField,
        x: i32,
        y: i32,
        surface_dm: i32,
        slope: u8,
        water_dist_m: u16,
    ) -> ColumnStack {
        let surface_z = surface_dm / VOXEL_DM;
        let elev_m = surface_dm / 10;
        let slope_frac = f32::from(slope) / 255.0;

        let mut stack: SmallVec<[(MaterialId, i32); 8]> = SmallVec::new();
        let mut top = surface_z;

        for layer in &self.layers {
            if elev_m < i32::from(layer.elev_min_m) || elev_m > i32::from(layer.elev_max_m) {
                continue;
            }
            // Nachylenie zdejmuje warstwę liniowo aż do zera przy `slope_falloff`.
            let slope_factor = if layer.slope_falloff <= 0.0 {
                1.0
            } else {
                (1.0 - slope_frac / layer.slope_falloff).clamp(0.0, 1.0)
            };
            if slope_factor <= 0.0 {
                continue;
            }
            // Powinowactwo do wody: dodatnie = warstwa tylko blisko koryta, ujemne = tylko daleko.
            let water_factor = water_factor(layer.water_affinity, water_dist_m);
            if water_factor <= 0.0 {
                continue;
            }

            let spec = FbmSpec::new(
                layer.thickness_noise.octaves,
                layer.thickness_noise.wavelength_m,
            );
            let n = fbm(noise, x as f32, y as f32, spec);
            let modulacja = 1.0 + n * layer.thickness_noise.amplitude;

            let cm = layer.thickness_base_cm as f32 * modulacja * slope_factor * water_factor;
            // Miąższość w jednostkach 0,5 m = 50 cm.
            let grubosc_z = (cm / 50.0) as i32;
            if grubosc_z <= 0 {
                continue;
            }
            top -= grubosc_z;
            stack.push((layer.material, top));
        }

        // Podłoże sięga dna świata — kolumna nie ma prawa mieć dziury.
        stack.push((self.bedrock, i32::MIN / 2));

        ColumnStack {
            surface_z,
            layers: stack,
            water_table_z: water_table_z(surface_z, slope, water_dist_m),
        }
    }
}

/// Wpływ bliskości wody na obecność warstwy.
fn water_factor(affinity: i8, water_dist_m: u16) -> f32 {
    if affinity == 0 {
        return 1.0;
    }
    // Zasięg oddziaływania koryta: 400 m. Dalej warstwa wodolubna znika, a wodowstrętna
    // osiąga pełną miąższość.
    let blisko = (1.0 - f32::from(water_dist_m.min(400)) / 400.0).clamp(0.0, 1.0);
    let a = f32::from(affinity) / 100.0;
    if a > 0.0 {
        (blisko * a + (1.0 - a)).clamp(0.0, 1.0) * if blisko > 0.0 { 1.0 } else { 0.0 }
    } else {
        (1.0 + a * blisko).clamp(0.0, 1.0)
    }
}

/// Głębokość zwierciadła wód gruntowych w jednostkach 0,5 m.
///
/// Model jest celowo prosty: woda gruntowa stoi płytko w dolinach przy ciekach i głęboko
/// na stromych wyniesieniach. Pełny model hydrogeologiczny nie ma w tej grze konsumenta —
/// M2 potrzebuje tego do kosztu fundamentów, M6 do studni, i obu wystarcza rząd wielkości.
fn water_table_z(surface_z: i32, slope: u8, water_dist_m: u16) -> i32 {
    let blisko = (1.0 - f32::from(water_dist_m.min(800)) / 800.0).clamp(0.0, 1.0);
    let stromo = f32::from(slope) / 255.0;
    // Od 0,5 m pod powierzchnią przy korycie do 20 m na stromym grzbiecie.
    let glebokosc_m = 0.5 + (1.0 - blisko) * 12.0 + stromo * 8.0;
    surface_z - (glebokosc_m * 2.0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::StreamId;

    fn model() -> (GeologyModel, MaterialRegistry) {
        let reg =
            MaterialRegistry::load_dir(&crate::data_path("materials")).expect("data/materials");
        let m = GeologyModel::load(&crate::data_path("geology/layers.ron"), &reg)
            .expect("data/geology/layers.ron");
        (m, reg)
    }

    #[test]
    fn kolumna_nie_ma_dziur_i_konczy_sie_podlozem() {
        let (m, _) = model();
        let noise = NoiseField::new(1, StreamId::WorldGeology, 0);
        for (slope, woda) in [(0u8, 10u16), (60, 200), (200, 5000)] {
            let c = m.column_at(&noise, 1000, 1000, 400, slope, woda);
            assert!(!c.layers.is_empty());
            assert_eq!(
                c.layers.last().unwrap().0,
                m.bedrock,
                "kolumna nie kończy się podłożem"
            );
            // Granice warstw schodzą monotonicznie w dół.
            let mut prev = c.surface_z;
            for (_, bottom) in &c.layers {
                assert!(
                    *bottom < prev,
                    "granica warstwy nie schodzi: {bottom} po {prev}"
                );
                prev = *bottom;
            }
            // Każda wysokość poniżej powierzchni ma materiał.
            for z in (c.surface_z - 60)..=c.surface_z {
                assert!(c.material_at(z).is_some(), "dziura na z = {z}");
            }
            assert!(
                c.material_at(c.surface_z + 1).is_none(),
                "materiał nad terenem"
            );
        }
    }

    #[test]
    fn gleba_nie_utrzymuje_sie_na_stromiznie() {
        // M1 §7.2 `soil_on_slopes`.
        let (m, reg) = model();
        let noise = NoiseField::new(1, StreamId::WorldGeology, 0);
        let gleba = reg.expect_id("soil");
        let plasko = m.column_at(&noise, 500, 500, 300, 5, 300);
        let stromo = m.column_at(&noise, 500, 500, 300, 250, 300);
        assert!(
            plasko.layers.iter().any(|(mm, _)| *mm == gleba),
            "brak gleby na płaskim"
        );
        assert!(
            !stromo.layers.iter().any(|(mm, _)| *mm == gleba),
            "gleba utrzymała się na nachyleniu 250/255"
        );
    }

    #[test]
    fn torf_wystepuje_tylko_przy_wodzie() {
        let (m, reg) = model();
        let noise = NoiseField::new(1, StreamId::WorldGeology, 0);
        let torf = reg.expect_id("peat");
        let przy_wodzie = m.column_at(&noise, 700, 700, 120, 3, 20);
        let daleko = m.column_at(&noise, 700, 700, 120, 3, 3000);
        assert!(przy_wodzie.layers.iter().any(|(mm, _)| *mm == torf));
        assert!(!daleko.layers.iter().any(|(mm, _)| *mm == torf));
    }

    #[test]
    fn woda_gruntowa_stoi_plycej_w_dolinie_niz_na_grzbiecie() {
        let (m, _) = model();
        let noise = NoiseField::new(1, StreamId::WorldGeology, 0);
        let dolina = m.column_at(&noise, 0, 0, 200, 5, 30);
        let grzbiet = m.column_at(&noise, 0, 0, 200, 220, 2000);
        assert!(
            dolina.surface_z - dolina.water_table_z < grzbiet.surface_z - grzbiet.water_table_z,
            "woda gruntowa nie reaguje na położenie w rzeźbie"
        );
    }

    #[test]
    fn kolumna_jest_czysta_funkcja() {
        let (m, _) = model();
        let noise = NoiseField::new(99, StreamId::WorldGeology, 0);
        let a = m.column_at(&noise, 123, 456, 350, 40, 150);
        let b = m.column_at(&noise, 123, 456, 350, 40, 150);
        assert_eq!(a, b);
    }
}
