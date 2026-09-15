//! Stan gospodarki wchodzi do hasha świata (00 §3.6, bramka 2 fazy).
//!
//! Test nie sprawdza konkretnej wartości hasha — złoty odcisk fazy powstaje razem
//! ze scenariuszem headless w M5e. Sprawdza rzeczy, bez których tamten odcisk byłby
//! bezwartościowy: że ten sam stan daje ten sam hash, że **każdy** kanał zmiany
//! pieniądza go rusza, że rusza go zmiana ceny i stanu półki, i że indeks ofert —
//! jako pochodna areny — nie rusza.

mod common;

use magnat_core::{DecisionReason, Money, Qty, StockCat, Tick};
use magnat_economy::{
    AccountId, Books, ExternalInvestorId, LoanId, LostSaleTracking, TxKind, TxMemo,
};
use magnat_ecs::World;
use magnat_io::world_state_hash;

use common::{bench, first_good};

/// Świat z gospodarką: dwa sklepy z towarem na półkach i konta z emisją.
fn swiat(seed: u64) -> World {
    let mut b = bench(
        seed,
        &[
            magnat_spatial::Vec2::new(300.0, 0.0),
            magnat_spatial::Vec2::new(900.0, 0.0),
        ],
    );
    let good = first_good(
        &magnat_economy::EconomyData::load_default().unwrap(),
        StockCat::Food,
    );
    for s in &b.sites {
        b.market
            .deliver_now(*s, good, Qty(10_000), Money(2_000), None, Tick(0));
    }
    b.market.restock_shelves();
    b.books
        .transfer(
            b.rest,
            b.market.account_of(b.sites[0]).unwrap(),
            Money(1),
            TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified),
            Tick(1),
        )
        .unwrap();
    let mut w = World::new(seed);
    w.insert_resource(b.market.clone());
    w.insert_resource(std::mem::replace(&mut b.books, Books::new()));
    w.register_resource_hash::<Books>();
    w.register_resource_hash::<magnat_economy::Market>();
    w
}

#[test]
fn ten_sam_stan_daje_ten_sam_hash() {
    assert_eq!(world_state_hash(&swiat(7)), world_state_hash(&swiat(7)));
}

#[test]
fn kazdy_kanal_pieniadza_zmienia_hash() {
    let base = world_state_hash(&swiat(7));
    let acc = AccountId(0);

    for (nazwa, zmiana) in [
        ("emisja", 0u8),
        ("kredyt", 1),
        ("kapitał zewnętrzny", 2),
        ("sektor gospodarstw", 3),
    ] {
        let mut w = swiat(7);
        let books = w.get_resource_mut::<Books>().unwrap();
        let memo = TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified);
        match zmiana {
            0 => books.endow(acc, Money(1), Tick(2)).map(|_| ()),
            1 => books
                .create_credit(acc, Money(1), LoanId(1), Tick(2))
                .map(|_| ()),
            2 => books
                .inject_external_capital(acc, Money(1), ExternalInvestorId(1), Tick(2))
                .map(|_| ()),
            _ => books.household_pay(acc, Money(1), memo, Tick(2)).map(|_| ()),
        }
        .unwrap();
        assert_ne!(base, world_state_hash(&w), "{nazwa} nie ruszyła hasha");
    }
}

#[test]
fn zmiana_ceny_i_stanu_polki_zmienia_hash() {
    let data = magnat_economy::EconomyData::load_default().unwrap();
    let good = first_good(&data, StockCat::Food);
    let base = world_state_hash(&swiat(7));

    let w = swiat(7);
    let m = w.get_resource::<magnat_economy::Market>().unwrap();
    let site = m.sites()[0];
    assert!(m.set_price(site, good, Money(999)));
    assert_ne!(base, world_state_hash(&w), "cena nie ruszyła hasha");

    let w2 = swiat(7);
    let m2 = w2.get_resource::<magnat_economy::Market>().unwrap();
    m2.deliver_now(site, good, Qty(1_000), Money(200), None, Tick(0));
    m2.restock_shelves();
    assert_ne!(base, world_state_hash(&w2), "zapas nie ruszył hasha");
}

#[test]
fn sledzenie_utraconych_sprzedazy_nie_wchodzi_do_hasha() {
    // Poziom śledzenia ustawia `game/`, kiedy gracz przejmuje sklep. Gdyby wchodził
    // do hasha, kliknięcie „śledź" zmieniałoby świat — dokładnie to, przed czym
    // broni zasada „pomiar nie jest stanem" (M4 §5.4).
    let a = swiat(7);
    let b = swiat(7);
    let site = b
        .get_resource::<magnat_economy::Market>()
        .unwrap()
        .sites()[0];
    b.get_resource::<magnat_economy::Market>()
        .unwrap()
        .set_tracking(site, LostSaleTracking::Full);
    assert_eq!(world_state_hash(&a), world_state_hash(&b));
}

#[test]
fn indeks_ofert_nie_wchodzi_do_hasha() {
    // Indeks jest pochodną areny: dwa światy o tej samej arenie mają być
    // nieodróżnialne niezależnie od tego, czy indeks zdążono przebudować.
    let pool = magnat_jobs::JobPool::new(1);
    let a = swiat(7);
    let b = swiat(7);
    b.get_resource::<magnat_economy::Market>()
        .unwrap()
        .rebuild_index(&pool);
    assert_eq!(world_state_hash(&a), world_state_hash(&b));
}
