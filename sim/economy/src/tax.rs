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

use magnat_core::{GoodId, Money, UtilityService};

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

    /// Część podatkowa kwoty brutto — to, co wpisuje się do `TxMemo.tax`
    /// i co sklep jest winien miastu za tę transakcję.
    ///
    /// Domyślnie wyprowadzona z [`TaxEngine::net_from_gross`], bo to ta sama liczba
    /// widziana z drugiej strony; implementacja nadpisuje ją tylko wtedy, gdy umie
    /// policzyć ją taniej.
    fn vat_on_gross(&self, good: GoodId, gross: Money) -> Money {
        Money(gross.get() - self.net_from_gross(good, gross).get())
    }

    /// Akcyza od masy wyrobu obłożonego. **Stawka kwotowa**, nie procentowa —
    /// i to jest cała różnica między akcyzą a VAT-em: podwyżka uderza w tani wyrób
    /// mocniej niż w drogi, a cenę podnosi dopiero polityka marżowa sklepu.
    ///
    /// Hak M8, domyślnie zero.
    fn excise_on(&self, _good: GoodId, _mass: magnat_core::Mass) -> Money {
        Money::ZERO
    }

    /// Akcyza od energii na rachunku za media. **Stawka kwotowa za jednostkę
    /// rozliczeniową** (grosze za kWh), naliczana w chwili wystawienia faktury.
    ///
    /// `units_milli` to rozliczone zużycie w **tysięcznych** jednostki: dzielimy
    /// na końcu, tak samo jak przy taryfie, żeby ułamek kilowatogodziny nie
    /// znikał z podstawy przy każdym rachunku.
    ///
    /// Hak M8b, domyślnie zero. Stoi osobno od [`TaxEngine::excise_on`], bo tamta
    /// liczy od **masy wyrobu**, a prąd masy nie ma — i to nie jest formalność:
    /// `ExciseClass::Energy` z PRD §6.8 jest jedyną z czterech klas akcyzowych,
    /// która ma w tej grze codzienny wolumen, a przechodzi wyłącznie przez rachunek
    /// za media. Wpięcie jej w `excise_on` wymagałoby udawanej masy kilowatogodziny.
    ///
    /// Zgodnie z `K-57`: faza dokładająca daninę naliczaną na zdarzeniu gospodarki
    /// dokłada **metodę z ciałem domyślnym**, a nie drugi trait.
    fn excise_on_utility(&self, _service: UtilityService, _units_milli: i64) -> Money {
        Money::ZERO
    }

    /// Potrącenie u źródła przy wypłacie, po indeksie encji gospodarstwa.
    ///
    /// **Hak M8, domyślnie zero.** Stoi w tym samym miejscu co przeliczenie ceny
    /// i z tego samego powodu: gospodarstwo ma dostać netto **w chwili wypłaty**,
    /// bo z tego, co dostanie, zaraz planuje koperty (`plan_budget`). Potrącenie
    /// doliczone później znaczyłoby, że planer dzieli dochód, którego nie ma.
    ///
    /// Zwrócona kwota może być **ujemna** — to nadpłata z poprzednich miesięcy,
    /// oddawana razem z wypłatą. Wołający dodaje ją do kwoty netto, a nie przycina
    /// do zera; przycięcie rozjechałoby sumę zaliczek z podatkiem rocznym.
    fn withhold(&self, _household_index: u32, _gross: Money) -> Money {
        Money::ZERO
    }
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
