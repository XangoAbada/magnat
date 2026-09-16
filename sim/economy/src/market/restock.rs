//! Zaopatrzenie i półka: wyłożenie, zamówienie, odpis terminu (szew (b)).

use super::*;

impl Market {
    /// Uzupełnienie półek z zaplecza (§5.3, co godzinę).
    pub fn restock_shelves(&self) {
        let mut m = self.lock();
        let mut restocks = 0u64;
        for i in 0..m.shops.len() {
            let braki: Vec<(GoodId, Qty)> = m.shops[i]
                .shelf
                .lines
                .iter()
                .filter_map(|l| {
                    let cap = SHELF_UNITS_PER_FACING * i64::from(l.facings.max(1));
                    let brak = cap - l.qty.get();
                    (brak > 0).then_some((l.good, Qty(brak)))
                })
                .collect();
            for (good, brak) in braki {
                let Some(line) = m.shops[i].inventory.backroom.get_mut(&good) else {
                    continue;
                };
                let take = Qty(brak.get().min(line.qty.get()));
                if take.get() <= 0 {
                    continue;
                }
                let cost = line.take(take);
                let data = line.expires;
                let Some(sl) = m.shops[i].shelf.line_mut(good) else {
                    continue;
                };
                // Ta sama reguła co w `StockLine::receive`: półka wyczerpana nie
                // ma czego przeterminować, więc nie przenosi swojej daty na towar
                // dołożony po opróżnieniu.
                if sl.qty.get() <= 0 {
                    sl.expires = None;
                }
                sl.qty = Qty(sl.qty.get() + take.get());
                sl.cost_total = sl
                    .cost_total
                    .checked_add(cost)
                    .expect("półka: przepełnienie kosztu linii");
                // Data ważności idzie z zapleczem na półkę — wcześniejsza z dwóch,
                // tak samo jak przy dostawie. Bez niej odpis (§5.8) i przecena
                // psującego się (§5.6) nie miałyby czego czytać o towarze wyłożonym.
                sl.expires = match (sl.expires, data) {
                    (Some(a), Some(b)) => Some(SimMinute(a.get().min(b.get()))),
                    (a, b) => a.or(b),
                };
                let (offer, qty) = (sl.offer, sl.qty);
                if let Some(o) = m.offers.get_mut(offer) {
                    o.available = qty;
                }
                restocks += 1;
            }
        }
        m.stats.restocks += restocks;
    }

    /// Zamówienia u dostawcy zewnętrznego i odbiór tego, co dojechało (§5.7).
    pub fn reorder_and_receive(&self, books: &mut Books, t: Tick) {
        let mut m = self.lock();
        let rest = m.rest_of_world;

        // 1. Odbiór. Dostawa jest zapłacona przy zamówieniu, więc tu jedzie sam towar.
        let mut deliv = std::mem::take(&mut m.deliv_buf);
        m.supplier.poll_deliveries(t, &mut deliv);
        m.stats.deliveries += deliv.len() as u64;
        for d in &deliv {
            let Some(i) = m.by_site.get(&d.site).copied() else {
                continue;
            };
            m.shops[i as usize]
                .inventory
                .backroom
                .entry(d.good)
                .or_default()
                .receive(d.qty, d.paid, d.expires);
            post_receipt(&mut m.shops[i as usize].ledger, d.paid, t);
        }
        deliv.clear();
        m.deliv_buf = deliv;

        // 2. Zamówienia. Sklepy w kolejności zakładania, towary w kolejności `BTreeMap` —
        // obie deterministyczne (00 §3.2).
        for i in 0..m.shops.len() {
            let braki: Vec<(GoodId, Qty)> = m.shops[i]
                .inventory
                .reorder
                .iter()
                .filter_map(|(g, p)| {
                    let have = m.shops[i]
                        .inventory
                        .backroom
                        .get(g)
                        .map_or(0, |l| l.qty.get());
                    let cel = docelowy_zapas(&m.shops[i], *g, p, m.supplier.goods(), t);
                    (have < p.point.get().min(cel)).then(|| (*g, Qty(cel - have)))
                })
                .collect();
            let (site, firm, account) = (m.shops[i].site, m.shops[i].firm, m.shops[i].account);
            for (good, qty) in braki {
                let Some(q) = m.supplier.quote(good, qty, site, t) else {
                    continue;
                };
                // Sklep bez środków nie zamawia — i to jest cała „upadłość" w M5b.
                // Prawdziwe postępowanie prowadzi M7 (`K-10`).
                if books
                    .transfer(
                        account,
                        rest,
                        q.total(),
                        wholesale_memo(
                            good,
                            q.qty,
                            m.supplier
                                .goods()
                                .spec(good)
                                .map_or(StockCat::Other, |s| s.cat),
                            0,
                        ),
                        t,
                    )
                    .is_err()
                {
                    continue;
                }
                post_purchase(&mut m.shops[i].ledger, q.total(), t);
                let _ = m.supplier.place_order(&q, firm, site, t);
            }
        }
    }

    /// Odpis towaru przeterminowanego (§5.8). Zwraca łączną wartość odpisu.
    ///
    /// Linia zapasu ma **jedną** datę ważności (M5 nie ma partii), więc przeterminowuje
    /// się w całości naraz. M6 zastąpi to odpisem per `BatchId` i wtedy dopiero będzie
    /// co odpisywać częściami.
    pub fn expire_goods(&self, t: Tick) -> Money {
        let mut m = self.lock();
        let teraz = t.get();
        let mut razem = Money::ZERO;
        for i in 0..m.shops.len() {
            let mut odpis = Money::ZERO;
            let mut sztuk = 0i64;

            let zaplecze: Vec<GoodId> = m.shops[i]
                .inventory
                .backroom
                .iter()
                .filter(|(_, l)| l.qty.get() > 0 && l.expires.is_some_and(|e| e.get() <= teraz))
                .map(|(g, _)| *g)
                .collect();
            for g in zaplecze {
                if let Some(l) = m.shops[i].inventory.backroom.get_mut(&g) {
                    let q = l.qty;
                    sztuk += q.get();
                    odpis = Money(odpis.get() + l.take(q).get());
                    l.expires = None;
                }
            }

            let polka: Vec<GoodId> = m.shops[i]
                .shelf
                .lines
                .iter()
                .filter(|l| l.qty.get() > 0 && l.expires.is_some_and(|e| e.get() <= teraz))
                .map(|l| l.good)
                .collect();
            for g in polka {
                let Some(sl) = m.shops[i].shelf.line_mut(g) else {
                    continue;
                };
                let q = sl.qty;
                sztuk += q.get();
                odpis = Money(odpis.get() + sl.take(q).get());
                sl.expires = None;
                let offer = sl.offer;
                if let Some(o) = m.offers.get_mut(offer) {
                    o.available = Qty::ZERO;
                }
            }

            if odpis.get() > 0 {
                let _ = ledger::post(
                    &mut m.shops[i].ledger,
                    JournalEntry::new(
                        t,
                        DecisionReason::Unspecified,
                        &[
                            (LedgerAccount::WriteOffExpense, odpis),
                            (LedgerAccount::InventoryGoods, Money(-odpis.get())),
                        ],
                    ),
                );
                m.stats.write_offs += 1;
                m.stats.write_off_value = Money(m.stats.write_off_value.get() + odpis.get());
                m.stats.expired_qty += sztuk;
                razem = Money(razem.get() + odpis.get());
            }
        }
        razem
    }
}

/// Zapłata dostawcy z góry: pieniądz wyszedł, towar jeszcze nie przyjechał.
///
/// `TradePayable` chodzi przez ten czas na saldzie Wn — to jest zaliczka, a nie
/// zobowiązanie. M6 wnosi terminy płatności i saldo staje się tym, czym nazwa
/// obiecuje; do tego czasu jedno konto niesie obie strony, bo obie są rozrachunkiem
/// z tym samym dostawcą.
/// Docelowy zapas zaplecza: **z popytu, nie z metrażu półki**.
///
/// To jest naprawa najmocniejszego pojedynczego sygnału, jaki znalazł balansator
/// przy zamknięciu M5e: odpisy towaru przeterminowanego sięgały **piętnastokrotności
/// obrotu**. Mechanizm był taki: `ReorderPolicy.target` stała na wielokrotności
/// wyłożenia półki (`SHELF_UNITS_PER_FACING`), czyli na **metrażu lokalu**, a nie na
/// tym, ile sklep sprzedaje. Osiedlowy sklep o dużej powierzchni i małym ruchu
/// zamawiał więc dziesiątki dób sprzedaży chleba o trzydniowym terminie i odpisywał
/// prawie wszystko. Sufit z terminu ważności (M5c) tego nie łapał, bo ograniczał
/// **wielokrotność wyłożenia**, a nie **liczbę dób sprzedaży**.
///
/// Wejściem jest okno obrotu tygodniowego z `PriceController` — to samo, które panel
/// pokazuje jako `turnover_7d`, i dokładnie ta rzecz, której M5 nie miała do M5e.
///
/// **Zerowy obrót znaczy dwie różne rzeczy i to jest sedno poprawki.** W sklepie
/// świeżo otwartym znaczy „jeszcze nie wiem" — i wtedy obowiązuje polityka statyczna,
/// bo zapas startowy musi skądś być. W sklepie działającym od tygodnia znaczy
/// **„nikt tego u mnie nie kupuje"** — i wtedy zamówienie jest zerowe. Pierwsza
/// wersja tej funkcji traktowała oba przypadki tak samo i nie zmieniła niczego:
/// sklep trzyma 18 towarów, a mieszkańcy kupują kilka, więc większość par
/// (zakład, towar) ma obrót zerowy **na stałe** i to one odpowiadały za odpisy.
/// Półka zostaje wyłożona i widoczna (`available == 0` to „znam, nie ma" — §5.3),
/// więc pierwszy klient, który jednak kupi, wznawia zamawianie sam.
///
/// Pokrycie: `lead_time + 2` doby, przycięte terminem ważności. Dwie doby zapasu
/// ponad czas dostawy to bufor na wahania ruchu, nie model — model zapasu z kosztem
/// braku i kosztem kapitału należy do M6 razem z realnym dostawcą.
fn docelowy_zapas(shop: &Shop, good: GoodId, p: &ReorderPolicy, goods: &GoodTable, t: Tick) -> i64 {
    let tygodniowo = shop
        .controllers
        .get(&good)
        .map_or(0, |pc| pc.turnover_7d().get());
    if tygodniowo <= 0 {
        let wiek_dob =
            t.get().saturating_sub(shop.opened.get()) / magnat_core::time::MINUTES_PER_DAY;
        // Młody sklep zamawia **jedno wyłożenie**, nie pełne zaplecze: półka ma być
        // widoczna i mieć co sprzedać, a nie nieść tygodniowy zapas towaru, o którym
        // nikt jeszcze nie wie, czy w ogóle schodzi.
        return if wiek_dob < 7 {
            p.target.get().min(SHELF_UNITS_PER_FACING)
        } else {
            0
        };
    }
    // Sklep, który **sprzedaje**, zamawia dalej wg polityki statycznej.
    //
    // To jest granica poprawki i warto wiedzieć, dlaczego biegnie właśnie tutaj:
    // wersja wiążąca cel zamówienia z obrotem także dla sklepów sprzedających
    // **odwróciła mechanizm inflacji emergentnej z WP10**. Większy popyt podnosił
    // wtedy cel zamówienia, zapas wracał do celu, `adj_stock` w `reprice` schodził
    // na minus i czterokrotna akcja kredytowa **obniżała** CPI zamiast go podnieść
    // (zmierzone: 9 793 → 8 239 wobec bazy 10 000, test
    // `wieksza_akcja_kredytowa_przy_stalej_podazy_dobr_podnosi_cpi`). Presja zapasu
    // jest w M5 jedynym kanałem, którym pieniądz dochodzi do cen — dostawca
    // zewnętrzny ma nieskończoną podaż po stałej cenie — więc tłumienie jej tutaj
    // wywraca bramki G1–G3 razem z kryterium WP10.
    //
    // Prawdziwy sterownik zapasu z kosztem braku i kosztem kapitału należy do M6
    // razem z realnym dostawcą; wtedy będzie też **czym** podnieść cenę hurtową.
    let _ = (tygodniowo, goods);
    p.target.get()
}
