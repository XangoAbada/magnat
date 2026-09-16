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

mod checks;
mod finalize;
mod hash;
mod report;

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

pub use checks::{dangling_high_class, is_connected, skladowe_jezdne};
use finalize::finalize;
pub use hash::{city_hash, world_hash_m2};
use hash::{mediana_wartosci, policz};
pub use report::GenerationReport;

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

/// Bilans otwarcia złóż miasta (`AP-1`).
///
/// Obejmuje **wszystkie** złoża w granicach świata, także te, pod którymi Etap 7 nie
/// postawił kopalni: M6 pyta o pozostałość po identyfikatorze, a nie po tym, czy ktoś
/// już kopie. Osobna funkcja, a nie pięć linii w `generate_city` — ta ostatnia jest
/// listą etapów generacji i ma nią zostać (rejestr długu R1, pozycja 24).
fn bilans_zloz(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
) -> std::sync::Arc<crate::deposit::DepositLedger> {
    let bok = plan.size.meters() as i32;
    std::sync::Arc::new(crate::deposit::DepositLedger::from_rows(
        t.deposits_in(magnat_core::IRect::from_size(
            magnat_core::IVec2::ZERO,
            bok,
            bok,
        ))
        .into_iter()
        .map(|id| {
            let d = t.deposit(id);
            (id, d.remaining(), d.reserves)
        }),
    ))
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
    /// Bilans wydobycia złóż, na których to miasto stanęło (`AP-1`).
    ///
    /// Tu, a nie w `WorldData`, z tego samego powodu, dla którego tu stoi `catalog`:
    /// czyta go most stawiający gospodarkę, a on widzi miasto, nie teren. Pisarz jest
    /// jeden — kopalnia przez `magnat_supply::Deposits` — a kto stoi na którym złożu,
    /// wie wyłącznie Etap 7 (`SiteSeed::deposit`).
    pub deposits: std::sync::Arc<crate::deposit::DepositLedger>,
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
        let dwelling_scale = build::capacity::rescale_dwellings(&mut buildings, plan.target_pop);
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
        let sites = sites::place::populate(
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
    build::capacity::recompute_pop_capacity(&mut districts, &parcel_set, &buildings);
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

    let deposits = bilans_zloz(plan, t);

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
        deposits,
        site_catalog,
        edits: edit_index,
        report,
    })
}
