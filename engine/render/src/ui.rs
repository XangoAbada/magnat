//! Warstwa UI w klatce (M3 §5.11, decyzja 9.2: `egui` + `egui-wgpu`).
//!
//! **Podział jest celowy i przebiega tutaj.** `engine/ui` buduje *co* ma być na ekranie —
//! karty, osie czasu, etykiety w języku gracza — i nie wie nic o `wgpu`; dzięki temu
//! ma testy, które chodzą w CI bez karty graficznej. Ten moduł robi jedną rzecz, której
//! tamten zrobić nie może: maluje gotowe trójkąty `egui` na buforze ekranu, w klatce,
//! której właścicielem jest [`crate::Renderer`] (K-3: od M1 urządzenie należy do renderu).
//!
//! Warstwa idzie **po** post-processingu, prosto na bufor ekranu. Nie do HDR — panel
//! ma nie przechodzić przez tonemapping ani FXAA: kolory UI są podane w sRGB i mają
//! takie wyjść, a rozmycie krawędzi na tekście wygląda jak wada sterownika.

use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};

/// Trójkąty i tekstury jednej klatki UI — wszystko, czego renderer potrzebuje, żeby
/// namalować panel, i nic ponadto.
pub struct UiFrame<'a> {
    pub jobs: &'a [egui::ClippedPrimitive],
    pub textures_delta: &'a egui::TexturesDelta,
    pub pixels_per_point: f32,
}

pub struct UiLayer {
    egui: EguiRenderer,
}

impl UiLayer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> UiLayer {
        UiLayer {
            egui: EguiRenderer::new(
                device,
                format,
                RendererOptions {
                    // Bez MSAA i bez głębi: panel jest płaski i rysuje się na samym końcu.
                    // `egui` wygładza krawędzie sam, przez „feathering".
                    msaa_samples: 1,
                    depth_stencil_format: None,
                    ..Default::default()
                },
            ),
        }
    }

    /// Wgrywa tekstury i bufory klatki UI. Musi iść **przed** przebiegiem malowania,
    /// do tego samego enkodera.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &UiFrame<'_>,
        size: [u32; 2],
    ) {
        // `set` niesie **listę** delt na teksturę: `egui` potrafi w jednej klatce
        // dołożyć atlas i zaraz go poprawić, a kolejność ma znaczenie.
        for (id, delty) in &frame.textures_delta.set {
            for d in delty {
                self.egui.update_texture(device, queue, *id, d);
            }
        }
        let desc = ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: frame.pixels_per_point,
        };
        self.egui
            .update_buffers(device, queue, encoder, frame.jobs, &desc);
    }

    /// Maluje panel na buforze ekranu. `load`, nie `clear` — pod spodem leży scena.
    pub fn paint(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        frame: &UiFrame<'_>,
        size: [u32; 2],
    ) {
        let desc = ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: frame.pixels_per_point,
        };
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            // `egui-wgpu` wymaga przebiegu o czasie życia `'static`; przebieg i tak kończy
            // się przed końcem tej funkcji, więc zdjęcie lifetime'u jest tu bezpieczne
            // i jest zalecanym sposobem użycia tej biblioteki.
            let mut pass = pass.forget_lifetime();
            self.egui.render(&mut pass, frame.jobs, &desc);
        }
        // Tekstury zwalniamy **po** malowaniu: `egui` zgłasza do zwolnienia atlas,
        // z którego ta klatka jeszcze korzystała.
        for id in &frame.textures_delta.free {
            self.egui.free_texture(id);
        }
    }
}
