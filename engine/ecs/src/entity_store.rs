//! Rejestr żywych encji (M0 §5.5).
//!
//! Despawn podbija generację, więc stary uchwyt wygasa na zawsze — nawet gdy indeks
//! zostanie natychmiast wykorzystany ponownie. To jest ta sama zasada co w arenie
//! (00 §K-16) i ten sam powód: cichy odczyt cudzych danych jest gorszy niż błąd.

use crate::archetype::EntityLocation;
use magnat_core::Entity;
use std::num::NonZeroU32;

struct EntityMeta {
    generation: NonZeroU32,
    location: Option<EntityLocation>,
}

const FIRST_GENERATION: NonZeroU32 = NonZeroU32::new(1).unwrap();

#[derive(Default)]
pub struct EntityStore {
    meta: Vec<EntityMeta>,
    /// Lista wolnych indeksów, LIFO. Kolejność zwalniania jest deterministyczna,
    /// bo despawn dzieje się wyłącznie w punktach synchronizacji (00 §3.4).
    free: Vec<u32>,
    alive: u32,
}

impl EntityStore {
    #[must_use]
    pub fn new() -> EntityStore {
        EntityStore::default()
    }

    pub fn alloc(&mut self) -> Entity {
        self.alive += 1;
        if let Some(index) = self.free.pop() {
            let meta = &mut self.meta[index as usize];
            meta.location = None;
            Entity::new(index, meta.generation)
        } else {
            let index = u32::try_from(self.meta.len()).expect("ponad 2^32 encji");
            self.meta.push(EntityMeta {
                generation: FIRST_GENERATION,
                location: None,
            });
            Entity::new(index, FIRST_GENERATION)
        }
    }

    /// Zwalnia encję i podbija jej generację. `false`, gdy uchwyt był już martwy.
    pub fn free(&mut self, e: Entity) -> bool {
        let Some(meta) = self.meta.get_mut(e.index() as usize) else {
            return false;
        };
        if meta.generation.get() != e.generation() {
            return false;
        }
        let next = meta.generation.get().wrapping_add(1);
        meta.generation = NonZeroU32::new(next).unwrap_or(FIRST_GENERATION);
        meta.location = None;
        self.free.push(e.index());
        self.alive -= 1;
        true
    }

    #[must_use]
    pub fn contains(&self, e: Entity) -> bool {
        self.meta
            .get(e.index() as usize)
            .is_some_and(|m| m.generation.get() == e.generation())
    }

    #[must_use]
    pub fn location(&self, e: Entity) -> Option<EntityLocation> {
        let meta = self.meta.get(e.index() as usize)?;
        (meta.generation.get() == e.generation())
            .then_some(meta.location)
            .flatten()
    }

    pub fn set_location(&mut self, e: Entity, loc: EntityLocation) {
        let meta = &mut self.meta[e.index() as usize];
        debug_assert_eq!(meta.generation.get(), e.generation());
        meta.location = Some(loc);
    }

    #[must_use]
    pub fn alive_count(&self) -> u32 {
        self.alive
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.meta.len()
    }

    /// Generacja slotu o danym indeksie — do odczytu snapshotu i inspektora.
    #[must_use]
    pub fn generation_at(&self, index: u32) -> Option<u32> {
        self.meta.get(index as usize).map(|m| m.generation.get())
    }

    /// Zrzut rejestru do zapisu: `(generacja, czy żywa)` dla każdego indeksu
    /// plus lista wolnych indeksów **w kolejności zwalniania**.
    ///
    /// Lista wolnych jest częścią stanu, nie szczegółem implementacji: to ona
    /// decyduje, który indeks dostanie następny `spawn`. Bez jej zapisania wznowienie
    /// z zapisu rozjeżdżałoby się z przebiegiem ciągłym przy pierwszym spawnie (T-D4).
    #[must_use]
    pub fn snapshot(&self) -> (Vec<(u32, bool)>, Vec<u32>) {
        let entries = self
            .meta
            .iter()
            .map(|m| (m.generation.get(), m.location.is_some()))
            .collect();
        (entries, self.free.clone())
    }

    /// Odtworzenie rejestru z zapisu. Lokalizacje ustawia potem `World::restore_rows`.
    pub fn restore(&mut self, entries: &[(u32, bool)], free: &[u32]) {
        self.meta = entries
            .iter()
            .map(|(generation, _)| EntityMeta {
                generation: NonZeroU32::new(*generation).unwrap_or(FIRST_GENERATION),
                location: None,
            })
            .collect();
        self.free = free.to_vec();
        self.alive = u32::try_from(entries.iter().filter(|(_, alive)| *alive).count())
            .expect("ponad 2^32 żywych encji");
    }

    /// Wszystkie żywe encje w kolejności indeksów — deterministyczna podstawa
    /// hasha stanu i zapisu.
    pub fn iter_alive(&self) -> impl Iterator<Item = Entity> + '_ {
        self.meta
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.location.map(|_| Entity::new(i as u32, m.generation)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archetype::ArchetypeId;

    fn loc() -> EntityLocation {
        EntityLocation {
            archetype: ArchetypeId::EMPTY,
            chunk: 0,
            row: 0,
        }
    }

    #[test]
    fn uchwyt_po_despawnie_nie_trafia_w_nowa_encje() {
        // 10^6 cykli: indeks wraca natychmiast, generacja rośnie, stary uchwyt
        // ma pozostać martwy przez cały czas.
        let mut store = EntityStore::new();
        let pierwsza = store.alloc();
        store.set_location(pierwsza, loc());
        assert!(store.free(pierwsza));
        for i in 0..1_000_000u32 {
            let nowa = store.alloc();
            assert_eq!(nowa.index(), pierwsza.index());
            assert!(!store.contains(pierwsza), "stary uchwyt ożył w cyklu {i}");
            assert!(store.contains(nowa));
            assert!(store.free(nowa));
        }
        assert_eq!(store.alive_count(), 0);
        assert_eq!(store.capacity(), 1);
    }

    #[test]
    fn podwojne_zwolnienie_jest_nieszkodliwe() {
        let mut store = EntityStore::new();
        let e = store.alloc();
        assert!(store.free(e));
        assert!(!store.free(e));
        assert_eq!(store.alive_count(), 0);
    }

    #[test]
    fn zywe_encje_ida_po_indeksach() {
        let mut store = EntityStore::new();
        let encje: Vec<Entity> = (0..10).map(|_| store.alloc()).collect();
        for e in &encje {
            store.set_location(*e, loc());
        }
        store.free(encje[3]);
        store.free(encje[7]);
        let widziane: Vec<u32> = store.iter_alive().map(Entity::index).collect();
        assert_eq!(widziane, vec![0, 1, 2, 4, 5, 6, 8, 9]);
    }
}
