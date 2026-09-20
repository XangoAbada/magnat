//! Reklama, marka i media (M10b: WP10.6, WP10.7).
//!
//! **Dlaczego to jest osobny crate.** Kampania reklamowa jest jedynym bytem w projekcie,
//! który widzi naraz cztery rzeczy: ruch (billboard stoi przy krawędzi grafu), pamięć
//! mieszkańca (ekspozycja zapisuje slot marki), firmę i pieniądz (kampanię ktoś płaci
//! i księguje) oraz zdarzenia (sponsoring i redakcja). W grafie zależności żaden
//! istniejący crate nie stoi nad całą tą czwórką: `sim/events` stoi, ale jest własnością
//! M8, a wrzucenie tam marketingu rozjechałoby własność modułu z 00 §1 — dokładnie tak,
//! jak `K-54` rozstrzygnął miejsce `sim/macro` **nad** `sim/economy`, a nie obok niego.
//!
//! **Czego tu nie ma i gdzie to jest.** Pamięć marki — sloty, zanik, asymetria — mieszka
//! w `sim/agents` (`magnat_agents::brand`), bo pisze do niej także `sim/economy` przy
//! zakupie i `sim/macro` przy zasiewie po `lower()`, a oba stoją **pod** tym crate'em.
//! Kalibracja (współczynniki uczenia, tablica zaniku, cennik kanałów, parametry redakcji)
//! jest w `data/tuning/brand.ron` (`K-35`) i ładuje ją `magnat_agents::BrandData` —
//! jeden plik, jeden `schema_version`, jedna ładowarka.

#![forbid(unsafe_code)]

pub mod ai;
pub mod campaign;
pub mod outlet;
pub mod reach;
pub mod system;

pub use campaign::{AdCampaign, AdChannel, CampaignMetrics, Campaigns};
pub use outlet::{MediaOutlet, Outlets, Story, STORY_SPREAD_DAYS};
pub use reach::{expose, expose_media, sample_stride, DistrictRoster};
pub use system::{step, MediaReport, MediaSystem};

use magnat_ecs::World;

/// Rejestruje zasoby reklamy i mediów wraz z hakami hasha stanu (00 §3.6).
///
/// Osobno od systemu, bo scenariusz stawiający wycinek świata (`m5shop`, `m7labor`)
/// może chcieć rejestru kampanii bez pętli mediów — tak samo jak `register_society`
/// jest osobne od `SocietySystem`.
pub fn register_media(world: &mut World) {
    world.insert_resource(Campaigns::new());
    world.insert_resource(Outlets::new());
    world.register_resource_hash::<Campaigns>();
    world.register_resource_hash::<Outlets>();
}
