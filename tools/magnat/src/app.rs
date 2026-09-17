//! Pętla okna: stan klienta, obsługa zdarzeń `winit` i rysowanie klatki.

use crate::preview::{czasy_passow, mapa_dalekiego_terenu, swiatla_testowe};
use crate::{bench, citizens, inspect, overlay, stream};
use crate::{BENCH_ROZGRZEWKA, KROK_LOTU_M, PROG_KLIKNIECIA_PX, PROMIEN_SKLEPU_M, WZROST_OCZU_M};
use magnat_core::SimMinute;
use magnat_render::{CameraMode, GpuContext, Renderer};
use magnat_voxel::{EditIndex, MaterialRegistry};
use magnat_world::{TerrainQuery, WorldGenParams};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) struct App {
    pub(crate) terrain: Arc<magnat_world::Terrain>,
    pub(crate) materials: Arc<MaterialRegistry>,
    pub(crate) edits: Arc<EditIndex>,
    /// Miasto (M2e, WP16) — nakładka wartości gruntu i karta inspekcji parceli czytają
    /// je bezpośrednio; bez `--no-city` jest zawsze.
    pub(crate) city: Option<Arc<magnat_world::CityData>>,
    pub(crate) params: WorldGenParams,
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) renderer: Option<Renderer>,
    pub(crate) streamer: Option<stream::Streamer>,
    pub(crate) camera: magnat_render::CameraState,
    pub(crate) minute: SimMinute,
    pub(crate) czas_x1000: bool,
    pub(crate) nakladka: overlay::Nakladka,
    pub(crate) cel: Option<(i32, i32)>,
    /// Klatki przelotu pomiarowego i zebrane z nich czasy.
    pub(crate) bench: Option<f32>,
    /// Czas od startu pomiaru i wyniki kolejnych etapów.
    pub(crate) bench_czas_s: f32,
    pub(crate) bench_etapy: Vec<bench::Pomiar>,
    pub(crate) lod0_radius: Option<i32>,
    pub(crate) swiatla: Vec<magnat_sim_snapshot::LightRecord>,
    pub(crate) bench_ms: Vec<f32>,
    pub(crate) bench_pass_ms: Vec<[f32; magnat_render::PASS_NAMES.len()]>,
    pub(crate) bench_chunks: usize,
    pub(crate) occupancy_opis: String,
    pub(crate) occupancy_max: u32,
    pub(crate) bench_start: glam::DVec3,
    pub(crate) zrzut: Option<std::path::PathBuf>,
    pub(crate) zrzut_po: u32,
    pub(crate) numer_klatki: u32,
    pub(crate) koniec: bool,
    pub(crate) obrot: bool,
    /// Przeciąganie prawym przyciskiem i suma jego drogi w pikselach — poniżej
    /// `PROG_KLIKNIECIA_PX` puszczenie przycisku liczy się jako kliknięcie, nie przesuw.
    pub(crate) przesuw: bool,
    pub(crate) przeciagniecie_px: f64,
    pub(crate) ostatnia_mysz: Option<(f64, f64)>,
    pub(crate) ostatnia_klatka: Instant,
    pub(crate) klatki: u32,
    pub(crate) fps_okno: Instant,
    pub(crate) fps: f64,
    /// Mieszkańcy, doba i panel (M3d, WP11/WP12). `None` przy `--no-city`, przy
    /// `--no-citizens` i dopóki okno nie powstało — Etap 8 potrzebuje miasta,
    /// a `egui` potrzebuje okna.
    pub(crate) citizens: Option<citizens::Citizens>,
    /// Teren i miasto, dopóki nie przejmie ich sesja gry. `take()` przy otwarciu
    /// okna — świat ma jednego właściciela, a od M9a jest nim `game::Session`.
    pub(crate) built: Option<magnat_game::BuiltCity>,
    pub(crate) bez_ludzi: bool,
    /// `--no-economy`: świat bez rynku, czyli zachowanie sprzed M5 (`AB-2`).
    pub(crate) bez_gospodarki: bool,
    pub(crate) jezyk: magnat_ui::Locale,
    pub(crate) watki: usize,
    /// Ostatnia znana pozycja kursora w pikselach — bufor ID kopiuje piksel spod niej.
    pub(crate) kursor: Option<(u32, u32)>,
    pub(crate) tryb_pick: bool,
    /// Minuta doby, do której przewijamy symulację przed pierwszą klatką.
    pub(crate) godzina_startu: u32,
    pub(crate) predkosc: magnat_core::SimSpeed,
    /// Dzień roku dla **słońca**. Symulacja liczy własną dobę od zera; `--day` ustawia
    /// porę roku, czyli deklinację, i tylko ją.
    pub(crate) dzien_slonca: u64,
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
        if let (Some(built), false) = (self.built.take(), self.bez_ludzi) {
            let start = Instant::now();
            eprintln!("Etap 8: zaludnianie miasta");
            match citizens::Citizens::new(
                self.params,
                built,
                window.as_ref(),
                self.jezyk,
                self.watki,
                !self.bez_gospodarki,
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
        // Nakładki ruchu mają własne źródło — zrzut symulacji, nie dane generatora.
        // Bez zaludnionego świata po prostu nie ma czego rysować i nakładka gaśnie;
        // to jest uczciwsze niż pokazanie pustej mapy, która wygląda jak brak korków.
        let ruch = self.nakladka.pole_ruchu().and_then(|f| {
            let rozmiar = self.city.as_deref()?.plan.map_size_m().max(1) as u32;
            self.citizens.as_ref()?.pole_ruchu(f, rozmiar)
        });
        let pole = match ruch {
            Some(p) => Some(p),
            None => overlay::zbuduj(&self.terrain, self.city.as_deref(), self.nakladka),
        };
        match pole {
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
                // Sklep ma pierwszeństwo przed kartą terenu: jeśli gracz trafił
                // w zabudowę handlową, chce panelu, a nie kolumny geologicznej.
                // Bufor identyfikatorów renderera niesie dziś **wyłącznie pieszych**
                // (korekta H-15 do M3d), więc zakład wybiera się z promienia wokół
                // punktu trafienia — promień jest rzędu długości pierzei, nie kadru.
                if let Some(c) = self.citizens.as_mut() {
                    if c.select_shop(x as f32, y as f32, PROMIEN_SKLEPU_M) {
                        return;
                    }
                }
                eprint!("{}", inspect::karta(&self.terrain, x, y));
                // Karta parceli **tą samą funkcją** co `headless preview --inspect`:
                // kryterium WP16 mówi, że klient ma pokazać to samo co wersja headless,
                // a dwie kopie tej listy rozjechałyby się przy pierwszym nowym polu.
                if let Some(c) = &self.city {
                    for l in
                        magnat_world::parcel_card(c, magnat_spatial::Vec2::new(x as f32, y as f32))
                    {
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
                    klatka_ui
                        .as_ref()
                        .map(|(jobs, wy)| magnat_render::ui::UiFrame {
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
