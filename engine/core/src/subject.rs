//! `Subject` — jeden uchwyt do każdej encji, o którą gracz może zapytać „dlaczego"
//! (00 §4a `K-62`, M9c §5.7).
//!
//! # Po co to istnieje
//!
//! Do M9b uchwytów było dwanaście: `CitizenId`, `FirmId`, `SiteId`, `VehicleId` i reszta
//! z 00 §2, a obok nich `magnat_ui::Selection` obejmujące **trzy** z nich. Każda nazwa
//! w karcie inspekcji była przez to napisem, a nie odnośnikiem — „wybrała »Dobry Koszyk«"
//! nie miało czym wskazać na ten zakład.
//!
//! # Dlaczego w `core`, a nie w `engine/ui`
//!
//! Bo niesie go [`crate::DecisionReason`], a powód powstaje w `sim/*`. Gdyby `Subject`
//! mieszkał w interfejsie, każda faza dopisująca wariant powodu musiałaby zależeć
//! od `engine/ui` — czyli dokładnie ta zależność, którą `K-8` wypycha do `core`.
//!
//! # Czego tu nie ma
//!
//! `Batch`, `Offer`, `Tender`, `Case` i `Permit` (`DE-9`). Pierwsze dwa to uchwyty aren
//! (`K-16`) parametryzowane typem, którego `core` nie zna; pozostałe trzy siedzą
//! w `sim/city`. Dokłada je `M9c` razem z decyzją, gdzie ich identyfikatory mają mieszkać.
//!
//! **To nie jest uniwersalny `EntityRef`.** `Subject` adresuje to, co ma **kartę**,
//! a nie każdą encję ECS: usługa publiczna jedzie jako [`Subject::Site`], bo
//! `PublicService.site` jest `SiteId`, a wybory są zakładką karty rady.

use crate::ids::{
    BuildingId, CitizenId, ContractId, FirmId, HouseholdId, ParcelId, SiteId, VehicleId,
};
use crate::types::{DistrictId, EventId};

/// Podmiot, który ma własną kartę inspekcji.
///
/// Rozszerza się go tak samo jak [`crate::DecisionReason`] (`K-12`): faza dokładająca byt,
/// o który gracz może zapytać „dlaczego", dokłada wariant **i** układ zakładek jego karty.
/// Brak ramienia w karcie ma łamać kompilację, a nie pokazywać pustą stronę.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Subject {
    Citizen(CitizenId),
    Household(HouseholdId),
    Firm(FirmId),
    /// Zakład: sklep, fabryka, ale też szkoła i przychodnia (M8d).
    Site(SiteId),
    Building(BuildingId),
    Parcel(ParcelId),
    Vehicle(VehicleId),
    Contract(ContractId),
    District(DistrictId),
    Event(EventId),
    /// Rada miasta. Jedna na świat, więc bez identyfikatora.
    Government,
}

/// Rodzaj podmiotu bez ładunku — do wyboru układu zakładek i ikony.
///
/// Osobny enum, a nie `std::mem::discriminant`: kolejność wariantów jest tu czytelna
/// i da się po niej indeksować, a `discriminant` nie daje ani jednego, ani drugiego.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SubjectKind {
    Citizen,
    Household,
    Firm,
    Site,
    Building,
    Parcel,
    Vehicle,
    Contract,
    District,
    Event,
    Government,
}

impl SubjectKind {
    /// Klucz tekstowy rodzaju — człon `ui.subject.<key>` w `data/locale/`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            SubjectKind::Citizen => "citizen",
            SubjectKind::Household => "household",
            SubjectKind::Firm => "firm",
            SubjectKind::Site => "site",
            SubjectKind::Building => "building",
            SubjectKind::Parcel => "parcel",
            SubjectKind::Vehicle => "vehicle",
            SubjectKind::Contract => "contract",
            SubjectKind::District => "district",
            SubjectKind::Event => "event",
            SubjectKind::Government => "government",
        }
    }

    pub const ALL: [SubjectKind; 11] = [
        SubjectKind::Citizen,
        SubjectKind::Household,
        SubjectKind::Firm,
        SubjectKind::Site,
        SubjectKind::Building,
        SubjectKind::Parcel,
        SubjectKind::Vehicle,
        SubjectKind::Contract,
        SubjectKind::District,
        SubjectKind::Event,
        SubjectKind::Government,
    ];
}

impl Subject {
    #[must_use]
    pub const fn kind(self) -> SubjectKind {
        match self {
            Subject::Citizen(_) => SubjectKind::Citizen,
            Subject::Household(_) => SubjectKind::Household,
            Subject::Firm(_) => SubjectKind::Firm,
            Subject::Site(_) => SubjectKind::Site,
            Subject::Building(_) => SubjectKind::Building,
            Subject::Parcel(_) => SubjectKind::Parcel,
            Subject::Vehicle(_) => SubjectKind::Vehicle,
            Subject::Contract(_) => SubjectKind::Contract,
            Subject::District(_) => SubjectKind::District,
            Subject::Event(_) => SubjectKind::Event,
            Subject::Government => SubjectKind::Government,
        }
    }

    /// Encja ECS, jeśli podmiot nią jest. `None` dla dzielnicy, zdarzenia i rady —
    /// to są indeksy do tablic, nie encje.
    #[must_use]
    pub const fn entity(self) -> Option<crate::Entity> {
        match self {
            Subject::Citizen(x) => Some(x.0),
            Subject::Household(x) => Some(x.0),
            Subject::Firm(x) => Some(x.0),
            Subject::Site(x) => Some(x.0),
            Subject::Building(x) => Some(x.0),
            Subject::Parcel(x) => Some(x.0),
            Subject::Vehicle(x) => Some(x.0),
            Subject::Contract(x) => Some(x.0),
            Subject::District(_) | Subject::Event(_) | Subject::Government => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    fn encja(i: u32) -> crate::Entity {
        crate::Entity::new(i, NonZeroU32::MIN)
    }

    #[test]
    fn rodzaj_odpowiada_wariantowi_i_ma_wlasny_klucz() {
        let wszystkie = [
            Subject::Citizen(CitizenId(encja(1))),
            Subject::Household(HouseholdId(encja(2))),
            Subject::Firm(FirmId(encja(3))),
            Subject::Site(SiteId(encja(4))),
            Subject::Building(BuildingId(encja(5))),
            Subject::Parcel(ParcelId(encja(6))),
            Subject::Vehicle(VehicleId(encja(7))),
            Subject::Contract(ContractId(encja(8))),
            Subject::District(DistrictId(9)),
            Subject::Event(EventId(10)),
            Subject::Government,
        ];
        // Pokrycie: każdy rodzaj ma swój wariant i odwrotnie. Wariant dopisany bez
        // rodzaju złamie `kind()`, rodzaj dopisany bez wariantu — ten test.
        assert_eq!(wszystkie.len(), SubjectKind::ALL.len());
        for (s, k) in wszystkie.iter().zip(SubjectKind::ALL) {
            assert_eq!(s.kind(), k);
        }
        let mut klucze: Vec<&str> = SubjectKind::ALL.iter().map(|k| k.key()).collect();
        klucze.sort_unstable();
        let ile = klucze.len();
        klucze.dedup();
        assert_eq!(klucze.len(), ile, "dwa rodzaje o tym samym kluczu");
    }

    #[test]
    fn encja_jest_tylko_tam_gdzie_naprawde_jest() {
        assert_eq!(
            Subject::Firm(FirmId(encja(3))).entity().map(|e| e.index()),
            Some(3)
        );
        assert!(Subject::District(DistrictId(1)).entity().is_none());
        assert!(Subject::Government.entity().is_none());
    }
}
