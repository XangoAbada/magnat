//! Cykl szkolny i wykształcenie w trakcie gry (`R2-WP1`, `R2-WP5`).
//!
//! Każdy test w tym pliku padał przed naprawą — to jest warunek z `R2` §3 pkt 1.
//! Trzy usterki, które opisują, mają wspólną przyczynę: stan nadawany przy generacji
//! świata nigdy nie był nadawany później, a `Employment::has_job()` nie odróżniało
//! ucznia od pracownika.

use magnat_agents::{
    demography, migration, register, society, CityFacts, DemographyTable, Employment, HomeSlot,
    Identity, JobSlot, NeedTable, NoInheritance, PlaceCatalog, PlaceEntry, PlaceTable, Population,
    RelationKind, RelationSlab, RelationsRef, Residence, SocialIndex, Vacancies, Vitals,
};
use magnat_core::{BuildingId, Entity, Money, PlaceKind, PlaceRef, WorldCoord};
use magnat_ecs::World;
use std::num::NonZeroU32;
use std::sync::Arc;

fn budynek(i: u32) -> PlaceRef {
    PlaceRef::Building(BuildingId(Entity::new(i, NonZeroU32::new(1).unwrap())))
}

const ID_DOM: u32 = 1_000_000;
const ID_SZKOLA: u32 = 3_000_000;
const KROK_M: i32 = 150;
/// Bok siatki domów. Dwadzieścia, czyli czterysta mieszkań: przebieg trzydziestoletni
/// mierzy **udział** uczniów, a udział w kohorcie siedmiu osób nie mierzy niczego.
/// Miasto musi mieć zapas mieszkań, bo gospodarstwo bez lokalu wyprowadza się z miasta
/// i populacja kurczy się do liczby domów, a nie do dzietności.
const BOK: u32 = 20;
const ROK: u64 = 360;

/// Miasto: `BOK × BOK` domów co 150 m, po jednej szkole w dwóch rogach.
///
/// Dwie szkoły, nie jedna, bo reguła wyboru („najbliższa, przy równej odległości
/// niższy klucz") nie miałaby czego rozstrzygać przy jednej.
fn swiat(seed: u64, gospodarstw: usize) -> World {
    let mut world = World::new(seed);
    register(&mut world, NeedTable::load_default().expect("data/needs"));
    society::register_society(
        &mut world,
        DemographyTable::load_default().expect("data/demography"),
    );

    let mut homes = Vec::new();
    let mut block_of = vec![0u32; (ID_DOM + BOK * BOK) as usize];
    for y in 0..BOK {
        for x in 0..BOK {
            let i = y * BOK + x;
            homes.push(HomeSlot {
                building: ID_DOM + i,
                unit: 0,
                district: 0,
                value: Money(1_000_000),
            });
            block_of[(ID_DOM + i) as usize] = 0;
        }
    }
    let jobs: Vec<JobSlot> = (0..gospodarstw * 2)
        .map(|i| JobSlot {
            site: 2_000_000 + (i as u32 % 4),
            role: (i % 4) as u16,
            shift: 1,
            work_days: Employment::WEEKDAYS,
            district: 0,
            wage_monthly: Money(250_000),
        })
        .collect();

    *world.resource_mut::<Vacancies>() = Vacancies::new(homes, jobs);
    *world.resource_mut::<CityFacts>() = CityFacts {
        job_prestige: Vec::new(),
        district_score: vec![50],
        block_of,
    };

    let mut wpisy: Vec<PlaceEntry> = Vec::new();
    for y in 0..BOK {
        for x in 0..BOK {
            wpisy.push(PlaceEntry {
                place: budynek(ID_DOM + y * BOK + x),
                kind: PlaceKind::Home,
                at: WorldCoord::new(x as i32 * KROK_M * 100, y as i32 * KROK_M * 100, 0),
            });
        }
    }
    for (i, (x, y)) in [(0u32, 0u32), (BOK - 1, BOK - 1)].into_iter().enumerate() {
        wpisy.push(PlaceEntry {
            place: budynek(ID_SZKOLA + i as u32),
            kind: PlaceKind::Education,
            at: WorldCoord::new(x as i32 * KROK_M * 100, y as i32 * KROK_M * 100, 0),
        });
    }
    *world.resource_mut::<PlaceCatalog>() = PlaceCatalog::new(Arc::new(PlaceTable::build(wpisy)));

    migration::seed_population(&mut world, 0, gospodarstw);
    world
}

fn przebieg(world: &mut World, od: u64, dob: u64) {
    let mut hooks = NoInheritance;
    for d in od..od + dob {
        society::step_day(world, d, &mut hooks);
    }
}

/// Sam cykl życia, bez migracji.
///
/// Testy progu wieku muszą wyjąć migrację z przebiegu, bo osiemnastolatek, który
/// właśnie wyszedł ze szkoły, jest dorosłym bez pracy — a taki wyprowadza się z miasta
/// i znika z ECS, zabierając ze sobą wynik, który test ma sprawdzić. To nie jest
/// obejście usterki: zawór migracyjny jest tu poprawny, tylko mierzymy co innego.
fn przebieg_cyklu(world: &mut World, od: u64, dob: u64) {
    let mut hooks = NoInheritance;
    for d in od..od + dob {
        demography::step_day(world, d, &mut hooks);
    }
}

/// Ustawia mieszkańcowi wiek na `lat` w dobie `day` i zeruje jego zatrudnienie.
fn ustaw_wiek(world: &mut World, e: Entity, lat: i32, day: i64) {
    if let Some(id) = world.get_mut::<Identity>(e) {
        id.birth_day = (day - i64::from(lat) * 360) as i32;
    }
    if let Some(emp) = world.get_mut::<Employment>(e) {
        *emp = Employment::default();
    }
}

/// Mieszkaniec z domem — bez adresu nie ma z czego policzyć najbliższej szkoły.
fn z_domem(world: &World) -> Vec<Entity> {
    world
        .resource::<Population>()
        .citizens()
        .iter()
        .copied()
        .filter(|e| {
            world
                .get::<Residence>(*e)
                .is_some_and(magnat_agents::Residence::has_home)
        })
        .collect()
}

fn emp(world: &World, e: Entity) -> Employment {
    world.get::<Employment>(e).copied().unwrap_or_default()
}

/// `R2-WP1`, kryterium: dziecko wchodzi do szkoły z placówką, a w `school_end`
/// wychodzi bez jednego i bez drugiego.
///
/// Przed naprawą padało na pierwszym asercie: `Employment.site` zostawał `NO_SITE`,
/// bo placówkę rozdawał wyłącznie Etap 8 generatora, raz, przy zaludnianiu świata.
#[test]
fn dziecko_wchodzi_do_szkoly_z_placowka_i_wychodzi_z_niej_bez_niej() {
    let mut world = swiat(7, 24);
    let ages = world.resource::<DemographyTable>().ages();
    let ludzie = z_domem(&world);
    assert!(ludzie.len() >= 2, "świat testowy bez mieszkańców z domem");

    // Jeden tuż przed wiekiem szkolnym, drugi tuż przed jego końcem — i drugi jest
    // uczniem z placówką, bo inaczej asercja o wyjściu ze szkoły byłaby spełniona
    // tożsamościowo przez kogoś, kto do niej nigdy nie chodził.
    //
    // **Dwa lata przebiegu, nie rok.** Hazardy są shardowane 1/360, więc mieszkaniec
    // przechodzi swoją dobę raz w roku — a przekroczenie progu wieku wypada w losowym
    // miejscu tego roku. Rok gwarantuje jedną dobę shardu, ale niekoniecznie **po**
    // urodzinach.
    let maluch = ludzie[0];
    let starszy = ludzie[1];
    ustaw_wiek(&mut world, maluch, i32::from(ages.school_start) - 1, 0);
    ustaw_wiek(&mut world, starszy, i32::from(ages.school_end) - 1, 0);
    if let Some(e) = world.get_mut::<Employment>(starszy) {
        e.flags = Employment::FLAG_PUPIL;
        e.site = ID_SZKOLA;
    }

    przebieg_cyklu(&mut world, 0, 2 * ROK + 1);

    let m = emp(&world, maluch);
    assert!(m.is_pupil(), "siedmiolatek nie został uczniem");
    assert!(
        m.has_job(),
        "uczeń bez placówki: `Employment.site` został pusty, więc nie ma szkoły w planie dnia"
    );
    assert!(!m.is_employed(), "uczeń nie jest zatrudniony");

    let s = emp(&world, starszy);
    assert!(!s.is_pupil(), "osiemnastolatek dalej ma flagę ucznia");
    assert!(
        !s.has_job(),
        "absolwent trzyma klucz szkoły w `site` — dla rynku pracy wygląda na zatrudnionego"
    );
}

/// `R2-WP1`: uczeń z generacji **nie traci** flagi w pierwszym roku gry.
///
/// Przed naprawą tracił ją przy pierwszej swojej dobie shardu: cykl pytał
/// `has_job()`, a uczeń z Etapu 8 ma w `site` szkołę, więc wyglądał na pracującego
/// i wypadał z wieku szkolnego, zachowując przy tym przypisaną placówkę.
#[test]
fn uczen_z_generacji_zostaje_uczniem() {
    let mut world = swiat(11, 24);
    let ages = world.resource::<DemographyTable>().ages();
    let uczen = z_domem(&world)[0];
    ustaw_wiek(&mut world, uczen, i32::from(ages.school_start) + 2, 0);
    // Tak wygląda uczeń po Etapie 8: flaga **i** przypisana szkoła.
    if let Some(e) = world.get_mut::<Employment>(uczen) {
        e.flags = Employment::FLAG_PUPIL;
        e.site = ID_SZKOLA;
        e.role = 3;
    }

    przebieg_cyklu(&mut world, 0, ROK + 1);

    let u = emp(&world, uczen);
    assert!(
        u.is_pupil(),
        "uczeń z generacji stracił flagę w pierwszym roku"
    );
    assert!(u.has_job(), "uczeń z generacji stracił szkołę");
}

/// `R2-WP1`: uczeń stoi poza indeksem miejsc pracy, a jego relacje klasowe są
/// `Acquaintance`, nie `Colleague` (`D-N7`).
///
/// Przed naprawą `SocialIndex::coworkers(szkoła)` zwracało całą klasę, a `M10e` §5.9
/// liczy warunek powstania związku zawodowego właśnie na tym indeksie.
#[test]
fn uczen_nie_jest_wspolpracownikiem() {
    let mut world = swiat(13, 24);
    let ages = world.resource::<DemographyTable>().ages();
    let ludzie = z_domem(&world);
    for e in ludzie.iter().take(6) {
        ustaw_wiek(&mut world, *e, i32::from(ages.school_start) + 3, 0);
        if let Some(x) = world.get_mut::<Employment>(*e) {
            x.flags = Employment::FLAG_PUPIL;
            x.site = ID_SZKOLA;
        }
    }
    let mut idx = SocialIndex::new();
    idx.rebuild(&world);

    assert!(
        idx.coworkers(ID_SZKOLA).is_empty(),
        "szkoła stoi w indeksie miejsc pracy — jej klasa uzwiązkowi się w M10e"
    );
    assert_eq!(
        idx.classmates(ID_SZKOLA).len(),
        6,
        "klasa nie trafiła do indeksu szkolnego"
    );

    // Doba społeczna zawiązuje relacje; klasowe mają być `Acquaintance`.
    *world.resource_mut::<SocialIndex>() = idx;
    przebieg(&mut world, 0, 30);
    let mut klasowych = 0usize;
    for e in ludzie.iter().take(6) {
        let rel = *world.get::<RelationsRef>(*e).expect("uchwyt relacji");
        let wpisy = world
            .resource::<RelationSlab>()
            .entries(demography::relations_ref(&rel))
            .to_vec();
        for r in wpisy {
            assert_ne!(
                r.kind,
                RelationKind::Colleague as u8,
                "uczeń dostał relację `Colleague` w szkole"
            );
            if r.kind == RelationKind::Acquaintance as u8 {
                klasowych += 1;
            }
        }
    }
    assert!(klasowych > 0, "klasa nie zawiązała ani jednej znajomości");
}

/// `R2-WP5`, kryterium: absolwent ma wykształcenie z tabeli, a jego sufit umiejętności
/// jest wyższy niż sufit rówieśnika, który szkoły nie skończył.
///
/// Przed naprawą oba sufity były równe i minimalne: `Vitals::edu_level` ustawiał
/// wyłącznie Etap 8, a noworodek dostawał `Vitals::default()`, czyli zero.
#[test]
fn absolwent_ma_wyksztalcenie_a_ten_bez_szkoly_nie_ma() {
    let mut world = swiat(17, 24);
    let tabela = world.resource::<DemographyTable>().clone();
    let ages = tabela.ages();
    let ludzie = z_domem(&world);
    let absolwent = ludzie[0];
    let bez_szkoly = ludzie[1];

    ustaw_wiek(&mut world, absolwent, i32::from(ages.school_end) - 1, 0);
    if let Some(e) = world.get_mut::<Employment>(absolwent) {
        e.flags = Employment::FLAG_PUPIL;
        e.site = ID_SZKOLA;
    }
    // Rówieśnik, który do szkoły nie chodził, bo już pracuje: ten sam wiek, etat
    // zamiast flagi. Bez etatu cykl posłałby go do szkoły na ten ostatni rok i test
    // porównywałby absolwenta z absolwentem.
    ustaw_wiek(&mut world, bez_szkoly, i32::from(ages.school_end) - 1, 0);
    if let Some(e) = world.get_mut::<Employment>(bez_szkoly) {
        e.flags = 0;
        e.site = 2_000_000;
        e.work_days = Employment::WEEKDAYS;
    }
    for e in [absolwent, bez_szkoly] {
        if let Some(v) = world.get_mut::<Vitals>(e) {
            v.edu_level = 0;
        }
    }

    przebieg_cyklu(&mut world, 0, 2 * ROK + 1);

    let pelny = u16::from(ages.school_end - ages.school_start);
    let a = world.get::<Vitals>(absolwent).copied().unwrap_or_default();
    let b = world.get::<Vitals>(bez_szkoly).copied().unwrap_or_default();
    assert_eq!(
        a.edu_level,
        tabela.education_level(pelny),
        "absolwent nie dostał poziomu z tabeli `education`"
    );
    assert_eq!(b.edu_level, 0, "ktoś bez szkoły dostał wykształcenie");
    assert!(
        a.edu_level > b.edu_level,
        "sufit umiejętności absolwenta i nieucznia jest ten sam"
    );
}

/// `R2-WP5`: tabela wykształcenia jest monotoniczna i osiągalna.
#[test]
fn tabela_wyksztalcenia_rosnie_i_domyka_pelny_cykl() {
    let t = DemographyTable::load_default().expect("data/demography");
    let ages = t.ages();
    let pelny = u16::from(ages.school_end - ages.school_start);
    assert_eq!(
        t.education_level(0),
        0,
        "zero lat szkoły daje wykształcenie"
    );
    let mut ostatni = 0;
    for lat in 0..=pelny {
        let p = t.education_level(lat);
        assert!(p >= ostatni, "poziom spada wraz z latami nauki");
        ostatni = p;
    }
    assert!(ostatni > 0, "pełny cykl szkolny nie daje żadnego poziomu");
}

/// `R2-WP1` i `R2-WP5`: trzydzieści lat gry, szkoła dalej ma uczniów, a oni — dyplomy.
///
/// Dwa kryteria podfazy przełożone na jeden przebieg. **Pierwsze**: udział uczniów
/// w populacji w wieku szkolnym nie spada poniżej 90 % — przed naprawą spadał do zera,
/// bo flagę tracili wszyscy, a nowi jej nie dostawali. **Drugie**: mediana wykształcenia
/// młodych dorosłych urodzonych w tej grze odpowiada pełnemu cyklowi z tabeli — przed
/// naprawą było to zero, bo `edu_level` ustawiał wyłącznie Etap 8 generatora.
#[test]
#[ignore = "trzydzieści lat gry — przebieg minutowy, tylko w --release"]
fn po_trzydziestu_latach_szkola_ma_uczniow() {
    let mut world = swiat(23, 150);
    let ages = world.resource::<DemographyTable>().ages();
    przebieg(&mut world, 0, 30 * ROK);

    let day = 30 * ROK as i64;
    let mut w_wieku = 0u32;
    let mut uczniow = 0u32;
    let ludzie: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    for e in ludzie {
        let Some(id) = world.get::<Identity>(e).copied() else {
            continue;
        };
        let lat = id.age_years(day as i32);
        if lat < i32::from(ages.school_start) || lat >= i32::from(ages.school_end) {
            continue;
        }
        w_wieku += 1;
        if emp(&world, e).is_pupil() {
            uczniow += 1;
        }
    }
    assert!(
        w_wieku > 0,
        "po trzydziestu latach nie ma nikogo w wieku szkolnym"
    );
    assert!(
        uczniow * 100 >= w_wieku * 90,
        "uczniów {uczniow} na {w_wieku} w wieku szkolnym — poniżej 90 %"
    );

    // ── R2-WP5: wykształcenie **dociera do dorosłych** ──────────────────────────
    //
    // Mediana liczy się w kohorcie dwudziestopięciolatków **urodzonych w tej grze**,
    // a nie w całym roczniku. Przyjezdni dostają `Vitals::default()`, czyli zero:
    // rozkład wykształcenia napływu nadaje w prawdziwym mieście Etap 8 generatora,
    // a ten test stawia miasto syntetyczne, w którym nikt go nie nadaje. Mierzenie
    // ich razem mierzyłoby brak tamtej tabeli, a nie cykl szkolny.
    //
    // Porównania z medianą Etapu 8 **tutaj nie ma i być nie może** z tego samego
    // powodu — tamto porównanie jest przebiegiem akceptacyjnym R2 (§7 pkt 4).
    let mut poziomy: Vec<u8> = Vec::new();
    for e in world.resource::<Population>().citizens().to_vec() {
        let Some(id) = world.get::<Identity>(e).copied() else {
            continue;
        };
        // Kohorta `[school_end, leave_nest)`, a nie sam rocznik dwudziestopięciolatków
        // z planu: w tym mieście dwudziestopięciolatek **wyprowadza się z gniazda**,
        // a przy braku wolnego mieszkania wyjeżdża z miasta — rocznik opróżnia się
        // sam i mierzyłby zawór migracyjny, a nie szkołę.
        let lat = id.age_years(day as i32);
        if lat < i32::from(ages.school_end)
            || lat >= i32::from(ages.leave_nest)
            || id.birth_district == Identity::DISTRICT_IMMIGRANT
        {
            continue;
        }
        poziomy.push(world.get::<Vitals>(e).map_or(0, |v| v.edu_level));
    }
    poziomy.sort_unstable();
    let mediana = poziomy.get(poziomy.len() / 2).copied().unwrap_or(0);
    let tabela = DemographyTable::load_default().expect("data/demography");
    let pelny = u16::from(ages.school_end - ages.school_start);
    assert!(
        !poziomy.is_empty(),
        "po trzydziestu latach nie ma ani jednego dorosłego urodzonego w tej grze"
    );
    assert_eq!(
        mediana,
        tabela.education_level(pelny),
        "mediana wykształcenia młodych dorosłych nie pochodzi z cyklu szkolnego"
    );
    let _ = demography::DEMOGRAPHY_SHARDS;
}
