//! Migawka panelu sklepu i kryterium WP12 (M5e §5.12).
//!
//! Kryterium fazy brzmi: *„pytanie «dlaczego Anna nie kupiła u mnie?» ma w panelu
//! odpowiedź dla ≥ 95 % mieszkańców, którzy w ostatnich 7 dniach byli kandydatami
//! i nie kupili"*. Zdanie jest łatwe do spełnienia **tożsamościowo** i dokładnie
//! tak było przed M5e: pierścień zapisywał wyłącznie wizyty, które doszły do
//! sklepu i tam się nie udały, więc mianownik był z definicji równy licznikowi,
//! a mieszkaniec, który porównał ceny i poszedł do konkurenta, w ogóle nie
//! wchodził do rachunku. Test mierzy **ten** zbiór: kto miał sklep wśród
//! kandydatów i u niego nie kupił.

mod common;

use magnat_agents::{ArrayVec, PlaceCandidate, PlaceProvider, MAX_CANDIDATES};
use magnat_core::{
    Money, NeedKind, PlaceRef, Qty, RejectCause, StockCat, Tick, UtilityKind, REJECT_CAUSE_COUNT,
};
use magnat_economy::{LostSaleTracking, ShelfRow};
use magnat_spatial::Vec2;

use common::{bench, buyer, first_good, knows, view};

/// Świat: dwa sklepy w różnej odległości, oba z towarem, oba znane.
/// Bliższy jest tani, dalszy drogi — kto wybierze dalszy, zrobi to dla jakości
/// albo z szumu, a to jest dokładnie ten rozkład, o który pyta kryterium.
fn dwa_sklepy() -> (common::Bench, magnat_core::GoodId) {
    let b = bench(11, &[Vec2::new(300.0, 0.0), Vec2::new(900.0, 0.0)]);
    let data = magnat_economy::EconomyData::load_default().unwrap();
    let good = first_good(&data, StockCat::Food);
    for s in &b.sites {
        b.market
            .deliver_now(*s, good, Qty(1_000_000), Money(200_000), None, Tick(0));
    }
    b.market.restock_shelves();
    // Drugi sklep drożej: ma przegrywać i ma o tym wiedzieć.
    b.market.set_price(b.sites[1], good, Money(400));
    b.market.set_price(b.sites[0], good, Money(260));
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
    (b, good)
}

#[test]
fn kazdy_kto_rozwazyl_sklep_i_nie_kupil_ma_powod() {
    let (b, _good) = dwa_sklepy();
    let sledzony = b.sites[1];
    b.market.set_tracking(sledzony, LostSaleTracking::Full);
    let wiedza = knows(&[PlaceRef::Site(b.sites[0]), PlaceRef::Site(b.sites[1])]);

    // Mianownik: ile razy śledzony sklep był kandydatem i **nie** został wybrany.
    // Licznik: ile wpisów utraconej sprzedaży ma za ten czas jego pierścień.
    let mut kandydowal_i_przegral = 0u32;
    for i in 0..400u32 {
        let kupujacy = buyer(i % 11, (i % 100) as u8);
        let mut out: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
        b.market.set_tick(Tick(u64::from(i)));
        b.market.candidates(
            NeedKind::Hunger,
            b.home,
            60,
            &view(&wiedza),
            &kupujacy.view(i),
            &mut out,
        );
        let byl = out.iter().any(|c| c.place == PlaceRef::Site(sledzony));
        let wygral = out.first().map(|c| c.place) == Some(PlaceRef::Site(sledzony));
        if byl && !wygral {
            kandydowal_i_przegral += 1;
        }
    }
    assert!(
        kandydowal_i_przegral > 50,
        "scena nie wytworzyła przegranych: {kandydowal_i_przegral}"
    );

    let h = b.market.lost_histogram(sledzony).expect("sklep istnieje");
    let zapisanych: u32 = h.by_cause.iter().sum();
    let pokrycie_permille = i64::from(zapisanych) * 1_000 / i64::from(kandydowal_i_przegral);
    assert!(
        pokrycie_permille >= 950,
        "panel odpowiada dla {} ‰ przegranych wyborów (próg 950 ‰): {zapisanych} z {kandydowal_i_przegral}",
        pokrycie_permille
    );

    // Odpowiedź ma być **odpowiedzią**, a nie samym faktem: powód musi wskazywać,
    // co gracz może z tym zrobić, a wpis — dokąd klient poszedł.
    assert!(
        h.by_cause[RejectCause::PriceTooHigh.as_index()] > 0,
        "sklep droższy o 54 % przegrywa, a histogram nie mówi, że przez cenę"
    );
    let ostatnie = b.market.lost_sales(sledzony);
    assert!(
        ostatnie.iter().all(|l| l.went_to.is_some()),
        "utracona sprzedaż z wyboru musi wskazywać, do kogo klient poszedł"
    );
}

#[test]
fn zaklad_niesledzony_nie_placi_za_ten_mechanizm_nic() {
    let (b, _) = dwa_sklepy();
    let wiedza = knows(&[PlaceRef::Site(b.sites[0]), PlaceRef::Site(b.sites[1])]);
    for i in 0..100u32 {
        let kupujacy = buyer(i % 11, 50);
        let mut out: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
        b.market.set_tick(Tick(u64::from(i)));
        b.market.candidates(
            NeedKind::Hunger,
            b.home,
            60,
            &view(&wiedza),
            &kupujacy.view(i),
            &mut out,
        );
    }
    for s in &b.sites {
        let h = b.market.lost_histogram(*s).unwrap();
        assert_eq!(
            h.by_cause, [0u32; REJECT_CAUSE_COUNT],
            "zakład bez oznaczenia zapisał utraconą sprzedaż"
        );
        assert!(b.market.lost_sales(*s).is_empty());
    }
}

#[test]
fn migawka_panelu_pokazuje_to_samo_co_rynek() {
    let (b, good) = dwa_sklepy();
    let site = b.sites[0];
    b.market.set_tracking(site, LostSaleTracking::Full);
    let snap = b
        .market
        .shop_panel(site, Tick(0), Tick(0))
        .expect("sklep istnieje");

    assert_eq!(snap.site, site);
    assert_eq!(snap.tracking, LostSaleTracking::Full);
    let wiersz = snap
        .shelves
        .iter()
        .find(|r| r.good == good)
        .expect("towar na półce");
    assert_eq!(Some(wiersz.price), b.market.price_at(site, good));
    assert_eq!(Some(wiersz.on_shelf), b.market.shelf_qty(site, good));
    assert_eq!(Some(wiersz.backroom), b.market.backroom_qty(site, good));
    assert_eq!(Some(wiersz.policy), b.market.policy_of(site, good));
    assert_eq!(
        snap.finance.inventory_value,
        b.market.inventory_value(site),
        "wartość zapasu w panelu rozjechała się z wyceną rynku (P5)"
    );
    // Nazwa towaru jedzie razem z migawką — interfejs nie ma skąd wziąć katalogu.
    assert!(!snap.good_key(good).is_empty());
    // Marża liczona z tych samych liczb, które migawka pokazuje obok.
    assert_eq!(
        wiersz.margin_bp,
        ShelfRow::margin_of(wiersz.price, wiersz.unit_cost)
    );
}

#[test]
fn obrot_tygodniowy_zbiera_siedem_dob_i_zapomina_osma() {
    let (b, good) = dwa_sklepy();
    let site = b.sites[0];
    b.market.set_tracking(site, LostSaleTracking::Full);

    // Osiem dób sprzedaży po jednej sztuce; ósma ma wypchnąć pierwszą.
    for d in 0..8u64 {
        let t = Tick(d * 1440 + 600);
        b.market.set_tick(t);
        b.market.record_sale(&magnat_economy::PurchaseIntent {
            buyer: magnat_core::CitizenId(common::ent(1)),
            household: magnat_core::HouseholdId(common::ent(2)),
            site,
            offer: b.market.offer_of(site, good).expect("oferta"),
            good,
            cat: StockCat::Food,
            qty: Qty(1_000),
            days: 1,
            agreed_price: Money(260),
            cogs: Money(200),
            arrived: t,
            reason: magnat_core::DecisionReason::ShopChosen {
                site,
                dominant: UtilityKind::Price,
                delta_bp: 0,
            },
            district: 3,
            status: magnat_core::Q::new(50),
        });
        // Domknięcie doby przepisuje sprzedaż do pierścienia tygodnia.
        b.market.reprice_all(Tick((d + 1) * 1440));
    }

    let snap = b
        .market
        .shop_panel(site, Tick(0), Tick(8 * 1440))
        .expect("sklep istnieje");
    let wiersz = snap.shelves.iter().find(|r| r.good == good).unwrap();
    assert_eq!(
        wiersz.turnover_7d,
        Qty(7_000),
        "okno tygodnia ma trzymać dokładnie siedem dób, nie osiem i nie sześć"
    );

    // Karta „Klienci": siedem dób zakupów, ostatnia pozycja to doba migawki.
    assert_eq!(snap.customers.total, 8);
    assert_eq!(snap.customers.daily.iter().sum::<u32>(), 7);
    assert_eq!(
        snap.customers.by_district,
        vec![(magnat_core::DistrictId(3), 8)]
    );
    assert_eq!(snap.customers.by_driver, vec![(UtilityKind::Price, 8)]);
}

#[test]
fn okno_utraconych_sprzedazy_wygasa_po_tygodniu() {
    let (b, _) = dwa_sklepy();
    let sledzony = b.sites[1];
    b.market.set_tracking(sledzony, LostSaleTracking::Histogram);
    let wiedza = knows(&[PlaceRef::Site(b.sites[0]), PlaceRef::Site(b.sites[1])]);

    // Doba 0: sto wyborów, w których śledzony przegrywa ceną.
    for i in 0..100u32 {
        let kupujacy = buyer(i % 11, 50);
        let mut out: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
        b.market.set_tick(Tick(u64::from(i)));
        b.market.candidates(
            NeedKind::Hunger,
            b.home,
            60,
            &view(&wiedza),
            &kupujacy.view(i),
            &mut out,
        );
    }
    let po_dobie: u32 = b
        .market
        .shop_panel(sledzony, Tick(0), Tick(600))
        .unwrap()
        .lost_sales
        .histogram
        .by_cause
        .iter()
        .sum();
    assert!(po_dobie > 0, "scena nie wytworzyła utraconych sprzedaży");

    // Osiem dób później okno ma być puste — pierścień pokazuje tydzień, nie historię.
    let pozniej: u32 = b
        .market
        .shop_panel(sledzony, Tick(0), Tick(8 * 1440))
        .unwrap()
        .lost_sales
        .histogram
        .by_cause
        .iter()
        .sum();
    assert_eq!(
        pozniej, 0,
        "okno siedmiu dób pokazuje zdarzenia sprzed ośmiu"
    );
}
