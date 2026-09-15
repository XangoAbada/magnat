//! Warstwa LOD Mikro — pozycje widoczne w kadrze (M4d §5.4, WP8; wcześniej `sim/agents`).
//!
//! **Wizualizator bez prawa zapisu** (00 §4): nie dotyka potrzeb, kolejki zdarzeń ani
//! czasu przybycia. Postęp encji wynika **wyłącznie** z pary `(wjazd, wyjazd)` policzonej
//! przez warstwę wyższą — mikro interpoluje między wyliczonym startem a wyliczonym
//! przybyciem i nie ma jak ich zmienić. Dlatego test spójności LOD przechodzi
//! z tolerancją 0 **z konstrukcji**, a nie przez kalibrację: pieszy i pojazd docierają
//! dokładnie w swojej minucie niezależnie od tego, czy krok mikro w ogóle się wykonał
//! i ile razy.
//!
//! Okno jest stanem prezentacji, a nie symulacji: może zależeć od tego, gdzie stoi
//! kamera, i hash stanu jest ten sam. Promień 0 (stan domyślny) wyłącza warstwę —
//! headless nie ma kadru, a setki tysięcy polilinii w pamięci to koszt, którego nikt
//! by nie oglądał.
//!
//! ## Co robi car-following, skoro nie wyznacza czasu
//!
//! IDM i MOBIL rozstrzygają **gdzie w danej chwili stoi bryła**, a nie **kiedy dojedzie**.
//! Kolejka przed światłem powstaje, bo pierwszy pojazd ma linię wyjazdową jako
//! nieruchomego poprzednika do chwili `exit_cs`, a reszta hamuje za nim — nie dlatego,
//! że ktoś liczy sygnalizację drugi raz. Serwo z §5.4 przycina pożądaną prędkość tak,
//! żeby dystans zszedł do zera w zaksięgowanej chwili; wielkość tej korekty jest
//! **metryką jakości kalibracji, nie poprawności** (miernik dryfu B1–B4 w WP9).
//!
//! Konsekwencja `R-1`: warstwa jest krokowana **raz na minutę świata** (WP14), a IDM
//! nie jest czystą funkcją czasu, więc całkowanie odbywa się **wewnątrz** wywołania,
//! w podkrokach z twardym limitem z `data/roads/idm.ron`. Liczba wywołań systemu
//! zostaje jedna — tego pilnuje bramka WP14.
//!
//! Arytmetyka trasy jest całkowitoliczbowa w centymetrach; float pojawia się w fizyce
//! ruchu i w pozycji wyjściowej, czyli po stronie prezentacji (00 §2). **Żadna z tych
//! liczb nie dotyka ledgera** — granicą jest sygnatura `settle_edge`, która przyjmuje
//! wyłącznie typy całkowite.
//!
//! `ponytail:` pasażer nie ma własnego rekordu w buforze — jedzie w bryle kursu.
//! Sufit nazwany: gracz nie zobaczy, ile osób siedzi w autobusie, dopóki M11 nie doda
//! wnętrz. Ścieżka wyjścia: `VehicleRecord.occupancy` albo osobna tablica pasażerów,
//! obie po stronie `sim/snapshot`, gdy wnętrza powstaną.

use crate::spec::DataError;
use magnat_core::{data_path, WorldCoord};
use magnat_sim_snapshot::{PedestrianRecord, VehicleRecord};
use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::Mutex;

/// Setne sekundy w minucie — ta sama jednostka, w której liczy przejazd warstwa mezo.
const CS_PER_MINUTE: u64 = 6_000;
const NO_SLOT: u32 = u32::MAX;
/// Krawędź, na której pojazd z nikim nie oddziałuje (kurs komunikacji odgrywany
/// z rozkładu, nie z kolejki krawędzi — `P-11`).
pub const NO_EDGE: u32 = u32::MAX;

/// Odległość w linii prostej w centymetrach. `isqrt` zamiast `f64::sqrt`, bo długość
/// trasy rozstrzyga, gdzie encja stoi przy danym postępie — a liczba całkowita nie ma
/// wariantów zaokrąglenia, o które można się spierać przy porównaniu mikro z mezo.
#[inline]
#[must_use]
fn euclid_cm(a: WorldCoord, b: WorldCoord) -> u32 {
    (a.distance_sq_xy(b).max(0) as u64)
        .isqrt()
        .min(u64::from(u32::MAX)) as u32
}

// ── arena polilinii ─────────────────────────────────────────────────────────────

/// Trasy, po których poruszają się encje w kadrze. Jedna arena dla pieszych i pojazdów,
/// bo to ta sama wiedza: łamana plus jej długość.
#[derive(Debug, Default)]
struct PathArena {
    points: Vec<WorldCoord>,
    /// `(offset w points, liczba punktów, długość w cm)`.
    paths: Vec<(u32, u32, u32)>,
}

impl PathArena {
    fn push(&mut self, route: &[WorldCoord]) -> u32 {
        debug_assert!(route.len() >= 2, "trasa bez dwóch końców");
        let offset = self.points.len() as u32;
        self.points.extend_from_slice(route);
        let dlugosc: u32 = route
            .windows(2)
            .map(|w| euclid_cm(w[0], w[1]))
            .fold(0u32, u32::saturating_add);
        self.paths.push((offset, route.len() as u32, dlugosc.max(1)));
        (self.paths.len() - 1) as u32
    }

    #[inline]
    fn len_cm(&self, path: u32) -> u32 {
        self.paths[path as usize].2
    }

    fn clear(&mut self) {
        self.points.clear();
        self.paths.clear();
    }

    /// Punkt na łamanej w ułamku `t` jej długości plus kurs odcinka, na którym leży.
    fn at(&self, path: u32, t: f32) -> ([f32; 3], f32) {
        let (offset, len, dlugosc) = self.paths[path as usize];
        let punkty = &self.points[offset as usize..(offset + len) as usize];
        let cel = t.clamp(0.0, 1.0) * dlugosc as f32;
        let mut przebyte = 0.0f32;
        for w in punkty.windows(2) {
            let d = euclid_cm(w[0], w[1]) as f32;
            if przebyte + d >= cel {
                let u = if d == 0.0 { 0.0 } else { (cel - przebyte) / d };
                let dx = (w[1].x - w[0].x) as f32;
                let dy = (w[1].y - w[0].y) as f32;
                return (
                    [
                        (w[0].x as f32 + dx * u) / 100.0,
                        (w[0].y as f32 + dy * u) / 100.0,
                        (w[0].z as f32 + (w[1].z - w[0].z) as f32 * u) / 100.0,
                    ],
                    dy.atan2(dx),
                );
            }
            przebyte += d;
        }
        let ostatni = punkty[punkty.len() - 1];
        let przedostatni = punkty[punkty.len() - 2];
        (
            [
                ostatni.x as f32 / 100.0,
                ostatni.y as f32 / 100.0,
                ostatni.z as f32 / 100.0,
            ],
            ((ostatni.y - przedostatni.y) as f32).atan2((ostatni.x - przedostatni.x) as f32),
        )
    }
}

// ── piesi ───────────────────────────────────────────────────────────────────────

/// Jeden pieszy w kadrze — 32 B. Bez unikania kolizji i bez steeringu: przy voxelu 1 m
/// tłum czyta się dobrze bez tego, a steering należy do M11 razem z animacjami.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Pedestrian {
    /// Pozycja w metrach — to jest warstwa prezentacji, więc float jest tu na miejscu.
    pub pos: [f32; 3],
    /// Postęp 0..=1 wzdłuż trasy.
    pub progress: f32,
    /// Indeks trasy w arenie polilinii.
    pub path: u32,
    /// Indeks encji mieszkańca.
    pub citizen: u32,
    pub depart_min: u16,
    pub arrive_min: u16,
    /// 0 = od początku trasy do końca. Zostawione dla tras dwukierunkowych.
    pub dir: u8,
    pub _pad: [u8; 3],
}

/// Pozycje pieszych w LOD Mikro: encje plus arena polilinii, po których się poruszają.
#[derive(Debug, Default)]
pub struct PedestrianBuffer {
    peds: Vec<Pedestrian>,
    paths: PathArena,
}

impl PedestrianBuffer {
    #[must_use]
    pub fn new() -> PedestrianBuffer {
        PedestrianBuffer::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.peds.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peds.is_empty()
    }

    #[must_use]
    pub fn get(&self, i: usize) -> Pedestrian {
        self.peds[i]
    }

    /// Dokłada pieszego na trasie `route` (polilinia w centymetrach). Zwraca jego indeks.
    pub fn spawn(
        &mut self,
        citizen: u32,
        route: &[WorldCoord],
        depart_min: u16,
        arrive_min: u16,
    ) -> usize {
        let path = self.paths.push(route);
        let pierwszy = route[0];
        self.peds.push(Pedestrian {
            pos: [
                pierwszy.x as f32 / 100.0,
                pierwszy.y as f32 / 100.0,
                pierwszy.z as f32 / 100.0,
            ],
            progress: 0.0,
            path,
            citizen,
            depart_min,
            arrive_min,
            dir: 0,
            _pad: [0; 3],
        });
        self.peds.len() - 1
    }

    /// Krok mikro. `now_ms` to milisekunda doby.
    ///
    /// Postęp wynika **wyłącznie** z pary `(depart, arrive)` policzonej przez mezo —
    /// dlatego pieszy dociera dokładnie w swojej minucie niezależnie od tego, czy krok
    /// mikro w ogóle się wykonał i ile razy. To jest konstrukcja, nie kalibracja (00 §4).
    pub fn step(&mut self, now_ms: u64) {
        // Minuta **doby**, bo `depart_min`/`arrive_min` są minutami doby (u16).
        // Bez reszty z dzielenia druga doba zastawałaby wszystkich w celu: `teraz`
        // rosłoby dalej od startu świata, a `arrive_min` wracało do zera o północy.
        let teraz = (now_ms % 86_400_000) as f32 / 60_000.0;
        for p in &mut self.peds {
            let start = f32::from(p.depart_min);
            let koniec = f32::from(p.arrive_min);
            let t = if koniec <= start {
                1.0
            } else {
                ((teraz - start) / (koniec - start)).clamp(0.0, 1.0)
            };
            p.progress = t;
            p.pos = self.paths.at(p.path, t).0;
        }
    }

    /// Usuwa pieszych, którzy dotarli. Arena punktów nie jest kompaktowana w locie:
    /// zbiór encji Mikro jest rzędu tysięcy (kadr kamery), więc zwolnienie jej dopiero
    /// wtedy, gdy opustoszeje, kosztuje mniej niż przepisywanie offsetów co minutę.
    pub fn retire(&mut self, now_min: u16) {
        self.peds.retain(|p| p.arrive_min > now_min);
        if self.peds.is_empty() {
            self.paths.clear();
        }
    }
}

// ── parametry IDM/MOBIL ─────────────────────────────────────────────────────────

/// Parametry car-followingu i zmiany pasa z `data/roads/idm.ron`.
///
/// **To są parametry animacji.** Nie wchodzą do żadnego wzoru pieniężnego ani czasowego;
/// ich jedyne sprzężenie z ekonomią jest offline, przez `balansator calibrate-vdf`.
#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
pub struct IdmParams {
    pub schema_version: u32,
    pub s0_cm: u32,
    pub headway_ds: u32,
    pub accel_cms2: u32,
    pub decel_cms2: u32,
    pub delta: u32,
    pub politeness_permille: u32,
    pub threshold_cms2: u32,
    pub safe_decel_cms2: u32,
    pub servo_min_permille: u32,
    pub servo_max_permille: u32,
    pub substep_ms: u32,
    pub max_substeps: u32,
}

pub const IDM_SCHEMA_VERSION: u32 = 1;

impl Default for IdmParams {
    /// Wartości z pliku danych powielone w kodzie **wyłącznie** dla testów jednostkowych,
    /// które nie mają katalogu `data/`. Ścieżka produkcyjna zawsze ładuje plik.
    fn default() -> IdmParams {
        IdmParams {
            schema_version: IDM_SCHEMA_VERSION,
            s0_cm: 120,
            headway_ds: 5,
            accel_cms2: 130,
            decel_cms2: 200,
            delta: 4,
            politeness_permille: 250,
            threshold_cms2: 20,
            safe_decel_cms2: 400,
            servo_min_permille: 850,
            servo_max_permille: 1150,
            substep_ms: 500,
            max_substeps: 120,
        }
    }
}

impl IdmParams {
    pub fn load_default() -> Result<IdmParams, DataError> {
        IdmParams::load(&data_path("roads/idm.ron"))
    }

    pub fn load(path: &Path) -> Result<IdmParams, DataError> {
        let txt = std::fs::read_to_string(path)?;
        let p: IdmParams = ron::from_str(&txt).map_err(|e| DataError::Ron(e.to_string()))?;
        if p.schema_version != IDM_SCHEMA_VERSION {
            return Err(DataError::Schema {
                found: p.schema_version,
                want: IDM_SCHEMA_VERSION,
            });
        }
        // `delta` inne niż 4 byłoby cichym kłamstwem: kod liczy człon prędkości jako
        // kwadrat kwadratu, bo `powf` jest zakazane w kodzie symulacji (`K-6`).
        if p.delta != 4 {
            return Err(DataError::Empty("roads/idm.ron: delta musi być 4"));
        }
        if p.accel_cms2 == 0 || p.decel_cms2 == 0 || p.max_substeps == 0 {
            return Err(DataError::Empty("roads/idm.ron: parametr zerowy"));
        }
        Ok(p)
    }
}

// ── pojazdy ─────────────────────────────────────────────────────────────────────

/// Zgłoszenie pojazdu do kadru. Wszystkie czasy w setnych sekundy, jak cała arytmetyka
/// przejazdu (`L-5` w M4b).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct VehicleFeed {
    /// Indeks encji pojazdu — to samo, co czyta bufor ID przy kliknięciu.
    pub vehicle: u32,
    /// Krawędź, na której pojazd stoi w kolejce; `NO_EDGE` = nie oddziałuje z nikim.
    pub edge: u32,
    pub class: u8,
    pub lanes: u8,
    pub len_cm: u16,
    /// Prędkość swobodna krawędzi w cm/s — sufit pożądanej prędkości.
    pub v_free_cms: f32,
    pub entry_cs: u64,
    pub exit_cs: u64,
    /// `false` = bryła odgrywa czas bez car-followingu (kurs komunikacji).
    pub car_following: bool,
}

/// Jeden pojazd w kadrze.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MicroVehicle {
    pub vehicle: u32,
    pub edge: u32,
    pub path: u32,
    pub entry_cs: u64,
    pub exit_cs: u64,
    /// Odległość od początku trasy w centymetrach.
    pub pos_cm: f32,
    pub speed_cms: f32,
    pub v_free_cms: f32,
    pub len_cm: u16,
    pub class: u8,
    pub lane: u8,
    pub lanes: u8,
    pub car_following: bool,
    pub pos: [f32; 3],
    pub heading: f32,
}

/// Pojazdy w kadrze plus arena ich tras.
///
/// Arena jest **przebudowywana przy każdym zasileniu**, a nie rozbudowywana: pojazd
/// zmienia krawędź co kilkadziesiąt sekund, więc arena narastająca urosłaby o dwa punkty
/// na pojazd na minutę i po dobie ważyłaby dziesiątki megabajtów. Pozycja i prędkość
/// przeżywają przebudowę, bo są kluczowane indeksem encji, nie miejscem w tablicy.
#[derive(Debug, Default)]
pub struct VehicleBuffer {
    vehs: Vec<MicroVehicle>,
    paths: PathArena,
    /// Zasilanie w toku — zamieniane z `vehs`/`paths` w `end_feed`.
    staged: Vec<MicroVehicle>,
    next: PathArena,
    /// Indeks encji pojazdu → slot w `vehs`; `NO_SLOT` = pojazdu nie ma w kadrze.
    slot_of: Vec<u32>,
    order: Vec<u32>,
    acc: Vec<f32>,
    /// Jednostki rysowania zgłoszone w bieżącym zasilaniu — licznik, nie suma po
    /// buforze: sumowanie przy każdym zgłoszeniu byłoby kwadratem liczby pojazdów.
    units: u32,
    clock_cs: u64,
}

impl VehicleBuffer {
    #[must_use]
    pub fn new() -> VehicleBuffer {
        VehicleBuffer::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.vehs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vehs.is_empty()
    }

    #[must_use]
    pub fn get(&self, i: usize) -> MicroVehicle {
        self.vehs[i]
    }

    /// Początek zasilania: nowa arena, pusta lista wstawianych.
    pub fn begin_feed(&mut self) {
        self.staged.clear();
        self.next.clear();
        self.units = 0;
    }

    /// Czy bryła o tej długości jeszcze mieści się w suficie rysowania.
    fn fits(&self, len_cm: u16, cap: u32) -> bool {
        self.units + waga(len_cm) <= cap
    }

    /// Zgłasza pojazd do kadru. Pojazd, który został na tej samej krawędzi, zachowuje
    /// pozycję, prędkość i pas — inaczej co minutę przeskakiwałby na początek odcinka.
    pub fn feed(&mut self, f: VehicleFeed, route: &[WorldCoord]) {
        if route.len() < 2 {
            return;
        }
        let path = self.next.push(route);
        let dlugosc = self.next.len_cm(path);
        let mut v = MicroVehicle {
            vehicle: f.vehicle,
            edge: f.edge,
            path,
            entry_cs: f.entry_cs,
            exit_cs: f.exit_cs.max(f.entry_cs + 1),
            pos_cm: 0.0,
            speed_cms: f.v_free_cms.max(1.0),
            v_free_cms: f.v_free_cms.max(1.0),
            len_cm: f.len_cm.max(100),
            class: f.class,
            lane: 0,
            lanes: f.lanes.max(1),
            car_following: f.car_following,
            pos: [0.0; 3],
            heading: 0.0,
        };
        if let Some(&slot) = self.slot_of.get(f.vehicle as usize) {
            if slot != NO_SLOT {
                let p = self.vehs[slot as usize];
                if p.edge == f.edge && p.entry_cs == f.entry_cs {
                    v.pos_cm = p.pos_cm.min(dlugosc as f32);
                    v.speed_cms = p.speed_cms;
                    v.lane = p.lane.min(v.lanes - 1);
                }
            }
        }
        self.units += waga(v.len_cm);
        self.staged.push(v);
    }

    /// Koniec zasilania: pojazdy niezgłoszone w tej minucie znikają z kadru.
    pub fn end_feed(&mut self) {
        std::mem::swap(&mut self.vehs, &mut self.staged);
        std::mem::swap(&mut self.paths, &mut self.next);
        // Kolejność jest kluczowana indeksem encji, a nie miejscem w tablicy — dlatego
        // mapa slotów odbudowuje się w całości, a nie łata przyrostowo.
        for s in &mut self.slot_of {
            *s = NO_SLOT;
        }
        let max = self.vehs.iter().map(|v| v.vehicle).max().unwrap_or(0) as usize;
        if self.slot_of.len() <= max {
            self.slot_of.resize(max + 1, NO_SLOT);
        }
        for (i, v) in self.vehs.iter().enumerate() {
            self.slot_of[v.vehicle as usize] = i as u32;
        }
        self.staged.clear();
        self.next.clear();
    }

    /// Krok mikro do chwili `now_cs` (setne sekundy, ta sama oś co `exit_cs` podróży).
    ///
    /// Całkowanie dzieje się **wewnątrz** wywołania, bo warstwa jest krokowana raz na
    /// minutę świata (`R-1`), a car-following nie jest czystą funkcją czasu. Liczba
    /// podkroków ma twardy limit z danych; powyżej niego krok jest rozciągany, a pozycja
    /// i tak domyka się do czasu zaksięgowanego.
    pub fn step(&mut self, now_cs: u64, p: &IdmParams) {
        if self.vehs.is_empty() {
            self.clock_cs = now_cs;
            return;
        }
        if now_cs <= self.clock_cs {
            // Cofnięcie zegara (nowa doba, wczytanie zapisu) — bez całkowania w tył.
            self.clock_cs = now_cs;
            self.place(now_cs);
            return;
        }
        let laczny = now_cs - self.clock_cs;
        let podkrok_cs = u64::from(p.substep_ms / 10).max(1);
        let krokow = laczny.div_ceil(podkrok_cs).clamp(1, u64::from(p.max_substeps));
        let start = self.clock_cs;
        let mut poprzedni = start;
        for k in 1..=krokow {
            let t = start + laczny * k / krokow;
            let dt = (t - poprzedni) as f32 / 100.0;
            self.substep(t, dt, p);
            poprzedni = t;
        }
        self.clock_cs = now_cs;
        self.place(now_cs);
    }

    /// Jeden podkrok całkowania — cztery fazy z §5.9 pkt 2.
    fn substep(&mut self, now_cs: u64, dt: f32, p: &IdmParams) {
        // A: porządek i przyspieszenia, na zamrożonym buforze pozycji.
        self.sortuj();
        let n = self.vehs.len();
        self.acc.clear();
        self.acc.resize(n, 0.0);
        for k in 0..n {
            let i = self.order[k] as usize;
            if !self.vehs[i].car_following {
                continue;
            }
            let lider = self.lider(k);
            self.acc[i] = self.przyspieszenie(i, lider, now_cs, p);
        }

        // B: zgłoszenia zmiany pasa, rozstrzygane w porządku totalnym
        // `(krawędź, pas docelowy, odległość, indeks encji)` — bez pytania o zegar,
        // wątek ani kolejność ukończenia jobów (§5.9 pkt 3).
        self.zmien_pasy(now_cs, p);

        // C: całkowanie. `dt` jest wspólne dla wszystkich, więc kolejność nie ma znaczenia.
        for i in 0..n {
            if !self.vehs[i].car_following {
                continue;
            }
            let dlugosc = self.paths.len_cm(self.vehs[i].path) as f32;
            let v = &mut self.vehs[i];
            let sufit = v.v_free_cms * 1.2;
            v.speed_cms = (v.speed_cms + self.acc[i] * dt).clamp(0.0, sufit);
            v.pos_cm = (v.pos_cm + v.speed_cms * dt).clamp(0.0, dlugosc);
        }
        // D: zamiana buforów jest tu tożsamościowa — `acc` jest scratchem podkroku,
        // a pozycje zmieniają się dopiero w fazie C, po policzeniu wszystkich sił.
    }

    /// Porządek totalny: krawędź, pas, odległość od początku, indeks encji.
    fn sortuj(&mut self) {
        self.order.clear();
        self.order.extend(0..self.vehs.len() as u32);
        let v = &self.vehs;
        self.order.sort_unstable_by(|a, b| {
            let (x, y) = (&v[*a as usize], &v[*b as usize]);
            x.edge
                .cmp(&y.edge)
                .then(x.lane.cmp(&y.lane))
                .then(x.pos_cm.total_cmp(&y.pos_cm))
                .then(x.vehicle.cmp(&y.vehicle))
        });
    }

    /// Poprzednik w porządku z `sortuj`: następny pojazd na tej samej krawędzi i pasie.
    fn lider(&self, k: usize) -> Option<usize> {
        let i = self.order[k] as usize;
        let me = &self.vehs[i];
        if me.edge == NO_EDGE {
            return None;
        }
        let j = *self.order.get(k + 1)?;
        let inny = &self.vehs[j as usize];
        (inny.edge == me.edge && inny.lane == me.lane).then_some(j as usize)
    }

    /// IDM. Bez poprzednika rolę przeszkody gra **linia wyjazdowa**: pojazd, który nie
    /// dostał jeszcze swojej minuty wyjazdu, hamuje do niej i staje — i to jest cały
    /// mechanizm kolejki przed światłem. Sygnalizacji nie liczymy tu drugi raz;
    /// czekanie wynika z `exit_cs`, które policzył `settle_node`.
    fn przyspieszenie(&self, i: usize, lider: Option<usize>, now_cs: u64, p: &IdmParams) -> f32 {
        let me = &self.vehs[i];
        let dlugosc = self.paths.len_cm(me.path) as f32;
        let a = p.accel_cms2 as f32;
        let b = p.decel_cms2 as f32;
        let v = me.speed_cms.max(0.0);

        let (luka, dv) = match lider {
            Some(j) => {
                let l = &self.vehs[j];
                (l.pos_cm - f32::from(l.len_cm) - me.pos_cm, v - l.speed_cms)
            }
            None => {
                // Za linią wyjazdową jest przeszkoda tylko dopóki pojazd nie ma prawa
                // wyjechać. Po `exit_cs` mezo już go zdjęło — bryła toczy się swobodnie
                // do najbliższego zasilenia i wtedy trafia na następną krawędź.
                if now_cs >= me.exit_cs {
                    (f32::MAX / 4.0, 0.0)
                } else {
                    (dlugosc - me.pos_cm, v)
                }
            }
        };
        let s = luka.max(10.0);

        // Serwo (§5.4): pożądana prędkość to ta, przy której dystans schodzi do zera
        // dokładnie w zaksięgowanej chwili, przycięta do widełek z danych.
        let v0 = self.pozadana(me, now_cs, p);
        let ratio = (v / v0).min(4.0);
        let r2 = ratio * ratio;
        let r4 = r2 * r2; // delta == 4, więc bez `powf` (K-6)
        let s_star = p.s0_cm as f32
            + (v * (p.headway_ds as f32 / 10.0) + v * dv / (2.0 * (a * b).sqrt())).max(0.0);
        let z = s_star / s;
        (a * (1.0 - r4 - z * z)).clamp(-3.0 * b, a)
    }

    /// Pożądana prędkość po korekcie serwa.
    fn pozadana(&self, me: &MicroVehicle, now_cs: u64, p: &IdmParams) -> f32 {
        let dlugosc = self.paths.len_cm(me.path) as f32;
        let zostalo_cm = (dlugosc - me.pos_cm).max(0.0);
        let zostalo_s = (me.exit_cs.saturating_sub(now_cs)) as f32 / 100.0;
        if zostalo_s <= 0.01 || zostalo_cm <= 1.0 {
            return me.v_free_cms;
        }
        let wymagana = zostalo_cm / zostalo_s;
        let lo = p.servo_min_permille as f32 / 1000.0;
        let hi = p.servo_max_permille as f32 / 1000.0;
        let korekta = (wymagana / me.v_free_cms).clamp(lo, hi);
        (me.v_free_cms * korekta).max(1.0)
    }

    /// MOBIL: zmiana pasa, gdy zysk własny przewyższa próg powiększony o uprzejmość
    /// wobec nadjeżdżającego z tyłu, a ten nie musi przy tym hamować ponad `safe_decel`.
    fn zmien_pasy(&mut self, now_cs: u64, p: &IdmParams) {
        let n = self.vehs.len();
        let uprzejmosc = p.politeness_permille as f32 / 1000.0;
        let prog = p.threshold_cms2 as f32;
        let bezpieczne = -(p.safe_decel_cms2 as f32);

        // Zgłoszenia w porządku z `sortuj`, czyli totalnym — remisy są niemożliwe.
        let mut zgloszenia: Vec<(usize, u8, f32)> = Vec::new();
        for k in 0..n {
            let i = self.order[k] as usize;
            let me = self.vehs[i];
            if !me.car_following || me.lanes < 2 || me.edge == NO_EDGE {
                continue;
            }
            let teraz = self.przyspieszenie(i, self.lider(k), now_cs, p);
            for d in [-1i16, 1] {
                let cel = i16::from(me.lane) + d;
                if cel < 0 || cel >= i16::from(me.lanes) {
                    continue;
                }
                let cel = cel as u8;
                let (przed, za) = self.sasiedzi(i, cel);
                let po = self.przyspieszenie_w(i, przed, now_cs, p);
                // Nadjeżdżający z tyłu nie może dostać hamowania ponad próg.
                let strata_za = match za {
                    Some(j) => {
                        let bez = self.przyspieszenie_w(j, self.przed_w(j), now_cs, p);
                        let z = self.przyspieszenie_w(j, Some(i), now_cs, p);
                        if z < bezpieczne {
                            continue;
                        }
                        bez - z
                    }
                    None => 0.0,
                };
                let zysk = po - teraz - uprzejmosc * strata_za;
                if zysk > prog {
                    zgloszenia.push((i, cel, zysk));
                }
            }
        }
        // Przy dwóch zgłoszeniach tego samego pojazdu wygrywa większy zysk, a przy
        // remisie mniejszy numer pasa — klucz pozostaje totalny.
        zgloszenia.sort_unstable_by(|a, b| {
            a.0.cmp(&b.0)
                .then(b.2.total_cmp(&a.2))
                .then(a.1.cmp(&b.1))
        });
        let mut poprzedni = usize::MAX;
        for (i, cel, _) in zgloszenia {
            if i == poprzedni {
                continue;
            }
            poprzedni = i;
            self.vehs[i].lane = cel;
        }
    }

    /// Zakres w `order` zajmowany przez jeden pas jednej krawędzi.
    ///
    /// Szukanie połówkowe, a nie przegląd bufora: `order` jest już posortowane kluczem
    /// `(krawędź, pas, pozycja, encja)`, więc pas jest w nim **ciągłym** przedziałem.
    /// Zmierzone: przegląd liniowy w MOBIL kosztował 459 ms na minutę świata przy
    /// 3 tys. pojazdów na trzech pasach wobec 11 ms na jednym — bo dopiero drugi pas
    /// włącza szukanie sąsiadów, a ono było kwadratowe względem całego bufora.
    fn zakres(&self, edge: u32, lane: u8) -> std::ops::Range<usize> {
        let klucz = |k: &u32| {
            let v = &self.vehs[*k as usize];
            (v.edge, v.lane)
        };
        let od = self.order.partition_point(|k| klucz(k) < (edge, lane));
        let do_ = self.order.partition_point(|k| klucz(k) <= (edge, lane));
        od..do_
    }

    /// Poprzednik i następca pojazdu `i`, gdyby stanął na pasie `lane`.
    fn sasiedzi(&self, i: usize, lane: u8) -> (Option<usize>, Option<usize>) {
        let me = &self.vehs[i];
        let r = self.zakres(me.edge, lane);
        let pierwszy_przed = self.order[r.clone()]
            .partition_point(|k| self.vehs[*k as usize].pos_cm < me.pos_cm);
        let przed = self.order[r.clone()]
            .get(pierwszy_przed)
            .map(|k| *k as usize)
            .filter(|j| *j != i);
        let za = pierwszy_przed
            .checked_sub(1)
            .and_then(|p| self.order[r].get(p))
            .map(|k| *k as usize)
            .filter(|j| *j != i);
        (przed, za)
    }

    /// Poprzednik pojazdu `i` na jego własnym pasie.
    fn przed_w(&self, i: usize) -> Option<usize> {
        let me = &self.vehs[i];
        let r = self.zakres(me.edge, me.lane);
        let p = self.order[r.clone()]
            .partition_point(|k| self.vehs[*k as usize].pos_cm <= me.pos_cm);
        self.order[r].get(p).map(|k| *k as usize)
    }

    fn przyspieszenie_w(&self, i: usize, lider: Option<usize>, now_cs: u64, p: &IdmParams) -> f32 {
        let me = &self.vehs[i];
        let dlugosc = self.paths.len_cm(me.path) as f32;
        let a = p.accel_cms2 as f32;
        let b = p.decel_cms2 as f32;
        let v = me.speed_cms.max(0.0);
        let (luka, dv) = match lider {
            Some(j) => {
                let l = &self.vehs[j];
                (l.pos_cm - f32::from(l.len_cm) - me.pos_cm, v - l.speed_cms)
            }
            None => {
                if now_cs >= me.exit_cs {
                    (f32::MAX / 4.0, 0.0)
                } else {
                    (dlugosc - me.pos_cm, v)
                }
            }
        };
        let s = luka.max(10.0);
        let v0 = self.pozadana(me, now_cs, p);
        let ratio = (v / v0).min(4.0);
        let r2 = ratio * ratio;
        let s_star = p.s0_cm as f32
            + (v * (p.headway_ds as f32 / 10.0) + v * dv / (2.0 * (a * b).sqrt())).max(0.0);
        let z = s_star / s;
        (a * (1.0 - r2 * r2 - z * z)).clamp(-3.0 * b, a)
    }

    /// Przelicza pozycje w metrach. Bryła bez car-followingu (kurs komunikacji) dostaje
    /// pozycję **wyłącznie** z pary `(wjazd, wyjazd)` — dokładnie jak pieszy.
    fn place(&mut self, now_cs: u64) {
        for v in &mut self.vehs {
            let dlugosc = self.paths.len_cm(v.path) as f32;
            let t = if v.car_following {
                (v.pos_cm / dlugosc.max(1.0)).clamp(0.0, 1.0)
            } else {
                let rozpietosc = (v.exit_cs - v.entry_cs) as f32;
                let u = ((now_cs.saturating_sub(v.entry_cs)) as f32 / rozpietosc).clamp(0.0, 1.0);
                v.pos_cm = u * dlugosc;
                u
            };
            let (pos, heading) = self.paths.at(v.path, t);
            v.pos = pos;
            v.heading = heading;
        }
    }

    /// Zmierzony diagram podstawowy: `(gęstość poj./km pasa, przepływ poj./h pasa)`
    /// na krawędzi. Wejście kalibratora `balansator calibrate-vdf` (WP9) — i **jedyne**
    /// miejsce, w którym pomiar z mikro w ogóle opuszcza warstwę.
    #[must_use]
    pub fn measured_flow(&self, edge: u32) -> Option<(f32, f32)> {
        let mut n = 0u32;
        let mut suma_v = 0.0f32;
        let mut path = None;
        let mut pasy = 1u8;
        for v in &self.vehs {
            if v.edge == edge {
                n += 1;
                suma_v += v.speed_cms;
                path = Some(v.path);
                pasy = v.lanes.max(1);
            }
        }
        let path = path?;
        let km = self.paths.len_cm(path) as f32 / 100_000.0;
        if km <= 0.0 {
            return None;
        }
        let gestosc = n as f32 / (km * f32::from(pasy));
        let srednia = suma_v / n as f32; // cm/s
        Some((gestosc, gestosc * srednia * 36.0 / 1000.0))
    }
}

// ── warstwa ─────────────────────────────────────────────────────────────────────

/// Okno LOD Mikro plus bufory encji w kadrze.
///
/// Mutacja idzie przez `Mutex` i atomiki, bo wołający trzyma `&self`: warstwa siedzi
/// w zasobie dzielonym z rendererem, a nie w wyłącznym stanie systemu. To jest wybór
/// właściwy dla wizualizatora — gdyby warstwa cokolwiek liczyła, dostałaby `&mut self`
/// i nie miałaby zamka.
pub struct MicroLayer {
    peds: Mutex<PedestrianBuffer>,
    vehs: Mutex<VehicleBuffer>,
    idm: IdmParams,
    /// Środek okna w metrach; promień 0 = warstwa wyłączona.
    center: (AtomicI32, AtomicI32),
    radius_m: AtomicU32,
    /// Sufit rysowania (§7.3): pojazd ponad limitem zostaje w mezo **bez skutku
    /// ekonomicznego**, bo mezo działa dla każdej krawędzi niezależnie od kadru.
    cap: AtomicU32,
}

/// Domyślny sufit liczby jednostek w kadrze (§7.3). Duża bryła liczy się podwójnie.
pub const MICRO_UNIT_CAP: u32 = 3_000;

impl MicroLayer {
    #[must_use]
    pub fn new() -> MicroLayer {
        MicroLayer::with_params(IdmParams::load_default().unwrap_or_default())
    }

    #[must_use]
    pub fn with_params(idm: IdmParams) -> MicroLayer {
        MicroLayer {
            peds: Mutex::new(PedestrianBuffer::new()),
            vehs: Mutex::new(VehicleBuffer::new()),
            idm,
            center: (AtomicI32::new(0), AtomicI32::new(0)),
            radius_m: AtomicU32::new(0),
            cap: AtomicU32::new(MICRO_UNIT_CAP),
        }
    }

    #[must_use]
    pub fn params(&self) -> IdmParams {
        self.idm
    }

    /// Okno warstwy: środek kadru i promień w metrach. `None` wyłącza ją zupełnie
    /// i jest stanem domyślnym.
    ///
    /// **To nie wpływa na wynik symulacji.** Warstwa Mikro nie wchodzi do funkcji
    /// haszującej stan, więc okno może zależeć od pozycji kamery, a hash i tak jest ten sam.
    pub fn set_window(&self, center: Option<(i32, i32)>, radius_m: u32) {
        match center {
            Some((x, y)) => {
                self.center.0.store(x, Ordering::Relaxed);
                self.center.1.store(y, Ordering::Relaxed);
                self.radius_m.store(radius_m.max(1), Ordering::Relaxed);
            }
            None => self.radius_m.store(0, Ordering::Relaxed),
        }
    }

    #[must_use]
    pub fn enabled(&self) -> bool {
        self.radius_m.load(Ordering::Relaxed) != 0
    }

    pub fn set_cap(&self, units: u32) {
        self.cap.store(units, Ordering::Relaxed);
    }

    /// Czy punkt (w centymetrach, jak `WorldCoord`) mieści się w oknie Mikro.
    #[must_use]
    pub fn contains(&self, p: WorldCoord) -> bool {
        let r = self.radius_m.load(Ordering::Relaxed);
        if r == 0 {
            return false;
        }
        let dx = i64::from(p.x / 100 - self.center.0.load(Ordering::Relaxed));
        let dy = i64::from(p.y / 100 - self.center.1.load(Ordering::Relaxed));
        dx * dx + dy * dy <= i64::from(r) * i64::from(r)
    }

    /// Wpuszcza pieszego do warstwy. `route` to polilinia w centymetrach, a para
    /// `(depart_min, arrive_min)` pochodzi z warstwy wyższej i jest jedynym źródłem
    /// tempa — mikro jej nie poprawia.
    ///
    /// Bramka okna jest **tutaj**, a nie u wołającego: dzięki temu system doby woła to
    /// bezwarunkowo i nie musi wiedzieć, gdzie stoi kamera ani czy w ogóle jest okno.
    pub fn enter(&self, citizen: u32, route: &[WorldCoord], depart_min: u16, arrive_min: u16) {
        if route.len() < 2 {
            return;
        }
        if !self.contains(route[0]) && !self.contains(route[route.len() - 1]) {
            return;
        }
        self.peds
            .lock()
            .expect("micro")
            .spawn(citizen, route, depart_min, arrive_min);
    }

    /// Początek zasilania pojazdów. Zwraca `false`, gdy warstwa jest wyłączona —
    /// wołający nie musi wtedy nic liczyć, a headless nie płaci nic.
    pub fn begin_vehicle_feed(&self) -> bool {
        if !self.enabled() {
            return false;
        }
        self.vehs.lock().expect("micro").begin_feed();
        true
    }

    /// Zgłasza pojazd. Bramka okna jest tutaj, tak samo jak u pieszego; sufit rysowania
    /// też — pojazd ponad limitem po prostu nie wchodzi w kadr i nic przez to nie traci,
    /// bo warstwa mezo liczy go tak czy inaczej (§5.4).
    pub fn feed_vehicle(&self, f: VehicleFeed, route: &[WorldCoord]) {
        if route.len() < 2 || !self.enabled() {
            return;
        }
        if !self.contains(route[0]) && !self.contains(route[route.len() - 1]) {
            return;
        }
        let mut buf = self.vehs.lock().expect("micro");
        if !buf.fits(f.len_cm.max(100), self.cap.load(Ordering::Relaxed)) {
            return;
        }
        buf.feed(f, route);
    }

    pub fn end_vehicle_feed(&self) {
        self.vehs.lock().expect("micro").end_feed();
    }

    /// Krok mikro: `now_ms` to milisekunda doby.
    pub fn step(&self, now_ms: u64) {
        self.peds.lock().expect("micro").step(now_ms);
        self.vehs
            .lock()
            .expect("micro")
            .step(now_ms / 10, &self.idm);
    }

    /// Usuwa encje, które już dotarły. Pojazdy znikają przez niezgłoszenie w zasilaniu,
    /// więc tu chodzi wyłącznie o pieszych.
    pub fn retire(&self, now_min: u16) {
        self.peds.lock().expect("micro").retire(now_min);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.peds.lock().expect("micro").len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peds.lock().expect("micro").is_empty()
    }

    #[must_use]
    pub fn vehicles(&self) -> usize {
        self.vehs.lock().expect("micro").len()
    }

    /// Zrzut pieszych dla renderera — **jedna kopia, prosto do struktury docelowej**.
    pub fn snapshot(&self, out: &mut Vec<PedestrianRecord>) {
        out.clear();
        let buf = self.peds.lock().expect("micro");
        out.reserve(buf.len());
        for i in 0..buf.len() {
            let p = buf.get(i);
            out.push(PedestrianRecord {
                pos: p.pos,
                entity: p.citizen,
            });
        }
    }

    /// Zrzut pojazdów dla renderera — ten sam kanał i ta sama zasada co u pieszych.
    pub fn vehicle_snapshot(&self, out: &mut Vec<VehicleRecord>) {
        out.clear();
        let buf = self.vehs.lock().expect("micro");
        out.reserve(buf.len());
        for i in 0..buf.len() {
            let v = buf.get(i);
            out.push(VehicleRecord {
                pos: v.pos,
                heading: v.heading,
                entity: v.vehicle,
                class: v.class,
                lane: v.lane,
                _pad: [0; 2],
            });
        }
    }

    /// Zmierzony diagram podstawowy krawędzi — wejście kalibratora VDF (WP9).
    #[must_use]
    pub fn measured_flow(&self, edge: u32) -> Option<(f32, f32)> {
        self.vehs.lock().expect("micro").measured_flow(edge)
    }
}

// ── kalibracja: IDM ↔ diagram podstawowy ────────────────────────────────────────

/// Prędkość równowagi IDM przy zadanej luce do poprzednika — **postać zamknięta z §5.4**:
/// `s_e(v) = s0 + v·T / sqrt(1 − (v/v0)^4)`, rozwiązana względem `v`.
///
/// Lewa strona rośnie monotonicznie na `[0, v0)`, więc bisekcja zbiega bez warunków
/// brzegowych i bez `powf` (`K-6`): wykładnik 4 to kwadrat kwadratu, a `sqrt` wolno.
///
/// To jest **rdzeń `calibrate-vdf`** (WP9) i powód, dla którego kalibrator nie potrzebuje
/// jednorodnego pierścienia z §5.4: pierścień służyłby wyłącznie zmierzeniu tej samej
/// równowagi, którą tu widać wprost, a bufor Mikro jest liniowy z konstrukcji — krawędź
/// ma początek i koniec, więc zawijanie pozycji byłoby kodem istniejącym tylko dla testu.
#[must_use]
pub fn equilibrium_speed_cms(gap_cm: f32, v_free_cms: f32, p: &IdmParams) -> f32 {
    let v0 = v_free_cms.max(1.0);
    let s0 = p.s0_cm as f32;
    let t = p.headway_ds as f32 / 10.0;
    if gap_cm <= s0 {
        return 0.0;
    }
    let odstep = |v: f32| {
        let r = (v / v0).min(0.999_9);
        let r2 = r * r;
        s0 + v * t / (1.0 - r2 * r2).max(1e-6).sqrt()
    };
    let (mut lo, mut hi) = (0.0f32, v0 * 0.999);
    if odstep(hi) <= gap_cm {
        return hi;
    }
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if odstep(mid) <= gap_cm {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Prędkość wyprowadzona z IDM dla danego zapełnienia krawędzi, w decykilometrach
/// na godzinę — czyli w jednostce, w której `data/roads/vdf.ron` trzyma swoją tabelę.
///
/// Przy zapełnieniu `d` promili na jeden pojazd przypada `jam_spacing / d` przestrzeni,
/// z czego jego własna długość jest zajęta; reszta to luka do poprzednika.
#[must_use]
pub fn idm_speed_dkmh(
    occupancy_permille: u32,
    jam_spacing_cm: u32,
    vehicle_len_cm: u32,
    free_dkmh: u16,
    p: &IdmParams,
) -> u16 {
    let d = occupancy_permille.clamp(1, 1000) as f32;
    let przestrzen = jam_spacing_cm as f32 * 1000.0 / d;
    let luka = przestrzen - vehicle_len_cm as f32;
    // Decykilometr na godzinę to 100 000 cm / 3 600 s / 10 = 2,778 cm/s.
    let v_free = f32::from(free_dkmh) * 100.0 / 36.0;
    let v = equilibrium_speed_cms(luka, v_free, p);
    (v * 36.0 / 100.0).round().clamp(0.0, f32::from(u16::MAX)) as u16
}

/// Długość pojazdu odniesienia kalibracji — samochód osobowy. Ta sama liczba, którą
/// `data/roads/vdf.ron` ma w głowie, mówiąc „odstęp korkowy 750 cm".
pub const CALIBRATION_VEHICLE_CM: u32 = 450;

/// Ciężarówka i autobus liczą się do sufitu podwójnie: dłuższa bryła, więcej pikseli
/// i więcej sąsiadów w car-followingu (§7.3, `R-8`).
#[inline]
const fn waga(len_cm: u16) -> u32 {
    if len_cm > 700 {
        2
    } else {
        1
    }
}

/// Minuta doby, do której należy dana setna sekundy.
#[inline]
#[must_use]
pub const fn minute_of_cs(cs: u64) -> u32 {
    (cs / CS_PER_MINUTE) as u32
}

impl Default for MicroLayer {
    fn default() -> MicroLayer {
        MicroLayer::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trasa 200 m na wschód od początku układu.
    fn trasa() -> Vec<WorldCoord> {
        vec![WorldCoord::new(0, 0, 0), WorldCoord::new(20_000, 0, 0)]
    }

    fn warstwa() -> MicroLayer {
        MicroLayer::with_params(IdmParams::default())
    }

    #[test]
    fn pieszy_poza_oknem_nie_trafia_do_bufora() {
        let m = warstwa();
        // Bez okna warstwa jest wyłączona — headless nie ma kadru.
        m.enter(1, &trasa(), 0, 10);
        assert!(m.is_empty(), "warstwa Mikro chodzi bez kadru");

        m.set_window(Some((10_000, 0)), 50);
        m.enter(1, &trasa(), 0, 10);
        assert_eq!(m.len(), 0, "pieszy wszedł w kadr, którego nie widać");

        m.set_window(Some((200, 0)), 50);
        m.enter(1, &trasa(), 0, 10);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn postep_rosnie_monotonicznie_i_konczy_sie_w_minucie_przybycia() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        m.enter(7, &trasa(), 480, 490);

        let postep = |m: &MicroLayer| m.peds.lock().expect("micro").get(0).progress;
        let mut zrzut = Vec::new();
        let mut poprzedni = -1.0f32;
        let krokow = 10u64 * 600;
        for krok in 0..krokow {
            m.step(480 * 60_000 + krok * 100);
            let p = postep(&m);
            assert!(p >= poprzedni, "postęp cofnął się w kroku {krok}");
            assert!(p < 1.0, "mikro dotarło przed czasem mezo");
            poprzedni = p;
        }
        m.step(490 * 60_000);
        m.snapshot(&mut zrzut);
        let r = zrzut[0];
        assert_eq!(r.entity, 7);
        assert!((postep(&m) - 1.0).abs() < 1e-6);
        assert!(
            (r.pos[0] - 200.0).abs() < 0.5 && r.pos[1].abs() < 0.5,
            "pieszy skończył w {:?}, a nie w celu",
            r.pos
        );

        m.step(490 * 60_000 + 5_000);
        assert!((postep(&m) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn retire_usuwa_tych_ktorzy_dotarli() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        m.enter(1, &trasa(), 0, 10);
        m.enter(2, &trasa(), 0, 30);
        assert_eq!(m.len(), 2);

        m.retire(10);
        let mut zrzut = Vec::new();
        m.snapshot(&mut zrzut);
        assert_eq!(zrzut.len(), 1);
        assert_eq!(zrzut[0].entity, 2, "retire usunął niewłaściwego pieszego");

        m.retire(30);
        assert!(m.is_empty(), "pieszy po przybyciu został w buforze");
    }

    #[test]
    fn rozmiar_pieszego_zgadza_sie_z_budzetem() {
        assert_eq!(size_of::<Pedestrian>(), 32);
    }

    fn feed(vehicle: u32, entry_cs: u64, exit_cs: u64, lanes: u8) -> VehicleFeed {
        VehicleFeed {
            vehicle,
            edge: 3,
            class: 0,
            lanes,
            len_cm: 450,
            // 200 m w 20 s to 10 m/s.
            v_free_cms: 1_000.0,
            entry_cs,
            exit_cs,
            car_following: true,
        }
    }

    /// Pojazd w kadrze jedzie i **dojeżdża**, a nie stoi ani nie wyprzedza czasu mezo.
    #[test]
    fn pojazd_przejezdza_krawedz_w_zaksiegowanym_czasie() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        assert!(m.begin_vehicle_feed());
        m.feed_vehicle(feed(5, 0, 2_000, 1), &trasa());
        m.end_vehicle_feed();
        assert_eq!(m.vehicles(), 1);

        let mut zrzut = Vec::new();
        let mut poprzednia_x = -1.0f32;
        for ms in (0..20_000).step_by(1_000) {
            m.step(ms);
            m.vehicle_snapshot(&mut zrzut);
            assert!(zrzut[0].pos[0] >= poprzednia_x - 0.01, "pojazd się cofnął");
            poprzednia_x = zrzut[0].pos[0];
        }
        m.step(20_000);
        m.vehicle_snapshot(&mut zrzut);
        assert!(
            zrzut[0].pos[0] > 150.0,
            "pojazd przejechał tylko {} m z 200 w zaksięgowanych 20 s",
            zrzut[0].pos[0]
        );
        assert!((zrzut[0].heading).abs() < 1e-3, "kurs nie jest wzdłuż osi X");
    }

    /// Kolejka: pojazd bez prawa wyjazdu staje przed linią, a następny staje za nim.
    #[test]
    fn kolejka_przed_linia_wyjazdowa() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        assert!(m.begin_vehicle_feed());
        // Obaj mają wyjechać dopiero po 120 s, a krawędź ma 200 m.
        m.feed_vehicle(feed(1, 0, 12_000, 1), &trasa());
        m.feed_vehicle(feed(2, 0, 12_000, 1), &trasa());
        m.end_vehicle_feed();

        for ms in (0..60_000).step_by(100) {
            m.step(ms);
        }
        let buf = m.vehs.lock().expect("micro");
        let (a, b) = (buf.get(0), buf.get(1));
        let (czolo, tyl) = if a.pos_cm > b.pos_cm { (a, b) } else { (b, a) };
        assert!(
            czolo.pos_cm > 19_000.0,
            "czoło kolejki stanęło {} cm przed linią",
            20_000.0 - czolo.pos_cm
        );
        assert!(
            czolo.pos_cm - tyl.pos_cm >= f32::from(tyl.len_cm),
            "pojazdy weszły na siebie: {} vs {}",
            czolo.pos_cm,
            tyl.pos_cm
        );
        assert!(czolo.speed_cms < 50.0, "czoło kolejki nie zatrzymało się");
    }

    /// Kurs komunikacji odgrywa czas i **nie** liczy car-followingu — dokładnie jak pieszy.
    #[test]
    fn kurs_komunikacji_odgrywa_czas_z_rozkladu() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        assert!(m.begin_vehicle_feed());
        let mut f = feed(9, 0, 6_000, 1);
        f.edge = NO_EDGE;
        f.car_following = false;
        f.len_cm = 1_200;
        m.feed_vehicle(f, &trasa());
        m.end_vehicle_feed();

        m.step(30_000);
        let mut zrzut = Vec::new();
        m.vehicle_snapshot(&mut zrzut);
        assert!(
            (zrzut[0].pos[0] - 100.0).abs() < 1.0,
            "w połowie rozkładu kurs był w {} m, a nie w połowie trasy",
            zrzut[0].pos[0]
        );
    }

    /// Sufit rysowania odcina nadmiar, a duża bryła liczy się podwójnie.
    #[test]
    fn sufit_rysowania_odcina_nadmiar() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        m.set_cap(3);
        assert!(m.begin_vehicle_feed());
        m.feed_vehicle(feed(1, 0, 6_000, 1), &trasa());
        let mut duzy = feed(2, 0, 6_000, 1);
        duzy.len_cm = 1_200;
        m.feed_vehicle(duzy, &trasa());
        m.feed_vehicle(feed(3, 0, 6_000, 1), &trasa());
        m.end_vehicle_feed();
        assert_eq!(m.vehicles(), 2, "sufit nie policzył dużej bryły podwójnie");
    }

    /// Pojazd, który został na tej samej krawędzi, nie przeskakuje na jej początek.
    #[test]
    fn zasilenie_zachowuje_pozycje_na_tej_samej_krawedzi() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        assert!(m.begin_vehicle_feed());
        m.feed_vehicle(feed(4, 0, 12_000, 1), &trasa());
        m.end_vehicle_feed();
        m.step(30_000);
        let przed = m.vehs.lock().expect("micro").get(0).pos_cm;
        assert!(przed > 100.0);

        assert!(m.begin_vehicle_feed());
        m.feed_vehicle(feed(4, 0, 12_000, 1), &trasa());
        m.end_vehicle_feed();
        let po = m.vehs.lock().expect("micro").get(0).pos_cm;
        assert!((po - przed).abs() < 1.0, "{przed} → {po}");

        // Zmiana krawędzi zeruje pozycję — to jest nowy odcinek, nie ten sam.
        assert!(m.begin_vehicle_feed());
        let mut dalej = feed(4, 12_000, 24_000, 1);
        dalej.edge = 4;
        m.feed_vehicle(dalej, &trasa());
        m.end_vehicle_feed();
        assert_eq!(m.vehs.lock().expect("micro").get(0).pos_cm, 0.0);
    }

    /// Pojazd niezgłoszony w zasilaniu znika z kadru.
    #[test]
    fn niezgloszony_pojazd_znika() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        assert!(m.begin_vehicle_feed());
        m.feed_vehicle(feed(1, 0, 6_000, 1), &trasa());
        m.feed_vehicle(feed(2, 0, 6_000, 1), &trasa());
        m.end_vehicle_feed();
        assert_eq!(m.vehicles(), 2);

        assert!(m.begin_vehicle_feed());
        m.feed_vehicle(feed(2, 0, 6_000, 1), &trasa());
        m.end_vehicle_feed();
        assert_eq!(m.vehicles(), 1);
        assert_eq!(m.vehs.lock().expect("micro").get(0).vehicle, 2);
    }

    /// Plik danych musi zgadzać się z tym, co kod zakłada — `delta` inna niż 4 byłaby
    /// cichym kłamstwem, bo człon prędkości liczy się jako kwadrat kwadratu.
    #[test]
    fn parametry_z_danych_zgadzaja_sie_z_domyslnymi() {
        let z_pliku = IdmParams::load_default().expect("data/roads/idm.ron");
        assert_eq!(z_pliku, IdmParams::default());
    }

    /// **Miernik dryfu kalibracji** (§5.4, B1–B4 w wersji zamkniętej): diagram
    /// podstawowy wyprowadzony z parametrów IDM ma opowiadać tę samą historię, co
    /// tabela, z której mezo liczy pieniądze. Rozjazd tych dwóch znaczy, że gracz widzi
    /// płynny ruch i dostaje rachunek za korek — albo odwrotnie (ryzyko R2).
    ///
    /// Tolerancja 25 % nie jest luzem, tylko **nazwaniem tego, czego jeden zestaw
    /// parametrów IDM nie umie wyrazić**: VDF daje klasom szybkim wyższą prędkość
    /// zjazdu z kolejki niż klasom lokalnym, a `s0` i `T` są w `idm.ron` wspólne dla
    /// wszystkich. Ścieżka wyjścia, gdyby to zaczęło przeszkadzać: parametry per klasa.
    #[test]
    fn diagram_idm_zgadza_sie_z_vdf() {
        use crate::spec::VdfTable;
        use magnat_core::RoadClass;

        let vdf = VdfTable::load_default().expect("data/roads/vdf.ron");
        let p = IdmParams::load_default().expect("data/roads/idm.ron");
        // Prędkości swobodne klas — te same, które generator miasta wpisuje krawędziom
        // (`sim/world::city::road::SPECS`). Powtórzone tu, bo `sim/traffic` nie może
        // zależeć od `sim/world`: zależność idzie w drugą stronę.
        let klasy = [
            (RoadClass::Highway, 1_200u16),
            (RoadClass::Arterial, 600),
            (RoadClass::Collector, 500),
            (RoadClass::Local, 300),
            (RoadClass::Service, 200),
        ];
        for (c, free) in klasy {
            let z_vdf = vdf.speed_dkmh(c, free, 100, 100);
            let z_idm = idm_speed_dkmh(
                1000,
                vdf.jam_spacing_cm(),
                CALIBRATION_VEHICLE_CM,
                free,
                &p,
            );
            let blad = (f64::from(z_idm) - f64::from(z_vdf)).abs() / f64::from(z_vdf.max(1));
            assert!(
                blad <= 0.25,
                "klasa {}: VDF mówi {z_vdf} dkmh przy zapełnieniu, IDM {z_idm} —                  rozjazd {:.0} %. Animacja przestała pasować do księgowości (R2);                  uruchom `headless calibrate-vdf` i zdecyduj, która strona się myli",
                c.key(),
                blad * 100.0
            );
        }
    }

    /// Równowaga jest monotoniczna po luce: więcej miejsca = nie wolniej. Bez tego
    /// bisekcja mogłaby zwrócić cokolwiek i nikt by tego nie zauważył.
    #[test]
    fn rownowaga_rosnie_z_luka_i_nasyca_sie_na_predkosci_swobodnej() {
        let p = IdmParams::default();
        let v0 = 1_000.0;
        let mut poprzednia = -1.0;
        for luka in (100..20_000).step_by(100) {
            let v = equilibrium_speed_cms(luka as f32, v0, &p);
            assert!(v >= poprzednia - 1e-3, "równowaga spadła przy luce {luka}");
            assert!(v <= v0, "równowaga przebiła prędkość swobodną");
            poprzednia = v;
        }
        assert!(equilibrium_speed_cms(100_000.0, v0, &p) > v0 * 0.99);
        assert_eq!(equilibrium_speed_cms(50.0, v0, &p), 0.0, "luka mniejsza od s0");
    }

    /// Zmiana pasa: wolny poprzednik na pasie 0, wolny pas 1 — MOBIL ma przełożyć.
    #[test]
    fn mobil_wyprowadza_na_wolny_pas() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        assert!(m.begin_vehicle_feed());
        // Wolniejszy z przodu (ma dużo czasu), szybszy tuż za nim.
        let mut wolny = feed(1, 0, 30_000, 2);
        wolny.v_free_cms = 200.0;
        m.feed_vehicle(wolny, &trasa());
        m.feed_vehicle(feed(2, 0, 2_000, 2), &trasa());
        m.end_vehicle_feed();
        {
            let mut buf = m.vehs.lock().expect("micro");
            buf.vehs[0].pos_cm = 3_000.0;
            buf.vehs[0].speed_cms = 200.0;
            buf.vehs[1].pos_cm = 2_000.0;
            buf.vehs[1].speed_cms = 1_000.0;
        }
        for ms in (0..4_000).step_by(100) {
            m.step(ms);
        }
        let buf = m.vehs.lock().expect("micro");
        assert_eq!(buf.get(1).lane, 1, "szybszy pojazd nie wyszedł na wolny pas");
    }
}
