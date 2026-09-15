//! Katalog archetypów zakładów, szablonów łańcuchów i członów nazw firm — wszystko,
//! co Etap 7 czyta z `data/` (M2 §5.8).
//!
//! Wydzielone z `city/sites.rs` w R-WP5 bez zmiany zachowania.

use super::*;

pub const ARCHETYPE_SCHEMA_VERSION: u32 = 1;
pub const CHAIN_SCHEMA_VERSION: u32 = 1;
pub const FIRM_NAMES_SCHEMA_VERSION: u32 = 1;

/// Skala bazowa archetypu. `capacity_scale` 1000 = jedna receptura chodząca bez przerwy.
pub const SCALE_BASE: u16 = 1000;
/// Widełki skalowania z KROKU 5 (§5.8).
///
/// **Korekta I-2 wobec planu:** dolna granica to 0,02 skali bazowej, a nie 0,4. Próg 0,4
/// z §5.8 zakładał, że liczba zakładów jest już z grubsza właściwa, a korekta ma tylko
/// domknąć zaokrąglenia. Przy liczbie zakładów wyprowadzonej z popytu ta granica wiąże
/// wyłącznie dla **ostatniego, niepodzielnego** zakładu — a miasteczko czterdziestotysięczne
/// naprawdę ma jedną małą piekarnię, a nie linię przemysłową chodzącą na 40 %.
pub const SCALE_MIN: u16 = 20;
pub const SCALE_MAX: u16 = 2500;
/// Dopuszczalny przedział `supply/demand` — warunek akceptacji domknięcia (test T11).
pub const RATIO_MIN: f64 = 0.85;
pub const RATIO_MAX: f64 = 1.30;

/// Indeks w katalogu archetypów (`data/buildings/`), stabilny w obrębie wersji danych.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SiteArchetypeId(pub u16);

/// Archetyp zakładu z `data/buildings/*.ron` (PRD §7.2).
///
/// Granica wobec `data/recipes/` jest rozstrzygnięciem 9.1/13: receptura niesie
/// pracochłonność szarży i klasę maszyny, archetyp — strefę, minimalną działkę,
/// obsadę etatową i mnożnik widełek. Nie wyprowadzamy jednego z drugiego.
#[derive(Clone, Debug, Deserialize)]
pub struct ArchetypeSpec {
    pub key: String,
    pub sector: SectorId,
    pub zones: Vec<String>,
    pub min_parcel_m2: u32,
    /// Powierzchnia lokalu na stanowisko. Zero = zostaje przelicznik roli z `data/jobs/`.
    #[serde(default)]
    pub m2_per_workplace: u16,
    #[serde(default = "jeden")]
    pub wage_mult: f32,
    #[serde(default)]
    pub recipes: Vec<String>,
    /// Normatyw ludnościowy: jedna placówka na tylu mieszkańców.
    #[serde(default)]
    pub per_pop: Option<u32>,
    /// Waga wypełniacza; 0 = archetyp stawiany wyłącznie z normatywu albo z bilansu.
    #[serde(default)]
    pub weight: u16,
    /// Epoki startowe, w których archetyp występuje. Puste = wszystkie.
    #[serde(default)]
    pub epochs: Vec<String>,
    /// Złoże wymagane pod parcelą. `None` = zakład nie potrzebuje złoża.
    #[serde(default)]
    pub needs_deposit: Option<ResourceKind>,
    /// Gramatyka bryły dla stref, których Etap 6 nie zabudowuje (zieleń, wydobycie).
    #[serde(default)]
    pub grammar: Option<String>,
    /// Dobowe zużycie materiałów eksploatacyjnych przy skali bazowej, w gramach.
    #[serde(default)]
    pub consumes: Vec<(String, i64)>,
    /// Rodzaj miejsca, w którym mieszkaniec zaspokaja potrzebę (M3d, Etap 8 krok 0).
    ///
    /// `None` = zakład jest wyłącznie miejscem pracy. Odwzorowanie stoi w danych,
    /// a nie w `match` po kluczu archetypu, bo rozstrzyga o tym, **do czego** miasto
    /// służy mieszkańcom — a to zmienia się z epoką i z profilem, nie z kodem.
    /// `PlaceKind` mieszka w `core` (K-8), więc `data/buildings/` nie wprowadza
    /// własnego słownika.
    #[serde(default)]
    pub place_kind: Option<magnat_core::PlaceKind>,
}

fn jeden() -> f32 {
    1.0
}

/// Archetyp z kluczami rozwiązanymi na identyfikatory.
#[derive(Clone, Debug)]
pub struct Archetype {
    pub spec: ArchetypeSpec,
    pub zones: Vec<ZoneKind>,
    pub recipes: Vec<RecipeId>,
    pub consumes: Vec<(GoodId, i64)>,
    pub grammar: Option<GrammarId>,
}

impl Archetype {
    #[must_use]
    pub fn key(&self) -> &str {
        &self.spec.key
    }

    #[must_use]
    pub fn sector(&self) -> SectorId {
        self.spec.sector
    }

    pub(super) fn pasuje_do_epoki(&self, epoch_key: &str) -> bool {
        self.spec.epochs.is_empty() || self.spec.epochs.iter().any(|e| e == epoch_key)
    }
}

#[derive(Deserialize)]
struct ArchetypeFile {
    schema_version: u32,
    archetypes: Vec<ArchetypeSpec>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChainTemplate {
    pub key: String,
    pub archetypes: Vec<String>,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub epochs: Vec<String>,
    pub weight: u16,
}

#[derive(Deserialize)]
struct ChainFile {
    schema_version: u32,
    chains: Vec<ChainTemplate>,
}

#[derive(Deserialize)]
struct FirmNamesFile {
    schema_version: u32,
    by_sector: Vec<(SectorId, Vec<String>)>,
}

/// Człony nazw firm per sektor. Nazwy są **proceduralne, nie lokalizowane** (CLAUDE.md).
#[derive(Clone, Debug)]
pub struct FirmNames {
    by_sector: Vec<(SectorId, Vec<String>)>,
}

impl FirmNames {
    pub(super) fn czlon(&self, s: SectorId, i: u32) -> &str {
        match self.by_sector.iter().find(|(k, _)| *k == s) {
            Some((_, v)) if !v.is_empty() => &v[i as usize % v.len()],
            _ => "Przedsiębiorstwo",
        }
    }
}

#[derive(Debug)]
pub enum SiteDataError {
    Io(std::io::Error),
    Ron { file: String, msg: String },
    Schema { file: String, found: u32, want: u32 },
    UnknownZone { archetype: String, key: String },
    UnknownRecipe { archetype: String, key: String },
    UnknownGood { archetype: String, key: String },
    UnknownGrammar { archetype: String, key: String },
    UnknownArchetype { chain: String, key: String },
    Duplicate(String),
}

impl std::fmt::Display for SiteDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SiteDataError::Io(e) => write!(f, "archetypy: {e}"),
            SiteDataError::Ron { file, msg } => write!(f, "{file}: {msg}"),
            SiteDataError::Schema { file, found, want } => {
                write!(f, "{file}: schema_version {found}, oczekiwano {want}")
            }
            SiteDataError::UnknownZone { archetype, key } => {
                write!(f, "archetyp {archetype}: nieznana strefa `{key}`")
            }
            SiteDataError::UnknownRecipe { archetype, key } => {
                write!(f, "archetyp {archetype}: nieznana receptura `{key}`")
            }
            SiteDataError::UnknownGood { archetype, key } => {
                write!(f, "archetyp {archetype}: nieznany towar `{key}`")
            }
            SiteDataError::UnknownGrammar { archetype, key } => {
                write!(f, "archetyp {archetype}: nieznana gramatyka `{key}`")
            }
            SiteDataError::UnknownArchetype { chain, key } => {
                write!(f, "łańcuch {chain}: nieznany archetyp `{key}`")
            }
            SiteDataError::Duplicate(k) => write!(f, "archetyp `{k}` zdefiniowany dwa razy"),
        }
    }
}

impl std::error::Error for SiteDataError {}

impl From<std::io::Error> for SiteDataError {
    fn from(e: std::io::Error) -> SiteDataError {
        SiteDataError::Io(e)
    }
}

/// Katalog archetypów, szablonów łańcuchów i członów nazw — wszystko, co Etap 7 czyta
/// z `data/`, w jednym miejscu.
#[derive(Clone, Debug)]
pub struct SiteCatalog {
    pub archetypes: Vec<Archetype>,
    pub chains: Vec<ChainTemplate>,
    pub names: FirmNames,
}

impl SiteCatalog {
    pub fn load(
        buildings_dir: &Path,
        chains: &Path,
        names: &Path,
        cat: &Catalog,
        grammars: &GrammarSet,
    ) -> Result<SiteCatalog, SiteDataError> {
        let mut specs: Vec<ArchetypeSpec> = Vec::new();
        let mut pliki: Vec<_> = std::fs::read_dir(buildings_dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "ron"))
            .collect();
        pliki.sort();
        for path in pliki {
            let name = path.display().to_string();
            let txt = std::fs::read_to_string(&path)?;
            let f: ArchetypeFile = ron::from_str(&txt).map_err(|e| SiteDataError::Ron {
                file: name.clone(),
                msg: e.to_string(),
            })?;
            if f.schema_version != ARCHETYPE_SCHEMA_VERSION {
                return Err(SiteDataError::Schema {
                    file: name,
                    found: f.schema_version,
                    want: ARCHETYPE_SCHEMA_VERSION,
                });
            }
            specs.extend(f.archetypes);
        }
        // Alfabetycznie po kluczu — ta sama zasada co dla towarów (dok. 00 §5).
        specs.sort_by(|a, b| a.key.cmp(&b.key));
        for w in specs.windows(2) {
            if w[0].key == w[1].key {
                return Err(SiteDataError::Duplicate(w[0].key.clone()));
            }
        }

        let mut archetypes = Vec::with_capacity(specs.len());
        for spec in specs {
            let mut zones = Vec::with_capacity(spec.zones.len());
            for z in &spec.zones {
                zones.push(crate::city::grammar::zone_z_klucza(z).ok_or_else(|| {
                    SiteDataError::UnknownZone {
                        archetype: spec.key.clone(),
                        key: z.clone(),
                    }
                })?);
            }
            let mut recipes = Vec::with_capacity(spec.recipes.len());
            for r in &spec.recipes {
                recipes.push(
                    cat.recipe_id(r)
                        .ok_or_else(|| SiteDataError::UnknownRecipe {
                            archetype: spec.key.clone(),
                            key: r.clone(),
                        })?,
                );
            }
            let mut consumes = Vec::with_capacity(spec.consumes.len());
            for (g, m) in &spec.consumes {
                let id = cat.good_id(g).ok_or_else(|| SiteDataError::UnknownGood {
                    archetype: spec.key.clone(),
                    key: g.clone(),
                })?;
                consumes.push((id, *m));
            }
            consumes.sort_by_key(|(g, _)| g.0);
            let grammar = match &spec.grammar {
                None => None,
                Some(k) => {
                    Some(
                        grammars
                            .id_of(k)
                            .ok_or_else(|| SiteDataError::UnknownGrammar {
                                archetype: spec.key.clone(),
                                key: k.clone(),
                            })?,
                    )
                }
            };
            archetypes.push(Archetype {
                spec,
                zones,
                recipes,
                consumes,
                grammar,
            });
        }

        let txt = std::fs::read_to_string(chains)?;
        let cf: ChainFile = ron::from_str(&txt).map_err(|e| SiteDataError::Ron {
            file: chains.display().to_string(),
            msg: e.to_string(),
        })?;
        if cf.schema_version != CHAIN_SCHEMA_VERSION {
            return Err(SiteDataError::Schema {
                file: chains.display().to_string(),
                found: cf.schema_version,
                want: CHAIN_SCHEMA_VERSION,
            });
        }
        for ch in &cf.chains {
            for a in &ch.archetypes {
                if !archetypes.iter().any(|x| x.key() == a) {
                    return Err(SiteDataError::UnknownArchetype {
                        chain: ch.key.clone(),
                        key: a.clone(),
                    });
                }
            }
        }

        let txt = std::fs::read_to_string(names)?;
        let nf: FirmNamesFile = ron::from_str(&txt).map_err(|e| SiteDataError::Ron {
            file: names.display().to_string(),
            msg: e.to_string(),
        })?;
        if nf.schema_version != FIRM_NAMES_SCHEMA_VERSION {
            return Err(SiteDataError::Schema {
                file: names.display().to_string(),
                found: nf.schema_version,
                want: FIRM_NAMES_SCHEMA_VERSION,
            });
        }

        Ok(SiteCatalog {
            archetypes,
            chains: cf.chains,
            names: FirmNames {
                by_sector: nf.by_sector,
            },
        })
    }

    pub fn load_default(
        cat: &Catalog,
        grammars: &GrammarSet,
    ) -> Result<SiteCatalog, SiteDataError> {
        SiteCatalog::load(
            &crate::assets::data_path("buildings"),
            &crate::assets::data_path("chains/templates.ron"),
            &crate::assets::data_path("names/firms_pl.ron"),
            cat,
            grammars,
        )
    }

    #[must_use]
    pub fn get(&self, id: SiteArchetypeId) -> &Archetype {
        &self.archetypes[id.0 as usize]
    }

    #[must_use]
    pub fn id_of(&self, key: &str) -> Option<SiteArchetypeId> {
        self.archetypes
            .binary_search_by(|a| a.spec.key.as_str().cmp(key))
            .ok()
            .map(|i| SiteArchetypeId(i as u16))
    }
}
