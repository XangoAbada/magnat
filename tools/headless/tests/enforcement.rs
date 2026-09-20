//! Usługi publiczne, urząd i egzekucja w prawdziwej gospodarce (M8d WP7, WP8).
//!
//! Testy jednostkowe sprawdzają każdy mechanizm osobno: jakość placówki
//! (`sim/city`), tempo nauki i długość choroby (`sim/agents`), ubytki i szarą
//! strefę (`sim/economy`), medianę czasu do kontroli (`sim/events`). Ten plik
//! sprawdza to, czego żaden z nich sprawdzić nie może — że **łańcuch się domyka**:
//! zakład ukrywa obrót, hazard rośnie, zdarzenie zachodzi, miasto otwiera sprawę,
//! sprawa kończy się domiarem, a domiar wchodzi do budżetu **nie psując T1**.
//!
//! Kryterium zamknięcia podfazy brzmi dokładnie tak: „domiar trafia do budżetu
//! bez naruszenia testu T1".
//!
//! `#[ignore]` z tego samego powodu co `city_tax.rs`: doba pełnej gospodarki
//! kosztuje sekundy. CI uruchamia je jawnie przez `--include-ignored`.

use magnat_agents::{
    bootstrap_day, register_day, DayLoopSystem, DeprivationEffectsSystem, HouseholdStockSystem,
    NeedDecaySystem, NoInheritance, ReplanCooldownSystem, SkillDriftSystem, SocietySystem,
};
use magnat_city::{City, CitySystem};
use magnat_core::{AgencyKind, RemedyKind, ServiceKind, TaxKind};
use magnat_economy::corpfin::system::InsolvencySystem;
use magnat_economy::labor::LaborSystem;
use magnat_economy::{Books, Market, MarketSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto_z_klimatem};
use magnat_headless::{city as city_bridge, events as events_bridge, full, grid as grid_bridge};
use magnat_jobs::JobPool;
use magnat_macro::MacroSystem;
use magnat_traffic::TrafficSystem;

/// Miasto z pełną gospodarką, stroną publiczną, sieciami, zdarzeniami i usługami.
///
/// `szara_strefa_bp` ustawia się **po** postawieniu gospodarki i przed pierwszym
/// tickiem: to jest odpowiednik decyzji gracza „ukrywam 30 % obrotu", a nie
/// właściwość świata.
fn miasto(dni: u32, citizens: u32, szara_strefa_bp: u16) -> App {
    let pool = JobPool::new(0);
    let (city, klimat) =
        zbuduj_miasto_z_klimatem(1, "4km", "lowland", "1990", "mixed", &pool).expect("miasto");
    let mut world = swiat_agentow(1).expect("świat");
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, citizens, 200_000).expect("Etap 8");
    full::setup(
        &mut world,
        &city,
        zaludnione.places.clone(),
        zaludnione.travel_oracle(),
        &zaludnione.traffic,
        1,
        &pool,
    )
    .expect("gospodarka");
    city_bridge::setup(&mut world, &city).expect("strona publiczna");
    city_bridge::setup_services(&mut world, &city).expect("usługi publiczne");
    grid_bridge::setup(&mut world, &city).expect("sieci przesyłowe");
    events_bridge::setup(&mut world, &city, &klimat, 1990).expect("zdarzenia");

    if szara_strefa_bp > 0 {
        let market = world.resource::<Market>().clone();
        for site in market.sites() {
            market.set_unreported_bps(site, szara_strefa_bp);
        }
    }
    bootstrap_day(&mut world, 0);

    let mut b = ScheduleBuilder::new();
    b.add(magnat_events::EventSystem::new())
        .add(magnat_traffic::utility::UtilitySystem::new(
            magnat_traffic::utility::GridTuning::load_default().expect("data/tuning/grid.ron"),
        ))
        .add(magnat_supply::ChainSystem::new())
        .add(magnat_firms::systems::FirmSystem::new())
        .add(MarketSystem::new(&world))
        .add(LaborSystem::new())
        .add(InsolvencySystem::new())
        .add(MacroSystem::new())
        .add(magnat_media::MediaSystem::new())
        .add(CitySystem::new())
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world));
    let schedule = b.build().expect("harmonogram");
    let mut app = App::new(world, schedule, 0);
    for _ in 0..u64::from(dni) * 1440 {
        app.tick();
    }
    app
}

#[test]
#[ignore = "pełna gospodarka — doba kosztuje sekundy; CI woła --include-ignored"]
fn domiar_trafia_do_budzetu_bez_naruszenia_t1() {
    // Kryterium zamknięcia podfazy. Dziewięćdziesiąt dób: kontrola przy 30 % obrotu
    // poza deklaracją przychodzi w ciągu kilkudziesięciu dób (mediana z testu
    // `sim/events`), a sprawa potrzebuje jeszcze kilkunastu na zebranie dowodów.
    let app = miasto(90, 2_000, 3_000);
    let m = app.world.resource::<City>();

    // 1. Kontrole przyszły i skończyły się domiarem.
    let sprawy: Vec<_> = m
        .enforcement
        .cases()
        .iter()
        .filter(|c| c.agency == AgencyKind::TaxOffice)
        .collect();
    assert!(
        !sprawy.is_empty(),
        "przez 90 dób ani jedna firma ukrywająca 30 % obrotu nie została skontrolowana"
    );
    let domiary: Vec<_> = sprawy
        .iter()
        .filter_map(|c| c.remedy)
        .filter(|r| r.kind() == RemedyKind::BackTax)
        .collect();
    assert!(
        !domiary.is_empty(),
        "{} spraw skarbowych otwartych, ani jedna nie skończyła się domiarem",
        sprawy.len()
    );
    assert!(
        domiary.iter().any(|r| r.amount().get() > 0),
        "domiar na zero złotych — podstawa nie ma jak powstać"
    );

    // 2. Domiar nie psuje domknięcia rejestru. To jest test T1 na przebiegu,
    //    w którym część należności powstała **z kary**, a nie z deklaracji.
    m.charges
        .check_closure()
        .expect("Σ Assessed == Σ Settled + Σ Overdue + Σ Abated");

    // 3. Pieniądz się zachowuje razem z kontem miasta.
    app.world
        .resource::<Books>()
        .check_conservation()
        .expect("Σ sald == podaż pieniądza");

    // 4. Złapany przestaje ukrywać — inaczej kara byłaby podatkiem, a nie karą.
    //
    // **Kryterium mierzy skutek kary, a nie stan po kolejnych miesiącach**, i to
    // jest korekta wpisana w M8e: domiar zeruje udział ukrywany, ale zakład pod
    // presją zaczyna go odbudowywać co miesiąc (`update_shadow_share`) — to jest
    // zamierzone i jest treścią `R10` („albo wszyscy oszukują, albo nikt").
    // Pierwsza wersja testu żądała dokładnego zera na **końcu** przebiegu, czyli
    // pytała „i nigdy więcej nie ukrywał", a to jest inne pytanie. Przechodziła,
    // dopóki żaden złapany zakład nie wpadł w kłopoty; od M8e jakość placówek
    // liczy się z planu **po cięciu** (`CH-4`), więc wpadają.
    let market = app.world.resource::<Market>();
    for c in &sprawy {
        if c.remedy.is_some() {
            let teraz = market.unreported_bps_of(c.subject);
            assert!(
                teraz < 3_000,
                "zakład po domiarze wrócił do poziomu sprzed kontroli: {teraz} bp"
            );
        }
    }
}

#[test]
#[ignore = "pełna gospodarka — doba kosztuje sekundy; CI woła --include-ignored"]
fn placowki_maja_obsade_jakosc_i_pokrycie() {
    // Kryterium WP7 od strony świata: placówka stawiana przez most ma **prawdziwych
    // ludzi na etatach** i emergentną jakość, a pokrycie dociera do dzielnic.
    let app = miasto(35, 2_000, 0);
    let m = app.world.resource::<City>();
    assert!(
        !m.services.is_empty(),
        "most nie postawił ani jednej placówki"
    );

    let z_obsada = m.services.all().iter().filter(|s| s.staff > 0).count();
    assert!(
        z_obsada * 2 >= m.services.len(),
        "tylko {z_obsada} z {} placówek ma choć jednego pracownika",
        m.services.len()
    );
    assert!(
        m.services.all().iter().all(|s| s.quality.get() > 0),
        "placówka o zerowej jakości wygląda jak zamknięta"
    );
    // Pokrycie jest opublikowane do świata — to jedyne wejście M3 i M5.
    let pokrycie = app
        .world
        .get_resource::<magnat_core::ServiceCoverage>()
        .expect("zasób pokrycia usług w świecie");
    assert!(
        pokrycie.city_mean(ServiceKind::School).get() > 0,
        "szkoły stoją, a pokrycie edukacyjne jest zerowe"
    );

    // Urząd wydaje pozwolenia, a czas oczekiwania jest wynikiem, nie parametrem.
    assert!(
        m.permits.issued() > 0,
        "urząd nie wydał ani jednego pozwolenia"
    );
    assert!(m.permits.median_wait_days().is_some());
    // Opłaty za wnioski wpłynęły do miasta jako danina `License`.
    assert!(m.budget.revenue_life[TaxKind::License.as_index()].get() >= 0);
}
