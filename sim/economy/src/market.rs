//! Rynek detaliczny: sklepy, decyzja zakupowa i rozliczenie (M5b §5.3–§5.5).
//!
//! # Dlaczego stan rynku siedzi w `Market`, a nie w zasobach świata
//!
//! Punktem podmiany, przez który M5 wchodzi do symulacji, jest
//! `sim::agents::Sources.places: Box<dyn PlaceProvider>` (`Z-1` w dokumencie fazy) —
//! i to on wymusza kształt. `PlaceProvider::candidates` i `fulfil` dostają `&self`
//! oraz `&mut self`, **ale nie dostają `&World`**: zasób `AgentSources` jest na czas
//! minuty wyjmowany z ECS, żeby pętla doby mogła trzymać `&mut World` obok. Wszystko,
//! czego dotyka decyzja zakupowa — arena ofert, indeks, półki, magazyny, katalog
//! towarów — musi więc być osiągalne **z samego providera**.
//!
//! Stąd `Market` jest `Arc<Mutex<MarketInner>>`: jedna kopia siedzi w świecie jako
//! zasób (i przez to w hashu stanu), druga jest zapakowana w `Box<dyn PlaceProvider>`.
//! To ten sam wzorzec, którym M4 wstawił `TrafficOracle` (`OracleHandle(Arc<…>)`),
//! z tego samego powodu i z tym samym kosztem: jeden zamek, bez rywalizacji, bo oba
//! systemy, które tu wchodzą, są **wyłączne** (`K-21`).
//!
//! # Gdzie zapada decyzja, a gdzie się ją wykonuje
//!
//! Plan §5.4/§5.5 dzielił pracę na fazę decyzji i fazę rozliczenia. Podział zostaje,
//! ale granice biegną tam, gdzie leżą dane:
//!
//! 1. **`candidates` (planer, plan dnia)** — zna kupującego, więc tu liczy się pełna
//!    funkcja użyteczności §6.4 (wagi z osobowości i statusu), wstępny ranking
//!    całkowitoliczbowy, softmax i **zapamiętanie decyzji** w [`PlannedPurchase`].
//! 2. **`fulfil` (wizyta w sklepie)** — nie zna kupującego, ale ma zapamiętaną
//!    decyzję, więc sprawdza to, co mogło się zmienić przez te godziny: stan półki,
//!    cenę (poślizg) i budżet. Tu też zapada odłożenie zakupu, i **to jest miejsce
//!    właściwe**: odmowa wraca przez `ReplanCause::PlaceRefused`, czyli przez ścieżkę,
//!    którą M3 nazywa „wróć do zadania później" (PRD §6.4 punkt 2).
//! 3. **`settle_transactions` (system, po pętli doby)** — pieniądz. Osobno, bo saldo
//!    gospodarstwa mieszka w komponencie `Household` (własność M3), a `fulfil` nie
//!    widzi świata.
//!
//! Wyścig o ostatnią sztukę rozstrzyga się **konstrukcyjnie w `fulfil`**: pętla doby
//! jest systemem wyłącznym, więc faza decyzji jest już sekwencyjna i deterministyczna,
//! a półka schodzi w tej samej chwili, w której zapada decyzja. Żadnych blokad,
//! żadnej zależności od kolejności ukończenia jobów (00 §3.3).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use magnat_agents::{
    default_hours, walk_minutes, ArrayVec, CitizenView, FulfilOutcome, FulfilRequest,
    InfinitePlaces, KnowledgeView, NeedTable, OpenHours, PlaceCandidate, PlaceProvider, PlaceTable,
    MAX_CANDIDATES,
};
use magnat_core::{
    Arena, CitizenId, DecisionReason, FirmId, GoodId, HashState, HouseholdId, Money, NeedKind,
    PlaceKind, PlaceRef, Qty, RejectCause, SimMinute, SiteId, StateHasher, StockCat, Tick,
    TraitId, WorldCoord, Q, STOCK_CAT_COUNT,
};
use magnat_jobs::JobPool;
use magnat_spatial::{GridSpec, Vec2};

use crate::books::{AccountId, Books, SupplierRef, TxKind, TxMemo};
use crate::choice::{
    budget_ref_for, choose_offer, days_bought, dominant_term, purchase_threshold, rating_of,
    offer_noise, utility_of_offer, wanted_qty, weights_for, BuyerState, Candidate,
};
use crate::data::{EconomyData, UtilityWeights};
use crate::offer::{query_offers, CategoryId, Offer, OfferId, OfferIndex, PriceBasis};
use crate::shop::{
    AssortmentPolicy, LostSale, LostSaleHistogram, LostSaleTracking, ReorderPolicy, Shelf,
    ShelfLine, Shop, ShopInventory, ShopLostSales,
};
use crate::supply::{line_total, ExternalSupplier, GoodTable, Wholesale, PRICE_UNIT};
use crate::kernel::BP;
use crate::ledger::{self, JournalEntry, Ledger, LedgerAccount};
use crate::pricing::{
    preview_price, reprice, CompetitorEntry, CompetitorRef, CompetitorSnapshot, FirmPricing,
    PriceController, PricePolicy, PricingCtx,
};
use crate::tax::{NoTax, TaxEngine};

/// Ile jednostek towaru mieści jedno miejsce na półce, ile razy tyle leży na zapleczu
/// i przy jakim stanie sklep zamawia.
///
/// `ponytail:` trzy stałe zamiast modelu pojemności z metrów sześciennych. Sufit
/// nazwany: półka nie zależy od gęstości towaru, więc kilogram mąki zajmuje tyle
/// co kilogram pierza. Ścieżka wyjścia jest gotowa w danych
/// (`Good.density_g_per_l` z `data/goods/`) i wchodzi razem z `capacity_m3`, kiedy
/// M6 zacznie wozić partie o realnej objętości.
const SHELF_UNITS_PER_FACING: i64 = 250 * PRICE_UNIT;
const BACKROOM_MULTIPLE: i64 = 8;
const REORDER_POINT_MULTIPLE: i64 = 3;

/// Sufit liczby towarów rozważanych w jednej wizycie — tyle, ile mieści największa
/// półka. Ponad to substytut niższego rzędu przestaje być substytutem.
const MAX_LINES: usize = 48;

/// Zaklepana transakcja: półka już zdjęta, pieniądz jeszcze nie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PurchaseIntent {
    pub buyer: CitizenId,
    pub household: HouseholdId,
    pub site: SiteId,
    pub offer: OfferId,
    pub good: GoodId,
    pub cat: StockCat,
    pub qty: Qty,
    /// Ile dni zapasu kategorii dokłada ten zakup.
    pub days: u8,
    /// Cena z momentu decyzji (`K-7`: kwota, którą płaci kupujący).
    pub agreed_price: Money,
    /// Koszt nabycia zdjętego towaru — zdjęty z półki **razem ze sztukami**, więc
    /// rozliczenie ma czym zaksięgować `Cogs`, a nieudane rozliczenie ma co oddać.
    /// Bez tego pola koszt przepadałby przy zwrocie i `InventoryGoods` rozjeżdżałby
    /// się z zapasem (P5) przy każdym nieudanym przelewie.
    pub cogs: Money,
    pub arrived: Tick,
    pub reason: DecisionReason,
}

/// Decyzja zapamiętana przy planowaniu dnia i odczytana przy wizycie.
///
/// Trzyma wyłącznie to, czego `fulfil` nie ma jak odtworzyć: wagi wyprowadzone
/// z osobowości, stan kupującego i próg policzony przy **tamtym** poziomie potrzeby.
/// Wszystko `Copy` i małe — jeden wpis na mieszkańca, nadpisywany przy każdym planie.
#[derive(Clone, Copy, PartialEq, Debug)]
struct PlannedPurchase {
    site: SiteId,
    need: NeedKind,
    weights: UtilityWeights,
    status: Q,
    openness: Q,
    vot_gr_per_min: i64,
    threshold: f64,
    /// Cena jednostkowa widziana w chwili decyzji — podstawa poślizgu (§5.5).
    unit_price: Money,
    good: GoodId,
}

/// Liczniki rynku — wejście metryk balansatora (M5e) i raportu scenariusza.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct MarketStats {
    pub purchases: u64,
    pub purchased_qty: i64,
    pub revenue: Money,
    pub stockouts: u64,
    pub deferrals: u64,
    pub budget_refusals: u64,
    pub no_candidates: u64,
    pub deliveries: u64,
    pub restocks: u64,
    /// Ile razy cena zmieniła się między decyzją a wizytą ponad `price_slippage_bp`.
    pub slippage_rechecks: u64,
    // ── M5c ──
    /// Ile ofert zmieniło cenę (dobowy przelot polityk).
    pub reprices: u64,
    /// Ile sklepów odświeżyło obraz konkurencji.
    pub observations: u64,
    /// Odpisy towaru przeterminowanego.
    pub write_offs: u64,
    pub write_off_value: Money,
    pub expired_qty: i64,
}

/// Migawka gospodarstwa, której `PlaceProvider` nie ma jak odczytać ze świata.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct HouseholdSnapshot {
    size: u8,
    stock: [u8; STOCK_CAT_COUNT],
}

impl Default for HouseholdSnapshot {
    fn default() -> HouseholdSnapshot {
        HouseholdSnapshot {
            size: 1,
            stock: [0; STOCK_CAT_COUNT],
        }
    }
}

/// Opis sklepu do postawienia. Wypełnia go most z `sim/world` (zakłady Etapu 7).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShopSeed {
    pub site: SiteId,
    pub firm: FirmId,
    /// Pozycja w metrach — ze środka budynku, w którym stoi zakład.
    pub pos: Vec2,
    pub kind: PlaceKind,
    /// Ile linii mieści półka — z powierzchni lokalu (§7.3).
    pub shelf_slots: u16,
    pub capacity_m3: i64,
}

struct MarketInner {
    offers: Arena<Offer>,
    index: OfferIndex,
    /// Sklepy w kolejności zakładania; `by_site` daje dostęp po `SiteId`.
    shops: Vec<Shop>,
    by_site: BTreeMap<SiteId, u32>,
    supplier: ExternalSupplier,
    data: EconomyData,
    needs: Arc<NeedTable>,
    places: Arc<PlaceTable>,
    /// Delegat dla miejsc, które nie są sklepem: dom, jadłodajnia, lekarz.
    /// M5b nie odbiera M3 niczego, co M3 już umiał (§2 dokumentu fazy).
    fallback: InfinitePlaces,
    households: Vec<HouseholdSnapshot>,
    planned: BTreeMap<u32, PlannedPurchase>,
    intents: Vec<PurchaseIntent>,
    /// Kwoty zaklepane w bieżącym ticku, po indeksie encji gospodarstwa. Bez tego
    /// dwa zakupy tej samej minuty widziałyby ten sam budżet dwa razy.
    committed: BTreeMap<u32, Money>,
    rest_of_world: AccountId,
    /// Hak podatkowy (`K-7`). W M5 `NoTax`; M8 wstawia `CityTaxEngine`.
    tax: Box<dyn TaxEngine>,
    seed: u64,
    tick: Tick,
    stats: MarketStats,
    // bufory wielokrotnego użytku — gorąca ścieżka nie zaczyna się od alokacji (§7.3)
    offer_buf: Vec<OfferId>,
    cand_buf: Vec<Candidate>,
    util_buf: Vec<f64>,
    order_buf: Vec<usize>,
    deliv_buf: Vec<crate::supply::Delivery>,
    /// Bufory obserwacji konkurencji — dobowy przelot, nie wolno mu alokować.
    obs_buf: Vec<(GoodId, Money, u32, SiteId)>,
    entry_buf: Vec<CompetitorEntry>,
}

/// Rynek detaliczny: stan współdzielony między zasobem świata a `PlaceProvider`em.
#[derive(Clone)]
pub struct Market(Arc<Mutex<MarketInner>>);

impl Market {
    /// Buduje pusty rynek. Sklepy dokłada [`Market::open_shop`].
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        grid: GridSpec,
        seed: u64,
        data: EconomyData,
        goods: GoodTable,
        needs: Arc<NeedTable>,
        places: Arc<PlaceTable>,
        rest_of_world: AccountId,
    ) -> Market {
        Market(Arc::new(Mutex::new(MarketInner {
            offers: Arena::new(),
            index: OfferIndex::new(grid),
            shops: Vec::new(),
            by_site: BTreeMap::new(),
            supplier: ExternalSupplier::new(seed, goods),
            data,
            needs: needs.clone(),
            places: places.clone(),
            fallback: InfinitePlaces::new(places, needs),
            households: Vec::new(),
            planned: BTreeMap::new(),
            intents: Vec::new(),
            committed: BTreeMap::new(),
            rest_of_world,
            tax: Box::new(NoTax),
            seed,
            tick: Tick(0),
            stats: MarketStats::default(),
            offer_buf: Vec::new(),
            cand_buf: Vec::new(),
            util_buf: Vec::new(),
            order_buf: Vec::new(),
            deliv_buf: Vec::new(),
            obs_buf: Vec::new(),
            entry_buf: Vec::new(),
        })))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MarketInner> {
        // Zatrucie zamka znaczy panikę w systemie symulacji — wtedy świat i tak jest
        // do wyrzucenia, więc rozwijamy ją dalej zamiast liczyć na stanie sprzed paniki.
        self.0.lock().expect("Market: zatruty zamek")
    }

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
        let back: i64 = s.inventory.backroom.values().map(|l| l.cost_total.get()).sum();
        let shelf: i64 = s.shelf.lines.iter().map(|l| l.cost_total.get()).sum();
        Money(back + shelf)
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
        m.by_site.get(&site).map(|i| m.shops[*i as usize].lost.today)
    }

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
                if let Some(i) = m.supplier.goods().in_cat(*c).get(rzad) {
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
        let mut shop = Shop {
            site: seed.site,
            firm: seed.firm,
            account,
            pos: seed.pos,
            kind: seed.kind,
            hours: default_hours(seed.kind),
            inventory: ShopInventory {
                capacity_m3: seed.capacity_m3,
                ..ShopInventory::default()
            },
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
            sold_qty: 0,
            revenue: Money::ZERO,
            pricing: osobowosc,
            controllers: BTreeMap::new(),
            observed: CompetitorSnapshot::new(osobowosc.delay_days),
            ledger: Ledger::new(seed.site, seed.firm, t),
            reprice_log: Vec::new(),
            depreciation_monthly: Money::ZERO,
        };

        for i in wybor {
            let spec = *m.supplier.goods().at(i);
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
                qty: Qty::ZERO,
                cost_total: Money::ZERO,
                facings: 1,
                offer,
                expires: None,
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
            for i in 0..m.shops.len() {
                let plan: Vec<(GoodId, Qty)> = m.shops[i]
                    .inventory
                    .reorder
                    .iter()
                    .map(|(g, p)| (*g, p.target))
                    .collect();
                let (site, account) = (m.shops[i].site, m.shops[i].account);
                for (good, target) in plan {
                    let Some(q) = m.supplier.quote(good, target, site, t) else {
                        continue;
                    };
                    if books
                        .transfer(account, rest, q.total(), wholesale_memo(good, q.qty), t)
                        .is_err()
                    {
                        continue;
                    }
                    let expires = q
                        .shelf_life
                        .map(|s| SimMinute(t.get().saturating_add(s.get())));
                    m.shops[i]
                        .inventory
                        .backroom
                        .entry(good)
                        .or_default()
                        .receive(q.qty, q.total(), expires);
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
        m.shops[i as usize]
            .inventory
            .backroom
            .entry(good)
            .or_default()
            .receive(qty, paid, expires);
        post_receipt(&mut m.shops[i as usize].ledger, paid, t);
        true
    }

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
                    (have < p.point.get()).then(|| (*g, Qty(p.target.get() - have)))
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
                    .transfer(account, rest, q.total(), wholesale_memo(good, q.qty), t)
                    .is_err()
                {
                    continue;
                }
                post_purchase(&mut m.shops[i].ledger, q.total(), t);
                let _ = m.supplier.place_order(&q, firm, site, t);
            }
        }
    }

    /// Przebudowa indeksu ofert, jeśli któraś warstwa jest brudna.
    pub fn rebuild_index(&self, pool: &JobPool) {
        let mut m = self.lock();
        if !m.index.is_dirty() {
            return;
        }
        let pozycje: BTreeMap<SiteId, Vec2> = m.shops.iter().map(|s| (s.site, s.pos)).collect();
        let MarketInner { index, offers, .. } = &mut *m;
        index.rebuild(
            offers,
            |s| pozycje.get(&s).copied().unwrap_or(Vec2::ZERO),
            pool,
        );
    }

    /// Migawka gospodarstw dla decyzji zakupowej: liczebność i zapas w dniach.
    pub fn refresh_households(&self, entries: &[(u32, u8, [u8; STOCK_CAT_COUNT])]) {
        let mut m = self.lock();
        let max = entries.iter().map(|e| e.0).max().unwrap_or(0) as usize;
        if m.households.len() <= max {
            m.households.resize(max + 1, HouseholdSnapshot::default());
        }
        for (i, size, stock) in entries {
            m.households[*i as usize] = HouseholdSnapshot {
                size: *size,
                stock: *stock,
            };
        }
    }

    pub fn set_tick(&self, t: Tick) {
        self.lock().tick = t;
    }

    #[must_use]
    pub fn tick(&self) -> Tick {
        self.lock().tick
    }

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
        if let Some(sl) = m.shops[i as usize].shelf.line_mut(intent.good) {
            sl.qty = Qty(sl.qty.get().saturating_add(intent.qty.get()));
            // Koszt nabycia wraca razem ze sztukami. Gdyby wracały same sztuki,
            // `InventoryGoods` rozjeżdżałby się z zapasem przy każdym nieudanym
            // przelewie — a niezmiennik P5 nie ma wyjątku na „prawie się udało"
            // tak samo, jak nie ma go niezmiennik masy.
            sl.cost_total = sl
                .cost_total
                .checked_add(intent.cogs)
                .expect("półka: przepełnienie kosztu przy zwrocie");
            let (offer, qty) = (sl.offer, sl.qty);
            if let Some(o) = m.offers.get_mut(offer) {
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
        m.stats.purchases += 1;
        m.stats.purchased_qty += intent.qty.get();
        m.stats.revenue = m
            .stats
            .revenue
            .checked_add(intent.agreed_price)
            .unwrap_or(m.stats.revenue);
    }
}

/// Ile powodów przecen pamięta zakład śledzony. Tyle, ile mieści panel — pierścień
/// jest tu po to, żeby gracz zobaczył „czemu wczoraj potaniało", a nie po to, żeby
/// prowadzić historię cen. Historię prowadzą miesięczne domknięcia księgi.
const REPRICE_LOG: usize = 64;

// ── M5c: ceny i księgowość ───────────────────────────────────────────────────────

impl Market {
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

    /// Odświeżenie obrazu cen konkurencji (§6.3). Zwraca liczbę sklepów, które
    /// coś zobaczyły.
    ///
    /// Odświeżają się **tylko** sklepy, którym minęła własna czujność `delay_days`,
    /// więc sklep zwykle działa na starej cenie konkurenta — i to jest zamierzone:
    /// stąd biorą się realne błędy decyzyjne AI i pole manewru gracza (przecena
    /// na trzy dni, zanim konkurencja zauważy).
    ///
    /// Jedno zapytanie na **kategorię**, nie na towar: kategorii jest osiem, towarów
    /// czterdzieści, a `query_offers` i tak zwraca całą warstwę w promieniu.
    pub fn observe_competitors(&self, t: Tick) -> u64 {
        let mut m = self.lock();
        let promien_bazowy = m.data.pricing.observe_radius_m;
        let mut obs = std::mem::take(&mut m.obs_buf);
        let mut ofr = std::mem::take(&mut m.offer_buf);
        let mut wpisy = std::mem::take(&mut m.entry_buf);
        let mut ile = 0u64;

        for i in 0..m.shops.len() {
            if !m.shops[i].observed.is_stale(t) {
                continue;
            }
            let mut promien = promien_bazowy;
            let mut nazwani: Vec<(GoodId, SiteId)> = Vec::new();
            for (g, pc) in &m.shops[i].controllers {
                if let PricePolicy::MatchCompetitor {
                    radius_m, reference, ..
                } = pc.policy
                {
                    promien = promien.max(radius_m);
                    if let CompetitorRef::Named(s) = reference {
                        nazwani.push((*g, s));
                    }
                }
            }
            let (pos, site) = (m.shops[i].pos, m.shops[i].site);

            // 1. Ceny konkurentów w promieniu, tylko dla towarów z naszej półki.
            obs.clear();
            let mut kategorie = [StockCat::Food; STOCK_CAT_COUNT];
            let mut n_kat = 0usize;
            for l in &m.shops[i].shelf.lines {
                if let Some(spec) = m.supplier.goods().spec(l.good) {
                    if !kategorie[..n_kat].contains(&spec.cat) && n_kat < STOCK_CAT_COUNT {
                        kategorie[n_kat] = spec.cat;
                        n_kat += 1;
                    }
                }
            }
            for c in &kategorie[..n_kat] {
                query_offers(&m.index, CategoryId::Stock(*c), pos, promien, &mut ofr);
                for id in &ofr {
                    let Some(o) = m.offers.get(*id) else { continue };
                    if o.site == site || m.shops[i].shelf.line(o.good).is_none() {
                        continue;
                    }
                    obs.push((o.good, o.unit_price, o.price_rev, o.site));
                }
            }
            // Porządek `(towar, cena, sklep)` — najtańszy i mediana czytają się wprost,
            // a wynik nie zależy od kolejności zwracanej przez indeks.
            obs.sort_unstable_by_key(|(g, p, _, s)| (g.get(), p.get(), s.entity().index()));

            // 2. Zbicie do jednego wpisu na towar.
            wpisy.clear();
            let mut k = 0usize;
            while k < obs.len() {
                let good = obs[k].0;
                let mut j = k;
                let mut rev = 0u32;
                while j < obs.len() && obs[j].0 == good {
                    rev = rev.wrapping_add(obs[j].2);
                    j += 1;
                }
                let n = j - k;
                let named = nazwani
                    .iter()
                    .find(|(g, _)| *g == good)
                    .and_then(|(_, s)| price_of(&m, *s, good));
                wpisy.push(CompetitorEntry {
                    good,
                    cheapest: obs[k].1,
                    cheapest_site: obs[k].3,
                    median: obs[k + n / 2].1,
                    named,
                    offers: n as u32,
                    seen_at: t,
                    rev,
                });
                k = j;
            }
            if !wpisy.is_empty() {
                ile += 1;
            }
            let kopia = wpisy.clone();
            m.shops[i].observed.replace(kopia, t);
        }
        m.stats.observations += ile;
        obs.clear();
        ofr.clear();
        wpisy.clear();
        m.obs_buf = obs;
        m.offer_buf = ofr;
        m.entry_buf = wpisy;
        ile
    }

    /// Dobowy przelot polityk cenowych (§5.6). Zwraca liczbę zmienionych ofert.
    ///
    /// Woła się **po** [`Market::observe_competitors`] i to jest kontrakt kolejności:
    /// przecena konkurenta z doby `D` wchodzi do obrazu najwcześniej w dobie `D+1`,
    /// więc reakcja mieści się w 1..=7 dobach, tak jak żąda kryterium WP6. Odwrotna
    /// kolejność dopuszczałaby ósmą dobę.
    pub fn reprice_all(&self, t: Tick) -> u64 {
        let mut m = self.lock();
        let seed = m.seed;
        let MarketInner {
            shops,
            offers,
            data,
            supplier,
            tax,
            stats,
            ..
        } = &mut *m;
        let mut zmian = 0u64;
        for shop in shops.iter_mut() {
            let sledzony = shop.tracking != LostSaleTracking::None;
            let firm_index = shop.firm.entity().index();
            let (site, osobowosc) = (shop.site, shop.pricing);
            for li in 0..shop.shelf.lines.len() {
                let linia = shop.shelf.lines[li];
                let good = linia.good;
                let Some(spec) = supplier.goods().spec(good).copied() else {
                    continue;
                };
                let zaplecze = shop
                    .inventory
                    .backroom
                    .get(&good)
                    .copied()
                    .unwrap_or_default();
                let ilosc = zaplecze.qty.get() + linia.qty.get();
                let koszt = zaplecze.cost_total.get() + linia.cost_total.get();
                // Koszt własny: średnia ważona zapasu, a przy pustym magazynie cena
                // hurtowa. Cena nie może zależeć od tego, czy akurat jest towar —
                // zależy od tego, ile kosztuje go zdobyć.
                let unit_cost = if ilosc > 0 && koszt > 0 {
                    Money(koszt).mul_ratio(PRICE_UNIT, ilosc)
                } else {
                    spec.wholesale_base
                };
                let cel = shop
                    .inventory
                    .reorder
                    .get(&good)
                    .map_or(SHELF_UNITS_PER_FACING, |r| r.target.get().max(1));
                let stock_bp = (ilosc.saturating_mul(BP) / cel).clamp(0, 200_000) as i32;
                let termin = match (zaplecze.expires, linia.expires) {
                    (Some(a), Some(b)) => Some(a.get().min(b.get())),
                    (a, b) => a.or(b).map(magnat_core::SimMinute::get),
                }
                .map(|e| {
                    u16::try_from(e.saturating_sub(t.get()) / magnat_core::time::MINUTES_PER_DAY)
                        .unwrap_or(u16::MAX)
                });
                let obserwacja = shop.observed.get(good).copied();
                let Some(pc) = shop.controllers.get_mut(&good) else {
                    continue;
                };
                let ctx = PricingCtx {
                    site,
                    good,
                    firm_index,
                    world_seed: seed,
                    unit_cost,
                    stock_bp_of_target: stock_bp,
                    days_to_expiry: termin,
                    firm: osobowosc,
                    observed: obserwacja,
                    params: &data.pricing,
                    tax: &**tax,
                };
                let Some(powod) = reprice(pc, &ctx, t) else {
                    continue;
                };
                let nowa = pc.current;
                if let Some(o) = offers.get_mut(linia.offer) {
                    // Zmiana ceny **nie brudzi indeksu** — indeks trzyma uchwyty,
                    // a cena czyta się z areny na żywo (§5.2, bez zmian od M5a).
                    o.set_price(nowa);
                }
                zmian += 1;
                if sledzony {
                    if shop.reprice_log.len() >= REPRICE_LOG {
                        shop.reprice_log.remove(0);
                    }
                    shop.reprice_log.push(powod);
                }
            }
        }
        stats.reprices += zmian;
        zmian
    }

    /// Koszty stałe miesiąca, amortyzacja i domknięcie okresu (§5.8).
    ///
    /// Sklep bez środków **nie płaci** i to jest cała „upadłość" w M5 — postępowanie
    /// prowadzi M7 (`K-10`). Zapis księgowy powstaje wyłącznie po udanym przelewie,
    /// więc `BankCurrent` nigdy nie rozjeżdża się z saldem konta w `Books`.
    pub fn close_month(&self, books: &mut Books, t: Tick) -> Money {
        let mut m = self.lock();
        let rest = m.rest_of_world;
        let koszty = m.data.costs;
        let miesiac = u32::try_from(t.get() / magnat_core::time::MINUTES_PER_MONTH).unwrap_or(0);
        let mut suma = Money::ZERO;
        for i in 0..m.shops.len() {
            let (site, konto, slots) = (m.shops[i].site, m.shops[i].account, m.shops[i].shelf.slots);
            let (czynsz, media, place) = koszty.monthly(slots);
            let pozycje: [(Money, TxKind, LedgerAccount); 3] = [
                (czynsz, TxKind::Rent { site }, LedgerAccount::RentExpense),
                (
                    media,
                    TxKind::Utility {
                        site,
                        kind: magnat_core::UtilityService::Electricity,
                    },
                    LedgerAccount::UtilitiesExpense,
                ),
                (place, TxKind::Wage { site }, LedgerAccount::WagesExpense),
            ];
            for (kwota, kind, konto_ks) in pozycje {
                if kwota.get() <= 0 {
                    continue;
                }
                let memo = TxMemo::new(kind, DecisionReason::Unspecified);
                if books.transfer(konto, rest, kwota, memo, t).is_err() {
                    continue;
                }
                let _ = ledger::post(
                    &mut m.shops[i].ledger,
                    JournalEntry::new(
                        t,
                        DecisionReason::Unspecified,
                        &[
                            (konto_ks, kwota),
                            (LedgerAccount::BankCurrent, Money(-kwota.get())),
                        ],
                    ),
                );
                suma = Money(suma.get() + kwota.get());
            }
            // Amortyzacja jest kosztem **bezgotówkowym** — nie ma po niej przelewu
            // i dlatego nie może iść tą samą ścieżką co czynsz.
            let odpis = m.shops[i].depreciation_monthly;
            if odpis.get() > 0 {
                let _ = ledger::post(
                    &mut m.shops[i].ledger,
                    JournalEntry::new(
                        t,
                        DecisionReason::Unspecified,
                        &[
                            (LedgerAccount::DepreciationExpense, odpis),
                            (LedgerAccount::AccumDepreciation, Money(-odpis.get())),
                        ],
                    ),
                );
            }
            let _ = ledger::close_period(
                &mut m.shops[i].ledger,
                miesiac,
                t,
                DecisionReason::Unspecified,
            );
        }
        suma
    }

    // ── polityki cenowe gracza (WP11) ────────────────────────────────────────────

    /// Ustawia politykę cenową towaru. **Ta sama funkcja dla gracza i dla AI** —
    /// różnica jest w `delegated`, nie w ścieżce kodu (§6.3 PRD).
    pub fn set_policy(
        &self,
        site: SiteId,
        good: GoodId,
        policy: PricePolicy,
        delegated: bool,
    ) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        let Some(pc) = m.shops[i as usize].controllers.get_mut(&good) else {
            return false;
        };
        pc.policy = policy;
        pc.delegated = delegated;
        true
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

    /// „Co by się stało z ceną dziś" — podgląd polityki bez jej zatwierdzania (WP11).
    ///
    /// Woła dokładnie to samo składanie co [`Market::reprice_all`], więc podgląd nie
    /// ma jak rozjechać się z wykonaniem.
    #[must_use]
    pub fn preview_policy(&self, site: SiteId, good: GoodId, policy: PricePolicy) -> Option<Money> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        let shop = &m.shops[i as usize];
        let pc = shop.controllers.get(&good)?;
        let spec = *m.supplier.goods().spec(good)?;
        let linia = shop.shelf.line(good).copied();
        let zaplecze = shop
            .inventory
            .backroom
            .get(&good)
            .copied()
            .unwrap_or_default();
        let ilosc = zaplecze.qty.get() + linia.map_or(0, |l| l.qty.get());
        let koszt = zaplecze.cost_total.get() + linia.map_or(0, |l| l.cost_total.get());
        let unit_cost = if ilosc > 0 && koszt > 0 {
            Money(koszt).mul_ratio(PRICE_UNIT, ilosc)
        } else {
            spec.wholesale_base
        };
        let cel = shop
            .inventory
            .reorder
            .get(&good)
            .map_or(SHELF_UNITS_PER_FACING, |r| r.target.get().max(1));
        let ctx = PricingCtx {
            site,
            good,
            firm_index: shop.firm.entity().index(),
            world_seed: m.seed,
            unit_cost,
            stock_bp_of_target: (ilosc.saturating_mul(BP) / cel).clamp(0, 200_000) as i32,
            days_to_expiry: None,
            firm: shop.pricing,
            observed: shop.observed.get(good).copied(),
            params: &m.data.pricing,
            tax: &*m.tax,
        };
        Some(preview_price(policy, pc, &ctx))
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

/// Zapłata dostawcy z góry: pieniądz wyszedł, towar jeszcze nie przyjechał.
///
/// `TradePayable` chodzi przez ten czas na saldzie Wn — to jest zaliczka, a nie
/// zobowiązanie. M6 wnosi terminy płatności i saldo staje się tym, czym nazwa
/// obiecuje; do tego czasu jedno konto niesie obie strony, bo obie są rozrachunkiem
/// z tym samym dostawcą.
fn post_purchase(l: &mut Ledger, kwota: Money, t: Tick) {
    if kwota.get() <= 0 {
        return;
    }
    let _ = ledger::post(
        l,
        JournalEntry::new(
            t,
            DecisionReason::Unspecified,
            &[
                (LedgerAccount::TradePayable, kwota),
                (LedgerAccount::BankCurrent, Money(-kwota.get())),
            ],
        ),
    );
}

/// Przyjęcie towaru na stan — dopiero **tu** rośnie `InventoryGoods`, bo dopiero
/// tu towar istnieje. Gdyby rósł przy zamówieniu, niezmiennik P5 pękałby na każdym
/// towarze będącym w drodze.
fn post_receipt(l: &mut Ledger, kwota: Money, t: Tick) {
    if kwota.get() <= 0 {
        return;
    }
    let _ = ledger::post(
        l,
        JournalEntry::new(
            t,
            DecisionReason::Unspecified,
            &[
                (LedgerAccount::InventoryGoods, kwota),
                (LedgerAccount::TradePayable, Money(-kwota.get())),
            ],
        ),
    );
}

/// Cena towaru we wskazanym sklepie — potrzebna wyłącznie przy `CompetitorRef::Named`.
fn price_of(m: &MarketInner, site: SiteId, good: GoodId) -> Option<Money> {
    let i = m.by_site.get(&site).copied()?;
    let line = m.shops[i as usize].shelf.line(good)?;
    m.offers.get(line.offer).map(|o| o.unit_price)
}

fn wholesale_memo(good: GoodId, qty: Qty) -> TxMemo {
    TxMemo::new(
        TxKind::WholesalePurchase {
            good,
            qty,
            supplier: SupplierRef::External,
        },
        DecisionReason::Unspecified,
    )
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

impl MarketInner {
    fn snapshot(&self, household: u32) -> HouseholdSnapshot {
        self.households
            .get(household as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Czy potrzebę wolno jeszcze zaspokoić w domu. Dopóki gospodarstwo ma zapas,
    /// M5 nie wypycha nikogo do sklepu — kolacja w domu zostaje kolacją w domu.
    fn has_home_stock(&self, household: u32, cats: &[StockCat]) -> bool {
        let s = self.snapshot(household);
        cats.iter()
            .any(|c| s.stock[c.as_index()] >= self.data.home_stock_min_days)
    }

    fn vot_for(&self, status: Q) -> i64 {
        // `vot` ze statusu, nie z dochodu: dochód gospodarstwa wchodzi do modelu razem
        // z budżetem w M5d/WP8 (`ponytail:` sufit nazwany w `data/economy/choice.ron`).
        self.data.vot_base_gr_per_min * (100 + i64::from(status.get())) / 100
    }

    fn buyer_state(&self, status: Q, openness: Q, vot: i64, cat: StockCat) -> BuyerState {
        BuyerState {
            status,
            openness,
            budget_ref: budget_ref_for(cat, &self.data),
            vot_gr_per_min: vot,
        }
    }
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
        for proba in 0..2u32 {
            cand.clear();
            let r = radius * (proba + 1);
            for c in cats {
                query_offers(&m.index, CategoryId::Stock(*c), origin, r, &mut buf);
                for id in &buf {
                    let Some(o) = m.offers.get(*id) else { continue };
                    if o.available.get() <= 0 || !known.knows(PlaceRef::Site(o.site)) {
                        continue;
                    }
                    let Some(i) = m.by_site.get(&o.site).copied() else {
                        continue;
                    };
                    let Some(spec) = m.supplier.goods().spec(o.good) else {
                        continue;
                    };
                    let want = wanted_qty(spec.daily_per_person, snap.size, dni);
                    if o.available.get() < want.get() {
                        continue;
                    }
                    let travel_min =
                        walk_minutes(origin_c, shop_coord(m.shops[i as usize].pos), 100);
                    if travel_min > max_travel_min {
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
            if cand.len() >= k_min {
                break;
            }
        }
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
            let cat = m.supplier.goods().spec(c.good).map_or(cats[0], |s| s.cat);
            let st = m.buyer_state(status, openness, vot, cat);
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
        let cat = m
            .supplier
            .goods()
            .spec(wybrany.good)
            .map_or(cats[0], |s| s.cat);
        let plan = PlannedPurchase {
            site: wybrany.site,
            need,
            weights: w,
            status,
            openness,
            vot_gr_per_min: vot,
            threshold: purchase_threshold(need, who.needs.get(need), 0, &m.data),
            unit_price: m
                .offers
                .get(wybrany.offer)
                .map_or(Money::ZERO, |o| o.unit_price),
            good: wybrany.good,
        };
        let _ = cat;
        m.planned.insert(who.id.entity().index(), plan);

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
        let plan = m.planned.get(&who_key(req)).copied().filter(|p| p.site == site);
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
                    purchase_threshold(req.need, Q::new(50), 0, &m.data),
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
            for gi in m.supplier.goods().in_cat(*c) {
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
            let spec = *m.supplier.goods().at(*gi);
            let Some(line) = m.shops[i as usize].shelf.line(spec.good).copied() else {
                continue;
            };
            if line.qty.get() <= 0 {
                continue;
            }
            let Some(offer) = m.offers.get(line.offer).copied() else {
                continue;
            };
            let want = wanted_qty(spec.daily_per_person, osob, dni);
            let take = Qty(want.get().min(line.qty.get()));
            let total = line_total(offer.unit_price, take);
            if total > budzet {
                powod = RejectCause::BudgetExhausted;
                continue;
            }
            // Poślizg ceny (§5.5): jeśli cena ruszyła się między decyzją a wizytą
            // o więcej niż `price_slippage_bp`, próg ocenia się **ponownie** — i to
            // jest ta jedna ponowna ocena, bez rekurencji.
            if let Some(stara) = cena_z_decyzji.filter(|c| c.get() > 0 && spec.good == plan.map_or(GoodId(u16::MAX), |p| p.good)) {
                let delta = (offer.unit_price.get() - stara.get()).abs() * 10_000 / stara.get();
                if delta > slippage {
                    m.stats.slippage_rechecks += 1;
                }
            }
            let st = m.buyer_state(status, openness, vot, spec.cat);
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
            let koszt_wlasny = m.shops[i as usize]
                .shelf
                .line_mut(spec.good)
                .map_or(Money::ZERO, |sl| sl.take(take));
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
                arrived: tick,
                reason,
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

impl HashState for Market {
    /// Stan rynku w hashu świata (00 §3.6).
    ///
    /// Arena ofert wchodzi tu, a nie przez `register_arena_hash` — i to jest świadoma
    /// korekta wobec M5a. `register_arena_hash::<T>(kind)` czyta `Arena<T>` **jako
    /// zasób świata**, a arena ofert musi mieszkać w `Market`, bo `PlaceProvider` nie
    /// widzi `World` (patrz nagłówek modułu). Wymaganie `K-16` jest spełnione co do
    /// treści: arena jest haszowana w kolejności indeksów, razem ze slotami
    /// i generacjami. Zmienia się wyłącznie sekcja, w której ląduje — z „areny wg
    /// `ArenaKind`" na „zasoby wg nazwy typu".
    ///
    /// `planned`, `committed` i bufory **nie wchodzą**: to są dane pomocnicze jednej
    /// decyzji, odtwarzalne z planu dnia, a nie stan świata. Intencje wchodzą, bo
    /// między `fulfil` a rozliczeniem są jedynym śladem po zdjętym z półki towarze.
    fn hash_state(&self, h: &mut StateHasher) {
        let m = self.lock();
        m.offers.hash_state(h);
        h.write_u32(m.shops.len() as u32);
        // `by_site` jest `BTreeMap`, więc kolejność idzie po `SiteId`, nie po wstawianiu.
        for (site, i) in &m.by_site {
            site.entity().hash_state(h);
            m.shops[*i as usize].hash_state(h);
        }
        h.write_u32(m.intents.len() as u32);
        for it in &m.intents {
            it.buyer.entity().hash_state(h);
            it.good.hash_state(h);
            it.qty.hash_state(h);
            it.agreed_price.hash_state(h);
            it.cogs.hash_state(h);
        }
    }
}
