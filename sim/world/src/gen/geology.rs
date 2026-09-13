//! P8 — model geologiczny i pole odległości od wody (M1 §5.6).
//!
//! Sam stos warstw jest analityczny i nie zajmuje pamięci ([`crate::geology`]); ten przebieg
//! ładuje jego definicję i liczy jedyną daną, której kolumna nie potrafi wyprowadzić sama:
//! **odległość do najbliższej wody**. Torf, mady i poziom wód gruntowych zależą od niej,
//! a policzenie jej na żądanie wymagałoby przeszukania mapy przy każdym zapytaniu.
//!
//! Pole jest na siatce **16 m**, nie 4 m. Zasięgi, które z niego korzystają, liczą się
//! w setkach metrów, więc czterokrotnie gęstsza siatka dodałaby 32 MB do stanu trwałego
//! (M1 §5.9) i ani jednej informacji.

use crate::data::WaterClass;
use crate::geology::GeologyModel;
use crate::grid::Grid2;
use crate::params::WORK_CELL_M;
use crate::pipeline::GenCtx;
use magnat_voxel::MaterialRegistry;

/// Bok komórki pola odległości od wody, w metrach.
pub const WATER_DIST_CELL_M: u32 = 16;
/// Ile komórek siatki roboczej przypada na komórkę pola odległości.
const COARSE: usize = (WATER_DIST_CELL_M / WORK_CELL_M) as usize;

/// Wagi transformaty odległościowej chamfer 3–4, przeskalowane ×10 dla arytmetyki całkowitej.
const D_ORTHO: u32 = 10;
const D_DIAG: u32 = 14;

pub fn run(ctx: &mut GenCtx) {
    // Rejestr materiałów i warstwy z `data/`. Brak plików to błąd konfiguracji, nie stan
    // do obsłużenia — świat bez geologii nie ma z czego zbudować kolumny.
    let reg = MaterialRegistry::load_dir(&crate::data_path("materials"))
        .expect("data/materials — rejestr materiałów jest wymagany");
    let model = GeologyModel::load(&crate::data_path("geology/layers.ron"), &reg)
        .expect("data/geology/layers.ron");

    ctx.world.water_dist = water_distance_field(ctx);
    ctx.world.geology = model;
}

/// Transformata odległościowa chamfer 3–4: dwa przejścia po siatce, w przód i w tył.
///
/// Dokładny algorytm Dijkstry dałby ułamek procenta lepszy wynik za koszt kopca na
/// milionie komórek. Chamfer myli się o ~2 % przy odległościach ukośnych — poniżej
/// rozdzielczości siatki, na której liczymy.
fn water_distance_field(ctx: &GenCtx) -> Grid2<u16> {
    let dim = ctx.dim();
    let cdim = dim / COARSE;
    let mut d = vec![u32::MAX / 4; cdim * cdim];

    // Zasiew: komórka gruba jest wodą, jeżeli jest w niej choć jedna komórka wodna.
    for cy in 0..cdim {
        for cx in 0..cdim {
            let mut woda = false;
            'szukaj: for y in 0..COARSE {
                for x in 0..COARSE {
                    let i = (cy * COARSE + y) * dim + cx * COARSE + x;
                    if ctx.world.water[i].class() != WaterClass::Dry {
                        woda = true;
                        break 'szukaj;
                    }
                }
            }
            if woda {
                d[cy * cdim + cx] = 0;
            }
        }
    }

    let at = |d: &[u32], x: i64, y: i64| -> u32 {
        if x < 0 || y < 0 || x >= cdim as i64 || y >= cdim as i64 {
            u32::MAX / 4
        } else {
            d[y as usize * cdim + x as usize]
        }
    };

    for cy in 0..cdim {
        for cx in 0..cdim {
            let (x, y) = (cx as i64, cy as i64);
            let m = d[cy * cdim + cx]
                .min(at(&d, x - 1, y) + D_ORTHO)
                .min(at(&d, x, y - 1) + D_ORTHO)
                .min(at(&d, x - 1, y - 1) + D_DIAG)
                .min(at(&d, x + 1, y - 1) + D_DIAG);
            d[cy * cdim + cx] = m;
        }
    }
    for cy in (0..cdim).rev() {
        for cx in (0..cdim).rev() {
            let (x, y) = (cx as i64, cy as i64);
            let m = d[cy * cdim + cx]
                .min(at(&d, x + 1, y) + D_ORTHO)
                .min(at(&d, x, y + 1) + D_ORTHO)
                .min(at(&d, x + 1, y + 1) + D_DIAG)
                .min(at(&d, x - 1, y + 1) + D_DIAG);
            d[cy * cdim + cx] = m;
        }
    }

    let metry: Vec<u16> = d
        .iter()
        .map(|u| {
            let m = u64::from(*u) * u64::from(WATER_DIST_CELL_M) / u64::from(D_ORTHO);
            m.min(u64::from(u16::MAX)) as u16
        })
        .collect();
    Grid2::from_vec(cdim, metry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn swiat(region: Region) -> GenCtx<'static> {
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(2)));
        let params = WorldGenParams {
            seed: 5,
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        for pass in crate::pipeline::PASSES.iter().take(8) {
            (pass.run)(&mut ctx);
        }
        ctx
    }

    #[test]
    fn odleglosc_od_wody_zeruje_sie_na_wodzie_i_rosnie_z_dala() {
        let ctx = swiat(Region::River);
        let d = &ctx.world.water_dist;
        assert!(d.as_slice().contains(&0), "brak komórek wodnych");
        let max = d.as_slice().iter().copied().max().unwrap();
        assert!(max > 100, "pole odległości płaskie: maksimum {max} m");
        // Mapa 4 km — nikt nie jest dalej od wody niż przekątna mapy.
        assert!(max < 6000, "odległość {max} m przekracza rozmiar mapy");
    }

    #[test]
    fn sasiednie_komorki_roznia_sie_o_najwyzej_bok_komorki() {
        // Niezmiennik transformaty odległościowej: pole jest 1-lipschitzowskie. Naruszenie
        // znaczy, że któryś przebieg nie propagował wartości.
        let ctx = swiat(Region::Lowland);
        let d = &ctx.world.water_dist;
        let cdim = d.dim();
        for y in 0..cdim {
            for x in 1..cdim {
                let a = i32::from(*d.get(x - 1, y));
                let b = i32::from(*d.get(x, y));
                assert!(
                    (a - b).abs() <= WATER_DIST_CELL_M as i32 + 1,
                    "skok {a} → {b} w ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn geologia_laduje_sie_i_daje_kolumne_w_kazdym_punkcie() {
        let ctx = swiat(Region::Mountain);
        assert!(!ctx.world.geology.layers.is_empty(), "brak warstw");
        let noise =
            crate::noise::NoiseField::new(ctx.params.seed, magnat_core::StreamId::WorldGeology, 0);
        for i in (0..ctx.world.height.len()).step_by(7919) {
            let (x, y) = ctx.world.height.xy(i);
            let c = ctx.world.geology.column_at(
                &noise,
                (x * WORK_CELL_M as usize) as i32,
                (y * WORK_CELL_M as usize) as i32,
                i32::from(ctx.world.height[i]),
                0,
                *ctx.world.water_dist.get(x / COARSE, y / COARSE),
            );
            assert!(c.material_at(c.surface_z).is_some(), "pusta kolumna w {i}");
        }
    }
}
