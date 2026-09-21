//! `magnat-agents` — mieszkańcy: stan, potrzeby, zdarzenia i punkty rozszerzenia (M3).
//!
//! Podfaza M3a położyła warstwę, na której stoi cała reszta fazy:
//! - **komponenty SoA i magazyny** (`components`, `store`) — kształt mieszkańca
//!   i rachunek pamięci 400 B na osobę (§17.7),
//! - **potrzeby** (`needs`) — spadek liczony z czasu absolutnego, więc shardowanie
//!   go nie zmienia,
//! - **DES** (`des`) — koło czasu, porządek totalny zdarzeń, lazy scheduling,
//! - **punkty rozszerzenia** (`places`) — `PlaceProvider` i `TravelOracle`
//!   z implementacjami tymczasowymi, które M5 i M4 podmieniają bez ruszania planera.
//!
//! Podfaza M3b dołożyła **dzień mieszkańca**:
//! - **planer** (`planner`) — cztery fazy priorytetów, `DayCanvas` na 24 sloty,
//!   `DecisionReason` per slot i tryb `explain`, który pełne uzasadnienia **odtwarza**
//!   zamiast je przechowywać,
//!
//! Podfaza M3c dołożyła **społeczeństwo**:
//! - **gospodarstwo domowe** (`household`) — skład, podział ról, typ liczony ze składu,
//! - **demografia** (`demography`) — hazardy roczne raz w roku po shardzie 1/360,
//!   terminarz porodów i wyzdrowień, dziedziczenie za `InheritanceHook`,
//! - **migracja** (`migration`) — napływ i odpływ sterowany wakatami i pustostanami;
//!   jedyny regulator populacji, bez spawnowania do celu,
//! - **status, relacje i plotka** (`social`) — klasa jako przedział statusu, wiedza
//!   o miejscach rozchodząca się wyłącznie przez kontakt,
//! - **rytm doby i miesiąca** (`society`) — kolejność wywołań, która ma znaczenie.
//!
//! **Po M4b nie ma tu modułu `walk`.** Graf pieszy i wszystko, co dotyczy trasy,
//! należy do `engine/nav` i `sim/traffic` (`K-2`, `Z-1`); `Sources.travel` jest
//! `Box<dyn TravelOracle>` i implementację wnosi crate ruchu. Tutaj zostaje
//! wyłącznie kontrakt: trait, `TripRequest`, `TripHandle` i `TravelEstimate`.
//!
//! Czego tu nie ma i gdzie to jest: generacja populacji (Etap 8), systemy ECS
//! z §5.12 i UI — M3d.
//!
//! Cały crate jest **kodem symulacji**: obowiązuje zakaz libm (00 §K-6) i zakaz
//! iterowania po `HashMap` (00 §3.2). Zależy od `core`, `ecs` i `spatial` — i **nie
//! zależy od `sim/world`**, bo to `sim/world` rozszerza się o populację (decyzja 9.11).

#![forbid(unsafe_code)]

pub mod arrayvec;
pub mod brand;
pub mod components;
pub mod demography;
pub mod des;
pub mod household;
pub mod migration;
pub mod names;
pub mod needs;
pub mod places;
pub mod planner;
pub mod school;
pub mod snapshot;
pub mod social;
pub mod society;
pub mod store;

pub mod systems;
pub mod worldparams;

pub use arrayvec::ArrayVec;
pub use brand::{
    affinity_of, brand_strength, compact_month, decayed, slots_of, touch, BrandAffinity, BrandData,
    BrandDataError, BrandSlab, BrandSlots, BrandStrength, BrandTuning, ChannelTuning, OutletTuning,
    Touch, BRAND_SCHEMA_VERSION, BRAND_SLOTS, DECAY_BUCKETS,
};
pub use components::{
    register, register_components, register_resources, AgentState, EduField, EduLevel, Employment,
    Identity, Lifecycle, Lod, Needs, Personality, PlanRef, Residence, ShiftKind, ShiftProfile,
    SkillSlot, Skills, Vitals, Wealth, HOT_COMPONENT_BYTES,
};
pub use components::{BrandsRef, KnowledgeRef, RelationsRef};
pub use demography::{
    citizen_by_index, compatibility, household_by_index, knowledge_ref, powiaz, przeklasyfikuj,
    relations_ref, Ages, DayReport, DemographyError, DemographyTable, InheritanceHook, LifeQueue,
    LifeTask, LifeTaskKind, MonthReport, NoInheritance, Population, StatusWeights, DAYS_PER_YEAR,
    DEMOGRAPHY_SHARDS,
};
pub use des::{
    order_key, EventKind, EventQueue, HhEventKind, ReplanCause, SimEvent, REPLAN_BUDGET_PER_TICK,
    REPLAN_COOLDOWN_MIN, WHEEL_MINUTES,
};
pub use household::{
    add_member, children_count, classify, members_of, remove_member, roles, Household,
    HouseholdKind, HouseholdOverflow, HouseholdRoles, MemberView, Purse, HH_INLINE_MEMBERS,
    HH_MAX_MEMBERS, MAX_ESCORTED,
};
pub use migration::{
    attractiveness, seed_population, shock_retire_jobs, spawn_household, spawn_household_aged,
    zaloz_gospodarstwo, HomeSlot, JobSlot, MigrationReport, Unsettled, UnsettledState, Vacancies,
};
pub use names::{catalog as name_catalog, NameCatalog, NameError, NAMES_SCHEMA_VERSION};
pub use needs::{
    decay_between, deprivation_of, DeprivationEffectsSystem, NeedDecaySystem, NeedEffect, NeedSpec,
    NeedTable, NeedTableError, DECAY_SHARDS,
};
pub use places::{
    choose_place, default_hours, home_of, knowledge_key, nearest_school, place_from_key, site_of,
    walk_minutes, BrandView, CitizenView, EmptyPlaces, FlakyPlaces, FulfilOutcome, FulfilRequest,
    InfinitePlaces, KnowledgeView, OpenHours, PanickingPlaces, PlaceCandidate, PlaceCatalog,
    PlaceEntry, PlaceProvider, PlaceTable, StraightLineTravel, TravelEstimate, TravelOracle,
    TripHandle, TripRequest, BASE_SPEED_M_PER_MIN, MAX_CANDIDATES, MAX_ON_ROUTE, SITE_KEY_BASE,
};
pub use planner::{
    load_plan, plan_day, plan_day_explained, render_day_debug, replan, replan_explained_into,
    request_replan, store_plan, tick_replan_cooldown, DayCanvas, HouseholdView, PlanCtx, PlanStats,
    ReasonEntry, ReasonLog, MAX_SLOTS,
};
pub use social::{
    awareness_of, for_each_known_place, knows_place, learn_place, relations_of, status_of,
    CityFacts, SocialClass, SocialIndex, SocialReport, StatusBreakdown, StatusDistribution,
    StatusInput, StatusReport, SOCIAL_CLASS_COUNT, SOCIAL_SHARDS,
};
pub use society::{
    households, is_month_start, population, register_society, total_money, SocietyReport,
};
pub use worldparams::{register_world_params, DemographyParams, NeedModifiers, NEUTRAL_BPS};

pub use school::{skill_drift_day, SkillDriftSystem, WEEK_SHARDS};
pub use snapshot::{CitizenSnapshot, MAX_TASK_TRAVEL_MIN};
pub use store::{
    Knowledge, KnowledgeKind, KnowledgeSlab, PlanSlab, PlanSlot, Relation, RelationKind,
    RelationSlab, Slab, SlabRef, SLAB_CLASSES, SLAB_MAX,
};
pub use systems::{
    bootstrap_day, micro_count, register_day, set_lod, AgentSources, DayLoopSystem, DayStats,
    HouseholdStockSystem, ReplanCooldownSystem, SocietySystem, Sources, Trace, TraceEntry,
    TravelMicroSystem, MAX_WATCHED, TRACE_LEN,
};
