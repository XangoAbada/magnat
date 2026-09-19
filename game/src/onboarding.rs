//! Pomiar onboardingu z dziennika replayu (M9e §5.12, PRD §20.3).
//!
//! # Jeden mechanizm, nie dwa
//!
//! Metryki gracza liczą się **z tego samego dziennika**, z którego odtwarza się
//! sesję. Osobna telemetria byłaby drugim źródłem prawdy o tej samej rozgrywce
//! i rozjechałaby się przy pierwszej zmianie formatu — a przy okazji kazałaby
//! wysyłać coś, czego zgłoszenie błędu i tak już niesie.
//!
//! Stąd bierze się kształt tego modułu: czyta [`ReplayLog`] i nic poza nim.
//!
//! # Co znaczy „sensowna decyzja"
//!
//! §5.12 definiuje ją jako pierwszą komendę ze zbioru [`SENSOWNE`]. Zbiór rośnie
//! razem z podfazami — po M9d były w nim dwie komendy, bo tyle istniało (`DH-2`);
//! po M9e jest pięć, bo `OpenSite`, `ApplyForJob` i `HireCandidate` mają wykonawców
//! i panele. Zbiór **nie wymienia komend, których nie ma**: metryka ma mierzyć grę,
//! a nie plan.
//!
//! # Czego ten pomiar nie robi
//!
//! Nie mierzy czasu zegarowego w CI (§5.12 pkt 7). Znacznik `wall_ms` niesie tylko
//! strumień widoku, więc czas do decyzji da się policzyć **z playtestu**, a nie
//! z przebiegu skryptowego, w którym klatki nie trwają. CI pilnuje liczby interakcji
//! i liczby otwartych paneli; mediana czasu liczy się offline z nagrań.

use crate::panels::PanelId;
use crate::replay::ReplayLog;
use crate::{PlayerCommand, ViewCommand};

/// Sufit interakcji do pierwszej sensownej decyzji (§5.12 pkt 3).
pub const MAX_INTERAKCJI: u32 = 12;

/// Sufit paneli otwartych w tym czasie (§5.12 pkt 5).
pub const MAX_PANELI: u32 = 3;

/// Co liczy się jako pierwsza sensowna decyzja.
///
/// Lista jest **kodem, a nie komentarzem**, bo metryka §20.3 stoi na jej treści:
/// dopisanie tu komendy, której gracz nie wydaje w pierwszych minutach, poprawiłoby
/// wynik bez poprawienia gry.
#[must_use]
pub const fn sensowna(cmd: &PlayerCommand) -> bool {
    matches!(
        cmd,
        PlayerCommand::SetPrice { .. }
            | PlayerCommand::AttachPolicy { .. }
            | PlayerCommand::OpenSite { .. }
            | PlayerCommand::ApplyForJob { .. }
            | PlayerCommand::HireCandidate { .. }
    )
}

/// Komendy zbioru [`sensowna`] — do raportu i do testu.
pub const SENSOWNE: [&str; 5] = [
    "SetPrice",
    "AttachPolicy",
    "OpenSite",
    "ApplyForJob",
    "HireCandidate",
];

/// Co dziennik mówi o pierwszych minutach gry.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Onboarding {
    /// Ile komend gracz wydał do pierwszej sensownej decyzji **włącznie**.
    /// `StartGame` się nie liczy: nikt jej nie klika, jest kopertą świata.
    pub interactions: u32,
    /// Ile **różnych** paneli otworzył w tym czasie.
    pub panels: u32,
    /// Czas realny do tej decyzji, jeśli strumień widoku był włączony.
    /// `None` w przebiegu skryptowym — tam klatki nie trwają.
    pub wall_ms: Option<u64>,
    /// Czy sensowna decyzja w ogóle padła.
    pub reached: bool,
}

impl Onboarding {
    /// Czy przebieg mieści się w budżecie §5.12. To jest bramka CI.
    #[must_use]
    pub const fn within_budget(&self) -> bool {
        self.reached && self.interactions <= MAX_INTERAKCJI && self.panels <= MAX_PANELI
    }
}

/// Liczy metryki z dziennika.
#[must_use]
pub fn measure(log: &ReplayLog) -> Onboarding {
    let Some(decyzja) = log.commands.iter().find(|e| sensowna(&e.cmd)) else {
        return Onboarding {
            interactions: policz_komendy(log, u64::MAX),
            panels: policz_panele(log, u64::MAX),
            wall_ms: None,
            reached: false,
        };
    };
    let do_ticku = decyzja.tick.get();
    Onboarding {
        interactions: policz_komendy(log, decyzja.seq),
        panels: policz_panele(log, do_ticku),
        wall_ms: log
            .view
            .iter()
            .filter(|v| v.tick.get() <= do_ticku)
            .map(|v| v.wall_ms)
            .max(),
        reached: true,
    }
}

/// Ile komend gracza do numeru `seq` włącznie, bez koperty startowej.
fn policz_komendy(log: &ReplayLog, seq: u64) -> u32 {
    let n = log
        .commands
        .iter()
        .filter(|e| e.seq <= seq)
        .filter(|e| !matches!(e.cmd, PlayerCommand::StartGame { .. }))
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Ile **różnych** paneli otwarto do ticku `tick` włącznie.
///
/// Różnych, a nie otwarć: gracz, który zamknął i otworzył ten sam panel, nadal
/// patrzył na jeden. Liczba z §5.12 pkt 5 mówi o tym, ile rzeczy naraz trzeba
/// ogarnąć, a nie ile razy kliknięto.
fn policz_panele(log: &ReplayLog, tick: u64) -> u32 {
    let mut widziane: Vec<PanelId> = Vec::new();
    for v in log.view.iter().filter(|v| v.tick.get() <= tick) {
        if let ViewCommand::OpenPanel(id) = v.cmd {
            if !widziane.contains(&id) {
                widziane.push(id);
            }
        }
    }
    u32::try_from(widziane.len()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::NewGameParams;
    use crate::{CommandEnvelope, PlayerId, ViewRecord};
    use magnat_core::{Money, SiteId, Tick};

    fn dziennik() -> ReplayLog {
        ReplayLog::new(NewGameParams::default())
    }

    fn koperta(seq: u64, tick: u64, cmd: PlayerCommand) -> CommandEnvelope {
        CommandEnvelope {
            seq,
            tick: Tick(tick),
            actor: PlayerId(0),
            cmd,
        }
    }

    fn cena() -> PlayerCommand {
        PlayerCommand::SetPrice {
            site: SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
            good: "food_bread_wheat".to_string(),
            price: Money(500),
        }
    }

    #[test]
    fn koperta_startowa_nie_jest_interakcja() {
        let mut log = dziennik();
        log.commands.push(koperta(
            0,
            0,
            PlayerCommand::StartGame {
                world: magnat_world::WorldGenParams::default(),
                scenario: crate::ScenarioId::SANDBOX,
                variant: crate::StartVariant::Worker,
                pick: None,
            },
        ));
        log.commands.push(koperta(1, 10, cena()));
        let m = measure(&log);
        assert_eq!(m.interactions, 1, "nikt nie klika koperty świata");
        assert!(m.within_budget());
    }

    #[test]
    fn ten_sam_panel_otwarty_dwa_razy_liczy_sie_raz() {
        let mut log = dziennik();
        for (wall, cmd) in [
            (100, ViewCommand::OpenPanel(PanelId::Shop)),
            (200, ViewCommand::ClosePanel(PanelId::Shop)),
            (300, ViewCommand::OpenPanel(PanelId::Shop)),
            (400, ViewCommand::OpenPanel(PanelId::Dashboard)),
        ] {
            log.view.push(ViewRecord {
                wall_ms: wall,
                tick: Tick(5),
                cmd,
            });
        }
        log.commands.push(koperta(0, 10, cena()));
        let m = measure(&log);
        assert_eq!(
            m.panels, 2,
            "liczy się ile rzeczy naraz, a nie ile kliknięć"
        );
        assert_eq!(m.wall_ms, Some(400));
        assert!(m.within_budget());
    }

    #[test]
    fn przebieg_bez_sensownej_decyzji_nie_mieści_sie_w_budzecie() {
        let mut log = dziennik();
        log.commands.push(koperta(
            0,
            5,
            PlayerCommand::SetAutonomy {
                field: crate::AutonomyField::Job,
                control: crate::Control::Manual,
            },
        ));
        let m = measure(&log);
        assert!(!m.reached, "zmiana autopilota nie jest decyzją biznesową");
        assert!(!m.within_budget());
    }

    #[test]
    fn powyzej_progu_budzet_peka() {
        let mut log = dziennik();
        for i in 0..MAX_INTERAKCJI {
            log.commands.push(koperta(
                u64::from(i),
                1,
                PlayerCommand::DetachPolicy {
                    site: SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
                },
            ));
        }
        log.commands
            .push(koperta(u64::from(MAX_INTERAKCJI), 2, cena()));
        let m = measure(&log);
        assert_eq!(m.interactions, MAX_INTERAKCJI + 1);
        assert!(!m.within_budget(), "trzynasta interakcja ma przebić budżet");
    }
}
