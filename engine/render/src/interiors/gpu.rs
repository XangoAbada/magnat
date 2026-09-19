//! Strona GPU domknięcia przekroju (M11c §5.7, WP5).
//!
//! Jeden bufor wierzchołków, jeden potok, jedno wywołanie rysowania na klatkę. Czapki
//! są płaskimi wielokątami na rzędnej cięcia, więc nie mają ani indeksów, ani normalnych,
//! ani instancjonowania — geometria jest tak mała, że wszystko inne byłoby kosztem
//! bez korzyści.

use super::CapVertex;

/// Ile wierzchołków mieści bufor czapek.
///
/// 16 384 to ok. 2 700 przeciętych budynków po sześć wierzchołków, czyli więcej, niż
/// mieści się w kadrze przy `CutPlane` na jedną dzielnicę. Bufor o stałej pojemności
/// z tego samego powodu co bufor instancji: klatka nie ma prawa alokować.
pub const MAX_CAP_VERTICES: usize = 16_384;

pub struct CapRenderer {
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    drawn: u32,
}

impl CapRenderer {
    pub fn new(
        device: &wgpu::Device,
        frame_buffer: &wgpu::Buffer,
        depth_format: wgpu::TextureFormat,
    ) -> CapRenderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render.cap"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/cap.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render.cap.layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render.cap.bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render.cap.pipeline.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        const ATRYBUTY: [wgpu::VertexAttribute; 2] = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 12,
                shader_location: 1,
            },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render.cap"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CapVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATRYBUTY,
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(crate::renderer::HDR_FORMAT.into())],
                compilation_options: Default::default(),
            }),
            // **Bez odrzucania ścianek.** Obrysy M2 mają obie orientacje obiegu, a czapka
            // widziana od spodu jest tak samo potrzebna jak od góry — gracz patrzy na
            // przekrój z dołu za każdym razem, gdy kamera jest niżej od cięcia.
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(true),
                // `LessEqual`, nie `Less`: prepass głębi rysuje **pełną** geometrię chunka,
                // więc na wysokości cięcia bufor głębi ma już wartość ściany, którą pass
                // nieprzezroczysty dopiero odrzucił. Przy `Less` czapka przegrywałaby
                // z nieistniejącym fragmentem i przekrój zostawałby dziurą.
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        CapRenderer {
            pipeline,
            bind,
            vertices: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render.cap.vertices"),
                size: (MAX_CAP_VERTICES * std::mem::size_of::<CapVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            drawn: 0,
        }
    }

    /// Wgrywa czapki tej klatki. Nadmiar ponad pojemność bufora jest **obcinany**,
    /// a nie realokowany: klatka nie alokuje.
    pub fn upload(&mut self, queue: &wgpu::Queue, vertices: &[CapVertex]) {
        let n = vertices.len().min(MAX_CAP_VERTICES);
        self.drawn = n as u32;
        if n > 0 {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices[..n]));
        }
    }

    /// Rysuje czapki. Zwraca liczbę trójkątów.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> usize {
        if self.drawn == 0 {
            return 0;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, Some(&self.bind), &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..self.drawn, 0..1);
        self.drawn as usize / 3
    }

    #[must_use]
    pub fn drawn(&self) -> u32 {
        self.drawn
    }
}
