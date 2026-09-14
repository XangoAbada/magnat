//! `magnat-agents` — mieszkańcy: stan, potrzeby, zdarzenia i punkty rozszerzenia (M3).
//!
//! Podfaza M3a kładzie warstwę, na której stoi cała reszta fazy:
//! - **komponenty SoA i magazyny** (`components`, `store`) — kształt mieszkańca
//!   i rachunek pamięci 400 B na osobę (§17.7),
//! - **potrzeby** (`needs`) — spadek liczony z czasu absolutnego, więc shardowanie
//!   go nie zmienia,
//! - **DES** (`des`) — koło czasu, porządek totalny zdarzeń, lazy scheduling,
//! - **punkty rozszerzenia** (`places`) — `PlaceProvider` i `TravelOracle`
//!   z implementacjami tymczasowymi, które M5 i M4 podmieniają bez ruszania planera.
//!
//! Czego tu nie ma i gdzie to jest: planer dnia i ruch pieszy po sieci ulic — M3b;
//! gospodarstwa domowe, demografia, relacje i plotka — M3c; generacja populacji
//! i UI — M3d.
//!
//! Cały crate jest **kodem symulacji**: obowiązuje zakaz libm (00 §K-6) i zakaz
//! iterowania po `HashMap` (00 §3.2). Zależy od `core`, `ecs` i `spatial` — i **nie
//! zależy od `sim/world`**, bo to `sim/world` rozszerza się o populację (decyzja 9.11).

#![forbid(unsafe_code)]

pub mod arrayvec;
pub mod components;
pub mod des;
pub mod needs;
pub mod places;
pub mod store;

// K-2: graf pieszy należy do M4. Moduł jest prywatny i taki zostaje — na zewnątrz
// wychodzi wyłącznie `WalkOracle` jako implementacja wspólnego traitu, bez ani jednego
// typu trasy. Test `architektura::walk_nie_wycieka` pilnuje tej linijki.
pub(crate) mod walk;

pub use arrayvec::ArrayVec;
pub use components::{
    register, register_components, register_resources, AgentState, EduField, EduLevel, Employment,
    Identity, Lifecycle, Lod, Needs, Personality, PlanRef, Residence, ShiftKind, SkillSlot, Skills,
    Vitals, Wealth, HOT_COMPONENT_BYTES,
};
pub use components::{KnowledgeRef, RelationsRef};
pub use des::{
    order_key, EventKind, EventQueue, HhEventKind, ReplanCause, SimEvent, REPLAN_BUDGET_PER_TICK,
    REPLAN_COOLDOWN_MIN, WHEEL_MINUTES,
};
pub use needs::{
    decay_between, deprivation_of, DeprivationEffectsSystem, NeedDecaySystem, NeedEffect, NeedSpec,
    NeedTable, NeedTableError, DECAY_SHARDS,
};
pub use places::{
    choose_place, default_hours, knowledge_key, CitizenView, EmptyPlaces, FlakyPlaces,
    FulfilOutcome, FulfilRequest, InfinitePlaces, KnowledgeView, OpenHours, PanickingPlaces,
    PlaceCandidate, PlaceEntry, PlaceProvider, PlaceTable, TravelEstimate, TravelOracle,
    TripHandle, TripRequest, MAX_CANDIDATES, MAX_ON_ROUTE,
};
pub use store::{
    Knowledge, KnowledgeKind, KnowledgeSlab, PlanSlab, PlanSlot, Relation, RelationKind,
    RelationSlab, Slab, SlabRef, SLAB_CLASSES, SLAB_MAX,
};
pub use walk::WalkOracle;
