//! Katalog towarów i receptur (M6a, WP1) wraz z walidatorem grafu produktów.
//!
//! **Właścicielem docelowego schematu i pełnego katalogu jest M6** (dok. 00 §5). M2
//! wypełnił go wcześniej minimalnym zestawem, bo Etap 7 generacji nie ma się jak domknąć
//! bez katalogu, i napisał walidator, który wszedł do CI już wtedy. M6a przejmuje jedno
//! i drugie: schemat rośnie do docelowego, walidator dostaje reguły 2–5, a klucze
//! przechodzą na konwencję `<domena>_<nazwa>[_<wariant>]`.
//!
//! Trzy rzeczy, których nie wolno zlać, bo wyglądają podobnie:
//!
//! 1. [`GoodUnit`] ma **dwie** wartości i jest **pochodną** [`GoodForm`] — `Volume` nigdy
//!    nie jest jednostką natywną, tylko funkcją masy przez gęstość albo objętość sztuki.
//! 2. Importowalność to `external_base_price: Option<Money>` — jedno pole, więc stan
//!    „importowalny bez ceny" nie istnieje.
//! 3. `yield` **nie jest przechowywany**: liczy się z mas receptury i `duration_minutes`,
//!    żeby nie mógł się z nimi rozjechać. Stąd bierze się reguła bilansu masy.
//!
//! Ilości w recepturach są **zawsze w gramach**, także dla towarów sztukowych — inaczej
//! bilans `Σ inputs == Σ outputs + process_loss` nie dałby się policzyć na jednej skali.
//! [`GoodUnit::Milliunits`] mówi, w czym towar jest *handlowany*, a `unit_mass`, ile
//! waży sztuka.

mod good;
mod graph;
mod load;
mod recipe;

pub use good::{
    Good, GoodForm, GoodSpec, GoodUnit, HazardClass, NeedCategory, StorageClass, Substitute,
    SubstituteSpec,
};
pub use graph::{GraphWarning, WarningKind};
pub use load::{
    load_default, CatalogError, CATEGORIES_SCHEMA_VERSION, GOODS_SCHEMA_VERSION,
    NEEDS_SCHEMA_VERSION, RECIPES_SCHEMA_VERSION,
};
pub use recipe::{
    CostAllocation, Emissions, MachineClassId, OutputKind, PlumeKind, QualityModel, Recipe,
    RecipeInput, RecipeInputSpec, RecipeOutput, RecipeOutputSpec, RecipeSource, RecipeSpec, Setup,
};

use magnat_core::{GoodId, Mass, NeedCategoryId, Qty, RecipeId, Volume};

/// Katalog towarów, receptur i kategorii potrzeb w jednym miejscu.
///
/// `GoodId`, `RecipeId` i `NeedCategoryId` nadawane są przy ładowaniu, w **stabilnej
/// kolejności alfabetycznej klucza tekstowego** (dok. 00 §5). W zapisie gry trzyma się
/// klucz, nie indeks — dlatego dopisanie towaru nie psuje starych zapisów.
#[derive(Clone, Debug)]
pub struct Catalog {
    pub goods: Vec<Good>,
    pub recipes: Vec<Recipe>,
    pub categories: Vec<NeedCategory>,
    /// Koszyk potrzeb epoki startowej: (towar, gramy na mieszkańca na dobę).
    pub basket: Vec<(GoodId, i64)>,
    /// Klasy maszyn, alfabetycznie, bez powtórzeń. **Wyprowadzone z receptur**, nie
    /// wczytane z osobnego pliku — patrz [`MachineClassId`].
    pub machine_classes: Vec<Box<str>>,
    /// Towary w kategorii, rosnąco po `GoodId`. Indeks, nie dane — odtwarzany przy
    /// ładowaniu, żeby `in_category` nie musiał skanować katalogu.
    by_category: Vec<Vec<GoodId>>,
}

impl Catalog {
    /// Katalog z gotowych części, z odtworzonym indeksem kategorii.
    ///
    /// Drugie wejście obok [`Catalog::load`] — dla atrap testowych i dla tego, kto
    /// zbuduje katalog inaczej niż z `data/`. **Nie waliduje**: walidacja jest w
    /// `load`, bo tam jest miejsce, w którym błąd danych ma zatrzymać uruchomienie.
    #[must_use]
    pub fn from_parts(
        goods: Vec<Good>,
        recipes: Vec<Recipe>,
        categories: Vec<NeedCategory>,
        basket: Vec<(GoodId, i64)>,
    ) -> Catalog {
        let mut by_category = vec![Vec::new(); categories.len()];
        for g in &goods {
            if let Some(v) = by_category.get_mut(g.category.0 as usize) {
                v.push(g.id);
            }
        }
        let mut machine_classes: Vec<Box<str>> = recipes
            .iter()
            .filter(|r| !r.machine_class.is_empty())
            .map(|r| r.machine_class.clone())
            .collect();
        machine_classes.sort_unstable();
        machine_classes.dedup();
        Catalog {
            goods,
            recipes,
            categories,
            basket,
            machine_classes,
            by_category,
        }
    }

    /// Klasa maszyny wymagana przez recepturę. `None` = receptura nie wymaga maszyny
    /// (pakowanie ręczne, uprawa polowa) i pójdzie na dowolnej linii.
    #[must_use]
    pub fn machine_class_of(&self, r: &Recipe) -> Option<MachineClassId> {
        self.machine_class_id(&r.machine_class)
    }

    #[must_use]
    pub fn machine_class_id(&self, key: &str) -> Option<MachineClassId> {
        if key.is_empty() {
            return None;
        }
        self.machine_classes
            .binary_search_by(|c| (**c).cmp(key))
            .ok()
            .map(|i| MachineClassId(i as u16))
    }

    #[must_use]
    pub fn machine_class_key(&self, c: MachineClassId) -> &str {
        &self.machine_classes[c.0 as usize]
    }

    #[must_use]
    pub fn good(&self, g: GoodId) -> &Good {
        &self.goods[g.0 as usize]
    }

    #[must_use]
    pub fn recipe(&self, r: RecipeId) -> &Recipe {
        &self.recipes[r.0 as usize]
    }

    #[must_use]
    pub fn category(&self, c: NeedCategoryId) -> &NeedCategory {
        &self.categories[c.0 as usize]
    }

    #[must_use]
    pub fn good_id(&self, key: &str) -> Option<GoodId> {
        self.goods
            .binary_search_by(|g| (*g.key).cmp(key))
            .ok()
            .map(|i| GoodId(i as u16))
    }

    #[must_use]
    pub fn recipe_id(&self, key: &str) -> Option<RecipeId> {
        self.recipes
            .binary_search_by(|r| (*r.key).cmp(key))
            .ok()
            .map(|i| RecipeId(i as u16))
    }

    #[must_use]
    pub fn category_id(&self, key: &str) -> Option<NeedCategoryId> {
        self.categories
            .binary_search_by(|c| c.key.as_str().cmp(key))
            .ok()
            .map(|i| NeedCategoryId(i as u16))
    }

    /// Towary w kategorii potrzeby — to stąd bierze się lista kandydatów na substytut,
    /// kiedy półka jest pusta (PRD §8.4).
    #[must_use]
    pub fn in_category(&self, c: NeedCategoryId) -> &[GoodId] {
        self.by_category
            .get(c.0 as usize)
            .map_or(&[], |v| v.as_slice())
    }

    /// Masa odpowiadająca liczbie milisztuk. Zero dla postaci sypkich.
    #[must_use]
    pub fn mass_of(&self, g: GoodId, qty: Qty) -> Mass {
        self.good(g).mass_of_qty(qty)
    }

    /// Masa odpowiadająca ilości handlowej — patrz [`Good::mass_of_units`].
    #[must_use]
    pub fn mass_of_units(&self, g: GoodId, qty: Qty) -> Mass {
        self.good(g).mass_of_units(qty)
    }

    /// Ilość handlowa odpowiadająca masie — patrz [`Good::units_of_mass`].
    #[must_use]
    pub fn units_of_mass(&self, g: GoodId, mass: Mass) -> Qty {
        self.good(g).units_of_mass(mass)
    }

    /// Objętość danej masy towaru.
    #[must_use]
    pub fn volume_of(&self, g: GoodId, mass: Mass) -> Volume {
        self.good(g).volume_of(mass)
    }

    /// Klucz tekstowy towaru — droga powrotna `GoodId → klucz` w czasie stałym.
    ///
    /// M5 miał tu przeszukanie liniowe i zapisał to jako sufit do zdjęcia, kiedy katalog
    /// urośnie (`AC-4`). Tu indeks **jest** identyfikatorem, więc pytanie znika razem
    /// z problemem.
    #[must_use]
    pub fn key_of(&self, g: GoodId) -> &str {
        &self.goods[g.0 as usize].key
    }
}
