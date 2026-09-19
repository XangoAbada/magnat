//! Bufor identyfikatorów: w co gracz kliknął (M3 §5.11, decyzje 9.3 i 9.17; M11a WP2).
//!
//! Żeby wiedzieć, w kogo się kliknęło, trzeba narysować encje **drugi raz** — do tekstury,
//! w której piksel niesie numer encji zamiast koloru.
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
//! Dlatego oba potoki stoją na jednym module `instance.wgsl` i jednym `vs_main`.
//!
//! ### Co ten moduł robi, a czego nie
//!
//! Po M11a nie rysuje **niczego**. Trzyma teksturę identyfikatorów, kursor i odczyt
//! jednego piksela; geometrię wstawia `instancing::InstanceRenderer`, bo to on ją ma.
//! Podział jest taki, bo to dwie różne rzeczy: cel rysowania i to, co się na nim rysuje.
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

use crate::instancing::{decode_pick, InstanceRenderer, PickHit};

/// Format bufora ID. `R32Uint`, a nie `R16Uint`, bo wartość niesie indeks encji
/// i jej rodzaj — a miasto z 274 tys. mieszkańców przekracza 65 535 w pierwszej
/// wygenerowanej metropolii.
pub const ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

/// Wyrównanie wiersza przy kopiowaniu tekstury do bufora (wymóg `wgpu`).
const ROW_ALIGN: u64 = 256;

pub struct PickBuffer {
    id_texture: wgpu::Texture,
    id_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    /// Piksel kopiowany w tej klatce. `None` = kursor poza oknem, nie kopiujemy nic.
    cursor: Option<(u32, u32)>,
    /// Ostatnia odczytana wartość bufora ID; 0 = nic pod kursorem.
    trafienie: Arc<AtomicU32>,
    w_locie: Arc<AtomicBool>,
}

impl PickBuffer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> PickBuffer {
        let (id_texture, id_view) = create_id(device, width, height);
        PickBuffer {
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

    /// Czy klatka ma w ogóle otwierać pass bufora identyfikatorów.
    ///
    /// Bez kursora nikt jego bajtów nie czyta — `set_cursor(None)` zeruje trafienie,
    /// a kopii piksela i tak nie ma. Rysowanie pełnej geometrii wszystkich encji drugi
    /// raz i czyszczenie tekstury wielkości okna byłoby wtedy pracą bez odbiorcy (`H-4`).
    #[must_use]
    pub fn aktywny(&self) -> bool {
        self.cursor.is_some()
    }

    /// W co gracz celuje, z **poprzedniej** klatki.
    #[must_use]
    pub fn hovered(&self) -> Option<PickHit> {
        decode_pick(self.trafienie.load(Ordering::Acquire))
    }

    /// Przebieg bufora ID plus kopia piksela spod kursora.
    ///
    /// Zwraca `true`, gdy kopia trafiła do enkodera — wtedy po `submit` trzeba wołać
    /// [`PickBuffer::read_back`]. **Mapowanie nie może iść tutaj**: `map_async` na buforze,
    /// do którego kolejka ma jeszcze niewysłaną kopię, to błąd walidacji („still mapped"
    /// przy `Queue::submit`). Kolejność jest więc twarda: nagraj → wyślij → mapuj.
    #[must_use]
    pub fn record_id_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        instances: &InstanceRenderer,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
        wywolania: &mut u32,
    ) -> bool {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pick_id"),
                timestamp_writes,
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
            *wywolania = instances.draw_ids(&mut pass);
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
