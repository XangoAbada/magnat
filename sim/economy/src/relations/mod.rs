//! Relacje międzyfirmowe i pracownicze (M10e, WP10.13 i WP10.14, PRD §7.9 i §6.6).
//!
//! Trzy mechaniki, jedna podfaza i jedno pytanie: **co dzieje się między firmami
//! i w firmach poza rynkiem**. Rynek odpowiada na „ile to kosztuje"; ten moduł
//! na „z kim handluję, z kim się zmawiam i z kim się kłócę".
//!
//! | Mechanika | Gdzie mieszka | Dlaczego tam |
//! |---|---|---|
//! | Zaufanie do dostawcy | `sim/supply::b2b::relation` | wszystkie trzy wejścia (kontrakty, rozliczenia, przetarg) są tam (`FE-13`) |
//! | Zmowa cenowa | [`cartel`] | podnosi cenę półkową, a półki ma `Market` |
//! | Związek zawodowy | [`union`] | żal liczy się z rachunku wyniku, mediany zawodu i stanu załogi — trzech rzeczy widocznych naraz tylko stąd |
//!
//! ## Czego tu nie ma, bo nie potrzebuje ani jednej linii kodu
//!
//! **Spółka celowa (JV)** to firma, której właścicielami są dwie inne firmy —
//! `Owner::Firm` istnieje od M7a, a `Firm::owners_sum_ok` pilnuje sumy 10 000 bp
//! (`FE-7`, `K-85`). Drugiej tablicy własności nie ma i nie będzie.
//!
//! **Franczyza i licencja** to `ContractId` z M6 (M10 §6: „bez nowego typu umowy");
//! licencję technologiczną zbudował już M10c tą samą drogą.
//!
//! **Integracja pionowa i pozioma** to przejęcie — a przejęcia zbudował M10d
//! (`ControlAcquired`). To, czy przejmowana firma jest dostawcą, czy konkurentem,
//! jest pytaniem do katalogu towarów, a nie osobnym mechanizmem.
//!
//! Wariant, który nie ma ani jednej ścieżki powstania, przechodzi każdy test
//! i wygląda w kodzie tak samo jak działający (`K-67`) — więc go tu nie ma.

pub mod cartel;
pub mod data;
pub mod grievance;
mod talks;
pub mod union;

mod system;

pub use cartel::{detection_ppm, Cartel, CartelId, Cartels, FloorIndex};
pub use data::{
    CartelParams, RelationsError, RelationsTuning, UnionParams, RELATIONS_SCHEMA_VERSION,
};
pub use grievance::{
    accept_bp, component_threshold, concession_bp, demand_raise_bp, grievance, largest_component,
    may_form, GrievanceTerms,
};
pub use system::{register_relations, step_day, RelationsDay, RelationsSystem};
pub use union::{
    DetectedCartel, Grievance, StrikeCall, Union, UnionDemand, UnionId, UnionState, Unions,
};
