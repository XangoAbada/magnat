//! Lista płac obciąża pracodawcę (`R2-WP30`, pozycja 58 wykazu `R2`).
//!
//! Przed naprawą `PayrollOutbox::take()` nie miało w repozytorium **ani jednego
//! wołającego**: skrzynka rosła w nieskończoność, saldo zakładu nie drgało, a pensje
//! wchodziły do gospodarstw z konta `rest_of_world` przez `pay_incomes`. Oba testy
//! w tym pliku mierzą dokładnie to: czy skrzynka się opróżnia i czy pieniądz wychodzi
//! od pracodawcy.

mod common;

use common::{bench, ent};
use magnat_agents::{Household, Identity, Population};
use magnat_core::{
    CitizenId, DecisionReason, DistrictId, Entity, FirmId, JobRoleId, Money, SimMinute, SiteId,
    Tick, Q,
};
use magnat_economy::books::{AccountId, AccountKind, AccountOwner, Books, TxKind, TxMemo};
use magnat_economy::payroll::absorb_payroll;
use magnat_economy::Market;
use magnat_ecs::World;
use magnat_firms::hr::employment::Employment;
use magnat_firms::{
    Firm, Firms, Owner, PayrollOutbox, Position, Ring, Site, SitePnlMonth, SiteTypeId,
};

const ROLA: JobRoleId = JobRoleId(0);
const PLACA: Money = Money(400_000);
const ETATOW: u32 = 3;
const SALDO_ZAKLADU: i64 = 50_000_000;

struct Swiat {
    world: World,
    market: Market,
    firms: Firms,
    site: SiteId,
    gospodarstwa: Vec<Entity>,
    konto_zakladu: AccountId,
}

/// Zakład z trzema obsadzonymi etatami i trzema gospodarstwami po jednej osobie.
fn swiat() -> Swiat {
    let mut world = World::new(17);
    magnat_agents::register(
        &mut world,
        magnat_agents::NeedTable::load_default().expect("data/needs"),
    );
    magnat_agents::society::register_society(
        &mut world,
        magnat_agents::DemographyTable::load_default().expect("data/demography"),
    );
    world.insert_resource(PayrollOutbox::default());

    let mut b = bench(17, &[]);
    let market = b.market.clone();
    let konto_zakladu = b.books.open_account(
        AccountOwner::Firm(FirmId(ent(900))),
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    b.books
        .transfer(
            b.rest,
            konto_zakladu,
            Money(SALDO_ZAKLADU),
            TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
            Tick(0),
        )
        .expect("wyposażenie zakładu");
    let books = b.books;

    let site = SiteId(ent(900));
    market.register_plant(site, FirmId(ent(900)), konto_zakladu);

    let mut firms = Firms::new();
    let firma = firms.insert(|k| {
        Firm::sole_owner(
            k,
            "Huta".to_string(),
            SimMinute(0),
            DistrictId(0),
            Owner::External,
        )
    });

    let mut gospodarstwa = Vec::new();
    let mut umowy = Vec::new();
    for i in 0..ETATOW {
        let hh = world
            .spawn()
            .with(Household {
                flags: Household::FLAG_ACTIVE,
                size: 1,
                income_monthly: PLACA,
                ..Household::default()
            })
            .id();
        world.resource_mut::<Population>().add_household(hh);
        let c = world
            .spawn()
            .with(Identity {
                birth_day: -35 * 360,
                flags: Identity::FLAG_ALIVE,
                household: hh.index(),
                ..Identity::default()
            })
            .id();
        world.resource_mut::<Population>().add_citizen(c);
        gospodarstwa.push(hh);
        umowy.push(Employment::new(
            CitizenId(c),
            ROLA,
            PLACA,
            SimMinute(0),
            magnat_agents::ShiftKind::Day,
            hh.index(),
        ));
        let _ = i;
    }

    let mut poz = Position::new(ROLA, ETATOW as u16, false, (PLACA, PLACA));
    poz.filled = umowy;
    let zaklad = Site {
        id: site,
        firm: firma,
        site_type: SiteTypeId(0),
        building: magnat_core::BuildingId(ent(1)),
        district: DistrictId(0),
        floor_m2: 400,
        positions: vec![poz],
        mgmt: magnat_firms::ManagementQuality::NEUTRAL,
        tech: Q::new(50),
        fixed_cost_month: Money::ZERO,
        hr_accrued: Money::ZERO,
        rnd_accrued: Money::ZERO,
        pnl: Ring::<SitePnlMonth, 36>::new(),
        opened: SimMinute(0),
        strike_bps: 0,
        strike_bp_days: 0,
        delegation: None,
        shift_profile: magnat_agents::ShiftProfile::Office,
    };
    assert!(firms.add_site(zaklad));
    world.insert_resource(books);

    Swiat {
        world,
        market,
        firms,
        site,
        gospodarstwa,
        konto_zakladu,
    }
}

fn saldo(w: &World, konto: AccountId) -> Money {
    w.resource::<Books>().balance(konto).expect("konto")
}

fn suma_gospodarstw(w: &World, hh: &[Entity]) -> Money {
    Money(
        hh.iter()
            .filter_map(|e| w.get::<Household>(*e))
            .map(|h| h.bank.get() + h.cash.get())
            .sum(),
    )
}

/// Doba tak, jak robi ją symulacja: `FirmSystem` produkuje o północy, `MarketSystem`
/// konsumuje w tej samej minucie.
fn doba(s: &mut Swiat, d: u64) -> magnat_economy::payroll::PayrollDay {
    let cal = magnat_core::SimCalendar::new(Tick(d * 1_440));
    let wyplaty = s.firms.run_payroll(cal);
    if !wyplaty.is_empty() {
        let out = s.world.resource_mut::<PayrollOutbox>();
        out.pending.items.extend(wyplaty.items);
        out.pending.hr_costs.extend(wyplaty.hr_costs);
    }
    absorb_payroll(&mut s.world, &s.market, Tick(d * 1_440))
}

/// Kryterium `R2-WP30`: po dniu wypłaty saldo zakładu jest mniejsze dokładnie o sumę
/// wypłat, a suma sald gospodarstw większa o tę samą kwotę.
///
/// Przed naprawą saldo zakładu **nie zmieniało się w ogóle**, bo skrzynki nikt
/// nie opróżniał.
#[test]
fn wyplata_schodzi_z_konta_zakladu_i_wchodzi_do_gospodarstw() {
    let mut s = swiat();
    let saldo_przed = saldo(&s.world, s.konto_zakladu);
    let gd_przed = suma_gospodarstw(&s.world, &s.gospodarstwa);

    let mut razem = Money::ZERO;
    for d in 1..=30 {
        let dzien = doba(&mut s, d);
        razem = Money(razem.get() + dzien.net.get());
    }

    assert_eq!(
        razem,
        Money(PLACA.get() * i64::from(ETATOW)),
        "w miesiącu wypłacono co innego niż trzy pensje"
    );
    assert_eq!(
        saldo(&s.world, s.konto_zakladu),
        Money(saldo_przed.get() - razem.get()),
        "saldo zakładu nie zmalało o sumę wypłat"
    );
    assert_eq!(
        suma_gospodarstw(&s.world, &s.gospodarstwa),
        Money(gd_przed.get() + razem.get()),
        "gospodarstwa nie dostały tego, co zeszło z konta zakładu"
    );
}

/// Druga połowa kryterium: **skrzynka się opróżnia**.
///
/// Przed naprawą `PayrollOutbox.pending` rosło w nieskończoność — po dziesięciu
/// dobach miało dziesięć dób wypłat i nie miało kto ich odebrać.
#[test]
fn skrzynka_nie_rosnie_w_nieskonczonosc() {
    let mut s = swiat();
    for d in 1..=60 {
        doba(&mut s, d);
        assert!(
            s.world.resource::<PayrollOutbox>().pending.is_empty(),
            "skrzynka nie została opróżniona w dobie {d}"
        );
    }
}

/// Lista płac nie tworzy ani nie niszczy pieniądza: ile zeszło z konta zakładu,
/// tyle weszło do gospodarstw, co do grosza.
#[test]
fn lista_plac_zachowuje_pieniadz() {
    let mut s = swiat();
    let przed = Money(
        saldo(&s.world, s.konto_zakladu).get() + suma_gospodarstw(&s.world, &s.gospodarstwa).get(),
    );
    for d in 1..=90 {
        doba(&mut s, d);
    }
    let po = Money(
        saldo(&s.world, s.konto_zakladu).get() + suma_gospodarstw(&s.world, &s.gospodarstwa).get(),
    );
    assert_eq!(przed, po, "lista płac zgubiła albo stworzyła pieniądz");
    // …a księgi domykają się tak samo jak zawsze.
    assert_eq!(s.world.resource::<Books>().check_conservation(), Ok(()));
    let _ = s.site;
}
