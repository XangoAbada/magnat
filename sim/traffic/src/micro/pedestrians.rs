//! Piesi w kadrze i arena polilinii, po których poruszają się encje Mikro.
//!
//! Wydzielone z `micro.rs` w R-WP10 bez zmiany zachowania. `Pedestrian` i
//! `PedestrianBuffer` nie mają z `VehicleBuffer` wspólnego nic poza tym, że jedno
//! i drugie się porusza — wspólna jest tylko arena tras, i dlatego leży tutaj.

use super::*;

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
pub(super) struct PathArena {
    points: Vec<WorldCoord>,
    /// `(offset w points, liczba punktów, długość w cm)`.
    paths: Vec<(u32, u32, u32)>,
}

impl PathArena {
    pub(super) fn push(&mut self, route: &[WorldCoord]) -> u32 {
        debug_assert!(route.len() >= 2, "trasa bez dwóch końców");
        let offset = self.points.len() as u32;
        self.points.extend_from_slice(route);
        let dlugosc: u32 = route
            .windows(2)
            .map(|w| euclid_cm(w[0], w[1]))
            .fold(0u32, u32::saturating_add);
        self.paths
            .push((offset, route.len() as u32, dlugosc.max(1)));
        (self.paths.len() - 1) as u32
    }

    #[inline]
    pub(super) fn len_cm(&self, path: u32) -> u32 {
        self.paths[path as usize].2
    }

    pub(super) fn clear(&mut self) {
        self.points.clear();
        self.paths.clear();
    }

    /// Punkt na łamanej w ułamku `t` jej długości plus kurs odcinka, na którym leży.
    pub(super) fn at(&self, path: u32, t: f32) -> ([f32; 3], f32) {
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
