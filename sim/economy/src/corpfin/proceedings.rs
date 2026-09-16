//! Postępowanie upadłościowe w ruchu: od progu niewypłacalności do podziału masy
//! (M7d WP9, `K-10`).
//!
//! Stan postępowania i **czysty** plan podziału są w [`super::bankruptcy`]; tutaj jest
//! to, co ten plan wykonuje na kontach, i to, co go otwiera. Podział przebiega między
//! „co się należy" a „co faktycznie poszło przelewem" — bo pierwsze da się sprawdzić
//! na dziesięciu tysiącach losowych konfiguracji bez budowania świata, a drugie nie.

use magnat_core::{
    BankruptcyTrigger, CitizenId, ClaimPriority, DecisionReason, FirmId, Money, Tick,
};

use crate::books::{AccountId, AccountOwner, Books, TxKind, TxMemo};
use crate::corpfin::arrears::{Arrear, ArrearId, ClaimOrigin};
use crate::corpfin::bankruptcy::{
    AssetLot, BankruptcyId, BankruptcyStage, Claim, ClaimId, Distribution, LotFate,
};
use crate::corpfin::instruments::AssetRef;
use crate::corpfin::{Bankruptcy, CorpFinance, SectorPayout};
use crate::credit::LoanBook;

impl CorpFinance {
    // ── niewypłacalność (WP9) ────────────────────────────────────────────────────

    /// Dobowa aktualizacja licznika niewypłacalności i sprawdzenie wyzwalacza.
    ///
    /// Zwraca wyzwalacz wraz z liczbą dób, jeśli firma właśnie przekroczyła próg.
    /// Sam **nie otwiera** postępowania — otwarcie jest osobnym krokiem, żeby
    /// wołający mógł najpierw zebrać to, co do masy wchodzi.
    pub fn check_insolvency(
        &mut self,
        firm: FirmId,
        equity: Money,
        loan_arrears_months: u8,
        t: Tick,
    ) -> Option<(BankruptcyTrigger, u16)> {
        if self.open_case.contains_key(&firm.entity().index()) {
            return None;
        }
        let próg = Money(self.params.trigger.illiquid_min_gr);
        let dni = self
            .arrears
            .oldest_overdue_days(AccountOwner::Firm(firm), próg, t);
        self.illiquid_days.insert(firm.entity().index(), dni);
        if dni >= self.params.trigger.illiquid_days {
            return Some((BankruptcyTrigger::Illiquid, dni));
        }
        if equity.get() < 0
            && loan_arrears_months >= self.params.trigger.negative_equity_arrears_months
        {
            return Some((BankruptcyTrigger::NegativeEquity, dni));
        }
        None
    }

    /// Otwiera postępowanie i przenosi do niego **wszystkie** otwarte zaległości
    /// firmy jako roszczenia.
    ///
    /// Przeniesienie jest tu, a nie u wołającego, i to jest istotne: roszczenie,
    /// które ktoś musiałby zgłosić ręcznie, prędzej czy później nie zostanie
    /// zgłoszone — a wtedy dług cicho zniknie razem z firmą i test zachowania
    /// pieniądza nic nie zauważy, bo pieniądza w nim nie było.
    pub fn open_bankruptcy(
        &mut self,
        firm: FirmId,
        account: AccountId,
        trigger: BankruptcyTrigger,
        days: u16,
        loans: &LoanBook,
        t: Tick,
    ) -> BankruptcyId {
        let id = BankruptcyId(u32::try_from(self.cases.len()).unwrap_or(u32::MAX));
        let mut b = Bankruptcy::new(
            id,
            firm,
            account,
            trigger,
            self.params.estate.trustee_fee_bp,
            t,
        );
        b.trigger_days = days;
        let moje: Vec<Arrear> = self
            .arrears
            .of_debtor(AccountOwner::Firm(firm))
            .copied()
            .collect();
        for a in &moje {
            let _ = b.file_claim(a.creditor, a.amount, a.origin, None, Some(a.id), t);
        }
        // Niespłacony kapitał kredytów firmy jest długiem, choć nie jest jeszcze
        // zaległością: rata przyszłego miesiąca nigdy nie zapadnie, bo firmy
        // wtedy nie będzie. Bez tego bank traciłby cały kapitał bez roszczenia.
        for l in loans.iter() {
            if l.borrower != AccountOwner::Firm(firm) || l.outstanding.get() <= 0 {
                continue;
            }
            let _ = b.file_claim(
                AccountOwner::Bank(l.lender),
                l.outstanding,
                ClaimOrigin::Loan(l.id),
                None,
                None,
                t,
            );
        }
        // Zaległe raty leasingowe zostały już wciągnięte jako zaległości; tutaj
        // kończymy same umowy, bo rzecz wraca do właściciela i do masy nie wchodzi.
        for l in &mut self.leases {
            if l.lessee == firm && !l.ended {
                l.ended = true;
            }
        }
        self.cases.push(b);
        self.open_case.insert(firm.entity().index(), id);
        self.illiquid_days.remove(&firm.entity().index());
        id
    }

    /// Zgłasza roszczenie do wskazanego postępowania. Opakowanie nad
    /// [`Bankruptcy::file_claim`] dla wołających, którzy trzymają `BankruptcyId`,
    /// a nie referencję — czyli dla wszystkich spoza tego modułu.
    pub fn file_claim_in(
        &mut self,
        case: BankruptcyId,
        creditor: AccountOwner,
        amount: Money,
        origin: ClaimOrigin,
        arrear: Option<ArrearId>,
        t: Tick,
    ) -> Option<ClaimId> {
        self.cases
            .get_mut(case.0 as usize)?
            .file_claim(creditor, amount, origin, None, arrear, t)
            .ok()
    }

    /// Wstawia lot do masy. Rzecz leasingowana **nie wchodzi** — wchodzi zapis
    /// o jej zwrocie, żeby niezmiennik 2 z §7.2 (każdy lot w dokładnie jednym
    /// stanie) obejmował też to, czego w masie nie ma.
    pub fn add_lot(&mut self, id: BankruptcyId, asset: AssetRef, book_value: Money) {
        let leased = self.is_leased(asset);
        let wycena = self.params.valuation.of(asset.kind, book_value);
        let Some(b) = self.cases.get_mut(id.0 as usize) else {
            return;
        };
        b.estate.push(AssetLot {
            asset,
            valuation: wycena,
            fate: if leased {
                LotFate::ReturnedToLessor
            } else {
                LotFate::InEstate
            },
        });
    }

    /// Kupno lotu — jedno wejście dla AI firm (M7e) i dla gracza (M9).
    ///
    /// Cena jest ceną bieżącej rundy i nie podlega negocjacji: wyprzedaż syndyka
    /// jest ofertą „bierz albo czekaj na tańszą rundę", a nie licytacją. Licytacja
    /// wymagałaby rozstrzygania remisów w tej samej minucie i drugiego mechanizmu
    /// wyceny — a plan mówi wprost, że żadnego z nich nie budujemy.
    pub fn bid_lot(
        &mut self,
        id: BankruptcyId,
        lot: usize,
        buyer: AccountOwner,
        buyer_account: AccountId,
        books: &mut Books,
        t: Tick,
    ) -> Option<Money> {
        let (konto, cena) = {
            let b = self.cases.get(id.0 as usize)?;
            let BankruptcyStage::Auction { round } = b.stage else {
                return None;
            };
            let l = b.estate.get(lot)?;
            if l.fate != LotFate::InEstate {
                return None;
            }
            (
                b.account,
                l.price_in_round(self.params.estate.discount(round)),
            )
        };
        if cena.get() <= 0 {
            return None;
        }
        let reason = DecisionReason::ClaimSettled {
            priority: ClaimPriority::Unsecured,
            ratio_bp: 0,
        };
        let asset = self.cases[id.0 as usize].estate[lot].asset;
        let memo = TxMemo::new(TxKind::Rent { site: asset.site }, reason);
        books.transfer(buyer_account, konto, cena, memo, t).ok()?;
        let b = &mut self.cases[id.0 as usize];
        b.estate[lot].fate = LotFate::Sold {
            to: buyer,
            price: cena,
        };
        b.proceeds = Money(b.proceeds.get().saturating_add(cena.get()));
        Some(cena)
    }

    /// Krok postępowania. Wołany raz na dobę; przechodzi do następnego etapu,
    /// gdy bieżący się wyczerpał.
    ///
    /// Zwraca `true`, jeśli postępowanie jest gotowe do podziału masy — wtedy
    /// wołający wykonuje [`CorpFinance::distribute`], bo on ma konta i świat.
    pub fn step_case(&mut self, id: BankruptcyId, t: Tick) -> bool {
        let p = self.params.estate.clone();
        let Some(b) = self.cases.get_mut(id.0 as usize) else {
            return false;
        };
        let doba = magnat_core::time::MINUTES_PER_DAY;
        let dni =
            u16::try_from((t.get().saturating_sub(b.stage_since.get())) / doba).unwrap_or(u16::MAX);
        match b.stage {
            BankruptcyStage::Filed => {
                b.stage = BankruptcyStage::Valuation;
                b.stage_since = t;
                false
            }
            BankruptcyStage::Valuation => {
                // Wycena jest gotowa w chwili wstawienia lotów; etap istnieje po to,
                // żeby wołający zdążył je wstawić po zgłoszeniu, a nie w jego trakcie.
                b.estate
                    .sort_by_key(|l| (l.asset.kind.as_index(), l.asset.site.entity().index()));
                b.stage = BankruptcyStage::Auction { round: 0 };
                b.stage_since = t;
                false
            }
            BankruptcyStage::Auction { round } => {
                if dni < p.round_days {
                    return false;
                }
                if round + 1 < p.rounds {
                    b.stage = BankruptcyStage::Auction { round: round + 1 };
                    b.stage_since = t;
                    return false;
                }
                // Po ostatniej rundzie loty idą po wartości złomu. Złom jest
                // **przychodem masy**, nie zniknięciem rzeczy: kupuje go reszta
                // świata i płaci za niego, więc pieniądz się domyka.
                for l in &mut b.estate {
                    if l.fate == LotFate::InEstate {
                        let zlom = l.valuation.mul_ratio(p.scrap_bp, 10_000);
                        if zlom.get() > 0 {
                            l.fate = LotFate::Sold {
                                to: AccountOwner::RestOfWorld,
                                price: zlom,
                            };
                        } else {
                            l.fate = LotFate::WrittenOff;
                        }
                    }
                }
                b.stage = BankruptcyStage::Distribution;
                b.stage_since = t;
                // Kolejność wypłat w obrębie priorytetu musi być ustalona **przed**
                // podziałem, bo to ona decyduje, kto dostaje resztę z zaokrąglenia.
                b.claims
                    .sort_by_key(|c| (c.priority.as_index(), c.creditor, c.id.0));
                true
            }
            BankruptcyStage::Distribution | BankruptcyStage::Closed => false,
        }
    }

    /// Wpływy ze złomu, które trzeba jeszcze zaksięgować po przejściu w `Distribution`.
    ///
    /// Osobno od [`CorpFinance::step_case`], bo tamten jest bez `Books` — a złom
    /// jest przelewem od reszty świata i musi go zobaczyć rejestr podaży pieniądza.
    pub fn settle_scrap(
        &mut self,
        id: BankruptcyId,
        rest_of_world: AccountId,
        books: &mut Books,
        t: Tick,
    ) -> Money {
        let Some(b) = self.cases.get(id.0 as usize) else {
            return Money::ZERO;
        };
        let konto = b.account;
        let plan: Vec<(usize, Money)> = b
            .estate
            .iter()
            .enumerate()
            .filter_map(|(i, l)| match l.fate {
                LotFate::Sold {
                    to: AccountOwner::RestOfWorld,
                    price,
                } => Some((i, price)),
                _ => None,
            })
            .collect();
        let mut suma = Money::ZERO;
        for (i, cena) in plan {
            let asset = self.cases[id.0 as usize].estate[i].asset;
            let memo = TxMemo::new(
                TxKind::Rent { site: asset.site },
                DecisionReason::BankruptcyOpened {
                    trigger: self.cases[id.0 as usize].trigger,
                    days: 0,
                },
            );
            if books.transfer(rest_of_world, konto, cena, memo, t).is_ok() {
                suma = Money(suma.get() + cena.get());
            } else {
                self.cases[id.0 as usize].estate[i].fate = LotFate::WrittenOff;
            }
        }
        let b = &mut self.cases[id.0 as usize];
        b.proceeds = Money(b.proceeds.get().saturating_add(suma.get()));
        suma
    }

    /// Podział masy i domknięcie postępowania.
    ///
    /// Wypłaca wierzycielom wg planu z [`Bankruptcy::plan_distribution`], odprowadza
    /// wynagrodzenie syndyka i umarza to, na co nie starczyło. Wypłaty do mieszkańców
    /// wychodzą listą [`SectorPayout`] — patrz tam, dlaczego nie mogą domknąć się tutaj.
    pub fn distribute(
        &mut self,
        id: BankruptcyId,
        trustee_account: AccountId,
        books: &mut Books,
        t: Tick,
        out: &mut Vec<SectorPayout>,
    ) -> Distribution {
        let Some(b) = self.cases.get(id.0 as usize) else {
            return Distribution {
                payouts: Vec::new(),
                trustee_fee: Money::ZERO,
                residual: Money::ZERO,
                ratio_bp: [0; magnat_core::CLAIM_PRIORITY_COUNT],
            };
        };
        let konto = b.account;
        let gotowka = books.balance(konto).unwrap_or(Money::ZERO);
        let plan = b.plan_distribution(gotowka);

        // Syndyk pierwszy — jego wynagrodzenie jest kosztem masy, a nie roszczeniem
        // w kolejce, i musi zejść z konta, zanim zejdzie z niego cokolwiek innego.
        if plan.trustee_fee.get() > 0 {
            let memo = TxMemo::new(
                TxKind::Withdrawal,
                DecisionReason::BankruptcyOpened {
                    trigger: b.trigger,
                    days: 0,
                },
            );
            let _ = books.transfer(konto, trustee_account, plan.trustee_fee, memo, t);
        }

        let roszczenia: Vec<Claim> = self.cases[id.0 as usize].claims.clone();
        for (i, c) in roszczenia.iter().enumerate() {
            let kwota = plan.payouts.get(i).copied().unwrap_or(Money::ZERO);
            let reason = plan.reason(c.priority);
            let wyplacono = if kwota.get() > 0 {
                let memo = TxMemo::new(TxKind::Withdrawal, reason);
                match c.creditor {
                    AccountOwner::Citizen(who) => {
                        let ok = books.household_receive(konto, kwota, memo, t).is_ok();
                        if ok {
                            out.push(SectorPayout {
                                to: who,
                                amount: kwota,
                                reason,
                            });
                        }
                        ok
                    }
                    AccountOwner::Household(hh) => {
                        let ok = books.household_receive(konto, kwota, memo, t).is_ok();
                        if ok {
                            out.push(SectorPayout {
                                to: CitizenId(hh.entity()),
                                amount: kwota,
                                reason,
                            });
                        }
                        ok
                    }
                    inny => match self.account_for(inny, books) {
                        Some(dokad) => books.transfer(konto, dokad, kwota, memo, t).is_ok(),
                        None => false,
                    },
                }
            } else {
                false
            };
            if wyplacono {
                self.cases[id.0 as usize].claims[i].paid = kwota;
                if let Some(a) = c.arrear {
                    self.arrears.settle(a, kwota);
                }
            }
            // Reszta roszczenia — zapłacona czy nie — gaśnie razem z firmą.
            // To jest jedyne miejsce, w którym dług znika bez zapłaty, i dlatego
            // jest jawne: masa się wyczerpała, a firmy już nie ma.
            if let Some(a) = c.arrear {
                self.arrears.write_off(a);
            }
        }

        let b = &mut self.cases[id.0 as usize];
        b.stage = BankruptcyStage::Closed;
        b.stage_since = t;
        self.open_case.remove(&b.firm.entity().index());
        plan
    }

    /// Rachunek wierzyciela. Konta zna [`Books`], więc szukamy w nim po właścicielu
    /// — pierwszy rachunek bieżący albo gotówkowy tego właściciela.
    fn account_for(&self, owner: AccountOwner, books: &Books) -> Option<AccountId> {
        (0..books.account_count()).find_map(|i| {
            let id = AccountId(u32::try_from(i).ok()?);
            books.account(id).filter(|a| a.owner == owner).map(|_| id)
        })
    }

    /// Otwarte postępowania, w kolejności identyfikatorów firm.
    pub fn open_cases(&self) -> impl Iterator<Item = BankruptcyId> + '_ {
        self.open_case.values().copied()
    }

    #[must_use]
    pub fn case_count(&self) -> usize {
        self.cases.len()
    }
}
