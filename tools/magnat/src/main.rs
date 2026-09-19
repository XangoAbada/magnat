//! `magnat` — gra.
//!
//! ```text
//! magnat                                  # menu główne, kreator świata, podgląd
//! magnat --seed 0xC0FFEE --size 8km       # świat wprost z wiersza poleceń
//! ```
//!
//! **Bez ani jednego argumentu gra prowadzi od menu głównego do grającego świata**
//! (M9b/WP14, PRD §14.7). Wiersz poleceń zostaje drogą dla nas, nie dla gracza:
//! podany parametr świata omija powłokę i stawia miasto od razu, tak jak do M9a.
//! Tej drogi używa każdy zrzut, przelot pomiarowy i test bufora identyfikatorów.

#![forbid(unsafe_code)]

mod app;
mod args;
mod bench;
mod citizens;
mod dock;
mod inspect;
mod interiors;
mod overlay;
mod preview;
mod session;
mod signs;
mod slots;
mod stream;

use crate::app::App;
use crate::args::{parse_hour, Args};
use crate::preview::startowa_kamera;
use clap::Parser;
use magnat_core::SimMinute;
use magnat_game::shell::Settings;
use magnat_game::{GameState, Shell, WorldPreview};
use magnat_voxel::{EditIndex, MaterialRegistry};
use magnat_world::generate;
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args = Args::parse();
    let pool = magnat_jobs::JobPool::new(args.threads);
    let materials = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path(
        "materials",
    ))?);

    // Profil gracza siedzi obok zapisów i przeżywa uruchomienie: język wybrany
    // w ustawieniach ma zostać wybrany także następnym razem.
    let zapisy = katalog_zapisow();
    let mut settings = Settings::load(&zapisy.join("settings.ron")).unwrap_or_default();
    if let Some(l) = args.locale.as_deref() {
        settings.locale = l.parse()?;
    }
    let mut shell = Shell::new(settings)?;
    shell.refresh_slots(&zapisy);
    // `--observe` i pozycja „Tryb przeglądu" w menu głównym ustawiają **tę samą**
    // flagę: jedna droga do trybu bez postaci, a nie dwie obok siebie.
    shell.observe = args.observe;

    // Świat z wiersza poleceń: generujemy go **przed** oknem, jak do M9a. Powłoka
    // dostaje wtedy tylko motyw i ustawienia, a gra startuje od razu w mieście.
    let (stan, params, centrum, edits) = match args.world_params()? {
        Some(params) => {
            params.validate()?;
            shell.draft.world = params;
            eprintln!(
                "generacja świata: seed {:#x}, {}, region {} — to potrwa kilka sekund",
                params.seed,
                params.size.key(),
                params.region.key()
            );
            let start = Instant::now();
            if args.no_city {
                let (data, report) = generate(params, &pool)?;
                for line in report.lines() {
                    eprintln!("{line}");
                }
                eprintln!("razem {:.1} s", start.elapsed().as_secs_f64());
                // Bez miasta nie ma czego stawiać w sesji: klient pokazuje sam
                // krajobraz M1, tak jak przed M2.
                let terrain = Arc::new(magnat_world::Terrain::new(data, materials.clone()));
                return uruchom(
                    args,
                    materials,
                    shell,
                    zapisy,
                    params,
                    Some(terrain),
                    None,
                    Arc::new(EditIndex::default()),
                    None,
                );
            }
            let b = magnat_game::world::population::zbuduj_z_params(
                params,
                &pool,
                &magnat_game::GenWatch::none(),
            )?
            .ok_or("generacja anulowana")?;
            for line in b.report.lines() {
                eprintln!("{line}");
            }
            for line in b.city.report.lines() {
                eprintln!("{line}");
            }
            eprintln!("razem {:.1} s", start.elapsed().as_secs_f64());
            let c = b.city.center;
            let edits = Arc::new(b.city.edits.clone());
            let terrain = b.terrain.clone();
            let city = Some(b.city.clone());
            let stan = GameState::WorldReady {
                preview: Box::new(WorldPreview::of(&b)),
                built: Box::new(b),
            };
            // Inspekcja punktu bez otwierania okna — ta sama karta co po kliknięciu.
            if let Some(punkt) = args.inspect.as_deref() {
                let (a, bb) = punkt
                    .split_once(',')
                    .ok_or("--inspect oczekuje `x,y` w metrach")?;
                let (x, y): (i32, i32) = (a.trim().parse()?, bb.trim().parse()?);
                print!("{}", inspect::karta(&terrain, x, y));
                if let Some(c) = &city {
                    for l in
                        magnat_world::parcel_card(c, magnat_spatial::Vec2::new(x as f32, y as f32))
                    {
                        println!("{l}");
                    }
                }
                return Ok(());
            }
            (stan, params, Some((c.x as i32, c.y as i32)), edits)
        }
        // Bez argumentów świata: menu główne. Teren powstanie dopiero po kreatorze.
        None => (
            GameState::Shell,
            magnat_world::WorldGenParams::default(),
            None,
            Arc::new(EditIndex::default()),
        ),
    };

    let (terrain, city) = match &stan {
        GameState::WorldReady { built, .. } => {
            (Some(built.terrain.clone()), Some(built.city.clone()))
        }
        _ => (None, None),
    };
    uruchom_ze_stanem(
        args, materials, shell, zapisy, params, terrain, city, edits, centrum, stan,
    )
}

/// Katalog zapisów i profilu gracza. Obok pliku wykonywalnego, a nie w katalogu
/// systemowym: gra jest na razie uruchamiana z repozytorium i zapisy mają być tam,
/// gdzie ich szuka ten, kto zgłasza błąd. Docelowe miejsce ustala M12.
fn katalog_zapisow() -> std::path::PathBuf {
    std::path::PathBuf::from("saves")
}

#[allow(clippy::too_many_arguments)]
fn uruchom(
    args: Args,
    materials: Arc<MaterialRegistry>,
    shell: Shell,
    zapisy: std::path::PathBuf,
    params: magnat_world::WorldGenParams,
    terrain: Option<Arc<magnat_world::Terrain>>,
    city: Option<Arc<magnat_world::CityData>>,
    edits: Arc<EditIndex>,
    centrum: Option<(i32, i32)>,
) -> Result<(), Box<dyn std::error::Error>> {
    uruchom_ze_stanem(
        args,
        materials,
        shell,
        zapisy,
        params,
        terrain,
        city,
        edits,
        centrum,
        GameState::Shell,
    )
}

#[allow(clippy::too_many_arguments)]
fn uruchom_ze_stanem(
    args: Args,
    materials: Arc<MaterialRegistry>,
    shell: Shell,
    zapisy: std::path::PathBuf,
    params: magnat_world::WorldGenParams,
    terrain: Option<Arc<magnat_world::Terrain>>,
    city: Option<Arc<magnat_world::CityData>>,
    edits: Arc<EditIndex>,
    centrum: Option<(i32, i32)>,
    stan: GameState,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        redaktor: None,
        filler: magnat_game::SnapshotFiller::new(),
        snapshot: magnat_sim_snapshot::SnapshotPair::default(),
        palettes: None,
        terrain,
        materials,
        params,
        window: None,
        renderer: None,
        streamer: None,
        camera: startowa_kamera(args.dist),
        minute: SimMinute(args.day.min(359) * 1440 + u64::from(parse_hour(&args.hour)?)),
        czas_x1000: false,
        nakladka: match args.overlay.as_deref() {
            Some(k) => overlay::Nakladka::z_klucza(k).ok_or("nieznana nakładka")?,
            None => overlay::Nakladka::Brak,
        },
        cel: match args.target.as_deref() {
            Some(t) => {
                let (a, b) = t
                    .split_once(',')
                    .ok_or("--target oczekuje `x,y` w metrach")?;
                Some((a.trim().parse()?, b.trim().parse()?))
            }
            // Bez jawnego celu kamera staje **nad miastem**, a nie nad środkiem mapy.
            None => centrum,
        },
        edits,
        city,
        game: stan,
        shell,
        pauza_menu: false,
        podglad_tex: None,
        zapisy,
        egui_ctx: None,
        egui_state: None,
        bench: args.bench,
        bench_czas_s: 0.0,
        anim_ms: 0,
        anim_reszta: 0.0,
        ciecie: args.cut,
        przekroj: magnat_render::CutPlane::off(),
        szyldy: magnat_render::SignAtlas::new(),
        szyldy_seed: None,
        szyldy_kadr: Vec::new(),
        wnetrza: interiors::Wnetrza::default(),
        tlum: args.crowd,
        krok_tlumu: args.crowd_step,
        bez_animacji: args.no_anim,
        klipy: magnat_voxel::ClipLibrary::builtin(),
        bench_etapy: Vec::new(),
        lod0_radius: args.lod0_radius,
        // Wypełniane po ustawieniu kamery — pozycje zależą od celu, którego tu jeszcze nie ma.
        swiatla: vec![magnat_sim_snapshot::LightRecord::default(); args.lights],
        bench_ms: Vec::new(),
        bench_pass_ms: Vec::new(),
        bench_chunks: 0,
        occupancy_opis: String::new(),
        occupancy_max: 0,
        bench_start: glam::DVec3::ZERO,
        zrzut: args.screenshot.clone(),
        zrzut_po: args.screenshot_after,
        numer_klatki: 0,
        koniec: false,
        obrot: false,
        przesuw: false,
        przeciagniecie_px: 0.0,
        ostatnia_mysz: None,
        ostatnia_klatka: Instant::now(),
        klatki: 0,
        fps_okno: Instant::now(),
        fps: 0.0,
        citizens: None,
        bez_ludzi: args.no_city || args.no_citizens,
        bez_gospodarki: args.no_economy,
        watki: args.threads,
        kursor: match args.pick.as_deref() {
            Some(t) => {
                let (a, b) = t
                    .split_once(',')
                    .ok_or("--pick oczekuje `x,y` w pikselach")?;
                Some((a.trim().parse()?, b.trim().parse()?))
            }
            None => None,
        },
        tryb_pick: args.pick.is_some(),
        godzina_startu: parse_hour(&args.hour)?,
        predkosc: match args.speed {
            0 => magnat_core::SimSpeed::Paused,
            3 => magnat_core::SimSpeed::X3,
            10 => magnat_core::SimSpeed::X10,
            _ => magnat_core::SimSpeed::X1,
        },
        dzien_slonca: args.day.min(359),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Wysokość oczu w trybie pierwszoosobowym.
pub(crate) const WZROST_OCZU_M: f32 = 1.7;
/// Krok lotu swobodnego na jedno naciśnięcie klawisza.
pub(crate) const KROK_LOTU_M: f64 = 12.0;
/// Do tylu pikseli przeciągnięcia puszczenie prawego przycisku jest jeszcze kliknięciem.
pub(crate) const PROG_KLIKNIECIA_PX: f64 = 4.0;
/// Promień, w którym kliknięcie w teren trafia w sklep (M5e/WP12).
///
/// `ponytail:` promień zamiast bufora identyfikatorów budynków. Sufit nazwany:
/// dwa sklepy bliżej siebie niż 25 m są nierozróżnialne kliknięciem, wygrywa
/// bliższy. Ścieżka wyjścia: `engine/render` dokłada `SiteId` do bufora ID i to
/// wywołanie zamienia się w odczyt, bez zmian po stronie panelu.
pub(crate) const PROMIEN_SKLEPU_M: f32 = 25.0;

/// Klatki rozgrzewkowe przed pomiarem.
pub(crate) const BENCH_ROZGRZEWKA: u32 = 60;

