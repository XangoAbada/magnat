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

mod bench;
mod citizens;
mod inspect;
mod overlay;
mod stream;

use clap::Parser;
use magnat_core::SimMinute;
use magnat_render::{CameraMode, GpuContext, Renderer};
use magnat_voxel::{EditIndex, MaterialRegistry};
use magnat_world::{generate, generate_city, CityPlan, Difficulty, TerrainQuery, WorldGenParams};
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

    /// Godzina dnia na starcie: `10` albo `8:15`.
    ///
    /// Minuty są tu potrzebne, a nie ozdobne: szczyt poranny w mieście z medianą dojazdu
    /// 18 minut trwa kilkanaście minut, a o pełnej ósmej ulice są jeszcze albo już puste.
    #[arg(long, default_value = "10")]
    hour: String,

    /// Wysokość orbity kamery w metrach nad celem.
    #[arg(long, default_value_t = 900.0)]
    dist: f32,

    /// Punkt, nad którym stoi kamera: `x,y` w metrach. Bez tego kamera zawsze celuje
    /// w środek mapy, a obejrzenie konkretnego miejsca (jeziora, doliny) wymaga latania
    /// myszą — czego zrzut z CI zrobić nie może.
    #[arg(long)]
    target: Option<String>,

    /// Scena pomiarowa z §7.4: ustalony przelot przez pięć etapów (orbita miasta →
    /// dzielnica → poziom ulicy → przelot 2 km → orbita) przez zadaną liczbę sekund,
    /// a na końcu raport wydajności. Bez okna nie da się tego zmierzyć uczciwie —
    /// czas GPU zależy od tego, co naprawdę trafiło na ekran.
    #[arg(long)]
    bench: Option<f32>,

    /// Wypisuje kartę inspekcji punktu `x,y` zaraz po generacji i kończy — ta sama karta,
    /// którą w oknie pokazuje klik prawym przyciskiem. Istnieje, bo kontrakt `TerrainQuery`
    /// da się wtedy sprawdzić bez GPU i bez rąk (§7.5).
    #[arg(long)]
    inspect: Option<String>,

    /// Nakładka debug na starcie: `height`, `flow`, `water`, `biome`, `temp-jan`,
    /// `temp-jul`, `precip`, `geology`, `deposits`, `land-value`. W oknie przełącza je `F3`.
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

    /// Pomija generację miasta i pokazuje czysty teren M1. Istnieje po to, żeby
    /// regresję terenu dało się zdiagnozować bez zabudowy zasłaniającej widok (WP12b).
    #[arg(long, default_value_t = false)]
    no_city: bool,

    /// Pomija Etap 8 i pokazuje puste miasto (M3d). Istnieje z tego samego powodu co
    /// `--no-city`: zaludnienie metropolii to kilkadziesiąt sekund, a regresji renderu
    /// nie diagnozuje się, czekając na nią.
    #[arg(long, default_value_t = false)]
    no_citizens: bool,

    /// Sprawdza bufor ID bez rąk: ustawia kursor na piksel `x,y`, przewija kilka klatek
    /// i wypisuje, w kogo trafiono (kryterium WP11: „kliknięcie w pieszego daje
    /// `CitizenId`"). Bez tego jedynym sposobem sprawdzenia selekcji jest mysz.
    #[arg(long)]
    pick: Option<String>,

    /// Prędkość gry na starcie: `0` (pauza), `1`, `3`, `10`. Ta sama czwórka co
    /// w widgecie sterowania czasem — i ta sama, na której stoi wymóg „prędkość nie
    /// wpływa na wynik" (§5.11, PRD §14.5).
    #[arg(long, default_value_t = 1)]
    speed: u32,

    /// Język interfejsu: `pl` albo `en`. Tekst w UI zawsze pochodzi z `data/locale/`
    /// w obu wersjach (CLAUDE.md), więc przełącznik nie ma prawa czegokolwiek zgubić.
    #[arg(long, default_value = "pl")]
    locale: String,
}

/// `HH` albo `HH:MM` na minutę doby.
fn parse_hour(s: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let (h, m) = match s.split_once(':') {
        Some((h, m)) => (h.trim().parse::<u32>()?, m.trim().parse::<u32>()?),
        None => (s.trim().parse::<u32>()?, 0),
    };
    if h > 23 || m > 59 {
        return Err(format!("--hour {s}: poza dobą").into());
    }
    Ok(h * 60 + m)
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

    // Miasto (M2d, WP12b). Bez tego kroku klient pokazuje krajobraz M1 — i dokładnie
    // tak zachowuje się `--no-city`.
    let (city, edits, centrum) = if args.no_city {
        (None, Arc::new(EditIndex::default()), None)
    } else {
        let plan = CityPlan::from_world(&params);
        let start = Instant::now();
        let city = generate_city(&plan, terrain.as_ref(), &materials, &pool)?;
        for line in city.report.lines() {
            eprintln!("{line}");
        }
        eprintln!(
            "miasto {:.2} s · {} komend voxelowych w {} wpisach indeksu",
            start.elapsed().as_secs_f64(),
            city.edits.commands().len(),
            city.edits.entries()
        );
        let c = city.center;
        let edits = Arc::new(city.edits.clone());
        (Some(Arc::new(city)), edits, Some((c.x as i32, c.y as i32)))
    };

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
        jezyk: match args.locale.as_str() {
            "en" => magnat_ui::Locale::En,
            _ => magnat_ui::Locale::Pl,
        },
        watki: args.threads,
        kursor: match args.pick.as_deref() {
            Some(t) => {
                let (a, b) = t.split_once(',').ok_or("--pick oczekuje `x,y` w pikselach")?;
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
/// Do tylu pikseli przeciągnięcia puszczenie prawego przycisku jest jeszcze kliknięciem.
const PROG_KLIKNIECIA_PX: f64 = 4.0;

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
    edits: Arc<EditIndex>,
    /// Miasto (M2e, WP16) — nakładka wartości gruntu i karta inspekcji parceli czytają
    /// je bezpośrednio; bez `--no-city` jest zawsze.
    city: Option<Arc<magnat_world::CityData>>,
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
    bench: Option<f32>,
    /// Czas od startu pomiaru i wyniki kolejnych etapów.
    bench_czas_s: f32,
    bench_etapy: Vec<bench::Pomiar>,
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
    /// Przeciąganie prawym przyciskiem i suma jego drogi w pikselach — poniżej
    /// `PROG_KLIKNIECIA_PX` puszczenie przycisku liczy się jako kliknięcie, nie przesuw.
    przesuw: bool,
    przeciagniecie_px: f64,
    ostatnia_mysz: Option<(f64, f64)>,
    ostatnia_klatka: Instant,
    klatki: u32,
    fps_okno: Instant,
    fps: f64,
    /// Mieszkańcy, doba i panel (M3d, WP11/WP12). `None` przy `--no-city`, przy
    /// `--no-citizens` i dopóki okno nie powstało — Etap 8 potrzebuje miasta,
    /// a `egui` potrzebuje okna.
    citizens: Option<citizens::Citizens>,
    bez_ludzi: bool,
    jezyk: magnat_ui::Locale,
    watki: usize,
    /// Ostatnia znana pozycja kursora w pikselach — bufor ID kopiuje piksel spod niej.
    kursor: Option<(u32, u32)>,
    tryb_pick: bool,
    /// Minuta doby, do której przewijamy symulację przed pierwszą klatką.
    godzina_startu: u32,
    predkosc: magnat_core::SimSpeed,
    /// Dzień roku dla **słońca**. Symulacja liczy własną dobę od zera; `--day` ustawia
    /// porę roku, czyli deklinację, i tylko ją.
    dzien_slonca: u64,
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
            if let Some(p) = overlay::zbuduj(&self.terrain, self.city.as_deref(), self.nakladka) {
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

        let mut streamer = stream::Streamer::new(
            self.terrain.clone(),
            self.materials.clone(),
            self.edits.clone(),
        );
        if let Some(r) = self.lod0_radius {
            streamer.wymus_lod0(r);
        }
        self.streamer = Some(streamer);
        self.renderer = Some(renderer);

        // Etap 8 **po** otwarciu okna, bo `egui_winit` potrzebuje uchwytu okna, a nie
        // dlatego, że zaludnienie zależy od GPU — nie zależy. Gracz widzi w tym czasie
        // pusty ekran i raport w konsoli; przy metropolii to kilkadziesiąt sekund.
        if let (Some(city), false) = (self.city.clone(), self.bez_ludzi) {
            let start = Instant::now();
            eprintln!("Etap 8: zaludnianie miasta");
            match citizens::Citizens::new(
                self.params.seed,
                city.as_ref(),
                window.as_ref(),
                self.jezyk,
                self.watki,
            ) {
                Ok(mut c) => {
                    eprintln!(
                        "Etap 8 {:.1} s — {} mieszkańców",
                        start.elapsed().as_secs_f64(),
                        c.population()
                    );
                    // `--hour`: symulacja zawsze startuje o północy doby zerowej, bo
                    // `bootstrap_day` zasiewa kolejkę od minuty zero. Godzinę osiąga się
                    // przewinięciem, nie przestawieniem zegara — przestawiony zegar
                    // zostawiłby zdarzenia w przeszłości.
                    c.warm_up(self.godzina_startu, self.camera.eye());
                    c.set_speed(self.predkosc);
                    self.citizens = Some(c);
                }
                // Brak ludzi nie jest powodem, żeby nie pokazać miasta: klient M2
                // działał bez nich i ma dalej działać.
                Err(e) => eprintln!("Etap 8 nieudany, miasto zostaje puste: {e}"),
            }
        }
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Priorytet UI nad kamerą (§5.11): zdarzenie pochłonięte przez panel nie obraca
        // kamery i nie zaznacza pieszego. Bez tego przeciąganie suwaka w panelu
        // kręciłoby światem pod spodem.
        let pochloniete = match (&mut self.citizens, &self.window) {
            (Some(c), Some(w)) => c.on_window_event(w.as_ref(), &event),
            _ => false,
        };
        if pochloniete && !matches!(event, WindowEvent::CloseRequested | WindowEvent::Resized(_)) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // Prawy przycisk robi dwie rzeczy rozróżniane przeciągnięciem: trzymany
                // i ciągnięty przesuwa kamerę, puszczony w miejscu robi inspekcję
                // (§1 pkt 5) — promień przez kursor w teren i karta na konsolę.
                // Konsola, bo UI dokłada dopiero M11.
                if button == MouseButton::Right {
                    match state {
                        ElementState::Pressed => {
                            self.przesuw = true;
                            self.przeciagniecie_px = 0.0;
                        }
                        ElementState::Released => {
                            self.przesuw = false;
                            if self.przeciagniecie_px < PROG_KLIKNIECIA_PX {
                                self.inspekcja();
                            }
                        }
                    }
                }
                if button == MouseButton::Left {
                    // Klik w pieszego (decyzja 9.3). Pytamy bufor ID **przed** ustawieniem
                    // obrotu: trafienie w mieszkańca otwiera kartę i nie kręci kamerą.
                    if state == ElementState::Pressed {
                        let trafiony = self.renderer.as_ref().and_then(Renderer::pick);
                        if let (Some(i), Some(c)) = (trafiony, &mut self.citizens) {
                            if c.select(i) {
                                return;
                            }
                        }
                    }
                    self.obrot = state == ElementState::Pressed;
                    if !self.obrot {
                        self.ostatnia_mysz = None;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = (position.x, position.y);
                if let Some(prev) = self.ostatnia_mysz {
                    let (dx, dy) = ((p.0 - prev.0) as f32, (p.1 - prev.1) as f32);
                    if self.obrot {
                        self.camera.orbit_rotate(-dx * 0.005, dy * 0.005);
                    }
                    if self.przesuw {
                        self.przeciagniecie_px += f64::from(dx.abs() + dy.abs());
                        // „Chwyt" za teren: punkt pod kursorem ma zostać pod kursorem,
                        // więc cel jedzie w stronę przeciwną do ruchu myszy.
                        let k = self.metry_na_piksel();
                        self.camera.orbit_pan(-dx * k, dy * k);
                    }
                }
                self.ostatnia_mysz = Some(p);
                // W trybie `--pick` kursor jest zadany z wiersza poleceń i mysz nad oknem
                // nie ma prawa go przestawić — inaczej test mierzyłby, gdzie leży mysz.
                if !self.tryb_pick {
                    self.kursor = Some((position.x.max(0.0) as u32, position.y.max(0.0) as u32));
                }
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
                    // Sterowanie czasem (§5.11, decyzja 9.1). Spacja wraca do prędkości
                    // sprzed pauzy, a nie zawsze do 1×.
                    Key::Named(NamedKey::Space) => {
                        if let Some(c) = &mut self.citizens {
                            c.toggle_pause();
                        }
                    }
                    Key::Character("f") | Key::Character("F") => {
                        if let Some(c) = &mut self.citizens {
                            c.toggle_card();
                        }
                    }
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
    /// Który etap scenariusza trwa w tej chwili.
    fn etap_pomiaru(&self, sekundy: f32) -> usize {
        let na_etap = sekundy / bench::ETAPY.len() as f32;
        ((self.bench_czas_s / na_etap.max(0.001)) as usize).min(bench::ETAPY.len() - 1)
    }

    /// Ustawia kamerę zgodnie z bieżącym etapem scenariusza z §7.4.
    fn ustaw_kamere_pomiarowa(&mut self) {
        let Some(sekundy) = self.bench else {
            return;
        };
        let i = self.etap_pomiaru(sekundy);
        let etap = bench::ETAPY[i];
        let na_etap = sekundy / bench::ETAPY.len() as f32;
        // Postęp **w obrębie etapu**, 0…1.
        let t = ((self.bench_czas_s - i as f32 * na_etap) / na_etap.max(0.001)).clamp(0.0, 1.0);

        let start = self.bench_start;
        // Przesunięcie kumuluje się z poprzednich etapów: przelot 2 km ma zaczynać się tam,
        // gdzie skończył poprzedni etap, a nie wracać na start.
        let przed: f64 = bench::ETAPY[..i].iter().map(|e| e.przesuniecie_m).sum();
        let x = start.x + przed + etap.przesuniecie_m * f64::from(t);

        if let CameraMode::Orbit {
            target, dist, yaw, ..
        } = &mut self.camera.mode
        {
            target.x = x;
            target.y = start.y;
            target.z = f64::from(self.terrain.height_at(x as i32, start.y as i32)) * 0.5;
            *dist = etap.dist_m;
            *yaw = 0.6 + etap.obrot_rad * t;
        }
        // FOV jest związane z dystansem orbity (§15.2) — bez tego zoom zmienia kadr,
        // ale nie zmienia perspektywy, a raport opisywałby inny widok niż gra.
        self.camera.fov_deg = self.camera.fov_for_distance();
    }

    /// Raport z przelotu: jeden wiersz na etap plus podsumowanie całości.
    ///
    /// Percentyle, nie sama średnia: 60 FPS średnio przy jednym zacięciu 200 ms to gorsze
    /// wrażenie niż równe 50 FPS, a średnia obu nie odróżnia.
    fn raport_bench(&mut self, s: magnat_render::FrameStats) {
        eprintln!("── raport wydajności (§7.4) ──");
        for (i, etap) in bench::ETAPY.iter().enumerate() {
            let Some(p) = self.bench_etapy.get(i) else {
                continue;
            };
            eprintln!("{}", bench::wiersz(etap, p));
        }

        let wszystkie: usize = self.bench_etapy.iter().map(|p| p.ms.len()).sum();
        let zaciecia: usize = self.bench_etapy.iter().map(bench::Pomiar::zaciecia).sum();
        eprintln!(
            "razem {wszystkie} klatek w {:.0} s, zacięć > 33 ms: {zaciecia} (próg §7.4: ≤ 2 na 60 s)",
            self.bench_czas_s
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
            "GPU {gpu}; arena {:.1} MB z {:.1} MB",
            (s.vertex_bytes + s.index_bytes) as f64 / (1024.0 * 1024.0),
            s.arena_capacity_bytes as f64 / (1024.0 * 1024.0),
        );
    }

    /// Przełącza nakładkę i przelicza jej pole. Przeliczenie jest jednorazowe (dziesiątki
    /// milisekund na mapie 8 km), bo pole opisuje świat, a ten w M1 się nie zmienia.
    fn przelacz_nakladke(&mut self) {
        self.nakladka = self.nakladka.nastepna();
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        match overlay::zbuduj(&self.terrain, self.city.as_deref(), self.nakladka) {
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

    /// Ile metrów na płaszczyźnie celu odpowiada jednemu pikselowi ekranu.
    ///
    /// Bez tego przesuw byłby albo ślamazarny z orbity, albo nie do opanowania
    /// z poziomu ulicy — dystans orbity zmienia się tu o trzy rzędy wielkości.
    fn metry_na_piksel(&self) -> f32 {
        let CameraMode::Orbit { dist, .. } = self.camera.mode else {
            return 0.0;
        };
        let h = self
            .renderer
            .as_ref()
            .map_or(1080, |r| r.gpu.config.height.max(1));
        2.0 * dist * (self.camera.fov_deg.to_radians() * 0.5).tan() / h as f32
    }

    /// Wypisuje kartę inspekcji punktu pod kursorem.
    fn inspekcja(&mut self) {
        let Some((mx, my)) = self.ostatnia_mysz else {
            return;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let (w, h) = (
            f64::from(renderer.gpu.config.width),
            f64::from(renderer.gpu.config.height),
        );
        // Kierunek promienia z odwróconej macierzy kamery. Macierz jest **względem oka**,
        // więc wynik jest od razu kierunkiem w świecie, bez odejmowania pozycji.
        let vp = self
            .camera
            .view_proj_relative((w / h.max(1.0)) as f32)
            .as_dmat4();
        let ndc = glam::DVec4::new(2.0 * mx / w - 1.0, 1.0 - 2.0 * my / h, 1.0, 1.0);
        let p = vp.inverse() * ndc;
        if p.w.abs() < 1e-9 {
            return;
        }
        let kierunek = (p.truncate() / p.w).normalize_or_zero();

        match inspect::trafienie(&self.terrain, self.camera.eye(), kierunek, 4000.0) {
            Some((x, y)) => {
                eprint!("{}", inspect::karta(&self.terrain, x, y));
                // Karta parceli **tą samą funkcją** co `headless preview --inspect`:
                // kryterium WP16 mówi, że klient ma pokazać to samo co wersja headless,
                // a dwie kopie tej listy rozjechałyby się przy pierwszym nowym polu.
                if let Some(c) = &self.city {
                    for l in magnat_world::parcel_card(c, magnat_spatial::Vec2::new(x as f32, y as f32)) {
                        eprintln!("{l}");
                    }
                }
            }
            None => eprintln!("inspekcja: promień nie trafił w teren"),
        }
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

        // Czas gry. Z mieszkańcami zegarem jest `TimeControlsWidget` (decyzja 9.1)
        // i to on decyduje, ile **minut** wykonać — prędkość nie zmienia wyniku, bo
        // każda minuta jest tym samym tickiem (§5.11). Bez mieszkańców zostaje stary
        // mnożnik M1, bo nie ma czego tykać: słońce ma chodzić i tak.
        if let Some(c) = &mut self.citizens {
            let t = c.advance((dt * 1000.0) as u32);
            self.minute = SimMinute(self.dzien_slonca * 1440 + t.0);
        } else {
            let mnoznik = if self.czas_x1000 { 1000.0 } else { 1.0 };
            self.minute = SimMinute(self.minute.0 + (dt * mnoznik) as u64);
        }

        // Kamera nie wchodzi pod teren.
        let punkt = match self.camera.mode {
            CameraMode::Orbit { target, .. } => target,
            _ => self.camera.eye(),
        };
        let h = f64::from(self.terrain.height_at(punkt.x as i32, punkt.y as i32)) * 0.5;
        self.camera.clamp_above_terrain(h, 2.0);

        // Scena pomiarowa (§7.4): kamera przechodzi ustalone etapy w funkcji **czasu**,
        // a nie numeru klatki. Trasa musi być ta sama niezależnie od tego, ile klatek
        // zdążyło się narysować — inaczej wolniejsza maszyna przechodziłaby krótszą drogę
        // i wyniki dwóch maszyn opisywałyby dwa różne przeloty.
        // Indeks etapu policzony **przed** pobraniem renderera: dalej trzymamy na nim
        // pożyczkę mutowalną i `self` jest wtedy niedostępne.
        let etap = self.bench.map_or(0, |s| self.etap_pomiaru(s));
        if self.bench.is_some() {
            self.bench_czas_s += dt as f32;
            self.ustaw_kamere_pomiarowa();
        }

        let (Some(renderer), Some(streamer)) = (&mut self.renderer, &mut self.streamer) else {
            return;
        };
        streamer.update(&self.camera, renderer);
        if !self.swiatla.is_empty() {
            renderer.set_lights(&self.swiatla, self.camera.eye());
        }
        renderer.set_cursor(self.kursor);
        if let Some(c) = &mut self.citizens {
            let piesi = c.pedestrians(self.camera.eye());
            renderer.set_pedestrians(piesi, &self.camera);
        }

        self.numer_klatki += 1;
        if let Some(sciezka) = self.zrzut.clone() {
            if self.numer_klatki >= self.zrzut_po {
                // Zrzut idzie tą samą ścieżką co okno — razem z panelem, jeśli jest.
                let klatka_ui = match (&mut self.citizens, &self.window) {
                    (Some(c), Some(okno)) => {
                        let (mut wy, _) = c.ui_frame(okno.as_ref());
                        let ksztalty = std::mem::take(&mut wy.shapes);
                        let jobs = c.context().tessellate(ksztalty, wy.pixels_per_point);
                        Some((jobs, wy))
                    }
                    _ => None,
                };
                let (w, h, px) = renderer.render_to_image_with_ui(
                    &self.camera,
                    self.minute,
                    self.params.region.latitude_ddeg(),
                    1600,
                    900,
                    klatka_ui.as_ref().map(|(jobs, wy)| magnat_render::ui::UiFrame {
                        jobs,
                        textures_delta: &wy.textures_delta,
                        pixels_per_point: wy.pixels_per_point,
                    }),
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
                if let Some(c) = &self.citizens {
                    eprintln!(
                        "mieszkańcy: {} · tick {} ({:02}:{:02}) · warstwa Mikro {} · rysowanych {}",
                        c.population(),
                        c.tick().0,
                        c.tick().0 % 1440 / 60,
                        c.tick().0 % 60,
                        c.micro_len(),
                        renderer.pedestrians.drawn()
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
        if let Some(sekundy) = self.bench {
            // Pierwsze klatki to rozgrzewka strumieniowania — mierzenie ich mówiłoby
            // o czasie materializacji, a nie o rysowaniu.
            if self.numer_klatki > BENCH_ROZGRZEWKA {
                let s = renderer.stats();
                let i = etap;
                while self.bench_etapy.len() <= i {
                    self.bench_etapy.push(bench::Pomiar::default());
                }
                self.bench_etapy[i].dodaj(
                    std::time::Duration::from_secs_f64(dt),
                    s.chunks_drawn,
                    s.triangles,
                );
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
            if self.bench_czas_s >= sekundy {
                let stats = renderer.stats();
                self.raport_bench(stats);
                self.koniec = true;
                return;
            }
        }
        match (&mut self.citizens, &self.window) {
            (Some(c), Some(okno)) => {
                let (mut wyjscie, wybor) = c.ui_frame(okno.as_ref());
                if let Some(s) = wybor {
                    c.set_speed(s);
                }
                let ksztalty = std::mem::take(&mut wyjscie.shapes);
                let jobs = c.context().tessellate(ksztalty, wyjscie.pixels_per_point);
                renderer.render_with_ui(
                    &self.camera,
                    self.minute,
                    self.params.region.latitude_ddeg(),
                    Some(magnat_render::ui::UiFrame {
                        jobs: &jobs,
                        textures_delta: &wyjscie.textures_delta,
                        pixels_per_point: wyjscie.pixels_per_point,
                    }),
                );
            }
            _ => renderer.render(
                &self.camera,
                self.minute,
                self.params.region.latitude_ddeg(),
            ),
        }

        // Sprawdzenie bufora ID bez myszy (kryterium WP11). Odczyt pochodzi z klatki
        // poprzedniej, więc pytamy dopiero po kilku — inaczej mierzylibyśmy to, że
        // pierwsza klatka jeszcze nie zdążyła nic skopiować.
        if self.tryb_pick && self.numer_klatki >= 10 {
            let trafiony = renderer.pick();
            eprintln!(
                "bufor ID {}×{}, pieszych rysowanych {}",
                renderer.gpu.config.width,
                renderer.gpu.config.height,
                renderer.pedestrians.drawn()
            );
            match (trafiony, &mut self.citizens) {
                (Some(i), Some(c)) => {
                    println!("bufor ID: piksel {:?} → encja {i}", self.kursor);
                    if c.select(i) {
                        println!("{}", c.card_text());
                    } else {
                        println!("encja {i} nie jest mieszkańcem z listy populacji");
                    }
                }
                (None, _) => println!("bufor ID: piksel {:?} → nic", self.kursor),
                _ => {}
            }
            // Ze zrzutem tryb `--pick` nie kończy przebiegu: zaznaczenie otwiera kartę,
            // a klatka ze zrzutem pokazuje ją narysowaną. To jest cały dowód na to,
            // że `egui` naprawdę się wpięło (decyzja 9.2).
            self.tryb_pick = false;
            if self.zrzut.is_none() {
                self.koniec = true;
            }
            return;
        }

        self.klatki += 1;
        if self.fps_okno.elapsed().as_secs_f64() >= 0.5 {
            self.fps = f64::from(self.klatki) / self.fps_okno.elapsed().as_secs_f64();
            self.klatki = 0;
            self.fps_okno = Instant::now();
            if let Some(w) = &self.window {
                let s = renderer.stats();
                let cal = magnat_core::SimCalendar::from_minute(self.minute);
                // Piesi w tytule, bo to jedyny licznik, który mówi, czy doba naprawdę
                // biegnie — miasto o trzeciej w nocy wygląda tak samo jak zatrzymane.
                let ludzie = match &self.citizens {
                    Some(c) => format!(
                        " · {} mieszkańców, {} pieszych w kadrze",
                        c.population(),
                        renderer.pedestrians.drawn()
                    ),
                    None => String::new(),
                };
                w.set_title(&format!(
                    "Magnat — {:.0} FPS · {} chunków [{}] ({} rysowanych, {} tys. trójkątów) · \
                     {:02}:{:02} dzień {} · {:.0} MB geometrii{}",
                    self.fps,
                    s.chunks_resident,
                    streamer.opis(),
                    s.chunks_drawn,
                    s.triangles / 1000,
                    cal.hour_of_day(),
                    cal.minute_of_hour(),
                    cal.day_of_year(),
                    (s.vertex_bytes + s.index_bytes) as f64 / (1024.0 * 1024.0),
                    ludzie,
                ));
            }
        }
    }
}
