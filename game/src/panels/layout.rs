//! Układ doku i jego zawartość: [`Layout`] i [`Panels`].
//!
//! Osobny plik od [`super`], bo to jest **inny temat**: tam stoi opis panelu —
//! co umie i czego potrzebuje — a tutaj to, które z nich gracz ma przed sobą.
//! Pierwsze jest kontraktem dla M10 i M12, drugie preferencją jednego człowieka.
//!
//! Układ **nie wchodzi do hasha stanu**: zapisuje się w profilu gracza, a nie
//! w zapisie świata (`docs/ui-design.md` §5). Dlatego przypięcie panelu, zwinięcie
//! go i przewinięcie tabeli są tą samą klasą zdarzenia co obrót kamerą.

use magnat_ui::{DataSource, Versions};
use serde::{Deserialize, Serialize};

use super::{PanelAction, PanelCtx, PanelId, PanelRegistry, PanelState};
use crate::career::CareerTier;
use crate::Session;

/// Układ doku lewego. **Preferencja widoku, nie stan świata** — zapisuje się
/// w profilu gracza i nie wchodzi do hasha (`docs/ui-design.md` §5).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Layout {
    /// Panele widoczne na liście doku, w kolejności.
    pub pinned: Vec<PanelId>,
    /// Który panel jest rozwinięty. `None` = sama lista.
    pub active: Option<PanelId>,
}

impl Default for Layout {
    /// Domyślny układ nowego gracza: **dwa panele, nie osiem** (§5.12 pkt 5).
    fn default() -> Layout {
        Layout {
            pinned: vec![PanelId::Shop, PanelId::Dashboard],
            active: Some(PanelId::Shop),
        }
    }
}

impl Layout {
    /// Układ dla etapu kariery: przypięte jest to, czego `min_tier` gracz dobił.
    #[must_use]
    pub fn for_tier(reg: &PanelRegistry, tier: CareerTier) -> Layout {
        let pinned: Vec<PanelId> = reg
            .panels
            .iter()
            .filter(|p| p.min_tier.is_none_or(|m| tier >= m))
            .map(|p| p.id)
            .collect();
        let active = pinned.first().copied();
        Layout { pinned, active }
    }

    /// Przypina panel i rozwija go — to robi kliknięcie gracza.
    pub fn pin(&mut self, id: PanelId) {
        self.pin_quiet(id);
        self.active = Some(id);
    }

    /// Przypina, nie rozwijając. Tak dopina się panel, którego próg kariery gracz
    /// właśnie przekroczył: pojawia się na liście, ale nie zasłania tego, na co
    /// gracz właśnie patrzył.
    pub fn pin_quiet(&mut self, id: PanelId) {
        if !self.pinned.contains(&id) {
            self.pinned.push(id);
        }
    }

    pub fn unpin(&mut self, id: PanelId) {
        self.pinned.retain(|p| *p != id);
        if self.active == Some(id) {
            self.active = self.pinned.first().copied();
        }
    }

    /// Kliknięcie w nazwę panelu: rozwija albo zwija.
    pub fn toggle(&mut self, id: PanelId) {
        if self.active == Some(id) {
            self.active = None;
        } else {
            self.active = Some(id);
        }
    }

    #[must_use]
    pub fn is_pinned(&self, id: PanelId) -> bool {
        self.pinned.contains(&id)
    }
}

/// Dok lewy: rejestr, układ, stany paneli i stemple źródeł.
pub struct Panels {
    pub reg: PanelRegistry,
    pub layout: Layout,
    versions: Versions,
    state: Vec<(PanelId, PanelState)>,
    /// Ostatnia godzina gry, na której podbito stempel zegara. Panele odświeżają się
    /// `EveryHour` (§5.9) i to jest to miejsce, w którym „co godzinę" jest liczbą.
    last_hour: u64,
    /// Etap kariery, przy którym ostatnio odświeżono przypięcia. Bez tego
    /// `min_tier` nie miałby czytelnika, a siedem z dziewięciu paneli byłoby
    /// w grze **nieosiągalnych** — wykryte recenzją przed commitem.
    last_tier: Option<CareerTier>,
    /// Otwarcia i zamknięcia paneli, których nikt jeszcze nie zapisał do strumienia
    /// widoku. **Bez tego metryka onboardingu byłaby przyrządem bez czujnika**:
    /// §5.12 liczy otwarte panele z dziennika, a dziennik zna tylko to, co mu
    /// powiedziano. Kolejka, a nie zapis wprost, bo dok dostaje `&Session`, a pisze
    /// do niego pętla gry.
    view_events: Vec<crate::ViewCommand>,
}

impl Default for Panels {
    fn default() -> Panels {
        Panels {
            reg: PanelRegistry::default(),
            layout: Layout::default(),
            versions: Versions::new(),
            state: Vec::new(),
            last_hour: u64::MAX,
            last_tier: None,
            view_events: Vec::new(),
        }
    }
}

impl Panels {
    /// Podbija stempel źródła — wołane przez klienta po komendzie, zmianie języka
    /// albo zaznaczenia.
    pub fn bump(&mut self, src: DataSource) {
        self.versions.bump(src);
    }

    /// Godzina gry minęła — unieważnia modele zależne od zegara.
    ///
    /// Woła się raz na klatkę i jest jedynym miejscem, w którym „odświeżanie
    /// `EveryHour`" z tabeli §5.9 się dzieje. Przy `X10` to jest co sześć sekund
    /// realnych, a nie co klatkę.
    pub fn sync(&mut self, session: &Session) {
        let h = session.tick().get() / 60;
        if h == self.last_hour {
            return;
        }
        self.last_hour = h;
        self.versions.bump(DataSource::Clock);

        // Etap kariery **dopina** panele, których próg gracz właśnie przekroczył,
        // i nigdy nic nie odpina. Dopina, bo inaczej `min_tier` nie miałby czytelnika
        // i gracz nigdy nie zobaczyłby panelu Finanse; nie odpina, bo to, co gracz
        // sam przypiął, jest jego decyzją — a upadłość nie ma mu zabierać narzędzi.
        let tier = crate::career::CareerTier::derive(session);
        if self.last_tier == Some(tier) {
            return;
        }
        self.last_tier = Some(tier);
        for id in self.reg.ids() {
            let prog = self.reg.get(id).and_then(|d| d.min_tier);
            if prog.is_some_and(|m| tier >= m) {
                self.layout.pin_quiet(id);
            }
        }
    }

    #[must_use]
    pub fn versions(&self) -> &Versions {
        &self.versions
    }

    /// Zdejmuje zdarzenia widoku do zapisania w dzienniku. Pętla gry woła to raz
    /// na klatkę i oddaje je sesji — strumień widoku nie wchodzi do hasha, więc
    /// kolejność zapisu nie ma znaczenia dla determinizmu (§5.5).
    pub fn take_view_events(&mut self) -> Vec<crate::ViewCommand> {
        std::mem::take(&mut self.view_events)
    }

    fn state_of(&mut self, id: PanelId) -> Option<&mut PanelState> {
        let i = self.state.iter().position(|(p, _)| *p == id);
        match i {
            Some(i) => Some(&mut self.state[i].1),
            None => {
                let d = self.reg.get(id)?;
                self.state.push((id, PanelState::new(d)));
                self.state.last_mut().map(|(_, s)| s)
            }
        }
    }

    /// Ile razy panel przeliczył swój model. Wejście testu „klatka bez zmian danych
    /// nie przebudowuje niczego".
    #[must_use]
    pub fn rebuilds(&self, id: PanelId) -> u32 {
        self.state
            .iter()
            .find(|(p, _)| *p == id)
            .map_or(0, |(_, s)| s.rebuilds())
    }

    /// Rysuje dok: listę przypiętych paneli i rozwinięty panel.
    pub fn draw(&mut self, ui: &mut egui::Ui, ctx: &PanelCtx<'_>) -> PanelAction {
        use magnat_ui::{ColorToken, TextRole};
        let mut akcja = PanelAction::None;
        let mut przelacz = None;
        for id in self.layout.pinned.clone() {
            let Some(d) = self.reg.get(id) else {
                continue;
            };
            let aktywny = self.layout.active == Some(id);
            let tytul = ctx.text(d.title);
            let tekst = egui::RichText::new(tytul)
                .font(ctx.theme.font(TextRole::Strong))
                .color(if aktywny {
                    ctx.theme.color(ColorToken::TextPrimary)
                } else {
                    ctx.theme.color(ColorToken::TextSecondary)
                });
            if ui.selectable_label(aktywny, tekst).clicked() {
                przelacz = Some(id);
            }
            if aktywny {
                let a = self.draw_body(ui, ctx, id);
                if a != PanelAction::None {
                    akcja = a;
                }
            }
        }
        // Panele nieprzypięte: jeden wiersz na dole doku. **`min_tier` nie blokuje**
        // (nagłówek `super`), więc każdy panel da się stąd otworzyć od pierwszej
        // minuty — decyduje wyłącznie o tym, co jest przypięte domyślnie.
        let nieprzypiete: Vec<PanelId> = self
            .reg
            .ids()
            .into_iter()
            .filter(|id| !self.layout.is_pinned(*id))
            .collect();
        if !nieprzypiete.is_empty() {
            ui.add_space(ctx.theme.gap(2));
            ui.label(
                egui::RichText::new(ctx.text("ui.panel.more"))
                    .font(ctx.theme.font(TextRole::Micro))
                    .color(ctx.theme.color(ColorToken::TextSecondary)),
            );
            ui.horizontal_wrapped(|ui| {
                for id in nieprzypiete {
                    let Some(d) = self.reg.get(id) else { continue };
                    let tekst = egui::RichText::new(ctx.text(d.title))
                        .font(ctx.theme.font(TextRole::Micro))
                        .color(ctx.theme.color(ColorToken::TextSecondary));
                    if ui.selectable_label(false, tekst).clicked() {
                        przelacz = Some(id);
                    }
                }
            });
        }

        if let Some(id) = przelacz {
            self.layout.pin_quiet(id);
            let bylo = self.layout.active == Some(id);
            self.layout.toggle(id);
            self.view_events.push(if bylo {
                crate::ViewCommand::ClosePanel(id)
            } else {
                crate::ViewCommand::OpenPanel(id)
            });
        }
        akcja
    }

    /// Rysuje sam panel, bez listy — kronika i inne panele pełnoekranowe.
    pub fn draw_body(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &PanelCtx<'_>,
        id: PanelId,
    ) -> PanelAction {
        let Some(d) = self.reg.get(id) else {
            return PanelAction::None;
        };
        let (build, render) = (d.build, d.render);
        let v = self.versions;
        let Some(st) = self.state_of(id) else {
            return PanelAction::None;
        };
        let sel = st.view.sel;
        let ctx = PanelCtx { sel, ..*ctx };
        let (model, view) = st.parts(&v, || build(&ctx));
        render(ui, &ctx, model, view)
    }
}
