//! Samouczek: trzy kroki do pierwszego sklepu (M9e §5.12 pkt 3–4, `DI-35`).
//!
//! # Czego tu nie ma
//!
//! **Modalnych okien.** Żaden krok nie blokuje gry: samouczek jest paskiem u góry
//! ekranu, a świat pod nim tyka. To nie jest ozdoba, tylko warunek z §5.12 pkt 4 —
//! okno, które trzeba zamknąć, żeby cokolwiek kliknąć, zamienia naukę w formalność.
//!
//! **Skryptu, który klika za gracza.** Krok kończy się wtedy, gdy gracz zrobi to,
//! o co samouczek prosi — a nie wtedy, gdy minie czas. Postęp sprawdza się faktem
//! ze świata ([`Progress`]), nie licznikiem.
//!
//! # Skąd wiadomo, że mieści się w budżecie
//!
//! §5.12 daje sufit: dwanaście interakcji i trzy panele do pierwszej sensownej
//! decyzji. Samouczek prosi o **cztery** kliknięcia zmieniające świat (śledź siebie,
//! otwórz punkt, wyłóż koszyk, ustaw cenę) i otwiera **dwa** panele (Sklep, Pulpit) —
//! resztę budżetu zostawia graczowi, który błądzi. Pilnuje tego
//! [`crate::onboarding::measure`] na przebiegu skryptowym, a nie ten moduł.
//!
//! # `TutorialScriptId` nie powstaje
//!
//! §5.11 zapowiadał `tutorial: Option<TutorialScriptId>`, czyli wybór **jednego
//! z wielu** skryptów. Skrypt jest jeden i `data/scenarios/scenarios.ron` mówi o nim
//! `bool` — identyfikator wskazujący zawsze tę samą pozycję byłby numerem bez zbioru.
//! Drugi skrypt (np. samouczek produkcji dla M6) dopisze wariant do [`Step`] albo
//! własny `Script`, i wtedy identyfikator dostanie z czego wybierać.

use crate::Session;

/// Krok samouczka. Trzy, dokładnie te z §5.12 pkt 3.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// *Kim jesteś* — kamera na domu postaci, karta inspekcji, „śledź siebie".
    WhoAreYou,
    /// *Czego brakuje* — nakładka zasięgu sklepu i sąsiad, który nie ma gdzie kupić.
    WhatIsMissing,
    /// *Otwórz i wyceń* — punkt, koszyk, cena.
    OpenAndPrice,
}

impl Step {
    /// Kolejność kroków. Tablica, a nie `next()`, bo to jest **lista**, a nie
    /// automat: krok pominięty przeskakuje do następnego bez pytania o warunek.
    pub const ALL: [Step; 3] = [Step::WhoAreYou, Step::WhatIsMissing, Step::OpenAndPrice];

    /// Człon klucza tekstu: `ui.tutorial.<key>.title` i `.hint`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Step::WhoAreYou => "who",
            Step::WhatIsMissing => "missing",
            Step::OpenAndPrice => "open",
        }
    }

    /// Czy gracz zrobił to, o co ten krok prosi.
    #[must_use]
    pub const fn done(self, p: &Progress) -> bool {
        match self {
            Step::WhoAreYou => p.following_self,
            Step::WhatIsMissing => p.catchment_shown,
            Step::OpenAndPrice => p.priced,
        }
    }
}

/// Fakty, z których samouczek poznaje postęp.
///
/// Struktura, a nie zaglądanie do klienta: dwa z trzech kroków kończą się czynnością
/// **widoku** (śledzenie, nakładka), a widok stoi po drugiej stronie granicy z §5.2.
/// Klient składa te trzy bity z tego, co ma pod ręką, i oddaje — tak samo jak panel
/// składa komendę i oddaje ją sesji.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Progress {
    /// Gracz śledzi własną postać.
    pub following_self: bool,
    /// Nakładka zasięgu sklepu jest włączona.
    pub catchment_shown: bool,
    /// Gracz ustawił cenę w swoim sklepie — koniec samouczka.
    pub priced: bool,
}

impl Progress {
    /// Postęp z samej sesji, bez wiedzy o widoku.
    ///
    /// `priced` bierze się stąd, że gracz ma zakład i wydał `SetPrice` — jedno bez
    /// drugiego nie jest końcem: sklep bez ceny nie sprzedaje, a cena bez sklepu
    /// nie istnieje.
    #[must_use]
    pub fn of(session: &Session) -> Progress {
        let ma_zaklad = !crate::career::Holdings::of(session).sites.is_empty();
        let wycenil = session
            .log()
            .commands
            .iter()
            .any(|e| matches!(e.cmd, crate::PlayerCommand::SetPrice { .. }));
        Progress {
            following_self: false,
            catchment_shown: false,
            priced: ma_zaklad && wycenil,
        }
    }
}

/// Gdzie gracz jest w samouczku.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tutorial {
    krok: usize,
    porzucony: bool,
}

impl Tutorial {
    /// Zaczyna samouczek, jeśli scenariusz tej gry o niego prosi.
    ///
    /// `None` dla każdego innego scenariusza — i to jest cały czytelnik flagi
    /// `Scenario::tutorial`, której do `DI-35` nie miał nikt.
    #[must_use]
    pub fn start(session: &Session) -> Option<Tutorial> {
        session
            .scenario()
            .filter(|sc| sc.tutorial)
            .map(|_| Tutorial {
                krok: 0,
                porzucony: false,
            })
    }

    /// Krok, na którym gracz stoi. `None` = samouczek skończony albo porzucony.
    #[must_use]
    pub fn step(&self) -> Option<Step> {
        if self.porzucony {
            return None;
        }
        Step::ALL.get(self.krok).copied()
    }

    /// Przesuwa samouczek, jeśli bieżący krok został wykonany.
    pub fn advance(&mut self, p: &Progress) {
        while let Some(s) = self.step() {
            if !s.done(p) {
                break;
            }
            self.krok += 1;
        }
    }

    /// Pomija bieżący krok. **Każdy krok jest pomijalny** (§5.12 pkt 4) — samouczek,
    /// którego nie da się przeskoczyć, jest przeszkodą, a nie pomocą.
    pub fn skip_step(&mut self) {
        self.krok += 1;
    }

    /// Porzuca samouczek do końca gry.
    pub fn skip_all(&mut self) {
        self.porzucony = true;
    }

    /// Czy samouczek się skończył — przejściem albo porzuceniem.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.step().is_none()
    }

    /// Prędkość, na którą samouczek prosi przestawić zegar.
    ///
    /// Trzykrotna od pierwszego kroku (§5.12 pkt 3: „doba w 3×") i po ostatnim
    /// (pkt 6: „zwrot w ciągu jednej doby gry"). Prośba, nie rozkaz: klient może ją
    /// zignorować, a gracz przestawić zegar z powrotem.
    #[must_use]
    pub fn speed(&self) -> Option<magnat_core::SimSpeed> {
        (!self.porzucony).then_some(magnat_core::SimSpeed::X3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samouczek() -> Tutorial {
        Tutorial {
            krok: 0,
            porzucony: false,
        }
    }

    #[test]
    fn kroki_ida_po_kolei_i_kazdy_czeka_na_swoj_fakt() {
        let mut t = samouczek();
        assert_eq!(t.step(), Some(Step::WhoAreYou));
        let mut p = Progress::default();
        t.advance(&p);
        assert_eq!(t.step(), Some(Step::WhoAreYou), "krok bez faktu nie mija");
        p.following_self = true;
        t.advance(&p);
        assert_eq!(t.step(), Some(Step::WhatIsMissing));
        p.catchment_shown = true;
        p.priced = true;
        t.advance(&p);
        assert!(t.is_done(), "trzy fakty domykają trzy kroki");
    }

    #[test]
    fn kazdy_krok_da_sie_pominac() {
        let mut t = samouczek();
        for _ in Step::ALL {
            assert!(t.step().is_some());
            t.skip_step();
        }
        assert!(
            t.is_done(),
            "po pominięciu wszystkich kroków samouczek się kończy"
        );
    }

    #[test]
    fn porzucenie_konczy_samouczek_od_razu() {
        let mut t = samouczek();
        t.skip_all();
        assert!(t.is_done());
        assert_eq!(t.speed(), None, "porzucony samouczek nie rusza zegara");
    }

    /// §5.12 pkt 4: żaden tekst dłuższy niż czterdzieści słów. Liczone w **obu**
    /// językach, bo przekroczyć limit da się w każdym osobno.
    #[test]
    fn zaden_tekst_samouczka_nie_przekracza_czterdziestu_slow() {
        let c = magnat_ui::Catalog::load().expect("data/locale");
        for l in magnat_ui::Locale::ALL {
            for s in Step::ALL {
                for czesc in ["title", "hint"] {
                    let k = format!("ui.tutorial.{}.{czesc}", s.key());
                    let t = c.fmt_key(l, &k, &[]);
                    let slow = t.split_whitespace().count();
                    assert!(slow <= 40, "{k} w {l:?} ma {slow} słów, a limit to 40");
                    assert!(!t.is_empty(), "{k} w {l:?} jest pusty");
                }
            }
        }
    }
}
