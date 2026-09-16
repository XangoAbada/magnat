//! Atrapa magazynu dla testów modułu [`super`].
//!
//! Jedna kopia zamiast trzech: `ops`, `aging` i `transit` testują ten sam magazyn
//! dwutowarowy i przed M6b każdy miał własny, identyczny `katalog()`. Trzy kopie
//! rozjeżdżają się przy pierwszej zmianie schematu towaru — a schemat zmienia się
//! co podfazę.

use magnat_core::{Entity, FirmId, GoodId, Mass, Money, SimMinute, SiteId, Volume, Q};
use std::num::NonZeroU32;

use super::{BatchDraft, Store, WarehouseRole};
use crate::batch::{BatchFlags, BatchOrigin};
use crate::catalog::{Catalog, GoodForm, HazardClass, NeedCategory, StorageClass};

/// Chleb — psuje się po dobie.
pub const CHLEB: GoodId = GoodId(0);
/// Piasek — nie psuje się nigdy, więc bilans nie miesza się z psuciem.
pub const PIASEK: GoodId = GoodId(1);

pub fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

pub fn katalog() -> Catalog {
    let towar = |i: u16, key: &str, zycie: Option<u32>| crate::catalog::Good {
        key: key.into(),
        id: GoodId(i),
        category: magnat_core::NeedCategoryId(0),
        form: GoodForm::Bulk,
        density_g_per_l: 500,
        unit_mass: Mass::ZERO,
        unit_volume: Volume::ZERO,
        shelf_life_minutes: zycie,
        storage: StorageClass::Ambient,
        hazard: HazardClass::None,
        has_quality: true,
        substitutes: Vec::new(),
        external_base_price: None,
        import_via: Vec::new(),
        disposal_cost: Money::ZERO,
    };
    Catalog::from_parts(
        vec![
            towar(0, "food_bread_wheat", Some(1440)),
            towar(1, "raw_sand", None),
        ],
        Vec::new(),
        vec![NeedCategory {
            key: "test".to_string(),
            stock_cat: None,
        }],
        Vec::new(),
    )
}

pub fn draft(good: GoodId, masa: i64, koszt: i64, minuta: u64) -> BatchDraft {
    BatchDraft {
        good,
        mass: Mass(masa),
        quality: Q::new(60),
        brand: None,
        producer: FirmId(encja(0)),
        produced_at: SimMinute(minuta),
        cost: Money(koszt),
        origin: BatchOrigin::default(),
        flags: BatchFlags::default(),
    }
}

/// Magazyn z jednym pojemnym slotem zaplecza.
pub fn magazyn() -> (Catalog, Store, super::SlotId) {
    let cat = katalog();
    let mut s = Store::new(cat.goods.len());
    let slot = s.add_slot(
        SiteId(encja(1)),
        WarehouseRole::Backroom,
        StorageClass::Ambient,
        Mass(10_000_000),
        Volume(100_000_000),
        0,
    );
    (cat, s, slot)
}
