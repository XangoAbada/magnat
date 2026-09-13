//! Profiler — zero kosztu bez feature'a `profiling` (M0 §5.10).
//!
//! `devtools::scope!("nazwa")` kompiluje się do **niczego**, dopóki nie włączy się
//! feature'a. Dzięki temu wolno go wstawiać w ścieżkę gorącą bez oglądania się
//! na koszt, a `cargo tree` w buildzie domyślnym nie zawiera tracy.

/// Otwiera zakres profilera do końca bieżącego bloku.
#[macro_export]
macro_rules! scope {
    ($name:literal) => {
        #[cfg(feature = "profiling")]
        let _magnat_scope = $crate::profile::span($name);
        #[cfg(not(feature = "profiling"))]
        let _magnat_scope = ();
    };
}

#[cfg(feature = "profiling")]
#[must_use]
pub fn span(name: &'static str) -> tracy_client::Span {
    tracy_client::span!(name)
}

/// Czy build ma wkompilowany profiler — do wypisania w banerze narzędzi.
#[must_use]
pub const fn enabled() -> bool {
    cfg!(feature = "profiling")
}
