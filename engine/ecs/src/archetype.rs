//! Archetypy — grupy encji o identycznym zestawie komponentów (M0 §5.5).
//!
//! Kanoniczna tożsamość archetypu to **posortowany rosnąco zestaw `ComponentId`**.
//! Dwa archetypy o tym samym zestawie to ten sam archetyp — zawsze, niezależnie
//! od kolejności, w jakiej komponenty dokładano.

use crate::chunk::{ArchetypeChunk, ChunkLayout, ChunkRef};
use crate::component::{ComponentId, ComponentRegistry};
use magnat_core::collections::{seeded_map, SeededMap};
use magnat_core::Entity;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ArchetypeId(u32);

impl ArchetypeId {
    pub const EMPTY: ArchetypeId = ArchetypeId(0);

    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Gdzie w świecie siedzi wiersz encji.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EntityLocation {
    pub archetype: ArchetypeId,
    pub chunk: u16,
    pub row: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum EdgeKind {
    Add,
    Remove,
}

pub struct Archetype {
    id: ArchetypeId,
    components: Box<[ComponentId]>,
    layout: Arc<ChunkLayout>,
    chunks: Vec<ArchetypeChunk>,
    rows: u32,
    /// Krawędzie grafu przejść: dodanie/usunięcie komponentu → archetyp docelowy.
    /// Cache, żeby `insert`/`remove` nie przeszukiwały rejestru archetypów.
    edges: SeededMap<(ComponentId, EdgeKind), ArchetypeId>,
}

impl Archetype {
    #[inline]
    #[must_use]
    pub fn id(&self) -> ArchetypeId {
        self.id
    }

    #[inline]
    #[must_use]
    pub fn components(&self) -> &[ComponentId] {
        &self.components
    }

    #[inline]
    #[must_use]
    pub fn contains(&self, c: ComponentId) -> bool {
        self.components.binary_search(&c).is_ok()
    }

    #[inline]
    #[must_use]
    pub fn column_index(&self, c: ComponentId) -> Option<usize> {
        self.layout.column_index(c)
    }

    #[inline]
    #[must_use]
    pub fn rows(&self) -> u32 {
        self.rows
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }

    #[inline]
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    #[inline]
    #[must_use]
    pub fn chunk(&self, i: usize) -> &ArchetypeChunk {
        &self.chunks[i]
    }

    #[inline]
    pub fn chunk_mut(&mut self, i: usize) -> &mut ArchetypeChunk {
        &mut self.chunks[i]
    }

    #[inline]
    #[must_use]
    pub fn chunks(&self) -> &[ArchetypeChunk] {
        &self.chunks
    }

    #[inline]
    pub fn chunks_mut(&mut self) -> &mut [ArchetypeChunk] {
        &mut self.chunks
    }

    #[inline]
    #[must_use]
    pub fn layout(&self) -> &Arc<ChunkLayout> {
        &self.layout
    }

    /// Surowe bajty jednej kolumny jednego chunka — wejście sekcji snapshotu.
    /// Bezpieczna droga: `engine/io` nie potrzebuje własnego `unsafe` (00 §6).
    #[must_use]
    pub fn column_bytes(&self, chunk_index: usize, c: ComponentId) -> Option<&[u8]> {
        let chunk = self.chunks.get(chunk_index)?;
        let col = chunk.layout().column_index(c)?;
        // SAFETY: kolumna należy do tego chunka, a wynik jest tylko do odczytu.
        Some(unsafe { chunk.column_bytes(col) })
    }

    /// Bajty zajęte przez chunki tego archetypu — do inspektora i budżetu pamięci.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.chunks.len() * self.layout.alloc_bytes()
    }

    #[must_use]
    pub fn edge(&self, c: ComponentId, kind: EdgeKind) -> Option<ArchetypeId> {
        self.edges.get(&(c, kind)).copied()
    }

    pub fn set_edge(&mut self, c: ComponentId, kind: EdgeKind, target: ArchetypeId) {
        self.edges.insert((c, kind), target);
    }

    /// Rezerwuje wiersz dla encji. Kolumny **nie są** zainicjowane — wywołujący
    /// musi je natychmiast wypełnić.
    ///
    /// # Safety
    /// Wywołujący inicjuje wszystkie kolumny nowego wiersza przed jakimkolwiek odczytem.
    pub unsafe fn allocate_row(&mut self, e: Entity) -> EntityLocation {
        let needs_new = self.chunks.last().is_none_or(ArchetypeChunk::is_full);
        if needs_new {
            self.chunks
                .push(ArchetypeChunk::new(Arc::clone(&self.layout)));
        }
        let chunk_index = self.chunks.len() - 1;
        // SAFETY: warunek inicjalizacji przeniesiony na wywołującego.
        let row = unsafe { self.chunks[chunk_index].push_entity(e) };
        self.rows += 1;
        EntityLocation {
            archetype: self.id,
            chunk: u16::try_from(chunk_index).expect("ponad 65 536 chunków w archetypie"),
            row,
        }
    }

    /// Usuwa wiersz, zasypując dziurę **ostatnim wierszem archetypu**.
    /// Zwraca encję, która się przeniosła, wraz z jej nową lokalizacją.
    ///
    /// `drop_existing == false`: wartości wiersza zostały już przeniesione do innego
    /// archetypu i nie wolno wywoływać na nich destruktorów.
    ///
    /// # Safety
    /// Lokalizacja musi wskazywać istniejący wiersz tego archetypu.
    pub unsafe fn swap_remove(
        &mut self,
        loc: EntityLocation,
        drop_existing: bool,
    ) -> Option<(Entity, EntityLocation)> {
        debug_assert_eq!(loc.archetype, self.id);
        let last_chunk = self.chunks.len() - 1;
        let last_row = self.chunks[last_chunk].len() - 1;

        let moved = if loc.chunk as usize == last_chunk {
            // SAFETY: wiersz istnieje; przeniesienie w obrębie jednego chunka.
            unsafe { self.chunks[last_chunk].swap_remove_row(loc.row, drop_existing) }.map(|e| {
                (
                    e,
                    EntityLocation {
                        archetype: self.id,
                        chunk: loc.chunk,
                        row: loc.row,
                    },
                )
            })
        } else {
            let (head, tail) = self.chunks.split_at_mut(last_chunk);
            let dst = &mut head[loc.chunk as usize];
            let src = &mut tail[0];
            // SAFETY: różne chunki tego samego archetypu, oba wiersze istnieją.
            let e =
                unsafe { ArchetypeChunk::move_last_row_between(dst, loc.row, src, drop_existing) };
            Some((
                e,
                EntityLocation {
                    archetype: self.id,
                    chunk: loc.chunk,
                    row: loc.row,
                },
            ))
        };

        self.rows -= 1;
        let _ = last_row;
        // Pusty chunk na końcu oddaje pamięć: po masowym despawnie archetyp nie ma
        // trzymać alokacji, których nie używa.
        if self.chunks.last().is_some_and(ArchetypeChunk::is_empty) {
            self.chunks.pop();
        }
        moved
    }
}

/// Rejestr archetypów świata.
pub struct Archetypes {
    list: Vec<Archetype>,
    by_components: SeededMap<Box<[ComponentId]>, ArchetypeId>,
    /// Rośnie przy każdym **nowym** archetypie — unieważnia cache dopasowań zapytań.
    version: u32,
}

impl Default for Archetypes {
    fn default() -> Self {
        Archetypes::new()
    }
}

impl Archetypes {
    #[must_use]
    pub fn new() -> Archetypes {
        Archetypes {
            list: Vec::new(),
            by_components: seeded_map(),
            version: 0,
        }
    }

    #[inline]
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    #[inline]
    #[must_use]
    pub fn get(&self, id: ArchetypeId) -> &Archetype {
        &self.list[id.index()]
    }

    #[inline]
    pub fn get_mut(&mut self, id: ArchetypeId) -> &mut Archetype {
        &mut self.list[id.index()]
    }

    pub fn iter(&self) -> impl Iterator<Item = &Archetype> {
        self.list.iter()
    }

    /// Dwa archetypy jednocześnie, do przenoszenia wiersza między nimi.
    /// Panika, gdy `a == b` — to byłby błąd wywołującego, nie stan danych.
    pub fn pair_mut(&mut self, a: ArchetypeId, b: ArchetypeId) -> (&mut Archetype, &mut Archetype) {
        assert_ne!(a, b, "pair_mut na tym samym archetypie");
        let (lo, hi) = (a.index().min(b.index()), a.index().max(b.index()));
        let (head, tail) = self.list.split_at_mut(hi);
        let (first, second) = (&mut head[lo], &mut tail[0]);
        if a.index() < b.index() {
            (first, second)
        } else {
            (second, first)
        }
    }

    /// Znajduje albo tworzy archetyp o zadanym zestawie komponentów.
    /// `components` jest sortowane i odduplikowane na wejściu — kanoniczność
    /// tożsamości nie może zależeć od tego, jak wywołujący ułożył listę.
    pub fn get_or_insert(
        &mut self,
        components: &[ComponentId],
        registry: &ComponentRegistry,
    ) -> ArchetypeId {
        let mut sorted: Vec<ComponentId> = components.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let key: Box<[ComponentId]> = sorted.into_boxed_slice();

        if let Some(id) = self.by_components.get(&key) {
            return *id;
        }
        let id = ArchetypeId(u32::try_from(self.list.len()).expect("ponad 2^32 archetypów"));
        let layout = Arc::new(ChunkLayout::new(&key, registry));
        self.list.push(Archetype {
            id,
            components: key.clone(),
            layout,
            chunks: Vec::new(),
            rows: 0,
            edges: seeded_map(),
        });
        self.by_components.insert(key, id);
        self.version += 1;
        id
    }

    /// Iteracja po wszystkich chunkach w kolejności `(ArchetypeId, index)` —
    /// deterministycznej. Konsument dziś: hash stanu i zapis; w M12: zapis w tle.
    pub fn chunk_refs(&self) -> impl Iterator<Item = ChunkRef<'_>> {
        self.list.iter().flat_map(|a| {
            a.chunks().iter().enumerate().map(move |(i, c)| {
                ChunkRef::new(
                    crate::chunk::ChunkId {
                        archetype: a.id(),
                        index: i as u32,
                    },
                    c,
                )
            })
        })
    }
}
