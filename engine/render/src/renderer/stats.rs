//! Pomiar klatki: nazwy passów, znaczniki czasu GPU i statystyki (M1 WP-R1, M11e/WP10).
//!
//! Wydzielone z `passes.rs` przy M11e bez zmiany zachowania. Powód jest ten sam, dla
//! którego M11d wydzieliło `report.rs` z `app.rs`: „co klatka rysuje" i „ile to trwało"
//! to dwa tematy, a plik przekroczył próg strukturalny dopiero wtedy, gdy doszedł do
//! niego ósmy pass i budżet klatki (CLAUDE.md, „przegląd strukturalny po zamkniętym
//! pakiecie").

use super::frame::VisibleChunk;
use super::passes::PassOutcome;
use super::Renderer;
use crate::gpu::GpuContext;

/// Nazwy mierzonych passów — indeks odpowiada parze znaczników w `QuerySet`.
pub const PASS_NAMES: [&str; 8] = [
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
    // Ósma pozycja, z tego samego powodu co siódma (`H-4`). Bufor identyfikatorów
    // rysuje pełną geometrię wszystkich encji **drugi raz w każdej klatce** i do M11e
    // był jedynym passem, którego żaden budżet klatki nie widział — zgłoszenie
    // z M4c/WP14. Teraz go widzi, a przy kursorze poza oknem pass się nie otwiera.
    "pick_id",
];

/// Pozycja passa pogody w [`PASS_NAMES`] — jedno miejsce zamiast trzech literałów.
pub(super) const PASS_WEATHER: usize = 6;

/// Pozycja passa bufora identyfikatorów w [`PASS_NAMES`].
pub(super) const PASS_PICK: usize = 7;

/// Pozycja passa wody w [`PASS_NAMES`]. Pass jest **warunkowy** tak samo jak pogoda
/// i bufor identyfikatorów — kadr bez ani jednego chunka z taflą go nie otwiera.
pub(super) const PASS_WATER: usize = 4;

/// Statystyki klatki — wejście do licznika w tytule okna i do raportu z §7.4.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct FrameStats {
    pub chunks_resident: usize,
    pub chunks_drawn: usize,
    pub triangles: usize,
    /// Ile encji dynamicznych poszło w buforze instancji tej klatki (M11b).
    pub instances: usize,
    /// Ile wywołań rysowania je obsłużyło — liczba, której pilnuje kryterium WP2.
    pub instance_batches: usize,
    /// Ile encji poszło na każdym poziomie detalu (§5.5). Indeks = poziom.
    pub instances_by_lod: [u32; 4],
    /// Wszystkie wywołania rysowania klatki, ze wszystkich passów — liczba z budżetów
    /// scen odniesienia (§5.5 M11b: ≤ 900 / 1 500 / 600).
    ///
    /// Liczona tam, gdzie wywołania powstają, a nie odtwarzana z listy chunków: ścieżka
    /// pośrednia rysuje cały kadr **jednym** wywołaniem, a ścieżka na chunk — tyloma,
    /// ile chunków. Druga kopia tej gałęzi rozjechałaby się z pierwszą.
    pub draw_calls: u32,
    /// Ile cząstek pogody i dymu poszło w tej klatce (`H-7`, próg 0,8 ms z WP7).
    pub weather_particles: u32,
    /// Czas procesora na przygotowanie i nagranie klatki, w milisekundach.
    ///
    /// Czas ściany, nie czas GPU: mierzy dokładnie to, co budżet CPU z §5.5 („≤ 4,0 ms
    /// na przygotowanie klatki, na wątku innym niż sim"). `Instant::now()` wolno tu użyć,
    /// bo to render — zakaz z 00 §3.5 dotyczy kodu symulacji.
    pub cpu_ms: f32,
    pub vertex_bytes: usize,
    pub index_bytes: usize,
    pub arena_capacity_bytes: usize,
    /// Czas GPU każdego passa w milisekundach, w kolejności [`PASS_NAMES`].
    /// Zera, gdy sterownik nie ma `TIMESTAMP_QUERY` — patrz [`FrameStats::gpu_timing`].
    pub pass_ms: [f32; PASS_NAMES.len()],
    /// Czy `pass_ms` niesie pomiar, czy tylko zera.
    pub gpu_timing: bool,
    /// Czy `pass_ms` jest **świeży**, czy kopią z poprzedniej klatki.
    ///
    /// Znaczniki czasu wychodzą co drugą klatkę, a `pass_ms` odbudowuje się co klatkę
    /// z ostatniego udanego odczytu — więc klatka bez odczytu niesie poprawne liczby,
    /// tylko nie swoje. Próbka pomiarowa ma brać wyłącznie klatki świeże, inaczej
    /// każdy pomiar liczy się dwa razy, a raport zawyża liczbę próbek dwukrotnie.
    pub gpu_fresh: bool,
}

/// Pomiar czasu GPU per pass (WP-R1: „mierzy czas każdego passu").
///
/// Czas CPU nie zastępuje tego pomiaru nawet w przybliżeniu: mierzy nagrywanie poleceń,
/// a GPU wykonuje je klatkę później. Stąd znaczniki czasu po stronie GPU i odczyt
/// **z opóźnieniem** — mapowanie bufora jest gotowe dopiero, gdy karta skończy klatkę.
pub(super) struct PassTimer {
    pub(super) set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    period_ns: f32,
    /// Mapowanie w toku — nie wolno go zlecić drugi raz ani pisać wtedy do bufora.
    w_locie: std::sync::Arc<std::sync::atomic::AtomicBool>,
    gotowe: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ms: [f32; PASS_NAMES.len()],
    /// Które passy odbyły się w klatce, której dotyczy **trwający** odczyt.
    ///
    /// Maska musi jechać razem z pomiarem, a nie obok niego: odczyt znaczników wraca
    /// o klatkę później, więc nałożenie na niego flag klatki bieżącej zerowałoby czas
    /// passa, który się odbył (kursor wyszedł poza okno między klatkami), albo
    /// przepuszczało wartość passa, którego nie było. Do pętli budżetu wchodziłaby
    /// wtedy liczba niezwiązana z żadną klatką.
    maska: [bool; PASS_NAMES.len()],
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
            maska: [true; PASS_NAMES.len()],
        })
    }

    /// Znaczniki dla passa o zadanym indeksie.
    pub(super) fn writes(&self, pass: usize) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(2 * pass as u32),
            end_of_pass_write_index: Some(2 * pass as u32 + 1),
        }
    }

    /// Znaczniki obejmujące **blok** passów: początek stempluje pierwszy, koniec ostatni.
    /// Cztery kaskady cieni to cztery passy, ale jeden budżet czasu (§4, WP-R3: ≤ 2 ms).
    pub(super) fn writes_block(
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
    ///
    /// Zwraca `true`, gdy `ms` niesie **świeży** pomiar. Klatka bez odczytu zostawia
    /// w polu wartość poprzednią — poprawną jako obraz, ale nie jako nową próbkę:
    /// wpuszczenie jej do okna budżetu policzyłoby jedną klatkę dwa razy.
    fn zbierz(&mut self, gpu: &GpuContext) -> bool {
        use std::sync::atomic::Ordering;
        if self.gotowe.swap(false, Ordering::Acquire) {
            {
                let Ok(dane) = self.readback.slice(..).get_mapped_range() else {
                    // Wyjście **bez** odmapowania zostawiłoby bufor zajęty na zawsze:
                    // `resolve_timer` pomijałby wtedy każdą następną klatkę, a licznik
                    // umarłby po cichu do końca sesji. Od M11e kosztowałoby to nie tylko
                    // raport, ale i pętlę budżetu, która z tych czasów steruje detalem.
                    self.readback.unmap();
                    self.w_locie
                        .store(false, std::sync::atomic::Ordering::Release);
                    return false;
                };
                let mut czasy = [0u64; 2 * PASS_NAMES.len()];
                for (i, c) in czasy.iter_mut().enumerate() {
                    *c = u64::from_le_bytes(dane[i * 8..i * 8 + 8].try_into().unwrap());
                }
                for (i, ms) in self.ms.iter_mut().enumerate() {
                    // Pass pominięty nie stemplował swojej pary znaczników, więc
                    // w buforze leży wartość z klatki, w której się odbył. Zero znaczy
                    // „nie było passa"; liczba z przeszłości znaczyłaby „był i kosztował".
                    if !self.maska[i] {
                        *ms = 0.0;
                        continue;
                    }
                    // Licznik GPU potrafi się cofnąć między passami przy przełączeniu
                    // kontekstu — ujemna różnica to nie pomiar, tylko szum.
                    let d = czasy[2 * i + 1].saturating_sub(czasy[2 * i]);
                    *ms = d as f32 * self.period_ns / 1.0e6;
                }
            }
            self.readback.unmap();
            self.w_locie.store(false, Ordering::Release);
            return true;
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
        false
    }
}

impl Renderer {
    pub(super) fn resolve_timer(&mut self, encoder: &mut wgpu::CommandEncoder, wynik: PassOutcome) {
        let Some(t) = self.timer.as_mut() else {
            return;
        };
        // Bufor odczytu jest zamapowany, dopóki nie odbierzemy poprzedniego wyniku;
        // zapis do zamapowanego bufora to błąd walidacji, więc klatkę pomijamy.
        // Dopóki mapowanie trwa (albo czeka na odbiór), bufor jest zajęty i zapis do niego
        // jest błędem walidacji, nie ostrzeżeniem.
        if t.w_locie.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        // Maska idzie razem z tym odczytem: opisuje **tę** klatkę, a nie tę, w której
        // odczyt wróci.
        t.maska = [true; PASS_NAMES.len()];
        t.maska[PASS_WATER] = wynik.woda;
        t.maska[PASS_WEATHER] = wynik.pogoda;
        t.maska[PASS_PICK] = wynik.pick;
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
        wynik: PassOutcome,
        cpu: std::time::Duration,
    ) {
        let swiezy = if let Some(mut t) = self.timer.take() {
            let s = t.zbierz(&self.gpu);
            self.timer = Some(t);
            s
        } else {
            false
        };
        self.zbierz_occupancy();
        self.stats = FrameStats {
            chunks_resident: self.chunks.len(),
            chunks_drawn: widoczne.len(),
            triangles: wynik.triangles,
            instances: self.instances.drawn() as usize,
            instance_batches: self.instances.batches(),
            instances_by_lod: self.instances.instances_by_lod(),
            draw_calls: wynik.draw_calls,
            weather_particles: self.weather_fx.particles(),
            cpu_ms: cpu.as_secs_f32() * 1000.0,
            vertex_bytes: self.vertex_arena.bytes_used(8),
            index_bytes: self.index_arena.bytes_used(4),
            arena_capacity_bytes: self.vertex_arena.capacity_bytes(8)
                + self.index_arena.capacity_bytes(4),
            // Maskę passów pominiętych nakłada `PassTimer::zbierz`, bo tylko tam wiadomo,
            // **której klatki** dotyczy odczyt.
            pass_ms: self
                .timer
                .as_ref()
                .map_or([0.0; PASS_NAMES.len()], |t| t.ms),
            gpu_timing: self.timer.is_some(),
            gpu_fresh: swiezy,
        };
        // Pętla sprzężenia zwrotnego domyka się tutaj, a nie u klienta: każdy konsument
        // renderu ma dostać tę samą adaptację, a jedyne wejście — czas GPU — jest tu.
        if swiezy {
            self.budget.observe(self.stats.pass_ms.iter().sum());
        }
    }
}
