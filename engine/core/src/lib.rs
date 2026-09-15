//! `magnat-core` — typy bazowe, determinizm i słowniki domenowe całego projektu.
//!
//! Ten crate nie zależy od niczego z `engine/*` ani `sim/*` i nigdy nie będzie —
//! to warunek rozcięcia cyklu zależności opisanego w 00 §K-8. Nie ma tu logiki
//! domenowej: `core` zna słowa, a nie zdania.
//!
//! Kontrakty, które ten crate implementuje:
//! - 00 §2 — typy bazowe, pieniądz w `i64`, `Fx`,
//! - 00 §3.1–3.2 — RNG ze strumieniami, kolekcje deterministyczne,
//! - 00 §4, §K-1, §K-15 — kalendarz 360-dniowy, `Cadence`, tydzień,
//! - 00 §K-6 — `det_math`, zakaz libm,
//! - 00 §K-8 — słowniki domenowe,
//! - 00 §K-12 — `DecisionReason`,
//! - 00 §K-16 — `Arena<T>`.

#![forbid(unsafe_code)]

pub mod arena;
pub mod assets;
pub mod collections;
pub mod decision;
pub mod det_math;
pub mod entity;
pub mod fixed;
pub mod geom;
pub mod hash;
pub mod ids;
pub mod money;
pub mod rng;
pub mod schema;
pub mod time;
pub mod types;
pub mod vocab;
pub mod weather;

pub use arena::{Arena, ArenaChunk, ArenaHandle, ArenaKind};
pub use assets::{data_dir, data_path};
pub use collections::{seeded_map, seeded_set, SeededMap, SeededMapExt, SeededSet};
pub use decision::DecisionReason;
pub use entity::Entity;
pub use fixed::Fx;
pub use geom::{IAabb3, IRect, IVec2, IVec3};
pub use hash::{HashState, StateHash, StateHasher};
pub use ids::{
    BuildingId, CitizenId, ContractId, FirmId, HouseholdId, ParcelId, SiteId, VehicleId,
};
pub use money::split_proportional;
pub use rng::{mix64, rng, Rng, StreamId, NO_ENTITY};
pub use schema::ComponentSchemaId;
pub use time::{Cadence, DayOfWeek, MinuteOfDay, SimCalendar, SimClock, SimSpeed};
pub use types::{
    DistrictId, Energy, GoodId, JobRoleId, Mass, Money, Mood, Qty, RecipeId, SimInstant, SimMinute,
    Tick, Volume, Q,
};
pub use weather::{weather_at, Weather};
pub use vocab::{
    ActivityKind, Biome, CommitmentKind, DeprivationEffect, LifeEventKind, MigrationKind, NeedKind,
    PlaceKind, PlaceRef, RejectCause, ResourceKind, RoadClass, StockCat, TraitId, TransportMode,
    UtilityKind, UtilityService, WorldCoord, NEED_COUNT, REJECT_CAUSE_COUNT, STOCK_CAT_COUNT,
};
