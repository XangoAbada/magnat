//! Chunk — jednostka pamięci, równoległości i copy-on-write (M0 §5.5, §5.5.1).
//!
//! **Kontrakt z M12, pilnowany testem layoutu:** jeden chunk to jedna alokacja
//! ≤ 64 KiB pod **jednym** wskaźnikiem, adresowalna przez `ChunkId`, z licznikiem
//! `generation`, dostępna jako `Send` do odczytu. Bez tego zapis w tle przez
//! copy-on-write (PRD §20.2: zapis < 5 s bez pauzy) jest niewykonalny, a dorobienie
//! go później oznaczałoby przepisanie storage'u.
//!
//! M0 **nie** dostarcza samego mechanizmu CoW — ani `ChunkMut`, ani licznika referencji,
//! ani `AtomicPtr` w miejscu wskaźnika. Dostarcza kształt, który to umożliwia.

use crate::component::{ComponentId, ComponentRegistry};
use magnat_core::Entity;
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::ptr::NonNull;
use std::sync::Arc;

/// Docelowy rozmiar chunka w bajtach.
///
/// 64 KiB nie jest wartością z sufitu: to jednostka copy-on-write zapisu w tle w M12.
/// Duży chunk = mniej kopiowania przy zapisie, ale grubsze ziarno równoległości
/// i gorsze trafienia w L2; 64 KiB mieści się w L2 każdego procesora z ostatniej dekady.
///
/// Arytmetyka (korekta wobec planu M0 §5.5, który mówił o „~160 chunkach"): wiersz
/// 400 B plus 8 B encji daje **128 wierszy w chunku** (potęga dwójki poniżej 160),
/// czyli ok. **3 100 chunków** na 400 tys. encji. To nadal podział wystarczająco drobny
/// dla 16 wątków — 160 to liczba wierszy, nie chunków. Zmiana tej stałej wymaga
/// uzgodnienia z M12 (D-6).
pub const CHUNK_TARGET_BYTES: usize = 64 * 1024;

/// Dolna i górna granica liczby wierszy w chunku. Dolna broni przed chunkiem
/// na dwa wiersze przy grubym archetypie, górna — przed zbyt grubym ziarnem
/// równoległości przy archetypie z jednym bajtem na wiersz.
const MIN_ROWS: u16 = 8;
const MAX_ROWS: u16 = 2048;

/// Położenie jednej kolumny wewnątrz chunka.
#[derive(Clone, Debug)]
pub struct ColumnLayout {
    pub component: ComponentId,
    pub offset: usize,
    pub size: usize,
    pub drop_fn: Option<unsafe fn(*mut u8)>,
}

/// Rozkład pamięci chunka — wspólny dla wszystkich chunków jednego archetypu.
/// Trzymany w `Arc`, żeby chunk umiał posprzątać po sobie bez oglądania się
/// na archetyp (to jest warunek tego, żeby `ArchetypeChunk` mógł być `Drop`).
#[derive(Debug)]
pub struct ChunkLayout {
    rows_per_chunk: u16,
    row_shift: u32,
    row_bytes: usize,
    alloc_layout: Layout,
    entities_offset: usize,
    columns: Box<[ColumnLayout]>,
}

/// Największa potęga dwójki nie większa niż `n` (dla `n >= 1`).
fn prev_pow2(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    1usize << (usize::BITS - 1 - n.leading_zeros())
}

fn align_up(offset: usize, align: usize) -> usize {
    (offset + align - 1) & !(align - 1)
}

impl ChunkLayout {
    /// `components` musi być posortowane rosnąco po `ComponentId` (kanoniczna
    /// tożsamość archetypu) — kolumny idą w tej samej kolejności.
    pub fn new(components: &[ComponentId], registry: &ComponentRegistry) -> ChunkLayout {
        debug_assert!(components.windows(2).all(|w| w[0] < w[1]));

        let entity_size = size_of::<Entity>();
        let row_bytes: usize = entity_size
            + components
                .iter()
                .map(|c| registry.info(*c).size())
                .sum::<usize>();

        let max_align = components
            .iter()
            .map(|c| registry.info(*c).layout().align())
            .chain(std::iter::once(align_of::<Entity>()))
            .max()
            .unwrap_or(align_of::<Entity>());

        // Liczba wierszy: potęga dwójki, więc lokalizacja wiersza w archetypie to
        // przesunięcie bitowe, a nie dzielenie.
        let mut rows = prev_pow2(CHUNK_TARGET_BYTES / row_bytes.max(1))
            .clamp(MIN_ROWS as usize, MAX_ROWS as usize) as u16;

        // Wyrównania mogą dołożyć padding ponad `rows * row_bytes` — jeśli przez to
        // alokacja przekracza budżet, schodzimy o połowę wierszy. Chunk większy niż
        // 64 KiB powstaje wyłącznie dla archetypu o wierszu grubszym niż 8 KiB,
        // czyli nigdy w tej grze.
        let (entities_offset, columns, mut total) =
            Self::compute_offsets(components, registry, rows, entity_size);
        let mut entities_offset = entities_offset;
        let mut columns = columns;
        while total > CHUNK_TARGET_BYTES && rows > 1 {
            rows /= 2;
            let computed = Self::compute_offsets(components, registry, rows, entity_size);
            entities_offset = computed.0;
            columns = computed.1;
            total = computed.2;
        }

        let alloc_layout =
            Layout::from_size_align(align_up(total, max_align).max(max_align), max_align)
                .expect("ChunkLayout: niepoprawny rozkład pamięci chunka");

        ChunkLayout {
            rows_per_chunk: rows,
            row_shift: rows.trailing_zeros(),
            row_bytes,
            alloc_layout,
            entities_offset,
            columns: columns.into_boxed_slice(),
        }
    }

    fn compute_offsets(
        components: &[ComponentId],
        registry: &ComponentRegistry,
        rows: u16,
        entity_size: usize,
    ) -> (usize, Vec<ColumnLayout>, usize) {
        let entities_offset = 0usize;
        let mut cursor = entity_size * rows as usize;
        let mut columns = Vec::with_capacity(components.len());
        for c in components {
            let info = registry.info(*c);
            let align = info.layout().align();
            let offset = align_up(cursor, align);
            columns.push(ColumnLayout {
                component: *c,
                offset,
                size: info.size(),
                drop_fn: info.drop_fn(),
            });
            cursor = offset + info.size() * rows as usize;
        }
        (entities_offset, columns, cursor)
    }

    #[inline]
    #[must_use]
    pub fn rows_per_chunk(&self) -> u16 {
        self.rows_per_chunk
    }

    #[inline]
    #[must_use]
    pub fn row_shift(&self) -> u32 {
        self.row_shift
    }

    #[inline]
    #[must_use]
    pub fn row_bytes(&self) -> usize {
        self.row_bytes
    }

    #[inline]
    #[must_use]
    pub fn alloc_bytes(&self) -> usize {
        self.alloc_layout.size()
    }

    #[inline]
    #[must_use]
    pub fn columns(&self) -> &[ColumnLayout] {
        &self.columns
    }

    /// Indeks kolumny komponentu w tym chunku. `None`, jeśli archetyp go nie ma.
    #[inline]
    #[must_use]
    pub fn column_index(&self, c: ComponentId) -> Option<usize> {
        self.columns
            .binary_search_by_key(&c, |col| col.component)
            .ok()
    }
}

/// Jeden chunk: **jedna alokacja**, kolumny to rozłączne wycinki tej alokacji (SoA),
/// tablica encji też w niej siedzi.
pub struct ArchetypeChunk {
    /// Jedyny wskaźnik na dane — to jest kontrakt z M12: podmiana chunka przy
    /// copy-on-write ma być podmianą jednego wskaźnika, nie rekonstrukcją struktury.
    data: NonNull<u8>,
    layout: Arc<ChunkLayout>,
    len: u16,
    /// Rośnie przy każdej mutacji zawartości. M12 czyta ją, żeby wiedzieć,
    /// czy sekcja snapshotu jest nadal aktualna (zapis przyrostowy).
    generation: u32,
}

// SAFETY: chunk jest wyłącznym właścicielem swojej alokacji (nikt inny nie trzyma
// wskaźnika do niej), a komponenty są `Send + Sync` z definicji `Component`.
unsafe impl Send for ArchetypeChunk {}
// SAFETY: jak wyżej — współdzielony dostęp do chunka daje wyłącznie odczyt.
unsafe impl Sync for ArchetypeChunk {}

impl ArchetypeChunk {
    pub fn new(layout: Arc<ChunkLayout>) -> ArchetypeChunk {
        // SAFETY: `alloc_layout` ma niezerowy rozmiar (zawsze jest tablica encji)
        // i poprawne wyrównanie — `Layout::from_size_align` to sprawdziło.
        let ptr = unsafe { alloc(layout.alloc_layout) };
        let Some(data) = NonNull::new(ptr) else {
            handle_alloc_error(layout.alloc_layout)
        };
        ArchetypeChunk {
            data,
            layout,
            len: 0,
            generation: 0,
        }
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> u16 {
        self.len
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len >= self.layout.rows_per_chunk
    }

    #[inline]
    #[must_use]
    pub fn generation(&self) -> u32 {
        self.generation
    }

    #[inline]
    pub fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    #[inline]
    #[must_use]
    pub fn layout(&self) -> &Arc<ChunkLayout> {
        &self.layout
    }

    #[inline]
    #[must_use]
    pub fn data_ptr(&self) -> NonNull<u8> {
        self.data
    }

    #[inline]
    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        // SAFETY: tablica encji zaczyna się na `entities_offset`, jest wyrównana
        // (offset 0, alokacja wyrównana co najmniej do align_of::<Entity>())
        // i ma `len` zainicjowanych elementów — niezmiennik utrzymywany przez
        // `push_entity`/`swap_remove_row`.
        unsafe {
            std::slice::from_raw_parts(
                self.data
                    .as_ptr()
                    .add(self.layout.entities_offset)
                    .cast::<Entity>(),
                self.len as usize,
            )
        }
    }

    #[inline]
    fn entity_slot(&self, row: u16) -> *mut Entity {
        debug_assert!(row < self.layout.rows_per_chunk);
        // SAFETY: `row` mieści się w pojemności chunka, więc adres leży w alokacji.
        unsafe {
            self.data
                .as_ptr()
                .add(self.layout.entities_offset)
                .cast::<Entity>()
                .add(row as usize)
        }
    }

    /// Adres wartości komponentu `col` w wierszu `row`.
    #[inline]
    #[must_use]
    pub fn value_ptr(&self, col: usize, row: u16) -> *mut u8 {
        let c = &self.layout.columns[col];
        debug_assert!(row < self.layout.rows_per_chunk);
        // SAFETY: offset kolumny i rozmiar wiersza pochodzą z `ChunkLayout`,
        // który policzył je tak, żeby cały zakres mieścił się w alokacji.
        unsafe { self.data.as_ptr().add(c.offset + c.size * row as usize) }
    }

    /// Surowe bajty całej kolumny — dokładnie to, co trafia do sekcji snapshotu.
    ///
    /// # Safety
    /// Kolumna musi mieć typ bez paddingu, jeśli wynik ma być porównywalny bajtowo;
    /// do hasha stanu służy `ComponentInfo::hash_value`, nie te bajty.
    #[must_use]
    pub unsafe fn column_bytes(&self, col: usize) -> &[u8] {
        let c = &self.layout.columns[col];
        // SAFETY: zakres [offset, offset + size*len) leży w alokacji i jest
        // zainicjowany dla `len` wierszy.
        unsafe {
            std::slice::from_raw_parts(self.data.as_ptr().add(c.offset), c.size * self.len as usize)
        }
    }

    /// Dopisuje encję w nowym wierszu i zwraca jego numer.
    ///
    /// # Safety
    /// Wywołujący **musi** natychmiast zainicjować wszystkie kolumny tego wiersza.
    /// Wiersz z niezainicjowaną kolumną to UB przy pierwszym odczycie.
    pub unsafe fn push_entity(&mut self, e: Entity) -> u16 {
        assert!(!self.is_full(), "push_entity do pełnego chunka");
        let row = self.len;
        // SAFETY: `row < rows_per_chunk`, adres leży w alokacji i jest wyrównany.
        unsafe { self.entity_slot(row).write(e) };
        self.len += 1;
        self.bump_generation();
        row
    }

    /// Usuwa wiersz, wstawiając w jego miejsce ostatni wiersz tego chunka.
    /// Zwraca encję, która się przeniosła (jeśli jakakolwiek) — wywołujący musi
    /// poprawić jej `EntityLocation`.
    ///
    /// `drop_existing == false` oznacza, że wartości wiersza **zostały już przeniesione**
    /// gdzie indziej (zmiana archetypu) i nie wolno wywoływać na nich destruktorów.
    ///
    /// # Safety
    /// `row < len`; przy `drop_existing == false` wartości muszą być wcześniej przeniesione.
    pub unsafe fn swap_remove_row(&mut self, row: u16, drop_existing: bool) -> Option<Entity> {
        debug_assert!(row < self.len);
        let last = self.len - 1;
        if drop_existing {
            // SAFETY: wiersz jest zainicjowany i nie będzie już czytany.
            unsafe { self.drop_row(row) };
        }
        let moved = if row != last {
            for col in 0..self.layout.columns.len() {
                let size = self.layout.columns[col].size;
                let src = self.value_ptr(col, last);
                let dst = self.value_ptr(col, row);
                // SAFETY: wiersze są rozłączne, oba w tej samej alokacji,
                // `size` to rozmiar jednej wartości tej kolumny.
                unsafe { std::ptr::copy_nonoverlapping(src, dst, size) };
            }
            // SAFETY: oba sloty encji są w alokacji i zainicjowane.
            let e = unsafe { self.entity_slot(last).read() };
            // SAFETY: jak wyżej.
            unsafe { self.entity_slot(row).write(e) };
            Some(e)
        } else {
            None
        };
        self.len -= 1;
        self.bump_generation();
        moved
    }

    /// Niszczy wartości wszystkich kolumn w wierszu (bez zmiany `len`).
    ///
    /// # Safety
    /// `row < len`, a wiersz jest zainicjowany i nie będzie już czytany.
    unsafe fn drop_row(&mut self, row: u16) {
        for col in 0..self.layout.columns.len() {
            if let Some(drop_fn) = self.layout.columns[col].drop_fn {
                let ptr = self.value_ptr(col, row);
                // SAFETY: `drop_fn` pochodzi z rejestracji typu tej kolumny,
                // a wiersz jest zainicjowany.
                unsafe { drop_fn(ptr) };
            }
        }
    }

    /// Przenosi zawartość jednej kolumny między wierszami dwóch chunków
    /// o **różnych** layoutach (zmiana archetypu). Bajty są przenoszone, nie kopiowane:
    /// po operacji źródłowy wiersz jest logicznie pusty i nie wolno go niszczyć.
    ///
    /// # Safety
    /// Kolumny muszą dotyczyć tego samego komponentu, a oba wiersze istnieć.
    pub unsafe fn move_column_value(
        src: &ArchetypeChunk,
        src_col: usize,
        src_row: u16,
        dst: &mut ArchetypeChunk,
        dst_col: usize,
        dst_row: u16,
    ) {
        let size = src.layout.columns[src_col].size;
        debug_assert_eq!(size, dst.layout.columns[dst_col].size);
        let from = src.value_ptr(src_col, src_row);
        let to = dst.value_ptr(dst_col, dst_row);
        // SAFETY: różne alokacje albo różne wiersze tej samej — w obu przypadkach
        // zakresy są rozłączne; rozmiar jest identyczny, bo to ten sam komponent.
        unsafe { std::ptr::copy_nonoverlapping(from, to, size) };
    }

    /// Niszczy wartość jednej kolumny w wierszu (przy usuwaniu komponentu).
    ///
    /// # Safety
    /// Wiersz i kolumna muszą istnieć, a wartość być zainicjowana.
    pub unsafe fn drop_column_value(&mut self, col: usize, row: u16) {
        if let Some(drop_fn) = self.layout.columns[col].drop_fn {
            let ptr = self.value_ptr(col, row);
            // SAFETY: warunki przeniesione na wywołującego.
            unsafe { drop_fn(ptr) };
        }
    }

    /// Zapomina wszystkie wiersze **bez** wywołania destruktorów — używane tylko
    /// wtedy, gdy zawartość została już przeniesiona gdzie indziej.
    ///
    /// # Safety
    /// Wartości muszą być wcześniej przeniesione albo zniszczone.
    pub unsafe fn forget_all(&mut self) {
        self.len = 0;
    }

    /// Zdejmuje ostatni wiersz **bez** destruktorów (zawartość już przeniesiona).
    ///
    /// # Safety
    /// Chunk musi mieć co najmniej jeden wiersz, a jego zawartość być przeniesiona.
    pub unsafe fn pop_forget(&mut self) {
        debug_assert!(self.len > 0);
        self.len -= 1;
        self.bump_generation();
    }

    #[inline]
    #[must_use]
    pub fn entity_at(&self, row: u16) -> Entity {
        debug_assert!(row < self.len);
        // SAFETY: `row < len`, więc slot jest zainicjowany.
        unsafe { self.entity_slot(row).read() }
    }

    /// # Safety
    /// `row` musi wskazywać istniejący wiersz.
    pub unsafe fn set_entity(&mut self, row: u16, e: Entity) {
        // SAFETY: warunek przeniesiony na wywołującego.
        unsafe { self.entity_slot(row).write(e) };
    }

    /// Przenosi **ostatni** wiersz chunka `src` na miejsce wiersza `dst_row`
    /// w chunku `dst`. Oba chunki muszą należeć do tego samego archetypu.
    /// Zwraca przeniesioną encję — wywołujący poprawia jej lokalizację.
    ///
    /// # Safety
    /// `dst` i `src` to różne chunki tego samego archetypu, `dst_row < dst.len`,
    /// `src.len > 0`.
    pub unsafe fn move_last_row_between(
        dst: &mut ArchetypeChunk,
        dst_row: u16,
        src: &mut ArchetypeChunk,
        drop_existing: bool,
    ) -> Entity {
        debug_assert!(Arc::ptr_eq(&dst.layout, &src.layout));
        if drop_existing {
            // SAFETY: wiersz docelowy jest zainicjowany i zaraz zostanie nadpisany.
            unsafe { dst.drop_row(dst_row) };
        }
        let src_row = src.len - 1;
        for col in 0..dst.layout.columns.len() {
            // SAFETY: ta sama kolumna w obu chunkach (identyczny layout),
            // oba wiersze istnieją, zakresy są rozłączne (różne alokacje).
            unsafe { ArchetypeChunk::move_column_value(src, col, src_row, dst, col, dst_row) };
        }
        let e = src.entity_at(src_row);
        // SAFETY: `dst_row` istnieje.
        unsafe { dst.set_entity(dst_row, e) };
        // SAFETY: zawartość ostatniego wiersza źródła została właśnie przeniesiona.
        unsafe { src.pop_forget() };
        dst.bump_generation();
        e
    }
}

impl Drop for ArchetypeChunk {
    fn drop(&mut self) {
        for row in 0..self.len {
            // SAFETY: `row < len`, wiersz zainicjowany, chunk znika.
            unsafe { self.drop_row(row) };
        }
        // SAFETY: pamięć pochodzi z `alloc` z tym samym `alloc_layout`.
        unsafe { dealloc(self.data.as_ptr(), self.layout.alloc_layout) };
    }
}

/// Stabilna tożsamość chunka w obrębie uruchomienia.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ChunkId {
    pub archetype: crate::archetype::ArchetypeId,
    pub index: u32,
}

/// Niemutowalny widok na chunk. `Send + Clone`, **bez dostępu mutowalnego** — to jest
/// cała umowa z M12: wątek zapisu trzyma `ChunkRef` i czyta bajty, a symulacja pracuje
/// w tym czasie na swojej kopii. M0 nie daje jeszcze mechanizmu, który tę kopię tworzy.
#[derive(Clone)]
pub struct ChunkRef<'w> {
    id: ChunkId,
    chunk: &'w ArchetypeChunk,
}

// SAFETY: `ChunkRef` daje wyłącznie odczyt danych, które są `Sync` (komponenty są
// `Send + Sync`), a czas życia `'w` gwarantuje, że w tym czasie nikt ich nie mutuje.
unsafe impl Send for ChunkRef<'_> {}

impl<'w> ChunkRef<'w> {
    pub(crate) fn new(id: ChunkId, chunk: &'w ArchetypeChunk) -> ChunkRef<'w> {
        ChunkRef { id, chunk }
    }

    #[must_use]
    pub fn id(&self) -> ChunkId {
        self.id
    }

    #[must_use]
    pub fn len(&self) -> u16 {
        self.chunk.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chunk.is_empty()
    }

    /// Rośnie przy każdej mutacji. Niezmieniona ⇒ sekcja snapshotu jest aktualna.
    #[must_use]
    pub fn generation(&self) -> u32 {
        self.chunk.generation()
    }

    #[must_use]
    pub fn entities(&self) -> &'w [Entity] {
        self.chunk.entities()
    }

    #[must_use]
    pub fn layout(&self) -> &'w ChunkLayout {
        // Arc żyje tak długo jak chunk, a chunk tak długo jak 'w.
        self.chunk.layout()
    }

    /// Surowe bajty jednej kolumny — dokładnie to, co trafia do sekcji snapshotu.
    #[must_use]
    pub fn column_bytes(&self, c: ComponentId) -> Option<&'w [u8]> {
        let col = self.chunk.layout().column_index(c)?;
        // SAFETY: kolumna istnieje w tym archetypie, a widok jest tylko do odczytu.
        Some(unsafe { self.chunk.column_bytes(col) })
    }

    /// Adres wartości komponentu w wierszu — dla odczytu typowanego przez rejestr.
    #[must_use]
    pub fn value_ptr(&self, c: ComponentId, row: u16) -> Option<*const u8> {
        let col = self.chunk.layout().column_index(c)?;
        (row < self.chunk.len()).then(|| self.chunk.value_ptr(col, row).cast_const())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use magnat_core::{HashState, StateHasher};

    #[derive(Debug, Clone, Copy)]
    struct Pos {
        x: i64,
        y: i64,
    }
    impl HashState for Pos {
        fn hash_state(&self, h: &mut StateHasher) {
            h.write_i64(self.x);
            h.write_i64(self.y);
        }
    }
    impl Component for Pos {
        const NAME: &'static str = "Pos";
    }

    #[derive(Debug, Clone, Copy)]
    struct Gruby([u8; 400]);
    impl HashState for Gruby {
        fn hash_state(&self, h: &mut StateHasher) {
            h.write(&self.0);
        }
    }
    impl Component for Gruby {
        const NAME: &'static str = "Gruby";
    }

    fn registry() -> (ComponentRegistry, ComponentId, ComponentId) {
        let mut reg = ComponentRegistry::new();
        let pos = reg.register::<Pos>();
        let gruby = reg.register::<Gruby>();
        (reg, pos, gruby)
    }

    #[test]
    fn chunk_to_jedna_alokacja_do_64_kib() {
        let (reg, pos, gruby) = registry();
        for components in [vec![pos], vec![gruby], vec![pos, gruby]] {
            let layout = ChunkLayout::new(&components, &reg);
            assert!(
                layout.alloc_bytes() <= CHUNK_TARGET_BYTES,
                "chunk {} B dla {components:?}",
                layout.alloc_bytes()
            );
            assert!(layout.rows_per_chunk() >= MIN_ROWS);
            assert!(layout.rows_per_chunk().is_power_of_two());
            assert_eq!(1u16 << layout.row_shift(), layout.rows_per_chunk());
        }
    }

    #[test]
    fn wiersz_400_b_daje_128_wierszy_w_chunku() {
        let (reg, _, gruby) = registry();
        let layout = ChunkLayout::new(&[gruby], &reg);
        assert_eq!(layout.row_bytes(), 408);
        assert_eq!(layout.rows_per_chunk(), 128);
        // Ziarno równoległości: ok. 3 100 chunków na 400 tys. encji, czyli grubo
        // ponad sto chunków na wątek przy 16 wątkach.
        let chunkow = 400_000usize.div_ceil(layout.rows_per_chunk() as usize);
        assert!((2_000..=4_000).contains(&chunkow), "{chunkow} chunków");
        assert!(chunkow / 16 > 100);
    }

    #[test]
    fn kolumny_sa_rozlaczne_i_wyrownane() {
        let (reg, pos, gruby) = registry();
        let layout = ChunkLayout::new(&[pos, gruby], &reg);
        let rows = layout.rows_per_chunk() as usize;
        let mut zakresy: Vec<(usize, usize)> = layout
            .columns()
            .iter()
            .map(|c| (c.offset, c.offset + c.size * rows))
            .collect();
        zakresy.push((0, size_of::<Entity>() * rows));
        zakresy.sort_unstable();
        for para in zakresy.windows(2) {
            assert!(
                para[0].1 <= para[1].0,
                "kolumny zachodzą na siebie: {para:?}"
            );
        }
        for (c, info) in layout.columns().iter().zip([pos, gruby]) {
            assert_eq!(c.offset % reg.info(info).layout().align(), 0);
        }
    }

    #[test]
    fn zapis_i_odczyt_wierszy() {
        let (reg, pos, _) = registry();
        let layout = Arc::new(ChunkLayout::new(&[pos], &reg));
        let mut chunk = ArchetypeChunk::new(layout);
        let e = Entity::from_bits(0x0000_0001_0000_0007).unwrap();
        // SAFETY: zaraz po push inicjujemy jedyną kolumnę.
        let row = unsafe { chunk.push_entity(e) };
        // SAFETY: kolumna 0 to Pos, wiersz istnieje.
        unsafe {
            chunk
                .value_ptr(0, row)
                .cast::<Pos>()
                .write(Pos { x: 3, y: 4 })
        };
        assert_eq!(chunk.entities(), &[e]);
        // SAFETY: wiersz zainicjowany wyżej.
        let p = unsafe { *chunk.value_ptr(0, row).cast::<Pos>() };
        assert_eq!((p.x, p.y), (3, 4));
        assert_eq!(chunk.generation(), 1);
    }

    #[test]
    fn swap_remove_przenosi_ostatni_wiersz() {
        let (reg, pos, _) = registry();
        let layout = Arc::new(ChunkLayout::new(&[pos], &reg));
        let mut chunk = ArchetypeChunk::new(layout);
        let encje: Vec<Entity> = (1..=3)
            .map(|i| Entity::from_bits(0x0000_0001_0000_0000 | i).unwrap())
            .collect();
        for (i, e) in encje.iter().enumerate() {
            // SAFETY: inicjujemy kolumnę natychmiast po push.
            let row = unsafe { chunk.push_entity(*e) };
            // SAFETY: wiersz właśnie powstał.
            unsafe {
                chunk
                    .value_ptr(0, row)
                    .cast::<Pos>()
                    .write(Pos { x: i as i64, y: 0 })
            };
        }
        // SAFETY: wiersz 0 istnieje.
        let przeniesiona = unsafe { chunk.swap_remove_row(0, true) };
        assert_eq!(przeniesiona, Some(encje[2]));
        assert_eq!(chunk.entities(), &[encje[2], encje[1]]);
        // SAFETY: wiersz 0 nadal zainicjowany — teraz zawiera dawny wiersz 2.
        assert_eq!(unsafe { (*chunk.value_ptr(0, 0).cast::<Pos>()).x }, 2);
    }
}
