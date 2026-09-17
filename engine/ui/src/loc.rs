//! Lokalizacja — `LocKey`, katalogi `data/locale/{pl,en}.ron`, `plural` (CLAUDE.md).
//!
//! **Literał tekstowy w kodzie UI to błąd, nie skrót.** Każdy napis widziany przez
//! gracza ma klucz, a klucz ma wpis w obu językach; test `klucze_obu_jezykow_sa_identyczne`
//! łamie build, gdy któregoś brakuje. Cichy fallback na drugi język byłby gorszy od
//! braku: wychodzi dopiero u gracza, który przełączył język.
//!
//! `LocKey` jest **interned** — indeksem do posortowanej tablicy kluczy, a nie łańcuchem.
//! Dzięki temu widget trzyma cztery bajty zamiast napisu, a sprawdzenie „czy ten klucz
//! istnieje" dzieje się raz, przy ładowaniu.
//!
//! Właścicielem tego modułu jest M3 (szkielet `engine/ui`); M9 rozszerza go o panele
//! biznesowe i edytor reguł, M12 o pełną lokalizację i modding.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Język interfejsu. Rozszerzanie listy należy do M12 — tu są dwa, bo dwa są w danych.
///
/// Serializowalny od M9a: język siedzi w profilu gracza (`game::shell::Settings`),
/// czyli w pliku obok zapisu, a nie w zapisie świata.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub enum Locale {
    #[default]
    Pl,
    En,
}

impl Locale {
    pub const ALL: [Locale; 2] = [Locale::Pl, Locale::En];

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Locale::Pl => "pl",
            Locale::En => "en",
        }
    }

    /// Ile form liczebnika ma ten język. Polski trzy (1 · 2–4 · 5+), angielski dwie.
    #[must_use]
    pub const fn plural_forms(self) -> usize {
        match self {
            Locale::Pl => 3,
            Locale::En => 2,
        }
    }

    /// Indeks formy liczebnika dla `n`.
    ///
    /// Reguła polska jest tą z CLDR i **nie jest** „1, 2–4, reszta": 22 idzie do formy
    /// drugiej, ale 12 do trzeciej. Pomyłka tu widać dopiero przy liczbach powyżej
    /// dziesięciu, więc test wypisuje je jawnie.
    #[must_use]
    pub const fn plural_form(self, n: u64) -> usize {
        match self {
            Locale::En => {
                if n == 1 {
                    0
                } else {
                    1
                }
            }
            Locale::Pl => {
                if n == 1 {
                    0
                } else if matches!(n % 10, 2..=4) && !matches!(n % 100, 12..=14) {
                    1
                } else {
                    2
                }
            }
        }
    }
}

impl std::str::FromStr for Locale {
    type Err = LocError;

    fn from_str(s: &str) -> Result<Locale, LocError> {
        match s {
            "pl" => Ok(Locale::Pl),
            "en" => Ok(Locale::En),
            other => Err(LocError::UnknownLocale(other.to_string())),
        }
    }
}

/// Klucz tekstu — indeks do posortowanej tablicy kluczy katalogu.
///
/// Nieprawidłowy `LocKey` nie istnieje: jedyną drogą do niego jest [`Catalog::key`],
/// które zwraca `None` dla klucza spoza katalogu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LocKey(u32);

impl LocKey {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Deserialize)]
enum Entry {
    One(String),
    Plural(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct LocaleFile {
    schema_version: u32,
    locale: String,
    entries: BTreeMap<String, Entry>,
}

pub const LOCALE_SCHEMA_VERSION: u32 = 1;

/// Błąd ładowania katalogu.
///
/// `Ron` niesie `SpannedError` w pudełku: wariant ma 128 B, a `Result<Catalog, LocError>`
/// wraca z każdej funkcji ładującej. Błąd składni w danych zdarza się raz na uruchomienie,
/// więc alokacja przy nim nie kosztuje nic, a bez pudełka każdy `Result` w tym module
/// rośnie do rozmiaru najgorszego przypadku.
#[derive(Debug)]
pub enum LocError {
    Io(String, std::io::Error),
    Ron(String, Box<ron::de::SpannedError>),
    Schema {
        file: String,
        found: u32,
    },
    UnknownLocale(String),
    /// Klucz jest w jednym języku, a w drugim go nie ma.
    KeyMismatch {
        key: String,
        missing_in: &'static str,
    },
    /// Wpis liczebnikowy nie ma tylu form, ile wymaga język.
    PluralArity {
        key: String,
        locale: &'static str,
        want: usize,
        got: usize,
    },
}

impl std::fmt::Display for LocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocError::Io(p, e) => write!(f, "{p}: {e}"),
            LocError::Ron(p, e) => write!(f, "{p}: {e}"),
            LocError::Schema { file, found } => write!(
                f,
                "{file}: schema_version {found}, oczekiwano {LOCALE_SCHEMA_VERSION}"
            ),
            LocError::UnknownLocale(s) => write!(f, "nieznany język: {s}"),
            LocError::KeyMismatch { key, missing_in } => {
                write!(f, "klucz `{key}` nie istnieje w języku {missing_in}")
            }
            LocError::PluralArity {
                key,
                locale,
                want,
                got,
            } => write!(
                f,
                "klucz liczebnikowy `{key}` ma w {locale} {got} form zamiast {want}"
            ),
        }
    }
}

impl std::error::Error for LocError {}

/// Katalog tekstów obu języków. Jedno źródło prawdy dla całego interfejsu.
#[derive(Clone, Debug)]
pub struct Catalog {
    /// Posortowane klucze; indeks w tej tablicy **jest** `LocKey`.
    keys: Vec<String>,
    /// Wpisy w kolejności `Locale::ALL`, w lockstepie z `keys`.
    entries: [Vec<Entry>; 2],
}

impl Catalog {
    /// Wczytuje `pl.ron` i `en.ron` z `data/locale/` i sprawdza, że opisują to samo.
    pub fn load() -> Result<Catalog, LocError> {
        let dir = magnat_core::data_path("locale");
        let mut pliki: Vec<(Locale, BTreeMap<String, Entry>)> = Vec::new();
        for l in Locale::ALL {
            let path = dir.join(format!("{}.ron", l.code()));
            let nazwa = path.display().to_string();
            let txt = std::fs::read_to_string(&path).map_err(|e| LocError::Io(nazwa.clone(), e))?;
            let f: LocaleFile =
                ron::from_str(&txt).map_err(|e| LocError::Ron(nazwa.clone(), Box::new(e)))?;
            if f.schema_version != LOCALE_SCHEMA_VERSION {
                return Err(LocError::Schema {
                    file: nazwa,
                    found: f.schema_version,
                });
            }
            if f.locale != l.code() {
                return Err(LocError::UnknownLocale(f.locale));
            }
            pliki.push((l, f.entries));
        }

        // Zbiory kluczy muszą być identyczne — to jest ta bramka z CLAUDE.md.
        let (pierwszy, drugi) = (&pliki[0].1, &pliki[1].1);
        for k in pierwszy.keys() {
            if !drugi.contains_key(k) {
                return Err(LocError::KeyMismatch {
                    key: k.clone(),
                    missing_in: Locale::ALL[1].code(),
                });
            }
        }
        for k in drugi.keys() {
            if !pierwszy.contains_key(k) {
                return Err(LocError::KeyMismatch {
                    key: k.clone(),
                    missing_in: Locale::ALL[0].code(),
                });
            }
        }

        let keys: Vec<String> = pierwszy.keys().cloned().collect();
        let mut entries: [Vec<Entry>; 2] = [Vec::new(), Vec::new()];
        for (i, (l, mapa)) in pliki.iter().enumerate() {
            for k in &keys {
                let e = mapa.get(k).expect("klucz sprawdzony wyżej").clone();
                if let Entry::Plural(f) = &e {
                    if f.len() != l.plural_forms() {
                        return Err(LocError::PluralArity {
                            key: k.clone(),
                            locale: l.code(),
                            want: l.plural_forms(),
                            got: f.len(),
                        });
                    }
                }
                entries[i].push(e);
            }
        }
        Ok(Catalog { keys, entries })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Klucz tekstowy → `LocKey`. `None` znaczy „takiego napisu nie ma w katalogu",
    /// i to jest jedyny moment, w którym literówka w kluczu wychodzi na jaw.
    #[must_use]
    pub fn key(&self, key: &str) -> Option<LocKey> {
        self.keys
            .binary_search_by(|k| k.as_str().cmp(key))
            .ok()
            .map(|i| LocKey(i as u32))
    }

    /// Klucz, który **musi** istnieć. Panikuje z nazwą klucza — brak napisu w katalogu
    /// jest błędem danych wykrywanym przy starcie, nie stanem do obsłużenia w widgecie.
    #[must_use]
    pub fn must(&self, key: &str) -> LocKey {
        self.key(key)
            .unwrap_or_else(|| panic!("brak klucza `{key}` w data/locale/"))
    }

    /// Tekst bez podstawień.
    #[must_use]
    pub fn text(&self, locale: Locale, key: LocKey) -> &str {
        match &self.entries[locale as usize][key.0 as usize] {
            Entry::One(s) => s,
            Entry::Plural(f) => f.first().map_or("", String::as_str),
        }
    }

    /// Forma liczebnikowa dla `n`, z podstawionym `{n}`.
    #[must_use]
    pub fn plural(&self, locale: Locale, key: LocKey, n: u64) -> String {
        let wzorzec = match &self.entries[locale as usize][key.0 as usize] {
            Entry::Plural(f) => f.get(locale.plural_form(n)).map_or("", String::as_str),
            Entry::One(s) => s,
        };
        podstaw(wzorzec, &[("n", &n.to_string())])
    }

    /// Tekst z podstawieniami `{nazwa}`.
    #[must_use]
    pub fn format(&self, locale: Locale, key: LocKey, args: &[(&str, &str)]) -> String {
        podstaw(self.text(locale, key), args)
    }

    /// Skrót do `format` po kluczu tekstowym — dla ścieżek, które i tak panikują
    /// na brakującym kluczu (renderer powodów, nagłówki karty).
    #[must_use]
    pub fn fmt_key(&self, locale: Locale, key: &str, args: &[(&str, &str)]) -> String {
        self.format(locale, self.must(key), args)
    }

    /// Wszystkie klucze — do testu zgodności i do diagnostyki.
    #[must_use]
    pub fn keys(&self) -> &[String] {
        &self.keys
    }
}

/// Podstawianie `{nazwa}`. Jedno przejście po wzorcu, bez alokacji na nietrafione klamry.
///
/// `ponytail:` własne podstawianie zamiast biblioteki szablonów. Sufit nazwany: nie ma
/// tu formatowania liczb, dat ani warunków — gdy M9 albo M12 będzie potrzebowało
/// formatowania zależnego od języka (separator tysięcy, waluta), to jest miejsce,
/// w którym wchodzi ICU MessageFormat.
#[must_use]
pub fn podstaw(wzorzec: &str, args: &[(&str, &str)]) -> String {
    if !wzorzec.contains('{') {
        return wzorzec.to_string();
    }
    let mut out = String::with_capacity(wzorzec.len() + 16);
    let mut reszta = wzorzec;
    while let Some(i) = reszta.find('{') {
        out.push_str(&reszta[..i]);
        let Some(j) = reszta[i..].find('}') else {
            out.push_str(&reszta[i..]);
            return out;
        };
        let nazwa = &reszta[i + 1..i + j];
        match args.iter().find(|(k, _)| *k == nazwa) {
            Some((_, v)) => out.push_str(v),
            // Nietrafiona klamra zostaje widoczna: napis „{ile}" na ekranie mówi,
            // czego brakuje, a cicha pustka nie mówi nic.
            None => out.push_str(&reszta[i..=i + j]),
        }
        reszta = &reszta[i + j + 1..];
    }
    out.push_str(reszta);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn klucze_obu_jezykow_sa_identyczne() {
        // To jest bramka z CLAUDE.md: brakujący klucz łamie build, a nie wychodzi
        // dopiero u gracza, który przełączył język.
        let c = Catalog::load().expect("data/locale/");
        assert!(c.len() > 100, "katalog ma {} kluczy", c.len());
        for k in c.keys() {
            let key = c.must(k);
            for l in Locale::ALL {
                assert!(
                    !c.text(l, key).is_empty(),
                    "pusty tekst dla `{k}` w {}",
                    l.code()
                );
            }
        }
    }

    #[test]
    fn polska_liczba_mnoga_ma_trzy_formy_i_zna_nastolatki() {
        let f = |n| Locale::Pl.plural_form(n);
        assert_eq!(f(1), 0);
        assert_eq!(f(2), 1);
        assert_eq!(f(4), 1);
        assert_eq!(f(5), 2);
        // Pułapka: 12–14 idą do formy trzeciej mimo końcówki 2–4.
        assert_eq!(f(12), 2);
        assert_eq!(f(13), 2);
        assert_eq!(f(14), 2);
        assert_eq!(f(22), 1);
        assert_eq!(f(112), 2);
        assert_eq!(f(122), 1);
        assert_eq!(f(0), 2);

        let e = |n| Locale::En.plural_form(n);
        assert_eq!(e(1), 0);
        assert_eq!(e(0), 1);
        assert_eq!(e(2), 1);
    }

    #[test]
    fn liczebnik_z_katalogu_odmienia_sie_w_obu_jezykach() {
        let c = Catalog::load().expect("data/locale/");
        let k = c.must("ui.unit.minutes");
        assert_eq!(c.plural(Locale::Pl, k, 1), "1 minuta");
        assert_eq!(c.plural(Locale::Pl, k, 3), "3 minuty");
        assert_eq!(c.plural(Locale::Pl, k, 15), "15 minut");
        assert_eq!(c.plural(Locale::En, k, 1), "1 minute");
        assert_eq!(c.plural(Locale::En, k, 15), "15 minutes");
    }

    #[test]
    fn podstawienie_zostawia_nietrafiona_klamre_widoczna() {
        assert_eq!(podstaw("a {x} b", &[("x", "1")]), "a 1 b");
        assert_eq!(podstaw("a {y} b", &[("x", "1")]), "a {y} b");
        assert_eq!(podstaw("bez klamer", &[]), "bez klamer");
        assert_eq!(podstaw("niedomknięta {x", &[("x", "1")]), "niedomknięta {x");
    }

    #[test]
    fn klucz_spoza_katalogu_nie_istnieje() {
        let c = Catalog::load().expect("data/locale/");
        assert!(c.key("ui.nie.ma.takiego").is_none());
    }
}
