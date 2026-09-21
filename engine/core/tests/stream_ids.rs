//! Weryfikacja wykonania `K-4` na `StreamId` (R2-WP23).
//!
//! `K-4` rozdziela numery strumieni blokami po 20 na fazę i mówi, że wartość raz
//! nadana jest niezmienna — bo zmiana numeru strumienia zmienia każdy świat
//! wygenerowany wcześniej z tego samego ziarna. Reguła zapisana w dokumencie
//! nie broni się sama: numer użyty dwa razy sprzęga dwa rozkłady i nie łamie
//! przy tym ani jednej kompilacji.
//!
//! Test czyta **źródło** enuma, a nie jego wartości. Powodem jest brak tablicy
//! `StreamId::ALL` — `StreamId` nie jest słownikiem `vocab_enum!` i nie ma być,
//! bo nikt nie iteruje po strumieniach. Bez tablicy jedyną drogą do listy
//! wariantów jest tekst pliku, w którym ta lista i tak jest kontraktem.

use std::collections::BTreeMap;

const ZRODLO: &str = include_str!("../src/rng.rs");

/// Bloki z `K-4`: faza → zakres. Rezerwa dalsza to `baza + 1000` o tej samej
/// szerokości — stąd druga para w każdym wierszu.
const BLOKI: &[(&str, u16, u16)] = &[
    ("M0", 0, 19),
    ("M1", 100, 119),
    ("M2", 120, 139),
    ("M3", 140, 159),
    ("M4", 160, 179),
    ("M5", 180, 199),
    ("M6", 200, 219),
    ("M7", 220, 239),
    ("M8", 240, 259),
    ("M9", 260, 279),
    ("M10", 280, 299),
    ("M11", 300, 319),
    ("M12", 320, 339),
];

/// Wariant z jawną dyskryminantą: `    NazwaWariantu = 241,`.
fn warianty() -> Vec<(String, u16)> {
    let mut out = Vec::new();
    let mut w_enumie = false;
    for linia in ZRODLO.lines() {
        if linia.starts_with("pub enum StreamId {") {
            w_enumie = true;
            continue;
        }
        if w_enumie && linia == "}" {
            break;
        }
        if !w_enumie {
            continue;
        }
        let t = linia.trim();
        let Some((nazwa, reszta)) = t.split_once(" = ") else {
            continue;
        };
        if !nazwa
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase() && nazwa.chars().all(char::is_alphanumeric))
        {
            continue;
        }
        let Some(liczba) = reszta.strip_suffix(',') else {
            continue;
        };
        if let Ok(n) = liczba.replace('_', "").parse::<u16>() {
            out.push((nazwa.to_string(), n));
        }
    }
    out
}

/// Wariant bez jawnej dyskryminanty: `    NazwaWariantu,`.
fn warianty_bez_numeru() -> Vec<String> {
    let mut out = Vec::new();
    let mut w_enumie = false;
    for linia in ZRODLO.lines() {
        if linia.starts_with("pub enum StreamId {") {
            w_enumie = true;
            continue;
        }
        if w_enumie && linia == "}" {
            break;
        }
        if !w_enumie {
            continue;
        }
        let t = linia.trim();
        if let Some(nazwa) = t.strip_suffix(',') {
            if !nazwa.is_empty()
                && nazwa.chars().all(char::is_alphanumeric)
                && nazwa.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            {
                out.push(nazwa.to_string());
            }
        }
    }
    out
}

#[test]
fn kazdy_wariant_ma_numer_wpisany_recznie() {
    // Numeru użytego dwa razy pilnuje `rustc` (`E0081`) i to wystarcza — ale tylko
    // dla wariantów, które numer mają. Wariant dopisany **bez** `= N` bierze
    // poprzedni plus jeden, czyli numer nadany przez pozycję w pliku. To jest
    // dokładnie ta wieczna wartość, o której `K-4` mówi „raz nadana, niezmienna":
    // wstawienie czegokolwiek wyżej przesunęłoby ją i zmieniło każdy świat
    // wygenerowany wcześniej z tego samego ziarna.
    let bez = warianty_bez_numeru();
    assert!(
        bez.is_empty(),
        "wariant `StreamId` bez jawnego numeru bierze go z pozycji w pliku (`K-4`): {}",
        bez.join(", ")
    );
    assert!(
        !warianty().is_empty(),
        "nie odczytano ani jednego wariantu — zmienił się kształt deklaracji `StreamId`"
    );
}

#[test]
fn kazdy_numer_mieszka_w_bloku_swojej_fazy() {
    let mut poza: Vec<String> = Vec::new();
    for (nazwa, n) in warianty() {
        // Rezerwa dalsza `baza + 1000` przysługuje fazie, której 20 wartości nie starczy.
        let w_bloku = n / 1000 * 1000;
        let reszta = n % 1000;
        let trafiony = BLOKI
            .iter()
            .any(|(_, od, do_)| reszta >= *od && reszta <= *do_)
            && (w_bloku == 0 || w_bloku == 1000);
        if !trafiony {
            poza.push(format!("{nazwa} = {n}"));
        }
    }
    assert!(
        poza.is_empty(),
        "numer spoza bloków `K-4` (0–19, 100–339 co 20, rezerwa +1000): {}",
        poza.join(", ")
    );
}

#[test]
fn blok_m8_niesie_numery_zadeklarowane_w_k63_i_k67() {
    // `K-63` zajmuje 241–244, `K-67` — 245 i 250. `EventHazard = 240` i
    // `CityPolicy = 249` **zostają niezajęte i takimi mają zostać**: hazard i wybór
    // burmistrza są funkcjami stanu, nie losowaniem. Test pilnuje obu stron —
    // dopisanie ich byłoby cofnięciem rozstrzygnięcia, a nie uzupełnieniem luki.
    let m8: BTreeMap<u16, String> = warianty()
        .into_iter()
        .filter(|(_, n)| (240..=259).contains(n))
        .map(|(nazwa, n)| (n, nazwa))
        .collect();
    for (n, nazwa) in [
        (241, "EventRoll"),
        (242, "EventSeverity"),
        (243, "EventDuration"),
        (244, "Weather"),
        (245, "Election"),
        (250, "TenderScoring"),
    ] {
        assert_eq!(
            m8.get(&n).map(String::as_str),
            Some(nazwa),
            "`K-63`/`K-67` nadały numerowi {n} nazwę {nazwa} i jest ona wieczna"
        );
    }
    assert!(
        !m8.contains_key(&240),
        "`EventHazard = 240` zostaje niezajęty (`K-63`): hazard jest funkcją stanu świata"
    );
    assert!(
        !m8.contains_key(&249),
        "`CityPolicy = 249` zostaje niezajęty (`K-67`): burmistrz nie rzuca kostką"
    );
}
