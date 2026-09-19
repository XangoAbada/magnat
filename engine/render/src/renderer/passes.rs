//! Nagrywanie passów klatki, pomiar czasu GPU i statystyki.
//!
//! Wydzielone z `renderer.rs` w R-WP3 bez zmiany zachowania.

use super::frame::{FrameLists, VisibleChunk, FAR_QUADS};
use super::Renderer;
use crate::camera::CameraState;
use crate::clusters::{CLUSTER_COUNT, CLUSTER_Z};
use crate::gpu::GpuContext;
use crate::shadow;
use crate::ui::UiFrame;
use magnat_core::SimMinute;

/// Nazwy mierzonych passów — indeks odpowiada parze znaczników w `QuerySet`.
pub const PASS_NAMES: [&str; 7] = [
    "clusters",
    "shadows",
    "depth_prepass",
    "opaque",
    "water",
    "post",
    // Dopisane **na końcu**, choć pass rysuje się przed `post`: indeksy znaczników
    // czasu są pozycyjne, więc wstawienie w środku przesunęłoby każdy wcześniejszy
    // pomiar i porównanie z baseline'em mierzyłoby przenumerowanie, nie zmianę kodu.
    "weather",
];

/// Pozycja passa pogody w [`PASS_NAMES`] — jedno miejsce zamiast trzech literałów.
pub(super) const PASS_WEATHER: usize = 6;

/// Statystyki klatki — wejście do licznika w tytule okna i do raportu z §7.4.
#[derive(Clone, Copy, Default, Debug)]
pub struct FrameStats {
    pub chunks_resident: usize,
    pub chunks_drawn: usize,
    pub triangles: usize,
    /// Ile encji dynamicznych poszło w buforze instancji tej klatki (M11b).
    pub instances: usize,
    /// Ile wywołań rysowania je obsłużyło — liczba, której pilnuje kryterium WP2.
    pub instance_batches: usize,
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
pub(super) struct PassTimer {
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
    pub(super) fn new(gpu: &GpuContext) -> Option<PassTimer> {
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

    /// Znaczniki obejmujące **blok** passów: początek stempluje pierwszy, koniec ostatni.
    /// Cztery kaskady cieni to cztery passy, ale jeden budżet czasu (§4, WP-R3: ≤ 2 ms).
    fn writes_block(
        &self,
        pass: usize,
        pierwszy: bool,
        ostatni: bool,
    ) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: pierwszy.then_some(2 * pass as u32),
            end_of_pass_write_index: ostatni.then_some(2 * pass as u32 + 1),
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

impl Renderer {
    /// Rysuje klatkę do okna.
    pub fn render(&mut self, camera: &CameraState, minute: SimMinute, latitude_ddeg: i16) {
        self.render_with_ui(camera, minute, latitude_ddeg, None);
    }

    /// Klatka razem z warstwą UI (M3 §5.11). `None` rysuje sam świat.
    ///
    /// Panel wchodzi **po** post-processingu, prosto na bufor ekranu — inaczej przeszedłby
    /// przez tonemapping i FXAA, czyli tekst byłby rozmyty, a kolory nie te, które podał
    /// autor panelu.
    pub fn render_with_ui(
        &mut self,
        camera: &CameraState,
        minute: SimMinute,
        latitude_ddeg: i16,
        ui: Option<UiFrame<'_>>,
    ) {
        let listy = self.prepare_frame(
            camera,
            minute,
            latitude_ddeg,
            (self.gpu.config.width, self.gpu.config.height),
        );
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
        let (triangles, odczyt, pogoda) =
            self.record_passes(&mut encoder, &view, &self.depth_view, &listy);
        if let Some(f) = ui.as_ref() {
            let rozmiar = [self.gpu.config.width, self.gpu.config.height];
            self.ui
                .prepare(&self.gpu.device, &self.gpu.queue, &mut encoder, f, rozmiar);
            self.ui.paint(&mut encoder, &view, f, rozmiar);
        }
        self.resolve_timer(&mut encoder);
        self.gpu.queue.submit(Some(encoder.finish()));
        // Mapowanie **po** wysłaniu kopii — patrz `pick::Pedestrians::record_id_pass`.
        if odczyt {
            self.pick_buffer.read_back();
        }
        self.gpu.queue.present(frame);
        self.finish_stats(&listy.widoczne, triangles, pogoda);
    }

    /// Zwraca `(trójkąty, czy odczytać bufor ID, czy pass pogody się odbył)`.
    ///
    /// Trzecia liczba nie jest statystyką: pass pominięty **nie stempluje swojej pary
    /// znaczników czasu**, a `resolve_timer` rozwiązuje cały zakres, więc bez niej
    /// pozycja „weather" w raporcie niosłaby wartość z wcześniejszej klatki. Scena
    /// sucha i bezdymna pokazywałaby czas passa, który się nie odbył (`I-25`).
    pub(super) fn record_passes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        listy: &FrameLists,
    ) -> (usize, bool, bool) {
        // Scena idzie do bufora HDR; `color` jest celem dopiero dla post-processingu.
        let scena = &self.hdr_view;
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(PASS_NAMES[0]),
                timestamp_writes: self
                    .timer
                    .as_ref()
                    .map(|t| wgpu::ComputePassTimestampWrites {
                        query_set: &t.set,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }),
            });
            pass.set_pipeline(&self.cluster_pipeline);
            pass.set_bind_group(0, Some(&self.cluster_bind), &[]);
            // Grupa robocza to jedna warstwa siatki (16 × 9), więc dispatch ma tyle grup,
            // ile warstw — patrz `clusters.wgsl`.
            pass.dispatch_workgroups(1, 1, CLUSTER_Z);
        }
        if !self
            .occupancy_w_locie
            .load(std::sync::atomic::Ordering::Acquire)
        {
            encoder.copy_buffer_to_buffer(
                &self.cluster_counts,
                0,
                &self.occupancy_readback,
                0,
                u64::from(CLUSTER_COUNT) * 4,
            );
        }

        // Kaskady idą **przed** wszystkim (slot `Offscreen` w grafie): ich wynik jest
        // wejściem passa nieprzezroczystego, a nie dodatkiem do niego.
        for (i, lista) in listy.cienie.iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow_cascade"),
                // Pośrednie kaskady nie stemplują niczego — wtedy deskryptor nie może
                // nieść pustych znaczników, bo to błąd walidacji, a nie „brak pomiaru".
                timestamp_writes: self
                    .timer
                    .as_ref()
                    .filter(|_| i == 0 || i == shadow::CASCADES - 1)
                    .map(|t| t.writes_block(1, i == 0, i == shadow::CASCADES - 1)),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_layers[i],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(1, Some(&self.cascade_binds[i]), &[]);
            self.draw_list(
                &mut pass,
                &self.shadow_bind,
                lista,
                false,
                listy.offset(2 + i),
            );
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(PASS_NAMES[2]),
                timestamp_writes: self.timer.as_ref().map(|t| t.writes(2)),
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
            pass.set_pipeline(if self.cut.mode == crate::interiors::CutMode::Off {
                &self.depth_pipeline
            } else {
                &self.depth_clip_pipeline
            });
            self.draw_list(&mut pass, &self.bind_group, &listy.widoczne, false, 0);
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(PASS_NAMES[3]),
            timestamp_writes: self.timer.as_ref().map(|t| t.writes(3)),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scena,
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

        // Daleki teren przed chunkami: zapisuje głębię, więc chunki wygrywają z nim
        // wszędzie tam, gdzie są — a tam, gdzie ich nie ma, zostaje panorama zamiast nieba.
        if let Some(bind) = self.far_bind.as_ref() {
            pass.set_pipeline(&self.far_pipeline);
            pass.set_bind_group(0, Some(&self.bind_group), &[]);
            pass.set_bind_group(1, Some(bind), &[]);
            pass.set_bind_group(2, Some(&self.overlay_bind), &[]);
            pass.draw(0..FAR_QUADS * FAR_QUADS * 6, 0..1);
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, Some(&self.overlay_bind), &[]);
        let mut trojkaty = self.draw_list(&mut pass, &self.bind_group, &listy.widoczne, false, 0);
        // Czapki przekroju zaraz po chunkach i **przed** encjami: zamykają dziurę, którą
        // zostawił `discard` w `voxel.wgsl`, a encje mają się rysować na nich, nie pod nimi
        // (mieszkaniec stojący na przeciętym piętrze).
        trojkaty += self.cap.draw(&mut pass);
        // Encje dynamiczne na końcu passa: zapisują głębię, więc bufor ID może potem odtworzyć
        // dokładnie te fragmenty porównaniem `Equal`.
        trojkaty += self.instances.draw(&mut pass);
        // Napisy na szyldach na samym końcu passa: leżą tuż przed licem tablicy, więc
        // muszą wygrać z nią głębią, a tablica jest zwykłą instancją.
        trojkaty += self.signs.draw(&mut pass);
        drop(pass);

        // Bufor ID **przed** wodą: tafla nie zapisuje głębi, więc kolejność jest tu
        // obojętna dla wyniku, ale trzymanie go tuż za passem, który głębię ustalił,
        // jest tym, co czyni porównanie `Equal` czytelnym.
        let odczyt = self
            .pick_buffer
            .record_id_pass(encoder, depth, &self.instances);

        // Woda: osobny przebieg z mieszaniem, głębia **tylko do odczytu** — pass czyta ją
        // jako teksturę, żeby policzyć grubość słupa wody, a zapis do tej samej tekstury
        // byłby jednoczesnym czytaniem i pisaniem zasobu, czyli błędem walidacji.
        if !listy.woda.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(PASS_NAMES[4]),
                timestamp_writes: self.timer.as_ref().map(|t| t.writes(4)),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scena,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.water_pipeline);
            pass.set_bind_group(1, Some(&self.water_bind), &[]);
            let t = self.draw_list(
                &mut pass,
                &self.bind_group,
                &listy.woda,
                true,
                listy.offset(1),
            );
            drop(pass);
            let pogoda = self.record_weather(encoder, scena, depth);
            self.record_post(encoder, color);
            return (trojkaty + t, odczyt, pogoda);
        }
        let pogoda = self.record_weather(encoder, scena, depth);
        self.record_post(encoder, color);
        (trojkaty, odczyt, pogoda)
    }

    /// Cząstki pogody i dymu — **po** wodzie i **przed** post-processingiem.
    ///
    /// Po wodzie, bo deszcz pada też na taflę; przed post-processingiem, bo kropla ma
    /// przejść przez ekspozycję i tonemap razem z resztą sceny — inaczej nocny deszcz
    /// byłby jaśniejszy od latarni, które go oświetlają.
    ///
    /// Sucha bezdymna scena **nie otwiera passa w ogóle**: pusty przebieg kosztuje
    /// przełączenie celu renderowania, a `bench_blackout` ma nie być wolniejszy
    /// od `bench_night_rain`.
    fn record_weather(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scena: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) -> bool {
        if self.weather_fx.is_empty() {
            return false;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(PASS_NAMES[PASS_WEATHER]),
            timestamp_writes: self.timer.as_ref().map(|t| t.writes(PASS_WEATHER)),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scena,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            // Głębia **tylko do odczytu**: cząstka zasłonięta budynkiem ma zniknąć,
            // ale dwieście tysięcy kropel zapisujących głębię zamieniłoby obraz w koc.
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: None,
            }),
            ..Default::default()
        });
        self.weather_fx.draw(&mut pass);
        true
    }

    /// Przebieg końcowy: ekspozycja, tonemap i FXAA z bufora HDR na ekran.
    fn record_post(&self, encoder: &mut wgpu::CommandEncoder, color: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(PASS_NAMES[5]),
            timestamp_writes: self.timer.as_ref().map(|t| t.writes(5)),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&self.post_pipeline);
        pass.set_bind_group(0, Some(&self.post_bind), &[]);
        pass.draw(0..3, 0..1);
    }

    /// Przepisuje znaczniki czasu do bufora odczytu. Musi iść do **tego samego** enkodera,
    /// zanim klatka trafi do kolejki — inaczej mierzyłaby się następna.
    pub(super) fn resolve_timer(&self, encoder: &mut wgpu::CommandEncoder) {
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

    /// Odbiera liczniki klastrów z poprzedniej klatki i zleca kolejny odczyt.
    /// Ten sam wzorzec co przy znacznikach czasu: mapowanie jest gotowe dopiero wtedy,
    /// gdy karta skończy klatkę, więc histogram jest o klatkę spóźniony — i to wystarcza,
    /// bo służy do kalibracji budżetu, a nie do sterowania rysowaniem.
    fn zbierz_occupancy(&mut self) {
        use std::sync::atomic::Ordering;
        if self.occupancy_gotowe.swap(false, Ordering::Acquire) {
            if let Ok(dane) = self.occupancy_readback.slice(..).get_mapped_range() {
                let counts: Vec<u32> = dane
                    .chunks_exact(4)
                    .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                drop(dane);
                self.occupancy.record(&counts);
            }
            self.occupancy_readback.unmap();
            self.occupancy_w_locie.store(false, Ordering::Release);
            return;
        }
        if !self.occupancy_w_locie.swap(true, Ordering::AcqRel) {
            let gotowe = self.occupancy_gotowe.clone();
            let w_locie = self.occupancy_w_locie.clone();
            self.occupancy_readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |wynik| {
                    if wynik.is_ok() {
                        gotowe.store(true, Ordering::Release);
                    } else {
                        w_locie.store(false, Ordering::Release);
                    }
                });
            self.gpu.device.poll(wgpu::PollType::Poll).ok();
        }
    }

    pub(super) fn finish_stats(
        &mut self,
        widoczne: &[VisibleChunk],
        triangles: usize,
        pogoda: bool,
    ) {
        if let Some(mut t) = self.timer.take() {
            t.zbierz(&self.gpu);
            self.timer = Some(t);
        }
        self.zbierz_occupancy();
        self.stats = FrameStats {
            chunks_resident: self.chunks.len(),
            chunks_drawn: widoczne.len(),
            triangles,
            instances: self.instances.drawn() as usize,
            instance_batches: self.instances.batches(),
            vertex_bytes: self.vertex_arena.bytes_used(8),
            index_bytes: self.index_arena.bytes_used(4),
            arena_capacity_bytes: self.vertex_arena.capacity_bytes(8)
                + self.index_arena.capacity_bytes(4),
            pass_ms: {
                let mut ms = self
                    .timer
                    .as_ref()
                    .map_or([0.0; PASS_NAMES.len()], |t| t.ms);
                // Pass pominięty nie stemplował swojej pary znaczników, więc w buforze
                // leży wartość z klatki, w której się odbył. Zero znaczy „nie było
                // passa"; liczba z przeszłości znaczyłaby „był i kosztował".
                if !pogoda {
                    ms[PASS_WEATHER] = 0.0;
                }
                ms
            },
            gpu_timing: self.timer.is_some(),
        };
    }
}
