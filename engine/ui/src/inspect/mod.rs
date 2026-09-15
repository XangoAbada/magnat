//! Karta inspekcji (M3d §5.11).
//!
//! Panel jest **budowniczym modelu**, nie rysownikiem: `draw` dostaje świat i zwraca
//! to, co ma się pojawić na ekranie, a rysowanie należy do klienta. Rozdział nie jest
//! ozdobą — dzięki niemu złoty test wydruku i testy karty biegną w CI bez GPU, a M11
//! może zmienić styl bez dotykania treści.

pub mod citizen;
pub mod reason;
pub mod shop;
pub mod timeline;
pub mod trip;

use crate::loc::{Catalog, Locale};
use crate::selection::Selection;
use crate::time::TimeControlsWidget;
use magnat_ecs::World;

/// Kontekst jednej klatki interfejsu.
///
/// Trzyma to, co jest wspólne dla wszystkich paneli: katalog tekstów, język, zaznaczenie
/// i sterowanie czasem. M9 dokłada tu rejestr paneli i układ okien.
pub struct UiContext {
    pub catalog: Catalog,
    pub locale: Locale,
    pub selection: Selection,
    pub time: TimeControlsWidget,
}

impl UiContext {
    /// Wczytuje katalog tekstów i ustawia zegar na podany tick.
    pub fn new(locale: Locale, start: magnat_core::Tick) -> Result<UiContext, crate::loc::LocError> {
        Ok(UiContext {
            catalog: Catalog::load()?,
            locale,
            selection: Selection::None,
            time: TimeControlsWidget::new(start),
        })
    }

    /// Skrót do tekstu po kluczu — panele nie sięgają do katalogu same.
    #[must_use]
    pub fn text(&self, key: &str) -> String {
        self.catalog.fmt_key(self.locale, key, &[])
    }
}

/// Panel inspekcji. M9 rejestruje tu panele biznesowe, M11 dokłada styl.
///
/// `build` zamiast `draw`: panel produkuje **model** tego, co ma być narysowane,
/// i nie zna biblioteki graficznej. To jest ta sama zasada co przy `sim-snapshot`
/// (00 §1) — warstwa prezentacji nie ma prawa mutować symulacji, więc dostaje `&World`.
pub trait InspectorPanel {
    /// Tytuł panelu w języku gracza.
    fn title(&self, ui: &UiContext) -> String;

    /// Treść panelu jako tekst — jedyna forma wymagana w M3. Widget graficzny M9
    /// czyta ten sam model, nie drugi.
    fn build(&mut self, ui: &UiContext, world: &World) -> String;
}
