//! `magnat-ui` — szkielet interfejsu i karta inspekcji (M3, właściciel `engine/ui`).
//!
//! M3 postawił tu cztery rzeczy; M9b dokłada rdzeń UI (WP3, WP6): [`rich`] (tekst
//! z odnośnikiem), [`theme`] (motyw z `data/ui/theme.ron`), [`cache`] (przebudowa
//! modelu tylko przy zmianie danych), [`fmt`] (liczby, pieniądz i daty per język),
//! [`tabs`], [`table`], [`chart`] i [`heatmap`].
//!
//! Cztery rzeczy M3:
//! - **lokalizację** (`loc`) — `LocKey`, katalogi `data/locale/{pl,en}.ron`, liczebniki;
//!   literał tekstowy w kodzie UI jest błędem, nie skrótem (CLAUDE.md),
//! - **sterowanie czasem** (`time`) — widget nad `SimClock` z `engine/core` (decyzja 9.1),
//! - **selekcję** (`selection`) — model zaznaczenia i interfejs wskazywania (decyzja 9.3),
//! - **kartę inspekcji** (`inspect`) — potrzeby, majątek, status i oś dnia z planem
//!   obok realizacji, plus **jedyne** w całej grze miejsce zamiany `DecisionReason`
//!   na tekst (00 §K-12).
//!
//! Czego tu **nie ma i gdzie to jest**: panele biznesowe, edytor reguł i automatyzacja
//! polityk — M9; styl, animacje i dźwięk — M11; pełna lokalizacja formatów liczb i dat
//! oraz modding — M12.
//!
//! Crate jest **niezależny od GPU**: rysowaniem zajmuje się klient, a `magnat-ui`
//! buduje model tego, co ma być narysowane. Dzięki temu karta inspekcji i wydruk dnia
//! są testowalne w CI bez karty graficznej — co jest wymogiem headless-first (00 §6).
//!
//! **Czego tu jeszcze nie ma** (korekta H-15 do M3d): warstwy rysującej. Nie ma bufora
//! identyfikatorów w `engine/render`, więc [`Picker`] nie ma produkcyjnej implementacji
//! — zostaje [`ListPicker`], czyli wybór po liście mieszkańców (fallback z ryzyka R8).
//! Nie ma też wpięcia `egui`, więc panele istnieją jako model i wydruk tekstowy,
//! a nie jako okna. Decyzje 9.2 i 9.3 fazy M3 są z tego powodu nadal otwarte.

#![forbid(unsafe_code)]

pub mod cache;
pub mod chart;
pub mod fmt;
pub mod heatmap;
pub mod inspect;
pub mod loc;
pub mod names;
pub mod rich;
pub mod selection;
pub mod table;
pub mod tabs;
pub mod testing;
pub mod theme;
pub mod time;
pub mod widgets;

pub use cache::{Cached, DataSource, Versions};
pub use chart::{chart, MipLevel, Sample, Series, SeriesKey};
pub use fmt::CalendarFmt;
pub use heatmap::HeatmapThumb;
pub use inspect::citizen::{zlotowki, CitizenCard, CitizenModel, CitizenPanel, NeedRow, StatusRow};
pub use inspect::reason::{describe, zegar};
pub use inspect::shop::{ShopCard, ShopTab, ShopView};
pub use inspect::supply::{SupplyCard, SupplyTab, SupplyView};
pub use inspect::timeline::{
    actual_from_trace, render_day_text, ActualBlock, CitizenHeader, DayTimeline, TimelineRow,
    DRIFT_HIGHLIGHT_MIN,
};
pub use inspect::trip::{
    infeasible, option_name, place, CandidateRow, TimeSplit, TripCard, TripView, VehicleCard,
    VehicleView,
};
pub use inspect::{InspectorPanel, UiContext};
pub use loc::{Catalog, LocKey, Locale};
pub use names::full_name;
pub use rich::{lines, lines_titled, Rich, RichExt, Span, SpanStyle};
pub use selection::{ListPicker, NoPicker, PickResult, Picker, Selection};
pub use table::{Align, Column, ColumnId, RowSource, SortDir, Table};
pub use tabs::tab_strip;
pub use theme::{ColorToken, TextRole, Theme};
pub use time::{TimeControlsWidget, CATCH_UP_CAP};
