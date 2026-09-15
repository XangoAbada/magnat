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
use magnat_core::{
    BuildingId, CitizenId, DecisionReason, Entity, FirmId, GoodId, Money, NeedKind, PlaceKind,
    PlaceRef, Q, Qty, SiteId, StockCat, Tick, WorldCoord,
};
use magnat_economy::{
    AccountId, AccountKind, AccountOwner, Books, EconomyData, GoodTable, Market, ShopSeed, TxKind,
    TxMemo,
};
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
    let sites: Vec<SiteId> = (0..pos.len() as u32).map(|i| SiteId(ent(100 + i))).collect();
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
