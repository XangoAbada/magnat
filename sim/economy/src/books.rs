//! Pieniądz, konta i podwójny zapis (M5 §5.1).
//!
//! Jedna reguła organizuje cały moduł: **saldo zmienia się wyłącznie przez metodę
//! `Books`**. Pole `Account::balance` jest prywatne, a każde wejście — przelew, emisja
//! początkowa, kreacja i destrukcja pieniądza kredytowego, kanał kapitału zewnętrznego —
//! przechodzi przez `apply` i dopisuje pozycję do `MoneySupplyLedger`, jeśli pieniądza
//! przybywa albo ubywa. Dzięki temu niezmiennik P1 (suma sald == emisja − destrukcja)
//! jest własnością struktury, a nie dyscypliny wołających.
//!
//! `RestOfWorld` jest **zwykłym kontem**, nie ujściem: zakup u dostawcy zewnętrznego
//! i wypłata od abstrakcyjnego pracodawcy to przelewy jak każde inne, więc handel
//! z zagranicą nie wymaga osobnej ewidencji i nie ma jak zgubić grosza.

use std::collections::VecDeque;
use std::fmt;

use magnat_core::{
    CitizenId, DecisionReason, FirmId, GoodId, HashState, HouseholdId, Money, Qty, SiteId,
    StateHasher, Tick, UtilityService,
};

use crate::offer::OfferId;

/// Pojemność pierścienia dziennika. Dziennik jest **oknem diagnostycznym**, nie księgą:
/// księgę (plan kont, RZiS, bilans) wnosi `Ledger` w M5c, a zrzut historii na dysk —
/// kronika M9. Okno ma starczać na dobę ruchu jednego miasta przy podglądzie w panelu.
const JOURNAL_CAPACITY: usize = 4096;

// ── konta ────────────────────────────────────────────────────────────────────────

/// Indeks konta w `Books`. Wartość [`AccountId::OUTSIDE`] nie indeksuje niczego.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AccountId(pub u32);

impl AccountId {
    /// Druga strona zapisu, która **nie jest kontem**: emisja przy inicjalizacji świata,
    /// kreacja i destrukcja pieniądza kredytowego, kapitał spoza systemu (kanał M7).
    ///
    /// Sentinel, a nie `Option<AccountId>`, bo `Transaction` ma być rozmiaru stałego
    /// i bez gałęzi w gorącej ścieżce; `Books` odrzuca go w każdym zapytaniu o saldo.
    pub const OUTSIDE: AccountId = AccountId(u32::MAX);
}

/// Właściciel konta. Wariant `Bank` przesądza decyzję otwartą nr 7 zgodnie z propozycją
/// M5: bank ma `FirmId` i księgę jak sklep, a M7 czyni go pełną firmą z załogą —
/// zmiana dotyczy wtedy zawartości firmy, nie kształtu konta.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AccountOwner {
    Household(HouseholdId),
    /// Gotówka w portfelu mieszkańca.
    Citizen(CitizenId),
    Firm(FirmId),
    /// Konto własne banku: rezerwy i kapitał.
    Bank(FirmId),
    /// Budżet miasta — wariant zamówiony przez M8, w M5 nieużywany.
    City,
    /// Dostawca zewnętrzny, abstrakcyjny pracodawca, import.
    RestOfWorld,
    CentralBank,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AccountKind {
    Cash,
    Current,
    Savings,
    LoanLiability,
}

pub struct Account {
    pub owner: AccountOwner,
    pub kind: AccountKind,
    /// `None` dla gotówki — banknot nie leży w żadnym banku.
    pub bank: Option<FirmId>,
    /// PRYWATNE. Zmiana wyłącznie przez metody `Books` (WP1, kryterium ukończenia).
    balance: Money,
    /// Dopuszczalny debet, zawsze `>= 0`. Saldo nie może zejść poniżej `-overdraft_limit`.
    pub overdraft_limit: Money,
}

impl Account {
    #[must_use]
    pub fn balance(&self) -> Money {
        self.balance
    }
}

// ── ewidencja podaży pieniądza ───────────────────────────────────────────────────

/// Pozycje, z których wynika, ile pieniądza jest w świecie. Struktura jest prywatna
/// w `Books` (dostęp przez [`Books::supply`]) — gdyby była mutowalna z zewnątrz,
/// niezmiennik P1 dałby się złamać bez jednego przelewu.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct MoneySupplyLedger {
    /// Pieniądz wykreowany przy inicjalizacji świata.
    pub endowment: Money,
    /// Suma kapitałów udzielonych kredytów.
    pub credit_created: Money,
    /// Suma spłaconych kapitałów.
    pub credit_repaid: Money,
    /// Kanał zamówiony przez M7: kapitał sieci zewnętrznej wchodzącej do miasta.
    /// To NIE jest przepływ z `RestOfWorld` — to emisja spoza systemu, więc ma własną
    /// pozycję. W M5 obie pozycje są zawsze 0 (żaden system ich nie rusza), ale istnieją
    /// w strukturze, w niezmienniku i w teście P1b, żeby M7 dopisał tylko wywołanie.
    pub external_capital_in: Money,
    pub external_capital_out: Money,
    /// Kanał sektora gospodarstw domowych (M5b).
    ///
    /// Saldo gospodarstwa **nie jest kontem w `Books`** i nie będzie: mieszka
    /// w komponencie `Household` (`cash`, `bank`, `savings`, `debt`), którego
    /// właścicielem jest M3 i który M3 sam sumuje w `society::total_money`. Dwa
    /// źródła salda rozjechałyby się przy pierwszej transakcji, a testu po żadnej
    /// ze stron by na to nie było.
    ///
    /// Z punktu widzenia `Books` gospodarstwa są więc **na zewnątrz**, dokładnie
    /// tak jak inwestor z kanału M7: `household_sector_in` to pieniądz, który wszedł
    /// do ksiąg z komponentów (zapłata w sklepie), `household_sector_out` — który
    /// z ksiąg do komponentów wyszedł (pensja, wypłata). Niezmiennik P1 zostaje
    /// zielony co do grosza, a niezmiennik całego świata —
    /// `society::total_money + Books::total_balance()` — sprawdza test end-to-end,
    /// bo tylko on widzi obie księgi naraz.
    pub household_sector_in: Money,
    pub household_sector_out: Money,
}

impl MoneySupplyLedger {
    /// Ile pieniądza powinno być w systemie. Prawa strona niezmiennika P1.
    #[must_use]
    pub fn total(&self) -> Money {
        // Kolejność działań jest ustalona, bo `checked_*` wolno tu tylko rozwinąć
        // w panikę: przekroczenie `i64` przy sumie podaży znaczy, że świat jest
        // zepsuty dużo wcześniej niż w tej linii.
        let plus = self
            .endowment
            .checked_add(self.credit_created)
            .and_then(|m| m.checked_add(self.external_capital_in))
            .and_then(|m| m.checked_add(self.household_sector_in))
            .expect("MoneySupplyLedger: przepełnienie sumy emisji");
        plus.checked_sub(self.credit_repaid)
            .and_then(|m| m.checked_sub(self.external_capital_out))
            .and_then(|m| m.checked_sub(self.household_sector_out))
            .expect("MoneySupplyLedger: przepełnienie sumy destrukcji")
    }
}

// ── transakcje ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TxId(pub u64);

/// Identyfikator kredytu. Samą strukturę `Loan` wnosi M5d — tutaj jest tylko uchwyt,
/// bo `TxKind::LoanDraw` musi istnieć w dzienniku od pierwszego pakietu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LoanId(pub u32);

/// Inwestor spoza miasta. Ziarnistość kanału = „inwestor + kwota" (decyzja otwarta
/// nr 14, propozycja M5 przyjęta domyślnie); transze i harmonogram wejścia, jeśli M7
/// ich potrzebuje, są jego stanem, nie stanem `Books`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ExternalInvestorId(pub u32);

/// Skąd przyszedł towar. W M5 jedyną możliwością jest dostawca zewnętrzny;
/// M6 dokłada wariant z `FirmId`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SupplierRef {
    External,
}

/// Rodzaj daniny. Znaczenie nadaje `ChargeRegistry` z M8 — tutaj jest wyłącznie
/// etykietą w dzienniku, dzięki czemu M8 nie musi migrować zapisów (§9 pkt 8).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ChargeKind(pub u16);

/// Program wydatkowy miasta. Właścicielem znaczenia jest M8, tak jak przy `ChargeKind`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ProgramId(pub u16);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TxKind {
    RetailSale {
        offer: OfferId,
        good: GoodId,
        qty: Qty,
        buyer: HouseholdId,
    },
    WholesalePurchase {
        good: GoodId,
        qty: Qty,
        supplier: SupplierRef,
    },
    /// W M5 z konta `RestOfWorld` do gospodarstwa domowego (dochód egzogeniczny, R9).
    Wage {
        site: SiteId,
    },
    Rent {
        site: SiteId,
    },
    Utility {
        site: SiteId,
        kind: UtilityService,
    },
    LoanDraw {
        loan: LoanId,
    },
    LoanPayment {
        loan: LoanId,
        principal: Money,
        interest: Money,
    },
    Deposit {
        from_cash: bool,
    },
    Withdrawal,
    /// Emisja przy inicjalizacji świata — jedyny zapis, w którym pieniądz powstaje
    /// bez kredytu i bez inwestora.
    Endowment,
    // ── warianty zamówione przez M8; w M5 nieużywane, ale obecne w enumie i w dzienniku ──
    TaxPayment {
        charge: ChargeKind,
    },
    PublicSpend {
        program: ProgramId,
    },
    /// Kanał M7 (§5.1).
    ExternalCapital {
        investor: ExternalInvestorId,
        inflow: bool,
    },
}

/// Opis zapisu podawany przez wołającego. `tax` jest **wyłącznie VAT-em**
/// (rozstrzygnięcie z M8) i w M5 zawsze zero — pole istnieje tutaj, a nie w sygnaturze
/// `transfer`, żeby `CityTaxEngine` z M8 dopisywał wartość, a nie zmieniał API.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TxMemo {
    pub kind: TxKind,
    pub reason: DecisionReason,
    pub tax: Money,
}

impl TxMemo {
    #[must_use]
    pub fn new(kind: TxKind, reason: DecisionReason) -> TxMemo {
        TxMemo {
            kind,
            reason,
            tax: Money::ZERO,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Transaction {
    pub id: TxId,
    pub tick: Tick,
    pub kind: TxKind,
    pub debit: AccountId,
    pub credit: AccountId,
    /// Kwota bez VAT.
    pub net: Money,
    /// WYŁĄCZNIE VAT. W M5 zawsze `Money(0)`.
    pub tax: Money,
    /// `net + tax` — kwota realnie przelana (K-7: cena w ofercie to kwota płacona).
    pub gross: Money,
    pub reason: DecisionReason,
}

/// Pierścień ostatnich transakcji.
pub struct TxJournal {
    ring: VecDeque<Transaction>,
    total: u64,
}

impl Default for TxJournal {
    fn default() -> TxJournal {
        TxJournal {
            ring: VecDeque::with_capacity(JOURNAL_CAPACITY),
            total: 0,
        }
    }
}

impl TxJournal {
    fn push(&mut self, tx: Transaction) {
        if self.ring.len() == JOURNAL_CAPACITY {
            self.ring.pop_front();
        }
        self.ring.push_back(tx);
        self.total += 1;
    }

    /// Od najstarszej do najnowszej z tych, które jeszcze mieszczą się w oknie.
    pub fn iter(&self) -> impl Iterator<Item = &Transaction> {
        self.ring.iter()
    }

    #[must_use]
    pub fn last(&self) -> Option<&Transaction> {
        self.ring.back()
    }

    /// Liczba transakcji od początku świata, a nie długość okna.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }

    #[must_use]
    pub fn window_len(&self) -> usize {
        self.ring.len()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TxError {
    /// Kwota musi być dodatnia — przelew zerowy albo ujemny to błąd wołającego,
    /// a nie sposób na przelew w drugą stronę.
    NonPositive,
    InsufficientFunds,
    SameAccount,
    Unknown,
    /// Saldo albo pozycja podaży wyszłyby poza `i64`.
    Overflow,
    /// `TxMemo.tax` jest ujemny albo większy od kwoty. Sprawdzane **przed** mutacją,
    /// bo w M8 wartość wypełnia `CityTaxEngine`, a rozbicie na netto i VAT dzieje się
    /// dopiero przy zapisie do dziennika — panika w tym miejscu zostawiłaby zmienione
    /// salda bez transakcji.
    InvalidTax,
}

impl fmt::Display for TxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TxError::NonPositive => "kwota przelewu musi być dodatnia",
            TxError::InsufficientFunds => "brak środków w granicach limitu debetu",
            TxError::SameAccount => "konto źródłowe i docelowe są tym samym kontem",
            TxError::Unknown => "nieznane konto",
            TxError::Overflow => "przepełnienie zakresu kwoty",
            TxError::InvalidTax => "VAT ujemny albo większy od kwoty transakcji",
        };
        f.write_str(s)
    }
}

impl std::error::Error for TxError {}

// ── księga ───────────────────────────────────────────────────────────────────────

/// Jedyne wejście do zmiany stanu pieniężnego w `sim/economy`.
#[derive(Default)]
pub struct Books {
    accounts: Vec<Account>,
    supply: MoneySupplyLedger,
    journal: TxJournal,
    next_tx: u64,
}

impl Books {
    #[must_use]
    pub fn new() -> Books {
        Books::default()
    }

    pub fn open_account(
        &mut self,
        owner: AccountOwner,
        kind: AccountKind,
        bank: Option<FirmId>,
        overdraft_limit: Money,
    ) -> AccountId {
        assert!(
            overdraft_limit.get() >= 0,
            "limit debetu jest kwotą dodatnią; ujemny znaczyłby saldo minimalne powyżej zera"
        );
        // Górna granica jest o jeden niższa niż zakres `u32`, bo `AccountId::OUTSIDE`
        // zajmuje ostatnią wartość — konto o tym numerze byłoby nieodróżnialne
        // od strony spoza systemu i emisja zaczęłaby uznawać przypadkowy rachunek.
        let n = u32::try_from(self.accounts.len()).expect("za dużo kont");
        assert!(n < AccountId::OUTSIDE.0, "za dużo kont");
        let id = AccountId(n);
        self.accounts.push(Account {
            owner,
            kind,
            bank,
            balance: Money::ZERO,
            overdraft_limit,
        });
        id
    }

    #[must_use]
    pub fn account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.get(id.0 as usize)
    }

    #[must_use]
    pub fn balance(&self, id: AccountId) -> Option<Money> {
        self.account(id).map(Account::balance)
    }

    #[must_use]
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }

    #[must_use]
    pub fn supply(&self) -> &MoneySupplyLedger {
        &self.supply
    }

    #[must_use]
    pub fn journal(&self) -> &TxJournal {
        &self.journal
    }

    /// Suma sald wszystkich kont. Lewa strona niezmiennika P1.
    #[must_use]
    pub fn total_balance(&self) -> Money {
        self.accounts
            .iter()
            .try_fold(Money::ZERO, |acc, a| acc.checked_add(a.balance))
            .expect("Books: przepełnienie sumy sald")
    }

    /// Niezmiennik P1/P1b. `Err((suma_sald, podaż))` zamiast `bool`, bo przy czerwonym
    /// teście chce się znać obie liczby, a nie fakt ich nierówności.
    pub fn check_conservation(&self) -> Result<(), (Money, Money)> {
        let sum = self.total_balance();
        let supply = self.supply.total();
        if sum == supply {
            Ok(())
        } else {
            Err((sum, supply))
        }
    }

    // ── jedyne wejścia zmieniające saldo ─────────────────────────────────────────

    /// JEDYNY sposób przeniesienia pieniądza między kontami.
    pub fn transfer(
        &mut self,
        from: AccountId,
        to: AccountId,
        amount: Money,
        memo: TxMemo,
        t: Tick,
    ) -> Result<TxId, TxError> {
        if amount.get() <= 0 {
            return Err(TxError::NonPositive);
        }
        if from == to {
            return Err(TxError::SameAccount);
        }
        if memo.tax.get() < 0 || memo.tax > amount {
            return Err(TxError::InvalidTax);
        }
        // Obie strony sprawdzane przed jakąkolwiek mutacją: przelew albo wykonuje się
        // w całości, albo nie zostawia po sobie śladu.
        let new_from = self.checked_debit(from, amount)?;
        let new_to = self.checked_credit(to, amount)?;
        self.set_balance(from, new_from);
        self.set_balance(to, new_to);
        Ok(self.record(t, memo, from, to, amount))
    }

    /// Emisja przy inicjalizacji świata (PRD §6.5). Wołana przez generator, nie przez
    /// system symulacji — pieniądz w obiegu ma źródło jawne i policzone w `supply`.
    pub fn endow(&mut self, to: AccountId, amount: Money, t: Tick) -> Result<TxId, TxError> {
        let memo = TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified);
        self.emit(to, amount, memo, t, |s, m| {
            s.endowment = s.endowment.checked_add(m).ok_or(TxError::Overflow)?;
            Ok(())
        })
    }

    /// Kreacja pieniądza kredytowego — jedyny kanał emisji poza [`Books::endow`]
    /// i kanałem M7. Wołać wolno wyłącznie bankowi przy uruchomieniu kredytu (M5d).
    ///
    /// `pub`, a nie `pub(crate)` jak w §5.1: cała faza M5 mieszka w tym crate'cie,
    /// więc `pub(crate)` nie ograniczałby niczego poza widocznością w teście
    /// własnościowym P1 — a to jest test, który ma sprawdzać **wszystkie** kanały
    /// podaży, nie te wygodne. Niezmiennik P1 trzyma się bez tej bariery, bo emisja
    /// i wpis do `MoneySupplyLedger` dzieją się w jednym wyrażeniu.
    /// `reason` niesie **decyzję kredytową banku** (`CreditApproved`): uruchomienie
    /// kredytu jest wyborem, a nie wykonaniem harmonogramu, więc powód jest tu
    /// obowiązkowy (PRD §14.1). Bramka G9 balansatora liczy właśnie tę ścieżkę.
    pub fn create_credit(
        &mut self,
        to: AccountId,
        amount: Money,
        loan: LoanId,
        reason: DecisionReason,
        t: Tick,
    ) -> Result<TxId, TxError> {
        let memo = TxMemo::new(TxKind::LoanDraw { loan }, reason);
        self.emit(to, amount, memo, t, |s, m| {
            s.credit_created = s.credit_created.checked_add(m).ok_or(TxError::Overflow)?;
            Ok(())
        })
    }

    /// Destrukcja pieniądza kredytowego przy spłacie kapitału (M5d).
    /// Widoczność jak przy [`Books::create_credit`].
    pub fn destroy_credit(
        &mut self,
        from: AccountId,
        amount: Money,
        loan: LoanId,
        t: Tick,
    ) -> Result<TxId, TxError> {
        let memo = TxMemo::new(
            TxKind::LoanPayment {
                loan,
                principal: amount,
                interest: Money::ZERO,
            },
            DecisionReason::Unspecified,
        );
        self.absorb(from, amount, memo, t, |s, m| {
            s.credit_repaid = s.credit_repaid.checked_add(m).ok_or(TxError::Overflow)?;
            Ok(())
        })
    }

    /// Wejście kapitału inwestora zewnętrznego. Jedyne wejście kanału M7; w M5 nie woła
    /// go żaden system i pokrywa go wyłącznie test własnościowy P1b.
    pub fn inject_external_capital(
        &mut self,
        to: AccountId,
        amount: Money,
        investor: ExternalInvestorId,
        t: Tick,
    ) -> Result<TxId, TxError> {
        let memo = TxMemo::new(
            TxKind::ExternalCapital {
                investor,
                inflow: true,
            },
            DecisionReason::Unspecified,
        );
        self.emit(to, amount, memo, t, |s, m| {
            s.external_capital_in = s
                .external_capital_in
                .checked_add(m)
                .ok_or(TxError::Overflow)?;
            Ok(())
        })
    }

    /// Repatriacja zysku albo wyjście z rynku — druga strona kanału M7.
    pub fn repatriate_external_capital(
        &mut self,
        from: AccountId,
        amount: Money,
        investor: ExternalInvestorId,
        t: Tick,
    ) -> Result<TxId, TxError> {
        let memo = TxMemo::new(
            TxKind::ExternalCapital {
                investor,
                inflow: false,
            },
            DecisionReason::Unspecified,
        );
        self.absorb(from, amount, memo, t, |s, m| {
            s.external_capital_out = s
                .external_capital_out
                .checked_add(m)
                .ok_or(TxError::Overflow)?;
            Ok(())
        })
    }

    /// Zapłata gospodarstwa domowego: pieniądz wchodzi do ksiąg z komponentu
    /// `Household` (M5b §5.5). Wołający **musi** w tej samej operacji zdjąć kwotę
    /// z komponentu — inaczej pieniądza przybywa w świecie.
    ///
    /// Jedno wywołanie, jedna strona: druga strona jest poza `Books`, więc zapis ma
    /// `AccountId::OUTSIDE` po stronie Wn, tak samo jak emisja i kanał M7.
    pub fn household_pay(
        &mut self,
        to: AccountId,
        amount: Money,
        memo: TxMemo,
        t: Tick,
    ) -> Result<TxId, TxError> {
        self.emit(to, amount, memo, t, |s, m| {
            s.household_sector_in = s
                .household_sector_in
                .checked_add(m)
                .ok_or(TxError::Overflow)?;
            Ok(())
        })
    }

    /// Wypłata do gospodarstwa domowego — druga strona kanału sektora GD.
    /// Symetryczny wymóg: wołający dopisuje kwotę do komponentu.
    pub fn household_receive(
        &mut self,
        from: AccountId,
        amount: Money,
        memo: TxMemo,
        t: Tick,
    ) -> Result<TxId, TxError> {
        self.absorb(from, amount, memo, t, |s, m| {
            s.household_sector_out = s
                .household_sector_out
                .checked_add(m)
                .ok_or(TxError::Overflow)?;
            Ok(())
        })
    }

    // ── mechanika wspólna ────────────────────────────────────────────────────────

    /// Pieniądz wchodzi do systemu: konto rośnie, pozycja podaży rośnie o tyle samo.
    fn emit(
        &mut self,
        to: AccountId,
        amount: Money,
        memo: TxMemo,
        t: Tick,
        bump: impl Fn(&mut MoneySupplyLedger, Money) -> Result<(), TxError>,
    ) -> Result<TxId, TxError> {
        if amount.get() <= 0 {
            return Err(TxError::NonPositive);
        }
        let new_to = self.checked_credit(to, amount)?;
        let mut supply = self.supply;
        bump(&mut supply, amount)?;
        self.supply = supply;
        self.set_balance(to, new_to);
        Ok(self.record(t, memo, AccountId::OUTSIDE, to, amount))
    }

    /// Pieniądz wychodzi z systemu: konto maleje, pozycja destrukcji rośnie.
    fn absorb(
        &mut self,
        from: AccountId,
        amount: Money,
        memo: TxMemo,
        t: Tick,
        bump: impl Fn(&mut MoneySupplyLedger, Money) -> Result<(), TxError>,
    ) -> Result<TxId, TxError> {
        if amount.get() <= 0 {
            return Err(TxError::NonPositive);
        }
        let new_from = self.checked_debit(from, amount)?;
        let mut supply = self.supply;
        bump(&mut supply, amount)?;
        self.supply = supply;
        self.set_balance(from, new_from);
        Ok(self.record(t, memo, from, AccountId::OUTSIDE, amount))
    }

    /// Saldo po obciążeniu — z kontrolą limitu debetu. Nic nie zapisuje.
    fn checked_debit(&self, id: AccountId, amount: Money) -> Result<Money, TxError> {
        let acc = self.account(id).ok_or(TxError::Unknown)?;
        let new = acc.balance.checked_sub(amount).ok_or(TxError::Overflow)?;
        let floor = Money(-acc.overdraft_limit.get());
        if new < floor {
            return Err(TxError::InsufficientFunds);
        }
        Ok(new)
    }

    /// Saldo po uznaniu. Nic nie zapisuje.
    fn checked_credit(&self, id: AccountId, amount: Money) -> Result<Money, TxError> {
        let acc = self.account(id).ok_or(TxError::Unknown)?;
        acc.balance.checked_add(amount).ok_or(TxError::Overflow)
    }

    fn set_balance(&mut self, id: AccountId, v: Money) {
        self.accounts[id.0 as usize].balance = v;
    }

    fn record(
        &mut self,
        t: Tick,
        memo: TxMemo,
        debit: AccountId,
        credit: AccountId,
        gross: Money,
    ) -> TxId {
        let id = TxId(self.next_tx);
        self.next_tx += 1;
        // Niezmiennik sprawdzony przez wołającego przed mutacją sald (TxError::InvalidTax);
        // tutaj jest już tylko asercją, żeby rozjazd wyszedł natychmiast, a nie w bilansie.
        let net = gross
            .checked_sub(memo.tax)
            .expect("TxMemo.tax większy niż kwota transakcji");
        self.journal.push(Transaction {
            id,
            tick: t,
            kind: memo.kind,
            debit,
            credit,
            net,
            tax: memo.tax,
            gross,
            reason: memo.reason,
        });
        id
    }
}

// ── hash stanu (00 §3.6) ─────────────────────────────────────────────────────────

impl HashState for AccountOwner {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            AccountOwner::Household(x) => {
                h.write_u8(0);
                x.entity().hash_state(h);
            }
            AccountOwner::Citizen(x) => {
                h.write_u8(1);
                x.entity().hash_state(h);
            }
            AccountOwner::Firm(x) => {
                h.write_u8(2);
                x.entity().hash_state(h);
            }
            AccountOwner::Bank(x) => {
                h.write_u8(3);
                x.entity().hash_state(h);
            }
            AccountOwner::City => h.write_u8(4),
            AccountOwner::RestOfWorld => h.write_u8(5),
            AccountOwner::CentralBank => h.write_u8(6),
        }
    }
}

impl HashState for Books {
    /// Hashowane jest to, co jest stanem: salda, właściciele i ewidencja podaży.
    /// Dziennik wchodzi wyłącznie licznikiem `next_tx` — jego okno jest pierścieniem
    /// o stałej pojemności, więc hash zawartości uzależniłby stan świata od rozmiaru
    /// bufora diagnostycznego. Rozjazd treści zapisu i tak wychodzi na saldach.
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.accounts.len() as u64);
        for a in &self.accounts {
            a.owner.hash_state(h);
            h.write_u8(a.kind as u8);
            a.bank.map(|f| f.entity()).hash_state(h);
            a.balance.hash_state(h);
            a.overdraft_limit.hash_state(h);
        }
        self.supply.endowment.hash_state(h);
        self.supply.credit_created.hash_state(h);
        self.supply.credit_repaid.hash_state(h);
        self.supply.external_capital_in.hash_state(h);
        self.supply.external_capital_out.hash_state(h);
        self.supply.household_sector_in.hash_state(h);
        self.supply.household_sector_out.hash_state(h);
        h.write_u64(self.next_tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_account(books: &mut Books) -> AccountId {
        books.open_account(
            AccountOwner::RestOfWorld,
            AccountKind::Current,
            None,
            Money::ZERO,
        )
    }

    fn cash(books: &mut Books) -> AccountId {
        books.open_account(AccountOwner::City, AccountKind::Cash, None, Money::ZERO)
    }

    #[test]
    fn emisja_i_przelew_zachowuja_sume() {
        let mut b = Books::new();
        let a = world_account(&mut b);
        let c = cash(&mut b);
        b.endow(a, Money(100_000), Tick(0)).unwrap();
        b.transfer(
            a,
            c,
            Money(30_000),
            TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified),
            Tick(1),
        )
        .unwrap();
        assert_eq!(b.balance(a), Some(Money(70_000)));
        assert_eq!(b.balance(c), Some(Money(30_000)));
        assert_eq!(b.check_conservation(), Ok(()));
        assert_eq!(b.journal().total(), 2);
    }

    #[test]
    fn przelew_ujemny_i_zerowy_zwraca_blad() {
        let mut b = Books::new();
        let a = world_account(&mut b);
        let c = cash(&mut b);
        b.endow(a, Money(1_000), Tick(0)).unwrap();
        let memo = TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified);
        assert_eq!(
            b.transfer(a, c, Money(0), memo, Tick(1)),
            Err(TxError::NonPositive)
        );
        assert_eq!(
            b.transfer(a, c, Money(-5), memo, Tick(1)),
            Err(TxError::NonPositive)
        );
        assert_eq!(
            b.transfer(a, a, Money(5), memo, Tick(1)),
            Err(TxError::SameAccount)
        );
        assert_eq!(
            b.transfer(a, AccountId(99), Money(5), memo, Tick(1)),
            Err(TxError::Unknown)
        );
        // Żaden błąd nie ruszył sald.
        assert_eq!(b.balance(a), Some(Money(1_000)));
        assert_eq!(b.journal().total(), 1);
    }

    #[test]
    fn limit_debetu_jest_dolna_granica_salda() {
        let mut b = Books::new();
        let a = world_account(&mut b);
        let c = b.open_account(AccountOwner::City, AccountKind::Current, None, Money(2_000));
        b.endow(a, Money(1_000), Tick(0)).unwrap();
        let memo = TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified);
        // Do limitu wolno.
        b.transfer(c, a, Money(2_000), memo, Tick(1)).unwrap();
        assert_eq!(b.balance(c), Some(Money(-2_000)));
        // O grosz dalej już nie.
        assert_eq!(
            b.transfer(c, a, Money(1), memo, Tick(2)),
            Err(TxError::InsufficientFunds)
        );
        assert_eq!(b.check_conservation(), Ok(()));
    }

    #[test]
    fn kredyt_tworzy_i_niszczy_pieniadz() {
        let mut b = Books::new();
        let a = world_account(&mut b);
        b.create_credit(
            a,
            Money(50_000),
            LoanId(1),
            DecisionReason::Unspecified,
            Tick(0),
        )
        .unwrap();
        assert_eq!(b.supply().credit_created, Money(50_000));
        assert_eq!(b.check_conservation(), Ok(()));
        b.destroy_credit(a, Money(20_000), LoanId(1), Tick(1))
            .unwrap();
        assert_eq!(b.supply().credit_repaid, Money(20_000));
        assert_eq!(b.total_balance(), Money(30_000));
        assert_eq!(b.check_conservation(), Ok(()));
    }

    #[test]
    fn kanal_kapitalu_zewnetrznego_domyka_sie() {
        let mut b = Books::new();
        let a = world_account(&mut b);
        b.inject_external_capital(a, Money(10_000), ExternalInvestorId(7), Tick(0))
            .unwrap();
        assert_eq!(b.check_conservation(), Ok(()));
        b.repatriate_external_capital(a, Money(4_000), ExternalInvestorId(7), Tick(1))
            .unwrap();
        assert_eq!(b.supply().external_capital_out, Money(4_000));
        assert_eq!(b.total_balance(), Money(6_000));
        assert_eq!(b.check_conservation(), Ok(()));
    }

    #[test]
    fn vat_rozbija_kwote_na_netto_i_podatek() {
        // W M5 `tax` jest zawsze zerem, ale rozbicie musi działać, zanim M8 je wypełni.
        let mut b = Books::new();
        let a = world_account(&mut b);
        let c = cash(&mut b);
        b.endow(a, Money(1_000), Tick(0)).unwrap();
        let mut memo = TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified);
        memo.tax = Money(230);
        b.transfer(a, c, Money(1_230 - 230), memo, Tick(1)).unwrap();
        let tx = *b.journal().last().unwrap();
        assert_eq!(tx.gross, Money(1_000));
        assert_eq!(tx.tax, Money(230));
        assert_eq!(tx.net, Money(770));

        // VAT większy od kwoty to błąd wołającego — i ma być wyłapany, zanim ruszy saldo.
        let before = b.balance(a);
        let mut zly = TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified);
        zly.tax = Money(1);
        assert_eq!(
            b.transfer(a, c, Money(0), zly, Tick(2)),
            Err(TxError::NonPositive)
        );
        zly.tax = Money(11);
        assert_eq!(
            b.transfer(a, c, Money(10), zly, Tick(2)),
            Err(TxError::InvalidTax)
        );
        zly.tax = Money(-1);
        assert_eq!(
            b.transfer(a, c, Money(10), zly, Tick(2)),
            Err(TxError::InvalidTax)
        );
        assert_eq!(before, b.balance(a));
    }

    #[test]
    fn dziennik_jest_pierscieniem_o_stalej_pojemnosci() {
        let mut b = Books::new();
        let a = world_account(&mut b);
        let c = cash(&mut b);
        b.endow(a, Money(1_000_000), Tick(0)).unwrap();
        let memo = TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified);
        for i in 0..(JOURNAL_CAPACITY + 100) {
            b.transfer(a, c, Money(1), memo, Tick(i as u64)).unwrap();
        }
        assert_eq!(b.journal().window_len(), JOURNAL_CAPACITY);
        assert_eq!(b.journal().total() as usize, JOURNAL_CAPACITY + 101);
    }
}
