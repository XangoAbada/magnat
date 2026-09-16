//! Port reguły do świata — **jedyne** wejście ewaluatora (M7c WP6b, M7 §5.8).
//!
//! # Dlaczego jedna metoda, a nie trzydzieści
//!
//! Metryk jest dziś dwadzieścia parę i będzie ich więcej. Trait z metodą na każdą
//! łamałby `I` z SOLID w najbardziej dosłowny sposób: konsument używa dwóch, a musi
//! zaimplementować wszystkie — a przy pierwszej nowej metryce trzeba by dopisać
//! zaślepkę w każdej implementacji. Dlatego metryka jest **daną** (enum), a widok
//! jest jedną funkcją odczytu. Nowa metryka to nowy wariant i jedno ramię po stronie
//! widoku, który akurat umie na nie odpowiedzieć.
//!
//! # Czym ten port nie jest
//!
//! Nie jest dostępem do świata. Implementacja produkcyjna ([`magnat_economy`] w M7c,
//! `FirmView` w M7e) buduje się **z tego, co firma widzi**: własnych ksiąg, własnego
//! zapasu i publicznej tablicy ofert. Wyciek `&World` do ewaluatora łamie test
//! asymetrii informacji z M7 §7.3 natychmiast, bo mutacja ukrytych danych gracza
//! zmieniłaby decyzję konkurenta.
//!
//! `None` z [`PolicyView::metric`] znaczy **„nie wiem"**, a nie zero. Reguła oparta
//! na nieznanej metryce się nie wyzwala — i to jest właściwe zachowanie: sklep, który
//! nie zna ceny konkurenta, nie ma jej jak podbić, a udawanie zera przeceniłoby towar
//! do grosza.

use magnat_core::{GoodId, NeedCategoryId};

use crate::ast::{Metric, Value};

/// Kontekst jednego wykonania polityki: dla którego towaru i w której godzinie doby.
///
/// Osobno od widoku, bo widok jest **zakładem** i żyje przez całe wykonanie, a kontekst
/// zmienia się z każdym towarem na półce — polityka o zakresie kategorii wykonuje się
/// raz na towar i to jest jedyna rzecz, która się między tymi przebiegami różni.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MetricCtx {
    /// Towar, do którego odnosi się `TEN_TOWAR` ([`crate::ast::GoodRef::This`]).
    pub good: Option<GoodId>,
    /// Kategoria potrzeby tego towaru — po niej zakres `Category` sprawdza pokrycie.
    pub category: Option<NeedCategoryId>,
}

impl MetricCtx {
    #[must_use]
    pub const fn for_good(good: GoodId) -> MetricCtx {
        MetricCtx {
            good: Some(good),
            category: None,
        }
    }
}

/// Widok firmy na świat.
pub trait PolicyView {
    /// Odczyt metryki. `None` = „nie wiem", nigdy „zero".
    ///
    /// Jednostka wyniku **musi** zgadzać się z [`Metric::unit`] — to jest kontrakt,
    /// na którym stoi walidator jednostek, i pilnuje go `debug_assert` w ewaluatorze.
    fn metric(&self, m: Metric, ctx: &MetricCtx) -> Option<Value>;

    /// Stawka VAT towaru z kontekstu, w punktach bazowych — przelicznik jawnej
    /// konwersji `brutto(x)` / `netto(x)` ([`crate::ast::Expr::Convert`]).
    ///
    /// Domyślnie zero, bo właścicielem podatków jest M8 (`TaxEngine`, dziś `NoTax`).
    /// Domyślna implementacja jest tu z rozmysłu: widok, który o podatkach nie wie,
    /// ma odpowiadać „bez VAT-u", a nie przestać się kompilować — inaczej każdy
    /// widok testowy musiałby powtarzać to samo zero.
    fn vat_bp(&self, _ctx: &MetricCtx) -> i32 {
        0
    }
}

/// Widok, który nic nie wie — punkt wyjścia testów i zachowanie zakładu bez danych.
///
/// Nie jest zaślepką „na zapas": to jest odpowiedź na pytanie, co robi polityka
/// przypięta do zakładu, który dopiero powstał i nie ma jeszcze ani sprzedaży,
/// ani obrazu konkurencji. Odpowiedź brzmi „nic", i tak ma być.
pub struct BlindView;

impl PolicyView for BlindView {
    fn metric(&self, _m: Metric, _ctx: &MetricCtx) -> Option<Value> {
        None
    }
}
