//! Kolekcje o deterministycznej kolejności iteracji (00 §3.2).
//!
//! Zakaz iteracji po `std::collections::HashMap` wymaga jednego, wygodnego zamiennika.
//! Nie piszemy własnej tablicy haszującej — `indexmap` daje kolejność wstawiania
//! i wymienny haszer, a koszt wyszukiwania zostaje ten sam.
//!
//! ## Reguła doboru (wiążąca dla faz M1+)
//!
//! - kolejność wstawiania jest sama w sobie deterministyczna (budowa z posortowanego
//!   wejścia) → `SeededMap`,
//! - kolejność ma znaczenie semantyczne (rankingi, sumowanie floatów) → `BTreeMap`
//!   albo `SeededMapExt::iter_sorted()`,
//! - klucz to gęsty indeks (`GoodId`, `DistrictId`) → zwykły `Vec` indeksowany, nie mapa.

use std::hash::{BuildHasher, Hasher};

/// Haszer o **stałym ziarnie wkompilowanym w binarkę**. To jest cała różnica wobec
/// `RandomState`: tam ziarno pochodzi z systemu przy starcie procesu, więc kolejność
/// kubełków (a w konsekwencji kolejność iteracji) różni się między uruchomieniami.
#[derive(Clone, Copy, Default)]
pub struct SeededHasherBuilder;

impl BuildHasher for SeededHasherBuilder {
    type Hasher = SeededHasher;

    #[inline]
    fn build_hasher(&self) -> SeededHasher {
        SeededHasher { state: SEED }
    }
}

const SEED: u64 = 0x4D41_474E_4154_0001; // "MAGNAT" + wersja ziarna

/// FxHash-podobny mieszalnik: tani, wystarczająco dobry dla kluczy całkowitych
/// i krótkich napisów. Nie jest odporny na atak HashDoS — i nie musi być,
/// bo klucze pochodzą z symulacji, nie z sieci.
pub struct SeededHasher {
    state: u64,
}

impl SeededHasher {
    #[inline]
    fn mix(&mut self, v: u64) {
        self.state = (self.state ^ v)
            .wrapping_mul(0x517C_C1B7_2722_0A95)
            .rotate_left(26);
    }
}

impl Hasher for SeededHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            self.mix(u64::from_le_bytes(c.try_into().expect("chunk 8 B")));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rest.len()].copy_from_slice(rest);
            self.mix(u64::from_le_bytes(buf));
        }
    }

    #[inline]
    fn write_u8(&mut self, v: u8) {
        self.mix(u64::from(v));
    }
    #[inline]
    fn write_u16(&mut self, v: u16) {
        self.mix(u64::from(v));
    }
    #[inline]
    fn write_u32(&mut self, v: u32) {
        self.mix(u64::from(v));
    }
    #[inline]
    fn write_u64(&mut self, v: u64) {
        self.mix(v);
    }
    #[inline]
    fn write_usize(&mut self, v: usize) {
        self.mix(v as u64);
    }
}

/// Mapa o deterministycznej kolejności iteracji: kolejność wstawiania,
/// haszer ze stałym ziarnem.
pub type SeededMap<K, V> = indexmap::IndexMap<K, V, SeededHasherBuilder>;
/// Zbiór o deterministycznej kolejności iteracji.
pub type SeededSet<K> = indexmap::IndexSet<K, SeededHasherBuilder>;

#[must_use]
pub fn seeded_map<K, V>() -> SeededMap<K, V> {
    SeededMap::with_hasher(SeededHasherBuilder)
}

#[must_use]
pub fn seeded_set<K>() -> SeededSet<K> {
    SeededSet::with_hasher(SeededHasherBuilder)
}

pub trait SeededMapExt<K, V> {
    /// Iteracja po **posortowanym kluczu** — do użycia wszędzie tam, gdzie wynik wchodzi
    /// do stanu trwałego, a kolejność wstawiania nie jest kontraktem (00 §2).
    /// Sortuje przy każdym wywołaniu: to jest droga operacja i ma taka wyglądać,
    /// żeby nikt nie wołał jej w pętli gorącej.
    fn iter_sorted<'a>(&'a self) -> impl Iterator<Item = (&'a K, &'a V)>
    where
        K: 'a,
        V: 'a;
}

impl<K: Ord + std::hash::Hash + Eq, V> SeededMapExt<K, V> for SeededMap<K, V> {
    fn iter_sorted<'a>(&'a self) -> impl Iterator<Item = (&'a K, &'a V)>
    where
        K: 'a,
        V: 'a,
    {
        let mut out: Vec<(&K, &V)> = self.iter().collect();
        out.sort_unstable_by(|a, b| a.0.cmp(b.0));
        out.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kolejnosc_iteracji_to_kolejnosc_wstawiania() {
        let mut m = seeded_map();
        for k in [7u32, 3, 9, 1] {
            m.insert(k, k * 10);
        }
        let klucze: Vec<u32> = m.keys().copied().collect();
        assert_eq!(klucze, vec![7, 3, 9, 1]);
    }

    #[test]
    fn iter_sorted_porzadkuje_po_kluczu() {
        let mut m = seeded_map();
        for k in [7u32, 3, 9, 1] {
            m.insert(k, k * 10);
        }
        let klucze: Vec<u32> = m.iter_sorted().map(|(k, _)| *k).collect();
        assert_eq!(klucze, vec![1, 3, 7, 9]);
    }

    #[test]
    fn ziarno_jest_stale_miedzy_instancjami() {
        // Dwie mapy zbudowane w tej samej kolejności mają tę samą kolejność iteracji
        // i te same hashe kluczy — to jest warunek powtarzalności między uruchomieniami.
        let h1 = SeededHasherBuilder.hash_one("magnat");
        let h2 = SeededHasherBuilder.hash_one("magnat");
        assert_eq!(h1, h2);
        assert_ne!(h1, SeededHasherBuilder.hash_one("magnab"));
    }

    #[test]
    fn zbior_zachowuje_kolejnosc() {
        let mut s = seeded_set();
        for k in ["b", "a", "c"] {
            s.insert(k);
        }
        assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec!["b", "a", "c"]);
    }
}
