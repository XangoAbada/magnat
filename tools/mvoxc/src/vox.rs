//! Czytnik MagicaVoxel `.vox` (M11a, WP1).
//!
//! Format jest drzewem kawałków RIFF-podobnych: `VOX ` + wersja, potem `MAIN`
//! z dziećmi. Interesują nas trzy rodzaje dzieci i tylko one:
//!
//! - `SIZE` — wymiary następnego modelu,
//! - `XYZI` — jego voxele jako `(x, y, z, index)`,
//! - `RGBA` — paleta 256 barw; czytamy ją wyłącznie po to, żeby `mvoxc` umiał
//!   **pokazać człowiekowi**, którym barwom przypisuje jakie sloty.
//!
//! Plik z wieloma parami `SIZE`/`XYZI` niesie wiele modeli i tak właśnie zapisuje się
//! postać: **jedna para na część**. Kolejność par jest kolejnością części w `.mvox`,
//! bo to jedyny porządek, który autor modelu widzi w edytorze.
//!
//! Czego nie czytamy i dlaczego: `nTRN`/`nGRP`/`nSHP` (graf sceny z przesunięciami)
//! i `MATL` (materiały). Przesunięcie części jest u nas **pivotem stawu** i pochodzi
//! ze specyfikacji obok pliku, a nie z edytora — staw trzeba nazwać i przypisać
//! rodzica, czego graf sceny MagicaVoxela nie wyraża. Materiał nie ma tu znaczenia:
//! barwę podstawia paleta gry, nie model (§5.1).

use std::fmt;

/// Jeden model z pliku `.vox` — u nas kandydat na część.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VoxShape {
    pub dims: [u32; 3],
    /// `(x, y, z, color_index)`; indeks 0 nie występuje — MagicaVoxel liczy od 1.
    pub voxels: Vec<[u8; 4]>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VoxFile {
    pub version: u32,
    pub shapes: Vec<VoxShape>,
    /// Paleta 1..255; wpis 0 jest pustką i zawsze zerowy.
    pub palette: [[u8; 4]; 256],
}

#[derive(Debug)]
pub enum VoxError {
    BadMagic,
    Truncated(&'static str),
    NoShapes,
    TooBig { dim: u32 },
    SizeWithoutVoxels,
}

impl fmt::Display for VoxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VoxError::BadMagic => write!(f, "to nie jest plik MagicaVoxel (.vox)"),
            VoxError::Truncated(co) => write!(f, "plik urwany przy: {co}"),
            VoxError::NoShapes => write!(f, "plik nie zawiera ani jednego modelu"),
            VoxError::TooBig { dim } => {
                write!(
                    f,
                    "wymiar {dim} voxeli przekracza 255 — część musi się zmieścić w bajcie"
                )
            }
            VoxError::SizeWithoutVoxels => write!(f, "kawałek SIZE bez odpowiadającego XYZI"),
        }
    }
}

impl std::error::Error for VoxError {}

pub fn read(buf: &[u8]) -> Result<VoxFile, VoxError> {
    if buf.len() < 8 || &buf[..4] != b"VOX " {
        return Err(VoxError::BadMagic);
    }
    let version = u32(buf, 4)?;
    let mut shapes: Vec<VoxShape> = Vec::new();
    let mut palette = [[0u8; 4]; 256];
    let mut oczekiwane: Option<[u32; 3]> = None;

    // `MAIN` zaczyna się na 8; jego zawartość jest pusta, a dzieci lecą jedno za drugim.
    // Przechodzimy je płasko: żaden z trzech interesujących kawałków nie ma własnych dzieci.
    let mut at = 8usize;
    let (main_content, _) = naglowek(buf, at)?;
    at += 12 + main_content as usize;

    while at + 12 <= buf.len() {
        let id = &buf[at..at + 4];
        let (content, children) = naglowek(buf, at)?;
        let start = at + 12;
        let koniec = start + content as usize;
        if koniec > buf.len() {
            return Err(VoxError::Truncated("zawartość kawałka"));
        }
        match id {
            b"SIZE" => {
                let d = [u32(buf, start)?, u32(buf, start + 4)?, u32(buf, start + 8)?];
                for v in d {
                    if v > 255 {
                        return Err(VoxError::TooBig { dim: v });
                    }
                }
                if oczekiwane.is_some() {
                    return Err(VoxError::SizeWithoutVoxels);
                }
                oczekiwane = Some(d);
            }
            b"XYZI" => {
                let dims = oczekiwane
                    .take()
                    .ok_or(VoxError::Truncated("XYZI bez SIZE"))?;
                let n = u32(buf, start)? as usize;
                if start + 4 + n * 4 > koniec {
                    return Err(VoxError::Truncated("lista voxeli"));
                }
                let mut voxels = Vec::with_capacity(n);
                for i in 0..n {
                    let o = start + 4 + i * 4;
                    voxels.push([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
                }
                shapes.push(VoxShape { dims, voxels });
            }
            b"RGBA" => {
                // Paleta MagicaVoxela jest przesunięta o jeden: bajty 0..3 opisują
                // indeks 1. To nie jest ozdobnik — bez przesunięcia `mvoxc import`
                // raportowałby człowiekowi cudze barwy.
                for i in 0..255usize {
                    let o = start + i * 4;
                    if o + 4 > koniec {
                        break;
                    }
                    palette[i + 1] = [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]];
                }
            }
            _ => {}
        }
        at = koniec + children as usize;
    }

    if oczekiwane.is_some() {
        return Err(VoxError::SizeWithoutVoxels);
    }
    if shapes.is_empty() {
        return Err(VoxError::NoShapes);
    }
    Ok(VoxFile {
        version,
        shapes,
        palette,
    })
}

fn naglowek(buf: &[u8], at: usize) -> Result<(u32, u32), VoxError> {
    if at + 12 > buf.len() {
        return Err(VoxError::Truncated("nagłówek kawałka"));
    }
    Ok((u32(buf, at + 4)?, u32(buf, at + 8)?))
}

fn u32(buf: &[u8], at: usize) -> Result<u32, VoxError> {
    if at + 4 > buf.len() {
        return Err(VoxError::Truncated("liczba 32-bitowa"));
    }
    Ok(u32::from_le_bytes([
        buf[at],
        buf[at + 1],
        buf[at + 2],
        buf[at + 3],
    ]))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Buduje minimalny poprawny `.vox` z jednym modelem — wzorzec do testów importu.
    pub(crate) fn maly_vox(dims: [u32; 3], voxels: &[[u8; 4]]) -> Vec<u8> {
        let mut dzieci: Vec<u8> = Vec::new();
        let kawalek = |id: &[u8; 4], tresc: Vec<u8>, out: &mut Vec<u8>| {
            out.extend_from_slice(id);
            out.extend_from_slice(&(tresc.len() as u32).to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&tresc);
        };
        let mut size = Vec::new();
        for d in dims {
            size.extend_from_slice(&d.to_le_bytes());
        }
        kawalek(b"SIZE", size, &mut dzieci);
        let mut xyzi = (voxels.len() as u32).to_le_bytes().to_vec();
        for v in voxels {
            xyzi.extend_from_slice(v);
        }
        kawalek(b"XYZI", xyzi, &mut dzieci);
        let mut rgba = Vec::new();
        for i in 0..256 {
            rgba.extend_from_slice(&[i as u8, 0, 0, 255]);
        }
        kawalek(b"RGBA", rgba, &mut dzieci);

        let mut out = b"VOX ".to_vec();
        out.extend_from_slice(&150u32.to_le_bytes());
        out.extend_from_slice(b"MAIN");
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(dzieci.len() as u32).to_le_bytes());
        out.extend_from_slice(&dzieci);
        out
    }

    #[test]
    fn czyta_wymiary_voxele_i_palete() {
        let plik = maly_vox([2, 3, 4], &[[0, 0, 0, 1], [1, 2, 3, 7]]);
        let v = read(&plik).expect("odczyt");
        assert_eq!(v.version, 150);
        assert_eq!(v.shapes.len(), 1);
        assert_eq!(v.shapes[0].dims, [2, 3, 4]);
        assert_eq!(v.shapes[0].voxels.len(), 2);
        // Przesunięcie palety o jeden: bajty indeksu 0 opisują barwę nr 1.
        assert_eq!(v.palette[1][0], 0);
        assert_eq!(v.palette[7][0], 6);
        assert_eq!(v.palette[0], [0, 0, 0, 0]);
    }

    #[test]
    fn urwany_plik_nie_panikuje() {
        let plik = maly_vox([2, 2, 2], &[[0, 0, 0, 1]]);
        for n in 0..plik.len() {
            let _ = read(&plik[..n]);
        }
    }

    #[test]
    fn model_wiekszy_niz_255_jest_bledem() {
        let plik = maly_vox([300, 2, 2], &[]);
        assert!(matches!(read(&plik), Err(VoxError::TooBig { dim: 300 })));
    }

    #[test]
    fn nie_jest_voxem() {
        assert!(matches!(read(b"RIFF...."), Err(VoxError::BadMagic)));
    }
}
