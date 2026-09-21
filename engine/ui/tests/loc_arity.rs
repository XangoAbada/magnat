//! Katalog tekstów: zła liczba form łamie wczytanie, brak klucza nie łamie klatki.
//!
//! Dwie usterki R2e o jednym adresie i jednym pliku testowym, bo obie potrzebują
//! **podstawionego** katalogu `data/locale/`, a `magnat_core::data_dir()`
//! rozstrzyga się raz na proces (`OnceLock`). Dwa testy w tym samym pliku szłyby
//! równolegle i biły się o ten sam katalog na dysku, więc jest jeden test i dwie
//! fazy — kolejność jest tu treścią, a nie wygodą.
//!
//! **Faza 1 (R2-WP23).** `CLAUDE.md` mówił do R2e, że polski ma trzy formy
//! liczebnika (1 · 2–4 · 5+), a `DE-8` w M9b dołożyło czwartą — CLDR-owe `other`
//! dla wartości ułamkowych. Zmiana była świadoma, opisana i przetestowana; reguła
//! w `CLAUDE.md` została stara. Rozjazd jest groźniejszy, niż wygląda: faza, która
//! wykona regułę dosłownie, napisze wpis o trzech formach, a bramka równości
//! zbiorów kluczy tego nie widzi — porównuje klucze, nie kształt wpisu. Sam
//! mechanizm (`LocError::PluralArity`) istnieje od `DE-8` i nie miał testu, więc
//! R2e dokłada test, a nie mechanizm.
//!
//! **Faza 2 (R2-WP28, poz. 50).** `Catalog::must` rozwijało `Option` przez
//! `panic!`, a wołał go `fmt_key` — najszerszy pośrednik w interfejsie. Literówka
//! w nazwie klucza wywracała **klatkę gry**. Argument „to błąd danych wykrywany
//! przy starcie" był nieprawdziwy podwójnie: w kodzie produkcyjnym nie było ani
//! jednego wołania przy starcie, a `Catalog::load` sprawdza **równość zbiorów**
//! `pl`/`en`, więc klucz nieobecny w żadnym z dwóch plików przechodzi tę bramkę
//! bez śladu.

use magnat_ui::loc::LOCALE_SCHEMA_VERSION;
use magnat_ui::{Catalog, Locale};

fn plik(locale: &str, formy: &[&str]) -> String {
    let lista = formy
        .iter()
        .map(|f| format!("{f:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "(schema_version: {LOCALE_SCHEMA_VERSION}, locale: {locale:?}, entries: {{\n\
         \x20   \"ui.unit.shops\": Plural([{lista}]),\n\
         }})\n"
    )
}

#[test]
fn liczebnik_o_zlej_liczbie_form_lamie_wczytanie_a_brak_klucza_nie_lamie_klatki() {
    let dir = std::env::temp_dir().join("magnat-r2e-loc-arity");
    let locale = dir.join("locale");
    std::fs::create_dir_all(&locale).expect("katalog tymczasowy");
    let angielski = plik("en", &["{n} shop", "{n} shops"]);

    // ── faza 1: trzy formy po polsku, dokładnie tak, jak napisałby ktoś, kto
    //    wykonał starą regułę dosłownie.
    std::fs::write(
        locale.join("pl.ron"),
        plik("pl", &["{n} sklep", "{n} sklepy", "{n} sklepów"]),
    )
    .expect("pl.ron");
    std::fs::write(locale.join("en.ron"), &angielski).expect("en.ron");
    std::env::set_var(magnat_core::assets::DATA_DIR_ENV, &dir);

    let blad = Catalog::load().expect_err("katalog z trzema formami nie ma prawa się wczytać");
    let opis = blad.to_string();
    assert!(
        opis.contains("ui.unit.shops") && opis.contains("pl"),
        "błąd ma nazwać klucz i język, a mówi: {opis}"
    );

    // ── faza 2: cztery formy przechodzą — inaczej faza 1 sprawdzałaby wyłącznie,
    //    że nic się nie wczytuje — a brakujący klucz daje pusty napis.
    std::fs::write(
        locale.join("pl.ron"),
        plik(
            "pl",
            &["{n} sklep", "{n} sklepy", "{n} sklepów", "{n} sklepu"],
        ),
    )
    .expect("pl.ron");
    let c = Catalog::load().expect("cztery formy po polsku, dwie po angielsku");

    // Klucz, którego nie ma w **żadnym** z dwóch plików — czyli ten, którego bramka
    // równości zbiorów z definicji nie widzi.
    assert_eq!(c.fmt_key(Locale::Pl, "ui.nie.ma.takiego", &[]), "");
    assert_eq!(c.plural_key(Locale::Pl, "ui.nie.ma.takiego", 5), "");
    // Klucz, który jest — żeby test nie sprawdzał, że wszystko jest puste.
    assert_eq!(c.plural_key(Locale::Pl, "ui.unit.shops", 5), "5 sklepów");

    std::fs::remove_dir_all(&dir).ok();
}
