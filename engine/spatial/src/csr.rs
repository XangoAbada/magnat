//! Indeks statyczny CSR i jego pochodne (M2 §5.1).
//!
//! Układ SoA: `pts` i `items` są równoległe i posortowane po numerze komórki,
//! a `starts` daje przedział każdej komórki. Pozycje leżą **w indeksie**, nie
//! w cudzej tablicy — zapytanie promieniowe filtruje po dokładnej odległości
//! i nie chce za to płacić skokiem do pamięci encji.

use crate::geom::{Aabb2, Vec2};
use crate::spec::{morton2, CellId, GridSpec};
use magnat_jobs::{map_reduce_indexed, JobPool};

/// Statystyka jakości indeksu — wchodzi do `GenerationReport` (M2 §6).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct SpatialStats {
    pub cells: u32,
    pub occupied_cells: u32,
    pub items: u32,
    pub max_per_cell: u32,
}

impl SpatialStats {
    #[must_use]
    pub fn avg_per_occupied(&self) -> f32 {
        if self.occupied_cells == 0 {
            0.0
        } else {
            self.items as f32 / self.occupied_cells as f32
        }
    }
}

/// Indeks statyczny: budowany raz, tylko do odczytu. CSR (compressed sparse row).
#[derive(Clone, Debug)]
pub struct CsrGrid<T: Copy> {
    pub(crate) spec: GridSpec,
    /// Długość `cell_count + 1`; `starts[c]..starts[c+1]` to zakres komórki `c`.
    pub(crate) starts: Vec<u32>,
    pub(crate) pts: Vec<Vec2>,
    pub(crate) items: Vec<T>,
}

impl<T: Copy> CsrGrid<T> {
    /// Pusty indeks o zadanym układzie — punkt wyjścia dla [`crate::DynamicGrid`].
    #[must_use]
    pub fn empty(spec: GridSpec) -> CsrGrid<T> {
        CsrGrid {
            starts: vec![0; spec.cell_count() + 1],
            spec,
            pts: Vec::new(),
            items: Vec::new(),
        }
    }

    /// Budowa w O(n) sortowaniem zliczającym. Kolejność encji w komórce = kolejność
    /// z iteratora, więc wynik zależy wyłącznie od wejścia.
    pub fn build(spec: GridSpec, it: impl Iterator<Item = (Vec2, T)>) -> CsrGrid<T> {
        let mut g = CsrGrid::empty(spec);
        let hint = it.size_hint().0;
        let mut src: Vec<(u32, Vec2, T)> = Vec::with_capacity(hint);
        for (p, t) in it {
            src.push((spec.cell_of(p).0, p, t));
        }
        let n = src.len();
        if n == 0 {
            return g;
        }

        for (c, _, _) in &src {
            g.starts[*c as usize + 1] += 1;
        }
        for i in 1..g.starts.len() {
            g.starts[i] += g.starts[i - 1];
        }

        // Kursory to kopia `starts` — scatter zjada je w miejscu. Wypełniacz bierzemy
        // z danych (`src[0].2`), bo `T` nie musi mieć `Default`.
        let mut cursor = g.starts.clone();
        g.pts = vec![Vec2::ZERO; n];
        g.items = vec![src[0].2; n];
        for (c, p, t) in src {
            let slot = &mut cursor[c as usize];
            let i = *slot as usize;
            *slot += 1;
            g.pts[i] = p;
            g.items[i] = t;
        }
        g
    }

    #[must_use]
    pub fn spec(&self) -> &GridSpec {
        &self.spec
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Zawartość jednej komórki — pary `(pozycja, element)`.
    #[must_use]
    pub fn cell(&self, c: CellId) -> (&[Vec2], &[T]) {
        let a = self.starts[c.0 as usize] as usize;
        let b = self.starts[c.0 as usize + 1] as usize;
        (&self.pts[a..b], &self.items[a..b])
    }

    #[must_use]
    pub fn stats(&self) -> SpatialStats {
        let mut s = SpatialStats {
            cells: self.spec.cell_count() as u32,
            items: self.items.len() as u32,
            ..SpatialStats::default()
        };
        for w in self.starts.windows(2) {
            let n = w[1] - w[0];
            if n > 0 {
                s.occupied_cells += 1;
                s.max_per_cell = s.max_per_cell.max(n);
            }
        }
        s
    }

    /// Zapytanie promieniowe. Skanuje **wiersz na raz**: komórki jednego wiersza leżą
    /// w CSR obok siebie, więc zamiast 81 rozrzuconych komórek jest 9 ciągłych odczytów.
    ///
    /// Szerokość wiersza liczona jest z cięciwy koła, nie z kwadratu opisanego —
    /// kwadrat daje o ~40 % kandydatów więcej, a to `m`, nie `k`, jest kosztem
    /// dominującym przy gęstości miejskiej (patrz benchmark `csr/pojedyncze zapytanie`).
    pub fn for_each_in_radius(&self, c: Vec2, r: f32, mut f: impl FnMut(Vec2, T)) {
        let r2 = r * r;
        let spec = &self.spec;
        let cell = spec.cell_size();
        let (x_lo, y_lo) = spec.cell_xy(Vec2::new(c.x - r, c.y - r));
        let (x_hi, y_hi) = spec.cell_xy(Vec2::new(c.x + r, c.y + r));
        for y in y_lo..=y_hi {
            // Odległość pionowa od środka zapytania do pasa tego wiersza.
            let y0 = spec.origin.y + f32::from(y) * cell;
            let dy = f32::max(f32::max(y0 - c.y, c.y - (y0 + cell)), 0.0);
            if dy > r {
                continue;
            }
            let hw = (r2 - dy * dy).sqrt();
            let xa = spec.cell_xy(Vec2::new(c.x - hw, c.y)).0.max(x_lo);
            let xb = spec.cell_xy(Vec2::new(c.x + hw, c.y)).0.min(x_hi);
            if xa > xb {
                continue;
            }
            let base = u32::from(y) * u32::from(spec.cols);
            let a = self.starts[(base + u32::from(xa)) as usize] as usize;
            let b = self.starts[(base + u32::from(xb) + 1) as usize] as usize;
            for i in a..b {
                let d = self.pts[i] - c;
                if d.x * d.x + d.y * d.y <= r2 {
                    f(self.pts[i], self.items[i]);
                }
            }
        }
    }

    pub fn query_radius(&self, c: Vec2, r: f32, out: &mut Vec<T>) {
        out.clear();
        self.for_each_in_radius(c, r, |_, t| out.push(t));
    }

    pub fn query_rect(&self, a: Aabb2, out: &mut Vec<T>) {
        out.clear();
        for (start, n) in self.spec.cells_in_aabb(a).row_spans() {
            let lo = self.starts[start as usize] as usize;
            let hi = self.starts[(start + n) as usize] as usize;
            for i in lo..hi {
                if a.contains(self.pts[i]) {
                    out.push(self.items[i]);
                }
            }
        }
    }

    /// `k` najbliższych przez ekspansję pierścieniową. Po zbadaniu pierścienia `R`
    /// wszystko, co zostało, leży nie bliżej niż `R * cell_m` — to jest warunek stopu
    /// i jedyny powód, dla którego to nie jest przegląd całej siatki.
    pub fn k_nearest(&self, c: Vec2, k: usize, out: &mut Vec<(f32, T)>) {
        out.clear();
        if k == 0 || self.items.is_empty() {
            return;
        }
        let (cx, cy) = self.spec.cell_xy(c);
        let max_ring = u32::max(
            u32::max(u32::from(cx), u32::from(self.spec.cols - 1 - cx)),
            u32::max(u32::from(cy), u32::from(self.spec.rows - 1 - cy)),
        );
        let cell = self.spec.cell_size();

        for ring in 0..=max_ring {
            // Przed skanem pierścienia `R` zbadane są pierścienie 0..R-1, więc wszystko,
            // co zostało, leży nie bliżej niż `(R-1) * cell_m`. To jest warunek stopu.
            if out.len() == k && out[k - 1].0 <= (ring.saturating_sub(1) as f32) * cell {
                break;
            }
            self.for_each_in_ring(cx, cy, ring, |p, t| {
                let d = (p - c).length();
                if out.len() == k {
                    if d >= out[k - 1].0 {
                        return;
                    }
                    out.pop();
                }
                let at = out.partition_point(|e| e.0 <= d);
                out.insert(at, (d, t));
            });
        }
    }

    /// Komórki o odległości Czebyszewa dokładnie `ring` od `(cx, cy)`, przycięte
    /// do siatki. Kolejność: wiersze rosnąco, w wierszu kolumny rosnąco.
    fn for_each_in_ring(&self, cx: u16, cy: u16, ring: u32, mut f: impl FnMut(Vec2, T)) {
        let (cx, cy) = (u32::from(cx), u32::from(cy));
        let (cols, rows) = (u32::from(self.spec.cols), u32::from(self.spec.rows));
        let y0 = cy.saturating_sub(ring);
        let y1 = (cy + ring).min(rows - 1);
        let x0 = cx.saturating_sub(ring);
        let x1 = (cx + ring).min(cols - 1);
        for y in y0..=y1 {
            let brzeg_wiersza = ring == 0 || y + ring == cy || y == cy + ring;
            let mut scan = |xa: u32, xb: u32| {
                let base = y * cols;
                let a = self.starts[(base + xa) as usize] as usize;
                let b = self.starts[(base + xb + 1) as usize] as usize;
                for i in a..b {
                    f(self.pts[i], self.items[i]);
                }
            };
            if brzeg_wiersza {
                scan(x0, x1);
            } else {
                // Wiersz środkowy: tylko dwie skrajne kolumny należą do pierścienia.
                if cx >= ring {
                    scan(x0, x0);
                }
                if cx + ring < cols {
                    scan(x1, x1);
                }
            }
        }
    }
}

impl<T: Copy + Send + Sync> CsrGrid<T> {
    /// Zapytania wsadowe (M2 §5.1): sortowanie po `morton2` komórki środka,
    /// porcje po 256, składanie wyniku **po indeksie porcji**, nie po kolejności
    /// zakończenia. Dostarczone teraz na potrzeby M3–M5 (ryzyko R7 fazy).
    pub fn query_radius_batch(
        &self,
        centers: &[Vec2],
        r: f32,
        pool: &JobPool,
        out: &mut BatchResult<T>,
    ) {
        const CHUNK: usize = 256;
        out.starts.clear();
        out.items.clear();
        out.starts.resize(centers.len() + 1, 0);
        if centers.is_empty() || self.items.is_empty() {
            return;
        }

        // Porządek Mortona: sąsiednie zapytania trafiają w te same komórki CSR.
        // Indeks w kluczu, bo `sort_unstable` nie jest stabilne.
        let mut order: Vec<u32> = (0..centers.len() as u32).collect();
        order.sort_unstable_by_key(|&i| {
            let (x, y) = self.spec.cell_xy(centers[i as usize]);
            (morton2(x, y), i)
        });

        let chunks: Vec<&[u32]> = order.chunks(CHUNK).collect();
        let parts: Vec<BatchPart<T>> = map_reduce_indexed(
            pool,
            &chunks,
            |_, chunk| {
                let mut part = BatchPart {
                    lens: Vec::with_capacity(chunk.len()),
                    flat: Vec::new(),
                };
                for &i in *chunk {
                    let przed = part.flat.len();
                    self.for_each_in_radius(centers[i as usize], r, |_, t| part.flat.push(t));
                    part.lens.push((part.flat.len() - przed) as u32);
                }
                part
            },
            |mut acc, p| {
                acc.push(p);
                acc
            },
            Vec::new(),
        );

        for (ci, part) in parts.iter().enumerate() {
            for (j, &len) in part.lens.iter().enumerate() {
                out.starts[chunks[ci][j] as usize + 1] = len;
            }
        }
        for i in 1..out.starts.len() {
            out.starts[i] += out.starts[i - 1];
        }
        let total = *out.starts.last().unwrap_or(&0) as usize;
        out.items.resize(total, self.items[0]);
        for (ci, part) in parts.iter().enumerate() {
            let mut off = 0usize;
            for (j, &len) in part.lens.iter().enumerate() {
                let dst = out.starts[chunks[ci][j] as usize] as usize;
                out.items[dst..dst + len as usize]
                    .copy_from_slice(&part.flat[off..off + len as usize]);
                off += len as usize;
            }
        }
    }
}

struct BatchPart<T> {
    lens: Vec<u32>,
    flat: Vec<T>,
}

/// Wynik zapytania wsadowego w układzie CSR: `get(i)` zwraca odpowiedź na `centers[i]`
/// w kolejności wejściowej, niezależnie od tego, jak zapytania były posortowane.
#[derive(Clone, Debug)]
pub struct BatchResult<T> {
    starts: Vec<u32>,
    items: Vec<T>,
}

impl<T> Default for BatchResult<T> {
    fn default() -> Self {
        BatchResult {
            starts: Vec::new(),
            items: Vec::new(),
        }
    }
}

impl<T> BatchResult<T> {
    #[must_use]
    pub fn new() -> BatchResult<T> {
        BatchResult::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn total(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn get(&self, i: usize) -> &[T] {
        let a = self.starts[i] as usize;
        let b = self.starts[i + 1] as usize;
        &self.items[a..b]
    }
}

/// PRD §17.5: jeden `CsrGrid` per klucz kategorii (kategoria oferty, `SiteArchetypeId`).
/// Kategorii jest dużo, encji w każdej mało — wspólna siatka marnowałaby na nie pamięć,
/// a wspólny indeks kazałby filtrować kandydatów po kategorii przy każdym zapytaniu.
#[derive(Clone, Debug)]
pub struct CategoryGrid<K: Ord + Copy, T: Copy> {
    keys: Vec<K>,
    grids: Vec<CsrGrid<T>>,
}

impl<K: Ord + Copy, T: Copy> CategoryGrid<K, T> {
    pub fn build(spec: GridSpec, it: impl Iterator<Item = (K, Vec2, T)>) -> CategoryGrid<K, T> {
        let mut src: Vec<(K, Vec2, T)> = it.collect();
        src.sort_by_key(|(k, _, _)| *k); // stabilne: kolejność w kategorii = wejściowa
        let mut keys = Vec::new();
        let mut grids = Vec::new();
        let mut i = 0;
        while i < src.len() {
            let k = src[i].0;
            let j = src[i..].partition_point(|(kk, _, _)| *kk <= k) + i;
            keys.push(k);
            grids.push(CsrGrid::build(
                spec,
                src[i..j].iter().map(|(_, p, t)| (*p, *t)),
            ));
            i = j;
        }
        CategoryGrid { keys, grids }
    }

    #[must_use]
    pub fn keys(&self) -> &[K] {
        &self.keys
    }

    #[must_use]
    pub fn grid(&self, k: K) -> Option<&CsrGrid<T>> {
        self.keys.binary_search(&k).ok().map(|i| &self.grids[i])
    }

    pub fn for_each_in_radius(&self, k: K, c: Vec2, r: f32, f: impl FnMut(Vec2, T)) {
        if let Some(g) = self.grid(k) {
            g.for_each_in_radius(c, r, f);
        }
    }
}
