//! `magnat-traffic` — ruch: pojazdy, warstwa mezo i podróże (M4, podfaza M4b).
//!
//! Crate stoi na `engine/nav` (grafy i routing z M4a) i wnosi to, czego M4a nie miał:
//! **czas, paliwo i pieniądze przejazdu**.
//!
//! Trzy rzeczy, które warto wiedzieć, zanim się tu coś dopisze:
//!
//! 1. **Warstwa mezo jest jedynym źródłem prawdy ekonomicznej** (00 §4, M4d §5.4).
//!    Mikro z M4d będzie wizualizatorem bez prawa zapisu. Jeśli kiedykolwiek pojawi
//!    się pokusa, żeby „prawdziwy korek kosztował", to znaczy, że ktoś chce oddać
//!    ekonomię warstwie zależnej od kamery — a kamera nie wchodzi do hasha stanu.
//! 2. **We wzorze kosztu nie ma floata.** Granicą jest sygnatura `settle_edge`.
//!    Floaty wolno stosować w geometrii i w szacunkach planowania, nigdy w ledgerze.
//! 3. **Sieć nie dotyka ECS.** `TrafficNetwork::step_minute` zwraca zdarzenia,
//!    a `TrafficSystem` je stosuje. Dzięki temu krok minutowy da się przetestować
//!    bez świata, a zbiór rzeczy, które ruch zapisuje, jest wypisany w jednym miejscu.
//!
//! Czego tu **nie ma** i gdzie to jest: wybór środka z pełnym kosztem uogólnionym,
//! parkingi i komunikacja miejska — M4c; car-following, sygnalizacja fazowa i dowód
//! równoważności LOD — M4d; ruch towarowy i rampy — M6.

#![forbid(unsafe_code)]

pub mod mezo;
pub mod micro;
pub mod mode;
pub mod overlay;
pub mod parking;
pub mod oracle;
pub mod spec;
pub mod systems;
pub mod transit;
pub mod trip;
pub mod vehicle;

pub use mezo::{
    settle_edge, settle_node, EdgeQueue, LedgerEntry, LinkState, MezoState, NodeState,
    VehicleSpecRef, CS_PER_MINUTE, UL_PER_ML,
};
pub use micro::{
    equilibrium_speed_cms, idm_speed_dkmh, IdmParams, MicroLayer, MicroVehicle, Pedestrian,
    PedestrianBuffer, VehicleBuffer, VehicleFeed, CALIBRATION_VEHICLE_CM, IDM_SCHEMA_VERSION,
    MICRO_UNIT_CAP, NO_EDGE,
};
pub use overlay::{
    rasterize_edges, rasterize_points, OverlayInputs, TrafficField, TrafficOverlay,
    TrafficOverlaySnapshot,
};
pub use mode::{
    evaluate_modes, Candidate, DiscomfortBreakdown, GeneralizedCost, Infeasible, ModeChoiceParams,
    ModeContext, ModeDecision, OptionOffer, ShareRange, TravelOption, MODE_CHOICE_SCHEMA_VERSION,
};
pub use parking::{
    ParkingDenied, ParkingKind, ParkingLot, ParkingRegistry, ParkingSlotRef, CURB_WALK_RADIUS_M,
    MAX_LOTS_SEARCHED, NO_LOT,
};
pub use oracle::{
    speed_pct, walk_minutes_for, DriverEntry, OracleHandle, Station, TrafficOracle,
    FUEL_RESERVE_FACTOR, WALK_SPEED_CM_PER_MIN,
};
pub use spec::{
    DataError, FuelKind, VdfClass, VdfTable, VehicleCatalog, VehicleClassId, VehicleClassSpec,
    VDF_SCHEMA_VERSION, VEHICLES_SCHEMA_VERSION,
};
pub use systems::{
    TripLog, TripRecord,
    register_traffic, FareLedger, FuelLedger, TrafficServices, TrafficSystem, VehicleWearSystem,
};
pub use transit::{
    LineId, OperatorRef, Timetable, TransitEvent, TransitJourney, TransitLine, TransitMode,
    TransitNetwork, TransitRun, TransitStats, TransitStop, Waiting, MAX_ACCESS_M, MAX_WAIT_MIN,
    STOP_SPACING_M,
};
pub use trip::{
    PendingTrip, TrafficEvent, TrafficNetwork, TrafficStats, TripFailure, TripId, TripLedger,
    TripOutcome, TripPurpose, GRIDLOCK_RELEASE_MIN, REFUEL_DWELL_MIN,
};
pub use vehicle::{
    FuelTank, LocationKind, OwnerKind, VehicleClass, VehicleCondition, VehicleLocation,
    VehicleOwner, VEHICLE_COMPONENT_BYTES,
};

use magnat_core::{PlaceRef, StateHasher};

/// Miejsce do hasha stanu. `PlaceRef` jest enumem z ładunkiem i nie ma własnego
/// `HashState` w `core` (to słownik, nie stan) — każda faza, która trzyma go
/// w komponencie, musi go zahaszować sama. Tutaj jest jedna taka funkcja zamiast
/// czterech kopii.
pub(crate) fn hash_place(p: PlaceRef, h: &mut StateHasher) {
    match p {
        PlaceRef::Building(e) => {
            h.write_u8(0);
            h.write_u32(e.0.index());
        }
        PlaceRef::Parcel(e) => {
            h.write_u8(1);
            h.write_u32(e.0.index());
        }
        PlaceRef::Site(e) => {
            h.write_u8(2);
            h.write_u32(e.0.index());
        }
        PlaceRef::District(d) => {
            h.write_u8(3);
            h.write_u16(d.0);
        }
        PlaceRef::Coord(c) => {
            h.write_u8(4);
            h.write_u32(c.x as u32);
            h.write_u32(c.y as u32);
            h.write_u32(c.z as u32);
        }
    }
}
