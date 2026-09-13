//! Szum proceduralny generatora (M1 §5.6, przebiegi P1–P3).
//!
//! **Determinizm.** Szum jest czystą funkcją `(seed, strumień, pozycja)` i używa wyłącznie
//! `+ − × ÷` na `f32` oraz całkowitoliczbowego mieszania bitów. Żadnej funkcji przestępnej,
//! żadnego `mul_add` (00 §K-6), żadnego stanu globalnego. To wystarcza, żeby wynik był
//! identyczny na każdej platformie: IEEE-754 wymaga poprawnego zaokrąglenia czterech działań
//! podstawowych, a Rust nie stosuje fast-math ani samoczynnej kontrakcji do FMA.
//!
//! **Dlaczego nie `rng()` per punkt siatki.** Kontrakt 00 §3.1 żąda, żeby losowość pochodziła
//! z `rng(seed, stream, index, tick)`. Wywołanie go dla każdego rogu każdej oktawy to
//! ~537 mln wywołań na mapę 16 km — kilkadziesiąt sekund za nic. Zamiast tego `rng()` wyprowadza
//! **klucz pola** raz (`NoiseField::new`), a punkty siatki liczy tani mieszalnik bitowy.
//! Kontrakt jest zachowany co do istoty: brak stanu globalnego i pełna czystość funkcji.

use magnat_core::{rng, StreamId, Tick, NO_ENTITY};

/// Pole szumu o ustalonym kluczu. Tworzy się je raz na przebieg, nie na próbkę.
#[derive(Clone, Copy, Debug)]
pub struct NoiseField {
    key: u64,
}

impl NoiseField {
    /// Klucz wyprowadzony z ziarna świata i strumienia przebiegu (00 §3.1).
    /// `salt` rozdziela pola w obrębie jednego przebiegu (np. oś X i Y domain warpingu),
    /// bez zajmowania kolejnego `StreamId`.
    #[must_use]
    pub fn new(world_seed: u64, stream: StreamId, salt: u32) -> NoiseField {
        let mut r = rng(world_seed, stream, NO_ENTITY, Tick(u64::from(salt)));
        NoiseField { key: r.next_u64() }
    }

    /// Mieszalnik punktu siatki. Trzy mnożenia i trzy przesunięcia — dość, żeby sąsiednie
    /// punkty nie były skorelowane, i tanio, żeby wytrzymać setki milionów wywołań.
    #[inline]
    fn hash2(&self, x: i32, y: i32) -> u32 {
        let mut h = self
            .key
            .wrapping_add((x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .wrapping_add((y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
        h ^= h >> 29;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 32;
        (h >> 32) as u32
    }

    /// Iloczyn skalarny gradientu punktu siatki z wektorem do próbki.
    ///
    /// **Szum gradientowy, nie wartościowy.** Szum wartościowy interpoluje liczby przypisane
    /// rogom komórki, przez co jego ekstrema siedzą **w** rogach — a rogi leżą na regularnej
    /// kratownicy. W terenie objawia się to grzbietami układającymi się w poziome i pionowe
    /// linie; widać to gołym okiem na podglądzie PNG i nie da się tego ukryć strojeniem.
    /// Szum gradientowy ma w rogach wartość zero, a ekstrema **między** nimi — kratownica znika.
    #[inline]
    fn grad_dot(&self, ix: i32, iy: i32, dx: f32, dy: f32) -> f32 {
        let (gx, gy) = GRADIENTS[(self.hash2(ix, iy) & 7) as usize];
        gx * dx + gy * dy
    }

    /// Szum gradientowy z interpolacją kwintyczną (`6t⁵ − 15t⁴ + 10t³`).
    /// Kwintyczna, a nie kosinusowa: ma zerową pierwszą i drugą pochodną na brzegach komórki,
    /// więc nie zostawia widocznego szwu w cieniowaniu — i nie potrzebuje `cos`.
    #[must_use]
    pub fn noise2(&self, x: f32, y: f32) -> f32 {
        let x0 = x.floor();
        let y0 = y.floor();
        let (ix, iy) = (x0 as i32, y0 as i32);
        let fx = x - x0;
        let fy = y - y0;
        let ux = quintic(fx);
        let uy = quintic(fy);

        let n00 = self.grad_dot(ix, iy, fx, fy);
        let n10 = self.grad_dot(ix + 1, iy, fx - 1.0, fy);
        let n01 = self.grad_dot(ix, iy + 1, fx, fy - 1.0);
        let n11 = self.grad_dot(ix + 1, iy + 1, fx - 1.0, fy - 1.0);

        let a = n00 + (n10 - n00) * ux;
        let b = n01 + (n11 - n01) * ux;
        // Szum gradientowy 2D o gradientach jednostkowych sięga ±√2/2; skalujemy do ±1.
        (a + (b - a) * uy) * SQRT_2
    }
}

/// Osiem kierunków gradientu: cztery osiowe i cztery po przekątnej. Stały zbiór zamiast
/// losowego kąta, bo kąt wymagałby `sin`/`cos`, a te są zakazane w kodzie symulacji (00 §K-6).
const S: f32 = 0.707_106_77;
const SQRT_2: f32 = 1.414_213_6;
/// Wzmocnienie sumy oktaw przed zaciśnięciem do `[-1, 1]`.
///
/// Suma oktaw szumu gradientowego ma odchylenie standardowe rzędu 0,25 i sięga ±1 tylko
/// teoretycznie — bez wzmocnienia każdy konsument dostawałby sygnał wypełniający ćwiartkę
/// deklarowanego zakresu i musiałby to nadrabiać własnym mnożnikiem. Lepiej raz, tutaj,
/// niż w każdym przebiegu z osobna.
pub const FBM_GAIN: f32 = 2.2;

/// Wzmocnienie wagi kolejnej oktawy w ridged multifractal. Powyżej ~2,2 grzbiety
/// zaczynają się rwać na plamy, poniżej ~1,5 znika ich rozgałęzienie.
const RIDGE_GAIN: f32 = 2.0;

const GRADIENTS: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (S, S),
    (-S, S),
    (S, -S),
    (-S, -S),
];

#[inline]
fn quintic(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Parametry szumu wielooktawowego. Trzymane w strukturze, bo są strojone per region
/// i przekazywanie ich pięcioma argumentami zamienia każde wywołanie w zagadkę.
#[derive(Clone, Copy, Debug)]
pub struct FbmSpec {
    pub octaves: u8,
    /// Częstotliwość pierwszej oktawy w cyklach na metr.
    pub frequency: f32,
    /// Mnożnik częstotliwości między oktawami.
    pub lacunarity: f32,
    /// Mnożnik amplitudy między oktawami.
    pub persistence: f32,
}

impl FbmSpec {
    /// Domyślne 8 oktaw o długości fali dobranej do mapy kilkukilometrowej.
    #[must_use]
    pub const fn new(octaves: u8, wavelength_m: f32) -> FbmSpec {
        FbmSpec {
            octaves,
            frequency: 1.0 / wavelength_m,
            lacunarity: 2.0,
            persistence: 0.5,
        }
    }
}

/// Klasyczny fBm, znormalizowany do `[-1, 1]`.
///
/// Sumowanie idzie od najniższej oktawy do najwyższej — kolejność jest częścią kontraktu
/// determinizmu, bo dodawanie `f32` nie jest łączne (00 §2, ostatni punkt).
#[must_use]
pub fn fbm(field: &NoiseField, x: f32, y: f32, s: FbmSpec) -> f32 {
    let mut freq = s.frequency;
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    for _ in 0..s.octaves {
        sum += field.noise2(x * freq, y * freq) * amp;
        norm += amp;
        freq *= s.lacunarity;
        amp *= s.persistence;
    }
    if norm > 0.0 {
        (sum / norm * FBM_GAIN).clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Ridged multifractal — grzbiety zamiast pagórków. Podstawa regionu górskiego.
///
/// `1 − |n|` odwraca doliny w granie, potęga 2 wyostrza je, a ważenie kolejnej oktawy
/// poprzednią sprawia, że szczegół pojawia się na graniach, a nie w dolinach — bez tego
/// góry wyglądają jak pomięty papier.
#[must_use]
pub fn ridged(field: &NoiseField, x: f32, y: f32, s: FbmSpec) -> f32 {
    let mut freq = s.frequency;
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    let mut weight = 1.0f32;
    for _ in 0..s.octaves {
        let n = (field.noise2(x * freq, y * freq) * FBM_GAIN).clamp(-1.0, 1.0);
        // Sygnał grzbietu przed wyostrzeniem. Waga **następnej** oktawy liczy się z niego,
        // a nie z wartości już podniesionej do kwadratu i przemnożonej przez wagę —
        // inaczej waga zapada się po pierwszej oktawie i z multifraktala zostaje
        // jedna gładka fala. To był realny błąd wyłapany na podglądzie PNG.
        let signal = 1.0 - if n < 0.0 { -n } else { n };
        sum += signal * signal * weight * amp;
        norm += amp;
        weight = (signal * RIDGE_GAIN).clamp(0.0, 1.0);
        freq *= s.lacunarity;
        amp *= s.persistence;
    }
    if norm > 0.0 {
        sum / norm * 2.0 - 1.0
    } else {
        0.0
    }
}

/// Zniekształcenie dziedziny: przesunięcie punktu próbkowania o wektor z dwóch pól szumu.
///
/// To jest jedyny zabieg, który odróżnia teren „proceduralny" od terenu wyglądającego jak
/// wynik procesów geologicznych — bez niego doliny są zbyt regularne, a grzbiety zbyt proste.
#[must_use]
pub fn domain_warp(
    wx: &NoiseField,
    wy: &NoiseField,
    x: f32,
    y: f32,
    strength_m: f32,
    s: FbmSpec,
) -> (f32, f32) {
    (
        x + fbm(wx, x, y, s) * strength_m,
        y + fbm(wy, x, y, s) * strength_m,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pole() -> NoiseField {
        NoiseField::new(0xC0FF_EE, StreamId::WorldHeightBase, 0)
    }

    #[test]
    fn szum_jest_czysta_funkcja_pozycji() {
        let f = pole();
        assert_eq!(f.noise2(3.25, -7.5), f.noise2(3.25, -7.5));
        // Inne ziarno = inne pole.
        let g = NoiseField::new(1, StreamId::WorldHeightBase, 0);
        assert_ne!(f.noise2(3.25, -7.5), g.noise2(3.25, -7.5));
        // Inna sól = inne pole, przy tym samym ziarnie i strumieniu.
        let h = NoiseField::new(0xC0FF_EE, StreamId::WorldHeightBase, 1);
        assert_ne!(f.noise2(3.25, -7.5), h.noise2(3.25, -7.5));
    }

    #[test]
    fn szum_trzyma_sie_zakresu_i_jest_ciagly() {
        let f = pole();
        let mut prev = f.noise2(0.0, 0.37);
        for i in 0..2000 {
            let x = i as f32 * 0.01;
            let v = f.noise2(x, 0.37);
            assert!((-1.0..=1.0).contains(&v), "v = {v} przy x = {x}");
            // Krok 0,01 komórki nie może dać skoku — to wychwyciłoby złą interpolację.
            assert!((v - prev).abs() < 0.2, "skok {prev} → {v} przy x = {x}");
            prev = v;
        }
    }

    #[test]
    fn w_punktach_siatki_szum_gradientowy_ma_zero() {
        // To jest podpis szumu gradientowego i zarazem powód, dla którego go wybraliśmy:
        // ekstrema leżą między punktami siatki, a nie na regularnej kratownicy.
        let f = pole();
        for (x, y) in [(0, 0), (5, -3), (-11, 7), (1000, -1000)] {
            assert_eq!(f.noise2(x as f32, y as f32), 0.0, "({x}, {y})");
        }
    }

    #[test]
    fn rozklad_nie_faworyzuje_osi() {
        // Kratownica objawiłaby się tym, że wartości wzdłuż osi są systematycznie inne niż
        // wzdłuż przekątnej. Porównujemy średnią wartość bezwzględną na obu kierunkach.
        let f = pole();
        let (mut osiowo, mut ukosem) = (0.0f64, 0.0f64);
        for i in 1..4000 {
            let t = i as f32 * 0.25;
            osiowo += f64::from(f.noise2(t, 0.5).abs());
            ukosem += f64::from(f.noise2(t * S, t * S + 0.5).abs());
        }
        let stosunek = osiowo / ukosem;
        assert!(
            (0.8..1.25).contains(&stosunek),
            "anizotropia: oś/ukos = {stosunek}"
        );
    }

    #[test]
    fn fbm_i_ridged_mieszcza_sie_w_zakresie() {
        let f = pole();
        let s = FbmSpec::new(8, 2048.0);
        for i in 0..500 {
            let (x, y) = (i as f32 * 13.7, i as f32 * -7.1);
            let a = fbm(&f, x, y, s);
            let b = ridged(&f, x, y, s);
            assert!((-1.0..=1.0).contains(&a), "fbm = {a}");
            assert!((-1.05..=1.05).contains(&b), "ridged = {b}");
        }
    }

    #[test]
    fn ridged_jest_dokladnie_odwroconym_i_wyostrzonym_szumem() {
        // Dla jednej oktawy wzór redukuje się do 2·(1−|n|)² − 1. Test sprawdza formułę
        // wprost, zamiast zgadywać kształt rozkładu wielooktawowego.
        let f = pole();
        let s = FbmSpec::new(1, 512.0);
        for i in 0..200 {
            let (x, y) = (i as f32 * 23.0, i as f32 * 9.5);
            let n = (f.noise2(x * s.frequency, y * s.frequency) * FBM_GAIN).clamp(-1.0, 1.0);
            let r = 1.0 - n.abs();
            assert_eq!(ridged(&f, x, y, s), 2.0 * (r * r) - 1.0, "i = {i}");
        }
    }

    #[test]
    fn ridged_ma_ostre_granie() {
        // Grzbiety to wąskie maksima: w próbce muszą się pojawić wartości blisko górnego
        // krańca zakresu, mimo że większość terenu leży nisko (doliny dominują powierzchniowo).
        let f = pole();
        let s = FbmSpec::new(6, 1024.0);
        let mut max = -2.0f32;
        for i in 0..4000 {
            let (x, y) = ((i % 64) as f32 * 37.0, (i / 64) as f32 * 41.0);
            max = max.max(ridged(&f, x, y, s));
        }
        assert!(max > 0.6, "brak wyraźnych grani, maksimum = {max}");
    }
}
