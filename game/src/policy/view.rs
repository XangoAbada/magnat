//! Ekran edytora reguł (`DD-1`: `RuleEditorView` należy do M9d, nie do rdzenia UI).
//!
//! # Co tu jest, a czego nie ma
//!
//! Jest **cztery zakładki**: reguły, uwagi walidatora, próba na ostatnich dobach
//! i zapis tekstowy. Nie ma rysowania drzewa, przeciągania ani własnego układu —
//! rdzeń UI z `M9b` daje motyw, zakładki i formatowanie, a edytor je składa.
//!
//! Nie ma też **wpisywania**. Klawiatura wybiera regułę, włącza ją i wyłącza, kasuje
//! i przypina politykę; składanie wyrażeń idzie przez [`crate::policy::Edit`], czyli
//! przez ten sam wjazd, którego pilnuje walidator formularza. Ekran nie jest drogą
//! obok edytora, tylko jego widokiem.
//!
//! ponytail: sufit nazwany — dodawanie reguł i akcji z klawiatury czeka na panel
//! firmy z `M9e`, bo to tam gracz wybiera zakład i towar. Tutaj ekran pokazuje
//! politykę, mówi, co z nią nie tak, i pozwala ją przypiąć albo odrzucić — czyli
//! odpowiada na pytania, które da się zadać bez panelu.

use magnat_core::Money;
use magnat_economy::DryRun;
use magnat_policy::Diagnostic;
use magnat_ui::{ColorToken, TextRole, Theme};

use super::{DrySummary, GoodKeys, Note, RuleEditor};

/// Co ekran każe zrobić pętli gry.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EditorAction {
    #[default]
    None,
    /// Zamknij bez zmian.
    Close,
    /// Przypnij politykę do zakładu — komendę składa wołający, bo to on zna zakład.
    Attach,
}

/// Zakładki edytora. Cztery, bo cztery są pytania: „co ta polityka robi",
/// „co z nią nie tak", „co by zrobiła" i „jak ją komuś wysłać".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    #[default]
    Rules,
    Notes,
    DryRun,
    Text,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Rules, Tab::Notes, Tab::DryRun, Tab::Text];

    const fn key(self) -> &'static str {
        match self {
            Tab::Rules => "ui.policy.editor.rules",
            Tab::Notes => "ui.policy.editor.diagnostics",
            Tab::DryRun => "ui.policy.editor.dryrun",
            Tab::Text => "ui.policy.editor.text",
        }
    }
}

/// Ekran edytora: formularz plus to, co z niego wynika.
pub struct RuleEditorView {
    pub editor: RuleEditor,
    pub tab: Tab,
    /// Która reguła jest pod kursorem.
    pub focus: usize,
    dry: Option<DrySummary>,
}

impl RuleEditorView {
    #[must_use]
    pub fn new(editor: RuleEditor) -> RuleEditorView {
        RuleEditorView {
            editor,
            tab: Tab::Rules,
            focus: 0,
            dry: None,
        }
    }

    /// Wynik próby na ostatnich dobach. Wołane po [`super::dry_run`] — ekran sam
    /// po rynek nie sięga, bo panele nie widzą świata (§5.2 fazy).
    pub fn set_dry(&mut self, r: &DryRun) {
        self.dry = Some(DrySummary::of(r));
    }

    /// Czy politykę wolno przypiąć. Ostrzeżenia nie blokują — błędy tak.
    #[must_use]
    pub fn can_attach(&self) -> bool {
        !self.editor.notes().iter().any(Note::is_error)
    }

    /// Rysuje ekran i zwraca to, co gracz kazał zrobić.
    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        c: &magnat_ui::Catalog,
        l: magnat_ui::Locale,
        goods: &GoodKeys,
    ) -> EditorAction {
        let keys = crate::screens::Keys::read(ui);
        let mut tab = self.tab;
        let etykieta = |t: Tab| c.fmt_key(l, t.key(), &[]);
        naglowek(ui, theme, &c.fmt_key(l, "ui.policy.editor.title", &[]));
        ui.label(
            egui::RichText::new(self.editor.name())
                .font(theme.font(TextRole::Title))
                .color(theme.color(ColorToken::TextPrimary)),
        );
        magnat_ui::tab_strip(ui, theme, &Tab::ALL, &mut tab, etykieta);
        self.tab = tab;
        ui.add_space(theme.gap(2));

        match self.tab {
            Tab::Rules => self.reguly(ui, theme, c, l, goods, &keys),
            Tab::Notes => self.uwagi(ui, theme, c, l),
            Tab::DryRun => self.proba(ui, theme, c, l),
            Tab::Text => {
                let txt = super::write(&self.editor.policy(), self.editor.scope(), goods, c, l);
                ui.label(
                    egui::RichText::new(txt)
                        .font(theme.mono(TextRole::Body))
                        .color(theme.color(ColorToken::TextSecondary)),
                );
            }
        }

        ui.add_space(theme.gap(3));
        let mozna = self.can_attach();
        ui.label(
            egui::RichText::new(c.fmt_key(
                l,
                if mozna {
                    "ui.policy.editor.attach"
                } else {
                    "ui.policy.editor.blocked"
                },
                &[],
            ))
            .font(theme.font(TextRole::Micro))
            .color(theme.color(if mozna {
                ColorToken::AccentHi
            } else {
                ColorToken::TextSecondary
            })),
        );

        if keys.esc {
            return EditorAction::Close;
        }
        if keys.enter && mozna {
            return EditorAction::Attach;
        }
        EditorAction::None
    }

    fn reguly(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        c: &magnat_ui::Catalog,
        l: magnat_ui::Locale,
        goods: &GoodKeys,
        keys: &crate::screens::Keys,
    ) {
        let ile = self.editor.rules().len();
        if ile == 0 {
            ui.label(
                egui::RichText::new(c.fmt_key(l, "ui.policy.editor.no_rules", &[]))
                    .font(theme.font(TextRole::Body))
                    .color(theme.color(ColorToken::TextSecondary)),
            );
            return;
        }
        keys.move_focus(&mut self.focus, ile);
        let focus = self.focus;
        for (i, r) in self.editor.rules().iter().enumerate() {
            let aktywna = i == focus;
            let kolor = if !r.enabled {
                ColorToken::TextSecondary
            } else if aktywna {
                ColorToken::TextPrimary
            } else {
                ColorToken::TextSecondary
            };
            for (j, linia) in super::text::rule_lines(r, goods, c, l).iter().enumerate() {
                let tekst = if j == 0 {
                    format!("{}. {linia}", i + 1)
                } else {
                    format!("   {linia}")
                };
                ui.label(
                    egui::RichText::new(tekst)
                        .font(theme.mono(if aktywna {
                            TextRole::Strong
                        } else {
                            TextRole::Body
                        }))
                        .color(theme.color(kolor)),
                );
            }
            // Treść komunikatu, jeśli reguła coś mówi. W zapisie tekstowym stoi
            // tam `msg#1`, bo tekst musi wrócić z parsera co do znaku — ale gracz
            // czytający regułę ma widzieć zdanie, a nie numer.
            for a in &r.actions {
                if !matches!(
                    a.kind,
                    magnat_core::ActionKind::Alert | magnat_core::ActionKind::AskPlayer
                ) {
                    continue;
                }
                // Komunikat spoza katalogu nie ma zdania — i wtedy nie pokazuje się
                // klucza, tylko nic. Klucz na ekranie jest gorszy od pustego miejsca,
                // bo wygląda jak tekst, a nim nie jest.
                let Some(k) = c.key(&format!("ui.policy.msg.{}", a.msg)) else {
                    continue;
                };
                ui.label(
                    egui::RichText::new(format!("   — {}", c.text(l, k)))
                        .font(theme.font(TextRole::Micro))
                        .color(theme.color(ColorToken::TextSecondary)),
                );
            }
            ui.add_space(theme.gap(1));
        }
        if let Some(f) = self.editor.fallback() {
            ui.label(
                egui::RichText::new(format!(
                    "{}: {}",
                    c.fmt_key(l, "ui.policy.editor.fallback", &[]),
                    super::text::action_line(f, goods, c, l)
                ))
                .font(theme.mono(TextRole::Body))
                .color(theme.color(ColorToken::TextSecondary)),
            );
        }
    }

    fn uwagi(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        c: &magnat_ui::Catalog,
        l: magnat_ui::Locale,
    ) {
        let uwagi = self.editor.notes();
        if uwagi.is_empty() {
            ui.label(
                egui::RichText::new(c.fmt_key(l, "ui.policy.editor.no_notes", &[]))
                    .font(theme.font(TextRole::Body))
                    .color(theme.color(ColorToken::TextSecondary)),
            );
            return;
        }
        for n in &uwagi {
            ui.label(
                egui::RichText::new(note_text(n, c, l))
                    .font(theme.font(TextRole::Body))
                    .color(theme.color(if n.is_error() {
                        ColorToken::Danger
                    } else {
                        ColorToken::Warn
                    })),
            );
        }
    }

    fn proba(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        c: &magnat_ui::Catalog,
        l: magnat_ui::Locale,
    ) {
        let Some(d) = self.dry else {
            ui.label(
                egui::RichText::new(c.fmt_key(l, "ui.policy.editor.no_trace", &[]))
                    .font(theme.font(TextRole::Body))
                    .color(theme.color(ColorToken::TextSecondary)),
            );
            return;
        };
        let dni = c.plural(l, c.must("ui.unit.days"), u64::from(d.days));
        ui.label(
            egui::RichText::new(c.fmt_key(
                l,
                "ui.policy.editor.dry_summary",
                &[
                    ("dni", &dni),
                    ("ceny", &d.price_moves.to_string()),
                    ("zamowienia", &d.orders.to_string()),
                    ("alerty", &d.alerts.to_string()),
                    ("slepe", &d.blind.to_string()),
                ],
            ))
            .font(theme.font(TextRole::Body))
            .color(theme.color(ColorToken::TextPrimary)),
        );
        if let (Some(min), Some(max)) = (d.min_price, d.max_price) {
            let kwota = |m: Money| magnat_ui::fmt::money(c, l, m);
            ui.label(
                egui::RichText::new(c.fmt_key(
                    l,
                    "ui.policy.editor.dry_range",
                    &[("min", &kwota(min)), ("max", &kwota(max))],
                ))
                .font(theme.font(TextRole::Body))
                .color(theme.color(ColorToken::TextSecondary)),
            );
        }
    }
}

fn naglowek(ui: &mut egui::Ui, theme: &Theme, tytul: &str) {
    ui.label(
        egui::RichText::new(tytul)
            .font(theme.font(TextRole::Screen))
            .color(theme.color(ColorToken::TextPrimary)),
    );
}

/// Uwaga walidatora po ludzku. Wyczerpujący `match`: nowy wariant diagnostyki
/// bez zdania **nie kompiluje się** — ta sama reguła co przy `DecisionReason`
/// (`K-12`), tylko po stronie edytora.
#[must_use]
pub fn note_text(n: &Note, c: &magnat_ui::Catalog, l: magnat_ui::Locale) -> String {
    let k = |name: &str, args: &[(&str, &str)]| c.fmt_key(l, &format!("ui.policy.diag.{name}"), args);
    let nr = |i: usize| (i + 1).to_string();
    match n {
        Note::BelowCost { rule } => k("BelowCost", &[("numer", &nr(*rule))]),
        Note::Language(d) => match d {
            Diagnostic::UnitMismatch { expected, got, .. } => k(
                "UnitMismatch",
                &[
                    ("oczekiwano", &format!("{expected:?}")),
                    ("jest", &format!("{got:?}")),
                ],
            ),
            Diagnostic::PriceBasisMismatch { .. } => k("PriceBasisMismatch", &[]),
            Diagnostic::ActionOutOfDomain { .. } => k("ActionOutOfDomain", &[]),
            Diagnostic::DomainNotAvailable { .. } => k("DomainNotAvailable", &[]),
            Diagnostic::TooDeep { .. } => k("TooDeep", &[]),
            Diagnostic::BudgetExceeded { what, .. } => k("BudgetExceeded", &[("co", what)]),
            Diagnostic::UnreachableRule { rule, .. } => {
                k("UnreachableRule", &[("numer", &nr(*rule))])
            }
            Diagnostic::PossibleOscillation { rule } => {
                k("PossibleOscillation", &[("numer", &nr(*rule))])
            }
        },
    }
}
