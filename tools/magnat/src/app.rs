//! Pętla okna: stan klienta, obsługa zdarzeń `winit` i rysowanie klatki.
//!
//! Od M9b/WP14 klient **zaczyna od menu głównego**, a nie od wygenerowanego świata:
//! `magnat` bez ani jednego argumentu prowadzi przez kreator, generację i podgląd
//! do grającego miasta. Droga przez wiersz poleceń zostaje bez zmian i omija powłokę —
//! używa jej każdy zrzut, przelot pomiarowy i test bufora identyfikatorów.
//!
//! Stan gry trzyma [`magnat_game::GameState`], czyli ten sam typ, którym posługuje się
//! przebieg bezgłowy. Klient nie ma własnego enuma stanów i mieć nie powinien:
//! dwie listy stanów rozjechałyby się przy pierwszym nowym ekranie.

use crate::{bench, citizens, inspect, overlay, scenes, stream};
use crate::{BENCH_ROZGRZEWKA, KROK_LOTU_M, PROG_KLIKNIECIA_PX, PROMIEN_SKLEPU_M, WZROST_OCZU_M};
use magnat_core::SimMinute;
use magnat_game::screens::Shell;
use magnat_game::shell::ShellScreen;
use magnat_game::GameState;
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
    /// Teren — `None`, dopóki gracz nie wyszedł z menu głównego.
    pub(crate) terrain: Option<Arc<magnat_world::Terrain>>,
    pub(crate) materials: Arc<MaterialRegistry>,
    pub(crate) edits: Arc<EditIndex>,
    /// Miasto (M2e, WP16) — nakładka wartości gruntu i karta inspekcji parceli czytają
    /// je bezpośrednio.
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
    pub(crate) bench_czas_s: f32,
    /// Zegar animacji w milisekundach (M11b §5.4). Rośnie czasem **realnym**, gdy gra
    /// idzie, i stoi przy pauzie i w powłoce.
    ///
    /// Nie jest minutą świata i nie jest z nią związany mnożnikiem prędkości: chód
    /// ma wyglądać jak chód przy każdej prędkości, a przy ×10 nogi przebierałyby
    /// dziesięć razy szybciej, niż da się zobaczyć. Do symulacji ta liczba nie wchodzi
    /// — jedzie do snapshotu wyłącznie jako faza klipu.
    pub(crate) anim_ms: u64,
    /// Nierozliczona część milisekundy zegara animacji — patrz [`App::klatka`].
    pub(crate) anim_reszta: f64,
    /// Poziom cięcia budynków: 0 = wyłączone (M11c §5.7). Przełącza `C`.
    pub(crate) ciecie: u8,
    /// Płaszczyzna przekroju policzona w tej klatce.
    pub(crate) przekroj: magnat_render::CutPlane,
    /// Atlas szyldów i odwzorowanie „zakład → kafel" (WP9). Powstaje raz na postawione
    /// miasto: nazwa firmy nie zmienia się sama, a gdy zmieni ją gracz, kafel dochodzi.
    pub(crate) szyldy: magnat_render::SignAtlas,
    pub(crate) szyldy_seed: Option<u64>,
    /// Prostokąty napisów tej klatki.
    pub(crate) szyldy_kadr: Vec<magnat_render::SignQuad>,
    /// Bufory przekroju i wnętrz — trzymane między klatkami, bo klatka nie alokuje.
    pub(crate) wnetrza: crate::interiors::Wnetrza,
    /// `--crowd`: ilu syntetycznych pieszych dosypać do snapshotu (scena pomiarowa M11b).
    pub(crate) tlum: usize,
    /// Odstęp między nimi w metrach — decyduje, w którym paśmie detalu wypadnie tłum.
    pub(crate) krok_tlumu: f64,
    /// `--no-anim`: wszystkie encje w pozie spoczynkowej — druga połowa pomiaru WP3.
    pub(crate) bez_animacji: bool,
    /// Katalog klipów. Ten sam, który renderer wypalił do atlasu póz, i ten sam,
    /// którego używa wypełniacz snapshotu — jedna funkcja, trzech czytelników.
    pub(crate) klipy: magnat_voxel::ClipLibrary,
    pub(crate) bench_etapy: Vec<bench::Pomiar>,
    pub(crate) lod0_radius: Option<i32>,
    pub(crate) swiatla: Vec<magnat_sim_snapshot::LightRecord>,
    /// Kanał sim → render (M11a WP2). Wypełniacz składa rekordy z ECS, para buforów
    /// oddziela to, co symulacja właśnie pisze, od tego, co render czyta.
    pub(crate) filler: magnat_game::SnapshotFiller,
    pub(crate) snapshot: magnat_sim_snapshot::SnapshotPair,
    /// Palety dzielnic — czyta je klient, bo katalog `data/` jest jego, nie renderu.
    pub(crate) palettes: Option<Arc<magnat_voxel::PaletteLibrary>>,
    pub(crate) bench_ms: Vec<f32>,
    pub(crate) bench_pass_ms: Vec<[f32; magnat_render::PASS_NAMES.len()]>,
    pub(crate) bench_chunks: usize,
    pub(crate) occupancy_opis: String,
    pub(crate) occupancy_max: u32,
    /// Zajętość najgęstszego klastra świateł z poprzedniej klatki. To ona, a nie suma
    /// świateł, steruje przerzedzaniem latarni (M11d §5.8) — odczyt z GPU jest
    /// o klatkę spóźniony i to nie przeszkadza, bo histereza i tak wygładza przełączanie.
    pub(crate) occupancy_klatki: u32,
    /// Warstwa dźwiękowa. `None` = `--no-audio` albo brak urządzenia; gra jest wtedy
    /// cicha i nic poza tym się nie zmienia (§7.1 `audio_off_equals_audio_on`).
    pub(crate) audio: Option<magnat_audio::AudioEngine>,
    /// Wymuszenia sceny pomiarowej pogody (`--precip`, `--snow`, `--blackout`).
    pub(crate) wymus_opad: Option<u8>,
    pub(crate) wymus_snieg: Option<u8>,
    pub(crate) wymus_blackout: u16,
    pub(crate) bench_start: glam::DVec3,
    /// Przebieg sceny odniesienia budżetu klatki (`--bench-scene`, M11e/WP10).
    pub(crate) scena: Option<scenes::Przebieg>,
    /// Klatki rozgrzewki i pomiaru sceny odniesienia.
    pub(crate) scena_rozgrzewka: u32,
    pub(crate) scena_klatek: u32,
    /// Kod wyjścia procesu: `false` = scena przekroczyła próg albo nie było pomiaru.
    pub(crate) scena_ok: bool,
    /// Koszt selekcji kadru po stronie klienta — `query_rect` plus odrzucenia po
    /// `Building.aabb` (zobowiązanie wobec M2, próg alarmowy 0,3 ms w `bench_city`).
    pub(crate) select_ms: f32,
    /// Z tego — sam koszt `CsrGrid::query_rect` po indeksie budynków i odrzuceń
    /// po `Building.aabb` (zobowiązanie wobec M2, próg alarmowy 0,3 ms).
    pub(crate) budynki_ms: f32,
    pub(crate) zrzut: Option<std::path::PathBuf>,
    pub(crate) zrzut_po: u32,
    pub(crate) numer_klatki: u32,
    pub(crate) koniec: bool,
    pub(crate) obrot: bool,
    /// Przeciąganie prawym przyciskiem i suma jego drogi w pikselach — poniżej
    /// `PROG_KLIKNIECIA_PX` puszczenie przycisku liczy się jako kliknięcie.
    pub(crate) przesuw: bool,
    pub(crate) przeciagniecie_px: f64,
    pub(crate) ostatnia_mysz: Option<(f64, f64)>,
    pub(crate) ostatnia_klatka: Instant,
    pub(crate) klatki: u32,
    pub(crate) fps_okno: Instant,
    pub(crate) fps: f64,
    /// Interfejs rozgrywki: zaznaczenie, panele, zegar. Sesję trzyma [`App::game`].
    pub(crate) citizens: Option<citizens::Citizens>,
    /// Stan gry: powłoka, generacja, podgląd albo stojący świat.
    pub(crate) game: GameState,
    /// Powłoka: ekrany, motyw, katalog tekstów, ustawienia gracza.
    pub(crate) shell: Shell,
    /// Czy nad grającym światem stoi menu pauzy. Osobna flaga, a nie stan gry:
    /// sesja **zostaje w pamięci**, więc powrót do gry nie wczytuje niczego (M9a §5.13).
    pub(crate) pauza_menu: bool,
    /// Miniatura podglądu świata jako tekstura `egui`. Powstaje raz, po generacji.
    pub(crate) podglad_tex: Option<egui::TextureHandle>,
    pub(crate) zapisy: std::path::PathBuf,
    pub(crate) egui_ctx: Option<egui::Context>,
    pub(crate) egui_state: Option<egui_winit::State>,
    pub(crate) bez_ludzi: bool,
    /// `--no-economy`: świat bez rynku, czyli zachowanie sprzed M5 (`AB-2`).
    pub(crate) bez_gospodarki: bool,
    pub(crate) watki: usize,
    /// Ostatnia znana pozycja kursora w pikselach — bufor ID kopiuje piksel spod niej.
    pub(crate) kursor: Option<(u32, u32)>,
    pub(crate) tryb_pick: bool,
    /// Minuta doby, do której przewijamy symulację przed pierwszą klatką.
    pub(crate) godzina_startu: u32,
    pub(crate) predkosc: magnat_core::SimSpeed,
    /// Dzień roku dla **słońca**. Symulacja liczy własną dobę od zera.
    pub(crate) dzien_slonca: u64,
    /// Edytor reguł otwarty nad światem (`M9d` WP8). `None` = zamknięty.
    ///
    /// Stan ekranu, a nie stan gry: świat pod spodem tyka dalej, a polityka wchodzi
    /// dopiero komendą `AttachPolicy`. Dzięki temu wyjście z edytora niczego nie cofa,
    /// bo niczego jeszcze nie zmieniło.
    pub(crate) redaktor: Option<crate::session::Redaktor>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Magnat")
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
        // Scena pomiarowa mierzy klatkę jak `--bench` — bez vsync (N1.12, `M11#3`).
        // Do E1 wyłączał go tylko `--bench`, więc każdy raport `bench/frames/*.json`
        // miał `frame_ms` p95 ≈ 27,4 ms niezależnie od sceny: to był takt monitora.
        if (self.bench.is_some() || self.scena.is_some()) && !gpu.disable_vsync() {
            eprintln!("uwaga: sterownik nie daje trybu bez vsync — FPS będzie obcięty do odświeżania monitora");
        }
        // Modele encji i palety dzielnic są daną tak samo jak materiały; czyta je klient
        // i podaje rendererowi, bo `engine/render` rysuje dane, a nie chodzi po katalogach.
        // Brak któregokolwiek z tych katalogów jest błędem instalacji, nie stanem gry —
        // stąd panika z nazwą pliku, tak samo jak przy otwieraniu okna wyżej.
        let models = magnat_voxel::ModelLibrary::load_dir(&magnat_world::data_path("models"))
            .unwrap_or_else(|e| panic!("data/models: {e}"));
        let palettes = Arc::new(
            magnat_voxel::PaletteLibrary::load(&magnat_world::data_path("palettes"))
                .unwrap_or_else(|e| panic!("data/palettes: {e}")),
        );
        self.renderer = Some(Renderer::new(gpu, &self.materials, &models, &palettes));
        self.palettes = Some(palettes);

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        self.egui_ctx = Some(egui_ctx);
        self.egui_state = Some(egui_state);
        self.window = Some(window);

        // Świat z wiersza poleceń jest już postawiony — wystarczy go wpiąć.
        // Bez niego zostajemy w menu głównym i nie ma czego strumieniować.
        if let GameState::WorldReady { built, .. } =
            std::mem::replace(&mut self.game, GameState::Shell)
        {
            self.wejdz_do_swiata(*built);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Priorytet UI nad kamerą (§5.11): zdarzenie pochłonięte przez panel nie obraca
        // kamery i nie zaznacza pieszego. Bez tego przeciąganie suwaka w panelu
        // kręciłoby światem pod spodem.
        let pochloniete = match (&mut self.egui_state, &self.window) {
            (Some(s), Some(w)) => s.on_window_event(w.as_ref(), &event).consumed,
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
            WindowEvent::MouseInput { state, button, .. } if self.game.is_playing() => {
                // Prawy przycisk robi dwie rzeczy rozróżniane przeciągnięciem: trzymany
                // i ciągnięty przesuwa kamerę, puszczony w miejscu robi inspekcję.
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
                        // Kartę mieszkańca otwiera wyłącznie trafienie w mieszkańca;
                        // pojazd ma własną kartę i wchodzi razem z nią (M11c).
                        let trafiony = self
                            .renderer
                            .as_ref()
                            .and_then(Renderer::pick)
                            .filter(|h| h.kind == magnat_render::PickKind::Citizen)
                            .map(|h| h.entity);
                        if let (Some(i), Some(c), Some(s)) =
                            (trafiony, self.citizens.as_mut(), self.game.session_mut())
                        {
                            if c.select(s, i) {
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
            WindowEvent::MouseWheel { delta, .. } if self.game.is_playing() => {
                let kroki = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                };
                self.camera.orbit_zoom(0.9f32.powf(kroki));
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.klawisz(&event);
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
    /// Klawiatura. W powłoce nie robi **nic** — tam klawisze należą do `egui`
    /// i do ekranów z `game::screens`; tutaj są wyłącznie skróty rozgrywki.
    ///
    /// Granica jest twarda, bo do M9e nie była: ta funkcja obsługiwała Esc równolegle
    /// z ekranem powłoki, a oba widziały **to samo** naciśnięcie w tej samej klatce.
    /// Na ekranach powłoki wygrywało wyjście z gry, w menu pauzy — natychmiastowy
    /// powrót do niej. Warunkiem jest `w_powloce()`, a nie `is_playing()`: menu pauzy
    /// stoi nad grającą sesją i też jest powłoką.
    fn klawisz(&mut self, event: &winit::event::KeyEvent) {
        if self.w_powloce() {
            return;
        }
        match event.logical_key.as_ref() {
            // Esc tu **nie ma** i mieć nie może: otwarcie pauzy czyta `buduj_ui`
            // po stronie `egui`, bo tamten klawisz obsługuje też menu pauzy
            // i oba miejsca widziałyby jedno naciśnięcie (`DE-13`).
            Key::Character("t") | Key::Character("T") => {
                self.czas_x1000 = !self.czas_x1000;
            }
            Key::Named(NamedKey::F3) => self.przelacz_nakladke(),
            Key::Named(NamedKey::F4) => {
                if let Some(c) = &mut self.citizens {
                    let f = c.przelacz_filtr();
                    eprintln!("filtr encji: {}", f.unwrap_or("wszyscy"));
                }
            }
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
            // Edytor reguł dla zaznaczonego zakładu (`M9d` WP8). Bez zaznaczenia
            // nie ma czego edytować — polityka jest zawsze polityką **czegoś**.
            Key::Character("r") | Key::Character("R") => self.otworz_edytor(),
            // Tryb „śledź" (§5.10): kamera idzie za zaznaczonym mieszkańcem,
            // a `LodPin` trzyma go w warstwie Mikro także przy 10×.
            Key::Character("g") | Key::Character("G") => self.przelacz_sledzenie(),
            // Cięcie poziomami (§15.2): kolejne naciśnięcie zdejmuje kolejną kondygnację
            // budynku, na który patrzy gracz, a czwarte wraca do widoku z dachami.
            Key::Character("c") | Key::Character("C") => {
                self.ciecie = (self.ciecie + 1) % 4;
                eprintln!(
                    "cięcie poziomami: {}",
                    if self.ciecie == 0 {
                        "wyłączone".to_string()
                    } else {
                        format!("poziom {}", self.ciecie)
                    }
                );
            }
            Key::Character("1") => self.camera.to_orbit(600.0),
            Key::Character("2") => self.camera.to_free(),
            // Widok pierwszoosobowy. Z postacią gracza kamera **przypina się do niej**
            // (§5.7): pozycja oka przychodzi ze snapshotu, a nie z klawiszy, bo postać
            // gracza jest zwykłym agentem i porusza się jak każdy inny (decyzja 9.7).
            Key::Character("3") => {
                let oko = self.camera.eye();
                let grunt = self.terrain.as_ref().map_or(0.0, |t| {
                    f64::from(t.height_at(oko.x as i32, oko.y as i32)) * 0.5
                });
                self.camera.to_first_person(grunt, WZROST_OCZU_M);
                if let Some(c) = self.postac_gracza() {
                    self.camera.set_anchor(Some(u64::from(c)));
                }
            }
            Key::Character(c) if matches!(c, "w" | "s" | "a" | "d" | "q" | "e") => {
                self.lec(c);
            }
            _ => {}
        }
    }

    pub(crate) fn pauza(&mut self) {
        if let Some(c) = &mut self.citizens {
            c.set_speed(magnat_core::SimSpeed::Paused);
        }
        self.shell.has_session = true;
        self.shell.refresh_slots(&self.zapisy);
        self.shell.go(ShellScreen::Pause);
        // Sesja **zostaje w pamięci**: powrót do gry nie wczytuje niczego, więc
        // `GameState` zostaje `Playing`, a nad nim staje ekran.
        self.pauza_menu = true;
    }

    /// Który etap scenariusza trwa w tej chwili.
    pub(crate) fn etap_pomiaru(&self, sekundy: f32) -> usize {
        let na_etap = sekundy / bench::ETAPY.len() as f32;
        ((self.bench_czas_s / na_etap.max(0.001)) as usize).min(bench::ETAPY.len() - 1)
    }

    /// Ustawia kamerę zgodnie z bieżącym etapem scenariusza z §7.4.
    pub(crate) fn ustaw_kamere_pomiarowa(&mut self) {
        let Some(sekundy) = self.bench else {
            return;
        };
        let Some(terrain) = self.terrain.clone() else {
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
            target.z = f64::from(terrain.height_at(x as i32, start.y as i32)) * 0.5;
            *dist = etap.dist_m;
            *yaw = 0.6 + etap.obrot_rad * t;
        }
        // FOV jest związane z dystansem orbity (§15.2) — bez tego zoom zmienia kadr,
        // ale nie zmienia perspektywy, a raport opisywałby inny widok niż gra.
        self.camera.fov_deg = self.camera.fov_for_distance();
    }

    /// Przełącza nakładkę i przelicza jej pole. Przeliczenie jest jednorazowe (dziesiątki
    /// milisekund na mapie 8 km), bo pole opisuje świat, a ten w M1 się nie zmienia.
    fn przelacz_nakladke(&mut self) {
        self.nakladka = self.nakladka.nastepna();
        let Some(terrain) = self.terrain.clone() else {
            return;
        };
        // Nakładki ruchu mają własne źródło — zrzut symulacji, nie dane generatora.
        // Bez zaludnionego świata po prostu nie ma czego rysować i nakładka gaśnie;
        // to jest uczciwsze niż pokazanie pustej mapy, która wygląda jak brak korków.
        // Dziewięć nakładek danych z §14.2 liczy `game::overlays` — pytają naraz
        // o mieszkańców, rynek i zakłady, więc potrzebują sesji, a nie terenu.
        let dane = self.pole_danych();
        let ruch = self.nakladka.pole_ruchu().and_then(|f| {
            let rozmiar = self.city.as_deref()?.plan.map_size_m().max(1) as u32;
            let s = self.game.session()?;
            self.citizens.as_ref()?.pole_ruchu(s, f, rozmiar)
        });
        let pole = dane
            .or(ruch)
            .or_else(|| overlay::zbuduj(&terrain, self.city.as_deref(), self.nakladka));
        let Some(renderer) = self.renderer.as_mut() else {
            return;
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

    /// Pole nakładki danych z §14.2 razem z legendą dla HUD-u.
    ///
    /// Zwraca `None`, gdy nakładka nie jest daną (teren, ruch), gdy świat jeszcze nie
    /// stoi albo gdy nakładce brakuje przedmiotu — zasięg bez wybranego sklepu nie ma
    /// czego pokazać i gaśnie, zamiast rysować pustą mapę.
    fn pole_danych(&mut self) -> Option<overlay::Pole> {
        // Drugi krok samouczka kończy się **włączeniem** nakładki zasięgu, a nie
        // tym, że miała co narysować (`DI-35`): gracz bez wybranego sklepu robi
        // dokładnie to, o co go poproszono, i ma iść dalej.
        let zasieg = self.nakladka == overlay::Nakladka::ZasiegSklepu;
        if let Some(c) = self.citizens.as_mut() {
            c.zasieg_wlaczony = zasieg;
        }
        if !self.nakladka.jest_danymi() {
            self.ustaw_legende(None);
            return None;
        }
        let site = self
            .citizens
            .as_ref()
            .and_then(crate::citizens::Citizens::wybrany_zaklad);
        let good = magnat_core::GoodId(0);
        let field = self.nakladka.pole_danych(site, good);
        let pole = field
            .zip(self.game.session())
            .and_then(|(f, s)| magnat_game::overlays::build(s, f));
        match pole {
            Some(p) => {
                self.ustaw_legende(field.map(|f| crate::citizens::Legenda {
                    key: f.key(),
                    stops: p.legend.clone(),
                    unit: p.unit.clone(),
                    palette: p.palette,
                }));
                Some(overlay::Pole {
                    dim: p.dim,
                    cell_m: p.cell_m,
                    values: p.values,
                    palette: p.palette,
                })
            }
            None => {
                self.ustaw_legende(None);
                None
            }
        }
    }

    fn ustaw_legende(&mut self, l: Option<crate::citizens::Legenda>) {
        if let Some(c) = &mut self.citizens {
            c.set_legend(l);
        }
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
        let (Some(renderer), Some(terrain)) = (self.renderer.as_ref(), self.terrain.clone()) else {
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

        match inspect::trafienie(&terrain, self.camera.eye(), kierunek, 4000.0) {
            Some((x, y)) => {
                // Sklep ma pierwszeństwo przed kartą terenu: jeśli gracz trafił
                // w zabudowę handlową, chce panelu, a nie kolumny geologicznej.
                if let (Some(c), Some(s)) = (self.citizens.as_mut(), self.game.session()) {
                    if c.select_shop(s, x as f32, y as f32, PROMIEN_SKLEPU_M) {
                        return;
                    }
                }
                eprint!("{}", inspect::karta(&terrain, x, y));
                // Karta parceli **tą samą funkcją** co `headless preview --inspect`.
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
}

impl App {
    /// Jedna klatka: czas gry, kamera, interfejs, render.
    ///
    /// `ponytail:` funkcja ma ~170 linii i zostaje jedną funkcją. Sufit nazwany:
    /// **kolejność kroków jest kontraktem determinizmu** (`game::session` §5.2) i rozbicie
    /// jej na pięć metod przeniosłoby tę kolejność do miejsca wywołania, czyli tam,
    /// gdzie nikt jej nie czyta. Rozgałęzienia, które dało się wyjąć (zrzut, pomiar,
    /// diagnostyka bufora ID), są już osobnymi metodami.
    pub(crate) fn klatka(&mut self) {
        let teraz = Instant::now();
        let dt = teraz.duration_since(self.ostatnia_klatka).as_secs_f64();
        self.ostatnia_klatka = teraz;
        self.odbierz_generacje();

        // Czas gry. Z sesją zegarem jest `TimeControlsWidget` (decyzja 9.1) i to on
        // decyduje, ile **minut** wykonać — prędkość nie zmienia wyniku, bo każda
        // minuta jest tym samym tickiem (§5.11). W powłoce symulacja stoi.
        if !self.w_powloce() {
            if let Some(c) = self.citizens.as_mut() {
                // Zegar gry stoi → animacja też stoi: postacie deptałyby w miejscu
                // na zamrożonym świecie.
                //
                // Scena odniesienia jest wyjątkiem i to nie jest obejście pauzy, tylko
                // jej właściwe użycie: symulacja ma stać, żeby sześćset klatek mierzyło
                // ten sam świat, ale **zegar prezentacji musi iść**, bo od niego zależy
                // faza klipu, ruch cząstek pogody i rampa wygaszenia dzielnicy. Z nim
                // zatrzymanym `bench_blackout` nigdy nie gasił ani jednej latarni
                // i porównywał scenę samą ze sobą.
                if c.ui.time.speed() != magnat_core::SimSpeed::Paused || self.scena.is_some() {
                    // Reszta zostaje w akumulatorze, bo przy 60 klatkach na sekundę
                    // `dt` to 16,67 ms i samo obcięcie gubiłoby 4 % czasu animacji —
                    // chód szedłby wolniej niż świat, a przy tysiącu klatek stanąłby.
                    self.anim_reszta += dt * 1000.0;
                    let cale = self.anim_reszta.floor();
                    self.anim_reszta -= cale;
                    self.anim_ms = self.anim_ms.wrapping_add(cale as u64);
                }
                if let GameState::Playing(s) = &mut self.game {
                    let t = c.advance(s, (dt * 1000.0) as u32);
                    self.minute = SimMinute(self.dzien_slonca * 1440 + t.0);
                }
            }
            // Zgon postaci i domknięcie scenariusza przełączają stan gry (`DI-33`,
            // `DI-34`). Decyzja jest w `game/`, a nie tutaj: przebieg bezgłowy musi
            // dostać tę samą odpowiedź co okno.
            self.game.settle();
        } else if self.terrain.is_some() && self.citizens.is_none() {
            let mnoznik = if self.czas_x1000 { 1000.0 } else { 1.0 };
            self.minute = SimMinute(self.minute.0 + (dt * mnoznik) as u64);
        }

        // Kamera nie wchodzi pod teren.
        if let Some(terrain) = self.terrain.clone() {
            let punkt = match self.camera.mode {
                CameraMode::Orbit { target, .. } => target,
                _ => self.camera.eye(),
            };
            let h = f64::from(terrain.height_at(punkt.x as i32, punkt.y as i32)) * 0.5;
            self.camera.clamp_above_terrain(h, 2.0);
        }

        // Scena pomiarowa (§7.4): kamera przechodzi ustalone etapy w funkcji **czasu**,
        // a nie numeru klatki. Trasa musi być ta sama niezależnie od tego, ile klatek
        // zdążyło się narysować.
        let etap = self.bench.map_or(0, |s| self.etap_pomiaru(s));
        if self.bench.is_some() {
            self.bench_czas_s += dt as f32;
            self.ustaw_kamere_pomiarowa();
        }

        // Selekcja kadru po stronie klienta, **cała**: wybór encji do snapshotu,
        // `CsrGrid::query_rect` po budynkach i odrzucenia po `Building.aabb`.
        // Mierzona osobno, bo to zobowiązanie wobec M2 — `GridSpec` został w 2D
        // na podstawie argumentu M11, a próg 0,3 ms jest jedyną liczbą, która może
        // ten argument obalić (WP10).
        //
        // Zegar obejmuje **wszystkie trzy kroki**, a nie dwa ostatnie: `query_rect`
        // po siatce budynków wołają wyłącznie wnętrza, a te milczą bez aktywnego
        // cięcia — więc pomiar zaczęty po publikacji snapshotu mierzył w scenach
        // orbitalnych dwa wczesne powroty i nic poza tym.
        let zegar_selekcji = Instant::now();
        self.publikuj_snapshot();
        // Drugi zegar obejmuje **samo odpytanie indeksu budynków** — to jest liczba,
        // której dotyczy zobowiązanie wobec M2, a nie koszt całego składania klatki.
        let zegar_budynkow = Instant::now();
        self.zloz_przekroj();
        self.zloz_szyldy();
        self.budynki_ms = zegar_budynkow.elapsed().as_secs_f32() * 1000.0;
        self.select_ms = zegar_selekcji.elapsed().as_secs_f32() * 1000.0;
        let klatka_ui = self.buduj_ui();
        if let Some((_, Some(predkosc))) = &klatka_ui {
            let p = *predkosc;
            if let Some(c) = &mut self.citizens {
                c.set_speed(p);
            }
        }

        let kamera = self.camera;
        let minuta = self.minute;
        let szerokosc = self.params.region.latitude_ddeg();
        let swiatla = std::mem::take(&mut self.swiatla);
        // Scena odniesienia ma kursor **na stałe w środku kadru**, a nie tam, gdzie
        // akurat leży mysz. Pass bufora identyfikatorów rysuje pełną geometrię encji
        // drugi raz, więc od położenia myszy zależałoby, czy klatka kosztuje o 0,1 ms
        // więcej — a bramka regresji porównywałaby wtedy myszy, nie kod.
        let kursor = match self.scena.as_ref() {
            Some(_) => self
                .window
                .as_ref()
                .map(|w| (w.inner_size().width / 2, w.inner_size().height / 2)),
            None => self.kursor,
        };
        let ctx = self.egui_ctx.clone();
        let (jobs, delta, ppp) = match (klatka_ui, ctx) {
            (Some((mut out, _)), Some(ctx)) => {
                let ksztalty = std::mem::take(&mut out.shapes);
                let jobs = ctx.tessellate(ksztalty, out.pixels_per_point);
                let ppp = out.pixels_per_point;
                (Some(jobs), std::mem::take(&mut out.textures_delta), ppp)
            }
            _ => (None, egui::TexturesDelta::default(), 1.0),
        };
        let ui_frame = jobs.as_ref().map(|j| magnat_render::ui::UiFrame {
            jobs: j,
            textures_delta: &delta,
            pixels_per_point: ppp,
        });

        let Some(renderer) = self.renderer.as_mut() else {
            self.swiatla = swiatla;
            return;
        };
        if let Some(streamer) = self.streamer.as_mut() {
            streamer.update(&kamera, renderer);
        }
        // Scena pomiarowa `--lights` ma pierwszeństwo: mierzy przypisanie świateł
        // na zadanej liczbie, a nie na tym, ile ich akurat stoi w kadrze. Bez niej
        // źródłem jest snapshot i to jest droga rozgrywki (§5.8).
        if swiatla.is_empty() {
            renderer.set_lights(self.snapshot.front().lights.as_slice(), kamera.eye());
        } else {
            renderer.set_lights(&swiatla, kamera.eye());
        }
        renderer.set_cursor(kursor);
        renderer.set_cut(self.przekroj, &self.wnetrza.cuts, kamera.eye());
        renderer.set_signs(&mut self.szyldy, &self.szyldy_kadr, kamera.eye());
        renderer.set_interiors(&self.wnetrza.props);
        renderer.set_entities(self.snapshot.front(), &kamera);
        renderer.set_weather(self.snapshot.front(), kamera.eye());
        self.numer_klatki += 1;

        if let Some(sciezka) = self.zrzut.clone() {
            if self.numer_klatki >= self.zrzut_po {
                let (w, h, px) = renderer
                    .render_to_image_with_ui(&kamera, minuta, szerokosc, 1600, 900, ui_frame);
                let s = renderer.stats();
                self.raport_zrzutu(&s, &kamera);
                match magnat_devtools::write_rgb(&sciezka, w, h, &px) {
                    Ok(b) => eprintln!("zrzut: {} ({w}×{h}, {b} B)", sciezka.display()),
                    Err(e) => eprintln!("zrzut nieudany: {e}"),
                }
                self.zrzut = None;
                self.koniec = true;
                self.swiatla = swiatla;
                return;
            }
        }

        if let Some(sekundy) = self.bench {
            // Pierwsze klatki to rozgrzewka strumieniowania — mierzenie ich mówiłoby
            // o czasie materializacji, a nie o rysowaniu.
            if self.numer_klatki > BENCH_ROZGRZEWKA {
                let s = renderer.stats();
                while self.bench_etapy.len() <= etap {
                    self.bench_etapy.push(bench::Pomiar::default());
                }
                self.bench_etapy[etap].dodaj(
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
                let o = renderer.cluster_occupancy();
                if o.max() >= self.occupancy_max {
                    self.occupancy_max = o.max();
                    self.occupancy_opis = o.summary();
                }
            }
            if self.bench_czas_s >= sekundy {
                let stats = renderer.stats();
                self.swiatla = swiatla;
                self.raport_bench(stats);
                self.koniec = true;
                return;
            }
        }

        // Cel budżetu wg tego, na co patrzy kadr — 33,3 ms dla widoku miasta, 16,6 ms
        // dla dzielnicy i ulicy (§5.10, PRD §20.2). Scena odniesienia ma cel zapisany
        // i **zamrożony detal**, żeby raport mierzył ten obraz, który opisuje (N1.12).
        renderer.budget.target_ms = match self.scena.as_ref() {
            Some(p) => p.scena.target_ms,
            None => magnat_render::target_for_camera(&self.camera.mode),
        };
        renderer.budget.frozen = self.scena.is_some();
        renderer.render_with_ui(&kamera, minuta, szerokosc, ui_frame);
        self.occupancy_klatki = renderer.cluster_occupancy().max();
        self.krok_dzwieku(&kamera, dt as f32);
        self.swiatla = swiatla;

        if self.scena.is_some() {
            self.krok_sceny(dt);
            if self.koniec {
                return;
            }
        }

        // Sprawdzenie bufora ID bez myszy (kryterium WP11). Odczyt pochodzi z klatki
        // poprzedniej, więc pytamy dopiero po kilku.
        if self.tryb_pick && self.numer_klatki >= 10 {
            self.pick_diagnostyczny();
            return;
        }

        self.klatki += 1;
        if self.fps_okno.elapsed().as_secs_f64() >= 0.5 {
            self.fps = f64::from(self.klatki) / self.fps_okno.elapsed().as_secs_f64();
            self.klatki = 0;
            self.fps_okno = Instant::now();
            self.tytul();
        }
    }

    /// Klatka dźwięku (M11d §5.9).
    ///
    /// Czas jest **realny**, a nie czasem gry: łoże dzielnicy i maszyny w hali mają grać
    /// przy pauzie, bo pauza zatrzymuje gospodarkę, a nie powietrze. Ta sama zasada,
    /// którą renderer stosuje do fal na wodzie.
    fn krok_dzwieku(&mut self, kamera: &magnat_render::CameraState, dt: f32) {
        let Some(audio) = self.audio.as_mut() else {
            return;
        };
        let sluchacz = crate::sound::sluchacz(kamera);
        let snap = self.snapshot.front();
        match self.terrain.as_ref() {
            Some(t) => audio.update(snap, &sluchacz, dt, &|pos| {
                crate::sound::okluzja(t, sluchacz.pos, pos)
            }),
            // Bez terenu (powłoka, podgląd) nie ma czym zasłaniać — i nie ma czego.
            None => audio.update(snap, &sluchacz, dt, &|_| 0.0),
        }
    }

    /// Składa i publikuje snapshot tej klatki (M11a §5.2).
    ///
    /// Trzy kroki i każdy ma jednego właściciela: klient ustawia okno warstwy Mikro
    /// (bo zna kamerę), wypełniacz składa rekordy z ECS (bo widzi ruch, mieszkańców
    /// i miasto naraz), a `publish` przestawia parę buforów. Render czyta **przedni**
    /// bufor i nie ma jak dosięgnąć tylnego.
    pub(crate) fn publikuj_snapshot(&mut self) {
        let GameState::Playing(s) = &self.game else {
            return;
        };
        // Kafle szyldów są własnością miasta, nie klatki — pożyczka kończy się przed
        // resztą publikacji, bo `zwiaz_szyldy` bierze `&mut self`.
        if self.szyldy_seed != Some(s.built.city.plan.seed) {
            let city = s.built.city.clone();
            self.zwiaz_szyldy(&city);
        }
        let GameState::Playing(s) = &self.game else {
            return;
        };
        let (Some(pal), Some(c)) = (self.palettes.as_ref(), self.citizens.as_ref()) else {
            return;
        };
        let oko = self.camera.eye();
        let cel = self.camera.target();
        let szerokosc = self.params.region.latitude_ddeg();
        c.okno_mikro(s, cel);

        let mm = |p: glam::DVec3| {
            [
                (p.x * 1000.0) as i32,
                (p.y * 1000.0) as i32,
                (p.z * 1000.0) as i32,
            ]
        };
        let oko_mm = mm(oko);
        // Kadr rozszerzony o promień rysowania: dokładny stożek widzenia liczy render,
        // bo to on zna macierze — snapshot ma tylko nie wozić drugiej połowy miasta.
        //
        // Wycinek idzie **za celem kamery**, a nie za okiem (`J-2`): przy orbicie z 900 m
        // oko stoi 767 m w poziomie od celu, więc prostopadłościan wokół oka był
        // przesunięty o tyle samo i połowa leżała za plecami kamery. Promień wynika
        // z rozmiaru ekranowego encji, bo stała 600 m była mniejsza od dystansu orbity.
        let promien = magnat_render::instancing::draw_distance_m(
            magnat_render::instancing::ENTITY_RADIUS_NOMINAL_M,
            self.camera.fov_deg,
            self.renderer
                .as_ref()
                .map_or(900.0, magnat_render::Renderer::viewport_height_px),
        );
        let zasieg = (promien * 1_200.0) as i32;
        let zapytanie = magnat_sim_snapshot::ViewQuery {
            aabb: magnat_sim_snapshot::Aabb::around(mm(cel), zasieg, zasieg),
            eye: oko_mm,
            caps: magnat_sim_snapshot::SnapshotCaps::DEFAULT,
            anim_ms: self.anim_ms,
        };

        self.filler.set_animations(!self.bez_animacji);
        self.filler.set_player(s.player().map(|p| p.citizen));
        self.filler.ensure_city(&s.built.city, pal);
        // Światło dzienne liczy **ta sama** funkcja, która ustawia słońce na niebie
        // (`engine/render::sky`). Druga kopia tej arytmetyki rozjechałaby się z pierwszą,
        // a objawem byłyby okna zapalające się w biały dzień.
        self.filler
            .ambience_mut()
            .set_daylight(magnat_render::daylight(self.minute, szerokosc));
        // Sprzężenie zwrotne przerzedzania latarni: przyrządem jest histogram zajętości
        // klastrów, a nie suma świateł (§5.8).
        self.filler
            .ambience_mut()
            .set_cluster_peak(self.occupancy_klatki);
        self.filler.ambience_mut().force_scene(
            self.wymus_opad,
            self.wymus_snieg,
            self.wymus_blackout,
        );
        // Scena `bench_winter` przewija cztery pory roku **w oknie pomiaru** (§7.3).
        // Bez tego sezon stoi razem ze światem i licznik remeshingu jest zerem
        // z konstrukcji, a nie dlatego, że pora roku nie dotyka geometrii.
        self.filler.ambience_mut().force_season(
            self.scena
                .as_ref()
                .filter(|p| p.scena.cykl_por_roku)
                .map(|p| scenes::pora_roku(p.zapisane, self.scena_klatek)),
        );
        let filtr = c.filtr();
        self.filler.fill_filtered(
            &s.app.world,
            &zapytanie,
            &|e| filtr.is_none_or(|f| f.accepts(s, e)),
            self.snapshot.back_mut(),
        );
        self.dosyp_tlum();
        self.snapshot.publish();
    }

    /// Scena pomiarowa `--crowd`: syntetyczni piesi wokół celu kamery.
    ///
    /// Wchodzą **po** wypełnieniu snapshotu i **nie istnieją w symulacji** — nie mają
    /// encji, nie chodzą do pracy i nie kupują chleba. To jest scena do pomiaru klatki,
    /// dokładnie tak samo jak `--lights`: kryteria WP3 i WP4 mówią o dwudziestu tysiącach
    /// postaci, a warstwa Mikro w oknie 900 m oddaje ich kilkadziesiąt.
    ///
    /// Rozstawienie jest siatką o kroku `--crowd-step` na wysokości terenu, z klipami i wariantami
    /// rozsuniętymi indeksem — tłum stojący w jednej pozie nie zmierzyłby ani kosztu
    /// animacji, ani przejść poziomu detalu.
    fn dosyp_tlum(&mut self) {
        if self.tlum == 0 {
            return;
        }
        let Some(terrain) = self.terrain.clone() else {
            return;
        };
        let cel = self.camera.target();
        let klipy = [
            magnat_voxel::ClipKind::Walk,
            magnat_voxel::ClipKind::Idle,
            magnat_voxel::ClipKind::Shop,
            magnat_voxel::ClipKind::Work(magnat_voxel::WorkStyle::Counter),
            magnat_voxel::ClipKind::Sit,
        ];
        let bok = (self.tlum as f64).sqrt().ceil() as i32;
        let anim_ms = self.anim_ms;
        let bez = self.bez_animacji;
        let klipy_id: Vec<u8> = klipy.iter().map(|k| self.klipy.id_of(*k).0).collect();
        let snap = self.snapshot.back_mut();
        for i in 0..self.tlum {
            let (kx, ky) = (i as i32 % bok, i as i32 / bok);
            let x = cel.x + f64::from(kx - bok / 2) * self.krok_tlumu;
            let y = cel.y + f64::from(ky - bok / 2) * self.krok_tlumu;
            let z = f64::from(terrain.height_at(x as i32, y as i32)) * 0.5;
            let entity = 1_000_000 + i as u32;
            let rec = magnat_sim_snapshot::CitizenRenderRec {
                pos: [
                    (x * 1000.0) as i32,
                    (y * 1000.0) as i32,
                    (z * 1000.0) as i32,
                ],
                entity_lo: entity,
                appearance: magnat_sim_snapshot::Appearance::derive(
                    0x4D41_474E_4154,
                    entity,
                    (i % 32) as u8,
                    (i % 4) as u8,
                    2,
                    0,
                )
                .0,
                yaw: (entity.wrapping_mul(2_654_435_761) >> 16) as u16,
                district: (i % 8) as u16,
                anim_state: if bez {
                    magnat_voxel::NO_CLIP.0
                } else {
                    klipy_id[i % klipy_id.len()]
                },
                anim_phase: magnat_voxel::anim_phase(anim_ms, entity),
                carry: 0,
                flags: 0,
                _pad: [0; 4],
            };
            if !snap.citizens.push(rec) {
                break;
            }
        }
    }

    /// Pasek tytułu okna: FPS, stan strumieniowania i doba. Diagnostyka, nie UI gracza —
    /// miasto o trzeciej w nocy wygląda tak samo jak zatrzymane.
    pub(crate) fn tytul(&mut self) {
        let (Some(w), Some(renderer)) = (self.window.as_ref(), self.renderer.as_ref()) else {
            return;
        };
        if self.terrain.is_none() {
            w.set_title("Magnat");
            return;
        }
        let s = renderer.stats();
        let cal = magnat_core::SimCalendar::from_minute(self.minute);
        let ludzie = match (self.citizens.as_ref(), self.game.session()) {
            (Some(c), Some(sesja)) => format!(
                " · {} mieszkańców, {} w warstwie Mikro",
                sesja.report.citizens,
                c.micro_len(sesja)
            ),
            _ => String::new(),
        };
        w.set_title(&format!(
            "Magnat — {:.0} FPS · {} chunków ({} rysowanych, {} tys. trójkątów) · \
             {:02}:{:02} dzień {} · {:.0} MB geometrii{}",
            self.fps,
            s.chunks_resident,
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
