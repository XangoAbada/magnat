//! Kodeks podatkowy: stawki, klasy i terminy (M8a §5.1).
//!
//! Plik `data/city/tax.ron` jest **polityką, nie projektem** — zmienia się uchwałą
//! rady i przebiegiem balansatora, nie rekompilacją. Kod zna kształt tabeli i nic
//! poza tym; ani jedna stawka nie jest tu wpisana na sztywno.
//!
//! **Zero floatów** (M8a §5.0). Stawki procentowe w punktach bazowych (`u32`,
//! 10 000 bp = 100 %), kwotowe w groszach. To jest wymóg silniejszy niż `K-6`:
//! naliczenie ma dawać ten sam grosz na każdej platformie, więc nie przechodzi
//! nawet przez `det_math`.

use magnat_core::{GoodId, Money, Tick};
use magnat_supply::Catalog;
use serde::{Deserialize, Serialize};

/// Wersja schematu `data/city/tax.ron`. Podbicie wymaga migracji zapisów gry.
pub const TAX_SCHEMA_VERSION: u32 = 1;

/// Próg skali podatkowej. `upper == 0` znaczy „bez górnej granicy" i wolno mu
/// wystąpić wyłącznie w ostatnim wierszu — pilnuje tego walidator.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PitBracket {
    /// Górna granica dochodu rocznego objętego tą stawką, w groszach.
    pub upper: i64,
    pub bp: u32,
}

/// Klasa stawki VAT. Przypisanie idzie przez **domenę klucza towaru**, tak samo
/// jak klasa taryfowa (`AD-12`): domen jest dwanaście i lista jest zamknięta,
/// więc nowy towar nie może zgubić klasy przez przeoczenie w danych.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VatClass {
    pub key: String,
    pub domains: Vec<String>,
    pub bp: u32,
}

/// Stawka akcyzy: kwota w groszach za kilogram wyrobu.
///
/// **Kwotowa, nie procentowa** — i to jest cała różnica między akcyzą a VAT-em.
/// Podwyżka akcyzy podnosi koszt nabycia niezależnie od tego, po ile towar
/// akurat chodzi, więc uderza w tani wyrób mocniej niż w drogi.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ExciseRate {
    pub good: String,
    pub per_kg: i64,
}

/// Opłata koncesyjna za rok działalności reglamentowanej, po kluczu rodzaju zakładu.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LicenseFee {
    pub site_type: String,
    pub fee_per_year: i64,
}

/// Kodeks w postaci, w jakiej leży w danych. Rozwiązanie kluczy tekstowych
/// na identyfikatory robi [`TaxCode::resolve`] — raz, przy budowie miasta.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TaxCode {
    pub schema_version: u32,
    pub cit_bp: u32,
    pub loss_carry_years: u8,
    pub pit_free_allowance: i64,
    pub pit_brackets: Vec<PitBracket>,
    pub vat_classes: Vec<VatClass>,
    pub property_bp_per_year: u32,
    pub excise: Vec<ExciseRate>,
    pub licenses: Vec<LicenseFee>,
    pub vat_due_day: u8,
    pub pit_due_day: u8,
    pub property_due_day: u8,
    pub excise_due_day: u8,
    pub cit_due_month: u8,
    pub late_interest_bp_per_year: u32,
    pub time_bar_days: u32,
    /// Tick, od którego obowiązują te stawki. **Nigdy wstecz**: należność naliczona
    /// wcześniej zachowuje swoją stawkę, bo `TaxCharge` niesie ją w migawce.
    ///
    /// Nie ma go w pliku — nadaje go uchwała, czyli M8e. Wczytany kodeks obowiązuje
    /// od zera świata.
    #[serde(default)]
    pub effective_from: Tick,
}

#[derive(Debug)]
pub enum TaxCodeError {
    Io(String),
    Parse(String),
    Schema {
        found: u32,
        want: u32,
    },
    /// Domena z klucza towaru nie ma klasy VAT — łapane przy ładowaniu, nie u gracza.
    DomainWithoutVat(String),
    /// Akcyza na towar, którego nie ma w katalogu: literówka w danych, nie fakt.
    UnknownExciseGood(String),
    /// Skala, w której próg bez górnej granicy nie jest ostatni — dalsze wiersze
    /// byłyby nieosiągalne, a nikt by tego nie zauważył.
    OpenBracketNotLast,
    EmptyBrackets,
}

impl std::fmt::Display for TaxCodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaxCodeError::Io(e) | TaxCodeError::Parse(e) => write!(f, "data/city/tax.ron: {e}"),
            TaxCodeError::Schema { found, want } => write!(
                f,
                "data/city/tax.ron: schema_version {found}, oczekiwano {want}"
            ),
            TaxCodeError::DomainWithoutVat(d) => {
                write!(f, "data/city/tax.ron: domena `{d}` bez klasy VAT")
            }
            TaxCodeError::UnknownExciseGood(k) => {
                write!(f, "data/city/tax.ron: akcyza na nieznany towar `{k}`")
            }
            TaxCodeError::OpenBracketNotLast => write!(
                f,
                "data/city/tax.ron: próg bez górnej granicy musi być ostatni"
            ),
            TaxCodeError::EmptyBrackets => write!(f, "data/city/tax.ron: pusta skala PIT"),
        }
    }
}

impl std::error::Error for TaxCodeError {}

impl TaxCode {
    /// Kodeks, który nic nie nalicza. Potrzebny wyłącznie po to, żeby zasób
    /// miasta dał się wyjąć ze świata na czas kroku systemu wyłącznego.
    #[must_use]
    pub fn empty() -> TaxCode {
        TaxCode {
            schema_version: TAX_SCHEMA_VERSION,
            cit_bp: 0,
            loss_carry_years: 0,
            pit_free_allowance: 0,
            pit_brackets: Vec::new(),
            vat_classes: Vec::new(),
            property_bp_per_year: 0,
            excise: Vec::new(),
            licenses: Vec::new(),
            vat_due_day: 20,
            pit_due_day: 20,
            property_due_day: 15,
            excise_due_day: 25,
            cit_due_month: 3,
            late_interest_bp_per_year: 0,
            time_bar_days: u32::MAX,
            effective_from: Tick(0),
        }
    }

    pub fn load(path: &std::path::Path) -> Result<TaxCode, TaxCodeError> {
        let tekst = std::fs::read_to_string(path).map_err(|e| TaxCodeError::Io(e.to_string()))?;
        let c: TaxCode = ron::from_str(&tekst).map_err(|e| TaxCodeError::Parse(e.to_string()))?;
        if c.schema_version != TAX_SCHEMA_VERSION {
            return Err(TaxCodeError::Schema {
                found: c.schema_version,
                want: TAX_SCHEMA_VERSION,
            });
        }
        c.check_brackets()?;
        Ok(c)
    }

    pub fn load_default() -> Result<TaxCode, TaxCodeError> {
        TaxCode::load(&magnat_core::data_path("city/tax.ron"))
    }

    fn check_brackets(&self) -> Result<(), TaxCodeError> {
        if self.pit_brackets.is_empty() {
            return Err(TaxCodeError::EmptyBrackets);
        }
        let ostatni = self.pit_brackets.len() - 1;
        for (i, b) in self.pit_brackets.iter().enumerate() {
            if b.upper <= 0 && i != ostatni {
                return Err(TaxCodeError::OpenBracketNotLast);
            }
        }
        Ok(())
    }

    /// Stawka VAT towaru po domenie jego klucza. `None` znaczy „domena bez klasy",
    /// czyli błąd danych — walidator [`TaxCode::resolve`] nie przepuszcza takiego
    /// katalogu dalej, więc w symulacji ten wariant nie występuje.
    #[must_use]
    pub fn vat_bp_of_key(&self, key: &str) -> Option<u32> {
        let domena = key.split_once('_').map_or(key, |(d, _)| d);
        self.vat_classes
            .iter()
            .find(|c| c.domains.iter().any(|d| d == domena))
            .map(|c| c.bp)
    }

    /// Rozwiązuje klucze tekstowe na tablice indeksowane `GoodId`. Wołane raz,
    /// przy budowie miasta — nigdy w pętli transakcji.
    pub fn resolve(&self, cat: &Catalog) -> Result<VatTable, TaxCodeError> {
        let n = cat.goods.len();
        let mut vat = vec![0u32; n];
        let mut excise = vec![0i64; n];
        for g in &cat.goods {
            let Some(bp) = self.vat_bp_of_key(&g.key) else {
                let domena = g.key.split_once('_').map_or(&*g.key, |(d, _)| d);
                return Err(TaxCodeError::DomainWithoutVat(domena.to_string()));
            };
            vat[g.id.0 as usize] = bp;
        }
        for r in &self.excise {
            let Some(g) = cat.goods.iter().find(|g| &*g.key == r.good.as_str()) else {
                return Err(TaxCodeError::UnknownExciseGood(r.good.clone()));
            };
            excise[g.id.0 as usize] = r.per_kg;
        }
        Ok(VatTable { vat, excise })
    }
}

/// Stawki rozwiązane na indeksy towaru. Tablica, a nie mapa: wyszukanie stawki
/// dzieje się przy **każdej** wycenie ceny półkowej, więc ma być indeksowaniem.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct VatTable {
    vat: Vec<u32>,
    excise: Vec<i64>,
}

impl VatTable {
    /// Stawka VAT towaru w punktach bazowych. Towar spoza katalogu (czyli błąd
    /// wołającego) dostaje zero, bo doliczenie mu przypadkowej stawki byłoby gorsze.
    #[inline]
    #[must_use]
    pub fn vat_bp(&self, good: GoodId) -> u32 {
        self.vat.get(good.0 as usize).copied().unwrap_or(0)
    }

    /// Stawka akcyzy w groszach za kilogram. Zero dla towaru nieobłożonego.
    #[inline]
    #[must_use]
    pub fn excise_per_kg(&self, good: GoodId) -> Money {
        Money(self.excise.get(good.0 as usize).copied().unwrap_or(0))
    }

    /// Ile towarów katalogu jest obłożonych akcyzą. Zero znaczy, że cała ścieżka
    /// akcyzowa jest w tym świecie martwa — i to jest informacja do raportu, a nie
    /// stan, który wolno przemilczeć (`R2`).
    #[must_use]
    pub fn excise_goods(&self) -> usize {
        self.excise.iter().filter(|r| **r != 0).count()
    }

    /// Ile towarów ma niezerową stawkę VAT.
    #[must_use]
    pub fn vat_goods(&self) -> usize {
        self.vat.iter().filter(|r| **r != 0).count()
    }
}
