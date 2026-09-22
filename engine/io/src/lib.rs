//! `magnat-io` — hash stanu i snapshot ECS (M0: wersja minimalna).
//!
//! Zakres M0: policzyć odcisk stanu, zapisać świat, wczytać go z powrotem i wywalić
//! się czytelnie, gdy plik pochodzi z innej wersji formatu. Pełne wersjonowanie
//! schematu, migracje i zapis w tle to M12 — tutaj powstaje **kształt**, na którym
//! M12 to zbuduje: lista sekcji z własnym zakresem bajtów, sumą kontrolną i generacją
//! chunka, oraz sekcje `Opaque` przepisywane bez zmian.

#![forbid(unsafe_code)]

pub mod hash;
pub mod snapshot;

pub use hash::{state_hash_parts, world_state_hash, HashState, StateHash, StateHasher};
pub use snapshot::{
    load_world, read_directory, read_section, rewrite_sections, save_world, ArchetypeEntry,
    ArenaPart, Compression, IoError, RawSnapshot, SaveReport, SchemaEntry, SectionKind,
    SnapshotHeader, SnapshotSection, FORMAT_VERSION, SNAPSHOT_MAGIC,
};

/// Makro `impl_hash_state_pod!` mieszka w `magnat-core` i jest tam eksportowane
/// na poziomie crate'a; re-eksport, żeby konsument nie musiał wiedzieć, gdzie leży.
pub use magnat_core::impl_hash_state_pod;
