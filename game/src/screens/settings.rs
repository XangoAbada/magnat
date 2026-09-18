//! Ustawienia (`ui-design.md` §6.5).
//!
//! **Język i skala interfejsu działają natychmiast, bez restartu** — to jest kryterium
//! akceptacyjne fazy, nie życzenie. Obie wartości siedzą w profilu gracza
//! ([`crate::shell::Settings`]), czyli w pliku obok zapisu, i **nie wchodzą do hasha
//! stanu**: zmiana języka w trakcie gry nie ma prawa ruszyć symulacji ani o minutę.
//!
//! `ponytail:` zakładki są dwie — „Gra" i „Sterowanie" — a nie cztery. Grafika i dźwięk
//! należą do M11 i dziś nie mają ani jednego ustawienia; pusta zakładka jest gorsza
//! od jej braku, bo obiecuje treść (`ui-design.md` §4). Wejdą razem ze swoją zawartością.

use magnat_ui::{ColorToken, Locale, TextRole};

use super::{header, segment_row, Keys, Shell, ShellAction};
use crate::shell::{SettingsTab, ShellScreen};

/// Skale z kryterium §7 dokumentu fazy, w promilach.
const SKALE: [u16; 5] = [750, 1000, 1500, 2000, 3000];

/// Wiersze zakładki „Gra".
const WIERSZE_GRY: usize = 3;

pub(super) fn screen(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    tab: SettingsTab,
) -> Option<ShellAction> {
    let keys = Keys::read(ui);
    let tytul = shell.text("ui.settings.title");
    header(shell, ui, &tytul);

    let zakladki = [SettingsTab::Game, SettingsTab::Controls];
    let mut wybrana = tab;
    let l = shell.locale();
    let c = shell.catalog.clone();
    magnat_ui::tab_strip(ui, &shell.theme, &zakladki, &mut wybrana, |t| match t {
        SettingsTab::Game => c.fmt_key(l, "ui.settings.tab.game", &[]),
        _ => c.fmt_key(l, "ui.settings.tab.controls", &[]),
    });
    if wybrana != tab {
        shell.go(ShellScreen::Settings { tab: wybrana });
        return None;
    }
    ui.separator();

    let mut zmiana = false;
    match tab {
        SettingsTab::Game => {
            keys.move_focus(&mut shell.focus, WIERSZE_GRY);
            let kursor = shell.focus.min(WIERSZE_GRY - 1);
            let mut ustawienia = shell.settings;

            zmiana |= segment_row(
                shell,
                ui,
                &shell.text("ui.settings.locale"),
                None,
                &Locale::ALL,
                &mut ustawienia.locale,
                |v| c.fmt_key(l, &format!("ui.settings.locale.{}", v.code()), &[]),
                kursor == 0,
                &keys,
            );
            zmiana |= segment_row(
                shell,
                ui,
                &shell.text("ui.settings.scale"),
                None,
                &SKALE,
                &mut ustawienia.ui_scale,
                |v| format!("{},{}×", v / 1000, (v % 1000) / 100),
                kursor == 1,
                &keys,
            );
            zmiana |= segment_row(
                shell,
                ui,
                &shell.text("ui.settings.record_view"),
                Some(&shell.text("ui.settings.record_view.hint")),
                &[true, false],
                &mut ustawienia.record_view,
                |v| {
                    c.fmt_key(
                        l,
                        if v {
                            "ui.settings.on"
                        } else {
                            "ui.settings.off"
                        },
                        &[],
                    )
                },
                kursor == 2,
                &keys,
            );
            shell.settings = ustawienia;
        }
        // Lista skrótów z podglądem — czytelna, nieedytowalna. Przypisywanie klawiszy
        // należy do M12 razem z moddingiem wejścia; wypisanie ich teraz kosztuje sześć
        // kluczy i odpowiada na pytanie, które gracz zadaje w pierwszej minucie.
        _ => {
            for k in [
                "ui.keys.camera",
                "ui.keys.move",
                "ui.keys.speed",
                "ui.keys.card",
                "ui.keys.overlay",
                "ui.keys.pause",
            ] {
                ui.label(
                    egui::RichText::new(shell.text(k))
                        .font(shell.theme.mono(TextRole::Body))
                        .color(shell.theme.color(ColorToken::TextPrimary)),
                );
            }
        }
    }

    ui.add_space(shell.theme.gap(4));
    let wstecz = ui.button(shell.text("ui.shell.back")).clicked();
    if wstecz || keys.esc {
        shell.go(if shell.has_session {
            ShellScreen::Pause
        } else {
            ShellScreen::MainMenu
        });
        return None;
    }
    zmiana.then_some(ShellAction::SettingsChanged)
}
