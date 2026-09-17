//! Budżet sieci przesyłowych — test T8b z §7 dokumentu M8.
//!
//! Cel: **< 0,3 ms na tick dla wszystkich pięciu sieci łącznie** (typowo, jedna
//! runda) i **< 2 ms** w kaskadzie ośmiorundowej. Skala z §17.7: metropolia
//! 400 tys. mieszkańców to ~5000 węzłów i ~6000 krawędzi w sieci energetycznej.
//!
//! Mierzymy **minutę świata**, a nie jedno wywołanie: `solve` woła się raz na
//! sieć na tick, więc grupa liczy pięć sieci naraz — tak jak liczy je scheduler.
//!
//! Przypadek typowy jest przy tym mierzony uczciwie: topologia **nie jest**
//! przebudowywana co tick, bo w grze przebudowuje się kilka razy na dobę.
//! Dlatego scena rozgrzewa się jednym krokiem, zanim zacznie się pomiar.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use magnat_core::{Entity, FirmId, Money, Tick, UtilityService};
use magnat_traffic::utility::solve::{ProfileTable, RepairWindow};
use magnat_traffic::utility::{
    LoadProfile, Tariff, UtilityEdge, UtilityNetwork, UtilityNode, MAX_CASCADE_ROUNDS,
};
use std::hint::black_box;
use std::num::NonZeroU32;

const SEED: u64 = 0x005E_ED8B;
/// Stacje w pierścieniu magistralnym — jedna na dzielnicę metropolii.
const STACJI: u32 = 200;
/// Przyłącza na stację. 200 × 24 = 4800 odbiorców, razem ~5000 węzłów.
const PRZYLACZY: u32 = 24;
const SIECI: usize = 5;

fn taryfa() -> Tariff {
    Tariff {
        standing_charge_per_month: Money(4_500),
        per_unit: Money(65),
    }
}

/// Sieć metropolii: źródło, pierścień magistralny stacji i promieniste przyłącza.
///
/// `ciasno` zwęża przepustowość odgałęzień tak, żeby każda runda wywalała kolejne
/// — służy wyłącznie zmierzeniu górnego kosztu kaskady.
fn metropolia(service: UtilityService, ciasno: bool) -> UtilityNetwork {
    let mut nodes = vec![UtilityNode::source(if ciasno {
        10_000_000
    } else {
        400_000_000
    })];
    let mut edges = Vec::new();
    for _ in 0..STACJI {
        nodes.push(UtilityNode::hub());
    }
    // Pierścień magistralny: 0 → 1 → … → STACJI → 0.
    for s in 1..=STACJI {
        edges.push(UtilityEdge::new(s - 1, s, 200_000_000, 90));
    }
    edges.push(UtilityEdge::new(STACJI, 0, 200_000_000, 90));
    // Przyłącza: co czwarte przemysłowe, reszta mieszkaniowa.
    for s in 1..=STACJI {
        for p in 0..PRZYLACZY {
            let i = nodes.len() as u32;
            let przemysl = p % 4 == 0;
            nodes.push(UtilityNode::connection(
                None,
                if przemysl { 3 } else { 2 },
                if przemysl { 60_000 } else { 9_000 },
                if przemysl {
                    LoadProfile::Industry
                } else {
                    LoadProfile::Household
                },
            ));
            edges.push(UtilityEdge::new(
                s,
                i,
                if ciasno { 1 } else { 1_000_000 },
                210,
            ));
        }
    }
    UtilityNetwork::new(
        service,
        FirmId(Entity::new(7, NonZeroU32::MIN)),
        nodes,
        edges,
        taryfa(),
    )
}

fn piec_sieci(ciasno: bool) -> Vec<UtilityNetwork> {
    [
        UtilityService::Electricity,
        UtilityService::Water,
        UtilityService::Sewage,
        UtilityService::Gas,
        UtilityService::Heat,
    ]
    .into_iter()
    .map(|s| metropolia(s, ciasno))
    .collect()
}

fn rozgrzej(sieci: &mut [UtilityNetwork]) {
    for n in sieci.iter_mut() {
        let _ = n.solve(
            SEED,
            Tick(1),
            3,
            &ProfileTable::default(),
            RepairWindow::default(),
        );
    }
}

fn m8b_tick(c: &mut Criterion) {
    let mut g = c.benchmark_group("m8b-1 solve_network");
    g.bench_function(format!("tick_{SIECI}_sieci_metropolii"), |b| {
        b.iter_batched(
            || {
                let mut s = piec_sieci(false);
                rozgrzej(&mut s);
                s
            },
            |mut sieci| {
                for (i, n) in sieci.iter_mut().enumerate() {
                    let r = n.solve(
                        SEED,
                        Tick(2 + i as u64),
                        19,
                        &ProfileTable::default(),
                        RepairWindow::default(),
                    );
                    black_box(r.unserved);
                }
                black_box(sieci.len())
            },
            BatchSize::LargeInput,
        );
    });
    g.finish();
}

/// Kaskada ośmiorundowa. Mierzona **osobno dla jednej sieci i dla pięciu naraz**,
/// bo to są dwa różne pytania.
///
/// Kaskada jest z definicji zdarzeniem **jednej** sieci: wypadnięcie linii zmienia
/// topologię tej sieci i nikogo innego. Do tego z tabeli częstotliwości w §5.4
/// wynika, że tylko prąd liczy się co minutę — reszta co godzinę, więc pięć sieci
/// spotyka się w jednym ticku raz na godzinę, a osiem rund kaskady w każdej z nich
/// w tej samej minucie nie jest stanem, który model umie wyprodukować.
///
/// Wiersz „pięć sieci naraz" zostaje mimo to, bo górny koszt ticku wolno znać,
/// a nie zakładać.
fn m8b_kaskada(c: &mut Criterion) {
    let mut g = c.benchmark_group("m8b-2 kaskada");
    g.bench_function(
        format!("{MAX_CASCADE_ROUNDS}_rund_siec_energetyczna"),
        |b| {
            b.iter_batched(
                || metropolia(UtilityService::Electricity, true),
                |mut n| {
                    let r = n.solve(
                        SEED,
                        Tick(2),
                        19,
                        &ProfileTable::default(),
                        RepairWindow::default(),
                    );
                    black_box(r.cascade_rounds)
                },
                BatchSize::LargeInput,
            );
        },
    );
    g.bench_function(
        format!("{MAX_CASCADE_ROUNDS}_rund_{SIECI}_sieci_naraz"),
        |b| {
            b.iter_batched(
                || piec_sieci(true),
                |mut sieci| {
                    for n in sieci.iter_mut() {
                        let r = n.solve(
                            SEED,
                            Tick(2),
                            19,
                            &ProfileTable::default(),
                            RepairWindow::default(),
                        );
                        black_box(r.cascade_rounds);
                    }
                    black_box(sieci.len())
                },
                BatchSize::LargeInput,
            );
        },
    );
    g.finish();
}

criterion_group!(benches, m8b_tick, m8b_kaskada);
criterion_main!(benches);
