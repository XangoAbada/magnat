//! Rząd zakładek (`M9b` §5.8, `Z-8`; `ui-design.md` §4 „Zakładki").
//!
//! # Dlaczego dopiero teraz
//!
//! `widgets.rs` mówił wprost: „osobnej abstrakcji zakładek w tym crate nie ma i nie jest
//! potrzebna". To była prawda przy jednym konsumencie i przestała nią być przy czwartym:
//! `ShopTab`, `SupplyTab`, `FirmTab` i karta inspekcji z `M9c` mają po własnej pętli
//! `selectable_label`, a wymagania z `ui-design.md` §4 (sufit siedmiu, sterowanie
//! strzałkami, kontrast wybranej) każda z nich spełniałaby po swojemu. To jest ten moment,
//! w którym YAGNI każe abstrakcję zrobić — nie wcześniej.

use crate::theme::{ColorToken, Theme};

/// Sufit z `ui-design.md` §4: powyżej siedmiu zakładek dzieli się panel,
/// a nie zwija etykiety.
pub const MAX_TABS: usize = 7;

/// Rysuje rząd zakładek i zwraca `true`, jeśli gracz zmienił wybór.
///
/// Stan wyboru trzyma **wołający** (`current`), a nie widget — dzięki temu przeżywa
/// przebudowę drzewa i zapisuje się tam, gdzie reszta stanu panelu. Sterowanie
/// klawiaturą: strzałki lewo/prawo działają, gdy fokus jest na którejkolwiek zakładce.
///
/// # Panics
/// Gdy `tabs` ma więcej niż [`MAX_TABS`] pozycji albo jest puste — oba są błędem
/// w kodzie panelu, nie stanem, w który gracz może wejść.
pub fn tab_strip<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    theme: &Theme,
    tabs: &[T],
    current: &mut T,
    label: impl Fn(T) -> String,
) -> bool {
    assert!(
        !tabs.is_empty() && tabs.len() <= MAX_TABS,
        "rząd zakładek ma {} pozycji, dopuszczalne 1..={MAX_TABS}",
        tabs.len()
    );
    let przed = tabs.iter().position(|t| *t == *current).unwrap_or(0);
    let mut wybrany = przed;
    let mut fokus = None;

    ui.horizontal(|ui| {
        for (i, t) in tabs.iter().enumerate() {
            let aktywna = i == przed;
            let tekst = egui::RichText::new(label(*t))
                .font(theme.font(crate::theme::TextRole::Body))
                .color(if aktywna {
                    theme.color(ColorToken::TextPrimary)
                } else {
                    theme.color(ColorToken::TextSecondary)
                });
            let odp = ui.selectable_label(aktywna, tekst);
            if odp.clicked() {
                wybrany = i;
            }
            if odp.has_focus() {
                fokus = Some((i, odp.id));
            }
        }
    });

    // Strzałki przesuwają wybór **bez zawijania**: przytrzymana strzałka na końcu
    // rzędu ma się zatrzymać, a nie wrócić na początek — zawijanie w rzędzie
    // czterech zakładek czyta się jak przypadek, nie jak nawigacja.
    if let Some((i, _)) = fokus {
        let lewo = ui.input(|s| s.key_pressed(egui::Key::ArrowLeft));
        let prawo = ui.input(|s| s.key_pressed(egui::Key::ArrowRight));
        if lewo && i > 0 {
            wybrany = i - 1;
        }
        if prawo && i + 1 < tabs.len() {
            wybrany = i + 1;
        }
    }

    if wybrany != przed {
        *current = tabs[wybrany];
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Z {
        A,
        B,
        C,
    }

    fn etykieta(z: Z) -> String {
        match z {
            Z::A => "Półki".to_string(),
            Z::B => "Klienci".to_string(),
            Z::C => "Konkurencja".to_string(),
        }
    }

    #[test]
    fn rzad_rysuje_wszystkie_etykiety_i_nie_zmienia_wyboru_bez_wejscia() {
        let theme = Theme::load().expect("data/ui/theme.ron");
        let mut wybor = Z::B;
        let mut zmiana = true;
        let teksty = testing::draw(|ui| {
            zmiana = tab_strip(ui, &theme, &[Z::A, Z::B, Z::C], &mut wybor, etykieta);
        });
        let razem = teksty.join(" ");
        for z in [Z::A, Z::B, Z::C] {
            assert!(razem.contains(&etykieta(z)), "brak zakładki {z:?}: {razem}");
        }
        assert!(!zmiana);
        assert_eq!(wybor, Z::B);
    }

    #[test]
    #[should_panic(expected = "rząd zakładek")]
    fn osiem_zakladek_jest_bledem_kodu_a_nie_zwinietym_rzedem() {
        let theme = Theme::load().expect("data/ui/theme.ron");
        let mut wybor = 0u8;
        let osiem: Vec<u8> = (0..8).collect();
        let _ = testing::draw(|ui| {
            tab_strip(ui, &theme, &osiem, &mut wybor, |t| t.to_string());
        });
    }
}
