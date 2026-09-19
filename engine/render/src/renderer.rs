//! Renderer: arena geometrii, passy i klatka (M1 WP-R1, R2, R4, R6).
//!
//! Geometria wszystkich chunków leży w **jednej parze buforów** (wierzchołki i indeksy),
//! podzielonej suballokatorem z listą wolnych bloków. Powód jest po stronie sterownika:
//! bufor na chunk znaczy 8000 wiązań na klatkę i tyle samo sprawdzeń walidacji, a jeden
//! bufor z przesunięciem znaczy jedno wiązanie i tyle rysowań, ile widocznych chunków.
//!
//! **Rysowanie idzie per chunk**, nie przez `multi_draw_indexed_indirect`. To jest ścieżka
//! zapasowa z ryzyka R6 fazy i zarazem jedyna, jaka działa na każdym backendzie `wgpu`.
//! Budżet CPU ją unosi (≤ 4000 wywołań rysowania mieści się w 1,5 ms zapisu komend, §5.9),
//! a droga do wersji pośredniej nie zmienia ani formatu wierzchołka, ani układu areny:
//! zmienia się jedna pętla.

mod arena;
mod frame;
mod offscreen;
mod passes;
mod pipelines;
mod scene;
mod stats;

use crate::camera::CameraState;
use crate::clusters::{self, GpuLight};
use crate::gpu::GpuContext;
use crate::pick;
use crate::sky::{sky_lut, SkySample, SKY_LUT_SIZE};
use crate::ui::UiLayer;
use arena::{Arena, GpuChunk};
use magnat_voxel::{ChunkCoord, MaterialRegistry};
use pipelines::{
    create_depth, create_hdr, create_overlay_bind, create_post_bind, create_water_bind,
};
use stats::PassTimer;
use std::collections::BTreeMap;

pub use frame::{frustum_planes, sphere_in_frustum};
pub use stats::{FrameStats, PASS_NAMES};

/// Nakładka terenowa: pole skalarne na siatce świata plus paleta (M1 §6.1, WP-R6).
pub struct TerrainOverlay<'a> {
    /// Bok komórki pola w metrach.
    pub cell_m: f32,
    /// Wymiar pola w komórkach (kwadratowe).
    pub dim: u32,
    /// Wartość na komórkę — indeks do palety.
    pub values: &'a [u8],
    /// 256 barw RGBA.
    pub palette: &'a [[u8; 4]; 256],
    /// Siła mieszania z barwą terenu, 0…1.
    pub strength: f32,
}

/// Format bufora sceny: HDR, bo ekspozycja i tonemap dzieją się dopiero w post-processingu.
/// Ósemkowy bufor obciąłby jasne końce **przed** krzywą i zachód słońca wychodziłby biały.
pub(crate) const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct Renderer {
    pub gpu: GpuContext,
    pipeline: wgpu::RenderPipeline,
    depth_pipeline: wgpu::RenderPipeline,
    /// Ten sam prepass z cięciem poziomami. Bez niego przekrój jest czarną dziurą:
    /// prepass bez shadera fragmentu zapisuje głębię także tam, gdzie pass
    /// nieprzezroczysty zaraz odrzuci fragment (M11c §5.7).
    depth_clip_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    frame_buffer: wgpu::Buffer,
    chunk_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    /// Argumenty rysowania pośredniego — jeden wpis na chunk widoczny w tej klatce.
    /// Istnieje tylko wtedy, gdy sterownik ma multi-draw; inaczej idzie ścieżka zapasowa.
    indirect_buffer: Option<wgpu::Buffer>,
    depth_view: wgpu::TextureView,
    cluster_pipeline: wgpu::ComputePipeline,
    cluster_bind: wgpu::BindGroup,
    cluster_config: wgpu::Buffer,
    view_buffer: wgpu::Buffer,
    light_buffer: wgpu::Buffer,
    cluster_counts: wgpu::Buffer,
    occupancy_readback: wgpu::Buffer,
    occupancy: magnat_devtools::ClusterOccupancy,
    /// Odczyt histogramu w toku — ten sam mechanizm co przy znacznikach czasu.
    occupancy_w_locie: std::sync::Arc<std::sync::atomic::AtomicBool>,
    occupancy_gotowe: std::sync::Arc<std::sync::atomic::AtomicBool>,
    lights: u32,
    /// Parametry nakładki terenowej w postaci, w jakiej trafiają do uniformu ramki.
    overlay_cfg: [f32; 4],
    overlay_bind: wgpu::BindGroup,
    overlay_layout: wgpu::BindGroupLayout,
    post_pipeline: wgpu::RenderPipeline,
    post_layout: wgpu::BindGroupLayout,
    post_bind: wgpu::BindGroup,
    post_cfg: wgpu::Buffer,
    post_sampler: wgpu::Sampler,
    hdr_view: wgpu::TextureView,
    /// Siła FXAA, 0 = wyłączone. Zmienna, bo to pierwsza rzecz, którą się wyłącza,
    /// oceniając ostrość obrazu.
    fxaa: f32,
    far_pipeline: wgpu::RenderPipeline,
    far_layout: wgpu::BindGroupLayout,
    far_cfg: wgpu::Buffer,
    /// Grupa wiązań dalekiego terenu powstaje dopiero wtedy, gdy mapa zostanie wgrana —
    /// bez niej pass nie ma czego rysować i jest pomijany.
    far_bind: Option<wgpu::BindGroup>,
    far_dim: u32,
    far_cell_m: f32,
    water_pipeline: wgpu::RenderPipeline,
    water_layout: wgpu::BindGroupLayout,
    water_bind: wgpu::BindGroup,
    time_buffer: wgpu::Buffer,
    /// Czas renderu w sekundach — napędza fale. Nie zegar symulacji: tafla ma falować
    /// także przy pauzie, bo pauza zatrzymuje gospodarkę, a nie wodę.
    czas_s: f32,
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_bind: wgpu::BindGroup,
    shadow_layers: Vec<wgpu::TextureView>,
    /// Po jednej grupie na kaskadę — niosą wyłącznie jej numer, żeby ten sam shader
    /// mógł sięgnąć po właściwą macierz z uniformu ramki.
    cascade_binds: Vec<wgpu::BindGroup>,
    vertex_arena: Arena,
    index_arena: Arena,
    /// Klucz niesie **LOD razem ze współrzędną**, bo `ChunkCoord` jest współrzędną
    /// względną dla poziomu szczegółowości: `(63, 3, 0)` przy LOD0 i przy LOD1 to dwa
    /// różne miejsca w świecie. Przy kluczu z samej współrzędnej chunki sąsiednich
    /// pierścieni nadpisywały się nawzajem — i wtedy w kadrze były dziury dokładnie tam,
    /// gdzie grubszy pierścień wyparł drobniejszy (najlepiej widoczne na taflach jezior).
    chunks: BTreeMap<(u8, ChunkCoord), GpuChunk>,
    sky: [SkySample; SKY_LUT_SIZE],
    timer: Option<PassTimer>,
    stats: FrameStats,
    /// Budżet klatki: patrzy na czas GPU i skaluje progi detalu (§5.10, WP10).
    /// Publiczny, bo cel zależy od trybu kamery, a ten zna klient.
    pub budget: crate::budget::RenderBudget,
    /// Piesi i bufor ID (M3 §5.11). Osobny moduł, bo to jedyny pass, który rysuje
    /// **encje symulacji**, a nie teren — i jedyny, który czyta z GPU z powrotem.
    pub pick_buffer: pick::PickBuffer,
    pub instances: crate::instancing::InstanceRenderer,
    /// Domknięcie przekroju budynków (M11c §5.7). Osobny potok, bo czapka jest płaskim
    /// wielokątem bez indeksów i bez normalnych — geometria chunka nie ma jak jej udawać.
    cap: crate::interiors::CapRenderer,
    /// Czapki tej klatki po stronie procesora — trzymane tu z tego samego powodu
    /// co `scratch`: klatka nie alokuje.
    cap_geom: crate::interiors::CapGeometry,
    /// Płaszczyzna przekroju obowiązująca w tej klatce. Ustawia ją klient, bo to on
    /// wie, na który budynek patrzy gracz i jakie ma stropy.
    cut: crate::interiors::CutPlane,
    /// Wyposażenie wnętrz widocznych w tej klatce. Składa je klient przez
    /// [`Renderer::set_interiors`], bo generator potrzebuje kondygnacji z `CityData`.
    props: Vec<crate::interiors::PropPlacement>,
    /// Cząstki opadu i dymu (M11d §5.8, WP7) — dwa potoki, dwa wywołania rysowania.
    weather_fx: crate::weather::WeatherRenderer,
    /// Pogoda tej klatki. Wchodzi do uniformu ramki (śnieg, wilgoć, sezon, mgła)
    /// i do cząstek. Domyślnie pogodnie — scena bez snapshotu ma być czysta, a nie
    /// zasypana śniegiem z niezainicjalizowanej pamięci.
    weather: magnat_sim_snapshot::WeatherState,
    /// Napisy na szyldach (M11c §5.7, WP9) — osobny potok, jedno wywołanie rysowania.
    signs: crate::signs::SignRenderer,
    sign_geom: crate::signs::SignGeometry,
    /// Bufory robocze ścieżki klatki — trzymane tu, żeby klatka nie alokowała.
    scratch: crate::instancing::InstanceScratch,
    /// Warstwa `egui` (decyzja 9.2). Powstaje zawsze; klatka bez panelu po prostu
    /// jej nie woła, a kosztem jest jeden potok i jeden bufor uniformów.
    ui: UiLayer,
}

impl Renderer {
    /// Tworzy renderer. Katalogi modeli i palet przychodzą **z zewnątrz**, tak samo jak
    /// rejestr materiałów: `engine/render` rysuje dane, a nie czyta katalogu `data/`.
    pub fn new(
        gpu: GpuContext,
        materials: &MaterialRegistry,
        models: &magnat_voxel::ModelLibrary,
        palettes: &magnat_voxel::PaletteLibrary,
    ) -> Renderer {
        let timer = PassTimer::new(&gpu);
        let device = &gpu.device;

        // Kolejność wywołań jest kontraktem sterownika, nie stylem: indeksy grup wiązań
        // i potoków biorą się z kolejności tworzenia. Każda funkcja `pipelines::*` niżej
        // to ciągły wycinek dawnego `new`, wołany dokładnie w dawnej kolejności.
        let buf = pipelines::buffers(device, materials, gpu.multi_draw_indirect);
        let terrain = pipelines::terrain_layouts(device);
        let cl = pipelines::cluster(device);
        let (water_layout, time_buffer) = pipelines::water_layout(device);
        let (shadow_layers, bind_group) = pipelines::main_bind(device, &terrain.layout, &buf, &cl);
        let (cascade_layout, cascade_binds) = pipelines::cascades(device);
        let layouts =
            pipelines::pipeline_layouts(device, &terrain, &water_layout, &cascade_layout, &buf);
        let scene = pipelines::scene_pipelines(device, &layouts);
        let (far_layout, far_cfg, far_pipeline) = pipelines::far(device, &terrain);
        let depth_view = create_depth(device, gpu.config.width, gpu.config.height);
        let water_bind = create_water_bind(device, &water_layout, &depth_view, &time_buffer);
        let post = pipelines::post(device, gpu.config.format);
        let hdr_view = create_hdr(device, gpu.config.width, gpu.config.height);
        let post_bind = create_post_bind(
            device,
            &post.post_layout,
            &hdr_view,
            &post.post_sampler,
            &post.post_cfg,
        );
        let instances = crate::instancing::InstanceRenderer::new(
            device,
            &gpu.queue,
            &buf.frame_buffer,
            models,
            palettes,
        );
        let pick_buffer = pick::PickBuffer::new(device, gpu.config.width, gpu.config.height);
        let cap = crate::interiors::CapRenderer::new(
            device,
            &buf.frame_buffer,
            wgpu::TextureFormat::Depth32Float,
        );
        let signs = crate::signs::SignRenderer::new(
            device,
            &buf.frame_buffer,
            wgpu::TextureFormat::Depth32Float,
        );
        let weather_fx = crate::weather::WeatherRenderer::new(
            device,
            &buf.frame_buffer,
            wgpu::TextureFormat::Depth32Float,
        );
        let ui = UiLayer::new(device, gpu.config.format);

        Renderer {
            gpu,
            pipeline: scene.pipeline,
            depth_pipeline: scene.depth_pipeline,
            depth_clip_pipeline: scene.depth_clip_pipeline,
            sky_pipeline: scene.sky_pipeline,
            bind_group,
            frame_buffer: buf.frame_buffer,
            chunk_buffer: buf.chunk_buffer,
            vertex_buffer: buf.vertex_buffer,
            index_buffer: buf.index_buffer,
            indirect_buffer: buf.indirect_buffer,
            depth_view,
            cluster_pipeline: cl.cluster_pipeline,
            cluster_bind: cl.cluster_bind,
            cluster_config: cl.cluster_config,
            view_buffer: cl.view_buffer,
            light_buffer: cl.light_buffer,
            cluster_counts: cl.cluster_counts,
            occupancy_readback: cl.occupancy_readback,
            occupancy: magnat_devtools::ClusterOccupancy::default(),
            occupancy_w_locie: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            occupancy_gotowe: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            lights: 0,
            overlay_cfg: [4.0, 1.0, 0.0, 0.0],
            overlay_bind: terrain.overlay_bind,
            overlay_layout: terrain.overlay_layout,
            post_pipeline: post.post_pipeline,
            post_layout: post.post_layout,
            post_bind,
            post_cfg: post.post_cfg,
            post_sampler: post.post_sampler,
            hdr_view,
            fxaa: 0.75,
            far_pipeline,
            far_layout,
            far_cfg,
            far_bind: None,
            far_dim: 0,
            far_cell_m: 4.0,
            water_pipeline: scene.water_pipeline,
            water_layout,
            water_bind,
            time_buffer,
            czas_s: 0.0,
            shadow_pipeline: scene.shadow_pipeline,
            shadow_bind: layouts.shadow_bind,
            shadow_layers,
            cascade_binds,
            vertex_arena: Arena::new(buf.vertex_capacity),
            index_arena: Arena::new(buf.index_capacity),
            chunks: BTreeMap::new(),
            sky: sky_lut(),
            timer,
            stats: FrameStats::default(),
            budget: crate::budget::RenderBudget::default(),
            pick_buffer,
            instances,
            cap,
            cap_geom: crate::interiors::CapGeometry::default(),
            cut: crate::interiors::CutPlane::off(),
            props: Vec::new(),
            weather_fx,
            weather: magnat_sim_snapshot::WeatherState::default(),
            signs,
            sign_geom: crate::signs::SignGeometry::default(),
            scratch: crate::instancing::InstanceScratch::default(),
            ui,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        self.depth_view = create_depth(
            &self.gpu.device,
            self.gpu.config.width,
            self.gpu.config.height,
        );
        // Grupy wiązań trzymają widoki tekstur, więc po ich odtworzeniu muszą powstać
        // na nowo — inaczej woda czytałaby głębię o starym rozmiarze, a post-processing
        // scenę o starym.
        self.water_bind = create_water_bind(
            &self.gpu.device,
            &self.water_layout,
            &self.depth_view,
            &self.time_buffer,
        );
        self.hdr_view = create_hdr(
            &self.gpu.device,
            self.gpu.config.width,
            self.gpu.config.height,
        );
        self.post_bind = create_post_bind(
            &self.gpu.device,
            &self.post_layout,
            &self.hdr_view,
            &self.post_sampler,
            &self.post_cfg,
        );
        self.pick_buffer.resize(
            &self.gpu.device,
            self.gpu.config.width,
            self.gpu.config.height,
        );
    }

    #[must_use]
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    /// Ustawia nakładkę terenową albo ją zdejmuje (`None`).
    ///
    /// To jest **mechanizm z §6.1, nie zestaw nakładek**: M1 używa go do ośmiu podglądów
    /// z §1 (wysokość, spływ, klasa wody, biom, temperatura, opady, geologia, złoża),
    /// a M2 podepnie pod niego własne pola — wartość dzielnicy, dostępność, hałas. Renderer
    /// nie wie, co znaczy liczba w polu; wie tylko, że ma ją zamienić na barwę z palety.
    ///
    /// `values` to jeden bajt na komórkę siatki `cell_m`, `palette` to 256 barw RGBA.
    pub fn set_overlay(&mut self, overlay: Option<TerrainOverlay<'_>>) {
        let Some(o) = overlay else {
            self.overlay_cfg[3] = 0.0;
            return;
        };
        assert_eq!(
            o.values.len(),
            (o.dim * o.dim) as usize,
            "pole nakładki ma inny wymiar niż zadeklarowany"
        );
        let device = &self.gpu.device;
        let rozmiar = wgpu::Extent3d {
            width: o.dim,
            height: o.dim,
            depth_or_array_layers: 1,
        };
        // Pole idzie jako RGBA8, a nie R8: w wartości siedzi tylko czerwony kanał, ale
        // jeden format tekstury dla pola i palety oznacza jeden układ wiązań i jedną
        // ścieżkę kodu. Cztery bajty na komórkę to przy siatce 4 m ułamek budżetu.
        let pole = {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("render.overlay.pole"),
                size: rozmiar,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &t,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                o.values,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(o.dim),
                    rows_per_image: Some(o.dim),
                },
                rozmiar,
            );
            t
        };
        let paleta = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render.overlay.paleta"),
            size: wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &paleta,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(o.palette.as_slice()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        self.overlay_bind = create_overlay_bind(device, &self.overlay_layout, &pole, &paleta);
        self.overlay_cfg = [o.cell_m, o.dim as f32, o.strength, 1.0];
    }

    /// Wgrywa mapę dalekiego terenu: wysokości w decymetrach i zapieczony kolor powierzchni,
    /// oba na siatce `cell_m` (tej samej, na której liczona jest hydrologia).
    ///
    /// Wołane **raz po wygenerowaniu świata**, nie co klatkę: mapa 16 km to 4096 × 4096
    /// komórek, czyli 32 MB wysokości i 64 MB koloru. Przesyłanie tego w pętli klatki
    /// zjadłoby całą przepustowość magistrali na dane, które się nie zmieniają.
    pub fn upload_far_terrain(&mut self, dim: u32, cell_m: f32, heights_dm: &[i16], albedo: &[u8]) {
        assert_eq!(
            heights_dm.len(),
            (dim * dim) as usize,
            "mapa wysokości ma inny wymiar niż zadeklarowany"
        );
        assert_eq!(
            albedo.len(),
            (dim * dim * 4) as usize,
            "mapa koloru ma inny wymiar niż zadeklarowany"
        );
        let device = &self.gpu.device;
        let rozmiar = wgpu::Extent3d {
            width: dim,
            height: dim,
            depth_or_array_layers: 1,
        };
        let wysokosci = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render.far.height"),
            size: rozmiar,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Sint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &wysokosci,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(heights_dm),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(dim * 2),
                rows_per_image: Some(dim),
            },
            rozmiar,
        );
        let kolor = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render.far.albedo"),
            size: rozmiar,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &kolor,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            albedo,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(dim * 4),
                rows_per_image: Some(dim),
            },
            rozmiar,
        );

        self.far_dim = dim;
        self.far_cell_m = cell_m;
        self.far_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render.far.bind"),
            layout: &self.far_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.far_cfg.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &wysokosci.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &kolor.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
            ],
        }));
    }

    /// Podaje światła punktowe tej klatki. Pozycje są w metrach świata — przeliczenie
    /// na układ kamery dzieje się tutaj, bo tylko renderer wie, gdzie jest oko.
    ///
    /// Do M11 źródłem jest snapshot (`RenderSnapshot::lights`); w M1 służy to scenie
    /// pomiarowej z §4 (4096 świateł ≤ 0,4 ms na przypisanie).
    pub fn set_lights(&mut self, lights: &[magnat_sim_snapshot::LightRecord], eye: glam::DVec3) {
        let dane: Vec<GpuLight> = lights
            .iter()
            .take(clusters::MAX_LIGHTS)
            .map(|l| clusters::to_gpu(l, eye))
            .collect();
        self.lights = dane.len() as u32;
        if !dane.is_empty() {
            self.gpu
                .queue
                .write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(&dane));
        }
    }

    /// Podaje pogodę tej klatki: cząstki opadu, kominy i parametry palety terenu.
    ///
    /// Osobno od [`Renderer::set_entities`], choć wejściem jest ten sam snapshot, bo to
    /// są dwa różne budżety: instancje mają swój cap i swój pass, cząstki swój. Wołający,
    /// który zapomni o jednym z nich, ma dostać scenę bez pogody, a nie scenę bez encji.
    pub fn set_weather(
        &mut self,
        snapshot: &magnat_sim_snapshot::RenderSnapshot,
        eye: glam::DVec3,
    ) {
        self.weather = snapshot.weather;
        let czas = self.czas_s;
        self.weather_fx.set(snapshot, eye, czas, &self.gpu.queue);
    }

    /// Ile cząstek pogody poszło w ostatniej klatce — do raportu i do progu z WP7.
    #[must_use]
    pub fn weather_particles(&self) -> u32 {
        self.weather_fx.particles()
    }

    /// Składa bufor instancji encji dynamicznych z opublikowanego snapshotu (M11a WP2).
    ///
    /// Odcięcie stożkiem i dobór poziomu detalu dzieją się **tutaj**, a nie po stronie
    /// symulacji: jedno i drugie zależy od kamery, a kamera nie wchodzi do hasha stanu
    /// (00 §4). Snapshot mówi, kto jest w kadrze; renderer — jak go narysować.
    pub fn set_entities(
        &mut self,
        snapshot: &magnat_sim_snapshot::RenderSnapshot,
        camera: &CameraState,
    ) {
        let vp = camera.view_proj_relative(self.gpu.aspect());
        let planes = frustum_planes(&vp);
        // Stożek jest liczony w układzie **względem kamery** (`view_proj_relative`),
        // więc test przesłania też dostaje pozycję względną — `build_instances`
        // odejmuje origin w `f64` przed rzutowaniem na `f32` (`R11`).
        // Próg rysowania liczy się z rozmiaru ekranowego encji, więc zależy od FOV
        // i od wysokości kadru — a nie od stałej, która przy orbicie 900 m odcinała
        // wszystko, na co gracz patrzy (`J-3`).
        // Skala budżetu klatki wchodzi **tu i tylko tu** (§5.10): progi odległości
        // po stronie GPU, nic po stronie symulacji.
        let bands = crate::instancing::LodBands::for_view(
            self.instances.models(),
            camera.fov_deg,
            self.gpu.config.height.max(1) as f32,
        )
        .scaled(self.budget.lod_scale());
        crate::instancing::build_instances(
            snapshot,
            camera.eye(),
            &planes,
            self.instances.models(),
            bands,
            &self.props,
            &mut self.scratch,
        );
        self.instances.upload(
            &self.gpu.queue,
            &self.scratch.instances,
            &self.scratch.batches,
        );
    }

    /// Kursor w pikselach okna albo `None`, gdy wyszedł poza nie. Ustawia, który piksel
    /// bufora ID klatka skopiuje — bez tego `hovered` zawsze zwraca `None`.
    pub fn set_cursor(&mut self, pos: Option<(u32, u32)>) {
        self.pick_buffer.set_cursor(pos);
    }

    /// W co gracz celuje kursorem albo `None` (decyzja 9.3). Wartość pochodzi z klatki
    /// poprzedniej — patrz nagłówek `pick.rs`. Rodzaj encji jest w wyniku, bo mieszkaniec
    /// i pojazd mają osobne karty (`K-62`).
    #[must_use]
    pub fn pick(&self) -> Option<crate::instancing::PickHit> {
        self.pick_buffer.hovered()
    }

    /// Histogram zajętości klastrów z ostatniego odczytu (przyrząd z §6.1 dla M2/M11).
    #[must_use]
    pub fn cluster_occupancy(&self) -> &magnat_devtools::ClusterOccupancy {
        &self.occupancy
    }
}

#[cfg(test)]
mod tests {
    /// Deklaracja `struct Frame` musi być **identyczna** we wszystkich shaderach sceny.
    ///
    /// Uniform to jeden bufor, a każdy shader czyta go po swojemu: shader z nieaktualną
    /// deklaracją nie zgłasza błędu, tylko odczytuje pola spod cudzych przesunięć. Tak
    /// właśnie stało się przy dokładaniu kaskad cieni — `sky.wgsl` został przy starym
    /// układzie i zaczął brać barwę nieba z macierzy światła, przez co niebo wychodziło
    /// białawe. Objaw wyglądał jak zła paleta, a był rozjazdem układu pamięci.
    #[test]
    fn uklad_uniformu_ramki_jest_ten_sam_we_wszystkich_shaderach() {
        let shadery = [
            ("voxel", include_str!("shaders/voxel.wgsl")),
            ("water", include_str!("shaders/water.wgsl")),
            ("sky", include_str!("shaders/sky.wgsl")),
            ("far_terrain", include_str!("shaders/far_terrain.wgsl")),
            ("instance", include_str!("shaders/instance.wgsl")),
        ];
        let pola = |src: &str| -> Vec<String> {
            let start = src.find("struct Frame {").expect("brak struktury Frame");
            let body = &src[start + "struct Frame {".len()..];
            let end = body.find('}').expect("niedomknięta struktura Frame");
            body[..end]
                .lines()
                .filter_map(|l| l.split("//").next())
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(|l| l.trim_end_matches(',').to_string())
                .collect()
        };
        let wzorzec = pola(shadery[0].1);
        assert!(wzorzec.len() > 5, "wzorzec wygląda na pusty: {wzorzec:?}");
        for (nazwa, src) in &shadery[1..] {
            assert_eq!(
                pola(src),
                wzorzec,
                "shader {nazwa} ma inny układ `Frame` niż voxel"
            );
        }
    }
}
