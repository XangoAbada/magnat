//! Ładowanie katalogu z `data/` i walidacja, która musi się wydarzyć **przed** użyciem.
//!
//! Błąd danych jest błędem, nie ostrzeżeniem: katalog z dziurą daje miasto, które nie
//! domyka Etapu 7, a to widać dopiero na końcu generacji.

use magnat_core::{Energy, GoodId, Mass, NeedCategoryId, RecipeId, Volume, Q};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::good::{Good, GoodSpec, NeedCategory, Substitute, SubstituteSpec};
use super::recipe::{
    CostAllocation, OutputKind, Recipe, RecipeInput, RecipeOutput, RecipeSource, RecipeSpec,
};
use super::Catalog;

pub const GOODS_SCHEMA_VERSION: u32 = 2;
pub const RECIPES_SCHEMA_VERSION: u32 = 2;
pub const NEEDS_SCHEMA_VERSION: u32 = 1;
pub const CATEGORIES_SCHEMA_VERSION: u32 = 1;

// ── Pliki danych ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GoodsFile {
    schema_version: u32,
    goods: Vec<GoodSpec>,
}

#[derive(Deserialize)]
struct RecipesFile {
    schema_version: u32,
    recipes: Vec<RecipeSpec>,
}

#[derive(Deserialize)]
struct CategoriesFile {
    schema_version: u32,
    categories: Vec<NeedCategory>,
}

/// Koszyk potrzeb epoki — gramy na mieszkańca na dobę.
///
/// Mieszka w `data/chains/`, a **nie** w `Good`: model potrzeb należy do M3/M5 i nie ma
/// powodu, żeby M6 dostał go w swoim schemacie towaru. Kiedy M5 zbuduje prawdziwy
/// koszyk, ten plik znika, a nie zmienia właściciela.
#[derive(Deserialize)]
struct NeedsFile {
    schema_version: u32,
    /// Klucz epoki z `data/epochs/` → lista (towar, g/osobę/dobę).
    baskets: Vec<(String, Vec<(String, i64)>)>,
}

#[derive(Debug)]
pub enum CatalogError {
    Io(std::io::Error),
    Ron {
        file: String,
        msg: String,
    },
    Schema {
        file: String,
        found: u32,
        want: u32,
    },
    DuplicateGood(String),
    DuplicateRecipe(String),
    DuplicateCategory(String),
    UnknownGood {
        recipe: String,
        good: String,
    },
    UnknownCategory {
        good: String,
        category: String,
    },
    MassBalance {
        recipe: String,
        inputs: i64,
        outputs: i64,
        loss: i64,
    },
    BadDuration(String),
    /// Postać sypka bez gęstości albo sztukowa bez masy i objętości sztuki (reguła 4).
    BadUnits(String),
    NoOutputs(String),
    /// Wagi modelu jakości sumują się powyżej 100 — zakład o średniej obsadzie
    /// wypuszczałby towar lepszy od wszystkiego, co do niego weszło.
    BadQualityWeights {
        recipe: String,
        sum: u32,
    },
    /// `ByMarketValue` bez ceny odniesienia dla wyjścia — alokacja kosztu jest wtedy
    /// niezdefiniowana, a nie „domyślna".
    NoMarketValue {
        recipe: String,
        good: String,
    },
    /// Towar, do którego nie prowadzi żadna ścieżka z wydobycia, rolnictwa ani importu.
    Unreachable(Vec<String>),
    /// Towar, którego nikt nie zużywa — zapchałby magazyny (reguła 2).
    NoConsumer(Vec<String>),
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
            CatalogError::DuplicateCategory(k) => {
                write!(f, "kategoria `{k}` zdefiniowana dwa razy")
            }
            CatalogError::UnknownGood { recipe, good } => write!(
                f,
                "receptura `{recipe}` odwołuje się do nieznanego towaru `{good}`"
            ),
            CatalogError::UnknownCategory { good, category } => write!(
                f,
                "towar `{good}` deklaruje nieznaną kategorię potrzeby `{category}`"
            ),
            CatalogError::MassBalance {
                recipe,
                inputs,
                outputs,
                loss,
            } => write!(
                f,
                "receptura `{recipe}`: bilans masy {inputs} g wejść wobec {outputs} g wyjść + {loss} g strat"
            ),
            CatalogError::BadDuration(k) => write!(f, "receptura `{k}`: zerowy czas szarży"),
            CatalogError::BadUnits(k) => write!(
                f,
                "towar `{k}`: postać sypka wymaga gęstości, sztukowa masy i objętości sztuki"
            ),
            CatalogError::NoOutputs(k) => write!(f, "receptura `{k}` nie produkuje niczego"),
            CatalogError::BadQualityWeights { recipe, sum } => write!(
                f,
                "receptura `{recipe}`: wagi jakości sumują się do {sum}, dozwolone najwyżej 100"
            ),
            CatalogError::NoMarketValue { recipe, good } => write!(
                f,
                "receptura `{recipe}` dzieli koszt wg wartości rynkowej, a `{good}` nie ma ceny odniesienia"
            ),
            CatalogError::Unreachable(v) => write!(
                f,
                "{} towarów bez źródła (ani wydobycie, ani rolnictwo, ani import): {}",
                v.len(),
                v.join(", ")
            ),
            CatalogError::NoConsumer(v) => write!(
                f,
                "{} towarów bez odbiorcy (ani receptura, ani koszyk, ani utylizacja): {}",
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

fn czytaj<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, CatalogError> {
    let txt = std::fs::read_to_string(path)?;
    ron::from_str(&txt).map_err(|e| CatalogError::Ron {
        file: path.display().to_string(),
        msg: e.to_string(),
    })
}

fn pliki_ron(dir: &Path) -> Result<Vec<PathBuf>, CatalogError> {
    let mut v: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    v.sort();
    Ok(v)
}

fn wersja(file: &Path, found: u32, want: u32) -> Result<(), CatalogError> {
    if found == want {
        Ok(())
    } else {
        Err(CatalogError::Schema {
            file: file.display().to_string(),
            found,
            want,
        })
    }
}

impl Catalog {
    /// Ładuje katalog i **waliduje** go w całości.
    ///
    /// `categories` to `data/needs/categories.ron`, `needs` to koszyk epoki
    /// z `data/chains/needs.ron`.
    pub fn load(
        goods_dir: &Path,
        recipes_dir: &Path,
        categories: &Path,
        needs: &Path,
        epoch_key: &str,
    ) -> Result<Catalog, CatalogError> {
        let categories = wczytaj_kategorie(categories)?;
        let kat_idx: BTreeMap<&str, NeedCategoryId> = categories
            .iter()
            .enumerate()
            .map(|(i, c)| (c.key.as_str(), NeedCategoryId(i as u16)))
            .collect();

        let specs = wczytaj_towary(goods_dir)?;
        let idx: BTreeMap<&str, GoodId> = specs
            .iter()
            .enumerate()
            .map(|(i, g)| (g.key.as_str(), GoodId(i as u16)))
            .collect();
        let goods = zloz_towary(&specs, &idx, &kat_idx)?;

        let recipes = wczytaj_receptury(recipes_dir, &idx, &goods)?;
        let basket = wczytaj_koszyk(needs, &idx, epoch_key)?;

        // Indeks kategorii i tablica klas maszyn są **pochodnymi**, nie danymi, więc
        // składa je jedno miejsce — `from_parts`. Powielenie tego tutaj dałoby katalog
        // z `data/` i katalog testowy o różnych indeksach, czyli najgorszy możliwy rodzaj
        // różnicy: taki, którego nie widać, dopóki test nie przejdzie, a produkcja nie.
        let cat = Catalog::from_parts(goods, recipes, categories, basket);
        cat.validate_reachability()?;
        cat.validate_consumers()?;
        Ok(cat)
    }
}

fn wczytaj_kategorie(path: &Path) -> Result<Vec<NeedCategory>, CatalogError> {
    let f: CategoriesFile = czytaj(path)?;
    wersja(path, f.schema_version, CATEGORIES_SCHEMA_VERSION)?;
    let mut v = f.categories;
    v.sort_by(|a, b| a.key.cmp(&b.key));
    for w in v.windows(2) {
        if w[0].key == w[1].key {
            return Err(CatalogError::DuplicateCategory(w[0].key.clone()));
        }
    }
    Ok(v)
}

fn wczytaj_towary(dir: &Path) -> Result<Vec<GoodSpec>, CatalogError> {
    let mut specs: Vec<GoodSpec> = Vec::new();
    for path in pliki_ron(dir)? {
        let f: GoodsFile = czytaj(&path)?;
        wersja(&path, f.schema_version, GOODS_SCHEMA_VERSION)?;
        specs.extend(f.goods);
    }
    specs.sort_by(|a, b| a.key.cmp(&b.key));
    for w in specs.windows(2) {
        if w[0].key == w[1].key {
            return Err(CatalogError::DuplicateGood(w[0].key.clone()));
        }
    }
    Ok(specs)
}

fn rozwiaz_towar(
    idx: &BTreeMap<&str, GoodId>,
    kto: &str,
    klucz: &str,
) -> Result<GoodId, CatalogError> {
    idx.get(klucz)
        .copied()
        .ok_or_else(|| CatalogError::UnknownGood {
            recipe: kto.to_string(),
            good: klucz.to_string(),
        })
}

fn rozwiaz_substytuty(
    v: &[SubstituteSpec],
    idx: &BTreeMap<&str, GoodId>,
    kto: &str,
) -> Result<Vec<Substitute>, CatalogError> {
    v.iter()
        .map(|s| {
            Ok(Substitute {
                good: rozwiaz_towar(idx, kto, &s.good)?,
                mass_ratio_permille: s.mass_ratio_permille,
                quality_penalty: s.quality_penalty,
            })
        })
        .collect()
}

fn zloz_towary(
    specs: &[GoodSpec],
    idx: &BTreeMap<&str, GoodId>,
    kat: &BTreeMap<&str, NeedCategoryId>,
) -> Result<Vec<Good>, CatalogError> {
    let mut goods = Vec::with_capacity(specs.len());
    for (i, s) in specs.iter().enumerate() {
        // Reguła 4 — spójność jednostek. Postać rozstrzyga, które pola muszą być
        // wypełnione, i to jest jedyne miejsce, w którym ta zależność jest sprawdzana.
        let ok = if s.form.is_bulk() {
            s.density_g_per_l > 0
        } else {
            s.unit_mass_g > 0 && s.unit_volume_ml > 0
        };
        if !ok {
            return Err(CatalogError::BadUnits(s.key.clone()));
        }
        let category =
            kat.get(s.category.as_str())
                .copied()
                .ok_or_else(|| CatalogError::UnknownCategory {
                    good: s.key.clone(),
                    category: s.category.clone(),
                })?;
        goods.push(Good {
            key: s.key.clone().into_boxed_str(),
            id: GoodId(i as u16),
            category,
            form: s.form,
            density_g_per_l: s.density_g_per_l,
            unit_mass: Mass(s.unit_mass_g),
            unit_volume: Volume(s.unit_volume_ml),
            shelf_life_minutes: s.shelf_life_minutes,
            storage: s.storage,
            hazard: s.hazard,
            has_quality: s.has_quality,
            substitutes: rozwiaz_substytuty(&s.substitutes, idx, &s.key)?,
            external_base_price: s.external_base_price,
            import_via: s.import_via.clone(),
            disposal_cost: s.disposal_cost,
        });
    }
    Ok(goods)
}

fn wczytaj_receptury(
    dir: &Path,
    idx: &BTreeMap<&str, GoodId>,
    goods: &[Good],
) -> Result<Vec<Recipe>, CatalogError> {
    let mut specs: Vec<RecipeSpec> = Vec::new();
    for path in pliki_ron(dir)? {
        let f: RecipesFile = czytaj(&path)?;
        wersja(&path, f.schema_version, RECIPES_SCHEMA_VERSION)?;
        specs.extend(f.recipes);
    }
    specs.sort_by(|a, b| a.key.cmp(&b.key));
    for w in specs.windows(2) {
        if w[0].key == w[1].key {
            return Err(CatalogError::DuplicateRecipe(w[0].key.clone()));
        }
    }

    let mut recipes = Vec::with_capacity(specs.len());
    for (i, spec) in specs.iter().enumerate() {
        recipes.push(zloz_recepture(i, spec, idx, goods)?);
    }
    Ok(recipes)
}

fn zloz_recepture(
    i: usize,
    spec: &RecipeSpec,
    idx: &BTreeMap<&str, GoodId>,
    goods: &[Good],
) -> Result<Recipe, CatalogError> {
    if spec.duration_minutes == 0 {
        return Err(CatalogError::BadDuration(spec.key.clone()));
    }
    if spec.outputs.is_empty() {
        return Err(CatalogError::NoOutputs(spec.key.clone()));
    }
    let suma_wag = spec.quality.weight_sum();
    if suma_wag > 100 {
        return Err(CatalogError::BadQualityWeights {
            recipe: spec.key.clone(),
            sum: suma_wag,
        });
    }

    let mut inputs = Vec::with_capacity(spec.inputs.len());
    for w in &spec.inputs {
        inputs.push(RecipeInput {
            good: rozwiaz_towar(idx, &spec.key, &w.good)?,
            mass: Mass(w.mass_g),
            min_quality: Q::new(w.min_quality),
            substitutes: rozwiaz_substytuty(&w.substitutes, idx, &spec.key)?,
            critical: w.critical,
        });
    }
    let mut outputs = Vec::with_capacity(spec.outputs.len());
    for o in &spec.outputs {
        outputs.push(RecipeOutput {
            good: rozwiaz_towar(idx, &spec.key, &o.good)?,
            mass: Mass(o.mass_g),
            kind: o.kind,
        });
    }

    // Bilans masy obowiązuje **wyłącznie wytwarzanie**. Kopalnia i pole nie mają wejścia
    // towarowego — ich masa pochodzi ze złoża i z gleby, a te nie są towarem. Gdyby
    // reguła obejmowała też je, każdy punkt wejścia łańcucha byłby formalnie nielegalny.
    //
    // Woda z licznika (`water_ml`) liczy się po stronie wejść: jest medium, nie towarem,
    // ale ma masę i ta masa wychodzi z pieca jako chleb (`D9`, wpisane w M6b).
    if spec.source == RecipeSource::Manufacturing {
        let we: i64 = inputs.iter().map(|i| i.mass.0).sum::<i64>() + spec.water_ml;
        let wy: i64 = outputs.iter().map(|o| o.mass.0).sum();
        if we != wy + spec.process_loss_g {
            return Err(CatalogError::MassBalance {
                recipe: spec.key.clone(),
                inputs: we,
                outputs: wy,
                loss: spec.process_loss_g,
            });
        }
    }

    // Alokacja wg wartości rynkowej potrzebuje ceny odniesienia dla każdego wyjścia,
    // które koszt dostaje. `Waste` i `SelfConsumed` kosztu nie dostają, więc ceny nie
    // potrzebują. Źródłem ceny jest statyczne `external_base_price` z katalogu (D3).
    if spec.cost_allocation == CostAllocation::ByMarketValue {
        for o in &outputs {
            let dostaje_koszt = matches!(o.kind, OutputKind::Main | OutputKind::ByProduct);
            if dostaje_koszt && goods[o.good.0 as usize].external_base_price.is_none() {
                return Err(CatalogError::NoMarketValue {
                    recipe: spec.key.clone(),
                    good: goods[o.good.0 as usize].key.to_string(),
                });
            }
        }
    }

    // Nominalny wsad: jawny z danych, inaczej suma wejść. Receptura **bez wejść**
    // (wydobycie: kopalnia, szyb, ujęcie wody) brałaby z tego zero i nie dałaby się
    // uruchomić w ogóle — jej „wsadem" jest to, co wychodzi ze złoża, więc skalę
    // wyznaczają wyjścia. Bez tego cała gałąź `Extraction` katalogu stała bezczynnie
    // od M6b, a masy z niczego nie tworzyła tylko dlatego, że nie ruszała.
    let batch_mass = if spec.batch_mass_g > 0 {
        Mass(spec.batch_mass_g)
    } else {
        let we: i64 = inputs.iter().map(|i| i.mass.0).sum();
        if we > 0 {
            Mass(we)
        } else {
            Mass(outputs.iter().map(|o| o.mass.0).sum())
        }
    };

    Ok(Recipe {
        key: spec.key.clone().into_boxed_str(),
        id: RecipeId(i as u16),
        source: spec.source,
        inputs,
        outputs,
        batch_mass,
        duration_minutes: spec.duration_minutes,
        process_loss: Mass(spec.process_loss_g),
        loss_kind: spec.loss_kind,
        energy: Energy(spec.energy_wh),
        water: Volume(spec.water_ml),
        labour: spec
            .labour
            .iter()
            .map(|(k, m)| (k.clone().into_boxed_str(), *m))
            .collect(),
        machine_class: spec.machine_class.clone().into_boxed_str(),
        setup: spec.setup,
        emissions: spec.emissions,
        quality: spec.quality,
        cost_allocation: spec.cost_allocation,
    })
}

fn wczytaj_koszyk(
    needs: &Path,
    idx: &BTreeMap<&str, GoodId>,
    epoch_key: &str,
) -> Result<Vec<(GoodId, i64)>, CatalogError> {
    let nf: NeedsFile = czytaj(needs)?;
    wersja(needs, nf.schema_version, NEEDS_SCHEMA_VERSION)?;
    let lista = nf
        .baskets
        .iter()
        .find(|(k, _)| k == epoch_key)
        .map(|(_, v)| v)
        .ok_or_else(|| CatalogError::UnknownEpoch(epoch_key.to_string()))?;
    let mut basket: Vec<(GoodId, i64)> = Vec::with_capacity(lista.len());
    for (k, g_na_dobe) in lista {
        basket.push((rozwiaz_towar(idx, "needs", k)?, *g_na_dobe));
    }
    basket.sort_by_key(|(g, _)| g.0);
    Ok(basket)
}

/// Ładowanie z domyślnego `data/` dla epoki startowej.
pub fn load_default(epoch_key: &str) -> Result<Catalog, CatalogError> {
    Catalog::load(
        &magnat_core::data_path("goods"),
        &magnat_core::data_path("recipes"),
        &magnat_core::data_path("needs/categories.ron"),
        &magnat_core::data_path("chains/needs.ron"),
        epoch_key,
    )
}
