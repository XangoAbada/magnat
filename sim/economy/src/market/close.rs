//! Domknięcie miesiąca zakładu i kapitał obrotowy firmy (szew (f′)).

use super::*;

impl Market {
    /// Koszty stałe miesiąca, amortyzacja i domknięcie okresu (§5.8).
    ///
    /// Sklep bez środków **nie płaci** i to jest cała „upadłość" w M5 — postępowanie
    /// prowadzi M7 (`K-10`). Zapis księgowy powstaje wyłącznie po udanym przelewie,
    /// więc `BankCurrent` nigdy nie rozjeżdża się z saldem konta w `Books`.
    pub fn close_month(&self, books: &mut Books, t: Tick) -> Money {
        let mut m = self.lock();
        let rest = m.rest_of_world;
        let koszty = m.data.costs;
        let miesiac = u32::try_from(t.get() / magnat_core::time::MINUTES_PER_MONTH).unwrap_or(0);
        let mut suma = Money::ZERO;
        for i in 0..m.shops.len() {
            let (site, konto, slots) =
                (m.shops[i].site, m.shops[i].account, m.shops[i].shelf.slots);
            let (czynsz, media, place) = koszty.monthly(slots);
            let pozycje: [(Money, TxKind, LedgerAccount); 3] = [
                (czynsz, TxKind::Rent { site }, LedgerAccount::RentExpense),
                (
                    media,
                    TxKind::Utility {
                        site,
                        kind: magnat_core::UtilityService::Electricity,
                    },
                    LedgerAccount::UtilitiesExpense,
                ),
                (place, TxKind::Wage { site }, LedgerAccount::WagesExpense),
            ];
            for (kwota, kind, konto_ks) in pozycje {
                if kwota.get() <= 0 {
                    continue;
                }
                let memo = TxMemo::new(kind, DecisionReason::Unspecified);
                if books.transfer(konto, rest, kwota, memo, t).is_err() {
                    continue;
                }
                let _ = ledger::post(
                    &mut m.shops[i].ledger,
                    JournalEntry::new(
                        t,
                        DecisionReason::Unspecified,
                        &[
                            (konto_ks, kwota),
                            (LedgerAccount::BankCurrent, Money(-kwota.get())),
                        ],
                    ),
                );
                suma = Money(suma.get() + kwota.get());
            }
            // Amortyzacja jest kosztem **bezgotówkowym** — nie ma po niej przelewu
            // i dlatego nie może iść tą samą ścieżką co czynsz.
            let odpis = m.shops[i].depreciation_monthly;
            if odpis.get() > 0 {
                let _ = ledger::post(
                    &mut m.shops[i].ledger,
                    JournalEntry::new(
                        t,
                        DecisionReason::Unspecified,
                        &[
                            (LedgerAccount::DepreciationExpense, odpis),
                            (LedgerAccount::AccumDepreciation, Money(-odpis.get())),
                        ],
                    ),
                );
            }
            // Kredyt obrotowy **przed** domknięciem okresu: odsetki zaksięgowane po
            // `close_period` wpadłyby do następnego miesiąca i RZiS przestałby się
            // zgadzać z przepływami (korekta wpisana do M5d po M5c).
            suma = Money(suma.get() + m.service_working_capital(i, books, t).get());
            m.maybe_borrow_working_capital(i, books, t);
            let _ = ledger::close_period(
                &mut m.shops[i].ledger,
                miesiac,
                t,
                DecisionReason::Unspecified,
            );
        }
        suma
    }
}

impl MarketInner {
    /// Obsługa kredytu obrotowego zakładu: odsetki i kapitał, każde z własnym
    /// zapisem w księdze. Zwraca kwotę, która wyszła z rachunku.
    ///
    /// **Każdy przelew zakładu ma swój zapis w księdze.** `LedgerAccount::BankCurrent`
    /// jest lustrem salda rachunku w `Books` i test WP7 sprawdza tę równość co do
    /// grosza — uruchomienie i spłata kredytu ruszają trzy rzeczy naraz (podaż
    /// pieniądza, saldo rachunku, księgę) i pominięcie trzeciej wychodzi dopiero
    /// w teście M5c, daleko od przyczyny.
    fn service_working_capital(&mut self, i: usize, books: &mut Books, t: Tick) -> Money {
        let Some(bank) = self.bank else {
            return Money::ZERO;
        };
        let Some(id) = self.shops[i].loan else {
            return Money::ZERO;
        };
        let Some(rata) = self
            .loans
            .get(id)
            .and_then(crate::credit::Loan::next_installment)
        else {
            return Money::ZERO;
        };
        let konto = self.shops[i].account;
        let mut wyszlo = Money::ZERO;
        if rata.interest.get() > 0 {
            let memo = TxMemo::new(
                TxKind::LoanPayment {
                    loan: id,
                    principal: Money::ZERO,
                    interest: rata.interest,
                },
                DecisionReason::Unspecified,
            );
            if books
                .transfer(konto, bank.account, rata.interest, memo, t)
                .is_err()
            {
                // Sklep bez środków nie płaci — zaległość, nie debet bez pokrycia.
                if let Some(l) = self.loans.get_mut(id) {
                    l.arrears_months = l.arrears_months.saturating_add(1);
                }
                return Money::ZERO;
            }
            let _ = ledger::post(
                &mut self.shops[i].ledger,
                JournalEntry::new(
                    t,
                    DecisionReason::Unspecified,
                    &[
                        (LedgerAccount::InterestExpense, rata.interest),
                        (LedgerAccount::BankCurrent, Money(-rata.interest.get())),
                    ],
                ),
            );
            wyszlo = Money(wyszlo.get() + rata.interest.get());
        }
        if rata.principal.get() > 0 {
            if books.destroy_credit(konto, rata.principal, id, t).is_err() {
                if let Some(l) = self.loans.get_mut(id) {
                    l.arrears_months = l.arrears_months.saturating_add(1);
                }
                return wyszlo;
            }
            let _ = ledger::post(
                &mut self.shops[i].ledger,
                JournalEntry::new(
                    t,
                    DecisionReason::Unspecified,
                    &[
                        (LedgerAccount::LoansShort, rata.principal),
                        (LedgerAccount::BankCurrent, Money(-rata.principal.get())),
                    ],
                ),
            );
            if let Some(l) = self.loans.get_mut(id) {
                l.outstanding = Money(l.outstanding.get() - rata.principal.get());
                l.paid_months += 1;
            }
            wyszlo = Money(wyszlo.get() + rata.principal.get());
        }
        if self
            .loans
            .get(id)
            .is_some_and(crate::credit::Loan::is_closed)
        {
            self.shops[i].loan = None;
        }
        wyszlo
    }

    /// Wniosek o kredyt obrotowy, kiedy na rachunku zostało mniej niż miesiąc kosztów.
    ///
    /// Miara jest celowo prosta i jawna: sklep pożycza pod **zapasy i koszty stałe**,
    /// nie pod inwestycję (ta jest w M7). Ocena idzie przez DSCR liczone z księgi.
    fn maybe_borrow_working_capital(&mut self, i: usize, books: &mut Books, t: Tick) {
        let Some(bank) = self.bank else {
            return;
        };
        if self.shops[i].loan.is_some() {
            return;
        }
        let (site, konto, slots) = (
            self.shops[i].site,
            self.shops[i].account,
            self.shops[i].shelf.slots,
        );
        let (czynsz, media, place) = self.data.costs.monthly(slots);
        let miesieczne = Money(czynsz.get() + media.get() + place.get());
        let saldo = books.balance(konto).unwrap_or(Money::ZERO);
        if saldo.get() >= miesieczne.get() {
            return;
        }
        let params = self.data.bank;
        let produkt = params.products.working_capital;
        let kwota = Money(miesieczne.get().saturating_mul(3));
        let rata = crate::kernel::annuity_payment(
            kwota,
            crate::credit::monthly_rate_bp(self.cpi.base_rate().bp + produkt.spread_bp),
            produkt.term_months,
        );
        let od = Tick(t.get().saturating_sub(12 * crate::credit::TICKS_PER_MONTH));
        let rzis = ledger::income_statement(&self.shops[i].ledger, od, t);
        // EBITDA to wynik **przed** amortyzacją i odsetkami — obie pozycje wracają
        // do wyniku, bo kredyt spłaca się z gotówki, a nie z zysku księgowego.
        let ebitda = Money(rzis.net_result().get() + rzis.depreciation.get() + rzis.interest.get());
        let miesiecy = u16::try_from(
            (t.get().saturating_sub(self.shops[i].opened.get())) / crate::credit::TICKS_PER_MONTH,
        )
        .unwrap_or(u16::MAX);
        let app = LoanApplication {
            kind: LoanKind::WorkingCapital,
            amount: kwota,
            income_monthly: Money::ZERO,
            existing_service: Money::ZERO,
            ebitda_12m: ebitda,
            debt_service_12m: Money(rata.get().saturating_mul(12)),
            months_in_business: miesiecy,
            arrears_months: 0,
            key: site.entity().index(),
        };
        let decyzja = assess_credit(&app, &params, self.cpi.base_rate(), self.seed, t);
        let reason = decyzja.reason();
        self.log_budget(site.entity().index(), reason);
        let CreditDecision::Approved { limit, rate_bp, .. } = decyzja else {
            return;
        };
        let id = self.loans.open(
            AccountOwner::Firm(self.shops[i].firm),
            bank.firm,
            LoanKind::WorkingCapital,
            limit,
            rate_bp,
            produkt.term_months,
            Tick(t.get() + crate::credit::TICKS_PER_MONTH),
        );
        if books.create_credit(konto, limit, id, reason, t).is_err() {
            return;
        }
        let _ = ledger::post(
            &mut self.shops[i].ledger,
            JournalEntry::new(
                t,
                reason,
                &[
                    (LedgerAccount::BankCurrent, limit),
                    (LedgerAccount::LoansShort, Money(-limit.get())),
                ],
            ),
        );
        self.shops[i].loan = Some(id);
    }
}
