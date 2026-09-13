//! Zapis PNG bez zależności zewnętrznej.
//!
//! PNG wymaga strumienia zlib, a zlib dopuszcza bloki **nieskompresowane** (`BTYPE = 00`).
//! Podgląd generatora zapisuje się raz na uruchomienie i ogląda w podglądzie systemowym,
//! więc rozmiar pliku nie ma tu znaczenia — a brak biblioteki kompresji w grafie zależności
//! narzędzia headless ma (00 §6: headless ma działać wszędzie, także na maszynie CI bez GPU).
//!
//! Jeśli kiedyś rozmiar zacznie przeszkadzać, wymiana idzie na `zstd` po stronie własnego
//! formatu podglądu albo na `flate2` — kontrakt funkcji się nie zmienia.

use std::io::{self, Write};
use std::path::Path;

const CRC_POLY: u32 = 0xEDB8_8320;

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in bytes {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ CRC_POLY
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for x in bytes {
        a = (a + u32::from(*x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// Zapisuje obraz RGB8. `pixels` ma mieć `w * h * 3` bajtów.
pub fn write_rgb(path: &Path, w: u32, h: u32, pixels: &[u8]) -> io::Result<usize> {
    assert_eq!(
        pixels.len(),
        (w * h * 3) as usize,
        "png: zły rozmiar bufora"
    );

    // Surowy strumień: każdy wiersz poprzedzony bajtem filtra 0 (brak filtra).
    let mut raw = Vec::with_capacity(pixels.len() + h as usize);
    for y in 0..h as usize {
        raw.push(0u8);
        let off = y * w as usize * 3;
        raw.extend_from_slice(&pixels[off..off + w as usize * 3]);
    }

    // zlib: nagłówek 0x78 0x01 (deflate, brak kompresji), bloki „stored" po ≤ 65535 B.
    let mut z = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65_535).enumerate() {
        let last = u8::from((i + 1) * 65_535 >= raw.len());
        z.push(last);
        z.extend_from_slice(&(block.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(z.len() + 128);
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bitów, truecolor, deflate, filtr 0, bez przeplotu
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);

    let mut f = std::fs::File::create(path)?;
    f.write_all(&png)?;
    Ok(png.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_i_adler_zgadzaja_sie_z_wektorami_referencyjnymi() {
        // CRC-32 z „IEND" + pusta zawartość to stała znana z każdego pliku PNG.
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        // Adler-32 z „abc" wg RFC 1950.
        assert_eq!(adler32(b"abc"), 0x024D_0127);
    }

    #[test]
    fn zapisany_plik_ma_sygnature_i_wszystkie_bloki() {
        let dir = std::env::temp_dir().join("magnat-png-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.png");
        let px = vec![7u8; 4 * 3 * 3];
        let n = write_rgb(&path, 4, 3, &px).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), n);
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
        for kind in [b"IHDR", b"IDAT", b"IEND"] {
            assert!(
                bytes.windows(4).any(|w| w == kind),
                "brak bloku {}",
                std::str::from_utf8(kind).unwrap()
            );
        }
        std::fs::remove_file(&path).ok();
    }
}
