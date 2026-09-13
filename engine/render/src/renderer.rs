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

use crate::camera::CameraState;
use crate::gpu::GpuContext;
use crate::sky::{exposure, sample_sky, sky_lut, sun_state, SkySample, SunState, SKY_LUT_SIZE};
use glam::{Mat4, Vec3, Vec4};
use magnat_core::SimMinute;
use magnat_voxel::{ChunkCoord, ChunkMesh, MaterialRegistry, CHUNK_DIM};
use std::collections::BTreeMap;

/// Blok w arenie: przesunięcie i długość w elementach.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Block {
    offset: u32,
    len: u32,
}

/// Suballokator listy wolnych bloków.
///
/// Pierwszy pasujący blok, nie najlepszy: mesh chunka ma rozmiar rzędu kilku kilobajtów
/// i rotuje przy każdej zmianie LOD, więc szukanie najlepszego dopasowania kosztowałoby
/// więcej niż fragmentacja, której ma zapobiec.
struct Arena {
    capacity: u32,
    free: Vec<Block>,
    used: u32,
}

impl Arena {
    fn new(capacity: u32) -> Arena {
        Arena {
            capacity,
            free: vec![Block {
                offset: 0,
                len: capacity,
            }],
            used: 0,
        }
    }

    fn alloc(&mut self, len: u32) -> Option<Block> {
        if len == 0 {
            return Some(Block { offset: 0, len: 0 });
        }
        let i = self.free.iter().position(|b| b.len >= len)?;
        let b = self.free[i];
        if b.len == len {
            self.free.remove(i);
        } else {
            self.free[i] = Block {
                offset: b.offset + len,
                len: b.len - len,
            };
        }
        self.used += len;
        Some(Block {
            offset: b.offset,
            len,
        })
    }

    fn free_block(&mut self, b: Block) {
        if b.len == 0 {
            return;
        }
        self.used -= b.len;
        // Scalanie z sąsiadami przy zwalnianiu — bez tego arena rozsypuje się na tysiąc
        // dziur po godzinie latania kamerą i przestaje mieścić cokolwiek, mimo że jest pusta.
        self.free.push(b);
        self.free.sort_by_key(|x| x.offset);
        let mut scalone: Vec<Block> = Vec::with_capacity(self.free.len());
        for blok in self.free.drain(..) {
            match scalone.last_mut() {
                Some(p) if p.offset + p.len == blok.offset => p.len += blok.len,
                _ => scalone.push(blok),
            }
        }
        self.free = scalone;
    }

    fn bytes_used(&self, stride: usize) -> usize {
        self.used as usize * stride
    }

    fn capacity_bytes(&self, stride: usize) -> usize {
        self.capacity as usize * stride
    }
}

/// Wpis chunka rezydentnego na GPU.
struct GpuChunk {
    vertices: Block,
    indices: Block,
    lod: u8,
    /// Środek chunka w metrach — do cullingu.
    center_m: [f32; 3],
    radius_m: f32,
    /// Rewizja, dla której zbudowano ten mesh; rozjazd wymusza przebudowę.
    revision: u32,
}

/// Chunk zakwalifikowany do rysowania w tej klatce. Kopiuje same bloki areny, żeby
/// nagrywanie passów nie trzymało pożyczki na mapie chunków.
#[derive(Clone, Copy)]
struct VisibleChunk {
    vertices: Block,
    indices: Block,
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
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sky_color: [f32; 4],
    ground_color: [f32; 4],
    fog: [f32; 4],
    clip: [f32; 4],
}

/// Per-chunk dane w SSBO: przesunięcie względem kamery i skala voxela.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct ChunkUniform {
    origin_scale: [f32; 4],
}

/// Ile chunków naraz mieści bufor per-chunk. Przekroczenie obcina listę rysowania —
/// widok z orbity i tak nie pokazuje więcej niż kilka tysięcy chunków (M1 §5.9).
const MAX_DRAWN_CHUNKS: usize = 8192;

/// Nazwy mierzonych passów — indeks odpowiada parze znaczników w `QuerySet`.
pub const PASS_NAMES: [&str; 2] = ["depth_prepass", "opaque"];

/// Statystyki klatki — wejście do licznika w tytule okna i do raportu z §7.4.
#[derive(Clone, Copy, Default, Debug)]
pub struct FrameStats {
    pub chunks_resident: usize,
    pub chunks_drawn: usize,
    pub triangles: usize,
    pub vertex_bytes: usize,
    pub index_bytes: usize,
    pub arena_capacity_bytes: usize,
    /// Czas GPU każdego passa w milisekundach, w kolejności [`PASS_NAMES`].
    /// Zera, gdy sterownik nie ma `TIMESTAMP_QUERY` — patrz [`FrameStats::gpu_timing`].
    pub pass_ms: [f32; PASS_NAMES.len()],
    /// Czy `pass_ms` niesie pomiar, czy tylko zera.
    pub gpu_timing: bool,
}

/// Pomiar czasu GPU per pass (WP-R1: „mierzy czas każdego passu").
///
/// Czas CPU nie zastępuje tego pomiaru nawet w przybliżeniu: mierzy nagrywanie poleceń,
/// a GPU wykonuje je klatkę później. Stąd znaczniki czasu po stronie GPU i odczyt
/// **z opóźnieniem** — mapowanie bufora jest gotowe dopiero, gdy karta skończy klatkę.
struct PassTimer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    period_ns: f32,
    /// Mapowanie w toku — nie wolno go zlecić drugi raz ani pisać wtedy do bufora.
    w_locie: std::sync::Arc<std::sync::atomic::AtomicBool>,
    gotowe: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ms: [f32; PASS_NAMES.len()],
}

impl PassTimer {
    /// `None`, gdy sterownik nie potrafi stemplować czasu — wtedy raport mówi „brak pomiaru"
    /// zamiast podawać liczbę, która nic nie znaczy.
    fn new(gpu: &GpuContext) -> Option<PassTimer> {
        if !gpu.timestamps {
            return None;
        }
        let n = 2 * PASS_NAMES.len() as u32;
        let bajtow = u64::from(n) * 8;
        Some(PassTimer {
            set: gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("render.pass_timer"),
                ty: wgpu::QueryType::Timestamp,
                count: n,
            }),
            resolve: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render.timer.resolve"),
                size: bajtow,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render.timer.readback"),
                size: bajtow,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            period_ns: gpu.timestamp_period_ns,
            w_locie: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            gotowe: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            ms: [0.0; PASS_NAMES.len()],
        })
    }

    /// Znaczniki dla passa o zadanym indeksie.
    fn writes(&self, pass: usize) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(2 * pass as u32),
            end_of_pass_write_index: Some(2 * pass as u32 + 1),
        }
    }

    /// Odczyt wyniku poprzedniej klatki i zlecenie następnego. Wołane po `submit`.
    ///
    /// Pomiar wychodzi co druga klatka i tak ma być: w klatce, w której bufor jest
    /// zamapowany, nie wolno do niego pisać — a to znaczy, że `resolve` tej klatki
    /// i tak jest pominięty.
    fn zbierz(&mut self, gpu: &GpuContext) {
        use std::sync::atomic::Ordering;
        if self.gotowe.swap(false, Ordering::Acquire) {
            {
                let Ok(dane) = self.readback.slice(..).get_mapped_range() else {
                    return;
                };
                let mut czasy = [0u64; 2 * PASS_NAMES.len()];
                for (i, c) in czasy.iter_mut().enumerate() {
                    *c = u64::from_le_bytes(dane[i * 8..i * 8 + 8].try_into().unwrap());
                }
                for (i, ms) in self.ms.iter_mut().enumerate() {
                    // Licznik GPU potrafi się cofnąć między passami przy przełączeniu
                    // kontekstu — ujemna różnica to nie pomiar, tylko szum.
                    let d = czasy[2 * i + 1].saturating_sub(czasy[2 * i]);
                    *ms = d as f32 * self.period_ns / 1.0e6;
                }
            }
            self.readback.unmap();
            self.w_locie.store(false, Ordering::Release);
            return;
        }
        if !self.w_locie.swap(true, Ordering::AcqRel) {
            let gotowe = self.gotowe.clone();
            let w_locie = self.w_locie.clone();
            self.readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |wynik| {
                    if wynik.is_ok() {
                        gotowe.store(true, Ordering::Release);
                    } else {
                        w_locie.store(false, Ordering::Release);
                    }
                });
            // Bez odpytania urządzenia wywołanie zwrotne nigdy nie przyjdzie —
            // `wgpu` woła je z `poll`, a nie z własnego wątku.
            gpu.device.poll(wgpu::PollType::Poll).ok();
        }
    }
}

pub struct Renderer {
    pub gpu: GpuContext,
    pipeline: wgpu::RenderPipeline,
    depth_pipeline: wgpu::RenderPipeline,
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
}

/// Docelowa pojemność areny w elementach: 8 B na wierzchołek, 4 B na indeks — razem 384 MB,
/// czyli połowa budżetu 768 MB z §5.9, z zapasem na kaskady cieni i bufory pośrednie.
///
/// **Wartość docelowa, nie ostateczna.** `wgpu` z limitami `downlevel_defaults` dopuszcza
/// bufor do 256 MB, a na słabszym sprzęcie mniej — dlatego faktyczna pojemność jest
/// przycinana do `max_buffer_size` urządzenia przy starcie. Przekroczenie limitu nie jest
/// ostrzeżeniem, tylko błędem walidacji i natychmiastową paniką, więc przycięcie musi być
/// w kodzie, a nie w komentarzu.
const VERTEX_CAPACITY: u64 = 24 * 1024 * 1024;
const INDEX_CAPACITY: u64 = 48 * 1024 * 1024;

/// Przycina pojemność areny do limitu bufora urządzenia.
fn pojemnosc(limit_bajtow: u64, docelowa: u64, stride: u64) -> u32 {
    let mieszczaca_sie = limit_bajtow / stride;
    docelowa.min(mieszczaca_sie).min(u64::from(u32::MAX)) as u32
}

impl Renderer {
    pub fn new(gpu: GpuContext, materials: &MaterialRegistry) -> Renderer {
        let timer = PassTimer::new(&gpu);
        let device = &gpu.device;

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render.frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let chunk_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render.chunks"),
            size: (MAX_DRAWN_CHUNKS * std::mem::size_of::<ChunkUniform>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Tabela materiałów: albedo w liniowej przestrzeni barw plus chropowatość.
        // Konwersja z sRGB robi się tutaj, raz przy starcie, a nie w shaderze co fragment.
        let mut tabela: Vec<[f32; 4]> = Vec::with_capacity(materials.len().max(1));
        for m in materials.all() {
            tabela.push([
                srgb_to_linear(f32::from(m.albedo[0]) / 255.0),
                srgb_to_linear(f32::from(m.albedo[1]) / 255.0),
                srgb_to_linear(f32::from(m.albedo[2]) / 255.0),
                f32::from(m.roughness) / 255.0,
            ]);
        }
        if tabela.is_empty() {
            tabela.push([1.0, 0.0, 1.0, 1.0]);
        }
        let material_buffer = create_init_buffer(
            device,
            "render.materials",
            bytemuck::cast_slice(&tabela),
            wgpu::BufferUsages::STORAGE,
        );

        let limit = device.limits().max_buffer_size;
        let vertex_capacity = pojemnosc(limit, VERTEX_CAPACITY, 8);
        let index_capacity = pojemnosc(limit, INDEX_CAPACITY, 4);
        log::info!(
            "arena GPU: {:.0} MB wierzchołków, {:.0} MB indeksów (limit bufora {:.0} MB)",
            f64::from(vertex_capacity) * 8.0 / (1024.0 * 1024.0),
            f64::from(index_capacity) * 4.0 / (1024.0 * 1024.0),
            limit as f64 / (1024.0 * 1024.0)
        );

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render.arena.vertices"),
            size: u64::from(vertex_capacity) * 8,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render.arena.indices"),
            size: u64::from(index_capacity) * 4,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let indirect_buffer = gpu.multi_draw_indirect.then(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render.indirect"),
                // `DrawIndexedIndirectArgs` to pięć `u32`.
                size: (MAX_DRAWN_CHUNKS * 20) as u64,
                usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render.layout"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                storage_entry(1, wgpu::ShaderStages::VERTEX),
                storage_entry(2, wgpu::ShaderStages::VERTEX),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render.bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: chunk_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: material_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render.pipeline.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let voxel_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render.voxel"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/voxel.wgsl").into()),
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render.sky"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sky.wgsl").into()),
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        let depth_state = |write: bool| wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(write),
            depth_compare: Some(if write {
                wgpu::CompareFunction::Less
            } else {
                wgpu::CompareFunction::LessEqual
            }),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };

        // Depth prepass: wypełnia bufor głębi bez zapisu koloru. Wypłaca się przy terenie,
        // gdzie nadrysowanie jest duże, a shader fragmentu liczy oświetlenie.
        let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render.depth_prepass"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &voxel_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(vertex_layout.clone())],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(depth_state(true)),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render.opaque"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &voxel_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(vertex_layout)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &voxel_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(gpu.config.format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(depth_state(false)),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render.sky"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(gpu.config.format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let depth_view = create_depth(device, gpu.config.width, gpu.config.height);

        Renderer {
            gpu,
            pipeline,
            depth_pipeline,
            sky_pipeline,
            bind_group,
            frame_buffer,
            chunk_buffer,
            vertex_buffer,
            index_buffer,
            indirect_buffer,
            depth_view,
            vertex_arena: Arena::new(vertex_capacity),
            index_arena: Arena::new(index_capacity),
            chunks: BTreeMap::new(),
            sky: sky_lut(),
            timer,
            stats: FrameStats::default(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        self.depth_view = create_depth(
            &self.gpu.device,
            self.gpu.config.width,
            self.gpu.config.height,
        );
    }

    #[must_use]
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    #[must_use]
    pub fn has_chunk(&self, coord: ChunkCoord, lod: u8, revision: u32) -> bool {
        self.chunks
            .get(&(lod, coord))
            .is_some_and(|c| c.revision == revision)
    }

    /// Wgrywa mesh chunka do areny. Stary blok jest zwalniany — bez tego arena wypełnia się
    /// po kilku minutach latania kamerą, a objawem jest zniknięcie odległych chunków.
    pub fn upload_chunk(&mut self, coord: ChunkCoord, lod: u8, revision: u32, mesh: &ChunkMesh) {
        self.remove_chunk(coord, lod);
        if mesh.is_empty() {
            return;
        }
        let Some(vb) = self.vertex_arena.alloc(mesh.vertices.len() as u32) else {
            log::warn!("arena wierzchołków pełna — chunk {coord:?} pominięty");
            return;
        };
        let Some(ib) = self.index_arena.alloc(mesh.indices.len() as u32) else {
            self.vertex_arena.free_block(vb);
            log::warn!("arena indeksów pełna — chunk {coord:?} pominięty");
            return;
        };

        self.gpu.queue.write_buffer(
            &self.vertex_buffer,
            u64::from(vb.offset) * 8,
            bytemuck::cast_slice(&mesh.vertices),
        );
        // Indeksy są lokalne dla mesha, a rysujemy z `base_vertex` — więc do bufora idą
        // bez przesunięcia. To jest powód, dla którego `base_vertex` w ogóle istnieje.
        self.gpu.queue.write_buffer(
            &self.index_buffer,
            u64::from(ib.offset) * 4,
            bytemuck::cast_slice(&mesh.indices),
        );

        let span = f32::from(CHUNK_DIM as u16) * f32::from(1u16 << lod);
        let d = CHUNK_DIM as i32;
        let skala = 1i32 << lod;
        let center_m = [
            (coord.x * d * skala) as f32 + span * 0.5,
            (coord.y * d * skala) as f32 + span * 0.5,
            (i32::from(coord.z) * d * skala) as f32 * 0.5 + span * 0.25,
        ];
        self.chunks.insert(
            (lod, coord),
            GpuChunk {
                vertices: vb,
                indices: ib,
                lod,
                center_m,
                // Promień sfery otaczającej chunk: przekątna prostopadłościanu.
                radius_m: span * 0.75,
                revision,
            },
        );
    }

    pub fn remove_chunk(&mut self, coord: ChunkCoord, lod: u8) {
        if let Some(c) = self.chunks.remove(&(lod, coord)) {
            self.vertex_arena.free_block(c.vertices);
            self.index_arena.free_block(c.indices);
        }
    }

    /// Rysuje klatkę do okna.
    pub fn render(&mut self, camera: &CameraState, minute: SimMinute, latitude_ddeg: i16) {
        let widoczne = self.prepare_frame(camera, minute, latitude_ddeg);
        let frame = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            // `Outdated`/`Lost` zdarza się przy każdej zmianie rozmiaru okna — przekonfigurowanie
            // powierzchni jest tu normalną obsługą, nie sytuacją wyjątkową.
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.gpu
                    .surface
                    .configure(&self.gpu.device, &self.gpu.config);
                return;
            }
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render.frame"),
            });
        let triangles = self.record_passes(&mut encoder, &view, &self.depth_view, &widoczne);
        self.resolve_timer(&mut encoder);
        self.gpu.queue.submit(Some(encoder.finish()));
        self.gpu.queue.present(frame);
        self.finish_stats(&widoczne, triangles);
    }

    /// Rysuje jedną klatkę do obrazu w pamięci i zwraca go jako RGB8.
    ///
    /// Istnieje po to, żeby renderer dało się **zweryfikować bez patrzenia na ekran**:
    /// w CI bez GPU test jest pominięty, ale na maszynie z kartą jeden zrzut mówi więcej
    /// niż komplet asercji na liczbę trójkątów. Wymóg §7.4 (raport z przelotu kamery)
    /// też zaczyna się od możliwości zapisania klatki.
    pub fn render_to_image(
        &mut self,
        camera: &CameraState,
        minute: SimMinute,
        latitude_ddeg: i16,
        width: u32,
        height: u32,
    ) -> (u32, u32, Vec<u8>) {
        let widoczne = self.prepare_frame(camera, minute, latitude_ddeg);
        let device = &self.gpu.device;

        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render.offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.gpu.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = create_depth(device, width, height);

        // Kopiowanie tekstury wymaga wierszy wyrównanych do 256 B — stąd `padded`.
        let bpp = 4u32;
        let padded = (width * bpp).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render.readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render.offscreen"),
        });
        let triangles = self.record_passes(&mut encoder, &color_view, &depth, &widoczne);
        self.resolve_timer(&mut encoder);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.gpu.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.gpu
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .ok();

        let data = slice.get_mapped_range().expect("odczyt bufora zrzutu");
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            let row = (y * padded) as usize;
            for x in 0..width {
                let i = row + (x * bpp) as usize;
                // Powierzchnia jest w formacie BGRA albo RGBA zależnie od sterownika —
                // rozpoznajemy po formacie, a nie po nadziei.
                let (r, g, b) = if matches!(
                    self.gpu.config.format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                ) {
                    (data[i + 2], data[i + 1], data[i])
                } else {
                    (data[i], data[i + 1], data[i + 2])
                };
                rgb.extend_from_slice(&[r, g, b]);
            }
        }
        drop(data);
        readback.unmap();

        self.finish_stats(&widoczne, triangles);
        (width, height, rgb)
    }

    /// Wspólne przygotowanie klatki: uniformy, culling, lista widocznych chunków.
    fn prepare_frame(
        &mut self,
        camera: &CameraState,
        minute: SimMinute,
        latitude_ddeg: i16,
    ) -> Vec<VisibleChunk> {
        let sun = sun_state(minute, latitude_ddeg);
        let sky = sample_sky(&self.sky, sun.elevation_deg);
        let aspect = self.gpu.aspect();
        let view_proj = camera.view_proj_relative(aspect);
        let eye = camera.eye();

        self.write_frame_uniform(&view_proj, &sun, &sky, camera, eye.z as f32);

        // Culling frustum po stronie CPU (§5.8). Test sfery otaczającej względem sześciu
        // płaszczyzn — tanio i wystarczająco: chunk odrzucony błędnie to chunk narysowany,
        // a nie chunk brakujący, więc błąd jest po bezpiecznej stronie.
        let planes = frustum_planes(&view_proj);
        let mut widoczne: Vec<VisibleChunk> = Vec::with_capacity(self.chunks.len());
        let mut per_chunk: Vec<ChunkUniform> =
            Vec::with_capacity(self.chunks.len().min(MAX_DRAWN_CHUNKS));
        for ((_, coord), c) in &self.chunks {
            let rel = Vec3::new(
                c.center_m[0] - eye.x as f32,
                c.center_m[1] - eye.y as f32,
                c.center_m[2] - eye.z as f32,
            );
            if !sphere_in_frustum(&planes, rel, c.radius_m) {
                continue;
            }
            if per_chunk.len() >= MAX_DRAWN_CHUNKS {
                break;
            }
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
            widoczne.push(VisibleChunk {
                vertices: c.vertices,
                indices: c.indices,
            });
        }
        if !per_chunk.is_empty() {
            self.gpu
                .queue
                .write_buffer(&self.chunk_buffer, 0, bytemuck::cast_slice(&per_chunk));
        }
        if let Some(buf) = self.indirect_buffer.as_ref() {
            let args: Vec<DrawArgs> = widoczne
                .iter()
                .enumerate()
                .map(|(i, c)| DrawArgs {
                    index_count: c.indices.len,
                    instance_count: 1,
                    first_index: c.indices.offset,
                    base_vertex: c.vertices.offset as i32,
                    first_instance: i as u32,
                })
                .collect();
            if !args.is_empty() {
                self.gpu
                    .queue
                    .write_buffer(buf, 0, bytemuck::cast_slice(&args));
            }
        }
        widoczne
    }

    fn record_passes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        widoczne: &[VisibleChunk],
    ) -> usize {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(PASS_NAMES[0]),
                timestamp_writes: self.timer.as_ref().map(|t| t.writes(0)),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.depth_pipeline);
            self.draw_chunks(&mut pass, widoczne);
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(PASS_NAMES[1]),
            timestamp_writes: self.timer.as_ref().map(|t| t.writes(1)),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        // Niebo najpierw: wypełnia tło tam, gdzie prepass nie zapisał głębi, i przegrywa
        // z terenem dzięki `LessEqual` przy głębokości 1,0.
        pass.set_pipeline(&self.sky_pipeline);
        pass.set_bind_group(0, Some(&self.bind_group), &[]);
        pass.draw(0..3, 0..1);

        pass.set_pipeline(&self.pipeline);
        self.draw_chunks(&mut pass, widoczne)
    }

    /// Przepisuje znaczniki czasu do bufora odczytu. Musi iść do **tego samego** enkodera,
    /// zanim klatka trafi do kolejki — inaczej mierzyłaby się następna.
    fn resolve_timer(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(t) = self.timer.as_ref() else {
            return;
        };
        // Bufor odczytu jest zamapowany, dopóki nie odbierzemy poprzedniego wyniku;
        // zapis do zamapowanego bufora to błąd walidacji, więc klatkę pomijamy.
        // Dopóki mapowanie trwa (albo czeka na odbiór), bufor jest zajęty i zapis do niego
        // jest błędem walidacji, nie ostrzeżeniem.
        if t.w_locie.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        let n = 2 * PASS_NAMES.len() as u32;
        encoder.resolve_query_set(&t.set, 0..n, &t.resolve, 0);
        encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, u64::from(n) * 8);
    }

    fn finish_stats(&mut self, widoczne: &[VisibleChunk], triangles: usize) {
        if let Some(mut t) = self.timer.take() {
            t.zbierz(&self.gpu);
            self.timer = Some(t);
        }
        self.stats = FrameStats {
            chunks_resident: self.chunks.len(),
            chunks_drawn: widoczne.len(),
            triangles,
            vertex_bytes: self.vertex_arena.bytes_used(8),
            index_bytes: self.index_arena.bytes_used(4),
            arena_capacity_bytes: self.vertex_arena.capacity_bytes(8)
                + self.index_arena.capacity_bytes(4),
            pass_ms: self
                .timer
                .as_ref()
                .map_or([0.0; PASS_NAMES.len()], |t| t.ms),
            gpu_timing: self.timer.is_some(),
        };
    }

    /// Rysuje widoczne chunki. Dwie ścieżki, jedna geometria: pośrednia, gdy sterownik ją ma,
    /// i wywołanie na chunk, gdy nie ma (ryzyko R6 fazy — WebGPU i część sterowników nie
    /// wystawiają multi-draw). Obie muszą dawać **ten sam obraz**, dlatego argumenty rysowania
    /// powstają z tej samej listy `widoczne`, a nie z osobnego przebiegu cullingu.
    fn draw_chunks(&self, pass: &mut wgpu::RenderPass<'_>, widoczne: &[VisibleChunk]) -> usize {
        pass.set_bind_group(0, Some(&self.bind_group), &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        let trojkaty = widoczne.iter().map(|c| (c.indices.len / 3) as usize).sum();

        match self.indirect_buffer.as_ref() {
            Some(buf) if !widoczne.is_empty() => {
                pass.multi_draw_indexed_indirect(buf, 0, widoczne.len() as u32);
            }
            _ => {
                for (i, c) in widoczne.iter().enumerate() {
                    let start = c.indices.offset;
                    let end = start + c.indices.len;
                    pass.draw_indexed(start..end, c.vertices.offset as i32, i as u32..i as u32 + 1);
                }
            }
        }
        trojkaty
    }

    fn write_frame_uniform(
        &self,
        view_proj: &Mat4,
        sun: &SunState,
        sky: &SkySample,
        camera: &CameraState,
        eye_z: f32,
    ) {
        let clip_active = f32::from(u8::from(camera.clip_plane_z.is_some()));
        let clip_level = camera.clip_plane_z.map_or(0.0, |z| z as f32 * 0.5);
        let u = FrameUniform {
            view_proj: view_proj.to_cols_array_2d(),
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
        };
        self.gpu
            .queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&u));
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("render.depth"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_init_buffer(
    device: &wgpu::Device,
    label: &str,
    data: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: data,
        usage,
    })
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
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
    fn arena_scala_zwolnione_bloki() {
        // Bez scalania arena rozsypuje się na dziury i przestaje mieścić cokolwiek,
        // mimo że jest pusta — objawem jest znikanie odległych chunków po kilku minutach.
        let mut a = Arena::new(100);
        let b1 = a.alloc(30).unwrap();
        let b2 = a.alloc(30).unwrap();
        let b3 = a.alloc(30).unwrap();
        a.free_block(b1);
        a.free_block(b2);
        a.free_block(b3);
        assert_eq!(
            a.free.len(),
            1,
            "wolne bloki nie zostały scalone: {:?}",
            a.free
        );
        assert_eq!(
            a.free[0],
            Block {
                offset: 0,
                len: 100
            }
        );
        assert_eq!(a.used, 0);
        // Po scaleniu znów mieści blok większy niż pojedynczy zwolniony.
        assert!(a.alloc(90).is_some());
    }

    #[test]
    fn arena_odmawia_gdy_brakuje_miejsca() {
        let mut a = Arena::new(10);
        assert!(a.alloc(8).is_some());
        assert!(a.alloc(8).is_none(), "arena przydzieliła ponad pojemność");
        assert!(a.alloc(2).is_some());
    }

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

    #[test]
    fn konwersja_barw_jest_odwracalna_na_krancach() {
        assert!((srgb_to_linear(0.0) - 0.0).abs() < 1e-6);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        assert!(
            srgb_to_linear(0.5) < 0.5,
            "sRGB 0,5 ma być ciemniejsze liniowo"
        );
    }
}
