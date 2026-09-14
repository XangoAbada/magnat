//! Przebudowa hierarchii z **deterministycznym budżetem pracy na tick** (M4a §5.1, WP2).
//!
//! Sieć drogowa zmienia się w trakcie gry: M8 buduje i remontuje, M2 tylko ją stawia.
//! Zmiana ma dwa poziomy kosztu i to jest cały powód, dla którego §5.1 wybiera CCH,
//! a nie klasyczne CH:
//!
//! | Zdarzenie | Reakcja | Co się przelicza |
//! |---|---|---|
//! | zmiana wagi (limit prędkości, remont mostu, zamknięcie pasa, strefa zakazu) | **kustomizacja** | same wagi skrótów, ta sama kolejność kontrakcji |
//! | zmiana topologii (nowa droga, zburzona droga) | **rekontrakcja** | nowa kolejność i nowe skróty |
//!
//! ## Dlaczego budżet, a nie czas zegarowy
//!
//! Symulacja nie ma prawa widzieć czasu rzeczywistego (00 §3.5). Gdyby moment podmiany
//! grafu zależał od tego, jak szybko policzy go maszyna, to **numer ticku, w którym
//! trasy się zmieniają**, byłby różny w dwóch przebiegach tego samego ziarna — a trasa
//! wpływa na czas dojazdu, czas dojazdu na spóźnienie, spóźnienie na produktywność.
//! Determinizm upadłby nie w routingu, tylko w saldzie gospodarstwa domowego, czyli
//! w miejscu, w którym nikt by go nie szukał.
//!
//! Dlatego zadanie dostaje **budżet `W` jednostek pracy na tick ekonomiczny**, a liczba
//! ticków do ukończenia jest funkcją `ceil(praca_całkowita / W)` — czystą funkcją grafu
//! i stałej, nie obciążenia maszyny. To jest dokładnie kryterium zamknięcia podfazy:
//! *rekontrakcja kończy się w tym samym ticku w dwóch przebiegach tego samego ziarna*.

use magnat_core::Tick;

use crate::graph::Modality;
use crate::router::{NavRouter, RouteProfile};

/// Jednostek pracy na tick ekonomiczny (1 minuta gry).
///
/// Kalibracja: rekontrakcja kosztuje `praca = liczba_węzłów` jednostek, a §5.1 daje jej
/// ≤ 30 ticków (30 minut gry). Dla metropolii M2 (~17 tys. węzłów po podziale na
/// warstwy) daje to ~570 jednostek; przyjęte 1024 zostawia zapas dla miasta dwa razy
/// większego i nadal mieści rekontrakcję w budżecie.
pub const WORK_PER_TICK: u64 = 1024;

/// Kustomizacja jest o rząd wielkości tańsza od rekontrakcji — §5.1 daje jej ≤ 2 ticki.
/// Praca liczy się jako `liczba_węzłów / KUSTOMIZACJA_DZIELNIK`.
pub const CUSTOMIZE_WORK_DIVISOR: u64 = 16;

/// Co jest w kolejce.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavJob {
    /// Przeliczenie wag jednego profilu.
    Customize(RouteProfile),
    /// Nowa kolejność kontrakcji i wszystkie trzy profile od nowa.
    Recontract,
}

impl NavJob {
    /// Praca w jednostkach budżetu — **czysta funkcja rozmiaru grafu**, nigdy pomiar.
    #[must_use]
    pub fn work(self, nodes: u64) -> u64 {
        match self {
            NavJob::Customize(_) => (nodes / CUSTOMIZE_WORK_DIVISOR).max(1),
            NavJob::Recontract => nodes.max(1),
        }
    }
}

/// Zadanie w trakcie: ile pracy zostało i od kiedy trwa.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pending {
    pub job: NavJob,
    pub started_at: Tick,
    pub work_total: u64,
    pub work_done: u64,
}

impl Pending {
    /// Tick, w którym zadanie się zamknie. Pełna informacja **z góry** — i to ona
    /// jest testowana: dwa przebiegi muszą podać tę samą liczbę.
    #[must_use]
    pub fn finishes_at(&self, work_per_tick: u64) -> Tick {
        let zostalo = self.work_total.saturating_sub(self.work_done);
        let ticki = zostalo.div_ceil(work_per_tick.max(1));
        Tick(self.started_at.0 + ticki)
    }
}

/// Co się stało w tym ticku.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RebuildProgress {
    /// Nic nie było do zrobienia.
    Idle,
    /// Zadanie postąpiło, ale się nie skończyło.
    Working { job: NavJob, remaining: u64 },
    /// Zadanie skończone i graf podmieniony w tym ticku.
    Swapped { job: NavJob, at: Tick },
}

/// Kolejka przebudów z budżetem.
///
/// Kolejka jest `Vec`-iem, nie mapą — zadań jest najwyżej cztery (trzy profile plus
/// rekontrakcja), a kolejność zgłoszeń jest częścią wyniku.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RebuildQueue {
    work_per_tick: u64,
    queue: Vec<NavJob>,
    current: Option<Pending>,
    last_topology: u32,
    last_weights: [u32; 3],
    /// Ile razy podmieniono graf — do dziennika i do testu determinizmu.
    swaps: u32,
}

impl RebuildQueue {
    #[must_use]
    pub fn new(work_per_tick: u64) -> RebuildQueue {
        RebuildQueue {
            work_per_tick: work_per_tick.max(1),
            queue: Vec::new(),
            current: None,
            last_topology: 0,
            last_weights: [0; 3],
            swaps: 0,
        }
    }

    #[must_use]
    pub fn pending(&self) -> Option<&Pending> {
        self.current.as_ref()
    }

    #[must_use]
    pub fn queued(&self) -> usize {
        self.queue.len() + usize::from(self.current.is_some())
    }

    #[must_use]
    pub fn swaps(&self) -> u32 {
        self.swaps
    }

    /// Zgłasza zadanie. Duplikat już stojący w kolejce jest pomijany — dziesięć
    /// zamknięć pasa w jednej minucie to nadal jedna kustomizacja.
    pub fn request(&mut self, job: NavJob) {
        let juz = self.current.map(|p| p.job) == Some(job) || self.queue.contains(&job);
        if !juz {
            self.queue.push(job);
        }
    }

    /// Porównuje wersje grafu z ostatnio widzianymi i sam zgłasza, co trzeba.
    /// To jest zamiennik zdarzenia `RoadNetworkChanged` z M2 (`D8`), którego M2 nie
    /// wystawia, bo nie zmienia dróg w trakcie gry. Faza, która zacznie je zmieniać,
    /// podmieni to na zdarzenie z listą brudnych krawędzi i nie ruszy reszty modułu.
    pub fn detect(&mut self, router: &NavRouter) {
        let road = router.graphs().layer(Modality::Road);
        if road.topology_version != self.last_topology {
            self.last_topology = road.topology_version;
            self.last_weights = [road.weight_version; 3];
            self.request(NavJob::Recontract);
            return;
        }
        for (i, p) in RouteProfile::ALL.iter().enumerate() {
            if road.weight_version != self.last_weights[i] {
                self.last_weights[i] = road.weight_version;
                self.request(NavJob::Customize(*p));
            }
        }
    }

    /// Jeden krok na tick ekonomiczny. Podmiana następuje w punkcie synchronizacji
    /// tego ticku, w którym budżet pracy się wyczerpie.
    pub fn step(&mut self, router: &mut NavRouter, tick: Tick) -> RebuildProgress {
        if self.current.is_none() {
            if self.queue.is_empty() {
                return RebuildProgress::Idle;
            }
            let job = self.queue.remove(0);
            let nodes = router.graphs().layer(Modality::Road).node_count() as u64;
            self.current = Some(Pending {
                job,
                started_at: tick,
                work_total: job.work(nodes),
                work_done: 0,
            });
        }

        let p = self.current.as_mut().expect("zadanie właśnie wstawione");
        p.work_done = p.work_done.saturating_add(self.work_per_tick);
        if p.work_done < p.work_total {
            let remaining = p.work_total - p.work_done;
            return RebuildProgress::Working {
                job: p.job,
                remaining,
            };
        }

        let job = p.job;
        self.current = None;
        self.swaps += 1;
        // ponytail: liczenie jest tu jednorazowe, w ticku podmiany, zamiast rozłożone
        // na kawałki. Kontrakt, który niesie ten moduł — **numer ticku podmiany** —
        // jest spełniony co do ticku; ceną jest zacięcie ~60 ms (metropolia M2)
        // albo ~1,9 s (200 tys. węzłów) w tym jednym ticku. Ścieżka wyjścia jest
        // znana i wąska: `ChGraph::customize_range(od_rangi, do_rangi)` i wznawialna
        // kontrakcja, wołane po kawałku w każdym `step`. Robi się to wtedy, gdy M8
        // zacznie przebudowywać sieć często — dziś nie zmienia jej nikt.
        match job {
            NavJob::Customize(profile) => {
                let wv = router.graphs().layer(Modality::Road).weight_version;
                router.customize(profile, None, wv);
            }
            NavJob::Recontract => router.recontract(),
        }
        RebuildProgress::Swapped { job, at: tick }
    }
}

impl Default for RebuildQueue {
    fn default() -> RebuildQueue {
        RebuildQueue::new(WORK_PER_TICK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{synthetic_grid, Modality, NavGraphs, RoadGraph, RoadGraphBuilder};
    use crate::router::{NavRouter, RouteQuery, Router};
    use magnat_core::TransportMode;

    fn router(road: RoadGraph) -> NavRouter {
        let pusty = |m: Modality| RoadGraphBuilder::new(m).with_turns(false).finish();
        NavRouter::build(
            NavGraphs::new(
                [
                    road,
                    pusty(Modality::Foot),
                    pusty(Modality::Bike),
                    pusty(Modality::Rail),
                ],
                Vec::new(),
            ),
            1,
            64,
        )
    }

    /// Przebieg: zgłoś rekontrakcję w ticku `start`, kręć ticki, zwróć tick podmiany.
    fn tick_podmiany(work_per_tick: u64, start: u64) -> u64 {
        let mut r = router(synthetic_grid(12, 12, 10_000));
        let mut q = RebuildQueue::new(work_per_tick);
        q.request(NavJob::Recontract);
        for t in start..start + 500 {
            if let RebuildProgress::Swapped { at, .. } = q.step(&mut r, Tick(t)) {
                return at.0;
            }
        }
        panic!("rekontrakcja nigdy się nie skończyła");
    }

    #[test]
    fn rekontrakcja_konczy_sie_w_tym_samym_ticku_w_dwoch_przebiegach() {
        // To jest kryterium zamknięcia M4a.
        for start in [0u64, 7, 1_000] {
            let a = tick_podmiany(37, start);
            let b = tick_podmiany(37, start);
            assert_eq!(a, b, "moment podmiany musi być powtarzalny");
        }
    }

    #[test]
    fn tick_podmiany_jest_funkcja_budzetu_a_nie_zegara() {
        // 144 węzły, budżet 37/tick → ceil(144/37) = 4 ticki od zgłoszenia.
        assert_eq!(tick_podmiany(37, 0), 3, "czwarty tick to numer 3");
        assert_eq!(tick_podmiany(37, 100), 103);
        // Dwa razy większy budżet → dwa razy mniej ticków.
        assert_eq!(tick_podmiany(144, 0), 0);
        assert_eq!(tick_podmiany(72, 0), 1);
    }

    #[test]
    fn zapowiedziany_tick_zgadza_sie_z_faktycznym() {
        let mut r = router(synthetic_grid(12, 12, 10_000));
        let mut q = RebuildQueue::new(37);
        q.request(NavJob::Recontract);
        let progress = q.step(&mut r, Tick(10));
        assert!(matches!(progress, RebuildProgress::Working { .. }));
        let zapowiedz = q.pending().expect("zadanie trwa").finishes_at(37);
        let mut faktyczny = None;
        for t in 11..100 {
            if let RebuildProgress::Swapped { at, .. } = q.step(&mut r, Tick(t)) {
                faktyczny = Some(at);
                break;
            }
        }
        assert_eq!(Some(zapowiedz), faktyczny);
    }

    #[test]
    fn duplikat_zgloszenia_nie_mnozy_pracy() {
        let mut r = router(synthetic_grid(8, 8, 10_000));
        let mut q = RebuildQueue::new(1_000_000);
        for _ in 0..10 {
            q.request(NavJob::Customize(RouteProfile::Passenger));
        }
        assert_eq!(q.queued(), 1);
        assert!(matches!(
            q.step(&mut r, Tick(0)),
            RebuildProgress::Swapped { .. }
        ));
        assert_eq!(q.step(&mut r, Tick(1)), RebuildProgress::Idle);
        assert_eq!(q.swaps(), 1);
    }

    #[test]
    fn wykrywanie_zmiany_wersji_zglasza_wlasciwe_zadanie() {
        let mut g = synthetic_grid(8, 8, 10_000);
        g.topology_version = 5;
        g.weight_version = 5;
        let r = router(g);
        let mut q = RebuildQueue::new(WORK_PER_TICK);
        q.detect(&r);
        assert_eq!(q.queued(), 1);
        assert_eq!(q.queue[0], NavJob::Recontract);
        // Druga detekcja bez zmiany wersji nie dokłada nic.
        q.detect(&r);
        assert_eq!(q.queued(), 1);
    }

    #[test]
    fn po_rekontrakcji_trasy_nadal_sa_poprawne() {
        let mut r = router(synthetic_grid(12, 12, 10_000));
        let q0 = RouteQuery::passenger(
            crate::graph::NodeId(0),
            crate::graph::NodeId(143),
            TransportMode::Car,
            7,
        );
        let przed = r.route(&q0).expect("trasa istnieje").cost_cs;
        let mut q = RebuildQueue::new(1_000_000);
        q.request(NavJob::Recontract);
        assert!(matches!(
            q.step(&mut r, Tick(0)),
            RebuildProgress::Swapped { .. }
        ));
        let po = r
            .route(&q0)
            .expect("trasa istnieje po rekontrakcji")
            .cost_cs;
        assert_eq!(przed, po, "rekontrakcja nie zmienia kosztu trasy");
    }
}
