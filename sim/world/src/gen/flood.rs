//! P4 — wypełnienie zagłębień metodą priority-flood + ε (M1 §5.7).
//!
//! Barnes, Lehman, Mulla 2014. Kolejka priorytetowa rusza od brzegów mapy i od morza
//! w głąb lądu; komórka niższa od bieżącego progu zostaje podniesiona do `próg + ε`.
//! Gwarancja: **każda komórka ma spływ do ujścia**. Bez tego erozja tworzy nieciągłe rzeki
//! i jeziora bez odpływu, a `no_local_minima` z §7.2 nie ma szans przejść.
//!
//! Praca idzie w **milimetrach całkowitych**, nie w metrach `f32`. Powód nie jest estetyczny:
//! ε musi być najmniejszym odróżnialnym przyrostem, a w `f32` przy wysokości 190 m najmniejszy
//! przyrost to ~0,015 m i zależy od wartości — czyli ε przestaje być stałą. W `i32` ε = 1 mm
//! zawsze i wszędzie, a 16,8 mln komórek mieści się w pamięci tak samo (4 B).

use crate::grid::{Grid2, NEIGHBORS_8};
use crate::pipeline::GenCtx;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Przyrost wysokości nadawany komórce wypełnianej — 1 mm.
const EPS_MM: i32 = 1;

pub fn run(ctx: &mut GenCtx) {
    fill(ctx);
}

/// Wypełnia zagłębienia w `work.height_m`, wynik w `work.filled_m`.
/// Wydzielone z `run`, bo P6 woła to powtórnie po erozji (erozja potrafi wydrążyć nowe misy).
pub fn fill(ctx: &mut GenCtx) {
    let dim = ctx.dim();
    let n = dim * dim;

    let h_mm: Vec<i32> = ctx
        .work
        .height_m
        .as_slice()
        .iter()
        .map(|m| (m * 1000.0) as i32)
        .collect();
    let mut filled = h_mm.clone();
    let mut visited = vec![false; n];

    // Kopiec minimowy. Klucz `(wysokość, indeks)` — indeks rozstrzyga remisy, więc kolejność
    // zdejmowania jest w pełni określona i nie zależy od historii wstawiania (00 §3.2).
    let mut heap: BinaryHeap<Reverse<(i32, u32)>> = BinaryHeap::with_capacity(4 * dim);

    // Zasiew: krawędź mapy i całe morze. Morze jest poziomem odniesienia — nie wolno go podnosić.
    for i in 0..n {
        let (x, y) = (i % dim, i / dim);
        let brzeg = x == 0 || y == 0 || x == dim - 1 || y == dim - 1;
        let morze = h_mm[i] <= 0 && ctx.params.region.has_sea();
        if brzeg || morze {
            visited[i] = true;
            heap.push(Reverse((filled[i], i as u32)));
        }
    }

    while let Some(Reverse((e, c))) = heap.pop() {
        let (cx, cy) = ((c as usize % dim) as i64, (c as usize / dim) as i64);
        for (dx, dy) in NEIGHBORS_8 {
            let (nx, ny) = (cx + i64::from(dx), cy + i64::from(dy));
            if nx < 0 || ny < 0 || nx >= dim as i64 || ny >= dim as i64 {
                continue;
            }
            let ni = ny as usize * dim + nx as usize;
            if visited[ni] {
                continue;
            }
            visited[ni] = true;
            // Komórka niższa od progu zostaje podniesiona do progu + ε — dzięki temu
            // powstaje minimalny, ale niezerowy spadek w stronę wyjścia z misy.
            filled[ni] = filled[ni].max(e + EPS_MM);
            heap.push(Reverse((filled[ni], ni as u32)));
        }
    }

    let cells: Vec<f32> = filled.iter().map(|mm| *mm as f32 / 1000.0).collect();
    ctx.work.filled_m = Grid2::from_vec(dim, cells);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    /// Komórka **lądowa** jest lokalnym minimum, gdy żaden z ośmiu sąsiadów nie jest niżej.
    /// Po P4 takich komórek nie może być.
    ///
    /// Morze jest z tego wyłączone i to nie jest wyjątek na siłę: dno morskie **ma** mieć
    /// zagłębienia, bo morze jest poziomem odniesienia, a nie terenem do odwodnienia.
    /// Podnoszenie dna do „spadku ku krawędzi mapy" zrobiłoby z szelfu równię pochyłą.
    fn lokalne_minima(g: &Grid2<f32>) -> usize {
        let dim = g.dim();
        let mut ile = 0;
        for y in 1..dim - 1 {
            for x in 1..dim - 1 {
                let h = *g.get(x, y);
                if h <= 0.0 {
                    continue;
                }
                let nizszy = NEIGHBORS_8.iter().any(|(dx, dy)| {
                    *g.get_clamped(x as i64 + i64::from(*dx), y as i64 + i64::from(*dy)) < h
                });
                if !nizszy {
                    ile += 1;
                }
            }
        }
        ile
    }

    fn swiat(region: Region, seed: u64) -> GenCtx<'static> {
        // Pula żyje tyle, co proces testowy — celowy wyciek zamiast przeplatania czasów życia
        // w pomocniku testowym.
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(2)));
        let params = WorldGenParams {
            seed,
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        crate::gen::landmask::run(&mut ctx);
        crate::gen::height::run(&mut ctx);
        ctx
    }

    #[test]
    fn po_wypelnieniu_nie_ma_lokalnych_minimow() {
        for region in [Region::Mountain, Region::Lowland, Region::Coastal] {
            let mut ctx = swiat(region, 5);
            let przed = lokalne_minima(&ctx.work.height_m);
            fill(&mut ctx);
            let po = lokalne_minima(&ctx.work.filled_m);
            assert!(
                przed > 0,
                "{}: teren bez zagłębień, test nic nie sprawdza",
                region.key()
            );
            assert_eq!(po, 0, "{}: zostało {po} zagłębień", region.key());
        }
    }

    #[test]
    fn wypelnienie_nigdy_nie_obniza_terenu() {
        let mut ctx = swiat(Region::Lowland, 9);
        let przed = ctx.work.height_m.as_slice().to_vec();
        fill(&mut ctx);
        for (i, h) in przed.iter().enumerate() {
            let f = ctx.work.filled_m[i];
            assert!(f >= *h - 0.001, "komórka {i}: {f} < {h}");
        }
    }

    #[test]
    fn morze_nie_jest_podnoszone() {
        let mut ctx = swiat(Region::Coastal, 4);
        let przed = ctx.work.height_m.as_slice().to_vec();
        fill(&mut ctx);
        for (i, h) in przed.iter().enumerate() {
            if *h < -1.0 {
                assert!(
                    (ctx.work.filled_m[i] - *h).abs() < 0.01,
                    "morska komórka {i} podniesiona z {h} na {}",
                    ctx.work.filled_m[i]
                );
            }
        }
    }
}
