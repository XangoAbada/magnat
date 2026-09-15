//! Pule imion i nazwisk mieszkańców (M4d §5.10, WP13 — dług `Z-7` po M3).
//!
//! `Identity.first_name` i `last_name` istnieją od M3a **jako indeksy w puli, której
//! nikt nie zbudował**: kod losował surowe liczby z zakresu 256/512, a karta mieszkańca
//! wypisywała je dosłownie (`#84213 (45/321)`). Ten moduł buduje pulę i przestawia
//! zakres losowania na jej długość. `Identity` nie zmienia się ani o bit.
//!
//! **To nie jest lokalizacja UI** (CLAUDE.md): nazwy własne generowane per region nie
//! są tłumaczone — mieszkaniec nazwiskiem Schmidt nazywa się tak samo w obu wersjach
//! językowych. Dlatego pule siedzą w `data/names/`, a nie w `data/locale/`.
//!
//! ## Region jest wymiarem puli, a nie polem komponentu
//!
//! Pule sześciu regionów są **sklejone w jedną tablicę** w kolejności z
//! `data/names/regions.ron`, więc sam indeks niesie już region i płeć — nie trzeba ich
//! nigdzie przechowywać. Stąd dwie własności za darmo:
//!
//! * imię wylosowane dla `FLAG_MALE` **nie może** pochodzić z podzbioru żeńskiego,
//!   bo podzbiory są rozłącznymi zakresami indeksów;
//! * dziecko dziedziczy nazwisko po matce, a region odczytuje się z tego nazwiska —
//!   więc rodzina Schmidtów nie rodzi Agnieszki, a `Identity` nadal ma 16 bajtów.
//!
//! Konsekwencja wiążąca: **kolejność regionów w `regions.ron` i kolejność wpisów
//! w plikach pul są kontraktem**, tak samo jak kolejność klas w `data/vehicles/`.
//! Indeks siedzi w zapisie gry. Dopisywać wolno na końcu, przestawiać nie wolno.
//!
//! `ponytail:` katalog jest ładowany raz na proces przez `OnceLock`, a nie jako zasób
//! świata. Sufit nazwany: dwa światy w jednym procesie nie mogą mieć różnych pul —
//! nie mają dziś po co, bo pula jest **danymi wejściowymi** (00 §5), tak samo jak
//! `NeedTable` i `DemographyTable`, i z tego samego powodu nie wchodzi do hasha stanu.
//! Ścieżka wyjścia: zasób świata wstawiany w `register_society`, gdy modding z M12
//! zechce podmienić pulę w locie.

use magnat_core::Rng;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const NAMES_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum NameError {
    Io { file: String, msg: String },
    Ron { file: String, msg: String },
    Schema { file: String, found: u32 },
    Empty { file: String },
    /// Suma pul przekroczyła zakres `u16`, czyli szerokość pola w `Identity`.
    TooMany { what: &'static str, count: usize },
    NoRegions,
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameError::Io { file, msg } => write!(f, "{file}: {msg}"),
            NameError::Ron { file, msg } => write!(f, "{file}: błąd RON: {msg}"),
            NameError::Schema { file, found } => write!(
                f,
                "{file}: schema_version {found}, oczekiwano {NAMES_SCHEMA_VERSION}"
            ),
            NameError::Empty { file } => write!(f, "{file}: pusta pula"),
            NameError::TooMany { what, count } => {
                write!(f, "pula {what} ma {count} pozycji, a indeks jest u16")
            }
            NameError::NoRegions => write!(f, "data/names/regions.ron: brak regionów"),
        }
    }
}

impl std::error::Error for NameError {}

#[derive(Deserialize)]
struct RegionsFile {
    schema_version: u32,
    regions: Vec<(String, u16)>,
}

#[derive(Deserialize)]
struct FirstNamesFile {
    schema_version: u32,
    male: Vec<String>,
    female: Vec<String>,
}

#[derive(Deserialize)]
struct SurnamesFile {
    schema_version: u32,
    entries: Vec<(String, String)>,
}

/// Sklejone pule wszystkich regionów plus zakresy, które je rozgraniczają.
///
/// Układ tablicy imion dla regionu `r`: najpierw męskie, potem żeńskie.
/// `first_off[r] .. first_off[r] + male_len[r]` to imiona męskie,
/// `first_off[r] + male_len[r] .. first_off[r + 1]` — żeńskie.
#[derive(Debug)]
pub struct NameCatalog {
    codes: Vec<String>,
    /// Kumulowane wagi losowania regionu; ostatni element to suma.
    weights_cum: Vec<u32>,
    first: Vec<String>,
    first_off: Vec<u32>,
    male_len: Vec<u32>,
    /// Para form: `(męska, żeńska)`. Dla nazwisk nieodmiennych obie są identyczne.
    surnames: Vec<(String, String)>,
    surname_off: Vec<u32>,
}

static CATALOG: OnceLock<NameCatalog> = OnceLock::new();

/// Katalog pul, ładowany raz na proces. Panikuje, gdy `data/names/` jest niekompletne —
/// tak samo jak `data_dir()`, bo świat bez pul nazw nie da się uruchomić w żadnym trybie.
#[must_use]
pub fn catalog() -> &'static NameCatalog {
    CATALOG.get_or_init(|| {
        NameCatalog::load().unwrap_or_else(|e| panic!("data/names/: {e}"))
    })
}

fn czytaj(path: &PathBuf) -> Result<String, NameError> {
    std::fs::read_to_string(path).map_err(|e| NameError::Io {
        file: path.display().to_string(),
        msg: e.to_string(),
    })
}

fn parsuj<T: serde::de::DeserializeOwned>(path: &Path, txt: &str) -> Result<T, NameError> {
    ron::from_str(txt).map_err(|e| NameError::Ron {
        file: path.display().to_string(),
        msg: e.to_string(),
    })
}

impl NameCatalog {
    /// Ładuje `data/names/regions.ron` i parę plików każdego wymienionego regionu.
    pub fn load() -> Result<NameCatalog, NameError> {
        let rp = magnat_core::data_path("names/regions.ron");
        let rf: RegionsFile = parsuj(&rp, &czytaj(&rp)?)?;
        if rf.schema_version != NAMES_SCHEMA_VERSION {
            return Err(NameError::Schema {
                file: rp.display().to_string(),
                found: rf.schema_version,
            });
        }
        if rf.regions.is_empty() {
            return Err(NameError::NoRegions);
        }

        let mut cat = NameCatalog {
            codes: Vec::with_capacity(rf.regions.len()),
            weights_cum: Vec::with_capacity(rf.regions.len()),
            first: Vec::new(),
            first_off: vec![0],
            male_len: Vec::with_capacity(rf.regions.len()),
            surnames: Vec::new(),
            surname_off: vec![0],
        };

        let mut suma = 0u32;
        for (kod, waga) in &rf.regions {
            suma += u32::from(*waga);
            cat.weights_cum.push(suma);

            let fp = magnat_core::data_path(&format!("names/first_names_{kod}.ron"));
            let ff: FirstNamesFile = parsuj(&fp, &czytaj(&fp)?)?;
            if ff.schema_version != NAMES_SCHEMA_VERSION {
                return Err(NameError::Schema {
                    file: fp.display().to_string(),
                    found: ff.schema_version,
                });
            }
            if ff.male.is_empty() || ff.female.is_empty() {
                return Err(NameError::Empty {
                    file: fp.display().to_string(),
                });
            }
            cat.male_len.push(ff.male.len() as u32);
            cat.first.extend(ff.male);
            cat.first.extend(ff.female);
            cat.first_off.push(cat.first.len() as u32);

            let sp = magnat_core::data_path(&format!("names/surnames_{kod}.ron"));
            let sf: SurnamesFile = parsuj(&sp, &czytaj(&sp)?)?;
            if sf.schema_version != NAMES_SCHEMA_VERSION {
                return Err(NameError::Schema {
                    file: sp.display().to_string(),
                    found: sf.schema_version,
                });
            }
            if sf.entries.is_empty() {
                return Err(NameError::Empty {
                    file: sp.display().to_string(),
                });
            }
            cat.surnames.extend(sf.entries);
            cat.surname_off.push(cat.surnames.len() as u32);

            cat.codes.push(kod.clone());
        }

        if suma == 0 {
            return Err(NameError::NoRegions);
        }
        // Indeks jest `u16`, bo taka jest szerokość pola w `Identity` — a `Identity`
        // się nie zmienia (§5.10 pkt 1). Pula, która się nie mieści, jest błędem danych.
        if cat.first.len() > usize::from(u16::MAX) {
            return Err(NameError::TooMany {
                what: "imion",
                count: cat.first.len(),
            });
        }
        if cat.surnames.len() > usize::from(u16::MAX) {
            return Err(NameError::TooMany {
                what: "nazwisk",
                count: cat.surnames.len(),
            });
        }
        Ok(cat)
    }

    #[must_use]
    pub fn region_count(&self) -> u16 {
        self.codes.len() as u16
    }

    #[must_use]
    pub fn region_code(&self, region: u16) -> &str {
        &self.codes[usize::from(region)]
    }

    #[must_use]
    pub fn first_len(&self) -> usize {
        self.first.len()
    }

    #[must_use]
    pub fn surname_len(&self) -> usize {
        self.surnames.len()
    }

    /// Region nowego gospodarstwa: losowanie ważone z `regions.ron`.
    pub fn pick_region(&self, r: &mut Rng) -> u16 {
        let suma = *self.weights_cum.last().expect("regiony");
        let los = r.gen_range_u32(suma);
        self.weights_cum.partition_point(|&w| w <= los) as u16
    }

    /// Imię z podzbioru właściwego płci i regionowi. Indeks niesie jedno i drugie.
    pub fn pick_first(&self, region: u16, male: bool, r: &mut Rng) -> u16 {
        let i = usize::from(region);
        let base = self.first_off[i];
        let m = self.male_len[i];
        if male {
            (base + r.gen_range_u32(m)) as u16
        } else {
            let f = self.first_off[i + 1] - base - m;
            (base + m + r.gen_range_u32(f)) as u16
        }
    }

    pub fn pick_surname(&self, region: u16, r: &mut Rng) -> u16 {
        let i = usize::from(region);
        let base = self.surname_off[i];
        let n = self.surname_off[i + 1] - base;
        (base + r.gen_range_u32(n)) as u16
    }

    /// Region, z którego pochodzi nazwisko o tym indeksie. To jest sposób, w jaki
    /// dziecko dostaje imię z regionu rodziny, nie mając pola na region.
    #[must_use]
    pub fn region_of_surname(&self, idx: u16) -> u16 {
        (self.surname_off.partition_point(|&o| o <= u32::from(idx)) - 1) as u16
    }

    /// Czy indeks imienia wskazuje na podzbiór męski swojego regionu.
    #[must_use]
    pub fn first_is_male(&self, idx: u16) -> bool {
        let i = self.first_off.partition_point(|&o| o <= u32::from(idx)) - 1;
        u32::from(idx) < self.first_off[i] + self.male_len[i]
    }

    /// Imię spoza zakresu daje `"?"` zamiast paniki: karta inspekcji ma pokazać, że
    /// coś jest nie tak, a nie wywalić grę z powodu jednego mieszkańca.
    #[must_use]
    pub fn first_name(&self, idx: u16) -> &str {
        self.first.get(usize::from(idx)).map_or("?", String::as_str)
    }

    /// Forma nazwiska zgodna z płcią. Dla nieodmiennych obie są tym samym napisem.
    #[must_use]
    pub fn surname(&self, idx: u16, male: bool) -> &str {
        match self.surnames.get(usize::from(idx)) {
            Some((m, f)) => {
                if male {
                    m
                } else {
                    f
                }
            }
            None => "?",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{rng, StreamId, Tick};

    fn kat() -> &'static NameCatalog {
        catalog()
    }

    #[test]
    fn pula_laduje_sie_i_ma_wszystkie_regiony() {
        let c = kat();
        assert!(c.region_count() >= 2, "multiregionowość zniknęła z danych");
        assert_eq!(c.region_code(0), "pl", "region rodzimy przestał być pierwszy");
        for r in 0..c.region_count() {
            let i = usize::from(r);
            assert!(c.male_len[i] > 0 && c.first_off[i + 1] - c.first_off[i] > c.male_len[i]);
            assert!(c.surname_off[i + 1] > c.surname_off[i]);
        }
    }

    /// §7.1 `indeks_zawsze_ma_wpis_w_puli` — zakres losowania pochodzi z długości puli,
    /// nie ze stałej. To jest własność, którą łamał `gen_range_u32(256)` przy 220 imionach.
    #[test]
    fn indeks_zawsze_ma_wpis_w_puli() {
        let c = kat();
        for i in 0..100_000u32 {
            let mut r = rng(0xD1CE, StreamId::PopGen, i, Tick(7));
            let region = c.pick_region(&mut r);
            assert!(region < c.region_count());
            let male = i % 2 == 0;
            let f = c.pick_first(region, male, &mut r);
            let s = c.pick_surname(region, &mut r);
            assert!(usize::from(f) < c.first_len(), "imię {f} poza pulą");
            assert!(usize::from(s) < c.surname_len(), "nazwisko {s} poza pulą");
            assert_ne!(c.first_name(f), "?");
            assert_ne!(c.surname(s, male), "?");
        }
    }

    /// §7.1 `imie_zgadza_sie_z_plcia` — 100 %, bo podzbiory są rozłącznymi zakresami.
    #[test]
    fn imie_zgadza_sie_z_plcia() {
        let c = kat();
        for i in 0..20_000u32 {
            let mut r = rng(0x1CE, StreamId::Demography, i, Tick(3));
            let region = c.pick_region(&mut r);
            let male = i % 3 != 0;
            let f = c.pick_first(region, male, &mut r);
            assert_eq!(c.first_is_male(f), male, "imię {} nie ma tej płci", c.first_name(f));
        }
    }

    /// Region nazwiska jest odczytywalny z samego indeksu — na tym stoi dziedziczenie.
    #[test]
    fn region_wynika_z_indeksu_nazwiska() {
        let c = kat();
        for region in 0..c.region_count() {
            let mut r = rng(0xBEEF, StreamId::Migration, u32::from(region), Tick(1));
            for _ in 0..500 {
                let s = c.pick_surname(region, &mut r);
                assert_eq!(c.region_of_surname(s), region);
                let f = c.pick_first(c.region_of_surname(s), true, &mut r);
                assert!(f >= c.first_off[usize::from(region)] as u16);
            }
        }
    }

    /// §7.1 `nazwisko_dziedziczone_w_formie_wlasnej_plci` — ten sam indeks, dwie formy.
    /// Nazwisko nieodmienne ma obie identyczne i to też jest sprawdzane.
    #[test]
    fn nazwisko_ma_dwie_formy_a_nieodmienne_jedna() {
        let c = kat();
        let mut odmiennych = 0;
        let mut nieodmiennych = 0;
        // Region 0 to pula rodzima — w niej oba rodzaje muszą wystąpić.
        for i in c.surname_off[0]..c.surname_off[1] {
            let idx = i as u16;
            if c.surname(idx, true) == c.surname(idx, false) {
                nieodmiennych += 1;
            } else {
                odmiennych += 1;
            }
        }
        assert!(odmiennych > 0, "pula rodzima nie ma nazwisk odmiennych");
        assert!(nieodmiennych > 0, "pula rodzima nie ma nazwisk nieodmiennych");
    }
}
