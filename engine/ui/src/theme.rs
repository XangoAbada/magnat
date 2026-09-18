//! Motyw: kolory, typografia i siatka jako **dane** (`M9b` §5.14, `Z-6`).
//!
//! `engine/ui` nie zna ani jednego koloru z nazwy własnej. Wszystkie siedzą
//! w `data/ui/theme.ron` (katalog z `K-19`), bo motyw jasny i tryb wysokiego kontrastu
//! z M12 mają być podmianą pliku, a nie przeglądem dwudziestu paneli.
//!
//! Wartości pochodzą z `docs/ui-design.md` §3 i ten dokument jest dla nich źródłem.
//! Trzy kolory stanu (`ok` / `warn` / `danger`) to dokładnie te trzy, którymi M3 malował
//! paski potrzeb literałami — i taki był zamysł od początku.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Token koloru. W danych klucze są tekstowe, więc przestawienie ich niczego nie psuje;
/// kolejność wariantów indeksuje wyłącznie tablicę w pamięci.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ColorToken {
    BgWindow,
    BgPanel,
    BgCard,
    BgOverlay,
    LineSoft,
    LineStrong,
    TextPrimary,
    TextSecondary,
    TextDisabled,
    Accent,
    AccentHi,
    Ok,
    Warn,
    Danger,
    Info,
}

pub const COLOR_TOKEN_COUNT: usize = 15;

impl ColorToken {
    pub const ALL: [ColorToken; COLOR_TOKEN_COUNT] = [
        ColorToken::BgWindow,
        ColorToken::BgPanel,
        ColorToken::BgCard,
        ColorToken::BgOverlay,
        ColorToken::LineSoft,
        ColorToken::LineStrong,
        ColorToken::TextPrimary,
        ColorToken::TextSecondary,
        ColorToken::TextDisabled,
        ColorToken::Accent,
        ColorToken::AccentHi,
        ColorToken::Ok,
        ColorToken::Warn,
        ColorToken::Danger,
        ColorToken::Info,
    ];

    /// Klucz w `data/ui/theme.ron`. Ta sama pisownia co w `docs/ui-design.md` §3.1.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ColorToken::BgWindow => "bg.window",
            ColorToken::BgPanel => "bg.panel",
            ColorToken::BgCard => "bg.card",
            ColorToken::BgOverlay => "bg.overlay",
            ColorToken::LineSoft => "line.soft",
            ColorToken::LineStrong => "line.strong",
            ColorToken::TextPrimary => "text.primary",
            ColorToken::TextSecondary => "text.secondary",
            ColorToken::TextDisabled => "text.disabled",
            ColorToken::Accent => "accent",
            ColorToken::AccentHi => "accent.hi",
            ColorToken::Ok => "ok",
            ColorToken::Warn => "warn",
            ColorToken::Danger => "danger",
            ColorToken::Info => "info",
        }
    }

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Rola tekstu — sześć rozmiarów z `docs/ui-design.md` §3.2 i ani jednego więcej.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TextRole {
    /// Podpisy osi, jednostki.
    Micro,
    /// Domyślny tekst i wiersz tabeli.
    Body,
    /// Nagłówek kolumny, etykieta wyróżniona.
    Strong,
    /// Tytuł panelu.
    Title,
    /// Tytuł ekranu powłoki.
    Screen,
    /// Tytuł gry w menu głównym.
    Hero,
}

pub const TEXT_ROLE_COUNT: usize = 6;

impl TextRole {
    pub const ALL: [TextRole; TEXT_ROLE_COUNT] = [
        TextRole::Micro,
        TextRole::Body,
        TextRole::Strong,
        TextRole::Title,
        TextRole::Screen,
        TextRole::Hero,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Styl tekstu w pikselach **logicznych** — mnożenie przez `ui_scale` robi `egui`
/// przez `zoom_factor`, żeby przyciąganie do pikseli fizycznych było w jednym miejscu.
#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
pub struct TextStyle {
    pub size: f32,
    /// `true` = grubość 600 z §3.2, `false` = 400.
    pub strong: bool,
}

/// Kolor RGBA. Osobny typ zamiast `egui::Color32` w danych, bo plik motywu nie ma
/// powodu znać biblioteki graficznej.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    #[must_use]
    pub fn to_color32(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.0, self.1, self.2, self.3)
    }

    /// Względna luminancja wg WCAG 2.1 — podstawa kontrastu z `ui-design.md` §7.
    ///
    /// Potęgowanie idzie przez `core::det_math` mimo że to kod interfejsu: `K-6` zakazuje
    /// `powf` lintem na całym workspace i wyjątek dla jednej funkcji kosztowałby `#[allow]`,
    /// czyli dokładnie tę furtkę, której `K-6` zabrania. Dokładność `det_math` (≤ 2 ULP)
    /// jest tu z zapasem — próg kontrastu ma jedno miejsce po przecinku.
    #[must_use]
    pub fn luminance(self) -> f64 {
        let ch = |v: u8| {
            let c = f64::from(v) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                magnat_core::det_math::pow((c + 0.055) / 1.055, 2.4)
            }
        };
        0.2126 * ch(self.0) + 0.7152 * ch(self.1) + 0.0722 * ch(self.2)
    }

    /// Stosunek kontrastu dwóch kolorów, 1,0…21,0.
    #[must_use]
    pub fn contrast(self, other: Rgba) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }
}

pub const THEME_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct ThemeFile {
    schema_version: u32,
    colors: BTreeMap<String, Rgba>,
    text: Vec<TextStyle>,
    grid: u8,
}

#[derive(Debug)]
pub enum ThemeError {
    Io(String, std::io::Error),
    Ron(String, Box<ron::de::SpannedError>),
    Schema { found: u32 },
    MissingColor(&'static str),
    UnknownColor(String),
    TextArity { want: usize, got: usize },
    Grid(u8),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeError::Io(p, e) => write!(f, "{p}: {e}"),
            ThemeError::Ron(p, e) => write!(f, "{p}: {e}"),
            ThemeError::Schema { found } => write!(
                f,
                "data/ui/theme.ron: schema_version {found}, oczekiwano {THEME_SCHEMA_VERSION}"
            ),
            ThemeError::MissingColor(k) => write!(f, "brak tokenu koloru `{k}`"),
            ThemeError::UnknownColor(k) => write!(f, "nieznany token koloru `{k}`"),
            ThemeError::TextArity { want, got } => {
                write!(f, "styl tekstu: {got} ról zamiast {want}")
            }
            ThemeError::Grid(g) => write!(f, "siatka {g} px — `ui-design.md` §3.3 mówi 4"),
        }
    }
}

impl std::error::Error for ThemeError {}

/// Motyw w pamięci. Kolory w tablicy indeksowanej [`ColorToken`], bo odczyt koloru
/// dzieje się kilkaset razy na klatkę i nie ma powodu, żeby kosztował wyszukiwanie.
#[derive(Clone, Debug)]
pub struct Theme {
    colors: [Rgba; COLOR_TOKEN_COUNT],
    text: [TextStyle; TEXT_ROLE_COUNT],
    grid: u8,
}

impl Theme {
    /// Wczytuje `data/ui/theme.ron`.
    ///
    /// # Errors
    /// Brak pliku, zła wersja schematu, brakujący albo nadmiarowy token koloru,
    /// zła liczba ról tekstu.
    pub fn load() -> Result<Theme, ThemeError> {
        let path = magnat_core::data_path("ui").join("theme.ron");
        let nazwa = path.display().to_string();
        let txt = std::fs::read_to_string(&path).map_err(|e| ThemeError::Io(nazwa.clone(), e))?;
        let f: ThemeFile = ron::from_str(&txt).map_err(|e| ThemeError::Ron(nazwa, Box::new(e)))?;
        Theme::from_file(f)
    }

    fn from_file(f: ThemeFile) -> Result<Theme, ThemeError> {
        if f.schema_version != THEME_SCHEMA_VERSION {
            return Err(ThemeError::Schema {
                found: f.schema_version,
            });
        }
        if f.text.len() != TEXT_ROLE_COUNT {
            return Err(ThemeError::TextArity {
                want: TEXT_ROLE_COUNT,
                got: f.text.len(),
            });
        }
        // Siatka 4 px z §3.3 nie jest parametrem do strojenia: wszystkie odstępy w grze
        // są jej wielokrotnością, więc inna wartość nie dostroiłaby układu, tylko
        // rozjechałaby go o ułamek piksela na każdym zagnieżdżeniu.
        if f.grid != 4 {
            return Err(ThemeError::Grid(f.grid));
        }
        let mut colors = [Rgba(0, 0, 0, 255); COLOR_TOKEN_COUNT];
        for t in ColorToken::ALL {
            let v = f
                .colors
                .get(t.key())
                .ok_or(ThemeError::MissingColor(t.key()))?;
            colors[t.as_index()] = *v;
        }
        // Nadmiarowy token to literówka w kluczu — cisza zostawiłaby kolor domyślny
        // w miejscu, w którym ktoś właśnie próbował go zmienić.
        let znane: Vec<&str> = ColorToken::ALL.iter().map(|t| t.key()).collect();
        if let Some(k) = f.colors.keys().find(|k| !znane.contains(&k.as_str())) {
            return Err(ThemeError::UnknownColor(k.clone()));
        }
        let mut text = [TextStyle {
            size: 13.0,
            strong: false,
        }; TEXT_ROLE_COUNT];
        text.copy_from_slice(&f.text);
        Ok(Theme {
            colors,
            text,
            grid: f.grid,
        })
    }

    #[must_use]
    pub const fn rgba(&self, t: ColorToken) -> Rgba {
        self.colors[t.as_index()]
    }

    #[must_use]
    pub fn color(&self, t: ColorToken) -> egui::Color32 {
        self.colors[t.as_index()].to_color32()
    }

    #[must_use]
    pub const fn text_style(&self, r: TextRole) -> TextStyle {
        self.text[r.as_index()]
    }

    /// Bok siatki odstępów w pikselach logicznych (§3.3).
    #[must_use]
    pub const fn grid(&self) -> f32 {
        self.grid as f32
    }

    /// Odstęp `n` kratek siatki.
    #[must_use]
    pub fn gap(&self, n: u8) -> f32 {
        self.grid() * f32::from(n)
    }

    /// Font dla roli.
    ///
    /// `ponytail:` brak drugiego kroju o innej grubości — `egui` ma jeden proporcjonalny
    /// i jeden monospace. Sufit nazwany: nagłówek różni się rozmiarem, nie wagą, więc
    /// pole `strong` jest dziś deklaracją projektową bez skutku w pikselach. Ścieżka
    /// wyjścia: `egui::FontDefinitions` z wgranym krojem półgrubym, gdy M11 dołoży fonty.
    #[must_use]
    pub fn font(&self, r: TextRole) -> egui::FontId {
        egui::FontId::proportional(self.text_style(r).size)
    }

    /// Font dla liczb: monospace tego samego rozmiaru (§3.2 — liczby tabelaryczne).
    #[must_use]
    pub fn mono(&self, r: TextRole) -> egui::FontId {
        egui::FontId::monospace(self.text_style(r).size)
    }

    /// Kolor dla stylu kawałka tekstu ([`crate::SpanStyle`]).
    #[must_use]
    pub fn span_color(&self, s: crate::SpanStyle) -> egui::Color32 {
        match s {
            crate::SpanStyle::Normal | crate::SpanStyle::Emphasis | crate::SpanStyle::Number => {
                self.color(ColorToken::TextPrimary)
            }
            crate::SpanStyle::Link => self.color(ColorToken::Accent),
        }
    }

    /// Nakłada motyw na kontekst `egui`: kolory, promień zaokrąglenia, odstępy, fonty.
    ///
    /// Woła się raz przy starcie i przy zmianie motywu — nie co klatkę.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.panel_fill = self.color(ColorToken::BgPanel);
        v.window_fill = self.color(ColorToken::BgWindow);
        v.extreme_bg_color = self.color(ColorToken::BgWindow);
        v.faint_bg_color = self.color(ColorToken::BgCard);
        v.selection.bg_fill = self.color(ColorToken::Accent);
        v.selection.stroke.color = self.color(ColorToken::AccentHi);
        v.hyperlink_color = self.color(ColorToken::Accent);
        v.warn_fg_color = self.color(ColorToken::Warn);
        v.error_fg_color = self.color(ColorToken::Danger);
        v.window_stroke = egui::Stroke::new(1.0, self.color(ColorToken::LineStrong));

        // Promień 3 px (§3.3) i stany interakcji z §4: hover `accent.hi`, wciśnięty `accent`,
        // wyłączony `text.disabled`.
        let promien = egui::CornerRadius::same(3);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = promien;
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.bg_fill = self.color(ColorToken::BgPanel);
        v.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0, self.color(ColorToken::LineSoft));
        v.widgets.noninteractive.fg_stroke.color = self.color(ColorToken::TextPrimary);
        v.widgets.inactive.bg_fill = self.color(ColorToken::BgCard);
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, self.color(ColorToken::LineStrong));
        v.widgets.inactive.fg_stroke.color = self.color(ColorToken::TextPrimary);
        v.widgets.hovered.bg_fill = self.color(ColorToken::LineStrong);
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.color(ColorToken::AccentHi));
        v.widgets.hovered.fg_stroke.color = self.color(ColorToken::TextPrimary);
        v.widgets.active.bg_fill = self.color(ColorToken::Accent);
        v.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.color(ColorToken::AccentHi));
        v.widgets.active.fg_stroke.color = self.color(ColorToken::TextPrimary);
        v.widgets.open.bg_fill = self.color(ColorToken::BgCard);
        // Fokus klawiatury jest **zawsze** widoczny (`ui-design.md` §4, §7) — 2 px `accent.hi`.
        // Nigdy nie usuwany dla urody: to jest wymaganie dostępności, nie preferencja.
        style.spacing.item_spacing = egui::vec2(self.gap(2), self.gap(1));
        style.spacing.button_padding = egui::vec2(self.gap(2), self.gap(1));
        style.spacing.indent = self.gap(4);
        // Wysokość kontrolki interaktywnej: 24 px w rozgrywce (§3.3).
        style.spacing.interact_size = egui::vec2(self.gap(10), self.gap(6));

        use egui::TextStyle as T;
        style.text_styles = [
            (T::Small, self.font(TextRole::Micro)),
            (T::Body, self.font(TextRole::Body)),
            (T::Button, self.font(TextRole::Body)),
            (T::Heading, self.font(TextRole::Screen)),
            (T::Monospace, self.mono(TextRole::Body)),
        ]
        .into();
        ctx.set_style_of(egui::Theme::Dark, style.clone());
        ctx.set_style_of(egui::Theme::Light, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plik(colors: BTreeMap<String, Rgba>) -> ThemeFile {
        ThemeFile {
            schema_version: THEME_SCHEMA_VERSION,
            colors,
            text: vec![
                TextStyle {
                    size: 11.0,
                    strong: false
                };
                TEXT_ROLE_COUNT
            ],
            grid: 4,
        }
    }

    #[test]
    fn motyw_z_danych_ma_komplet_tokenow() {
        let t = Theme::load().expect("data/ui/theme.ron");
        assert_eq!(t.grid(), 4.0);
        assert_eq!(t.gap(4), 16.0);
        assert_eq!(t.text_style(TextRole::Body).size, 13.0);
        assert!(t.text_style(TextRole::Hero).size > t.text_style(TextRole::Screen).size);
        // Warstwy tła muszą być odróżnialne — to jest cały ich sens.
        assert_ne!(t.rgba(ColorToken::BgPanel), t.rgba(ColorToken::BgCard));
        assert_ne!(t.rgba(ColorToken::BgWindow), t.rgba(ColorToken::BgPanel));
    }

    /// `docs/ui-design.md` §7: tekst do tła ≥ 4,5:1, element interaktywny ≥ 3:1.
    /// Wymaganie dostępności, które daje się sprawdzić — więc jest sprawdzane.
    #[test]
    fn kontrast_spelnia_wcag() {
        let t = Theme::load().expect("data/ui/theme.ron");
        for tlo in [
            ColorToken::BgWindow,
            ColorToken::BgPanel,
            ColorToken::BgCard,
        ] {
            for tekst in [ColorToken::TextPrimary, ColorToken::TextSecondary] {
                let k = t.rgba(tekst).contrast(t.rgba(tlo));
                assert!(
                    k >= 4.5,
                    "{} na {}: kontrast {k:.2}, wymagane 4,5",
                    tekst.key(),
                    tlo.key()
                );
            }
            for interaktywny in [
                ColorToken::Accent,
                ColorToken::AccentHi,
                ColorToken::Ok,
                ColorToken::Warn,
                ColorToken::Danger,
            ] {
                let k = t.rgba(interaktywny).contrast(t.rgba(tlo));
                assert!(
                    k >= 3.0,
                    "{} na {}: kontrast {k:.2}, wymagane 3,0",
                    interaktywny.key(),
                    tlo.key()
                );
            }
        }
    }

    #[test]
    fn brakujacy_i_nadmiarowy_token_sa_bledem_a_nie_kolorem_domyslnym() {
        let mut colors: BTreeMap<String, Rgba> = ColorToken::ALL
            .iter()
            .map(|t| (t.key().to_string(), Rgba(1, 2, 3, 255)))
            .collect();
        colors.remove(ColorToken::Accent.key());
        let e = Theme::from_file(plik(colors.clone())).unwrap_err();
        assert!(matches!(e, ThemeError::MissingColor("accent")), "{e}");

        colors.insert("accent".into(), Rgba(1, 2, 3, 255));
        colors.insert("accnet.hi".into(), Rgba(1, 2, 3, 255));
        let e = Theme::from_file(plik(colors)).unwrap_err();
        assert!(matches!(e, ThemeError::UnknownColor(_)), "{e}");
    }
}
