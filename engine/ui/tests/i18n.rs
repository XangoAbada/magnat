//! i18n i typografia: liczebniki, polskie znaki w atlasie, pseudo-lokalizacja (WP3).
//!
//! Wszystkie trzy rzeczy łączy to, że wychodzą **u gracza**, a nie w kompilacji:
//! zła forma liczebnika, brak znaku w atlasie fontów i napis, który nie mieści się
//! w kontrolce. Każda z nich daje się sprawdzić i dlatego jest sprawdzana.

use magnat_ui::{testing, Catalog, Locale};

/// Kryterium §7 dokumentu fazy: `1 sklep / 2 sklepy / 5 sklepów / 1,5 sklepu`.
///
/// Czwarta forma jest tą, o której najłatwiej zapomnieć: CLDR-owe `other` dotyczy
/// **ułamków**, a „1,5 sklep" i „1,5 sklepy" są oba źle.
#[test]
fn polski_liczebnik_ma_cztery_formy_a_angielski_dwie() {
    let c = Catalog::load().expect("data/locale/");
    let k = c.must("ui.unit.shops");

    assert_eq!(c.plural(Locale::Pl, k, 1), "1 sklep");
    assert_eq!(c.plural(Locale::Pl, k, 2), "2 sklepy");
    assert_eq!(c.plural(Locale::Pl, k, 5), "5 sklepów");
    assert_eq!(c.plural(Locale::Pl, k, 22), "22 sklepy");
    assert_eq!(c.plural(Locale::Pl, k, 12), "12 sklepów");
    assert_eq!(c.plural(Locale::Pl, k, 0), "0 sklepów");
    // Ułamek: forma czwarta i liczba sformatowana po polsku (przecinek dziesiętny).
    assert_eq!(c.plural_frac(Locale::Pl, k, 15, 1), "1,5 sklepu");
    assert_eq!(c.plural_frac(Locale::Pl, k, 25, 1), "2,5 sklepu");
    // Wartość ułamkowa **bez ułamka** wraca do form całkowitych: „2,0 sklepu"
    // nie ma prawa wyprzeć „2 sklepów".
    assert_eq!(c.plural_frac(Locale::Pl, k, 20, 1), "2 sklepy");
    assert_eq!(c.plural_frac(Locale::Pl, k, 50, 1), "5 sklepów");

    assert_eq!(c.plural(Locale::En, k, 1), "1 shop");
    assert_eq!(c.plural(Locale::En, k, 5), "5 shops");
    assert_eq!(c.plural_frac(Locale::En, k, 15, 1), "1.5 shops");
}

/// `docs/ui-design.md` §3.2: krój **musi** mieć pełny zestaw polskich diakrytyków.
///
/// Brak znaku w atlasie widać dopiero u gracza — jako prostokąt zastępczy. `egui`
/// podmienia go na znak zastępczy o własnej szerokości, więc test porównuje szerokości:
/// znak, którego font nie ma, mierzy się dokładnie tak jak `U+FFFD`.
#[test]
fn atlas_fontow_zna_polskie_znaki() {
    let ctx = egui::Context::default();
    // Jedna klatka, żeby `egui` zbudowało atlas.
    let _ = testing::draw_in(&ctx, testing::input(testing::EKRAN), |ui| {
        ui.label("ąćęłńóśźż ĄĆĘŁŃÓŚŹŻ");
    });
    let font = egui::FontId::proportional(13.0);
    let zastepczy = ctx.fonts_mut(|f| f.glyph_width(&font, '\u{FFFD}'));
    for c in "ąćęłńóśźżĄĆĘŁŃÓŚŹŻ".chars() {
        let w = ctx.fonts_mut(|f| f.glyph_width(&font, c));
        assert!(w > 0.0, "znak `{c}` ma zerową szerokość");
        assert!(
            (w - zastepczy).abs() > f32::EPSILON,
            "znak `{c}` mierzy się jak znak zastępczy — brak go w atlasie"
        );
    }
}

/// Pseudo-lokalizacja wydłuża napisy o ~40 %, przepisuje je diakrytykami i **nie tyka
/// podstawień** — inaczej mierzyłaby własną awarię zamiast układu.
#[test]
fn pseudolokalizacja_wydluza_napis_i_zostawia_podstawienia() {
    let zwykly = Catalog::load().expect("data/locale/");
    let pseudo = Catalog::load_pseudo().expect("data/locale/");
    assert_eq!(zwykly.len(), pseudo.len(), "pseudo-katalog zgubił klucze");

    for l in Locale::ALL {
        for klucz in [
            "ui.shell.new_game",
            "ui.newgame.size.hint",
            "ui.preview.title",
        ] {
            let a = zwykly.fmt_key(l, klucz, &[]);
            let b = pseudo.fmt_key(l, klucz, &[]);
            assert!(b.len() > a.len(), "`{klucz}` w {l:?} się nie wydłużył");
            assert!(b.starts_with('[') && b.ends_with(']'), "brak granic: {b}");
        }
        // Podstawienie działa mimo przepisania wzorca.
        let z = pseudo.fmt_key(
            l,
            "ui.shell.version",
            &[("wersja", "0.1.0"), ("format", "1")],
        );
        assert!(z.contains("0.1.0"), "podstawienie przepadło: {z}");
        assert!(!z.contains('{'), "niepodstawiony parametr: {z}");
    }
}

/// Ten sam pomiar co wyżej, ale na **całej treści obu plików lokalizacji**.
///
/// Powód jest empiryczny: podpowiedź klawiszy w powłoce („↑↓ wybór · ←→ zmiana…")
/// wychodziła u gracza jako ciąg prostokątów, bo domyślny atlas `egui` nie ma
/// znaków strzałek, a własnego kroju gra jeszcze nie wgrywa (`Theme::font`, M11).
/// Lista znaków z góry by tego nie złapała — łapie to dopiero przejście po tekstach,
/// które naprawdę jadą na ekran.
///
/// Test **nie czyta plików RON jako tekstu**: komentarze w nich są pełne znaków
/// rysunkowych, których nikt nie wyświetla. Pyta katalog o wszystkie formy każdego
/// klucza, czyli dokładnie o to, co zobaczy gracz.
#[test]
fn atlas_fontow_zna_wszystkie_znaki_z_lokalizacji() {
    let c = Catalog::load().expect("data/locale/");
    let mut znaki: std::collections::BTreeMap<char, String> = std::collections::BTreeMap::new();
    for nazwa in c.keys() {
        let k = c.must(nazwa);
        for l in Locale::ALL {
            let mut warianty = vec![c.text(l, k).to_string()];
            // Formy liczebnikowe: 1 · 2–4 · 5+ · ułamek. Klucz nieliczebnikowy
            // zwraca z nich ten sam napis, więc pytanie jest tanie i bezpieczne.
            warianty.extend([0, 1, 2, 5].map(|n| c.plural(l, k, n)));
            warianty.push(c.plural_frac(l, k, 15, 1));
            for s in warianty {
                for ch in s.chars() {
                    if ch.is_whitespace() {
                        continue;
                    }
                    znaki.entry(ch).or_insert_with(|| nazwa.clone());
                }
            }
        }
    }

    let ctx = egui::Context::default();
    let wszystkie: String = znaki.keys().collect();
    let _ = testing::draw_in(&ctx, testing::input(testing::EKRAN), |ui| {
        ui.label(&wszystkie);
    });
    let font = egui::FontId::proportional(13.0);
    let zastepczy = ctx.fonts_mut(|f| f.glyph_width(&font, '\u{FFFD}'));

    let mut brakujace: Vec<String> = Vec::new();
    for (ch, klucz) in &znaki {
        if *ch == '\u{FFFD}' {
            continue;
        }
        let w = ctx.fonts_mut(|f| f.glyph_width(&font, *ch));
        if w <= 0.0 || (w - zastepczy).abs() <= f32::EPSILON {
            brakujace.push(format!("U+{:04X} `{ch}` (np. `{klucz}`)", *ch as u32));
        }
    }
    assert!(
        brakujace.is_empty(),
        "znaki spoza atlasu fontów — gracz zobaczy prostokąty:\n  {}",
        brakujace.join("\n  ")
    );
}
