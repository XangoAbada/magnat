//! Piesi w kadrze i bufor identyfikatorów (M3 §5.11, decyzje 9.3 i 9.17).
//!
//! Dwie rzeczy, bo są tą samą rzeczą widzianą z dwóch stron: żeby kliknąć w mieszkańca,
//! trzeba go najpierw narysować, a żeby wiedzieć, w którego się kliknęło, trzeba narysować
//! go **drugi raz** — do tekstury, w której piksel niesie numer encji zamiast koloru.
//!
//! ### Dlaczego osobny pass, a nie drugi cel passa głównego
//!
//! Bufor ID jest rysowany **po** passie nieprzezroczystym, z porównaniem głębi `Equal`
//! i bez zapisu. Dzięki temu przesłanianie wychodzi za darmo: pieszy za ścianą nie zdaje
//! testu głębi, bo w buforze siedzi już głębia ściany. Alternatywa — drugi cel koloru
//! w passie głównym — kazałaby dopisać wyjście do shadera terenu, wody i dalekiego terenu,
//! czyli zapłacić przepustowością za każdy piksel nieba po to, żeby wiedzieć, że jest niebem.
//!
//! Wymaga to jednego: geometria w obu przebiegach musi być **bit w bit ta sama**.
//! Dlatego oba potoki stoją na jednym module `pedestrian.wgsl` i jednym `vs_main`.
//!
//! ### Odczyt bez zatrzymania klatki
//!
//! `hovered` nie czyta GPU w momencie kliknięcia — to kosztowałoby `submit`, `map`
//! i oczekiwanie, czyli zacięcie na całą klatkę przy każdym kliknięciu. Zamiast tego każda
//! klatka kopiuje **jeden piksel** spod kursora (256 B, tyle wynosi minimalne wyrównanie
//! wiersza) do bufora odczytu, a wynik ląduje w `trafienie`. Kliknięcie czyta wartość
//! sprzed klatki. Opóźnienie jednej klatki jest własnością bufora ID, a nie kompromisem:
//! przy 60 Hz to 16 ms, a kursor stoi w miejscu w chwili kliknięcia. Przy okazji
//! podświetlenie pod kursorem (M9) dostaje ten sam odczyt bez dodatkowej pracy.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use magnat_sim_snapshot::PedestrianRecord;

/// Format bufora ID. `R32Uint`, a nie `R16Uint`, bo indeks encji jest `u32`, a miasto
/// z 274 tys. mieszkańców przekracza 65 535 w pierwszej wygenerowanej metropolii.
pub const ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

/// Ile pieszych trafia na GPU w jednej klatce.
///
/// ponytail: twardy cap zamiast rosnącego bufora. 65 536 instancji to 1 MB zapisu na klatkę
/// i 786 tys. trójkątów — a po odcięciu promieniem i stożkiem widzenia realna liczba
/// w kadrze ulicznym idzie w tysiące. Ścieżka wyjścia, gdyby M11 chciało tłum na placu:
/// sortowanie po odległości i LOD instancji (dalsi jako billboard zamiast bryły).
pub const MAX_PEDESTRIANS: usize = 64 * 1024;

/// Za tym promieniem pieszy jest mniejszy od piksela i przestaje być rysowany.
/// To odcięcie, nie LOD — LOD wnosi M11.
pub const DRAW_RADIUS_M: f32 = 600.0;

/// Wyrównanie wiersza przy kopiowaniu tekstury do bufora (wymóg `wgpu`).
const ROW_ALIGN: u64 = 256;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    /// Pozycja **względem kamery** w metrach (`camera.rs`: absolutne nie wchodzą do shadera).
    pos: [f32; 3],
    entity: u32,
}

pub struct Pedestrians {
    pipeline: wgpu::RenderPipeline,
    id_pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    instances: wgpu::Buffer,
    count: u32,
    id_texture: wgpu::Texture,
    id_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    /// Piksel kopiowany w tej klatce. `None` = kursor poza oknem, nie kopiujemy nic.
    cursor: Option<(u32, u32)>,
    /// Ostatni odczytany identyfikator, przesunięty o jeden (0 = nic pod kursorem).
    trafienie: Arc<AtomicU32>,
    w_locie: Arc<AtomicBool>,
    /// Bufor roboczy `upload`, żeby klatka nie alokowała.
    scratch: Vec<Instance>,
}

impl Pedestrians {
    pub fn new(
        device: &wgpu::Device,
        frame_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> Pedestrians {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render.pedestrian"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/pedestrian.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render.pedestrian.layout"),
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
            label: Some("render.pedestrian.bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render.pedestrian.pipeline.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let atrybuty = [
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
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &atrybuty,
        };
        let primitive = wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render.pedestrian.opaque"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(instance_layout.clone())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
            label: Some("render.pedestrian.id"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(instance_layout)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_id"),
                targets: &[Some(ID_FORMAT.into())],
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
        let (id_texture, id_view) = create_id(device, width, height);
        Pedestrians {
            pipeline,
            id_pipeline,
            bind,
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render.pedestrian.instances"),
                size: (MAX_PEDESTRIANS * std::mem::size_of::<Instance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            count: 0,
            id_texture,
            id_view,
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render.pick.readback"),
                size: ROW_ALIGN,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            cursor: None,
            trafienie: Arc::new(AtomicU32::new(0)),
            w_locie: Arc::new(AtomicBool::new(false)),
            scratch: Vec::with_capacity(4096),
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let (t, v) = create_id(device, width, height);
        self.id_texture = t;
        self.id_view = v;
    }

    /// Gdzie stoi kursor. `None` = poza oknem, wtedy klatka nie kopiuje piksela.
    pub fn set_cursor(&mut self, pos: Option<(u32, u32)>) {
        self.cursor = pos;
        if pos.is_none() {
            self.trafienie.store(0, Ordering::Release);
        }
    }

    /// Indeks encji pod kursorem z **poprzedniej** klatki albo `None`.
    #[must_use]
    pub fn hovered(&self) -> Option<u32> {
        match self.trafienie.load(Ordering::Acquire) {
            0 => None,
            n => Some(n - 1),
        }
    }

    #[must_use]
    pub fn drawn(&self) -> u32 {
        self.count
    }

    /// Wgrywa pieszych widocznych w kadrze. Odcina promieniem i stożkiem widzenia,
    /// bo z 274 tys. mieszkańców w kadrze ulicznym widać tysiące.
    pub fn upload(
        &mut self,
        queue: &wgpu::Queue,
        peds: &[PedestrianRecord],
        eye: glam::DVec3,
        frustum: &[glam::Vec4; 6],
    ) {
        self.scratch.clear();
        let oko = eye.as_vec3();
        for p in peds {
            let pos = glam::Vec3::from(p.pos);
            let wzgledem = pos - oko;
            if wzgledem.length_squared() > DRAW_RADIUS_M * DRAW_RADIUS_M {
                continue;
            }
            // Kula o promieniu 1,2 m obejmuje bryłę 0,5 × 0,5 × 1,75 m liczoną od gruntu
            // w górę, więc środkiem testu jest punkt metr nad pozycją.
            //
            // Test idzie na pozycji **względem kamery**, bo `view_proj_relative` daje
            // płaszczyzny w tym samym układzie. Podanie tu pozycji absolutnej odrzuca
            // wszystko i jest niewidoczne inaczej niż jako „pieszych zero".
            if !crate::renderer::sphere_in_frustum(
                frustum,
                wzgledem + glam::Vec3::new(0.0, 0.0, 1.0),
                1.2,
            ) {
                continue;
            }
            self.scratch.push(Instance {
                pos: wzgledem.into(),
                entity: p.entity,
            });
            if self.scratch.len() == MAX_PEDESTRIANS {
                break;
            }
        }
        self.count = self.scratch.len() as u32;
        if self.count > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.scratch));
        }
    }

    /// Rysuje pieszych w passie nieprzezroczystym. Zwraca liczbę trójkątów.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> usize {
        if self.count == 0 {
            return 0;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, Some(&self.bind), &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..36, 0..self.count);
        self.count as usize * 12
    }

    /// Przebieg bufora ID plus kopia piksela spod kursora.
    ///
    /// Zwraca `true`, gdy kopia trafiła do enkodera — wtedy po `submit` trzeba wołać
    /// [`Pedestrians::read_back`]. **Mapowanie nie może iść tutaj**: `map_async` na buforze,
    /// do którego kolejka ma jeszcze niewysłaną kopię, to błąd walidacji („still mapped"
    /// przy `Queue::submit`). Kolejność jest więc twarda: nagraj → wyślij → mapuj.
    #[must_use]
    pub fn record_id_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
    ) -> bool {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pick_id"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.id_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
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
            if self.count > 0 {
                pass.set_pipeline(&self.id_pipeline);
                pass.set_bind_group(0, Some(&self.bind), &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                pass.draw(0..36, 0..self.count);
            }
        }

        let Some((x, y)) = self.cursor else {
            return false;
        };
        let rozmiar = self.id_texture.size();
        if x >= rozmiar.width || y >= rozmiar.height {
            return false;
        }
        // Zapis do zamapowanego bufora to błąd walidacji — dopóki poprzedni odczyt
        // nie wrócił, klatka nie kopiuje. Ten sam wzorzec, co przy znacznikach czasu.
        if self.w_locie.load(Ordering::Acquire) {
            return false;
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.id_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW_ALIGN as u32),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.w_locie.store(true, Ordering::Release);
        true
    }

    /// Zamawia odczyt piksela. Wołane **po** `Queue::submit` tej samej klatki.
    pub fn read_back(&self) {
        let trafienie = Arc::clone(&self.trafienie);
        let w_locie = Arc::clone(&self.w_locie);
        let bufor = self.readback.clone();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                if r.is_ok() {
                    if let Ok(dane) = bufor.slice(..).get_mapped_range() {
                        let v = u32::from_le_bytes([dane[0], dane[1], dane[2], dane[3]]);
                        drop(dane);
                        trafienie.store(v, Ordering::Release);
                    }
                    bufor.unmap();
                }
                w_locie.store(false, Ordering::Release);
            });
    }
}

fn create_id(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render.pick.id"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ID_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let v = t.create_view(&wgpu::TextureViewDescriptor::default());
    (t, v)
}
