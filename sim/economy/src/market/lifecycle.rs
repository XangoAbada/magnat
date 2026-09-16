//! Cykl życia sklepu: otwarcie, kapitał, zapas startowy, dostawa (szew (a)).

use super::*;

impl Market {
    /// Stawia sklep i obsadza go asortymentem swoich kategorii.
    ///
    /// Zwraca `false`, jeśli archetyp nie jest sklepem w rozumieniu M5
    /// (`data/economy/retail.ron` → `shop_kinds`) albo `SiteId` jest już zajęty.
    pub fn open_shop(&self, seed: ShopSeed, account: AccountId, t: Tick) -> bool {
        let mut m = self.lock();
        if m.by_site.contains_key(&seed.site) {
            return false;
        }
        let cats: Vec<StockCat> = m.data.retail.cats_for(seed.kind).to_vec();
        if cats.is_empty() {
            return false;
        }
        let slots = seed.shelf_slots.max(1);

        // Asortyment: kategorie sklepu na przemian, w każdej towary w kolejności rangi
        // substytutu — dzięki temu mały sklep ma chleb **i** mleko, a nie sam chleb.
        let mut wybor: Vec<u32> = Vec::new();
        let mut rzad = 0usize;
        loop {
            let mut dodano = false;
            for c in &cats {
                if let Some(i) = m.goods.in_cat(*c).get(rzad) {
                    wybor.push(*i);
                    dodano = true;
                }
            }
            rzad += 1;
            if !dodano || wybor.len() >= usize::from(slots) {
                break;
            }
        }
        wybor.truncate(usize::from(slots));
        if wybor.is_empty() {
            return false;
        }

        // Osobowość cenowa jest własnością **firmy**, nie zakładu, i losuje się raz
        // (§5.6). Dwa sklepy tej samej firmy dostaną te same czułości — i tak ma być.
        let osobowosc = FirmPricing::draw(m.seed, seed.firm.entity().index(), &m.data.pricing);

        // Sklep jest od WP11 **zakladem z magazynem** (§6.3 krok 1): zaplecze, polka,
        // rampa i reguly zapasu. Bez `PlantSite` rynek B2B nie mialby dokad dowiezc —
        // `B2b::serve` szuka slotu wejsciowego zakladu, a nie mapy sklepow.
        let (backroom, shelf_slot) = {
            let chain = m.chain.clone();
            let mut ch = chain.lock();
            let poj_m3 = seed.capacity_m3.max(1);
            let backroom = ch.store.add_slot(
                seed.site,
                magnat_supply::WarehouseRole::Backroom,
                magnat_supply::StorageClass::Ambient,
                magnat_core::Mass(poj_m3.saturating_mul(400_000)),
                magnat_core::Volume(poj_m3.saturating_mul(1_000_000)),
                HAZARD_SKLEPU,
            );
            // Polka miesci ulamek zaplecza — to jest ekspozycja, nie magazyn.
            let shelf_slot = ch.store.add_slot(
                seed.site,
                magnat_supply::WarehouseRole::Shelf,
                magnat_supply::StorageClass::Ambient,
                magnat_core::Mass(poj_m3.saturating_mul(80_000)),
                magnat_core::Volume(poj_m3.saturating_mul(200_000)),
                HAZARD_SKLEPU,
            );
            let mut zaklad = magnat_supply::PlantSite::new(
                seed.site,
                seed.firm,
                magnat_supply::Dock::new(1, 10, 2, magnat_core::OpenHours::ALWAYS),
            );
            zaklad.inputs.push(backroom);
            zaklad.outputs.push(shelf_slot);
            ch.plant.insert(zaklad);
            (backroom, shelf_slot)
        };

        let mut shop = Shop {
            site: seed.site,
            firm: seed.firm,
            account,
            pos: seed.pos,
            district: seed.district,
            kind: seed.kind,
            hours: default_hours(seed.kind),
            inventory: ShopInventory {
                capacity_m3: seed.capacity_m3,
                ..ShopInventory::default()
            },
            backroom,
            shelf_slot,
            shelf: Shelf {
                slots,
                lines: Vec::new(),
            },
            assortment: AssortmentPolicy::Auto {
                max_lines: slots,
                min_margin_bp: osobowosc.min_margin_bp,
            },
            tracking: LostSaleTracking::None,
            lost: ShopLostSales::default(),
            customers: ShopCustomers::default(),
            sold_qty: 0,
            revenue: Money::ZERO,
            pricing: osobowosc,
            controllers: BTreeMap::new(),
            observed: CompetitorSnapshot::new(osobowosc.delay_days),
            ledger: Ledger::new(seed.site, seed.firm, t),
            reprice_log: Vec::new(),
            depreciation_monthly: Money::ZERO,
            loan: None,
            opened: t,
        };

        for i in wybor {
            let spec = *m.goods.at(i);
            let offer = m.offers.insert(Offer {
                seller: seed.firm,
                site: seed.site,
                good: spec.good,
                unit_price: spec.retail_price(),
                price_basis: PriceBasis::GrossRetail,
                available: Qty::ZERO,
                quality: spec.quality,
                category: CategoryId::Stock(spec.cat),
                since: t,
                price_rev: 0,
            });
            shop.shelf.insert(ShelfLine {
                good: spec.good,
                facings: 1,
                offer,
            });
            // Każdy towar dostaje własny sterownik ceny. AI zaczyna od polityki
            // dynamicznej — to ona składa cztery korekty z §5.6; gracz podmienia
            // wariant przez `set_policy`, nie przez inną ścieżkę kodu (WP11).
            shop.controllers.insert(
                spec.good,
                PriceController::new(
                    PricePolicy::Dynamic {
                        target_margin_bp: osobowosc.target_margin_bp,
                        floor_margin_bp: osobowosc.min_margin_bp,
                        ceil_margin_bp: osobowosc.max_margin_bp,
                    },
                    spec.retail_price(),
                    t,
                ),
            );
            // Zapas nie może przeżyć własnego terminu ważności (M5c). Sklep, który
            // zamawia osiem wyłożeń chleba o trzydniowym terminie, odpisuje pięć
            // z nich — i to nie jest zła polityka zakupowa, tylko stała z M5b,
            // która do M5c nie miała jak zaboleć, bo nic się nie psuło.
            let krotnosc = if spec.shelf_life_days > 0 {
                BACKROOM_MULTIPLE.min(i64::from(spec.shelf_life_days))
            } else {
                BACKROOM_MULTIPLE
            };
            shop.inventory.reorder.insert(
                spec.good,
                ReorderPolicy {
                    point: Qty(SHELF_UNITS_PER_FACING * REORDER_POINT_MULTIPLE.min(krotnosc)),
                    target: Qty(SHELF_UNITS_PER_FACING * krotnosc),
                    lead_time_days: spec.lead_time_days,
                },
            );
            m.index.mark_dirty(CategoryId::Stock(spec.cat));
        }
        // **Sklep nie dostaje `InventoryRule`** i to jest decyzja, nie przeoczenie.
        // Łańcuch umie sam przeglądać zapasy (`Chain::step_hour`), ale sklep ma już
        // własną politykę zamówień — `docelowy_zapas`, wiążącą cel z obrotem
        // tygodniowym i z terminem ważności, kalibrowaną balansatorem w M5e. Dwie
        // polityki nad jednym magazynem nie są nadmiarem, tylko **sprzecznością**:
        // obie zamawiają, żadna nie widzi zamówień drugiej i zaplecze rośnie do sumy
        // obu celów. Reguły łańcucha zostają dla zakładów produkcyjnych, gdzie polityki
        // detalicznej nie ma (`AL-5`).
        // Wyposażenie lokalu: wkład właściciela, więc druga strona to `Equity`,
        // a nie przelew. Amortyzacja liniowa schodzi z niego co miesiąc (§5.8).
        let (wartosc, odpis) = m.data.costs.equipment(slots);
        shop.depreciation_monthly = odpis;
        if wartosc.get() > 0 {
            let _ = ledger::post(
                &mut shop.ledger,
                JournalEntry::new(
                    t,
                    DecisionReason::Unspecified,
                    &[
                        (LedgerAccount::FixedAssets, wartosc),
                        (LedgerAccount::Equity, Money(-wartosc.get())),
                    ],
                ),
            );
        }
        let idx = u32::try_from(m.shops.len()).expect("za dużo sklepów");
        m.by_site.insert(seed.site, idx);
        m.shops.push(shop);
        true
    }

    /// Kapitał obrotowy wniesiony na rachunek sklepu — druga strona przelewu,
    /// którego `Books` już dokonały.
    ///
    /// Osobne wywołanie, bo `open_shop` nie widzi `Books`: pieniądz ma jedno wejście
    /// (`Books::transfer`) i to wołający nim dysponuje. Bez tego wywołania
    /// `BankCurrent` w księdze rozjedzie się z saldem konta, a to jest pierwsza
    /// rzecz, którą sprawdza test WP7.
    pub fn record_capital(&self, site: SiteId, amount: Money, t: Tick) -> bool {
        if amount.get() == 0 {
            return false;
        }
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        ledger::post(
            &mut m.shops[i as usize].ledger,
            JournalEntry::new(
                t,
                DecisionReason::Unspecified,
                &[
                    (LedgerAccount::BankCurrent, amount),
                    (LedgerAccount::Equity, Money(-amount.get())),
                ],
            ),
        )
        .is_ok()
    }

    /// Zatowarowanie startowe: sklep postawiony przez generator ma towar w dniu 0.
    ///
    /// Pusty sklep w pierwszej dobie nie jest stanem gospodarki, tylko stanem
    /// symulacji — mieszkańcy czekaliby na pierwszą dostawę tyle, ile trwa
    /// `lead_time_days`, i przez ten czas nie kupiliby nic. Towar jest **kupiony**,
    /// a nie wyczarowany: przelew idzie z konta sklepu na `RestOfWorld`.
    pub fn stock_initial(&self, books: &mut Books, t: Tick) {
        {
            let mut m = self.lock();
            let rest = m.rest_of_world;
            let chain = m.chain.clone();
            let cat = chain.cat.clone();
            for i in 0..m.shops.len() {
                // **Zapas startowy to wylozenie polki, nie pelne zaplecze.** Cel polityki
                // jest wielokrotnoscia wylozenia, a przy towarze o trzydniowym terminie
                // ta wielokrotnosc to trzy doby zapasu, ktorych w dniu zerowym **nikt
                // jeszcze nie kupuje** — wiec schodzily w calosci na odpis.
                let plan: Vec<(GoodId, Qty)> = m.shops[i]
                    .inventory
                    .reorder
                    .iter()
                    .map(|(g, p)| (*g, Qty(p.target.get().min(SHELF_UNITS_PER_FACING))))
                    .collect();
                let (site, account, backroom, firm) = (
                    m.shops[i].site,
                    m.shops[i].account,
                    m.shops[i].backroom,
                    m.shops[i].firm,
                );
                for (good, target) in plan {
                    // Zapas startowy jest **kupiony**, a nie wyczarowany: przelew idzie
                    // z konta sklepu na `RestOfWorld`. To jedyne miejsce, w ktorym towar
                    // detaliczny wchodzi do magazynu bez dostawcy, i jest nim dokladnie
                    // ten sam wyjatek, ktory dopuszcza zapas startowy swiata: bez niego
                    // pierwszy klient trafilby na pustke, bo pierwszy mlyn zaczyna miec
                    // dopiero w minucie zero.
                    let Some(q) = m.supplier.quote(good, target, site, t) else {
                        continue;
                    };
                    let kat = m.goods.spec(good).map_or(StockCat::Other, |s| s.cat);
                    if books
                        .transfer(
                            account,
                            rest,
                            q.total(),
                            wholesale_memo(good, q.qty, kat, 0),
                            t,
                        )
                        .is_err()
                    {
                        continue;
                    }
                    let draft = magnat_supply::BatchDraft {
                        good,
                        mass: cat.good(good).mass_of_units(q.qty),
                        quality: q.quality,
                        brand: None,
                        producer: firm,
                        produced_at: SimMinute(t.get()),
                        cost: q.total(),
                        origin: magnat_supply::BatchOrigin::imported(),
                        flags: Default::default(),
                    };
                    if chain
                        .lock()
                        .store
                        .put(&cat, backroom, draft, magnat_supply::MassIn::Initial)
                        .is_err()
                    {
                        continue;
                    }
                    post_purchase(&mut m.shops[i].ledger, q.total(), t);
                    post_receipt(&mut m.shops[i].ledger, q.total(), t);
                }
            }
        }
        self.restock_shelves();
    }

    /// Wstawia towar na zaplecze **z pominięciem dostawcy**.
    ///
    /// Używane przez scenariusze i testy, które chcą sklep w zadanym stanie, oraz
    /// przez most z M6, kiedy towar przyjeżdża z zakładu, a nie z importu.
    /// `paid` to koszt nabycia wchodzący do wyceny zapasu — wołający odpowiada za to,
    /// żeby ta kwota **została naprawdę zapłacona**, inaczej wartość zapasu w bilansie
    /// nie będzie miała pokrycia (P5, M5c).
    pub fn deliver_now(
        &self,
        site: SiteId,
        good: GoodId,
        qty: Qty,
        paid: Money,
        expires: Option<SimMinute>,
        t: Tick,
    ) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        let backroom = m.shops[i as usize].backroom;
        let firm = m.shops[i as usize].firm;
        let chain = m.chain.clone();
        let cat = chain.cat.clone();
        // Data przydatności jest od WP11 **własnością partii** i liczy się jako
        // `produced_at + shelf_life` z katalogu. Wołający, który podaje termin wprost,
        // dostaje więc partię **odpowiednio starą** — to jest to samo zdanie wyrażone
        // drugą stroną równania i jedyny sposób, żeby obie drogi dawały ten sam wynik.
        let produced_at = match (expires, cat.good(good).shelf_life_minutes) {
            (Some(e), Some(zycie)) => SimMinute(e.get().saturating_sub(u64::from(zycie))),
            _ => SimMinute(t.get()),
        };
        let draft = magnat_supply::BatchDraft {
            good,
            mass: cat.good(good).mass_of_units(qty),
            quality: magnat_core::Q::new(60),
            brand: None,
            producer: firm,
            produced_at,
            cost: paid,
            origin: magnat_supply::BatchOrigin::imported(),
            flags: Default::default(),
        };
        if chain
            .lock()
            .store
            .put(&cat, backroom, draft, magnat_supply::MassIn::Initial)
            .is_err()
        {
            return false;
        }
        post_receipt(&mut m.shops[i as usize].ledger, paid, t);
        true
    }
}

/// Maska klas niebezpieczenstwa, na ktore sklep ma zgode.
///
/// Zero: sklep osiedlowy nie trzyma benzyny ani chemii przemyslowej, a slot bez maski
/// **odmawia przyjecia** takiej partii (`Store::put`). Stacja paliw dostanie wlasna
/// maske razem z wlasnym archetypem — to jest decyzja M7, nie M6.
const HAZARD_SKLEPU: u8 = 0;
