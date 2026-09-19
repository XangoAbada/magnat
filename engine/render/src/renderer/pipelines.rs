//! Tworzenie obiektów GPU: układy grup wiązań, moduły shaderów i potoki.
//!
//! Kod bezstanowy — bierze `&Device` i oddaje obiekty GPU, nie dotyka `Renderer`.
//! Wydzielone z `Renderer::new` w R-WP3. Kolejność wywołań `create_bind_group_layout`
//! i `create_*_pipeline` jest zachowana co do jednego: każda funkcja niżej to
//! **ciągły** wycinek dawnego `new`, a `new` woła je w tej samej kolejności.

use super::arena::{pojemnosc, INDEX_CAPACITY, VERTEX_CAPACITY};
use super::frame::{ChunkUniform, FrameUniform, MAX_DRAWN_CHUNKS, MAX_INDIRECT_ARGS};
use super::HDR_FORMAT;
use crate::clusters::{self, ClusterConfig, GpuLight, CLUSTER_CAPACITY, CLUSTER_COUNT};
use crate::shadow;
use magnat_voxel::MaterialRegistry;

/// Bufory stałe renderera: ramka, dane per chunk, tabela materiałów i arena geometrii.
pub(super) struct Buffers {
    pub frame_buffer: wgpu::Buffer,
    pub chunk_buffer: wgpu::Buffer,
    pub material_buffer: wgpu::Buffer,
    pub vertex_capacity: u32,
    pub index_capacity: u32,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub indirect_buffer: Option<wgpu::Buffer>,
}

pub(super) fn buffers(
    device: &wgpu::Device,
    materials: &MaterialRegistry,
    multi_draw_indirect: bool,
) -> Buffers {
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

    let indirect_buffer = multi_draw_indirect.then(|| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render.indirect"),
            // `DrawIndexedIndirectArgs` to pięć `u32`.
            size: (MAX_INDIRECT_ARGS * 20) as u64,
            usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    });

    Buffers {
        frame_buffer,
        chunk_buffer,
        material_buffer,
        vertex_capacity,
        index_capacity,
        vertex_buffer,
        index_buffer,
        indirect_buffer,
    }
}

/// Grupa główna sceny i grupa nakładki terenowej.
pub(super) struct TerrainLayouts {
    pub layout: wgpu::BindGroupLayout,
    pub overlay_layout: wgpu::BindGroupLayout,
    pub overlay_bind: wgpu::BindGroup,
}

pub(super) fn terrain_layouts(device: &wgpu::Device) -> TerrainLayouts {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.layout"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            storage_entry(1, wgpu::ShaderStages::VERTEX),
            storage_entry(2, wgpu::ShaderStages::VERTEX),
            // Mapa cieni: tablica czterech warstw i sampler porównawczy. Porównawczy,
            // bo sprzętowe porównanie z filtrowaniem daje PCF 2×2 za darmo — nasze
            // 3×3 dokłada do niego tylko cztery próbki, a nie dwadzieścia pięć.
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            storage_entry(5, wgpu::ShaderStages::FRAGMENT),
            storage_entry(6, wgpu::ShaderStages::FRAGMENT),
            storage_entry(7, wgpu::ShaderStages::FRAGMENT),
            uniform_entry(8, wgpu::ShaderStages::FRAGMENT),
        ],
    });
    // Nakładka terenowa (§6.1, kontrakt dla M2): pole skalarne na siatce świata plus
    // paleta. Osobna grupa wiązań, bo nakładkę ma widzieć teren bliski **i** daleki,
    // a te dwa pipeline'y nie dzielą niczego poza grupą ramki.
    let overlay_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.overlay.layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    // Nakładka domyślnie nie istnieje, ale grupa wiązań musi — pipeline jej wymaga.
    // Jednopikselowe tekstury są tańsze niż drugi wariant pipeline'u bez nakładki.
    let overlay_bind = create_overlay_bind(
        device,
        &overlay_layout,
        &create_pixel_texture(device),
        &create_pixel_texture(device),
    );

    TerrainLayouts {
        layout,
        overlay_layout,
        overlay_bind,
    }
}

/// Klastrowanie świateł: bufory, grupa wiązań i potok obliczeniowy.
pub(super) struct Cluster {
    pub cluster_config: wgpu::Buffer,
    pub view_buffer: wgpu::Buffer,
    pub light_buffer: wgpu::Buffer,
    pub cluster_counts: wgpu::Buffer,
    pub cluster_indices: wgpu::Buffer,
    pub occupancy_readback: wgpu::Buffer,
    pub cluster_bind: wgpu::BindGroup,
    pub cluster_pipeline: wgpu::ComputePipeline,
}

pub(super) fn cluster(device: &wgpu::Device) -> Cluster {
    // Klastry świateł: konfiguracja, lista świateł, liczniki i listy indeksów.
    // Liczniki mają `COPY_SRC`, bo `ClusterOccupancy` czyta je z powrotem na CPU —
    // histogram bez odczytu byłby przyrządem, którego nie da się odczytać.
    let cluster_config = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.clusters.config"),
        size: std::mem::size_of::<ClusterConfig>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let view_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.clusters.view"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let light_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.lights"),
        size: (clusters::MAX_LIGHTS * std::mem::size_of::<GpuLight>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let cluster_counts = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.clusters.counts"),
        size: u64::from(CLUSTER_COUNT) * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let cluster_indices = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.clusters.indices"),
        size: u64::from(CLUSTER_COUNT) * u64::from(CLUSTER_CAPACITY) * 4,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let occupancy_readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.clusters.readback"),
        size: u64::from(CLUSTER_COUNT) * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let cluster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.clusters.layout"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::COMPUTE),
            storage_entry(1, wgpu::ShaderStages::COMPUTE),
            rw_storage_entry(2),
            rw_storage_entry(3),
            uniform_entry(4, wgpu::ShaderStages::COMPUTE),
        ],
    });
    let cluster_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render.clusters.bind"),
        layout: &cluster_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: cluster_config.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: light_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: cluster_counts.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: cluster_indices.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: view_buffer.as_entire_binding(),
            },
        ],
    });
    let cluster_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render.clusters"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/clusters.wgsl").into()),
    });
    let cluster_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("render.clusters.pipeline"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render.clusters.pipeline.layout"),
                bind_group_layouts: &[Some(&cluster_layout)],
                immediate_size: 0,
            }),
        ),
        module: &cluster_shader,
        entry_point: Some("cs_main"),
        compilation_options: Default::default(),
        cache: None,
    });

    Cluster {
        cluster_config,
        view_buffer,
        light_buffer,
        cluster_counts,
        cluster_indices,
        occupancy_readback,
        cluster_bind,
        cluster_pipeline,
    }
}

pub(super) fn water_layout(device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::Buffer) {
    // Przebieg wody: własna grupa wiązań (głębia sceny + czas) i własny pipeline
    // z mieszaniem. Reszta — ramka, chunki, materiały — idzie z grupy głównej.
    let water_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.water.layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
        ],
    });
    let time_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.water.time"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    (water_layout, time_buffer)
}

/// Mapy cieni i grupa wiązań sceny — jedyne miejsce, w którym schodzą się bufory
/// ramki, chunków, materiałów i klastrów.
pub(super) fn main_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    b: &Buffers,
    c: &Cluster,
) -> (Vec<wgpu::TextureView>, wgpu::BindGroup) {
    let (shadow_layers, shadow_array) = create_shadow_maps(device);
    let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("render.shadow.sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render.bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: b.frame_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: b.chunk_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: b.material_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&shadow_array),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: c.light_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: c.cluster_counts.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: c.cluster_indices.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: c.cluster_config.as_entire_binding(),
            },
        ],
    });

    (shadow_layers, bind_group)
}

pub(super) fn cascades(device: &wgpu::Device) -> (wgpu::BindGroupLayout, Vec<wgpu::BindGroup>) {
    // Grupa 1 niesie numer kaskady. Osobny bufor na kaskadę zamiast dynamicznego
    // przesunięcia: cztery szesnastobajtowe bufory są tańsze w czytaniu niż wyrównanie
    // do 256 B i przesunięcia liczone przy każdym passie.
    let cascade_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.cascade.layout"),
        entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
    });
    let cascade_binds: Vec<wgpu::BindGroup> = (0..shadow::CASCADES as u32)
        .map(|i| {
            let buf = create_init_buffer(
                device,
                "render.cascade.index",
                bytemuck::bytes_of(&[i, 0, 0, 0]),
                wgpu::BufferUsages::UNIFORM,
            );
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("render.cascade.bind"),
                layout: &cascade_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            })
        })
        .collect();

    (cascade_layout, cascade_binds)
}

/// Układy potoków i wąska grupa wiązań passa cienia.
pub(super) struct PipelineLayouts {
    pub pipeline_layout: wgpu::PipelineLayout,
    pub terrain_layout: wgpu::PipelineLayout,
    pub shadow_bind: wgpu::BindGroup,
    pub water_pipeline_layout: wgpu::PipelineLayout,
    pub shadow_layout: wgpu::PipelineLayout,
}

pub(super) fn pipeline_layouts(
    device: &wgpu::Device,
    t: &TerrainLayouts,
    water_layout: &wgpu::BindGroupLayout,
    cascade_layout: &wgpu::BindGroupLayout,
    b: &Buffers,
) -> PipelineLayouts {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render.pipeline.layout"),
        bind_group_layouts: &[Some(&t.layout)],
        immediate_size: 0,
    });
    // Teren nieprzezroczysty widzi nakładkę jako grupę 2 — patrz `overlay_layout`.
    let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render.terrain.pipeline.layout"),
        bind_group_layouts: &[Some(&t.layout), Some(&t.overlay_layout)],
        immediate_size: 0,
    });
    // Pass cienia **pisze** do mapy cieni, więc nie wolno mu jej jednocześnie widzieć
    // jako tekstury: to nie jest kwestia stylu, tylko błąd walidacji. Stąd druga grupa
    // wiązań, węższa — sama ramka i dane chunków.
    let geom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.geometry.layout"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::VERTEX),
            storage_entry(1, wgpu::ShaderStages::VERTEX),
        ],
    });
    let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render.geometry.bind"),
        layout: &geom_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: b.frame_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: b.chunk_buffer.as_entire_binding(),
            },
        ],
    });
    let water_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render.water.pipeline.layout"),
        bind_group_layouts: &[Some(&t.layout), Some(water_layout)],
        immediate_size: 0,
    });
    let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render.shadow.layout"),
        bind_group_layouts: &[Some(&geom_layout), Some(cascade_layout)],
        immediate_size: 0,
    });

    PipelineLayouts {
        pipeline_layout,
        terrain_layout,
        shadow_bind,
        water_pipeline_layout,
        shadow_layout,
    }
}

/// Stan bufora głębi: zapis albo sam test. Dawne domknięcie `depth_state` z `new` —
/// funkcja wolna, bo korzystają z niego potoki sceny **i** potok dalekiego terenu.
fn depth_state(write: bool) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(write),
        depth_compare: Some(if write {
            wgpu::CompareFunction::Less
        } else {
            wgpu::CompareFunction::LessEqual
        }),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

/// Pięć potoków rysujących scenę, w kolejności tworzenia z dawnego `new`.
pub(super) struct ScenePipelines {
    pub shadow_pipeline: wgpu::RenderPipeline,
    pub depth_pipeline: wgpu::RenderPipeline,
    /// Prepass głębi z cięciem poziomami — używany wyłącznie przy aktywnym przekroju.
    pub depth_clip_pipeline: wgpu::RenderPipeline,
    pub pipeline: wgpu::RenderPipeline,
    pub sky_pipeline: wgpu::RenderPipeline,
    pub water_pipeline: wgpu::RenderPipeline,
}

pub(super) fn scene_pipelines(device: &wgpu::Device, p: &PipelineLayouts) -> ScenePipelines {
    let voxel_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render.voxel"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/voxel.wgsl").into()),
    });
    let water_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render.water"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/water.wgsl").into()),
    });
    let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render.sky"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/sky.wgsl").into()),
    });

    let vertex_layout_water = wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 0,
            shader_location: 0,
        }],
    };
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 0,
            shader_location: 0,
        }],
    };

    // Pass cienia: sama głębia, z przodu odciętymi ściankami **przednimi**, nie tylnymi.
    // Rysowanie tylnych ścianek do mapy cienia odsuwa zapisaną głębię o grubość bryły
    // i usuwa akne cieniowania na powierzchniach oświetlonych — tanim kosztem
    // (cień „odkleja się" od podstawy tylko tam, gdzie bryła jest cienka).
    let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.shadow"),
        layout: Some(&p.shadow_layout),
        vertex: wgpu::VertexState {
            module: &voxel_shader,
            entry_point: Some("vs_shadow"),
            buffers: &[Some(vertex_layout.clone())],
            compilation_options: Default::default(),
        },
        fragment: None,
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Front),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            // Bias stały plus zależny od nachylenia: na zboczu jeden texel cienia
            // pokrywa większy przedział głębi, więc stały bias tam nie wystarcza.
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // Depth prepass: wypełnia bufor głębi bez zapisu koloru. Wypłaca się przy terenie,
    // gdzie nadrysowanie jest duże, a shader fragmentu liczy oświetlenie.
    let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.depth_prepass"),
        layout: Some(&p.pipeline_layout),
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

    // Ten sam prepass **z cięciem poziomami** (M11c §5.7). Osobny potok, a nie jedna
    // gałąź w shaderze: przy wyłączonym przekroju — czyli prawie zawsze — prepass ma
    // zostać bez shadera fragmentu, bo na tym polega jego cała wartość.
    let depth_clip_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.depth_prepass.clip"),
        layout: Some(&p.pipeline_layout),
        vertex: wgpu::VertexState {
            module: &voxel_shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(vertex_layout.clone())],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &voxel_shader,
            entry_point: Some("fs_depth"),
            targets: &[],
            compilation_options: Default::default(),
        }),
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
        layout: Some(&p.terrain_layout),
        vertex: wgpu::VertexState {
            module: &voxel_shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(vertex_layout)],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &voxel_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(HDR_FORMAT.into())],
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
        layout: Some(&p.pipeline_layout),
        vertex: wgpu::VertexState {
            module: &sky_shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &sky_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(HDR_FORMAT.into())],
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

    // Woda rysowana **po** terenie, z mieszaniem i bez zapisu głębi: dwie tafle
    // jedna za drugą nie mają się wzajemnie wycinać, a bufor głębi opisuje dno,
    // z którego liczy się grubość słupa wody.
    let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.water"),
        layout: Some(&p.water_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &water_shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(vertex_layout_water)],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &water_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    ScenePipelines {
        shadow_pipeline,
        depth_pipeline,
        depth_clip_pipeline,
        pipeline,
        sky_pipeline,
        water_pipeline,
    }
}

pub(super) fn far(
    device: &wgpu::Device,
    t: &TerrainLayouts,
) -> (wgpu::BindGroupLayout, wgpu::Buffer, wgpu::RenderPipeline) {
    let far_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.far.layout"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::VERTEX),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Sint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let far_cfg = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.far.cfg"),
        size: 48,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let far_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render.far"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/far_terrain.wgsl").into()),
    });
    let far_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.far"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render.far.pipeline.layout"),
                bind_group_layouts: &[Some(&t.layout), Some(&far_layout), Some(&t.overlay_layout)],
                immediate_size: 0,
            }),
        ),
        vertex: wgpu::VertexState {
            module: &far_shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &far_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(HDR_FORMAT.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(depth_state(true)),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    (far_layout, far_cfg, far_pipeline)
}

/// Post-processing: układ wiązań, sampler, konfiguracja i potok.
pub(super) struct Post {
    pub post_layout: wgpu::BindGroupLayout,
    pub post_sampler: wgpu::Sampler,
    pub post_cfg: wgpu::Buffer,
    pub post_pipeline: wgpu::RenderPipeline,
}

pub(super) fn post(device: &wgpu::Device, format: wgpu::TextureFormat) -> Post {
    // Post-processing: bufor HDR sceny → ekran. Sampler liniowy, bo FXAA pobiera
    // próbki między pikselami.
    let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.post.layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
        ],
    });
    let post_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("render.post.sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let post_cfg = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render.post.cfg"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("render.post"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/post.wgsl").into()),
    });
    let post_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.post"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render.post.pipeline.layout"),
                bind_group_layouts: &[Some(&post_layout)],
                immediate_size: 0,
            }),
        ),
        vertex: wgpu::VertexState {
            module: &post_shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &post_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(format.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    Post {
        post_layout,
        post_sampler,
        post_cfg,
        post_pipeline,
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
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
            // `TEXTURE_BINDING` obok załącznika, bo przebieg wody **czyta** głębię terenu,
            // żeby policzyć grubość słupa wody. Odczyt jest bezpieczny, dopóki ten sam
            // przebieg nie zapisuje głębi — i dlatego pass wody ma ją tylko do odczytu.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Mapa cieni: jedna tekstura tablicowa, widok na całość do próbkowania i widok na każdą
/// warstwę do rysowania. Warstwy nie mogą być osobnymi teksturami, bo shader wybiera
/// kaskadę indeksem — a indeksować da się tablicę, nie cztery uchwyty.
/// Grupa wiązań wody: głębia sceny i czas. Powstaje przy starcie i po każdej zmianie
/// rozmiaru okna, bo trzyma widok tekstury głębi.
/// Bufor sceny w HDR. Osobna tekstura, a nie powierzchnia okna: powierzchnia jest
/// ośmiobitowa i w sRGB, więc nie uniesie wartości powyżej jedynki, na których pracuje
/// ekspozycja.
/// Jednopikselowa tekstura — zaślepka dla nieaktywnej nakładki. Jej zawartość nie ma
/// znaczenia: shader czyta nakładkę dopiero wtedy, gdy uniform mówi, że jest aktywna.
fn create_pixel_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render.overlay.pusta"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(super) fn create_overlay_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    pole: &wgpu::Texture,
    paleta: &wgpu::Texture,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render.overlay.bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &pole.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &paleta.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
        ],
    })
}

pub(super) fn create_hdr(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("render.hdr"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

pub(super) fn create_post_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    hdr: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    cfg: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render.post.bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(hdr),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: cfg.as_entire_binding(),
            },
        ],
    })
}

pub(super) fn create_water_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    depth: &wgpu::TextureView,
    time: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render.water.bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: time.as_entire_binding(),
            },
        ],
    })
}

fn create_shadow_maps(device: &wgpu::Device) -> (Vec<wgpu::TextureView>, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render.shadow"),
        size: wgpu::Extent3d {
            width: shadow::SHADOW_MAP_SIZE,
            height: shadow::SHADOW_MAP_SIZE,
            depth_or_array_layers: shadow::CASCADES as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let warstwy = (0..shadow::CASCADES as u32)
        .map(|i| {
            tex.create_view(&wgpu::TextureViewDescriptor {
                label: Some("render.shadow.layer"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let tablica = tex.create_view(&wgpu::TextureViewDescriptor {
        label: Some("render.shadow.array"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    (warstwy, tablica)
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

/// Bufor zapisywalny przez compute — tylko tam, bo zapis z fragmentu wymagałby
/// synchronizacji, której ten renderer nie potrzebuje.
fn rw_storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
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

#[cfg(test)]
mod tests {
    use super::*;

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
