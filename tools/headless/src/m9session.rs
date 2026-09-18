//! Scenariusz `new-game` — **wynik podfazy M9a**: sesja gry bez GPU.
//!
//! Odpowiada na trzy pytania i tylko na te trzy:
//!
//! 1. **czy da się założyć grę bez ani jednego argumentu opisującego świat** —
//!    komplet parametrów przychodzi z pliku RON (`--from`), tą samą drogą, którą
//!    pójdzie kreator świata z WP14;
//! 2. **czy sesja się odtwarza** — nagrany dziennik wejść puszczony jeszcze raz
//!    daje ten sam łańcuch hashy stanu co 1000 ticków (kryterium WP2);
//! 3. **czy odrzucenie jest odtwarzalne** — komenda z błędnym towarem ma zostać
//!    odrzucona przy odtworzeniu z tym samym `CommandError`, a nie po cichu minąć.
//!
//! Przebieg idzie przez `GameState` tak, jak pójdzie klient graficzny: menu →
//! generacja w tle z postępem → podgląd świata → zaludnienie → gra.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use magnat_core::{Money, StateHash};
use magnat_game::screens::ending::EndAction;
use magnat_game::session::{replay, GameState};
use magnat_game::shell::{NewGameParams, WorldGenJob, WorldPreview};
use magnat_game::{PlayerCommand, SaveSlot, Session, SAVE_SCHEMA_VERSION};
use magnat_jobs::JobPool;

#[derive(Args, Debug)]
pub struct M9SessionArgs {
    /// Plik RON z `NewGameParams`. **Jedyne** wejście opisujące świat.
    #[arg(long)]
    pub from: Option<PathBuf>,

    /// Zamiast nowej gry: odtwórz dziennik z tego pliku.
    #[arg(long)]
    pub replay: Option<PathBuf>,

    #[arg(long, default_value_t = 1)]
    pub days: u16,

    #[arg(long, default_value_t = 1440)]
    pub hash_every: u64,

    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Zapis łańcucha hashy.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Porównanie łańcucha hashy z plikiem.
    #[arg(long)]
    pub expect: Option<PathBuf>,

    /// Zapis dziennika wejść.
    #[arg(long)]
    pub record: Option<PathBuf>,

    /// Katalog slotów zapisu — zapisuje slot 0 po przebiegu.
    #[arg(long)]
    pub slot_dir: Option<PathBuf>,

    /// Cena w groszach, którą gracz ustawia pierwszego dnia w pierwszym sklepie.
    /// 0 = nie ustawiaj.
    #[arg(long, default_value_t = 0)]
    pub price: i64,
}

pub fn run(a: &M9SessionArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pool = JobPool::new(a.threads);

    if let Some(p) = &a.replay {
        return odtworz(a, p, &pool);
    }

    let Some(from) = &a.from else {
        eprintln!("podaj --from <params.ron> albo --replay <dziennik.ron>");
        return Ok(ExitCode::from(2));
    };
    let params: NewGameParams = ron::from_str(&std::fs::read_to_string(from)?)?;

    let session = zaloz(params, &pool)?;
    let ticki = u64::from(a.days) * 1440;

    // Sesja jedzie w `GameState`, a nie obok niego, bo zgon postaci i domknięcie
    // scenariusza przełączają **stan gry** (`DI-33`, `DI-34`) — a przebieg bezgłowy
    // ma przechodzić przez to samo, co okno. Do domknięcia `WP12` trzymał samą sesję
    // i dlatego nie było czym zauważyć, że `legacy::check` nie ma wołającego.
    let mut stan = GameState::Playing(session);

    // Doba pierwsza — bez gracza. Potem gracz wydaje komendy i gra dalej.
    let mut hashe = przebieg(&mut stan, 1440.min(ticki), a.hash_every);
    if let Some(s) = stan.session_mut() {
        komendy_gracza(s, a.price);
    }
    hashe.extend(przebieg(&mut stan, ticki.saturating_sub(1440), a.hash_every));

    let Some(session) = stan.session() else {
        eprintln!("sesja przepadła w trakcie przebiegu");
        return Ok(ExitCode::from(1));
    };
    raport(session);
    if let Some(p) = &a.record {
        session.log().save(p)?;
        eprintln!(
            "dziennik: {} komend, {} odrzuconych → {}",
            session.log().commands.len(),
            session.log().rejected.len(),
            p.display()
        );
    }
    if let Some(dir) = &a.slot_dir {
        let slot = slot_z_sesji(session, 0);
        magnat_game::save::write_slot(dir, &slot, session.log())?;
        let back = magnat_game::save::read_header(dir, 0)?;
        eprintln!(
            "slot 0: {} — {} min gry, nagłówek odczytany bez wczytania świata ({} B)",
            back.city,
            back.game_date.get(),
            std::fs::metadata(magnat_game::save::meta_path(dir, 0))?.len()
        );
    }
    porownaj(a, &hashe)
}

/// Przewija `ticki`, rozliczając po każdej dobie to, co zgłosiła sesja.
///
/// Rozliczenie idzie przez [`GameState::settle`] — tę samą funkcję, którą woła okno.
/// Przebieg bezgłowy odpowiada za gracza w jedyny sposób, który nie zatrzymuje biegu:
/// dziedzic przejmuje firmy, brak dziedzica zaczyna nową dynastię, a domknięty
/// scenariusz gra dalej. Wybór jest wypisany na stderr, więc bramka go widzi.
fn przebieg(stan: &mut GameState, ticki: u64, hash_every: u64) -> Vec<(u64, StateHash)> {
    let mut out = Vec::new();
    let mut zostalo = ticki;
    while zostalo > 0 {
        let krok = zostalo.min(1440);
        let Some(s) = stan.session_mut() else {
            break;
        };
        out.extend(s.run_hashing(krok, hash_every));
        zostalo -= krok;
        if !stan.settle() {
            continue;
        }
        let wybor = match stan {
            GameState::Succession { session, heir } => {
                let doba = session.tick().get() / 1440;
                match heir {
                    Some(h) => {
                        eprintln!("  doba {doba}: postać zmarła, firmy przejmuje dziedzic");
                        Some(EndAction::Succeed(*h))
                    }
                    // Bez dziedzica przebieg zaczyna **nową dynastię**, a nie zostaje
                    // w sukcesji: stan, z którego nikt nie wychodzi, zatrzymałby
                    // rozliczanie scenariusza do końca przebiegu — `settle` zwraca
                    // wtedy `false` przy każdej dobie i bramka mierzy ciszę.
                    None => nowa_dynastia(session).map(|k| {
                        eprintln!("  doba {doba}: postać zmarła bez dziedzica — nowa dynastia");
                        EndAction::NewDynasty(k)
                    }),
                }
            }
            GameState::ScenarioEnd { session, outcome } => {
                let doba = session.tick().get() / 1440;
                eprintln!("  doba {doba}: scenariusz domknięty — {outcome:?}");
                Some(EndAction::KeepPlaying)
            }
            _ => continue,
        };
        match wybor {
            Some(w) => {
                stan.apply_end(w);
            }
            // Świat bez ani jednego kandydata na postać: dalsza gra nie ma komu
            // przypaść. Przebieg kończy się tutaj i mówi o tym wprost, zamiast
            // dowozić ticki, których nikt już nie rozlicza.
            None => {
                eprintln!("  przebieg przerwany: nie ma komu przejąć gry");
                break;
            }
        }
    }
    out
}

/// Pierwszy mieszkaniec spełniający predykat wariantu startu — ta sama droga,
/// którą gracz wybiera postać w oknie.
fn nowa_dynastia(session: &Session) -> Option<magnat_core::CitizenId> {
    let doba = session.tick().get() / 1440;
    magnat_game::player::candidates(&session.app.world, session.variant(), doba)
        .first()
        .map(|k| k.citizen)
}

/// Zakłada grę tą samą drogą, którą pójdzie klient: przez `GameState`.
fn zaloz(
    params: NewGameParams,
    pool: &JobPool,
) -> Result<Box<Session>, Box<dyn std::error::Error>> {
    // Etap A: teren i miasto w wątku w tle, z postępem per pass. Szkic kreatora
    // jest tutaj argumentem, a nie ładunkiem stanu: który ekran powłoki stoi na
    // wierzchu, wie od M9e wyłącznie `Shell::screen`, a przebieg bezgłowy powłoki
    // nie ma.
    let mut stan = GameState::Generating(WorldGenJob::start(params.world, pool.thread_count()));
    let built = match stan {
        GameState::Generating(job) => {
            let postep = job.progress().clone();
            let mut ostatni = 0;
            while !job.is_finished() {
                let d = postep.done();
                if d != ostatni {
                    eprintln!("  [{}/{}] {}", d, postep.total(), postep.pass_name());
                    ostatni = d;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            job.join()?.ok_or("generacja anulowana")?
        }
        _ => unreachable!("stan po starcie generacji"),
    };

    // Etap B: podgląd (pojemność, nie populacja — ludzi jeszcze nie ma), potem zgoda gracza.
    let preview = WorldPreview::of(&built);
    eprintln!(
        "świat gotowy: {} mieszkań, {} miejsc pracy, {} firm, {} dzielnic, {} km² miasta; hash terenu {:#034x}",
        preview.homes,
        preview.jobs,
        preview.firms,
        preview.districts,
        preview.city_area_km2,
        preview.report.terrain_hash.0
    );
    stan = GameState::WorldReady {
        preview: Box::new(preview),
        built: Box::new(built),
    };

    let GameState::WorldReady { built, .. } = stan else {
        unreachable!("stan po podglądzie")
    };
    let session = Session::begin(*built, params, pool)?;
    Ok(Box::new(session))
}

/// Trzy komendy: jedna poprawna, jedna z nieznanym towarem, jedna z ceną zero.
/// Dwie ostatnie mają zostać odrzucone — i tak samo odrzucone przy odtworzeniu.
fn komendy_gracza(session: &mut Session, cena: i64) {
    let Some(market) = session.market.clone() else {
        return;
    };
    let Some(sklep) = market.sites().first().copied() else {
        return;
    };
    let towar = market
        .shelf_snapshot()
        .iter()
        .find(|s| s.site == sklep)
        .and_then(|s| market.good_key(s.good));
    if cena > 0 {
        if let Some(klucz) = towar {
            let przed = market
                .good_of_key(&klucz)
                .and_then(|g| market.price_at(sklep, g));
            let r = session.submit(PlayerCommand::SetPrice {
                site: sklep,
                good: klucz.clone(),
                price: Money(cena),
            });
            eprintln!(
                "gracz: {klucz} w zakładzie {sklep:?} z {} na {} gr — {}",
                przed.map_or(-1, magnat_core::Money::get),
                cena,
                if r.is_ok() { "przyjęte" } else { "odrzucone" }
            );
        }
    }
    let _ = session.submit(PlayerCommand::SetPrice {
        site: sklep,
        good: "nie_ma_takiego_towaru".to_string(),
        price: Money(100),
    });
    let _ = session.submit(PlayerCommand::SetPrice {
        site: sklep,
        good: "good_bread".to_string(),
        price: Money(0),
    });
}

fn odtworz(
    a: &M9SessionArgs,
    p: &std::path::Path,
    pool: &JobPool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let log = magnat_game::ReplayLog::load(p)?;
    let ticki = u64::from(a.days) * 1440;
    let (session, hashe) = replay(&log, ticki, a.hash_every, pool)?;
    eprintln!(
        "odtworzono {} komend, {} odrzuceń zgodnych",
        log.commands.len(),
        log.rejected.len()
    );
    raport(&session);
    porownaj(a, &hashe)
}

fn raport(s: &Session) {
    let r = &s.report;
    eprintln!(
        "sesja: tick {}, {} mieszkańców w {} gospodarstwach, {} sklepów, {} zakładów, {} firm, {} sieci, {} zdarzeń w katalogu",
        s.tick().get(),
        r.citizens,
        r.households,
        r.shops,
        r.plants,
        r.firms,
        r.grids,
        r.events
    );
    eprintln!(
        "harmonogram: {} systemów w {} etapach, odcisk {:#018x}; hash stanu {:#034x}",
        r.systems,
        r.stages,
        r.schedule_fingerprint,
        s.state_hash().0
    );
}

fn slot_z_sesji(s: &Session, id: u8) -> SaveSlot {
    SaveSlot {
        id,
        city: s
            .built
            .city
            .report
            .district_names
            .first()
            .cloned()
            .unwrap_or_else(|| format!("Świat {:#x}", s.built.city.plan.seed)),
        game_date: magnat_core::SimMinute(s.tick().get()),
        // Majątek gracza — WP4 (`M9c`) wnosi postać, więc do tego czasu jest zero.
        net_worth: Money(0),
        played_secs: u32::try_from(s.played_ms() / 1000).unwrap_or(u32::MAX),
        world: s.log().header.params.world,
        schema_version: SAVE_SCHEMA_VERSION,
        saved_at_wall: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    }
}

fn porownaj(
    a: &M9SessionArgs,
    hashe: &[(u64, magnat_core::StateHash)],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let linie: Vec<String> = hashe
        .iter()
        .map(|(t, h)| format!("{t} {:032x}", h.0))
        .collect();
    if let Some(p) = &a.out {
        std::fs::write(p, format!("{}\n", linie.join("\n")))?;
        eprintln!("{} punktów kontrolnych → {}", linie.len(), p.display());
    }
    if let Some(p) = &a.expect {
        let wzorzec: Vec<String> = std::fs::read_to_string(p)?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(ToString::to_string)
            .collect();
        if wzorzec.len() != linie.len() {
            eprintln!(
                "ROZJAZD: {} punktów kontrolnych wobec {} we wzorcu",
                linie.len(),
                wzorzec.len()
            );
            return Ok(ExitCode::FAILURE);
        }
        for (i, (a, b)) in wzorzec.iter().zip(linie.iter()).enumerate() {
            if a != b {
                eprintln!("ROZJAZD w linii {i}: wzorzec {a}, przebieg {b}");
                return Ok(ExitCode::FAILURE);
            }
        }
        eprintln!("{} punktów kontrolnych zgodnych", linie.len());
    }
    Ok(ExitCode::SUCCESS)
}
