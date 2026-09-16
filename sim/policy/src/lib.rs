//! `magnat-policy` — **jeden silnik reguł dla gracza i dla AI** (`K-11`, M7c WP6b).
//!
//! # Po co ten crate w ogóle jest
//!
//! PRD §6.3 obiecuje graczowi „ten sam zestaw narzędzi co AI". Gdyby polityka firmy
//! była zaszyta w Ruście dla AI, a M9 zbudowała osobny interpreter reguł dla gracza,
//! to ta sama reguła — „−2 % względem najtańszego konkurenta w promieniu 3 km" —
//! zachowywałaby się inaczej po obu stronach. Nie od razu i nie widocznie: rozjazd
//! wyszedłby przy pierwszym przypadku brzegowym (brak konkurenta w promieniu, remis
//! cenowy, konkurent poniżej kosztu), czyli tam, gdzie najtrudniej powiązać objaw
//! z przyczyną. Jeden ewaluator usuwa tę klasę błędów z definicji.
//!
//! Przy okazji jest to droga do moddowalnej AI w M12: polityka staje się plikiem
//! danych, a nie kodem.
//!
//! # Podział własności (`K-11`)
//!
//! **Język projektuje M9** (`M9d-jezyk-regul.md` §5.6) i jego AST jest dla M7 wiążącą
//! specyfikacją. M7 jest właścicielem **crate'u**: buduje ewaluator, walidator i port
//! odczytu. W `game/` zostanie warstwa gracza — edytor, dry-run, podpowiedzi
//! i `ManagerExecution`, czyli jakość wykonania zależna od menedżera.
//!
//! Odstępstwa od AST z M9d są trzy i wszystkie są wypisane w tabeli korekt
//! `M7c-polityki-i-menedzerowie.md` oraz wpisane z powrotem do dokumentu M9d (`K-18`).
//!
//! # Trzy rzeczy, które warto wiedzieć, zanim się to czyta
//!
//! 1. **Reguła nie widzi świata.** Jedynym wejściem jest [`PolicyView`] — metryka jest
//!    daną, nie dostępem. To jest ta sama asymetria informacji, której pilnuje test
//!    M7 §7.3: mutacja ukrytych danych gracza nie ma prawa zmienić decyzji konkurenta.
//! 2. **Arytmetyka jest całkowitoliczbowa** (00 §2). Pieniądz w groszach, procenty
//!    w punktach bazowych, mnożenie przez `i128` z zaokrągleniem od zera. Polityka
//!    cenowa zmienia stan trwały, więc float jest tu zakazany, a nie niepożądany.
//! 3. **Ewaluacja nie alokuje i nie losuje.** Wynik idzie do `SmallVec`, drzewo warunku
//!    jest czytane, nie budowane. Wyjaśnienie ([`explain`]) alokuje — i dlatego jest
//!    osobną funkcją, wołaną raz na otwarcie karty, a nie raz na zakład na dobę.
//!
//! # Czego tu nie ma
//!
//! - **Rejestrów `register_metric`/`register_action`.** §6 dokumentu M7 je zapowiada,
//!   ale `D18` fazy mówi wprost, że drugiego konsumenta (polityki miasta M8, polityki
//!   gospodarstw M3) nikt jeszcze nie potwierdził. Rejestr z jednym wpisem jest
//!   abstrakcją, której zakazuje YAGNI, a metryka jest tu **wariantem enuma**, więc
//!   rozszerza się ją tak samo tanio i z kontrolą kompilatora. Korekta `AX-4`.
//! - **Parsera tekstu.** Reprezentacją kanoniczną jest AST; postać tekstowa służy
//!   do wyświetlania i należy do edytora M9.
//! - **Wykonawcy akcji.** Ewaluator mówi, co zrobić; robi to `sim/economy` (ceny
//!   i zapas sklepu, M7c) i dalsze podfazy. Crate reguł nie może zależeć od
//!   `sim/economy`, bo zależność idzie `economy → firms → policy`.

#![forbid(unsafe_code)]

pub mod ast;
pub mod eval;
pub mod explain;
mod hash;
pub mod presets;
pub mod scope;
pub mod validate;
pub mod view;

pub use ast::{
    Action, ArithOp, Bp, Cadence, CmpOp, ConditionExpr, Expr, FirmPolicy, GoodRef, Metric,
    OrderSource, Policy, PolicyDomain, Rule, Severity, Unit, Value, MAX_COMPETITIVE, MAX_DEPTH,
    MAX_RADIUS_M, MAX_RULES,
};
pub use eval::{condition, eval, evaluate, Decided, Decisions, FALLBACK_RULE};
pub use explain::{explain, inputs, ExplainTree};
pub use presets::{PolicyCatalog, PolicyPreset, POLICIES_SCHEMA_VERSION};
pub use scope::{resolve, PolicyScope, TagId};
pub use validate::{diagnose, validate, Diagnostic, NodePath, PolicyError};
pub use view::{BlindView, MetricCtx, PolicyView};
