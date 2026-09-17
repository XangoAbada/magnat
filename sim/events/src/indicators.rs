//! Systemy dynamiczne stałe: wskaźniki miasta, zegar epoki i parametry demografii
//! (WP6, §5.6, PRD §11.3).
//!
//! **Cykl koniunkturalny nie jest symulowany — jest mierzony.** To nie jest skrót,
//! tylko granica fazy: model makro należy do M10, a tu powstaje **odczyt** stanu,
//! który zdarzenia zewnętrzne wzmacniają przez sondy. Każda z pięciu liczb ma
//! źródło, które już istnieje: inflację liczy `CpiTracker` (M5d), bezrobocie —
//! rynek pracy (M7b), przyrost kredytu — księga kredytów (M5d), nastrój — spis
//! mieszkańców (M3), a koniunkturę składa się z dwóch pierwszych.
//!
//! Wskaźnik bez źródła tu nie powstał. `MarketHeatBps` z §5.5 istnieje, bo daje się
//! złożyć z mierzonych liczb; `ImportDependencyBps` nie, bo nie daje się — i wariant
//! zwracający zero byłby sondą, która nigdy nie ruszy żadnej krzywej (`R2`).

use magnat_core::{HashState, Mood, StateHasher};

/// Wskaźniki miasta, liczone raz na miesiąc gry.
///
/// Miesięcznie, bo tak liczy je statystyka publiczna i tak czyta je gracz —
/// a przede wszystkim dlatego, że spis nastroju to przejście po całej populacji
/// i 30 razy na dobę byłoby kosztem bez treści.
#[derive(Clone, Copy, Debug)]
pub struct CityIndicators {
    /// Indeks cen konsumpcyjnych w punktach bazowych (10 000 = baza koszyka).
    pub cpi_index_bp: i32,
    /// Inflacja rok do roku w punktach bazowych. Zero, dopóki nie ma roku historii.
    pub cpi_yoy_bp: i32,
    /// Stopa bezrobocia w promilach siły roboczej.
    pub unemployment_permille: u32,
    /// Przyrost niespłaconego kapitału kredytowego wobec poprzedniego miesiąca,
    /// w punktach bazowych.
    pub credit_growth_bp: i32,
    /// Średni nastrój mieszkańców.
    pub mood_mean: Mood,
    /// Koniunktura w punktach bazowych: 10 000 to stan neutralny.
    pub heat_bps: u32,
    /// Miesiąc, dla którego policzone są powyższe. `u32::MAX` znaczy „jeszcze nigdy".
    pub month: u32,
    /// Poprzedni stan kredytu — potrzebny, bo przyrost jest różnicą, a księga
    /// zna wyłącznie poziom.
    prev_credit_gr: i64,
}

impl Default for CityIndicators {
    fn default() -> CityIndicators {
        CityIndicators {
            cpi_index_bp: 10_000,
            cpi_yoy_bp: 0,
            unemployment_permille: 0,
            credit_growth_bp: 0,
            mood_mean: Mood::NEUTRAL,
            heat_bps: 10_000,
            month: u32::MAX,
            prev_credit_gr: 0,
        }
    }
}

impl CityIndicators {
    /// Domyka miesiąc: zapisuje odczyty i składa z nich koniunkturę.
    pub fn close_month(
        &mut self,
        month: u32,
        cpi_index_bp: i32,
        cpi_yoy_bp: Option<i32>,
        unemployment_permille: u32,
        credit_gr: i64,
        mood_mean: Mood,
    ) {
        self.cpi_index_bp = cpi_index_bp;
        self.cpi_yoy_bp = cpi_yoy_bp.unwrap_or(0);
        self.unemployment_permille = unemployment_permille;
        self.credit_growth_bp = if self.month == u32::MAX || self.prev_credit_gr <= 0 {
            0
        } else {
            i32::try_from((credit_gr - self.prev_credit_gr) * 10_000 / self.prev_credit_gr)
                .unwrap_or(0)
        };
        self.prev_credit_gr = credit_gr;
        self.mood_mean = mood_mean;
        self.month = month;
        // Koniunktura: kredyt ciągnie w górę, bezrobocie w dół. Dwie mierzone
        // liczby i nic więcej — wskaźnik złożony z sześciu składników wygląda
        // mądrzej i nie daje się wytłumaczyć graczowi ani zestroić balansatorem.
        let bez = i32::try_from(unemployment_permille).unwrap_or(0);
        let v = 10_000 + self.credit_growth_bp.clamp(-3_000, 3_000) - (bez - 50) * 20;
        self.heat_bps = u32::try_from(v.clamp(2_000, 20_000)).unwrap_or(10_000);
    }

    /// Parametry demografii wyprowadzone ze wskaźników i z pogody (`D11` fazy M8).
    ///
    /// Każdy z czterech mnożników ma **mierzone** wejście, więc żaden nie stoi
    /// na stałej: dzietność spada przy bezrobociu i drożyźnie, umieralność rośnie
    /// w mrozie, napływ idzie za pracą, odpływ za jej brakiem.
    #[must_use]
    pub fn demography(&self, temp_dc: i16) -> magnat_agents::DemographyParams {
        let bez = i32::try_from(self.unemployment_permille).unwrap_or(0);
        let drozyzna = (self.cpi_yoy_bp - 300).max(0);
        let fert = (10_000 - (bez - 50) * 30 - drozyzna * 5).clamp(5_000, 13_000);
        let mroz = i32::from((-temp_dc).max(0));
        let smier = (10_000 + mroz * 10).clamp(10_000, 15_000);
        let imm = (10_000 + (50 - bez) * 40).clamp(2_000, 20_000);
        let emi = (10_000 + (bez - 50) * 30).clamp(5_000, 20_000);
        magnat_agents::DemographyParams {
            fertility_bps: u16::try_from(fert).unwrap_or(10_000),
            mortality_bps: u16::try_from(smier).unwrap_or(10_000),
            immigration_bps: u16::try_from(imm).unwrap_or(10_000),
            emigration_bps: u16::try_from(emi).unwrap_or(10_000),
        }
    }
}

impl HashState for CityIndicators {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.cpi_index_bp as u32);
        h.write_u32(self.cpi_yoy_bp as u32);
        h.write_u32(self.unemployment_permille);
        h.write_u32(self.credit_growth_bp as u32);
        h.write_u8(self.mood_mean.get() as u8);
        h.write_u32(self.heat_bps);
        h.write_u32(self.month);
        h.write_i64(self.prev_credit_gr);
    }
}

/// Zegar epoki: rok gry i epoka, w której miasto się znajduje.
///
/// **`unlocked: BitSet<TechId>` z §5.6 nie powstaje.** Nie dlatego, że jest trudne,
/// tylko dlatego, że `TechId` nie istnieje: drzewa technologii mieszkają
/// w `data/tech/`, którego właścicielem jest M10, a zegar bez drzewa wystawiałby
/// pusty zbiór i wyglądałby na działający (`R2`). Rok i epoka mają dziś czytelników
/// — sondę `SeasonIndex`, bramkę epoki w katalogu zdarzeń i raport scenariusza —
/// i to jest cały zakres, jaki M8c może domknąć uczciwie.
#[derive(Clone, Copy, Debug, Default)]
pub struct EpochClock {
    /// Rok kalendarzowy, w którym zaczyna się gra.
    pub start_year: u16,
    /// Ile pełnych lat gry minęło.
    pub years_elapsed: u16,
}

impl EpochClock {
    #[must_use]
    pub fn new(start_year: u16) -> EpochClock {
        EpochClock {
            start_year,
            years_elapsed: 0,
        }
    }

    /// Bieżący rok kalendarzowy gry.
    #[must_use]
    pub fn year(&self) -> u16 {
        self.start_year.saturating_add(self.years_elapsed)
    }

    /// Ustawia rok z doby świata (kalendarz `K-1`: 360 dób).
    pub fn set_day(&mut self, day: u64) {
        self.years_elapsed = u16::try_from(day / 360).unwrap_or(u16::MAX);
    }
}

impl HashState for EpochClock {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.start_year);
        h.write_u16(self.years_elapsed);
    }
}
