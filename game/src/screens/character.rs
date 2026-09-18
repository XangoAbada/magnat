//! Ekran wyboru postaci (M9c §5.3, WP4; PRD §13.1, §14.7).
//!
//! Ostatni ekran przed grą i pierwszy, na którym gracz widzi ludzi, a nie parametry.
//! Lista jest krótka z rozmysłu: to jest wybór postaci, a nie spis ludności.
//!
//! Kandydat to **zwykły mieszkaniec** wygenerowanego miasta — z domem, rodziną,
//! pracą i oszczędnościami, które już ma. Predykat wariantu startu decyduje, kto
//! w ogóle wchodzi na listę (`StartVariant::accepts`), a nie co dostanie po wyborze.

use magnat_ui::{ColorToken, TextRole};

use super::{header, menu_list, Keys, Shell, ShellAction};

pub(super) fn screen(shell: &mut Shell, ui: &mut egui::Ui) -> Option<ShellAction> {
    let keys = Keys::read(ui);
    let tytul = shell.text("ui.character.title");
    header(shell, ui, &tytul);

    ui.label(
        egui::RichText::new(shell.fmt(
            "ui.character.variant",
            &[(
                "wariant",
                &shell.text(&format!("ui.variant.{:?}", shell.draft.variant)),
            )],
        ))
        .font(shell.theme.font(TextRole::Body))
        .color(shell.theme.color(ColorToken::TextSecondary)),
    );
    ui.add_space(shell.theme.gap(4));

    ui.label(
        egui::RichText::new(shell.text("ui.character.observe.hint"))
            .font(shell.theme.font(TextRole::Micro))
            .color(shell.theme.color(ColorToken::TextSecondary)),
    );
    ui.add_space(shell.theme.gap(2));

    if shell.candidates.is_empty() {
        // Świat bez kandydata nie jest błędem — jest światem, w którym nikt nie
        // spełnia predykatu wariantu. Gracz ma wtedy jedno wyjście i ono jest widoczne.
        ui.label(
            egui::RichText::new(shell.text("ui.character.none"))
                .font(shell.theme.font(TextRole::Body))
                .color(shell.theme.color(ColorToken::Warn)),
        );
    }

    let mut pozycje: Vec<String> = shell
        .candidates
        .iter()
        .map(|k| {
            shell.fmt(
                "ui.character.row",
                &[
                    ("imie", &k.name),
                    ("wiek", &k.age_years.to_string()),
                    (
                        "praca",
                        &shell.text(if k.employed {
                            "ui.character.employed"
                        } else {
                            "ui.character.jobless"
                        }),
                    ),
                    ("oszczednosci", &magnat_ui::zlotowki(k.savings)),
                    ("osob", &k.household_size.to_string()),
                ],
            )
        })
        .collect();
    let losuj = pozycje.len();
    pozycje.push(shell.text("ui.character.random"));
    let obserwuj = pozycje.len();
    pozycje.push(shell.text("ui.character.observe"));

    // Kursor trzyma powłoka i wraca do niej po rysowaniu — ten sam wzorzec co
    // w menu głównym, bo `menu_list` bierze `&Shell` i `&mut Focus` osobno.
    let mut kursor = shell.focus;
    let wybor = menu_list(shell, ui, &pozycje, &mut kursor, &keys);
    shell.focus = kursor;

    match wybor {
        Some(i) if i == obserwuj => Some(ShellAction::Observe),
        Some(i) if i == losuj => Some(ShellAction::PickRandomCitizen),
        Some(i) => shell
            .candidates
            .get(i)
            .map(|k| ShellAction::PickCitizen(k.citizen)),
        None => None,
    }
}
