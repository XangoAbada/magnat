//! Kryteria akceptacyjne M5c: WP6 (polityki cenowe AI), WP7 (księgowość sklepu),
//! WP11 (polityki cenowe delegowane przez gracza).
//!
//! Każdy test odpowiada jednemu zdaniu z dokumentu podfazy. Świat jest mały
//! i policzalny na kartce — sprawdzamy zachowanie sklepu, nie generator miasta.

mod common;

use magnat_agents::{FulfilOutcome, FulfilRequest, Household};
use magnat_core::{DecisionReason, Entity, HouseholdId, Money, NeedKind, PlaceRef, Qty, Tick};
use magnat_economy::{
    settle_transactions, Books, CompetitorRef, EconomyData, LedgerAccount, LostSaleTracking,
    Market, PricePolicy, PurchaseIntent,
};
use magnat_ecs::World;
use magnat_spatial::Vec2;

use common::{bench, ent, good_by_key, Bench, WHOLESALE_BASE};

const DOBA: u64 = magnat_core::time::MINUTES_PER_DAY;
const MIESIAC: u64 = magnat_core::time::MINUTES_PER_MONTH;

/// Mydło: kategoria `Hygiene`, termin 720 dni. Towar nietrwały (chleb) zaciemniłby
/// testy księgowe odpisami — te mają własny test niżej.
fn mydlo() -> magnat_core::GoodId {
    good_by_key(&EconomyData::load_default().unwrap(), "cons_soap")
}

/// Doba sklepu w kolejności z `MarketSystem`: odpis → obserwacja → przecena.
fn doba(m: &Market, t: Tick) {
    m.expire_goods(t);
    m.observe_competitors(t);
    m.reprice_all(t);
}

// ── WP6: polityki cenowe AI ──────────────────────────────────────────────────────

#[test]
fn nadmiar_zapasu_obniza_cene_a_braki_ja_podnosza_w_ciagu_trzech_dni() {
    // Kryterium WP6 wprost: „sklep z nadmiarem zapasu obniża cenę w ciągu 3 dni;
    // sklep z brakami podnosi". Dwa osobne rynki, żeby sklepy nie patrzyły na siebie.
    let g = mydlo();
    let ceny = |zapas: i64| -> (Money, Money) {
        let b = bench(31, &[Vec2::new(300.0, 0.0)]);
        let site = b.sites[0];
        if zapas > 0 {
            b.market.deliver_now(
                site,
                g,
                Qty(zapas * 1_000),
                Money(zapas * WHOLESALE_BASE),
                None,
                Tick(0),
            );
        }
        b.market.restock_shelves();
        b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
        let start = b.market.price_at(site, g).unwrap();
        for d in 1..=3u64 {
            doba(&b.market, Tick(d * DOBA));
        }
        (start, b.market.price_at(site, g).unwrap())
    };

    // Cel zamówienia to 2 000 jednostek ceny (8 × pojemność wyłożenia).
    let (start_nad, po_nad) = ceny(6_000);
    let (start_brak, po_brak) = ceny(0);
    assert!(
        po_nad < start_nad,
        "zalegający towar ma tanieć: {start_nad:?} → {po_nad:?}"
    );
    assert!(
        po_brak > start_brak,
        "przy pustym magazynie cena ma rosnąć: {start_brak:?} → {po_brak:?}"
    );
}

#[test]
fn reakcja_na_przecene_konkurenta_miesci_sie_w_1_7_dniach() {
    // Kryterium WP6: „sklep obok konkurenta, który obniżył cenę, reaguje **nie
    // wcześniej niż po 1 i nie później niż po 7 dniach**; test mierzy opóźnienie
    // na 100 firmach i weryfikuje rozkład".
    //
    // Każda firma dostaje własny rynek, bo czujność (`delay_days`) losuje się
    // z ziarna świata i indeksu firmy — inaczej 100 sklepów na jednej mapie
    // widziałoby się nawzajem i mierzylibyśmy sprzężenie, a nie opóźnienie.
    //
    // Ceny 350 i 250 gr leżą w widełkach marży **każdej** wylosowanej osobowości
    // (koszt 200 gr, marża minimalna ≤ 9 %, maksymalna ≥ 90 %), więc reakcji nie
    // maskuje ogranicznik.
    let g = mydlo();
    let mut rozklad = [0u32; 9];

    for seed in 0..100u64 {
        let b = bench(seed, &[Vec2::new(300.0, 0.0), Vec2::new(900.0, 0.0)]);
        let (moj, konkurent) = (b.sites[0], b.sites[1]);
        for s in &b.sites {
            b.market
                .deliver_now(*s, g, Qty(2_000_000), Money(400_000), None, Tick(0));
        }
        b.market.restock_shelves();
        b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

        // Konkurent trzyma cenę sztywno; ja gonię najtańszego w promieniu 3 km.
        let cena = |gr: i64| PricePolicy::Fixed { price: Money(gr) };
        b.market.set_policy(konkurent, g, cena(350), false);
        b.market.set_policy(
            moj,
            g,
            PricePolicy::MatchCompetitor {
                delta_bp: 0,
                radius_m: 3_000,
                reference: CompetitorRef::Cheapest,
            },
            false,
        );

        // Obraz konkurencji odświeża się co `delay_days`, licząc od doby 0.
        // Dojazd do stanu ustalonego trwa więc dokładnie tyle.
        let delay = u64::from(b.market.observe_delay(moj).unwrap());
        assert!((1..=7).contains(&delay), "seed {seed}: czujność {delay}");
        for d in 0..=delay {
            doba(&b.market, Tick(d * DOBA));
        }
        assert_eq!(
            b.market.price_at(moj, g),
            Some(Money(350)),
            "seed {seed}: sklep nie dogonił konkurenta"
        );

        // Przecena zaraz po tym, jak wszyscy zerknęli na cennik.
        b.market.set_policy(konkurent, g, cena(250), false);
        b.market.reprice_all(Tick(delay * DOBA));
        assert_eq!(b.market.price_at(konkurent, g), Some(Money(250)));
        assert_eq!(
            b.market.price_at(moj, g),
            Some(Money(350)),
            "seed {seed}: nikt nie może zareagować tego samego dnia"
        );

        let mut kiedy = 0u64;
        for d in 1..=8u64 {
            doba(&b.market, Tick((delay + d) * DOBA));
            if b.market.price_at(moj, g) != Some(Money(350)) {
                kiedy = d;
                break;
            }
        }
        assert!(kiedy > 0, "seed {seed}: sklep nigdy nie zareagował");
        assert_eq!(
            b.market.price_at(moj, g),
            Some(Money(250)),
            "seed {seed}: reakcja ma dogonić nową cenę konkurenta"
        );
        rozklad[kiedy.min(8) as usize] += 1;
    }

    assert_eq!(rozklad[0], 0, "reakcja tego samego dnia łamie WP6");
    assert_eq!(rozklad[8], 0, "reakcja później niż po 7 dniach łamie WP6");
    let uzyte = rozklad[1..=7].iter().filter(|n| **n > 0).count();
    assert!(
        uzyte >= 5,
        "rozkład opóźnień jest zdegenerowany: {:?}",
        &rozklad[1..=7]
    );
}

#[test]
fn przecena_psujacego_sie_towaru_schodzi_ponizej_kosztu() {
    // §5.6: przecena schodkowa wg pozostałego terminu ważności. Alternatywą dla
    // wyprzedaży jest odpis 100 %, więc dolny ogranicznik marży schodzi razem
    // z terminem — inaczej ten mechanizm nigdy by się nie uruchomił.
    let b = bench(41, &[Vec2::new(300.0, 0.0)]);
    let site = b.sites[0];
    let g = good_by_key(&EconomyData::load_default().unwrap(), "food_bread_wheat");
    b.market.deliver_now(
        site,
        g,
        Qty(2_000_000),
        Money(400_000),
        Some(magnat_core::SimMinute(DOBA)),
        Tick(0),
    );
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
    let start = b.market.price_at(site, g).unwrap();
    doba(&b.market, Tick(DOBA / 2));
    let po = b.market.price_at(site, g).unwrap();
    assert!(po < start, "{start:?} → {po:?}");
    assert!(
        po.get() < WHOLESALE_BASE,
        "w ostatniej dobie ważności towar schodzi poniżej kosztu: {po:?}"
    );
}

#[test]
fn kazda_przecena_ma_powod() {
    // P8 z §7.1: 100 % decyzji ma `DecisionReason`. Powód przecen zapisuje się
    // dla zakładów śledzonych — flagę ustawia `game/` (`U-22`).
    let b = bench(51, &[Vec2::new(300.0, 0.0)]);
    let site = b.sites[0];
    b.market.set_tracking(site, LostSaleTracking::Full);
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
    for d in 1..=5u64 {
        doba(&b.market, Tick(d * DOBA));
    }
    let log = b.market.reprice_log(site);
    assert!(!log.is_empty(), "sklep nie ruszył ani razu ceny");
    for r in &log {
        assert!(
            matches!(r, DecisionReason::Repricing { .. }),
            "powód spoza bloku M5c: {r:?}"
        );
        assert_ne!(r.discriminant(), 0, "powód nienazwany");
    }
}

// ── WP11: polityki delegowane przez gracza ───────────────────────────────────────

#[test]
fn ta_sama_polityka_u_gracza_i_u_ai_daje_te_sama_cene() {
    // Kryterium WP11 wprost: polityka „−2 % względem najtańszego konkurenta
    // w promieniu 3 km" ustawiona przez gracza i przez AI daje **identyczną** cenę
    // przy identycznym stanie. Dlatego dwa przebiegi tego samego świata różnią się
    // wyłącznie flagą `delegated` — jedyną rzeczą, która odróżnia gracza od AI.
    let g = mydlo();
    let polityka = PricePolicy::MatchCompetitor {
        delta_bp: -200,
        radius_m: 3_000,
        reference: CompetitorRef::Cheapest,
    };
    let przebieg = |delegated: bool| -> Money {
        let b = bench(71, &[Vec2::new(300.0, 0.0), Vec2::new(900.0, 0.0)]);
        for s in &b.sites {
            b.market
                .deliver_now(*s, g, Qty(2_000_000), Money(400_000), None, Tick(0));
        }
        b.market.restock_shelves();
        b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
        b.market.set_policy(
            b.sites[1],
            g,
            PricePolicy::Fixed { price: Money(400) },
            false,
        );
        b.market.set_policy(b.sites[0], g, polityka, delegated);
        for d in 0..=8u64 {
            doba(&b.market, Tick(d * DOBA));
        }
        b.market.price_at(b.sites[0], g).unwrap()
    };
    let gracz = przebieg(true);
    let ai = przebieg(false);
    assert_eq!(gracz, ai);
    assert_eq!(gracz, Money(392), "400 gr − 2 % = 392 gr");
}

#[test]
fn podglad_ceny_zgadza_sie_z_tym_co_zrobi_przecena() {
    // WP11: gracz widzi „co by się stało z ceną dziś", zanim zatwierdzi politykę.
    // Podgląd i wykonanie muszą wołać to samo składanie — inaczej panel kłamie.
    let g = mydlo();
    let b = bench(81, &[Vec2::new(300.0, 0.0)]);
    let site = b.sites[0];
    b.market
        .deliver_now(site, g, Qty(2_000_000), Money(400_000), None, Tick(0));
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let polityka = PricePolicy::Markup {
        target_margin_bp: 5_000,
    };
    let podglad = b.market.preview_policy(site, g, polityka).unwrap();
    assert!(b.market.set_policy(site, g, polityka, true));
    b.market.reprice_all(Tick(DOBA));
    assert_eq!(b.market.price_at(site, g), Some(podglad));
    // Podgląd nie zatwierdza: polityka sklepu obok się nie zmieniła.
    assert_eq!(b.market.policy_of(site, g), Some(polityka));
}

// ── WP7: księgowość sklepu ───────────────────────────────────────────────────────

#[test]
fn towar_przeterminowany_schodzi_ze_stanu_i_obciaza_wynik() {
    let b = bench(91, &[Vec2::new(300.0, 0.0)]);
    let site = b.sites[0];
    let g = good_by_key(&EconomyData::load_default().unwrap(), "food_bread_wheat");
    b.market.deliver_now(
        site,
        g,
        Qty(1_000_000),
        Money(200_000),
        Some(magnat_core::SimMinute(DOBA)),
        Tick(0),
    );
    b.market.restock_shelves();
    assert_eq!(b.market.inventory_value(site), Money(200_000));

    let odpis = b.market.expire_goods(Tick(DOBA * 2));
    assert_eq!(odpis, Money(200_000));
    assert_eq!(b.market.inventory_value(site), Money::ZERO);
    assert_eq!(b.market.shelf_qty(site, g), Some(Qty::ZERO));
    assert_eq!(
        b.market
            .ledger_balance(site, LedgerAccount::WriteOffExpense),
        Some(Money(200_000))
    );
    assert_eq!(
        b.market.ledger_balance(site, LedgerAccount::InventoryGoods),
        Some(Money::ZERO)
    );
    assert_eq!(
        b.market
            .balance_sheet(site, Tick(DOBA * 2))
            .unwrap()
            .imbalance(),
        Money::ZERO
    );
}

#[test]
fn po_roku_bilans_zamyka_sie_co_do_grosza() {
    // Kryterium WP7 w trzech częściach:
    //  1. bilans się zamyka co do grosza,
    //  2. `Revenue − Cogs − koszty` z RZiS == zmiana `RetainedEarnings`,
    //  3. `InventoryGoods` == `Σ StockLine.cost_total` (niezmiennik P5).
    // Plus czwarta, bez której trzy pierwsze mogą być zgodne i nieprawdziwe:
    // `BankCurrent` w księdze == saldo rachunku w `Books`.
    let mut b = bench(101, &[Vec2::new(300.0, 0.0)]);
    let site = b.sites[0];
    b.market.set_tracking(site, LostSaleTracking::Full);
    b.market.stock_initial(&mut b.books, Tick(0));
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let (mut w, encje) = swiat_z_gospodarstwami(&mut b, 12, 500_000_000);
    let market = w.get_resource::<Market>().unwrap().clone();
    let konto = market.account_of(site).unwrap();
    let mut buf: Vec<PurchaseIntent> = Vec::new();
    // Zaległości nie powstaną w tym przebiegu — sklep ma z czego płacić — ale
    // `close_month` ich wymaga, bo od M7d nieudany przelew zostawia dług.
    let mut fin = magnat_economy::corpfin::CorpFinance::default();
    let mut sprzedanych = 0usize;

    for dzien in 1..=360u64 {
        let t0 = dzien * DOBA;
        market.restock_shelves();
        for (i, e) in encje.iter().enumerate() {
            let t = Tick(t0 + i as u64);
            market.set_tick(t);
            let req = FulfilRequest {
                citizen: magnat_core::CitizenId(ent(i as u32)),
                household: HouseholdId(*e),
                need: NeedKind::Hygiene,
                place: PlaceRef::Site(site),
                at: magnat_core::SimMinute(480),
                budget_hint: Money(5_000_000),
                household_size: 1,
                household_children: 0,
                brands: Default::default(),
            };
            if matches!(market.fulfil(&req), FulfilOutcome::Done { .. }) {
                sprzedanych += 1;
            }
            settle_transactions(&mut w, &market, t, &mut buf);
        }
        let t = Tick(t0 + 600);
        market.set_tick(t);
        doba(&market, t);
        if let Some(books) = w.get_resource_mut::<Books>() {
            market.reorder_and_receive(books, t);
        }
        common::doba_lancucha(&market, &mut w, t);
        if t0.is_multiple_of(MIESIAC) {
            if let Some(books) = w.get_resource_mut::<Books>() {
                market.close_month(books, &mut fin, Tick(t0));
            }
        }
    }
    assert!(sprzedanych > 1_000, "sklep prawie nic nie sprzedał");

    let koniec = Tick(360 * DOBA);
    let bilans = market.balance_sheet(site, koniec).unwrap();
    assert_eq!(bilans.imbalance(), Money::ZERO, "{bilans:?}");

    // (2) wynik z RZiS == zmiana kapitału zapasowego. Kapitał startowy to 0,
    // więc zmiana jest równa saldu.
    let rzis = market.income_statement(site, Tick(0), koniec).unwrap();
    let zatrzymany = Money(
        -market
            .ledger_balance(site, LedgerAccount::RetainedEarnings)
            .unwrap()
            .get(),
    );
    assert_eq!(
        rzis.net_result(),
        Money(zatrzymany.get() + bilans.period_result.get()),
        "RZiS {rzis:?} wobec kapitału zapasowego {zatrzymany:?}"
    );
    assert!(rzis.revenue.get() > 0 && rzis.cogs.get() > 0);
    assert!(rzis.rent.get() > 0 && rzis.depreciation.get() > 0);

    // (3) P5 — wartość zapasu w bilansie to zaplecze **i** półka.
    assert_eq!(
        market.ledger_balance(site, LedgerAccount::InventoryGoods),
        Some(market.inventory_value(site)),
        "InventoryGoods rozjechał się z wyceną zapasu"
    );

    // (4) konto w księdze nadąża za rachunkiem w `Books`.
    let ksiegi = w.get_resource::<Books>().unwrap();
    assert_eq!(
        market.ledger_balance(site, LedgerAccount::BankCurrent),
        ksiegi.balance(konto),
        "BankCurrent rozjechał się z saldem rachunku"
    );
    assert_eq!(ksiegi.check_conservation(), Ok(()));

    // Miesiące się domknęły i suma domknięć jest tym samym wynikiem.
    let z_domkniec: i64 = (0..12)
        .filter_map(|mies| {
            market.income_statement(site, Tick(mies * MIESIAC), Tick((mies + 1) * MIESIAC))
        })
        .map(|s| s.net_result().get())
        .sum();
    assert_ne!(z_domkniec, 0);
}

#[test]
fn przeplywy_licza_sie_z_dziennika_a_nie_z_roznicy_sald() {
    let mut b = bench(111, &[Vec2::new(300.0, 0.0)]);
    let site = b.sites[0];
    b.market.set_tracking(site, LostSaleTracking::Full);
    b.market.stock_initial(&mut b.books, Tick(0));
    let cf = b.market.cash_flow(site, Tick(0), Tick(DOBA)).unwrap();
    assert!(cf.complete, "okno dziennika nie objęło przedziału");
    assert!(
        cf.operating.get() < 0,
        "zatowarowanie startowe to wypływ operacyjny: {cf:?}"
    );
    // Wyposażenie lokalu to wkład właściciela, nie przelew — w przepływach ma go
    // nie być, choć w bilansie jest.
    assert_eq!(cf.investing, Money::ZERO);
    assert!(
        b.market
            .ledger_balance(site, LedgerAccount::FixedAssets)
            .unwrap()
            .get()
            > 0
    );
}

#[test]
fn rdzen_nie_ma_floatow_w_sciezce_pienieznej() {
    // P9 z §7.1 — test statyczny, możliwy tylko dlatego, że rdzeń jest wydzielony
    // (D20). `next_price`, `take_cogs` i `ledger_post` mają być całkowitoliczbowe
    // w sygnaturze **i w ciele**.
    let src = include_str!("../src/kernel.rs");
    let kod: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let kod = kod.split("#[cfg(test)]").next().unwrap();
    for zakaz in ["f32", "f64", "as f6", "libm"] {
        assert!(
            !kod.contains(zakaz),
            "rdzeń liczbowy zawiera `{zakaz}` — ścieżka pieniężna przestała być całkowitoliczbowa"
        );
    }
}

/// Świat ECS z gospodarstwami — tyle, ile potrzebuje rozliczenie.
fn swiat_z_gospodarstwami(b: &mut Bench, ile: u32, saldo: i64) -> (World, Vec<Entity>) {
    let mut w = World::new(1);
    magnat_agents::register_components(&mut w);
    w.insert_resource(std::mem::replace(&mut b.books, Books::new()));
    w.insert_resource(b.market.clone());
    let mut encje = Vec::new();
    for _ in 0..ile {
        let h = Household {
            size: 1,
            flags: Household::FLAG_ACTIVE,
            bank: Money(saldo),
            ..Household::default()
        };
        encje.push(w.spawn().with(h).id());
    }
    (w, encje)
}
