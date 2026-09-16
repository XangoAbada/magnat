//! Klucze towarów w `data/economy/` muszą istnieć w katalogu M6 (`AF-3`).
//!
//! Powód, dla którego to jest test, a nie założenie: `GoodTable::build` **pomija** klucz,
//! którego nie umie rozwiązać, i nie jest to błąd ładowania — tak ma być, bo `retail.ron`
//! wolno wymieniać towary spoza koszyka epoki. Cena tej tolerancji jest taka, że literówka
//! albo przemianowanie po stronie M6 ujawnia się dopiero jako **pusta półka**, i to bez
//! ani jednego komunikatu. Sprawdzenie kosztuje jedno wczytanie obu katalogów.
//!
//! `cpi.ron` jest w tym samym teście, bo jego koszyk `q_0` jest bazą indeksu: towar,
//! który z niego wypadnie, nie zgłasza się nigdzie, tylko przestawia całą historię CPI.

use magnat_economy::EconomyData;
use magnat_supply::catalog::load_default;

#[test]
fn kazdy_klucz_z_data_economy_jest_w_katalogu() {
    let cat = load_default("contemporary").expect("katalog z data/");
    let d = EconomyData::load_default().expect("data/economy/");

    let brak: Vec<&str> = d
        .retail
        .goods
        .iter()
        .map(|g| g.key.as_str())
        .filter(|k| cat.good_id(k).is_none())
        .collect();
    assert!(
        brak.is_empty(),
        "retail.ron wskazuje towary spoza katalogu: {brak:?}"
    );

    let brak: Vec<&str> = d
        .cpi
        .items
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| cat.good_id(k).is_none())
        .collect();
    assert!(
        brak.is_empty(),
        "cpi.ron wskazuje towary spoza katalogu: {brak:?}"
    );
}
