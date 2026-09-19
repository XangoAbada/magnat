//! Strona GPU instancingu: geometria modeli, paleta i dwa potoki (M11a §5.3).
//!
//! Wszystkie siatki modeli leżą w **jednej parze buforów** — wierzchołków i indeksów —
//! tak samo jak chunki terenu w `renderer::arena` i z tego samego powodu: bufor na model
//! znaczy jedno wiązanie na wsad, a jeden bufor z przesunięciem znaczy jedno wiązanie
//! na całą klatkę. Modeli jest kilkadziesiąt, więc suballokatora nie ma — siatki
//! doklejają się po kolei przy starcie i nigdy nie znikają.
//!
//! Dwa potoki na jednym module shadera: `fs_main` maluje encję w scenie HDR, `fs_id`
//! zapisuje jej identyfikator do bufora ID. Punkt wejścia wierzchołka jest **jeden**,
//! bo bufor ID porównuje głębię przez `Equal` — najmniejsza różnica w wierzchołku
//! zerowałaby trafienia (`Z-4` dokumentu fazy).

use super::{Batch, GpuInstance, MeshSlot, ModelTable, MAX_INSTANCES};
use magnat_voxel::{
    build_model_mesh, ClipLibrary, ImpostorAtlas, ModelLibrary, ModelVertex, PaletteLibrary,
    PoseAtlas, LOD_COUNT,
};

/// Rodzaj encji zapisywany w starszych bitach bufora ID.
///
/// Bez niego `Renderer::pick` zwracałby goły indeks encji, a wołający nie miałby z czego
/// zbudować `Subject` (`K-62`) — mieszkaniec i pojazd są osobnymi encjami o osobnych
/// kartach i rozróżnia je wyłącznie to, czym są, a nie numer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PickKind {
    Citizen = 1,
    Vehicle = 2,
    /// Zakład — **przez swoje rzeczy, nie przez mury**. Szyld i wyposażenie wnętrza są
    /// encjami instancjonowanymi, więc niosą identyfikator w instancji; ściana budynku
    /// go nie niesie, bo wierzchołek chunka ma osiem bajtów i nie ma w nich miejsca
    /// na `BuildingId` (M11c, korekta `J-19`).
    Site = 3,
}

/// W co gracz trafił kursorem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PickHit {
    pub kind: PickKind,
    /// Dolne 32 bity `Entity` — to samo, co `entity_lo` w rekordzie snapshotu.
    pub entity: u32,
}

/// Ile bitów zostaje na indeks encji. Cztery starsze niosą rodzaj, więc mieszkańców
/// i pojazdów może być 268 mln — o trzy rzędy więcej niż największa metropolia.
pub const PICK_ENTITY_BITS: u32 = 28;

/// Odczytuje wartość z bufora identyfikatorów. `None` = kursor nad niczym.
#[must_use]
pub fn decode_pick(raw: u32) -> Option<PickHit> {
    let entity = raw & ((1 << PICK_ENTITY_BITS) - 1);
    if entity == 0 {
        return None;
    }
    let kind = match raw >> PICK_ENTITY_BITS {
        1 => PickKind::Citizen,
        2 => PickKind::Vehicle,
        3 => PickKind::Site,
        _ => return None,
    };
    Some(PickHit {
        kind,
        entity: entity - 1,
    })
}

pub struct InstanceRenderer {
    pipeline: wgpu::RenderPipeline,
    id_pipeline: wgpu::RenderPipeline,
    /// Poziom impostora ma **własny potok**: rysuje dwa trójkąty na encję, bez bufora
    /// wierzchołków i bez indeksów, a kafel czyta z atlasu sylwetek.
    impostor_pipeline: wgpu::RenderPipeline,
    impostor_id_pipeline: wgpu::RenderPipeline,
    /// Tekstura kafli. Trzymana jawnie, a nie tylko przez widok w grupie wiązań —
    /// żywotność zasobu GPU ma być widoczna w typie, a nie w dokumentacji `wgpu`.
    _impostor_tiles: wgpu::Texture,
    bind: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    models: ModelTable,
    batches: Vec<Batch>,
    /// Rozmiar wypalonego atlasu póz — kryterium WP3 (≤ 512 KB) i wejście do statystyk.
    pose_atlas_bytes: usize,
    /// Rozmiar atlasu sylwetek w bajtach — wejście do budżetu VRAM z kryterium WP4.
    impostor_atlas_bytes: usize,
    /// Ile instancji leży w buforze — do statystyk i do pominięcia pustej klatki.
    drawn: u32,
}

impl InstanceRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_buffer: &wgpu::Buffer,
        models: &ModelLibrary,
        palettes: &PaletteLibrary,
    ) -> InstanceRenderer {
        let (vertices_cpu, indices_cpu, table) = build_geometry(models);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render.instance"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/instance.wgsl").into()),
        });

        let layout = bind_layout(device);

        // Bufory palety są małe (kilka kilobajtów) i **nie zmieniają się w trakcie gry**:
        // paleta jest daną, a zmiana danych to restart. Stąd zapis raz, przy tworzeniu.
        let colors = storage_buffer(device, queue, "render.instance.palette.colors", bajty_barw(palettes.colors()));
        let ramps = storage_buffer(
            device,
            queue,
            "render.instance.palette.ramps",
            bytemuck::cast_slice(&ramps_to_gpu(palettes)),
        );

        // Atlas póz jest **statyczny tak samo jak paleta**: wypala się raz przy starcie
        // i nie zmienia do końca sesji. Stąd zapis w konstruktorze i brak API do
        // podmiany — instancja niesie `(clip, phase)`, więc na klatkę nie idzie ani
        // jedna macierz kości (M11b §5.4).
        //
        // ponytail: biblioteka klipów jest brana z `ClipLibrary::builtin()` w środku,
        // a nie przyjmowana argumentem. Drugiego źródła klipów nie ma — wypełniacz
        // snapshotu woła tę samą funkcję. Modding (M12) podmieni ją na ładowarkę pliku
        // i wtedy dopiero opłaci się przekazywać bibliotekę przez sygnaturę.
        let pozy = PoseAtlas::bake(models, &ClipLibrary::builtin());
        let pose_atlas_bytes = pozy.bytes();
        let pose_atlas = storage_buffer(
            device,
            queue,
            "render.instance.pose.atlas",
            bytemuck::cast_slice(&pozy.to_gpu()),
        );

        // Atlas sylwetek: kafle wypalone z tych samych modeli i tego samego klipu chodu.
        // `Rgba8Uint` zamiast `Rgba8Unorm`, bo kanał `R` niesie **numer roli palety** —
        // liczbę, nie barwę, a filtrowanie zrobiłoby z ról 2 i 4 rolę 3.
        let sylwetki = ImpostorAtlas::bake(models, &ClipLibrary::builtin(), &pozy);
        let impostor_atlas_bytes = sylwetki.bytes();
        let (impostor_tex, impostor_widok) = impostor_texture(device, queue, &sylwetki);
        let impostor_index = storage_buffer(
            device,
            queue,
            "render.instance.impostor.index",
            bytemuck::cast_slice(&impostors_to_gpu(&sylwetki)),
        );

        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render.instance.bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: colors.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: ramps.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: pose_atlas.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: impostor_index.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&impostor_widok),
                },
            ],
        });

        let (pipeline, id_pipeline) = create_pipelines(device, &shader, &layout);
        let impostor_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render.impostor"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/impostor.wgsl").into()),
        });
        let (impostor_pipeline, impostor_id_pipeline) =
            create_impostor_pipelines(device, &impostor_shader, &layout);

        let vertices = storage_buffer_usage(
            device,
            queue,
            "render.instance.vertices",
            bytemuck::cast_slice(&vertices_cpu),
            wgpu::BufferUsages::VERTEX,
        );
        let indices = storage_buffer_usage(
            device,
            queue,
            "render.instance.indices",
            bytemuck::cast_slice(&indices_cpu),
            wgpu::BufferUsages::INDEX,
        );

        InstanceRenderer {
            pipeline,
            id_pipeline,
            impostor_pipeline,
            impostor_id_pipeline,
            _impostor_tiles: impostor_tex,
            bind,
            vertices,
            indices,
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render.instance.instances"),
                size: (MAX_INSTANCES * std::mem::size_of::<GpuInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            models: table.with_impostors(
                sylwetki.index().iter().map(|e| !e.is_empty()).collect(),
            ),
            batches: Vec::new(),
            drawn: 0,
            pose_atlas_bytes,
            impostor_atlas_bytes,
        }
    }

    #[must_use]
    pub fn models(&self) -> &ModelTable {
        &self.models
    }

    /// Ile waży atlas sylwetek wgrany na GPU.
    #[must_use]
    pub fn impostor_atlas_bytes(&self) -> usize {
        self.impostor_atlas_bytes
    }

    #[must_use]
    pub fn drawn(&self) -> u32 {
        self.drawn
    }

    /// Ile wsadów rysowania obsłużyło ostatnią klatkę.
    #[must_use]
    pub fn batches(&self) -> usize {
        self.batches.len()
    }

    /// Ile waży atlas póz wgrany na GPU.
    #[must_use]
    pub fn pose_atlas_bytes(&self) -> usize {
        self.pose_atlas_bytes
    }

    /// Wgrywa gotowy bufor instancji i wsady tej klatki.
    pub fn upload(&mut self, queue: &wgpu::Queue, instances: &[GpuInstance], batches: &[Batch]) {
        self.drawn = instances.len() as u32;
        self.batches.clear();
        self.batches.extend_from_slice(batches);
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
        }
    }

    /// Rysuje wszystkie wsady. Zwraca liczbę trójkątów.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> usize {
        self.draw_with(pass, &self.pipeline, &self.impostor_pipeline)
    }

    /// Ten sam zestaw wsadów do bufora identyfikatorów.
    ///
    /// Impostory też tu wchodzą, i to jest wymaganie produktowe, nie ozdoba: „każdy
    /// obiekt widoczny w świecie jest klikalny" (korekta po decyzji właściciela produktu
    /// w M11), a encja na dwustu metrach jest widoczna.
    pub fn draw_ids(&self, pass: &mut wgpu::RenderPass<'_>) {
        let _ = self.draw_with(pass, &self.id_pipeline, &self.impostor_id_pipeline);
    }

    fn draw_with(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        impostor_pipeline: &wgpu::RenderPipeline,
    ) -> usize {
        if self.drawn == 0 {
            return 0;
        }
        pass.set_bind_group(0, Some(&self.bind), &[]);
        pass.set_vertex_buffer(1, self.instances.slice(..));
        let mut trojkaty = 0usize;

        // Najpierw geometria, potem sylwetki: dwa potoki, ale nadal **jedno wywołanie
        // rysowania na wsad**. Rozdział idzie po poziomie detalu zapisanym w kluczu
        // wsadu, więc nie kosztuje ani jednego porównania na encję.
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for b in &self.batches {
            if super::lod_of(b.model_lod) == super::LOD_IMPOSTOR {
                continue;
            }
            let s = self.models.slot(b.model_lod);
            if s.index_count == 0 {
                continue;
            }
            pass.draw_indexed(
                s.index_offset..s.index_offset + s.index_count,
                s.base_vertex,
                b.first..b.first + b.count,
            );
            trojkaty += (s.index_count / 3) as usize * b.count as usize;
        }

        let mut impostory = false;
        for b in &self.batches {
            if super::lod_of(b.model_lod) != super::LOD_IMPOSTOR {
                continue;
            }
            if !impostory {
                pass.set_pipeline(impostor_pipeline);
                impostory = true;
            }
            // Sześć wierzchołków generowanych z numeru wierzchołka — bufor na dwa
            // trójkąty, które są takie same dla każdej encji, byłby pamięcią pod stałą.
            pass.draw(0..6, b.first..b.first + b.count);
            trojkaty += 2 * b.count as usize;
        }
        trojkaty
    }
}

/// Układ atrybutów instancji — **jeden dla obu potoków**: geometrii i impostora.
/// Dwie kopie tej tablicy rozjechałyby się przy pierwszym nowym polu `GpuInstance`,
/// a objawem byłby shader czytający cudze bajty bez błędu walidacji.
const ATRYBUTY_INSTANCJI: [wgpu::VertexAttribute; 7] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 2,
        },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 12,
        shader_location: 3,
        },
        // `palette_base` i `model_lod` leżą obok siebie jako dwa `u16`; shader
        // czyta je jako jedno 32-bitowe słowo i rozdziela przesunięciem.
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 16,
        shader_location: 4,
        },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 20,
        shader_location: 5,
        },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 24,
        shader_location: 6,
        },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 28,
        shader_location: 7,
        },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 32,
        shader_location: 8,
        },
    ];

/// Układ grupy wiązań, wspólny dla obu potoków geometrii i obu potoków impostora.
///
/// Cztery bufory storage — tyle, ile daje `downlevel_defaults`, i ani jednego więcej:
/// paleta (barwy i zestawy), atlas póz i wpisy atlasu sylwetek. Piąty wymagałby
/// podniesienia limitu urządzenia, czyli odcięcia starszych kart.
fn bind_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render.instance.layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            storage_entry(1),
            storage_entry(2),
            storage_entry(3),
            storage_entry(4),
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

/// Dwa potoki na jednym module shadera: scena HDR i bufor identyfikatorów.
///
/// Wydzielone z `InstanceRenderer::new` z tego samego powodu, dla którego R-WP3 wydzieliło
/// `renderer::pipelines`: opis atrybutów wierzchołka i instancji to dwie trzecie długości
/// konstruktora, a zmienia się z innego powodu niż reszta — z układu `GpuInstance`,
/// nie z tego, jakie bufory renderer trzyma.
fn create_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render.instance.pipeline.layout"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });

    const ATRYBUTY_WIERZCHOLKA: [wgpu::VertexAttribute; 2] = [
        wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Sint16x4,
        offset: 0,
        shader_location: 0,
        },
        wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint8x4,
        offset: 8,
        shader_location: 1,
        },
    ];
    let bufory = [
        Some(wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<ModelVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATRYBUTY_WIERZCHOLKA,
        }),
        Some(wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATRYBUTY_INSTANCJI,
        }),
    ];
    let primitive = wgpu::PrimitiveState {
        cull_mode: Some(wgpu::Face::Back),
        ..Default::default()
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.instance.opaque"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
        module: shader,
        entry_point: Some("vs_main"),
        buffers: &bufory,
        compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
        module: shader,
        entry_point: Some("fs_main"),
        targets: &[Some(crate::renderer::HDR_FORMAT.into())],
        compilation_options: Default::default(),
        }),
        primitive,
        depth_stencil: Some(wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: Default::default(),
        bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let id_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.instance.id"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
        module: shader,
        entry_point: Some("vs_main"),
        buffers: &bufory,
        compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
        module: shader,
        entry_point: Some("fs_id"),
        targets: &[Some(crate::pick::ID_FORMAT.into())],
        compilation_options: Default::default(),
        }),
        primitive,
        // `Equal` bez zapisu: rysujemy dokładnie te fragmenty, które wygrały
        // w passie nieprzezroczystym. Stąd bierze się poprawne przesłanianie.
        depth_stencil: Some(wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Equal),
        stencil: Default::default(),
        bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    (pipeline, id_pipeline)
}

/// Skleja siatki wszystkich modeli i poziomów detalu w jedną parę buforów.
fn build_geometry(models: &ModelLibrary) -> (Vec<ModelVertex>, Vec<u32>, ModelTable) {
    let mut vertices: Vec<ModelVertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut slots = vec![MeshSlot::default(); models.len() * LOD_COUNT as usize];
    for (id, m) in models.iter() {
        for lod in 0..LOD_COUNT {
            let siatka = build_model_mesh(m, lod);
            if siatka.is_empty() {
                continue;
            }
            let slot = MeshSlot {
                index_offset: indices.len() as u32,
                index_count: siatka.indices.len() as u32,
                base_vertex: vertices.len() as i32,
                radius_m: siatka.bounding_radius_m(),
            };
            slots[id.0 as usize * LOD_COUNT as usize + lod as usize] = slot;
            vertices.extend_from_slice(&siatka.vertices);
            indices.extend_from_slice(&siatka.indices);
        }
    }
    // Bufor o zerowej długości jest błędem walidacji `wgpu`, a katalog bez modeli
    // jest stanem, w którym gra i tak nic nie narysuje — jeden pusty trójkąt kosztuje
    // 36 bajtów i oszczędza gałąź w każdym miejscu, które te bufory wiąże.
    if indices.is_empty() {
        vertices.push(ModelVertex::default());
        indices.extend_from_slice(&[0, 0, 0]);
    }
    let citizen = models.id_of("citizen").unwrap_or_default();
    let vehicle = models.id_of("car").unwrap_or_default();
    // Klucze propów są **kontraktem z `tools/mvoxc gen`**: brak pliku nie jest błędem,
    // tylko wnętrzem bez tego mebla — `MODEL_BRAK` przechodzi przez generator wnętrz
    // jako „nie ma czym tego narysować".
    let id = |k: &str| models.id_of(k).unwrap_or(crate::interiors::MODEL_BRAK);
    let props = crate::interiors::PropModels {
        shelf: id("shelf"),
        crate_: id("crate"),
        desk: id("desk"),
        machine: id("machine"),
    };
    let table = ModelTable::new(slots, citizen, vehicle).with_props(props, id("sign"));
    (vertices, indices, table)
}

/// Tekstura warstwowa z kaflami sylwetek. `Rgba8Uint`, bo kanały niosą **liczby**
/// (rolę palety i numer ściany), a nie barwy — filtrowanie zrobiłoby z nich średnią.
fn impostor_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &ImpostorAtlas,
) -> (wgpu::Texture, wgpu::TextureView) {
    // Tekstura o zerowej liczbie warstw jest błędem walidacji, a katalog bez modeli
    // z impostorem jest stanem dopuszczalnym — stąd jedna pusta warstwa.
    let warstwy = atlas.layers().max(1);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render.instance.impostor.tiles"),
        size: wgpu::Extent3d {
            width: magnat_voxel::TILE_W,
            height: magnat_voxel::TILE_H,
            depth_or_array_layers: warstwy,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let oczekiwane =
        (magnat_voxel::TILE_W * magnat_voxel::TILE_H * warstwy) as usize * 4;
    let mut dane = atlas.pixels().to_vec();
    dane.resize(oczekiwane, 0);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &dane,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(magnat_voxel::TILE_W * 4),
            rows_per_image: Some(magnat_voxel::TILE_H),
        },
        wgpu::Extent3d {
            width: magnat_voxel::TILE_W,
            height: magnat_voxel::TILE_H,
            depth_or_array_layers: warstwy,
        },
    );
    let widok = tex.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    (tex, widok)
}

/// Wpisy atlasu sylwetek w układzie, którego oczekuje `impostor.wgsl`.
fn impostors_to_gpu(atlas: &ImpostorAtlas) -> Vec<[u32; 4]> {
    let mut v: Vec<[u32; 4]> = atlas
        .index()
        .iter()
        .map(|e| {
            [
                e.base_layer,
                u32::from(e.dirs)
                    | (u32::from(e.frames) << 8)
                    | (u32::from(e.frame_shift) << 16),
                e.size_m[0].to_bits(),
                e.size_m[1].to_bits(),
            ]
        })
        .collect();
    if v.is_empty() {
        v.push([0; 4]);
    }
    v
}

/// Dwa potoki impostora: scena i bufor identyfikatorów. Bez bufora wierzchołków —
/// billboard powstaje z numeru wierzchołka, a jedynym wejściem jest bufor instancji.
fn create_impostor_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("render.impostor.pipeline.layout"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    // Slot 0 zostaje pusty, żeby numeracja atrybutów instancji była ta sama co w potoku
    // geometrii — jeden układ `GpuInstance`, dwa potoki, zero miejsc na rozjazd.
    let bufory = [
        None,
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATRYBUTY_INSTANCJI,
        }),
    ];
    // Billboard jest dwustronny: kafel wypalono dla azymutu, a obrót ku kamerze może
    // wypaść tak, że trójkąt patrzy tyłem.
    let primitive = wgpu::PrimitiveState {
        cull_mode: None,
        ..Default::default()
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.impostor.opaque"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &bufory,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(crate::renderer::HDR_FORMAT.into())],
            compilation_options: Default::default(),
        }),
        primitive,
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let id_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.impostor.id"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &bufory,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_id"),
            targets: &[Some(crate::pick::ID_FORMAT.into())],
            compilation_options: Default::default(),
        }),
        primitive,
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Equal),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, id_pipeline)
}

fn ramps_to_gpu(p: &PaletteLibrary) -> Vec<[u32; 2]> {
    let mut v: Vec<[u32; 2]> = p.ramps().iter().map(|r| [r.offset, r.len]).collect();
    if v.is_empty() {
        v.push([0, 0]);
    }
    v
}

/// `&[u32]` na bajty. Osobna funkcja, bo `bytemuck::cast_slice` na pustym wycinku zwraca
/// pusty wycinek, a `wgpu` nie przyjmuje bufora o zerowej długości — katalog palet bez
/// ani jednej barwy jest błędem danych, ale ma dać komunikat walidatora, a nie panikę
/// sterownika przy tworzeniu bufora.
fn bajty_barw(v: &[u32]) -> &[u8] {
    if v.is_empty() {
        return &[0, 0, 0, 0];
    }
    bytemuck::cast_slice(v)
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    data: &[u8],
) -> wgpu::Buffer {
    storage_buffer_usage(device, queue, label, data, wgpu::BufferUsages::STORAGE)
}

fn storage_buffer_usage(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    data: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    let rozmiar = (data.len().max(4) as u64).next_multiple_of(4);
    let b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: rozmiar,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&b, 0, data);
    b
}
