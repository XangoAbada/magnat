//! Kontrolki wspólne dla ekranów powłoki: kursor klawiatury, lista wyborów,
//! wiersz segmentowy i nagłówek ekranu.
//!
//! Osobny plik, bo to jest **inny temat** niż ekrany: tutaj jest mechanika wejścia
//! i wygląd elementu, tam — treść konkretnego ekranu. Wszystkie cztery kontrolki
//! istnieją, bo powtarzały się w pięciu ekranach, a nie na zapas.

use super::{Focus, Shell};

/// Klawisze nawigacji, odczytane raz na klatkę.
pub(crate) struct Keys {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub enter: bool,
    pub esc: bool,
}

impl Keys {
    pub(crate) fn read(ui: &egui::Ui) -> Keys {
        ui.input(|i| Keys {
            up: i.key_pressed(egui::Key::ArrowUp),
            down: i.key_pressed(egui::Key::ArrowDown),
            left: i.key_pressed(egui::Key::ArrowLeft),
            right: i.key_pressed(egui::Key::ArrowRight),
            enter: i.key_pressed(egui::Key::Enter),
            esc: i.key_pressed(egui::Key::Escape),
        })
    }

    /// Przesuwa kursor w pionie bez zawijania. Bez zawijania, bo przytrzymana
    /// strzałka na końcu listy ma się zatrzymać, a nie wrócić na początek —
    /// zawijanie w liście pięciu pozycji czyta się jak przypadek, nie jak nawigacja.
    pub(crate) fn move_focus(&self, focus: &mut Focus, len: usize) {
        if len == 0 {
            return;
        }
        if self.up && *focus > 0 {
            *focus -= 1;
        }
        if self.down && *focus + 1 < len {
            *focus += 1;
        }
        *focus = (*focus).min(len - 1);
    }
}

/// Pionowa lista wyborów: kursor, Enter i klik myszą.
///
/// Zwraca indeks pozycji, którą gracz właśnie zatwierdził.
pub(crate) fn menu_list(
    shell: &Shell,
    ui: &mut egui::Ui,
    items: &[String],
    focus: &mut Focus,
    keys: &Keys,
) -> Option<usize> {
    use magnat_ui::{ColorToken, TextRole};
    keys.move_focus(focus, items.len());
    let mut wybrane = None;
    for (i, etykieta) in items.iter().enumerate() {
        let aktywny = i == *focus;
        let tekst = egui::RichText::new(etykieta)
            .font(shell.theme.font(TextRole::Title))
            .color(if aktywny {
                shell.theme.color(ColorToken::TextPrimary)
            } else {
                shell.theme.color(ColorToken::TextSecondary)
            });
        // Szerokość z dostępnej, nie z literału: przy skali 3,0 na oknie 1600 px
        // logiczny ekran ma ~530 punktów i przycisk 280-punktowy byłby wtedy
        // szeroki na połowę widoku, a slot 640-punktowy po prostu by z niego wyszedł.
        let szerokosc = ui.available_width().clamp(80.0, 280.0);
        let odp = ui.add_sized(
            egui::vec2(szerokosc, shell.theme.gap(8)),
            egui::Button::new(tekst).fill(if aktywny {
                shell.theme.color(ColorToken::Accent)
            } else {
                shell.theme.color(ColorToken::BgCard)
            }),
        );
        if odp.clicked() {
            *focus = i;
            wybrane = Some(i);
        }
    }
    if keys.enter && !items.is_empty() {
        wybrane = Some(*focus);
    }
    wybrane
}

/// Wiersz kreatora: etykieta, segmenty wartości, zdanie o tym, co ta wartość zmienia.
///
/// Zwraca `true`, gdy wartość się zmieniła. Strzałki lewo/prawo działają wyłącznie
/// na wierszu pod kursorem — dzięki temu klawiatura obsługuje cały ekran jedną ręką.
#[allow(clippy::too_many_arguments)]
pub(crate) fn segment_row<T: Copy + PartialEq>(
    shell: &Shell,
    ui: &mut egui::Ui,
    label: &str,
    hint: Option<&str>,
    values: &[T],
    current: &mut T,
    name: impl Fn(T) -> String,
    aktywny: bool,
    keys: &Keys,
) -> bool {
    use magnat_ui::{ColorToken, TextRole};
    let przed = values.iter().position(|v| *v == *current).unwrap_or(0);
    let mut i = przed;
    ui.horizontal_wrapped(|ui| {
        ui.add_sized(
            egui::vec2(ui.available_width().min(120.0), shell.theme.gap(6)),
            egui::Label::new(
                egui::RichText::new(label)
                    .font(shell.theme.font(TextRole::Strong))
                    .color(if aktywny {
                        shell.theme.color(ColorToken::AccentHi)
                    } else {
                        shell.theme.color(ColorToken::TextSecondary)
                    }),
            ),
        );
        for (j, v) in values.iter().enumerate() {
            let zaznaczony = j == przed;
            let tekst = egui::RichText::new(name(*v))
                .font(shell.theme.font(TextRole::Body))
                .color(if zaznaczony {
                    shell.theme.color(ColorToken::TextPrimary)
                } else {
                    shell.theme.color(ColorToken::TextSecondary)
                });
            if ui.selectable_label(zaznaczony, tekst).clicked() {
                i = j;
            }
        }
    });
    if let Some(h) = hint {
        ui.label(
            egui::RichText::new(h)
                .font(shell.theme.font(TextRole::Micro))
                .color(shell.theme.color(ColorToken::TextSecondary)),
        );
    }
    ui.add_space(shell.theme.gap(2));

    if aktywny {
        if keys.left && i > 0 {
            i -= 1;
        }
        if keys.right && i + 1 < values.len() {
            i += 1;
        }
    }
    if i != przed {
        *current = values[i];
        return true;
    }
    false
}

/// Tytuł ekranu powłoki plus podpowiedź o klawiszach.
pub(crate) fn header(shell: &Shell, ui: &mut egui::Ui, title: &str) {
    use magnat_ui::{ColorToken, TextRole};
    ui.label(
        egui::RichText::new(title)
            .font(shell.theme.font(TextRole::Screen))
            .color(shell.theme.color(ColorToken::TextPrimary)),
    );
    ui.label(
        egui::RichText::new(shell.text("ui.shell.hint_keys"))
            .font(shell.theme.font(TextRole::Micro))
            .color(shell.theme.color(ColorToken::TextSecondary)),
    );
    ui.add_space(shell.theme.gap(4));
}
