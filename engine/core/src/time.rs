//! Czas symulacji: kalendarz 360-dniowy (00 §K-1) i częstotliwość systemów (00 §4).

use crate::types::Tick;
use serde::{Deserialize, Serialize};

pub const MINUTES_PER_HOUR: u64 = 60;
pub const HOURS_PER_DAY: u64 = 24;
pub const DAYS_PER_MONTH: u64 = 30; // 00 §K-1: kalendarz 360-dniowy
pub const MONTHS_PER_YEAR: u64 = 12; // 12 × 30 = 360 dni, BEZ lat przestępnych
pub const DAYS_PER_WEEK: u64 = 7; // 00 §K-15: tydzień dryfuje względem miesiąca
pub const MICRO_TICK_MS: u64 = 100; // tick ruchu mikro

pub const MINUTES_PER_DAY: u64 = MINUTES_PER_HOUR * HOURS_PER_DAY; // 1440
pub const MINUTES_PER_MONTH: u64 = MINUTES_PER_DAY * DAYS_PER_MONTH; // 43 200
pub const MINUTES_PER_YEAR: u64 = MINUTES_PER_MONTH * MONTHS_PER_YEAR; // 518 400
/// Ile podkroków ruchu mikro mieści się w jednym ticku ekonomicznym (D-3).
pub const MICRO_STEPS_PER_TICK: u64 = 60_000 / MICRO_TICK_MS; // 600

/// Dzień tygodnia wyprowadzony z absolutnego numeru doby modulo 7 (00 §K-15).
/// Tydzień **nie jest** podwielokrotnością miesiąca — 30 nie dzieli się przez 7 —
/// więc pierwszy dzień roku wypada co roku w inny dzień tygodnia. To jest zamierzone:
/// rytm tygodniowy jest realny w handlu i w grafikach zmianowych, a agregacja wykresów
/// idzie po dekadach (3 × 10 dni), nie po tygodniach.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[repr(u8)]
pub enum DayOfWeek {
    Monday = 0,
    Tuesday = 1,
    Wednesday = 2,
    Thursday = 3,
    Friday = 4,
    Saturday = 5,
    Sunday = 6,
}

impl DayOfWeek {
    /// Doba 0 świata to poniedziałek — punkt zaczepienia kalendarza.
    #[inline]
    #[must_use]
    pub const fn from_day_index(day: u64) -> DayOfWeek {
        match day % DAYS_PER_WEEK {
            0 => DayOfWeek::Monday,
            1 => DayOfWeek::Tuesday,
            2 => DayOfWeek::Wednesday,
            3 => DayOfWeek::Thursday,
            4 => DayOfWeek::Friday,
            5 => DayOfWeek::Saturday,
            _ => DayOfWeek::Sunday,
        }
    }

    #[inline]
    #[must_use]
    pub const fn is_weekend(self) -> bool {
        matches!(self, DayOfWeek::Saturday | DayOfWeek::Sunday)
    }
}

/// Widok kalendarzowy na `Tick`. Czysta arytmetyka, bez stanu.
///
/// Kalendarz jest STAŁY: 12 miesięcy po 30 dni, rok = 360 dni, brak lat przestępnych,
/// każdy miesiąc tak samo długi (00 §K-1). Konsekwencje, na których polegają kolejne fazy:
/// okres rozliczeniowy, pensja, czynsz i odsetki są zawsze tej samej długości, więc
/// porównania rok do roku nie wymagają normalizacji, a testy regresji balansu (M5)
/// nie mają szumu z lutego. Sezony (M1, M8) dzielą rok na cztery równe kwartały po 90 dni.
/// To nie jest uproszczenie do poprawienia później — zmiana na kalendarz gregoriański
/// unieważniłaby każdy zapis gry i każdą linię bazową balansatora.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SimCalendar {
    pub tick: Tick,
}

impl SimCalendar {
    #[inline]
    #[must_use]
    pub const fn new(tick: Tick) -> SimCalendar {
        SimCalendar { tick }
    }

    #[inline]
    const fn minutes(self) -> u64 {
        self.tick.0
    }

    #[inline]
    #[must_use]
    pub const fn minute_of_hour(self) -> u8 {
        (self.minutes() % MINUTES_PER_HOUR) as u8
    }

    #[inline]
    #[must_use]
    pub const fn hour_of_day(self) -> u8 {
        (self.minutes() / MINUTES_PER_HOUR % HOURS_PER_DAY) as u8
    }

    /// Absolutny numer doby od startu świata — podstawa dnia tygodnia (00 §K-15).
    #[inline]
    #[must_use]
    pub const fn day_index(self) -> u64 {
        self.minutes() / MINUTES_PER_DAY
    }

    #[inline]
    #[must_use]
    pub const fn day_of_month(self) -> u8 {
        (self.day_index() % DAYS_PER_MONTH) as u8 + 1
    }

    #[inline]
    #[must_use]
    pub const fn month_of_year(self) -> u8 {
        (self.day_index() / DAYS_PER_MONTH % MONTHS_PER_YEAR) as u8 + 1
    }

    #[inline]
    #[must_use]
    pub const fn year(self) -> u32 {
        (self.minutes() / MINUTES_PER_YEAR) as u32
    }

    #[inline]
    #[must_use]
    pub const fn day_of_week(self) -> DayOfWeek {
        DayOfWeek::from_day_index(self.day_index())
    }

    /// Dekada miesiąca: 0, 1 albo 2 (10 dni każda). Dekada dzieli miesiąc bez reszty,
    /// więc sumy agregatów są bezstratne — tydzień tego nie robi (00 §K-15).
    #[inline]
    #[must_use]
    pub const fn decade_of_month(self) -> u8 {
        (self.day_of_month() - 1) / 10
    }

    #[inline]
    #[must_use]
    pub const fn is_hour_boundary(self) -> bool {
        self.minutes().is_multiple_of(MINUTES_PER_HOUR)
    }

    #[inline]
    #[must_use]
    pub const fn is_day_boundary(self) -> bool {
        self.minutes().is_multiple_of(MINUTES_PER_DAY)
    }

    #[inline]
    #[must_use]
    pub const fn is_month_boundary(self) -> bool {
        self.minutes().is_multiple_of(MINUTES_PER_MONTH)
    }
}

/// Częstotliwość systemu (00 §4). Scheduler odpytuje ją co tick.
///
/// `EveryMicroTick` jest należny w **każdym** ticku ekonomicznym: tick to minuta,
/// a system mikro wykonuje wewnątrz niej 600 podkroków po 100 ms (D-3). Jego wynik
/// staje się widoczny dopiero na granicy minuty.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Cadence {
    EveryMicroTick,
    EveryMinute,
    EveryHour,
    EveryDay,
    EveryMonth,
}

impl Cadence {
    /// Czy system ma się wykonać w tym ticku ekonomicznym.
    #[inline]
    #[must_use]
    pub const fn due(self, tick: Tick) -> bool {
        match self {
            Cadence::EveryMicroTick | Cadence::EveryMinute => true,
            Cadence::EveryHour => tick.0.is_multiple_of(MINUTES_PER_HOUR),
            Cadence::EveryDay => tick.0.is_multiple_of(MINUTES_PER_DAY),
            Cadence::EveryMonth => tick.0.is_multiple_of(MINUTES_PER_MONTH),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rok_ma_360_dni() {
        assert_eq!(MINUTES_PER_YEAR, 360 * 24 * 60);
        let ostatnia_minuta_roku = SimCalendar::new(Tick(MINUTES_PER_YEAR - 1));
        assert_eq!(ostatnia_minuta_roku.year(), 0);
        assert_eq!(ostatnia_minuta_roku.month_of_year(), 12);
        assert_eq!(ostatnia_minuta_roku.day_of_month(), 30);
        assert_eq!(ostatnia_minuta_roku.hour_of_day(), 23);
        assert_eq!(ostatnia_minuta_roku.minute_of_hour(), 59);

        let nowy_rok = SimCalendar::new(Tick(MINUTES_PER_YEAR));
        assert_eq!(nowy_rok.year(), 1);
        assert_eq!(nowy_rok.month_of_year(), 1);
        assert_eq!(nowy_rok.day_of_month(), 1);
    }

    #[test]
    fn tydzien_dryfuje_wzgledem_miesiaca() {
        // 360 dni nie dzieli się przez 7: 360 = 51*7 + 3, więc rok przesuwa dzień
        // tygodnia o 3. To jest treść K-15, nie błąd.
        let start = SimCalendar::new(Tick(0));
        let rok_pozniej = SimCalendar::new(Tick(MINUTES_PER_YEAR));
        assert_eq!(start.day_of_week(), DayOfWeek::Monday);
        assert_eq!(rok_pozniej.day_of_week(), DayOfWeek::Thursday);
        assert!(DayOfWeek::Saturday.is_weekend());
        assert!(!DayOfWeek::Friday.is_weekend());
    }

    #[test]
    fn dekady_dziela_miesiac_bez_reszty() {
        assert_eq!(SimCalendar::new(Tick(0)).decade_of_month(), 0);
        assert_eq!(
            SimCalendar::new(Tick(10 * MINUTES_PER_DAY)).decade_of_month(),
            1
        );
        assert_eq!(
            SimCalendar::new(Tick(29 * MINUTES_PER_DAY)).decade_of_month(),
            2
        );
    }

    #[test]
    fn cadence_trafia_w_granice() {
        assert!(Cadence::EveryMinute.due(Tick(7)));
        assert!(Cadence::EveryHour.due(Tick(120)));
        assert!(!Cadence::EveryHour.due(Tick(121)));
        assert!(Cadence::EveryDay.due(Tick(MINUTES_PER_DAY)));
        assert!(!Cadence::EveryDay.due(Tick(MINUTES_PER_DAY + 1)));
        assert!(Cadence::EveryMonth.due(Tick(MINUTES_PER_MONTH)));
        assert!(!Cadence::EveryMonth.due(Tick(MINUTES_PER_MONTH - 1)));
        // Mikro biegnie w każdej minucie; 600 podkroków dzieje się wewnątrz systemu.
        assert!(Cadence::EveryMicroTick.due(Tick(1)));
        assert_eq!(MICRO_STEPS_PER_TICK, 600);
    }
}
