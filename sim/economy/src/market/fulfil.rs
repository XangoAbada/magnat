//! Decyzja zakupowa i rozliczenie transakcji (szwy (e) i (h)).

use super::*;

/// Sufit liczby towarów rozważanych w jednej wizycie — tyle, ile mieści największa
/// półka. Ponad to substytut niższego rzędu przestaje być substytutem.
const MAX_LINES: usize = 48;

impl Market {
    /// Wyjmuje zaklepane transakcje w kolejności `(SiteId, GoodId, arrived, CitizenId)`
    /// i zwalnia rezerwacje budżetu (§5.5).
    #[must_use]
    pub fn take_intents(&self) -> Vec<PurchaseIntent> {
        let mut m = self.lock();
        m.committed.clear();
        let mut v = std::mem::take(&mut m.intents);
        v.sort_unstable_by_key(|i| {
            (
                i.site.entity().index(),
                i.good.get(),
                i.arrived.get(),
                i.buyer.entity().index(),
            )
        });
        v
    }

    /// Oddaje towar na półkę, kiedy rozliczenie nie doszło do skutku.
    ///
    /// Bez tego sztuka zdjęta w `fulfil` znikałaby ze świata przy każdym nieudanym
    /// przelewie — a niezmiennik masy (00 §6) nie ma wyjątku na „prawie się udało".
    pub fn return_goods(&self, intent: &PurchaseIntent) {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&intent.site).copied() else {
            return;
        };
        let Some(kawalek) = intent.taken else {
            return;
        };
        // Partia wraca **w całości i taka sama**: z jakością, marką, datą i kosztem.
        // Gdyby wracały same sztuki, `InventoryGoods` rozjeżdżałby się z zapasem (P5),
        // a bilans masy — z bilansem magazynu; ani jeden, ani drugi nie ma wyjątku
        // na „prawie się udało".
        let (slot, good) = (m.shops[i as usize].shelf_slot, intent.good);
        let chain = m.chain.clone();
        let wrocilo = {
            let mut ch = chain.lock();
            ch.store.return_taken(&chain.cat, slot, &kawalek)
        };
        if !wrocilo {
            return;
        }
        let qty = m.shelf_units(i as usize, good);
        if let Some(sl) = m.shops[i as usize].shelf.line(good).copied() {
            if let Some(o) = m.offers.get_mut(sl.offer) {
                o.available = qty;
            }
        }
    }

    /// Księguje sprzedaż po stronie sklepu — wołane przez [`settle_transactions`],
    /// kiedy pieniądz naprawdę się przesunął.
    pub fn record_sale(&self, intent: &PurchaseIntent) {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&intent.site).copied() else {
            return;
        };
        let s = &mut m.shops[i as usize];
        s.sold_qty = s.sold_qty.saturating_add(intent.qty.get());
        s.revenue = s
            .revenue
            .checked_add(intent.agreed_price)
            .unwrap_or(s.revenue);
        // Sprzedaż w księdze: przychód po jednej stronie, koszt własny po drugiej —
        // **jednym** zapisem, żeby marża nie dała się policzyć z połowy zdarzenia.
        let _ = ledger::post(
            &mut s.ledger,
            JournalEntry::new(
                intent.arrived,
                intent.reason,
                &[
                    (LedgerAccount::BankCurrent, intent.agreed_price),
                    (LedgerAccount::Revenue, Money(-intent.agreed_price.get())),
                    (LedgerAccount::Cogs, intent.cogs),
                    (LedgerAccount::InventoryGoods, Money(-intent.cogs.get())),
                ],
            ),
        );
        if let Some(pc) = s.controllers.get_mut(&intent.good) {
            pc.sold_today = Qty(pc.sold_today.get().saturating_add(intent.qty.get()));
        }
        // Karta „Klienci" (§5.12). Jak histogram utraconych sprzedaży: zakład
        // nieoznaczony nie płaci za ten mechanizm nic poza odczytem bitu.
        if s.tracking != LostSaleTracking::None {
            let dominant = match intent.reason {
                DecisionReason::ShopChosen { dominant, .. } => dominant,
                // Wizyta bez decyzji z planu dnia (mieszkaniec trafił inną ścieżką):
                // przeważyła wygoda, bo nic innego nie było porównywane.
                _ => magnat_core::UtilityKind::Convenience,
            };
            let doba = u32::try_from(intent.arrived.get() / magnat_core::time::MINUTES_PER_DAY)
                .unwrap_or(u32::MAX);
            s.customers.record(
                doba,
                intent.district,
                magnat_agents::SocialClass::of(intent.status),
                dominant,
            );
        }
        // Koszyk CPI liczy się z **cen transakcyjnych**, więc wchodzi tutaj, a nie
        // przy wystawieniu oferty: oferta, której nikt nie kupuje, nie jest ceną (§6.1).
        m.cpi.record(intent.good, intent.agreed_price, intent.qty);
        // Koperta śledzi pieniądze, które wyszły — nie te, które ktoś rozważył.
        let hh = intent.household.entity().index() as usize;
        if let Some(b) = m.budgets.get_mut(hh) {
            b.charge(intent.cat, intent.agreed_price);
        }
        m.stats.purchases += 1;
        m.stats.purchased_qty += intent.qty.get();
        m.stats.revenue = m
            .stats
            .revenue
            .checked_add(intent.agreed_price)
            .unwrap_or(m.stats.revenue);
    }
}

fn shop_coord(pos: Vec2) -> WorldCoord {
    WorldCoord::new((pos.x * 100.0) as i32, (pos.y * 100.0) as i32, 0)
}

/// Kategorie zapasu zaspokajające tę potrzebę — odwrotność `StockCat::need()`.
///
/// Tablica na stosie z licznikiem, nie `Vec`: to jest gorąca ścieżka decyzji
/// zakupowej (§7.3 żąda zera alokacji), a kategorii jest najwyżej osiem.
fn cats_of(need: NeedKind) -> ([StockCat; STOCK_CAT_COUNT], usize) {
    let mut v = [StockCat::Food; STOCK_CAT_COUNT];
    let mut n = 0;
    for c in StockCat::ALL {
        if c.need() == need {
            v[n] = *c;
            n += 1;
        }
    }
    (v, n)
}

impl PlaceProvider for Market {
    fn candidates(
        &self,
        need: NeedKind,
        from: PlaceRef,
        max_travel_min: u16,
        known: &KnowledgeView<'_>,
        who: &CitizenView<'_>,
        out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    ) {
        out.clear();
        let mut m = self.lock();
        let (cats_buf, cats_n) = cats_of(need);
        let cats = &cats_buf[..cats_n];
        // Potrzeba bez kategorii zapasu (sen, wypoczynek, kontakty) nie jest zakupem,
        // a gospodarstwo z zapasem nie musi wychodzić — w obu wypadkach M3 wie lepiej
        // i M5 nie odbiera mu decyzji.
        if cats.is_empty() || m.has_home_stock(who.identity.household, cats) {
            m.fallback
                .candidates(need, from, max_travel_min, known, who, out);
            return;
        }
        let Some(origin_c) = m.places.coord_of(from) else {
            return;
        };
        let origin = Vec2::new(origin_c.x as f32 / 100.0, origin_c.y as f32 / 100.0);
        let snap = m.snapshot(who.identity.household);
        let status = Q::new(who.vitals.status);
        let openness = who.personality.get(TraitId::Openness);
        let vot = m.vot_for(status);
        let dni = m.data.purchase_days;
        let (radius, k_min, k_max) = (m.data.radius_m, m.data.k_min, m.data.k_max);

        let mut cand = std::mem::take(&mut m.cand_buf);
        let mut buf = std::mem::take(&mut m.offer_buf);

        // Promień rozszerzany **raz**, jeśli kandydatów jest mniej niż `k_min` (§5.4).
        zbierz_kandydatow(
            &m,
            KandydaciCtx {
                cats,
                origin,
                origin_c,
                radius,
                k_min,
                dni,
                max_travel_min,
                rozmiar: snap.size,
            },
            known,
            &mut cand,
            &mut buf,
        );
        buf.clear();
        m.offer_buf = buf;

        if cand.is_empty() {
            m.stats.no_candidates += 1;
            m.cand_buf = cand;
            return;
        }

        // Wstępny ranking **całkowitoliczbowy przed** wyceną użyteczności (R6/R7):
        // koszt w groszach, remisy po `(SiteId, GoodId)`, potem obcięcie do `k_max`.
        cand.sort_unstable_by_key(|c| {
            let koszt = c
                .price_total
                .get()
                .saturating_add(c.travel_money.get())
                .saturating_add(i64::from(c.travel_min).saturating_mul(vot));
            (koszt, c.site.entity().index(), c.good.get())
        });
        cand.truncate(k_max);

        // Po obcięciu wracamy do porządku **tożsamościowego** `(SiteId, GoodId)`.
        // To nie jest kosmetyka. `softmax_pick` idzie po sumie skumulowanej w kolejności
        // wejścia, więc gdyby kolejnością było „od najtańszego", podniesienie ceny
        // w jednym sklepie przestawiałoby całą tablicę i ten sam los trafiałby w innego
        // kandydata — udział rynkowy przestawałby reagować na cenę monotonicznie.
        // Kryterium WP4 („podwyżka o 10 % przesuwa udział w dół dla **każdej**
        // z 20 populacji") wyłapało to od razu. Porządek tożsamościowy jest stały
        // wobec cen, a determinizm sumowania (`K-6`) trzyma się tak samo dobrze.
        cand.sort_unstable_by_key(|c| (c.site.entity().index(), c.good.get()));

        // Użyteczność liczona **po** posortowaniu: to sortowanie jest jedynym miejscem,
        // w którym ustala się kolejność sumowania w softmaxie (`K-6`).
        let w = weights_for(who.personality, status, need, &m.data);
        let mut utils = std::mem::take(&mut m.util_buf);
        utils.clear();
        for c in cand.iter() {
            let cat = m.goods.spec(c.good).map_or(cats[0], |s| s.cat);
            let st = m.buyer_state(status, openness, vot, cat, who.identity.household);
            let noise = offer_noise(
                m.seed,
                who.id.entity().index(),
                c.offer,
                m.tick,
                m.data.noise_sigma,
            );
            utils.push(utility_of_offer(c, &w, &st, noise));
        }
        let wybor = choose_offer(
            &utils,
            m.data.temperature,
            m.seed,
            who.id.entity().index(),
            m.tick,
        )
        .unwrap_or(0);

        // Zapamiętanie decyzji: `fulfil` nie zna kupującego, a próg i wagi są jego.
        let wybrany = cand[wybor];
        let cat = m.goods.spec(wybrany.good).map_or(cats[0], |s| s.cat);

        let plan = PlannedPurchase {
            site: wybrany.site,
            need,
            weights: w,
            status,
            openness,
            vot_gr_per_min: vot,
            // Próg z kopertą: wyczerpany budżet kategorii podnosi go przez człon
            // `k_envelope`, czyli odróżnia „nie stać mnie" od „nie warto" (§5.4).
            threshold: purchase_threshold(
                need,
                who.needs.get(need),
                m.overspend_bp(who.identity.household, cat),
                &m.data,
            ),
            unit_price: m
                .offers
                .get(wybrany.offer)
                .map_or(Money::ZERO, |o| o.unit_price),
            good: wybrany.good,
            district: who.residence.district,
        };
        m.planned.insert(who.id.entity().index(), plan);

        // ── utracona sprzedaż u tych, którzy przegrali wybór (WP12) ──────────────
        //
        // **To jest właściwa połowa pytania „dlaczego Anna nie kupiła u mnie".**
        // Do tej pory pierścień zapisywał wyłącznie wizyty, które doszły do sklepu
        // i tam się nie udały — a mieszkaniec, który porównał moją cenę z ceną
        // konkurenta i poszedł do niego, **nigdy do mnie nie wchodzi** i nie
        // zostawiał po sobie śladu. Scena z §1 dokumentu fazy („cena o 12 % wyższa
        // niż w *Dobry Koszyk*, 700 m dalej") jest dokładnie tym przypadkiem, więc
        // bez tego zapisu kryterium WP12 spełniałoby się tożsamościowo na zbiorze,
        // który je omija. Stąd też bierze się wartość pola `went_to`, które do dziś
        // było zawsze `None`.
        //
        // Koszt: jeden odczyt bitu `tracking` na kandydata (≤ 15 na decyzję),
        // czyli tyle samo, co kosztuje pierścień w `fulfil`.
        let dzien = u32::try_from(m.tick.get() / magnat_core::time::MINUTES_PER_DAY).unwrap_or(0);
        for (i, c) in cand.iter().enumerate() {
            if i == wybor || c.site == wybrany.site {
                continue;
            }
            let Some(j) = m.by_site.get(&c.site).copied() else {
                continue;
            };
            let poziom = m.shops[j as usize].tracking;
            if poziom == LostSaleTracking::None {
                continue;
            }
            // Powód: czym przegrał wobec zwycięzcy. Kolejność sprawdzania jest
            // kolejnością tego, co gracz może z tym zrobić — cena najpierw, bo
            // ją ustawia sam; jakość i odległość są własnością zakładu.
            let cause = if c.price_total.get() > wybrany.price_total.get() {
                RejectCause::PriceTooHigh
            } else if c.travel_min > wybrany.travel_min {
                RejectCause::TooFar
            } else if c.quality.get() < wybrany.quality.get() {
                RejectCause::QualityBelowStatus
            } else {
                RejectCause::BelowThreshold
            };
            let sale = LostSale {
                citizen: who.id,
                good: c.good,
                when: m.tick,
                cause,
                went_to: Some(wybrany.site),
            };
            m.shops[j as usize].lost.record(poziom, dzien, sale);
        }

        // Wybrany idzie pierwszy — planer bierze `out.first()` jako decyzję; reszta
        // malejąco po użyteczności, żeby miał czym zastąpić zamknięty sklep.
        let mut porzadek = std::mem::take(&mut m.order_buf);
        porzadek.clear();
        porzadek.extend(0..cand.len());
        porzadek.sort_by(|a, b| {
            (*a != wybor)
                .cmp(&(*b != wybor))
                .then_with(|| utils[*b].total_cmp(&utils[*a]))
                .then_with(|| a.cmp(b))
        });
        for (poz, i) in porzadek.iter().enumerate() {
            if out.is_full() {
                break;
            }
            let c = cand[*i];
            let runner = porzadek
                .get(poz + 1)
                .map_or(c.travel_min, |j| cand[*j].travel_min);
            out.push(PlaceCandidate {
                place: PlaceRef::Site(c.site),
                travel_min: c.travel_min,
                score: (utils[*i] * 1_000.0) as i32,
                // Powód slotu planu musi się mieścić w bajcie (`PlanSlot.reason`,
                // M3a §5.1), a blok M5 zaczyna się od 300. Plan dnia niesie więc
                // powód **wyjścia**; pełne uzasadnienie zakupu (`ShopChosen` z członem
                // dominującym) powstaje w `fulfil`, gdzie zapada decyzja o pieniądzach,
                // i idzie do dziennika transakcji oraz do karty inspekcji.
                reason: DecisionReason::ChosenNearest {
                    travel_min: c.travel_min,
                    runner_up_min: runner,
                },
            });
        }
        porzadek.clear();
        m.order_buf = porzadek;
        utils.clear();
        m.util_buf = utils;
        cand.clear();
        m.cand_buf = cand;
    }

    fn opening_hours(&self, place: PlaceRef) -> OpenHours {
        let m = self.lock();
        if let PlaceRef::Site(s) = place {
            if let Some(i) = m.by_site.get(&s) {
                return m.shops[*i as usize].hours;
            }
        }
        m.fallback.opening_hours(place)
    }

    fn fulfil(&mut self, req: &FulfilRequest) -> FulfilOutcome {
        Market::fulfil(self, req)
    }
}

impl Market {
    /// Wizyta w sklepie — ciało `PlaceProvider::fulfil`.
    ///
    /// Metoda inherentna bierze `&self`, bo `Market` jest uchwytem do wspólnego
    /// stanu (`Arc<Mutex<…>>`) i mutowalność jest **w środku**, nie w uchwycie.
    /// Trait wymaga `&mut self`, więc deleguje tutaj; wołający spoza pętli doby
    /// (scenariusz, test, panel) nie musi przez to trzymać rynku mutowalnie.
    #[allow(clippy::too_many_lines)]
    pub fn fulfil(&self, req: &FulfilRequest) -> FulfilOutcome {
        let mut m = self.lock();
        let PlaceRef::Site(site) = req.place else {
            return m.fallback.fulfil(req);
        };
        let Some(i) = m.by_site.get(&site).copied() else {
            return m.fallback.fulfil(req);
        };
        let (cats_buf, cats_n) = cats_of(req.need);
        let cats = &cats_buf[..cats_n];
        if cats.is_empty() {
            return m.fallback.fulfil(req);
        }
        let tick = m.tick;
        let day = (tick.get() / magnat_core::time::MINUTES_PER_DAY) as u32;
        let hh = req.household.entity().index();
        let zaklepane = m.committed.get(&hh).copied().unwrap_or(Money::ZERO);
        let budzet = Money(req.budget_hint.get().saturating_sub(zaklepane.get()).max(0));
        let dni = m.data.purchase_days;
        let osob = req.household_size;
        let tracking = m.shops[i as usize].tracking;
        let slippage = i64::from(m.data.price_slippage_bp);

        // Decyzja z planu dnia. Jej brak (mieszkaniec trafił tu inną ścieżką) nie jest
        // błędem — wtedy kupujący jest „przeciętny": wagi z osobowości neutralnej.
        let plan = m
            .planned
            .get(&who_key(req))
            .copied()
            .filter(|p| p.site == site);
        let (w, status, openness, vot, prog, cena_z_decyzji) = match plan {
            Some(p) => (
                p.weights,
                p.status,
                p.openness,
                p.vot_gr_per_min,
                p.threshold,
                Some(p.unit_price),
            ),
            None => {
                let neutral = magnat_agents::Personality([50; 8]);
                let st = Q::new(50);
                (
                    weights_for(&neutral, st, req.need, &m.data),
                    st,
                    Q::new(50),
                    m.vot_for(st),
                    purchase_threshold(
                        req.need,
                        Q::new(50),
                        m.overspend_bp(req.household.entity().index(), cats[0]),
                        &m.data,
                    ),
                    None,
                )
            }
        };

        // Towary kategorii w kolejności rangi substytutu — to jest ścieżka „substytut
        // niższego rzędu" z PRD §6.4, bez rekurencji: jedna pętla po malejącej randze,
        // wygrywa pierwszy towar, który przechodzi wszystkie progi.
        // Towary kategorii na stos, nie do `Vec`: `fulfil` woła się raz na wizytę,
        // czyli rzędu miliona razy na dobę metropolii (§7.3 — zero alokacji).
        let mut kolejnosc = [0u32; MAX_LINES];
        let mut n_kolejnosc = 0usize;
        for c in cats {
            for gi in m.goods.in_cat(*c) {
                if n_kolejnosc == MAX_LINES {
                    break;
                }
                kolejnosc[n_kolejnosc] = *gi;
                n_kolejnosc += 1;
            }
        }
        let mut powod = RejectCause::OutOfStock;
        let mut detal = 0i16;
        let mut brakowalo = 0i16;

        for gi in &kolejnosc[..n_kolejnosc] {
            let spec = *m.goods.at(*gi);
            let Some(line) = m.shops[i as usize].shelf.line(spec.good).copied() else {
                continue;
            };
            // Ilość na półce czyta się z **magazynu** (WP11), nie z linii: linia jest
            // ekspozycją, a towar leży w slocie o roli `Shelf`.
            let na_polce = m.shelf_units(i as usize, spec.good);
            if na_polce.get() <= 0 {
                continue;
            }
            let Some(offer) = m.offers.get(line.offer).copied() else {
                continue;
            };
            let want = wanted_qty(spec.daily_per_person, osob, dni);
            let take = Qty(want.get().min(na_polce.get()));
            let total = line_total(offer.unit_price, take);
            if total > budzet {
                powod = RejectCause::BudgetExhausted;
                continue;
            }
            // Poślizg ceny (§5.5): jeśli cena ruszyła się między decyzją a wizytą
            // o więcej niż `price_slippage_bp`, próg ocenia się **ponownie** — i to
            // jest ta jedna ponowna ocena, bez rekurencji.
            if let Some(stara) = cena_z_decyzji
                .filter(|c| c.get() > 0 && spec.good == plan.map_or(GoodId(u16::MAX), |p| p.good))
            {
                let delta = (offer.unit_price.get() - stara.get()).abs() * 10_000 / stara.get();
                if delta > slippage {
                    m.stats.slippage_rechecks += 1;
                }
            }
            let st = m.buyer_state(
                status,
                openness,
                vot,
                spec.cat,
                req.household.entity().index(),
            );
            let cand = Candidate {
                offer: line.offer,
                site,
                good: spec.good,
                qty: take,
                price_total: total,
                travel_min: 0,
                travel_money: Money::ZERO,
                quality: offer.quality,
                rating: None,
                visited: true,
            };
            let u = utility_of_offer(&cand, &w, &st, 0.0);
            if u < prog {
                powod = RejectCause::BelowThreshold;
                brakowalo = ((u - prog) * 1_000.0).clamp(-32_000.0, 32_000.0) as i16;
                continue;
            }

            // Wszystko przeszło: półka schodzi **teraz**, pieniądz w rozliczeniu.
            let dominujacy = dominant_term(&cand, &w, &st);
            // Sprzedaż zdejmuje partię z półki w porządku FEFO i **stąd** bierze się
            // koszt własny — z partii, a nie ze średniej ważonej całej linii. To jest
            // krok 5 migracji z §6.3 i to on sprawia, że marża wreszcie liczy się
            // od czegoś, co naprawdę zapłacono.
            let Some(kawalek) = m.shelf_pick(i as usize, spec.good, take) else {
                powod = RejectCause::OutOfStock;
                continue;
            };
            let koszt_wlasny = kawalek.cost_total;
            if let Some(o) = m.offers.get_mut(line.offer) {
                o.available = Qty((o.available.get() - take.get()).max(0));
            }
            let dni_kupione = days_bought(spec.daily_per_person, osob, take);
            let delta_bp = cena_z_decyzji.map_or(0i16, |stara| {
                if stara.get() <= 0 {
                    0
                } else {
                    (((offer.unit_price.get() - stara.get()) * 10_000 / stara.get())
                        .clamp(-32_000, 32_000)) as i16
                }
            });
            let reason = DecisionReason::ShopChosen {
                site,
                dominant: dominujacy,
                delta_bp,
            };
            m.intents.push(PurchaseIntent {
                buyer: req.citizen,
                household: req.household,
                site,
                offer: line.offer,
                good: spec.good,
                cat: spec.cat,
                qty: take,
                days: dni_kupione,
                agreed_price: total,
                cogs: koszt_wlasny,
                taken: Some(kawalek),
                arrived: tick,
                reason,
                district: plan.map_or(0, |p| p.district),
                status,
            });
            let suma = zaklepane.get().saturating_add(total.get());
            m.committed.insert(hh, Money(suma));
            // Zaspokojenie proporcjonalne do tego, ile dni zapasu udało się kupić:
            // kto kupił połowę koszyka, ma zaspokojoną połowę potrzeby (`Z-2`).
            let pelne = u32::from(m.needs.spec(req.need).satisfaction);
            let gain = (pelne * u32::from(dni_kupione.max(1)) / u32::from(dni.max(1))).min(100);
            let visit = m.needs.spec(req.need).visit_min;
            return FulfilOutcome::Done {
                satisfaction: Q::new(gain as u8),
                spent: total,
                duration_min: visit,
                reason,
            };
        }

        // Nic nie przeszło — to jest utracona sprzedaż i tu się ją zapisuje (PRD §14.1).
        match powod {
            RejectCause::OutOfStock => m.stats.stockouts += 1,
            RejectCause::BudgetExhausted => m.stats.budget_refusals += 1,
            _ => m.stats.deferrals += 1,
        }
        let sale = LostSale {
            citizen: req.citizen,
            good: plan.map_or(GoodId(0), |p| p.good),
            when: tick,
            cause: powod,
            went_to: None,
        };
        m.shops[i as usize].lost.record(tracking, day, sale);
        let reason = if powod == RejectCause::BelowThreshold {
            DecisionReason::PurchaseDeferred {
                need: req.need,
                cause: powod,
                gap_permille: brakowalo,
            }
        } else {
            DecisionReason::OfferRejected {
                site,
                cause: powod,
                detail: detal,
            }
        };
        detal = 0;
        let _ = detal;
        FulfilOutcome::Refused(reason)
    }
}

fn who_key(req: &FulfilRequest) -> u32 {
    req.citizen.entity().index()
}


/// Co zapytanie o kandydatów wie o kupującym i o jego zasięgu.
///
/// Struktura zamiast ośmiu argumentów: `candidates` przekroczyło twardy próg długości
/// funkcji z `CLAUDE.md` po dołożeniu odczytu półki z magazynu (WP11), a zbieranie
/// kandydatów jest w nim jedynym kawałkiem z własnym, domkniętym zadaniem — reszta
/// to ranking i wybór. Podział jest mechaniczny: przeniesienie symboli bez zmiany
/// zachowania.
struct KandydaciCtx<'a> {
    cats: &'a [StockCat],
    origin: Vec2,
    origin_c: WorldCoord,
    radius: u32,
    k_min: usize,
    dni: u8,
    max_travel_min: u16,
    rozmiar: u8,
}

/// Zbiera oferty w zasięgu, odsiewa te, których kupujący nie zna albo które nie mają
/// dość towaru, i zamienia je w kandydatów. Promień rozszerza się **raz**, jeśli
/// kandydatów jest mniej niż `k_min` (§5.4).
fn zbierz_kandydatow(
    m: &MarketInner,
    ctx: KandydaciCtx<'_>,
    known: &KnowledgeView<'_>,
    cand: &mut Vec<Candidate>,
    buf: &mut Vec<OfferId>,
) {
    for proba in 0..2u32 {
        cand.clear();
        let r = ctx.radius * (proba + 1);
        for c in ctx.cats {
            query_offers(&m.index, CategoryId::Stock(*c), ctx.origin, r, buf);
            for id in buf.iter() {
                let Some(o) = m.offers.get(*id) else { continue };
                if o.available.get() <= 0 || !known.knows(PlaceRef::Site(o.site)) {
                    continue;
                }
                let Some(i) = m.by_site.get(&o.site).copied() else {
                    continue;
                };
                let Some(spec) = m.goods.spec(o.good) else {
                    continue;
                };
                let want = wanted_qty(spec.daily_per_person, ctx.rozmiar, ctx.dni);
                if o.available.get() < want.get() {
                    continue;
                }
                let travel_min =
                    walk_minutes(ctx.origin_c, shop_coord(m.shops[i as usize].pos), 100);
                if travel_min > ctx.max_travel_min {
                    continue;
                }
                let (rating, visited) = rating_of(known.entry(PlaceRef::Site(o.site)));
                cand.push(Candidate {
                    offer: *id,
                    site: o.site,
                    good: o.good,
                    qty: want,
                    price_total: line_total(o.unit_price, want),
                    travel_min,
                    travel_money: Money::ZERO,
                    quality: o.quality,
                    rating,
                    visited,
                });
            }
        }
        if cand.len() >= ctx.k_min {
            break;
        }
    }
}
