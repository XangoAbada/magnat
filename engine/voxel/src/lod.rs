//! Agregacja LOD 2× / 4× / 8× (M1 §5.4, WP-V3).
//!
//! **Agregat liczy się z `ColumnSource`, nie z voxeli LOD0.** To nie jest optymalizacja,
//! tylko warunek wykonalności: widok całego miasta pokazuje pierścień LOD3 o promieniu 4 km,
//! a wymaganie rezydentnego LOD0 pod spodem znaczyłoby trzymanie 64× więcej chunków, niż
//! mieści budżet pamięci (M1 §5.9).
//!
//! Reguła agregatu jest prosta i musi taka zostać, bo decyduje o wyglądzie horyzontu:
//! **głosowanie większościowe** po sześcianie `2^lod`, remis rozstrzyga najniższy `MaterialId`,
//! a komórka jest pusta, gdy powietrze zajmuje ponad połowę objętości. Ostatni warunek nie jest
//! kosmetyczny: bez niego zbocza puchną, bo agregat zawsze znajdzie w sześcianie jakiś kamień.

use crate::chunk::{ChunkBuilder, ChunkCoord};
use crate::material::MaterialId;
use crate::ColumnSource;

/// Najwyższy obsługiwany poziom szczegółowości. LOD3 to 8 m na voxel (M1 §5.4).
pub const MAX_LOD: u8 = 3;

/// Histogram materiałów jednej komórki agregatu.
mod glosowanie {
    use super::MaterialId;
    use smallvec::SmallVec;

    /// `SmallVec` na sześć pozycji: tyle materiałów ma typowy sześcian terenu
    /// (powietrze, gleba, skała, woda i dwa rzadsze). Pełna tablica liczników dla 32³
    /// komórek × 256 materiałów to 16 MB na chunk — alokowana przy każdej agregacji
    /// kosztowałaby więcej niż całe liczenie.
    pub type Histogram = SmallVec<[(MaterialId, u16); 6]>;

    pub fn dodaj(h: &mut Histogram, m: MaterialId) {
        if let Some(w) = h.iter_mut().find(|(k, _)| *k == m) {
            w.1 += 1;
            return;
        }
        h.push((m, 1));
    }

    /// Zwycięzca głosowania: najwięcej głosów, remis rozstrzyga **najniższy** `MaterialId`.
    /// Pusto, gdy powietrze ma ponad połowę.
    pub fn zwyciezca(h: &Histogram, razem: u32) -> MaterialId {
        let powietrze = h
            .iter()
            .find(|(k, _)| k.is_air())
            .map_or(0, |(_, v)| u32::from(*v));
        if powietrze * 2 > razem {
            return MaterialId::AIR;
        }
        let mut best = MaterialId::AIR;
        let mut best_v = 0u16;
        for (k, v) in h {
            if k.is_air() {
                continue;
            }
            if *v > best_v || (*v == best_v && *k < best) {
                best = *k;
                best_v = *v;
            }
        }
        best
    }
}

/// Buduje chunk na poziomie `lod`.
///
/// **Agregację wykonuje źródło, nie ten moduł** — i to nie jest przerzucanie odpowiedzialności,
/// tylko jedyny wariant mieszczący się w budżecie. Nadpróbkowanie po stronie `engine/voxel`
/// wymagałoby `8^lod` próbek na komórkę agregatu: dla LOD3 to 16,7 mln próbek na jeden chunk,
/// zmierzone 305 ms wobec budżetu 1,5 ms (M1 §4 WP-V3). Źródło zna swoją funkcję wysokości
/// i potrafi ją próbkować rzadko, ale sensownie — `sim/world` bierze średnią z czterech
/// punktów na komórkę agregatu.
///
/// Reguła agregatu, którą **źródło ma realizować**, zostaje niezmieniona (M1 §5.4):
/// głosowanie większościowe, remis po najniższym `MaterialId`, pusto przy ponad połowie
/// powietrza. Test `lod_consistency` (§7.1) sprawdza ją na gotowym terenie.
pub fn aggregate(src: &dyn ColumnSource, coord: ChunkCoord, lod: u8, out: &mut ChunkBuilder) {
    assert!(lod <= MAX_LOD, "LOD {lod} poza zakresem 0..={MAX_LOD}");
    src.fill_chunk(coord, lod, out);
}

/// Zwycięzca głosowania większościowego dla komórki agregatu — do użytku przez źródła,
/// które realizują regułę z §5.4 wprost.
#[must_use]
pub fn majority(probki: &[MaterialId]) -> MaterialId {
    let mut h = glosowanie::Histogram::new();
    for m in probki {
        glosowanie::dodaj(&mut h, *m);
    }
    glosowanie::zwyciezca(&h, probki.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::CHUNK_DIM;
    use glosowanie::Histogram;

    /// Źródło testowe: teren o zadanej wysokości w voxelach świata.
    struct Plaski {
        wysokosc: i32,
        material: MaterialId,
    }

    impl ColumnSource for Plaski {
        fn fill_chunk(&self, coord: ChunkCoord, lod: u8, out: &mut ChunkBuilder) {
            // Źródło samo skaluje do LOD — tak jak `sim/world` (patrz `aggregate`).
            let skala = 1i32 << lod;
            let origin = coord.origin_voxels(lod);
            let r = out.range();
            for y in r.clone() {
                for x in r.clone() {
                    let lokalna = (self.wysokosc - origin.z).div_euclid(skala);
                    out.fill_column(x, y, -(out.pad() as i32), lokalna, self.material);
                }
            }
        }
    }

    #[test]
    fn agregat_zachowuje_plaski_teren() {
        // Chunk LOD1 pokrywa 64 m; teren płaski na wysokości 40 voxeli ma po agregacji
        // leżeć na wysokości 20 komórek LOD1 — połowie, bo komórka jest dwa razy wyższa.
        let src = Plaski {
            wysokosc: 40,
            material: MaterialId(5),
        };
        let coord = ChunkCoord::new(0, 0, 0);
        let mut out = ChunkBuilder::new(coord, 1, 0);
        aggregate(&src, coord, 1, &mut out);

        assert_eq!(
            out.material_at(10, 10, 0),
            MaterialId(5),
            "dno agregatu puste"
        );
        assert_eq!(
            out.material_at(10, 10, 19),
            MaterialId(5),
            "teren urwał się za nisko"
        );
        assert_eq!(
            out.material_at(10, 10, 20),
            MaterialId::AIR,
            "teren sięgnął wyżej niż powinien"
        );
    }

    #[test]
    fn agregat_lod0_jest_tozsamoscia() {
        let src = Plaski {
            wysokosc: 12,
            material: MaterialId(3),
        };
        let coord = ChunkCoord::new(1, 2, 0);
        let mut a = ChunkBuilder::new(coord, 0, 0);
        aggregate(&src, coord, 0, &mut a);
        let mut b = ChunkBuilder::new(coord, 0, 0);
        src.fill_chunk(coord, 0, &mut b);
        for z in 0..CHUNK_DIM as i32 {
            assert_eq!(a.material_at(0, 0, z), b.material_at(0, 0, z), "z = {z}");
        }
    }

    #[test]
    fn powietrze_w_wiekszosci_daje_pustke() {
        // M1 §5.4: komórka pusta, gdy ponad połowa to powietrze — inaczej zbocza puchną.
        let mut h = Histogram::new();
        glosowanie::dodaj(&mut h, MaterialId::AIR);
        for _ in 0..5 {
            glosowanie::dodaj(&mut h, MaterialId::AIR);
        }
        glosowanie::dodaj(&mut h, MaterialId(7));
        glosowanie::dodaj(&mut h, MaterialId(7));
        assert_eq!(glosowanie::zwyciezca(&h, 8), MaterialId::AIR);

        // Dokładnie połowa powietrza — komórka **nie** jest pusta (warunek jest ostry).
        let mut h = Histogram::new();
        for _ in 0..4 {
            glosowanie::dodaj(&mut h, MaterialId::AIR);
        }
        for _ in 0..4 {
            glosowanie::dodaj(&mut h, MaterialId(7));
        }
        assert_eq!(glosowanie::zwyciezca(&h, 8), MaterialId(7));
    }

    #[test]
    fn remis_rozstrzyga_najnizszy_material() {
        let mut h = Histogram::new();
        glosowanie::dodaj(&mut h, MaterialId(9));
        glosowanie::dodaj(&mut h, MaterialId(4));
        glosowanie::dodaj(&mut h, MaterialId(9));
        glosowanie::dodaj(&mut h, MaterialId(4));
        assert_eq!(
            glosowanie::zwyciezca(&h, 4),
            MaterialId(4),
            "remis nie rozstrzygnięty deterministycznie"
        );
    }

    #[test]
    fn agregacja_jest_powtarzalna() {
        let src = Plaski {
            wysokosc: 37,
            material: MaterialId(2),
        };
        let coord = ChunkCoord::new(0, 0, 0);
        let mut a = ChunkBuilder::new(coord, 2, 0);
        let mut b = ChunkBuilder::new(coord, 2, 0);
        aggregate(&src, coord, 2, &mut a);
        aggregate(&src, coord, 2, &mut b);
        for z in 0..CHUNK_DIM as i32 {
            for y in [0i32, 15, 31] {
                for x in [0i32, 15, 31] {
                    assert_eq!(a.material_at(x, y, z), b.material_at(x, y, z));
                }
            }
        }
    }
}
