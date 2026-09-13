//! `magnat-voxel` — reprezentacja voxelowa świata (M1, właściciel `engine/voxel`).
//!
//! Zakres: chunk 32³, paleta lokalna, składowanie, meshing, agregacja LOD, streaming
//! i kolejka edycji. Crate **nie zna terenu** — teren wchodzi wyłącznie przez trait
//! [`ColumnSource`], którego implementacja mieszka w `sim/world`. Zależność idzie w jedną
//! stronę, od symulacji do silnika, dzięki czemu `engine/voxel` nie ma w drzewie żadnego
//! `sim/*` (reguła `cargo tree` uzgodniona z M11, M1 §6.1).

#![forbid(unsafe_code)]

pub mod chunk;
pub mod material;

pub use chunk::{
    lin, pack, unlin, Chunk, ChunkBuilder, ChunkCoord, ChunkState, ChunkStorage, Palette, Run,
    CHUNK_DIM, CHUNK_HEIGHT_M, CHUNK_SPAN_M, CHUNK_VOXELS, VOXEL_HEIGHT_DM,
};
pub use material::{
    LocalIdx, MaterialError, MaterialFlags, MaterialId, MaterialRegistry, VoxelMaterial,
    MATERIALS_SCHEMA_VERSION,
};

/// Jedyne wejście voxeli do świata (M1 §5.2).
///
/// Implementacja musi być **czysta i deterministyczna** — jest wołana z dowolnego workera,
/// w dowolnej kolejności, i ten sam `(coord, lod)` musi dać ten sam wynik za każdym razem.
/// To nie jest zalecenie: kolejność materializacji chunków zależy od kamery gracza, a kamera
/// nie wchodzi do hasha stanu (00 §4), więc źródło zależne od historii łamałoby determinizm.
pub trait ColumnSource: Send + Sync {
    fn fill_chunk(&self, coord: ChunkCoord, lod: u8, out: &mut ChunkBuilder);
}
