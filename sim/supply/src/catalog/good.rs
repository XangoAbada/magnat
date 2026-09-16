//! Towar — docelowy schemat katalogu (M6a §5.1) i warunki przechowywania (§5.3).

use magnat_core::{GateKind, GoodId, Mass, Money, NeedCategoryId, StockCat, Volume};
use serde::Deserialize;

/// Postać fizyczna towaru. To ona rozstrzyga, w czym towar jest liczony i jakiej
/// pojemności potrzebuje — jednostka handlowa ([`GoodUnit`]) jest jej funkcją,
/// a nie osobnym polem, żeby nie dało się zadeklarować sypkiego towaru w sztukach.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum GoodForm {
    Bulk,
    Liquid,
    Gas,
    Piece,
    Palletized,
}

/// Jednostka natywna towaru — **publiczna i stabilna**, nie szczegół wewnętrzny
/// `sim/supply`. Czyta ją `sim/macro::lift()` (M10) oraz walidator katalogu.
///
/// Dwie wartości, nie trzy: `Volume` nigdy nie jest jednostką natywną, tylko pochodną
/// masy przez `density_g_per_l` albo `unit_volume`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum GoodUnit {
    /// `Bulk` / `Liquid` / `Gas` — wartość `i64` to gramy.
    Grams,
    /// `Piece` / `Palletized` — wartość `i64` to milisztuki, zawsze wielokrotność 1000.
    Milliunits,
}

/// Wymagana klasa magazynu (§5.3). Logika działa na niej, nie na krzywej temperatury:
/// dwa stany — warunki spełnione albo nie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[repr(u8)]
pub enum StorageClass {
    Ambient,
    Dry,
    Chilled,
    Frozen,
    Silo,
    Tank,
    PressureTank,
    Yard,
    Secure,
}

/// Klasa niebezpieczeństwa (§5.3). Dla `Flammable` / `Explosive` / `Toxic` slot bez maski
/// **odmawia przyjęcia** partii — to granica zaufania, nie miejsce na uproszczenie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[repr(u8)]
pub enum HazardClass {
    None,
    Flammable,
    Explosive,
    Toxic,
    Corrosive,
    Perishable,
    Oversize,
}

impl StorageClass {
    /// Czy slot tej klasy przyjmuje towar wymagający klasy `req` **w warunkach**.
    ///
    /// Nie modelujemy ciągłej krzywej temperatury — dwa stany: warunki spełnione albo
    /// niespełnione. Drabinka jest krótka i fizyczna: mroźnia utrzyma chłodnię, chłodnia
    /// utrzyma suche i otoczenie, suchy magazyn utrzyma otoczenie. Reszta to równość.
    /// Niespełnione warunki **nie blokują** przyjęcia — dają mnożnik psucia ×8.
    #[must_use]
    pub const fn accepts(self, req: StorageClass) -> bool {
        use StorageClass::{Ambient, Chilled, Dry, Frozen};
        match (self, req) {
            (Frozen, Frozen | Chilled | Dry | Ambient) => true,
            (Chilled, Chilled | Dry | Ambient) => true,
            (Dry, Dry | Ambient) => true,
            (a, b) => a as u8 == b as u8,
        }
    }
}

impl HazardClass {
    /// Bit tej klasy w masce slotu.
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    /// Czy klasa wymaga slotu z jawną zgodą. Dla tych trzech `put` po prostu się nie
    /// powiedzie — to granica zaufania, nie miejsce na uproszczenie.
    #[must_use]
    pub const fn needs_permit(self) -> bool {
        matches!(
            self,
            HazardClass::Flammable | HazardClass::Explosive | HazardClass::Toxic
        )
    }
}

/// Substytut: czym podmienić i ile to kosztuje na jakości.
///
/// `mass_ratio_permille` mówi, ile gramów substytutu wchodzi za 1000 g oryginału —
/// promile, bo zamiana rzadko jest jeden do jednego (olej rzepakowy za słonecznikowy
/// idzie 1:1, ale mąka żytnia za pszenną już nie).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct Substitute {
    pub good: GoodId,
    pub mass_ratio_permille: u16,
    pub quality_penalty: u8,
}

/// Substytut przed rozwiązaniem klucza na [`GoodId`] — postać z pliku danych.
#[derive(Clone, Debug, Deserialize)]
pub struct SubstituteSpec {
    pub good: String,
    pub mass_ratio_permille: u16,
    #[serde(default)]
    pub quality_penalty: u8,
}

/// Kategoria potrzeby — drobny podział, ten, który odpowiada na pytanie „czym to
/// podmienić" (PRD §8.4). Mieszka w `data/needs/categories.ron`.
///
/// `stock_cat` wiąże ją z grubym podziałem gospodarstwa domowego z M5 ([`StockCat`]).
/// `None` znaczy „to nie jest towar, który gospodarstwo trzyma w spiżarni" — ruda żelaza
/// i olej smarowy takiej kategorii nie mają i nie powinny jej udawać.
#[derive(Clone, Debug, Deserialize)]
pub struct NeedCategory {
    pub key: String,
    #[serde(default)]
    pub stock_cat: Option<StockCat>,
}

/// Towar w katalogu.
///
/// `key` jest tym, co idzie do zapisu gry; `id` to indeks nadawany przy ładowaniu
/// w kolejności alfabetycznej klucza (dok. 00 §5), więc dopisanie towaru nie psuje
/// starych zapisów, ale przenumerowuje indeksy w obrębie sesji.
///
/// Konwencja klucza: `<domena>_<nazwa>[_<wariant>]` — `food_bread_wheat`,
/// `fuel_diesel_b7`, `part_bearing_6204`, `raw_crude_oil`.
#[derive(Clone, Debug)]
pub struct Good {
    pub key: Box<str>,
    pub id: GoodId,
    pub category: NeedCategoryId,
    pub form: GoodForm,
    /// Gęstość — jedyne źródło objętości dla `Bulk` / `Liquid` / `Gas`
    /// (00 §2: bez floatów w stanie magazynowym).
    pub density_g_per_l: u32,
    /// Masa jednej sztuki netto. Zero dla postaci sypkich.
    pub unit_mass: Mass,
    /// Objętość jednej sztuki brutto, z opakowaniem. Zero dla postaci sypkich.
    pub unit_volume: Volume,
    /// `None` = towar się nie psuje.
    pub shelf_life_minutes: Option<u32>,
    pub storage: StorageClass,
    pub hazard: HazardClass,
    /// Towary homogeniczne (piasek, woda) mają `quality = 50` i koniec.
    pub has_quality: bool,
    pub substitutes: Vec<Substitute>,
    /// Cena bazowa dostawcy zewnętrznego, w groszach za 1000 jednostek natywnych
    /// (za kilogram dla `Grams`, za sztukę dla `Milliunits`).
    /// `None` = **nieimportowalny**; jedno pole, więc nie da się mieć stanu
    /// „importowalny bez ceny".
    pub external_base_price: Option<Money>,
    /// Bramy, przez które ten towar wchodzi do miasta. Puste = wszystkie towarowe.
    pub import_via: Vec<GateKind>,
    /// Koszt utylizacji w groszach za tonę — dla odpadów. Zero dla reszty.
    pub disposal_cost: Money,
}

/// Towar w postaci, w jakiej stoi w pliku RON — przed nadaniem `id` i rozwiązaniem
/// kluczy substytutów.
#[derive(Clone, Debug, Deserialize)]
pub struct GoodSpec {
    pub key: String,
    pub category: String,
    pub form: GoodForm,
    #[serde(default)]
    pub density_g_per_l: u32,
    #[serde(default)]
    pub unit_mass_g: i64,
    #[serde(default)]
    pub unit_volume_ml: i64,
    #[serde(default)]
    pub shelf_life_minutes: Option<u32>,
    pub storage: StorageClass,
    #[serde(default = "hazard_none")]
    pub hazard: HazardClass,
    #[serde(default = "prawda")]
    pub has_quality: bool,
    #[serde(default)]
    pub substitutes: Vec<SubstituteSpec>,
    #[serde(default)]
    pub external_base_price: Option<Money>,
    #[serde(default)]
    pub import_via: Vec<GateKind>,
    #[serde(default)]
    pub disposal_cost: Money,
}

const fn hazard_none() -> HazardClass {
    HazardClass::None
}

const fn prawda() -> bool {
    true
}

impl GoodForm {
    /// Jednostka natywna wynika z postaci — i to jest jedyne miejsce, w którym ten
    /// związek jest zapisany. Gdyby jednostka była osobnym polem w danych, dałoby się
    /// zadeklarować „sypki, liczony w sztukach" i walidator musiałby tego pilnować;
    /// tak ten stan po prostu nie istnieje.
    #[must_use]
    pub const fn unit(self) -> GoodUnit {
        match self {
            GoodForm::Bulk | GoodForm::Liquid | GoodForm::Gas => GoodUnit::Grams,
            GoodForm::Piece | GoodForm::Palletized => GoodUnit::Milliunits,
        }
    }

    /// Czy postać jest sypka/ciekła/gazowa — czyli czy objętość liczy się z gęstości.
    #[must_use]
    pub const fn is_bulk(self) -> bool {
        matches!(self, GoodForm::Bulk | GoodForm::Liquid | GoodForm::Gas)
    }
}

impl Good {
    /// Jednostka natywna — pochodna postaci, patrz [`GoodForm::unit`].
    #[must_use]
    pub const fn unit(&self) -> GoodUnit {
        self.form.unit()
    }

    /// Czy towar **da się** sprowadzić — samo pole ceny, bez sprawdzania bram.
    /// Pełna definicja `importable(g)` wymaga jeszcze bramy zgodnego typu i mieszka
    /// po stronie miasta, bo dopiero tam wiadomo, jakie bramy miasto ma.
    #[must_use]
    pub const fn has_external_price(&self) -> bool {
        self.external_base_price.is_some()
    }

    /// Bramy, przez które wolno sprowadzić ten towar.
    #[must_use]
    pub fn gates(&self) -> &[GateKind] {
        if self.import_via.is_empty() {
            const TOWAROWE: [GateKind; 3] =
                [GateKind::Highway, GateKind::RailFreight, GateKind::Port];
            &TOWAROWE
        } else {
            &self.import_via
        }
    }

    /// Objętość danej masy tego towaru, w mililitrach.
    ///
    /// Dla postaci sypkich z gęstości; dla sztukowych z masy sztuki i objętości brutto
    /// sztuki, bo pudełko zajmuje więcej niż to, co w nim jest. Zaokrąglenie w górę:
    /// pojemność magazynu ma się nie przepełnić przez zaokrąglenie w dół.
    #[must_use]
    pub fn volume_of(&self, mass: Mass) -> Volume {
        if mass.0 <= 0 {
            return Volume::ZERO;
        }
        if self.form.is_bulk() {
            let d = i128::from(self.density_g_per_l.max(1));
            Volume(sufit_dzielenia(i128::from(mass.0) * 1000, d))
        } else {
            let um = i128::from(self.unit_mass.0.max(1));
            let sztuk = sufit_dzielenia(i128::from(mass.0), um);
            Volume(i128::from(sztuk).saturating_mul(i128::from(self.unit_volume.0)) as i64)
        }
    }

    /// Masa odpowiadająca danej liczbie milisztuk. Zero dla postaci sypkich —
    /// tam jednostką natywną jest gram i pytanie nie ma sensu.
    #[must_use]
    pub fn mass_of_qty(&self, qty: magnat_core::Qty) -> Mass {
        if self.form.is_bulk() {
            return Mass::ZERO;
        }
        Mass((i128::from(qty.0) * i128::from(self.unit_mass.0) / 1000) as i64)
    }

    /// Masa odpowiadająca ilości **handlowej** — tej, którą liczy detal (M5).
    ///
    /// To jest przelicznik, którego szukał `AD-4` pod nazwą `Good::pack`, i okazuje
    /// się, że nie potrzebuje ani jednego nowego pola: `GoodUnit` już mówi, czym jest
    /// jednostka natywna towaru, a `unit_mass` — ile waży sztuka. Dla postaci sypkich
    /// i ciekłych ilość **jest** masą w gramach (`Qty(800)` = 800 g chleba), dla
    /// sztukowych jest w milisztukach i przelicza się przez masę sztuki. Pole „pack"
    /// dokładałoby trzecią liczbę do dwóch, które już się zgadzają (`AL-4`).
    #[must_use]
    pub fn mass_of_units(&self, qty: magnat_core::Qty) -> Mass {
        if self.form.is_bulk() {
            Mass(qty.0)
        } else {
            self.mass_of_qty(qty)
        }
    }

    /// Droga powrotna: ile jednostek handlowych niesie ta masa. Zaokrąglenie **w dół**,
    /// bo pół bochenka nie stoi na półce.
    #[must_use]
    pub fn units_of_mass(&self, mass: Mass) -> magnat_core::Qty {
        if self.form.is_bulk() {
            return magnat_core::Qty(mass.0.max(0));
        }
        let um = self.unit_mass.0.max(1);
        magnat_core::Qty((i128::from(mass.0.max(0)) * 1000 / i128::from(um)) as i64)
    }
}

fn sufit_dzielenia(a: i128, b: i128) -> i64 {
    debug_assert!(b > 0);
    let q = (a + b - 1) / b;
    i64::try_from(q).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn towar(form: GoodForm, gestosc: u32, masa_szt: i64, obj_szt: i64) -> Good {
        Good {
            key: "x".into(),
            id: GoodId(0),
            category: NeedCategoryId(0),
            form,
            density_g_per_l: gestosc,
            unit_mass: Mass(masa_szt),
            unit_volume: Volume(obj_szt),
            shelf_life_minutes: None,
            storage: StorageClass::Ambient,
            hazard: HazardClass::None,
            has_quality: true,
            substitutes: Vec::new(),
            external_base_price: None,
            import_via: Vec::new(),
            disposal_cost: Money::ZERO,
        }
    }

    #[test]
    fn jednostka_wynika_z_postaci() {
        assert_eq!(GoodForm::Bulk.unit(), GoodUnit::Grams);
        assert_eq!(GoodForm::Liquid.unit(), GoodUnit::Grams);
        assert_eq!(GoodForm::Gas.unit(), GoodUnit::Grams);
        assert_eq!(GoodForm::Piece.unit(), GoodUnit::Milliunits);
        assert_eq!(GoodForm::Palletized.unit(), GoodUnit::Milliunits);
    }

    /// Objętość sypkiego liczy się z gęstości; 870 g/l to ropa, więc tona zajmuje
    /// 1 149,4 l — i zaokrągla się **w górę**, bo zbiornik ma się nie przelać.
    #[test]
    fn objetosc_sypkiego_z_gestosci_i_w_gore() {
        let ropa = towar(GoodForm::Liquid, 870, 0, 0);
        assert_eq!(ropa.volume_of(Mass(1_000_000)), Volume(1_149_426));
        assert_eq!(ropa.volume_of(Mass(1)), Volume(2));
        assert_eq!(ropa.volume_of(Mass(0)), Volume::ZERO);
    }

    /// Sztukowy liczy objętość z opakowania, nie z masy: pół sztuki i tak zajmuje
    /// całe pudełko.
    #[test]
    fn objetosc_sztukowego_z_opakowania() {
        let lozysko = towar(GoodForm::Piece, 0, 400, 900);
        assert_eq!(lozysko.volume_of(Mass(400)), Volume(900));
        assert_eq!(lozysko.volume_of(Mass(401)), Volume(1800));
        assert_eq!(lozysko.mass_of_qty(magnat_core::Qty(3000)), Mass(1200));
        assert_eq!(
            towar(GoodForm::Bulk, 800, 0, 0).mass_of_qty(magnat_core::Qty(3000)),
            Mass::ZERO
        );
    }
}
