//! Siatka froxeli i bufory świateł (M1 §5.8, WP-R4).
//!
//! Sam podział przestrzeni jest tutaj, bo jest wspólny dla trzech miejsc, które muszą się
//! zgadzać co do jednego indeksu: compute przypisujący światła, shader fragmentu czytający
//! listę i `ClusterOccupancy` liczący histogram. Trzy kopie tej samej arytmetyki rozjeżdżają
//! się przy pierwszej zmianie wymiarów siatki, a objawem jest oświetlenie z sąsiedniego
//! klastra — czyli błąd, który wygląda jak problem z materiałem.

use magnat_devtools::MAX_LIGHTS_PER_CLUSTER;

/// Wymiary siatki froxeli (M1 §4, WP-R4: 16 × 9 × 24).
pub const CLUSTER_X: u32 = 16;
pub const CLUSTER_Y: u32 = 9;
pub const CLUSTER_Z: u32 = 24;

/// Liczba klastrów.
pub const CLUSTER_COUNT: u32 = CLUSTER_X * CLUSTER_Y * CLUSTER_Z;

/// Pojemność jednego klastra — tyle indeksów świateł mieści jego lista.
pub const CLUSTER_CAPACITY: u32 = MAX_LIGHTS_PER_CLUSTER as u32;

/// Zakres odległości obsługiwany przez klastry.
///
/// Światła punktowe w mieście mają zasięg rzędu kilkunastu metrów, więc poza kilkuset
/// metrami ich wkład jest poniżej progu widoczności, a froxele stają się tak długie,
/// że przypisanie przestaje cokolwiek odsiewać.
pub const CLUSTER_NEAR_M: f32 = 1.0;
pub const CLUSTER_FAR_M: f32 = 600.0;

/// Górna granica liczby świateł w klatce — ta sama, którą deklaruje snapshot M11.
pub const MAX_LIGHTS: usize = magnat_sim_snapshot::MAX_LIGHTS;

/// Światło w postaci, w jakiej trafia na GPU: pozycja **względem kamery**, bo cała
/// geometria renderera jest w tym układzie (`camera.rs`).
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    pub pos_range: [f32; 4],
    pub color: [f32; 4],
}

/// Uniform opisujący siatkę — układ musi odpowiadać `struct Clusters` w WGSL.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ClusterConfig {
    pub frustum: [f32; 4],
    pub dims: [u32; 4],
    pub light_count: [u32; 4],
}

impl ClusterConfig {
    /// `fov_deg` i `aspect` muszą pochodzić z tej samej klatki co macierz widoku —
    /// inaczej froxele opisują inny ostrosłup niż ten, który widać.
    #[must_use]
    pub fn new(fov_deg: f32, aspect: f32, lights: u32) -> ClusterConfig {
        let t = (fov_deg.to_radians() * 0.5).tan();
        ClusterConfig {
            frustum: [t * aspect, t, CLUSTER_NEAR_M, CLUSTER_FAR_M],
            dims: [CLUSTER_X, CLUSTER_Y, CLUSTER_Z, CLUSTER_CAPACITY],
            light_count: [lights.min(MAX_LIGHTS as u32), 0, 0, 0],
        }
    }
}

/// Zamienia rekord świetlny snapshotu na postać GPU: pozycja względem kamery,
/// barwa rozpakowana z RGBE.
#[must_use]
pub fn to_gpu(light: &magnat_sim_snapshot::LightRecord, eye: glam::DVec3) -> GpuLight {
    let [r, g, b] = light.color();
    GpuLight {
        pos_range: [
            light.pos[0] - eye.x as f32,
            light.pos[1] - eye.y as f32,
            light.pos[2] - eye.z as f32,
            light.range,
        ],
        color: [r, g, b, 0.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siatka_ma_wymiary_z_planu() {
        // §4 WP-R4 mówi wprost: froxele 16 × 9 × 24. Ta stała jest kontraktem z M11,
        // które dostanie budżet świateł policzony na tej siatce.
        assert_eq!((CLUSTER_X, CLUSTER_Y, CLUSTER_Z), (16, 9, 24));
        assert_eq!(CLUSTER_COUNT, 3456);
    }

    #[test]
    fn swiatlo_trafia_na_gpu_wzglednie_do_kamery() {
        let l = magnat_sim_snapshot::LightRecord {
            pos: [4000.0, 3000.0, 20.0],
            range: 15.0,
            color_rgbe: (128 << 24) | (255 << 16) | (255 << 8) | 255,
        };
        let g = to_gpu(&l, glam::DVec3::new(3990.0, 3000.0, 18.0));
        assert_eq!(g.pos_range, [10.0, 0.0, 2.0, 15.0]);
    }
}
