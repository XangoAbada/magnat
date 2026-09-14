//! Warstwa LOD Mikro — pozycje widoczne w kadrze (M4d §5.10, wcześniej `sim/agents`).
//!
//! **Wizualizator bez prawa zapisu** (00 §4): nie dotyka potrzeb, kolejki zdarzeń ani
//! czasu przybycia. Postęp encji wynika **wyłącznie** z pary `(depart_min, arrive_min)`
//! policzonej przez warstwę wyższą — mikro interpoluje między wyliczonym startem
//! a wyliczonym przybyciem i nie ma jak ich zmienić. Dlatego test spójności LOD
//! przechodzi z tolerancją 0 **z konstrukcji**, a nie przez kalibrację: pieszy dociera
//! dokładnie w swojej minucie niezależnie od tego, czy krok mikro w ogóle się wykonał
//! i ile razy.
//!
//! Okno jest stanem prezentacji, a nie symulacji: może zależeć od tego, gdzie stoi
//! kamera, i hash stanu jest ten sam. Promień 0 (stan domyślny) wyłącza warstwę —
//! headless nie ma kadru, a setki tysięcy polilinii w pamięci to koszt, którego nikt
//! by nie oglądał.
//!
//! Arytmetyka trasy jest całkowitoliczbowa w centymetrach; float pojawia się dopiero
//! w pozycji wyjściowej, czyli po stronie prezentacji.
//!
//! `ponytail:` bufor wyłącznie dla pieszych. Sufit nazwany: pojazd wchodzący w kadr
//! nie ma gdzie usiąść, bo `Pedestrian` niesie tylko pozycję i postęp. Ścieżka wyjścia:
//! wspólny bufor dla pieszych, pojazdów i pasażerów dokłada M4d/WP8 — do tego czasu
//! `MicroLayer` jest jedną trasą i jedną encją na trasę.

use magnat_core::WorldCoord;
use magnat_sim_snapshot::PedestrianRecord;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::Mutex;

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
    points: Vec<WorldCoord>,
    /// `(offset w points, liczba punktów, długość trasy w cm)`.
    paths: Vec<(u32, u32, u32)>,
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
        debug_assert!(route.len() >= 2, "trasa bez dwóch końców");
        let offset = self.points.len() as u32;
        self.points.extend_from_slice(route);
        let dlugosc: u32 = route
            .windows(2)
            .map(|w| euclid_cm(w[0], w[1]))
            .fold(0u32, u32::saturating_add);
        self.paths
            .push((offset, route.len() as u32, dlugosc.max(1)));
        let pierwszy = route[0];
        self.peds.push(Pedestrian {
            pos: [
                pierwszy.x as f32 / 100.0,
                pierwszy.y as f32 / 100.0,
                pierwszy.z as f32 / 100.0,
            ],
            progress: 0.0,
            path: (self.paths.len() - 1) as u32,
            citizen,
            depart_min,
            arrive_min,
            dir: 0,
            _pad: [0; 3],
        });
        self.peds.len() - 1
    }

    /// Krok mikro: tick 100 ms gry. `now_ms` to milisekunda doby.
    ///
    /// Postęp wynika **wyłącznie** z pary `(depart, arrive)` policzonej przez mezo —
    /// dlatego pieszy dociera dokładnie w swojej minucie niezależnie od tego, czy krok
    /// mikro w ogóle się wykonał i ile razy. To jest konstrukcja, nie kalibracja (00 §4).
    pub fn step(&mut self, now_ms: u64) {
        let teraz = now_ms as f32 / 60_000.0;
        for p in &mut self.peds {
            let start = f32::from(p.depart_min);
            let koniec = f32::from(p.arrive_min);
            let t = if koniec <= start {
                1.0
            } else {
                ((teraz - start) / (koniec - start)).clamp(0.0, 1.0)
            };
            p.progress = t;
            let (offset, len, dlugosc) = self.paths[p.path as usize];
            let punkty = &self.points[offset as usize..(offset + len) as usize];
            p.pos = punkt_na_lamanej(punkty, dlugosc, t);
        }
    }

    /// Usuwa pieszych, którzy dotarli. Arena punktów nie jest kompaktowana w locie:
    /// zbiór encji Mikro jest rzędu tysięcy (kadr kamery), więc zwolnienie jej dopiero
    /// wtedy, gdy opustoszeje, kosztuje mniej niż przepisywanie offsetów co minutę.
    pub fn retire(&mut self, now_min: u16) {
        self.peds.retain(|p| p.arrive_min > now_min);
        if self.peds.is_empty() {
            self.points.clear();
            self.paths.clear();
        }
    }
}

/// Punkt na łamanej w ułamku `t` jej długości.
fn punkt_na_lamanej(punkty: &[WorldCoord], dlugosc_cm: u32, t: f32) -> [f32; 3] {
    let cel = t.clamp(0.0, 1.0) * dlugosc_cm as f32;
    let mut przebyte = 0.0f32;
    for w in punkty.windows(2) {
        let d = euclid_cm(w[0], w[1]) as f32;
        if przebyte + d >= cel {
            let u = if d == 0.0 { 0.0 } else { (cel - przebyte) / d };
            return [
                (w[0].x as f32 + (w[1].x - w[0].x) as f32 * u) / 100.0,
                (w[0].y as f32 + (w[1].y - w[0].y) as f32 * u) / 100.0,
                (w[0].z as f32 + (w[1].z - w[0].z) as f32 * u) / 100.0,
            ];
        }
        przebyte += d;
    }
    let ostatni = punkty[punkty.len() - 1];
    [
        ostatni.x as f32 / 100.0,
        ostatni.y as f32 / 100.0,
        ostatni.z as f32 / 100.0,
    ]
}

/// Okno LOD Mikro plus bufor encji w kadrze.
///
/// Mutacja idzie przez `Mutex` i atomiki, bo wołający trzyma `&self`: warstwa siedzi
/// w zasobie dzielonym z rendererem, a nie w wyłącznym stanie systemu. To jest wybór
/// właściwy dla wizualizatora — gdyby warstwa cokolwiek liczyła, dostałaby `&mut self`
/// i nie miałaby zamka.
pub struct MicroLayer {
    peds: Mutex<PedestrianBuffer>,
    /// Środek okna w metrach; promień 0 = warstwa wyłączona.
    center: (AtomicI32, AtomicI32),
    radius_m: AtomicU32,
}

impl MicroLayer {
    #[must_use]
    pub fn new() -> MicroLayer {
        MicroLayer {
            peds: Mutex::new(PedestrianBuffer::new()),
            center: (AtomicI32::new(0), AtomicI32::new(0)),
            radius_m: AtomicU32::new(0),
        }
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
    /// Trasa wchodzi, gdy którykolwiek jej koniec mieści się w oknie; poza tym metoda
    /// milczy i nie robi nic.
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

    /// Krok mikro: `now_ms` to milisekunda doby, tick 100 ms (00 §4).
    pub fn step(&self, now_ms: u64) {
        self.peds.lock().expect("micro").step(now_ms);
    }

    /// Usuwa encje, które już dotarły.
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

    /// Zrzut dla renderera — **jedna kopia, prosto do struktury docelowej**.
    ///
    /// Wcześniej metoda oddawała krotki `(encja, pozycja, postęp)`, a wołający
    /// przepisywał je natychmiast drugi raz na `PedestrianRecord`, żeby odrzucić
    /// postęp, którego renderer nie czyta. Dwie pełne kopie `O(n)` na klatkę zamiast
    /// jednej (M4c §5.12 punkt 3).
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

    #[test]
    fn pieszy_poza_oknem_nie_trafia_do_bufora() {
        let m = MicroLayer::new();
        // Bez okna warstwa jest wyłączona — headless nie ma kadru.
        m.enter(1, &trasa(), 0, 10);
        assert!(m.is_empty(), "warstwa Mikro chodzi bez kadru");

        // Okno o promieniu 50 m wokół punktu 10 km stąd: oba końce trasy są poza nim.
        m.set_window(Some((10_000, 0)), 50);
        m.enter(1, &trasa(), 0, 10);
        assert_eq!(m.len(), 0, "pieszy wszedł w kadr, którego nie widać");

        // Wystarczy, że w oknie jest jeden koniec trasy.
        m.set_window(Some((200, 0)), 50);
        m.enter(1, &trasa(), 0, 10);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn postep_rosnie_monotonicznie_i_konczy_sie_w_minucie_przybycia() {
        let m = MicroLayer::new();
        m.set_window(Some((0, 0)), 1_000);
        m.enter(7, &trasa(), 480, 490);

        let postep = |m: &MicroLayer| m.peds.lock().expect("micro").get(0).progress;
        let mut zrzut = Vec::new();
        let mut poprzedni = -1.0f32;
        let krokow = 10u64 * 600; // 600 kroków po 100 ms = minuta gry
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

        // Krok po czasie przybycia niczego nie psuje i nie cofa.
        m.step(490 * 60_000 + 5_000);
        assert!((postep(&m) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn retire_usuwa_tych_ktorzy_dotarli() {
        let m = MicroLayer::new();
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
}
