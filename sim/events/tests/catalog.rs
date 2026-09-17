//! Walidator katalogu zdarzeń (WP5) — test CI.
//!
//! Sprawdza to, czego kompilator nie sprawdzi: że katalog w `data/events/` da się
//! wczytać, że ma wszystkie sześć kategorii i że żadna definicja nie jest martwa.
//! „Martwa" ma tu twarde znaczenie: definicja, której hazard nie może urosnąć,
//! albo taka, której oczekiwana liczba wystąpień w dwudziestoleciu jest poniżej
//! jedności — czyli ryzyko `R2` fazy zmierzone, a nie opisane.

use magnat_events::catalog::CATEGORY_FILES;
use magnat_events::{EventCatalog, EventScope, MAX_PPM};

fn katalog() -> EventCatalog {
    EventCatalog::load_default().expect("data/events/*.ron")
}

#[test]
fn katalog_sie_laduje_i_ma_szesc_kategorii() {
    let c = katalog();
    for (nazwa, kat) in CATEGORY_FILES {
        let ile = c.defs.iter().filter(|d| d.category == kat).count();
        assert!(ile >= 4, "kategoria {nazwa} ma tylko {ile} definicji");
    }
    assert!(c.defs.len() >= 24, "katalog ma {} definicji", c.defs.len());
}

#[test]
fn kazda_definicja_moze_zajsc_w_dwudziestoleciu() {
    // Ile instancji zakresu ma typowe miasto scenariusza `m8miasto`. Liczby są
    // ostrożne w dół: przy większym mieście każda definicja zachodzi częściej,
    // więc próg trzyma tym pewniej.
    let instancji = |s: EventScope| -> f64 {
        match s {
            EventScope::World => 1.0,
            EventScope::District => 6.0,
            EventScope::Site => 8.0,
            EventScope::Firm => 10.0,
            EventScope::Network => 1.0,
        }
    };
    for d in &katalog().defs {
        // Najwyższy mnożnik, jaki krzywe tej definicji mogą dać razem.
        let szczyt: f64 = d
            .trigger
            .factors
            .iter()
            .map(|f| f.curve.0.iter().map(|(_, m)| *m).max().unwrap_or(10_000) as f64 / 10_000.0)
            .product();
        let ocen_na_rok = if d.hourly { 360.0 * 24.0 } else { 360.0 };
        // Bramka sezonowa obcina rok do ćwiartek — ostrożnie zakładamy jedną porę.
        let sezonowa = d
            .trigger
            .gate
            .iter()
            .any(|g| matches!(g, magnat_events::Precondition::Season(_)));
        let ulamek_roku = if sezonowa { 0.25 } else { 1.0 };
        let oczekiwane = f64::from(d.trigger.base_ppm) / 1e6
            * szczyt
            * ocen_na_rok
            * ulamek_roku
            * instancji(d.scope)
            * 20.0;
        assert!(
            oczekiwane >= 1.0,
            "`{}` w szczycie hazardu daje {oczekiwane:.2} wystąpienia na dwudziestolecie \
             — definicja jest martwa (ryzyko R2)",
            d.key
        );
        // I w drugą stronę: definicja, która przy neutralnych sondach zachodzi
        // częściej niż raz na dobę, zamieniłaby grę w festiwal katastrof (`R1`).
        let neutralnie = f64::from(d.trigger.base_ppm) / 1e6 * ocen_na_rok * instancji(d.scope);
        assert!(
            neutralnie <= 60.0,
            "`{}` przy neutralnych sondach daje {neutralnie:.1} wystąpień na rok",
            d.key
        );
    }
}

#[test]
fn hazard_nie_przekracza_pewnosci() {
    // Iloczyn czterech krzywych po ×8 to ×4096 — bez przycięcia hazard wyszedłby
    // poza skalę ppm i rzut przestałby cokolwiek znaczyć.
    for d in &katalog().defs {
        let ppm = magnat_events::hazard_ppm(d, |_| i64::MAX, None);
        assert!(ppm <= MAX_PPM, "`{}` daje hazard {ppm} ppm", d.key);
    }
}

#[test]
fn klucz_kroniki_ma_ksztalt_klucza_lokalizacji() {
    for d in &katalog().defs {
        assert!(
            d.chronicle.starts_with("event.") && d.chronicle.matches('.').count() >= 2,
            "`{}` ma klucz kroniki `{}`, a ten ma być kluczem lokalizacji",
            d.key,
            d.chronicle
        );
    }
}

#[test]
fn kazdy_wpis_kroniki_ma_tlumaczenie_w_obu_jezykach() {
    // Klucz kroniki jest kluczem lokalizacji, ale **nikt go jeszcze nie renderuje**:
    // panel kroniki powstaje w M8e, karta zdarzenia w M9. Bez tego testu brak
    // tłumaczenia wyszedłby dopiero wtedy — czyli u gracza, i to w jednym języku.
    //
    // Test na tekście pliku, a nie przez `engine/ui`: `sim/events` nie zależy od UI
    // i nie ma powodu, żeby zaczął zależeć dla jednego sprawdzenia.
    for jezyk in ["pl", "en"] {
        let p = magnat_core::data_path(&format!("locale/{jezyk}.ron"));
        let tekst = std::fs::read_to_string(&p).expect("plik lokalizacji");
        for d in &katalog().defs {
            let klucz = format!("\"{}\":", d.chronicle);
            assert!(
                tekst.contains(&klucz),
                "brak klucza `{}` w data/locale/{jezyk}.ron — zdarzenie `{}` nie ma \
                 wpisu do kroniki w tym języku",
                d.chronicle,
                d.key
            );
        }
    }
}
