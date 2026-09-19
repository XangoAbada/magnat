//! Ekrany domknięcia: koniec scenariusza i spuścizna (`ui-design.md` §6.6, `DI-34`).
//!
//! Do domknięcia `WP12` żadnego z nich nie było, a `Session::scenario_outcome()` miał
//! czytelnika **wyłącznie w teście** — gracz nie dowiadywał się, że wygrał. Ekrany
//! stoją poza rozgrywką, bo świat za nimi tyka dalej: sukcesja jest komendą w tym
//! samym świecie, a nie nową grą.
//!
//! **Żaden z nich nie jest ślepym zaułkiem** (`ui-design.md` §6.6). Koniec scenariusza
//! wychodzi do gry albo do menu, spuścizna — do nowej dynastii albo do menu.

use magnat_core::CitizenId;
use magnat_ui::{ColorToken, TextRole};

use super::{header, menu_list, Keys, Shell};
use crate::scenario::{streak_progress_bp, ScenarioOutcome};
use crate::Session;

/// Co gracz wybrał na ekranie domknięcia.
///
/// Osobno od [`super::ShellAction`], bo te akcje **nie zmieniają ekranu powłoki** —
/// zmieniają stan gry, a on stoi nad powłoką. Wspólny enum kazałby ekranom powłoki
/// znać sukcesję, a sukcesji — listę slotów.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EndAction {
    /// Przejmij firmy jako wskazany dziedzic.
    Succeed(CitizenId),
    /// Zacznij nową dynastię w tym samym świecie.
    NewDynasty(CitizenId),
    /// Graj dalej mimo domknięcia scenariusza — cele są celami, nie końcem świata.
    KeepPlaying,
    /// Porzuć sesję i wróć do menu głównego.
    ToMenu,
}

/// Ile wpisów kroniki dynastii pokazać. Tyle, ile mieści się bez przewijania —
/// spuścizna ma być podsumowaniem, a nie pełnym dziennikiem; pełny jest w panelu
/// Kronika i zostaje w zapisie.
const WPISOW: usize = 12;

/// Ważność, od której wpis wchodzi do spuścizny. Ta sama granica, od której kronika
/// nie decymuje wpisów spoza zakresu gracza.
const WAZNOSC: u8 = 60;

/// Koniec scenariusza: cele rozliczone co do jednego.
pub fn scenario_end(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    session: &Session,
    outcome: ScenarioOutcome,
) -> Option<EndAction> {
    let keys = Keys::read(ui);
    let tytul = shell.text(match outcome {
        ScenarioOutcome::Won => "ui.ending.won",
        ScenarioOutcome::Lost => "ui.ending.lost",
        ScenarioOutcome::Running => "ui.ending.running",
    });
    let _ = header(shell, ui, &tytul, &["ui.path.game"]);

    match session.scenario() {
        Some(sc) => {
            ui.label(
                egui::RichText::new(shell.text(&sc.title))
                    .font(shell.theme.font(TextRole::Title))
                    .color(shell.theme.color(ColorToken::TextPrimary)),
            );
            ui.add_space(shell.theme.gap(4));
            let stan = session.scenario_state();
            // Majątek raz, przed pętlą: `Holdings::of` przechodzi rejestr firm
            // i zakłady gracza, a cele pytają o tę samą liczbę.
            let h = crate::career::Holdings::of(session);
            for o in &sc.objectives {
                // Trzy odpowiedzi, nie dwie: „osiągnięty", „przepadł" i „o ile
                // chybiony". Ostatnia jest tą, po którą gracz tu przychodzi.
                let (klucz, kolor) = if stan.is_done(o.id) {
                    ("ui.ending.goal.done", ColorToken::Ok)
                } else if stan.is_failed(o.id) {
                    ("ui.ending.goal.failed", ColorToken::Danger)
                } else {
                    ("ui.ending.goal.missed", ColorToken::Warn)
                };
                let postep = streak_progress_bp(stan, o).max(o.goal.progress_bp(session, &h));
                let wiersz = shell.fmt(
                    klucz,
                    &[
                        ("cel", &shell.text(&o.title)),
                        ("procent", &crate::panels::percent_bp(i32::from(postep))),
                        // Znak obok koloru: kolor nigdy nie jest jedynym nośnikiem
                        // (`ui-design.md` §3.1, reguła daltonizmu).
                        ("opcjonalny", if o.optional { "○" } else { "●" }),
                    ],
                );
                ui.label(
                    egui::RichText::new(wiersz)
                        .font(shell.theme.font(TextRole::Body))
                        .color(shell.theme.color(kolor)),
                );
            }
        }
        // Tryb otwarty nie ma czego rozliczyć i mówi to wprost, zamiast pokazywać
        // pustą listę, która wygląda jak zero osiągniętych celów.
        None => {
            ui.label(
                egui::RichText::new(shell.text("ui.ending.no_scenario"))
                    .font(shell.theme.font(TextRole::Body))
                    .color(shell.theme.color(ColorToken::TextSecondary)),
            );
        }
    }

    ui.add_space(shell.theme.gap(6));
    let pozycje = vec![
        shell.text("ui.ending.keep_playing"),
        shell.text("ui.shell.to_menu"),
    ];
    let mut focus = shell.focus;
    let wybor = menu_list(shell, ui, &pozycje, &mut focus, &keys);
    shell.focus = focus;
    match wybor {
        Some(0) => Some(EndAction::KeepPlaying),
        Some(_) => Some(EndAction::ToMenu),
        None => None,
    }
}

/// Spuścizna: kto dziedziczy i co po postaci zostało.
///
/// `heir` jest propozycją gry, a nie wyrokiem — `None` znaczy „nie ma komu"
/// i wtedy jedynym wyjściem jest nowa dynastia albo menu.
pub fn succession(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    session: &Session,
    heir: Option<CitizenId>,
) -> Option<EndAction> {
    let keys = Keys::read(ui);
    let tytul = shell.text("ui.legacy.title");
    let _ = header(shell, ui, &tytul, &["ui.path.game"]);

    // Wiek **postaci**, nie świata. Dziedzic, który przejął firmy w pięćdziesiątym
    // roku gry, przeżył trzydzieści lat, a nie osiemdziesiąt — a tak właśnie brzmi
    // zdanie pod spodem. Encji zmarłego zwykle już nie ma (demografia despawnuje
    // w tej samej dobie), więc `None` znaczy „nie wiadomo" i wtedy pokazuje się
    // wiek świata, jako jedyna liczba, którą da się uczciwie podać.
    let doba = session.tick().get() / magnat_core::time::MINUTES_PER_DAY;
    let lata = session
        .player()
        .and_then(|p| {
            session
                .app
                .world
                .get::<magnat_agents::Identity>(p.citizen.entity())
        })
        .map_or(doba / 360, |id| {
            u64::try_from(id.age_years(i32::try_from(doba).unwrap_or(i32::MAX))).unwrap_or(0)
        });
    ui.label(
        egui::RichText::new(shell.fmt("ui.legacy.summary", &[("lata", &lata.to_string())]))
            .font(shell.theme.font(TextRole::Body))
            .color(shell.theme.color(ColorToken::TextSecondary)),
    );
    ui.add_space(shell.theme.gap(4));

    // Kronika dynastii: to, co po postaci zostało. Wpisy gracza nigdy nie są
    // decymowane (`Chronicle::decimate`), więc sto lat gry ich nie zgubi.
    let zapytanie = crate::chronicle::Query {
        kind: None,
        actor: None,
        min_importance: WAZNOSC,
        from_day: None,
        to_day: None,
    };
    let wpisy = session.chronicle().query(&zapytanie);
    if wpisy.is_empty() {
        ui.label(
            egui::RichText::new(shell.text("ui.legacy.empty"))
                .font(shell.theme.font(TextRole::Body))
                .color(shell.theme.color(ColorToken::TextSecondary)),
        );
    }
    let l = shell.locale();
    let c = shell.catalog.clone();
    for e in wpisy.iter().take(WPISOW) {
        let doba = e.at.0 / magnat_core::time::MINUTES_PER_DAY;
        let tekst = format!(
            "{}  {}",
            c.fmt_key(l, "ui.legacy.day", &[("doba", &doba.to_string())]),
            crate::chronicle::text(e, session, &c, l)
        );
        ui.label(
            egui::RichText::new(tekst)
                .font(shell.theme.font(TextRole::Micro))
                .color(shell.theme.color(ColorToken::TextSecondary)),
        );
    }

    ui.add_space(shell.theme.gap(6));
    let mut akcje: Vec<EndAction> = Vec::new();
    let mut pozycje: Vec<String> = Vec::new();
    if let Some(h) = heir {
        akcje.push(EndAction::Succeed(h));
        pozycje.push(shell.fmt("ui.legacy.succeed", &[("kto", &imie(session, h))]));
    }
    // Nowa dynastia jest zawsze dostępna, także gdy dziedzic jest: „zacznij od zera
    // w tym samym mieście" to osobna gra, a nie kara za brak dzieci.
    if let Some(k) = kandydat(session) {
        akcje.push(EndAction::NewDynasty(k));
        pozycje.push(shell.text("ui.legacy.new_dynasty"));
    }
    akcje.push(EndAction::ToMenu);
    pozycje.push(shell.text("ui.shell.to_menu"));

    let mut focus = shell.focus;
    let wybor = menu_list(shell, ui, &pozycje, &mut focus, &keys);
    shell.focus = focus;
    wybor.and_then(|i| akcje.get(i).copied())
}

/// Imię mieszkańca do etykiety przycisku. Imiona pochodzą z `data/names/` i **nie są**
/// lokalizacją UI (CLAUDE.md) — ta sama nazwa pada w obu wersjach językowych.
fn imie(session: &Session, c: CitizenId) -> String {
    session
        .app
        .world
        .get::<magnat_agents::Identity>(c.entity())
        .map(magnat_ui::full_name)
        .unwrap_or_default()
}

/// Kandydat na nową dynastię: pierwszy mieszkaniec spełniający predykat wariantu
/// startu tej gry. Ta sama droga, którą gracz wybierał postać na początku — nie
/// druga, bo dwie listy kandydatów rozjechałyby się przy pierwszej zmianie predykatu.
fn kandydat(session: &Session) -> Option<CitizenId> {
    let doba = session.tick().get() / magnat_core::time::MINUTES_PER_DAY;
    crate::player::candidates(&session.app.world, session.variant(), doba)
        .first()
        .map(|k| k.citizen)
}
