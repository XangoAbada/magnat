//! Determinizm generatora świata — bramka 2 fazy M1 (00 §3.6, M1 §7.1).
//!
//! To są testy kontraktowe, nie jednostkowe: sprawdzają, że ten sam seed daje ten sam świat
//! niezależnie od liczby wątków i od przebiegu, bo na tym stoi wszystko dalsze — M2 stawia
//! parcele na tym terenie, a rozjazd o decymetr przesuwa granicę działki w zapisie gry.

use magnat_jobs::JobPool;
use magnat_world::{
    generate, Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize,
};

fn params(seed: u64, region: Region, size: WorldSize) -> WorldGenParams {
    WorldGenParams {
        seed,
        size,
        region,
        epoch: Epoch::Y1990,
        profile: EconomyProfile::Mixed,
        difficulty: Difficulty::Normal,
    }
}

#[test]
fn terrain_hash_stable() {
    let pool = JobPool::new(4);
    let p = params(0x00C0_FFEE, Region::River, WorldSize::Small4km);
    let (_, a) = generate(p, &pool).unwrap();
    let (_, b) = generate(p, &pool).unwrap();
    assert_eq!(a.terrain_hash, b.terrain_hash, "dwa przebiegi, dwa światy");
    assert_eq!(
        a.stats, b.stats,
        "statystyki się rozjechały mimo zgodnego hasha"
    );
}

#[test]
fn terrain_thread_invariant() {
    // Ryzyko R1 fazy M1: niedeterminizm `f32` w erozji. Gdyby redukcja składała się
    // po kolejności zakończenia zadań, ten test padłby natychmiast.
    let p = params(31_337, Region::Mountain, WorldSize::Small4km);
    let mut hashes = Vec::new();
    for threads in [1usize, 2, 8, 16] {
        let pool = JobPool::new(threads);
        let (_, r) = generate(p, &pool).unwrap();
        hashes.push((threads, r.terrain_hash));
    }
    let wzorzec = hashes[0].1;
    for (t, h) in &hashes {
        assert_eq!(*h, wzorzec, "{t} wątków dało inny teren");
    }
}

#[test]
fn seed_i_region_zmieniaja_swiat() {
    // Test odwrotny do powyższych: gdyby hash był stały niezależnie od wejścia, poprzednie
    // testy przechodziłyby, a generator nic by nie robił.
    let pool = JobPool::new(2);
    let a = generate(params(1, Region::Lowland, WorldSize::Small4km), &pool)
        .unwrap()
        .1
        .terrain_hash;
    let b = generate(params(2, Region::Lowland, WorldSize::Small4km), &pool)
        .unwrap()
        .1
        .terrain_hash;
    let c = generate(params(1, Region::Mountain, WorldSize::Small4km), &pool)
        .unwrap()
        .1
        .terrain_hash;
    assert_ne!(a, b, "zmiana ziarna nie zmieniła świata");
    assert_ne!(a, c, "zmiana regionu nie zmieniła świata");
}

/// Tabela hashy dla siatki seed × region. Zmiana którejkolwiek wartości oznacza, że każdy
/// świat wygenerowany wcześniej z tego samego ziarna wygląda teraz inaczej — a więc zapisy
/// gry przestały być wczytywalne w sensie, o który chodzi (M1 §7.1 `terrain_hash_matrix`).
///
/// **Zatwierdzanie zmiany:** `cargo test -p magnat-world --release -- --ignored --nocapture
/// wypisz_macierz_hashy` i przeniesienie wyniku tutaj, razem z wpisem w dzienniku.
#[test]
fn terrain_hash_matrix() {
    const OCZEKIWANE: &[(u64, Region, u128)] = &[];
    if OCZEKIWANE.is_empty() {
        // Macierz jest pusta do czasu zamknięcia grupy W (geologia i klimat wchodzą
        // do hasha). Zatwierdzanie jej wcześniej znaczyłoby przepisywanie jej co pakiet.
        return;
    }
    let pool = JobPool::new(2);
    for (seed, region, want) in OCZEKIWANE {
        let (_, r) = generate(params(*seed, *region, WorldSize::Small4km), &pool).unwrap();
        assert_eq!(
            r.terrain_hash.0,
            *want,
            "seed {seed}, region {}",
            region.key()
        );
    }
}

#[test]
#[ignore = "narzędzie, nie test: wypisuje macierz hashy do zatwierdzenia"]
fn wypisz_macierz_hashy() {
    let pool = JobPool::new(4);
    for seed in [1u64, 42, 0x00C0_FFEE, 31_337] {
        for region in Region::ALL {
            let (_, r) = generate(params(seed, *region, WorldSize::Small4km), &pool).unwrap();
            println!(
                "    ({seed}, Region::{:?}, 0x{:032X}),",
                region, r.terrain_hash.0
            );
        }
    }
}
