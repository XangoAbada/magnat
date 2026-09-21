//! `magnat-economy` — gospodarka detaliczna (M5).
//!
//! Crate stoi na jednym zdaniu z PRD §6.1: **nie istnieje globalna cena rynkowa**.
//! Cenę niesie wyłącznie oferta konkretnego sklepu, a wszystko, co wygląda na „cenę
//! mleka w mieście", jest agregatem po ofertach ([`price_stats`]). Drugie zdanie jest
//! z §6.5: pieniądz nie powstaje i nie znika poza jawnymi kanałami, a każdą jego
//! zmianę widać w [`Books`].
//!
//! Co zawiera **M5a**:
//! - §5.1 — [`Books`], konta, podwójny zapis, ewidencja podaży pieniądza,
//! - §5.2 — [`Offer`] w arenie (`K-16`) i [`OfferIndex`] z zapytaniem promieniowym.
//!
//! Co dokłada **M5b**:
//! - §5.3 — sklep: zaplecze, półka, asortyment ([`Shop`], [`Shelf`]; od WP11 towar
//!   leży w magazynie M6, a nie w liniach zapasu),
//! - §5.4 — funkcja użyteczności zakupu i wybór oferty ([`utility_of_offer`],
//!   [`choose_offer`], [`purchase_threshold`]) oraz zapis utraconych sprzedaży,
//! - §5.5 — rozliczanie transakcji ([`MarketSystem`], [`PurchaseIntent`]),
//! - §5.7 — punkt wymiany z M6: [`Wholesale`] i [`ExternalSupplier`].
//!
//! Co dokłada **M5c**:
//! - §5.6 — polityki cenowe AI i gracza ([`PricePolicy`], [`reprice`], obserwacja
//!   konkurencji z opóźnieniem 1–7 dni, eksperymenty cenowe),
//! - §5.8 — księgowość zakładu ([`Ledger`], [`post`], [`income_statement`],
//!   [`balance_sheet`], [`cash_flow`]) i hak podatkowy [`TaxEngine`] (`K-7`),
//! - [`kernel`] — rdzeń liczbowy (D20), który M10 zawoła tym samym kodem co mezo.
//!
//! Co dokłada **M5d**:
//! - §5.9 — budżety gospodarstw ([`HouseholdBudget`], [`plan_budget`]),
//! - §5.10 — banki, kredyt, CPI i stopa bazowa ([`Loan`], [`assess_credit`], [`CpiTracker`]).
//!
//! Co dokłada **M5e**:
//! - §5.12 — [`ShopPanelSnapshot`]: jedyne wejście interfejsu do gospodarki.
//!
//! Co dokłada **M7b** (autor M7, właściciel crate'a M5 — `D2`):
//! - §5.5 — rynek pracy ([`labor`]): oferty i aplikacje na tej samej maszynerii aren
//!   co oferty detaliczne, licytacja płac z indeksem niedoboru, headhunting
//!   i kadry. **Nigdzie w tym module nie ma tabeli płac** — pensja jest wynikiem
//!   licytacji o człowieka.
//!
//! Czego tu **nie ma** i gdzie to jest: rynek B2B i partie towaru należą do M6,
//! podatki do M8, pełne panele gracza do M9.

#![forbid(unsafe_code)]

pub mod ai_run;
pub mod board;
pub mod books;
pub mod budget;
pub mod chain_supply;
pub mod choice;
pub mod corpfin;
pub mod cpi;
pub mod credit;
pub mod data;
pub mod equity;
pub mod firmlife;
pub mod inherit;
pub mod insurance;
pub mod kernel;
pub mod labor;
pub mod ledger;
pub mod manager_exec;
pub mod market;
pub mod mobility;
pub mod offer;
pub mod owner_ops;
pub mod panel;
pub mod payroll;
pub mod policy_run;
pub mod pricing;
pub mod relations;
pub mod rnd;
pub mod shop;
pub mod supply;
pub mod systems;
pub mod tax;

pub use ai_run::{register_firm_ai, FirmAiDay};
pub use board::{ObservedPrice, PublicMarketBoard, WINDOW_DAYS};
pub use books::{
    Account, AccountId, AccountKind, AccountOwner, Books, ChargeKind, ExternalInvestorId, LoanId,
    MoneySupplyLedger, ProgramId, SupplierRef, Transaction, TxError, TxId, TxJournal, TxKind,
    TxMemo,
};
pub use budget::{
    budget_ref_for_need, expected_purchases, plan_budget, BudgetPlan, Envelope, HouseholdBudget,
    HouseholdProfile,
};
pub use choice::{
    budget_ref_for, choose_offer, cost_term, days_bought, dominant_term, offer_noise,
    purchase_threshold, rating_of, status_fit, utility_of_offer, wanted_qty, weights_for,
    BuyerState, Candidate, Choice,
};
pub use cpi::{CpiBasket, CpiTracker, IndexBp, INDEX_BASE};
pub use credit::{
    assess_credit, build_schedule, monthly_rate_bp, update_base_rate, BaseRate, CreditDecision,
    Installment, Loan, LoanApplication, LoanBook,
};
pub use data::{
    BankParams, BaseRateRule, BudgetParams, CpiSpec, CreditScoring, EconomyData, EconomyDataError,
    HouseholdFixedCosts, LoanProduct, LoanProducts, PricingParams, Range, RetailGood, RetailTable,
    ShopCosts, SpoilageStep, ThresholdSpec, UtilityWeights, ECONOMY_SCHEMA_VERSION,
    HOUSEHOLD_KIND_COUNT,
};
pub use kernel::{
    annuity_payment, apply_bp, clamp_to_margin, cost_from_price, interest_accrual, ledger_post,
    monthly_interest, next_price, next_price_full, take_cogs, throughput, wage_bid, LedgerError,
    PriceBreakdown, PriceInput, StockValue, BP,
};
pub use labor::{
    register_labor, Application, JobIndex, JobOffer, JobOfferId, LaborDay, LaborHandle,
    LaborMarket, LaborMarketStats, LaborSystem, PersonFacts, RoleStats, Seeker, SkillReq,
    Workforce,
};
pub use ledger::{
    balance_sheet, cash_flow, close_period, income_statement, post, BalanceSheet, CashFlow,
    IncomeStatement, JournalEntry, Ledger, LedgerAccount, PeriodClose, LEDGER_ACCOUNT_COUNT,
};
pub use manager_exec::{
    ManagerCurve, ManagerExecution, PolicyTuning, PolicyTuningError, POLICY_TUNING_SCHEMA_VERSION,
};
pub use market::{
    Bank, HouseholdMonth, HouseholdMonthReport, Market, MarketStats, PurchaseIntent, ShelfSnapshot,
    ShopSeed, SiteEnforcementRow,
};
pub use offer::{
    price_stats, query_offers, CategoryId, Offer, OfferId, OfferIndex, PriceBasis, PriceStats,
};
pub use panel::{
    BalanceSample, CompetitorRow, CustomerStats, FinanceSummary, LostSalesView, PriceDist,
    ShelfRow, ShopPanelSnapshot,
};
pub use policy_run::{
    preset_for, DryDay, DryRun, GoodFacts, PolicyAlert, PolicyDay, PolicyOutcome, PolicyTrace,
    TraceDay, MAX_COVER, POLICY_INBOX, TRACE_DAYS,
};
pub use pricing::{
    preview_price, reprice, CompetitorEntry, CompetitorRef, CompetitorSnapshot, FirmPricing,
    ObservedElasticity, PriceController, PriceExperiment, PricePolicy, PricingCtx,
};
pub use relations::{
    register_relations, Cartel, CartelId, Cartels, RelationsDay, RelationsSystem, RelationsTuning,
    Union, UnionId, UnionState, Unions,
};
pub use shop::{
    AssortmentPolicy, B2bTax, LostSale, LostSaleHistogram, LostSaleTracking, ReorderPolicy, Shelf,
    ShelfLine, Shop, ShopCustomers, ShopInventory, ShopLostSales, TaxAccrual, LOST_SALE_RING,
};
pub use supply::{
    line_total, Delivery, GoodSpec, GoodTable, OrderId, PurchaseQuote, SupplyError, Wholesale,
    PRICE_UNIT,
};
pub use systems::{
    pay_incomes, register_books, register_economy, settle_household_month, settle_transactions,
    MarketSystem,
};
pub use tax::{NoTax, TaxEngine};
