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
    Arena, CitizenId, DecisionReason, DistrictId, FirmId, FixedCost, GoodId, HashState,
    HouseholdId, LoanKind, Money, NeedKind, PlaceKind, PlaceRef, Qty, RejectCause, RejectCredit,
    SimMinute, SiteId, StateHasher, StockCat, Tick, TraitId, WorldCoord, FIXED_COST_COUNT, Q,
    STOCK_CAT_COUNT,
};
use magnat_jobs::JobPool;
use magnat_spatial::{GridSpec, Vec2};

use crate::books::{AccountId, AccountOwner, Books, LoanId, SupplierRef, TxKind, TxMemo};
use crate::budget::{budget_ref_for_need, plan_budget, HouseholdBudget, HouseholdProfile};
use crate::chain_supply::default_supplier;
use crate::choice::{
    choose_offer, days_bought, dominant_term, offer_noise, purchase_threshold, rating_of,
    utility_of_offer, wanted_qty, weights_for, BuyerState, Candidate,
};
use crate::cpi::{CpiTracker, IndexBp};
use crate::credit::{assess_credit, BaseRate, CreditDecision, LoanApplication, LoanBook};
use crate::data::{EconomyData, UtilityWeights};
use crate::kernel::BP;
use crate::ledger::{self, JournalEntry, Ledger, LedgerAccount};
use crate::offer::{query_offers, CategoryId, Offer, OfferId, OfferIndex, PriceBasis};
use crate::panel::{
    BalanceSample, CompetitorRow, CustomerStats, FinanceSummary, LostSalesView, PriceDist,
    ShelfRow, ShopPanelSnapshot,
};
use crate::pricing::{
    preview_price, reprice, CompetitorEntry, CompetitorRef, CompetitorSnapshot, FirmPricing,
    PriceController, PricePolicy, PricingCtx,
};
use crate::shop::{
    AssortmentPolicy, LostSale, LostSaleHistogram, LostSaleTracking, ReorderPolicy, Shelf,
    ShelfLine, Shop, ShopCustomers, ShopInventory, ShopLostSales,
};
use crate::supply::{line_total, GoodTable, Wholesale, PRICE_UNIT};
use crate::tax::{NoTax, TaxEngine};
use magnat_supply::ChainHandle;

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

mod api;
mod close;
mod enforce;
pub use enforce::SiteEnforcementRow;
mod fulfil;
mod household;
mod lifecycle;
mod price_day;
mod readout;
mod restock;
mod tax;

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
    /// Co dokładnie zeszło z półki. Niesione intencją, bo nieudane rozliczenie musi
    /// oddać **tę samą partię** — jakość, markę, datę i pochodzenie — a nie sztuki
    /// bez właściwości (WP11). Poza hashem tak samo jak reszta intencji poza
    /// pięcioma polami, które rozliczenie naprawdę zmienia.
    pub taken: Option<magnat_supply::BatchSlice>,
    pub arrived: Tick,
    pub reason: DecisionReason,
    /// Dzielnica zamieszkania kupującego — „skąd" w karcie Klienci (§5.12).
    /// Niesione intencją, bo rozliczenie ma świat, ale nie ma już decyzji,
    /// a decyzja ma `CitizenView` i nie ma świata.
    pub district: u16,
    /// Status kupującego — „kto" w karcie Klienci, po przeliczeniu na klasę.
    pub status: Q,
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
    /// Dzielnica zamieszkania — jedyna rzecz z `CitizenView`, której `fulfil`
    /// nie ma skąd wziąć, a karta Klienci jej potrzebuje.
    district: u16,
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
    /// Dzielnica, w której stoi zakład. Rynek jej nie używa do niczego poza
    /// metrykami: bramka G6 balansatora mierzy koncentrację **per dzielnica**,
    /// a pozycja w metrach nie mówi, gdzie kończy się jedna, a zaczyna druga.
    pub district: u16,
}

/// Bank w M5: `FirmId`, konto i funkcja `assess_credit` zamiast AI (decyzja
/// otwarta nr 7, szeroka część — propozycja domyślna przyjęta w M5d).
///
/// Nie ma pracowników, produkcji ani osobowości; M7 czyni go pełną firmą.
/// Ma za to konto jak każda firma, bo przez nie przechodzi **cała** kreacja
/// i destrukcja pieniądza kredytowego.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bank {
    pub firm: FirmId,
    pub account: AccountId,
}

/// Gospodarstwo na wejściu i wyjściu miesięcznego rozliczenia (M5d §5.9).
///
/// Salda wchodzą i wychodzą **przez tę strukturę**, a nie przez `Books`: pieniądz
/// gospodarstwa mieszka w komponencie `Household` (`U-17`), więc wołający zdejmuje
/// go stamtąd przed wywołaniem i wpisuje z powrotem po nim. Kanał sektora gospodarstw
/// w `MoneySupplyLedger` jest drugą stroną każdej z tych kwot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HouseholdMonth {
    pub index: u32,
    pub profile: HouseholdProfile,
    pub income: Money,
    pub cash: Money,
    pub bank: Money,
    pub savings: Money,
    // ── wyjście ──
    /// Ile z kosztów stałych zostało nieopłacone w tym miesiącu.
    pub shortfall: Money,
    /// Powód decyzji kredytowej, jeśli gospodarstwo składało wniosek.
    pub credit: Option<DecisionReason>,
    /// Powód pierwszej niedopłaty, jeśli jakaś była.
    pub unpaid: Option<DecisionReason>,
}

/// Zbiorczy wynik miesiąca gospodarstw — to, co wypisuje scenariusz i zbiera balansator.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HouseholdMonthReport {
    pub planned: u64,
    pub fixed_paid: Money,
    pub savings: Money,
    pub credit_applications: u64,
    pub credit_granted: u64,
    pub credit_amount: Money,
    pub installments_paid: u64,
    pub interest_paid: Money,
    pub principal_repaid: Money,
    pub shortfalls: u64,
    pub arrears_added: Money,
}

/// Ile wpisów trzyma okno decyzji budżetowych. Tyle samo co pierścień utraconych
/// sprzedaży — to jest podgląd dla panelu, nie historia.
const BUDGET_LOG_RING: usize = 256;

pub(crate) struct MarketInner {
    offers: Arena<Offer>,
    index: OfferIndex,
    /// Sklepy w kolejności zakładania; `by_site` daje dostęp po `SiteId`.
    pub(crate) shops: Vec<Shop>,
    pub(crate) by_site: BTreeMap<SiteId, u32>,
    /// Konta zakładów produkcyjnych (`AP-2`). Zakład **nie jest** sklepem: nie ma
    /// półki, ceny ani pierścienia utraconych sprzedaży, a księgi zakładowej dostanie
    /// dopiero w M7 razem z rachunkiem wyniku firmy. Ma za to rachunek bieżący i to
    /// wystarcza, żeby rozliczenie rynku B2B miało dokąd trafić — bez tego zakup mąki
    /// przez piekarnię był darmowy, a pieniądz przestawał się domykać z masą.
    plants: BTreeMap<SiteId, (magnat_core::FirmId, AccountId)>,
    /// Kto dostarcza towar na zaplecze. Od WP11 **`dyn`**, a nie typ konkretny:
    /// bez tego nie ma jak podmienić dostawcy zewnętrznego na rynek B2B, a kryterium
    /// pakietu mówi wprost o przełączniku (`AK-2`). Domyślną implementacją jest
    /// [`crate::chain_supply::ChainSupply`]; `ExternalSupplier` zostaje za feature
    /// `infinite_supply`.
    supplier: Box<dyn Wholesale + Send>,
    /// Katalog detaliczny. Wyjęty z dostawcy, bo czyta go osiemnaście miejsc rynku,
    /// a nie jest własnością dostawcy — jest własnością danych.
    pub(crate) goods: GoodTable,
    /// Łańcuch dostaw M6: magazyn, zakłady, transport, rynek B2B. Uchwyt, nie kopia —
    /// ten sam łańcuch widzi produkcja i ten sam widzi półka.
    pub(crate) chain: ChainHandle,
    data: EconomyData,
    needs: Arc<NeedTable>,
    places: Arc<PlaceTable>,
    /// Delegat dla miejsc, które nie są sklepem: dom, jadłodajnia, lekarz.
    /// M5b nie odbiera M3 niczego, co M3 już umiał (§2 dokumentu fazy).
    fallback: InfinitePlaces,
    households: Vec<HouseholdSnapshot>,
    /// Budżety gospodarstw, indeks = indeks encji. Stoją tutaj, a nie w zasobie
    /// świata, bo czyta je `candidates` — a `PlaceProvider` nie dostaje `&World`.
    budgets: Vec<HouseholdBudget>,
    loans: LoanBook,
    /// Jedyny bank w M5. `None`, dopóki scenariusz go nie otworzy — wtedy każdy
    /// wniosek kończy się `RejectCredit::NoLender`, a nie cichym brakiem ścieżki.
    bank: Option<Bank>,
    cpi: CpiTracker,
    /// Okno ostatnich decyzji budżetowych i kredytowych — podgląd dla panelu (M5e)
    /// i dla testu „ścieżka w pełni wyjaśnialna". Poza hashem, tak samo jak dziennik.
    budget_log: Vec<(u32, DecisionReason)>,
    planned: BTreeMap<u32, PlannedPurchase>,
    intents: Vec<PurchaseIntent>,
    /// Kwoty zaklepane w bieżącym ticku, po indeksie encji gospodarstwa. Bez tego
    /// dwa zakupy tej samej minuty widziałyby ten sam budżet dwa razy.
    committed: BTreeMap<u32, Money>,
    rest_of_world: AccountId,
    /// Hak podatkowy (`K-7`). W M5 `NoTax`; M8 wstawia `CityTaxEngine`.
    pub(crate) tax: Box<dyn TaxEngine>,
    /// Daniny naliczone na rozliczeniu hurtowym, czekające na odebranie przez
    /// miasto. `BTreeMap`, więc kolejność idzie po `SiteId`, a nie po kolejności
    /// dostaw (00 §3.2).
    pub(crate) b2b_outbox: BTreeMap<SiteId, crate::shop::B2bTax>,
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
        chain: ChainHandle,
        needs: Arc<NeedTable>,
        places: Arc<PlaceTable>,
        rest_of_world: AccountId,
    ) -> Market {
        // CPI rozwiązuje koszyk raz, przy budowie rynku — potem katalog już się
        // nie zmienia, a przeszukiwanie go przy każdej transakcji byłoby kosztem
        // na gorącej ścieżce (§7.3).
        let cpi = CpiTracker::new(&data, &goods);
        let supplier = default_supplier(seed, &goods, &chain);
        Market(Arc::new(Mutex::new(MarketInner {
            offers: Arena::new(),
            index: OfferIndex::new(grid),
            shops: Vec::new(),
            by_site: BTreeMap::new(),
            plants: BTreeMap::new(),
            supplier,
            goods,
            chain,
            data,
            needs: needs.clone(),
            places: places.clone(),
            fallback: InfinitePlaces::new(places, needs),
            households: Vec::new(),
            budgets: Vec::new(),
            loans: LoanBook::new(),
            bank: None,
            cpi,
            budget_log: Vec::new(),
            planned: BTreeMap::new(),
            intents: Vec::new(),
            committed: BTreeMap::new(),
            rest_of_world,
            tax: Box::new(NoTax),
            b2b_outbox: BTreeMap::new(),
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

    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, MarketInner> {
        // Zatrucie zamka znaczy panikę w systemie symulacji — wtedy świat i tak jest
        // do wyrzucenia, więc rozwijamy ją dalej zamiast liczyć na stanie sprzed paniki.
        self.0.lock().expect("Market: zatruty zamek")
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
        if m.budgets.len() <= max {
            m.budgets.resize(max + 1, HouseholdBudget::default());
        }
        for (i, size, stock) in entries {
            m.households[*i as usize] = HouseholdSnapshot {
                size: *size,
                stock: *stock,
            };
        }
    }

    /// Ziarno świata — potrzebne łańcuchowi dostaw do losowań awarii i szumu wyceny.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.lock().seed
    }

    pub fn set_tick(&self, t: Tick) {
        self.lock().tick = t;
    }

    #[must_use]
    pub fn tick(&self) -> Tick {
        self.lock().tick
    }
}

impl MarketInner {
    // ── półka jako slot magazynu (WP11) ────────────────────────────────────────
    //
    // Trzy najkrótsze funkcje w tym pliku i trzy, które zdejmują z rynku detalicznego
    // cały dawny stan zapasu. Ilość, koszt i data ważności mieszkają od WP11
    // w magazynie M6; tutaj zostaje przeliczenie masy na sztuki, bo `Qty` jest
    // jednostką detalu, a `Mass` jednostką łańcucha.

    /// Ile sztuk tego towaru leży na półce sklepu `i`.
    pub(crate) fn shelf_units(&self, i: usize, good: GoodId) -> Qty {
        let slot = self.shops[i].shelf_slot;
        let ch = self.chain.lock();
        self.chain
            .cat
            .good(good)
            .units_of_mass(ch.store.shelf_state(slot, good).mass)
    }

    /// Ile sztuk tego towaru leży na zapleczu sklepu `i`.
    pub(crate) fn backroom_units(&self, i: usize, good: GoodId) -> Qty {
        let slot = self.shops[i].backroom;
        let ch = self.chain.lock();
        self.chain
            .cat
            .good(good)
            .units_of_mass(ch.store.available(slot, good, magnat_core::Q::MIN))
    }

    /// Stan półki: masa, jakość, marka, data i koszt nabycia.
    pub(crate) fn shelf_state(&self, i: usize, good: GoodId) -> magnat_supply::ShelfState {
        let slot = self.shops[i].shelf_slot;
        let ch = self.chain.lock();
        ch.store.shelf_state(slot, good)
    }

    /// Koszt jednostkowy zapasu sklepu: **zaplecze i półka razem**, za `PRICE_UNIT`.
    ///
    /// Jeden wzór dla przeceny (`reprice_all`), dla panelu i dla metryki marży
    /// balansatora — inny wzór w którymkolwiek z tych trzech miejsc znaczyłby marżę
    /// w panelu inną niż marża, na której stoi przecena. Przy pustym zapasie wchodzi
    /// cena hurtowa z katalogu, bo od czegoś marża liczyć się musi.
    pub(crate) fn unit_cost(&self, i: usize, good: GoodId) -> Money {
        let (backroom, shelf_slot) = (self.shops[i].backroom, self.shops[i].shelf_slot);
        let (masa, koszt) = {
            let ch = self.chain.lock();
            let b = ch.store.shelf_state(backroom, good);
            let p = ch.store.shelf_state(shelf_slot, good);
            (b.mass.0 + p.mass.0, b.cost_total.get() + p.cost_total.get())
        };
        let ilosc = self
            .chain
            .cat
            .good(good)
            .units_of_mass(magnat_core::Mass(masa))
            .get();
        if ilosc > 0 && koszt > 0 {
            Money(koszt).mul_ratio(PRICE_UNIT, ilosc)
        } else {
            self.goods
                .spec(good)
                .map_or(Money::ZERO, |g| g.wholesale_base)
        }
    }

    /// Zdejmuje z półki podaną liczbę sztuk. `None`, gdy tyle nie leży.
    pub(crate) fn shelf_pick(
        &mut self,
        i: usize,
        good: GoodId,
        qty: Qty,
    ) -> Option<magnat_supply::BatchSlice> {
        let slot = self.shops[i].shelf_slot;
        let masa = self.chain.cat.good(good).mass_of_units(qty);
        let mut ch = self.chain.lock();
        ch.store.shelf_pick(slot, good, masa)
    }

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

    fn buyer_state(
        &self,
        status: Q,
        openness: Q,
        vot: i64,
        cat: StockCat,
        household: u32,
    ) -> BuyerState {
        BuyerState {
            status,
            openness,
            // Mianownik członu ceny z **koperty gospodarstwa** (M5d/WP8). Gospodarstwo
            // bez zaplanowanego budżetu wraca do stałej z `choice.ron` — to jest
            // pierwsze kilka dób świata, zanim wypadnie granica miesiąca.
            budget_ref: match self.budgets.get(household as usize) {
                Some(b) => budget_ref_for_need(b, cat, &self.data),
                // Gospodarstwo spoza migawki: stała z `choice.ron`, bez kopiowania
                // budżetu na gorącej ścieżce (§7.3 — zero alokacji i zero zbędnych
                // kopii struktury, która ma ćwierć kilobajta).
                None => crate::choice::budget_ref_for(cat, &self.data),
            },
            vot_gr_per_min: vot,
        }
    }
}

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

/// Zamówienie u dostawcy **jest decyzją firmy**, więc niesie powód (PRD §14.1).
///
/// Powód jest ten sam, którym M3 tłumaczy wyjście gospodarstwa po zakupy
/// (`StockBelowThreshold`), i to nie jest oszczędność na wariancie: reguła jest
/// dosłownie ta sama po obu stronach lady — zapas spadł poniżej progu, więc
/// uzupełniamy. `days_left` liczy się z pokrycia, czyli z tego samego, co panel
/// pokazuje jako „dni pokrycia".
fn wholesale_memo(good: GoodId, qty: Qty, cat: StockCat, days_left: u8) -> TxMemo {
    TxMemo::new(
        TxKind::WholesalePurchase {
            good,
            qty,
            supplier: SupplierRef::External,
        },
        DecisionReason::StockBelowThreshold { cat, days_left },
    )
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
        h.write_u32(m.plants.len() as u32);
        for (site, (firm, acc)) in &m.plants {
            site.entity().hash_state(h);
            firm.entity().hash_state(h);
            h.write_u32(acc.0);
        }
        h.write_u32(m.intents.len() as u32);
        for it in &m.intents {
            it.buyer.entity().hash_state(h);
            it.good.hash_state(h);
            it.qty.hash_state(h);
            it.agreed_price.hash_state(h);
            it.cogs.hash_state(h);
        }
        // M5d. Budżety, kredyty i koszyk CPI **są stanem**, a nie pomiarem: stopa
        // bazowa wpływa na oprocentowanie, oprocentowanie na ratę, rata na saldo
        // gospodarstwa. Pomiar, który zmienia świat, wchodzi do hasha.
        // `budget_log` nie wchodzi — to okno podglądu, jak dziennik zakładu.
        h.write_u32(m.budgets.len() as u32);
        for b in &m.budgets {
            b.hash_state(h);
        }
        // Cło i akcyza naliczone, a jeszcze nieodebrane przez miasto, **są stanem**:
        // między odprawą a deklaracją są jedynym śladem po pieniądzu, który należy
        // się budżetowi. Tak samo jak intencje zakupowe wyżej.
        h.write_u32(m.b2b_outbox.len() as u32);
        for (site, t) in &m.b2b_outbox {
            site.entity().hash_state(h);
            t.hash_state(h);
        }
        m.loans.hash_state(h);
        h.write_u8(u8::from(m.bank.is_some()));
        if let Some(bank) = m.bank {
            bank.firm.entity().hash_state(h);
            h.write_u32(bank.account.0);
        }
        m.cpi.hash_state(h);
    }
}

/// Jedna linia półki w zdjęciu miasta dla modelu makro (M7f WP13).
///
/// Struktura faktów, nie referencji — ten sam wybór co przy `FirmView` (`BC-1`)
/// i z tego samego powodu: konsument stoi poza zamkiem rynku i nie ma prawa
/// trzymać wskaźnika do jego wnętrza przez cały krok.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShelfSnapshot {
    pub site: SiteId,
    pub firm: FirmId,
    pub district: DistrictId,
    pub good: GoodId,
    /// Cena **netto** (`K-7`): makro liczy marże, a marża zawsze stoi na netto.
    pub price_net: Money,
    /// Półka i zaplecze razem — dla doby makro to jeden zapas.
    pub qty: Qty,
}
