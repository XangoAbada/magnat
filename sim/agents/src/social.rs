//! Status, klasy, relacje i plotka (M3c §5.8, WP9).
//!
//! **Klasa nie jest przechowywana.** Jest przedziałem statusu, a status funkcją siedmiu
//! składników liczoną raz w miesiącu. Dzięki temu mobilność społeczna (§5.4) wychodzi
//! sama: mieszkaniec, który awansował i przeprowadził się, zmienia klasę bez żadnej
//! osobnej mechaniki „awansu". To jest ta sama zasada co przy `LandValueBreakdown`
//! z M2 — wynik pokazuje się graczowi **z rozbiciem na czynniki**, a nie jako liczba,
//! która spadła z nieba (00 §7).
//!
//! **Plotka jest jedynym sposobem, w jaki wiedza o miejscu rozchodzi się po mieście**
//! (§5.7). Mieszkaniec zna miejsce, bo w nim był, bo je mija albo bo mu ktoś powiedział;
//! `PlaceProvider::candidates` nie pokaże mu niczego innego. M10 dopisze `KnowledgeKind::Ad`
//! i nie będzie musiał zmienić tu niczego — reklama to czwarte źródło tego samego wpisu.
//!
//! Dwa składniki statusu — prestiż zawodu i wartość adresu — przychodzą z zewnątrz
//! w `CityFacts`. Powód jest ten sam co przy `PlaceTable` i `Vacancies`: `sim/agents`
//! nie zależy od `sim/world`, więc katalog zawodów z `data/jobs/` i wartość gruntu
//! dzielnicy wsypuje ten, kto je zna (Etap 8 w M3d).

use crate::arrayvec::ArrayVec;
use crate::components::{Employment, Identity, KnowledgeRef, Skills, Vitals};
use crate::demography::{self, DemographyTable, Population, StatusWeights};
use crate::household::Household;
use crate::store::{Knowledge, KnowledgeKind, KnowledgeSlab, RelationKind, RelationSlab, SLAB_MAX};
use magnat_core::{rng, Entity, HashState, Money, StateHasher, StreamId, Q};
use magnat_ecs::World;

/// Re-eksport z `places`: kontrakt §6.1 wymienia `KnowledgeView` pod adresem
/// `sim::agents::social`, bo tam jest jego treść — wiedza o miejscach. Typ powstał
/// w M3a razem z `PlaceProvider` i zostaje tam, gdzie go zdefiniowano.
pub use crate::places::KnowledgeView;

/// Ile dób dzieli dwa przebiegi relacji i plotki dla tego samego mieszkańca (§5.8).
pub const SOCIAL_SHARDS: u64 = 7;

use crate::demography::K_GOSSIP;

/// Klasa społeczna — **przedział statusu**, nie pole (§5.8).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum SocialClass {
    Lower = 0,
    Working = 1,
    LowerMiddle = 2,
    UpperMiddle = 3,
    Upper = 4,
    Elite = 5,
}

impl SocialClass {
    /// Wszystkie klasy od najniższej. Kolejność jest kontraktem: indeksuje tablicę
    /// „kto u mnie kupuje" w panelu sklepu (M5e §5.12), tak samo jak `RejectCause`
    /// indeksuje histogram utraconych sprzedaży.
    pub const ALL: [SocialClass; 6] = [
        SocialClass::Lower,
        SocialClass::Working,
        SocialClass::LowerMiddle,
        SocialClass::UpperMiddle,
        SocialClass::Upper,
        SocialClass::Elite,
    ];

    /// Indeks w [`SocialClass::ALL`].
    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Granice z §5.8: 0–15, 16–33, 34–52, 53–72, 73–89, 90–100.
    #[must_use]
    pub const fn of(status: Q) -> SocialClass {
        match status.get() {
            0..=15 => SocialClass::Lower,
            16..=33 => SocialClass::Working,
            34..=52 => SocialClass::LowerMiddle,
            53..=72 => SocialClass::UpperMiddle,
            73..=89 => SocialClass::Upper,
            _ => SocialClass::Elite,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            SocialClass::Lower => "Lower",
            SocialClass::Working => "Working",
            SocialClass::LowerMiddle => "LowerMiddle",
            SocialClass::UpperMiddle => "UpperMiddle",
            SocialClass::Upper => "Upper",
            SocialClass::Elite => "Elite",
        }
    }
}

/// Liczba klas społecznych — rozmiar tablicy „kto u mnie kupuje" (M5e §5.12).
pub const SOCIAL_CLASS_COUNT: usize = SocialClass::ALL.len();

/// To, czego funkcja statusu potrzebuje o mieście, a `sim/agents` nie ma skąd wziąć.
///
/// Puste tablice są dopuszczalne i znaczą „nie wiem" — składnik dostaje wtedy 50,
/// czyli wartość neutralną. Pusty `CityFacts` nie łamie statusu, tylko go spłaszcza;
/// to jest stan M3c, który M3d wypełnia z `data/jobs/` i z `land_value_at` M2.
#[derive(Clone, Debug, Default)]
pub struct CityFacts {
    /// Prestiż zawodu 0..=100, indeksowany `JobRoleId`.
    pub job_prestige: Vec<u8>,
    /// Pozycja dzielnicy 0..=100 (percentyl wartości gruntu), indeksowana `DistrictId`.
    pub district_score: Vec<u8>,
    /// Kwartał, w którym stoi budynek — indeksowane indeksem encji budynku.
    ///
    /// Sąsiedztwo jest **przestrzenne**, nie budynkowe (§5.8: „wspólny budynek/kwartał"),
    /// i to od niego zależy, czy plotka o nowym sklepie zostaje w promieniu kilometra.
    /// Pusta tablica znaczy „kwartał = budynek", czyli sąsiadem jest tylko ten, kto
    /// mieszka pod tym samym adresem — bezpiecznie, ale ciasno. M3d wypełnia ją
    /// kwartałami z M2.
    pub block_of: Vec<u32>,
}

impl CityFacts {
    #[must_use]
    pub fn prestige(&self, role: u16) -> Option<u8> {
        self.job_prestige.get(role as usize).copied()
    }

    #[must_use]
    pub fn district(&self, district: u16) -> u8 {
        self.district_score
            .get(district as usize)
            .copied()
            .unwrap_or(50)
    }

    #[must_use]
    pub fn block(&self, building: u32) -> u32 {
        self.block_of
            .get(building as usize)
            .copied()
            .unwrap_or(building)
    }
}

impl HashState for CityFacts {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.job_prestige.len() as u64);
        h.write(&self.job_prestige);
        h.write_u64(self.district_score.len() as u64);
        h.write(&self.district_score);
        h.write_u64(self.block_of.len() as u64);
        for b in &self.block_of {
            h.write_u32(*b);
        }
    }
}

/// Rozkłady, względem których liczy się percentyle dochodu i majątku.
///
/// Przeliczane raz w miesiącu na posortowanych wektorach. Percentyl, a nie wartość
/// bezwzględna, bo status jest **pozycją w społeczeństwie**: podwojenie wszystkich
/// pensji nie ma nikogo awansować, a inflacja z M5 nie ma przestawiać klas.
#[derive(Clone, Debug, Default)]
pub struct StatusDistribution {
    income: Vec<i64>,
    wealth: Vec<i64>,
}

impl StatusDistribution {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.income.is_empty()
    }

    #[must_use]
    pub fn income_percentile(&self, v: Money) -> u8 {
        percentyl(&self.income, v.get())
    }

    #[must_use]
    pub fn wealth_percentile(&self, v: Money) -> u8 {
        percentyl(&self.wealth, v.get())
    }
}

impl HashState for StatusDistribution {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.income.len() as u64);
        for v in &self.income {
            h.write_i64(*v);
        }
        h.write_u64(self.wealth.len() as u64);
        for v in &self.wealth {
            h.write_i64(*v);
        }
    }
}

/// Pozycja wartości w posortowanym rozkładzie, w skali 0..=100.
fn percentyl(sorted: &[i64], v: i64) -> u8 {
    if sorted.is_empty() {
        return 50;
    }
    let ponizej = sorted.partition_point(|x| *x < v);
    (ponizej * 100 / sorted.len()).min(100) as u8
}

/// Rozbicie statusu na czynniki — to, co widać w karcie inspekcji (00 §7).
///
/// Każdy składnik jest w skali 0..=100 **przed** przemnożeniem przez wagę, żeby dało
/// się powiedzieć „mieszka dobrze, ale zarabia słabo", a nie tylko „ma 47".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StatusBreakdown {
    pub income: u8,
    pub wealth: u8,
    pub education: u8,
    pub occupation: u8,
    pub address: u8,
    pub consumption: u8,
    pub family: u8,
    pub status: u8,
}

/// Wejście funkcji statusu dla jednego mieszkańca.
#[derive(Clone, Copy, Debug)]
pub struct StatusInput {
    /// Dochód gospodarstwa **na osobę** — bez tego rodzina wielodzietna wyglądałaby
    /// zamożniej niż singiel o tej samej pensji.
    pub income_per_capita: Money,
    pub wealth: Money,
    /// `EduLevel` 0..=4.
    pub edu_level: u8,
    pub employment: Employment,
    pub skill_in_role: Q,
    pub district: u16,
    /// Średni status rodziców albo `None`, gdy nie żyją lub nie są znani.
    pub parents_status: Option<u8>,
    pub age_years: i32,
}

/// Funkcja statusu z §5.8. Czysta — te same wejścia dają tę samą liczbę.
#[must_use]
pub fn status_of(
    input: &StatusInput,
    dist: &StatusDistribution,
    facts: &CityFacts,
    w: StatusWeights,
) -> StatusBreakdown {
    let income = dist.income_percentile(input.income_per_capita);
    let wealth = dist.wealth_percentile(input.wealth);
    let education = input.edu_level.min(4) * 25;
    let occupation = prestiz(input, facts);
    let address = facts.district(input.district);
    // M3 nie ma konsumpcji statusowej — waga jest w danych zerowa, a wartość neutralna.
    let consumption = 50;
    let family = reputacja_rodziny(input);

    let suma = u32::from(income) * u32::from(w.income)
        + u32::from(wealth) * u32::from(w.wealth)
        + u32::from(education) * u32::from(w.education)
        + u32::from(occupation) * u32::from(w.occupation)
        + u32::from(address) * u32::from(w.address)
        + u32::from(consumption) * u32::from(w.consumption)
        + u32::from(family) * u32::from(w.family);

    StatusBreakdown {
        income,
        wealth,
        education,
        occupation,
        address,
        consumption,
        family,
        status: (suma / 100).min(100) as u8,
    }
}

/// Prestiż zawodu.
///
/// `ponytail:` gdy `CityFacts` nie zna prestiżu roli, liczy się go z umiejętności —
/// bezrobotny 10, emeryt 40, uczeń 45, pracujący 40 + połowa poziomu w swojej roli.
/// Sufit nazwany: to jest przybliżenie prestiżu przez kompetencję, więc nie odróżni
/// wykwalifikowanego rzemieślnika od lekarza. Znika, gdy Etap 8 (M3d) wsypie tabelę
/// z `data/jobs/`, i wtedy ta gałąź przestaje się wykonywać.
fn prestiz(input: &StatusInput, facts: &CityFacts) -> u8 {
    let f = input.employment.flags;
    // Uczeń **przed** tabelą prestiżu ról (`R2-WP1`). Uczeń z generacji ma w `role`
    // rolę odziedziczoną po losowaniu cech, więc pod `has_job()` dostawał prestiż
    // zawodu, którego nie wykonuje — a gałąź „uczeń 45" była pod spodem i nigdy
    // się nie wykonywała.
    if input.employment.is_pupil() {
        return 45;
    }
    if input.employment.is_employed() {
        if let Some(p) = facts.prestige(input.employment.role) {
            return p;
        }
    }
    if f & Employment::FLAG_RETIRED != 0 {
        return 40;
    }
    if !input.employment.is_employed() {
        return 10;
    }
    (40 + input.skill_in_role.get() / 2).min(100)
}

/// Reputacja rodziny — średnia statusu rodziców, **zanikająca z wiekiem** (§5.8).
/// Pełna do 18 lat, zerowa od 40; potem liczy się już tylko to, co mieszkaniec zrobił sam.
fn reputacja_rodziny(input: &StatusInput) -> u8 {
    let Some(rodzice) = input.parents_status else {
        return 50;
    };
    let waga = 40 - input.age_years.clamp(18, 40); // 22 → 0
    let wynik = 50 + (i32::from(rodzice) - 50) * waga / 22;
    wynik.clamp(0, 100) as u8
}

// ── miesiąc: przeliczenie statusu ───────────────────────────────────────────────

/// Bilans miesiąca społecznego.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StatusReport {
    pub recomputed: u32,
    pub mean_status: u8,
    pub by_class: [u32; 6],
}

/// Przelicza rozkłady i status wszystkich mieszkańców (§5.8, `EveryMonth`).
///
/// Bez shardowania 1/30 z §5.12: status stoi na **percentylu**, a percentyl liczy się
/// z całego rozkładu naraz. Przeliczanie co trzydziestej osoby dziennie znaczyłoby,
/// że dwie osoby o identycznym dochodzie mają różny status przez cały miesiąc, bo
/// mierzono je względem dwóch różnych rozkładów. Rozkład i tak trzeba zbudować w całości,
/// a sam przebieg po spisie jest przy tym kosztem pomijalnym (korekta G-4).
pub fn step_month(world: &mut World, day: u64) -> StatusReport {
    let mut raport = StatusReport::default();
    let wagi = world.resource::<DemographyTable>().status_weights();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    if spis.is_empty() {
        return raport;
    }

    // 1. Rozkłady. Sortowanie jest deterministyczne, bo wejście idzie po spisie
    // posortowanym po indeksie encji, a `sort_unstable` na `i64` nie ma remisów
    // do rozstrzygnięcia.
    let mut dochody: Vec<i64> = Vec::with_capacity(spis.len());
    let mut majatki: Vec<i64> = Vec::with_capacity(spis.len());
    for e in &spis {
        let (d, m) = zasoby_gospodarstwa(world, *e);
        dochody.push(d.get());
        majatki.push(m.get());
    }
    dochody.sort_unstable();
    majatki.sort_unstable();
    *world.resource_mut::<StatusDistribution>() = StatusDistribution {
        income: dochody,
        wealth: majatki,
    };

    // 2. Status. Rodzice czytają się z **poprzedniego** miesiąca, bo inaczej wynik
    // zależałby od kolejności przeliczania, czyli od indeksów encji w rodzinie.
    let mut nowe: Vec<(Entity, u8)> = Vec::with_capacity(spis.len());
    let mut suma = 0u64;
    for e in &spis {
        let Some(id) = world.get::<Identity>(*e).copied() else {
            continue;
        };
        let (dochod, majatek) = zasoby_gospodarstwa(world, *e);
        let emp = world.get::<Employment>(*e).copied().unwrap_or_default();
        let wej = StatusInput {
            income_per_capita: dochod,
            wealth: majatek,
            edu_level: world.get::<Vitals>(*e).map_or(0, |v| v.edu_level),
            employment: emp,
            skill_in_role: world
                .get::<Skills>(*e)
                .map_or(Q::MIN, |s| s.level_in(emp.role)),
            district: world
                .get::<Household>(*e)
                .map_or(id.birth_district, |h| h.district),
            parents_status: status_rodzicow(world, *e),
            age_years: id.age_years(day as i32),
        };
        let b = status_of(
            &wej,
            world.resource::<StatusDistribution>(),
            world.resource::<CityFacts>(),
            wagi,
        );
        suma += u64::from(b.status);
        raport.by_class[SocialClass::of(Q::new(b.status)) as usize] += 1;
        nowe.push((*e, b.status));
    }
    for (e, s) in &nowe {
        if let Some(v) = world.get_mut::<Vitals>(*e) {
            v.status = *s;
        }
    }
    raport.recomputed = nowe.len() as u32;
    raport.mean_status = suma
        .checked_div(nowe.len() as u64)
        .map_or(0, |x| x.min(100) as u8);
    raport
}

/// Dochód na osobę i majątek gospodarstwa mieszkańca.
fn zasoby_gospodarstwa(world: &World, e: Entity) -> (Money, Money) {
    let Some(hh) = world
        .get::<Identity>(e)
        .and_then(|id| demography::household_by_index(world, id.household))
        .and_then(|h| world.get::<Household>(h))
    else {
        return (Money::ZERO, Money::ZERO);
    };
    let osob = i64::from(hh.size.max(1));
    let dochod = Money(hh.income_monthly.get() / osob);
    let majatek = Money(
        hh.cash
            .get()
            .saturating_add(hh.bank.get())
            .saturating_add(hh.savings.get()),
    );
    (dochod, majatek)
}

/// Średni status rodziców z grafu relacji.
fn status_rodzicow(world: &World, e: Entity) -> Option<u8> {
    let rel = world.get::<crate::components::RelationsRef>(e)?;
    let wpisy = world
        .resource::<crate::store::RelationSlab>()
        .entries(demography::relations_ref(rel));
    let mut suma = 0u32;
    let mut n = 0u32;
    for r in wpisy {
        if r.kind != RelationKind::Parent as u8 {
            continue;
        }
        let Some(p) = demography::citizen_by_index(world, r.other) else {
            continue;
        };
        if let Some(v) = world.get::<Vitals>(p) {
            suma += u32::from(v.status);
            n += 1;
        }
    }
    (n > 0).then(|| (suma / n).min(100) as u8)
}

// ── indeks kontaktów ────────────────────────────────────────────────────────────

/// Kto z kim pracuje i kto z kim mieszka — dwie posortowane listy par.
///
/// Odbudowywana raz w miesiącu, bo praca i adres zmieniają się właśnie w tym rytmie
/// (migracja, ślub, emerytura). Codzienne przeszukiwanie spisu kosztowałoby przy
/// 400 tys. mieszkańców tyle, ile cała reszta systemu społecznego razem wzięta.
#[derive(Clone, Debug, Default)]
pub struct SocialIndex {
    by_site: Vec<(u32, u32)>,
    by_school: Vec<(u32, u32)>,
    by_building: Vec<(u32, u32)>,
}

impl SocialIndex {
    #[must_use]
    pub fn new() -> SocialIndex {
        SocialIndex::default()
    }

    pub fn rebuild(&mut self, world: &World) {
        self.by_site.clear();
        self.by_school.clear();
        self.by_building.clear();
        for e in world.resource::<Population>().citizens() {
            if let Some(emp) = world.get::<Employment>(*e) {
                // **Uczeń nie jest pracownikiem** (`R2-WP1`). Do M8d szkoła siedziała
                // w tym samym indeksie co zakład, więc `coworkers(szkoła)` zwracało
                // całą klasę — a `M10e` §5.9 liczy warunek powstania związku zawodowego
                // na spójnej składowej grafu relacji **wśród pracowników zakładu**.
                // Pierwszą kandydatką do uzwiązkowienia była szkoła podstawowa.
                if emp.is_employed() {
                    self.by_site.push((emp.site, e.index()));
                } else if emp.is_pupil() && emp.has_job() {
                    self.by_school.push((emp.site, e.index()));
                }
            }
            if let Some(res) = world.get::<crate::components::Residence>(*e) {
                if res.has_home() {
                    let kwartal = world.resource::<CityFacts>().block(res.building);
                    self.by_building.push((kwartal, e.index()));
                }
            }
        }
        self.by_site.sort_unstable();
        self.by_school.sort_unstable();
        self.by_building.sort_unstable();
    }

    fn grupa(pary: &[(u32, u32)], klucz: u32) -> &[(u32, u32)] {
        let od = pary.partition_point(|(k, _)| *k < klucz);
        let do_ = pary.partition_point(|(k, _)| *k <= klucz);
        &pary[od..do_]
    }

    #[must_use]
    pub fn coworkers(&self, site: u32) -> &[(u32, u32)] {
        SocialIndex::grupa(&self.by_site, site)
    }

    /// Koledzy z klasy. Osobne wejście od `coworkers`, bo osobny indeks — i to jest
    /// cała treść `D-N7`: relacja jest `Acquaintance`, a nie nowy wariant `Classmate`.
    #[must_use]
    pub fn classmates(&self, school: u32) -> &[(u32, u32)] {
        SocialIndex::grupa(&self.by_school, school)
    }

    /// Sąsiedzi z **kwartału**, nie z budynku — klucz pochodzi z `CityFacts::block`.
    #[must_use]
    pub fn neighbours(&self, block: u32) -> &[(u32, u32)] {
        SocialIndex::grupa(&self.by_building, block)
    }
}

impl HashState for SocialIndex {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.by_site.len() as u64);
        for (a, b) in &self.by_site {
            h.write_u32(*a);
            h.write_u32(*b);
        }
        h.write_u64(self.by_school.len() as u64);
        for (a, b) in &self.by_school {
            h.write_u32(*a);
            h.write_u32(*b);
        }
        h.write_u64(self.by_building.len() as u64);
        for (a, b) in &self.by_building {
            h.write_u32(*a);
            h.write_u32(*b);
        }
    }
}

// ── doba: kontakty, zanik, plotka ───────────────────────────────────────────────

/// Bilans doby społecznej.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SocialReport {
    pub contacts: u32,
    pub decayed: u32,
    pub dropped: u32,
    pub gossip_told: u32,
}

/// Jedna doba relacji i plotki (§5.8). Shard 1/7 po indeksie encji — każdy mieszkaniec
/// przechodzi to raz w tygodniu, więc „−1 za każdy pełny tydzień bez kontaktu" jest
/// dosłownie jednym odjęciem na przebieg.
pub fn step_day(world: &mut World, day: u64) -> SocialReport {
    let mut raport = SocialReport::default();
    let shard = day % SOCIAL_SHARDS;
    let dzisiaj: Vec<Entity> = world
        .resource::<Population>()
        .citizens()
        .iter()
        .copied()
        .filter(|e| u64::from(e.index()) % SOCIAL_SHARDS == shard)
        .collect();
    if dzisiaj.is_empty() {
        return raport;
    }
    let params = world.resource::<DemographyTable>().social();

    for e in dzisiaj {
        kontakty(world, e, day, &params, &mut raport);
        zanik(world, e, day, &params, &mut raport);
        plotka(world, e, day, &params, &mut raport);
    }
    raport
}

fn kontakty(
    world: &mut World,
    e: Entity,
    day: u64,
    params: &crate::demography::table::SocialParams,
    raport: &mut SocialReport,
) {
    let emp = world.get::<Employment>(e).copied();
    let site = emp.filter(Employment::is_employed).map(|x| x.site);
    let szkola = emp.filter(|x| x.is_pupil() && x.has_job()).map(|x| x.site);
    let building = world
        .get::<crate::components::Residence>(e)
        .filter(|r| r.has_home())
        .map(|r| r.building);

    // Bufory na stosie, nie na stercie: ten kod biegnie dla 1/7 populacji **co dobę**,
    // czyli 57 tys. razy na dobę gry przy 400 tys. mieszkańców. Każda alokacja jest
    // tu mnożona przez tę liczbę, a limity i tak są twarde (32 relacje, 32 wpisy wiedzy).
    let mut nowe: ArrayVec<(u32, u8, u8, u8), 16> = ArrayVec::new();
    if let Some(s) = site {
        let grupa = world.resource::<SocialIndex>().coworkers(s);
        for kto in bliscy(grupa, e.index(), params.close_contacts)
            .as_slice()
            .iter()
            .copied()
        {
            nowe.push((
                kto,
                RelationKind::Colleague as u8,
                params.coworker_gain_per_week,
                params.coworker_max,
            ));
        }
    }
    // Klasa daje `Acquaintance`, nie `Colleague` (`D-N7`): osobny wariant kosztowałby
    // miejsce w enumie, który jest kontraktem zapisu gry, i nie miałby konsumenta —
    // nikt nie pyta, czy znajomy jest kolegą z klasy. Wagi są te same co w zakładzie,
    // bo klasa jest tym samym rodzajem codziennego kontaktu.
    if let Some(sz) = szkola {
        let grupa = world.resource::<SocialIndex>().classmates(sz);
        for kto in bliscy(grupa, e.index(), params.close_contacts)
            .as_slice()
            .iter()
            .copied()
        {
            nowe.push((
                kto,
                RelationKind::Acquaintance as u8,
                params.coworker_gain_per_week,
                params.coworker_max,
            ));
        }
    }
    if let Some(b) = building {
        let kwartal = world.resource::<CityFacts>().block(b);
        let grupa = world.resource::<SocialIndex>().neighbours(kwartal);
        for kto in bliscy(grupa, e.index(), params.close_contacts)
            .as_slice()
            .iter()
            .copied()
        {
            nowe.push((
                kto,
                RelationKind::Neighbour as u8,
                params.neighbour_gain_per_week,
                params.neighbour_max,
            ));
        }
    }

    for (kto, kind, gain, max) in nowe.as_slice().iter().copied() {
        let Some(inny) = demography::citizen_by_index(world, kto) else {
            continue;
        };
        let kind = match kind {
            k if k == RelationKind::Colleague as u8 => RelationKind::Colleague,
            k if k == RelationKind::Acquaintance as u8 => RelationKind::Acquaintance,
            _ => RelationKind::Neighbour,
        };
        wzmocnij(world, e, inny, kind, gain, max, day);
        wzmocnij(world, inny, e, kind, gain, max, day);
        raport.contacts += 1;
    }
}

/// `n` najbliższych kontaktów z grupy: `n` kolejnych osób **za sobą** w pierścieniu.
///
/// Trzy rzeczy naraz, i każda jest tu potrzebna. Po pierwsze zbiór jest **stały w czasie**:
/// relacja rośnie o `gain` co tydzień, a zanika o 1 — kontakt rotujący po całym zakładzie
/// spotykałby tę samą osobę raz na kilka tygodni i waga nigdy nie przekroczyłaby progu
/// plotki. Po drugie nie jest to „pierwsze `n` z listy": wtedy cała klatka schodowa
/// zawiązywałaby relacje z tą samą trójką i graf byłby gwiazdą, a plotka szłaby przez
/// piastę albo nie szłaby wcale. Po trzecie pierścień jest spójny — plotka ma po czym iść
/// przez cały zakład i cały kwartał, choć nikt nie zna w nim wszystkich.
fn bliscy(grupa: &[(u32, u32)], ja: u32, n: u8) -> ArrayVec<u32, 8> {
    let mut out: ArrayVec<u32, 8> = ArrayVec::new();
    if grupa.len() <= 1 {
        return out;
    }
    let moja = grupa.iter().position(|(_, k)| *k == ja).unwrap_or(0);
    for k in 1..grupa.len() {
        if out.len() >= usize::from(n) || out.is_full() {
            break;
        }
        let (_, kto) = grupa[(moja + k) % grupa.len()];
        if kto != ja {
            out.push(kto);
        }
    }
    out
}

/// Podnosi wagę relacji o `gain`, nie wyżej niż `max`; zakłada ją, jeśli nie istnieje.
fn wzmocnij(
    world: &mut World,
    kto: Entity,
    z_kim: Entity,
    kind: RelationKind,
    gain: u8,
    max: u8,
    day: u64,
) {
    let Some(rel) = world.get::<crate::components::RelationsRef>(kto).copied() else {
        return;
    };
    let sr = demography::relations_ref(&rel);
    let dzis = (day % 65_536) as u16;
    let slab = world.resource_mut::<RelationSlab>();
    if let Some(x) = slab
        .entries_mut(sr)
        .iter_mut()
        .find(|x| x.other == z_kim.index())
    {
        // Relacja rodzinna nie „zamienia się" we współpracowniczą, gdy rodzeństwo
        // trafi do jednej firmy — mocniejsza więź wygrywa z częstszym kontaktem.
        if x.weight < max {
            x.weight = x.weight.saturating_add(gain).min(max);
        }
        x.last_contact_day = dzis;
        return;
    }
    demography::powiaz(world, kto, z_kim, kind, gain.max(1), day);
}

fn zanik(
    world: &mut World,
    e: Entity,
    day: u64,
    params: &crate::demography::table::SocialParams,
    raport: &mut SocialReport,
) {
    let Some(rel) = world.get::<crate::components::RelationsRef>(e).copied() else {
        return;
    };
    let mut sr = demography::relations_ref(&rel);
    let dzis = (day % 65_536) as u16;
    let slab = world.resource_mut::<RelationSlab>();

    let mut wygasle: ArrayVec<u32, SLAB_MAX> = ArrayVec::new();
    let mut i = 0;
    while i < sr.len as usize {
        let r = slab.entries(sr)[i];
        let tygodnie = u32::from(dzis.wrapping_sub(r.last_contact_day)) / 7;
        if tygodnie == 0 {
            i += 1;
            continue;
        }
        let ubytek = (tygodnie * u32::from(params.relation_decay_per_week)).min(255) as u8;
        let nowa = r.weight.saturating_sub(ubytek);
        let rodzinna = matches!(
            r.kind,
            x if x == RelationKind::Parent as u8
                || x == RelationKind::Child as u8
                || x == RelationKind::Sibling as u8
                || x == RelationKind::Partner as u8
        );
        if nowa == 0 && !rodzinna {
            // Relacja wygasła. Zdejmuje się ją **po obu stronach naraz**: shard 1/7
            // znaczyłby inaczej, że przez tydzień jedna strona pamięta drugą, a druga
            // nie — a `prop_relation_symmetry` i dobór partnera czytają obie.
            slab.remove_at(&mut sr, i);
            wygasle.push(r.other);
            raport.dropped += 1;
            continue;
        }
        slab.entries_mut(sr)[i].weight = nowa.max(u8::from(rodzinna));
        raport.decayed += 1;
        i += 1;
    }
    if let Some(slot) = world.get_mut::<crate::components::RelationsRef>(e) {
        slot.handle = sr.handle;
        slot.len = sr.len;
        slot.class = sr.class;
    }

    for idx in wygasle.as_slice().iter().copied() {
        let Some(inny) = demography::citizen_by_index(world, idx) else {
            continue;
        };
        let Some(r2) = world.get::<crate::components::RelationsRef>(inny).copied() else {
            continue;
        };
        let mut sr2 = demography::relations_ref(&r2);
        let slab = world.resource_mut::<RelationSlab>();
        if let Some(j) = slab.entries(sr2).iter().position(|x| x.other == e.index()) {
            slab.remove_at(&mut sr2, j);
            if let Some(slot) = world.get_mut::<crate::components::RelationsRef>(inny) {
                slot.handle = sr2.handle;
                slot.len = sr2.len;
                slot.class = sr2.class;
            }
        }
    }
}

/// Plotka (§5.8): ≤ 2 relacje losowane z wagą, jeden wpis wiedzy na relację.
fn plotka(
    world: &mut World,
    e: Entity,
    day: u64,
    params: &crate::demography::table::SocialParams,
    raport: &mut SocialReport,
) {
    let Some(rel) = world.get::<crate::components::RelationsRef>(e).copied() else {
        return;
    };
    let mut sluchacze: ArrayVec<(u32, u8), SLAB_MAX> = ArrayVec::new();
    for r in world
        .resource::<RelationSlab>()
        .entries(demography::relations_ref(&rel))
    {
        if r.weight >= params.gossip_min_weight {
            sluchacze.push((r.other, r.weight));
        }
    }
    if sluchacze.is_empty() {
        return;
    }
    let Some(kref) = world.get::<KnowledgeRef>(e).copied() else {
        return;
    };
    let moja_wiedza: ArrayVec<Knowledge, SLAB_MAX> = ArrayVec::from_slice(
        world
            .resource::<KnowledgeSlab>()
            .entries(demography::knowledge_ref(&kref)),
    );
    if moja_wiedza.is_empty() {
        return;
    }

    let mut r = rng(
        world.seed,
        StreamId::Gossip,
        e.index(),
        demography::stream_key(day, K_GOSSIP),
    );
    let dzis = (day % 65_536) as u16;

    // Losowanie bez powtórzeń, z wagą ∝ `Relation.weight`, **do skutku**: liczy się
    // `gossip_targets` osób, którym udało się coś powiedzieć, a nie `gossip_targets`
    // zagadniętych. Rozmowa z kimś, kto już wie, nie jest opowiedzeniem plotki — jest
    // zdaniem „wiem". Bez tego rozróżnienia wiedza zatrzymywała się na pierwszym
    // nasyconym kwartale: sąsiedzi dowiadywali się w tydzień, a potem każdy tydzień
    // schodził na powtarzaniu im tego samego (kryterium WP9).
    let mut opowiedziane = 0u8;
    let mut zagadnieci: ArrayVec<u32, 8> = ArrayVec::new();
    while opowiedziane < params.gossip_targets
        && zagadnieci.len() < sluchacze.len()
        && !zagadnieci.is_full()
    {
        let suma: u32 = sluchacze
            .as_slice()
            .iter()
            .filter(|(k, _)| !zagadnieci.as_slice().contains(k))
            .map(|(_, w)| u32::from(*w))
            .sum();
        if suma == 0 {
            break;
        }
        let mut los = r.gen_range_u32(suma);
        let mut wybrany = None;
        for (k, w) in sluchacze.as_slice() {
            if zagadnieci.as_slice().contains(k) {
                continue;
            }
            if los < u32::from(*w) {
                wybrany = Some(*k);
                break;
            }
            los -= u32::from(*w);
        }
        let Some(kto) = wybrany else { break };
        zagadnieci.push(kto);

        let Some(sluchacz) = demography::citizen_by_index(world, kto) else {
            continue;
        };
        let Some(ich) = world.get::<KnowledgeRef>(sluchacz).copied() else {
            continue;
        };
        // Najlepszy wpis, którego słuchacz jeszcze nie ma — ranga to ocena × świeżość.
        // Zbiór słuchacza czyta się **wprost ze slabu**, bez kopii: ma najwyżej 32 wpisy,
        // a kopia byłaby alokacją na każdą opowiedzianą plotkę.
        let znane = world
            .resource::<KnowledgeSlab>()
            .entries(demography::knowledge_ref(&ich));
        let Some(najlepszy) = moja_wiedza
            .as_slice()
            .iter()
            .filter(|k| !znane.iter().any(|z| z.target == k.target))
            .max_by_key(|k| (k.rank(dzis), std::cmp::Reverse(k.target)))
            .copied()
        else {
            continue;
        };
        learn_place(
            world,
            sluchacz,
            najlepszy.target,
            KnowledgeKind::Heard,
            najlepszy.score.saturating_sub(params.gossip_score_penalty),
            day,
        );
        opowiedziane += 1;
        raport.gossip_told += 1;
    }
}

/// Dopisuje albo odświeża wpis wiedzy o miejscu.
///
/// **Jedyna droga**, którą wiedza trafia do mieszkańca — używają jej plotka (§5.8),
/// widoczność z trasy (`TravelOracle::places_on_route`), zasiew Etapu 8 i, od M10,
/// reklama. Jedno wejście znaczy jeden limit 32 wpisów i jedna reguła wypychania.
pub fn learn_place(
    world: &mut World,
    citizen: Entity,
    target: u32,
    kind: KnowledgeKind,
    score: u8,
    day: u64,
) {
    let Some(kref) = world.get::<KnowledgeRef>(citizen).copied() else {
        return;
    };
    let mut sr = demography::knowledge_ref(&kref);
    let dzis = (day % 65_536) as u16;
    let slab = world.resource_mut::<KnowledgeSlab>();

    if let Some(x) = slab.entries_mut(sr).iter_mut().find(|k| k.target == target) {
        // Źródło silniejsze wygrywa: „byłem" nie zamienia się w „słyszałem".
        if kind as u8 <= x.kind {
            x.kind = kind as u8;
            x.score = x.score.max(score);
            x.day = dzis;
        }
        return;
    }
    slab.push(
        &mut sr,
        Knowledge {
            target,
            day: dzis,
            score,
            kind: kind as u8,
        },
        |wpisy| {
            wpisy
                .iter()
                .enumerate()
                .min_by_key(|(i, k)| (k.rank(dzis), *i))
                .map_or(0, |(i, _)| i)
        },
    );
    if let Some(slot) = world.get_mut::<KnowledgeRef>(citizen) {
        slot.handle = sr.handle;
        slot.len = sr.len;
        slot.class = sr.class;
    }
}

/// Z kim mieszkaniec jest w relacji i jak silnej — `(druga strona, waga 0..=100)`.
///
/// Wyjście grafu relacji **na zewnątrz `sim/agents`**: plotka M3 chodzi po nim
/// wewnętrznie, ale PR (M10b §5.2) i związki zawodowe (M10e) muszą po nim przejść
/// z góry. Zwraca encje, nie indeksy — wołający i tak potrzebuje encji, a konwersja
/// w jednym miejscu jest tańsza niż w każdym z osobna.
#[must_use]
pub fn relations_of(world: &World, citizen: Entity) -> Vec<(Entity, u8)> {
    let Some(rref) = world.get::<crate::components::RelationsRef>(citizen) else {
        return Vec::new();
    };
    let sr = demography::relations_ref(rref);
    world
        .resource::<RelationSlab>()
        .entries(sr)
        .iter()
        .filter_map(|r| demography::citizen_by_index(world, r.other).map(|e| (e, r.weight)))
        .collect()
}

/// Czy mieszkaniec zna to miejsce.
#[must_use]
pub fn knows_place(world: &World, citizen: Entity, target: u32) -> bool {
    world.get::<KnowledgeRef>(citizen).is_some_and(|kref| {
        world
            .resource::<KnowledgeSlab>()
            .entries(demography::knowledge_ref(kref))
            .iter()
            .any(|k| k.target == target)
    })
}

/// Ilu mieszkańców zna to miejsce i ilu jest w ogóle (§5.7).
///
/// Fundament pod markę w M10: „świadomość" marki to dokładnie ta liczba, policzona
/// dla produktu zamiast dla miejsca.
#[must_use]
pub fn awareness_of(world: &World, target: u32) -> (u32, u32) {
    let spis = world.resource::<Population>().citizens();
    let znajacy = spis
        .iter()
        .filter(|e| knows_place(world, **e, target))
        .count();
    (znajacy as u32, spis.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn klasa_jest_przedzialem_statusu_a_nie_polem() {
        assert_eq!(SocialClass::of(Q::new(0)), SocialClass::Lower);
        assert_eq!(SocialClass::of(Q::new(15)), SocialClass::Lower);
        assert_eq!(SocialClass::of(Q::new(16)), SocialClass::Working);
        assert_eq!(SocialClass::of(Q::new(33)), SocialClass::Working);
        assert_eq!(SocialClass::of(Q::new(34)), SocialClass::LowerMiddle);
        assert_eq!(SocialClass::of(Q::new(52)), SocialClass::LowerMiddle);
        assert_eq!(SocialClass::of(Q::new(53)), SocialClass::UpperMiddle);
        assert_eq!(SocialClass::of(Q::new(72)), SocialClass::UpperMiddle);
        assert_eq!(SocialClass::of(Q::new(73)), SocialClass::Upper);
        assert_eq!(SocialClass::of(Q::new(89)), SocialClass::Upper);
        assert_eq!(SocialClass::of(Q::new(90)), SocialClass::Elite);
        assert_eq!(SocialClass::of(Q::MAX), SocialClass::Elite);
    }

    #[test]
    fn percentyl_jest_pozycja_a_nie_wartoscia() {
        let d = StatusDistribution {
            income: vec![100, 200, 300, 400],
            wealth: vec![0],
        };
        assert_eq!(d.income_percentile(Money(50)), 0);
        assert_eq!(d.income_percentile(Money(300)), 50);
        assert_eq!(d.income_percentile(Money(1_000)), 100);
        // Pusty rozkład nie zeruje statusu — daje wartość neutralną.
        let pusty = StatusDistribution::default();
        assert!(pusty.is_empty());
        assert_eq!(pusty.income_percentile(Money(12_345)), 50);
    }

    #[test]
    fn status_sklada_sie_z_czynnikow_ktore_widac() {
        let wagi = StatusWeights {
            income: 25,
            wealth: 15,
            education: 15,
            occupation: 20,
            address: 15,
            consumption: 0,
            family: 10,
        };
        let dist = StatusDistribution {
            income: vec![0, 100, 200, 300],
            wealth: vec![0, 100],
        };
        let facts = CityFacts::default();
        let baza = StatusInput {
            income_per_capita: Money(200),
            wealth: Money(100),
            edu_level: 3,
            employment: Employment {
                site: 7,
                role: 1,
                ..Employment::default()
            },
            skill_in_role: Q::new(60),
            district: 0,
            parents_status: None,
            age_years: 35,
        };
        let b = status_of(&baza, &dist, &facts, wagi);
        assert_eq!(b.education, 75);
        assert!(b.status > 0 && b.status <= 100);

        // Wyższy dochód podnosi status i podnosi **swój** składnik, a nie inny.
        let bogatszy = StatusInput {
            income_per_capita: Money(1000),
            ..baza
        };
        let b2 = status_of(&bogatszy, &dist, &facts, wagi);
        assert!(b2.income > b.income);
        assert_eq!(b2.education, b.education);
        assert!(b2.status > b.status);

        // Bezrobocie uderza w prestiż zawodu.
        let bez_pracy = StatusInput {
            employment: Employment::default(),
            ..baza
        };
        assert!(status_of(&bez_pracy, &dist, &facts, wagi).occupation < b.occupation);
    }

    #[test]
    fn reputacja_rodziny_zanika_z_wiekiem() {
        let wej = |wiek: i32| StatusInput {
            income_per_capita: Money(0),
            wealth: Money(0),
            edu_level: 0,
            employment: Employment::default(),
            skill_in_role: Q::MIN,
            district: 0,
            parents_status: Some(90),
            age_years: wiek,
        };
        assert_eq!(reputacja_rodziny(&wej(18)), 90);
        assert!(reputacja_rodziny(&wej(30)) < 90);
        assert_eq!(reputacja_rodziny(&wej(40)), 50);
        assert_eq!(reputacja_rodziny(&wej(70)), 50);
        // Bez znanych rodziców składnik jest neutralny, a nie zerowy.
        assert_eq!(
            reputacja_rodziny(&StatusInput {
                parents_status: None,
                ..wej(20)
            }),
            50
        );
    }

    #[test]
    fn bliscy_tworza_pierscien_a_nie_gwiazde() {
        let grupa: Vec<(u32, u32)> = (0..5).map(|i| (7, i)).collect();
        // Zbiór jest stały w czasie — bez tego waga relacji nigdy nie dogoni zaniku.
        assert_eq!(bliscy(&grupa, 0, 2).as_slice(), &[1, 2]);
        assert_eq!(bliscy(&grupa, 3, 2).as_slice(), &[4, 0]);
        // Każdy ma innych bliskich, więc graf jest pierścieniem: plotka przejdzie
        // przez całą grupę, choć nikt nie zna w niej wszystkich (kryterium WP9).
        let mut krawedzie: Vec<(u32, u32)> = (0..5u32)
            .flat_map(|i| {
                bliscy(&grupa, i, 1)
                    .as_slice()
                    .iter()
                    .map(move |j| (i, *j))
                    .collect::<Vec<_>>()
            })
            .collect();
        krawedzie.sort_unstable();
        assert_eq!(krawedzie, vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 0)]);
        // Grupa jednoosobowa nie produkuje kontaktów.
        assert!(bliscy(&[(7, 3)], 3, 2).is_empty());
    }

    #[test]
    fn indeks_kontaktow_zwraca_grupy_a_nie_calosc() {
        let mut idx = SocialIndex::new();
        idx.by_site = vec![(1, 10), (1, 11), (2, 20), (2, 21), (2, 22)];
        idx.by_building = vec![(5, 10)];
        assert_eq!(idx.coworkers(1).len(), 2);
        assert_eq!(idx.coworkers(2).len(), 3);
        assert!(idx.coworkers(3).is_empty());
        assert_eq!(idx.neighbours(5), &[(5, 10)]);
    }
}
