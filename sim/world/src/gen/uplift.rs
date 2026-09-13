//! P3 — pola wypiętrzenia `U` i podatności na erozję `K` (M1 §5.6, §5.7).
//!
//! To są dwa pola wejściowe równania stream-power. `U` mówi, gdzie skorupa się podnosi,
//! `K` — jak łatwo skała się ściera. Zróżnicowanie `K` jest tym, co daje **ostańce**:
//! twarda intruzja zostaje jako wzgórze, gdy otoczenie zostało zerodowane. Bez tego pola
//! teren po erozji wygląda jak równomiernie zmyty, a nie jak krajobraz o historii.

use crate::gen::shape::RegionShape;
use crate::noise::{fbm, FbmSpec, NoiseField};
use crate::params::WORK_CELL_M;
use crate::pipeline::GenCtx;
use magnat_core::StreamId;

/// Przelicznik mm/rok → m/rok.
const MM_TO_M: f32 = 0.001;
/// Przelicznik jednostek `erodibility_e6` → wartość `K`.
const E6: f32 = 1e-6;

pub fn run(ctx: &mut GenCtx) {
    let params = ctx.params;
    let shape = RegionShape::of(params.region);
    let pool = ctx.pool;

    let u_field = NoiseField::new(params.seed, StreamId::WorldUplift, 0);
    let k_field = NoiseField::new(params.seed, StreamId::WorldErodibility, 0);

    // Wypiętrzenie ma strukturę regionalną (dziesiątki kilometrów — całe pasmo),
    // erodowalność lokalną (setki metrów — pojedyncza intruzja). Stąd różne długości fal.
    let u_spec = FbmSpec::new(3, 9_000.0);
    let k_spec = FbmSpec::new(5, 1_400.0);

    let u_mid = shape.uplift_mm_yr * MM_TO_M;
    let k_mid = shape.erodibility_e6 * E6;

    // Wysokość wchodzi do `U`: partie już wyniesione podnoszą się szybciej, co podtrzymuje
    // kontrast rzeźby zamiast pozwolić erozji wyrównać wszystko do jednego poziomu.
    let relief = shape.relief_m.max(1.0);
    let heights = ctx.work.height_m.as_slice().to_vec();
    let dim = ctx.dim();

    ctx.work.uplift.par_rows_mut(pool, |y, row| {
        for (x, u) in row.iter_mut().enumerate() {
            let (mx, my) = (
                (x as f32) * WORK_CELL_M as f32,
                (y as f32) * WORK_CELL_M as f32,
            );
            let h = heights[y * dim + x];
            let wysokosc = (h / relief).clamp(0.0, 1.0);
            // Mnożnik 0,4…1,8 od szumu, 0,6…1,4 od wysokości.
            let n = fbm(&u_field, mx, my, u_spec);
            *u = u_mid * (1.1 + n * 0.7) * (0.6 + wysokosc * 0.8);
        }
    });

    ctx.work.erodibility.par_rows_mut(pool, |y, row| {
        for (x, k) in row.iter_mut().enumerate() {
            let (mx, my) = (
                (x as f32) * WORK_CELL_M as f32,
                (y as f32) * WORK_CELL_M as f32,
            );
            let n = fbm(&k_field, mx, my, k_spec);
            // Zakres 0,25×…2,0× wartości środkowej. Dolny kraniec to twarda intruzja —
            // stąd bierze się ostaniec.
            let mnoznik = if n < 0.0 {
                0.25 + (n + 1.0) * 0.75
            } else {
                1.0 + n
            };
            *k = k_mid * mnoznik;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn pola(region: Region) -> crate::fields::WorkFields {
        let pool = JobPool::new(1);
        let params = WorldGenParams {
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, &pool);
        crate::gen::landmask::run(&mut ctx);
        crate::gen::height::run(&mut ctx);
        run(&mut ctx);
        ctx.work
    }

    #[test]
    fn oba_pola_sa_dodatnie_i_ograniczone() {
        let w = pola(Region::Mountain);
        let shape = RegionShape::of(Region::Mountain);
        for u in w.uplift.as_slice() {
            assert!(*u > 0.0, "zerowe wypiętrzenie łamie równanie stream-power");
            assert!(*u < shape.uplift_mm_yr * MM_TO_M * 2.5, "U = {u}");
        }
        for k in w.erodibility.as_slice() {
            assert!(*k > 0.0, "zerowa erodowalność zamraża teren na zawsze");
            assert!(*k < shape.erodibility_e6 * E6 * 2.5, "K = {k}");
        }
    }

    #[test]
    fn erodowalnosc_jest_zroznicowana_bo_od_tego_zaleza_ostance() {
        let w = pola(Region::Mountain);
        let min = w.erodibility.as_slice().iter().fold(f32::MAX, |a, b| a.min(*b));
        let max = w.erodibility.as_slice().iter().fold(f32::MIN, |a, b| a.max(*b));
        assert!(max / min > 3.0, "K zmienia się tylko {}×", max / min);
    }

    #[test]
    fn gory_wypietrzaja_sie_szybciej_niz_nizina() {
        let sr = |w: &crate::fields::WorkFields| {
            w.uplift.as_slice().iter().map(|u| f64::from(*u)).sum::<f64>() / w.uplift.len() as f64
        };
        assert!(sr(&pola(Region::Mountain)) > sr(&pola(Region::Lowland)) * 3.0);
    }
}
