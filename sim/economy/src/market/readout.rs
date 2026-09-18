//! Odczyty dla prezentacji i pomiaru (szew (g)).

use super::*;

impl Market {
    /// Zbiorczy odczyt stanu rynku dla balansatora (§7.4) — jeden zamek na dobę.
    ///
    /// Liczy tu, a nie w balansatorze, z tego samego powodu, dla którego CPI liczy
    /// `Market`, a nie narzędzie (`AA-7`): druga implementacja rozkładu cen
    /// rozjechałaby się z pierwszą przy pierwszej zmianie i bramka mierzyłaby
    /// własny błąd zamiast rynku.
    #[must_use]
    pub fn balance_sample(&self) -> BalanceSample {
        let m = self.lock();

        // ── ceny per towar ────────────────────────────────────────────────────
        // Zbieranie idzie po `by_site` (BTreeMap), więc kolejność jest stanem,
        // a nie przypadkiem; sortowanie i tak następuje, ale determinizm wejścia
        // jest tańszy od dowodzenia, że sortowanie stabilne wystarczy.
        let mut per_good: BTreeMap<u16, Vec<i64>> = BTreeMap::new();
        let mut pustych = 0u64;
        let mut wszystkich = 0u64;
        let mut zywe = [false; STOCK_CAT_COUNT];
        for i in m.by_site.values() {
            let s = &m.shops[*i as usize];
            for l in &s.shelf.lines {
                let Some(o) = m.offers.get(l.offer) else {
                    continue;
                };
                wszystkich += 1;
                if o.available.get() <= 0 {
                    pustych += 1;
                } else if let Some(spec) = m.goods.spec(l.good) {
                    zywe[spec.cat.as_index()] = true;
                }
                per_good
                    .entry(l.good.0)
                    .or_default()
                    .push(o.unit_price.get());
            }
        }
        let prices = per_good
            .into_iter()
            .map(|(g, mut v)| {
                v.sort_unstable();
                let ranga = |p: usize| v[(v.len() * p / 100).min(v.len() - 1)];
                PriceDist {
                    good: GoodId(g),
                    min: Money(v[0]),
                    p10: Money(ranga(10)),
                    p50: Money(ranga(50)),
                    p90: Money(ranga(90)),
                    max: Money(v[v.len() - 1]),
                    offers: v.len() as u32,
                }
            })
            .collect();

        // ── marża i wypłacalność zakładów ─────────────────────────────────────
        let mut marze: Vec<i32> = Vec::with_capacity(m.shops.len());
        let mut insolvent = 0u32;
        for i in m.by_site.values() {
            let s = &m.shops[*i as usize];
            if s.ledger.balance(LedgerAccount::BankCurrent).get() < 0 {
                insolvent += 1;
            }
            let mut suma = 0i64;
            let mut ile = 0i64;
            for l in &s.shelf.lines {
                let Some(pc) = s.controllers.get(&l.good) else {
                    continue;
                };
                let unit_cost = m.unit_cost(*i as usize, l.good);
                if unit_cost.get() > 0 {
                    suma += i64::from(ShelfRow::margin_of(pc.current, unit_cost));
                    ile += 1;
                }
            }
            if ile > 0 {
                marze.push(i32::try_from(suma / ile).unwrap_or(i32::MAX));
            }
        }
        marze.sort_unstable();
        let margin_median_bp = marze.get(marze.len() / 2).copied().unwrap_or(0);

        // ── koncentracja per (kategoria, dzielnica) ───────────────────────────
        // Udział liczy się **obrotem**, nie liczbą sklepów: dwa sklepy, z których
        // jeden sprzedaje wszystko, to monopol, a nie duopol.
        let mut udzialy: BTreeMap<(u8, u16), Vec<i64>> = BTreeMap::new();
        for i in m.by_site.values() {
            let s = &m.shops[*i as usize];
            let mut per_cat: BTreeMap<u8, i64> = BTreeMap::new();
            for l in &s.shelf.lines {
                let Some(spec) = m.goods.spec(l.good) else {
                    continue;
                };
                // Obrót tygodniowy jako miara udziału: stan półki mówi o dostawie,
                // nie o tym, kto sprzedaje.
                let obrot = s
                    .controllers
                    .get(&l.good)
                    .map_or(0, |pc| pc.turnover_7d().get());
                *per_cat.entry(spec.cat.as_index() as u8).or_insert(0) += obrot;
            }
            for (cat, v) in per_cat {
                if v > 0 {
                    udzialy.entry((cat, s.district)).or_default().push(v);
                }
            }
        }
        let mut hhi: Vec<i32> = udzialy
            .into_values()
            .filter(|v| v.len() >= 2)
            .map(|v| {
                let suma: i64 = v.iter().sum();
                // `HHI × 10 000` na `i128`, żeby kwadrat udziału nie przepełnił `i64`.
                let s2 = i128::from(suma) * i128::from(suma);
                let sum_sq: i128 = v.iter().map(|x| i128::from(*x) * i128::from(*x)).sum();
                i32::try_from(sum_sq * 10_000 / s2.max(1)).unwrap_or(10_000)
            })
            .collect();
        hhi.sort_unstable();

        BalanceSample {
            prices,
            margin_median_bp,
            insolvent,
            shops: m.shops.len() as u32,
            stockout_permille: pustych
                .saturating_mul(1_000)
                .checked_div(wszystkich)
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(0),
            hhi_median: hhi.get(hhi.len() / 2).copied().unwrap_or(0),
            hhi_pairs: hhi.len() as u32,
            live_categories: zywe.iter().filter(|z| **z).count() as u32,
        }
    }

    // ── migawka panelu (M5e §5.12) ───────────────────────────────────────────────

    /// Wszystko, co pokazuje panel sklepu, w jednym odczycie pod jednym zamkiem.
    ///
    /// **Jedno wywołanie, nie dwadzieścia.** Panel składany z `price_at`,
    /// `shelf_qty`, `policy_of`… brałby zamek raz na wiersz i mógłby złapać dwa
    /// różne stany świata w jednej tabeli — cena z minuty `t` obok zapasu z `t+1`.
    /// Migawka jest z definicji spójna, bo powstaje pod jednym zamkiem.
    ///
    /// `from`/`to` wyznaczają okres rachunku wyników; zwykle początek miesiąca
    /// i chwila bieżąca.
    #[must_use]
    pub fn shop_panel(&self, site: SiteId, from: Tick, to: Tick) -> Option<ShopPanelSnapshot> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        let s = &m.shops[i as usize];
        let doba = u32::try_from(to.get() / magnat_core::time::MINUTES_PER_DAY).unwrap_or(0);

        let mut shelves = Vec::with_capacity(s.shelf.lines.len());
        for linia in &s.shelf.lines {
            let good = linia.good;
            // Koszt własny liczy się dokładnie tak samo jak w `reprice_all`: średnia
            // ważona zapasu, a przy pustym magazynie cena hurtowa. Inny wzór tutaj
            // znaczyłby marżę w panelu inną niż marża, na której stoi przecena.
            let unit_cost = m.unit_cost(i as usize, good);
            // Cena **z oferty**, nie ze sterownika: oferta jest jedynym nośnikiem
            // ceny (PRD §6.1) i to ją płaci kupujący (`K-7`). Sterownik niesie
            // politykę i obrót — rzeczy, których oferta nie zna.
            let price = m
                .offers
                .get(linia.offer)
                .map_or(Money::ZERO, |o| o.unit_price);
            let (policy, delegated, turnover) = match s.controllers.get(&good) {
                Some(pc) => (pc.policy, pc.delegated, pc.turnover_7d()),
                None => (PricePolicy::Fixed { price }, false, Qty::ZERO),
            };
            shelves.push(ShelfRow {
                good,
                price,
                unit_cost,
                margin_bp: ShelfRow::margin_of(price, unit_cost),
                on_shelf: m.shelf_units(i as usize, good),
                backroom: m.backroom_units(i as usize, good),
                days_of_cover: ShelfRow::cover_of(
                    Qty(m.shelf_units(i as usize, good).get()
                        + m.backroom_units(i as usize, good).get()),
                    turnover,
                ),
                turnover_7d: turnover,
                expires_at: m.shelf_state(i as usize, good).expires_at,
                policy,
                delegated,
            });
        }

        // Konkurencja: obraz jest prowadzony **per towar** (tak go używa przecena),
        // a panel pokazuje go **per sklep** — tu jest ta jedna transpozycja.
        let mut konkurenci: BTreeMap<SiteId, (Vec<(GoodId, Money)>, Tick)> = BTreeMap::new();
        for e in s.observed.entries() {
            let wpis = konkurenci
                .entry(e.cheapest_site)
                .or_insert_with(|| (Vec::new(), e.seen_at));
            wpis.0.push((e.good, e.cheapest));
            wpis.1 = wpis.1.min(e.seen_at);
        }
        let competition = konkurenci
            .into_iter()
            .map(|(cs, (prices, seen))| {
                let dist = m
                    .by_site
                    .get(&cs)
                    .map_or(0, |j| (m.shops[*j as usize].pos - s.pos).length() as u32);
                CompetitorRow {
                    site: cs,
                    distance_m: dist,
                    prices,
                    observed_age_days: u8::try_from(
                        to.get().saturating_sub(seen.get()) / magnat_core::time::MINUTES_PER_DAY,
                    )
                    .unwrap_or(u8::MAX),
                }
            })
            .collect();

        let customers = CustomerStats {
            by_district: s
                .customers
                .by_district
                .iter()
                .map(|(d, n)| (DistrictId(*d), *n))
                .collect(),
            by_class: magnat_agents::SocialClass::ALL
                .iter()
                .map(|c| (*c, s.customers.by_class[c.as_index()]))
                .filter(|(_, n)| *n > 0)
                .collect(),
            by_driver: magnat_core::UtilityKind::ALL
                .iter()
                .map(|u| (*u, s.customers.by_driver[u.as_index()]))
                .filter(|(_, n)| *n > 0)
                .collect(),
            // Pierścień jest indeksowany dobą modulo 7; panel dostaje go obróconego
            // tak, żeby ostatnia pozycja była dobą migawki (patrz `CustomerStats`).
            daily: std::array::from_fn(|i| s.customers.daily[(doba as usize + 1 + i) % 7]),
            total: s.customers.total,
        };

        let (back, shelf) = {
            let ch = m.chain.lock();
            (
                ch.store.slot_value(s.backroom).get(),
                ch.store.slot_value(s.shelf_slot).get(),
            )
        };

        Some(ShopPanelSnapshot {
            site,
            firm: s.firm,
            kind: s.kind,
            at: to,
            tracking: s.tracking,
            shelves,
            customers,
            lost_sales: LostSalesView {
                histogram: s.lost.window(doba),
                recent: s.lost.iter().copied().collect(),
            },
            competition,
            finance: FinanceSummary {
                statement: ledger::income_statement(&s.ledger, from, to),
                balance: ledger::balance_sheet(&s.ledger, to),
                cash: ledger::cash_flow(&s.ledger, from, to),
                inventory_value: Money(back + shelf),
                loan: s.loan,
            },
            reprices: s.reprice_log.clone(),
            good_keys: {
                let mut k: Vec<(GoodId, String)> = s
                    .shelf
                    .lines
                    .iter()
                    .map(|l| l.good)
                    .chain(s.observed.entries().iter().map(|e| e.good))
                    .map(|g| (g, m.goods.key_of(g).unwrap_or_default().to_string()))
                    .collect();
                k.sort_by_key(|(g, _)| g.0);
                k.dedup_by_key(|(g, _)| g.0);
                k
            },
        })
    }
}

// ── odczyty zakładu dla polityk i paneli (M7c WP7, M9d WP8) ─────────────────────
//
// Mieszkają tu, a nie przy wykonawcy polityk, bo to są **odczyty**, a nie krok
// doby: czyta je test, panel sklepu i edytor reguł. Przeprowadzka z `policy_run`
// przy M9d — tam rosły obok pętli, z którą nie mają nic wspólnego poza historią.

impl Market {
    /// Cel zamówienia towaru w sklepie — po nim widać, czy polityka zapasu zadziałała.
    #[must_use]
    pub fn reorder_target(&self, site: SiteId, good: GoodId) -> Option<Qty> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.inventory.reorder.get(&good).map(|r| r.target)
    }

    /// Polityka cenowa sterownika — po niej widać, czy regułę wykonano ceną stałą,
    /// czy marżą. Odczyt dla testów i dla panelu M9.
    #[must_use]
    pub fn price_policy(&self, site: SiteId, good: GoodId) -> Option<PricePolicy> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.controllers.get(&good).map(|pc| pc.policy)
    }

    /// Cena półkowa towaru w zakładzie, brutto. Odczyt dla testów i dla panelu.
    #[must_use]
    pub fn shelf_price(&self, site: SiteId, good: GoodId) -> Option<Money> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.controllers.get(&good).map(|pc| pc.current)
    }

    /// Podgląd obrazu konkurencji — wyłącznie do testów i do panelu.
    #[must_use]
    pub fn competitor_entry(&self, site: SiteId, good: GoodId) -> Option<(Money, Money, u32)> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.observed
            .get(good)
            .map(|e| (e.cheapest, e.median, e.offers))
    }

    /// Wiek obrazu konkurencji, którym pracuje zakład. Odczyt dla testu i dla panelu:
    /// to jest ta sama liczba, którą ustawia menedżer, a nie druga jej kopia.
    #[must_use]
    pub fn competitor_delay_days(&self, site: SiteId) -> Option<u8> {
        let m = self.lock();
        m.by_site
            .get(&site)
            .map(|i| m.shops[*i as usize].observed.delay_days)
    }

    /// Stawka VAT w punktach bazowych — przelicznik jawnej konwersji `brutto`/`netto`
    /// (`K-7`). Czytana z silnika podatkowego, a nie wpisana: do M8 wynosi zero, ale
    /// edytor reguł ma po niej sprowadzać koszt i cenę półkową do jednej podstawy.
    #[must_use]
    pub fn vat_bp(&self) -> i32 {
        let m = self.lock();
        let brutto = m.tax.gross_from_net(GoodId(0), Money(10_000));
        i32::try_from(brutto.get() - 10_000).unwrap_or(0)
    }

    /// Rachunki bieżące zakładów handlowych — wejście metryki `saldo` w regule.
    ///
    /// Osobno od [`Market::run_policies`], bo saldo prowadzi `Books`, a `Books`
    /// i rejestr firm nie dają się pożyczyć ze świata naraz. Wołający czyta jedno,
    /// potem drugie — i to jest cała treść tej funkcji.
    #[must_use]
    pub fn shop_accounts(&self) -> Vec<(SiteId, crate::books::AccountId)> {
        let m = self.lock();
        m.shops.iter().map(|s| (s.site, s.account)).collect()
    }

    /// Rachunek bieżący jednego zakładu handlowego. `None` dla zakładu, którego
    /// rynek detaliczny nie zna — zakład produkcyjny ma konto w `plants`.
    #[must_use]
    pub fn shop_account(&self, site: SiteId) -> Option<crate::books::AccountId> {
        let m = self.lock();
        m.by_site
            .get(&site)
            .copied()
            .map(|i| m.shops[i as usize].account)
    }

    /// Udział zbioru zakładów w obrocie towarem, w punktach bazowych obrotu miasta.
    ///
    /// Obrót to sztuki sprzedane w oknie siedmiu dób — ta sama liczba, którą panel
    /// sklepu pokazuje w kolumnie „rotacja". **Nie zapas na półce**: udział liczony
    /// z tego, co leży, mówiłby, kto ma największy magazyn, a nie kto sprzedaje.
    ///
    /// `district` zawęża rynek do jednej dzielnicy; `None` bierze całe miasto.
    /// `None` w wyniku znaczy „tego towaru nikt tu w tym tygodniu nie sprzedał",
    /// a to jest **inne zdanie** niż udział zero.
    ///
    /// Stoi tutaj, a nie u wołającego, z tego samego powodu co `balance_sample`:
    /// wnętrze rynku jest za zamkiem, a cel scenariusza i tytuł kariery nie mają
    /// prawa go otwierać.
    #[must_use]
    pub fn turnover_share_bp(
        &self,
        good: GoodId,
        district: Option<DistrictId>,
        sites: &[SiteId],
    ) -> Option<u16> {
        let m = self.lock();
        let mut caly = 0i64;
        let mut nasz = 0i64;
        for i in m.by_site.values() {
            let s = &m.shops[*i as usize];
            if s.closed || district.is_some_and(|d| d.0 != s.district) {
                continue;
            }
            let Some(pc) = s.controllers.get(&good) else {
                continue;
            };
            let obrot = pc.turnover_7d().get();
            caly = caly.saturating_add(obrot);
            if sites.contains(&s.site) {
                nasz = nasz.saturating_add(obrot);
            }
        }
        if caly <= 0 {
            return None;
        }
        let bp = nasz.saturating_mul(10_000) / caly;
        Some(u16::try_from(bp.clamp(0, 10_000)).unwrap_or(0))
    }

    /// Towary, którymi handluje wskazany zakład — wejście pytania o udział w rynku.
    #[must_use]
    pub fn goods_of(&self, site: SiteId) -> Vec<GoodId> {
        let m = self.lock();
        m.by_site.get(&site).map_or_else(Vec::new, |i| {
            m.shops[*i as usize]
                .shelf
                .lines
                .iter()
                .map(|l| l.good)
                .collect()
        })
    }
}
