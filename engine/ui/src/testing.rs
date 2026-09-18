//! Rysowanie interfejsu w teście, bez karty graficznej (00 §6, headless-first).
//!
//! `egui` jest czystym procesorem: zamienia wejście i kod widgetów na listę kształtów,
//! a malowaniem trójkątów zajmuje się `engine/render::ui`. Dzięki temu **każdy** ekran
//! tej gry da się narysować w CI i sprawdzić, co na nim stanęło.
//!
//! Moduł jest publiczny, a nie `#[cfg(test)]`, bo korzystają z niego testy w `game/`
//! (ekrany powłoki) — a `#[cfg(test)]` nie przechodzi przez granicę crate'u.

/// Rozmiar ekranu w testach: 1600 × 900 pikseli logicznych.
pub const EKRAN: egui::Vec2 = egui::vec2(1600.0, 900.0);

/// Rysuje zawartość w prawdziwym kontekście `egui` i zwraca każdy tekst, który trafił
/// na ekran.
///
/// To jest test **widgetu**, a nie modelu: przechodzi tylko wtedy, gdy panel faktycznie
/// się zbudował i nie spanikował po drodze.
pub fn draw(content: impl FnMut(&mut egui::Ui)) -> Vec<String> {
    let ctx = egui::Context::default();
    draw_in(&ctx, input(EKRAN), content).0
}

/// Jak [`draw`], ale w podanym kontekście i z podanym wejściem — dla testów, które
/// klikają, piszą albo sprawdzają, czy napisy mieszczą się na ekranie.
///
/// Zwraca teksty oraz **sumę prostokątów każdego napisu**, który trafił na ekran.
/// To jest odpowiedź na pytanie „czy pseudo-lokalizacja ×1,4 rozwala układ": rozwala,
/// jeśli któryś napis wyszedł poza ekran. `Rect::NOTHING`, gdy nic nie narysowano.
pub fn draw_in(
    ctx: &egui::Context,
    input: egui::RawInput,
    content: impl FnMut(&mut egui::Ui),
) -> (Vec<String>, egui::Rect) {
    let mut content = content;
    let mut wyjscie = ctx.run_ui(input, &mut content);
    let ksztalty = std::mem::take(&mut wyjscie.shapes);
    wyjscie.drop_without_applying_deltas();
    let mut teksty = Vec::new();
    let mut zakres = egui::Rect::NOTHING;
    for k in &ksztalty {
        zbierz(&k.shape, &mut teksty, &mut zakres);
    }
    (teksty, zakres)
}

/// Wejście o ustalonym rozmiarze ekranu — bez tego `egui` przyjmuje domyślny
/// i test mierzyłby układ na innym ekranie niż gra.
#[must_use]
pub fn input(ekran: egui::Vec2) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, ekran)),
        ..Default::default()
    }
}

/// Wejście z pojedynczym naciśnięciem klawisza — do testów przejścia klawiaturą.
#[must_use]
pub fn key(ekran: egui::Vec2, k: egui::Key) -> egui::RawInput {
    let mut we = input(ekran);
    we.events.push(egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    we.events.push(egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    we
}

fn zbierz(s: &egui::epaint::Shape, out: &mut Vec<String>, zakres: &mut egui::Rect) {
    match s {
        egui::epaint::Shape::Text(t) => {
            out.push(t.galley.text().to_string());
            *zakres = zakres.union(egui::Rect::from_min_size(t.pos, t.galley.size()));
        }
        egui::epaint::Shape::Vec(v) => {
            for x in v {
                zbierz(x, out, zakres);
            }
        }
        _ => {}
    }
}
