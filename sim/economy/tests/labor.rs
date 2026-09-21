//! Rynek pracy M7b: licytacja płac, zbieżność bezrobocia, kadry (M7 §7.1, WP4–WP6).
//!
//! Testy stoją na **własnym porcie do mieszkańców**, a nie na mieście z generatora.
//! Powód jest ten sam, dla którego M5b buduje dwa sklepy zamiast metropolii: kryterium
//! §7.1 mówi „trzy zakłady po osiem etatów spawacza, dwudziestu spawaczy w mieście",
//! a to jest zdanie o rynku pracy, nie o generatorze. Świat, który da się policzyć
//! na kartce, mówi, **co** pękło.

use std::collections::BTreeMap;

use magnat_agents::{ShiftKind, Vitals};
use magnat_core::{
    hash::StateHasher, CitizenId, DecisionReason, DistrictId, Entity, HashState, JobRoleId,
    LeaveCause, Money, NeedKind, SimMinute, SiteId, Tick, WageCause, Q,
};
use magnat_economy::labor::{LaborDay, LaborMarket, PersonFacts, Workforce};
use magnat_firms::{
    Firm, FirmKey, Firms, LaborTuning, Owner, Position, Ring, RoleTable, Site, SitePnlMonth,
    SiteTypeId,
};
use std::num::NonZeroU32;

// ── świat testowy ────────────────────────────────────────────────────────────────

const SPAWACZ: JobRoleId = JobRoleId(0);
const KASJER: JobRoleId = JobRoleId(1);

/// Widełki spawacza i kasjera. Szerokie, żeby licytacja miała dokąd iść —
/// w kryterium §7.1 mediana ma urosnąć o 15 %, a sufit jest twardy.
const WIDELKI_SPAWACZ: (Money, Money) = (Money(400_000), Money(800_000));
const WIDELKI_KASJER: (Money, Money) = (Money(300_000), Money(420_000));

fn ent(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::MIN)
}

fn zdrowy() -> Vitals {
    Vitals {
        health: 90,
        energy: 90,
        mood: 40,
        stress: 20,
        edu_level: 3,
        edu_field: 0,
        status: 50,
        _pad: 0,
    }
}

#[derive(Clone)]
struct Osoba {
    vitals: Vitals,
    district: DistrictId,
    ambition: Q,
    loyalty: Q,
    job: Option<(SiteId, JobRoleId)>,
    skills: BTreeMap<JobRoleId, u8>,
    alive: bool,
}

/// Port testowy: miasto jako mapa ludzi. Druga implementacja [`Workforce`] obok
/// produkcyjnej nad ECS-em — i to ona uzasadnia, że ten trait w ogóle istnieje.
#[derive(Default)]
struct TestPeople {
    ludzie: BTreeMap<CitizenId, Osoba>,
    /// Ile razy etat wrócił do puli. Kryterium M3c: **każde** wyjście z rynku pracy
    /// przechodzi tą jedną drogą.
    releases: u32,
    /// Ile punktów potrzeby dołożyły świadczenia — wprost kryterium WP6.
    need_gain: BTreeMap<(CitizenId, NeedKind), u32>,
    hires: u32,
    /// Grafik każdego zatrudnienia — `(zmiana, maska dni)` (`R2-WP37`).
    grafiki: Vec<(ShiftKind, u8)>,
}

impl TestPeople {
    fn dodaj(&mut self, i: u32, role: Option<JobRoleId>, skill: u8, district: u16) -> CitizenId {
        let c = CitizenId(ent(i));
        let mut skills = BTreeMap::new();
        if let Some(r) = role {
            skills.insert(r, skill);
        }
        self.ludzie.insert(
            c,
            Osoba {
                vitals: zdrowy(),
                district: DistrictId(district),
                ambition: Q::new(50),
                loyalty: Q::new(50),
                job: None,
                skills,
                alive: true,
            },
        );
        c
    }
}

impl Workforce for TestPeople {
    fn facts(&self, c: CitizenId) -> Option<PersonFacts> {
        let o = self.ludzie.get(&c).filter(|o| o.alive)?;
        Some(PersonFacts {
            vitals: o.vitals,
            district: o.district,
            ambition: o.ambition,
            loyalty: o.loyalty,
            job: o.job,
            best_role: o
                .skills
                .iter()
                .max_by_key(|(_, lvl)| **lvl)
                .map(|(r, _)| *r),
            on_sick_leave: false,
        })
    }

    fn skill_in(&self, c: CitizenId, role: JobRoleId) -> Q {
        Q::new(
            self.ludzie
                .get(&c)
                .and_then(|o| o.skills.get(&role).copied())
                .unwrap_or(0),
        )
    }

    fn job_seekers(&mut self, day: u32, on_the_job_every: u16, out: &mut Vec<CitizenId>) {
        out.clear();
        let co_ile = u32::from(on_the_job_every.max(1));
        for (c, o) in &self.ludzie {
            if !o.alive {
                continue;
            }
            if o.job.is_some() && c.0.index() % co_ile != day % co_ile {
                continue;
            }
            out.push(*c);
        }
    }

    fn hire(
        &mut self,
        c: CitizenId,
        site: SiteId,
        role: JobRoleId,
        shift: ShiftKind,
        work_days: u8,
        _wage: Money,
    ) {
        if let Some(o) = self.ludzie.get_mut(&c) {
            o.job = Some((site, role));
            o.skills.entry(role).or_insert(30);
        }
        self.grafiki.push((shift, work_days));
        self.hires += 1;
    }

    fn release(&mut self, c: CitizenId, _wage: Money) {
        if let Some(o) = self.ludzie.get_mut(&c) {
            o.job = None;
        }
        self.releases += 1;
    }

    fn raise_wage(&mut self, _c: CitizenId, _from: Money, _to: Money) {}

    fn add_need(&mut self, c: CitizenId, need: NeedKind, points: u8) {
        *self.need_gain.entry((c, need)).or_default() += u32::from(points);
    }

    fn set_skill(&mut self, c: CitizenId, role: JobRoleId, level: Q) {
        if let Some(o) = self.ludzie.get_mut(&c) {
            o.skills.insert(role, level.get());
        }
    }

    fn labour_force(&mut self, _day: u32) -> (u32, u32) {
        let mut sila = 0;
        let mut bez = 0;
        for o in self.ludzie.values() {
            if !o.alive {
                continue;
            }
            sila += 1;
            if o.job.is_none() {
                bez += 1;
            }
        }
        (sila, bez)
    }
}

/// Katalog ról na potrzeby testu: dwa zawody, wagi z prawdziwego pliku byłyby tu
/// szumem. Kolejność jest kontraktem tylko wobec zapisu gry, a tego test nie tworzy.
fn role() -> RoleTable {
    RoleTable::parse(
        r#"(
            schema_version: 2,
            roles: [
                ( key: "welder",  weights: (skill: 500, energy: 250, mood: 100, health: 150) ),
                ( key: "cashier", weights: (skill: 250, energy: 300, mood: 250, health: 200) ),
            ],
        )"#,
    )
    .expect("katalog ról testu")
}

fn tuning() -> LaborTuning {
    LaborTuning::load_default().expect("data/tuning/labor.ron")
}

fn rynek() -> LaborMarket {
    LaborMarket::new(tuning(), role())
}

/// Zakład z listą stanowisk. Pola `Site` są publiczne, więc test nie potrzebuje
/// katalogu typów zakładów — a katalog i tak by tu niczego nie sprawdził.
fn zaklad(
    firms: &mut Firms,
    firm: FirmKey,
    i: u32,
    district: u16,
    stanowiska: &[(JobRoleId, u16, (Money, Money))],
) -> SiteId {
    let id = SiteId(ent(1_000 + i));
    let site = Site {
        id,
        firm,
        site_type: SiteTypeId(0),
        building: magnat_core::BuildingId(ent(i)),
        district: DistrictId(district),
        floor_m2: 400,
        positions: stanowiska
            .iter()
            .map(|(r, n, band)| Position::new(*r, *n, false, *band))
            .collect(),
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
    assert!(firms.add_site(site), "zakład bez firmy");
    id
}

fn firma(firms: &mut Firms, nazwa: &str) -> FirmKey {
    firms.insert(|key| {
        Firm::sole_owner(
            key,
            nazwa.to_owned(),
            SimMinute(0),
            DistrictId(0),
            Owner::External,
        )
    })
}

/// Przesuwa rynek o `dni` dób i zwraca podsumowanie ostatniej z nich.
fn przebieg(
    m: &mut LaborMarket,
    firms: &mut Firms,
    people: &mut TestPeople,
    od: u32,
    dni: u32,
) -> LaborDay {
    let mut ostatni = LaborDay::default();
    for d in od..od + dni {
        ostatni = m.step_day(firms, people, 7, Tick(u64::from(d) * 1440));
    }
    ostatni
}

// ── §7.1: płace reagują na niedobór zawodu ───────────────────────────────────────

/// Scenariusz z §7.1 dokumentu fazy: trzy zakłady po osiem etatów spawacza (24 wakaty),
/// w mieście dwudziestu spawaczy, z czego szesnastu już pracuje gdzie indziej.
/// Zawód kontrolny — kasjer — jest w równowadze.
fn miasto_spawaczy() -> (LaborMarket, Firms, TestPeople) {
    let mut firms = Firms::new();
    let mut people = TestPeople::default();

    let huta = firma(&mut firms, "Huta");
    for i in 0..3 {
        zaklad(&mut firms, huta, i, 0, &[(SPAWACZ, 8, WIDELKI_SPAWACZ)]);
    }
    // Szesnastu spawaczy pracuje u konkurencji: cztery zakłady po cztery etaty.
    let warsztat = firma(&mut firms, "Warsztat");
    for i in 0..4 {
        zaklad(
            &mut firms,
            warsztat,
            10 + i,
            0,
            &[(SPAWACZ, 4, WIDELKI_SPAWACZ)],
        );
    }
    // Zawód kontrolny: tyle samo etatów co ludzi, więc bez niedoboru.
    let sklep = firma(&mut firms, "Sklep");
    for i in 0..5 {
        zaklad(&mut firms, sklep, 20 + i, 0, &[(KASJER, 4, WIDELKI_KASJER)]);
    }

    for i in 0..20 {
        people.dodaj(i, Some(SPAWACZ), 60, 0);
    }
    for i in 0..20 {
        people.dodaj(100 + i, Some(KASJER), 55, 0);
    }
    (rynek(), firms, people)
}

#[test]
fn wage_reacts_to_shortage() {
    let (mut m, mut firms, mut people) = miasto_spawaczy();

    // Pierwsze dwie doby: rynek się zapełnia po stawkach startowych.
    przebieg(&mut m, &mut firms, &mut people, 0, 2);
    let start_spawacz = m
        .stats()
        .median_accepted(SPAWACZ, DistrictId(0))
        .expect("ktoś został zatrudniony");
    let start_kasjer = m
        .stats()
        .median_accepted(KASJER, DistrictId(0))
        .expect("kasjerzy też");

    przebieg(&mut m, &mut firms, &mut people, 2, 58);

    // 1. Mediana **zaakceptowanej** płacy spawacza rośnie o ≥ 15 % w 60 dób.
    let po_spawacz = m
        .stats()
        .median_accepted(SPAWACZ, DistrictId(0))
        .expect("mediana spawacza");
    let wzrost = (po_spawacz.get() - start_spawacz.get()) * 100 / start_spawacz.get();
    assert!(
        wzrost >= 15,
        "mediana spawacza {} → {} ({wzrost} %)",
        start_spawacz.get(),
        po_spawacz.get()
    );

    // 2. Zawód kontrolny zmienia się o < 3 %: podwyżka jest lokalna dla zawodu,
    //    a nie inflacją całego rynku pracy.
    let po_kasjer = m
        .stats()
        .median_accepted(KASJER, DistrictId(0))
        .expect("mediana kasjera");
    let zmiana = ((po_kasjer.get() - start_kasjer.get()) * 100 / start_kasjer.get()).abs();
    assert!(
        zmiana < 3,
        "kasjer {} → {} ({zmiana} %)",
        start_kasjer.get(),
        po_kasjer.get()
    );

    // 3. Po wstrzyknięciu trzydziestu spawaczy niedobór spada, a płace **stabilizują
    //    się, a nie walą w dół**: istniejące umowy nie są cięte (lepkość w dół).
    let niedobor_przed = m.shortage_index(SPAWACZ, DistrictId(0));
    let umowy_przed = umowy(&firms, SPAWACZ);
    for i in 0..30 {
        people.dodaj(200 + i, Some(SPAWACZ), 60, 0);
    }
    przebieg(&mut m, &mut firms, &mut people, 60, 30);
    assert!(
        m.shortage_index(SPAWACZ, DistrictId(0)) < niedobor_przed,
        "migracja nie obniżyła niedoboru: {niedobor_przed} → {}",
        m.shortage_index(SPAWACZ, DistrictId(0))
    );
    // Lepkość w dół dotyczy **umowy konkretnego człowieka**, a nie najniższej stawki
    // w mieście: trzydziestu nowych zatrudnionych poniżej mediany to jest właśnie
    // to, co ma się stać, i nie jest obcięciem niczyjej pensji.
    let umowy_po = umowy(&firms, SPAWACZ);
    for (c, przed) in &umowy_przed {
        if let Some(po) = umowy_po.get(c) {
            assert!(
                po.get() >= przed.get(),
                "obcięto stawkę pracownikowi {c:?}: {} → {}",
                przed.get(),
                po.get()
            );
        }
    }
    assert!(
        umowy_po.len() > umowy_przed.len(),
        "trzydziestu spawaczy weszło na rynek i nikt ich nie zatrudnił"
    );
}

#[test]
fn headhunting_wchodzi_dopiero_przy_niedoborze() {
    // Przeciąganie pracownika jest kosztowne i ma być rzadkie: oferta bezpośrednia
    // powstaje dopiero wtedy, gdy zwykłe ogłoszenie wisi i nikt nie przychodzi.
    let (mut m, mut firms, mut people) = miasto_spawaczy();
    let mut lowcy = 0;
    let mut niedobor_przy_pierwszym = 0;
    for d in 0..90u64 {
        // Decyzja o przeciąganiu zapada na **wczorajszym** indeksie: statystyki doby
        // przelicza się dopiero na jej końcu. Mierzymy więc przed krokiem, a nie po.
        let niedobor = m.shortage_index(SPAWACZ, DistrictId(0));
        let r = m.step_day(&mut firms, &mut people, 7, Tick(d * 1440));
        if r.headhunts > 0 && lowcy == 0 {
            niedobor_przy_pierwszym = niedobor;
        }
        lowcy += r.headhunts;
    }
    assert!(
        lowcy > 0,
        "niedobór spawaczy nie wywołał ani jednej oferty bezpośredniej"
    );
    assert!(
        niedobor_przy_pierwszym >= tuning().wage.headhunt_shortage,
        "oferta bezpośrednia poszła przy niedoborze {niedobor_przy_pierwszym},          czyli poniżej progu"
    );
}

/// Księgowość obsady: etatów nie przybywa, nikt nie stoi na dwóch listach płac,
/// a zakład nie płaci za stanowisko, którego nie ma.
///
/// Test istnieje, bo trzy różne drogi prowadziły do jego złamania: oferta publiczna
/// i bezpośrednia na ten sam wakat, odjęcie od oferty etatów **zaplanowanych** zamiast
/// obsadzonych, i „zatrudnienie" własnego pracownika na jego własne stanowisko.
#[test]
fn obsada_nigdy_nie_przekracza_liczby_etatow() {
    let (mut m, mut firms, mut people) = miasto_spawaczy();
    for d in 0..300u64 {
        m.step_day(&mut firms, &mut people, 7, Tick(d * 1440));
        let mut widziani: BTreeMap<CitizenId, u32> = BTreeMap::new();
        for (id, site) in firms.sites() {
            for p in &site.positions {
                assert!(
                    p.filled.len() <= usize::from(p.slots),
                    "doba {d}: zakład {id:?} ma {} osób na {} etatach",
                    p.filled.len(),
                    p.slots
                );
                for e in &p.filled {
                    *widziani.entry(e.citizen).or_default() += 1;
                }
            }
        }
        for (c, ile) in &widziani {
            assert_eq!(*ile, 1, "doba {d}: {c:?} stoi na {ile} listach płac");
        }
    }
}

#[test]
fn brak_spirali_placowej_w_pieciu_latach() {
    let (mut m, mut firms, mut people) = miasto_spawaczy();
    przebieg(&mut m, &mut firms, &mut people, 0, 5 * 360);
    // §7.1 pkt 4: płaca nie przekracza sufitu wynikającego z widełek stanowiska,
    // więc część wakatów zostaje nieobsadzona — i **to jest poprawny wynik**.
    for w in stawki(&firms, SPAWACZ) {
        assert!(
            w.get() <= WIDELKI_SPAWACZ.1.get(),
            "stawka {} przebiła sufit {}",
            w.get(),
            WIDELKI_SPAWACZ.1.get()
        );
    }
    assert!(
        m.stats().shortage_index(SPAWACZ, DistrictId(0)) > 0,
        "24 etaty na 20 spawaczy, a niedobór zniknął"
    );
}

#[test]
fn kazda_podwyzka_ma_powod_z_przyczyna() {
    let (mut m, mut firms, mut people) = miasto_spawaczy();
    przebieg(&mut m, &mut firms, &mut people, 0, 40);
    let mut podwyzki = 0;
    let mut sufity = 0;
    for (_, f) in firms.iter() {
        for wpis in f.log.iter() {
            if let DecisionReason::WageRaise {
                delta_bp, cause, ..
            } = wpis.reason
            {
                // Każdy wpis niesie przyczynę; przyrost zerowy znaczy sufit i **tylko** sufit.
                if delta_bp == 0 {
                    assert_eq!(cause, WageCause::Ceiling);
                    sufity += 1;
                } else {
                    podwyzki += 1;
                }
            }
        }
    }
    assert!(podwyzki > 0, "w 40 dobach nikt nie podbił stawki");
    let _ = sufity;
}

/// Stawka per pracownik — do sprawdzenia lepkości płac w dół.
fn umowy(firms: &Firms, role: JobRoleId) -> BTreeMap<CitizenId, Money> {
    firms
        .sites()
        .flat_map(|(_, s)| s.positions.iter())
        .filter(|p| p.role == role)
        .flat_map(|p| p.filled.iter())
        .map(|e| (e.citizen, e.wage_month))
        .collect()
}

/// Stawki wszystkich zawartych umów tego zawodu.
fn stawki(firms: &Firms, role: JobRoleId) -> Vec<Money> {
    firms
        .sites()
        .flat_map(|(_, s)| s.positions.iter())
        .filter(|p| p.role == role)
        .flat_map(|p| p.filled.iter())
        .map(|e| e.wage_month)
        .collect()
}

// ── WP4: bezrobocie zbiega do pasma ──────────────────────────────────────────────

/// Miasto z kryterium WP4: 5 tys. mieszkańców i 300 firm.
///
/// Etatów jest **mniej niż ludzi** i to jest cała treść tego testu: rynek pracy,
/// w którym pracy wystarcza dla każdego, nie ma o czym opowiadać. Przy pokryciu
/// 95 % bezrobocie musi wylądować nad progiem strukturalnym 5 % i pod górnym
/// krańcem pasma — czyli tam, gdzie tarcie wyszukiwania jest policzalne.
fn miasto_5k() -> (LaborMarket, Firms, TestPeople) {
    let mut firms = Firms::new();
    let mut people = TestPeople::default();
    let zawody = [SPAWACZ, KASJER];
    let mut etaty = 0u32;
    for i in 0..300u32 {
        let f = firma(&mut firms, "Firma");
        let dzielnica = (i % 3) as u16;
        let role = zawody[(i % 2) as usize];
        let band = if role == SPAWACZ {
            WIDELKI_SPAWACZ
        } else {
            WIDELKI_KASJER
        };
        // 300 zakładów po 15–16 etatów ≈ 4 750, czyli 95 % siły roboczej.
        let n = if i % 2 == 0 { 16 } else { 15 };
        zaklad(&mut firms, f, i, dzielnica, &[(role, n, band)]);
        etaty += u32::from(n);
    }
    assert_eq!(etaty, 4_650, "pokrycie etatowe miasta testowego");
    for i in 0..5_000u32 {
        people.dodaj(i, Some(zawody[(i % 2) as usize]), 50, (i % 3) as u16);
    }
    (rynek(), firms, people)
}

#[test]
fn bezrobocie_zbiega_do_pasma_w_dziewiecdziesiat_dni() {
    let (mut m, mut firms, mut people) = miasto_5k();
    let dzien = przebieg(&mut m, &mut firms, &mut people, 0, 90);
    let stopa = dzien.unemployment_permille();
    assert!(
        (30..=90).contains(&stopa),
        "bezrobocie {} ‰ poza pasmem 30–90 ‰ (zatrudnionych {}, wakatów {})",
        stopa,
        dzien.employed,
        dzien.vacancies
    );
}

#[test]
fn kazde_zatrudnienie_ma_wynik_i_drugiego_w_kolejce() {
    let (mut m, mut firms, mut people) = miasto_spawaczy();
    przebieg(&mut m, &mut firms, &mut people, 0, 10);
    let mut z_konkurencja = 0;
    let mut razem = 0;
    for (_, f) in firms.iter() {
        for wpis in f.log.iter() {
            if let DecisionReason::Hired { runner_up, .. } = wpis.reason {
                razem += 1;
                if runner_up != i32::MIN {
                    z_konkurencja += 1;
                }
            }
        }
    }
    assert!(razem > 0, "nikogo nie zatrudniono");
    assert!(
        z_konkurencja > 0,
        "żadne zatrudnienie nie miało drugiego w kolejce — scoring nie ma czego porównywać"
    );
}

// ── WP6: kadry ───────────────────────────────────────────────────────────────────

#[test]
fn swiadczenie_zdrowotne_podnosi_potrzebe_pracownika() {
    let mut firms = Firms::new();
    let mut people = TestPeople::default();
    let f = firma(&mut firms, "Zakład");
    let site = zaklad(&mut firms, f, 0, 0, &[(SPAWACZ, 1, WIDELKI_SPAWACZ)]);
    let c = people.dodaj(1, Some(SPAWACZ), 60, 0);
    let mut m = rynek();
    przebieg(&mut m, &mut firms, &mut people, 0, 1);
    assert!(people.ludzie[&c].job.is_some(), "kandydat nie dostał pracy");

    // Firma przyznaje opiekę medyczną — w M7b robi to, gdy stawka stoi na suficie,
    // ale test sprawdza **skutek**, nie ścieżkę decyzji.
    let umowa = firms
        .site_mut(site)
        .and_then(|s| s.positions.iter_mut().find(|p| p.role == SPAWACZ))
        .and_then(|p| p.filled.first_mut())
        .expect("umowa");
    umowa.benefits = magnat_firms::BenefitSet(magnat_firms::BenefitSet::HEALTH);

    przebieg(&mut m, &mut firms, &mut people, 1, 5);
    let przyrost = people
        .need_gain
        .get(&(c, NeedKind::Health))
        .copied()
        .unwrap_or(0);
    assert!(
        przyrost > 0,
        "opieka medyczna nie podniosła potrzeby Zdrowie ani o punkt"
    );
    // I nie jest to „liczba dodana do nastroju": nastrój nie drgnął.
    assert_eq!(people.need_gain.get(&(c, NeedKind::Leisure)), None);
}

#[test]
fn odejscie_zawsze_ma_powod_po_stronie_odchodzacego() {
    // Trzy przyczyny z kryterium WP6, każda wymuszona **stanem człowieka**, a nie
    // wpisaniem powodu z ręki. Czwarta — lepsza oferta — ma własny test niżej.
    //
    // `dni` różnią się, bo różnią się skale: zniechęcenie i przeciążenie są ryzykiem
    // dobowym rzędu 20–30 bp (czyli latami), a zwolnienie za wynik wymaga trzech
    // miesięcznych upomnień i zapada co do doby.
    for (vitals, umiejetnosc, dni, oczekiwana) in [
        (kiepski_nastroj(), 60u8, 3_000u64, LeaveCause::Mood),
        (przeciazony(), 60, 3_000, LeaveCause::Stress),
        // Zwolnienie za wynik wymaga pracownika, którego wynik naprawdę jest słaby:
        // umiejętność dokładnie na progu stanowiska i zero formy.
        (bez_sil(), 12, 200, LeaveCause::Dismissed),
    ] {
        let mut firms = Firms::new();
        let mut people = TestPeople::default();
        let f = firma(&mut firms, "Zakład");
        let site = zaklad(&mut firms, f, 0, 0, &[(SPAWACZ, 1, WIDELKI_SPAWACZ)]);
        let c = people.dodaj(1, Some(SPAWACZ), umiejetnosc, 0);
        let mut m = rynek();
        przebieg(&mut m, &mut firms, &mut people, 0, 1);
        assert!(people.ludzie[&c].job.is_some(), "kandydat nie dostał pracy");

        // Staż liczy się od daty umowy, a odejście dobrowolne ma próg stażu.
        firms
            .site_mut(site)
            .and_then(|s| s.positions.iter_mut().find(|p| p.role == SPAWACZ))
            .and_then(|p| p.filled.first_mut())
            .expect("umowa")
            .since = SimMinute(0);
        people.ludzie.get_mut(&c).expect("osoba").vitals = vitals;

        // Zakład jest jedyny, więc odchodzący nie ma dokąd pójść — powód musi być ten,
        // który go wypchnął, a nie „lepsza oferta".
        let mut znaleziony = None;
        for d in 100..100 + dni {
            m.step_day(&mut firms, &mut people, 7, Tick(d * 1440));
            znaleziony =
                firms
                    .iter()
                    .flat_map(|(_, f)| f.log.iter())
                    .find_map(|w| match w.reason {
                        DecisionReason::JobLeft { cause, .. } => Some(cause),
                        _ => None,
                    });
            if znaleziony.is_some() {
                break;
            }
        }
        assert_eq!(
            znaleziony,
            Some(oczekiwana),
            "wymuszone odejście dostało inny powód"
        );
        assert!(people.releases > 0, "etat nie wrócił do puli miasta");
    }
}

fn kiepski_nastroj() -> Vitals {
    Vitals {
        mood: -100,
        ..zdrowy()
    }
}

fn przeciazony() -> Vitals {
    Vitals {
        mood: 40,
        stress: 100,
        ..zdrowy()
    }
}

/// Pracownik, który po prostu nie daje rady: ocena wyniku schodzi mu poniżej progu,
/// po trzech miesiącach ma trzy upomnienia i firma go zwalnia. Nastrój neutralny,
/// żeby odejście dobrowolne nie wyprzedziło zwolnienia.
fn bez_sil() -> Vitals {
    Vitals {
        health: 0,
        energy: 0,
        mood: 0,
        stress: 0,
        ..zdrowy()
    }
}

#[test]
fn szkolenie_podnosi_umiejetnosc_najslabszego() {
    let mut firms = Firms::new();
    let mut people = TestPeople::default();
    let f = firma(&mut firms, "Zakład");
    zaklad(&mut firms, f, 0, 0, &[(SPAWACZ, 3, WIDELKI_SPAWACZ)]);
    for i in 0..3 {
        people.dodaj(i, Some(SPAWACZ), 20 + i as u8 * 10, 0);
    }
    let mut m = rynek();
    // Szkolenie wypada raz na `training_every_months`; rok gry mieści dwa takie terminy.
    let mut szkolen = 0;
    for d in 0..360u64 {
        szkolen += m
            .step_day(&mut firms, &mut people, 7, Tick(d * 1440))
            .trained;
    }
    assert!(
        szkolen > 0,
        "w rok nikt nie przeszedł ani jednego szkolenia"
    );
    let po = people.ludzie[&CitizenId(ent(0))].skills[&SPAWACZ];
    assert!(
        po > 20,
        "najsłabszy pracownik nie podniósł umiejętności: {po}"
    );
    // Koszt szkolenia obciąża zakład, a nie powietrze.
    let koszt: i64 = firms.sites().map(|(_, s)| s.hr_accrued.get()).sum::<i64>()
        + firms
            .sites()
            .flat_map(|(_, s)| s.pnl.iter())
            .map(|p| p.labor.get())
            .sum::<i64>();
    assert!(koszt > 0, "szkolenie nie kosztowało firmy ani grosza");
}

#[test]
fn zmiana_pracy_przechodzi_przez_zwolnienie_etatu() {
    // Dwa zakłady, jeden pracownik: gdy przechodzi, stary etat **musi** wrócić do puli.
    let mut firms = Firms::new();
    let mut people = TestPeople::default();
    let a = firma(&mut firms, "A");
    let b = firma(&mut firms, "B");
    zaklad(
        &mut firms,
        a,
        0,
        0,
        &[(SPAWACZ, 1, (Money(400_000), Money(410_000)))],
    );
    zaklad(
        &mut firms,
        b,
        1,
        0,
        &[(SPAWACZ, 1, (Money(700_000), Money(800_000)))],
    );
    let c = people.dodaj(1, Some(SPAWACZ), 60, 0);
    let mut m = rynek();
    przebieg(&mut m, &mut firms, &mut people, 0, 60);
    assert!(people.releases > 0, "nikt nigdy nie zwolnił etatu");
    let gdzie = people.ludzie[&c].job.expect("pracuje");
    assert_eq!(
        stawki(&firms, SPAWACZ).len(),
        1,
        "ten sam człowiek figuruje na dwóch listach płac"
    );
    let _ = gdzie;
}

#[test]
fn pokrycie_etatowe_idzie_za_obsada() {
    // `K-44`: to jest liczba, przez którą M6 mnoży przepustowość linii. Do M7a była
    // wpisywana **raz**, przy stawianiu miasta, i zamarzała — zakład, z którego odeszła
    // połowa załogi, mielił dalej tyle samo (`AT-2`).
    let mut firms = Firms::new();
    let mut people = TestPeople::default();
    let f = firma(&mut firms, "Młyn");
    let site = zaklad(&mut firms, f, 0, 0, &[(SPAWACZ, 4, WIDELKI_SPAWACZ)]);
    let role = role();

    let pusty = magnat_economy::labor::hr::labor_coverage(&firms, &role, &people);
    assert_eq!(
        pusty,
        vec![(site, 0)],
        "zakład bez ludzi ma pracować na zero"
    );

    for i in 0..4 {
        people.dodaj(i, Some(SPAWACZ), 60, 0);
    }
    let mut m = rynek();
    przebieg(&mut m, &mut firms, &mut people, 0, 2);
    let pelny = magnat_economy::labor::hr::labor_coverage(&firms, &role, &people)[0].1;
    assert!(pelny > 500, "pełna obsada daje pokrycie {pelny} ‰");

    // Połowa załogi znika z miasta — pokrycie musi zjechać, i to **samo**.
    for i in 0..2 {
        people
            .ludzie
            .get_mut(&CitizenId(ent(i)))
            .expect("osoba")
            .alive = false;
    }
    przebieg(&mut m, &mut firms, &mut people, 2, 1);
    let polowa = magnat_economy::labor::hr::labor_coverage(&firms, &role, &people)[0].1;
    assert!(
        polowa < pelny / 2 + pelny / 10,
        "połowa załogi daje pokrycie {polowa} ‰ przy pełnym {pelny} ‰"
    );
}

// ── determinizm ──────────────────────────────────────────────────────────────────

#[test]
fn dwa_przebiegi_tego_samego_ziarna_daja_ten_sam_stan() {
    let hash = |()| {
        let (mut m, mut firms, mut people) = miasto_spawaczy();
        przebieg(&mut m, &mut firms, &mut people, 0, 45);
        let mut h = StateHasher::new();
        m.hash_state(&mut h);
        firms.hash_state(&mut h);
        (h.finish(), people.hires, people.releases)
    };
    assert_eq!(hash(()), hash(()));
}

// ── M7d: upadłość pracodawcy ─────────────────────────────────────────────────────

/// Niezmiennik 4 z M7 §7.2: **każdy `Employment` zakończony dokładnie raz, dokładnie
/// jedna odprawa naliczona**.
///
/// Upadłość jest jedynym zdarzeniem, które rozwiązuje wszystkie umowy zakładu naraz,
/// więc jest też jedynym, przy którym da się zwolnić kogoś dwa razy albo policzyć mu
/// dwie odprawy — a ani jednego, ani drugiego nie widać w saldzie, dopóki ktoś nie
/// policzy ludzi. Ten test ich liczy.
#[test]
fn upadlosc_konczy_kazda_umowe_dokladnie_raz() {
    let mut firms = Firms::new();
    let mut people = TestPeople::default();
    let m = rynek();

    let huta = firma(&mut firms, "Huta");
    let site = zaklad(
        &mut firms,
        huta,
        0,
        0,
        &[(SPAWACZ, 3, WIDELKI_SPAWACZ), (KASJER, 2, WIDELKI_KASJER)],
    );
    // Drugi zakład tej samej firmy — upadłość zakładu nie ma prawa ruszyć cudzej
    // załogi ani załogi sąsiedniej hali.
    let obok = zaklad(&mut firms, huta, 1, 0, &[(SPAWACZ, 2, WIDELKI_SPAWACZ)]);

    for i in 0..5u32 {
        let c = people.dodaj(i, Some(SPAWACZ), 60, 0);
        let (rola, gdzie) = if i < 3 {
            (SPAWACZ, site)
        } else {
            (SPAWACZ, obok)
        };
        people.hire(c, gdzie, rola, ShiftKind::Day, 0b001_1111, Money(500_000));
        firms
            .site_mut(gdzie)
            .expect("zakład")
            .positions
            .iter_mut()
            .find(|p| p.role == rola)
            .expect("stanowisko")
            .filled
            .push(magnat_firms::Employment {
                citizen: c,
                role: rola,
                wage_month: Money(500_000),
                since: SimMinute(0),
                shift: ShiftKind::Day,
                benefits: magnat_firms::BenefitSet(0),
                perf_ema: 500,
                warnings: 0,
            });
    }
    let przed = people.releases;

    let odprawy = magnat_economy::labor::hr::dismiss_all(
        &m.hr_tuning(),
        &mut firms,
        &mut people,
        site,
        SimMinute(400 * 1440),
    );

    assert_eq!(
        odprawy.len(),
        3,
        "odprawa dla każdego z załogi, i tylko dla niej"
    );
    assert_eq!(
        people.releases - przed,
        3,
        "etat wraca do puli dokładnie raz"
    );
    let kto: Vec<CitizenId> = odprawy.iter().map(|(c, _)| *c).collect();
    let mut unikaty = kto.clone();
    unikaty.sort_unstable();
    unikaty.dedup();
    assert_eq!(unikaty.len(), kto.len(), "ktoś dostał dwie odprawy");
    assert!(
        odprawy.iter().all(|(_, m)| m.get() > 0),
        "odprawa po roku pracy nie może być zerowa"
    );

    // Zakład jest pusty, sąsiedni nietknięty.
    let pusty: usize = firms
        .site(site)
        .expect("zakład")
        .positions
        .iter()
        .map(|p| p.filled.len())
        .sum();
    assert_eq!(pusty, 0, "w upadłym zakładzie ktoś został");
    let sasiad: usize = firms
        .site(obok)
        .expect("zakład")
        .positions
        .iter()
        .map(|p| p.filled.len())
        .sum();
    assert_eq!(sasiad, 2, "upadłość ruszyła cudzą załogę");
    for (c, _) in &odprawy {
        assert!(
            people.facts(*c).is_some_and(|f| f.job.is_none()),
            "mieszkaniec nadal ma zapisaną pracę"
        );
    }

    // Powtórne wołanie na pustym zakładzie nie produkuje drugiej odprawy.
    let znowu = magnat_economy::labor::hr::dismiss_all(
        &m.hr_tuning(),
        &mut firms,
        &mut people,
        site,
        SimMinute(400 * 1440),
    );
    assert!(znowu.is_empty(), "druga odprawa dla tej samej załogi");
}

/// `R2-WP37`: rynek pracy obsadza zakład **wg jego profilu zmianowości**, a nie
/// wpisuje wszystkim zmiany dziennej od poniedziałku do piątku.
///
/// Przed naprawą `post_offers` wstawiało do każdej oferty `ShiftKind::Day`, a `hire`
/// do każdego etatu `Employment::WEEKDAYS` — więc huta o ruchu ciągłym po pierwszej
/// rotacji kadrowej przestawała pracować w nocy, a sklep w sobotę.
#[test]
fn zaklad_o_ruchu_ciaglym_obsadza_takze_noc() {
    let mut m = rynek();
    let mut firms = Firms::new();
    let mut people = TestPeople::default();
    let huta = firma(&mut firms, "Huta");
    let site = zaklad(
        &mut firms,
        huta,
        0,
        0,
        &[(SPAWACZ, 8, WIDELKI_SPAWACZ)],
    );
    firms.site_mut(site).expect("zakład").shift_profile =
        magnat_agents::ShiftProfile::Continuous;

    for i in 0..12u32 {
        people.dodaj(i, Some(SPAWACZ), 60, 0);
    }
    for d in 0..40u64 {
        m.step_day(&mut firms, &mut people, 7, Tick(d * 1440));
    }

    assert!(people.hires > 0, "nikogo nie zatrudniono — test mierzyłby własny brak");
    let pory: std::collections::BTreeSet<u8> =
        people.grafiki.iter().map(|(s, _)| *s as u8).collect();
    assert!(
        pory.contains(&(ShiftKind::Night as u8)),
        "huta o ruchu ciągłym nie obsadziła nocy: {:?}",
        people.grafiki
    );
    assert!(
        pory.len() >= 3,
        "cztery brygady, a pór zmian tylko {}: {:?}",
        pory.len(),
        people.grafiki
    );
    let maski: std::collections::BTreeSet<u8> =
        people.grafiki.iter().map(|(_, d)| *d).collect();
    assert!(
        maski.len() >= 2,
        "wszyscy pracują w te same dni: {maski:?}"
    );
}
