//! Księgowość zakładu: plan kont, dziennik, raporty (M5c §5.8, PRD §6.9).
//!
//! # Jedna konwencja znaku
//!
//! **Kwota dodatnia to Wn (debet), ujemna to Ma (kredyt)** — dla każdego konta tak
//! samo. Konto pasywne z saldem Ma ma więc saldo ujemne, a bilans zamyka się jako
//! `Σ wszystkich sald == 0`. Alternatywa (znak zależny od strony konta) wymagałaby
//! tablicy stron i dawałaby drugie miejsce, w którym da się pomylić kierunek.
//! Raporty odwracają znak tam, gdzie człowiek spodziewa się liczby dodatniej
//! (przychód, kapitał) — i to jest jedyne miejsce, w którym znak się odwraca.
//!
//! # Skąd biorą się liczby w raporcie
//!
//! Z sald, a salda wyłącznie z zapisów w dzienniku — [`post`] jest jedynym wejściem,
//! a [`crate::kernel::ledger_post`] odrzuca każdy zapis niezbilansowany (P4, tolerancja
//! 0 gr). Nie ma licznika obok: `Shop.revenue` z M5b zostaje jako szybki podgląd
//! scenariusza, ale to `Revenue` z tej księgi jest liczbą, którą pokazuje panel.
//!
//! # Okno dziennika
//!
//! Zapisy lecą do **pierścienia**, nie do rosnącej listy: sklep metropolii księguje
//! setki sprzedaży na dobę, a pełna historia ×2000 sklepów byłaby gigabajtami.
//! Pierścień prowadzą wyłącznie zakłady **śledzone** ([`crate::LostSaleTracking`]),
//! bo tylko one mają odbiorcę — to ta sama zasada i ta sama flaga, którą M5b
//! zastosował do utraconych sprzedaży. Historia dłuższa niż okno mieszka
//! w miesięcznych domknięciach ([`PeriodClose`]), które są **wynikiem** księgowania,
//! a nie zapisem obok niego.
//!
//! `ponytail:` sufit nazwany — starsze zapisy nie idą na dysk. Ścieżka wyjścia jest
//! w `engine/io` i wchodzi razem z zapisem gry w M12; do tego czasu okno i domknięcia
//! wystarczają na rok gry.

use magnat_core::{DecisionReason, FirmId, HashState, Money, SiteId, StateHasher, Tick};

use crate::books::TxId;
use crate::kernel::{ledger_post, LedgerError};

/// Ile zapisów trzyma pierścień śledzonego zakładu. Tyle samo, co dziennik
/// transakcji w `Books` — okno ma starczyć na dobę handlu w podglądzie panelu.
pub const JOURNAL_WINDOW: usize = 4_096;

/// Maksymalna liczba linii w jednym zapisie. Cztery starczają każdemu zdarzeniu
/// z §5.8 (sprzedaż to Wn Cash / Ma Revenue / Wn Cogs / Ma Inventory); domknięcie
/// miesiąca idzie **wieloma zapisami po dwie linie**, każdy zbilansowany osobno,
/// zamiast jednym wielolinijkowym. Dzięki temu `JournalEntry` jest `Copy` i nie
/// alokuje — a alokacja na każdą sprzedaż to 450 tys. alokacji na dobę.
pub const MAX_LINES: usize = 4;

// ── plan kont ────────────────────────────────────────────────────────────────────

/// Plan kont zakładu (§5.8). Kolejność jest kontraktem: `as_index()` indeksuje
/// tablicę sald, która wchodzi do hasha stanu i do zapisu gry.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u8)]
pub enum LedgerAccount {
    // ── aktywa ──
    Cash,
    BankCurrent,
    InventoryGoods,
    FixedAssets,
    /// Umorzenie — konto korygujące aktywa, więc saldo ma po stronie Ma (ujemne).
    AccumDepreciation,
    // ── pasywa ── rozbite na `*Payable` na wniosek M8: zobowiązanie ma mieć
    // wierzyciela i termin, a nie jeden worek „zobowiązania".
    /// Dostawcy. W M5 dostawa zewnętrzna jest **płatna z góry**, więc to konto
    /// chodzi przejściowo na saldzie Wn (zaliczka). M6 wnosi terminy płatności
    /// i saldo staje się tym, czym nazwa obiecuje.
    TradePayable,
    /// VAT i daniny z `ChargeRegistry` (M8). W M5 zawsze 0.
    TaxPayable,
    /// Naliczone, niewypłacone — hak M7.
    WagePayable,
    LoansShort,
    LoansLong,
    Equity,
    RetainedEarnings,
    // ── rachunek wyników ──
    Revenue,
    Cogs,
    WagesExpense,
    RentExpense,
    UtilitiesExpense,
    DepreciationExpense,
    InterestExpense,
    WriteOffExpense,
    /// HAK M8 — w M5 zawsze 0.
    TaxExpense,
}

impl LedgerAccount {
    pub const ALL: &'static [LedgerAccount] = &[
        LedgerAccount::Cash,
        LedgerAccount::BankCurrent,
        LedgerAccount::InventoryGoods,
        LedgerAccount::FixedAssets,
        LedgerAccount::AccumDepreciation,
        LedgerAccount::TradePayable,
        LedgerAccount::TaxPayable,
        LedgerAccount::WagePayable,
        LedgerAccount::LoansShort,
        LedgerAccount::LoansLong,
        LedgerAccount::Equity,
        LedgerAccount::RetainedEarnings,
        LedgerAccount::Revenue,
        LedgerAccount::Cogs,
        LedgerAccount::WagesExpense,
        LedgerAccount::RentExpense,
        LedgerAccount::UtilitiesExpense,
        LedgerAccount::DepreciationExpense,
        LedgerAccount::InterestExpense,
        LedgerAccount::WriteOffExpense,
        LedgerAccount::TaxExpense,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            LedgerAccount::Cash => "Cash",
            LedgerAccount::BankCurrent => "BankCurrent",
            LedgerAccount::InventoryGoods => "InventoryGoods",
            LedgerAccount::FixedAssets => "FixedAssets",
            LedgerAccount::AccumDepreciation => "AccumDepreciation",
            LedgerAccount::TradePayable => "TradePayable",
            LedgerAccount::TaxPayable => "TaxPayable",
            LedgerAccount::WagePayable => "WagePayable",
            LedgerAccount::LoansShort => "LoansShort",
            LedgerAccount::LoansLong => "LoansLong",
            LedgerAccount::Equity => "Equity",
            LedgerAccount::RetainedEarnings => "RetainedEarnings",
            LedgerAccount::Revenue => "Revenue",
            LedgerAccount::Cogs => "Cogs",
            LedgerAccount::WagesExpense => "WagesExpense",
            LedgerAccount::RentExpense => "RentExpense",
            LedgerAccount::UtilitiesExpense => "UtilitiesExpense",
            LedgerAccount::DepreciationExpense => "DepreciationExpense",
            LedgerAccount::InterestExpense => "InterestExpense",
            LedgerAccount::WriteOffExpense => "WriteOffExpense",
            LedgerAccount::TaxExpense => "TaxExpense",
        }
    }

    /// Konta rachunku wyników — te, które domyka koniec miesiąca.
    #[must_use]
    pub const fn is_profit_and_loss(self) -> bool {
        matches!(
            self,
            LedgerAccount::Revenue
                | LedgerAccount::Cogs
                | LedgerAccount::WagesExpense
                | LedgerAccount::RentExpense
                | LedgerAccount::UtilitiesExpense
                | LedgerAccount::DepreciationExpense
                | LedgerAccount::InterestExpense
                | LedgerAccount::WriteOffExpense
                | LedgerAccount::TaxExpense
        )
    }

    /// Konto pieniężne — punkt zaczepienia przepływów metodą bezpośrednią.
    #[must_use]
    pub const fn is_cash(self) -> bool {
        matches!(self, LedgerAccount::Cash | LedgerAccount::BankCurrent)
    }
}

pub const LEDGER_ACCOUNT_COUNT: usize = LedgerAccount::ALL.len();

// ── zapis ────────────────────────────────────────────────────────────────────────

/// Jeden zapis księgowy. `Σ lines == 0` jest sprawdzane przy nanoszeniu, nie tu —
/// struktura ma być tania w budowie, a walidacja ma mieć jedno miejsce.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JournalEntry {
    pub tick: Tick,
    /// Powiązanie z dziennikiem pieniężnym `Books`, jeśli zapis miał przelew.
    pub tx: Option<TxId>,
    lines: [(LedgerAccount, Money); MAX_LINES],
    n: u8,
    pub reason: DecisionReason,
}

impl JournalEntry {
    /// Buduje zapis. Panika przy pustej liście i przy przekroczeniu [`MAX_LINES`] —
    /// oba są błędem wołającego, nie stanem danych.
    #[must_use]
    pub fn new(tick: Tick, reason: DecisionReason, lines: &[(LedgerAccount, Money)]) -> JournalEntry {
        assert!(
            !lines.is_empty() && lines.len() <= MAX_LINES,
            "zapis księgowy ma 1..={MAX_LINES} linii, dostał {}",
            lines.len()
        );
        let mut buf = [(LedgerAccount::Cash, Money::ZERO); MAX_LINES];
        buf[..lines.len()].copy_from_slice(lines);
        JournalEntry {
            tick,
            tx: None,
            lines: buf,
            n: lines.len() as u8,
            reason,
        }
    }

    #[must_use]
    pub fn with_tx(mut self, tx: TxId) -> JournalEntry {
        self.tx = Some(tx);
        self
    }

    #[must_use]
    pub fn lines(&self) -> &[(LedgerAccount, Money)] {
        &self.lines[..self.n as usize]
    }
}

/// Miesięczne domknięcie: wynik okresu przeniesiony na `RetainedEarnings`
/// i migawka liczb, których pierścień dziennika nie utrzyma przez rok.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PeriodClose {
    /// Numer miesiąca od startu świata (kalendarz 360-dniowy, `K-1`).
    pub month: u32,
    pub from: Tick,
    pub to: Tick,
    pub statement: IncomeStatement,
    pub cash_end: Money,
    pub inventory_end: Money,
    pub retained_end: Money,
}

/// Rachunek zysków i strat. Wszystkie pozycje **dodatnie**, jak w druku.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct IncomeStatement {
    pub revenue: Money,
    pub cogs: Money,
    pub wages: Money,
    pub rent: Money,
    pub utilities: Money,
    pub depreciation: Money,
    pub interest: Money,
    pub write_off: Money,
    pub tax: Money,
}

impl IncomeStatement {
    #[must_use]
    pub fn gross_margin(&self) -> Money {
        Money(self.revenue.get() - self.cogs.get())
    }

    /// `Revenue − Cogs − koszty` — liczba, która ma się równać zmianie
    /// `RetainedEarnings` (kryterium WP7).
    #[must_use]
    pub fn net_result(&self) -> Money {
        Money(
            self.revenue.get()
                - self.cogs.get()
                - self.wages.get()
                - self.rent.get()
                - self.utilities.get()
                - self.depreciation.get()
                - self.interest.get()
                - self.write_off.get()
                - self.tax.get(),
        )
    }

    fn add(&mut self, o: &IncomeStatement) {
        self.revenue = Money(self.revenue.get() + o.revenue.get());
        self.cogs = Money(self.cogs.get() + o.cogs.get());
        self.wages = Money(self.wages.get() + o.wages.get());
        self.rent = Money(self.rent.get() + o.rent.get());
        self.utilities = Money(self.utilities.get() + o.utilities.get());
        self.depreciation = Money(self.depreciation.get() + o.depreciation.get());
        self.interest = Money(self.interest.get() + o.interest.get());
        self.write_off = Money(self.write_off.get() + o.write_off.get());
        self.tax = Money(self.tax.get() + o.tax.get());
    }
}

/// Bilans na dzień. Aktywa dodatnie, pasywa dodatnie — znak odwrócony wobec sald.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BalanceSheet {
    pub cash: Money,
    pub bank: Money,
    pub inventory: Money,
    pub fixed_net: Money,
    pub assets: Money,
    pub trade_payable: Money,
    pub tax_payable: Money,
    pub wage_payable: Money,
    pub loans: Money,
    pub liabilities: Money,
    pub equity: Money,
    pub retained: Money,
    /// Wynik bieżącego, jeszcze niedomkniętego okresu.
    pub period_result: Money,
    pub equity_total: Money,
}

impl BalanceSheet {
    /// `Aktywa − Pasywa`. Zero co do grosza jest kryterium WP7 i wynika wprost
    /// z `Σ lines == 0` w każdym zapisie — ale sprawdza się je, a nie zakłada.
    #[must_use]
    pub fn imbalance(&self) -> Money {
        Money(self.assets.get() - self.liabilities.get() - self.equity_total.get())
    }
}

/// Przepływy pieniężne metodą bezpośrednią — z zapisów, nie z różnicy sald.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CashFlow {
    pub operating: Money,
    pub investing: Money,
    pub financing: Money,
    pub net: Money,
    /// `false`, gdy okno dziennika nie objęło całego żądanego przedziału — wtedy
    /// liczby są prawdziwe dla tego, co widać, i nieprawdziwe dla całości.
    /// Milcząco obcięty raport finansowy jest gorszy od raportu z adnotacją.
    pub complete: bool,
}

// ── księga ───────────────────────────────────────────────────────────────────────

/// Księga jednego zakładu.
pub struct Ledger {
    pub site: SiteId,
    pub firm: FirmId,
    balances: [Money; LEDGER_ACCOUNT_COUNT],
    journal: Vec<JournalEntry>,
    head: usize,
    /// Ile zapisów w ogóle przeszło przez księgę — pierścień pokazuje ostatnie.
    posted: u64,
    /// `false` = zakład nieśledzony, dziennik nie jest prowadzony (salda tak).
    keep_journal: bool,
    pub periods: Vec<PeriodClose>,
    /// Tick, od którego liczy się bieżący, niedomknięty okres.
    period_from: Tick,
}

impl Ledger {
    #[must_use]
    pub fn new(site: SiteId, firm: FirmId, opened: Tick) -> Ledger {
        Ledger {
            site,
            firm,
            balances: [Money::ZERO; LEDGER_ACCOUNT_COUNT],
            journal: Vec::new(),
            head: 0,
            posted: 0,
            keep_journal: false,
            periods: Vec::new(),
            period_from: opened,
        }
    }

    /// Włącza albo wyłącza pierścień zapisów. Wołane z tego samego miejsca, co
    /// `Market::set_tracking` — poziom śledzenia **nie wchodzi do hasha** (`U-22`),
    /// więc kliknięcie „śledź" nie zmienia świata.
    pub fn set_journal(&mut self, on: bool) {
        self.keep_journal = on;
        if !on {
            self.journal.clear();
            self.head = 0;
        }
    }

    #[must_use]
    pub fn balance(&self, a: LedgerAccount) -> Money {
        self.balances[a.as_index()]
    }

    #[must_use]
    pub fn balances(&self) -> &[Money; LEDGER_ACCOUNT_COUNT] {
        &self.balances
    }

    #[must_use]
    pub fn posted(&self) -> u64 {
        self.posted
    }

    /// Zapisy od najstarszego do najnowszego w oknie.
    pub fn journal(&self) -> impl Iterator<Item = &JournalEntry> {
        self.journal[self.head..]
            .iter()
            .chain(self.journal[..self.head].iter())
    }

    #[must_use]
    pub fn window_len(&self) -> usize {
        self.journal.len()
    }

    #[must_use]
    pub fn period_from(&self) -> Tick {
        self.period_from
    }
}

/// Jedyne wejście do sald. Odrzuca niezbilansowane (P4) — i to jest cały powód,
/// dla którego `Ledger.balances` jest prywatne.
pub fn post(ledger: &mut Ledger, e: JournalEntry) -> Result<(), LedgerError> {
    ledger_post(e.lines(), &mut ledger.balances)?;
    ledger.posted += 1;
    if ledger.keep_journal {
        if ledger.journal.len() < JOURNAL_WINDOW {
            ledger.journal.push(e);
        } else {
            ledger.journal[ledger.head] = e;
            ledger.head = (ledger.head + 1) % JOURNAL_WINDOW;
        }
    }
    Ok(())
}

/// Domknięcie okresu: wynik z kont wynikowych przenosi się na `RetainedEarnings`,
/// a konta wynikowe wracają do zera.
///
/// Każde konto idzie **osobnym, dwuliniowym zapisem** — dzięki temu zapis mieści się
/// w [`MAX_LINES`], a dziennik pokazuje, skąd wziął się wynik, zamiast jednej pozycji
/// „domknięcie".
pub fn close_period(
    ledger: &mut Ledger,
    month: u32,
    to: Tick,
    reason: DecisionReason,
) -> Result<PeriodClose, LedgerError> {
    let statement = statement_from(&ledger.balances);
    for a in LedgerAccount::ALL {
        if !a.is_profit_and_loss() {
            continue;
        }
        let saldo = ledger.balance(*a);
        if saldo == Money::ZERO {
            continue;
        }
        post(
            ledger,
            JournalEntry::new(
                to,
                reason,
                &[
                    (*a, Money(-saldo.get())),
                    (LedgerAccount::RetainedEarnings, saldo),
                ],
            ),
        )?;
    }
    let close = PeriodClose {
        month,
        from: ledger.period_from,
        to,
        statement,
        cash_end: Money(
            ledger.balance(LedgerAccount::Cash).get() + ledger.balance(LedgerAccount::BankCurrent).get(),
        ),
        inventory_end: ledger.balance(LedgerAccount::InventoryGoods),
        retained_end: Money(-ledger.balance(LedgerAccount::RetainedEarnings).get()),
    };
    ledger.periods.push(close);
    ledger.period_from = to;
    Ok(close)
}

/// Rachunek wyników za przedział. Miesiące domknięte biorą się z [`PeriodClose`],
/// bieżący — z sald kont wynikowych, które jeszcze nie zostały domknięte.
///
/// Przedział jest półotwarty `[from, to)` i obejmuje domknięcie, którego `to` w nim
/// leży — miesiąc liczy się do tego okresu, w którym się skończył.
#[must_use]
pub fn income_statement(l: &Ledger, from: Tick, to: Tick) -> IncomeStatement {
    let mut out = IncomeStatement::default();
    for p in &l.periods {
        if p.to.get() > from.get() && p.to.get() <= to.get() {
            out.add(&p.statement);
        }
    }
    // Bieżący okres wchodzi, jeśli jeszcze się nie domknął i zahacza o przedział.
    if to.get() > l.period_from.get() {
        out.add(&statement_from(&l.balances));
    }
    out
}

/// Bilans na chwilę `at`. Salda są bieżące — księga nie zna historii sald poza
/// domknięciami, więc `at` służy wyłącznie do opisania wyniku i wyboru okresu.
#[must_use]
pub fn balance_sheet(l: &Ledger, at: Tick) -> BalanceSheet {
    let b = &l.balances;
    let g = |a: LedgerAccount| b[a.as_index()].get();
    let cash = g(LedgerAccount::Cash);
    let bank = g(LedgerAccount::BankCurrent);
    let inventory = g(LedgerAccount::InventoryGoods);
    let fixed_net = g(LedgerAccount::FixedAssets) + g(LedgerAccount::AccumDepreciation);
    let assets = cash + bank + inventory + fixed_net;

    let trade = -g(LedgerAccount::TradePayable);
    let tax = -g(LedgerAccount::TaxPayable);
    let wage = -g(LedgerAccount::WagePayable);
    let loans = -(g(LedgerAccount::LoansShort) + g(LedgerAccount::LoansLong));
    let liabilities = trade + tax + wage + loans;

    let equity = -g(LedgerAccount::Equity);
    let retained = -g(LedgerAccount::RetainedEarnings);
    let period = statement_from(b).net_result().get();
    let _ = at;
    BalanceSheet {
        cash: Money(cash),
        bank: Money(bank),
        inventory: Money(inventory),
        fixed_net: Money(fixed_net),
        assets: Money(assets),
        trade_payable: Money(trade),
        tax_payable: Money(tax),
        wage_payable: Money(wage),
        loans: Money(loans),
        liabilities: Money(liabilities),
        equity: Money(equity),
        retained: Money(retained),
        period_result: Money(period),
        equity_total: Money(equity + retained + period),
    }
}

/// Przepływy metodą bezpośrednią: przegląd zapisów okna, klasyfikacja po koncie
/// przeciwstawnym do linii pieniężnej.
#[must_use]
pub fn cash_flow(l: &Ledger, from: Tick, to: Tick) -> CashFlow {
    let mut out = CashFlow {
        complete: l.window_len() < JOURNAL_WINDOW || l.journal().next().is_some_and(|e| e.tick <= from),
        ..CashFlow::default()
    };
    if !l.keep_journal {
        out.complete = false;
    }
    for e in l.journal() {
        if e.tick.get() < from.get() || e.tick.get() >= to.get() {
            continue;
        }
        let ruch: i64 = e
            .lines()
            .iter()
            .filter(|(a, _)| a.is_cash())
            .map(|(_, m)| m.get())
            .sum();
        if ruch == 0 {
            continue;
        }
        let kategoria = e
            .lines()
            .iter()
            .find(|(a, _)| !a.is_cash())
            .map_or(Kind::Operating, |(a, _)| classify(*a));
        match kategoria {
            Kind::Operating => out.operating = Money(out.operating.get() + ruch),
            Kind::Investing => out.investing = Money(out.investing.get() + ruch),
            Kind::Financing => out.financing = Money(out.financing.get() + ruch),
        }
    }
    out.net = Money(out.operating.get() + out.investing.get() + out.financing.get());
    out
}

enum Kind {
    Operating,
    Investing,
    Financing,
}

fn classify(a: LedgerAccount) -> Kind {
    match a {
        LedgerAccount::FixedAssets | LedgerAccount::AccumDepreciation => Kind::Investing,
        LedgerAccount::LoansShort
        | LedgerAccount::LoansLong
        | LedgerAccount::Equity
        | LedgerAccount::RetainedEarnings => Kind::Financing,
        _ => Kind::Operating,
    }
}

/// Rachunek wyników wprost z sald kont wynikowych — znak odwrócony na „jak w druku".
fn statement_from(b: &[Money; LEDGER_ACCOUNT_COUNT]) -> IncomeStatement {
    let g = |a: LedgerAccount| b[a.as_index()].get();
    IncomeStatement {
        revenue: Money(-g(LedgerAccount::Revenue)),
        cogs: Money(g(LedgerAccount::Cogs)),
        wages: Money(g(LedgerAccount::WagesExpense)),
        rent: Money(g(LedgerAccount::RentExpense)),
        utilities: Money(g(LedgerAccount::UtilitiesExpense)),
        depreciation: Money(g(LedgerAccount::DepreciationExpense)),
        interest: Money(g(LedgerAccount::InterestExpense)),
        write_off: Money(g(LedgerAccount::WriteOffExpense)),
        tax: Money(g(LedgerAccount::TaxExpense)),
    }
}

impl HashState for Ledger {
    /// Do hasha wchodzą **salda i domknięcia**, nie pierścień zapisów: pierścień
    /// prowadzi się tylko dla zakładów śledzonych, a poziom śledzenia nie jest
    /// stanem świata (`U-22`). Gdyby dziennik wchodził, kliknięcie „śledź"
    /// zmieniałoby hash.
    fn hash_state(&self, h: &mut StateHasher) {
        for m in &self.balances {
            m.hash_state(h);
        }
        h.write_u32(self.periods.len() as u32);
        for p in &self.periods {
            h.write_u32(p.month);
            p.statement.revenue.hash_state(h);
            p.statement.cogs.hash_state(h);
            p.retained_end.hash_state(h);
        }
        h.write_u64(self.posted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn ledger() -> Ledger {
        let e = Entity::new(1, NonZeroU32::MIN);
        let mut l = Ledger::new(SiteId(e), FirmId(e), Tick(0));
        l.set_journal(true);
        l
    }

    fn sprzedaz(l: &mut Ledger, t: u64, cena: i64, koszt: i64) {
        post(
            l,
            JournalEntry::new(
                Tick(t),
                DecisionReason::Unspecified,
                &[
                    (LedgerAccount::BankCurrent, Money(cena)),
                    (LedgerAccount::Revenue, Money(-cena)),
                    (LedgerAccount::Cogs, Money(koszt)),
                    (LedgerAccount::InventoryGoods, Money(-koszt)),
                ],
            ),
        )
        .unwrap();
    }

    #[test]
    fn bilans_zamyka_sie_po_kazdym_zapisie() {
        let mut l = ledger();
        post(
            &mut l,
            JournalEntry::new(
                Tick(0),
                DecisionReason::Unspecified,
                &[
                    (LedgerAccount::BankCurrent, Money(100_000)),
                    (LedgerAccount::Equity, Money(-100_000)),
                ],
            ),
        )
        .unwrap();
        post(
            &mut l,
            JournalEntry::new(
                Tick(0),
                DecisionReason::Unspecified,
                &[
                    (LedgerAccount::InventoryGoods, Money(40_000)),
                    (LedgerAccount::BankCurrent, Money(-40_000)),
                ],
            ),
        )
        .unwrap();
        for t in 1..200u64 {
            sprzedaz(&mut l, t, 260, 200);
        }
        let b = balance_sheet(&l, Tick(200));
        assert_eq!(b.imbalance(), Money::ZERO, "{b:?}");
        assert_eq!(b.period_result, Money(199 * 60));
    }

    #[test]
    fn domkniecie_miesiaca_przenosi_wynik_na_kapital_zapasowy() {
        let mut l = ledger();
        for t in 1..100u64 {
            sprzedaz(&mut l, t, 260, 200);
        }
        post(
            &mut l,
            JournalEntry::new(
                Tick(100),
                DecisionReason::Unspecified,
                &[
                    (LedgerAccount::RentExpense, Money(1_000)),
                    (LedgerAccount::BankCurrent, Money(-1_000)),
                ],
            ),
        )
        .unwrap();
        let wynik_przed = statement_from(l.balances()).net_result();
        let close = close_period(&mut l, 0, Tick(43_200), DecisionReason::Unspecified).unwrap();
        assert_eq!(close.statement.net_result(), wynik_przed);
        // Konta wynikowe wyzerowane, wynik siedzi w RetainedEarnings.
        for a in LedgerAccount::ALL.iter().filter(|a| a.is_profit_and_loss()) {
            assert_eq!(l.balance(*a), Money::ZERO, "{}", a.name());
        }
        assert_eq!(close.retained_end, wynik_przed);
        assert_eq!(balance_sheet(&l, Tick(43_200)).imbalance(), Money::ZERO);
    }

    #[test]
    fn rachunek_wynikow_roku_sumuje_domkniecia() {
        let mut l = ledger();
        for m in 0..12u32 {
            for t in 0..10u64 {
                sprzedaz(&mut l, u64::from(m) * 43_200 + t, 260, 200);
            }
            close_period(&mut l, m, Tick(u64::from(m + 1) * 43_200), DecisionReason::Unspecified)
                .unwrap();
        }
        let rzis = income_statement(&l, Tick(0), Tick(518_400));
        assert_eq!(rzis.revenue, Money(12 * 10 * 260));
        assert_eq!(rzis.cogs, Money(12 * 10 * 200));
        assert_eq!(rzis.net_result(), Money(12 * 10 * 60));
        // Kryterium WP7: wynik z RZiS == zmiana RetainedEarnings.
        assert_eq!(
            rzis.net_result(),
            Money(-l.balance(LedgerAccount::RetainedEarnings).get())
        );
    }

    #[test]
    fn przeplywy_dziela_sie_na_trzy_kategorie() {
        let mut l = ledger();
        post(
            &mut l,
            JournalEntry::new(
                Tick(1),
                DecisionReason::Unspecified,
                &[
                    (LedgerAccount::BankCurrent, Money(100_000)),
                    (LedgerAccount::Equity, Money(-100_000)),
                ],
            ),
        )
        .unwrap();
        post(
            &mut l,
            JournalEntry::new(
                Tick(2),
                DecisionReason::Unspecified,
                &[
                    (LedgerAccount::FixedAssets, Money(30_000)),
                    (LedgerAccount::BankCurrent, Money(-30_000)),
                ],
            ),
        )
        .unwrap();
        sprzedaz(&mut l, 3, 260, 200);
        let cf = cash_flow(&l, Tick(0), Tick(10));
        assert_eq!(cf.financing, Money(100_000));
        assert_eq!(cf.investing, Money(-30_000));
        assert_eq!(cf.operating, Money(260));
        assert_eq!(cf.net, Money(70_260));
        assert!(cf.complete);
    }

    #[test]
    fn nieśledzony_zaklad_ma_salda_ale_nie_ma_dziennika() {
        let e = Entity::new(1, NonZeroU32::MIN);
        let mut l = Ledger::new(SiteId(e), FirmId(e), Tick(0));
        sprzedaz(&mut l, 1, 260, 200);
        assert_eq!(l.window_len(), 0);
        assert_eq!(l.posted(), 1);
        assert_eq!(l.balance(LedgerAccount::Revenue), Money(-260));
        assert!(!cash_flow(&l, Tick(0), Tick(10)).complete);
    }
}
