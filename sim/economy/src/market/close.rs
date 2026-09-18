//! Domknięcie miesiąca zakładu i kapitał obrotowy firmy (szew (f′)).

use super::*;

use crate::corpfin::{ClaimOrigin, CorpFinance};

impl Market {
    /// Koszty stałe miesiąca, amortyzacja i domknięcie okresu (§5.8).
    ///
    /// Zapis księgowy powstaje wyłącznie po udanym przelewie, więc `BankCurrent`
    /// nigdy nie rozjeżdża się z saldem konta w `Books`.
    ///
    /// **Od M7d nieudany przelew nie jest ciszą.** Do tej pory sklep bez środków
    /// po prostu nie płacił i nie zostawało po tym nic: ani długu, ani śladu
    /// w księdze. Dług, którego nie ma, nie może wpędzić firmy w bankructwo ani stać
    /// się roszczeniem w postępowaniu — więc od tej chwili niezapłacona pozycja
    /// zostaje **zaległością** wobec wierzyciela i zobowiązaniem w księdze
    /// (`*Payable`). Koszt firmy jest ten sam, bo koszt powstaje w chwili, w której
    /// się należy, a nie w chwili zapłaty; zmienia się druga strona zapisu.
    pub fn close_month(&self, books: &mut Books, fin: &mut CorpFinance, t: Tick) -> Money {
        self.close_month_with(books, fin, t).0
    }

    /// To samo, co [`Market::close_month`], plus **rachunek wyniku zakładu**: utarg
    /// i koszt własny miesiąca dla każdego sklepu (M7e).
    ///
    /// Osobny podpis, a nie zmiana tamtego, bo tamten ma czternastu wołających
    /// w testach i scenariuszach, a ten dokłada wyłącznie drugi wynik. Liczby idą
    /// wprost z domknięcia okresu księgowego — `SitePnlMonth` nie liczy utargu
    /// drugi raz i nie ma jak rozjechać się z RZiS.
    pub fn close_month_with(
        &self,
        books: &mut Books,
        fin: &mut CorpFinance,
        t: Tick,
    ) -> (Money, Vec<(SiteId, u32, Money, Money)>) {
        let mut m = self.lock();
        let rest = m.rest_of_world;
        let koszty = m.data.costs;
        let miesiac = u32::try_from(t.get() / magnat_core::time::MINUTES_PER_MONTH).unwrap_or(0);
        let mut suma = Money::ZERO;
        let mut wyniki: Vec<(SiteId, u32, Money, Money)> = Vec::new();
        for i in 0..m.shops.len() {
            // Zamknięty zakład nie wynajmuje lokalu, nie zużywa prądu i nikomu
            // nie płaci — a jego księga zostaje, bo to z niej panel tłumaczy,
            // dlaczego padł (M7e WP12).
            if m.shops[i].closed {
                continue;
            }
            let (site, konto, slots) =
                (m.shops[i].site, m.shops[i].account, m.shops[i].shelf.slots);
            let (czynsz, media, place) = koszty.monthly(slots);
            let firma = m.shops[i].firm;
            // Czwarta i piąta kolumna: czym staje się ta pozycja, gdy nie ma z czego
            // jej zapłacić. Czynsz, media i płace mają w upadłości różne priorytety
            // (`ClaimPriority`), więc niezapłacona pozycja musi pamiętać, czym była.
            let pozycje: [(Money, TxKind, LedgerAccount, LedgerAccount, ClaimOrigin); 3] = [
                (
                    czynsz,
                    TxKind::Rent { site },
                    LedgerAccount::RentExpense,
                    LedgerAccount::TradePayable,
                    ClaimOrigin::Rent,
                ),
                (
                    media,
                    TxKind::Utility {
                        site,
                        kind: magnat_core::UtilityService::Electricity,
                    },
                    LedgerAccount::UtilitiesExpense,
                    LedgerAccount::TradePayable,
                    ClaimOrigin::Utility,
                ),
                (
                    place,
                    TxKind::Wage { site },
                    LedgerAccount::WagesExpense,
                    LedgerAccount::WagePayable,
                    ClaimOrigin::Wages,
                ),
            ];
            for (kwota, kind, konto_ks, konto_zob, origin) in pozycje {
                if kwota.get() <= 0 {
                    continue;
                }
                let memo = TxMemo::new(kind, DecisionReason::Unspecified);
                let zaplacone = books.transfer(konto, rest, kwota, memo, t).is_ok();
                // Koszt jest ten sam w obu gałęziach — różni się druga strona zapisu:
                // zapłacone schodzi z rachunku, niezapłacone rośnie na zobowiązaniu.
                let druga = if zaplacone {
                    (LedgerAccount::BankCurrent, Money(-kwota.get()))
                } else {
                    (konto_zob, Money(-kwota.get()))
                };
                let _ = ledger::post(
                    &mut m.shops[i].ledger,
                    JournalEntry::new(t, DecisionReason::Unspecified, &[(konto_ks, kwota), druga]),
                );
                if zaplacone {
                    suma = Money(suma.get() + kwota.get());
                } else {
                    fin.arrears_mut().accrue(
                        AccountOwner::Firm(firma),
                        AccountOwner::RestOfWorld,
                        kwota,
                        origin,
                        t,
                    );
                }
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
            suma = Money(suma.get() + m.service_working_capital(i, books, fin, t).get());
            m.maybe_borrow_working_capital(i, books, t);
            if let Ok(zamkniecie) = ledger::close_period(
                &mut m.shops[i].ledger,
                miesiac,
                t,
                DecisionReason::Unspecified,
            ) {
                wyniki.push((
                    m.shops[i].site,
                    miesiac,
                    zamkniecie.statement.revenue,
                    zamkniecie.statement.cogs,
                ));
            }
        }
        // Zakłady produkcyjne: utarg hurtowy zebrany przez `absorb_settlements`
        // (`R2-WP7`). Nie przechodzi przez `ledger::close_period`, bo zakład księgi
        // nie ma — ale wchodzi do tej samej listy, więc `post_revenue` ma jednego
        // wołającego i jedną drogę, tak samo jak przed tą naprawą.
        //
        // Zakład, który w tym miesiącu nic nie wysłał, **nie dostaje wpisu**: wpis
        // z zerowym utargiem znaczyłby w `margin_bp()` „nie wiem", czyli dokładnie
        // to, co ta naprawa usuwa.
        for (site, (utarg, koszt)) in std::mem::take(&mut m.wholesale_pnl) {
            // Sklep sprzedający hurtowo ma już wiersz z domknięcia własnej księgi —
            // ale sprzedaż B2B **nie zapisuje sprzedawcy nic w `LedgerAccount::Revenue`**,
            // więc tamten wiersz jej nie zawiera. Dopisujemy do niego, zamiast pchać
            // drugi: `post_revenue` **nadpisuje** wpis miesiąca, więc druga pozycja
            // skasowałaby utarg detaliczny kwotą samego hurtu.
            match wyniki.iter_mut().find(|(s, _, _, _)| *s == site) {
                Some(w) => {
                    w.2 = Money(w.2.get() + utarg.get());
                    w.3 = Money(w.3.get() + koszt.get());
                }
                None => wyniki.push((site, miesiac, utarg, koszt)),
            }
        }
        (suma, wyniki)
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
    fn service_working_capital(
        &mut self,
        i: usize,
        books: &mut Books,
        fin: &mut CorpFinance,
        t: Tick,
    ) -> Money {
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
                // Od M7d zaległość jest **rzeczą**, a nie samym licznikiem: bank ma
                // roszczenie, a wiek najstarszej niezapłaconej pozycji jest pierwszą
                // drogą do postępowania upadłościowego (M7d §5.13).
                self.arrear_for_loan(fin, i, id, rata.interest, t);
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
                self.arrear_for_loan(fin, i, id, rata.principal, t);
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

    /// Zaległość wobec banku z niezapłaconej raty.
    ///
    /// Osobno, bo woła ją i gałąź odsetkowa, i kapitałowa — a dwie kopie tych pięciu
    /// linii rozjechałyby się przy pierwszej zmianie strony wierzyciela.
    fn arrear_for_loan(
        &mut self,
        fin: &mut CorpFinance,
        i: usize,
        id: crate::books::LoanId,
        kwota: Money,
        t: Tick,
    ) {
        let Some(bank) = self.bank else {
            return;
        };
        fin.arrears_mut().accrue(
            AccountOwner::Firm(self.shops[i].firm),
            AccountOwner::Bank(bank.firm),
            kwota,
            ClaimOrigin::Loan(id),
            t,
        );
    }

    /// Wniosek o kredyt obrotowy, kiedy na rachunku zostało mniej niż miesiąc kosztów.
    ///
    /// Miara jest celowo prosta i jawna: sklep pożycza pod **zapasy i koszty stałe**,
    /// nie pod inwestycję (ta jest w M7). Ocena idzie przez DSCR liczone z księgi.
    fn maybe_borrow_working_capital(&mut self, i: usize, books: &mut Books, t: Tick) {
        let konto = self.shops[i].account;
        let slots = self.shops[i].shelf.slots;
        let (czynsz, media, place) = self.data.costs.monthly(slots);
        let miesieczne = Money(czynsz.get() + media.get() + place.get());
        let saldo = books.balance(konto).unwrap_or(Money::ZERO);
        if saldo.get() >= miesieczne.get() {
            return;
        }
        let _ = self.borrow_working_capital(i, books, t);
    }

    /// Sam wniosek, bez progu „zostało mniej niż miesiąc kosztów".
    ///
    /// Osobno, bo próg jest **powodem, dla którego sklep pyta**, a nie częścią
    /// pytania: gracz pyta, bo tak zdecydował, i ma dostać tę samą ocenę tym samym
    /// wzorem. Dwie ścieżki do jednego banku rozjechałyby się przy pierwszej zmianie
    /// widełek (`K-11` w wydaniu kredytowym).
    pub(crate) fn borrow_working_capital(
        &mut self,
        i: usize,
        books: &mut Books,
        t: Tick,
    ) -> Result<crate::books::LoanId, RejectCredit> {
        let Some(bank) = self.bank else {
            return Err(RejectCredit::NoLender);
        };
        // Zamknięty sklep nie obsługuje kredytu: dobowa pętla pomija go przy
        // `service_working_capital`, więc rata nigdy by nie wyszła. Bez tego
        // strażnika zakład zamknięty przez UOKiK (`sim/city::law`) dostawałby
        // z panelu darmową gotówkę i zobowiązanie widmo.
        if self.shops[i].closed {
            return Err(RejectCredit::NoLender);
        }
        if self.shops[i].loan.is_some() {
            return Err(RejectCredit::DscrTooLow);
        }
        let (site, konto, slots) = (
            self.shops[i].site,
            self.shops[i].account,
            self.shops[i].shelf.slots,
        );
        let (czynsz, media, place) = self.data.costs.monthly(slots);
        let miesieczne = Money(czynsz.get() + media.get() + place.get());
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
            let CreditDecision::Rejected { cause, .. } = decyzja else {
                unreachable!("decyzja kredytowa ma dwa warianty")
            };
            return Err(cause);
        };
        // **Najpierw pieniądz, potem wpis do rejestru.** Odwrotna kolejność zostawiała
        // przy nieudanej kreacji depozytu kredyt w `self.loans`, którego nikt nie ma
        // i nikt nie spłaca — a gracz może ponawiać wniosek z panelu, więc osieroconych
        // wpisów byłoby tyle, ile kliknięć. `LoanId` nadaje się z długości rejestru,
        // więc podglądnięcie go przed wstawieniem jest tanie i dokładne.
        let id = crate::books::LoanId(u32::try_from(self.loans.len()).unwrap_or(u32::MAX));
        if books.create_credit(konto, limit, id, reason, t).is_err() {
            return Err(RejectCredit::NoLender);
        }
        let id = self.loans.open(
            AccountOwner::Firm(self.shops[i].firm),
            bank.firm,
            LoanKind::WorkingCapital,
            limit,
            rate_bp,
            produkt.term_months,
            Tick(t.get() + crate::credit::TICKS_PER_MONTH),
        );
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
        Ok(id)
    }
}

impl Market {
    /// Wniosek o kredyt obrotowy złożony **przez gracza** z panelu Finanse.
    ///
    /// Ta sama ocena, ten sam bank i ta sama księga co przy wniosku, który sklep
    /// składa sam przy pustej kasie — różni się wyłącznie tym, kto nacisnął.
    ///
    /// # Errors
    /// [`RejectCredit`] — powód odmowy, ten sam, który trafia do dziennika decyzji.
    pub fn request_working_capital(
        &self,
        books: &mut Books,
        site: SiteId,
        t: Tick,
    ) -> Result<crate::books::LoanId, RejectCredit> {
        let mut m = self.lock();
        let i = m
            .by_site
            .get(&site)
            .copied()
            .ok_or(RejectCredit::NoLender)? as usize;
        m.borrow_working_capital(i, books, t)
    }
}
