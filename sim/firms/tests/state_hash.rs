//! Stan firm wchodzi do hasha świata (dokument 00 §3.6, bramka 2 fazy, M7 §7.5).
//!
//! Test nie sprawdza konkretnej wartości — złoty odcisk fazy powstaje razem z jej
//! domknięciem. Sprawdza to, co jest tu bezwartościowe, gdy przestaje być prawdą:
//! że ten sam stan daje ten sam hash, i że **każda** rzecz, którą M7a zmienia,
//! ten hash rusza. Komponent poza hashem to dwa przebiegi jednego ziarna, które
//! rozjeżdżają się bez śladu.

use magnat_core::{CitizenReason, DecisionReason, DistrictId, SimCalendar, SimMinute, Tick};
use magnat_ecs::World;
use magnat_firms::firm::{Firm, FirmStatus, Owner};
use magnat_firms::systems::register_firms;
use magnat_firms::{FirmKey, Firms, Tier};
use magnat_io::world_state_hash;

fn swiat(firm: u32) -> World {
    let mut w = World::new(7);
    let mut f = Firms::new();
    for i in 0..firm {
        f.insert(|key| {
            Firm::sole_owner(
                key,
                format!("Firma {i}"),
                SimMinute(0),
                DistrictId((i % 24) as u16),
                Owner::Player,
            )
        });
    }
    register_firms(&mut w, f);
    w
}

fn zmien(f: impl FnOnce(&mut Firms)) -> magnat_io::StateHash {
    let mut w = swiat(50);
    f(w.get_resource_mut::<Firms>().expect("rejestr firm"));
    world_state_hash(&w)
}

#[test]
fn ten_sam_stan_daje_ten_sam_hash() {
    assert_eq!(world_state_hash(&swiat(50)), world_state_hash(&swiat(50)));
}

#[test]
fn zalozenie_firmy_rusza_hash() {
    assert_ne!(world_state_hash(&swiat(50)), world_state_hash(&swiat(51)));
}

#[test]
fn kazda_zmiana_stanu_firmy_rusza_hash() {
    let baza = world_state_hash(&swiat(50));

    /// Jedna zmiana stanu rejestru, opisana nazwą do komunikatu asercji.
    type Przypadek = (&'static str, Box<dyn FnOnce(&mut Firms)>);

    let przypadki: Vec<Przypadek> = vec![
        (
            "zamknięcie firmy",
            Box::new(|f: &mut Firms| {
                f.get_mut(FirmKey(1)).expect("firma").status = FirmStatus::Closed;
            }),
        ),
        (
            "wpis do dziennika decyzji",
            Box::new(|f: &mut Firms| {
                f.log(FirmKey(1), Tick(5), DecisionReason::Unspecified);
            }),
        ),
        (
            "zmiana dyrektora",
            Box::new(|f: &mut Firms| {
                f.get_mut(FirmKey(2)).expect("firma").director = None;
                f.get_mut(FirmKey(2)).expect("firma").hq_district = DistrictId(99);
            }),
        ),
    ];

    for (nazwa, zmiana) in przypadki {
        assert_ne!(baza, zmien(zmiana), "{nazwa} nie ruszyła hasha");
    }
}

#[test]
fn menedzer_i_delegacja_wchodza_do_hasha() {
    // M7c: menedżer zmienia produktywność zakładu, jego rotację i jego straty,
    // a autonomia decyduje, czy polityka w ogóle się wykona. Wszystko troje jest
    // stanem symulacji, więc milczenie hasha o nich znaczyłoby, że dwa przebiegi
    // tego samego ziarna wolno rozjechać w wyniku produkcyjnym całego miasta.
    use magnat_core::{BuildingId, CitizenId, Entity, Money, SiteId, Q};
    use magnat_firms::{
        Autonomy, LaborTuning, Manager, ManagerStyle, Ring, Site, SiteDelegation, SitePnlMonth,
        SiteTypeId,
    };
    use std::num::NonZeroU32;

    let e = |i: u32| Entity::new(i, NonZeroU32::MIN);
    let zaklad = |firm: FirmKey| Site {
        id: SiteId(e(500)),
        firm,
        site_type: SiteTypeId(0),
        building: BuildingId(e(500)),
        district: DistrictId(1),
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
    };
    let t = LaborTuning::load_default()
        .expect("data/tuning/labor.ron")
        .manager;

    let zbuduj = |menedzer: Option<(u32, u8, ManagerStyle, Autonomy)>| {
        let mut w = swiat(3);
        let f = w.get_resource_mut::<Firms>().expect("rejestr");
        assert!(f.add_site(zaklad(FirmKey(1))));
        if let Some((c, skill, styl, autonomia)) = menedzer {
            let obywatel = CitizenId(e(c));
            let mgr = Manager::new(obywatel, Q::new(skill), styl, SimMinute(0));
            let mut deleg = SiteDelegation::new(
                obywatel,
                magnat_policy::Policy::empty(
                    magnat_core::PolicyId(1),
                    "t",
                    magnat_policy::PolicyDomain::Pricing,
                ),
                autonomia,
            );
            deleg.autonomy = autonomia;
            assert!(f.assign_manager(SiteId(e(500)), mgr, deleg, |_| Q::new(50), &t, Tick(0)));
        }
        world_state_hash(&w)
    };

    let bez = zbuduj(None);
    let z_menedzerem = zbuduj(Some((800, 70, ManagerStyle::Coach, Autonomy::Full)));
    assert_ne!(bez, z_menedzerem, "przypisanie menedżera nie ruszyło hasha");

    // Każde z trzech pól osobno: inny człowiek, inna umiejętność, inna autonomia.
    assert_ne!(
        z_menedzerem,
        zbuduj(Some((801, 70, ManagerStyle::Coach, Autonomy::Full))),
        "inny menedżer daje ten sam hash"
    );
    assert_ne!(
        z_menedzerem,
        zbuduj(Some((800, 71, ManagerStyle::Coach, Autonomy::Full))),
        "inna umiejętność zarządzania daje ten sam hash"
    );
    assert_ne!(
        z_menedzerem,
        zbuduj(Some((800, 70, ManagerStyle::Coach, Autonomy::PricesOnly))),
        "inna autonomia daje ten sam hash"
    );
    // Styl kierowania też: z niego wychodzi agresja licytacyjna i wybór kandydata.
    assert_ne!(
        z_menedzerem,
        zbuduj(Some((800, 70, ManagerStyle::Dealmaker, Autonomy::Full))),
        "inny styl kierowania daje ten sam hash"
    );
    // Ten sam stan — ten sam hash, także po przejściu przez przypisanie.
    assert_eq!(
        z_menedzerem,
        zbuduj(Some((800, 70, ManagerStyle::Coach, Autonomy::Full)))
    );
}

#[test]
fn dziennik_decyzji_rozroznia_powod() {
    // Dwa wpisy w tym samym ticku, różne powody — wyjaśnialność jest stanem,
    // a nie ozdobą karty inspekcji (dokument 00 §7).
    let a = zmien(|f| f.log(FirmKey(1), Tick(5), DecisionReason::Unspecified));
    let b = zmien(|f| {
        f.log(
            FirmKey(1),
            Tick(5),
            DecisionReason::Citizen(CitizenReason::NeedCritical {
                need: magnat_core::NeedKind::Hunger,
                level: magnat_core::Q::new(10),
            }),
        );
    });
    assert_ne!(a, b);
}

#[test]
fn kubelki_slotow_nie_wchodza_do_hasha() {
    // Indeks pochodny **nie może** wchodzić: jest funkcją zbioru kluczy i odtwarza
    // się z niego w całości. Gdyby wchodził, jego przebudowa zmieniałaby hash stanu
    // bez zmiany stanu — czyli replay pękałby po każdej reorganizacji indeksu.
    //
    // Dowód pośredni, bo indeks jest prywatny: przydział slotów bez zdjęcia decyzji
    // z kolejki nie zmienia niczego, co jest stanem.
    let mut a = swiat(50);
    let przed = world_state_hash(&a);
    {
        let f = a.get_resource_mut::<Firms>().expect("rejestr");
        // Minuta, w której żadna z 50 firm nie ma slotu operacyjnego — kolejka
        // zostaje pusta, więc stan się nie zmienia.
        let pusta = (0..1440u64)
            .find(|m| {
                let cal = SimCalendar::new(Tick(*m));
                f.iter().all(|(k, _)| {
                    !magnat_firms::due(magnat_firms::slots(k), Tier::Operational, cal)
                })
            })
            .expect("jakaś minuta bez decyzji przy 50 firmach");
        f.schedule(SimCalendar::new(Tick(pusta)));
    }
    assert_eq!(przed, world_state_hash(&a));
}

#[test]
fn kolejka_przepelnienia_jest_stanem_i_rusza_hash() {
    // Przesunięcie decyzji na następny tick jest stanem, nie szczegółem wykonania
    // (M7 §8, ryzyko `R4`): gdyby nie wchodziło do hasha, dwa przebiegi jednego
    // ziarna mogłyby podjąć te same decyzje w **innych** minutach i nikt by tego
    // nie zobaczył.
    //
    // Przepełnienie trzeba wywołać **naprawdę**, a nie założyć. Poziom operacyjny
    // się nie przepełnia — 10 tys. firm na 1440 minut daje pik równy sufitowi 32 —
    // więc bierze się poziom taktyczny: te same firmy mieszczą się w 30 dniach
    // po 24 godziny, czyli kilkanaście na godzinę przy sufycie 4.
    let mut w = swiat(6_000);
    let przed = world_state_hash(&w);
    let f = w.get_resource_mut::<Firms>().expect("rejestr");

    let mut minuta = 0u64;
    while f.backlog(Tier::Tactical) == 0 {
        f.schedule(SimCalendar::new(Tick(minuta)));
        minuta += 60;
        assert!(
            minuta < 30 * 1440,
            "kolejka taktyczna nigdy się nie przepełniła"
        );
    }
    assert_ne!(przed, world_state_hash(&w), "kolejka nie ruszyła hasha");
}
