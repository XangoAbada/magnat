//! Pole skalarne na `GridSpec` — nośnik map wpływu generatora i nakładek UI (M2 §5.1).
//!
//! Dane trzymane jako `u16` znormalizowane 0..=65535: pole 1 mln komórek to 2 MB,
//! a nie 4, i wchodzi do tekstury R16 nakładki bez konwersji. Wartość fizyczną
//! (metry, minuty) odtwarza się mnożeniem przez [`ScalarField::full_scale`].

use crate::geom::{Aabb2, Vec2};
use crate::spec::{CellId, GridSpec};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Wartość oznaczająca „nieosiągalne" w polu odległości. Dla nakładki to po prostu
/// maksimum skali — najdalej, jak się da — więc nie wymaga osobnej gałęzi w shaderze.
pub const UNREACHABLE: u16 = u16::MAX;

#[derive(Clone, Debug)]
pub struct ScalarField {
    spec: GridSpec,
    data: Vec<u16>,
    full_scale: f32,
}

impl ScalarField {
    #[must_use]
    pub fn new(spec: GridSpec) -> ScalarField {
        ScalarField {
            data: vec![0; spec.cell_count()],
            spec,
            full_scale: 1.0,
        }
    }

    /// Panika przy niezgodnej długości danych — pole o innym rozmiarze niż siatka
    /// to błąd wywołującego, nie stan do obsłużenia w `sample`.
    #[must_use]
    pub fn from_data(spec: GridSpec, data: Vec<u16>, full_scale: f32) -> ScalarField {
        assert_eq!(
            data.len(),
            spec.cell_count(),
            "ScalarField: zła długość danych"
        );
        ScalarField {
            spec,
            data,
            full_scale,
        }
    }

    #[must_use]
    pub fn spec(&self) -> &GridSpec {
        &self.spec
    }

    #[must_use]
    pub fn data(&self) -> &[u16] {
        &self.data
    }

    /// Wartość fizyczna odpowiadająca 65535 (metry dla pola odległości, 1.0 dla pola
    /// znormalizowanego). Bez tego odległość po drogach traciłaby skalę przy zapisie.
    #[must_use]
    pub fn full_scale(&self) -> f32 {
        self.full_scale
    }

    #[must_use]
    pub fn get(&self, c: CellId) -> u16 {
        self.data[c.0 as usize]
    }

    pub fn set(&mut self, c: CellId, v: u16) {
        self.data[c.0 as usize] = v;
    }

    /// Próbkowanie dwuliniowe po środkach komórek, wynik 0..=1. O(1).
    #[must_use]
    pub fn sample(&self, p: Vec2) -> f32 {
        let cell = self.spec.cell_size();
        let (cols, rows) = (self.spec.cols as i32, self.spec.rows as i32);
        let u = (p.x - self.spec.origin.x) / cell - 0.5;
        let v = (p.y - self.spec.origin.y) / cell - 0.5;
        let (x0, y0) = (u.floor(), v.floor());
        let (fx, fy) = (u - x0, v - y0);
        let at = |x: i32, y: i32| -> f32 {
            let x = x.clamp(0, cols - 1);
            let y = y.clamp(0, rows - 1);
            f32::from(self.data[(y * cols + x) as usize]) / 65535.0
        };
        let (xi, yi) = (x0 as i32, y0 as i32);
        let a = at(xi, yi) * (1.0 - fx) + at(xi + 1, yi) * fx;
        let b = at(xi, yi + 1) * (1.0 - fx) + at(xi + 1, yi + 1) * fx;
        a * (1.0 - fy) + b * fy
    }

    /// Próbkowanie w jednostkach fizycznych pola.
    #[must_use]
    pub fn sample_scaled(&self, p: Vec2) -> f32 {
        self.sample(p) * self.full_scale
    }

    /// Odległość od najbliższego źródła po siatce, 8-sąsiedztwo, koszt krawędzi
    /// z domknięcia. Kolejka priorytetowa po `(dystans, komórka)` — para jest
    /// porządkiem totalnym, więc remisy nie zależą od kolejności wstawiania.
    ///
    /// `cost(from, to) == u32::MAX` oznacza krawędź nieprzejezdną.
    /// Komórki nieosiągalne dostają [`UNREACHABLE`].
    #[must_use]
    pub fn multi_source_dijkstra(
        spec: GridSpec,
        sources: &[CellId],
        cost: impl Fn(CellId, CellId) -> u32,
    ) -> ScalarField {
        let n = spec.cell_count();
        let cols = i64::from(spec.cols);
        let rows = i64::from(spec.rows);
        let mut dist = vec![u32::MAX; n];
        let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::with_capacity(n / 4 + 16);
        for s in sources {
            if dist[s.0 as usize] != 0 {
                dist[s.0 as usize] = 0;
                heap.push(Reverse((0, s.0)));
            }
        }

        while let Some(Reverse((d, c))) = heap.pop() {
            if d > dist[c as usize] {
                continue; // wpis nieaktualny (leniwe usuwanie)
            }
            let (x, y) = (i64::from(c) % cols, i64::from(c) / cols);
            for (dx, dy) in [
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= cols || ny >= rows {
                    continue;
                }
                let nc = (ny * cols + nx) as u32;
                let w = cost(CellId(c), CellId(nc));
                if w == u32::MAX {
                    continue;
                }
                let nd = d.saturating_add(w);
                if nd < dist[nc as usize] {
                    dist[nc as usize] = nd;
                    heap.push(Reverse((nd, nc)));
                }
            }
        }

        let dmax = dist
            .iter()
            .copied()
            .filter(|d| *d != u32::MAX)
            .max()
            .unwrap_or(0);
        let data = dist
            .iter()
            .map(|&d| {
                if d == u32::MAX {
                    UNREACHABLE
                } else if dmax == 0 {
                    0
                } else {
                    (u64::from(d) * 65534 / u64::from(dmax)) as u16
                }
            })
            .collect();
        ScalarField::from_data(spec, data, dmax as f32)
    }

    /// Suma wpływów zanikających wykładniczo z odległości euklidesowej.
    /// Wkład poniżej 1,6 % (6 okresów połowicznego zaniku) jest odcinany —
    /// inaczej każde źródło kosztowałoby przebieg po całej siatce.
    #[must_use]
    pub fn decay_from(spec: GridSpec, sources: &[(CellId, f32)], half_life_m: f32) -> ScalarField {
        assert!(
            half_life_m > 0.0,
            "decay_from: zerowy okres połowicznego zaniku"
        );
        let mut acc = vec![0.0f32; spec.cell_count()];
        let zasieg = half_life_m * 6.0;
        for &(src, amp) in sources {
            let p0 = spec.cell_center(src);
            for c in spec.cells_in_aabb(Aabb2::from_center_radius(p0, zasieg)) {
                let d = (spec.cell_center(c) - p0).length();
                if d > zasieg {
                    continue;
                }
                // det_math zamiast powf — 00 §K-6 (zakaz libm w symulacji).
                let f = magnat_core::det_math::exp2(-f64::from(d) / f64::from(half_life_m)) as f32;
                acc[c.0 as usize] += amp * f;
            }
        }
        ScalarField::from_data(spec, quantize(&acc), 1.0)
    }

    /// Suma ważona w **stałej kolejności** wejść (00 §2: float wpływający na stan trwały
    /// sumuje się po ustalonym kluczu). Wynik przycięty do 0..=1.
    #[must_use]
    pub fn combine(inputs: &[(&ScalarField, f32)]) -> ScalarField {
        let spec = inputs.first().expect("combine: pusta lista wejść").0.spec;
        let mut acc = vec![0.0f32; spec.cell_count()];
        for (f, w) in inputs {
            assert_eq!(f.spec, spec, "combine: pola o różnych siatkach");
            for (i, a) in acc.iter_mut().enumerate() {
                *a += (f32::from(f.data[i]) / 65535.0) * w;
            }
        }
        ScalarField::from_data(spec, quantize(&acc), 1.0)
    }
}

fn quantize(v: &[f32]) -> Vec<u16> {
    v.iter()
        .map(|x| (x.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(n: u16) -> GridSpec {
        GridSpec::new(Vec2::ZERO, 16, n, n)
    }

    #[test]
    fn dijkstra_zgadza_sie_z_przegladem_zupelnym() {
        // Koszt = 10 na krawędź prostą, 14 na skośną — metryka oktagonalna.
        let s = spec(24);
        let koszt = |a: CellId, b: CellId| {
            let (ax, ay) = s.cell_of_id(a);
            let (bx, by) = s.cell_of_id(b);
            if ax != bx && ay != by {
                14
            } else {
                10
            }
        };
        let pole = ScalarField::multi_source_dijkstra(s, &[CellId(0)], koszt);

        // Odniesienie: Bellman–Ford (przegląd do punktu stałego).
        let n = s.cell_count();
        let mut d = vec![u32::MAX; n];
        d[0] = 0;
        for _ in 0..n {
            let mut zmiana = false;
            for c in 0..n as u32 {
                if d[c as usize] == u32::MAX {
                    continue;
                }
                let (x, y) = s.cell_of_id(CellId(c));
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (i32::from(x) + dx, i32::from(y) + dy);
                        if (dx == 0 && dy == 0)
                            || nx < 0
                            || ny < 0
                            || nx >= i32::from(s.cols)
                            || ny >= i32::from(s.rows)
                        {
                            continue;
                        }
                        let nc = (ny * i32::from(s.cols) + nx) as u32;
                        let w = koszt(CellId(c), CellId(nc));
                        if d[c as usize] + w < d[nc as usize] {
                            d[nc as usize] = d[c as usize] + w;
                            zmiana = true;
                        }
                    }
                }
            }
            if !zmiana {
                break;
            }
        }
        let dmax = *d.iter().max().unwrap();
        for (c, dc) in d.iter().enumerate() {
            let oczekiwane = (u64::from(*dc) * 65534 / u64::from(dmax)) as u16;
            assert_eq!(pole.data()[c], oczekiwane, "komórka {c}");
        }
        assert_eq!(pole.full_scale(), dmax as f32);
    }

    #[test]
    fn nieosiagalne_zostaje_nieosiagalne() {
        let s = spec(8);
        // Pionowa ściana w kolumnie 4 odcina prawą połowę.
        let pole = ScalarField::multi_source_dijkstra(s, &[CellId(0)], |_, b| {
            if s.cell_of_id(b).0 == 4 {
                u32::MAX
            } else {
                1
            }
        });
        for c in 0..s.cell_count() as u32 {
            let (x, _) = s.cell_of_id(CellId(c));
            if x >= 4 {
                assert_eq!(
                    pole.get(CellId(c)),
                    UNREACHABLE,
                    "komórka {c} miała być odcięta"
                );
            } else {
                assert_ne!(pole.get(CellId(c)), UNREACHABLE);
            }
        }
    }

    #[test]
    fn sample_interpoluje_miedzy_srodkami_komorek() {
        let s = spec(4);
        let mut f = ScalarField::new(s);
        f.set(CellId(0), 0);
        f.set(CellId(1), 65535);
        let a = s.cell_center(CellId(0));
        let b = s.cell_center(CellId(1));
        assert!((f.sample(a) - 0.0).abs() < 1e-4);
        assert!((f.sample(b) - 1.0).abs() < 1e-4);
        assert!((f.sample((a + b) * 0.5) - 0.5).abs() < 1e-3);
        // Poza siatką wartość jest przyciągana do brzegu, nie ekstrapolowana.
        assert!((f.sample(a - Vec2::splat(1000.0)) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn combine_sumuje_w_stalej_kolejnosci() {
        let s = spec(4);
        let mut a = ScalarField::new(s);
        let mut b = ScalarField::new(s);
        for c in 0..s.cell_count() as u32 {
            a.set(CellId(c), 20_000);
            b.set(CellId(c), 40_000);
        }
        let c = ScalarField::combine(&[(&a, 0.5), (&b, 0.25)]);
        let oczekiwane =
            (((20_000.0 / 65535.0) * 0.5 + (40_000.0 / 65535.0) * 0.25) * 65535.0 + 0.5) as u16;
        assert!(c.data().iter().all(|v| *v == oczekiwane));
        // Ta sama lista w tej samej kolejności = ten sam wynik bit w bit.
        assert_eq!(
            c.data(),
            ScalarField::combine(&[(&a, 0.5), (&b, 0.25)]).data()
        );
    }

    #[test]
    fn zanik_maleje_z_odlegloscia_i_ma_zasieg() {
        let s = spec(32);
        let srodek = CellId(s.cols as u32 / 2 + (s.rows as u32 / 2) * s.cols as u32);
        let f = ScalarField::decay_from(s, &[(srodek, 1.0)], 32.0);
        assert_eq!(f.get(srodek), 65535);
        let (cx, cy) = s.cell_of_id(srodek);
        let obok = CellId(u32::from(cy) * u32::from(s.cols) + u32::from(cx) + 2);
        assert!(f.get(obok) < f.get(srodek));
        assert_eq!(f.get(CellId(0)), 0, "poza zasięgiem 6 okresów ma być zero");
    }
}
