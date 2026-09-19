//! Klatka rysowana do obrazu w pamięci zamiast na ekran.
//!
//! Wydzielone z `passes.rs` w M11d bez zmiany zachowania: nagrywanie passów i obsługa
//! zrzutu to dwa tematy, a blok `impl Renderer` przekroczył próg strukturalny dopiero
//! wtedy, gdy doszedł do niego pass pogody (CLAUDE.md, „przegląd strukturalny
//! po zamkniętym pakiecie").
//!
//! Zrzut istnieje po to, żeby renderer dało się **zweryfikować bez patrzenia na ekran**.
//! Połowa tego kodu to wyrównanie wierszy kopii tekstury do 256 B i rozpoznanie, czy
//! sterownik oddaje BGRA czy RGBA — czyli rzeczy, które z passami nie mają nic wspólnego
//! poza tym, że dzieją się w tej samej klatce.

use super::pipelines::create_depth;
use super::Renderer;
use crate::camera::CameraState;
use crate::ui::UiFrame;
use magnat_core::SimMinute;

impl Renderer {
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
        self.render_to_image_with_ui(camera, minute, latitude_ddeg, width, height, None)
    }

    /// Zrzut razem z warstwą UI. Istnieje po to, żeby panel dało się **zobaczyć**
    /// w raporcie z CI — inaczej jedynym dowodem na to, że `egui` się wpięło, byłoby
    /// słowo autora.
    pub fn render_to_image_with_ui(
        &mut self,
        camera: &CameraState,
        minute: SimMinute,
        latitude_ddeg: i16,
        width: u32,
        height: u32,
        ui: Option<UiFrame<'_>>,
    ) -> (u32, u32, Vec<u8>) {
        let listy = self.prepare_frame(camera, minute, latitude_ddeg, (width, height));
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
        // Zrzut offscreen nie ma kursora, więc kopia piksela ID i tak nie powstaje.
        let (triangles, _, pogoda) = self.record_passes(&mut encoder, &color_view, &depth, &listy);
        if let Some(f) = ui.as_ref() {
            self.ui
                .prepare(device, &self.gpu.queue, &mut encoder, f, [width, height]);
            self.ui.paint(&mut encoder, &color_view, f, [width, height]);
        }
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

        self.finish_stats(&listy.widoczne, triangles, pogoda);
        (width, height, rgb)
    }
}
