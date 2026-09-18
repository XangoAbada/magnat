//! Elementy, z których składa się panel biznesowy.
//!
//! Lista jest zamknięta w tym sensie, w jakim zamyka ją `docs/ui-design.md` §4:
//! panel składa się **z tych elementów**, a element spoza listy wymaga dopisania go
//! tam, nie wymyślenia go tutaj. Wszystkie powstały, bo powtarzały się w kilku
//! panelach — żaden na zapas.
//!
//! Jedna reguła, która rządzi całym plikiem: **wyłączony przycisk zawsze mówi,
//! czego brakuje**. Powód jest `CommandError`, a nie napis wymyślony przy guziku —
//! ta sama funkcja `precheck`, która odrzuci komendę, wcześniej wygasza przycisk.

use magnat_ui::{ColorToken, TextRole, Theme};

use super::PanelCtx;

/// Tytuł sekcji wewnątrz panelu.
pub(super) fn section(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add_space(theme.gap(2));
    ui.label(
        egui::RichText::new(text)
            .font(theme.font(TextRole::Strong))
            .color(theme.color(ColorToken::TextSecondary)),
    );
}

/// Wiersz „etykieta — wartość". Wartość do prawej, bo jest liczbą.
pub(super) fn row(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .font(theme.font(TextRole::Body))
                .color(theme.color(ColorToken::TextSecondary)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .font(theme.font(TextRole::Body))
                    .color(theme.color(ColorToken::TextPrimary)),
            );
        });
    });
}

/// Kafelek KPI: liczba dużym krojem, pod nią nazwa.
pub(super) fn kpi(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &str, sev: Sev) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(value)
                .font(theme.font(TextRole::Title))
                .color(theme.color(sev.token())),
        );
        ui.label(
            egui::RichText::new(label)
                .font(theme.font(TextRole::Micro))
                .color(theme.color(ColorToken::TextSecondary)),
        );
    });
}

/// Waga liczby albo alertu. Kolor **zawsze z drugim nośnikiem** — tekstem obok —
/// bo zrzut w skali szarości ma zostać czytelny (`ui-design.md` §7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Sev {
    #[default]
    Normal,
    Good,
    Warn,
    Bad,
}

impl Sev {
    pub(super) const fn token(self) -> ColorToken {
        match self {
            Sev::Normal => ColorToken::TextPrimary,
            Sev::Good => ColorToken::Ok,
            Sev::Warn => ColorToken::Warn,
            Sev::Bad => ColorToken::Danger,
        }
    }

    /// Waga z kwoty: ujemna jest zła, dodatnia dobra, zero obojętne.
    #[must_use]
    pub fn of_money(m: magnat_core::Money) -> Sev {
        match m.get() {
            v if v < 0 => Sev::Bad,
            0 => Sev::Normal,
            _ => Sev::Good,
        }
    }
}

/// Przycisk, który **zawsze mówi, czego brakuje**.
///
/// `blokada` to powód z `precheck`. `Some` wygasza przycisk i wiesza powód
/// w podpowiedzi; `None` znaczy „wolno". Zwraca `true` przy kliknięciu.
pub(super) fn action(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    label: &str,
    blokada: Option<String>,
) -> bool {
    let wolno = blokada.is_none();
    let tekst = egui::RichText::new(label)
        .font(ctx.theme.font(TextRole::Body))
        .color(ctx.theme.color(if wolno {
            ColorToken::TextPrimary
        } else {
            ColorToken::TextDisabled
        }));
    let odp = ui.add_enabled(
        wolno,
        egui::Button::new(tekst).fill(ctx.theme.color(if wolno {
            ColorToken::Accent
        } else {
            ColorToken::BgCard
        })),
    );
    if let Some(p) = blokada {
        odp.on_disabled_hover_text(p);
        return false;
    }
    odp.clicked()
}

/// Wiersz alertu: waga, czego dotyczy, co z tym zrobić. Kliknięcie otwiera podmiot.
///
/// Alert bez możliwej akcji jest wpisem kroniki, nie alertem (`ui-design.md` §4) —
/// dlatego każdy wiersz niesie podmiot albo zdanie o tym, co gracz może zrobić.
pub(super) fn alert(ui: &mut egui::Ui, theme: &Theme, sev: Sev, text: &str) -> bool {
    let tekst = egui::RichText::new(text)
        .font(theme.font(TextRole::Body))
        .color(theme.color(sev.token()));
    ui.add(egui::Label::new(tekst).sense(egui::Sense::click()))
        .clicked()
}

/// Punkty bazowe jako procent z dwoma miejscami.
///
/// **Znak bierze się z całości, a nie z części.** `-50 bp` to −0,50 %, ale
/// `-50 / 100 == 0`, więc zapis „część całkowita, przecinek, reszta" gubił minus
/// dla wszystkiego między −1 % a 0 — strata wyglądała jak zysk, a kolor obok
/// pokazywał coś przeciwnego niż liczba. Bez `f64`: to jest liczba, którą gracz
/// porównuje z marżą, a marża jest całkowitoliczbowa (00 §2).
#[must_use]
pub fn percent_bp(bp: i32) -> String {
    let znak = if bp < 0 { "−" } else { "" };
    let a = bp.unsigned_abs();
    format!("{znak}{},{:02}%", a / 100, a % 100)
}

/// Pasek wyboru: do pięciu wariantów segmentami, powyżej lista rozwijana.
///
/// Zwraca `true`, gdy wybór się zmienił.
pub(super) fn picker(
    ui: &mut egui::Ui,
    theme: &Theme,
    items: &[String],
    sel: &mut usize,
) -> bool {
    picker_id(ui, theme, items, sel, "picker")
}

/// To samo, ale z własnym identyfikatorem listy.
///
/// Panel, który ma **dwie** listy rozwijane (łańcuch dostaw: zakłady i towary),
/// musi je rozróżnić — `egui` trzyma stan rozwinięcia pod identyfikatorem, więc
/// dwie listy o tym samym id rozwijają się razem i przestawiają nawzajem.
pub(super) fn picker_id(
    ui: &mut egui::Ui,
    theme: &Theme,
    items: &[String],
    sel: &mut usize,
    id: &str,
) -> bool {
    if items.is_empty() {
        return false;
    }
    *sel = (*sel).min(items.len() - 1);
    let przed = *sel;
    if items.len() <= 5 {
        ui.horizontal_wrapped(|ui| {
            for (i, it) in items.iter().enumerate() {
                let tekst = egui::RichText::new(it)
                    .font(theme.font(TextRole::Body))
                    .color(theme.color(if i == przed {
                        ColorToken::TextPrimary
                    } else {
                        ColorToken::TextSecondary
                    }));
                if ui.selectable_label(i == przed, tekst).clicked() {
                    *sel = i;
                }
            }
        });
    } else {
        egui::ComboBox::from_id_salt(id)
            .selected_text(items[przed].clone())
            .show_ui(ui, |ui| {
                for (i, it) in items.iter().enumerate() {
                    ui.selectable_value(sel, i, it);
                }
            });
    }
    *sel != przed
}
