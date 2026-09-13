//! Hash stanu — podstawa testów determinizmu (00 §3.6, M0 §5.9).
//!
//! **Odstępstwo od planu M0 §5.9, świadome:** `StateHasher`/`HashState` mieszkają
//! w `core`, nie w `engine/io`. Powód jest ten sam co przy `Entity` (§5.1a): rejestr
//! komponentów w `engine/ecs` musi umieć zahaszować komponent, którego typu nie zna,
//! a `ecs` nie może zależeć od `io` (to `io` zależy od `ecs`). Gdyby trait siedział
//! w `io`, funkcja haszująca nie miałaby jak trafić do rejestru inaczej niż przez
//! rejestrację po stronie `io` — czyli przez drugi, równoległy rejestr komponentów.
//! `magnat-io` re-eksportuje te typy, więc nazwy z kontraktu §6.5 nadal się zgadzają.
//!
//! Zasada: **float wchodzi do hasha wyłącznie przez `to_bits()` z kanonizacją NaN.**
//! Nie ma implementacji `HashState` dla `f32`/`f64`, żeby autor komponentu musiał
//! podjąć tę decyzję świadomie.

use crate::arena::{Arena, ArenaHandle};
use crate::entity::Entity;
use crate::types::{
    DistrictId, Energy, GoodId, JobRoleId, Mass, Money, Mood, Qty, RecipeId, SimInstant, SimMinute,
    Tick, Volume, Q,
};
use xxhash_rust::xxh3::Xxh3;

/// 128-bitowy odcisk stanu. XXH3 — szybki i o dobrym rozproszeniu; nie jest funkcją
/// kryptograficzną i nie musi być, bo broni przed przypadkiem, nie przed napastnikiem.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct StateHash(pub u128);

impl std::fmt::Display for StateHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl std::str::FromStr for StateHash {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u128::from_str_radix(s.trim(), 16).map(StateHash)
    }
}

#[derive(Default)]
pub struct StateHasher {
    inner: Xxh3,
}

impl StateHasher {
    #[must_use]
    pub fn new() -> StateHasher {
        StateHasher::default()
    }

    #[inline]
    pub fn write(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }

    #[inline]
    pub fn write_u8(&mut self, v: u8) {
        self.write(&[v]);
    }

    #[inline]
    pub fn write_u16(&mut self, v: u16) {
        self.write(&v.to_le_bytes());
    }

    #[inline]
    pub fn write_u32(&mut self, v: u32) {
        self.write(&v.to_le_bytes());
    }

    #[inline]
    pub fn write_u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }

    #[inline]
    pub fn write_i64(&mut self, v: i64) {
        self.write(&v.to_le_bytes());
    }

    /// Float **tylko** tędy: bity zamiast wartości, a każdy NaN sprowadzony
    /// do jednej postaci. Inaczej dwa przebiegi o tym samym stanie logicznym
    /// dawałyby różne hashe, gdy jeden wyprodukował NaN o innym ładunku.
    #[inline]
    pub fn write_f64_bits(&mut self, v: f64) {
        let bits = if v.is_nan() {
            0x7FF8_0000_0000_0000
        } else if v == 0.0 {
            0 // −0.0 i +0.0 to ten sam stan
        } else {
            v.to_bits()
        };
        self.write_u64(bits);
    }

    #[must_use]
    pub fn finish(self) -> StateHash {
        StateHash(self.inner.digest128())
    }
}

/// Każdy komponent trwały implementuje to jawnie (00 §3.6).
/// Brak blanket impl dla `f32`/`f64` — patrz nagłówek modułu.
pub trait HashState {
    fn hash_state(&self, h: &mut StateHasher);
}

/// Dla typów będących czystym POD bez paddingu i bez floatów — jedna linia
/// zamiast boilerplate'u.
#[macro_export]
macro_rules! impl_hash_state_pod {
    ($($t:ty),* $(,)?) => {$(
        impl $crate::hash::HashState for $t {
            #[inline]
            fn hash_state(&self, h: &mut $crate::hash::StateHasher) {
                h.write(&self.to_le_bytes());
            }
        }
    )*};
}

impl_hash_state_pod!(u8, u16, u32, u64, i8, i16, i32, i64, u128, i128);

impl HashState for bool {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(u8::from(*self));
    }
}

impl HashState for Entity {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.to_bits());
    }
}

impl<T> HashState for ArenaHandle<T> {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.to_bits());
    }
}

impl<T: HashState> HashState for Option<T> {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            None => h.write_u8(0),
            Some(v) => {
                h.write_u8(1);
                v.hash_state(h);
            }
        }
    }
}

impl<T: HashState> HashState for [T] {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.len() as u64);
        for v in self {
            v.hash_state(h);
        }
    }
}

impl<T: HashState> HashState for Vec<T> {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        self.as_slice().hash_state(h);
    }
}

macro_rules! impl_hash_state_newtype {
    ($($t:ty),* $(,)?) => {$(
        impl HashState for $t {
            #[inline]
            fn hash_state(&self, h: &mut StateHasher) {
                self.0.hash_state(h);
            }
        }
    )*};
}

impl_hash_state_newtype!(
    Money, SimMinute, SimInstant, Tick, Mass, Volume, Energy, Qty, DistrictId, GoodId, RecipeId,
    JobRoleId
);

impl HashState for Q {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.get());
    }
}

impl HashState for Mood {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.get() as u8);
    }
}

impl HashState for crate::decision::DecisionReason {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.discriminant());
    }
}

impl<T: HashState> Arena<T> {
    /// Hash areny: **sloty, generacje i znacznik zajętości**, w kolejności indeksów
    /// (00 §K-16, test T-D12).
    ///
    /// Nie sama zawartość: dwa przebiegi o identycznej treści partii, ale różnym
    /// układzie slotów, muszą dać różny hash, bo są różnymi stanami — inaczej rozjazd
    /// układu areny przeszedłby przez CI niezauważony i wypłynął dopiero jako rozjazd
    /// cen dwadzieścia tysięcy ticków później.
    pub fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.slot_count() as u64);
        for (index, generation, value) in self.iter_all_slots() {
            h.write_u32(index);
            h.write_u32(generation);
            match value {
                Some(v) => {
                    h.write_u8(1);
                    v.hash_state(h);
                }
                None => h.write_u8(0),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_jest_kanonizowany() {
        let mut a = StateHasher::new();
        a.write_f64_bits(f64::NAN);
        let mut b = StateHasher::new();
        b.write_f64_bits(f64::from_bits(0x7FF8_0000_0000_0001));
        assert_eq!(a.finish(), b.finish());
    }

    #[test]
    fn zera_o_roznym_znaku_to_ten_sam_stan() {
        let mut a = StateHasher::new();
        a.write_f64_bits(0.0);
        let mut b = StateHasher::new();
        b.write_f64_bits(-0.0);
        assert_eq!(a.finish(), b.finish());
    }

    #[test]
    fn zmiana_jednego_bajtu_zmienia_hash() {
        let mut a = StateHasher::new();
        Money(1_000).hash_state(&mut a);
        let mut b = StateHasher::new();
        Money(1_001).hash_state(&mut b);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn arena_o_tej_samej_tresci_ale_innym_ukladzie_ma_inny_hash() {
        // T-D12: arena zbudowana wprost kontra arena po cyklu insert/remove/insert.
        let mut wprost: Arena<u32> = Arena::new();
        wprost.insert(10);
        wprost.insert(20);

        let mut po_cyklu: Arena<u32> = Arena::new();
        let a = po_cyklu.insert(99);
        po_cyklu.insert(20);
        po_cyklu.remove(a);
        po_cyklu.insert(10); // ten sam zestaw wartości, inny układ generacji

        let mut h1 = StateHasher::new();
        wprost.hash_state(&mut h1);
        let mut h2 = StateHasher::new();
        po_cyklu.hash_state(&mut h2);
        assert_ne!(
            h1.finish(),
            h2.finish(),
            "różne stany areny dały ten sam hash"
        );
    }

    #[test]
    fn hash_stanu_jest_powtarzalny() {
        let odcisk = || {
            let mut h = StateHasher::new();
            for i in 0..1000u64 {
                Money(i as i64).hash_state(&mut h);
                Some(Tick(i)).hash_state(&mut h);
            }
            h.finish()
        };
        assert_eq!(odcisk(), odcisk());
    }

    #[test]
    fn hash_ma_reprezentacje_tekstowa_w_obie_strony() {
        let h = StateHash(0x0123_4567_89AB_CDEF_0123_4567_89AB_CDEF);
        let tekst = h.to_string();
        assert_eq!(tekst.len(), 32);
        assert_eq!(tekst.parse::<StateHash>().unwrap(), h);
    }
}
