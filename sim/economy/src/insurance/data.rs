//! Kalibracja ubezpieczeń i giełdy: `data/tuning/insurance.ron` (M10d, `K-35`).
//!
//! Jeden plik na całą podfazę, bo obie mechaniki stroi się razem: zamożność
//! gospodarstw decyduje i o tym, kto inwestuje, i o tym, kogo stać na polisę.
//!
//! **Czego w tym pliku nie ma i nie będzie: ani jednej liczby mówiącej, jak groźna
//! jest dzielnica.** To jest treść kryterium WP10.12, a nie przeoczenie — składka
//! wychodzi z historii szkód, a nie z tabeli ryzyka. Walidator tego nie sprawdzi
//! (nie da się sprawdzić nieobecności pola), więc pilnuje tego test
//! `skladka_rosnie_z_historii_a_nie_z_danych`.

use magnat_core::Money;
use serde::Deserialize;
use std::path::Path;

/// Wersja schematu `data/tuning/insurance.ron` (00 §5).
pub const INSURANCE_SCHEMA_VERSION: u32 = 1;

/// Kalibracja zakładu ubezpieczeń.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct InsuranceData {
    /// Ile polisomiesięcy własnej historii waży tyle co prior miejski (Bühlmann).
    pub credibility_k: u32,
    /// Stawka wyjściowa, gdy nikt w mieście nie ma jeszcze żadnej historii,
    /// w punktach bazowych sumy ubezpieczenia rocznie. **To nie jest miara ryzyka
    /// dzielnicy** — jest jedna dla całego miasta i znika, gdy tylko pojawią się dane.
    pub prior_rate_bp: u16,
    /// Narzut na koszty i zysk zakładu, w punktach bazowych.
    pub loading_bp: u16,
    /// Opłata stała doliczana do każdej składki miesięcznej, w groszach.
    pub policy_fee_gr: i64,
    /// Okno obserwacji szkodowości w miesiącach (plan §5.5: 60).
    ///
    /// Okno jest **wykładnicze**: każdy miesiąc mnoży statystykę przez
    /// `1 − 1/window_months`, więc średni wiek obserwacji równa się tej liczbie.
    /// Patrz [`super::Insurers::age_one_month`] — tam jest powód.
    pub window_months: u16,
    /// Udział własny jako część sumy ubezpieczenia, w punktach bazowych.
    pub deductible_bp: u16,
    /// Jaka część zapasu zakładu przepada przy zdarzeniu o sile 10 000 bps,
    /// w punktach bazowych. Skala jest **wspólna dla wszystkich ryzyk**: różnicę
    /// robi to, jak często i jak silnie dane zdarzenie zachodzi, a nie mnożnik.
    pub damage_at_full_bp: u16,
    /// Minimalna wartość zapasu, od której zakład w ogóle się ubezpiecza.
    pub min_sum_insured_gr: i64,
    /// Ile miesięcy trwa polisa, zanim wymaga odnowienia.
    pub term_months: u16,
}

impl InsuranceData {
    #[must_use]
    pub fn deductible(&self, sum_insured: Money) -> Money {
        Money(sum_insured.get().max(0) * i64::from(self.deductible_bp) / 10_000)
    }

    #[must_use]
    pub fn policy_fee(&self) -> Money {
        Money(self.policy_fee_gr)
    }

    #[must_use]
    pub fn min_sum_insured(&self) -> Money {
        Money(self.min_sum_insured_gr)
    }
}

/// Wartości, przy których mechanizm istnieje, ale nikt nie ubezpiecza się za darmo.
///
/// Nie zera: rejestr z zerowym priorem i zerową wiarygodnością wystawiałby polisy
/// za nic i wypłacał odszkodowania z niczego — czyli wyglądałby jak działający,
/// a był darmowym pieniądzem. Ta sama zasada, co przy `CorpFinance::default`,
/// tylko odwrócona: tam zera znaczyły „nikt nie upada", tu znaczyłyby „każdy wygrywa".
impl Default for InsuranceData {
    fn default() -> InsuranceData {
        InsuranceData {
            credibility_k: 100,
            prior_rate_bp: 300,
            loading_bp: 2_500,
            policy_fee_gr: 2_000,
            window_months: 60,
            deductible_bp: 1_000,
            damage_at_full_bp: 3_000,
            min_sum_insured_gr: 200_000,
            term_months: 12,
        }
    }
}

#[derive(Deserialize)]
struct Plik {
    schema_version: u32,
    insurance: InsuranceData,
    equity: EquityFile,
}

#[derive(Deserialize)]
struct EquityFile {
    investor_wealth_min_gr: i64,
    investor_stake_bp: u16,
    noise_household_bp: u16,
    noise_firm_bp: u16,
    earnings_noise_bp: u16,
    pe_min: u16,
    pe_max: u16,
    tender_premium_bp: u16,
    payout_bp: u16,
}

/// Błąd wczytania `data/tuning/insurance.ron`.
#[derive(Debug)]
pub enum InsuranceError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    Schema {
        found: u32,
        want: u32,
    },
    /// Wartość, przy której mechanizm przestaje istnieć, a plik nadal wygląda
    /// na poprawny — ta sama klasa błędu, którą `RndTuningError::Zero` łapie w R&D.
    Zero(&'static str),
    Range {
        pole: &'static str,
        min: u32,
        max: u32,
    },
}

impl std::fmt::Display for InsuranceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InsuranceError::Io(e) => write!(f, "insurance.ron: {e}"),
            InsuranceError::Ron(e) => write!(f, "insurance.ron: {e}"),
            InsuranceError::Schema { found, want } => {
                write!(
                    f,
                    "insurance.ron: schema_version {found}, oczekiwano {want}"
                )
            }
            InsuranceError::Zero(p) => write!(
                f,
                "insurance.ron: {p} jest zerem, czyli mechanizm nie istnieje"
            ),
            InsuranceError::Range { pole, min, max } => {
                write!(f, "insurance.ron: {pole} — {max} poniżej {min}")
            }
        }
    }
}

impl std::error::Error for InsuranceError {}

/// Komplet kalibracji podfazy: ubezpieczenia i giełda.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct M10dTuning {
    pub insurance: InsuranceData,
    pub equity: crate::equity::EquityParams,
}

impl M10dTuning {
    /// Wczytuje i waliduje plik kalibracji.
    pub fn load(path: &Path) -> Result<M10dTuning, InsuranceError> {
        let tekst = std::fs::read_to_string(path).map_err(InsuranceError::Io)?;
        let p: Plik = ron::from_str(&tekst).map_err(InsuranceError::Ron)?;
        if p.schema_version != INSURANCE_SCHEMA_VERSION {
            return Err(InsuranceError::Schema {
                found: p.schema_version,
                want: INSURANCE_SCHEMA_VERSION,
            });
        }
        if p.insurance.credibility_k == 0 {
            return Err(InsuranceError::Zero("credibility_k"));
        }
        if p.insurance.window_months == 0 {
            return Err(InsuranceError::Zero("window_months"));
        }
        if p.insurance.term_months == 0 {
            return Err(InsuranceError::Zero("term_months"));
        }
        if p.insurance.damage_at_full_bp == 0 {
            return Err(InsuranceError::Zero("damage_at_full_bp"));
        }
        if p.insurance.deductible_bp >= 10_000 {
            return Err(InsuranceError::Range {
                pole: "deductible_bp",
                min: 0,
                max: u32::from(p.insurance.deductible_bp),
            });
        }
        if p.equity.pe_max < p.equity.pe_min {
            return Err(InsuranceError::Range {
                pole: "pe",
                min: u32::from(p.equity.pe_min),
                max: u32::from(p.equity.pe_max),
            });
        }
        if p.equity.pe_min == 0 {
            return Err(InsuranceError::Zero("pe_min"));
        }
        Ok(M10dTuning {
            insurance: p.insurance,
            equity: crate::equity::EquityParams {
                investor_wealth_min: Money(p.equity.investor_wealth_min_gr),
                investor_stake_bp: p.equity.investor_stake_bp,
                noise_household_bp: p.equity.noise_household_bp,
                noise_firm_bp: p.equity.noise_firm_bp,
                earnings_noise_bp: p.equity.earnings_noise_bp,
                pe_min: p.equity.pe_min,
                pe_max: p.equity.pe_max,
                tender_premium_bp: p.equity.tender_premium_bp,
                payout_bp: p.equity.payout_bp,
            },
        })
    }

    /// Wczytuje z katalogu danych gry.
    pub fn load_default() -> Result<M10dTuning, InsuranceError> {
        M10dTuning::load(&magnat_core::assets::data_path("tuning/insurance.ron"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plik_gry_wczytuje_sie_i_przechodzi_walidacje() {
        let t = M10dTuning::load_default().expect("data/tuning/insurance.ron");
        assert!(t.insurance.credibility_k > 0);
        assert!(t.equity.pe_min <= t.equity.pe_max);
    }

    #[test]
    fn udzial_wlasny_liczy_sie_od_sumy_ubezpieczenia() {
        let d = InsuranceData::default();
        assert_eq!(d.deductible(Money(1_000_000)), Money(100_000));
    }
}
