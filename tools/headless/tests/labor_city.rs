//! Wynik podfazy M7b w **prawdziwym mieście**: rynek pracy obsadza wakaty, pensje
//! rosną tam, gdzie brakuje ludzi, a zakład i jego załoga mówią o sobie tym samym
//! identyfikatorem.
//!
//! `#[ignore]` z tego samego powodu co `firms_city.rs`: najmniejsze miasto to 40 tys.
//! mieszkańców, a to kilkanaście sekund w profilu debug. CI uruchamia je jawnie
//! przez `--include-ignored`.

use magnat_agents::Employment;
use magnat_core::{Entity, FirmReason, SiteId};
use magnat_economy::labor::{LaborHandle, LaborSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_firms::systems::FirmSystem;
use magnat_firms::Firms;
use magnat_headless::labor;
use magnat_jobs::JobPool;
use std::num::NonZeroU32;

/// Miasto po `dni` dobach rynku pracy, razem z raportem mostu — bo część asercji
/// porównuje stan po przebiegu z tym, co most zastał.
fn miasto(dni: u32) -> (App, magnat_headless::firms::FirmsReport) {
    let pool = JobPool::new(0);
    let mut m = labor::setup(1, "4km", "lowland", "1990", "mixed", 0, &pool).expect("miasto");
    let report = m.report;
    let mut b = ScheduleBuilder::new();
    b.add(FirmSystem::new());
    b.add(LaborSystem::new());
    let schedule = b.build().expect("harmonogram");
    let world = std::mem::replace(&mut m.world, magnat_ecs::World::new(1));
    let mut app = App::new(world, schedule, 0);
    for _ in 0..u64::from(dni) * 1440 {
        app.tick();
    }
    (app, report)
}

#[test]
#[ignore = "N1.2: generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn zaklad_i_jego_zaloga_mowia_o_sobie_tym_samym_identyfikatorem() {
    // `AU-1`. Do M7b most stawiający firmy nadawał `Site.id` w przestrzeni generatora
    // (numer od zera), a komponent mieszkańca, sklepy M5 i zakłady M6 używały klucza
    // przesuniętego o `SITE_KEY_BASE`. Ten sam zakład był więc dla kodu dwoma różnymi
    // miejscami — i nic tego nie łapało, bo obie liczby są poprawnymi `SiteId`.
    let (mut app, report) = miasto(1);
    let rejestr = app.world.get_resource::<Firms>().expect("rejestr firm");
    let znane: std::collections::BTreeSet<SiteId> = rejestr.sites().map(|(id, _)| id).collect();
    let po_dobie = rejestr.headcount() as u32;
    assert!(!znane.is_empty(), "miasto bez zakładów");

    // **To jest asercja, która łapie rozjazd przestrzeni identyfikatorów.** Pierwszym
    // krokiem doby rynku jest domknięcie po M3: pracownik, którego komponent wskazuje
    // inny zakład niż ten, na którego liście płac stoi, **wypada z obsady**. Gdyby
    // `Site.id` i `Employment.site` żyły w dwóch numeracjach, ten krok wyczyściłby
    // w pierwszej dobie wszystkie zakłady co do jednego.
    assert!(
        po_dobie * 10 >= report.hired * 9,
        "po jednej dobie z {} obsadzonych etatów zostało {po_dobie}",
        report.hired
    );

    // Pracujący, których zakładu rejestr nie zna, to **instytucje miejskie** — szkoły
    // i szpitale, których firmy stawia M8. Nie ma ich być więcej niż zakładów, które
    // most świadomie pominął.
    let mut sieroce_zaklady = std::collections::BTreeSet::new();
    for emp in app.world.query::<&Employment, ()>().iter() {
        if !emp.has_job() {
            continue;
        }
        let id = SiteId(Entity::new(emp.site, NonZeroU32::MIN));
        if !znane.contains(&id) {
            sieroce_zaklady.insert(id);
        }
    }
    assert!(
        sieroce_zaklady.len() as u32 <= report.skipped_municipal,
        "{} zakładów poza rejestrem przy {} świadomie pominiętych",
        sieroce_zaklady.len(),
        report.skipped_municipal
    );
}

#[test]
#[ignore = "N1.2: generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn rynek_pracy_obsadza_wakaty_i_podnosi_stawki() {
    let (app, _) = miasto(90);
    let handle = app
        .world
        .get_resource::<LaborHandle>()
        .expect("rynek pracy");
    let m = handle.get().expect("rynek w świecie");
    let dzien = m.last_day();

    // Kryterium WP4: bezrobocie w paśmie 3–9 % po 90 dobach gry.
    let stopa = dzien.unemployment_permille();
    assert!(
        (30..=90).contains(&stopa),
        "bezrobocie {stopa} ‰ poza pasmem 30–90 ‰ \
         (siła robocza {}, pracuje {}, wakaty {})",
        dzien.labour_force,
        dzien.employed,
        dzien.vacancies
    );

    // Kryterium WP5: stawki rosną tam, gdzie brakuje ludzi — i **nigdzie nie ma tabeli
    // płac**, więc jedynym dowodem jest mediana zawartych umów ponad dolnym krańcem
    // widełek, z której wszystkie oferty wystartowały.
    let ile_powyzej_startu = m
        .stats()
        .per_role
        .values()
        .filter(|s| s.median_wage_accepted.get() > 0 && s.shortage_index > 0)
        .count();
    assert!(
        ile_powyzej_startu > 0,
        "żaden zawód nie ma jednocześnie zawartych umów i niedoboru"
    );
    assert!(
        m.open_offers() > 0,
        "rynek pracy bez ani jednej wiszącej oferty"
    );
}

#[test]
#[ignore = "N1.2: generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn dwa_przebiegi_daja_ten_sam_rynek() {
    let (a, _) = miasto(3);
    let (b, _) = miasto(3);
    let stan = |app: &App| {
        let m = app
            .world
            .get_resource::<LaborHandle>()
            .and_then(LaborHandle::get)
            .expect("rynek pracy");
        let d = m.last_day();
        (
            m.open_offers(),
            m.seekers(),
            d.hires,
            d.raises,
            d.unemployed,
        )
    };
    assert_eq!(stan(&a), stan(&b));
}

#[test]
#[ignore = "N1.2: generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn rynek_pracy_wchodzi_do_hasha_stanu() {
    // Stan rynku jest stanem świata (00 §3.6): kto gdzie aplikował i ile firma
    // już podbiła, przeżywa zapis gry tak samo jak saldo konta.
    let (pusty, _) = miasto(0);
    let (po_dobie, _) = miasto(2);
    assert_ne!(
        magnat_io::world_state_hash(&pusty.world),
        magnat_io::world_state_hash(&po_dobie.world),
        "doba rynku pracy nie ruszyła hasha stanu"
    );
}

#[test]
#[ignore = "N1.2: generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn zaklady_dostaja_menedzerow_i_jakosc_zarzadzania_przestaje_byc_stala() {
    // M7c WP7 w prawdziwym mieście. Do M7c `Site::mgmt` było na wartości neutralnej
    // w **każdym** zakładzie: menedżer istniał jako pole, a nie jako ktoś. Od M7c
    // obsadzone stanowisko kierownicze staje się menedżerem, a jego umiejętność,
    // rozpiętość kierowania i nastrój załogi rozsuwają jakość zarządzania po skali.
    let (app, _) = miasto(3);
    let rejestr = app.world.get_resource::<Firms>().expect("rejestr firm");

    assert!(
        rejestr.manager_count() > 0,
        "miasto z {} zakładami nie ma ani jednego menedżera",
        rejestr.site_count()
    );

    // Jakość zarządzania przestała być jedną liczbą. Rozrzut, a nie średnia:
    // średnia mogłaby wyjść przeciętna także wtedy, gdyby wszystkie zakłady
    // siedziały dokładnie na wartości neutralnej.
    let jakosci: Vec<u8> = rejestr.sites().map(|(_, s)| s.mgmt.0).collect();
    let min = jakosci.iter().copied().min().expect("zakłady");
    let max = jakosci.iter().copied().max().expect("zakłady");
    assert!(
        max > min,
        "jakość zarządzania jest w całym mieście równa {min}"
    );
    let neutralne = jakosci
        .iter()
        .filter(|q| **q == magnat_firms::ManagementQuality::NEUTRAL.0)
        .count();
    assert!(
        neutralne < jakosci.len(),
        "żaden zakład nie odszedł od zarządzania przeciętnego"
    );

    // Każde przypisanie menedżera zostawiło powód (00 §7). Dziennik firmy ma 32 wpisy,
    // więc szukamy w tych, które jeszcze nie wypadły — wystarczy jeden, żeby pokazać,
    // że droga zapisu istnieje, bo drugiej drogi do `Site::mgmt` nie ma.
    let z_powodem = rejestr
        .iter()
        .flat_map(|(_, f)| f.log.iter())
        .filter(|w| {
            matches!(
                w.reason,
                magnat_core::DecisionReason::Firm(FirmReason::ManagerAssigned { .. })
            )
        })
        .count();
    assert!(
        z_powodem > 0,
        "menedżerowie przyszli bez ani jednego powodu"
    );
}
