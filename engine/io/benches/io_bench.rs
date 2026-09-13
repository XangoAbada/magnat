//! Benchmarki `magnat-io` (M0 §7.3): B-4 hash stanu, B-6 snapshot.

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_core::{HashState, Money, StateHasher};
use magnat_ecs::{Component, World};
use magnat_io::{load_world, save_world, world_state_hash};
use std::hint::black_box;

macro_rules! komponent {
    ($name:ident, $inner:ty) => {
        #[derive(Debug, Clone, Copy)]
        struct $name($inner);
        impl HashState for $name {
            fn hash_state(&self, h: &mut StateHasher) {
                h.write(&self.0.to_le_bytes());
            }
        }
        impl Component for $name {
            const NAME: &'static str = stringify!($name);
        }
    };
}

komponent!(A, i64);
komponent!(B, i64);
komponent!(C, i64);
komponent!(D, i64);
komponent!(E, i64);
komponent!(F, i64);

#[derive(Debug, Clone, Copy)]
struct Wallet(Money);
impl HashState for Wallet {
    fn hash_state(&self, h: &mut StateHasher) {
        self.0.hash_state(h);
    }
}
impl Component for Wallet {
    const NAME: &'static str = "Wallet";
}

#[derive(Debug, Clone, Copy)]
struct Bulk([u8; 256]);
impl HashState for Bulk {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&self.0);
    }
}
impl Component for Bulk {
    const NAME: &'static str = "Bulk";
}

const N: i64 = 400_000;

fn swiat() -> World {
    let mut w = World::new(3);
    for i in 0..N {
        let mut e = w
            .spawn()
            .with(A(i))
            .with(B(i))
            .with(C(i))
            .with(Wallet(Money(i * 13)));
        match i % 4 {
            0 => e = e.with(D(i)).with(E(i)).with(F(i)),
            1 => e = e.with(D(i)).with(Bulk([7; 256])),
            2 => e = e.with(E(i)),
            _ => {}
        }
        let _ = e.id();
    }
    w
}

/// B-4: hash 400 tys. encji. Liczony co 1000 ticków, więc amortyzowany narzut
/// ma być poniżej 0,1 % — próg bezwzględny z §7.3 to 40 ms.
fn b4_hash(c: &mut Criterion) {
    let w = swiat();
    c.bench_function("B-4 world_state_hash 400k encji", |b| {
        b.iter(|| black_box(world_state_hash(&w)));
    });
}

/// B-6: zapis i odczyt 400 tys. encji (zstd:3). Próg: zapis ≤ 5 s i ≤ 300 MB.
fn b6_snapshot(c: &mut Criterion) {
    let w = swiat();
    let dir = std::env::temp_dir().join("magnat-bench");
    std::fs::create_dir_all(&dir).expect("katalog");
    let sciezka = dir.join("bench.mgs");
    let rejestr = w.components().clone();

    let mut grupa = c.benchmark_group("B-6 snapshot");
    grupa.sample_size(10);
    grupa.bench_function("zapis 400k", |b| {
        b.iter(|| black_box(save_world(&w, &sciezka).expect("zapis")));
    });
    let raport = save_world(&w, &sciezka).expect("zapis");
    println!(
        "rozmiar pliku: {:.1} MB ({} sekcji)",
        raport.bytes as f64 / 1_048_576.0,
        raport.sections
    );
    grupa.bench_function("odczyt 400k", |b| {
        b.iter(|| black_box(load_world(&sciezka, &rejestr).expect("odczyt")));
    });
    grupa.finish();
    std::fs::remove_file(&sciezka).ok();
}

criterion_group!(benches, b4_hash, b6_snapshot);
criterion_main!(benches);
