//! Rig kamery (PRD §15.2, M1 WP-C1).
//!
//! Trzy tryby: orbita nad miastem, swobodny lot i pierwsza osoba. Zoom miesza dystans z FOV
//! po krzywej — od dalekiej orbity z wąskim polem widzenia (wrażenie izometrii, §15.2)
//! do bliskiej perspektywy na poziomie ulicy.
//!
//! **Pozycja kamery jest w `f64`.** Mapa 16 km w rozdzielczości 1 m przekracza precyzję `f32`
//! na krawędziach: przy 16 000 m najmniejszy odstęp `f32` to ~1 mm, a po przemnożeniu przez
//! macierz widoku błąd rośnie do centymetrów i geometria zaczyna drgać. Do shaderów idą
//! **pozycje względem kamery**, nie absolutne — i to jest powód, dla którego per-chunk SSBO
//! niesie offset, a nie pełną macierz świata.

use glam::{DVec3, Mat4, Vec3};

/// Tryb pracy kamery.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CameraMode {
    /// Orbita nad punktem: obrót, pochylenie, zoom.
    Orbit {
        target: DVec3,
        dist: f32,
        yaw: f32,
        pitch: f32,
    },
    /// Swobodny lot.
    Free { pos: DVec3, yaw: f32, pitch: f32 },
    /// Pierwsza osoba. Wariant dostarcza **M1** (PRD §15.2 jest w zakresie tej fazy);
    /// M11 wiąże go z encją postaci gracza z M9, nie dopisuje go od nowa (M1 §6.1).
    ///
    /// Pozycja jest **własna**, a `anchor` opcjonalny, i to nie jest zapas na przyszłość:
    /// w M1 nie ma encji, a tryb ma działać już teraz — inaczej kryterium „przejście
    /// orbita → poziom ulicy → pierwsza osoba" nie da się ani pokazać, ani sprawdzić.
    /// Gdy M9 poda encję, `anchor` przejmuje prowadzenie i `pos` staje się jej śladem.
    FirstPerson {
        pos: DVec3,
        yaw: f32,
        pitch: f32,
        anchor: Option<u64>,
        eye_height_m: f32,
    },
}

/// Stan kamery — zasób ramki czytany przez każdy pass i przez `engine/audio` (M1 §6.1).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CameraState {
    pub mode: CameraMode,
    /// 20° przy dalekiej orbicie (quasi-izometria) → 60° na ulicy.
    pub fov_deg: f32,
    /// Cięcie poziomem w jednostkach 0,5 m. Uniform; cap przekroju dokłada M11.
    pub clip_plane_z: Option<i32>,
    pub near: f32,
    pub far: f32,
}

/// Najmniejsze i największe pochylenie. Pion pełny łamałby bazę `look_at`, a spojrzenie
/// spod ziemi nie ma zastosowania.
const PITCH_MIN: f32 = 5.0f32 * std::f32::consts::PI / 180.0;
const PITCH_MAX: f32 = 89.0f32 * std::f32::consts::PI / 180.0;
/// Granice zoomu orbity w metrach.
pub const ORBIT_MIN_M: f32 = 8.0;
pub const ORBIT_MAX_M: f32 = 6000.0;
/// FOV na krańcach zoomu.
const FOV_NEAR_DEG: f32 = 60.0;
const FOV_FAR_DEG: f32 = 20.0;

impl Default for CameraState {
    fn default() -> Self {
        CameraState {
            mode: CameraMode::Orbit {
                target: DVec3::ZERO,
                dist: 600.0,
                yaw: 0.7,
                pitch: 0.7,
            },
            fov_deg: 35.0,
            clip_plane_z: None,
            near: 0.5,
            far: 8000.0,
        }
    }
}

impl CameraState {
    /// Pozycja oka w metrach.
    #[must_use]
    pub fn eye(&self) -> DVec3 {
        match self.mode {
            CameraMode::Orbit {
                target,
                dist,
                yaw,
                pitch,
            } => {
                let d = f64::from(dist);
                target
                    + DVec3::new(
                        d * f64::from(pitch.cos()) * f64::from(yaw.sin()),
                        d * f64::from(pitch.cos()) * f64::from(yaw.cos()),
                        d * f64::from(pitch.sin()),
                    )
            }
            CameraMode::Free { pos, .. } => pos,
            CameraMode::FirstPerson {
                pos, eye_height_m, ..
            } => pos + DVec3::new(0.0, 0.0, f64::from(eye_height_m)),
        }
    }

    /// Punkt, na który patrzy kamera.
    #[must_use]
    pub fn target(&self) -> DVec3 {
        match self.mode {
            CameraMode::Orbit { target, .. } => target,
            CameraMode::Free { pos, yaw, pitch } => pos + kierunek(yaw, pitch),
            CameraMode::FirstPerson { yaw, pitch, .. } => self.eye() + kierunek(yaw, pitch),
        }
    }

    /// Macierz widok-rzutowanie **względem kamery**: oko jest w początku układu.
    ///
    /// To nie jest kosmetyka — patrz nagłówek modułu. Geometria chunków dostaje offset
    /// w per-chunk SSBO i jest w tej samej przestrzeni, więc `f32` w shaderze pracuje
    /// na liczbach rzędu setek metrów, a nie dziesiątek tysięcy.
    #[must_use]
    pub fn view_proj_relative(&self, aspect: f32) -> Mat4 {
        let eye = self.eye();
        let target = self.target();
        let view = glam::dcamera::rh::view::look_at_mat4(DVec3::ZERO, target - eye, DVec3::Z);
        // Konwencja wgpu/DirectX: zakres Z w NDC to [0, 1], nie [−1, 1] jak w OpenGL.
        let proj = glam::camera::rh::proj::directx::perspective(
            self.fov_deg.to_radians(),
            aspect.max(0.01),
            self.near,
            self.far,
        );
        proj * view.as_mat4()
    }

    /// Kierunek patrzenia, znormalizowany.
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        (self.target() - self.eye()).as_vec3().normalize_or_zero()
    }

    /// Obrót i pochylenie. Pochylenie jest zaciskane, nie zawijane.
    pub fn orbit_rotate(&mut self, d_yaw: f32, d_pitch: f32) {
        match &mut self.mode {
            CameraMode::Orbit { yaw, pitch, .. } => {
                *yaw += d_yaw;
                *pitch = (*pitch + d_pitch).clamp(PITCH_MIN, PITCH_MAX);
            }
            // W trybach z pierwszej osoby myszka obraca głowę, a nie orbitę — i pochylenie
            // idzie w drugą stronę, bo patrzymy „z" punktu, a nie „na" punkt.
            CameraMode::Free { yaw, pitch, .. } | CameraMode::FirstPerson { yaw, pitch, .. } => {
                *yaw -= d_yaw;
                *pitch = (*pitch - d_pitch).clamp(PITCH_MIN, PITCH_MAX);
            }
        }
    }

    /// Kąty odpowiadające bieżącemu kierunkowi patrzenia.
    fn katy(&self) -> (f32, f32) {
        match self.mode {
            CameraMode::Free { yaw, pitch, .. } | CameraMode::FirstPerson { yaw, pitch, .. } => {
                (yaw, pitch)
            }
            // Orbita patrzy **na** cel, czyli w stronę przeciwną do przesunięcia oka.
            CameraMode::Orbit { yaw, pitch, .. } => (yaw + std::f32::consts::PI, -pitch),
        }
    }

    /// Przełącza na swobodny lot, zachowując pozycję oka i kierunek patrzenia.
    ///
    /// Zachowanie jednego i drugiego jest całym sensem tych przejść: kryterium C1 mówi
    /// „płynne", a płynne znaczy tu dokładnie tyle, że w klatce przełączenia obraz się
    /// nie zmienia. Reszta płynności to sprawa interpolacji, nie zmiany trybu.
    pub fn to_free(&mut self) {
        let (yaw, pitch) = self.katy();
        self.mode = CameraMode::Free {
            pos: self.eye(),
            yaw,
            pitch,
        };
    }

    /// Przełącza na pierwszą osobę: oko zostaje tam, gdzie patrzyło, a stopy lądują
    /// na podanej rzędnej terenu.
    pub fn to_first_person(&mut self, ground_z_m: f64, eye_height_m: f32) {
        let (yaw, pitch) = self.katy();
        let eye = self.eye();
        self.mode = CameraMode::FirstPerson {
            pos: DVec3::new(eye.x, eye.y, ground_z_m),
            yaw,
            pitch,
            anchor: None,
            eye_height_m,
        };
    }

    /// Przypina widok pierwszoosobowy do encji (M11c §5.7).
    ///
    /// `anchor` niesie `entity_lo` postaci gracza; pozycję oka podaje potem
    /// [`CameraState::set_eye`] z rekordu snapshotu. Kamera **nie chodzi sama** — postać
    /// gracza jest zwykłym agentem, a tryb pierwszoosobowy tylko ją ogląda.
    pub fn set_anchor(&mut self, anchor: Option<u64>) {
        if let CameraMode::FirstPerson { anchor: a, .. } = &mut self.mode {
            *a = anchor;
        }
    }

    /// Kotwica widoku pierwszoosobowego, jeśli jest.
    #[must_use]
    pub fn anchor(&self) -> Option<u64> {
        match self.mode {
            CameraMode::FirstPerson { anchor, .. } => anchor,
            _ => None,
        }
    }

    /// Stawia oko przypiętej kamery w zadanym punkcie (grunt pod postacią, w metrach).
    ///
    /// Bierze **grunt**, a nie oko: wysokość oczu jest własnością kamery (`eye_height_m`)
    /// i dokłada ją `eye()`, więc podanie tu gotowej rzędnej oka podniosłoby ją dwa razy.
    pub fn set_eye(&mut self, ground: DVec3) {
        if let CameraMode::FirstPerson { pos, .. } = &mut self.mode {
            *pos = ground;
        }
    }

    /// Przełącza na orbitę wokół punktu oddalonego o `dist` w kierunku patrzenia.
    pub fn to_orbit(&mut self, dist: f32) {
        let (yaw, pitch) = self.katy();
        let eye = self.eye();
        self.mode = CameraMode::Orbit {
            target: eye + kierunek(yaw, pitch) * f64::from(dist),
            dist,
            yaw: yaw + std::f32::consts::PI,
            pitch: -pitch,
        };
        self.fov_deg = self.fov_for_distance();
    }

    /// Zoom orbity. Mnożnikowy, nie addytywny: przy dystansie 3 km krok 10 m jest niewidoczny,
    /// a przy 20 m wyrzuca kamerę pod ziemię.
    pub fn orbit_zoom(&mut self, factor: f32) {
        if let CameraMode::Orbit { dist, .. } = &mut self.mode {
            *dist = (*dist * factor).clamp(ORBIT_MIN_M, ORBIT_MAX_M);
        }
        self.fov_deg = self.fov_for_distance();
    }

    /// FOV wyprowadzone z dystansu orbity — krzywa z §15.2.
    ///
    /// Interpolacja idzie po **logarytmie** dystansu, bo tak działa postrzeganie skali:
    /// przejście 20 → 200 m ma zmienić kadr tak samo jak 200 → 2000 m.
    #[must_use]
    pub fn fov_for_distance(&self) -> f32 {
        let CameraMode::Orbit { dist, .. } = self.mode else {
            return FOV_NEAR_DEG;
        };
        let lo = ORBIT_MIN_M.ln();
        let hi = ORBIT_MAX_M.ln();
        let t = ((dist.clamp(ORBIT_MIN_M, ORBIT_MAX_M).ln() - lo) / (hi - lo)).clamp(0.0, 1.0);
        FOV_NEAR_DEG + (FOV_FAR_DEG - FOV_NEAR_DEG) * t
    }

    /// Przesuwa cel orbity w płaszczyźnie poziomej, w osiach ekranu.
    pub fn orbit_pan(&mut self, right_m: f32, up_m: f32) {
        let CameraMode::Orbit { yaw, .. } = self.mode else {
            return;
        };
        let (s, c) = (f64::from(yaw.sin()), f64::from(yaw.cos()));
        if let CameraMode::Orbit { target, .. } = &mut self.mode {
            // Oś „w prawo" na ekranie to `forward × Z` (baza prawoskrętna, up = +Z),
            // czyli (−cos yaw, sin yaw); „w górę" rzutowane na poziom to sam kierunek
            // patrzenia, czyli (−sin yaw, −cos yaw).
            target.x -= f64::from(right_m) * c + f64::from(up_m) * s;
            target.y += f64::from(right_m) * s - f64::from(up_m) * c;
        }
    }

    /// Nie pozwala kamerze wejść pod teren.
    ///
    /// Margines jest w metrach i celowo niezerowy: kamera dokładnie na powierzchni
    /// przycina geometrię płaszczyzną bliską i wygląda jak błąd renderu.
    pub fn clamp_above_terrain(&mut self, height_m: f64, margin_m: f64) {
        let min_z = height_m + margin_m;
        match &mut self.mode {
            CameraMode::Orbit { target, .. } => {
                if target.z < min_z {
                    target.z = min_z;
                }
            }
            CameraMode::Free { pos, .. } => {
                if pos.z < min_z {
                    pos.z = min_z;
                }
            }
            // Pierwsza osoba stoi **na** gruncie: jej `pos` to stopy, a nie oko,
            // więc podnosi się do wysokości terenu, a nie o margines nad nim.
            CameraMode::FirstPerson { pos, .. } => pos.z = height_m,
        }
        // Przy niskim pochyleniu oko schodzi poniżej celu mimo poprawnego celu —
        // wtedy skracamy dystans orbity, bo jest to jedyna wielkość, którą wolno ruszyć
        // bez przeskoku obrazu.
        while self.eye().z < min_z {
            let CameraMode::Orbit { dist, .. } = &mut self.mode else {
                break;
            };
            if *dist <= ORBIT_MIN_M {
                break;
            }
            *dist = (*dist * 0.9).max(ORBIT_MIN_M);
        }
    }
}

fn kierunek(yaw: f32, pitch: f32) -> DVec3 {
    DVec3::new(
        f64::from(pitch.cos() * yaw.sin()),
        f64::from(pitch.cos() * yaw.cos()),
        f64::from(pitch.sin()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn przejscie_miedzy_trybami_nie_przesuwa_obrazu() {
        // Kryterium C1: orbita → poziom ulicy → pierwsza osoba ma być płynne. Jeśli
        // przełączenie trybu przesuwa oko albo kierunek, obraz przeskakuje — i żadna
        // interpolacja tego nie naprawi, bo skok jest w stanie, nie w animacji.
        // Tolerancja milimetrowa, nie zerowa: kąty są w `f32`, a przy dystansie orbity
        // 600 m jeden ulp kąta to ułamek milimetra na pozycji. Zero oznaczałoby tu
        // wymaganie dokładności, której typ nie ma.
        const TOL_M: f64 = 1.0e-3;
        let mut c = CameraState::default();
        let oko = c.eye();
        let patrzy = (c.target() - c.eye()).normalize();

        c.to_free();
        assert!(
            (c.eye() - oko).length() < TOL_M,
            "lot swobodny przesunął oko"
        );
        assert!(
            (c.target() - c.eye()).normalize().distance(patrzy) < TOL_M,
            "lot swobodny obrócił kamerę"
        );

        c.to_orbit(600.0);
        assert!(
            (c.eye() - oko).length() < TOL_M,
            "powrót do orbity przesunął oko"
        );
        assert!(
            (c.target() - c.eye()).normalize().distance(patrzy) < TOL_M,
            "powrót do orbity obrócił kamerę"
        );

        let grunt = oko.z - 50.0;
        c.to_first_person(grunt, 1.7);
        assert!(
            (c.eye().x - oko.x).abs() < TOL_M && (c.eye().y - oko.y).abs() < TOL_M,
            "pierwsza osoba przeskoczyła w poziomie"
        );
        assert!(
            (c.eye().z - (grunt + 1.7)).abs() < TOL_M,
            "pierwsza osoba nie stanęła na gruncie"
        );
        assert!(
            (c.target() - c.eye()).normalize().distance(patrzy) < TOL_M,
            "pierwsza osoba obróciła kamerę"
        );
    }

    #[test]
    fn pan_przesuwa_cel_w_osiach_ekranu() {
        // Przeciąganie ma być „chwytem" za teren: cel jedzie dokładnie w ekranowe prawo
        // i w ekranową górę. Znak łatwo tu odwrócić i nikt tego nie zauważy w typie,
        // więc bazę liczymy z kierunku patrzenia, nie z pamięci.
        let mut c = CameraState::default();
        let CameraMode::Orbit { .. } = c.mode else {
            panic!("domyślna kamera nie jest orbitą");
        };
        c.orbit_rotate(0.7, 0.0);
        let prawo = c.forward().cross(Vec3::Z).normalize();
        let gora = {
            let f = c.forward();
            Vec3::new(f.x, f.y, 0.0).normalize()
        };

        let przed = c.target();
        c.orbit_pan(10.0, 0.0);
        let d = (c.target() - przed).as_vec3();
        assert!(
            d.dot(prawo) > 9.99,
            "pan w prawo poszedł nie w prawo: {d:?}"
        );

        let przed = c.target();
        c.orbit_pan(0.0, 10.0);
        let d = (c.target() - przed).as_vec3();
        assert!(d.dot(gora) > 9.99, "pan w górę poszedł nie w górę: {d:?}");
    }

    #[test]
    fn zaden_tryb_nie_wchodzi_pod_teren() {
        let grunt = 120.0;
        for mut c in [
            CameraState::default(),
            {
                let mut c = CameraState::default();
                c.to_free();
                c
            },
            {
                let mut c = CameraState::default();
                c.to_first_person(0.0, 1.7);
                c
            },
        ] {
            c.clamp_above_terrain(grunt, 2.0);
            assert!(
                c.eye().z >= grunt,
                "kamera pod terenem: {:?} przy gruncie {grunt}",
                c.eye()
            );
        }
    }

    #[test]
    fn pochylenie_jest_zaciskane_a_nie_zawijane() {
        let mut c = CameraState::default();
        for _ in 0..100 {
            c.orbit_rotate(0.0, 1.0);
        }
        let CameraMode::Orbit { pitch, .. } = c.mode else {
            panic!()
        };
        assert!(pitch <= PITCH_MAX, "pochylenie {pitch} przekroczyło zenit");
        for _ in 0..200 {
            c.orbit_rotate(0.0, -1.0);
        }
        let CameraMode::Orbit { pitch, .. } = c.mode else {
            panic!()
        };
        assert!(pitch >= PITCH_MIN, "kamera spojrzała spod ziemi");
    }

    #[test]
    fn zoom_zwiazuje_fov_z_dystansem() {
        let mut c = CameraState::default();
        for _ in 0..60 {
            c.orbit_zoom(0.9);
        }
        let bliski_fov = c.fov_deg;
        for _ in 0..200 {
            c.orbit_zoom(1.1);
        }
        let daleki_fov = c.fov_deg;
        assert!(
            bliski_fov > daleki_fov + 20.0,
            "FOV nie zmienia się z zoomem: {bliski_fov}° → {daleki_fov}°"
        );
        assert!((FOV_FAR_DEG..=FOV_NEAR_DEG).contains(&bliski_fov));
        assert!((FOV_FAR_DEG..=FOV_NEAR_DEG).contains(&daleki_fov));
    }

    #[test]
    fn zoom_jest_ograniczony_z_obu_stron() {
        let mut c = CameraState::default();
        for _ in 0..500 {
            c.orbit_zoom(0.5);
        }
        let CameraMode::Orbit { dist, .. } = c.mode else {
            panic!()
        };
        assert!((dist - ORBIT_MIN_M).abs() < 0.01, "dystans {dist}");
        for _ in 0..500 {
            c.orbit_zoom(2.0);
        }
        let CameraMode::Orbit { dist, .. } = c.mode else {
            panic!()
        };
        assert!((dist - ORBIT_MAX_M).abs() < 0.01, "dystans {dist}");
    }

    #[test]
    fn kamera_nie_wchodzi_pod_teren() {
        let mut c = CameraState {
            mode: CameraMode::Free {
                pos: DVec3::new(0.0, 0.0, -50.0),
                yaw: 0.0,
                pitch: 0.0,
            },
            ..CameraState::default()
        };
        c.clamp_above_terrain(12.0, 2.0);
        assert!(c.eye().z >= 14.0, "oko na {}", c.eye().z);
    }

    #[test]
    fn macierz_jest_wzgledem_kamery() {
        // Punkt w miejscu oka ma po transformacji wypaść w początku układu widoku —
        // to jest cały kontrakt „pozycje względem kamery".
        let c = CameraState::default();
        let vp = c.view_proj_relative(1.6);
        let w = vp * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!(
            w.w.abs() < 1e-3,
            "oko nie leży w początku układu: w = {}",
            w.w
        );
    }

    #[test]
    fn orbita_patrzy_na_cel() {
        let c = CameraState::default();
        let do_celu = (c.target() - c.eye()).as_vec3().normalize();
        assert!(
            (c.forward() - do_celu).length() < 1e-5,
            "kierunek patrzenia nie pokrywa się z osią oko–cel"
        );
        assert!(c.eye().z > c.target().z, "kamera orbitalna patrzy z dołu");
    }
}
