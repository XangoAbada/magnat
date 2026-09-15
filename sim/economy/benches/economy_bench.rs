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

criterion_group!(benches, bench_query, bench_rebuild, bench_transfer);
criterion_main!(benches);
