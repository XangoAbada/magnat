//! Inspektor ECS — tekstowy zrzut stanu (M0 §5.10).
//!
//! Karty inspekcji z M3+ budują się **nad tym**, a nie zamiast tego: tu jest surowa
//! prawda o tym, co siedzi w pamięci, i ona musi działać także wtedy, gdy UI nie działa.

use magnat_core::Entity;
use magnat_ecs::World;
use std::fmt::Write;

pub struct Inspector;

impl Inspector {
    /// Lista archetypów: zestaw komponentów, liczba encji, liczba chunków, bajty.
    #[must_use]
    pub fn archetypes(world: &World) -> String {
        let reg = world.components();
        let mut out = String::new();
        let mut wiersze: Vec<(String, u32, usize, usize)> = world
            .archetypes()
            .iter()
            .filter(|a| !a.is_empty())
            .map(|a| {
                let nazwy: Vec<&str> = a.components().iter().map(|c| reg.info(*c).name()).collect();
                (
                    nazwy.join("+"),
                    a.rows(),
                    a.chunk_count(),
                    a.allocated_bytes(),
                )
            })
            .collect();
        wiersze.sort();

        let _ = writeln!(
            out,
            "{:<40} {:>10} {:>8} {:>12}",
            "archetyp", "encje", "chunki", "bajty"
        );
        let mut suma_encji = 0u32;
        let mut suma_bajtow = 0usize;
        for (nazwa, encje, chunki, bajty) in &wiersze {
            let _ = writeln!(out, "{nazwa:<40} {encje:>10} {chunki:>8} {bajty:>12}");
            suma_encji += encje;
            suma_bajtow += bajty;
        }
        let _ = writeln!(
            out,
            "{:<40} {:>10} {:>8} {:>12}",
            "RAZEM",
            suma_encji,
            wiersze.iter().map(|w| w.2).sum::<usize>(),
            suma_bajtow
        );
        let _ = writeln!(
            out,
            "tick {} · seed {} · encje żywe {}",
            world.tick.0,
            world.seed,
            world.entity_count()
        );
        out
    }

    /// Pełny zrzut jednej encji: wszystkie komponenty przez `Debug` z rejestru.
    #[must_use]
    pub fn entity(world: &World, e: Entity) -> String {
        let Some(loc) = world.entities().location(e) else {
            return format!(
                "encja {}v{} nie żyje (albo nigdy nie istniała)\n",
                e.index(),
                e.generation()
            );
        };
        let reg = world.components();
        let arch = world.archetypes().get(loc.archetype);
        let chunk = arch.chunk(loc.chunk as usize);
        let mut out = format!(
            "encja {}v{} · archetyp {} · chunk {} · wiersz {}\n",
            e.index(),
            e.generation(),
            loc.archetype.get(),
            loc.chunk,
            loc.row
        );
        let mut komponenty: Vec<(&str, String)> = arch
            .components()
            .iter()
            .map(|c| {
                let info = reg.info(*c);
                let col = arch.column_index(*c).expect("kolumna archetypu");
                (info.name(), info.debug_in_chunk(chunk, col, loc.row))
            })
            .collect();
        komponenty.sort();
        for (nazwa, wartosc) in komponenty {
            let _ = writeln!(out, "  {nazwa:<20} {wartosc}");
        }
        out
    }

    /// Różnica dwóch światów — raport rozbieżności determinizmu (WP-12).
    ///
    /// Porównuje **po indeksie encji**, bo to jest porządek, w którym liczony jest
    /// hash stanu: wskazanie „tick 17 000" bez wskazania encji i komponentu nie
    /// skraca szukania przyczyny ani o godzinę.
    #[must_use]
    pub fn diff(a: &World, b: &World, max_entities: usize) -> String {
        let mut out = String::new();
        let indeksy_a: Vec<u32> = a.entities().iter_alive().map(|e| e.index()).collect();
        let indeksy_b: Vec<u32> = b.entities().iter_alive().map(|e| e.index()).collect();

        if indeksy_a.len() != indeksy_b.len() {
            let _ = writeln!(
                out,
                "różna liczba encji: {} vs {}",
                indeksy_a.len(),
                indeksy_b.len()
            );
        }
        for index in &indeksy_a {
            if !indeksy_b.contains(index) {
                let _ = writeln!(out, "encja {index} istnieje tylko w pierwszym świecie");
            }
        }
        for index in &indeksy_b {
            if !indeksy_a.contains(index) {
                let _ = writeln!(out, "encja {index} istnieje tylko w drugim świecie");
            }
        }

        let mut roznice = 0usize;
        for index in indeksy_a.iter().filter(|i| indeksy_b.contains(i)) {
            let ea = a
                .entities()
                .iter_alive()
                .find(|e| e.index() == *index)
                .expect("encja z listy żywych");
            let eb = b
                .entities()
                .iter_alive()
                .find(|e| e.index() == *index)
                .expect("encja z listy żywych");
            let zrzut_a = Inspector::entity(a, ea);
            let zrzut_b = Inspector::entity(b, eb);
            // Porównujemy same komponenty: numer chunka i wiersza może się różnić
            // przy tym samym stanie logicznym.
            let ciala: (Vec<&str>, Vec<&str>) = (
                zrzut_a.lines().skip(1).collect(),
                zrzut_b.lines().skip(1).collect(),
            );
            if ciala.0 != ciala.1 {
                roznice += 1;
                if roznice <= max_entities {
                    let _ = writeln!(out, "── encja {index} ──");
                    for (la, lb) in ciala.0.iter().zip(&ciala.1) {
                        if la != lb {
                            let _ = writeln!(out, "  A: {la}\n  B: {lb}");
                        }
                    }
                }
            }
        }
        if roznice > max_entities {
            let _ = writeln!(
                out,
                "... i {} dalszych encji z różnicami",
                roznice - max_entities
            );
        }
        if out.is_empty() {
            out.push_str("brak różnic\n");
        }
        out
    }
}
