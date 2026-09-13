//! B-7: budżet pamięci storage'u (M0 §7.3, PRD §17.7).
//!
//! Mierzymy **zaalokowane bajty chunków**, a nie RSS procesu: RSS zależy od alokatora
//! i systemu, więc w teście dawałby fałszywe alarmy na współdzielonych runnerach.
//! To, co naprawdę pilnujemy, to narzut storage'u nad surowymi danymi — RSS całego
//! procesu jest bramką ręczną na sprzęcie odniesienia.

use magnat_core::{HashState, StateHasher};
use magnat_ecs::{Component, World};

macro_rules! komponent {
    ($name:ident, $rozmiar:literal) => {
        #[derive(Debug, Clone, Copy)]
        struct $name([u8; $rozmiar]);
        impl HashState for $name {
            fn hash_state(&self, h: &mut StateHasher) {
                h.write(&self.0);
            }
        }
        impl Component for $name {
            const NAME: &'static str = stringify!($name);
        }
    };
}

komponent!(Hot, 64);
komponent!(Warm, 128);
komponent!(Cold, 208);

#[test]
fn czterysta_tysiecy_encji_miesci_sie_w_budzecie() {
    const N: u32 = 400_000;
    let mut w = World::new(1);
    for _ in 0..N {
        let _ = w
            .spawn()
            .with(Hot([1; 64]))
            .with(Warm([2; 128]))
            .with(Cold([3; 208]))
            .id();
    }

    let zaalokowane: usize = w.archetypes().iter().map(|a| a.allocated_bytes()).sum();
    let dane_uzyteczne = N as usize * (64 + 128 + 208 + 8); // + Entity
    let narzut = zaalokowane as f64 / dane_uzyteczne as f64;

    println!(
        "400 tys. encji × 400 B: {:.1} MB zaalokowane, narzut {:.1} %",
        zaalokowane as f64 / 1_048_576.0,
        (narzut - 1.0) * 100.0
    );

    // Cel z §7.3 to 1,2 GB RSS dla świata 400 tys. encji; sam storage ma być
    // rzędu 165 MB, a narzut chunkowania — poniżej 15 %.
    assert!(
        zaalokowane < 400 * 1024 * 1024,
        "storage zajął {} MB",
        zaalokowane / 1_048_576
    );
    assert!(
        narzut < 1.15,
        "narzut chunkowania {:.1} %",
        (narzut - 1.0) * 100.0
    );
}
