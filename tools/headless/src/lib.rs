//! `magnat-headless` jako **biblioteka** — rusztowanie scenariuszy wołane spoza
//! własnej binarki.
//!
//! Wystawienie targetu bibliotecznego jest wykonaniem **decyzji otwartej nr 10**
//! dokumentu fazy M5: balansator (`tools/balansator`, WP13) ma uruchamiać przebiegi
//! jako funkcje, a nie parsować polski raport z wyjścia procesu.
//!
//! # Gdzie są mosty (zmiana z M9a)
//!
//! Do M8e mieszkały tutaj i to było odwrócenie zależności: narzędzie testowe było
//! właścicielem drogi, którą powstaje świat, a gra tej drogi nie miała wcale.
//! Od M9a mosty są w `magnat-game` ([`magnat_game::world`]), a ten moduł
//! **reeksportuje je pod starymi nazwami** — scenariusze, testy integracyjne
//! i balansator nie drgnęły, a `magnat_headless::retail::setup` dalej znaczy to,
//! co znaczyło.
//!
//! Wystawione jest dokładnie to, co ma więcej niż jednego konsumenta:
//! - [`population`] — budowa miasta M2 i Etap 8 (`zbuduj_miasto`, `swiat_agentow`,
//!   `zaludnij`),
//! - [`retail`] — most „zakłady Etapu 7 → rynek detaliczny" (`AB-1`),
//! - [`plants`] — ten sam Etap 7 od strony produkcji: `SiteSeed` → `PlantSite`
//!   z liniami, licznikami, rampą i zapasem startowym (`AO-3`),
//! - [`firms`] — most „miasto Etapu 7 → firmy M7" (`AR-16`),
//! - [`labor`] — miasto z firmami i wpiętym rynkiem pracy (M7b): scenariusz `m7labor`
//!   i test integracyjny `labor_city.rs` stawiają **ten sam** świat,
//! - [`city`] — strona publiczna: budżet, kodeks podatkowy i kataster (M8a).
//!
//! Scenariusze (`m3day`, `m5shop`, `nav`, …) zostają modułami **binarki**: mają CLI,
//! raport i kod wyjścia, czyli wszystko to, czego biblioteka nie powinna nieść.

#![forbid(unsafe_code)]

pub use magnat_game::world::{city, events, firms, full, grid, labor, plants, retail};

pub mod population;
