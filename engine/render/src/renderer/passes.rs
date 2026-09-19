//! Nagrywanie passów klatki, pomiar czasu GPU i statystyki.
//!
//! Wydzielone z `renderer.rs` w R-WP3 bez zmiany zachowania.

use super::frame::{FrameLists, FAR_QUADS};
use super::stats::{PASS_NAMES, PASS_PICK, PASS_WEATHER};
use super::Renderer;
use crate::camera::CameraState;
use crate::clusters::{CLUSTER_COUNT, CLUSTER_Z};
use crate::shadow;
use crate::ui::UiFrame;
use magnat_core::SimMinute;

/// Co klatka zgłasza poza zawartością bufora ekranu.
#[derive(Clone, Copy, Default)]
pub(super) struct PassOutcome {
    pub(super) triangles: usize,
    pub(super) draw_calls: u32,
    /// Czy po `submit` trzeba odczytać piksel bufora identyfikatorów.
    pub(super) odczyt_id: bool,
    /// Czy pass pogody się odbył — inaczej jego pozycja w `pass_ms` idzie na zero.
    pub(super) pogoda: bool,
    /// To samo dla passa bufora identyfikatorów.
    pub(super) pick: bool,
    /// I dla passa wody: kadr bez ani jednego chunka z taflą go nie otwiera.
    pub(super) woda: bool,
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
        let zegar = std::time::Instant::now();
        let listy = self.prepare_frame(
            camera,
            minute,
            latitude_ddeg,
            (self.gpu.config.width, self.gpu.config.height),
        );
        // Zegar procesora zatrzymuje się **przed** pobraniem tekstury powierzchni
        // i rusza po nim. `get_current_texture` blokuje do synchronizacji pionowej,
        // więc wliczony dawał 25 ms „kosztu procesora" na scenie, której GPU
        // zajmowało 5 — czyli mierzył monitor, nie kod.
        let przygotowanie = zegar.elapsed();
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

        let zegar = std::time::Instant::now();
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render.frame"),
            });
        let wynik = self.record_passes(&mut encoder, &view, &self.depth_view, &listy);
        if let Some(f) = ui.as_ref() {
            let rozmiar = [self.gpu.config.width, self.gpu.config.height];
            self.ui
                .prepare(&self.gpu.device, &self.gpu.queue, &mut encoder, f, rozmiar);
            self.ui.paint(&mut encoder, &view, f, rozmiar);
        }
        self.resolve_timer(&mut encoder, wynik);
        self.gpu.queue.submit(Some(encoder.finish()));
        // Mapowanie **po** wysłaniu kopii — patrz `pick::Pedestrians::record_id_pass`.
        if wynik.odczyt_id {
            self.pick_buffer.read_back();
        }
        self.gpu.queue.present(frame);
        self.finish_stats(&listy.widoczne, wynik, przygotowanie + zegar.elapsed());
    }

    /// Co z klatki wyszło poza samą zawartością bufora ekranu.
    ///
    /// `pogoda` i `pick` nie są statystyką: pass pominięty **nie stempluje swojej pary
    /// znaczników czasu**, a `resolve_timer` rozwiązuje cały zakres, więc bez nich
    /// pozycja w raporcie niosłaby wartość z wcześniejszej klatki. Scena sucha
    /// i bezdymna pokazywałaby czas passa, który się nie odbył (`I-25`).
    pub(super) fn record_passes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        listy: &FrameLists,
    ) -> PassOutcome {
        // Scena idzie do bufora HDR; `color` jest celem dopiero dla post-processingu.
        let scena = &self.hdr_view;
        let mut wywolania = 0u32;
        self.record_clusters(encoder);
        wywolania += self.record_shadows(encoder, listy);
        wywolania += self.record_depth_prepass(encoder, depth, listy);

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
        wywolania += 1;

        // Daleki teren przed chunkami: zapisuje głębię, więc chunki wygrywają z nim
        // wszędzie tam, gdzie są — a tam, gdzie ich nie ma, zostaje panorama zamiast nieba.
        if let Some(bind) = self.far_bind.as_ref() {
            pass.set_pipeline(&self.far_pipeline);
            pass.set_bind_group(0, Some(&self.bind_group), &[]);
            pass.set_bind_group(1, Some(bind), &[]);
            pass.set_bind_group(2, Some(&self.overlay_bind), &[]);
            pass.draw(0..FAR_QUADS * FAR_QUADS * 6, 0..1);
            wywolania += 1;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, Some(&self.overlay_bind), &[]);
        let (mut trojkaty, w) =
            self.draw_list(&mut pass, &self.bind_group, &listy.widoczne, false, 0);
        wywolania += w;
        // Czapki przekroju zaraz po chunkach i **przed** encjami: zamykają dziurę, którą
        // zostawił `discard` w `voxel.wgsl`, a encje mają się rysować na nich, nie pod nimi
        // (mieszkaniec stojący na przeciętym piętrze).
        trojkaty += self.cap.draw(&mut pass);
        wywolania += u32::from(self.cap.drawn() > 0);
        // Encje dynamiczne na końcu passa: zapisują głębię, więc bufor ID może potem odtworzyć
        // dokładnie te fragmenty porównaniem `Equal`.
        let (t, w) = self.instances.draw(&mut pass);
        trojkaty += t;
        wywolania += w;
        // Napisy na szyldach na samym końcu passa: leżą tuż przed licem tablicy, więc
        // muszą wygrać z nią głębią, a tablica jest zwykłą instancją.
        trojkaty += self.signs.draw(&mut pass);
        wywolania += u32::from(self.signs.drawn() > 0);
        drop(pass);

        // Bufor ID **przed** wodą: tafla nie zapisuje głębi, więc kolejność jest tu
        // obojętna dla wyniku, ale trzymanie go tuż za passem, który głębię ustalił,
        // jest tym, co czyni porównanie `Equal` czytelnym.
        //
        // Pass **nie otwiera się bez kursora** (`H-4`). Rysuje pełną geometrię wszystkich
        // encji drugi raz i czyści teksturę identyfikatorów wielkości okna, a bez kursora
        // nikt tych bajtów nie czyta: `set_cursor(None)` zeruje trafienie, a kopii piksela
        // i tak nie było. Dotyczy to każdego zrzutu offscreen i każdej sceny pomiarowej.
        let pick = self.pick_buffer.aktywny();
        let mut w_pick = 0;
        let odczyt = pick
            && self.pick_buffer.record_id_pass(
                encoder,
                depth,
                &self.instances,
                self.timer.as_ref().map(|t| t.writes(PASS_PICK)),
                &mut w_pick,
            );
        wywolania += w_pick;

        // Woda: osobny przebieg z mieszaniem, głębia **tylko do odczytu** — pass czyta ją
        // jako teksturę, żeby policzyć grubość słupa wody, a zapis do tej samej tekstury
        // byłby jednoczesnym czytaniem i pisaniem zasobu, czyli błędem walidacji.
        let woda = !listy.woda.is_empty();
        if woda {
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
            let (t, w) = self.draw_list(
                &mut pass,
                &self.bind_group,
                &listy.woda,
                true,
                listy.offset(1),
            );
            drop(pass);
            trojkaty += t;
            wywolania += w;
        }
        let (pogoda, w) = self.record_weather(encoder, scena, depth);
        wywolania += w;
        self.record_post(encoder, color);
        // Post-processing to jeden trójkąt na cały ekran; warstwa UI doklejana przez
        // wołającego nie jest passem sceny i do budżetu z §5.5 nie wchodzi.
        wywolania += 1;
        PassOutcome {
            triangles: trojkaty,
            draw_calls: wywolania,
            odczyt_id: odczyt,
            pogoda,
            pick,
            woda,
        }
    }

    /// Przypisanie świateł do klastrów — jedyny przebieg obliczeniowy klatki.
    ///
    /// Kopia liczników do odczytu idzie tym samym enkoderem: histogram zajętości
    /// (`ClusterOccupancy`) steruje przerzedzaniem latarni po stronie producenta listy,
    /// a odczyt spóźniony o klatkę nie przeszkadza, bo histereza go wygładza.
    fn record_clusters(&self, encoder: &mut wgpu::CommandEncoder) {
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
    }

    /// Cztery kaskady cieni. Idą **przed** wszystkim (slot `Offscreen` w grafie): ich
    /// wynik jest wejściem passa nieprzezroczystego, a nie dodatkiem do niego.
    ///
    /// Zwraca liczbę wywołań rysowania.
    fn record_shadows(&self, encoder: &mut wgpu::CommandEncoder, listy: &FrameLists) -> u32 {
        let mut wywolania = 0;
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
            wywolania += self
                .draw_list(
                    &mut pass,
                    &self.shadow_bind,
                    lista,
                    false,
                    listy.offset(2 + i),
                )
                .1;
        }
        wywolania
    }

    /// Przebieg głębi. Przy aktywnym cięciu poziomami idzie potokiem z odrzucaniem —
    /// bez niego przekrój jest czarną dziurą, bo prepass bez shadera fragmentu zapisuje
    /// głębię także tam, gdzie pass nieprzezroczysty zaraz odrzuci fragment (M11c §5.7).
    fn record_depth_prepass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        listy: &FrameLists,
    ) -> u32 {
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
        self.draw_list(&mut pass, &self.bind_group, &listy.widoczne, false, 0)
            .1
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
    ) -> (bool, u32) {
        if self.weather_fx.is_empty() {
            return (false, 0);
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
        let n = self.weather_fx.draw(&mut pass);
        (true, n)
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
}
