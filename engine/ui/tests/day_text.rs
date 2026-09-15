//! **Złoty test wydruku dnia** — kryterium ukończenia WP12 (M3d).
//!
//! Wydruk dla gracza w układzie z PRD §5.5: godzina, czynność, uzasadnienie
//! w nawiasie. Test jest tekstowy i biegnie bez GPU, więc karta inspekcji ma
//! strażnika w CI, a nie tylko na przeglądzie (ryzyko R8).
//!
//! Scena jest tą samą, którą PRD §5.5 podaje jako przykład: **Anna**, 34 lata,
//! księgowa, zmiana 8–16, dziecko w szkole, pieczywo na jeden dzień. Powtórzenie
//! sceny z `sim/agents/tests/planner.rs` jest świadome: tamten test broni **planera**
//! i drukuje diagnostycznie po angielsku, ten broni **interfejsu gracza** i drukuje
//! w obu językach. Wspólny byłby jeden plik złoty dla dwóch różnych rzeczy.
//!
//! Po M4b dublerem podróży jest `StraightLineTravel` — `WalkOracle` z siatką ulic
//! zniknął z `sim/agents` razem z przeniesieniem tras do `sim/traffic`. Wzorce złote
//! zostały przez to wygenerowane na nowo: plan Anny z odległościami w linii prostej
//! robi zakupy wieczorem (`ChosenNearest`) zamiast po drodze z pracy (`ChosenOnRoute`).
//!
//! **Korekta po M4d (`S-15`):** stała tu wcześniej notatka, że „`engine/ui` od
//! `sim/traffic` nie zależy i zależeć nie ma". Zależy od WP11 i ma zależeć: karta
//! podróży czyta `ModeDecision`, `TripLedger` i `LedgerEntry`, a te trzy typy M4 §6
//! wypisuje jako kontrakt z adresatem „karta inspekcji". **Ten** test dublera nie
//! porzuca — nie dlatego, że nie może, tylko dlatego, że broni planera dnia, a nie
//! podróży: prawdziwy oracle wciągnąłby do niego router, flotę i stacje paliw.

use magnat_agents::{
    plan_day_explained, DayCanvas, Employment, HouseholdView, Identity, InfinitePlaces, Knowledge,
    KnowledgeKind, KnowledgeView, NeedTable, Needs, Personality, PlaceEntry, PlaceTable, PlanCtx,
    ReasonLog, Residence, ShiftKind, StraightLineTravel, Vitals,
};
use magnat_core::{
    ActivityKind, BuildingId, CitizenId, DayOfWeek, Entity, NeedKind, PlaceKind, PlaceRef, SiteId,
    StockCat, WorldCoord, STOCK_CAT_COUNT,
};
use magnat_ui::{
    render_day_text, ActualBlock, Catalog, CitizenHeader, DayTimeline, Locale, TimelineRow,
};
use std::num::NonZeroU32;
use std::sync::Arc;

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

fn katalog() -> PlaceTable {
    let wpis = |place, kind, x, y| PlaceEntry {
        place,
        kind,
        at: WorldCoord::new(x, y, 0),
    };
    PlaceTable::build(vec![
        wpis(budynek(DOM), PlaceKind::Home, 0, 0),
        wpis(zaklad(PRACA), PlaceKind::Workplace, 90_000, 0),
        wpis(budynek(SKLEP_PO_DRODZE), PlaceKind::Grocery, 60_000, 0),
        wpis(budynek(SKLEP_PRZY_DOMU), PlaceKind::Grocery, 0, 30_000),
        wpis(budynek(SZKOLA), PlaceKind::Education, 30_000, 0),
        wpis(budynek(PRZYCHODNIA), PlaceKind::Doctor, 60_000, 30_000),
    ])
}

struct Anna {
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

impl Anna {
    fn new() -> Anna {
        let places = Arc::new(katalog());
        let tabela = Arc::new(NeedTable::load_default().expect("data/needs/needs.ron"));
        let mut stock = HouseholdView::FULL;
        stock[StockCat::Food.as_index()] = 1;

        Anna {
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
                status: 58,
                ..Vitals::default()
            },
            needs_stan: Needs {
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
            oracle: StraightLineTravel::new(places),
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

fn naglowek(c: &Catalog, l: Locale) -> CitizenHeader {
    CitizenHeader {
        // Imię i nazwisko pochodzą z `data/names/` i **nie są** lokalizacją UI.
        name: "Anna Wiśniewska".to_string(),
        age_years: 34,
        occupation: c.fmt_key(l, "ui.commitment.Work", &[]),
        address: c.fmt_key(
            l,
            "ui.card.address",
            &[("budynek", "1"), ("lokal", "4"), ("dzielnica", "1")],
        ),
    }
}

#[test]
fn zloty_wydruk_dnia_po_polsku() {
    let s = Anna::new();
    let ctx = s.ctx();
    let mut canvas = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut canvas, &mut log);

    let c = Catalog::load().expect("data/locale/");
    let timeline = DayTimeline {
        canvas: &canvas,
        log: &log,
        actual: &[],
        stored: &[],
    };
    let wydruk = render_day_text(&c, Locale::Pl, &naglowek(&c, Locale::Pl), &timeline);

    if std::env::var("ZAPISZ_GOLDEN").is_ok() {
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/anna_pl.txt"),
            &wydruk,
        )
        .expect("zapis wzorca");
    }
    let wzorzec = include_str!("golden/anna_pl.txt");
    assert_eq!(
        wydruk, wzorzec,
        "wydruk dnia dla gracza się zmienił.\n--- otrzymano ---\n{wydruk}"
    );
}

#[test]
fn zloty_wydruk_dnia_po_angielsku() {
    let s = Anna::new();
    let ctx = s.ctx();
    let mut canvas = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut canvas, &mut log);

    let c = Catalog::load().expect("data/locale/");
    let timeline = DayTimeline {
        canvas: &canvas,
        log: &log,
        actual: &[],
        stored: &[],
    };
    let wydruk = render_day_text(&c, Locale::En, &naglowek(&c, Locale::En), &timeline);

    if std::env::var("ZAPISZ_GOLDEN").is_ok() {
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/anna_en.txt"),
            &wydruk,
        )
        .expect("zapis wzorca");
    }
    let wzorzec = include_str!("golden/anna_en.txt");
    assert_eq!(
        wydruk, wzorzec,
        "wydruk dnia dla gracza się zmienił.\n--- otrzymano ---\n{wydruk}"
    );
}

#[test]
fn kazdy_slot_planu_ma_niepusty_powod() {
    // Kryterium WP12: „każdy slot planu ma niepusty powód". Sprawdzane na tekście
    // dla gracza, a nie na tagu w slocie — bo to tekst jest tym, co widzi gracz.
    let s = Anna::new();
    let ctx = s.ctx();
    let mut canvas = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut canvas, &mut log);

    let c = Catalog::load().expect("data/locale/");
    for l in Locale::ALL {
        let timeline = DayTimeline {
            canvas: &canvas,
            log: &log,
            actual: &[],
            stored: &[],
        };
        let rows: Vec<TimelineRow> = timeline.rows(&c, l);
        assert!(!rows.is_empty(), "pusty plan");
        for r in &rows {
            assert!(!r.reason.is_empty(), "slot {} bez powodu", r.slot);
            assert!(
                !r.reason.contains("bez podanego powodu") && !r.reason.contains("no reason"),
                "slot {} ({}) ma powód `Unspecified`",
                r.slot,
                r.label
            );
            assert!(!r.label.is_empty());
            assert!(!r.reason.contains('{'), "nietrafione podstawienie: {}", r.reason);
        }
    }
}

#[test]
fn realizacja_rysuje_sie_obok_planu_i_rozjazd_widac() {
    // Kryterium WP12: „realizacja (faktyczne zdarzenia DES) rysowana obok planu,
    // rozjazdy widoczne".
    let s = Anna::new();
    let ctx = s.ctx();
    let mut canvas = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut canvas, &mut log);
    let c = Catalog::load().expect("data/locale/");

    // Pierwszy slot wykonany punktualnie, drugi z kwadransem opóźnienia.
    let sloty = canvas.slots();
    let realizacja = vec![
        ActualBlock {
            start_min: sloty[0].start_min,
            end_min: sloty[0].end_min(),
            kind: sloty[0].kind,
            slot: 0,
        },
        ActualBlock {
            start_min: sloty[1].start_min + 15,
            end_min: sloty[1].end_min() + 15,
            kind: sloty[1].kind,
            slot: 1,
        },
    ];
    let timeline = DayTimeline {
        canvas: &canvas,
        log: &log,
        actual: &realizacja,
        stored: &[],
    };
    let rows = timeline.rows(&c, Locale::Pl);
    assert_eq!(rows[0].drift_min, 0);
    assert!(!rows[0].drifted());
    assert_eq!(rows[1].drift_min, 15);
    assert!(rows[1].drifted(), "kwadrans rozjazdu ma być widoczny");

    let wydruk = render_day_text(&c, Locale::Pl, &naglowek(&c, Locale::Pl), &timeline);
    assert!(
        wydruk.contains("plan mówił"),
        "wydruk nie pokazuje rozjazdu:\n{wydruk}"
    );
}

#[test]
fn plan_ma_ksztalt_z_prd() {
    // Złoty plik pilnuje stabilności, ta asercja pilnuje sensu: dzień wzorcowy
    // z PRD §5.5 ma mieć sen, posiłek, dojazd, pracę i zakupy po drodze.
    let s = Anna::new();
    let ctx = s.ctx();
    let mut canvas = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut canvas, &mut log);
    let ma = |k: ActivityKind| canvas.slots().iter().any(|s| s.kind == k as u8);
    assert!(ma(ActivityKind::Sleep), "brak snu");
    assert!(ma(ActivityKind::Eat), "brak posiłku");
    assert!(ma(ActivityKind::Commute), "brak dojazdu");
    assert!(ma(ActivityKind::Work), "brak pracy");
    assert!(ma(ActivityKind::Shop), "brak zakupów");
}
