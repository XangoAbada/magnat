//! `magnat-headless` jako **biblioteka** — rusztowanie scenariuszy wołane spoza
//! własnej binarki.
//!
//! Wystawienie targetu bibliotecznego jest wykonaniem **decyzji otwartej nr 10**
//! dokumentu fazy M5: balansator (`tools/balansator`, WP13) ma uruchamiać przebiegi
//! jako funkcje, a nie parsować polski raport z wyjścia procesu. Przy okazji
//! rozwiązuje to `AB-1`: klient graficzny stawia **tę samą** gospodarkę co scenariusz
//! `m5shop`, zamiast przepisywać ją u siebie.
//!
//! Wystawione jest dokładnie to, co ma więcej niż jednego konsumenta:
//! - [`population`] — budowa miasta M2 i Etap 8 (`zbuduj_miasto`, `swiat_agentow`,
//!   `zaludnij`),
//! - [`retail`] — most „zakłady Etapu 7 → rynek detaliczny" (`AB-1`),
//! - [`plants`] — ten sam Etap 7 od strony produkcji: `SiteSeed` → `PlantSite`
//!   z liniami, licznikami, rampą i zapasem startowym (`AO-3`),
//! - [`firms`] — most „miasto Etapu 7 → firmy M7" (`AR-16`),
//! - [`labor`] — miasto z firmami i wpiętym rynkiem pracy (M7b): scenariusz `m7labor`
//!   i test integracyjny `labor_city.rs` stawiają **ten sam** świat.
//!
//! Scenariusze (`m3day`, `m5shop`, `nav`, …) zostają modułami **binarki**: mają CLI,
//! raport i kod wyjścia, czyli wszystko to, czego biblioteka nie powinna nieść.

#![forbid(unsafe_code)]

pub mod firms;
pub mod full;
pub mod labor;
pub mod plants;
pub mod population;
pub mod retail;
