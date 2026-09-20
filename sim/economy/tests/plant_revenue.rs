//! Utarg zakładu produkcyjnego (`R2-WP7`, pozycja 2 wykazu `R2`).
//!
//! Zakład bez półki nigdy nie dostawał wpisu w rachunku wyniku: `close_month_with`
//! iteruje po sklepach, a `Firms::post_revenue` ma w symulacji jednego wołającego —
//! tę pętlę. Skutek był cichy i dokładnie odwrotny do zamierzonego.
//! `SitePnlMonth::margin_bp()` zwraca przy zerowym utargu `None`, czyli „nie wiem",
//! a tier taktyczny AI **przerywa** liczenie miesięcy straty na pierwszym miesiącu
//! bez pomiaru. Fabryka nie miała pomiaru nigdy, więc była strukturalnie odporna
//! na zamknięcie, niezależnie od tego, jak długo przynosiła straty.
//!
//! Oba testy padały przed naprawą: pierwszy na `margin_bp() == None`, drugi na tym,
//! że tier taktyczny nie zwracał `CloseSite` po żadnej liczbie stratnych miesięcy.

mod common;

use std::collections::BTreeMap;

use common::{bench, ent, good_by_key, Bench};
use magnat_core::{
    DecisionReason, DistrictId, FirmId, GoodId, Mass, Money, SimMinute, SiteId, Tick, Q,
};
use magnat_economy::ai_run::AiInputs;
use magnat_economy::{AccountKind, AccountOwner, PublicMarketBoard, TxKind, TxMemo};
use magnat_firms::{
    Firm, FirmKey, FirmPersonality, Firms, Owner, Ring, Site, SitePnlMonth, SiteTypeId,
    StrategicOutlooks, Tier,
};
use magnat_policy::PolicyCatalog;
use magnat_supply::{SellerRef, Settlement};

const MIESIAC: u64 = 30 * 24 * 60;

/// Chleb: towar z katalogu detalicznego, bo `good_by_key` czyta `retail.ron`.
/// Dla tego testu liczy się wyłącznie to, że towar istnieje — mierzy on utarg
/// zakładu, a nie półkę.
fn maka(d: &magnat_economy::EconomyData) -> GoodId {
    good_by_key(d, "food_bread_wheat")
}

/// Zakład produkcyjny: bez półki, z kosztem stałym i z miejscem na rachunek wyniku.
fn zaklad(id: SiteId, firm: FirmKey) -> Site {
    Site {
        id,
        firm,
        site_type: SiteTypeId(0),
        building: magnat_core::BuildingId(ent(900)),
        district: DistrictId(0),
        floor_m2: 400,
        positions: Vec::new(),
        mgmt: magnat_firms::ManagementQuality::NEUTRAL,
        tech: Q::new(50),
        fixed_cost_month: Money(100_000),
        hr_accrued: Money::ZERO,
        rnd_accrued: Money::ZERO,
        pnl: Ring::<SitePnlMonth, 36>::new(),
        opened: SimMinute(0),
        strike_bps: 0,
        strike_bp_days: 0,
        delegation: None,
    }
}

/// Młyn: zakład w rejestrze firm i strona rozliczeń w rynku, bez półki.
struct Mlyn {
    site: SiteId,
    firma: FirmKey,
    firms: Firms,
}

/// Konto firmy z kapitałem — ta sama droga, którą `bench` zasila sklepy.
fn konto(b: &mut Bench, firm: FirmId, kapital: i64) -> magnat_economy::AccountId {
    let acc = b.books.open_account(
        AccountOwner::Firm(firm),
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    b.books
        .transfer(
            b.rest,
            acc,
            Money(kapital),
            TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
            Tick(0),
        )
        .unwrap();
    acc
}

fn mlyn(b: &mut Bench) -> Mlyn {
    let site = SiteId(ent(4_242));
    let mut firms = Firms::new();
    let firma = firms.insert(|k| {
        Firm::sole_owner(
            k,
            "Młyn".to_string(),
            SimMinute(0),
            DistrictId(0),
            Owner::External,
        )
    });
    if let Some(f) = firms.get_mut(firma) {
        f.set_personality(FirmPersonality::NEUTRAL);
    }
    assert!(firms.add_site(zaklad(site, firma)));
    let acc = konto(b, FirmId(site.entity()), 50_000_000);
    b.market.register_plant(site, FirmId(site.entity()), acc);
    Mlyn { site, firma, firms }
}

/// Wysyłka hurtowa z młyna: to, co po dostawie oddaje `sim/supply`.
fn wysylka(
    do_zakladu: SiteId,
    z_zakladu: SiteId,
    good: GoodId,
    netto: i64,
    koszt: i64,
) -> Settlement {
    Settlement {
        buyer: FirmId(do_zakladu.entity()),
        deliver_to: do_zakladu,
        seller: SellerRef::Firm(FirmId(z_zakladu.entity())),
        seller_site: Some(z_zakladu),
        seller_cogs: Money(koszt),
        good,
        mass: Mass(100_000),
        net: Money(netto),
        duty: Money::ZERO,
        order: None,
        reason: magnat_core::DecisionReason::Unspecified,
    }
}

/// Kryterium `R2-WP7`: młyn sprzedaje mąkę piekarni, a po domknięciu miesiąca jego
/// rachunek wyniku ma utarg i **umie policzyć marżę**.
///
/// Przed naprawą `margin_bp()` zwracało `None`, bo `revenue` zostawało zerem.
#[test]
fn mlyn_ma_utarg_i_marze_po_domknieciu_miesiaca() {
    let mut b = bench(31, &[]);
    let dane = magnat_economy::EconomyData::load_default().expect("data/economy/");
    let g = maka(&dane);
    let m = mlyn(&mut b);
    let piekarnia = SiteId(ent(4_243));
    let acc = konto(&mut b, FirmId(piekarnia.entity()), 50_000_000);
    b.market
        .register_plant(piekarnia, FirmId(piekarnia.entity()), acc);

    let wysylki = [
        wysylka(piekarnia, m.site, g, 2_000_000, 1_200_000),
        wysylka(piekarnia, m.site, g, 1_000_000, 700_000),
    ];
    b.market.absorb_settlements(&wysylki, &mut b.books, Tick(0));

    let mut fin = magnat_economy::corpfin::CorpFinance::default();
    let (_, wyniki) = b
        .market
        .close_month_with(&mut b.books, &mut fin, Tick(MIESIAC));
    let wpis = wyniki
        .iter()
        .find(|(s, _, _, _)| *s == m.site)
        .copied()
        .expect("młyn nie dostał wiersza w domknięciu miesiąca");
    assert_eq!(
        wpis.2,
        Money(3_000_000),
        "utarg nie jest sumą wysyłek netto"
    );
    assert_eq!(
        wpis.3,
        Money(1_900_000),
        "koszt własny nie jest sumą kosztów partii"
    );

    let mut firms = m.firms;
    assert!(
        firms.post_revenue(wpis.0, wpis.1, wpis.2, wpis.3),
        "zakładu nie ma w rejestrze firm"
    );
    let marza = firms
        .site(m.site)
        .and_then(|s| s.pnl.last())
        .and_then(SitePnlMonth::margin_bp);
    assert!(
        marza.is_some(),
        "młyn dalej nie ma pomiaru marży — `margin_bp()` zwraca „nie wiem”"
    );

    // Drugi miesiąc zaczyna się od zera: licznik jest miesięczny, nie narastający.
    let (_, wyniki2) = b
        .market
        .close_month_with(&mut b.books, &mut fin, Tick(2 * MIESIAC));
    assert!(
        !wyniki2.iter().any(|(s, _, _, _)| *s == m.site),
        "młyn dostał wpis za miesiąc, w którym nic nie wysłał"
    );
}

/// Kryterium `R2-WP7`, druga połowa: zakład produkcyjny z trwałą stratą **zostaje
/// zamknięty**. Przed naprawą nie zostawał zamknięty nigdy.
#[test]
fn trwale_stratny_zaklad_produkcyjny_zostaje_zamkniety() {
    let mut b = bench(37, &[]);
    let dane = magnat_economy::EconomyData::load_default().expect("data/economy/");
    let g = maka(&dane);
    let m = mlyn(&mut b);
    let piekarnia = SiteId(ent(4_243));
    let acc = konto(&mut b, FirmId(piekarnia.entity()), 500_000_000);
    b.market
        .register_plant(piekarnia, FirmId(piekarnia.entity()), acc);
    let mut firms = m.firms;
    let mut fin = magnat_economy::corpfin::CorpFinance::default();

    // Trzy miesiące sprzedaży poniżej kosztu wytworzenia — tyle, ile znosi
    // najcierpliwszy dyrektor (`FirmPersonality::loss_patience_months`).
    for i in 0..3u64 {
        let wysylki = [wysylka(piekarnia, m.site, g, 1_000_000, 1_500_000)];
        b.market
            .absorb_settlements(&wysylki, &mut b.books, Tick(i * MIESIAC));
        let (_, wyniki) =
            b.market
                .close_month_with(&mut b.books, &mut fin, Tick((i + 1) * MIESIAC));
        for (site, miesiac, utarg, koszt) in wyniki {
            firms.post_revenue(site, miesiac, utarg, koszt);
        }
    }

    let ostatnia = firms
        .site(m.site)
        .and_then(|s| s.pnl.last())
        .and_then(SitePnlMonth::margin_bp);
    assert!(
        ostatnia.is_some_and(|x| x < 0),
        "trzy miesiące poniżej kosztu, a marża to {ostatnia:?}"
    );

    // Decyzja idzie tą samą drogą co w symulacji — przez `run_firm_ai`, a nie przez
    // `decide_tactical` wołane wprost. Inaczej test sprawdzałby funkcję, a nie to,
    // czy fabryka faktycznie zostanie zamknięta.
    let mut board = PublicMarketBoard::new();
    b.market.refresh_board(&mut board, Tick(3 * MIESIAC));
    let cash: BTreeMap<SiteId, Money> = BTreeMap::new();
    let due = vec![
        (Tier::Operational, Vec::new()),
        (Tier::Tactical, vec![m.firma]),
        (Tier::Strategic, Vec::new()),
    ];
    let dzien = b.market.run_firm_ai(
        &mut firms,
        &AiInputs {
            board: &board,
            catalog: &PolicyCatalog::load_default().expect("data/policies/presets.ron"),
            outlooks: &StrategicOutlooks::new(),
            cash: &cash,
        },
        &due,
        Tick(3 * MIESIAC),
    );
    assert_eq!(
        dzien.to_close,
        vec![m.site],
        "tier taktyczny nie zamyka zakładu, który trzeci miesiąc sprzedaje poniżej kosztu"
    );
}
