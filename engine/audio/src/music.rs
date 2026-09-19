//! Muzyka adaptacyjna (M11d §5.9, WP8).
//!
//! Nastrój wyprowadza się ze stanu finansów gracza, a nie ze skryptu: `PlayerViewRec`
//! niesie płynność i kierunek zysku, i to są jedyne dwie liczby, które muzyka widzi.
//! Dwie, a nie saldo — muzyka ma reagować na **sytuację**, a nie na kwotę.
//!
//! ### Dwie reguły, bez których to nie działa
//!
//! **Histereza ±15 %**: płynność oscylująca wokół progu przełączałaby nastrój co kilka
//! sekund. Próg wejścia w nastrój jest inny niż próg wyjścia z niego, więc wahanie
//! w okolicy progu nie zmienia niczego.
//!
//! **Przejście wyłącznie na granicy taktu**: crossfade wpadający w środek frazy słychać
//! natychmiast jako błąd, nawet gdy trwa dwa takty. Zmiana nastroju **czeka** na
//! najbliższą granicę i dopiero wtedy startuje.

/// Nastrój muzyczny — cztery stemy w `data/audio/audio.ron` i cztery warianty tutaj.
/// Kolejność jest kontraktem, bo indeksuje listę stemów.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[repr(u8)]
pub enum MusicMood {
    /// Płynność rośnie, zysk w górę.
    Rozwoj = 0,
    /// Nic się nie dzieje i to jest dobrze.
    #[default]
    Stabilnosc = 1,
    /// Płynność cienka albo zysk spada.
    Napiecie = 2,
    /// Poniżej miesiąca kosztów stałych.
    Kryzys = 3,
}

impl MusicMood {
    pub const ALL: [MusicMood; 4] = [
        MusicMood::Rozwoj,
        MusicMood::Stabilnosc,
        MusicMood::Napiecie,
        MusicMood::Kryzys,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Progi płynności w tej samej skali co `PlayerViewRec.liquidity_ratio`
/// (płynność / 30-dniowe koszty stałe, przycięte do 0..255; 255 = jeden miesiąc).
const PROG_KRYZYS: i32 = 64;
const PROG_NAPIECIE: i32 = 140;
/// Histereza ±15 % progu (§5.9).
const HISTEREZA_PROC: i32 = 15;

/// Ile taktów trwa crossfade między stemami.
pub const CROSSFADE_BARS: u32 = 2;

/// Wybór nastroju i moment przejścia.
#[derive(Clone, Debug)]
pub struct MusicDirector {
    /// Nastrój, który **gra**.
    biezacy: MusicMood,
    /// Nastrój, do którego zmierzamy; `None` = żaden nie czeka.
    oczekujacy: Option<MusicMood>,
    /// Numer taktu, w którym rozpoczęło się bieżące przejście; `None` = nie trwa.
    przejscie_od: Option<u64>,
    /// Ostatni takt, w jakim byliśmy — wykrycie granicy taktu.
    ostatni_takt: u64,
    /// Ile razy nastrój faktycznie się zmienił. Do testu `music_hysteresis`.
    zmian: u32,
}

impl Default for MusicDirector {
    fn default() -> Self {
        MusicDirector {
            biezacy: MusicMood::Stabilnosc,
            oczekujacy: None,
            przejscie_od: None,
            ostatni_takt: 0,
            zmian: 0,
        }
    }
}

impl MusicDirector {
    #[must_use]
    pub fn new() -> MusicDirector {
        MusicDirector::default()
    }

    #[must_use]
    pub const fn mood(&self) -> MusicMood {
        self.biezacy
    }

    /// Nastrój, do którego trwa przejście — `None`, gdy żadne nie trwa.
    #[must_use]
    pub const fn pending(&self) -> Option<MusicMood> {
        self.oczekujacy
    }

    #[must_use]
    pub const fn changes(&self) -> u32 {
        self.zmian
    }

    /// Postęp crossfade'u 0..1; 1 = przejście skończone.
    #[must_use]
    pub fn crossfade(&self, takt: u64) -> f32 {
        match self.przejscie_od {
            None => 1.0,
            Some(od) => {
                let minelo = takt.saturating_sub(od) as f32;
                (minelo / CROSSFADE_BARS as f32).clamp(0.0, 1.0)
            }
        }
    }

    /// Krok dyrektora. `takt` to numer taktu od startu sesji — liczy go zegar miksera,
    /// bo tylko on wie, ile próbek naprawdę poszło na kartę.
    ///
    /// Zwraca `true`, gdy w tej chwili **rozpoczęło się** przejście — wtedy strona
    /// odtwarzająca zleca crossfade.
    pub fn step(&mut self, takt: u64, liquidity_ratio: u8, profit_trend: i8) -> bool {
        let chciany = self.docelowy(liquidity_ratio, profit_trend);
        if chciany != self.biezacy {
            self.oczekujacy = Some(chciany);
        } else {
            // Powrót do bieżącego nastroju **kasuje** oczekiwanie: nastrój, który minął,
            // zanim doszło do granicy taktu, nie ma po co dochodzić.
            self.oczekujacy = None;
        }

        let granica = takt > self.ostatni_takt;
        self.ostatni_takt = takt;
        // Przejście zaczyna się **wyłącznie** na granicy taktu i tylko wtedy, gdy
        // poprzednie się skończyło — nakładające się crossfade'y brzmią jak błąd miksu.
        if granica && self.crossfade(takt) >= 1.0 {
            if let Some(m) = self.oczekujacy.take() {
                self.biezacy = m;
                self.przejscie_od = Some(takt);
                self.zmian += 1;
                return true;
            }
        }
        false
    }

    /// Nastrój wynikający ze stanu finansów, z histerezą wokół progów.
    fn docelowy(&self, liquidity_ratio: u8, profit_trend: i8) -> MusicMood {
        let l = i32::from(liquidity_ratio);
        // Próg przesuwa się w stronę, w którą **nie** chcemy iść: wychodząc z kryzysu
        // trzeba przekroczyć próg z zapasem, wchodzący — spaść z zapasem.
        let prog = |p: i32, ciasniejszy: bool| {
            let d = p * HISTEREZA_PROC / 100;
            if ciasniejszy {
                p - d
            } else {
                p + d
            }
        };
        let w_kryzysie = self.biezacy == MusicMood::Kryzys;
        if l < prog(PROG_KRYZYS, !w_kryzysie) {
            return MusicMood::Kryzys;
        }
        let w_napieciu = matches!(self.biezacy, MusicMood::Napiecie | MusicMood::Kryzys);
        if l < prog(PROG_NAPIECIE, !w_napieciu) || profit_trend < -32 {
            return MusicMood::Napiecie;
        }
        if profit_trend > 24 {
            return MusicMood::Rozwoj;
        }
        MusicMood::Stabilnosc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `music_hysteresis` z §7.4: płynność oscylująca ±10 % wokół progu daje **zero**
    /// zmian nastroju.
    #[test]
    fn oscylacja_wokol_progu_nie_zmienia_nastroju() {
        let mut d = MusicDirector::new();
        // Wchodzimy w napięcie i tam zostajemy.
        for t in 0..8 {
            d.step(t, 100, 0);
        }
        assert_eq!(d.mood(), MusicMood::Napiecie);
        let przed = d.changes();

        // Sześćdziesiąt sekund przy ~84 BPM i takcie 4/4 to ok. 21 taktów.
        let amplituda = PROG_NAPIECIE * 10 / 100;
        for i in 0..21u64 {
            let l = if i % 2 == 0 {
                PROG_NAPIECIE - amplituda
            } else {
                PROG_NAPIECIE + amplituda
            };
            d.step(8 + i, l as u8, 0);
        }
        assert_eq!(d.changes(), przed, "histereza nie zadziałała");
    }

    /// Przejście zaczyna się wyłącznie na granicy taktu — `music_on_bar_boundary`.
    #[test]
    fn przejscie_czeka_na_granice_taktu() {
        let mut d = MusicDirector::new();
        assert_eq!(d.mood(), MusicMood::Stabilnosc);
        // Dziesięć wywołań **w tym samym takcie**: kryzys jest oczywisty, ale nie
        // ma gdzie się zacząć.
        for _ in 0..10 {
            assert!(!d.step(0, 0, 0), "przejście w środku taktu");
        }
        assert_eq!(d.mood(), MusicMood::Stabilnosc);
        assert_eq!(d.pending(), Some(MusicMood::Kryzys));
        // Następny takt — dopiero teraz.
        assert!(d.step(1, 0, 0));
        assert_eq!(d.mood(), MusicMood::Kryzys);
    }

    /// Crossfade trwa dwa takty i w tym czasie nie zaczyna się drugi.
    #[test]
    fn crossfade_nie_nachodzi_na_siebie() {
        let mut d = MusicDirector::new();
        assert!(d.step(1, 0, 0));
        assert_eq!(d.mood(), MusicMood::Kryzys);
        assert!((d.crossfade(1) - 0.0).abs() < 1e-6);
        assert!((d.crossfade(2) - 0.5).abs() < 1e-6);
        // W trakcie przejścia zmiana nastroju czeka.
        assert!(!d.step(2, 255, 100), "drugi crossfade wszedł w pierwszy");
        assert_eq!(d.mood(), MusicMood::Kryzys);
        // Po dwóch taktach przejście jest skończone i następne może wejść.
        assert!((d.crossfade(3) - 1.0).abs() < 1e-6);
        assert!(d.step(3, 255, 100));
        assert_eq!(d.mood(), MusicMood::Rozwoj);
    }

    /// Nastrój, który minął przed granicą taktu, nie dochodzi do skutku — inaczej
    /// jednoklatkowy skok płynności przestawiałby muzykę kilka sekund później.
    #[test]
    fn chwilowy_skok_nie_przestawia_muzyki() {
        let mut d = MusicDirector::new();
        d.step(0, 0, 0);
        assert_eq!(d.pending(), Some(MusicMood::Kryzys));
        // Płynność wróciła jeszcze w tym samym takcie.
        d.step(0, 200, 0);
        assert_eq!(d.pending(), None);
        assert!(!d.step(1, 200, 0));
        assert_eq!(d.mood(), MusicMood::Stabilnosc);
    }

    /// Cztery nastroje mają cztery różne wejścia — wariant, do którego nie da się
    /// dojść, przechodzi każdy test i wygląda tak samo jak działający.
    #[test]
    fn kazdy_nastroj_jest_osiagalny() {
        let mut widziane = std::collections::BTreeSet::new();
        for (l, p) in [(0u8, 0i8), (100, 0), (200, 100), (200, 0), (200, -100)] {
            let mut d = MusicDirector::new();
            for t in 0..8 {
                d.step(t, l, p);
            }
            widziane.insert(d.mood());
        }
        assert_eq!(widziane.len(), 4, "osiągalne: {widziane:?}");
    }
}
