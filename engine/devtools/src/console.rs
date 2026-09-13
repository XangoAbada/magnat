//! Konsola deweloperska — rejestr komend (M0 §5.10).
//!
//! Przestrzeń nazw `<obszar>.<akcja>`: `ecs.archetypes`, `sim.step`, `prof.frame`.
//! **Każda faza dopisuje swoje komendy przez `register`**, zamiast budować własną
//! konsolę — inaczej po dwunastu fazach byłoby dwanaście sposobów zadania pytania
//! „co siedzi w tej encji".

use crate::inspector::Inspector;
use magnat_core::collections::{seeded_map, SeededMap};
use magnat_core::{Entity, SimCalendar};
use magnat_ecs::World;
use magnat_io::world_state_hash;

pub type ConsoleFn = fn(&mut World, &[&str]) -> String;

struct ConsoleCommand {
    help: &'static str,
    f: ConsoleFn,
}

pub struct Console {
    commands: SeededMap<&'static str, ConsoleCommand>,
}

impl Default for Console {
    fn default() -> Self {
        Console::with_builtins()
    }
}

impl Console {
    #[must_use]
    pub fn new() -> Console {
        Console {
            commands: seeded_map(),
        }
    }

    #[must_use]
    pub fn with_builtins() -> Console {
        let mut c = Console::new();
        c.register(
            "ecs.archetypes",
            "lista archetypów z licznikami i zajętością pamięci",
            |world, _| Inspector::archetypes(world),
        );
        c.register(
            "ecs.entity",
            "ecs.entity <indeks> — zrzut wszystkich komponentów encji",
            |world, args| {
                let Some(index) = args.first().and_then(|a| a.parse::<u32>().ok()) else {
                    return "użycie: ecs.entity <indeks>\n".to_string();
                };
                match world
                    .entities()
                    .iter_alive()
                    .find(|e: &Entity| e.index() == index)
                {
                    Some(e) => Inspector::entity(world, e),
                    None => format!("encja {index} nie żyje\n"),
                }
            },
        );
        c.register("ecs.hash", "hash stanu świata", |world, _| {
            format!("{}\n", world_state_hash(world))
        });
        c.register("sim.time", "data i godzina gry", |world, _| {
            let cal = SimCalendar::new(world.tick);
            format!(
                "tick {} · rok {} · {:02}.{:02} · {:02}:{:02} · {:?}\n",
                world.tick.0,
                cal.year(),
                cal.day_of_month(),
                cal.month_of_year(),
                cal.hour_of_day(),
                cal.minute_of_hour(),
                cal.day_of_week()
            )
        });
        c.register(
            "prof.frame",
            "czy build ma wkompilowany profiler",
            |_, _| {
                format!(
                    "profiler: {}\n",
                    if crate::profile::enabled() {
                        "włączony (tracy)"
                    } else {
                        "wyłączony — zbuduj z --features profiling"
                    }
                )
            },
        );
        c
    }

    pub fn register(&mut self, name: &'static str, help: &'static str, f: ConsoleFn) {
        assert!(
            self.commands
                .insert(name, ConsoleCommand { help, f })
                .is_none(),
            "komenda {name} zarejestrowana dwa razy"
        );
    }

    /// Wykonuje linię. Nieznana komenda nie jest błędem programu — konsola jest
    /// interaktywna, a literówka ma dać podpowiedź, nie panikę.
    pub fn exec(&mut self, world: &mut World, line: &str) -> String {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            return String::new();
        };
        if name == "help" {
            return self.help();
        }
        let args: Vec<&str> = parts.collect();
        match self.commands.get(name) {
            Some(cmd) => (cmd.f)(world, &args),
            None => format!("nieznana komenda {name:?}; wpisz help\n"),
        }
    }

    #[must_use]
    pub fn help(&self) -> String {
        let mut nazwy: Vec<(&str, &str)> = self
            .commands
            .iter()
            .map(|(name, cmd)| (*name, cmd.help))
            .collect();
        nazwy.sort_unstable();
        nazwy
            .into_iter()
            .map(|(name, help)| format!("{name:<18} {help}\n"))
            .collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
