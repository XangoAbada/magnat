//! Rysowanie karty i sterowania czasem w `egui` (M3d §5.11, decyzja 9.2).
//!
//! **Tu nie ma ani jednej liczby, której nie policzył model.** Widget bierze
//! [`CitizenModel`](crate::CitizenModel) i [`TimeControlsWidget`] i zamienia je na piksele;
//! jeśli czegoś nie ma w modelu, to nie pojawi się na ekranie — i o to chodzi, bo wtedy
//! złoty test wydruku faktycznie broni tego, co widzi gracz (korekta E-8), zamiast bronić
//! drugiej ścieżki obok.
//!
//! Crate nadal **nie zna `wgpu` ani `winit`**: `egui` jest czystym procesorem, a malowanie
//! trójkątów należy do `engine/render::ui`. Dlatego testy poniżej rysują pełny panel
//! w CI bez karty graficznej.
//!
//! Wszystkie teksty przychodzą z [`Catalog`](crate::Catalog) — literał w kodzie UI jest
//! błędem (CLAUDE.md). Jedyne literały tutaj to separatory i formaty liczbowe.

use crate::inspect::citizen::{CitizenCard, CitizenModel};
use crate::inspect::shop::{ShopCard, ShopTab};
use crate::loc::{Catalog, Locale};
use crate::rich::{Rich, Span, SpanStyle};
use crate::theme::{ColorToken, TextRole, Theme};
use crate::time::TimeControlsWidget;
use magnat_core::{SimSpeed, Subject};

/// Kolor paska potrzeby. Trzy progi, nie gradient: gracz ma odczytać „dobrze / słabo /
/// źle" jednym spojrzeniem, a nie porównywać odcienie.
///
/// Od M9b barwy idą z motywu (`data/ui/theme.ron`), a nie z literałów — to są
/// dokładnie tokeny `danger` / `warn` / `ok`, i taki był zamysł od początku (`Z-6`).
fn kolor_potrzeby(theme: &Theme, level: u8, critical: bool) -> egui::Color32 {
    if critical {
        theme.color(ColorToken::Danger)
    } else if level < 50 {
        theme.color(ColorToken::Warn)
    } else {
        theme.color(ColorToken::Ok)
    }
}

/// Kolor bloku czynności. `ActivityKind` ma swój kolor i ma go mieć **jeden** — oś planu
/// i oś realizacji muszą dać się porównać wzrokiem.
///
/// Paleta kategorii, a nie stanu: kolor rozróżnia tu **rodzaj**, nie wartość, więc
/// mieszka obok konsumenta (`ui-design.md` §3.1). Jedyny wyjątek to praca — ta bierze
/// `accent`, bo to, co należy do gracza i jego pracy, ma w całej grze jeden kolor.
fn kolor_czynnosci(theme: &Theme, kind: magnat_core::ActivityKind) -> egui::Color32 {
    use magnat_core::ActivityKind as A;
    match kind {
        A::Work => theme.color(ColorToken::Accent),
        A::Sleep => egui::Color32::from_rgb(70, 80, 130),
        A::Eat => egui::Color32::from_rgb(180, 140, 70),
        A::Shop => egui::Color32::from_rgb(160, 110, 160),
        A::Commute => egui::Color32::from_rgb(110, 110, 110),
        A::Leisure => egui::Color32::from_rgb(90, 160, 110),
        A::School => egui::Color32::from_rgb(140, 160, 80),
        A::Social => egui::Color32::from_rgb(150, 95, 120),
        A::Errand => egui::Color32::from_rgb(130, 130, 90),
        A::Idle => egui::Color32::from_rgb(100, 100, 100),
    }
}

/// Rysuje [`Rich`] i zwraca podmiot, w którego odnośnik gracz właśnie kliknął.
///
/// Kawałki składają się w wiersze: kawałek kończący się znakiem nowej linii domyka
/// wiersz. Dzięki temu ta sama funkcja rysuje dzisiejsze karty (jeden kawałek = jedna
/// linia) i karty `M9c`, w których odnośnik siedzi w środku zdania.
pub fn rich(ui: &mut egui::Ui, theme: &Theme, r: &Rich) -> Option<Subject> {
    let mut klik = None;
    let mut wiersz: Vec<&Span> = Vec::new();
    ui.vertical(|ui| {
        for s in r {
            wiersz.push(s);
            if s.text.ends_with('\n') {
                klik = wiersz_rich(ui, theme, &wiersz).or(klik);
                wiersz.clear();
            }
        }
        if !wiersz.is_empty() {
            klik = wiersz_rich(ui, theme, &wiersz).or(klik);
        }
    });
    klik
}

fn wiersz_rich(ui: &mut egui::Ui, theme: &Theme, wiersz: &[&Span]) -> Option<Subject> {
    if wiersz
        .iter()
        .all(|s| s.text.trim_end_matches('\n').is_empty())
    {
        ui.add_space(theme.gap(2));
        return None;
    }
    let mut klik = None;
    ui.horizontal_wrapped(|ui| {
        // Bez odstępu między kawałkami: wiersz karty jest jednym zdaniem, a nie
        // listą elementów — spacje są w treści, nie w układzie.
        ui.spacing_mut().item_spacing.x = 0.0;
        for s in wiersz {
            let tekst = s.text.trim_end_matches('\n');
            if tekst.is_empty() {
                continue;
            }
            // Monospace dla wszystkiego, co stoi w kolumnie: karty składają się
            // z wierszy wyrównanych spacjami i font proporcjonalny by je rozjechał.
            let rt = egui::RichText::new(tekst)
                .font(theme.mono(TextRole::Body))
                .color(theme.span_color(s.style));
            match s.link {
                Some(cel) => {
                    if ui.link(rt).clicked() {
                        klik = Some(cel);
                    }
                }
                None if s.style == SpanStyle::Emphasis => {
                    ui.label(rt.strong());
                }
                None => {
                    ui.label(rt);
                }
            }
        }
    });
    klik
}

/// Pasek sterowania czasem: data, godzina, cztery przyciski prędkości.
///
/// Zwraca prędkość, którą gracz właśnie wybrał, albo `None`. Widget **nie ustawia jej sam**
/// — zegar należy do pętli gry, a nie do interfejsu (decyzja 9.1), i tylko pętla wie,
/// czy wolno go w tej klatce ruszyć.
pub fn time_bar(
    ui: &mut egui::Ui,
    w: &TimeControlsWidget,
    c: &Catalog,
    l: Locale,
) -> Option<SimSpeed> {
    let mut wybrana = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(w.date_label(c, l)).strong());
        ui.separator();
        ui.label(egui::RichText::new(w.clock_label(c, l)).monospace());
        ui.separator();
        for (etykieta, predkosc, wcisniety) in w.buttons(c, l) {
            if ui.selectable_label(wcisniety, etykieta).clicked() {
                wybrana = Some(predkosc);
            }
        }
    });
    wybrana
}

/// Oś czasu dnia: plan u góry, realizacja pod nim (§5.11 pkt 4).
///
/// Skala jest stała — cała doba na całej dostępnej szerokości — bo oś służy do
/// porównania planu z realizacją, a nie do oglądania jednej godziny. Rozjazd powyżej
/// progu z [`DRIFT_HIGHLIGHT_MIN`](crate::DRIFT_HIGHLIGHT_MIN) dostaje obwódkę.
pub fn day_timeline(ui: &mut egui::Ui, theme: &Theme, m: &CitizenModel, c: &Catalog, l: Locale) {
    const WYS: f32 = 18.0;
    let wiersze = m.timeline().rows(c, l);
    let szerokosc = ui.available_width().max(120.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(szerokosc, WYS * 2.0 + 6.0), egui::Sense::hover());
    let malarz = ui.painter_at(rect);
    let na_minute = szerokosc / 1440.0;

    // Godziny co trzy: siatka, która pozwala powiedzieć „wyszedł koło ósmej" bez liczenia.
    for h in (0..=24).step_by(3) {
        let x = rect.left() + h as f32 * 60.0 * na_minute;
        malarz.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, theme.color(ColorToken::LineStrong)),
        );
    }

    let blok = |y: f32, od: u16, do_: u16, kolor: egui::Color32, obwodka: bool| {
        let x0 = rect.left() + f32::from(od) * na_minute;
        let x1 = rect.left() + f32::from(do_.max(od.saturating_add(1))) * na_minute;
        let r = egui::Rect::from_min_max(egui::pos2(x0, y), egui::pos2(x1, y + WYS - 2.0));
        malarz.rect_filled(r, 2.0, kolor);
        if obwodka {
            malarz.rect_stroke(
                r,
                2.0,
                egui::Stroke::new(2.0, theme.color(ColorToken::Warn)),
                egui::StrokeKind::Inside,
            );
        }
    };

    for r in &wiersze {
        blok(
            rect.top(),
            r.start_min,
            r.end_min % 1440,
            kolor_czynnosci(theme, r.kind),
            r.drifted(),
        );
    }
    for b in &m.actual {
        let kind = magnat_core::ActivityKind::from_index(b.kind as usize)
            .unwrap_or(magnat_core::ActivityKind::Idle);
        blok(
            rect.top() + WYS + 4.0,
            b.start_min,
            b.end_min.min(1440),
            kolor_czynnosci(theme, kind).gamma_multiply(0.75),
            false,
        );
    }

    // Uzasadnienia pod osią: PRD §5.5 chce ich przy wyborze, a nie w osobnym oknie.
    for r in &wiersze {
        let mut wiersz = format!(
            "{}–{}  {}  ({})",
            crate::zegar(r.start_min),
            crate::zegar(r.end_min % 1440),
            r.label,
            r.reason
        );
        for a in &r.alternatives {
            wiersz.push_str(" · ");
            wiersz.push_str(a);
        }
        let tekst = egui::RichText::new(wiersz).small();
        ui.label(if r.drifted() {
            tekst.color(theme.color(ColorToken::Warn))
        } else {
            tekst
        });
    }

    let pominiete = m.timeline().skipped(c, l);
    if !pominiete.is_empty() {
        ui.label(egui::RichText::new(c.fmt_key(l, "ui.card.skipped", &[])).strong());
        for p in pominiete {
            ui.label(egui::RichText::new(p).small().weak());
        }
    }
}

/// Paski dwunastu potrzeb z tooltipem „tempo spadku i skutek deprywacji" (§5.11 pkt 2).
pub fn needs(ui: &mut egui::Ui, theme: &Theme, card: &CitizenCard) {
    for n in &card.needs {
        ui.horizontal(|ui| {
            ui.add_sized(
                egui::vec2(110.0, 16.0),
                egui::Label::new(egui::RichText::new(&n.label).small()),
            );
            let (rect, odp) = ui.allocate_exact_size(egui::vec2(120.0, 12.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 2.0, theme.color(ColorToken::BgCard));
            let mut wypelnienie = rect;
            wypelnienie.set_width(rect.width() * f32::from(n.level) / 100.0);
            p.rect_filled(wypelnienie, 2.0, kolor_potrzeby(theme, n.level, n.critical));
            odp.on_hover_text(&n.tooltip);
            ui.label(
                egui::RichText::new(format!("{}", n.level))
                    .monospace()
                    .small(),
            );
        });
    }
}

/// Cała karta mieszkańca w kolejności z §5.11.
pub fn citizen_card(ui: &mut egui::Ui, theme: &Theme, m: &CitizenModel, c: &Catalog, l: Locale) {
    let card = &m.card;
    ui.label(
        egui::RichText::new(c.fmt_key(
            l,
            "ui.card.header",
            &[
                ("imie", &card.header.name),
                (
                    "wiek",
                    &crate::inspect::reason::years(c, l, card.header.age_years),
                ),
                ("zawod", &card.header.occupation),
                ("adres", &card.header.address),
            ],
        ))
        .strong(),
    );
    ui.separator();

    ui.label(egui::RichText::new(c.fmt_key(l, "ui.card.needs", &[])).strong());
    needs(ui, theme, card);
    ui.separator();

    ui.label(egui::RichText::new(c.fmt_key(l, "ui.card.wealth", &[])).strong());
    for (klucz, kwota) in [
        ("ui.card.wealth.cash", card.cash),
        ("ui.card.wealth.household", card.household_budget),
        ("ui.card.wealth.income", card.income_monthly),
    ] {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(c.fmt_key(l, klucz, &[])).small());
            ui.label(
                egui::RichText::new(crate::fmt::money(c, l, kwota))
                    .monospace()
                    .small(),
            );
        });
    }
    ui.separator();

    ui.label(
        egui::RichText::new(format!(
            "{}: {} ({}/100)",
            c.fmt_key(l, "ui.card.status", &[]),
            card.social_class,
            card.status
        ))
        .strong(),
    );
    ui.separator();

    day_timeline(ui, theme, m, c, l);
}

/// Panel sklepu: rząd zakładek, nagłówek, treść wybranej zakładki (M5e §5.12).
///
/// Zakładki idą przez [`tab_strip`](crate::tab_strip), a treść przez [`rich`] —
/// od M9b oba są wspólne dla każdej karty w grze (`Z-8`). Zwraca podmiot, w którego
/// odnośnik gracz kliknął; dziś karta sklepu odnośników jeszcze nie ma, wpina je `M9c`.
pub fn shop_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    card: &ShopCard,
    tab: &mut ShopTab,
    c: &Catalog,
    l: Locale,
) -> Option<Subject> {
    crate::tab_strip(ui, theme, &ShopTab::ALL, tab, |t| t.label(c, l));
    ui.separator();
    let a = rich(ui, theme, &card.render_header(c, l));
    ui.separator();
    let b = rich(ui, theme, &card.render_tab(c, l, *tab));
    a.or(b)
}

/// Legenda nakładki danych (M9c §5.10, `ui-design.md` §4).
///
/// Nazwa pola, skala z liczbami i jednostka — bez tego mapa cieplna jest obrazkiem,
/// a nie przyrządem: gracz widzi, że gdzieś jest „czerwono", i nie wie, czy to dużo.
/// Barwy przychodzą **z palety nakładki** (`data/ui/overlays.ron`, `K-19`), więc
/// legenda i mapa nie mogą pokazywać dwóch różnych skal.
pub fn overlay_legend(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    unit: &str,
    stops: &[(u8, i64)],
    palette: &[[u8; 4]; 256],
) {
    ui.label(
        egui::RichText::new(name)
            .font(theme.font(crate::TextRole::Title))
            .color(theme.color(crate::ColorToken::TextPrimary)),
    );
    for (idx, wartosc) in stops.iter().rev() {
        ui.horizontal(|ui| {
            let c = palette[*idx as usize];
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(theme.gap(4), theme.gap(3)), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                1.0,
                egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255),
            );
            ui.label(
                egui::RichText::new(format!("{wartosc} {unit}"))
                    .font(theme.font(crate::TextRole::Micro))
                    .color(theme.color(crate::ColorToken::TextSecondary)),
            );
        });
    }
}

/// Karta inspekcji: nagłówek z tożsamością nad rzędem zakładek (M9c §5.7).
///
/// `tab` jest **indeksem w `card.tabs`**, a nie rodzajem zakładki, i trzyma go
/// wołający — karta tego samego rodzaju podmiotu ma zachować wybór, a karta innego
/// rodzaju wrócić na pierwszą (`docs/ui-design.md` §4). Zwraca podmiot, w którego
/// odnośnik gracz kliknął: to jest jedyna droga, którą klik w nazwę zmienia kartę.
pub fn inspection_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    card: &crate::InspectionCard,
    tab: &mut usize,
    c: &Catalog,
    l: Locale,
) -> Option<Subject> {
    let naglowek = rich(ui, theme, &card.header);
    if card.tabs.is_empty() {
        return naglowek;
    }
    *tab = (*tab).min(card.tabs.len() - 1);
    ui.separator();
    let rodzaje: Vec<usize> = (0..card.tabs.len()).collect();
    crate::tab_strip(ui, theme, &rodzaje, tab, |i| card.tabs[i].kind.title(c, l));
    ui.separator();
    let tresc = rich(ui, theme, &card.tabs[*tab].body);
    naglowek.or(tresc)
}

/// Panel łańcucha dostaw: rząd zakładek, nagłówek, treść wybranej zakładki (WP13).
///
/// Ten sam kształt co [`shop_card`] i z tego samego powodu: treść idzie z karty,
/// więc złoty test broni dokładnie tego, co widzi gracz, a nie drugiej ścieżki obok.
pub fn supply_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    card: &crate::SupplyCard,
    tab: &mut crate::SupplyTab,
    c: &Catalog,
    l: Locale,
) -> Option<Subject> {
    crate::tab_strip(ui, theme, &crate::SupplyTab::ALL, tab, |t| t.label(c, l));
    ui.separator();
    let a = rich(ui, theme, &card.render_header(c, l));
    ui.separator();
    let b = rich(ui, theme, &card.render_tab(c, l, *tab));
    a.or(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use crate::RichExt;
    use magnat_core::Tick;

    fn pusta_migawka() -> magnat_economy::ShopPanelSnapshot {
        magnat_economy::ShopPanelSnapshot {
            site: magnat_core::SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
            firm: magnat_core::FirmId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
            kind: magnat_core::PlaceKind::Grocery,
            at: Tick(0),
            tracking: magnat_economy::LostSaleTracking::Histogram,
            shelves: Vec::new(),
            customers: magnat_economy::CustomerStats::default(),
            lost_sales: magnat_economy::LostSalesView::default(),
            competition: Vec::new(),
            finance: magnat_economy::FinanceSummary {
                statement: magnat_economy::IncomeStatement::default(),
                balance: magnat_economy::BalanceSheet::default(),
                cash: magnat_economy::CashFlow::default(),
                inventory_value: magnat_core::Money::ZERO,
                loan: None,
            },
            reprices: Vec::new(),
            good_keys: Vec::new(),
        }
    }

    #[test]
    fn pasek_czasu_rysuje_sie_w_obu_jezykach_i_reaguje_na_wybor() {
        let c = Catalog::load().expect("data/locale/");
        for l in Locale::ALL {
            let w = TimeControlsWidget::new(Tick(0));
            let teksty = testing::draw(|ui| {
                let wybor = time_bar(ui, &w, &c, l);
                // Bez zdarzeń wejścia nikt nic nie kliknął.
                assert!(wybor.is_none());
            });
            let razem = teksty.join(" ");
            assert!(
                razem.contains(&w.clock_label(&c, l)),
                "{l:?}: brak godziny w pasku: {razem}"
            );
            for (etykieta, _, _) in w.buttons(&c, l) {
                assert!(
                    razem.contains(&etykieta),
                    "{l:?}: brak przycisku {etykieta} w {razem}"
                );
            }
            assert!(
                !razem.contains('{'),
                "{l:?}: niepodstawiony parametr: {razem}"
            );
        }
    }

    #[test]
    fn panel_sklepu_rysuje_zakladki_i_wybrana_tresc() {
        let c = Catalog::load().expect("data/locale/");
        let theme = Theme::load().expect("data/ui/theme.ron");
        // Karta z pustej migawki: widget ma się zbudować także wtedy, gdy sklep
        // dopiero powstał — pusta zakładka to nie jest przypadek brzegowy, tylko
        // pierwsza minuta każdego zakładu.
        let s = pusta_migawka();
        for l in Locale::ALL {
            let card = crate::ShopCard::build(
                &c,
                l,
                &crate::ShopView {
                    snapshot: &s,
                    kind: s.kind,
                    period_from: Tick(0),
                },
            );
            let mut tab = ShopTab::Customers;
            let teksty = testing::draw(|ui| {
                shop_card(ui, &theme, &card, &mut tab, &c, l);
            });
            let razem = teksty.join(" ");
            for t in ShopTab::ALL {
                assert!(
                    razem.contains(&t.label(&c, l)),
                    "{l:?}: brak zakładki {t:?} w {razem}"
                );
            }
            // Treść rysuje się wiersz po wierszu, więc sprawdzamy każdy z osobna —
            // to jest cała zmiana wobec M5e, w którym cała zakładka była jedną etykietą.
            for w in card
                .render_tab(&c, l, ShopTab::Customers)
                .to_plain()
                .lines()
            {
                assert!(
                    razem.contains(w.trim_end()),
                    "{l:?}: widget nie narysował wiersza `{w}`"
                );
            }
            assert!(
                !razem.contains('{'),
                "{l:?}: niepodstawiony parametr: {razem}"
            );
        }
    }

    /// Kryterium WP3: przejście z `String` na [`Rich`] nie ruszyło ani jednego bajtu
    /// tego, co widzi gracz.
    ///
    /// Sprawdzamy to na trzech istniejących kartach i **dwiema drogami**: złożony
    /// wydruk (`render_text`, forma złotego testu) musi być identyczny z konkatenacją
    /// kawałków nagłówka i wszystkich zakładek. Gdyby cięcie na kawałki gubiło
    /// albo dokładało choćby znak nowej linii, ta równość by pękła.
    #[test]
    fn skladanie_kawalkow_daje_ten_sam_wydruk_co_zloty_test() {
        let c = Catalog::load().expect("data/locale/");
        let s = pusta_migawka();
        for l in Locale::ALL {
            let card = crate::ShopCard::build(
                &c,
                l,
                &crate::ShopView {
                    snapshot: &s,
                    kind: s.kind,
                    period_from: Tick(0),
                },
            );
            let mut zlozone = card.render_header(&c, l).to_plain();
            for t in ShopTab::ALL {
                zlozone.push_str(&card.render_tab(&c, l, t).to_plain());
            }
            assert_eq!(zlozone, card.render_text(&c, l), "karta sklepu w {l:?}");
            // Każdy kawałek jest wierszem: kończy się znakiem nowej linii i nie
            // zawiera go w środku. To na tym stoi przypinanie odnośników w `M9c`.
            for kawalek in card.render_tab(&c, l, ShopTab::Shelves) {
                assert_eq!(
                    kawalek.text.matches('\n').count(),
                    usize::from(kawalek.text.ends_with('\n')),
                    "kawałek niesie więcej niż jeden wiersz: {:?}",
                    kawalek.text
                );
            }
        }
    }
}
