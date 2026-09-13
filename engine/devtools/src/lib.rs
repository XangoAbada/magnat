//! `magnat-devtools` — szkielet narzędzi deweloperskich (PRD §16.5, M0 §5.10).
//!
//! M0 daje rusztowanie, na którym kolejne fazy wieszają swoje inspektory i komendy:
//! tekstowy zrzut ECS, rejestr komend konsoli, makro profilera o zerowym koszcie
//! bez feature'a i pierścieniowy bufor metryk. **Każda faza dopisuje swoje komendy
//! przez `Console::register`** — nie buduje własnej konsoli.

#![forbid(unsafe_code)]

pub mod console;
pub mod inspector;
pub mod metrics;
pub mod profile;

pub use console::{Console, ConsoleFn};
pub use inspector::Inspector;
pub use metrics::MetricSink;
