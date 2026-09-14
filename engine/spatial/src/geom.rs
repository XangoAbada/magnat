//! Geometria zmiennoprzecinkowa indeksów przestrzennych.
//!
//! `Vec2` to `glam::Vec2` — biblioteka jest już w projekcie (render, `tools/magnat`),
//! więc crate nie dokłada trzeciego wektora 2D obok `core::IVec2` i glam.
//! Dozwolone przez 00 §2: float wolno w geometrii, zakaz dotyczy pieniądza i magazynów.
//! Determinizm: 00 §K-6 — mnożenie, dodawanie i `sqrt` w IEEE-754 są powtarzalne
//! międzyplatformowo; żadna funkcja przestępna tu nie występuje.

pub use glam::Vec2;

/// Prostokąt ograniczający, oba krańce **włącznie**.
///
/// Odwrotnie niż `core::IRect` (półotwarty) i to jest celowe: tam krańce są indeksami
/// voxeli, tu są współrzędnymi w metrach, a „parcela kończy się dokładnie na miedzy"
/// nie da się wyrazić przedziałem otwartym bez epsilona.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Aabb2 {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb2 {
    /// Zbiór pusty w sensie `union` — każdy punkt go powiększa.
    pub const EMPTY: Aabb2 = Aabb2 {
        min: Vec2::new(f32::INFINITY, f32::INFINITY),
        max: Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
    };

    #[must_use]
    pub const fn new(min: Vec2, max: Vec2) -> Aabb2 {
        Aabb2 { min, max }
    }

    #[must_use]
    pub fn from_points(a: Vec2, b: Vec2) -> Aabb2 {
        Aabb2 {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// Kwadrat opisany na kole — podstawa każdego zapytania promieniowego.
    #[must_use]
    pub fn from_center_radius(c: Vec2, r: f32) -> Aabb2 {
        Aabb2 {
            min: Vec2::new(c.x - r, c.y - r),
            max: Vec2::new(c.x + r, c.y + r),
        }
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.max.x < self.min.x || self.max.y < self.min.y
    }

    #[must_use]
    pub fn contains(self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    #[must_use]
    pub fn intersects(self, o: Aabb2) -> bool {
        self.min.x <= o.max.x
            && o.min.x <= self.max.x
            && self.min.y <= o.max.y
            && o.min.y <= self.max.y
    }

    #[must_use]
    pub fn union(self, o: Aabb2) -> Aabb2 {
        Aabb2 {
            min: self.min.min(o.min),
            max: self.max.max(o.max),
        }
    }

    #[must_use]
    pub fn union_point(self, p: Vec2) -> Aabb2 {
        Aabb2 {
            min: self.min.min(p),
            max: self.max.max(p),
        }
    }

    #[must_use]
    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    #[must_use]
    pub fn size(self) -> Vec2 {
        self.max - self.min
    }

    /// Kwadrat odległości od punktu do prostokąta; 0 wewnątrz.
    /// Kwadrat, a nie odległość — zapytania promieniowe porównują z `r*r`.
    #[must_use]
    pub fn distance_sq(self, p: Vec2) -> f32 {
        let d = (self.min - p).max(p - self.max).max(Vec2::ZERO);
        d.x * d.x + d.y * d.y
    }

    /// Test przecięcia z odcinkiem metodą płyt (slab). Bez dzielenia przez zero:
    /// dla odcinka równoległego do osi kierunek zeruje się i przedział wychodzi
    /// z porównania krańców.
    #[must_use]
    pub fn intersects_segment(self, a: Vec2, b: Vec2) -> bool {
        if !self.intersects(Aabb2::from_points(a, b)) {
            return false;
        }
        let d = b - a;
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for axis in 0..2 {
            let (da, pa) = (d[axis], a[axis]);
            let (lo, hi) = (self.min[axis], self.max[axis]);
            if da == 0.0 {
                if pa < lo || pa > hi {
                    return false;
                }
                continue;
            }
            let inv = 1.0 / da;
            let (mut n, mut f) = ((lo - pa) * inv, (hi - pa) * inv);
            if n > f {
                std::mem::swap(&mut n, &mut f);
            }
            t0 = t0.max(n);
            t1 = t1.min(f);
            if t0 > t1 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pusty_prostokat_znika_w_sumie() {
        let a = Aabb2::EMPTY.union_point(Vec2::new(3.0, 4.0));
        assert_eq!(a.min, Vec2::new(3.0, 4.0));
        assert_eq!(a.max, Vec2::new(3.0, 4.0));
        assert!(Aabb2::EMPTY.is_empty());
    }

    #[test]
    fn odleglosc_do_prostokata() {
        let r = Aabb2::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        assert_eq!(r.distance_sq(Vec2::new(5.0, 5.0)), 0.0);
        assert_eq!(r.distance_sq(Vec2::new(13.0, 14.0)), 9.0 + 16.0);
        assert_eq!(r.distance_sq(Vec2::new(-3.0, 5.0)), 9.0);
    }

    #[test]
    fn odcinek_rownolegly_do_osi_nie_dzieli_przez_zero() {
        let r = Aabb2::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        assert!(r.intersects_segment(Vec2::new(-5.0, 5.0), Vec2::new(15.0, 5.0)));
        assert!(!r.intersects_segment(Vec2::new(-5.0, 15.0), Vec2::new(15.0, 15.0)));
        // Odcinek kończy się przed prostokątem — AABB odcinka już to wyłapuje.
        assert!(!r.intersects_segment(Vec2::new(-5.0, 5.0), Vec2::new(-1.0, 5.0)));
        assert!(r.intersects_segment(Vec2::new(5.0, 5.0), Vec2::new(5.0, 50.0)));
    }
}
