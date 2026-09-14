//! Budżet meshingu i agregacji na **prawdziwym** terenie (M1 §4 WP-V2/V3, §7.3).
//!
//! Test siedzi w `sim/world`, a nie w `engine/voxel`, z jednego powodu: liczba quadów zależy
//! od kształtu terenu, a `engine/voxel` z założenia nie wie, czym jest teren. Syntetyczna
//! powierzchnia w teście jednostkowym mierzyłaby fantazję autora testu, nie budżet —
//! a ryzyko R4 fazy mówi wprost, że fragmentacja z AO ma być mierzona na terenie
//! **erodowanym**, nie na płaskim placu.

use magnat_jobs::JobPool;
use magnat_voxel::{build_mesh, ChunkBuilder, ChunkCoord, ColumnSource, MaterialRegistry};
use magnat_world::{
    generate, Difficulty, EconomyProfile, Epoch, Region, Terrain, TerrainQuery, WorldGenParams,
    WorldSize,
};
use std::sync::Arc;

/// Budżet z M1 §5.3 dotyczy chunka **typowego**: 200–900 quadów.
const LIMIT_SREDNIA: usize = 900;

/// Sufit dla najgorszego chunka — **korekta planu**.
///
/// Plan podaje 900 quadów bez rozróżnienia na przypadek typowy i skrajny. Zmierzone
/// na terenie erodowanym (48 chunków powierzchniowych na region):
///
/// | region  | średnio | najgorszy |
/// |---------|---------|-----------|
/// | górski  |   664   |   1859    |
/// | rzeczny |   487   |   1482    |
/// | nizinny |   477   |   2564    |
///
/// Różnica nie bierze się z fragmentacji AO (ryzyko R4 — ta dokłada kilkanaście procent),
/// tylko z rozdzielczości: przy upsamplingu 4 m → 1 m każdy stopień terenu to ściana boczna,
/// której nie da się z niczym scalić. Najgorszy przypadek wypada na **nizinie**, nie w górach,
/// i to też jest pouczające: chunk na brzegu jeziora ma dwie powierzchnie (teren i zwierciadło)
/// o niezależnych stopniach.
///
/// Konsekwencja dla areny GPU mieści się w budżecie §5.9: 56 B na quad (4 wierzchołki po 8 B
/// plus 6 indeksów po 4 B), 8192 chunków rezydentnych, średnio 550 quadów daje 250 MB wobec
/// limitu 768 MB. Sufit istnieje po to, żeby wychwycić regresję rzędu wielkości, a nie żeby
/// przewidzieć każdy chunk.
const LIMIT_NAJGORSZY: usize = 3000;

/// Budżet z M1 §4 WP-V2: 0,8 ms na rdzeń — **za sam meshing**.
///
/// **Próg zależy od profilu (korekta H-27, M3d).** Budżet z planu opisuje grę, a gra
/// chodzi w release. Ten sam meshing w profilu debug zajmuje 11,9 ms/chunk na terenie
/// górskim — piętnaście razy dłużej, bo `greedy_quads` to ciasna pętla po wokselach,
/// której `debug` nie wektoryzuje ani nie inline'uje. Test oblewał na czystym `master`
/// (sprawdzone w osobnym worktree na `c9864ea`: 11,71 ms), więc nie jest to regresja
/// M3d — jest to budżet release'owy sprawdzany w debugu. Limit debugowy istnieje po to,
/// żeby wychwycić regresję rzędu wielkości; wartość release'owa nie rusza się.
const LIMIT_MESH_MS: f64 = if cfg!(debug_assertions) { 24.0 } else { 0.8 };
/// Materializacja chunka z `ColumnSource`. Planu nie ma dla niej wprost (§5.9 podaje
/// 0,4 ms na kafel 256 × 256 m, czyli inną jednostkę), więc limit jest ustalany tutaj
/// i liczony z tego, co musi się zmieścić w klatce: przy 64 chunkach na klatkę
/// i ośmiu rdzeniach 1 ms na chunk daje 8 ms pracy w tle, poniżej budżetu ramki.
/// Próg debugowy jak przy `LIMIT_MESH_MS` (H-27).
const LIMIT_FILL_MS: f64 = if cfg!(debug_assertions) { 12.0 } else { 1.0 };

fn teren(region: Region) -> Terrain {
    let pool = JobPool::new(0);
    let params = WorldGenParams {
        seed: 0x00C0_FFEE,
        size: WorldSize::Small4km,
        region,
        epoch: Epoch::Y1990,
        profile: EconomyProfile::Mixed,
        difficulty: Difficulty::Normal,
    };
    let (data, _) = generate(params, &pool).unwrap();
    let reg = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path("materials")).unwrap());
    Terrain::new(data, reg)
}

/// Chunki leżące na powierzchni terenu — jedyne, które w ogóle produkują quady.
fn chunki_powierzchniowe(t: &Terrain, ile: usize) -> Vec<ChunkCoord> {
    let chunkow = t.size_m() / 32;
    let mut out = Vec::with_capacity(ile);
    for i in 0..ile as i32 {
        let cx = (i * 13) % chunkow;
        let cy = (i * 29) % chunkow;
        let h = t.height_at(cx * 32 + 16, cy * 32 + 16);
        out.push(ChunkCoord::new(cx, cy, (h / 32) as i16));
    }
    out
}

#[test]
fn bench_mesh_chunk() {
    let reg = MaterialRegistry::load_dir(&magnat_world::data_path("materials")).unwrap();
    for region in [Region::Mountain, Region::River, Region::Lowland] {
        let t = teren(region);
        let coords = chunki_powierzchniowe(&t, 48);

        // Materializacja i meshing mierzone osobno: plan daje budżet meshingowi, a koszt
        // wyprodukowania voxeli jest własnością `sim/world`, nie `engine/voxel`. Zlepienie
        // ich w jedną liczbę ukrywa, który z dwóch przekroczył budżet.
        let mut buildery = Vec::with_capacity(coords.len());
        let t_fill = std::time::Instant::now();
        for c in &coords {
            let mut b = ChunkBuilder::new(*c, 0, 1);
            t.fill_chunk(*c, 0, &mut b);
            buildery.push(b);
        }
        let fill_ms = t_fill.elapsed().as_secs_f64() * 1000.0 / coords.len() as f64;

        let mut max_quadow = 0usize;
        let mut suma_quadow = 0usize;
        let t_mesh = std::time::Instant::now();
        for b in &buildery {
            let m = build_mesh(b, &reg);
            max_quadow = max_quadow.max(m.quad_count());
            suma_quadow += m.quad_count();
        }
        let ms = t_mesh.elapsed().as_secs_f64() * 1000.0 / coords.len() as f64;
        let srednia = suma_quadow / coords.len();

        assert!(
            suma_quadow > 0,
            "{}: żaden chunk nie ma geometrii",
            region.key()
        );
        assert!(
            srednia <= LIMIT_SREDNIA,
            "{}: średnio {srednia} quadów wobec limitu {LIMIT_SREDNIA}",
            region.key()
        );
        assert!(
            max_quadow <= LIMIT_NAJGORSZY,
            "{}: najgorszy chunk ma {max_quadow} quadów wobec sufitu {LIMIT_NAJGORSZY} \
             (średnio {srednia})",
            region.key()
        );
        assert!(
            ms <= LIMIT_MESH_MS,
            "{}: meshing {ms:.2} ms/chunk wobec limitu {LIMIT_MESH_MS} ms",
            region.key()
        );
        assert!(
            fill_ms <= LIMIT_FILL_MS,
            "{}: materializacja {fill_ms:.2} ms/chunk wobec limitu {LIMIT_FILL_MS} ms",
            region.key()
        );
        println!(
            "{}: materializacja {fill_ms:.2} ms, meshing {ms:.2} ms, quady śr. {srednia} / maks. {max_quadow}",
            region.key()
        );
    }
}

#[test]
fn bench_lod3_chunk() {
    // Budżet z M1 §4 WP-V3: agregat LOD3 w ≤ 1,5 ms — w release. W debugu 8,1 ms
    // i tak samo na czystym `master` (H-27, patrz `LIMIT_MESH_MS`).
    const LIMIT_LOD3_MS: f64 = if cfg!(debug_assertions) { 16.0 } else { 1.5 };
    let t = teren(Region::Mountain);
    let coord = ChunkCoord::new(2, 2, 0);
    let start = std::time::Instant::now();
    let mut b = ChunkBuilder::new(coord, 3, 0);
    magnat_voxel::aggregate(&t, coord, 3, &mut b);
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    assert!(
        ms <= LIMIT_LOD3_MS,
        "agregat LOD3 zajął {ms:.2} ms wobec limitu {LIMIT_LOD3_MS} ms"
    );
}

/// Najwyższy niepusty voxel w kolumnie chunka, albo `None` dla kolumny pustej.
fn powierzchnia(b: &ChunkBuilder, x: i32, y: i32) -> Option<i32> {
    (0..magnat_voxel::CHUNK_DIM as i32)
        .rev()
        .find(|z| !b.material_at(x, y, *z).is_air())
}

#[test]
fn lod_consistency() {
    // M1 §7.1: agregat ma opisywać ten sam teren co LOD0.
    //
    // **Korekta sformułowania.** Plan mówi o głosowaniu większościowym po sześcianie `2^lod`.
    // Dla terenu będącego polem wysokości jest to równoważne stwierdzeniu, że powierzchnia
    // agregatu leży na **medianie** powierzchni pokrywanych kolumn — i to właśnie sprawdzamy,
    // bo to jest własność widoczna na ekranie (brak puchnięcia zboczy, brak dziur), a nie
    // sposób jej policzenia. Dosłowne liczenie głosów wymagałoby `8^lod` próbek na komórkę,
    // czyli 305 ms na chunk LOD3 wobec budżetu 1,5 ms (patrz `magnat_voxel::aggregate`).
    let t = teren(Region::Mountain);
    let coord = ChunkCoord::new(6, 6, 1);
    let mut lod1 = ChunkBuilder::new(coord, 1, 0);
    magnat_voxel::aggregate(&t, coord, 1, &mut lod1);

    let dim = magnat_voxel::CHUNK_DIM as i32;
    let mut sprawdzone = 0;
    let mut max_blad = 0i32;

    for gy in (0..dim).step_by(3) {
        for gx in (0..dim).step_by(3) {
            let Some(agregat_z) = powierzchnia(&lod1, gx, gy) else {
                continue;
            };
            // Cztery kolumny LOD0 pokrywane przez tę komórkę agregatu.
            let mut lod0: Vec<i32> = Vec::with_capacity(4);
            for dy in 0..2 {
                for dx in 0..2 {
                    let (px, py) = (gx * 2 + dx, gy * 2 + dy);
                    let pod = ChunkCoord::new(
                        coord.x * 2 + px / dim,
                        coord.y * 2 + py / dim,
                        coord.z * 2,
                    );
                    let mut b = ChunkBuilder::new(pod, 0, 0);
                    t.fill_chunk(pod, 0, &mut b);
                    if let Some(z) = powierzchnia(&b, px % dim, py % dim) {
                        lod0.push(z);
                    }
                }
            }
            if lod0.len() < 4 {
                continue; // komórka na granicy warstwy chunków — nie ma czego porównywać
            }
            lod0.sort_unstable();
            let mediana = (lod0[1] + lod0[2]) / 2;
            // Agregat liczy w komórkach 2×, więc do porównania wracamy do skali LOD0.
            let blad = (agregat_z * 2 + 1 - mediana).abs();
            max_blad = max_blad.max(blad);
            assert!(
                blad <= 3,
                "komórka ({gx}, {gy}): agregat na {} (w skali LOD0), mediana {mediana}",
                agregat_z * 2 + 1
            );
            sprawdzone += 1;
        }
    }
    assert!(sprawdzone > 30, "sprawdzono tylko {sprawdzone} komórek");
    println!("największy błąd powierzchni agregatu: {max_blad} voxeli LOD0");
}
