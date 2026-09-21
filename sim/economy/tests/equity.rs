//! Giełda: sesja, progi, przejęcie, obrona (M10d WP10.10 i WP10.11).
//!
//! Testy chodzą **przez `step_day`**, a nie przez same funkcje czyste: kryteria
//! WP10.10 i WP10.11 mówią o pieniądzu i o akcjonariacie, a jedno i drugie rusza się
//! dopiero w systemie. Same funkcje (fixing, racjonowanie, podział dywidendy, emisja)
//! mają testy jednostkowe przy sobie — tu sprawdza się, czy to, co one liczą,
//! **naprawdę dociera do ksiąg**.

mod common;

use common::{bench, ent, Bench};
use magnat_core::{
    CitizenId, DecisionReason, DistrictId, FirmId, Money, SimMinute, SiteId, Subject, Tick, Q,
};
use magnat_economy::equity::{
    self, book::Side, system, Equity, EquityParams, CONTROL_BP, DISCLOSURE_BP,
};
use magnat_economy::{AccountId, AccountKind, AccountOwner, Books, Market, TxKind, TxMemo};
use magnat_ecs::World;
use magnat_firms::{
    firm_id, Firm, FirmKey, FirmPersonality, Firms, ManagementQuality, Owner, Ring, Site,
    SitePnlMonth, SiteTypeId,
};

const DOBA: u64 = 24 * 60;

/// Świat z rynkiem, księgami i rejestrem firm — bez mieszkańców.
///
/// Inwestorami są tu firmy, bo giełda ma je obie obsłużyć tą samą drogą, a firma
/// ma konto w księgach i nie wymaga stawiania gospodarstwa z tożsamością.
fn swiat(b: Bench) -> World {
    let mut w = World::new(7);
    magnat_agents::register_components(&mut w);
    w.insert_resource(b.books);
    w.insert_resource(b.market);
    w.insert_resource(Firms::new());
    w
}

fn zaklad(id: SiteId, firm: FirmKey, district: u16) -> Site {
    Site {
        id,
        firm,
        site_type: SiteTypeId(0),
        building: magnat_core::BuildingId(ent(900)),
        district: DistrictId(district),
        floor_m2: 200,
        positions: Vec::new(),
        mgmt: ManagementQuality::NEUTRAL,
        tech: Q::new(50),
        fixed_cost_month: Money(10_000),
        hr_accrued: Money::ZERO,
        rnd_accrued: Money::ZERO,
        pnl: Ring::<SitePnlMonth, 36>::new(),
        opened: SimMinute(0),
        strike_bps: 0,
        strike_bp_days: 0,
        delegation: None,
        shift_profile: magnat_agents::ShiftProfile::Office,
    }
}

/// Zakłada firmę, daje jej konto z kapitałem i wiąże z zakładem w rynku.
fn firma(w: &mut World, nazwa: &str, site: SiteId, kapital: i64) -> FirmKey {
    let key = {
        let firms = w.resource_mut::<Firms>();
        let k = firms.insert(|k| {
            Firm::sole_owner(
                k,
                nazwa.to_string(),
                SimMinute(0),
                DistrictId(0),
                Owner::External,
            )
        });
        if let Some(f) = firms.get_mut(k) {
            f.set_personality(FirmPersonality::NEUTRAL);
        }
        assert!(firms.add_site(zaklad(site, k, 0)));
        k
    };
    let konto = konto_firmy(w, firm_id(key), kapital);
    let market = w.resource::<Market>().clone();
    market.register_plant(site, firm_id(key), konto);
    key
}

fn konto_firmy(w: &mut World, firm: FirmId, kapital: i64) -> AccountId {
    let rest = w.resource::<Market>().rest_of_world();
    let books = w.resource_mut::<Books>();
    let acc = books.open_account(
        AccountOwner::Firm(firm),
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    if kapital > 0 {
        books
            .transfer(
                rest,
                acc,
                Money(kapital),
                TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
                Tick(0),
            )
            .expect("zasilenie konta firmy");
    }
    acc
}

fn saldo(w: &World, firm: FirmKey) -> Money {
    let market = w.resource::<Market>().clone();
    let Some(a) = market.account_of_firm(firm_id(firm)) else {
        return Money::ZERO;
    };
    w.resource::<Books>()
        .account(a)
        .map_or(Money::ZERO, |x| x.balance())
}

fn pieniadz(w: &World) -> Money {
    w.resource::<Books>().total_balance()
}

/// Parametry bez szumu — testy sesji mierzą mechanikę, nie rozkład przekonań.
fn bez_szumu() -> EquityParams {
    EquityParams {
        noise_household_bp: 0,
        noise_firm_bp: 0,
        earnings_noise_bp: 0,
        ..EquityParams::default()
    }
}

/// Kryteria WP10.10: suma akcji w obiegu się nie zmienia, a pieniądz nie ginie.
#[test]
fn sesja_przenosi_udzial_i_pieniadz_bez_zgubienia_grosza() {
    let b = bench(11, &[]);
    let mut w = swiat(b);
    let spolka = firma(&mut w, "Spółka", SiteId(ent(501)), 0);
    let kupiec = firma(&mut w, "Kupiec", SiteId(ent(502)), 100_000_000);

    let mut eq = Equity::new(bez_szumu());
    assert!(eq.list(spolka, 2_500, Money(1_000), SimMinute(0)));
    // Dotychczasowy właściciel wystawia pakiet, kupiec go bierze po tej samej cenie.
    eq.place_order(
        spolka,
        Owner::External,
        Side::Sell,
        Money(1_000),
        2_000,
        SimMinute(u64::MAX),
    );
    eq.place_order(
        spolka,
        Owner::Firm(kupiec),
        Side::Buy,
        Money(1_000),
        2_000,
        SimMinute(u64::MAX),
    );
    system::register_equity(&mut w, eq);

    let przed = pieniadz(&w);
    let raport = system::step_day(&mut w, Tick(DOBA));
    assert_eq!(raport.fixings, 1, "sesja się nie odbyła");
    assert_eq!(raport.volume_bp, 2_000);
    assert_eq!(
        pieniadz(&w),
        przed,
        "pieniądz w księgach zmienił sumę na sesji"
    );

    let firms = w.resource::<Firms>();
    let f = firms.get(spolka).expect("spółka");
    assert!(f.owners_sum_ok(), "suma udziałów przestała być 10 000");
    assert_eq!(equity::stake_of(f, Owner::Firm(kupiec)), 2_000);
    assert_eq!(equity::stake_of(f, Owner::External), 8_000);
    // Kupiec zapłacił 2 000 bp × 1 000 gr = 2 000 000 gr.
    assert_eq!(saldo(&w, kupiec), Money(98_000_000));
}

/// Kryterium WP10.11: przekroczenie 5 % się ujawnia, a ponad 50 % zmienia zarząd
/// **i podmienia osobowość firmy na osobowość przejmującego**.
#[test]
fn przekroczenie_progow_ujawnia_i_zmienia_zarzad() {
    let b = bench(12, &[]);
    let mut w = swiat(b);
    let cel = firma(&mut w, "Cel", SiteId(ent(511)), 0);
    let napastnik = firma(&mut w, "Napastnik", SiteId(ent(512)), 900_000_000);
    // Napastnik ma inne cechy niż cel — inaczej podmiana nie byłaby widoczna.
    {
        let firms = w.resource_mut::<Firms>();
        let p = FirmPersonality {
            aggression: 90,
            risk_tolerance: 90,
            quality_focus: 20,
            price_focus: 80,
            innovation: 70,
            patience: 10,
            staff_loyalty: 30,
        };
        firms
            .get_mut(napastnik)
            .expect("napastnik")
            .set_personality(p);
        assert_ne!(
            firms.get(cel).expect("cel").personality,
            firms.get(napastnik).expect("napastnik").personality
        );
    }

    let mut eq = Equity::new(bez_szumu());
    assert!(eq.list(cel, 9_000, Money(1_000), SimMinute(0)));
    // Najpierw mały pakiet: ma przekroczyć próg ujawnienia i nic więcej.
    eq.place_order(
        cel,
        Owner::External,
        Side::Sell,
        Money(1_000),
        600,
        SimMinute(u64::MAX),
    );
    eq.place_order(
        cel,
        Owner::Firm(napastnik),
        Side::Buy,
        Money(1_000),
        600,
        SimMinute(u64::MAX),
    );
    system::register_equity(&mut w, eq);
    system::step_day(&mut w, Tick(DOBA));

    let ujawnienia = w.resource_mut::<Equity>().take_disclosures();
    assert_eq!(ujawnienia.len(), 1, "brak ujawnienia po przekroczeniu 5 %");
    assert!(!ujawnienia[0].control);
    assert_eq!(
        ujawnienia[0].holder,
        Subject::Firm(firm_id(napastnik)),
        "ujawnienie wskazuje kogoś innego"
    );
    assert!(u32::from(ujawnienia[0].bp) >= DISCLOSURE_BP);
    assert_eq!(
        w.resource::<Firms>().get(cel).expect("cel").personality,
        FirmPersonality::NEUTRAL,
        "pakiet 6 % nie może zmieniać zarządu"
    );

    // Teraz reszta do kontroli.
    {
        let eq = w.resource_mut::<Equity>();
        eq.place_order(
            cel,
            Owner::External,
            Side::Sell,
            Money(1_000),
            5_000,
            SimMinute(u64::MAX),
        );
        eq.place_order(
            cel,
            Owner::Firm(napastnik),
            Side::Buy,
            Money(1_000),
            5_000,
            SimMinute(u64::MAX),
        );
    }
    let raport = system::step_day(&mut w, Tick(2 * DOBA));
    assert_eq!(raport.takeovers, 1, "przejęcie się nie odbyło");

    let firms = w.resource::<Firms>();
    let f = firms.get(cel).expect("cel");
    assert!(u32::from(equity::stake_of(f, Owner::Firm(napastnik))) > CONTROL_BP);
    assert_eq!(
        f.personality,
        firms.get(napastnik).expect("napastnik").personality,
        "kontrola nie podmieniła cech firmy"
    );
    assert!(f.owners_sum_ok());
}

/// Kryterium WP10.10: kurs nie porusza się przed publikacją wyników, jeśli nie ma
/// plotki.
///
/// Mierzy się to na **przekonaniu**, a nie na liczbie z fixingu, i to jest istota:
/// wycena fundamentalna zmienia się wyłącznie w dobie publikacji, a między
/// publikacjami jedynym kanałem jest plotka. Sama sesja z identycznym arkuszem daje
/// identyczną cenę — to pilnuje czystość `fixing`.
#[test]
fn kurs_nie_rusza_sie_przed_publikacja_wynikow() {
    let p = equity::Published {
        month: 3,
        profit_12m: Money(120_000_000),
        book: Money(10_000_000),
    };
    let f = equity::fundamental(&p, 20);
    let bez_plotki: Vec<Money> = (100..110u32)
        .map(|d| {
            equity::belief(
                f,
                0,
                0,
                7,
                5,
                1,
                magnat_economy::equity::invest::decision_tick(d),
            )
        })
        .collect();
    assert!(
        bez_plotki.windows(2).all(|x| x[0] == x[1]),
        "przekonanie rusza się bez publikacji i bez plotki: {bez_plotki:?}"
    );
    let z_plotka = equity::belief(
        f,
        0,
        800,
        7,
        5,
        1,
        magnat_economy::equity::invest::decision_tick(105),
    );
    assert!(
        z_plotka > bez_plotki[0],
        "plotka nie ruszyła przekonania: {z_plotka:?} vs {:?}",
        bez_plotki[0]
    );
    // Nowa publikacja rusza wycenę — i to jest drugi z dwóch kanałów.
    let lepszy = equity::Published {
        profit_12m: Money(180_000_000),
        ..p
    };
    assert!(equity::fundamental(&lepszy, 20) > f);
}

/// Kryterium WP10.11 i „wynik do pokazania" podfazy: wrogie przejęcie da się
/// **obronić** — emisja na rzecz największego właściciela zbija napastnika.
#[test]
fn emisja_obronna_zbija_napastnika() {
    let b = bench(13, &[]);
    let mut w = swiat(b);
    let cel = firma(&mut w, "Cel", SiteId(ent(521)), 0);
    let obronca = firma(&mut w, "Obrońca", SiteId(ent(522)), 500_000_000);
    let napastnik = firma(&mut w, "Napastnik", SiteId(ent(523)), 500_000_000);
    {
        let firms = w.resource_mut::<Firms>();
        let f = firms.get_mut(cel).expect("cel");
        f.owners.clear();
        f.owners.push(magnat_firms::OwnerShare {
            owner: Owner::Firm(obronca),
            bp: 4_800,
        });
        f.owners.push(magnat_firms::OwnerShare {
            owner: Owner::Firm(napastnik),
            bp: 4_600,
        });
        f.owners.push(magnat_firms::OwnerShare {
            owner: Owner::External,
            bp: 600,
        });
        assert!(f.owners_sum_ok());
    }
    let mut eq = Equity::new(bez_szumu());
    assert!(eq.list(cel, 4_500, Money(1_000), SimMinute(0)));
    system::register_equity(&mut w, eq);

    let przed_pieniadz = pieniadz(&w);
    let przed_napastnik = equity::stake_of(
        w.resource::<Firms>().get(cel).expect("cel"),
        Owner::Firm(napastnik),
    );
    // Emisje idą na granicy miesiąca.
    let raport = system::step_day(&mut w, Tick(30 * DOBA));
    assert_eq!(raport.issues, 1, "emisja obronna się nie odbyła");

    let firms = w.resource::<Firms>();
    let f = firms.get(cel).expect("cel");
    assert!(f.owners_sum_ok(), "emisja zgubiła udział: {:?}", f.owners);
    let po = equity::stake_of(f, Owner::Firm(napastnik));
    assert!(po < przed_napastnik, "napastnik nie został rozwodniony");
    assert!(
        u32::from(equity::stake_of(f, Owner::Firm(obronca))) > CONTROL_BP,
        "obrońca nie odzyskał kontroli"
    );
    assert_eq!(pieniadz(&w), przed_pieniadz, "emisja stworzyła pieniądz");
}

/// Gospodarstwo domowe jako inwestor: pieniądz przechodzi granicę sektora w obie
/// strony i suma świata się nie zmienia.
#[test]
fn gospodarstwo_kupuje_i_sprzedaje_przez_kanal_sektora() {
    let b = bench(14, &[]);
    let mut w = swiat(b);
    let spolka = firma(&mut w, "Spółka", SiteId(ent(531)), 0);

    // Mieszkaniec z tożsamością i gospodarstwem — bez `Identity` giełda nie ma jak
    // dojść do jego pieniądza, bo saldo siedzi w komponencie gospodarstwa (M5b).
    let gd = w
        .spawn()
        .with(magnat_agents::Household {
            size: 1,
            flags: magnat_agents::Household::FLAG_ACTIVE,
            savings: Money(20_000_000),
            ..magnat_agents::Household::default()
        })
        .id();
    let mieszkaniec = w
        .spawn()
        .with(magnat_agents::Personality([50; 8]))
        .with(magnat_agents::Identity {
            household: gd.index(),
            flags: magnat_agents::Identity::FLAG_ALIVE,
            ..magnat_agents::Identity::default()
        })
        .id();
    let mut pop = magnat_agents::Population::new();
    pop.add_citizen(mieszkaniec);
    pop.add_household(gd);
    w.insert_resource(pop);

    let kto = Owner::Citizen(CitizenId(mieszkaniec));
    let mut eq = Equity::new(bez_szumu());
    assert!(eq.list(spolka, 3_000, Money(1_000), SimMinute(0)));
    eq.place_order(
        spolka,
        Owner::External,
        Side::Sell,
        Money(1_000),
        1_000,
        SimMinute(u64::MAX),
    );
    eq.place_order(
        spolka,
        kto,
        Side::Buy,
        Money(1_000),
        1_000,
        SimMinute(u64::MAX),
    );
    system::register_equity(&mut w, eq);

    let suma = |w: &World| -> i64 {
        let ksiegi = w.resource::<Books>().total_balance().get();
        let h = w
            .get::<magnat_agents::Household>(gd)
            .map_or(0, |h| h.cash.get() + h.bank.get() + h.savings.get());
        ksiegi + h
    };
    let przed = suma(&w);
    system::step_day(&mut w, Tick(DOBA));
    assert_eq!(suma(&w), przed, "pieniądz zginął na granicy sektora");

    let firms = w.resource::<Firms>();
    let f = firms.get(spolka).expect("spółka");
    assert_eq!(equity::stake_of(f, kto), 1_000);
    assert!(f.owners_sum_ok());
    assert_eq!(
        w.get::<magnat_agents::Household>(gd).expect("gd").savings,
        Money(19_000_000),
        "gospodarstwo nie zapłaciło za pakiet"
    );
}

/// Sesja nie może przenieść pieniądza za udział, którego nie ma.
///
/// Zlecenie sprzedaży większe od pakietu i zlecenie kupna większe niż stać kupca
/// przycinają się **przed** fixingiem. Bez tego kupujący płaciłby za wolumen,
/// którego nie ma z czego przenieść: pieniądz domykałby się co do grosza,
/// a akcjonariat nie.
#[test]
fn zlecenie_bez_pokrycia_przycina_sie_przed_fixingiem() {
    let b = bench(15, &[]);
    let mut w = swiat(b);
    let spolka = firma(&mut w, "Spółka", SiteId(ent(541)), 0);
    // Biedny kupiec: stać go na 100 bp po kursie 1 000 gr.
    let biedny = firma(&mut w, "Biedny", SiteId(ent(542)), 100_000);
    // Sprzedający ma 1 000 bp, a wystawia 9 000.
    let maly = firma(&mut w, "Mały", SiteId(ent(543)), 0);
    {
        let firms = w.resource_mut::<Firms>();
        let f = firms.get_mut(spolka).expect("spółka");
        f.owners.clear();
        f.owners.push(magnat_firms::OwnerShare {
            owner: Owner::Firm(maly),
            bp: 1_000,
        });
        f.owners.push(magnat_firms::OwnerShare {
            owner: Owner::External,
            bp: 9_000,
        });
    }
    let mut eq = Equity::new(bez_szumu());
    assert!(eq.list(spolka, 1_000, Money(1_000), SimMinute(0)));
    eq.place_order(
        spolka,
        Owner::Firm(maly),
        Side::Sell,
        Money(1_000),
        9_000,
        SimMinute(u64::MAX),
    );
    eq.place_order(
        spolka,
        Owner::Firm(biedny),
        Side::Buy,
        Money(1_000),
        9_000,
        SimMinute(u64::MAX),
    );
    system::register_equity(&mut w, eq);

    let przed = pieniadz(&w);
    let raport = system::step_day(&mut w, Tick(DOBA));
    // Ciaśniejsze z dwóch ograniczeń: kupca stać na 100 bp.
    assert_eq!(raport.volume_bp, 100, "wolumen nie został przycięty");
    assert_eq!(pieniadz(&w), przed);

    let firms = w.resource::<Firms>();
    let f = firms.get(spolka).expect("spółka");
    assert!(f.owners_sum_ok());
    assert_eq!(equity::stake_of(f, Owner::Firm(biedny)), 100);
    assert_eq!(equity::stake_of(f, Owner::Firm(maly)), 900);
    assert_eq!(
        saldo(&w, biedny),
        Money::ZERO,
        "kupiec zapłacił dokładnie tyle, ile miał"
    );
    assert_eq!(
        saldo(&w, maly),
        Money(100_000),
        "sprzedający dostał za 100 bp"
    );
}
