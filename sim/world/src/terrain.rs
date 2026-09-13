//! `Terrain` — materializacja 1 m i implementacja kontraktu (M1 WP-W5, WP-W7).
//!
//! Ten typ jest jedynym obiektem, który fazy M2+ widzą z całego generatora. Robi trzy rzeczy:
//!
//! 1. **Podnosi rozdzielczość z 4 m na 1 m** — deterministycznie, przez interpolację
//!    Catmulla–Roma plus szum detalu. Siatka 4 m nie jest zaokrąglona do 1 m, tylko z niej
//!    wyprowadzona: `height_at` w punkcie siatki roboczej daje **dokładnie** jej wartość.
//! 2. **Wcina koryta z wektora**, nie z rastra. Erozja liczyła się na 4 m, a rzeka miejska
//!    ma 20–100 m szerokości; wcięcie z polilinii zachowuje ciągłość, której raster 4 m
//!    nie utrzyma (ryzyko R2 fazy).
//! 3. **Zamienia kolumnę na voxele** jako [`ColumnSource`] dla `engine/voxel`.
//!
//! Nic z tego nie jest składowane: mapa 16 km w voxelach to 134 GB (M1 §5.1), więc voxel
//! istnieje tylko w chwili, w której ktoś o niego pyta.

use crate::climate::ClimateCell;
use crate::data::{RiverNetwork, WaterClass, WorldData};
use crate::deposit::{Deposit, DepositId};
use crate::geology::{ColumnStack, VOXEL_DM};
use crate::noise::{fbm, FbmSpec, NoiseField};
use crate::params::{CLIMATE_CELL_M, WORK_CELL_M};
use crate::query::{
    Buildability, Crossing, NavigableClass, ObstacleBitset, TerrainQuery, TileCoord, WaterCell,
    TILE_M,
};
use magnat_core::{Biome, IRect, IVec2, IVec3, StreamId, Q};
use magnat_voxel::{
    ChunkBuilder, ChunkCoord, ColumnSource, MaterialId, MaterialRegistry, CHUNK_DIM,
};
use std::sync::Arc;

/// Amplituda szumu detalu przy materializacji 1 m, w metrach.
///
/// Celowo mała: detal ma rozbić gładkość interpolacji, a nie dołożyć rzeźby, której
/// hydrologia nie widziała. Większa amplituda tworzyłaby zagłębienia poza korytami —
/// a więc lokalne minima, których P4 już nie poprawi.
const DETAIL_AMPLITUDE_M: f32 = 0.35;
/// Bok komórki indeksu przestrzennego koryt w metrach.
const RIVER_INDEX_CELL_M: i32 = 128;
/// Nachylenie, powyżej którego teren jest nie do zabudowy (≈ 22°).
const BUILDABLE_MAX_SLOPE: u8 = 26;

/// Teren gotowy do odpytywania. Buduje się go raz, po generacji.
pub struct Terrain {
    data: WorldData,
    materials: Arc<MaterialRegistry>,
    geo_noise: NoiseField,
    detail_noise: NoiseField,
    /// Indeks przestrzenny odcinków koryt: dla komórki 128 m lista `(segment, punkt)`.
    /// Bez niego wcięcie koryta przy materializacji chunka przeszukiwałoby całą sieć.
    river_index: Vec<Vec<(u32, u32)>>,
    river_index_dim: usize,
}

impl Terrain {
    #[must_use]
    pub fn new(data: WorldData, materials: Arc<MaterialRegistry>) -> Terrain {
        let seed = data.params.seed;
        let map_m = data.params.size.meters() as i32;
        let idx_dim = (map_m / RIVER_INDEX_CELL_M).max(1) as usize;

        let mut river_index = vec![Vec::new(); idx_dim * idx_dim];
        for (si, seg) in data.rivers.segments.iter().enumerate() {
            for (pi, p) in seg.points.iter().enumerate() {
                let cx = (p.0 / RIVER_INDEX_CELL_M).clamp(0, idx_dim as i32 - 1) as usize;
                let cy = (p.1 / RIVER_INDEX_CELL_M).clamp(0, idx_dim as i32 - 1) as usize;
                river_index[cy * idx_dim + cx].push((si as u32, pi as u32));
            }
        }

        Terrain {
            geo_noise: NoiseField::new(seed, StreamId::WorldGeology, 0),
            detail_noise: NoiseField::new(seed, StreamId::WorldDetail, 0),
            river_index,
            river_index_dim: idx_dim,
            materials,
            data,
        }
    }

    #[must_use]
    pub fn data(&self) -> &WorldData {
        &self.data
    }

    #[must_use]
    pub fn materials(&self) -> &MaterialRegistry {
        &self.materials
    }

    /// Bok mapy w metrach.
    #[must_use]
    pub fn size_m(&self) -> i32 {
        self.data.params.size.meters() as i32
    }

    /// Wysokość terenu w decymetrach, w rozdzielczości 1 m. Rdzeń materializacji.
    #[must_use]
    pub fn height_dm(&self, x: i32, y: i32) -> i32 {
        self.height_dm_z_detalem(x, y, true)
    }

    /// Wysokość z możliwością pominięcia szumu detalu.
    ///
    /// Detal ma amplitudę 3,5 dm i istnieje po to, żeby płaski teren nie był idealnie gładki
    /// z bliska. Przy agregacie LOD2 jeden voxel ma 2 m wysokości, a przy LOD3 — 4 m, więc
    /// szum mieści się w ułamku voxela i nie zmienia wyniku, a kosztuje trzy oktawy szumu
    /// na każdą z czterech próbek komórki. To była różnica między 1,2 a 1,8 ms na chunk
    /// LOD3, czyli między budżetem z §7.3 a jego przekroczeniem.
    fn height_dm_z_detalem(&self, x: i32, y: i32, detal: bool) -> i32 {
        let (wciecie_dm, udzial) = self.carve_river(x, y);
        if !detal {
            let h = self.bicubic_dm(x, y) - wciecie_dm;
            return match self.data.water_surface_dm(self.work_index(x, y)) {
                Some(zwierciadlo) => h.min(i32::from(zwierciadlo) - VOXEL_DM),
                None => h,
            };
        }
        // Szum detalu jest **wygaszany w korycie**, a nie dokładany do niego.
        // Powód jest mierzalny: amplituda szumu to 3,5 dm, a małe koryto ma 3 dm głębokości,
        // więc szum całkowicie zjadał wcięcie — rzeka wyglądała jak rowek, który raz jest,
        // a raz go nie ma. Dno rzeki jest zresztą gładkie i w naturze.
        let detal = if udzial > 0.0 {
            (self.detail_dm(x, y) as f32 * (1.0 - udzial)) as i32
        } else {
            self.detail_dm(x, y)
        };
        let h = self.bicubic_dm(x, y) + detal - wciecie_dm;

        // Dno wody nie ma prawa wystawać ponad jej zwierciadło. Interpolacja 4 m → 1 m
        // i szum detalu obie działają w obie strony, więc na płytkim obrzeżu jeziora
        // podnosiły dno o kilka decymetrów ponad lustro: komórka nadal była wodna, więc
        // nie dostawała pokrywy biomu, ale wody też już w niej nie było — i na ekranie
        // brzegi jezior były obwiedzione gołym bazaltem. Przycięcie jest jednostronne,
        // więc nigdzie nie podnosi terenu, a zostawia co najmniej jedną warstwę wody.
        match self.data.water_surface_dm(self.work_index(x, y)) {
            Some(zwierciadlo) => h.min(i32::from(zwierciadlo) - VOXEL_DM),
            None => h,
        }
    }

    /// Interpolacja Catmulla–Roma siatki 4 m. W punktach siatki zwraca **dokładnie**
    /// jej wartość, więc materializacja 1 m nie rozjeżdża się z hydrologią liczoną na 4 m.
    fn bicubic_dm(&self, x: i32, y: i32) -> i32 {
        let g = &self.data.height;
        let cell = WORK_CELL_M as i32;
        let (gx, gy) = (x.div_euclid(cell), y.div_euclid(cell));
        let (fx, fy) = (
            x.rem_euclid(cell) as f32 / cell as f32,
            y.rem_euclid(cell) as f32 / cell as f32,
        );

        let mut kolumny = [0.0f32; 4];
        for (j, kol) in kolumny.iter_mut().enumerate() {
            let mut wiersz = [0.0f32; 4];
            for (i, w) in wiersz.iter_mut().enumerate() {
                *w = f32::from(
                    *g.get_clamped(i64::from(gx) + i as i64 - 1, i64::from(gy) + j as i64 - 1),
                );
            }
            *kol = catmull_rom(wiersz, fx);
        }
        let v = catmull_rom(kolumny, fy);
        // Zaokrąglenie symetryczne — to samo, którym generator zapisywał wysokości.
        if v >= 0.0 {
            (v + 0.5) as i32
        } else {
            (v - 0.5) as i32
        }
    }

    /// Szum detalu w decymetrach, wygaszany na stromiźnie.
    ///
    /// Wygaszany, bo na zboczu dokładanie szumu produkuje półki i zagłębienia; na płaskim
    /// dodaje faktury tam, gdzie interpolacja jest idealnie gładka i to widać.
    fn detail_dm(&self, x: i32, y: i32) -> i32 {
        let slope = self.slope_at(x, y);
        let tlumienie = 1.0 - (f32::from(slope) / 60.0).clamp(0.0, 1.0);
        if tlumienie <= 0.0 {
            return 0;
        }
        let n = fbm(
            &self.detail_noise,
            x as f32,
            y as f32,
            FbmSpec::new(3, 24.0),
        );
        (n * DETAIL_AMPLITUDE_M * tlumienie * 10.0) as i32
    }

    /// Rozmycie granicy biomu: punkt próbkowania przesuwany szumem o ± ~110 m.
    ///
    /// Komórka klimatu ma 256 m i biom jest w niej stały, więc granica biegnie dokładnie
    /// po jej boku. Przy sąsiedztwie o kontrastowej pokrywie — bagno przy jeziorze, skała
    /// przy lesie — wychodzi z tego na ekranie kwadrat 256 × 256 m z ostrymi bokami,
    /// widoczny z kilometra. Przesunięcie punktu próbkowania nie zmienia **udziału**
    /// biomów (klasyfikacja Whittakera zostaje ta sama), tylko łamie granicę na
    /// nieregularną linię — tę samą sztuczkę stosuje domain warp w generatorze wysokości.
    fn biome_warp(&self, x: i32, y: i32) -> (i32, i32) {
        const AMP_M: f32 = 110.0;
        let spec = FbmSpec::new(2, 140.0);
        // Przesunięcia próbkowania rozsunięte w dziedzinie, żeby `dx` i `dy` nie były
        // tą samą liczbą — inaczej warp działa wyłącznie po przekątnej.
        let dx = fbm(&self.detail_noise, x as f32 + 3300.0, y as f32, spec) * AMP_M;
        let dy = fbm(&self.detail_noise, x as f32, y as f32 + 7700.0, spec) * AMP_M;
        (x + dx as i32, y + dy as i32)
    }

    /// Wcięcie koryta z **wektorowej** sieci (ryzyko R2).
    ///
    /// Zwraca `(głębokość wcięcia w dm, udział koryta 0..1)`. Udział służy do wygaszenia
    /// szumu detalu — patrz [`Terrain::height_dm`]. Profil jest paraboliczny: najgłębiej
    /// w osi, zero na krawędzi.
    fn carve_river(&self, x: i32, y: i32) -> (i32, f32) {
        let idx_dim = self.river_index_dim as i32;
        let (cx, cy) = (
            (x / RIVER_INDEX_CELL_M).clamp(0, idx_dim - 1),
            (y / RIVER_INDEX_CELL_M).clamp(0, idx_dim - 1),
        );
        let mut najglebiej = 0i32;
        let mut udzial = 0.0f32;

        for oy in (cy - 1).max(0)..=(cy + 1).min(idx_dim - 1) {
            for ox in (cx - 1).max(0)..=(cx + 1).min(idx_dim - 1) {
                for (si, pi) in &self.river_index[(oy * idx_dim + ox) as usize] {
                    let seg = &self.data.rivers.segments[*si as usize];
                    let pi = *pi as usize;
                    if pi + 1 >= seg.points.len() {
                        continue;
                    }
                    let a = IVec2::new(seg.points[pi].0, seg.points[pi].1);
                    let b = IVec2::new(seg.points[pi + 1].0, seg.points[pi + 1].1);
                    let half = i64::from(seg.width_dm) / 20; // dm → m, połowa szerokości
                    if half <= 0 {
                        continue;
                    }
                    let d2 = dist_sq_to_segment(IVec2::new(x, y), a, b);
                    if d2 >= half * half {
                        continue;
                    }
                    // Profil paraboliczny: 1 w osi, 0 na krawędzi.
                    let t = d2 as f32 / (half * half) as f32;
                    let profil = 1.0 - t;
                    let g = (f32::from(seg.depth_dm) * profil) as i32;
                    najglebiej = najglebiej.max(g);
                    udzial = udzial.max(profil);
                }
            }
        }
        (najglebiej, udzial.clamp(0.0, 1.0))
    }

    /// Indeks komórki siatki roboczej dla punktu w metrach.
    #[inline]
    fn work_index(&self, x: i32, y: i32) -> usize {
        let dim = self.data.height.dim() as i32;
        let gx = (x / WORK_CELL_M as i32).clamp(0, dim - 1);
        let gy = (y / WORK_CELL_M as i32).clamp(0, dim - 1);
        (gy * dim + gx) as usize
    }

    #[inline]
    fn climate_index(&self, x: i32, y: i32) -> usize {
        let cdim = self.data.climate.dim() as i32;
        let cx = (x / CLIMATE_CELL_M as i32).clamp(0, cdim - 1);
        let cy = (y / CLIMATE_CELL_M as i32).clamp(0, cdim - 1);
        (cy * cdim + cx) as usize
    }

    #[inline]
    fn water_dist_m(&self, x: i32, y: i32) -> u16 {
        let d = self.data.water_dist.dim() as i32;
        let cell = crate::gen::geology::WATER_DIST_CELL_M as i32;
        let cx = (x / cell).clamp(0, d - 1);
        let cy = (y / cell).clamp(0, d - 1);
        self.data.water_dist[(cy * d + cx) as usize]
    }
}

/// Splajn Catmulla–Roma. Tylko mnożenia i dodawania, więc deterministyczny (00 §K-6);
/// przechodzi przez punkty kontrolne, więc siatka 4 m zostaje odtworzona dokładnie.
#[inline]
fn catmull_rom(p: [f32; 4], t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p[1])
        + (-p[0] + p[2]) * t
        + (2.0 * p[0] - 5.0 * p[1] + 4.0 * p[2] - p[3]) * t2
        + (-p[0] + 3.0 * p[1] - 3.0 * p[2] + p[3]) * t3)
}

/// Kwadrat odległości punktu od odcinka, całkowitoliczbowo.
fn dist_sq_to_segment(p: IVec2, a: IVec2, b: IVec2) -> i64 {
    let (abx, aby) = (i64::from(b.x - a.x), i64::from(b.y - a.y));
    let (apx, apy) = (i64::from(p.x - a.x), i64::from(p.y - a.y));
    let len_sq = abx * abx + aby * aby;
    if len_sq == 0 {
        return apx * apx + apy * apy;
    }
    let dot = (apx * abx + apy * aby).clamp(0, len_sq);
    let cx = apx - abx * dot / len_sq;
    let cy = apy - aby * dot / len_sq;
    cx * cx + cy * cy
}

impl TerrainQuery for Terrain {
    fn height_at(&self, x: i32, y: i32) -> i32 {
        self.height_dm(x, y) / VOXEL_DM
    }

    fn height_tile(&self, tile: TileCoord, out: &mut [i32; TILE_M * TILE_M]) {
        let o = tile.origin_m();
        for ly in 0..TILE_M {
            for lx in 0..TILE_M {
                out[ly * TILE_M + lx] = self.height_dm(o.x + lx as i32, o.y + ly as i32) / VOXEL_DM;
            }
        }
    }

    fn slope_at(&self, x: i32, y: i32) -> u8 {
        // Liczone z siatki 4 m, nie z materializacji 1 m: nachylenie ma opisywać rzeźbę,
        // a nie szum detalu, który sam jest tym nachyleniem tłumiony.
        let g = &self.data.height;
        let cell = WORK_CELL_M as i64;
        let (gx, gy) = (i64::from(x) / cell, i64::from(y) / cell);
        let h = |dx: i64, dy: i64| f32::from(*g.get_clamped(gx + dx, gy + dy));
        let dzdx = (h(1, 0) - h(-1, 0)) / (2.0 * WORK_CELL_M as f32 * 10.0);
        let dzdy = (h(0, 1) - h(0, -1)) / (2.0 * WORK_CELL_M as f32 * 10.0);
        let tan = magnat_core::det_math::sqrt(f64::from(dzdx * dzdx + dzdy * dzdy)) as f32;
        (tan * 64.0).clamp(0.0, 255.0) as u8
    }

    fn surface_material_at(&self, x: i32, y: i32) -> MaterialId {
        let c = self.column_at(x, y);
        c.material_at(c.surface_z).unwrap_or(MaterialId::AIR)
    }

    fn column_at(&self, x: i32, y: i32) -> ColumnStack {
        // Miąższości warstw liczone na tej **samej** rzadkiej siatce, z której korzysta
        // materializacja voxeli (`GEOLOGY_SAMPLE_M`). To nie jest oszczędność przeniesiona
        // do zapytania, tylko warunek spójności: gdyby inspektor liczył geologię dokładnie,
        // a chunk co 8 m, karta inspekcji pokazywałaby inną warstwę niż ta, którą widać
        // w wykopie — i test `column_matches_voxels` (§7.1) miałby rację, zgłaszając błąd.
        let (gx, gy) = (
            (x / GEOLOGY_SAMPLE_M) * GEOLOGY_SAMPLE_M,
            (y / GEOLOGY_SAMPLE_M) * GEOLOGY_SAMPLE_M,
        );
        let t = self.data.geology.layer_thicknesses(
            &self.geo_noise,
            gx,
            gy,
            self.bicubic_dm(gx, gy) / 10,
            self.slope_at(gx, gy),
            self.water_dist_m(gx, gy),
        );
        // Powierzchnia i zwierciadło wód gruntowych — już w pełnej rozdzielczości 1 m.
        let mut stack = self.data.geology.stack_from(
            &t,
            self.height_dm(x, y),
            self.slope_at(x, y),
            self.water_dist_m(x, y),
        );
        // Pokrywa biomu wchodzi też tutaj, nie tylko do voxeli. Bez tego inspektor
        // pokazywałby piasek tam, gdzie w wykopie widać darń — a to jest dokładnie ta
        // rozbieżność, którą ma wyłapywać `column_matches_voxels` (§7.1). Zgadzało się
        // dopóty, dopóki biom pokrywał się z wierzchnią warstwą geologiczną, czyli
        // przypadkiem.
        // Biom na siatce `GEOLOGY_SAMPLE_M`, dokładnie jak w `fill_chunk` — inaczej
        // inspektor pokazywałby inną pokrywę niż widać w wykopie (`column_matches_voxels`).
        let (bx, by) = (
            (x / GEOLOGY_SAMPLE_M) * GEOLOGY_SAMPLE_M,
            (y / GEOLOGY_SAMPLE_M) * GEOLOGY_SAMPLE_M,
        );
        if let Some(m) = SurfaceCover::new(&self.materials).material(self.biome_at(bx, by)) {
            stack
                .layers
                .insert(0, (m, stack.surface_z - SURFACE_COVER_VOXELS + 1));
        }
        stack
    }

    fn water_at(&self, x: i32, y: i32) -> WaterCell {
        let i = self.work_index(x, y);
        let bits = self.data.water[i];
        WaterCell {
            depth_dm: self.data.water_depth_dm(i),
            class: bits.class(),
            flow_dir: bits.flow_dir(),
            strahler: self.data.river_cell(i).map_or(0, |r| r.strahler),
        }
    }

    fn river_network(&self) -> &RiverNetwork {
        &self.data.rivers
    }

    fn navigable(&self, x: i32, y: i32) -> Option<NavigableClass> {
        let i = self.work_index(x, y);
        match self.data.water[i].class() {
            WaterClass::Sea => Some(NavigableClass::Sea),
            WaterClass::River => {
                // Spławna jest rzeka, w której coś się zmieści. Próg 8 dm to płaskodenna
                // barka śródlądowa; poniżej zostaje kajak, a kajakiem nie wozi się węgla.
                let d = self.data.water_depth_dm(i);
                (d >= 8).then_some(NavigableClass::River { min_draft_dm: d })
            }
            WaterClass::Lake => {
                let d = self.data.water_depth_dm(i);
                (d >= 8).then_some(NavigableClass::Canal)
            }
            WaterClass::Dry => None,
        }
    }

    fn buildability_at(&self, x: i32, y: i32) -> Buildability {
        let i = self.work_index(x, y);
        let slope = self.slope_at(x, y);
        let h_dm = self.height_dm(x, y);

        // Ryzyko powodziowe: wysokość nad zwierciadłem najbliższej wody miarodajnej.
        // Woda miarodajna to zwierciadło koryta podniesione o zapas rosnący z rzędem
        // Strahlera — duża rzeka wylewa wyżej niż strumień, i to jest cała różnica
        // między „dom nad wodą" a „dom w wodzie".
        let dist = self.water_dist_m(x, y);
        let flood = if dist > 900 {
            Q::MIN
        } else {
            let strahler = self.data.river_cell(i).map_or(1, |r| r.strahler);
            let zapas_dm = 10 + i32::from(strahler) * 12;
            let zwierciadlo = self.data.water_surface_dm(i).map_or(h_dm - 40, i32::from) + zapas_dm;
            let ponad = h_dm - zwierciadlo;
            // 0 m nad wodą miarodajną → 100, 12 m i wyżej → 0.
            let q = (100 - (ponad * 100 / 120)).clamp(0, 100);
            // Oddalenie od koryta też chroni, niezależnie od wysokości.
            let tlumienie = 100 - i32::from(dist) * 100 / 900;
            Q::new((q * tlumienie / 100).clamp(0, 100) as u8)
        };

        let material = self.surface_material_at(x, y);
        let bearing = self.materials.get(material).bearing_capacity;

        // Indeks kosztu robót: rośnie z nachyleniem (wykop i nasyp) oraz spada przy
        // słabym gruncie (wymiana podłoża).
        let earthwork = 100u32
            + u32::from(slope) * 6
            + u32::from(255 - bearing) / 2
            + u32::from(flood.get()) * 2;

        Buildability {
            slope_ok: slope <= BUILDABLE_MAX_SLOPE && self.data.water[i].class() == WaterClass::Dry,
            flood_risk: flood,
            bearing_capacity: bearing,
            earthwork_cost_index: earthwork.min(u32::from(u16::MAX)) as u16,
        }
    }

    fn obstacle_mask(&self, rect: IRect) -> ObstacleBitset {
        let mut m = ObstacleBitset::new(rect);
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                let i = self.work_index(x, y);
                let woda = self.data.water[i].class().is_water();
                let stromo = self.slope_at(x, y) > BUILDABLE_MAX_SLOPE;
                m.set(IVec2::new(x, y), woda || stromo);
            }
        }
        m
    }

    fn crossing_cost(&self, a: IVec2, b: IVec2) -> Crossing {
        let len = magnat_core::det_math::sqrt(a.distance_sq(b) as f64) as u32;
        if len == 0 {
            return Crossing::Flat;
        }
        let kroki = (len / 4).max(1);
        let (mut woda_dm, mut min_dm, mut max_dm) = (0i32, i32::MAX, i32::MIN);
        let (mut nad_woda, mut skala) = (false, MaterialId::AIR);

        for k in 0..=kroki {
            let t = k as i32;
            let x = a.x + (b.x - a.x) * t / kroki as i32;
            let y = a.y + (b.y - a.y) * t / kroki as i32;
            let h = self.height_dm(x, y);
            min_dm = min_dm.min(h);
            max_dm = max_dm.max(h);
            let i = self.work_index(x, y);
            if self.data.water[i].class().is_water() {
                nad_woda = true;
                woda_dm = woda_dm.max(self.data.water_depth_dm(i) as i32);
            }
            if k == kroki / 2 {
                let c = self.column_at(x, y);
                skala = c.material_at(c.surface_z - 8).unwrap_or(MaterialId::AIR);
            }
        }

        let deniwelacja = max_dm - min_dm;
        if nad_woda {
            return Crossing::Bridge {
                span_m: len,
                // Prześwit nad wodą miarodajną: 5 m plus głębokość koryta.
                clearance_m: (50 + woda_dm) as u16 / 10,
            };
        }
        // Wzniesienie wyższe niż połowa długości przejścia to góra, nie pagórek —
        // tam tunel wychodzi taniej niż wcinanie się w zbocze.
        if deniwelacja > (len as i32) * 10 / 2 && deniwelacja > 300 {
            return Crossing::Tunnel {
                len_m: len,
                rock: skala,
            };
        }
        if deniwelacja > 30 {
            // Objętość nasypu: przekrój trapezowy o szerokości 10 m i średniej wysokości
            // połowy deniwelacji.
            let fill = i64::from(len) * i64::from(deniwelacja) / 10 * 10 / 2;
            return Crossing::Embankment { fill_m3: fill };
        }
        Crossing::Flat
    }

    fn deposits_in(&self, rect: IRect) -> Vec<DepositId> {
        // Liniowe przeszukanie listy złóż. Na mapie 16 km jest ich ~6000, a M2 pyta
        // o prostokąt raz na dzielnicę — indeks przestrzenny byłby tu strukturą,
        // która kosztuje pamięć i nigdy się nie zwraca (YAGNI).
        self.data
            .deposits
            .iter()
            .filter(|d| {
                let c = deposit_center(d);
                rect.contains(IVec2::new(c.x, c.y))
            })
            .map(|d| d.id)
            .collect()
    }

    fn deposit(&self, id: DepositId) -> &Deposit {
        &self.data.deposits[id.0 as usize]
    }

    fn deposits_at_column(&self, x: i32, y: i32) -> Vec<DepositId> {
        let surface = self.height_dm(x, y);
        self.data
            .deposits
            .iter()
            .filter(|d| {
                // Próbkujemy kolumnę na głębokości stropu złoża — tam, gdzie formacja
                // faktycznie jest, a nie w punkcie środkowym, który może być poza nią.
                let z = surface - i32::from(d.depth_top_m) * 10 - 1;
                d.shape.contains(IVec3::new(x, y, z))
            })
            .map(|d| d.id)
            .collect()
    }

    fn climate_at(&self, x: i32, y: i32) -> &ClimateCell {
        &self.data.climate[self.climate_index(x, y)]
    }

    fn biome_at(&self, x: i32, y: i32) -> Biome {
        // Woda rozstrzyga przed klimatem: komórka klimatu ma 256 m, a rzeka 20 m, więc
        // klasyfikacja klimatyczna nigdy by jej nie zobaczyła. Punkt na rzece jest rzeką.
        match self.data.water[self.work_index(x, y)].class() {
            WaterClass::Sea => Biome::Sea,
            WaterClass::Lake => Biome::Lake,
            WaterClass::River => Biome::River,
            WaterClass::Dry => {
                let (wx, wy) = self.biome_warp(x, y);
                self.climate_at(wx, wy).biome
            }
        }
    }

    fn soil_fertility_at(&self, x: i32, y: i32) -> Q {
        self.climate_at(x, y).soil_fertility
    }
}

fn deposit_center(d: &Deposit) -> IVec3 {
    match &d.shape {
        crate::deposit::DepositShape::Ellipsoid { center, .. }
        | crate::deposit::DepositShape::Trap { center, .. } => *center,
        crate::deposit::DepositShape::Seam {
            polyline, top_z, ..
        } => {
            let p = polyline.first().copied().unwrap_or(IVec2::ZERO);
            IVec3::new(p.x, p.y, *top_z)
        }
        crate::deposit::DepositShape::Aquifer { poly, top_z, .. } => {
            let p = poly.first().copied().unwrap_or(IVec2::ZERO);
            IVec3::new(p.x, p.y, *top_z)
        }
    }
}

/// Co ile metrów próbkowana jest geologia przy materializacji chunka.
///
/// Miąższości warstw mają długości fal rzędu kilometra (`data/geology/layers.ron`), więc
/// próbkowanie ich co metr to liczenie tej samej wartości szesnaście razy. Różnica jest
/// mierzalna: chunk LOD0 to 1156 kolumn, a przy kroku 8 m — 25 próbek geologii zamiast 1156.
const GEOLOGY_SAMPLE_M: i32 = 8;

/// Grubość pokrywy biomu w voxelach 0,5 m.
///
/// Geologia nie zna biomu i nie powinna: warstwy skalne zależą od wypiętrzenia i erozji,
/// a darń od klimatu. Pokrywa jest więc **nakładana na wierzch** przy materializacji.
///
/// Cztery voxele, nie jeden. Przy upsamplingu 4 m → 1 m zbocze ma stopnie po 1–2 m, a ściana
/// stopnia pokazuje to, co leży pod pokrywą. Przy pokrywie jednovoxelowej każdy stopień
/// odsłaniał piasek i mapa pokrywała się siatką jasnych poziomic — widać to na zrzutach
/// z pierwszej wersji. Dwa metry darni zakrywają typowy stopień i nie zmieniają niczego
/// poza wyglądem: kolumna geologiczna pod spodem zostaje ta sama.
const SURFACE_COVER_VOXELS: i32 = 4;

impl ColumnSource for Terrain {
    /// Materializacja chunka. Wołana z dowolnego workera — i dlatego nie ma tu ani jednego
    /// zapisu do `self` (M1 §5.2: źródło musi być czyste, bo kolejność ładowania chunków
    /// zależy od kamery, a kamera nie wchodzi do hasha stanu).
    ///
    /// **Agregacja LOD dzieje się tutaj, nie w `engine/voxel`** (M1 §5.4: „agregat liczy się
    /// z `ColumnSource`"). Dla `lod > 0` powierzchnia komórki agregatu jest średnią z czterech
    /// próbek w jej obrębie — to jest praktyczna postać reguły „pusto, gdy ponad połowa
    /// to powietrze": zbocze nie puchnie, bo komórka nie bierze najwyższego punktu, ani się
    /// nie dziurawi, bo nie bierze najniższego.
    fn fill_chunk(&self, coord: ChunkCoord, lod: u8, out: &mut ChunkBuilder) {
        let skala = 1i32 << lod;
        let origin = coord.origin_voxels(lod);
        let zakres = out.range();
        let water = self.materials.id_of("water").unwrap_or(MaterialId::AIR);
        let size = self.size_m();
        let pokrywa = SurfaceCover::new(&self.materials);

        // Geologia na rzadszej siatce: jedna próbka na `GEOLOGY_SAMPLE_M` metrów obszaru chunka.
        let span = CHUNK_DIM as i32 * skala;
        let krok = GEOLOGY_SAMPLE_M.max(skala);
        let n_geo = (span / krok + 2) as usize;
        let mut geo: Vec<smallvec::SmallVec<[(MaterialId, i32); 8]>> =
            Vec::with_capacity(n_geo * n_geo);
        for gy in 0..n_geo {
            for gx in 0..n_geo {
                let wx = (origin.x + gx as i32 * krok).clamp(0, size - 1);
                let wy = (origin.y + gy as i32 * krok).clamp(0, size - 1);
                geo.push(self.data.geology.layer_thicknesses(
                    &self.geo_noise,
                    wx,
                    wy,
                    self.bicubic_dm(wx, wy) / 10,
                    self.slope_at(wx, wy),
                    self.water_dist_m(wx, wy),
                ));
            }
        }

        for ly in zakres.clone() {
            for lx in zakres.clone() {
                let wx = origin.x + lx * skala;
                let wy = origin.y + ly * skala;
                if wx < 0 || wy < 0 || wx >= size || wy >= size {
                    continue;
                }

                // Powierzchnia: jedna próbka w LOD0, średnia z czterech w agregacie.
                // Od LOD2 wzwyż bez szumu detalu — jego amplituda jest mniejsza niż
                // wysokość voxela agregatu (patrz `height_dm_z_detalem`).
                let detal = skala < 4;
                let surface_dm = if skala == 1 {
                    self.height_dm(wx, wy)
                } else {
                    let p = skala / 2;
                    let h = |x: i32, y: i32| i64::from(self.height_dm_z_detalem(x, y, detal));
                    let s = h(wx, wy)
                        + h((wx + p).min(size - 1), wy)
                        + h(wx, (wy + p).min(size - 1))
                        + h((wx + p).min(size - 1), (wy + p).min(size - 1));
                    (s / 4) as i32
                };

                // Zwierciadło wody dla tej kolumny. W agregacie **najwyższe** z tych samych
                // czterech próbek, nie jedno z lewego górnego rogu: kolumna LOD2 opisuje
                // 4 m terenu, a jezioro albo w niej jest, albo go nie ma — uśrednianie
                // lustra dałoby brzeg schodzący pod wodę w połowie voxela.
                let woda_dm = if skala == 1 {
                    self.data
                        .water_surface_dm(self.work_index(wx, wy))
                        .map(i32::from)
                } else {
                    let p = skala / 2;
                    [(0, 0), (p, 0), (0, p), (p, p)]
                        .iter()
                        .filter_map(|(dx, dy)| {
                            let i =
                                self.work_index((wx + dx).min(size - 1), (wy + dy).min(size - 1));
                            self.data.water_surface_dm(i).map(i32::from)
                        })
                        .max()
                };
                // Uśredniona powierzchnia potrafi wyjść **ponad** lustro — i wtedy komórka
                // jest nadal wodna (więc bez pokrywy biomu), ale wody w niej nie ma:
                // z daleka jezioro dostawało łatę gołego bazaltu wielkości chunka.
                // Przycięcie jest jednostronne, dokładnie jak w `height_dm`.
                let surface_dm = match woda_dm {
                    Some(w) => surface_dm.min(w - VOXEL_DM),
                    None => surface_dm,
                };

                let gx = ((lx * skala).clamp(0, span) / krok).clamp(0, n_geo as i32 - 1) as usize;
                let gy = ((ly * skala).clamp(0, span) / krok).clamp(0, n_geo as i32 - 1) as usize;
                let column = self.data.geology.stack_from(
                    &geo[gy * n_geo + gx],
                    surface_dm,
                    0,
                    self.water_dist_m(wx, wy),
                );

                // Voxele ziemi: warstwa po warstwie, od powierzchni w dół.
                //
                // Przedziały muszą pokrywać się **dokładnie** z regułą `ColumnStack::material_at`
                // (pierwsza warstwa, dla której `z >= bottom`): warstwa o granicy `bottom`
                // sięga od `bottom` do `top`, a następna zaczyna się od `bottom - 1`.
                // Bez tego przesunięcia sąsiednie warstwy nachodzą na siebie jednym voxelem,
                // a materializacja przestaje zgadzać się z kolumną — dokładnie to sprawdza
                // test `column_matches_voxels` z §7.1.
                let mut top = column.surface_z;
                for (material, bottom) in &column.layers {
                    if top < origin.z {
                        break;
                    }
                    let hi = to_local(top, origin.z, skala);
                    let lo = to_local((*bottom).max(origin.z - 1), origin.z, skala);
                    out.fill_column(lx, ly, lo, hi + 1, *material);
                    top = *bottom - 1;
                }

                // Pokrywa biomu na wierzchu — darń, piasek, skała albo śnieg. Biom
                // próbkowany na tej samej rzadkiej siatce co geologia: komórka klimatu ma
                // 256 m, a rozmycie granicy działa w skali ~140 m, więc pytanie o biom co
                // metr liczy ten sam szum sześćdziesiąt cztery razy. Przy LOD3 to była
                // różnica między 1,2 a 1,8 ms na chunk, czyli między budżetem a jego
                // przekroczeniem (§7.3).
                let (bx, by) = (
                    (wx / GEOLOGY_SAMPLE_M) * GEOLOGY_SAMPLE_M,
                    (wy / GEOLOGY_SAMPLE_M) * GEOLOGY_SAMPLE_M,
                );
                if let Some(m) = pokrywa.material(self.biome_at(bx, by)) {
                    let hi = to_local(column.surface_z, origin.z, skala);
                    let lo = to_local(column.surface_z - SURFACE_COVER_VOXELS + 1, origin.z, skala);
                    out.fill_column(lx, ly, lo, hi + 1, m);
                }

                // Woda: od powierzchni terenu do zwierciadła.
                if let Some(surface) = woda_dm {
                    let z_wody = surface / VOXEL_DM;
                    if z_wody > column.surface_z {
                        let lo = to_local(column.surface_z + 1, origin.z, skala);
                        let hi = to_local(z_wody, origin.z, skala);
                        out.fill_column(lx, ly, lo, hi + 1, water);
                    }
                }
            }
        }
    }
}

/// Materiały pokrywy terenu, rozwiązane raz zamiast po kluczu tekstowym na każdą kolumnę.
struct SurfaceCover {
    grass: Option<MaterialId>,
    sand: Option<MaterialId>,
    rock: Option<MaterialId>,
    snow: Option<MaterialId>,
    peat: Option<MaterialId>,
}

impl SurfaceCover {
    fn new(reg: &MaterialRegistry) -> SurfaceCover {
        SurfaceCover {
            grass: reg.id_of("grass"),
            sand: reg.id_of("sand"),
            rock: reg.id_of("granite"),
            snow: reg.id_of("snow"),
            peat: reg.id_of("peat"),
        }
    }

    /// `None` dla biomów wodnych — tam powierzchnią jest dno, a wodę kładzie osobny krok.
    fn material(&self, biome: Biome) -> Option<MaterialId> {
        match biome {
            Biome::Sea | Biome::Lake | Biome::River => None,
            Biome::Sand => self.sand,
            Biome::Rock => self.rock,
            Biome::Snow => self.snow,
            Biome::Marsh => self.peat,
            // Las, step, pole i zarośla mają darń — różnią się roślinnością, a ta jest
            // zadaniem M11 (modele), nie kolorem gruntu.
            _ => self.grass,
        }
    }
}

/// Przeliczenie wysokości świata w jednostkach 0,5 m na lokalny indeks Z chunka.
#[inline]
fn to_local(z_world: i32, origin_z: i32, skala: i32) -> i32 {
    (z_world - origin_z).div_euclid(skala)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;
    use magnat_voxel::CHUNK_DIM;

    fn teren(region: Region, seed: u64) -> Terrain {
        let pool = JobPool::new(2);
        let params = WorldGenParams {
            seed,
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let (data, _) = crate::generate(params, &pool).unwrap();
        let reg = Arc::new(MaterialRegistry::load_dir(&crate::data_path("materials")).unwrap());
        Terrain::new(data, reg)
    }

    #[test]
    fn interpolacja_odtwarza_siatke_robocza_co_do_decymetra() {
        // Niezmiennik materializacji: w punkcie siatki 4 m wynik to dokładnie ta wartość.
        // Bez niego hydrologia liczona na 4 m i teren oglądany na 1 m opisują dwa różne
        // światy, a koryto mija się z doliną.
        let t = teren(Region::Lowland, 5);
        let g = &t.data.height;
        for gy in (2..g.dim() - 2).step_by(37) {
            for gx in (2..g.dim() - 2).step_by(41) {
                let (x, y) = ((gx * 4) as i32, (gy * 4) as i32);
                assert_eq!(
                    t.bicubic_dm(x, y),
                    i32::from(*g.get(gx, gy)),
                    "punkt siatki ({gx}, {gy})"
                );
            }
        }
    }

    #[test]
    fn materializacja_jest_czysta_funkcja_pozycji() {
        let t = teren(Region::River, 9);
        for (x, y) in [(123, 456), (0, 0), (2047, 3001)] {
            assert_eq!(t.height_dm(x, y), t.height_dm(x, y));
        }
    }

    #[test]
    fn koryto_na_metrze_pokrywa_sie_z_wektorem() {
        // Ryzyko R2: rzeka wcięta z wektora ma leżeć w dolinie wyerodowanej na 4 m.
        let t = teren(Region::River, 6);
        let siec = t.river_network();
        assert!(!siec.segments.is_empty());

        // Właściwym sprawdzianem jest to, czy **wcięcie zostało zastosowane** na osi koryta.
        // Porównanie „oś niżej niż brzeg" działa tylko dla rzek głębszych niż poprzeczny
        // spadek zbocza: strumień o głębokości 0,3 m płynący po stoku ma brzeg oddalony
        // o 6 m naturalnie niżej od własnej osi, i to nie jest błąd materializacji.
        let (mut sprawdzone, mut szerokie) = (0, 0);
        for seg in siec.segments.iter().take(200) {
            if seg.width_dm < 20 {
                continue; // koryto węższe niż 2 m nie ma się w co wcinać na siatce 1 m
            }
            for p in seg.points.iter().take(seg.points.len() - 1).step_by(4) {
                let (x, y) = (p.0, p.1);
                if x < 16 || y < 16 || x >= t.size_m() - 16 || y >= t.size_m() - 16 {
                    continue;
                }
                let (wciecie, udzial) = t.carve_river(x, y);
                assert!(
                    udzial > 0.9,
                    "punkt osi ({x}, {y}) nie leży w korycie: udział {udzial}"
                );
                assert!(
                    wciecie >= i32::from(seg.depth_dm) - 1,
                    "oś ({x}, {y}) wcięta o {wciecie} dm zamiast {} dm",
                    seg.depth_dm
                );
                assert_eq!(
                    t.height_dm(x, y),
                    t.bicubic_dm(x, y) - wciecie,
                    "szum detalu przecieka do koryta w ({x}, {y})"
                );
                sprawdzone += 1;

                // Rzeka szeroka na 6 m i więcej ma leżeć niżej niż jej brzeg — tu wcięcie
                // dominuje nad poprzecznym spadkiem terenu.
                if seg.width_dm >= 60 {
                    let obok = (seg.width_dm / 10) as i32 + 4;
                    let brzeg = t.height_dm(x + obok, y).max(t.height_dm(x - obok, y));
                    assert!(
                        t.height_dm(x, y) < brzeg,
                        "szerokie koryto ({x}, {y}) nie leży niżej niż brzeg"
                    );
                    szerokie += 1;
                }
            }
        }
        assert!(
            sprawdzone > 100,
            "sprawdzono tylko {sprawdzone} punktów koryta"
        );
        let _ = szerokie;
    }

    #[test]
    fn kontrakt_wysokosci_jest_w_jednostkach_pol_metra() {
        let t = teren(Region::Mountain, 2);
        for (x, y) in [(100, 100), (500, 1500), (3000, 200)] {
            assert_eq!(t.height_at(x, y), t.height_dm(x, y) / 5);
        }
    }

    #[test]
    fn kafel_zgadza_sie_z_zapytaniem_punktowym() {
        let t = teren(Region::Lowland, 7);
        let mut buf = [0i32; TILE_M * TILE_M];
        let tile = TileCoord::new(3, 2);
        t.height_tile(tile, &mut buf);
        let o = tile.origin_m();
        for (ly, lx) in [(0usize, 0usize), (10, 200), (255, 255), (128, 64)] {
            assert_eq!(
                buf[ly * TILE_M + lx],
                t.height_at(o.x + lx as i32, o.y + ly as i32),
                "kafel rozjechał się z zapytaniem punktowym w ({lx}, {ly})"
            );
        }
    }

    #[test]
    fn kazda_komorka_wodna_ma_wode_na_wierzchu_kolumny() {
        // Regresja z podglądu: interpolacja 1 m i uśrednianie w agregacie LOD potrafiły
        // wypchnąć dno ponad zwierciadło. Komórka zostawała wodna (więc bez pokrywy
        // biomu), ale wody w niej nie było — na ekranie jezioro dostawało łatę gołej
        // skały. Sprawdzamy oba tryby materializacji: dokładny i agregat.
        let t = teren(Region::Mountain, 0x1CE);
        let reg = t.materials();
        let woda = reg.expect_id("water");
        let dim = t.data().height.dim();
        for lod in [0u8, 2] {
            let span = (CHUNK_DIM as i32) << lod;
            let skala = 1i32 << lod;
            let mut sprawdzonych = 0;
            for gy in (0..dim).step_by(11) {
                for gx in (0..dim).step_by(11) {
                    let i = gy * dim + gx;
                    let Some(surf) = t.data().water_surface_dm(i) else {
                        continue;
                    };
                    let (x, y) = (
                        (gx * WORK_CELL_M as usize) as i32,
                        (gy * WORK_CELL_M as usize) as i32,
                    );
                    let z = i32::from(surf) / VOXEL_DM;
                    let coord = ChunkCoord::new(
                        x.div_euclid(span),
                        y.div_euclid(span),
                        z.div_euclid(span) as i16,
                    );
                    let mut b = ChunkBuilder::new(coord, lod, 1);
                    t.fill_chunk(coord, lod, &mut b);
                    let m = b.material_at(
                        (x - coord.x * span) / skala,
                        (y - coord.y * span) / skala,
                        (z - i32::from(coord.z) * span) / skala,
                    );
                    assert_eq!(
                        m, woda,
                        "brak wody na zwierciadle w ({x}, {y}) przy LOD{lod}"
                    );
                    sprawdzonych += 1;
                }
            }
            assert!(sprawdzonych > 50, "za mało komórek wodnych do sprawdzenia");
        }
    }

    #[test]
    fn granica_biomu_nie_biegnie_po_siatce_klimatu() {
        // Biom jest stały w komórce 256 m, więc bez rozmycia jego granica to bok tej
        // komórki — a przy kontrastowej pokrywie (torf przy jeziorze) widać z kilometra
        // kwadrat. Po warpie przejścia mają wypadać **między** wielokrotnościami 256 m.
        let t = teren(Region::Mountain, 0x1CE);
        let mut przejsc = 0;
        let mut na_siatce = 0;
        for y in (256..3800).step_by(256) {
            let mut poprzedni = t.biome_at(0, y);
            for x in 1..t.size_m() {
                let b = t.biome_at(x, y);
                if b != poprzedni {
                    przejsc += 1;
                    if x % (CLIMATE_CELL_M as i32) == 0 {
                        na_siatce += 1;
                    }
                    poprzedni = b;
                }
            }
        }
        assert!(przejsc > 50, "za mało granic biomów do oceny: {przejsc}");
        assert!(
            na_siatce * 4 < przejsc,
            "granice biomów wciąż trzymają się siatki klimatu: {na_siatce} z {przejsc}"
        );
    }

    #[test]
    fn woda_zawsze_ma_nieujemna_glebokosc_i_spojna_klase() {
        // M1 §7.2 `no_negative_water`.
        let t = teren(Region::Coastal, 4);
        for y in (0..t.size_m()).step_by(211) {
            for x in (0..t.size_m()).step_by(197) {
                let w = t.water_at(x, y);
                if w.class == WaterClass::Dry {
                    assert_eq!(w.depth_dm, 0, "sucha komórka z wodą w ({x}, {y})");
                } else {
                    assert!(t.biome_at(x, y).is_water(), "woda bez biomu wodnego");
                }
            }
        }
    }

    #[test]
    fn zabudowa_jest_wykluczona_na_wodzie_i_na_stromiznie() {
        let t = teren(Region::Mountain, 8);
        let mut znaleziono_woda = false;
        for y in (0..t.size_m()).step_by(101) {
            for x in (0..t.size_m()).step_by(103) {
                let b = t.buildability_at(x, y);
                if t.water_at(x, y).class != WaterClass::Dry {
                    assert!(!b.slope_ok, "zabudowa dopuszczona na wodzie w ({x}, {y})");
                    znaleziono_woda = true;
                }
                if t.slope_at(x, y) > BUILDABLE_MAX_SLOPE {
                    assert!(!b.slope_ok);
                }
                assert!(b.earthwork_cost_index >= 100);
            }
        }
        assert!(znaleziono_woda, "test nie trafił w żadną komórkę wodną");
    }

    #[test]
    fn ryzyko_powodziowe_maleje_z_wysokoscia_nad_woda() {
        let t = teren(Region::River, 6);
        let siec = t.river_network();
        let seg = siec
            .segments
            .iter()
            .max_by_key(|s| s.strahler)
            .expect("sieć rzeczna");
        let p = seg.points[seg.points.len() / 2];
        let przy_wodzie = t.flood_risk_at(p.0, p.1);

        // Punkt oddalony o kilometr od koryta ma ryzyko wyraźnie niższe.
        let daleko_x = (p.0 + 1200).min(t.size_m() - 1);
        let daleko = t.flood_risk_at(daleko_x, p.1);
        assert!(
            przy_wodzie.get() > daleko.get(),
            "ryzyko przy korycie {} nie jest wyższe niż o kilometr dalej {}",
            przy_wodzie.get(),
            daleko.get()
        );
        // Skróty M2 muszą dawać tę samą liczbę co pole w `Buildability`.
        assert_eq!(
            t.flood_risk_at(p.0, p.1),
            t.buildability_at(p.0, p.1).flood_risk
        );
    }

    #[test]
    fn skroty_m2_sa_aliasami_a_nie_drugim_modelem() {
        let t = teren(Region::Lowland, 3);
        for (x, y) in [(500, 500), (1200, 2400), (3000, 1000)] {
            assert_eq!(t.water_depth_at(x, y), t.water_at(x, y).depth_dm);
            assert_eq!(t.soil_quality_at(x, y), t.soil_fertility_at(x, y));
            assert_eq!(
                t.prevailing_wind(x, y),
                t.climate_at(x, y).prevailing_wind_deg
            );
            assert_eq!(t.flood_risk_at(x, y), t.buildability_at(x, y).flood_risk);
        }
    }

    #[test]
    fn maska_przeszkod_zgadza_sie_z_zapytaniami_punktowymi() {
        let t = teren(Region::River, 6);
        // Prostokąt zaczepiony na rzece — maska bez ani jednej przeszkody niczego nie sprawdza.
        let seg = t
            .river_network()
            .segments
            .iter()
            .max_by_key(|s| s.strahler)
            .unwrap();
        let p = seg.points[seg.points.len() / 2];
        let rect = IRect::from_size(IVec2::new((p.0 - 32).max(0), (p.1 - 32).max(0)), 64, 64);
        let m = t.obstacle_mask(rect);
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                let oczekiwane =
                    t.water_at(x, y).class.is_water() || t.slope_at(x, y) > BUILDABLE_MAX_SLOPE;
                assert_eq!(m.get(IVec2::new(x, y)), oczekiwane, "({x}, {y})");
            }
        }
        assert!(m.count_blocked() > 0, "prostokąt bez żadnej przeszkody");
    }

    #[test]
    fn przejscie_nad_rzeka_to_most() {
        let t = teren(Region::River, 6);
        let seg = t
            .river_network()
            .segments
            .iter()
            .max_by_key(|s| s.strahler)
            .unwrap();
        let p = seg.points[seg.points.len() / 2];
        let a = IVec2::new((p.0 - 60).max(0), p.1);
        let b = IVec2::new((p.0 + 60).min(t.size_m() - 1), p.1);
        assert!(
            matches!(t.crossing_cost(a, b), Crossing::Bridge { .. }),
            "przejście przez koryto nie wyszło mostem"
        );
        // Odcinek zerowy jest płaski, a nie mostem donikąd.
        assert_eq!(t.crossing_cost(a, a), Crossing::Flat);
    }

    #[test]
    fn kolumna_zgadza_sie_z_materializacja_voxeli() {
        // M1 §7.1 `column_matches_voxels`: to, co mówi `column_at`, musi wyjść z chunka.
        let t = teren(Region::Lowland, 5);
        let mut b = ChunkBuilder::new(ChunkCoord::new(4, 4, 1), 0, 0);
        t.fill_chunk(ChunkCoord::new(4, 4, 1), 0, &mut b);

        let origin = ChunkCoord::new(4, 4, 1).origin_voxels(0);
        let mut sprawdzone = 0;
        for ly in [0i32, 7, 15, 31] {
            for lx in [0i32, 3, 19, 31] {
                let (wx, wy) = (origin.x + lx, origin.y + ly);
                let c = t.column_at(wx, wy);
                for lz in 0..CHUNK_DIM as i32 {
                    let z_world = origin.z + lz;
                    if z_world > c.surface_z {
                        continue;
                    }
                    let oczekiwany = c.material_at(z_world).unwrap();
                    assert_eq!(
                        b.material_at(lx, ly, lz),
                        oczekiwany,
                        "voxel ({lx}, {ly}, {lz}) nie zgadza się z kolumną"
                    );
                    sprawdzone += 1;
                }
            }
        }
        assert!(sprawdzone > 50, "sprawdzono tylko {sprawdzone} voxeli");
    }

    #[test]
    fn zloza_da_sie_znalezc_po_prostokacie_i_po_kolumnie() {
        let t = teren(Region::Mountain, 11);
        assert!(!t.data.deposits.is_empty());
        let wszystkie = t.deposits_in(IRect::from_size(IVec2::ZERO, t.size_m(), t.size_m()));
        assert_eq!(
            wszystkie.len(),
            t.data.deposits.len(),
            "prostokąt mapy gubi złoża"
        );

        // Punkt nad środkiem złoża ma to złoże w kolumnie.
        let d = &t.data.deposits[0];
        let c = deposit_center(d);
        let w_kolumnie = t.deposits_at_column(c.x, c.y);
        assert!(
            w_kolumnie.contains(&d.id),
            "złoże {} nie jest widoczne w kolumnie nad własnym środkiem",
            d.id.0
        );
        assert_eq!(t.deposit(d.id).id, d.id);
    }
}
