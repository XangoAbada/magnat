//! `magnat-voxelview` — bootstrap okna i stosu graficznego (M0 §5.12, WP-15).
//!
//! Zero powiązania z ECS i zero generatora terenu: to celowo ślepy zaułek, z którego
//! M1 przenosi **wyłącznie** `init_gpu()` i obsługę surface do `engine/render` (D-5).
//! Reszta — siatka, pipeline, shader — jest do skasowania.
//!
//! Tu wolno używać `f64::sin` i spółki: wynik nie wchodzi do stanu trwałego,
//! a lokalny `clippy.toml` zdejmuje zakaz z 00 §K-6.

#![forbid(unsafe_code)]

mod chunk_mesh;

use chunk_mesh::{Vertex, CHUNK_SIZE};
use glam::{Mat4, Vec3};

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// To jest jedyna część tego narzędzia, którą M1 przejmuje do `engine/render`.
pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

impl GpuContext {
    /// Łańcuch inicjalizacji: instancja → adapter → urządzenie → surface.
    ///
    /// Limity `downlevel_defaults` są celowe: łapią wymagania sprzętowe **teraz**,
    /// a nie w M11, gdy okaże się, że pipeline nie startuje na połowie kart.
    fn new(window: Arc<Window>) -> GpuContext {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(window.clone())
            .expect("nie udało się utworzyć powierzchni okna");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("brak adaptera GPU zgodnego z tą powierzchnią");

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("magnat-voxelview"),
            required_features: wgpu::Features::empty(),
            // Limity downlevel łapią wymagania sprzętowe wcześnie, ale rozdzielczość
            // bierzemy z adaptera: inaczej okno na monitorze 4K nie przechodzi
            // walidacji, bo downlevel dopuszcza tekstury tylko do 2048 px.
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("nie udało się utworzyć urządzenia GPU");

        let size = window.inner_size();
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        GpuContext {
            instance,
            surface,
            adapter,
            device,
            queue,
            config,
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }
}

/// Kamera orbitalna: kąt poziomy, pionowy i dystans.
struct Camera {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Camera {
    fn view_proj(&self, aspect: f32) -> Mat4 {
        let cel = Vec3::splat(CHUNK_SIZE as f32 * 0.5);
        let oko = cel
            + Vec3::new(
                self.distance * self.pitch.cos() * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                self.distance * self.pitch.cos() * self.yaw.cos(),
            );
        // NDC z zakresem Z w [0,1] — konwencja wgpu/DirectX, nie OpenGL.
        let proj =
            glam::camera::rh::proj::directx::perspective(60f32.to_radians(), aspect, 0.1, 500.0);
        proj * glam::camera::rh::view::look_at_mat4(oko, cel, Vec3::Y)
    }
}

struct Renderer {
    gpu: GpuContext,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
}

impl Renderer {
    fn new(gpu: GpuContext) -> Renderer {
        let (vertices, indices) = chunk_mesh::build();
        let device = &gpu.device;

        let vertex_buffer = create_buffer(
            device,
            "voxelview.vertices",
            bytemuck::cast_slice(&vertices),
            wgpu::BufferUsages::VERTEX,
        );
        let index_buffer = create_buffer(
            device,
            "voxelview.indices",
            bytemuck::cast_slice(&indices),
            wgpu::BufferUsages::INDEX,
        );
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxelview.camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("voxelview.camera.layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("voxelview.camera.bind"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxelview.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("voxelview.pipeline.layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxelview.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::LAYOUT)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let depth_view = create_depth_view(&gpu);

        Renderer {
            gpu,
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            camera_buffer,
            camera_bind_group,
            depth_view,
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.gpu.resize(size);
        self.depth_view = create_depth_view(&self.gpu);
    }

    /// `Ok(false)` = powierzchnia wymaga rekonfiguracji (zmiana DPI, przełączenie GPU) —
    /// to normalne zdarzenie okna, nie awaria.
    fn render(&mut self, camera: &Camera) -> Result<bool, String> {
        let aspect = self.gpu.config.width as f32 / self.gpu.config.height.max(1) as f32;
        let view_proj = camera.view_proj(aspect);
        self.gpu.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&view_proj.to_cols_array()),
        );

        let frame = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                return Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(true)
            }
            inne => return Err(format!("powierzchnia nie wydała klatki: {inne:?}")),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("voxelview.encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("voxelview.pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.07,
                            b: 0.10,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }
        self.gpu.queue.submit(Some(encoder.finish()));
        // wgpu 30: prezentacja przeszła z tekstury na kolejkę.
        self.gpu.queue.present(frame);
        Ok(true)
    }
}

fn create_buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
}

fn create_depth_view(gpu: &GpuContext) -> wgpu::TextureView {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("voxelview.depth"),
        size: wgpu::Extent3d {
            width: gpu.config.width.max(1),
            height: gpu.config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

struct VoxelView {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    obracanie: bool,
    ostatnia_pozycja: Option<(f64, f64)>,
    klatki: u32,
    ostatni_pomiar: std::time::Instant,
}

impl Default for VoxelView {
    fn default() -> Self {
        VoxelView {
            window: None,
            renderer: None,
            camera: Camera {
                yaw: 0.8,
                pitch: 0.6,
                distance: 70.0,
            },
            obracanie: false,
            ostatnia_pozycja: None,
            klatki: 0,
            ostatni_pomiar: std::time::Instant::now(),
        }
    }
}

impl ApplicationHandler for VoxelView {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("magnat-voxelview")
                        .with_inner_size(PhysicalSize::new(1280, 720)),
                )
                .expect("nie udało się utworzyć okna"),
        );
        let gpu = GpuContext::new(window.clone());
        log::info!(
            "GPU: {} ({:?})",
            gpu.adapter.get_info().name,
            gpu.adapter.get_info().backend
        );
        self.renderer = Some(Renderer::new(gpu));
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(window), Some(renderer)) = (&self.window, &mut self.renderer) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => renderer.resize(size),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.obracanie = state == ElementState::Pressed;
                if !self.obracanie {
                    self.ostatnia_pozycja = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.obracanie {
                    if let Some((px, py)) = self.ostatnia_pozycja {
                        let dx = (position.x - px) as f32;
                        let dy = (position.y - py) as f32;
                        self.camera.yaw -= dx * 0.005;
                        self.camera.pitch = (self.camera.pitch + dy * 0.005).clamp(-1.45, 1.45);
                    }
                    self.ostatnia_pozycja = Some((position.x, position.y));
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let krok = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                };
                self.camera.distance = (self.camera.distance - krok * 3.0).clamp(10.0, 200.0);
            }
            WindowEvent::RedrawRequested => {
                match renderer.render(&self.camera) {
                    Ok(true) => {}
                    Ok(false) => renderer.resize(window.inner_size()),
                    Err(e) => {
                        log::error!("{e}");
                        event_loop.exit();
                    }
                }

                self.klatki += 1;
                let uplynelo = self.ostatni_pomiar.elapsed();
                if uplynelo.as_secs_f32() >= 1.0 {
                    let fps = self.klatki as f32 / uplynelo.as_secs_f32();
                    window.set_title(&format!(
                        "magnat-voxelview — {fps:.0} FPS · {} trójkątów",
                        renderer.index_count / 3
                    ));
                    self.klatki = 0;
                    self.ostatni_pomiar = std::time::Instant::now();
                }
                window.request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("pętla zdarzeń");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = VoxelView::default();
    event_loop.run_app(&mut app).expect("pętla zdarzeń");
}
