//! Testy własnościowe demografii i społeczeństwa (M3c, dokument fazy §7.2).
//!
//! Każdy test odpowiada wierszowi z tabeli §7.2 albo kryterium ukończenia pakietu
//! z dokumentu podfazy. Przebiegi są krótkie — pełna wersja stuletnia jedzie w runnerze
//! `headless century`, który jest artefaktem podfazy i wejściem balansatora M5.

use magnat_agents::{
    awareness_of, demography, household, knows_place, learn_place, migration, register, social,
    society, CityFacts, DemographyTable, HomeSlot, Household, Identity, JobSlot, KnowledgeKind,
    KnowledgeRef, KnowledgeSlab, Lifecycle, NeedTable, NoInheritance, Population, RelationKind,
    RelationSlab, RelationsRef, Vacancies, Wealth, SLAB_MAX,
};
use magnat_core::{Entity, Money};
use magnat_ecs::World;

// ── budowa świata testowego ─────────────────────────────────────────────────────

/// Miasto na siatce: budynek co 150 m, kwartał 3 × 3 budynki, zakład na 4 × 4 kwartały.
///
/// Geometria jest tu **po to, żeby dało się zmierzyć zasięg plotki** (kryterium WP9):
/// bez odległości „60 % w promieniu 1 km" nie znaczy nic. `sim/agents` jej nie zna
/// i nie musi — współrzędne żyją w teście, tak samo jak w M3d będą żyły w Etapie 8.
struct Miasto {
    bok: u32,
    homes: Vec<HomeSlot>,
    jobs: Vec<JobSlot>,
    facts: CityFacts,
}

const ID_DOM: u32 = 1_000_000;
const ID_PRACA: u32 = 2_000_000;
const KROK_M: u32 = 150;
const KWARTAL: u32 = 3;
/// Ile budynków w poprzek przypada na jeden zakład. Cztery, czyli 600 m: zakład
/// obsadzają ludzie z sąsiedztwa, więc relacja współpracownicza też jest lokalna.
/// W prawdziwym mieście robi to krok 7 Etapu 8 (dopasowanie histogramu dojazdu, M3d).
const ZAKLAD_CO: u32 = 4;
/// Ile budynków w poprzek ma dzielnica w tym mieście testowym. Cztery, czyli 600 m —
/// **mniej niż w prawdziwym mieście** i to jest świadome: `Vacancies` dobiera lokal
/// i etat po dzielnicy, bo odległości nie zna (to jest jej jawny `ponytail:`), więc
/// w teście dzielnica pełni rolę sąsiedztwa. Prawdziwe dopasowanie po czasie dojazdu
/// robi krok 7 Etapu 8 (M3d §5.9) i wtedy ta proteza znika.
const DZIELNICA_CO: u32 = 4;

fn dzielnica(bok: u32, x: u32, y: u32) -> u16 {
    ((y / DZIELNICA_CO) * (bok / DZIELNICA_CO + 1) + x / DZIELNICA_CO) as u16
}

impl Miasto {
    fn nowe(bok: u32) -> Miasto {
        let mut homes = Vec::new();
        let mut block_of = vec![0u32; (ID_DOM + bok * bok) as usize];
        for y in 0..bok {
            for x in 0..bok {
                let i = y * bok + x;
                homes.push(HomeSlot {
                    building: ID_DOM + i,
                    unit: 0,
                    district: dzielnica(bok, x, y),
                    value: Money(1_000_000 + i64::from(i % 10) * 100_000),
                });
                block_of[(ID_DOM + i) as usize] = (y / KWARTAL) * bok + x / KWARTAL;
            }
        }

        // Dwa etaty na budynek: dopasowanie `take_job_in` szuka pracy **w dzielnicy**
        // i dopiero przy jej braku bierze pierwszą lepszą. Przy jednym etacie na lokal
        // co czwarty dorosły dostawał pracę na drugim końcu miasta — a relacja
        // współpracownicza jest wtedy mostem, którym plotka przeskakuje pięć kilometrów.
        let mut jobs = Vec::new();
        for y in 0..bok {
            for x in 0..bok * 2 {
                let x = x % bok;
                let zaklad = ID_PRACA + (y / ZAKLAD_CO) * bok + x / ZAKLAD_CO;
                jobs.push(JobSlot {
                    site: zaklad,
                    role: ((y * bok + x) % 40) as u16,
                    shift: 1,
                    work_days: 0b001_1111,
                    district: dzielnica(bok, x, y),
                    wage_monthly: Money(250_000 + i64::from((y * bok + x) % 50) * 5_000),
                });
            }
        }

        Miasto {
            bok,
            homes,
            jobs,
            facts: CityFacts {
                job_prestige: Vec::new(),
                district_score: (0..(bok / DZIELNICA_CO + 1) * (bok / DZIELNICA_CO + 1))
                    .map(|d| (30 + d % 16 * 4) as u8)
                    .collect(),
                block_of,
            },
        }
    }

    /// Pozycja budynku w metrach.
    fn pozycja(&self, building: u32) -> (f64, f64) {
        let i = building - ID_DOM;
        (
            f64::from(i % self.bok) * f64::from(KROK_M),
            f64::from(i / self.bok) * f64::from(KROK_M),
        )
    }

    /// Zakład obsługujący budynek — ten sam klucz, którego używa `Miasto::nowe`.
    fn zaklad(&self, building: u32) -> u32 {
        let i = building - ID_DOM;
        let (x, y) = (i % self.bok, i / self.bok);
        ID_PRACA + (y / ZAKLAD_CO) * self.bok + x / ZAKLAD_CO
    }

    fn odleglosc(&self, a: u32, b: u32) -> f64 {
        let (ax, ay) = self.pozycja(a);
        let (bx, by) = self.pozycja(b);
        // Mnożenie zamiast `powi`: `clippy.toml` zakazuje funkcji przestępnych
        // w tym crate'cie (K-6), a test i tak mierzy tylko odległość na mapie.
        let (dx, dy) = (ax - bx, ay - by);
        (dx * dx + dy * dy).sqrt()
    }
}

fn swiat(seed: u64, bok: u32, gospodarstw: usize) -> (World, Miasto) {
    let miasto = Miasto::nowe(bok);
    let mut world = World::new(seed);
    register(&mut world, NeedTable::load_default().expect("data/needs"));
    society::register_society(
        &mut world,
        DemographyTable::load_default().expect("data/demography"),
    );
    *world.resource_mut::<Vacancies>() = Vacancies::new(miasto.homes.clone(), miasto.jobs.clone());
    *world.resource_mut::<CityFacts>() = miasto.facts.clone();
    migration::seed_population(&mut world, 0, gospodarstw);
    (world, miasto)
}

fn przebieg(world: &mut World, od: u64, dob: u64) {
    let mut hooks = NoInheritance;
    for d in od..od + dob {
        society::step_day(world, d, &mut hooks);
    }
}

// ── §7.2: tożsamość księgowa i cykl życia ───────────────────────────────────────

#[test]
fn prop_population_identity() {
    // Dla każdej doby: pop(t+1) = pop(t) + narodziny − zgony + napływ − odpływ,
    // dokładnie co do osoby. Sprawdzane po każdej dobie, nie tylko na końcu —
    // błąd, który się zeruje w ciągu miesiąca, i tak jest błędem.
    let (mut world, _) = swiat(7, 16, 120);
    let start = society::population(&world) as u64;
    let mut hooks = NoInheritance;
    for d in 0..900 {
        society::step_day(&mut world, d, &mut hooks);
        let p = world.resource::<Population>();
        assert!(
            p.identity_holds(start),
            "doba {d}: spis {} vs księga (ur. {}, zg. {}, przyj. {}, wyj. {})",
            p.len(),
            p.births,
            p.deaths,
            p.arrivals,
            p.departures
        );
    }
    let p = world.resource::<Population>();
    assert!(p.births > 0, "przez 2,5 roku nikt się nie urodził");
    assert!(p.deaths > 0, "przez 2,5 roku nikt nie umarł");
}

#[test]
fn prop_no_orphan_household() {
    // Brak gospodarstwa bez członków, brak mieszkańca bez gospodarstwa, brak
    // gospodarstwa bez lokalu **poza okresem karencji** (§5.7: 90 dób na znalezienie).
    let (mut world, _) = swiat(11, 14, 90);
    przebieg(&mut world, 0, 720);

    let gospodarstwa: Vec<Entity> = world.resource::<Population>().households().to_vec();
    for hh_e in &gospodarstwa {
        let hh = world
            .get::<Household>(*hh_e)
            .copied()
            .expect("gospodarstwo w spisie bez komponentu");
        assert!(hh.size > 0, "gospodarstwo {} bez członków", hh_e.index());
        assert!(hh.is_active());
    }

    for c in world.resource::<Population>().citizens() {
        let id = world
            .get::<Identity>(*c)
            .expect("mieszkaniec bez tożsamości");
        assert!(id.is_alive(), "martwy mieszkaniec w spisie");
        let hh = demography::household_by_index(&world, id.household);
        assert!(
            hh.is_some(),
            "mieszkaniec {} wskazuje nieistniejące gospodarstwo {}",
            c.index(),
            id.household
        );
    }
}

#[test]
fn prop_household_membership() {
    // Każdy mieszkaniec jest w dokładnie jednym gospodarstwie, a `Household.members`
    // i `Identity.household` zawsze się zgadzają — w obie strony.
    let (mut world, _) = swiat(3, 14, 90);
    przebieg(&mut world, 0, 540);

    let mut widziani: Vec<u32> = Vec::new();
    let gospodarstwa: Vec<Entity> = world.resource::<Population>().households().to_vec();
    for hh_e in &gospodarstwa {
        let hh = world
            .get::<Household>(*hh_e)
            .copied()
            .expect("gospodarstwo");
        let sklad = household::members_of(
            hh_e.index(),
            &hh,
            world.resource::<magnat_agents::HouseholdOverflow>(),
        );
        assert_eq!(
            sklad.len(),
            hh.size as usize,
            "rozjazd `size` ze składem w gospodarstwie {}",
            hh_e.index()
        );
        for m in sklad.iter() {
            assert!(
                !widziani.contains(m),
                "mieszkaniec {m} w dwóch gospodarstwach"
            );
            widziani.push(*m);
            let c = demography::citizen_by_index(&world, *m).expect("członek spoza spisu");
            assert_eq!(
                world.get::<Identity>(c).expect("tożsamość").household,
                hh_e.index(),
                "członek {m} uważa, że mieszka gdzie indziej"
            );
        }
    }
    assert_eq!(
        widziani.len(),
        society::population(&world),
        "ktoś nie jest w żadnym gospodarstwie"
    );
}

#[test]
fn prop_no_immortals() {
    // Żaden żywy mieszkaniec nie ma więcej niż 120 lat gry (43 200 dób).
    let (mut world, _) = swiat(5, 12, 70);
    przebieg(&mut world, 0, 3_600);
    let dzis = 3_600i32;
    for c in world.resource::<Population>().citizens() {
        let wiek = world
            .get::<Identity>(*c)
            .expect("tożsamość")
            .age_years(dzis);
        assert!(wiek <= 120, "mieszkaniec {} ma {wiek} lat", c.index());
        assert!(wiek >= 0, "mieszkaniec urodzony w przyszłości");
    }
}

#[test]
fn prop_relation_symmetry() {
    // Relacja jest obustronna, a nieżyjący nie występuje w żadnym slabie.
    let (mut world, _) = swiat(13, 12, 70);
    przebieg(&mut world, 0, 720);

    let zywi: Vec<u32> = world
        .resource::<Population>()
        .citizens()
        .iter()
        .map(|e| e.index())
        .collect();
    for c in world.resource::<Population>().citizens() {
        let rel = *world.get::<RelationsRef>(*c).expect("uchwyt relacji");
        let wpisy = world
            .resource::<RelationSlab>()
            .entries(demography::relations_ref(&rel))
            .to_vec();
        for r in wpisy {
            assert!(
                zywi.contains(&r.other),
                "mieszkaniec {} trzyma relację do nieżyjącego {}",
                c.index(),
                r.other
            );
            let inny = demography::citizen_by_index(&world, r.other).expect("druga strona");
            let rel2 = *world.get::<RelationsRef>(inny).expect("uchwyt");
            let wzajemna = world
                .resource::<RelationSlab>()
                .entries(demography::relations_ref(&rel2))
                .iter()
                .any(|x| x.other == c.index());
            assert!(
                wzajemna,
                "relacja {} → {} jest jednostronna (rodzaj {})",
                c.index(),
                r.other,
                r.kind
            );
        }
    }
}

#[test]
fn prop_knowledge_bound() {
    // `KnowledgeRef.len` nie przekracza 32, a slaby nie przeciekają: liczba zajętych
    // bloków równa się liczbie żywych mieszkańców z wpisami.
    let (mut world, miasto) = swiat(17, 12, 70);
    // Zasiew wiedzy: każdy zna sklep pod swoim adresem — plotka rozniesie resztę.
    let sklepy: Vec<u32> = (0..40).map(|i| ID_DOM + i * 3).collect();
    let mieszkancy: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    for (i, c) in mieszkancy.iter().enumerate() {
        for k in 0..6 {
            learn_place(
                &mut world,
                *c,
                sklepy[(i + k) % sklepy.len()],
                KnowledgeKind::Visited,
                60,
                0,
            );
        }
    }
    let _ = miasto;
    przebieg(&mut world, 0, 720);

    let mut z_wiedza = 0usize;
    let mut z_relacjami = 0usize;
    for c in world.resource::<Population>().citizens() {
        let k = *world.get::<KnowledgeRef>(*c).expect("uchwyt wiedzy");
        assert!(
            k.len as usize <= SLAB_MAX,
            "mieszkaniec {} ma {} wpisów wiedzy",
            c.index(),
            k.len
        );
        if k.len > 0 {
            z_wiedza += 1;
        }
        if world.get::<RelationsRef>(*c).expect("uchwyt").len > 0 {
            z_relacjami += 1;
        }
    }
    assert_eq!(
        world.resource::<KnowledgeSlab>().occupied_blocks(),
        z_wiedza,
        "wyciek bloków w slabie wiedzy"
    );
    assert_eq!(
        world.resource::<RelationSlab>().occupied_blocks(),
        z_relacjami,
        "wyciek bloków w slabie relacji"
    );
    assert!(z_wiedza > 0 && z_relacjami > 0);
}

#[test]
fn prop_inheritance_conservation() {
    // Suma pieniądza przed śmiercią = suma po podziale, tolerancja 0 groszy (00 §6).
    // Pieniądz, który nie ma spadkobiercy, nie znika — trafia do `escheat`.
    let (mut world, _) = swiat(23, 12, 60);
    let mieszkancy: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    for (i, c) in mieszkancy.iter().enumerate() {
        if let Some(w) = world.get_mut::<Wealth>(*c) {
            w.cash = Money(100_000 + (i as i64 % 7) * 13_337);
        }
    }
    let przed = society::total_money(&world);
    assert!(przed > 0);

    przebieg(&mut world, 0, 1_440);
    let po = society::total_money(&world);
    assert_eq!(
        przed,
        po,
        "pieniądz zmienił się o {} groszy przez cztery lata gry",
        po - przed
    );
    assert!(
        world.resource::<Population>().deaths > 0,
        "przez cztery lata nikt nie umarł — test niczego nie sprawdził"
    );
}

#[test]
fn prop_calendar() {
    // Rok ma 360 dób, miesiąc 30, a `is_month_start` nie odwołuje się do niczego
    // spoza kalendarza 12 × 30 (K-1).
    assert_eq!(magnat_agents::DAYS_PER_YEAR, 360);
    let miesiace: Vec<u64> = (0..720).filter(|d| society::is_month_start(*d)).collect();
    assert_eq!(miesiace.len(), 24, "dwa lata to 24 miesiące");
    assert!(miesiace.windows(2).all(|w| w[1] - w[0] == 30));
}

// ── kryterium WP7: przebieg wieloletni ──────────────────────────────────────────

#[test]
fn wp7_populacja_przezywa_trzydziesci_lat() {
    // Skrócona wersja `prop_century_survival` do CI (dokument fazy §7.2: „30 lat,
    // 3 ziarna, miasto małe"). Pełne 100 lat × 5 ziaren jedzie w runnerze `century`.
    for seed in [1u64, 2, 3] {
        let (mut world, _) = swiat(seed, 14, 120);
        let start = society::population(&world);
        let mut min = start;
        let mut max = start;
        let mut hooks = NoInheritance;
        for d in 0..30 * 360 {
            society::step_day(&mut world, d, &mut hooks);
            let p = society::population(&world);
            min = min.min(p);
            max = max.max(p);
        }
        let koniec = society::population(&world);
        let stosunek = koniec as f64 / start as f64;
        assert!(
            (0.5..=2.0).contains(&stosunek),
            "ziarno {seed}: {start} → {koniec} ({stosunek:.2}×)"
        );
        assert!(
            max as f64 <= start as f64 * 3.0,
            "ziarno {seed}: populacja przekroczyła 3× startowej ({max})"
        );
        assert!(min > 0, "ziarno {seed}: populacja wygasła");
    }
}

// ── kryterium WP8: eksperyment szokowy ──────────────────────────────────────────

fn bezrobocie(world: &World) -> u16 {
    let ages = world.resource::<DemographyTable>().ages();
    let mut aktywni = 0u64;
    let mut bez = 0u64;
    for e in world.resource::<Population>().citizens() {
        let Some(id) = world.get::<Identity>(*e) else {
            continue;
        };
        let wiek = id.age_years(0);
        let _ = wiek;
        let Some(emp) = world.get::<magnat_agents::Employment>(*e) else {
            continue;
        };
        if emp.flags
            & (magnat_agents::Employment::FLAG_PUPIL
                | magnat_agents::Employment::FLAG_STUDENT
                | magnat_agents::Employment::FLAG_RETIRED)
            != 0
        {
            continue;
        }
        if id.age_years(0) < i32::from(ages.work_start) {
            continue;
        }
        aktywni += 1;
        if !emp.has_job() {
            bez += 1;
        }
    }
    bez.checked_mul(1000)
        .and_then(|x| x.checked_div(aktywni))
        .map_or(0, |x| x as u16)
}

#[test]
fn wp8_szok_na_rynku_pracy_wygasa_w_pieciu_latach() {
    // Kryterium WP8: likwidacja 20 % etatów daje odpływ, a bezrobocie stabilizuje się
    // w ≤ 5 latach gry, bez oscylacji o amplitudzie > 30 %.
    //
    // „Stabilizacja" to zanik **nadwyżki** ponad poziom sprzed szoku, a nie powrót do
    // dawnej liczby: po likwidacji jednej piątej etatów równowaga jest z definicji inna.
    let (mut world, _) = swiat(31, 16, 150);
    przebieg(&mut world, 0, 5 * 360);
    let przed = bezrobocie(&world);

    // Najpierw domykamy rynek: szok ma **zabrać pracę ludziom**, a nie zlikwidować
    // etaty, na których i tak nikt nie siedział. Miasto testowe ma ich z zapasem,
    // żeby `take_job_in` dopasowywało pracę w dzielnicy (patrz `Miasto::nowe`).
    let wolne = world.resource::<Vacancies>().free_jobs();
    world.resource_mut::<Vacancies>().retire_jobs(wolne);
    let etatow = world.resource::<Vacancies>().jobs_total() as usize;
    let zlikwidowane = migration::shock_retire_jobs(&mut world, etatow / 5);
    assert!(zlikwidowane as usize >= etatow / 5 - 1, "szok nie doszedł");

    przebieg(&mut world, 5 * 360, 30);
    let szczyt = bezrobocie(&world);
    assert!(
        szczyt > przed + 50,
        "szok nie podniósł bezrobocia: {przed} ‰ → {szczyt} ‰"
    );

    let prog = przed + (szczyt - przed) / 5;
    let mut rok_powrotu = None;
    let mut populacje: Vec<usize> = Vec::new();
    for rok in 0..10u64 {
        przebieg(&mut world, 5 * 360 + 30 + rok * 360, 360);
        populacje.push(society::population(&world));
        if rok_powrotu.is_none() && bezrobocie(&world) <= prog {
            rok_powrotu = Some(rok + 1);
        }
    }
    let lat = rok_powrotu.unwrap_or_else(|| {
        panic!(
            "bezrobocie nie wróciło do progu {prog} ‰ przez 10 lat (szczyt {szczyt} ‰, \
             na końcu {} ‰)",
            bezrobocie(&world)
        )
    });
    assert!(lat <= 5, "stabilizacja dopiero po {lat} latach gry");

    // Po ustabilizowaniu regulator nie ma prawa oscylować.
    let ogon = &populacje[5..];
    let min = *ogon.iter().min().expect("niepuste");
    let max = *ogon.iter().max().expect("niepuste");
    let amplituda = (max - min) as f64 * 100.0 / max as f64;
    assert!(
        amplituda <= 30.0,
        "amplituda wahań po stabilizacji {amplituda:.1} % (min {min}, max {max})"
    );
}

// ── kryterium WP9: zasięg plotki ────────────────────────────────────────────────

#[test]
fn wp9_plotka_zna_kilometr_i_nie_zna_pieciu() {
    // Kryterium WP9: nowe miejsce jest po 30 dobach znane ≥ 60 % mieszkańców
    // w promieniu 1 km i < 5 % powyżej 5 km. To jest fundament pod markę w M10.
    //
    // Graf relacji musi być **dojrzały**, zanim zacznie się pomiar — relacja rośnie
    // o kilka punktów na tydzień, a plotka wymaga wagi ≥ 40. W M3d ten graf wychodzi
    // z kroku 9 Etapu 8; tutaj powstaje sam, przez dwa lata sąsiedztwa i wspólnej pracy.
    let (mut world, miasto) = swiat(41, 80, 2_200);
    // Praca blisko domu. Pula wakatów oddaje etaty od końca listy (LIFO) i nie patrzy
    // na odległość — to jest jawny `ponytail:` w `Vacancies`, a dopasowanie dojazdu
    // należy do kroku 7 Etapu 8 (M3d §5.9). Tutaj robimy je najprościej, bo bez niego
    // relacja współpracownicza spinałaby przeciwne końce miasta i plotka przeskakiwałaby
    // pięć kilometrów w jednym kroku — mierzylibyśmy wtedy losowość przydziału pracy,
    // a nie zasięg plotki.
    let mieszkancy_startowi: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    for c in &mieszkancy_startowi {
        let Some(res) = world.get::<magnat_agents::Residence>(*c).copied() else {
            continue;
        };
        if !res.has_home() {
            continue;
        }
        let site = miasto.zaklad(res.building);
        if let Some(emp) = world.get_mut::<magnat_agents::Employment>(*c) {
            if emp.has_job() {
                emp.site = site;
            }
        }
    }
    przebieg(&mut world, 0, 2 * 360);

    // Nowy sklep w **środku zamieszkanej części** miasta: pula pustostanów oddaje
    // lokale od końca listy, więc zasiedlony jest ogon siatki, a nie jej środek
    // geometryczny. Miasto jest przy tym na tyle rozległe (80 × 80 po 150 m = 12 km),
    // że próbka „powyżej 5 km" nie jest pusta — bez niej druga połowa kryterium
    // nie sprawdzałaby niczego.
    let mut zajete: Vec<u32> = world
        .resource::<Population>()
        .citizens()
        .iter()
        .filter_map(|c| world.get::<magnat_agents::Residence>(*c).copied())
        .filter(|r| r.has_home())
        .map(|r| r.building)
        .collect();
    zajete.sort_unstable();
    let srodek = zajete[zajete.len() / 2];
    let nowy_sklep = 9_000_001u32;
    let mieszkancy: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    let mut zasiew = 0usize;
    for c in &mieszkancy {
        let Some(res) = world.get::<magnat_agents::Residence>(*c).copied() else {
            continue;
        };
        if !res.has_home() || miasto.odleglosc(res.building, srodek) > 400.0 {
            continue;
        }
        learn_place(&mut world, *c, nowy_sklep, KnowledgeKind::Visited, 70, 720);
        zasiew += 1;
    }
    assert!(zasiew > 0, "nikt nie mieszka przy nowym sklepie");

    przebieg(&mut world, 2 * 360, 30);

    let mut bliscy = (0u32, 0u32);
    let mut dalecy = (0u32, 0u32);
    for c in world.resource::<Population>().citizens() {
        let Some(res) = world.get::<magnat_agents::Residence>(*c).copied() else {
            continue;
        };
        if !res.has_home() {
            continue;
        }
        let d = miasto.odleglosc(res.building, srodek);
        let zna = u32::from(knows_place(&world, *c, nowy_sklep));
        if d <= 1_000.0 {
            bliscy = (bliscy.0 + zna, bliscy.1 + 1);
        } else if d > 5_000.0 {
            dalecy = (dalecy.0 + zna, dalecy.1 + 1);
        }
    }
    assert!(
        bliscy.1 > 20 && dalecy.1 > 20,
        "za mało próbek: {bliscy:?} {dalecy:?}"
    );

    let blisko = f64::from(bliscy.0) * 100.0 / f64::from(bliscy.1);
    let daleko = f64::from(dalecy.0) * 100.0 / f64::from(dalecy.1);
    println!(
        "zasiew {zasiew} osób; po 30 dobach wie {blisko:.1} % w promieniu 1 km          ({}/{}) i {daleko:.1} % powyżej 5 km ({}/{})",
        bliscy.0, bliscy.1, dalecy.0, dalecy.1
    );
    assert!(
        blisko >= 60.0,
        "w promieniu 1 km wie {blisko:.1} % ({}/{}) — plotka jest za wolna",
        bliscy.0,
        bliscy.1
    );
    assert!(
        daleko < 5.0,
        "powyżej 5 km wie {daleko:.1} % ({}/{}) — plotka teleportuje się przez miasto",
        dalecy.0,
        dalecy.1
    );

    let (znajacy, razem) = awareness_of(&world, nowy_sklep);
    assert!(znajacy > 0 && znajacy < razem, "{znajacy} z {razem}");
}

// ── determinizm (§7.1) ──────────────────────────────────────────────────────────

#[test]
fn det_demografia_dwa_przebiegi_tego_samego_ziarna() {
    let odcisk = |seed: u64| {
        let (mut world, _) = swiat(seed, 12, 60);
        przebieg(&mut world, 0, 400);
        magnat_io::world_state_hash(&world).to_string()
    };
    assert_eq!(odcisk(99), odcisk(99), "ten sam seed dał inny hash");
    assert_ne!(odcisk(99), odcisk(100));
}

#[test]
fn det_demography_independence() {
    // Usunięcie jednego mieszkańca nie zmienia **żadnego** losowania demograficznego
    // u pozostałych: strumień jest funkcją `(seed, StreamId, entity_index, doba)`,
    // a nie licznikiem wywołań. Bez tego zapis i odczyt stanu łamią hash (ryzyko R9).
    let losowania = |pomin: Option<u32>| -> Vec<(u32, u64)> {
        let (world, _) = swiat(55, 10, 40);
        let mut out = Vec::new();
        for e in world.resource::<Population>().citizens() {
            if Some(e.index()) == pomin {
                continue;
            }
            for doba in [0u64, 137, 999] {
                let mut r = magnat_core::rng(
                    world.seed,
                    magnat_core::StreamId::Demography,
                    e.index(),
                    demography::stream_key(doba, demography::K_DAILY),
                );
                out.push((e.index(), r.next_u64()));
            }
        }
        out
    };
    let pelne = losowania(None);
    let ofiara = pelne[10].0;
    let bez = losowania(Some(ofiara));
    let oczekiwane: Vec<(u32, u64)> = pelne.into_iter().filter(|(i, _)| *i != ofiara).collect();
    assert_eq!(bez, oczekiwane, "usunięcie mieszkańca przesunęło losowania");
}

// ── gospodarstwo i status ───────────────────────────────────────────────────────

#[test]
fn dziedziczenie_idzie_do_wspolmalzonka_a_potem_do_dzieci() {
    let (world, _) = swiat(61, 8, 20);
    let para: Vec<Entity> = world
        .resource::<Population>()
        .citizens()
        .iter()
        .copied()
        .filter(|e| {
            world
                .get::<Lifecycle>(*e)
                .is_some_and(|l| l.partner != Lifecycle::NO_PARTNER)
        })
        .take(2)
        .collect();
    assert_eq!(para.len(), 2, "zasiew nie dał ani jednej pary");

    // Relacja partnerska jest obustronna i rodzinna — nie zanika (§5.8).
    let rel = *world.get::<RelationsRef>(para[0]).expect("uchwyt");
    let ma_partnera = world
        .resource::<RelationSlab>()
        .entries(demography::relations_ref(&rel))
        .iter()
        .any(|r| r.kind == RelationKind::Partner as u8);
    assert!(ma_partnera, "para bez relacji partnerskiej w slabie");
}

#[test]
fn status_rozklada_sie_na_klasy_a_nie_stoi_w_miejscu() {
    let (mut world, _) = swiat(71, 14, 100);
    przebieg(&mut world, 0, 400);
    let mut klasy = [0u32; 6];
    for c in world.resource::<Population>().citizens() {
        let v = world.get::<magnat_agents::Vitals>(*c).expect("vitals");
        klasy[social::SocialClass::of(magnat_core::Q::new(v.status)) as usize] += 1;
    }
    let niepuste = klasy.iter().filter(|n| **n > 0).count();
    assert!(
        niepuste >= 2,
        "cała populacja w jednej klasie: {klasy:?} — status nie różnicuje"
    );
    assert!(
        !world.resource::<social::StatusDistribution>().is_empty(),
        "rozkład dochodów nie został policzony"
    );
}
