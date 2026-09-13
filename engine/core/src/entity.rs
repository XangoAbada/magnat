//! `Entity` — uchwyt do encji ECS (00 §2, M0 §5.1a, §5.5).
//!
//! Mieszka w `core`, a nie w `ecs`, bo `CitizenId(pub Entity)` z 00 §2 i `PlaceRef`
//! z K-8 są typami `core`. `ecs` re-eksportuje — to jedyna poprawna strona tej relacji,
//! odwrotna dałaby zależność `core` → `ecs`.

use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// 8 bajtów. Generacja niezerowa, więc `size_of::<Option<Entity>>() == 8`.
/// `Ord` po `(index, generation)` — to jest porządek sortowania komend (00 §3.4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct Entity {
    index: u32,
    generation: NonZeroU32,
}

impl Entity {
    /// Konstruktor dla `ecs` (rejestr encji) i odczytu snapshotu.
    /// Generacja zerowa nie istnieje — jest kodowana jako `None` w `Option<Entity>`.
    #[inline]
    #[must_use]
    pub const fn new(index: u32, generation: NonZeroU32) -> Entity {
        Entity { index, generation }
    }

    #[inline]
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[inline]
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation.get()
    }

    /// Płaska reprezentacja do serializacji i kluczy. Odwracalna przez `from_bits`.
    /// Układ: `generation` w starszych 32 bitach, `index` w młodszych — dzięki temu
    /// porządek bitowy **nie** jest porządkiem `Ord` i nikt przez pomyłkę nie sortuje
    /// encji po `to_bits`, dostając inną kolejność niż komendy (00 §3.4).
    #[inline]
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        ((self.generation.get() as u64) << 32) | self.index as u64
    }

    #[inline]
    #[must_use]
    pub const fn from_bits(bits: u64) -> Option<Entity> {
        match NonZeroU32::new((bits >> 32) as u32) {
            Some(generation) => Some(Entity {
                index: bits as u32,
                generation,
            }),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(index: u32, generation: u32) -> Entity {
        Entity::new(index, NonZeroU32::new(generation).unwrap())
    }

    #[test]
    fn rozmiar_i_nisza() {
        assert_eq!(size_of::<Entity>(), 8);
        assert_eq!(size_of::<Option<Entity>>(), 8);
    }

    #[test]
    fn bity_sa_odwracalne() {
        let x = e(123_456, 7);
        assert_eq!(Entity::from_bits(x.to_bits()), Some(x));
        assert_eq!(Entity::from_bits(0), None);
    }

    #[test]
    fn porzadek_po_indeksie_potem_generacji() {
        assert!(e(1, 9) < e(2, 1));
        assert!(e(1, 1) < e(1, 2));
    }
}
