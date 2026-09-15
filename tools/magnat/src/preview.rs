//! Podgląd świata w oknie: daleki teren, scena świateł i formatowanie raportu z przelotu.

use magnat_render::CameraMode;

/// Mapa dalekiego terenu: wysokości w decymetrach i kolor powierzchni na siatce roboczej.
///
/// Kolor bierze się z **klasy wody i biomu klimatu**, a nie z `biome_at`: to drugie liczy
/// szum przesunięcia granicy dla każdego punktu, a tutaj punktów jest kilka milionów i nikt
/// ich nie ogląda z bliska. Różnicy nie widać z kilometra, a generacja skraca się z sekund
/// do milisekund.
pub(crate) fn mapa_dalekiego_terenu(terrain: &magnat_world::Terrain) -> (u32, Vec<i16>, Vec<u8>) {
    use magnat_core::Biome;
    use magnat_world::WaterClass;

    let dane = terrain.data();
    let dim = dane.height.dim();
    let cdim = dane.climate.dim();
    let mut wysokosci = Vec::with_capacity(dim * dim);
    let mut kolor = Vec::with_capacity(dim * dim * 4);
    // Komórka klimatu ma 256 m, robocza 4 m — stąd przelicznik między siatkami.
    let na_klimat = (magnat_world::CLIMATE_CELL_M / magnat_world::WORK_CELL_M) as usize;

    for gy in 0..dim {
        for gx in 0..dim {
            let i = gy * dim + gx;
            wysokosci.push(dane.height[i]);
            let biom = match dane.water[i].class() {
                WaterClass::Sea => Biome::Sea,
                WaterClass::Lake => Biome::Lake,
                WaterClass::River => Biome::River,
                WaterClass::Dry => {
                    let c = (gy / na_klimat).min(cdim - 1) * cdim + (gx / na_klimat).min(cdim - 1);
                    dane.climate[c].biome
                }
            };
            let (r, g, b) = barwa_biomu(biom);
            kolor.extend_from_slice(&[r, g, b, 255]);
        }
    }
    (dim as u32, wysokosci, kolor)
}

/// Barwa powierzchni dla dalekiego terenu. Te same wartości co albedo materiałów pokrywy
/// z `data/materials/terrain.ron` — gdyby się rozjechały, granica między terenem voxelowym
/// a panoramą byłaby widoczna jako zmiana koloru w poprzek horyzontu.
fn barwa_biomu(biom: magnat_core::Biome) -> (u8, u8, u8) {
    use magnat_core::Biome;
    match biom {
        Biome::Sea | Biome::Lake | Biome::River => (40, 90, 130),
        Biome::Sand => (196, 178, 132),
        Biome::Rock => (124, 120, 118),
        Biome::Snow => (238, 242, 248),
        Biome::Marsh => (58, 44, 32),
        _ => (86, 124, 62),
    }
}

/// Światła testowe do sceny pomiarowej WP-R4.
///
/// Rozrzucone po siatce z przesunięciem z RNG świata, a nie losowo przy każdym starcie:
/// pomiar przypisania świateł ma być powtarzalny, bo inaczej porównanie dwóch commitów
/// mierzy różnicę w rozkładzie, a nie w kodzie.
pub(crate) fn swiatla_testowe(
    ile: usize,
    terrain: &magnat_world::Terrain,
    cel: glam::DVec3,
) -> Vec<magnat_sim_snapshot::LightRecord> {
    use magnat_core::{rng, StreamId, Tick, NO_ENTITY};
    use magnat_world::TerrainQuery;
    let mut r = rng(0xC0FFEE, StreamId::WorldDetail, NO_ENTITY, Tick(1));
    let bok = (ile as f64).sqrt().ceil() as i32;
    // Rozstaw i zasięg jak przy ulicznych latarniach: co 25 m, świecą na 12 m dookoła.
    let rozstaw = 25.0;
    let mut out = Vec::with_capacity(ile);
    for i in 0..ile as i32 {
        let (gx, gy) = (i % bok, i / bok);
        let x = cel.x + f64::from(gx - bok / 2) * rozstaw + f64::from(r.next_u32() % 8);
        let y = cel.y + f64::from(gy - bok / 2) * rozstaw + f64::from(r.next_u32() % 8);
        let z = f64::from(terrain.height_at(x as i32, y as i32)) * 0.5 + 6.0;
        out.push(magnat_sim_snapshot::LightRecord {
            pos: [x as f32, y as f32, z as f32],
            range: 12.0,
            // Ciepła barwa latarni. Wykładnik 128 + 3 daje mnożnik 8/256, czyli 1/32 —
            // latarnia świeci ułamkiem mocy słońca, a nie tyle samo co ono.
            color_rgbe: ((128 + 3) << 24) | (255 << 16) | (180 << 8) | 90,
        });
    }
    out
}

/// Czasy passów GPU do jednej linijki raportu (§7.4).
pub(crate) fn czasy_passow(s: &magnat_render::FrameStats) -> String {
    if !s.gpu_timing {
        return "brak pomiaru (sterownik bez TIMESTAMP_QUERY)".to_string();
    }
    magnat_render::PASS_NAMES
        .iter()
        .zip(s.pass_ms)
        .map(|(n, ms)| format!("{n} {ms:.2} ms"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Kamera startowa: orbita nad środkiem mapy, z wysokości dającej widok dzielnicy.
pub(crate) fn startowa_kamera(dist: f32) -> magnat_render::CameraState {
    magnat_render::CameraState {
        mode: CameraMode::Orbit {
            target: glam::DVec3::ZERO,
            dist,
            yaw: 0.6,
            pitch: 0.55,
        },
        fov_deg: 35.0,
        ..magnat_render::CameraState::default()
    }
}
