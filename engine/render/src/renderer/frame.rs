//! Klatka: uniformy, culling stożkowy, listy rysowania i zapis argumentów pośrednich.
//!
//! Wydzielone z `renderer.rs` w R-WP3 bez zmiany zachowania.

use super::arena::Block;
use super::Renderer;
use crate::camera::CameraState;
use crate::clusters::ClusterConfig;
use crate::shadow::{self, Cascade};
use crate::sky::{exposure, sample_sky, sun_state, SkySample, SunState};
use glam::{Mat4, Vec3, Vec4};
use magnat_core::SimMinute;
use magnat_voxel::CHUNK_DIM;

/// Chunk zakwalifikowany do rysowania w tej klatce. Kopiuje same bloki areny, żeby
/// nagrywanie passów nie trzymało pożyczki na mapie chunków.
#[derive(Clone, Copy)]
pub(super) struct VisibleChunk {
    /// Indeks w buforze per-chunk — ten sam dla kadru i dla każdej kaskady.
    instance: u32,
    vertices: Block,
    indices: Block,
    opaque_indices: u32,
}

impl VisibleChunk {
    /// Zakres indeksów do rysowania w danym przebiegu.
    fn zakres(self, woda: bool) -> std::ops::Range<u32> {
        let start = self.indices.offset;
        if woda {
            start + self.opaque_indices..start + self.indices.len
        } else {
            start..start + self.opaque_indices
        }
    }

    fn pusty(self, woda: bool) -> bool {
        self.zakres(woda).is_empty()
    }
}

/// Listy rysowania jednej klatki: geometria nieprzezroczysta kadru, woda kadru
/// i po jednej liście na kaskadę cienia.
///
/// Listy są **rozłączne i już przefiltrowane**, bo ta sama kolejność rządzi zapisem
/// argumentów rysowania pośredniego. Filtrowanie w dwóch miejscach (raz przy zapisie
/// argumentów, raz przy rysowaniu) rozjeżdża się przy pierwszej zmianie warunku,
/// a objawem jest chunk rysujący cudzą geometrię.
pub(super) struct FrameLists {
    pub(super) widoczne: Vec<VisibleChunk>,
    pub(super) woda: Vec<VisibleChunk>,
    pub(super) cienie: [Vec<VisibleChunk>; shadow::CASCADES],
}

impl FrameLists {
    /// Kolejność list w buforze argumentów: kadr, woda, potem kaskady.
    pub(super) fn offset(&self, ktora: usize) -> u32 {
        let mut o = 0u32;
        for (i, l) in std::iter::once(&self.widoczne)
            .chain(std::iter::once(&self.woda))
            .chain(self.cienie.iter())
            .enumerate()
        {
            if i == ktora {
                break;
            }
            o += l.len() as u32;
        }
        o
    }
}

/// Argumenty `draw_indexed` dla ścieżki pośredniej. Układ jest kontraktem sterownika,
/// nie naszym — pięć `u32` w ustalonej kolejności.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

/// Dane ramki przekazywane shaderom. Układ **musi** odpowiadać `struct Frame` w WGSL.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    /// Macierze kaskad cieni, w kolejności od najbliższej.
    light_view_proj: [[[f32; 4]; 4]; shadow::CASCADES],
    /// x..w = koniec zakresu kolejnych kaskad w metrach od kamery.
    cascade_far: [f32; 4],
    /// x..w = rozmiar texela kaskady w metrach — wejście do biasu głębi w shaderze.
    cascade_texel: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sky_color: [f32; 4],
    ground_color: [f32; 4],
    fog: [f32; 4],
    clip: [f32; 4],
    /// xy = rozmiar okna w pikselach; shader dzieli go na kafle klastrów.
    /// zw = współczynniki odwrócenia bufora głębi.
    screen: [f32; 4],
    /// xyz = pozycja kamery w świecie. Potrzebna tam, gdzie shader musi odtworzyć
    /// **absolutną** współrzędną punktu — czyli przy próbkowaniu map pokrywających świat.
    eye: [f32; 4],
    /// x = bok komórki nakładki w metrach, y = jej wymiar, z = siła mieszania, w = czy aktywna.
    overlay: [f32; 4],
}

/// Per-chunk dane w SSBO: przesunięcie względem kamery i skala voxela.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
pub(super) struct ChunkUniform {
    origin_scale: [f32; 4],
}

/// Ile wpisów rysowania pośredniego mieści bufor: kadr plus cztery kaskady.
pub(super) const MAX_INDIRECT_ARGS: usize = MAX_DRAWN_CHUNKS * (1 + shadow::CASCADES);

/// Ile chunków naraz mieści bufor per-chunk. Przekroczenie obcina listę rysowania —
/// widok z orbity i tak nie pokazuje więcej niż kilka tysięcy chunków (M1 §5.9).
pub(super) const MAX_DRAWN_CHUNKS: usize = 8192;

/// Oczko siatki dalekiego terenu w metrach i jej rozmiar w quadach.
///
/// 32 m na 512 quadów to 16 km w poprzek — cała największa mapa (§5.1). Drobniejsze oczko
/// nie ma czego pokazać: siatka próbkuje mapę 4 m, a z odległości, na której w ogóle ją
/// widać, cztery metry to ułamek piksela.
const FAR_CELL_M: f32 = 32.0;
pub(super) const FAR_QUADS: u32 = 512;
/// Promień wyciętego środka siatki: tam rysują chunki (`LOD_RADII_M` sięga 4 km).
const FAR_INNER_M: f32 = 800.0;

impl Renderer {
    /// Wspólne przygotowanie klatki: uniformy, culling, lista widocznych chunków.
    pub(super) fn prepare_frame(
        &mut self,
        camera: &CameraState,
        minute: SimMinute,
        latitude_ddeg: i16,
        rozmiar: (u32, u32),
    ) -> FrameLists {
        let sun = sun_state(minute, latitude_ddeg);
        let sky = sample_sky(&self.sky, sun.elevation_deg);
        let aspect = rozmiar.0 as f32 / rozmiar.1.max(1) as f32;
        let view_proj = camera.view_proj_relative(aspect);
        let eye = camera.eye();
        let kaskady = shadow::cascades(&view_proj, sun.direction, camera.near);

        self.write_frame_uniform(
            &view_proj,
            &sun,
            &sky,
            camera,
            eye.z as f32,
            (eye.x as f32, eye.y as f32),
            &kaskady,
            rozmiar,
        );

        // Konfiguracja klastrów i macierz widoku — froxele opisują ten sam ostrosłup,
        // który widać w kadrze, więc muszą pochodzić z tej samej klatki.
        // Czas fal: klatka przy 60 Hz to 1/60 s. Liczymy go tutaj, a nie z zegara systemu,
        // bo zrzut offscreen ma wyglądać tak samo przy każdym uruchomieniu.
        self.czas_s += 1.0 / 60.0;
        self.gpu.queue.write_buffer(
            &self.time_buffer,
            0,
            bytemuck::cast_slice(&[self.czas_s, 0.0, 0.0, 0.0]),
        );

        // Siatka dalekiego terenu jest **zatrzaśnięta** do swojego oczka: bez tego
        // przesuwa się o ułamek oczka przy każdym ruchu kamery i cała odległa panorama
        // faluje, mimo że stoi w miejscu.
        let snap = |v: f64| (v / f64::from(FAR_CELL_M)).floor() * f64::from(FAR_CELL_M);
        let far = [
            self.far_cell_m,
            self.far_dim as f32,
            FAR_CELL_M,
            FAR_QUADS as f32,
            snap(eye.x) as f32,
            snap(eye.y) as f32,
            FAR_INNER_M,
            0.0,
            eye.x as f32,
            eye.y as f32,
            eye.z as f32,
            0.0,
        ];
        self.gpu
            .queue
            .write_buffer(&self.far_cfg, 0, bytemuck::cast_slice(&far));

        // Ekspozycja idzie **do post-processingu**, a nie do shaderów sceny: jedna krzywa
        // dla całego obrazu, a nie po kopii w każdym shaderze.
        let post = [
            exposure(sun.elevation_deg),
            self.fxaa,
            1.0 / rozmiar.0 as f32,
            1.0 / rozmiar.1.max(1) as f32,
        ];
        self.gpu
            .queue
            .write_buffer(&self.post_cfg, 0, bytemuck::cast_slice(&post));

        let cfg = ClusterConfig::new(camera.fov_deg, aspect, self.lights);
        self.gpu
            .queue
            .write_buffer(&self.cluster_config, 0, bytemuck::bytes_of(&cfg));
        let view = glam::camera::rh::view::look_at_mat4(
            Vec3::ZERO,
            (camera.target() - eye).as_vec3(),
            Vec3::Z,
        );
        self.gpu.queue.write_buffer(
            &self.view_buffer,
            0,
            bytemuck::cast_slice(&view.to_cols_array()),
        );

        // Bufor per-chunk opisuje **wszystkie** chunki rezydentne, a nie tylko widoczne
        // z kamery. Powód jest w cieniach: kaskada rysuje inny podzbiór niż kadr, a oba
        // passy indeksują ten sam bufor przez `instance_index`. Dwa bufory znaczyłyby dwie
        // numeracje i pierwszą pomyłkę przy pierwszej zmianie cullingu.
        let mut per_chunk: Vec<ChunkUniform> =
            Vec::with_capacity(self.chunks.len().min(MAX_DRAWN_CHUNKS));
        let mut wszystkie: Vec<(VisibleChunk, Vec3, f32)> =
            Vec::with_capacity(per_chunk.capacity());
        for ((_, coord), c) in &self.chunks {
            if per_chunk.len() >= MAX_DRAWN_CHUNKS {
                break;
            }
            let rel = Vec3::new(
                c.center_m[0] - eye.x as f32,
                c.center_m[1] - eye.y as f32,
                c.center_m[2] - eye.z as f32,
            );
            let skala = f32::from(1u16 << c.lod);
            let d = CHUNK_DIM as i32;
            let origin = [
                (coord.x * d * (1 << c.lod)) as f32 - eye.x as f32,
                (coord.y * d * (1 << c.lod)) as f32 - eye.y as f32,
                (i32::from(coord.z) * d * (1 << c.lod)) as f32 * 0.5 - eye.z as f32,
            ];
            per_chunk.push(ChunkUniform {
                origin_scale: [origin[0], origin[1], origin[2], skala],
            });
            wszystkie.push((
                VisibleChunk {
                    instance: (per_chunk.len() - 1) as u32,
                    vertices: c.vertices,
                    indices: c.indices,
                    opaque_indices: c.opaque_indices,
                },
                rel,
                c.radius_m,
            ));
        }
        if !per_chunk.is_empty() {
            self.gpu
                .queue
                .write_buffer(&self.chunk_buffer, 0, bytemuck::cast_slice(&per_chunk));
        }

        // Culling frustum po stronie CPU (§5.8). Test sfery otaczającej względem sześciu
        // płaszczyzn — tanio i wystarczająco: chunk odrzucony błędnie to chunk narysowany,
        // a nie chunk brakujący, więc błąd jest po bezpiecznej stronie. Ta sama funkcja
        // obsługuje kaskady, bo rzutowanie ortograficzne też ma sześć płaszczyzn.
        let planes = frustum_planes(&view_proj);
        let w_kadrze: Vec<VisibleChunk> = wszystkie
            .iter()
            .filter(|(_, rel, r)| sphere_in_frustum(&planes, *rel, *r))
            .map(|(v, _, _)| *v)
            .collect();
        let widoczne: Vec<VisibleChunk> = w_kadrze
            .iter()
            .filter(|c| !c.pusty(false))
            .copied()
            .collect();
        let woda: Vec<VisibleChunk> = w_kadrze
            .iter()
            .filter(|c| !c.pusty(true))
            .copied()
            .collect();

        // Woda nie rzuca cienia: tafla jest przezroczysta, a jej cień wyglądałby jak
        // czarna plama na dnie. Kaskady dostają więc samą geometrię nieprzezroczystą.
        let cienie: [Vec<VisibleChunk>; shadow::CASCADES] = std::array::from_fn(|i| {
            let planes = frustum_planes(&kaskady[i].view_proj);
            wszystkie
                .iter()
                .filter(|(c, rel, r)| !c.pusty(false) && sphere_in_frustum(&planes, *rel, *r))
                .map(|(v, _, _)| *v)
                .collect()
        });

        let listy = FrameLists {
            widoczne,
            woda,
            cienie,
        };
        self.write_indirect(&listy);
        listy
    }

    /// Argumenty rysowania pośredniego: kadr, a za nim kolejne kaskady — każdy widok
    /// dostaje własny zakres bufora, żeby jedno wywołanie pośrednie nie rysowało cudzej listy.
    fn write_indirect(&self, listy: &FrameLists) {
        let Some(buf) = self.indirect_buffer.as_ref() else {
            return;
        };
        let mut args: Vec<DrawArgs> = Vec::with_capacity(MAX_DRAWN_CHUNKS);
        let mut wpisz = |lista: &[VisibleChunk], woda: bool| {
            for c in lista {
                let zakres = c.zakres(woda);
                args.push(DrawArgs {
                    index_count: zakres.end - zakres.start,
                    instance_count: 1,
                    first_index: zakres.start,
                    base_vertex: c.vertices.offset as i32,
                    first_instance: c.instance,
                });
            }
        };
        wpisz(&listy.widoczne, false);
        wpisz(&listy.woda, true);
        for lista in &listy.cienie {
            wpisz(lista, false);
        }
        args.truncate(MAX_INDIRECT_ARGS);
        if !args.is_empty() {
            self.gpu
                .queue
                .write_buffer(buf, 0, bytemuck::cast_slice(&args));
        }
    }

    /// Rysuje listę chunków. Dwie ścieżki, jedna geometria: pośrednia, gdy sterownik ją ma,
    /// i wywołanie na chunk, gdy nie ma (ryzyko R6 fazy — WebGPU i część sterowników nie
    /// wystawiają multi-draw). Obie muszą dawać **ten sam obraz**, dlatego argumenty rysowania
    /// powstają z tej samej listy, a nie z osobnego przebiegu cullingu.
    ///
    /// `pierwszy_arg` to pozycja listy w buforze pośrednim: kadr zaczyna się od zera,
    /// a kaskady leżą za nim, jedna za drugą.
    pub(super) fn draw_list(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        bind: &wgpu::BindGroup,
        lista: &[VisibleChunk],
        woda: bool,
        pierwszy_arg: u32,
    ) -> usize {
        pass.set_bind_group(0, Some(bind), &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        let trojkaty = lista
            .iter()
            .map(|c| {
                let z = c.zakres(woda);
                ((z.end - z.start) / 3) as usize
            })
            .sum();

        let miesci_sie = (pierwszy_arg as usize + lista.len()) <= MAX_INDIRECT_ARGS;
        match self.indirect_buffer.as_ref() {
            Some(buf) if !lista.is_empty() && miesci_sie => {
                pass.multi_draw_indexed_indirect(
                    buf,
                    u64::from(pierwszy_arg) * 20,
                    lista.len() as u32,
                );
            }
            _ => {
                for c in lista {
                    pass.draw_indexed(
                        c.zakres(woda),
                        c.vertices.offset as i32,
                        c.instance..c.instance + 1,
                    );
                }
            }
        }
        trojkaty
    }

    // Osiem argumentów, bo tyle niezależnych wielkości opisuje klatkę. Opakowanie ich
    // w strukturę pośrednią dodałoby typ istniejący wyłącznie po to, żeby zaraz się
    // rozpakować — dokładnie ten sam przypadek co `PackedVertex::new` w `engine/voxel`.
    #[allow(clippy::too_many_arguments)]
    fn write_frame_uniform(
        &self,
        view_proj: &Mat4,
        sun: &SunState,
        sky: &SkySample,
        camera: &CameraState,
        eye_z: f32,
        eye_xy: (f32, f32),
        kaskady: &[Cascade; shadow::CASCADES],
        rozmiar: (u32, u32),
    ) {
        let clip_active = f32::from(u8::from(camera.clip_plane_z.is_some()));
        let clip_level = camera.clip_plane_z.map_or(0.0, |z| z as f32 * 0.5);
        let u = FrameUniform {
            view_proj: view_proj.to_cols_array_2d(),
            light_view_proj: std::array::from_fn(|i| kaskady[i].view_proj.to_cols_array_2d()),
            cascade_far: std::array::from_fn(|i| kaskady[i].far_m),
            cascade_texel: std::array::from_fn(|i| kaskady[i].texel_m),
            sun_dir: [
                sun.direction.x,
                sun.direction.y,
                sun.direction.z,
                exposure(sun.elevation_deg),
            ],
            sun_color: Vec4::from((sky.sun_color, sun.elevation_deg)).to_array(),
            sky_color: Vec4::from((sky.zenith, 0.0)).to_array(),
            ground_color: Vec4::from((sky.ground, 0.0)).to_array(),
            // Gęstość mgły: horyzont na granicy pierścienia LOD3 (4 km) ma być wyraźnie
            // zamglony, ale teren w promieniu kilometra — czysty. Przy 0,000 18 mgła zjadała
            // kontrast już na 500 m i cały widok wychodził jednolicie brązowy.
            fog: Vec4::from((sky.horizon, 0.000_06)).to_array(),
            clip: [clip_level, clip_active, eye_z, 0.0],
            // zw: współczynniki odwrócenia bufora głębi, `d = z / (ndc + w)`. Bez nich
            // odczytana głębia jest liczbą z przedziału [0, 1] o nieliniowym rozkładzie,
            // z której nie da się policzyć grubości słupa wody w metrach.
            eye: [eye_xy.0, eye_xy.1, eye_z, 0.0],
            overlay: self.overlay_cfg,
            screen: [
                rozmiar.0 as f32,
                rozmiar.1 as f32,
                camera.near * camera.far / (camera.near - camera.far),
                camera.far / (camera.near - camera.far),
            ],
        };
        self.gpu
            .queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&u));
    }
}

/// Sześć płaszczyzn frustum wyciągniętych z macierzy widok-rzutowanie (metoda Gribba–Hartmanna).
///
/// Każda płaszczyzna jako `(nx, ny, nz, d)` po normalizacji — normalizacja jest konieczna,
/// bo bez niej `dot` nie jest odległością i test sfery przestaje mieć sens.
#[must_use]
pub fn frustum_planes(vp: &Mat4) -> [Vec4; 6] {
    let m = vp.to_cols_array_2d();
    let wiersz = |i: usize| Vec4::new(m[0][i], m[1][i], m[2][i], m[3][i]);
    let (r0, r1, r2, r3) = (wiersz(0), wiersz(1), wiersz(2), wiersz(3));
    let plaszczyzny = [
        r3 + r0, // lewa
        r3 - r0, // prawa
        r3 + r1, // dolna
        r3 - r1, // górna
        r2,      // bliska (konwencja Z w [0, 1])
        r3 - r2, // daleka
    ];
    plaszczyzny.map(|p| {
        let len = p.truncate().length();
        if len > 0.0 {
            p / len
        } else {
            p
        }
    })
}

/// Test sfery względem frustum. `center` jest **względem kamery**, tak jak cała geometria.
#[must_use]
pub fn sphere_in_frustum(planes: &[Vec4; 6], center: Vec3, radius: f32) -> bool {
    planes
        .iter()
        .all(|p| p.truncate().dot(center) + p.w >= -radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frustum_odrzuca_to_co_za_plecami() {
        let cam = CameraState::default();
        let vp = cam.view_proj_relative(1.6);
        let planes = frustum_planes(&vp);
        let przed = (cam.target() - cam.eye()).as_vec3().normalize() * 100.0;
        assert!(
            sphere_in_frustum(&planes, przed, 20.0),
            "punkt przed kamerą został odrzucony"
        );
        assert!(
            !sphere_in_frustum(&planes, -przed, 5.0),
            "punkt za kamerą przeszedł test frustum"
        );
    }

    #[test]
    fn frustum_przepuszcza_sfere_dotykajaca_krawedzi() {
        // Błąd po bezpiecznej stronie: chunk na granicy ma zostać narysowany,
        // a nie zniknąć — zniknięcie jest widoczne, nadmiarowe rysowanie nie.
        let cam = CameraState::default();
        let vp = cam.view_proj_relative(1.6);
        let planes = frustum_planes(&vp);
        let daleko_w_bok = Vec3::new(5000.0, 0.0, 0.0);
        assert!(!sphere_in_frustum(&planes, daleko_w_bok, 1.0));
        assert!(
            sphere_in_frustum(&planes, daleko_w_bok, 100_000.0),
            "sfera obejmująca kamerę została odrzucona"
        );
    }
}
