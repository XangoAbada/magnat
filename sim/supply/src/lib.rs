//! `sim/supply` — łańcuch dostaw (M6).
//!
//! Crate powstaje w M6a i od razu przejmuje **katalog towarów i receptur**, który M2
//! trzymał u siebie (`sim/world::city::catalog`). Powód jest kierunkiem zależności,
//! nie porządkami: katalogu potrzebuje `sim/economy` (od M6d półka sklepu bierze towar
//! stąd, a nie z powietrza), a `sim/economy` nie zależy i nie może zależeć od `sim/world`,
//! który ciągnie za sobą `nav`, `agents`, `traffic` i `voxel`. Odwrotnie natomiast wolno:
//! generator miasta pyta katalog, bo to on sprawdza domknięcie łańcuchów w Etapie 7.
//!
//! `sim/world` re-eksportuje to, co miał (`city::catalog::{Catalog, Good, …}`), więc
//! nazwy z dokumentu M2 nie drgnęły.
//!
//! Zakres M6a: katalog (WP1), partia i magazyn (WP2), receptury i model jakości (WP3).
//! Zakres M6b: zakład fizyczny (WP4), zlecenia transportowe (WP5), polityki zapasów
//! i kaskada niedoboru (WP6). Rynek B2B i wydobycie wchodzą w kolejnych podfazach.

#![forbid(unsafe_code)]

pub mod batch;
pub mod catalog;
pub mod cost;
pub mod inventory;
pub mod plant;
pub mod shortage;
pub mod store;
pub mod transport;
pub mod tuning;

pub use batch::{
    Batch, BatchEvent, BatchFlags, BatchId, BatchLedger, BatchLocation, BatchOrigin, BrandId,
    CoalesceKey, DepositId, LineId, SlotId, TraceKind, TransportOrderId,
};
pub use catalog::{
    load_default, Catalog, CatalogError, CostAllocation, Emissions, Good, GoodForm, GoodSpec,
    GoodUnit, GraphWarning, HazardClass, NeedCategory, OutputKind, PlumeKind, QualityModel, Recipe,
    RecipeInput, RecipeOutput, RecipeSource, RecipeSpec, Setup, StorageClass, Substitute,
    WarningKind, CATEGORIES_SCHEMA_VERSION, GOODS_SCHEMA_VERSION, NEEDS_SCHEMA_VERSION,
    RECIPES_SCHEMA_VERSION,
};
pub use cost::{allocate_cost, disposal_cost, waste_mass};
pub use inventory::{
    InventoryPolicy, InventoryRule, MinMaxParams, PreferredSource, ReplenishRequest, Review,
};
pub use plant::{
    advance_production, BreakCause, Charge, Dock, EmissionTotals, LineState, Plant, PlantSite,
    PlannedRun, ProductionCtx, ProductionLine, ProductionReport, ProductionSchedule, Shift,
    SiteDwellResponse, UtilityMeter, VehicleArrivedAtSite,
};
pub use shortage::{RfqId, ShortageAction, ShortageStage, ShortageState};
pub use store::{
    BatchDraft, BatchSlice, MassIn, Reservation, StorageSlot, Store, StoreError, WarehouseRole,
};
pub use transport::{
    BodyType, Carrier, FailReason, FreightOracle, FreightQuote, Transport, TransportOrder,
    TransportOrderState, TransportRequest, VehicleRequirements,
};
pub use tuning::{Tuning, TuningError, TUNING_SCHEMA_VERSION};
