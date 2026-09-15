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
use magnat_economy::{LostSaleTracking, Market, MarketSystem, ShopPanelSnapshot};
use magnat_ecs::{App, ScheduleBuilder, World};
use magnat_jobs::JobPool;
use magnat_sim_snapshot::PedestrianRecord;
use magnat_traffic::{TrafficSystem, VehicleWearSystem};
use magnat_ui::{CitizenPanel, Locale, Selection, UiContext};
use magnat_ui::{ShopTab, ShopView};
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
    /// Rekordy dla renderera, wypełniane co klatkę wprost przez warstwę Mikro.
    /// Jedna kopia, nie dwie (M4c §5.12 punkt 3) — bufor pośredni zniknął razem
    /// ze zrzutem do krotki.
    peds: Vec<PedestrianRecord>,
    /// Doba, dla której karta odtwarza plan.
    dzien: u64,
    /// Czy panel jest widoczny. Karta bez zaznaczenia pokazuje komunikat, więc panel
    /// da się otworzyć, zanim gracz w kogokolwiek kliknie.
    pokaz_karte: bool,
    ludzi: usize,
    /// Rynek, jeśli gospodarka jest włączona. `Market` jest `Clone` i wewnętrznie
    /// współdzielony, więc klient trzyma go **obok** świata i czyta bez `&World`.
    market: Option<Market>,
    /// Migawka otwartego sklepu. **Podwójne buforowanie w wersji, która tu wystarcza**:
    /// panel czyta zawsze poprzednią migawkę, a nowa powstaje raz na godzinę gry.
    /// Nie ma tu wyścigu do rozwiązania — symulacja i render chodzą w jednym wątku
    /// pętli klatki — jest za to koszt: składanie migawki bierze zamek rynku, więc
    /// robienie tego co klatkę kosztowałoby tyle, ile panel jest wart.
    sklep: Option<ShopPanelSnapshot>,
    zakladka: ShopTab,
    pokaz_sklep: bool,
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
        economy: bool,
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

        let oracle = zaludnione.travel_oracle();
        oracle.set_micro_window(None, 0);

        // ── gospodarka w oknie (M5e/WP12, `AB-1`) ────────────────────────────────
        //
        // Do M5d klient wstawiał tu atrapę `InfinitePlaces` z M3, więc **cała faza M5
        // była niewidoczna w oknie z miastem**: mieszkańcy „chodzili po zakupy" do
        // miejsca, które zawsze miało wszystko i nic nie kosztowało. Most
        // `retail::setup` stawia dokładnie tę samą gospodarkę co scenariusz `m5shop`
        // i podmienia `Sources.places` na rynek (`Z-1`).
        //
        // `--no-economy` wraca do zachowania M3 i jest **udokumentowaną drogą
        // wyjścia** z kosztu klatki przy `X10` (`AB-2`): zakup to dwa wywołania
        // routera M4, więc doba z gospodarką kosztuje wielokrotnie więcej niż bez.
        let market = if economy {
            let r = magnat_headless::retail::setup(
                &mut world,
                city,
                zaludnione.places.clone(),
                oracle,
                seed,
                &JobPool::new(threads),
            )?;
            eprintln!(
                "gospodarka: {} sklepów, {} ofert, wypłata startowa {} zł dla {} gospodarstw",
                r.shops,
                r.market.offer_count(),
                r.incomes.1.get() / 100,
                r.incomes.0
            );
            Some(r.market)
        } else {
            let tabela = Arc::new(NeedTable::load_default()?);
            *world.resource_mut::<AgentSources>() = AgentSources::new(
                Box::new(InfinitePlaces::new(zaludnione.places.clone(), tabela)),
                oracle,
            );
            eprintln!("gospodarka wyłączona (--no-economy): miejsca z atrapy M3");
            None
        };

        let zasiane = bootstrap_day(&mut world, 0);
        eprintln!("kolejka zasiana: {zasiane} mieszkańców");

        let mut builder = ScheduleBuilder::new();
        // `MarketSystem` **przed** pętlą doby — kolejność jest kontraktem
        // `sim/economy` (ustawia rynkowi tick i rozlicza intencje z minuty `t−1`),
        // a nie preferencją klienta.
        if market.is_some() {
            builder.add(MarketSystem::new(&world));
        }
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
            market,
            sklep: None,
            zakladka: ShopTab::default(),
            pokaz_sklep: false,
            app,
            egui_ctx,
            egui_state,
            peds: Vec::new(),
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
        self.odswiez_sklep(t, false);
        t
    }

    /// Składa migawkę otwartego sklepu, jeśli minęła godzina gry albo panel właśnie
    /// się otworzył (`wymus`). Kadencja `EveryHour` z §5.11: wszystko, co panel
    /// pokazuje, i tak zmienia się co najwyżej raz na dobę poza stanem półki.
    fn odswiez_sklep(&mut self, t: Tick, wymus: bool) {
        if !self.pokaz_sklep {
            return;
        }
        let (Some(m), Some(stary)) = (self.market.as_ref(), self.sklep.as_ref()) else {
            return;
        };
        if !wymus && t.get() / 60 == stary.at.get() / 60 {
            return;
        }
        let site = stary.site;
        // Okres rachunku wyników to **bieżący miesiąc gry**, nie cała historia:
        // RZiS od początku świata pokazywałby sumę, a nie to, jak sklep radzi
        // sobie teraz — czyli nie odpowiadałby na pytanie, które gracz zadaje.
        let od = Tick(t.get() - t.get() % magnat_core::time::MINUTES_PER_MONTH);
        if let Some(s) = m.shop_panel(site, od, t) {
            self.sklep = Some(s);
        }
    }

    /// Otwiera panel sklepu stojącego najbliżej punktu trafienia.
    ///
    /// Bufor identyfikatorów z M3d niesie **wyłącznie pieszych** (korekta H-15 do M3:
    /// `engine/render` nie rysuje budynków do bufora ID), więc sklep wybiera się
    /// z promienia wokół punktu trafienia w teren — tą samą drogą, którą klient
    /// pokazuje kartę parceli. Kiedy `engine/render` dostanie budynki w buforze ID,
    /// to wywołanie zamieni się na odczyt `SiteId` i nic poza nim się nie zmieni.
    pub fn select_shop(&mut self, x: f32, y: f32, promien_m: f32) -> bool {
        let Some(m) = self.market.as_ref() else {
            return false;
        };
        let cel = magnat_spatial::Vec2::new(x, y);
        let najblizszy = m
            .sites()
            .into_iter()
            .filter_map(|s| m.shop_pos(s).map(|p| (s, (p - cel).length())))
            .filter(|(_, d)| *d <= promien_m)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((site, _)) = najblizszy else {
            return false;
        };
        // Otwarcie panelu **oznacza** zakład: to jest ta flaga, którą ustawia `game/`,
        // a `sim/economy` tylko czyta (`U-22`). Poziom nie wchodzi do hasha, więc
        // kliknięcie „pokaż" nie zmienia świata.
        m.set_tracking(site, LostSaleTracking::Full);
        let t = self.ui.time.clock().tick();
        let od = Tick(t.get() - t.get() % magnat_core::time::MINUTES_PER_MONTH);
        self.sklep = m.shop_panel(site, od, t);
        self.pokaz_sklep = self.sklep.is_some();
        self.pokaz_sklep
    }
    /// Ustawia okno warstwy Mikro na kadr i przepisuje pieszych dla renderera.
    pub fn pedestrians(&mut self, eye: glam::DVec3) -> &[PedestrianRecord] {
        let Some(z) = self.app.world.resource::<AgentSources>().get() else {
            self.peds.clear();
            return &self.peds;
        };
        z.travel
            .set_micro_window(Some((eye.x as i32, eye.y as i32)), MICRO_RADIUS_M);
        z.travel.micro_snapshot(&mut self.peds);
        &self.peds
    }

    /// Klik w pieszego: indeks encji z bufora ID na zaznaczenie (decyzja 9.3).
    ///
    /// Encja jest sprawdzana przez listę populacji, a nie brana na wiarę: bufor ID niesie
    /// odczyt sprzed klatki, a mieszkaniec mógł w tym czasie umrzeć albo się wyprowadzić.
    /// Raster nakładki ruchu z **bieżącej minuty symulacji** (WP11).
    ///
    /// Czyta **przedni** bufor zrzutu, którego krok minutowy w tej chwili nie dotyka —
    /// dlatego przełączenie nakładki nie czeka na symulację i mieści się w klatce.
    /// Wartości surowe mapuje na indeksy palety `data/ui/overlays.ron`, czyli tej samej
    /// tabeli, którą czyta podgląd `headless m3day --overlay`.
    #[must_use]
    pub fn pole_ruchu(
        &self,
        pole: magnat_traffic::TrafficField,
        map_size_m: u32,
    ) -> Option<crate::overlay::Pole> {
        let tab = magnat_world::OverlayTable::load().ok()?;
        let spec = tab.get(pole.key()).ok()?;
        let bok = u32::from(magnat_world::city::overlay::OVERLAY_CELL_M);
        let dim = (map_size_m / bok).max(1);
        let oracle = self
            .app
            .world
            .resource::<magnat_traffic::TrafficServices>()
            .oracle
            .clone();
        let surowe = self
            .app
            .world
            .resource::<magnat_traffic::TrafficOverlay>()
            .with_front(|snap| {
                oracle.with_road(|road| {
                    if pole.is_edge_field() {
                        let v: Vec<u16> = (0..road.edge_count())
                            .map(|i| snap.edge_value(pole, i).clamp(0, i64::from(u16::MAX)) as u16)
                            .collect();
                        magnat_traffic::rasterize_edges(road, &v, dim, bok, 1)
                    } else {
                        magnat_traffic::rasterize_points(&snap.lots, dim, bok, 1)
                    }
                })
            });
        Some(crate::overlay::Pole {
            dim,
            cell_m: bok as f32,
            values: surowe
                .iter()
                .map(|v| {
                    if *v == 0 {
                        0
                    } else {
                        spec.index_of(i64::from(*v)).max(1)
                    }
                })
                .collect(),
            palette: spec.palette(),
        })
    }

    pub fn select(&mut self, entity_index: u32) -> bool {
        let lista =
            magnat_ui::ListPicker::new(self.app.world.resource::<Population>().citizens().to_vec());
        match lista.by_entity_index(entity_index) {
            Selection::Citizen(c) => {
                // Dwa bufory śledzenia, bo dwie różne rzeczy: `Trace` zbiera zdarzenia
                // DES (co mieszkaniec robił), `TrafficOracle::watch` włącza rejestr
                // krawędź po krawędzi i zapamiętywanie porównania środków transportu
                // (jak jechał i dlaczego tak). Bez tego drugiego `TripLedger.entries`
                // jest puste dla **każdego** mieszkańca, a karta podróży nie ma z czego
                // policzyć rozbioru czasu (`N-6`).
                self.app.world.resource_mut::<Trace>().watch(entity_index);
                self.app
                    .world
                    .resource::<magnat_traffic::TrafficServices>()
                    .oracle
                    .watch(entity_index);
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
        let mut zamknij_sklep = false;
        let mut zakladka = self.zakladka;
        // Karta powstaje **przed** domknięciem, tak samo jak karta mieszkańca:
        // domknięcie nie może pożyczyć `self`, bo `egui_ctx` jest w środku.
        let karta_sklepu = self.sklep.as_ref().map(|s| {
            magnat_ui::ShopCard::build(
                catalog,
                locale,
                &ShopView {
                    snapshot: s,
                    kind: s.kind,
                    period_from: Tick(
                        s.at.get() - s.at.get() % magnat_core::time::MINUTES_PER_MONTH,
                    ),
                },
            )
        });

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
            if let Some(k) = karta_sklepu.as_ref() {
                let mut otwarte = true;
                egui::Window::new(catalog.fmt_key(locale, "ui.shop.title", &[]))
                    .open(&mut otwarte)
                    .default_pos(egui::pos2(500.0, 70.0))
                    .default_size(egui::vec2(560.0, 760.0))
                    .vscroll(true)
                    .show(ui.ctx(), |ui| {
                        magnat_ui::widgets::shop_card(ui, k, &mut zakladka, catalog, locale);
                    });
                zamknij_sklep = !otwarte;
            }
        });
        self.zakladka = zakladka;
        if zamknij {
            self.pokaz_karte = false;
        }
        if zamknij_sklep {
            self.pokaz_sklep = false;
            self.sklep = None;
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
