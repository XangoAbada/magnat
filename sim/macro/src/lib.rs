//! `sim/macro` — model makro i „co jeśli" (crate fazy **M10**, podzbiór budowany w M7f).
//!
//! # Czyj to crate i co tu robi M7
//!
//! Właścicielem jest M10 (dokument 00 §1). M7 buduje w nim **podzbiór wymagany
//! przez tier strategiczny** (M10 §6, tabela podzbioru): `MacroState`, `lift()`,
//! fazy 2–6 `step()` i `what_if()`. Nie powstaje tu drugi model makro obok modelu
//! M10 — ryzyko `R9` mówi wprost, dlaczego byłoby to najgorsze z możliwych wyjść.
//!
//! # Zasada, na której ten crate stoi
//!
//! **`sim/macro` nie zawiera logiki ekonomicznej.** Zawiera agregację, alokację
//! i pętlę czasu. Każda decyzja o cenie, płacy, produkcji i użyteczności oraz każde
//! zaksięgowanie pieniądza przechodzi przez `sim/economy::kernel` (`K-50`, M10a §5.1).
//! Różnica mezo vs. makro to **wyłącznie** to, czy `softmax_shares` jest losowany
//! (mezo: jeden agent wybiera jedną opcję), czy stosowany jako wagi (makro: komórka
//! dzieli popyt proporcjonalnie).
//!
//! Stąd bierze się miejsce tego crate'u w grafie: stoi **nad** `sim/economy`,
//! bo z niego czerpie. Zależność w drugą stronę zamknęłaby cykl — i to jest powód,
//! dla którego tier strategiczny nie mógł zostać w `sim/economy::ai_run` razem
//! z dwoma pozostałymi.
//!
//! # Tryb wyłącznie porównawczy
//!
//! Model ma nieusuwalne odchylenie 3–12% dla pojedynczej firmy (wariancja rozkładu
//! wielomianowego przy ~200 klientach) i to nie jest kwestia kalibracji. Dlatego
//! `MacroOutcome` **nie wychodzi z tego crate'u jako liczba**: jedynym wyjściem jest
//! [`whatif::RankedVariants`] z `decisive_winner()` i `direction()`. Kwota do grosza
//! pochodzi z ksiąg firmy, nigdy z prognozy (§5.10, `R14`, `R15`).

pub mod brandseed;
pub mod commute;
pub mod dryrun;
pub mod lift;
pub mod lod;
pub mod lower;
pub mod state;
pub mod step;
pub mod system;
pub mod types;
pub mod whatif;

pub use brandseed::{fill_brand_stock, seed_memory, SEED_MAX, SEED_MIN};
pub use dryrun::{
    dry_run, ChronicleEvent, ChronicleKind, DryRunConfig, DryRunResult, GateCheck,
    VerificationReport,
};
pub use lift::{lift, state_hash};
pub use lod::{BlockReason, CityLod, LoadStats, MacroLodPolicy};
pub use lower::{lower, lower_cell, CellExpansion, LowerError, LowerReport, PersonState};
pub use state::{MacroCell, MacroFirm, MacroState, N_NEEDS};
pub use step::{step, MacroParams};
pub use system::{register_macro, MacroHandle, MacroSystem};
pub use types::{
    cell_grain, BranchId, CitizenSeed, ClassGrain, ClassId, CommuteMatrix, EpochState,
    MacroAccount, MacroLedger, MacroStock, SparseVec, TechLevel, MACRO_ACCOUNTS, MIN_CELL_POP,
};
pub use whatif::{rank_variants, what_if, MacroOutcome, RankedVariants, Scenario};
