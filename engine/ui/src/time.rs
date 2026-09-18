//! Sterowanie czasem — widget (M3d §5.11, decyzja 9.1, PRD §14.5).
//!
//! **M3 dostarcza wyłącznie widget.** Zegar (`SimClock`, `SimSpeed`) należy do
//! `engine/core` — tu jest stan przycisków, etykiety w języku gracza i kalendarz
//! 12 × 30 (K-1). Widget ustawia prędkość i nic poza tym.
//!
//! Twardy wymóg §5.11: **prędkość nie wpływa na wynik.** Wynika to ze struktury,
//! a nie z dyscypliny — zegar zamienia czas realny na liczbę minut gry do wykonania,
//! a każda minuta jest tym samym tickiem niezależnie od prędkości. Test
//! `det_speed_invariance` porównuje hash doby przy 1× i przy 10×.

use crate::loc::{Catalog, Locale};
use magnat_core::{SimCalendar, SimClock, SimSpeed, Tick};

/// Ile minut gry wolno nadrobić w jednej klatce po zacięciu.
///
/// Bez limitu okno przywrócone po minucie w tle próbowałoby dogonić 600 minut gry
/// w jednej klatce i zawiesiłoby się na kilka sekund. Limit jest hojny (dwie godziny
/// gry przy 10× to 12 sekund realnych), ale skończony.
pub const CATCH_UP_CAP: u32 = 120;

/// Stan widgetu sterowania czasem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TimeControlsWidget {
    clock: SimClock,
    /// Minuta świata, przy której symulacja ma się zatrzymać („zatrzymaj przy X"),
    /// albo `None`. Prędkość wraca wtedy do pauzy.
    stop_at: Option<Tick>,
    /// Prędkość sprzed pauzy — żeby spacja wracała tam, skąd zatrzymała.
    poprzednia: SimSpeed,
}

impl TimeControlsWidget {
    #[must_use]
    pub fn new(start: Tick) -> TimeControlsWidget {
        TimeControlsWidget {
            clock: SimClock::new(start),
            stop_at: None,
            poprzednia: SimSpeed::X1,
        }
    }

    #[must_use]
    pub fn clock(&self) -> SimClock {
        self.clock
    }

    #[must_use]
    pub fn speed(&self) -> SimSpeed {
        self.clock.speed()
    }

    #[must_use]
    pub fn calendar(&self) -> SimCalendar {
        self.clock.calendar()
    }

    pub fn set_speed(&mut self, s: SimSpeed) {
        if s != SimSpeed::Paused {
            self.poprzednia = s;
        }
        self.clock.set_speed(s);
    }

    /// Pauza / wznowienie — wraca do prędkości sprzed pauzy, a nie zawsze do 1×.
    pub fn toggle_pause(&mut self) {
        let nowa = if self.speed() == SimSpeed::Paused {
            self.poprzednia
        } else {
            SimSpeed::Paused
        };
        self.clock.set_speed(nowa);
    }

    /// „Zatrzymaj przy X" — symulacja stanie na tym ticku i przejdzie w pauzę.
    pub fn stop_at(&mut self, tick: Option<Tick>) {
        self.stop_at = tick;
    }

    /// Ile ticków ekonomicznych wykonać za `dt_ms` czasu realnego.
    ///
    /// Zegar sam siebie przesuwa; wywołujący ma wykonać dokładnie tyle ticków.
    /// Punkt zatrzymania obcina wynik i przełącza w pauzę — **nie pomija** ticków,
    /// bo pominięty tick to inny świat, a nie ten sam świat wcześniej.
    pub fn advance(&mut self, dt_ms: u32) -> u32 {
        let mut minut = self.clock.advance(dt_ms, CATCH_UP_CAP);
        if let Some(cel) = self.stop_at {
            let teraz = self.clock.tick().0;
            if teraz >= cel.0 {
                let nadmiar = (teraz - cel.0) as u32;
                minut = minut.saturating_sub(nadmiar);
                self.clock = SimClock::new(cel);
                self.clock.set_speed(SimSpeed::Paused);
                self.stop_at = None;
            }
        }
        minut
    }

    /// Przyciski prędkości w kolejności wyświetlania: etykieta, prędkość, czy wciśnięty.
    #[must_use]
    pub fn buttons(&self, c: &Catalog, l: Locale) -> Vec<(String, SimSpeed, bool)> {
        [
            ("ui.time.paused", SimSpeed::Paused),
            ("ui.time.x1", SimSpeed::X1),
            ("ui.time.x3", SimSpeed::X3),
            ("ui.time.x10", SimSpeed::X10),
        ]
        .iter()
        .map(|(k, s)| (c.fmt_key(l, k, &[]), *s, *s == self.speed()))
        .collect()
    }

    /// Data w języku gracza: rok, miesiąc, dzień, dzień tygodnia (K-1, K-15).
    #[must_use]
    pub fn date_label(&self, c: &Catalog, l: Locale) -> String {
        crate::CalendarFmt::date(c, l, self.calendar())
    }

    /// Godzina w języku gracza.
    #[must_use]
    pub fn clock_label(&self, c: &Catalog, l: Locale) -> String {
        crate::CalendarFmt::clock(c, l, self.calendar())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pauza_wraca_do_predkosci_sprzed_pauzy() {
        let mut w = TimeControlsWidget::new(Tick(0));
        w.set_speed(SimSpeed::X10);
        w.toggle_pause();
        assert_eq!(w.speed(), SimSpeed::Paused);
        assert_eq!(w.advance(5_000), 0, "pauza nie zatrzymuje czasu");
        w.toggle_pause();
        assert_eq!(w.speed(), SimSpeed::X10);
    }

    #[test]
    fn zatrzymanie_przy_ticku_nie_przeskakuje_go() {
        let mut w = TimeControlsWidget::new(Tick(0));
        w.set_speed(SimSpeed::X10);
        w.stop_at(Some(Tick(30)));
        // Sekunda realna przy 10× to 10 minut gry — trzy sekundy przekroczyłyby cel.
        let mut suma = 0u32;
        for _ in 0..5 {
            suma += w.advance(1_000);
        }
        assert_eq!(w.clock().tick(), Tick(30));
        assert_eq!(suma, 30, "przeskoczono albo zgubiono tick");
        assert_eq!(w.speed(), SimSpeed::Paused);
    }

    #[test]
    fn predkosc_nie_zmienia_liczby_tickow_doby() {
        // To jest wymóg §5.11 wyrażony liczbą: doba to 1440 ticków niezależnie od tego,
        // ile realnego czasu zajmie. Reszta akumulatora bierze się z podziału sekundy
        // na klatki i nie kumuluje się między prędkościami.
        for (predkosc, sekundy) in [
            (SimSpeed::X1, 1440u32),
            (SimSpeed::X3, 480),
            (SimSpeed::X10, 144),
        ] {
            let mut w = TimeControlsWidget::new(Tick(0));
            w.set_speed(predkosc);
            let mut suma = 0u32;
            for _ in 0..sekundy * 10 {
                suma += w.advance(100);
            }
            assert_eq!(suma, 1440, "{predkosc:?}");
        }
    }

    #[test]
    fn etykiety_sa_w_jezyku_gracza() {
        let c = crate::loc::Catalog::load().expect("data/locale/");
        let w = TimeControlsWidget::new(Tick(0));
        for l in Locale::ALL {
            let b = w.buttons(&c, l);
            assert_eq!(b.len(), 4);
            assert!(b.iter().all(|(t, _, _)| !t.is_empty()));
            assert!(b[1].2, "1× powinno być wciśnięte na starcie");
            let d = w.date_label(&c, l);
            assert!(!d.contains('{'), "{d}");
            assert_eq!(w.clock_label(&c, l), "00:00");
        }
    }
}
