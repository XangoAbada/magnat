//! P2 — baza wysokości: relief dokładany do formy kontynentalnej z P1 (M1 §5.6).
//!
//! Szum wielooktawowy ze zniekształceniem dziedziny; region górski i pustynny na ridged
//! multifractal, reszta na fBm. Relief **wygasza się nad morzem** — inaczej pod wodą rosną
//! góry, które erozja i tak zetrze, a które zdążą popsuć profil brzegowy.

use crate::gen::shape::{RegionShape, WORLD_MAX_M, WORLD_MIN_M};
use crate::noise::{domain_warp, fbm, ridged, FbmSpec, NoiseField};
use crate::params::WORK_CELL_M;
use crate::pipeline::GenCtx;
use magnat_core::StreamId;

pub fn run(ctx: &mut GenCtx) {
    let params = ctx.params;
    let shape = RegionShape::of(params.region);
    let pool = ctx.pool;

    let base = NoiseField::new(params.seed, StreamId::WorldHeightBase, 0);
    // Dwa niezależne pola na osie zniekształcenia — jedno pole użyte dwa razy dałoby
    // przesunięcie wyłącznie po przekątnej, czyli wzór widoczny gołym okiem.
    let warp_x = NoiseField::new(params.seed, StreamId::WorldDomainWarp, 0);
    let warp_y = NoiseField::new(params.seed, StreamId::WorldDomainWarp, 1);

    let spec = FbmSpec::new(shape.octaves, shape.wavelength_m);
    // Zniekształcenie próbkuje się rzadziej niż relief: jego rolą jest wyginanie dolin,
    // a nie dokładanie szczegółu.
    let warp_spec = FbmSpec::new(3, shape.wavelength_m * 2.0);

    ctx.work.height_m.par_rows_mut(pool, |y, row| {
        for (x, h) in row.iter_mut().enumerate() {
            let (mx, my) = (
                (x as f32) * WORK_CELL_M as f32,
                (y as f32) * WORK_CELL_M as f32,
            );
            let (wx, wy) = domain_warp(&warp_x, &warp_y, mx, my, shape.warp_m, warp_spec);

            let n = if shape.ridged {
                ridged(&base, wx, wy, spec)
            } else {
                fbm(&base, wx, wy, spec)
            };

            // Relief liczony od 0 w górę: n ∈ [−1, 1] → [0, 1]. Teren ma się wznosić
            // ponad formę kontynentalną, a nie ją przecinać w połowie.
            let relief = (n + 1.0) * 0.5 * shape.relief_m;

            // Wygaszenie nad morzem: pełny relief od 10 m n.p.m. w górę, zero poniżej brzegu.
            let land_factor = (*h / 10.0).clamp(0.0, 1.0);
            *h = (*h + relief * land_factor).clamp(WORLD_MIN_M, WORLD_MAX_M);
        }
    });

    // Maska lądu po dołożeniu reliefu — P3 i dalej korzystają z tej, nie z tej z P1.
    let dim = ctx.dim();
    let land: Vec<bool> = ctx
        .work
        .height_m
        .as_slice()
        .iter()
        .map(|h| *h > 0.0)
        .collect();
    ctx.work.land = crate::grid::Grid2::from_vec(dim, land);

    crate::gen::commit_height(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn teren(region: Region, size: WorldSize, seed: u64) -> crate::grid::Grid2<f32> {
        let pool = JobPool::new(1);
        let params = WorldGenParams {
            seed,
            size,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, &pool);
        crate::gen::landmask::run(&mut ctx);
        run(&mut ctx);
        ctx.work.height_m
    }

    #[test]
    fn teren_miesci_sie_w_zakresie_pionowym_swiata() {
        for &r in Region::ALL {
            let h = teren(r, WorldSize::Small4km, 7);
            for v in h.as_slice() {
                assert!(
                    (WORLD_MIN_M..=WORLD_MAX_M).contains(v),
                    "{}: wysokość {v} m poza zakresem",
                    r.key()
                );
            }
        }
    }

    #[test]
    fn gory_sa_wyzsze_od_niziny() {
        let g = teren(Region::Mountain, WorldSize::Small4km, 11);
        let n = teren(Region::Lowland, WorldSize::Small4km, 11);
        let max = |g: &crate::grid::Grid2<f32>| g.as_slice().iter().fold(f32::MIN, |a, b| a.max(*b));
        assert!(
            max(&g) > max(&n) * 2.0,
            "góry {} m vs nizina {} m",
            max(&g),
            max(&n)
        );
    }

    #[test]
    fn relief_nie_wyrasta_z_dna_morza() {
        // Nad morzem obowiązuje wyłącznie forma kontynentalna z P1 — żadnych podwodnych gór.
        let h = teren(Region::Coastal, WorldSize::Small4km, 3);
        let mut pod_woda = 0;
        for v in h.as_slice() {
            if *v < -5.0 {
                pod_woda += 1;
            }
        }
        assert!(pod_woda > 0, "region nadmorski bez morza");
        // Najgłębszy punkt ma być blisko zadanej głębokości szelfu, nie wypłycony reliefem.
        let min = h.as_slice().iter().fold(f32::MAX, |a, b| a.min(*b));
        let shape = RegionShape::of(Region::Coastal);
        assert!(
            min < -shape.sea_depth_m * 0.8,
            "szelf wypłycony do {min} m przy zakładanych {} m",
            -shape.sea_depth_m
        );
    }

    #[test]
    fn ten_sam_seed_daje_ten_sam_teren() {
        let a = teren(Region::River, WorldSize::Small4km, 42);
        let b = teren(Region::River, WorldSize::Small4km, 42);
        assert_eq!(a.as_slice(), b.as_slice());
        let c = teren(Region::River, WorldSize::Small4km, 43);
        assert_ne!(a.as_slice(), c.as_slice());
    }
}
