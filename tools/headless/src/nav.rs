//! Podpolecenie `nav` — artefakt fazy M4a: inspektor grafu i pomiar routingu
//! (M4a WP1 i WP2).
//!
//! Headless-first jest kontraktem (00 §6): graf nawigacyjny i router muszą dać się
//! zbudować, obejrzeć i zmierzyć na maszynie CI bez karty graficznej. Inspektor grafu
//! z kryterium WP1 („devtools rysuje krawędzie z atrybutami") jest tutaj kartą tekstową,
//! a nie nakładką w kliencie — atrybut, którego nie widać w tej karcie, nie istnieje
//! też dla routera, bo obie strony czytają tę samą strukturę.
//!
//! Pomiar jest częścią artefaktu, nie dodatkiem: budżety §7.3 (budowa < 400 ms,
//! zapytanie CCH ≤ 60 µs p95, kustomizacja trzech profili ≤ 450 ms) są liczbami,
//! które to podpolecenie wypisuje razem z werdyktem, żeby regres było widać bez
//! uruchamiania criteriona.

use clap::Args as ClapArgs;
use magnat_core::{rng, IVec2, StreamId, Tick, TransportMode};
use magnat_jobs::JobPool;
use magnat_nav::{
    cch, profile_weights, ChScratch, Modality, NavGraphs, NavJob, NavRouter, NodeControl, NodeId,
    RebuildProgress, RebuildQueue, RoadGraph, RoadGraphBuilder, Route, RouteProfile, RouteQuery,
    Router, WORK_PER_TICK,
};
use magnat_voxel::MaterialRegistry;
use magnat_world::{
    build_nav, generate as generate_world, generate_city, CityPlan, Difficulty, Terrain,
    WorldGenParams,
};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

/// Budżet budowy grafu z WP1, w mikrosekundach.
const BUILD_BUDGET_US: u64 = 400_000;

/// Budżet zapytania CCH z WP2 (p95), w nanosekundach.
const QUERY_P95_BUDGET_NS: u64 = 60_000;

#[derive(ClapArgs, Debug)]
pub struct NavArgs {
    #[arg(long, default_value = "1")]
    seed: String,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "4km")]
    size: String,

    /// `coastal` | `mountain` | `lowland` | `river` | `desert`.
    #[arg(long, default_value = "lowland")]
    region: String,

    /// `1950` | `1970` | `1990` | `2010` | `2020`.
    #[arg(long, default_value = "1990")]
    epoch: String,

    /// `industrial` | `port` | `university` | `tourist` | `agricultural` | `mixed`.
    #[arg(long, default_value = "mixed")]
    profile: String,

    #[arg(long, default_value_t = 0)]
    threads: usize,

    /// Inspektor grafu: wypisz atrybuty krawędzi wychodzących z węzła najbliższego
    /// `x,y` (metry).
    #[arg(long)]
    inspect: Option<String>,

    /// Ile losowych (deterministycznych) par tras zmierzyć. 0 = nie mierz.
    #[arg(long, default_value_t = 2000)]
    queries: u32,

    /// Zmierz też instancję syntetyczną `cols x rows x per_street` (np. „100x100x10").
    #[arg(long)]
    synthetic: Option<String>,

    /// Porównaj CCH z Dijkstrą na N parach i wypisz odsetek zgodności.
    #[arg(long, default_value_t = 0)]
    verify: u32,
}

/// Ziarno dziesiętnie albo szesnastkowo — `0xC0FFEE` z PRD §4.1 ma się dać wpisać wprost.
// Kopia z `worldgen` (tam prywatna); dwie linie parsowania nie są wiedzą, której
// współdzielenie cokolwiek kupuje.
fn parse_seed(s: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let t = s.trim();
    let v = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(&hex.replace('_', ""), 16)?,
        None => t.replace('_', "").parse::<u64>()?,
    };
    Ok(v)
}

fn params(a: &NavArgs) -> Result<WorldGenParams, Box<dyn std::error::Error>> {
    let p = WorldGenParams {
        seed: parse_seed(&a.seed)?,
        size: a.size.parse()?,
        region: a.region.parse()?,
        epoch: a.epoch.parse()?,
        profile: a.profile.parse()?,
        difficulty: Difficulty::Normal,
    };
    p.validate()?;
    Ok(p)
}

pub fn run(a: &NavArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let p = params(a)?;
    let pool = JobPool::new(a.threads);
    eprintln!(
        "świat: seed {:#x} · {} · {} · {} · {} · {} wątków",
        p.seed,
        p.size.key(),
        p.region.key(),
        p.epoch.year(),
        p.profile.key(),
        pool.thread_count()
    );

    let (world, wrep) = generate_world(p, &pool)?;
    eprintln!("teren {:.1} ms", wrep.total_millis);
    let reg = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path(
        "materials",
    ))?);
    let terrain = Terrain::new(world, reg);
    let plan = CityPlan::from_world(&p);
    let t_city = Instant::now();
    let city = generate_city(&plan, &terrain, terrain.materials(), &pool)?;
    eprintln!(
        "miasto {:.1} ms",
        millis(t_city.elapsed().as_nanos() as u64)
    );

    // ── WP1: budowa grafu i walidacja ────────────────────────────────────────
    let (graphs, rep) = build_nav(&city)?;
    println!("── graf nawigacyjny (M4a/WP1) ──");
    println!(
        "{:<6} {:>8} {:>9} {:>8} {:>7} {:>9} {:>6} {:>10} {:>11} {:>10}",
        "warstwa",
        "węzły",
        "krawędzie",
        "skręty",
        "mosty",
        "izolowane",
        "ujścia",
        "słabo sp.",
        "silnie sp.",
        "długość km"
    );
    for (i, m) in Modality::ALL.iter().enumerate() {
        let l = &rep.layers[i];
        println!(
            "{:<6} {:>8} {:>9} {:>8} {:>7} {:>9} {:>6} {:>10} {:>11} {:>10}",
            m.name(),
            l.nodes,
            l.edges,
            l.turns,
            l.bridges,
            l.isolated,
            l.sinks,
            l.largest_component,
            l.largest_scc,
            l.total_length_km
        );
    }
    println!(
        "przesiadki {} · parcele z dojściem {} · bez dojścia {}",
        rep.transfers, rep.parcels_with_access, rep.parcels_without_access
    );
    let build_ok = rep.build_micros <= BUILD_BUDGET_US;
    println!(
        "budowa {:.1} ms wobec budżetu 400 ms — {}",
        millis(rep.build_micros * 1_000),
        werdykt(build_ok)
    );

    let n_districts = u16::try_from(city.districts.districts.len()).unwrap_or(u16::MAX);
    let mut router = build_router(graphs, n_districts, "miasto");

    if a.queries > 0 {
        zmierz_zapytania(&mut router, p.seed, a.queries);
    }

    let mut ok = build_ok;

    if a.verify > 0 {
        ok &= zweryfikuj(&router, p.seed, a.verify);
    }

    if let Some(spec) = &a.inspect {
        karta_wezla(router.graphs().layer(Modality::Road), spec)?;
    }

    // ── Rekontrakcja w budżecie: tick podmiany musi być tym zapowiedzianym ────
    ok &= przebuduj(&mut router);

    if let Some(spec) = &a.synthetic {
        let (cols, rows, per) = parse_synthetic(spec)?;
        println!();
        println!("── instancja syntetyczna {cols}×{rows}×{per} (J-11) ──");
        let t = Instant::now();
        let road = magnat_nav::synthetic_road_network(cols, rows, per, 10_000);
        println!(
            "budowa {:.1} ms · {} węzłów · {} krawędzi",
            millis(t.elapsed().as_nanos() as u64),
            road.node_count(),
            road.edge_count()
        );
        let mut r = build_router(warstwy_z(road), 1, "syntetyk");
        if a.queries > 0 {
            zmierz_zapytania(&mut r, p.seed, a.queries);
        }
    }

    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

// ── pomocnicze ───────────────────────────────────────────────────────────────

/// Nanosekundy na milisekundy. Float wchodzi wyłącznie do **wypisania** liczby —
/// nic z tego nie wraca do stanu symulacji (00 §2 dotyczy księgowości, nie raportu).
fn millis(ns: u64) -> f64 {
    ns as f64 / 1_000_000.0
}

fn mikro(ns: u64) -> f64 {
    ns as f64 / 1_000.0
}

fn werdykt(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

/// Warstwy `Foot`/`Bike`/`Rail` puste — instancja syntetyczna mierzy warstwę drogową.
fn warstwy_z(road: RoadGraph) -> NavGraphs {
    let pusta = |m: Modality| RoadGraphBuilder::new(m).with_turns(false).finish();
    NavGraphs::new(
        [
            road,
            pusta(Modality::Foot),
            pusta(Modality::Bike),
            pusta(Modality::Rail),
        ],
        Vec::new(),
    )
}

fn parse_synthetic(s: &str) -> Result<(u32, u32, u32), Box<dyn std::error::Error>> {
    let v: Vec<u32> = s
        .split(['x', 'X'])
        .map(|t| t.trim().parse::<u32>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("--synthetic oczekuje `kolumnyxwierszexodcinki`: {e}"))?;
    let [cols, rows, per] = v[..] else {
        return Err("--synthetic oczekuje trzech liczb, np. `100x100x10`".into());
    };
    Ok((cols, rows, per))
}

/// Budowa routera z osobnym pomiarem trzech etapów: kolejność kontrakcji liczy się raz
/// i nie zmienia jej kustomizacja — to jest cały powód wyboru CCH zamiast CH (§5.1).
fn build_router(graphs: NavGraphs, n_districts: u16, etykieta: &str) -> NavRouter {
    let road = graphs.layer(Modality::Road);
    let t = Instant::now();
    let order = cch::build_order(road);
    let t_order = t.elapsed().as_nanos() as u64;

    let t = Instant::now();
    let mut ch = cch::contract(road, &order);
    let t_contract = t.elapsed().as_nanos() as u64;

    let t = Instant::now();
    for profil in RouteProfile::ALL {
        let w = profile_weights(road, *profil, None);
        ch.customize(road, &w, road.weight_version);
    }
    let t_custom = t.elapsed().as_nanos() as u64;
    let luki = ch.arc_count();
    drop(ch);

    println!("── router ({etykieta}) ──");
    println!("kolejność kontrakcji   {:>9.1} ms", millis(t_order));
    println!(
        "kontrakcja             {:>9.1} ms  ({luki} łuków)",
        millis(t_contract)
    );
    println!(
        "kustomizacja ×3 profile{:>9.1} ms  wobec budżetu 450 ms — {}",
        millis(t_custom),
        werdykt(t_custom <= 450_000_000)
    );

    let t = Instant::now();
    let r = NavRouter::build(graphs, n_districts, 1 << 16);
    println!(
        "NavRouter::build       {:>9.1} ms  · pamięć {:.1} MB wobec budżetu 30 MB — {}",
        millis(t.elapsed().as_nanos() as u64),
        r.memory_bytes() as f64 / (1024.0 * 1024.0),
        werdykt(r.memory_bytes() <= 30 * 1024 * 1024)
    );
    r
}

/// Deterministyczne pary origin–dest na warstwie drogowej: tylko węzły, z których
/// w ogóle da się wyjechać (w warstwie rzadkiej większość węzłów nie ma krawędzi).
fn pary(road: &RoadGraph, seed: u64, n: u32) -> Vec<(NodeId, NodeId)> {
    let zrodla: Vec<u32> = (0..road.node_count() as u32)
        .filter(|&v| !road.out(NodeId(v)).is_empty())
        .collect();
    if zrodla.len() < 2 {
        return Vec::new();
    }
    let mut r = rng(seed, StreamId::EngineSelfTest, 0, Tick(0));
    let mut out = Vec::with_capacity(n as usize);
    while out.len() < n as usize {
        let a = zrodla[r.gen_range_u32(zrodla.len() as u32) as usize];
        let b = zrodla[r.gen_range_u32(zrodla.len() as u32) as usize];
        if a != b {
            out.push((NodeId(a), NodeId(b)));
        }
    }
    out
}

fn kwantyl(posortowane: &[u64], promile: u64) -> u64 {
    if posortowane.is_empty() {
        return 0;
    }
    let i = ((posortowane.len() as u64 - 1) * promile / 1000) as usize;
    posortowane[i]
}

fn rozklad(etykieta: &str, mut czasy: Vec<u64>, budzet: bool) {
    czasy.sort_unstable();
    let p95 = kwantyl(&czasy, 950);
    println!(
        "{etykieta:<22} mediana {:>7.1} µs · p95 {:>7.1} µs · p99 {:>7.1} µs · max {:>8.1} µs{}",
        mikro(kwantyl(&czasy, 500)),
        mikro(p95),
        mikro(kwantyl(&czasy, 990)),
        mikro(czasy.last().copied().unwrap_or(0)),
        if budzet {
            format!("  wobec 60 µs — {}", werdykt(p95 <= QUERY_P95_BUDGET_NS))
        } else {
            String::new()
        }
    );
}

/// Dwa przebiegi po tych samych parach: pierwszy trafia w pusty cache (czysty koszt
/// CCH plus rozpakowanie trasy), drugi wyłącznie w cache. Bez rozdzielenia tych dwóch
/// liczb średnia mówi tylko o trafialności, a nie o koszcie zapytania.
fn zmierz_zapytania(router: &mut NavRouter, seed: u64, n: u32) {
    let pary = pary(router.graphs().layer(Modality::Road), seed, n);
    if pary.is_empty() {
        println!("brak par do zmierzenia — warstwa drogowa jest pusta");
        return;
    }

    let mut zimne = Vec::with_capacity(pary.len());
    let mut brak = 0u32;
    for (o, d) in &pary {
        let q = RouteQuery::passenger(*o, *d, TransportMode::Car, 8);
        let t = Instant::now();
        let r: Option<Arc<Route>> = router.route(&q);
        zimne.push(t.elapsed().as_nanos() as u64);
        if r.is_none() {
            brak += 1;
        }
    }

    let mut cieple = Vec::with_capacity(pary.len());
    for (o, d) in &pary {
        let q = RouteQuery::passenger(*o, *d, TransportMode::Car, 8);
        let t = Instant::now();
        let r = router.route(&q);
        cieple.push(t.elapsed().as_nanos() as u64);
        std::hint::black_box(&r);
    }

    println!("zapytań {} · bez trasy {brak}", pary.len());
    rozklad("zimne (CCH + trasa)", zimne, true);
    rozklad("cieple (cache)", cieple, false);

    let s = router.stats();
    let c = router.cache_stats();
    println!(
        "RouteStats: trafienia {} · pudła {} · CCH {} · ALT {} · fallback masy {} · bez trasy {}",
        s.cache_hits, s.cache_misses, s.cch_queries, s.alt_queries, s.mass_fallbacks, s.infeasible
    );
    println!(
        "RouteCache: {} wpisów z {} · eksmisje {} · trafialność {} ‰",
        c.len,
        c.capacity,
        c.evictions,
        c.hit_permille()
    );
}

/// `route_optimality` z §7.1 wykonane na żywym mieście: koszt z hierarchii kontrakcji
/// musi być **co do jednostki** kosztem Dijkstry na tych samych wagach. Zgodność
/// poniżej 100 % to błąd, nie statystyka.
fn zweryfikuj(router: &NavRouter, seed: u64, n: u32) -> bool {
    let road = router.graphs().layer(Modality::Road);
    let order = cch::build_order(road);
    let mut ch = cch::contract(road, &order);
    let w = profile_weights(road, RouteProfile::Passenger, None);
    ch.customize(road, &w, road.weight_version);

    let mut scratch = ChScratch::new();
    let pary = pary(road, seed ^ 0x5EED, n);
    let mut zgodne = 0u32;
    let mut czasy = Vec::with_capacity(pary.len());
    let mut cch_czasy = Vec::with_capacity(pary.len());
    for (o, d) in &pary {
        let t = Instant::now();
        let a = ch.query(*o, *d, &mut scratch);
        cch_czasy.push(t.elapsed().as_nanos() as u64);
        let t = Instant::now();
        let b = cch::dijkstra(road, &w, *o, *d).map(|(c, _)| c);
        czasy.push(t.elapsed().as_nanos() as u64);
        if a == b {
            zgodne += 1;
        }
    }
    let ok = zgodne as usize == pary.len();
    println!(
        "weryfikacja CCH ↔ Dijkstra: {zgodne}/{} par zgodnych ({} %) — {}",
        pary.len(),
        if pary.is_empty() {
            0
        } else {
            zgodne as usize * 100 / pary.len()
        },
        werdykt(ok)
    );
    // Sam koszt zapytania w hierarchii, bez rozpakowania trasy — to jest liczba,
    // której dotyczy kryterium WP2 (≤ 60 µs p95) i benchmark criteriona.
    rozklad("goły ChGraph::query", cch_czasy, true);
    czasy.sort_unstable();
    println!(
        "  (Dijkstra odniesienia: mediana {:.1} µs)",
        mikro(kwantyl(&czasy, 500))
    );
    ok
}

/// Kryterium WP2: rekontrakcja kończy się w **tym samym ticku**, który kolejka
/// zapowiedziała z góry. Zapowiedź jest czystą funkcją rozmiaru grafu, więc nie
/// zależy od obciążenia maszyny — i dokładnie to jest tu sprawdzane.
fn przebuduj(router: &mut NavRouter) -> bool {
    let mut q = RebuildQueue::new(WORK_PER_TICK);
    q.request(NavJob::Recontract);

    let start = Tick(1000);
    let mut zapowiedz = None;
    let t = Instant::now();
    for k in 0..10_000u64 {
        let tick = Tick(start.0 + k);
        match q.step(router, tick) {
            RebuildProgress::Idle => break,
            RebuildProgress::Working { .. } => {
                if zapowiedz.is_none() {
                    zapowiedz = q.pending().map(|p| p.finishes_at(WORK_PER_TICK));
                }
            }
            RebuildProgress::Swapped { at, .. } => {
                let zap = zapowiedz.unwrap_or(at);
                let ok = zap == at;
                println!(
                    "rekontrakcja: zapowiedziany tick {} · podmiana w ticku {} · {} ticków, {:.1} ms — {}",
                    zap.0,
                    at.0,
                    k + 1,
                    millis(t.elapsed().as_nanos() as u64),
                    werdykt(ok)
                );
                return ok;
            }
        }
    }
    println!("rekontrakcja: brak podmiany w 10 000 ticków — FAIL");
    false
}

// ── inspektor grafu (kryterium WP1) ──────────────────────────────────────────

fn sterowanie(c: NodeControl) -> String {
    match c {
        NodeControl::Uncontrolled => "bez sterowania".to_string(),
        NodeControl::PrioritySigns { major } => {
            format!("znaki (główne: {} i {})", major[0].0, major[1].0)
        }
        NodeControl::Signal(p) => format!("sygnalizacja (plan {})", p.0),
    }
}

fn warstwy(m: magnat_nav::ModalityMask) -> String {
    let mut s = String::new();
    for x in Modality::ALL {
        if m.contains(*x) {
            if !s.is_empty() {
                s.push('+');
            }
            s.push_str(x.name());
        }
    }
    if s.is_empty() {
        s.push('—');
    }
    s
}

/// Karta węzła: wszystko, co graf o nim wie. To jest artefakt WP1 — jeśli atrybutu
/// nie ma na tej karcie, to znaczy, że nie przeszedł z geometrii M2 do grafu.
fn karta_wezla(g: &RoadGraph, spec: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (xs, ys) = spec
        .split_once(',')
        .ok_or("--inspect oczekuje `x,y` w metrach")?;
    let cel = IVec2::new(
        (xs.trim().parse::<f64>()? * 100.0) as i32,
        (ys.trim().parse::<f64>()? * 100.0) as i32,
    );

    let mut best = None;
    for (i, n) in g.nodes.iter().enumerate() {
        let dx = i64::from(n.pos_cm.x - cel.x);
        let dy = i64::from(n.pos_cm.y - cel.y);
        let d2 = dx * dx + dy * dy;
        if best.is_none_or(|(_, b)| d2 < b) {
            best = Some((i as u32, d2));
        }
    }
    let Some((v, d2)) = best else {
        return Err("warstwa drogowa nie ma węzłów".into());
    };
    let n = NodeId(v);
    let node = &g.nodes[v as usize];

    println!();
    println!("── inspektor grafu: węzeł warstwy `road` ──");
    println!("węzeł          {v}");
    println!(
        "pozycja        {:.2}, {:.2} m  (odległość od punktu {:.2} m)",
        f64::from(node.pos_cm.x) / 100.0,
        f64::from(node.pos_cm.y) / 100.0,
        (d2 as f64).sqrt() / 100.0
    );
    println!("wysokość       {:.2} m", f64::from(node.z_cm) / 100.0);
    println!(
        "sterowanie     {}",
        sterowanie(g.controls.get(v as usize).copied().unwrap_or_default())
    );
    println!(
        "krawędzie      {} wychodzących, {} wchodzących",
        g.out(n).len(),
        g.inc(n).len()
    );
    println!();
    println!(
        "{:>7} {:>7} {:<12} {:>9} {:>5} {:>10} {:>8} {:>8} {:>9} {:>7} {:>6} {:<8} {:>8}",
        "edge",
        "→ węzeł",
        "klasa",
        "długość m",
        "pasy",
        "limit km/h",
        "tonaż t",
        "spadek ‰",
        "dzielnica",
        "postój",
        "most",
        "warstwy",
        "ffree s"
    );
    for &e in g.out(n) {
        let id = magnat_nav::EdgeId(e);
        let ed = g.edge(id);
        println!(
            "{:>7} {:>7} {:<12} {:>9.2} {:>5} {:>10.1} {:>8} {:>8} {:>9} {:>7} {:>6} {:<8} {:>8.2}",
            e,
            ed.to.0,
            ed.class.key(),
            f64::from(ed.length_cm) / 100.0,
            ed.lanes,
            f64::from(ed.speed_limit_dkmh) / 10.0,
            if ed.max_mass.0 > 0 {
                format!("{}", ed.max_mass.0 / 1_000_000)
            } else {
                "—".to_string()
            },
            ed.grade_permille,
            ed.district.0,
            ed.curb_parking,
            ed.bridge.map_or("—".to_string(), |b| b.0.to_string()),
            warstwy(ed.modalities),
            f64::from(ed.free_flow_cs(g.modality)) / 100.0
        );
    }

    // Tabela skrętów: manewry z pierwszej krawędzi wjazdowej. Zakaz zawracania
    // i pierwszeństwo są atrybutem **pary** krawędzi, nie węzła — bez tej tabeli
    // karta nie pokazywałaby, czemu router nie zawraca na skrzyżowaniu.
    match g.inc(n).first() {
        None => println!("brak krawędzi wjazdowych — tabela skrętów pusta"),
        Some(&we) => {
            let ruchy = g.turns_from(magnat_nav::EdgeId(we));
            println!();
            println!(
                "tabela skrętów z krawędzi wjazdowej {we} ({} manewrów):",
                ruchy.len()
            );
            for t in ruchy {
                println!(
                    "  → krawędź {:<7} zakaz: {:<3} pierwszeństwo: {:?} · nasycenie {} poj./h",
                    t.out_edge.0,
                    if t.banned { "tak" } else { "nie" },
                    t.priority,
                    t.saturation_flow_vph
                );
            }
        }
    }

    Ok(())
}
