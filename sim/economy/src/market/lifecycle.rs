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
                // Towar poza obiegiem nie trafia na półkę nowego sklepu — inaczej
                // pierwszy telefon komórkowy stałby w witrynie od pierwszej doby
                // partii, a cała technologia byłaby ozdobą (M10c WP10.9).
                if let Some(i) = m
                    .goods
                    .in_cat(*c)
                    .iter()
                    .filter(|i| !m.locked_goods.contains(&m.goods.at(**i).good))
                    .nth(rzad)
                {
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
            accrued: crate::shop::TaxAccrual::default(),
            pricing: osobowosc,
            controllers: BTreeMap::new(),
            observed: CompetitorSnapshot::new(osobowosc.delay_days),
            // Sklep bez przypiętej polityki nie pyta o żaden dodatkowy promień.
            asked_radii: [0; crate::pricing::MAX_NEAR_RADII],
            ledger: Ledger::new(seed.site, seed.firm, t),
            reprice_log: Vec::new(),
            trace: crate::policy_run::PolicyTrace::default(),
            depreciation_monthly: Money::ZERO,
            loan: None,
            opened: t,
            closed: false,
            unreported_bps: 0,
            suspended_until: magnat_core::Tick(0),
            expired_mass: magnat_core::Mass::ZERO,
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
                // Marka wchodzi dopiero z pierwszą partią na półce (`restock`):
                // pusta półka nie ma producenta, więc nie ma czyjej marki nosić.
                brand: None,
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

impl Market {
    /// Zamyka zakład handlowy: półka schodzi, oferty znikają z areny, koszty stałe
    /// przestają się naliczać (M7e WP12, WP15).
    ///
    /// Odwrotność [`Market::open_shop`] i dlatego stoi obok niej. **Rekord sklepu
    /// zostaje** razem z księgą: historia wyniku jest tym, z czego panel tłumaczy
    /// graczowi, dlaczego zakład padł, a usunięcie go z wektora przesunęłoby indeksy
    /// wszystkich pozostałych — czyli `by_site` każdego innego sklepu w mieście.
    ///
    /// Zwraca `false`, gdy takiego zakładu nie ma albo jest już zamknięty; zamknięcie
    /// jest **idempotentne**, bo decyzja taktyczna i upadłość mogą trafić w ten sam
    /// zakład w tej samej dobie.
    pub fn close_shop(&self, site: SiteId) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied().map(|i| i as usize) else {
            return false;
        };
        if m.shops[i].closed {
            return false;
        }
        let linie = std::mem::take(&mut m.shops[i].shelf.lines);
        for l in linie {
            if let Some(cat) = m.goods.spec(l.good).map(|s| CategoryId::Stock(s.cat)) {
                m.index.mark_dirty(cat);
            }
            m.offers.remove(l.offer);
        }
        m.shops[i].inventory.reorder.clear();
        m.shops[i].assortment = AssortmentPolicy::Manual { goods: Vec::new() };
        m.shops[i].closed = true;
        true
    }

    /// Ustawia asortyment półki: dokłada brakujące linie, zdejmuje te spoza listy.
    ///
    /// **To jest wykonawca [`AssortmentPolicy::Manual`]**, którego ten wariant do tej
    /// pory nie miał — pole istniało od M5b i nikt go nie czytał, czyli było dokładnie
    /// tym, przed czym broni `K-67`. Komenda gracza `SetShelfAssortment` wchodzi tędy.
    ///
    /// Linia zdjęta z półki **nie zabiera towaru z zaplecza**: zapas leży w magazynie
    /// M6 i zostaje tam, gdzie leżał. Znika ekspozycja i oferta, czyli to, co widzi
    /// kupujący — a to jest cała treść słowa „asortyment".
    ///
    /// Zwraca liczbę linii po zmianie; `None`, gdy rynek nie zna tego zakładu.
    /// Lista dłuższa od półki jest **przycinana**, a nie odrzucana: gracz wybiera
    /// z listy, a ile się zmieści, wie półka.
    pub fn set_assortment(&self, site: SiteId, goods: &[GoodId], t: Tick) -> Option<u16> {
        let mut m = self.lock();
        let i = m.by_site.get(&site).copied()? as usize;
        if m.shops[i].closed {
            return None;
        }
        let slots = usize::from(m.shops[i].shelf.slots);
        let mut chciane: Vec<GoodId> = Vec::new();
        for g in goods {
            if !chciane.contains(g) && m.goods.spec(*g).is_some() && !m.locked_goods.contains(g) {
                chciane.push(*g);
            }
        }
        chciane.truncate(slots);

        // Zdjęcie: linia, oferta, sterownik i polityka zamówień schodzą razem.
        // Zostawienie któregokolwiek z nich znaczyłoby sklep, który dalej zamawia
        // towar, którego nie sprzedaje.
        let do_zdjecia: Vec<GoodId> = m.shops[i]
            .shelf
            .lines
            .iter()
            .map(|l| l.good)
            .filter(|g| !chciane.contains(g))
            .collect();
        for g in do_zdjecia {
            m.zdejmij_linie(i, g);
        }

        // Dołożenie: nowa linia startuje z ceny katalogowej i polityki stałej.
        // Stałej, a nie dynamicznej, z tego samego powodu, dla którego `SetPrice`
        // przestawia sterownik na `Fixed`: gracz właśnie wybrał ten towar ręcznie
        // i pierwsza przecena nie ma prawa go zaskoczyć.
        for g in chciane {
            dodaj_linie(&mut m, i, site, g, t);
        }
        let linie = m.shops[i].shelf.lines.iter().map(|l| l.good).collect();
        m.shops[i].assortment = AssortmentPolicy::Manual { goods: linie };
        u16::try_from(m.shops[i].shelf.lines.len()).ok()
    }

    /// Wyjmuje towar z obiegu: nie stanie na żadnej półce i nikt go nie zamówi.
    ///
    /// Wolno **tylko przy stawianiu świata** — towar zdjęty z półek w trakcie partii
    /// byłby zniknięciem, a nie blokadą, i zostawiłby po sobie oferty bez towaru.
    /// Listę podaje drzewo technologii (`magnat_firms::gated_goods`).
    pub fn lock_good(&self, good: GoodId) {
        self.lock().locked_goods.insert(good);
    }

    /// Czy towar jest poza obiegiem.
    #[must_use]
    pub fn is_good_locked(&self, good: GoodId) -> bool {
        self.lock().is_locked(good)
    }

    /// Wstawia świeżo odblokowany towar na półki sklepów jego kategorii (M10c WP10.9).
    ///
    /// Zwraca liczbę sklepów, które go przyjęły. Dostają go **wyłącznie sklepy
    /// prowadzone przez AI** ([`AssortmentPolicy::Auto`]) i **wyłącznie te, którym
    /// została wolna półka**: sklep, którego asortyment gracz ustawił ręcznie, nie ma
    /// prawa dostać nowego towaru bez jego wiedzy, a półka bez miejsca musiałaby coś
    /// z niej zdjąć — a to już jest decyzja, nie skutek odkrycia.
    ///
    /// Kolejność jest kolejnością `SiteId`, więc wynik nie zależy od tego, w jakiej
    /// kolejności sklepy powstały (00 §3.2).
    pub fn stock_new_good(&self, good: GoodId, t: Tick) -> u16 {
        let mut m = self.lock();
        let Some(spec) = m.goods.spec(good).copied() else {
            return 0;
        };
        // Odblokowanie jest zdarzeniem **raz**: ten sam węzeł odkryty przez drugą
        // firmę nie wypuszcza towaru drugi raz i nie odnawia mu nowości.
        if !m.locked_goods.remove(&good) {
            return 0;
        }
        // Nowość trwa rok gry (kalendarz `K-1`: 360 dób). Tyle, a nie miesiąc, bo
        // telefon kupuje się raz na kilka lat i miesięczne okno minęłoby, zanim
        // pierwszy mieszkaniec w ogóle wyszedłby po niego do sklepu.
        m.fresh_goods
            .insert(good, Tick(t.0.saturating_add(360 * 1_440)));
        let sklepy: Vec<usize> = m
            .by_site
            .values()
            .map(|i| *i as usize)
            .filter(|i| {
                let s = &m.shops[*i];
                !s.closed
                    && matches!(s.assortment, AssortmentPolicy::Auto { .. })
                    && m.data.retail.cats_for(s.kind).contains(&spec.cat)
                    && s.shelf.lines.len() < usize::from(s.shelf.slots)
            })
            .collect();
        let mut ile = 0u16;
        for i in sklepy {
            let site = m.shops[i].site;
            if dodaj_linie(&mut m, i, site, good, t) {
                ile = ile.saturating_add(1);
            }
        }
        ile
    }

    /// Czy zakład handlowy jest zamknięty. `false` także dla zakładu, którego rynek
    /// w ogóle nie zna — pytanie brzmi „czy przestał sprzedawać", a zakład
    /// produkcyjny nigdy nie zaczął.
    #[must_use]
    pub fn is_shop_closed(&self, site: SiteId) -> bool {
        let m = self.lock();
        m.by_site
            .get(&site)
            .copied()
            .is_some_and(|i| m.shops[i as usize].closed)
    }
}

/// Stawia jedną linię na półce zakładu: ofertę, sterownik ceny i regułę zamówień.
///
/// Wydzielone przy M10c, bo to samo trzeba zrobić w trzech miejscach: przy otwarciu
/// sklepu, przy ręcznej zmianie asortymentu i przy wejściu **nowego towaru** na rynek
/// ([`Market::stock_new_good`]). Trzy kopie tej wiedzy rozjechałyby się przy pierwszej
/// zmianie polityki zamówień — a to jest jedna reguła domenowa, nie trzy podobne pętle.
fn dodaj_linie(m: &mut MarketInner, i: usize, site: SiteId, g: GoodId, t: Tick) -> bool {
    if m.shops[i].shelf.line(g).is_some() {
        return false;
    }
    let Some(spec) = m.goods.spec(g).copied() else {
        return false;
    };
    let firma = m.shops[i].firm;
    let offer = m.offers.insert(Offer {
        seller: firma,
        site,
        good: spec.good,
        unit_price: spec.retail_price(),
        price_basis: PriceBasis::GrossRetail,
        available: Qty::ZERO,
        quality: spec.quality,
        // Marka wchodzi dopiero z pierwszą partią na półce (`restock`):
        // pusta półka nie ma producenta, więc nie ma czyjej marki nosić.
        brand: None,
        category: CategoryId::Stock(spec.cat),
        since: t,
        price_rev: 0,
    });
    if !m.shops[i].shelf.insert(ShelfLine {
        good: spec.good,
        facings: 1,
        offer,
    }) {
        m.offers.remove(offer);
        return false;
    }
    m.shops[i].controllers.insert(
        spec.good,
        PriceController::new(
            PricePolicy::Fixed {
                price: spec.retail_price(),
            },
            spec.retail_price(),
            t,
        ),
    );
    let krotnosc = if spec.shelf_life_days > 0 {
        BACKROOM_MULTIPLE.min(i64::from(spec.shelf_life_days))
    } else {
        BACKROOM_MULTIPLE
    };
    m.shops[i].inventory.reorder.insert(
        spec.good,
        ReorderPolicy {
            point: Qty(SHELF_UNITS_PER_FACING * REORDER_POINT_MULTIPLE.min(krotnosc)),
            target: Qty(SHELF_UNITS_PER_FACING * krotnosc),
            lead_time_days: spec.lead_time_days,
        },
    );
    m.index.mark_dirty(CategoryId::Stock(spec.cat));
    true
}

impl MarketInner {
    /// Zdejmuje jedną linię z półki: linia, oferta, sterownik ceny i punkt zamówienia
    /// schodzą **razem**.
    ///
    /// Zostawienie któregokolwiek z nich znaczyłoby sklep, który dalej zamawia towar,
    /// którego nie sprzedaje, albo ofertę w arenie bez linii, która ją niosła.
    ///
    /// Jedna reguła, dwóch wołających: ręczny asortyment gracza
    /// ([`Market::set_assortment`]) i akcja `RemoveFromShelf` w polityce zakładu.
    /// Do R2e ta druga zwracała `PolicyOutcome::Blind` — przechodziła walidator,
    /// wykonywała się i **nie robiła nic**, a gracz wybierał ją z listy w edytorze
    /// reguł (`R2-WP22`, poz. 53). Druga kopia tej pętli rozjechałaby się z pierwszą
    /// przy pierwszym nowym polu linii półki.
    ///
    /// Zapas **zostaje na zapleczu**: znika ekspozycja i oferta, czyli to, co widzi
    /// kupujący. To jest ta sama zasada, którą `set_assortment` ma w swojej
    /// dokumentacji, i to jest cała treść słowa „asortyment".
    ///
    /// Zwraca `false`, gdy tego towaru na półce nie było — akcja jest wtedy ślepa,
    /// a nie wykonana, i polityka ma to zobaczyć.
    pub(crate) fn zdejmij_linie(&mut self, shop: usize, good: GoodId) -> bool {
        let Some(pos) = self.shops[shop]
            .shelf
            .lines
            .iter()
            .position(|l| l.good == good)
        else {
            return false;
        };
        let linia = self.shops[shop].shelf.lines.remove(pos);
        self.offers.remove(linia.offer);
        self.shops[shop].controllers.remove(&good);
        self.shops[shop].inventory.reorder.remove(&good);
        if let Some(cat) = self.goods.spec(good).map(|s| CategoryId::Stock(s.cat)) {
            self.index.mark_dirty(cat);
        }
        true
    }
}
