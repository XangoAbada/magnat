//! Kryteria akceptacyjne M5b: WP3 (sklep), WP4 (wybór oferty), WP5 (rozliczenie).
//!
//! Każdy test odpowiada jednemu zdaniu z dokumentu podfazy. Świat jest mały
//! i policzalny na kartce — testy sprawdzają zachowanie rynku, nie generator miasta.

mod common;

use std::collections::BTreeMap;

use magnat_agents::{
    ArrayVec, FulfilOutcome, FulfilRequest, Household, PlaceCandidate, PlaceProvider,
    MAX_CANDIDATES,
};
use magnat_core::{
    DecisionReason, Entity, HouseholdId, Money, NeedKind, PlaceRef, Qty, RejectCause, SiteId,
    StockCat, Tick, Q,
};
use magnat_economy::{settle_transactions, Books, EconomyData, Market, PurchaseIntent};
use magnat_ecs::World;
use magnat_spatial::Vec2;

use common::{bench, buyer, ent, first_good, knows, view, Bench, WHOLESALE_BASE};

const KAT: StockCat = StockCat::Food;

fn towar() -> magnat_core::GoodId {
    first_good(&EconomyData::load_default().unwrap(), KAT)
}

/// Sklepy z towarem na półce; `units` to liczba jednostek ceny (1000 milisztuk).
fn zatowarowany(seed: u64, pos: &[Vec2], units: i64) -> Bench {
    let b = bench(seed, pos);
    let g = towar();
    for s in &b.sites {
        b.market.deliver_now(
            *s,
            g,
            Qty(units * 1_000),
            Money(units * WHOLESALE_BASE),
            None,
            Tick(0),
        );
    }
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
    b
}

fn zadanie(citizen: u32, household: u32, site: SiteId, budzet: i64) -> FulfilRequest<'static> {
    FulfilRequest {
        citizen: magnat_core::CitizenId(ent(citizen)),
        household: HouseholdId(ent(household)),
        need: NeedKind::Hunger,
        place: PlaceRef::Site(site),
        at: magnat_core::SimMinute(480),
        budget_hint: Money(budzet),
        household_size: 1,
        brands: Default::default(),
    }
}

// ── WP3: sklep, magazyn, półka, zewnętrzny dostawca ──────────────────────────────

#[test]
fn polka_i_zaplecze_to_dwa_stany() {
    let b = bench(1, &[Vec2::new(300.0, 0.0)]);
    let (site, g) = (b.sites[0], towar());
    // Towar na zapleczu **nie jest** na sprzedaż: oferta widzi wyłącznie półkę.
    b.market
        .deliver_now(site, g, Qty(50_000), Money(10_000), None, Tick(0));
    assert_eq!(b.market.backroom_qty(site, g), Some(Qty(50_000)));
    assert_eq!(b.market.shelf_qty(site, g), Some(Qty::ZERO));

    b.market.restock_shelves();
    let na_polce = b.market.shelf_qty(site, g).unwrap();
    assert!(na_polce.get() > 0, "półka pozostała pusta");
    assert_eq!(
        b.market.backroom_qty(site, g).unwrap().get() + na_polce.get(),
        50_000,
        "przy przesunięciu na półkę sztuki nie mogą zniknąć ani się rozmnożyć"
    );
    // Koszt nabycia idzie za towarem — inaczej wycena zapasu (P5) nie miałaby sensu.
    assert_eq!(b.market.inventory_value(site), Money(10_000));
}

#[test]
fn zamowienie_u_dostawcy_kosztuje_i_dociera_po_czasie() {
    // **Kontrakt zmienił się w WP11 i test mówi teraz o tym, co naprawdę się dzieje.**
    // Do M6c sklep płacił przy zamówieniu, a towar materializował się po
    // `lead_time_days`. Od WP11 zamówienie otwiera zapytanie ofertowe, które może nie
    // znaleźć dostawcy — więc pieniądz wychodzi dopiero wtedy, gdy ciężarówka stanie
    // na rampie. Sklep, który zapłacił za towar, którego nikt nie przywiózł, miałby
    // dziurę w kasie bez zdarzenia, które by ją tłumaczyło.
    let mut b = bench(2, &[Vec2::new(300.0, 0.0)]);
    let (site, g) = (b.sites[0], towar());
    let konto = b.market.account_of(site).unwrap();
    let saldo0 = b.books.balance(konto).unwrap();
    let podaz0 = b.books.supply().total();
    let mut w = magnat_ecs::World::new(1);
    w.insert_resource(std::mem::replace(&mut b.books, Books::new()));

    b.market
        .reorder_and_receive(w.get_resource_mut::<Books>().unwrap(), Tick(0));
    // Zamówienie samo w sobie nie kosztuje: na tym etapie istnieje wyłącznie
    // zapytanie ofertowe.
    assert_eq!(b.market.backroom_qty(site, g), Some(Qty::ZERO));

    // Doba łańcucha: import z węzła granicznego jedzie ciężarówką na rampę sklepu.
    let doba = magnat_core::time::MINUTES_PER_DAY;
    for d in 0..8u64 {
        let t = Tick(d * doba);
        common::doba_lancucha(&b.market, &mut w, t);
        b.market
            .reorder_and_receive(w.get_resource_mut::<Books>().unwrap(), t);
    }

    let books = w.get_resource::<Books>().unwrap();
    assert!(
        books.balance(konto).unwrap() < saldo0,
        "dostawa musi kosztować"
    );
    assert_eq!(
        books.supply().total(),
        podaz0,
        "zakup u dostawcy zewnętrznego nie tworzy ani nie niszczy pieniądza"
    );
    assert_eq!(books.check_conservation(), Ok(()));
    assert!(
        b.market.backroom_qty(site, g).unwrap().get() > 0,
        "po ośmiu dobach towar ma stać na zapleczu"
    );
}

#[test]
fn zerwanie_dostaw_pustoszy_polke_ale_sklep_zostaje_widoczny() {
    // Kryterium WP3: oferta znika z kandydatów, ale sklep pozostaje widoczny
    // w inspekcji z powodem „brak towaru" (PRD §14.1).
    let mut b = zatowarowany(3, &[Vec2::new(300.0, 0.0)], 3);
    let (site, g) = (b.sites[0], towar());
    let kupujacy = buyer(1, 50);
    let wiedza = knows(&[PlaceRef::Site(site)]);

    let mut kupili = 0;
    for i in 0..10u32 {
        let mut out: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
        b.market.candidates(
            NeedKind::Hunger,
            b.home,
            60,
            &view(&wiedza),
            &kupujacy.view(i),
            &mut out,
        );
        let byl_kandydatem = out.iter().any(|c| c.place == PlaceRef::Site(site));
        match b.market.fulfil(&zadanie(i, 1, site, 1_000_000)) {
            FulfilOutcome::Done { .. } => {
                assert!(
                    byl_kandydatem,
                    "kupił w sklepie, którego nie było w wyborze"
                );
                kupili += 1;
            }
            FulfilOutcome::Refused(r) => {
                assert!(
                    !byl_kandydatem,
                    "sklep bez towaru nie ma prawa być kandydatem"
                );
                assert!(
                    matches!(
                        r,
                        DecisionReason::OfferRejected {
                            cause: RejectCause::OutOfStock,
                            ..
                        }
                    ),
                    "sklep ma powiedzieć, **dlaczego** nie sprzedał: {r:?}"
                );
            }
        }
    }
    assert_eq!(kupili, 3, "trzy jednostki to trzej klienci");
    assert_eq!(b.market.shelf_qty(site, g), Some(Qty::ZERO));
    assert!(
        b.market.sites().contains(&site),
        "sklep bez towaru nadal istnieje"
    );
    let _ = &mut b.books;
}

// ── WP4: funkcja użyteczności i wybór oferty ─────────────────────────────────────

fn ciag_wyborow(seed: u64) -> Vec<SiteId> {
    let b = zatowarowany(
        seed,
        &[
            Vec2::new(300.0, 0.0),
            Vec2::new(700.0, 0.0),
            Vec2::new(1_100.0, 0.0),
        ],
        500,
    );
    let wiedza = knows(
        &b.sites
            .iter()
            .map(|s| PlaceRef::Site(*s))
            .collect::<Vec<_>>(),
    );
    let mut wybory = Vec::new();
    for i in 0..200u32 {
        let kupujacy = buyer(i % 7, (i % 100) as u8);
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
        if let Some(c) = out.first() {
            if let PlaceRef::Site(s) = c.place {
                wybory.push(s);
            }
        }
    }
    wybory
}

#[test]
fn dwa_przebiegi_tego_samego_seeda_daja_ten_sam_ciag_wyborow() {
    let a = ciag_wyborow(11);
    assert!(!a.is_empty(), "nikt niczego nie wybrał — test jest pusty");
    assert_eq!(a, ciag_wyborow(11));
    // Inny seed ma dać **inny** ciąg, inaczej szum i softmax są ozdobą.
    assert_ne!(a, ciag_wyborow(12));
}

#[test]
fn wybor_nie_jest_argmaxem() {
    // Trzy sklepy o tej samej cenie, różniące się tylko odległością: przy softmaxie
    // najbliższy wygrywa **najczęściej**, ale nie zawsze (PRD §6.4, bramka G6).
    let wybory = ciag_wyborow(21);
    let mut ile: BTreeMap<u32, usize> = BTreeMap::new();
    for s in &wybory {
        *ile.entry(s.entity().index()).or_default() += 1;
    }
    assert!(
        ile.len() > 1,
        "wszyscy poszli do jednego sklepu — softmax zdegenerował się do argmaxu"
    );
}

#[test]
fn podniesienie_ceny_przesuwa_udzial_rynkowy_w_dol() {
    // Kryterium WP4: podniesienie ceny w jednym sklepie o 10 % przesuwa jego udział
    // monotonicznie w dół — dla każdej z 20 losowych populacji.
    let g = towar();
    let udzial = |populacja: u32, narzut_bp: i64| -> f64 {
        let b = zatowarowany(
            100 + u64::from(populacja),
            &[Vec2::new(400.0, 0.0), Vec2::new(500.0, 0.0)],
            20_000,
        );
        let bazowa = b.market.price_at(b.sites[0], g).unwrap();
        b.market.set_price(
            b.sites[0],
            g,
            Money(bazowa.get() * (10_000 + narzut_bp) / 10_000),
        );
        let wiedza = knows(&[PlaceRef::Site(b.sites[0]), PlaceRef::Site(b.sites[1])]);
        let mut u_nas = 0usize;
        let mut razem = 0usize;
        for i in 0..400u32 {
            let kupujacy = buyer(populacja * 1_000 + i, ((populacja * 7 + i) % 100) as u8);
            let mut out: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
            b.market.set_tick(Tick(u64::from(i)));
            b.market.candidates(
                NeedKind::Hunger,
                b.home,
                60,
                &view(&wiedza),
                &kupujacy.view(populacja * 1_000 + i),
                &mut out,
            );
            if let Some(PlaceRef::Site(s)) = out.first().map(|c| c.place) {
                razem += 1;
                if s == b.sites[0] {
                    u_nas += 1;
                }
            }
        }
        assert!(razem > 0);
        u_nas as f64 / razem as f64
    };

    let mut gorszych = 0;
    for p in 0..20u32 {
        let przed = udzial(p, 0);
        let po = udzial(p, 1_000);
        if po <= przed {
            gorszych += 1;
        }
    }
    assert_eq!(
        gorszych,
        20,
        "podwyżka ceny nie obniżyła udziału w {} z 20 populacji",
        20 - gorszych
    );
}

#[test]
fn kazda_decyzja_ma_powod() {
    // P8: 100 % decyzji ma `DecisionReason` (00 §7). Test przechodzi po wszystkich
    // trzech wyjściach `fulfil`: kupno, brak towaru, brak środków.
    let b = zatowarowany(31, &[Vec2::new(300.0, 0.0)], 4);
    let site = b.sites[0];
    // 0–3 kupują (cztery jednostki), 4 nie ma już czego kupić, 5 ma pusty portfel.
    let wyniki: Vec<FulfilOutcome> = (0..6u32)
        .map(|i| {
            let budzet = if i == 5 { 1 } else { 1_000_000 };
            if i == 5 {
                b.market
                    .deliver_now(site, towar(), Qty(4_000), Money(800), None, Tick(0));
                b.market.restock_shelves();
            }
            b.market.fulfil(&zadanie(i, i, site, budzet))
        })
        .collect();
    assert!(wyniki
        .iter()
        .any(|w| matches!(w, FulfilOutcome::Done { .. })));
    assert!(wyniki
        .iter()
        .any(|w| matches!(w, FulfilOutcome::Refused(_))));
    for w in &wyniki {
        let r = match w {
            FulfilOutcome::Done { reason, .. } | FulfilOutcome::Refused(reason) => *reason,
        };
        assert_ne!(r, DecisionReason::Unspecified, "decyzja bez powodu: {w:?}");
    }
    // Brak środków ma własny powód, odróżnialny od braku towaru.
    assert!(wyniki.iter().any(|w| matches!(
        w,
        FulfilOutcome::Refused(DecisionReason::OfferRejected {
            cause: RejectCause::BudgetExhausted,
            ..
        })
    )));
}

#[test]
fn utracona_sprzedaz_trafia_do_histogramu_tylko_gdy_ktos_slucha() {
    let b = zatowarowany(41, &[Vec2::new(300.0, 0.0), Vec2::new(400.0, 0.0)], 0);
    let (sledzony, cichy) = (b.sites[0], b.sites[1]);
    b.market
        .set_tracking(sledzony, magnat_economy::LostSaleTracking::Full);
    for i in 0..5u32 {
        b.market.fulfil(&zadanie(i, 1, sledzony, 1_000_000));
        b.market.fulfil(&zadanie(i, 1, cichy, 1_000_000));
    }
    assert_eq!(b.market.lost_sales(sledzony).len(), 5);
    assert_eq!(
        b.market.lost_histogram(sledzony).unwrap().by_cause[RejectCause::OutOfStock.as_index()],
        5
    );
    // Sklep AI bez śledzenia nie płaci za ten mechanizm nic.
    assert!(b.market.lost_sales(cichy).is_empty());
    assert_eq!(
        b.market.lost_histogram(cichy).unwrap().by_cause[RejectCause::OutOfStock.as_index()],
        0
    );
}

// ── WP5: rozliczanie transakcji ──────────────────────────────────────────────────

#[test]
fn wyscig_o_ostatnia_sztuke_konczy_sie_dokladnie_dziesiecioma_transakcjami() {
    // Kryterium WP5: 10 tys. agentów kierujących się do sklepu z 10 sztukami
    // na półce → dokładnie 10 transakcji, reszta ze zdarzeniem `Stockout`.
    let b = zatowarowany(51, &[Vec2::new(300.0, 0.0)], 10);
    let (site, g) = (b.sites[0], towar());
    let mut udane = 0usize;
    let mut braki = 0usize;
    for i in 0..10_000u32 {
        match b.market.fulfil(&zadanie(i, i, site, 1_000_000)) {
            FulfilOutcome::Done { .. } => udane += 1,
            FulfilOutcome::Refused(r) => {
                assert!(matches!(
                    r,
                    DecisionReason::OfferRejected {
                        cause: RejectCause::OutOfStock,
                        ..
                    }
                ));
                braki += 1;
            }
        }
        assert!(
            b.market.shelf_qty(site, g).unwrap().get() >= 0,
            "stan półki zszedł poniżej zera"
        );
    }
    assert_eq!((udane, braki), (10, 9_990));
    assert_eq!(b.market.take_intents().len(), 10);
}

/// Świat ECS z gospodarstwami — tyle, ile potrzebuje rozliczenie.
fn swiat_z_gospodarstwami(b: &mut Bench, ile: u32, saldo: i64) -> (World, Vec<Entity>) {
    let mut w = World::new(1);
    magnat_agents::register_components(&mut w);
    w.insert_resource(std::mem::replace(&mut b.books, Books::new()));
    w.insert_resource(b.market.clone());
    let mut encje = Vec::new();
    for i in 0..ile {
        let h = Household {
            size: 1,
            flags: Household::FLAG_ACTIVE,
            bank: Money(saldo),
            ..Household::default()
        };
        encje.push(w.spawn().with(h).id());
        let _ = i;
    }
    (w, encje)
}

#[test]
fn pieniadz_i_sztuki_zgadzaja_sie_po_obu_stronach() {
    // Wynik podfazy: mieszkaniec wychodzi po chleb, wybiera ofertę i wraca,
    // a po obu stronach zgadza się i pieniądz, i liczba sztuk.
    let mut b = zatowarowany(61, &[Vec2::new(300.0, 0.0)], 400);
    let (site, g) = (b.sites[0], towar());
    let (mut w, encje) = swiat_z_gospodarstwami(&mut b, 40, 500_000);
    let market = w.get_resource::<Market>().unwrap().clone();

    let pieniadz = |w: &World, encje: &[Entity]| -> i64 {
        let ksiegi: i64 = w.get_resource::<Books>().unwrap().total_balance().get();
        let gd: i64 = encje
            .iter()
            .filter_map(|e| w.get::<Household>(*e))
            .map(|h| h.cash.get() + h.bank.get() + h.savings.get())
            .sum();
        ksiegi + gd
    };
    let start = pieniadz(&w, &encje);
    let polka0 = market.shelf_qty(site, g).unwrap().get();

    let mut buf: Vec<PurchaseIntent> = Vec::new();
    let mut kupione = 0i64;
    for (i, e) in encje.iter().enumerate() {
        market.set_tick(Tick(i as u64));
        let req = FulfilRequest {
            citizen: magnat_core::CitizenId(ent(i as u32)),
            household: HouseholdId(*e),
            need: NeedKind::Hunger,
            place: PlaceRef::Site(site),
            at: magnat_core::SimMinute(480),
            budget_hint: Money(500_000),
            household_size: 1,
            brands: Default::default(),
        };
        if let FulfilOutcome::Done { spent, .. } = market.fulfil(&req) {
            assert!(spent.get() > 0);
        }
        kupione += settle_transactions(&mut w, &market, Tick(i as u64), &mut buf) as i64;
    }

    assert_eq!(kupione, 40, "każde gospodarstwo miało kupić raz");
    assert_eq!(
        pieniadz(&w, &encje),
        start,
        "suma pieniądza w świecie musi być niezmienna co do grosza"
    );
    assert_eq!(
        w.get_resource::<Books>().unwrap().check_conservation(),
        Ok(()),
        "niezmiennik P1 po stronie ksiąg"
    );
    // Kanał sektora gospodarstw niesie dokładnie to, co gospodarstwa zapłaciły.
    let kanal = w
        .get_resource::<Books>()
        .unwrap()
        .supply()
        .household_sector_in
        .get();
    assert_eq!(kanal, market.stats().revenue.get());
    assert!(kanal > 0);

    // Sztuki: ile zeszło z półki, tyle wpłynęło do gospodarstw jako dni zapasu.
    let zeszlo = polka0 - market.shelf_qty(site, g).unwrap().get();
    assert_eq!(zeszlo, market.stats().purchased_qty);
    let dni: u32 = encje
        .iter()
        .filter_map(|e| w.get::<Household>(*e))
        .map(|h| u32::from(h.stock[KAT.as_index()]))
        .sum();
    assert!(dni > 0, "zapas gospodarstw nie urósł");
}

#[test]
fn brak_srodkow_przy_rozliczeniu_oddaje_towar_na_polke() {
    // Ścieżka, w której świat mógłby stracić i pieniądz, i towar naraz. Nie może.
    let mut b = zatowarowany(71, &[Vec2::new(300.0, 0.0)], 50);
    let (site, g) = (b.sites[0], towar());
    let (mut w, encje) = swiat_z_gospodarstwami(&mut b, 1, 0);
    let market = w.get_resource::<Market>().unwrap().clone();
    let polka0 = market.shelf_qty(site, g).unwrap();

    // `budget_hint` kłamie — gospodarstwo nie ma ani grosza.
    let req = FulfilRequest {
        citizen: magnat_core::CitizenId(ent(1)),
        household: HouseholdId(encje[0]),
        need: NeedKind::Hunger,
        place: PlaceRef::Site(site),
        at: magnat_core::SimMinute(480),
        budget_hint: Money(999_999),
        household_size: 1,
        brands: Default::default(),
    };
    assert!(matches!(market.fulfil(&req), FulfilOutcome::Done { .. }));
    assert!(market.shelf_qty(site, g).unwrap() < polka0);

    let mut buf = Vec::new();
    assert_eq!(settle_transactions(&mut w, &market, Tick(1), &mut buf), 0);
    assert_eq!(
        market.shelf_qty(site, g).unwrap(),
        polka0,
        "towar musi wrócić na półkę"
    );
    assert_eq!(
        w.get_resource::<Books>().unwrap().check_conservation(),
        Ok(())
    );
}

#[test]
fn gospodarstwo_nie_wydaje_dwa_razy_tego_samego_budzetu_w_jednej_minucie() {
    // Dwa zakupy tej samej minuty widziałyby ten sam `budget_hint` dwa razy, gdyby
    // rynek nie zaklepywał kwot. Trzeci zakup ma odpaść na braku środków.
    let b = zatowarowany(81, &[Vec2::new(300.0, 0.0)], 100);
    let site = b.sites[0];
    let g = towar();
    let cena = b.market.price_at(site, g).unwrap().get();
    let budzet = cena * 2 + cena / 2;
    let wyniki: Vec<bool> = (0..3u32)
        .map(|i| {
            matches!(
                b.market.fulfil(&zadanie(i, 7, site, budzet)),
                FulfilOutcome::Done { .. }
            )
        })
        .collect();
    assert_eq!(wyniki, vec![true, true, false]);
}

#[test]
fn stan_polki_i_oferty_nigdy_nie_schodzi_ponizej_zera() {
    // P3 na całej ścieżce: kupno, zwrot, uzupełnienie, kupno.
    let b = zatowarowany(91, &[Vec2::new(300.0, 0.0)], 4);
    let (site, g) = (b.sites[0], towar());
    for i in 0..50u32 {
        b.market.fulfil(&zadanie(i, i, site, 1_000_000));
        let q = b.market.shelf_qty(site, g).unwrap();
        assert!(q.get() >= 0, "półka: {q:?}");
        assert!(b.market.backroom_qty(site, g).unwrap_or(Qty::ZERO).get() >= 0);
        if i % 10 == 0 {
            b.market.restock_shelves();
        }
    }
}

#[test]
fn zaspokojenie_jest_proporcjonalne_do_kupionych_dni() {
    // `Z-2`: `satisfaction` jest **przyrostem** i zależy od tego, ile udało się kupić.
    let b = zatowarowany(101, &[Vec2::new(300.0, 0.0)], 1);
    let site = b.sites[0];
    let pelne = match b.market.fulfil(&zadanie(1, 1, site, 1_000_000)) {
        FulfilOutcome::Done { satisfaction, .. } => satisfaction,
        other => panic!("{other:?}"),
    };
    assert!(pelne > Q::new(0));
    assert!(pelne <= Q::new(100));
}
