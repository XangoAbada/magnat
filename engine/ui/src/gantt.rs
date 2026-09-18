//! Gantt: paski w oknie czasu (M9e, `DF-2`).
//!
//! Zlecenia produkcyjne mają okna czasu, dostawy mają terminy, przeglądy maszyn
//! mają daty — i wszystkie trzy odpowiadają na to samo pytanie: **co i kiedy**.
//! Widget rysuje więc paski na osi, nic o nich nie wiedząc.
//!
//! Wirtualizacja idzie po **oknie czasu**, a nie po liczbie pasków: pasek poza
//! oknem nie dotyka ani rysowania, ani układu. Dzięki temu koszt zależy od tego,
//! ile widać, a nie od tego, ile jest — a zleceń przez sto lat gry jest dużo więcej
//! niż pikseli.

use crate::theme::{ColorToken, TextRole, Theme};

/// Jeden pasek: kiedy, jak długo, w którym wierszu i w jakim stanie.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GanttBar {
    /// Wiersz — linia produkcyjna, rampa, maszyna.
    pub row: u16,
    pub label: String,
    /// Minuta gry, w której pasek się zaczyna i kończy.
    pub from: u64,
    pub to: u64,
    pub state: BarState,
}

/// Stan paska. Kolor **i kształt** — pasek opóźniony ma ramkę, nie tylko barwę,
/// bo zrzut w skali szarości ma zostać czytelny (`ui-design.md` §7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BarState {
    Planned,
    Running,
    Done,
    Late,
}

impl BarState {
    const fn token(self) -> ColorToken {
        match self {
            BarState::Planned => ColorToken::LineStrong,
            BarState::Running => ColorToken::Accent,
            BarState::Done => ColorToken::Ok,
            BarState::Late => ColorToken::Danger,
        }
    }
}

/// Paski i okno, w którym się je ogląda.
#[derive(Default)]
pub struct GanttView {
    bars: Vec<GanttBar>,
    rows: u16,
}

impl GanttView {
    pub fn set(&mut self, bars: Vec<GanttBar>) {
        self.rows = bars.iter().map(|b| b.row).max().map_or(0, |m| m + 1);
        self.bars = bars;
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    #[must_use]
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Paski, które wpadają w okno `[from, to]` — wejście testu wirtualizacji.
    #[must_use]
    pub fn visible(&self, from: u64, to: u64) -> Vec<&GanttBar> {
        self.bars
            .iter()
            .filter(|b| b.to >= from && b.from <= to)
            .collect()
    }

    /// Rysuje okno `[from, to]`. Zwraca etykietę klikniętego paska.
    pub fn show(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        from: u64,
        to: u64,
        height: f32,
    ) -> Option<String> {
        let (rect, odp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click(),
        );
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, theme.color(ColorToken::BgCard));
        let rozpietosc = to.saturating_sub(from).max(1) as f32;
        let h = rect.height() / f32::from(self.rows.max(1));
        let x = |t: u64| -> f32 {
            rect.left() + (t.saturating_sub(from) as f32 / rozpietosc) * rect.width()
        };
        let mut klikniety = None;
        for b in self.visible(from, to) {
            let y = rect.top() + f32::from(b.row) * h;
            let r = egui::Rect::from_min_max(
                egui::pos2(x(b.from.max(from)), y + 1.0),
                egui::pos2(x(b.to.min(to)).max(x(b.from.max(from)) + 2.0), y + h - 1.0),
            );
            p.rect_filled(r, 1.0, theme.color(b.state.token()));
            if b.state == BarState::Late {
                p.rect_stroke(
                    r,
                    1.0,
                    egui::Stroke::new(1.0, theme.color(ColorToken::TextPrimary)),
                    egui::StrokeKind::Inside,
                );
            }
            if r.width() > theme.gap(8) {
                p.text(
                    r.left_center() + egui::vec2(2.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &b.label,
                    theme.mono(TextRole::Micro),
                    theme.color(ColorToken::TextPrimary),
                );
            }
            if odp.clicked() && odp.interact_pointer_pos().is_some_and(|q| r.contains(q)) {
                klikniety = Some(b.label.clone());
            }
        }
        klikniety
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasek(od: u64, do_: u64) -> GanttBar {
        GanttBar {
            row: 0,
            label: "x".to_string(),
            from: od,
            to: do_,
            state: BarState::Planned,
        }
    }

    #[test]
    fn okno_odcina_paski_spoza_zakresu() {
        let mut g = GanttView::default();
        g.set(vec![pasek(0, 10), pasek(100, 110), pasek(5, 200)]);
        let widok = g.visible(50, 150);
        assert_eq!(widok.len(), 2, "pasek 0–10 nie należy do okna 50–150");
    }
}
