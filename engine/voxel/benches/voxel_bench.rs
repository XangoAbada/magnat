//! Benchmarki meshingu i LOD (M1 §7.3, WP-X1).
//!
//! Chunk jest **syntetyczny**, a nie wzięty z generatora, i to jest celowe: benchmark ma
//! mierzyć meshing, a nie generację terenu, która trwa sto razy dłużej i przesłoniłaby
//! wynik. Kształt powierzchni odpowiada temu, co mesher naprawdę dostaje — zbocze ze
//! stopniami i płaski fragment — bo płaski plac testowy scala się do jednego quada
//! i mierzyłby przypadek, którego w terenie nie ma (patrz korekta budżetu quadów z WP-V2).

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_voxel::{
    aggregate, build_mesh, ChunkBuilder, ChunkCoord, ColumnSource, MaterialId, MaterialRegistry,
    CHUNK_DIM,
};
use std::hint::black_box;

/// Powierzchnia testowa: zbocze o zmiennym spadku plus falowanie, czyli kształt, który
/// łamie scalanie zachłanne mniej więcej tak samo jak teren po erozji.
struct Zbocze {
    kamien: MaterialId,
    darn: MaterialId,
}

impl ColumnSource for Zbocze {
    fn fill_chunk(&self, coord: ChunkCoord, lod: u8, out: &mut ChunkBuilder) {
        let skala = 1i32 << lod;
        let origin = coord.origin_voxels(lod);
        let r = out.range();
        for ly in r.clone() {
            for lx in r.clone() {
                let (wx, wy) = (origin.x + lx * skala, origin.y + ly * skala);
                // Stopnie co kilka metrów i drobne falowanie — bez nich cały przekrój
                // wychodzi jednym prostokątem.
                let h = wx / 3 + (wy / 5) % 7 + ((wx * wy) % 11) / 4;
                let top = (h - origin.z) / skala;
                if top < 0 {
                    continue;
                }
                out.fill_column(lx, ly, *r.start(), top.min(CHUNK_DIM as i32), self.kamien);
                out.fill_column(lx, ly, top - 2, top + 1, self.darn);
            }
        }
    }
}

fn rejestr() -> MaterialRegistry {
    MaterialRegistry::load_dir(std::path::Path::new("../../data/materials"))
        .expect("brak katalogu materiałów")
}

fn meshing(c: &mut Criterion) {
    let reg = rejestr();
    let src = Zbocze {
        kamien: reg.expect_id("granite"),
        darn: reg.expect_id("grass"),
    };
    let coord = ChunkCoord::new(2, 3, 0);

    let mut g = c.benchmark_group("m1-3 meshing");
    g.bench_function("materializacja chunka", |b| {
        b.iter(|| {
            let mut builder = ChunkBuilder::new(coord, 0, 1);
            src.fill_chunk(coord, 0, &mut builder);
            black_box(builder.range());
        });
    });

    let mut builder = ChunkBuilder::new(coord, 0, 1);
    src.fill_chunk(coord, 0, &mut builder);
    g.bench_function("greedy + AO", |b| {
        b.iter(|| black_box(build_mesh(&builder, &reg).indices.len()));
    });
    g.finish();
}

fn lod(c: &mut Criterion) {
    let reg = rejestr();
    let src = Zbocze {
        kamien: reg.expect_id("granite"),
        darn: reg.expect_id("grass"),
    };
    let mut g = c.benchmark_group("m1-4 lod");
    for poziom in [1u8, 3] {
        g.bench_function(format!("agregat LOD{poziom}"), |b| {
            b.iter(|| {
                let coord = ChunkCoord::new(0, 0, 0);
                let mut builder = ChunkBuilder::new(coord, poziom, 1);
                aggregate(&src, coord, poziom, &mut builder);
                black_box(build_mesh(&builder, &reg).indices.len())
            });
        });
    }
    g.finish();
}

criterion_group!(benches, meshing, lod);
criterion_main!(benches);
