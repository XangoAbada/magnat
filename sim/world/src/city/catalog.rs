//! Minimalny katalog towarów i receptur (M2 §5.8) wraz z walidatorem grafu produktów.
//!
//! **Właścicielem docelowego schematu i katalogu (~400 towarów) jest M6** (decyzja 9.1/4).
//! M2 wypełnia go wcześniej, bo Etap 7 nie ma się jak domknąć bez katalogu, i pisze
//! walidator, który wchodzi do CI już teraz (dok. 00 §5, zmieniony przez tę decyzję).
//!
//! Trzy rzeczy przyjęte od M6 bez zmian i **nie do zlania** z polami M2 (9.1/13, 9.1/16):
//!
//! 1. `GoodUnit` ma **dwie** wartości, nie trzy. `Volume` jest zawsze pochodną masy przez
//!    `density_g_per_l`, więc nie jest jednostką natywną.
//! 2. Importowalność to `external_base_price: Option<Money>` — jedno pole, więc nie da się
//!    mieć stanu „importowalny bez ceny". Epoki dostępności importu należą do M10.
//! 3. `yield` **nie jest przechowywany**: liczy się z mas receptury i `duration_minutes`,
//!    żeby nie mógł się z nimi rozjechać. Stąd bierze się reguła bilansu masy niżej.
//!
//! Ilości w recepturach są **zawsze w gramach**, także dla towarów sztukowych — inaczej
//! bilans masy `Σ inputs == Σ outputs + process_loss` nie dałby się policzyć na jednej
//! skali. `GoodUnit::Milliunits` mówi, w czym towar jest *handlowany*, a `unit_mass_g`,
//! ile waży sztuka.

use magnat_core::{GoodId, Money, RecipeId, ResourceKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

use super::gates::GateKind;

pub const GOODS_SCHEMA_VERSION: u32 = 1;
pub const RECIPES_SCHEMA_VERSION: u32 = 1;
pub const NEEDS_SCHEMA_VERSION: u32 = 1;

/// Jednostka handlowa towaru (M6 §5.1, przyjęte w 9.1/16).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum GoodUnit {
    /// Sypkie, ciekłe, gazowe — masa jest jednostką naturalną.
    Grams,
    /// Sztukowe i paletowe — milisztuki, zawsze wielokrotność 1000.
    Milliunits,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Good {
    pub key: String,
    pub unit: GoodUnit,
    /// Gęstość — jedyne źródło objętości (00 §2: bez floatów w stanie magazynowym).
    pub density_g_per_l: u32,
    /// Masa sztuki w gramach. Wymagana dla [`GoodUnit::Milliunits`], zero dla `Grams`.
    #[serde(default)]
    pub unit_mass_g: i64,
    /// Cena bazowa dostawcy zewnętrznego. `None` = towaru nie da się sprowadzić.
    #[serde(default)]
    pub external_base_price: Option<Money>,
    /// Bramy, przez które ten towar wchodzi do miasta. Puste = wszystkie towarowe.
    #[serde(default)]
    pub import_via: Vec<GateKind>,
}

impl Good {
    /// Czy towar **da się** sprowadzić — samo pole ceny, bez sprawdzania bram.
    /// Pełna definicja `importable(g)` z §5.8 wymaga jeszcze bramy zgodnego typu
    /// i mieszka w [`super::sites`], bo dopiero tam wiadomo, jakie bramy miasto ma.
    #[must_use]
    pub fn has_external_price(&self) -> bool {
        self.external_base_price.is_some()
    }

    /// Bramy, przez które wolno sprowadzić ten towar.
    #[must_use]
    pub fn gates(&self) -> &[GateKind] {
        if self.import_via.is_empty() {
            const TOWAROWE: [GateKind; 3] = [GateKind::Highway, GateKind::RailFreight, GateKind::Port];
            &TOWAROWE
        } else {
            &self.import_via
        }
    }
}

/// Punkt wejścia łańcucha rozpoznawany **jawną flagą**, nie brakiem wejść (9.1/14).
/// Pole dodane przez M6 dokładnie na potrzeby domknięcia z §5.8.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum RecipeSource {
    Manufacturing,
    Extraction(ResourceKind),
    Agriculture,
}

impl RecipeSource {
    /// Czy receptura jest punktem wejścia domknięcia (KROK 1 z §5.8).
    #[must_use]
    pub const fn is_entry(self) -> bool {
        matches!(self, RecipeSource::Extraction(_) | RecipeSource::Agriculture)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RecipeSpec {
    pub key: String,
    pub source: RecipeSource,
    /// Wejścia szarży w gramach.
    #[serde(default)]
    pub inputs: Vec<(String, i64)>,
    /// Wyjścia szarży w gramach.
    pub outputs: Vec<(String, i64)>,
    /// Ubytek procesowy w gramach — domyka bilans masy.
    #[serde(default)]
    pub process_loss_g: i64,
    pub duration_minutes: u32,
    /// **Osobominuty szarży**, nie obsada etatowa (9.1/13). Obsada jest w archetypie.
    #[serde(default)]
    pub labour: Vec<(String, u32)>,
    /// Abstrakcyjna zdolność maszynowa — receptura nie zna obiektu fizycznego.
    #[serde(default)]
    pub machine_class: String,
}

/// Receptura z kluczami rozwiązanymi na identyfikatory.
#[derive(Clone, Debug)]
pub struct Recipe {
    pub spec: RecipeSpec,
    pub inputs: Vec<(GoodId, i64)>,
    pub outputs: Vec<(GoodId, i64)>,
}

impl Recipe {
    #[must_use]
    pub fn key(&self) -> &str {
        &self.spec.key
    }

    #[must_use]
    pub fn source(&self) -> RecipeSource {
        self.spec.source
    }

    /// Dobowa wydajność receptury dla towaru `g`, w gramach przy skali bazowej.
    /// `yield` nie jest polem — liczy się tutaj, z mas i czasu szarży (9.1/16).
    #[must_use]
    pub fn daily_yield(&self, g: GoodId) -> i64 {
        let masa = self
            .outputs
            .iter()
            .find(|(k, _)| *k == g)
            .map_or(0, |(_, m)| *m);
        masa.saturating_mul(1440) / i64::from(self.spec.duration_minutes.max(1))
    }

    /// Dobowe zapotrzebowanie na wejście `g`, w gramach przy skali bazowej.
    #[must_use]
    pub fn daily_input(&self, g: GoodId) -> i64 {
        let masa = self
            .inputs
            .iter()
            .find(|(k, _)| *k == g)
            .map_or(0, |(_, m)| *m);
        masa.saturating_mul(1440) / i64::from(self.spec.duration_minutes.max(1))
    }
}

// ── Pliki danych ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GoodsFile {
    schema_version: u32,
    goods: Vec<Good>,
}

#[derive(Deserialize)]
struct RecipesFile {
    schema_version: u32,
    recipes: Vec<RecipeSpec>,
}

/// Koszyk potrzeb epoki — gramy na mieszkańca na dobę.
///
/// Mieszka w `data/chains/`, czyli w katalogu M2, a **nie** w `Good`: model potrzeb
/// należy do M3/M5 i nie ma powodu, żeby M6 dostał go w swoim schemacie towaru.
/// Kiedy M5 zbuduje prawdziwy koszyk, ten plik znika, a nie zmienia właściciela.
#[derive(Deserialize)]
struct NeedsFile {
    schema_version: u32,
    /// Klucz epoki z `data/epochs/` → lista (towar, g/osobę/dobę).
    baskets: Vec<(String, Vec<(String, i64)>)>,
}

#[derive(Debug)]
pub enum CatalogError {
    Io(std::io::Error),
    Ron { file: String, msg: String },
    Schema { file: String, found: u32, want: u32 },
    DuplicateGood(String),
    DuplicateRecipe(String),
    UnknownGood { recipe: String, good: String },
    MassBalance { recipe: String, inputs: i64, outputs: i64, loss: i64 },
    BadDuration(String),
    MissingUnitMass(String),
    NoOutputs(String),
    /// Towar, do którego nie prowadzi żadna ścieżka z wydobycia, rolnictwa ani importu.
    Unreachable(Vec<String>),
    UnknownEpoch(String),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::Io(e) => write!(f, "katalog: {e}"),
            CatalogError::Ron { file, msg } => write!(f, "{file}: {msg}"),
            CatalogError::Schema { file, found, want } => {
                write!(f, "{file}: schema_version {found}, oczekiwano {want}")
            }
            CatalogError::DuplicateGood(k) => write!(f, "towar `{k}` zdefiniowany dwa razy"),
            CatalogError::DuplicateRecipe(k) => write!(f, "receptura `{k}` zdefiniowana dwa razy"),
            CatalogError::UnknownGood { recipe, good } => {
                write!(f, "receptura `{recipe}` odwołuje się do nieznanego towaru `{good}`")
            }
            CatalogError::MassBalance { recipe, inputs, outputs, loss } => write!(
                f,
                "receptura `{recipe}`: bilans masy {inputs} g wejść wobec {outputs} g wyjść + {loss} g strat"
            ),
            CatalogError::BadDuration(k) => write!(f, "receptura `{k}`: zerowy czas szarży"),
            CatalogError::MissingUnitMass(k) => {
                write!(f, "towar `{k}` jest sztukowy, ale nie ma `unit_mass_g`")
            }
            CatalogError::NoOutputs(k) => write!(f, "receptura `{k}` nie produkuje niczego"),
            CatalogError::Unreachable(v) => write!(
                f,
                "{} towarów bez źródła (ani wydobycie, ani rolnictwo, ani import): {}",
                v.len(),
                v.join(", ")
            ),
            CatalogError::UnknownEpoch(k) => write!(f, "koszyk potrzeb dla nieznanej epoki `{k}`"),
        }
    }
}

impl std::error::Error for CatalogError {}

impl From<std::io::Error> for CatalogError {
    fn from(e: std::io::Error) -> CatalogError {
        CatalogError::Io(e)
    }
}

/// Katalog towarów i receptur w jednym miejscu.
///
/// `GoodId` i `RecipeId` nadawane są przy ładowaniu, w **stabilnej kolejności
/// alfabetycznej klucza tekstowego** (dok. 00 §5). W zapisie gry trzyma się klucz,
/// nie indeks — dlatego dopisanie towaru nie psuje starych zapisów.
#[derive(Clone, Debug)]
pub struct Catalog {
    pub goods: Vec<Good>,
    pub recipes: Vec<Recipe>,
    /// Koszyk potrzeb epoki startowej: (towar, gramy na mieszkańca na dobę).
    pub basket: Vec<(GoodId, i64)>,
}

impl Catalog {
    /// Ładuje `data/goods/`, `data/recipes/` i koszyk z `data/chains/needs.ron`,
    /// po czym **waliduje** całość. Błąd danych jest błędem, nie ostrzeżeniem:
    /// katalog z dziurą daje miasto, które nie domyka Etapu 7, a to widać dopiero
    /// na końcu generacji.
    pub fn load(goods_dir: &Path, recipes_dir: &Path, needs: &Path, epoch_key: &str) -> Result<Catalog, CatalogError> {
        let mut goods: Vec<Good> = Vec::new();
        for path in pliki_ron(goods_dir)? {
            let name = path.display().to_string();
            let txt = std::fs::read_to_string(&path)?;
            let f: GoodsFile = ron::from_str(&txt).map_err(|e| CatalogError::Ron {
                file: name.clone(),
                msg: e.to_string(),
            })?;
            if f.schema_version != GOODS_SCHEMA_VERSION {
                return Err(CatalogError::Schema {
                    file: name,
                    found: f.schema_version,
                    want: GOODS_SCHEMA_VERSION,
                });
            }
            goods.extend(f.goods);
        }
        goods.sort_by(|a, b| a.key.cmp(&b.key));
        for w in goods.windows(2) {
            if w[0].key == w[1].key {
                return Err(CatalogError::DuplicateGood(w[0].key.clone()));
            }
        }
        // Indeks klucz → GoodId. `BTreeMap`, nie `HashMap`: po tej mapie się iteruje
        // w walidatorze, a po `HashMap` iterować nie wolno (00 §3.2).
        let idx: BTreeMap<&str, GoodId> = goods
            .iter()
            .enumerate()
            .map(|(i, g)| (g.key.as_str(), GoodId(i as u16)))
            .collect();

        for g in &goods {
            if g.unit == GoodUnit::Milliunits && g.unit_mass_g <= 0 {
                return Err(CatalogError::MissingUnitMass(g.key.clone()));
            }
        }

        let mut specs: Vec<RecipeSpec> = Vec::new();
        for path in pliki_ron(recipes_dir)? {
            let name = path.display().to_string();
            let txt = std::fs::read_to_string(&path)?;
            let f: RecipesFile = ron::from_str(&txt).map_err(|e| CatalogError::Ron {
                file: name.clone(),
                msg: e.to_string(),
            })?;
            if f.schema_version != RECIPES_SCHEMA_VERSION {
                return Err(CatalogError::Schema {
                    file: name,
                    found: f.schema_version,
                    want: RECIPES_SCHEMA_VERSION,
                });
            }
            specs.extend(f.recipes);
        }
        specs.sort_by(|a, b| a.key.cmp(&b.key));
        for w in specs.windows(2) {
            if w[0].key == w[1].key {
                return Err(CatalogError::DuplicateRecipe(w[0].key.clone()));
            }
        }

        let mut recipes: Vec<Recipe> = Vec::with_capacity(specs.len());
        for spec in specs {
            if spec.duration_minutes == 0 {
                return Err(CatalogError::BadDuration(spec.key.clone()));
            }
            if spec.outputs.is_empty() {
                return Err(CatalogError::NoOutputs(spec.key.clone()));
            }
            let rozwiaz = |v: &[(String, i64)]| -> Result<Vec<(GoodId, i64)>, CatalogError> {
                v.iter()
                    .map(|(k, m)| {
                        idx.get(k.as_str()).copied().map(|g| (g, *m)).ok_or_else(|| {
                            CatalogError::UnknownGood {
                                recipe: spec.key.clone(),
                                good: k.clone(),
                            }
                        })
                    })
                    .collect()
            };
            let inputs = rozwiaz(&spec.inputs)?;
            let outputs = rozwiaz(&spec.outputs)?;

            // Bilans masy obowiązuje **wyłącznie wytwarzanie**. Kopalnia i pole nie mają
            // wejścia towarowego — ich masa pochodzi ze złoża i z gleby, a te nie są
            // towarem. Gdyby reguła obejmowała też je, każdy punkt wejścia łańcucha
            // musiałby być formalnie nielegalny.
            if spec.source == RecipeSource::Manufacturing {
                let we: i64 = inputs.iter().map(|(_, m)| *m).sum();
                let wy: i64 = outputs.iter().map(|(_, m)| *m).sum();
                if we != wy + spec.process_loss_g {
                    return Err(CatalogError::MassBalance {
                        recipe: spec.key.clone(),
                        inputs: we,
                        outputs: wy,
                        loss: spec.process_loss_g,
                    });
                }
            }
            recipes.push(Recipe {
                spec,
                inputs,
                outputs,
            });
        }

        let txt = std::fs::read_to_string(needs)?;
        let nf: NeedsFile = ron::from_str(&txt).map_err(|e| CatalogError::Ron {
            file: needs.display().to_string(),
            msg: e.to_string(),
        })?;
        if nf.schema_version != NEEDS_SCHEMA_VERSION {
            return Err(CatalogError::Schema {
                file: needs.display().to_string(),
                found: nf.schema_version,
                want: NEEDS_SCHEMA_VERSION,
            });
        }
        let lista = nf
            .baskets
            .iter()
            .find(|(k, _)| k == epoch_key)
            .map(|(_, v)| v)
            .ok_or_else(|| CatalogError::UnknownEpoch(epoch_key.to_string()))?;
        let mut basket: Vec<(GoodId, i64)> = Vec::with_capacity(lista.len());
        for (k, g_na_dobe) in lista {
            let id = idx
                .get(k.as_str())
                .copied()
                .ok_or_else(|| CatalogError::UnknownGood {
                    recipe: "needs".to_string(),
                    good: k.clone(),
                })?;
            basket.push((id, *g_na_dobe));
        }
        basket.sort_by_key(|(g, _)| g.0);

        let cat = Catalog {
            goods,
            recipes,
            basket,
        };
        cat.validate_reachability()?;
        Ok(cat)
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
    pub fn good_id(&self, key: &str) -> Option<GoodId> {
        self.goods
            .binary_search_by(|g| g.key.as_str().cmp(key))
            .ok()
            .map(|i| GoodId(i as u16))
    }

    #[must_use]
    pub fn recipe_id(&self, key: &str) -> Option<RecipeId> {
        self.recipes
            .binary_search_by(|r| r.spec.key.as_str().cmp(key))
            .ok()
            .map(|i| RecipeId(i as u16))
    }

    /// **Walidator grafu produktów** — ten, który wg dok. 00 §5 wchodzi do CI już w M2.
    ///
    /// Pyta o katalog, nie o miasto: czy z samych punktów wejścia (wydobycie, rolnictwo,
    /// import) da się w ogóle dojść do każdego towaru. Miasto sprawdza to jeszcze raz,
    /// ale na zbiorze receptur **zainstancjonowanych** — patrz `sites::supply_closure_check`.
    ///
    /// Zapas startowy (`data/scenarios/initial_stock.ron`) **nie jest** źródłem (9.1/15):
    /// łańcuch domknięty zapasem wystartuje raz i nigdy się nie odtworzy.
    pub fn validate_reachability(&self) -> Result<(), CatalogError> {
        let seed: Vec<GoodId> = self
            .goods
            .iter()
            .enumerate()
            .filter(|(_, g)| g.has_external_price())
            .map(|(i, _)| GoodId(i as u16))
            .collect();
        let dostepne: Vec<RecipeId> = (0..self.recipes.len() as u16).map(RecipeId).collect();
        let osiagalne = self.reachable(&seed, &dostepne);
        let brak: Vec<String> = self
            .goods
            .iter()
            .enumerate()
            .filter(|(i, _)| !osiagalne[*i])
            .map(|(_, g)| g.key.clone())
            .collect();
        if brak.is_empty() {
            Ok(())
        } else {
            Err(CatalogError::Unreachable(brak))
        }
    }

    /// Domknięcie osiągalności na hipergrafie receptur — schemat Dowlinga–Galliera,
    /// O(V + E), bez przechodzenia na punkt stały (9.1/14).
    ///
    /// `seed` to towary dane z zewnątrz (import), `available` to receptury, które wolno
    /// odpalić. Punkty wejścia rozpoznaje **jawna flaga** `RecipeSource`, nie puste
    /// `inputs` — receptura bez wejść, która nie jest wydobyciem, byłaby błędem danych,
    /// a nie kopalnią.
    #[must_use]
    pub fn reachable(&self, seed: &[GoodId], available: &[RecipeId]) -> Vec<bool> {
        let mut osiagalne = vec![false; self.goods.len()];
        // Ile wejść receptury jeszcze brakuje; `u32::MAX` = receptura niedostępna.
        let mut brakow = vec![u32::MAX; self.recipes.len()];
        let mut kolejka: std::collections::VecDeque<RecipeId> = std::collections::VecDeque::new();

        let mut lista: Vec<RecipeId> = available.to_vec();
        lista.sort_by_key(|r| r.0);
        lista.dedup();
        for r in &lista {
            let rec = self.recipe(*r);
            brakow[r.0 as usize] = rec.inputs.len() as u32;
        }
        // KROK 1: punktami wejścia są wyłącznie wydobycie, rolnictwo i import.
        for g in seed {
            osiagalne[g.0 as usize] = true;
        }
        for r in &lista {
            if self.recipe(*r).source().is_entry() {
                brakow[r.0 as usize] = 0;
            }
        }
        // Receptury gotowe od razu — rosnąco po `RecipeId`, żeby kolejność była danymi,
        // a nie przypadkiem (00 §3.2).
        for r in &lista {
            if brakow[r.0 as usize] == 0 {
                kolejka.push_back(*r);
            }
        }
        // Wejścia już osiągalne z importu odliczamy przed startem.
        for r in &lista {
            if brakow[r.0 as usize] == 0 || brakow[r.0 as usize] == u32::MAX {
                continue;
            }
            let rec = self.recipe(*r);
            let brak = rec
                .inputs
                .iter()
                .filter(|(g, _)| !osiagalne[g.0 as usize])
                .count() as u32;
            brakow[r.0 as usize] = brak;
            if brak == 0 {
                kolejka.push_back(*r);
            }
        }

        // KROK 2.
        while let Some(r) = kolejka.pop_front() {
            let rec = self.recipe(r);
            let mut wyjscia: Vec<GoodId> = rec.outputs.iter().map(|(g, _)| *g).collect();
            wyjscia.sort_by_key(|g| g.0);
            for g in wyjscia {
                if osiagalne[g.0 as usize] {
                    continue;
                }
                osiagalne[g.0 as usize] = true;
                for r2 in &lista {
                    let licznik = brakow[r2.0 as usize];
                    if licznik == 0 || licznik == u32::MAX {
                        continue;
                    }
                    if self.recipe(*r2).inputs.iter().any(|(x, _)| *x == g) {
                        brakow[r2.0 as usize] = licznik - 1;
                        if brakow[r2.0 as usize] == 0 {
                            kolejka.push_back(*r2);
                        }
                    }
                }
            }
        }
        osiagalne
    }
}

fn pliki_ron(dir: &Path) -> Result<Vec<std::path::PathBuf>, CatalogError> {
    let mut v: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    v.sort();
    Ok(v)
}

/// Ładowanie z domyślnego `data/` dla epoki startowej.
pub fn load_default(epoch_key: &str) -> Result<Catalog, CatalogError> {
    Catalog::load(
        &crate::assets::data_path("goods"),
        &crate::assets::data_path("recipes"),
        &crate::assets::data_path("chains/needs.ron"),
        epoch_key,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn katalog() -> Catalog {
        load_default("contemporary").expect("katalog z data/")
    }

    /// Walidator z dok. 00 §5 — każdy towar w katalogu ma źródło.
    #[test]
    fn katalog_danych_domyka_sie() {
        let c = katalog();
        assert!(c.goods.len() >= 55, "katalog ma {} towarów", c.goods.len());
        assert!(c.recipes.len() >= 40, "katalog ma {} receptur", c.recipes.len());
        c.validate_reachability().expect("graf produktów");
    }

    /// Test negatywny wymagany kryterium WP14: towar osiągalny **wyłącznie** z zapasu
    /// startowego musi oblać walidację.
    ///
    /// Zapas startowy nie jest źródłem (9.1/15) i w tym kodzie nie jest nim nawet
    /// z nazwy — walidator zna trzy punkty wejścia: wydobycie, rolnictwo i import.
    /// Towar, który nie ma żadnego z nich, a jest w katalogu, to dokładnie ten przypadek:
    /// magazyn pozwoliłby mu raz wystartować i nigdy się nie odtworzyć.
    /// Beton nadaje się na dowód, bo jako jedyny półprodukt nie ma ceny importowej —
    /// betonu się nie sprowadza z drugiego końca kraju, on wiąże po drodze.
    #[test]
    fn zapas_startowy_nie_jest_zrodlem() {
        let mut c = katalog();
        assert!(
            c.good_id("concrete")
                .is_some_and(|g| !c.good(g).has_external_price()),
            "test stoi na tym, że betonu nie da się sprowadzić"
        );
        let wezel = c.recipe_id("concrete_plant").expect("receptura betoniarni");
        c.recipes.remove(wezel.0 as usize);
        let err = c
            .validate_reachability()
            .expect_err("beton bez wytwórni i bez importu musi oblać walidację");
        match err {
            CatalogError::Unreachable(v) => {
                assert!(
                    v.iter().any(|k| k == "concrete"),
                    "brakujący beton nie zgłoszony: {v:?}"
                );
            }
            other => panic!("zły błąd: {other}"),
        }
    }

    /// Bilans masy jest liczony, a nie deklarowany — `yield` nie jest polem (9.1/16).
    #[test]
    fn wytwarzanie_domyka_bilans_masy() {
        let c = katalog();
        for r in &c.recipes {
            if r.source() != RecipeSource::Manufacturing {
                continue;
            }
            let we: i64 = r.inputs.iter().map(|(_, m)| *m).sum();
            let wy: i64 = r.outputs.iter().map(|(_, m)| *m).sum();
            assert_eq!(we, wy + r.spec.process_loss_g, "receptura {}", r.key());
        }
    }

    #[test]
    fn identyfikatory_sa_alfabetyczne_i_stabilne() {
        let c = katalog();
        assert!(c.goods.windows(2).all(|w| w[0].key < w[1].key));
        assert!(c.recipes.windows(2).all(|w| w[0].spec.key < w[1].spec.key));
        let id = c.good_id("coal").expect("węgiel w katalogu");
        assert_eq!(c.good(id).key, "coal");
    }
}
