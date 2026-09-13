//! P5 — kierunki spływu D8, porządek topologiczny i akumulacja (M1 §5.7).
//!
//! Trzy rzeczy w jednym przebiegu, bo wszystkie stoją na tej samej strukturze — drzewie
//! odbiorników:
//!
//! 1. **Odbiornik D8**: sąsiad o największym spadku `(h_i − h_j)/dist`. Remis rozstrzyga
//!    **najniższy indeks sąsiada** w kolejności N, NE, E, SE, S, SW, W, NW. To nie jest
//!    szczegół implementacyjny: przy płaskim terenie remisy są częste, a zmiana reguły
//!    zmienia bieg każdej rzeki w każdym świecie wygenerowanym wcześniej.
//! 2. **Stos** — kolejność od ujść w górę zlewni. Przejście w przód liczy Strahlera i erozję
//!    (odbiornik przed donorem), w tył — akumulację (donor przed odbiornikiem). Jeden wektor
//!    zamiast rekurencji, bo zlewnia potrafi mieć milion komórek, a stos wątku nie.
//! 3. **Akumulacja** pola zlewni w m² i przydział komórek do zlewni.

use crate::fields::NO_RECEIVER;
use crate::grid::{neighbor_dist_m, NEIGHBORS_8};
use crate::params::WORK_CELL_M;
use crate::pipeline::GenCtx;

pub fn run(ctx: &mut GenCtx) {
    route(ctx);
}

/// Wyznacza odbiorniki, stos, zlewnie i akumulację na podstawie `work.filled_m`.
/// Wydzielone z `run`, bo P6 woła to powtórnie po erozji.
pub fn route(ctx: &mut GenCtx) {
    let dim = ctx.dim();
    let n = dim * dim;
    let cell_m = f64::from(WORK_CELL_M);

    // ── 1. Odbiorniki ────────────────────────────────────────────────────────────────
    let filled = ctx.work.filled_m.as_slice().to_vec();
    let mut receiver = vec![NO_RECEIVER; n];
    let dist: [f32; 8] = std::array::from_fn(|k| neighbor_dist_m(k, cell_m) as f32);

    ctx.work.receiver.par_rows_mut(ctx.pool, |y, row| {
        for (x, r) in row.iter_mut().enumerate() {
            let i = y * dim + x;
            let h = filled[i];
            let mut best = NO_RECEIVER;
            let mut best_slope = 0.0f32;
            for (k, (dx, dy)) in NEIGHBORS_8.iter().enumerate() {
                let (nx, ny) = (x as i64 + i64::from(*dx), y as i64 + i64::from(*dy));
                if nx < 0 || ny < 0 || nx >= dim as i64 || ny >= dim as i64 {
                    continue;
                }
                let ni = ny as usize * dim + nx as usize;
                let slope = (h - filled[ni]) / dist[k];
                // Ostry warunek `>`: pierwszy sąsiad o danym spadku wygrywa, a sąsiedzi
                // idą w ustalonej kolejności — stąd deterministyczne rozstrzyganie remisów.
                if slope > best_slope {
                    best_slope = slope;
                    best = ni as u32;
                }
            }
            *r = best;
        }
    });
    receiver.copy_from_slice(ctx.work.receiver.as_slice());

    // ── 2. Listy donorów w postaci CSR ───────────────────────────────────────────────
    // Wektor przesunięć + wektor donorów zamiast `Vec<Vec<u32>>`: jedna alokacja zamiast
    // 16,8 mln, i sąsiedztwo w pamięci przy przejściu stosu.
    let mut donor_count = vec![0u32; n + 1];
    for r in &receiver {
        if *r != NO_RECEIVER {
            donor_count[*r as usize + 1] += 1;
        }
    }
    for i in 0..n {
        donor_count[i + 1] += donor_count[i];
    }
    let donor_start = donor_count;
    let mut fill_at = donor_start.clone();
    let mut donors = vec![0u32; donor_start[n] as usize];
    for (c, r) in receiver.iter().enumerate() {
        if *r != NO_RECEIVER {
            let slot = &mut fill_at[*r as usize];
            donors[*slot as usize] = c as u32;
            *slot += 1;
        }
    }

    // ── 3. Stos: przejście w głąb od każdego ujścia ──────────────────────────────────
    let mut stack: Vec<u32> = Vec::with_capacity(n);
    let mut basin = vec![u32::MAX; n];
    let mut outlets: Vec<u32> = Vec::new();
    let mut ranges: Vec<(u32, u32)> = Vec::new();

    // Ujścia w rosnącej kolejności indeksów — to ustala kolejność zlewni w stosie,
    // a więc i kolejność redukcji w erozji równoległej (00 §3.3).
    for c in 0..n {
        if receiver[c] != NO_RECEIVER {
            continue;
        }
        let basin_id = outlets.len() as u32;
        let start = stack.len() as u32;
        outlets.push(c as u32);

        // Iteracyjne DFS: kursor mówi, ilu donorów danej komórki już wciągnięto.
        // Przy milionie komórek w zlewni rekurencja przepełniłaby stos wątku.
        stack.push(c as u32);
        basin[c] = basin_id;
        let mut head = start as usize;
        while head < stack.len() {
            let cur = stack[head] as usize;
            head += 1;
            let (a, b) = (donor_start[cur] as usize, donor_start[cur + 1] as usize);
            for d in &donors[a..b] {
                basin[*d as usize] = basin_id;
                stack.push(*d);
            }
        }
        ranges.push((start, stack.len() as u32));
    }
    debug_assert_eq!(stack.len(), n, "stos nie pokrywa wszystkich komórek");

    // ── 4. Akumulacja: w tył po stosie, donor przed odbiornikiem ─────────────────────
    let cell_area = (WORK_CELL_M * WORK_CELL_M) as f32;
    let mut acc = vec![cell_area; n];
    for c in stack.iter().rev() {
        let r = receiver[*c as usize];
        if r != NO_RECEIVER {
            acc[r as usize] += acc[*c as usize];
        }
    }

    ctx.work.stack = stack;
    ctx.work.outlets = outlets;
    ctx.work.basin_ranges = ranges;
    ctx.work.basin = crate::grid::Grid2::from_vec(dim, basin);
    ctx.work.flow_acc = crate::grid::Grid2::from_vec(dim, acc);
    ctx.work.donor_start = donor_start;
    ctx.work.donors = donors;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn swiat(region: Region, seed: u64, threads: usize) -> GenCtx<'static> {
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(threads)));
        let params = WorldGenParams {
            seed,
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        crate::gen::landmask::run(&mut ctx);
        crate::gen::height::run(&mut ctx);
        crate::gen::flood::run(&mut ctx);
        route(&mut ctx);
        ctx
    }

    #[test]
    fn kazda_komorka_ma_droge_do_ujscia() {
        let ctx = swiat(Region::Mountain, 3, 1);
        let n = ctx.work.receiver.len();
        for c in 0..n {
            let mut cur = c;
            let mut kroki = 0;
            while ctx.work.receiver[cur] != NO_RECEIVER {
                cur = ctx.work.receiver[cur] as usize;
                kroki += 1;
                assert!(kroki <= n, "pętla spływu przy komórce {c}");
            }
        }
    }

    #[test]
    fn stos_stawia_odbiornik_przed_donorem() {
        // To jest niezmiennik, na którym stoi cała erozja: w przejściu w przód odbiornik
        // ma już policzoną nową wysokość, zanim policzy ją donor.
        let ctx = swiat(Region::River, 8, 1);
        let mut pozycja = vec![u32::MAX; ctx.work.receiver.len()];
        for (p, c) in ctx.work.stack.iter().enumerate() {
            pozycja[*c as usize] = p as u32;
        }
        for c in 0..ctx.work.receiver.len() {
            let r = ctx.work.receiver[c];
            if r != NO_RECEIVER {
                assert!(
                    pozycja[r as usize] < pozycja[c],
                    "komórka {c} stoi przed swoim odbiornikiem"
                );
            }
        }
    }

    #[test]
    fn akumulacja_zachowuje_powierzchnie_mapy() {
        // Suma akumulacji w ujściach musi równać się powierzchni całej mapy — inaczej
        // gdzieś zniknęła woda, a z nią kilometry kwadratowe zlewni.
        let ctx = swiat(Region::Lowland, 12, 1);
        let cell = (WORK_CELL_M * WORK_CELL_M) as f64;
        let suma: f64 = ctx
            .work
            .outlets
            .iter()
            .map(|o| f64::from(ctx.work.flow_acc[*o as usize]))
            .sum();
        let oczekiwana = ctx.work.flow_acc.len() as f64 * cell;
        // f32 przy 16 mln komórek gubi młodsze bity — 0,1 % to granica sensowna,
        // a nie zamiatanie błędu pod dywan.
        let blad = (suma - oczekiwana).abs() / oczekiwana;
        assert!(blad < 0.001, "suma {suma} wobec {oczekiwana}, błąd {blad}");
    }

    #[test]
    fn trasowanie_nie_zalezy_od_liczby_watkow() {
        let a = swiat(Region::Mountain, 21, 1);
        let b = swiat(Region::Mountain, 21, 8);
        assert_eq!(a.work.receiver.as_slice(), b.work.receiver.as_slice());
        assert_eq!(a.work.stack, b.work.stack);
        assert_eq!(a.work.flow_acc.as_slice(), b.work.flow_acc.as_slice());
    }
}
