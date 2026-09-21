//! Związki zawodowe, strajki i zmowy cenowe — kryteria WP10.13 i WP10.14 (M10e §5.9).
//!
//! Testy chodzą **przez `step_day`**, a nie przez same funkcje czyste: kryteria
//! mówią o czasie („związek w ciągu 3–9 miesięcy", „strajk 4–21 dni"), a czas
//! istnieje dopiero w pętli. Same reguły (żal, spójna składowa, próg akceptacji,
//! hazard wykrycia) mają testy jednostkowe przy sobie, w `relations::grievance`
//! i `relations::cartel`.

mod common;

use common::ent;
use magnat_agents::{
    Employment as CitEmployment, Household, Population, Relation, RelationKind, RelationSlab,
    RelationsRef, ShiftKind, SlabRef, SocialIndex, Vitals,
};
use magnat_core::{DistrictId, GoodId, JobRoleId, Money, SimMinute, SiteId, Tick, Q};
use magnat_economy::relations::{step_day, RelationsTuning, UnionState, Unions};
use magnat_ecs::World;
use magnat_firms::{
    hr::employment::{Employment, Position},
    Firm, FirmKey, FirmPersonality, Firms, ManagementQuality, Owner, OwnerShare, Ring, Site,
    SitePnlMonth, SiteTypeId,
};

const DOBA: u64 = 1_440;
const ROLA: JobRoleId = JobRoleId(3);
/// Płaca w badanym zakładzie i płaca w mieście: dwadzieścia procent różnicy,
/// dokładnie tyle, ile mówi kryterium WP10.14.
const PLACA_TU: i64 = 400_000;
const PLACA_MIASTA: i64 = 500_000;

/// Zakład z załogą, marżą i gęstą siecią znajomości — czyli dokładnie to, co
/// kryterium WP10.14 opisuje słowami.
struct Zaklad {
    site: SiteId,
    firm: FirmKey,
    placa: i64,
    marza_bp: i32,
    zaloga: Vec<magnat_core::Entity>,
}

/// Świat: `n` badanych zakładów o niskiej płacy plus jeden „rynkowy", który
/// wyznacza medianę miejską. Bez tego drugiego luka płacowa nie miałaby wobec
/// czego istnieć — a mediana z jednego zakładu jest jego własną płacą.
fn swiat(n: u32, zaloga: u32, marza_bp: i32) -> (World, Vec<Zaklad>) {
    let mut w = World::new(11);
    magnat_agents::register_components(&mut w);
    // Slab relacji, a nie cały `register_resources`: ten drugi chce tabeli potrzeb,
    // której ten test nie ma po co wczytywać.
    w.insert_resource(RelationSlab::default());
    w.insert_resource(SocialIndex::new());

    let mut firms = Firms::new();
    let mut pop = Population::new();
    let mut zaklady = Vec::new();

    // Zakład odniesienia: płaci jak miasto i ma **więcej** ludzi niż badane razem,
    // więc to on wyznacza medianę zawodu.
    zaklady.push(postaw(
        &mut w,
        &mut firms,
        &mut pop,
        0,
        zaloga * n.max(1) + 1,
        PLACA_MIASTA,
        marza_bp,
    ));
    for i in 1..=n {
        zaklady.push(postaw(
            &mut w, &mut firms, &mut pop, i, zaloga, PLACA_TU, marza_bp,
        ));
    }
    zawiaz_relacje(&mut w, &zaklady);

    w.insert_resource(firms);
    w.insert_resource(pop);
    let mut idx = SocialIndex::new();
    idx.rebuild(&w);
    *w.resource_mut::<SocialIndex>() = idx;

    magnat_economy::register_relations(&mut w, RelationsTuning::default());
    (w, zaklady)
}

/// Stawia firmę z jednym zakładem, obsadza go i zawiązuje znajomości w załodze.
#[allow(clippy::too_many_arguments)]
fn postaw(
    w: &mut World,
    firms: &mut Firms,
    pop: &mut Population,
    nr: u32,
    zaloga: u32,
    placa: i64,
    marza_bp: i32,
) -> Zaklad {
    let site = SiteId(ent(1_000 + nr));
    let key = firms.insert(|k| {
        let mut f = Firm::new(
            k,
            format!("firma {nr}"),
            SimMinute(0),
            DistrictId(0),
            smallvec_owner(),
        );
        f.personality = FirmPersonality::NEUTRAL;
        f
    });

    let mut poz = Position::new(
        ROLA,
        u16::try_from(zaloga).unwrap_or(u16::MAX),
        false,
        (Money(placa / 2), Money(placa * 2)),
    );
    let mut ludzie = Vec::new();
    for i in 0..zaloga {
        let e = w
            .spawn()
            .with(CitEmployment {
                site: site.entity().index(),
                role: ROLA.0,
                ..CitEmployment::default()
            })
            .with(Vitals {
                stress: 30,
                ..Vitals::default()
            })
            .with(RelationsRef::default())
            .id();
        pop.add_citizen(e);
        ludzie.push(e);
        poz.filled.push(Employment::new(
            magnat_core::CitizenId(e),
            ROLA,
            Money(placa),
            SimMinute(0),
            ShiftKind::Day,
        ));
        // Gospodarstwo z realnym buforem: półtora miesiąca płacy na koncie.
        // Fundusz strajkowy to **nadwyżka ponad miesiąc utrzymania**, więc załoga
        // wytrzyma bez pensji około pół miesiąca — i to jest liczba, z której
        // wychodzi pasmo 4–21 dób z kryterium WP10.14.
        let mut h = Household {
            size: 1,
            flags: Household::FLAG_ACTIVE,
            bank: Money(placa * 3 / 2),
            income_monthly: Money(placa),
            ..Household::default()
        };
        h.members[0] = e.index();
        let hh = w.spawn().with(h).id();
        pop.add_household(hh);
        let _ = i;
    }

    let mut s = Site {
        id: site,
        firm: key,
        site_type: SiteTypeId(0),
        building: magnat_core::BuildingId(ent(500 + nr)),
        district: DistrictId(0),
        floor_m2: 400,
        positions: vec![poz],
        mgmt: ManagementQuality::NEUTRAL,
        tech: Q::new(50),
        fixed_cost_month: Money(0),
        hr_accrued: Money::ZERO,
        rnd_accrued: Money::ZERO,
        pnl: Ring::<SitePnlMonth, 36>::new(),
        opened: SimMinute(0),
        strike_bps: 0,
        strike_bp_days: 0,
        delegation: None,
    };
    // Rachunek wyniku o zadanej marży — bez niego związek nie wie, czy jest
    // z czego dać, i nie formuje się w ogóle.
    let utarg = 10_000_000i64;
    s.pnl.push(SitePnlMonth {
        month: 0,
        revenue: Money(utarg),
        cogs: Money(utarg - i64::from(marza_bp) * utarg / 10_000),
        labor: Money::ZERO,
        fixed: Money::ZERO,
    });
    firms.add_site(s);

    Zaklad {
        site,
        firm: key,
        placa,
        marza_bp,
        zaloga: ludzie,
    }
}

fn smallvec_owner() -> smallvec::SmallVec<[OwnerShare; 4]> {
    smallvec::smallvec![OwnerShare {
        owner: Owner::City,
        bp: 10_000
    }]
}

/// Dwa rodzaje znajomości i oba są potrzebne (§5.9).
///
/// **Wewnątrz zakładu** — każdy zna każdego, więc spójna składowa równa się całej
/// załodze i pierwszy warunek powstania związku jest spełniony.
///
/// **Poza zakładem** — każdy zna jednego pracownika zakładu rynkowego. To jest
/// jedyne źródło kotwicy żądania: załoga żąda tego, co **wie** o płacach gdzie
/// indziej, a nie prawdziwej mediany. Bez tych krawędzi związek powstaje i nie
/// ma czego żądać — i dokładnie tak wyglądał pierwszy przebieg tego testu.
fn zawiaz_relacje(w: &mut World, zaklady: &[Zaklad]) {
    let rynek = zaklady[0].zaloga.clone();
    let mut pary: Vec<(magnat_core::Entity, Vec<u32>)> = Vec::new();
    for (i, z) in zaklady.iter().enumerate().skip(1) {
        for (j, a) in z.zaloga.iter().enumerate() {
            let mut inni: Vec<u32> = z
                .zaloga
                .iter()
                .filter(|b| *b != a)
                .map(|b| b.index())
                .collect();
            // Jeden znajomy z zakładu rynkowego na osobę, rozłożony po jego załodze.
            if let Some(o) = rynek.get((i * 7 + j) % rynek.len()) {
                inni.push(o.index());
            }
            pary.push((*a, inni));
        }
    }
    let mut uchwyty = Vec::new();
    {
        let slab = w.resource_mut::<RelationSlab>();
        for (a, inni) in pary {
            let mut r = SlabRef::EMPTY;
            for b in inni {
                slab.push(
                    &mut r,
                    Relation {
                        other: b,
                        kind: RelationKind::Colleague as u8,
                        weight: 60,
                        last_contact_day: 0,
                    },
                    |_| 0,
                );
            }
            uchwyty.push((a, r));
        }
    }
    for (e, r) in uchwyty {
        if let Some(slot) = w.get_mut::<RelationsRef>(e) {
            slot.handle = r.handle;
            slot.len = r.len;
            slot.class = r.class;
        }
    }
}

fn przebieg(w: &mut World, od: u32, do_: u32) {
    for d in od..do_ {
        step_day(w, Tick(u64::from(d) * DOBA));
    }
}

// ── WP10.14: związek i strajk ───────────────────────────────────────────────────

/// Kryterium WP10.14, pierwsza połowa: **zakład o marży 35 % płacący 20 % poniżej
/// mediany miejskiej, z gęstą siecią relacji, formuje związek w 3–9 miesięcy.**
#[test]
fn zaklad_o_marzy_35_i_placy_20_ponizej_mediany_formuje_zwiazek_w_3_9_miesiacach() {
    let (mut w, zaklady) = swiat(1, 12, 3_500);
    let badany = zaklady[1].site;
    let mut doba_zawiazania = None;
    for d in 0..10u32 * 30 {
        step_day(&mut w, Tick(u64::from(d) * DOBA));
        if doba_zawiazania.is_none() && w.resource::<Unions>().get(badany).is_some() {
            doba_zawiazania = Some(d);
        }
    }
    let d = doba_zawiazania.expect("związek nie powstał przez dziesięć miesięcy");
    assert!(
        (90..=270).contains(&d),
        "związek zawiązał się w dobie {d}, a kryterium mówi o 3–9 miesiącach"
    );
}

/// Druga połowa pierwszej: **pracodawca płacący jak rynek nie dorabia się związku.**
/// Bez tego testu poprzedni mierzyłby wyłącznie to, że mechanizm w ogóle działa.
#[test]
fn pracodawca_placacy_jak_rynek_nie_dorabia_sie_zwiazku() {
    // Wszystkie zakłady płacą tyle samo, więc luka płacowa jest zerem.
    let (mut w, zaklady) = swiat(1, 12, 1_000);
    let badany = zaklady[1].site;
    for z in &zaklady {
        let _ = z.placa;
    }
    // Zrównanie płac: badany zakład dostaje stawkę rynkową.
    {
        let firms = w.resource_mut::<Firms>();
        let s = firms.site_mut(badany).expect("zakład");
        for p in &mut s.positions {
            for e in &mut p.filled {
                e.wage_month = Money(PLACA_MIASTA);
            }
        }
    }
    przebieg(&mut w, 0, 12 * 30);
    assert!(
        w.resource::<Unions>().get(badany).is_none(),
        "związek powstał tam, gdzie płacą jak rynek i marża jest niska"
    );
}

/// Kryterium WP10.14, druga połowa: **strajk staje, kosztuje i sam się kończy.**
///
/// Sto strajków, mediana 4–21 dni, maksimum poniżej 90. Sto zakładów w jednym
/// świecie, a nie sto światów: strajki są niezależne, a jeden przebieg mierzy
/// to samo taniej.
#[test]
fn sto_strajkow_konczy_sie_w_pasmie_4_21_dni_i_zaden_nie_trwa_wiecznie() {
    let (mut w, zaklady) = swiat(100, 12, 3_500);
    let badane: Vec<SiteId> = zaklady[1..].iter().map(|z| z.site).collect();
    let mut poczatek: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    #[allow(clippy::items_after_statements)]
    let mut dlugosci: Vec<u32> = Vec::new();
    let mut strajkowalo = std::collections::BTreeSet::new();

    for d in 0..4u32 * 360 {
        step_day(&mut w, Tick(u64::from(d) * DOBA));
        let u = w.resource::<Unions>();
        for s in &badane {
            let k = s.entity().index();
            let teraz = u.get(*s).is_some_and(|z| z.state.is_striking());
            match (poczatek.get(&k).copied(), teraz) {
                (None, true) => {
                    poczatek.insert(k, d);
                    strajkowalo.insert(k);
                }
                (Some(od), false) => {
                    poczatek.remove(&k);
                    dlugosci.push(d - od);
                }
                _ => {}
            }
        }
    }

    assert!(
        dlugosci.len() >= 100,
        "zmierzono tylko {} zakończonych strajków — za mało na kryterium",
        dlugosci.len()
    );
    // Strajk trwający na końcu okna nie jest wieczny — jest zaczęty niedawno.
    // Wieczny to taki, który przekroczył twardy sufit i nadal stoi.
    let koniec_okna = 4 * 360;
    let wieczne = poczatek
        .values()
        .filter(|od| koniec_okna - *od >= 90)
        .count();
    assert_eq!(
        wieczne, 0,
        "{wieczne} strajków przekroczyło dziewięćdziesiąt dób i nadal trwa"
    );
    dlugosci.sort_unstable();
    let mediana = dlugosci[dlugosci.len() / 2];
    let maks = *dlugosci.last().expect("jakiś strajk");
    assert!(
        (4..=21).contains(&mediana),
        "mediana długości strajku to {mediana} dni, a kryterium mówi 4–21"
    );
    assert!(maks < 90, "najdłuższy strajk trwał {maks} dni");
}

/// Strajk **zatrzymuje zakład i kosztuje załogę** — dwie strony jednej liczby.
/// Bez tego poprzedni test mierzyłby wyłącznie czas trwania flagi.
#[test]
fn strajk_zatrzymuje_zaklad_i_zabiera_zaloge_z_listy_plac() {
    let (mut w, zaklady) = swiat(1, 12, 3_500);
    let badany = zaklady[1].site;
    let mut widziano_przestoj = false;
    for d in 0..2u32 * 360 {
        step_day(&mut w, Tick(u64::from(d) * DOBA));
        let strajkuje = w
            .resource::<Unions>()
            .get(badany)
            .is_some_and(|z| z.state.is_striking());
        let s = w.resource::<Firms>().site(badany).expect("zakład");
        if strajkuje {
            assert!(
                s.strike_bps > 0,
                "zakład nie wie o strajku swojej załogi w dobie {d}"
            );
            assert!(
                s.strike_bp_days > 0,
                "przestój nie narasta, więc lista płac zapłaci za dni, w których nikt nie pracował"
            );
            widziano_przestoj = true;
        } else if widziano_przestoj {
            assert_eq!(
                s.strike_bps, 0,
                "po zakończeniu strajku zakład nadal stoi w dobie {d}"
            );
            break;
        }
    }
    assert!(widziano_przestoj, "w dwa lata nie było ani jednego strajku");
}

/// Żądanie płacowe jest **zakotwiczone na tym, co załoga wie**, a nie na prawdziwej
/// medianie: znajomości spoza zakładu podnoszą kotwicę, ich brak ją zbija.
#[test]
fn zadanie_placowe_powstaje_i_niesie_kotwice() {
    let (mut w, zaklady) = swiat(1, 12, 3_500);
    let badany = zaklady[1].site;
    let mut zadanie = None;
    for d in 0..10u32 * 30 {
        step_day(&mut w, Tick(u64::from(d) * DOBA));
        if let Some(z) = w.resource::<Unions>().get(badany) {
            if let Some(dem) = z.demand {
                zadanie = Some(dem);
                break;
            }
        }
    }
    let dem = zadanie.expect("związek nie przedstawił żądania");
    assert!(
        dem.raise_bp > 0,
        "żądanie o zerowej podwyżce to nie żądanie"
    );
    assert!(
        dem.raise_bp <= RelationsTuning::default().union.demand_cap_bp,
        "żądanie {} bp ponad sufitem",
        dem.raise_bp
    );
    assert!(
        dem.anchor.get() > PLACA_TU,
        "kotwica {} nie jest wyższa od dzisiejszej płacy, więc nie ma o co prosić",
        dem.anchor.get()
    );
}

/// Spór kończy się **albo ugodą, albo kapitulacją** — i jedno i drugie zostawia
/// związek w stanie spokoju, a nie w zawieszeniu.
#[test]
fn spor_zawsze_dochodzi_do_konca() {
    let (mut w, zaklady) = swiat(1, 12, 3_500);
    let badany = zaklady[1].site;
    przebieg(&mut w, 0, 3 * 360);
    let z = w.resource::<Unions>().get(badany).expect("związek");
    assert!(
        matches!(z.state, UnionState::Dormant { .. }),
        "po trzech latach spór nadal trwa: {:?}",
        z.state
    );
    for zz in &zaklady {
        let _ = zz.marza_bp;
        let _ = zz.firm;
    }
}

// ── WP10.13: zmowa cenowa ───────────────────────────────────────────────────────

/// Świat trzech sklepów z tym samym towarem w jednej dzielnicy.
///
/// Firmy są tu skłonne do ryzyka, bo zmowa jest decyzją firmy, a nie rynku:
/// ostrożny właściciel się nie zmawia i to jest część mechanizmu, nie jego brak.
/// Identyfikatory sklepów wychodzą z `firm_id(FirmKey)`, bo tylko wtedy rejestr
/// firm i rynek mówią o tych samych firmach (`K-46`).
fn swiat_zmowy(ryzyko: u8) -> (World, GoodId, Vec<SiteId>) {
    use magnat_agents::{NeedTable, PlaceEntry, PlaceTable};
    use magnat_core::{BuildingId, PlaceKind, PlaceRef, Qty, WorldCoord};
    use magnat_economy::{
        AccountKind, AccountOwner, Books, EconomyData, Market, ShopSeed, TxKind, TxMemo,
    };
    use magnat_spatial::Vec2;
    use std::sync::Arc;

    const N: u32 = 3;
    let data = EconomyData::load_default().expect("data/economy/");
    let katalog = common::goods(&data);
    let towar = common::good_by_key(&data, "food_bread_wheat");
    let needs = Arc::new(NeedTable::load_default().expect("data/needs/"));

    let sites: Vec<SiteId> = (0..N).map(|i| SiteId(ent(100 + i))).collect();
    let mut entries = vec![PlaceEntry {
        place: PlaceRef::Building(BuildingId(ent(0))),
        kind: PlaceKind::Home,
        at: WorldCoord::new(0, 0, 0),
    }];
    for (i, s) in sites.iter().enumerate() {
        entries.push(PlaceEntry {
            place: PlaceRef::Site(*s),
            kind: PlaceKind::Grocery,
            at: WorldCoord::new(30_000 + i as i32 * 10_000, 0, 0),
        });
    }

    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    books.endow(rest, Money(1_000_000_000), Tick(0)).unwrap();
    let market = Market::new(
        common::spec(),
        5,
        data,
        katalog,
        common::lancuch_testowy(),
        needs,
        Arc::new(PlaceTable::build(entries)),
        rest,
    );

    let mut w = World::new(5);
    magnat_agents::register_components(&mut w);
    w.insert_resource(RelationSlab::default());
    w.insert_resource(SocialIndex::new());
    w.insert_resource(Population::new());

    let mut firms = Firms::new();
    for (i, site) in sites.iter().enumerate() {
        let key = firms.insert(|k| {
            let mut f = Firm::new(
                k,
                format!("sklep {i}"),
                SimMinute(0),
                DistrictId(0),
                smallvec_owner(),
            );
            f.personality = FirmPersonality {
                risk_tolerance: ryzyko,
                ..FirmPersonality::NEUTRAL
            };
            f
        });
        let firm = magnat_firms::firm_id(key);
        let acc = books.open_account(
            AccountOwner::Firm(firm),
            AccountKind::Current,
            None,
            Money::ZERO,
        );
        books
            .transfer(
                rest,
                acc,
                Money(50_000_000),
                TxMemo::new(TxKind::Endowment, magnat_core::DecisionReason::Unspecified),
                Tick(0),
            )
            .unwrap();
        assert!(market.open_shop(
            ShopSeed {
                site: *site,
                firm,
                pos: Vec2::new(300.0 + i as f32 * 100.0, 0.0),
                kind: PlaceKind::Grocery,
                shelf_slots: 4,
                capacity_m3: 200,
                district: 0,
            },
            acc,
            Tick(0),
        ));
        market.record_capital(*site, Money(50_000_000), Tick(0));
        market.deliver_now(*site, towar, Qty(50_000), Money(10_000), None, Tick(0));
        // Zakład **także** w rejestrze firm: bez tego firma nie ma adresu, a urząd
        // antymonopolowy nie ma komu wystawić sprawy (podmiotem sprawy jest zakład,
        // `CA-4`). To nie jest wymóg testu, tylko tego, jak stoi prawdziwe miasto.
        firms.add_site(Site {
            id: *site,
            firm: key,
            site_type: SiteTypeId(0),
            building: magnat_core::BuildingId(ent(600 + i as u32)),
            district: DistrictId(0),
            floor_m2: 200,
            positions: Vec::new(),
            mgmt: ManagementQuality::NEUTRAL,
            tech: Q::new(50),
            fixed_cost_month: Money(0),
            hr_accrued: Money::ZERO,
            rnd_accrued: Money::ZERO,
            pnl: Ring::<SitePnlMonth, 36>::new(),
            opened: SimMinute(0),
            strike_bps: 0,
            strike_bp_days: 0,
            delegation: None,
        });
    }
    market.restock_shelves();

    w.insert_resource(books);
    w.insert_resource(market);
    w.insert_resource(firms);
    magnat_economy::register_relations(&mut w, RelationsTuning::default());
    (w, towar, sites)
}

/// Kryterium WP10.13, druga połowa: **zmowa podnosi cenę.**
///
/// Trzy sklepy z tym samym towarem w jednej dzielnicy zawiązują zmowę i cena
/// wszystkich trzech idzie w górę o piętnaście procent ponad medianę sprzed
/// zmowy — bo o tyle mówi cennik zmowy, a nie dlatego, że któryś podniósł sam.
#[test]
fn zmowa_podnosi_cene_we_wszystkich_sklepach_dzielnicy() {
    use magnat_economy::Market;
    let (mut w, towar, sites) = swiat_zmowy(90);
    let przed: Vec<Money> = sites
        .iter()
        .map(|s| w.resource::<Market>().price_at(*s, towar).expect("cena"))
        .collect();

    przebieg(&mut w, 0, 31);
    // Sklep ma asortyment z `retail.ron`, więc zmów jest tyle, ile wspólnych
    // towarów na półkach — a badany towar musi być wśród nich.
    assert!(
        w.resource::<magnat_economy::Cartels>()
            .iter()
            .any(|c| c.good == towar),
        "trzech sprzedawców tego samego towaru w dzielnicy nie zawiązało zmowy"
    );
    let po: Vec<Money> = sites
        .iter()
        .map(|s| w.resource::<Market>().price_at(*s, towar).expect("cena"))
        .collect();
    for (i, (a, b)) in przed.iter().zip(po.iter()).enumerate() {
        assert!(
            b.get() > a.get(),
            "sklep {i}: cena nie drgnęła ({} → {})",
            a.get(),
            b.get()
        );
    }
    let zmowa = w
        .resource::<magnat_economy::Cartels>()
        .iter()
        .find(|c| c.good == towar)
        .expect("zmowa")
        .clone();
    assert_eq!(zmowa.members.len(), 3);
    assert!(
        (14..=15).contains(&zmowa.deviation_pct()),
        "cennik zmowy to +15 % nad medianą, zmierzono {} %",
        zmowa.deviation_pct()
    );
}

/// Ostrożne firmy się nie zmawiają — mechanizm jest decyzją, a nie automatem.
#[test]
fn ostrozne_firmy_nie_zawiazuja_zmowy() {
    let (mut w, _, _) = swiat_zmowy(10);
    przebieg(&mut w, 0, 31);
    assert!(
        w.resource::<magnat_economy::Cartels>().is_empty(),
        "zmowa powstała mimo niskiej skłonności do ryzyka wszystkich firm"
    );
}

/// Trzecia połowa: **zmowa zostaje wykryta, rozwiązana i zgłoszona urzędowi,
/// a marki członków obrywają.**
///
/// Sprawę prowadzi M8 (`K-10`), więc model zostawia zgłoszenie w skrzynce;
/// test sprawdza, że skrzynka się napełnia i że zmowa znika razem z karencją.
#[test]
fn wykryta_zmowa_znika_zglasza_sie_urzedowi_i_zostawia_karencje() {
    let (mut w, _, _) = swiat_zmowy(90);
    przebieg(&mut w, 0, 31);
    let zmow = w.resource::<magnat_economy::Cartels>().len();
    assert!(zmow >= 1, "nie powstała żadna zmowa");

    let mut zgloszen = 0usize;
    let mut doba_wykrycia = None;
    for d in 31..30 * 240u32 {
        step_day(&mut w, Tick(u64::from(d) * DOBA));
        let c = w.resource_mut::<magnat_economy::Cartels>();
        let z = c.take_detected();
        if !z.is_empty() {
            zgloszen += z.len();
            doba_wykrycia = Some(d);
            break;
        }
    }
    let d = doba_wykrycia.expect("zmowa nie została wykryta przez dwadzieścia lat");
    assert_eq!(
        zgloszen % 3,
        0,
        "urząd ma dostać sprawę przeciw **każdemu** członkowi wykrytej zmowy"
    );
    let zostalo = w.resource::<magnat_economy::Cartels>().len();
    assert!(
        zostalo < zmow,
        "wykryta zmowa nie została rozwiązana: {zmow} → {zostalo}"
    );
    // Karencja: przez dwa lata nikt się na ten sam towar w tej dzielnicy nie zmówi,
    // więc liczba zmów nie może wrócić do stanu sprzed wykrycia.
    przebieg(&mut w, d + 1, d + 30 * 12);
    assert!(
        w.resource::<magnat_economy::Cartels>().len() < zmow,
        "zmowa odrodziła się w środku dwuletniej karencji"
    );
}

/// Trzy warunki powstania związku są **mierzone osobno i widać je osobno**
/// (§5.9, §6 pkt 5 dokumentu fazy).
///
/// Sama liczba związków nie odpowiada na pytanie „dlaczego go nie ma": zakład,
/// w którym żal sięga progu, ale załoga się nie zna, i zakład, w którym załoga
/// się zna, ale nie ma o co walczyć, wyglądają w niej identycznie — a są dwiema
/// różnymi sytuacjami i wymagają dwóch różnych ruchów pracodawcy. Ten test
/// pilnuje, że `Grievance` niesie wszystkie trzy liczby, a nie samą sumę.
#[test]
fn obserwacja_zakladu_niesie_wszystkie_trzy_warunki_osobno() {
    let (mut w, zaklady) = swiat(1, 12, 3_500);
    let badany = zaklady[1].site;
    let rynkowy = zaklady[0].site;
    przebieg(&mut w, 0, 31);

    let u = w.resource::<Unions>();
    let g = u.grievance(badany);
    assert!(
        g.level >= RelationsTuning::default().union.grievance_threshold,
        "żal {} nie sięga progu w zakładzie z kryterium",
        g.level
    );
    assert_eq!(
        g.component, 12,
        "klika dwunastu to spójna składowa dwunastu"
    );
    assert_eq!(g.density_bp, 10_000, "cała załoga zarabia poniżej mediany");

    // Zakład rynkowy jest przeciwieństwem: płaci jak miasto i nikt się w nim
    // nie zna, więc żaden z warunków nie jest spełniony.
    let r = u.grievance(rynkowy);
    assert_eq!(r.component, 0, "bez znajomości nie ma składowej");
    assert_eq!(r.density_bp, 0, "nikt nie zarabia poniżej mediany");
    assert_eq!(r.days_above, 0, "licznik dób stoi, bo żal nie sięga progu");

    // Obserwacja obejmuje **oba** zakłady, także ten bez sporu — inaczej raport
    // mierzyłby wyłącznie te, które już się buntują.
    let ile = u.watched().count();
    assert_eq!(ile, 2, "pod obserwacją mają być wszystkie zakłady z załogą");
}

/// `FF-23`: liczba podstawiona przez gracza **zastępuje** ustępstwo wyliczone
/// z marży, a reszta negocjacji liczy się tym samym kodem.
///
/// To jest cała treść kanału gracza z M10g: nie drugi silnik negocjacji, tylko
/// podstawienie — dokładnie tak, jak `PricePolicy::Fixed` podstawia cenę gracza
/// w miejsce przeceny. Test sprawdza obie strony tego zdania: że liczba gracza
/// robi różnicę (ugoda zamiast kolejnej rundy) i że **nie omija sufitu**, którym
/// związany jest każdy pracodawca.
#[test]
fn liczba_gracza_zastepuje_ustepstwo_wyliczone_z_marzy() {
    let (mut w, zaklady) = swiat(1, 12, 3_500);
    let badany = zaklady[1].site;

    // Doprowadź spór do chwili, w której jest o czym rozmawiać.
    let mut zadanie = 0u16;
    let mut doba = 0u32;
    for d in 0..10u32 * 30 {
        step_day(&mut w, Tick(u64::from(d) * DOBA));
        if let Some(z) = w.resource::<Unions>().get(badany) {
            if let Some(dem) = z.demand {
                zadanie = dem.raise_bp;
                doba = d;
                break;
            }
        }
    }
    assert!(zadanie > 0, "związek nie przedstawił żądania");

    // Zakład bez sporu nie przyjmuje odpowiedzi — panel wygasza wtedy przycisk,
    // a `precheck` odmawia z `NoDispute`. Odpowiedź bez pytania nie jest odpowiedzią.
    let obcy = zaklady[0].site;
    if !w.resource::<Unions>().in_dispute(obcy) {
        assert!(
            !w.resource_mut::<Unions>().set_player_offer(obcy, 500),
            "odpowiedź przyjęta w zakładzie, w którym nie ma sporu"
        );
    }

    // Gracz przyjmuje żądanie w całości. Załoga ma próg akceptacji, ale przy
    // pełnym żądaniu jest on spełniony z definicji — więc spór ma się zamknąć.
    assert!(
        w.resource_mut::<Unions>().set_player_offer(badany, zadanie),
        "spór trwa, a odpowiedź nie została przyjęta"
    );
    for d in doba..doba + 40 {
        step_day(&mut w, Tick(u64::from(d) * DOBA));
        if w.resource::<Unions>()
            .get(badany)
            .is_some_and(|z| matches!(z.state, UnionState::Dormant { .. }))
        {
            return;
        }
    }
    panic!("po przyjęciu całego żądania spór nadal trwa");
}

/// Sufit ustępstwa obowiązuje **także gracza**: panel jest podstawieniem liczby,
/// a nie zwolnieniem z reguły. Bez tego warunku gracz mógłby obiecać załodze
/// tysiąc procent i zamknąć każdy spór jednym kliknięciem.
#[test]
fn liczba_gracza_nie_omija_sufitu_ustepstwa() {
    let (mut w, zaklady) = swiat(1, 12, 3_500);
    let badany = zaklady[1].site;
    for d in 0..10u32 * 30 {
        step_day(&mut w, Tick(u64::from(d) * DOBA));
        if w.resource::<Unions>()
            .get(badany)
            .is_some_and(|z| z.demand.is_some())
        {
            break;
        }
    }
    assert!(w
        .resource_mut::<Unions>()
        .set_player_offer(badany, u16::MAX));
    let przed = placa(&w, badany);
    step_day(&mut w, Tick(u64::from(10u32 * 30) * DOBA));
    let po = placa(&w, badany);
    let sufit = RelationsTuning::default().union.concession_cap_bp;
    let wzrost_bp = (po.saturating_sub(przed)).saturating_mul(10_000) / przed.max(1);
    assert!(
        wzrost_bp <= i64::from(sufit),
        "gracz podniósł płacę o {wzrost_bp} bp wobec sufitu {sufit} bp"
    );
}

/// Miesięczna płaca zakładu — suma listy płac podzielona przez obsadę.
fn placa(w: &World, site: SiteId) -> i64 {
    let firms = w.resource::<magnat_firms::Firms>();
    let Some(s) = firms.site(site) else { return 0 };
    let ilu = s.headcount() as i64;
    if ilu == 0 {
        return 0;
    }
    s.labor_cost_month().get() / ilu
}
