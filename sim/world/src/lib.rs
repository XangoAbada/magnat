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
pub mod nav_build;
pub mod noise;
pub mod params;
pub mod pipeline;
pub mod population;
pub mod query;
pub mod terrain;
pub mod traffic_build;

pub use assets::{data_dir, data_path};
pub use city::blocks::{block_adjacency, Block, BlockId, BlockSet};
// M2d — Etap 6: to jest kontrakt dla M3 (mieszkania i stanowiska pracy), M7 (etaty)
// i M11 (bryły, stropy, wejścia). Nazwy z §6 dokumentu fazy.
pub use city::build::{
    BuildReport, Building, BuildingSet, Entrance, EntranceKind, JobRole, JobTable, ShiftId, Unit,
    UnitIdx, UnitKind, UnitOccupant, WageBand, Workplace,
};
pub use city::derive::{BuildParams, Derived, Part, Scope};
pub use city::districts::{District, DistrictKind, DistrictNames, DistrictSet, Toponym};
pub use city::gates::{CityGate, GateKind, GateProfile};
pub use city::grammar::{BuildingGrammar, GrammarError, GrammarId, GrammarSet, Massing, Rule};
pub use city::parcels::{Frontage, Parcel, ParcelOwner, ParcelSet, ParcelStatus, ZoneSpec};
pub use city::rail::RailReport;
pub use city::road::{
    street_lines, FurnitureKind, NodeFlags, NodeId, PolyArena, PolyRef, RoadClass, RoadFlags,
    RoadNetwork, RoadNode, RoadSegment, RoadStructure, SegmentId, StreetFurniture, StreetLine,
};
pub use city::value::{land_value_at, AccessFields, LandValueBreakdown, LandValueFactor, ValueCtx};
// M2e — Etap 7 i wycena `pass_2`: kontrakt dla M5 (ceny startowe, `SiteSeed` sklepów),
// M6 (`SiteSeed.{archetype, recipes, capacity_scale}`, `ClosureReport`) i M7 (`FirmSeed`).
pub use city::catalog::{Catalog, CatalogError, Good, GoodUnit, Recipe, RecipeSource};
pub use city::inspect::{parcel_at, parcel_card};
pub use city::overlay::{OverlaySpec, OverlayTable, OVERLAY_CELL_M};
pub use city::sites::{
    supply_closure_check, Archetype, ClosureReport, FirmSeed, SectorId, SiteArchetypeId,
    SiteCatalog, SiteReport, SiteSeed, SiteSet,
};
pub use city::zoning::{
    CityFields, EpochId, EpochSpec, EpochTable, ResDensity, StyleId, ZoneField, ZoneKind, ZoneMix,
    ZoneResult, ZoningWeights,
};
pub use city::{
    city_hash, generate_city, world_hash_m2, CityData, CityGenError, CityPlan, GenerationReport,
};
pub use climate::ClimateCell;
pub use data::{
    HeightDm, LakeCells, RiverCell, RiverNetwork, RiverSegment, WaterBits, WaterClass, WorldData,
    SEA_LEVEL_DM,
};
pub use deposit::{Deposit, DepositId, DepositLedger, DepositShape};
pub use geology::{ColumnStack, GeologyModel, TerrainLayer};
pub use grid::Grid2;
pub use io::{load_mgw, save_mgw, WorldIoError, MGW_VERSION};
pub use nav_build::{build_nav, NavBuildError, NavBuildReport};
pub use params::{
    Difficulty, EconomyProfile, Epoch, ParamError, Region, WorldGenParams, WorldSize,
    CLIMATE_CELL_M, WORK_CELL_M,
};
pub use pipeline::{generate, GenCtx, GenPass, WorldGenReport, WorldStats, PASSES};
pub use population::{
    generate_population, home_place, site_place, Populated, PopulationError, PopulationParams,
    PopulationReport, COMMUTE_BINS, SITE_KEY_BASE,
};
pub use query::{
    Buildability, Crossing, NavigableClass, ObstacleBitset, TerrainQuery, TileCoord, WaterCell,
    TILE_M,
};
pub use terrain::Terrain;
