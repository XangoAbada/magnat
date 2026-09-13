//! `CommandBuffer` — mutacje strukturalne jako komendy (00 §3.4, M0 §5.8).
//!
//! W trakcie ticku nikt nie mutuje struktury świata bezpośrednio. System zgłasza
//! komendę do **własnego** bufora (bez zamka, bez współdzielenia), a wszystkie bufory
//! są aplikowane w barierze, po posortowaniu kluczy `(SystemId, target, seq)`.
//! Dzięki temu wynik nie zależy od tego, który worker skończył pierwszy.

use crate::component::Component;
use crate::system::SystemId;
use crate::world::World;
use magnat_core::Entity;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
enum CommandKind {
    Despawn = 0,
    Remove = 1,
    Spawn = 2,
    Insert = 3,
}

/// Klucz sortowania. Osobno od ładunku, żeby sortować 32 B, a nie komponenty.
#[derive(Clone, Copy)]
struct CommandKey {
    system: SystemId,
    /// Encja → `entity.index()`, rezerwacja → `2^32 + seq`. Porządek jest tym samym
    /// porządkiem, w którym encje leżą w archetypach.
    target: u64,
    /// Kolejność zgłoszenia w obrębie systemu — rozstrzyga remisy.
    seq: u32,
    kind: CommandKind,
    /// Pozycja ładunku w blobie (tylko `Insert`).
    offset: u32,
    entity: Option<Entity>,
    reservation: u32,
    apply_fn: Option<unsafe fn(&mut World, Entity, *mut u8)>,
    remove_fn: Option<fn(&mut World, Entity)>,
    drop_fn: Option<unsafe fn(*mut u8)>,
}

impl CommandKey {
    fn sort_key(&self) -> (SystemId, u64, u32, CommandKind) {
        (self.system, self.target, self.seq, self.kind)
    }
}

/// Uchwyt do encji, która jeszcze nie istnieje.
///
/// **Odstępstwo od planu M0 §5.8, świadome:** plan chciał, żeby użycie rezerwacji
/// po flushu było błędem **kompilacji**. Osiągnięcie tego wymagałoby powiązania
/// rezerwacji czasem życia bufora, a wtedy `cmd.insert_reserved(r, v)` nie mogłoby
/// się skompilować w ogóle (rezerwacja trzymałaby pożyczkę mutowalną bufora,
/// którą `insert_reserved` musi wziąć drugi raz). Zamiast tego rezerwacja niesie
/// **epokę** bufora: użycie po flushu panikuje z komunikatem wskazującym system,
/// zamiast po cichu wstawić komponent do cudzej encji.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EntityReservation {
    system: SystemId,
    seq: u32,
    epoch: u32,
}

pub struct CommandBuffer {
    system: SystemId,
    /// Ładunki komend w płaskim blobie — bez `Box`, bez alokacji per komenda.
    blob: Vec<u8>,
    index: Vec<CommandKey>,
    next_seq: u32,
    next_reservation: u32,
    epoch: u32,
}

impl CommandBuffer {
    #[must_use]
    pub fn new(system: SystemId) -> CommandBuffer {
        CommandBuffer {
            system,
            blob: Vec::new(),
            index: Vec::new(),
            next_seq: 0,
            next_reservation: 0,
            epoch: 0,
        }
    }

    #[must_use]
    pub fn system(&self) -> SystemId {
        self.system
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    fn next_seq(&mut self) -> u32 {
        let s = self.next_seq;
        self.next_seq += 1;
        s
    }

    /// Rezerwuje encję, która powstanie w najbliższej barierze.
    pub fn spawn(&mut self) -> EntityReservation {
        let reservation = self.next_reservation;
        self.next_reservation += 1;
        let seq = self.next_seq();
        self.index.push(CommandKey {
            system: self.system,
            target: (1u64 << 32) + u64::from(reservation),
            seq,
            kind: CommandKind::Spawn,
            offset: 0,
            entity: None,
            reservation,
            apply_fn: None,
            remove_fn: None,
            drop_fn: None,
        });
        EntityReservation {
            system: self.system,
            seq: reservation,
            epoch: self.epoch,
        }
    }

    pub fn despawn(&mut self, e: Entity) {
        let seq = self.next_seq();
        self.index.push(CommandKey {
            system: self.system,
            target: u64::from(e.index()),
            seq,
            kind: CommandKind::Despawn,
            offset: 0,
            entity: Some(e),
            reservation: 0,
            apply_fn: None,
            remove_fn: None,
            drop_fn: None,
        });
    }

    pub fn insert<T: Component>(&mut self, e: Entity, value: T) {
        let (offset, drop_fn) = self.push_payload(value);
        let seq = self.next_seq();
        self.index.push(CommandKey {
            system: self.system,
            target: u64::from(e.index()),
            seq,
            kind: CommandKind::Insert,
            offset,
            entity: Some(e),
            reservation: 0,
            apply_fn: Some(apply_insert::<T>),
            remove_fn: None,
            drop_fn,
        });
    }

    pub fn insert_reserved<T: Component>(&mut self, r: EntityReservation, value: T) {
        assert_eq!(
            r.epoch, self.epoch,
            "rezerwacja z poprzedniego ticku użyta po flushu — encja, do której \
             miała trafić, już istnieje i ma inny numer"
        );
        assert_eq!(r.system, self.system, "rezerwacja z bufora innego systemu");
        let (offset, drop_fn) = self.push_payload(value);
        let seq = self.next_seq();
        self.index.push(CommandKey {
            system: self.system,
            target: (1u64 << 32) + u64::from(r.seq),
            seq,
            kind: CommandKind::Insert,
            offset,
            entity: None,
            reservation: r.seq,
            apply_fn: Some(apply_insert::<T>),
            remove_fn: None,
            drop_fn,
        });
    }

    pub fn remove<T: Component>(&mut self, e: Entity) {
        let seq = self.next_seq();
        self.index.push(CommandKey {
            system: self.system,
            target: u64::from(e.index()),
            seq,
            kind: CommandKind::Remove,
            offset: 0,
            entity: Some(e),
            reservation: 0,
            apply_fn: None,
            remove_fn: Some(apply_remove::<T>),
            drop_fn: None,
        });
    }

    fn push_payload<T>(&mut self, value: T) -> (u32, Option<unsafe fn(*mut u8)>) {
        // Wyrównanie do 8 B: odczyt i tak idzie przez `read_unaligned`, ale równe
        // offsety ułatwiają czytanie zrzutów bufora w debugerze.
        let padding = (8 - (self.blob.len() % 8)) % 8;
        self.blob.resize(self.blob.len() + padding, 0);
        let offset = u32::try_from(self.blob.len()).expect("bufor komend ponad 4 GiB");
        let size = size_of::<T>();
        self.blob.resize(self.blob.len() + size, 0);
        // SAFETY: właśnie zarezerwowaliśmy `size` bajtów pod tym offsetem;
        // zapis niewyrównany jest jawny, więc wyrównanie bloba nie ma znaczenia.
        unsafe {
            self.blob
                .as_mut_ptr()
                .add(offset as usize)
                .cast::<T>()
                .write_unaligned(value);
        }
        let drop_fn = if std::mem::needs_drop::<T>() {
            Some(drop_payload::<T> as unsafe fn(*mut u8))
        } else {
            None
        };
        (offset, drop_fn)
    }

    /// Porzuca niezaaplikowane komendy, wywołując destruktory ładunków.
    pub fn clear(&mut self) {
        for key in &self.index {
            if let (CommandKind::Insert, Some(drop_fn)) = (key.kind, key.drop_fn) {
                // SAFETY: ładunek leży pod tym offsetem i nie został jeszcze przeniesiony.
                unsafe { drop_fn(self.blob.as_mut_ptr().add(key.offset as usize)) };
            }
        }
        self.reset();
    }

    /// Czyści bufor **bez** destruktorów — po flushu, gdy ładunki trafiły do świata.
    fn reset(&mut self) {
        self.index.clear();
        self.blob.clear();
        self.next_seq = 0;
        self.next_reservation = 0;
        self.epoch = self.epoch.wrapping_add(1);
    }
}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        // Bufor porzucony bez flusha nie ma prawa wyciec komponentami.
        self.clear();
    }
}

unsafe fn apply_insert<T: Component>(world: &mut World, e: Entity, payload: *mut u8) {
    // SAFETY: `payload` wskazuje na zapisaną wcześniej wartość typu T; odczyt
    // przenosi ją (blob nie będzie już czytany pod tym offsetem).
    let value = unsafe { payload.cast::<T>().read_unaligned() };
    world.insert(e, value);
}

fn apply_remove<T: Component>(world: &mut World, e: Entity) {
    let _ = world.remove::<T>(e);
}

unsafe fn drop_payload<T>(payload: *mut u8) {
    // SAFETY: pod adresem leży niezaaplikowana wartość typu T.
    drop(unsafe { payload.cast::<T>().read_unaligned() });
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct FlushStats {
    pub spawned: u32,
    pub despawned: u32,
    pub inserted: u32,
    pub removed: u32,
    /// Komendy skierowane do encji, która już nie żyje. **Nie są błędem**:
    /// panika przy przegranym wyścigu byłaby niedeterministyczna względem
    /// kolejności zgłoszeń, której i tak nie kontrolujemy (M0 §5.8).
    pub dropped_dead: u32,
}

/// Aplikacja komend w barierze. Wywoływana wyłącznie przez `App::tick`, jednowątkowo.
///
/// Kolejność: sortowanie wszystkich kluczy, potem **faza po fazie**:
/// `Despawn` → `Remove` → `Spawn` → `Insert`. Faza przed fazą, a nie komenda po
/// komendzie: dzięki temu „despawn + spawn" w jednym ticku recyklinguje indeks
/// deterministycznie, niezależnie od tego, który system zgłosił się pierwszy.
pub fn flush_commands(world: &mut World, buffers: &mut [CommandBuffer]) -> FlushStats {
    let mut stats = FlushStats::default();

    let mut keys: Vec<(usize, CommandKey)> = Vec::new();
    for (i, buf) in buffers.iter().enumerate() {
        keys.extend(buf.index.iter().map(|k| (i, *k)));
    }
    keys.sort_unstable_by_key(|(_, k)| k.sort_key());

    // Faza 1: despawn.
    for (_, key) in keys.iter().filter(|(_, k)| k.kind == CommandKind::Despawn) {
        let e = key.entity.expect("despawn bez encji");
        if world.despawn(e) {
            stats.despawned += 1;
        } else {
            stats.dropped_dead += 1;
        }
    }

    // Faza 2: usunięcie komponentów.
    for (_, key) in keys.iter().filter(|(_, k)| k.kind == CommandKind::Remove) {
        let e = key.entity.expect("remove bez encji");
        if world.contains(e) {
            (key.remove_fn.expect("remove bez funkcji"))(world, e);
            stats.removed += 1;
        } else {
            stats.dropped_dead += 1;
        }
    }

    // Faza 3: spawn. Indeksy są pobierane z listy wolnych w porządku zwolnienia
    // z fazy 1, więc wynik jest identyczny przy dowolnej liczbie wątków.
    let mut spawned: Vec<((SystemId, u32), Entity)> = Vec::new();
    for (_, key) in keys.iter().filter(|(_, k)| k.kind == CommandKind::Spawn) {
        let e = world.spawn_empty();
        spawned.push(((key.system, key.reservation), e));
        stats.spawned += 1;
    }
    // Rezerwacje są rozwiązywane przez wyszukiwanie binarne, nie liniowe: przy
    // 100 tys. komend liniowe skanowanie kosztowało 233 ms zamiast 15 ms z §7.3.
    spawned.sort_unstable_by_key(|(klucz, _)| *klucz);

    // Faza 4: wstawienie komponentów (adresy rezerwacji rozwiązane po fazie 3).
    for (buf_index, key) in keys.iter().filter(|(_, k)| k.kind == CommandKind::Insert) {
        let target = match key.entity {
            Some(e) => Some(e),
            None => spawned
                .binary_search_by_key(&(key.system, key.reservation), |(klucz, _)| *klucz)
                .ok()
                .map(|i| spawned[i].1),
        };
        let payload = {
            let buf = &mut buffers[*buf_index];
            // SAFETY: offset pochodzi z tego samego bufora, ładunek jest zainicjowany.
            unsafe { buf.blob.as_mut_ptr().add(key.offset as usize) }
        };
        match target {
            Some(e) if world.contains(e) => {
                // SAFETY: ładunek ma typ, dla którego `apply_fn` zostało wygenerowane.
                unsafe { (key.apply_fn.expect("insert bez funkcji"))(world, e, payload) };
                stats.inserted += 1;
            }
            _ => {
                // Encja zniknęła między zgłoszeniem a barierą — ładunek trzeba zniszczyć,
                // inaczej komponent z destruktorem wyciekłby po cichu.
                if let Some(drop_fn) = key.drop_fn {
                    // SAFETY: jak wyżej.
                    unsafe { drop_fn(payload) };
                }
                stats.dropped_dead += 1;
            }
        }
    }

    for buf in buffers.iter_mut() {
        buf.reset();
    }
    stats
}
