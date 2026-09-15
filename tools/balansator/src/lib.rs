//! `magnat-balansator` — WP13 fazy M5: przebiegi strojeniowe i bramki CI (§7.4).
//!
//! Narzędzie robi trzy rzeczy i nic poza nimi:
//!
//! 1. `run` — puszcza N ziaren tego samego świata co scenariusz `m5shop` i zapisuje
//!    metryki doba po dobie ([`metrics`], [`run`]),
//! 2. `gate` — czyta katalog metryk i wydaje werdykt bramek G1–G9 ([`gates`]),
//!    z opcjonalnym raportem Markdown ([`report`]),
//! 3. `calibrate-vdf` — kalibracja diagramu podstawowego, przeniesiona tu z headlessa
//!    (`T-1`): to jest narzędzie strojeniowe, a nie scenariusz ([`calibrate`]).
//!
//! **Balansator nie liczy niczego, co liczy symulacja.** CPI, inflacja, rozkład cen,
//! HHI i mediana marż przychodzą gotowe z `Market` (`AA-7`, `BalanceSample`). Druga
//! implementacja którejkolwiek z tych liczb rozjechałaby się z pierwszą przy pierwszej
//! zmianie i bramka mierzyłaby własny błąd zamiast rynku. Balansator liczy wyłącznie
//! **progi**: przyrosty między dobami, mediany po ziarnach i warunki tabeli §7.4.

#![forbid(unsafe_code)]

pub mod calibrate;
pub mod gates;
pub mod metrics;
pub mod report;
pub mod run;
