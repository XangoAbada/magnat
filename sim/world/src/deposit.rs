//! Złoża surowców (M1 §5.5, przebieg P9).
//!
//! **Kluczowa decyzja modelowa:** ekonomiczną prawdą o złożu jest całkowitoliczbowy bilans
//! masy w gramach (00 §2 — żadnych floatów w stanach magazynowych), a nie liczba voxeli.
//! Voxele są pochodną wizualną: kopalnia (M6) zgłasza wydobytą masę, [`Deposit::voxels_for`]
//! przelicza ją przez gęstość materiału na objętość wykopu, a `VoxelWorld::edit_region` drąży.
//!
//! Dzięki temu wyczerpanie złoża jest dokładne i testowalne własnościowo, działa **bez
//! rezydentnych voxeli** (kopalnia poza kadrem kamery), a w zapisie gry zostaje
//! `(DepositId, extracted)` — dwa `u64` na złoże.

use magnat_core::hash::StateHasher;
use magnat_core::{IVec2, IVec3, Mass, ResourceKind, Q};
use magnat_voxel::{MaterialId, VoxelMaterial};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct DepositId(pub u32);

/// Kształt formacji złożowej. Cztery warianty, bo tyle wystarcza do odróżnienia sposobów
/// wydobycia, które M6 będzie musiał modelować: gniazdo, pokład, pułapka, warstwa wodonośna.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DepositShape {
    /// Ruda, kruszywo. Promienie w metrach, `yaw_deg` obraca elipsoidę w poziomie.
    Ellipsoid {
        center: IVec3,
        radii: IVec3,
        yaw_deg: u16,
    },
    /// Pokład węgla — polilinia osi w metrach, stały strop i miąższość.
    Seam {
        polyline: Vec<IVec2>,
        top_z: i32,
        thickness_dm: u16,
    },
    /// Ropa lub gaz w pułapce pod warstwą nieprzepuszczalną.
    Trap {
        center: IVec3,
        radii: IVec3,
        cap_layer: MaterialId,
    },
    /// Warstwa wodonośna — wielokąt zasięgu i przedział głębokości.
    Aquifer {
        poly: Vec<IVec2>,
        top_z: i32,
        bottom_z: i32,
    },
}

impl DepositShape {
    /// Czy punkt (w metrach, `z` w decymetrach jak wysokości terenu) leży w formacji.
    #[must_use]
    pub fn contains(&self, p: IVec3) -> bool {
        match self {
            DepositShape::Ellipsoid { center, radii, .. }
            | DepositShape::Trap { center, radii, .. } => {
                // Elipsoida w arytmetyce wymiernej: Σ (d_i / r_i)² ≤ 1 przemnożone przez
                // iloczyn kwadratów promieni, żeby nie wchodzić w dzielenie.
                let d = p - *center;
                let (rx, ry, rz) = (
                    i64::from(radii.x.max(1)),
                    i64::from(radii.y.max(1)),
                    i64::from(radii.z.max(1)),
                );
                let (dx, dy, dz) = (i64::from(d.x), i64::from(d.y), i64::from(d.z));
                let lhs = dx * dx * (ry * ry) * (rz * rz)
                    + dy * dy * (rx * rx) * (rz * rz)
                    + dz * dz * (rx * rx) * (ry * ry);
                lhs <= rx * rx * ry * ry * rz * rz
            }
            DepositShape::Seam {
                polyline,
                top_z,
                thickness_dm,
            } => {
                if p.z > *top_z || p.z < *top_z - i32::from(*thickness_dm) {
                    return false;
                }
                let half = i64::from(seam_half_width_m(*thickness_dm));
                polyline
                    .windows(2)
                    .any(|w| dist_sq_to_segment(p.xy(), w[0], w[1]) <= half * half)
            }
            DepositShape::Aquifer {
                poly,
                top_z,
                bottom_z,
            } => p.z <= *top_z && p.z >= *bottom_z && point_in_polygon(p.xy(), poly),
        }
    }
}

/// Połowa szerokości pokładu w metrach, jako pochodna jego miąższości.
///
/// Pokłady są płaskie, nie sznurkowe: warstwa o miąższości 2,5 m ciągnie się na setki metrów
/// w poprzek. Mnożnik 8 jest tu **jedynym** źródłem tej proporcji — i dlatego jest funkcją,
/// a nie liczbą powtórzoną w teście przynależności i w rachunku objętości, gdzie rozjechałaby
/// się przy pierwszej zmianie (i rozjechała: objętość liczyła 40 m tam, gdzie kształt miał 200).
#[inline]
#[must_use]
pub fn seam_half_width_m(thickness_dm: u16) -> u32 {
    u32::from(thickness_dm) * 8
}

/// Kwadrat odległości punktu od odcinka, w arytmetyce całkowitej.
/// Zwraca `i64`, bo na mapie 16 km kwadrat odległości przekracza `i32`.
fn dist_sq_to_segment(p: IVec2, a: IVec2, b: IVec2) -> i64 {
    let (abx, aby) = (i64::from(b.x - a.x), i64::from(b.y - a.y));
    let (apx, apy) = (i64::from(p.x - a.x), i64::from(p.y - a.y));
    let len_sq = abx * abx + aby * aby;
    if len_sq == 0 {
        return apx * apx + apy * apy;
    }
    // Rzut na odcinek, zaciśnięty do [0, 1] — w postaci ułamka t = dot / len_sq.
    let dot = (apx * abx + apy * aby).clamp(0, len_sq);
    // Odległość od punktu rzutu: (ap - ab·t)², przemnożone przez len_sq², żeby uniknąć dzielenia.
    let cx = apx * len_sq - abx * dot;
    let cy = apy * len_sq - aby * dot;
    // Dzielimy raz, na końcu — wynik jest kwadratem odległości w jednostkach wejściowych.
    (cx / len_sq) * (cx / len_sq) + (cy / len_sq) * (cy / len_sq)
}

/// Test przynależności do wielokąta metodą promienia (even-odd), całkowitoliczbowo.
fn point_in_polygon(p: IVec2, poly: &[IVec2]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) {
            // Przecięcie promienia: porównanie ułamków bez dzielenia, ze znakiem mianownika.
            let dy = i64::from(b.y - a.y);
            let lhs = i64::from(p.x - a.x) * dy;
            let rhs = i64::from(b.x - a.x) * i64::from(p.y - a.y);
            if (dy > 0 && lhs < rhs) || (dy < 0 && lhs > rhs) {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Złoże. Encja ECS (00 §K-16: areny dostają partie i oferty, złoża zostają encjami —
/// jest ich ~500 do ~6000, są długowieczne i mutowane rzadko).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Deposit {
    pub id: DepositId,
    pub resource: ResourceKind,
    pub shape: DepositShape,
    /// Objętość geometryczna formacji.
    pub volume_m3: i64,
    /// Udział surowca w skale, 0..=100.
    pub concentration: Q,
    /// Gramy surowca — **wartość pierwotna**, całkowitoliczbowa.
    pub reserves: Mass,
    /// 0 na starcie; M6 wyłącznie inkrementuje.
    pub extracted: Mass,
    pub depth_top_m: i16,
    pub depth_bottom_m: i16,
    /// Badania geologiczne (M6) odkrywają złoże.
    pub discovered: bool,
    /// Wpływa na cenę i wydajność przerobu (M6).
    pub quality: Q,
}

impl Deposit {
    /// Zasoby liczone raz, przy generacji, w arytmetyce całkowitej:
    /// `gramy = m³ × kg/m³ × koncentracja/100 × 1000`.
    ///
    /// `checked_mul` na każdym kroku, bo przepełnienie tutaj oznaczałoby złoże z ujemną
    /// masą, a to psuje bilans własnościowy dopiero kilka faz później.
    #[must_use]
    pub fn reserves_from(volume_m3: i64, density_kg_m3: u16, concentration: Q) -> Mass {
        let kg = volume_m3
            .checked_mul(i64::from(density_kg_m3))
            .expect("objętość × gęstość przepełniła i64");
        let kg_of_resource = kg
            .checked_mul(i64::from(concentration.get()))
            .expect("masa × koncentracja przepełniła i64")
            / 100;
        Mass(
            kg_of_resource
                .checked_mul(1_000)
                .expect("kilogramy → gramy przepełniły i64"),
        )
    }

    #[inline]
    #[must_use]
    pub fn remaining(&self) -> Mass {
        Mass(self.reserves.0 - self.extracted.0)
    }

    /// Stopień wyczerpania, 0..=100.
    #[must_use]
    pub fn depletion(&self) -> Q {
        if self.reserves.0 <= 0 {
            return Q::MAX;
        }
        // Zaokrąglenie w dół: złoże jest „wyczerpane w 100 %" dopiero, gdy naprawdę jest.
        Q::new((self.extracted.0.saturating_mul(100) / self.reserves.0).clamp(0, 100) as u8)
    }

    /// Wydobycie. Zwraca masę faktycznie wydobytą — nigdy więcej niż zostało.
    /// To jedyna droga zmiany `extracted`; pole jest publiczne wyłącznie dla serializacji.
    pub fn extract(&mut self, want: Mass) -> Mass {
        let got = want.0.clamp(0, self.remaining().0);
        self.extracted = Mass(self.extracted.0 + got);
        Mass(got)
    }

    /// Ile voxeli usunąć, żeby wizualizacja odpowiadała wydobytej masie.
    ///
    /// Masa dotyczy **surowca**, a wykop obejmuje całą skałę wraz z płonną częścią, stąd
    /// dzielenie przez koncentrację. Voxel terenu to 1 × 1 × 0,5 m = 0,5 m³.
    #[must_use]
    pub fn voxels_for(&self, m: Mass, mat: &VoxelMaterial) -> u64 {
        if mat.density_kg_m3 == 0 || self.concentration.get() == 0 {
            return 0;
        }
        let rock_g = m.0.max(0).saturating_mul(100) / i64::from(self.concentration.get());
        let rock_kg = rock_g / 1_000;
        // 0,5 m³ na voxel → objętość×2 daje liczbę voxeli bez ułamka.
        let voxels = rock_kg.saturating_mul(2) / i64::from(mat.density_kg_m3);
        voxels.max(0) as u64
    }

    pub fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u8(self.resource.as_index() as u8);
        h.write_i64(self.volume_m3);
        h.write_u8(self.concentration.get());
        h.write_i64(self.reserves.0);
        h.write_i64(self.extracted.0);
        h.write_u16(self.depth_top_m as u16);
        h.write_u16(self.depth_bottom_m as u16);
        h.write_u8(u8::from(self.discovered));
        h.write_u8(self.quality.get());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zloze(reserves: i64, concentration: u8) -> Deposit {
        Deposit {
            id: DepositId(1),
            resource: ResourceKind::Coal,
            shape: DepositShape::Ellipsoid {
                center: IVec3::new(100, 100, -200),
                radii: IVec3::new(50, 50, 20),
                yaw_deg: 0,
            },
            volume_m3: 1000,
            concentration: Q::new(concentration),
            reserves: Mass(reserves),
            extracted: Mass(0),
            depth_top_m: 10,
            depth_bottom_m: 30,
            discovered: false,
            quality: Q::new(50),
        }
    }

    #[test]
    fn zasoby_licza_sie_calkowitoliczbowo() {
        // 1000 m³ × 1350 kg/m³ × 60 % × 1000 g/kg = 810 000 000 g
        let r = Deposit::reserves_from(1000, 1350, Q::new(60));
        assert_eq!(r, Mass(810_000_000));
    }

    #[test]
    fn wydobycie_nigdy_nie_przekracza_zasobow() {
        let mut d = zloze(1_000, 50);
        assert_eq!(d.extract(Mass(400)), Mass(400));
        assert_eq!(d.remaining(), Mass(600));
        assert_eq!(
            d.extract(Mass(10_000)),
            Mass(600),
            "dostajemy tylko to, co zostało"
        );
        assert_eq!(d.remaining(), Mass(0));
        assert_eq!(d.depletion(), Q::MAX);
        assert_eq!(
            d.extract(Mass(1)),
            Mass(0),
            "z pustego złoża nic nie wychodzi"
        );
        // Żądanie ujemne nie może dopisać masy do złoża.
        assert_eq!(d.extract(Mass(-500)), Mass(0));
        assert_eq!(d.extracted, Mass(1_000));
    }

    #[test]
    fn elipsoida_zawiera_srodek_i_odrzuca_daleki_punkt() {
        let s = DepositShape::Ellipsoid {
            center: IVec3::new(0, 0, 0),
            radii: IVec3::new(10, 20, 5),
            yaw_deg: 0,
        };
        assert!(s.contains(IVec3::ZERO));
        assert!(
            s.contains(IVec3::new(10, 0, 0)),
            "punkt na promieniu należy"
        );
        assert!(!s.contains(IVec3::new(11, 0, 0)));
        assert!(s.contains(IVec3::new(0, 20, 0)));
        assert!(!s.contains(IVec3::new(0, 0, 6)));
    }

    #[test]
    fn wielokat_rozroznia_wnetrze_od_zewnetrza() {
        let poly = vec![
            IVec2::new(0, 0),
            IVec2::new(100, 0),
            IVec2::new(100, 100),
            IVec2::new(0, 100),
        ];
        let s = DepositShape::Aquifer {
            poly,
            top_z: 0,
            bottom_z: -100,
        };
        assert!(s.contains(IVec3::new(50, 50, -50)));
        assert!(!s.contains(IVec3::new(150, 50, -50)));
        assert!(!s.contains(IVec3::new(50, 50, 10)), "ponad stropem");
    }
}
