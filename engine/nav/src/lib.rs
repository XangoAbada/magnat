//! `magnat-nav` — grafy transportu i routing (M4a, PRD §9.1, §17.6).
//!
//! | Moduł | Za co odpowiada |
//! |---|---|
//! | [`graph`] | `RoadGraph`, warstwy modalne, manewry skrętne, walidator, siatki syntetyczne |
//! | [`cch`] | kolejność kontrakcji, kustomizacja, zapytanie dwukierunkowe, rozpakowanie trasy |
//! | [`alt`] | A\* z heurystyką landmarkową — warstwa piesza i rowerowa |
//! | [`cache`] | `RouteCache` tras stabilnych (dom↔praca) |
//! | [`matrix`] | `TravelTimeMatrix` dzielnica × godzina × środek |
//! | [`rebuild`] | budżetowana kustomizacja i rekontrakcja, deterministyczny tick podmiany |
//! | [`router`] | `Router`, profile wag, spięcie powyższych |
//!
//! **Crate nie wie nic o mieście.** Nie ma tu `RoadNetwork`, parcel ani budynków —
//! wejście wchodzi przez [`graph::RoadGraphBuilder`], a adapter z geometrii M2 mieszka
//! w `sim/world::nav_build` (`Z-3`). Kierunek zależności jest wymuszony, nie estetyczny:
//! `sim/world` zależy od `sim/agents`, a `sim/agents` od M4b zależy od `magnat-nav`,
//! więc `magnat-nav → sim/world` byłoby cyklem, którego Cargo nie zbuduje.
//!
//! Determinizm (00 §3): brak `HashMap`, brak funkcji przestępnych poza `core::det_math`,
//! równoległość wyłącznie przez `engine/jobs` ze składaniem po indeksie.

#![forbid(unsafe_code)]

pub mod alt;
pub mod cache;
pub mod cch;
pub mod graph;
pub mod matrix;
pub mod rebuild;
pub mod router;

pub use graph::{
    synthetic_grid, synthetic_road_network, validate, Bridge, BridgeId, Csr, EdgeId, EdgeSpec,
    GeomRef, GraphError, GraphReport, Modality, ModalityMask, NavGraphs, NodeControl, NodeId,
    RoadEdge, RoadGraph, RoadGraphBuilder, RoadNode, SignalPlanId, TransferKind, TransferLink,
    TurnMovement, TurnPriority,
};

pub use cache::{CacheStats, RouteCache, RouteKey};
pub use cch::{build_order, contract, ChGraph, ChScratch, ContractionOrder};
pub use matrix::TravelTimeMatrix;
pub use rebuild::{NavJob, RebuildProgress, RebuildQueue, WORK_PER_TICK};
pub use router::{
    profile_weights, NavRouter, Route, RouteLeg, RouteProfile, RouteQuery, RouteStats, Router,
    EXCLUDED, HEAVY_REFERENCE_MASS, LANDMARK_COUNT,
};
