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

pub mod climate;
pub mod data;
pub mod deposit;
pub mod fields;
pub mod gen;
pub mod grid;
pub mod io;
pub mod noise;
pub mod params;
pub mod pipeline;

pub use climate::ClimateCell;
pub use data::{
    HeightDm, LakeCells, RiverCell, RiverNetwork, RiverSegment, WaterBits, WaterClass, WorldData,
    SEA_LEVEL_DM,
};
pub use deposit::{Deposit, DepositId, DepositShape};
pub use grid::Grid2;
pub use io::{load_mgw, save_mgw, WorldIoError, MGW_VERSION};
pub use params::{
    Difficulty, EconomyProfile, Epoch, ParamError, Region, WorldGenParams, WorldSize,
    CLIMATE_CELL_M, WORK_CELL_M,
};
pub use pipeline::{generate, GenCtx, GenPass, WorldGenReport, WorldStats, PASSES};
