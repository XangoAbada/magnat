//! Planer dnia — testy §7.3 fazy M3 (podfaza M3b, WP5).
//!
//! Scenariusz wzorcowy jest jeden i wspólny dla wszystkich testów: **Anna**, kobieta
//! 34 lata, księgowa, zmiana 8–16, rodzina z dziećmi (odprowadza jedno do szkoły),
//! zapas pieczywa na jeden dzień. To ten sam przypadek co w PRD §5.5 i to on jest
//! złotym testem akceptacyjnym fazy.
//!
//! Miasto testowe jest siatką ulic 3 × 3 kwartały po 300 m, żeby odległość sieciowa
//! **różniła się** od linii prostej — inaczej WP6 testowałby sam siebie.

use magnat_agents::{
    load_plan, plan_day, plan_day_explained, render_day_debug, replan, replan_explained_into,
    request_replan, store_plan, tick_replan_cooldown, AgentState, DayCanvas, Employment,
    EmptyPlaces, FlakyPlaces, HouseholdView, Identity, InfinitePlaces, Knowledge, KnowledgeKind,
    KnowledgeView, NeedTable, Needs, Personality, PlaceEntry, PlaceTable, PlanCtx, PlanRef,
    PlanSlab, ReasonLog, ReplanCause, Residence, ShiftKind, StraightLineTravel, Vitals, MAX_SLOTS,
};
use magnat_core::{
    ActivityKind, BuildingId, CitizenId, DayOfWeek, DecisionReason, Entity, NeedKind, PlaceKind,
    PlaceRef, SiteId, StockCat, WorldCoord, STOCK_CAT_COUNT,
};
use std::num::NonZeroU32;
use std::sync::Arc;

// ── scena ───────────────────────────────────────────────────────────────────────

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).unwrap())
}

fn budynek(i: u32) -> PlaceRef {
    PlaceRef::Building(BuildingId(encja(i)))
}

fn zaklad(i: u32) -> PlaceRef {
    PlaceRef::Site(SiteId(encja(i)))
}

const DOM: u32 = 1;
const PRACA: u32 = 2;
const SKLEP_PO_DRODZE: u32 = 3;
const SKLEP_PRZY_DOMU: u32 = 4;
const SZKOLA: u32 = 5;
const PRZYCHODNIA: u32 = 6;

/// Siatka 4 × 4 węzłów co 300 m — dziewięć kwartałów. Odległość po ulicach między
/// przeciwległymi rogami to 1800 m, w linii prostej 1273 m.
fn siatka() -> (Vec<WorldCoord>, Vec<(u32, u32, u32)>) {
    let mut nodes = Vec::new();
    for y in 0..4i32 {
        for x in 0..4i32 {
            nodes.push(WorldCoord::new(x * 30_000, y * 30_000, 0));
        }
    }
    let idx = |x: i32, y: i32| (y * 4 + x) as u32;
    let mut segs = Vec::new();
    for y in 0..4i32 {
        for x in 0..4i32 {
            if x + 1 < 4 {
                segs.push((idx(x, y), idx(x + 1, y), 30_000));
            }
            if y + 1 < 4 {
                segs.push((idx(x, y), idx(x, y + 1), 30_000));
            }
        }
    }
    (nodes, segs)
}

fn katalog() -> PlaceTable {
    PlaceTable::build(vec![
        PlaceEntry {
            place: budynek(DOM),
            kind: PlaceKind::Home,
            at: WorldCoord::new(0, 0, 0),
        },
        PlaceEntry {
            place: zaklad(PRACA),
            kind: PlaceKind::Workplace,
            at: WorldCoord::new(90_000, 0, 0), // 900 m na wschód
        },
        PlaceEntry {
            place: budynek(SKLEP_PO_DRODZE),
            kind: PlaceKind::Grocery,
            at: WorldCoord::new(60_000, 0, 0), // przy trasie do pracy
        },
        PlaceEntry {
            place: budynek(SKLEP_PRZY_DOMU),
            kind: PlaceKind::Grocery,
            at: WorldCoord::new(0, 30_000, 0), // 300 m na północ od domu
        },
        PlaceEntry {
            place: budynek(SZKOLA),
            kind: PlaceKind::Education,
            at: WorldCoord::new(30_000, 0, 0),
        },
        PlaceEntry {
            place: budynek(PRZYCHODNIA),
            kind: PlaceKind::Doctor,
            at: WorldCoord::new(30_000, 30_000, 0),
        },
    ])
}

/// Wszystko, co żyje dłużej niż `PlanCtx`. Osobno, bo kontekst trzyma referencje.
struct Scena {
    identity: Identity,
    vitals: Vitals,
    needs_stan: Needs,
    personality: Personality,
    residence: Residence,
    employment: Employment,
    wiedza: Vec<Knowledge>,
    stock: [u8; STOCK_CAT_COUNT],
    escorts: Vec<PlaceRef>,
    tabela: Arc<NeedTable>,
    miejsca: InfinitePlaces,
    oracle: StraightLineTravel,
    day: u64,
}

impl Scena {
    /// Anna Wiśniewska, 34 lata, księgowa, zmiana 8–16, dziecko w szkole,
    /// pieczywo na jeden dzień.
    fn anna() -> Scena {
        let places = Arc::new(katalog());
        let tabela = Arc::new(NeedTable::load_default().expect("data/needs/needs.ron"));
        let (_nodes, _segs) = siatka();
        let mut stock = HouseholdView::FULL;
        stock[StockCat::Food.as_index()] = 1;

        Scena {
            identity: Identity {
                birth_day: -360 * 34,
                flags: Identity::FLAG_ALIVE,
                household: 100,
                ..Identity::default()
            },
            vitals: Vitals {
                health: 80,
                energy: 70,
                edu_level: 3,
                ..Vitals::default()
            },
            needs_stan: Needs {
                // Higiena poniżej progu 40 → faza 2 wstawi poranną toaletę.
                level: {
                    let mut l = [70u8; 12];
                    l[NeedKind::Sleep.as_index()] = 35;
                    l[NeedKind::Hunger.as_index()] = 45;
                    l[NeedKind::Hygiene.as_index()] = 30;
                    l
                },
                updated_at: 0,
            },
            personality: Personality([55, 50, 60, 50, 45, 65, 40, 70]),
            residence: Residence {
                building: DOM,
                unit: 4,
                district: 1,
            },
            employment: Employment {
                site: PRACA,
                role: 7,
                shift: ShiftKind::Day as u8,
                flags: 0,
                commute_baseline_min: 15,
                work_days: Employment::WEEKDAYS,
                _pad: 0,
            },
            wiedza: [SKLEP_PO_DRODZE, SKLEP_PRZY_DOMU, SZKOLA, PRZYCHODNIA]
                .iter()
                .map(|i| Knowledge {
                    target: *i,
                    day: 0,
                    score: 70,
                    kind: KnowledgeKind::Visited as u8,
                })
                .collect(),
            stock,
            escorts: vec![budynek(SZKOLA)],
            miejsca: InfinitePlaces::new(places.clone(), tabela.clone()),
            oracle: StraightLineTravel::new(places.clone()),
            tabela,
            // Doba 3 świata = czwartek (doba 0 to poniedziałek, K-15).
            day: 3,
        }
    }

    fn ctx(&self) -> PlanCtx<'_> {
        PlanCtx {
            seed: 4242,
            day: self.day,
            dow: DayOfWeek::from_day_index(self.day),
            citizen: magnat_agents::CitizenView {
                id: CitizenId(encja(77)),
                identity: &self.identity,
                vitals: &self.vitals,
                needs: &self.needs_stan,
                personality: &self.personality,
                residence: &self.residence,
                today: self.day as i32,
                brands: Default::default(),
            },
            household: HouseholdView {
                id: magnat_core::HouseholdId(encja(100)),
                stock: &self.stock,
                escorts: &self.escorts,
                pickups: &self.escorts,
            },
            employment: &self.employment,
            known: KnowledgeView::new(&self.wiedza),
            needs: &self.tabela,
            home: budynek(DOM),
            work: Some(zaklad(PRACA)),
            school: Some(budynek(SZKOLA)),
            places: &self.miejsca,
            travel: &self.oracle,
            max_task_travel_min: 30,
        }
    }
}

fn ma_rodzaj(canvas: &DayCanvas, k: ActivityKind) -> bool {
    canvas.slots().iter().any(|s| s.kind == k as u8)
}

// ── testy ───────────────────────────────────────────────────────────────────────

#[test]
fn odbior_dziecka_wraca_przez_szkole_a_bez_niego_prosto() {
    // Korekta C-8: popołudniowy odbiór dziecka należy do M3c i wchodzi do planu jako
    // trzy sloty (praca → szkoła, przekazanie, szkoła → dom), tak samo jak poranne
    // odprowadzenie. Kto nie odbiera, wraca prosto — i to jest cała różnica.
    let s = Scena::anna();
    let brak: Vec<PlaceRef> = Vec::new();

    let mut z_odbiorem = DayCanvas::new();
    plan_day(&s.ctx(), &mut z_odbiorem);

    let mut ctx = s.ctx();
    ctx.household = HouseholdView {
        id: magnat_core::HouseholdId(encja(100)),
        stock: &s.stock,
        escorts: &s.escorts,
        pickups: &brak,
    };
    let mut bez_odbioru = DayCanvas::new();
    plan_day(&ctx, &mut bez_odbioru);

    let po_pracy = |c: &DayCanvas| -> Vec<(u16, u8)> {
        c.slots()
            .iter()
            .filter(|x| x.start_min >= 16 * 60 && x.start_min < 17 * 60)
            .map(|x| (x.start_min, x.kind))
            .collect()
    };
    let z = po_pracy(&z_odbiorem);
    let b = po_pracy(&bez_odbioru);
    assert!(
        z.len() > b.len(),
        "odbiór nie dołożył slotów: {z:?} vs {b:?}"
    );
    assert!(
        z_odbiorem
            .slots()
            .iter()
            .any(|x| { x.kind == ActivityKind::Errand as u8 && x.start_min > 16 * 60 }),
        "brak przekazania dziecka po południu"
    );
    assert!(
        !bez_odbioru
            .slots()
            .iter()
            .any(|x| { x.kind == ActivityKind::Errand as u8 && x.start_min > 16 * 60 }),
        "mieszkaniec bez przypisanego odbioru poszedł po dziecko"
    );

    // Oba plany zostają spójne: żadnych nakładek i żadnego przejścia przez północ.
    for c in [&z_odbiorem, &bez_odbioru] {
        let mut koniec = 0u16;
        for x in c.slots() {
            assert!(x.start_min >= koniec, "nakładanie slotów");
            koniec = x.end_min();
        }
        assert!(koniec <= 1440);
    }
}

#[test]
fn plan_golden_anna() {
    // Test akceptacyjny podfazy: wydruk dnia wzorcowego zgodny bajt w bajt
    // z zatwierdzonym plikiem. Kolejność z §5.5: pobudka → posiłek → dojazd → praca →
    // zadanie po drodze → dom → czas wolny → sen.
    let s = Scena::anna();
    let ctx = s.ctx();
    let mut canvas = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut canvas, &mut log);
    let wydruk = render_day_debug(&ctx, &canvas, &log);

    let wzorzec = include_str!("golden/anna_day.txt");
    assert_eq!(
        wydruk, wzorzec,
        "wydruk dnia wzorcowego się zmienił.\n--- otrzymano ---\n{wydruk}"
    );

    // Struktura z PRD §5.5 — sprawdzana osobno, żeby złoty plik nie był jedynym
    // strażnikiem sensu: plik pilnuje stabilności, te asercje pilnują treści.
    assert!(ma_rodzaj(&canvas, ActivityKind::Sleep), "brak snu");
    assert!(ma_rodzaj(&canvas, ActivityKind::Eat), "brak posiłku");
    assert!(ma_rodzaj(&canvas, ActivityKind::Commute), "brak dojazdu");
    assert!(ma_rodzaj(&canvas, ActivityKind::Work), "brak pracy");
    assert!(ma_rodzaj(&canvas, ActivityKind::Shop), "brak zakupów");
    assert!(
        canvas.slots().iter().any(|s| matches!(
            ActivityKind::from_index(s.kind as usize),
            Some(ActivityKind::Leisure | ActivityKind::Social | ActivityKind::Idle)
        )),
        "brak czasu wolnego"
    );

    // Zakupy mają uzasadnienie mówiące, dlaczego **to** miejsce, i z czym konkurowało.
    let zakupy = canvas
        .slots()
        .iter()
        .position(|s| s.kind == ActivityKind::Shop as u8)
        .expect("slot zakupów");
    let powody: Vec<DecisionReason> = log.for_slot(zakupy).map(|e| e.reason).collect();
    assert!(
        powody.iter().any(|r| matches!(
            r,
            DecisionReason::ChosenOnRoute { .. } | DecisionReason::ChosenNearest { .. }
        )),
        "wybór sklepu bez uzasadnienia: {powody:?}"
    );
    // Wyzwalacz zakupów to zapas gospodarstwa, nie kaprys — i to musi być w logu.
    assert!(
        log.entries().iter().any(|e| matches!(
            e.reason,
            DecisionReason::StockBelowThreshold {
                cat: StockCat::Food,
                days_left: 1
            }
        )),
        "brak powodu „zapas pieczywa na jeden dzień”"
    );
}

#[test]
fn plan_day_jest_czysta_funkcja() {
    // `det_plan_pure` z §7.1: 1000 wywołań na tych samych danych = identyczne bajty.
    let s = Scena::anna();
    let ctx = s.ctx();
    let mut wzorzec = DayCanvas::new();
    let stats = plan_day(&ctx, &mut wzorzec);
    for _ in 0..1000 {
        let mut c = DayCanvas::new();
        let st = plan_day(&ctx, &mut c);
        assert_eq!(c, wzorzec, "plan zmienił się między wywołaniami");
        assert_eq!(st, stats);
    }
}

#[test]
fn plan_explain_matches() {
    // Tryb `explain` musi dawać **te same** sloty, inaczej karta inspekcji pokazuje
    // inny dzień niż ten, który agent przeżył.
    for dzien in 0..14u64 {
        let mut s = Scena::anna();
        s.day = dzien;
        let ctx = s.ctx();
        let mut a = DayCanvas::new();
        let mut b = DayCanvas::new();
        let mut log = ReasonLog::new();
        let sa = plan_day(&ctx, &mut a);
        let sb = plan_day_explained(&ctx, &mut b, &mut log);
        assert_eq!(a, b, "doba {dzien}: explain rozjechał się z planem");
        assert_eq!(sa, sb);
        assert!(!log.entries().is_empty(), "doba {dzien}: pusty log");
    }
}

#[test]
fn plan_no_overlap() {
    // Żadne dwa sloty się nie nakładają, a suma czasów i luk to dokładnie doba.
    for dzien in 0..30u64 {
        let mut s = Scena::anna();
        s.day = dzien;
        let ctx = s.ctx();
        let mut c = DayCanvas::new();
        let stats = plan_day(&ctx, &mut c);
        let sloty = c.slots();
        for para in sloty.windows(2) {
            assert!(
                para[0].end_min() <= para[1].start_min,
                "doba {dzien}: sloty {:?} i {:?} nachodzą",
                para[0],
                para[1]
            );
        }
        let zajete: u32 = sloty.iter().map(|s| u32::from(s.dur_min)).sum();
        assert!(zajete <= 1440, "doba {dzien}: plan wychodzi poza dobę");
        assert_eq!(
            zajete + u32::from(stats.free_min),
            1440,
            "doba {dzien}: sloty plus luki nie składają się na dobę"
        );
        assert!(sloty.len() <= MAX_SLOTS);
    }
}

#[test]
fn plan_every_slot_has_reason() {
    // Każdy slot niesie powód (00 §7). Tag 0 to `Unspecified` — jego obecność
    // w planie byłaby błędem przeglądu, nie wartością domyślną.
    for i in 0..500u32 {
        let mut s = Scena::anna();
        s.day = u64::from(i % 30);
        s.needs_stan.level = std::array::from_fn(|n| ((i * 7 + n as u32 * 13) % 101) as u8);
        s.personality = Personality(std::array::from_fn(|n| {
            ((i * 3 + n as u32 * 29) % 101) as u8
        }));
        s.stock = std::array::from_fn(|n| ((i + n as u32) % 5) as u8);
        let ctx = s.ctx();
        let mut c = DayCanvas::new();
        plan_day(&ctx, &mut c);
        for slot in c.slots() {
            assert_ne!(
                slot.reason_tag(),
                0,
                "mieszkaniec {i}: slot {slot:?} bez powodu"
            );
        }
    }
}

#[test]
fn plan_commitments_never_dropped() {
    // Praca i szkoła nie znikają nigdy — ani przez zadania, ani przez czas wolny,
    // ani przy przepełnieniu 24 slotów.
    for i in 0..200u32 {
        let mut s = Scena::anna();
        s.day = u64::from(i % 5); // dni robocze
                                  // Wszystkie zapasy na zerze i wszystkie potrzeby na dnie: maksymalny napór
                                  // na limit slotów.
        s.stock = [0; STOCK_CAT_COUNT];
        s.needs_stan.level = [1; 12];
        // Sen na 100, żeby ryzyko absencji ze `Sleep` nie zabrało pracy z planu.
        s.needs_stan.level[NeedKind::Sleep.as_index()] = 100;
        s.needs_stan.level[NeedKind::Health.as_index()] = 100;
        s.needs_stan.level[NeedKind::Mobility.as_index()] = 100;
        s.personality = Personality(std::array::from_fn(|n| ((i + n as u32 * 17) % 101) as u8));
        let ctx = s.ctx();
        let mut c = DayCanvas::new();
        plan_day(&ctx, &mut c);
        assert!(
            ma_rodzaj(&c, ActivityKind::Work),
            "mieszkaniec {i}: praca wypadła z planu"
        );
    }
}

#[test]
fn plan_sleep_and_meal_survive() {
    // Przy przepełnieniu slotów plan nadal zawiera sen i co najmniej jeden posiłek.
    for i in 0..200u32 {
        let mut s = Scena::anna();
        s.day = u64::from(i % 7);
        s.stock = [0; STOCK_CAT_COUNT];
        s.needs_stan.level = std::array::from_fn(|n| ((i * 11 + n as u32 * 5) % 40) as u8);
        let ctx = s.ctx();
        let mut c = DayCanvas::new();
        plan_day(&ctx, &mut c);
        assert!(
            ma_rodzaj(&c, ActivityKind::Sleep),
            "mieszkaniec {i}: bez snu"
        );
        assert!(
            ma_rodzaj(&c, ActivityKind::Eat),
            "mieszkaniec {i}: bez posiłku"
        );
    }
}

#[test]
fn plan_unknown_place() {
    // Mieszkaniec, który nie zna żadnego miejsca, pomija zadanie z powodem —
    // bez paniki i bez teleportacji do najbliższego sklepu w mieście.
    let mut s = Scena::anna();
    s.wiedza.clear();
    let ctx = s.ctx();
    let mut c = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut c, &mut log);

    assert!(
        !ma_rodzaj(&c, ActivityKind::Shop),
        "poszedł na zakupy do sklepu, którego nie zna"
    );
    assert!(
        log.skipped().any(|e| matches!(
            e.reason,
            DecisionReason::PlaceUnknown {
                need: NeedKind::Hunger,
                known_count: 0
            }
        )),
        "brak powodu pominięcia: {:?}",
        log.skipped().collect::<Vec<_>>()
    );

    // Ta sama ścieżka przez atrapę, która nie zna niczego — planer się nie zmienia.
    let ctx_pusty = PlanCtx {
        places: &EmptyPlaces,
        ..s.ctx()
    };
    let mut c2 = DayCanvas::new();
    plan_day(&ctx_pusty, &mut c2);
    assert!(!ma_rodzaj(&c2, ActivityKind::Shop));
}

#[test]
fn plan_dziala_przez_atrape_odmawiajaca() {
    // `plan_refusal_path`: planowanie nie zależy od tego, czy miejsce **zrealizuje**
    // wizytę — odmowa jest zdarzeniem wykonania, a nie planowania. Ścieżka domknięta
    // jest przeplanowaniem z `ReplanCause::PlaceRefused` (M5 wypełni ją treścią).
    let s = Scena::anna();
    let places = Arc::new(katalog());
    let flaky = FlakyPlaces::new(InfinitePlaces::new(places, s.tabela.clone()));
    let ctx = PlanCtx {
        places: &flaky,
        ..s.ctx()
    };
    let mut c = DayCanvas::new();
    plan_day(&ctx, &mut c);
    let zakupy = c
        .slots()
        .iter()
        .find(|x| x.kind == ActivityKind::Shop as u8)
        .copied()
        .expect("zakupy w planie");

    let mut log = ReasonLog::new();
    let stats = replan_explained_into(
        &ctx,
        magnat_core::MinuteOfDay::new(zakupy.start_min),
        ReplanCause::PlaceRefused {
            place: budynek(SKLEP_PO_DRODZE),
        },
        &mut c,
        &mut log,
    );
    assert!(stats.slots > 0);
    for para in c.slots().windows(2) {
        assert!(
            para[0].end_min() <= para[1].start_min,
            "plan po odmowie się rozjechał"
        );
    }
    assert!(
        log.entries()
            .iter()
            .any(|e| matches!(e.reason, DecisionReason::Replanned { .. })),
        "przeplanowanie bez powodu w logu"
    );
}

#[test]
fn replan_zachowuje_zobowiazania_i_przeszlosc() {
    let s = Scena::anna();
    let ctx = s.ctx();
    let mut c = DayCanvas::new();
    plan_day(&ctx, &mut c);
    let praca_przed = c
        .slots()
        .iter()
        .find(|x| x.kind == ActivityKind::Work as u8)
        .copied()
        .expect("praca");

    let stats = replan(
        &ctx,
        magnat_core::MinuteOfDay::new(10 * 60),
        ReplanCause::NeedCritical {
            need: NeedKind::Hunger,
        },
        &mut c,
    );
    let praca_po = c
        .slots()
        .iter()
        .find(|x| x.kind == ActivityKind::Work as u8)
        .copied()
        .expect("praca zniknęła przy przeplanowaniu przyrostowym");
    assert_eq!(praca_przed, praca_po);
    assert!(stats.slots > 0);

    // Pełne przeplanowanie (zmiana grafiku) buduje dobę od zera — i nadal ma pracę.
    let stats2 = replan(
        &ctx,
        magnat_core::MinuteOfDay::new(10 * 60),
        ReplanCause::ShiftChanged,
        &mut c,
    );
    assert!(stats2.slots > 0);
    assert!(ma_rodzaj(&c, ActivityKind::Work));
}

#[test]
fn plan_replan_debounce() {
    // 1000 zgłoszeń w tej samej minucie → dokładnie jedno przeplanowanie (R3).
    let mut state = AgentState::default();
    let zgody = (0..1000).filter(|_| request_replan(&mut state)).count();
    assert_eq!(zgody, 1, "debouncing przepuścił {zgody} przeplanowań");

    // Po odczekaniu okna kolejne zgłoszenie znowu przechodzi.
    for _ in 0..magnat_agents::REPLAN_COOLDOWN_MIN {
        tick_replan_cooldown(&mut state);
    }
    assert!(request_replan(&mut state), "debouncing nigdy nie wygasa");
}

#[test]
fn plan_wraca_ze_slabu_taki_sam() {
    // Plan zapisany do slabu i odczytany jest tym samym planem — to jest wejście
    // do `det_plan_replay` (§7.1) i do karty inspekcji w M3d.
    let s = Scena::anna();
    let ctx = s.ctx();
    let mut c = DayCanvas::new();
    plan_day(&ctx, &mut c);

    let mut slab = PlanSlab::new();
    let mut plan = PlanRef::default();
    store_plan(&c, &mut plan, &mut slab, s.day);
    assert!(plan.is_current(s.day));
    assert_eq!(load_plan(&plan, &slab), c.slots());
    assert_eq!(slab.occupied_blocks(), 1, "zapis planu wycieka blokami");
}

#[test]
fn weekend_wyglada_inaczej_niz_dzien_roboczy() {
    // `prop_weekly_rhythm` z §7.2, część dotycząca planera: sobota i niedziela mają
    // mniej pracy i więcej czasu wolnego. Dzień tygodnia **wyłącznie** z `SimCalendar`.
    let mut praca_robocze = 0;
    let mut praca_weekend = 0;
    for dzien in 0..14u64 {
        let mut s = Scena::anna();
        s.day = dzien;
        let ctx = s.ctx();
        let mut c = DayCanvas::new();
        plan_day(&ctx, &mut c);
        let ile = c
            .slots()
            .iter()
            .filter(|x| x.kind == ActivityKind::Work as u8)
            .count();
        if DayOfWeek::from_day_index(dzien).is_weekend() {
            praca_weekend += ile;
        } else {
            praca_robocze += ile;
        }
    }
    assert_eq!(praca_weekend, 0, "księgowa pracuje w weekend");
    assert!(
        praca_robocze >= 10,
        "dni robocze bez pracy: {praca_robocze}"
    );
}

#[test]
fn nocna_zmiana_nie_przechodzi_przez_polnoc_w_planie() {
    // Zmiana 22–6 zawija się przez północ, a plan nie: doba dostaje ogon wczorajszej
    // zmiany i początek dzisiejszej, każdy jako osobny slot w swojej dobie.
    let mut s = Scena::anna();
    s.employment.shift = ShiftKind::Night as u8;
    s.day = 3; // czwartek, wczoraj też roboczy
    let ctx = s.ctx();
    let mut c = DayCanvas::new();
    plan_day(&ctx, &mut c);

    let praca: Vec<_> = c
        .slots()
        .iter()
        .filter(|x| x.kind == ActivityKind::Work as u8)
        .collect();
    assert_eq!(praca.len(), 2, "nocka dała {} slotów pracy", praca.len());
    assert_eq!(praca[0].start_min, 0);
    assert_eq!(praca[0].end_min(), 6 * 60);
    assert_eq!(praca[1].start_min, 22 * 60);
    assert_eq!(praca[1].end_min(), 1440);
    assert!(
        ma_rodzaj(&c, ActivityKind::Sleep),
        "nocny marek nie śpi wcale"
    );
}

#[test]
fn niedobor_zdrowia_wysyla_do_lekarza_a_nie_do_sklepu() {
    let mut s = Scena::anna();
    s.needs_stan.level[NeedKind::Health.as_index()] = 10;
    s.stock = HouseholdView::FULL;
    let ctx = s.ctx();
    let mut c = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut c, &mut log);

    let cel_przychodni = magnat_agents::knowledge_key(budynek(PRZYCHODNIA)).unwrap();
    assert!(
        c.slots()
            .iter()
            .any(|x| x.kind == ActivityKind::Errand as u8 && x.target == cel_przychodni),
        "zdrowie 10, a wizyty u lekarza brak"
    );
    assert!(
        log.entries().iter().any(|e| matches!(
            e.reason,
            DecisionReason::NeedCritical {
                need: NeedKind::Health,
                ..
            }
        )),
        "wizyta bez powodu"
    );
}

#[test]
fn absencja_z_deprywacji_zabiera_prace_z_planu() {
    // B-7: skutek progowy `AbsenceRisk` stosuje planer, bo tylko on wie, co znaczy
    // „nie poszedł do pracy". Mobilność ma w danych ryzyko 1000‰, więc jej brak
    // zdejmuje pracę z pewnością — i to jest treść tego testu, nie losowanie.
    let mut s = Scena::anna();
    s.needs_stan.level[NeedKind::Mobility.as_index()] = 0;
    s.day = 3;
    let ctx = s.ctx();
    let mut c = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut c, &mut log);

    assert!(
        !ma_rodzaj(&c, ActivityKind::Work),
        "mieszkaniec bez mobilności poszedł do pracy"
    );
    assert!(
        log.skipped().any(|e| matches!(
            e.reason,
            DecisionReason::Deprivation {
                effect: magnat_core::DeprivationEffect::AbsenceRisk,
                ..
            }
        )),
        "absencja bez powodu w karcie inspekcji"
    );
    // Absencja nie znaczy pustka: doba nadal ma sen i posiłek.
    assert!(ma_rodzaj(&c, ActivityKind::Sleep));
    assert!(ma_rodzaj(&c, ActivityKind::Eat));
}

#[test]
fn kazdy_dojazd_trwa_tyle_co_droga_z_miejsca_poprzedniego() {
    // Spójność planu z `TravelOracle` z tolerancją 0. Bez niej pieszy wyrusza
    // w minucie zaplanowanego wyjścia i dociera **kiedy indziej** niż plan zakłada —
    // a wtedy „pieszy dociera na czas" jest tylko deklaracją.
    //
    // Pułapka, którą ten test złapał: miejscem slotu dojazdu jest jego **cel**,
    // więc luka kończąca się dojazdem wymaga obecności w jego **punkcie wyjścia**,
    // a nie w celu. Planer, który tego nie odróżniał, odsyłał mieszkańca ze sklepu
    // do pracy, a potem liczył drogę do domu tak, jakby wychodził z domu.
    use magnat_agents::TravelOracle;

    for i in 0..300u32 {
        let mut s = Scena::anna();
        s.day = u64::from(i % 7);
        s.employment.shift = match i % 5 {
            0 => ShiftKind::Early,
            1 => ShiftKind::Afternoon,
            2 => ShiftKind::Night,
            3 => ShiftKind::Weekend,
            _ => ShiftKind::Day,
        } as u8;
        if s.employment.shift == ShiftKind::Weekend as u8 {
            s.employment.work_days = 0b110_0000;
        }
        s.needs_stan.level = std::array::from_fn(|n| ((i * 13 + n as u32 * 7) % 101) as u8);
        s.stock = std::array::from_fn(|n| ((i + n as u32 * 3) % 6) as u8);
        let ctx = s.ctx();
        let mut c = DayCanvas::new();
        plan_day(&ctx, &mut c);

        for (k, slot) in c.slots().iter().enumerate() {
            if slot.kind != ActivityKind::Commute as u8 {
                continue;
            }
            let skad = if k == 0 { ctx.home } else { c.place_of(k - 1) };
            let dokad = c.place_of(k);
            let ile = s
                .oracle
                .estimate(
                    skad,
                    dokad,
                    magnat_core::MinuteOfDay::new(slot.start_min),
                    &ctx.citizen,
                )
                .minutes;
            assert_eq!(
                slot.dur_min, ile,
                "mieszkaniec {i}, zmiana {}, slot {k}: plan daje {} min na drogę, \
                 a `TravelOracle` liczy {ile}",
                s.employment.shift, slot.dur_min
            );
        }
    }
}
