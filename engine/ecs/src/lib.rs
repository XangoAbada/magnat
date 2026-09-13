//! `magnat-ecs` — archetypowy ECS w układzie SoA, scheduler DAG i bufory komend.
//!
//! Kontrakty: 00 §3.3 (równoległość), §3.4 (mutacje strukturalne), PRD §17.2.
//!
//! **To jedyny crate w projekcie z `unsafe`** (00 §6). Cały `unsafe` siedzi
//! w `chunk.rs` i sprowadza się do trzech operacji: alokacji chunka, rzutowania
//! wycinka kolumny na typ komponentu i przeniesienia wiersza między archetypami.
//! Każdy blok ma komentarz `// SAFETY:` i jest objęty testem pod Miri.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod access;
pub mod archetype;
pub mod chunk;
pub mod component;
pub mod entity_store;
pub mod query;
pub mod resources;
pub mod world;

pub use access::Access;
pub use archetype::{Archetype, ArchetypeId, Archetypes, EntityLocation};
pub use chunk::{ChunkId, ChunkLayout, ChunkRef, CHUNK_TARGET_BYTES};
pub use component::{Component, ComponentId, ComponentInfo, ComponentRegistry};
pub use entity_store::EntityStore;
pub use query::{ChunkView, Query, QueryData, QueryFilter, UnsafeWorldCell, With, Without};
pub use resources::{ResourceId, Resources};
pub use world::{EntityMut, RestoreError, StateHooks, World};

/// `Entity` mieszka w `core` (00 §2, M0 §5.1a) — tutaj tylko re-eksport,
/// żeby dla wywołujących nic się nie zmieniło.
pub use magnat_core::Entity;

pub mod command;
pub mod system;

pub use command::{flush_commands, CommandBuffer, EntityReservation, FlushStats};
pub use system::{
    App, Schedule, ScheduleBuilder, ScheduleError, System, SystemCtx, SystemDesc, SystemId,
};
