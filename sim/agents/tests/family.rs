//! Graf rodziny: rodzeństwo, dziadkowie, ochrona wpisu (`R2-WP2`).
//!
//! Każdy test w tym pliku padał przed naprawą — to jest warunek z `R2` §3 pkt 1.
//! Wszystkie trzy usterki mają jedną przyczynę: relacje rodzinne wyprowadzały się
//! w dwóch miejscach po dwóch różnych regułach, a slab nie wiedział, że rodzina
//! jest czymś innym niż znajomość z przystanku.

use magnat_agents::{
    demography, household, migration, register, society, CityFacts, DemographyTable, HomeSlot,
    Household, HouseholdOverflow, Identity, JobSlot, LifeQueue, LifeTask, LifeTaskKind, Lifecycle,
    NeedTable, NoInheritance, Population, RelationKind, RelationSlab, RelationsRef, Vacancies,
};
use magnat_core::{Entity, Money};
use magnat_ecs::World;

const ID_DOM: u32 = 1_000_000;
const BOK: u32 = 8;

fn swiat(seed: u64) -> World {
    let mut world = World::new(seed);
    register(&mut world, NeedTable::load_default().expect("data/needs"));
    society::register_society(
        &mut world,
        DemographyTable::load_default().expect("data/demography"),
    );
    let mut homes = Vec::new();
    let mut block_of = vec![0u32; (ID_DOM + BOK * BOK) as usize];
    for i in 0..BOK * BOK {
        homes.push(HomeSlot {
            building: ID_DOM + i,
            unit: 0,
            district: 0,
            value: Money(1_000_000),
        });
        block_of[(ID_DOM + i) as usize] = 0;
    }
    let jobs: Vec<JobSlot> = (0..16)
        .map(|i| JobSlot {
            site: 2_000_000 + i % 4,
            role: (i % 4) as u16,
            shift: 1,
            work_days: 0b001_1111,
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
    world
}

/// Mieszkaniec o zadanym wieku, wpisany do spisu. Tyle komponentów, ile czyta
/// reguła wiązania — wiek i uchwyt relacji; reszta nie ma tu nic do powiedzenia.
fn czlowiek(world: &mut World, wiek: i32, gospodarstwo: u32) -> Entity {
    let e = world
        .spawn()
        .with(Identity {
            birth_day: -wiek * 360,
            flags: Identity::FLAG_ALIVE,
            household: gospodarstwo,
            ..Identity::default()
        })
        .with(RelationsRef::default())
        .id();
    world.resource_mut::<Population>().add_citizen(e);
    e
}

/// Rodzaj relacji, jaką `kto` ma zapisaną wobec `z_kim`.
fn relacja(world: &World, kto: Entity, z_kim: Entity) -> Option<RelationKind> {
    let rel = world.get::<RelationsRef>(kto).copied()?;
    world
        .resource::<RelationSlab>()
        .entries(demography::relations_ref(&rel))
        .iter()
        .find(|r| r.other == z_kim.index())
        .map(|r| RelationKind::from_u8(r.kind))
}

/// Pozycja 8 wykazu: dwoje dzieci w gospodarstwie z zasiedlenia albo z napływu
/// to dla silnika dwoje obcych ludzi pod jednym dachem.
///
/// Przed naprawą padało na pierwszym asercie: `spawn_household_aged` wiązało
/// wyłącznie dorosłych z dziećmi (`Child` w podwójnej pętli) i na tym kończyło,
/// a `RelationKind::Sibling` powstawał w całym repozytorium tylko przy porodzie.
#[test]
fn rodzenstwo_z_zasiedlenia_ma_relacje() {
    let mut world = swiat(5);
    migration::seed_population(&mut world, 0, 24);

    // Gospodarstwo z dwojgiem dzieci — szukamy go w zasiedleniu, bo rozmiary
    // gospodarstw losuje tabela migracji i nie każdy świat ma takie na pierwszym
    // miejscu.
    let ages = world.resource::<DemographyTable>().ages();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    let mut dzieci_wg_domu: std::collections::BTreeMap<u32, Vec<Entity>> = Default::default();
    for c in spis {
        let Some(id) = world.get::<Identity>(c).copied() else {
            continue;
        };
        if id.age_years(0) < i32::from(ages.adult) {
            dzieci_wg_domu.entry(id.household).or_default().push(c);
        }
    }
    let (_, para) = dzieci_wg_domu
        .iter()
        .find(|(_, v)| v.len() >= 2)
        .expect("świat testowy bez gospodarstwa z dwojgiem dzieci");

    assert_eq!(
        relacja(&world, para[0], para[1]),
        Some(RelationKind::Sibling),
        "rodzeństwo z zasiedlenia nie zna się nawzajem"
    );
    assert_eq!(
        relacja(&world, para[1], para[0]),
        Some(RelationKind::Sibling),
        "relacja rodzeństwa nie jest obustronna"
    );
}

/// Pozycja 9 wykazu: w gospodarstwie wielopokoleniowym babcia dostawała z wnukiem
/// relację **rodzeństwa**, bo poród wiązał `Sibling` z każdym domownikiem poza matką
/// i jej partnerem.
///
/// Przed naprawą zwracało `Some(Sibling)` — wariantu `Grandparent` nie było w enumie.
#[test]
fn babcia_ma_z_wnukiem_relacje_dziadka_a_nie_rodzenstwa() {
    let mut world = swiat(9);
    let hh = 42;
    let babcia = czlowiek(&mut world, 68, hh);
    let matka = czlowiek(&mut world, 38, hh);
    let wnuk = czlowiek(&mut world, 8, hh);

    demography::zwiaz_rodzine(&mut world, &[babcia, matka, wnuk], 0);

    assert_eq!(
        relacja(&world, babcia, wnuk),
        Some(RelationKind::Grandparent),
        "babcia widzi wnuka jako kogoś innego niż wnuka"
    );
    assert_eq!(
        relacja(&world, wnuk, babcia),
        Some(RelationKind::Grandparent),
        "relacja dziadkowska nie jest obustronna"
    );
    // Pokolenie w środku zostaje rodzicem — gdyby reguła brała samą różnicę wieku,
    // matka czterdziestoletnia z noworodkiem byłaby babcią własnego dziecka.
    assert_eq!(
        relacja(&world, matka, wnuk),
        Some(RelationKind::Child),
        "matka nie jest rodzicem swojego dziecka"
    );
    assert_eq!(
        relacja(&world, babcia, matka),
        Some(RelationKind::Child),
        "babcia nie jest rodzicem matki"
    );
}

/// Rodzeństwo wyprowadza się ze wspólnego rodzica, a nie z samej różnicy wieku —
/// inaczej czworo współlokatorów w akademiku byłoby rodziną.
#[test]
fn wspollokatorzy_bez_wspolnego_rodzica_nie_sa_rodzenstwem() {
    let mut world = swiat(3);
    let hh = 7;
    let a = czlowiek(&mut world, 24, hh);
    let b = czlowiek(&mut world, 26, hh);
    let c = czlowiek(&mut world, 23, hh);

    demography::zwiaz_rodzine(&mut world, &[a, b, c], 0);

    assert_eq!(relacja(&world, a, b), None, "współlokator stał się bratem");
    assert_eq!(relacja(&world, b, c), None, "współlokator stał się bratem");
}

/// Pozycja 24 wykazu: waga relacji rodzinnej zanika do jedynki, a ofiarą wypychania
/// jest wpis o najniższej wadze — więc nieodwiedzany ojciec wypadał ze slabu przy
/// trzydziestej trzeciej znajomości.
///
/// Przed naprawą ojca w slabie już nie było.
#[test]
fn relacja_rodzinna_nie_wypada_przy_przepelnieniu() {
    let mut world = swiat(13);
    let hh = 1;
    let ojciec = czlowiek(&mut world, 40, hh);
    let dziecko = czlowiek(&mut world, 10, hh);
    demography::zwiaz_rodzine(&mut world, &[ojciec, dziecko], 0);

    // Czterdziestu znajomych o wadze wyższej niż podłoga rodzinna: gdyby ofiara
    // szła po samej wadze, ojciec (podłogowany) byłby pierwszy w kolejce.
    for i in 0..40 {
        let znajomy = czlowiek(&mut world, 30, 2);
        demography::powiaz(
            &mut world,
            dziecko,
            znajomy,
            RelationKind::Acquaintance,
            50 + (i % 7) as u8,
            0,
        );
    }

    assert_eq!(
        relacja(&world, dziecko, ojciec),
        Some(RelationKind::Parent),
        "ojciec wypadł ze slabu, wypchnięty przez znajomych"
    );
}

// ── R2-WP3: gospodarstwo bez cichego przepełnienia ──────────────────────────────

/// Pozycja 10 wykazu: poród do gospodarstwa o pełnym składzie kończył się
/// **milczeniem**. `uroda` ignorowała `false` z `add_member`, więc dziecko miało
/// `Identity.household` wskazujące na dom rodziców i nie było w ich składzie:
/// nie liczyło się do rozmiaru, nie widziała go klasyfikacja ani role.
///
/// Przed naprawą padał pierwszy assert — suma składów wszystkich gospodarstw była
/// o jeden mniejsza niż liczba żywych mieszkańców.
#[test]
fn porod_do_pelnego_gospodarstwa_nie_gubi_dziecka() {
    let mut world = swiat(21);
    migration::seed_population(&mut world, 0, 40);

    // Gospodarstwo z dorosłą kobietą, dopchane do `HH_MAX_MEMBERS` mieszkańcami
    // z innych domów. Pełnych gospodarstw generator sam nie robi (`arrival_sizes`
    // kończy się na czwórce), a usterka odsłania się dopiero przy pełnym składzie.
    let ages = world.resource::<DemographyTable>().ages();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    let matka = *spis
        .iter()
        .find(|c| {
            world.get::<Identity>(**c).is_some_and(|i| {
                !i.is_male() && i.age_years(0) >= i32::from(ages.adult) && i.age_years(0) < 45
            })
        })
        .expect("świat testowy bez dorosłej kobiety");
    let hh_idx = world.get::<Identity>(matka).expect("matka").household;
    let hh_e = demography::household_by_index(&world, hh_idx).expect("gospodarstwo matki");

    let mut hh = world.get::<Household>(hh_e).copied().expect("skład");
    for c in spis.iter().copied() {
        if hh.size as usize >= magnat_agents::HH_MAX_MEMBERS {
            break;
        }
        if world.get::<Identity>(c).is_some_and(|i| i.household == hh_idx) {
            continue;
        }
        let stary = world.get::<Identity>(c).expect("tożsamość").household;
        if let Some(stare) = demography::household_by_index(&world, stary) {
            let mut s = world.get::<Household>(stare).copied().expect("skład");
            let ov = world.resource_mut::<HouseholdOverflow>();
            household::remove_member(stary, &mut s, ov, c.index());
            if let Some(slot) = world.get_mut::<Household>(stare) {
                *slot = s;
            }
        }
        let ov = world.resource_mut::<HouseholdOverflow>();
        assert!(household::add_member(hh_idx, &mut hh, ov, c.index()));
        if let Some(id) = world.get_mut::<Identity>(c) {
            id.household = hh_idx;
        }
    }
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh;
    }
    assert_eq!(
        hh.size as usize,
        magnat_agents::HH_MAX_MEMBERS,
        "nie udało się zapełnić gospodarstwa"
    );

    // Poród następnej doby.
    if let Some(l) = world.get_mut::<Lifecycle>(matka) {
        l.flags |= Lifecycle::FLAG_PREGNANT;
    }
    world.resource_mut::<LifeQueue>().schedule(
        1,
        LifeTask {
            kind: LifeTaskKind::Birth,
            actor: matka.index(),
        },
    );
    let przed = world.resource::<Population>().len();
    let mut hooks = NoInheritance;
    demography::step_day(&mut world, 1, &mut hooks);
    assert!(
        world.resource::<Population>().len() > przed,
        "poród się nie odbył — test mierzyłby własny brak"
    );

    // Każdy żywy mieszkaniec jest w składzie **jakiegoś** gospodarstwa.
    let mut w_skladach = 0usize;
    for hh_e in world.resource::<Population>().households().to_vec() {
        let Some(h) = world.get::<Household>(hh_e).copied() else {
            continue;
        };
        w_skladach +=
            household::members_of(hh_e.index(), &h, world.resource::<HouseholdOverflow>()).len();
    }
    assert_eq!(
        w_skladach,
        world.resource::<Population>().len(),
        "mieszkaniec istnieje w spisie i nie istnieje w żadnym składzie"
    );

    // …i mieszka tam, gdzie mówi jego karta.
    for c in world.resource::<Population>().citizens().to_vec() {
        let hh_idx = world.get::<Identity>(c).expect("tożsamość").household;
        let hh_e = demography::household_by_index(&world, hh_idx).expect("gospodarstwo");
        let h = world.get::<Household>(hh_e).copied().expect("skład");
        assert!(
            household::members_of(hh_idx, &h, world.resource::<HouseholdOverflow>())
                .contains(&c.index()),
            "karta mieszkańca wskazuje gospodarstwo, które go nie zna"
        );
    }
}

/// Piąte dziecko ponad `MAX_ESCORTED` i dziecko w domu bez dorosłego przestają
/// znikać bez śladu: `HouseholdRoles` niesie ich liczbę, a planer zamienia ją
/// na powód decyzji.
///
/// Przed naprawą `roles` kończyło zbieranie `break`-iem, a brak dorosłego —
/// `clear()`-em, i pole `unescorted` nie istniało.
#[test]
fn piate_dziecko_zostawia_slad() {
    let uczen = |i: u32| magnat_agents::MemberView {
        citizen: i,
        age_years: 8,
        is_pupil: true,
        works: false,
        shift: magnat_agents::ShiftKind::Day,
        site: 500,
        partnered_inside: false,
    };
    let dorosly = |i: u32| magnat_agents::MemberView {
        citizen: i,
        age_years: 35,
        is_pupil: false,
        works: true,
        shift: magnat_agents::ShiftKind::Day,
        site: 700,
        partnered_inside: false,
    };

    let mut sklad: Vec<magnat_agents::MemberView> = (0..5).map(uczen).collect();
    sklad.push(dorosly(10));
    let role = household::roles(&sklad, 10, 18, 0, None);
    assert_eq!(
        role.escorted.len(),
        household::MAX_ESCORTED,
        "limit odprowadzania przestał obowiązywać"
    );
    assert_eq!(role.unescorted, 1, "piąte dziecko zniknęło bez śladu");

    // Dom bez dorosłego: nikt nie odprowadza **żadnego** dziecka i to też jest stan.
    let same_dzieci: Vec<magnat_agents::MemberView> = (0..2).map(uczen).collect();
    let role = household::roles(&same_dzieci, 10, 18, 0, None);
    assert!(role.escorted.is_empty());
    assert_eq!(role.unescorted, 2, "dzieci bez dorosłego zniknęły bez śladu");
}

// ── R2-WP4: opiekun prawny i gospodarstwo osierocone ────────────────────────────

/// Pozycja 20 wykazu: gdy umierał ostatni dorosły, a w gospodarstwie zostawały
/// dzieci, **nie działo się nic**. Gospodarstwo trwało, klasyfikacja dawała
/// `FamilyWithKids`, a lista odprowadzanych była czyszczona, bo dorosłych było zero.
///
/// Przed naprawą `Household.guardian` nie istniało, a `escorts` opiekuna były puste.
#[test]
fn smierc_ostatniego_doroslego_daje_dzieciom_opiekuna() {
    let mut world = swiat(29);
    let mut r = magnat_core::rng(world.seed, magnat_core::StreamId::Migration, 0, magnat_core::Tick(0));

    let dom = |i: u32| HomeSlot {
        building: ID_DOM + i,
        unit: 0,
        district: 0,
        value: Money(1_000_000),
    };
    // Rodzic z dwojgiem dzieci i ciotka mieszkająca osobno.
    let rodzina = migration::spawn_household_aged(&mut world, 0, &mut r, 1, 2, dom(0), 40, 1);
    let sasiedzi = migration::spawn_household_aged(&mut world, 0, &mut r, 1, 0, dom(1), 35, 1);

    let ages = world.resource::<DemographyTable>().ages();
    let sklad = |world: &magnat_ecs::World, hh: Entity| -> Vec<Entity> {
        let h = world.get::<Household>(hh).copied().expect("skład");
        household::members_of(hh.index(), &h, world.resource::<HouseholdOverflow>())
            .iter()
            .filter_map(|m| demography::citizen_by_index(world, *m))
            .collect()
    };
    let czlonkowie = sklad(&world, rodzina);
    let ciotka = sklad(&world, sasiedzi)[0];
    let rodzic = *czlonkowie
        .iter()
        .find(|c| {
            world
                .get::<Identity>(**c)
                .is_some_and(|i| i.age_years(0) >= i32::from(ages.adult))
        })
        .expect("rodzic");
    let dzieci: Vec<Entity> = czlonkowie.iter().copied().filter(|c| *c != rodzic).collect();
    assert_eq!(dzieci.len(), 2);

    // Dzieci są uczniami w wieku wymagającym odprowadzenia, ciotka zna je z podwórka.
    for d in &dzieci {
        if let Some(id) = world.get_mut::<Identity>(*d) {
            id.birth_day = -8 * 360;
        }
        if let Some(e) = world.get_mut::<magnat_agents::Employment>(*d) {
            e.flags = magnat_agents::Employment::FLAG_PUPIL;
            e.site = 3_000_000;
            e.work_days = magnat_agents::Employment::WEEKDAYS;
        }
        demography::powiaz(&mut world, ciotka, *d, RelationKind::Acquaintance, 60, 0);
    }

    // Rodzic przekracza `ages.max`, więc umiera na pierwszej swojej dobie shardu.
    if let Some(id) = world.get_mut::<Identity>(rodzic) {
        id.birth_day = -(i32::from(ages.max) + 5) * 360;
    }
    let mut hooks = NoInheritance;
    for d in 0..=360 {
        society::step_day(&mut world, d, &mut hooks);
        if world.get::<Identity>(rodzic).is_none_or(|i| !i.is_alive()) {
            society::step_day(&mut world, d + 1, &mut hooks);
            break;
        }
    }
    assert!(
        world.get::<Identity>(rodzic).is_none_or(|i| !i.is_alive()),
        "rodzic nie umarł — test mierzyłby własny brak"
    );

    let h = world.get::<Household>(rodzina).copied().expect("skład");
    assert_eq!(
        h.guardian_of(),
        Some(ciotka.index()),
        "osierocone gospodarstwo nie dostało opiekuna"
    );

    // …a obowiązek wchodzi do planu dnia opiekunki, choć mieszka gdzie indziej.
    let migawka = magnat_agents::CitizenSnapshot::of(&world, ciotka, 361).expect("migawka");
    assert!(
        !migawka.escorts.is_empty(),
        "opiekunka nie ma w planie dnia odprowadzenia podopiecznych"
    );
}

/// Test własnościowy `R2-WP4`: w przebiegu wieloletnim **żadne** gospodarstwo
/// z dzieckiem nie ma jednocześnie zera dorosłych i pustego opiekuna.
///
/// Skład świata jest **postawiony pod ten niezmiennik**, i to jest konieczne, a nie
/// wygodne. Zasiedlenie rozdaje rozmiary z `migration.arrival_sizes` i samotny rodzic
/// z dziećmi nie wychodzi z niego ani razu, a przy naturalnej śmiertelności trzydziestolatka
/// osierocenie nie zdarza się przez pięć lat w ogóle — niezmiennik byłby wtedy spełniony
/// tożsamościowo, czyli mierzyłby własny brak (`R2` §7 pkt 1). Świat dostaje więc
/// dwadzieścia gospodarstw samotnego rodzica z dwojgiem dzieci, rodzicom ustawia się
/// wiek tuż pod `ages.max`, a obok stoi dwadzieścia gospodarstw sąsiedzkich. Asercja
/// na końcu pilnuje, że osierocenie faktycznie zaszło.
#[test]
fn prop_dziecko_nigdy_bez_doroslego_i_bez_opiekuna() {
    let mut world = swiat(31);
    let mut r = magnat_core::rng(
        world.seed,
        magnat_core::StreamId::Migration,
        0,
        magnat_core::Tick(0),
    );
    let ages = world.resource::<DemographyTable>().ages();
    let adult = i32::from(ages.adult);
    let dom = |i: u32| HomeSlot {
        building: ID_DOM + i,
        unit: 0,
        district: 0,
        value: Money(1_000_000),
    };

    let mut rodziny: Vec<Entity> = Vec::new();
    for i in 0..20 {
        rodziny.push(migration::spawn_household_aged(
            &mut world,
            0,
            &mut r,
            1,
            2,
            dom(i),
            30,
            8,
        ));
        migration::spawn_household_aged(&mut world, 0, &mut r, 2, 0, dom(20 + i), 30, 8);
    }

    // Rodzice tuż pod granicą wieku — umrą w ciągu roku, każdy w swojej dobie shardu.
    for hh in &rodziny {
        let h = world.get::<Household>(*hh).copied().expect("skład");
        let sklad = household::members_of(hh.index(), &h, world.resource::<HouseholdOverflow>());
        for m in sklad.iter() {
            let Some(c) = demography::citizen_by_index(&world, *m) else {
                continue;
            };
            let dorosly = world
                .get::<Identity>(c)
                .is_some_and(|i| i.age_years(0) >= adult);
            if dorosly {
                if let Some(id) = world.get_mut::<Identity>(c) {
                    id.birth_day = -(i32::from(ages.max) - 1) * 360;
                }
            }
        }
    }

    let mut osierocenia = 0usize;
    let mut hooks = NoInheritance;

    for d in 0..1800 {
        society::step_day(&mut world, d, &mut hooks);
        if d % 30 != 0 {
            continue;
        }
        for hh in world.resource::<Population>().households().to_vec() {
            let Some(h) = world.get::<Household>(hh).copied() else {
                continue;
            };
            if !h.is_active() {
                continue;
            }
            let sklad =
                household::members_of(hh.index(), &h, world.resource::<HouseholdOverflow>());
            let mut dzieci = 0usize;
            let mut dorosli = 0usize;
            for m in sklad.iter() {
                let Some(c) = demography::citizen_by_index(&world, *m) else {
                    continue;
                };
                let wiek = world.get::<Identity>(c).map_or(0, |i| i.age_years(d as i32));
                if wiek < adult {
                    dzieci += 1;
                } else {
                    dorosli += 1;
                }
            }
            if dzieci > 0 && dorosli == 0 {
                osierocenia += 1;
            }
            assert!(
                dzieci == 0 || dorosli > 0 || h.guardian_of().is_some(),
                "doba {d}: gospodarstwo {} ma {dzieci} dzieci, zero dorosłych i pustego opiekuna",
                hh.index()
            );
        }
    }
    assert!(
        osierocenia > 0,
        "przez pięć lat nie osierociało ani jedno gospodarstwo — niezmiennik          byłby spełniony tożsamościowo"
    );
}
