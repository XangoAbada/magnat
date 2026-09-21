//! Migawka mieszkańca: wszystko, czego planer potrzebuje o jednej osobie (M3d §5.12).
//!
//! Wydzielone z `systems.rs` w `R2-WP4` bez zmiany zachowania. Powód jest ten sam,
//! dla którego reguła strukturalna w ogóle istnieje: `systems.rs` niósł dwa tematy —
//! **czym jest doba jednego mieszkańca jako dane** i **które systemy ECS ją wykonują** —
//! a opieka zewnętrzna (`SocialIndex::wards`) przeważyła pierwszy z nich ponad próg.
//! Dzieli się pliki, w których są dwa tematy, a nie pliki, które są długie.

use crate::components::{
    Employment, Identity, KnowledgeRef, Needs, Personality, Residence, Skills,
    Vitals, Wealth,
};
use crate::household::{self, Household, HouseholdOverflow, MemberView};
use crate::needs::NeedTable;
use crate::places::{site_of, PlaceProvider, TravelOracle};
use crate::planner::{HouseholdView, PlanCtx};
use crate::store::{Knowledge, KnowledgeSlab};
use crate::{demography, social};
use magnat_core::{CitizenId, DayOfWeek, Entity, HouseholdId, PlaceRef, STOCK_CAT_COUNT};
use magnat_ecs::World;

/// Wszystko, czego planer potrzebuje o mieszkańcu, jako **kopia**.
///
/// `PlanCtx` trzyma referencje, a systemy pracują na `&mut World` — bez migawki nie da
/// się jednocześnie czytać mieszkańca i pisać do areny planów. Kopia jest tania:
/// trzynaście komponentów to 140 B plus do 32 wpisów wiedzy.
///
/// Używa jej też karta inspekcji (M3d §5.11): odtworzenie planu z uzasadnieniami
/// wymaga dokładnie tego samego kontekstu, w którym plan powstał.
#[derive(Clone, Debug)]
pub struct CitizenSnapshot {
    pub citizen: Entity,
    pub identity: Identity,
    pub vitals: Vitals,
    pub needs: Needs,
    pub personality: Personality,
    pub residence: Residence,
    pub employment: Employment,
    pub skills: Skills,
    pub wealth: Wealth,
    pub household: Entity,
    pub stock: [u8; STOCK_CAT_COUNT],
    pub escorts: Vec<PlaceRef>,
    pub pickups: Vec<PlaceRef>,
    /// Dzieci bez odprowadzenia (`R2-WP3`) — patrz `HouseholdRoles::unescorted`.
    pub unescorted: u8,
    pub knowledge: Vec<Knowledge>,
    /// Sloty marek **z naniesionym zanikiem** (M10b §5.1). Zanik liczy się tu raz
    /// na migawkę, a nie przy każdym z kilkunastu kandydatów decyzji zakupowej.
    pub brands: crate::brand::BrandSlots,
    pub home: Option<PlaceRef>,
    pub work: Option<PlaceRef>,
    pub school: Option<PlaceRef>,
}

impl CitizenSnapshot {
    /// Migawka albo `None`, gdy encja nie jest żywym mieszkańcem.
    #[must_use]
    pub fn of(world: &World, citizen: Entity, day: u64) -> Option<CitizenSnapshot> {
        let identity = *world.get::<Identity>(citizen)?;
        if !identity.is_alive() {
            return None;
        }
        let employment = *world.get::<Employment>(citizen)?;
        let residence = *world.get::<Residence>(citizen)?;
        let kref = world
            .get::<KnowledgeRef>(citizen)
            .copied()
            .unwrap_or_default();
        let knowledge = world
            .resource::<KnowledgeSlab>()
            .entries(demography::knowledge_ref(&kref))
            .to_vec();
        let brands = crate::brand::slots_of(world, citizen, day);

        let hh = demography::household_by_index(world, identity.household);
        let (stock, mut escorts, mut pickups, mut unescorted) = match hh {
            Some(e) => role_places(world, e, citizen, day),
            None => ([0; STOCK_CAT_COUNT], Vec::new(), Vec::new(), 0),
        };
        let (e2, p2, u2) = opieka_zewnetrzna(world, citizen, day);
        escorts.extend(e2);
        pickups.extend(p2);
        unescorted = unescorted.saturating_add(u2);

        let uczen = employment.flags & Employment::FLAG_PUPIL != 0;
        let miejsce = site_of(&employment);
        Some(CitizenSnapshot {
            citizen,
            identity,
            vitals: *world.get::<Vitals>(citizen)?,
            needs: *world.get::<Needs>(citizen)?,
            personality: *world.get::<Personality>(citizen)?,
            residence,
            employment,
            skills: world.get::<Skills>(citizen).copied().unwrap_or_default(),
            wealth: world.get::<Wealth>(citizen).copied().unwrap_or_default(),
            household: hh.unwrap_or(citizen),
            stock,
            escorts,
            pickups,
            unescorted,
            knowledge,
            brands,
            home: crate::places::home_of(&residence),
            work: if uczen { None } else { miejsce },
            school: if uczen { miejsce } else { None },
        })
    }

    /// Kontekst planera nad migawką.
    #[must_use]
    pub fn ctx<'a>(
        &'a self,
        seed: u64,
        day: u64,
        needs: &'a NeedTable,
        places: &'a dyn PlaceProvider,
        travel: &'a dyn TravelOracle,
    ) -> PlanCtx<'a> {
        PlanCtx {
            seed,
            day,
            dow: DayOfWeek::from_day_index(day),
            citizen: crate::places::CitizenView {
                id: CitizenId(self.citizen),
                identity: &self.identity,
                vitals: &self.vitals,
                needs: &self.needs,
                personality: &self.personality,
                residence: &self.residence,
                today: day as i32,
                brands: crate::places::BrandView::new(self.brands.as_slice()),
            },
            household: HouseholdView {
                id: HouseholdId(self.household),
                stock: &self.stock,
                escorts: &self.escorts,
                pickups: &self.pickups,
                unescorted: self.unescorted,
            },
            employment: &self.employment,
            known: crate::places::KnowledgeView::new(&self.knowledge),
            needs,
            home: self.home.unwrap_or_default(),
            work: self.work,
            school: self.school,
            places,
            travel,
            max_task_travel_min: MAX_TASK_TRAVEL_MIN,
        }
    }
}

/// Zasięg osobisty zadania w minutach marszu (§5.4, ryzyko R6).
pub const MAX_TASK_TRAVEL_MIN: u16 = 30;

/// Zapas gospodarstwa oraz szkoły dzieci, które ten mieszkaniec odprowadza i odbiera.
///
/// Podział ról liczy `household::roles` ze składu; tłumaczenie „dziecko → jego szkoła"
/// jest tutaj, bo `roles` zwraca indeksy encji, a planer potrzebuje `PlaceRef`
/// (korekta E-19).
fn role_places(
    world: &World,
    hh: Entity,
    citizen: Entity,
    day: u64,
) -> ([u8; STOCK_CAT_COUNT], Vec<PlaceRef>, Vec<PlaceRef>, u8) {
    let Some(h) = world.get::<Household>(hh).copied() else {
        return ([0; STOCK_CAT_COUNT], Vec::new(), Vec::new(), 0);
    };
    let sklad = household::members_of(hh.index(), &h, world.resource::<HouseholdOverflow>());
    let widoki: Vec<MemberView> = sklad
        .iter()
        .filter_map(|m| {
            let c = demography::citizen_by_index(world, *m)?;
            Some(MemberView::new(
                *m,
                world.get::<Identity>(c)?,
                world.get::<Employment>(c)?,
                day as i32,
                false,
            ))
        })
        .collect();
    let ages = world.resource::<demography::DemographyTable>().ages();
    let opiekun = h
        .guardian_of()
        .and_then(|g| widok_czlonka(world, g, day));
    let role = household::roles(
        &widoki,
        i32::from(ages.escort),
        i32::from(ages.adult),
        day,
        opiekun,
    );

    let szkoly = |lista: &[u32]| -> Vec<PlaceRef> {
        lista
            .iter()
            .filter_map(|m| {
                let c = demography::citizen_by_index(world, *m)?;
                site_of(world.get::<Employment>(c)?)
            })
            .collect()
    };
    let escorted = role.escorted.as_slice();
    (
        h.stock,
        if role.escort == citizen.index() {
            szkoly(escorted)
        } else {
            Vec::new()
        },
        if role.pickup == citizen.index() {
            szkoly(escorted)
        } else {
            Vec::new()
        },
        role.unescorted,
    )
}

/// Widok członka po indeksie encji — jedno miejsce konwersji dla obu wołających.
fn widok_czlonka(world: &World, member: u32, day: u64) -> Option<MemberView> {
    let c = demography::citizen_by_index(world, member)?;
    Some(MemberView::new(
        member,
        world.get::<Identity>(c)?,
        world.get::<Employment>(c)?,
        day as i32,
        false,
    ))
}

/// Dzieci, które ten mieszkaniec odprowadza jako **opiekun prawny** cudzego
/// gospodarstwa (`R2-WP4`).
///
/// Bez tego kroku opiekun byłby nazwany w `Household.guardian` i nie robiłby nic:
/// migawka mieszkańca powstaje z jego własnego gospodarstwa, a podopieczni mieszkają
/// pod innym adresem. Indeks odwrotny (`SocialIndex::wards`) odbudowuje się co dobę,
/// bo opiekun powstaje przy zgonie, czyli w środku miesiąca.
fn opieka_zewnetrzna(
    world: &World,
    citizen: Entity,
    day: u64,
) -> (Vec<PlaceRef>, Vec<PlaceRef>, u8) {
    let wards: Vec<u32> = world
        .resource::<social::SocialIndex>()
        .wards(citizen.index())
        .iter()
        .map(|(_, hh)| *hh)
        .collect();
    let (mut escorts, mut pickups, mut unescorted) = (Vec::new(), Vec::new(), 0u8);
    for hh_idx in wards {
        let Some(hh_e) = demography::household_by_index(world, hh_idx) else {
            continue;
        };
        let (_, e, p, u) = role_places(world, hh_e, citizen, day);
        escorts.extend(e);
        pickups.extend(p);
        unescorted = unescorted.saturating_add(u);
    }
    (escorts, pickups, unescorted)
}

