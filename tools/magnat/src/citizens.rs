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
//! `ponytail:` blok `impl` ma ~450 linii i zostaje jednym blokiem. Sufit nazwany:
//! to jest jeden temat — stan interfejsu rozgrywki — a podział na „panele" i „warstwę
//! Mikro" przeciąłby `advance`, które dotyka obu. Punkt podziału przyjdzie sam razem
//! z panelami biznesowymi `M9e`: wtedy panele wyprowadzą się do własnego rejestru,
//! a tutaj zostanie wejście, kadr i nawigacja po kartach. Wiersz 41 rejestru długu.
//!
//! ### Warstwa Mikro chodzi tylko w kadrze
//!
//! Polilinia trasy dla 274 tys. mieszkańców to koszt, którego nikt nie ogląda. Oracle ruchu
//! dostaje **okno**: środek kadru i promień. Mezo — czyli cały wynik — nie zależy od tego
//! ani o minutę, bo warstwa Mikro nie ma prawa zapisu do stanu (00 §4).

use magnat_agents::{AgentSources, Population, Trace};
use magnat_core::Subject;
use magnat_core::{SimSpeed, Tick};
use magnat_economy::LostSaleTracking;
use magnat_game::inspect::CardCtx;
use magnat_game::Session;
use magnat_ui::{CitizenPanel, InspectionNav, Locale, Theme, UiContext};

/// Promień okna warstwy Mikro w metrach, liczony **od celu kamery**.
///
/// Zapas nad kadrem jest po to, żeby pieszy wszedł w warstwę **zanim** wjedzie w pole
/// widzenia — inaczej pojawiałby się na środku ulicy w chwili, gdy kamera go dosięga.
/// Środkiem jest cel, a nie oko: przy orbicie z 900 m oko stoi 767 m w poziomie od celu,
/// więc okno wokół oka było przesunięte o tyle samo i połowa leżała za plecami (`J-2`).
const MICRO_RADIUS_M: u32 = 900;

/// Ile prób poprawkowych mediany dojazdu wykonuje Etap 8 w kliencie.
///
/// ponytail: mniej niż w `headless population` (200 tys.), bo klient startuje przy każdym
/// uruchomieniu, a pętla poprawkowa to sekundy. Kryterium `gen_commute_hist` sprawdza
/// headless; okno ma pokazać miasto, a nie zdać test statystyczny.
pub(crate) const SWAPS: u32 = 50_000;

/// Legenda otwartej nakładki danych (§5.10). Dane, nie referencja: nakładka przelicza
/// się przy przełączeniu, a HUD rysuje ją w każdej klatce.
pub struct Legenda {
    /// Klucz nakładki — człon `ui.overlay.<key>` w `data/locale/`.
    pub key: &'static str,
    pub stops: Vec<(u8, i64)>,
    pub unit: String,
    pub palette: [[u8; 4]; 256],
}

pub struct Citizens {
    pub(crate) ui: UiContext,
    panel: CitizenPanel,
    /// Rekordy dla renderera, wypełniane co klatkę wprost przez warstwę Mikro.
    /// Doba, dla której karta odtwarza plan.
    dzien: u64,
    /// Czy panel jest widoczny. Karta bez zaznaczenia pokazuje komunikat, więc panel
    /// da się otworzyć, zanim gracz w kogokolwiek kliknie.
    pokaz_karte: bool,
    /// Stos „wstecz/dalej" karty inspekcji (`M9c` §5.7). To **on** jest zaznaczeniem:
    /// `UiContext::selection` idzie za nim, a nie odwrotnie.
    nav: InspectionNav,
    /// Wybrana zakładka karty — indeks w `InspectionCard::tabs`.
    zakladka: usize,
    /// Legenda otwartej nakładki danych. `None` = nakładka terenu albo żadna.
    legenda: Option<Legenda>,
    /// Filtr encji rysowanych w świecie (§14.2). `None` = wszyscy.
    filtr: Option<magnat_game::EntityFilter>,
    /// Wersje źródeł danych dla modeli paneli (`M9b` §5.8): model przebudowuje się
    /// wyłącznie wtedy, gdy któreś z nich drgnęło.
    pub(crate) wersje: magnat_ui::Versions,
    /// Model karty mieszkańca — paski potrzeb i płótno doby rysują się z niego,
    /// bo to są **wykresy**, a nie tekst. Treść karty idzie osobno, przez `karta`.
    model: magnat_ui::Cached<Option<magnat_ui::CitizenModel>>,
    /// Karta inspekcji zaznaczonego podmiotu. **To jest miejsce, w którym kryterium
    /// WP3 ma skutek**: bez zmiany danych klatka nie buduje modelu i nie alokuje.
    karta: magnat_ui::Cached<Option<magnat_ui::InspectionCard>>,
    /// Dok lewy: panele biznesowe (`M9e` WP10). Prawy należy do karty inspekcji
    /// i ta różnica jest stała — zamiana miejscami psuje nawyk (`ui-design.md` §5).
    pub(crate) panele: magnat_game::Panels,
    /// Warunki „zatrzymaj, gdy…" (`M9e` WP11). Strona widoku: pauza to `SimSpeed`,
    /// a prędkość nie wchodzi do hasha, więc zatrzymanie nie zmienia wyniku.
    pub(crate) stop: magnat_game::StopWatch,
    /// Tryb „śledź": kamera idzie za mieszkańcem, a `LodPin` trzyma go w Mikro.
    pub(crate) follow: magnat_game::timectl::Follow,
    /// Ostatnie trafienie warunku — zdanie w pasie alertów, dopóki gracz nie ruszy.
    pub(crate) trafienie: Option<magnat_game::StopHit>,
    /// Samouczek, jeśli scenariusz o niego prosi (`DI-35`). Strona widoku: nie
    /// dotyka świata, nie wchodzi do hasha i nie wydaje komend za gracza.
    pub(crate) samouczek: Option<magnat_game::Tutorial>,
    /// Czy gracz ma dziś włączoną nakładkę zasięgu sklepu — drugi krok samouczka
    /// kończy się tym faktem, a nakładkę wybiera `App`, nie ten moduł.
    pub(crate) zasieg_wlaczony: bool,
}

/// Co unieważnia kartę mieszkańca: świat (potrzeby, majątek), zaznaczenie, minuta
/// (oś dnia) i język. Cztery źródła, bo cztery rzeczy, które ją naprawdę zmieniają.
static ZRODLA_KARTY: [magnat_ui::DataSource; 4] = [
    magnat_ui::DataSource::World,
    magnat_ui::DataSource::Selection,
    magnat_ui::DataSource::Clock,
    magnat_ui::DataSource::Locale,
];

/// Co unieważnia kartę inspekcji: zaznaczenie, minuta świata, język i rynek.
///
/// **Minuta, a nie klatka** — i to jest cała oszczędność: karta zakładu składa migawkę
/// panelu, a ta bierze zamek rynku. Przy 60 klatkach na sekundę i prędkości 10× rynek
/// jest odpytywany dziesięć razy na sekundę, a nie sześćdziesiąt.
static ZRODLA_KARTY_INSPEKCJI: [magnat_ui::DataSource; 4] = [
    magnat_ui::DataSource::Selection,
    magnat_ui::DataSource::Clock,
    magnat_ui::DataSource::Locale,
    magnat_ui::DataSource::Market,
];

impl Citizens {
    /// # Errors
    /// Brak katalogu tekstów w `data/locale/`.
    pub fn new(seed: u64, locale: Locale) -> Result<Citizens, Box<dyn std::error::Error>> {
        Ok(Citizens {
            ui: UiContext::new(locale, Tick(0))?,
            panel: CitizenPanel { day: 0, seed },
            zakladka: 0,
            legenda: None,
            filtr: None,
            nav: InspectionNav::new(),
            dzien: 0,
            pokaz_karte: false,
            wersje: magnat_ui::Versions::new(),
            panele: magnat_game::Panels::default(),
            stop: magnat_game::StopWatch::default(),
            follow: magnat_game::timectl::Follow::default(),
            trafienie: None,
            samouczek: None,
            zasieg_wlaczony: false,
            model: magnat_ui::Cached::new(&ZRODLA_KARTY),
            karta: magnat_ui::Cached::new(&ZRODLA_KARTY_INSPEKCJI),
        })
    }

    /// Karta zaznaczonego podmiotu jako tekst — do konsoli deweloperskiej.
    ///
    /// **Ta sama funkcja, którą rysuje okno** (`game::inspect::card`), a nie drugi
    /// wydruk obok niej: konsola ma pokazywać to, co widzi gracz, inaczej diagnostyka
    /// odpowiada na inne pytanie niż zadane. Złotego wydruku dnia broni osobno
    /// `CitizenPanel` w `engine/ui` — tamtędy chodzi `headless m3day --inspect`.
    #[must_use]
    pub fn card_text(&mut self, session: &Session) -> String {
        let Some(p) = self.nav.current() else {
            return self.ui.text("ui.card.no_selection");
        };
        let (c, l) = (&self.ui.catalog, self.ui.locale);
        magnat_game::inspect::card(&CardCtx::new(c, l, session), p).render_text(c, l)
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
        // Gracz ruszył grę dalej — wiersz o zatrzymaniu przestał być odpowiedzią
        // na pytanie „czemu stoi".
        self.trafienie = None;
    }

    /// Zaznaczony podmiot — wejście trybu „śledź".
    #[must_use]
    pub fn selected(&self) -> Option<magnat_core::Subject> {
        self.nav.current()
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
    /// Okno warstwy Mikro jest otwarte **już w trakcie przewijania**, na celu kamery
    /// startowej. Bez tego szczyt poranny jest niewidzialny: pieszy wchodzi w warstwę
    /// w chwili, gdy **zaczyna** podróż, a kto wyszedł o 7:40, o 8:15 jest już w drodze.
    pub fn warm_up(&mut self, session: &mut Session, minut: u32, cel_kamery: glam::DVec3) {
        let (x, y) = (cel_kamery.x as i32, cel_kamery.y as i32);
        if let Some(z) = session.app.world.resource::<AgentSources>().get() {
            z.travel.set_micro_window(Some((x, y)), MICRO_RADIUS_M);
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
            self.panele.sync(session);
            self.tutorial_tick(session);
            // Warunki sprawdzają się **na stanie, który już jest**: nic nie liczą
            // i nic nie zapisują. Trafienie stawia zegar na pauzie na granicy klatki.
            let cel = !session.fresh_objectives().is_empty();
            if let Some(h) = self.stop.check(session, cel) {
                self.trafienie = Some(h);
                self.ui.time.set_speed(SimSpeed::Paused);
            }
        }
        let t = self.ui.time.clock().tick();
        self.dzien = t.0 / 1440;
        self.panel.day = self.dzien;
        t
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
        self.idz_do(Subject::Site(site));
        true
    }

    /// Zaznaczony zakład — przedmiot nakładki „zasięg sklepu".
    #[must_use]
    pub fn wybrany_zaklad(&self) -> Option<magnat_core::SiteId> {
        match self.nav.current() {
            Some(Subject::Site(s)) => Some(s),
            _ => None,
        }
    }

    /// Legenda nakładki danych — nazwa, podziałki i jednostka.
    pub fn set_legend(&mut self, l: Option<Legenda>) {
        self.legenda = l;
    }

    /// Otwiera kartę podmiotu i odkłada poprzednią na stos wstecz.
    pub fn idz_do(&mut self, s: Subject) {
        if self.nav.current() != Some(s) {
            // Zakładka wraca na pierwszą przy zmianie **rodzaju** podmiotu: gracz,
            // który patrzył na „Dlaczego" u mieszkanki, nie ma trafić na czwartą
            // zakładkę zakładu (`ui-design.md` §4).
            if self.nav.current().map(Subject::kind) != Some(s.kind()) {
                self.zakladka = 0;
            }
            self.nav.go(s);
            self.ui.selection = Some(s);
            self.pokaz_karte = true;
            self.wersje.bump(magnat_ui::DataSource::Selection);
        }
    }

    /// Wstecz i dalej po stosie kart (`ViewCommand::NavigateBack`/`Forward`).
    pub fn nawiguj(&mut self, wstecz: bool) -> bool {
        let ruszyl = if wstecz {
            self.nav.back()
        } else {
            self.nav.forward()
        };
        if ruszyl {
            self.ui.selection = self.nav.current();
            self.zakladka = 0;
            self.wersje.bump(magnat_ui::DataSource::Selection);
        }
        ruszyl
    }

    /// Ustawia okno warstwy Mikro na kadr.
    ///
    /// Od M11a klient **nie przepisuje** pieszych dla renderera: rekordy składa
    /// `magnat_game::SnapshotFiller`, bo kanałem sim → render jest snapshot, a nie
    /// osobny slice (M11a §5.2). Zostaje to, co należy do widoku i tylko do niego —
    /// okno warstwy Mikro, czyli wycinek miasta, który w ogóle jest krokowany.
    pub fn okno_mikro(&self, session: &Session, cel: glam::DVec3) {
        let Some(z) = session.app.world.resource::<AgentSources>().get() else {
            return;
        };
        z.travel
            .set_micro_window(Some((cel.x as i32, cel.y as i32)), MICRO_RADIUS_M);
    }

    /// Filtr encji (§14.2) albo `None`, gdy widać wszystkich.
    ///
    /// Filtr działa **tutaj**, a nie w warstwie Mikro: warstwa jest wspólna dla renderu
    /// i dla śledzenia, a filtr jest preferencją widoku i nie ma prawa zmienić tego,
    /// kogo symulacja liczy.
    #[must_use]
    pub fn filtr(&self) -> Option<magnat_game::EntityFilter> {
        self.filtr
    }

    /// Przełącza filtr encji: wszyscy → moi klienci → moi pracownicy → wszyscy.
    ///
    /// Oba filtry potrzebują **wskazanego zakładu**, więc bez zaznaczonego zakładu
    /// przełącznik gaśnie i mówi o tym w konsoli — filtr bez przedmiotu ukrywałby
    /// wszystkich i wyglądał jak zepsuty render.
    pub fn przelacz_filtr(&mut self) -> Option<&'static str> {
        use magnat_game::EntityFilter as F;
        let site = self.wybrany_zaklad();
        self.filtr = match (self.filtr, site) {
            (None, Some(s)) => Some(F::MyCustomers { site: s }),
            (Some(F::MyCustomers { .. }), Some(s)) => Some(F::MyEmployees { site: s }),
            _ => None,
        };
        self.filtr.map(magnat_game::EntityFilter::key)
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
            Some(magnat_core::Subject::Citizen(c)) => {
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
                self.idz_do(Subject::Citizen(c));
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
    ) -> (Option<SimSpeed>, magnat_game::PanelAction) {
        // Rozbicie na pola, bo `Cached::get` pożycza `self` mutowalnie, a budowniczy
        // modelu czyta `panel` i `ui` — kompilator nie zna granicy między polami
        // struktury, dopóki mu jej nie pokażemy.
        let Citizens {
            ui: uictx,
            panel,
            wersje,
            model,
            karta,
            nav,
            pokaz_karte,
            zakladka,
            ..
        } = self;
        let (catalog, locale) = (&uictx.catalog, uictx.locale);
        let zegar = uictx.time;
        let podmiot = nav.current();
        // **Tu działa dirty-flagging (`M9b` §5.8):** bez zmiany zaznaczenia, minuty,
        // języka i rynku karta nie powstaje po raz drugi — a to ona kosztuje, nie
        // rysowanie kilkuset widgetów.
        let karta = pokaz_karte
            .then(|| {
                karta
                    .get(wersje, || {
                        podmiot.map(|p| {
                            magnat_game::inspect::card(&CardCtx::new(catalog, locale, session), p)
                        })
                    })
                    .as_ref()
            })
            .flatten();
        // Paski potrzeb i płótno doby to **wykresy**, a nie tekst — rysują się z modelu
        // mieszkańca, obok treści zakładki. Karta i model czytają ten sam świat, więc
        // rozjechać się nie mogą; różni je forma, nie liczba.
        let model = matches!(podmiot, Some(Subject::Citizen(_)))
            .then(|| {
                model
                    .get(wersje, || panel.model(uictx, &session.app.world))
                    .as_ref()
            })
            .flatten();

        let mut zamknij = false;
        let mut zakladka_lokalna = *zakladka;
        let mut skok = None;
        let mut wstecz = false;
        let mut dalej = false;
        let (moze_wstecz, moze_dalej) = (nav.can_go_back(), nav.can_go_forward());

        let wybor = pasek_czasu(ui, &zegar, catalog, locale);
        // Wolny obszar liczy się **po** pasku czasu: wszystko, co ustawia się samo,
        // ma startować pod nim, a nie na nim.
        let wolne = ui.available_rect_before_wrap();

        if let Some(l) = &self.legenda {
            // Prawy dolny róg — tam, gdzie legendę stawia `ui-design.md` §5.
            egui::Area::new(egui::Id::new("magnat.legenda"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        magnat_ui::widgets::overlay_legend(
                            ui,
                            theme,
                            &catalog.fmt_key(locale, &format!("ui.overlay.{}", l.key), &[]),
                            &l.unit,
                            &l.stops,
                            &l.palette,
                        );
                    });
                });
        }

        if let Some(k) = karta {
            let mut otwarte = true;
            egui::Window::new(catalog.fmt_key(locale, "ui.card.inspection", &[]))
                .open(&mut otwarte)
                .default_pos(wolne.left_top() + egui::vec2(crate::dock::DOK_PX + 12.0, 12.0))
                .default_size(egui::vec2(520.0, 780.0))
                .vscroll(true)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        wstecz = ui
                            .add_enabled(
                                moze_wstecz,
                                egui::Button::new(catalog.fmt_key(locale, "ui.card.back", &[])),
                            )
                            .clicked();
                        dalej = ui
                            .add_enabled(
                                moze_dalej,
                                egui::Button::new(catalog.fmt_key(locale, "ui.card.forward", &[])),
                            )
                            .clicked();
                    });
                    skok = magnat_ui::widgets::inspection_card(
                        ui,
                        theme,
                        k,
                        &mut zakladka_lokalna,
                        catalog,
                        locale,
                    );
                    if let Some(m) = model {
                        match k.tabs.get(zakladka_lokalna).map(|t| t.kind) {
                            Some(magnat_ui::CardTabKind::State) => {
                                magnat_ui::widgets::needs(ui, theme, &m.card);
                            }
                            Some(magnat_ui::CardTabKind::Day) => {
                                magnat_ui::widgets::day_timeline(ui, theme, m, catalog, locale);
                            }
                            _ => {}
                        }
                    }
                });
            zamknij = !otwarte;
        }

        self.zakladka = zakladka_lokalna;

        let akcja_panelu = crate::dock::draw(
            ui,
            theme,
            session,
            &self.ui.catalog,
            self.ui.locale,
            crate::dock::Dok {
                panele: &mut self.panele,
                stop: &mut self.stop,
                samouczek: &mut self.samouczek,
            },
            self.trafienie,
        );

        if zamknij {
            self.pokaz_karte = false;
        }
        if wstecz {
            self.nawiguj(true);
        } else if dalej {
            self.nawiguj(false);
        } else if let Some(s) = skok {
            self.idz_do(s);
        }
        if let magnat_game::PanelAction::Show(s) = akcja_panelu {
            self.idz_do(s);
            self.pokaz_karte = true;
        }
        (wybor, akcja_panelu)
    }
}

/// Pas czasu u góry ekranu: data, zegar i prędkość (`ui-design.md` §5).
///
/// **Panel, a nie pływające `Area`** — i to jest cała różnica. `Area` pozycjonuje się
/// w przestrzeni ekranu i nie rezerwuje niczego, więc pasek wpisany na sztywno w lewy
/// górny róg leżał na pierwszej pozycji doku. `Panel::top` przesuwa kursor rodzica,
/// więc dok rysowany niżej sam zaczyna się pod paskiem — bez dobierania liczb.
///
/// Osobna funkcja, bo test ma ją zawołać bez `Session`.
pub(crate) fn pasek_czasu(
    ui: &mut egui::Ui,
    zegar: &magnat_ui::TimeControlsWidget,
    catalog: &magnat_ui::Catalog,
    locale: Locale,
) -> Option<SimSpeed> {
    egui::Panel::top("magnat.czas")
        .show(ui, |ui| {
            magnat_ui::widgets::time_bar(ui, zegar, catalog, locale)
        })
        .inner
}

#[cfg(test)]
mod tests {
    use magnat_ui::{testing, Catalog, Locale, TimeControlsWidget};

    /// Pasek czasu ma **zabrać** pas u góry, a nie położyć się na doku.
    ///
    /// Test rysuje pasek i zaraz pod nim dok tej samej szerokości co w grze, a potem
    /// pyta o wolny prostokąt. Przed naprawą pierwsze twierdzenie padało: pływające
    /// `Area` nie rezerwuje nic, więc wolny obszar zaczynał się w `y = 0` — dokładnie
    /// tam, gdzie zaczyna się dok.
    #[test]
    fn pasek_czasu_rezerwuje_pas_u_gory() {
        let c = Catalog::load().expect("data/locale/");
        let w = TimeControlsWidget::new(magnat_core::Tick(0));
        let ctx = egui::Context::default();
        let mut wolne = egui::Rect::NOTHING;
        testing::draw_in(&ctx, testing::input(testing::EKRAN), |ui| {
            super::pasek_czasu(ui, &w, &c, Locale::Pl);
            egui::Panel::left("test.dok")
                .resizable(false)
                .exact_size(crate::dock::DOK_PX)
                .show(ui, |ui| {
                    ui.label("dok");
                });
            wolne = ui.available_rect_before_wrap();
        });
        assert!(
            wolne.top() > 20.0,
            "pasek czasu nie zarezerwował pasa u góry: {wolne:?}"
        );
        assert!(
            wolne.left() >= crate::dock::DOK_PX,
            "dok nie zarezerwował szerokości: {wolne:?}"
        );
    }
}
