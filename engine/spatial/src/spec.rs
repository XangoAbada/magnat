//! Układ siatki wspólny dla wszystkich indeksów w świecie (M2 §5.1).

use crate::geom::{Aabb2, Vec2};

/// Numer komórki w porządku **row-major**: `y * cols + x`.
///
/// Row-major, a nie Morton, i to jest decyzja wydajnościowa, nie estetyczna:
/// przy row-major komórki jednego wiersza są w CSR ciągłe, więc zapytanie
/// promieniowe skanuje 9 ciągłych odcinków pamięci zamiast 81 rozrzuconych.
/// [`morton2`] zostaje do porządkowania encji i sortowania zapytań wsadowych.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CellId(pub u32);

/// Definicja układu siatki. Wspólna dla wszystkich indeksów w świecie.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridSpec {
    pub origin: Vec2,
    pub cell_m: u16,
    pub cols: u16,
    pub rows: u16,
}

impl GridSpec {
    /// Panika przy `cell_m == 0` albo pustej siatce — indeks bez komórek to błąd
    /// wywołującego, a nie stan, który warto obsługiwać w każdym zapytaniu.
    #[must_use]
    pub fn new(origin: Vec2, cell_m: u16, cols: u16, rows: u16) -> GridSpec {
        assert!(cell_m > 0 && cols > 0 && rows > 0, "GridSpec: pusta siatka");
        GridSpec {
            origin,
            cell_m,
            cols,
            rows,
        }
    }

    /// Siatka pokrywająca zadany obszar. Liczba komórek zaokrąglana w górę.
    #[must_use]
    pub fn covering(area: Aabb2, cell_m: u16) -> GridSpec {
        let size = area.size();
        let n = |v: f32| {
            ((v / f32::from(cell_m)).ceil().max(1.0) as u32).min(u32::from(u16::MAX)) as u16
        };
        GridSpec::new(area.min, cell_m, n(size.x), n(size.y))
    }

    #[must_use]
    pub fn cell_count(&self) -> usize {
        usize::from(self.cols) * usize::from(self.rows)
    }

    #[must_use]
    pub fn cell_size(&self) -> f32 {
        f32::from(self.cell_m)
    }

    /// Współrzędne komórki, **przycięte do siatki**. Punkt spoza mapy trafia do
    /// komórki brzegowej — zapytania i tak filtrują po dokładnej odległości,
    /// więc przycięcie nie może dać fałszywego trafienia, a oszczędza `Option`
    /// w każdej pętli budującej indeks.
    #[must_use]
    pub fn cell_xy(&self, p: Vec2) -> (u16, u16) {
        let d = (p - self.origin) / self.cell_size();
        // NaN jawnie: bez tego punkt o nieokreślonej współrzędnej wpadłby
        // w `as u16` z wynikiem zależnym od platformy.
        let clamp = |v: f32, hi: u16| -> u16 {
            if v.is_nan() || v < 0.0 {
                0
            } else if v >= f32::from(hi) {
                hi - 1
            } else {
                v as u16
            }
        };
        (clamp(d.x, self.cols), clamp(d.y, self.rows))
    }

    #[must_use]
    pub fn cell_of(&self, p: Vec2) -> CellId {
        let (x, y) = self.cell_xy(p);
        CellId(u32::from(y) * u32::from(self.cols) + u32::from(x))
    }

    #[must_use]
    pub fn cell_of_id(&self, c: CellId) -> (u16, u16) {
        let cols = u32::from(self.cols);
        ((c.0 % cols) as u16, (c.0 / cols) as u16)
    }

    #[must_use]
    pub fn cell_center(&self, c: CellId) -> Vec2 {
        let (x, y) = self.cell_of_id(c);
        self.origin
            + Vec2::new(
                (f32::from(x) + 0.5) * self.cell_size(),
                (f32::from(y) + 0.5) * self.cell_size(),
            )
    }

    #[must_use]
    pub fn cell_bounds(&self, c: CellId) -> Aabb2 {
        let (x, y) = self.cell_of_id(c);
        let min = self.origin + Vec2::new(f32::from(x), f32::from(y)) * self.cell_size();
        Aabb2::new(min, min + Vec2::splat(self.cell_size()))
    }

    /// Zakres komórek przecinających prostokąt — iterator, bez alokacji.
    #[must_use]
    pub fn cells_in_aabb(&self, a: Aabb2) -> CellRange {
        let (x0, y0) = self.cell_xy(a.min);
        let (x1, y1) = self.cell_xy(a.max);
        CellRange {
            x0,
            x1,
            y0,
            y1,
            cols: self.cols,
            x: x0,
            y: y0,
        }
    }
}

/// Prostokątny zakres komórek. Iteruje wierszami, a `row_spans` zwraca **ciągłe**
/// przedziały numerów komórek w wierszu — to one są sekwencyjnym odczytem w CSR.
#[derive(Clone, Copy, Debug)]
pub struct CellRange {
    x0: u16,
    x1: u16,
    y0: u16,
    y1: u16,
    cols: u16,
    x: u16,
    y: u16,
}

impl CellRange {
    #[must_use]
    pub fn len(&self) -> usize {
        usize::from(self.x1 - self.x0 + 1) * usize::from(self.y1 - self.y0 + 1)
    }

    /// Zakres powstaje z przyciętych współrzędnych, więc zawsze ma co najmniej
    /// jedną komórkę. Metoda istnieje, bo clippy wymaga jej obok `len`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// `(pierwsza_komórka, liczba_komórek)` dla każdego wiersza zakresu.
    pub fn row_spans(&self) -> RowSpans {
        RowSpans {
            x0: self.x0,
            width: u32::from(self.x1 - self.x0) + 1,
            y: self.y0,
            y1: self.y1,
            cols: self.cols,
        }
    }
}

impl Iterator for CellRange {
    type Item = CellId;

    fn next(&mut self) -> Option<CellId> {
        if self.y > self.y1 {
            return None;
        }
        let id = CellId(u32::from(self.y) * u32::from(self.cols) + u32::from(self.x));
        if self.x == self.x1 {
            self.x = self.x0;
            self.y += 1;
        } else {
            self.x += 1;
        }
        Some(id)
    }
}

/// Iterator ciągłych odcinków wierszy — osobny typ zamiast `impl Iterator`,
/// żeby dało się go trzymać w polu i nie ciągnąć czasu życia siatki.
#[derive(Clone, Copy, Debug)]
pub struct RowSpans {
    x0: u16,
    width: u32,
    y: u16,
    y1: u16,
    cols: u16,
}

impl Iterator for RowSpans {
    type Item = (u32, u32);

    fn next(&mut self) -> Option<(u32, u32)> {
        if self.y > self.y1 {
            return None;
        }
        let start = u32::from(self.y) * u32::from(self.cols) + u32::from(self.x0);
        self.y += 1;
        Some((start, self.width))
    }
}

/// Przeplot bitów — klucz porządku Mortona (Z-order). Dla kolejności encji w ECS
/// i sortowania zapytań wsadowych, nie dla numeracji komórek (patrz [`CellId`]).
#[must_use]
pub const fn morton2(x: u16, y: u16) -> u32 {
    part1by1(x) | (part1by1(y) << 1)
}

const fn part1by1(v: u16) -> u32 {
    let mut x = v as u32;
    x = (x | (x << 8)) & 0x00FF_00FF;
    x = (x | (x << 4)) & 0x0F0F_0F0F;
    x = (x | (x << 2)) & 0x3333_3333;
    x = (x | (x << 1)) & 0x5555_5555;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> GridSpec {
        GridSpec::new(Vec2::new(-100.0, -100.0), 64, 10, 8)
    }

    #[test]
    fn morton_przeplata_bity() {
        assert_eq!(morton2(0, 0), 0);
        assert_eq!(morton2(1, 0), 1);
        assert_eq!(morton2(0, 1), 2);
        assert_eq!(morton2(3, 3), 0b1111);
        assert_eq!(morton2(0xFFFF, 0), 0x5555_5555);
        assert_eq!(morton2(0, 0xFFFF), 0xAAAA_AAAA);
        assert_eq!(morton2(2, 1), 0b0110);
    }

    #[test]
    fn punkt_spoza_mapy_trafia_do_komorki_brzegowej() {
        let s = spec();
        assert_eq!(s.cell_of(Vec2::new(-1e9, -1e9)), CellId(0));
        assert_eq!(s.cell_of(Vec2::new(1e9, 1e9)), CellId(10 * 8 - 1));
        assert_eq!(s.cell_of(Vec2::new(f32::NAN, 0.0)).0 % 10, 0);
    }

    #[test]
    fn komorka_i_jej_srodek_wracaja_do_siebie() {
        let s = spec();
        for c in 0..s.cell_count() as u32 {
            assert_eq!(s.cell_of(s.cell_center(CellId(c))), CellId(c));
        }
    }

    #[test]
    fn zakres_iteruje_tyle_samo_co_deklaruje() {
        let s = spec();
        let a = Aabb2::new(Vec2::new(-50.0, -60.0), Vec2::new(120.0, 40.0));
        let r = s.cells_in_aabb(a);
        let zebrane: Vec<CellId> = r.collect();
        assert_eq!(zebrane.len(), r.len());
        // Suma długości wierszy = liczba komórek zakresu.
        let z_wierszy: u32 = r.row_spans().map(|(_, n)| n).sum();
        assert_eq!(z_wierszy as usize, r.len());
        for c in zebrane {
            assert!(
                s.cell_bounds(c).intersects(a),
                "komórka {c:?} poza zakresem"
            );
        }
    }
}
