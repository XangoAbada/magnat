//! Mieszkańcy w oknie — Etap 8, systemy doby i panel inspekcji (M3d, WP11 i WP12).
//!
//! To jest miejsce, w którym klient przestaje być przeglądarką krajobrazu i zaczyna być
//! grą: miasto z M2 dostaje ludzi (Etap 8), doba biegnie przez systemy §5.12, piesi są
//! rysowani, a kliknięcie w pieszego otwiera jego kartę.
//!
//! ### Co jest tu, a czego tu nie ma
//!
//! Tu jest **spięcie**, a nie logika. Populację liczy `sim/world::population`, dobę
//! przewijają systemy z `sim/agents`, treść karty buduje `magnat-ui`, a rysuje ją
//! `engine/render::ui`. Gdyby którakolwiek z tych rzeczy zaczęła się liczyć w tym pliku,
//! przestałaby być sprawdzalna bez okna — a headless-first (00 §6) mówi dokładnie odwrotnie.
//!
//! ### Prędkość gry nie zmienia wyniku
//!
//! Zegar zamienia czas realny na **liczbę minut gry do wykonania**, a każda minuta jest
//! tym samym tickiem (§5.11). Klatka o dowolnej długości przy dowolnej prędkości daje ten
//! sam ciąg ticków — a to jest ta sama ścieżka, którą chodzi `headless m3day --speed`,
//! czyli ta, na której stoi test `det_speed_invariance`.
//!
//! ### Warstwa Mikro chodzi tylko w kadrze
//!
//! Polilinia trasy dla 274 tys. mieszkańców to koszt, którego nikt nie ogląda. Oracle ruchu
//! dostaje **okno**: środek kadru i promień. Mezo — czyli cały wynik — nie zależy od tego
//! ani o minutę, bo warstwa Mikro nie ma prawa zapisu do stanu (00 §4) i `AgentSources`
//! świadomie nie wchodzi do hasha stanu (bramka 2 fazy M3).

use magnat_agents::{
    bootstrap_day, register, register_day, society, AgentSources, DayLoopSystem, DemographyTable,
    DeprivationEffectsSystem, HouseholdStockSystem, InfinitePlaces, NeedDecaySystem, NeedTable,
    NoInheritance, Population, ReplanCooldownSystem, SkillDriftSystem, SocietySystem, Trace,
    TravelMicroSystem,
};
use magnat_core::{SimSpeed, Tick};
use magnat_ecs::{App, ScheduleBuilder, World};
use magnat_traffic::{TrafficSystem, VehicleWearSystem};
use magnat_sim_snapshot::PedestrianRecord;
use magnat_ui::{CitizenPanel, Locale, Selection, UiContext};
use magnat_world::{generate_population, CityData, PopulationParams};
use std::sync::Arc;
use winit::window::Window;

/// Promień okna warstwy Mikro w metrach.
///
/// Większy od promienia rysowania z `render::pick` (600 m), bo pieszy musi wejść w warstwę
/// **zanim** wjedzie w kadr — inaczej pojawiałby się na środku ulicy w chwili, gdy kamera
/// go dosięga. Zapas jest jedną minutą marszu z okładem.
const MICRO_RADIUS_M: u32 = 900;

/// Ile prób poprawkowych mediany dojazdu wykonuje Etap 8 w kliencie.
///
/// ponytail: mniej niż w `headless population` (200 tys.), bo klient startuje przy każdym
/// uruchomieniu, a pętla poprawkowa to sekundy. Kryterium `gen_commute_hist` sprawdza
/// headless; okno ma pokazać miasto, a nie zdać test statystyczny.
const SWAPS: u32 = 50_000;

pub struct Citizens {
    app: App,
    ui: UiContext,
    panel: CitizenPanel,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    /// Rekordy dla renderera, przepisywane co klatkę z warstwy Mikro.
    peds: Vec<PedestrianRecord>,
    /// Bufor pośredni `TravelOracle::micro_snapshot`, żeby klatka nie alokowała.
    zrzut: Vec<(u32, [f32; 3], f32)>,
    /// Doba, dla której karta odtwarza plan.
    dzien: u64,
    /// Czy panel jest widoczny. Karta bez zaznaczenia pokazuje komunikat, więc panel
    /// da się otworzyć, zanim gracz w kogokolwiek kliknie.
    pokaz_karte: bool,
    ludzi: usize,
}

impl Citizens {
    /// Zaludnia miasto i stawia harmonogram. Zwraca też, ile to trwało — start
    /// metropolii to kilkadziesiąt sekund i gracz ma prawo wiedzieć, na co czeka.
    pub fn new(
        seed: u64,
        city: &CityData,
        window: &Window,
        locale: Locale,
        threads: usize,
    ) -> Result<Citizens, Box<dyn std::error::Error>> {
        let mut world = World::new(seed);
        register(&mut world, NeedTable::load_default()?);
        society::register_society(&mut world, DemographyTable::load_default()?);
        register_day(&mut world);

        let zaludnione = generate_population(
            &mut world,
            city,
            &PopulationParams {
                target_population: None,
                unemployment_target_permille: None,
                commute_median_min: None,
                commute_swaps: Some(SWAPS),
            },
        )?;
        for l in zaludnione.report.lines() {
            eprintln!("{l}");
        }

        let tabela = Arc::new(NeedTable::load_default()?);
        let oracle = zaludnione.travel_oracle();
        oracle.set_micro_window(None, 0);
        *world.resource_mut::<AgentSources>() = AgentSources::new(
            Box::new(InfinitePlaces::new(zaludnione.places.clone(), tabela)),
            oracle,
        );
        let zasiane = bootstrap_day(&mut world, 0);
        eprintln!("kolejka zasiana: {zasiane} mieszkańców");

        let mut builder = ScheduleBuilder::new();
        builder
            .add(DayLoopSystem::new(&world))
            .add(ReplanCooldownSystem::new(&world))
            .add(NeedDecaySystem::new(&world))
            .add(DeprivationEffectsSystem::new(&world))
            .add(SkillDriftSystem::new(&world))
            .add(HouseholdStockSystem::new(&world))
            .add(SocietySystem::new(Box::new(NoInheritance)))
            // Warstwa mezo musi tu być, i to nie dla widoku. Zlecenie przejazdu
            // zebrane przez `begin_trip` wykonuje **tylko** ten system; bez niego
            // kierowca zgłasza podróż, której nikt nie realizuje, nie dostaje
            // `Arrive` i stoi do końca sesji — a jego auto zostaje zajęte na zawsze.
            .add(TrafficSystem::new(&world))
            .add(VehicleWearSystem::new(&world))
            // W oknie warstwa Mikro jest zawsze: bez niej nie ma czego rysować.
            .add(TravelMicroSystem::new(&world));
        let schedule = builder.build()?;
        let ludzi = society::population(&world);
        let app = App::new(world, schedule, threads);

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        Ok(Citizens {
            ui: UiContext::new(locale, Tick(0))?,
            panel: CitizenPanel { day: 0, seed },
            app,
            egui_ctx,
            egui_state,
            peds: Vec::new(),
            zrzut: Vec::new(),
            dzien: 0,
            pokaz_karte: false,
            ludzi,
        })
    }

    /// Ilu pieszych trzyma warstwa Mikro. Diagnostyka: „nie widać nikogo" ma dwie
    /// różne przyczyny — nikt nie wszedł w kadr albo nikt nie jest rysowany.
    /// Karta zaznaczonego mieszkańca jako tekst — **ta sama funkcja**, którą wypisuje
    /// `headless m3day --inspect` i której broni złoty test (korekta E-8).
    #[must_use]
    pub fn card_text(&mut self) -> String {
        use magnat_ui::InspectorPanel;
        self.panel.build(&self.ui, &self.app.world)
    }

    #[must_use]
    pub fn tick(&self) -> Tick {
        self.ui.time.clock().tick()
    }

    #[must_use]
    pub fn micro_len(&self) -> usize {
        self.app
            .world
            .resource::<AgentSources>()
            .get()
            .map_or(0, |z| z.travel.micro_len())
    }

    #[must_use]
    pub fn population(&self) -> usize {
        self.ludzi
    }

    pub fn set_speed(&mut self, s: SimSpeed) {
        self.ui.time.set_speed(s);
    }

    pub fn toggle_pause(&mut self) {
        self.ui.time.toggle_pause();
    }

    pub fn toggle_card(&mut self) {
        self.pokaz_karte = !self.pokaz_karte;
    }

    /// Zdarzenie wejścia. `true` = pochłonięte przez UI i kamera ma go **nie** widzieć
    /// (§5.11: priorytet UI nad kamerą).
    pub fn on_window_event(&mut self, window: &Window, e: &winit::event::WindowEvent) -> bool {
        let odp = self.egui_state.on_window_event(window, e);
        odp.consumed
    }

    /// Przewija dobę do podanej godziny **przed** pierwszą klatką.
    ///
    /// Miasto o północy śpi, więc okno otwarte na minucie zero pokazuje puste ulice
    /// i wygląda dokładnie jak zepsute. Przewinięcie do rana jest tanie (600 minut to
    /// ułamek sekundy dla miasta 8 km) i przy okazji rozgrzewa kolejkę zdarzeń.
    ///
    /// Okno warstwy Mikro jest otwarte **już w trakcie przewijania**, na pozycji kamery
    /// startowej. Bez tego szczyt poranny jest niewidzialny: pieszy wchodzi w warstwę
    /// w chwili, gdy **zaczyna** podróż, a kto wyszedł o 7:40, ten o 8:15 jest już
    /// w drodze i drugiej szansy nie dostanie.
    pub fn warm_up(&mut self, minut: u32, kamera: glam::DVec3) {
        if let Some(z) = self.app.world.resource::<AgentSources>().get() {
            z.travel
                .set_micro_window(Some((kamera.x as i32, kamera.y as i32)), MICRO_RADIUS_M);
        }
        for _ in 0..minut {
            self.app.tick();
        }
        let t = Tick(u64::from(minut));
        self.ui.time = magnat_ui::TimeControlsWidget::new(t);
        self.dzien = t.0 / 1440;
        self.panel.day = self.dzien;
    }

    /// Przewija symulację o tyle minut, ile zegar naliczył za `dt_ms` czasu realnego.
    /// Zwraca minutę świata po przewinięciu.
    pub fn advance(&mut self, dt_ms: u32) -> Tick {
        let minut = self.ui.time.advance(dt_ms);
        for _ in 0..minut {
            self.app.tick();
        }
        let t = self.ui.time.clock().tick();
        self.dzien = t.0 / 1440;
        self.panel.day = self.dzien;
        t
    }

    /// Ustawia okno warstwy Mikro na kadr i przepisuje pieszych dla renderera.
    pub fn pedestrians(&mut self, eye: glam::DVec3) -> &[PedestrianRecord] {
        let Some(z) = self.app.world.resource::<AgentSources>().get() else {
            self.peds.clear();
            return &self.peds;
        };
        z.travel
            .set_micro_window(Some((eye.x as i32, eye.y as i32)), MICRO_RADIUS_M);
        z.travel.micro_snapshot(&mut self.zrzut);
        self.peds.clear();
        self.peds.extend(self.zrzut.iter().map(|(e, pos, _)| PedestrianRecord {
            pos: *pos,
            entity: *e,
        }));
        &self.peds
    }

    /// Klik w pieszego: indeks encji z bufora ID na zaznaczenie (decyzja 9.3).
    ///
    /// Encja jest sprawdzana przez listę populacji, a nie brana na wiarę: bufor ID niesie
    /// odczyt sprzed klatki, a mieszkaniec mógł w tym czasie umrzeć albo się wyprowadzić.
    pub fn select(&mut self, entity_index: u32) -> bool {
        let lista = magnat_ui::ListPicker::new(
            self.app.world.resource::<Population>().citizens().to_vec(),
        );
        match lista.by_entity_index(entity_index) {
            Selection::Citizen(c) => {
                // Bufor śledzenia zbiera **realizację** obserwowanego mieszkańca — bez
                // tego karta pokazuje sam plan (decyzja 9.16, najwyżej ośmiu naraz).
                self.app.world.resource_mut::<Trace>().watch(entity_index);
                self.ui.selection = Selection::Citizen(c);
                self.pokaz_karte = true;
                true
            }
            _ => false,
        }
    }

    /// Buduje klatkę UI. Zwraca trójkąty do namalowania oraz prędkość, jeśli gracz
    /// właśnie kliknął w przycisk — pętla gry ustawia ją sama, bo to ona jest
    /// właścicielem zegara (decyzja 9.1).
    pub fn ui_frame(&mut self, window: &Window) -> (egui::FullOutput, Option<SimSpeed>) {
        let wejscie = self.egui_state.take_egui_input(window);
        let (catalog, locale) = (&self.ui.catalog, self.ui.locale);
        let zegar = self.ui.time;
        let model = self
            .pokaz_karte
            .then(|| self.panel.model(&self.ui, &self.app.world))
            .flatten();
        let mut wybor = None;
        let mut zamknij = false;

        let out = self.egui_ctx.clone().run_ui(wejscie, |ui| {
            egui::Area::new(egui::Id::new("magnat.time"))
                .fixed_pos(egui::pos2(12.0, 12.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        if let Some(s) = magnat_ui::widgets::time_bar(ui, &zegar, catalog, locale) {
                            wybor = Some(s);
                        }
                    });
                });
            if let Some(m) = model.as_ref() {
                let mut otwarte = true;
                egui::Window::new(catalog.fmt_key(locale, "ui.card.title", &[]))
                    .open(&mut otwarte)
                    .default_pos(egui::pos2(12.0, 70.0))
                    .default_size(egui::vec2(460.0, 760.0))
                    .vscroll(true)
                    .show(ui.ctx(), |ui| {
                        magnat_ui::widgets::citizen_card(ui, m, catalog, locale);
                    });
                zamknij = !otwarte;
            }
        });
        if zamknij {
            self.pokaz_karte = false;
        }
        self.egui_state
            .handle_platform_output(window, out.platform_output.clone());
        (out, wybor)
    }

    #[must_use]
    pub fn context(&self) -> &egui::Context {
        &self.egui_ctx
    }
}
