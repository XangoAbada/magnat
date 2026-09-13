//! Klimat i biomy (M1 §5.5, przebiegi P10–P12).
//!
//! `ClimateCell` jest **niezmienna** — to klimatologia, czyli roczny cykl wyprowadzony
//! z seeda. Pogoda jako zdarzenie (burza, susza, powódź) należy do M8 i **nakłada
//! odchylenie** na ten cykl, nigdy go nie modyfikuje (M1 §9 D7). Inaczej klimat przestaje
//! być regenerowalny z seeda i musiałby wejść do zapisu gry.

use magnat_core::hash::StateHasher;
use magnat_core::{Biome, Q};
use serde::{Deserialize, Serialize};

/// Komórka siatki klimatu 256 m. Kalendarz 360-dniowy = 12 miesięcy × 30 (00 §K-1),
/// więc tablice miesięczne interpolują się równomiernie, bez przypadków szczególnych
/// na luty i lata przestępne.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ClimateCell {
    /// Dziesiąte części °C — bez floatów w stanie trwałym (00 §2).
    pub temp_monthly_dc: [i16; 12],
    pub precip_monthly_mm: [u16; 12],
    pub biome: Biome,
    /// 0..=100; M5/M6 przeliczy na plon.
    pub soil_fertility: Q,
    pub water_table_depth_dm: u16,
    pub prevailing_wind_deg: u16,
    /// Suma stopniodni grzania — konsumuje M8 (zapotrzebowanie na ciepło).
    pub heating_degree_days: u16,
}

impl Default for ClimateCell {
    fn default() -> Self {
        ClimateCell {
            temp_monthly_dc: [0; 12],
            precip_monthly_mm: [0; 12],
            biome: Biome::Grassland,
            soil_fertility: Q::MIN,
            water_table_depth_dm: 0,
            prevailing_wind_deg: 0,
            heating_degree_days: 0,
        }
    }
}

impl ClimateCell {
    /// Średnia roczna temperatura w dziesiątych częściach °C.
    /// Miesiące są równe (30 dni), więc średnia arytmetyczna jest średnią ważoną czasem —
    /// to jest praktyczny zysk z kalendarza 360-dniowego (00 §K-1).
    #[must_use]
    pub fn mean_annual_temp_dc(&self) -> i16 {
        let sum: i32 = self.temp_monthly_dc.iter().map(|t| i32::from(*t)).sum();
        (sum / 12) as i16
    }

    /// Suma roczna opadu w mm.
    #[must_use]
    pub fn annual_precip_mm(&self) -> u32 {
        self.precip_monthly_mm.iter().map(|p| u32::from(*p)).sum()
    }

    /// Temperatura w zadanym dniu roku (0..=359), interpolowana liniowo między środkami
    /// miesięcy. Środek miesiąca `m` wypada w dniu `30m + 15`.
    #[must_use]
    pub fn temp_at_day_dc(&self, day_of_year: u16) -> i16 {
        let d = i32::from(day_of_year % 360);
        // Pozycja względem środków miesięcy, przesunięta o 15 dni.
        let shifted = d - 15;
        let (m0, frac) = if shifted < 0 {
            (11i32, shifted + 30)
        } else {
            (shifted / 30, shifted % 30)
        };
        let a = i32::from(self.temp_monthly_dc[(m0 % 12) as usize]);
        let b = i32::from(self.temp_monthly_dc[((m0 + 1) % 12) as usize]);
        (a + (b - a) * frac / 30) as i16
    }

    pub fn hash_state(&self, h: &mut StateHasher) {
        for t in self.temp_monthly_dc {
            h.write_u16(t as u16);
        }
        for p in self.precip_monthly_mm {
            h.write_u16(p);
        }
        h.write_u8(self.biome.as_index() as u8);
        h.write_u8(self.soil_fertility.get());
        h.write_u16(self.water_table_depth_dm);
        h.write_u16(self.prevailing_wind_deg);
        h.write_u16(self.heating_degree_days);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cela() -> ClimateCell {
        // Roczny przebieg zbliżony do klimatu umiarkowanego: styczeń −20 dC, lipiec 180 dC.
        ClimateCell {
            temp_monthly_dc: [-20, -10, 30, 80, 130, 170, 180, 175, 130, 80, 30, 0],
            precip_monthly_mm: [30, 28, 34, 40, 58, 72, 80, 70, 50, 42, 40, 36],
            ..ClimateCell::default()
        }
    }

    #[test]
    fn interpolacja_dobowa_trafia_w_srodki_miesiecy() {
        let c = cela();
        assert_eq!(c.temp_at_day_dc(15), -20, "środek stycznia");
        assert_eq!(c.temp_at_day_dc(45), -10, "środek lutego");
        assert_eq!(c.temp_at_day_dc(30 * 6 + 15), 180, "środek lipca");
        // Między środkami — dokładnie w połowie drogi.
        assert_eq!(c.temp_at_day_dc(30), -15);
    }

    #[test]
    fn przejscie_przez_koniec_roku_jest_ciagle() {
        let c = cela();
        // Dzień 359 leży między środkiem grudnia (345) a środkiem stycznia następnego roku.
        let grudzien = c.temp_at_day_dc(345);
        let styczen = c.temp_at_day_dc(15);
        let koniec = c.temp_at_day_dc(359);
        assert!(
            koniec <= grudzien && koniec >= styczen,
            "{grudzien} {koniec} {styczen}"
        );
        // Dzień 0 i dzień 360 to ten sam punkt cyklu.
        assert_eq!(c.temp_at_day_dc(0), c.temp_at_day_dc(360));
    }

    #[test]
    fn agregaty_roczne() {
        let c = cela();
        assert_eq!(c.annual_precip_mm(), 580);
        assert_eq!(c.mean_annual_temp_dc(), 81);
    }
}
