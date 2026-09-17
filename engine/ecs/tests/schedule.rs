//! Scheduler DAG i bufory komend (M0 WP-07, WP-08; testy T-D2, T-D3, T-D6).

use magnat_core::{rng, Cadence, HashState, StateHasher, StreamId};
use magnat_ecs::{
    flush_commands, App, CommandBuffer, Component, Entity, ScheduleBuilder, ScheduleError, System,
    SystemCtx, SystemDesc, SystemId, World,
};

macro_rules! komponent {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq)]
        struct $name(i64);
        impl HashState for $name {
            fn hash_state(&self, h: &mut StateHasher) {
                self.0.hash_state(h);
            }
        }
        impl Component for $name {
            const NAME: &'static str = stringify!($name);
        }
    };
}

komponent!(A);
komponent!(B);
komponent!(C);

/// System dodający do A wartość z B — czyta jedno, pisze drugie.
struct DodajBDoA(SystemDesc);
impl System for DodajBDoA {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        for (a, b) in ctx.query::<(&mut A, &B), ()>().iter() {
            a.0 = a.0.wrapping_add(b.0);
        }
    }
}

/// System czysto odczytowy — dwa takie mogą biec równolegle.
struct SumujA(SystemDesc, std::sync::Arc<std::sync::atomic::AtomicI64>);
impl System for SumujA {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let mut suma = 0i64;
        for a in ctx.query::<&A, ()>().iter() {
            suma = suma.wrapping_add(a.0);
        }
        self.1.store(suma, std::sync::atomic::Ordering::Relaxed);
    }
}

/// System mnożący C — nie konfliktuje z żadnym z powyższych.
struct PomnozC(SystemDesc);
impl System for PomnozC {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        for c in ctx.query::<&mut C, ()>().iter() {
            c.0 = c.0.wrapping_mul(3).wrapping_add(1);
        }
    }
}

/// System strukturalny: spawnuje i despawnuje ok. 0,1 % encji na tick,
/// korzystając wyłącznie z RNG wyprowadzonego z seeda i ticku.
struct Rotacja(SystemDesc);
impl System for Rotacja {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let tick = ctx.tick;
        let seed = ctx.seed;
        let do_usuniecia: Vec<Entity> = ctx
            .query::<(Entity, &A), ()>()
            .iter()
            .filter_map(|(e, _)| {
                let mut r = rng(seed, StreamId::EngineSelfTest, e.index(), tick);
                r.gen_bool_permille(1).then_some(e)
            })
            .collect();
        for e in &do_usuniecia {
            ctx.cmd.despawn(*e);
        }
        for i in 0..do_usuniecia.len() {
            let r = ctx.cmd.spawn();
            ctx.cmd.insert_reserved(r, A(tick.0 as i64 + i as i64));
            ctx.cmd.insert_reserved(r, B(1));
        }
    }
}

fn zbuduj_swiat(n: i64) -> World {
    let mut w = World::new(42);
    w.register_component::<A>();
    w.register_component::<B>();
    w.register_component::<C>();
    for i in 0..n {
        let mut e = w.spawn().with(A(i));
        if i % 2 == 0 {
            e = e.with(B(i));
        }
        if i % 3 == 0 {
            e = e.with(C(i));
        }
        let _ = e.id();
    }
    w
}

fn opisy(world: &World) -> Vec<SystemDesc> {
    vec![
        SystemDesc::new("test.dodaj_b_do_a", Cadence::EveryMinute)
            .with_query::<(&mut A, &B), ()>(world),
        SystemDesc::new("test.sumuj_a", Cadence::EveryMinute).with_query::<&A, ()>(world),
        SystemDesc::new("test.pomnoz_c", Cadence::EveryMinute).with_query::<&mut C, ()>(world),
        SystemDesc::new("test.rotacja", Cadence::EveryMinute)
            .with_query::<(Entity, &A), ()>(world)
            .structural(),
    ]
}

fn zbuduj_app(world: World, threads: usize, odwroc: bool) -> App {
    let mut b = ScheduleBuilder::new();
    let mut d = opisy(&world);
    if odwroc {
        d.reverse();
    }
    let licznik = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));
    for desc in d {
        match desc.name {
            "test.dodaj_b_do_a" => b.add(DodajBDoA(desc)),
            "test.sumuj_a" => b.add(SumujA(desc, licznik.clone())),
            "test.pomnoz_c" => b.add(PomnozC(desc)),
            "test.rotacja" => b.add(Rotacja(desc)),
            inne => panic!("nieznany system {inne}"),
        };
    }
    App::new(world, b.build().expect("harmonogram"), threads)
}

/// Odcisk stanu na potrzeby porównań — pełny `world_state_hash` mieszka w `engine/io`.
fn odcisk(world: &mut World) -> u64 {
    let mut h = StateHasher::new();
    let mut wiersze: Vec<(u32, i64, i64, i64)> = world
        .query::<(Entity, &A, Option<&B>, Option<&C>), ()>()
        .iter()
        .map(|(e, a, b, c)| (e.index(), a.0, b.map_or(-1, |v| v.0), c.map_or(-1, |v| v.0)))
        .collect();
    wiersze.sort_unstable();
    for (i, a, b, c) in wiersze {
        h.write_u32(i);
        h.write_i64(a);
        h.write_i64(b);
        h.write_i64(c);
    }
    h.finish().0 as u64
}

#[test]
fn dag_nie_zalezy_od_kolejnosci_rejestracji() {
    // T-D3: ten sam zestaw systemów w różnych permutacjach daje identyczny graf.
    let world = zbuduj_swiat(100);
    let a = zbuduj_app(zbuduj_swiat(100), 4, false);
    let b = zbuduj_app(zbuduj_swiat(100), 4, true);
    assert_eq!(
        a.schedule().fingerprint(),
        b.schedule().fingerprint(),
        "odcisk DAG zależy od kolejności rejestracji"
    );
    assert_eq!(a.schedule().system_names(), b.schedule().system_names());
    drop(world);
}

#[test]
fn wynik_nie_zalezy_od_liczby_watkow() {
    // T-D2: 1 vs 16 wątków, 2000 ticków z systemem strukturalnym i RNG.
    let mut odciski = Vec::new();
    for threads in [1usize, 2, 8, 16] {
        let mut app = zbuduj_app(zbuduj_swiat(2_000), threads, threads % 2 == 0);
        app.run_ticks(2_000);
        odciski.push(odcisk(&mut app.world));
    }
    assert!(
        odciski.windows(2).all(|p| p[0] == p[1]),
        "stan zależy od liczby wątków: {odciski:?}"
    );
}

#[test]
fn cadence_filtruje_wykonania() {
    struct Licznik(SystemDesc, std::sync::Arc<std::sync::atomic::AtomicI64>);
    impl System for Licznik {
        fn desc(&self) -> &SystemDesc {
            &self.0
        }
        fn run(&mut self, _ctx: &mut SystemCtx<'_>) {
            self.1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    let world = World::new(1);
    let minutowy = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));
    let godzinny = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));
    let dobowy = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));
    let mut b = ScheduleBuilder::new();
    b.add(Licznik(
        SystemDesc::new("test.minuta", Cadence::EveryMinute),
        minutowy.clone(),
    ));
    b.add(Licznik(
        SystemDesc::new("test.godzina", Cadence::EveryHour),
        godzinny.clone(),
    ));
    b.add(Licznik(
        SystemDesc::new("test.doba", Cadence::EveryDay),
        dobowy.clone(),
    ));
    let mut app = App::new(world, b.build().unwrap(), 2);
    app.run_ticks(1_440); // doba gry

    use std::sync::atomic::Ordering::Relaxed;
    assert_eq!(minutowy.load(Relaxed), 1_440);
    assert_eq!(godzinny.load(Relaxed), 24);
    assert_eq!(dobowy.load(Relaxed), 1);
}

#[test]
fn cykl_ograniczen_jest_bledem_a_nie_zakleszczeniem() {
    struct Pusty(SystemDesc);
    impl System for Pusty {
        fn desc(&self) -> &SystemDesc {
            &self.0
        }
        fn run(&mut self, _ctx: &mut SystemCtx<'_>) {}
    }

    let a = SystemId::from_name("test.a");
    let b = SystemId::from_name("test.b");
    let mut builder = ScheduleBuilder::new();
    builder.add(Pusty(
        SystemDesc::new("test.a", Cadence::EveryMinute).after(b),
    ));
    builder.add(Pusty(
        SystemDesc::new("test.b", Cadence::EveryMinute).after(a),
    ));
    match builder.build() {
        Err(ScheduleError::OrderingCycle { cycle }) => {
            assert_eq!(cycle.len(), 2, "ścieżka cyklu ma być wypisana: {cycle:?}");
        }
        inne => panic!(
            "oczekiwano cyklu, dostano {inne:?}",
            inne = inne.map(|s| s.fingerprint())
        ),
    }
}

#[test]
fn ograniczenie_do_nieistniejacego_systemu_jest_bledem() {
    struct Pusty(SystemDesc);
    impl System for Pusty {
        fn desc(&self) -> &SystemDesc {
            &self.0
        }
        fn run(&mut self, _ctx: &mut SystemCtx<'_>) {}
    }
    let mut builder = ScheduleBuilder::new();
    builder.add(Pusty(
        SystemDesc::new("test.a", Cadence::EveryMinute).after(SystemId::from_name("nie.ma")),
    ));
    assert!(matches!(
        builder.build(),
        Err(ScheduleError::UnknownConstraint { .. })
    ));
}

#[test]
fn ograniczenie_warunkowe_milczy_gdy_celu_nie_ma_i_dziala_gdy_jest() {
    // `K-51`: scenariusz stawia **wycinek** symulacji, więc system, którego kolejność
    // ma znaczenie tylko wobec systemu opcjonalnego, musi mieć czym to wyrazić.
    // Bez tego `after` wywraca połowę scenariuszy, a jego brak oddaje kolejność
    // hashowi nazwy — czyli przypadkowi, przed którym broni `K-42`.
    struct Pusty(SystemDesc);
    impl System for Pusty {
        fn desc(&self) -> &SystemDesc {
            &self.0
        }
        fn run(&mut self, _ctx: &mut SystemCtx<'_>) {}
    }
    let b = SystemId::from_name("test.b");

    // Bez celu: buduje się, i to jest cała różnica wobec `after`.
    let mut sam = ScheduleBuilder::new();
    sam.add(Pusty(
        SystemDesc::new("test.a", Cadence::EveryMinute).after_if_present(b),
    ));
    let sam = sam.build().expect("brak celu nie jest błędem");

    // Z celem: krawędź powstaje, więc odcisk grafu jest inny niż bez niej.
    let mut para = ScheduleBuilder::new();
    para.add(Pusty(
        SystemDesc::new("test.a", Cadence::EveryMinute).after_if_present(b),
    ));
    para.add(Pusty(SystemDesc::new("test.b", Cadence::EveryMinute)));
    let para = para.build().expect("krawędź warunkowa nie tworzy cyklu");

    let mut luzno = ScheduleBuilder::new();
    luzno.add(Pusty(SystemDesc::new("test.a", Cadence::EveryMinute)));
    luzno.add(Pusty(SystemDesc::new("test.b", Cadence::EveryMinute)));
    let luzno = luzno.build().expect("dwa niezależne systemy");

    assert_ne!(
        para.fingerprint(),
        luzno.fingerprint(),
        "ograniczenie warunkowe nie zbudowało krawędzi mimo obecnego celu"
    );
    let _ = sam.fingerprint();
}

#[test]
fn duplikat_identyfikatora_systemu_jest_bledem() {
    struct Pusty(SystemDesc);
    impl System for Pusty {
        fn desc(&self) -> &SystemDesc {
            &self.0
        }
        fn run(&mut self, _ctx: &mut SystemCtx<'_>) {}
    }
    let mut builder = ScheduleBuilder::new();
    builder.add(Pusty(SystemDesc::new("test.a", Cadence::EveryMinute)));
    builder.add(Pusty(SystemDesc::new("test.a", Cadence::EveryMinute)));
    assert!(matches!(
        builder.build(),
        Err(ScheduleError::DuplicateSystemId { .. })
    ));
}

#[test]
fn sto_tysiecy_komend_daje_ten_sam_stan_niezaleznie_od_kolejnosci_buforow() {
    // T-D6: komendy z 16 „systemów", w tym kolizje (despawn + insert na tej samej encji).
    let zbuduj = |odwroc: bool| {
        let mut w = World::new(7);
        w.register_component::<A>();
        w.register_component::<B>();
        let encje: Vec<Entity> = (0..10_000).map(|i| w.spawn().with(A(i)).id()).collect();

        let mut bufory: Vec<CommandBuffer> = (0..16)
            .map(|i| CommandBuffer::new(SystemId::from_name(&format!("test.sys{i}"))))
            .collect();
        for (i, e) in encje.iter().enumerate() {
            let buf = &mut bufory[i % 16];
            match i % 5 {
                0 => buf.despawn(*e),
                1 => buf.insert(*e, B(i as i64)),
                2 => {
                    // Kolizja: despawn i insert na tej samej encji w jednym ticku.
                    buf.despawn(*e);
                    buf.insert(*e, B(-1));
                }
                3 => {
                    let r = buf.spawn();
                    buf.insert_reserved(r, A(i as i64));
                    buf.insert_reserved(r, B(i as i64 * 2));
                }
                _ => buf.remove::<A>(*e),
            }
        }
        if odwroc {
            bufory.reverse();
        }
        let stats = flush_commands(&mut w, &mut bufory);
        (w, stats)
    };

    let (mut w1, s1) = zbuduj(false);
    let (mut w2, s2) = zbuduj(true);
    assert_eq!(s1, s2, "statystyki flusha zależą od kolejności buforów");
    assert_eq!(odcisk(&mut w1), odcisk(&mut w2));
    assert!(s1.spawned > 0 && s1.despawned > 0 && s1.inserted > 0 && s1.removed > 0);
    // Komendy skierowane do encji zdespawnowanej w tym samym flushu są ciche.
    assert!(s1.dropped_dead > 0);
}

#[test]
fn rezerwacja_z_poprzedniego_ticku_panikuje() {
    let mut w = World::new(1);
    w.register_component::<A>();
    let mut buf = CommandBuffer::new(SystemId::from_name("test.sys"));
    let r = buf.spawn();
    let _ = flush_commands(&mut w, std::slice::from_mut(&mut buf));

    let wynik = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        buf.insert_reserved(r, A(1));
    }));
    assert!(wynik.is_err(), "rezerwacja po flushu musi być wykryta");
}

#[test]
fn zapytanie_w_systemie_widzi_encje_z_poprzedniej_bariery() {
    // Encje utworzone przez system strukturalny stają się widoczne dopiero
    // po barierze — to jest sens dzielenia harmonogramu na etapy.
    let mut app = zbuduj_app(zbuduj_swiat(100), 4, false);
    let przed = app.world.entity_count();
    app.run_ticks(1);
    let po = app.world.entity_count();
    assert_eq!(przed, po, "rotacja usuwa i tworzy tyle samo encji");
    assert!(!app.world.query::<&A, ()>().is_empty());
}
