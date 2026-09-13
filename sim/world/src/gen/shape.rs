//! Parametry kształtowania terenu per region (M1 §5.5).
//!
//! **Dlaczego w kodzie, a nie w RON.** Zbiór regionów jest zamkniętym enumem — dołożenie
//! regionu i tak wymaga rekompilacji, więc plik danych dawałby tu wyłącznie iluzję
//! rozszerzalności (00 §5 mówi o danych, które faktycznie rosną: towary, receptury, budynki).
//! Parametry erozji są odwrotnym przypadkiem i dlatego **są** w `data/geology/erosion.ron`:
//! stroi się je bez zmiany kodu, wielokrotnie, na podstawie oglądanego wyniku.
//!
//! Zakres pionowy świata to −64 m … +192 m n.p.m. (M1 §5.1, 512 voxeli po 0,5 m).
//! To jest twardy sufit każdej wartości poniżej — „góry" w tej grze są pogórzem, nie Alpami,
//! i tak też brzmi szerokość geograficzna przypisana regionowi górskiemu.

use crate::params::Region;

/// Najwyższy i najniższy punkt świata w metrach (M1 §5.1).
pub const WORLD_MAX_M: f32 = 192.0;
pub const WORLD_MIN_M: f32 = -64.0;

#[derive(Clone, Copy, Debug)]
pub struct RegionShape {
    /// Docelowa wysokość najwyższych partii lądu w metrach.
    pub relief_m: f32,
    /// Wysokość bazowa lądu przy dolnej (południowej) krawędzi mapy.
    pub base_m: f32,
    /// Regionalne nachylenie: o ile metrów północna krawędź mapy leży wyżej od południowej.
    ///
    /// **To nie jest ozdobnik, tylko warunek działania hydrologii.** Bez dominującego
    /// spadku ridged multifractal tworzy komórkową sieć zamkniętych bowli, z których woda
    /// nie ma dokąd odpłynąć — w regionie górskim jeziora zajmowały wtedy jedną trzecią
    /// mapy. Mapa 16 km jest wycinkiem większego krajobrazu i musi mieć kierunek odpływu,
    /// tak jak ma go każdy realny wycinek 16 km.
    pub tilt_m: f32,
    /// Długość fali pierwszej oktawy w metrach.
    pub wavelength_m: f32,
    pub octaves: u8,
    /// Siła zniekształcenia dziedziny w metrach.
    pub warp_m: f32,
    /// Czy podstawą jest ridged multifractal (grzbiety) zamiast fBm (pagórki).
    pub ridged: bool,
    /// Wypiętrzenie `U` w mm/rok — środek zakresu (M1 §5.7, wartości wg planu).
    pub uplift_mm_yr: f32,
    /// Podatność na erozję `K` — środek zakresu, w jednostkach 10⁻⁶.
    ///
    /// **Rząd wielkości wyżej niż tabela w M1 §5.7 (3·10⁻⁶ … 3·10⁻⁵)** i to jest świadoma
    /// kalibracja. Rządzą dwie wielkości pochodne, nie samo `K`:
    ///
    /// * nachylenie koryta w stanie ustalonym `S = U / (K·A^m)` — przy planowym `K` wychodzi
    ///   20 % w zlewni 1 km², czyli 800 m różnicy na długości rzeki. W świecie o zakresie
    ///   pionowym 256 m (§5.1) nie ma na to miejsca;
    /// * tempo zbieżności `C = K·dt·A^m/d` — przy planowym `K` wynosi 0,1, więc przez
    ///   80 iteracji koryto pogłębia się o centymetry i sieć dolin w ogóle nie powstaje.
    ///
    /// Wartości poniżej dają `S` rzędu 0,1–2 % zależnie od regionu i `C` rzędu jedności.
    /// Literaturowe `K` odnosi się do siatek 30–90 m i milionów lat; my mamy 4 m i 12 tys. lat.
    pub erodibility_e6: f32,
    /// Maksymalna głębokość morza w metrach (dodatnia liczba).
    pub sea_depth_m: f32,
    /// Bazowa wilgotność regionu, 0..=100 — wejście do P11.
    pub humidity: u8,
}

impl RegionShape {
    #[must_use]
    pub const fn of(region: Region) -> RegionShape {
        match region {
            // Pogórze: grzbiety, duża amplituda, silne wypiętrzenie, twarde skały.
            Region::Mountain => RegionShape {
                relief_m: 80.0,
                base_m: 10.0,
                tilt_m: 100.0,
                wavelength_m: 1_800.0,
                octaves: 9,
                warp_m: 260.0,
                ridged: true,
                uplift_mm_yr: 1.6,
                erodibility_e6: 80.0,
                sea_depth_m: 0.0,
                humidity: 60,
            },
            // Nizina: długie fale, mała amplituda — teren, po którym miasto rozlewa się szeroko.
            Region::Lowland => RegionShape {
                relief_m: 30.0,
                base_m: 12.0,
                tilt_m: 16.0,
                wavelength_m: 5_000.0,
                octaves: 7,
                warp_m: 400.0,
                ridged: false,
                uplift_mm_yr: 0.25,
                erodibility_e6: 240.0,
                sea_depth_m: 0.0,
                humidity: 55,
            },
            // Dolina dużej rzeki: wyraźne krawędzie doliny, dno szerokie i płaskie.
            Region::River => RegionShape {
                relief_m: 42.0,
                base_m: 10.0,
                tilt_m: 36.0,
                wavelength_m: 2_400.0,
                octaves: 9,
                warp_m: 320.0,
                ridged: false,
                uplift_mm_yr: 0.5,
                erodibility_e6: 200.0,
                sea_depth_m: 0.0,
                humidity: 62,
            },
            // Wybrzeże: łagodny profil brzegowy i szelf; ląd rzadko przekracza 60 m.
            Region::Coastal => RegionShape {
                relief_m: 58.0,
                base_m: 18.0,
                tilt_m: 0.0,
                wavelength_m: 4_200.0,
                octaves: 7,
                warp_m: 350.0,
                ridged: false,
                uplift_mm_yr: 0.3,
                erodibility_e6: 260.0,
                sea_depth_m: 55.0,
                humidity: 70,
            },
            // Pustynia: ostańce na płaskim podłożu, znikoma erozja rzeczna.
            Region::Desert => RegionShape {
                relief_m: 50.0,
                base_m: 18.0,
                tilt_m: 72.0,
                wavelength_m: 2_600.0,
                octaves: 9,
                warp_m: 300.0,
                ridged: true,
                uplift_mm_yr: 0.6,
                erodibility_e6: 50.0,
                sea_depth_m: 0.0,
                humidity: 12,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zaden_region_nie_wychodzi_poza_zakres_pionowy_swiata() {
        for &r in Region::ALL {
            let s = RegionShape::of(r);
            assert!(
                s.base_m + s.tilt_m + s.relief_m <= WORLD_MAX_M,
                "{}: {} + {} + {} przekracza sufit {WORLD_MAX_M} m",
                r.key(),
                s.base_m,
                s.tilt_m,
                s.relief_m
            );
            assert!(
                -s.sea_depth_m >= WORLD_MIN_M,
                "{}: morze głębsze niż dno świata",
                r.key()
            );
            assert_eq!(
                s.tilt_m > 0.0,
                !r.has_sea(),
                "{}: region bez morza musi mieć kierunek odpływu, region nadmorski ma nim morze",
                r.key()
            );
            assert_eq!(
                s.sea_depth_m > 0.0,
                r.has_sea(),
                "{}: głębokość morza nie zgadza się z has_sea()",
                r.key()
            );
        }
    }
}
