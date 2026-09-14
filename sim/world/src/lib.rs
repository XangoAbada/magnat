//! `magnat-world` — generator świata i kontrakt terenu (M1, właściciel terenowej części
//! `sim/world`).
//!
//! Cały ten crate jest **kodem symulacji**, mimo że produkuje dane wizualne: obowiązuje
//! w nim zakaz libm (00 §K-6, `core::det_math`) i zakaz iterowania po `HashMap` (00 §3.2).
//! Teren musi być odtwarzalny bit w bit, bo M2 stawia na nim parcele — rozjazd o jeden
//! decymetr przesuwa granicę działki i psuje zapis gry.
//!
//! Zależności idą w jedną stronę: `sim/world` → `engine/voxel` (implementuje `ColumnSource`),
//! nigdy odwrotnie. `engine/render` nie widzi tego crate'u wcale (M1 §6.1, reguła `cargo tree`).

#![forbid(unsafe_code)]

pub mod assets;
pub mod city;
pub mod climate;
pub mod data;
pub mod deposit;
pub mod fields;
pub mod gen;
pub mod geology;
pub mod grid;
pub mod io;
pub mod noise;
pub mod params;
pub mod pipeline;
pub mod query;
pub mod terrain;

pub use assets::{data_dir, data_path};
pub use city::blocks::{Block, BlockId, BlockSet};
pub use city::gates::{CityGate, GateKind, GateProfile};
pub use city::road::{
    street_lines, FurnitureKind, NodeFlags, NodeId, PolyArena, PolyRef, RoadClass, RoadFlags,
    RoadNetwork, RoadNode, RoadSegment, RoadStructure, SegmentId, StreetFurniture, StreetLine,
};
pub use city::{generate_city, CityData, CityGenError, CityPlan, GenerationReport};
pub use climate::ClimateCell;
pub use data::{
    HeightDm, LakeCells, RiverCell, RiverNetwork, RiverSegment, WaterBits, WaterClass, WorldData,
    SEA_LEVEL_DM,
};
pub use deposit::{Deposit, DepositId, DepositShape};
pub use geology::{ColumnStack, GeologyModel, TerrainLayer};
pub use grid::Grid2;
pub use io::{load_mgw, save_mgw, WorldIoError, MGW_VERSION};
pub use params::{
    Difficulty, EconomyProfile, Epoch, ParamError, Region, WorldGenParams, WorldSize,
    CLIMATE_CELL_M, WORK_CELL_M,
};
pub use pipeline::{generate, GenCtx, GenPass, WorldGenReport, WorldStats, PASSES};
pub use query::{
    Buildability, Crossing, NavigableClass, ObstacleBitset, TerrainQuery, TileCoord, WaterCell,
    TILE_M,
};
pub use terrain::Terrain;
