//! **Złoty test panelu sklepu** — kryterium WP12 (M5e §5.12).
//!
//! Scena jest zdaniem testowym fazy M5 z §1: gracz prowadzi sklep osiedlowy, podniósł
//! cenę mleka i po trzech dobach widzi w panelu spadek liczby klientów oraz utracone
//! sprzedaże z powodem „cena za wysoka". W zakładce Konkurencja stoi sklep 700 m dalej
//! z ceną mleka niższą o 12 % — obraz sprzed dwóch dób, bo tyle ma opóźnienia
//! obserwacja (§6.3), i panel mówi to graczowi wprost.
//!
//! Test biegnie bez GPU i bez świata: karta dostaje [`ShopPanelSnapshot`] jako dane,
//! bo migawka jest z definicji oderwana od stanu — to jedyne wejście interfejsu
//! do gospodarki.

use magnat_agents::SocialClass;
use magnat_core::{
    DecisionReason, DistrictId, Entity, FirmId, GoodId, Money, PlaceKind, PriceDriver, Qty,
    RejectCause, SimMinute, SiteId, Tick, UtilityKind,
};
use magnat_economy::{
    BalanceSheet, CashFlow, CompetitorRef, CompetitorRow, CustomerStats, FinanceSummary,
    IncomeStatement, LoanId, LostSale, LostSaleHistogram, LostSaleTracking, LostSalesView,
    PricePolicy, ShelfRow, ShopPanelSnapshot,
};
use magnat_ui::{Catalog, Locale, ShopCard, ShopTab, ShopView};
use std::num::NonZeroU32;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).unwrap())
}

const SKLEP: u32 = 12;
const FIRMA: u32 = 3;
/// „Dobry Koszyk" — sklep konkurenta 700 m dalej.
const KOSZYK: u32 = 77;

const CHLEB: GoodId = GoodId(0);
const MLEKO: GoodId = GoodId(3);
const SER: GoodId = GoodId(6);

/// Doba 40 świata, 17:30 — wieczorna migawka trzeciego dnia po podwyżce.
const TERAZ: Tick = Tick(40 * 1440 + 17 * 60 + 30);
/// Miesiąc gry ma 30 dób (`K-1`), więc bieżący okres zaczął się w dobie 30.
const OD: Tick = Tick(30 * 1440);

/// Uzupełnia pola, które migawka liczy sama. Test podaje cenę, koszt i stany, a marżę
/// i pokrycie liczy `ShelfRow` — inaczej złoty wydruk broniłby arytmetyki testu
/// zamiast arytmetyki gospodarki.
fn policzona(r: ShelfRow) -> ShelfRow {
    ShelfRow {
        margin_bp: ShelfRow::margin_of(r.price, r.unit_cost),
        days_of_cover: ShelfRow::cover_of(Qty(r.on_shelf.get() + r.backroom.get()), r.turnover_7d),
        ..r
    }
}

fn migawka(tracking: LostSaleTracking, cash_complete: bool) -> ShopPanelSnapshot {
    let site = SiteId(encja(SKLEP));
    let mut by_cause = [0u32; magnat_core::REJECT_CAUSE_COUNT];
    by_cause[RejectCause::PriceTooHigh.as_index()] = 34;
    by_cause[RejectCause::OutOfStock.as_index()] = 6;
    by_cause[RejectCause::TooFar.as_index()] = 2;

    let zdarzenie = |good, cause, gdzie| LostSale {
        citizen: magnat_core::CitizenId(encja(511)),
        good,
        when: Tick(TERAZ.0 - 90),
        cause,
        went_to: gdzie,
    };

    ShopPanelSnapshot {
        site,
        firm: FirmId(encja(FIRMA)),
        kind: PlaceKind::Grocery,
        at: TERAZ,
        tracking,
        shelves: vec![
            policzona(ShelfRow {
                good: CHLEB,
                price: Money(429),
                unit_cost: Money(331),
                on_shelf: Qty(18_000),
                backroom: Qty(24_000),
                turnover_7d: Qty(112_000),
                expires_at: Some(SimMinute(TERAZ.0 + 2 * 1440)),
                policy: PricePolicy::Dynamic {
                    target_margin_bp: 2_600,
                    floor_margin_bp: 800,
                    ceil_margin_bp: 6_000,
                },
                delegated: true,
                margin_bp: 0,
                days_of_cover: 0,
            }),
            // Mleko: gracz podniósł cenę z 2,99 zł na 3,49 zł i wyjął ją spod polityki.
            policzona(ShelfRow {
                good: MLEKO,
                price: Money(349),
                unit_cost: Money(272),
                on_shelf: Qty(31_000),
                backroom: Qty(60_000),
                turnover_7d: Qty(84_000),
                expires_at: Some(SimMinute(TERAZ.0 + 5 * 1440)),
                policy: PricePolicy::Fixed { price: Money(349) },
                delegated: false,
                margin_bp: 0,
                days_of_cover: 0,
            }),
            // Ser: nic się nie sprzedaje, więc pokrycie jest brakiem odpowiedzi.
            policzona(ShelfRow {
                good: SER,
                price: Money(1_890),
                unit_cost: Money(1_410),
                on_shelf: Qty(4_000),
                backroom: Qty(6_000),
                turnover_7d: Qty(0),
                expires_at: None,
                policy: PricePolicy::MatchCompetitor {
                    delta_bp: -200,
                    radius_m: 3_000,
                    reference: CompetitorRef::Cheapest,
                },
                delegated: true,
                margin_bp: 0,
                days_of_cover: 0,
            }),
        ],
        customers: CustomerStats {
            by_district: vec![(DistrictId(2), 150), (DistrictId(5), 91)],
            by_class: vec![
                (SocialClass::Working, 120),
                (SocialClass::LowerMiddle, 88),
                (SocialClass::Upper, 33),
            ],
            by_driver: vec![
                (UtilityKind::Price, 140),
                (UtilityKind::Distance, 70),
                (UtilityKind::Habit, 31),
            ],
            // Trzy ostatnie doby to skutek podwyżki — to jest ta liczba, na której
            // gracz widzi, co zrobił.
            daily: [41, 44, 43, 39, 28, 24, 22],
            total: 241,
        },
        lost_sales: LostSalesView {
            histogram: LostSaleHistogram { day: 40, by_cause },
            recent: vec![
                zdarzenie(CHLEB, RejectCause::OutOfStock, None),
                zdarzenie(
                    MLEKO,
                    RejectCause::PriceTooHigh,
                    Some(SiteId(encja(KOSZYK))),
                ),
                zdarzenie(
                    MLEKO,
                    RejectCause::PriceTooHigh,
                    Some(SiteId(encja(KOSZYK))),
                ),
            ],
        },
        competition: vec![CompetitorRow {
            site: SiteId(encja(KOSZYK)),
            distance_m: 700,
            // Mleko o 12 % taniej niż u nas, chleb drożej — obraz sprzed dwóch dób.
            prices: vec![(CHLEB, Money(449)), (MLEKO, Money(307))],
            observed_age_days: 2,
        }],
        finance: FinanceSummary {
            statement: IncomeStatement {
                revenue: Money(1_284_500),
                cogs: Money(931_200),
                wages: Money(180_000),
                rent: Money(60_000),
                utilities: Money(21_400),
                depreciation: Money(12_000),
                interest: Money(4_300),
                write_off: Money(18_700),
                tax: Money(10_400),
            },
            balance: BalanceSheet {
                cash: Money(240_000),
                bank: Money(610_000),
                inventory: Money(412_800),
                fixed_net: Money(900_000),
                assets: Money(2_162_800),
                trade_payable: Money(320_000),
                tax_payable: Money(10_400),
                wage_payable: Money(0),
                loans: Money(500_000),
                liabilities: Money(830_400),
                equity: Money(1_000_000),
                retained: Money(285_900),
                period_result: Money(46_500),
                equity_total: Money(1_332_400),
            },
            cash: CashFlow {
                operating: Money(92_300),
                investing: Money(-40_000),
                financing: Money(-12_800),
                net: Money(39_500),
                complete: cash_complete,
            },
            inventory_value: Money(412_800),
            loan: Some(LoanId(4)),
        },
        reprices: vec![DecisionReason::Repricing {
            site,
            good: MLEKO,
            driver: PriceDriver::Policy,
            delta_bp: 1_670,
        }],
        good_keys: vec![
            (CHLEB, "bread".to_string()),
            (MLEKO, "milk".to_string()),
            (SER, "cheese".to_string()),
        ],
    }
}

fn karta(c: &Catalog, l: Locale, s: &ShopPanelSnapshot) -> ShopCard {
    ShopCard::build(
        c,
        l,
        &ShopView {
            snapshot: s,
            kind: s.kind,
            period_from: OD,
        },
    )
}

fn wydruk(l: Locale) -> String {
    let c = Catalog::load().expect("data/locale/");
    karta(&c, l, &migawka(LostSaleTracking::Full, true)).render_text(&c, l)
}

fn sprawdz_wzorzec(l: Locale, plik: &str, wzorzec: &str) {
    let w = wydruk(l);
    if std::env::var("ZAPISZ_GOLDEN").is_ok() {
        std::fs::write(
            format!("{}/tests/golden/{plik}", env!("CARGO_MANIFEST_DIR")),
            &w,
        )
        .expect("zapis wzorca");
    }
    assert_eq!(
        w, wzorzec,
        "panel sklepu się zmienił.\n--- otrzymano ---\n{w}"
    );
}

#[test]
fn zloty_wydruk_panelu_po_polsku() {
    sprawdz_wzorzec(
        Locale::Pl,
        "shop_pl.txt",
        include_str!("golden/shop_pl.txt"),
    );
}

#[test]
fn zloty_wydruk_panelu_po_angielsku() {
    sprawdz_wzorzec(
        Locale::En,
        "shop_en.txt",
        include_str!("golden/shop_en.txt"),
    );
}

#[test]
fn kazda_zakladka_ma_tekst_w_obu_jezykach() {
    let c = Catalog::load().expect("data/locale/");
    let s = migawka(LostSaleTracking::Full, true);
    for l in Locale::ALL {
        let k = karta(&c, l, &s);
        for t in ShopTab::ALL {
            let w = k.render_tab(&c, l, t);
            assert!(!w.is_empty(), "{t:?} w {} jest pusta", l.code());
            assert!(
                !w.contains('{'),
                "{t:?} w {}: nietrafione podstawienie:\n{w}",
                l.code()
            );
        }
        assert!(!k.render_header(&c, l).contains('{'));
    }
}

#[test]
fn niesledzony_zaklad_mowi_ze_nie_ma_danych() {
    // Pusta tabela udawałaby zero klientów, a to co innego niż „nie wiemy" (`W-7`).
    let c = Catalog::load().expect("data/locale/");
    let s = migawka(LostSaleTracking::None, true);
    for l in Locale::ALL {
        let w = karta(&c, l, &s).render_tab(&c, l, ShopTab::Customers);
        assert!(
            w.contains(&c.fmt_key(l, "ui.shop.untracked", &[])),
            "{}: zakładka Klienci milczy o braku śledzenia:\n{w}",
            l.code()
        );
        assert!(
            !w.contains(&c.fmt_key(l, "ui.shop.lost", &[])),
            "{}: histogram utraconych sprzedaży udaje zera:\n{w}",
            l.code()
        );
    }
}

#[test]
fn obciety_dziennik_jest_widoczny() {
    // Liczba obcięta oknem dziennika wygląda jak liczba prawdziwa — nagłówek musi
    // powiedzieć wprost, że nią nie jest.
    let c = Catalog::load().expect("data/locale/");
    for l in Locale::ALL {
        let pelny = karta(&c, l, &migawka(LostSaleTracking::Full, true)).render_header(&c, l);
        let obciety = karta(&c, l, &migawka(LostSaleTracking::Full, false)).render_header(&c, l);
        let adnotacja = c.fmt_key(l, "ui.shop.fin.cash_partial", &[]);
        assert!(!pelny.contains(&adnotacja), "{}: fałszywy alarm", l.code());
        assert!(
            obciety.contains(&adnotacja),
            "{}: nagłówek przemilcza obcięcie:\n{obciety}",
            l.code()
        );
    }
}

#[test]
fn kazdy_towar_ma_nazwe_w_obu_jezykach() {
    // Lista jest kompletnym zestawem z `data/economy/retail.ron` — towar bez nazwy
    // wyszedłby dopiero jako surowy klucz na ekranie gracza.
    let c = Catalog::load().expect("data/locale/");
    let towary = [
        "bread",
        "potato",
        "vegetables",
        "milk",
        "fruit",
        "poultry_meat",
        "cheese",
        "meat",
        "sugar",
        "juice",
        "beer",
        "soap",
        "toilet_paper",
        "cosmetics",
        "detergent",
        "medicine",
        "clothing",
        "shoes",
    ];
    for t in towary {
        let k = c
            .key(&format!("ui.good.{t}"))
            .unwrap_or_else(|| panic!("brak klucza ui.good.{t}"));
        for l in Locale::ALL {
            assert!(!c.text(l, k).is_empty(), "ui.good.{t} w {}", l.code());
        }
    }
}
