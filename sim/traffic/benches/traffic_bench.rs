//! Budżety warstwy Mikro (M4d §7.3, WP12).
//!
//! **Bench mierzy minutę świata, a nie jedno wywołanie.** To jest cała nauka z WP14
//! (M4c §5.12): kryterium M3b brzmiało „5 tys. pieszych ≤ 2 ms/klatkę" i raportowało
//! 47 µs, bo mierzyło **jeden** `micro_step`, a system robił ich sześćset. Realny koszt
//! tej samej sceny był czternastokrotnością progu ogłoszonego jako spełniony z zapasem
//! rzędu wielkości.
//!
//! Dlatego każda grupa poniżej wywołuje warstwę tak, jak woła ją system doby: zasilenie
//! plus jeden `micro_step` na koniec minuty, z pełnym całkowaniem IDM w środku (`R-1`).
//! Liczba, która stąd wychodzi, jest bezpośrednio porównywalna z wierszem „`micro_step`
//! 4,0 ms / 100 ms gry" z §7.3, czyli z **2 400 ms na minutę świata**.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use magnat_core::WorldCoord;
use magnat_sim_snapshot::{PedestrianRecord, VehicleRecord};
use magnat_traffic::{IdmParams, MicroLayer, VehicleFeed};
use std::hint::black_box;

/// Sufit rysowania z §7.3. Ciężarówka liczy się podwójnie, więc przy samych osobowych
/// jest to zarazem liczba pojazdów.
const POJAZDOW: u32 = 3_000;
/// Sześć krawędzi po pięciuset metrach — tyle, żeby na każdej stało po pięćset
/// pojazdów i car-following miał z kim liczyć luki.
const KRAWEDZI: u32 = 6;
const DLUGOSC_CM: i32 = 50_000;
const MINUTA_MS: u64 = 60_000;

fn krawedz(i: u32) -> [WorldCoord; 2] {
    let y = (i as i32) * 2_000;
    [
        WorldCoord::new(0, y, 0),
        WorldCoord::new(DLUGOSC_CM, y, 0),
    ]
}

/// Warstwa z `n` pojazdami rozstawionymi po `KRAWEDZI` krawędziach, oknem obejmującym
/// całą scenę i zegarem ustawionym na początek minuty.
fn scena(n: u32, pasy: u8) -> MicroLayer {
    let m = MicroLayer::with_params(IdmParams::load_default().unwrap_or_default());
    m.set_window(Some((0, 0)), 100_000);
    m.set_cap(u32::MAX);
    assert!(m.begin_vehicle_feed());
    for v in 0..n {
        let e = v % KRAWEDZI;
        m.feed_vehicle(
            VehicleFeed {
                vehicle: v,
                edge: e,
                class: 0,
                lanes: pasy,
                len_cm: 450,
                // 50 km/h.
                v_free_cms: 1_389.0,
                entry_cs: 0,
                // Krawędź 500 m przy 50 km/h to 36 s; przy pięciuset pojazdach na
                // odcinek scena jest z definicji zatłoczona i IDM ma co robić.
                exit_cs: 3_600,
                car_following: true,
            },
            &krawedz(e),
        );
    }
    m.end_vehicle_feed();
    m
}

/// `bench_micro_tick_3k` z §7.3 — **na minutę świata**.
fn m4d_micro(c: &mut Criterion) {
    let mut g = c.benchmark_group("m4d-1 micro");
    for pasy in [1u8, 3] {
        g.bench_function(format!("krok_minuty_{POJAZDOW}_poj_{pasy}_pasy"), |b| {
            b.iter_batched(
                || scena(POJAZDOW, pasy),
                |m| {
                    m.step(black_box(MINUTA_MS));
                    black_box(m.vehicles())
                },
                BatchSize::LargeInput,
            );
        });
    }
    g.finish();
}

/// Zrzut do renderera — kopia raz na klatkę, budżet 0,8 ms z §7.3.
fn m4d_snapshot(c: &mut Criterion) {
    let m = scena(POJAZDOW, 2);
    m.step(MINUTA_MS);
    for i in 0..5_000u32 {
        m.enter(
            i,
            &[WorldCoord::new(0, 0, 0), WorldCoord::new(20_000, 0, 0)],
            0,
            30,
        );
    }
    let mut pojazdy: Vec<VehicleRecord> = Vec::new();
    let mut piesi: Vec<PedestrianRecord> = Vec::new();
    let mut g = c.benchmark_group("m4d-2 zrzut");
    g.bench_function("pojazdy_3k", |b| {
        b.iter(|| {
            m.vehicle_snapshot(&mut pojazdy);
            black_box(pojazdy.len())
        });
    });
    g.bench_function("piesi_5k", |b| {
        b.iter(|| {
            m.snapshot(&mut piesi);
            black_box(piesi.len())
        });
    });
    g.finish();
}

/// Zasilenie warstwy z mezo: ta pętla chodzi co minutę dla każdego pojazdu w ruchu,
/// więc jej koszt jest tym, co płaci gracz za **otwarty** kadr.
fn m4d_feed(c: &mut Criterion) {
    let mut g = c.benchmark_group("m4d-3 zasilenie");
    g.bench_function(format!("{POJAZDOW}_pojazdow"), |b| {
        b.iter(|| black_box(scena(POJAZDOW, 2).vehicles()));
    });
    g.finish();
}

criterion_group!(benches, m4d_micro, m4d_snapshot, m4d_feed);
criterion_main!(benches);
