//! Komponenty i ich rejestr (M0 §5.5).

use magnat_core::collections::{seeded_map, SeededMap};
use magnat_core::{ComponentSchemaId, HashState, StateHasher};
use std::alloc::Layout;
use std::any::TypeId;
use std::fmt::Debug;

/// Komponent = zwykły typ danych. Bez logiki, bez referencji, bez `Drop`
/// w ścieżce gorącej.
///
/// **Nadtypy są mocniejsze niż w planie M0 §5.5 — celowo.** `HashState` i `Debug`
/// są wymaganiami Definition of Done każdej fazy (00 §3.6 i §7): komponent bez hasha
/// wypada z testu determinizmu, komponent bez `Debug` nie pokaże się w karcie inspekcji.
/// Wpisane w nadtypy, są egzekwowane przez kompilator przy rejestracji, a nie przez
/// czyjąś pamięć na przeglądzie kodu — dokładnie tak, jak K-12 robi to z `DecisionReason`.
pub trait Component: Send + Sync + HashState + Debug + 'static {
    /// Nazwa stabilna między wersjami binarki — klucz w tablicy schematu snapshotu.
    /// Zmiana nazwy = zmiana formatu zapisu.
    const NAME: &'static str;

    /// Wersja **układu pól**. Zmiana kształtu komponentu to podbicie wersji,
    /// nie zmiana nazwy (M0 §5.1a) — dopiero ta para pozwala M12 napisać migrację.
    const SCHEMA_VERSION: u16 = 1;

    /// Tożsamość schematu, liczona w czasie kompilacji.
    const SCHEMA_ID: ComponentSchemaId = ComponentSchemaId::new(Self::NAME, Self::SCHEMA_VERSION);
}

/// Nadawany przy rejestracji w danym uruchomieniu. **Nie jest stabilny** między
/// uruchomieniami — w snapshocie zapisujemy `Component::NAME` (00 §5).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ComponentId(u16);

impl ComponentId {
    /// Konstruktor z surowego numeru — wyłącznie dla odczytu snapshotu i testów.
    /// Kod symulacji dostaje `ComponentId` z rejestru, nigdy z liczby.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: u16) -> ComponentId {
        ComponentId(raw)
    }

    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Wszystko, co świat wie o komponencie, którego typu nie zna statycznie.
///
/// Wskaźniki na funkcje zamiast `dyn` : rejestr jest odpytywany w pętli gorącej
/// (hash stanu, snapshot, inspektor), a `dyn` dokładałby tam podwójną dereferencję.
#[derive(Clone)]
pub struct ComponentInfo {
    id: ComponentId,
    name: &'static str,
    schema_id: ComponentSchemaId,
    layout: Layout,
    type_id: TypeId,
    /// `None` dla typów bez destruktora — wtedy usuwanie wiersza to samo przesunięcie bajtów.
    drop_fn: Option<unsafe fn(*mut u8)>,
    hash_fn: unsafe fn(*const u8, &mut StateHasher),
    debug_fn: unsafe fn(*const u8) -> String,
}

impl ComponentInfo {
    #[inline]
    #[must_use]
    pub fn id(&self) -> ComponentId {
        self.id
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    #[inline]
    #[must_use]
    pub fn schema_id(&self) -> ComponentSchemaId {
        self.schema_id
    }

    #[inline]
    #[must_use]
    pub fn layout(&self) -> Layout {
        self.layout
    }

    #[inline]
    #[must_use]
    pub fn size(&self) -> usize {
        self.layout.size()
    }

    #[inline]
    #[must_use]
    pub fn needs_drop(&self) -> bool {
        self.drop_fn.is_some()
    }

    #[inline]
    #[must_use]
    pub fn drop_fn(&self) -> Option<unsafe fn(*mut u8)> {
        self.drop_fn
    }

    /// # Safety
    /// `ptr` musi wskazywać na zainicjowaną wartość tego komponentu.
    #[inline]
    pub unsafe fn hash_value(&self, ptr: *const u8, h: &mut StateHasher) {
        // SAFETY: warunek przeniesiony na wywołującego; `hash_fn` pochodzi
        // z rejestracji tego samego typu, którego dotyczy `ptr`.
        unsafe { (self.hash_fn)(ptr, h) }
    }

    /// # Safety
    /// `ptr` musi wskazywać na zainicjowaną wartość tego komponentu.
    #[inline]
    pub unsafe fn debug_value(&self, ptr: *const u8) -> String {
        // SAFETY: jak wyżej.
        unsafe { (self.debug_fn)(ptr) }
    }

    /// Bezpieczna droga do hasha wartości w chunku — sprawdza, że kolumna należy
    /// do tego komponentu i że wiersz istnieje. Dzięki niej `engine/io` i
    /// `engine/devtools` nie potrzebują własnego `unsafe` (00 §6).
    pub fn hash_in_chunk(
        &self,
        chunk: &crate::chunk::ArchetypeChunk,
        col: usize,
        row: u16,
        h: &mut StateHasher,
    ) {
        assert_eq!(
            chunk.layout().columns()[col].component,
            self.id,
            "kolumna {col} nie należy do komponentu {}",
            self.name
        );
        assert!(row < chunk.len(), "wiersz {row} poza chunkiem");
        // SAFETY: powyższe asercje dowodzą, że pod tym adresem leży zainicjowana
        // wartość tego właśnie typu.
        unsafe { (self.hash_fn)(chunk.value_ptr(col, row).cast_const(), h) }
    }

    /// Jak `hash_in_chunk`, ale zwraca `Debug` — dla inspektora ECS.
    #[must_use]
    pub fn debug_in_chunk(
        &self,
        chunk: &crate::chunk::ArchetypeChunk,
        col: usize,
        row: u16,
    ) -> String {
        assert_eq!(chunk.layout().columns()[col].component, self.id);
        assert!(row < chunk.len());
        // SAFETY: jak wyżej.
        unsafe { (self.debug_fn)(chunk.value_ptr(col, row).cast_const()) }
    }
}

unsafe fn drop_raw<T>(ptr: *mut u8) {
    // SAFETY: wywoływane wyłącznie dla kolumny komponentu T, na wiersz zainicjowany.
    unsafe { std::ptr::drop_in_place(ptr.cast::<T>()) }
}

unsafe fn hash_raw<T: HashState>(ptr: *const u8, h: &mut StateHasher) {
    // SAFETY: jak wyżej — typ kolumny zgadza się z typem funkcji z rejestracji.
    unsafe { (*ptr.cast::<T>()).hash_state(h) }
}

unsafe fn debug_raw<T: Debug>(ptr: *const u8) -> String {
    // SAFETY: jak wyżej.
    unsafe { format!("{:?}", *ptr.cast::<T>()) }
}

/// Rejestr komponentów jednego świata.
///
/// `Clone` istnieje dla odczytu snapshotu: `load_world` buduje nowy świat z tym samym
/// zestawem typów komponentów, a tożsamość typu (`TypeId`) i funkcje pomocnicze
/// są w pełni kopiowalne.
#[derive(Clone)]
pub struct ComponentRegistry {
    infos: Vec<ComponentInfo>,
    by_type: SeededMap<TypeId, ComponentId>,
    by_name: SeededMap<&'static str, ComponentId>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        ComponentRegistry::new()
    }
}

impl ComponentRegistry {
    #[must_use]
    pub fn new() -> ComponentRegistry {
        ComponentRegistry {
            infos: Vec::new(),
            by_type: seeded_map(),
            by_name: seeded_map(),
        }
    }

    /// Rejestracja jest idempotentna: drugie wywołanie zwraca ten sam `ComponentId`.
    pub fn register<T: Component>(&mut self) -> ComponentId {
        let type_id = TypeId::of::<T>();
        if let Some(id) = self.by_type.get(&type_id) {
            return *id;
        }
        assert!(
            !self.by_name.contains_key(T::NAME),
            "dwa komponenty o nazwie {:?} — nazwa jest kluczem formatu zapisu (00 §5)",
            T::NAME
        );
        let id =
            ComponentId(u16::try_from(self.infos.len()).expect("ponad 65 536 typów komponentów"));
        self.infos.push(ComponentInfo {
            id,
            name: T::NAME,
            schema_id: T::SCHEMA_ID,
            layout: Layout::new::<T>(),
            type_id,
            drop_fn: if std::mem::needs_drop::<T>() {
                Some(drop_raw::<T>)
            } else {
                None
            },
            hash_fn: hash_raw::<T>,
            debug_fn: debug_raw::<T>,
        });
        self.by_type.insert(type_id, id);
        self.by_name.insert(T::NAME, id);
        id
    }

    #[must_use]
    pub fn id_of<T: Component>(&self) -> Option<ComponentId> {
        self.by_type.get(&TypeId::of::<T>()).copied()
    }

    #[must_use]
    pub fn id_by_name(&self, name: &str) -> Option<ComponentId> {
        self.by_name.get(name).copied()
    }

    /// Nazwa schematu do komunikatów błędów — zastępuje `ComponentSchemaId::name_hint`
    /// z planu, bez globalnego rejestru w `core` (patrz `core::schema`).
    #[must_use]
    pub fn name_of_schema(&self, schema: ComponentSchemaId) -> Option<&'static str> {
        self.infos
            .iter()
            .find(|i| i.schema_id == schema)
            .map(|i| i.name)
    }

    #[must_use]
    pub fn info(&self, id: ComponentId) -> &ComponentInfo {
        &self.infos[id.index()]
    }

    #[must_use]
    pub fn get(&self, id: ComponentId) -> Option<&ComponentInfo> {
        self.infos.get(id.index())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.infos.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ComponentInfo> {
        self.infos.iter()
    }

    /// Sprawdza, czy `id` odpowiada typowi `T` — używane przy typowanym dostępie
    /// do kolumny, zanim padnie rzutowanie wskaźnika.
    #[must_use]
    pub fn matches_type<T: Component>(&self, id: ComponentId) -> bool {
        self.infos
            .get(id.index())
            .is_some_and(|i| i.type_id == TypeId::of::<T>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Money;

    #[derive(Debug, Clone, Copy)]
    struct Wallet(Money);
    impl HashState for Wallet {
        fn hash_state(&self, h: &mut StateHasher) {
            self.0.hash_state(h);
        }
    }
    impl Component for Wallet {
        const NAME: &'static str = "Wallet";
    }

    #[derive(Debug)]
    struct Label(String);
    impl HashState for Label {
        fn hash_state(&self, h: &mut StateHasher) {
            h.write(self.0.as_bytes());
        }
    }
    impl Component for Label {
        const NAME: &'static str = "Label";
        const SCHEMA_VERSION: u16 = 2;
    }

    #[test]
    fn rejestracja_jest_idempotentna() {
        let mut reg = ComponentRegistry::new();
        let a = reg.register::<Wallet>();
        let b = reg.register::<Wallet>();
        assert_eq!(a, b);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn rejestr_zna_nazwy_i_schematy() {
        let mut reg = ComponentRegistry::new();
        let w = reg.register::<Wallet>();
        let l = reg.register::<Label>();
        assert_eq!(reg.info(w).name(), "Wallet");
        assert_eq!(reg.id_by_name("Label"), Some(l));
        assert_eq!(reg.name_of_schema(Label::SCHEMA_ID), Some("Label"));
        // Wersja schematu zmienia tożsamość, nazwa zostaje.
        assert_ne!(
            Label::SCHEMA_ID,
            ComponentSchemaId::new("Label", 1),
            "SCHEMA_VERSION nie weszła do tożsamości"
        );
    }

    #[test]
    fn destruktor_jest_wykrywany() {
        let mut reg = ComponentRegistry::new();
        let w = reg.register::<Wallet>();
        let l = reg.register::<Label>();
        assert!(!reg.info(w).needs_drop(), "Copy-owy komponent nie ma dropu");
        assert!(
            reg.info(l).needs_drop(),
            "String w komponencie wymaga dropu"
        );
    }

    #[test]
    fn typ_jest_sprawdzalny() {
        let mut reg = ComponentRegistry::new();
        let w = reg.register::<Wallet>();
        let l = reg.register::<Label>();
        assert!(reg.matches_type::<Wallet>(w));
        assert!(!reg.matches_type::<Wallet>(l));
    }
}
