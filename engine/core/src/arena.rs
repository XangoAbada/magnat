//! `Arena<T>` — generacyjna arena dla kategorii, które **nie są encjami** (00 §K-16).
//!
//! Partii towaru jest rzędu 600 tys., ofert podobnie, jedne i drugie powstają i giną
//! w każdym ticku, a żadna z nich nie jest nigdy odpytywana przekrojowo po archetypach —
//! do partii dociera się przez magazyn, pojazd albo półkę, do oferty przez indeks
//! przestrzenny kategorii. Płacenie za to buforem komend i przenoszeniem wierszy między
//! archetypami to czysty koszt: ECS zarabia na iteracji po komponentach, której tu nie ma.
//!
//! `core` dostarcza sam mechanizm — nie wie, czym jest partia ani oferta.
//! Zawartość definiują M5 (`Offer`) i M6 (`Batch`).
//!
//! **Determinizm.** Arena wchodzi do `world_state_hash` razem z generacjami i mapą
//! zajętości, więc `insert`/`remove` wolno wywoływać wyłącznie w punktach synchronizacji
//! (arena jest zasobem `ResMut<Arena<T>>`, więc scheduler serializuje do niej dostęp).

use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::num::NonZeroU32;

/// Uchwyt do slotu areny. Ten sam kształt co `Entity` (8 B, generacja niezerowa),
/// ale **inny typ** — kompilator nie pozwoli wsadzić `BatchId` tam, gdzie oczekiwana
/// jest encja. Parametr `T` wiąże uchwyt z zawartością: `ArenaHandle<Batch>` nie pasuje
/// do `Arena<Offer>`.
///
/// `PhantomData<fn() -> T>` zamiast `PhantomData<T>`: uchwyt jest `Send + Sync`
/// niezależnie od tego, czy `T` jest.
pub struct ArenaHandle<T> {
    index: u32,
    generation: NonZeroU32,
    _t: PhantomData<fn() -> T>,
}

// Ręczne implementacje zamiast `derive`: `derive` dołożyłoby wymaganie `T: Clone`
// i spółka, a uchwyt jest tylko parą liczb — zawartość areny nie ma z tym nic wspólnego.
impl<T> Clone for ArenaHandle<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for ArenaHandle<T> {}
impl<T> PartialEq for ArenaHandle<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}
impl<T> Eq for ArenaHandle<T> {}
impl<T> PartialOrd for ArenaHandle<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for ArenaHandle<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.index, self.generation).cmp(&(other.index, other.generation))
    }
}
impl<T> std::hash::Hash for ArenaHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}
impl<T> std::fmt::Debug for ArenaHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ArenaHandle({}v{})", self.index, self.generation)
    }
}

impl<T> ArenaHandle<T> {
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

    #[inline]
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        ((self.generation.get() as u64) << 32) | self.index as u64
    }

    #[inline]
    #[must_use]
    pub const fn from_bits(bits: u64) -> Option<ArenaHandle<T>> {
        match NonZeroU32::new((bits >> 32) as u32) {
            Some(generation) => Some(ArenaHandle {
                index: bits as u32,
                generation,
                _t: PhantomData,
            }),
            None => None,
        }
    }
}

/// Uchwyt do slotu areny **bez typu zawartości** — adres, a nie dostęp.
///
/// Powstał dla [`crate::Subject`] (`K-62`): karta inspekcji musi umieć wskazać partię
/// towaru i ofertę, a te dwie kategorie są arenami (`K-16`) parametryzowanymi typem,
/// którego `core` nie zna i znać nie ma — `Batch` należy do `sim/supply`, `Offer`
/// do `sim/economy`. Zatarcie parametru jest tańsze niż przeniesienie obu struktur
/// do `core` i uczciwsze niż surowe `u64` w publicznym enumie, bo nazywa moment
/// przejścia granicy: [`ArenaRef::of`] po jednej stronie, [`ArenaRef::to_handle`]
/// po drugiej.
///
/// Zatarcie typu **nie osłabia bezpieczeństwa uchwytu**: generacja dalej odróżnia
/// slot ponownie użyty od tego samego slotu sprzed zwolnienia, a `Arena::get` zwraca
/// `None`, gdy się nie zgadza. Pomylenie areny (uchwyt partii użyty jako uchwyt oferty)
/// przestaje być błędem kompilacji i staje się pustą kartą — dlatego `to_handle`
/// woła się **wyłącznie** w miejscu, które wie, o którą arenę pyta.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ArenaRef {
    index: u32,
    generation: NonZeroU32,
}

impl ArenaRef {
    #[inline]
    #[must_use]
    pub const fn of<T>(h: ArenaHandle<T>) -> ArenaRef {
        ArenaRef {
            index: h.index,
            generation: h.generation,
        }
    }

    /// Przywraca parametr typu. Wołający odpowiada za to, że to ta arena.
    #[inline]
    #[must_use]
    pub const fn to_handle<T>(self) -> ArenaHandle<T> {
        ArenaHandle {
            index: self.index,
            generation: self.generation,
            _t: PhantomData,
        }
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
}

impl<T> From<ArenaHandle<T>> for ArenaRef {
    #[inline]
    fn from(h: ArenaHandle<T>) -> ArenaRef {
        ArenaRef::of(h)
    }
}

enum Slot<T> {
    Occupied { generation: NonZeroU32, value: T },
    Vacant { generation: NonZeroU32 },
}

impl<T> Slot<T> {
    #[inline]
    fn generation(&self) -> NonZeroU32 {
        match self {
            Slot::Occupied { generation, .. } | Slot::Vacant { generation } => *generation,
        }
    }
}

/// Arena generacyjna: sloty o stabilnych indeksach, swobodna lista wolnych slotów,
/// iteracja **zawsze** w kolejności indeksów.
///
/// Kompaktowanie nie istnieje i nie powstanie — przesunięcie wartości między slotami
/// unieważniłoby wszystkie uchwyty trzymane w komponentach ECS. Puste sloty odzyskuje
/// swobodna lista, nie przenoszenie danych.
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>, // LIFO; zwolnienia dzieją się w punktach synchronizacji
    live: u32,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Arena::new()
    }
}

const FIRST_GENERATION: NonZeroU32 = NonZeroU32::new(1).unwrap();

impl<T> Arena<T> {
    #[must_use]
    pub const fn new() -> Arena<T> {
        Arena {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    #[must_use]
    pub fn with_capacity(n: usize) -> Arena<T> {
        Arena {
            slots: Vec::with_capacity(n),
            free: Vec::new(),
            live: 0,
        }
    }

    pub fn insert(&mut self, value: T) -> ArenaHandle<T> {
        self.live += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            let generation = slot.generation();
            *slot = Slot::Occupied { generation, value };
            ArenaHandle {
                index,
                generation,
                _t: PhantomData,
            }
        } else {
            let index = u32::try_from(self.slots.len()).expect("Arena: ponad 2^32 slotów");
            self.slots.push(Slot::Occupied {
                generation: FIRST_GENERATION,
                value,
            });
            ArenaHandle {
                index,
                generation: FIRST_GENERATION,
                _t: PhantomData,
            }
        }
    }

    /// Zwalnia slot i **podbija jego generację**. Stary uchwyt przestaje być ważny
    /// na zawsze — kolejny `insert` w ten sam slot dostaje nową generację.
    pub fn remove(&mut self, h: ArenaHandle<T>) -> Option<T> {
        let slot = self.slots.get_mut(h.index as usize)?;
        match slot {
            Slot::Occupied { generation, .. } if *generation == h.generation => {
                // Generacja rośnie o 1 z zawinięciem omijającym zero (generacja jest niezerowa).
                let next = generation.get().wrapping_add(1);
                let next = NonZeroU32::new(next).unwrap_or(FIRST_GENERATION);
                let old = std::mem::replace(slot, Slot::Vacant { generation: next });
                self.free.push(h.index);
                self.live -= 1;
                match old {
                    Slot::Occupied { value, .. } => Some(value),
                    Slot::Vacant { .. } => unreachable!(),
                }
            }
            _ => None,
        }
    }

    /// `None`, jeśli slot jest wolny **albo** generacja się nie zgadza. Nigdy nie zwraca
    /// cudzej wartości z ponownie użytego slotu — to jest cały powód istnienia generacji.
    #[inline]
    pub fn get(&self, h: ArenaHandle<T>) -> Option<&T> {
        match self.slots.get(h.index as usize)? {
            Slot::Occupied { generation, value } if *generation == h.generation => Some(value),
            _ => None,
        }
    }

    #[inline]
    pub fn get_mut(&mut self, h: ArenaHandle<T>) -> Option<&mut T> {
        match self.slots.get_mut(h.index as usize)? {
            Slot::Occupied { generation, value } if *generation == h.generation => Some(value),
            _ => None,
        }
    }

    #[inline]
    pub fn contains(&self, h: ArenaHandle<T>) -> bool {
        self.get(h).is_some()
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.live as usize
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Liczba slotów łącznie z wolnymi — rozmiar przestrzeni indeksów, nie liczba wartości.
    /// Konsument: hash stanu i snapshot (00 §K-16).
    #[inline]
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Iteracja w kolejności indeksów slotów, z pominięciem wolnych. Deterministyczna
    /// z definicji — to jedyna dopuszczona iteracja po arenie w kodzie symulacji.
    pub fn iter(&self) -> impl Iterator<Item = (ArenaHandle<T>, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| match slot {
                Slot::Occupied { generation, value } => Some((
                    ArenaHandle {
                        index: i as u32,
                        generation: *generation,
                        _t: PhantomData,
                    },
                    value,
                )),
                Slot::Vacant { .. } => None,
            })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (ArenaHandle<T>, &mut T)> {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(i, slot)| match slot {
                Slot::Occupied { generation, value } => Some((
                    ArenaHandle {
                        index: i as u32,
                        generation: *generation,
                        _t: PhantomData,
                    },
                    value,
                )),
                Slot::Vacant { .. } => None,
            })
    }

    /// Wszystkie sloty, także wolne: `(indeks, generacja, wartość)`.
    /// Istnieje dla hasha stanu i snapshotu — dwa światy o tej samej treści, ale różnym
    /// układzie slotów, **muszą** dać różny hash (00 §K-16, T-D12).
    pub fn iter_all_slots(&self) -> impl Iterator<Item = (u32, u32, Option<&T>)> {
        self.slots.iter().enumerate().map(|(i, slot)| match slot {
            Slot::Occupied { generation, value } => (i as u32, generation.get(), Some(value)),
            Slot::Vacant { generation } => (i as u32, generation.get(), None),
        })
    }

    /// Chunki slotów do równoległej iteracji przez `JobPool` — podział po indeksie,
    /// więc redukcja składa się deterministycznie (00 §3.3).
    pub fn chunks(&self, rows: usize) -> impl Iterator<Item = ArenaChunk<'_, T>> {
        assert!(rows > 0, "Arena::chunks: rozmiar chunka musi być dodatni");
        self.slots
            .chunks(rows)
            .enumerate()
            .map(move |(i, slots)| ArenaChunk {
                first_index: (i * rows) as u32,
                slots,
            })
    }
}

/// Widok na spójny zakres slotów areny. Jednostka równoległości (00 §3.3).
pub struct ArenaChunk<'a, T> {
    first_index: u32,
    slots: &'a [Slot<T>],
}

impl<'a, T> ArenaChunk<'a, T> {
    #[inline]
    #[must_use]
    pub fn first_index(&self) -> u32 {
        self.first_index
    }

    pub fn iter(&self) -> impl Iterator<Item = (ArenaHandle<T>, &'a T)> + '_ {
        let first = self.first_index;
        self.slots
            .iter()
            .enumerate()
            .filter_map(move |(i, slot)| match slot {
                Slot::Occupied { generation, value } => Some((
                    ArenaHandle {
                        index: first + i as u32,
                        generation: *generation,
                        _t: PhantomData,
                    },
                    value,
                )),
                Slot::Vacant { .. } => None,
            })
    }
}

/// Rejestr aren w świecie — żeby hash i snapshot wiedziały, co mają objąć,
/// bez zgadywania i bez zależności `core` → `sim`.
/// Wartości są **wieczne**, tak jak `StreamId`: M5 = `Offers`, M6 = `Batches`,
/// kolejne fazy dopisują na końcu (00 §K-16, D-12).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[repr(u16)]
pub enum ArenaKind {
    Offers = 1,
    Batches = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rozmiar_uchwytu() {
        assert_eq!(size_of::<ArenaHandle<u64>>(), 8);
        assert_eq!(size_of::<Option<ArenaHandle<u64>>>(), 8);
    }

    #[test]
    fn uchwyt_zwolnionego_slotu_nigdy_nie_wraca() {
        // 10^6 cykli insert/remove/insert na tym samym slocie: stary uchwyt musi
        // pozostać martwy, nawet gdy slot obsłużył milion kolejnych wartości.
        let mut arena: Arena<u32> = Arena::new();
        let first = arena.insert(0);
        arena.remove(first);
        for i in 1..1_000_000u32 {
            let h = arena.insert(i);
            assert_eq!(h.index(), first.index(), "slot powinien się recyklingować");
            assert_eq!(arena.get(first), None, "stary uchwyt ożył w cyklu {i}");
            assert_eq!(arena.get(h), Some(&i));
            arena.remove(h);
        }
        assert_eq!(arena.len(), 0);
        assert_eq!(arena.slot_count(), 1);
    }

    #[test]
    fn iteracja_idzie_po_indeksach_mimo_dziur() {
        let mut arena: Arena<u32> = Arena::new();
        let handles: Vec<_> = (0..10).map(|i| arena.insert(i)).collect();
        for h in handles.iter().step_by(3) {
            arena.remove(*h);
        }
        let widziane: Vec<u32> = arena.iter().map(|(h, _)| h.index()).collect();
        let mut posortowane = widziane.clone();
        posortowane.sort_unstable();
        assert_eq!(widziane, posortowane);
        // Usunięto sloty 0, 3, 6, 9 — zostaje sześć wartości, dziury nie przesuwają reszty.
        assert_eq!(arena.len(), 6);
        assert_eq!(widziane, vec![1, 2, 4, 5, 7, 8]);
    }

    #[test]
    fn wszystkie_sloty_widac_razem_z_generacja() {
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(10);
        let _b = arena.insert(20);
        arena.remove(a);
        let sloty: Vec<(u32, u32, Option<u32>)> = arena
            .iter_all_slots()
            .map(|(i, g, v)| (i, g, v.copied()))
            .collect();
        assert_eq!(sloty, vec![(0, 2, None), (1, 1, Some(20))]);
    }

    #[test]
    fn chunki_pokrywaja_cala_arene() {
        let mut arena: Arena<u32> = Arena::new();
        for i in 0..100 {
            arena.insert(i);
        }
        let suma: u32 = arena
            .chunks(16)
            .flat_map(|c| c.iter().map(|(_, v)| *v).collect::<Vec<_>>())
            .sum();
        assert_eq!(suma, (0..100).sum());
        assert_eq!(arena.chunks(16).count(), 7);
    }

    #[test]
    fn remove_z_cudzym_uchwytem_nic_nie_robi() {
        let mut a: Arena<u32> = Arena::new();
        let h = a.insert(1);
        assert_eq!(a.remove(h), Some(1));
        assert_eq!(a.remove(h), None);
        assert_eq!(a.len(), 0);
    }
}
