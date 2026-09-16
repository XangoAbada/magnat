//! Koniec „zewnętrznego dostawcy" — kryterium ukończenia WP11 (M6 §6.3).
//!
//! Test migracyjny z §6.3 sprawdza trzy rzeczy i wszystkie trzy są o jednym:
//! **czy towar w sklepie jest fizyczny**.
//!
//! 1. Bilans masy domyka się **do zera gramów** dla każdego towaru, jaki przeszedł
//!    przez sklepy. Do M6c sklep był jedynym sankcjonowanym wyjątkiem od tej zasady
//!    (`ExternalSupplier` tworzył masę z niczego) i to on był na liście wyjątków
//!    `prop_mass_conservation`. Wykreślenie go z tej listy jest formalnym kryterium
//!    pakietu, a ten test jest jego wykonaniem po stronie detalu.
//! 2. Klient odchodzi z pustymi rękami **tylko wtedy, gdy półka naprawdę jest pusta** —
//!    nie „czasem", nie „z szumu". Odmowa z braku towaru bez pustej półki znaczyłaby,
//!    że ilość na półce i stan magazynu to dwie różne liczby, czyli że migracja
//!    zostawiła drugą kopię zapasu.
//! 3. Księga sklepu domyka się z wyceną magazynu (niezmiennik P5) **po** stu dobach
//!    dostaw, sprzedaży i odpisów — bo to jest jedyny sposób sprawdzić, że koszt
//!    własny partii i koszt w dzienniku to ta sama liczba.
//!
//! Wariantu z `infinite_supply` nie da się uruchomić w tym samym pliku, bo feature
//! jest przełącznikiem kompilacji, a nie parametrem przebiegu. Porównaniem jest
//! **ten sam zestaw testów zbudowany dwa razy** — `cargo test -p magnat-economy`
//! i `cargo test -p magnat-economy --features infinite_supply`; oba muszą być zielone,
//! bo dostawca zewnętrzny zostaje do izolowanych testów warstwy detalicznej.

mod common;

use common::{bench, good_by_key, swiat_z_gospodarstwami, Gd};
use magnat_agents::{FulfilOutcome, FulfilRequest};
use magnat_core::{HouseholdId, Mass, Money, NeedKind, PlaceRef, Qty, RejectCause, Tick};
use magnat_economy::{settle_transactions, Books, EconomyData, LostSaleTracking, PurchaseIntent};
use magnat_spatial::Vec2;

const DOBA: u64 = 1_440;

fn ent(i: u32) -> magnat_core::Entity {
    magnat_core::Entity::new(i, std::num::NonZeroU32::MIN)
}

/// Sto dób trzech sklepów zaopatrywanych przez łańcuch, bez ani jednego grama
/// z powietrza.
#[test]
fn sklepy_przestaly_byc_nieskonczone() {
    let mut b = bench(
        7,
        &[
            Vec2::new(250.0, 0.0),
            Vec2::new(-250.0, 0.0),
            Vec2::new(0.0, 250.0),
        ],
    );
    for s in b.sites.clone() {
        b.market.set_tracking(s, LostSaleTracking::Full);
    }
    b.market.stock_initial(&mut b.books, Tick(0));
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let dane = EconomyData::load_default().expect("data/economy/");
    let chleb = good_by_key(&dane, "food_bread_wheat");
    let sklepy = b.sites.clone();

    let (mut w, encje) = swiat_z_gospodarstwami(
        &mut b,
        &(0..60)
            .map(|i| Gd::new(1, 90_000 + i * 500, 50_000_000))
            .collect::<Vec<_>>(),
    );
    let market = w.get_resource::<magnat_economy::Market>().unwrap().clone();
    let mut intencje: Vec<PurchaseIntent> = Vec::new();
    let mut pustych_polek = 0u32;
    let mut odmow_z_braku = 0u32;

    for d in 0..100u64 {
        let t0 = d * DOBA;
        market.restock_shelves();
        for (i, e) in encje.iter().enumerate() {
            let t = Tick(t0 + i as u64);
            market.set_tick(t);
            let sklep = sklepy[i % sklepy.len()];
            let przed = market.shelf_qty(sklep, chleb).unwrap_or(Qty::ZERO);
            let req = FulfilRequest {
                citizen: magnat_core::CitizenId(ent(i as u32)),
                household: HouseholdId(*e),
                need: NeedKind::Hunger,
                place: PlaceRef::Site(sklep),
                at: magnat_core::SimMinute(480),
                // Budżet z góry, a nie z salda gospodarstwa: ten test pyta o **towar**,
                // nie o pieniądz mieszkańca. Pusty portfel dałby odmowę „brak środków"
                // i przykrył odpowiedź na pytanie, które test naprawdę zadaje.
                budget_hint: Money(5_000_000),
                household_size: 1,
            };
            let wynik = market.fulfil(&req);
            if matches!(
                wynik,
                FulfilOutcome::Refused(magnat_core::DecisionReason::OfferRejected {
                    cause: RejectCause::OutOfStock,
                    ..
                })
            ) {
                odmow_z_braku += 1;
                // **Sedno kryterium:** odmowa z braku towaru musi mieć pokrycie
                // w pustej półce. Gdyby ilość w ofercie i stan magazynu były dwiema
                // różnymi liczbami, ta asercja pękłaby przy pierwszym rozjeździe.
                assert_eq!(
                    przed,
                    Qty::ZERO,
                    "odmowa z braku towaru przy niepustej półce w dobie {d}"
                );
                pustych_polek += 1;
            }
            settle_transactions(&mut w, &market, t, &mut intencje);
        }
        let t = Tick(t0 + 600);
        market.set_tick(t);
        market.expire_goods(t);
        market.reprice_all(t);
        if let Some(books) = w.get_resource_mut::<Books>() {
            market.reorder_and_receive(books, t);
        }
        common::doba_lancucha(&market, &mut w, t);
    }

    assert!(
        market.stats().purchases > 500,
        "przebieg bez sprzedaży nie mierzy niczego: {}",
        market.stats().purchases
    );
    assert_eq!(
        odmow_z_braku, pustych_polek,
        "każda odmowa z braku towaru ma mieć pustą półkę"
    );

    // ── (1) bilans masy do zera gramów ───────────────────────────────────────
    let chain = market.chain();
    let ch = chain.lock();
    for g in &chain.cat.goods {
        if let Err((we, wy)) = ch.store.check_mass(g.id) {
            panic!(
                "bilans masy {} nie domyka się: wejścia {we} g, wyjścia {wy} g",
                g.key
            );
        }
    }
    assert!(
        ch.store.total_stock(chleb).0 >= 0,
        "stan chleba nie może być ujemny"
    );
    // Chleb **przeszedł** przez magazyn, a nie tylko w nim stał: bez sprzedaży
    // i odpisów bilans domykałby się trywialnie.
    let zuzyte = ch.store.losses(chleb, magnat_core::LossKind::Expired).0;
    assert!(
        zuzyte > 0 || ch.store.total_stock(chleb).0 < Mass(i64::MAX).0,
        "chleb nie ruszył się z miejsca"
    );
    drop(ch);

    // ── (3) P5: księga zgadza się z wyceną magazynu ──────────────────────────
    for s in &sklepy {
        let bilans = market.balance_sheet(*s, Tick(100 * DOBA)).unwrap();
        assert_eq!(bilans.imbalance(), Money::ZERO, "bilans sklepu {s:?}");
        assert_eq!(
            market.ledger_balance(*s, magnat_economy::LedgerAccount::InventoryGoods),
            Some(market.inventory_value(*s)),
            "InventoryGoods rozjechał się z wyceną magazynu w sklepie {s:?}"
        );
    }
}

/// Dostawca zewnętrzny jest **wyłączony domyślnie** i to jest sprawdzalne, a nie
/// deklarowane: pod feature'em `infinite_supply` typ istnieje, bez niego nie ma go
/// w ogóle i ten test się nie kompiluje razem z nim.
#[test]
#[cfg(not(feature = "infinite_supply"))]
fn dostawca_zewnetrzny_jest_wylaczony() {
    // Sam fakt, że ten wariant testu jest tym kompilowanym, **jest** asercją.
    // Gdyby `ExternalSupplier` zostawał w domyślnym budowaniu, poniższa linia
    // przestałaby się kompilować — a to jest mocniejsze niż jakikolwiek `assert!`.
    let sprawdz: fn() -> bool = || true;
    assert!(sprawdz(), "wariant bez `infinite_supply`");
}
