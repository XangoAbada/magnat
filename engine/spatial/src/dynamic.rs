//! Indeks dynamiczny: przebudowa co tick sortowaniem zliczającym (M2 §5.1).
//!
//! Determinizm nie pochodzi z „posortujemy na końcu", tylko z układu przebiegu:
//! kolejność encji w komórce to zawsze kolejność ich indeksów wejściowych,
//! bo każdy wątek przepisuje encje w rosnącym indeksie do **własnego, rozłącznego**
//! zakresu komórek. Wynik jest bit-identyczny przy 1, 4 i 8 wątkach (test D3, 00 §3.3).

use crate::csr::{CsrGrid, SpatialStats};
use crate::geom::{Aabb2, Vec2};
use crate::spec::GridSpec;
use magnat_jobs::{for_each_chunk_mut, JobPool};

pub struct DynamicGrid<T: Copy> {
    front: CsrGrid<T>,
    /// Numer komórki per encja, w kolejności wejściowej. Bufor roboczy trzymany
    /// między przebudowami, żeby tick nie zaczynał się od alokacji.
    cells: Vec<u32>,
}

/// Fragment wyjścia przypisany jednemu wątkowi: rozłączny zakres komórek
/// i odpowiadający mu **ciągły** wycinek buforów CSR.
struct Part<'a, T> {
    lo: usize,
    hi: usize,
    base: u32,
    pts: &'a mut [Vec2],
    items: &'a mut [T],
}

impl<T: Copy> DynamicGrid<T> {
    #[must_use]
    pub fn new(spec: GridSpec) -> DynamicGrid<T> {
        DynamicGrid {
            front: CsrGrid::empty(spec),
            cells: Vec::new(),
        }
    }

    #[must_use]
    pub fn grid(&self) -> &CsrGrid<T> {
        &self.front
    }

    #[must_use]
    pub fn spec(&self) -> &GridSpec {
        self.front.spec()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.front.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.front.is_empty()
    }

    #[must_use]
    pub fn stats(&self) -> SpatialStats {
        self.front.stats()
    }

    pub fn for_each_in_radius(&self, c: Vec2, r: f32, f: impl FnMut(Vec2, T)) {
        self.front.for_each_in_radius(c, r, f);
    }

    pub fn query_radius(&self, c: Vec2, r: f32, out: &mut Vec<T>) {
        self.front.query_radius(c, r, out);
    }

    pub fn query_rect(&self, a: Aabb2, out: &mut Vec<T>) {
        self.front.query_rect(a, out);
    }

    pub fn k_nearest(&self, c: Vec2, k: usize, out: &mut Vec<(f32, T)>) {
        self.front.k_nearest(c, k, out);
    }
}

impl<T: Copy + Send + Sync> DynamicGrid<T> {
    /// Przebudowa w O(n + cells). Trzy przebiegi: komórka encji (równolegle),
    /// histogram z prefiksem (sekwencyjnie — 62 tys. komórek to ~30 µs), rozrzut
    /// do rozłącznych zakresów komórek (równolegle).
    pub fn rebuild(&mut self, pos: &[Vec2], ids: &[T], pool: &JobPool) {
        assert_eq!(pos.len(), ids.len(), "rebuild: pos i ids różnej długości");
        let spec = self.front.spec;
        let n = pos.len();
        let ncells = spec.cell_count();

        self.cells.clear();
        self.cells.resize(n, 0);
        for_each_chunk_mut(pool, &mut self.cells, |i, c| *c = spec.cell_of(pos[i]).0);

        let CsrGrid {
            starts, pts, items, ..
        } = &mut self.front;
        starts.clear();
        starts.resize(ncells + 1, 0);
        for &c in &self.cells {
            starts[c as usize + 1] += 1;
        }
        for i in 1..starts.len() {
            starts[i] += starts[i - 1];
        }

        pts.clear();
        items.clear();
        if n == 0 {
            return;
        }
        pts.resize(n, Vec2::ZERO);
        items.resize(n, ids[0]);

        // Granice zakresów komórek: mniej więcej równa liczba encji na wątek.
        let threads = pool.thread_count().max(1);
        let starts_ro: &[u32] = starts;
        let mut bounds: Vec<usize> = Vec::with_capacity(threads + 1);
        bounds.push(0);
        for t in 1..threads {
            let target = (n * t / threads) as u32;
            let c = starts_ro
                .partition_point(|&s| s < target)
                .min(ncells)
                .max(*bounds.last().unwrap_or(&0));
            bounds.push(c);
        }
        bounds.push(ncells);

        let mut rest_pts = &mut pts[..];
        let mut rest_items = &mut items[..];
        let mut parts: Vec<Part<T>> = Vec::with_capacity(threads);
        for w in bounds.windows(2) {
            let (lo, hi) = (w[0], w[1]);
            let cnt = (starts_ro[hi] - starts_ro[lo]) as usize;
            let (a, b) = rest_pts.split_at_mut(cnt);
            let (ai, bi) = rest_items.split_at_mut(cnt);
            rest_pts = b;
            rest_items = bi;
            parts.push(Part {
                lo,
                hi,
                base: starts_ro[lo],
                pts: a,
                items: ai,
            });
        }

        // ponytail: każdy wątek przegląda całą tablicę `cells` i bierze swoje —
        // O(n · wątki) tanich odczytów strumieniowych zamiast rozrzutu z atomikami
        // albo `unsafe`. Sufit: przy n · wątki rzędu 10⁷ ten przebieg zaczyna dominować;
        // wyjście to histogramy per porcja i rozrzut po (porcja, komórka).
        let cells: &[u32] = &self.cells;
        for_each_chunk_mut(pool, &mut parts, |_, part| {
            let mut cursor = vec![0u32; part.hi - part.lo];
            for i in 0..n {
                let c = cells[i] as usize;
                if c >= part.lo && c < part.hi {
                    let k = (starts_ro[c] - part.base + cursor[c - part.lo]) as usize;
                    part.pts[k] = pos[i];
                    part.items[k] = ids[i];
                    cursor[c - part.lo] += 1;
                }
            }
        });
    }
}
