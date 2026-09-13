//! Całkowitoliczbowa geometria siatki — wspólna dla `engine/voxel`, `sim/world`
//! i `engine/render` (00 §K-8: typ używany przez więcej niż jedną fazę mieszka w `core`).
//!
//! To **nie jest** biblioteka wektorowa. Nie ma tu iloczynu skalarnego, normalizacji ani
//! niczego zmiennoprzecinkowego — od tego jest `glam` po stronie renderu. Tutaj są
//! współrzędne komórek i prostokąty, czyli rzeczy, które muszą być dokładne,
//! bo wyznaczają, który voxel należy do którego chunka.
//!
//! Jednostki nadaje **konsument**: `sim/world` liczy w metrach, `engine/voxel` w voxelach.
//! `core` nie zna skali i nie próbuje jej zgadywać.

use serde::{Deserialize, Serialize};

macro_rules! ivec {
    ($name:ident, $($f:ident),+) => {
        #[derive(
            Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default,
            Serialize, Deserialize,
        )]
        pub struct $name { $(pub $f: i32),+ }

        impl $name {
            pub const ZERO: $name = $name { $($f: 0),+ };

            #[inline]
            #[must_use]
            pub const fn new($($f: i32),+) -> $name {
                $name { $($f),+ }
            }

            #[inline]
            #[must_use]
            pub const fn splat(v: i32) -> $name {
                $name { $($f: v),+ }
            }
        }

        impl std::ops::Add for $name {
            type Output = $name;
            #[inline]
            fn add(self, o: $name) -> $name {
                $name { $($f: self.$f + o.$f),+ }
            }
        }

        impl std::ops::Sub for $name {
            type Output = $name;
            #[inline]
            fn sub(self, o: $name) -> $name {
                $name { $($f: self.$f - o.$f),+ }
            }
        }
    };
}

ivec!(IVec2, x, y);
ivec!(IVec3, x, y, z);

impl IVec2 {
    /// Kwadrat odległości. Kwadrat, a nie odległość: porównania i sortowania po dystansie
    /// nie potrzebują pierwiastka, a `i64` nie przepełni się na mapie 16 km.
    #[inline]
    #[must_use]
    pub const fn distance_sq(self, o: IVec2) -> i64 {
        let dx = (self.x - o.x) as i64;
        let dy = (self.y - o.y) as i64;
        dx * dx + dy * dy
    }
}

impl IVec3 {
    #[inline]
    #[must_use]
    pub const fn xy(self) -> IVec2 {
        IVec2 { x: self.x, y: self.y }
    }

    #[inline]
    #[must_use]
    pub const fn distance_sq(self, o: IVec3) -> i64 {
        let dx = (self.x - o.x) as i64;
        let dy = (self.y - o.y) as i64;
        let dz = (self.z - o.z) as i64;
        dx * dx + dy * dy + dz * dz
    }
}

/// Prostokąt 2D, `min` włącznie, `max` **wyłącznie**.
///
/// Półotwarty przedział, bo każdy kod, który liczy rozmiar jako `max - min`, jest wtedy
/// poprawny bez `+1` — a `+1` zgubione w jednym miejscu to klasyczny błąd o jeden voxel
/// na styku chunków.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct IRect {
    pub min: IVec2,
    pub max: IVec2,
}

impl IRect {
    #[must_use]
    pub const fn new(min: IVec2, max: IVec2) -> IRect {
        IRect { min, max }
    }

    #[must_use]
    pub const fn from_size(min: IVec2, w: i32, h: i32) -> IRect {
        IRect {
            min,
            max: IVec2::new(min.x + w, min.y + h),
        }
    }

    #[inline]
    #[must_use]
    pub const fn width(self) -> i32 {
        self.max.x - self.min.x
    }

    #[inline]
    #[must_use]
    pub const fn height(self) -> i32 {
        self.max.y - self.min.y
    }

    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }

    #[inline]
    #[must_use]
    pub const fn contains(self, p: IVec2) -> bool {
        p.x >= self.min.x && p.x < self.max.x && p.y >= self.min.y && p.y < self.max.y
    }

    #[must_use]
    pub fn intersects(self, o: IRect) -> bool {
        self.min.x < o.max.x && o.min.x < self.max.x && self.min.y < o.max.y && o.min.y < self.max.y
    }
}

/// Prostopadłościan 3D, `min` włącznie, `max` wyłącznie. Zasięg komendy edycji voxeli
/// (M1 §5.4a) — bucketing po chunkach robi się z tego pola bez interpretacji operacji.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct IAabb3 {
    pub min: IVec3,
    pub max: IVec3,
}

impl IAabb3 {
    #[must_use]
    pub const fn new(min: IVec3, max: IVec3) -> IAabb3 {
        IAabb3 { min, max }
    }

    #[must_use]
    pub fn from_points(a: IVec3, b: IVec3) -> IAabb3 {
        IAabb3 {
            min: IVec3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            max: IVec3::new(a.x.max(b.x) + 1, a.y.max(b.y) + 1, a.z.max(b.z) + 1),
        }
    }

    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y || self.max.z <= self.min.z
    }

    #[inline]
    #[must_use]
    pub const fn contains(self, p: IVec3) -> bool {
        p.x >= self.min.x
            && p.x < self.max.x
            && p.y >= self.min.y
            && p.y < self.max.y
            && p.z >= self.min.z
            && p.z < self.max.z
    }

    #[must_use]
    pub fn intersects(self, o: IAabb3) -> bool {
        self.min.x < o.max.x
            && o.min.x < self.max.x
            && self.min.y < o.max.y
            && o.min.y < self.max.y
            && self.min.z < o.max.z
            && o.min.z < self.max.z
    }

    #[must_use]
    pub fn union(self, o: IAabb3) -> IAabb3 {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        IAabb3 {
            min: IVec3::new(
                self.min.x.min(o.min.x),
                self.min.y.min(o.min.y),
                self.min.z.min(o.min.z),
            ),
            max: IVec3::new(
                self.max.x.max(o.max.x),
                self.max.y.max(o.max.y),
                self.max.z.max(o.max.z),
            ),
        }
    }

    /// Liczba komórek. `i64`, bo prostopadłościan obejmujący całą mapę przepełniłby `i32`.
    #[must_use]
    pub fn volume(self) -> i64 {
        if self.is_empty() {
            return 0;
        }
        i64::from(self.max.x - self.min.x)
            * i64::from(self.max.y - self.min.y)
            * i64::from(self.max.z - self.min.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polotwarty_przedzial_nie_wymaga_plus_jeden() {
        let r = IRect::from_size(IVec2::new(10, 20), 4, 3);
        assert_eq!(r.width(), 4);
        assert_eq!(r.height(), 3);
        assert!(r.contains(IVec2::new(13, 22)));
        assert!(!r.contains(IVec2::new(14, 22)), "max jest wyłączne");
        // Sąsiadujące prostokąty nie nachodzą na siebie — to jest cała korzyść z półotwartości.
        let s = IRect::from_size(IVec2::new(14, 20), 4, 3);
        assert!(!r.intersects(s));
    }

    #[test]
    fn aabb_liczy_objetosc_bez_przepelnienia() {
        let big = IAabb3::new(IVec3::ZERO, IVec3::new(16_384, 16_384, 512));
        assert_eq!(big.volume(), 16_384i64 * 16_384 * 512);
        assert!(IAabb3::default().is_empty());
        assert_eq!(IAabb3::default().volume(), 0);
    }

    #[test]
    fn suma_pustego_z_niepustym_daje_niepusty() {
        let a = IAabb3::default();
        let b = IAabb3::new(IVec3::new(1, 1, 1), IVec3::new(2, 2, 2));
        assert_eq!(a.union(b), b);
        assert_eq!(b.union(a), b);
    }
}
