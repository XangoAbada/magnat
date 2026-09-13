//! Zapis i odczyt świata `.mgw` (M1 §4 WP-W7).
//!
//! Do pliku idzie **wyłącznie** stan trwały z [`crate::data::WorldData`]. Reszta terenu —
//! geologia, materializacja 1 m, kolumny voxeli — jest funkcją seeda i liczy się na żądanie,
//! więc jej zapisywanie byłoby płaceniem gigabajtami za coś, co odtwarza się w sekundy
//! (M1 §6.1, kontrakt dla M12).
//!
//! Format jest ten sam co w `engine/io`: bincode + zstd. Nie dziedziczymy po `engine/io`
//! bezpośrednio, bo tam zapisywany jest świat ECS, a tu siatki — wspólny byłby tylko
//! nagłówek, a to za mało na abstrakcję.

use crate::data::WorldData;
use std::io;
use std::path::Path;

/// Magiczna liczba nagłówka — „MGW1" little-endian. Plik bez niej nie jest światem
/// i lepiej to powiedzieć od razu niż po rozpakowaniu 30 MB śmieci.
const MAGIC: [u8; 4] = *b"MGW1";
/// Wersja formatu. Podniesienie wymusza migrację po stronie M12.
pub const MGW_VERSION: u16 = 1;
/// Poziom kompresji zstd. 3 to punkt, w którym dalsze podnoszenie kosztuje sekundy,
/// a zyskuje procenty.
const ZSTD_LEVEL: i32 = 3;

#[derive(Debug)]
pub enum WorldIoError {
    Io(io::Error),
    Codec(String),
    BadMagic,
    Version(u16),
}

impl std::fmt::Display for WorldIoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldIoError::Io(e) => write!(f, "{e}"),
            WorldIoError::Codec(m) => write!(f, "niepoprawna zawartość .mgw: {m}"),
            WorldIoError::BadMagic => write!(f, "to nie jest plik .mgw"),
            WorldIoError::Version(v) => {
                write!(f, "wersja formatu {v}, obsługiwana {MGW_VERSION}")
            }
        }
    }
}

impl std::error::Error for WorldIoError {}

impl From<io::Error> for WorldIoError {
    fn from(e: io::Error) -> Self {
        WorldIoError::Io(e)
    }
}

/// Zapisuje świat. Zwraca rozmiar pliku w bajtach — budżet z M1 §5.9 jest asercją, nie sugestią,
/// więc wywołujący ma czym go sprawdzić.
pub fn save_mgw(world: &WorldData, path: &Path) -> Result<usize, WorldIoError> {
    let raw = bincode::serde::encode_to_vec(world, bincode::config::standard())
        .map_err(|e| WorldIoError::Codec(e.to_string()))?;
    let packed = zstd::encode_all(&raw[..], ZSTD_LEVEL)?;

    let mut out = Vec::with_capacity(packed.len() + 6);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&MGW_VERSION.to_le_bytes());
    out.extend_from_slice(&packed);
    std::fs::write(path, &out)?;
    Ok(out.len())
}

pub fn load_mgw(path: &Path) -> Result<WorldData, WorldIoError> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 6 || bytes[..4] != MAGIC {
        return Err(WorldIoError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != MGW_VERSION {
        return Err(WorldIoError::Version(version));
    }
    let raw = zstd::decode_all(&bytes[6..])?;
    let (world, _) = bincode::serde::decode_from_slice(&raw, bincode::config::standard())
        .map_err(|e| WorldIoError::Codec(e.to_string()))?;
    Ok(world)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{WaterBits, WaterClass};
    use crate::params::WorldGenParams;
    use magnat_core::hash::StateHasher;

    #[test]
    fn zapis_i_odczyt_zachowuja_hash() {
        let mut w = WorldData::empty(WorldGenParams::default());
        for i in 0..w.height.len() {
            w.height[i] = (i % 997) as i16 - 400;
            w.water[i] = WaterBits::new(
                if w.height[i] < 0 {
                    WaterClass::Sea
                } else {
                    WaterClass::Dry
                },
                (i % 8) as u8,
                false,
            );
        }
        let mut h1 = StateHasher::new();
        w.hash_state(&mut h1);

        let dir = std::env::temp_dir().join("magnat-mgw-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("world.mgw");
        let bytes = save_mgw(&w, &path).unwrap();
        assert!(bytes > 0);

        let back = load_mgw(&path).unwrap();
        let mut h2 = StateHasher::new();
        back.hash_state(&mut h2);
        assert_eq!(h1.finish(), h2.finish(), "round-trip zmienił stan świata");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn obcy_plik_nie_udaje_swiata() {
        let dir = std::env::temp_dir().join("magnat-mgw-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("obcy.bin");
        std::fs::write(&path, b"to nie jest swiat").unwrap();
        assert!(matches!(load_mgw(&path), Err(WorldIoError::BadMagic)));
        std::fs::remove_file(&path).ok();
    }
}
