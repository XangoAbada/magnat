//! Wykonanie decyzji kwartalnych: otwarcie zakładu i dobrowolne zwinięcie firmy
//! (M7f WP13/WP15, §5.14).
//!
//! # Dobrowolne zwinięcie to nie upadłość
//!
//! Właściciel jednozakładowej firmy z trwale ujemnym wynikiem i pustą kasą wychodzi
//! **przed** syndykiem: zamyka zakład, zwalnia załogę z odprawą, a co zostanie na
//! rachunku, zabiera. Tańsze dla niego (nie płaci opłaty syndyka) i dla symulacji
//! (nie trzeba prowadzić postępowania). Postępowanie upadłościowe zostaje tam,
//! gdzie było — w `corpfin` (`K-10`), i ta droga go nie omija ani nie dubluje:
//! firma, która ma wierzycieli, nie zwija się dobrowolnie, bo `RequestVoluntaryClosure`
//! wymaga pustej kasy **i** trwałej straty, a wierzyciel doprowadza do niewypłacalności
//! wcześniej.

use magnat_core::{DecisionReason, DistrictId, Money, SiteId, Tick};
use magnat_ecs::World;
use magnat_firms::{FirmKey, Firms, Site, SitePlacement, SiteTypeCatalog};

use crate::market::{Market, ShopSeed};

use super::FirmLifeDay;

/// Otwiera zakłady, o które poprosił tier strategiczny.
pub fn open_all(
    world: &mut World,
    market: &Market,
    zlecenia: &[(FirmKey, DistrictId, u32, Money)],
    t: Tick,
) -> FirmLifeDay {
    let mut d = FirmLifeDay::default();
    for (key, district, slots, capex) in zlecenia {
        if open_one(world, market, *key, *district, *slots, *capex, t) {
            d.opened += 1;
        }
    }
    d
}

fn open_one(
    world: &mut World,
    market: &Market,
    key: FirmKey,
    district: DistrictId,
    slots: u32,
    capex: Money,
    t: Tick,
) -> bool {
    // Wzór lokalu bierze się z dzielnicy, a nie z katalogu — ten sam `ponytail:`
    // co przy powstawaniu firm: budowy nie ma, jest wejście do tego, co stoi.
    let Some(wzor) = market
        .shop_seeds()
        .into_iter()
        .find(|s| s.district == district.0)
    else {
        return false;
    };
    let Some(katalog) = world.get_resource::<SiteTypeCatalog>().cloned() else {
        return false;
    };
    let Some(mut firms) = world.get_resource_mut::<Firms>().map(std::mem::take) else {
        return false;
    };
    // Typ zakładu kopiuje się z tego, który firma już prowadzi: kwartał pytał
    // o „drugi taki sam", a nie o wejście w nową branżę. Wejście w nową branżę jest
    // dywersyfikacją i należy do M10 razem z przejęciami.
    let typ = firms
        .get(key)
        .and_then(|f| f.sites.first().copied())
        .and_then(|s| firms.site(s))
        .map(|s| s.site_type);
    let Some(typ) = typ else {
        *world.resource_mut::<Firms>() = firms;
        return false;
    };
    let nowy = super::founding::nowy_site_id(&firms, market);
    let site = Site::from_type(
        SitePlacement {
            id: nowy,
            building: magnat_core::BuildingId(wzor.site.0),
            district,
            floor_m2: POWIERZCHNIA_NA_ETAT.saturating_mul(slots.max(1)),
            opened: magnat_core::SimMinute(t.get()),
        },
        key,
        typ,
        katalog.get(typ),
        |_| WIDELKI,
    );
    let dodany = firms.add_site(site);
    *world.resource_mut::<Firms>() = firms;
    if !dodany {
        return false;
    }

    let firm_id = magnat_firms::firm_id(key);
    let Some(konto) = world.get_resource_mut::<crate::Books>().map(|b| {
        b.open_account(
            crate::books::AccountOwner::Firm(firm_id),
            crate::books::AccountKind::Current,
            None,
            Money::ZERO,
        )
    }) else {
        return false;
    };
    if !market.open_shop(
        ShopSeed {
            site: nowy,
            firm: firm_id,
            pos: wzor.pos,
            kind: wzor.kind,
            shelf_slots: wzor.shelf_slots,
            capacity_m3: wzor.capacity_m3,
            district: district.0,
        },
        konto,
        t,
    ) {
        return false;
    }

    // Nakład: przelew z rachunku firmy-matki na rachunek nowego zakładu. Firma,
    // której na to nie starcza, otwiera zakład bez kapitału obrotowego i **to jest
    // poprawny wynik** — jej tier operacyjny zobaczy to w pierwszym miesiącu.
    if let (Some(zrodlo), Some(books)) = (
        market.account_of_firm(firm_id),
        world.get_resource_mut::<crate::Books>(),
    ) {
        let kwota = Money(
            capex
                .get()
                .min(books.balance(zrodlo).unwrap_or(Money::ZERO).get()),
        );
        if kwota.get() > 0
            && books
                .transfer(
                    zrodlo,
                    konto,
                    kwota,
                    crate::books::TxMemo::new(
                        crate::books::TxKind::Endowment,
                        DecisionReason::Unspecified,
                    ),
                    t,
                )
                .is_ok()
        {
            market.record_capital(nowy, kwota, t);
        }
    }
    true
}

/// Zwija firmy, których właściciel się poddał.
pub fn wind_down_all(
    world: &mut World,
    market: &Market,
    klucze: &[FirmKey],
    t: Tick,
) -> FirmLifeDay {
    let mut d = FirmLifeDay::default();
    for key in klucze {
        let zaklady: Vec<SiteId> = world
            .get_resource::<Firms>()
            .and_then(|f| f.get(*key))
            .map_or_else(Vec::new, |f| f.sites.to_vec());
        if zaklady.is_empty() {
            continue;
        }
        for site in zaklady {
            // Ta sama jedyna droga wyjścia z etatu, którą chodzi zamknięcie zakładu
            // przez tier taktyczny i upadłość — inaczej niezmiennik „każdy
            // `Employment` zakończony dokładnie raz" miałby trzecią ścieżkę.
            crate::systems::close_site(world, market, site, t);
        }
        if let Some(f) = world
            .get_resource_mut::<Firms>()
            .and_then(|r| r.get_mut(*key))
        {
            f.status = magnat_firms::FirmStatus::Closed;
        }
        d.wound_down += 1;
    }
    d
}

/// Metry lokalu na jedno stanowisko. Wchodzi wyłącznie do kosztu stałego zakładu.
const POWIERZCHNIA_NA_ETAT: u32 = 18;

/// Widełki płacowe nowego zakładu — te same, których używa most M7a dla roli bez
/// własnego `Workplace`.
const WIDELKI: (Money, Money) = (Money(280_000), Money(520_000));
