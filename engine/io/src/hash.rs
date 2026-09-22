//! Hash stanu świata (00 §3.6, M0 §5.9, WP-09).
//!
//! Typy `StateHash`, `StateHasher` i trait `HashState` mieszkają w `magnat-core`
//! (uzasadnienie w `core::hash`) — tutaj są re-eksportowane, żeby nazwy z kontraktu
//! §6.5 wskazywały na `magnat-io`, oraz dołożona jest jedyna funkcja, która
//! naprawdę potrzebuje `World`.

pub use magnat_core::hash::{HashState, StateHash, StateHasher};

use magnat_ecs::{ArchetypeId, World};

/// Hash całego stanu ECS. **Kolejność jest częścią kontraktu:**
///
/// - archetypy po kanonicznym zestawie **nazw** komponentów (leksykograficznie),
///   nie po `ComponentId` — te nie są stabilne między uruchomieniami,
/// - w archetypie encje po `entity.index()` rosnąco,
/// - w encji komponenty po `Component::NAME`,
/// - potem areny w kolejności `ArenaKind`, na końcu zasoby po nazwie typu.
///
/// Encje martwe i puste archetypy nie wchodzą. **Generacje encji też nie** — dzięki
/// temu ten sam stan logiczny osiągnięty inną historią spawnów daje ten sam hash
/// (test T-D5), co jest sensem tej funkcji: wykrywać rozjazd stanu, a nie rozjazd
/// historii, która do niego doprowadziła.
#[must_use]
pub fn world_state_hash(world: &World) -> StateHash {
    let mut h = StateHasher::new();
    for (names, id) in archetypy(world) {
        hash_archetypu(world, &names, id, &mut h);
    }
    for (kind, hook) in world.state_hooks().arenas() {
        h.write_u16(kind as u16);
        hook(world, &mut h);
    }
    for (name, hook) in world.state_hooks().resources() {
        h.write(name.as_bytes());
        h.write_u8(0);
        hook(world, &mut h);
    }
    h.finish()
}

/// Hash stanu **w częściach**: osobno każdy archetyp (nazwany składem komponentów,
/// np. `Pos+Wallet`), każda arena (`arena:<numer>`) i każdy zasób (`res:<nazwa>`).
///
/// Istnieje dla raportu rozbieżności (N1.11, `M0#8`): sam `world_state_hash` mówi
/// „coś się rozjechało na ticku N", a części mówią **co**. Kolejność i reguła
/// hashowania są te same co w [`world_state_hash`] — obie funkcje wołają
/// `hash_archetypu` — więc część jest równa wtedy i tylko wtedy, gdy równy jest
/// jej wkład do hasha całości.
#[must_use]
pub fn state_hash_parts(world: &World) -> Vec<(String, StateHash)> {
    let mut czesci = Vec::new();
    for (names, id) in archetypy(world) {
        let mut h = StateHasher::new();
        hash_archetypu(world, &names, id, &mut h);
        czesci.push((names.join("+"), h.finish()));
    }
    for (kind, hook) in world.state_hooks().arenas() {
        let mut h = StateHasher::new();
        hook(world, &mut h);
        czesci.push((format!("arena:{}", kind as u16), h.finish()));
    }
    for (name, hook) in world.state_hooks().resources() {
        let mut h = StateHasher::new();
        hook(world, &mut h);
        czesci.push((format!("res:{name}"), h.finish()));
    }
    czesci
}

/// Niepuste archetypy w kolejności nazw komponentów.
fn archetypy(world: &World) -> Vec<(Vec<&'static str>, ArchetypeId)> {
    let reg = world.components();
    let mut archetypes: Vec<(Vec<&'static str>, ArchetypeId)> = world
        .archetypes()
        .iter()
        .filter(|a| !a.is_empty())
        .map(|a| {
            (
                a.components().iter().map(|c| reg.info(*c).name()).collect(),
                a.id(),
            )
        })
        .collect();
    archetypes.sort();
    archetypes
}

/// Wkład jednego archetypu: nazwy komponentów, potem wiersze po indeksie encji.
fn hash_archetypu(world: &World, names: &[&'static str], id: ArchetypeId, h: &mut StateHasher) {
    let reg = world.components();
    {
        for n in names {
            h.write(n.as_bytes());
            h.write_u8(0);
        }
        let arch = world.archetypes().get(id);

        // Kolumny w kolejności nazw komponentów.
        let mut columns: Vec<(&'static str, usize)> = arch
            .components()
            .iter()
            .map(|c| {
                (
                    reg.info(*c).name(),
                    arch.column_index(*c).expect("kolumna archetypu"),
                )
            })
            .collect();
        columns.sort_unstable();
        let infos: Vec<_> = arch.components().iter().map(|c| reg.info(*c)).collect();
        let column_info: Vec<_> = columns
            .iter()
            .map(|(name, col)| {
                (
                    *col,
                    infos
                        .iter()
                        .find(|i| i.name() == *name)
                        .expect("info kolumny"),
                )
            })
            .collect();

        // Wiersze po indeksie encji.
        let mut rows: Vec<(u32, u16, u16)> = Vec::with_capacity(arch.rows() as usize);
        for (chunk_index, chunk) in arch.chunks().iter().enumerate() {
            for (row, e) in chunk.entities().iter().enumerate() {
                rows.push((e.index(), chunk_index as u16, row as u16));
            }
        }
        rows.sort_unstable();

        for (entity_index, chunk_index, row) in rows {
            h.write_u32(entity_index);
            let chunk = arch.chunk(chunk_index as usize);
            for (col, info) in &column_info {
                info.hash_in_chunk(chunk, *col, row, h);
            }
        }
    }
}
