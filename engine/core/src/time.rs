//! Czas symulacji: kalendarz 360-dniowy (00 §K-1) i częstotliwość systemów (00 §4).

use crate::types::{SimMinute, Tick};
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

/// Minuta doby, 0..=1439. Typ, a nie `u16`, bo plan dnia, godziny otwarcia i czasy
/// przybycia wymieniają się tą wartością przez granice trzech faz (M3 planer,
/// M4 podróże, M5 godziny handlu), a `u16` w tej roli milczy o tym, że 1500 nie istnieje.
/// Konstruktor zawija modulo doby — minuta 1445 to 5 minut po północy, nie błąd.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct MinuteOfDay(u16);

impl MinuteOfDay {
    pub const MIDNIGHT: MinuteOfDay = MinuteOfDay(0);

    #[inline]
    #[must_use]
    pub const fn new(v: u16) -> MinuteOfDay {
        MinuteOfDay(v % MINUTES_PER_DAY as u16)
    }

    #[inline]
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    #[inline]
    #[must_use]
    pub const fn hour(self) -> u8 {
        (self.0 / 60) as u8
    }

    #[inline]
    #[must_use]
    pub const fn minute(self) -> u8 {
        (self.0 % 60) as u8
    }

    /// Przesunięcie w przód z zawinięciem przez północ.
    #[inline]
    #[must_use]
    pub const fn plus(self, minutes: u16) -> MinuteOfDay {
        MinuteOfDay::new(self.0 % MINUTES_PER_DAY as u16 + minutes % MINUTES_PER_DAY as u16)
    }
}

impl std::fmt::Display for MinuteOfDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}", self.hour(), self.minute())
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

    /// Widok na `SimMinute`. Tick ekonomiczny **jest** minutą gry (00 §4), więc to
    /// przeliczenie jest tożsamością — istnieje po to, żeby konsument nie musiał sam
    /// zakładać, że nią jest.
    #[inline]
    #[must_use]
    pub const fn from_minute(m: SimMinute) -> SimCalendar {
        SimCalendar { tick: Tick(m.0) }
    }

    /// Minuta doby, 0..1439. Podstawa kąta godzinnego słońca (M1 §5.8).
    #[inline]
    #[must_use]
    pub const fn minute_of_day(self) -> u16 {
        (self.minutes() % MINUTES_PER_DAY) as u16
    }

    /// To samo, typowane — wejście planera dnia i godzin otwarcia (M3).
    #[inline]
    #[must_use]
    pub const fn minute_of_day_typed(self) -> MinuteOfDay {
        MinuteOfDay((self.minutes() % MINUTES_PER_DAY) as u16)
    }

    /// Dzień roku, 0..359. Podstawa deklinacji słońca i sezonowości (00 §K-1).
    #[inline]
    #[must_use]
    pub const fn day_of_year(self) -> u16 {
        (self.day_index() % (DAYS_PER_MONTH * MONTHS_PER_YEAR)) as u16
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

/// Prędkość biegu symulacji (PRD §14.5, decyzja 9.1 fazy M3).
///
/// **Prędkość nie wpływa na wynik.** Zmienia wyłącznie, ile ticków ekonomicznych
/// wypada na sekundę czasu realnego — sam tick jest zawsze tą samą minutą gry
/// i liczy się tak samo. Test `det_speed_invariance` porównuje hash doby przy 1×
/// i przy 10×: musi być identyczny.
///
/// 50× i tryb makro dokłada M12; wariant dopisuje się **na końcu** (00 §3.1).
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum SimSpeed {
    Paused = 0,
    #[default]
    X1 = 1,
    X3 = 2,
    X10 = 3,
}

impl SimSpeed {
    pub const ALL: &'static [SimSpeed] =
        &[SimSpeed::Paused, SimSpeed::X1, SimSpeed::X3, SimSpeed::X10];

    /// Minut gry na sekundę czasu realnego. 1× = 1 minuta/s, czyli doba w 24 minuty.
    #[inline]
    #[must_use]
    pub const fn minutes_per_second(self) -> u32 {
        match self {
            SimSpeed::Paused => 0,
            SimSpeed::X1 => 1,
            SimSpeed::X3 => 3,
            SimSpeed::X10 => 10,
        }
    }

    #[inline]
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            SimSpeed::Paused => "Paused",
            SimSpeed::X1 => "X1",
            SimSpeed::X3 => "X3",
            SimSpeed::X10 => "X10",
        }
    }
}

/// Zegar gry: ile minut symulacji należy wykonać za miniony czas realny.
///
/// Zegar **nie jest stanem symulacji** i nie wchodzi do hasha (00 §3.6) — jest
/// przelicznikiem po stronie prezentacji. Kod symulacji nie widzi go wcale, bo
/// widziałby wtedy czas rzeczywisty (00 §3.5). Jedyne, co z niego wychodzi, to liczba
/// ticków do wykonania; każdy z nich jest tą samą minutą gry niezależnie od prędkości.
///
/// Reszta milisekund **zostaje w akumulatorze**, a nie jest zaokrąglana: bez tego
/// przy 1× i klatce 16 ms nie ruszyłaby ani jedna minuta.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SimClock {
    tick: Tick,
    speed: SimSpeed,
    /// Nieskonsumowane milisekundy czasu realnego × minuty/s; dzielnik to 1000.
    accum: u32,
}

impl SimClock {
    #[must_use]
    pub const fn new(start: Tick) -> SimClock {
        SimClock {
            tick: start,
            speed: SimSpeed::X1,
            accum: 0,
        }
    }

    #[inline]
    #[must_use]
    pub const fn tick(self) -> Tick {
        self.tick
    }

    #[inline]
    #[must_use]
    pub const fn speed(self) -> SimSpeed {
        self.speed
    }

    /// Zmiana prędkości kasuje akumulator, żeby przełączenie nie wnosiło ułamka minuty
    /// naliczonego według poprzedniej prędkości.
    #[inline]
    pub fn set_speed(&mut self, speed: SimSpeed) {
        self.speed = speed;
        self.accum = 0;
    }

    #[inline]
    #[must_use]
    pub const fn calendar(self) -> SimCalendar {
        SimCalendar::new(self.tick)
    }

    /// Ile minut gry minęło przez `dt_ms` czasu realnego. Wywołujący ma wykonać
    /// dokładnie tyle ticków — zegar sam siebie przy tym przesuwa.
    ///
    /// `cap` ogranicza skok po zacięciu klatki: bez niego okno przywrócone po minucie
    /// w tle próbowałoby dogonić 600 minut gry w jednej klatce.
    pub fn advance(&mut self, dt_ms: u32, cap: u32) -> u32 {
        let na_sekunde = self.speed.minutes_per_second();
        if na_sekunde == 0 {
            return 0;
        }
        self.accum = self.accum.saturating_add(dt_ms.saturating_mul(na_sekunde));
        let minut = (self.accum / 1000).min(cap);
        self.accum -= minut.saturating_mul(1000);
        self.tick = Tick(self.tick.0 + u64::from(minut));
        minut
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
    fn minuta_doby_zawija_sie_przez_polnoc() {
        let m = MinuteOfDay::new(23 * 60 + 50);
        assert_eq!(m.to_string(), "23:50");
        assert_eq!(m.plus(20), MinuteOfDay::new(10));
        assert_eq!(MinuteOfDay::new(1440), MinuteOfDay::MIDNIGHT);
        assert_eq!(
            SimCalendar::new(Tick(MINUTES_PER_DAY + 61)).minute_of_day_typed(),
            MinuteOfDay::new(61)
        );
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

    #[test]
    fn predkosc_zmienia_tempo_a_nie_ziarnistosc() {
        // Doba gry to zawsze 1440 ticków — przy 1× trwa 1440 s realnych, przy 10× 144 s.
        for (predkosc, sekundy) in [
            (SimSpeed::X1, 1440u32),
            (SimSpeed::X3, 480),
            (SimSpeed::X10, 144),
        ] {
            let mut z = SimClock::new(Tick(0));
            z.set_speed(predkosc);
            let mut minut = 0u32;
            for _ in 0..sekundy * 1000 / 16 {
                minut += z.advance(16, 1000);
            }
            // Reszta akumulatora bierze się z dzielenia sekundy na klatki po 16 ms.
            assert!((1430..=1440).contains(&minut), "{predkosc:?}: {minut}");
            assert_eq!(z.tick().0, u64::from(minut));
        }
        let mut z = SimClock::new(Tick(7));
        z.set_speed(SimSpeed::Paused);
        assert_eq!(z.advance(10_000, 1000), 0);
        assert_eq!(z.tick().0, 7);
    }
}
