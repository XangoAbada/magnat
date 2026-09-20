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
//! i kaskada niedoboru (WP6).
//! Zakres M6c: rynek spot (WP7), kontrakty terminowe (WP8), import i eksport (WP9) —
//! czyli strona, która odpowiada na akcje kaskady, zamiast pozwalać zapotrzebowaniu
//! zniknąć. Wydobycie i koniec dostawcy zewnętrznego wchodzą w M6d.

#![forbid(unsafe_code)]

pub mod b2b;
pub mod batch;
pub mod catalog;
pub mod chain;
pub mod cost;
pub mod inventory;
pub mod kernel;
pub mod mining;
pub mod plant;
pub mod shortage;
pub mod store;
pub mod systems;
pub mod trace;
pub mod transport;
pub mod tuning;

pub use b2b::{
    gate_allows, B2b, ContractDelivery, ContractError, ContractPricing, DeliverySchedule,
    Exclusives, ImportQuote, Lock, Penalty, PendingImport, Quote, QuoteId, Rfq, RfqDraft,
    RfqOutcome, SellerIndex, SellerRef, Settlement, SupplyContract, SupplyContractDraft,
    TariffClass, TariffError, TariffTable, TradeError, TradeGood, TradeNode, TradeNodeId,
    WhoTransports, TARIFFS_SCHEMA_VERSION,
};
pub use batch::{
    brand_of, firm_of, Batch, BatchEvent, BatchFlags, BatchId, BatchLedger, BatchLocation,
    BatchOrigin, BrandId, CoalesceKey, LineId, SlotId, TraceKind, TransportOrderId,
};
pub use catalog::{
    load_default, Catalog, CatalogError, CostAllocation, Emissions, Good, GoodForm, GoodSpec,
    GoodUnit, GraphWarning, HazardClass, NeedCategory, OutputKind, PlumeKind, QualityModel, Recipe,
    RecipeInput, RecipeOutput, RecipeSource, RecipeSpec, Setup, StorageClass, Substitute,
    WarningKind, CATEGORIES_SCHEMA_VERSION, GOODS_SCHEMA_VERSION, NEEDS_SCHEMA_VERSION,
    RECIPES_SCHEMA_VERSION,
};
pub use chain::{Chain, ChainHandle, ChainTick};
pub use cost::{allocate_cost, disposal_cost, waste_mass};
pub use inventory::{
    InventoryPolicy, InventoryRule, MinMaxParams, PreferredSource, ReplenishRequest, Review,
};
pub use kernel::throughput;
pub use mining::{Deposits, MiningSite, NoDeposits};
pub use plant::{
    advance_production, BreakCause, Charge, Dock, EmissionTotals, LineState, PlannedRun, Plant,
    PlantSite, ProductionCtx, ProductionLine, ProductionReport, ProductionSchedule, Shift,
    SiteDwellResponse, UtilityBill, UtilityMeter, VehicleArrivedAtSite,
};
pub use shortage::{RfqId, ShortageAction, ShortageStage, ShortageState};
pub use store::{
    BatchDraft, BatchSlice, MassIn, Reservation, ShelfState, Spoiled, StorageSlot, Store,
    StoreError, WarehouseRole, BATCH_HARD_LIMIT, BATCH_SOFT_LIMIT,
};
pub use systems::ChainSystem;
pub use trace::{
    batches_in_role, supply_graph, trace_batch, BatchTrace, SupplyCoverage, SupplyEdge,
    SupplyGraphView, TraceOrigin, TraceStage,
};
pub use transport::{
    body_for_storage, consolidate, BodyType, Carrier, ConsolidationLimits, FailReason,
    FlatRateFreight, FreightOracle, FreightQuote, MilkRun, Transport, TransportOrder,
    TransportOrderState, TransportRequest, VehicleRequirements,
};
pub use tuning::{Tuning, TuningError, TUNING_SCHEMA_VERSION};
