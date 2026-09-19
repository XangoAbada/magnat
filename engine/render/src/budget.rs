//! Budżet klatki i statystyki renderu (M11e §5.10, WP10).
//!
//! Dwie rzeczy, jedna pętla sprzężenia zwrotnego. [`RenderBudget`] patrzy na czas GPU
//! ostatnich ośmiu klatek i skaluje progi odległości poziomu detalu; [`RenderStats`]
//! jest tym, co z klatki zostaje na papierze — dla `engine/devtools`, dla raportu
//! benchmarku i dla M12, który z tych liczb zaczyna profilowanie.
//!
//! **Reguła wiążąca (decyzja 9.11 dokumentu fazy, `I-3`):** budżet zmienia wyłącznie
//! odległości progowe po stronie GPU. Nie dotyka `ViewQuery` ani `SnapshotCaps` —
//! obniżanie capu snapshotu pod presją wydajności byłoby sprzężeniem render → symulacja,
//! czyli dokładnie tym, czemu zapobiega wydzielenie `sim-snapshot`. Cap jest stałą
//! konfiguracji, nie zmienną runtime'u.

use crate::renderer::FrameStats;

/// Ile klatek wchodzi do okna decyzyjnego.
const OKNO: usize = 8;

/// Ile klatek z zapasem musi minąć, zanim progi wrócą w górę.
const KLATEK_NA_PODWYZKE: u32 = 60;

/// Ile klatek odpoczynku po każdej zmianie skali.
///
/// Bez karencji pętla oscyluje: obniżenie progów zdejmuje encje, czas spada poniżej
/// 80 % celu, skala rośnie z powrotem i pop poziomu detalu widać jako pulsowanie.
const KARENCJA: u32 = 30;

/// Dolny limit skali progów — poniżej tego widok dzielnicy przestaje być widokiem dzielnicy.
pub const LOD_SCALE_MIN: f32 = 0.5;

/// Górny limit: progi z §5.5 są wartością nominalną, a nie punktem wyjścia w górę.
pub const LOD_SCALE_MAX: f32 = 1.0;

/// Cel czasu klatki dla widoku dzielnicy i ulicy (60 FPS, PRD §20.2).
pub const TARGET_60_MS: f32 = 16.6;

/// Cel czasu klatki dla widoku miasta (30 FPS, PRD §20.2).
pub const TARGET_30_MS: f32 = 33.3;

/// Adaptacyjna skala progów poziomu detalu (§5.10).
///
/// Wejściem jest czas **GPU**, nie czas ściany: czas ściany niesie też koszt symulacji
/// i wsyncu, a z nich żaden nie zmieni się od zdjęcia detalu z encji na trzystu metrach.
#[derive(Clone, Debug)]
pub struct RenderBudget {
    /// 16,6 albo 33,3 ms wg trybu kamery — ustawia klient, bo to on wie, na co patrzy.
    pub target_ms: f32,
    lod_scale: f32,
    historia: [f32; OKNO],
    probek: usize,
    zapis: usize,
    karencja: u32,
    z_zapasem: u32,
}

impl Default for RenderBudget {
    fn default() -> Self {
        RenderBudget::new(TARGET_60_MS)
    }
}

impl RenderBudget {
    #[must_use]
    pub fn new(target_ms: f32) -> RenderBudget {
        RenderBudget {
            target_ms,
            lod_scale: LOD_SCALE_MAX,
            historia: [0.0; OKNO],
            probek: 0,
            zapis: 0,
            karencja: 0,
            z_zapasem: 0,
        }
    }

    /// Skala progów, którą ma wziąć najbliższa klatka.
    #[must_use]
    pub fn lod_scale(&self) -> f32 {
        self.lod_scale
    }

    /// Ile klatek pomiaru ma już w oknie. Poniżej [`OKNO`] budżet **nie decyduje**:
    /// pierwsze klatki po starcie i po zmianie kadru niosą koszt materializacji chunków,
    /// a nie koszt rysowania.
    #[must_use]
    pub fn probek(&self) -> usize {
        self.probek.min(OKNO)
    }

    /// p95 okna: przy ośmiu próbkach to **najgorsza klatka**, i to jest zamierzone.
    ///
    /// Zdjęcie detalu ma być reakcją na zacięcie, a nie na trend — ośmioklatkowa
    /// mediana przepuściłaby jedno szarpnięcie na cztery. `0.0` znaczy „brak pomiaru".
    #[must_use]
    pub fn p95_ms(&self) -> f32 {
        let n = self.probek();
        if n == 0 {
            return 0.0;
        }
        let mut v: Vec<f32> = self.historia[..n].to_vec();
        v.sort_by(f32::total_cmp);
        // Zaokrąglenie w górę — ta sama konwencja co w raporcie przelotu (`bench::Pomiar`).
        let i = (((n - 1) as f32 * 0.95).ceil() as usize).min(n - 1);
        v[i]
    }

    /// Dopisuje czas GPU klatki i zwraca skalę progów dla następnej.
    ///
    /// Reguła z §5.10: p95 z ośmiu klatek powyżej celu → skala w dół o 10 % (podłoga 0,5);
    /// p95 poniżej 80 % celu przez 60 klatek → w górę o 5 % (sufit 1,0). Między zmianami
    /// 30 klatek karencji.
    pub fn observe(&mut self, gpu_ms: f32) -> f32 {
        // Klatka bez znaczników czasu (sterownik bez `TIMESTAMP_QUERY`, albo klatka,
        // w której bufor odczytu był zajęty) nie jest pomiarem zera — jest brakiem
        // pomiaru. Wpuszczenie jej do okna obniżyłoby p95 o połowę i budżet podnosiłby
        // progi dokładnie wtedy, gdy nie ma z czego.
        // NaN wymieniony wprost, a nie zaprzeczeniem porównania: znacznik czasu
        // z cofniętego licznika GPU potrafi dać wartość spoza liczb, a taka próbka
        // zatrułaby sortowanie percentyla na resztę sesji.
        if gpu_ms.is_nan() || gpu_ms <= 0.0 {
            return self.lod_scale;
        }
        self.historia[self.zapis] = gpu_ms;
        self.zapis = (self.zapis + 1) % OKNO;
        self.probek += 1;

        if self.probek() < OKNO {
            return self.lod_scale;
        }
        // Odliczanie **przed** decyzją, nie po: karencja 30 ma blokować trzydzieści
        // klatek, a odjęcie po sprawdzeniu blokowałoby dwadzieścia dziewięć.
        if self.karencja > 0 {
            self.karencja -= 1;
            return self.lod_scale;
        }

        let p95 = self.p95_ms();
        if p95 > self.target_ms {
            self.z_zapasem = 0;
            let nowa = (self.lod_scale * 0.9).max(LOD_SCALE_MIN);
            if nowa < self.lod_scale {
                self.lod_scale = nowa;
                self.karencja = KARENCJA;
            }
            return self.lod_scale;
        }

        if p95 < self.target_ms * 0.8 {
            self.z_zapasem += 1;
            if self.z_zapasem >= KLATEK_NA_PODWYZKE {
                self.z_zapasem = 0;
                let nowa = (self.lod_scale * 1.05).min(LOD_SCALE_MAX);
                if nowa > self.lod_scale {
                    self.lod_scale = nowa;
                    self.karencja = KARENCJA;
                }
            }
        } else {
            self.z_zapasem = 0;
        }
        self.lod_scale
    }
}

/// Statystyki klatki eksportowane na zewnątrz renderu (§5.10).
///
/// **Nie jest kopią [`FrameStats`], tylko obwódką na niej** — i to jest korekta §5.10,
/// nie skrót. Plan wypisywał `RenderStats` jako osobną strukturę z własnymi `gpu_ms`,
/// `draw_calls` i `triangles`, a te liczby renderer już liczy i wpisuje do `FrameStats`
/// (`H-2`). Dwie struktury o wspólnych polach rozjeżdżają się przy pierwszej zmianie,
/// więc wspólne pola są tu **jedne**, a doklejone są wyłącznie te, których renderer
/// z definicji nie widzi: mikser dźwięku, strumieniowanie chunków klienta i koszt
/// selekcji kadru po stronie symulacji.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderStats {
    /// Wszystko, co renderer zmierzył sam: czasy passów, wywołania, trójkąty, instancje.
    pub frame: FrameStats,
    /// Skala progów detalu, którą ta klatka dostała od [`RenderBudget`].
    pub lod_scale: f32,
    /// Głosy czynne miksera. `engine/audio` nie widzi renderu (§6.3 pkt 1), więc liczbę
    /// wpisuje klient — jedyna strona widząca oba budżety naraz.
    pub voices_active: u8,
    /// Ile chunków przemeshowano w tej klatce. Licznik strumieniowania klienta (`I-14`):
    /// to jest cały dowód na „pory roku nie dotykają geometrii".
    pub chunk_remesh_count: u32,
    /// Koszt złożenia danych klatki po stronie klienta **w całości**: wypełnienie
    /// snapshotu (warstwa Mikro, wybór top-K, tłum sceny pomiarowej), przekrój i szyldy.
    pub snapshot_select_ms: f32,
    /// Z tego — sam koszt odpytania indeksu budynków: `CsrGrid::query_rect` plus
    /// odrzucenia i sortowanie po `Building.aabb`.
    ///
    /// Osobno od [`RenderStats::snapshot_select_ms`], bo to jest **zobowiązanie wobec
    /// M2** (próg alarmowy 0,3 ms), a tamta liczba niesie też wypełnienie snapshotu,
    /// czyli koszt, który z `GridSpec` nie ma nic wspólnego. Ścieżka milczy bez aktywnego
    /// cięcia poziomami, więc miarodajne są sceny `bench_interiors` i `bench_street`.
    pub building_query_ms: f32,
    /// Kafle impostorów dzielnic zregenerowane w tej klatce (WP4b).
    pub impostor_regen: u8,
    /// Kafle impostorów dzielnic rezydentne w atlasie (WP4b).
    pub impostor_resident: u16,
    /// Ile świateł punktowych klatka podała do przypisania klastrów.
    ///
    /// Bez tej liczby kryterium „`bench_blackout` ≤ `bench_night_rain`" nie ma jak
    /// pokazać, że blackout w ogóle zaszedł: obie sceny mają ten sam kadr i tę samą
    /// pogodę, a różnią się wyłącznie długością listy świateł.
    pub lights: u32,
}

impl RenderStats {
    /// Statystyki klatki bez liczb spoza renderu — te dokłada klient polami publicznymi.
    #[must_use]
    pub fn from_frame(frame: FrameStats, lod_scale: f32) -> RenderStats {
        RenderStats {
            frame,
            lod_scale,
            ..RenderStats::default()
        }
    }

    /// Suma czasów passów tej klatki. Zero, gdy sterownik nie ma `TIMESTAMP_QUERY`.
    #[must_use]
    pub fn gpu_ms(&self) -> f32 {
        self.frame.pass_ms.iter().sum()
    }

    /// Czas procesora na przygotowanie i nagranie klatki.
    #[must_use]
    pub fn cpu_ms(&self) -> f32 {
        self.frame.cpu_ms
    }

    #[must_use]
    pub fn draw_calls(&self) -> u32 {
        self.frame.draw_calls
    }

    #[must_use]
    pub fn triangles(&self) -> u64 {
        self.frame.triangles as u64
    }

    /// Ile encji poszło na każdym poziomie detalu (§5.5).
    #[must_use]
    pub fn instances_by_lod(&self) -> [u32; 4] {
        self.frame.instances_by_lod
    }

    /// Zapisuje statystyki do licznika `engine/devtools` (DoD §7.5 pkt 6).
    ///
    /// Tędy, a nie typem w `devtools`: `engine/render` **zależy** od `engine/devtools`
    /// (`ClusterOccupancy`, zrzut PNG), więc zależność w drugą stronę zamknęłaby cykl,
    /// którego Cargo nie zbuduje — ta sama reguła, która trzyma słowniki w `engine/core`
    /// (`K-8`). `MetricSink` jest przy tym kontraktem, którego M12 potrzebuje naprawdę:
    /// eksportuje CSV, więc profilowanie długiej sesji zaczyna się od gotowego szeregu,
    /// a nie od pisania drugiego licznika.
    ///
    /// Czasy idą w **mikrosekundach**, bo seria jest całkowitoliczbowa; nazwy są
    /// statyczne, bo `MetricSink` kluczuje po `&'static str`.
    pub fn record_into(&self, sink: &mut magnat_devtools::MetricSink, tick: magnat_core::Tick) {
        // Zaokrąglenie, nie obcięcie: `0.9f32` to w `f64` 0,899999976, więc obcięcie
        // dawało 899 promili zamiast 900 — a przy czasach passów gubiłoby do jednej
        // mikrosekundy na każdym wpisie, zawsze w tę samą stronę.
        let us = |ms: f32| (f64::from(ms) * 1000.0).round() as i64;
        sink.record(tick, "render.gpu_us", us(self.gpu_ms()));
        sink.record(tick, "render.cpu_us", us(self.cpu_ms()));
        sink.record(tick, "render.select_us", us(self.snapshot_select_ms));
        sink.record(tick, "render.building_query_us", us(self.building_query_ms));
        sink.record(tick, "render.draw_calls", i64::from(self.draw_calls()));
        sink.record(tick, "render.triangles", self.triangles() as i64);
        sink.record(tick, "render.instances", self.frame.instances as i64);
        sink.record(tick, "render.chunks_drawn", self.frame.chunks_drawn as i64);
        sink.record(
            tick,
            "render.weather_particles",
            i64::from(self.frame.weather_particles),
        );
        sink.record(
            tick,
            "render.chunk_remesh",
            i64::from(self.chunk_remesh_count),
        );
        sink.record(tick, "render.voices", i64::from(self.voices_active));
        sink.record(
            tick,
            "render.impostor_resident",
            i64::from(self.impostor_resident),
        );
        // Skala progów w promilach: seria jest całkowita, a 0,9 i 0,95 muszą się różnić.
        // `us` mnoży przez tysiąc, czyli zamienia ułamek wprost na promile — dzielenie
        // przez tysiąc, które tu stało, sprowadzało całą serię do zer i jedynek.
        sink.record(tick, "render.lod_scale_permille", us(self.lod_scale));
        for (i, ms) in self.frame.pass_ms.iter().enumerate() {
            sink.record(tick, PASS_METRICS[i], us(*ms));
        }
    }

    /// Jedna linijka do konsoli `engine/devtools` i do raportu.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "GPU {:.2} ms, CPU {:.2} ms, {} wywołań, {} trójkątów, {} encji (L0 {} / L1 {} / L2 {}),              skala detalu {:.2}, selekcja {:.3} ms (indeks budynków {:.3} ms), głosów {}",
            self.gpu_ms(),
            self.cpu_ms(),
            self.draw_calls(),
            self.triangles(),
            self.frame.instances,
            self.instances_by_lod()[0],
            self.instances_by_lod()[1],
            self.instances_by_lod()[2],
            self.lod_scale,
            self.snapshot_select_ms,
            self.building_query_ms,
            self.voices_active,
        )
    }
}

/// Nazwy serii metryk passów — `MetricSink` kluczuje po `&'static str`, a `PASS_NAMES`
/// niesie same nazwy passów. Test niżej pilnuje, żeby obie tablice miały tę samą długość
/// i tę samą kolejność: rozjazd dałby czas jednego passa pod nazwą drugiego.
const PASS_METRICS: [&str; crate::renderer::PASS_NAMES.len()] = [
    "render.pass.clusters_us",
    "render.pass.shadows_us",
    "render.pass.depth_prepass_us",
    "render.pass.opaque_us",
    "render.pass.water_us",
    "render.pass.post_us",
    "render.pass.weather_us",
    "render.pass.pick_id_us",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn nasyc(b: &mut RenderBudget, ms: f32, klatek: usize) {
        for _ in 0..klatek {
            b.observe(ms);
        }
    }

    #[test]
    fn budzet_nie_decyduje_przed_pelnym_oknem() {
        let mut b = RenderBudget::new(16.6);
        // Siedem klatek po 40 ms to jeszcze nie jest okno — materializacja chunków po
        // starcie wygląda dokładnie tak i nie ma z niej wnioskować budżet.
        nasyc(&mut b, 40.0, OKNO - 1);
        assert!((b.lod_scale() - 1.0).abs() < 1e-6);
        b.observe(40.0);
        assert!(b.lod_scale() < 1.0, "ósma klatka domyka okno");
    }

    #[test]
    fn przekroczony_cel_obniza_skale_do_podlogi_i_nie_nizej() {
        let mut b = RenderBudget::new(16.6);
        // Każda obniżka kosztuje 30 klatek karencji, więc do podłogi trzeba ich sporo.
        nasyc(&mut b, 40.0, 8 + 30 * 20);
        assert!(
            (b.lod_scale() - LOD_SCALE_MIN).abs() < 1e-6,
            "{}",
            b.lod_scale()
        );
    }

    #[test]
    fn zapas_podnosi_skale_dopiero_po_szescdziesieciu_klatkach() {
        let mut b = RenderBudget::new(16.6);
        nasyc(&mut b, 40.0, OKNO); // jedna obniżka: 1,0 → 0,9
        let po_obnizce = b.lod_scale();
        assert!(po_obnizce < 1.0);
        // Karencja 30 klatek, potem 60 klatek z zapasem — wcześniej nic się nie rusza.
        nasyc(&mut b, 5.0, 30 + 59);
        assert!((b.lod_scale() - po_obnizce).abs() < 1e-6, "za wcześnie");
        b.observe(5.0);
        assert!(b.lod_scale() > po_obnizce, "sześćdziesiąta klatka podnosi");
    }

    #[test]
    fn klatka_bez_pomiaru_nie_wchodzi_do_okna() {
        let mut b = RenderBudget::new(16.6);
        // Czas GPU wychodzi co drugą klatkę (bufor odczytu zajęty), więc zer jest
        // połowa. Gdyby wchodziły do okna, p95 spadłby i budżet podnosiłby progi.
        nasyc(&mut b, 0.0, 1000);
        assert_eq!(b.probek(), 0);
        assert!((b.lod_scale() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn p95_osmiu_probek_to_najgorsza_klatka() {
        let mut b = RenderBudget::new(1000.0);
        for ms in [8.0, 9.0, 8.5, 9.5, 8.2, 40.0, 8.1, 9.1] {
            b.observe(ms);
        }
        assert!((b.p95_ms() - 40.0).abs() < 1e-6, "{}", b.p95_ms());
    }

    #[test]
    fn nazwy_serii_ida_w_kolejnosci_passow() {
        // Kolejność `pass_ms` jest pozycyjna (`I-12`), więc nazwa serii musi iść tą samą
        // pozycją. Rozjazd zapisałby czas jednego passa pod nazwą drugiego, a raport
        // wyglądałby poprawnie.
        for (i, n) in PASS_METRICS.iter().enumerate() {
            assert!(
                n.contains(crate::renderer::PASS_NAMES[i]),
                "{i}: {n} vs {}",
                crate::renderer::PASS_NAMES[i]
            );
        }
    }

    #[test]
    fn metryki_trafiaja_do_licznika_devtools() {
        let mut sink = magnat_devtools::MetricSink::new(8);
        let mut f = FrameStats {
            draw_calls: 7,
            triangles: 1234,
            ..FrameStats::default()
        };
        f.pass_ms[3] = 2.5;
        let s = RenderStats::from_frame(f, 1.0);
        s.record_into(&mut sink, magnat_core::Tick(1));
        assert_eq!(sink.series("render.draw_calls"), vec![7]);
        assert_eq!(sink.series("render.pass.opaque_us"), vec![2500]);
        assert_eq!(sink.series("render.lod_scale_permille"), vec![1000]);
        // Dwie sąsiednie skale muszą dawać dwie różne liczby — inaczej seria,
        // po której M12 miało czytać adaptację detalu, jest ciągiem zer.
        let mut s2 = magnat_devtools::MetricSink::new(8);
        RenderStats::from_frame(FrameStats::default(), 0.9)
            .record_into(&mut s2, magnat_core::Tick(1));
        let mut s3 = magnat_devtools::MetricSink::new(8);
        RenderStats::from_frame(FrameStats::default(), 0.95)
            .record_into(&mut s3, magnat_core::Tick(1));
        assert_eq!(s2.series("render.lod_scale_permille"), vec![900]);
        assert_eq!(s3.series("render.lod_scale_permille"), vec![950]);
    }

    #[test]
    fn statystyki_nie_powielaja_pol_klatki() {
        let mut f = FrameStats {
            draw_calls: 7,
            triangles: 1234,
            ..FrameStats::default()
        };
        f.pass_ms[0] = 1.5;
        f.pass_ms[3] = 2.5;
        let s = RenderStats::from_frame(f, 0.9);
        assert!((s.gpu_ms() - 4.0).abs() < 1e-6);
        assert_eq!(s.draw_calls(), 7);
        assert_eq!(s.triangles(), 1234);
    }
}
