//! Zasoby świata — dane globalne, których nie ma sensu wieszać na encji
//! (M0 §5.6: `Res<T>` / `ResMut<T>`).
//!
//! Zasób jest też jedynym poprawnym miejscem na `Arena<T>` (00 §K-16, ryzyko R-11):
//! scheduler serializuje dostęp do zasobów po `ResourceId`, więc ochrona przed
//! równoległym `Arena::insert` jest strukturalna, a nie regulaminowa.

use magnat_core::collections::{seeded_map, SeededMap};
use std::any::{Any, TypeId};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ResourceId(u16);

impl ResourceId {
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Konstruktor z surowego numeru — dla testów i narzędzi.
    #[inline]
    #[must_use]
    pub const fn from_index(i: usize) -> ResourceId {
        ResourceId(i as u16)
    }
}

struct ResourceSlot {
    /// Nazwa typu — klucz kolejności w hashu stanu i w komunikatach błędów.
    type_name: &'static str,
    value: Box<dyn Any + Send + Sync>,
}

#[derive(Default)]
pub struct Resources {
    slots: Vec<ResourceSlot>,
    by_type: SeededMap<TypeId, ResourceId>,
}

impl Resources {
    #[must_use]
    pub fn new() -> Resources {
        Resources {
            slots: Vec::new(),
            by_type: seeded_map(),
        }
    }

    /// Wstawia zasób. Powtórne wstawienie **nadpisuje** wartość, zachowując `ResourceId` —
    /// inaczej identyfikator w `Access` przestałby być stabilny w trakcie uruchomienia.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> ResourceId {
        let type_id = TypeId::of::<T>();
        if let Some(id) = self.by_type.get(&type_id).copied() {
            self.slots[id.index()].value = Box::new(value);
            return id;
        }
        let id = ResourceId(u16::try_from(self.slots.len()).expect("ponad 65 536 zasobów"));
        self.slots.push(ResourceSlot {
            type_name: std::any::type_name::<T>(),
            value: Box::new(value),
        });
        self.by_type.insert(type_id, id);
        id
    }

    #[must_use]
    pub fn id_of<T: Send + Sync + 'static>(&self) -> Option<ResourceId> {
        self.by_type.get(&TypeId::of::<T>()).copied()
    }

    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        let id = self.id_of::<T>()?;
        self.slots[id.index()].value.downcast_ref::<T>()
    }

    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        let id = self.id_of::<T>()?;
        self.slots[id.index()].value.downcast_mut::<T>()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Nazwy typów zasobów w kolejności rejestracji — konsument: hash stanu,
    /// który sortuje je po nazwie (M0 §5.9).
    pub fn iter_names(&self) -> impl Iterator<Item = (ResourceId, &'static str)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .map(|(i, s)| (ResourceId(i as u16), s.type_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zasob_zachowuje_identyfikator_po_nadpisaniu() {
        let mut r = Resources::new();
        let a = r.insert(7u32);
        let b = r.insert(9u32);
        assert_eq!(a, b, "ResourceId musi być stabilny w obrębie uruchomienia");
        assert_eq!(r.get::<u32>(), Some(&9));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn rozne_typy_dostaja_rozne_identyfikatory() {
        let mut r = Resources::new();
        let a = r.insert(1u32);
        let b = r.insert(1u64);
        assert_ne!(a, b);
        *r.get_mut::<u64>().unwrap() += 41;
        assert_eq!(r.get::<u64>(), Some(&42));
        assert_eq!(r.get::<i8>(), None);
    }
}
