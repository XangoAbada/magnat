//! # `magnat-events` — generator zdarzeń świata (M8c)
//!
//! Zdarzenie **wynika ze stanu świata**, a nie z kalendarza. Susza nie jest wpisem
//! „w 120. dobie susza" — jest skutkiem deficytu opadów. Awaria bloku nie jest rzutem
//! kostką — jest skutkiem wieku maszyny, zaległej konserwacji i miesięcy pracy
//! na granicy mocy. Strajk nie jest zdarzeniem losowym — jest skutkiem tego, że
//! firma płaci poniżej mediany zawodu, a załoga ma zły nastrój.
//!
//! Mechanizm jest jeden i ten sam dla wszystkich sześciu kategorii:
//!
//! ```text
//! stan świata → sonda → krzywa → hazard → rzut → zmiana parametru
//!     → reakcja innych faz → nowy stan świata → inna sonda
//! ```
//!
//! ## Trzy reguły, których ten crate nie łamie
//!
//! 1. **Zdarzenie nie zna słowa „cena" w mieście.** Wolno mu zmienić plon,
//!    dostępność mocy, tempo spadku potrzeby, stawkę celną i cenę **importu**.
//!    Nie ma i nie będzie miało wariantu ustawiającego cenę oferty, marżę ani
//!    wolumen sprzedaży — ceny zmieniają się same, w decyzji cenowej firmy (M7).
//!    Zakaz jest wpisany w typ [`param::Effect`], a nie w regulamin.
//! 2. **Losuje się rzut, nigdy szansa.** Hazard jest funkcją stanu świata,
//!    liczoną arytmetyką całkowitą; strumień rozstrzyga wyłącznie „czy dziś".
//! 3. **Zero floatów na ścieżce hazardu.** Wymóg silniejszy niż `K-6` — tu chodzi
//!    o to, czy zdarzenie w ogóle zaszło, a nie o dokładność liczby.
//!
//! ## Kierunek zależności
//!
//! Crate stoi **nad** wszystkim, co odpytuje i co zmienia. To jedyny układ bez
//! cyklu: gdyby konsumenci mieli **czytać** nakładkę parametrów, każdy z nich
//! musiałby zależeć od tego crate'u, a `sim/economy` zależy już od `sim/supply`.
//! Dlatego nakładka **zapisuje** wartość do pola właściciela i pamięta, co tam
//! zastała — a właściciel nie ma ani jednego `if zdarzenie` u siebie.

pub mod apply;
pub mod catalog;
pub mod eval;
pub mod hazard;
pub mod indicators;
pub mod param;
pub mod probe;
pub mod registry;
pub mod system;
pub mod weather;

pub use catalog::{
    CatalogError, Curve, DurationSpec, EventCatalog, EventDef, EventScope, EventTrigger,
    HazardFactor, Precondition, SeveritySpec, CATEGORY_FILES, EVENTS_SCHEMA_VERSION,
};
pub use eval::{evaluate, Fired, ProbeWorld};
pub use hazard::{
    duration_days, hazard_ppm, roll, severity_bps, FactorTrace, HazardTrace, MAX_PPM,
};
pub use indicators::{CityIndicators, EpochClock};
pub use param::{Effect, ParamOverlay, ParamPatch, SimParam};
pub use probe::{Probe, ProbeCache, SiteClass, SiteFilter, SiteRef};
pub use registry::{
    ChronicleEntry, Diagnosis, EventCause, Events, ResolveError, ResolvedEffect, ScopeInstance,
    WorldEvent,
};
pub use system::{indicators as city_indicators, krok, register_events, EventSystem};
pub use weather::{ClimateNorms, WeatherState, PRECIP_RING};
