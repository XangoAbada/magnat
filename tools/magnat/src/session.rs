//! Stan gry w kliencie: powłoka, generacja świata i wejście do miasta.
//!
//! Drugi temat pliku [`crate::app`] — i dlatego osobny plik. Tam jest **klatka**:
//! okno, kamera, render i wejście. Tutaj jest **droga do świata**: co się dzieje
//! między menu głównym a stojącym miastem.
//!
//! Granica przebiega tam, gdzie zwykle: rzeczy, które dzieją się co klatkę, zostają
//! w `app`; rzeczy, które dzieją się przy zmianie stanu gry, są tutaj. Zapis i odczyt
//! slotu mieszkają od M9e obok, w [`crate::slots`] — to jedyna grupa metod w tym
//! bloku, która opisywała plik na dysku, a nie przejście między stanami gry.
//!
//! `ponytail:` reszta zostaje jednym blokiem `impl`. Sufit nazwany: to jest jeden
//! temat i każda z tych metod jest wołana z `wykonaj`. Podział po rzeczach
//! („generacja", „kamera") dałby dwa pliki po sto linii i trzeci z dyspozytorem,
//! który i tak musi znać oba.

use crate::app::App;
use crate::citizens;
use crate::{
    overlay,
    preview::{mapa_dalekiego_terenu, swiatla_testowe},
    stream,
};
use magnat_game::screens::ShellAction;
use magnat_game::shell::{NewGameParams, ShellScreen, WorldGenJob, WorldPreview};
use magnat_game::{BuiltCity, GameState, Session, SessionOpts};
use magnat_render::CameraMode;
use magnat_voxel::EditIndex;
use magnat_world::TerrainQuery;
use std::sync::Arc;
use std::time::Instant;

/// Edytor reguł otwarty nad grającym światem (`M9d` WP8).
///
/// Trzyma **zakład**, bo polityka jest zawsze polityką czegoś, i **klucze towarów**,
/// bo postać tekstowa zapisuje towar kluczem, a nie indeksem (00 §5). Jedno i drugie
/// jest kopią zrobioną przy otwarciu: ekran nie ma prawa sięgać do świata w klatce.
pub(crate) struct Redaktor {
    pub(crate) site: magnat_core::SiteId,
    pub(crate) view: magnat_game::policy::RuleEditorView,
    pub(crate) goods: magnat_game::policy::GoodKeys,
}


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
                        // Gotowy zestaw warunków „zatrzymaj, gdy…" (§5.10) —
                        // uzbrojony od razu, bo gracz ma go **wyłączać**, a nie
                        // składać: warunek, który trzeba najpierw znaleźć, nie
                        // zatrzyma pierwszej katastrofy.
                        c.arm_stop_conditions();
                        c.start_tutorial(&session);
                        self.citizens = Some(c);
                        self.shell.has_session = true;
                        self.pauza_menu = false;
                        // Ostatni ekran przed grą: kim chcesz być (WP4). Kandydaci
                        // powstają z **postawionego** świata, więc dopiero tutaj.
                        if self.shell.observe {
                            // Tryb przeglądu: świat bez postaci i bez ekranu wyboru.
                            // Flagę ustawia albo pozycja „Tryb przeglądu" w menu
                            // głównym, albo `--observe` — jedna i ta sama, żeby
                            // zrzut, pomiar i gracz szli tą samą drogą.
                            self.game = GameState::Playing(Box::new(session));
                        } else {
                            self.shell.candidates = magnat_game::player::candidates(
                                &session.app.world,
                                self.shell.draft.variant,
                                session.tick().get() / 1440,
                            );
                            self.shell.go(ShellScreen::CharacterSelect);
                            self.game = GameState::CharacterSelect(Box::new(session));
                        }
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
        let redaktor = &mut self.redaktor;
        let mut akcja_edytora = magnat_game::policy::EditorAction::None;
        let mut akcja_panelu = magnat_game::PanelAction::None;
        let mut akcja_konca: Option<magnat_game::screens::ending::EndAction> = None;
        // Esc nad grającym światem otwiera pauzę — i robi to **po stronie `egui`**,
        // tak samo jak Esc na każdym ekranie powłoki. Czytamy go tutaj, a nie w pętli
        // okna, bo inaczej oba miejsca widzą to samo naciśnięcie w tej samej klatce:
        // pętla otwierała pauzę, a narysowane zaraz potem menu pauzy widziało ten sam
        // klawisz i wracało do gry (`DE-13`).
        let mut otworz_pauze = false;

        let out = ctx.clone().run_ui(wejscie, |ui| {
            if w_powloce {
                akcja = match (&postep, gra) {
                    (Some(p), _) => shell.draw_generating(ui, p),
                    (None, GameState::WorldReady { preview, .. }) => {
                        shell.draw_preview(ui, preview, tex.as_ref())
                    }
                    // Ekrany domknięcia stoją poza powłoką, bo za nimi jest sesja:
                    // świat tyka dalej, a gracz decyduje, kto go poprowadzi.
                    (None, GameState::Succession { session, heir }) => {
                        akcja_konca = magnat_game::screens::ending::succession(
                            shell, ui, session, *heir,
                        );
                        None
                    }
                    (None, GameState::ScenarioEnd { session, outcome }) => {
                        akcja_konca = magnat_game::screens::ending::scenario_end(
                            shell, ui, session, *outcome,
                        );
                        None
                    }
                    _ => shell.draw(ui),
                };
            } else if let Some(r) = redaktor.as_mut() {
                akcja_edytora =
                    r.view
                        .draw(ui, &shell.theme, &shell.catalog, shell.settings.locale, &r.goods);
            } else if let (Some(c), Some(s)) = (citizens.as_mut(), gra.session()) {
                let (p, a) = c.draw(ui, &shell.theme, s);
                predkosc = p;
                akcja_panelu = a;
                // Nad edytorem reguł Esc zamyka edytor (gałąź wyżej), więc tutaj
                // nie dochodzi — jedno naciśnięcie, jeden skutek.
                otworz_pauze = ui.input(|i| i.key_pressed(egui::Key::Escape));
            }
        });
        if let (Some(st), Some(w)) = (self.egui_state.as_mut(), self.window.as_ref()) {
            st.handle_platform_output(w, out.platform_output.clone());
        }
        if otworz_pauze {
            self.pauza();
        }
        if let Some(a) = akcja {
            self.wykonaj(a);
        }
        if let Some(a) = akcja_konca {
            self.shell.focus = 0;
            if !self.game.apply_end(a) {
                self.opusc_swiat();
            }
        }
        self.wykonaj_edytor(akcja_edytora);
        self.wykonaj_panel(akcja_panelu);
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
                    std::mem::replace(&mut self.game, GameState::Shell)
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
            ShellAction::Observe => self.wejdz_bez_postaci(),
            // Do klienta nie ma prawa dojść: `Shell::draw` rozwiązuje cofnięcie
            // do konkretnego celu, a ekran generacji i podgląd — jedyne spoza
            // `ShellScreen` — zgłaszają od razu `CancelGeneration` i `BackToWizard`.
            // Gdyby któryś z nich zaczął kiedyś zwracać `Back`, przycisk „Wstecz"
            // przestałby działać bez jednego słowa — stąd asercja, a nie cisza.
            ShellAction::Back => debug_assert!(
                false,
                "ShellAction::Back doszło do klienta — ekran spoza ShellScreen zwrócił cofnięcie, którego nikt nie rozwiązał"
            ),
        }
    }

    /// Tryb przeglądu: świat rusza **bez postaci gracza**.
    ///
    /// Nie jest to wariant startu, tylko jego brak: `Session::player` zostaje `None`,
    /// a dziennik wejść nie dostaje `SetCharacter`. Dzięki temu replay odtwarza tryb
    /// przeglądu **z samej nieobecności komendy** i nie trzeba go nigdzie zapisywać.
    ///
    /// Co z tego wynika dla gracza: klika, ogląda karty i nakładki, przewija czas —
    /// ale nie ma czym wydać komendy dotyczącej postaci (`precheck` odpowiada
    /// `NoCharacter`), a majątek w wierszu slotu jest zerem, bo nie ma czyjego liczyć.
    pub(crate) fn wejdz_bez_postaci(&mut self) {
        let GameState::CharacterSelect(session) =
            std::mem::replace(&mut self.game, GameState::Shell)
        else {
            return;
        };
        self.shell.candidates = Vec::new();
        self.game = GameState::Playing(session);
        self.pauza_menu = false;
    }

    /// Wybór postaci: komenda do dziennika, a potem świat rusza.
    ///
    /// `None` znaczy „wylosuj" — losowanie jest funkcją ziarna świata, nie zegara,
    /// więc replay odtworzy je z tej samej koperty. Świat bez kandydata wchodzi do gry
    /// **bez postaci**: to jest stan, w którym gracz ogląda miasto, a nie błąd.
    pub(crate) fn wybierz_postac(&mut self, kto: Option<magnat_core::CitizenId>) {
        let GameState::CharacterSelect(mut session) =
            std::mem::replace(&mut self.game, GameState::Shell)
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

    /// Wraca do kreatora — z podglądu, z anulowanej generacji albo z ekranu postaci.
    ///
    /// **Porzuca świat, jeśli jakiś stoi.** Do M9e tego nie robiło i po cofnięciu
    /// z podglądu w kliencie zostawały teren, strumieniowanie i miasto poprzedniego
    /// przebiegu — niewidoczne, bo kreator ich nie rysuje, ale wciąż w pamięci.
    pub(crate) fn do_kreatora(&mut self) {
        self.porzuc_swiat();
        let draft = self.shell.draft;
        self.game = GameState::Shell;
        self.shell.go(ShellScreen::NewGame { draft });
    }

    /// Porzuca świat i wraca do menu głównego. Render zostaje bez sceny — to jest
    /// dokładnie ten stan, w którym gra startuje bez argumentów.
    pub(crate) fn opusc_swiat(&mut self) {
        self.porzuc_swiat();
        self.game = GameState::Shell;
        self.shell.go(ShellScreen::MainMenu);
    }

    /// Zwalnia wszystko, co wisi na postawionym świecie. Sam `GameState` zostaje
    /// bez zmian — ustawia go wołający, bo to on wie, dokąd gracz idzie.
    fn porzuc_swiat(&mut self) {
        self.citizens = None;
        self.streamer = None;
        self.terrain = None;
        self.city = None;
        self.edits = Arc::new(EditIndex::default());
        self.podglad_tex = None;
        self.pauza_menu = false;
        self.shell.has_session = false;
        self.shell.candidates = Vec::new();
        // Tryb przeglądu należy do **tej** rozgrywki, nie do profilu gracza:
        // porzucony świat zabiera flagę ze sobą. Dziś nikt jej po drodze nie czyta,
        // ale niezmiennika „w menu głównym flaga jest zgaszona" pilnowałby
        // przypadek, a nie kod.
        self.shell.observe = false;
    }

    /// Odbiera skończoną generację i przechodzi do podglądu świata.
    pub(crate) fn odbierz_generacje(&mut self) {
        if !matches!(&self.game, GameState::Generating(j) if j.is_finished()) {
            return;
        }
        let GameState::Generating(job) =
            std::mem::replace(&mut self.game, GameState::Shell)
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

// ── edytor reguł (`M9d` WP8) ────────────────────────────────────────────────────
//
// Osobny blok `impl`, bo to jest inny temat niż przejścia między stanami gry:
// świat pod edytorem **tyka dalej**, a ekran nad nim jest stanem klienta, nie gry.
// Podział jest zarazem odpowiedzią na próg strukturalny (CLAUDE.md).

impl App {
    /// Otwiera edytor reguł dla zaznaczonego zakładu.
    ///
    /// Punkt wyjścia to polityka, która na tym zakładzie **już stoi**; zakład bez
    /// polityki dostaje preset „Kurs stały" z `data/policies/`, czyli ten sam,
    /// od którego zaczyna firma AI. Pusty formularz byłby uczciwy i bezużyteczny —
    /// gracz uczy się języka, patrząc na regułę, która działa.
    pub(crate) fn otworz_edytor(&mut self) {
        if self.redaktor.is_some() {
            self.redaktor = None;
            return;
        }
        let Some(site) = self.citizens.as_ref().and_then(citizens::Citizens::wybrany_zaklad) else {
            eprintln!("edytor reguł: najpierw kliknij w zakład");
            return;
        };
        let Some(session) = self.game.session() else {
            return;
        };
        let Some(market) = session.market.as_ref() else {
            eprintln!("edytor reguł: gospodarka jest wyłączona");
            return;
        };
        let Some((edytor, polityka)) = magnat_game::policy::open_for(session, site) else {
            eprintln!("edytor reguł: tej polityki formularz nie umie pokazać");
            return;
        };
        let mut view = magnat_game::policy::RuleEditorView::new(edytor);
        view.set_dry(&market.dry_run(site, &polityka));
        self.redaktor = Some(Redaktor {
            site,
            view,
            goods: magnat_game::policy::good_keys(market),
        });
    }

    /// Wykonuje to, o co poprosił edytor reguł.
    fn wykonaj_edytor(&mut self, a: magnat_game::policy::EditorAction) {
        use magnat_game::policy::EditorAction as A;
        match a {
            A::None => {}
            A::Close => self.redaktor = None,
            A::Attach => {
                let Some(r) = self.redaktor.take() else { return };
                let polityka = r.view.editor.policy();
                if let Some(s) = self.game.session_mut() {
                    if let Err(e) = s.submit(magnat_game::PlayerCommand::AttachPolicy {
                        site: r.site,
                        policy: Box::new(polityka),
                    }) {
                        eprintln!("polityka odrzucona: {e}");
                    }
                }
            }
        }
    }
}
