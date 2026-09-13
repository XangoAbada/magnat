//! Kontekst GPU: urządzenie, kolejka i powierzchnia okna (M1 WP-R1, 00 §K-3).
//!
//! Kod przejęty z `tools/voxelview` M0 — zgodnie z rozstrzygnięciem K-3 renderer M0 był
//! rusztowaniem do wyrzucenia, a od tej chwili właścicielem urządzenia jest `engine/render`.
//!
//! Limity `downlevel_defaults` są celowe i zostają: łapią wymagania sprzętowe **teraz**,
//! a nie w M11, gdy okazałoby się, że pipeline nie startuje na połowie kart. Rozdzielczość
//! bierzemy z adaptera, bo downlevel dopuszcza tekstury tylko do 2048 px, a okno na monitorze
//! 4K nie przeszłoby walidacji.

use std::sync::Arc;
use winit::window::Window;

/// Urządzenie i powierzchnia. Jedna instancja na proces.
pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    /// Czy da się rysować pośrednio: sterownik wykonuje polecenia z bufora
    /// (`INDIRECT_EXECUTION`) **i** dopuszcza niezerowe `first_instance`. Ścieżka zapasowa (draw per chunk)
    /// istnieje i jest testowana — ryzyko R6 fazy mówi wprost, że nie na każdym backendzie
    /// ta funkcja jest dostępna.
    pub multi_draw_indirect: bool,
    /// Czy sterownik potrafi stemplować czas GPU w passach. Bez tego pomiar per pass
    /// nie istnieje — i nie ma go czym zastąpić, bo czas CPU mierzy kolejkowanie poleceń,
    /// a nie ich wykonanie. Raport wydajności (§7.4) mówi wtedy wprost „brak pomiaru".
    pub timestamps: bool,
    /// Okres tyknięcia zegara GPU w nanosekundach.
    pub timestamp_period_ns: f32,
}

impl GpuContext {
    /// Łańcuch inicjalizacji: instancja → adapter → urządzenie → powierzchnia.
    pub fn new(window: Arc<Window>) -> GpuContext {
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

        // Ścieżka zapasowa (rysowanie per chunk) istnieje niezależnie — ryzyko R6 fazy
        // mówi wprost, że nie na każdym backendzie ta funkcja jest dostępna, a ≤ 4000
        // wywołań rysowania mieści się w budżecie CPU.
        // Rysowanie pośrednie wymaga **dwóch** rzeczy naraz: samego multi-draw i prawa
        // do niezerowego `first_instance` — bez tego drugiego każdy chunk musiałby mieć
        // własne wywołanie, czyli dokładnie to, czego multi-draw ma uniknąć.
        // `MAGNAT_DRAW=per-chunk` wymusza ścieżkę zapasową na sprzęcie, który ma multi-draw.
        // Bez tego przełącznika druga ścieżka byłaby testowana wyłącznie tam, gdzie nie ma
        // wyboru — czyli nigdy u nikogo, kto ją psuje.
        let wymuszony_fallback = std::env::var("MAGNAT_DRAW").as_deref() == Ok("per-chunk");
        let multi_draw_indirect = !wymuszony_fallback
            && adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::INDIRECT_EXECUTION)
            && adapter
                .features()
                .contains(wgpu::Features::INDIRECT_FIRST_INSTANCE);
        let timestamps = adapter.features().contains(
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES,
        );
        let mut required_features = wgpu::Features::empty();
        if multi_draw_indirect {
            required_features |= wgpu::Features::INDIRECT_FIRST_INSTANCE;
        }
        if timestamps {
            required_features |=
                wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
        }

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("magnat-render"),
            required_features,
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

        let timestamp_period_ns = queue.get_timestamp_period();

        GpuContext {
            instance,
            surface,
            adapter,
            device,
            queue,
            config,
            multi_draw_indirect,
            timestamps,
            timestamp_period_ns,
        }
    }

    /// Wyłącza synchronizację pionową, jeśli sterownik na to pozwala.
    ///
    /// Do pomiaru, nie do grania: przy `Fifo` każda klatka czeka na monitor, więc raport
    /// pokazywałby 60 FPS niezależnie od tego, czy renderer ma zapas, czy ledwo zdąża.
    pub fn disable_vsync(&mut self) -> bool {
        let tryby = self.surface.get_capabilities(&self.adapter).present_modes;
        let Some(tryb) = [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox]
            .into_iter()
            .find(|m| tryby.contains(m))
        else {
            return false;
        };
        self.config.present_mode = tryb;
        self.surface.configure(&self.device, &self.config);
        true
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    #[must_use]
    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height.max(1) as f32
    }

    /// Nazwa adaptera i backendu — do tytułu okna i do raportu wydajności (§7.4),
    /// który bez tej informacji nie mówi nic o tym, na czym był mierzony.
    #[must_use]
    pub fn adapter_name(&self) -> String {
        let info = self.adapter.get_info();
        format!("{} ({:?})", info.name, info.backend)
    }
}
