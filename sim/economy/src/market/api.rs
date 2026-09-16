//! Gettery i cienkie opakowania na raporty księgowe (szew (d) R-WP12).

use super::*;

impl Market {
    #[must_use]
    pub fn stats(&self) -> MarketStats {
        self.lock().stats
    }

    #[must_use]
    pub fn shop_count(&self) -> usize {
        self.lock().shops.len()
    }

    #[must_use]
    pub fn offer_count(&self) -> usize {
        self.lock().offers.len()
    }

    /// Cena oferty towaru w sklepie — do testów i do panelu (M5e).
    #[must_use]
    pub fn price_at(&self, site: SiteId, good: GoodId) -> Option<Money> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        let line = s.shelf.line(good)?;
        m.offers.get(line.offer).map(|o| o.unit_price)
    }

    /// Ręczna zmiana ceny — w M5b używana przez testy i scenariusz; od M5c robi to
    /// `reprice` na podstawie polityki cenowej.
    pub fn set_price(&self, site: SiteId, good: GoodId, price: Money) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        let Some(offer) = m.shops[i as usize].shelf.line(good).map(|l| l.offer) else {
            return false;
        };
        match m.offers.get_mut(offer) {
            Some(o) => {
                o.set_price(price);
                // Sterownik idzie za ofertą, choć polityka i tak nadpisze cenę przy
                // najbliższej przecenie. Powód jest taki, że **dwa źródła ceny nie
                // mają prawa się rozjechać nawet na jedną dobę**: panel czyta jedno,
                // decyzja zakupowa drugie, a gracz zobaczyłby wtedy inną cenę niż
                // ta, którą płaci jego klient.
                if let Some(pc) = m.shops[i as usize].controllers.get_mut(&good) {
                    pc.current = price;
                }
                true
            }
            None => false,
        }
    }

    #[must_use]
    pub fn shelf_qty(&self, site: SiteId, good: GoodId) -> Option<Qty> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.shelf.line(good).map(|l| l.qty)
    }

    #[must_use]
    pub fn backroom_qty(&self, site: SiteId, good: GoodId) -> Option<Qty> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.inventory.backroom.get(&good).map(|l| l.qty)
    }

    /// Wartość zapasu sklepu: zaplecze **i** półka. To jest lewa strona niezmiennika
    /// P5, który zamyka się dopiero w M5c — ale liczyć ją trzeba już tutaj, bo to tu
    /// zapas zmienia właściciela.
    #[must_use]
    pub fn inventory_value(&self, site: SiteId) -> Money {
        let m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return Money::ZERO;
        };
        let s = &m.shops[i as usize];
        let back: i64 = s
            .inventory
            .backroom
            .values()
            .map(|l| l.cost_total.get())
            .sum();
        let shelf: i64 = s.shelf.lines.iter().map(|l| l.cost_total.get()).sum();
        Money(back + shelf)
    }

    /// Uchwyt oferty stojącej na półce. Testy i narzędzia potrzebują go, żeby
    /// zbudować `PurchaseIntent` bez przechodzenia przez pełną wizytę.
    #[must_use]
    pub fn offer_of(&self, site: SiteId, good: GoodId) -> Option<OfferId> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        m.shops[i as usize].shelf.line(good).map(|l| l.offer)
    }

    /// Szok ceny hurtowej: mnożnik w punktach bazowych (10 000 = bez zmian).
    /// Wejście scenariusza `supply-shock` balansatora (§7.4, bramka G4).
    pub fn set_supply_shock(&self, good: GoodId, factor_bp: i32) {
        self.lock().supplier.set_shock(good, factor_bp);
    }

    /// `GoodId` po kluczu tekstowym z `data/economy/retail.ron` — scenariusz szoku
    /// wskazuje towar nazwą, bo identyfikator zależy od katalogu miasta.
    #[must_use]
    pub fn good_of_key(&self, key: &str) -> Option<GoodId> {
        self.lock().supplier.goods().id_of_key(key)
    }

    /// Pozycja zakładu w metrach — klient potrzebuje jej, żeby zamienić kliknięcie
    /// w teren na sklep, bo bufor identyfikatorów renderera niesie tylko pieszych.
    #[must_use]
    pub fn shop_pos(&self, site: SiteId) -> Option<Vec2> {
        let m = self.lock();
        m.by_site.get(&site).map(|i| m.shops[*i as usize].pos)
    }

    #[must_use]
    pub fn account_of(&self, site: SiteId) -> Option<AccountId> {
        let m = self.lock();
        m.by_site.get(&site).map(|i| m.shops[*i as usize].account)
    }

    /// Konto reszty świata — druga strona zakupów u dostawcy i wypłat dochodu.
    #[must_use]
    pub fn rest_of_world(&self) -> AccountId {
        self.lock().rest_of_world
    }

    #[must_use]
    pub fn sites(&self) -> Vec<SiteId> {
        self.lock().by_site.keys().copied().collect()
    }

    /// Ustawia poziom śledzenia utraconych sprzedaży. Flagę ustawia **`game/`**,
    /// `sim/economy` ją tylko czyta — „kto jest graczem" nie jest pojęciem
    /// ekonomicznym (§9 pkt 13 dokumentu fazy).
    pub fn set_tracking(&self, site: SiteId, level: LostSaleTracking) {
        let mut m = self.lock();
        if let Some(i) = m.by_site.get(&site).copied() {
            let s = &mut m.shops[i as usize];
            s.tracking = level;
            // Ta sama flaga włącza pierścień dziennika księgowego (§5.8): salda
            // prowadzą **wszystkie** zakłady, okno zapisów tylko śledzone. Poziom
            // nie wchodzi do hasha, więc kliknięcie „śledź" nie zmienia świata.
            s.ledger.set_journal(level != LostSaleTracking::None);
            // Wyłączenie śledzenia kasuje rozkłady klientów: po ponownym włączeniu
            // gracz ma zobaczyć **swój** zasięg, a nie osad sprzed przejęcia sklepu.
            if level == LostSaleTracking::None {
                s.customers = ShopCustomers::default();
            }
        }
    }

    /// Utracone sprzedaże sklepu — „dlaczego Anna nie kupiła u mnie" (PRD §14.1).
    #[must_use]
    pub fn lost_sales(&self, site: SiteId) -> Vec<LostSale> {
        let m = self.lock();
        m.by_site.get(&site).map_or_else(Vec::new, |i| {
            m.shops[*i as usize].lost.iter().copied().collect()
        })
    }

    #[must_use]
    pub fn lost_histogram(&self, site: SiteId) -> Option<LostSaleHistogram> {
        let m = self.lock();
        m.by_site
            .get(&site)
            .map(|i| m.shops[*i as usize].lost.today)
    }

    #[must_use]
    pub fn policy_of(&self, site: SiteId, good: GoodId) -> Option<PricePolicy> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        m.shops[i as usize]
            .controllers
            .get(&good)
            .map(|pc| pc.policy)
    }

    /// Powody ostatnich przecen zakładu śledzonego (§7 — wyjaśnialność).
    #[must_use]
    pub fn reprice_log(&self, site: SiteId) -> Vec<DecisionReason> {
        let m = self.lock();
        m.by_site
            .get(&site)
            .map_or_else(Vec::new, |i| m.shops[*i as usize].reprice_log.clone())
    }

    /// Obraz konkurencji, jaki sklep ma **w tej chwili** — z opóźnieniem, jakie ma.
    #[must_use]
    pub fn observed_of(&self, site: SiteId, good: GoodId) -> Option<CompetitorEntry> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        m.shops[i as usize].observed.get(good).copied()
    }

    /// Czujność firmy: co ile dni odświeża obraz cen konkurencji (1..=7).
    #[must_use]
    pub fn observe_delay(&self, site: SiteId) -> Option<u8> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        Some(m.shops[i as usize].pricing.delay_days)
    }

    /// Zmierzona elastyczność popytu, jeśli eksperyment ją rozstrzygnął.
    #[must_use]
    pub fn elasticity_of(
        &self,
        site: SiteId,
        good: GoodId,
    ) -> Option<crate::pricing::ObservedElasticity> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        m.shops[i as usize].controllers.get(&good)?.elasticity
    }

    // ── raporty księgowe (§5.8) ──────────────────────────────────────────────────

    #[must_use]
    pub fn income_statement(
        &self,
        site: SiteId,
        from: Tick,
        to: Tick,
    ) -> Option<crate::ledger::IncomeStatement> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        Some(ledger::income_statement(
            &m.shops[i as usize].ledger,
            from,
            to,
        ))
    }

    #[must_use]
    pub fn balance_sheet(&self, site: SiteId, at: Tick) -> Option<crate::ledger::BalanceSheet> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        Some(ledger::balance_sheet(&m.shops[i as usize].ledger, at))
    }

    #[must_use]
    pub fn cash_flow(&self, site: SiteId, from: Tick, to: Tick) -> Option<crate::ledger::CashFlow> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        Some(ledger::cash_flow(&m.shops[i as usize].ledger, from, to))
    }

    /// Saldo pojedynczego konta księgi — lewa strona niezmiennika P5 i test na to,
    /// czy `BankCurrent` nadąża za rachunkiem w `Books`.
    #[must_use]
    pub fn ledger_balance(&self, site: SiteId, account: LedgerAccount) -> Option<Money> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        Some(m.shops[i as usize].ledger.balance(account))
    }
}
