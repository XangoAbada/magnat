//! `magnat` — artefakt końcowy fazy M1 (§1): oglądalny krajobraz z 64-bitowego ziarna.
//!
//! ```text
//! magnat --seed 0xC0FFEE --size 8km --region rzeczny --epoch 1990 --profile przemyslowe
//! ```
//!
//! Okno pokazuje teren wygenerowany w całości na starcie (headless-first: ten sam kod,
//! który liczy `magnat-headless generate`), zmaterializowany do voxeli leniwie, wokół kamery.
//! Słońce przesuwa się po niebie; klawisz `T` przyspiesza czas ×1000, `F3` przełącza
//! nakładki debug.

#![forbid(unsafe_code)]

mod app;
mod args;
mod bench;
mod citizens;
mod inspect;
mod overlay;
mod preview;
mod stream;

use crate::app::App;
use crate::args::{parse_hour, parse_seed, Args};
use crate::preview::startowa_kamera;
use clap::Parser;
use magnat_core::SimMinute;
use magnat_voxel::{EditIndex, MaterialRegistry};
use magnat_world::{generate, Difficulty, WorldGenParams};
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args = Args::parse();

    let params = WorldGenParams {
        seed: parse_seed(&args.seed)?,
        size: args.size.parse()?,
        region: args.region.parse()?,
        epoch: args.epoch.parse()?,
        profile: args.profile.parse()?,
        difficulty: Difficulty::Normal,
    };
    params.validate()?;

    let pool = magnat_jobs::JobPool::new(args.threads);
    eprintln!(
        "generacja świata: seed {:#x}, {}, region {} — to potrwa kilka sekund",
        params.seed,
        params.size.key(),
        params.region.key()
    );
    let start = Instant::now();
    let materials = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path(
        "materials",
    ))?);

    // Miasto (M2d, WP12b). Bez tego kroku klient pokazuje krajobraz M1 — i dokładnie
    // tak zachowuje się `--no-city`.
    //
    // Świat stawia **`game/`**, nie klient (M9a): `zbuduj_z_params` jest tą samą
    // funkcją, którą woła przebieg bezgłowy, więc `magnat --seed 7` i
    // `headless new-game --from params.ron` dają ten sam świat co do bitu.
    let (terrain, built, edits, centrum) = if args.no_city {
        let (data, report) = generate(params, &pool)?;
        for line in report.lines() {
            eprintln!("{line}");
        }
        let t = Arc::new(magnat_world::Terrain::new(data, materials.clone()));
        (t, None, Arc::new(EditIndex::default()), None)
    } else {
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
        eprintln!(
            "{} komend voxelowych w {} wpisach indeksu",
            b.city.edits.commands().len(),
            b.city.edits.entries()
        );
        let c = b.city.center;
        let edits = Arc::new(b.city.edits.clone());
        let t = b.terrain.clone();
        (t, Some(b), edits, Some((c.x as i32, c.y as i32)))
    };
    eprintln!("razem {:.1} s", start.elapsed().as_secs_f64());
    let city = built.as_ref().map(|b| b.city.clone());

    // Inspekcja punktu bez otwierania okna — ta sama karta co po kliknięciu prawym.
    if let Some(punkt) = args.inspect.as_deref() {
        let (a, b) = punkt
            .split_once(',')
            .ok_or("--inspect oczekuje `x,y` w metrach")?;
        let (x, y): (i32, i32) = (a.trim().parse()?, b.trim().parse()?);
        print!("{}", inspect::karta(&terrain, x, y));
        if let Some(c) = &city {
            for l in magnat_world::parcel_card(c, magnat_spatial::Vec2::new(x as f32, y as f32)) {
                println!("{l}");
            }
        }
        return Ok(());
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        terrain,
        built,
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
            // Bez jawnego celu kamera staje **nad miastem**, a nie nad środkiem mapy:
            // miasto rzadko leży dokładnie w środku, a widok pustego pola nie jest
            // artefaktem tej podfazy (WP12b pkt 3).
            None => centrum,
        },
        edits,
        city,
        bench: args.bench,
        bench_czas_s: 0.0,
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
        jezyk: match args.locale.as_str() {
            "en" => magnat_ui::Locale::En,
            _ => magnat_ui::Locale::Pl,
        },
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
