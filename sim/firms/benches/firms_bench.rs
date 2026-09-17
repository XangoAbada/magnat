//! Budżet decyzji firmy (M7e WP11, M7 §7.6).
//!
//! §5.6 mówi, że **budżet obliczeniowy wyraża się w liczbie firm na tick, nigdy
//! w czasie rzeczywistym** — i tak jest egzekwowany w symulacji (`Scheduler`,
//! `Tier::max_per_tick`, test `scheduling.rs`). Czas mierzy się **tutaj**, bo zegar
//! ścienny jest niedeterministyczny i nie ma prawa sterować tym, ile decyzji zapadnie.
//!
//! Cel z §7.6: `ai::operational` ≤ 5 µs na firmę na jednym rdzeniu. Przy 10 tys. firm
//! i jednej decyzji operacyjnej na dobę daje to 50 ms na dobę gry.

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_core::{DistrictId, Entity, FirmStrategy, GoodId, Money, SiteId, Tick};
use magnat_firms::view::{CityFacts, FirmView, GoodFacts, SiteFacts};
use magnat_firms::{decide_operational, decide_tactical, FirmKey, FirmPersonality};
use std::num::NonZeroU32;

fn site(i: u32) -> SiteId {
    SiteId(Entity::new(i, NonZeroU32::MIN))
}

/// Firma o rozmiarze typowym dla miasta: jeden zakład, osiem towarów na półce.
fn towary() -> Vec<GoodFacts> {
    (0..8u16)
        .map(|i| GoodFacts {
            site: site(1),
            good: GoodId(i),
            own_price_net: Money(500 + i64::from(i) * 7),
            unit_cost_net: Money(400),
            margin_bp: 2_500,
            stock_days: 3 + i32::from(i),
            restock_days: 7,
            rival_cheapest_net: Some(Money(460 + i64::from(i))),
            rival_cheapest_site: Some(site(2)),
            rival_median_net: Some(Money(500)),
            rivals: 3,
            rivals_before: 3,
            sold_7d: 700,
            sold_7d_prev: 700,
        })
        .collect()
}

fn zaklady() -> Vec<SiteFacts> {
    vec![SiteFacts {
        site: site(1),
        district: DistrictId(0),
        vacancies: 1,
        headcount: 6,
        months_in_loss: 0,
        last_margin_bp: Some(1_200),
        delegated: false,
        needs_policy: false,
        scarcest_role: None,
    }]
}

fn widok<'a>(sites: &'a [SiteFacts], goods: &'a [GoodFacts]) -> FirmView<'a> {
    FirmView {
        key: FirmKey(1),
        cash: Money(5_000_000),
        personality: FirmPersonality::NEUTRAL,
        strategy: FirmStrategy::Cautious,
        margin_floor_bp: 1_000,
        margin_ceiling_bp: 4_000,
        lag_days: 3,
        tick: Tick(0),
        sites,
        goods,
        city: CityFacts::default(),
    }
}

fn bench(c: &mut Criterion) {
    let s = zaklady();
    let g = towary();
    let v = widok(&s, &g);
    c.bench_function("b-m7e ai/decide_operational (8 towarów)", |b| {
        b.iter(|| std::hint::black_box(decide_operational(std::hint::black_box(&v))));
    });
    c.bench_function("b-m7e ai/decide_tactical (1 zakład)", |b| {
        b.iter(|| std::hint::black_box(decide_tactical(std::hint::black_box(&v))));
    });
}

criterion_group!(firms, bench);
criterion_main!(firms);
