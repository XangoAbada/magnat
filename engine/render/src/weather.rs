//! Opad, mgła i dym z kominów (M11d §5.8, WP7).
//!
//! ### Dlaczego cząstki nie mają bufora stanu
//!
//! Deszcz jest **funkcją czystą od numeru cząstki i czasu**, a nie symulacją. Pozycja
//! bierze się z rozsiania numeru instancji po pudle wokół kamery i z opadania modulo
//! wysokość pudła; wiatr przesuwa tor, nie stan. Nie ma więc czego przechowywać między
//! klatkami, nie ma kroku compute, nie ma podwójnego bufora — jest jedno wywołanie
//! rysowania na 200 tys. instancji i zero bajtów zapisu.
//!
//! To samo z dymem, z jedną różnicą: kominy są prawdziwe, więc ich lista jedzie
//! w małym buforze (≤ 256 wpisów), a 64 cząstki na komin wyprowadzają się z numeru
//! instancji tak samo jak krople.
//!
//! ### Czego tu nie ma
//!
//! Śniegu na ziemi, kałuż i pory roku. To nie są cząstki, tylko **przesunięcie palety
//! materiału w shaderze terenu** — dlatego siedzą w `voxel.wgsl` i w uniformie ramki,
//! a nie tutaj. Remeshing całego miasta cztery razy w roku gry za zmianę koloru liści
//! byłby kosztem bez pokrycia (§5.8).

use magnat_sim_snapshot::{RenderSnapshot, SiteRenderRec, WeatherState};

/// Bok pudła cząstek opadu wokół kamery w metrach. Nie symulujemy pogody nad całym
/// miastem — tylko tam, gdzie kamera, a reszta i tak jest za mgłą.
pub const PRECIP_BOX_XY_M: f32 = 60.0;
pub const PRECIP_BOX_Z_M: f32 = 40.0;

/// Ile cząstek opadu przy pełnym natężeniu. Budżet §5.8: ≤ 0,8 ms GPU.
pub const MAX_PRECIP: u32 = 200_000;

/// Ile kominów naraz dymi i po ile cząstek każdy (§5.8: cap 256 emiterów, 64 cząstki).
pub const MAX_PLUMES: usize = 256;
pub const PARTICLES_PER_PLUME: u32 = 64;

/// Komin w postaci, w jakiej trafia na GPU: pozycja **względem kamery**, jak cała
/// reszta geometrii renderera.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPlume {
    /// xyz = pozycja względem kamery, w = gęstość 0..1.
    pos_density: [f32; 4],
    /// xyz = barwa pióropusza, w = prędkość wznoszenia w m/s.
    tint_rise: [f32; 4],
}

/// Parametry pogody dla shadera cząstek.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct WeatherUniform {
    /// x = natężenie opadu 0..1, y = 0 deszcz / 1 śnieg, z = czas renderu w s,
    /// w = **wyrównanie**, nie dana.
    ///
    /// Czwarta składowa jest wymuszona przez wyrównanie `vec4` w uniformie i nie ma
    /// czytelnika. Stała pokrywa śnieżna, która tu początkowo siedziała, nie miała go
    /// również — śnieg na ziemi idzie **uniformem ramki** do shaderów terenu, a cząstki
    /// opadu nic o nim nie wiedzą (`I-24`). Pole ustawiane i nieczytane wygląda w kodzie
    /// tak samo jak działające, więc lepiej, żeby jawnie było zerem.
    params: [f32; 4],
    /// xy = wiatr w m/s, z = bok pudła w poziomie, w = światło dzienne 0..1.
    wind: [f32; 4],
}

/// Barwa pióropusza wg `PlumeKind` (`sim/supply`): brak, para, sadza, opary.
///
/// Tablica jest tutaj, a nie w danych, bo to są cztery liczby opisujące **materiał**,
/// a nie kalibrację: para jest biała, sadza czarna i nie ma tego jak przestawić bez
/// zmiany znaczenia (`K-35` — w `data/tuning/` siedzi to, co wolno przestawić).
const PLUME_TINT: [[f32; 3]; 4] = [
    [0.70, 0.70, 0.72],
    [0.95, 0.96, 0.98],
    [0.18, 0.17, 0.16],
    [0.72, 0.78, 0.55],
];

/// Prędkość wznoszenia pióropusza w m/s — para unosi się szybciej niż sadza.
const PLUME_RISE: [f32; 4] = [2.0, 4.5, 2.4, 1.8];

pub struct WeatherRenderer {
    precip: wgpu::RenderPipeline,
    smoke: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    cfg: wgpu::Buffer,
    plumes: wgpu::Buffer,
    /// Ile cząstek opadu rysować w tej klatce. Zero przy suchej pogodzie — pass jest
    /// wtedy pomijany w całości, a nie rysowany z zerową przezroczystością.
    precip_count: u32,
    plume_count: u32,
    /// Robocze bufory kominów — trzymane tu, bo klatka nie alokuje. Dwa, bo wybór
    /// kominów sortuje **indeksy**, a na GPU idą gotowe rekordy.
    scratch: Vec<GpuPlume>,
    wybor: Vec<u32>,
}

impl WeatherRenderer {
    pub fn new(
        device: &wgpu::Device,
        frame_buffer: &wgpu::Buffer,
        depth_format: wgpu::TextureFormat,
    ) -> WeatherRenderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("render.weather"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/weather.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render.weather.layout"),
            entries: &[
                uniform(0),
                uniform(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let cfg = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render.weather.cfg"),
            size: std::mem::size_of::<WeatherUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let plumes = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render.weather.plumes"),
            size: (MAX_PLUMES * std::mem::size_of::<GpuPlume>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render.weather.bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cfg.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: plumes.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render.weather.pipeline.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let precip = potok(device, &shader, &pipeline_layout, "vs_precip", depth_format);
        let smoke = potok(device, &shader, &pipeline_layout, "vs_smoke", depth_format);
        WeatherRenderer {
            precip,
            smoke,
            bind,
            cfg,
            plumes,
            precip_count: 0,
            plume_count: 0,
            scratch: Vec::with_capacity(MAX_PLUMES),
            wybor: Vec::with_capacity(MAX_PLUMES),
        }
    }

    /// Ustawia pogodę i kominy tej klatki ze snapshotu.
    ///
    /// `eye` przychodzi w `f64`, a odejmowanie idzie **przed** rzutowaniem na `f32` —
    /// ta sama reguła co w `instancing` (`R11`, `U-4`): przy współrzędnej 16 km rzutowanie
    /// przed odjęciem gubi milimetry, a komin stojący w miejscu zaczyna drgać.
    pub fn set(
        &mut self,
        snap: &RenderSnapshot,
        eye: glam::DVec3,
        czas_s: f32,
        queue: &wgpu::Queue,
    ) {
        let w = snap.weather;
        self.precip_count = (u32::from(w.precipitation) * MAX_PRECIP / 255).min(MAX_PRECIP);
        let u = WeatherUniform {
            params: [
                f32::from(w.precipitation) / 255.0,
                f32::from(u8::from(w.kind != 0)),
                czas_s,
                0.0,
            ],
            wind: [
                f32::from(w.wind[0]),
                f32::from(w.wind[1]),
                PRECIP_BOX_XY_M,
                f32::from(w.daylight) / 255.0,
            ],
        };
        queue.write_buffer(&self.cfg, 0, bytemuck::bytes_of(&u));

        self.scratch.clear();
        wybierz_kominy(snap.sites.as_slice(), &mut self.wybor);
        for &i in &self.wybor {
            let s = &snap.sites.as_slice()[i as usize];
            let p = [
                (f64::from(s.pos[0]) * 0.001 - eye.x) as f32,
                (f64::from(s.pos[1]) * 0.001 - eye.y) as f32,
                (f64::from(s.pos[2]) * 0.001 - eye.z) as f32,
            ];
            let k = s.plume_kind() as usize;
            self.scratch.push(GpuPlume {
                // Komin jest nad dachem, a `pos` zakładu to wejście — bez podniesienia
                // dym szedłby z drzwi. Wysokość jest przybliżona i taka zostanie:
                // geometrii komina nikt nie modeluje, a pióropusz i tak się rozmywa.
                pos_density: [p[0], p[1], p[2] + 12.0, f32::from(plume_density(s)) / 255.0],
                tint_rise: [
                    PLUME_TINT[k][0],
                    PLUME_TINT[k][1],
                    PLUME_TINT[k][2],
                    PLUME_RISE[k],
                ],
            });
        }
        self.plume_count = self.scratch.len() as u32;
        if self.plume_count > 0 {
            queue.write_buffer(&self.plumes, 0, bytemuck::cast_slice(&self.scratch));
        }
    }

    /// Czy w tej klatce jest cokolwiek do narysowania. Sucha bezwietrzna scena pomija
    /// pass w całości — pusty pass kosztuje tyle, co przełączenie celu renderowania,
    /// a `bench_blackout` ma nie być wolniejszy od `bench_night_rain`.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.precip_count == 0 && self.plume_count == 0
    }

    /// Ile cząstek poszło w tej klatce — do `FrameStats` i do progu z kryterium WP7.
    #[must_use]
    pub const fn particles(&self) -> u32 {
        self.precip_count + self.plume_count * PARTICLES_PER_PLUME
    }

    #[must_use]
    pub const fn plumes(&self) -> u32 {
        self.plume_count
    }

    /// Dwa wywołania rysowania: całość opadu i całość dymu (§5.8 — „1 draw call").
    /// Zwraca, ile ich faktycznie poszło — scena bez dymu ma jedno, bez opadu też.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> u32 {
        pass.set_bind_group(0, Some(&self.bind), &[]);
        let mut n = 0;
        if self.precip_count > 0 {
            pass.set_pipeline(&self.precip);
            pass.draw(0..6, 0..self.precip_count);
            n += 1;
        }
        if self.plume_count > 0 {
            pass.set_pipeline(&self.smoke);
            pass.draw(0..6, 0..self.plume_count * PARTICLES_PER_PLUME);
            n += 1;
        }
        n
    }
}

/// Gęstość pióropusza zakładu, 0..=255 — i **nie jest** to samo `emission`.
///
/// `emission` jest tempem emisji **pyłu** w skali bezwzględnej (`K-35`), a z komina
/// nie zawsze leci pył: chłodnia kominowa i mleczarnia mają `PlumeKind::Steam`
/// i zerowy `pm_g`, bo para nie jest zanieczyszczeniem. Gdyby widoczność pióropusza
/// zależała od samego pyłu, połowa kominów w mieście byłaby niewidoczna mimo pracy.
///
/// Reguła: **rodzaj** pióropusza mówi, co leci, `activity` — czy cokolwiek leci,
/// a `emission` — jak gęsto. Dolna granica przy pracującej linii jest ćwiartką
/// obłożenia, więc para widać, a sadza i tak wygrywa własnym tempem.
#[must_use]
pub fn plume_density(s: &SiteRenderRec) -> u8 {
    if s.plume_kind() == 0 || s.activity == 0 {
        return 0;
    }
    s.emission.max(s.activity / 4)
}

/// Kominy tej klatki, do capu [`MAX_PLUMES`].
///
/// Wybór idzie **po sile dymu**, a nie po odległości: zakłady w snapshocie są już
/// wybrane po odległości od oka (`select_top_k` po stronie wypełniacza), więc drugie
/// sito na tej samej wielkości nic by nie odsiało. Rafineria na skraju kadru ma dymić,
/// a piekarnia pod nosem nie musi.
fn wybierz_kominy(sites: &[SiteRenderRec], out: &mut Vec<u32>) {
    out.clear();
    for (i, s) in sites.iter().enumerate() {
        if plume_density(s) > 0 {
            out.push(i as u32);
        }
    }
    if out.len() > MAX_PLUMES {
        // Malejąco po gęstości; remis po `entity_lo`, żeby lista nie migotała między
        // klatkami przy zakładach o równym dymie.
        out.sort_unstable_by_key(|&i| {
            let s = &sites[i as usize];
            (std::cmp::Reverse(plume_density(s)), s.entity_lo)
        });
        out.truncate(MAX_PLUMES);
    }
}

fn uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Potok cząstek: mieszanie alfa, głębia **czytana, nie zapisywana**.
///
/// Zapis głębi przez kroplę deszczu zasłoniłby wszystko, co jest za nią, a cząstek jest
/// dwieście tysięcy — obraz zamieniłby się w szary koc. Test głębi zostaje, bo kropla
/// za budynkiem ma być niewidoczna.
fn potok(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    vs: &str,
    depth_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("render.weather"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vs),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: crate::renderer::HDR_FORMAT,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::COLOR,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Parametry pogody wchodzące do **uniformu ramki**, czyli do shaderów terenu.
///
/// Śnieg na ziemi, wilgoć i pora roku nie są cząstkami — są przesunięciem palety
/// materiału. Ta funkcja jest jedynym miejscem, które zamienia rekord snapshotu na
/// cztery liczby, których oczekuje `voxel.wgsl`; bez niej ta konwencja rozłaziłaby się
/// po `write_frame_uniform` i po shaderach.
#[must_use]
pub fn terrain_params(w: &WeatherState) -> [f32; 4] {
    [
        f32::from(w.snow_cover) / 255.0,
        // Wilgoć: pada teraz albo dopiero co przestało. Modelu kałuż nie ma i nie będzie
        // — maska wilgotności to jedna liczba, bo różnica między mokrym a suchym asfaltem
        // jest w połysku, a nie w kształcie plam.
        f32::from(w.precipitation) / 255.0,
        f32::from(w.season),
        f32::from(w.cloud) / 255.0,
    ]
}

/// Gęstość mgły dla shaderów: bazowa zamglenie horyzontu plus wkład pogody.
///
/// Baza `0,000 06` jest liczbą M1 i zostaje: przy niej teren w promieniu kilometra
/// jest czysty, a pierścień LOD3 na 4 km wyraźnie zamglony. Pogoda dokłada do niej
/// tyle, żeby przy `fog_density == 255` widoczność spadła do ok. 200 m — dalej mgła
/// przestaje być pogodą, a staje się awarią renderu.
#[must_use]
pub fn fog_density(w: &WeatherState) -> f32 {
    const BAZA: f32 = 0.000_06;
    const MAKS: f32 = 0.005;
    BAZA + (f32::from(w.fog_density) / 255.0) * MAKS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cap 256 kominów obowiązuje, a wybór jest **powtarzalny**: lista, która migocze
    /// między klatkami, wygląda jak usterka symulacji.
    #[test]
    fn kominy_sa_przyciete_i_stabilne() {
        let sites: Vec<SiteRenderRec> = (0..400u32)
            .map(|i| SiteRenderRec {
                entity_lo: i,
                emission: (i % 255) as u8 + 1,
                activity: 200,
                flags: 1 << 1,
                ..Default::default()
            })
            .collect();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        wybierz_kominy(&sites, &mut a);
        wybierz_kominy(&sites, &mut b);
        assert_eq!(a.len(), MAX_PLUMES);
        assert_eq!(a, b, "wybór kominów nie jest powtarzalny");
    }

    /// Trzy reguły pióropusza naraz: rodzaj mówi, co leci, `activity` — czy cokolwiek
    /// leci, `emission` — jak gęsto. Zakład bez rodzaju nie dymi mimo pyłu, zatrzymana
    /// linia nie dymi mimo rodzaju, a chłodnia kominowa (para, zero pyłu) dymi.
    #[test]
    fn pioropusz_bierze_sie_z_rodzaju_i_pracy_linii() {
        let pylacy_bez_rodzaju = SiteRenderRec {
            emission: 200,
            activity: 255,
            flags: 0,
            ..Default::default()
        };
        let stojacy = SiteRenderRec {
            emission: 200,
            activity: 0,
            flags: 2 << 1,
            ..Default::default()
        };
        let chlodnia = SiteRenderRec {
            emission: 0,
            activity: 200,
            flags: 1 << 1,
            ..Default::default()
        };
        let huta = SiteRenderRec {
            emission: 240,
            activity: 200,
            flags: 2 << 1,
            ..Default::default()
        };
        assert_eq!(plume_density(&pylacy_bez_rodzaju), 0);
        assert_eq!(plume_density(&stojacy), 0, "stojąca linia dymi");
        assert_eq!(plume_density(&chlodnia), 50, "para bez pyłu nie widać");
        assert_eq!(plume_density(&huta), 240);
        let sites = [pylacy_bez_rodzaju, stojacy, chlodnia, huta];
        let mut wybrane = Vec::new();
        wybierz_kominy(&sites, &mut wybrane);
        assert_eq!(wybrane.len(), 2);
    }

    /// Mgła rośnie z pogodą i nigdy nie schodzi poniżej bazy M1.
    #[test]
    fn mgla_dokłada_sie_do_bazy() {
        let czysto = WeatherState::default();
        let gesto = WeatherState {
            fog_density: 255,
            ..Default::default()
        };
        assert!(fog_density(&czysto) > 0.0);
        assert!(fog_density(&gesto) > fog_density(&czysto) * 10.0);
        // Przy pełnej mgle widoczność (1/e) ma być rzędu 200 m, a nie 20 ani 2000.
        let widocznosc = 1.0 / fog_density(&gesto);
        assert!(
            (150.0..350.0).contains(&widocznosc),
            "widoczność {widocznosc} m"
        );
    }
}
