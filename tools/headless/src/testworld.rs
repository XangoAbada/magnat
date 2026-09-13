//! Świat syntetyczny — **część kontraktu testowego, nie zabawka** (M0 §5.11).
//!
//! Sześć archetypów o wierszach od 24 do 400 B, dwanaście komponentów i osiem systemów
//! o profilach dostępu odwzorowujących przyszłą symulację: czytaj-wiele/pisz-jeden,
//! dwa systemy czysto odczytowe biegnące równolegle, jeden strukturalny rotujący
//! ok. 0,1 % encji na tick, jeden losujący per encja, jeden godzinny, jeden dobowy.
//!
//! **Fazy M1+ nie modyfikują tego świata** — dopisują własne scenariusze obok.
//! Gdyby go ruszyły, wyniki benchmarków przestałyby być porównywalne między fazami,
//! a to jedyna linia bazowa, jaką mamy.

use magnat_core::{rng, Cadence, Entity, HashState, Money, Mood, StateHasher, StreamId, Q};
use magnat_ecs::{Component, ScheduleBuilder, System, SystemCtx, SystemDesc, World};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

macro_rules! komponent {
    ($(#[$meta:meta])* $name:ident { $($pole:ident : $typ:ty),* $(,)? }, $hash:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct $name { $(pub $pole: $typ),* }
        impl HashState for $name {
            fn hash_state(&self, h: &mut StateHasher) {
                #[allow(clippy::redundant_closure_call)]
                ($hash)(self, h);
            }
        }
        impl Component for $name {
            const NAME: &'static str = stringify!($name);
        }
    };
}

komponent!(
    /// 16 B — najczęściej czytany komponent świata testowego.
    Position { x: i64, y: i64 },
    |s: &Position, h: &mut StateHasher| {
        h.write_i64(s.x);
        h.write_i64(s.y);
    }
);

komponent!(
    Velocity { dx: i32, dy: i32 },
    |s: &Velocity, h: &mut StateHasher| {
        h.write_u32(s.dx as u32);
        h.write_u32(s.dy as u32);
    }
);

komponent!(
    Wallet { amount: Money },
    |s: &Wallet, h: &mut StateHasher| s.amount.hash_state(h)
);

komponent!(
    Needs {
        hunger: Q,
        rest: Q,
        fun: Q
    },
    |s: &Needs, h: &mut StateHasher| {
        s.hunger.hash_state(h);
        s.rest.hash_state(h);
        s.fun.hash_state(h);
    }
);

komponent!(Age { minutes: u64 }, |s: &Age, h: &mut StateHasher| h
    .write_u64(s.minutes));

komponent!(
    Employer { firm: Entity },
    |s: &Employer, h: &mut StateHasher| h.write_u64(s.firm.to_bits())
);

komponent!(
    /// 32 B — koszyk towarów.
    Inventory { slots: [i32; 8] },
    |s: &Inventory, h: &mut StateHasher| {
        for v in s.slots {
            h.write_u32(v as u32);
        }
    }
);

komponent!(
    /// 24 B — plan doby w godzinach.
    DailyPlan { hours: [u8; 24] },
    |s: &DailyPlan, h: &mut StateHasher| h.write(&s.hours)
);

komponent!(
    Reputation { mood: Mood },
    |s: &Reputation, h: &mut StateHasher| s.mood.hash_state(h)
);

komponent!(
    Label { bytes: [u8; 16] },
    |s: &Label, h: &mut StateHasher| h.write(&s.bytes)
);

komponent!(
    /// 400 B — wiersz odniesienia dla budżetu pamięci z PRD §17.7.
    Bulk { data: [u8; 400] },
    |s: &Bulk, h: &mut StateHasher| h.write(&s.data)
);

komponent!(Ticker { value: u64 }, |s: &Ticker, h: &mut StateHasher| h
    .write_u64(s.value));

/// Buduje świat deterministycznie z ziarna. Sześć archetypów, rozkład encji stały.
#[must_use]
pub fn build(seed: u64, entities: u32) -> World {
    let mut w = World::new(seed);
    w.register_component::<Position>();
    w.register_component::<Velocity>();
    w.register_component::<Wallet>();
    w.register_component::<Needs>();
    w.register_component::<Age>();
    w.register_component::<Employer>();
    w.register_component::<Inventory>();
    w.register_component::<DailyPlan>();
    w.register_component::<Reputation>();
    w.register_component::<Label>();
    w.register_component::<Bulk>();
    w.register_component::<Ticker>();

    for i in 0..entities {
        let mut r = rng(seed, StreamId::EngineSelfTest, i, magnat_core::Tick(0));
        let x = i64::from(i);
        let pozycja = Position {
            x,
            y: x.wrapping_mul(7),
        };
        match i % 6 {
            0 => {
                let _ = w
                    .spawn()
                    .with(pozycja)
                    .with(Velocity {
                        dx: r.gen_range_u32(11) as i32 - 5,
                        dy: r.gen_range_u32(11) as i32 - 5,
                    })
                    .with(Ticker { value: 0 })
                    .id();
            }
            1 => {
                let _ = w
                    .spawn()
                    .with(pozycja)
                    .with(Wallet {
                        amount: Money(i64::from(r.gen_range_u32(100_000))),
                    })
                    .with(Needs {
                        hunger: r.gen_q(),
                        rest: r.gen_q(),
                        fun: r.gen_q(),
                    })
                    .with(Age {
                        minutes: u64::from(r.gen_range_u32(1_000_000)),
                    })
                    .id();
            }
            2 => {
                let firma = Entity::from_bits(0x0000_0001_0000_0000 | u64::from(i % 97))
                    .expect("encja pracodawcy");
                let _ = w
                    .spawn()
                    .with(pozycja)
                    .with(Wallet {
                        amount: Money(i64::from(r.gen_range_u32(50_000))),
                    })
                    .with(Needs {
                        hunger: r.gen_q(),
                        rest: r.gen_q(),
                        fun: r.gen_q(),
                    })
                    .with(Age {
                        minutes: u64::from(r.gen_range_u32(1_000_000)),
                    })
                    .with(Employer { firm: firma })
                    .with(Inventory {
                        slots: [r.gen_range_u32(100) as i32; 8],
                    })
                    .id();
            }
            3 => {
                let _ = w
                    .spawn()
                    .with(pozycja)
                    .with(DailyPlan {
                        hours: [(i % 7) as u8; 24],
                    })
                    .with(Reputation {
                        mood: Mood::new(r.gen_range_u32(201) as i8 - 100),
                    })
                    .with(Label {
                        bytes: [(i % 251) as u8; 16],
                    })
                    .id();
            }
            4 => {
                let _ = w
                    .spawn()
                    .with(pozycja)
                    .with(Bulk {
                        data: [(i % 253) as u8; 400],
                    })
                    .with(Ticker { value: 1 })
                    .id();
            }
            _ => {
                let _ = w
                    .spawn()
                    .with(Wallet {
                        amount: Money(i64::from(r.gen_range_u32(10_000))),
                    })
                    .with(Ticker { value: 2 })
                    .with(Age {
                        minutes: u64::from(i),
                    })
                    .id();
            }
        }
    }
    w
}

// ── Systemy ─────────────────────────────────────────────────────────────────────

struct Ruch(SystemDesc);
impl System for Ruch {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        for (p, v) in ctx.query::<(&mut Position, &Velocity), ()>().iter() {
            p.x = p.x.wrapping_add(i64::from(v.dx));
            p.y = p.y.wrapping_add(i64::from(v.dy));
        }
    }
}

struct Zegar(SystemDesc);
impl System for Zegar {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        for t in ctx.query::<&mut Ticker, ()>().iter() {
            t.value = t.value.wrapping_add(1);
        }
    }
}

struct Potrzeby(SystemDesc);
impl System for Potrzeby {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        // Test negatywny T-D8: iteracja po `HashMap` wnosi niedeterminizm zależny
        // od ziarna haszera, które std losuje przy starcie procesu. Ten kod istnieje
        // po to, żeby sprawdzić, że testy determinizmu w ogóle działają — bez niego
        // „zielony CI" znaczyłby tylko tyle, że nikt niczego nie złamał w sposób,
        // który akurat umiemy wykryć.
        #[cfg(feature = "chaos")]
        let zaklocenie = {
            #[allow(clippy::disallowed_types)]
            let mut mapa: std::collections::HashMap<u64, u8> = std::collections::HashMap::new();
            for i in 0..64u64 {
                mapa.insert(i.wrapping_mul(2_654_435_761), i as u8);
            }
            let mut mieszanka = 0u8;
            for (k, v) in &mapa {
                mieszanka = mieszanka.wrapping_mul(31).wrapping_add((*k as u8) ^ v);
            }
            mieszanka % 3
        };
        #[cfg(not(feature = "chaos"))]
        let zaklocenie = 0u8;

        for (n, a) in ctx.query::<(&mut Needs, &Age), ()>().iter() {
            let krok = ((a.minutes % 3) + 1) as u8 + zaklocenie;
            n.hunger = n.hunger.saturating_sub(krok);
            n.rest = n.rest.saturating_sub(1);
            if n.hunger.get() == 0 {
                n.hunger = Q::MAX;
            }
        }
    }
}

/// System godzinny: płaca za godzinę pracy.
struct Wyplata(SystemDesc);
impl System for Wyplata {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        for (w, n) in ctx.query::<(&mut Wallet, &Needs), ()>().iter() {
            // Pieniądz w groszach, arytmetyka całkowita (00 §2).
            let stawka = Money(i64::from(n.rest.get()) * 7);
            w.amount = w.amount.checked_add(stawka).unwrap_or(w.amount);
        }
    }
}

/// System dobowy: starzenie.
struct Starzenie(SystemDesc);
impl System for Starzenie {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        for a in ctx.query::<&mut Age, ()>().iter() {
            a.minutes = a.minutes.wrapping_add(1_440);
        }
    }
}

/// Pierwszy z dwóch systemów czysto odczytowych — biegną równolegle,
/// bo nie konfliktują z niczym.
struct SumaPozycji(SystemDesc, Arc<AtomicI64>);
impl System for SumaPozycji {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let pool = ctx.pool;
        let suma = ctx.query::<&Position, ()>().par_fold(
            pool,
            |view| {
                let mut s = 0i64;
                while let Some(p) = view.next_row() {
                    s = s.wrapping_add(p.x).wrapping_add(p.y);
                }
                s
            },
            |acc: i64, v| acc.wrapping_add(v),
            0,
        );
        self.1.store(suma, Ordering::Relaxed);
    }
}

/// Drugi system odczytowy.
struct SumaPortfeli(SystemDesc, Arc<AtomicI64>);
impl System for SumaPortfeli {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let mut suma = 0i64;
        for w in ctx.query::<&Wallet, ()>().iter() {
            suma = suma.wrapping_add(w.amount.0);
        }
        self.1.store(suma, Ordering::Relaxed);
    }
}

/// System strukturalny: rotuje ok. 0,1 % encji na tick i losuje per encja.
struct Rotacja(SystemDesc);
impl System for Rotacja {
    fn desc(&self) -> &SystemDesc {
        &self.0
    }
    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let tick = ctx.tick;
        let seed = ctx.seed;
        let do_rotacji: Vec<(Entity, i64)> = ctx
            .query::<(Entity, &Position), ()>()
            .iter()
            .filter_map(|(e, p)| {
                let mut r = rng(seed, StreamId::EngineSelfTest, e.index(), tick);
                r.gen_bool_permille(1).then_some((e, p.x))
            })
            .collect();
        for (e, _) in &do_rotacji {
            ctx.cmd.despawn(*e);
        }
        for (i, (_, x)) in do_rotacji.iter().enumerate() {
            let r = ctx.cmd.spawn();
            ctx.cmd.insert_reserved(
                r,
                Position {
                    x: x.wrapping_add(i as i64),
                    y: tick.0 as i64,
                },
            );
            ctx.cmd.insert_reserved(r, Ticker { value: tick.0 });
        }
    }
}

/// Rejestruje osiem systemów świata testowego.
pub fn schedule(world: &World) -> ScheduleBuilder {
    let mut b = ScheduleBuilder::new();
    b.add(Ruch(
        SystemDesc::new("test.ruch", Cadence::EveryMinute)
            .with_query::<(&mut Position, &Velocity), ()>(world),
    ));
    b.add(Zegar(
        SystemDesc::new("test.zegar", Cadence::EveryMinute).with_query::<&mut Ticker, ()>(world),
    ));
    b.add(Potrzeby(
        SystemDesc::new("test.potrzeby", Cadence::EveryMinute)
            .with_query::<(&mut Needs, &Age), ()>(world),
    ));
    b.add(Wyplata(
        SystemDesc::new("test.wyplata", Cadence::EveryHour)
            .with_query::<(&mut Wallet, &Needs), ()>(world),
    ));
    b.add(Starzenie(
        SystemDesc::new("test.starzenie", Cadence::EveryDay).with_query::<&mut Age, ()>(world),
    ));
    b.add(SumaPozycji(
        SystemDesc::new("test.suma_pozycji", Cadence::EveryMinute)
            .with_query::<&Position, ()>(world),
        Arc::new(AtomicI64::new(0)),
    ));
    b.add(SumaPortfeli(
        SystemDesc::new("test.suma_portfeli", Cadence::EveryMinute)
            .with_query::<&Wallet, ()>(world),
        Arc::new(AtomicI64::new(0)),
    ));
    b.add(Rotacja(
        SystemDesc::new("test.rotacja", Cadence::EveryMinute)
            .with_query::<(Entity, &Position), ()>(world)
            .structural(),
    ));
    b
}
