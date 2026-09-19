//! Dok lewy: panele biznesowe i pas alertów (`M9e` WP10).
//!
//! Osobny plik, bo to jest **inny temat** niż karta mieszkańca: tam gracz pyta
//! „kto to jest", tutaj „co z tym zrobić". `docs/ui-design.md` §5 trzyma tę różnicę
//! przestrzennie — dok lewy należy do tego, co gracz prowadzi, prawy do tego, co
//! kliknął — a zamiana miejscami psuje nawyk. Rozdzielenie plików trzyma ją w kodzie.
//!
//! Widok 3D zostaje widoczny zawsze: panel pełnoekranowy istnieje wyłącznie dla
//! kroniki i edytora reguł.

use magnat_game::{PanelAction, PanelCtx, Panels, Session, StopHit, StopWatch, ViewCommand};
use magnat_ui::{Catalog, Locale, Theme};

/// Szerokość doku w punktach logicznych (`ui-design.md` §5).
pub(crate) const DOK_PX: f32 = 320.0;

/// Stan widoku, który dok zmienia: panele, warunki zatrzymania i samouczek.
///
/// Jedna paczka zamiast trzech osobnych `&mut` w sygnaturze — wszystkie trzy są polami
/// `Citizens` i żyją dokładnie tak długo, jak ta klatka.
pub(crate) struct Dok<'a> {
    pub panele: &'a mut Panels,
    pub stop: &'a mut StopWatch,
    pub samouczek: &'a mut Option<magnat_game::Tutorial>,
}

/// Rysuje dok i pas alertów. Zwraca to, o co poprosił panel.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    theme: &Theme,
    session: &Session,
    catalog: &Catalog,
    locale: Locale,
    dok: Dok<'_>,
    trafienie: Option<StopHit>,
) -> PanelAction {
    let Dok {
        panele,
        stop,
        samouczek,
    } = dok;
    let holdings = magnat_game::Holdings::of(session);
    let ctx = PanelCtx {
        theme,
        c: catalog,
        l: locale,
        session,
        holdings: &holdings,
        sel: 0,
    };
    let mut akcja = PanelAction::None;
    egui::Panel::left("magnat.panele")
        .resizable(false)
        .exact_size(DOK_PX)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let a = panele.draw(ui, &ctx);
                if a != PanelAction::None {
                    akcja = a;
                }
                warunki(ui, &ctx, panele, stop);
            });
        });

    // Co zostało po pasku czasu i doku — czyli widok 3D. Warstwy kotwiczone liczą
    // kotwicę względem prostokąta z `constrain_to`, a domyślny (`Context::content_rect`)
    // to ekran minus wcięcia systemowe, **nie** minus panele. Bez tego „u góry na środku"
    // znaczyłoby „na pasku czasu".
    let wolne = ui.available_rect_before_wrap();

    pasek_samouczka(ui, &ctx, samouczek, wolne);

    // Pas alertów na dole: **jeden wiersz** o zatrzymaniu, bo to on tłumaczy,
    // czemu gra stanęła. Alert bez możliwej akcji jest wpisem kroniki, nie alertem
    // (`ui-design.md` §4) — ten ma akcję oczywistą: ruszyć zegar.
    if let Some(h) = trafienie {
        egui::Area::new(egui::Id::new("magnat.stop"))
            .constrain_to(wolne)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let co = ctx.text(&format!("ui.stop.{}", h.cond.key()));
                    ui.label(ctx.fmt("ui.stop.hit", &[("co", &co)]));
                });
            });
    }
    akcja
}

/// Pasek samouczka u góry ekranu (`DI-35`).
///
/// **Nie jest oknem modalnym** i to jest warunek z §5.12 pkt 4, nie estetyka: świat
/// pod paskiem tyka, a każde kliknięcie w mapę dochodzi tam, gdzie zawsze. Dwa
/// przyciski, bo pomijalny ma być **krok** i **całość**.
fn pasek_samouczka(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    samouczek: &mut Option<magnat_game::Tutorial>,
    wolne: egui::Rect,
) {
    let Some(t) = samouczek.as_mut() else {
        return;
    };
    let Some(krok) = t.step() else {
        return;
    };
    egui::Area::new(egui::Id::new("magnat.tutorial"))
        .constrain_to(wolne)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 12.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(520.0);
                ui.label(
                    egui::RichText::new(ctx.text(&format!("ui.tutorial.{}.title", krok.key())))
                        .font(ctx.theme.font(magnat_ui::TextRole::Title)),
                );
                ui.label(
                    egui::RichText::new(ctx.text(&format!("ui.tutorial.{}.hint", krok.key())))
                        .font(ctx.theme.font(magnat_ui::TextRole::Body)),
                );
                ui.horizontal(|ui| {
                    if ui.button(ctx.text("ui.tutorial.skip_step")).clicked() {
                        t.skip_step();
                    }
                    if ui.button(ctx.text("ui.tutorial.skip_all")).clicked() {
                        t.skip_all();
                    }
                });
            });
        });
}

/// Lista warunków „zatrzymaj, gdy…" (`DI-37`).
///
/// Do domknięcia `WP12` zestaw był uzbrojony na sztywno przy wejściu do świata,
/// a `StopWatch::set` i `ViewCommand::SetStopCondition` nie miały ani jednego
/// wołającego — gracz nie mógł wyłączyć warunku, który mu przeszkadzał. Warunki
/// **wyłączone zostają na liście**: gracz ma je znaleźć tam, gdzie je zostawił.
///
/// Zwinięte domyślnie, bo to jest ustawienie, a nie codzienna decyzja — a dok należy
/// do tego, co gracz prowadzi.
fn warunki(ui: &mut egui::Ui, ctx: &PanelCtx<'_>, panele: &mut Panels, stop: &mut StopWatch) {
    ui.add_space(ctx.theme.gap(4));
    egui::CollapsingHeader::new(ctx.text("ui.stop.list"))
        .default_open(false)
        .show(ui, |ui| {
            for (id, c, on) in stop.armed() {
                let mut wlaczony = on;
                let etykieta = ctx.text(&format!("ui.stop.{}", c.key()));
                if ui.checkbox(&mut wlaczony, etykieta).changed() {
                    stop.set(id, wlaczony);
                    // Do dziennika wejść, tak samo jak otwarcie panelu: strumień
                    // widoku jest tym, z czego liczy się metryka i odtwarza zgłoszenie.
                    panele.push_view(ViewCommand::SetStopCondition { id, on: wlaczony });
                }
            }
        });
}

impl crate::app::App {
    /// Włącza albo wyłącza śledzenie zaznaczonego mieszkańca.
    ///
    /// Cel bierze się z **karty inspekcji**, a nie z osobnego wyboru: gracz już
    /// w kogoś kliknął, a druga lista „kogo śledzić" byłaby drugim zaznaczeniem.
    pub(crate) fn przelacz_sledzenie(&mut self) {
        let Some(s) = self.game.session() else { return };
        let cel = match self.citizens.as_ref().and_then(crate::citizens::Citizens::selected) {
            Some(magnat_core::Subject::Citizen(c)) => Some(magnat_game::FollowTarget::Citizen(c)),
            Some(magnat_core::Subject::Vehicle(v)) => Some(magnat_game::FollowTarget::Vehicle(v)),
            _ => None,
        };
        if let Some(c) = self.citizens.as_mut() {
            let teraz = c.following();
            c.follow(s, if teraz == cel { None } else { cel });
        }
    }

    /// Czas realny sesji w milisekundach. Wolno go tu czytać, bo strumień widoku
    /// **nie wchodzi do symulacji** — zakaz zegara ściennego z 00 §3.5 dotyczy
    /// kodu, który liczy świat, a nie tego, który opisuje, co gracz widział.
    pub(crate) fn czas_sesji_ms(&self) -> u64 {
        self.game.session().map_or(0, magnat_game::Session::played_ms)
    }

    /// Wykonuje komendę złożoną w panelu biznesowym.
    ///
    /// Panel **składa** komendę i oddaje ją — wykonuje ją sesja w punkcie
    /// synchronizacji, tą samą drogą co każda inna (§5.2). Odrzucenie nie jest
    /// awarią: panel wygasił przycisk tym samym `precheck`, więc do tego miejsca
    /// dociera wyłącznie to, co miało prawo dojść.
    pub(crate) fn wykonaj_panel(&mut self, akcja: magnat_game::PanelAction) {
        // Najpierw strumień widoku: otwarcie panelu jest interakcją, którą liczy
        // metryka §20.3, a liczy ją **z dziennika** — więc dziennik musi ją dostać
        // niezależnie od tego, czy gracz cokolwiek zmienił w świecie.
        let zdarzenia = self
            .citizens
            .as_mut()
            .map(|c| c.panele.take_view_events())
            .unwrap_or_default();
        if !zdarzenia.is_empty() {
            let ms = self.czas_sesji_ms();
            if let Some(s) = self.game.session_mut() {
                for z in zdarzenia {
                    s.record_view(ms, z);
                }
            }
        }
        let magnat_game::PanelAction::Command(cmd) = akcja else {
            return;
        };
        if let Some(s) = self.game.session_mut() {
            let _ = s.submit(*cmd);
        }
        if let Some(c) = self.citizens.as_mut() {
            c.bump(magnat_ui::DataSource::Market);
            c.bump(magnat_ui::DataSource::World);
        }
    }
}

impl crate::citizens::Citizens {
    /// Uzbraja gotowy zestaw warunków zatrzymania (§5.10). Wołane raz, po wejściu
    /// do świata: lista jest **zestawem**, a nie wyrażeniem, więc nie ma czego budować.
    ///
    /// W trybie przeglądu warunki **biznesowe się nie zbroją** i to nie jest wyjątek,
    /// tylko wykonanie ich definicji: obserwator nie ma interesu, więc „gotówka poniżej
    /// progu" jest u niego prawdą od pierwszej minuty i zatrzymuje świat na zawsze —
    /// zamiast ostrzec, blokuje jedyną rzecz, po którą się do tego trybu wchodzi.
    /// Zostają zdarzenia miejskie, bo te dotyczą miasta, a nie gracza.
    pub(crate) fn arm_stop_conditions(&mut self, observe: bool) {
        if !self.stop.is_empty() {
            return;
        }
        for c in magnat_game::StopCondition::ready_set() {
            if observe && !matches!(c, magnat_game::StopCondition::CityEvent) {
                continue;
            }
            self.stop.arm(c);
        }
    }

    /// Ustawia cel trybu „śledź" i przypina go do warstwy Mikro.
    pub(crate) fn follow(
        &mut self,
        session: &Session,
        target: Option<magnat_game::FollowTarget>,
    ) {
        self.follow.set(session, target);
    }

    /// Kogo śledzi kamera.
    pub(crate) fn following(&self) -> Option<magnat_game::FollowTarget> {
        self.follow.target()
    }

    /// Uruchamia samouczek, jeśli scenariusz tej gry o niego prosi (`DI-35`).
    /// Wołane raz, po wejściu do świata — tak samo jak uzbrojenie warunków.
    pub(crate) fn start_tutorial(&mut self, session: &Session) {
        self.samouczek = magnat_game::Tutorial::start(session);
        if let Some(v) = self.samouczek.and_then(|t| t.speed()) {
            self.set_speed(v);
        }
    }

    /// Przesuwa samouczek faktami ze świata i z widoku.
    ///
    /// Postęp składa się z dwóch źródeł, bo takie są kroki: śledzenie i nakładka
    /// należą do widoku, a otwarty sklep z ceną do sesji. `game::tutorial` nie
    /// zagląda do klienta — dostaje trzy bity i tyle mu wystarcza.
    pub(crate) fn tutorial_tick(&mut self, session: &Session) {
        let Some(t) = self.samouczek.as_mut() else {
            return;
        };
        let mut p = magnat_game::TutorialProgress::of(session);
        p.following_self = match (self.follow.target(), session.player()) {
            (Some(magnat_game::FollowTarget::Citizen(c)), Some(g)) => c == g.citizen,
            _ => false,
        };
        p.catchment_shown = self.zasieg_wlaczony;
        t.advance(&p);
        // Skończony samouczek **znika**, zamiast stać jako `Some` do końca gry:
        // `TutorialProgress::of` przechodzi cały dziennik wejść, więc trzymanie go
        // przy życiu kosztowałoby coraz więcej za odpowiedź, która już się nie zmieni.
        if t.is_done() {
            self.samouczek = None;
        }
    }

    /// Podbija stempel źródła — po komendzie gracza panele mają się przebudować.
    pub(crate) fn bump(&mut self, src: magnat_ui::DataSource) {
        self.wersje.bump(src);
        self.panele.bump(src);
    }
}
