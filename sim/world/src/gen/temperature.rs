//! P10 — temperatura: dwanaście map miesięcznych na siatce 256 m (M1 §5.6).
//!
//! Model ma trzy składniki i żaden z nich nie jest ozdobnikiem:
//! **szerokość geograficzna** ustawia średnią roczną, **wysokość** odejmuje 6,5 °C na kilometr,
//! a **kontynentalność** (odległość od morza) rozciąga amplitudę roczną. Ostatni składnik jest
//! tym, co odróżnia region nadmorski od śródlądowego na tej samej szerokości — a od tego
//! zależy zapotrzebowanie na ciepło, które konsumuje M8.
//!
//! Cykl roczny jest sinusoidą po **kalendarzu 360-dniowym** (00 §K-1): dwanaście równych
//! miesięcy, więc środek miesiąca `m` wypada dokładnie w dniu `30m + 15` i interpolacja
//! między miesiącami nie ma przypadków szczególnych.

use crate::data::WaterClass;
use crate::gen::shape::RegionShape;
use crate::params::{CLIMATE_CELL_M, WORK_CELL_M};
use crate::pipeline::GenCtx;
use magnat_core::det_math;

/// Pionowy gradient temperatury w dziesiątych częściach °C na metr (6,5 °C/km).
const LAPSE_RATE_DC_PER_M: f64 = -0.065;
/// Próg grzewczy dla stopniodni: poniżej 15 °C budynek trzeba ogrzewać (M8).
const HEATING_BASE_DC: i32 = 150;

pub fn run(ctx: &mut GenCtx) {
    let cdim = ctx.params.climate_dim();
    let lat = f64::from(ctx.params.region.latitude_ddeg()) / 10.0;
    let shape = RegionShape::of(ctx.params.region);

    // Średnia roczna na poziomie morza: kwadratowe przybliżenie rozkładu południkowego,
    // zaczepione w dwóch punktach — 27 °C na równiku i 2 °C przy 60°.
    // Kontrola: 31,5° → 20,1 °C, 49,4° → 10,1 °C, 54,2° → 6,6 °C.
    let mean_sea_level_dc = 270.0 - 250.0 * (lat / 60.0) * (lat / 60.0);

    // Połowa amplitudy rocznej: rośnie z szerokością i z oddaleniem od morza.
    // Przy 52° daje ~24 °C rozpiętości styczeń–lipiec w głębi lądu.
    let amp_base = 40.0 + lat * 1.5;

    let mut komorki = Vec::with_capacity(cdim * cdim);
    for cy in 0..cdim {
        for cx in 0..cdim {
            let (h_m, dist_sea_m) = srednia_komorki(ctx, cx, cy);
            let mut c = crate::climate::ClimateCell::default();

            // Kontynentalność: 0 przy morzu, 1 w głębi lądu (nasycenie po 30 km).
            let kontynent = if shape.sea_depth_m > 0.0 {
                (f64::from(dist_sea_m) / 30_000.0).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let mean_dc = mean_sea_level_dc + f64::from(h_m) * LAPSE_RATE_DC_PER_M;
            let amp_dc = amp_base * (0.55 + 0.45 * kontynent);

            let mut hdd = 0i32;
            for m in 0..12usize {
                // Minimum w styczniu (m = 0), maksimum w lipcu (m = 6) — półkula północna.
                let faza = (m as f64 + 0.5) / 12.0 * std::f64::consts::TAU;
                let t = mean_dc - amp_dc * det_math::cos(faza);
                let t_dc = t.clamp(-600.0, 500.0) as i16;
                c.temp_monthly_dc[m] = t_dc;
                // Stopniodni grzania: 30 dni × niedobór względem progu.
                hdd += (HEATING_BASE_DC - i32::from(t_dc)).max(0) * 30;
            }
            // Skala: setki stopniodni, żeby zmieścić rok w `u16` bez utraty rozdzielczości.
            c.heating_degree_days = (hdd / 100).clamp(0, i32::from(u16::MAX)) as u16;
            komorki.push(c);
        }
    }
    ctx.world.climate = crate::grid::Grid2::from_vec(cdim, komorki);
}

/// Średnia wysokość komórki klimatu i odległość do morza w metrach.
///
/// Uśrednianie po całej komórce 256 m, a nie próbka ze środka: pojedynczy punkt na dnie
/// wąwozu dałby całej komórce temperaturę doliny.
fn srednia_komorki(ctx: &GenCtx, cx: usize, cy: usize) -> (i32, u32) {
    let dim = ctx.dim();
    let krok = (CLIMATE_CELL_M / WORK_CELL_M) as usize;
    let (x0, y0) = (cx * krok, cy * krok);

    let mut suma = 0i64;
    let mut ile = 0i64;
    let mut najblizsze_morze = u32::MAX;
    for y in y0..(y0 + krok).min(dim) {
        for x in x0..(x0 + krok).min(dim) {
            let i = y * dim + x;
            suma += i64::from(ctx.world.height[i]);
            ile += 1;
            if ctx.world.water[i].class() == WaterClass::Sea {
                najblizsze_morze = 0;
            }
        }
    }
    let h_dm = if ile > 0 { (suma / ile) as i32 } else { 0 };

    // Odległość do morza: pole odległości od wody jest odległością do **dowolnej** wody,
    // więc dla kontynentalności szukamy morza osobno — po komórkach klimatu, których
    // na mapie 16 km jest 4096, czyli tyle, że przeszukanie liniowe jest tańsze niż
    // trzymanie drugiego pola odległości w pamięci.
    if najblizsze_morze == u32::MAX {
        najblizsze_morze = odleglosc_do_morza(ctx, cx, cy);
    }
    (h_dm / 10, najblizsze_morze)
}

fn odleglosc_do_morza(ctx: &GenCtx, cx: usize, cy: usize) -> u32 {
    if !ctx.params.region.has_sea() {
        return u32::MAX / 2;
    }
    let dim = ctx.dim();
    let krok = (CLIMATE_CELL_M / WORK_CELL_M) as usize;
    let cdim = dim / krok;
    let mut best = u64::MAX;
    // Skan po środkach komórek klimatu — wystarczająco gęsto dla skali zjawiska.
    for oy in 0..cdim {
        for ox in 0..cdim {
            let i = (oy * krok + krok / 2) * dim + ox * krok + krok / 2;
            if ctx.world.water[i].class() != WaterClass::Sea {
                continue;
            }
            let dx = (ox as i64 - cx as i64) * i64::from(CLIMATE_CELL_M);
            let dy = (oy as i64 - cy as i64) * i64::from(CLIMATE_CELL_M);
            let d = (dx * dx + dy * dy) as u64;
            if d < best {
                best = d;
            }
        }
    }
    if best == u64::MAX {
        u32::MAX / 2
    } else {
        det_math::sqrt(best as f64) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn swiat(region: Region) -> GenCtx<'static> {
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(2)));
        let params = WorldGenParams {
            seed: 3,
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        for pass in crate::pipeline::PASSES.iter().take(10) {
            (pass.run)(&mut ctx);
        }
        ctx
    }

    #[test]
    fn temperatura_maleje_z_wysokoscia_w_kazdym_miesiacu() {
        // M1 §7.2 `climate_monotone`.
        let ctx = swiat(Region::Mountain);
        let cdim = ctx.world.climate.dim();
        let krok = (CLIMATE_CELL_M / WORK_CELL_M) as usize;

        // Para komórek o największej różnicy wysokości.
        let wys = |cx: usize, cy: usize| srednia_komorki(&ctx, cx, cy).0;
        let (mut lo, mut hi) = ((0usize, 0usize), (0usize, 0usize));
        let (mut lo_h, mut hi_h) = (i32::MAX, i32::MIN);
        for cy in 0..cdim {
            for cx in 0..cdim {
                let h = wys(cx, cy);
                if h < lo_h {
                    lo_h = h;
                    lo = (cx, cy);
                }
                if h > hi_h {
                    hi_h = h;
                    hi = (cx, cy);
                }
            }
        }
        assert!(
            hi_h - lo_h > 30,
            "za mała różnica wysokości: {} m",
            hi_h - lo_h
        );
        let _ = krok;
        let nizej = ctx.world.climate.get(lo.0, lo.1);
        let wyzej = ctx.world.climate.get(hi.0, hi.1);
        for m in 0..12 {
            assert!(
                wyzej.temp_monthly_dc[m] < nizej.temp_monthly_dc[m],
                "miesiąc {m}: {} m ma {} dC, {} m ma {} dC",
                hi_h,
                wyzej.temp_monthly_dc[m],
                lo_h,
                nizej.temp_monthly_dc[m]
            );
        }
    }

    #[test]
    fn cykl_roczny_ma_zime_w_styczniu_i_lato_w_lipcu() {
        let ctx = swiat(Region::Lowland);
        let c = ctx.world.climate.get(2, 2);
        let min = c.temp_monthly_dc.iter().copied().min().unwrap();
        let max = c.temp_monthly_dc.iter().copied().max().unwrap();
        assert_eq!(c.temp_monthly_dc[0], min, "styczeń nie jest najzimniejszy");
        assert_eq!(c.temp_monthly_dc[6], max, "lipiec nie jest najcieplejszy");
        assert!(
            max - min > 100,
            "amplituda roczna {} dC to za mało",
            max - min
        );
    }

    #[test]
    fn pustynia_jest_cieplejsza_od_niziny() {
        let sr = |r: Region| {
            let ctx = swiat(r);
            let s: i64 = ctx
                .world
                .climate
                .as_slice()
                .iter()
                .map(|c| i64::from(c.mean_annual_temp_dc()))
                .sum();
            s / ctx.world.climate.len() as i64
        };
        let pustynia = sr(Region::Desert);
        let nizina = sr(Region::Lowland);
        assert!(
            pustynia > nizina + 50,
            "pustynia {pustynia} dC, nizina {nizina} dC"
        );
    }

    #[test]
    fn wybrzeze_ma_lagodniejszy_rok_niz_wnetrze_ladu() {
        // Kontynentalność: ta sama szerokość, inna amplituda. To jest cały powód,
        // dla którego odległość od morza w ogóle wchodzi do modelu.
        let ctx = swiat(Region::Coastal);
        let amplituda = |c: &crate::climate::ClimateCell| {
            c.temp_monthly_dc.iter().copied().max().unwrap()
                - c.temp_monthly_dc.iter().copied().min().unwrap()
        };
        let cdim = ctx.world.climate.dim();
        // Południe mapy to morze, północ to wnętrze lądu (konwencja z P1).
        let przy_morzu = amplituda(ctx.world.climate.get(cdim / 2, 1));
        let w_gledzi = amplituda(ctx.world.climate.get(cdim / 2, cdim - 2));
        assert!(
            w_gledzi > przy_morzu,
            "wnętrze lądu {w_gledzi} dC nie ma większej amplitudy niż brzeg {przy_morzu} dC"
        );
    }

    #[test]
    fn stopniodni_rosna_w_chlodniejszym_klimacie() {
        let hdd = |r: Region| {
            let ctx = swiat(r);
            let s: u64 = ctx
                .world
                .climate
                .as_slice()
                .iter()
                .map(|c| u64::from(c.heating_degree_days))
                .sum();
            s / ctx.world.climate.len() as u64
        };
        assert!(
            hdd(Region::Lowland) > hdd(Region::Desert),
            "stopniodni nie reagują na klimat"
        );
    }
}
