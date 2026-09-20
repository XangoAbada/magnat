//! Ubezpieczenia: wycena z historii, szkoda, wypłata (M10d WP10.12).
//!
//! Dwa kryteria z dokumentu podfazy kończą się w tym pliku:
//!
//! - **składka rośnie tam, gdzie się paliło i lało, bez żadnego parametru „ryzyko"
//!   w danych** — dowodem jest to, że dwie dzielnice różnią się **wyłącznie**
//!   przebiegiem zdarzeń, a nie ani jedną liczbą w `data/`;
//! - **przy małej ekspozycji jedna szkoda nie wywraca składki** — wygładzanie
//!   Bühlmanna ma tu test na liczbach, nie komentarz.
//!
//! Trzeci test jest o tym, czego przed M10d w grze nie było w ogóle: **zdarzenie
//! coś niszczy** (`GD-4`). Do tej podfazy sześć wariantów `Effect` zmieniało wyłącznie
//! parametry, a nakładka przywracała je przy wygaśnięciu.

mod common;

use common::{bench, ent, Bench, WHOLESALE_BASE};
use magnat_core::{
    DecisionReason, DistrictId, FirmId, LossKind, Money, PerilKind, Qty, SimMinute, SiteId, Tick,
};
use magnat_economy::insurance::{
    credibility_rate_bp, premium, system, Cover, InsuranceData, Insurers, PerilOccurrence,
};
use magnat_economy::{
    AccountKind, AccountOwner, Books, EconomyData, LedgerAccount, Market, TxKind, TxMemo,
};
use magnat_ecs::World;
use magnat_firms::{firm_id, Firm, Firms, Owner};
use magnat_spatial::Vec2;

const DOBA: u64 = 24 * 60;

fn towar() -> magnat_core::GoodId {
    common::first_good(
        &EconomyData::load_default().unwrap(),
        magnat_core::StockCat::Food,
    )
}

/// Dwa sklepy w dwóch dzielnicach, oba z zapasem.
fn dwa_sklepy() -> Bench {
    let b = bench(21, &[Vec2::new(100.0, 0.0), Vec2::new(900.0, 0.0)]);
    let g = towar();
    for s in &b.sites {
        b.market.deliver_now(
            *s,
            g,
            Qty(400 * 1_000),
            Money(400 * WHOLESALE_BASE),
            None,
            Tick(0),
        );
    }
    b.market.restock_shelves();
    b
}

/// Kryterium WP10.12, część pierwsza: składka po dwóch szkodach w dzielnicy jest
/// co najmniej dwa razy wyższa niż tam, gdzie nic się nie stało — i **nie ma
/// w danych ani jednej liczby, która by to przesądzała**.
///
/// Obie dzielnice czyta ten sam `InsuranceData`, ten sam prior i ten sam narzut.
/// Jedyną różnicą jest historia.
#[test]
fn skladka_rosnie_z_historii_a_nie_z_danych() {
    let mut ins = Insurers::new(InsuranceData::default());
    let nad_rzeka = DistrictId(3);
    let wyzej = DistrictId(4);
    let suma = Money(10_000_000);

    // Pięć lat ekspozycji w obu dzielnicach — identycznych.
    for _ in 0..60 {
        ins.note_exposure(PerilKind::Flood, nad_rzeka, suma);
        ins.note_exposure(PerilKind::Flood, wyzej, suma);
    }
    let przed = ins.rate_bp(PerilKind::Flood, nad_rzeka);
    assert_eq!(
        przed,
        ins.rate_bp(PerilKind::Flood, wyzej),
        "dzielnice bez różnicy w historii mają różną stawkę — gdzieś jest parametr ryzyka"
    );

    // Dwie powodzie po 30 % sumy ubezpieczenia, wyłącznie nad rzeką.
    ins.note_loss(PerilKind::Flood, nad_rzeka, Money(3_000_000));
    ins.note_loss(PerilKind::Flood, nad_rzeka, Money(3_000_000));

    let po = ins.rate_bp(PerilKind::Flood, nad_rzeka);
    let sucha = ins.rate_bp(PerilKind::Flood, wyzej);
    assert!(
        po >= 2 * sucha,
        "po dwóch powodziach stawka nad rzeką {po} nie jest dwukrotnością {sucha}"
    );

    let d = InsuranceData::default();
    let skladka_po = premium(suma, po, d.loading_bp, d.policy_fee()).get();
    let skladka_sucha = premium(suma, sucha, d.loading_bp, d.policy_fee()).get();
    // Opłata stała jest taka sama po obu stronach, więc porównuje się część
    // zależną od ryzyka — inaczej test mierzyłby stałą z pliku.
    assert!(
        skladka_po - d.policy_fee().get() >= 2 * (skladka_sucha - d.policy_fee().get()),
        "składka nie poszła za stawką: {skladka_po} vs {skladka_sucha}"
    );
}

/// Kryterium WP10.12, część druga: przy ekspozycji poniżej progu wiarygodności
/// jedna szkoda nie zmienia składki o rząd wielkości.
#[test]
fn mala_probka_nie_daje_skoku_o_rzad_wielkosci() {
    let d = InsuranceData::default();
    let prior = u32::from(d.prior_rate_bp);
    // Dwanaście polisomiesięcy — jeden zakład przez rok.
    let bez = credibility_rate_bp(Some(0), prior, 12, d.credibility_k);
    let po = credibility_rate_bp(Some(10_000), prior, 12, d.credibility_k);
    assert!(po > bez, "szkoda musi podnieść stawkę");
    assert!(
        po < 10 * bez,
        "stawka skoczyła o rząd wielkości: {bez} → {po}"
    );
}

/// Szkoda sprzed dekady nie trzyma składki w górze.
///
/// Okno obserwacji jest wykładnicze (`Insurers::age_one_month`), więc po
/// dostatecznej liczbie miesięcy bez szkód stawka wraca w okolice tej, którą płaci
/// dzielnica bez historii. Bez tego jedna powódź podnosiłaby składkę do końca gry
/// i mechanizm mierzyłby wiek świata, a nie ryzyko.
#[test]
fn stara_szkoda_wietrzeje() {
    let mut ins = Insurers::new(InsuranceData::default());
    let d = DistrictId(1);
    let suma = Money(10_000_000);
    for _ in 0..60 {
        ins.note_exposure(PerilKind::Flood, d, suma);
    }
    ins.note_loss(PerilKind::Flood, d, Money(3_000_000));
    let tuz_po = ins.rate_bp(PerilKind::Flood, d);

    // Dziesięć lat gry bez ani jednej szkody, z tą samą ekspozycją co przedtem.
    for _ in 0..120 {
        ins.age_one_month();
        ins.note_exposure(PerilKind::Flood, d, suma);
    }
    let po_dekadzie = ins.rate_bp(PerilKind::Flood, d);
    assert!(
        po_dekadzie < tuz_po / 2,
        "stawka po dekadzie bez szkód {po_dekadzie} nie zeszła o połowę wobec {tuz_po}"
    );
}

/// `GD-4`: zdarzenie **niszczy majątek**, a polisa go pokrywa.
///
/// Cały łańcuch: zgłoszenie z `sim/events` → odpis zapasu przez `Store::write_off`
/// (czyli bilans masy się domyka) → wypłata z polisy → pieniądz nie ginie.
#[test]
fn powodz_niszczy_zapas_a_polisa_placi() {
    let b = dwa_sklepy();
    let poszkodowany = b.sites[0];
    let dzielnica = b
        .market
        .district_of(poszkodowany)
        .expect("dzielnica sklepu");
    let zapas_przed = b.market.inventory_value(poszkodowany);
    assert!(zapas_przed.get() > 0, "sklep testowy nie ma zapasu");
    let straty_przed = b
        .market
        .chain()
        .lock()
        .store
        .losses(towar(), LossKind::Disaster);

    let mut w = World::new(3);
    magnat_agents::register_components(&mut w);
    w.insert_resource(b.books);
    w.insert_resource(b.market.clone());
    let mut firms = Firms::new();
    let ubezpieczyciel = firms.insert(|k| {
        Firm::sole_owner(
            k,
            "Zakład ubezpieczeń".to_string(),
            SimMinute(0),
            DistrictId(0),
            Owner::External,
        )
    });
    w.insert_resource(firms);
    let konto = konto_firmy(&mut w, firm_id(ubezpieczyciel), 500_000_000);
    b.market
        .register_plant(SiteId(ent(777)), firm_id(ubezpieczyciel), konto);

    let dane = InsuranceData::default();
    let mut ins = Insurers::new(dane);
    let suma = zapas_przed;
    ins.push_cover(Cover {
        id: magnat_core::CoverId(0),
        insurer: ubezpieczyciel,
        site: poszkodowany,
        district: dzielnica,
        peril: PerilKind::Flood,
        sum_insured: suma,
        deductible: dane.deductible(suma),
        premium_monthly: Money(1_000),
        rate_bp: 300,
        from: SimMinute(0),
        until: SimMinute(u64::MAX),
        ended: false,
    });
    ins.report_peril(PerilOccurrence {
        peril: PerilKind::Flood,
        district: Some(dzielnica),
        site: None,
        severity_bps: 10_000,
        day: 1,
    });
    system::register_insurers(&mut w, ins);

    let pieniadz_przed = w.resource::<Books>().total_balance();
    let raport = system::step_day(&mut w, Tick(DOBA));

    assert!(raport.damages >= 1, "powódź nie zniszczyła niczego");
    assert!(raport.loss.get() > 0);
    assert!(raport.paid.get() > 0, "polisa nie wypłaciła ani grosza");
    assert_eq!(
        w.resource::<Books>().total_balance(),
        pieniadz_przed,
        "odszkodowanie stworzyło albo zniszczyło pieniądz"
    );
    assert!(
        b.market.inventory_value(poszkodowany) < zapas_przed,
        "zapas nie zmalał"
    );
    let straty_po = b
        .market
        .chain()
        .lock()
        .store
        .losses(towar(), LossKind::Disaster);
    assert!(
        straty_po.0 > straty_przed.0,
        "masa zniknęła bez kategorii — bilans masy (00 §6) nie domyka się"
    );
    // Odszkodowanie zmniejsza odpis w księdze zakładu, a nie tworzy przychodu.
    let odpis = b
        .market
        .ledger_balance(poszkodowany, LedgerAccount::WriteOffExpense)
        .expect("księga zakładu");
    assert!(
        odpis.get() > 0 && odpis.get() < raport.loss.get(),
        "odszkodowanie nie pomniejszyło odpisu: odpis {odpis:?}, szkoda {:?}",
        raport.loss
    );
    // Udział własny zostaje po stronie poszkodowanego — inaczej polisa byłaby
    // darmowym pieniądzem, a każdy zakład ubezpieczałby się na maksimum.
    assert!(
        raport.paid.get() < raport.loss.get(),
        "polisa pokryła szkodę w całości mimo udziału własnego"
    );
    // Sąsiednia dzielnica nie ucierpiała.
    assert_eq!(
        b.market.inventory_value(b.sites[1]),
        zapas_przed,
        "szkoda wyszła poza swoją dzielnicę"
    );
}

fn konto_firmy(w: &mut World, firm: FirmId, kapital: i64) -> magnat_economy::AccountId {
    let rest = w.resource::<Market>().rest_of_world();
    let books = w.resource_mut::<Books>();
    let acc = books.open_account(
        AccountOwner::Firm(firm),
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    books
        .transfer(
            rest,
            acc,
            Money(kapital),
            TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
            Tick(0),
        )
        .expect("zasilenie ubezpieczyciela");
    acc
}
