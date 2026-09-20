//! Kryterium zamknięcia WP10.8, zdanie po zdaniu:
//!
//! 1. **firma z 4 badaczami (umiejętność 60) i budżetem 50 tys./mies. odkrywa węzeł
//!    o koszcie 1200 RP w 9 ± 1 miesiącu gry, deterministycznie**;
//! 2. **odkrycie przed rokiem „światowym" daje patent**;
//! 3. **po roku „światowym" węzeł jest dostępny bez patentu po obniżonym koszcie**;
//! 4. **licencja jest `ContractId` z M6, nie nowym mechanizmem**.
//!
//! Świat jest stawiany ręcznie, a nie generatorem, i to jest ta sama decyzja, którą
//! podjął `sim/media/tests/reach.rs`: kryterium podaje liczby („cztery, sześćdziesiąt,
//! dziewięć"), więc wejście musi być znane co do sztuki. Miasto z generatora mierzyłoby
//! własny rozkład obsady, a nie tempo badań.

use magnat_agents::{ShiftKind, Vitals};
use magnat_core::{
    BuildingId, CitizenId, ContractId, DistrictId, Entity, JobRoleId, Money, SimMinute, SiteId,
    TechId, Tick, Q,
};
use magnat_firms::firm::Owner;
use magnat_firms::hr::employment::Employment;
use magnat_firms::hr::roles::RoleTable;
use magnat_firms::rnd::tree::TechTree;
use magnat_firms::rnd::ChargeKind;
use magnat_firms::rnd::{step_day, RndData, RndDay, RndTuning};
use magnat_firms::{Firm, FirmKey, Firms, Position, Site, SitePnlMonth, SiteTypeId};
use std::num::NonZeroU32;

/// `researcher` jest jedyną rolą w tej tablicy, więc ma indeks zero.
const BADACZ: JobRoleId = JobRoleId(0);
const SEED: u64 = 4242;

fn role_table() -> RoleTable {
    // Te same wagi, co w `data/jobs/roles.ron` — rachunek z nagłówka
    // `data/tuning/rnd.ron` stoi na nich i na niczym więcej.
    RoleTable::parse(
        r#"(
            schema_version: 2,
            roles: [
                ( key: "researcher",
                  weights: (skill: 650, energy: 150, mood: 150, health: 50) ),
            ],
        )"#,
    )
    .expect("tablica ról")
}

fn w_formie() -> Vitals {
    Vitals {
        health: 100,
        energy: 100,
        mood: 100,
        stress: 0,
        edu_level: 0,
        edu_field: 0,
        status: 50,
        _pad: 0,
    }
}

fn mieszkaniec(i: u32) -> CitizenId {
    CitizenId(Entity::new(i, NonZeroU32::MIN))
}

fn zaklad(key: FirmKey, id: u32, badaczy: u16) -> Site {
    let mut p = Position::new(BADACZ, badaczy, false, (Money(520_000), Money(1_450_000)));
    for i in 0..badaczy {
        p.filled.push(Employment::new(
            mieszkaniec(1_000 + id * 100 + u32::from(i)),
            BADACZ,
            Money(760_000),
            SimMinute(0),
            ShiftKind::Day,
        ));
    }
    Site {
        id: SiteId(Entity::new(id, NonZeroU32::MIN)),
        firm: key,
        site_type: SiteTypeId(0),
        building: BuildingId(Entity::new(id, NonZeroU32::MIN)),
        district: DistrictId(1),
        floor_m2: 1_000,
        positions: vec![p],
        mgmt: magnat_firms::ManagementQuality::NEUTRAL,
        tech: Q::new(50),
        hr_accrued: Money::ZERO,
        rnd_accrued: Money::ZERO,
        fixed_cost_month: Money(1_200_000),
        pnl: magnat_firms::Ring::new(),
        opened: SimMinute(0),
        delegation: None,
    }
}

/// Rejestr z `n` firmami, każda z jednym laboratorium o czterech badaczach.
fn firmy(n: u32) -> (Firms, Vec<FirmKey>) {
    let mut f = Firms::new();
    let mut klucze = Vec::new();
    for i in 0..n {
        let key = f.insert(|k| {
            Firm::sole_owner(
                k,
                format!("Laboratorium {i}"),
                SimMinute(0),
                DistrictId(1),
                Owner::Player,
            )
        });
        assert!(f.add_site(zaklad(key, 10 + i, 4)));
        klucze.push(key);
    }
    (f, klucze)
}

/// Drzewo z jednym węzłem: koszt 1200 RP, rok światowy `world_year`.
fn drzewo(world_year: i32, start_year: i32) -> RndData {
    // Katalog **unikatowy na wywołanie**: testy jednego pliku biegną równolegle,
    // a dwa z nich proszą o to samo drzewo — wspólna ścieżka znaczyła zapis i odczyt
    // tego samego pliku naraz i test przewracał się zależnie od kolejności wątków.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let i = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("magnat_rnd_{world_year}_{i}"));
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    let p = dir.join("tech.ron");
    std::fs::write(
        &p,
        format!(
            r#"(
                schema_version: 1,
                branches: [(key: "x")],
                nodes: [(key: "a", branch: "x", prereqs: [], cost_rp: 1200,
                         world_year: {world_year}, gain: 8)],
            )"#
        ),
    )
    .expect("zapis");
    let cat = magnat_supply::catalog::load_default("contemporary").expect("katalog towarów");
    // Przełom wyłączony: kryterium mówi „deterministycznie w 9 ± 1 miesiącu", a rzut
    // 10–40 % na pozostałym koszcie potrafi z tego zrobić siedem. Rzadkość przełomu
    // sprawdza osobny test, który go **wymusza**.
    let t = RndTuning {
        breakthrough_per_10k_day: 0,
        ..RndTuning::default()
    };
    RndData::new(TechTree::load(&p, start_year, &cat).expect("drzewo"), t)
}

fn mint() -> impl FnMut() -> Option<ContractId> {
    let mut n = 1u32;
    move || {
        n += 1;
        Some(ContractId(Entity::new(n, NonZeroU32::MIN)))
    }
}

/// Puszcza `dob` dób badań i zwraca dobę odkrycia, jeśli nastąpiło.
fn przebieg(firms: &mut Firms, data: &RndData, dob: u32) -> Option<u32> {
    let roles = role_table();
    let zdrowi = |_: CitizenId| Some((w_formie(), Q::new(60)));
    let mut m = mint();
    for d in 0..dob {
        let dzien = RndDay {
            day: d,
            now: SimMinute(u64::from(d) * 1_440),
            tick: Tick(u64::from(d) * 1_440),
            world_seed: SEED,
            roles: &roles,
            researcher: Some(BADACZ),
            vitals: &zdrowi,
        };
        let out = step_day(firms, data, &dzien, &mut m);
        if !out.discovered.is_empty() {
            return Some(d);
        }
    }
    None
}

#[test]
fn czterech_badaczy_odkrywa_wezel_za_1200_rp_w_dziewiatym_miesiacu() {
    // Rok światowy daleko w przyszłości, więc koszt jest pełny: 1200 RP.
    let data = drzewo(2050, 1990);
    let (mut f, _) = firmy(1);
    let doba = przebieg(&mut f, &data, 400).expect("węzeł nieodkryty w 400 dób");
    let miesiac = doba / 30 + 1;
    assert!(
        (8..=10).contains(&miesiac),
        "odkrycie w {miesiac}. miesiącu (doba {doba}), a kryterium mówi 9 ± 1"
    );
    // Kalibracja stoi w **środku** przedziału, nie na jego krańcu: doba 254 jest
    // dwudziestą czwartą dobą dziewiątego miesiąca. Ostrzejsza asercja niż kryterium
    // z rozmysłu — przesunięcie tempa o pięć procent zapali ten test, zanim zdąży
    // wypchnąć odkrycie poza „9 ± 1".
    assert_eq!(doba, 254, "kalibracja tempa zeszła ze środka przedziału");
}

#[test]
fn ten_sam_seed_daje_te_sama_dobe_odkrycia() {
    let data = drzewo(2050, 1990);
    let (mut a, _) = firmy(1);
    let (mut b, _) = firmy(1);
    assert_eq!(przebieg(&mut a, &data, 400), przebieg(&mut b, &data, 400));
}

#[test]
fn odkrycie_przed_rokiem_swiatowym_daje_patent() {
    let data = drzewo(2050, 1990);
    let (mut f, klucze) = firmy(1);
    przebieg(&mut f, &data, 400).expect("odkrycie");
    let p = f.rnd().patents.get(&TechId(0)).expect("patent");
    assert_eq!(p.owner, klucze[0]);
    // Dwadzieścia lat gry po 360 dób po 1440 minut.
    assert_eq!(p.expires.0 - p.granted.0, 20 * 360 * 1_440);
    // Poziom wyposażenia zakładu podniósł się o `gain` — to jest kanał, którym
    // technologia dociera do jakości wyrobu (M6) i do `MacroFirm.tech` (`FC-2`).
    let s = f
        .site(SiteId(Entity::new(10, NonZeroU32::MIN)))
        .expect("zakład");
    assert_eq!(s.tech.get(), 58);
}

#[test]
fn po_roku_swiatowym_nie_ma_patentu_a_koszt_jest_nizszy() {
    // Rok światowy minął przed startem partii: wiedza jest w obiegu.
    let data = drzewo(1980, 1990);
    let (mut f, _) = firmy(1);
    let doba = przebieg(&mut f, &data, 400).expect("odkrycie");
    assert!(
        f.rnd().patents.is_empty(),
        "technologia znana światu nie może dawać patentu"
    );
    // Koszt obniżony o 60 %, więc i czas: 480 RP zamiast 1200.
    let miesiac = doba / 30 + 1;
    assert!(
        (3..=5).contains(&miesiac),
        "odkrycie w {miesiac}. miesiącu, a przy zniżce 60 % ma wypaść w czwartym"
    );
}

#[test]
fn przelom_skraca_badania_i_zostawia_slad() {
    // Przełom **wymuszony**: dziesięć tysięcy na dziesięć tysięcy, czyli co dobę.
    let mut data = drzewo(2050, 1990);
    let z_przelomem = {
        let t = RndTuning {
            breakthrough_per_10k_day: 10_000,
            ..data.tuning.clone()
        };
        data = RndData::new(data.tree.clone(), t);
        let (mut f, _) = firmy(1);
        przebieg(&mut f, &data, 400).expect("odkrycie")
    };
    let bez = {
        let data = drzewo(2050, 1990);
        let (mut f, _) = firmy(1);
        przebieg(&mut f, &data, 400).expect("odkrycie")
    };
    assert!(
        z_przelomem < bez,
        "przełom nie skrócił badań: {z_przelomem} wobec {bez}"
    );
}

#[test]
fn cudzy_patent_zmusza_do_licencji_a_licencja_jest_kontraktem() {
    let data = drzewo(2050, 1990);
    // Dwie firmy: pierwsza ma cztery lata forsu, druga zaczyna z pustą wiedzą.
    let (mut f, klucze) = firmy(2);
    let roles = role_table();
    let zdrowi = |_: CitizenId| Some((w_formie(), Q::new(60)));
    let mut m = mint();
    let mut podpisana = None;
    for d in 0..800u32 {
        let dzien = RndDay {
            day: d,
            now: SimMinute(u64::from(d) * 1_440),
            tick: Tick(u64::from(d) * 1_440),
            world_seed: SEED,
            roles: &roles,
            researcher: Some(BADACZ),
            // Druga firma ma badaczy bez formy, więc nigdy nie skończy sama —
            // jedyną drogą do technologii jest dla niej licencja. Badacze pierwszej
            // mają indeksy 2000+, drugiej 2100+ (zakład 10 i 11, `zaklad`).
            vitals: &|c: CitizenId| {
                if c.0.index() >= 2_100 {
                    Some((
                        Vitals {
                            energy: 0,
                            mood: -100,
                            health: 1,
                            ..w_formie()
                        },
                        Q::new(1),
                    ))
                } else {
                    zdrowi(c)
                }
            },
        };
        let out = step_day(&mut f, &data, &dzien, &mut m);
        if let Some(l) = out.licensed.first() {
            podpisana = Some(*l);
            break;
        }
    }
    let (licencjobiorca, tech, licencjodawca, contract) =
        podpisana.expect("druga firma nie kupiła licencji");
    assert_eq!(licencjobiorca, klucze[1]);
    assert_eq!(licencjodawca, klucze[0]);
    assert_eq!(tech, TechId(0));
    // Licencja **jest** `ContractId` — numer pochodzi z licznika umów, a nie
    // z własnej numeracji R&D (§5.4 pkt 3).
    let l = f
        .rnd()
        .licenses
        .get(&contract.entity().index())
        .expect("wpis licencji");
    assert_eq!(l.contract, contract);
    // Stawka mieści się w widełkach i jest wysoka, bo patent jest świeży.
    let (lo, hi) = RndTuning::default().royalty_bp;
    assert!((lo..=hi).contains(&l.royalty_bp), "stawka {}", l.royalty_bp);
    assert!(l.royalty_bp > (lo + hi) / 2);
    // Licencjobiorca **umie** technologię, choć jej nie odkrył.
    assert!(f.rnd().knows(klucze[1], TechId(0)));
    assert!(f
        .rnd()
        .patents
        .get(&TechId(0))
        .is_some_and(|p| p.owner == klucze[0]));
}

#[test]
fn licencjobiorca_placi_royalty_od_utargu_i_placi_je_bez_badaczy() {
    // **To jest test na to, że opłata licencyjna w ogóle powstaje.** Stawka zapisana
    // w umowie i nigdy nienaliczona byłaby polem w danych, a nie mechaniką — i pierwsza
    // wersja tej podfazy miała dokładnie taki błąd: `ChargeKind::Royalty` istniał
    // i nie miał producenta.
    let data = drzewo(2050, 1990);
    let (mut f, _) = firmy(2);
    let roles = role_table();
    let mut m = mint();
    let slaby = |c: CitizenId| {
        if c.0.index() >= 2_100 {
            Some((
                Vitals {
                    energy: 0,
                    mood: -100,
                    health: 1,
                    ..w_formie()
                },
                Q::new(1),
            ))
        } else {
            Some((w_formie(), Q::new(60)))
        }
    };
    let mut contract = None;
    for d in 0..800u32 {
        let dzien = RndDay {
            day: d,
            now: SimMinute(u64::from(d) * 1_440),
            tick: Tick(u64::from(d) * 1_440),
            world_seed: SEED,
            roles: &roles,
            researcher: Some(BADACZ),
            vitals: &slaby,
        };
        let out = step_day(&mut f, &data, &dzien, &mut m);
        if let Some(l) = out.licensed.first() {
            contract = Some(*l);
            break;
        }
    }
    let (licencjobiorca, _, licencjodawca, _) = contract.expect("brak licencji");

    // Licencjobiorca zamyka miesiąc z utargiem — dopiero wtedy jest od czego liczyć.
    let site = f
        .firm(licencjobiorca)
        .and_then(|x| x.sites.first().copied())
        .expect("zakład licencjobiorcy");
    f.site_mut(site).expect("zakład").pnl.push(SitePnlMonth {
        month: 1,
        revenue: Money(10_000_000),
        ..SitePnlMonth::default()
    });
    // **Badaczy zabieramy w całości**: licencjobiorca nie prowadzi badań i właśnie
    // dlatego kupił licencję. Gdyby royalty naliczało się w pętli firm badawczych,
    // ten przypadek nie zapłaciłby nigdy.
    f.site_mut(site).expect("zakład").positions.clear();

    let stawka = f
        .rnd()
        .licenses
        .values()
        .find(|l| l.licensee == licencjobiorca)
        .map(|l| l.royalty_bp)
        .expect("wpis licencji");

    let dzien = RndDay {
        day: 810,
        now: SimMinute(810 * 1_440),
        tick: Tick(810 * 1_440),
        world_seed: SEED,
        roles: &roles,
        researcher: Some(BADACZ),
        vitals: &slaby,
    };
    let out = step_day(&mut f, &data, &dzien, &mut m);
    let oplata = out
        .charges
        .iter()
        .find(|c| matches!(c.kind, ChargeKind::Royalty { .. }))
        .expect("royalty nie zostało naliczone");
    assert_eq!(oplata.firm, licencjobiorca);
    assert_eq!(
        oplata.amount,
        Money(10_000_000 * i64::from(stawka) / 10_000)
    );
    let ChargeKind::Royalty { licensor, .. } = oplata.kind else {
        unreachable!()
    };
    assert_eq!(licensor, licencjodawca);
}
