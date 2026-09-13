//! Siatka 2D o stałym boku — nośnik każdego pola generatora (wysokość, spływ, klimat).
//!
//! Jeden typ zamiast pięciu `Vec` z ręcznym `y * dim + x`, bo pomyłka w kolejności indeksów
//! daje teren obrócony o 90°, a nie błąd kompilacji. Indeksowanie jest **row-major**
//! i to jest kontrakt: cała iteracja w generatorze idzie w tej kolejności (00 §3.2),
//! więc redukcje i hash są powtarzalne.

use serde::{Deserialize, Serialize};
use std::ops::{Index, IndexMut};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Grid2<T> {
    dim: usize,
    cells: Vec<T>,
}

impl<T: Clone> Grid2<T> {
    /// Siatka `dim × dim` wypełniona wartością.
    #[must_use]
    pub fn filled(dim: usize, value: T) -> Grid2<T> {
        Grid2 {
            dim,
            cells: vec![value; dim * dim],
        }
    }
}

impl<T> Grid2<T> {
    #[must_use]
    pub fn from_vec(dim: usize, cells: Vec<T>) -> Grid2<T> {
        assert_eq!(cells.len(), dim * dim, "Grid2: rozmiar nie zgadza się z bokiem");
        Grid2 { dim, cells }
    }

    #[inline]
    #[must_use]
    pub const fn dim(&self) -> usize {
        self.dim
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Indeks liniowy. Row-major: `y` wolniejsze, `x` szybsze.
    #[inline]
    #[must_use]
    pub const fn idx(&self, x: usize, y: usize) -> usize {
        y * self.dim + x
    }

    #[inline]
    #[must_use]
    pub const fn xy(&self, i: usize) -> (usize, usize) {
        (i % self.dim, i / self.dim)
    }

    #[inline]
    #[must_use]
    pub fn in_bounds(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.dim && (y as usize) < self.dim
    }

    #[inline]
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> &T {
        &self.cells[self.idx(x, y)]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: T) {
        let i = self.idx(x, y);
        self.cells[i] = v;
    }

    /// Wartość z zaciśnięciem do brzegu — brzeg mapy nie jest przypadkiem szczególnym
    /// w żadnym filtrze, więc nie ma go w żadnej pętli.
    #[inline]
    #[must_use]
    pub fn get_clamped(&self, x: i64, y: i64) -> &T {
        let cx = x.clamp(0, self.dim as i64 - 1) as usize;
        let cy = y.clamp(0, self.dim as i64 - 1) as usize;
        &self.cells[cy * self.dim + cx]
    }

    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.cells
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.cells
    }

    /// Równoległa mutacja po wierszach. Wiersz jest naturalną jednostką podziału siatki
    /// row-major: pasma są rozłączne, a wynik nie zależy od tego, który worker skończył
    /// pierwszy, bo żaden nie czyta cudzego wiersza (00 §3.3).
    pub fn par_rows_mut(&mut self, pool: &magnat_jobs::JobPool, f: impl Fn(usize, &mut [T]) + Sync)
    where
        T: Send,
    {
        let dim = self.dim;
        let mut rows: Vec<&mut [T]> = self.cells.chunks_mut(dim).collect();
        magnat_jobs::for_each_chunk_mut(pool, &mut rows, |y, row| f(y, row));
    }

    /// Mapowanie na siatkę innego typu, w kolejności indeksów.
    pub fn map<U>(&self, f: impl Fn(usize, &T) -> U) -> Grid2<U> {
        Grid2 {
            dim: self.dim,
            cells: self.cells.iter().enumerate().map(|(i, v)| f(i, v)).collect(),
        }
    }
}

impl<T> Index<usize> for Grid2<T> {
    type Output = T;
    #[inline]
    fn index(&self, i: usize) -> &T {
        &self.cells[i]
    }
}

impl<T> IndexMut<usize> for Grid2<T> {
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut T {
        &mut self.cells[i]
    }
}

/// Ośmiu sąsiadów w kolejności N, NE, E, SE, S, SW, W, NW.
/// **Ta kolejność jest kontraktem** — rozstrzyga remisy w D8 (M1 §5.7), więc jej zmiana
/// zmienia bieg każdej rzeki w każdym świecie wygenerowanym wcześniej.
pub const NEIGHBORS_8: [(i8, i8); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// Odległość do sąsiada `k` w metrach, dla siatki o boku komórki `cell_m`.
/// Przekątna to `sqrt(2)` — liczone przez `det_math::sqrt`, bo `sqrt` jest w IEEE-754
/// dokładnie zaokrąglane, więc wynik jest identyczny na każdej platformie (00 §K-6).
#[must_use]
pub fn neighbor_dist_m(k: usize, cell_m: f64) -> f64 {
    let (dx, dy) = NEIGHBORS_8[k];
    if dx != 0 && dy != 0 {
        cell_m * magnat_core::det_math::sqrt(2.0)
    } else {
        cell_m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indeksowanie_jest_row_major() {
        let mut g = Grid2::filled(4, 0u8);
        g.set(3, 0, 7);
        assert_eq!(g[3], 7, "x jest szybszym indeksem");
        assert_eq!(g.xy(3), (3, 0));
        assert_eq!(g.idx(2, 1), 6);
        assert_eq!(g.xy(6), (2, 1));
    }

    #[test]
    fn zacisniecie_do_brzegu() {
        let mut g = Grid2::filled(3, 0u8);
        g.set(0, 0, 9);
        assert_eq!(*g.get_clamped(-5, -5), 9);
        assert_eq!(*g.get_clamped(100, 0), *g.get(2, 0));
        assert!(!g.in_bounds(-1, 0));
        assert!(g.in_bounds(2, 2));
    }

    #[test]
    fn kolejnosc_sasiadow_jest_wieczna() {
        // Ten test istnieje po to, żeby zmiana kolejności wymagała świadomej decyzji:
        // remisy w D8 rozstrzyga najniższy indeks sąsiada (M1 §5.7).
        assert_eq!(NEIGHBORS_8[0], (0, -1), "N");
        assert_eq!(NEIGHBORS_8[2], (1, 0), "E");
        assert_eq!(NEIGHBORS_8[4], (0, 1), "S");
        assert_eq!(NEIGHBORS_8[6], (-1, 0), "W");
    }
}
