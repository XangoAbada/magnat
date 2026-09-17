//! Pierścień o stałej pojemności — historia, która nie rośnie.
//!
//! Trzech konsumentów w samym M7a (dziennik decyzji 32, rachunek wyniku zakładu 36,
//! kapitał własny 60), więc generyk zarabia na siebie od razu. W `core` nie stoi,
//! bo poza firmami nikt go nie potrzebuje — pierścienie M5 i M6 są wbudowane
//! w swoje struktury i mają inne niezmienniki (`ShopLostSales` zeruje sloty po dobie).

use magnat_core::hash::{HashState, StateHasher};

/// Ostatnie `N` elementów w kolejności wstawiania. Starsze wypadają po cichu —
/// to jest historia do pokazania graczowi, a nie księga.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ring<T, const N: usize> {
    items: Vec<T>,
    head: usize,
}

impl<T, const N: usize> Default for Ring<T, N> {
    fn default() -> Ring<T, N> {
        Ring {
            items: Vec::new(),
            head: 0,
        }
    }
}

impl<T, const N: usize> Ring<T, N> {
    #[must_use]
    pub fn new() -> Ring<T, N> {
        Ring::default()
    }

    pub fn push(&mut self, v: T) {
        if self.items.len() < N {
            self.items.push(v);
        } else {
            self.items[self.head] = v;
            self.head = (self.head + 1) % N;
        }
    }

    /// Od najstarszego do najnowszego. Kolejność jest kontraktem karty inspekcji
    /// i **wejściem do hasha** — gdyby zależała od tego, ile razy pierścień się
    /// przekręcił, dwa przebiegi tego samego ziarna dałyby inny hash.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> {
        let (a, b) = self.items.split_at(self.head.min(self.items.len()));
        b.iter().chain(a.iter())
    }

    /// Ta sama kolejność co [`Ring::iter`], z prawem zapisu. Potrzebne tam, gdzie
    /// wpis domyka się w dwóch krokach — rachunek wyniku zakładu dostaje koszty
    /// przy liście płac, a przychód dopiero przy domknięciu okresu księgowego.
    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut T> {
        let split = self.head.min(self.items.len());
        let (a, b) = self.items.split_at_mut(split);
        b.iter_mut().chain(a.iter_mut())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Najnowszy wpis.
    #[must_use]
    pub fn last(&self) -> Option<&T> {
        self.iter().next_back()
    }
}

impl<T: HashState, const N: usize> HashState for Ring<T, N> {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.items.len() as u32);
        for v in self.iter() {
            v.hash_state(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn przekrecony_pierscien_czyta_sie_od_najstarszego() {
        let mut r: Ring<u32, 3> = Ring::new();
        for i in 1..=5 {
            r.push(i);
        }
        let v: Vec<u32> = r.iter().copied().collect();
        assert_eq!(v, vec![3, 4, 5]);
        assert_eq!(r.last(), Some(&5));
    }

    #[test]
    fn hash_nie_zalezy_od_liczby_przekrecen() {
        // Ten sam widoczny ciąg, dwie różne historie dojścia do niego.
        let mut a: Ring<u32, 3> = Ring::new();
        for i in 1..=5 {
            a.push(i);
        }
        let mut b: Ring<u32, 3> = Ring::new();
        for i in 3..=5 {
            b.push(i);
        }
        let mut ha = StateHasher::new();
        let mut hb = StateHasher::new();
        a.hash_state(&mut ha);
        b.hash_state(&mut hb);
        assert_eq!(ha.finish(), hb.finish());
    }
}
