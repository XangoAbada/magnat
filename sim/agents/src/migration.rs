//! Migracja gospodarstw — **jedyny regulator populacji** (M3c §5.7, WP8).
//!
//! Populacja nie jest sterowana do celu i nie ma „spawnowania do liczby". Regulatorem
//! jest migracja, która widzi dwa obserwowalne sygnały miasta: **wakaty** (etaty bez
//! pracownika) i **pustostany** (lokale bez gospodarstwa). Napływ jest proporcjonalny
//! do `min(wakaty, pustostany)` — bo przyjezdny potrzebuje obu naraz — a odpływ do
//! liczby gospodarstw, którym nie wyszło. To zamyka pętlę ujemnego sprzężenia: nadwyżka
//! urodzeń zjada wakaty, napływ spada do zera, a dorośli bez pracy wyjeżdżają.
//!
//! **Miasto wchodzi tu jako dwie płaskie listy**, nie jako `sim/world`. `Vacancies`
//! wypełnia ten, kto zna budynki i zakłady: w M3d generator populacji z `Unit`
//! i `Workplace` M2, w testach — ręcznie. Kierunek zależności jest ten sam co przy
//! `PlaceTable` (korekta A-5): `sim/world` zależy od `sim/agents`, nigdy odwrotnie.
//!
//! `spawn_household` jest **tym samym kodem**, którego użyje Etap 8 (§5.7: „ten sam
//! generator, mniejsze N"). Etap 8 dokłada nad nim dopasowania statystyczne — piramidę
//! wieku, dochód↔wartość lokalu, histogram dojazdu — i nadpisuje, co potrzebuje.

use crate::components::{
    AgentState, Employment, Identity, KnowledgeRef, Lifecycle, Needs, Personality, PlanRef,
    RelationsRef, Residence, Skills, Vitals, Wealth,
};
use crate::demography::{
    self, demography_system_id, stream_key, DemographyTable, LifeQueue, Population, DAYS_PER_YEAR,
    K_JOBLESS, K_MIGRATION, K_NEST,
};
use crate::household::{self, Household, HouseholdOverflow};
use magnat_core::{
    rng, DecisionReason, Entity, HashState, MigrationKind, Money, NeedKind, Rng, StateHasher,
    StreamId, NEED_COUNT, STOCK_CAT_COUNT,
};
use magnat_ecs::{CommandBuffer, World};
use std::collections::BTreeMap;

/// Wolny lokal. Tyle, ile trzeba, żeby przypisać gospodarstwo pod adres i oddać go
/// z powrotem, gdy gospodarstwo zniknie — reszta (powierzchnia, czynsz, właściciel)
/// jest w M2 i M5, a `sim/agents` nie ma powodu tego kopiować.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HomeSlot {
    pub building: u32,
    pub unit: u16,
    pub district: u16,
    /// Wartość lokalu — wejście dopasowania dochód↔mieszkanie w Etapie 8 i składnika
    /// „adres" w funkcji statusu (§5.8).
    pub value: Money,
}

/// Wolny etat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JobSlot {
    pub site: u32,
    pub role: u16,
    /// `ShiftKind`.
    pub shift: u8,
    /// Maska `DayOfWeek` (K-15).
    pub work_days: u8,
    pub district: u16,
    /// Mediana widełek z `Workplace.wage_band` (M2), miesięcznie, w groszach.
    pub wage_monthly: Money,
}

/// Wakaty i pustostany miasta — wejście regulatora (§5.7).
///
/// `ponytail:` wolne miejsca zdejmuje się z końca listy (LIFO), bez dobierania
/// dzielnicy ani odległości. Sufit nazwany: histogram czasu dojazdu wychodzi wtedy
/// z rozkładu miejsc, a nie z dopasowania — i dlatego **krok 7 Etapu 8** (M3d §5.9)
/// ma własną pętlę poprawkową na zamianach mieszkań. Tu chodzi o to, żeby przyjezdny
/// dostał dach i etat, a nie o to, żeby dostał je blisko siebie.
#[derive(Clone, Debug, Default)]
pub struct Vacancies {
    homes: Vec<HomeSlot>,
    jobs: Vec<JobSlot>,
    /// Dzielnica i płaca **zakładu**, żeby zwolniony etat wracał do puli taki sam,
    /// jaki z niej wyszedł. Bez tego etat po emeryturze wracał z dzielnicą 0
    /// i płacą 0: `take_job_in` przestawało go dopasowywać, następny pracownik
    /// dostawał go z drugiego końca miasta, a graf relacji dostawał most przez
    /// całe miasto (kryterium WP9, druga połowa).
    sites: BTreeMap<u32, (u16, Money)>,
    homes_total: u32,
    jobs_total: u32,
}

impl Vacancies {
    #[must_use]
    pub fn new(homes: Vec<HomeSlot>, jobs: Vec<JobSlot>) -> Vacancies {
        let mut sites = BTreeMap::new();
        for j in &jobs {
            sites.entry(j.site).or_insert((j.district, j.wage_monthly));
        }
        Vacancies {
            homes_total: homes.len() as u32,
            jobs_total: jobs.len() as u32,
            sites,
            homes,
            jobs,
        }
    }

    /// Pula **po zasiedleniu**: wolne jest to, co zostało, a pojemność miasta jest
    /// tym, co miasto ma.
    ///
    /// Etap 8 (M3d §5.9) rozdaje lokale i etaty własnym dopasowaniem statystycznym,
    /// więc nie może ich zdejmować przez `take_*` — a `new` policzyłoby pojemność
    /// z tego, co **zostało**, i regulator populacji zobaczyłby miasto bez ani jednego
    /// mieszkania. Dlatego pojemność wchodzi tu osobno od wolnych miejsc.
    ///
    /// `all_jobs` to **wszystkie** etaty miasta, także obsadzone: z nich powstaje mapa
    /// zakładów, dzięki której etat zwolniony po emeryturze wraca do puli z właściwą
    /// dzielnicą i płacą.
    #[must_use]
    pub fn with_occupancy(
        free_homes: Vec<HomeSlot>,
        free_jobs: Vec<JobSlot>,
        homes_total: u32,
        all_jobs: &[JobSlot],
    ) -> Vacancies {
        let mut sites = BTreeMap::new();
        for j in all_jobs {
            sites.entry(j.site).or_insert((j.district, j.wage_monthly));
        }
        Vacancies {
            homes_total,
            jobs_total: all_jobs.len() as u32,
            sites,
            homes: free_homes,
            jobs: free_jobs,
        }
    }

    /// Dzielnica i płaca zakładu, jeśli pula go zna.
    #[must_use]
    pub fn site_facts(&self, site: u32) -> Option<(u16, Money)> {
        self.sites.get(&site).copied()
    }

    #[must_use]
    pub fn free_homes(&self) -> usize {
        self.homes.len()
    }

    #[must_use]
    pub fn free_jobs(&self) -> usize {
        self.jobs.len()
    }

    #[must_use]
    pub fn homes_total(&self) -> u32 {
        self.homes_total
    }

    #[must_use]
    pub fn jobs_total(&self) -> u32 {
        self.jobs_total
    }

    pub fn take_home(&mut self) -> Option<HomeSlot> {
        self.homes.pop()
    }

    /// Wolny lokal **w tej dzielnicy** albo `None`.
    ///
    /// Bez fallbacku na „pierwszy lepszy" i to jest istota rzeczy. Przeprowadzka po
    /// rozstaniu albo wyprowadzka od rodziców jest ruchem **lokalnym**: człowiek zostaje
    /// tam, gdzie ma pracę i znajomych. Lokal na drugim końcu miasta wstawiałby do grafu
    /// relacji most przez całą metropolię — sąsiedzi z nowego kwartału, współpracownicy
    /// ze starego zakładu — a plotka przeskakiwałaby po nim pięć kilometrów w jednym
    /// kroku (kryterium WP9, druga połowa). W mieście zapełnionym po brzegi taki ruch
    /// po prostu **się nie udaje**, i to też jest prawda o mieście, a nie uproszczenie:
    /// rozwiedziona para mieszka razem, dopóki coś się nie zwolni.
    pub fn take_home_in(&mut self, district: u16) -> Option<HomeSlot> {
        let i = self.homes.iter().rposition(|h| h.district == district)?;
        Some(self.homes.remove(i))
    }

    /// Czy w dzielnicy jest wolny lokal.
    #[must_use]
    pub fn has_home_in(&self, district: u16) -> bool {
        self.homes.iter().any(|h| h.district == district)
    }

    pub fn release_home(&mut self, h: HomeSlot) {
        self.homes.push(h);
    }

    pub fn take_job(&mut self) -> Option<JobSlot> {
        self.jobs.pop()
    }

    /// Wolny etat **w tej dzielnicy**, a jak nie ma — pierwszy lepszy.
    ///
    /// `ponytail:` liniowe szukanie od końca listy zamiast indeksu per dzielnica.
    /// Sufit nazwany: lokale i etaty są w puli w tej samej kolejności co w mieście,
    /// a obie strony zdejmuje się od końca, więc pasujący wpis leży zwykle tuż przy
    /// końcu i szukanie kończy się po kilku krokach. Najgorszy przypadek to O(n) na
    /// wywołanie i dopiero on może zaboleć Etap 8 przy 400 tys. mieszkańców
    /// (M3d §5.9, cel ≤ 30 s) — wtedy pula dostaje `BTreeMap<u16, Vec<usize>>`
    /// z wolnymi indeksami per dzielnica. Nie wcześniej.
    ///
    /// Nie jest to dopasowanie dojazdu (to robi krok 7 Etapu 8 w M3d), tylko tyle,
    /// żeby przyjezdny nie dostawał pracy na drugim końcu miasta z prawdopodobieństwem
    /// proporcjonalnym do niczego. Ma to skutek widoczny w §5.8: relacja współpracownicza
    /// jest wtedy lokalna, a plotka nie przeskakuje miasta w jednym kroku.
    pub fn take_job_in(&mut self, district: u16) -> Option<JobSlot> {
        match self.jobs.iter().rposition(|j| j.district == district) {
            Some(i) => Some(self.jobs.remove(i)),
            None => self.jobs.pop(),
        }
    }

    pub fn release_job(&mut self, mut j: JobSlot) {
        if let Some((d, w)) = self.sites.get(&j.site) {
            j.district = *d;
            j.wage_monthly = *w;
        }
        self.jobs.push(j);
    }

    /// Likwidacja **wolnych** etatów. Zwraca, ilu nie udało się zdjąć, bo są zajęte —
    /// tymi zajmuje się `shock_retire_jobs`, bo do ich likwidacji trzeba znać pracownika.
    pub fn retire_jobs(&mut self, n: usize) -> usize {
        let zdjete = n.min(self.jobs.len());
        self.jobs.truncate(self.jobs.len() - zdjete);
        self.jobs_total -= zdjete as u32;
        n - zdjete
    }

    /// Zmniejsza pulę o jeden etat zajęty, który właśnie przestał istnieć.
    pub fn drop_job(&mut self) {
        self.jobs_total = self.jobs_total.saturating_sub(1);
    }
}

impl HashState for Vacancies {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.homes_total);
        h.write_u32(self.jobs_total);
        h.write_u64(self.homes.len() as u64);
        for s in &self.homes {
            h.write_u32(s.building);
            h.write_u16(s.unit);
            h.write_u16(s.district);
            s.value.hash_state(h);
        }
        h.write_u64(self.sites.len() as u64);
        for (site, (d, w)) in &self.sites {
            h.write_u32(*site);
            h.write_u16(*d);
            w.hash_state(h);
        }
        h.write_u64(self.jobs.len() as u64);
        for j in &self.jobs {
            h.write_u32(j.site);
            h.write_u16(j.role);
            h.write(&[j.shift, j.work_days]);
            h.write_u16(j.district);
            j.wage_monthly.hash_state(h);
        }
    }
}

/// Gospodarstwa, którym coś nie wychodzi. Wpis powstaje, gdy gospodarstwo traci dach
/// albo pracę, i znika, gdy odzyska — więc mapa ma rozmiar problemu, a nie miasta.
///
/// Osobny zasób zamiast pól w `Household`: licznik „miesięcy bez pracy" dotyczy
/// znikomej mniejszości gospodarstw, a komponent płaci za niego pamięcią razy 167 tys.
#[derive(Clone, Debug, Default)]
pub struct Unsettled {
    by_household: BTreeMap<u32, UnsettledState>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct UnsettledState {
    pub jobless_months: u8,
    /// Doba, od której gospodarstwo jest bez lokalu; `u32::MAX` = ma lokal.
    pub homeless_since: u32,
}

impl Unsettled {
    #[must_use]
    pub fn new() -> Unsettled {
        Unsettled::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_household.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_household.is_empty()
    }

    #[must_use]
    pub fn get(&self, household: u32) -> UnsettledState {
        self.by_household
            .get(&household)
            .copied()
            .unwrap_or(UnsettledState {
                jobless_months: 0,
                homeless_since: u32::MAX,
            })
    }

    pub fn set(&mut self, household: u32, state: UnsettledState) {
        if state.jobless_months == 0 && state.homeless_since == u32::MAX {
            self.by_household.remove(&household);
        } else {
            self.by_household.insert(household, state);
        }
    }

    pub fn forget(&mut self, household: u32) {
        self.by_household.remove(&household);
    }
}

impl HashState for Unsettled {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.by_household.len() as u64);
        for (hh, s) in &self.by_household {
            h.write_u32(*hh);
            h.write_u8(s.jobless_months);
            h.write_u32(s.homeless_since);
        }
    }
}

/// Bilans miesiąca migracyjnego.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MigrationReport {
    pub arrived_households: u32,
    pub arrived_citizens: u32,
    pub left_households: u32,
    pub left_citizens: u32,
    /// Dorośli, którzy założyli własne gospodarstwo w mieście.
    pub left_nest: u32,
    /// Dorośli, którzy wyjechali samotnie, bo nie mieli ani pracy, ani dokąd się
    /// wyprowadzić. To jest zawór, przez który nadwyżka urodzeń opuszcza miasto.
    pub left_alone: u32,
    pub attractiveness: u8,
    pub free_homes: u32,
    pub free_jobs: u32,
    pub unemployment_permille: u16,
    pub reasons: Vec<(u32, DecisionReason)>,
}

/// Atrakcyjność miasta w skali 0..=100 (§5.7).
///
/// W M3 widać z niej dwa człony z czterech: bezrobocie i bezpieczeństwo. Płaca
/// i wartość gruntu wchodzą, gdy M5 i M7 zaczną je zmieniać — dziś są stałe z generacji,
/// więc dokładałyby do wyniku stałą, a nie informację. To jest granica fazy, nie luka.
#[must_use]
pub fn attractiveness(unemployment_permille: u16, safety_mean: u8) -> u8 {
    let bezrobocie = i32::from(unemployment_permille) * 3 / 10; // 10 % → −30 pkt
    let bezpieczenstwo = (i32::from(safety_mean) - 50) / 2; // ±25 pkt
    (70 - bezrobocie + bezpieczenstwo).clamp(0, 100) as u8
}

/// Jeden miesiąc migracji (§5.7). Wołany co 30 dób.
pub fn step_month(world: &mut World, day: u64) -> MigrationReport {
    let mut raport = MigrationReport::default();
    let (bezrobocie, bezpieczenstwo) = wskazniki(world, day);
    raport.unemployment_permille = bezrobocie;
    raport.attractiveness = attractiveness(bezrobocie, bezpieczenstwo);

    usamodzielnienie(world, day, &mut raport);
    odplyw(world, day, &mut raport);
    naplyw(world, day, &mut raport);

    let v = world.resource::<Vacancies>();
    raport.free_homes = v.free_homes() as u32;
    raport.free_jobs = v.free_jobs() as u32;
    raport
}

/// Bezrobocie w promilach i średnie bezpieczeństwo. Jeden przebieg po spisie —
/// oba wskaźniki są wejściem tej samej decyzji, więc liczą się razem.
fn wskazniki(world: &World, day: u64) -> (u16, u8) {
    let ages = world.resource::<DemographyTable>().ages();
    let mut aktywni = 0u64;
    let mut bez_pracy = 0u64;
    let mut safety = 0u64;
    let mut ludzi = 0u64;

    for e in world.resource::<Population>().citizens() {
        let Some(id) = world.get::<Identity>(*e) else {
            continue;
        };
        ludzi += 1;
        if let Some(n) = world.get::<Needs>(*e) {
            safety += u64::from(n.get(NeedKind::Safety).get());
        }
        let wiek = id.age_years(day as i32);
        if wiek < i32::from(ages.work_start) || wiek >= i32::from(ages.retirement) {
            continue;
        }
        let Some(emp) = world.get::<Employment>(*e) else {
            continue;
        };
        if emp.flags & (Employment::FLAG_PUPIL | Employment::FLAG_STUDENT) != 0 {
            continue;
        }
        aktywni += 1;
        if !emp.has_job() {
            bez_pracy += 1;
        }
    }

    let bezrobocie = bez_pracy
        .checked_mul(1000)
        .and_then(|x| x.checked_div(aktywni))
        .map_or(0, |x| x.min(1000) as u16);
    let srednie = safety.checked_div(ludzi).map_or(50, |x| x.min(100) as u8);
    (bezrobocie, srednie)
}

// ── odpływ ──────────────────────────────────────────────────────────────────────

fn odplyw(world: &mut World, day: u64, raport: &mut MigrationReport) {
    let params = world.resource::<DemographyTable>().migration().clone();
    let ages = world.resource::<DemographyTable>().ages();
    let gospodarstwa: Vec<Entity> = world.resource::<Population>().households().to_vec();

    let mut do_wyjazdu: Vec<(Entity, MigrationKind, u8)> = Vec::new();
    for hh_e in gospodarstwa {
        let Some(hh) = world.get::<Household>(hh_e).copied() else {
            continue;
        };
        if !hh.is_active() {
            continue;
        }
        let idx = hh_e.index();
        let mut stan = world.resource::<Unsettled>().get(idx);

        // Bez dachu: licznik startuje w miesiącu, w którym lokal zniknął.
        if hh.has_home() {
            stan.homeless_since = u32::MAX;
        } else if stan.homeless_since == u32::MAX {
            stan.homeless_since = day as u32;
        }

        // Bez pracy: liczy się **gospodarstwo**, nie osoba — jeden etat na cztery osoby
        // wystarcza, żeby zostać, i to jest treść §5.7.
        let sklad = household::members_of(idx, &hh, world.resource::<HouseholdOverflow>());
        let mut dorosli = 0u32;
        let mut pracujacy = 0u32;
        for m in sklad.iter() {
            let Some(c) = znajdz(world, *m) else { continue };
            let Some(id) = world.get::<Identity>(c) else {
                continue;
            };
            if id.age_years(day as i32) < i32::from(ages.adult) {
                continue;
            }
            dorosli += 1;
            if world
                .get::<Employment>(c)
                .is_some_and(|e| e.is_employed() || e.flags & Employment::FLAG_RETIRED != 0)
            {
                pracujacy += 1;
            }
        }
        if dorosli > 0 && pracujacy == 0 {
            stan.jobless_months = stan.jobless_months.saturating_add(1);
        } else {
            stan.jobless_months = 0;
        }

        // Gospodarstwo, w którym **część** dorosłych nie ma pracy, też się zastanawia —
        // z hazardem proporcjonalnym do udziału bez pracy. Bez tego członu miasto po
        // szoku wracałoby do równowagi wyłącznie przez gospodarstwa, w których nie
        // pracuje **nikt**, a te są mniejszością: para, w której jedno straciło etat,
        // zostawałaby w nieskończoność i bezrobocie schodziłoby dekadami zamiast lat
        // (kryterium WP8).
        let bez_pracy = dorosli.saturating_sub(pracujacy);
        let czesciowy = if bez_pracy > 0 && dorosli > 0 {
            // Mnożnik zdarzeniowy wchodzi tu, a nie przy gospodarstwie bez ani jednego
            // pracującego: wyjazd z braku pracy jest kanałem koniunktury, a wyjazd
            // z braku dachu nad głową — nie (`D11`).
            let emig = u32::from(
                world
                    .get_resource::<crate::worldparams::DemographyParams>()
                    .copied()
                    .unwrap_or_default()
                    .emigration_bps,
            );
            let p = u32::from(params.jobless_leave_permille) * bez_pracy / dorosli * emig / 10_000;
            let mut r = rng(
                world.seed,
                StreamId::Migration,
                idx,
                stream_key(day, K_JOBLESS),
            );
            r.gen_bool_permille(p.min(1000) as u16)
        } else {
            false
        };

        let powod = if stan.jobless_months >= params.jobless_months_to_leave || czesciowy {
            Some(MigrationKind::LeftJobless)
        } else if stan.homeless_since != u32::MAX
            && day.saturating_sub(u64::from(stan.homeless_since))
                >= u64::from(params.settle_grace_days)
        {
            Some(MigrationKind::LeftHousing)
        } else {
            niezadowolenie(world, &sklad, &params, day, idx)
        };

        world.resource_mut::<Unsettled>().set(idx, stan);
        if let Some(k) = powod {
            do_wyjazdu.push((hh_e, k, stan.jobless_months));
        }
    }

    if do_wyjazdu.is_empty() {
        return;
    }
    let mut cmd = CommandBuffer::new(demography_system_id());
    for (hh_e, kind, miesiace) in do_wyjazdu {
        let n = wyprowadz(world, hh_e, &mut cmd);
        raport.left_households += 1;
        raport.left_citizens += n;
        raport.reasons.push((
            hh_e.index(),
            DecisionReason::MigrationDecision {
                kind,
                months_jobless: miesiace,
            },
        ));
    }
    magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
}

/// Wyjazd z powodu warunków, nie z powodu braku pracy: ciasnota albo brak bezpieczeństwa.
fn niezadowolenie(
    world: &World,
    sklad: &[u32],
    params: &crate::demography::table::MigrationParams,
    day: u64,
    hh_idx: u32,
) -> Option<MigrationKind> {
    let mut zle = false;
    for m in sklad {
        let Some(c) = znajdz(world, *m) else { continue };
        let Some(n) = world.get::<Needs>(c) else {
            continue;
        };
        if n.get(NeedKind::Housing).get() < params.housing_need_threshold
            || n.get(NeedKind::Safety).get() < params.safety_need_threshold
        {
            zle = true;
            break;
        }
    }
    if !zle {
        return None;
    }
    let mut r = rng(
        world.seed,
        StreamId::Migration,
        hh_idx,
        stream_key(day, K_MIGRATION),
    );
    if r.gen_bool_permille(params.need_leave_permille) {
        Some(MigrationKind::LeftUnsettled)
    } else {
        None
    }
}

/// Wyprowadzka gospodarstwa: zwolnienie lokalu i etatów, usunięcie mieszkańców.
/// Zwraca, ilu mieszkańców wyjechało.
pub fn wyprowadz(world: &mut World, hh_e: Entity, cmd: &mut CommandBuffer) -> u32 {
    let idx = hh_e.index();
    let Some(hh) = world.get::<Household>(hh_e).copied() else {
        return 0;
    };
    let sklad = household::members_of(idx, &hh, world.resource::<HouseholdOverflow>());
    let mut n = 0u32;

    for m in sklad.iter() {
        let Some(c) = znajdz(world, *m) else { continue };
        demography::month::wyprowadz_mieszkanca(world, c, cmd);
        n += 1;
    }

    // Gospodarstwo pochodne (`R2-WP3`) mieszka kątem u innego i lokalu nie ma —
    // oddanie go do puli tworzyłoby mieszkanie z niczego.
    if hh.has_home() && !hh.is_overcrowded() {
        world.resource_mut::<Vacancies>().release_home(HomeSlot {
            building: hh.building,
            unit: hh.unit,
            district: hh.district,
            value: Money::ZERO,
        });
    }
    world
        .resource_mut::<HouseholdOverflow>()
        .clear_household(idx);
    world.resource_mut::<Unsettled>().forget(idx);
    world.resource_mut::<Population>().remove_household(hh_e);
    world.resource_mut::<Population>().departures += u64::from(n);
    cmd.despawn(hh_e);
    n
}

/// Zwalnia etat mieszkańca i oddaje go do puli wakatów.
///
/// **Każde** wyjście z rynku pracy musi tędy przejść: zgon, emerytura, wyjazd. Etat,
/// który nie wraca do puli, znika z miasta na zawsze — a wtedy `min(wakaty, pustostany)`
/// zostaje na zero i napływ wygasa, mimo że miasto ma wolne miejsca pracy. Populacja
/// może wtedy już tylko maleć, a regulator z §5.7 przestaje być regulatorem.
pub fn release_job_of(world: &mut World, c: Entity) {
    let Some(emp) = world.get::<Employment>(c).copied() else {
        return;
    };
    // `is_employed`, nie `has_job`: uczeń ma w `site` szkołę, a szkoła nie jest
    // etatem wziętym z puli wakatów — oddanie jej do `Vacancies` **tworzyłoby**
    // miejsce pracy z niczego, raz na każde dziecko, które umiera albo się wyprowadza.
    if !emp.is_employed() {
        return;
    }
    world.resource_mut::<Vacancies>().release_job(JobSlot {
        site: emp.site,
        role: emp.role,
        shift: emp.shift,
        work_days: emp.work_days,
        district: 0,
        wage_monthly: Money::ZERO,
    });
    if let Some(e) = world.get_mut::<Employment>(c) {
        e.site = Employment::NO_SITE;
        e.work_days = 0;
        e.flags |= Employment::FLAG_UNEMPLOYED;
    }
}

// ── napływ ──────────────────────────────────────────────────────────────────────

fn naplyw(world: &mut World, day: u64, raport: &mut MigrationReport) {
    let params = world.resource::<DemographyTable>().migration().clone();
    let (wakaty, pustostany) = {
        let v = world.resource::<Vacancies>();
        (v.free_jobs(), v.free_homes())
    };
    let szansa = wakaty.min(pustostany);
    if szansa == 0 {
        return;
    }
    // napływ = k_in · min(wakaty, pustostany) · atrakcyjność · mnożnik zdarzeń
    //
    // Mnożnik jest czwartym czynnikiem, a nie zastąpieniem trzeciego: `attractiveness`
    // mówi, jak miasto wygląda z zewnątrz **dziś**, a `immigration_bps` niesie falę
    // migracji jako zdarzenie (PRD §11.2) i koniunkturę jako wskaźnik (`D11`).
    let mnoznik = u64::from(
        world
            .get_resource::<crate::worldparams::DemographyParams>()
            .copied()
            .unwrap_or_default()
            .immigration_bps,
    );
    let ile = (szansa as u64
        * u64::from(params.k_in_permille)
        * u64::from(raport.attractiveness)
        * mnoznik
        / (1000 * 100 * 10_000)) as usize;
    if ile == 0 {
        return;
    }

    for i in 0..ile {
        let mut r = rng(
            world.seed,
            StreamId::Migration,
            i as u32,
            stream_key(day, K_MIGRATION),
        );
        let Some(home) = world.resource_mut::<Vacancies>().take_home() else {
            break;
        };
        let rozmiar = params.arrival_sizes
            [r.gen_range_u32(params.arrival_sizes.len() as u32) as usize]
            .max(1);
        let dorosli = rozmiar.clamp(1, 2);
        let dzieci = rozmiar - dorosli;
        let hh_e = spawn_household(world, day, &mut r, dorosli, dzieci, home);
        raport.arrived_households += 1;
        raport.arrived_citizens += u32::from(rozmiar);
        raport.reasons.push((
            hh_e.index(),
            DecisionReason::MigrationDecision {
                kind: MigrationKind::Arrived,
                months_jobless: 0,
            },
        ));
    }
}

/// Tworzy gospodarstwo z lokalem i obsadza dorosłych wolnymi etatami.
///
/// **To jest generator, którego użyje Etap 8** (§5.7: „ten sam kod, mniejsze N").
/// Tutaj jest minimum, które musi działać samo: wiek z parametrów danych, osobowość
/// z `StreamId::PersonalityGen`, praca z puli wakatów. Dopasowania statystyczne
/// (piramida wieku epoki, dochód↔wartość lokalu, histogram dojazdu) dokłada M3d
/// **nad** tym, nadpisując komponenty — nie obok, przez drugi generator.
pub fn spawn_household(
    world: &mut World,
    day: u64,
    r: &mut Rng,
    adults: u8,
    children: u8,
    home: HomeSlot,
) -> Entity {
    let params = world.resource::<DemographyTable>().migration().clone();
    spawn_household_aged(
        world,
        day,
        r,
        adults,
        children,
        home,
        params.arrival_adult_age_min,
        params.arrival_adult_age_spread,
    )
}

/// Jak `spawn_household`, ale z jawnym zakresem wieku dorosłych.
///
/// Migracja ciągnie ludzi w wieku produkcyjnym (parametry `arrival_*` w danych),
/// a zasiew startowy potrzebuje **całego** przekroju, łącznie z emerytami — inaczej
/// pierwsze pokolenie starzeje się zbiorczo i piramida wieku przez pół stulecia wygląda
/// jak słupek wędrujący w górę. Właściwą piramidę epoki wstawia Etap 8 (M3d §5.9 krok 1);
/// to jest jej najuboższa forma: rozkład jednostajny.
#[allow(clippy::too_many_arguments)]
pub fn spawn_household_aged(
    world: &mut World,
    day: u64,
    r: &mut Rng,
    adults: u8,
    children: u8,
    home: HomeSlot,
    adult_age_min: u8,
    adult_age_spread: u8,
) -> Entity {
    let ages = world.resource::<DemographyTable>().ages();
    // Region i nazwisko losuje się **raz na gospodarstwo**, nie raz na dorosłego:
    // inaczej mąż jest Nowakiem, żona Kowalską, a dzieci dziedziczą po matce, więc
    // ojciec nazywa się w karcie inaczej niż reszta domu.
    //
    // **Nazwisko nie zmienia się po ślubie i nie zmieni** (`R2-WP6`, `D-N4`). Nie
    // jest to odłożone do „fazy, która ślub modeluje" — takiej fazy nie ma i nie
    // będzie. `M10a` §5.8 opiera most makro↔mezo na tym, że imię, nazwisko, płeć
    // i osobowość są funkcją `(world_seed, birth_index)` i **nigdy nie są
    // przechowywane**; `MacroCell` ma na mieszkańca osiem bajtów i nie ma gdzie
    // trzymać nazwiska zmienionego w połowie życia. Zysk byłby kosmetyczny, koszt
    // to osiem bajtów na mieszkańca w modelu makro plus migracja formatu zapisu.
    let nazwy = crate::names::catalog();
    let region = nazwy.pick_region(r);
    let nazwisko = nazwy.pick_surname(region, r);

    let hh_e = world
        .spawn()
        .with(Household {
            district: home.district,
            building: home.building,
            unit: home.unit,
            flags: Household::FLAG_ACTIVE,
            stock: [7; STOCK_CAT_COUNT],
            ..Household::default()
        })
        .id();
    world.resource_mut::<Population>().add_household(hh_e);
    let hh_idx = hh_e.index();

    let mut czlonkowie: Vec<Entity> = Vec::with_capacity(usize::from(adults + children));
    for i in 0..adults {
        let wiek =
            i32::from(adult_age_min) + r.gen_range_u32(u32::from(adult_age_spread).max(1)) as i32;
        // Para dorosłych jest różnopłciowa z konstrukcji (dwoje pierwszych), bo to ona
        // ma rodzić dzieci; dorosły samotny dostaje płeć z losowania. Bez tego miasto
        // ma systematycznie za mało kobiet w wieku rozrodczym i umiera na demografię,
        // której nikt nie zmieniał.
        let mezczyzna = if adults >= 2 {
            i == 0
        } else {
            r.gen_bool_permille(500)
        };
        let c = spawn_citizen(world, day, r, hh_idx, &home, wiek, mezczyzna, nazwisko);
        czlonkowie.push(c);
    }
    for _ in 0..children {
        let wiek = r.gen_range_u32(u32::from(ages.adult)) as i32;
        let mezczyzna = r.gen_bool_permille(500);
        let c = spawn_citizen(world, day, r, hh_idx, &home, wiek, mezczyzna, nazwisko);
        czlonkowie.push(c);
    }

    // Skład, partnerstwo dorosłych i relacje rodzinne.
    let mut hh = world.get::<Household>(hh_e).copied().unwrap_or_default();
    for c in &czlonkowie {
        let ov = world.resource_mut::<HouseholdOverflow>();
        // Rozmiar gospodarstwa przyjezdnego pochodzi z `migration.arrival_sizes`
        // i walidator trzyma go poniżej `HH_MAX_MEMBERS` — ale wynik sprawdzamy,
        // bo dane wolno przestawić, a `R2-WP3` właśnie po to zrobiło z tego
        // `#[must_use]`: mieszkaniec poza składem istnieje i nie istnieje naraz.
        //
        // **Wywołanie stoi poza `debug_assert!`, i to nie jest kosmetyka** (`R2-WP8`,
        // pozycja 77 wykazu): `debug_assert!` **nie oblicza swojego argumentu**
        // w profilu `release`, więc dopóki dopisanie członka było argumentem asercji,
        // release stawiał miasto z samych **pustych** gospodarstw. Nikt nie dostawał
        // etatu, nikt nie miał dochodu, nikt niczego nie kupował — a debug pokazywał
        // świat zdrowy, więc `cargo test` był zielony i nic tego nie łapało.
        let dopisany = household::add_member(hh_idx, &mut hh, ov, c.index());
        debug_assert!(
            dopisany,
            "gospodarstwo przyjezdne ponad HH_MAX_MEMBERS — popraw data/demography"
        );
    }
    if adults >= 2 {
        let (a, b) = (czlonkowie[0], czlonkowie[1]);
        if let Some(l) = world.get_mut::<Lifecycle>(a) {
            l.partner = b.index();
            l.flags |= Lifecycle::FLAG_PARTNERED;
        }
        if let Some(l) = world.get_mut::<Lifecycle>(b) {
            l.partner = a.index();
            l.flags |= Lifecycle::FLAG_PARTNERED;
        }
        let waga = world.resource::<DemographyTable>().social().family_weight;
        demography::powiaz(world, a, b, crate::store::RelationKind::Partner, waga, day);
    }
    // Rodzice, rodzeństwo i dziadkowie jedną regułą — tą samą, którą wiąże poród
    // (`R2-WP2`). Przedtem stała tu podwójna pętla wiążąca dorosłych z dziećmi
    // i **tylko** ich: dwoje dzieci w gospodarstwie z napływu albo z zasiedlenia
    // było dla silnika dwojgiem obcych ludzi pod jednym dachem.
    demography::zwiaz_rodzine(world, &czlonkowie, day);

    demography::przeklasyfikuj(world, hh_idx, &mut hh, day);
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh;
    }

    // Etaty dla dorosłych w wieku produkcyjnym.
    for c in czlonkowie.iter().take(usize::from(adults)) {
        let wiek = world
            .get::<Identity>(*c)
            .map_or(0, |i| i.age_years(day as i32));
        if wiek < i32::from(ages.work_start) || wiek >= i32::from(ages.retirement) {
            continue;
        }
        let Some(job) = world.resource_mut::<Vacancies>().take_job_in(home.district) else {
            continue;
        };
        if let Some(e) = world.get_mut::<Employment>(*c) {
            e.site = job.site;
            e.role = job.role;
            e.shift = job.shift;
            e.work_days = job.work_days;
            e.flags &= !Employment::FLAG_UNEMPLOYED;
        }
        if let Some(h) = world.get_mut::<Household>(hh_e) {
            h.income_monthly = h
                .income_monthly
                .checked_add(job.wage_monthly)
                .unwrap_or(h.income_monthly);
        }
    }

    world.resource_mut::<Population>().arrivals += u64::from(adults) + u64::from(children);
    hh_e
}

/// Zalążkowa populacja: gospodarstwa wstawiane w wolne lokale **tym samym generatorem**,
/// którego używa napływ (§5.7).
///
/// To jest minimum, żeby `m3_century` miał od czego zacząć, a nie Etap 8. Brakuje tu
/// wszystkiego, co Etap 8 wnosi: piramidy wieku epoki, składania gospodarstw po wieku,
/// wykształcenia, dopasowania dochód↔wartość lokalu i histogramu dojazdu (M3d §5.9).
/// M3d **nadpisuje** wynik tej funkcji, a nie zakłada drugiego generatora obok.
///
/// Zwraca liczbę utworzonych mieszkańców. Kończy, gdy skończą się pustostany.
pub fn seed_population(world: &mut World, day: u64, households: usize) -> u32 {
    let mut ludzi = 0u32;
    for i in 0..households {
        let Some(home) = world.resource_mut::<Vacancies>().take_home() else {
            break;
        };
        let mut r = rng(world.seed, StreamId::PopGen, i as u32, stream_key(day, 0));
        let params = world.resource::<DemographyTable>().migration().clone();
        let rozmiar = params.arrival_sizes
            [r.gen_range_u32(params.arrival_sizes.len() as u32) as usize]
            .max(1);
        let dorosli = rozmiar.clamp(1, 2);
        let dzieci = rozmiar - dorosli;
        let ages = world.resource::<DemographyTable>().ages();
        let rozpietosc = ages.max.saturating_sub(ages.adult).saturating_sub(25);
        spawn_household_aged(
            world, day, &mut r, dorosli, dzieci, home, ages.adult, rozpietosc,
        );
        ludzi += u32::from(rozmiar);
    }
    // Zasiew nie jest napływem migracyjnym — księga populacji ma odróżniać ludzi,
    // którzy „byli od początku", od tych, którzy przyjechali (`prop_population_identity`).
    world.resource_mut::<Population>().arrivals -= u64::from(ludzi);
    ludzi
}

/// Jeden mieszkaniec z kompletem trzynastu komponentów. Uczeń dostaje flagę, ale
/// **nie dostaje szkoły** — placówki zna Etap 8, a nie ta funkcja.
#[allow(clippy::too_many_arguments)]
fn spawn_citizen(
    world: &mut World,
    day: u64,
    r: &mut Rng,
    household: u32,
    home: &HomeSlot,
    age_years: i32,
    male: bool,
    surname: u16,
) -> Entity {
    let ages = world.resource::<DemographyTable>().ages();
    let uczen = age_years >= i32::from(ages.school_start) && age_years < i32::from(ages.school_end);
    let cechy: [u8; 8] = std::array::from_fn(|_| r.gen_q().get());
    let e = world
        .spawn()
        .with(Identity {
            first_name: crate::names::catalog().pick_first(
                crate::names::catalog().region_of_surname(surname),
                male,
                r,
            ),
            last_name: surname,
            birth_day: day as i32 - age_years * DAYS_PER_YEAR as i32,
            birth_district: Identity::DISTRICT_IMMIGRANT,
            flags: Identity::FLAG_ALIVE | if male { Identity::FLAG_MALE } else { 0 },
            _pad: 0,
            household,
        })
        .with(Personality(cechy))
        .with(Vitals {
            health: 60 + r.gen_range_u32(40) as u8,
            energy: 60 + r.gen_range_u32(40) as u8,
            ..Vitals::default()
        })
        .with(Needs {
            level: [100; NEED_COUNT],
            updated_at: (day * 1440) as u32,
        })
        .with(Skills::default())
        .with(Wealth::default())
        .with(Employment {
            flags: if uczen {
                Employment::FLAG_PUPIL
            } else {
                Employment::FLAG_UNEMPLOYED
            },
            ..Employment::default()
        })
        .with(Residence {
            building: home.building,
            unit: home.unit,
            district: home.district,
        })
        .with(PlanRef::default())
        .with(AgentState::default())
        .with(KnowledgeRef::default())
        .with(RelationsRef::default())
        .with(crate::components::BrandsRef::default())
        .with(Lifecycle::default())
        .id();
    world.resource_mut::<Population>().add_citizen(e);
    world.resource_mut::<LifeQueue>().cancel(e.index());
    e
}

fn znajdz(world: &World, index: u32) -> Option<Entity> {
    let p = world.resource::<Population>();
    p.citizens()
        .binary_search_by_key(&index, |e| e.index())
        .ok()
        .map(|i| p.citizens()[i])
}

// -- eksperyment szokowy (WP8) --------------------------------------------------

/// Likwiduje `n` etatów: najpierw wolne, potem zajęte (WP8).
///
/// Zajęty etat znika razem z pracą swojego pracownika — i to jest treść eksperymentu:
/// „usunięcie 20 % miejsc pracy" znaczy, że miasto **traci pojemność**, a nie że ktoś
/// zwalnia po kolei. Pracownik zostaje bezrobotny, jego gospodarstwo zaczyna liczyć
/// miesiące bez pracy i po trzech wyjeżdża (§5.7). Ofiary bierze się od najwyższego
/// indeksu encji, czyli deterministycznie i niezależnie od kolejności czegokolwiek.
///
/// Rynek pracy — kto kogo zwalnia i za ile — należy do M7. To jest narzędzie testu
/// regulatora, a nie mechanika gospodarcza.
pub fn shock_retire_jobs(world: &mut World, n: usize) -> u32 {
    let zostalo = world.resource_mut::<Vacancies>().retire_jobs(n);
    let mut zlikwidowane = (n - zostalo) as u32;
    if zostalo == 0 {
        return zlikwidowane;
    }

    let mut zatrudnieni: Vec<Entity> = world
        .resource::<Population>()
        .citizens()
        .iter()
        .copied()
        .filter(|e| {
            world
                .get::<Employment>(*e)
                .is_some_and(Employment::is_employed)
        })
        .collect();
    zatrudnieni.reverse();
    for e in zatrudnieni.into_iter().take(zostalo) {
        if let Some(emp) = world.get_mut::<Employment>(e) {
            emp.site = Employment::NO_SITE;
            emp.work_days = 0;
            emp.flags |= Employment::FLAG_UNEMPLOYED;
        }
        world.resource_mut::<Vacancies>().drop_job();
        zlikwidowane += 1;
    }
    zlikwidowane
}

// -- usamodzielnienie: zawor nadwyzki urodzen -----------------------------------

/// Dorosły wyprowadza się od rodziców (§5.7).
///
/// Dwa wyjścia i to jest cały regulator od strony podaży ludzi: jeśli w mieście jest
/// pustostan, dorosły zakłada własne gospodarstwo i zostaje; jeśli nie ma ani lokalu,
/// ani pracy — **wyjeżdża sam**. Bez tego drugiego wyjścia nadwyżka urodzeń zostawałaby
/// w gospodarstwach rodziców na zawsze: gospodarstwo z jednym pracującym rodzicem nie
/// jest „bez pracy", więc reguła odpływu gospodarstw nigdy by go nie ruszyła,
/// a populacja rosłaby ponad pojemność miasta bez żadnego hamulca.
fn usamodzielnienie(world: &mut World, day: u64, raport: &mut MigrationReport) {
    let tabela = world.resource::<DemographyTable>().clone();
    let ages = tabela.ages();
    let params = tabela.migration().clone();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();
    let mut cmd = CommandBuffer::new(demography_system_id());
    let mut wyjechali = 0u32;

    for e in spis {
        let Some(id) = world.get::<Identity>(e).copied() else {
            continue;
        };
        if id.age_years(day as i32) < i32::from(ages.leave_nest) {
            continue;
        }
        if world
            .get::<Lifecycle>(e)
            .is_some_and(|l| l.partner != Lifecycle::NO_PARTNER)
        {
            continue;
        }
        // Sam w gospodarstwie = już jest u siebie.
        let Some(hh_e) = demography::household_by_index(world, id.household) else {
            continue;
        };
        let Some(hh) = world.get::<Household>(hh_e).copied() else {
            continue;
        };
        if hh.size <= 1 {
            continue;
        }

        let mut r = rng(
            world.seed,
            StreamId::Migration,
            e.index(),
            stream_key(day, K_NEST),
        );
        if !r.gen_bool_permille(params.need_leave_permille * 4) {
            continue;
        }

        let dzielnica = world.get::<Residence>(e).map_or(u16::MAX, |r| r.district);
        if world.resource::<Vacancies>().has_home_in(dzielnica)
            && zaloz_gospodarstwo(world, e, day, Czesci::Domownicy).is_some()
        {
            raport.left_nest += 1;
            continue;
        }
        // Brak lokalu. Kto ma pracę, czeka; kto nie ma — wyjeżdża.
        if world
            .get::<Employment>(e)
            .is_some_and(Employment::is_employed)
        {
            continue;
        }
        demography::day::opusc_gospodarstwo(world, e, day, &mut cmd);
        demography::month::wyprowadz_mieszkanca(world, e, &mut cmd);
        wyjechali += 1;
        raport.reasons.push((
            e.index(),
            DecisionReason::MigrationDecision {
                kind: MigrationKind::LeftJobless,
                months_jobless: 0,
            },
        ));
    }

    if wyjechali > 0 {
        raport.left_alone = wyjechali;
        raport.left_citizens += wyjechali;
        world.resource_mut::<Population>().departures += u64::from(wyjechali);
        magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
    }
}

/// Na ile części dzieli się majątek gospodarstwa, kiedy ktoś się z niego wyprowadza.
///
/// Dwie wartości, bo dwie sytuacje i dwie różne prawdy o tym, kto jest stroną podziału:
/// wyprowadzka z gniazda dzieli na tylu, ilu jest domowników (`R2-WP8`, „udział `1/n`"),
/// a rozstanie na dwoje — dzieci zostają z jednym z rodziców, ale nie są stroną umowy
/// majątkowej. Jedna wartość dla obu byłaby regułą, której nikt nie zapisał.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Czesci {
    /// `1/rozmiar` — wyprowadzka z gniazda.
    Domownicy,
    /// `1/2` — rozstanie pary.
    Polowa,
}

/// Udział wyprowadzającego się w płynnym majątku jego gospodarstwa (`R2-WP8`).
///
/// `czesci` podaje wołający, bo tylko on wie, ile stron ma podział: wyprowadzka
/// z gniazda dzieli na tylu, ilu jest domowników, a rozstanie — na dwoje, bo dzieci
/// nie są stroną umowy majątkowej. [`Czesci::Domownicy`] rozwiązuje się na miejscu,
/// żeby liczba nigdy nie pochodziła z pola przeczytanego po wypisaniu ze składu.
/// Reszta z dzielenia zostaje w gospodarstwie dzielącym.
fn wydziel_udzial(world: &mut World, citizen: Entity, czesci: Czesci) -> household::Purse {
    let Some(hh_idx) = world.get::<Identity>(citizen).map(|i| i.household) else {
        return household::Purse::default();
    };
    let Some(hh_e) = demography::household_by_index(world, hh_idx) else {
        return household::Purse::default();
    };
    world
        .get_mut::<Household>(hh_e)
        .map(|h| {
            let n = match czesci {
                Czesci::Domownicy => h.size,
                Czesci::Polowa => 2,
            };
            h.split_off(n)
        })
        .unwrap_or_default()
}

/// Zakłada gospodarstwo dla jednego mieszkańca: wypisuje go ze starego, bierze
/// pustostan (jeśli jest) i przenosi pod nowy adres.
///
/// Gospodarstwo bez lokalu jest dopuszczalnym stanem przejściowym — migracja daje mu
/// `settle_grace_days` na znalezienie dachu i dopiero potem wyprowadza z miasta (§5.7).
/// Blokowanie rozstań i usamodzielnień brakiem pustostanu byłoby regułą, której nikt
/// nie zapisał, a która cicho zamrażałaby demografię przy zapełnionym mieście.
pub fn zaloz_gospodarstwo(
    world: &mut World,
    citizen: Entity,
    day: u64,
    czesci: Czesci,
) -> Option<Entity> {
    let dzielnica = world.get::<Residence>(citizen).map_or(0, |r| r.district);
    let home = world.resource_mut::<Vacancies>().take_home_in(dzielnica)?;
    // Udział w majątku zdejmuje się **przed** wypisaniem ze składu (`R2-WP8`):
    // po `opusc_gospodarstwo` gospodarstwo o rozmiarze 1 jest już rozwiązane,
    // a jego saldo — na koncie technicznym.
    let udzial = wydziel_udzial(world, citizen, czesci);
    let mut cmd = CommandBuffer::new(demography_system_id());
    demography::day::opusc_gospodarstwo(world, citizen, day, &mut cmd);
    magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
    let (building, unit, district) = (home.building, home.unit, home.district);

    let hh_e = world
        .spawn()
        .with(Household {
            district,
            building,
            unit,
            flags: Household::FLAG_ACTIVE,
            stock: [7; STOCK_CAT_COUNT],
            cash: udzial.cash,
            bank: udzial.bank,
            savings: udzial.savings,
            ..Household::default()
        })
        .id();
    world.resource_mut::<Population>().add_household(hh_e);
    let hh_idx = hh_e.index();

    let mut hh = world.get::<Household>(hh_e).copied().unwrap_or_default();
    {
        let ov = world.resource_mut::<HouseholdOverflow>();
        // Świeże gospodarstwo z jednym członkiem — limitu nie da się tu przekroczyć.
        // Wywołanie **poza** asercją, bo `debug_assert!` nie oblicza argumentu
        // w release — patrz komentarz w `spawn_household_aged`.
        let dopisany = household::add_member(hh_idx, &mut hh, ov, citizen.index());
        debug_assert!(dopisany);
    }
    demography::przeklasyfikuj(world, hh_idx, &mut hh, day);
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh;
    }
    if let Some(id) = world.get_mut::<Identity>(citizen) {
        id.household = hh_idx;
    }
    if let Some(res) = world.get_mut::<Residence>(citizen) {
        res.building = building;
        res.unit = unit;
        res.district = district;
    }
    Some(hh_e)
}

/// Gospodarstwo pochodne **pod tym samym adresem** (`R2-WP3`).
///
/// Jedyny wołający to poród do gospodarstwa o pełnym składzie. Różnice wobec
/// [`zaloz_gospodarstwo`] są dwie i obie wynikają z tego, że nikt się nie wyprowadza:
/// lokal nie pochodzi z puli pustostanów, tylko jest cudzy, a gospodarstwo dostaje
/// [`Household::FLAG_OVERCROWDED`], żeby przy rozwiązaniu nie oddało do puli lokalu,
/// którego nigdy z niej nie wzięło.
///
/// Zapasu spiżarni nie dostaje: dzieli kuchnię z gospodarstwem pierwotnym, a siedem
/// dób zapasu z niczego byłoby towarem stworzonym przez podział encji.
pub fn zaloz_gospodarstwo_pochodne(
    world: &mut World,
    citizen: Entity,
    gospodarz: &Household,
    day: u64,
) -> Option<Entity> {
    let hh_e = world
        .spawn()
        .with(Household {
            district: gospodarz.district,
            building: gospodarz.building,
            unit: gospodarz.unit,
            flags: Household::FLAG_ACTIVE | Household::FLAG_OVERCROWDED,
            ..Household::default()
        })
        .id();
    world.resource_mut::<Population>().add_household(hh_e);
    let hh_idx = hh_e.index();

    let mut hh = world.get::<Household>(hh_e).copied().unwrap_or_default();
    {
        let ov = world.resource_mut::<HouseholdOverflow>();
        // Poza asercją — `debug_assert!` nie oblicza argumentu w release.
        let dopisany = household::add_member(hh_idx, &mut hh, ov, citizen.index());
        debug_assert!(dopisany);
    }
    demography::przeklasyfikuj(world, hh_idx, &mut hh, day);
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh;
    }
    if let Some(id) = world.get_mut::<Identity>(citizen) {
        id.household = hh_idx;
    }
    Some(hh_e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atrakcyjnosc_spada_z_bezrobociem_i_rosnie_z_bezpieczenstwem() {
        let spokojne = attractiveness(50, 80);
        let bezrobotne = attractiveness(300, 80);
        assert!(bezrobotne < spokojne, "{bezrobotne} vs {spokojne}");
        assert!(attractiveness(50, 20) < spokojne);
        // Skala jest domknięta — regulator nie dostaje ujemnego wzmocnienia.
        assert_eq!(attractiveness(1000, 0), 0);
        assert!(attractiveness(0, 100) <= 100);
    }

    #[test]
    fn pula_oddaje_i_przyjmuje_miejsca_bez_gubienia_sum() {
        let dom = |i: u32| HomeSlot {
            building: i,
            unit: 0,
            district: 0,
            value: Money(100),
        };
        let etat = |i: u32| JobSlot {
            site: i,
            role: 0,
            shift: 1,
            work_days: 0b1_1111,
            district: 0,
            wage_monthly: Money(300_000),
        };
        let mut v = Vacancies::new(vec![dom(1), dom(2), dom(3)], vec![etat(1), etat(2)]);
        assert_eq!((v.free_homes(), v.free_jobs()), (3, 2));
        assert_eq!(v.homes_total(), 3);

        let d = v.take_home().expect("pustostan");
        assert_eq!(v.free_homes(), 2);
        assert_eq!(v.homes_total(), 3, "zajęcie lokalu zmieniło liczbę lokali");
        v.release_home(d);
        assert_eq!(v.free_homes(), 3);

        // Likwidacja etatów: zdejmuje z wolnych i zmniejsza pulę, zajętych nie rusza.
        let _ = v.take_job();
        assert_eq!(v.retire_jobs(1), 0);
        assert_eq!((v.free_jobs(), v.jobs_total()), (0, 1));
        assert_eq!(v.retire_jobs(5), 5, "zajęty etat zniknął mimo obsady");
    }

    #[test]
    fn niezasiedlone_zapamietuje_tylko_problemy() {
        let mut u = Unsettled::new();
        assert!(u.is_empty());
        u.set(
            7,
            UnsettledState {
                jobless_months: 2,
                homeless_since: u32::MAX,
            },
        );
        assert_eq!(u.len(), 1);
        assert_eq!(u.get(7).jobless_months, 2);
        // Gospodarstwo, któremu się poprawiło, znika z mapy — inaczej rosłaby
        // do rozmiaru miasta i zostawała tam na sto lat.
        u.set(
            7,
            UnsettledState {
                jobless_months: 0,
                homeless_since: u32::MAX,
            },
        );
        assert!(u.is_empty());
        assert_eq!(u.get(7).homeless_since, u32::MAX);
    }
}
