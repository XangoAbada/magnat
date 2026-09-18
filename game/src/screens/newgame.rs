//! Kreator świata, ekran generacji i podgląd (`ui-design.md` §6.2 i §6.3).
//!
//! **To jest ekran, który zdejmuje z gracza wiersz poleceń.** Komplet parametrów
//! to dokładnie [`WorldGenParams`] plus scenariusz i wariant startu — nic tu nie jest
//! „ustawieniem zaawansowanym" ukrytym za rozwijaczem, bo ukrycie zmusza do CLI.
//!
//! Zakładanie gry jest **dwuetapowe** (`Z-2`): teren i miasto → podgląd i decyzja →
//! zaludnienie. Dlatego podgląd pokazuje **pojemność** miasta, a nie populację:
//! w tym momencie nie istnieje jeszcze ani jeden mieszkaniec i podanie ich liczby
//! byłoby zmyśleniem.

use magnat_ui::{ColorToken, TextRole};
use magnat_world::{Difficulty, EconomyProfile, Epoch, Region, WorldSize};

use super::{header, segment_row, Keys, Shell, ShellAction};
use crate::shell::{
    GenProgress, NewGameParams, ScenarioId, ShellScreen, StartVariant, WorldPreview,
};

/// Wiersze kreatora w kolejności, w jakiej gracz je przechodzi.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Row {
    Seed,
    Size,
    Region,
    Epoch,
    Profile,
    Difficulty,
    Scenario,
    Variant,
}

const ROWS: [Row; 8] = [
    Row::Seed,
    Row::Size,
    Row::Region,
    Row::Epoch,
    Row::Profile,
    Row::Difficulty,
    Row::Scenario,
    Row::Variant,
];

pub(super) fn wizard(shell: &mut Shell, ui: &mut egui::Ui) -> Option<ShellAction> {
    let keys = Keys::read(ui);
    let tytul = shell.text("ui.newgame.title");
    header(shell, ui, &tytul);

    let ShellScreen::NewGame { mut draft } = shell.screen.clone() else {
        return None;
    };
    keys.move_focus(&mut shell.focus, ROWS.len());
    let kursor = ROWS[shell.focus.min(ROWS.len() - 1)];

    for row in ROWS {
        let aktywny = row == kursor;
        wiersz(shell, ui, row, &mut draft, aktywny, &keys);
    }

    ui.add_space(shell.theme.gap(4));
    let generuj = ui
        .add_sized(
            egui::vec2(ui.available_width().min(220.0), shell.theme.gap(8)),
            egui::Button::new(
                egui::RichText::new(shell.text("ui.newgame.generate"))
                    .font(shell.theme.font(TextRole::Title))
                    .color(shell.theme.color(ColorToken::TextPrimary)),
            )
            .fill(shell.theme.color(ColorToken::Accent)),
        )
        .clicked();

    shell.draft = draft;
    shell.screen = ShellScreen::NewGame { draft };

    if keys.esc {
        shell.go(ShellScreen::MainMenu);
        return None;
    }
    // Enter zatwierdza cały ekran, nie wiersz: parametry mają wartości domyślne,
    // więc gracz, który niczego nie zmienia, generuje świat jednym klawiszem.
    if generuj || keys.enter {
        return Some(ShellAction::Generate(draft));
    }
    None
}

fn wiersz(
    shell: &Shell,
    ui: &mut egui::Ui,
    row: Row,
    draft: &mut NewGameParams,
    aktywny: bool,
    keys: &Keys,
) {
    let l = shell.locale();
    let c = &shell.catalog;
    match row {
        // Ziarno nie jest wyborem z listy, więc strzałki je **losują**: na tym wierszu
        // jedyną sensowną zmianą jest „daj inne", a wpisywanie liczby z klawiatury
        // przy pustym ekranie startowym byłoby dla gracza pierwszą czynnością w grze.
        Row::Seed => {
            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    egui::vec2(ui.available_width().min(120.0), shell.theme.gap(6)),
                    egui::Label::new(
                        egui::RichText::new(shell.text("ui.newgame.seed"))
                            .font(shell.theme.font(TextRole::Strong))
                            .color(if aktywny {
                                shell.theme.color(ColorToken::AccentHi)
                            } else {
                                shell.theme.color(ColorToken::TextSecondary)
                            }),
                    ),
                );
                ui.label(
                    egui::RichText::new(format!("{:#018x}", draft.world.seed))
                        .font(shell.theme.mono(TextRole::Body))
                        .color(shell.theme.color(ColorToken::TextPrimary)),
                );
                if ui.button(shell.text("ui.newgame.reroll")).clicked() {
                    draft.world.seed = nastepne_ziarno(draft.world.seed);
                }
            });
            ui.label(
                egui::RichText::new(shell.text("ui.newgame.seed.hint"))
                    .font(shell.theme.font(TextRole::Micro))
                    .color(shell.theme.color(ColorToken::TextSecondary)),
            );
            ui.add_space(shell.theme.gap(2));
            if aktywny && (keys.left || keys.right) {
                draft.world.seed = nastepne_ziarno(draft.world.seed);
            }
        }
        Row::Size => {
            segment_row(
                shell,
                ui,
                &shell.text("ui.newgame.size"),
                Some(&shell.text("ui.newgame.size.hint")),
                WorldSize::ALL,
                &mut draft.world.size,
                |v| c.fmt_key(l, &format!("ui.size.{}", v.key()), &[]),
                aktywny,
                keys,
            );
        }
        Row::Region => {
            segment_row(
                shell,
                ui,
                &shell.text("ui.newgame.region"),
                Some(&shell.text("ui.newgame.region.hint")),
                Region::ALL,
                &mut draft.world.region,
                |v| c.fmt_key(l, &format!("ui.region.{}", v.key()), &[]),
                aktywny,
                keys,
            );
        }
        Row::Epoch => {
            segment_row(
                shell,
                ui,
                &shell.text("ui.newgame.epoch"),
                Some(&shell.text("ui.newgame.epoch.hint")),
                Epoch::ALL,
                &mut draft.world.epoch,
                |v| v.key().to_string(),
                aktywny,
                keys,
            );
        }
        Row::Profile => {
            segment_row(
                shell,
                ui,
                &shell.text("ui.newgame.profile"),
                Some(&shell.text("ui.newgame.profile.hint")),
                EconomyProfile::ALL,
                &mut draft.world.profile,
                |v| c.fmt_key(l, &format!("ui.profile.{}", v.key()), &[]),
                aktywny,
                keys,
            );
        }
        Row::Difficulty => {
            segment_row(
                shell,
                ui,
                &shell.text("ui.newgame.difficulty"),
                Some(&shell.text("ui.newgame.difficulty.hint")),
                Difficulty::ALL,
                &mut draft.world.difficulty,
                |v| c.fmt_key(l, &format!("ui.difficulty.{}", v.key()), &[]),
                aktywny,
                keys,
            );
        }
        // Scenariusze idą z `data/scenarios/scenarios.ron` (`K-71`). Podpowiedź pod
        // wierszem to **opis wybranego**, a nie jedno zdanie o samym wyborze:
        // gracz ma przeczytać, w co się pakuje, zanim naciśnie „Generuj".
        Row::Scenario => {
            let lista: Vec<ScenarioId> = shell.scenarios.iter().map(|(id, _, _)| *id).collect();
            let opis = shell
                .scenarios
                .iter()
                .find(|(id, _, _)| *id == draft.scenario)
                .map_or_else(
                    || shell.text("ui.newgame.scenario.hint"),
                    |(_, _, brief)| c.fmt_key(l, brief, &[]),
                );
            let tytuly = shell.scenarios.clone();
            segment_row(
                shell,
                ui,
                &shell.text("ui.newgame.scenario"),
                Some(&opis),
                &lista,
                &mut draft.scenario,
                |v| {
                    tytuly
                        .iter()
                        .find(|(id, _, _)| *id == v)
                        .map_or_else(String::new, |(_, t, _)| c.fmt_key(l, t, &[]))
                },
                aktywny,
                keys,
            );
        }
        Row::Variant => {
            let warianty = [
                StartVariant::Graduate,
                StartVariant::Worker,
                StartVariant::Heir,
                StartVariant::Investor,
                StartVariant::Sandbox,
            ];
            let hint = c.fmt_key(l, &format!("ui.variant.{:?}.hint", draft.variant), &[]);
            segment_row(
                shell,
                ui,
                &shell.text("ui.newgame.variant"),
                Some(&hint),
                &warianty,
                &mut draft.variant,
                |v| c.fmt_key(l, &format!("ui.variant.{v:?}"), &[]),
                aktywny,
                keys,
            );
        }
    }
}

/// Kolejne ziarno z bieżącego. Funkcja czysta, więc „losuj" jest powtarzalne —
/// gracz, który zapamiętał ziarno, dostanie ten sam świat, a test nie potrzebuje
/// źródła losowości spoza świata (00 §3.5 zabrania zegara ściennego w kodzie gry).
fn nastepne_ziarno(seed: u64) -> u64 {
    magnat_core::mix64(seed ^ 0x9E37_79B9_7F4A_7C15)
}

pub(super) fn generating(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    progress: &GenProgress,
) -> Option<ShellAction> {
    let keys = Keys::read(ui);
    let opis = opis_swiata(shell, &shell.draft);
    let tytul = shell.fmt("ui.gen.title", &[("opis", &opis)]);
    header(shell, ui, &tytul);

    let (done, total) = (progress.done(), progress.total());
    let ulamek = f32::from(done) / f32::from(total.max(1));
    ui.add(
        egui::ProgressBar::new(ulamek)
            .fill(shell.theme.color(ColorToken::Accent))
            .show_percentage(),
    );
    // Nazwa etapu pochodzi z `PASSES`, a nie z listy przepisanej tutaj: pasek ma mówić
    // prawdę o tym, co się dzieje, a nie animować się dla wrażenia (`Z-4`).
    ui.label(
        egui::RichText::new(shell.fmt(
            "ui.gen.step",
            &[
                ("etap", progress.pass_name()),
                ("done", &done.to_string()),
                ("total", &total.to_string()),
            ],
        ))
        .font(shell.theme.font(TextRole::Body))
        .color(shell.theme.color(ColorToken::TextSecondary)),
    );
    ui.add_space(shell.theme.gap(4));

    let anuluj = ui.button(shell.text("ui.gen.cancel")).clicked();
    if anuluj || keys.esc {
        return Some(ShellAction::CancelGeneration);
    }
    None
}

pub(super) fn preview(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    p: &WorldPreview,
    mapa: Option<&egui::TextureHandle>,
) -> Option<ShellAction> {
    let keys = Keys::read(ui);
    let tytul = shell.text("ui.preview.title");
    header(shell, ui, &tytul);

    let l = shell.locale();
    ui.horizontal_top(|ui| {
        if let Some(t) = mapa {
            let bok = ui.available_width().min(360.0);
            ui.add(egui::Image::new((t.id(), egui::vec2(bok, bok))));
            ui.add_space(shell.theme.gap(6));
        }
        ui.vertical(|ui| {
            let wiersze: [(&str, String); 5] = [
                (
                    "ui.preview.homes",
                    magnat_ui::fmt::integer(l, i64::from(p.homes)),
                ),
                (
                    "ui.preview.jobs",
                    magnat_ui::fmt::integer(l, i64::from(p.jobs)),
                ),
                (
                    "ui.preview.area",
                    magnat_ui::fmt::integer(l, i64::from(p.city_area_km2)),
                ),
                (
                    "ui.preview.districts",
                    magnat_ui::fmt::integer(l, i64::from(p.districts)),
                ),
                (
                    "ui.preview.firms",
                    magnat_ui::fmt::integer(l, i64::from(p.firms)),
                ),
            ];
            for (klucz, wartosc) in wiersze {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        egui::vec2(ui.available_width().min(180.0), shell.theme.gap(6)),
                        egui::Label::new(
                            egui::RichText::new(shell.text(klucz))
                                .font(shell.theme.font(TextRole::Body))
                                .color(shell.theme.color(ColorToken::TextSecondary)),
                        ),
                    );
                    ui.label(
                        egui::RichText::new(wartosc)
                            .font(shell.theme.mono(TextRole::Body))
                            .color(shell.theme.color(ColorToken::TextPrimary)),
                    );
                });
            }
            let zloza: Vec<String> = p
                .deposits
                .iter()
                .take(5)
                .map(|(k, _)| {
                    shell
                        .catalog
                        .fmt_key(l, &format!("ui.resource.{}", k.name()), &[])
                })
                .collect();
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(shell.text("ui.preview.deposits"))
                        .font(shell.theme.font(TextRole::Body))
                        .color(shell.theme.color(ColorToken::TextSecondary)),
                );
                ui.label(
                    egui::RichText::new(zloza.join(", "))
                        .font(shell.theme.font(TextRole::Body))
                        .color(shell.theme.color(ColorToken::TextPrimary)),
                );
            });
        });
    });

    ui.add_space(shell.theme.gap(3));
    // Zdanie, którego nie wolno pominąć: te liczby to pojemność, nie populacja.
    ui.label(
        egui::RichText::new(shell.text("ui.preview.capacity_note"))
            .font(shell.theme.font(TextRole::Micro))
            .color(shell.theme.color(ColorToken::TextSecondary)),
    );
    ui.add_space(shell.theme.gap(4));

    let pozycje = ["ui.preview.play", "ui.preview.reroll", "ui.preview.params"];
    let etykiety: Vec<String> = pozycje.iter().map(|k| shell.text(k)).collect();
    let mut kursor = shell.focus;
    let wybor = super::menu_list(shell, ui, &etykiety, &mut kursor, &keys);
    shell.focus = kursor;

    if keys.esc {
        return Some(ShellAction::BackToWizard);
    }
    match wybor? {
        0 => Some(ShellAction::PlayHere),
        1 => Some(ShellAction::Reroll),
        _ => Some(ShellAction::BackToWizard),
    }
}

/// „ziarno 0x…, 8 km, rzeczny, 1990, mieszany" — jedno zdanie o świecie,
/// wspólne dla ekranu generacji i dla wiersza slotu.
pub(super) fn opis_swiata(shell: &Shell, p: &NewGameParams) -> String {
    let l = shell.locale();
    let c = &shell.catalog;
    c.fmt_key(
        l,
        "ui.slots.world",
        &[
            ("ziarno", &format!("{:#x}", p.world.seed)),
            (
                "rozmiar",
                &c.fmt_key(l, &format!("ui.size.{}", p.world.size.key()), &[]),
            ),
            (
                "region",
                &c.fmt_key(l, &format!("ui.region.{}", p.world.region.key()), &[]),
            ),
            ("epoka", p.world.epoch.key()),
        ],
    )
}
