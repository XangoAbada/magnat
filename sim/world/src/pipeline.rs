//! Potok generacji — lista nazwanych przebiegów (M1 §4 WP-W1, §5.6).
//!
//! Potok jest **płaską listą funkcji**, nie drzewem obiektów: przebiegi nie mają stanu
//! własnego, a jedyne, co je różni, to nazwa, strumień RNG i ciało. Trait z trzynastoma
//! implementacjami bez pola byłby tu kosztem bez korzyści.
//!
//! Każdy przebieg ma **własny strumień RNG** (00 §3.1), więc dopisanie przebiegu nie
//! przesuwa losowań pozostałych — świat wygenerowany wcześniej z tego samego seeda
//! pozostaje ten sam wszędzie tam, gdzie nowy przebieg nie sięga.

use crate::data::WorldData;
use crate::fields::WorkFields;
use crate::params::{ParamError, WorldGenParams};
use magnat_core::hash::{StateHash, StateHasher};
use magnat_core::StreamId;
use magnat_jobs::JobPool;
use std::time::Instant;

/// Kontekst przebiegu: parametry, pula wątków, stan trwały i pola robocze.
pub struct GenCtx<'a> {
    pub params: WorldGenParams,
    pub pool: &'a JobPool,
    pub world: WorldData,
    /// Pola pomocnicze żyjące tylko na czas generacji (wypiętrzenie, spływ, stos odbiorników).
    /// Nie wchodzą do stanu trwałego ani do hasha — są w całości pochodną seeda.
    pub work: WorkFields,
}

impl<'a> GenCtx<'a> {
    #[must_use]
    pub fn new(params: WorldGenParams, pool: &'a JobPool) -> GenCtx<'a> {
        let n = params.work_dim();
        GenCtx {
            params,
            pool,
            world: WorldData::empty(params),
            work: WorkFields::new(n),
        }
    }

    /// Bok siatki roboczej.
    #[inline]
    #[must_use]
    pub fn dim(&self) -> usize {
        self.params.work_dim()
    }

    #[inline]
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.params.seed
    }
}

/// Jeden przebieg potoku.
pub struct GenPass {
    /// Etykieta w raporcie i w logu — `"P4 priority-flood"`.
    pub name: &'static str,
    /// Strumień RNG przebiegu; `None` dla przebiegów w pełni deterministycznych
    /// (wypełnianie zagłębień, akumulacja spływu — tam nie ma czego losować).
    pub stream: Option<StreamId>,
    pub run: fn(&mut GenCtx),
}

/// Czas i ślad jednego przebiegu.
#[derive(Clone, Debug)]
pub struct PassTiming {
    pub name: &'static str,
    pub millis: f64,
}

/// Raport z generacji. **Nie wchodzi do stanu świata ani do hasha** — czas zegarowy jest
/// jedyną wartością w tym crate'cie, która zależy od maszyny, i dlatego nie ma prawa
/// dotknąć niczego, co symulacja później czyta (00 §3.5).
#[derive(Clone, Debug, Default)]
pub struct WorldGenReport {
    pub timings: Vec<PassTiming>,
    pub total_millis: f64,
    pub terrain_hash: StateHash,
    pub stats: WorldStats,
}

/// Statystyki świata — pozycje wypisywane przez `headless generate` i sprawdzane w testach
/// poprawności generatora (M1 §7.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldStats {
    pub land_cells: u64,
    pub sea_cells: u64,
    pub lake_cells: u64,
    pub river_cells: u64,
    pub river_length_m: u64,
    pub deposits: u64,
    /// Suma zasobów wszystkich złóż w gramach.
    pub deposit_reserves_g: i64,
    pub min_height_dm: i16,
    pub max_height_dm: i16,
    pub persistent_bytes: usize,
}

impl WorldGenReport {
    /// Wiersze raportu w postaci gotowej do wypisania na konsolę.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .timings
            .iter()
            .map(|t| format!("{:<28} {:>9.1} ms", t.name, t.millis))
            .collect();
        out.push(format!("{:<28} {:>9.1} ms", "RAZEM", self.total_millis));
        out.push(format!("hash terenu                  {:032x}", self.terrain_hash.0));
        out.push(format!(
            "ląd/morze/jezioro/rzeka      {} / {} / {} / {}",
            self.stats.land_cells,
            self.stats.sea_cells,
            self.stats.lake_cells,
            self.stats.river_cells
        ));
        out.push(format!(
            "długość koryt                {} m",
            self.stats.river_length_m
        ));
        out.push(format!(
            "złoża                        {} szt., {} t surowca",
            self.stats.deposits,
            self.stats.deposit_reserves_g / 1_000_000
        ));
        out.push(format!(
            "wysokość min/max             {} / {} dm",
            self.stats.min_height_dm, self.stats.max_height_dm
        ));
        out.push(format!(
            "stan trwały                  {:.1} MB",
            self.stats.persistent_bytes as f64 / (1024.0 * 1024.0)
        ));
        out
    }
}

/// Kolejność przebiegów (M1 §5.6). Zmiana kolejności zmienia każdy wygenerowany świat,
/// więc lista jest kontraktem, a nie konfiguracją.
pub const PASSES: &[GenPass] = &[
    GenPass {
        name: "P1  maska lądu",
        stream: Some(StreamId::WorldLandmask),
        run: crate::gen::landmask::run,
    },
    GenPass {
        name: "P2  baza wysokości",
        stream: Some(StreamId::WorldHeightBase),
        run: crate::gen::height::run,
    },
    GenPass {
        name: "P3  wypiętrzenie i erodowalność",
        stream: Some(StreamId::WorldUplift),
        run: crate::gen::uplift::run,
    },
    GenPass {
        name: "P4  wypełnienie zagłębień",
        stream: None,
        run: crate::gen::flood::run,
    },
    GenPass {
        name: "P5  spływ i akumulacja",
        stream: None,
        run: crate::gen::flow::run,
    },
    GenPass {
        name: "P6  erozja",
        stream: None,
        run: crate::gen::erosion::run,
    },
    GenPass {
        name: "P7  klasyfikacja wód",
        stream: None,
        run: crate::gen::water::run,
    },
    GenPass {
        name: "P8  geologia",
        stream: Some(StreamId::WorldGeology),
        run: crate::gen::geology::run,
    },
    GenPass {
        name: "P9  złoża",
        stream: Some(StreamId::WorldDeposits),
        run: crate::gen::deposits::run,
    },
    GenPass {
        name: "P10 temperatura",
        stream: Some(StreamId::WorldClimate),
        run: crate::gen::temperature::run,
    },
    GenPass {
        name: "P11 wiatr i opady",
        stream: Some(StreamId::WorldWind),
        run: crate::gen::precip::run,
    },
    GenPass {
        name: "P12 biomy i żyzność",
        stream: Some(StreamId::WorldBiome),
        run: crate::gen::biome::run,
    },
];

/// Generacja świata. Jedyne wejście: `WorldGenParams`.
///
/// P13 (materializacja 1 m) **nie jest** częścią potoku — jest leniwa, per kafel,
/// i żyje w [`crate::column`] (M1 §5.6).
pub fn generate(
    params: WorldGenParams,
    pool: &JobPool,
) -> Result<(WorldData, WorldGenReport), ParamError> {
    params.validate()?;
    let mut ctx = GenCtx::new(params, pool);
    let mut report = WorldGenReport::default();

    let t_total = Instant::now();
    for pass in PASSES {
        let t = Instant::now();
        (pass.run)(&mut ctx);
        report.timings.push(PassTiming {
            name: pass.name,
            millis: t.elapsed().as_secs_f64() * 1000.0,
        });
    }
    report.total_millis = t_total.elapsed().as_secs_f64() * 1000.0;

    let mut h = StateHasher::new();
    ctx.world.hash_state(&mut h);
    report.terrain_hash = h.finish();
    report.stats = collect_stats(&ctx.world);

    Ok((ctx.world, report))
}

fn collect_stats(w: &WorldData) -> WorldStats {
    use crate::data::WaterClass;
    let mut s = WorldStats {
        min_height_dm: i16::MAX,
        max_height_dm: i16::MIN,
        ..WorldStats::default()
    };
    // Iteracja po indeksach — kolejność deterministyczna (00 §3.2).
    for i in 0..w.height.len() {
        let h = w.height[i];
        s.min_height_dm = s.min_height_dm.min(h);
        s.max_height_dm = s.max_height_dm.max(h);
        match w.water[i].class() {
            WaterClass::Dry => s.land_cells += 1,
            WaterClass::Sea => s.sea_cells += 1,
            WaterClass::Lake => s.lake_cells += 1,
            WaterClass::River => s.river_cells += 1,
        }
    }
    s.river_length_m = w.rivers.total_length_m();
    s.deposits = w.deposits.len() as u64;
    s.deposit_reserves_g = w.deposits.iter().map(|d| d.reserves.0).sum();
    s.persistent_bytes = w.persistent_bytes();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kazdy_przebieg_ma_wlasny_strumien() {
        // Dwa przebiegi dzielące strumień losowałyby z tej samej sekwencji i zmiana
        // jednego przesuwałaby drugi — dokładnie to, czemu zapobiega 00 §3.1.
        let mut uzyte: Vec<u16> = PASSES.iter().filter_map(|p| p.stream.map(|s| s as u16)).collect();
        let ile = uzyte.len();
        uzyte.sort_unstable();
        uzyte.dedup();
        assert_eq!(uzyte.len(), ile, "strumień użyty w dwóch przebiegach");
        // Wszystkie w zakresie zarezerwowanym dla M1 (00 §K-4).
        assert!(uzyte.iter().all(|s| (100..=119).contains(s)), "{uzyte:?}");
    }

    #[test]
    fn raport_wypisuje_kazdy_przebieg() {
        let pool = JobPool::new(1);
        let (_, report) = generate(WorldGenParams::default(), &pool).unwrap();
        assert_eq!(report.timings.len(), PASSES.len());
        assert!(report.lines().len() > PASSES.len());
    }
}
