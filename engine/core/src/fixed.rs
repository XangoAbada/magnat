//! `Fx` — liczba stałoprzecinkowa Q32.32 (00 §2, M0 §5.1).
//!
//! Istnieje dla ułamków w miejscach, w których float jest zabroniony, a `i64`
//! za grubo ziarnisty: udziały, współczynniki, czasy przejazdu wchodzące do stanu
//! trwałego. Reprezentacja: `i64`, 32 bity części całkowitej, 32 bity ułamkowej.
//!
//! Zakres bezpieczny: część całkowita −2^31 ..= 2^31−1. `mul` i `div` liczą w `i128`,
//! więc przepełnia się dopiero **wynik**, nigdy krok pośredni — i wtedy panikuje
//! zamiast po cichu zawinąć.

use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Fx(i64);

impl Fx {
    pub const FRAC_BITS: u32 = 32;
    pub const ONE: Fx = Fx(1 << 32);
    pub const ZERO: Fx = Fx(0);
    pub const MIN: Fx = Fx(i64::MIN);
    pub const MAX: Fx = Fx(i64::MAX);

    #[inline]
    #[must_use]
    pub const fn from_bits(bits: i64) -> Fx {
        Fx(bits)
    }

    #[inline]
    #[must_use]
    pub const fn to_bits(self) -> i64 {
        self.0
    }

    #[inline]
    #[must_use]
    pub const fn from_int(v: i32) -> Fx {
        Fx((v as i64) << Fx::FRAC_BITS)
    }

    /// `num/den` jako Q32.32, zaokrąglane w dół (ku −∞).
    /// Panika przy `den == 0`.
    #[inline]
    #[must_use]
    pub fn from_ratio(num: i32, den: i32) -> Fx {
        assert!(den != 0, "Fx::from_ratio: mianownik zerowy");
        let v = (i128::from(num) << Fx::FRAC_BITS).div_euclid(i128::from(den));
        Fx(i64::try_from(v).expect("Fx::from_ratio: wynik poza zakresem Q32.32"))
    }

    /// Obcięcie ku −∞ (przesunięcie arytmetyczne), nie ku zeru — jedna reguła
    /// dla obu znaków, więc brak niespodzianki przy wartościach ujemnych.
    #[inline]
    #[must_use]
    pub const fn to_int_trunc(self) -> i32 {
        (self.0 >> Fx::FRAC_BITS) as i32
    }

    /// Zaokrąglenie do najbliższej liczby całkowitej, połówki w górę (ku +∞).
    #[inline]
    #[must_use]
    pub const fn to_int_round(self) -> i32 {
        ((self.0 + (1 << 31)) >> Fx::FRAC_BITS) as i32
    }

    /// Mnożenie przez `i128`, wynik obcinany ku −∞. Panika przy przepełnieniu wyniku.
    ///
    /// Nazwa z kontraktu 00 §2. Świadomie **nie** implementujemy `std::ops::Mul`:
    /// operator sugerowałby arytmetykę bez efektów ubocznych, a ta funkcja panikuje
    /// przy przepełnieniu — i ma panikować widocznie.
    #[inline]
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, rhs: Fx) -> Fx {
        let v = (i128::from(self.0) * i128::from(rhs.0)) >> Fx::FRAC_BITS;
        Fx(i64::try_from(v).expect("Fx::mul: przepełnienie Q32.32"))
    }

    /// Dzielenie przez `i128`. Panika przy `rhs == 0` i przy przepełnieniu wyniku.
    /// Jak `mul`: nazwa z kontraktu, bez `std::ops::Div` (patrz komentarz wyżej).
    #[inline]
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn div(self, rhs: Fx) -> Fx {
        assert!(rhs.0 != 0, "Fx::div: dzielenie przez zero");
        let v = (i128::from(self.0) << Fx::FRAC_BITS).div_euclid(i128::from(rhs.0));
        Fx(i64::try_from(v).expect("Fx::div: przepełnienie Q32.32"))
    }

    #[inline]
    #[must_use]
    pub const fn saturating_add(self, rhs: Fx) -> Fx {
        Fx(self.0.saturating_add(rhs.0))
    }

    #[inline]
    #[must_use]
    pub const fn saturating_sub(self, rhs: Fx) -> Fx {
        Fx(self.0.saturating_sub(rhs.0))
    }

    #[inline]
    #[must_use]
    pub const fn neg(self) -> Fx {
        Fx(-self.0)
    }

    #[inline]
    #[must_use]
    pub const fn abs(self) -> Fx {
        Fx(self.0.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn konwersje_calkowite() {
        assert_eq!(Fx::from_int(3).to_int_trunc(), 3);
        assert_eq!(Fx::from_int(-3).to_int_trunc(), -3);
        assert_eq!(Fx::ONE.to_int_round(), 1);
        assert_eq!(Fx::from_ratio(1, 2).to_int_round(), 1); // 0,5 → 1 (połówki w górę)
        assert_eq!(Fx::from_ratio(-1, 2).to_int_round(), 0); // -0,5 → 0
        assert_eq!(Fx::from_ratio(1, 3).to_int_trunc(), 0);
        assert_eq!(Fx::from_ratio(-1, 3).to_int_trunc(), -1); // ku -inf
    }

    #[test]
    fn mnozenie_i_dzielenie_w_zakresie() {
        let a = Fx::from_ratio(3, 4); // 0,75
        let b = Fx::from_int(8);
        assert_eq!(a.mul(b).to_int_round(), 6);
        assert_eq!(b.div(Fx::from_int(2)).to_int_round(), 4);
        assert_eq!(Fx::from_ratio(1, 3).mul(Fx::from_int(3)).to_int_round(), 1);

        // Skraj zadeklarowanego zakresu: 2^29 * 2 = 2^30 mieści się w Q32.32.
        let big = Fx::from_int(1 << 29);
        assert_eq!(big.mul(Fx::from_int(2)).to_int_trunc(), 1 << 30);
    }

    #[test]
    #[should_panic(expected = "przepełnienie")]
    fn mnozenie_poza_zakresem_panikuje() {
        let _ = Fx::from_int(i32::MAX).mul(Fx::from_int(i32::MAX));
    }

    #[test]
    #[should_panic(expected = "dzielenie przez zero")]
    fn dzielenie_przez_zero_panikuje() {
        let _ = Fx::ONE.div(Fx::ZERO);
    }

    #[test]
    fn rozmiar() {
        assert_eq!(size_of::<Fx>(), 8);
    }
}
