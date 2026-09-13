//! Kaskadowe mapy cieni (M1 §5.8, WP-R3).
//!
//! Cztery kaskady, każda z własną macierzą ortograficzną wyliczoną z **wycinka** frustum
//! kamery. Wszystko liczy się **względem kamery**, tak jak reszta renderera (`camera.rs`):
//! światło jest kierunkowe, więc przesunięcie układu o pozycję oka nic w nim nie zmienia,
//! a `f32` przestaje walczyć ze współrzędnymi rzędu dziesiątek tysięcy metrów.
//!
//! Dwie decyzje rozstrzygają o tym, czy cień migocze przy ruchu kamery, i obie są tutaj:
//!
//! 1. **Sfera otaczająca zamiast pudełka.** Rozmiar kaskady wyprowadzamy z promienia sfery
//!    opisanej na wycinku frustum, a nie z jego AABB w przestrzeni światła. AABB zmienia
//!    rozmiar przy każdym obrocie kamery, więc mapa cienia co klatkę ma inną skalę i krawędzie
//!    pełzają. Sfera jest niezmiennicza na obrót — kaskada może być większa, ale jest stała.
//! 2. **Zatrzask do texela.** Środek kaskady przesuwamy do najbliższej wielokrotności rozmiaru
//!    texela. Bez tego przesunięcie kamery o pół texela przesuwa cały raster cienia o ułamek
//!    texela i krawędzie drgają, mimo że scena stoi w miejscu.

use glam::{Mat4, Vec3, Vec4Swizzles};

/// Liczba kaskad (M1 §4, WP-R3).
pub const CASCADES: usize = 4;

/// Bok mapy cienia jednej kaskady w texelach.
///
/// 2048 na kaskadę to 4 × 16 MB w D32 — mieści się w budżecie §5.9 obok areny geometrii.
/// Mniej znaczy widoczne schodki na krawędzi cienia już przy pierwszej kaskadzie.
pub const SHADOW_MAP_SIZE: u32 = 2048;

/// Zasięg cieni w metrach. Dalej cień zajmuje mniej niż piksel ekranu i nie jest widoczny,
/// a rozciąganie kaskad na cały zasięg widzenia (8 km) obniżyłoby rozdzielczość wszystkich.
pub const SHADOW_DISTANCE_M: f32 = 1200.0;

/// Waga podziału logarytmicznego. 0 = równe odcinki, 1 = czysto logarytmiczny.
///
/// Czysto logarytmiczny podział daje najlepszą rozdzielczość przy kamerze, ale pierwsza
/// kaskada robi się mikroskopijna i przejście między kaskadami wypada tuż przed nosem.
/// 0,7 to klasyczny kompromis — pierwsza kaskada ma kilkanaście metrów, czwarta resztę.
const LAMBDA: f32 = 0.7;

/// Zapas głębi przed kaskadą: obiekty **za** wycinkiem frustum też rzucają do niego cień.
const DEPTH_MARGIN_M: f32 = 400.0;

/// Jedna kaskada: macierz światła i koniec jej zakresu w metrach od kamery.
#[derive(Clone, Copy, Debug)]
pub struct Cascade {
    pub view_proj: Mat4,
    pub far_m: f32,
    /// Rozmiar texela w metrach — wejście do biasu w shaderze.
    pub texel_m: f32,
}

/// Granice kaskad: podział logarytmiczno-liniowy między `near` a [`SHADOW_DISTANCE_M`].
#[must_use]
pub fn splits(near_m: f32) -> [f32; CASCADES] {
    let mut out = [0.0; CASCADES];
    let (n, f) = (near_m.max(0.1), SHADOW_DISTANCE_M);
    for (i, s) in out.iter_mut().enumerate() {
        let t = (i + 1) as f32 / CASCADES as f32;
        let log = n * (f / n).powf(t);
        let lin = n + (f - n) * t;
        *s = LAMBDA * log + (1.0 - LAMBDA) * lin;
    }
    out
}

/// Macierze kaskad dla zadanej kamery i kierunku **do** słońca.
///
/// `view_proj` jest macierzą kamery względem oka (patrz [`crate::CameraState::view_proj_relative`]),
/// więc wynik też jest względem oka — i tak trafia do shadera, który operuje na pozycjach
/// chunków względem kamery.
#[must_use]
pub fn cascades(view_proj: &Mat4, sun_dir: Vec3, near_m: f32) -> [Cascade; CASCADES] {
    let granice = splits(near_m);
    let inv = view_proj.inverse();
    let mut out = [Cascade {
        view_proj: Mat4::IDENTITY,
        far_m: 0.0,
        texel_m: 1.0,
    }; CASCADES];

    let mut bliska = near_m;
    for (i, c) in out.iter_mut().enumerate() {
        let daleka = granice[i];
        let (srodek, promien) = sfera_wycinka(&inv, bliska, daleka);
        *c = Cascade {
            view_proj: macierz_kaskady(srodek, promien, sun_dir),
            far_m: daleka,
            texel_m: 2.0 * promien / SHADOW_MAP_SIZE as f32,
        };
        bliska = daleka;
    }
    out
}

/// Sfera opisana na wycinku frustum między dwiema odległościami.
///
/// Rogi odtwarzamy z odwróconej macierzy kamery zamiast liczyć je z FOV i proporcji:
/// macierz jest jedynym miejscem, w którym te wartości naprawdę są, a druga ich kopia
/// rozjechałaby się przy pierwszej zmianie rzutowania.
fn sfera_wycinka(inv_view_proj: &Mat4, bliska_m: f32, daleka_m: f32) -> (Vec3, f32) {
    let mut rogi = [Vec3::ZERO; 8];
    let mut i = 0;
    for z in [0.0f32, 1.0] {
        for y in [-1.0f32, 1.0] {
            for x in [-1.0f32, 1.0] {
                let p = *inv_view_proj * glam::Vec4::new(x, y, z, 1.0);
                rogi[i] = p.xyz() / p.w;
                i += 1;
            }
        }
    }
    // Rogi bliskiej i dalekiej płaszczyzny frustum; wycinek to interpolacja między nimi
    // po **odległości**, a nie po współrzędnej NDC (ta jest nieliniowa).
    let (bliskie, dalekie) = rogi.split_at(4);
    let mut punkty = [Vec3::ZERO; 8];
    for k in 0..4 {
        let kierunek = (dalekie[k] - bliskie[k]).normalize_or_zero();
        let dlugosc = (dalekie[k] - bliskie[k]).length().max(1.0);
        punkty[k] = bliskie[k] + kierunek * (bliska_m.min(dlugosc));
        punkty[k + 4] = bliskie[k] + kierunek * (daleka_m.min(dlugosc));
    }
    let srodek = punkty.iter().fold(Vec3::ZERO, |a, p| a + *p) / 8.0;
    let promien = punkty
        .iter()
        .map(|p| p.distance(srodek))
        .fold(0.0f32, f32::max);
    (srodek, promien.max(1.0))
}

/// Macierz ortograficzna kaskady z zatrzaśnięciem środka do siatki texeli.
fn macierz_kaskady(srodek: Vec3, promien: f32, sun_dir: Vec3) -> Mat4 {
    let kierunek = sun_dir.normalize_or_zero();
    // Słońce dokładnie w zenicie zostawia `look_at` bez osi odniesienia — wtedy bierzemy
    // oś X zamiast Z. Przypadek jest realny: południe na równiku.
    let up = if kierunek.z.abs() > 0.99 {
        Vec3::X
    } else {
        Vec3::Z
    };
    let oko = srodek + kierunek * (promien + DEPTH_MARGIN_M);
    let view = glam::camera::rh::view::look_at_mat4(oko, srodek, up);

    // Zatrzask: środek w przestrzeni światła przesuwamy do wielokrotności texela.
    let texel = 2.0 * promien / SHADOW_MAP_SIZE as f32;
    let srodek_sw = view.transform_point3(srodek);
    let przesun = Vec3::new(
        (srodek_sw.x / texel).round() * texel - srodek_sw.x,
        (srodek_sw.y / texel).round() * texel - srodek_sw.y,
        0.0,
    );

    let proj = glam::camera::rh::proj::directx::orthographic(
        -promien + przesun.x,
        promien + przesun.x,
        -promien + przesun.y,
        promien + przesun.y,
        0.0,
        2.0 * promien + 2.0 * DEPTH_MARGIN_M,
    );
    proj * view
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CameraState;

    fn kamera(yaw: f32) -> Mat4 {
        let mut c = CameraState::default();
        c.orbit_rotate(yaw, 0.0);
        c.view_proj_relative(16.0 / 9.0)
    }

    #[test]
    fn podzialy_rosna_i_koncza_sie_na_zasiegu_cieni() {
        let s = splits(0.5);
        assert!(
            s.windows(2).all(|w| w[1] > w[0]),
            "podziały nie rosną: {s:?}"
        );
        assert!((s[CASCADES - 1] - SHADOW_DISTANCE_M).abs() < 0.5);
        // Pierwsza kaskada ma obsłużyć okolicę kamery, a nie połowę zasięgu.
        assert!(
            s[0] < SHADOW_DISTANCE_M / 8.0,
            "pierwsza kaskada za duża: {}",
            s[0]
        );
    }

    #[test]
    fn rozmiar_kaskady_nie_zalezy_od_obrotu_kamery() {
        // To jest cały powód, dla którego kaskada opisana jest na sferze, a nie na AABB:
        // przy zmiennym rozmiarze mapa cienia co klatkę ma inną skalę i krawędzie pełzają.
        let sun = Vec3::new(0.3, 0.2, 0.9);
        let odniesienie = cascades(&kamera(0.0), sun, 0.5);
        for krok in 1..16 {
            let k = cascades(&kamera(krok as f32 * 0.4), sun, 0.5);
            for i in 0..CASCADES {
                let wzg = (k[i].texel_m - odniesienie[i].texel_m).abs() / odniesienie[i].texel_m;
                assert!(
                    wzg < 0.02,
                    "kaskada {i} zmieniła skalę o {:.1} % przy obrocie",
                    wzg * 100.0
                );
            }
        }
    }

    #[test]
    fn srodek_kaskady_jest_zatrzasniety_do_texela() {
        // Przesunięcie kamery o ułamek texela nie ma prawa przesunąć rastra cienia —
        // inaczej stojąca scena drga przy każdym ruchu myszy.
        let sun = Vec3::new(0.3, 0.2, 0.9);
        let a = cascades(&kamera(0.0), sun, 0.5);
        let b = cascades(&kamera(0.000_01), sun, 0.5);
        for i in 0..CASCADES {
            let ruch = (a[i].view_proj.w_axis - b[i].view_proj.w_axis).length();
            assert!(ruch < 1e-4, "kaskada {i} przesunęła się o {ruch}");
        }
    }

    #[test]
    fn slonce_w_zenicie_daje_poprawna_macierz() {
        let k = cascades(&kamera(0.0), Vec3::Z, 0.5);
        for c in k {
            assert!(
                c.view_proj.is_finite(),
                "macierz kaskady nie jest skończona"
            );
        }
    }
}
