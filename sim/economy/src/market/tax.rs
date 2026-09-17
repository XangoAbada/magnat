//! Odczyty i zapisy podatkowe rynku — hak, który M8 wypełnia (M8a, `K-57`).
//!
//! Osobny plik, a nie kolejne osiem metod w `api.rs`, bo to jest **drugi temat
//! w tym samym typie**: `api.rs` odpowiada na pytania o rynek („ile stoi na półce",
//! „kto ma to konto"), a ten plik na pytania o daninę. Podział jest mechaniczny —
//! przeniesienie symboli bez zmiany zachowania — i to jest pierwsza z trzech
//! odpowiedzi, których wymaga reguła przeglądu strukturalnego.
//!
//! Wszystko tutaj jest **kolejką albo księgowaniem**, nigdy naliczeniem: kwotę
//! liczy `sim/city`, a rynek oddaje fakty i przyjmuje zapisy. Gdyby było odwrotnie,
//! `sim/economy` musiałby znać stawki — czyli zależeć od miasta, które od niego
//! zależy.

use super::*;

impl Market {
    /// Wstawia silnik podatkowy w miejsce `NoTax` (`K-7`, hak dla M8).
    ///
    /// Wołane **raz**, przy stawianiu miasta, zanim padnie pierwsza cena. Podmiana
    /// w trakcie przebiegu jest dopuszczalna (uchwała rady zmieniająca stawkę),
    /// ale nie działa wstecz: ceny już wystawione przeliczy dopiero doba sklepu.
    pub fn set_tax_engine(&self, engine: Box<dyn crate::tax::TaxEngine>) {
        self.lock().tax = engine;
    }

    /// Część podatkowa kwoty brutto dla tego towaru. Zero, dopóki miasto nie wstawi
    /// swojego silnika.
    #[must_use]
    pub fn vat_on_gross(&self, good: magnat_core::GoodId, gross: Money) -> Money {
        self.lock().tax.vat_on_gross(good, gross)
    }

    /// Potrącenie u źródła od wypłaty dla gospodarstwa o tym indeksie encji.
    #[must_use]
    pub fn withhold(&self, household_index: u32, gross: Money) -> Money {
        self.lock().tax.withhold(household_index, gross)
    }

    /// Wyjmuje daniny naliczone od ostatniego odebrania, zakład po zakładzie,
    /// rosnąco po `SiteId`. Wołane raz w miesiącu przez system miasta.
    ///
    /// Wyjmuje, a nie czyta: licznik jest **kolejką do rozliczenia**, nie saldem.
    /// Dwa odczyty bez wyzerowania naliczyłyby ten sam podatek dwa razy, a zerowanie
    /// po stronie wołającego byłoby drugą ścieżką zmiany stanu rynku.
    #[must_use]
    pub fn take_tax_accrued(&self) -> Vec<(SiteId, crate::shop::TaxAccrual)> {
        let mut m = self.lock();
        let mut out = Vec::new();
        // Po `by_site`, a nie po `shops`: kolejność ma być kolejnością `SiteId`,
        // a nie kolejnością zakładania sklepów (00 §3.2).
        let kolejnosc: Vec<(SiteId, u32)> = m.by_site.iter().map(|(s, i)| (*s, *i)).collect();
        for (site, i) in kolejnosc {
            let a = &mut m.shops[i as usize].accrued;
            if a.vat.get() != 0 || a.excise.get() != 0 {
                out.push((site, *a));
                *a = crate::shop::TaxAccrual::default();
            }
        }
        out
    }

    /// Wyjmuje daniny naliczone na rozliczeniach hurtowych od ostatniego odebrania.
    ///
    /// Dwie daniny, dwa różne stany. **Cło** jest jedyną, którą liczy nie miasto:
    /// `sim/supply` dolicza je przy wycenie importu (`ImportQuote.duty`) i wsadza
    /// w koszt nabycia partii, bo tam jest jego miejsce (`K-36`) — miasto dostaje
    /// stąd fakt, że cło zostało pobrane, i zabiera pieniądz z kanału importowego,
    /// a nie drugi raz od importera. **Akcyza** jest zobowiązaniem jeszcze
    /// niezapłaconym i czeka na deklarację miesięczną.
    #[must_use]
    pub fn take_b2b_tax(&self) -> Vec<(SiteId, crate::shop::B2bTax)> {
        let mut m = self.lock();
        std::mem::take(&mut m.b2b_outbox).into_iter().collect()
    }

    /// Zobowiązanie podatkowe w księdze zakładu: `Dr TaxExpense, Cr TaxPayable`.
    ///
    /// Księguje ten, kto ma księgę (`AI-1`) — miasto podaje fakt, a nie zapis.
    /// VAT i akcyza tędy **nie idą**: ich zobowiązanie powstaje przy sprzedaży,
    /// w tym samym zapisie, w którym powstaje przychód.
    pub fn post_tax_accrual(&self, site: SiteId, amount: Money, reason: DecisionReason) -> bool {
        if amount.get() <= 0 {
            return false;
        }
        let mut m = self.lock();
        let tick = m.tick;
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        crate::ledger::post(
            &mut m.shops[i as usize].ledger,
            crate::ledger::JournalEntry::new(
                tick,
                reason,
                &[
                    (crate::ledger::LedgerAccount::TaxExpense, amount),
                    (
                        crate::ledger::LedgerAccount::TaxPayable,
                        Money(-amount.get()),
                    ),
                ],
            ),
        )
        .is_ok()
    }

    /// Zapłata daniny w księdze zakładu: `Dr TaxPayable, Cr BankCurrent`.
    pub fn post_tax_payment(&self, site: SiteId, amount: Money, reason: DecisionReason) -> bool {
        if amount.get() <= 0 {
            return false;
        }
        let mut m = self.lock();
        let tick = m.tick;
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        crate::ledger::post(
            &mut m.shops[i as usize].ledger,
            crate::ledger::JournalEntry::new(
                tick,
                reason,
                &[
                    (crate::ledger::LedgerAccount::TaxPayable, amount),
                    (
                        crate::ledger::LedgerAccount::BankCurrent,
                        Money(-amount.get()),
                    ),
                ],
            ),
        )
        .is_ok()
    }

    /// Wynik zakładu z **domkniętych** miesięcy w przedziale `[from, to)`.
    ///
    /// Z `Ledger::periods`, a nie z `income_statement`: dziennik zakładu jest
    /// pierścieniem i po roku gry nie pamięta stycznia, a podstawa CIT musi.
    /// Migawki domknięć pamiętają — po to są.
    ///
    /// `None` znaczy „nie ma takiego zakładu"; pusty przedział daje `Some(0)`,
    /// bo zakład bez domkniętego miesiąca ma wynik zerowy, a nie nieznany.
    #[must_use]
    pub fn closed_result(&self, site: SiteId, from: Tick, to: Tick) -> Option<Money> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        let suma: i64 = m.shops[i as usize]
            .ledger
            .periods
            .iter()
            .filter(|p| p.from.get() >= from.get() && p.to.get() <= to.get())
            .map(|p| p.statement.net_result().get())
            .sum();
        Some(Money(suma))
    }
}
