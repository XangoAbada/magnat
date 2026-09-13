//! Testy własnościowe pieniądza (M0 WP-02, 00 §6).
//!
//! Bramka 3 z `00-postep.md`: suma podziału równa się kwocie dzielonej **z tolerancją 0**.
//! Nie „w przybliżeniu", nie „dla typowych danych" — zawsze, także dla kwot ujemnych,
//! wag zerowych i skrajów zakresu `i64`.

use magnat_core::{split_proportional, Money};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    /// Własność nr 1: podział niczego nie gubi i niczego nie tworzy.
    #[test]
    fn podzial_sumuje_sie_do_calosci(
        total in any::<i64>(),
        weights in prop::collection::vec(0u64..1_000_000, 1..12),
    ) {
        let parts = split_proportional(Money(total), &weights);
        prop_assert_eq!(parts.len(), weights.len());
        let sum: i128 = parts.iter().map(|m| i128::from(m.0)).sum();
        prop_assert_eq!(sum, i128::from(total));
    }

    /// Własność nr 2: strona o wadze zerowej nie dostaje ani grosza.
    /// Bez tego reszta „po jednym groszu do kolejnych" wypłacałaby komuś,
    /// kto nie ma tytułu do wypłaty.
    #[test]
    fn waga_zerowa_nie_dostaje_nic(
        total in any::<i64>(),
        weights in prop::collection::vec(0u64..100, 1..12),
    ) {
        prop_assume!(weights.iter().any(|&w| w > 0));
        let parts = split_proportional(Money(total), &weights);
        for (part, &w) in parts.iter().zip(&weights) {
            if w == 0 {
                prop_assert_eq!(*part, Money::ZERO);
            }
        }
    }

    /// Własność nr 3: podział jest monotoniczny względem wag **z dokładnością do
    /// jednego grosza reszty**. Dokładnej monotoniczności nie da się mieć razem
    /// z własnością nr 1: przy `total = 1` i wagach `[1, 2]` część całkowita obu
    /// udziałów to zero, a jedyny grosz musi trafić do kogoś — i trafia do pierwszego
    /// w kolejności wag, bo ta kolejność jest kontraktem wywołującego.
    /// To jest własność, na której opiera się podział pensji (M7) i dywidend (M10).
    #[test]
    fn wieksza_waga_nie_dostaje_istotnie_mniej(
        total in 0i64..i64::MAX,
        a in 1u64..1000,
        b in 1u64..1000,
    ) {
        let (mniejsza, wieksza) = if a <= b { (a, b) } else { (b, a) };
        let parts = split_proportional(Money(total), &[mniejsza, wieksza]);
        prop_assert!(
            parts[1].0 >= parts[0].0 - 1,
            "{:?} vs {:?} przy wagach {mniejsza}/{wieksza}",
            parts[0],
            parts[1]
        );
    }

    /// Własność nr 4: zaokrąglenie połówek od zera jest symetryczne względem znaku.
    #[test]
    fn dzielenie_jest_symetryczne_wzgledem_znaku(
        v in (i64::MIN + 1)..i64::MAX,
        den in prop::sample::select(vec![1i64, 2, 3, 7, 100, -1, -2, -3, -7, -100]),
    ) {
        let dodatnie = Money(v).div_round_half_up(den);
        let ujemne = Money(-v).div_round_half_up(den);
        prop_assert_eq!(dodatnie.0, -ujemne.0);
    }

    /// Własność nr 5: mnożenie przez ułamek `n/n` jest tożsamością.
    #[test]
    fn mnozenie_przez_jedynke_nic_nie_zmienia(
        v in any::<i64>(),
        n in 1i64..1_000_000,
    ) {
        prop_assert_eq!(Money(v).mul_ratio(n, n), Money(v));
    }
}
