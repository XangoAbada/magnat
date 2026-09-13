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

mod overlay;
mod stream;

use clap::Parser;
use magnat_core::SimMinute;
use magnat_render::{CameraMode, GpuContext, Renderer};
use magnat_voxel::MaterialRegistry;
use magnat_world::{generate, Difficulty, TerrainQuery, WorldGenParams};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[derive(Parser, Debug)]
#[command(
    name = "magnat",
    about = "Symulator miasta i gospodarki — podgląd świata (M1)"
)]
struct Args {
    /// Ziarno świata, dziesiętnie albo `0x…`.
    #[arg(long, default_value = "0xC0FFEE")]
    seed: String,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "8km")]
    size: String,

    /// `coastal`/`nadmorski`, `mountain`/`gorski`, `lowland`/`nizinny`,
    /// `river`/`rzeczny`, `desert`/`pustynny`.
    #[arg(long, default_value = "river")]
    region: String,

    #[arg(long, default_value = "1990")]
    epoch: String,

    #[arg(long, default_value = "mixed")]
    profile: String,

    /// Liczba wątków generacji i meshingu; 0 = liczba rdzeni.
    #[arg(long, default_value_t = 0)]
    threads: usize,

    /// Zapisuje jedną klatkę do pliku PNG i kończy. Bez tego renderu nie da się
    /// zweryfikować inaczej niż patrząc na ekran — a §7.4 wymaga raportu.
    #[arg(long)]
    screenshot: Option<std::path::PathBuf>,

    /// Ile klatek odczekać przed zrzutem, żeby strumieniowanie zdążyło wypełnić kadr.
    #[arg(long, default_value_t = 120)]
    screenshot_after: u32,

    /// Godzina dnia na starcie (0–23).
    #[arg(long, default_value_t = 10)]
    hour: u64,

    /// Wysokość orbity kamery w metrach nad celem.
    #[arg(long, default_value_t = 900.0)]
    dist: f32,

    /// Punkt, nad którym stoi kamera: `x,y` w metrach. Bez tego kamera zawsze celuje
    /// w środek mapy, a obejrzenie konkretnego miejsca (jeziora, doliny) wymaga latania
    /// myszą — czego zrzut z CI zrobić nie może.
    #[arg(long)]
    target: Option<String>,

    /// Przelot pomiarowy: kamera przechodzi 2 km wzdłuż mapy przez zadaną liczbę klatek,
    /// po czym program wypisuje raport wydajności (§7.4) i kończy. Bez okna nie da się tego
    /// zmierzyć uczciwie — czas GPU zależy od tego, co naprawdę trafiło na ekran.
    #[arg(long)]
    bench: Option<u32>,

    /// Nakładka debug na starcie: `height`, `flow`, `water`, `biome`, `temp-jan`,
    /// `temp-jul`, `precip`, `geology`, `deposits`. W oknie przełącza je `F3`.
    #[arg(long)]
    overlay: Option<String>,

    /// Scena pomiarowa WP-R4: tyle świateł punktowych rozrzuconych wokół celu kamery.
    /// Kryterium mówi o 4096 — tyle właśnie mieści snapshot M11 (`MAX_LIGHTS`).
    #[arg(long, default_value_t = 0)]
    lights: usize,

    /// Scena pomiarowa WP-R2: wszystko w LOD0 do zadanego promienia w metrach.
    #[arg(long)]
    lod0_radius: Option<i32>,

    /// Dzień roku (0–359) — wpływa na deklinację słońca, czyli na porę roku.
    /// 170 to przesilenie letnie w kalendarzu 360-dniowym (00 §K-1).
    #[arg(long, default_value_t = 170)]
    day: u64,
}

fn parse_seed(s: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let t = s.trim();
    Ok(
        match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            Some(hex) => u64::from_str_radix(&hex.replace('_', ""), 16)?,
            None => t.replace('_', "").parse::<u64>()?,
        },
    )
}

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
    let (data, report) = generate(params, &pool)?;
    for line in report.lines() {
        eprintln!("{line}");
    }
    eprintln!("razem {:.1} s", start.elapsed().as_secs_f64());

    let materials = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path(
        "materials",
    ))?);
    let terrain = Arc::new(magnat_world::Terrain::new(data, materials.clone()));

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        terrain,
        materials,
        params,
        window: None,
        renderer: None,
        streamer: None,
        camera: startowa_kamera(args.dist),
        minute: SimMinute(args.day.min(359) * 1440 + args.hour.min(23) * 60),
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
            None => None,
        },
        bench: args.bench,
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
        ostatnia_mysz: None,
        ostatnia_klatka: Instant::now(),
        klatki: 0,
        fps_okno: Instant::now(),
        fps: 0.0,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Mapa dalekiego terenu: wysokości w decymetrach i kolor powierzchni na siatce roboczej.
///
/// Kolor bierze się z **klasy wody i biomu klimatu**, a nie z `biome_at`: to drugie liczy
/// szum przesunięcia granicy dla każdego punktu, a tutaj punktów jest kilka milionów i nikt
/// ich nie ogląda z bliska. Różnicy nie widać z kilometra, a generacja skraca się z sekund
/// do milisekund.
fn mapa_dalekiego_terenu(terrain: &magnat_world::Terrain) -> (u32, Vec<i16>, Vec<u8>) {
    use magnat_core::Biome;
    use magnat_world::WaterClass;

    let dane = terrain.data();
    let dim = dane.height.dim();
    let cdim = dane.climate.dim();
    let mut wysokosci = Vec::with_capacity(dim * dim);
    let mut kolor = Vec::with_capacity(dim * dim * 4);
    // Komórka klimatu ma 256 m, robocza 4 m — stąd przelicznik między siatkami.
    let na_klimat = (magnat_world::CLIMATE_CELL_M / magnat_world::WORK_CELL_M) as usize;

    for gy in 0..dim {
        for gx in 0..dim {
            let i = gy * dim + gx;
            wysokosci.push(dane.height[i]);
            let biom = match dane.water[i].class() {
                WaterClass::Sea => Biome::Sea,
                WaterClass::Lake => Biome::Lake,
                WaterClass::River => Biome::River,
                WaterClass::Dry => {
                    let c = (gy / na_klimat).min(cdim - 1) * cdim + (gx / na_klimat).min(cdim - 1);
                    dane.climate[c].biome
                }
            };
            let (r, g, b) = barwa_biomu(biom);
            kolor.extend_from_slice(&[r, g, b, 255]);
        }
    }
    (dim as u32, wysokosci, kolor)
}

/// Barwa powierzchni dla dalekiego terenu. Te same wartości co albedo materiałów pokrywy
/// z `data/materials/terrain.ron` — gdyby się rozjechały, granica między terenem voxelowym
/// a panoramą byłaby widoczna jako zmiana koloru w poprzek horyzontu.
fn barwa_biomu(biom: magnat_core::Biome) -> (u8, u8, u8) {
    use magnat_core::Biome;
    match biom {
        Biome::Sea | Biome::Lake | Biome::River => (40, 90, 130),
        Biome::Sand => (196, 178, 132),
        Biome::Rock => (124, 120, 118),
        Biome::Snow => (238, 242, 248),
        Biome::Marsh => (58, 44, 32),
        _ => (86, 124, 62),
    }
}

/// Światła testowe do sceny pomiarowej WP-R4.
///
/// Rozrzucone po siatce z przesunięciem z RNG świata, a nie losowo przy każdym starcie:
/// pomiar przypisania świateł ma być powtarzalny, bo inaczej porównanie dwóch commitów
/// mierzy różnicę w rozkładzie, a nie w kodzie.
fn swiatla_testowe(
    ile: usize,
    terrain: &magnat_world::Terrain,
    cel: glam::DVec3,
) -> Vec<magnat_sim_snapshot::LightRecord> {
    use magnat_core::{rng, StreamId, Tick, NO_ENTITY};
    use magnat_world::TerrainQuery;
    let mut r = rng(0xC0FFEE, StreamId::WorldDetail, NO_ENTITY, Tick(1));
    let bok = (ile as f64).sqrt().ceil() as i32;
    // Rozstaw i zasięg jak przy ulicznych latarniach: co 25 m, świecą na 12 m dookoła.
    let rozstaw = 25.0;
    let mut out = Vec::with_capacity(ile);
    for i in 0..ile as i32 {
        let (gx, gy) = (i % bok, i / bok);
        let x = cel.x + f64::from(gx - bok / 2) * rozstaw + f64::from(r.next_u32() % 8);
        let y = cel.y + f64::from(gy - bok / 2) * rozstaw + f64::from(r.next_u32() % 8);
        let z = f64::from(terrain.height_at(x as i32, y as i32)) * 0.5 + 6.0;
        out.push(magnat_sim_snapshot::LightRecord {
            pos: [x as f32, y as f32, z as f32],
            range: 12.0,
            // Ciepła barwa latarni. Wykładnik 128 + 3 daje mnożnik 8/256, czyli 1/32 —
            // latarnia świeci ułamkiem mocy słońca, a nie tyle samo co ono.
            color_rgbe: ((128 + 3) << 24) | (255 << 16) | (180 << 8) | 90,
        });
    }
    out
}

/// Wysokość oczu w trybie pierwszoosobowym.
const WZROST_OCZU_M: f32 = 1.7;
/// Krok lotu swobodnego na jedno naciśnięcie klawisza.
const KROK_LOTU_M: f64 = 12.0;

/// Dystans przelotu pomiarowego — kryterium V4 mówi wprost o 2 km wzdłuż mapy.
const BENCH_DYSTANS_M: f64 = 2000.0;
/// Klatki rozgrzewkowe przed pomiarem.
const BENCH_ROZGRZEWKA: u32 = 60;

/// Czasy passów GPU do jednej linijki raportu (§7.4).
fn czasy_passow(s: &magnat_render::FrameStats) -> String {
    if !s.gpu_timing {
        return "brak pomiaru (sterownik bez TIMESTAMP_QUERY)".to_string();
    }
    magnat_render::PASS_NAMES
        .iter()
        .zip(s.pass_ms)
        .map(|(n, ms)| format!("{n} {ms:.2} ms"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Kamera startowa: orbita nad środkiem mapy, z wysokości dającej widok dzielnicy.
fn startowa_kamera(dist: f32) -> magnat_render::CameraState {
    magnat_render::CameraState {
        mode: CameraMode::Orbit {
            target: glam::DVec3::ZERO,
            dist,
            yaw: 0.6,
            pitch: 0.55,
        },
        fov_deg: 35.0,
        ..magnat_render::CameraState::default()
    }
}

struct App {
    terrain: Arc<magnat_world::Terrain>,
    materials: Arc<MaterialRegistry>,
    params: WorldGenParams,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    streamer: Option<stream::Streamer>,
    camera: magnat_render::CameraState,
    minute: SimMinute,
    czas_x1000: bool,
    nakladka: overlay::Nakladka,
    cel: Option<(i32, i32)>,
    /// Klatki przelotu pomiarowego i zebrane z nich czasy.
    bench: Option<u32>,
    lod0_radius: Option<i32>,
    swiatla: Vec<magnat_sim_snapshot::LightRecord>,
    bench_ms: Vec<f32>,
    bench_pass_ms: Vec<[f32; magnat_render::PASS_NAMES.len()]>,
    bench_chunks: usize,
    occupancy_opis: String,
    occupancy_max: u32,
    bench_start: glam::DVec3,
    zrzut: Option<std::path::PathBuf>,
    zrzut_po: u32,
    numer_klatki: u32,
    koniec: bool,
    obrot: bool,
    ostatnia_mysz: Option<(f64, f64)>,
    ostatnia_klatka: Instant,
    klatki: u32,
    fps_okno: Instant,
    fps: f64,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Magnat — M1")
            .with_inner_size(PhysicalSize::new(1600, 900));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("nie udało się otworzyć okna"),
        );
        let gpu = GpuContext::new(window.clone());
        eprintln!(
            "GPU: {} — rysowanie {}",
            gpu.adapter_name(),
            if gpu.multi_draw_indirect {
                "pośrednie (multi-draw)"
            } else {
                "per chunk (ścieżka zapasowa)"
            }
        );
        let mut gpu = gpu;
        if self.bench.is_some() && !gpu.disable_vsync() {
            eprintln!("uwaga: sterownik nie daje trybu bez vsync — FPS będzie obcięty do odświeżania monitora");
        }
        let renderer = Renderer::new(gpu, &self.materials);

        // Kamera startuje nad środkiem mapy, na wysokości terenu.
        let (tx, ty) = self.cel.unwrap_or_else(|| {
            let s = self.terrain.size_m() / 2;
            (s, s)
        });
        let h = f64::from(self.terrain.height_at(tx, ty)) * 0.5;
        if let CameraMode::Orbit { target, .. } = &mut self.camera.mode {
            *target = glam::DVec3::new(f64::from(tx), f64::from(ty), h);
        }

        if !self.swiatla.is_empty() {
            let cel = self.camera.target();
            self.swiatla = swiatla_testowe(self.swiatla.len(), &self.terrain, cel);
        }
        // Daleki teren: mapa wysokości i zapieczony kolor powierzchni, raz na świat.
        let (dim, wysokosci, kolor) = mapa_dalekiego_terenu(&self.terrain);
        let mut renderer = renderer;
        renderer.upload_far_terrain(dim, magnat_world::WORK_CELL_M as f32, &wysokosci, &kolor);

        // Nakładka wybrana z wiersza poleceń musi trafić do renderera zaraz po jego
        // powstaniu — `przelacz_nakladke` przesunęłoby ją o jedną pozycję.
        if self.nakladka != overlay::Nakladka::Brak {
            if let Some(p) = overlay::zbuduj(&self.terrain, self.nakladka) {
                renderer.set_overlay(Some(magnat_render::TerrainOverlay {
                    cell_m: p.cell_m,
                    dim: p.dim,
                    values: &p.values,
                    palette: &p.palette,
                    strength: 0.75,
                }));
                eprintln!("nakładka: {}", self.nakladka.nazwa());
            }
        }

        let mut streamer = stream::Streamer::new(self.terrain.clone(), self.materials.clone());
        if let Some(r) = self.lod0_radius {
            streamer.wymus_lod0(r);
        }
        self.streamer = Some(streamer);
        self.renderer = Some(renderer);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    self.obrot = state == ElementState::Pressed;
                    if !self.obrot {
                        self.ostatnia_mysz = None;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = (position.x, position.y);
                if self.obrot {
                    if let Some(prev) = self.ostatnia_mysz {
                        let (dx, dy) = ((p.0 - prev.0) as f32, (p.1 - prev.1) as f32);
                        self.camera.orbit_rotate(-dx * 0.005, dy * 0.005);
                    }
                }
                self.ostatnia_mysz = Some(p);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let kroki = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                };
                self.camera.orbit_zoom(0.9f32.powf(kroki));
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Character("t") | Key::Character("T") => {
                        self.czas_x1000 = !self.czas_x1000;
                    }
                    // Tryby kamery (§15.2). Przejścia zachowują pozycję oka i kierunek —
                    // to `CameraState` gwarantuje, tutaj zostaje tylko wysokość gruntu.
                    // F3 przechodzi po nakładkach debug z §1 — wszystkie przez jeden
                    // mechanizm `TerrainOverlay`, ten sam, którego użyje M2.
                    Key::Named(NamedKey::F3) => self.przelacz_nakladke(),
                    Key::Character("1") => self.camera.to_orbit(600.0),
                    Key::Character("2") => self.camera.to_free(),
                    Key::Character("3") => {
                        let oko = self.camera.eye();
                        let grunt =
                            f64::from(self.terrain.height_at(oko.x as i32, oko.y as i32)) * 0.5;
                        self.camera.to_first_person(grunt, WZROST_OCZU_M);
                    }
                    Key::Character(c) if matches!(c, "w" | "s" | "a" | "d" | "q" | "e") => {
                        self.lec(c);
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.klatka();
                if self.koniec {
                    event_loop.exit();
                    return;
                }
            }
            _ => {}
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl App {
    /// Raport z przelotu: rozkład czasu klatki, zacięcia, szczyt czasu GPU i pamięci.
    ///
    /// Percentyle, nie sama średnia: 60 FPS średnio przy jednym zacięciu 200 ms to gorsze
    /// wrażenie niż równe 50 FPS, a średnia obu nie odróżnia.
    fn raport_bench(&mut self, s: magnat_render::FrameStats) {
        self.bench_ms.sort_by(f32::total_cmp);
        let n = self.bench_ms.len().max(1);
        let suma: f32 = self.bench_ms.iter().sum();
        let p = |q: f32| self.bench_ms[((n - 1) as f32 * q) as usize];
        let zaciecia = self.bench_ms.iter().filter(|ms| **ms > 33.0).count();
        eprintln!(
            "przelot {BENCH_DYSTANS_M:.0} m, {n} klatek: średnio {:.1} FPS (mediana {:.1} ms, p95 {:.1} ms, maks. {:.1} ms), zacięć > 33 ms: {zaciecia}",
            1000.0 * n as f32 / suma,
            p(0.5),
            p(0.95),
            p(1.0),
        );
        let gpu = magnat_render::PASS_NAMES
            .iter()
            .enumerate()
            .map(|(i, nazwa)| {
                let mut v: Vec<f32> = self.bench_pass_ms.iter().map(|p| p[i]).collect();
                v.sort_by(f32::total_cmp);
                if v.is_empty() {
                    return format!("{nazwa} brak pomiaru");
                }
                format!(
                    "{nazwa} {:.2} ms (p95 {:.2})",
                    v[v.len() / 2],
                    v[(v.len() - 1) * 95 / 100]
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        if !self.swiatla.is_empty() {
            eprintln!("światła: {} · {}", self.swiatla.len(), self.occupancy_opis);
        }
        eprintln!(
            "szczyt: {} chunków rysowanych, GPU {gpu}; arena {:.1} MB z {:.1} MB",
            self.bench_chunks,
            (s.vertex_bytes + s.index_bytes) as f64 / (1024.0 * 1024.0),
            s.arena_capacity_bytes as f64 / (1024.0 * 1024.0),
        );
    }

    /// Przesuwa kamerę w locie swobodnym i z pierwszej osoby. W orbicie nic nie robi —
    /// tam od przesuwania jest przeciąganie myszą.
    /// Przełącza nakładkę i przelicza jej pole. Przeliczenie jest jednorazowe (dziesiątki
    /// milisekund na mapie 8 km), bo pole opisuje świat, a ten w M1 się nie zmienia.
    fn przelacz_nakladke(&mut self) {
        self.nakladka = self.nakladka.nastepna();
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        match overlay::zbuduj(&self.terrain, self.nakladka) {
            Some(p) => {
                renderer.set_overlay(Some(magnat_render::TerrainOverlay {
                    cell_m: p.cell_m,
                    dim: p.dim,
                    values: &p.values,
                    palette: &p.palette,
                    strength: 0.75,
                }));
            }
            None => renderer.set_overlay(None),
        }
        eprintln!("nakładka: {}", self.nakladka.nazwa());
    }

    fn lec(&mut self, klawisz: &str) {
        let przod = self.camera.forward();
        let przod = glam::DVec3::new(f64::from(przod.x), f64::from(przod.y), f64::from(przod.z));
        let prawo = przod.cross(glam::DVec3::Z).normalize_or_zero();
        let d = match klawisz {
            "w" => przod,
            "s" => -przod,
            "a" => -prawo,
            "d" => prawo,
            "q" => -glam::DVec3::Z,
            _ => glam::DVec3::Z,
        } * KROK_LOTU_M;
        match &mut self.camera.mode {
            CameraMode::Free { pos, .. } => *pos += d,
            // Z pierwszej osoby chodzi się po gruncie, więc pion zostaje pionowi terenu.
            CameraMode::FirstPerson { pos, .. } => {
                pos.x += d.x;
                pos.y += d.y;
            }
            CameraMode::Orbit { .. } => {}
        }
    }

    fn klatka(&mut self) {
        let teraz = Instant::now();
        let dt = teraz.duration_since(self.ostatnia_klatka).as_secs_f64();
        self.ostatnia_klatka = teraz;

        // Czas gry: 1 minuta na sekundę realną, albo ×1000 po wciśnięciu `T`
        // (doba w 30 s realnych — wprost z kryterium §7 dla cyklu dobowego).
        let mnoznik = if self.czas_x1000 { 1000.0 } else { 1.0 };
        self.minute = SimMinute(self.minute.0 + (dt * mnoznik) as u64);

        // Kamera nie wchodzi pod teren.
        let punkt = match self.camera.mode {
            CameraMode::Orbit { target, .. } => target,
            _ => self.camera.eye(),
        };
        let h = f64::from(self.terrain.height_at(punkt.x as i32, punkt.y as i32)) * 0.5;
        self.camera.clamp_above_terrain(h, 2.0);

        // Przelot pomiarowy: cel sunie po prostej, żeby trasa była **ta sama** przy każdym
        // uruchomieniu — inaczej porównywanie wyników między commitami nie ma sensu.
        if let Some(klatek) = self.bench {
            if let CameraMode::Orbit { target, .. } = &mut self.camera.mode {
                if self.bench_start == glam::DVec3::ZERO {
                    self.bench_start = *target;
                }
                let krok = BENCH_DYSTANS_M / f64::from(klatek.max(1));
                target.x = self.bench_start.x + krok * f64::from(self.numer_klatki);
                target.z =
                    f64::from(self.terrain.height_at(target.x as i32, target.y as i32)) * 0.5;
            }
        }

        let (Some(renderer), Some(streamer)) = (&mut self.renderer, &mut self.streamer) else {
            return;
        };
        streamer.update(&self.camera, renderer);
        if !self.swiatla.is_empty() {
            renderer.set_lights(&self.swiatla, self.camera.eye());
        }

        self.numer_klatki += 1;
        if let Some(sciezka) = self.zrzut.clone() {
            if self.numer_klatki >= self.zrzut_po {
                let (w, h, px) = renderer.render_to_image(
                    &self.camera,
                    self.minute,
                    self.params.region.latitude_ddeg(),
                    1600,
                    900,
                );
                let s = renderer.stats();
                eprintln!(
                    "stan renderu: {} chunków rezydentnych ({}), {} rysowanych, {} trójkątów,                      {:.1} MB geometrii; GPU {}; kamera {:?} cel {:?}",
                    s.chunks_resident,
                    streamer.opis(),
                    s.chunks_drawn,
                    s.triangles,
                    (s.vertex_bytes + s.index_bytes) as f64 / (1024.0 * 1024.0),
                    czasy_passow(&s),
                    self.camera.eye(),
                    match self.camera.mode {
                        CameraMode::Orbit { target, .. } => target,
                        _ => glam::DVec3::ZERO,
                    }
                );
                if !self.swiatla.is_empty() {
                    eprintln!(
                        "światła: {} · {}",
                        self.swiatla.len(),
                        renderer.cluster_occupancy().summary()
                    );
                }
                match magnat_devtools::write_rgb(&sciezka, w, h, &px) {
                    Ok(b) => eprintln!("zrzut: {} ({w}×{h}, {b} B)", sciezka.display()),
                    Err(e) => eprintln!("zrzut nieudany: {e}"),
                }
                self.zrzut = None;
                self.koniec = true;
                return;
            }
        }
        if let Some(klatek) = self.bench {
            // Pierwsze klatki to rozgrzewka strumieniowania — mierzenie ich mówiłoby
            // o czasie materializacji, a nie o rysowaniu.
            if self.numer_klatki > BENCH_ROZGRZEWKA {
                let s = renderer.stats();
                self.bench_ms.push((dt * 1000.0) as f32);
                // Czas GPU wychodzi co druga klatka (bufor odczytu jest wtedy zajęty),
                // więc zera to brak pomiaru, a nie pass bez pracy.
                if s.pass_ms.iter().any(|ms| *ms > 0.0) {
                    self.bench_pass_ms.push(s.pass_ms);
                }
                self.bench_chunks = self.bench_chunks.max(s.chunks_drawn);
                // Zapamiętujemy klatkę o **największym** obciążeniu klastra, a nie ostatnią:
                // przelot wychodzi poza obszar świateł, więc ostatnia klatka pokazywałaby zera.
                let o = renderer.cluster_occupancy();
                if o.max() >= self.occupancy_max {
                    self.occupancy_max = o.max();
                    self.occupancy_opis = o.summary();
                }
            }
            if self.numer_klatki >= klatek + BENCH_ROZGRZEWKA {
                let stats = renderer.stats();
                self.raport_bench(stats);
                self.koniec = true;
                return;
            }
        }
        renderer.render(
            &self.camera,
            self.minute,
            self.params.region.latitude_ddeg(),
        );

        self.klatki += 1;
        if self.fps_okno.elapsed().as_secs_f64() >= 0.5 {
            self.fps = f64::from(self.klatki) / self.fps_okno.elapsed().as_secs_f64();
            self.klatki = 0;
            self.fps_okno = Instant::now();
            if let Some(w) = &self.window {
                let s = renderer.stats();
                let cal = magnat_core::SimCalendar::from_minute(self.minute);
                w.set_title(&format!(
                    "Magnat — {:.0} FPS · {} chunków [{}] ({} rysowanych, {} tys. trójkątów) · \
                     {:02}:{:02} dzień {} · {:.0} MB geometrii",
                    self.fps,
                    s.chunks_resident,
                    streamer.opis(),
                    s.chunks_drawn,
                    s.triangles / 1000,
                    cal.hour_of_day(),
                    cal.minute_of_hour(),
                    cal.day_of_year(),
                    (s.vertex_bytes + s.index_bytes) as f64 / (1024.0 * 1024.0),
                ));
            }
        }
    }
}
