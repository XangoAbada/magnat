//! Pogoda — zaślepka deterministyczna (M4 decyzja otwarta **D4**).
//!
//! Właścicielem pogody jest **M8** (§9.6 i zdarzenia miejskie). M4 potrzebuje jej
//! wcześniej, bo `DiscomfortBreakdown.weather` rozstrzyga o udziale roweru i pieszych
//! w wyborze środka transportu (§5.3): deszcz i mróz bolą pieszego i rowerzystę,
//! a kierowcy nie. Bez tego składnika udział roweru byłby stały przez cały rok,
//! czyli fałszywy w sposób widoczny gołym okiem.
//!
//! Dlatego zaślepka stoi **w `core`, za docelową sygnaturą**: M8 podmienia ciało
//! [`weather_at`] na realny system klimatu z M1, a `sim/traffic` nie zmienia ani
//! jednej linii. To ta sama zasada, co przy `FuelPrices` (`D9`) — punkt podmiany
//! projektuje się wtedy, gdy powstaje pierwszy konsument, a nie wtedy, gdy powstaje
//! właściciel.
//!
//! Model jest **całkowitoliczbowy i bez funkcji przestępnych** (`K-6`): temperatura
//! to trójkąt roczny, opad — losowanie o sezonowym prawdopodobieństwie. Żadnego
//! sinusa, bo `f64::sin` jest tak samo libmowy jak `exp`, a różnica między sinusem
//! a trójkątem jest dla wyboru roweru nieodczuwalna.

use crate::rng::{rng, StreamId, NO_ENTITY};
use crate::time::{DAYS_PER_MONTH, MONTHS_PER_YEAR};
use crate::types::Tick;

/// Dni w roku gry (`K-1`: 12 × 30, bez lat przestępnych).
const DAYS_PER_YEAR: u64 = DAYS_PER_MONTH * MONTHS_PER_YEAR;

/// Temperatura najzimniejszej i najcieplejszej doby roku, w dziesiątych stopnia.
/// Wartości odpowiadają klimatowi umiarkowanemu — M8 weźmie je z `data/climate/`.
const TEMP_MIN_DC: i32 = -50;
const TEMP_MAX_DC: i32 = 225;

/// Doba, w której wypada szczyt lata. 30 × 6 + 15 = połowa siódmego miesiąca.
const SUMMER_PEAK_DAY: u64 = 195;

/// Prawdopodobieństwo opadu w promilach: zimą i jesienią częściej niż latem.
const RAIN_WINTER_PERMILLE: u16 = 420;
const RAIN_SUMMER_PERMILLE: u16 = 240;

/// Pogoda doby — tyle, ile potrzebuje wybór środka transportu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Weather {
    /// Temperatura w dziesiątych stopnia Celsjusza (`-50` = −5,0 °C).
    pub temp_dc: i16,
    /// Natężenie opadu w promilach: 0 = sucho, 1000 = ulewa.
    pub precip_permille: u16,
    /// Prędkość wiatru w km/h. Dopisane w M8c: wiatr jest sondą zdarzeń (wichura,
    /// zerwana linia) i składnikiem dyskomfortu pieszego. Zaślepka `weather_at`
    /// zostawia zero — wiatru nie ma, dopóki nie liczy go `sim/events`.
    pub wind_kmh: u16,
    /// Pokrywa śnieżna w milimetrach słupa wody. Dopisane w M8c: śnieg **zalega**,
    /// więc jest stanem, a nie pogodą doby — i to on spowalnia ruch po tym, jak
    /// przestało padać. Zaślepka zostawia zero.
    pub snow_cover_mm: u16,
}

impl Weather {
    /// Czy warunki są uciążliwe dla kogoś bez dachu nad głową — pieszego,
    /// rowerzysty, pasażera czekającego na przystanku.
    #[must_use]
    pub const fn is_harsh(self) -> bool {
        self.precip_permille > 200 || self.temp_dc < 0 || self.temp_dc > 280
    }

    /// Miara uciążliwości 0..=1000: opad plus kara za mróz i upał. Jedna liczba,
    /// bo wybór środka i tak mnoży ją przez ekspozycję środka i przez wartość czasu.
    #[must_use]
    pub fn harshness_permille(self) -> u16 {
        let mroz = if self.temp_dc < 0 {
            (i32::from(-self.temp_dc) * 4).min(400) as u16
        } else {
            0
        };
        let upal = if self.temp_dc > 280 {
            ((i32::from(self.temp_dc) - 280) * 3).min(300) as u16
        } else {
            0
        };
        self.precip_permille
            .saturating_add(mroz)
            .saturating_add(upal)
            .min(1_000)
    }
}

/// Pogoda w podanej dobie świata. Czysta funkcja `(seed, day)` — ta sama doba
/// w dwóch przebiegach tego samego ziarna daje tę samą pogodę, także wtedy, gdy
/// jeden z nich przewinął ją w innej kolejności (00 §3.1).
#[must_use]
pub fn weather_at(seed: u64, day: u64) -> Weather {
    let doba = day % DAYS_PER_YEAR;
    // Trójkąt roczny: od szczytu zimy do szczytu lata i z powrotem. `dystans` to
    // odległość doby od szczytu lata mierzona po okręgu roku.
    let dystans = {
        let d = doba.abs_diff(SUMMER_PEAK_DAY);
        d.min(DAYS_PER_YEAR - d)
    };
    let polrocze = DAYS_PER_YEAR / 2;
    let sezon = TEMP_MAX_DC - (TEMP_MAX_DC - TEMP_MIN_DC) * dystans as i32 / polrocze as i32;

    let mut r = rng(seed, StreamId::WeatherStub, NO_ENTITY, Tick(doba));
    // Wahanie dobowe ±4 °C wokół krzywej sezonowej — tyle, żeby marzec bywał
    // cieplejszy od kwietnia, a rowerzysta nie liczył na kalendarz.
    let wahanie = r.gen_range_u32(81) as i32 - 40;
    let temp_dc = (sezon + wahanie).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;

    let zima = 1_000 - (dystans as u32 * 1_000 / polrocze as u32);
    let prog = RAIN_SUMMER_PERMILLE as u32
        + (RAIN_WINTER_PERMILLE - RAIN_SUMMER_PERMILLE) as u32 * zima / 1_000;
    let precip_permille = if r.gen_bool_permille(prog as u16) {
        // Opad jest skośny: mży częściej, niż leje.
        let x = r.gen_range_u32(1_000);
        (x * x / 1_000).min(1_000) as u16
    } else {
        0
    };

    Weather {
        temp_dc,
        precip_permille,
        // Zaślepka nie liczy ani wiatru, ani zalegania śniegu — obie liczby są
        // stanem, a `weather_at` jest funkcją czystą doby. Wypełnia je `sim/events`.
        wind_kmh: 0,
        snow_cover_mm: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lato_jest_cieplejsze_od_zimy() {
        let mut lato = 0i64;
        let mut zima = 0i64;
        for rok in 0..10u64 {
            lato += i64::from(weather_at(7, rok * DAYS_PER_YEAR + SUMMER_PEAK_DAY).temp_dc);
            zima += i64::from(weather_at(7, rok * DAYS_PER_YEAR + 15).temp_dc);
        }
        assert!(
            lato / 10 > 180,
            "lipiec ma {} dziesiątych stopnia",
            lato / 10
        );
        assert!(
            zima / 10 < 0,
            "styczeń ma {} dziesiątych stopnia",
            zima / 10
        );
    }

    #[test]
    fn pogoda_jest_funkcja_doby_a_nie_kolejnosci() {
        let a: Vec<Weather> = (0..400).map(|d| weather_at(11, d)).collect();
        let b: Vec<Weather> = (0..400).rev().map(|d| weather_at(11, d)).collect();
        for (i, w) in b.iter().rev().enumerate() {
            assert_eq!(a[i], *w, "pogoda doby {i} zależy od kolejności odpytania");
        }
        assert_ne!(
            weather_at(11, 5),
            weather_at(12, 5),
            "ziarno nic nie zmienia"
        );
    }

    #[test]
    fn uciazliwosc_rosnie_z_mrozem_i_deszczem() {
        let sucho = Weather {
            temp_dc: 180,
            precip_permille: 0,
            ..Weather::default()
        };
        let deszcz = Weather {
            temp_dc: 180,
            precip_permille: 600,
            ..Weather::default()
        };
        let mroz = Weather {
            temp_dc: -100,
            precip_permille: 0,
            ..Weather::default()
        };
        assert_eq!(sucho.harshness_permille(), 0);
        assert!(!sucho.is_harsh());
        assert_eq!(deszcz.harshness_permille(), 600);
        assert!(mroz.harshness_permille() >= 400);
        assert!(mroz.is_harsh() && deszcz.is_harsh());
    }
}
