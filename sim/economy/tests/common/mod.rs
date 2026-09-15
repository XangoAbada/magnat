//! Wspólne rusztowanie testów M5b: mały świat z gospodarstwami i sklepami.
//!
//! Testy fazy sprawdzają **zachowanie rynku**, nie generator miasta, więc świat jest
//! tu zbudowany wprost: dwa–trzy sklepy o znanych pozycjach, gospodarstwa o znanym
//! saldzie i katalog towarów o znanych cenach. Dzięki temu każdy wynik da się
//! policzyć na kartce, a test, który pęka, mówi, co pękło.

#![allow(dead_code)]

use std::num::NonZeroU32;
use std::sync::Arc;

use magnat_agents::{
    CitizenView, Identity, Knowledge, KnowledgeKind, KnowledgeView, NeedTable, Needs, Personality,
    PlaceEntry, PlaceTable, Residence, Vitals,
};
use magnat_agents::{Household, Population};
use magnat_core::{
    BuildingId, CitizenId, DecisionReason, Entity, FirmId, GoodId, Money, NeedKind, PlaceKind,
    PlaceRef, Qty, SiteId, StockCat, Tick, WorldCoord, Q,
};
use magnat_economy::{
    AccountId, AccountKind, AccountOwner, Books, EconomyData, GoodTable, Market, ShopSeed, TxKind,
    TxMemo,
};
use magnat_ecs::World;
use magnat_spatial::{Aabb2, GridSpec, Vec2};

pub const CITY_M: f32 = 4_000.0;

/// Zużycie dobowe na osobę dla towarów testowych — 250 jednostek, czyli ćwierć
/// jednostki ceny. Przy `purchase_days = 4` i gospodarstwie jednoosobowym jedno
/// wyjście po zakupy to **dokładnie jedna jednostka**, więc „10 sztuk na półce"
/// z kryterium WP5 znaczy „dziesięciu klientów".
pub const DAILY_PER_PERSON: i64 = 250;
pub const WHOLESALE_BASE: i64 = 200;

#[must_use]
pub fn ent(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::MIN)
}

#[must_use]
pub fn spec() -> GridSpec {
    GridSpec::covering(
        Aabb2::new(Vec2::new(0.0, 0.0), Vec2::new(CITY_M, CITY_M)),
        200,
    )
}

/// Katalog detaliczny: `GoodId` po pozycji w `retail.ron`, cena hurtowa stała.
#[must_use]
pub fn goods(data: &EconomyData) -> GoodTable {
    let keys: Vec<String> = data.retail.goods.iter().map(|g| g.key.clone()).collect();
    GoodTable::build(&data.retail, |key| {
        let i = keys.iter().position(|k| k == key)?;
        Some((
            GoodId(i as u16),
            Money(WHOLESALE_BASE),
            Qty(DAILY_PER_PERSON),
        ))
    })
}

/// Towar po kluczu tekstowym z `retail.ron`.
#[must_use]
pub fn good_by_key(data: &EconomyData, key: &str) -> GoodId {
    let i = data
        .retail
        .goods
        .iter()
        .position(|g| g.key == key)
        .expect("towar spoza data/economy/retail.ron");
    GoodId(i as u16)
}

/// Pierwszy towar kategorii — ten, po który decyzja zakupowa sięga najpierw.
#[must_use]
pub fn first_good(data: &EconomyData, cat: StockCat) -> GoodId {
    let i = data
        .retail
        .goods
        .iter()
        .position(|g| g.cat == cat)
        .expect("kategoria bez towaru w data/economy/retail.ron");
    GoodId(i as u16)
}

pub struct Bench {
    pub market: Market,
    pub books: Books,
    pub rest: AccountId,
    pub sites: Vec<SiteId>,
    pub home: PlaceRef,
    pub needs: Arc<NeedTable>,
}

/// Świat testowy: dom w (0,0) i sklepy w podanych punktach, każdy z kontem i kapitałem.
#[must_use]
pub fn bench(seed: u64, pos: &[Vec2]) -> Bench {
    let data = EconomyData::load_default().expect("data/economy/");
    let katalog = goods(&data);
    let needs = Arc::new(NeedTable::load_default().expect("data/needs/"));

    let home = PlaceRef::Building(BuildingId(ent(0)));
    let mut entries = vec![PlaceEntry {
        place: home,
        kind: PlaceKind::Home,
        at: WorldCoord::new(0, 0, 0),
    }];
    let sites: Vec<SiteId> = (0..pos.len() as u32)
        .map(|i| SiteId(ent(100 + i)))
        .collect();
    for (i, p) in pos.iter().enumerate() {
        entries.push(PlaceEntry {
            place: PlaceRef::Site(sites[i]),
            kind: PlaceKind::Grocery,
            at: WorldCoord::new((p.x * 100.0) as i32, (p.y * 100.0) as i32, 0),
        });
    }
    let places = Arc::new(PlaceTable::build(entries));

    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    books.endow(rest, Money(1_000_000_000), Tick(0)).unwrap();

    let market = Market::new(spec(), seed, data, katalog, needs.clone(), places, rest);
    for (i, p) in pos.iter().enumerate() {
        let firm = FirmId(ent(200 + i as u32));
        let acc = books.open_account(
            AccountOwner::Firm(firm),
            AccountKind::Current,
            None,
            Money::ZERO,
        );
        books
            .transfer(
                rest,
                acc,
                Money(50_000_000),
                TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
                Tick(0),
            )
            .unwrap();
        assert!(market.open_shop(
            ShopSeed {
                site: sites[i],
                firm,
                pos: *p,
                kind: PlaceKind::Grocery,
                shelf_slots: 4,
                capacity_m3: 200,
            },
            acc,
            Tick(0),
        ));
        // Kapitał wniesiony na rachunek musi trafić też do księgi zakładu (M5c §5.8),
        // inaczej `BankCurrent` od pierwszej minuty nie zgadza się z saldem konta.
        market.record_capital(sites[i], Money(50_000_000), Tick(0));
    }
    Bench {
        market,
        books,
        rest,
        sites,
        home,
        needs,
    }
}

/// Otwiera bank miasta i rejestruje go w rynku (M5d §5.10).
///
/// Bez tego kroku każdy wniosek kredytowy kończy się `RejectCredit::NoLender` —
/// i to jest właściwe zachowanie, a nie brak ścieżki: miasto bez banku nie daje
/// kredytu. Test, który chce zobaczyć kredyt, musi bank otworzyć.
pub fn otworz_bank(b: &mut Bench) -> FirmId {
    let firm = FirmId(ent(900));
    let acc = b.books.open_account(
        AccountOwner::Bank(firm),
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    b.books
        .transfer(
            b.rest,
            acc,
            Money(500_000_000),
            TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
            Tick(0),
        )
        .unwrap();
    b.market.open_bank(firm, acc);
    firm
}

/// Opis gospodarstwa dla świata testowego: liczba osób, dochód i saldo rachunku.
#[derive(Clone, Copy)]
pub struct Gd {
    pub size: u8,
    pub income_gr: i64,
    pub bank_gr: i64,
}

impl Gd {
    #[must_use]
    pub fn new(size: u8, income_gr: i64, bank_gr: i64) -> Gd {
        Gd {
            size,
            income_gr,
            bank_gr,
        }
    }
}

/// Świat ECS z gospodarstwami **i spisem `Population`**.
///
/// Spis jest potrzebny, bo `pay_incomes` i `settle_household_month` chodzą po nim,
/// a nie po archetypach: gospodarstwo bez wpisu w spisie nie istnieje dla gospodarki.
/// Każde gospodarstwo dostaje jednego mieszkańca z osobowością i potrzebami —
/// to od niego bierze się `Thrift` i `Ambition` w profilu budżetu.
#[must_use]
pub fn swiat_z_gospodarstwami(b: &mut Bench, gd: &[Gd]) -> (World, Vec<Entity>) {
    let mut w = World::new(1);
    magnat_agents::register_components(&mut w);
    w.insert_resource(std::mem::replace(&mut b.books, Books::new()));
    w.insert_resource(b.market.clone());
    let mut pop = Population::new();
    let mut encje = Vec::with_capacity(gd.len());
    for g in gd {
        let mieszkaniec = w
            .spawn()
            .with(Personality([50; 8]))
            .with(Needs::default())
            .id();
        pop.add_citizen(mieszkaniec);
        let mut h = Household {
            size: g.size,
            flags: Household::FLAG_ACTIVE,
            bank: Money(g.bank_gr),
            income_monthly: Money(g.income_gr),
            ..Household::default()
        };
        h.members[0] = mieszkaniec.index();
        let e = w.spawn().with(h).id();
        pop.add_household(e);
        encje.push(e);
    }
    w.insert_resource(pop);
    (w, encje)
}

/// Suma pieniądza świata testowego: księgi plus salda gospodarstw.
///
/// `society::total_money` wymaga pełnego świata M3 (mieszkańcy z `Wealth`, spadki,
/// emigracja), którego ten harness nie buduje — tutaj gospodarstwa są jedynym
/// pieniądzem poza księgami i suma jest zupełna.
#[must_use]
pub fn pieniadz_swiata(w: &World, encje: &[Entity]) -> i64 {
    let ksiegi = w
        .get_resource::<Books>()
        .map_or(0, |b| b.total_balance().get());
    let gd: i64 = encje
        .iter()
        .filter_map(|e| w.get::<Household>(*e))
        .map(|h| h.cash.get() + h.bank.get() + h.savings.get())
        .sum();
    ksiegi + gd
}

/// Komponenty mieszkańca, które trzyma `CitizenView`.
pub struct Buyer {
    pub identity: Identity,
    pub vitals: Vitals,
    pub needs: Needs,
    pub personality: Personality,
    pub residence: Residence,
}

#[must_use]
pub fn buyer(household: u32, status: u8) -> Buyer {
    let identity = Identity {
        household,
        ..Identity::default()
    };
    let vitals = Vitals {
        status,
        ..Vitals::default()
    };
    let mut needs = Needs::default();
    needs.set(NeedKind::Hunger, Q::new(20));
    Buyer {
        identity,
        vitals,
        needs,
        personality: Personality([50; 8]),
        residence: Residence::default(),
    }
}

impl Buyer {
    #[must_use]
    pub fn view(&self, citizen: u32) -> CitizenView<'_> {
        CitizenView {
            id: CitizenId(ent(citizen)),
            identity: &self.identity,
            vitals: &self.vitals,
            needs: &self.needs,
            personality: &self.personality,
            residence: &self.residence,
            today: 0,
        }
    }
}

/// Wiedza mieszkańca: zna wszystkie podane miejsca, każde z oceną 60.
#[must_use]
pub fn knows(places: &[PlaceRef]) -> Vec<Knowledge> {
    places
        .iter()
        .filter_map(|p| {
            Some(Knowledge {
                target: magnat_agents::knowledge_key(*p)?,
                day: 0,
                score: 60,
                kind: KnowledgeKind::Visited as u8,
            })
        })
        .collect()
}

#[must_use]
pub fn view(k: &[Knowledge]) -> KnowledgeView<'_> {
    KnowledgeView::new(k)
}
