//! Warstwa gry w oknie: mieszkańcy, doba, panele (M3d WP11/WP12; M9b WP14).
//!
//! ### Co jest tu, a czego tu nie ma
//!
//! Tu jest **spięcie**, a nie logika. Populację liczy `sim/world::population`, dobę
//! przewijają systemy z `sim/agents`, treść karty buduje `magnat-ui`, a rysuje ją
//! `engine/render::ui`. Gdyby którakolwiek z tych rzeczy zaczęła się liczyć w tym pliku,
//! przestałaby być sprawdzalna bez okna — a headless-first (00 §6) mówi odwrotnie.
//!
//! ### Sesja mieszka w `GameState`, nie tutaj (M9b)
//!
//! Do M9a `Citizens` trzymał [`Session`] u siebie. Od WP14 świat ma jednego właściciela
//! i jest nim `magnat_game::GameState` w [`crate::app::App`], bo to on przechodzi przez
//! menu, generację i podgląd — a przez te stany sesji jeszcze nie ma. `Citizens` jest
//! odtąd **stanem interfejsu rozgrywki**: co jest zaznaczone, która zakładka otwarta,
//! kiedy odświeżyć migawkę panelu.
//!
//! ### Prędkość gry nie zmienia wyniku
//!
//! Zegar zamienia czas realny na **liczbę minut gry do wykonania**, a każda minuta jest
//! tym samym tickiem (§5.11). Klatka o dowolnej długości przy dowolnej prędkości daje ten
//! sam ciąg ticków — ta sama ścieżka, na której stoi test `det_speed_invariance`.
//!
//! `ponytail:` blok `impl` ma ~360 linii i zostaje jednym blokiem. Sufit nazwany:
//! to jest jeden temat — stan interfejsu rozgrywki — a podział na „panele" i „warstwę
//! Mikro" przeciąłby `advance`, które dotyka obu. Punkt podziału przyjdzie sam razem
//! z panelami biznesowymi `M9e`: wtedy panele wyprowadzą się do własnego rejestru.
//!
//! ### Warstwa Mikro chodzi tylko w kadrze
//!
//! Polilinia trasy dla 274 tys. mieszkańców to koszt, którego nikt nie ogląda. Oracle ruchu
//! dostaje **okno**: środek kadru i promień. Mezo — czyli cały wynik — nie zależy od tego
//! ani o minutę, bo warstwa Mikro nie ma prawa zapisu do stanu (00 §4).

use magnat_agents::{AgentSources, Population, Trace};
use magnat_core::{SimSpeed, Tick};
use magnat_economy::{LostSaleTracking, ShopPanelSnapshot};
use magnat_game::Session;
use magnat_sim_snapshot::PedestrianRecord;
use magnat_ui::{CitizenPanel, Locale, Selection, Theme, UiContext};
use magnat_ui::{ShopTab, ShopView};

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
pub(crate) const SWAPS: u32 = 50_000;

pub struct Citizens {
    ui: UiContext,
    panel: CitizenPanel,
    /// Rekordy dla renderera, wypełniane co klatkę wprost przez warstwę Mikro.
    peds: Vec<PedestrianRecord>,
    /// Doba, dla której karta odtwarza plan.
    dzien: u64,
    /// Czy panel jest widoczny. Karta bez zaznaczenia pokazuje komunikat, więc panel
    /// da się otworzyć, zanim gracz w kogokolwiek kliknie.
    pokaz_karte: bool,
    /// Migawka otwartego sklepu. **Podwójne buforowanie w wersji, która tu wystarcza**:
    /// panel czyta zawsze poprzednią migawkę, a nowa powstaje raz na godzinę gry.
    /// Nie ma tu wyścigu do rozwiązania — symulacja i render chodzą w jednym wątku
    /// pętli klatki — jest za to koszt: składanie migawki bierze zamek rynku, więc
    /// robienie tego co klatkę kosztowałoby tyle, ile panel jest wart.
    sklep: Option<ShopPanelSnapshot>,
    zakladka: ShopTab,
    pokaz_sklep: bool,
    /// Wersje źródeł danych dla modeli paneli (`M9b` §5.8): model przebudowuje się
    /// wyłącznie wtedy, gdy któreś z nich drgnęło.
    wersje: magnat_ui::Versions,
    /// Model karty mieszkańca. **To jest miejsce, w którym kryterium WP3 ma skutek**:
    /// bez zmiany danych klatka nie buduje modelu i nie alokuje ani bajtu.
    model: magnat_ui::Cached<Option<magnat_ui::CitizenModel>>,
    /// Model karty sklepu — ta sama zasada, inne źródło.
    karta_sklepu: magnat_ui::Cached<Option<magnat_ui::ShopCard>>,
}

/// Co unieważnia kartę mieszkańca: świat (potrzeby, majątek), zaznaczenie, minuta
/// (oś dnia) i język. Cztery źródła, bo cztery rzeczy, które ją naprawdę zmieniają.
static ZRODLA_KARTY: [magnat_ui::DataSource; 4] = [
    magnat_ui::DataSource::World,
    magnat_ui::DataSource::Selection,
    magnat_ui::DataSource::Clock,
    magnat_ui::DataSource::Locale,
];

/// Kartę sklepu unieważnia wyłącznie nowa migawka rynku i zmiana języka — migawka
/// powstaje raz na godzinę gry, więc karta też.
static ZRODLA_SKLEPU: [magnat_ui::DataSource; 2] =
    [magnat_ui::DataSource::Market, magnat_ui::DataSource::Locale];

impl Citizens {
    /// # Errors
    /// Brak katalogu tekstów w `data/locale/`.
    pub fn new(seed: u64, locale: Locale) -> Result<Citizens, Box<dyn std::error::Error>> {
        Ok(Citizens {
            ui: UiContext::new(locale, Tick(0))?,
            panel: CitizenPanel { day: 0, seed },
            sklep: None,
            zakladka: ShopTab::default(),
            pokaz_sklep: false,
            peds: Vec::new(),
            dzien: 0,
            pokaz_karte: false,
            wersje: magnat_ui::Versions::new(),
            model: magnat_ui::Cached::new(&ZRODLA_KARTY),
            karta_sklepu: magnat_ui::Cached::new(&ZRODLA_SKLEPU),
        })
    }

    /// Karta zaznaczonego mieszkańca jako tekst — **ta sama funkcja**, którą wypisuje
    /// `headless m3day --inspect` i której broni złoty test (korekta E-8).
    #[must_use]
    pub fn card_text(&mut self, session: &Session) -> String {
        use magnat_ui::{InspectorPanel, RichExt};
        self.panel.build(&self.ui, &session.app.world).to_plain()
    }

    pub fn set_locale(&mut self, l: Locale) {
        if self.ui.locale != l {
            self.ui.locale = l;
            // Zmiana języka unieważnia **każdy** model niosący tekst — to jest
            // dokładnie ten przypadek, dla którego `DataSource::Locale` istnieje.
            self.wersje.bump(magnat_ui::DataSource::Locale);
        }
    }

    /// Ilu pieszych trzyma warstwa Mikro. Diagnostyka: „nie widać nikogo" ma dwie
    /// różne przyczyny — nikt nie wszedł w kadr albo nikt nie jest rysowany.
    #[must_use]
    pub fn micro_len(&self, session: &Session) -> usize {
        session
            .app
            .world
            .resource::<AgentSources>()
            .get()
            .map_or(0, |z| z.travel.micro_len())
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

    /// Przewija dobę do podanej godziny **przed** pierwszą klatką.
    ///
    /// Miasto o północy śpi, więc okno otwarte na minucie zero pokazuje puste ulice
    /// i wygląda dokładnie jak zepsute. Przewinięcie do rana jest tanie i przy okazji
    /// rozgrzewa kolejkę zdarzeń.
    ///
    /// Okno warstwy Mikro jest otwarte **już w trakcie przewijania**, na pozycji kamery
    /// startowej. Bez tego szczyt poranny jest niewidzialny: pieszy wchodzi w warstwę
    /// w chwili, gdy **zaczyna** podróż, a kto wyszedł o 7:40, o 8:15 jest już w drodze.
    pub fn warm_up(&mut self, session: &mut Session, minut: u32, kamera: glam::DVec3) {
        if let Some(z) = session.app.world.resource::<AgentSources>().get() {
            z.travel
                .set_micro_window(Some((kamera.x as i32, kamera.y as i32)), MICRO_RADIUS_M);
        }
        for _ in 0..minut {
            session.step(1, 0);
        }
        let t = Tick(u64::from(minut));
        self.ui.time = magnat_ui::TimeControlsWidget::new(t);
        self.dzien = t.0 / 1440;
        self.panel.day = self.dzien;
        self.wersje.bump(magnat_ui::DataSource::Clock);
        self.wersje.bump(magnat_ui::DataSource::World);
    }

    /// Przewija symulację o tyle minut, ile zegar naliczył za `dt_ms` czasu realnego.
    /// Zwraca minutę świata po przewinięciu.
    pub fn advance(&mut self, session: &mut Session, dt_ms: u32) -> Tick {
        let minut = self.ui.time.advance(dt_ms);
        for _ in 0..minut {
            session.step(1, 0);
        }
        if minut > 0 {
            self.wersje.bump(magnat_ui::DataSource::Clock);
            self.wersje.bump(magnat_ui::DataSource::World);
        }
        let t = self.ui.time.clock().tick();
        self.dzien = t.0 / 1440;
        self.panel.day = self.dzien;
        self.odswiez_sklep(session, t, false);
        t
    }

    /// Składa migawkę otwartego sklepu, jeśli minęła godzina gry albo panel właśnie
    /// się otworzył (`wymus`). Kadencja `EveryHour` z §5.11: wszystko, co panel
    /// pokazuje, i tak zmienia się co najwyżej raz na dobę poza stanem półki.
    fn odswiez_sklep(&mut self, session: &Session, t: Tick, wymus: bool) {
        if !self.pokaz_sklep {
            return;
        }
        let (Some(m), Some(stary)) = (session.market.as_ref(), self.sklep.as_ref()) else {
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
            self.wersje.bump(magnat_ui::DataSource::Market);
        }
    }

    /// Otwiera panel sklepu stojącego najbliżej punktu trafienia.
    ///
    /// Bufor identyfikatorów niesie **wyłącznie pieszych** (korekta H-15 do M3), więc
    /// sklep wybiera się z promienia wokół punktu trafienia w teren. Kiedy `engine/render`
    /// dostanie budynki w buforze ID, to wywołanie zamieni się na odczyt `SiteId`.
    pub fn select_shop(&mut self, session: &Session, x: f32, y: f32, promien_m: f32) -> bool {
        let Some(m) = session.market.as_ref() else {
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
        self.wersje.bump(magnat_ui::DataSource::Market);
        self.pokaz_sklep
    }

    /// Ustawia okno warstwy Mikro na kadr i przepisuje pieszych dla renderera.
    pub fn pedestrians(&mut self, session: &Session, eye: glam::DVec3) -> &[PedestrianRecord] {
        let Some(z) = session.app.world.resource::<AgentSources>().get() else {
            self.peds.clear();
            return &self.peds;
        };
        z.travel
            .set_micro_window(Some((eye.x as i32, eye.y as i32)), MICRO_RADIUS_M);
        z.travel.micro_snapshot(&mut self.peds);
        &self.peds
    }

    /// Raster nakładki ruchu z **bieżącej minuty symulacji** (WP11).
    ///
    /// Czyta **przedni** bufor zrzutu, którego krok minutowy w tej chwili nie dotyka —
    /// dlatego przełączenie nakładki nie czeka na symulację i mieści się w klatce.
    #[must_use]
    pub fn pole_ruchu(
        &self,
        session: &Session,
        pole: magnat_traffic::TrafficField,
        map_size_m: u32,
    ) -> Option<crate::overlay::Pole> {
        let tab = magnat_world::OverlayTable::load().ok()?;
        let spec = tab.get(pole.key()).ok()?;
        let bok = u32::from(magnat_world::city::overlay::OVERLAY_CELL_M);
        let dim = (map_size_m / bok).max(1);
        let oracle = session
            .app
            .world
            .resource::<magnat_traffic::TrafficServices>()
            .oracle
            .clone();
        let surowe = session
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

    /// Klik w pieszego: indeks encji z bufora ID na zaznaczenie (decyzja 9.3).
    ///
    /// Encja jest sprawdzana przez listę populacji, a nie brana na wiarę: bufor ID niesie
    /// odczyt sprzed klatki, a mieszkaniec mógł w tym czasie umrzeć albo się wyprowadzić.
    pub fn select(&mut self, session: &mut Session, entity_index: u32) -> bool {
        let lista = magnat_ui::ListPicker::new(
            session
                .app
                .world
                .resource::<Population>()
                .citizens()
                .to_vec(),
        );
        match lista.by_entity_index(entity_index) {
            Selection::Citizen(c) => {
                // Dwa bufory śledzenia, bo dwie różne rzeczy: `Trace` zbiera zdarzenia
                // DES (co mieszkaniec robił), `TrafficOracle::watch` włącza rejestr
                // krawędź po krawędzi i porównanie środków transportu (jak jechał
                // i dlaczego tak). Bez tego drugiego `TripLedger.entries` jest puste
                // dla **każdego** mieszkańca, a karta podróży nie ma z czego liczyć (`N-6`).
                session
                    .app
                    .world
                    .resource_mut::<Trace>()
                    .watch(entity_index);
                session
                    .app
                    .world
                    .resource::<magnat_traffic::TrafficServices>()
                    .oracle
                    .watch(entity_index);
                self.ui.selection = Selection::Citizen(c);
                self.pokaz_karte = true;
                self.wersje.bump(magnat_ui::DataSource::Selection);
                true
            }
            _ => false,
        }
    }

    /// Buduje interfejs rozgrywki w podanym `Ui`. Zwraca prędkość, jeśli gracz
    /// kliknął w przycisk — pętla gry ustawia ją sama, bo to ona jest właścicielem
    /// zegara (decyzja 9.1).
    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        session: &Session,
    ) -> Option<SimSpeed> {
        // Rozbicie na pola, bo `Cached::get` pożycza `self` mutowalnie, a budowniczy
        // modelu czyta `panel` i `ui` — kompilator nie zna granicy między polami
        // struktury, dopóki mu jej nie pokażemy.
        let Citizens {
            ui: uictx,
            panel,
            wersje,
            model,
            karta_sklepu,
            sklep,
            pokaz_karte,
            zakladka,
            ..
        } = self;
        let (catalog, locale) = (&uictx.catalog, uictx.locale);
        let zegar = uictx.time;
        // **Tu działa dirty-flagging (`M9b` §5.8):** bez zmiany świata, zaznaczenia,
        // minuty i języka model nie powstaje po raz drugi — a to on kosztuje, nie
        // rysowanie kilkuset widgetów.
        let model = pokaz_karte
            .then(|| {
                model
                    .get(wersje, || panel.model(uictx, &session.app.world))
                    .as_ref()
            })
            .flatten();
        let karta_sklepu = karta_sklepu
            .get(wersje, || {
                sklep.as_ref().map(|s| {
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
                })
            })
            .as_ref();
        let mut wybor = None;
        let mut zamknij = false;
        let mut zamknij_sklep = false;
        let mut zakladka = *zakladka;

        egui::Area::new(egui::Id::new("magnat.time"))
            .fixed_pos(egui::pos2(12.0, 12.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    if let Some(s) = magnat_ui::widgets::time_bar(ui, &zegar, catalog, locale) {
                        wybor = Some(s);
                    }
                });
            });
        if let Some(m) = model {
            let mut otwarte = true;
            egui::Window::new(catalog.fmt_key(locale, "ui.card.title", &[]))
                .open(&mut otwarte)
                .default_pos(egui::pos2(12.0, 70.0))
                .default_size(egui::vec2(460.0, 760.0))
                .vscroll(true)
                .show(ui.ctx(), |ui| {
                    magnat_ui::widgets::citizen_card(ui, theme, m, catalog, locale);
                });
            zamknij = !otwarte;
        }
        if let Some(k) = karta_sklepu {
            let mut otwarte = true;
            egui::Window::new(catalog.fmt_key(locale, "ui.shop.title", &[]))
                .open(&mut otwarte)
                .default_pos(egui::pos2(500.0, 70.0))
                .default_size(egui::vec2(560.0, 760.0))
                .vscroll(true)
                .show(ui.ctx(), |ui| {
                    magnat_ui::widgets::shop_card(ui, theme, k, &mut zakladka, catalog, locale);
                });
            zamknij_sklep = !otwarte;
        }

        self.zakladka = zakladka;
        if zamknij {
            self.pokaz_karte = false;
        }
        if zamknij_sklep {
            self.pokaz_sklep = false;
            self.sklep = None;
        }
        wybor
    }
}
