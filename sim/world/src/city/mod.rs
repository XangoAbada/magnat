//! Generator miasta (faza M2). **M2b** dostarcza Etap 3 (bramy, szkielet transportu,
//! kwartały), **M2c** Etapy 4 i 5 (pola wpływu, strefy, pierścienie epok, dzielnice,
//! sieć lokalna, parcele, kolej towarowa).
//!
//! Wejściem jest `CityPlan` i `TerrainQuery` (**K-13** — teren wyłącznie przez kontrakt
//! M1, nigdy przez surowe dane). Wyjściem `CityData`. Zabudowę (M2d) i gospodarkę bazową
//! wraz z wyceną (M2e) dokładają kolejne podfazy.
//!
//! Kolejność etapów M2c odbiega od numeracji pakietów i to jest świadome (korekta C8):
//! dzielnice (WP9) idą **przed** parcelami (WP8), bo przypisanie dzielnic przenumerowuje
//! kwartały, a po powstaniu parcel trzeba by przestawiać dwie tablice zamiast jednej.

pub mod blocks;
pub mod build;
pub mod catalog;
pub mod derive;
pub mod districts;
pub mod gates;
pub mod grammar;
pub mod inspect;
pub mod lsystem;
pub mod overlay;
pub mod parcels;
pub mod pattern;
pub mod poly;
pub mod rail;
pub mod road;
pub mod sites;
pub mod value;
pub mod voxels;
pub mod zoning;

use crate::assets::data_path;
use crate::params::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};
use crate::query::TerrainQuery;
use blocks::BlockSet;
use build::UnitKind;
use districts::{DistrictNames, DistrictSet};
use gates::{CityGate, GateKind, GateProfile};
use magnat_core::{StateHash, StateHasher};
use magnat_spatial::Vec2;
use parcels::ParcelSet;
use pattern::RingTable;
use road::{
    FurnitureKind, NodeFlags, NodeId, PolyArena, RoadClass, RoadFlags, RoadNetwork, RoadNode,
    RoadSegment, SegmentId, StreetFurniture,
};
use zoning::{CityFields, EpochTable, ZoneKind, ZoneResult, ZoningWeights};

/// Wejście generatora miasta (PRD §4.1). Wszystko, czego trzeba, żeby z samego ziarna
/// odtworzyć miasto — i nic ponadto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CityPlan {
    pub seed: u64,
    pub size: WorldSize,
    pub region: Region,
    pub epoch: Epoch,
    pub profile: EconomyProfile,
    pub difficulty: Difficulty,
    /// Docelowa liczba mieszkańców. Nie jest parametrem wejściowym gracza — wynika
    /// z rozmiaru mapy (PRD §4.1) — ale jest **jedyną** wielkością, z której generator
    /// liczy budżety, więc stoi w planie jawnie, a nie w trzech miejscach osobno.
    pub target_pop: u32,
}

/// Gęstość zaludnienia obszaru zurbanizowanego, os./km².
/// Z budżetów M2 §7: 400 tys. mieszkańców na ~70 km².
const URBAN_DENSITY_PER_KM2: f32 = 5_714.0;
/// Segmentów drogowych na mieszkańca⁻¹. Z budżetów M2 §7: ~16 tys. segmentów na 400 tys.
const POP_PER_SEGMENT: u32 = 25;

impl CityPlan {
    #[must_use]
    pub fn from_world(p: &WorldGenParams) -> CityPlan {
        CityPlan {
            seed: p.seed,
            size: p.size,
            region: p.region,
            epoch: p.epoch,
            profile: p.profile,
            difficulty: p.difficulty,
            target_pop: target_pop(p.size),
        }
    }

    #[must_use]
    pub fn map_size_m(&self) -> i32 {
        self.size.meters() as i32
    }

    /// Promień obszaru zurbanizowanego. Przycięty do mapy — miasto ma się na niej
    /// zmieścić, nawet jeśli ludności starczyłoby na większe.
    #[must_use]
    pub fn urban_radius_m(&self) -> f32 {
        let area_km2 = self.target_pop as f32 / URBAN_DENSITY_PER_KM2;
        let r =
            magnat_core::det_math::sqrt(f64::from(area_km2) / std::f64::consts::PI) as f32 * 1000.0;
        r.min(self.map_size_m() as f32 * 0.4)
    }

    /// Twardy budżet segmentów — zabezpieczenie przed rozbieganiem się L-systemu (R1).
    #[must_use]
    pub fn segment_budget(&self) -> usize {
        (self.target_pop / POP_PER_SEGMENT) as usize
    }

    fn profile_path(&self) -> String {
        format!("zoning/profile_{}.ron", self.profile.key())
    }
}

#[must_use]
pub const fn target_pop(size: WorldSize) -> u32 {
    match size {
        WorldSize::Small4km => 40_000,
        WorldSize::Medium8km => 120_000,
        WorldSize::Large12km => 250_000,
        WorldSize::Metropolis16km => 400_000,
    }
}

/// Raport generacji miasta (M2 §1, artefakt 1). M2b wypełnia część transportową;
/// kolejne podfazy dokładają swoje sekcje do tej samej struktury.
#[derive(Clone, PartialEq, Debug)]
pub struct GenerationReport {
    pub center: Vec2,
    pub gates: Vec<(GateKind, Vec2)>,
    pub missing_gates: Vec<GateKind>,
    /// Bramy postawione, ale bez połączenia z siecią w tej podfazie: kolejowe czekają
    /// na tory z M2c. Wypisywane, bo brama, o której raport milczy, znika bez śladu.
    pub deferred_gates: Vec<(GateKind, Vec2)>,
    pub nodes: u32,
    pub segments: u32,
    pub road_km: f64,
    pub bridges: u32,
    pub tunnels: u32,
    pub embankments: u32,
    pub blocks: u32,
    pub dropped_faces: u32,
    pub swallowed_faces: u32,
    pub faces_debug: String,
    pub urban_area_km2: f64,
    pub road_area_km2: f64,
    pub stats: lsystem::LStats,
    // ── M2c ──────────────────────────────────────────────────────────────────────────
    /// Zrealizowany i docelowy udział powierzchniowy stref, w indeksach `ZoneKind::ALL`.
    pub zone_share: [f32; 16],
    pub zone_target: [f32; 16],
    /// Największe odchylenie udziału od kwoty, w punktach procentowych (test T9: ≤ 3).
    pub zone_dev_pp: f32,
    /// Kwartały pozamiejskie — poza kwotami (patrz `zoning::MAX_URBAN_BLOCK_M2`).
    pub rural_blocks: u32,
    pub rural_area_km2: f64,
    /// Dolna granica `zone_dev_pp` wynikająca z ziarnistości kwartałów.
    pub zone_dev_floor_pp: f32,
    pub zone_count: [u32; 16],
    pub zone_candidates: [u32; 16],
    /// Liczba kwartałów w każdym pierścieniu epoki, w kolejności od najstarszego.
    pub epoch_rings: Vec<(String, u32)>,
    pub districts: u32,
    pub district_names: Vec<String>,
    pub district_seeds: u32,
    pub district_snapped: u32,
    pub parcels: u32,
    pub parcels_without_frontage: u32,
    pub parcel_slivers: u32,
    pub local_streets: u32,
    pub segment_splits: u32,
    pub rail: rail::RailReport,
    // ── M2d ──────────────────────────────────────────────────────────────────────────
    /// Etap 6: budynki, lokale, stanowiska, gramatyka awaryjna.
    pub build: build::BuildReport,
    /// Warstwa transportowa w voxelach (§5.6b).
    pub road_voxels: voxels::RoadVoxelReport,
    /// Wartość gruntu po `pass_1` (WP15a): mediana i skrajne dzielnice.
    pub land_value_median: magnat_core::Money,
    /// Średnia w dzielnicach rdzenia (starówka + śródmieście) i obrzeża (przedmieście
    /// + wieś) — para z kryterium WP15a.
    pub land_value_core: magnat_core::Money,
    pub land_value_fringe: magnat_core::Money,
    // ── M2e ──────────────────────────────────────────────────────────────────────────
    /// Etap 7: firmy, zakłady, normatywy.
    pub sites: sites::SiteReport,
    /// Domknięcie łańcuchów produktowych (WP14).
    pub closure: sites::ClosureReport,
    /// Wartość gruntu po `pass_2` — te same trzy liczby co dla `pass_1`, żeby dało się
    /// zobaczyć, co zmienił Etap 7.
    pub land_value_median_2: magnat_core::Money,
    pub land_value_core_2: magnat_core::Money,
    pub land_value_fringe_2: magnat_core::Money,
    /// Odcisk całej warstwy M2 — `city_hash` plus Etap 7 (D1/D5 z §7 fazy).
    pub world_hash_m2: StateHash,
    /// Zastosowane współczynniki kalibracji T10: zagęszczenie mieszkań i przelicznik
    /// stanowisk. 1,0 znaczy „nie było czego kalibrować"; wartość na granicy widełek
    /// znaczy, że teren nie pozwolił dojść do `target_pop` i T10 może nie przejść.
    pub dwelling_scale: f32,
    pub stage_millis: Vec<(&'static str, f64)>,
    pub road_hash: StateHash,
    /// Odcisk warstwy M2c: strefy, dzielnice, parcele. Wchodzi do `world_hash_m2` (M2e).
    pub city_hash: StateHash,
    /// Ostrzeżenia — R1 fazy: generator, który odrzuca ponad 40% propozycji,
    /// buduje co innego, niż planowano, i ma o tym powiedzieć.
    pub warnings: Vec<String>,
}

impl GenerationReport {
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![
            format!("centrum: {:.0}, {:.0}", self.center.x, self.center.y),
            format!(
                "bramy: {}",
                self.gates
                    .iter()
                    .map(|(k, p)| format!("{}@{:.0},{:.0}", k.key(), p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            format!(
                "sieć: {} węzłów, {} segmentów, {:.1} km",
                self.nodes, self.segments, self.road_km
            ),
            format!(
                "struktury: {} mostów, {} tuneli, {} nasypów",
                self.bridges, self.tunnels, self.embankments
            ),
            format!(
                "kwartały: {} · obszar {:.2} km² · pas drogowy {:.2} km²",
                self.blocks, self.urban_area_km2, self.road_area_km2
            ),
            format!(
                "propozycje: {} · przyjęte {} · odrzucone {}% (nachylenie {}, przeprawa {}, kąt {}, strefa {})",
                self.stats.proposals,
                self.stats.accepted,
                self.stats.rejection_pct(),
                self.stats.rejected_slope,
                self.stats.rejected_crossing,
                self.stats.rejected_angle,
                self.stats.rejected_zone,
            ),
            format!(
                "scalenia {} · podziały {} · przycięte {} · kwartały odrzucone {} / pochłonięte {}",
                self.stats.snapped, self.stats.split, self.stats.pruned,
                self.dropped_faces, self.swallowed_faces,
            ),
            self.faces_debug.clone(),
            format!("hash sieci: {:032x}", self.road_hash.0),
        ];
        if self.parcels > 0 || self.districts > 0 {
            v.push(format!(
                "strefy: odchylenie max {:.1} pp (próg ziarnistości {:.1}) · pozamiejskich {} ({:.2} km²) · {}",
                self.zone_dev_pp,
                self.zone_dev_floor_pp,
                self.rural_blocks,
                self.rural_area_km2,
                ZoneKind::ALL
                    .iter()
                    .filter(|z| {
                    self.zone_share[z.index()] > 0.0005
                        || self.zone_target[z.index()] > 0.0005
                        || self.zone_count[z.index()] > 0
                })
                    .map(|z| format!(
                        "{} {}k{}×{:.1}/{:.1}%",
                        z.key(),
                        self.zone_candidates[z.index()],
                        self.zone_count[z.index()],
                        self.zone_share[z.index()] * 100.0,
                        self.zone_target[z.index()] * 100.0
                    ))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
            v.push(format!(
                "pierścienie epok: {}",
                self.epoch_rings
                    .iter()
                    .map(|(k, n)| format!("{k} {n}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
            v.push(format!(
                "dzielnice: {} (zalążków {}, dogięć {}) — {}",
                self.districts,
                self.district_seeds,
                self.district_snapped,
                self.district_names.join(", ")
            ));
            v.push(format!(
                "parcele: {} · bez frontu {} · odpad {} · ulice lokalne {} · podziały segmentów {}",
                self.parcels,
                self.parcels_without_frontage,
                self.parcel_slivers,
                self.local_streets,
                self.segment_splits
            ));
            v.push(format!(
                "kolej: {:.1} km · bocznic {} · rozjazdów {} · w zasięgu istniejących {} · bez połączenia {} · max nachylenie {:.2}%",
                self.rail.track_km,
                self.rail.sidings,
                self.rail.junctions,
                self.rail.covered,
                self.rail.unreachable,
                self.rail.max_grade_pct
            ));
            v.push(format!(
                "zabudowa: {} budynków · {} lokali ({} mieszkań) · {} stanowisk · fallback {} ({:.1}%) · rozluźnień {} · za małe {} · puste z zamiaru {} · bez rampy {}",
                self.build.buildings,
                self.build.units,
                self.build.dwellings,
                self.build.workplaces,
                self.build.fallback,
                if self.build.buildings > 0 {
                    f64::from(self.build.fallback) * 100.0 / f64::from(self.build.buildings)
                } else {
                    0.0
                },
                self.build.relaxed,
                self.build.too_small,
                self.build.left_vacant,
                self.build.ramp_missing
            ));
            if self.build.relaxed > 0 {
                v.push(format!(
                    "  rozluźnienia: epoka {} · styl {} · wartość gruntu {} — pierwsze dwa łata się plikiem w data/grammar/, trzecie liczbą w istniejącym",
                    self.build.relaxed_epoch, self.build.relaxed_style, self.build.relaxed_value
                ));
            }
            v.push(format!(
                "  detal bryły: {} wysunięć ({} przyciętych do działki, {} odrzuconych jako płytsze niż voxel)",
                self.build.protrusions, self.build.protrusions_clipped, self.build.protrusions_dropped
            ));
            v.push(format!(
                "  różnorodność: powtórki sygnatury w promieniu 60 m {:.1}% ({} z {} par) · najgorsze {:.1}% w {},{} · entropia gramatyk: miasto {:.2} bita, dzielnica {} min {:.2} ({} mierzonych, dominuje gramatyka #{} z {} na {} budynków)",
                if self.build.signature_pairs > 0 {
                    f64::from(self.build.signature_repeats) * 100.0 / f64::from(self.build.signature_pairs)
                } else {
                    0.0
                },
                self.build.signature_repeats,
                self.build.signature_pairs,
                f64::from(self.build.worst_neighbourhood_permille) / 10.0,
                self.build.worst_neighbourhood_at.0,
                self.build.worst_neighbourhood_at.1,
                f64::from(self.build.city_entropy_mbits) / 1000.0,
                self.build.min_district_entropy_at,
                f64::from(self.build.min_district_entropy_mbits) / 1000.0,
                self.build.districts_measured,
                self.build.min_district_top.0,
                self.build.min_district_top.1,
                self.build.min_district_top.2
            ));
            if !self.build.grammar_hist.is_empty() {
                let mut h: Vec<(usize, u32)> = self
                    .build
                    .grammar_hist
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|(_, n)| *n > 0)
                    .collect();
                h.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                v.push(format!(
                    "  gramatyki wg liczby budynków: {}",
                    h.iter()
                        .map(|(i, n)| format!("#{i} {n}"))
                        .collect::<Vec<_>>()
                        .join(" · ")
                ));
            }
            if self.build.fallback > 0 {
                v.push(format!(
                    "  awaryjne wg stref: {}",
                    ZoneKind::ALL
                        .iter()
                        .filter(|z| self.build.fallback_zone[z.index()] > 0)
                        .map(|z| format!("{} {}", z.key(), self.build.fallback_zone[z.index()]))
                        .collect::<Vec<_>>()
                        .join(" · ")
                ));
            }
            v.push(format!(
                "voxele: {} komend zabudowy + {} komend drogowych na {} segmentach ({} schodkowanych, {} mostów, {} tuneli)",
                self.build.edit_commands,
                self.road_voxels.commands,
                self.road_voxels.segments,
                self.road_voxels.stepped,
                self.road_voxels.bridges,
                self.road_voxels.tunnels
            ));
            v.push(format!(
                "wartość gruntu (pass_1): mediana {:.2} zł/m² · rdzeń {:.2} · obrzeże {:.2}",
                self.land_value_median.0 as f64 / 100.0,
                self.land_value_core.0 as f64 / 100.0,
                self.land_value_fringe.0 as f64 / 100.0
            ));
            v.push(format!(
                "wartość gruntu (pass_2): mediana {:.2} zł/m² · rdzeń {:.2} · obrzeże {:.2}",
                self.land_value_median_2.0 as f64 / 100.0,
                self.land_value_core_2.0 as f64 / 100.0,
                self.land_value_fringe_2.0 as f64 / 100.0
            ));
            v.push(format!(
                "etap 7: {} firm · {} zakładów · normatywy {}/{} · z łańcuchów {} · budynki dostawione {} (nieudane {}) · bez zakładu {}",
                self.sites.firms,
                self.sites.sites,
                self.sites.norm_placed,
                self.sites.norm_target,
                self.sites.from_chain,
                self.sites.buildings_added,
                self.sites.buildings_failed,
                self.sites.parcels_without_site
            ));
            if !self.sites.unplaced.is_empty() {
                v.push(format!(
                    "  bez działki: {}",
                    self.sites
                        .unplaced
                        .iter()
                        .map(|(k, n)| format!("{k} ×{n}"))
                        .collect::<Vec<_>>()
                        .join(" · ")
                ));
            }
            v.push(format!(
                "  sektory: {}",
                sites::SectorId::ALL
                    .iter()
                    .filter(|s| self.sites.by_sector[**s as usize] > 0)
                    .map(|s| format!("{} {}", s.key(), self.sites.by_sector[*s as usize]))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
            v.push(format!(
                "domknięcie łańcuchów: brakujące {} · import {} towarów · najgorszy stosunek {} {:.2} · nadwyżki uboczne {}",
                if self.closure.missing.is_empty() {
                    "brak".to_string()
                } else {
                    self.closure.missing.join(", ")
                },
                self.closure.imported.len(),
                self.closure.worst.0,
                self.closure.worst.1,
                self.closure.byproduct_surplus.len()
            ));
            if !self.closure.imported.is_empty() {
                v.push(format!(
                    "  import (t/dobę): {}",
                    self.closure
                        .imported
                        .iter()
                        .map(|(k, t)| format!("{k} {}", t / 1000))
                        .collect::<Vec<_>>()
                        .join(" · ")
                ));
            }
            for e in &self.closure.errors {
                v.push(format!("  BŁĄD DOMKNIĘCIA: {e}"));
            }
            v.push(format!("hash miasta: {:032x}", self.city_hash.0));
            v.push(format!("hash M2: {:032x}", self.world_hash_m2.0));
        }
        for (n, ms) in &self.stage_millis {
            v.push(format!("  {n}: {ms:.1} ms"));
        }
        if !self.deferred_gates.is_empty() {
            v.push(format!(
                "bramy odłożone do M2c (tory): {}",
                self.deferred_gates
                    .iter()
                    .map(|(k, p)| format!("{}@{:.0},{:.0}", k.key(), p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        if !self.missing_gates.is_empty() {
            v.push(format!(
                "BRAK BRAM WYMAGANYCH: {}",
                self.missing_gates
                    .iter()
                    .map(|k| k.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for w in &self.warnings {
            v.push(format!("OSTRZEŻENIE: {w}"));
        }
        v
    }
}

#[derive(Clone, Debug)]
pub struct CityData {
    pub plan: CityPlan,
    pub center: Vec2,
    pub roads: RoadNetwork,
    pub blocks: BlockSet,
    /// Pola wpływu — wejście wyceny gruntu (M2e) i nakładek renderu.
    pub fields: CityFields,
    pub zones: ZoneResult,
    pub districts: DistrictSet,
    pub parcels: ParcelSet,
    /// Etap 6 — budynki, lokale, stanowiska (M2d).
    pub buildings: build::BuildingSet,
    /// Etap 7 — firmy i zakłady jako obiekty danych (M2e).
    pub sites: sites::SiteSet,
    /// Pola dostępności `pass_2` — wejście nakładki wartości gruntu i karty inspekcji.
    pub access: value::AccessFields,
    /// Katalog towarów i receptur, na którym domknięto łańcuchy. Trzymany, bo karta
    /// inspekcji i raport mówią o towarach kluczami, a nie indeksami.
    pub catalog: catalog::Catalog,
    pub site_catalog: sites::SiteCatalog,
    /// Komendy voxelowe całej generacji, z indeksem chunkowym.
    ///
    /// **Nie nakładka per voxel**: miasto to setki milionów zmienionych voxeli, a komend
    /// jest kilkaset tysięcy. Konsument (klient graficzny, headless) stosuje je przy
    /// materializacji chunka — patrz `magnat_voxel::EditIndex`.
    pub edits: magnat_voxel::EditIndex,
    pub report: GenerationReport,
}

#[derive(Debug)]
pub enum CityGenError {
    Profile(String),
    Rings(pattern::RingError),
    Data(zoning::EpochError),
    Grammar(grammar::GrammarError),
    Catalog(catalog::CatalogError),
    Sites(sites::SiteDataError),
}

impl std::fmt::Display for CityGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CityGenError::Profile(s) => write!(f, "{s}"),
            CityGenError::Rings(e) => write!(f, "{e}"),
            CityGenError::Data(e) => write!(f, "{e}"),
            CityGenError::Grammar(e) => write!(f, "{e}"),
            CityGenError::Catalog(e) => write!(f, "{e}"),
            CityGenError::Sites(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CityGenError {}

pub const PROFILE_SCHEMA_VERSION: u32 = 2;

fn load_profile(plan: &CityPlan) -> Result<GateProfile, CityGenError> {
    let path = data_path(&plan.profile_path());
    let txt = std::fs::read_to_string(&path)
        .map_err(|e| CityGenError::Profile(format!("{}: {e}", path.display())))?;
    let p: GateProfile = ron::from_str(&txt)
        .map_err(|e| CityGenError::Profile(format!("{}: {e}", path.display())))?;
    if p.schema_version != PROFILE_SCHEMA_VERSION {
        return Err(CityGenError::Profile(format!(
            "{}: schema_version {}, oczekiwano {PROFILE_SCHEMA_VERSION}",
            path.display(),
            p.schema_version
        )));
    }
    Ok(p)
}

/// Generacja miasta: Etap 3 (M2b), Etapy 4 i 5 (M2c) oraz Etap 6 z wyceną `pass_1` (M2d).
///
/// `mats` i `pool` doszły w M2d: gramatyka odwołuje się do materiałów po kluczu, a derywacja
/// 50 tys. budynków jest jedynym zrównoleglonym krokiem fazy. Sygnatura z §6 dokumentu fazy
/// nie przewidywała ani jednego, ani drugiego (korekta E2).
pub fn generate_city(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
    mats: &magnat_voxel::MaterialRegistry,
    pool: &magnat_jobs::JobPool,
) -> Result<CityData, CityGenError> {
    let profile = load_profile(plan)?;
    let rings_tbl = RingTable::load().map_err(CityGenError::Rings)?;
    let epochs = EpochTable::load().map_err(CityGenError::Data)?;
    let weights = ZoningWeights::load().map_err(CityGenError::Data)?;
    let names = DistrictNames::load().map_err(CityGenError::Data)?;
    let grammars = grammar::GrammarSet::load_dir(&data_path("grammar"), mats)
        .map_err(CityGenError::Grammar)?;
    let jobs = build::JobTable::load().map_err(CityGenError::Data)?;
    // Epoka **startowa** miasta to ostatni pierścień z `data/epochs/` — to ona decyduje
    // o koszyku potrzeb i o tym, które archetypy w ogóle występują.
    let rings_epoch = epochs.rings_for(plan.epoch);
    let epoch_key: String = rings_epoch
        .last()
        .map_or_else(|| "contemporary".to_string(), |e| e.key.clone());
    let epoch_key = epoch_key.as_str();
    let catalog = catalog::load_default(epoch_key).map_err(CityGenError::Catalog)?;
    let site_catalog =
        sites::SiteCatalog::load_default(&catalog, &grammars).map_err(CityGenError::Sites)?;
    let mut stage = Vec::new();
    let mut zegar = std::time::Instant::now();
    let mut tik = |stage: &mut Vec<(&'static str, f64)>, n: &'static str| {
        stage.push((n, zegar.elapsed().as_secs_f64() * 1000.0));
        zegar = std::time::Instant::now();
    };

    let gp = gates::place_gates(plan, t, &profile);
    tik(&mut stage, "bramy");

    let grown = lsystem::grow_network(plan, t, &gp, &rings_tbl);
    tik(&mut stage, "l-system");

    let (mut roads, stats) = finalize(grown, &gp, t);
    tik(&mut stage, "domknięcie sieci");

    // Arena wyjęta na czas budowy kwartałów i parcel: te funkcje czytają sieć
    // i **dopisują** do areny obrysy, a to dwa różne pożyczenia tej samej struktury.
    let mut geom = std::mem::take(&mut roads.geom);
    let mut blocks = blocks::build_blocks(&roads, &mut geom);
    tik(&mut stage, "kwartały");

    // ── M2c ─────────────────────────────────────────────────────────────────────────
    let fields = zoning::build_fields(plan, t, &roads, &gp, gp.center);
    tik(&mut stage, "pola wpływu");

    let adj = blocks::block_adjacency(&blocks, roads.segments.len());
    let samples = zoning::sample_blocks(&blocks, &geom, t, gp.center);
    let rings = epochs.rings_for(plan.epoch);
    let mut zones = zoning::assign_zones(
        plan,
        &blocks,
        &adj,
        &fields,
        &samples,
        &weights,
        &profile.mix,
        rings,
        gp.center,
    );
    // Zakaz ruchu ciężkiego jest nadawany po tonażu klasy w Etapie 3, kiedy stref
    // jeszcze nie ma. Teraz są — i ulica obsługująca halę przestaje być ulicą osiedlową.
    zoning::allow_heavy_on_industrial_streets(&mut roads, &blocks, &zones);
    tik(&mut stage, "strefy i pierścienie epok");

    // Dzielnice **przed** parcelami: przypisanie przenumerowuje kwartały (korekta C8).
    let mut districts = districts::build_districts(
        plan,
        t,
        &mut roads,
        &mut geom,
        &mut blocks,
        &mut zones,
        &adj,
        &fields,
        gp.center,
        &names,
    );
    tik(&mut stage, "dzielnice");

    let centroidy: Vec<Vec2> = blocks
        .blocks
        .iter()
        .map(|b| poly::centroid(geom.get(b.poly)))
        .collect();
    let segments_before = roads.segments.len();
    let rail_rep = rail::build_rail(
        t,
        &gp,
        &mut roads,
        &mut geom,
        &blocks,
        &zones,
        &fields,
        &centroidy,
        segments_before,
    );
    tik(&mut stage, "kolej towarowa");

    let mut parcel_set = parcels::subdivide(plan, t, &mut roads, &mut geom, &mut blocks, &zones);
    tik(&mut stage, "sieć lokalna i parcele");

    districts::assign_road_districts(&mut roads, &blocks);

    // ── M2d ─────────────────────────────────────────────────────────────────────────
    // `pass_1` **przed** zabudową (korekta D1): wartość gruntu jest wejściem doboru
    // gramatyki i liczby kondygnacji, nie jej wynikiem.
    let vctx = value::ValueCtx {
        fields: &fields,
        roads: &roads,
        geom: &geom,
        blocks: &blocks,
        districts: &districts,
        rings: zones.rings.len() as u8,
        access: None,
    };
    value::pass_1(&vctx, &mut parcel_set);
    tik(&mut stage, "wycena gruntu (pass_1)");

    let edits = magnat_voxel::EditQueue::new();
    // Blok, bo `BuildInput` trzyma `&districts`, a `recompute_pop_capacity` zaraz za nim
    // potrzebuje `&mut`. Zakres pożyczenia jest tu treścią, nie formalnością.
    let (buildings, dwelling_scale, sites, land_value_pass_1) = {
        let build_input = build::BuildInput {
            plan,
            terrain: t,
            roads: &roads,
            blocks: &blocks,
            zones: &zones,
            districts: &districts,
            grammars: &grammars,
            materials: mats,
            jobs: &jobs,
        };
        let mut buildings =
            build::build_all(&build_input, &mut geom, &mut parcel_set, &edits, pool);
        // Kalibracja pojemności mieszkaniowej do `target_pop` (T10, korekta I-6) — przed
        // Etapem 7, bo zakłady odwołują się do zakresów lokali.
        let dwelling_scale = build::rescale_dwellings(&mut buildings, plan.target_pop);
        tik(&mut stage, "gramatyka i zabudowa");

        // ── M2e ─────────────────────────────────────────────────────────────────────
        // `pass_2` nadpisze `land_value_per_m2`, więc trzy liczby z `pass_1` zapamiętujemy
        // teraz — raport ma pokazywać, co zmienił Etap 7, a nie samą wartość końcową.
        let lv1 = (
            mediana_wartosci(&parcel_set),
            value::average_by_kind(&districts, &parcel_set, &value::CORE_KINDS),
            value::average_by_kind(&districts, &parcel_set, &value::FRINGE_KINDS),
        );
        // Etap 7: obsada budynków firmami-danymi i domknięcie łańcuchów produktowych.
        let sites = sites::populate(
            &build_input,
            &catalog,
            &site_catalog,
            epoch_key,
            &mut geom,
            &mut parcel_set,
            &mut buildings,
            &edits,
        );
        (buildings, dwelling_scale, sites, lv1)
    };
    // Obietnica korekty C9 z M2c: pojemność dzielnicy liczona z mieszkań, nie z gęstości
    // strefy. Dopiero teraz jest z czego — do M2d `Unit` nie istniał. Po Etapie 7,
    // bo on dostawia budynki na zieleni i w wydobyciu (korekta F2).
    build::recompute_pop_capacity(&mut districts, &parcel_set, &buildings);
    tik(&mut stage, "etap 7 — firmy i domknięcie łańcuchów");

    // `pass_2` **po** Etapie 7 (§5.7): dostęp do pracy i handlu nie istnieje wcześniej,
    // bo nie ma jeszcze ani stanowisk, ani sklepów.
    let access = value::build_access(plan, &geom, &parcel_set, &buildings, &sites, &site_catalog);
    {
        let vctx2 = value::ValueCtx {
            fields: &fields,
            roads: &roads,
            geom: &geom,
            blocks: &blocks,
            districts: &districts,
            rings: zones.rings.len() as u8,
            access: Some(&access),
        };
        value::pass_2(&vctx2, &mut parcel_set);
    }
    value::set_district_averages(&mut districts, &parcel_set);
    tik(&mut stage, "wycena gruntu (pass_2)");

    let road_voxels = voxels::queue_roads(&edits, &roads, &districts, t, mats);
    tik(&mut stage, "warstwa transportowa w voxelach");

    let edit_index = magnat_voxel::EditIndex::build(&edits);
    tik(&mut stage, "indeks edycji voxeli");

    roads.geom = geom;

    let mut warnings = Vec::new();
    if stats.rejection_pct() > 40 {
        warnings.push(format!(
            "L-system odrzucił {}% propozycji (R1: teren wymusza inny układ niż wzorzec)",
            stats.rejection_pct()
        ));
    }
    for g in gp.gates.iter().filter(|g| !g.kind.is_rail()) {
        if !roads
            .gates
            .iter()
            .any(|x| x.kind == g.kind && x.pos == g.pos)
        {
            warnings.push(format!(
                "brama {} przy {:.0},{:.0} nie połączyła się z siecią i wypadła przy domykaniu",
                g.kind.key(),
                g.pos.x,
                g.pos.y
            ));
        }
    }
    // Spójność sieci jezdnej **po** wszystkich krokach, które ją mutują (kolej, dzielnice,
    // ulice lokalne) — `finalize` zostawia jedną składową, ale kolejne etapy dokładają
    // segmenty i test T1 mierzy stan końcowy, nie stan po Etapie 3.
    let (skladowe, najwieksza, jezdnych) = skladowe_jezdne(&roads);
    if skladowe > 1 {
        warnings.push(format!(
            "sieć jezdna rozpadła się na {skladowe} składowych; największa ma {najwieksza} z {jezdnych} segmentów (T1)"
        ));
    }
    if blocks.blocks.is_empty() {
        warnings.push("graf dróg nie zamknął ani jednego kwartału".to_string());
    }
    warnings.extend(zones.warnings.iter().cloned());
    // T9 z progiem ziarnistości: kwartał trafia do strefy w całości, więc odchylenia
    // mniejszego niż udział największego kwartału nie da się osiągnąć żadnym przydziałem.
    let limit_t9 = 3.0f32.max(zones.largest_block_share * 150.0);
    if zones.max_deviation_pp() > limit_t9 {
        warnings.push(format!(
            "udział stref odbiega od profilu o {:.1} pp. (T9: limit {limit_t9:.1})",
            zones.max_deviation_pp()
        ));
    }
    if rail_rep.unreachable > 0 {
        warnings.push(format!(
            "{} klastrów przemysłowych bez bocznicy — teren nie pozwolił poprowadzić toru",
            rail_rep.unreachable
        ));
    }

    let bez_frontu = parcel_set
        .parcels
        .iter()
        .filter(|p| p.frontage.is_none() && !matches!(p.zone, ZoneKind::Green))
        .count() as u32;
    if bez_frontu > 0 {
        warnings.push(format!(
            "{bez_frontu} parcel poza zielenią nie ma frontu drogowego (T7)"
        ));
    }

    let epoch_rings = zones
        .rings
        .iter()
        .enumerate()
        .map(|(i, e)| {
            (
                e.key.clone(),
                zones
                    .epoch_ring
                    .iter()
                    .filter(|r| usize::from(**r) == i)
                    .count() as u32,
            )
        })
        .collect();

    let report = GenerationReport {
        center: gp.center,
        gates: roads.gates.iter().map(|g| (g.kind, g.pos)).collect(),
        missing_gates: gp.missing.clone(),
        deferred_gates: gp
            .gates
            .iter()
            .filter(|g| g.kind.is_rail() && !roads.gates.iter().any(|x| x.pos == g.pos))
            .map(|g| (g.kind, g.pos))
            .collect(),
        nodes: roads.nodes.len() as u32,
        segments: roads.segments.len() as u32,
        road_km: roads.length_m(|s| s.class.is_driveable()) / 1000.0,
        bridges: policz(&roads, |s| {
            matches!(s.structure, road::RoadStructure::Bridge { .. })
        }),
        tunnels: policz(&roads, |s| {
            matches!(s.structure, road::RoadStructure::Tunnel { .. })
        }),
        embankments: policz(&roads, |s| {
            matches!(s.structure, road::RoadStructure::Embankment { .. })
        }),
        blocks: blocks.blocks.len() as u32,
        dropped_faces: blocks.dropped,
        swallowed_faces: blocks.swallowed,
        faces_debug: format!(
            "orbity {} · zewnętrzne {} · zdegenerowane {} · wiadukty {}",
            blocks.orbits, blocks.outer, blocks.degenerate, blocks.excluded_edges
        ),
        urban_area_km2: blocks.urban_area_m2 / 1e6,
        road_area_km2: blocks.road_area_m2 / 1e6,
        stats,
        zone_share: zones.share,
        zone_target: zones.target,
        zone_dev_pp: zones.max_deviation_pp(),
        rural_blocks: zones.rural_blocks,
        rural_area_km2: zones.rural_area_m2 / 1e6,
        zone_dev_floor_pp: zones.largest_block_share * 100.0,
        zone_count: zones.count,
        zone_candidates: zones.candidates,
        epoch_rings,
        districts: districts.districts.len() as u32,
        district_names: districts.districts.iter().map(|d| d.name.clone()).collect(),
        district_seeds: districts.seeds,
        district_snapped: districts.snapped,
        parcels: parcel_set.parcels.len() as u32,
        parcels_without_frontage: bez_frontu,
        parcel_slivers: parcel_set.slivers,
        local_streets: parcel_set.local_streets,
        segment_splits: parcel_set.splits,
        rail: rail_rep,
        build: buildings.report.clone(),
        road_voxels,
        land_value_median: land_value_pass_1.0,
        land_value_core: land_value_pass_1.1,
        land_value_fringe: land_value_pass_1.2,
        sites: sites.report.clone(),
        closure: sites.closure.clone(),
        land_value_median_2: mediana_wartosci(&parcel_set),
        land_value_core_2: value::average_by_kind(&districts, &parcel_set, &value::CORE_KINDS),
        land_value_fringe_2: value::average_by_kind(&districts, &parcel_set, &value::FRINGE_KINDS),
        dwelling_scale,
        world_hash_m2: world_hash_m2(
            &city_hash(&blocks, &zones, &districts, &parcel_set, &buildings),
            &sites,
        ),
        stage_millis: stage,
        road_hash: roads.hash(),
        city_hash: city_hash(&blocks, &zones, &districts, &parcel_set, &buildings),
        warnings,
    };

    Ok(CityData {
        plan: *plan,
        center: gp.center,
        roads,
        blocks,
        fields,
        zones,
        districts,
        parcels: parcel_set,
        buildings,
        sites,
        access,
        catalog,
        site_catalog,
        edits: edit_index,
        report,
    })
}

/// Mediana wartości gruntu po działkach zabudowywalnych — jedna liczba do raportu.
fn mediana_wartosci(parcels: &ParcelSet) -> magnat_core::Money {
    let mut v: Vec<i64> = parcels
        .parcels
        .iter()
        .filter(|p| p.zone.parcelled())
        .map(|p| p.land_value_per_m2.0)
        .collect();
    if v.is_empty() {
        return magnat_core::Money(0);
    }
    v.sort_unstable();
    magnat_core::Money(v[v.len() / 2])
}

/// Odcisk warstwy M2c. Kolejność jest kolejnością tablic, a te powstają deterministycznie
/// (00 §3.6) — dwa przebiegi tego samego ziarna muszą dać ten sam hash.
#[must_use]
pub fn city_hash(
    blocks: &BlockSet,
    zones: &ZoneResult,
    districts: &DistrictSet,
    parcels: &ParcelSet,
    buildings: &build::BuildingSet,
) -> StateHash {
    let mut h = StateHasher::new();
    h.write_u32(blocks.blocks.len() as u32);
    for (i, b) in blocks.blocks.iter().enumerate() {
        h.write_u32(b.area_m2);
        h.write_u16(b.district.0);
        h.write_u16(b.neighborhood);
        h.write_u8(zones.zone[i].index() as u8);
        h.write_u8(zones.epoch_ring[i]);
        h.write_u32(b.parcels.start);
        h.write_u32(b.parcels.end);
    }
    h.write_u32(districts.districts.len() as u32);
    for d in &districts.districts {
        for b in d.name.as_bytes() {
            h.write_u8(*b);
        }
        h.write_u8(d.kind as u8);
        h.write_u8(d.founded_epoch.0);
        h.write_u16(d.style.0);
        h.write_u8(d.income_tier);
        h.write_u8(d.reputation.get());
        h.write_u8(d.crime.get());
        h.write_u32(d.pop_capacity);
    }
    h.write_u32(parcels.parcels.len() as u32);
    for p in &parcels.parcels {
        h.write_u32(p.block.0);
        h.write_u16(p.district.0);
        h.write_u32(p.area_m2);
        h.write_u8(p.zone.index() as u8);
        h.write_u32(p.frontage.seg.0);
        h.write_u32(p.frontage.t0.to_bits());
        h.write_u32(p.frontage.t1.to_bits());
        h.write_i64(p.land_value_per_m2.0);
    }
    // ── M2d: zabudowa i wnętrza ─────────────────────────────────────────────────────
    h.write_u32(buildings.buildings.len() as u32);
    for b in &buildings.buildings {
        h.write_u32(b.parcel.0.index());
        h.write_u16(b.grammar.0);
        h.write_u8(b.epoch.0);
        h.write_u8(b.floors);
        h.write_u8(b.basements);
        h.write_u16(b.height_dm);
        h.write_u32(b.gross_area_m2);
        h.write_u8(b.condition.get());
        for fh in &b.floor_heights_dm {
            h.write_u16(*fh);
        }
        h.write_u32(b.entrances.len() as u32);
        for e in &b.entrances {
            h.write_u32(e.seg.0);
            h.write_u32(e.t.to_bits());
            h.write_u8(e.kind as u8);
        }
    }
    h.write_u32(buildings.units.len() as u32);
    for u in &buildings.units {
        h.write_u32(u.building.0.index());
        h.write_i64(i64::from(u.floor));
        h.write_u16(u.area_m2);
        h.write_i64(u.rent_hint.0);
        h.write_u8(match u.kind {
            UnitKind::Dwelling { rooms } => 16 + rooms,
            UnitKind::Retail => 1,
            UnitKind::Office => 2,
            UnitKind::Workshop => 3,
            UnitKind::Storage => 4,
            UnitKind::Common => 5,
        });
    }
    h.write_u32(buildings.workplaces.len() as u32);
    for w in &buildings.workplaces {
        h.write_u32(w.unit.0);
        h.write_u16(w.role.0);
        h.write_u8(w.shift as u8);
        h.write_i64(w.wage_band.median.0);
    }
    h.finish()
}

/// Odcisk całej warstwy M2 (D1 i D5 z §7 fazy, dok. 00 §3.6).
///
/// Buduje się na `city_hash`, a nie obok niego (korekta F5): tamten obejmuje już strefy,
/// dzielnice, parcele, budynki, lokale, stanowiska i `land_value_per_m2`. Tu dochodzi
/// Etap 7 — firmy, zakłady i wynik domknięcia łańcuchów.
#[must_use]
pub fn world_hash_m2(city: &StateHash, sites: &sites::SiteSet) -> StateHash {
    let mut h = StateHasher::new();
    h.write_u64(city.0 as u64);
    h.write_u64((city.0 >> 64) as u64);
    h.write_u32(sites.firms.len() as u32);
    for f in &sites.firms {
        for b in f.name.as_bytes() {
            h.write_u8(*b);
        }
        h.write_u8(f.sector as u8);
        h.write_u32(f.sites.len() as u32);
    }
    h.write_u32(sites.sites.len() as u32);
    for s in &sites.sites {
        h.write_u32(s.firm.0.index());
        h.write_u32(s.building.0.index());
        h.write_u32(s.parcel.0.index());
        h.write_u16(s.archetype.0);
        h.write_u16(s.capacity_scale);
        h.write_u32(s.units.start);
        h.write_u32(s.units.end);
        h.write_u32(s.workplaces.start);
        h.write_u32(s.workplaces.end);
        for r in &s.recipes {
            h.write_u16(r.0);
        }
    }
    // Wynik domknięcia wchodzi do odcisku, bo zmienia `capacity_scale` — a to jest stan
    // trwały, z którego M6 wyprowadzi zdolności produkcyjne.
    for (k, t) in &sites.closure.imported {
        for b in k.as_bytes() {
            h.write_u8(*b);
        }
        h.write_i64(*t);
    }
    h.finish()
}

/// Liczba segmentów spełniających warunek — raport mówi o **gotowej** sieci,
/// a nie o tym, co powstało przed przycięciem wiszących końców.
fn policz(net: &RoadNetwork, f: impl Fn(&RoadSegment) -> bool) -> u32 {
    net.segments.iter().filter(|s| f(s)).count() as u32
}

/// Domknięcie sieci: przycięcie wiszących końców, wybór głównej składowej, kompaktowanie
/// tablic, CSR sąsiedztwa i mała architektura.
///
/// **Przycinamy wszystkie** wiszące końce, nie tylko klasy ≥ Collector z kryterium WP4.
/// Powód jest geometryczny: wisząca krawędź wewnątrz ściany grafu robi w niej szczelinę
/// o zerowej szerokości, której odsunięcie kwartału (WP6) nie ma jak obsłużyć. Sięgacze
/// (`cul-de-sac`) wracają w M2c, gdzie powstają świadomie przy podziale kwartału.
fn finalize(
    grown: lsystem::Grown,
    gp: &gates::GatePlan,
    t: &dyn TerrainQuery,
) -> (RoadNetwork, lsystem::LStats) {
    let lsystem::Grown {
        nodes,
        segments,
        geom,
        gate_nodes,
        mut stats,
    } = grown;

    let mut alive = vec![true; segments.len()];
    let is_gate =
        |n: NodeId, nodes: &[RoadNode]| nodes[n.0 as usize].flags.contains(NodeFlags::GATE);

    // 1. Przycinanie wiszących końców — do punktu stałego.
    loop {
        let mut deg = vec![0u32; nodes.len()];
        for (i, s) in segments.iter().enumerate() {
            if alive[i] {
                deg[s.a.0 as usize] += 1;
                deg[s.b.0 as usize] += 1;
            }
        }
        let mut zmiana = false;
        for (i, s) in segments.iter().enumerate() {
            if !alive[i] {
                continue;
            }
            let a_leaf = deg[s.a.0 as usize] == 1 && !is_gate(s.a, &nodes);
            let b_leaf = deg[s.b.0 as usize] == 1 && !is_gate(s.b, &nodes);
            if a_leaf || b_leaf {
                alive[i] = false;
                stats.pruned += 1;
                zmiana = true;
            }
        }
        if !zmiana {
            break;
        }
    }

    // 2. Największa składowa spójna — reszta to wyspy, do których nie da się dojechać.
    let mut comp = vec![u32::MAX; nodes.len()];
    let mut sizes: Vec<u32> = Vec::new();
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); nodes.len()];
    for (i, s) in segments.iter().enumerate() {
        if alive[i] {
            adj[s.a.0 as usize].push(i as u32);
            adj[s.b.0 as usize].push(i as u32);
        }
    }
    let mut stos = Vec::new();
    for start in 0..nodes.len() {
        if comp[start] != u32::MAX || adj[start].is_empty() {
            continue;
        }
        let id = sizes.len() as u32;
        let mut n = 0u32;
        comp[start] = id;
        stos.push(start);
        while let Some(v) = stos.pop() {
            n += 1;
            for &si in &adj[v] {
                let s = &segments[si as usize];
                let o = if s.a.0 as usize == v {
                    s.b.0 as usize
                } else {
                    s.a.0 as usize
                };
                if comp[o] == u32::MAX {
                    comp[o] = id;
                    stos.push(o);
                }
            }
        }
        sizes.push(n);
    }
    let glowna = sizes
        .iter()
        .enumerate()
        .max_by_key(|(i, n)| (**n, std::cmp::Reverse(*i)))
        .map_or(u32::MAX, |(i, _)| i as u32);
    for (i, s) in segments.iter().enumerate() {
        if alive[i] && comp[s.a.0 as usize] != glowna {
            alive[i] = false;
            stats.pruned += 1;
        }
    }

    // 3. Kompaktowanie: nowe indeksy węzłów i segmentów, świeża arena geometrii.
    let mut node_map = vec![u32::MAX; nodes.len()];
    let mut new_nodes: Vec<RoadNode> = Vec::new();
    let mut new_segments: Vec<RoadSegment> = Vec::new();
    let mut new_geom = PolyArena::new();
    for (i, s) in segments.iter().enumerate() {
        if !alive[i] {
            continue;
        }
        let mut przepisz = |n: NodeId, new_nodes: &mut Vec<RoadNode>| -> NodeId {
            let slot = &mut node_map[n.0 as usize];
            if *slot == u32::MAX {
                *slot = new_nodes.len() as u32;
                let mut nn = nodes[n.0 as usize];
                nn.degree = 0;
                nn.flags = NodeFlags(nn.flags.0 & NodeFlags::GATE.0);
                new_nodes.push(nn);
            }
            NodeId(*slot)
        };
        let a = przepisz(s.a, &mut new_nodes);
        let b = przepisz(s.b, &mut new_nodes);
        let mut ns = *s;
        ns.a = a;
        ns.b = b;
        ns.geom = new_geom.push(geom.get(s.geom));
        new_nodes[a.0 as usize].degree = new_nodes[a.0 as usize].degree.saturating_add(1);
        new_nodes[b.0 as usize].degree = new_nodes[b.0 as usize].degree.saturating_add(1);
        new_segments.push(ns);
    }

    // 4. Flagi węzłów i CSR sąsiedztwa.
    for n in &mut new_nodes {
        if n.degree >= 3 {
            n.flags = n.flags.with(NodeFlags::JUNCTION);
        } else if n.degree == 1 {
            n.flags = n.flags.with(NodeFlags::DEAD_END);
        }
    }
    let mut adj_start = vec![0u32; new_nodes.len() + 1];
    for s in &new_segments {
        adj_start[s.a.0 as usize + 1] += 1;
        adj_start[s.b.0 as usize + 1] += 1;
    }
    for i in 1..adj_start.len() {
        adj_start[i] += adj_start[i - 1];
    }
    let mut kursor = adj_start.clone();
    let mut adj_items = vec![SegmentId(0); adj_start[new_nodes.len()] as usize];
    for (i, s) in new_segments.iter().enumerate() {
        for end in [s.a, s.b] {
            let k = &mut kursor[end.0 as usize];
            adj_items[*k as usize] = SegmentId(i as u32);
            *k += 1;
        }
    }

    // 5. Bramy: uchwyt do węzła po przenumerowaniu. Brama, której węzeł nie przetrwał
    //    domknięcia, wypada z sieci — i widać to w raporcie, bo lista jest krótsza.
    let mut city_gates = Vec::new();
    for (g, n) in gp.gates.iter().zip(gate_nodes.iter()) {
        let Some(n) = n else { continue };
        let new = node_map[n.0 as usize];
        if new == u32::MAX {
            continue;
        }
        city_gates.push(CityGate {
            kind: g.kind,
            pos: g.pos,
            dir_inward: g.dir_inward,
            capacity: g.kind.capacity(),
            node: NodeId(new),
        });
    }

    let furniture = place_furniture(&new_nodes, &new_segments, t);

    (
        RoadNetwork {
            nodes: new_nodes,
            segments: new_segments,
            adj_start,
            adj_items,
            geom: new_geom,
            gates: city_gates,
            furniture,
        },
        stats,
    )
}

/// Latarnie wzdłuż osi, naprzemiennie po obu stronach — wejście dla M11 (M2 §6).
fn place_furniture(
    nodes: &[RoadNode],
    segments: &[RoadSegment],
    t: &dyn TerrainQuery,
) -> Vec<StreetFurniture> {
    let mut out = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        let spacing = f32::from(road::spec(s.class).lamp_spacing_m);
        if spacing <= 0.0 || !matches!(s.structure, road::RoadStructure::AtGrade) {
            continue;
        }
        let (a, b) = (nodes[s.a.0 as usize].pos, nodes[s.b.0 as usize].pos);
        let len = (b - a).length();
        let d = (b - a).normalize_or_zero();
        let bok = Vec2::new(-d.y, d.x) * (f32::from(s.row_m) * 0.5 - 1.5);
        let n = (len / spacing) as u32;
        for k in 0..n {
            let t_param = (k as f32 + 0.5) * spacing / len;
            let strona = if k % 2 == 0 { 1.0 } else { -1.0 };
            let p = a + (b - a) * t_param + bok * strona;
            // `height_at` jest w jednostkach 0,5 m (K-13).
            let z = t.height_at(p.x as i32, p.y as i32) as f32 * 0.5;
            out.push(StreetFurniture {
                seg: SegmentId(i as u32),
                t: t_param,
                pos: glam::Vec3::new(p.x, p.y, z),
                kind: FurnitureKind::StreetLamp,
            });
        }
    }
    out
}

/// Sanity-check klas dróg w jednym miejscu — używany przez testy spójności.
#[must_use]
pub fn dangling_high_class(net: &RoadNetwork) -> Vec<NodeId> {
    net.nodes
        .iter()
        .enumerate()
        .filter(|(i, n)| {
            n.degree == 1
                && !n.flags.contains(NodeFlags::GATE)
                && net
                    .segments_at(NodeId(*i as u32))
                    .iter()
                    .any(|s| net.segments[s.0 as usize].class.rank() >= RoadClass::Collector.rank())
        })
        .map(|(i, _)| NodeId(i as u32))
        .collect()
}

/// Składowe spójne sieci jezdnej: ile ich jest, ile segmentów ma największa i ile
/// jest segmentów jezdnych razem. Diagnostyka T1 — sama odpowiedź „nie jest spójna"
/// nie mówi, czy odpadł kwartał, czy pojedynczy ślepy zaułek.
#[must_use]
pub fn skladowe_jezdne(net: &RoadNetwork) -> (u32, u32, u32) {
    let jezdny = |s: &RoadSegment| s.class.is_driveable() && !s.flags.contains(RoadFlags::RAIL);
    let ile_jezdnych = net.segments.iter().filter(|s| jezdny(s)).count() as u32;
    let mut seen = vec![false; net.nodes.len()];
    let mut skladowych = 0u32;
    let mut najwieksza = 0u32;
    for start in 0..net.nodes.len() {
        if seen[start]
            || !net
                .segments_at(NodeId(start as u32))
                .iter()
                .any(|s| jezdny(&net.segments[s.0 as usize]))
        {
            continue;
        }
        skladowych += 1;
        let mut n = 0u32;
        let mut stos = vec![NodeId(start as u32)];
        seen[start] = true;
        while let Some(v) = stos.pop() {
            for &s in net.segments_at(v) {
                if !jezdny(&net.segments[s.0 as usize]) {
                    continue;
                }
                n += 1;
                let o = net.other_end(s, v);
                if !seen[o.0 as usize] {
                    seen[o.0 as usize] = true;
                    stos.push(o);
                }
            }
        }
        najwieksza = najwieksza.max(n / 2);
    }
    (skladowych, najwieksza, ile_jezdnych)
}

/// Czy sieć jezdna jest jedną składową spójną (wstęp do testu T1 z M2 §7).
///
/// Liczone po węzłach **dotkniętych przez drogi jezdne**: od M2c w sieci siedzą też
/// tory, a te są osobną składową z definicji — pociąg nie skręca w ulicę.
#[must_use]
pub fn is_connected(net: &RoadNetwork) -> bool {
    let jezdny = |s: &RoadSegment| s.class.is_driveable() && !s.flags.contains(RoadFlags::RAIL);
    let mut w_sieci = vec![false; net.nodes.len()];
    for s in net.segments.iter().filter(|s| jezdny(s)) {
        w_sieci[s.a.0 as usize] = true;
        w_sieci[s.b.0 as usize] = true;
    }
    let ile = w_sieci.iter().filter(|x| **x).count();
    let Some(start) = w_sieci.iter().position(|x| *x) else {
        return true;
    };
    let mut seen = vec![false; net.nodes.len()];
    let mut stos = vec![NodeId(start as u32)];
    seen[start] = true;
    let mut n = 1;
    while let Some(v) = stos.pop() {
        for &s in net.segments_at(v) {
            if !jezdny(&net.segments[s.0 as usize]) {
                continue;
            }
            let o = net.other_end(s, v);
            if !seen[o.0 as usize] {
                seen[o.0 as usize] = true;
                n += 1;
                stos.push(o);
            }
        }
    }
    n == ile
}
