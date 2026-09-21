//! Miesiąc gospodarstwa, kredyt konsumencki i koszyk CPI (szew (f)).

use super::*;

impl Market {
    // ── budżety, bank i kredyt (M5d) ─────────────────────────────────────────────

    /// Otwiera bank miasta. W M5 jest jeden; drugie wywołanie go podmienia.
    pub fn open_bank(&self, firm: FirmId, account: AccountId) {
        self.lock().bank = Some(Bank { firm, account });
    }

    #[must_use]
    pub fn bank(&self) -> Option<Bank> {
        self.lock().bank
    }

    /// Budżet gospodarstwa — kopia, bo wołający nie trzyma zamka.
    #[must_use]
    pub fn budget_of(&self, household: u32) -> HouseholdBudget {
        self.lock().budget_of(household)
    }

    #[must_use]
    pub fn loan(&self, id: LoanId) -> Option<crate::credit::Loan> {
        self.lock().loans.get(id).cloned()
    }

    /// Spłaca kredyt gospodarstwa z masy spadkowej i zwraca, ile z niej poszło
    /// (`R2-WP10`).
    ///
    /// Wołane wtedy i tylko wtedy, gdy gospodarstwo przestaje istnieć: dopóki żyje,
    /// kredyt ma dłużnika i idzie harmonogramem. Kwota wchodzi do ksiąg kanałem
    /// sektora gospodarstw — masa spadkowa jest po stronie komponentów — a dopiero
    /// z konta banku znika jako pieniądz kredytowy. Ta sama kolejność co w racie
    /// miesięcznej i z tego samego powodu: `destroy_credit` działa na kontach.
    ///
    /// Reszty, na którą masy nie starczyło, **nikt nie spłaci**: gospodarstwa nie ma,
    /// więc kredyt zamyka się jako strata banku. To jest uczciwsza odpowiedź niż
    /// zobowiązanie bytu, którego nie ma — a stratę widać, bo `credit_repaid` zostaje
    /// mniejsze od `credit_created` i niezmiennik P1 nadal się domyka.
    pub fn settle_household_loan(
        &self,
        household: u32,
        available: Money,
        books: &mut Books,
        t: Tick,
    ) -> Money {
        let mut m = self.lock();
        let Some(bank) = m.bank else {
            return Money::ZERO;
        };
        let Some(id) = m.budget_of(household).loan else {
            return Money::ZERO;
        };
        let Some(zostalo) = m.loans.get(id).map(|l| l.outstanding) else {
            return Money::ZERO;
        };
        let kwota = Money(zostalo.get().min(available.get().max(0)));
        if kwota.get() > 0 {
            let memo = TxMemo::new(
                TxKind::LoanPayment {
                    loan: id,
                    principal: kwota,
                    interest: Money::ZERO,
                },
                DecisionReason::Unspecified,
            );
            if books.household_pay(bank.account, kwota, memo, t).is_err() {
                return Money::ZERO;
            }
            let _ = books.destroy_credit(bank.account, kwota, id, t);
        }
        if let Some(l) = m.loans.get_mut(id) {
            l.outstanding = Money::ZERO;
            l.arrears_months = 0;
        }
        kwota
    }

    #[must_use]
    pub fn loan_count(&self) -> usize {
        self.lock().loans.len()
    }

    /// Niespłacony kapitał wszystkich kredytów — tyle pieniądza kredytowego krąży.
    #[must_use]
    pub fn credit_outstanding(&self) -> Money {
        self.lock().loans.outstanding_total()
    }

    /// Okno ostatnich decyzji budżetowych i kredytowych (podgląd, nie historia).
    #[must_use]
    pub fn budget_log(&self) -> Vec<(u32, DecisionReason)> {
        self.lock().budget_log.clone()
    }

    #[must_use]
    pub fn cpi_index_bp(&self) -> IndexBp {
        self.lock().cpi.index_bp()
    }

    #[must_use]
    pub fn cpi_yoy_bp(&self) -> Option<i32> {
        self.lock().cpi.yoy_bp()
    }

    #[must_use]
    pub fn cpi_mom_bp(&self) -> Option<i32> {
        self.lock().cpi.mom_bp()
    }

    #[must_use]
    pub fn base_rate(&self) -> BaseRate {
        self.lock().cpi.base_rate()
    }

    /// Zamyka dobę koszyka CPI. Woła to pętla doby, po zaopatrzeniu — wtedy wszystkie
    /// transakcje doby są już zaksięgowane.
    pub fn roll_cpi_day(&self) {
        self.lock().cpi.roll_day();
    }

    /// Zamyka miesiąc CPI i przestawia stopę bazową banku centralnego.
    pub fn close_cpi_month(&self, t: Tick) -> BaseRate {
        let mut m = self.lock();
        // `BankParams` jest `Copy`, więc kopia zdejmuje kolizję pożyczek (`&m.data`
        // obok `&mut m.cpi`) bez przebudowy struktury.
        let params = m.data.bank;
        m.cpi.close_month(&params, t)
    }

    /// Miesięczne rozliczenie gospodarstw: plan budżetu, koszty stałe, rata kredytu,
    /// wniosek kredytowy przy niedoborze i zaległość przy odmowie (§5.9).
    ///
    /// Kolejność jest **ścieżką wyjaśnienia** z kryterium WP8: debet → wniosek →
    /// odmowa → zaległość. Odwrócenie jej dałoby zaległość, której nikt nie próbował
    /// uniknąć, czyli kartę inspekcji bez pierwszego ogniwa.
    pub fn household_month(
        &self,
        rows: &mut [HouseholdMonth],
        books: &mut Books,
        t: Tick,
    ) -> HouseholdMonthReport {
        let mut m = self.lock();
        m.household_month(rows, books, t)
    }
}

impl MarketInner {
    /// Dopisuje powód do okna podglądu. Pierścień, nie historia — pełna kronika
    /// decyzji należy do M9.
    pub(super) fn log_budget(&mut self, household: u32, reason: DecisionReason) {
        if self.budget_log.len() >= BUDGET_LOG_RING {
            self.budget_log.remove(0);
        }
        self.budget_log.push((household, reason));
    }

    /// Zdejmuje kwotę z gospodarstwa: najpierw rachunek, potem gotówka. Zwraca, ile
    /// udało się zdjąć — reszta jest niedoborem, nie debetem.
    fn take_from_household(row: &mut HouseholdMonth, amount: Money) -> Money {
        let z_banku = row.bank.get().min(amount.get()).max(0);
        row.bank = Money(row.bank.get() - z_banku);
        let brakuje = amount.get() - z_banku;
        let z_gotowki = row.cash.get().min(brakuje).max(0);
        row.cash = Money(row.cash.get() - z_gotowki);
        Money(z_banku + z_gotowki)
    }

    /// Rata kredytu gospodarstwa: kapitał niszczy pieniądz, odsetki są przelewem.
    ///
    /// Kolejność jest wymuszona przez `Books`: `destroy_credit` działa na kontach,
    /// więc kapitał musi **najpierw** wejść do ksiąg kanałem sektora gospodarstw,
    /// a dopiero z konta banku zniknąć. To jedno dodatkowe wywołanie, nie inna
    /// mechanika (korekta wpisana do M5d po M5b).
    fn pay_installment(
        &mut self,
        row: &mut HouseholdMonth,
        books: &mut Books,
        rep: &mut HouseholdMonthReport,
        t: Tick,
    ) -> Money {
        let Some(bank) = self.bank else {
            return Money::ZERO;
        };
        let Some(id) = self.budget_of(row.index).loan else {
            return Money::ZERO;
        };
        let Some(rata) = self
            .loans
            .get(id)
            .and_then(crate::credit::Loan::next_installment)
        else {
            return Money::ZERO;
        };
        let nalezne = Money(rata.principal.get() + rata.interest.get());
        let zaplacone = MarketInner::take_from_household(row, nalezne);
        if zaplacone.get() < nalezne.get() {
            // Niedopłata raty nie dzieli się na kapitał i odsetki — bank widzi
            // zaległy miesiąc, a nie część raty. Kwota wraca do gospodarstwa.
            row.bank = Money(row.bank.get() + zaplacone.get());
            if let Some(l) = self.loans.get_mut(id) {
                l.arrears_months = l.arrears_months.saturating_add(1);
            }
            return Money::ZERO;
        }
        let memo = TxMemo::new(
            TxKind::LoanPayment {
                loan: id,
                principal: rata.principal,
                interest: rata.interest,
            },
            DecisionReason::Unspecified,
        );
        if books.household_pay(bank.account, nalezne, memo, t).is_err() {
            row.bank = Money(row.bank.get() + nalezne.get());
            return Money::ZERO;
        }
        // Kapitał znika z obiegu; odsetki zostają na koncie banku jako jego przychód.
        if rata.principal.get() > 0 {
            let _ = books.destroy_credit(bank.account, rata.principal, id, t);
        }
        if let Some(l) = self.loans.get_mut(id) {
            l.outstanding = Money(l.outstanding.get() - rata.principal.get());
            l.paid_months += 1;
        }
        rep.installments_paid += 1;
        rep.interest_paid = Money(rep.interest_paid.get() + rata.interest.get());
        rep.principal_repaid = Money(rep.principal_repaid.get() + rata.principal.get());
        if self
            .loans
            .get(id)
            .is_some_and(crate::credit::Loan::is_closed)
        {
            if let Some(b) = self.budgets.get_mut(row.index as usize) {
                b.loan = None;
            }
        }
        nalezne
    }

    /// Wniosek o kredyt konsumpcyjny przy niedoborze na koszty stałe.
    fn apply_for_credit(
        &mut self,
        row: &mut HouseholdMonth,
        books: &mut Books,
        rep: &mut HouseholdMonthReport,
        gap: Money,
        t: Tick,
    ) {
        rep.credit_applications += 1;
        let Some(bank) = self.bank else {
            let r = DecisionReason::CreditRejected {
                kind: LoanKind::Consumer,
                cause: RejectCredit::NoLender,
                margin_bp: 0,
            };
            row.credit = Some(r);
            self.log_budget(row.index, r);
            return;
        };
        let b = self.budget_of(row.index);
        let app = LoanApplication {
            kind: LoanKind::Consumer,
            // Prosimy o niedobór tego miesiąca razy trzy — kredyt na jedną ratę nie
            // rozwiązuje niczego, bo w przyszłym miesiącu brakuje tyle samo.
            amount: Money(gap.get().saturating_mul(3)),
            income_monthly: row.income,
            existing_service: b.fixed[FixedCost::LoanService.as_index()],
            ebitda_12m: Money::ZERO,
            debt_service_12m: Money::ZERO,
            months_in_business: 0,
            arrears_months: b.arrears_months,
            key: row.index,
        };
        let params = self.data.bank;
        let decyzja = assess_credit(&app, &params, self.cpi.base_rate(), self.seed, t);
        let reason = decyzja.reason();
        row.credit = Some(reason);
        self.log_budget(row.index, reason);
        let CreditDecision::Approved { limit, rate_bp, .. } = decyzja else {
            return;
        };
        if limit.get() <= 0 {
            return;
        }
        let id = self.loans.open(
            AccountOwner::Household(HouseholdId(magnat_core::Entity::new(
                row.index,
                std::num::NonZeroU32::MIN,
            ))),
            bank.firm,
            LoanKind::Consumer,
            limit,
            rate_bp,
            params.products.consumer.term_months,
            Tick(t.get() + crate::credit::TICKS_PER_MONTH),
        );
        // Kreacja pieniądza: depozyt powstaje na koncie banku, a stamtąd kanałem
        // sektora gospodarstw wchodzi do komponentu.
        if books
            .create_credit(bank.account, limit, id, reason, t)
            .is_err()
        {
            return;
        }
        let memo = TxMemo::new(TxKind::LoanDraw { loan: id }, reason);
        if books
            .household_receive(bank.account, limit, memo, t)
            .is_err()
        {
            let _ = books.destroy_credit(bank.account, limit, id, t);
            return;
        }
        row.bank = Money(row.bank.get() + limit.get());
        if let Some(b) = self.budgets.get_mut(row.index as usize) {
            b.loan = Some(id);
        }
        rep.credit_granted += 1;
        rep.credit_amount = Money(rep.credit_amount.get() + limit.get());
    }

    fn household_month(
        &mut self,
        rows: &mut [HouseholdMonth],
        books: &mut Books,
        t: Tick,
    ) -> HouseholdMonthReport {
        let mut rep = HouseholdMonthReport::default();
        let rest = self.rest_of_world;
        for row in rows.iter_mut() {
            let i = row.index as usize;
            if self.budgets.len() <= i {
                self.budgets.resize(i + 1, HouseholdBudget::default());
            }
            let rata = self
                .budget_of(row.index)
                .loan
                .and_then(|id| self.loans.get(id))
                .map_or(Money::ZERO, crate::credit::Loan::monthly_service);

            let mut b = self.budgets[i];
            let plan = plan_budget(&mut b, &row.profile, row.income, rata, &self.data, t);
            self.budgets[i] = b;
            rep.planned += 1;

            // Rata idzie pierwsza: bank jest wierzycielem uprzywilejowanym, a jej
            // niezapłacenie ma inny skutek niż niezapłacenie czynszu — zaległość
            // kredytowa psuje scoring na dwadzieścia cztery miesiące.
            let _ = self.pay_installment(row, books, &mut rep, t);

            // Niedobór na pozostałe koszty stałe uruchamia wniosek — **zanim**
            // cokolwiek zostanie niezapłacone. To jest pierwsze ogniwo ścieżki
            // z kryterium WP8: debet → wniosek → odmowa → zaległość.
            let pozostale = Money(
                self.budgets[i].fixed_total().get()
                    - self.budgets[i].fixed[FixedCost::LoanService.as_index()].get(),
            );
            let dostepne = row.cash.get() + row.bank.get();
            if pozostale.get() > dostepne && self.budget_of(row.index).loan.is_none() {
                self.apply_for_credit(row, books, &mut rep, Money(pozostale.get() - dostepne), t);
            }

            // Koszty stałe w kolejności `FixedCost`. Każda niedopłata zostawia
            // zaległość i powód — bez tego karta inspekcji urywa się na odmowie.
            for k in 0..FIXED_COST_COUNT {
                if k == FixedCost::LoanService.as_index() {
                    continue;
                }
                let kwota = self.budgets[i].fixed[k];
                if kwota.get() <= 0 {
                    continue;
                }
                let zaplacone = MarketInner::take_from_household(row, kwota);
                if zaplacone.get() > 0 {
                    let memo = TxMemo::new(
                        TxKind::Rent {
                            site: SiteId(magnat_core::Entity::new(
                                row.index,
                                std::num::NonZeroU32::MIN,
                            )),
                        },
                        DecisionReason::Unspecified,
                    );
                    if books.household_pay(rest, zaplacone, memo, t).is_err() {
                        row.bank = Money(row.bank.get() + zaplacone.get());
                        continue;
                    }
                    rep.fixed_paid = Money(rep.fixed_paid.get() + zaplacone.get());
                }
                let brak = kwota.get() - zaplacone.get();
                if brak <= 0 {
                    continue;
                }
                let cost = FixedCost::ALL[k];
                let r = DecisionReason::BudgetShortfall {
                    cost,
                    gap_permille: i16::try_from(brak * 1_000 / kwota.get().max(1)).unwrap_or(1_000),
                };
                if row.unpaid.is_none() {
                    row.unpaid = Some(r);
                }
                self.log_budget(row.index, r);
                row.shortfall = Money(row.shortfall.get() + brak);
                rep.arrears_added = Money(rep.arrears_added.get() + brak);
            }
            if row.shortfall.get() > 0 {
                rep.shortfalls += 1;
                let b = &mut self.budgets[i];
                b.arrears = Money(b.arrears.get().saturating_add(row.shortfall.get()));
                b.arrears_months = b.arrears_months.saturating_add(1);
            } else {
                // Miesiąc bez zaległości spłaca historię kredytową o jeden krok.
                let b = &mut self.budgets[i];
                b.arrears_months = b.arrears_months.saturating_sub(1);
            }

            // Oszczędności przenoszą się **wewnątrz** gospodarstwa, więc nie ruszają
            // ksiąg: `bank` i `savings` są po tej samej stronie kanału sektora.
            let odlozone = plan.savings.get().min(row.bank.get()).max(0);
            row.bank = Money(row.bank.get() - odlozone);
            row.savings = Money(row.savings.get() + odlozone);
            rep.savings = Money(rep.savings.get() + odlozone);
        }
        rep
    }

    /// Budżet gospodarstwa; domyślny (`planned == false`) dla nieznanego indeksu.
    fn budget_of(&self, household: u32) -> HouseholdBudget {
        self.budgets
            .get(household as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Przekroczenie koperty kategorii — wejście członu `k_envelope` w progu (§5.4).
    pub(super) fn overspend_bp(&self, household: u32, cat: StockCat) -> i32 {
        match self.budgets.get(household as usize) {
            Some(b) if b.planned => b.envelope(cat).overspend_bp(),
            _ => 0,
        }
    }
}
