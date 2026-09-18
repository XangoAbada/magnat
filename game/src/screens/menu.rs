//! Menu główne i menu pauzy (`ui-design.md` §6.1 i §6.6).
//!
//! Menu pauzy to ta sama lista co menu główne minus „Nowa gra", plus „Wróć do gry"
//! i „Zapisz". Jedna funkcja składa obie, bo to jest ta sama lista, a nie dwie podobne.

use magnat_ui::{ColorToken, TextRole};

use super::slots::Mode;
use super::{header, menu_list, Keys, Shell, ShellAction};
use crate::shell::{SettingsTab, ShellScreen};

/// Pozycja menu. Enum, a nie indeks, bo lista jest różna w menu i w pauzie —
/// indeks 2 znaczyłby wtedy dwie różne rzeczy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Poz {
    Continue,
    NewGame,
    Load,
    Save,
    Settings,
    ToMenu,
    Quit,
}

impl Poz {
    const fn key(self) -> &'static str {
        match self {
            Poz::Continue => "ui.shell.continue",
            Poz::NewGame => "ui.shell.new_game",
            Poz::Load => "ui.shell.load",
            Poz::Save => "ui.shell.save",
            Poz::Settings => "ui.shell.settings",
            Poz::ToMenu => "ui.shell.to_menu",
            Poz::Quit => "ui.shell.quit",
        }
    }
}

pub(super) fn main_menu(shell: &mut Shell, ui: &mut egui::Ui) -> Option<ShellAction> {
    let keys = Keys::read(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(shell.theme.gap(12));
        ui.label(
            egui::RichText::new(shell.text("ui.shell.title"))
                .font(shell.theme.font(TextRole::Hero))
                .color(shell.theme.color(ColorToken::TextPrimary)),
        );
        ui.label(
            egui::RichText::new(shell.text("ui.shell.subtitle"))
                .font(shell.theme.font(TextRole::Body))
                .color(shell.theme.color(ColorToken::TextSecondary)),
        );
        ui.add_space(shell.theme.gap(8));
    });

    // „Kontynuuj" jest **ukryte**, gdy nie ma czego kontynuować — wyszarzona pozycja
    // bez powodu jest ślepym zaułkiem (`ui-design.md` §1 pkt 3).
    let mut pozycje = Vec::new();
    if shell.has_session {
        pozycje.push(Poz::Continue);
    }
    pozycje.extend([Poz::NewGame, Poz::Load, Poz::Settings, Poz::Quit]);

    let wybor = lista(shell, ui, &pozycje, &keys);

    ui.add_space(shell.theme.gap(6));
    // Wersja gry i wersja formatu zapisu są widoczne, bo od nich zaczyna się
    // każde zgłoszenie błędu (`ui-design.md` §6.1).
    ui.label(
        egui::RichText::new(shell.fmt(
            "ui.shell.version",
            &[
                ("wersja", env!("CARGO_PKG_VERSION")),
                ("format", &crate::save::SAVE_SCHEMA_VERSION.to_string()),
            ],
        ))
        .font(shell.theme.font(TextRole::Micro))
        .color(shell.theme.color(ColorToken::TextSecondary)),
    );

    wykonaj(shell, wybor)
}

pub(super) fn pause_menu(shell: &mut Shell, ui: &mut egui::Ui) -> Option<ShellAction> {
    let keys = Keys::read(ui);
    let tytul = shell.text("ui.shell.pause_title");
    header(shell, ui, &tytul);
    let pozycje = [
        Poz::Continue,
        Poz::Save,
        Poz::Load,
        Poz::Settings,
        Poz::ToMenu,
    ];
    let wybor = lista(shell, ui, &pozycje, &keys);
    // Esc z pauzy wraca do gry — tą samą drogą, którą się w nią weszło.
    if keys.esc {
        return Some(ShellAction::Resume);
    }
    wykonaj(shell, wybor)
}

fn lista(shell: &mut Shell, ui: &mut egui::Ui, pozycje: &[Poz], keys: &Keys) -> Option<Poz> {
    let etykiety: Vec<String> = pozycje.iter().map(|p| shell.text(p.key())).collect();
    let mut kursor = shell.focus;
    let wybor = menu_list(shell, ui, &etykiety, &mut kursor, keys).map(|i| pozycje[i]);
    shell.focus = kursor;
    wybor
}

fn wykonaj(shell: &mut Shell, poz: Option<Poz>) -> Option<ShellAction> {
    let numery: Vec<u8> = shell.slots.iter().map(|(i, _)| *i).collect();
    match poz? {
        Poz::Continue => Some(ShellAction::Resume),
        Poz::NewGame => {
            let draft = shell.draft;
            shell.go(ShellScreen::NewGame { draft });
            None
        }
        // Wczytanie i zapis dzielą jedną listę slotów — różni je wyłącznie tryb,
        // bo pytanie „który slot" jest w obu przypadkach to samo.
        Poz::Load => {
            shell.slot_mode = Mode::Load;
            shell.go(ShellScreen::Load {
                slots: numery,
                selected: None,
            });
            None
        }
        Poz::Save => {
            shell.slot_mode = Mode::Save;
            shell.go(ShellScreen::Load {
                slots: numery,
                selected: None,
            });
            None
        }
        Poz::Settings => {
            shell.go(ShellScreen::Settings {
                tab: SettingsTab::Game,
            });
            None
        }
        Poz::ToMenu => Some(ShellAction::ToMenu),
        Poz::Quit => Some(ShellAction::Quit),
    }
}
