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
//! - §5.3 — sklep: zaplecze, półka, asortyment ([`Shop`], [`Shelf`], [`StockLine`]),
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
//! Czego tu **nie ma** i gdzie to jest: budżety gospodarstw, banki i inflacja — M5d;
//! panel i balansator — M5e. Rynek B2B i partie towaru należą do M6, podatki do M8.

#![forbid(unsafe_code)]

pub mod books;
pub mod choice;
pub mod data;
pub mod kernel;
pub mod ledger;
pub mod market;
pub mod offer;
pub mod pricing;
pub mod shop;
pub mod supply;
pub mod systems;
pub mod tax;

pub use books::{
    Account, AccountId, AccountKind, AccountOwner, Books, ChargeKind, ExternalInvestorId, LoanId,
    MoneySupplyLedger, ProgramId, SupplierRef, Transaction, TxError, TxId, TxJournal, TxKind,
    TxMemo,
};
pub use choice::{
    budget_ref_for, choose_offer, cost_term, days_bought, dominant_term, purchase_threshold,
    offer_noise, rating_of, status_fit, utility_of_offer, wanted_qty, weights_for, BuyerState,
    Candidate, Choice,
};
pub use data::{EconomyData, EconomyDataError, PricingParams, Range, RetailGood, RetailTable,
    ShopCosts, SpoilageStep, ThresholdSpec, UtilityWeights, ECONOMY_SCHEMA_VERSION};
pub use kernel::{
    clamp_to_margin, ledger_post, next_price, next_price_full, take_cogs, LedgerError,
    PriceBreakdown, PriceInput, StockValue, BP,
};
pub use ledger::{
    balance_sheet, cash_flow, close_period, income_statement, post, BalanceSheet, CashFlow,
    IncomeStatement, JournalEntry, Ledger, LedgerAccount, PeriodClose, LEDGER_ACCOUNT_COUNT,
};
pub use pricing::{
    preview_price, reprice, CompetitorEntry, CompetitorRef, CompetitorSnapshot, FirmPricing,
    ObservedElasticity, PriceController, PriceExperiment, PricePolicy, PricingCtx,
};
pub use tax::{NoTax, TaxEngine};
pub use market::{Market, MarketStats, PurchaseIntent, ShopSeed};
pub use offer::{
    price_stats, query_offers, CategoryId, Offer, OfferId, OfferIndex, PriceBasis, PriceStats,
};
pub use shop::{
    take_units, AssortmentPolicy, LostSale, LostSaleHistogram, LostSaleTracking, ReorderPolicy,
    Shelf, ShelfLine, Shop, ShopInventory, ShopLostSales, StockLine, LOST_SALE_RING,
};
pub use supply::{
    line_total, Delivery, ExternalSupplier, GoodSpec, GoodTable, OrderId, PurchaseQuote,
    SupplyError, Wholesale, PRICE_UNIT,
};
pub use systems::{
    pay_incomes, register_books, register_economy, settle_transactions, MarketSystem,
};
