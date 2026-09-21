//! Tekst, który da się kliknąć (`M9b` §5.8, `Z-7`).
//!
//! # Po co, skoro `String` działał
//!
//! Bo w `String` nie da się zakotwiczyć celu kliknięcia. `docs/ui-design.md` §4 mówi:
//! „każda nazwa innego podmiotu w karcie jest odnośnikiem" — a karta zwracała napis,
//! więc obietnica nie miała na czym stanąć. [`Span`] niesie obok tekstu
//! [`Subject`](magnat_core::Subject), czyli adres karty, do której prowadzi.
//!
//! # Dlaczego złoty wydruk nic nie traci
//!
//! [`Rich`] składa się z powrotem w ten sam napis przez [`RichExt::to_plain`], bo każdy
//! kawałek trzyma **swoje** znaki końca linii ([`lines`] tnie przez `split_inclusive`).
//! Konkatenacja jest więc tożsamościowa co do bajtu i to ona, a nie widget, jest wejściem
//! złotego testu (korekta E-8 do M3).
//!
//! `ponytail:` ziarnistość kawałka to dziś **jedna linia**. Sufit nazwany: odnośnik
//! w środku zdania („wybrała »Dobry Koszyk«") wymaga cięcia drobniejszego i zrobi je
//! `M9c` tam, gdzie faktycznie przypina link — dzielenie wszystkiego teraz dałoby
//! trzy razy więcej kawałków i ani jednego linku.

use magnat_core::Subject;

/// Rola kawałka tekstu. Styl, nie kolor — kolor bierze się z motywu (`theme.ron`),
/// żeby tryb wysokiego kontrastu był podmianą pliku, a nie przeglądem paneli.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SpanStyle {
    #[default]
    Normal,
    /// Wyróżnienie treściowe: nagłówek sekcji, etykieta wiersza.
    Emphasis,
    /// Liczba — krój tabelaryczny, wyrównanie do prawej tam, gdzie stoją w kolumnie.
    Number,
    /// Odnośnik do podmiotu. Bez [`Span::link`] jest to styl bez skutku i taki
    /// przypadek jest **poprawny**: cel, który przestał istnieć, renderuje się jako
    /// nazwa bez odnośnika (`M9c` `Z-5`).
    Link,
}

/// Kawałek tekstu o jednym stylu i najwyżej jednym celu kliknięcia.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
    pub link: Option<Subject>,
}

/// Tekst złożony z kawałków. Alias, a nie nowy typ — to ma być `Vec`, bo panele
/// budują go dopisywaniem i nic poza tym z nim nie robią.
pub type Rich = Vec<Span>;

impl Span {
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            style: SpanStyle::Normal,
            link: None,
        }
    }

    #[must_use]
    pub fn emphasis(text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            style: SpanStyle::Emphasis,
            link: None,
        }
    }

    #[must_use]
    pub fn number(text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            style: SpanStyle::Number,
            link: None,
        }
    }

    /// Odnośnik do podmiotu, który **istnieje**. Cel nieistniejący nie jest tu
    /// obsługiwany z rozmysłu: rozstrzyga o tym ten, kto go rozwiązuje, i wtedy
    /// buduje [`Span::plain`] plus zdanie o tym, co się stało.
    #[must_use]
    pub fn link(text: impl Into<String>, to: Subject) -> Span {
        Span {
            text: text.into(),
            style: SpanStyle::Link,
            link: Some(to),
        }
    }
}

/// Operacje na [`Rich`]. Trait, bo `Rich` jest aliasem na `Vec` i metod się na nim
/// nie definiuje inaczej.
pub trait RichExt {
    /// Napis, który widzi gracz — konkatenacja bez ani jednego dodanego znaku.
    fn to_plain(&self) -> String;
    /// Czy cokolwiek widać. Pusty wektor i wektor pustych napisów to to samo.
    fn is_blank(&self) -> bool;
}

impl RichExt for Rich {
    fn to_plain(&self) -> String {
        let n = self.iter().map(|s| s.text.len()).sum();
        let mut out = String::with_capacity(n);
        for s in self {
            out.push_str(&s.text);
        }
        out
    }

    fn is_blank(&self) -> bool {
        self.iter().all(|s| s.text.is_empty())
    }
}

/// Tnie gotowy napis na kawałki po liniach, **zachowując znaki końca linii**.
///
/// To jest droga, którą istniejące karty (sklep, zakład, firma) przechodzą z `String`
/// na [`Rich`] bez zmiany ani jednego bajtu wydruku. Każda linia jest osobnym kawałkiem,
/// bo linia jest jednostką, do której `M9c` przypina odnośnik.
#[must_use]
pub fn lines(text: &str) -> Rich {
    text.split_inclusive('\n').map(Span::plain).collect()
}

/// To samo co [`lines`], ale pierwsza linia dostaje [`SpanStyle::Emphasis`] —
/// nagłówek sekcji jest w kartach zawsze pierwszy.
#[must_use]
pub fn lines_titled(text: &str) -> Rich {
    let mut r = lines(text);
    if let Some(p) = r.first_mut() {
        p.style = SpanStyle::Emphasis;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{DistrictId, Subject};

    #[test]
    fn skladanie_z_powrotem_jest_tozsamoscia_co_do_bajtu() {
        for wzorzec in [
            "",
            "jedna linia",
            "dwie\nlinie",
            "z końcowym\nznakiem\n",
            "\n\npuste w środku\n",
            "ą ć ę ł ń ó ś ź ż\nĄĆĘŁŃÓŚŹŻ\n",
        ] {
            assert_eq!(lines(wzorzec).to_plain(), wzorzec, "wzorzec {wzorzec:?}");
        }
    }

    #[test]
    fn linia_jest_kawalkiem_a_naglowek_dostaje_wyroznienie() {
        let r = lines_titled("Klienci:\n  ktoś\n  ktoś inny\n");
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].style, SpanStyle::Emphasis);
        assert_eq!(r[1].style, SpanStyle::Normal);
        assert_eq!(r.to_plain(), "Klienci:\n  ktoś\n  ktoś inny\n");
    }

    #[test]
    fn odnosnik_niesie_cel_a_zwykly_tekst_nie() {
        let cel = Subject::District(DistrictId(3));
        let r: Rich = vec![
            Span::plain("dzielnica "),
            Span::link("Stare Miasto", cel),
            Span::plain("\n"),
        ];
        assert_eq!(r.to_plain(), "dzielnica Stare Miasto\n");
        assert_eq!(r[1].link, Some(cel));
        assert!(r[0].link.is_none());
        assert!(!r.is_blank());
        assert!(Rich::new().is_blank());
    }
}
