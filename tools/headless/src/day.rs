//! Scenariusz `day` — wynik do pokazania podfazy M3b.
//!
//! „Wydruk dnia wzorcowego: pobudka → posiłek → dojazd → praca → zadanie po drodze →
//! dom → czas wolny → sen; pieszy dociera na czas." Dokładnie to i nic więcej.
//!
//! Miasto jest **syntetyczne**: siatka ulic i katalog miejsc bez generatora M2, bo
//! Etap 8 (zaludnienie prawdziwego miasta) to M3d. Mieszkańcy są zwykłymi strukturami,
//! a nie encjami ECS — spadek potrzeb i rachunek pamięci mierzy scenariusz `agents`
//! z M3a, a tutaj chodzi o **planer, kolejkę zdarzeń i ruch pieszy**.
//!
//! Pętla zdarzeń jest w runnerze, nie w systemie ECS, z tego samego powodu co w M3a:
//! systemy ECS i ich częstotliwości projektuje M3d (§5.12).

use clap::Args;
use magnat_agents::{
    plan_day, plan_day_explained, render_day_debug, CitizenView, DayCanvas, Employment, EventKind,
    EventQueue, HouseholdView, Identity, InfinitePlaces, Knowledge, KnowledgeKind, KnowledgeView,
    NeedTable, Needs, Personality, PlaceEntry, PlaceTable, PlanCtx, ReasonLog, Residence,
    ShiftKind, SimEvent, StraightLineTravel, TravelOracle, TripRequest, Vitals,
};
use magnat_core::{
    rng, ActivityKind, BuildingId, CitizenId, DayOfWeek, Entity, HouseholdId, MinuteOfDay,
    PlaceKind, PlaceRef, SiteId, StreamId, Tick, WorldCoord, STOCK_CAT_COUNT,
};
use std::num::NonZeroU32;
use std::sync::Arc;

#[derive(Args, Debug)]
pub struct DayArgs {
    /// Ziarno świata.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// Liczba mieszkańców.
    #[arg(long, default_value_t = 20_000)]
    pub citizens: u32,

    /// Ile dób gry przebiec.
    #[arg(long, default_value_t = 1)]
    pub days: u32,

    /// Bok siatki ulic w węzłach (odstęp 150 m).
    #[arg(long, default_value_t = 24)]
    pub grid: u32,

    /// Wydruk planu tego mieszkańca (indeks); brak = pierwszy pracujący.
    #[arg(long)]
    pub print: Option<u32>,

    /// Ilu mieszkańców trzymać w LOD Mikro (pozycja co 100 ms).
    #[arg(long, default_value_t = 0)]
    pub micro: u32,
}

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).unwrap())
}

/// Identyfikatory miejsc są rozłączne z identyfikatorami mieszkańców, żeby wydruk
/// dało się czytać: domy od 1 mln, zakłady od 2 mln, sklepy od 3 mln.
const ID_DOM: u32 = 1_000_000;
const ID_PRACA: u32 = 2_000_000;
const ID_SKLEP: u32 = 3_000_000;
const KROK_CM: i32 = 15_000; // 150 m

struct Miasto {
    places: Arc<PlaceTable>,
    nodes: Vec<WorldCoord>,
    segments: Vec<(u32, u32, u32)>,
    domy: Vec<PlaceRef>,
    zaklady: Vec<PlaceRef>,
    sklepy: Vec<PlaceRef>,
}

fn zbuduj_miasto(bok: u32) -> Miasto {
    let bok = bok.max(4) as i32;
    let mut nodes = Vec::new();
    for y in 0..bok {
        for x in 0..bok {
            nodes.push(WorldCoord::new(x * KROK_CM, y * KROK_CM, 0));
        }
    }
    let idx = |x: i32, y: i32| (y * bok + x) as u32;
    let mut segments = Vec::new();
    for y in 0..bok {
        for x in 0..bok {
            if x + 1 < bok {
                segments.push((idx(x, y), idx(x + 1, y), KROK_CM as u32));
            }
            if y + 1 < bok {
                segments.push((idx(x, y), idx(x, y + 1), KROK_CM as u32));
            }
        }
    }

    // Domy w każdym kwartale, zakłady i sklepy rzadziej — rozkład bez pretensji
    // do realizmu, bo od realizmu jest Etap 8 w M3d.
    let mut wpisy = Vec::new();
    let (mut domy, mut zaklady, mut sklepy) = (Vec::new(), Vec::new(), Vec::new());
    let mut n = 0u32;
    for y in 0..bok - 1 {
        for x in 0..bok - 1 {
            let at = WorldCoord::new(x * KROK_CM + 5_000, y * KROK_CM + 5_000, 0);
            let dom = PlaceRef::Building(BuildingId(encja(ID_DOM + n)));
            domy.push(dom);
            wpisy.push(PlaceEntry {
                place: dom,
                kind: PlaceKind::Home,
                at,
            });
            if n.is_multiple_of(4) {
                let z = PlaceRef::Site(SiteId(encja(ID_PRACA + n)));
                zaklady.push(z);
                wpisy.push(PlaceEntry {
                    place: z,
                    kind: PlaceKind::Workplace,
                    at: WorldCoord::new(at.x + 3_000, at.y + 3_000, 0),
                });
            }
            if n.is_multiple_of(6) {
                let s = PlaceRef::Building(BuildingId(encja(ID_SKLEP + n)));
                sklepy.push(s);
                wpisy.push(PlaceEntry {
                    place: s,
                    kind: PlaceKind::Grocery,
                    at: WorldCoord::new(at.x + 6_000, at.y, 0),
                });
            }
            n += 1;
        }
    }

    Miasto {
        places: Arc::new(PlaceTable::build(wpisy)),
        nodes,
        segments,
        domy,
        zaklady,
        sklepy,
    }
}

/// Mieszkaniec jako zwykłe pola — `PlanCtx` trzyma do nich referencje.
struct Stan {
    identity: Identity,
    vitals: Vitals,
    needs: Needs,
    personality: Personality,
    residence: Residence,
    employment: Employment,
    wiedza: Vec<Knowledge>,
    stock: [u8; STOCK_CAT_COUNT],
    home: PlaceRef,
    work: Option<PlaceRef>,
}

fn zaludnij(seed: u64, n: u32, miasto: &Miasto) -> Vec<Stan> {
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let mut r = rng(seed, StreamId::PersonalityGen, i, Tick(0));
        let wiek = 18 + r.gen_range_u32(50);
        let pracuje = wiek < 65;
        let dom = miasto.domy[(i as usize) % miasto.domy.len()];
        let praca = miasto.zaklady[(i as usize * 7) % miasto.zaklady.len()];

        // Osiem najbliższych sklepów wybranych po indeksie — zasiew wiedzy
        // z sąsiedztwa i plotki to M3c (§5.7), tutaj wystarczy, żeby było co wybierać.
        let baza = (i as usize * 3) % miasto.sklepy.len();
        let wiedza = (0..8)
            .map(|k| {
                let s = miasto.sklepy[(baza + k) % miasto.sklepy.len()];
                Knowledge {
                    target: magnat_agents::knowledge_key(s).unwrap_or(0),
                    day: 0,
                    score: 50 + r.gen_range_u32(50) as u8,
                    kind: KnowledgeKind::Visited as u8,
                }
            })
            .collect();

        let mut stock = [30u8; STOCK_CAT_COUNT];
        stock[0] = r.gen_range_u32(5) as u8;

        out.push(Stan {
            identity: Identity {
                birth_day: -(360 * wiek as i32),
                flags: Identity::FLAG_ALIVE | u8::from(r.gen_bool_permille(500)),
                household: i / 3,
                ..Identity::default()
            },
            vitals: Vitals {
                health: 55 + r.gen_range_u32(45) as u8,
                energy: 45 + r.gen_range_u32(55) as u8,
                ..Vitals::default()
            },
            needs: Needs {
                level: std::array::from_fn(|_| 20 + r.gen_range_u32(80) as u8),
                updated_at: 0,
            },
            personality: Personality(std::array::from_fn(|_| r.gen_q().get())),
            residence: Residence {
                building: magnat_agents::knowledge_key(dom).unwrap_or(0),
                unit: (i % 8) as u16,
                district: (i % 12) as u16,
            },
            employment: if pracuje {
                Employment {
                    site: magnat_agents::knowledge_key(praca).unwrap_or(0),
                    role: (i % 40) as u16,
                    shift: match r.gen_range_u32(10) {
                        0 => ShiftKind::Early,
                        1 => ShiftKind::Afternoon,
                        2 => ShiftKind::Night,
                        _ => ShiftKind::Day,
                    } as u8,
                    flags: 0,
                    commute_baseline_min: 0,
                    work_days: Employment::WEEKDAYS,
                    _pad: 0,
                }
            } else {
                Employment {
                    flags: Employment::FLAG_RETIRED | Employment::FLAG_UNEMPLOYED,
                    ..Employment::default()
                }
            },
            wiedza,
            stock,
            home: dom,
            work: if pracuje { Some(praca) } else { None },
        });
    }
    out
}

fn widok<'a>(s: &'a Stan, i: u32, dzien: u64) -> CitizenView<'a> {
    CitizenView {
        id: CitizenId(encja(i)),
        identity: &s.identity,
        vitals: &s.vitals,
        needs: &s.needs,
        personality: &s.personality,
        residence: &s.residence,
        today: dzien as i32,
    }
}

#[allow(clippy::too_many_arguments)]
fn kontekst<'a>(
    s: &'a Stan,
    i: u32,
    dzien: u64,
    seed: u64,
    tabela: &'a NeedTable,
    places: &'a InfinitePlaces,
    oracle: &'a StraightLineTravel,
    brak_eskorty: &'a [PlaceRef],
) -> PlanCtx<'a> {
    PlanCtx {
        seed,
        day: dzien,
        dow: DayOfWeek::from_day_index(dzien),
        citizen: widok(s, i, dzien),
        household: HouseholdView {
            id: HouseholdId(encja(s.identity.household)),
            stock: &s.stock,
            escorts: brak_eskorty,
                pickups: brak_eskorty,
        },
        employment: &s.employment,
        known: KnowledgeView::new(&s.wiedza),
        needs: tabela,
        home: s.home,
        work: s.work,
        school: None,
        places,
        travel: oracle,
        max_task_travel_min: 30,
    }
}

pub fn run(a: &DayArgs) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let miasto = zbuduj_miasto(a.grid);
    let tabela = Arc::new(NeedTable::load_default()?);
    let places = InfinitePlaces::new(miasto.places.clone(), tabela.clone());
    // Scenariusz M3b mierzy dobę mieszkańca, nie ruch: estymatorem jest dubler
    // w linii prostej z `sim/agents`. Sieć, pojazdy i korki pokazuje `m4b`.
    let mut oracle = StraightLineTravel::new(miasto.places.clone());
    let ludzie = zaludnij(a.seed, a.citizens, &miasto);
    let brak_eskorty: Vec<PlaceRef> = Vec::new();
    eprintln!(
        "miasto: {} węzłów, {} odcinków, {} miejsc · {} mieszkańców w {:.2} s",
        miasto.nodes.len(),
        miasto.segments.len(),
        miasto.places.len(),
        ludzie.len(),
        start.elapsed().as_secs_f64()
    );

    let mut plany = vec![DayCanvas::new(); ludzie.len()];
    let mut q = EventQueue::new();
    let mut minuty = [0u64; ActivityKind::ALL.len()];
    let mut dojazdy: Vec<u16> = Vec::new();
    let (mut na_czas, mut spoznione) = (0u64, 0u64);
    let mut zakonczone = 0u64;
    let mut bufor: Vec<SimEvent> = Vec::with_capacity(65_536);
    let bieg = std::time::Instant::now();

    for dzien in 0..u64::from(a.days) {
        // Planowanie doby. Pętla po mieszkańcach, każdy ze swoim strumieniem RNG —
        // kolejność nie ma wpływu na wynik (00 §3.1).
        let plan_start = std::time::Instant::now();
        for (i, s) in ludzie.iter().enumerate() {
            let ctx = kontekst(
                s,
                i as u32,
                dzien,
                a.seed,
                &tabela,
                &places,
                &oracle,
                &brak_eskorty,
            );
            plan_day(&ctx, &mut plany[i]);
        }
        let plan_czas = plan_start.elapsed();

        // Lazy scheduling (§5.2): do kolejki wchodzi wyłącznie **pierwsze** zdarzenie
        // mieszkańca; obsługa każdego harmonogramuje następne.
        for (i, canvas) in plany.iter().enumerate() {
            if let Some(slot) = canvas.slots().first() {
                q.schedule(SimEvent::new(
                    (dzien * 1440) as u32 + u32::from(slot.start_min),
                    i as u32,
                    EventKind::StartActivity,
                    0,
                ));
            }
        }

        for _ in 0..1440 {
            q.drain_minute(&mut bufor);
            // Zegar kolejki stoi na minucie właśnie rozdawanej (korekta D-14), więc
            // `begin_trip` liczy przybycie od tej samej minuty, od której liczył planer.
            let teraz = q.now();
            for e in std::mem::take(&mut bufor) {
                let i = e.actor as usize;
                let Some(slot) = plany[i].slots().get(e.slot as usize).copied() else {
                    continue;
                };
                match e.event_kind() {
                    Some(EventKind::StartActivity) => {
                        minuty[slot.kind as usize] += u64::from(slot.dur_min);
                        // Dojazd zgłaszamy do `TravelOracle` — to on, a nie planer,
                        // mówi, kiedy pieszy faktycznie dotrze.
                        if slot.kind == ActivityKind::Commute as u8 {
                            let cel = plany[i].place_of(e.slot as usize);
                            let skad = if e.slot == 0 {
                                ludzie[i].home
                            } else {
                                plany[i].place_of(e.slot as usize - 1)
                            };
                            let w = widok(&ludzie[i], e.actor, dzien);
                            let h = oracle.begin_trip(
                                TripRequest {
                                    traveller: w.id,
                                    from: skad,
                                    to: cel,
                                    depart: MinuteOfDay::new(slot.start_min),
                                    slot: e.slot,
                                },
                                &w,
                                &mut q,
                            );
                            dojazdy.push(h.minutes);
                            if a.micro > e.actor {
                                oracle.enter_micro(&h, e.actor, MinuteOfDay::new(slot.start_min));
                            }
                        }
                        // Lazy scheduling: rozpoczęcie czynności harmonogramuje jej
                        // koniec **i** początek następnej. Łańcuch przez `EndActivity`
                        // by się urwał — sloty planu przylegają do siebie, więc koniec
                        // jednego i początek następnego wypadają w tej samej minucie,
                        // a kubełek tej minuty jest już rozdany (korekta D-14).
                        // Czynność kończąca się o północy nie dostaje `EndActivity`
                        // w tej dobie: jej koniec jest pierwszą minutą doby następnej,
                        // a tę otwiera `PlanDay` (M3d §5.12). Plan nie przechodzi przez
                        // północ, więc nie ma tu nic do dokończenia.
                        if slot.end_min() < 1440 {
                            q.schedule(SimEvent::new(
                                (dzien * 1440) as u32 + u32::from(slot.end_min()),
                                e.actor,
                                EventKind::EndActivity,
                                e.slot,
                            ));
                        }
                        let nastepny = e.slot as usize + 1;
                        if let Some(s) = plany[i].slots().get(nastepny) {
                            q.schedule(SimEvent::new(
                                (dzien * 1440) as u32 + u32::from(s.start_min),
                                e.actor,
                                EventKind::StartActivity,
                                nastepny as u8,
                            ));
                        }
                    }
                    Some(EventKind::Arrive) => {
                        // „Pieszy dociera na czas": mezo policzyło ten sam czas przy
                        // planowaniu i przy wyruszeniu, więc przybycie ma wypaść
                        // dokładnie na koniec slotu dojazdu. Rozjazd znaczy, że planer
                        // i `TravelOracle` przestały mówić o tej samej podróży.
                        let planowane = (dzien * 1440) as u32 + u32::from(slot.end_min());
                        if e.time == planowane {
                            na_czas += 1;
                        } else {
                            spoznione += 1;
                        }
                    }
                    Some(EventKind::EndActivity) => {
                        // Tu M3d wywoła `PlaceProvider::fulfil` i podniesie potrzebę,
                        // a M5 pobierze pieniądze. W M3b zdarzenie istnieje po to,
                        // żeby łańcuch był kompletny i mierzalny.
                        zakonczone += 1;
                    }
                    _ => {}
                }
            }
            if a.micro > 0 {
                // Sześćset podkroków po 100 ms w każdej minucie (00 §4). Liczymy
                // co dziesiąty, bo runner nie rysuje — chodzi o koszt, nie o obraz.
                for k in (0..600).step_by(10) {
                    oracle.micro_step(u64::from(teraz) * 60_000 + k * 100);
                }
                oracle.micro_retire((teraz % 1440) as u16);
            }
        }
        eprintln!(
            "doba {dzien} ({:?}): plan {:.0} µs/mieszkańca",
            DayOfWeek::from_day_index(dzien),
            plan_czas.as_secs_f64() * 1e6 / ludzie.len() as f64
        );
    }

    let czas = bieg.elapsed();
    eprintln!(
        "{} dób gry w {:.2} s · {} dojazdów, {} na czas, {} spóźnionych",
        a.days,
        czas.as_secs_f64(),
        dojazdy.len(),
        na_czas,
        spoznione
    );
    eprintln!("czynności zakończonych: {zakonczone}");

    raport(&minuty, &dojazdy, a);
    wydruk_dnia(&ludzie, a, &tabela, &places, &oracle, &brak_eskorty);

    if spoznione > 0 {
        eprintln!("BŁĄD: {spoznione} przybyć poza planem — mezo rozjechało się z planerem");
        return Ok(std::process::ExitCode::FAILURE);
    }
    Ok(std::process::ExitCode::SUCCESS)
}

fn raport(minuty: &[u64], dojazdy: &[u16], a: &DayArgs) {
    let suma: u64 = minuty.iter().sum();
    println!("wykorzystanie doby (średnio minut na mieszkańca na dobę)");
    for (i, k) in ActivityKind::ALL.iter().enumerate() {
        if minuty[i] == 0 {
            continue;
        }
        let na_osobe = minuty[i] as f64 / f64::from(a.citizens) / f64::from(a.days);
        println!(
            "  {:<9} {:>7.1} min  {:>5.1} %",
            k.name(),
            na_osobe,
            minuty[i] as f64 * 100.0 / suma.max(1) as f64
        );
    }

    if dojazdy.is_empty() {
        return;
    }
    let mut posortowane = dojazdy.to_vec();
    posortowane.sort_unstable();
    let percentyl = |p: usize| posortowane[(posortowane.len() - 1) * p / 100];
    println!(
        "czas dojścia: mediana {} min, p90 {} min, maks {} min",
        percentyl(50),
        percentyl(90),
        posortowane[posortowane.len() - 1]
    );
    let mut histogram = [0u32; 7];
    for d in &posortowane {
        let kubelek = match d {
            0..=5 => 0,
            6..=10 => 1,
            11..=15 => 2,
            16..=20 => 3,
            21..=30 => 4,
            31..=45 => 5,
            _ => 6,
        };
        histogram[kubelek] += 1;
    }
    const ETYKIETY: [&str; 7] = ["0–5", "6–10", "11–15", "16–20", "21–30", "31–45", "45+"];
    for (i, n) in histogram.iter().enumerate() {
        let udzial = f64::from(*n) * 100.0 / posortowane.len() as f64;
        let slupek = "#".repeat((udzial / 2.0) as usize);
        println!("  {:>6} min {n:>7}  {slupek}", ETYKIETY[i]);
    }
}

/// Wydruk planu jednego mieszkańca — ten sam, który w CI jest złotym testem.
fn wydruk_dnia(
    ludzie: &[Stan],
    a: &DayArgs,
    tabela: &NeedTable,
    places: &InfinitePlaces,
    oracle: &StraightLineTravel,
    brak_eskorty: &[PlaceRef],
) {
    let i = match a.print {
        Some(i) => i.min(a.citizens - 1),
        None => ludzie.iter().position(|s| s.work.is_some()).unwrap_or(0) as u32,
    };
    let s = &ludzie[i as usize];
    let dzien = u64::from(a.days.saturating_sub(1));
    let ctx = kontekst(s, i, dzien, a.seed, tabela, places, oracle, brak_eskorty);
    let mut canvas = DayCanvas::new();
    let mut log = ReasonLog::new();
    plan_day_explained(&ctx, &mut canvas, &mut log);
    println!("\n{}", render_day_debug(&ctx, &canvas, &log));
}
