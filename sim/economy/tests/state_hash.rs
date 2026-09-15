//! Stan gospodarki wchodzi do hasha świata (00 §3.6, bramka 2 fazy).
//!
//! Test nie sprawdza konkretnej wartości hasha — złoty odcisk fazy powstaje razem
//! ze scenariuszem headless w M5e. Sprawdza trzy rzeczy, bez których tamten odcisk
//! byłby bezwartościowy: że ten sam stan daje ten sam hash, że **każdy** kanał zmiany
//! pieniądza go rusza, i że indeks ofert — jako pochodna areny — nie rusza.

use std::num::NonZeroU32;

use magnat_core::{
    Arena, DecisionReason, Entity, FirmId, GoodId, Money, Qty, SiteId, StockCat, Tick, Q,
};
use magnat_economy::books::{
    AccountKind, AccountOwner, Books, ExternalInvestorId, LoanId, TxKind, TxMemo,
};
use magnat_economy::offer::{CategoryId, Offer, OfferIndex, PriceBasis};
use magnat_economy::register_economy;
use magnat_ecs::World;
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use magnat_spatial::{Aabb2, GridSpec, Vec2};

fn spec() -> GridSpec {
    GridSpec::covering(
        Aabb2::new(Vec2::new(0.0, 0.0), Vec2::new(4_000.0, 4_000.0)),
        200,
    )
}

fn offer(i: u32) -> Offer {
    let e = Entity::new(i, NonZeroU32::MIN);
    Offer {
        seller: FirmId(e),
        site: SiteId(e),
        good: GoodId(1),
        unit_price: Money(199 + i64::from(i)),
        price_basis: PriceBasis::GrossRetail,
        available: Qty(5_000),
        quality: Q::new(50),
        category: CategoryId::Stock(StockCat::Food),
        since: Tick(0),
        price_rev: 0,
    }
}

/// Świat z gospodarką: dwa konta z emisją i trzy oferty w arenie.
fn world() -> World {
    let mut w = World::new(7);
    register_economy(&mut w, spec());
    {
        let books = w.get_resource_mut::<Books>().unwrap();
        let rest = books.open_account(
            AccountOwner::RestOfWorld,
            AccountKind::Current,
            None,
            Money::ZERO,
        );
        let home = books.open_account(
            AccountOwner::Firm(FirmId(Entity::new(1, NonZeroU32::MIN))),
            AccountKind::Current,
            None,
            Money::ZERO,
        );
        books.endow(rest, Money(1_000_000), Tick(0)).unwrap();
        books
            .transfer(
                rest,
                home,
                Money(250_000),
                TxMemo::new(
                    TxKind::Wage {
                        site: SiteId(Entity::new(1, NonZeroU32::MIN)),
                    },
                    DecisionReason::Unspecified,
                ),
                Tick(1),
            )
            .unwrap();
    }
    {
        let arena = w.get_resource_mut::<Arena<Offer>>().unwrap();
        for i in 0..3 {
            arena.insert(offer(i));
        }
    }
    w
}

#[test]
fn ten_sam_stan_daje_ten_sam_hash() {
    assert_eq!(world_state_hash(&world()), world_state_hash(&world()));
}

#[test]
fn kazdy_kanal_pieniadza_zmienia_hash() {
    let base = world_state_hash(&world());
    let acc = magnat_economy::AccountId(0);

    let mut w = world();
    w.get_resource_mut::<Books>()
        .unwrap()
        .endow(acc, Money(1), Tick(2))
        .unwrap();
    assert_ne!(base, world_state_hash(&w), "emisja nie ruszyła hasha");

    let mut w = world();
    w.get_resource_mut::<Books>()
        .unwrap()
        .create_credit(acc, Money(1), LoanId(1), Tick(2))
        .unwrap();
    assert_ne!(base, world_state_hash(&w), "kredyt nie ruszył hasha");

    let mut w = world();
    w.get_resource_mut::<Books>()
        .unwrap()
        .inject_external_capital(acc, Money(1), ExternalInvestorId(1), Tick(2))
        .unwrap();
    assert_ne!(
        base,
        world_state_hash(&w),
        "kapitał zewnętrzny nie ruszył hasha"
    );
}

#[test]
fn zmiana_ceny_oferty_zmienia_hash() {
    let base = world_state_hash(&world());
    let mut w = world();
    let arena = w.get_resource_mut::<Arena<Offer>>().unwrap();
    let id = arena.iter().next().map(|(h, _)| h).unwrap();
    arena.get_mut(id).unwrap().set_price(Money(1));
    assert_ne!(base, world_state_hash(&w));
}

#[test]
fn indeks_ofert_nie_wchodzi_do_hasha() {
    // Indeks jest pochodną areny: dwa światy o tej samej arenie mają być
    // nieodróżnialne niezależnie od tego, czy indeks zdążono przebudować.
    let pool = JobPool::new(1);
    let a = world();
    let mut b = world();
    {
        let arena = b.get_resource::<Arena<Offer>>().unwrap();
        let mut idx = OfferIndex::new(spec());
        idx.mark_all_dirty();
        idx.rebuild(arena, |_| Vec2::ZERO, &pool);
        *b.get_resource_mut::<OfferIndex>().unwrap() = idx;
    }
    assert_eq!(world_state_hash(&a), world_state_hash(&b));
}
