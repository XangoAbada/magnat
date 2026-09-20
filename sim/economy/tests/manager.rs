//! Kryterium WP9 (M9d): **200 sklepów z politykami poniżej 1 ms na dobę i ten sam
//! wynik w dwóch przebiegach**, plus cztery wejścia jakości menedżera.
//!
//! Test odpowiada na pytanie, którego WP7 nie stawiał: czy różnica między menedżerem
//! o umiejętności 20 a menedżerem o umiejętności 90 jest **widoczna w cenie**, a nie
//! tylko w tabeli. Bez tego zatrudnianie ludzi jest kosztem bez skutku.

mod common;

use std::collections::BTreeMap;

use magnat_core::{
    BuildingId, CitizenId, DistrictId, GoodId, Money, Qty, SimMinute, SiteId, Tick, Q,
};
use magnat_economy::{EconomyData, ManagerCurve, ManagerExecution, Market, PolicyTuning};
use magnat_firms::{
    Autonomy, Firm, FirmKey, Firms, Manager, ManagerStyle, Owner, Ring, Site, SiteDelegation,
    SitePnlMonth, SiteTypeId,
};
use magnat_policy::{FirmPolicy, PolicyCatalog, PolicyDomain};
use magnat_spatial::Vec2;

use common::{bench, ent, good_by_key, Bench, WHOLESALE_BASE};

const DOBA: u64 = magnat_core::time::MINUTES_PER_DAY;

/// Budżet doby polityk dla 200 zakładów (M9 §7). Wydanie nieoptymalizowane liczy
/// kilkanaście razy wolniej i to jest **znany** mnożnik, a nie zapas na wszelki
/// wypadek — ten sam wzorzec, którym mierzą budżety M1 i M3.
const LIMIT_MS: f64 = if cfg!(debug_assertions) { 15.0 } else { 1.0 };

fn mydlo() -> GoodId {
    good_by_key(&EconomyData::load_default().unwrap(), "cons_soap")
}

fn krzywa() -> ManagerCurve {
    PolicyTuning::load_default()
        .expect("data/tuning/policy.ron")
        .manager
}

fn preset(key: &str, domain: PolicyDomain) -> FirmPolicy {
    let c = PolicyCatalog::load_default().expect("data/policies/presets.ron");
    magnat_economy::preset_for(&c, key, domain).expect("preset z katalogu")
}

fn zaklad(id: SiteId, firm: FirmKey, i: usize) -> Site {
    Site {
        id,
        firm,
        site_type: SiteTypeId(0),
        building: BuildingId(ent(500 + i as u32)),
        district: DistrictId(i as u16),
        floor_m2: 200,
        positions: Vec::new(),
        mgmt: magnat_firms::ManagementQuality::NEUTRAL,
        tech: Q::new(50),
        fixed_cost_month: Money(100_000),
        hr_accrued: Money::ZERO,
        rnd_accrued: Money::ZERO,
        pnl: Ring::<SitePnlMonth, 36>::new(),
        opened: SimMinute(0),
        delegation: None,
    }
}

/// Rejestr firm, w którym **każdy zakład prowadzi menedżer o zadanej umiejętności**.
fn firmy(b: &Bench, p: &FirmPolicy, skill: u8) -> Firms {
    let mut f = Firms::new();
    let t = magnat_firms::LaborTuning::load_default()
        .expect("data/tuning/labor.ron")
        .manager;
    for (i, site) in b.sites.iter().enumerate() {
        let key = f.insert(|k| {
            Firm::sole_owner(
                k,
                format!("Sklep {i}"),
                SimMinute(0),
                DistrictId(i as u16),
                Owner::Player,
            )
        });
        assert!(f.add_site(zaklad(*site, key, i)));
        let c = CitizenId(ent(800 + i as u32));
        let m = Manager::new(c, Q::new(skill), ManagerStyle::Bureaucrat, SimMinute(0));
        let d = SiteDelegation::new(c, p.clone(), Autonomy::Full);
        assert!(f.assign_manager(*site, m, d, |_| Q::new(50), &t, Tick(0)));
    }
    f
}

fn doba(m: &Market, f: &mut Firms, c: Option<&ManagerCurve>, t: Tick) -> magnat_economy::PolicyDay {
    m.expire_goods(t);
    m.observe_competitors(t);
    m.record_policy_trace(t);
    let d = m.run_policies(f, &BTreeMap::new(), c, t);
    m.reprice_all(t);
    d
}

/// Dwa sklepy obok siebie z zapasem — tyle, ile potrzebuje polityka dyskontowa.
fn dwa_sklepy(seed: u64) -> Bench {
    let g = mydlo();
    let b = bench(seed, &[Vec2::new(300.0, 0.0), Vec2::new(340.0, 0.0)]);
    for s in &b.sites {
        b.market.deliver_now(
            *s,
            g,
            Qty(2_000_000),
            Money(2_000 * WHOLESALE_BASE),
            None,
            Tick(0),
        );
    }
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
    b
}

#[test]
fn krzywa_z_danych_rozroznia_menedzerow() {
    let c = krzywa();
    let slaby = ManagerExecution::from_skill(Q::new(10), &c);
    let dobry = ManagerExecution::from_skill(Q::new(90), &c);
    assert!(slaby.info_lag_days > dobry.info_lag_days);
    assert!(slaby.reaction_delay_h > dobry.reaction_delay_h);
    assert!(slaby.exec_error_bp > dobry.exec_error_bp);
    assert!(slaby.skip_chance_bp > dobry.skip_chance_bp);
}

#[test]
fn slaby_menedzer_ustawia_inna_cene_niz_dobry() {
    // Ta sama reguła, ten sam świat, ta sama doba — różni się **wyłącznie** człowiek.
    // Gdyby cena wyszła identyczna, warstwa HR z PRD §7.5 nie miałaby skutku w cenie.
    let g = mydlo();
    let cena = |skill: u8| {
        let b = dwa_sklepy(91);
        let mut f = firmy(&b, &preset("retail_discount", PolicyDomain::Pricing), skill);
        for d in 1..=3 {
            doba(&b.market, &mut f, Some(&krzywa()), Tick(d * DOBA));
        }
        b.market.shelf_price(b.sites[0], g).expect("cena półkowa")
    };
    let slaby = cena(10);
    let dobry = cena(95);
    assert_ne!(slaby, dobry, "menedżer nie zmienia niczego");
}

#[test]
fn dwa_przebiegi_tego_samego_ziarna_daja_te_same_ceny() {
    // Determinizm wykonania: błąd i pominięcie pochodzą wyłącznie ze strumienia
    // `StreamId::PolicyExecution`, a ten jest funkcją (ziarno, menedżer, tick).
    let g = mydlo();
    let przebieg = || {
        let b = dwa_sklepy(17);
        let mut f = firmy(&b, &preset("retail_discount", PolicyDomain::Pricing), 25);
        let mut slad = Vec::new();
        for d in 1..=10 {
            let day = doba(&b.market, &mut f, Some(&krzywa()), Tick(d * DOBA));
            slad.push((
                day.applied,
                day.skipped,
                day.on_cooldown,
                b.market.shelf_price(b.sites[0], g),
                b.market.shelf_price(b.sites[1], g),
            ));
        }
        slad
    };
    assert_eq!(przebieg(), przebieg());
}

#[test]
fn menedzer_ustawia_wiek_obrazu_konkurencji() {
    // Opóźnienie informacji **nie ma własnego licznika** — jest wiekiem obrazu
    // konkurencji, który istnieje od M5c. Test pilnuje, żeby drugi licznik nie wrócił.
    let b = dwa_sklepy(33);
    let c = krzywa();
    for skill in [0u8, 100] {
        let mut f = firmy(&b, &preset("retail_discount", PolicyDomain::Pricing), skill);
        doba(&b.market, &mut f, Some(&c), Tick(DOBA));
        let oczekiwane = ManagerExecution::from_skill(Q::new(skill), &c).info_lag_days;
        assert_eq!(
            b.market.competitor_delay_days(b.sites[0]),
            Some(oczekiwane),
            "umiejętność {skill}"
        );
    }
}

#[test]
fn zwloka_reakcji_wydluza_martwa_strefe() {
    // Polityka godzinowa z martwą strefą sześciu godzin: menedżer doskonały wraca
    // do pracy po sześciu, słaby — dopiero po sześciu plus swojej zwłoce.
    let c = krzywa();
    let sprawdz = |skill: u8, po_godzinach: u64| {
        let b = dwa_sklepy(51);
        let mut f = firmy(
            &b,
            &preset("perishables_markdown", PolicyDomain::Pricing),
            skill,
        );
        doba(&b.market, &mut f, Some(&c), Tick(DOBA));
        b.market
            .run_policies(
                &mut f,
                &BTreeMap::new(),
                Some(&c),
                Tick(DOBA + po_godzinach * 60),
            )
            .on_cooldown
    };
    // Zakłady są dwa, więc licznik chodzi po dwa.
    assert_eq!(
        sprawdz(100, 7),
        0,
        "menedżer doskonały ma wrócić po sześciu"
    );
    assert_eq!(sprawdz(0, 7), 2, "słaby menedżer wrócił za wcześnie");
}

#[test]
fn zapytaj_gracza_laduje_w_skrzynce_i_zatrzymuje_polityke() {
    // Eskalacja: `AskPlayer` nie zmienia świata, ale **musi** dojść do gracza,
    // inaczej „automatyzacja, której da się przerwać" jest pustym zdaniem.
    let g = mydlo();
    let b = dwa_sklepy(61);
    b.market
        .set_tracking(b.sites[0], magnat_economy::LostSaleTracking::Full);
    let mut p =
        magnat_policy::Policy::empty(magnat_core::PolicyId(9), "pytam", PolicyDomain::Stock);
    p.cooldown_h = 12;
    p.rules.push(magnat_policy::Rule {
        when: magnat_policy::ConditionExpr::Always,
        then: [
            magnat_policy::Action::AskPlayer { msg: 3 },
            // Akcja **po** pytaniu nie ma się wykonać: polityka zatrzymuje się
            // na tym towarze do najbliższego wykonania.
            magnat_policy::Action::OrderUpTo {
                good: magnat_policy::GoodRef::This,
                days: magnat_policy::Expr::Lit(magnat_policy::Value::Days(30)),
            },
        ]
        .into_iter()
        .collect(),
        enabled: true,
        note: String::new(),
    });
    let mut f = firmy(&b, &p, 100);
    let przed = b.market.reorder_target(b.sites[0], g);
    doba(&b.market, &mut f, Some(&krzywa()), Tick(DOBA));
    let skrzynka = b.market.policy_inbox();
    assert!(
        skrzynka
            .iter()
            .any(|a| a.ask && a.msg == 3 && a.site == b.sites[0]),
        "{skrzynka:?}"
    );
    // Zakład nieśledzony nie zapycha skrzynki cudzymi pytaniami.
    assert!(skrzynka.iter().all(|a| a.site == b.sites[0]));
    assert_eq!(
        b.market.reorder_target(b.sites[0], g),
        przed,
        "akcja po pytaniu do gracza jednak się wykonała"
    );
}

#[test]
fn dry_run_zgadza_sie_z_wykonaniem_co_do_grosza() {
    // §7: „dry-run na 30 dniach zgodny z późniejszym rzeczywistym wykonaniem przy
    // `ManagerExecution` o `skill = 100`, tolerancja 0".
    //
    // Dwie **niezależne** drogi do tej samej liczby: podgląd liczy ze śladu doby,
    // wykonanie z żywej półki. Gdyby ślad kłamał, wynik by się rozjechał.
    let g = mydlo();
    let polityka = preset("retail_discount", PolicyDomain::Pricing);

    let b = dwa_sklepy(77);
    b.market
        .set_tracking(b.sites[0], magnat_economy::LostSaleTracking::Full);
    b.market.expire_goods(Tick(DOBA));
    b.market.observe_competitors(Tick(DOBA));
    b.market.record_policy_trace(Tick(DOBA));
    let podglad = b.market.dry_run(b.sites[0], &polityka);
    let przewidziana = podglad
        .prices_of(g)
        .last()
        .map(|(_, m)| *m)
        .expect("dry-run nie ma ceny");

    // Ten sam świat, ta sama doba — tym razem naprawdę.
    let b2 = dwa_sklepy(77);
    let mut f = firmy(&b2, &polityka, 100);
    b2.market.expire_goods(Tick(DOBA));
    b2.market.observe_competitors(Tick(DOBA));
    b2.market
        .run_policies(&mut f, &BTreeMap::new(), Some(&krzywa()), Tick(DOBA));
    let wykonana = b2.market.shelf_price(b2.sites[0], g).expect("cena półkowa");
    assert_eq!(przewidziana, wykonana);
}

#[test]
fn dry_run_bez_sladu_nie_udaje_ze_cos_wie() {
    let b = dwa_sklepy(78);
    let r = b.market.dry_run(
        b.sites[0],
        &preset("retail_discount", PolicyDomain::Pricing),
    );
    assert!(r.is_empty(), "zakład nieśledzony nie ma śladu");
}

#[test]
fn slad_trzyma_najwyzej_trzydziesci_dob() {
    let b = dwa_sklepy(79);
    b.market
        .set_tracking(b.sites[0], magnat_economy::LostSaleTracking::Full);
    for d in 1..=40 {
        b.market.record_policy_trace(Tick(d * DOBA));
    }
    let s = b.market.policy_trace(b.sites[0]);
    assert_eq!(s.days().len(), magnat_economy::TRACE_DAYS);
    assert_eq!(
        s.days()[0].day,
        Tick(11 * DOBA),
        "najstarsza doba nie wypadła"
    );
}

#[test]
fn dwiescie_sklepow_z_politykami_ponizej_milisekundy_na_dobe() {
    // Kryterium WP9. Mierzona jest **doba polityk**, a nie cała doba sklepu:
    // przecena i zaopatrzenie mają własne budżety w M5.
    let g = mydlo();
    let pozycje: Vec<Vec2> = (0..200)
        .map(|i| Vec2::new(300.0 + (i % 20) as f32 * 30.0, (i / 20) as f32 * 30.0))
        .collect();
    let b = bench(101, &pozycje);
    for s in &b.sites {
        b.market.deliver_now(
            *s,
            g,
            Qty(2_000_000),
            Money(2_000 * WHOLESALE_BASE),
            None,
            Tick(0),
        );
    }
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
    let mut f = firmy(&b, &preset("retail_discount", PolicyDomain::Pricing), 60);
    let c = krzywa();
    // Rozgrzewka: pierwsza doba buduje obraz konkurencji dla dwustu sklepów.
    doba(&b.market, &mut f, Some(&c), Tick(DOBA));

    const DNI: u64 = 20;
    let mut suma = std::time::Duration::ZERO;
    for d in 2..2 + DNI {
        // Obserwacja konkurencji ma **własny** budżet w M5 i nie jest kosztem
        // polityki — mierzy się to, za co odpowiada ta podfaza.
        b.market.observe_competitors(Tick(d * DOBA));
        let start = std::time::Instant::now();
        b.market
            .run_policies(&mut f, &BTreeMap::new(), Some(&c), Tick(d * DOBA));
        suma += start.elapsed();
    }
    let na_dobe = suma.as_secs_f64() * 1000.0 / DNI as f64;
    assert!(
        na_dobe < LIMIT_MS,
        "doba polityk 200 zakładów: {na_dobe:.3} ms (limit {LIMIT_MS} ms)"
    );
}

#[test]
fn powod_niesie_wiek_danych_i_odchylke_menedzera() {
    // Bez tych dwóch liczb zdanie „cel 6,38 zł, menedżer ustawił 6,44 zł" z §5.6
    // nie ma z czego powstać: dzień później obraz konkurencji jest już inny,
    // a rzut menedżera nie zostawia śladu nigdzie indziej.
    let b = dwa_sklepy(83);
    b.market
        .set_tracking(b.sites[0], magnat_economy::LostSaleTracking::Full);
    let mut f = firmy(&b, &preset("retail_discount", PolicyDomain::Pricing), 15);
    for d in 1..=8 {
        doba(&b.market, &mut f, Some(&krzywa()), Tick(d * DOBA));
    }
    let log = b.market.reprice_log(b.sites[0]);
    let polityki: Vec<_> = log
        .iter()
        .filter_map(|r| match r {
            magnat_core::DecisionReason::PolicyApplied {
                lag_days,
                deviation_bp,
                ..
            } => Some((*lag_days, *deviation_bp)),
            _ => None,
        })
        .collect();
    assert!(
        !polityki.is_empty(),
        "polityka nie zapisała ani jednego powodu"
    );
    assert!(
        polityki.iter().any(|(l, d)| *l > 1 || *d != 0),
        "słaby menedżer nie zostawił po sobie ani wieku danych, ani odchyłki: {polityki:?}"
    );
}

#[test]
fn menedzer_doskonaly_nie_zostawia_odchylki() {
    let b = dwa_sklepy(84);
    b.market
        .set_tracking(b.sites[0], magnat_economy::LostSaleTracking::Full);
    let mut f = firmy(&b, &preset("retail_discount", PolicyDomain::Pricing), 100);
    for d in 1..=5 {
        doba(&b.market, &mut f, Some(&krzywa()), Tick(d * DOBA));
    }
    for r in b.market.reprice_log(b.sites[0]) {
        if let magnat_core::DecisionReason::PolicyApplied { deviation_bp, .. } = r {
            assert_eq!(deviation_bp, 0);
        }
    }
}
