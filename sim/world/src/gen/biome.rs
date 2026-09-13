//! P12 — biomy, żyzność gleby i poziom wód gruntowych (M1 §5.6).
//!
//! Klasyfikacja Whittakera (temperatura roczna × opad roczny) z **nadpisaniami lokalnymi**,
//! bo Whittaker opisuje klimat, a nie teren: jezioro w środku strefy lasu liściastego dalej
//! jest jeziorem, a mokradło powstaje tam, gdzie zbiera się woda, niezależnie od tego,
//! ile jej spada z nieba.
//!
//! Kolejność rozstrzygania jest istotna i jest odwrotna do kolejności ogólności: najpierw
//! woda, potem skrajności terenu (skała, śnieg), potem mokradło, a Whittaker na końcu,
//! jako reguła domyślna.

use crate::data::WaterClass;
use crate::params::{CLIMATE_CELL_M, WORK_CELL_M};
use crate::pipeline::GenCtx;
use magnat_core::{Biome, Q};

/// Udział powierzchni wodnej, od którego komórka klimatu jest po prostu wodą.
const WATER_DOMINANT: f32 = 0.5;
/// Akumulacja spływu, od której pojedyncza komórka robocza liczy się jako podmokła.
const MARSH_MIN_ACCUM: f32 = 20_000.0;
/// Nachylenie, poniżej którego woda nie ma jak odpłynąć.
const MARSH_MAX_SLOPE: u8 = 10;
/// Jaka część komórki klimatu musi być podmokła, żeby nazwać ją mokradłem.
///
/// **Udział, nie maksimum.** Pierwsza wersja pytała o maksymalną akumulację w komórce
/// 256 m — a komórka 256 m zawiera 4096 komórek roboczych, więc prawie zawsze łapie jakiś
/// ciek i cała nizina wychodziła jednym wielkim bagnem. Mokradło to teren, który jest
/// podmokły **w większości**, a nie taki, przez który przepływa strumień.
const MARSH_MIN_SHARE: f32 = 0.30;
/// Nachylenie, powyżej którego nie utrzyma się nic poza skałą.
const ROCK_SLOPE: u8 = 190;

pub fn run(ctx: &mut GenCtx) {
    let cdim = ctx.params.climate_dim();
    let krok = (CLIMATE_CELL_M / WORK_CELL_M) as usize;
    let dim = ctx.dim();

    for cy in 0..cdim {
        for cx in 0..cdim {
            let agg = agreguj(ctx, cx, cy, krok, dim);
            let i = cy * cdim + cx;
            let c = &mut ctx.world.climate[i];

            let temp = c.mean_annual_temp_dc();
            let opad = c.annual_precip_mm();
            c.biome = sklasyfikuj(temp, opad, &agg);
            c.soil_fertility = zyznosc(c.biome, temp, opad, &agg);
            c.water_table_depth_dm = agg.water_table_dm;
        }
    }
}

/// Agregat terenowy komórki klimatu — wszystko, czego klasyfikacja potrzebuje od terenu.
struct Agg {
    water_share: f32,
    /// Udział komórek roboczych, które są płaskie i zbierają spływ.
    wet_share: f32,
    dominant_water: WaterClass,
    mean_slope: u8,
    max_accum: f32,
    mean_height_m: i32,
    water_dist_m: u16,
    water_table_dm: u16,
}

fn agreguj(ctx: &GenCtx, cx: usize, cy: usize, krok: usize, dim: usize) -> Agg {
    let (mut woda, mut ile) = (0u32, 0u32);
    let mut mokre = 0u32;
    let (mut sea, mut lake, mut river) = (0u32, 0u32, 0u32);
    let mut suma_h = 0i64;
    let mut max_accum = 0.0f32;
    let mut suma_slope = 0u32;

    for y in cy * krok..(cy * krok + krok).min(dim) {
        for x in cx * krok..(cx * krok + krok).min(dim) {
            let i = y * dim + x;
            ile += 1;
            suma_h += i64::from(ctx.world.height[i]);
            max_accum = max_accum.max(ctx.work.flow_acc[i]);
            let s = nachylenie(ctx, x, y);
            suma_slope += u32::from(s);
            if s < MARSH_MAX_SLOPE && ctx.work.flow_acc[i] >= MARSH_MIN_ACCUM {
                mokre += 1;
            }
            match ctx.world.water[i].class() {
                WaterClass::Dry => {}
                WaterClass::Sea => {
                    woda += 1;
                    sea += 1;
                }
                WaterClass::Lake => {
                    woda += 1;
                    lake += 1;
                }
                WaterClass::River => {
                    woda += 1;
                    river += 1;
                }
            }
        }
    }
    let ile = ile.max(1);
    let dominant_water = if sea >= lake && sea >= river && sea > 0 {
        WaterClass::Sea
    } else if lake >= river && lake > 0 {
        WaterClass::Lake
    } else if river > 0 {
        WaterClass::River
    } else {
        WaterClass::Dry
    };

    let cdist = ctx.world.water_dist.dim();
    let water_dist_m = *ctx.world.water_dist.get(
        (cx * krok / 4).min(cdist - 1),
        (cy * krok / 4).min(cdist - 1),
    );
    let mean_slope = (suma_slope / ile) as u8;

    Agg {
        water_share: woda as f32 / ile as f32,
        wet_share: mokre as f32 / ile as f32,
        dominant_water,
        mean_slope,
        max_accum,
        mean_height_m: (suma_h / i64::from(ile)) as i32 / 10,
        water_dist_m,
        water_table_dm: poziom_wod(mean_slope, water_dist_m),
    }
}

fn nachylenie(ctx: &GenCtx, x: usize, y: usize) -> u8 {
    let h =
        |dx: i64, dy: i64| f32::from(*ctx.world.height.get_clamped(x as i64 + dx, y as i64 + dy));
    let dzdx = (h(1, 0) - h(-1, 0)) / (2.0 * WORK_CELL_M as f32 * 10.0);
    let dzdy = (h(0, 1) - h(0, -1)) / (2.0 * WORK_CELL_M as f32 * 10.0);
    let tan = magnat_core::det_math::sqrt(f64::from(dzdx * dzdx + dzdy * dzdy)) as f32;
    (tan * 64.0).clamp(0.0, 255.0) as u8
}

/// Głębokość zwierciadła wód gruntowych w decymetrach poniżej terenu.
fn poziom_wod(slope: u8, water_dist_m: u16) -> u16 {
    let blisko = (1.0 - f32::from(water_dist_m.min(800)) / 800.0).clamp(0.0, 1.0);
    let stromo = f32::from(slope) / 255.0;
    let m = 0.5 + (1.0 - blisko) * 12.0 + stromo * 8.0;
    (m * 10.0).clamp(0.0, f32::from(u16::MAX)) as u16
}

/// Klasyfikacja biomu. Nadpisania idą przed regułą Whittakera — patrz nagłówek modułu.
fn sklasyfikuj(temp_dc: i16, precip_mm: u32, a: &Agg) -> Biome {
    if a.water_share >= WATER_DOMINANT {
        return match a.dominant_water {
            WaterClass::Sea => Biome::Sea,
            WaterClass::Lake => Biome::Lake,
            WaterClass::River => Biome::River,
            WaterClass::Dry => Biome::Grassland,
        };
    }
    if a.mean_slope >= ROCK_SLOPE {
        return Biome::Rock;
    }
    // Śnieg wieczny: średnia roczna poniżej −2 °C. W zakresie wysokości tego świata
    // (do 192 m) to się nie zdarzy na żadnej z przyjętych szerokości — wariant istnieje
    // dla regionów, które mogą dojść w M12, i dla pewności, że klasyfikacja jest zupełna.
    if temp_dc < -20 {
        return Biome::Snow;
    }
    // Mokradło: większość komórki jest płaska i zbiera spływ, a woda stoi blisko.
    if a.wet_share >= MARSH_MIN_SHARE && a.water_dist_m < 300 {
        return Biome::Marsh;
    }

    let t = f32::from(temp_dc) / 10.0;
    let p = precip_mm as f32;

    // Whittaker w postaci progowej. Granice są celowo ostre: rozmycie i tak wprowadza
    // rozdzielczość siatki 256 m, a progi da się przeczytać i zakwestionować.
    if p < 250.0 {
        return if t > 18.0 { Biome::Sand } else { Biome::Scrub };
    }
    if p < 500.0 {
        return if t > 15.0 {
            Biome::Scrub
        } else {
            Biome::Grassland
        };
    }
    if t < 4.0 {
        return Biome::ConiferForest;
    }
    if t < 9.0 {
        return if p > 800.0 {
            Biome::ConiferForest
        } else {
            Biome::MixedForest
        };
    }
    if p > 900.0 {
        Biome::BroadleafForest
    } else if p > 650.0 {
        Biome::MixedForest
    } else {
        Biome::Grassland
    }
}

/// Żyzność gleby 0..=100. Konsument: M5/M6 (plon), M2 (strefowanie rolne).
fn zyznosc(biome: Biome, temp_dc: i16, precip_mm: u32, a: &Agg) -> Q {
    if biome.is_water() || matches!(biome, Biome::Rock | Biome::Snow) {
        return Q::MIN;
    }
    // Punkt wyjścia: optimum przy 8–15 °C i 600–900 mm.
    let t = f32::from(temp_dc) / 10.0;
    let temp_score = 1.0 - ((t - 11.0).abs() / 14.0).clamp(0.0, 1.0);
    let p = precip_mm as f32;
    let precip_score = 1.0 - ((p - 750.0).abs() / 700.0).clamp(0.0, 1.0);
    // Stromizna zabiera glebę, dolina ją gromadzi.
    let slope_score = 1.0 - (f32::from(a.mean_slope) / 120.0).clamp(0.0, 1.0);
    // Mady w dolinach rzecznych są najżyźniejsze — stąd premia za bliskość wody.
    let dolina = 1.0 - (f32::from(a.water_dist_m.min(1200)) / 1200.0) * 0.35;

    let mut v = temp_score * 0.3 + precip_score * 0.3 + slope_score * 0.4;
    v *= dolina;
    if matches!(biome, Biome::Marsh) {
        // Torf jest żyzny dopiero po osuszeniu — na mokro nie da się na nim uprawiać.
        v *= 0.45;
    }
    if matches!(biome, Biome::Sand) {
        v *= 0.25;
    }
    let _ = (a.mean_height_m, a.max_accum);
    Q::new((v * 100.0).clamp(0.0, 100.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn swiat(region: Region) -> GenCtx<'static> {
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(2)));
        let params = WorldGenParams {
            seed: 4,
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        for pass in crate::pipeline::PASSES.iter() {
            (pass.run)(&mut ctx);
        }
        ctx
    }

    fn udzial(ctx: &GenCtx, f: impl Fn(Biome) -> bool) -> f32 {
        let n = ctx.world.climate.len();
        ctx.world
            .climate
            .as_slice()
            .iter()
            .filter(|c| f(c.biome))
            .count() as f32
            / n as f32
    }

    #[test]
    fn pustynia_nie_generuje_lasu_lisciastego() {
        // M1 §7.2 `biome_plausible`.
        let ctx = swiat(Region::Desert);
        assert_eq!(
            udzial(&ctx, |b| b == Biome::BroadleafForest),
            0.0,
            "las liściasty na pustyni"
        );
        assert!(
            udzial(&ctx, |b| matches!(b, Biome::Sand | Biome::Scrub)) > 0.5,
            "pustynia bez piasku i zarośli"
        );
    }

    #[test]
    fn wybrzeze_ma_linie_brzegowa_i_morze_w_biomach() {
        let ctx = swiat(Region::Coastal);
        assert!(
            udzial(&ctx, |b| b == Biome::Sea) > 0.1,
            "wybrzeże bez morza w biomach"
        );
        assert!(udzial(&ctx, |b| !b.is_water()) > 0.3, "wybrzeże bez lądu");
    }

    #[test]
    fn region_wilgotny_ma_lasy() {
        let ctx = swiat(Region::River);
        assert!(
            udzial(&ctx, Biome::is_forest) > 0.2,
            "region rzeczny bez lasów: {:?}",
            ctx.world.climate.as_slice()[0].biome
        );
    }

    #[test]
    fn zyznosc_jest_zerowa_na_wodzie_i_skale_a_dodatnia_na_polu() {
        let ctx = swiat(Region::Lowland);
        let mut rolnicze = 0;
        for c in ctx.world.climate.as_slice() {
            if c.biome.is_water() || matches!(c.biome, Biome::Rock | Biome::Snow) {
                assert_eq!(c.soil_fertility, Q::MIN, "żyzna woda albo żyzna skała");
            } else if c.soil_fertility.get() > 30 {
                rolnicze += 1;
            }
        }
        assert!(rolnicze > 0, "nizina bez gruntów rolnych");
    }

    #[test]
    fn poziom_wod_gruntowych_jest_ustawiony_i_rozny() {
        let ctx = swiat(Region::River);
        let v: Vec<u16> = ctx
            .world
            .climate
            .as_slice()
            .iter()
            .map(|c| c.water_table_depth_dm)
            .collect();
        assert!(
            v.iter().any(|d| *d > 0),
            "zwierciadło wszędzie na powierzchni"
        );
        assert!(
            v.iter().copied().max().unwrap() > v.iter().copied().min().unwrap(),
            "zwierciadło nie reaguje na teren"
        );
    }
}
