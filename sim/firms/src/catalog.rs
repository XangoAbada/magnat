//! Katalog typów zakładów — `data/site_types/*.ron` (M7a WP2, M7 §5.2, PRD §7.2).
//!
//! **Dodanie nowego typu zakładu nie dotyka kodu.** Kod zna [`SiteTypeCategory`] i pola
//! poniżej; czym jest młyn, a czym supermarket, mówią dane.
//!
//! ## Granica wobec `data/buildings/` — i dlaczego nie jest to ten sam plik
//!
//! Archetyp z `data/buildings/` (M2) odpowiada na pytanie **„gdzie wolno to postawić
//! i jak duże to jest"**: strefa, minimalna działka, powierzchnia na stanowisko,
//! mnożnik widełek, receptury, epoki, wymagane złoże. Typ zakładu z `data/site_types/`
//! (M7) odpowiada na **„czym to się zarządza"**: struktura obsady z podziałem na zawody,
//! stanowisko kierownicze, nakład na uruchomienie i koszt stały miesiąca.
//!
//! Oba pliki mówią o tym samym zakładzie **tym samym kluczem tekstowym**, więc rozjazd
//! byłby cichy — i dlatego jest zakazany wprost: [`SiteTypeCatalog::load`] przyjmuje listę
//! kluczy archetypów i **odrzuca katalog**, w którym typ zakładu nie ma archetypu.
//! To jedyny realny koszt trzymania tego osobno i jest spłacony walidatorem, nie regulaminem.
//!
//! **Czego tu nie ma, bo ma już właściciela:** media i liczniki (M6, `UtilityMeter`),
//! rampa (M6, `Dock`), emisje (M6, `PlantSite::emissions`), pojemność półki (M5, `Shelf`),
//! linie produkcyjne (wyprowadzane z receptur archetypu, M6b), strefy i epoki
//! (`data/buildings/`). Powtórzenie któregokolwiek z nich byłoby drugą prawdą.

use magnat_core::{JobRoleId, Money};
use magnat_supply::Catalog;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::hr::roles::RoleTable;

pub const SITE_TYPES_SCHEMA_VERSION: u32 = 1;

/// Indeks typu zakładu w katalogu. Stabilny w obrębie wersji danych, jak `GoodId` —
/// kolejność jest alfabetyczna po kluczu, więc dopisanie typu w środku pliku
/// niczego nie przestawia.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SiteTypeId(pub u16);

impl magnat_core::hash::HashState for SiteTypeId {
    #[inline]
    fn hash_state(&self, h: &mut magnat_core::hash::StateHasher) {
        h.write_u16(self.0);
    }
}

/// Branża (PRD §7.2). Kolejność wariantów jest kontraktem: `as_index()` indeksuje
/// zestawienia balansatora i histogram struktury gospodarki.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Deserialize)]
pub enum SiteTypeCategory {
    Extraction,
    Processing,
    Manufacturing,
    Logistics,
    Retail,
    ConsumerServices,
    BusinessServices,
    Finance,
    Media,
    RealEstate,
}

impl SiteTypeCategory {
    pub const ALL: [SiteTypeCategory; 10] = [
        SiteTypeCategory::Extraction,
        SiteTypeCategory::Processing,
        SiteTypeCategory::Manufacturing,
        SiteTypeCategory::Logistics,
        SiteTypeCategory::Retail,
        SiteTypeCategory::ConsumerServices,
        SiteTypeCategory::BusinessServices,
        SiteTypeCategory::Finance,
        SiteTypeCategory::Media,
        SiteTypeCategory::RealEstate,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Nazwa wariantu — klucz tekstu w `data/locale/`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            SiteTypeCategory::Extraction => "Extraction",
            SiteTypeCategory::Processing => "Processing",
            SiteTypeCategory::Manufacturing => "Manufacturing",
            SiteTypeCategory::Logistics => "Logistics",
            SiteTypeCategory::Retail => "Retail",
            SiteTypeCategory::ConsumerServices => "ConsumerServices",
            SiteTypeCategory::BusinessServices => "BusinessServices",
            SiteTypeCategory::Finance => "Finance",
            SiteTypeCategory::Media => "Media",
            SiteTypeCategory::RealEstate => "RealEstate",
        }
    }
}

/// Wiersz obsady w danych: ilu ludzi którego zawodu potrzebuje ten zakład.
///
/// **Liczby są całkowite** — bo liczba stanowisk wchodzi do stanu trwałego,
/// a tam floata nie ma (dokument 00 §2).
///
/// Mianownik to **10 000 m², a nie 100 ani 1 000**, i to jest wybór wymuszony przez
/// rozpiętość danych, nie estetykę. Zakłady w `data/buildings/` mają od 22 m² na
/// stanowisko (biuro) do 1 920 m² (las, pole) — prawie stukrotna rozpiętość. Przy
/// mianowniku 1 000 gospodarstwo rolne wymagałoby „0,5 stanowiska", czyli w `u16`
/// wychodziłoby 1 — i cała gałąź rolna miasta byłaby **dwukrotnie** przeobsadzona,
/// razem z dwukrotnie zawyżonym kosztem pracy. Przy 10 000 biuro ma 4 500, pole 5,
/// i obie liczby są dokładne.
#[derive(Clone, Debug, Deserialize)]
pub struct StaffingSpec {
    pub role: String,
    /// Stanowiska proporcjonalne do powierzchni zakładu, na 10 000 m².
    #[serde(default)]
    pub per_10000m2: u16,
    /// Stanowiska niezależne od powierzchni — kierownik, księgowa, dozór.
    #[serde(default)]
    pub per_site: u16,
    /// Dolna granica: zakład czynny musi mieć tylu, choćby był mikroskopijny.
    #[serde(default)]
    pub min: u16,
    /// Stanowisko kierownicze. Każdy typ zakładu musi mieć co najmniej jedno —
    /// pilnuje tego walidator, bo zakład bez kierownika nie ma komu delegować (M7c).
    #[serde(default)]
    pub managerial: bool,
}

/// Wiersz obsady po związaniu roli z katalogiem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Staffing {
    pub role: JobRoleId,
    pub per_10000m2: u16,
    pub per_site: u16,
    pub min: u16,
    pub managerial: bool,
}

impl Staffing {
    /// Liczba stanowisk tej roli w zakładzie o danej powierzchni użytkowej.
    ///
    /// Dzielenie całkowite z zaokrągleniem do najbliższego: zakład o 1 400 m²
    /// i stawce 90/10 000 dostaje 13 kasjerów, a nie 12 — inaczej każdy zakład
    /// gubiłby ułamek stanowiska w tę samą stronę.
    #[must_use]
    pub fn slots(&self, floor_m2: u32) -> u16 {
        let z_powierzchni = (u64::from(floor_m2) * u64::from(self.per_10000m2) + 5_000) / 10_000;
        let razem = z_powierzchni.saturating_add(u64::from(self.per_site));
        u16::try_from(razem.max(u64::from(self.min))).unwrap_or(u16::MAX)
    }
}

/// Nakład na uruchomienie zakładu (PRD §7.2). Konsumuje go M7f przy zakładaniu firmy
/// i M7e przy decyzji o otwarciu zakładu; w M7a jest wyceną tego, co już stoi.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct Capex {
    /// Budowa — grosze.
    pub build: i64,
    /// Wyposażenie na 100 m² — grosze.
    pub equip_per_100m2: i64,
}

/// Koszt stały miesiąca, bez płac i bez mediów licznikowych.
///
/// Płace liczy [`crate::hr`], media liczy M6 po licznikach. Tu siedzi to, czego
/// nie widzi ani jedno, ani drugie: czynsz albo amortyzacja budynku i administracja.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct FixedCostSpec {
    /// Grosze za m² miesięcznie.
    pub rent_per_m2: i64,
    /// Ryczałt administracyjny zakładu — grosze miesięcznie.
    pub admin: i64,
}

#[derive(Debug, Deserialize)]
struct SiteTypeSpec {
    key: String,
    category: SiteTypeCategory,
    staffing: Vec<StaffingSpec>,
    capex: Capex,
    fixed_cost: FixedCostSpec,
}

#[derive(Deserialize)]
struct SiteTypeFile {
    schema_version: u32,
    site_types: Vec<SiteTypeSpec>,
}

/// Typ zakładu po związaniu z katalogiem ról.
#[derive(Clone, Debug)]
pub struct SiteType {
    pub key: String,
    pub category: SiteTypeCategory,
    pub staffing: Vec<Staffing>,
    pub capex: Capex,
    pub fixed_cost: FixedCostSpec,
}

impl SiteType {
    /// Miesięczny koszt stały zakładu o danej powierzchni.
    #[must_use]
    pub fn fixed_cost_month(&self, floor_m2: u32) -> Money {
        Money(
            self.fixed_cost
                .rent_per_m2
                .saturating_mul(i64::from(floor_m2))
                .saturating_add(self.fixed_cost.admin),
        )
    }

    /// Nakład na uruchomienie zakładu o danej powierzchni.
    #[must_use]
    pub fn capex(&self, floor_m2: u32) -> Money {
        Money(
            self.capex.build.saturating_add(
                self.capex
                    .equip_per_100m2
                    .saturating_mul(i64::from(floor_m2))
                    / 100,
            ),
        )
    }
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
    /// Dwa rekordy o tym samym kluczu — indeks byłby niejednoznaczny.
    DuplicateKey(String),
    /// Rola z obsady nie istnieje w `data/jobs/roles.ron`.
    UnknownRole {
        site_type: String,
        role: String,
    },
    /// Typ zakładu bez stanowiska kierowniczego — nie ma komu delegować (M7c).
    NoManager(String),
    /// Typ zakładu bez archetypu w `data/buildings/`: zakład, którego nie da się
    /// postawić w mieście. Cichy rozjazd dwóch katalogów, dokładnie ten,
    /// przed którym broni się nagłówek tego modułu.
    NoArchetype(String),
    /// Receptura archetypu wskazuje towar spoza katalogu M6.
    UnknownRecipe {
        site_type: String,
        recipe: String,
    },
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::Io(e) => write!(f, "błąd wejścia-wyjścia: {e}"),
            CatalogError::Ron { file, msg } => write!(f, "{file}: {msg}"),
            CatalogError::Schema { file, found, want } => {
                write!(f, "{file}: schema_version {found}, oczekiwano {want}")
            }
            CatalogError::DuplicateKey(k) => {
                write!(f, "typ zakładu „{k}” jest zdefiniowany dwa razy")
            }
            CatalogError::UnknownRole { site_type, role } => {
                write!(
                    f,
                    "typ zakładu „{site_type}” obsadza nieznaną rolę „{role}”"
                )
            }
            CatalogError::NoManager(k) => write!(
                f,
                "typ zakładu „{k}” nie ma ani jednego stanowiska kierowniczego"
            ),
            CatalogError::NoArchetype(k) => write!(
                f,
                "typ zakładu „{k}” nie ma archetypu w data/buildings/ — nie da się go postawić"
            ),
            CatalogError::UnknownRecipe { site_type, recipe } => write!(
                f,
                "typ zakładu „{site_type}” wskazuje nieznaną recepturę „{recipe}”"
            ),
        }
    }
}

impl std::error::Error for CatalogError {}

impl From<std::io::Error> for CatalogError {
    fn from(e: std::io::Error) -> CatalogError {
        CatalogError::Io(e)
    }
}

/// Katalog typów zakładów.
#[derive(Clone, Debug, Default)]
pub struct SiteTypeCatalog {
    types: Vec<SiteType>,
    index: BTreeMap<String, SiteTypeId>,
}

impl SiteTypeCatalog {
    #[must_use]
    pub fn get(&self, id: SiteTypeId) -> &SiteType {
        &self.types[id.0 as usize]
    }

    #[must_use]
    pub fn id(&self, key: &str) -> Option<SiteTypeId> {
        self.index.get(key).copied()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (SiteTypeId, &SiteType)> {
        self.types
            .iter()
            .enumerate()
            .map(|(i, t)| (SiteTypeId(i as u16), t))
    }

    /// Ładuje katalog i **waliduje go w całości**, tak jak katalog towarów M6:
    /// błąd danych jest błędem, nie ostrzeżeniem.
    ///
    /// `archetypes` to klucze archetypów z `data/buildings/` wraz z ich recepturami —
    /// podaje je wywołujący, bo to on ma `SiteCatalog` M2; `sim/firms` nie zależy
    /// od `sim/world` i zależeć nie będzie. Mapa pusta wyłącza obie reguły krzyżowe
    /// (testy jednostkowe samego katalogu).
    pub fn load(
        dir: &Path,
        roles: &RoleTable,
        goods: &Catalog,
        archetypes: &BTreeMap<String, Vec<String>>,
    ) -> Result<SiteTypeCatalog, CatalogError> {
        let mut specs: Vec<SiteTypeSpec> = Vec::new();
        for path in pliki_ron(dir)? {
            let name = path.display().to_string();
            let txt = std::fs::read_to_string(&path)?;
            let f: SiteTypeFile = ron::from_str(&txt).map_err(|e| CatalogError::Ron {
                file: name.clone(),
                msg: e.to_string(),
            })?;
            if f.schema_version != SITE_TYPES_SCHEMA_VERSION {
                return Err(CatalogError::Schema {
                    file: name,
                    found: f.schema_version,
                    want: SITE_TYPES_SCHEMA_VERSION,
                });
            }
            specs.extend(f.site_types);
        }
        // Kolejność alfabetyczna po kluczu, tak jak w katalogu towarów (00 §5):
        // `SiteTypeId` ma nie zależeć od tego, w którym pliku rekord wylądował.
        specs.sort_by(|a, b| a.key.cmp(&b.key));

        let mut types = Vec::with_capacity(specs.len());
        let mut index = BTreeMap::new();
        for spec in specs {
            if index.contains_key(&spec.key) {
                return Err(CatalogError::DuplicateKey(spec.key));
            }
            if !archetypes.is_empty() {
                let recepty = archetypes
                    .get(&spec.key)
                    .ok_or_else(|| CatalogError::NoArchetype(spec.key.clone()))?;
                // Kryterium WP2: każda receptura zakładu wskazuje istniejący towar.
                // Sprawdzamy tu, a nie w M2, bo dopiero tu widać **jeden** zakład
                // opisany przez oba katalogi naraz.
                for r in recepty {
                    if goods.recipe_id(r).is_none() {
                        return Err(CatalogError::UnknownRecipe {
                            site_type: spec.key.clone(),
                            recipe: r.clone(),
                        });
                    }
                }
            }
            let mut staffing = Vec::with_capacity(spec.staffing.len());
            for s in &spec.staffing {
                let role = roles.id(&s.role).ok_or_else(|| CatalogError::UnknownRole {
                    site_type: spec.key.clone(),
                    role: s.role.clone(),
                })?;
                staffing.push(Staffing {
                    role,
                    per_10000m2: s.per_10000m2,
                    per_site: s.per_site,
                    min: s.min,
                    managerial: s.managerial,
                });
            }
            if !staffing.iter().any(|s| s.managerial) {
                return Err(CatalogError::NoManager(spec.key));
            }
            index.insert(spec.key.clone(), SiteTypeId(types.len() as u16));
            types.push(SiteType {
                key: spec.key,
                category: spec.category,
                staffing,
                capex: spec.capex,
                fixed_cost: spec.fixed_cost,
            });
        }
        Ok(SiteTypeCatalog { types, index })
    }

    /// Ładowanie z domyślnego `data/site_types/`.
    pub fn load_default(
        roles: &RoleTable,
        goods: &Catalog,
        archetypes: &BTreeMap<String, Vec<String>>,
    ) -> Result<SiteTypeCatalog, CatalogError> {
        SiteTypeCatalog::load(
            &magnat_core::assets::data_path("site_types"),
            roles,
            goods,
            archetypes,
        )
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsada_zaokragla_do_najblizszego() {
        let s = Staffing {
            role: JobRoleId(0),
            per_10000m2: 90,
            per_site: 0,
            min: 2,
            managerial: false,
        };
        // 1400 × 90 / 10 000 = 12,6 → 13, nie 12.
        assert_eq!(s.slots(1_400), 13);
        // Zakład mikroskopijny dostaje minimum, nie zero.
        assert_eq!(s.slots(50), 2);
    }

    #[test]
    fn stanowiska_niezalezne_od_powierzchni_dochodza() {
        let s = Staffing {
            role: JobRoleId(1),
            per_10000m2: 0,
            per_site: 1,
            min: 0,
            managerial: true,
        };
        assert_eq!(s.slots(100_000), 1);
    }
}
