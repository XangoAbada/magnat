//! Typowane identyfikatory nad `Entity` (00 §2).
//!
//! Newtype, a nie alias: `FirmId` w miejscu `CitizenId` ma być błędem kompilacji,
//! bo w tym projekcie obie wartości są liczbą 32-bitową i inaczej rozjazd wyszedłby
//! dopiero jako dziwny wynik ekonomiczny.
//!
//! `BatchId` i `OfferId` **nie są** tutaj: po korekcie K-16 to uchwyty do aren
//! (`ArenaHandle<Batch>` definiuje M6, `ArenaHandle<Offer>` — M5), a nie encje.

use crate::entity::Entity;
use serde::{Deserialize, Serialize};

macro_rules! entity_id {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$meta])*
        #[derive(
            Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize,
        )]
        #[repr(transparent)]
        pub struct $name(pub Entity);

        impl $name {
            #[inline]
            #[must_use]
            pub const fn entity(self) -> Entity {
                self.0
            }
        }

        impl From<$name> for Entity {
            #[inline]
            fn from(id: $name) -> Entity {
                id.0
            }
        }
    )+};
}

entity_id! {
    /// Mieszkaniec (M3).
    CitizenId,
    /// Gospodarstwo domowe (M3).
    HouseholdId,
    /// Firma (M7; szkic sklepu już w M5).
    FirmId,
    /// Zakład — fizyczne miejsce działalności firmy (M5/M7).
    SiteId,
    /// Budynek (M2).
    BuildingId,
    /// Parcela (M2).
    ParcelId,
    /// Pojazd (M4).
    VehicleId,
    /// Umowa: najmu, pracy, dostawy (M6/M7).
    ContractId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    #[test]
    fn identyfikator_jest_przezroczysty_rozmiarowo() {
        assert_eq!(size_of::<CitizenId>(), 8);
        assert_eq!(size_of::<Option<FirmId>>(), 8);
    }

    #[test]
    fn konwersja_w_jedna_strone() {
        let e = Entity::new(7, NonZeroU32::new(1).unwrap());
        let firm = FirmId(e);
        assert_eq!(Entity::from(firm), e);
        assert_eq!(firm.entity().index(), 7);
    }
}
