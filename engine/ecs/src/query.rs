//! Zapytania: co system czyta i pisze, wyliczone **z typów** (M0 §5.6).
//!
//! Rozwiązywanie kolumn dzieje się raz na chunk (`init_fetch`), a nie raz na wiersz —
//! dzięki temu iteracja po 400 tys. encji to przebieg po ciągłej pamięci, a nie
//! 400 tys. wyszukiwań w rejestrze.

use crate::access::Access;
use crate::archetype::{Archetype, ArchetypeId};
use crate::chunk::ArchetypeChunk;
use crate::component::{Component, ComponentRegistry};
use crate::world::World;
use magnat_core::Entity;
use magnat_jobs::{for_each_chunk_mut, map_reduce_indexed, JobPool};
use std::marker::PhantomData;

/// Surowy dostęp do świata dla systemów biegnących równolegle.
///
/// **Niezmiennik bezpieczeństwa:** scheduler uruchamia dwa systemy jednocześnie
/// wyłącznie wtedy, gdy ich `Access` nie konfliktują (§5.7). Dwa systemy bez konfliktu
/// nie dotykają tego samego komponentu w sposób, w którym którykolwiek pisze —
/// więc referencje, które z tej komórki powstają, nie aliasują się mutowalnie.
/// To jest jedyne miejsce w silniku, w którym poprawność zależy od schedulera,
/// a nie od typów; dlatego jest tu opisane wprost i sprawdzane testem T-D2.
pub struct UnsafeWorldCell<'w> {
    ptr: *mut World,
    _m: PhantomData<&'w mut World>,
}

impl Clone for UnsafeWorldCell<'_> {
    fn clone(&self) -> Self {
        *self
    }
}
impl Copy for UnsafeWorldCell<'_> {}

// SAFETY: komórka sama w sobie jest tylko wskaźnikiem; rozłączność dostępów
// gwarantuje scheduler (patrz niezmiennik wyżej), a `World` zawiera wyłącznie
// dane `Send + Sync`.
unsafe impl Send for UnsafeWorldCell<'_> {}
// SAFETY: jak wyżej.
unsafe impl Sync for UnsafeWorldCell<'_> {}

impl<'w> UnsafeWorldCell<'w> {
    #[must_use]
    pub fn new(world: &'w mut World) -> UnsafeWorldCell<'w> {
        UnsafeWorldCell {
            ptr: std::ptr::from_mut(world),
            _m: PhantomData,
        }
    }

    /// Skraca czas życia komórki. `UnsafeWorldCell` jest niezmienniczy w `'w`
    /// (`PhantomData<&'w mut World>`), więc skrócenie wymaga nowej instancji —
    /// dzięki temu `SystemCtx::query` może związać zapytanie z pożyczką `&mut self`,
    /// a nie z życiem całego świata.
    #[must_use]
    pub fn reborrow(&self) -> UnsafeWorldCell<'_> {
        UnsafeWorldCell {
            ptr: self.ptr,
            _m: PhantomData,
        }
    }

    /// # Safety
    /// Wywołujący gwarantuje, że w tym czasie nikt nie trzyma referencji mutowalnej
    /// do tych samych danych (patrz niezmiennik schedulera).
    #[must_use]
    pub unsafe fn world(self) -> &'w World {
        // SAFETY: wskaźnik pochodzi z żywej referencji `&'w mut World`.
        unsafe { &*self.ptr }
    }

    /// # Safety
    /// Jak wyżej, a dodatkowo: dostęp musi być wyłączny dla tego systemu.
    #[must_use]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn world_mut(self) -> &'w mut World {
        // SAFETY: jak wyżej.
        unsafe { &mut *self.ptr }
    }
}

/// Co system czyta z jednego wiersza. Implementacje: `&T`, `&mut T`, `Entity`,
/// `Option<&T>` oraz krotki do 12 elementów.
pub trait QueryData: Send + Sync {
    /// Wartość zwracana dla jednego wiersza.
    type Item<'w>;
    /// Stan rozwiązany raz na chunk: bazy kolumn.
    type Fetch<'w>;

    /// Wpisuje własne wymagania do `Access` — stąd bierze się deklaracja systemu.
    fn declare_access(reg: &ComponentRegistry, out: &mut Access);

    fn matches(arch: &Archetype, reg: &ComponentRegistry) -> bool;

    /// # Safety
    /// `chunk` musi należeć do archetypu przechodzącego `matches`.
    unsafe fn init_fetch<'w>(reg: &ComponentRegistry, chunk: &'w ArchetypeChunk)
        -> Self::Fetch<'w>;

    /// # Safety
    /// `row < chunk.len()`, a każdy wiersz jest odwiedzany **co najwyżej raz**
    /// w obrębie jednej iteracji — inaczej `&mut T` mogłoby się zaaliasować.
    unsafe fn fetch_row<'w>(fetch: &Self::Fetch<'w>, row: u16) -> Self::Item<'w>;
}

/// Filtr archetypu — nie dotyka danych, więc nie wnosi dostępu.
pub trait QueryFilter: Send + Sync {
    fn matches(arch: &Archetype, reg: &ComponentRegistry) -> bool;
}

impl QueryFilter for () {
    #[inline]
    fn matches(_arch: &Archetype, _reg: &ComponentRegistry) -> bool {
        true
    }
}

pub struct With<T: Component>(PhantomData<T>);
pub struct Without<T: Component>(PhantomData<T>);

impl<T: Component> QueryFilter for With<T> {
    fn matches(arch: &Archetype, reg: &ComponentRegistry) -> bool {
        reg.id_of::<T>().is_some_and(|c| arch.contains(c))
    }
}

impl<T: Component> QueryFilter for Without<T> {
    fn matches(arch: &Archetype, reg: &ComponentRegistry) -> bool {
        // Komponent nigdy nierejestrowany = żaden archetyp go nie ma.
        reg.id_of::<T>().is_none_or(|c| !arch.contains(c))
    }
}

macro_rules! impl_filter_tuple {
    ($($name:ident),+) => {
        impl<$($name: QueryFilter),+> QueryFilter for ($($name,)+) {
            fn matches(arch: &Archetype, reg: &ComponentRegistry) -> bool {
                $($name::matches(arch, reg))&&+
            }
        }
    };
}
impl_filter_tuple!(A);
impl_filter_tuple!(A, B);
impl_filter_tuple!(A, B, C);
impl_filter_tuple!(A, B, C, D);

// ── QueryData dla elementów podstawowych ────────────────────────────────────────

impl QueryData for Entity {
    type Item<'w> = Entity;
    type Fetch<'w> = &'w [Entity];

    fn declare_access(_reg: &ComponentRegistry, _out: &mut Access) {}

    fn matches(_arch: &Archetype, _reg: &ComponentRegistry) -> bool {
        true
    }

    unsafe fn init_fetch<'w>(
        _reg: &ComponentRegistry,
        chunk: &'w ArchetypeChunk,
    ) -> Self::Fetch<'w> {
        chunk.entities()
    }

    unsafe fn fetch_row<'w>(fetch: &Self::Fetch<'w>, row: u16) -> Self::Item<'w> {
        fetch[row as usize]
    }
}

impl<T: Component> QueryData for &T {
    type Item<'w> = &'w T;
    type Fetch<'w> = *const T;

    fn declare_access(reg: &ComponentRegistry, out: &mut Access) {
        if let Some(c) = reg.id_of::<T>() {
            out.read_component(c);
        }
    }

    fn matches(arch: &Archetype, reg: &ComponentRegistry) -> bool {
        reg.id_of::<T>().is_some_and(|c| arch.contains(c))
    }

    unsafe fn init_fetch<'w>(
        reg: &ComponentRegistry,
        chunk: &'w ArchetypeChunk,
    ) -> Self::Fetch<'w> {
        let cid = reg.id_of::<T>().expect("komponent niezarejestrowany");
        let col = chunk
            .layout()
            .column_index(cid)
            .expect("archetyp nie pasuje do zapytania");
        chunk.value_ptr(col, 0).cast::<T>().cast_const()
    }

    unsafe fn fetch_row<'w>(fetch: &Self::Fetch<'w>, row: u16) -> &'w T {
        // SAFETY: `row` mieści się w chunku, kolumna ma typ T, a dostęp do odczytu
        // jest zadeklarowany w `Access`.
        unsafe { &*fetch.add(row as usize) }
    }
}

impl<T: Component> QueryData for &mut T {
    type Item<'w> = &'w mut T;
    type Fetch<'w> = *mut T;

    fn declare_access(reg: &ComponentRegistry, out: &mut Access) {
        if let Some(c) = reg.id_of::<T>() {
            out.write_component(c);
        }
    }

    fn matches(arch: &Archetype, reg: &ComponentRegistry) -> bool {
        reg.id_of::<T>().is_some_and(|c| arch.contains(c))
    }

    unsafe fn init_fetch<'w>(
        reg: &ComponentRegistry,
        chunk: &'w ArchetypeChunk,
    ) -> Self::Fetch<'w> {
        let cid = reg.id_of::<T>().expect("komponent niezarejestrowany");
        let col = chunk
            .layout()
            .column_index(cid)
            .expect("archetyp nie pasuje do zapytania");
        chunk.value_ptr(col, 0).cast::<T>()
    }

    unsafe fn fetch_row<'w>(fetch: &Self::Fetch<'w>, row: u16) -> &'w mut T {
        // SAFETY: każdy wiersz jest odwiedzany co najwyżej raz w jednej iteracji,
        // a zapis do tego komponentu jest zadeklarowany w `Access`, więc żaden
        // równoległy system go nie dotyka.
        unsafe { &mut *fetch.add(row as usize) }
    }
}

impl<T: Component> QueryData for Option<&T> {
    type Item<'w> = Option<&'w T>;
    type Fetch<'w> = Option<*const T>;

    fn declare_access(reg: &ComponentRegistry, out: &mut Access) {
        if let Some(c) = reg.id_of::<T>() {
            out.read_component(c);
        }
    }

    fn matches(_arch: &Archetype, _reg: &ComponentRegistry) -> bool {
        true // opcjonalny komponent nie zawęża zbioru archetypów
    }

    unsafe fn init_fetch<'w>(
        reg: &ComponentRegistry,
        chunk: &'w ArchetypeChunk,
    ) -> Self::Fetch<'w> {
        let cid = reg.id_of::<T>()?;
        let col = chunk.layout().column_index(cid)?;
        Some(chunk.value_ptr(col, 0).cast::<T>().cast_const())
    }

    unsafe fn fetch_row<'w>(fetch: &Self::Fetch<'w>, row: u16) -> Option<&'w T> {
        // SAFETY: jak w `&T`.
        fetch.map(|p| unsafe { &*p.add(row as usize) })
    }
}

macro_rules! impl_query_data_tuple {
    ($($name:ident),+) => {
        impl<$($name: QueryData),+> QueryData for ($($name,)+) {
            type Item<'w> = ($($name::Item<'w>,)+);
            type Fetch<'w> = ($($name::Fetch<'w>,)+);

            fn declare_access(reg: &ComponentRegistry, out: &mut Access) {
                $($name::declare_access(reg, out);)+
            }

            fn matches(arch: &Archetype, reg: &ComponentRegistry) -> bool {
                $($name::matches(arch, reg))&&+
            }

            unsafe fn init_fetch<'w>(
                reg: &ComponentRegistry,
                chunk: &'w ArchetypeChunk,
            ) -> Self::Fetch<'w> {
                // SAFETY: warunek `matches` przeniesiony na wywołującego.
                unsafe { ($($name::init_fetch(reg, chunk),)+) }
            }

            #[allow(non_snake_case)]
            unsafe fn fetch_row<'w>(fetch: &Self::Fetch<'w>, row: u16) -> Self::Item<'w> {
                let ($($name,)+) = fetch;
                // SAFETY: jak wyżej.
                unsafe { ($($name::fetch_row($name, row),)+) }
            }
        }
    };
}

impl_query_data_tuple!(A);
impl_query_data_tuple!(A, B);
impl_query_data_tuple!(A, B, C);
impl_query_data_tuple!(A, B, C, D);
impl_query_data_tuple!(A, B, C, D, E);
impl_query_data_tuple!(A, B, C, D, E, F);
impl_query_data_tuple!(A, B, C, D, E, F, G);
impl_query_data_tuple!(A, B, C, D, E, F, G, H);
impl_query_data_tuple!(A, B, C, D, E, F, G, H, I);
impl_query_data_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_query_data_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_query_data_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);

// ── Zapytanie ───────────────────────────────────────────────────────────────────

/// Zapytanie o wiersze pasujących archetypów.
///
/// Dopasowanie archetypów liczone jest przy konstrukcji (kilkadziesiąt archetypów,
/// jedno przejście) i ważne tak długo, jak żyje `Query` — w trakcie ticku nowe
/// archetypy nie powstają, bo mutacje strukturalne idą przez `CommandBuffer`.
pub struct Query<'w, Q: QueryData, F: QueryFilter = ()> {
    world: UnsafeWorldCell<'w>,
    matched: Vec<ArchetypeId>,
    _m: PhantomData<(Q, F)>,
}

impl<'w, Q: QueryData, F: QueryFilter> Query<'w, Q, F> {
    /// # Safety
    /// Dostęp wyliczony z `Q` musi być rozłączny z dostępem innych systemów
    /// biegnących równolegle (gwarantuje to scheduler).
    pub unsafe fn new(world: UnsafeWorldCell<'w>) -> Query<'w, Q, F> {
        // SAFETY: konstrukcja wymaga tylko odczytu rejestru i listy archetypów.
        let w = unsafe { world.world() };
        let reg = w.components();

        let mut access = Access::new();
        Q::declare_access(reg, &mut access);
        if let Some(c) = access.self_conflict() {
            panic!(
                "zapytanie deklaruje sprzeczny dostęp do komponentu {:?} \
                 (np. Query<(&mut A, &A)>) — rozdziel je na dwa systemy",
                reg.info(c).name()
            );
        }

        let matched = w
            .archetypes()
            .iter()
            .filter(|a| Q::matches(a, reg) && F::matches(a, reg))
            .map(Archetype::id)
            .collect();

        Query {
            world,
            matched,
            _m: PhantomData,
        }
    }

    /// Dostęp wyliczony z typów — to jego scheduler używa do budowy DAG.
    #[must_use]
    pub fn access(world: &World) -> Access {
        let mut access = Access::new();
        Q::declare_access(world.components(), &mut access);
        access
    }

    /// Liczba pasujących wierszy.
    #[must_use]
    pub fn len(&self) -> usize {
        // SAFETY: odczyt liczby wierszy nie dotyka danych komponentów.
        let w = unsafe { self.world.world() };
        self.matched
            .iter()
            .map(|a| w.archetypes().get(*a).rows() as usize)
            .sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iteracja wiersz po wierszu, w kolejności `(ArchetypeId, chunk, row)`.
    pub fn iter(&mut self) -> QueryIter<'_, 'w, Q, F> {
        QueryIter {
            query: self,
            arch_index: 0,
            chunk_index: 0,
            row: 0,
            fetch: None,
            chunk_len: 0,
        }
    }

    /// Pojedynczy wiersz konkretnej encji. `None`, gdy encja nie pasuje do zapytania.
    pub fn get(&mut self, e: Entity) -> Option<Q::Item<'_>> {
        // SAFETY: dostęp zadeklarowany w `Access` tego zapytania.
        let w = unsafe { self.world.world() };
        let loc = w.entities().location(e)?;
        if !self.matched.contains(&loc.archetype) {
            return None;
        }
        let chunk = w.archetypes().get(loc.archetype).chunk(loc.chunk as usize);
        // SAFETY: archetyp przeszedł `matches`, wiersz istnieje.
        let fetch = unsafe { Q::init_fetch(w.components(), chunk) };
        // SAFETY: pojedynczy wiersz, odwiedzany raz.
        Some(unsafe { Q::fetch_row(&fetch, loc.row) })
    }

    /// Równoległa iteracja po chunkach — jednostka pracy to chunk, nie wiersz.
    /// Domknięcie nie może mieć efektów ubocznych poza swoim chunkiem
    /// i buforem komend (00 §3.3).
    pub fn par_for_each(&mut self, pool: &JobPool, f: impl Fn(Q::Item<'_>) + Sync) {
        let mut zadania = self.chunk_tasks();
        let world = self.world;
        for_each_chunk_mut(pool, &mut zadania, |_, task| {
            // SAFETY: chunki są rozłączne, a dostęp do komponentów zadeklarowany
            // w `Access` — scheduler nie puści równolegle systemu w konflikcie.
            let w = unsafe { world.world() };
            let chunk = w
                .archetypes()
                .get(task.archetype)
                .chunk(task.chunk_index as usize);
            // SAFETY: archetyp przeszedł `matches`.
            let fetch = unsafe { Q::init_fetch(w.components(), chunk) };
            for row in 0..chunk.len() {
                // SAFETY: każdy wiersz odwiedzany dokładnie raz.
                f(unsafe { Q::fetch_row(&fetch, row) });
            }
        });
    }

    /// Deterministyczna redukcja równoległa — opakowanie `map_reduce_indexed` (§5.4).
    /// Składanie idzie po indeksie chunka, nigdy po kolejności zakończenia.
    pub fn par_fold<A: Send, R: Send>(
        &mut self,
        pool: &JobPool,
        fold: impl Fn(&mut ChunkView<'_, Q>) -> A + Sync,
        combine: impl Fn(R, A) -> R,
        init: R,
    ) -> R {
        let zadania = self.chunk_tasks();
        let world = self.world;
        map_reduce_indexed(
            pool,
            &zadania,
            |_, task| {
                // SAFETY: jak w `par_for_each`.
                let w = unsafe { world.world() };
                let chunk = w
                    .archetypes()
                    .get(task.archetype)
                    .chunk(task.chunk_index as usize);
                // SAFETY: archetyp przeszedł `matches`.
                let fetch = unsafe { Q::init_fetch(w.components(), chunk) };
                let mut view = ChunkView {
                    fetch,
                    len: chunk.len(),
                    entities: chunk.entities(),
                    row: 0,
                };
                fold(&mut view)
            },
            combine,
            init,
        )
    }

    fn chunk_tasks(&self) -> Vec<ChunkTask> {
        // SAFETY: odczyt struktury archetypów, bez dotykania danych komponentów.
        let w = unsafe { self.world.world() };
        let mut out = Vec::new();
        for a in &self.matched {
            let arch = w.archetypes().get(*a);
            for i in 0..arch.chunk_count() {
                out.push(ChunkTask {
                    archetype: *a,
                    chunk_index: i as u32,
                });
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct ChunkTask {
    archetype: ArchetypeId,
    chunk_index: u32,
}

/// Widok na jeden chunk w redukcji równoległej.
pub struct ChunkView<'w, Q: QueryData> {
    fetch: Q::Fetch<'w>,
    len: u16,
    entities: &'w [Entity],
    row: u16,
}

impl<'w, Q: QueryData> ChunkView<'w, Q> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn entities(&self) -> &'w [Entity] {
        self.entities
    }

    /// Kolejny wiersz chunka albo `None` na końcu.
    pub fn next_row(&mut self) -> Option<Q::Item<'w>> {
        if self.row >= self.len {
            return None;
        }
        let row = self.row;
        self.row += 1;
        // SAFETY: `row < len`, każdy wiersz wydawany dokładnie raz.
        Some(unsafe { Q::fetch_row(&self.fetch, row) })
    }
}

pub struct QueryIter<'q, 'w, Q: QueryData, F: QueryFilter> {
    query: &'q mut Query<'w, Q, F>,
    arch_index: usize,
    chunk_index: usize,
    row: u16,
    fetch: Option<Q::Fetch<'w>>,
    chunk_len: u16,
}

impl<'q, 'w, Q: QueryData, F: QueryFilter> Iterator for QueryIter<'q, 'w, Q, F> {
    type Item = Q::Item<'w>;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: dostęp zadeklarowany przez `Access` zapytania.
        let w = unsafe { self.query.world.world() };
        loop {
            if let Some(fetch) = &self.fetch {
                if self.row < self.chunk_len {
                    let row = self.row;
                    self.row += 1;
                    // SAFETY: `row < chunk_len`, wiersz wydawany dokładnie raz.
                    return Some(unsafe { Q::fetch_row(fetch, row) });
                }
                self.fetch = None;
                self.chunk_index += 1;
            }

            let arch_id = *self.query.matched.get(self.arch_index)?;
            let arch = w.archetypes().get(arch_id);
            if self.chunk_index >= arch.chunk_count() {
                self.arch_index += 1;
                self.chunk_index = 0;
                continue;
            }
            let chunk = arch.chunk(self.chunk_index);
            self.chunk_len = chunk.len();
            self.row = 0;
            // SAFETY: archetyp przeszedł `matches` przy konstrukcji zapytania.
            self.fetch = Some(unsafe { Q::init_fetch(w.components(), chunk) });
        }
    }
}

impl World {
    /// Zapytanie o wiersze świata. Wymaga `&mut self`, bo może wydać `&mut T`;
    /// systemy dostają zapytania przez `SystemCtx`, nie tędy.
    pub fn query<Q: QueryData, F: QueryFilter>(&mut self) -> Query<'_, Q, F> {
        let cell = UnsafeWorldCell::new(self);
        // SAFETY: `&mut self` daje wyłączność na cały świat.
        unsafe { Query::new(cell) }
    }
}
