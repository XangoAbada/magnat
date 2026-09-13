//! Arytmetyka pieniądza (00 §2, M0 §5.1).
//!
//! Zasada: nigdy niejawne obcinanie. Każde mnożenie przez ułamek i każde dzielenie
//! mówi wprost, jak zaokrągla, a podział kwoty między N stron sumuje się do oryginału
//! co do grosza — to jest własność testowana, nie intencja.

use crate::types::Money;

impl Money {
    #[inline]
    #[must_use]
    pub const fn checked_add(self, rhs: Money) -> Option<Money> {
        match self.0.checked_add(rhs.0) {
            Some(v) => Some(Money(v)),
            None => None,
        }
    }

    #[inline]
    #[must_use]
    pub const fn checked_sub(self, rhs: Money) -> Option<Money> {
        match self.0.checked_sub(rhs.0) {
            Some(v) => Some(Money(v)),
            None => None,
        }
    }

    #[inline]
    #[must_use]
    pub const fn checked_mul_int(self, k: i64) -> Option<Money> {
        match self.0.checked_mul(k) {
            Some(v) => Some(Money(v)),
            None => None,
        }
    }

    /// Mnożenie przez ułamek `num/den` z zaokrągleniem połówek **od zera**.
    /// Liczone w `i128`, więc nie przepełnia się dla |kwota| < 2^63 i |num|, |den| < 2^63.
    ///
    /// Panika przy `den == 0` — to błąd wywołującego, nie stan danych.
    #[inline]
    #[must_use]
    pub fn mul_ratio(self, num: i64, den: i64) -> Money {
        assert!(den != 0, "Money::mul_ratio: mianownik zerowy");
        div_round_half_away(i128::from(self.0) * i128::from(num), i128::from(den))
    }

    /// Dzielenie z zaokrągleniem połówek od zera: `5/2 = 3`, `-5/2 = -3`.
    ///
    /// Panika przy `den == 0`.
    #[inline]
    #[must_use]
    pub fn div_round_half_up(self, den: i64) -> Money {
        assert!(den != 0, "Money::div_round_half_up: mianownik zerowy");
        div_round_half_away(i128::from(self.0), i128::from(den))
    }
}

/// Wspólny rdzeń zaokrąglania: `round_half_away_from_zero(a / b)`, liczony na modułach,
/// żeby znak nie wpływał na próg zaokrąglenia.
fn div_round_half_away(a: i128, b: i128) -> Money {
    debug_assert!(b != 0);
    let negative = (a < 0) != (b < 0);
    let a_abs = a.unsigned_abs();
    let b_abs = b.unsigned_abs();
    let q = (a_abs * 2 + b_abs) / (b_abs * 2);
    let q = i128::try_from(q).expect("Money: przepełnienie zakresu i128 przy zaokrąglaniu");
    let q = if negative { -q } else { q };
    Money(i64::try_from(q).expect("Money: wynik poza zakresem i64 (grosze)"))
}

/// Podział kwoty między N stron proporcjonalnie do wag.
///
/// **Suma wyniku jest zawsze równa `total`** — to jest cały powód istnienia tej funkcji.
/// Każda strona dostaje część całkowitą swojego udziału (obcinaną do zera), a reszta
/// rozchodzi się po 1 groszu do kolejnych stron o niezerowej wadze, **w kolejności wag**.
/// Kolejność jest kontraktem wywołującego — nigdy nie wolno jej brać z iteracji po mapie
/// (00 §3.2), bo wtedy o tym, kto dostanie dodatkowy grosz, decyduje układ pamięci.
///
/// Przypadki brzegowe:
/// - `weights` puste → panika (nie ma komu wypłacić, a kwota nie może zniknąć),
/// - wszystkie wagi zerowe → całość trafia do pierwszej strony,
/// - `total` ujemny → reszta rozchodzi się po −1 grosz, suma nadal równa `total`.
#[must_use]
pub fn split_proportional(total: Money, weights: &[u64]) -> Vec<Money> {
    assert!(
        !weights.is_empty(),
        "split_proportional: pusta lista wag — kwota nie ma gdzie trafić"
    );

    let weight_sum: u128 = weights.iter().map(|&w| u128::from(w)).sum();
    if weight_sum == 0 {
        let mut out = vec![Money::ZERO; weights.len()];
        out[0] = total;
        return out;
    }

    let total_i = i128::from(total.0);
    let negative = total_i < 0;
    let total_abs = total_i.unsigned_abs();

    let mut out: Vec<Money> = Vec::with_capacity(weights.len());
    let mut assigned: u128 = 0;
    for &w in weights {
        let share = total_abs * u128::from(w) / weight_sum; // obcięcie w dół co do modułu
        assigned += share;
        let share = i128::try_from(share).expect("split_proportional: przepełnienie");
        out.push(Money(
            i64::try_from(if negative { -share } else { share })
                .expect("split_proportional: udział poza zakresem i64"),
        ));
    }

    // Reszta: zawsze < liczba stron, rozchodzi się po jednym groszu.
    let mut remainder = total_abs - assigned;
    let step = if negative { -1 } else { 1 };
    for (out_i, &w) in out.iter_mut().zip(weights) {
        if remainder == 0 {
            break;
        }
        if w == 0 {
            continue;
        }
        out_i.0 += step;
        remainder -= 1;
    }
    debug_assert_eq!(remainder, 0, "split_proportional: reszta nie rozeszła się");

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zaokraglanie_polowek_od_zera() {
        // Tabela przypadków brzegowych: dokładne połówki, znaki, dzielnik ujemny.
        let cases: &[(i64, i64, i64)] = &[
            (5, 2, 3),
            (-5, 2, -3),
            (5, -2, -3),
            (-5, -2, 3),
            (4, 2, 2),
            (3, 2, 2),
            (-3, 2, -2),
            (1, 2, 1),
            (-1, 2, -1),
            (0, 7, 0),
            (1, 3, 0),
            (2, 3, 1),
            (-2, 3, -1),
            (100, 3, 33),
            (-100, 3, -33),
            (150, 100, 2),
            (-150, 100, -2),
            (249, 100, 2),
            (250, 100, 3),
            (i64::MAX, i64::MAX, 1),
            (i64::MIN, 1, i64::MIN),
            (i64::MIN, 2, -4_611_686_018_427_387_904),
        ];
        for &(a, b, expected) in cases {
            assert_eq!(Money(a).div_round_half_up(b), Money(expected), "{a} / {b}");
        }
    }

    #[test]
    fn mul_ratio_liczy_w_i128() {
        assert_eq!(Money(1_000_000).mul_ratio(23, 100), Money(230_000));
        // 19,99 zł * 23% VAT = 4,5977 → 4,60 zł
        assert_eq!(Money(1999).mul_ratio(23, 100), Money(460));
        // Iloczyn przekracza i64, iloraz już nie.
        assert_eq!(
            Money(i64::MAX).mul_ratio(1_000_000, 1_000_000),
            Money(i64::MAX)
        );
        assert_eq!(Money(-1999).mul_ratio(23, 100), Money(-460));
    }

    #[test]
    fn podzial_zawsze_sumuje_sie_do_calosci() {
        let cases: &[(i64, &[u64])] = &[
            (100, &[1, 1, 1]),
            (-100, &[1, 1, 1]),
            (1, &[1, 1, 1, 1]),
            (0, &[5, 3]),
            (99_999, &[7, 11, 13, 0]),
            (-7, &[0, 0, 1]),
            (i64::MAX, &[1, 1]),
        ];
        for &(total, weights) in cases {
            let parts = split_proportional(Money(total), weights);
            let sum: i64 = parts.iter().map(|m| m.0).sum();
            assert_eq!(sum, total, "total={total} weights={weights:?}");
            assert_eq!(parts.len(), weights.len());
            // Strona o wadze zerowej nie dostaje ani grosza.
            for (part, &w) in parts.iter().zip(weights) {
                if w == 0 {
                    assert_eq!(*part, Money::ZERO);
                }
            }
        }
    }

    #[test]
    fn podzial_wszystkie_wagi_zerowe_trafia_do_pierwszego() {
        let parts = split_proportional(Money(13), &[0, 0, 0]);
        assert_eq!(parts, vec![Money(13), Money::ZERO, Money::ZERO]);
    }

    #[test]
    fn reszta_idzie_w_kolejnosci_wag() {
        // 10 gr na trzy równe strony: 4/3/3, nie 3/3/4.
        let parts = split_proportional(Money(10), &[1, 1, 1]);
        assert_eq!(parts, vec![Money(4), Money(3), Money(3)]);
    }

    #[test]
    #[should_panic(expected = "pusta lista wag")]
    fn podzial_bez_stron_panikuje() {
        let _ = split_proportional(Money(1), &[]);
    }
}
