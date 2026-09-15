//! Systemy ECS i ich częstotliwości (M3d §5.12).
//!
//! Tu warstwy M3a–M3c zostają wpięte w pętlę ticku. **Nic tu nie jest wymyślane od
//! nowa**: kolejność doby i miesiąca stoi w `society::step_day` (korekta E-16), plan
//! dnia w `planner`, spadek potrzeb w `needs`, ruch pieszy w `walk`. Systemy dodają
//! jedną rzecz — kiedy to się dzieje.
//!
//! **Cztery decyzje, których §5.12 nie przesądzało** (korekta E-3):
//!
//! 1. `DayPlannerSystem`, `ReplanSystem` i `NeedSatisfySystem` **nie są osobnymi
//!    systemami**, tylko obsługą zdarzeń wewnątrz [`DayLoopSystem`]. Powód jest
//!    w kontrakcie DES: kolejność zdarzeń w obrębie minuty wyznacza `order_key`
//!    (§6.4 — zamrożone), a trzy systemy przechodzące kolejkę osobno musiałyby ją
//!    rozdać trzy razy i pogodzić trzy kolejności. Rozdaje ją jeden.
//! 2. Systemy, które są gotową funkcją nad całym światem — pętla doby i rytm
//!    społeczeństwa — są **wyłączne** (`SystemDesc::exclusive`). Rozpisanie
//!    `demography::step_day` na deklaracje dostępu nie kupiłoby ani jednej krawędzi
//!    DAG, bo dotyka wszystkiego o mieszkańcu.
//! 3. Potrzeby i osobowość zostają przy zrównolegleniu po chunkach (M3a) — to one
//!    dotykają 400 tys. encji na tick i to one płacą za brak równoległości.
//! 4. **`PlanDay` nie jest rozrzucone 18:00–02:00**, tylko wypada w pierwszej minucie
//!    doby, którą planuje. §5.12 zakładało planowanie z wieczora na jutro, a to wymaga
//!    **dwóch** planów naraz: wykonywanego dziś i gotowego na jutro. Podwójne
//!    buforowanie areny planów zostało z M3 zdjęte po pomiarze pamięci (korekta D-1,
//!    288 B → 144 B na mieszkańca), więc plan na jutro zamazywałby plan wykonywany
//!    dziś — a łańcuch zdarzeń wskazywałby wtedy na sloty z innego dnia i kolejka
//!    odrzucałaby je asercją. Koszt spiętrzenia jest zmierzony i mały: plan to 0,8 µs,
//!    czyli 0,22 s na dobę gry przy 274 tys. mieszkańców — raz na 24 minuty realnego
//!    czasu przy prędkości 1×.

use crate::components::{
    AgentState, Employment, Identity, KnowledgeRef, Lod, Needs, Personality, PlanRef, Residence,
    Skills, Vitals, Wealth,
};
use crate::des::{EventKind, EventQueue, ReplanCause, SimEvent};
use crate::household::{self, Household, HouseholdOverflow, MemberView};
use crate::needs::NeedTable;
use crate::places::{
    site_of, FulfilOutcome, FulfilRequest, PlaceProvider, TravelOracle, TripRequest,
};
use crate::planner::{load_plan, plan_day, replan, store_plan, DayCanvas, HouseholdView, PlanCtx};
use crate::store::{Knowledge, KnowledgeSlab, PlanSlab, PlanSlot};
use crate::{demography, society};
use magnat_core::{
    ActivityKind, Cadence, CitizenId, DayOfWeek, Entity, HouseholdId, MinuteOfDay, NeedKind,
    PlaceRef, STOCK_CAT_COUNT,
};
use magnat_ecs::{System, SystemCtx, SystemDesc, World};

/// Źródła miejsc i podróży — jedyne wejście systemów do świata poza ECS.
///
/// Zasób jest pusty aż do zaludnienia miasta: dopóki nie ma katalogu miejsc, nie ma
/// dokąd chodzić. Systemy **wyjmują** go na czas obsługi minuty, bo `fulfil`
/// i `begin_trip` biorą `&mut self`, a jednocześnie potrzebny jest `&mut World`.
///
/// M5 podmienia `places` na indeks ofert. **M4b podmienił `travel`** (`Z-1`): pole
/// jest `Box<dyn TravelOracle>`, moduł `walk` nie istnieje, a implementację wnosi
/// `magnat-traffic`. Żadne wywołanie w planerze ani w pętli doby się przez to
/// nie zmieniło — o to chodziło w tym punkcie podmiany.
#[derive(Default)]
pub struct AgentSources(Option<Sources>);

pub struct Sources {
    pub places: Box<dyn PlaceProvider>,
    pub travel: Box<dyn TravelOracle>,
}

impl AgentSources {
    #[must_use]
    pub fn new(places: Box<dyn PlaceProvider>, travel: Box<dyn TravelOracle>) -> AgentSources {
        AgentSources(Some(Sources { places, travel }))
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.0.is_some()
    }

    #[must_use]
    pub fn get(&self) -> Option<&Sources> {
        self.0.as_ref()
    }

    fn take(&mut self) -> Option<Sources> {
        self.0.take()
    }

    fn put(&mut self, s: Sources) {
        self.0 = Some(s);
    }
}

/// Rejestruje zasoby i systemy podfazy M3d.
///
/// Osobno od `register` (M3a) i `register_society` (M3c) z tego samego powodu co tamte:
/// scenariusz, który nie uruchamia doby, nie płaci za nią ani bajtem hasha stanu.
pub fn register_day(world: &mut World) {
    world.insert_resource(AgentSources::default());
    world.insert_resource(DayStats::default());
    world.insert_resource(Trace::new());
    // `AgentSources` **nie wchodzi** do hasha stanu: katalog miejsc i estymator
    // odległości są danymi wejściowymi miasta, nie stanem symulacji — ta sama zasada
    // co przy `NeedTable` (M3a) i `DemographyTable` (M3c). `DayStats` to licznik
    // diagnostyczny i też nim nie jest.
}

// ── migawka mieszkańca ──────────────────────────────────────────────────────────

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
    pub knowledge: Vec<Knowledge>,
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

        let hh = demography::household_by_index(world, identity.household);
        let (stock, escorts, pickups) = match hh {
            Some(e) => role_places(world, e, citizen, day),
            None => ([0; STOCK_CAT_COUNT], Vec::new(), Vec::new()),
        };

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
            knowledge,
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
            },
            household: HouseholdView {
                id: HouseholdId(self.household),
                stock: &self.stock,
                escorts: &self.escorts,
                pickups: &self.pickups,
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
) -> ([u8; STOCK_CAT_COUNT], Vec<PlaceRef>, Vec<PlaceRef>) {
    let Some(h) = world.get::<Household>(hh).copied() else {
        return ([0; STOCK_CAT_COUNT], Vec::new(), Vec::new());
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
    let role = household::roles(&widoki, i32::from(ages.escort), i32::from(ages.adult), day);

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
    )
}

// ── pętla doby ──────────────────────────────────────────────────────────────────

/// Rdzeń §5.12: rozdanie zdarzeń minuty i ich obsługa.
///
/// Wzorzec łańcucha jest ten sprawdzony w M3b (korekta E-9): `StartActivity` slotu `k`
/// harmonogramuje `EndActivity` slotu `k` **oraz** `StartActivity` slotu `k+1`. Łańcuch
/// przez `EndActivity` by się urwał — sloty planu przylegają do siebie, więc koniec
/// jednego i początek następnego wypadają w tej samej minucie, a kubełek tej minuty
/// jest już rozdany (korekta D-14 do M3a).
pub struct DayLoopSystem {
    desc: SystemDesc,
    table: NeedTable,
    bufor: Vec<SimEvent>,
    canvas: DayCanvas,
}

/// Co się wydarzyło w pętli doby. Zasób, a nie pole systemu: `Schedule` wypożycza
/// systemy na czas wykonania, więc runner nie ma jak do nich zajrzeć — a liczby
/// z tego licznika są treścią raportu scenariusza i testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DayStats {
    pub events: u64,
    pub plans: u64,
    pub replans: u64,
    pub trips: u64,
    pub arrivals: u64,
    /// Przybycia później niż zakładał plan — i suma opóźnień w minutach.
    pub late: u64,
    pub late_minutes: u64,
    pub fulfilled: u64,
    pub refused: u64,
}

impl DayLoopSystem {
    #[must_use]
    pub fn new(world: &World) -> DayLoopSystem {
        DayLoopSystem {
            desc: SystemDesc::new("agents.DayLoop", Cadence::EveryMinute).exclusive(),
            table: world.resource::<NeedTable>().clone(),
            bufor: Vec::with_capacity(4096),
            canvas: DayCanvas::new(),
        }
    }
}

impl System for DayLoopSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let seed = ctx.seed;
        let world = ctx.world_mut();
        if !world.resource::<AgentSources>().is_ready() {
            return;
        }
        let Some(mut zrodla) = world.resource_mut::<AgentSources>().take() else {
            return;
        };
        let mut kolejka = std::mem::take(world.resource_mut::<EventQueue>());
        let mut stats = *world.resource::<DayStats>();

        kolejka.drain_minute(&mut self.bufor);
        // Zegar kolejki stoi na minucie **wlasnie rozdawanej** (korekta A-12), wiec
        // `begin_trip` liczy przybycie od tej samej minuty, od ktorej liczyl planer.
        let teraz = kolejka.now();
        let doba = u64::from(teraz) / 1440;

        // Zdarzenie na minute juz rozdana nie moze wrocic do kolejki - ta odrzuca je
        // asercja (korekta D-14). Obslugujemy je **w tej samej minucie**, dopisujac
        // na koniec listy: kolejnosc produkcji jest deterministyczna, wiec kolejnosc
        // obslugi tez. Dotyczy to planu, ktorego pierwszy slot zaczyna sie o polnocy,
        // i czynnosci przylegajacych do siebie bez luki.
        let mut do_obsluzenia: std::collections::VecDeque<SimEvent> =
            self.bufor.drain(..).collect();
        let sledzone = !world.resource::<Trace>().is_empty();
        while let Some(e) = do_obsluzenia.pop_front() {
            stats.events += 1;
            let Some(kto) = demography::citizen_by_index(world, e.actor) else {
                continue;
            };
            if sledzone && world.resource::<Trace>().is_watched(e.actor) {
                let aktywnosc = world.get::<AgentState>(kto).map_or(0, |a| a.activity);
                let slot = slot_planu(world, kto, e.slot);
                world.resource_mut::<Trace>().record(
                    e.actor,
                    TraceEntry {
                        minute: e.time,
                        kind: e.kind,
                        slot: e.slot,
                        activity: slot.map_or(aktywnosc, |s| s.kind),
                    },
                );
            }
            match e.event_kind() {
                Some(EventKind::PlanDay) => {
                    if zaplanuj(
                        world,
                        kto,
                        doba,
                        seed,
                        &self.table,
                        &zrodla,
                        &mut self.canvas,
                    ) {
                        stats.plans += 1;
                        if let Some(slot) = slot_planu(world, kto, 0) {
                            wstaw(
                                &mut kolejka,
                                &mut do_obsluzenia,
                                teraz,
                                SimEvent::new(
                                    (doba * 1440) as u32 + u32::from(slot.start_min),
                                    e.actor,
                                    EventKind::StartActivity,
                                    0,
                                ),
                            );
                        }
                    }
                    kolejka.schedule(SimEvent::new(
                        e.time.saturating_add(1440),
                        e.actor,
                        EventKind::PlanDay,
                        0,
                    ));
                }
                Some(EventKind::StartActivity) => {
                    let Some(slot) = slot_planu(world, kto, e.slot) else {
                        continue;
                    };
                    // Zdarzenie musi pasować do slotu, na który wskazuje. Przeplanowanie
                    // przestawia indeksy slotów **za** minutą, od której liczy, a łańcuch
                    // zdarzeń niesie indeksy sprzed niego — więc po przeplanowaniu
                    // w kolejce zostają wpisy wskazujące na cudzą czynność. Minuta
                    // startu jest tu kluczem obcym: zgadza się wtedy i tylko wtedy, gdy
                    // zdarzenie nadal opisuje ten sam slot (korekta H-23).
                    if slot.start_min != (e.time % 1440) as u16 {
                        continue;
                    }
                    if let Some(a) = world.get_mut::<AgentState>(kto) {
                        a.activity = slot.kind;
                    }
                    if let Some(p) = world.get_mut::<PlanRef>(kto) {
                        p.cursor = e.slot;
                    }

                    if slot.kind == ActivityKind::Commute as u8
                        && wyrusz(world, kto, e.slot, slot, doba, &mut zrodla, &mut kolejka)
                            .is_some()
                    {
                        stats.trips += 1;
                    }

                    if slot.end_min() < 1440 {
                        wstaw(
                            &mut kolejka,
                            &mut do_obsluzenia,
                            teraz,
                            SimEvent::new(
                                (doba * 1440) as u32 + u32::from(slot.end_min()),
                                e.actor,
                                EventKind::EndActivity,
                                e.slot,
                            ),
                        );
                    }
                    let nastepny = e.slot as usize + 1;
                    if let Some(s) = slot_planu(world, kto, nastepny as u8) {
                        wstaw(
                            &mut kolejka,
                            &mut do_obsluzenia,
                            teraz,
                            SimEvent::new(
                                (doba * 1440) as u32 + u32::from(s.start_min),
                                e.actor,
                                EventKind::StartActivity,
                                nastepny as u8,
                            ),
                        );
                    }
                }
                Some(EventKind::EndActivity) => {
                    // Koniec czynnosci zeruje stan **tylko wtedy**, gdy mieszkaniec nadal
                    // jest w tym slocie. Sloty planu przylegaja do siebie, wiec koniec
                    // jednego i poczatek nastepnego wypadaja w tej samej minucie, a przy
                    // remisie czasowym `order_key` obsluguje najpierw `StartActivity`
                    // (2), potem `EndActivity` (3) - bez tego warunku koniec poprzedniej
                    // czynnosci kasowalby wlasnie rozpoczeta nastepna.
                    let Some(slot) = slot_planu(world, kto, e.slot) else {
                        continue;
                    };
                    // Ten sam klucz obcy co przy `StartActivity` (H-23).
                    if slot.end_min() != (e.time % 1440) as u16 {
                        continue;
                    }
                    let biezacy = world.get::<PlanRef>(kto).map(|p| p.cursor);
                    if biezacy == Some(e.slot) {
                        if let Some(a) = world.get_mut::<AgentState>(kto) {
                            a.activity = ActivityKind::Idle as u8;
                        }
                    }
                    let Some(need) = potrzeba_slotu(slot) else {
                        continue;
                    };
                    let miejsce = crate::places::place_from_key(slot.target).unwrap_or_default();
                    match zaspokoj(world, kto, need, slot, e.time, &self.table, &mut zrodla) {
                        Some(FulfilOutcome::Done { .. }) => stats.fulfilled += 1,
                        Some(FulfilOutcome::Refused(_)) => {
                            stats.refused += 1;
                            // Odmowa jest **sygnalem przeplanowania**, a nie koncem
                            // czynnosci: sciezka jest zbudowana tutaj, zeby M5
                            // wlaczylo istniejace sterowanie, a nie dokladalo nowe.
                            if przeplanuj(
                                world,
                                kto,
                                doba,
                                MinuteOfDay::new((teraz % 1440) as u16),
                                ReplanCause::PlaceRefused { place: miejsce },
                                seed,
                                &self.table,
                                &zrodla,
                                &mut self.canvas,
                            ) {
                                stats.replans += 1;
                            }
                        }
                        None => {}
                    }
                }
                Some(EventKind::Arrive) => {
                    stats.arrivals += 1;
                    if let Some(a) = world.get_mut::<AgentState>(kto) {
                        a.activity = ActivityKind::Idle as u8;
                    }
                    let Some(slot) = slot_planu(world, kto, e.slot) else {
                        continue;
                    };
                    let planowane = (doba * 1440) as u32 + u32::from(slot.end_min());
                    if e.time <= planowane {
                        continue;
                    }
                    // Pieszy dotarl pozniej, niz zakladal plan. To **nie jest** rozjazd
                    // planera z mezo: plan powstal o polnocy, a predkosc marszu zalezy
                    // od energii i zdrowia, ktore przez dobe spadaja (`DeprivationEffects`).
                    // Planer nie mial jak tego wiedziec - i dokladnie po to istnieje
                    // `ReplanCause::Late` (§5.2). Rozjazd przy **niezmienionym** stanie
                    // mieszkanca lapie osobny niezmiennik w tescie planera.
                    let delay_min = (e.time - planowane).min(u32::from(u16::MAX)) as u16;
                    stats.late += 1;
                    stats.late_minutes += u64::from(delay_min);
                    if przeplanuj(
                        world,
                        kto,
                        doba,
                        MinuteOfDay::new((e.time % 1440) as u16),
                        ReplanCause::Late { delay_min },
                        seed,
                        &self.table,
                        &zrodla,
                        &mut self.canvas,
                    ) {
                        stats.replans += 1;
                        zakotwicz(world, kto, doba, e.time, &mut kolejka, &mut do_obsluzenia);
                    }
                }
                _ => {}
            }
        }

        *world.resource_mut::<EventQueue>() = kolejka;
        *world.resource_mut::<DayStats>() = stats;
        world.resource_mut::<AgentSources>().put(zrodla);
    }
}

/// Wstawia zdarzenie do kolejki albo - gdy wypada na minute juz rozdawana - na koniec
/// listy zdarzen tej minuty.
fn wstaw(
    q: &mut EventQueue,
    teraz_lista: &mut std::collections::VecDeque<SimEvent>,
    teraz: u32,
    e: SimEvent,
) {
    if e.time <= teraz {
        teraz_lista.push_back(e);
    } else {
        q.schedule(e);
    }
}

/// Slot planu o danym indeksie — kopia, bo arena jest zaraz potem zapisywana.
fn slot_planu(world: &World, citizen: Entity, slot: u8) -> Option<PlanSlot> {
    let plan = world.get::<PlanRef>(citizen)?;
    load_plan(plan, world.resource::<PlanSlab>())
        .get(slot as usize)
        .copied()
}

/// Potrzeba, którą zaspokaja slot. W M3 zadanie jest tożsame z potrzebą (korekta F-2),
/// więc powód wpisany w slot niesie jej indeks.
fn potrzeba_slotu(slot: PlanSlot) -> Option<NeedKind> {
    /// Powody, ktorych `param` niesie **indeks potrzeby**: potrzeba krytyczna (faza 2)
    /// oraz zadania (faza 3). Bez tego nie da sie powiedziec, co zaspokaja slot
    /// `Idle` wstawiony na poranna toalete — rodzaj czynnosci tego nie mowi.
    const PARAM_TO_POTRZEBA: [u16; 4] = [101, 102, 106, 107];
    if PARAM_TO_POTRZEBA.contains(&u16::from(slot.reason_tag())) {
        return NeedKind::from_index(slot.reason_param() as usize);
    }
    match ActivityKind::from_index(slot.kind as usize)? {
        ActivityKind::Sleep => Some(NeedKind::Sleep),
        ActivityKind::Eat => Some(NeedKind::Hunger),
        ActivityKind::Leisure => Some(NeedKind::Leisure),
        ActivityKind::Social => Some(NeedKind::Social),
        ActivityKind::School => Some(NeedKind::Development),
        _ => None,
    }
}

/// Plan doby dla mieszkańca; `false`, gdy encja nie jest już mieszkańcem.
fn zaplanuj(
    world: &mut World,
    citizen: Entity,
    day: u64,
    seed: u64,
    table: &NeedTable,
    zrodla: &Sources,
    canvas: &mut DayCanvas,
) -> bool {
    let Some(snap) = CitizenSnapshot::of(world, citizen, day) else {
        return false;
    };
    let ctx = snap.ctx(
        seed,
        day,
        table,
        zrodla.places.as_ref(),
        zrodla.travel.as_ref(),
    );
    plan_day(&ctx, canvas);
    // Uchwyt do areny planow **nadpisuje sie**, a nie alokuje od nowa: `PlanRef::default()`
    // ma `offset = NO_PLAN`, wiec `store_plan` wzialby swiezy blok i porzucil poprzedni.
    // Przy 274 tys. mieszkancow to 40 MB wyciekajacych na dobe gry - i plan wczorajszy,
    // ktory nikomu juz nie sluzy, a nadal zajmuje klase rozmiaru w slabie (M3a §5.1).
    let mut plan = world.get::<PlanRef>(citizen).copied().unwrap_or_default();
    let slab = world.resource_mut::<PlanSlab>();
    store_plan(canvas, &mut plan, slab, day);
    if let Some(p) = world.get_mut::<PlanRef>(citizen) {
        *p = plan;
    }
    true
}

/// Przeplanowanie od podanej minuty, z debouncingiem 15 minut (§5.2).
#[allow(clippy::too_many_arguments)]
fn przeplanuj(
    world: &mut World,
    citizen: Entity,
    day: u64,
    from: MinuteOfDay,
    cause: ReplanCause,
    seed: u64,
    table: &NeedTable,
    zrodla: &Sources,
    canvas: &mut DayCanvas,
) -> bool {
    let Some(state) = world.get_mut::<AgentState>(citizen) else {
        return false;
    };
    if !crate::planner::request_replan(state) {
        return false;
    }
    let Some(snap) = CitizenSnapshot::of(world, citizen, day) else {
        return false;
    };
    let plan = world.get::<PlanRef>(citizen).copied().unwrap_or_default();
    canvas.load(load_plan(&plan, world.resource::<PlanSlab>()));
    let ctx = snap.ctx(
        seed,
        day,
        table,
        zrodla.places.as_ref(),
        zrodla.travel.as_ref(),
    );
    replan(&ctx, from, cause, canvas);
    let slab = world.resource_mut::<PlanSlab>();
    let mut nowy = plan;
    store_plan(canvas, &mut nowy, slab, day);
    // `store_plan` ustawia kursor na zero, bo pisze plan na całą dobę. Tutaj doba jest
    // w połowie: kursor wraca na slot, w którym mieszkaniec właśnie jest.
    nowy.cursor = canvas
        .slots()
        .iter()
        .rposition(|s| s.start_min <= from.get())
        .unwrap_or(0) as u8;
    if let Some(p) = world.get_mut::<PlanRef>(citizen) {
        *p = nowy;
    }
    true
}

/// Zakotwicza łańcuch zdarzeń po przeplanowaniu: pierwszy slot zaczynający się **po**
/// minucie `teraz` dostaje własne `StartActivity`.
///
/// Stare wpisy w kolejce zostają, ale nic nie robią — nie pasują już do slotu,
/// na który wskazują (H-23). Bez tego zakotwiczenia mieszkaniec dostawał nowy plan
/// i **nie wykonywał go**: łańcuch urywał się na pierwszym slocie, który przeplanowanie
/// przesunęło.
fn zakotwicz(
    world: &World,
    citizen: Entity,
    day: u64,
    teraz: u32,
    q: &mut EventQueue,
    natychmiast: &mut std::collections::VecDeque<SimEvent>,
) {
    let Some(plan) = world.get::<PlanRef>(citizen).copied() else {
        return;
    };
    let minuta = (teraz % 1440) as u16;
    let sloty = load_plan(&plan, world.resource::<PlanSlab>());
    let Some((i, s)) = sloty.iter().enumerate().find(|(_, s)| s.start_min > minuta) else {
        return;
    };
    wstaw(
        q,
        natychmiast,
        teraz,
        SimEvent::new(
            (day * 1440) as u32 + u32::from(s.start_min),
            citizen.index(),
            EventKind::StartActivity,
            i as u8,
        ),
    );
}

/// Zgłasza podróż do `TravelOracle` — to on, a nie planer, mówi, kiedy pieszy dotrze.
fn wyrusz(
    world: &World,
    citizen: Entity,
    slot_idx: u8,
    slot: PlanSlot,
    day: u64,
    zrodla: &mut Sources,
    q: &mut EventQueue,
) -> Option<crate::places::TripHandle> {
    let dom = crate::places::home_of(world.get::<Residence>(citizen)?);
    let skad = if slot_idx == 0 {
        dom?
    } else {
        cel_slotu(world, citizen, slot_idx - 1).or(dom)?
    };
    let dokad = cel_slotu(world, citizen, slot_idx).or(dom)?;

    // `begin_trip` czyta z podróżnika wyłącznie wiek, zdrowie i energię (prędkość
    // marszu, korekta A-2) — reszta widoku jest wypełnieniem kontraktu.
    let identity = *world.get::<Identity>(citizen)?;
    let vitals = *world.get::<Vitals>(citizen)?;
    let needs = *world.get::<Needs>(citizen)?;
    let personality = *world.get::<Personality>(citizen)?;
    let residence = *world.get::<Residence>(citizen)?;
    let who = crate::places::CitizenView {
        id: CitizenId(citizen),
        identity: &identity,
        vitals: &vitals,
        needs: &needs,
        personality: &personality,
        residence: &residence,
        today: day as i32,
    };
    let handle = zrodla.travel.begin_trip(
        TripRequest {
            traveller: CitizenId(citizen),
            from: skad,
            to: dokad,
            depart: MinuteOfDay::new(slot.start_min),
            slot: slot_idx,
        },
        &who,
        q,
    );
    // Warstwa Mikro (§5.10): pieszy wchodzi w kadr, jeżeli kadr istnieje i jeżeli
    // jego trasa go dotyka. `WalkOracle` sam to rozstrzyga (`set_micro_window`),
    // więc system doby woła to bezwarunkowo i nic nie wie o kamerze. Mezo — czyli
    // minuta przybycia i cała reszta wyniku — nie zmienia się od tego ani trochę.
    zrodla
        .travel
        .enter_micro(&handle, citizen.index(), MinuteOfDay::new(slot.start_min));
    Some(handle)
}

/// Cel slotu jako `PlaceRef`. W arenie siedzi sam indeks encji, więc rodzaj miejsca
/// odtwarza się z konwencji kluczy Etapu 8: budynek albo zakład.
fn cel_slotu(world: &World, citizen: Entity, slot: u8) -> Option<PlaceRef> {
    let s = slot_planu(world, citizen, slot)?;
    crate::places::place_from_key(s.target)
}

/// Wizyta w miejscu — `NeedSatisfySystem` z §5.12.
///
/// **Wołana z `EndActivity`, nie z `StartActivity`** (korekta H-4 do §5.12). Powód jest
/// w danych: `satisfaction` to **przyrost z jednej wizyty**, a wizyta daje go, gdy się
/// skończy — nie gdy się zacznie. Przy okazji rozstrzyga to drugą rzecz, której §5.12
/// nie mówiło: **czas spędzony na zaspokajaniu potrzeby nie liczy się do jej spadku**
/// (`data/needs/needs.ron` mówi o tempie „na jawie", a sen trwa osiem godzin). Spadek
/// z czasu wizyty jest oddawany razem z przyrostem, więc `NeedDecaySystem` nie musi
/// wiedzieć, co mieszkaniec akurat robi.
fn zaspokoj(
    world: &mut World,
    citizen: Entity,
    need: NeedKind,
    slot: PlanSlot,
    teraz: u32,
    table: &NeedTable,
    zrodla: &mut Sources,
) -> Option<FulfilOutcome> {
    let identity = *world.get::<Identity>(citizen)?;
    let miejsce = crate::places::place_from_key(slot.target)
        .or_else(|| crate::places::home_of(world.get::<Residence>(citizen)?))?;
    let gospodarstwo = demography::household_by_index(world, identity.household);
    // Budżet i liczebność czyta się z komponentu `Household` — on jest właścicielem
    // salda gospodarstwa (M5d, korekta po M3c) i M5 nie trzyma drugiej kopii.
    // `budget_hint` to całość dostępnych środków; kopertę per potrzeba wstawi M5d/WP8.
    let (budzet, osob) = gospodarstwo.and_then(|e| world.get::<Household>(e)).map_or(
        (magnat_core::Money::ZERO, 1u8),
        |h| {
            (
                magnat_core::Money(h.cash.get().saturating_add(h.bank.get()).max(0)),
                h.size.max(1),
            )
        },
    );
    let wynik = zrodla.places.fulfil(&FulfilRequest {
        citizen: CitizenId(citizen),
        household: HouseholdId(gospodarstwo.unwrap_or(citizen)),
        need,
        place: miejsce,
        at: magnat_core::SimMinute(u64::from(teraz)),
        budget_hint: budzet,
        household_size: osob,
    });
    if let FulfilOutcome::Done { satisfaction, .. } = wynik {
        let oddane = crate::needs::decay_between(
            table.spec(need).decay_centi_per_hour,
            0,
            u64::from(slot.dur_min),
        )
        .min(255) as u8;
        if let Some(n) = world.get_mut::<Needs>(citizen) {
            let i = need.as_index();
            n.level[i] = n.level[i]
                .saturating_add(satisfaction.get())
                .saturating_add(oddane)
                .min(100);
        }
    }
    Some(wynik)
}

// ── pozostałe systemy §5.12 ─────────────────────────────────────────────────────

/// Odliczanie debouncingu przeplanowań — jedyny system dotykający `AgentState`
/// w każdej minucie, więc idzie po chunkach równolegle (§5.2).
pub struct ReplanCooldownSystem {
    desc: SystemDesc,
}

impl ReplanCooldownSystem {
    #[must_use]
    pub fn new(world: &World) -> ReplanCooldownSystem {
        ReplanCooldownSystem {
            desc: SystemDesc::new("agents.ReplanCooldown", Cadence::EveryMinute)
                .with_query::<&mut AgentState, ()>(world),
        }
    }
}

impl System for ReplanCooldownSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let pool = ctx.pool;
        ctx.query::<&mut AgentState, ()>()
            .par_for_each(pool, |state| {
                crate::planner::tick_replan_cooldown(state);
            });
    }
}

/// Ile shardów ma doba tygodniowa: system dotyka 1/7 populacji na dobę.
pub const WEEK_SHARDS: u32 = 7;

/// Wzrost umiejętności przez pracę i zanik przez nieużywanie (§5.12).
pub struct SkillDriftSystem {
    desc: SystemDesc,
}

impl SkillDriftSystem {
    #[must_use]
    pub fn new(world: &World) -> SkillDriftSystem {
        SkillDriftSystem {
            desc: SystemDesc::new("agents.SkillDrift", Cadence::EveryDay).with_query::<(
                Entity,
                &Employment,
                &mut Skills,
            ), ()>(world),
        }
    }
}

impl System for SkillDriftSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let doba = ctx.tick.0 / 1440;
        let shard = (doba % u64::from(WEEK_SHARDS)) as u32;
        let pool = ctx.pool;
        ctx.query::<(Entity, &Employment, &mut Skills), ()>()
            .par_for_each(pool, |(e, emp, skills)| {
                if e.index() % WEEK_SHARDS != shard {
                    return;
                }
                for s in &mut skills.0 {
                    if s.role == Skills::ROLE_NONE {
                        continue;
                    }
                    // Tydzień pracy w roli podnosi ją o punkt; tydzień bez niej
                    // odbiera tyle, ile mówi `decay` (setne punktu na dobę × 7).
                    if emp.has_job() && emp.role == s.role {
                        s.level = s.level.saturating_add(1).min(100);
                    } else {
                        let ubytek = (u32::from(s.decay) * 7 / 100).max(1) as u8;
                        s.level = s.level.saturating_sub(ubytek);
                    }
                }
            });
    }
}

/// Zużycie zapasów gospodarstwa — jeden dzień zapasu na dobę (§5.12).
/// M5 zastąpi to realną konsumpcją towarów z partiami.
pub struct HouseholdStockSystem {
    desc: SystemDesc,
}

impl HouseholdStockSystem {
    #[must_use]
    pub fn new(world: &World) -> HouseholdStockSystem {
        HouseholdStockSystem {
            desc: SystemDesc::new("agents.HouseholdStock", Cadence::EveryDay)
                .with_query::<&mut Household, ()>(world),
        }
    }
}

impl System for HouseholdStockSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let pool = ctx.pool;
        ctx.query::<&mut Household, ()>().par_for_each(pool, |h| {
            if h.flags & Household::FLAG_ACTIVE == 0 {
                return;
            }
            for s in &mut h.stock {
                *s = s.saturating_sub(1);
            }
        });
    }
}

/// Rytm doby i miesiąca społeczeństwa: demografia, migracja, status, relacje, plotka.
///
/// Opakowuje `society::step_day` **bez zmiany kolejności** (korekta E-16): zgon przed
/// migracją, status przed doborem partnera, odbudowa indeksu kontaktów po migracji.
/// Wyłączny, bo `demography::step_day` dotyka wszystkiego o mieszkańcu.
pub struct SocietySystem {
    desc: SystemDesc,
    hooks: Box<dyn demography::InheritanceHook>,
    last: Option<society::SocietyReport>,
}

impl SocietySystem {
    #[must_use]
    pub fn new(hooks: Box<dyn demography::InheritanceHook>) -> SocietySystem {
        SocietySystem {
            desc: SystemDesc::new("agents.Society", Cadence::EveryDay).exclusive(),
            hooks,
            last: None,
        }
    }

    /// Raport ostatniej doby — wejście karty inspekcji i scenariuszy.
    #[must_use]
    pub fn last_report(&self) -> Option<&society::SocietyReport> {
        self.last.as_ref()
    }
}

impl System for SocietySystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let doba = ctx.tick.0 / 1440;
        let world = ctx.world_mut();
        self.last = Some(society::step_day(world, doba, self.hooks.as_mut()));
    }
}

/// Warstwa Mikro: **jedno** przestawienie pozycji na minutę świata (M4c §5.12).
///
/// **Wyłącznie wizualna** — nie zapisuje niczego do stanu ekonomicznego, więc obrót
/// kamerą nie zmienia wyniku symulacji (00 §4, tolerancja mikro↔mezo = 0).
///
/// **Dlaczego jedno wywołanie, a nie 600.** Tick ruchu mikro to 100 ms gry (00 §4),
/// więc pierwotnie system krokował bufor `MICRO_STEPS_PER_TICK` razy w każdej minucie.
/// Krok jest jednak **czystą funkcją czasu**: pozycja wynika z `(now_ms, depart, arrive)`
/// i nic się między podkrokami nie akumuluje, więc 599 z 600 przebiegów było
/// nadpisywanych. Nie kupowały nawet płynności, bo renderer czyta zrzut raz na klatkę
/// i widzi wyłącznie stan po ostatnim podkroku. Płynność wymaga kroku sterowanego
/// czasem **klatki**, a nie tickiem symulacji — to M11b (animacja), nie tutaj.
pub struct TravelMicroSystem {
    desc: SystemDesc,
}

impl TravelMicroSystem {
    #[must_use]
    pub fn new(world: &World) -> TravelMicroSystem {
        TravelMicroSystem {
            desc: SystemDesc::new("agents.TravelMicro", Cadence::EveryMicroTick)
                .reads_resource::<AgentSources>(world),
        }
    }
}

impl System for TravelMicroSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let minuta = ctx.tick.0;
        let Some(z) = ctx.res::<AgentSources>().get() else {
            return;
        };
        // Koniec minuty, nie jej początek: `micro_retire` usuwa tych, którzy w tej
        // minucie dotarli, więc pozycja musi być już policzona na moment przybycia.
        let koniec_ms = (minuta + 1) * 60_000 - magnat_core::time::MICRO_TICK_MS;
        z.travel.micro_step(koniec_ms);
        z.travel.micro_retire((minuta % 1440) as u16);
    }
}

// ── start doby ──────────────────────────────────────────────────────────────────

/// Zasiewa kolejkę: `PlanDay` dla każdego mieszkańca w pierwszej minucie doby `day`.
///
/// Dalej kolejka napędza się sama (lazy scheduling §5.2): plan harmonogramuje pierwszą
/// czynność, każda czynność następną, a obsługa `PlanDay` dopisuje `PlanDay` na dobę
/// następną. Runner nie planuje niczego — od tego jest [`DayLoopSystem`].
pub fn bootstrap_day(world: &mut World, day: u64) -> u32 {
    let mieszkancy: Vec<Entity> = world
        .resource::<demography::Population>()
        .citizens()
        .to_vec();
    let mut kolejka = std::mem::take(world.resource_mut::<EventQueue>());
    for c in &mieszkancy {
        kolejka.schedule(SimEvent::new(
            (day * 1440) as u32,
            c.index(),
            EventKind::PlanDay,
            0,
        ));
    }
    *world.resource_mut::<EventQueue>() = kolejka;
    mieszkancy.len() as u32
}

/// Ilu mieszkańców jest w LOD Mikro — wejście renderera i licznika kadru.
#[must_use]
pub fn micro_count(world: &World) -> usize {
    world
        .resource::<AgentSources>()
        .get()
        .map_or(0, |z| z.travel.micro_len())
}

/// Przełącza mieszkańca w LOD Mikro (kadr kamery). Sam LOD nie wpływa na wynik.
pub fn set_lod(world: &mut World, citizen: Entity, micro: bool) {
    if let Some(a) = world.get_mut::<AgentState>(citizen) {
        a.lod = if micro { Lod::Micro } else { Lod::Meso } as u8;
    }
}

// ── bufor śledzenia (§14.4, decyzja 9.16) ───────────────────────────────────────

/// Ilu mieszkańców da się śledzić naraz.
pub const MAX_WATCHED: usize = 8;
/// Ile ostatnich zdarzeń trzyma bufor jednego śledzonego.
pub const TRACE_LEN: usize = 64;

/// Jedno zdarzenie w buforze śledzenia.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TraceEntry {
    /// Minuta świata.
    pub minute: u32,
    /// `EventKind`.
    pub kind: u8,
    pub slot: u8,
    /// `ActivityKind` w chwili zdarzenia.
    pub activity: u8,
}

impl TraceEntry {
    /// Czy wpis otwiera blok realizacji.
    #[must_use]
    pub fn is_start(&self) -> bool {
        self.kind == EventKind::StartActivity as u8
    }
}

/// Bufor śledzenia: ostatnie decyzje najwyżej ośmiu obserwowanych mieszkańców.
///
/// **Nie jest logiem całej populacji i nigdy nim nie będzie** (decyzja 9.16): pełne
/// logi decyzji dla 400 tys. mieszkańców to 14 GB na rok gry. Reszta odtwarza się
/// z ziarna — `plan_day_explained` kosztuje 0,84 µs, więc karta inspekcji liczy
/// uzasadnienia przy każdym otwarciu, zamiast je przechowywać.
///
/// Zasób **nie wchodzi do hasha stanu**: to, kogo gracz akurat ogląda, nie jest stanem
/// symulacji (00 §4 — kamera nie ma prawa zmieniać wyniku).
#[derive(Clone, Debug, Default)]
pub struct Trace {
    /// `(indeks encji, kolejka wpisów)`, posortowane po indeksie — iteracja
    /// deterministyczna (00 §3.2).
    watched: Vec<(u32, std::collections::VecDeque<TraceEntry>)>,
}

impl Trace {
    #[must_use]
    pub fn new() -> Trace {
        Trace::default()
    }

    /// Zaczyna śledzić mieszkańca. Zwraca `false`, gdy wszystkie osiem miejsc zajęte —
    /// cisza byłaby gorsza, bo karta pokazywałaby pustą realizację bez wyjaśnienia.
    pub fn watch(&mut self, citizen: u32) -> bool {
        if self.watched.iter().any(|(c, _)| *c == citizen) {
            return true;
        }
        if self.watched.len() >= MAX_WATCHED {
            return false;
        }
        let poz = self
            .watched
            .binary_search_by_key(&citizen, |(c, _)| *c)
            .unwrap_or_else(|i| i);
        self.watched
            .insert(poz, (citizen, std::collections::VecDeque::new()));
        true
    }

    pub fn unwatch(&mut self, citizen: u32) {
        self.watched.retain(|(c, _)| *c != citizen);
    }

    pub fn clear(&mut self) {
        self.watched.clear();
    }

    #[must_use]
    pub fn is_watched(&self, citizen: u32) -> bool {
        self.watched.iter().any(|(c, _)| *c == citizen)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.watched.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.watched.is_empty()
    }

    /// Wpisy śledzonego mieszkańca w kolejności zdarzeń.
    #[must_use]
    pub fn entries(&self, citizen: u32) -> Vec<TraceEntry> {
        self.watched
            .binary_search_by_key(&citizen, |(c, _)| *c)
            .ok()
            .map(|i| self.watched[i].1.iter().copied().collect())
            .unwrap_or_default()
    }

    fn record(&mut self, citizen: u32, e: TraceEntry) {
        let Ok(i) = self.watched.binary_search_by_key(&citizen, |(c, _)| *c) else {
            return;
        };
        let bufor = &mut self.watched[i].1;
        if bufor.len() >= TRACE_LEN {
            bufor.pop_front();
        }
        bufor.push_back(e);
    }
}
