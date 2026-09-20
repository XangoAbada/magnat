//! Benchmarki M5a — dwie ścieżki z §7.3 dokumentu fazy.
//!
//! `query_offers`: 5 tys. ofert, promień 3 km, cel **< 20 µs**. To jest budżet na
//! **jedno zapytanie**, a nie na tick: decyzja zakupowa woła je raz na potrzebę,
//! a nie raz na kandydata.
//!
//! `Books::transfer`: budżetu w §7.3 nie ma, bo przelew nie jest ścieżką gorącą sam
//! z siebie — gorące jest `settle_transactions` (M5b, ≈2 tys. intencji na tick
//! szczytowy, < 1,5 ms). Pomiar jest tutaj, żeby ta bramka miała znany składnik:
//! 2 tys. przelewów musi się zmieścić grubo poniżej tamtego budżetu.

use std::hint::black_box;
use std::num::NonZeroU32;

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_core::{
    Arena, DecisionReason, Entity, FirmId, GoodId, Money, Qty, SiteId, StockCat, Tick, Q,
};
use magnat_economy::books::{AccountKind, AccountOwner, Books, TxKind, TxMemo};
use magnat_economy::offer::{query_offers, CategoryId, Offer, OfferId, OfferIndex, PriceBasis};
use magnat_jobs::JobPool;
use magnat_spatial::{Aabb2, GridSpec, Vec2};

/// Metropolia 16 × 16 km, komórka 200 m — ten sam rząd wielkości co siatki M2.
const CITY_M: f32 = 16_000.0;
const OFFERS: u32 = 5_000;

fn city_spec() -> GridSpec {
    GridSpec::covering(
        Aabb2::new(Vec2::new(0.0, 0.0), Vec2::new(CITY_M, CITY_M)),
        200,
    )
}

/// Sklepy rozrzucone po mieście deterministycznie: 71 i 97 są względnie pierwsze
/// z rozmiarem siatki, więc nie tworzą regularnych pasów.
fn pos_of(s: SiteId) -> Vec2 {
    let i = s.entity().index();
    Vec2::new(
        ((i * 71) % 1_600) as f32 * 10.0,
        ((i * 97) % 1_600) as f32 * 10.0,
    )
}

fn build_offers() -> (Arena<Offer>, OfferIndex) {
    let pool = JobPool::new(1);
    let mut arena: Arena<Offer> = Arena::new();
    for i in 0..OFFERS {
        let e = Entity::new(i, NonZeroU32::MIN);
        arena.insert(Offer {
            seller: FirmId(e),
            site: SiteId(e),
            good: GoodId((i % 24) as u16),
            unit_price: Money(150 + i64::from(i % 200)),
            price_basis: PriceBasis::GrossRetail,
            available: Qty(5_000),
            quality: Q::new(50),
            brand: None,
            category: CategoryId::Stock(StockCat::Food),
            since: Tick(0),
            price_rev: 0,
        });
    }
    let mut index = OfferIndex::new(city_spec());
    index.mark_all_dirty();
    index.rebuild(&arena, pos_of, &pool);
    (arena, index)
}

fn bench_query(c: &mut Criterion) {
    let (_arena, index) = build_offers();
    let mut out: Vec<OfferId> = Vec::with_capacity(1024);
    c.bench_function("m5a query_offers 5k ofert, promień 3 km", |b| {
        b.iter(|| {
            query_offers(
                black_box(&index),
                CategoryId::Stock(StockCat::Food),
                Vec2::new(8_000.0, 8_000.0),
                3_000,
                &mut out,
            );
            black_box(out.len())
        });
    });
}

fn bench_rebuild(c: &mut Criterion) {
    let (arena, mut index) = build_offers();
    let pool = JobPool::new(0);
    c.bench_function("m5a rebuild warstwy 5k ofert", |b| {
        b.iter(|| {
            index.mark_all_dirty();
            index.rebuild(black_box(&arena), pos_of, &pool);
        });
    });
}

fn bench_transfer(c: &mut Criterion) {
    let mut books = Books::new();
    let src = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    let dst: Vec<_> = (0..64)
        .map(|_| books.open_account(AccountOwner::City, AccountKind::Current, None, Money::ZERO))
        .collect();
    books.endow(src, Money(1_000_000_000_000), Tick(0)).unwrap();
    let memo = TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified);
    let mut i = 0usize;
    c.bench_function("m5a Books::transfer", |b| {
        b.iter(|| {
            i = (i + 1) % dst.len();
            black_box(
                books
                    .transfer(src, dst[i], Money(100), memo, Tick(1))
                    .unwrap(),
            )
        });
    });
}

/// Ścieżki gorące M5b (§7.3): sama funkcja użyteczności i cała decyzja zakupowa.
///
/// Budżety z dokumentu fazy: `utility_of_offer` < 120 ns na kandydata,
/// `purchase_decision` ≤ 3 ms na tick minutowy w szczycie. Pomiar jest tutaj,
/// a nie w scenariuszu, bo w scenariuszu koszt zakupów miesza się z kosztem
/// **podróży**, które te zakupy generują — a te należą do routera M4.
fn bench_uzytecznosc(c: &mut Criterion) {
    use magnat_economy::{
        offer_noise, utility_of_offer, weights_for, BuyerState, Candidate, EconomyData, OfferId,
    };
    let data = EconomyData::load_default().expect("data/economy/");
    let p = magnat_agents::Personality([50; 8]);
    let w = weights_for(&p, Q::new(50), magnat_core::NeedKind::Hunger, &data);
    let st = BuyerState {
        status: Q::new(50),
        openness: Q::new(50),
        budget_ref: Money(1_500),
        vot_gr_per_min: 12,
        brands: Default::default(),
    };
    let kandydaci: Vec<Candidate> = (0..15u32)
        .map(|i| Candidate {
            offer: OfferId::from_bits((1u64 << 32) | u64::from(i)).unwrap(),
            site: SiteId(Entity::new(i, NonZeroU32::MIN)),
            good: GoodId(1),
            qty: Qty(1_000),
            price_total: Money(200 + i64::from(i) * 7),
            travel_min: (4 + i % 11) as u16,
            travel_money: Money::ZERO,
            quality: Q::new(50 + (i % 40) as u8),
            brand: None,
            rating: Some(60),
            visited: i % 3 == 0,
        })
        .collect();

    c.bench_function("m5b utility_of_offer", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % kandydaci.len();
            let n = offer_noise(7, 42, kandydaci[i].offer, Tick(100), 0.02);
            black_box(utility_of_offer(&kandydaci[i], &w, &st, n))
        });
    });

    c.bench_function("m5b wycena 15 kandydatów", |b| {
        b.iter(|| {
            let mut s = 0.0f64;
            for k in &kandydaci {
                let n = offer_noise(7, 42, k.offer, Tick(100), 0.02);
                s += utility_of_offer(k, &w, &st, n);
            }
            black_box(s)
        });
    });
}

/// Dobowy przelot sklepu (M5c §7.3): `reprice` dla 2 tys. sklepów × asortyment,
/// budżet **< 40 ms raz na dobę**, i obserwacja konkurencji, która go poprzedza.
///
/// Katalog detaliczny M5 ma 18 towarów, więc przy pełnej półce wychodzi 2 000 × 18
/// sterowników zamiast 2 000 × 40 z dokumentu fazy. Budżet skaluje się liniowo
/// z liczbą sterowników, więc pomiar czyta się jako „połowa budżetu na połowie
/// asortymentu" — rozmiar katalogu podnosi M6, nie M5.
fn bench_doba_sklepu(c: &mut Criterion) {
    use magnat_agents::{NeedTable, PlaceEntry, PlaceTable};
    use magnat_core::{PlaceKind, PlaceRef, WorldCoord};
    use magnat_economy::{AccountKind, AccountOwner, EconomyData, GoodTable, Market, ShopSeed};
    use std::sync::Arc;

    const SKLEPOW: u32 = 2_000;

    let data = EconomyData::load_default().expect("data/economy/");
    let klucze: Vec<String> = data.retail.goods.iter().map(|g| g.key.clone()).collect();
    let goods = GoodTable::build(&data.retail, |k| {
        let i = klucze.iter().position(|x| x == k)?;
        Some((GoodId(i as u16), Money(200), Qty(250)))
    });
    let needs = Arc::new(NeedTable::load_default().expect("data/needs/"));

    let sites: Vec<SiteId> = (0..SKLEPOW)
        .map(|i| SiteId(Entity::new(i, NonZeroU32::MIN)))
        .collect();
    let places = Arc::new(PlaceTable::build(
        sites
            .iter()
            .map(|s| {
                let p = pos_of(*s);
                PlaceEntry {
                    place: PlaceRef::Site(*s),
                    kind: PlaceKind::Grocery,
                    at: WorldCoord::new((p.x * 100.0) as i32, (p.y * 100.0) as i32, 0),
                }
            })
            .collect(),
    ));

    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    books
        .endow(rest, Money(1_000_000_000_000), Tick(0))
        .unwrap();
    let market = Market::new(
        city_spec(),
        7,
        data,
        goods,
        lancuch_testowy(),
        needs,
        places,
        rest,
    );
    for (i, s) in sites.iter().enumerate() {
        let firm = FirmId(Entity::new(i as u32, NonZeroU32::MIN));
        let acc = books.open_account(
            AccountOwner::Firm(firm),
            AccountKind::Current,
            None,
            Money::ZERO,
        );
        assert!(market.open_shop(
            ShopSeed {
                site: *s,
                firm,
                pos: pos_of(*s),
                kind: PlaceKind::Grocery,
                shelf_slots: 18,
                capacity_m3: 400,
                district: 0,
            },
            acc,
            Tick(0),
        ));
    }
    market.stock_initial(&mut books, Tick(0));
    market.rebuild_index(&JobPool::new(0));

    let mut doba = 0u64;
    c.bench_function("m5c reprice 2000 sklepów", |b| {
        b.iter(|| {
            doba += 1;
            black_box(market.reprice_all(Tick(doba * 1_440)))
        });
    });

    let mut doba2 = 10_000u64;
    c.bench_function("m5c observe_competitors 2000 sklepów", |b| {
        b.iter(|| {
            doba2 += 8;
            black_box(market.observe_competitors(Tick(doba2 * 1_440)))
        });
    });

    // ── M5e ──────────────────────────────────────────────────────────────────
    //
    // Dwie **nowe dobowe ścieżki**, obie mierzone osobno i obie z tego samego
    // powodu, dla którego `observe_competitors` dostał własną linię budżetu
    // (`X-5`): rosną z czego innego niż `reprice`. Migawka panelu rośnie
    // z liczby linii **jednego** sklepu, a próbka balansatora z liczby
    // **wszystkich** sklepów — i to ona jest tą, która wejdzie pod bramkę
    // czasu profilu `ci` (8 seedów × 365 dób < 10 min).
    market.set_tracking(sites[0], magnat_economy::LostSaleTracking::Full);
    c.bench_function("m5e shop_panel migawka jednego sklepu", |b| {
        b.iter(|| black_box(market.shop_panel(sites[0], Tick(0), Tick(doba2 * 1_440))));
    });

    c.bench_function("m5e balance_sample 2000 sklepów", |b| {
        b.iter(|| black_box(market.balance_sample()));
    });
}

/// Decyzja zakupowa **bez** podróży, które ona generuje (`U-24`).
///
/// Budżet §7.3 mówi o `purchase_decision`, a scenariusz `m5shop` pokazał wzrost
/// czasu doby z 0,6 s do ~10 s po uruchomieniu zakupów. Różnica nie jest w tej
/// funkcji — `utility_of_offer` mierzy kilkanaście nanosekund wobec budżetu 120 ns
/// — tylko w routerze M4: każdy zakup to dwa wywołania `begin_trip`. Bramka
/// benchmarkowa fazy **musi** to rozdzielać, inaczej czerwieni się na koszcie
/// cudzego modułu i nikt nie wie, co naprawiać.
///
/// Ten benchmark woła `candidates` na rynku z **atrapą podróży**: `PlaceProvider`
/// nie dostaje `TravelOracle` (`U-23`), więc mierzy się tu dokładnie tyle, ile
/// kosztuje wybór oferty — zebranie kandydatów, użyteczność, softmax i zapis planu.
fn bench_decyzja_zakupowa(c: &mut Criterion) {
    use magnat_agents::{
        ArrayVec, Identity, Knowledge, KnowledgeKind, KnowledgeView, NeedTable, Needs, Personality,
        PlaceCandidate, PlaceEntry, PlaceProvider, PlaceTable, Residence, Vitals, MAX_CANDIDATES,
    };
    use magnat_core::{CitizenId, NeedKind, PlaceKind, PlaceRef, WorldCoord, Q};
    use magnat_economy::{AccountKind, AccountOwner, EconomyData, GoodTable, Market, ShopSeed};
    use std::sync::Arc;

    // Tyle sklepów, ile mieści się w promieniu zapytania z `choice.ron` w mieście
    // 150 tys. — dalsze i tak odpadają przed wyceną.
    const SKLEPOW: u32 = 40;

    let data = EconomyData::load_default().expect("data/economy/");
    let klucze: Vec<String> = data.retail.goods.iter().map(|g| g.key.clone()).collect();
    let goods = GoodTable::build(&data.retail, |k| {
        let i = klucze.iter().position(|x| x == k)?;
        Some((GoodId(i as u16), Money(200), Qty(250)))
    });
    let needs = Arc::new(NeedTable::load_default().expect("data/needs/"));
    let dom = PlaceRef::Building(magnat_core::BuildingId(Entity::new(9_999, NonZeroU32::MIN)));
    let sites: Vec<SiteId> = (0..SKLEPOW)
        .map(|i| SiteId(Entity::new(i, NonZeroU32::MIN)))
        .collect();
    let mut wpisy = vec![PlaceEntry {
        place: dom,
        kind: PlaceKind::Home,
        at: WorldCoord::new(0, 0, 0),
    }];
    wpisy.extend(sites.iter().map(|s| {
        let p = pos_of(*s);
        PlaceEntry {
            place: PlaceRef::Site(*s),
            kind: PlaceKind::Grocery,
            at: WorldCoord::new((p.x * 100.0) as i32, (p.y * 100.0) as i32, 0),
        }
    }));
    let places = Arc::new(PlaceTable::build(wpisy));

    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    books
        .endow(rest, Money(1_000_000_000_000), Tick(0))
        .unwrap();
    let market = Market::new(
        city_spec(),
        7,
        data,
        goods,
        lancuch_testowy(),
        needs,
        places,
        rest,
    );
    for (i, s) in sites.iter().enumerate() {
        let firm = FirmId(Entity::new(i as u32, NonZeroU32::MIN));
        let acc = books.open_account(
            AccountOwner::Firm(firm),
            AccountKind::Current,
            None,
            Money::ZERO,
        );
        assert!(market.open_shop(
            ShopSeed {
                site: *s,
                firm,
                pos: pos_of(*s),
                kind: PlaceKind::Grocery,
                shelf_slots: 18,
                capacity_m3: 400,
                district: (i % 8) as u16,
            },
            acc,
            Tick(0),
        ));
    }
    market.stock_initial(&mut books, Tick(0));
    market.rebuild_index(&JobPool::new(0));

    let wiedza: Vec<Knowledge> = std::iter::once(dom)
        .chain(sites.iter().map(|s| PlaceRef::Site(*s)))
        .filter_map(|p| {
            Some(Knowledge {
                target: magnat_agents::knowledge_key(p)?,
                day: 0,
                score: 60,
                kind: KnowledgeKind::Visited as u8,
            })
        })
        .collect();
    let identity = Identity::default();
    let vitals = Vitals {
        status: 50,
        ..Vitals::default()
    };
    let mut potrzeby = Needs::default();
    potrzeby.set(NeedKind::Hunger, Q::new(20));
    let osobowosc = Personality([50; 8]);
    let mieszkanie = Residence::default();

    let mut i = 0u32;
    c.bench_function("m5e decyzja zakupowa bez podróży", |b| {
        b.iter(|| {
            i += 1;
            let mut out: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
            market.candidates(
                NeedKind::Hunger,
                dom,
                60,
                &KnowledgeView::new(&wiedza),
                &magnat_agents::CitizenView {
                    id: CitizenId(Entity::new(i, NonZeroU32::MIN)),
                    identity: &identity,
                    vitals: &vitals,
                    needs: &potrzeby,
                    personality: &osobowosc,
                    residence: &mieszkanie,
                    today: 0,
                    brands: Default::default(),
                },
                &mut out,
            );
            black_box(out.len())
        });
    });
}

criterion_group!(
    benches,
    bench_query,
    bench_rebuild,
    bench_transfer,
    bench_uzytecznosc,
    bench_doba_sklepu,
    bench_decyzja_zakupowa
);
criterion_main!(benches);

/// Łańcuch dostaw dla przebiegów testowych: katalog z `data/`, stawka ryczałtowa
/// i jedna brama towarowa o dużej przepustowości.
///
/// Brama jest tu po to, żeby sklep miał **skąd** wziąć towar: od WP11 zapas leży
/// w magazynie M6, a magazyn zapełnia dostawa. Przepustowość jest szeroka z rozmysłu —
/// test warstwy detalicznej nie ma się wywracać na kolejce na granicy; od badania
/// kolejki jest `import_not_free` po stronie `sim/supply`.
fn lancuch_testowy() -> magnat_supply::ChainHandle {
    let cat =
        std::sync::Arc::new(magnat_supply::load_default("contemporary").expect("katalog z data/"));
    let tuning =
        std::sync::Arc::new(magnat_supply::Tuning::load_default().expect("data/tuning/supply.ron"));
    let oracle: std::sync::Arc<dyn magnat_supply::FreightOracle> =
        std::sync::Arc::new(magnat_supply::FlatRateFreight {
            km: 5,
            tuning: tuning.transport,
            blocked: Vec::new(),
        });
    magnat_supply::ChainHandle::with_import_gate(
        cat,
        tuning,
        oracle,
        magnat_supply::TariffTable::load_default().expect("data/trade/tariffs.ron"),
        7,
        magnat_core::SiteId(magnat_core::Entity::new(
            u32::MAX - 2,
            std::num::NonZeroU32::MIN,
        )),
        magnat_core::Mass(1_000_000_000_000),
        60,
    )
}
