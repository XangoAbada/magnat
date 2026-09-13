//! `World` — stan świata i operacje bezpośrednie (M0 §5.5).
//!
//! **Operacje bezpośrednie (`spawn`, `despawn`, `insert`, `remove`) są dozwolone
//! wyłącznie poza tickiem:** przy ładowaniu świata, w testach i przy odczycie snapshotu.
//! W trakcie ticku mutacje strukturalne idą przez `CommandBuffer` i punkt synchronizacji
//! (00 §3.4) — inaczej kolejność zmian zależałaby od kolejności wykonania systemów,
//! a ta zależy od liczby wątków.

use crate::archetype::{ArchetypeId, Archetypes, EdgeKind, EntityLocation};
use crate::chunk::{ChunkId, ChunkRef};
use crate::component::{Component, ComponentId, ComponentRegistry};
use crate::entity_store::EntityStore;
use crate::resources::{ResourceId, Resources};
use magnat_core::{Entity, Tick};

pub struct World {
    entities: EntityStore,
    archetypes: Archetypes,
    components: ComponentRegistry,
    resources: Resources,
    hooks: StateHooks,
    /// Rośnie wyłącznie w `App::tick` — świat sam z siebie nie zna upływu czasu.
    pub tick: Tick,
    pub seed: u64,
}

impl World {
    #[must_use]
    pub fn new(seed: u64) -> World {
        World::with_registry(seed, ComponentRegistry::new())
    }

    /// Świat z gotowym rejestrem komponentów — wejście dla `engine/io::load_world`,
    /// które musi odtworzyć te same `ComponentId` co świat zapisujący.
    #[must_use]
    pub fn with_registry(seed: u64, components: ComponentRegistry) -> World {
        let mut archetypes = Archetypes::new();
        // Archetyp pusty ma numer 0 i istnieje od początku: to w nim ląduje
        // każda encja zaraz po `spawn`, zanim dostanie pierwszy komponent.
        let empty = archetypes.get_or_insert(&[], &components);
        debug_assert_eq!(empty, ArchetypeId::EMPTY);
        World {
            entities: EntityStore::new(),
            archetypes,
            components,
            resources: Resources::new(),
            hooks: StateHooks::default(),
            tick: Tick(0),
            seed,
        }
    }

    // ── Rejestr i dostęp do wnętrzności (konsument: io, devtools, query) ─────────

    pub fn register_component<T: Component>(&mut self) -> ComponentId {
        self.components.register::<T>()
    }

    #[must_use]
    pub fn components(&self) -> &ComponentRegistry {
        &self.components
    }

    #[must_use]
    pub fn archetypes(&self) -> &Archetypes {
        &self.archetypes
    }

    #[must_use]
    pub fn entities(&self) -> &EntityStore {
        &self.entities
    }

    #[must_use]
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    // ── Encje ───────────────────────────────────────────────────────────────────

    /// Nowa encja bez komponentów.
    pub fn spawn_empty(&mut self) -> Entity {
        let e = self.entities.alloc();
        let arch = self.archetypes.get_mut(ArchetypeId::EMPTY);
        // SAFETY: archetyp pusty nie ma kolumn, więc nie ma czego inicjować.
        let loc = unsafe { arch.allocate_row(e) };
        self.entities.set_location(e, loc);
        e
    }

    /// Builder: `world.spawn().with(Pos::default()).id()`.
    ///
    /// ponytail: każdy `with` przenosi wiersz do kolejnego archetypu, więc encja
    /// o sześciu komponentach wykonuje sześć przeniesień. Sufit: budowa świata
    /// 400 tys. encji rzędu sekundy, mierzona benchmarkiem B-3. Ścieżka wyjścia,
    /// gdy zacznie przeszkadzać: `spawn_bundle` wyliczające docelowy archetyp z góry.
    /// Nie robimy tego dziś, bo to ścieżka poza tickiem — a w tickach spawnuje
    /// `CommandBuffer`, który już teraz składa komplet komponentów przed flushem.
    pub fn spawn(&mut self) -> EntityMut<'_> {
        let entity = self.spawn_empty();
        EntityMut {
            world: self,
            entity,
        }
    }

    /// `false`, gdy uchwyt był już martwy — despawn encji, której nie ma, nie jest
    /// błędem programu (komendy mogą się o to potknąć, 00 §3.4).
    pub fn despawn(&mut self, e: Entity) -> bool {
        let Some(loc) = self.entities.location(e) else {
            return false;
        };
        let arch = self.archetypes.get_mut(loc.archetype);
        // SAFETY: lokalizacja pochodzi z rejestru encji, więc wiersz istnieje;
        // wartości mają zostać zniszczone, bo encja znika.
        let moved = unsafe { arch.swap_remove(loc, true) };
        if let Some((moved_entity, new_loc)) = moved {
            self.entities.set_location(moved_entity, new_loc);
        }
        self.entities.free(e)
    }

    #[must_use]
    pub fn contains(&self, e: Entity) -> bool {
        self.entities.contains(e)
    }

    #[must_use]
    pub fn entity_count(&self) -> u32 {
        self.entities.alive_count()
    }

    // ── Komponenty ──────────────────────────────────────────────────────────────

    /// Wstawia albo nadpisuje komponent. Nadpisanie w miejscu nie zmienia archetypu.
    pub fn insert<T: Component>(&mut self, e: Entity, value: T) {
        let cid = self.components.register::<T>();
        let Some(loc) = self.entities.location(e) else {
            return; // encja martwa — cicho, tak jak flush komend (00 §3.4)
        };

        if self.archetypes.get(loc.archetype).contains(cid) {
            let col = self
                .archetypes
                .get(loc.archetype)
                .column_index(cid)
                .expect("archetyp deklaruje komponent, ale nie ma kolumny");
            let chunk = self
                .archetypes
                .get_mut(loc.archetype)
                .chunk_mut(loc.chunk as usize);
            let ptr = chunk.value_ptr(col, loc.row).cast::<T>();
            // SAFETY: kolumna jest typu T (rejestr nadał `cid` dla T), wiersz istnieje
            // i jest zainicjowany — stara wartość musi zostać zniszczona.
            unsafe { std::ptr::drop_in_place(ptr) };
            // SAFETY: adres jest wyrównany i należy do tej kolumny.
            unsafe { ptr.write(value) };
            chunk.bump_generation();
            return;
        }

        let target = self.target_archetype(loc.archetype, cid, EdgeKind::Add);
        let new_loc = self.relocate(e, loc, target, None);
        let col = self
            .archetypes
            .get(target)
            .column_index(cid)
            .expect("archetyp docelowy nie ma dokładanej kolumny");
        let chunk = self
            .archetypes
            .get_mut(target)
            .chunk_mut(new_loc.chunk as usize);
        // SAFETY: wiersz właśnie powstał, ta kolumna jest jedyną niezainicjowaną.
        unsafe { chunk.value_ptr(col, new_loc.row).cast::<T>().write(value) };
    }

    /// Usuwa komponent i zwraca jego wartość. `None`, gdy encja go nie miała.
    pub fn remove<T: Component>(&mut self, e: Entity) -> Option<T> {
        let cid = self.components.id_of::<T>()?;
        let loc = self.entities.location(e)?;
        if !self.archetypes.get(loc.archetype).contains(cid) {
            return None;
        }
        let target = self.target_archetype(loc.archetype, cid, EdgeKind::Remove);
        let mut taken = std::mem::MaybeUninit::<T>::uninit();
        self.relocate(e, loc, target, Some((cid, taken.as_mut_ptr().cast::<u8>())));
        // SAFETY: `relocate` przeniosło bajty usuwanego komponentu dokładnie tutaj.
        Some(unsafe { taken.assume_init() })
    }

    #[must_use]
    pub fn get<T: Component>(&self, e: Entity) -> Option<&T> {
        let cid = self.components.id_of::<T>()?;
        let loc = self.entities.location(e)?;
        let arch = self.archetypes.get(loc.archetype);
        let col = arch.column_index(cid)?;
        let ptr = arch
            .chunk(loc.chunk as usize)
            .value_ptr(col, loc.row)
            .cast::<T>();
        // SAFETY: kolumna ma typ T (tożsamość typu sprawdzona przy rejestracji),
        // wiersz jest zainicjowany, a `&self` zabrania mutacji w tym czasie.
        Some(unsafe { &*ptr })
    }

    pub fn get_mut<T: Component>(&mut self, e: Entity) -> Option<&mut T> {
        let cid = self.components.id_of::<T>()?;
        let loc = self.entities.location(e)?;
        let col = self.archetypes.get(loc.archetype).column_index(cid)?;
        let chunk = self
            .archetypes
            .get_mut(loc.archetype)
            .chunk_mut(loc.chunk as usize);
        chunk.bump_generation();
        let ptr = chunk.value_ptr(col, loc.row).cast::<T>();
        // SAFETY: jak w `get`, a `&mut self` daje wyłączność.
        Some(unsafe { &mut *ptr })
    }

    #[must_use]
    pub fn has<T: Component>(&self, e: Entity) -> bool {
        self.components
            .id_of::<T>()
            .and_then(|cid| {
                self.entities
                    .location(e)
                    .map(|loc| self.archetypes.get(loc.archetype).contains(cid))
            })
            .unwrap_or(false)
    }

    /// Archetyp po dodaniu albo usunięciu komponentu — z cache krawędzi,
    /// żeby nie przeszukiwać rejestru przy każdej operacji.
    fn target_archetype(
        &mut self,
        from: ArchetypeId,
        cid: ComponentId,
        kind: EdgeKind,
    ) -> ArchetypeId {
        if let Some(target) = self.archetypes.get(from).edge(cid, kind) {
            return target;
        }
        let mut components: Vec<ComponentId> = self.archetypes.get(from).components().to_vec();
        match kind {
            EdgeKind::Add => components.push(cid),
            EdgeKind::Remove => components.retain(|c| *c != cid),
        }
        let target = self.archetypes.get_or_insert(&components, &self.components);
        self.archetypes.get_mut(from).set_edge(cid, kind, target);
        // Krawędź powrotna: przejście tam i z powrotem to typowy wzorzec
        // (komponent stanu dokładany na kilka ticków).
        let back = match kind {
            EdgeKind::Add => EdgeKind::Remove,
            EdgeKind::Remove => EdgeKind::Add,
        };
        self.archetypes.get_mut(target).set_edge(cid, back, from);
        target
    }

    /// Przenosi wiersz encji do innego archetypu. `taken_out` wskazuje komponent,
    /// który znika z archetypu docelowego: jego bajty trafiają pod podany adres
    /// (`remove`) — a jeśli adresu brak, wartość jest niszczona.
    ///
    /// Zwraca nową lokalizację. Wiersz w archetypie docelowym ma zainicjowane
    /// **wszystkie kolumny wspólne**; kolumnę dokładaną inicjuje wywołujący.
    fn relocate(
        &mut self,
        e: Entity,
        from: EntityLocation,
        to: ArchetypeId,
        taken_out: Option<(ComponentId, *mut u8)>,
    ) -> EntityLocation {
        let (src, dst) = self.archetypes.pair_mut(from.archetype, to);
        // SAFETY: kolumny wiersza zostaną zainicjowane w pętli poniżej (wspólne)
        // oraz przez wywołującego (kolumna dokładana).
        let new_loc = unsafe { dst.allocate_row(e) };

        // Kolumna, której archetyp docelowy nie ma, a której nikt nie odbiera —
        // niszczona po pętli, żeby nie mieszać pożyczki mutowalnej z niemutowalną.
        let mut to_drop: Option<usize> = None;
        {
            let src_chunk = src.chunk(from.chunk as usize);
            let column_count = src_chunk.layout().columns().len();
            for src_col in 0..column_count {
                let cid = src_chunk.layout().columns()[src_col].component;
                if let Some(dst_col) = dst.column_index(cid) {
                    let dst_chunk = dst.chunk_mut(new_loc.chunk as usize);
                    // SAFETY: ten sam komponent po obu stronach (równe rozmiary),
                    // oba wiersze istnieją, zakresy rozłączne (różne archetypy).
                    unsafe {
                        crate::chunk::ArchetypeChunk::move_column_value(
                            src_chunk,
                            src_col,
                            from.row,
                            dst_chunk,
                            dst_col,
                            new_loc.row,
                        );
                    }
                } else if let Some((removed, out)) = taken_out {
                    debug_assert_eq!(removed, cid);
                    let size = src_chunk.layout().columns()[src_col].size;
                    // SAFETY: `out` wskazuje na bufor o rozmiarze tego komponentu
                    // (`MaybeUninit<T>` w `remove`), zakresy są rozłączne.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            src_chunk.value_ptr(src_col, from.row),
                            out,
                            size,
                        );
                    }
                } else {
                    to_drop = Some(src_col);
                }
            }
        }
        if let Some(src_col) = to_drop {
            let src_mut = src.chunk_mut(from.chunk as usize);
            // SAFETY: wartość znika razem z komponentem, wiersz jest zainicjowany.
            unsafe { src_mut.drop_column_value(src_col, from.row) };
        }

        // Wiersz źródłowy znika BEZ destruktorów — jego wartości właśnie zostały
        // przeniesione albo zniszczone punktowo wyżej.
        // SAFETY: dokładnie ten warunek jest spełniony.
        let moved = unsafe { src.swap_remove(from, false) };
        if let Some((moved_entity, moved_loc)) = moved {
            self.entities.set_location(moved_entity, moved_loc);
        }
        self.entities.set_location(e, new_loc);
        new_loc
    }

    // ── Zasoby ──────────────────────────────────────────────────────────────────

    pub fn insert_resource<T: Send + Sync + 'static>(&mut self, v: T) -> ResourceId {
        self.resources.insert(v)
    }

    /// Panika, gdy zasobu nie ma: brak zasobu to błąd konfiguracji świata,
    /// a nie stan, który system ma obsługiwać w każdym ticku.
    #[must_use]
    pub fn resource<T: Send + Sync + 'static>(&self) -> &T {
        self.resources.get::<T>().unwrap_or_else(|| {
            panic!(
                "brak zasobu {} — wstaw go przy budowie świata",
                std::any::type_name::<T>()
            )
        })
    }

    pub fn resource_mut<T: Send + Sync + 'static>(&mut self) -> &mut T {
        let name = std::any::type_name::<T>();
        self.resources
            .get_mut::<T>()
            .unwrap_or_else(|| panic!("brak zasobu {name} — wstaw go przy budowie świata"))
    }

    #[must_use]
    pub fn get_resource<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.resources.get::<T>()
    }

    pub fn get_resource_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.resources.get_mut::<T>()
    }

    // ── Chunki (API serializacji i devtools, M0 §5.5.1) ──────────────────────────

    /// Iteracja po wszystkich chunkach w kolejności `(ArchetypeId, index)`.
    ///
    /// To jest API **serializacji i narzędzi**, nie ścieżka dostępu dla systemów:
    /// `ChunkRef` omija `Access`, więc scheduler nie zobaczy zależności (ryzyko R-12).
    /// System czyta dane przez `Query`.
    pub fn chunks(&self) -> impl Iterator<Item = ChunkRef<'_>> {
        self.archetypes.chunk_refs()
    }

    #[must_use]
    pub fn chunk(&self, id: ChunkId) -> Option<ChunkRef<'_>> {
        let arch = self.archetypes.iter().find(|a| a.id() == id.archetype)?;
        let chunk = arch.chunks().get(id.index as usize)?;
        Some(ChunkRef::new(id, chunk))
    }
}

/// Dlaczego odczyt zapisu może się nie udać — komunikat trafia do gracza jako
/// „zapis uszkodzony", więc musi wskazywać komponent, a nie tylko fakt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RestoreError {
    ColumnOutsideArchetype,
    UnknownComponent,
    MissingColumn,
    ComponentNeedsDrop {
        name: &'static str,
    },
    ColumnLength {
        name: &'static str,
        expected: usize,
        found: usize,
    },
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreError::ColumnOutsideArchetype => {
                write!(f, "kolumna spoza zestawu komponentów archetypu")
            }
            RestoreError::UnknownComponent => write!(f, "komponent nieznany temu światu"),
            RestoreError::MissingColumn => write!(f, "brakuje kolumny dla komponentu archetypu"),
            RestoreError::ComponentNeedsDrop { name } => write!(
                f,
                "komponent {name} ma destruktor — format zapisu M0 go nie obsługuje (M12)"
            ),
            RestoreError::ColumnLength {
                name,
                expected,
                found,
            } => write!(f, "kolumna {name}: {found} B zamiast {expected} B"),
        }
    }
}

impl std::error::Error for RestoreError {}

/// Zaczepy stanu, których `World` nie umie obejść samodzielnie: areny i zasoby.
///
/// `Arena<T>` żyje jako zasób, a zasób jest `dyn Any` — bez zarejestrowanej funkcji
/// haszującej hash stanu po prostu by je pominął i rozjazd 600 tys. partii przeszedłby
/// przez CI niezauważony (00 §K-16). Rejestracja jest jawna, bo ma być widoczna
/// w kodzie fazy, która arenę zakłada.
/// Funkcja dopisująca fragment stanu do hasha świata.
pub type StateHashHook = fn(&World, &mut magnat_core::StateHasher);

#[derive(Default)]
pub struct StateHooks {
    arenas: Vec<(magnat_core::ArenaKind, StateHashHook)>,
    resources: Vec<(&'static str, StateHashHook)>,
}

impl StateHooks {
    /// Areny w kolejności `ArenaKind` (M0 §5.9).
    pub fn arenas(&self) -> impl Iterator<Item = (magnat_core::ArenaKind, StateHashHook)> + '_ {
        let mut sorted: Vec<_> = self.arenas.clone();
        sorted.sort_by_key(|(kind, _)| *kind);
        sorted.into_iter()
    }

    /// Zasoby w kolejności nazwy typu (M0 §5.9).
    pub fn resources(&self) -> impl Iterator<Item = (&'static str, StateHashHook)> + '_ {
        let mut sorted: Vec<_> = self.resources.clone();
        sorted.sort_by_key(|(name, _)| *name);
        sorted.into_iter()
    }
}

impl World {
    /// Wpina arenę do hasha stanu. Wywoływane przez fazę, która arenę zakłada
    /// (M5 — oferty, M6 — partie).
    pub fn register_arena_hash<T>(&mut self, kind: magnat_core::ArenaKind)
    where
        T: magnat_core::HashState + Send + Sync + 'static,
    {
        fn hook<T: magnat_core::HashState + Send + Sync + 'static>(
            world: &World,
            h: &mut magnat_core::StateHasher,
        ) {
            if let Some(arena) = world.get_resource::<magnat_core::Arena<T>>() {
                arena.hash_state(h);
            }
        }

        self.hooks.arenas.push((kind, hook::<T>));
    }

    /// Wpina zasób do hasha stanu.
    pub fn register_resource_hash<T>(&mut self)
    where
        T: magnat_core::HashState + Send + Sync + 'static,
    {
        fn hook<T: magnat_core::HashState + Send + Sync + 'static>(
            world: &World,
            h: &mut magnat_core::StateHasher,
        ) {
            if let Some(v) = world.get_resource::<T>() {
                v.hash_state(h);
            }
        }
        self.hooks
            .resources
            .push((std::any::type_name::<T>(), hook::<T>));
    }

    #[must_use]
    pub fn state_hooks(&self) -> &StateHooks {
        &self.hooks
    }

    #[must_use]
    pub fn entities_mut(&mut self) -> &mut EntityStore {
        &mut self.entities
    }

    /// Bezpieczna brama do `restore_rows` — **to jest granica zaufania odczytu zapisu**.
    ///
    /// Sprawdza wszystko, co da się sprawdzić bez znajomości typu: długości kolumn,
    /// przynależność komponentów do archetypu i brak destruktorów. Ostatni warunek jest
    /// istotny: typ bez destruktora nie trzyma wskaźnika ani alokacji, więc dowolny
    /// układ bajtów nie zrobi z niego wiszącej referencji. Komponenty z destruktorem
    /// nie są w formacie M0 zapisywane w ogóle (M12 dołoży im serializację).
    ///
    /// Reszta zaufania jest po stronie `engine/io`: każda sekcja ma sumę kontrolną
    /// XXH3, a po wczytaniu porównywany jest hash całego stanu. Dzięki tej bramie
    /// `engine/io` pozostaje `#![forbid(unsafe_code)]` (00 §6).
    pub fn restore_rows_checked(
        &mut self,
        components: &[ComponentId],
        entities: &[Entity],
        columns: &[(ComponentId, &[u8])],
    ) -> Result<(), RestoreError> {
        for (cid, bytes) in columns {
            if !components.contains(cid) {
                return Err(RestoreError::ColumnOutsideArchetype);
            }
            let info = self
                .components
                .get(*cid)
                .ok_or(RestoreError::UnknownComponent)?;
            if info.needs_drop() {
                return Err(RestoreError::ComponentNeedsDrop { name: info.name() });
            }
            let expected = info.size() * entities.len();
            if bytes.len() != expected {
                return Err(RestoreError::ColumnLength {
                    name: info.name(),
                    expected,
                    found: bytes.len(),
                });
            }
        }
        if columns.len() != components.len() {
            return Err(RestoreError::MissingColumn);
        }
        // SAFETY: powyższe sprawdzenia pokrywają wszystkie warunki `restore_rows`
        // poza poprawnością samych bitów, którą po stronie `io` gwarantuje suma
        // kontrolna sekcji i weryfikacja hasha stanu po wczytaniu.
        unsafe { self.restore_rows(components, entities, columns) };
        Ok(())
    }

    /// Odtwarza wiersze z zapisu: tworzy (albo znajduje) archetyp, alokuje wiersze
    /// dla podanych encji i kopiuje surowe bajty kolumn.
    ///
    /// Konsument: wyłącznie `engine/io` przy wczytywaniu snapshotu.
    ///
    /// # Safety
    /// `columns` musi nieść bajty pochodzące z kolumn **tych samych typów**
    /// (weryfikacja po `ComponentSchemaId` należy do `engine/io`), a każda kolumna
    /// musi mieć `entities.len() * rozmiar` bajtów.
    pub unsafe fn restore_rows(
        &mut self,
        components: &[ComponentId],
        entities: &[Entity],
        columns: &[(ComponentId, &[u8])],
    ) {
        let arch_id = self.archetypes.get_or_insert(components, &self.components);
        for (row_index, e) in entities.iter().enumerate() {
            let arch = self.archetypes.get_mut(arch_id);
            // SAFETY: wszystkie kolumny są inicjowane w pętli poniżej.
            let loc = unsafe { arch.allocate_row(*e) };
            for (cid, bytes) in columns {
                let col = arch
                    .column_index(*cid)
                    .expect("kolumna spoza archetypu docelowego");
                let size = self.components.info(*cid).size();
                let from = row_index * size;
                let chunk = arch.chunk_mut(loc.chunk as usize);
                // SAFETY: zakres źródłowy jest w granicach bufora sekcji,
                // a docelowy to świeżo zaalokowany wiersz tej kolumny.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        bytes[from..from + size].as_ptr(),
                        chunk.value_ptr(col, loc.row),
                        size,
                    );
                }
            }
            self.entities.set_location(*e, loc);
        }
    }
}

/// Builder encji. Istnieje tylko po to, żeby `spawn().with(..).with(..).id()`
/// czytało się jak jedna operacja.
pub struct EntityMut<'w> {
    world: &'w mut World,
    entity: Entity,
}

impl EntityMut<'_> {
    #[must_use]
    pub fn with<T: Component>(self, value: T) -> Self {
        let EntityMut { world, entity } = self;
        world.insert(entity, value);
        EntityMut { world, entity }
    }

    #[must_use]
    pub fn id(self) -> Entity {
        self.entity
    }
}
