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
//! Zakład, transport, rynek B2B i wydobycie wchodzą w kolejnych podfazach.

#![forbid(unsafe_code)]

pub mod batch;
pub mod catalog;
pub mod cost;
pub mod store;

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
pub use store::{
    BatchDraft, BatchSlice, MassIn, Reservation, StorageSlot, Store, StoreError, WarehouseRole,
};
