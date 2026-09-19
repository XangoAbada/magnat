//! Budżet ścieżki klatki po stronie procesora (M11a §5.3, kryterium WP2).
//!
//! §5.3 daje na całość 1,5 ms przy 20 000 encji: filtr stożka ~0,10 ms, klasyfikacja
//! LOD ~0,08, sortowanie ~0,20, zapis instancji ~0,35. Ten benchmark mierzy je **razem**,
//! bo razem stoją w budżecie i razem je się przekracza — a rozbicie na cztery pomiary
//! dałoby cztery liczby, z których żadna nie odpowiada na pytanie „czy klatka się mieści".
//!
//! Scena jest najgorszym przypadkiem sortowania: encje są rozrzucone tak, żeby oba
//! poziomy detalu miały porównywalne obsadzenie i żeby nic nie odpadło na stożku.
//! Scena, w której wszystko wpada do jednego wsadu, mierzyłaby przypadek, którego
//! w kadrze ulicznym nie ma.

use criterion::{criterion_group, criterion_main, Criterion};
use glam::{DVec3, Vec4};
use magnat_render::instancing::{build_instances, InstanceScratch, LodBands, MeshSlot, ModelTable};
use magnat_sim_snapshot::{CitizenRenderRec, RenderSnapshot, SnapshotCaps, VehicleRenderRec};
use magnat_voxel::{ModelId, LOD_COUNT};
use std::hint::black_box;

fn stozek() -> [Vec4; 6] {
    [
        Vec4::new(1.0, 0.0, 0.0, 1.0e6),
        Vec4::new(-1.0, 0.0, 0.0, 1.0e6),
        Vec4::new(0.0, 1.0, 0.0, 1.0e6),
        Vec4::new(0.0, -1.0, 0.0, 1.0e6),
        Vec4::new(0.0, 0.0, 1.0, 1.0e6),
        Vec4::new(0.0, 0.0, -1.0, 1.0e6),
    ]
}

fn tablica() -> ModelTable {
    let slot = MeshSlot {
        index_offset: 0,
        index_count: 540,
        base_vertex: 0,
        radius_m: 1.2,
    };
    ModelTable::new(vec![slot; 2 * LOD_COUNT as usize], ModelId(0), ModelId(1))
}

/// Scena `bench_district` z §7.2: ok. 20 tys. widocznych mieszkańców i 6 tys. pojazdów.
fn scena() -> RenderSnapshot {
    let mut s = RenderSnapshot::new(SnapshotCaps::DEFAULT);
    for i in 0..20_000u32 {
        // Pierścień 5–300 m wokół kamery: połowa encji wypada powyżej progu L1 (60 m).
        let r = 5_000 + (i % 295) * 1_000;
        let kat = i % 360;
        s.citizens.push(CitizenRenderRec {
            pos: [
                (r as i32) * i32::from(kat as i16 % 7 - 3) / 3,
                (r as i32) * i32::from(kat as i16 % 5 - 2) / 2,
                0,
            ],
            entity_lo: i,
            appearance: i.wrapping_mul(2_654_435_761),
            district: (i % 54) as u16,
            ..Default::default()
        });
    }
    for i in 0..6_000u32 {
        let r = 4_000 + (i % 280) * 1_000;
        s.vehicles.push(VehicleRenderRec {
            pos: [(r as i32), (r as i32) / 2, 0],
            entity_lo: 1_000_000 + i,
            paint: (i % 64) as u16,
            district: (i % 54) as u16,
            ..Default::default()
        });
    }
    s
}

fn bench(c: &mut Criterion) {
    let snap = scena();
    let models = tablica();
    let frustum = stozek();
    let mut scratch = InstanceScratch::default();
    // Rozgrzewka: bufory robocze mają być już alokowane, bo w grze są — mierzymy
    // klatkę ustaloną, a nie pierwszą.
    build_instances(
        &snap,
        DVec3::ZERO,
        &frustum,
        &models,
        LodBands::default(),
        &mut scratch,
    );

    c.bench_function("m11a-1 kompakcja instancji 26k encji", |b| {
        b.iter(|| {
            build_instances(
                black_box(&snap),
                black_box(DVec3::new(100.0, 100.0, 30.0)),
                black_box(&frustum),
                black_box(&models),
                LodBands::default(),
                &mut scratch,
            );
            black_box(scratch.instances.len())
        });
    });
}

criterion_group!(instancing, bench);
criterion_main!(instancing);
