//! Wynik do pokazania M7a: **firmy w prawdziwym mieście**, a nie w rejestrze z testu.
//!
//! `#[ignore]` z tego samego powodu co cała rodzina testów generujących świat
//! (`sim/world/tests/city.rs`): najmniejsze miasto to 40 tys. mieszkańców, a to
//! kilkanaście sekund w profilu debug. CI uruchamia je jawnie przez `--include-ignored`.

use magnat_firms::hr::roles::RoleTable;
use magnat_firms::{slots, SiteTypeCatalog, Tier};
use magnat_headless::{firms, population};
use magnat_jobs::JobPool;
use std::collections::BTreeMap;

struct Miasto {
    city: magnat_world::CityData,
    world: magnat_ecs::World,
    types: SiteTypeCatalog,
}

fn zbuduj() -> Miasto {
    let pool = JobPool::new(0);
    let city = population::zbuduj_miasto(1, "4km", "lowland", "1990", "mixed", &pool)
        .expect("miasto 4 km");
    let mut world = population::swiat_agentow(1).expect("świat agentów");
    population::zaludnij(&mut world, &city, 0, 0).expect("zaludnienie");

    let roles = RoleTable::load_default().expect("data/jobs/roles.ron");
    let goods = magnat_supply::catalog::load_default("contemporary").expect("katalog towarów");
    let archetypy: BTreeMap<String, Vec<String>> = city
        .site_catalog
        .archetypes
        .iter()
        .map(|a| {
            (
                a.key().to_owned(),
                a.recipes
                    .iter()
                    .map(|r| goods.recipe(*r).key.to_string())
                    .collect(),
            )
        })
        .collect();
    let types = SiteTypeCatalog::load_default(&roles, &goods, &archetypy).expect("typy zakładów");
    Miasto { city, world, types }
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn miasto_dostaje_firmy_z_trzema_slotami_decyzyjnymi() {
    let mut m = zbuduj();
    let (firms, rep) = firms::zbuduj_firmy(&m.city, &mut m.world, &m.types);

    assert!(rep.firms > 0, "miasto bez ani jednej firmy");
    assert!(rep.sites > 0, "firmy bez ani jednego zakładu");
    assert_eq!(
        rep.skipped_unknown, 0,
        "zakłady bez typu w data/site_types/ — dziura w danych, nie instytucja miejska"
    );

    // Każda firma ma trzy sloty i wszystkie mieszczą się w kalendarzu 12 × 30 (`K-1`).
    for (key, _) in firms.iter() {
        let s = slots(key);
        assert!(s.ops_minute_of_day < 1440);
        assert!(s.tac_day_of_month < 30);
        assert!(s.str_day_of_quarter < 90);
    }

    // Etap 8 kogoś zatrudnił i ci ludzie trafili do firm, a nie zostali po drodze.
    assert!(
        rep.hired > 0,
        "żaden z mieszkańców zatrudnionych w Etapie 8 nie trafił do żadnej firmy"
    );
    assert_eq!(firms.headcount() as u32, rep.hired);
    println!(
        "firm: {}, zakładów: {}, stanowisk: {}, zatrudnionych: {}, pominiętych municypalnych: {}",
        rep.firms, rep.sites, rep.positions, rep.hired, rep.skipped_municipal
    );
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn most_jest_deterministyczny() {
    // Dwa przebiegi tego samego ziarna dają ten sam rejestr — łącznie z kolejnością
    // wypłat, bo ta wychodzi z kolejności obsady, a ta z sortowania po `CitizenId`,
    // nie z kolejności archetypów w ECS.
    let odcisk = |_: u8| {
        let mut m = zbuduj();
        let (firms, rep) = firms::zbuduj_firmy(&m.city, &mut m.world, &m.types);
        let mut h = magnat_core::hash::StateHasher::new();
        magnat_core::hash::HashState::hash_state(&firms, &mut h);
        (h.finish(), rep)
    };
    let (a, ra) = odcisk(0);
    let (b, rb) = odcisk(1);
    assert_eq!(ra, rb);
    assert_eq!(a, b, "most dał dwa różne rejestry z tego samego ziarna");
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn zaklady_maja_obsade_w_rozsadnej_relacji_do_etatow() {
    // Nie testuję tu kalibracji, tylko że most nie zgubił ludzi i nie wymyślił etatów.
    // Wakat nieobsadzony jest **poprawnym** wynikiem (M7 §7.1 pkt 4), więc dolnej
    // granicy nie ma — jest za to górna: nie da się zatrudnić więcej ludzi niż istnieje.
    let mut m = zbuduj();
    let (firms, _) = firms::zbuduj_firmy(&m.city, &mut m.world, &m.types);
    for (_, s) in firms.sites() {
        assert!(
            s.headcount() <= s.required_slots() as usize,
            "zakład ma {} ludzi na {} etatach",
            s.headcount(),
            s.required_slots()
        );
    }
    // Pokrycie etatowe miasta jako całości: gdyby most gubił obsadę, byłoby zerowe.
    let obsadzone: usize = firms.sites().map(|(_, s)| s.headcount()).sum();
    let etaty: u32 = firms.sites().map(|(_, s)| s.required_slots()).sum();
    assert!(etaty > 0);
    println!("obsadzonych {obsadzone} z {etaty} etatów");
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn doba_decyzji_obejmuje_kazda_firme_dokladnie_raz() {
    let mut m = zbuduj();
    let (mut firms, _) = firms::zbuduj_firmy(&m.city, &mut m.world, &m.types);
    let ile_firm = firms.len();
    let mut widziane = std::collections::BTreeSet::new();
    for minuta in 0..1440u64 {
        for (tier, keys) in firms.schedule(magnat_core::SimCalendar::new(magnat_core::Tick(minuta)))
        {
            if tier == Tier::Operational {
                for k in keys {
                    assert!(
                        widziane.insert(k),
                        "firma {k:?} decydowała dwa razy w dobie"
                    );
                }
            }
        }
    }
    assert_eq!(
        widziane.len(),
        ile_firm,
        "któraś firma nie decydowała wcale"
    );
    assert_eq!(
        firms.backlog(Tier::Operational),
        0,
        "kolejka nie domknęła się w dobie"
    );
}
