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

mod idm;
mod lanes;
mod pedestrians;

use crate::spec::DataError;
use magnat_core::{data_path, WorldCoord};
use magnat_sim_snapshot::{PedestrianRecord, VehicleRecord};
use pedestrians::PathArena;
use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::Mutex;

pub use idm::{equilibrium_speed_cms, idm_speed_dkmh, IdmParams, IDM_SCHEMA_VERSION};
pub use pedestrians::{Pedestrian, PedestrianBuffer};

/// Setne sekundy w minucie — ta sama jednostka, w której liczy przejazd warstwa mezo.
const CS_PER_MINUTE: u64 = 6_000;
const NO_SLOT: u32 = u32::MAX;
/// Krawędź, na której pojazd z nikim nie oddziałuje (kurs komunikacji odgrywany
/// z rozkładu, nie z kolejki krawędzi — `P-11`).
pub const NO_EDGE: u32 = u32::MAX;

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
        // Pozycje **od razu**, a nie dopiero przy następnym kroku. `feed` zna postęp
        // wzdłuż trasy (`pos_cm`), ale nie przelicza go na metry, a zasilenie jest
        // ostatnią rzeczą, jaka w minucie dotyka tej warstwy — więc bez tego wywołania
        // każdy pojazd stoi w kadrze w punkcie `[0, 0, 0]` przez całą minutę. Objaw:
        // „zero aut w widoku dzielnicy" (`J-2`), a rekord w snapshocie **jest**, tylko
        // wskazuje róg mapy.
        self.place(self.clock_cs);
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
        let krokow = laczny
            .div_ceil(podkrok_cs)
            .clamp(1, u64::from(p.max_substeps));
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
    /// Mieszkańcy przypięci do warstwy Mikro niezależnie od kadru (`LodPin`, M9 §9 pkt 2).
    ///
    /// Postać gracza i cel trybu „śledź" mają zostać widoczni także wtedy, gdy kamera
    /// patrzy gdzie indziej — inaczej „śledź tego mieszkańca" znaczyłoby „patrz na
    /// niego tak długo, jak na niego patrzysz". Sufit to [`MAX_PINNED`] i jest twardy:
    /// przypięcie kosztuje symulację mikro poza kadrem, więc ma być decyzją, a nie
    /// nawykiem. `u32::MAX` = wolne miejsce.
    ///
    /// **Poza hashem stanu**, tak samo jak okno: warstwa Mikro jest wizualizatorem
    /// bez prawa zapisu do stanu ekonomicznego (00 §4).
    pinned: [AtomicU32; MAX_PINNED],
}

/// Ilu mieszkańców da się przypiąć do warstwy Mikro naraz (M9 §8, ryzyko LOD).
pub const MAX_PINNED: usize = 8;

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
            pinned: std::array::from_fn(|_| AtomicU32::new(u32::MAX)),
        }
    }

    /// Przypina mieszkańców do warstwy Mikro. Lista krótsza niż [`MAX_PINNED`]
    /// zwalnia pozostałe miejsca; pusta gasi przypięcie zupełnie.
    pub fn set_pinned(&self, citizens: &[u32]) {
        for (slot, v) in self.pinned.iter().enumerate() {
            v.store(
                citizens.get(slot).copied().unwrap_or(u32::MAX),
                Ordering::Relaxed,
            );
        }
    }

    #[must_use]
    pub fn is_pinned(&self, citizen: u32) -> bool {
        citizen != u32::MAX
            && self
                .pinned
                .iter()
                .any(|v| v.load(Ordering::Relaxed) == citizen)
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
        // Przypięty mieszkaniec wchodzi do warstwy także spoza kadru (`LodPin`).
        if !self.is_pinned(citizen)
            && !self.contains(route[0])
            && !self.contains(route[route.len() - 1])
        {
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
                heading: p.heading,
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

    /// Kurs pieszego idzie do rekordu i wynika z osi trasy. Bez niego renderer musiałby
    /// go wyprowadzić z różnicy pozycji między klatkami — czyli uzależnić obrót postaci
    /// od częstotliwości publikacji (`F-5`).
    #[test]
    fn pieszy_niesie_kurs_odcinka_trasy() {
        let m = warstwa();
        m.set_window(Some((0, 0)), 1_000);
        // Trasa na północ: kurs ma wyjść π/2, a nie zero.
        let na_polnoc = vec![WorldCoord::new(0, 0, 0), WorldCoord::new(0, 20_000, 0)];
        m.enter(7, &na_polnoc, 480, 490);
        m.step(485 * 60_000);
        let mut zrzut = Vec::new();
        m.snapshot(&mut zrzut);
        assert_eq!(zrzut.len(), 1);
        assert!(
            (zrzut[0].heading - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "kurs {} rad, oczekiwano π/2",
            zrzut[0].heading
        );
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
        assert_eq!(size_of::<Pedestrian>(), 36);
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
        assert!(
            (zrzut[0].heading).abs() < 1e-3,
            "kurs nie jest wzdłuż osi X"
        );
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
            let z_idm =
                idm_speed_dkmh(1000, vdf.jam_spacing_cm(), CALIBRATION_VEHICLE_CM, free, &p);
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
        assert_eq!(
            equilibrium_speed_cms(50.0, v0, &p),
            0.0,
            "luka mniejsza od s0"
        );
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
        assert_eq!(
            buf.get(1).lane,
            1,
            "szybszy pojazd nie wyszedł na wolny pas"
        );
    }
}
