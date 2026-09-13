//! P11 — wiatr dominujący, adwekcja wilgoci i opad orograficzny (M1 §5.6).
//!
//! Model jednokierunkowy: wilgoć wchodzi na mapę z nawietrznej krawędzi i wędruje po siatce
//! klimatu w kierunku wiatru. Na wzniesieniu wytrąca się proporcjonalnie do przyrostu
//! wysokości — to jest opad orograficzny; za grzbietem wilgoci już nie ma i powstaje **cień
//! opadowy** (M1 §7.2 `rain_shadow`).
//!
//! Przejście po komórkach idzie w kolejności **od nawietrznej do zawietrznej**, co gwarantuje,
//! że komórka nawietrzna ma policzoną wilgoć, zanim potrzebuje jej sąsiad. Kierunek pętli
//! wynika ze znaku składowych wiatru, więc nie ma tu ani sortowania, ani kolejki —
//! i nie ma czego zrównoleglić, co akurat jest zaletą: wynik nie zależy od liczby wątków.

use crate::gen::shape::RegionShape;
use crate::params::Region;
use crate::params::{CLIMATE_CELL_M, WORK_CELL_M};
use crate::pipeline::GenCtx;

/// Ile milimetrów opadu przypada na metr wzniesienia terenu przy pełnej wilgotności.
const OROGRAPHIC_MM_PER_M: f32 = 5.0;
/// Ułamek wilgoci traconej na każde 100 m wysokości bezwzględnej.
///
/// Nie jest to powtórzenie składnika orograficznego: tamten reaguje na **przyrost**
/// wysokości, ten na jej poziom. Chłodniejsze powietrze wyżej mieści mniej pary, więc
/// masa, która już się wspięła, jest trwale uboższa — i dlatego cień opadowy utrzymuje się
/// także tam, gdzie teren przestał się wznosić.
const ALTITUDE_LOSS_PER_100M: f32 = 0.05;
/// Ułamek wilgoci tracony przez komórkę nawet na płaskim (opad konwekcyjny i frontalny).
const BASE_LOSS: f32 = 0.035;
/// Ile wilgoci odzyskuje masa powietrza nad wodą, na komórkę.
const RECHARGE_OVER_WATER: f32 = 0.25;

pub fn run(ctx: &mut GenCtx) {
    let cdim = ctx.params.climate_dim();
    let region = ctx.params.region;
    let shape = RegionShape::of(region);
    let wind_deg = prevailing_wind_deg(region);
    let (sx, sy) = wind_step(wind_deg);

    // Roczna suma opadu przy pełnej wilgotności — ta sama wartość, którą P7 wziął
    // do geometrii koryt, żeby rzeki i klimat mówiły o tym samym deszczu.
    let base_mm = f32::from(crate::gen::water::annual_precip_mm(shape.humidity));

    let wysokosci = srednie_wysokosci(ctx, cdim);
    let woda = udzial_wody(ctx, cdim);

    let mut moisture = vec![0.0f32; cdim * cdim];
    let mut precip = vec![0.0f32; cdim * cdim];

    // Kolejność pętli: zaczynamy od krawędzi, z której wieje.
    let xs: Vec<usize> = if sx >= 0 {
        (0..cdim).collect()
    } else {
        (0..cdim).rev().collect()
    };
    let ys: Vec<usize> = if sy >= 0 {
        (0..cdim).collect()
    } else {
        (0..cdim).rev().collect()
    };

    for &cy in &ys {
        for &cx in &xs {
            let i = cy * cdim + cx;
            let (ux, uy) = (cx as i64 - i64::from(sx), cy as i64 - i64::from(sy));
            let poza = ux < 0 || uy < 0 || ux >= cdim as i64 || uy >= cdim as i64;

            // Masa powietrza wchodząca na mapę niesie pełną wilgoć regionu.
            let (m_in, h_up) = if poza {
                (1.0f32, wysokosci[i])
            } else {
                let j = uy as usize * cdim + ux as usize;
                (moisture[j], wysokosci[j])
            };

            let m_in = (m_in + woda[i] * RECHARGE_OVER_WATER).min(1.0);
            let podnoszenie = (wysokosci[i] - h_up).max(0.0);

            // Opad: pełna podstawa regionalna plus składnik orograficzny, oba skalowane
            // wilgocią. Podstawa jest **pełna**, nie ułamkowa: `annual_precip_mm` z P7 opisuje
            // opad regionu na płaskim, a nie jakiś jego procent — wcześniejszy mnożnik 0,55
            // zaniżał cały świat o 45 % i region rzeczny wychodził stepem zamiast lasem.
            let p = base_mm * m_in * (1.0 + podnoszenie * OROGRAPHIC_MM_PER_M / base_mm);
            precip[i] = p;

            // Utrata wilgoci: opad orograficzny plus tło plus ubytek z wysokością.
            let strata = BASE_LOSS
                + podnoszenie * OROGRAPHIC_MM_PER_M / base_mm * 0.9
                + (wysokosci[i] / 100.0).max(0.0) * ALTITUDE_LOSS_PER_100M;
            moisture[i] = (m_in * (1.0 - strata)).clamp(0.0, 1.0);
        }
    }

    // Rozkład roczny: pora wilgotna latem, sucha zimą. Udział miesiąca jest stały w skali
    // regionu — sezonowość zdarzeniowa (susza, nawalnica) należy do M8 i nakłada się
    // na ten rozkład, nie zastępuje go (M1 §9 D7).
    // Lokalna zmienność opadu. Bez niej mapa biomów rozpada się na idealnie proste pasy
    // prostopadłe do wiatru — model jest wtedy poprawny i wygląda jak wykres, bo jedyną
    // zmienną wzdłuż pasa jest odległość od krawędzi. ±18 % o długości fali 2 km wystarcza,
    // żeby granice biomów zaczęły falować; nie zmienia to średnich ani cienia opadowego.
    let szum = crate::noise::NoiseField::new(ctx.params.seed, magnat_core::StreamId::WorldWind, 1);
    let szum_spec = crate::noise::FbmSpec::new(4, 2_000.0);

    let sezon = sezonowy_rozklad(region);
    for (i, roczny_bazowy) in precip.iter().enumerate() {
        let (cx, cy) = (i % cdim, i / cdim);
        let (mx, my) = (
            (cx as u32 * CLIMATE_CELL_M) as f32,
            (cy as u32 * CLIMATE_CELL_M) as f32,
        );
        let lokalnie = 1.0 + crate::noise::fbm(&szum, mx, my, szum_spec) * 0.18;
        let roczny = (roczny_bazowy * lokalnie).clamp(0.0, 4000.0);
        let c = &mut ctx.world.climate[i];
        for (mies, udzial) in sezon.iter().enumerate() {
            c.precip_monthly_mm[mies] = (roczny * udzial).clamp(0.0, f32::from(u16::MAX)) as u16;
        }
        c.prevailing_wind_deg = wind_deg;
    }
}

/// Kierunek wiatru dominującego w stopniach (0 = z północy, 90 = ze wschodu).
#[must_use]
pub fn prevailing_wind_deg(region: Region) -> u16 {
    match region {
        // Strefa umiarkowana: wiatry zachodnie.
        Region::Coastal | Region::Lowland | Region::River => 270,
        // Pasmo górskie jest w tym świecie zorientowane z południa na północ (rośnie ku
        // północy, `RegionShape::tilt_m`), więc wiatr **z południa** wspina się na nie
        // czołowo. To nie jest realizm szerokości umiarkowanych — to warunek, żeby efekt
        // orograficzny w ogóle był widoczny na mapie o jednym dominującym spadku.
        Region::Mountain => 180,
        // Strefa podzwrotnikowa: pasaty ze wschodu.
        Region::Desert => 75,
    }
}

/// Krok siatki w kierunku, w którym **przemieszcza się** powietrze.
///
/// Uwaga na konwencję: kierunek wiatru podaje się jako ten, **z którego** wieje, więc wiatr
/// zachodni (270°) przesuwa masę powietrza na wschód. Pomylenie tego obraca cień opadowy
/// na drugą stronę pasma, co wygląda wiarygodnie i jest po cichu nieprawdą.
fn wind_step(from_deg: u16) -> (i8, i8) {
    let to = (u32::from(from_deg) + 180) % 360;
    match to {
        23..=67 => (1, 1),
        68..=112 => (1, 0),
        113..=157 => (1, -1),
        158..=202 => (0, -1),
        203..=247 => (-1, -1),
        248..=292 => (-1, 0),
        293..=337 => (-1, 1),
        _ => (0, 1),
    }
}

/// Rozkład roczny opadu: 12 udziałów sumujących się do jedności.
fn sezonowy_rozklad(region: Region) -> [f32; 12] {
    // Maksimum latem. Pustynia ma rozkład skrajnie nierówny — pora deszczowa i dziewięć
    // miesięcy suszy; klimat umiarkowany rozkłada opad łagodniej.
    let kontrast = if matches!(region, Region::Desert) {
        0.75
    } else {
        0.28
    };
    let mut out = [0.0f32; 12];
    let mut suma = 0.0f32;
    for (m, o) in out.iter_mut().enumerate() {
        let faza = (m as f64 + 0.5) / 12.0 * std::f64::consts::TAU;
        let v = 1.0 - kontrast * magnat_core::det_math::cos(faza) as f32;
        *o = v;
        suma += v;
    }
    for o in &mut out {
        *o /= suma;
    }
    out
}

fn srednie_wysokosci(ctx: &GenCtx, cdim: usize) -> Vec<f32> {
    let dim = ctx.dim();
    let krok = (CLIMATE_CELL_M / WORK_CELL_M) as usize;
    let mut out = Vec::with_capacity(cdim * cdim);
    for cy in 0..cdim {
        for cx in 0..cdim {
            let mut suma = 0i64;
            let mut ile = 0i64;
            for y in cy * krok..(cy * krok + krok).min(dim) {
                for x in cx * krok..(cx * krok + krok).min(dim) {
                    suma += i64::from(ctx.world.height[y * dim + x]);
                    ile += 1;
                }
            }
            out.push(if ile > 0 {
                (suma / ile) as f32 / 10.0
            } else {
                0.0
            });
        }
    }
    out
}

/// Udział powierzchni wodnej w komórce klimatu, 0..1 — źródło wilgoci dla adwekcji.
fn udzial_wody(ctx: &GenCtx, cdim: usize) -> Vec<f32> {
    let dim = ctx.dim();
    let krok = (CLIMATE_CELL_M / WORK_CELL_M) as usize;
    let mut out = Vec::with_capacity(cdim * cdim);
    for cy in 0..cdim {
        for cx in 0..cdim {
            let mut woda = 0u32;
            let mut ile = 0u32;
            for y in cy * krok..(cy * krok + krok).min(dim) {
                for x in cx * krok..(cx * krok + krok).min(dim) {
                    if ctx.world.water[y * dim + x].class().is_water() {
                        woda += 1;
                    }
                    ile += 1;
                }
            }
            out.push(if ile > 0 {
                woda as f32 / ile as f32
            } else {
                0.0
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn swiat(region: Region, size: WorldSize) -> GenCtx<'static> {
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(2)));
        let params = WorldGenParams {
            seed: 3,
            size,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        for pass in crate::pipeline::PASSES.iter().take(11) {
            (pass.run)(&mut ctx);
        }
        ctx
    }

    #[test]
    fn kierunek_wiatru_przesuwa_powietrze_w_przeciwna_strone() {
        // Wiatr „z zachodu" (270°) niesie powietrze na wschód, czyli w stronę rosnącego X.
        assert_eq!(wind_step(270), (1, 0));
        // Wiatr z południa (180°) niesie na północ, czyli w stronę rosnącego Y.
        assert_eq!(wind_step(180), (0, 1));
        assert_eq!(wind_step(0), (0, -1));
        assert_eq!(wind_step(90), (-1, 0));
    }

    #[test]
    fn rozklad_sezonowy_sumuje_sie_do_jednosci() {
        for r in Region::ALL {
            let s = sezonowy_rozklad(*r);
            let suma: f32 = s.iter().sum();
            assert!((suma - 1.0).abs() < 1e-4, "{}: suma {suma}", r.key());
            assert!(s.iter().all(|v| *v > 0.0), "{}: miesiąc bez opadu", r.key());
            // Lato mokrzejsze od zimy.
            assert!(s[6] > s[0], "{}: styczeń mokrzejszy od lipca", r.key());
        }
    }

    #[test]
    fn region_gorski_ma_cien_opadowy() {
        // M1 §7.2 `rain_shadow`: opad po stronie zawietrznej < 60 % nawietrznej.
        // Sprawdzamy na ośmiu ziarnach — model orograficzny ma działać na terenie,
        // jaki wyjdzie, a nie na jednym szczęśliwym.
        let mut zdane = 0;
        for seed in 0..8u64 {
            let pool = JobPool::new(2);
            let params = WorldGenParams {
                seed,
                size: WorldSize::Medium8km,
                region: Region::Mountain,
                ..WorldGenParams::default()
            };
            let mut ctx = GenCtx::new(params, &pool);
            for pass in crate::pipeline::PASSES.iter().take(11) {
                (pass.run)(&mut ctx);
            }
            let cdim = ctx.world.climate.dim();
            // Wiatr z południa → nawietrzna to południowa jedna trzecia mapy (małe `y`).
            let sr = |od: usize, do_: usize| {
                let mut s = 0u64;
                let mut n = 0u64;
                for cy in od..do_ {
                    for cx in 0..cdim {
                        s += u64::from(ctx.world.climate.get(cx, cy).annual_precip_mm());
                        n += 1;
                    }
                }
                s as f64 / n as f64
            };
            let nawietrzna = sr(0, cdim / 3);
            let zawietrzna = sr(cdim * 2 / 3, cdim);
            if zawietrzna < nawietrzna * 0.6 {
                zdane += 1;
            }
        }
        assert!(
            zdane >= 7,
            "cień opadowy widoczny tylko w {zdane}/8 światach"
        );
    }

    #[test]
    fn pustynia_dostaje_wielokrotnie_mniej_deszczu_niz_wybrzeze() {
        let sr = |r: Region| {
            let ctx = swiat(r, WorldSize::Small4km);
            let s: u64 = ctx
                .world
                .climate
                .as_slice()
                .iter()
                .map(|c| u64::from(c.annual_precip_mm()))
                .sum();
            s / ctx.world.climate.len() as u64
        };
        let pustynia = sr(Region::Desert);
        let wybrzeze = sr(Region::Coastal);
        assert!(pustynia < 400, "pustynia z opadem {pustynia} mm");
        assert!(
            wybrzeze > pustynia * 2,
            "wybrzeże {wybrzeze} mm wobec pustyni {pustynia} mm"
        );
    }

    #[test]
    fn opad_roczny_zgadza_sie_z_suma_miesiecy() {
        let ctx = swiat(Region::River, WorldSize::Small4km);
        for c in ctx.world.climate.as_slice() {
            let suma = c.annual_precip_mm();
            assert!(suma > 0, "komórka bez opadu");
            assert!(
                suma < 5000,
                "opad {suma} mm/rok to nie jest klimat, tylko błąd jednostek"
            );
        }
    }
}
