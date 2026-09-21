//! Wpięcie rynku pracy w świat: zasób, system doby i port nad ECS-em (M7b).
//!
//! # Dlaczego osobny system, a nie krok w `economy.Market`
//!
//! Rynek pracy rozstrzyga się **raz na dobę**, a rynek detaliczny co minutę. Wsadzenie
//! go do `MarketSystem` znaczyłoby 1439 sprawdzeń „czy już" na każdą jedną robotę,
//! w systemie, który i tak jest najgorętszy w całej symulacji. Kadencja `EveryDay`
//! kosztuje jeden poziom harmonogramu raz na dobę i nic poza tym.
//!
//! Kolejność wobec `firms.Firm` jest **zadeklarowana jawnie** (`K-42`), a nie wzięta
//! z hasza nazwy: lista płac ma domykać miesiąc na obsadzie, którą rynek pracy już
//! ustalił, a nie na wczorajszej.
//!
//! # `labor_pct` — miejsce, w którym praca staje się produkcją
//!
//! Po każdej dobie rynku pokrycie etatowe zakładu ląduje w `PlantSite::labor_pct`
//! (`K-44`). To domyka `AT-2` z M7a: do tej pory pisał je most stawiający miasto,
//! **raz**, i była to liczba zamarznięta — zakład, z którego odeszła połowa załogi,
//! mielił dalej tyle samo.

use magnat_agents::{
    Employment as AgentEmployment, Identity, Needs, Personality, Residence, ShiftKind, Skills,
    Vitals,
};
use magnat_core::{Cadence, CitizenId, DistrictId, JobRoleId, Money, NeedKind, SiteId, TraitId, Q};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};
use magnat_firms::{Firms, RoleTable};

use super::{LaborMarket, PersonFacts, Workforce};

/// Uchwyt rynku pracy w świecie.
///
/// `Option` w środku, bo system musi na czas kroku **wyjąć** rynek ze świata: krok
/// dotyka naraz rejestru firm i komponentów mieszkańców, a dwóch pożyczek `&mut World`
/// naraz nie ma. Ten sam wzorzec, którym `sim/agents` wyjmuje `AgentSources`.
#[derive(Default)]
pub struct LaborHandle(Option<LaborMarket>);

impl LaborHandle {
    #[must_use]
    pub fn new(m: LaborMarket) -> LaborHandle {
        LaborHandle(Some(m))
    }

    #[must_use]
    pub fn get(&self) -> Option<&LaborMarket> {
        self.0.as_ref()
    }

    pub fn get_mut(&mut self) -> Option<&mut LaborMarket> {
        self.0.as_mut()
    }

    fn take(&mut self) -> Option<LaborMarket> {
        self.0.take()
    }

    fn put(&mut self, m: LaborMarket) {
        self.0 = Some(m);
    }
}

impl magnat_core::HashState for LaborHandle {
    fn hash_state(&self, h: &mut magnat_core::StateHasher) {
        match &self.0 {
            // Rynek wyjęty ze świata zdarza się wyłącznie **wewnątrz** kroku systemu,
            // a hash liczy się na granicy ticku. Gdyby kiedyś trafił tu pusty uchwyt,
            // ma dać inny hash niż rynek pusty — stąd znacznik.
            None => h.write_u8(0),
            Some(m) => {
                h.write_u8(1);
                m.hash_state(h);
            }
        }
    }
}

/// Wpina rynek pracy do świata i do funkcji haszującej stan (00 §3.6).
pub fn register_labor(world: &mut World, market: LaborMarket) {
    world.insert_resource(LaborHandle::new(market));
    world.register_resource_hash::<LaborHandle>();
}

/// Doba rynku pracy.
pub struct LaborSystem {
    desc: SystemDesc,
}

impl LaborSystem {
    #[must_use]
    pub fn new() -> LaborSystem {
        LaborSystem {
            desc: SystemDesc::new("economy.Labor", Cadence::EveryDay)
                .exclusive()
                .after_if_present(SystemId::from_name("firms.Firm"))
                // Zgon, emerytura i wyjazd z miasta dzieją się w `agents.Society`,
                // a domyka je tutaj `reconcile` (`R2-WP9`). Do R2 kolejność brała się
                // z hasha nazwy systemu, bo obydwa są wyłączne i żaden nie deklarował
                // krawędzi — czyli moment, w którym rynek pracy dowiaduje się o zgonie,
                // był przypadkiem. `after_if_present`, a nie `after` (`K-51`):
                // scenariusz `m7labor` stawia rynek pracy bez świata mieszkańców.
                .after_if_present(SystemId::from_name("agents.Society")),
        }
    }
}

impl Default for LaborSystem {
    fn default() -> LaborSystem {
        LaborSystem::new()
    }
}

impl System for LaborSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let tick = ctx.tick;
        let seed = ctx.world().seed;
        let Some(mut market) = ctx
            .world_mut()
            .get_resource_mut::<LaborHandle>()
            .and_then(LaborHandle::take)
        else {
            return;
        };
        let Some(mut firms) = ctx
            .world_mut()
            .get_resource_mut::<Firms>()
            .map(std::mem::take)
        else {
            ctx.world_mut().resource_mut::<LaborHandle>().put(market);
            return;
        };
        {
            let mut people = WorldWorkforce {
                world: ctx.world_mut(),
            };
            market.step_day(&mut firms, &mut people, seed, tick);
        }
        write_labor_pct(ctx.world_mut(), &firms, market.roles());
        // Doba badań jedzie **za** rynkiem pracy i to jest kolejność, nie przypadek:
        // tempo liczy się z obsady laboratorium, więc musi widzieć obsadę po dzisiejszych
        // przyjęciach i odejściach. Osobnego systemu nie ma z rozmysłu — rejestr firm
        // jest już wyjęty z ECS-u, a drugie wyjęcie w tej samej dobie nie kupiłoby nic
        // poza kolejnym poziomem harmonogramu (`AP-4`).
        crate::rnd::step_day(ctx.world_mut(), &mut firms, market.roles(), tick);
        *ctx.world_mut().resource_mut::<Firms>() = firms;
        ctx.world_mut().resource_mut::<LaborHandle>().put(market);
    }
}

/// Przepisuje pokrycie etatowe zakładów do `PlantSite` (`K-44`, `AT-2`).
///
/// Zakład bez odpowiednika w łańcuchu (sklep, biuro) po prostu nie ma dokąd tej liczby
/// oddać — i to jest w porządku, bo nic tam z niej nie wynika.
fn write_labor_pct(world: &mut World, firms: &Firms, roles: &RoleTable) {
    let Some(chain) = world.get_resource::<magnat_supply::ChainHandle>().cloned() else {
        return;
    };
    let plan = super::hr::labor_coverage(firms, roles, &WorldWorkforce { world });
    let mut ch = chain.lock();
    for (id, pct) in plan {
        if let Some(p) = ch.plant.get_mut(id) {
            p.labor_pct = pct;
        }
    }
}

/// Wiek, od którego mieszkaniec liczy się do siły roboczej, i wiek, do którego się liczy.
///
/// Granice wieku produkcyjnego czyta się z `data/demography/demography.ron`
/// (`ages.labour_force`, `K-60`) — **nie** ze stałej w kodzie.
///
/// Granice są po to, żeby stopa bezrobocia miała ten sam mianownik co w statystyce
/// publicznej: dziecko bez pracy nie jest bezrobotne. Emerytura ma własną flagę
/// i to ona rozstrzyga o górnej granicy — te liczby są tylko zabezpieczeniem
/// dla mieszkańców, którym M3 flagi nie nadał.
fn wiek_produkcyjny(world: &World) -> std::ops::RangeInclusive<i32> {
    let lf = world
        .resource::<magnat_agents::DemographyTable>()
        .ages()
        .labour_force;
    i32::from(lf.min)..=i32::from(lf.max)
}

/// Port rynku pracy nad ECS-em — implementacja produkcyjna [`Workforce`].
pub struct WorldWorkforce<'a> {
    world: &'a mut World,
}

impl<'a> WorldWorkforce<'a> {
    #[must_use]
    pub fn new(world: &'a mut World) -> WorldWorkforce<'a> {
        WorldWorkforce { world }
    }

    /// Gospodarstwo mieszkańca — tam trafia jego dochód.
    fn gospodarstwo(&self, c: CitizenId) -> Option<magnat_core::Entity> {
        let id = self.world.get::<Identity>(c.0)?;
        magnat_agents::demography::household_by_index(self.world, id.household)
    }

    /// Przesuwa miesięczny dochód gospodarstwa o `delta`.
    ///
    /// `Household.income_monthly` jest **denormalizacją** — źródłem prawdy o płacy
    /// zostaje umowa po stronie firmy (`D1`). Bez tego kroku rynek pracy byłby
    /// niewidoczny dla budżetów M5: człowiek traciłby pracę i dalej wydawał tyle samo.
    fn przesun_dochod(&mut self, c: CitizenId, delta: i64) {
        let Some(h) = self.gospodarstwo(c) else {
            return;
        };
        self.przesun_dochod_w(h, delta);
    }

    /// Ta sama operacja, ale na gospodarstwie wskazanym **wprost** (`R2-WP9`).
    ///
    /// Odejście z etatu bywa skutkiem zgonu albo wyjazdu z miasta, a wtedy encji
    /// mieszkańca już nie ma i [`WorldWorkforce::gospodarstwo`] nie ma czego odczytać.
    /// Indeks gospodarstwa niesie wtedy umowa (`Employment::household`).
    fn przesun_dochod_w(&mut self, h: magnat_core::Entity, delta: i64) {
        if let Some(gd) = self.world.get_mut::<magnat_agents::Household>(h) {
            gd.income_monthly = Money(gd.income_monthly.get().saturating_add(delta).max(0));
        }
    }
}

impl Workforce for WorldWorkforce<'_> {
    fn facts(&self, c: CitizenId) -> Option<PersonFacts> {
        let id = self.world.get::<Identity>(c.0)?;
        if !id.is_alive() {
            return None;
        }
        let vitals = *self.world.get::<Vitals>(c.0)?;
        let emp = *self.world.get::<AgentEmployment>(c.0)?;
        let district = self
            .world
            .get::<Residence>(c.0)
            .map_or(DistrictId(0), |r| DistrictId(r.district));
        let osobowosc = self
            .world
            .get::<Personality>(c.0)
            .copied()
            .unwrap_or_default();
        // `is_employed`, nie `has_job`: uczeń ma w `site` szkołę (`R2-WP1`), a pod
        // `has_job` wchodził do faktów jako zatrudniony — z rolą odziedziczoną po
        // losowaniu cech, w zakładzie, który jest szkołą.
        let job = emp
            .is_employed()
            .then(|| (site_id(emp.site), JobRoleId(emp.role)));
        let best_role = self.world.get::<Skills>(c.0).and_then(najlepszy_zawod);
        let chory = self
            .world
            .get::<magnat_agents::Lifecycle>(c.0)
            .is_some_and(magnat_agents::Lifecycle::is_ill);
        // Skutki progowe deprywacji (`R2-WP16`). Świat bez tabeli potrzeb albo
        // mieszkaniec bez komponentu `Needs` nie naciska — zero jest tu treścią,
        // nie brakiem danych.
        let deprivation = match (
            self.world.get::<magnat_agents::Needs>(c.0),
            self.world.get_resource::<magnat_agents::NeedTable>(),
        ) {
            (Some(n), Some(t)) => magnat_agents::pressure(n, t),
            _ => magnat_agents::DeprivationPressure::default(),
        };
        Some(PersonFacts {
            vitals,
            district,
            // Promile deprywacji na punkty skali `Q`: 100 promili = +10 punktów
            // ambicji, czyli −100 bp progu zmiany pracy. Przelicznik jest tutaj,
            // bo `sim/agents` nie wie, co znaczy „ambicja" dla rynku pracy.
            ambition: Q::new(
                osobowosc
                    .get(TraitId::Ambition)
                    .get()
                    .saturating_add(
                        u8::try_from(deprivation.ambition_gain_permille / 10).unwrap_or(u8::MAX),
                    )
                    .min(100),
            ),
            loyalty: osobowosc.get(TraitId::Loyalty),
            job,
            best_role,
            on_sick_leave: chory,
            deprivation,
        })
    }

    fn skill_in(&self, c: CitizenId, role: JobRoleId) -> Q {
        self.world
            .get::<Skills>(c.0)
            .map_or(Q::MIN, |s| s.level_in(role.0))
    }

    fn job_seekers(&mut self, day: u32, on_the_job_every: u16, out: &mut Vec<CitizenId>) {
        out.clear();
        let dzis = day as i32;
        let wiek = wiek_produkcyjny(self.world);
        let co_ile = u32::from(on_the_job_every.max(1));
        let dzisiejsza_zmiana = day % co_ile;
        let mut kandydaci: Vec<(u64, CitizenId)> = Vec::new();
        for (e, id, emp) in self
            .world
            .query::<(magnat_core::Entity, &Identity, &AgentEmployment), ()>()
            .iter()
        {
            // Zatrudnieni rozglądają się rzadko i **rozproszeni po dobach** — inaczej
            // całe miasto składałoby aplikacje tego samego dnia, a rynek pracy dostawał
            // pik raz na dziesięć dób zamiast równego strumienia.
            if emp.is_employed() && e.index() % co_ile != dzisiejsza_zmiana {
                continue;
            }
            if !w_sile_roboczej(id, emp, dzis, &wiek) {
                continue;
            }
            kandydaci.push((e.to_bits(), CitizenId(e)));
        }
        // Kolejność zgłoszeń wchodzi do hasha, a kolejność archetypów w ECS nie jest
        // niczym gwarantowanym — stąd sortowanie po tożsamości encji (`AR-16`).
        kandydaci.sort_unstable();
        out.extend(kandydaci.into_iter().map(|(_, c)| c));
    }

    fn hire(
        &mut self,
        c: CitizenId,
        site: SiteId,
        role: JobRoleId,
        shift: ShiftKind,
        work_days: u8,
        wage: Money,
    ) {
        let district = self.world.get::<Residence>(c.0).map_or(0, |r| r.district);
        if let Some(e) = self.world.get_mut::<AgentEmployment>(c.0) {
            e.site = site.entity().index();
            e.role = role.0;
            e.shift = shift as u8;
            // Maska dni **z oferty**, nie stała (`R2-WP37`): ta sama zmiana poranna
            // wypada raz w poniedziałek–piątek, a raz we wtorek–sobotę.
            e.work_days = work_days;
            e.flags &= !AgentEmployment::FLAG_UNEMPLOYED;
        }
        // Pula wakatów miasta jest wejściem regulatora napływu (M3c §5.7), więc
        // zajęty etat musi z niej zejść tak samo, jak zwolniony do niej wraca.
        if let Some(v) = self
            .world
            .get_resource_mut::<magnat_agents::migration::Vacancies>()
        {
            v.take_job_in(district);
        }
        self.przesun_dochod(c, wage.get());
    }

    fn household_of(&self, c: CitizenId) -> u32 {
        self.world
            .get::<Identity>(c.0)
            .map_or(magnat_firms::Employment::NO_HOUSEHOLD, |id| id.household)
    }

    fn release(&mut self, c: CitizenId, wage: Money, household: u32) {
        magnat_agents::migration::release_job_of(self.world, c.0);
        // Gospodarstwo **z umowy**, a nie z komponentu odchodzącego (`R2-WP9`):
        // przy zgonie i przy wyjeździe z miasta encji już nie ma, a płaca zostawała
        // wtedy w dochodzie gospodarstwa na zawsze. Zapis w umowie pochodzi z chwili
        // zatrudnienia, więc `+wage` i `−wage` trafiają zawsze po tej samej stronie.
        let cel = magnat_agents::demography::household_by_index(self.world, household)
            .or_else(|| self.gospodarstwo(c));
        if let Some(h) = cel {
            self.przesun_dochod_w(h, -wage.get());
        }
    }

    fn raise_wage(&mut self, c: CitizenId, from: Money, to: Money) {
        self.przesun_dochod(c, to.get() - from.get());
    }

    fn add_need(&mut self, c: CitizenId, need: NeedKind, points: u8) {
        if let Some(n) = self.world.get_mut::<Needs>(c.0) {
            let teraz = n.get(need).get();
            n.set(need, Q::new(teraz.saturating_add(points).min(100)));
        }
    }

    fn set_skill(&mut self, c: CitizenId, role: JobRoleId, level: Q) {
        let Some(s) = self.world.get_mut::<Skills>(c.0) else {
            return;
        };
        if let Some(slot) = s.0.iter_mut().find(|slot| slot.role == role.0) {
            slot.level = level.get();
            return;
        }
        // Brak miejsca: zawód wyparty jest tym, w którym człowiek jest najsłabszy.
        // Cztery sloty to decyzja M3 i M7 jej nie podważa (`Skills`: „kto ma więcej,
        // trzyma resztę w bocznej mapie po stronie M7" — ta mapa nie jest potrzebna,
        // dopóki szkolenie dotyczy zawodu wykonywanego).
        if let Some(slot) = s.0.iter_mut().min_by_key(|slot| slot.level) {
            if slot.level < level.get() {
                slot.role = role.0;
                slot.level = level.get();
                slot.decay = 10;
            }
        }
    }

    fn labour_force(&mut self, day: u32) -> (u32, u32) {
        let dzis = day as i32;
        let wiek = wiek_produkcyjny(self.world);
        let mut sila = 0;
        let mut bez_pracy = 0;
        for (id, emp) in self
            .world
            .query::<(&Identity, &AgentEmployment), ()>()
            .iter()
        {
            if !w_sile_roboczej(id, emp, dzis, &wiek) {
                continue;
            }
            sila += 1;
            if !emp.is_employed() {
                bez_pracy += 1;
            }
        }
        (sila, bez_pracy)
    }
}

/// `SiteId` z klucza miejsca w komponencie mieszkańca.
///
/// Klucz jest ten sam po obu stronach od poprawki mostu M7a (`AU-1`): zakład ma
/// **jeden** identyfikator w przestrzeni gospodarki, a nie osobny u M5, M6 i M7.
fn site_id(key: u32) -> SiteId {
    SiteId(magnat_core::Entity::new(key, std::num::NonZeroU32::MIN))
}

/// Czy ten mieszkaniec w ogóle należy do siły roboczej.
///
/// Funkcja wolna, a nie metoda, bo woła się ją w środku zapytania ECS — a zapytanie
/// trzyma świat na wyłączność i drugiego odczytu przez `&self` już nie puści.
fn w_sile_roboczej(
    id: &Identity,
    emp: &AgentEmployment,
    dzis: i32,
    wiek: &std::ops::RangeInclusive<i32>,
) -> bool {
    if !id.is_alive() {
        return false;
    }
    if emp.flags
        & (AgentEmployment::FLAG_PUPIL
            | AgentEmployment::FLAG_STUDENT
            | AgentEmployment::FLAG_RETIRED)
        != 0
    {
        return false;
    }
    wiek.contains(&id.age_years(dzis))
}

/// Zawód, w którym mieszkaniec jest najlepszy.
fn najlepszy_zawod(s: &Skills) -> Option<JobRoleId> {
    s.0.iter()
        .filter(|slot| slot.role != Skills::ROLE_NONE && slot.level > 0)
        .max_by_key(|slot| slot.level)
        .map(|slot| JobRoleId(slot.role))
}
