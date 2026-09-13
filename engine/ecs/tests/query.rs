//! Zapytania: kompletność iteracji, filtry, dostęp i równoległość (M0 WP-05, §7.2).

use magnat_core::{HashState, StateHasher};
use magnat_ecs::{Component, Entity, With, Without, World};
use magnat_jobs::JobPool;

macro_rules! komponent {
    ($name:ident, $pole:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct $name($pole);
        impl HashState for $name {
            fn hash_state(&self, h: &mut StateHasher) {
                self.0.hash_state(h);
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

fn swiat(n: i64) -> World {
    let mut w = World::new(1);
    for i in 0..n {
        let mut e = w.spawn().with(A(i));
        if i % 2 == 0 {
            e = e.with(B(i * 10));
        }
        if i % 5 == 0 {
            e = e.with(C(i * 100));
        }
        let _ = e.id();
    }
    w
}

#[test]
fn iteracja_odwiedza_wszystkie_i_tylko_pasujace() {
    let mut w = swiat(10_000);

    let suma_a: i64 = w.query::<&A, ()>().iter().map(|a| a.0).sum();
    assert_eq!(suma_a, (0..10_000).sum::<i64>());

    let ile_ab = w.query::<(&A, &B), ()>().iter().count();
    assert_eq!(ile_ab, 5_000);

    // Suma kontrolna po parach — wyłapuje pomieszanie kolumn między archetypami.
    let suma_par: i64 = w
        .query::<(&A, &B), ()>()
        .iter()
        .map(|(a, b)| b.0 - a.0 * 10)
        .sum();
    assert_eq!(suma_par, 0);
}

#[test]
fn filtry_zawezaja_zbior_archetypow() {
    let mut w = swiat(1_000);
    let z_b = w.query::<&A, With<B>>().iter().count();
    assert_eq!(z_b, 500);
    let bez_b = w.query::<&A, Without<B>>().iter().count();
    assert_eq!(bez_b, 500);
    let z_b_bez_c = w.query::<&A, (With<B>, Without<C>)>().iter().count();
    // Co druga ma B, co piąta ma C; wspólne (co dziesiąta) odpada.
    assert_eq!(z_b_bez_c, 500 - 100);
}

#[test]
fn zapis_przez_zapytanie_dociera_do_swiata() {
    let mut w = swiat(1_000);
    for (a, b) in w.query::<(&mut A, &B), ()>().iter() {
        a.0 += b.0;
    }
    let suma: i64 = w.query::<&A, With<B>>().iter().map(|a| a.0).sum();
    let oczekiwana: i64 = (0..1_000).filter(|i| i % 2 == 0).map(|i| i + i * 10).sum();
    assert_eq!(suma, oczekiwana);
}

#[test]
fn opcjonalny_komponent_nie_zaweza_zbioru() {
    let mut w = swiat(100);
    let mut q = w.query::<(&A, Option<&C>), ()>();
    let (z_c, bez_c): (Vec<_>, Vec<_>) = q.iter().partition(|(_, c)| c.is_some());
    assert_eq!(z_c.len(), 20);
    assert_eq!(bez_c.len(), 80);
}

#[test]
fn entity_jest_elementem_zapytania() {
    let mut w = swiat(100);
    let pary: Vec<(Entity, i64)> = w
        .query::<(Entity, &A), ()>()
        .iter()
        .map(|(e, a)| (e, a.0))
        .collect();
    assert_eq!(pary.len(), 100);
    // Każda encja wystąpiła dokładnie raz.
    let mut indeksy: Vec<u32> = pary.iter().map(|(e, _)| e.index()).collect();
    indeksy.sort_unstable();
    indeksy.dedup();
    assert_eq!(indeksy.len(), 100);
}

#[test]
fn get_trafia_w_konkretna_encje() {
    let mut w = World::new(1);
    let z_b = w.spawn().with(A(1)).with(B(2)).id();
    let bez_b = w.spawn().with(A(3)).id();

    let mut q = w.query::<(&A, &B), ()>();
    assert_eq!(q.get(z_b).map(|(a, b)| (a.0, b.0)), Some((1, 2)));
    assert!(
        q.get(bez_b).is_none(),
        "encja bez B nie pasuje do zapytania"
    );
}

#[test]
#[should_panic(expected = "sprzeczny dostęp")]
fn zapytanie_czytajace_i_piszace_ten_sam_komponent_panikuje() {
    let mut w = swiat(10);
    // Query<(&mut A, &A)> dałoby &mut A i &A na ten sam wiersz — typ tego nie wyłapie,
    // więc łapie to konstruktor zapytania (M0 §5.6).
    let _ = w.query::<(&mut A, &A), ()>();
}

#[test]
fn rownolegla_iteracja_dociera_do_kazdego_wiersza() {
    let mut w = swiat(50_000);
    let pool = JobPool::new(8);
    w.query::<&mut A, ()>().par_for_each(&pool, |a| {
        a.0 *= 2;
    });
    let suma: i64 = w.query::<&A, ()>().iter().map(|a| a.0).sum();
    assert_eq!(suma, 2 * (0..50_000).sum::<i64>());
}

#[test]
fn redukcja_rownolegla_nie_zalezy_od_liczby_watkow() {
    let mut w = swiat(50_000);
    // Splot niełączny i nieprzemienny: gdyby składanie szło po kolejności zakończenia
    // zadań, wynik byłby inny przy każdej liczbie wątków.
    let splot = |acc: i64, v: i64| acc.wrapping_mul(31).wrapping_add(v);
    let mut wyniki = Vec::new();
    for threads in [1, 2, 8, 16] {
        let pool = JobPool::new(threads);
        wyniki.push(w.query::<&A, ()>().par_fold(
            &pool,
            |view| {
                let mut suma = 0i64;
                while let Some(a) = view.next_row() {
                    suma = suma.wrapping_mul(7).wrapping_add(a.0);
                }
                suma
            },
            splot,
            0i64,
        ));
    }
    assert!(wyniki.windows(2).all(|p| p[0] == p[1]), "{wyniki:?}");
}

#[test]
fn dlugosc_zapytania_zgadza_sie_z_iteracja() {
    let mut w = swiat(3_333);
    let mut q = w.query::<(&A, &B), ()>();
    let len = q.len();
    assert_eq!(len, q.iter().count());
}
