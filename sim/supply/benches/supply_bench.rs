//! Benchmarki M6 — sześć ścieżek z §7.4 dokumentu fazy (WP14).
//!
//! **Po co, skoro `AG-7` i `AJ-5` już nazwały sufity.** Bo nazwany sufit to teza,
//! a nie pomiar. `AG-7` zapisało, że `advance_production` chodzi pętlą minuta po
//! minucie i że przy makro z krokiem dobowym to 1 440 obrotów na zakład; `AJ-5`,
//! że `SellerIndex` przebudowuje się w całości. Obie tezy są prawdziwe i obie mogą
//! być nieistotne — a rozstrzyga to liczba, nie rozumowanie. `AO-5` mówi wprost:
//! dzisiejszy brak sygnału nie jest pomiarem i nie wolno go czytać jako „mieści się
//! w budżecie".
//!
//! Skala jest ta z §7.4 (metropolia 400 tys.), budżety też:
//!
//! | Benchmark | Skala | Budżet |
//! |---|---|---|
//! | `production_2400_lines` | 2 400 linii, 1 minuta | 0,5 ms |
//! | `dock_queue_1200` | 1 200 kolejek ramp | 0,2 ms |
//! | `replenish_9000_sites` | 9 000 zakładów, godzina **rozproszona** | 12 ms / 60 |
//! | `rfq_600_open` | 600 otwartych zapytań | mieści się w 12 ms godziny |
//! | `coalesce_600k_batches` | 600 tys. partii, doba **rozproszona** | 40 ms / 1440 |
//! | `trace_batch_depth12` | ślad o dwunastu przodkach | poniżej klatki |
//!
//! Pomiar rozproszenia jest tu istotą, a nie szczegółem: `Chain::step_hour` wołane
//! co minutę robi **jedną sześćdziesiątą** pracy, więc benchmark liczący całą godzinę
//! naraz mierzyłby kod, który w grze nigdy się nie wykonuje.

use std::hint::black_box;
use std::num::NonZeroU32;

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_core::{
    Energy, Entity, FirmId, GoodId, Mass, Money, OpenHours, SimMinute, SiteId, Volume, Q,
};
use magnat_supply::batch::TransportOrderId;
use magnat_supply::catalog::load_default;
use magnat_supply::plant::{Dock, PlantSite, ProductionCtx, VehicleArrivedAtSite};
use magnat_supply::store::{BatchDraft, MassIn, WarehouseRole};
use magnat_supply::{
    advance_production, BatchFlags, BatchOrigin, Catalog, NoDeposits, Plant, ProductionLine,
    StorageClass, Store, Tuning,
};

/// Skala z §7.4. Mniejsza od metropolii tam, gdzie pomiar jest liniowy i da się
/// przeskalować w głowie — a nie tam, gdzie koszt rośnie inaczej niż liniowo.
const LINIE: u32 = 2_400;
const RAMPY: u32 = 1_200;
const ZAKLADY: u32 = 9_000;
const PARTIE: u32 = 600_000;
/// ~12 000 nowych zleceń na dobę gry (§7.4) to około ośmiu na tick ekonomiczny.
const PRZYJAZDY_NA_TICK: u32 = 8;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

fn dane() -> (Catalog, Tuning) {
    (
        load_default("contemporary").expect("katalog z data/"),
        Tuning::load_default().expect("data/tuning/supply.ron"),
    )
}

/// Miasto z `n` młynami: jedna linia, magazyn pełen zboża, trzy zmiany.
fn mlyny(cat: &Catalog, n: u32) -> (Store, Plant) {
    let mut store = Store::new(cat.goods.len());
    let mut plant = Plant::new();
    let zboze = cat.good_id("raw_wheat").expect("raw_wheat");
    let przemial = cat
        .recipe_id("milling_wheat_t550")
        .expect("milling_wheat_t550");
    let czesc = cat.good_id("part_bearing_6204").expect("część");
    let klasa = cat.machine_class_id("mill_roller").expect("klasa maszyny");

    for i in 0..n {
        let site = SiteId(encja(i + 1));
        let owner = FirmId(encja(100_000 + i));
        let we = store.add_slot(
            site,
            WarehouseRole::Input,
            StorageClass::Silo,
            Mass(i64::MAX / 16),
            Volume(i64::MAX / 16),
            0xFF,
        );
        let wy = store.add_slot(
            site,
            WarehouseRole::Output,
            StorageClass::Dry,
            Mass(i64::MAX / 16),
            Volume(i64::MAX / 16),
            0xFF,
        );
        store
            .put(
                cat,
                we,
                BatchDraft {
                    good: zboze,
                    mass: Mass(200_000_000),
                    quality: Q::new(64),
                    brand: None,
                    producer: owner,
                    produced_at: SimMinute(0),
                    cost: Money(15_600_000),
                    origin: BatchOrigin::default(),
                    flags: BatchFlags::default(),
                },
                MassIn::Initial,
            )
            .expect("zboże do silosu");
        let mut z = PlantSite::new(site, owner, Dock::new(2, 25, 1, OpenHours::ALWAYS));
        z.inputs.push(we);
        z.outputs.push(wy);
        let mut l = ProductionLine::new(klasa, Mass(3_000_000), Energy(55), czesc);
        l.recipe = Some(przemial);
        l.next_maintenance = SimMinute(u64::MAX);
        z.lines.push(l);
        plant.insert(z);
    }
    (store, plant)
}

/// `bench_production_2400_lines` — minuta produkcji całej metropolii.
///
/// To jest pomiar sufitu z `AG-7`: `advance_production` chodzi pętlą minuta po
/// minucie. Przy kroku minutowym to jedna iteracja na linię i tego dotyczy budżet
/// 0,5 ms z §7.4.
fn production(c: &mut Criterion) {
    let (cat, tuning) = dane();
    let (mut store, mut plant) = mlyny(&cat, LINIE);
    let sites: Vec<SiteId> = plant.sites().collect();
    let mut minuta = 0u64;
    c.bench_function("production_2400_lines", |b| {
        b.iter(|| {
            let ctx = ProductionCtx {
                cat: &cat,
                tuning: &tuning,
                world_seed: 7,
                deposits: &NoDeposits,
            };
            for s in &sites {
                advance_production(&ctx, &mut store, &mut plant, *s, SimMinute(minuta), 1);
            }
            minuta += 1;
            black_box(store.live_batches())
        });
    });
}

/// `bench_production_lod_step` — ta sama doba policzona raz, nie 1 440 razy.
///
/// Drugi pomiar tego samego sufitu, i ten jest ważniejszy: `AG-7` mówi, że przy makro
/// z krokiem dobowym `advance_production` robi 1 440 obrotów **na zakład**. Ta liczba
/// mówi, czy droga wyjścia (skok do najbliższego zdarzenia) jest w ogóle potrzebna.
fn production_lod(c: &mut Criterion) {
    let (cat, tuning) = dane();
    let (mut store, mut plant) = mlyny(&cat, 100);
    let sites: Vec<SiteId> = plant.sites().collect();
    let mut doba = 0u64;
    c.bench_function("production_lod_step_day", |b| {
        b.iter(|| {
            let ctx = ProductionCtx {
                cat: &cat,
                tuning: &tuning,
                world_seed: 7,
                deposits: &NoDeposits,
            };
            for s in &sites {
                advance_production(
                    &ctx,
                    &mut store,
                    &mut plant,
                    *s,
                    SimMinute(doba * 1_440),
                    1_440,
                );
            }
            doba += 1;
            black_box(store.live_batches())
        });
    });
}

/// `bench_dock_queue_1200` — minuta systemu ramp przy skali metropolii.
///
/// **Tempo przyjazdów jest tu treścią pomiaru, nie tłem.** §7.4 podaje 1 200 aktywnych
/// kolejek i ~12 000 nowych zleceń na dobę gry, czyli **około ośmiu na tick** — a nie
/// jednego na każdą rampę co minutę. Pierwsza wersja tego benchmarku mierzyła to
/// drugie i dawała 4,6 ms wobec budżetu 0,2 ms; obciążenie było jednak nie do
/// osiągnięcia w grze, bo oznaczało rampę trwale przeciążoną czterdziestokrotnie.
/// Liczba była prawdziwa i nieistotna, czyli najgorszy rodzaj liczby w benchmarku.
fn dock_queue(c: &mut Criterion) {
    let (_, tuning) = dane();
    let mut rampy: Vec<Dock> = (0..RAMPY)
        .map(|_| Dock::new(3, 12, 1, OpenHours::ALWAYS))
        .collect();
    let mut minuta = 0u64;
    let mut kolejny = 0usize;
    c.bench_function("dock_queue_1200", |b| {
        b.iter(|| {
            for _ in 0..PRZYJAZDY_NA_TICK {
                kolejny = (kolejny + 1) % rampy.len();
                let r = rampy[kolejny].arrive(
                    &VehicleArrivedAtSite {
                        vehicle: magnat_core::VehicleId(encja(kolejny as u32)),
                        site: SiteId(encja(kolejny as u32)),
                        order: TransportOrderId(kolejny as u32),
                        at: SimMinute(minuta),
                        mass: Mass(12_000_000),
                    },
                    &tuning.dock,
                );
                black_box(r.release_at);
            }
            minuta += 1;
        });
    });
}

/// Rampa **trwale przeciążona** — pomiar sufitu, nie budżetu.
///
/// Kolejka rampy jest wektorem przeglądanym w całości przy każdym przyjeździe
/// (`prune`, `occupancy`, wstawienie z zachowaniem porządku). Dopóki pojazdy zjeżdżają
/// szybciej, niż przyjeżdżają, kolejka jest krótka i koszt jest stały. Rampa, do której
/// przyjazdy są **trwale** częstsze niż obsługa, rośnie bez ograniczenia i koszt staje
/// się kwadratowy — a `yard_capacity` tego nie zatrzymuje, bo wpływa na `on_street`,
/// a nie na wpis w kolejce.
///
/// `ponytail:` sufit nazwany i zmierzony, bez zmiany zachowania. Droga wyjścia to
/// odmowa przyjęcia pojazdu ponad `bays + yard_capacity + rezerwa` — ale to jest zmiana
/// kontraktu z M4 (`SiteDwellResponse` musiałby umieć powiedzieć „odjedź i wróć"),
/// więc należy do fazy, która ten kontrakt otwiera, a nie do benchmarku.
fn dock_saturated(c: &mut Criterion) {
    let (_, tuning) = dane();
    let mut minuta = 0u64;
    c.bench_function("dock_queue_saturated_100", |b| {
        b.iter_batched(
            || Dock::new(1, 12, 1, OpenHours::ALWAYS),
            |mut d| {
                for i in 0..100u32 {
                    minuta += 1;
                    black_box(d.arrive(
                        &VehicleArrivedAtSite {
                            vehicle: magnat_core::VehicleId(encja(i)),
                            site: SiteId(encja(1)),
                            order: TransportOrderId(i),
                            at: SimMinute(minuta),
                            mass: Mass(12_000_000),
                        },
                        &tuning.dock,
                    ));
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// `bench_replenish_9000_sites` — godzina przeglądu zapasów **po rozproszeniu**.
///
/// Mierzone jest to, co naprawdę biegnie w ticku: jedna faza z sześćdziesięciu.
/// Budżet §7.4 na systemy godzinowe to 12 ms na całą godzinę, czyli 0,2 ms na fazę.
fn replenish(c: &mut Criterion) {
    use magnat_supply::{B2b, Chain, FlatRateFreight, InventoryRule, TariffTable};

    let (cat, tuning) = dane();
    let (store, plant) = mlyny(&cat, ZAKLADY);
    let zboze = cat.good_id("raw_wheat").expect("raw_wheat");
    let mut chain = Chain::new(
        cat.goods.len(),
        B2b::new(
            cat.goods.len(),
            TariffTable::load_default().expect("data/trade/tariffs.ron"),
            7,
        ),
    );
    chain.store = store;
    chain.plant = plant;
    for i in 0..ZAKLADY {
        chain.set_rules(
            SiteId(encja(i + 1)),
            vec![InventoryRule::continuous(zboze, 480, 240)],
        );
    }
    let oracle = FlatRateFreight {
        km: 8,
        tuning: tuning.transport,
        blocked: Vec::new(),
    };
    let mut minuta = 0u64;
    c.bench_function("replenish_9000_sites_one_phase", |b| {
        b.iter(|| {
            // Minuta **niebędąca** granicą godziny: mierzymy samo rozproszenie,
            // bez rozstrzygania rynku, które ma własny benchmark.
            minuta += 1;
            if minuta.is_multiple_of(60) {
                minuta += 1;
            }
            black_box(
                chain
                    .step_hour(&cat, &tuning, &oracle, SimMinute(minuta))
                    .len(),
            )
        });
    });
}

/// `bench_rfq_600_open` — przebudowa indeksu dostawców przy skali metropolii.
///
/// Sufit nazwany w `AJ-5`: `SellerIndex::rebuild` przechodzi po **wszystkich** liniach
/// wszystkich zakładów i robi to raz na dobę. 2 400 linii to rząd 10⁴ operacji na dobę
/// gry — teza mówi, że nie ma czego optymalizować, a to jest liczba, która to rozstrzyga.
fn rfq(c: &mut Criterion) {
    use magnat_supply::{B2b, TariffTable};

    let (cat, _) = dane();
    let (_, plant) = mlyny(&cat, LINIE);
    let mut b2b = B2b::new(
        cat.goods.len(),
        TariffTable::load_default().expect("data/trade/tariffs.ron"),
        7,
    );
    c.bench_function("rfq_seller_index_rebuild", |b| {
        b.iter(|| {
            b2b.reindex(&cat, &plant);
            black_box(b2b.sellers_of(GoodId(0)).len())
        });
    });
}

/// `bench_coalesce_600k_batches` — scalanie partii **po rozproszeniu** i pomiar pamięci.
///
/// Kryterium WP15 jest dwuczęściowe i obie części są tutaj: 600 tys. partii ma się
/// zmieścić w 64 MB pamięci gorącej, a scalanie ma być amortyzowane po dobie.
/// Rozmiar partii liczy `size_of`, a nie szacunek z §7.4 — szacunek mówił 88 B.
fn coalesce(c: &mut Criterion) {
    let (cat, _) = dane();
    let mut store = Store::new(cat.goods.len());
    let chleb = cat.good_id("food_bread_wheat").expect("chleb");
    // 1 200 slotów po 500 partii: rozkład bliski metropolii (4 500 sklepów × ~80
    // partii po agregacji), przy rozmiarze, który mieści się w pamięci runnera.
    let sloty: Vec<_> = (0..1_200u32)
        .map(|i| {
            store.add_slot(
                SiteId(encja(i)),
                WarehouseRole::Shelf,
                StorageClass::Ambient,
                Mass(i64::MAX / 32),
                Volume(i64::MAX / 32),
                0,
            )
        })
        .collect();
    let na_slot = PARTIE / 1_200;
    for (i, s) in sloty.iter().enumerate() {
        for j in 0..na_slot {
            // Jakość w kubełkach po pięć i data co dobę — klucz agregacji ma mieć
            // co scalać, ale nie wszystko naraz.
            let _ = store.put(
                &cat,
                *s,
                BatchDraft {
                    good: chleb,
                    mass: Mass(1_000 + i64::from(j % 7)),
                    quality: Q::new(40 + (j % 40) as u8),
                    brand: None,
                    producer: FirmId(encja(i as u32 % 50)),
                    produced_at: SimMinute(u64::from(j % 3) * 1_440),
                    cost: Money(i64::from(j) + 100),
                    origin: BatchOrigin::default(),
                    flags: BatchFlags::default(),
                },
                MassIn::Initial,
            );
        }
    }
    let bajty = store.arena_slots() * std::mem::size_of::<magnat_supply::Batch>();
    eprintln!(
        "coalesce: {} partii, {} slotów areny, {:.1} MB pamięci gorącej ({} B na partię)",
        store.live_batches(),
        store.arena_slots(),
        bajty as f64 / (1024.0 * 1024.0),
        std::mem::size_of::<magnat_supply::Batch>()
    );

    let mut faza = 0u32;
    c.bench_function("coalesce_600k_batches_one_phase", |b| {
        b.iter(|| {
            faza = (faza + 1) % 1_440;
            black_box(store.coalesce_phase(faza))
        });
    });
}

/// `bench_trace_batch_depth12` — ślad o dwunastu przodkach.
///
/// Sufit nazwany w `BatchLedger::trace`: przeszukanie jest liniowe po całym dzienniku.
/// Do dziennika trafiają wyłącznie partie `TRACED`, więc teza brzmi „nie ma czego
/// optymalizować" — a to jest liczba, która ją sprawdza.
fn trace(c: &mut Criterion) {
    let (cat, _) = dane();
    let mut store = Store::new(cat.goods.len());
    let site = SiteId(encja(1));
    let slot = store.add_slot(
        site,
        WarehouseRole::Input,
        StorageClass::Dry,
        Mass(i64::MAX / 16),
        Volume(i64::MAX / 16),
        0xFF,
    );
    let maka = cat.good_id("food_flour_t550").expect("mąka");
    // Dwanaście pokoleń partii, każde z własnym etapem w dzienniku, plus tło
    // z tysiąca innych partii śledzonych — bo to ono decyduje o koszcie skanu.
    let mut ostatnia = None;
    for i in 0..1_012u32 {
        store.set_now(SimMinute(u64::from(i)));
        let mut flags = BatchFlags::default();
        flags.set(BatchFlags::TRACED);
        let id = store
            .put(
                &cat,
                slot,
                BatchDraft {
                    good: maka,
                    mass: Mass(10_000),
                    quality: Q::new(60),
                    brand: None,
                    producer: FirmId(encja(1)),
                    produced_at: SimMinute(u64::from(i)),
                    cost: Money(10_000),
                    origin: BatchOrigin {
                        site: Some(site),
                        recipe: None,
                        depth: (i % 12) as u8,
                        deposit: None,
                    },
                    flags,
                },
                MassIn::Produced,
            )
            .expect("partia");
        ostatnia = Some(id);
    }
    let id = ostatnia.expect("co najmniej jedna partia");
    c.bench_function("trace_batch_depth12", |b| {
        b.iter(|| black_box(magnat_supply::trace_batch(&store, id).stages.len()));
    });
}

criterion_group!(
    benches,
    production,
    production_lod,
    dock_queue,
    dock_saturated,
    replenish,
    rfq,
    coalesce,
    trace
);
criterion_main!(benches);
