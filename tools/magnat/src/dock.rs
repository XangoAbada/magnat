//! Dok lewy: panele biznesowe i pas alertów (`M9e` WP10).
//!
//! Osobny plik, bo to jest **inny temat** niż karta mieszkańca: tam gracz pyta
//! „kto to jest", tutaj „co z tym zrobić". `docs/ui-design.md` §5 trzyma tę różnicę
//! przestrzennie — dok lewy należy do tego, co gracz prowadzi, prawy do tego, co
//! kliknął — a zamiana miejscami psuje nawyk. Rozdzielenie plików trzyma ją w kodzie.
//!
//! Widok 3D zostaje widoczny zawsze: panel pełnoekranowy istnieje wyłącznie dla
//! kroniki i edytora reguł.

use magnat_game::{PanelAction, PanelCtx, Panels, Session, StopHit};
use magnat_ui::{Catalog, Locale, Theme};

/// Szerokość doku w punktach logicznych (`ui-design.md` §5).
const DOK_PX: f32 = 320.0;

/// Rysuje dok i pas alertów. Zwraca to, o co poprosił panel.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    theme: &Theme,
    session: &Session,
    catalog: &Catalog,
    locale: Locale,
    panele: &mut Panels,
    trafienie: Option<StopHit>,
) -> PanelAction {
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
            });
        });

    // Pas alertów na dole: **jeden wiersz** o zatrzymaniu, bo to on tłumaczy,
    // czemu gra stanęła. Alert bez możliwej akcji jest wpisem kroniki, nie alertem
    // (`ui-design.md` §4) — ten ma akcję oczywistą: ruszyć zegar.
    if let Some(h) = trafienie {
        egui::Area::new(egui::Id::new("magnat.stop"))
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
    pub(crate) fn arm_stop_conditions(&mut self) {
        if self.stop.is_empty() {
            for c in magnat_game::StopCondition::ready_set() {
                self.stop.arm(c);
            }
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

    /// Podbija stempel źródła — po komendzie gracza panele mają się przebudować.
    pub(crate) fn bump(&mut self, src: magnat_ui::DataSource) {
        self.wersje.bump(src);
        self.panele.bump(src);
    }
}
