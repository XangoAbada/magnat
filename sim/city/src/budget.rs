//! Budżet miasta i księga publiczna (M8a §5.1, WP1).
//!
//! **Miasto nie trzyma własnego salda.** Gotówka leży na koncie w `Books`
//! (`AccountOwner::City`), a nie w polu tej struktury — dwa źródła prawdy o jednej
//! liczbie rozjechałyby się przy pierwszym przelewie, a testu, który by to złapał,
//! nie ma po żadnej ze stron. Plan fazy zapisywał `CityBudget.cash: Money`; tutaj
//! jest `account: AccountId` i to jest cała różnica.
//!
//! Dzięki temu niezmiennik `Σ firmy + Σ mieszkańcy + Σ banki + miasto = const`
//! z kryterium WP1 nie wymaga po stronie M8 **ani jednej linii kodu**: jest to
//! `Books::check_conservation`, którego miasto nie umie złamać, bo nie ma innego
//! sposobu ruszenia pieniądza niż `transfer`.

use magnat_core::{
    DecisionReason, HashState, Money, SpendCategory, StateHasher, TaxKind, Tick,
    SPEND_CATEGORY_COUNT, TAX_KIND_COUNT,
};
use magnat_economy::{AccountId, Books, ProgramId, TxKind, TxMemo};
use serde::{Deserialize, Serialize};

/// Wersja schematu `data/city/budget.ron`.
pub const BUDGET_SCHEMA_VERSION: u32 = 1;

/// Numeracja kredytów miasta zaczyna się wysoko, żeby nie mieszała się w dzienniku
/// z kredytami gospodarstw (`Market::loans` liczy od zera). Ten sam zabieg co
/// `SITE_KEY_BASE` przy zakładach (`K-46`) i z tego samego powodu: dwie numeracje
/// w jednej przestrzeni są pomyłką, którą widać dopiero w raporcie.
pub const CITY_LOAN_BASE: u32 = 1 << 24;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SpendPlanRow {
    pub category: SpendCategory,
    pub bp: u32,
}

/// Polityka wydatkowa i długu — dane, nie kod.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BudgetPolicy {
    pub schema_version: u32,
    pub reserve_target_months: u8,
    pub plan_bp: Vec<SpendPlanRow>,
    pub max_cut_bp: u32,
    pub bond_min_principal: i64,
    pub bond_coupon_bp_per_year: u32,
    pub bond_term_months: u16,
    pub debt_ceiling_bp: u32,
}

impl Default for BudgetPolicy {
    /// Polityka „miasto nic nie wydaje" — używana tam, gdzie scenariusz stawia same
    /// daniny bez budżetu. Zero wydatków jest **brakiem polityki**, nie polityką
    /// oszczędności, i tak ma być czytane.
    fn default() -> BudgetPolicy {
        BudgetPolicy {
            schema_version: BUDGET_SCHEMA_VERSION,
            reserve_target_months: 2,
            plan_bp: Vec::new(),
            max_cut_bp: 3_000,
            bond_min_principal: 50_000_000,
            bond_coupon_bp_per_year: 650,
            bond_term_months: 120,
            debt_ceiling_bp: 20_000,
        }
    }
}

#[derive(Debug)]
pub enum BudgetError {
    Io(String),
    Parse(String),
    Schema { found: u32, want: u32 },
}

impl std::fmt::Display for BudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BudgetError::Io(e) | BudgetError::Parse(e) => write!(f, "data/city/budget.ron: {e}"),
            BudgetError::Schema { found, want } => write!(
                f,
                "data/city/budget.ron: schema_version {found}, oczekiwano {want}"
            ),
        }
    }
}

impl std::error::Error for BudgetError {}

impl BudgetPolicy {
    pub fn load(path: &std::path::Path) -> Result<BudgetPolicy, BudgetError> {
        let tekst = std::fs::read_to_string(path).map_err(|e| BudgetError::Io(e.to_string()))?;
        let p: BudgetPolicy =
            ron::from_str(&tekst).map_err(|e| BudgetError::Parse(e.to_string()))?;
        if p.schema_version != BUDGET_SCHEMA_VERSION {
            return Err(BudgetError::Schema {
                found: p.schema_version,
                want: BUDGET_SCHEMA_VERSION,
            });
        }
        Ok(p)
    }

    pub fn load_default() -> Result<BudgetPolicy, BudgetError> {
        BudgetPolicy::load(&magnat_core::data_path("city/budget.ron"))
    }

    /// Udziały rozwinięte w tablicę indeksowaną `SpendCategory`. Kategoria bez
    /// wpisu dostaje zero — brak wiersza znaczy „nie wydajemy", a nie błąd danych.
    #[must_use]
    pub fn shares(&self) -> [u32; SPEND_CATEGORY_COUNT] {
        let mut t = [0u32; SPEND_CATEGORY_COUNT];
        for r in &self.plan_bp {
            t[r.category.as_index()] = r.bp;
        }
        t
    }
}

/// Obligacja komunalna: nominał, kupon i termin wykupu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MunicipalBond {
    pub loan: magnat_economy::LoanId,
    pub principal: Money,
    pub coupon_bp_per_year: u32,
    pub issued_at: Tick,
    pub matures_at: Tick,
    pub redeemed: bool,
}

impl HashState for MunicipalBond {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.loan.0);
        h.write_i64(self.principal.get());
        h.write_u32(self.coupon_bp_per_year);
        h.write_u64(self.issued_at.0);
        h.write_u64(self.matures_at.0);
        h.write_u8(u8::from(self.redeemed));
    }
}

/// Księga publiczna: co wpłynęło, co wypłynęło, ile miasto jest winne.
#[derive(Clone, Debug)]
pub struct CityBudget {
    /// Konto miasta w `Books`. Saldo czyta się stamtąd, nie stąd.
    pub account: AccountId,
    pub revenue_ytd: [Money; TAX_KIND_COUNT],
    /// Wpływy **od początku świata**, per danina. Nie zerują się nigdy.
    ///
    /// Drugi licznik tej samej rzeczy jest tu z rozmysłu i ma powód zmierzony
    /// w przebiegu rocznym: `revenue_ytd` zeruje się 1 stycznia, więc równanie
    /// `Σ Settled == Δ revenue` z testu T1 da się nim sprawdzić **wyłącznie**
    /// w przebiegu krótszym niż rok. Pierwszy przebieg na 380 dobach pokazał
    /// rozjazd rzędu dziesięciokrotnego i była to własność raportu, nie budżetu.
    pub revenue_life: [Money; TAX_KIND_COUNT],
    pub spend_ytd: [Money; SPEND_CATEGORY_COUNT],
    /// Wpływy zamknięte, po miesiącach — okno dwunastu, podstawa planu wydatków.
    pub revenue_window: [Money; 12],
    pub debt: Vec<MunicipalBond>,
    pub fiscal_year: u16,
    /// O ile promili przycięto plan wydatków w ostatnim domknięciu miesiąca.
    pub cut_bp: u32,
    next_loan: u32,
}

impl CityBudget {
    #[must_use]
    pub fn new(account: AccountId) -> CityBudget {
        CityBudget {
            account,
            revenue_ytd: [Money::ZERO; TAX_KIND_COUNT],
            revenue_life: [Money::ZERO; TAX_KIND_COUNT],
            spend_ytd: [Money::ZERO; SPEND_CATEGORY_COUNT],
            revenue_window: [Money::ZERO; 12],
            debt: Vec::new(),
            fiscal_year: 0,
            cut_bp: 0,
            next_loan: CITY_LOAN_BASE,
        }
    }

    /// Wpływ z daniny. Jedyne wejście, którym rosną **oba** liczniki — roczny
    /// i ten od początku świata. Druga strona równania `Σ Settled == Δ revenue`
    /// z testu T1; sprawdza się je licznikiem dożywotnim, bo roczny zeruje się
    /// 1 stycznia i przy dłuższym przebiegu porównywałby dwie różne liczby.
    pub fn take_revenue(&mut self, kind: TaxKind, amount: Money) {
        let r = &mut self.revenue_ytd[kind.as_index()];
        *r = Money(r.get() + amount.get());
        let l = &mut self.revenue_life[kind.as_index()];
        *l = Money(l.get() + amount.get());
    }

    /// Wpływy od początku świata, wszystkie daniny razem — lewa strona równania,
    /// którą da się porównać z rejestrem niezależnie od roku obrotowego.
    #[must_use]
    pub fn revenue_life_total(&self) -> Money {
        Money(self.revenue_life.iter().map(|m| m.get()).sum())
    }

    #[must_use]
    pub fn revenue_total(&self) -> Money {
        Money(self.revenue_ytd.iter().map(|m| m.get()).sum())
    }

    #[must_use]
    pub fn spend_total(&self) -> Money {
        Money(self.spend_ytd.iter().map(|m| m.get()).sum())
    }

    /// Zadłużenie niewykupione.
    #[must_use]
    pub fn debt_outstanding(&self) -> Money {
        Money(
            self.debt
                .iter()
                .filter(|b| !b.redeemed)
                .map(|b| b.principal.get())
                .sum(),
        )
    }

    /// Podstawa planu wydatków: wpływy ostatnich dwunastu miesięcy podzielone
    /// przez dwanaście. Roczne, a nie miesięczne, bo CIT przychodzi raz w roku
    /// i miasto planujące z miesiąca na miesiąc miałoby jedną hossę i jedenaście głodów.
    #[must_use]
    pub fn plan_base_month(&self) -> Money {
        let suma: i64 = self.revenue_window.iter().map(|m| m.get()).sum();
        Money(suma / 12)
    }

    fn issue_loan_id(&mut self) -> magnat_economy::LoanId {
        let id = magnat_economy::LoanId(self.next_loan);
        self.next_loan += 1;
        id
    }
}

/// Plan wydatków miesiąca: udziały po uchwałach rady i to, co już jest zakontraktowane.
///
/// Dwie tablice w jednej strukturze, bo zawsze chodzą razem i zawsze pochodzą
/// z jednego miejsca ([`crate::rule::shares`] i [`crate::rule::contracted`]).
/// Osobne argumenty znaczyłyby, że da się podać jedną bez drugiej — a wtedy
/// miasto albo wydaje plan sprzed uchwały, albo płaci za odbiór odpadów dwa razy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpendPlan {
    pub shares: [u32; SPEND_CATEGORY_COUNT],
    pub contracted: [Money; SPEND_CATEGORY_COUNT],
}

/// Wynik domknięcia miesiąca budżetowego — to, co idzie do dziennika i do inspektora.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BudgetMonth {
    pub planned: Money,
    pub spent: Money,
    pub debt_service: Money,
    pub cut_bp: u32,
    pub bond_issued: Money,
}

/// Domknięcie miesiąca: obsługa długu, plan wydatków, cięcie albo emisja.
///
/// Kolejność jest kontraktem, nie wygodą: **najpierw dług**, bo kupon jest
/// zobowiązaniem, a wydatek bieżący decyzją; potem wydatki, bo to je się tnie.
/// Odwrotna kolejność znaczyłaby, że miasto wydaje pieniądze, których nie ma,
/// i dopiero potem odkrywa, że nie zapłaci odsetek.
pub fn close_month(
    budget: &mut CityBudget,
    policy: &BudgetPolicy,
    plan: &SpendPlan,
    books: &mut Books,
    rest: AccountId,
    month_revenue: Money,
    t: Tick,
) -> BudgetMonth {
    let mut rap = BudgetMonth::default();

    // Okno wpływów: miesiąc, który się właśnie skończył, wchodzi na swoje miejsce.
    let cal = magnat_core::SimCalendar::new(t);
    let slot = usize::from(cal.month_of_year().max(1) - 1) % 12;
    budget.revenue_window[slot] = month_revenue;

    rap.debt_service = obsluz_dlug(budget, books, rest, t);

    let podstawa = budget.plan_base_month();
    // Udziały przychodzą po nałożeniu uchwał rady (`Policy::SpendShare`), a nie
    // wprost z `data/city/budget.ron`: ten plik daje wartość startową, z którą
    // miasto rusza, zanim ktokolwiek zacznie rządzić.
    let udzialy = plan.shares;
    let zakontraktowane = plan.contracted;
    let plan: i64 = udzialy
        .iter()
        .map(|bp| podstawa.mul_ratio(i64::from(*bp), 10_000).get())
        .sum();
    rap.planned = Money(plan);
    if plan <= 0 {
        return rap;
    }

    // Ile wolno wydać: gotówka ponad rezerwę.
    let gotowka = books.balance(budget.account).unwrap_or(Money::ZERO).get();
    let rezerwa = podstawa
        .checked_mul_int(i64::from(policy.reserve_target_months))
        .unwrap_or(Money::ZERO)
        .get();
    let dostepne = (gotowka - rezerwa).max(0);

    let (cut_bp, emisja) = domknij_deficyt(budget, policy, books, plan, dostepne, t);
    rap.cut_bp = cut_bp;
    rap.bond_issued = emisja;
    budget.cut_bp = cut_bp;

    let gotowka = books.balance(budget.account).unwrap_or(Money::ZERO).get();
    let dostepne = (gotowka - rezerwa).max(0);
    for (i, bp) in udzialy.iter().enumerate() {
        if *bp == 0 {
            continue;
        }
        let Some(cat) = SpendCategory::from_index(i) else {
            continue;
        };
        let pelna = podstawa.mul_ratio(i64::from(*bp), 10_000);
        let po_cieciu = Money(pelna.get() - pelna.mul_ratio(i64::from(cut_bp), 10_000).get());
        // **Rezerwa na umowy z przetargów.** Usługa kupiona na zewnątrz jest
        // płacona osobnym przelewem, na konto wykonawcy — więc gdyby plan szedł
        // w całości, miasto zapłaciłoby za odbiór odpadów dwa razy: raz
        // „reszcie świata" z planu, raz firmie z umowy.
        let po_umowach = Money((po_cieciu.get() - zakontraktowane[i].get()).max(0));
        let kwota = Money(po_umowach.get().min(dostepne - rap.spent.get()).max(0));
        if kwota.get() <= 0 {
            continue;
        }
        if wydaj(budget, books, rest, cat, kwota, t) {
            rap.spent = Money(rap.spent.get() + kwota.get());
        }
    }
    rap
}

/// Wypłata z budżetu. `ponytail:` sufit — odbiorcą jest „reszta świata", bo usługi
/// publiczne, ich obsada i ich zakłady powstają dopiero w M8d. Kwota i kierunek
/// są prawdziwe, adresat jeszcze nie; droga wyjścia: konto zakładu usługowego
/// w miejsce `rest`, bez zmiany tej sygnatury.
fn wydaj(
    budget: &mut CityBudget,
    books: &mut Books,
    rest: AccountId,
    cat: SpendCategory,
    amount: Money,
    t: Tick,
) -> bool {
    let memo = TxMemo::new(
        TxKind::PublicSpend {
            program: ProgramId(cat.as_index() as u16),
        },
        DecisionReason::PublicSpend {
            category: cat,
            amount,
        },
    );
    if books
        .transfer(budget.account, rest, amount, memo, t)
        .is_err()
    {
        return false;
    }
    let s = &mut budget.spend_ytd[cat.as_index()];
    *s = Money(s.get() + amount.get());
    true
}

/// Kupon od obligacji niewykupionych i wykup tych, którym minął termin.
fn obsluz_dlug(budget: &mut CityBudget, books: &mut Books, rest: AccountId, t: Tick) -> Money {
    let mut razem = Money::ZERO;
    let obligacje: Vec<(usize, Money, u32, bool)> = budget
        .debt
        .iter()
        .enumerate()
        .filter(|(_, b)| !b.redeemed)
        .map(|(i, b)| (i, b.principal, b.coupon_bp_per_year, b.matures_at.0 <= t.0))
        .collect();
    for (i, nominal, kupon, wymagalna) in obligacje {
        let odsetki = nominal.mul_ratio(i64::from(kupon), 10_000 * 12);
        if odsetki.get() > 0 {
            let memo = TxMemo::new(
                TxKind::PublicSpend {
                    program: ProgramId(SpendCategory::DebtService.as_index() as u16),
                },
                DecisionReason::PublicSpend {
                    category: SpendCategory::DebtService,
                    amount: odsetki,
                },
            );
            if books
                .transfer(budget.account, rest, odsetki, memo, t)
                .is_ok()
            {
                let s = &mut budget.spend_ytd[SpendCategory::DebtService.as_index()];
                *s = Money(s.get() + odsetki.get());
                razem = Money(razem.get() + odsetki.get());
            }
        }
        if wymagalna {
            let loan = budget.debt[i].loan;
            // Wykup niszczy pieniądz kredytowy — druga strona `create_credit`.
            if books
                .destroy_credit(budget.account, nominal, loan, t)
                .is_ok()
            {
                budget.debt[i].redeemed = true;
                let s = &mut budget.spend_ytd[SpendCategory::DebtService.as_index()];
                *s = Money(s.get() + nominal.get());
                razem = Money(razem.get() + nominal.get());
            }
        }
    }
    razem
}

/// Zwraca `(cięcie w bp, wyemitowany nominał)`.
///
/// Cięcie idzie pierwsze, bo jest odwracalne; obligacja dopiero wtedy, gdy cięcie
/// sięgnęło dna i sufit zadłużenia jeszcze na to pozwala. Miasto, które przekroczyło
/// sufit, zostaje przy cięciu — i to jest jedyny powód, dla którego dług nie wchodzi
/// w spiralę (kryterium T5).
fn domknij_deficyt(
    budget: &mut CityBudget,
    policy: &BudgetPolicy,
    books: &mut Books,
    plan: i64,
    dostepne: i64,
    t: Tick,
) -> (u32, Money) {
    if dostepne >= plan {
        return (0, Money::ZERO);
    }
    let luka = plan - dostepne;
    // Ile z luki domyka samo cięcie.
    let max_ciecie = Money(plan)
        .mul_ratio(i64::from(policy.max_cut_bp), 10_000)
        .get();
    if luka <= max_ciecie {
        let bp = u32::try_from(luka * 10_000 / plan.max(1)).unwrap_or(policy.max_cut_bp);
        return (bp.min(policy.max_cut_bp), Money::ZERO);
    }
    let brakuje = luka - max_ciecie;
    let roczne_wplywy = budget
        .plan_base_month()
        .checked_mul_int(12)
        .unwrap_or(Money::ZERO);
    let sufit = roczne_wplywy
        .mul_ratio(i64::from(policy.debt_ceiling_bp), 10_000)
        .get();
    let zaduzenie = budget.debt_outstanding().get();
    if brakuje < policy.bond_min_principal || zaduzenie + brakuje > sufit {
        return (policy.max_cut_bp, Money::ZERO);
    }
    let nominal = Money(brakuje);
    let loan = budget.issue_loan_id();
    let powod = DecisionReason::MunicipalBondIssued {
        coupon_bp: u16::try_from(policy.bond_coupon_bp_per_year).unwrap_or(u16::MAX),
        principal: nominal,
    };
    if books
        .create_credit(budget.account, nominal, loan, powod, t)
        .is_err()
    {
        return (policy.max_cut_bp, Money::ZERO);
    }
    budget.debt.push(MunicipalBond {
        loan,
        principal: nominal,
        coupon_bp_per_year: policy.bond_coupon_bp_per_year,
        issued_at: t,
        matures_at: Tick(t.0 + u64::from(policy.bond_term_months) * 43_200),
        redeemed: false,
    });
    (policy.max_cut_bp, nominal)
}

/// Domknięcie roku budżetowego: zerowanie liczników YTD.
///
/// Dług i okno wpływów **nie** są zerowane — pierwszy przeżywa rok z definicji,
/// drugie jest oknem kroczącym i wyzerowanie go co styczeń znaczyłoby, że miasto
/// raz do roku zapomina, ile ma pieniędzy.
pub fn close_year(budget: &mut CityBudget) {
    budget.revenue_ytd = [Money::ZERO; TAX_KIND_COUNT];
    budget.spend_ytd = [Money::ZERO; SPEND_CATEGORY_COUNT];
    budget.fiscal_year = budget.fiscal_year.saturating_add(1);
}

impl HashState for CityBudget {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.account.0);
        for m in &self.revenue_ytd {
            h.write_i64(m.get());
        }
        for m in &self.revenue_life {
            h.write_i64(m.get());
        }
        for m in &self.spend_ytd {
            h.write_i64(m.get());
        }
        for m in &self.revenue_window {
            h.write_i64(m.get());
        }
        h.write_u64(self.debt.len() as u64);
        for b in &self.debt {
            b.hash_state(h);
        }
        h.write_u16(self.fiscal_year);
        h.write_u32(self.cut_bp);
        h.write_u32(self.next_loan);
    }
}
