//! Budżety generatora — bramka 4 fazy M1 (M1 §5.9, §7.3).
//!
//! Budżet jest **asercją, nie sugestią**: przekroczenie pamięci na mapie 16 km nie objawia
//! się wolniejszą grą, tylko brakiem pamięci u gracza, który ma dokładnie tyle RAM-u,
//! ile deklaruje PRD. Dlatego progi są tutaj, a nie w komentarzu.
//!
//! Testy 16-kilometrowe są oznaczone `#[ignore]`, bo trwają ~9 s każdy. CI uruchamia je
//! jawnie (`--ignored`); przy zwykłym `cargo test` nie blokują pętli zwrotnej.

use magnat_jobs::JobPool;
use magnat_world::{
    generate, Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize,
};

/// Twardy limit CI dla mapy 4 km (M1 §5.9). Cel to 1,0 s, limit 2,5 s — mierzymy limit,
/// bo maszyna CI bywa wolniejsza od deweloperskiej i test ma łapać regresje rzędu wielkości,
/// a nie wahania obciążenia.
const LIMIT_4KM_MS: f64 = 2_500.0;
const LIMIT_16KM_MS: f64 = 20_000.0;
/// Stan trwały mapy 16 km w RAM (M1 §5.9: ≤ 60 MB).
const LIMIT_PERSISTENT_16KM: usize = 60 * 1024 * 1024;
/// Zapis `.mgw` mapy 16 km (M1 §4 WP-W7: ≤ 25 MB).
const LIMIT_MGW_16KM: usize = 25 * 1024 * 1024;

fn params(size: WorldSize, region: Region) -> WorldGenParams {
    WorldGenParams {
        seed: 7,
        size,
        region,
        epoch: Epoch::Y1990,
        profile: EconomyProfile::Mixed,
        difficulty: Difficulty::Normal,
    }
}

#[test]
fn bench_generate_4km() {
    let pool = JobPool::new(0);
    for region in Region::ALL {
        let (_, r) = generate(params(WorldSize::Small4km, *region), &pool).unwrap();
        assert!(
            r.total_millis < LIMIT_4KM_MS,
            "{}: {:.0} ms wobec limitu {LIMIT_4KM_MS:.0} ms",
            region.key(),
            r.total_millis
        );
    }
}

/// Świat, który nie ma rzek, jest bezużyteczny dla M2 (drogi omijają wodę, mosty potrzebują
/// koryt) — a taki błąd nie objawia się ani panice, ani przekroczeniem budżetu.
#[test]
fn kazdy_region_ma_siec_rzeczna() {
    let pool = JobPool::new(0);
    for region in Region::ALL {
        let (w, r) = generate(params(WorldSize::Small4km, *region), &pool).unwrap();
        assert!(
            r.stats.river_cells > 500,
            "{}: tylko {} komórek rzecznych",
            region.key(),
            r.stats.river_cells
        );
        assert!(
            !w.rivers.segments.is_empty(),
            "{}: sieć wektorowa pusta mimo komórek rzecznych",
            region.key()
        );
        if region.has_sea() {
            assert!(r.stats.sea_cells > 10_000, "{}: brak morza", region.key());
        }
    }
}

#[test]
#[ignore = "9 s na region — CI uruchamia jawnie przez --ignored"]
fn bench_generate_16km() {
    let pool = JobPool::new(0);
    for region in Region::ALL {
        let (_, r) = generate(params(WorldSize::Metropolis16km, *region), &pool).unwrap();
        assert!(
            r.total_millis < LIMIT_16KM_MS,
            "{}: {:.0} ms wobec limitu {LIMIT_16KM_MS:.0} ms",
            region.key(),
            r.total_millis
        );
    }
}

#[test]
#[ignore = "9 s na region — CI uruchamia jawnie przez --ignored"]
fn mem_world_persistent_16km() {
    let pool = JobPool::new(0);
    for region in Region::ALL {
        let (w, r) = generate(params(WorldSize::Metropolis16km, *region), &pool).unwrap();
        // Wypisane, nie tylko sprawdzone: raport budżetów z §7.4 ma pokazywać zapas,
        // a nie wyłącznie fakt, że limit nie pękł.
        println!(
            "{}: {:.1} MB stanu trwałego z {:.0} MB ({} komórek jezior)",
            region.key(),
            r.stats.persistent_bytes as f64 / (1024.0 * 1024.0),
            LIMIT_PERSISTENT_16KM as f64 / (1024.0 * 1024.0),
            r.stats.lake_cells,
        );
        assert!(
            r.stats.persistent_bytes <= LIMIT_PERSISTENT_16KM,
            "{}: {:.1} MB stanu trwałego wobec limitu {:.0} MB (jeziora: {} komórek)",
            region.key(),
            r.stats.persistent_bytes as f64 / (1024.0 * 1024.0),
            LIMIT_PERSISTENT_16KM as f64 / (1024.0 * 1024.0),
            r.stats.lake_cells
        );

        let dir = std::env::temp_dir().join("magnat-budget");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{}.mgw", region.key()));
        let bytes = magnat_world::save_mgw(&w, &path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(
            bytes <= LIMIT_MGW_16KM,
            "{}: zapis {:.1} MB wobec limitu {:.0} MB",
            region.key(),
            bytes as f64 / (1024.0 * 1024.0),
            LIMIT_MGW_16KM as f64 / (1024.0 * 1024.0)
        );
    }
}

#[test]
#[ignore = "9 s na region — CI uruchamia jawnie przez --ignored"]
fn bench_priority_flood_16km() {
    // §7.3: samo wypełnianie zagłębień ma zmieścić się w 1,5 s na mapie 16 km. To jest
    // przebieg, który najłatwiej zamienić w wąskie gardło (kolejka priorytetowa po
    // 16 mln komórek), więc ma własny próg, a nie tylko udział w sumie.
    const LIMIT_MS: f64 = 1500.0;
    let pool = JobPool::new(0);
    for region in Region::ALL {
        let (_, r) = generate(params(WorldSize::Metropolis16km, *region), &pool).unwrap();
        let flood = r
            .timings
            .iter()
            .find(|t| t.name.contains("P4"))
            .unwrap_or_else(|| panic!("{}: brak przebiegu P4 w raporcie", region.key()));
        println!(
            "{}: {} {:.0} ms z {LIMIT_MS:.0} ms",
            region.key(),
            flood.name,
            flood.millis
        );
        assert!(
            flood.millis < LIMIT_MS,
            "{}: {} zajęło {:.0} ms wobec limitu {LIMIT_MS:.0} ms",
            region.key(),
            flood.name,
            flood.millis
        );
    }
}

#[test]
fn mem_voxel_budget() {
    // §7.3: pula chunków ≤ 384 MB, arena GPU ≤ 768 MB. To asercja na **stałych**, nie na
    // pomiarze: budżet jest deklaracją, którą łatwo bezwiednie podnieść przy strojeniu
    // promieni LOD, a skutek — zabraknięcie pamięci na słabszej karcie — wychodzi dopiero
    // u gracza. Zajętość w biegu pilnuje `budzet_wywlaszcza_zamiast_rosnac` w `engine/voxel`.
    const LIMIT_PULI: usize = 384 * 1024 * 1024;
    let budzet = magnat_voxel::VoxelBudget::default();
    assert!(
        budzet.chunk_pool_bytes <= LIMIT_PULI,
        "pula chunków {} MB wobec limitu {} MB",
        budzet.chunk_pool_bytes / (1024 * 1024),
        LIMIT_PULI / (1024 * 1024)
    );
}
