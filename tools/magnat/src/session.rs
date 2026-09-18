//! Stan gry w kliencie: powłoka, generacja świata, wejście do miasta i sloty zapisu.
//!
//! Drugi temat pliku [`crate::app`] — i dlatego osobny plik. Tam jest **klatka**:
//! okno, kamera, render i wejście. Tutaj jest **droga do świata**: co się dzieje
//! między menu głównym a stojącym miastem, i co robi „Zapisz" oraz „Wczytaj".
//!
//! Granica przebiega tam, gdzie zwykle: rzeczy, które dzieją się co klatkę, zostają
//! w `app`; rzeczy, które dzieją się przy zmianie stanu gry, są tutaj.
//!
//! `ponytail:` blok `impl` ma ~420 linii i zostaje jednym blokiem. Sufit nazwany:
//! to jest jeden temat — przejścia między stanami gry — i każda z tych metod jest
//! wołana z `wykonaj`. Podział po rzeczach („generacja", „zapisy") dałby dwa pliki
//! po sto linii i trzeci z dyspozytorem, który i tak musi znać oba.

use crate::app::App;
use crate::citizens;
use crate::{
    overlay,
    preview::{mapa_dalekiego_terenu, swiatla_testowe},
    stream,
};
use magnat_core::SimMinute;
use magnat_game::screens::ShellAction;
use magnat_game::shell::{NewGameParams, ShellScreen, WorldGenJob, WorldPreview};
use magnat_game::{BuiltCity, GameState, Session, SessionOpts};
use magnat_render::CameraMode;
use magnat_voxel::EditIndex;
use magnat_world::TerrainQuery;
use std::sync::Arc;
use std::time::Instant;

impl App {
    /// Wpina postawione miasto do renderu i zaczyna grę (etap B: zaludnienie).
    ///
    /// **To jest jedyna droga do grającego świata w kliencie.** Wiersz poleceń dochodzi
    /// tu z gotowym miastem, powłoka — po „Gram tutaj"; jedno i drugie woła
    /// `game::Session::begin`, czyli tę samą funkcję co przebieg bezgłowy (`K-68`).
    pub(crate) fn wejdz_do_swiata(&mut self, built: BuiltCity) {
        let centrum = built.city.center;
        self.terrain = Some(built.terrain.clone());
        self.edits = Arc::new(built.city.edits.clone());
        self.city = Some(built.city.clone());
        if self.cel.is_none() {
            self.cel = Some((centrum.x as i32, centrum.y as i32));
        }
        self.ustaw_kamere_startowa();
        self.wpnij_render();

        if self.bez_ludzi {
            // Miasto bez ludzi: klient M2 działał tak i ma dalej działać. Sesji nie ma,
            // więc nie ma też zegara gry — słońce chodzi po starym mnożniku M1.
            self.game = GameState::WorldReady {
                preview: Box::new(WorldPreview::of(&built)),
                built: Box::new(built),
            };
            self.pauza_menu = false;
            return;
        }

        let start = Instant::now();
        eprintln!("Etap 8: zaludnianie miasta");
        let params = NewGameParams {
            world: self.params,
            scenario: self.shell.draft.scenario,
            variant: self.shell.draft.variant,
            opts: SessionOpts {
                citizens: 0,
                commute_swaps: citizens::SWAPS,
                economy: !self.bez_gospodarki,
                // W oknie warstwa Mikro jest zawsze: bez niej nie ma czego rysować.
                micro: true,
            },
        };
        let pula = magnat_jobs::JobPool::new(self.watki);
        match Session::begin(built, params, &pula) {
            Ok(mut session) => {
                let r = session.report;
                eprintln!(
                    "świat gotowy w {:.1} s: {} mieszkańców w {} gospodarstwach, {} sklepów, \
                     {} zakładów, {} firm, {} sieci",
                    start.elapsed().as_secs_f64(),
                    r.citizens,
                    r.households,
                    r.shops,
                    r.plants,
                    r.firms,
                    r.grids
                );
                eprintln!(
                    "harmonogram: {} systemów w {} etapach, odcisk {:#018x}",
                    r.systems, r.stages, r.schedule_fingerprint
                );
                match citizens::Citizens::new(self.params.seed, self.shell.settings.locale) {
                    Ok(mut c) => {
                        // `--hour`: symulacja zawsze startuje o północy doby zerowej, bo
                        // `bootstrap_day` zasiewa kolejkę od minuty zero. Godzinę osiąga
                        // się przewinięciem, nie przestawieniem zegara — przestawiony
                        // zegar zostawiłby zdarzenia w przeszłości.
                        c.warm_up(&mut session, self.godzina_startu, self.camera.eye());
                        c.set_speed(self.predkosc);
                        self.citizens = Some(c);
                        self.shell.has_session = true;
                        self.pauza_menu = false;
                        // Ostatni ekran przed grą: kim chcesz być (WP4). Kandydaci
                        // powstają z **postawionego** świata, więc dopiero tutaj.
                        self.shell.candidates = magnat_game::player::candidates(
                            &session.app.world,
                            self.shell.draft.variant,
                            session.tick().get() / 1440,
                        );
                        self.shell.go(ShellScreen::CharacterSelect);
                        self.game = GameState::CharacterSelect(Box::new(session));
                    }
                    Err(e) => eprintln!("interfejs rozgrywki nieudany: {e}"),
                }
            }
            // Brak ludzi nie jest powodem, żeby nie pokazać miasta.
            Err(e) => eprintln!("Etap 8 nieudany, miasto zostaje puste: {e}"),
        }
    }

    /// Kamera nad miastem i światła testowe — po wpięciu terenu.
    pub(crate) fn ustaw_kamere_startowa(&mut self) {
        let Some(terrain) = self.terrain.clone() else {
            return;
        };
        let (tx, ty) = self.cel.unwrap_or_else(|| {
            let s = terrain.size_m() / 2;
            (s, s)
        });
        let h = f64::from(terrain.height_at(tx, ty)) * 0.5;
        if let CameraMode::Orbit { target, .. } = &mut self.camera.mode {
            *target = glam::DVec3::new(f64::from(tx), f64::from(ty), h);
        }
        if !self.swiatla.is_empty() {
            let cel = self.camera.target();
            self.swiatla = swiatla_testowe(self.swiatla.len(), &terrain, cel);
        }
    }

    /// Daleki teren, strumieniowanie i nakładka startowa.
    pub(crate) fn wpnij_render(&mut self) {
        let (Some(terrain), Some(renderer)) = (self.terrain.clone(), self.renderer.as_mut()) else {
            return;
        };
        let (dim, wysokosci, kolor) = mapa_dalekiego_terenu(&terrain);
        renderer.upload_far_terrain(dim, magnat_world::WORK_CELL_M as f32, &wysokosci, &kolor);
        if self.nakladka != overlay::Nakladka::Brak {
            if let Some(p) = overlay::zbuduj(&terrain, self.city.as_deref(), self.nakladka) {
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
        let mut streamer =
            stream::Streamer::new(terrain, self.materials.clone(), self.edits.clone());
        if let Some(r) = self.lod0_radius {
            streamer.wymus_lod0(r);
        }
        self.streamer = Some(streamer);
    }

    /// Czy w tej klatce rysujemy ekran powłoki zamiast rozgrywki.
    pub(crate) fn w_powloce(&self) -> bool {
        self.pauza_menu || !self.game.is_playing()
    }

    /// Buduje klatkę interfejsu: ekran powłoki albo panele rozgrywki.
    pub(crate) fn buduj_ui(&mut self) -> Option<(egui::FullOutput, Option<magnat_core::SimSpeed>)> {
        let (ctx, window) = (self.egui_ctx.clone()?, self.window.clone()?);
        let wejscie = self.egui_state.as_mut()?.take_egui_input(&window);
        self.shell.prepare(&ctx);

        let mut akcja = None;
        let mut predkosc = None;
        let w_powloce = self.w_powloce();
        // Domknięcie nie może pożyczyć `self` w całości, więc to, czego potrzebuje,
        // wyjmujemy przed nim — tak samo jak przy kartach paneli.
        let postep = match &self.game {
            GameState::Generating(job) => Some(job.progress().clone()),
            _ => None,
        };
        let tex = self.podglad_tex.clone();
        let shell = &mut self.shell;
        let citizens = &mut self.citizens;
        let gra = &self.game;

        let out = ctx.clone().run_ui(wejscie, |ui| {
            if w_powloce {
                akcja = match (&postep, gra) {
                    (Some(p), _) => shell.draw_generating(ui, p),
                    (None, GameState::WorldReady { preview, .. }) => {
                        shell.draw_preview(ui, preview, tex.as_ref())
                    }
                    _ => shell.draw(ui),
                };
            } else if let (Some(c), Some(s)) = (citizens.as_mut(), gra.session()) {
                predkosc = c.draw(ui, &shell.theme, s);
            }
        });
        if let (Some(st), Some(w)) = (self.egui_state.as_mut(), self.window.as_ref()) {
            st.handle_platform_output(w, out.platform_output.clone());
        }
        if let Some(a) = akcja {
            self.wykonaj(a);
        }
        Some((out, predkosc))
    }

    /// Wykonuje to, o co poprosił ekran powłoki.
    pub(crate) fn wykonaj(&mut self, akcja: ShellAction) {
        match akcja {
            ShellAction::Generate(params) => {
                self.shell.draft = params;
                self.params = params.world;
                self.podglad_tex = None;
                self.game = GameState::Generating(WorldGenJob::start(params.world, self.watki));
            }
            ShellAction::CancelGeneration => {
                // `WorldGenJob` anuluje się w `Drop` i czeka na wątek — pięćdziesiąt
                // anulowań w pętli ma dawać stałe zużycie pamięci (M9 §7).
                self.do_kreatora();
            }
            ShellAction::PlayHere => {
                if let GameState::WorldReady { built, .. } =
                    std::mem::replace(&mut self.game, GameState::Shell(ShellScreen::MainMenu))
                {
                    self.wejdz_do_swiata(*built);
                }
            }
            ShellAction::Reroll => {
                let mut p = self.shell.draft;
                p.world.seed = magnat_core::mix64(p.world.seed ^ 0x9E37_79B9_7F4A_7C15);
                self.shell.draft = p;
                self.podglad_tex = None;
                self.game = GameState::Generating(WorldGenJob::start(p.world, self.watki));
            }
            ShellAction::BackToWizard => self.do_kreatora(),
            ShellAction::Resume => {
                self.pauza_menu = false;
                if let Some(c) = &mut self.citizens {
                    c.set_speed(self.predkosc);
                }
            }
            ShellAction::ToMenu => self.opusc_swiat(),
            ShellAction::Quit => self.koniec = true,
            ShellAction::SettingsChanged => {
                if let Some(c) = &mut self.citizens {
                    c.set_locale(self.shell.settings.locale);
                }
                if let Err(e) = self.shell.settings.save(&self.zapisy.join("settings.ron")) {
                    eprintln!("nie udało się zapisać ustawień: {e}");
                }
            }
            ShellAction::SaveSlot(id) => self.zapisz(id),
            ShellAction::LoadSlot(id) => self.wczytaj(id),
            ShellAction::PickCitizen(c) => self.wybierz_postac(Some(c)),
            ShellAction::PickRandomCitizen => self.wybierz_postac(None),
        }
    }

    /// Wybór postaci: komenda do dziennika, a potem świat rusza.
    ///
    /// `None` znaczy „wylosuj" — losowanie jest funkcją ziarna świata, nie zegara,
    /// więc replay odtworzy je z tej samej koperty. Świat bez kandydata wchodzi do gry
    /// **bez postaci**: to jest stan, w którym gracz ogląda miasto, a nie błąd.
    pub(crate) fn wybierz_postac(&mut self, kto: Option<magnat_core::CitizenId>) {
        let GameState::CharacterSelect(mut session) =
            std::mem::replace(&mut self.game, GameState::Shell(ShellScreen::MainMenu))
        else {
            return;
        };
        let wybor = kto
            .or_else(|| magnat_game::player::pick_random(&self.shell.candidates, self.params.seed));
        if let Some(c) = wybor {
            if let Err(e) = session.submit(magnat_game::PlayerCommand::SetCharacter { citizen: c })
            {
                eprintln!("wybór postaci odrzucony: {e}");
            }
            // Komenda stosuje się na najbliższym ticku, więc jeden krok — inaczej
            // gracz stałby na ekranie, którego skutku nie widać.
            session.step(1, 0);
            if let Some(cz) = &mut self.citizens {
                cz.idz_do(magnat_core::Subject::Citizen(c));
            }
        }
        self.shell.candidates = Vec::new();
        self.game = GameState::Playing(session);
        self.pauza_menu = false;
    }

    pub(crate) fn do_kreatora(&mut self) {
        let draft = self.shell.draft;
        self.game = GameState::Shell(ShellScreen::NewGame { draft });
        self.shell.go(ShellScreen::NewGame { draft });
    }

    /// Porzuca świat i wraca do menu głównego. Render zostaje bez sceny — to jest
    /// dokładnie ten stan, w którym gra startuje bez argumentów.
    pub(crate) fn opusc_swiat(&mut self) {
        self.game = GameState::Shell(ShellScreen::MainMenu);
        self.citizens = None;
        self.streamer = None;
        self.terrain = None;
        self.city = None;
        self.edits = Arc::new(EditIndex::default());
        self.pauza_menu = false;
        self.shell.has_session = false;
        self.shell.go(ShellScreen::MainMenu);
    }

    /// Zapis slotu: nagłówek plus dziennik wejść (`DA-7`).
    ///
    /// Majątek w wierszu slotu to majątek gospodarstwa gracza (`M9c` WP4); świat bez
    /// wybranej postaci zapisuje zero i to jest prawda, a nie zaślepka.
    /// Nazwa „miasta" to nazwa pierwszej dzielnicy — własnej nazwy miasto nie ma.
    pub(crate) fn zapisz(&mut self, id: u8) {
        let Some(session) = self.game.session() else {
            return;
        };
        let miasto = nazwa_miasta(session, self.params.seed);
        // Majątek gracza to majątek jego gospodarstwa — jedna liczba, ta sama, którą
        // pokazuje karta. Bez postaci zostaje zero i to jest prawda, a nie zaślepka.
        let majatek = session
            .player()
            .and_then(|p| {
                session
                    .app
                    .world
                    .get::<magnat_agents::Household>(p.household.entity())
                    .map(magnat_game::inspect::household_worth)
            })
            .unwrap_or(magnat_core::Money::ZERO);
        let slot = magnat_game::SaveSlot {
            id,
            city: miasto,
            game_date: SimMinute(session.tick().get()),
            net_worth: majatek,
            played_secs: u32::try_from(session.played_ms() / 1000).unwrap_or(u32::MAX),
            world: self.params,
            schema_version: magnat_game::SAVE_SCHEMA_VERSION,
            saved_at_wall: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        };
        match magnat_game::save::write_slot(&self.zapisy, &slot, session.log()) {
            Ok(()) => eprintln!("zapisano slot {id}"),
            Err(e) => eprintln!("zapis slotu {id} nieudany: {e}"),
        }
        self.shell.refresh_slots(&self.zapisy);
        self.pauza_menu = true;
        self.shell.go(ShellScreen::Pause);
    }

    /// Wczytanie slotu: przewinięcie dziennika wejść.
    ///
    /// Kosztuje tyle, ile kosztowała rozgrywka (`DB-3`) i dlatego blokuje klatkę —
    /// ekran mówi to wprost, zamiast udawać, że odtworzenie roku gry jest darmowe.
    pub(crate) fn wczytaj(&mut self, id: u8) {
        let log = match magnat_game::save::read_log(&self.zapisy, id) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("slot {id}: {e}");
                return;
            }
        };
        let pula = magnat_jobs::JobPool::new(self.watki);
        let ticki = log.commands.last().map_or(0, |e| e.tick.get());
        eprintln!("wczytuję slot {id}: odtwarzam {ticki} minut gry");
        match magnat_game::replay_session(&log, ticki, 0, &pula) {
            Ok((mut session, _)) => {
                self.opusc_swiat();
                self.params = log.header.params.world;
                self.shell.draft = log.header.params;
                self.terrain = Some(session.built.terrain.clone());
                self.edits = Arc::new(session.built.city.edits.clone());
                self.city = Some(session.built.city.clone());
                let c = session.built.city.center;
                self.cel = Some((c.x as i32, c.y as i32));
                self.ustaw_kamere_startowa();
                self.wpnij_render();
                match citizens::Citizens::new(self.params.seed, self.shell.settings.locale) {
                    Ok(mut ui) => {
                        ui.warm_up(&mut session, 0, self.camera.eye());
                        ui.set_speed(self.predkosc);
                        self.citizens = Some(ui);
                        self.game = GameState::Playing(Box::new(session));
                        self.shell.has_session = true;
                    }
                    Err(e) => eprintln!("interfejs rozgrywki nieudany: {e}"),
                }
            }
            Err(e) => eprintln!("slot {id}: nie udało się odtworzyć sesji: {e}"),
        }
    }

    /// Odbiera skończoną generację i przechodzi do podglądu świata.
    pub(crate) fn odbierz_generacje(&mut self) {
        if !matches!(&self.game, GameState::Generating(j) if j.is_finished()) {
            return;
        }
        let GameState::Generating(job) =
            std::mem::replace(&mut self.game, GameState::Shell(ShellScreen::MainMenu))
        else {
            return;
        };
        match job.join() {
            Ok(Some(built)) => {
                let preview = WorldPreview::of(&built);
                self.podglad_tex = self.wgraj_podglad(&preview);
                self.game = GameState::WorldReady {
                    preview: Box::new(preview),
                    built: Box::new(built),
                };
                self.shell.focus = 0;
            }
            // Anulowanie wraca do kreatora — to nie jest błąd, tylko decyzja gracza.
            Ok(None) => self.do_kreatora(),
            Err(e) => {
                eprintln!("generacja świata nieudana: {e}");
                self.do_kreatora();
            }
        }
    }

    /// Miniatura podglądu jako tekstura `egui`. Ta sama mapa co w podglądzie
    /// `headless` — kolor z klasy wody i wysokości, nie drugi renderer (§5.14 pkt 3).
    pub(crate) fn wgraj_podglad(&self, p: &WorldPreview) -> Option<egui::TextureHandle> {
        let ctx = self.egui_ctx.as_ref()?;
        let bok = magnat_game::shell::PREVIEW_PX;
        let piksele: Vec<egui::Color32> = p
            .map
            .chunks_exact(4)
            .map(|c| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
            .collect();
        let obraz = egui::ColorImage {
            size: [bok, bok],
            pixels: piksele,
            source_size: egui::vec2(bok as f32, bok as f32),
        };
        Some(ctx.load_texture("magnat.podglad", obraz, egui::TextureOptions::LINEAR))
    }
}

/// Nazwa dla wiersza slotu. Miasto nie ma własnej nazwy — bierzemy nazwę pierwszej
/// dzielnicy, bo to jedyna prawdziwa nazwa, jaką ten świat niesie.
fn nazwa_miasta(session: &Session, seed: u64) -> String {
    session
        .built
        .city
        .districts
        .districts
        .first()
        .map_or_else(|| format!("{seed:#x}"), |d| d.name.clone())
}
