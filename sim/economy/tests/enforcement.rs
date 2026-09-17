//! Szara strefa i ubytki inwentaryzacyjne po stronie zakładu (M8d WP7/WP8).
//!
//! Trzy kryteria z dokumentu fazy kończą się w tym pliku:
//!
//! - **T4 (b)** — posterunek obniża straty inwentaryzacyjne sklepów o ≥ 25 % i widać
//!   to jako pozycję w rachunku wyników (`WriteOffExpense`);
//! - **WP8** — szara strefa obniża **fakt**, a nie naliczenie: kolejka VAT-u sklepu
//!   maleje razem z przychodem, a gotówka zostaje;
//! - niezmiennik z 00 §6 — odpisana masa schodzi do `LossKind::Theft`, a nie znika.

mod common;

use magnat_core::{DistrictId, LossKind, Money, Qty, ServiceCoverage, ServiceKind, StockCat, Tick, Q};
use magnat_economy::{EconomyData, LedgerAccount, PurchaseIntent};
use magnat_spatial::Vec2;

use common::{bench, ent, first_good, Bench, WHOLESALE_BASE};

const KAT: StockCat = StockCat::Food;

fn towar() -> magnat_core::GoodId {
    first_good(&EconomyData::load_default().unwrap(), KAT)
}

/// Jeden sklep z towarem na półce i na zapleczu.
fn sklep(seed: u64, units: i64) -> Bench {
    let b = bench(seed, &[Vec2::new(100.0, 0.0)]);
    let g = towar();
    b.market.deliver_now(
        b.sites[0],
        g,
        Qty(units * 1_000),
        Money(units * WHOLESALE_BASE),
        None,
        Tick(0),
    );
    b.market.restock_shelves();
    b
}

fn pokrycie(policja: u8) -> ServiceCoverage {
    let mut c = ServiceCoverage::new(4);
    for d in 0..4u16 {
        c.set(DistrictId(d), ServiceKind::Police, Q::new(policja));
    }
    c
}

#[test]
fn posterunek_obniza_ubytki_i_widac_to_w_rachunku_wynikow() {
    // T4 (b). Dwa identyczne sklepy, jedyna różnica to pokrycie policyjne dzielnicy.
    let bez = sklep(1, 400);
    let z_policja = sklep(1, 400);

    let (n_bez, straty_bez) = bez.market.shrinkage(&pokrycie(0), 180, Tick(1_440));
    let (n_z, straty_z) = z_policja
        .market
        .shrinkage(&pokrycie(80), 180, Tick(1_440));

    assert_eq!(n_bez, 1, "sklep bez policji nie stracił nic");
    assert!(
        straty_bez.get() > 0,
        "kanał kradzieży martwy: odpis {straty_bez:?}"
    );
    assert!(
        straty_z.get() * 100 <= straty_bez.get() * 75,
        "bez policji {straty_bez:?}, z policją {straty_z:?} — spadek miał być ≥ 25 %"
    );
    let _ = n_z;

    // …i widać to jako pozycję w rachunku wyników, a nie tylko w statystyce rynku.
    let rzis = bez
        .market
        .ledger_balance(bez.sites[0], LedgerAccount::WriteOffExpense)
        .expect("księga zakładu");
    assert_eq!(
        rzis, straty_bez,
        "odpis nie trafił na `WriteOffExpense` w księdze zakładu"
    );

    // Masa zeszła z bilansu jako kradzież, a nie zniknęła (00 §6).
    let chain = bez.market.chain();
    let ch = chain.lock();
    assert!(
        ch.store.losses(towar(), LossKind::Theft).0 > 0,
        "masa odpisana bez kategorii `Theft`"
    );
}

#[test]
fn pelne_pokrycie_zdejmuje_kradziez_do_zera() {
    let b = sklep(2, 400);
    let (ile, straty) = b.market.shrinkage(&pokrycie(100), 180, Tick(1_440));
    assert_eq!((ile, straty), (0, Money::ZERO));
}

/// Silnik podatkowy testu: jedna stawka 23 % na wszystko.
///
/// Własna implementacja zamiast `CityTaxEngine`, bo `sim/economy` **nie może**
/// zależeć od `sim/city` — zależność idzie w drugą stronę. Trait jest publiczny
/// właśnie po to (`K-57`), a test potrzebuje niezerowej stawki, nie kodeksu.
struct Flat23;

impl magnat_economy::tax::TaxEngine for Flat23 {
    fn gross_from_net(&self, _g: magnat_core::GoodId, net: Money) -> Money {
        Money(net.get() * 123 / 100)
    }

    fn net_from_gross(&self, _g: magnat_core::GoodId, gross: Money) -> Money {
        Money(gross.get() * 100 / 123)
    }
}

#[test]
fn szara_strefa_obniza_fakt_a_nie_naliczenie() {
    // WP8: część utargu nie wchodzi ani do kolejki VAT-u, ani do przychodu w księdze,
    // ale **gotówka wpływa w całości** — inaczej `BankCurrent` rozjechałby się
    // z saldem rachunku w `Books`, czyli z pierwszym niezmiennikiem, który postawiło M5.
    let jawny = sklep(3, 400);
    let szary = sklep(3, 400);
    jawny.market.set_tax_engine(Box::new(Flat23));
    szary.market.set_tax_engine(Box::new(Flat23));
    assert!(szary.market.set_unreported_bps(szary.sites[0], 3_000));

    let g = towar();
    for b in [&jawny, &szary] {
        let offer = b.market.offer_of(b.sites[0], g).expect("oferta na półce");
        b.market.record_sale(&PurchaseIntent {
            buyer: magnat_core::CitizenId(ent(1)),
            household: magnat_core::HouseholdId(ent(2)),
            site: b.sites[0],
            offer,
            good: g,
            cat: KAT,
            qty: Qty(10_000),
            days: 1,
            agreed_price: Money(123_000),
            cogs: Money(60_000),
            taken: None,
            arrived: Tick(600),
            reason: magnat_core::DecisionReason::Unspecified,
            district: 0,
            status: Q::new(50),
        });
    }

    let vat_jawny = jawny.market.take_tax_accrued()[0].1;
    let vat_szary = szary.market.take_tax_accrued()[0].1;
    assert!(
        vat_jawny.vat.get() > 0,
        "sklep jawny nie naliczył VAT-u — test nic nie mierzy"
    );
    // 30 % obrotu poza deklaracją zdejmuje dokładnie 30 % podatku i 30 % podstawy.
    assert_eq!(vat_szary.vat.get() * 10, vat_jawny.vat.get() * 7);
    assert_eq!(vat_szary.vat_base.get() * 10, vat_jawny.vat_base.get() * 7);

    let przychod = |b: &Bench| {
        b.market
            .ledger_balance(b.sites[0], LedgerAccount::Revenue)
            .expect("księga zakładu")
            .get()
            .abs()
    };
    assert!(
        przychod(&szary) < przychod(&jawny),
        "przychód w księdze nie spadł: jawny {}, szary {}",
        przychod(&jawny),
        przychod(&szary)
    );
    // Gotówka wpłynęła w całości — to jest warunek, którego nie wolno złamać.
    let kasa = |b: &Bench| {
        b.market
            .ledger_balance(b.sites[0], LedgerAccount::BankCurrent)
            .expect("księga zakładu")
            .get()
    };
    assert_eq!(kasa(&jawny), kasa(&szary));
    // Reszta poszła do kapitału właściciela, a nie w powietrze. Porównanie dwóch
    // ksiąg, a nie wartość bezwzględna: `Equity` niesie też kapitał założycielski.
    let kapital = |b: &Bench| {
        b.market
            .ledger_balance(b.sites[0], LedgerAccount::Equity)
            .expect("księga zakładu")
            .get()
    };
    assert_eq!(
        kapital(&szary) - kapital(&jawny),
        -(123_000 * 3_000 / 10_000)
    );
}

#[test]
fn zakres_szarej_strefy_odpowiada_na_wynik_miesiaca() {
    // Ryzyko `R10`: udział szarej strefy ma być **wynikiem koniunktury**, a nie
    // liczbą z danych. Zakład bez ani jednego domkniętego miesiąca nie decyduje
    // o niczym — i to jest właściwa odpowiedź, a nie zero.
    let b = sklep(4, 100);
    assert!(b.market.update_shadow_share(600, 250, 4_500, 300).is_empty());
    assert_eq!(b.market.shadow_stats(), (0, 0));
}
