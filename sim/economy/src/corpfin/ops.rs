//! Instrumenty firmy w ruchu: leasing, obligacja, faktoring (M7d WP8).
//!
//! Struktury są w [`super::instruments`], tutaj są **operacje na kontach** — i to
//! jest cała linia podziału. Struktura leasingu nie potrzebuje `Books`; zapłata raty
//! potrzebuje, i to ona decyduje, czy niezapłacona rata staje się długiem, czy ciszą.
//!
//! Kredyt nie ma tu nic, bo ma własny moduł od M5d ([`crate::credit`]). M7d dokłada
//! mu wyłącznie trzeci produkt (`LoanKind::Investment`) i zaległość z niezapłaconej
//! raty — obie zmiany są u niego, a nie tutaj.

use magnat_core::{CitizenId, DecisionReason, FirmId, FirmReason, Money, Tick};

use crate::books::{AccountId, AccountOwner, Books, TxKind, TxMemo};
use crate::corpfin::arrears::{ArrearId, ClaimOrigin};
use crate::corpfin::instruments::{AssetRef, Bond, BondHolder, BondId, Lease, LeaseId};
use crate::corpfin::{owner_of, CorpFinance, SectorPayout};

impl CorpFinance {
    // ── leasing (WP8) ────────────────────────────────────────────────────────────

    /// Podpisuje umowę leasingową.
    ///
    /// Leasing **nie tworzy pieniądza i nie przesuwa go w chwili podpisania** —
    /// firma dostaje rzecz do używania, a nie kwotę. Pierwszy przepływ jest dopiero
    /// przy pierwszej racie. Dlatego ta funkcja nie dotyka `Books` i nie może
    /// zawieść: zawieść może rata, nie podpis.
    ///
    /// Siedem argumentów zamiast struktury wejściowej z tego samego powodu co przy
    /// `LoanBook::open`: wszystkie są **warunkami umowy**, a nie konfiguracją, i mają
    /// tu jedyne wywołanie. `LeaseTerms` byłby typem z jednym producentem i jednym
    /// konsumentem, czyli nazwą dla listy argumentów.
    #[allow(clippy::too_many_arguments)]
    pub fn sign_lease(
        &mut self,
        lessee: FirmId,
        lessor: FirmId,
        lessor_account: AccountId,
        asset: AssetRef,
        monthly: Money,
        months: u16,
        buyout: Money,
    ) -> LeaseId {
        let id = LeaseId(u32::try_from(self.leases.len()).unwrap_or(u32::MAX));
        self.leases.push(Lease {
            id,
            lessee,
            lessor,
            lessor_account,
            asset,
            monthly,
            months_left: months,
            buyout,
            missed: 0,
            ended: false,
            bought_out: false,
        });
        id
    }

    /// Wykup po ostatniej racie: rzecz przechodzi na własność leasingobiorcy
    /// i od tej chwili **wchodzi** do jego majątku, także do masy upadłościowej.
    pub fn buy_out_lease(
        &mut self,
        id: LeaseId,
        payer: AccountId,
        books: &mut Books,
        t: Tick,
    ) -> bool {
        let Some(l) = self.leases.get(id.0 as usize).copied() else {
            return false;
        };
        if l.bought_out || l.months_left > 0 {
            return false;
        }
        let memo = TxMemo::new(
            TxKind::Rent { site: l.asset.site },
            DecisionReason::Firm(FirmReason::LeaseSigned {
                site: l.asset.site,
                months: 0,
            }),
        );
        if l.buyout.get() > 0
            && books
                .transfer(payer, l.lessor_account, l.buyout, memo, t)
                .is_err()
        {
            return false;
        }
        self.leases[id.0 as usize].bought_out = true;
        true
    }

    /// Rata leasingowa jednej umowy. Zwraca kwotę, która wyszła z rachunku.
    ///
    /// Nieudany przelew **nie jest niczym** dla `Books`, ale jest długiem: zostaje
    /// zaległością wobec leasingodawcy. Po `repossess_after` ratach z rzędu umowa
    /// kończy się odbiorem rzeczy — i to jest cała różnica między leasingiem
    /// a kredytem pod zastaw, bo rzecz nigdy nie była firmy.
    pub fn pay_lease(
        &mut self,
        id: LeaseId,
        payer: AccountId,
        payer_firm: FirmId,
        books: &mut Books,
        t: Tick,
    ) -> Money {
        let Some(l) = self.leases.get(id.0 as usize).copied() else {
            return Money::ZERO;
        };
        if !l.is_active() || l.monthly.get() <= 0 {
            return Money::ZERO;
        }
        let memo = TxMemo::new(
            TxKind::Rent { site: l.asset.site },
            DecisionReason::Firm(FirmReason::LeaseSigned {
                site: l.asset.site,
                months: l.months_left,
            }),
        );
        if books
            .transfer(payer, l.lessor_account, l.monthly, memo, t)
            .is_ok()
        {
            let lease = &mut self.leases[id.0 as usize];
            lease.months_left = lease.months_left.saturating_sub(1);
            lease.missed = 0;
            if lease.months_left == 0 {
                lease.ended = true;
            }
            return l.monthly;
        }
        self.arrears.accrue(
            AccountOwner::Firm(payer_firm),
            AccountOwner::Firm(l.lessor),
            l.monthly,
            ClaimOrigin::Lease(id),
            t,
        );
        let lease = &mut self.leases[id.0 as usize];
        lease.missed = lease.missed.saturating_add(1);
        Money::ZERO
    }

    /// Odbiera rzecz leasingodawcy, gdy leasingobiorca przestał płacić.
    /// Zwraca `true`, jeśli umowa właśnie się skończyła.
    pub fn repossess_if_due(&mut self, id: LeaseId, after: u8) -> bool {
        match self.leases.get_mut(id.0 as usize) {
            Some(l) if !l.ended && l.missed >= after.max(1) => {
                l.ended = true;
                true
            }
            _ => false,
        }
    }

    /// Wszystkie czynne umowy leasingobiorcy, w kolejności podpisania.
    pub fn leases_of(&self, firm: FirmId) -> impl Iterator<Item = &Lease> {
        self.leases
            .iter()
            .filter(move |l| l.is_active() && l.lessee == firm)
    }

    // ── obligacje (WP8) ──────────────────────────────────────────────────────────

    /// Emisja obligacji: nabywcy płacą emitentowi, emitent zostaje z długiem.
    ///
    /// Obligacja **nie tworzy pieniądza** — przesuwa ten, który już jest, więc idzie
    /// zwykłym przelewem, a nie `create_credit`. Nabywca, który nie ma czym zapłacić,
    /// po prostu nie obejmuje pakietu; emisja dochodzi do skutku w tej części,
    /// w której ktoś zapłacił, i `face` jest sumą **objętych** pakietów, nie
    /// zamierzonych. Inaczej firma miałaby dług wobec kogoś, kto jej nic nie dał.
    ///
    /// Zwraca `None`, jeśli nikt nie objął ani grosza.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_bond(
        &mut self,
        issuer: FirmId,
        issuer_account: AccountId,
        coupon_bp: u16,
        months: u16,
        buyers: &[(BondHolder, AccountId, Money)],
        books: &mut Books,
        t: Tick,
    ) -> Option<BondId> {
        let id = BondId(u32::try_from(self.bonds.len()).unwrap_or(u32::MAX));
        let reason = DecisionReason::Firm(FirmReason::BondIssued { coupon_bp, months });
        let memo = TxMemo::new(
            TxKind::ExternalCapital {
                investor: crate::books::ExternalInvestorId(id.0),
                inflow: true,
            },
            reason,
        );
        let mut holders: Vec<(BondHolder, Money)> = Vec::new();
        let mut objete = Money::ZERO;
        for (who, konto, kwota) in buyers {
            if kwota.get() <= 0 {
                continue;
            }
            if books
                .transfer(*konto, issuer_account, *kwota, memo, t)
                .is_err()
            {
                continue;
            }
            holders.push((*who, *kwota));
            objete = Money(objete.get().saturating_add(kwota.get()));
        }
        if objete.get() <= 0 {
            return None;
        }
        // Kolejność wypłat kuponu wchodzi do hasha stanu, więc lista nabywców
        // jest sortowana raz, tutaj, i od tej chwili się nie przestawia.
        holders.sort_by_key(|(who, _)| *who);
        self.bonds.push(Bond {
            id,
            issuer,
            issuer_account,
            face: objete,
            coupon_bp,
            matures: Tick(t.get() + u64::from(months) * magnat_core::time::MINUTES_PER_MONTH),
            holders,
            redeemed: false,
        });
        Some(id)
    }

    /// Kupon miesięczny jednej emisji. Zwraca wypłaty, które wyszły z ksiąg
    /// i muszą trafić do komponentów mieszkańców — patrz [`SectorPayout`].
    pub fn pay_coupon(
        &mut self,
        id: BondId,
        books: &mut Books,
        t: Tick,
        out: &mut Vec<SectorPayout>,
    ) -> Money {
        let Some(b) = self.bonds.get(id.0 as usize) else {
            return Money::ZERO;
        };
        if b.redeemed || b.face.get() <= 0 {
            return Money::ZERO;
        }
        let kupon = b.monthly_coupon();
        if kupon.get() <= 0 {
            return Money::ZERO;
        }
        // Podział kuponu między nabywców proporcjonalnie do objętych kwot,
        // z resztą do pierwszego wg ustalonego porządku (00 §2).
        let wagi: Vec<u64> = b
            .holders
            .iter()
            .map(|(_, m)| u64::try_from(m.get()).unwrap_or(0))
            .collect();
        let udzialy = magnat_core::split_proportional(kupon, &wagi);
        let konto = b.issuer_account;
        let issuer = b.issuer;
        let reason = DecisionReason::Firm(FirmReason::BondIssued {
            coupon_bp: b.coupon_bp,
            months: 0,
        });
        let plan: Vec<(BondHolder, Money)> = b
            .holders
            .iter()
            .enumerate()
            .map(|(i, (who, _))| (*who, udzialy.get(i).copied().unwrap_or(Money::ZERO)))
            .collect();
        let mut wyszlo = Money::ZERO;
        for (who, kwota) in plan {
            if kwota.get() <= 0 {
                continue;
            }
            let memo = TxMemo::new(
                TxKind::ExternalCapital {
                    investor: crate::books::ExternalInvestorId(id.0),
                    inflow: false,
                },
                reason,
            );
            match who {
                BondHolder::Firm(_, dokad) => {
                    if books.transfer(konto, dokad, kwota, memo, t).is_ok() {
                        wyszlo = Money(wyszlo.get() + kwota.get());
                    } else {
                        self.arrears.accrue(
                            AccountOwner::Firm(issuer),
                            owner_of(who),
                            kwota,
                            ClaimOrigin::Bond(id),
                            t,
                        );
                    }
                }
                BondHolder::Household(hh) => {
                    if books.household_receive(konto, kwota, memo, t).is_ok() {
                        out.push(SectorPayout {
                            to: CitizenId(hh.entity()),
                            amount: kwota,
                            reason,
                        });
                        wyszlo = Money(wyszlo.get() + kwota.get());
                    } else {
                        self.arrears.accrue(
                            AccountOwner::Firm(issuer),
                            owner_of(who),
                            kwota,
                            ClaimOrigin::Bond(id),
                            t,
                        );
                    }
                }
            }
        }
        wyszlo
    }

    /// Czy emisja dojrzała do wykupu.
    #[must_use]
    pub fn bond_matured(&self, id: BondId, t: Tick) -> bool {
        self.bonds
            .get(id.0 as usize)
            .is_some_and(|b| !b.redeemed && t >= b.matures)
    }

    /// Wykup emisji: zwrot nominału nabywcom. Nabywca, któremu nie starczyło
    /// pieniędzy emitenta, zostaje z zaległością — i to jest właśnie ten moment,
    /// w którym obligacja wpycha firmę w upadłość.
    pub fn redeem_bond(
        &mut self,
        id: BondId,
        books: &mut Books,
        t: Tick,
        out: &mut Vec<SectorPayout>,
    ) -> Money {
        let Some(b) = self.bonds.get(id.0 as usize) else {
            return Money::ZERO;
        };
        if b.redeemed {
            return Money::ZERO;
        }
        let konto = b.issuer_account;
        let issuer = b.issuer;
        let reason = DecisionReason::Firm(FirmReason::BondIssued {
            coupon_bp: b.coupon_bp,
            months: 0,
        });
        let plan: Vec<(BondHolder, Money)> = b.holders.clone();
        let mut wyszlo = Money::ZERO;
        for (who, kwota) in plan {
            if kwota.get() <= 0 {
                continue;
            }
            let memo = TxMemo::new(
                TxKind::ExternalCapital {
                    investor: crate::books::ExternalInvestorId(id.0),
                    inflow: false,
                },
                reason,
            );
            let ok = match who {
                BondHolder::Firm(_, dokad) => books.transfer(konto, dokad, kwota, memo, t).is_ok(),
                BondHolder::Household(hh) => {
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
            };
            if ok {
                wyszlo = Money(wyszlo.get() + kwota.get());
            } else {
                self.arrears.accrue(
                    AccountOwner::Firm(issuer),
                    owner_of(who),
                    kwota,
                    ClaimOrigin::Bond(id),
                    t,
                );
            }
        }
        if let Some(b) = self.bonds.get_mut(id.0 as usize) {
            b.redeemed = true;
        }
        wyszlo
    }

    /// Czynne emisje danego emitenta.
    pub fn bonds_of(&self, firm: FirmId) -> impl Iterator<Item = &Bond> {
        self.bonds
            .iter()
            .filter(move |b| !b.redeemed && b.issuer == firm)
    }

    // ── faktoring (WP8) ──────────────────────────────────────────────────────────

    /// Sprzedaż należności faktorowi: wierzyciel dostaje `kwota × (1 − dyskonto)`
    /// **teraz**, faktor przejmuje prawo do pełnej kwoty **kiedyś**.
    ///
    /// Pieniądz się przy tym zachowuje — to zwykły przelew między dwoma kontami,
    /// a różnica między kwotą nominalną a zapłaconą to nie ubytek, tylko kwota,
    /// której faktor nigdy nie wypłacił. Dłużnik nie bierze w tym udziału i nie
    /// musi o niczym wiedzieć; zmienia mu się wyłącznie wierzyciel.
    ///
    /// Zwraca kwotę, którą wierzyciel dostał.
    ///
    /// Trzy konta w argumentach, bo trzy strony biorą udział i żadnej z nich nie da
    /// się wyprowadzić z pozostałych: `Books` indeksuje konta po uchwycie, a nie po
    /// właścicielu, więc szukanie ich w środku byłoby przeglądem całej księgi
    /// na każdą sprzedaną należność.
    #[allow(clippy::too_many_arguments)]
    pub fn factor_receivable(
        &mut self,
        id: ArrearId,
        factor: FirmId,
        factor_account: AccountId,
        creditor_account: AccountId,
        discount_bp: i64,
        books: &mut Books,
        t: Tick,
    ) -> Money {
        let Some(a) = self.arrears.get(id).copied() else {
            return Money::ZERO;
        };
        if a.settled || a.factored || a.amount.get() <= 0 {
            return Money::ZERO;
        }
        let cena = a
            .amount
            .mul_ratio(10_000 - discount_bp.clamp(0, 10_000), 10_000);
        if cena.get() <= 0 {
            return Money::ZERO;
        }
        let reason = DecisionReason::Firm(FirmReason::ReceivablesFactored {
            count: 1,
            discount_bp: u16::try_from(discount_bp.clamp(0, 10_000)).unwrap_or(0),
        });
        let memo = TxMemo::new(TxKind::Withdrawal, reason);
        if books
            .transfer(factor_account, creditor_account, cena, memo, t)
            .is_err()
        {
            return Money::ZERO;
        }
        self.arrears.assign(id, AccountOwner::Firm(factor));
        cena
    }
}
