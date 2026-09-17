//! Systemy, scheduler DAG i pętla ticku (M0 §5.7, PRD §17.2).
//!
//! **Dlaczego wynik nie zależy od liczby wątków:** jeśli dwa systemy biegną
//! równolegle, to znaczy, że nie mają między sobą krawędzi, czyli ich zbiory dostępów
//! nie konfliktują — nie widzą nawzajem swoich zapisów. Jedyny kanał, przez który
//! mogłyby na siebie wpłynąć, to bufory komend, a te są sortowane po
//! `(SystemId, target, seq)` niezależnie od kolejności wykonania. Gwarancja jest
//! strukturalna; test T-D2 sprawdza ją empirycznie.

use crate::access::Access;
use crate::command::{flush_commands, CommandBuffer, FlushStats};
use crate::query::{Query, QueryData, QueryFilter, UnsafeWorldCell};
use crate::world::World;
use magnat_core::{Cadence, Tick};
use magnat_jobs::JobPool;
use xxhash_rust::xxh3::xxh3_64;

/// Stabilny identyfikator systemu, wyprowadzony z **pełnej nazwy**
/// (`"sim.economy.match_offers"`), nie z kolejności rejestracji: dodanie systemu
/// w M7 nie może zmienić porządku sortowania komend systemów z M5 (00 §3.4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SystemId(u32);

impl SystemId {
    #[must_use]
    pub fn from_name(name: &str) -> SystemId {
        SystemId((xxh3_64(name.as_bytes()) >> 32) as u32)
    }

    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

pub struct SystemDesc {
    pub id: SystemId,
    pub name: &'static str,
    pub cadence: Cadence,
    pub access: Access,
    /// Ograniczenia jawne — tylko tam, gdzie kolejność jest semantyczna, a nie wynika
    /// z konfliktu dostępów (np. „naliczanie odsetek przed rozliczeniem dnia").
    pub after: Vec<SystemId>,
    pub before: Vec<SystemId>,
    /// Ograniczenia **warunkowe**: obowiązują, jeśli wskazany system stoi w tym
    /// harmonogramie, i milczą, jeśli go nie ma (`K-51`).
    pub after_if_present: Vec<SystemId>,
    /// System otwierający tick — patrz [`SystemDesc::opens_tick`].
    pub opens_tick: bool,
}

impl SystemDesc {
    #[must_use]
    pub fn new(name: &'static str, cadence: Cadence) -> SystemDesc {
        SystemDesc {
            id: SystemId::from_name(name),
            name,
            cadence,
            access: Access::new(),
            after: Vec::new(),
            before: Vec::new(),
            after_if_present: Vec::new(),
            opens_tick: false,
        }
    }

    /// Dokłada dostęp wyliczony **z typów zapytania** — to jest zalecana droga.
    #[must_use]
    pub fn with_query<Q: QueryData, F: QueryFilter>(mut self, world: &World) -> SystemDesc {
        Q::declare_access(world.components(), &mut self.access);
        self
    }

    #[must_use]
    pub fn reads_resource<T: Send + Sync + 'static>(mut self, world: &World) -> SystemDesc {
        if let Some(r) = world.resources().id_of::<T>() {
            self.access.read_resource(r);
        }
        self
    }

    #[must_use]
    pub fn writes_resource<T: Send + Sync + 'static>(mut self, world: &World) -> SystemDesc {
        if let Some(r) = world.resources().id_of::<T>() {
            self.access.write_resource(r);
        }
        self
    }

    /// System zgłaszający komendy strukturalne — kończy etap, bo jego komendy muszą
    /// zostać zaaplikowane, zanim ktokolwiek zobaczy nowe encje.
    #[must_use]
    pub fn structural(mut self) -> SystemDesc {
        self.access.set_structural(true);
        self
    }

    /// System wyłączny: dostaje `&mut World` przez [`SystemCtx::world_mut`] i biegnie
    /// sam. Determinizm jest wtedy trywialny — nie ma z kim się ścigać — a cena
    /// jest jawna: ten system nie korzysta ze zrównoleglenia.
    #[must_use]
    pub fn exclusive(mut self) -> SystemDesc {
        self.access.set_exclusive(true);
        self
    }

    #[must_use]
    pub fn after(mut self, other: SystemId) -> SystemDesc {
        self.after.push(other);
        self
    }

    #[must_use]
    pub fn before(mut self, other: SystemId) -> SystemDesc {
        self.before.push(other);
        self
    }

    /// „Po tamtym, **jeśli tamten tu jest**" (`K-51`).
    ///
    /// [`SystemDesc::after`] wymaga, żeby wskazany system stał w tym samym
    /// harmonogramie — i słusznie, bo literówka w nazwie jest wtedy błędem budowy,
    /// a nie po cichu zignorowaną krawędzią. Ale scenariusz stawia **wycinek**
    /// symulacji: `m5shop` buduje rynek bez rejestru firm, `m7labor` rejestr bez
    /// rynku, a klient graficzny jedno i drugie. System, którego kolejność ma
    /// znaczenie tylko wobec systemu **opcjonalnego**, nie ma czym tego wyrazić
    /// przez `after` — i albo wywraca połowę scenariuszy, albo oddaje kolejność
    /// hashowi nazwy, czyli przypadkowi, przed którym broni `K-42`.
    ///
    /// Różnica wobec `after` jest jedna i jest cała: **brak celu nie jest błędem**.
    /// Cel obecny daje dokładnie tę samą krawędź, z tym samym pierwszeństwem nad
    /// krawędzią wyprowadzoną z konfliktu dostępów.
    #[must_use]
    pub fn after_if_present(mut self, other: SystemId) -> SystemDesc {
        self.after_if_present.push(other);
        self
    }

    /// System, który **otwiera tick**: w kolejności kanonicznej staje przed
    /// wszystkimi, które go nie otwierają (`K-53`).
    ///
    /// # Po co, skoro jest `before`
    ///
    /// Bo `before` rozstrzyga **parę**, a tu chodzi o pozycję wobec wszystkich.
    /// Kolejność kanoniczna to sortowanie po `SystemId`, czyli po **hashu nazwy**,
    /// i to z niej biorą kierunek krawędzie konfliktu dostępów. Dla systemów
    /// wyłącznych (`K-21`) konflikt zachodzi z każdym, więc taki system dostaje
    /// krawędź od każdego, kto w hashu wypadł przed nim. Jeden jawny `before`
    /// odwraca jedną z nich i domyka cykl przez pozostałe — a cykl ma wtedy długość
    /// pięciu systemów i wygląda na błąd deklaracji, którym nie jest.
    ///
    /// Tak właśnie pękł pierwszy scenariusz stawiający `firms.Firm` obok pełnego
    /// zestawu systemów mieszkańca (M7f WP17): `economy.Market → economy.Insolvency
    /// → agents.DayLoop → traffic.Mezo → agents.Society → firms.Firm → …`. Deklaracja
    /// „firmy otwierają minutę" jest jednym zdaniem i nie wymienia ani jednego
    /// cudzego systemu — a to jest istotne, bo `sim/firms` nie wie o istnieniu ruchu
    /// i nie ma powodu, żeby wiedział.
    ///
    /// Determinizm nie zmienia się w niczym: w obrębie obu grup kolejność nadal
    /// rozstrzyga `SystemId`, a `Schedule::fingerprint` liczy się z krawędzi
    /// wynikowych. Dwa systemy otwierające tick są dopuszczalne i uszeregują się
    /// między sobą kanonicznie.
    #[must_use]
    pub fn opens_tick(mut self) -> SystemDesc {
        self.opens_tick = true;
        self
    }
}

/// Krawędzie z jawnych `before`/`after`. Wyniesione z [`ScheduleBuilder::build`],
/// bo są osobnym pytaniem: „co autor zadeklarował" wobec „co wynika z dostępów".
fn krawedzie_jawne(
    systems: &[Box<dyn System>],
    index_of: &dyn Fn(SystemId) -> Option<usize>,
) -> Result<Vec<(usize, usize)>, ScheduleError> {
    let mut jawne = Vec::new();
    for (i, sys) in systems.iter().enumerate() {
        let desc = sys.desc();
        for target in &desc.after {
            let Some(j) = index_of(*target) else {
                return Err(ScheduleError::UnknownConstraint {
                    system: desc.name,
                    target: *target,
                });
            };
            jawne.push((j, i));
        }
        for target in &desc.before {
            let Some(j) = index_of(*target) else {
                return Err(ScheduleError::UnknownConstraint {
                    system: desc.name,
                    target: *target,
                });
            };
            jawne.push((i, j));
        }
        // Warunkowe: cel nieobecny to **brak krawędzi**, nie błąd (`K-51`).
        for target in &desc.after_if_present {
            if let Some(j) = index_of(*target) {
                jawne.push((j, i));
            }
        }
    }
    Ok(jawne)
}

pub trait System: Send + Sync + 'static {
    fn desc(&self) -> &SystemDesc;
    fn run(&mut self, ctx: &mut SystemCtx<'_>);
}

/// Wszystko, co system dostaje. **Brak dostępu do zegara systemowego** — z założenia
/// (00 §3.5): `Instant::now()` w kodzie symulacji to natychmiastowy niedeterminizm.
pub struct SystemCtx<'w> {
    world: UnsafeWorldCell<'w>,
    pub pool: &'w JobPool,
    pub tick: Tick,
    pub seed: u64,
    pub cmd: &'w mut CommandBuffer,
    access: &'w Access,
}

impl<'w> SystemCtx<'w> {
    /// Zapytanie o wiersze. Pożyczka `&mut self` gwarantuje, że system nie trzyma
    /// dwóch zapytań naraz — inaczej mógłby sam sobie zaaliasować `&mut T`.
    pub fn query<Q: QueryData, F: QueryFilter>(&mut self) -> Query<'_, Q, F> {
        // SAFETY: scheduler uruchamia równolegle wyłącznie systemy o rozłącznych
        // dostępach, a `&mut self` wyklucza drugie zapytanie z tego samego systemu.
        unsafe { Query::new(self.world.reborrow()) }
    }

    /// Świat tylko do odczytu — do sprawdzenia istnienia encji, kalendarza, seeda.
    #[must_use]
    pub fn world(&self) -> &World {
        // SAFETY: odczyt; systemy konfliktujące nie biegną równolegle.
        unsafe { self.world.world() }
    }

    /// Zasób do odczytu. W trybie debug sprawdza, czy system go zadeklarował —
    /// niezadeklarowany dostęp znaczy, że scheduler nie widzi prawdziwej zależności.
    #[must_use]
    pub fn res<T: Send + Sync + 'static>(&self) -> &T {
        // SAFETY: jak w `world`.
        let w = unsafe { self.world.world() };
        debug_assert!(
            w.resources()
                .id_of::<T>()
                .is_some_and(|r| self.access.reads_resource(r)),
            "system czyta zasób {}, którego nie zadeklarował — scheduler nie zbuduje \
             poprawnej krawędzi DAG",
            std::any::type_name::<T>()
        );
        w.resource::<T>()
    }

    /// Cały świat do zapisu — **wyłącznie dla systemu wyłącznego** (`SystemDesc::exclusive`).
    ///
    /// Scheduler stawia taki system sam na swoim poziomie i domyka nim etap, więc
    /// wyłączna pożyczka jest tu prawdziwa, a nie deklarowana. Bez tej furtki nie da
    /// się opakować kroku, który jest funkcją nad całym światem (M3d §5.12).
    pub fn world_mut(&mut self) -> &mut World {
        debug_assert!(
            self.access.is_exclusive(),
            "world_mut w systemie, który nie jest wyłączny — scheduler puści obok niego              inny system i pożyczka przestanie być wyłączna"
        );
        // SAFETY: system wyłączny konfliktuje z każdym innym, więc jest jedynym
        // systemem na swoim poziomie; nikt inny nie trzyma w tej chwili pożyczki świata.
        unsafe { self.world.world_mut() }
    }

    /// Zasób do zapisu. Dwa systemy z niezadeklarowanym `res_mut` na ten sam zasób
    /// biegłyby równolegle — dlatego brak deklaracji panikuje w debug.
    pub fn res_mut<T: Send + Sync + 'static>(&mut self) -> &mut T {
        // SAFETY: zapis; scheduler serializuje systemy deklarujące ten sam zasób.
        let w = unsafe { self.world.world_mut() };
        debug_assert!(
            w.resources()
                .id_of::<T>()
                .is_some_and(|r| self.access.writes_resource(r)),
            "system pisze do zasobu {}, którego nie zadeklarował",
            std::any::type_name::<T>()
        );
        w.resource_mut::<T>()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScheduleError {
    DuplicateSystemId {
        name_a: &'static str,
        name_b: &'static str,
    },
    OrderingCycle {
        cycle: Vec<&'static str>,
    },
    UnknownConstraint {
        system: &'static str,
        target: SystemId,
    },
}

impl std::fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduleError::DuplicateSystemId { name_a, name_b } => write!(
                f,
                "kolizja SystemId: {name_a} i {name_b} mają ten sam identyfikator"
            ),
            ScheduleError::OrderingCycle { cycle } => {
                write!(f, "cykl ograniczeń after/before: {}", cycle.join(" → "))
            }
            ScheduleError::UnknownConstraint { system, target } => write!(
                f,
                "system {system} odwołuje się do nieistniejącego systemu {target:?}"
            ),
        }
    }
}

impl std::error::Error for ScheduleError {}

/// Etap: zbiór systemów rozdzielony na poziomy zależności, zakończony barierą.
pub struct Stage {
    /// Poziomy: systemy w jednym poziomie nie mają między sobą krawędzi,
    /// więc mogą biec równolegle. Podział jest funkcją grafu, nie kolejności
    /// rejestracji — to on daje powtarzalność przy każdej liczbie wątków.
    levels: Vec<Vec<usize>>,
}

pub struct Schedule {
    systems: Vec<Option<Box<dyn System>>>,
    /// Kopia dostępów w kolejności kanonicznej. Osobno od systemów, bo w trakcie
    /// wykonania system jest wyłącznie pożyczony przez swoje zadanie, a `SystemCtx`
    /// musi jednocześnie widzieć jego deklarację dostępu.
    accesses: Vec<Access>,
    stages: Vec<Stage>,
    /// Odcisk grafu — test T-D3 porównuje go między permutacjami rejestracji.
    fingerprint: u64,
}

impl Schedule {
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    #[must_use]
    pub fn system_count(&self) -> usize {
        self.systems.len()
    }

    #[must_use]
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Nazwy systemów w kolejności kanonicznej — do diagnostyki i testów.
    pub fn system_names(&self) -> Vec<&'static str> {
        self.systems
            .iter()
            .map(|s| s.as_ref().expect("system wypożyczony").desc().name)
            .collect()
    }
}

#[derive(Default)]
pub struct ScheduleBuilder {
    systems: Vec<Box<dyn System>>,
}

impl ScheduleBuilder {
    #[must_use]
    pub fn new() -> ScheduleBuilder {
        ScheduleBuilder::default()
    }

    pub fn add<S: System>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(system));
        self
    }

    pub fn add_boxed(&mut self, system: Box<dyn System>) -> &mut Self {
        self.systems.push(system);
        self
    }

    /// Buduje DAG wg algorytmu z M0 §5.7. To jest kontrakt, nie implementacja
    /// do wyboru:
    /// 1. kolejność kanoniczna = sortowanie po `(nie otwiera ticku, SystemId)` —
    ///    systemy z [`SystemDesc::opens_tick`] idą pierwsze (`K-53`),
    /// 2. krawędź dla każdej pary konfliktującej `(i, j)`, `i < j` — zawsze „w przód",
    ///    więc graf konfliktów jest acykliczny **z konstrukcji**,
    /// 3. krawędzie z `after`/`before` — dopiero tu możliwy jest cykl,
    /// 4. podział na etapy: system strukturalny kończy etap,
    /// 5. wykonanie poziomami na `JobPool`.
    pub fn build(self) -> Result<Schedule, ScheduleError> {
        let mut systems = self.systems;
        // 1. Kolejność kanoniczna. `opens_tick` jest **tylko** kluczem sortowania:
        // w obrębie każdej z dwóch grup rozstrzyga `SystemId`, więc determinizm
        // kolejności zostaje bez zmian (`K-53`).
        systems.sort_by_key(|s| (!s.desc().opens_tick, s.desc().id));

        for para in systems.windows(2) {
            if para[0].desc().id == para[1].desc().id {
                return Err(ScheduleError::DuplicateSystemId {
                    name_a: para[0].desc().name,
                    name_b: para[1].desc().name,
                });
            }
        }

        let n = systems.len();
        let index_of = |id: SystemId| systems.iter().position(|s| s.desc().id == id);
        let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];

        // 2. Ograniczenia jawne — **przed** konfliktami, bo to one rozstrzygają spór.
        //
        //    Kolejność kroków 2 i 3 jest odwrotna niż do M6e i to nie jest kosmetyka
        //    (`AP-6`). Konflikt dostępów mówi „tych dwóch nie wolno puścić równolegle"
        //    i nie ma zdania o kierunku — kierunek brał się z kolejności kanonicznej,
        //    czyli z **hasha nazwy**. Dla pary systemów wyłącznych (`K-21`) konflikt
        //    zachodzi zawsze, więc jawne `before` między nimi dawało cykl zawsze wtedy,
        //    gdy hash trafił odwrotnie — a trafiał losowo, bo nazwa jest nazwą.
        //    Skutek: kontraktu kolejności (`D12` fazy M6: psucie przed detalem)
        //    **nie dało się wyrazić**, choć jest to dokładnie to, do czego `before`
        //    służy. Ograniczenie jawne jest deklaracją autora, konflikt jest wnioskiem
        //    z dostępów; deklaracja wygrywa.
        let jawne = krawedzie_jawne(&systems, &index_of)?;
        for (a, b) in &jawne {
            edges[*a].push(*b);
        }

        // 3. Krawędzie z konfliktów dostępów — pomijane tam, gdzie jawne ograniczenie
        //    ustawiło już parę w drugą stronę. Kolejność i tak zostaje wymuszona, tylko
        //    kierunkiem, który zadeklarował autor.
        for i in 0..n {
            for j in (i + 1)..n {
                if jawne.contains(&(j, i)) {
                    continue;
                }
                if systems[i]
                    .desc()
                    .access
                    .conflicts_with(&systems[j].desc().access)
                {
                    edges[i].push(j);
                }
            }
        }
        for e in &mut edges {
            e.sort_unstable();
            e.dedup();
        }

        // 4. Podział na etapy: system strukturalny domyka etap, ale grupa sąsiadujących
        //    systemów strukturalnych dzieli jedną barierę (jeden wspólny flush).
        let mut stage_ranges: Vec<(usize, usize)> = Vec::new();
        let mut start = 0usize;
        for i in 0..n {
            let strukturalny = systems[i].desc().access.is_structural();
            let nastepny_strukturalny = i + 1 < n && systems[i + 1].desc().access.is_structural();
            if strukturalny && !nastepny_strukturalny {
                stage_ranges.push((start, i + 1));
                start = i + 1;
            }
        }
        if start < n {
            stage_ranges.push((start, n));
        }

        // 5. Poziomy wewnątrz etapu + wykrycie cyklu (poziom bez gotowych węzłów).
        let mut stages = Vec::new();
        for (from, to) in stage_ranges {
            let mut pozostale: Vec<usize> = (from..to).collect();
            let mut levels: Vec<Vec<usize>> = Vec::new();
            let mut gotowe: Vec<bool> = vec![false; n];
            while !pozostale.is_empty() {
                let level: Vec<usize> = pozostale
                    .iter()
                    .copied()
                    .filter(|&i| {
                        // Węzeł jest gotowy, gdy nikt z pozostałych nie ma do niego krawędzi.
                        !pozostale
                            .iter()
                            .any(|&j| j != i && edges[j].contains(&i) && !gotowe[j])
                    })
                    .collect();
                if level.is_empty() {
                    let cycle = pozostale
                        .iter()
                        .map(|&i| systems[i].desc().name)
                        .collect::<Vec<_>>();
                    return Err(ScheduleError::OrderingCycle { cycle });
                }
                for &i in &level {
                    gotowe[i] = true;
                }
                pozostale.retain(|i| !level.contains(i));
                levels.push(level);
            }
            stages.push(Stage { levels });
        }

        // Odcisk grafu: nazwy w kolejności kanonicznej + krawędzie + podział na etapy.
        let mut odcisk = Vec::new();
        for (i, s) in systems.iter().enumerate() {
            odcisk.extend_from_slice(s.desc().name.as_bytes());
            odcisk.push(b'|');
            for j in &edges[i] {
                odcisk.extend_from_slice(systems[*j].desc().name.as_bytes());
                odcisk.push(b',');
            }
            odcisk.push(b';');
        }
        for stage in &stages {
            for level in &stage.levels {
                odcisk.extend_from_slice(format!("{level:?}").as_bytes());
            }
            odcisk.push(b'#');
        }

        let accesses: Vec<Access> = systems.iter().map(|s| s.desc().access.clone()).collect();
        Ok(Schedule {
            systems: systems.into_iter().map(Some).collect(),
            accesses,
            stages,
            fingerprint: xxh3_64(&odcisk),
        })
    }
}

/// Pętla ticku. Jedyne miejsce, w którym rośnie `Tick`.
pub struct App {
    pub world: World,
    schedule: Schedule,
    pool: JobPool,
    buffers: Vec<CommandBuffer>,
    last_flush: FlushStats,
}

impl App {
    #[must_use]
    pub fn new(world: World, schedule: Schedule, threads: usize) -> App {
        let buffers = schedule
            .systems
            .iter()
            .map(|s| CommandBuffer::new(s.as_ref().expect("system wypożyczony").desc().id))
            .collect();
        App {
            world,
            schedule,
            pool: JobPool::new(threads),
            buffers,
            last_flush: FlushStats::default(),
        }
    }

    #[must_use]
    pub fn thread_count(&self) -> usize {
        self.pool.thread_count()
    }

    #[must_use]
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    #[must_use]
    pub fn last_flush(&self) -> FlushStats {
        self.last_flush
    }

    /// 1. `tick += 1`, 2. dla każdego etapu: uruchom należne systemy poziom po poziomie,
    /// 3. bariera: flush komend.
    pub fn tick(&mut self) {
        self.world.tick = Tick(self.world.tick.0 + 1);
        let tick = self.world.tick;
        let seed = self.world.seed;

        for stage_index in 0..self.schedule.stages.len() {
            let poziomy = self.schedule.stages[stage_index].levels.clone();
            for level in poziomy {
                // Do poziomu wchodzą tylko systemy należne w tym ticku (Cadence).
                let nalezne: Vec<usize> = level
                    .into_iter()
                    .filter(|&i| {
                        self.schedule.systems[i]
                            .as_ref()
                            .expect("system wypożyczony")
                            .desc()
                            .cadence
                            .due(tick)
                    })
                    .collect();
                if nalezne.is_empty() {
                    continue;
                }

                let cell = UnsafeWorldCell::new(&mut self.world);
                // Systemy i ich bufory są wyjmowane na czas poziomu: dzięki temu
                // każde zadanie dostaje wyłączną pożyczkę bez `unsafe`.
                let mut wypozyczone: Vec<(usize, Box<dyn System>, CommandBuffer)> = nalezne
                    .iter()
                    .map(|&i| {
                        let sys = self.schedule.systems[i].take().expect("system wypożyczony");
                        let buf = std::mem::replace(
                            &mut self.buffers[i],
                            CommandBuffer::new(sys.desc().id),
                        );
                        (i, sys, buf)
                    })
                    .collect();

                let pool = &self.pool;
                let accesses = &self.schedule.accesses;
                pool.scope(|s| {
                    for (i, sys, buf) in wypozyczone.iter_mut() {
                        let access = &accesses[*i];
                        s.spawn(move |_| {
                            let mut ctx = SystemCtx {
                                world: cell,
                                pool,
                                tick,
                                seed,
                                cmd: buf,
                                access,
                            };
                            sys.run(&mut ctx);
                        });
                    }
                });

                for (i, sys, buf) in wypozyczone {
                    self.schedule.systems[i] = Some(sys);
                    self.buffers[i] = buf;
                }
            }

            // Bariera po etapie: komendy stają się widoczne dla kolejnych etapów.
            self.last_flush = flush_commands(&mut self.world, &mut self.buffers);
        }
    }

    pub fn run_ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }
}
