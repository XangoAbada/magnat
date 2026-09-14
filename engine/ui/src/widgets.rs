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
use crate::loc::{Catalog, Locale};
use crate::time::TimeControlsWidget;
use magnat_core::SimSpeed;

/// Kolor paska potrzeby. Trzy progi, nie gradient: gracz ma odczytać „dobrze / słabo /
/// źle" jednym spojrzeniem, a nie porównywać odcienie.
fn kolor_potrzeby(level: u8, critical: bool) -> egui::Color32 {
    if critical {
        egui::Color32::from_rgb(200, 70, 60)
    } else if level < 50 {
        egui::Color32::from_rgb(200, 160, 60)
    } else {
        egui::Color32::from_rgb(90, 160, 100)
    }
}

/// Kolor bloku czynności. `ActivityKind` ma swój kolor i ma go mieć **jeden** — oś planu
/// i oś realizacji muszą dać się porównać wzrokiem.
fn kolor_czynnosci(kind: magnat_core::ActivityKind) -> egui::Color32 {
    use magnat_core::ActivityKind as A;
    match kind {
        A::Sleep => egui::Color32::from_rgb(70, 80, 130),
        A::Work => egui::Color32::from_rgb(80, 120, 170),
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
pub fn day_timeline(ui: &mut egui::Ui, m: &CitizenModel, c: &Catalog, l: Locale) {
    const WYS: f32 = 18.0;
    let wiersze = m.timeline().rows(c, l);
    let szerokosc = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(szerokosc, WYS * 2.0 + 6.0),
        egui::Sense::hover(),
    );
    let malarz = ui.painter_at(rect);
    let na_minute = szerokosc / 1440.0;

    // Godziny co trzy: siatka, która pozwala powiedzieć „wyszedł koło ósmej" bez liczenia.
    for h in (0..=24).step_by(3) {
        let x = rect.left() + h as f32 * 60.0 * na_minute;
        malarz.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
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
                egui::Stroke::new(2.0, egui::Color32::from_rgb(230, 120, 60)),
                egui::StrokeKind::Inside,
            );
        }
    };

    for r in &wiersze {
        blok(
            rect.top(),
            r.start_min,
            r.end_min % 1440,
            kolor_czynnosci(r.kind),
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
            kolor_czynnosci(kind).gamma_multiply(0.75),
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
            tekst.color(egui::Color32::from_rgb(230, 120, 60))
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
pub fn needs(ui: &mut egui::Ui, card: &CitizenCard) {
    for n in &card.needs {
        ui.horizontal(|ui| {
            ui.add_sized(
                egui::vec2(110.0, 16.0),
                egui::Label::new(egui::RichText::new(&n.label).small()),
            );
            let (rect, odp) =
                ui.allocate_exact_size(egui::vec2(120.0, 12.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 2.0, egui::Color32::from_gray(45));
            let mut wypelnienie = rect;
            wypelnienie.set_width(rect.width() * f32::from(n.level) / 100.0);
            p.rect_filled(wypelnienie, 2.0, kolor_potrzeby(n.level, n.critical));
            odp.on_hover_text(&n.tooltip);
            ui.label(egui::RichText::new(format!("{}", n.level)).monospace().small());
        });
    }
}

/// Cała karta mieszkańca w kolejności z §5.11.
pub fn citizen_card(ui: &mut egui::Ui, m: &CitizenModel, c: &Catalog, l: Locale) {
    let card = &m.card;
    ui.label(
        egui::RichText::new(c.fmt_key(
            l,
            "ui.card.header",
            &[
                ("imie", &card.header.name),
                ("wiek", &crate::inspect::reason::years(c, l, card.header.age_years)),
                ("zawod", &card.header.occupation),
                ("adres", &card.header.address),
            ],
        ))
        .strong(),
    );
    ui.separator();

    ui.label(egui::RichText::new(c.fmt_key(l, "ui.card.needs", &[])).strong());
    needs(ui, card);
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
                egui::RichText::new(crate::zlotowki(kwota))
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

    day_timeline(ui, m, c, l);
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Tick;

    /// Rysuje `zawartosc` w prawdziwym kontekście `egui` i zwraca każdy tekst, który
    /// trafił na ekran. To jest test panelu, a nie modelu: przechodzi tylko wtedy,
    /// gdy widget faktycznie się zbudował i nie spanikował po drodze.
    fn narysuj(mut zawartosc: impl FnMut(&mut egui::Ui)) -> Vec<String> {
        let ctx = egui::Context::default();
        let mut wyjscie = ctx.run_ui(egui::RawInput::default(), &mut zawartosc);
        let ksztalty = std::mem::take(&mut wyjscie.shapes);
        wyjscie.drop_without_applying_deltas();
        let mut teksty = Vec::new();
        for k in &ksztalty {
            zbierz(&k.shape, &mut teksty);
        }
        teksty
    }

    fn zbierz(s: &egui::epaint::Shape, out: &mut Vec<String>) {
        match s {
            egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::epaint::Shape::Vec(v) => {
                for x in v {
                    zbierz(x, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn pasek_czasu_rysuje_sie_w_obu_jezykach_i_reaguje_na_wybor() {
        let c = Catalog::load().expect("data/locale/");
        for l in Locale::ALL {
            let w = TimeControlsWidget::new(Tick(0));
            let teksty = narysuj(|ui| {
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
            assert!(!razem.contains('{'), "{l:?}: niepodstawiony parametr: {razem}");
        }
    }
}
