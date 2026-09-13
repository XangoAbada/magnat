//! Przebiegi generatora P1–P12 (M1 §5.6). Jeden moduł na przebieg — granica modułu
//! pokrywa się z granicą pakietu roboczego, więc widać, co jest zrobione, a co nie.

pub mod biome;
pub mod deposits;
pub mod erosion;
pub mod flood;
pub mod flow;
pub mod geology;
pub mod height;
pub mod landmask;
pub mod precip;
pub mod shape;
pub mod temperature;
pub mod uplift;
pub mod water;

use crate::pipeline::GenCtx;

/// Przepisanie wysokości roboczej (`f32`, metry) do stanu trwałego (`i16`, decymetry).
///
/// Wołane po P2 i ponownie po P6. Dwa razy, a nie raz na końcu, bo dzięki temu `world.height`
/// jest sensowne na każdym etapie potoku — podgląd i inspektor nie muszą wiedzieć, który
/// przebieg już poszedł. Kwantyzacja jest **jedynym** miejscem, w którym teren traci precyzję,
/// i dlatego jest funkcją, a nie linijką powtórzoną w dwóch przebiegach.
pub(crate) fn commit_height(ctx: &mut GenCtx) {
    use crate::gen::shape::{WORLD_MAX_M, WORLD_MIN_M};
    for i in 0..ctx.work.height_m.len() {
        let m = ctx.work.height_m[i].clamp(WORLD_MIN_M, WORLD_MAX_M);
        // Zaokrąglenie do najbliższego decymetra, symetryczne względem zera.
        let dm = if m >= 0.0 {
            (m * 10.0 + 0.5) as i32
        } else {
            (m * 10.0 - 0.5) as i32
        };
        ctx.world.height[i] = dm.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    }
}
