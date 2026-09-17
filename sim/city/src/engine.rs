//! `CityTaxEngine` — jedyna prawdziwa implementacja haka, który M5 zostawił
//! (`sim/economy::TaxEngine`, `K-7`).
//!
//! Hak robi dwie rzeczy i obie są przeliczeniem, nie decyzją: zamienia cenę netto,
//! w której liczy polityka cenowa sklepu, na cenę brutto, którą płaci mieszkaniec,
//! i z powrotem. Gdyby tego kroku nie było, podniesienie stawki VAT podniosłoby
//! marże wszystkich sklepów o tę stawkę bez zmiany jakiejkolwiek polityki cenowej.
//!
//! Trzecią rzeczą jest **potrącenie u źródła** przy wypłacie. Stoi tutaj, a nie
//! w systemie miasta, z jednego powodu: wypłatę robi `pay_incomes` w `sim/economy`,
//! a gospodarstwo ma dostać **netto**. Gdyby potrącenie działo się później, planer
//! budżetu domowego rozdzieliłby koperty z dochodu, którego nikt nigdy nie dostał,
//! i pierwszy miesiąc z podatkiem zamieniłby połowę miasta w wnioskodawców kredytowych.
//!
//! Silnik jest **taniutki w odczycie i drogi tylko przy wypłacie**: stawki leżą
//! w tablicy za `Arc`, więc wycena ceny nie zakłada zamka; zamek jest wyłącznie
//! na liczniku potrąceń, dotykanym raz na gospodarstwo na miesiąc.

use std::sync::{Arc, Mutex};

use magnat_core::{GoodId, HashState, Money, StateHasher};
use magnat_economy::TaxEngine;

use crate::calc::{pit_withheld, vat_add_to_net, vat_from_gross};
use crate::code::{TaxCode, VatTable};

/// Licznik zaliczek PIT: ile każde gospodarstwo zarobiło i ile mu potrącono
/// od początku roku podatkowego.
///
/// Narastająco, bo tylko tak suma dwunastu zaliczek równa się podatkowi rocznemu
/// co do grosza (`R5`). Indeksem jest indeks encji gospodarstwa — ten sam, którym
/// posługuje się `Population::households()`.
#[derive(Default)]
struct WithholdingInner {
    ytd_gross: Vec<Money>,
    ytd_withheld: Vec<Money>,
    /// Która to wypłata w tym roku podatkowym. Bez tego zaliczka nie umiałaby
    /// rozłożyć kwoty wolnej na miesiące i pierwsze pół roku byłoby bez podatku.
    ytd_months: Vec<u8>,
    /// Potrącone i jeszcze nieprzekazane miastu. Pieniądz stoi w tej chwili na
    /// koncie pracodawcy (w M8a: „reszta świata"), bo wypłata wyszła pomniejszona.
    pending: Money,
    pending_base: Money,
}

/// Uchwyt do licznika zaliczek. `Clone` dzieli stan — jedna kopia siedzi w silniku
/// podatkowym wewnątrz rynku, druga w zasobie miasta, który ją haszuje.
///
/// Ta sama konstrukcja co `Market` i `ChainHandle`, i z tego samego powodu:
/// `TaxEngine::withhold` dostaje `&self`, bo tak wygląda hak po stronie M5.
#[derive(Clone, Default)]
pub struct Withholding(Arc<Mutex<WithholdingInner>>);

impl Withholding {
    #[must_use]
    pub fn new() -> Withholding {
        Withholding::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, WithholdingInner> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Nalicza zaliczkę od wypłaty i zapisuje ją w liczniku. Zwraca kwotę potrąconą.
    pub fn withhold(&self, household: u32, gross: Money, code: &TaxCode) -> Money {
        if gross.get() <= 0 {
            return Money::ZERO;
        }
        let mut w = self.lock();
        let i = household as usize;
        if w.ytd_gross.len() <= i {
            w.ytd_gross.resize(i + 1, Money::ZERO);
            w.ytd_withheld.resize(i + 1, Money::ZERO);
            w.ytd_months.resize(i + 1, 0);
        }
        let miesiac = w.ytd_months[i].saturating_add(1);
        let zaliczka = pit_withheld(gross, w.ytd_gross[i], w.ytd_withheld[i], miesiac, code);
        // Zaliczka ujemna znaczy nadpłatę z poprzednich miesięcy. Oddajemy ją,
        // ale nie więcej niż wynosi bieżąca wypłata — inaczej „wypłata" byłaby
        // przelewem od miasta do gospodarstwa, a takiego kanału tu nie ma.
        let zaliczka = Money(zaliczka.get().min(gross.get()).max(-gross.get()));
        w.ytd_gross[i] = Money(w.ytd_gross[i].get() + gross.get());
        w.ytd_withheld[i] = Money(w.ytd_withheld[i].get() + zaliczka.get());
        w.ytd_months[i] = miesiac.min(12);
        w.pending = Money(w.pending.get() + zaliczka.get());
        w.pending_base = Money(w.pending_base.get() + gross.get());
        zaliczka
    }

    /// Wyjmuje to, co potrącono od ostatniego odebrania: `(podstawa, podatek)`.
    /// Wołane raz w miesiącu przez system miasta — to jest moment, w którym
    /// zaliczka staje się należnością i idzie do budżetu.
    pub fn take_pending(&self) -> (Money, Money) {
        let mut w = self.lock();
        let r = (w.pending_base, w.pending);
        w.pending = Money::ZERO;
        w.pending_base = Money::ZERO;
        r
    }

    /// Zerowanie liczników na nowy rok podatkowy. Bez tego drugi rok liczyłby
    /// zaliczki od dochodu dwuletniego i każdy wpadłby w drugi próg.
    pub fn reset_year(&self) {
        let mut w = self.lock();
        w.ytd_gross.clear();
        w.ytd_withheld.clear();
        w.ytd_months.clear();
    }

    /// Dochód i podatek gospodarstwa od początku roku — treść karty inspekcji.
    #[must_use]
    pub fn ytd_of(&self, household: u32) -> (Money, Money) {
        let w = self.lock();
        let i = household as usize;
        (
            w.ytd_gross.get(i).copied().unwrap_or(Money::ZERO),
            w.ytd_withheld.get(i).copied().unwrap_or(Money::ZERO),
        )
    }
}

impl HashState for Withholding {
    fn hash_state(&self, h: &mut StateHasher) {
        let w = self.lock();
        h.write_u64(w.ytd_gross.len() as u64);
        for m in &w.ytd_gross {
            h.write_i64(m.get());
        }
        for m in &w.ytd_withheld {
            h.write_i64(m.get());
        }
        for m in &w.ytd_months {
            h.write_u8(*m);
        }
        h.write_i64(w.pending.get());
        h.write_i64(w.pending_base.get());
    }
}

/// Silnik podatkowy miasta wstawiany do rynku w miejsce `NoTax`.
#[derive(Clone)]
pub struct CityTaxEngine {
    rates: Arc<VatTable>,
    code: Arc<TaxCode>,
    withholding: Withholding,
}

impl CityTaxEngine {
    #[must_use]
    pub fn new(
        rates: Arc<VatTable>,
        code: Arc<TaxCode>,
        withholding: Withholding,
    ) -> CityTaxEngine {
        CityTaxEngine {
            rates,
            code,
            withholding,
        }
    }

    #[must_use]
    pub fn rates(&self) -> &VatTable {
        &self.rates
    }
}

impl TaxEngine for CityTaxEngine {
    fn gross_from_net(&self, good: GoodId, net: Money) -> Money {
        vat_add_to_net(net, self.rates.vat_bp(good))
    }

    fn net_from_gross(&self, good: GoodId, gross: Money) -> Money {
        Money(gross.get() - vat_from_gross(gross, self.rates.vat_bp(good)).get())
    }

    fn vat_on_gross(&self, good: GoodId, gross: Money) -> Money {
        vat_from_gross(gross, self.rates.vat_bp(good))
    }

    fn withhold(&self, household: u32, gross: Money) -> Money {
        self.withholding.withhold(household, gross, &self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::PitBracket;

    fn kodeks() -> TaxCode {
        TaxCode {
            schema_version: crate::code::TAX_SCHEMA_VERSION,
            cit_bp: 1900,
            loss_carry_years: 5,
            pit_free_allowance: 3_000_000,
            pit_brackets: vec![
                PitBracket {
                    upper: 12_000_000,
                    bp: 1200,
                },
                PitBracket { upper: 0, bp: 3200 },
            ],
            vat_classes: Vec::new(),
            property_bp_per_year: 100,
            excise: Vec::new(),
            licenses: Vec::new(),
            vat_due_day: 20,
            pit_due_day: 20,
            property_due_day: 15,
            excise_due_day: 25,
            cit_due_month: 3,
            late_interest_bp_per_year: 1450,
            time_bar_days: 1800,
            effective_from: magnat_core::Tick(0),
        }
    }

    #[test]
    fn potracenia_sumuja_sie_do_podatku_rocznego() {
        let c = kodeks();
        let w = Withholding::new();
        for _ in 0..12 {
            w.withhold(7, Money(600_000), &c);
        }
        let (brutto, pobrane) = w.ytd_of(7);
        assert_eq!(brutto, Money(7_200_000));
        assert_eq!(
            pobrane,
            crate::calc::pit_annual(brutto, Money(c.pit_free_allowance), &c.pit_brackets)
        );
        // Wyjęcie zeruje kolejkę, ale nie licznik roczny.
        let (podstawa, podatek) = w.take_pending();
        assert_eq!(podstawa, brutto);
        assert_eq!(podatek, pobrane);
        assert_eq!(w.take_pending(), (Money::ZERO, Money::ZERO));
        assert_eq!(w.ytd_of(7), (brutto, pobrane));
        // Nowy rok zaczyna się od zera, inaczej wszyscy wpadliby w drugi próg.
        w.reset_year();
        assert_eq!(w.ytd_of(7), (Money::ZERO, Money::ZERO));
    }

    #[test]
    fn zaliczka_nigdy_nie_przekracza_wyplaty() {
        let c = kodeks();
        let w = Withholding::new();
        // Rok z jedną olbrzymią wypłatą i jedenastoma groszowymi: korekta w dół
        // nie może zabrać więcej, niż akurat wypłacono.
        w.withhold(1, Money(30_000_000), &c);
        for _ in 0..11 {
            let z = w.withhold(1, Money(100), &c);
            assert!(z.get().abs() <= 100, "zaliczka {z:?} większa od wypłaty");
        }
    }
}
