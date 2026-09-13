//! P1 — maska lądu, profil brzegowy i forma kontynentalna (M1 §5.6).
//!
//! Przebieg wpisuje do `work.height_m` **formę kontynentalną**: bardzo długofalowy podkład
//! w metrach, ujemny na morzu. P2 dokłada na to relief. Taki podział jest celowy: dzięki
//! niemu „gdzie jest morze" rozstrzyga się raz i na jednej, gładkiej funkcji, a nie wypada
//! przypadkiem z sumy ośmiu oktaw szumu.

use crate::fields::WorkFields;
use crate::gen::shape::RegionShape;
use crate::noise::{fbm, FbmSpec, NoiseField};
use crate::params::{WorldGenParams, WORK_CELL_M};
use crate::pipeline::GenCtx;
use magnat_core::StreamId;

pub fn run(ctx: &mut GenCtx) {
    let params = ctx.params;
    let shape = RegionShape::of(params.region);
    let field = NoiseField::new(params.seed, StreamId::WorldLandmask, 0);
    let dim = ctx.dim();
    let pool = ctx.pool;

    // Linia brzegowa faluje wzdłuż osi X. Cztery oktawy: pierwsza daje wielką zatokę,
    // ostatnia przylądki rzędu kilkuset metrów. Detal poniżej tego powstaje dopiero
    // z erozji i z wcięcia ujść rzek w P7.
    let shore = FbmSpec::new(4, 3_500.0);

    ctx.world.params = params;
    let work: &mut WorkFields = &mut ctx.work;
    work.height_m.par_rows_mut(pool, |y, row| {
        for (x, h) in row.iter_mut().enumerate() {
            *h = continental_form(&field, shore, &params, &shape, x, y, dim);
        }
    });

    // Maska lądu wynika z formy, nie odwrotnie — jedno źródło prawdy.
    let land: Vec<bool> = work.height_m.as_slice().iter().map(|h| *h > 0.0).collect();
    work.land = crate::grid::Grid2::from_vec(dim, land);
}

/// Forma kontynentalna w metrach n.p.m. w punkcie siatki roboczej.
fn continental_form(
    field: &NoiseField,
    shore: FbmSpec,
    params: &WorldGenParams,
    shape: &RegionShape,
    x: usize,
    y: usize,
    dim: usize,
) -> f32 {
    let mx = (x as f32) * WORK_CELL_M as f32;

    if !params.region.has_sea() {
        // Ląd w całości. Delikatne pochylenie mapy nadaje kierunek odpływu — bez niego
        // rzeki nie mają dokąd płynąć i cała hydrologia kończy się w jeziorach.
        let tilt = 1.0 - (y as f32) / (dim as f32); // 1 na północy, 0 na południu
        return shape.base_m * 0.5 + tilt * shape.base_m * 0.5 + 1.0;
    }

    // Wybrzeże: morze na południu (małe `y`), ląd na północy. Linia brzegowa faluje.
    let d = (y as f32) / (dim as f32);
    let shore_line = 0.28 + fbm(field, mx, 0.0, shore) * 0.13;
    let t = d - shore_line;

    if t >= 0.0 {
        // Ląd: wznosi się od brzegu w głąb, z nasyceniem — nie chcemy równi pochyłej.
        let k = (t / (1.0 - shore_line)).clamp(0.0, 1.0);
        shape.base_m * k * (2.0 - k)
    } else {
        // Morze: szelf opadający ku krawędzi mapy. Kwadrat, bo szelf jest płaski
        // przy brzegu i stromieje dopiero dalej — to widać na profilu plaży.
        let k = (-t / shore_line).clamp(0.0, 1.0);
        -shape.sea_depth_m * k * k
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn ctx_dla(region: Region) -> (crate::data::WorldData, crate::fields::WorkFields) {
        let pool = JobPool::new(1);
        let params = WorldGenParams {
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, &pool);
        run(&mut ctx);
        (ctx.world, ctx.work)
    }

    #[test]
    fn region_bez_morza_jest_w_calosci_ladem() {
        for r in [Region::Lowland, Region::Mountain, Region::River, Region::Desert] {
            let (_, work) = ctx_dla(r);
            assert!(
                work.land.as_slice().iter().all(|l| *l),
                "{}: znalazło się morze tam, gdzie go nie ma",
                r.key()
            );
        }
    }

    #[test]
    fn wybrzeze_ma_i_lad_i_morze_i_brzeg_jest_ciagly() {
        let (_, work) = ctx_dla(Region::Coastal);
        let dim = work.land.dim();
        let lad = work.land.as_slice().iter().filter(|l| **l).count();
        assert!(lad > 0 && lad < work.land.len(), "brak linii brzegowej");

        // W każdej kolumnie przejście morze→ląd ma nastąpić dokładnie raz: linia brzegowa
        // jest funkcją X, więc archipelag oznaczałby błąd w profilu, a nie ciekawą wyspę.
        for x in 0..dim {
            let zmiany = (1..dim)
                .filter(|y| work.land.get(x, *y) != work.land.get(x, y - 1))
                .count();
            assert_eq!(zmiany, 1, "kolumna {x} ma {zmiany} przejść brzegowych");
        }
    }

    #[test]
    fn morze_nie_jest_glebsze_niz_dno_swiata() {
        let (_, work) = ctx_dla(Region::Coastal);
        let min = work
            .height_m
            .as_slice()
            .iter()
            .fold(f32::MAX, |a, b| a.min(*b));
        assert!(min >= crate::gen::shape::WORLD_MIN_M, "dno na {min} m");
    }
}
