//! Formatowanie liczb, pieniądza i dat per język (`M9b` §5.8, `ui-design.md` §3.2).
//!
//! **Widget nie formatuje liczb sam.** Przecinek dziesiętny, separator tysięcy
//! i pozycja waluty zależą od języka, a język zmienia się w trakcie gry bez restartu —
//! więc gdyby każdy panel składał napis po swojemu, zmiana języka poprawiałaby część
//! ekranu i zostawiała resztę po staremu.
//!
//! Separator tysięcy w polskim to **spacja nierozdzielająca** (U+00A0), a nie zwykła:
//! „12 345 zł" złamane na końcu wiersza czyta się jak dwie różne kwoty.

use crate::loc::{Catalog, Locale};
use magnat_core::{Money, SimCalendar};

/// Separator tysięcy.
#[must_use]
pub const fn group_sep(l: Locale) -> char {
    match l {
        Locale::Pl => '\u{a0}',
        Locale::En => ',',
    }
}

/// Separator dziesiętny.
#[must_use]
pub const fn decimal_sep(l: Locale) -> char {
    match l {
        Locale::Pl => ',',
        Locale::En => '.',
    }
}

/// Liczba całkowita z separatorem tysięcy: `1 284 300` / `1,284,300`.
#[must_use]
pub fn integer(l: Locale, n: i64) -> String {
    let sep = group_sep(l);
    let cyfry = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(cyfry.len() + cyfry.len() / 3 + 1);
    if n < 0 {
        // Minus typograficzny (U+2212), nie łącznik: w kolumnie kwot ma mieć
        // szerokość cyfry, inaczej liczby ujemne przestają się wyrównywać.
        out.push('\u{2212}');
    }
    let pierwsza = cyfry.len() % 3;
    for (i, c) in cyfry.chars().enumerate() {
        if i > 0 && i % 3 == pierwsza {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Ułamek dziesiętny z `frac` cyframi po przecinku, podany w najmniejszej jednostce.
///
/// `decimal(Pl, 1_234_567, 2)` → `12 345,67`.
#[must_use]
pub fn decimal(l: Locale, units: i64, frac: u32) -> String {
    let dzielnik = 10i64.pow(frac);
    let calosc = units / dzielnik;
    let reszta = (units % dzielnik).unsigned_abs();
    let mut out = integer(l, calosc);
    // Kwota z zerową częścią całkowitą i ujemną resztą („−0,01") gubiłaby znak,
    // bo `integer(0)` nie ma czego podpisać.
    if units < 0 && calosc == 0 {
        out.insert(0, '\u{2212}');
    }
    if frac > 0 {
        out.push(decimal_sep(l));
        out.push_str(&format!("{reszta:0width$}", width = frac as usize));
    }
    out
}

/// Kwota z walutą: `12 345,67 zł`.
///
/// Waluta idzie z katalogu (`ui.fmt.money`), bo jej pozycja względem liczby jest
/// cechą języka, a nie kodu.
#[must_use]
pub fn money(c: &Catalog, l: Locale, m: Money) -> String {
    c.fmt_key(l, "ui.fmt.money", &[("kwota", &decimal(l, m.0, 2))])
}

/// Liczba z jednostką odmienioną przez liczebnik: `3 sklepy`, `5 sklepów`.
#[must_use]
pub fn count(c: &Catalog, l: Locale, key: &str, n: u64) -> String {
    c.plural(l, c.must(key), n)
}

/// Formatowanie dat w kalendarzu **12 × 30** (`K-1`) i tylko w nim.
///
/// Nie ma tu lat przestępnych ani miesięcy o różnej długości, więc arytmetyka osi czasu
/// jest całkowitoliczbowa, a oś wykresu ma równe podziałki miesięczne bez dryfu.
pub struct CalendarFmt;

impl CalendarFmt {
    /// `Rok 3, 14 marca, wt`.
    #[must_use]
    pub fn date(c: &Catalog, l: Locale, k: SimCalendar) -> String {
        let miesiac = c.fmt_key(l, &format!("ui.month.{}", k.month_of_year()), &[]);
        let dow = c.fmt_key(l, &format!("ui.dow.{:?}", k.day_of_week()), &[]);
        c.fmt_key(
            l,
            "ui.time.date",
            &[
                ("rok", &k.year().to_string()),
                ("miesiac", &miesiac),
                ("dzien", &k.day_of_month().to_string()),
                ("dow", &dow),
            ],
        )
    }

    /// `08:42`.
    #[must_use]
    pub fn clock(c: &Catalog, l: Locale, k: SimCalendar) -> String {
        c.fmt_key(
            l,
            "ui.time.clock",
            &[
                ("godzina", &format!("{:02}", k.hour_of_day())),
                ("minuta", &format!("{:02}", k.minute_of_hour())),
            ],
        )
    }

    /// Krótka data bez dnia tygodnia — podpis podziałki osi: `3/07/14`.
    ///
    /// Numerycznie, nie słownie: podziałek na osi dziesięcioletniej jest 120,
    /// a „14 marca" w każdej z nich zamieniłoby oś w ścianę tekstu.
    #[must_use]
    pub fn axis_day(k: SimCalendar) -> String {
        format!(
            "{}/{:02}/{:02}",
            k.year(),
            k.month_of_year(),
            k.day_of_month()
        )
    }

    /// Podpis podziałki miesięcznej: `3/07`.
    #[must_use]
    pub fn axis_month(k: SimCalendar) -> String {
        format!("{}/{:02}", k.year(), k.month_of_year())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NBSP: char = '\u{a0}';
    const MINUS: char = '\u{2212}';

    #[test]
    fn separator_tysiecy_zalezy_od_jezyka() {
        assert_eq!(
            integer(Locale::Pl, 1_284_300),
            format!("1{NBSP}284{NBSP}300")
        );
        assert_eq!(integer(Locale::En, 1_284_300), "1,284,300");
        assert_eq!(integer(Locale::Pl, 0), "0");
        assert_eq!(integer(Locale::Pl, 999), "999");
        assert_eq!(integer(Locale::Pl, 1000), format!("1{NBSP}000"));
        assert_eq!(integer(Locale::En, -12_345), "−12,345");
    }

    #[test]
    fn grosze_nie_gina_i_nie_gubia_znaku() {
        assert_eq!(decimal(Locale::Pl, 1_234_567, 2), format!("12{NBSP}345,67"));
        assert_eq!(decimal(Locale::En, 1_234_567, 2), "12,345.67");
        assert_eq!(decimal(Locale::Pl, 0, 2), "0,00");
        assert_eq!(decimal(Locale::Pl, 5, 2), "0,05");
        // Pułapka: −0,01 ma minus, choć część całkowita jest zerem.
        assert_eq!(decimal(Locale::Pl, -1, 2), format!("{MINUS}0,01"));
        assert_eq!(decimal(Locale::Pl, -101, 2), format!("{MINUS}1,01"));
        assert_eq!(decimal(Locale::Pl, 100, 0), format!("100"));
    }

    #[test]
    fn kwota_bierze_walute_z_katalogu() {
        let c = Catalog::load().expect("data/locale/");
        assert_eq!(
            money(&c, Locale::Pl, Money(1_234_567)),
            format!("12{NBSP}345,67 zł")
        );
        assert_eq!(money(&c, Locale::En, Money(1_234_567)), "12,345.67 zł");
    }

    #[test]
    fn data_i_zegar_w_kalendarzu_12x30() {
        let c = Catalog::load().expect("data/locale/");
        // Rok liczy się od zera (`SimCalendar::year`), miesiąc i dzień od jedynki.
        // Doba 400 to rok 1, drugi miesiąc, jedenasty dzień — 360 dób w roku (`K-1`).
        let k = SimCalendar::new(magnat_core::Tick(400 * 1440 + 8 * 60 + 42));
        assert_eq!((k.year(), k.month_of_year(), k.day_of_month()), (1, 2, 11));
        assert_eq!(CalendarFmt::clock(&c, Locale::Pl, k), "08:42");
        assert_eq!(CalendarFmt::axis_month(k), "1/02");
        assert_eq!(CalendarFmt::axis_day(k), "1/02/11");
        let d = CalendarFmt::date(&c, Locale::Pl, k);
        assert!(d.contains("lutego"), "{d}");
        assert!(!d.contains('{'), "niepodstawiony parametr: {d}");
    }
}
