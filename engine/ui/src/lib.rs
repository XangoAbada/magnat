//! `magnat-ui` — szkielet interfejsu i karta inspekcji (M3, właściciel `engine/ui`).
//!
//! M3 stawia tu cztery rzeczy i ani jednej więcej:
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

pub mod inspect;
pub mod loc;
pub mod selection;
pub mod time;

pub use inspect::citizen::{zlotowki, CitizenCard, CitizenPanel, NeedRow, StatusRow};
pub use inspect::reason::{describe, zegar};
pub use inspect::timeline::{
    actual_from_trace, render_day_text, ActualBlock, CitizenHeader, DayTimeline, TimelineRow,
    DRIFT_HIGHLIGHT_MIN,
};
pub use inspect::{InspectorPanel, UiContext};
pub use loc::{Catalog, LocKey, Locale};
pub use selection::{ListPicker, NoPicker, PickResult, Picker, Selection};
pub use time::{TimeControlsWidget, CATCH_UP_CAP};
