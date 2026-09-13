//! Inspektor terenu (M1 §1 pkt 5, WP-W7).
//!
//! Klik w teren ma odpowiadać na pytanie „co tu jest?" **danymi generatora**, a nie tym,
//! co akurat widać na ekranie. Dlatego karta czyta wyłącznie `TerrainQuery` — ten sam
//! interfejs, z którego będzie korzystać M2. Gdyby inspektor sięgał do wnętrza `WorldData`,
//! pokazywałby rzeczy, których M2 nie może dostać, i przestałby być dowodem na to,
//! że kontrakt z §6.1 wystarcza.

use magnat_world::{Terrain, TerrainQuery};

/// Trafienie promienia w teren: punkt w metrach.
///
/// Marsz po promieniu ze stałym krokiem, nie przecięcie analityczne: teren jest funkcją
/// próbkowaną (interpolacja + szum + wcięcie koryta), więc nie ma postaci, którą dałoby się
/// przeciąć wzorem. Krok metrowy odpowiada rozdzielczości materializacji — drobniejszy
/// pokazywałby dokładność, której teren nie ma.
#[must_use]
pub fn trafienie(
    terrain: &Terrain,
    oko: glam::DVec3,
    kierunek: glam::DVec3,
    zasieg_m: f64,
) -> Option<(i32, i32)> {
    let krok = kierunek.normalize_or_zero();
    if krok.length_squared() < 0.5 {
        return None;
    }
    let mut t = 0.0;
    while t < zasieg_m {
        let p = oko + krok * t;
        let size = f64::from(terrain.size_m());
        if p.x < 0.0 || p.y < 0.0 || p.x >= size || p.y >= size {
            return None;
        }
        let h = f64::from(terrain.height_at(p.x as i32, p.y as i32)) * 0.5;
        if p.z <= h {
            return Some((p.x as i32, p.y as i32));
        }
        // Krok rośnie z odległością: przy 2 km od kamery metrowa dokładność jest poniżej
        // wielkości piksela, a marsz metrowy do takiego dystansu to dwa tysiące próbek.
        t += 1.0 + t / 400.0;
    }
    None
}

/// Karta inspekcji punktu terenu — dokładnie zakres z §1 pkt 5.
#[must_use]
pub fn karta(terrain: &Terrain, x: i32, y: i32) -> String {
    let mut out = String::new();
    let h_m = f64::from(terrain.height_at(x, y)) * 0.5;
    let klimat = terrain.climate_at(x, y);
    out.push_str(&format!(
        "── inspekcja ({x} m, {y} m) ──\nwysokość {h_m:.1} m n.p.m. · nachylenie {} · biom {:?}\n",
        terrain.slope_at(x, y),
        terrain.biome_at(x, y)
    ));

    let woda = terrain.water_at(x, y);
    out.push_str(&format!(
        "woda: {:?}, głębokość {:.1} m · spławność {:?}\n",
        woda.class,
        f64::from(woda.depth_dm) / 10.0,
        terrain.navigable(x, y),
    ));

    let kolumna = terrain.column_at(x, y);
    out.push_str(
        "kolumna geologiczna (od powierzchni):
",
    );
    let mut top = kolumna.surface_z;
    for (i, (material, bottom)) in kolumna.layers.iter().enumerate() {
        // Warstwy o niedodatniej miąższości pomijamy: pokrywa biomu leży **na** stosie
        // geologicznym i przykrywa kilka górnych voxeli, więc warstwa pod nią bywa w całości
        // schowana. Pokazywanie „−0,5 m torfu" byłoby opisem artefaktu, nie kolumny.
        let miazszosc_voxeli = top - bottom + 1;
        if miazszosc_voxeli <= 0 {
            top = (*bottom - 1).min(top);
            continue;
        }
        // Ostatnia warstwa sięga dna świata — podawanie jej „grubości" w metrach niczego
        // nie mówi, bo jest to po prostu wszystko, co zostało.
        if i + 1 == kolumna.layers.len() {
            out.push_str(&format!(
                "  {:<12}  do dna (od {:.1} m n.p.m.)
",
                terrain.materials().get(*material).key,
                f64::from(top) * 0.5
            ));
            break;
        }
        out.push_str(&format!(
            "  {:<12} {:>6.1} m
",
            terrain.materials().get(*material).key,
            f64::from(miazszosc_voxeli) * 0.5
        ));
        top = *bottom - 1;
    }
    out.push_str(&format!(
        "zwierciadło wód gruntowych: {:.1} m n.p.m. ({:.1} m pod powierzchnią)\n",
        f64::from(kolumna.water_table_z) * 0.5,
        f64::from(kolumna.surface_z - kolumna.water_table_z) * 0.5,
    ));

    out.push_str(&format!(
        "klimat: styczeń {:.1} °C, lipiec {:.1} °C, opad {} mm/rok, żyzność {}\n",
        f64::from(klimat.temp_monthly_dc[0]) / 10.0,
        f64::from(klimat.temp_monthly_dc[6]) / 10.0,
        klimat
            .precip_monthly_mm
            .iter()
            .map(|m| u32::from(*m))
            .sum::<u32>(),
        klimat.soil_fertility.get(),
    ));

    let b = terrain.buildability_at(x, y);
    out.push_str(&format!(
        "zabudowa: {} · ryzyko powodziowe {} · koszt robót {} · nośność gruntu {}\n",
        if b.slope_ok {
            "dopuszczalna"
        } else {
            "wykluczona (nachylenie)"
        },
        b.flood_risk.get(),
        b.earthwork_cost_index,
        b.bearing_capacity,
    ));

    let zloza = terrain.deposits_at_column(x, y);
    if zloza.is_empty() {
        out.push_str("złoża: brak w tej kolumnie\n");
    } else {
        out.push_str("złoża przecinające kolumnę:\n");
        for id in zloza {
            let d = terrain.deposit(id);
            out.push_str(&format!(
                "  {:?} · {} m…{} m · {} t pozostało z {} t · koncentracja {} % · jakość {}\n",
                d.resource,
                d.depth_top_m,
                d.depth_bottom_m,
                d.remaining().0 / 1_000_000,
                d.reserves.0 / 1_000_000,
                d.concentration.get(),
                d.quality.get(),
            ));
        }
    }
    out
}
