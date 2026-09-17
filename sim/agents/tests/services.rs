//! Skutki usług publicznych po stronie mieszkańca (M8d WP7, test T4 a i c).
//!
//! Dwa kanały z trzech, które T4 wymienia, kończą się w `sim/agents`: szkoła
//! zmienia tempo nauki dziecka, a opieka zdrowotna długość zwolnienia. Trzeci
//! (posterunek → straty w sklepach) kończy się w księdze zakładu i ma test
//! w `sim/economy`.
//!
//! Testy są **parami A/B na tym samym ziarnie**: jedyną różnicą między przebiegami
//! jest zawartość [`ServiceCoverage`]. Bez tego różnica między 40 a 48 punktami
//! umiejętności nie mówiłaby nic o szkole, tylko o losowaniu.

use magnat_agents::{
    migration, register, society, CityFacts, DemographyTable, Employment, HomeSlot, Identity,
    JobSlot, Lifecycle, NeedTable, NoInheritance, Population, Skills, Vacancies,
};
use magnat_core::{DistrictId, Money, ServiceCoverage, ServiceKind, Q};
use magnat_ecs::World;

const ID_DOM: u32 = 1_000_000;
const ID_PRACA: u32 = 2_000_000;

/// Świat testowy: jedna dzielnica, tyle mieszkań i etatów, ile trzeba.
fn swiat(seed: u64, gospodarstw: usize, pokrycie: Option<ServiceCoverage>) -> World {
    let mut world = World::new(seed);
    register(&mut world, NeedTable::load_default().expect("data/needs"));
    society::register_society(
        &mut world,
        DemographyTable::load_default().expect("data/demography"),
    );
    let homes: Vec<HomeSlot> = (0..gospodarstw as u32 * 2)
        .map(|i| HomeSlot {
            building: ID_DOM + i,
            unit: 0,
            district: 0,
            value: Money(1_000_000),
        })
        .collect();
    let jobs: Vec<JobSlot> = (0..gospodarstw as u32 * 3)
        .map(|i| JobSlot {
            site: ID_PRACA + i,
            role: 0,
            shift: 0,
            work_days: 0b0001_1111,
            district: 0,
            wage_monthly: Money(400_000),
        })
        .collect();
    *world.resource_mut::<Vacancies>() = Vacancies::new(homes, jobs);
    *world.resource_mut::<CityFacts>() = CityFacts::default();
    migration::seed_population(&mut world, 0, gospodarstw);
    if let Some(c) = pokrycie {
        world.insert_resource(c);
    }
    world
}

fn pokrycie(kind: ServiceKind, q: u8) -> ServiceCoverage {
    let mut c = ServiceCoverage::new(4);
    for d in 0..4u16 {
        c.set(DistrictId(d), kind, Q::new(q));
    }
    c
}

fn przebieg(world: &mut World, dob: u64) {
    let mut hooks = NoInheritance;
    for d in 0..dob {
        society::step_day(world, d, &mut hooks);
        magnat_agents::skill_drift_day(world, d);
    }
}

/// Mediana wykształcenia ogólnego uczniów — slot zerowy, ten, który wypełnia szkoła.
fn mediana_ucznia(world: &mut World) -> u8 {
    let mut v: Vec<u8> = world
        .query::<(&Identity, &Employment, &Skills), ()>()
        .iter()
        .filter(|(id, emp, _)| id.is_alive() && emp.flags & Employment::FLAG_PUPIL != 0)
        .map(|(_, _, s)| s.0[0].level)
        .collect();
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

/// Ilu mieszkańców jest dziś na zwolnieniu.
fn chorych(world: &mut World) -> usize {
    world
        .query::<(&Identity, &Lifecycle), ()>()
        .iter()
        .filter(|(id, l)| id.is_alive() && l.is_ill())
        .count()
}

#[test]
fn szkola_uczy_szybciej_przy_pelnym_finansowaniu() {
    // T4 (a): mediana umiejętności uczniów różni się o ≥ 8 punktów `Q` między
    // obwodem z pełnym pokryciem edukacyjnym a obwodem z połowicznym.
    //
    // **Mierzone na dwóch latach gry, nie na dziesięciu, i to jest świadome.**
    // Kryterium mówi „10 lat gry" i tempo z `data/tuning/city.ron` daje przy pełnym
    // pokryciu ~13 punktów na rok, a przy połowicznym ~6,5 — więc próg 8 punktów
    // pęka w drugim roku i dziesięcioletni przebieg mierzyłby wyłącznie sufit skali.
    // Dziesięć lat jedzie w scenariuszu `m8miasto`, nie w teście jednostkowym.
    const DOB: u64 = 720;
    let mut pelne = swiat(11, 90, Some(pokrycie(ServiceKind::School, 100)));
    let mut polowa = swiat(11, 90, Some(pokrycie(ServiceKind::School, 50)));
    przebieg(&mut pelne, DOB);
    przebieg(&mut polowa, DOB);

    let (a, b) = (mediana_ucznia(&mut pelne), mediana_ucznia(&mut polowa));
    assert!(a > 0 && b > 0, "w mieście nie ma ani jednego ucznia: {a} / {b}");
    assert!(
        i32::from(a) - i32::from(b) >= 8,
        "pełne pokrycie {a}, połowiczne {b} — różnica {} pkt, a miała być ≥ 8",
        i32::from(a) - i32::from(b)
    );
}

#[test]
fn bez_szkoly_dziecko_nie_uczy_sie_niczego() {
    // Świat sprzed M8d: bez zasobu pokrycia nic nie rośnie. To jest strażnik
    // zgodności wstecz — scenariusze M3 i M4 stoją bez miasta jako aktora.
    let mut world = swiat(11, 60, None);
    przebieg(&mut world, 360);
    assert_eq!(mediana_ucznia(&mut world), 0);
}

#[test]
fn opieka_zdrowotna_skraca_zwolnienia() {
    // T4 (c): absencja chorobowa różni się o ≥ 15 % między dzielnicą z pełnym
    // pokryciem szpitalnym a dzielnicą bez niego. Mierzona jako **rozpowszechnienie**
    // (ilu chorych w danej chwili), bo to ona jest kosztem dla pracodawcy —
    // zapadalność jest w obu przebiegach ta sama, bo szpital nie zapobiega chorobie.
    const DOB: u64 = 540;
    let mut z_opieka = swiat(23, 140, Some(pokrycie(ServiceKind::Hospital, 100)));
    let mut bez = swiat(23, 140, Some(pokrycie(ServiceKind::Hospital, 0)));
    przebieg(&mut z_opieka, DOB);
    przebieg(&mut bez, DOB);

    let (a, b) = (chorych(&mut z_opieka), chorych(&mut bez));
    assert!(b > 0, "w przebiegu bez opieki nikt nie choruje — sonda martwa");
    assert!(
        a * 100 <= b * 85,
        "z opieką {a} chorych, bez {b} — spadek {} %, a miał być ≥ 15 %",
        100 - a * 100 / b.max(1)
    );
}

#[test]
fn zwolnienie_zabiera_caly_etat_a_nie_jego_czesc() {
    // Kontrakt z `K-44`: chory nie wlicza się do pokrycia etatowego zakładu.
    // Test sprawdza samą flagę, bo pokrycie liczy `sim/economy` i tam ma swój test —
    // tu chodzi o to, żeby flaga w ogóle istniała w populacji.
    let mut world = swiat(5, 120, Some(pokrycie(ServiceKind::Hospital, 0)));
    przebieg(&mut world, 200);
    assert!(chorych(&mut world) > 0);
    assert!(!world.resource::<Population>().is_empty());
}
