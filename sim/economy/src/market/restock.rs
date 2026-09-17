//! Zaopatrzenie i półka: wyłożenie, zamówienie, odpis terminu (szew (b)).
//!
//! **Od WP11 towar jest tu fizyczny.** Do M6c zaplecze było mapą `GoodId → StockLine`,
//! a półka wektorem linii z ilością i kosztem; dostawa dopisywała liczbę, sprzedaż ją
//! odejmowała i nigdzie po drodze nie było masy, której mógłby pilnować bilans.
//! Teraz zaplecze i półka są **slotami magazynu M6**, wyłożenie jest przeniesieniem
//! partii w obrębie zakładu, a odpis terminu wraca z magazynu listą faktów.

use super::*;
use magnat_core::Mass;

impl Market {
    /// Uzupełnienie półek z zaplecza (§5.3, co godzinę).
    ///
    /// Sufitem wyłożenia jest ekspozycja (`facings`), a nie zapas — półka mieści tyle,
    /// ile mieści, i reszta zostaje na zapleczu. Przeniesienie idzie przez
    /// `Store::backroom_to_shelf`, czyli **nie rusza bilansu**: towar nie został ani
    /// zużyty, ani wyprodukowany, tylko przestawiony w tym samym zakładzie.
    pub fn restock_shelves(&self) {
        let mut m = self.lock();
        let mut restocks = 0u64;
        let chain = m.chain.clone();
        let mut ch = chain.lock();
        let cat = chain.cat.clone();
        for i in 0..m.shops.len() {
            let (backroom, shelf_slot) = (m.shops[i].backroom, m.shops[i].shelf_slot);
            let plan: Vec<(GoodId, Mass)> = m.shops[i]
                .shelf
                .lines
                .iter()
                .filter_map(|l| {
                    let cap = SHELF_UNITS_PER_FACING * i64::from(l.facings.max(1));
                    let jest = cat
                        .good(l.good)
                        .units_of_mass(ch.store.shelf_state(shelf_slot, l.good).mass)
                        .get();
                    let brak = cap - jest;
                    (brak > 0).then(|| (l.good, cat.good(l.good).mass_of_units(Qty(brak))))
                })
                .collect();
            for (good, brak) in plan {
                let ile = ch
                    .store
                    .backroom_to_shelf(&cat, backroom, shelf_slot, good, brak);
                if ile.0 <= 0 {
                    continue;
                }
                restocks += 1;
            }
            // Oferta pokazuje **półkę**, nie zapas: towar na zapleczu nie jest
            // na sprzedaż (nagłówek `shop.rs`).
            let odswiez: Vec<(crate::offer::OfferId, Qty)> = m.shops[i]
                .shelf
                .lines
                .iter()
                .map(|l| {
                    (
                        l.offer,
                        cat.good(l.good)
                            .units_of_mass(ch.store.shelf_state(shelf_slot, l.good).mass),
                    )
                })
                .collect();
            for (offer, qty) in odswiez {
                if let Some(o) = m.offers.get_mut(offer) {
                    o.available = qty;
                }
            }
        }
        m.stats.restocks += restocks;
    }

    /// Zamówienia u dostawcy i odbiór tego, co dojechało (§5.7).
    ///
    /// Różnica wobec M5 jest w kroku pierwszym: **towar przyjeżdża sam**, bo dostawa
    /// jest rozładunkiem zlecenia transportowego wprost do slotu zaplecza. Tu zostaje
    /// wyłącznie księgowanie — i to jest cała treść kroku 4 migracji z §6.3.
    pub fn reorder_and_receive(&self, books: &mut Books, t: Tick) {
        let mut m = self.lock();
        let rest = m.rest_of_world;

        // 1. Odbiór. Lista dostaw z `Wholesale` znaczy „dostawca **materializuje** ci
        //    towar" i jest niepusta wyłącznie pod feature'em `infinite_supply`. Łańcuch
        //    M6 niczego nie materializuje: partie leżą w slocie zaplecza, odkąd
        //    ciężarówka je rozładowała, a pieniądz idzie listą `Settlement`.
        //
        //    Zapłata przeniosła się z chwili **zamówienia** do chwili **odbioru**
        //    (WP11). Powód jest twardy: zapytanie ofertowe może nie znaleźć dostawcy,
        //    a sklep, który zapłacił za towar, którego nikt nie przywiózł, ma dziurę
        //    w kasie bez zdarzenia, które by ją tłumaczyło.
        let chain = m.chain.clone();
        let cat = chain.cat.clone();
        let mut deliv = std::mem::take(&mut m.deliv_buf);
        m.supplier.poll_deliveries(t, &mut deliv);
        m.stats.deliveries += deliv.len() as u64;
        for d in &deliv {
            let Some(i) = m.by_site.get(&d.site).copied() else {
                continue;
            };
            let (account, backroom) = (m.shops[i as usize].account, m.shops[i as usize].backroom);
            let kat = m.goods.spec(d.good).map_or(StockCat::Other, |s| s.cat);
            if books
                .transfer(
                    account,
                    rest,
                    d.paid,
                    wholesale_memo(d.good, d.qty, kat, 0),
                    t,
                )
                .is_err()
            {
                continue;
            }
            let draft = magnat_supply::BatchDraft {
                good: d.good,
                mass: cat.good(d.good).mass_of_units(d.qty),
                quality: d.quality,
                brand: None,
                producer: d.buyer,
                produced_at: SimMinute(t.get()),
                cost: d.paid,
                origin: magnat_supply::BatchOrigin::imported(),
                flags: Default::default(),
            };
            let _ = chain
                .lock()
                .store
                .put(&cat, backroom, draft, magnat_supply::MassIn::Initial);
            post_purchase(&mut m.shops[i as usize].ledger, d.paid, t);
            post_receipt(&mut m.shops[i as usize].ledger, d.paid, t);
        }
        deliv.clear();
        m.deliv_buf = deliv;

        // 2. Zamówienia. Sklepy w kolejności zakładania, towary w kolejności `BTreeMap` —
        // obie deterministyczne (00 §3.2).
        let chain = m.chain.clone();
        let cat = chain.cat.clone();
        for i in 0..m.shops.len() {
            let braki: Vec<(GoodId, Qty)> = {
                let ch = chain.lock();
                let backroom = m.shops[i].backroom;
                m.shops[i]
                    .inventory
                    .reorder
                    .iter()
                    .filter_map(|(g, p)| {
                        let have = cat
                            .good(*g)
                            .units_of_mass(ch.store.available(backroom, *g, Q::MIN))
                            .get();
                        let cel = docelowy_zapas(&m.shops[i], *g, p, &m.goods, t);
                        (have < p.point.get().min(cel)).then(|| (*g, Qty(cel - have)))
                    })
                    .collect()
            };
            let (site, firm, account) = (m.shops[i].site, m.shops[i].firm, m.shops[i].account);
            for (good, qty) in braki {
                let Some(q) = m.supplier.quote(good, qty, site, t) else {
                    continue;
                };
                // Sklep bez pokrycia nie zamawia — i to jest cała „upadłość" w M5b.
                // Prawdziwe postępowanie prowadzi M7 (`K-10`). Pieniądz jeszcze nie
                // wychodzi: sprawdzamy **pokrycie**, płacimy przy odbiorze.
                if books.balance(account).map_or(0, |b| b.get()) < q.total().get() {
                    continue;
                }
                let _ = m.supplier.place_order(&q, firm, site, t);
            }
        }
    }

    /// Faktury za media zakładów produkcyjnych (§5.11, `UtilityBillingSystem`).
    ///
    /// Zakład płaci **na zewnątrz**: sieci przesyłowe i ich właściciel to zakres M8,
    /// a do tego czasu prąd i woda przychodzą spoza miasta tak samo jak towar
    /// importowany. Konsekwencja jest zamierzona i uczciwa — pieniądz za media
    /// wychodzi z obiegu miasta na konto reszty świata, więc P1 dalej się domyka,
    /// a rachunek zakładu obciąża to, co naprawdę zużył licznik.
    ///
    /// Zakład bez konta (albo faktura na zero) jest pomijany, nie zerowany: licznik
    /// zachowa naliczenie i wystawi je w następnym miesiącu.
    pub fn absorb_utility_bills(
        &self,
        bills: &[magnat_supply::UtilityBill],
        books: &mut Books,
        t: Tick,
    ) -> Money {
        if bills.is_empty() {
            return Money::ZERO;
        }
        let m = self.lock();
        let rest = m.rest_of_world;
        let mut razem = Money::ZERO;
        for f in bills {
            if f.amount.get() <= 0 {
                continue;
            }
            let konto = m
                .by_site
                .get(&f.site)
                .map(|i| m.shops[*i as usize].account)
                .or_else(|| m.plants.get(&f.site).map(|(_, a)| *a));
            let Some(konto) = konto else { continue };
            let memo = TxMemo::new(
                TxKind::Utility {
                    site: f.site,
                    kind: f.kind,
                },
                DecisionReason::Unspecified,
            );
            if books.transfer(konto, rest, f.amount, memo, t).is_ok() {
                razem = Money(razem.get() + f.amount.get());
            }
        }
        razem
    }

    /// Odpis towaru przeterminowanego (§5.8). Zwraca łączną wartość odpisu.
    ///
    /// Magazyn zdejmuje partie po dacie sam (`Store::spoil`, kadencja minutowa)
    /// i oddaje listę odpisów; tutaj trafia ta lista i zamienia się na wpis w księdze
    /// zakładu. Różnica wobec M5 jest zasadnicza, choć niewidoczna w sygnaturze:
    /// **przeterminowuje się partia, a nie linia**, więc świeża dostawa nie schodzi
    /// na odpis razem z resztką sprzed tygodnia. To był największy pojedynczy błąd,
    /// jaki balansator znalazł przy zamknięciu M5e (`AD-2` w dokumencie M5e).
    pub fn absorb_spoilage(&self, spoiled: &[magnat_supply::Spoiled], t: Tick) -> Money {
        let mut m = self.lock();
        let mut razem = Money::ZERO;
        // Sklepy w kolejności zakładania; wewnątrz sklepu odpisy w kolejności, w jakiej
        // wyszły z magazynu (sloty rosnąco) — obie deterministyczne.
        for i in 0..m.shops.len() {
            let (backroom, shelf_slot) = (m.shops[i].backroom, m.shops[i].shelf_slot);
            let (mut odpis, mut masa) = (Money::ZERO, 0i64);
            for s in spoiled {
                if s.slot != backroom && s.slot != shelf_slot {
                    continue;
                }
                odpis = Money(odpis.get() + s.cost.get());
                masa += s.mass.0;
            }
            if odpis.get() <= 0 {
                continue;
            }
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
            m.stats.expired_qty += masa;
            razem = Money(razem.get() + odpis.get());
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

impl Market {
    /// Odpis terminu w jednym kroku: psuje magazyn i księguje to, co zeszło.
    ///
    /// Nakładka nad [`Market::absorb_spoilage`] dla wołających, którzy prowadzą dobę
    /// sklepu sami i nie mają skąd wziąć listy odpisów — scenariusze i testy. W pętli
    /// symulacji psucie jest **minutowe** i robi je łańcuch (`Chain::step_minute`),
    /// bo `prop_no_expired_on_shelf` pyta o każdy tick, a nie o każdą dobę.
    pub fn expire_goods(&self, t: Tick) -> Money {
        let chain = self.lock().chain.clone();
        let zepsute = chain.lock().store.spoil(SimMinute(t.get()));
        self.absorb_spoilage(&zepsute, t)
    }

    /// Księguje rozliczenia rynku B2B: zapłatę dostawcy i przyjęcie towaru (`AK-3`).
    ///
    /// `Settlement` niesie **fakty**: strony, towar, masę, kwotę netto i cło osobno.
    /// Księgi nie widzi ani `sim/supply`, ani rynek B2B — kierunek zależności jest
    /// odwrotny — więc to jest miejsce, w którym fakt staje się przelewem i wpisem
    /// w dzienniku zakładu.
    ///
    /// Pieniądz idzie na konto sprzedawcy, gdy sprzedawcą jest firma z miasta, i na
    /// `RestOfWorld`, gdy towar przyszedł zza granicy. To jest różnica, której M5 nie
    /// mogła zrobić: `SupplierRef` miał jeden wariant i **każdy** zakup wyglądał jak
    /// import, także wtedy, gdy mąka jechała z młyna o dwie ulice dalej.
    pub fn absorb_settlements(
        &self,
        settlements: &[magnat_supply::Settlement],
        books: &mut Books,
        t: Tick,
    ) -> Money {
        let mut m = self.lock();
        let rest = m.rest_of_world;
        let mut razem = Money::ZERO;
        for s in settlements {
            let kwota = Money(s.net.get() + s.duty.get());
            if kwota.get() <= 0 {
                continue;
            }
            // Daniny hurtowe do kolejki miasta (M8a WP2).
            //
            // **Cło** zapisujemy jako fakt, nie jako przelew: pieniądz wyszedł już
            // z konta importera razem z zapłatą za towar (`payable() = net + duty`),
            // więc miasto zabierze go z kanału importowego, a nie drugi raz od firmy.
            //
            // **Akcyza** jest odwrotnie: nikt jej jeszcze nie zapłacił. Nalicza się
            // w chwili, w której wyrób obłożony zmienia właściciela w hurcie, i staje
            // się zobowiązaniem kupującego do najbliższej deklaracji. Dlatego idzie
            // tędy, a nie ceną: podwyżka akcyzy ma podnieść cenę **emergentnie**,
            // przez politykę marżową firmy, a nie przez zadanie jej z zewnątrz (T7).
            let akcyza = m.tax.excise_on(s.good, s.mass);
            if s.duty.get() != 0 || akcyza.get() != 0 {
                let e = m.b2b_outbox.entry(s.deliver_to).or_default();
                e.customs_value = Money(e.customs_value.get() + s.net.get());
                e.duty = Money(e.duty.get() + s.duty.get());
                if akcyza.get() != 0 {
                    e.excise = Money(e.excise.get() + akcyza.get());
                    e.excise_mass = magnat_core::Mass(e.excise_mass.0 + s.mass.0);
                }
            }
            // Kupującym jest sklep **albo zakład produkcyjny** (`AP-2`). Do M6d była
            // to wyłącznie pierwsza możliwość, więc dostawa mąki do piekarni nie miała
            // konta, z którego zapłacić — rozliczenie było po cichu pomijane, towar
            // wjeżdżał do magazynu za darmo i `Store::paid_in` rozjeżdżał się
            // z `Books`. Objaw: `prop_cost_vs_mass` czerwony o wartość każdej
            // dostawy między zakładami.
            let sklep = m.by_site.get(&s.deliver_to).copied().map(|i| i as usize);
            let account = match sklep {
                Some(i) => m.shops[i].account,
                None => match m.plants.get(&s.deliver_to) {
                    Some((_, a)) => *a,
                    None => continue,
                },
            };
            let odbiorca = match s.seller {
                magnat_supply::SellerRef::Firm(f) => m
                    .shops
                    .iter()
                    .find(|sh| sh.firm == f)
                    .map(|sh| sh.account)
                    .or_else(|| m.plants.values().find(|(pf, _)| *pf == f).map(|(_, a)| *a))
                    .unwrap_or(rest),
                magnat_supply::SellerRef::External(_) => rest,
            };
            let kat = m.goods.spec(s.good).map_or(StockCat::Other, |g| g.cat);
            let qty = m.chain.cat.good(s.good).units_of_mass(s.mass);
            let mut memo = wholesale_memo(s.good, qty, kat, 0);
            memo.reason = s.reason;
            // **Przyjęcie księguje się zawsze, zapłata tylko wtedy, gdy jest z czego.**
            // Towar stoi już w magazynie — zlecenie transportowe go tam rozładowało —
            // więc pominięcie przyjęcia rozjechałoby `InventoryGoods` z wyceną zapasu
            // (P5) o wartość każdej dostawy, na którą sklepowi zabrakło. Sklep, który
            // nie zapłacił, ma **zobowiązanie**: `TradePayable` schodzi na minus i to
            // jest właściwa odpowiedź, bo długiem zajmuje się M7, a nie magazyn.
            //
            // Zakład księgi zakładowej nie ma i mieć nie będzie do M7 — dla niego
            // zostaje sam przelew, a rachunek wyniku dopisze faza, która go zaprojektuje.
            if let Some(i) = sklep {
                post_receipt(&mut m.shops[i].ledger, kwota, t);
                if books.transfer(account, odbiorca, kwota, memo, t).is_ok() {
                    post_purchase(&mut m.shops[i].ledger, kwota, t);
                }
            } else {
                let _ = books.transfer(account, odbiorca, kwota, memo, t);
            }
            razem = Money(razem.get() + kwota.get());
        }
        razem
    }
}
