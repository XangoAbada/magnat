//! Wektor o stałej pojemności, bez sterty (M3a §5.3, M3b §5.4).
//!
//! Potrzebny, bo limity w kontraktach fazy są **twarde, nie orientacyjne**: 16 kandydatów
//! w `PlaceProvider::candidates` (ryzyko R6 — planer ma się zmieścić w 10 µs także po
//! dołożeniu użyteczności §6.4 przez M5), 24 sloty w `DayCanvas`, 8 miejsc widocznych
//! z trasy. `SmallVec` tych limitów nie pilnuje — przy przepełnieniu cicho przechodzi
//! na stertę, czyli dokładnie w to, przed czym limit ma bronić.
//!
//! `ponytail:` własne 60 linii zamiast zależności `arrayvec`. Sufit znany i wąski:
//! wyłącznie `T: Copy + Default`, bez `Drop`, bez `insert` w środek. Gdyby kiedyś
//! trzeba było trzymać tu typ z destruktorem, bierzemy `arrayvec` z crates.io —
//! ale wtedy dlatego, że jest potrzebny, a nie na wszelki wypadek.

use magnat_core::{HashState, StateHasher};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ArrayVec<T: Copy + Default, const N: usize> {
    buf: [T; N],
    len: u8,
}

impl<T: Copy + Default, const N: usize> Default for ArrayVec<T, N> {
    fn default() -> Self {
        ArrayVec::new()
    }
}

impl<T: Copy + Default, const N: usize> ArrayVec<T, N> {
    #[must_use]
    pub fn new() -> Self {
        assert!(N <= u8::MAX as usize, "ArrayVec: pojemność > 255");
        ArrayVec {
            buf: [T::default(); N],
            len: 0,
        }
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len() == N
    }

    #[inline]
    #[must_use]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Dopisuje na koniec. Zwraca `false`, gdy pełny — **cicha odmowa jest tu celowa**:
    /// przepełnienie limitu kandydatów nie jest błędem programu, tylko sytuacją, w której
    /// szesnastu wystarczy. Wywołujący, dla którego to ma znaczenie, sprawdza wynik.
    #[inline]
    pub fn push(&mut self, v: T) -> bool {
        if self.is_full() {
            return false;
        }
        let n = self.len();
        self.buf[n] = v;
        self.len += 1;
        true
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.buf[self.len()])
    }

    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Usuwa element, przesuwając ogon — kolejność zostaje zachowana, bo `DayCanvas`
    /// trzyma sloty posortowane po `start_min`.
    pub fn remove(&mut self, i: usize) -> T {
        assert!(i < self.len(), "ArrayVec::remove poza zakresem");
        let n = self.len();
        let v = self.buf[i];
        self.buf.copy_within(i + 1..n, i);
        self.len -= 1;
        v
    }

    /// Wstawia na pozycję `i`, przesuwając ogon. Zwraca `false`, gdy pełny.
    pub fn insert(&mut self, i: usize, v: T) -> bool {
        assert!(i <= self.len(), "ArrayVec::insert poza zakresem");
        if self.is_full() {
            return false;
        }
        let n = self.len();
        self.buf.copy_within(i..n, i + 1);
        self.buf[i] = v;
        self.len += 1;
        true
    }

    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.buf[..self.len()]
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        let n = self.len();
        &mut self.buf[..n]
    }

    #[must_use]
    pub fn from_slice(src: &[T]) -> Self {
        let mut v = ArrayVec::new();
        for item in src.iter().take(N) {
            v.push(*item);
        }
        v
    }
}

impl<T: Copy + Default, const N: usize> std::ops::Deref for ArrayVec<T, N> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T: Copy + Default, const N: usize> std::ops::DerefMut for ArrayVec<T, N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T: Copy + Default + HashState, const N: usize> HashState for ArrayVec<T, N> {
    fn hash_state(&self, h: &mut StateHasher) {
        // Tylko zajęta część: ogon bufora jest `T::default()`, nie stanem.
        self.as_slice().hash_state(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_jest_twardy_a_nie_orientacyjny() {
        let mut v: ArrayVec<u16, 4> = ArrayVec::new();
        for i in 0..4 {
            assert!(v.push(i));
        }
        assert!(v.is_full());
        assert!(
            !v.push(99),
            "przepełnienie przeszło — limit przestał być limitem"
        );
        assert_eq!(v.as_slice(), &[0, 1, 2, 3]);
    }

    #[test]
    fn wstawianie_i_usuwanie_zachowuje_kolejnosc() {
        let mut v: ArrayVec<u16, 8> = ArrayVec::from_slice(&[1, 2, 4, 5]);
        assert!(v.insert(2, 3));
        assert_eq!(v.as_slice(), &[1, 2, 3, 4, 5]);
        assert_eq!(v.remove(0), 1);
        assert_eq!(v.as_slice(), &[2, 3, 4, 5]);
        assert_eq!(v.pop(), Some(5));
        v.clear();
        assert!(v.is_empty());
    }

    #[test]
    fn hash_nie_widzi_ogona_bufora() {
        // Dwa wektory o tej samej treści, ale różnej historii, są tym samym stanem.
        let a: ArrayVec<u16, 8> = ArrayVec::from_slice(&[7, 8]);
        let mut b: ArrayVec<u16, 8> = ArrayVec::from_slice(&[7, 8, 9, 10]);
        b.pop();
        b.pop();
        let odcisk = |v: &ArrayVec<u16, 8>| {
            let mut h = StateHasher::new();
            v.hash_state(&mut h);
            h.finish()
        };
        assert_eq!(odcisk(&a), odcisk(&b));
    }
}
