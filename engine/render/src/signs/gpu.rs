//! Strona GPU szyldów (M11c §5.7, WP9).
//!
//! Atlas jest teksturą tablicową `R8Unorm`: jedna warstwa na kafel, 256 × 64 px.
//! Warstwy zamiast jednej wysokiej tekstury, bo 256 kafli po 64 px to 16 384 px wysokości,
//! czyli dokładnie sufit `max_texture_dimension_2d` na sprzęcie, na którym gra ma chodzić —
//! a tablica nie ma tego ograniczenia i przy okazji zdejmuje z shadera arytmetykę
//! przesunięcia kafla.
//!
//! Wszystkie napisy idą **jednym wywołaniem rysowania**: to jest wprost kryterium WP9
//! („sto różnych szyldów w kadrze w jednym wywołaniu").

use super::{SignAtlas, TILE_H, TILE_W};

/// Ile prostokątów mieści bufor. Sześć wierzchołków na szyld, więc 4 096 napisów —
/// czterdzieści razy więcej, niż mówi kryterium.
pub const MAX_SIGNS: usize = 4_096;

/// Wierzchołek napisu.
#[derive(Clone, Copy, PartialEq, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct SignVertex {
    /// Pozycja **względem origin kamery**, w metrach.
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    /// Warstwa atlasu, czyli [`super::SignId`].
    pub layer: u32,
    /// Barwa liter, RGBA8.
    pub color: u32,
}

/// Jeden szyld do narysowania — tyle, ile trzeba, żeby zbudować prostokąt.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SignQuad {
    /// Środek lica tablicy w metrach świata.
    pub center: [f32; 3],
    /// Obrót wokół osi pionowej w 1/65536 obrotu — tablica patrzy w `+X` przy zerze.
    pub yaw: u16,
    pub tile: super::SignId,
    pub color: [u8; 4],
}

/// Szerokość tablicy w metrach — tyle, ile ma model `sign.mvox` (10 voxeli × 0,25 m).
pub const SIGN_W_M: f32 = 2.5;
/// Wysokość tablicy w metrach (3 voxele).
pub const SIGN_H_M: f32 = 0.75;
/// O ile napis wysuwa się przed lico tablicy, w metrach. Bez tego walczy z nią o głębię
/// i miga w połowie pikseli.
const OFFSET_M: f32 = 0.16;

#[derive(Debug, Default)]
pub struct SignGeometry {
    pub vertices: Vec<SignVertex>,
}

impl SignGeometry {
    /// Buduje prostokąty napisów. Szyld bez kafla (`SignId(0)`) jest pomijany.
    pub fn build(&mut self, signs: &[SignQuad], eye: glam::DVec3) {
        self.vertices.clear();
        for s in signs {
            if s.tile.0 == 0 || self.vertices.len() + 6 > MAX_SIGNS * 6 {
                continue;
            }
            let kat = f32::from(s.yaw) / 65_536.0 * std::f32::consts::TAU;
            let (sin, cos) = kat.sin_cos();
            // Oś napisu idzie **w poprzek** kierunku patrzenia tablicy, a normalna
            // wzdłuż niego — tak samo jak w modelu, którego lico leży w płaszczyźnie YZ.
            let wzdluz = glam::Vec3::new(-sin, cos, 0.0) * (SIGN_W_M * 0.5);
            let przod = glam::Vec3::new(cos, sin, 0.0) * OFFSET_M;
            let gora = glam::Vec3::Z * (SIGN_H_M * 0.5);
            let c = glam::DVec3::new(
                f64::from(s.center[0]),
                f64::from(s.center[1]),
                f64::from(s.center[2]),
            ) - eye;
            let c = c.as_vec3() + przod;
            let color = u32::from_le_bytes(s.color);
            let v = |o: glam::Vec3, uv: [f32; 2]| SignVertex {
                pos: (c + o).into(),
                uv,
                layer: u32::from(s.tile.0),
                color,
            };
            let lg = v(-wzdluz + gora, [0.0, 0.0]);
            let pg = v(wzdluz + gora, [1.0, 0.0]);
            let ld = v(-wzdluz - gora, [0.0, 1.0]);
            let pd = v(wzdluz - gora, [1.0, 1.0]);
            self.vertices.extend_from_slice(&[lg, ld, pd, lg, pd, pg]);
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

pub struct SignRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    bind: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    texture: wgpu::Texture,
    frame_buffer: wgpu::Buffer,
    vertices: wgpu::Buffer,
    warstwy: u32,
    drawn: u32,
}

impl SignRenderer {
    pub fn new(
        device: &wgpu::Device,
        frame_buffer: &wgpu::Buffer,
        depth_format: wgpu::TextureFormat,
    ) -> SignRenderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render.sign"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/sign.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render.sign.layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("render.sign.sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (texture, bind) = utworz(device, &layout, &sampler, frame_buffer, 1);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render.sign.pipeline.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        const ATRYBUTY: [wgpu::VertexAttribute; 4] = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 12,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 20,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 24,
                shader_location: 3,
            },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render.sign"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SignVertex>() as u64,
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
            // Bez odrzucania ścianek: szyld czyta się z obu stron ulicy, a tablica
            // ma w modelu flagę `TWO_SIDED` z tego samego powodu.
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        SignRenderer {
            pipeline,
            layout,
            bind,
            sampler,
            texture,
            frame_buffer: frame_buffer.clone(),
            vertices: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render.sign.vertices"),
                size: (MAX_SIGNS * 6 * std::mem::size_of::<SignVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            warstwy: 1,
            drawn: 0,
        }
    }

    /// Wgrywa atlas, jeśli przybyło kafli. Tekstura rośnie **skokowo**, przez odtworzenie:
    /// szyldów przybywa kilka na sesję, więc realokacja jest tańsza niż rezerwacja
    /// czterech megabajtów na zapas.
    pub fn upload_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, atlas: &SignAtlas) {
        let n = atlas.tiles() as u32;
        if n > self.warstwy {
            let (tex, bind) = utworz(device, &self.layout, &self.sampler, &self.frame_buffer, n);
            self.texture = tex;
            self.bind = bind;
            self.warstwy = n;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels()[..(n as usize * TILE_W * TILE_H)],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TILE_W as u32),
                rows_per_image: Some(TILE_H as u32),
            },
            wgpu::Extent3d {
                width: TILE_W as u32,
                height: TILE_H as u32,
                depth_or_array_layers: n,
            },
        );
    }

    pub fn upload(&mut self, queue: &wgpu::Queue, vertices: &[SignVertex]) {
        let n = vertices.len().min(MAX_SIGNS * 6);
        self.drawn = n as u32;
        if n > 0 {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices[..n]));
        }
    }

    /// Rysuje wszystkie napisy **jednym wywołaniem** (kryterium WP9).
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

fn utworz(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    frame_buffer: &wgpu::Buffer,
    warstwy: u32,
) -> (wgpu::Texture, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render.sign.atlas"),
        size: wgpu::Extent3d {
            width: TILE_W as u32,
            height: TILE_H as u32,
            depth_or_array_layers: warstwy.max(1),
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let widok = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("render.sign.bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&widok),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (texture, bind)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prostokąt napisu ma sześć wierzchołków i leży w płaszczyźnie tablicy.
    #[test]
    fn napis_jest_prostokatem_przed_tablica() {
        let mut g = SignGeometry::default();
        g.build(
            &[SignQuad {
                center: [10.0, 20.0, 5.0],
                yaw: 0,
                tile: super::super::SignId(3),
                color: [255, 240, 200, 255],
            }],
            glam::DVec3::ZERO,
        );
        assert_eq!(g.vertices.len(), 6);
        // Przy `yaw = 0` tablica patrzy w `+X`, więc napis jest przed nią.
        assert!(g.vertices.iter().all(|v| v.pos[0] > 10.0));
        // Szerokość wzdłuż `Y`, wysokość wzdłuż `Z`.
        let y: Vec<f32> = g.vertices.iter().map(|v| v.pos[1]).collect();
        let z: Vec<f32> = g.vertices.iter().map(|v| v.pos[2]).collect();
        let rozpietosc = |v: &[f32]| {
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!((rozpietosc(&y) - SIGN_W_M).abs() < 1e-3);
        assert!((rozpietosc(&z) - SIGN_H_M).abs() < 1e-3);
        assert!(g.vertices.iter().all(|v| v.layer == 3));
    }

    /// Szyld bez kafla nie jest rysowany — `SignId(0)` znaczy „bez szyldu".
    #[test]
    fn brak_kafla_nie_daje_geometrii() {
        let mut g = SignGeometry::default();
        g.build(
            &[SignQuad {
                center: [0.0; 3],
                yaw: 0,
                tile: super::super::SignId(0),
                color: [0; 4],
            }],
            glam::DVec3::ZERO,
        );
        assert!(g.is_empty());
    }

    /// Sto szyldów mieści się w jednym buforze, czyli w jednym wywołaniu rysowania
    /// — to jest kryterium WP9 po stronie procesora.
    #[test]
    fn sto_szyldow_to_jeden_bufor() {
        let mut g = SignGeometry::default();
        let szyldy: Vec<SignQuad> = (0..100)
            .map(|i| SignQuad {
                center: [i as f32 * 8.0, 0.0, 4.0],
                yaw: (i * 512) as u16,
                tile: super::super::SignId(i as u16 + 1),
                color: [255, 255, 255, 255],
            })
            .collect();
        g.build(&szyldy, glam::DVec3::ZERO);
        assert_eq!(g.vertices.len(), 600);
        assert!(g.vertices.len() <= MAX_SIGNS * 6);
    }
}
