//! Lista slotów — wczytanie i zapis (`ui-design.md` §6.4).
//!
//! Dwie rzeczy, które ten ekran musi robić i które łatwo pominąć:
//!
//! 1. **Nagłówek jest tani, wczytanie nie** (`DB-3`). Lista czyta wyłącznie
//!    `slot-N.meta.ron` — kilkaset bajtów — i nie ma prawa wczytać dziesięciu światów.
//!    „Wczytaj" przewija za to dziennik wejść, więc kosztuje tyle, ile kosztowała
//!    rozgrywka; ekran mówi to wprost, zamiast kręcić kręciołkiem.
//! 2. **Zapis niezgodny wersją jest widoczny i opisany**, nie ukryty — zapis, który
//!    zniknął z listy, czyta się jako utracona gra.

use magnat_ui::{ColorToken, TextRole};

use super::{header, Keys, Shell, ShellAction};
use crate::save::{SaveError, SaveSlot};
use crate::shell::ShellScreen;

/// Po co gracz tu wszedł. Jedna lista, dwa tryby — pytanie „który slot" jest to samo.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Load,
    Save,
}

pub(super) fn list(shell: &mut Shell, ui: &mut egui::Ui) -> Option<ShellAction> {
    let keys = Keys::read(ui);
    let mode = shell.slot_mode;
    let tytul = shell.text(match mode {
        Mode::Load => "ui.slots.load_title",
        Mode::Save => "ui.slots.save_title",
    });
    header(shell, ui, &tytul);

    let ile = shell.slots.len();
    keys.move_focus(&mut shell.focus, ile);
    let kursor = shell.focus.min(ile.saturating_sub(1));

    let mut klikniety = None;
    for i in 0..ile {
        let (id, stan) = (shell.slots[i].0, shell.slots[i].1.as_ref());
        let aktywny = i == kursor;
        let (wiersz, kolor) = opis_slotu(shell, stan);
        let tekst = egui::RichText::new(format!("{id}  {wiersz}"))
            .font(shell.theme.mono(TextRole::Body))
            .color(kolor);
        let odp = ui.add_sized(
            egui::vec2(ui.available_width().min(640.0), shell.theme.gap(7)),
            egui::Button::new(tekst).fill(if aktywny {
                shell.theme.color(ColorToken::Accent)
            } else {
                shell.theme.color(ColorToken::BgCard)
            }),
        );
        if odp.clicked() {
            klikniety = Some(i);
        }
    }
    if let Some(i) = klikniety {
        shell.focus = i;
    }

    ui.add_space(shell.theme.gap(3));
    if mode == Mode::Load {
        ui.label(
            egui::RichText::new(shell.text("ui.slots.replay_note"))
                .font(shell.theme.font(TextRole::Micro))
                .color(shell.theme.color(ColorToken::TextSecondary)),
        );
    }

    if keys.esc {
        shell.go(if shell.has_session {
            ShellScreen::Pause
        } else {
            ShellScreen::MainMenu
        });
        return None;
    }

    let zatwierdzone = klikniety.is_some() || keys.enter;
    if !zatwierdzone || ile == 0 {
        return None;
    }
    let (id, stan) = (shell.slots[kursor].0, shell.slots[kursor].1.as_ref());
    match mode {
        Mode::Save => Some(ShellAction::SaveSlot(id)),
        // Slotu, którego nie da się odczytać, nie wczytujemy — i to nie jest cisza:
        // powód stoi w wierszu listy, więc gracz wie, czemu nic się nie stało.
        Mode::Load => match stan {
            Some(Ok(_)) => Some(ShellAction::LoadSlot(id)),
            _ => None,
        },
    }
}

fn opis_slotu(
    shell: &Shell,
    stan: Option<&Result<SaveSlot, SaveError>>,
) -> (String, egui::Color32) {
    let l = shell.locale();
    match stan {
        None => (
            shell.text("ui.slots.empty"),
            shell.theme.color(ColorToken::TextDisabled),
        ),
        Some(Err(e)) => {
            let klucz = match e {
                SaveError::SchemaTooOld { .. } => "ui.slots.too_old",
                SaveError::SchemaTooNew { .. } => "ui.slots.too_new",
                _ => "ui.slots.broken",
            };
            (
                shell.fmt(klucz, &[("powod", &e.to_string())]),
                shell.theme.color(ColorToken::Warn),
            )
        }
        Some(Ok(s)) => {
            let kal = magnat_core::SimCalendar::from_minute(s.game_date);
            let godziny = s.played_secs / 3600;
            (
                shell.fmt(
                    "ui.slots.row",
                    &[
                        ("miasto", &s.city),
                        (
                            "data",
                            &magnat_ui::CalendarFmt::date(&shell.catalog, l, kal),
                        ),
                        (
                            "majatek",
                            &magnat_ui::fmt::money(&shell.catalog, l, s.net_worth),
                        ),
                        (
                            "czas",
                            &shell.fmt("ui.slots.hours", &[("godziny", &godziny.to_string())]),
                        ),
                    ],
                ),
                shell.theme.color(ColorToken::TextPrimary),
            )
        }
    }
}
