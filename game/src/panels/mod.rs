//! Panele biznesowe (M9e §5.9, PRD §14.3) — rejestr, układ doku i stan.
//!
//! # Dziewięć paneli i jeden rejestr
//!
//! [`PanelRegistry`] jest **punktem rozszerzenia**, a nie abstrakcją na zapas:
//! M10 dokłada panele marki, R&D i giełdy, M12 panele z modów, i żaden z nich nie
//! ma dotykać kodu tej podfazy. Dlatego opis panelu jest **daną** ([`PanelDesc`]) —
//! nazwa, zależności danych, funkcja rysująca i etap kariery, przy którym panel
//! jest domyślnie przypięty.
//!
//! # `min_tier` nie blokuje
//!
//! To jest cała treść §13.2: żadna komenda nie sprawdza etapu kariery, a panel
//! otwiera się zawsze. [`PanelDesc::min_tier`] decyduje **wyłącznie** o tym, czy
//! panel jest przypięty nowemu graczowi — bo panel Finanse z pełną księgowością
//! otwarty w piątej minucie zabija metrykę onboardingu (§5.12 pkt 5).
//!
//! # Dok lewy, nie okna
//!
//! `docs/ui-design.md` §5: dok lewy należy do tego, co gracz **prowadzi**, prawy do
//! tego, co **kliknął**. Panele idą do lewego i nie mieszają się z kartą inspekcji;
//! „pokaż w inspekcji" znaczy [`PanelAction::Show`], a nie druga karta (`DF-6`).
//!
//! # Skąd 0 alokacji na klatce bez zmian
//!
//! Każdy panel liczy swój **model** raz i trzyma go w [`magnat_ui::Cached`] pod
//! stemplami źródeł, które sam deklaruje. Klatka, w której nie drgnęła ani godzina
//! gry, ani rynek, ani język, nie buduje niczego — rysowanie chodzi po gotowych
//! liczbach. To jest ten sam mechanizm, którym `M9b` zdał budżet klatki, a nie
//! drugi obok niego.

pub mod layout;

mod chronicle;
mod city;
mod dashboard;
mod finance;
mod market;
mod people;
mod plant;
mod shop;
mod supply;
mod widgets;

pub use layout::{Layout, Panels};
pub use widgets::Sev;

use magnat_core::Subject;
use magnat_ui::{Cached, Catalog, DataSource, Locale, Theme, Versions};
use serde::{Deserialize, Serialize};

use crate::career::{CareerTier, Holdings};
use crate::{PlayerCommand, Session};

/// Który panel. Warianty M10 istnieją od tej podfazy jako **zarezerwowane** i nie
/// ma ich w rejestrze — tak samo jak `OverlayField::BrandAwareness` istnieje bez
/// wpisu w `data/ui/overlays.ron` (decyzja §9 pkt 11 dokumentu fazy).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum PanelId {
    Dashboard,
    Plant,
    Shop,
    Supply,
    Market,
    People,
    Finance,
    City,
    Chronicle,
    /// Marka i media — M10.
    Brand,
    /// Badania i rozwój — M10.
    Rnd,
    /// Giełda — M10.
    Stock,
}

impl PanelId {
    /// Klucz tekstu i klucz układu: `ui.panel.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            PanelId::Dashboard => "dashboard",
            PanelId::Plant => "plant",
            PanelId::Shop => "shop",
            PanelId::Supply => "supply",
            PanelId::Market => "market",
            PanelId::People => "people",
            PanelId::Finance => "finance",
            PanelId::City => "city",
            PanelId::Chronicle => "chronicle",
            PanelId::Brand => "brand",
            PanelId::Rnd => "rnd",
            PanelId::Stock => "stock",
        }
    }

    /// Czy panel jest zarezerwowany dla fazy, która go jeszcze nie napisała.
    #[must_use]
    pub const fn is_reserved(self) -> bool {
        matches!(self, PanelId::Brand | PanelId::Rnd | PanelId::Stock)
    }

    /// Czy z panelu da się wydać komendę.
    ///
    /// **To jest lista z `DH-3`, wypisana wprost.** Kryterium WP10 brzmi „z każdego
    /// panelu **operacyjnego** da się wydać co najmniej jedną `PlayerCommand`", bo
    /// kronika jest dziennikiem i komendy mieć nie może — a kryterium udające, że
    /// ją obejmuje, byłoby spełnione tożsamościowo przez pierwszy przycisk bez
    /// skutku. Panel bez komendy jest panelem tylko do oglądania i mówi to o sobie.
    #[must_use]
    pub const fn is_operational(self) -> bool {
        !matches!(self, PanelId::Chronicle) && !self.is_reserved()
    }
}

/// Co panel każe zrobić pętli gry. Panel niczego nie wykonuje sam — komendę składa
/// i oddaje, a wykonuje ją sesja w punkcie synchronizacji (§5.2).
#[derive(Clone, PartialEq, Debug, Default)]
pub enum PanelAction {
    #[default]
    None,
    /// Wydaj komendę gracza.
    Command(Box<PlayerCommand>),
    /// Pokaż podmiot w karcie inspekcji (dok prawy).
    Show(Subject),
}

impl PanelAction {
    #[must_use]
    pub fn cmd(c: PlayerCommand) -> PanelAction {
        PanelAction::Command(Box::new(c))
    }
}

/// Opis panelu. Dana, nie `impl Trait`: M12 ma móc zarejestrować panel z moda,
/// a mod nie dopisuje wariantu enuma.
pub struct PanelDesc {
    pub id: PanelId,
    /// Klucz tytułu w `data/locale/`. Literał tekstowy byłby tu błędem (CLAUDE.md).
    pub title: &'static str,
    /// Źródła, których zmiana unieważnia model panelu. Sufit to `Cached::MAX_DEPS`.
    pub deps: &'static [DataSource],
    pub build: fn(&PanelCtx<'_>) -> PanelModel,
    pub render: fn(&mut egui::Ui, &PanelCtx<'_>, &PanelModel, &mut PanelView) -> PanelAction,
    /// Etap, od którego panel jest **domyślnie przypięty**. `None` = zawsze.
    /// Nie blokuje otwarcia — patrz nagłówek modułu.
    pub min_tier: Option<CareerTier>,
}

/// Rejestr paneli.
pub struct PanelRegistry {
    panels: Vec<PanelDesc>,
}

impl Default for PanelRegistry {
    fn default() -> PanelRegistry {
        PanelRegistry {
            panels: vec![
                dashboard::DESC,
                plant::DESC,
                shop::DESC,
                supply::DESC,
                market::DESC,
                people::DESC,
                finance::DESC,
                city::DESC,
                chronicle::DESC,
            ],
        }
    }
}

impl PanelRegistry {
    /// Dokłada panel. Wejście dla M10 i M12 — istniejącego nie podmienia, bo dwa
    /// panele o jednym identyfikatorze rozjechałyby się przy pierwszym układzie.
    pub fn register(&mut self, d: PanelDesc) -> bool {
        if self.panels.iter().any(|p| p.id == d.id) {
            return false;
        }
        self.panels.push(d);
        true
    }

    #[must_use]
    pub fn get(&self, id: PanelId) -> Option<&PanelDesc> {
        self.panels.iter().find(|p| p.id == id)
    }

    #[must_use]
    pub fn ids(&self) -> Vec<PanelId> {
        self.panels.iter().map(|p| p.id).collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.panels.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }
}

/// Wszystko, czego panel potrzebuje ze świata i z ustawień.
///
/// `&Session`, a nie `&mut` — panel czyta i składa komendę, nigdy nie zmienia
/// świata sam. To jest granica z §5.2 w postaci, w jakiej daje się wymusić typem.
#[derive(Clone, Copy)]
pub struct PanelCtx<'a> {
    pub theme: &'a Theme,
    pub c: &'a Catalog,
    pub l: Locale,
    pub session: &'a Session,
    /// Stan posiadania gracza — liczy się raz na klatkę dla wszystkich paneli,
    /// bo pyta o niego pulpit, sklep, ludzie i finanse.
    pub holdings: &'a Holdings,
    /// Co gracz wybrał w tym panelu: zakład, towar, dzielnica — znaczenie należy
    /// do panelu. Model zależy od tego tak samo jak od zegara, więc wybór **też**
    /// unieważnia model: inaczej gracz przełączałby sklep i widział poprzedni
    /// aż do najbliższej pełnej godziny.
    pub sel: usize,
}

impl PanelCtx<'_> {
    #[must_use]
    pub fn text(&self, key: &str) -> String {
        self.c.fmt_key(self.l, key, &[])
    }

    #[must_use]
    pub fn fmt(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.c.fmt_key(self.l, key, args)
    }

    /// Tekst klucza, którego **może nie być w katalogu**.
    ///
    /// `Catalog::fmt_key` panikuje na nieznanym kluczu i słusznie: literówka w kluczu
    /// stałym ma łamać build, a nie po cichu znikać. Ale numer komunikatu polityki
    /// pochodzi z **drzewa reguł gracza** i nie jest ograniczony walidatorem, więc
    /// dla niego panika w ścieżce rysowania znaczyłaby, że gracz wywala sobie grę
    /// własną regułą. `fallback` jest odpowiedzią na „nie wiem, co to za komunikat".
    #[must_use]
    pub fn fmt_or(&self, key: &str, fallback: &str, args: &[(&str, &str)]) -> String {
        if self.c.key(key).is_some() {
            self.c.fmt_key(self.l, key, args)
        } else {
            self.c.fmt_key(self.l, fallback, args)
        }
    }

    #[must_use]
    pub fn money(&self, m: magnat_core::Money) -> String {
        magnat_ui::fmt::money(self.c, self.l, m)
    }

    #[must_use]
    pub fn int(&self, v: i64) -> String {
        magnat_ui::fmt::integer(self.l, v)
    }
}

/// Model panelu — wynik jednego przeliczenia, trzymany do czasu zmiany danych.
pub enum PanelModel {
    Dashboard(dashboard::Model),
    Plant(plant::Model),
    Shop(shop::Model),
    Supply(supply::Model),
    Market(market::Model),
    People(people::Model),
    Finance(finance::Model),
    City(city::Model),
    Chronicle(chronicle::Model),
}

/// Stan widoku panelu między klatkami: co gracz wybrał i jak posortował.
///
/// Osobno od modelu, bo zmienia go **gracz**, a nie świat: przewinięcie tabeli nie
/// ma unieważniać przeliczonych liczb, a zmiana ceny w mieście nie ma przestawiać
/// zakładki, na której gracz właśnie stoi.
#[derive(Default)]
pub struct PanelView {
    pub tab: usize,
    /// Zaznaczony wiersz albo zakład — znaczenie należy do panelu.
    pub sel: usize,
    /// Zakres wykresu w dobach. Przestawia go pulpit; pozostałe panele go nie mają,
    /// bo nie mają wykresu.
    pub days: u32,
}

/// Stan jednego panelu: model pod stemplami i widok gracza.
pub struct PanelState {
    model: Cached<PanelModel>,
    view: PanelView,
    /// Wybór, przy którym zbudowano bieżący model.
    built_for: usize,
}

impl PanelState {
    #[must_use]
    fn new(d: &PanelDesc) -> PanelState {
        PanelState {
            model: Cached::new(d.deps),
            view: PanelView {
                days: 30,
                ..PanelView::default()
            },
            built_for: usize::MAX,
        }
    }

    /// Model i widok naraz. Destrukturyzacja, a nie dwa wywołania — inaczej pożyczka
    /// modelu trzymałaby cały stan i panel nie mógłby ruszyć własnej tabeli.
    fn parts(
        &mut self,
        v: &Versions,
        build: impl FnOnce() -> PanelModel,
    ) -> (&PanelModel, &mut PanelView) {
        let PanelState {
            model,
            view,
            built_for,
        } = self;
        if *built_for != view.sel {
            *built_for = view.sel;
            model.clear();
        }
        (model.get(v, build), view)
    }

    #[must_use]
    pub fn rebuilds(&self) -> u32 {
        self.model.rebuilds()
    }
}
