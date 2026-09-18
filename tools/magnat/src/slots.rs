//! Sloty zapisu w kliencie: „Zapisz" i „Wczytaj".
//!
//! Trzeci temat obok [`crate::app`] (klatka) i [`crate::session`] (droga do świata):
//! tutaj jest **plik na dysku**. Wydzielone przy przeglądzie strukturalnym po M9e —
//! blok `impl` w `session.rs` przekroczył próg błędu, a to była w nim jedyna grupa
//! metod, która nie opisuje przejścia między stanami gry, tylko zapis i odczyt.
//!
//! Zapis jest nagłówkiem plus dziennikiem wejść (`DA-7`), a wczytanie — przewinięciem
//! tego dziennika. Drugiej drogi do świata z pliku nie ma i mieć nie powinno: świat
//! zapisany inaczej niż odtwarzalnie nie jest deterministyczny (`00` §3).

use crate::app::App;
use crate::citizens;
use magnat_core::SimMinute;
use magnat_game::shell::ShellScreen;
use magnat_game::{GameState, Session};
use std::sync::Arc;

impl App {
    /// Zapis slotu: nagłówek plus dziennik wejść (`DA-7`).
    ///
    /// Majątek w wierszu slotu to majątek gospodarstwa gracza (`M9c` WP4); świat bez
    /// wybranej postaci zapisuje zero i to jest prawda, a nie zaślepka.
    /// Nazwa „miasta" to nazwa pierwszej dzielnicy — własnej nazwy miasto nie ma.
    pub(crate) fn zapisz(&mut self, id: u8) {
        let Some(session) = self.game.session() else {
            return;
        };
        let miasto = nazwa_miasta(session, self.params.seed);
        // Majątek gracza to majątek jego gospodarstwa — jedna liczba, ta sama, którą
        // pokazuje karta. Bez postaci zostaje zero i to jest prawda, a nie zaślepka.
        let majatek = session
            .player()
            .and_then(|p| {
                session
                    .app
                    .world
                    .get::<magnat_agents::Household>(p.household.entity())
                    .map(magnat_game::inspect::household_worth)
            })
            .unwrap_or(magnat_core::Money::ZERO);
        let slot = magnat_game::SaveSlot {
            id,
            city: miasto,
            game_date: SimMinute(session.tick().get()),
            net_worth: majatek,
            played_secs: u32::try_from(session.played_ms() / 1000).unwrap_or(u32::MAX),
            world: self.params,
            schema_version: magnat_game::SAVE_SCHEMA_VERSION,
            saved_at_wall: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        };
        match magnat_game::save::write_slot(&self.zapisy, &slot, session.log()) {
            Ok(()) => eprintln!("zapisano slot {id}"),
            Err(e) => eprintln!("zapis slotu {id} nieudany: {e}"),
        }
        self.shell.refresh_slots(&self.zapisy);
        self.pauza_menu = true;
        self.shell.go(ShellScreen::Pause);
    }

    /// Wczytanie slotu: przewinięcie dziennika wejść.
    ///
    /// Kosztuje tyle, ile kosztowała rozgrywka (`DB-3`) i dlatego blokuje klatkę —
    /// ekran mówi to wprost, zamiast udawać, że odtworzenie roku gry jest darmowe.
    pub(crate) fn wczytaj(&mut self, id: u8) {
        let log = match magnat_game::save::read_log(&self.zapisy, id) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("slot {id}: {e}");
                return;
            }
        };
        let pula = magnat_jobs::JobPool::new(self.watki);
        let ticki = log.commands.last().map_or(0, |e| e.tick.get());
        eprintln!("wczytuję slot {id}: odtwarzam {ticki} minut gry");
        let (mut session, mut ui) = match magnat_game::replay_session(&log, ticki, 0, &pula) {
            Ok((session, _)) => {
                // Interfejs rozgrywki powstaje **przed** porzuceniem starego świata.
                // Kolejność jest istotna: gdyby padł po `opusc_swiat`, poprzednia gra
                // byłaby już bezpowrotnie porzucona, a wczytana nie miałaby czym
                // grać — gracz zostałby w menu nad światem bez wejścia.
                match citizens::Citizens::new(log.header.params.world.seed, self.shell.settings.locale) {
                    Ok(ui) => (session, ui),
                    Err(e) => {
                        eprintln!("slot {id}: interfejs rozgrywki nieudany: {e}");
                        return;
                    }
                }
            }
            Err(e) => {
                eprintln!("slot {id}: nie udało się odtworzyć sesji: {e}");
                return;
            }
        };
        self.opusc_swiat();
        self.params = log.header.params.world;
        self.shell.draft = log.header.params;
        self.terrain = Some(session.built.terrain.clone());
        self.edits = Arc::new(session.built.city.edits.clone());
        self.city = Some(session.built.city.clone());
        let c = session.built.city.center;
        self.cel = Some((c.x as i32, c.y as i32));
        self.ustaw_kamere_startowa();
        self.wpnij_render();
        ui.warm_up(&mut session, 0, self.camera.eye());
        ui.set_speed(self.predkosc);
        ui.arm_stop_conditions();
        ui.start_tutorial(&session);
        self.citizens = Some(ui);
        self.game = GameState::Playing(Box::new(session));
        self.shell.has_session = true;
    }
}

/// Nazwa dla wiersza slotu. Miasto nie ma własnej nazwy — bierzemy nazwę pierwszej
/// dzielnicy, bo to jedyna prawdziwa nazwa, jaką ten świat niesie.
fn nazwa_miasta(session: &Session, seed: u64) -> String {
    session
        .built
        .city
        .districts
        .districts
        .first()
        .map_or_else(|| format!("{seed:#x}"), |d| d.name.clone())
}
