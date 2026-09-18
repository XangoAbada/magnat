//! Karta inspekcji: nagłówek z tożsamością i zakładki (M9c §5.7, `Z-1`).
//!
//! # Kształt tutaj, treść w `game/`
//!
//! Ten moduł zna **kształt** karty — nagłówek zawsze widoczny, reszta w zakładkach
//! o stałej kolejności — i nic poza nim. Treść składa `game::inspect`, bo dopiero
//! stamtąd widać naraz świat ECS, miasto (`CityData`), rynek i rejestr firm; `engine/ui`
//! nie zależy od `sim/world` i nie ma powodu zaczynać (§6 dokumentu fazy wymienia
//! `InspectionCard` pod adresem `game::inspect` właśnie dlatego).
//!
//! # Zakładki, nie sekcje jedna pod drugą
//!
//! Decyzja właściciela produktu z 2026-09-17. Lista sekcji działa dla mieszkanki
//! z czterema polami i przestaje działać dla firmy z sześcioma obszarami. Układ jest
//! **stały per rodzaj podmiotu**: gracz, który nauczył się, że „Dlaczego" jest ostatnie
//! u mieszkanki, ma je znaleźć na tym samym miejscu u zakładu.
//!
//! **Pusta zakładka znika, a nie szarzeje** — pusta obiecuje treść, której nie ma.
//! Pilnuje tego [`InspectionCard::tab`]: kawałek bez tekstu nie wchodzi do karty.

use crate::loc::{Catalog, Locale};
use crate::rich::{Rich, RichExt, Span};
use magnat_core::Subject;

/// Zakładka karty. Kolejność wariantów jest kolejnością, w jakiej rysuje je widget,
/// a `key()` wskazuje klucz w `data/locale/` — literał tekstowy w UI jest błędem.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CardTabKind {
    /// Stan bieżący: potrzeby, gotówka, zdrowie, zużycie — zależnie od podmiotu.
    State,
    /// Historia podmiotu: ślad partii „od pola do półki", przebieg pojazdu, przeszłość
    /// firmy. Wchodzi w `M9e` razem z pierwszą kartą, która ma czym ją wypełnić
    /// (`DF-8`) — zakładka bez treści jest gorsza od jej braku.
    History,
    /// Dzień: plan wobec realizacji (mieszkaniec).
    Day,
    /// Rodzina: gospodarstwo, krewni, znajomi.
    Family,
    /// Majątek: mieszkanie, pojazdy, konta.
    Wealth,
    /// Praca: zakład, stanowisko, płaca.
    Work,
    /// Półki zakładu handlowego.
    Shelves,
    /// Klienci zakładu: skąd, kto, dlaczego.
    Customers,
    /// Konkurencja w zasięgu.
    Competition,
    /// Powody decyzji (`DecisionReason`) — zawsze ostatnia.
    Why,
    /// Jedyna zakładka podmiotów, które mają jedno ekranowe zdanie treści:
    /// partia, oferta, przetarg, sprawa, pozwolenie, umowa.
    Detail,
}

impl CardTabKind {
    /// Klucz tekstu zakładki: `ui.tab.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            CardTabKind::State => "state",
            CardTabKind::History => "history",
            CardTabKind::Day => "day",
            CardTabKind::Family => "family",
            CardTabKind::Wealth => "wealth",
            CardTabKind::Work => "work",
            CardTabKind::Shelves => "shelves",
            CardTabKind::Customers => "customers",
            CardTabKind::Competition => "competition",
            CardTabKind::Why => "why",
            CardTabKind::Detail => "detail",
        }
    }

    pub const ALL: [CardTabKind; 11] = [
        CardTabKind::State,
        CardTabKind::History,
        CardTabKind::Day,
        CardTabKind::Family,
        CardTabKind::Wealth,
        CardTabKind::Work,
        CardTabKind::Shelves,
        CardTabKind::Customers,
        CardTabKind::Competition,
        CardTabKind::Why,
        CardTabKind::Detail,
    ];

    #[must_use]
    pub fn title(self, c: &Catalog, l: Locale) -> String {
        c.fmt_key(l, &format!("ui.tab.{}", self.key()), &[])
    }
}

/// Jedna zakładka: rodzaj i treść. Treść jest [`Rich`], więc **każda nazwa w środku
/// może być odnośnikiem** (`Z-2`) — nie tylko powiązania wymienione z nazwy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CardTab {
    pub kind: CardTabKind,
    pub body: Rich,
}

/// Karta jednego podmiotu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InspectionCard {
    pub subject: Subject,
    /// Tożsamość — **zawsze widoczna**, nigdy w zakładce.
    pub header: Rich,
    pub tabs: Vec<CardTab>,
}

/// Sufit zakładek wymuszony przez `TabStrip` (M9b): powyżej siedmiu dzieli się panel,
/// a nie zwija etykiety (`docs/ui-design.md` §4).
pub const MAX_CARD_TABS: usize = 7;

impl InspectionCard {
    #[must_use]
    pub fn new(subject: Subject, header: Rich) -> InspectionCard {
        InspectionCard {
            subject,
            header,
            tabs: Vec::new(),
        }
    }

    /// Dokłada zakładkę. Pusta treść **nie tworzy zakładki** — pojazd nieużywany
    /// od roku nie ma „Użycia", a pusta zakładka jest gorsza od jej braku.
    ///
    /// # Panics
    /// Gdy karta przekroczy [`MAX_CARD_TABS`]. To jest błąd projektu karty, a nie
    /// stan danych: ósma zakładka nie zmieściłaby się w rzędzie i musiałaby zwinąć
    /// etykiety, czego `docs/ui-design.md` §4 zabrania.
    #[must_use]
    pub fn tab(mut self, kind: CardTabKind, body: Rich) -> InspectionCard {
        // Sam biały znak to też pusta zakładka: wiersz, w którym nic nie stanęło,
        // obiecuje treść tak samo jak pusty wektor.
        if body.is_blank() || body.to_plain().trim().is_empty() {
            return self;
        }
        assert!(
            self.tabs.len() < MAX_CARD_TABS,
            "karta {:?} ma więcej niż {MAX_CARD_TABS} zakładek",
            self.subject.kind()
        );
        self.tabs.push(CardTab { kind, body });
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Podmioty, do których karta ma odnośniki — wejście testu klikalności.
    #[must_use]
    pub fn links(&self) -> Vec<Subject> {
        self.header
            .iter()
            .chain(self.tabs.iter().flat_map(|t| t.body.iter()))
            .filter_map(|s| s.link)
            .collect()
    }

    /// Cała karta jako tekst: nagłówek i wszystkie zakładki pod sobą. To jest forma
    /// testowalna bez GPU i to ona jest złotym wydrukiem (00 §6).
    #[must_use]
    pub fn render_text(&self, c: &Catalog, l: Locale) -> String {
        let mut s = self.header.to_plain();
        for t in &self.tabs {
            s.push_str(&format!("[{}]\n", t.kind.title(c, l)));
            s.push_str(&t.body.to_plain());
        }
        s
    }
}

/// Tekst, którym kończy się ścieżka do celu, którego już nie ma (`Z-5`).
///
/// Firma upadła, mieszkaniec zmarł, partia się zużyła — `None` z rozwiązania podmiotu
/// jest **normalnym stanem świata**, nie błędem. Odnośnik prowadzący w pustkę jest
/// gorszy od jego braku, więc taka nazwa zostaje nazwą.
#[must_use]
pub fn gone_span(c: &Catalog, l: Locale, name: &str) -> Span {
    Span::plain(c.fmt_key(l, "ui.card.gone", &[("co", name)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{CitizenId, Entity, FirmId};
    use std::num::NonZeroU32;

    fn encja(i: u32) -> Entity {
        Entity::new(i, NonZeroU32::MIN)
    }

    fn katalog() -> Catalog {
        Catalog::load().expect("katalog tekstów")
    }

    #[test]
    fn pusta_zakladka_nie_powstaje() {
        let s = Subject::Citizen(CitizenId(encja(1)));
        let k = InspectionCard::new(s, crate::rich::lines("Anna\n"))
            .tab(CardTabKind::State, crate::rich::lines("głód 42\n"))
            .tab(CardTabKind::Day, Vec::new())
            .tab(CardTabKind::Why, crate::rich::lines("   \n"));
        assert_eq!(k.tabs.len(), 1, "pusta zakładka weszła do karty");
    }

    #[test]
    fn odnosniki_zbieraja_sie_z_naglowka_i_zakladek() {
        let anna = Subject::Citizen(CitizenId(encja(1)));
        let firma = Subject::Firm(FirmId(encja(2)));
        let k = InspectionCard::new(anna, vec![Span::link("Dobry Koszyk", firma)]).tab(
            CardTabKind::Why,
            vec![Span::link("Dobry Koszyk", firma), Span::plain("\n")],
        );
        assert_eq!(k.links(), vec![firma, firma]);
    }

    #[test]
    fn kazda_zakladka_ma_tytul_w_obu_jezykach() {
        let c = katalog();
        for l in Locale::ALL {
            for t in CardTabKind::ALL {
                let s = t.title(&c, l);
                assert!(!s.is_empty(), "zakładka {t:?} bez tytułu w {l:?}");
                assert!(!s.contains('{'), "niepodstawiony parametr w {s}");
            }
        }
    }
}
