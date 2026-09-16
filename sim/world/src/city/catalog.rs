//! Katalog towarów i receptur — **przeniesiony do `sim/supply` w M6a** (WP1).
//!
//! M2 trzymał go tutaj, bo Etap 7 generacji nie ma się jak domknąć bez katalogu,
//! a `sim/supply` jeszcze nie istniał. Od M6a właścicielem jest M6 nie tylko co do
//! schematu, ale i co do adresu: katalogu potrzebuje `sim/economy` (półka sklepu bierze
//! towar stąd, a nie z powietrza), a `sim/economy` nie może zależeć od `sim/world`.
//!
//! Zostaje re-eksport, więc nazwy z dokumentu M2 (`city::catalog::{Catalog, Good, …}`)
//! nie drgnęły, a `supply_closure_check` dalej woła `Catalog::reachable` bez zmian.

pub use magnat_supply::catalog::*;
