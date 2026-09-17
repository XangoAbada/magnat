//! AI firm — trzy poziomy decyzji (M7e WP11/WP12/WP14, M7 §5.6, §5.9, PRD §12).
//!
//! **Wejściem jest [`crate::view::FirmView`] i nic poza nim.** Nie ma tu `&World`,
//! nie ma zapytań ECS, nie ma dostępu do ksiąg — i nie jest to samodyscyplina, tylko
//! granica crate'u: `sim/firms` nie widzi `sim/economy`, więc kodu, który mógłby tu
//! podejrzeć cudzy koszt, nie da się napisać (§5.8).
//!
//! **Każda funkcja zwraca akcję razem z powodem.** `Decided<T>` z `sim/policy` nie ma
//! konstruktora bez powodu, więc decyzji bez `DecisionReason` nie da się wyprodukować —
//! wyjaśnialność (00 §7) jest tu wymuszona typem, a nie przeglądem kodu.
//!
//! ## Co jest tutaj, a co u wykonawcy
//!
//! Tutaj: **co zrobić**. Wykonanie — ustawienie sterownika ceny, wpisanie celu zamówienia,
//! zamknięcie zakładu, przypięcie presetu — jest w `sim/economy::ai_run`, bo tam są dane.
//! Ten sam podział, którym `D2` rozciął rynek pracy, a `AY-3` wykonawcę polityk.
//!
//! ## Trzy poziomy i ich budżet
//!
//! Budżet wyraża się **w liczbie firm na tick**, nigdy w czasie rzeczywistym (§5.6,
//! 00 §3.5) — pilnuje tego `Tier::max_per_tick` i kolejka przepełnienia w `Scheduler`.
//!
//! - **operacyjny**, raz na dobę: cena i zapas ([`ops`]);
//! - **taktyczny**, raz na miesiąc: rentowność zakładów, kurs firmy, polityki ([`tactical`]);
//! - **strategiczny**, raz na kwartał: reakcja na wejście rywala ([`reaction`])
//!   oraz wybór wariantu rozwoju ([`strategic`]). Reakcja makra **nie potrzebuje** —
//!   wyzwala ją zmierzona utrata udziału. Wybór wariantu potrzebuje, ale dostaje
//!   go już przeliczonego: `Outlook` w widoku niesie ranking, nie prognozę.

pub mod ops;
pub mod reaction;
pub mod strategic;
pub mod tactical;

pub use ops::{decide_operational, OpsAction};
pub use reaction::{decide_reaction, Campaign};
pub use strategic::{decide_strategic, propose_variants, Outlook, StrAction, StrategicOutlooks};
pub use tactical::{decide_tactical, TacAction};

/// Decyzja z powodem — ten sam typ, którym posługuje się ewaluator polityk (`K-11`).
pub use magnat_policy::Decided;
