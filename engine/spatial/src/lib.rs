//! `magnat-spatial` — indeksy przestrzenne świata (M2 §5.1, PRD §16.2, §17.5).
//!
//! Cztery struktury, zero dziedziczenia, wszystko SoA i deterministyczne:
//!
//! | Struktura | Do czego | Złożoność |
//! |---|---|---|
//! | [`CsrGrid`] | encje statyczne (budynki, parcele, wejścia) | budowa O(n), zapytanie O(k + m) |
//! | [`CategoryGrid`] | jeden `CsrGrid` per kategoria (PRD §17.5) | jak wyżej, m tylko z kategorii |
//! | [`DynamicGrid`] | encje ruchome, przebudowa co tick | O(n + cells) |
//! | [`ParcelTree`] | AABB o rozpiętości trzech rzędów wielkości | O(log n + b) |
//! | [`ScalarField`] | mapy wpływu i nakładki UI | próbkowanie O(1) |
//!
//! Crate **nie wie nic o mieście** — nie ma tu dróg, stref ani budynków. Wie o nim
//! `sim/world` (M2b+). Jedyny typ domenowy, który tu wchodzi, to `ParcelId` z `core`,
//! bo `ParcelTree` jest z nazwy indeksem parcel.
//!
//! Determinizm (00 §3): brak `HashMap`, brak funkcji przestępnych poza `core::det_math`,
//! równoległość wyłącznie przez `engine/jobs` i zawsze ze składaniem po indeksie,
//! nigdy po kolejności ukończenia zadań.

#![forbid(unsafe_code)]

pub mod csr;
pub mod dynamic;
pub mod field;
pub mod geom;
pub mod spec;
pub mod tree;

pub use csr::{BatchResult, CategoryGrid, CsrGrid, SpatialStats};
pub use dynamic::DynamicGrid;
pub use field::{ScalarField, UNREACHABLE};
pub use geom::{Aabb2, Vec2};
pub use spec::{morton2, CellId, CellRange, GridSpec, RowSpans};
pub use tree::ParcelTree;
