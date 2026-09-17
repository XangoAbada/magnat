//! `sim/city` — miasto jako aktor: pieniądz publiczny (M8a, PRD §6.8, §10.1).
//!
//! Po tej podfazie miasto ma **budżet, siedem danin i cykl życia należności**.
//! Nie ma jeszcze burmistrza, wyborów, usług publicznych ani sieci przesyłowych —
//! te przychodzą w M8b–M8e. To, co jest, ma za to jedną własność, od której
//! wszystko dalsze zależy: **budżet domyka się do grosza**.
//!
//! Trzy reguły, które trzymają tę własność i których nie wolno obejść:
//!
//! 1. **Pieniądz rusza się wyłącznie przez `Books::transfer`** (M5). Miasto jest
//!    zwykłym posiadaczem konta (`AccountOwner::City`), objętym niezmiennikiem
//!    `MoneySupplyLedger` bez wyjątków. `CityBudget` nie ma pola `cash`.
//! 2. **Kwotę liczy czysta funkcja** z [`calc`], bez dostępu do świata. „Dlaczego
//!    tyle" da się przez to odpowiedzieć bez odtwarzania stanu z chwili naliczenia.
//! 3. **Stan należności zmieniają trzy metody** [`charge::ChargeRegistry`] i nic
//!    poza nimi, więc domknięcie `Σ Assessed = Σ Settled + Σ Overdue + Σ Abated`
//!    jest niezmiennikiem struktury, a nie wnioskiem z testu.
//!
//! Zero floatów w całej fazie (M8a §5.0): stawki w punktach bazowych, kwoty
//! w groszach, zaokrąglenie jawne i jedno.
#![forbid(unsafe_code)]

pub mod assess;
pub mod budget;
pub mod calc;
pub mod charge;
pub mod city;
pub mod code;
pub mod engine;
pub mod settle;
pub mod systems;
pub mod world;

pub use budget::{close_month, close_year, BudgetMonth, BudgetPolicy, CityBudget, MunicipalBond};
pub use calc::{
    cit_due, duty_due, excise_due, late_interest, license_fee, pit_annual, pit_withheld,
    property_tax_month, property_tax_year, vat_add_to_net, vat_from_gross,
};
pub use charge::{
    ChargeRegistry, ChargeState, ChargeTotals, FiscalPeriod, TaxCharge, TaxChargeId, TaxPayer,
};
pub use city::{CadastreEntry, City};
pub use code::{TaxCode, TaxCodeError, VatTable, TAX_SCHEMA_VERSION};
pub use engine::{CityTaxEngine, Withholding};
pub use settle::{abate_bankrupt, age_overdue, settle_due, SettleReport};
pub use systems::{register_city, CitySystem};
pub use world::licenses_from_catalog;
