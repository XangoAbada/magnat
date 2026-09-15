//! Hak podatkowy (`K-7`, kontrakt §6 dla M8).
//!
//! W M5 VAT jest zerowy i całe to miejsce mnoży przez jeden. Istnieje mimo to
//! od pierwszego dnia, bo `K-7` wymaga, żeby **cała arytmetyka ceny działa się
//! netto**, a przeliczenie na `Offer.unit_price` w podstawie `GrossRetail` było
//! ostatnim krokiem. Gdyby tego kroku nie było, M8 podniósłby stawkę VAT-u
//! i marże wszystkich sklepów skoczyłyby o tę stawkę bez zmiany jakiejkolwiek
//! polityki cenowej — a błąd wyszedłby w balansatorze, nie w przeglądzie kodu.
//!
//! To jeden z trzech jawnie zaplanowanych punktów wymiany fazy (obok `Wholesale`
//! i `TravelOracle`), więc trait z jedną implementacją jest tu decyzją, a nie
//! zapasem: drugi konsument jest znany z nazwy i z numeru fazy.

use magnat_core::{GoodId, Money};

/// Przeliczenie między podstawą netto (w której liczy się cena) a brutto
/// (w której płaci kupujący, `K-7`).
///
/// M8 dostarcza `CityTaxEngine` ze stawkami per towar; sygnatura się nie zmienia.
pub trait TaxEngine: Send + Sync {
    /// Cena, którą zobaczy kupujący, z ceny liczonej przez politykę cenową.
    fn gross_from_net(&self, good: GoodId, net: Money) -> Money;

    /// Odwrotność — sprowadza obserwowaną cenę konkurenta do podstawy netto,
    /// zanim wejdzie do `adj_comp`.
    fn net_from_gross(&self, good: GoodId, gross: Money) -> Money;
}

/// Jedyna implementacja w M5: mnożnik 1 w obie strony.
#[derive(Clone, Copy, Default, Debug)]
pub struct NoTax;

impl TaxEngine for NoTax {
    fn gross_from_net(&self, _good: GoodId, net: Money) -> Money {
        net
    }

    fn net_from_gross(&self, _good: GoodId, gross: Money) -> Money {
        gross
    }
}
