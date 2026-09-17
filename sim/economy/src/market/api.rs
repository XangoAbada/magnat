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
        let i = *m.by_site.get(&site)? as usize;
        m.shops[i].shelf.line(good)?;
        Some(m.shelf_units(i, good))
    }

    #[must_use]
    pub fn backroom_qty(&self, site: SiteId, good: GoodId) -> Option<Qty> {
        let m = self.lock();
        let i = *m.by_site.get(&site)? as usize;
        Some(m.backroom_units(i, good))
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
        let (backroom, shelf_slot) = (m.shops[i as usize].backroom, m.shops[i as usize].shelf_slot);
        let ch = m.chain.lock();
        Money(ch.store.slot_value(backroom).get() + ch.store.slot_value(shelf_slot).get())
    }

    /// Uchwyt oferty stojącej na półce. Testy i narzędzia potrzebują go, żeby
    /// zbudować `PurchaseIntent` bez przechodzenia przez pełną wizytę.
    #[must_use]
    pub fn offer_of(&self, site: SiteId, good: GoodId) -> Option<OfferId> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        m.shops[i as usize].shelf.line(good).map(|l| l.offer)
    }

    /// Uchwyt do łańcucha dostaw M6.
    ///
    /// Potrzebny wszędzie tam, gdzie wołający prowadzi kadencję sam — w scenariuszach
    /// i w testach, które nie idą przez `MarketSystem`. Docelowo kadencję łańcucha
    /// prowadzą systemy ECS z §5.11 (M6e) i wtedy ten getter zostaje odczytem
    /// dla panelu, a nie drogą sterowania.
    #[must_use]
    pub fn chain(&self) -> magnat_supply::ChainHandle {
        self.lock().chain.clone()
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
        self.lock().goods.id_of_key(key)
    }

    /// Klucz tekstowy towaru — droga powrotna do [`Market::good_of_key`].
    ///
    /// Potrzebna komendom gracza (M9a): w dzienniku wejść towar jedzie **kluczem,
    /// nie indeksem**, bo `GoodId` nadaje się przy ładowaniu katalogu i przesuwa
    /// przy każdym nowym towarze (00 §5).
    #[must_use]
    pub fn good_key(&self, good: GoodId) -> Option<String> {
        self.lock().goods.key_of(good).map(ToString::to_string)
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
        m.by_site
            .get(&site)
            .map(|i| m.shops[*i as usize].account)
            .or_else(|| m.plants.get(&site).map(|(_, a)| *a))
    }

    /// Zapisuje zakład produkcyjny jako stronę rozliczeń B2B (`AP-2`).
    ///
    /// Osobno od [`Market::open_shop`], bo zakład **nie jest** sklepem i nie ma
    /// dostać półki: wspólna ścieżka dałaby osiemset sklepów bez klientów, które
    /// bramka G6 policzyłaby jako koncentrację handlu.
    pub fn register_plant(&self, site: SiteId, firm: magnat_core::FirmId, account: AccountId) {
        self.lock().plants.insert(site, (firm, account));
    }

    /// Konto firmy, jeśli prowadzi w mieście sklep albo zakład.
    #[must_use]
    pub fn account_of_firm(&self, firm: magnat_core::FirmId) -> Option<AccountId> {
        let m = self.lock();
        m.shops
            .iter()
            .find(|s| s.firm == firm)
            .map(|s| s.account)
            .or_else(|| m.plants.values().find(|(f, _)| *f == firm).map(|(_, a)| *a))
    }

    /// Ile zakładów produkcyjnych stoi w mieście.
    #[must_use]
    pub fn plant_count(&self) -> usize {
        self.lock().plants.len()
    }

    /// Nasiona wszystkich stojących sklepów — wejście powstawania firm (M7f WP15).
    ///
    /// Nowa firma nie stawia budynku: wchodzi do dzielnicy, w której ktoś już handluje,
    /// i otwiera **taki sam lokal**. To jest `ponytail:` z nazwanym sufitem — wybór
    /// parceli, powierzchni i budowa należą do rynku nieruchomości, czyli do M10
    /// (§6.7 PRD; M7 §2 mówi wprost, że M7 „tylko kupuje/najmuje parcele istniejącym
    /// API", a takiego API jeszcze nie ma). Konsekwencja jest jawna: miasto nie rośnie
    /// przestrzennie od powstawania firm, tylko gęstnieje.
    #[must_use]
    pub fn shop_seeds(&self) -> Vec<ShopSeed> {
        let m = self.lock();
        m.shops
            .iter()
            .filter(|s| !s.closed)
            .map(|s| ShopSeed {
                site: s.site,
                firm: s.firm,
                pos: s.pos,
                kind: s.kind,
                shelf_slots: s.shelf.lines.len().max(1) as u16,
                capacity_m3: s.inventory.capacity_m3,
                district: s.district,
            })
            .collect()
    }

    /// Zdjęcie półek miasta — jedno wejście `sim/macro::lift()` (M7f WP13).
    ///
    /// Osobny odczyt zamiast otwierania wnętrza rynku, z tego samego powodu, dla
    /// którego `refresh_board` nie wystawia `MarketInner`: zamek jest granicą i
    /// makro nie ma prawa go otwierać. Zwraca **cenę netto** (`K-7` — makro liczy
    /// marże, więc podstawa musi być jedna) i sztuki razem z zapleczem, bo z punktu
    /// widzenia dobowego kroku makro półka i magazyn sklepu to jeden zapas.
    ///
    /// Sklep zamknięty nie wchodzi: dla modelu nie istnieje, tak samo jak nie
    /// istnieje dla tablicy publicznej.
    #[must_use]
    pub fn shelf_snapshot(&self) -> Vec<ShelfSnapshot> {
        let m = self.lock();
        let mut out: Vec<ShelfSnapshot> = Vec::new();
        for (i, shop) in m.shops.iter().enumerate() {
            if shop.closed {
                continue;
            }
            for linia in &shop.shelf.lines {
                let Some(pc) = shop.controllers.get(&linia.good) else {
                    continue;
                };
                let qty =
                    Qty(m.shelf_units(i, linia.good).get() + m.backroom_units(i, linia.good).get());
                out.push(ShelfSnapshot {
                    site: shop.site,
                    firm: shop.firm,
                    district: DistrictId(shop.district),
                    good: linia.good,
                    price_net: m.tax.net_from_gross(linia.good, pc.current),
                    qty,
                });
            }
        }
        // Kolejność wchodzi do stanu makro, a kolejność sklepów w wektorze jest
        // kolejnością zakładania — sortowanie po kluczu zdejmuje z niej tę zależność.
        out.sort_unstable_by_key(|s| (s.site.0.to_bits(), s.good.0));
        out
    }

    // ── odczyty dla finansów firmy (M7d) ─────────────────────────────────────────
    //
    // Wszystkie pięć czyta to, co w rynku już leży, i żaden niczego nie liczy od nowa.
    // Stoją tutaj, a nie w `corpfin`, z tego samego powodu, dla którego `shop_accounts`
    // stoi tutaj, a nie w `policy_run`: wnętrze rynku jest za zamkiem, a postępowanie
    // upadłościowe nie ma prawa go otwierać.

    /// Firma prowadząca ten zakład — sklep albo zakład produkcyjny.
    #[must_use]
    pub fn firm_of(&self, site: SiteId) -> Option<magnat_core::FirmId> {
        let m = self.lock();
        m.by_site
            .get(&site)
            .map(|i| m.shops[*i as usize].firm)
            .or_else(|| m.plants.get(&site).map(|(f, _)| *f))
    }

    /// Kapitał własny zakładu z jego księgi — druga droga do niewypłacalności
    /// (M7d §5.13). Zakład bez księgi zwraca zero: nie wiemy, że jest pod kreską,
    /// więc nie twierdzimy, że jest.
    #[must_use]
    pub fn equity_of(&self, site: SiteId) -> Money {
        let m = self.lock();
        match m.by_site.get(&site) {
            Some(i) => {
                let t = m.tick;
                ledger::balance_sheet(&m.shops[*i as usize].ledger, t).equity_total
            }
            None => Money::ZERO,
        }
    }

    /// Ile miesięcy zalega kredyt tego zakładu.
    #[must_use]
    pub fn loan_arrears_of(&self, site: SiteId) -> u8 {
        let m = self.lock();
        m.by_site
            .get(&site)
            .and_then(|i| m.shops[*i as usize].loan)
            .and_then(|id| m.loans.get(id))
            .map_or(0, |l| l.arrears_months)
    }

    /// Wartość księgowa majątku zakładu: `(wyposażenie netto, zapas po koszcie)`.
    /// To jest wejście wyceny masy, a nie sama wycena — dyskonto dokłada
    /// [`crate::corpfin::ValuationParams`].
    #[must_use]
    pub fn book_assets_of(&self, site: SiteId) -> (Money, Money) {
        let m = self.lock();
        match m.by_site.get(&site) {
            Some(i) => {
                let t = m.tick;
                let b = ledger::balance_sheet(&m.shops[*i as usize].ledger, t);
                (b.fixed_net, b.inventory)
            }
            None => (Money::ZERO, Money::ZERO),
        }
    }

    /// Daje postępowaniu wgląd w rejestr kredytów bez wypuszczania go spod zamka.
    pub fn with_loans<R>(&self, f: impl FnOnce(&crate::credit::LoanBook) -> R) -> R {
        let m = self.lock();
        f(&m.loans)
    }

    /// Dopisuje powód do dziennika decyzji zakładu (wyjaśnialność, 00 §7).
    pub fn log_firm_decision(&self, site: SiteId, reason: magnat_core::DecisionReason) {
        let mut m = self.lock();
        let idx = site.entity().index();
        m.log_budget(idx, reason);
    }

    /// Konto reszty świata — druga strona zakupów u dostawcy i wypłat dochodu.
    #[must_use]
    /// Przestawia godziny handlu wszystkim sklepom dzielnicy (uchwała rady, M8e).
    ///
    /// Zwraca liczbę sklepów, których to dotknęło — zero znaczy „uchwała o dzielnicy
    /// bez sklepów", a to jest odpowiedź, nie awaria. Godziny są `OpenHours`, czyli
    /// tym samym typem, którym posługują się mieszkaniec i rampa magazynu (`K-34`):
    /// uchwała ma zmieniać to, co widać w sklepie, a nie drugą reprezentację obok.
    pub fn set_trading_hours(&self, district: u16, hours: magnat_agents::OpenHours) -> usize {
        let mut m = self.lock();
        let mut ile = 0;
        for s in &mut m.shops {
            if s.district == district {
                s.hours = hours;
                ile += 1;
            }
        }
        ile
    }

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
