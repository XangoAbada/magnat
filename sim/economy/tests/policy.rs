//! Kryterium WP7: **zakład zdelegowany menedżerowi działa bez ingerencji gracza
//! przez rok gry i nie degeneruje się** — magazyn nie pustoszeje, ceny nie uciekają.
//!
//! Do tego kryterium należy drugie zdanie WP6b, które da się sprawdzić dopiero tutaj,
//! bo wymaga prawdziwego sklepu: **ta sama reguła zastosowana do zakładu gracza
//! i do identycznego zakładu AI daje identyczną akcję** (M7 §7.9 pkt 1). Test
//! równoważności na samym ewaluatorze jest w `magnat-policy`; ten sprawdza całą
//! drogę — widok, ewaluację, wykonanie i zapis powodu.

mod common;

use magnat_core::{
    BuildingId, CitizenId, DecisionReason, DistrictId, FirmReason, GoodId, Money, PolicyId, Qty,
    SimMinute, SiteId, Tick, Q,
};
use magnat_economy::{EconomyData, Market, PricePolicy};
use magnat_firms::{
    Autonomy, Firm, FirmKey, Firms, Manager, ManagerStyle, Owner, Ring, Site, SiteDelegation,
    SitePnlMonth, SiteTypeId,
};
use magnat_policy::{FirmPolicy, PolicyCatalog, PolicyDomain};
use magnat_spatial::Vec2;

use common::{bench, ent, good_by_key, Bench, WHOLESALE_BASE};

const DOBA: u64 = magnat_core::time::MINUTES_PER_DAY;

fn mydlo() -> GoodId {
    good_by_key(&EconomyData::load_default().unwrap(), "cons_soap")
}

/// Rejestr firm z jednym zakładem na sklep — tyle, ile potrzebuje wykonawca polityk.
///
/// Zakład jest budowany wprost, a nie z katalogu typów: test sprawdza **wykonanie
/// polityki**, a nie obsadę z `data/site_types/`, i mały świat ma się dać policzyć
/// na kartce (ta sama zasada, co w reszcie testów M5).
fn firmy(b: &Bench, polityka: &FirmPolicy, menedzer: Option<ManagerStyle>) -> Firms {
    let mut f = Firms::new();
    for (i, site) in b.sites.iter().enumerate() {
        let key = f.insert(|k| {
            Firm::sole_owner(
                k,
                format!("Sklep {i}"),
                SimMinute(0),
                DistrictId(i as u16),
                // Pierwszy zakład jest gracza, drugi firmy AI — i to jest **cała**
                // różnica między nimi. Kod wykonania jest jeden (`K-11`).
                if i == 0 {
                    Owner::Player
                } else {
                    Owner::External
                },
            )
        });
        assert!(f.add_site(zaklad(*site, key, i)));
        deleguj(&mut f, *site, polityka.clone(), menedzer, i as u32);
    }
    f
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
        strike_bps: 0,
        strike_bp_days: 0,
        delegation: None,
        shift_profile: magnat_agents::ShiftProfile::Office,
    }
}

fn deleguj(f: &mut Firms, site: SiteId, p: FirmPolicy, styl: Option<ManagerStyle>, i: u32) {
    let c = CitizenId(ent(800 + i));
    let m = Manager::new(
        c,
        Q::new(70),
        styl.unwrap_or(ManagerStyle::Bureaucrat),
        SimMinute(0),
    );
    let d = SiteDelegation::new(c, p, Autonomy::Full);
    let t = magnat_firms::LaborTuning::load_default()
        .expect("data/tuning/labor.ron")
        .manager;
    assert!(f.assign_manager(site, m, d, |_| Q::new(50), &t, Tick(0)));
}

fn preset(key: &str, domain: PolicyDomain) -> FirmPolicy {
    let c = PolicyCatalog::load_default().expect("data/policies/presets.ron");
    magnat_economy::preset_for(&c, key, domain).expect("preset z katalogu")
}

/// Doba sklepu w kolejności z `MarketSystem`: obserwacja → polityki → przecena.
fn doba(m: &Market, f: &mut Firms, t: Tick) -> magnat_economy::PolicyDay {
    m.expire_goods(t);
    m.observe_competitors(t);
    let d = m.run_policies(f, &std::collections::BTreeMap::new(), None, t);
    m.reprice_all(t);
    d
}

#[test]
fn zaklad_gracza_i_zaklad_ai_wykonuja_regule_tym_samym_kodem() {
    // Dwa sklepy w tej samej pozycji rynku, z tym samym zapasem i tą samą polityką.
    // Jeden należy do gracza, drugi do firmy AI. Kryterium WP6b: **identyczna akcja
    // i identyczny powód** — różnicę ma robić menedżer, a nie kod.
    let g = mydlo();
    let b = bench(71, &[Vec2::new(300.0, 0.0), Vec2::new(300.0, 40.0)]);
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

    let p = preset("retail_discount", PolicyDomain::Pricing);
    let mut f = firmy(&b, &p, Some(ManagerStyle::Bureaucrat));
    let dzien = doba(&b.market, &mut f, Tick(DOBA));

    assert_eq!(dzien.sites, 2, "obie delegacje wykonały się w tej dobie");
    assert!(dzien.applied > 0, "polityka nie zrobiła nic");
    let gracz = b.market.price_at(b.sites[0], g).expect("cena gracza");
    let ai = b.market.price_at(b.sites[1], g).expect("cena AI");
    assert_eq!(gracz, ai, "ten sam kod dał dwie różne ceny");

    // Powód jest ten sam co do wariantu i co do reguły — to jest §7.9 pkt 1.
    // Wpisy `ManagerAssigned` **muszą** się różnić, bo niosą numer zakładu, i to jest
    // poprawne: różnicę ma robić przypisanie menedżera, a nie wykonanie reguły.
    let powody: Vec<Vec<DecisionReason>> = f
        .iter()
        .map(|(_, firma)| {
            firma
                .log
                .iter()
                .map(|w| w.reason)
                .filter(|r| matches!(r, DecisionReason::Firm(FirmReason::PolicyApplied { .. })))
                .collect()
        })
        .collect();
    assert_eq!(powody[0], powody[1], "ta sama reguła, dwa różne powody");
    assert!(
        !powody[0].is_empty(),
        "wykonanie polityki nie zostawiło powodu"
    );
}

#[test]
fn zdelegowany_sklep_nie_degeneruje_sie_przez_rok_gry() {
    // Kryterium WP7 wprost: rok gry bez ingerencji gracza.
    //
    // Towar jest **nietrwały** i to jest istota testu: chleb z dostawy przeterminuje
    // się w kilka dób, pokrycie zapasu spadnie do zera i wtedy dopiero widać, czy
    // polityka zapasu w ogóle reaguje. Na mydle (termin 720 dni) sklep stałby rok
    // z pełną półką, a test przechodziłby tożsamościowo.
    let g = good_by_key(&EconomyData::load_default().unwrap(), "food_bread_wheat");
    let b = bench(72, &[Vec2::new(300.0, 0.0)]);
    let site = b.sites[0];
    b.market.deliver_now(
        site,
        g,
        Qty(4_000_000),
        Money(4_000 * WHOLESALE_BASE),
        None,
        Tick(0),
    );
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let start = b.market.price_at(site, g).expect("cena startowa");
    let mut f = firmy(
        &b,
        &preset("stock_min_max", PolicyDomain::Stock),
        Some(ManagerStyle::Coach),
    );

    let (mut wykonan, mut dobowe) = (0u32, 0u32);
    for d in 1..=360u64 {
        let dzien = doba(&b.market, &mut f, Tick(d * DOBA));
        assert_eq!(dzien.sites, 1, "delegacja zniknęła w dobie {d}");
        wykonan += dzien.applied;
        dobowe += dzien.applied + dzien.alerts;
    }

    let koniec = b.market.price_at(site, g).expect("cena po roku");
    // Ceny nie uciekają: rok bez gracza nie wywraca ceny o rząd wielkości w żadną
    // stronę. Pasmo jest szerokie z rozmysłu — test pilnuje **degeneracji**,
    // a nie kalibracji cennika, którą stroi balansator.
    assert!(
        koniec.get() * 4 > start.get() && koniec.get() < start.get() * 4,
        "cena uciekła: {start:?} → {koniec:?}"
    );
    // Magazyn nie pustoszeje: polityka zapasu ustawiła cel zamówienia i on stoi.
    let cel = b
        .market
        .reorder_target(site, g)
        .expect("polityka zapasu nie ustawiła celu");
    assert!(cel.get() > 0, "cel zamówienia spadł do zera");
    assert!(wykonan > 0, "polityka przez rok nie zamówiła ani razu");
    assert!(dobowe >= 360, "polityka milczała w części dób: {dobowe}");
}

#[test]
fn martwa_strefa_wstrzymuje_polityke_a_nie_gasi_jej() {
    // `cooldown_h` z M9d na polityce **godzinowej** — bo tylko tam znaczy cokolwiek:
    // polityka dobowa i tak pyta raz na dobę, więc martwa strefa krótsza niż doba
    // byłaby dla niej martwym polem. Preset przecen ma kadencję godzinową i strefę
    // sześciu godzin, więc po wykonaniu ma milczeć przez pięć kolejnych godzin.
    let g = mydlo();
    let b = bench(73, &[Vec2::new(300.0, 0.0)]);
    b.market.deliver_now(
        b.sites[0],
        g,
        Qty(2_000_000),
        Money(2_000 * WHOLESALE_BASE),
        None,
        Tick(0),
    );
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let mut f = firmy(
        &b,
        &preset("perishables_markdown", PolicyDomain::Pricing),
        Some(ManagerStyle::Bureaucrat),
    );
    let pierwsza = doba(&b.market, &mut f, Tick(DOBA));
    assert_eq!(pierwsza.sites, 1, "polityka godzinowa nie weszła do doby");
    assert_eq!(pierwsza.on_cooldown, 0, "pierwsze wykonanie nie ma czekać");
    // Godzinę później — martwa strefa ma sześć godzin.
    let druga = b.market.run_policies(
        &mut f,
        &std::collections::BTreeMap::new(),
        None,
        Tick(DOBA + 60),
    );
    assert_eq!(druga.on_cooldown, 1, "martwa strefa nie zadziałała");
    assert_eq!(druga.applied, 0);
    // Po sześciu godzinach polityka wraca do pracy.
    let trzecia = b.market.run_policies(
        &mut f,
        &std::collections::BTreeMap::new(),
        None,
        Tick(DOBA + 6 * 60),
    );
    assert_eq!(trzecia.on_cooldown, 0, "martwa strefa nie wygasła");

    // A polityka **dobowa** w środku doby nie wchodzi nawet do licznika: nie jest
    // wstrzymana, tylko nie ma dziś więcej terminów.
    let mut f = firmy(
        &b,
        &preset("retail_discount", PolicyDomain::Pricing),
        Some(ManagerStyle::Bureaucrat),
    );
    let w_dobie = doba(&b.market, &mut f, Tick(2 * DOBA));
    assert_eq!(w_dobie.sites, 1);
    let w_srodku = b.market.run_policies(
        &mut f,
        &std::collections::BTreeMap::new(),
        None,
        Tick(2 * DOBA + 60),
    );
    assert_eq!(
        w_srodku.sites, 0,
        "polityka dobowa wykonała się w środku doby"
    );
}

#[test]
fn autonomia_ogranicza_menedzera_a_nie_polityke() {
    // Menedżer z autonomią „tylko ceny" nie wykona polityki zapasu. To nie jest
    // błąd polityki — to jest zakres pełnomocnictwa, i różnica ma być widoczna
    // w liczniku, a nie w ciszy.
    let g = mydlo();
    let b = bench(74, &[Vec2::new(300.0, 0.0)]);
    b.market.deliver_now(
        b.sites[0],
        g,
        Qty(2_000_000),
        Money(2_000 * WHOLESALE_BASE),
        None,
        Tick(0),
    );
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let mut f = firmy(
        &b,
        &preset("stock_min_max", PolicyDomain::Stock),
        Some(ManagerStyle::Bureaucrat),
    );
    if let Some(s) = f.site_mut(b.sites[0]) {
        if let Some(d) = s.delegation.as_mut() {
            d.autonomy = Autonomy::PricesOnly;
        }
    }
    let dzien = doba(&b.market, &mut f, Tick(DOBA));
    assert_eq!(dzien.sites, 1);
    assert_eq!(
        dzien.applied, 0,
        "menedżer bez pełnomocnictwa zamówił towar"
    );
}

#[test]
fn odejscie_menedzera_obniza_jakosc_i_odbiera_autonomie() {
    // §5.4: „odejście menedżera zakładu natychmiast obniża `ManagementQuality`
    // do wartości zastępstwa, a autonomię do `PricesOnly`".
    let b = bench(75, &[Vec2::new(300.0, 0.0)]);
    let site = b.sites[0];
    let mut f = firmy(
        &b,
        &preset("retail_discount", PolicyDomain::Pricing),
        Some(ManagerStyle::Coach),
    );
    let przed = f.site(site).expect("zakład").mgmt;
    assert_eq!(przed.0, 70, "menedżer w optimum rozpiętości");

    let t = magnat_firms::LaborTuning::load_default()
        .expect("data/tuning/labor.ron")
        .manager;
    assert_eq!(f.release_manager(CitizenId(ent(800)), &t, Tick(DOBA)), 1);

    let zaklad = f.site(site).expect("zakład");
    assert!(zaklad.mgmt.0 < przed.0, "zastępstwo nie jest gorsze");
    let d = zaklad.delegation.as_ref().expect("polityka zostaje");
    assert!(d.manager.is_none());
    assert_eq!(d.autonomy, Autonomy::PricesOnly);
    // Polityka **zostaje** — to ona jest tym, co zakład umie robić sam.
    assert!(!d.policy.rules.is_empty());
}

#[test]
fn polityka_nie_widzi_ukrytych_danych_sasiada() {
    // Zalążek testu asymetrii informacji z M7 §7.3: zmiana **ukrytej** danej jednego
    // sklepu (koszt nabycia zapasu) nie ma prawa zmienić decyzji drugiego. Pełny test
    // dwuprzebiegowy należy do M7e razem z `FirmView`; ten sprawdza tę jego część,
    // którą da się sprawdzić na widoku sklepu.
    let g = mydlo();
    let cena_pierwszego = |koszt_drugiego: i64| -> Money {
        let b = bench(76, &[Vec2::new(300.0, 0.0), Vec2::new(2_000.0, 0.0)]);
        b.market.deliver_now(
            b.sites[0],
            g,
            Qty(2_000_000),
            Money(2_000 * WHOLESALE_BASE),
            None,
            Tick(0),
        );
        b.market.deliver_now(
            b.sites[1],
            g,
            Qty(2_000_000),
            Money(koszt_drugiego),
            None,
            Tick(0),
        );
        b.market.restock_shelves();
        b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
        let mut f = firmy(
            &b,
            &preset("retail_discount", PolicyDomain::Pricing),
            Some(ManagerStyle::Bureaucrat),
        );
        for d in 1..=3u64 {
            doba(&b.market, &mut f, Tick(d * DOBA));
        }
        b.market.price_at(b.sites[0], g).expect("cena")
    };
    let tanio = cena_pierwszego(2_000 * WHOLESALE_BASE / 3);
    let drogo = cena_pierwszego(2_000 * WHOLESALE_BASE * 3);
    assert_eq!(
        tanio, drogo,
        "cena sklepu zmieniła się po zmianie ukrytego kosztu sąsiada"
    );
}

#[test]
fn dwa_przebiegi_tego_samego_ziarna_daja_te_same_ceny() {
    let g = mydlo();
    let przebieg = || -> Vec<Money> {
        let b = bench(77, &[Vec2::new(300.0, 0.0), Vec2::new(340.0, 0.0)]);
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
        let mut f = firmy(
            &b,
            &preset("retail_discount", PolicyDomain::Pricing),
            Some(ManagerStyle::Dealmaker),
        );
        for d in 1..=30u64 {
            doba(&b.market, &mut f, Tick(d * DOBA));
        }
        b.sites
            .iter()
            .map(|s| b.market.price_at(*s, g).expect("cena"))
            .collect()
    };
    assert_eq!(przebieg(), przebieg());
}

/// Sterownik ceny po wykonaniu `SetPrice` stoi na `Fixed` — to jest **znaczenie**
/// tej akcji, a nie szczegół wykonania (patrz `policy_run::ustaw_cene`).
#[test]
fn ustaw_cene_przestawia_sterownik_na_cene_stala() {
    let g = mydlo();
    let b = bench(78, &[Vec2::new(300.0, 0.0), Vec2::new(340.0, 0.0)]);
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
    let mut f = firmy(
        &b,
        &preset("retail_discount", PolicyDomain::Pricing),
        Some(ManagerStyle::Bureaucrat),
    );
    doba(&b.market, &mut f, Tick(DOBA));
    assert!(matches!(
        b.market.price_policy(b.sites[0], g),
        Some(PricePolicy::Fixed { .. }) | Some(PricePolicy::Markup { .. })
    ));
}

#[test]
fn saldo_zakladu_dociera_do_reguly() {
    // Metryka `saldo` czyta rachunek bieżący zakładu, a nie zero. Bez tego reguła
    // „nie zamawiaj, gdy nie masz z czego" byłaby zawsze prawdziwa.
    let g = mydlo();
    let b = bench(79, &[Vec2::new(300.0, 0.0)]);
    b.market.deliver_now(
        b.sites[0],
        g,
        Qty(2_000_000),
        Money(2_000 * WHOLESALE_BASE),
        None,
        Tick(0),
    );
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
    let konta = b.market.shop_accounts();
    assert_eq!(konta.len(), 1);
    let saldo = b.books.balance(konta[0].1).expect("konto zakładu");
    assert!(saldo.get() > 0, "zakład bez salda");
}

/// Numer polityki jest **pozycją w katalogu**, więc nie zależy od kolejności przypinania.
#[test]
fn numer_polityki_pochodzi_z_katalogu_a_nie_z_kolejnosci() {
    let c = PolicyCatalog::load_default().expect("data/policies/presets.ron");
    let a = magnat_economy::preset_for(&c, "retail_discount", PolicyDomain::Pricing).unwrap();
    let b = magnat_economy::preset_for(&c, "retail_discount", PolicyDomain::Pricing).unwrap();
    assert_eq!(a.id, b.id);
    assert_ne!(
        a.id,
        magnat_economy::preset_for(&c, "stock_min_max", PolicyDomain::Stock)
            .unwrap()
            .id
    );
    assert_ne!(a.id, PolicyId(u16::MAX));
}

#[test]
fn dyskont_faktycznie_schodzi_ponizej_najtanszego_konkurenta() {
    // **Test, który złapał najpoważniejszy błąd tej podfazy.** Preset `retail_discount`
    // to PRD §6.3 wprost: „−2 % względem najtańszego konkurenta w promieniu 3 km"
    // z ogranicznikiem chroniącym przed sprzedażą poniżej kosztu. Obie akcje wykonują
    // się w jednej regule — i dopóki `ClampPrice` liczyło się na cenie **sprzed**
    // `SetPrice`, druga akcja cofała pierwszą i sztandarowa polityka fazy nie robiła nic.
    //
    // Test sprawdza **skutek**, nie mechanizm: cena zdelegowanego sklepu ma zejść
    // poniżej ceny sąsiada. Asercja na samym „cena się zmieniła" przechodziłaby
    // także wtedy, gdy zmienia ją przecena, a nie polityka.
    let g = mydlo();
    let b = bench(80, &[Vec2::new(300.0, 0.0), Vec2::new(340.0, 0.0)]);
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

    // Sąsiad stoi **niżej o dziesięć procent** i nie jest zdelegowany: to on jest
    // punktem odniesienia, a nie drugą stroną tej samej reguły. Dziesięć procent,
    // a nie połowa — cel „−2 %" ma zmieścić się **nad** podłogą ogranicznika, żeby
    // test mierzył samą regułę, a nie to, gdzie ją przycięto.
    let start = b.market.price_at(b.sites[0], g).expect("cena");
    b.market
        .set_price(b.sites[1], g, Money(start.get() * 9 / 10));
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let mut f = Firms::new();
    let key = f.insert(|k| {
        Firm::sole_owner(
            k,
            "Dyskont".to_owned(),
            SimMinute(0),
            DistrictId(0),
            Owner::Player,
        )
    });
    assert!(f.add_site(zaklad(b.sites[0], key, 0)));
    deleguj(
        &mut f,
        b.sites[0],
        preset("retail_discount", PolicyDomain::Pricing),
        Some(ManagerStyle::Bureaucrat),
        0,
    );

    // Dwie doby: pierwsza buduje obraz konkurencji, druga na nim działa.
    //
    // **Bez `reprice_all`** i to jest istota testu: dobowa przecena sąsiada wróciłaby
    // po własną cenę i test mierzyłby ją zamiast polityki. Tu ma być widać wyłącznie
    // to, co zrobiła reguła.
    for d in 1..=2u64 {
        b.market.observe_competitors(Tick(d * DOBA));
        b.market.run_policies(
            &mut f,
            &std::collections::BTreeMap::new(),
            None,
            Tick(d * DOBA),
        );
    }

    // Mierzymy **wyjście reguły**, czyli sterownik ceny, a nie cenę w ofercie:
    // cenę do oferty przenosi dopiero `reprice_all`, a ten w tym teście nie biegnie,
    // żeby przecena sąsiada nie ruszyła punktu odniesienia w trakcie pomiaru.
    let (najtanszy, _, ilu) = b
        .market
        .competitor_entry(b.sites[0], g)
        .expect("sklep nie zobaczył sąsiada");
    assert_eq!(
        ilu, 1,
        "obraz konkurencji nie widzi dokładnie jednego sąsiada"
    );
    let cel = najtanszy.mul_ratio(9_800, 10_000);

    match b.market.price_policy(b.sites[0], g) {
        Some(PricePolicy::Fixed { price }) => {
            assert_ne!(
                price, start,
                "polityka nie ruszyła ceny ani o grosz — ogranicznik cofnął własną regułę"
            );
            assert_eq!(
                price, cel,
                "cena nie trafiła w cel −2 % wobec najtańszego sąsiada ({najtanszy:?})"
            );
        }
        inne => panic!("reguła nie ustawiła ceny stałej, tylko {inne:?}"),
    }
}

/// R2-WP22 (poz. 53): `RemoveFromShelf` zdejmuje linię z półki i zwalnia ofertę.
///
/// Do R2e akcja przechodziła walidator, wykonywała się i **nie robiła nic** —
/// wykonawca zwracał `PolicyOutcome::Blind`, bo zwolnienie oferty w arenie razem
/// z linią półki nie miało ścieżki. Gracz wybierał ją z listy w edytorze reguł,
/// więc był to nie dług, tylko obietnica bez pokrycia: polityka „Nabiał —
/// nie wyrzucamy" z `M9d` §5.6 kończyła się akcją, która nic nie wycofywała.
///
/// Przed naprawą oferta stoi dalej i `blind` rośnie; po naprawie linia znika,
/// oferta wraca do areny, a `applied` rośnie.
#[test]
fn wycofanie_z_polki_zdejmuje_linie_i_zwalnia_oferte() {
    use magnat_policy::{Action, Cadence, ConditionExpr, GoodRef, Rule};

    let g = mydlo();
    let b = bench(91, &[Vec2::new(300.0, 0.0)]);
    b.market.deliver_now(
        b.sites[0],
        g,
        Qty(2_000_000),
        Money(2_000 * WHOLESALE_BASE),
        None,
        Tick(0),
    );
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let ofert_przed = b.market.offer_count();
    assert!(
        b.market.shelf_price(b.sites[0], g).is_some(),
        "towar nie stanął na półce — test nie ma czego zdejmować"
    );

    // Polityka bezwarunkowa: wycofaj ten towar. Zakres `Always`, bo sprawdzamy
    // wykonawcę, a nie warunek.
    let polityka = magnat_policy::FirmPolicy {
        id: PolicyId(77),
        name: "Wycofanie".to_string(),
        domain: PolicyDomain::Stock,
        rules: vec![Rule {
            when: ConditionExpr::Always,
            then: [Action::RemoveFromShelf(GoodRef::Id(g))]
                .into_iter()
                .collect(),
            enabled: true,
            note: String::new(),
        }],
        fallback: None,
        cadence: Cadence::Daily,
        cooldown_h: 0,
    };
    let mut f = firmy(&b, &polityka, Some(ManagerStyle::Bureaucrat));
    let d = doba(&b.market, &mut f, Tick(DOBA));

    // Polityka chodzi raz na towar stojący na półce, a wycofuje **wskazany** —
    // więc pierwsze przejście zdejmuje linię, a kolejne trafiają w pustkę i są
    // ślepe. To jest poprawne: „zdjąłem coś, czego nie miałem" byłoby nieprawdą.
    // Przed naprawą `applied` było zerem, bo wykonawca zwracał `Blind` zawsze.
    assert!(d.applied >= 1, "akcja nie wykonała się: {d:?}");
    assert!(
        b.market.shelf_price(b.sites[0], g).is_none(),
        "towar został na półce mimo wycofania"
    );
    assert!(
        b.market.offer_count() < ofert_przed,
        "oferta nie wróciła do areny: {} wobec {ofert_przed}",
        b.market.offer_count()
    );
}

/// R2-WP22 (poz. 54): dwie reguły o różnych promieniach dają **różne** liczby.
///
/// Pole `radius_m` w metryce konkurencyjnej wchodziło do walidatora (limit 10 km)
/// i **nie wchodziło do odczytu**: obraz konkurencji powstawał jednym promieniem
/// obserwacji, więc reguła „najtańszy w 1 km" i reguła „najtańszy w 8 km" dostawały
/// tę samą cenę. Gracz widział dwie różne reguły i jeden wynik, a PRD §6.3 cytuje
/// „w promieniu 3 km" jako **treść** reguły.
///
/// Scena: dwaj konkurenci w różnej odległości i różnej cenie. Bliski jest droższy,
/// daleki tańszy — więc zawężenie promienia ma podnieść widzianą cenę najtańszego.
/// Przed naprawą obie liczby są równe.
#[test]
fn promien_metryki_zaweza_obraz_konkurencji() {
    use magnat_policy::{Action, Cadence, ConditionExpr, Expr, GoodRef, Metric, Rule};

    let g = mydlo();
    // Nasz sklep w zerze, sąsiad bliski 400 m dalej, daleki 2 500 m dalej.
    let b = bench(
        93,
        &[
            Vec2::new(0.0, 0.0),
            Vec2::new(400.0, 0.0),
            Vec2::new(2_500.0, 0.0),
        ],
    );
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

    let start = b.market.price_at(b.sites[0], g).expect("cena");
    // Bliski sąsiad drożej, daleki zostaje przy cenie katalogowej. Podnosimy tylko
    // jedną cenę, bo obniżenie drugiej przycięłoby się o ogranicznik marży
    // (`min_margin_bp`) i obaj sąsiedzi wyszliby z tą samą kwotą — a wtedy test
    // mierzyłby ogranicznik, nie promień.
    b.market
        .set_price(b.sites[1], g, Money(start.get() * 15 / 10));
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    // Polityka pyta o dwa promienie: ciasny (tylko bliski sąsiad) i szeroki (obaj).
    let metryka = |r: u32| Metric::CheapestCompetitorPrice {
        good: GoodRef::This,
        radius_m: r,
        basis: magnat_core::PriceBasis::GrossRetail,
    };
    let polityka = magnat_policy::FirmPolicy {
        id: PolicyId(88),
        name: "Dwa promienie".to_string(),
        domain: PolicyDomain::Pricing,
        rules: vec![Rule {
            when: ConditionExpr::Cmp {
                lhs: Expr::Metric(metryka(1_000)),
                op: magnat_policy::CmpOp::Gt,
                rhs: Expr::Metric(metryka(5_000)),
            },
            then: [Action::SetPrice {
                good: GoodRef::This,
                to: Expr::Metric(metryka(1_000)),
            }]
            .into_iter()
            .collect(),
            enabled: true,
            note: String::new(),
        }],
        fallback: None,
        cadence: Cadence::Daily,
        cooldown_h: 0,
    };

    // Delegowany jest **tylko nasz sklep**: gdyby politykę dostali też sąsiedzi,
    // przestawiliby sobie ceny i test mierzyłby własną scenę zamiast promienia.
    let mut f = Firms::new();
    let key = f.insert(|k| {
        Firm::sole_owner(
            k,
            "Obserwator".to_owned(),
            SimMinute(0),
            DistrictId(0),
            Owner::Player,
        )
    });
    assert!(f.add_site(zaklad(b.sites[0], key, 0)));
    deleguj(
        &mut f,
        b.sites[0],
        polityka,
        Some(ManagerStyle::Bureaucrat),
        0,
    );

    // Trzy doby: pierwsza zgłasza promienie, druga buduje na nich obraz, trzecia
    // na nim działa. Opóźnienie jest zamierzone i opisane przy `Shop::asked_radii`.
    for d in 1..=3u64 {
        b.market.observe_competitors(Tick(d * DOBA));
        doba(&b.market, &mut f, Tick(d * DOBA));
    }

    let obs = b
        .market
        .observed_of(b.sites[0], g)
        .expect("obraz konkurencji");
    let ciasny = obs
        .near
        .iter()
        .find(|n| n.radius_m == 1_000)
        .expect("slot promienia 1 km");
    let szeroki = obs
        .near
        .iter()
        .find(|n| n.radius_m == 5_000)
        .expect("slot promienia 5 km");
    assert_eq!(ciasny.offers, 1, "w 1 km stoi dokładnie jeden konkurent");
    assert_eq!(szeroki.offers, 2, "w 5 km stoją obaj");
    assert!(
        ciasny.cheapest > szeroki.cheapest,
        "zawężenie promienia nie zmieniło ceny najtańszego: {} wobec {}",
        ciasny.cheapest.get(),
        szeroki.cheapest.get()
    );
}
