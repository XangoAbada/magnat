//! `sim/firms` — firmy AI i rynek pracy (faza M7, właściciel crate'a).
//!
//! Podfaza **M7a** buduje firmę jako dane: stabilny klucz i sharding decyzji (WP1),
//! katalog typów zakładów w `data/site_types/` (WP2) oraz stanowiska, zatrudnienie,
//! listę płac i produktywność (WP3). Rynek pracy, polityki, finanse i AI dokładają
//! kolejne podfazy — patrz `docs/implementation-plan/M7-firmy-ai-i-rynek-pracy.md`.
//!
//! ## Trzy rzeczy, które warto wiedzieć, zanim się to czyta
//!
//! 1. **Firma nie jest encją ECS.** Cały jej stan siedzi w zasobie [`Firms`], bo dociera
//!    się do niej przez klucz albo przez zakład, nigdy przekrojowo po archetypach.
//! 2. **Pieniądza tu nie ma.** Konto firmy prowadzi `Books` z M5; lista płac produkuje
//!    fakty do zaksięgowania, a księguje je `sim/economy`. Zależność idzie
//!    `economy → firms` i odwrócić się nie da — rynek pracy M7b stoi na ofertach M5.
//! 3. **Fizyki zakładu tu nie ma.** Linie i magazyny są w `sim/supply`, półka
//!    w `sim/economy`; łącznikiem jest `SiteId`. Tu jest tylko strona zarządcza.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod firm;
pub mod hr;
pub mod key;
pub mod registry;
pub mod ring;
pub mod site;
pub mod systems;

pub use catalog::{SiteType, SiteTypeCatalog, SiteTypeCategory, SiteTypeId, Staffing};
pub use firm::{DecisionLog, Firm, FirmStatus, LoggedDecision, Owner, OwnerShare};
pub use hr::employment::{payday, BenefitSet, Employment, PayrollItem, PayrollRun, Position};
pub use hr::productivity::{effective_labor, loss_multiplier, ManagementQuality};
pub use hr::roles::{RoleTable, RoleWeights};
pub use key::{due, firm_id, slots, DecisionSlots, FirmKey, Scheduler, Tier};
pub use registry::Firms;
pub use ring::Ring;
pub use site::{Site, SitePlacement, SitePnlMonth};
