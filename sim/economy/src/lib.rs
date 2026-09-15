//! `magnat-economy` — gospodarka detaliczna (M5).
//!
//! Crate stoi na jednym zdaniu z PRD §6.1: **nie istnieje globalna cena rynkowa**.
//! Cenę niesie wyłącznie oferta konkretnego sklepu, a wszystko, co wygląda na „cenę
//! mleka w mieście", jest agregatem po ofertach ([`price_stats`]). Drugie zdanie jest
//! z §6.5: pieniądz nie powstaje i nie znika poza dwoma jawnymi kanałami (emisja
//! początkowa i kredyt), a każdą jego zmianę widać w [`Books`].
//!
//! Co zawiera **M5a**:
//! - §5.1 — [`Books`], konta, podwójny zapis, ewidencja podaży pieniądza,
//! - §5.2 — [`Offer`] w arenie (`K-16`) i [`OfferIndex`] z zapytaniem promieniowym.
//!
//! Czego tu **nie ma** i gdzie to jest: sklep, półka i zapas — M5b; polityki cenowe
//! i księgowość — M5c; budżety gospodarstw, banki i inflacja — M5d; panel i balansator
//! — M5e. Rynek B2B i partie towaru należą do M6, podatki do M8.
//!
//! Podmoduł `kernel` (D20 — rdzeń liczbowy wołany tak samo przez mezo i makro) powstaje
//! razem ze swoimi funkcjami: `next_price` w M5c/WP6, `take_cogs` i `ledger_post`
//! w M5c/WP7. W M5a nie ma go z czego złożyć — zapisu księgowego jeszcze nie ma,
//! a arytmetyka podziału kwoty jest gotowa w `magnat_core::split_proportional`.

#![forbid(unsafe_code)]

pub mod books;
pub mod offer;

pub use books::{
    Account, AccountId, AccountKind, AccountOwner, Books, ChargeKind, ExternalInvestorId, LoanId,
    MoneySupplyLedger, ProgramId, SupplierRef, Transaction, TxError, TxId, TxJournal, TxKind,
    TxMemo,
};
pub use offer::{
    price_stats, query_offers, CategoryId, Offer, OfferId, OfferIndex, PriceBasis, PriceStats,
};

use magnat_core::{Arena, ArenaKind};
use magnat_ecs::World;
use magnat_spatial::GridSpec;

/// Wstawia stan gospodarki do świata i wpina go do hasha (00 §3.6).
///
/// Hashowane są [`Books`] i arena ofert. [`OfferIndex`] **nie** — jest pochodną areny
/// i pozycji zakładów, więc hashowanie go dokładałoby do stanu świata coś, co da się
/// z tego stanu odtworzyć; ta sama zasada, którą M4 zastosował do `TrafficOverlay`.
pub fn register_economy(world: &mut World, grid: GridSpec) {
    world.insert_resource(Books::new());
    world.insert_resource(Arena::<Offer>::new());
    world.insert_resource(OfferIndex::new(grid));
    world.register_resource_hash::<Books>();
    world.register_arena_hash::<Offer>(ArenaKind::Offers);
}
