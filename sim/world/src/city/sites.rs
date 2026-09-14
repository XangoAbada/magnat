//! Etap 7 — gospodarka bazowa: firmy jako obiekty danych (M2 §5.8, WP13 i WP14).
//!
//! W M2 firma **nie ma zachowań**: nie produkuje, nie zatrudnia, nie wycenia. Jest
//! rekordem, który mówi „w tym budynku będzie huta o skali 1,4, z 320 stanowiskami
//! tych ról". Zachowania dokładają M6 (produkcja) i M7 (pełna `Firm`).
//!
//! ## Co się tu dzieje, w kolejności
//!
//! 1. **Normatywy ludnościowe** (`per_pop`) — handel, usługi publiczne i zieleń.
//!    Rozmieszczenie zachłanne po pokryciu ludności, nie losowe.
//! 2. **Wypełniacze stref nieprodukcyjnych** — handel, biura, urzędy, logistyka.
//!    Muszą stanąć przed bilansem, bo ich `consumes` jest częścią popytu (def(3) z §5.8).
//! 3. **Zakłady produkcyjne** — liczba wyprowadzona z popytu, rozmieszczenie
//!    po klastrach przemysłowych wg szablonów łańcuchów z `data/chains/`.
//! 4. **Wypełniacze stref przemysłowych** — reszta działek.
//! 5. **Domknięcie łańcuchów** (`supply_closure_check`) i bilans przepustowości.
//!
//! ## Korekta wobec §5.8: KROK 5 liczy się konstrukcyjnie, nie korekcyjnie
//!
//! Plan opisywał KROK 5 jako korektę po fakcie: policz `ratio`, przeskaluj `capacity_scale`
//! w zakresie 0,4–2,5, w razie czego dostaw zakład. To działa tylko wtedy, kiedy liczba
//! zakładów jest z grubsza właściwa **zanim** zacznie się korekta — a przy obsadzie
//! „każda działka rolna to gospodarstwo" nie jest: metropolia ma ok. 2 400 działek
//! rolnych wobec zapotrzebowania rzędu stu gospodarstw, więc nadwyżka jest kilkunastokrotna,
//! a dolny limit skali (0,4) nie ma jej jak zjeść. §5.8 nigdzie nie przewiduje **usuwania**
//! zakładu, bo milcząco zakłada, że obsada była od początku świadoma popytu.
//!
//! Robimy więc to, co plan zakłada, jawnie: liczbę zakładów każdego archetypu wyprowadzamy
//! z popytu (propagacja wstecz po hipergrafie receptur), a działki, których popyt nie
//! potrzebuje, dostają **wypełniacz** — warsztat, skład, punkt usługowy. Korekta z KROKU 5
//! zostaje i pilnuje zaokrągleń. Zapisane jako korekta I-2 w dokumencie podfazy.

use super::blocks::BlockSet;
use super::build::{self, BuildInput, BuildingSet, UnitIdx, UnitOccupant, Workplace};
use super::catalog::Catalog;
use super::districts::DistrictSet;
use super::gates::GateKind;
use super::grammar::{GrammarId, GrammarSet};
use super::parcels::{ParcelOwner, ParcelSet, ParcelStatus};
use super::poly;
use super::road::PolyArena;
use super::zoning::{ZoneKind, ZoneResult};
use super::CityPlan;
use magnat_core::{rng, Entity, FirmId, GoodId, RecipeId, ResourceKind, SiteId, StreamId, Tick};
use magnat_spatial::Vec2;
use magnat_voxel::EditQueue;
use serde::Deserialize;
use smallvec::SmallVec;
use std::num::NonZeroU32;
use std::ops::Range;
use std::path::Path;

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

/// Sektor gospodarki. Grupuje archetypy do wypełniania stref i do nazw firm;
/// pełna klasyfikacja PKD to nie jest zadanie M2.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Deserialize)]
pub enum SectorId {
    Industry,
    Logistics,
    Retail,
    Services,
    Office,
    Public,
    Agriculture,
    Extraction,
    Green,
}

impl SectorId {
    pub const ALL: [SectorId; 9] = [
        SectorId::Industry,
        SectorId::Logistics,
        SectorId::Retail,
        SectorId::Services,
        SectorId::Office,
        SectorId::Public,
        SectorId::Agriculture,
        SectorId::Extraction,
        SectorId::Green,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            SectorId::Industry => "industry",
            SectorId::Logistics => "logistics",
            SectorId::Retail => "retail",
            SectorId::Services => "services",
            SectorId::Office => "office",
            SectorId::Public => "public",
            SectorId::Agriculture => "agriculture",
            SectorId::Extraction => "extraction",
            SectorId::Green => "green",
        }
    }

    /// Czy zakład należy do miasta (decyzja 9.2/5). Budżet i polityka usługi — M8.
    #[must_use]
    pub const fn is_municipal(self) -> bool {
        matches!(self, SectorId::Public | SectorId::Green)
    }
}

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

    fn pasuje_do_epoki(&self, epoch_key: &str) -> bool {
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
    fn czlon(&self, s: SectorId, i: u32) -> &str {
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
                zones.push(super::grammar::zone_z_klucza(z).ok_or_else(|| {
                    SiteDataError::UnknownZone {
                        archetype: spec.key.clone(),
                        key: z.clone(),
                    }
                })?);
            }
            let mut recipes = Vec::with_capacity(spec.recipes.len());
            for r in &spec.recipes {
                recipes.push(cat.recipe_id(r).ok_or_else(|| {
                    SiteDataError::UnknownRecipe {
                        archetype: spec.key.clone(),
                        key: r.clone(),
                    }
                })?);
            }
            let mut consumes = Vec::with_capacity(spec.consumes.len());
            for (g, m) in &spec.consumes {
                let id = cat
                    .good_id(g)
                    .ok_or_else(|| SiteDataError::UnknownGood {
                        archetype: spec.key.clone(),
                        key: g.clone(),
                    })?;
                consumes.push((id, *m));
            }
            consumes.sort_by_key(|(g, _)| g.0);
            let grammar = match &spec.grammar {
                None => None,
                Some(k) => Some(grammars.id_of(k).ok_or_else(|| {
                    SiteDataError::UnknownGrammar {
                        archetype: spec.key.clone(),
                        key: k.clone(),
                    }
                })?),
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

    pub fn load_default(cat: &Catalog, grammars: &GrammarSet) -> Result<SiteCatalog, SiteDataError> {
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

// ── Wyjście Etapu 7 (kontrakt M2 §6) ─────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct FirmSeed {
    /// Nazwa proceduralna — **nie jest** lokalizacją UI (CLAUDE.md).
    pub name: String,
    pub sector: SectorId,
    pub sites: SmallVec<[SiteId; 4]>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SiteSeed {
    pub firm: FirmId,
    pub building: magnat_core::BuildingId,
    pub units: Range<u32>,
    pub archetype: SiteArchetypeId,
    /// Receptury z archetypu; puste dla handlu, usług i administracji.
    pub recipes: Vec<RecipeId>,
    /// 1000 = skala bazowa archetypu.
    pub capacity_scale: u16,
    pub workplaces: Range<u32>,
    /// Parcela zakładu — do karty inspekcji i do testów spójności.
    pub parcel: magnat_core::ParcelId,
}

#[must_use]
pub fn site_id(i: u32) -> SiteId {
    SiteId(Entity::new(i, NonZeroU32::new(1).expect("1 != 0")))
}

#[must_use]
pub fn firm_id(i: u32) -> FirmId {
    FirmId(Entity::new(i, NonZeroU32::new(1).expect("1 != 0")))
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct SiteReport {
    pub firms: u32,
    pub sites: u32,
    /// Zakłady per sektor, w kolejności `SectorId::ALL`.
    pub by_sector: [u32; 9],
    /// Budynki dostawione przez Etap 7 (zieleń, wydobycie, naprawa domknięcia).
    pub buildings_added: u32,
    /// Działki, dla których Etap 7 chciał budynek, ale nic się nie zmieściło.
    pub buildings_failed: u32,
    /// Zabudowane parcele niemieszkalne bez zakładu — kryterium WP13 mówi „zero".
    pub parcels_without_site: u32,
    /// Normatywy ludnościowe: ile placówek oczekiwano i ile udało się postawić.
    pub norm_target: u32,
    pub norm_placed: u32,
    /// Zakłady postawione z szablonu łańcucha (a nie pojedynczo).
    pub from_chain: u32,
    /// Archetypy, których bilans potrzebował, a dla których zabrakło działki — i ile.
    /// Bez tej listy „podaż/popyt 0,00" nie mówi, czy zabrakło popytu, czy miejsca.
    pub unplaced: Vec<(String, u32)>,
    /// Zakłady postawione po **rozluźnieniu wymagania strefy** (KROK 4c po korekcie I-3).
    pub relaxed_zone: u32,
}

/// Wynik domknięcia łańcuchów produktowych (§5.8, WP14).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct ClosureReport {
    /// Towary, do których nie prowadzi żadna ścieżka — warunek akceptacji: pusta lista.
    pub missing: Vec<String>,
    /// Towary sprowadzane z zewnątrz i dobowy wolumen w kilogramach.
    pub imported: Vec<(String, i64)>,
    /// Zakłady dostawione przez naprawę (KROK 4b).
    pub added_sites: u32,
    /// Archetypy, którym trzeba było rozluźnić wymaganie strefy (KROK 4c po korekcie I-3).
    pub relaxed_zone: u32,
    /// `supply/demand` dla każdego towaru z popytem — rosnąco po `GoodId`.
    pub ratios: Vec<(String, f32)>,
    /// Najgorszy stosunek i towar, którego dotyczy.
    pub worst: (String, f32),
    /// Nadwyżki produktów ubocznych — patrz `wiodace_wyjscia` (korekta I-4). To nie jest
    /// błąd bilansu, tylko towar, który z miasta wyjeżdża; wypisany, żeby nie znikał.
    pub byproduct_surplus: Vec<(String, f32)>,
    /// Pozycje, przy których generacja się nie domknęła (KROK 4d).
    pub errors: Vec<String>,
}

impl ClosureReport {
    /// Warunek akceptacji z §5.8 i test T11.
    ///
    /// Liczony **tą samą regułą co `errors`**: nadwyżka produktu ubocznego nie jest
    /// błędem bilansu (korekta I-4), więc nie może wywracać akceptacji. Dwie reguły
    /// w dwóch miejscach dawały raport bez ani jednego błędu i domknięcie odrzucone.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.missing.is_empty() && self.errors.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct SiteSet {
    pub firms: Vec<FirmSeed>,
    pub sites: Vec<SiteSeed>,
    /// Zakład stojący w budynku, po indeksie budynku. `None` = budynek bez zakładu
    /// (mieszkalny albo pusty).
    pub by_building: Vec<Option<u32>>,
    pub report: SiteReport,
    pub closure: ClosureReport,
}

impl SiteSet {
    #[must_use]
    pub fn site_of_building(&self, b: magnat_core::BuildingId) -> Option<&SiteSeed> {
        self.by_building
            .get(b.0.index() as usize)
            .and_then(|x| *x)
            .map(|i| &self.sites[i as usize])
    }
}

// ── Etap 7 ───────────────────────────────────────────────────────────────────────────

/// Decyzja obsady dla jednej parceli, zanim cokolwiek powstanie.
#[derive(Clone, Copy)]
struct Przydzial {
    parcel: u32,
    archetype: SiteArchetypeId,
    /// Zakłady jednego szablonu w jednym klastrze tworzą jedną firmę.
    firma_grupa: u32,
    capacity_scale: u16,
}

/// Kandydat na zakład: zabudowana (albo zabudowywalna) parcela w strefie niemieszkalnej.
struct Kand {
    idx: u32,
    zone: ZoneKind,
    area: u32,
    block: u32,
    pos: Vec2,
    built: bool,
    zajeta: bool,
}

/// Cała obsada Etapu 7 plus domknięcie łańcuchów.
///
/// Mutuje `parcels` (właściciel, status, budynek) i `buildings` (nowe bryły na zieleni
/// i w wydobyciu, przepięcie stanowisk pracy na zakłady — korekta F1).
#[allow(clippy::too_many_arguments)]
pub fn populate(
    bi: &BuildInput,
    cat: &Catalog,
    sc: &SiteCatalog,
    epoch_key: &str,
    geom: &mut PolyArena,
    parcels: &mut ParcelSet,
    buildings: &mut BuildingSet,
    q: &EditQueue,
) -> SiteSet {
    let plan = bi.plan;
    let districts = bi.districts;
    let mut kandydaci = zbierz_kandydatow(parcels, geom);
    let mut przydzialy: Vec<Przydzial> = Vec::new();
    let mut grupa = 0u32;
    let mut rep = SiteReport::default();

    let pop_pole = pole_ludnosci(plan, bi.blocks, parcels, buildings);

    // ── 1. Normatywy ludnościowe: handel, usługi publiczne, zieleń ──────────────────
    for (i, a) in sc.archetypes.iter().enumerate() {
        let Some(per_pop) = a.spec.per_pop else {
            continue;
        };
        if !a.pasuje_do_epoki(epoch_key) {
            continue;
        }
        let cel = (plan.target_pop / per_pop.max(1)).max(1);
        rep.norm_target += cel;
        let wybrane = rozmiesc_normatyw(&mut kandydaci, a, cel, &pop_pole, plan);
        rep.norm_placed += wybrane.len() as u32;
        for p in wybrane {
            przydzialy.push(Przydzial {
                parcel: p,
                archetype: SiteArchetypeId(i as u16),
                firma_grupa: {
                    grupa += 1;
                    grupa
                },
                capacity_scale: SCALE_BASE,
            });
        }
    }

    // ── 2. Wypełniacze stref nieprodukcyjnych ──────────────────────────────────────
    // Przed bilansem, bo ich `consumes` jest częścią popytu (def(3) z §5.8).
    const NIEPRODUKCYJNE: [ZoneKind; 4] = [
        ZoneKind::Commercial,
        ZoneKind::Office,
        ZoneKind::Institutional,
        ZoneKind::Logistics,
    ];
    wypelnij(
        plan,
        &mut kandydaci,
        sc,
        epoch_key,
        &NIEPRODUKCYJNE,
        &mut przydzialy,
        &mut grupa,
    );

    // ── 3. Zakłady produkcyjne: liczba z popytu, miejsce z klastrów ────────────────
    let popyt = popyt_bazowy(plan, cat, sc, &przydzialy);
    let potrzeby = zapotrzebowanie_zakladow(cat, sc, &popyt, epoch_key);
    posadz_produkcje(
        bi,
        sc,
        cat,
        &potrzeby,
        &mut kandydaci,
        &mut przydzialy,
        &mut grupa,
        &mut rep,
        epoch_key,
    );

    // ── 4. Wypełniacze stref przemysłowych i reszty ────────────────────────────────
    const RESZTA: [ZoneKind; 4] = [
        ZoneKind::IndustryLight,
        ZoneKind::IndustryHeavy,
        ZoneKind::Extraction,
        ZoneKind::Agriculture,
    ];
    wypelnij(
        plan,
        &mut kandydaci,
        sc,
        epoch_key,
        &RESZTA,
        &mut przydzialy,
        &mut grupa,
    );

    // ── 5. Materializacja: brakujące bryły, potem encje ────────────────────────────
    przydzialy.sort_by_key(|p| p.parcel);
    let zlecenia: Vec<(u32, GrammarId)> = przydzialy
        .iter()
        .filter(|p| parcels.parcels[p.parcel as usize].building.is_none())
        .filter_map(|p| sc.get(p.archetype).grammar.map(|g| (p.parcel, g)))
        .collect();
    let wynik = build::build_for_sites(bi, geom, parcels, buildings, q, &zlecenia);
    rep.buildings_added = wynik.iter().filter(|x| x.is_some()).count() as u32;
    rep.buildings_failed = wynik.iter().filter(|x| x.is_none()).count() as u32;

    // Przydział, któremu bryła się nie zmieściła, **przenosi się na działkę zabudowaną**,
    // zamiast zniknąć. Bez tego jedyne ujęcie wody w mieście turystycznym trafiało na
    // parcelę zieleni, gramatyka się na niej nie mieściła i całe miasto zostawało bez
    // wody — a raport mówił tylko „domknięcie nie przeszło", trzy kroki dalej.
    for p in &mut przydzialy {
        if parcels.parcels[p.parcel as usize].building.is_some() {
            continue;
        }
        let a = sc.get(p.archetype);
        let Some(i) = kandydaci.iter().position(|c| {
            !c.zajeta && c.built && niemieszkalna(c.zone) && c.area >= a.spec.min_parcel_m2
        }) else {
            continue;
        };
        kandydaci[i].zajeta = true;
        p.parcel = kandydaci[i].idx;
        rep.relaxed_zone += 1;
    }

    let mut set = utworz_zaklady(plan, sc, districts, parcels, buildings, &przydzialy, rep);

    // ── 6. Domknięcie łańcuchów i bilans przepustowości (WP14) ─────────────────────
    // Popyt liczony **jeszcze raz**, na komplecie przydziałów: wypełniacze z kroku 4
    // i zakłady z kroku 3 też coś zużywają, a ten z kroku 3 był policzony przed nimi.
    let popyt_koncowy = popyt_bazowy(plan, cat, sc, &przydzialy);
    set.closure = supply_closure_check(cat, &mut set.sites, &popyt_koncowy, bi.roads);

    // ── 7. Stanowiska pracy: przepięcie na zakłady i korekta liczby (korekta F1) ───
    rebind_workplaces(bi, sc, districts, parcels, buildings, &mut set, epoch_key);

    set.report.parcels_without_site = parcels
        .parcels
        .iter()
        .enumerate()
        .filter(|(_, p)| p.building.is_some() && niemieszkalna(p.zone))
        .filter(|(i, _)| {
            let b = parcels.parcels[*i].building.expect("sprawdzone wyżej");
            set.site_of_building(b).is_none()
        })
        .count() as u32;
    set
}

/// Strefy, w których stoi zakład. Mieszkaniówka odpada: sklep na parterze kamienicy jest
/// lokalem, nie zakładem z własnym budynkiem, i przypisanie mu `SiteSeed` obejmującego
/// całą kamienicę mówiłoby nieprawdę o tym, kto jest właścicielem mieszkań.
#[must_use]
pub const fn niemieszkalna(z: ZoneKind) -> bool {
    matches!(
        z,
        ZoneKind::Commercial
            | ZoneKind::Office
            | ZoneKind::IndustryLight
            | ZoneKind::IndustryHeavy
            | ZoneKind::Logistics
            | ZoneKind::Institutional
            | ZoneKind::Agriculture
            | ZoneKind::Green
            | ZoneKind::Extraction
    )
}

fn zbierz_kandydatow(parcels: &ParcelSet, geom: &PolyArena) -> Vec<Kand> {
    parcels
        .parcels
        .iter()
        .enumerate()
        .filter(|(_, p)| niemieszkalna(p.zone))
        .map(|(i, p)| Kand {
            idx: i as u32,
            zone: p.zone,
            area: p.area_m2,
            block: p.block.0,
            pos: poly::centroid(geom.get(p.poly)),
            built: p.building.is_some(),
            zajeta: false,
        })
        .collect()
}

/// Siatka ludności: mieszkania × 2,4, zsumowane po komórkach 256 m.
///
/// Potrzebna do rozmieszczenia normatywów „maksymalizującego pokrycie ludności" (§5.8).
/// Liczona z **mieszkań**, nie z gęstości strefy — po Etapie 6 jest z czego, a gęstość
/// strefy mówi o zamiarze planisty, nie o tym, ile mieszkań faktycznie stanęło.
struct PoleLudnosci {
    dim: usize,
    cell_m: f32,
    v: Vec<f32>,
}

const POP_CELL_M: f32 = 256.0;

impl PoleLudnosci {
    fn suma_w_promieniu(&self, p: Vec2, r_m: f32) -> f32 {
        let k = (r_m / self.cell_m).ceil() as i32;
        let (cx, cy) = (
            (p.x / self.cell_m) as i32,
            (p.y / self.cell_m) as i32,
        );
        let mut s = 0.0;
        for dy in -k..=k {
            for dx in -k..=k {
                if dx * dx + dy * dy > k * k {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x as usize >= self.dim || y as usize >= self.dim {
                    continue;
                }
                s += self.v[y as usize * self.dim + x as usize];
            }
        }
        s
    }
}

fn pole_ludnosci(
    plan: &CityPlan,
    _blocks: &BlockSet,
    parcels: &ParcelSet,
    buildings: &BuildingSet,
) -> PoleLudnosci {
    let dim = ((plan.map_size_m() as f32 / POP_CELL_M).ceil() as usize).max(1);
    let mut v = vec![0.0f32; dim * dim];
    for u in buildings.units.iter().filter(|u| u.kind.is_dwelling()) {
        let b = &buildings.buildings[u.building.0.index() as usize];
        let p = &parcels.parcels[b.parcel.0.index() as usize];
        let _ = p;
        let c = Vec2::new(
            (b.aabb.min.x + b.aabb.max.x) * 0.5,
            (b.aabb.min.y + b.aabb.max.y) * 0.5,
        );
        let (x, y) = ((c.x / POP_CELL_M) as i32, (c.y / POP_CELL_M) as i32);
        if x < 0 || y < 0 || x as usize >= dim || y as usize >= dim {
            continue;
        }
        v[y as usize * dim + x as usize] += 2.4;
    }
    PoleLudnosci {
        dim,
        cell_m: POP_CELL_M,
        v,
    }
}

/// Zachłanne rozmieszczenie normatywu: bierz działkę o największym pokryciu ludności,
/// potem tłum pokrycie w jej sąsiedztwie i powtarzaj. To jest k-center po ludności
/// z §5.8, wyrażony tak, żeby nie wymagał macierzy odległości 4 000 × 4 000.
fn rozmiesc_normatyw(
    kand: &mut [Kand],
    a: &Archetype,
    cel: u32,
    pop: &PoleLudnosci,
    plan: &CityPlan,
) -> Vec<u32> {
    let promien = (plan.urban_radius_m() / (cel as f32).sqrt()).clamp(150.0, 2500.0);
    let mut punkty: Vec<(usize, f32)> = kand
        .iter()
        .enumerate()
        .filter(|(_, k)| pasuje_dzialka(k, a))
        .map(|(i, k)| (i, pop.suma_w_promieniu(k.pos, promien)))
        .collect();
    let mut out = Vec::new();
    for _ in 0..cel {
        // Remisy po indeksie parceli — nigdy po kolejności iteracji (00 §3.2).
        let Some(&(best, _)) = punkty
            .iter()
            .filter(|(i, _)| !kand[*i].zajeta)
            .max_by(|a, b| {
                a.1.total_cmp(&b.1)
                    .then(kand[b.0].idx.cmp(&kand[a.0].idx))
            })
        else {
            break;
        };
        let p = kand[best].pos;
        kand[best].zajeta = true;
        out.push(kand[best].idx);
        for (i, s) in &mut punkty {
            if (kand[*i].pos - p).length() < promien {
                *s *= 0.2;
            }
        }
    }
    out.sort_unstable();
    out
}

fn pasuje_dzialka(k: &Kand, a: &Archetype) -> bool {
    !k.zajeta
        && a.zones.contains(&k.zone)
        && k.area >= a.spec.min_parcel_m2
        // Działka bez budynku nadaje się tylko wtedy, gdy archetyp umie go postawić.
        && (k.built || a.grammar.is_some())
}

/// Wypełniacze: każda pozostała zabudowana działka we wskazanych strefach dostaje
/// archetyp losowany wagą. Kryterium WP13 mówi „każda zabudowana parcela niemieszkalna
/// ma `SiteSeed`" — to jest to miejsce, w którym ta obietnica się spełnia.
fn wypelnij(
    plan: &CityPlan,
    kand: &mut [Kand],
    sc: &SiteCatalog,
    epoch_key: &str,
    strefy: &[ZoneKind],
    out: &mut Vec<Przydzial>,
    grupa: &mut u32,
) {
    for k in kand.iter_mut() {
        if k.zajeta || !strefy.contains(&k.zone) {
            continue;
        }
        let mut r = rng(plan.seed, StreamId::SitePlacement, k.idx, Tick(0));
        let kandydaci: Vec<usize> = sc
            .archetypes
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.spec.weight > 0 && a.pasuje_do_epoki(epoch_key) && pasuje_dzialka(k, a)
            })
            .map(|(j, _)| j)
            .collect();
        let suma: u32 = kandydaci
            .iter()
            .map(|j| u32::from(sc.archetypes[*j].spec.weight))
            .sum();
        if suma == 0 {
            // Działka mniejsza od najmniejszego archetypu strefy. Kryterium WP13 mówi
            // „każda zabudowana parcela niemieszkalna ma `SiteSeed`" bez wyjątku dla
            // małych, więc bierzemy najmniejszy pasujący archetyp i ignorujemy metraż —
            // zakład na 60 m² jest mniej nieprawdziwy niż budynek bez właściciela.
            let Some(j) = sc
                .archetypes
                .iter()
                .enumerate()
                .filter(|(_, a)| {
                    a.spec.weight > 0
                        && a.pasuje_do_epoki(epoch_key)
                        && a.zones.contains(&k.zone)
                        && (k.built || a.grammar.is_some())
                })
                .min_by_key(|(j, a)| (a.spec.min_parcel_m2, *j))
                .map(|(j, _)| j)
            else {
                continue;
            };
            k.zajeta = true;
            *grupa += 1;
            out.push(Przydzial {
                parcel: k.idx,
                archetype: SiteArchetypeId(j as u16),
                firma_grupa: *grupa,
                capacity_scale: SCALE_BASE,
            });
            continue;
        }
        let mut los = r.next_u32() % suma;
        for j in kandydaci {
            let w = u32::from(sc.archetypes[j].spec.weight);
            if los < w {
                k.zajeta = true;
                *grupa += 1;
                out.push(Przydzial {
                    parcel: k.idx,
                    archetype: SiteArchetypeId(j as u16),
                    firma_grupa: *grupa,
                    capacity_scale: SCALE_BASE,
                });
                break;
            }
            los -= w;
        }
    }
}

// ── Bilans popytu ────────────────────────────────────────────────────────────────────

/// Popyt niewynikający z produkcji: koszyk mieszkańców + materiały eksploatacyjne
/// archetypów już postawionych (def(3) z §5.8). Gramy na dobę.
fn popyt_bazowy(
    plan: &CityPlan,
    cat: &Catalog,
    sc: &SiteCatalog,
    przydzialy: &[Przydzial],
) -> Vec<i64> {
    let mut d = vec![0i64; cat.goods.len()];
    for (g, na_osobe) in &cat.basket {
        d[g.0 as usize] += na_osobe.saturating_mul(i64::from(plan.target_pop));
    }
    for p in przydzialy {
        for (g, m) in &sc.get(p.archetype).consumes {
            d[g.0 as usize] += *m;
        }
    }
    d
}

/// Ile zakładów każdego archetypu produkcyjnego trzeba, żeby pokryć popyt.
///
/// Propagacja wstecz po hipergrafie receptur: `required` startuje z popytu bazowego,
/// a każda runda dokłada wejścia potrzebne do jego pokrycia. Rund jest **dwanaście**,
/// nie „do zbieżności": łańcuch w katalogu M2 ma głębokość co najwyżej pięciu ogniw,
/// a stały budżet rund jest warunkiem determinizmu (ryzyko R3).
///
/// Receptura wielowyjściowa (rafineria → trzy paliwa) skalowana jest **raz**, po
/// najbardziej wymagającym wyjściu — inaczej ropa liczyłaby się trzykrotnie.
fn zapotrzebowanie_zakladow(
    cat: &Catalog,
    sc: &SiteCatalog,
    popyt: &[i64],
    epoch_key: &str,
) -> Vec<(SiteArchetypeId, RecipeId, f64)> {
    // Receptura krajowa dla towaru: pierwsza po `RecipeId`, której archetyp jest dostępny.
    let mut recepta_dla: Vec<Option<(RecipeId, SiteArchetypeId)>> = vec![None; cat.goods.len()];
    for (ai, a) in sc.archetypes.iter().enumerate() {
        if !a.pasuje_do_epoki(epoch_key) {
            continue;
        }
        for r in &a.recipes {
            for (g, _) in &cat.recipe(*r).outputs {
                let slot = &mut recepta_dla[g.0 as usize];
                if slot.is_none_or(|(stary, _)| r.0 < stary.0) {
                    *slot = Some((*r, SiteArchetypeId(ai as u16)));
                }
            }
        }
    }

    let mut required = popyt.to_vec();
    let mut skala: Vec<(RecipeId, SiteArchetypeId, f64)> = Vec::new();
    for _ in 0..12 {
        skala.clear();
        // Skala każdej używanej receptury — po najbardziej wymagającym wyjściu.
        for (gi, slot) in recepta_dla.iter().enumerate() {
            let Some((r, a)) = *slot else { continue };
            if required[gi] <= 0 {
                continue;
            }
            let y = cat.recipe(r).daily_yield(GoodId(gi as u16));
            if y <= 0 {
                continue;
            }
            let s = required[gi] as f64 / y as f64;
            match skala.iter_mut().find(|(x, _, _)| *x == r) {
                Some((_, _, v)) => *v = v.max(s),
                None => skala.push((r, a, s)),
            }
        }
        skala.sort_by_key(|(r, _, _)| r.0);
        let mut next = popyt.to_vec();
        for (r, _, s) in &skala {
            let rec = cat.recipe(*r);
            for (g, _) in &rec.inputs {
                let na_dobe = rec.daily_input(*g) as f64 * s;
                next[g.0 as usize] = next[g.0 as usize].saturating_add(na_dobe.round() as i64);
            }
        }
        required = next;
    }
    skala.iter().map(|(r, a, s)| (*a, *r, *s)).collect()
}

/// Klastry przemysłowe: spójne grupy kwartałów o strefie przemysłowej albo logistycznej.
/// Ta sama definicja co w `rail.rs` i `districts.rs` — bocznica, dzielnica i firma mają
/// mówić o tym samym klastrze, a nie o trzech różnych.
fn klastry(blocks: &BlockSet, zones: &ZoneResult, segments: usize) -> Vec<Vec<u32>> {
    let przemysl = |z: ZoneKind| {
        matches!(
            z,
            ZoneKind::IndustryHeavy | ZoneKind::IndustryLight | ZoneKind::Logistics
        )
    };
    let n = blocks.blocks.len();
    let (start, items) = super::blocks::block_adjacency(blocks, segments);
    let mut seen = vec![false; n];
    let mut out: Vec<Vec<u32>> = Vec::new();
    for s in 0..n {
        if seen[s] || !przemysl(zones.zone[s]) {
            continue;
        }
        let mut stos = vec![s];
        seen[s] = true;
        let mut grupa = Vec::new();
        while let Some(v) = stos.pop() {
            grupa.push(v as u32);
            for k in start[v]..start[v + 1] {
                let j = items[k as usize] as usize;
                if !seen[j] && przemysl(zones.zone[j]) {
                    seen[j] = true;
                    stos.push(j);
                }
            }
        }
        grupa.sort_unstable();
        out.push(grupa);
    }
    // Od największego: magistrala kolejowa powstaje tam samo (rail.rs), więc łańcuch
    // trafia do klastra, który faktycznie ma bocznicę.
    out.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));
    out
}

#[allow(clippy::too_many_arguments)]
fn posadz_produkcje(
    bi: &BuildInput,
    sc: &SiteCatalog,
    cat: &Catalog,
    potrzeby: &[(SiteArchetypeId, RecipeId, f64)],
    kand: &mut [Kand],
    out: &mut Vec<Przydzial>,
    grupa: &mut u32,
    rep: &mut SiteReport,
    epoch_key: &str,
) {
    let plan = bi.plan;
    // Ile zakładów każdego archetypu i z jaką skalą.
    let mut zostalo: Vec<(SiteArchetypeId, u32, u16)> = Vec::new();
    for (a, r, s) in potrzeby {
        if *s <= 0.0 {
            continue;
        }
        // Zakład jest niepodzielny, więc liczba zakładów to sufit skali, a `capacity_scale`
        // rozkłada resztę po równo. Poniżej `SCALE_MIN` zakład przestaje mieć sens:
        // jeśli wszystko, co produkuje, da się sprowadzić — miasto to sprowadza,
        // a jeśli nie (woda, beton) — stoi mimo wszystko, w najmniejszej skali.
        let n = s.ceil().clamp(1.0, 4096.0) as u32;
        let dokladna = s / f64::from(n) * f64::from(SCALE_BASE);
        if dokladna < f64::from(SCALE_MIN)
            && cat
                .recipe(*r)
                .outputs
                .iter()
                .all(|(g, _)| cat.good(*g).has_external_price())
        {
            continue;
        }
        let skala = (dokladna.round() as i64)
            .clamp(i64::from(SCALE_MIN), i64::from(SCALE_MAX)) as u16;
        zostalo.push((*a, n, skala));
    }
    // Najpierw zakłady produkujące to, czego **nie da się sprowadzić** (woda, beton).
    // Reszta może przegrać wyścig o działkę — miasto to wtedy kupi; wodociąg nie ma
    // takiej alternatywy, a przy zwykłej kolejności alfabetycznej `waterworks` trafiał
    // na koniec i zostawał bez miejsca w mieście o wąskim pasie przemysłowym.
    let musi_powstac = |a: SiteArchetypeId| {
        sc.get(a)
            .recipes
            .iter()
            .flat_map(|r| cat.recipe(*r).outputs.iter())
            .any(|(g, _)| !cat.good(*g).has_external_price())
    };
    zostalo.sort_by_key(|(a, _, _)| (!musi_powstac(*a), a.0));

    let profil = plan.profile.key();
    let klastry = klastry(bi.blocks, bi.zones, bi.roads.segments.len());

    // ── 3a. Po klastrach, wg szablonów łańcuchów ───────────────────────────────────
    for (ki, k) in klastry.iter().enumerate() {
        let szablony: Vec<&ChainTemplate> = sc
            .chains
            .iter()
            .filter(|c| c.profiles.is_empty() || c.profiles.iter().any(|p| p == profil))
            .filter(|c| c.epochs.is_empty() || c.epochs.iter().any(|e| e == epoch_key))
            .collect();
        let suma: u32 = szablony.iter().map(|c| u32::from(c.weight)).sum();
        if suma == 0 {
            break;
        }
        let mut r = rng(plan.seed, StreamId::FirmSeed, ki as u32, Tick(0));
        let mut los = r.next_u32() % suma;
        let mut wybrany = szablony[0];
        for c in &szablony {
            let w = u32::from(c.weight);
            if los < w {
                wybrany = c;
                break;
            }
            los -= w;
        }
        *grupa += 1;
        let moja_grupa = *grupa;
        let mut cos_stanelo = false;
        for klucz in &wybrany.archetypes {
            let Some(aid) = sc.id_of(klucz) else { continue };
            let Some(slot) = zostalo.iter().position(|(a, n, _)| *a == aid && *n > 0) else {
                continue;
            };
            let skala = zostalo[slot].2;
            let a = sc.get(aid);
            let Some(i) = znajdz(kand, a, &|c: &Kand| {
                k.contains(&c.block) && zloze_ok(bi, c, a)
            }) else {
                continue;
            };
            kand[i].zajeta = true;
            zostalo[slot].1 -= 1;
            cos_stanelo = true;
            rep.from_chain += 1;
            out.push(Przydzial {
                parcel: kand[i].idx,
                archetype: aid,
                firma_grupa: moja_grupa,
                capacity_scale: skala,
            });
        }
        if !cos_stanelo {
            *grupa -= 1;
        }
    }

    // ── 3b. Reszta: gdziekolwiek się mieści, każdy zakład własną firmą ─────────────
    #[allow(clippy::needless_range_loop, reason = "pętla mutuje `zostalo[slot].1` i jednocześnie czyta `kand`; iterator po `zostalo` zablokowałby drugie pożyczenie")]
    for slot in 0..zostalo.len() {
        let (aid, _, skala) = zostalo[slot];
        let a = sc.get(aid);
        let musi = musi_powstac(aid);
        while zostalo[slot].1 > 0 {
            // Zakład, który **musi** powstać, siada wyłącznie na działce już zabudowanej:
            // pusta działka daje szansę, że bryła się nie zmieści, a wtedy nie ma wody.
            let znaleziona = if musi {
                kand.iter()
                    .position(|c| c.built && pasuje_dzialka(c, a) && zloze_ok(bi, c, a))
            } else {
                znajdz(kand, a, &|c: &Kand| zloze_ok(bi, c, a))
            };
            let Some(i) = znaleziona else {
                break;
            };
            kand[i].zajeta = true;
            zostalo[slot].1 -= 1;
            *grupa += 1;
            out.push(Przydzial {
                parcel: kand[i].idx,
                archetype: aid,
                firma_grupa: *grupa,
                capacity_scale: skala,
            });
        }
        // ── 3c. Rozluźnienie strefy — zamiast przestrefowania kwartału (korekta I-3) ──
        // Dotyczy **wyłącznie** zakładów produkujących towar, którego nie da się
        // sprowadzić: wodociąg i betoniarnia muszą stanąć, bo importu wody nie ma.
        // Reszta może zostać nieposadzona — miasto to wtedy kupi.
        while musi && zostalo[slot].1 > 0 {
            let Some(i) = kand.iter().position(|c| {
                !c.zajeta
                    && c.built
                    && niemieszkalna(c.zone)
                    && c.area >= a.spec.min_parcel_m2
                    && zloze_ok(bi, c, a)
            }) else {
                break;
            };
            kand[i].zajeta = true;
            zostalo[slot].1 -= 1;
            rep.relaxed_zone += 1;
            *grupa += 1;
            out.push(Przydzial {
                parcel: kand[i].idx,
                archetype: aid,
                firma_grupa: *grupa,
                capacity_scale: skala,
            });
        }
        if zostalo[slot].1 > 0 {
            rep.unplaced.push((a.key().to_string(), zostalo[slot].1));
        }
    }
}

/// Pierwsza pasująca działka, **z pierwszeństwem dla już zabudowanych**.
///
/// Kolejność ma znaczenie praktyczne: na działce z budynkiem zakład powstanie na pewno,
/// a na pustej dopiero, jeśli bryła się zmieści. Bez tego pierwszeństwa jedyne ujęcie
/// wody w mieście trafiało na parcelę, na której gramatyka nie miała jak stanąć, i całe
/// miasto zostawało bez wody — objaw widoczny dopiero w bilansie, o trzy kroki dalej.
fn znajdz(kand: &[Kand], a: &Archetype, extra: &dyn Fn(&Kand) -> bool) -> Option<usize> {
    kand.iter()
        .position(|c| c.built && pasuje_dzialka(c, a) && extra(c))
        .or_else(|| kand.iter().position(|c| pasuje_dzialka(c, a) && extra(c)))
}

/// Czy pod działką leży złoże, którego archetyp wymaga (M1 `deposit_at`, K-13).
fn zloze_ok(bi: &BuildInput, k: &Kand, a: &Archetype) -> bool {
    let Some(want) = a.spec.needs_deposit else {
        return true;
    };
    match bi.terrain.deposit_at(k.pos.x as i32, k.pos.y as i32) {
        Some(id) => bi.terrain.deposit(id).resource == want,
        None => false,
    }
}

// ── Materializacja encji ─────────────────────────────────────────────────────────────

fn utworz_zaklady(
    plan: &CityPlan,
    sc: &SiteCatalog,
    districts: &DistrictSet,
    parcels: &mut ParcelSet,
    buildings: &BuildingSet,
    przydzialy: &[Przydzial],
    mut rep: SiteReport,
) -> SiteSet {
    let mut firms: Vec<FirmSeed> = Vec::new();
    let mut sites: Vec<SiteSeed> = Vec::new();
    let mut by_building = vec![None; buildings.buildings.len()];
    // Grupa → indeks firmy. Wektor par, nie mapa: iterujemy po nim w kolejności wstawiania.
    let mut grupy: Vec<(u32, u32)> = Vec::new();

    for p in przydzialy {
        let parcel = &parcels.parcels[p.parcel as usize];
        let Some(bid) = parcel.building else { continue };
        let a = sc.get(p.archetype);
        let fidx = match grupy.iter().find(|(g, _)| *g == p.firma_grupa) {
            Some((_, i)) => *i,
            None => {
                let i = firms.len() as u32;
                let dzielnica = districts
                    .districts
                    .get(parcel.district.0 as usize)
                    .map_or("Miasto", |d| d.name.as_str());
                let mut r = rng(plan.seed, StreamId::FirmSeed, 1_000_000 + i, Tick(0));
                let czlon = sc.names.czlon(a.sector(), r.next_u32());
                let mut name = format!("{czlon} „{dzielnica}”");
                // Ta sama nazwa dwa razy w mieście jest błędem widocznym natychmiast
                // (ryzyko R10 dotyczy dzielnic, ale firma z tym samym szyldem po drugiej
                // stronie ulicy wygląda tak samo źle).
                let mut n = 2;
                while firms.iter().any(|f| f.name == name) {
                    name = format!("{czlon} „{dzielnica}” {}", rzymska(n));
                    n += 1;
                }
                firms.push(FirmSeed {
                    name,
                    sector: a.sector(),
                    sites: SmallVec::new(),
                });
                grupy.push((p.firma_grupa, i));
                i
            }
        };
        let b = &buildings.buildings[bid.0.index() as usize];
        let sid = site_id(sites.len() as u32);
        firms[fidx as usize].sites.push(sid);
        by_building[bid.0.index() as usize] = Some(sites.len() as u32);
        sites.push(SiteSeed {
            firm: firm_id(fidx),
            building: bid,
            units: b.units.clone(),
            archetype: p.archetype,
            recipes: a.recipes.clone(),
            capacity_scale: p.capacity_scale,
            workplaces: 0..0,
            parcel: super::parcels::parcel_id(p.parcel),
        });
        rep.by_sector[a.sector() as usize] += 1;

        let parcel = &mut parcels.parcels[p.parcel as usize];
        parcel.status = ParcelStatus::Built;
        parcel.owner = if a.sector().is_municipal() {
            ParcelOwner::City
        } else {
            ParcelOwner::Firm(firm_id(fidx))
        };
    }

    rep.firms = firms.len() as u32;
    rep.sites = sites.len() as u32;
    SiteSet {
        firms,
        sites,
        by_building,
        report: rep,
        closure: ClosureReport::default(),
    }
}

/// Liczebnik porządkowy do odróżniania firm o tej samej nazwie. Do dziesiątej wystarczy
/// rzymski, dalej arabski — „Fabryka «Wola» XXIV" wygląda gorzej niż „… 24".
fn rzymska(n: u32) -> String {
    const R: [&str; 9] = ["II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    if (2..=10).contains(&n) {
        R[(n - 2) as usize].to_string()
    } else {
        n.to_string()
    }
}

// ── Stanowiska pracy (korekta F1) ────────────────────────────────────────────────────

/// Przepina stanowiska pracy na zakłady i poprawia ich liczbę tam, gdzie archetyp mówi
/// co innego niż przelicznik roli.
///
/// M2d wypełnił `BuildingSet.workplaces` rolami z `data/jobs/roles.ron`, bo archetypy
/// powstają dopiero tutaj (korekta E5). Tablica jest płaska, a `Unit.workplaces` to
/// zakres w niej — więc zmiana liczby stanowisk w jednym lokalu przesuwa wszystkie
/// następne. Przebudowa całej tablicy raz jest tańsza i prostsza od wstawiania w środek.
fn rebind_workplaces(
    bi: &BuildInput,
    sc: &SiteCatalog,
    districts: &DistrictSet,
    parcels: &ParcelSet,
    buildings: &mut BuildingSet,
    set: &mut SiteSet,
    epoch_key: &str,
) {
    let _ = epoch_key;
    // Kalibracja drugiej połowy T10 (korekta I-6). Przelicznik „m² na stanowisko" jest
    // tą samą dźwignią co zagęszczenie mieszkań: liczba etatów wychodząca z Etapu 6
    // zależy od tego, ile powierzchni usługowej dał teren, a to zmienia się z ziarnem.
    // Najpierw liczymy stanowiska przy przeliczniku z danych, potem korygujemy go raz,
    // globalnie, żeby suma trafiła w środek okna T10.
    let cel = bi.plan.target_pop as f32 * build::ETATOW_NA_MIESZKANCA * 1.03;
    // Cztery przebiegi o **stałym budżecie**, nie do zbieżności. Jedno przeliczenie
    // nie wystarcza, bo liczba stanowisk w lokalu jest zaokrąglana w górę do jedynki:
    // przy małych lokalach — a takie ma wieś — suma nie jest liniowa względem
    // przelicznika i pierwsze podstawienie trafia obok o kilka procent.
    let mut wp_scale = 1.0f32;
    for _ in 0..4 {
        let ile = licz_stanowiska(bi, sc, set, buildings, wp_scale);
        if ile <= 0.0 || cel <= 0.0 {
            break;
        }
        let nowa = (wp_scale * ile / cel).clamp(build::ETATY_SCALE_MIN, build::ETATY_SCALE_MAX);
        if (nowa - wp_scale).abs() < 0.005 {
            break;
        }
        wp_scale = nowa;
    }

    let mut nowe: Vec<Workplace> = Vec::with_capacity(buildings.workplaces.len());
    let mut zakres_zakladu: Vec<Range<u32>> = vec![0..0; set.sites.len()];

    for bi_idx in 0..buildings.buildings.len() {
        let (units, parcel) = {
            let b = &buildings.buildings[bi_idx];
            (b.units.clone(), b.parcel)
        };
        let site_idx = set.by_building[bi_idx];
        let p = &parcels.parcels[parcel.0.index() as usize];
        let block = &bi.blocks.blocks[p.block.0 as usize];
        let epoka = bi
            .zones
            .rings
            .get(usize::from(block.epoch_ring))
            .map_or("contemporary", |e| e.key.as_str());
        let epoch_mult = bi.jobs.epoch_mult(epoka);
        let tier = districts
            .districts
            .get(p.district.0 as usize)
            .map_or(2, |d| d.income_tier);
        let arch = site_idx.map(|i| sc.get(set.sites[i as usize].archetype));
        let od_budynku = nowe.len() as u32;

        for ui in units.start..units.end {
            let (kind, area) = {
                let u = &buildings.units[ui as usize];
                (u.kind, u.area_m2)
            };
            let wp_od = nowe.len() as u32;
            if let Some((role_id, role)) = bi.jobs.role_for(kind) {
                let na_stanowisko = match arch {
                    Some(a) if a.spec.m2_per_workplace > 0 => a.spec.m2_per_workplace,
                    _ => role.m2_per_workplace.max(1),
                };
                let ile = (f32::from(area) / (f32::from(na_stanowisko.max(1)) * wp_scale))
                    .round()
                    .max(1.0) as u32;
                let mult = epoch_mult * arch.map_or(1.0, |a| a.spec.wage_mult);
                let band = build::wage_band(role, mult, tier);
                for _ in 0..ile {
                    nowe.push(Workplace {
                        unit: UnitIdx(ui),
                        site: site_idx.map(site_id),
                        role: role_id,
                        shift: role.shift.into(),
                        wage_band: band,
                        occupant: None,
                    });
                }
            }
            let u = &mut buildings.units[ui as usize];
            u.workplaces = wp_od..nowe.len() as u32;
            // Lokal niemieszkalny w budynku zakładu należy do tego zakładu; mieszkanie
            // zostaje `Vacant` i czeka na gospodarstwo domowe z M3.
            if let Some(i) = site_idx {
                if !kind.is_dwelling() {
                    u.occupant = UnitOccupant::Site(site_id(i));
                }
            }
        }
        if let Some(i) = site_idx {
            zakres_zakladu[i as usize] = od_budynku..nowe.len() as u32;
        }
    }

    for (s, r) in set.sites.iter_mut().zip(zakres_zakladu) {
        s.workplaces = r;
    }
    buildings.report.workplaces = nowe.len() as u32;
    buildings.workplaces = nowe;
}

/// Liczba stanowisk, jaka wyszłaby przy zadanym mnożniku przelicznika. Przebieg suchy —
/// potrzebny raz, żeby wyznaczyć mnożnik, i nic poza liczbą nie zmienia.
fn licz_stanowiska(
    bi: &BuildInput,
    sc: &SiteCatalog,
    set: &SiteSet,
    buildings: &BuildingSet,
    scale: f32,
) -> f32 {
    let mut n = 0f32;
    for (i, b) in buildings.buildings.iter().enumerate() {
        let arch = set.by_building[i].map(|s| sc.get(set.sites[s as usize].archetype));
        for u in &buildings.units[b.units.start as usize..b.units.end as usize] {
            let Some((_, role)) = bi.jobs.role_for(u.kind) else {
                continue;
            };
            let na_stanowisko = match arch {
                Some(a) if a.spec.m2_per_workplace > 0 => a.spec.m2_per_workplace,
                _ => role.m2_per_workplace.max(1),
            };
            n += (f32::from(u.area_m2) / (f32::from(na_stanowisko.max(1)) * scale))
                .round()
                .max(1.0);
        }
    }
    n
}

// ── Domknięcie łańcuchów produktowych (WP14, §5.8) ───────────────────────────────────

/// Dobowa przepustowość towarowa bramy, w gramach.
///
/// `GateKind::capacity()` jest w `Qty` (milisztuki) i opisuje przepustowość **ruchu**,
/// nie masy — przeliczanie jednego na drugie byłoby myleniem jednostek. Import liczy się
/// więc w tonach na dobę, a właścicielem docelowego, dynamicznego limitu jest M6.
const fn brama_t_na_dobe(k: GateKind) -> i64 {
    match k {
        GateKind::Highway => 4_000,
        GateKind::RailFreight => 20_000,
        GateKind::Port => 40_000,
        GateKind::RailPassenger | GateKind::Airport => 0,
    }
}

/// Domknięcie osiągalności + naprawa + bilans przepustowości (§5.8, KROKI 1–5).
///
/// Zwraca raport; `sites` są mutowane, bo KROK 5 zmienia `capacity_scale`.
pub fn supply_closure_check(
    cat: &Catalog,
    sites: &mut [SiteSeed],
    popyt_bazowy: &[i64],
    roads: &super::road::RoadNetwork,
) -> ClosureReport {
    let mut rep = ClosureReport::default();
    let n = cat.goods.len();

    // Budżet importu: suma przepustowości bram towarowych, w gramach na dobę.
    let mut budzet_importu: i64 = roads
        .gates
        .iter()
        .map(|g| brama_t_na_dobe(g.kind).saturating_mul(1_000_000))
        .sum();
    let importowalny: Vec<bool> = cat
        .goods
        .iter()
        .map(|g| {
            g.has_external_price()
                && g.gates()
                    .iter()
                    .any(|k| roads.gates.iter().any(|x| x.kind == *k))
        })
        .collect();

    // KROK 1–2: osiągalność na recepturach **zainstancjonowanych**, nie na całym katalogu.
    let mut zainstalowane: Vec<RecipeId> = sites.iter().flat_map(|s| s.recipes.clone()).collect();
    zainstalowane.sort_by_key(|r| r.0);
    zainstalowane.dedup();

    let mut import = vec![0i64; n];
    let seed: Vec<GoodId> = (0..n as u16)
        .map(GoodId)
        .filter(|g| importowalny[g.0 as usize])
        .collect();
    let osiagalne = cat.reachable(&seed, &zainstalowane);

    // KROK 3: czego brakuje. `demanded` wg def(3) z §5.8: koszyk, wejścia receptur
    // zainstancjonowanych i materiały eksploatacyjne archetypów (te siedzą już
    // w `popyt_bazowy`).
    let mut demanded = vec![false; n];
    for (g, d) in demanded.iter_mut().enumerate() {
        if popyt_bazowy[g] > 0 {
            *d = true;
        }
    }
    for r in &zainstalowane {
        for (g, _) in &cat.recipe(*r).inputs {
            demanded[g.0 as usize] = true;
        }
    }

    // KROK 4: naprawa. 4a — import, jeśli towar ma cenę zewnętrzną i zgodną bramę.
    // 4b (dostawienie zakładu) jest w tym generatorze **wykonane wcześniej**: liczba
    // zakładów wynika z popytu (korekta I-2), więc brak zakładu znaczy „nie było popytu",
    // a nie „zabrakło miejsca". 4c (przestrefowanie kwartału) nie jest wykonalne po
    // podziale na parcele i po zabudowie — jego rolę przejmuje wielostrefowość archetypów
    // w `data/buildings/` (korekta I-3).
    for g in 0..n {
        if demanded[g] && !osiagalne[g] {
            if importowalny[g] {
                import[g] = import[g].max(1);
            } else {
                rep.missing.push(cat.goods[g].key.clone());
            }
        }
    }

    // KROK 5: bilans przepustowości. Sześć przebiegów o **stałym budżecie** — nie pętla
    // do zbieżności (ryzyko R3): skala zakładów wyszła już z popytu, więc te przebiegi
    // domykają zaokrąglenia, a nie szukają rozwiązania.
    //
    // Import liczy się **od zera w każdym przebiegu**, a nie narastająco. Kumulowanie go
    // było błędem: przywóz uzupełniony w przebiegu drugim zostawał w mocy po tym, jak
    // przebieg trzeci zmniejszył popyt, i zboże wychodziło z bilansu z nadwyżką 4,7×,
    // choć nikt go nie zamawiał.
    let mut import = vec![0i64; n];
    for _ in 0..6 {
        let (supply, demand) = bilans(cat, sites, popyt_bazowy, &pusty(n));
        let wiodacy = wiodace_wyjscia(cat, &zainstalowane, &demand);
        let mut zmiana = false;
        for g in 0..n {
            if !demanded[g] || demand[g] <= 0 {
                continue;
            }
            let r = supply[g] as f64 / demand[g] as f64;
            let za_malo = r < RATIO_MIN && !importowalny[g];
            let za_duzo = r > RATIO_MAX && wiodacy[g];
            if !za_malo && !za_duzo {
                continue;
            }
            let cel = if za_malo { 0.98 } else { 1.10 };
            let mnoznik = cel / r.max(1e-9);
            for s in sites.iter_mut() {
                let dotyczy = s.recipes.iter().any(|x| {
                    let rec = cat.recipe(*x);
                    rec.outputs.iter().any(|(o, _)| o.0 as usize == g)
                        // Przy nadmiarze ruszamy **tylko** zakłady, dla których ten towar
                        // jest wyjściem wiodącym — inaczej ścięcie nadmiaru skóry
                        // zabrałoby miastu mięso.
                        && (za_malo || wiodace_dla(cat, *x, &demand) == Some(GoodId(g as u16)))
                });
                if !dotyczy {
                    continue;
                }
                let nowa = ((f64::from(s.capacity_scale) * mnoznik).round() as i64)
                    .clamp(i64::from(SCALE_MIN), i64::from(SCALE_MAX))
                    as u16;
                if nowa != s.capacity_scale {
                    s.capacity_scale = nowa;
                    zmiana = true;
                }
            }
            // Kiedy nadmiaru nie da się już zdjąć skalą, bo wszystkie zakłady stoją
            // na dolnej granicy, **gasimy** nadmiarowe: zostają budynkiem i etatami,
            // tracą receptury. To jest dźwignia, której §5.8 nie przewidział (korekta I-2)
            // — bez niej ostatni niepodzielny zakład trzyma bilans poza widełkami.
            if za_duzo && !zmiana {
                zmiana |= zgas_nadmiarowe(
                    cat,
                    sites,
                    GoodId(g as u16),
                    supply[g],
                    demand[g],
                    importowalny[g],
                );
            }
        }
        if !zmiana {
            break;
        }
    }

    // Czego po skalowaniu wciąż brakuje, a da się sprowadzić — dowozi się, w kolejności
    // `GoodId` i do wyczerpania przepustowości bram. Zawsze od zera, więc przywóz jest
    // funkcją stanu końcowego, a nie historii przebiegów.
    {
        let (supply, demand) = bilans(cat, sites, popyt_bazowy, &pusty(n));
        for g in 0..n {
            if !demanded[g] || !importowalny[g] || demand[g] <= 0 {
                continue;
            }
            let brak = demand[g] - supply[g];
            if brak <= 0 {
                continue;
            }
            let ile = brak.min(budzet_importu.max(0));
            if ile > 0 {
                import[g] = ile;
                budzet_importu -= ile;
            }
        }
    }

    let (supply, demand) = bilans(cat, sites, popyt_bazowy, &import);
    let wiodacy = wiodace_wyjscia(cat, &zainstalowane, &demand);
    let stosunek = |g: usize| supply[g] as f64 / demand[g] as f64;
    rep.ratios = (0..n)
        .filter(|g| demanded[*g] && demand[*g] > 0)
        .map(|g| (cat.goods[g].key.clone(), stosunek(g) as f32))
        .collect();
    rep.worst = rep
        .ratios
        .iter()
        .map(|(k, r)| (k.clone(), *r))
        .min_by(|a, b| {
            let da = (f64::from(a.1) - 1.0).abs();
            let db = (f64::from(b.1) - 1.0).abs();
            db.total_cmp(&da).then(a.0.cmp(&b.0))
        })
        .unwrap_or_default();
    rep.imported = (0..n)
        .filter(|g| import[*g] > 0)
        .map(|g| (cat.goods[g].key.clone(), import[g] / 1000))
        .collect();
    rep.byproduct_surplus = (0..n)
        .filter(|g| demanded[*g] && demand[*g] > 0 && !wiodacy[*g] && stosunek(*g) > RATIO_MAX)
        .map(|g| (cat.goods[g].key.clone(), stosunek(g) as f32))
        .collect();
    for g in 0..n {
        if !demanded[g] || demand[g] <= 0 {
            continue;
        }
        let r = stosunek(g);
        if r < RATIO_MIN || (r > RATIO_MAX && wiodacy[g]) {
            rep.errors.push(format!(
                "podaż/popyt dla `{}` wynosi {r:.2}, poza [{RATIO_MIN}; {RATIO_MAX}]",
                cat.goods[g].key
            ));
        }
    }
    rep
}

/// Zdejmuje receptury z nadmiarowych zakładów produkujących `g`, od najwyższego indeksu,
/// zostawiając zawsze co najmniej jeden. Zakład bez receptur nadal ma budynek, etaty
/// i firmę — przestaje tylko być ogniwem łańcucha.
fn zgas_nadmiarowe(
    cat: &Catalog,
    sites: &mut [SiteSeed],
    g: GoodId,
    supply: i64,
    demand: i64,
    importowalny: bool,
) -> bool {
    let czynne: Vec<usize> = (0..sites.len())
        .filter(|i| {
            sites[*i]
                .recipes
                .iter()
                .any(|x| cat.recipe(*x).outputs.iter().any(|(o, _)| *o == g))
        })
        .collect();
    // Ostatni zakład wolno zgasić tylko wtedy, kiedy towar da się sprowadzić. Inaczej
    // miasto zostałoby bez wody albo bez betonu, a tego nie naprawi żaden przywóz.
    let minimum = usize::from(!importowalny);
    if czynne.len() <= minimum {
        return false;
    }
    let cel = (demand as f64 * RATIO_MAX) as i64;
    let na_zaklad = supply / czynne.len() as i64;
    if na_zaklad <= 0 {
        return false;
    }
    let zostawic = ((cel / na_zaklad).max(minimum as i64) as usize).min(czynne.len());
    if zostawic >= czynne.len() {
        return false;
    }
    for i in czynne.into_iter().skip(zostawic) {
        sites[i].recipes.clear();
    }
    true
}

/// Wektor zer o długości katalogu — bilans „bez importu".
fn pusty(n: usize) -> Vec<i64> {
    vec![0; n]
}

/// Wyjście, które **wyznacza skalę** receptury przy danym popycie: to, dla którego
/// `popyt / wydajność` jest największe. Pozostałe wyjścia są produktem ubocznym.
fn wiodace_dla(cat: &Catalog, r: RecipeId, demand: &[i64]) -> Option<GoodId> {
    let rec = cat.recipe(r);
    rec.outputs
        .iter()
        .map(|(g, _)| {
            let y = rec.daily_yield(*g).max(1);
            (*g, demand[g.0 as usize] as f64 / y as f64)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0 .0.cmp(&a.0 .0)))
        .map(|(g, _)| g)
}

/// Maska „ten towar jest dla którejś receptury wyjściem wiodącym".
///
/// **Korekta I-4 wobec §5.8.** Górna granica `ratio ≤ 1,30` z testu T11 jest nieosiągalna
/// dla produktu ubocznego: rzeźnia skalowana mięsem wyprodukuje tyle skóry, ile wyjdzie
/// z tuszy, i żadne skalowanie tego nie zmieni bez zepsucia bilansu mięsa. Algorytm
/// z §5.8 zna jedną dźwignię — `capacity_scale` całej receptury — więc nie ma jak
/// rozdzielić wyjść sprzężonych. Nadwyżka produktu ubocznego **wychodzi z miasta**
/// (odbiera ją dostawca zewnętrzny) i jest w raporcie wypisana osobno, zamiast udawać
/// błąd bilansu. Dolna granica 0,85 obowiązuje **bez wyjątku**: niedobór jest zawsze błędem.
fn wiodace_wyjscia(cat: &Catalog, zainstalowane: &[RecipeId], demand: &[i64]) -> Vec<bool> {
    let mut out = vec![false; cat.goods.len()];
    for r in zainstalowane {
        if let Some(g) = wiodace_dla(cat, *r, demand) {
            out[g.0 as usize] = true;
        }
    }
    out
}

/// Dobowa podaż i popyt każdego towaru, w gramach.
fn bilans(
    cat: &Catalog,
    sites: &[SiteSeed],
    popyt_bazowy: &[i64],
    import: &[i64],
) -> (Vec<i64>, Vec<i64>) {
    let mut supply = import.to_vec();
    let mut demand = popyt_bazowy.to_vec();
    for s in sites {
        let k = i64::from(s.capacity_scale);
        for r in &s.recipes {
            let rec = cat.recipe(*r);
            for (g, _) in &rec.outputs {
                let v = rec.daily_yield(*g).saturating_mul(k) / i64::from(SCALE_BASE);
                supply[g.0 as usize] = supply[g.0 as usize].saturating_add(v);
            }
            // Wejścia rolnictwa **też** są popytem: obora naprawdę zjada paszę, choć
            // formalnie jest punktem wejścia łańcucha (`RecipeSource::Agriculture`).
            for (g, _) in &rec.inputs {
                let v = rec.daily_input(*g).saturating_mul(k) / i64::from(SCALE_BASE);
                demand[g.0 as usize] = demand[g.0 as usize].saturating_add(v);
            }
        }
    }
    (supply, demand)
}
