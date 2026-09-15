//! Nakładki danych na terenie (M2 §5.7 „Nakładka UI", WP16; PRD §14.2).
//!
//! M2 dostarcza **jedną** nakładkę — wartość gruntu — bo jest jedyną fazą, która ma
//! czym ją wypełnić. Mechanizm jest jednak wspólny i wchodzi do `data/ui/overlays.ron`:
//! każda następna faza dopisuje tam swój wpis, zamiast rysować mapę po swojemu
//! (kontrakt `TerrainOverlay` z M1 §6.1 mówi dokładnie to samo od drugiej strony).
//!
//! Paleta jest **piecewise po progach**, nie liniowa po wartości. Powód jest praktyczny:
//! rozkład wartości gruntu ma długi ogon — mediana bywa rzędu 450 zł/m², a biurowiec
//! w śródmieściu kilku tysięcy — więc paleta liniowa pokazywałaby całe miasto w jednym
//! odcieniu i trzy piksele na czerwono. Progi z danych rozciągają kontrast tam, gdzie
//! naprawdę leżą działki.

use super::CityData;
use magnat_core::Money;
use magnat_spatial::Vec2;
use serde::Deserialize;

pub const OVERLAY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
pub struct OverlaySpec {
    pub key: String,
    /// Klucz do `data/locale/`, kiedy `engine/ui` powstanie w M3.
    pub loc_key: String,
    pub unit: String,
    /// Próg (w jednostkach nakładki) i barwa RGBA nad nim; rosnąco.
    pub stops: Vec<(i64, (u8, u8, u8, u8))>,
}

impl OverlaySpec {
    /// Indeks palety dla wartości. Interpolacja **wewnątrz** przedziału progów, więc
    /// sąsiednie działki o zbliżonej cenie nie skaczą o pół palety.
    #[must_use]
    pub fn index_of(&self, v: i64) -> u8 {
        let n = self.stops.len();
        if n < 2 {
            return 0;
        }
        let ostatni = n - 1;
        if v <= self.stops[0].0 {
            return 0;
        }
        for k in 0..ostatni {
            let (lo, hi) = (self.stops[k].0, self.stops[k + 1].0);
            if v < hi {
                let t = if hi > lo {
                    (v - lo) as f32 / (hi - lo) as f32
                } else {
                    0.0
                };
                return (((k as f32 + t) * 255.0 / ostatni as f32).round()).clamp(0.0, 255.0) as u8;
            }
        }
        255
    }

    /// Paleta 256-barwna — kontrakt `TerrainOverlay` z M1.
    #[must_use]
    pub fn palette(&self) -> [[u8; 4]; 256] {
        let mut p = [[0u8, 0, 0, 0]; 256];
        let n = self.stops.len();
        if n == 0 {
            return p;
        }
        if n == 1 {
            let c = self.stops[0].1;
            return [[c.0, c.1, c.2, c.3]; 256];
        }
        let ostatni = (n - 1) as f32;
        for (i, v) in p.iter_mut().enumerate() {
            let pos = i as f32 * ostatni / 255.0;
            let k = (pos.floor() as usize).min(n - 2);
            let t = pos - k as f32;
            let a = self.stops[k].1;
            let b = self.stops[k + 1].1;
            let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t) as u8;
            *v = [
                mix(a.0, b.0),
                mix(a.1, b.1),
                mix(a.2, b.2),
                mix(a.3, b.3),
            ];
        }
        p
    }

    /// Pozycje progów w palecie — wejście legendy. Para (indeks, wartość).
    #[must_use]
    pub fn legend(&self) -> Vec<(u8, i64)> {
        let ostatni = self.stops.len().saturating_sub(1).max(1) as f32;
        self.stops
            .iter()
            .enumerate()
            .map(|(k, (v, _))| ((k as f32 * 255.0 / ostatni) as u8, *v))
            .collect()
    }
}

#[derive(Deserialize)]
struct OverlayFile {
    schema_version: u32,
    overlays: Vec<OverlaySpec>,
}

#[derive(Debug)]
pub enum OverlayError {
    Io(std::io::Error),
    Ron(String),
    Schema(u32),
    Missing(String),
}

impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverlayError::Io(e) => write!(f, "data/ui/overlays.ron: {e}"),
            OverlayError::Ron(s) => write!(f, "data/ui/overlays.ron: {s}"),
            OverlayError::Schema(v) => write!(
                f,
                "data/ui/overlays.ron: schema_version {v}, oczekiwano {OVERLAY_SCHEMA_VERSION}"
            ),
            OverlayError::Missing(k) => write!(f, "brak nakładki `{k}` w data/ui/overlays.ron"),
        }
    }
}

impl std::error::Error for OverlayError {}

#[derive(Clone, Debug)]
pub struct OverlayTable {
    pub overlays: Vec<OverlaySpec>,
}

impl OverlayTable {
    pub fn load() -> Result<OverlayTable, OverlayError> {
        let path = crate::assets::data_path("ui/overlays.ron");
        let txt = std::fs::read_to_string(&path).map_err(OverlayError::Io)?;
        let f: OverlayFile =
            ron::from_str(&txt).map_err(|e| OverlayError::Ron(e.to_string()))?;
        if f.schema_version != OVERLAY_SCHEMA_VERSION {
            return Err(OverlayError::Schema(f.schema_version));
        }
        Ok(OverlayTable {
            overlays: f.overlays,
        })
    }

    pub fn get(&self, key: &str) -> Result<&OverlaySpec, OverlayError> {
        self.overlays
            .iter()
            .find(|o| o.key == key)
            .ok_or_else(|| OverlayError::Missing(key.to_string()))
    }
}

/// Bok komórki rastra nakładki. 16 m to rozdzielczość, przy której widać granicę między
/// pierzeją a podwórzem, a raster metropolii ma milion komórek, nie szesnaście milionów.
pub const OVERLAY_CELL_M: u16 = 16;

/// Raster wartości gruntu: dla każdej komórki wartość działki, na której leży jej środek.
///
/// Bez interpolacji z rozmysłem — wartość gruntu jest **stała w obrębie działki**
/// i granica między nią a sąsiadką jest informacją, a nie artefaktem. Rozmycie
/// pokazywałoby gradient tam, gdzie go nie ma.
#[must_use]
pub fn land_value_raster(city: &CityData, spec: &OverlaySpec) -> (u32, f32, Vec<u8>) {
    let bok = f32::from(OVERLAY_CELL_M);
    let dim = ((city.plan.map_size_m() as f32 / bok).ceil() as usize).max(1);
    let mut v = vec![0u8; dim * dim];
    let geom = &city.roads.geom;
    for p in &city.parcels.parcels {
        let poly = geom.get(p.poly);
        if poly.len() < 3 {
            continue;
        }
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for q in poly {
            lo = Vec2::new(lo.x.min(q.x), lo.y.min(q.y));
            hi = Vec2::new(hi.x.max(q.x), hi.y.max(q.y));
        }
        let idx = spec.index_of(p.land_value_per_m2.0);
        let (x0, x1) = (
            ((lo.x / bok).floor().max(0.0)) as usize,
            (((hi.x / bok).ceil()) as usize).min(dim - 1),
        );
        let (y0, y1) = (
            ((lo.y / bok).floor().max(0.0)) as usize,
            (((hi.y / bok).ceil()) as usize).min(dim - 1),
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = Vec2::new((x as f32 + 0.5) * bok, (y as f32 + 0.5) * bok);
                if super::poly::contains(poly, c) {
                    v[y * dim + x] = idx;
                }
            }
        }
    }
    (dim as u32, bok, v)
}

/// Wartość w złotych do legendy — jedyne miejsce, w którym grosze zamieniają się
/// na liczbę pokazywaną człowiekowi.
#[must_use]
pub fn zlote(m: Money) -> f64 {
    m.0 as f64 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> OverlaySpec {
        OverlayTable::load()
            .expect("data/ui/overlays.ron")
            .get("land_value")
            .expect("nakładka wartości gruntu")
            .clone()
    }

    #[test]
    fn paleta_jest_monotoniczna_i_pelna() {
        let s = spec();
        let p = s.palette();
        assert_eq!(p.len(), 256);
        // Jasność rośnie monotonicznie w skali szarości — warunek czytelności przy
        // deuteranopii i na wydruku (M2 §5.7).
        let luma = |c: [u8; 4]| {
            0.299 * f32::from(c[0]) + 0.587 * f32::from(c[1]) + 0.114 * f32::from(c[2])
        };
        let spadki = p.windows(2).filter(|w| luma(w[1]) + 1.0 < luma(w[0])).count();
        assert!(spadki <= 8, "paleta ma {spadki} spadków jasności");
    }

    /// Reguła jasności obowiązuje **każdą** nakładkę, nie tylko tę pierwszą.
    /// Nakładka dopisana przez kolejną fazę z paletą turbo albo tęczową czytałaby się
    /// źle na wydruku i przy deuteranopii, a zauważyłby to dopiero gracz.
    #[test]
    fn kazda_nakladka_ma_palete_monotoniczna_w_jasnosci() {
        let t = OverlayTable::load().expect("data/ui/overlays.ron");
        let luma = |c: [u8; 4]| {
            0.299 * f32::from(c[0]) + 0.587 * f32::from(c[1]) + 0.114 * f32::from(c[2])
        };
        assert!(t.overlays.len() >= 6, "nakładki ruchu z M4d zniknęły z danych");
        for s in &t.overlays {
            let k = &s.key;
            let p = s.palette();
            let spadki = p.windows(2).filter(|w| luma(w[1]) + 1.0 < luma(w[0])).count();
            assert!(spadki <= 8, "nakładka {k}: paleta ma {spadki} spadków jasności");
            assert!(!s.unit.is_empty(), "nakładka {k} bez jednostki w legendzie");
            assert!(
                s.loc_key.starts_with("overlay."),
                "nakładka {k}: klucz lokalizacji poza przestrzenią `overlay.`"
            );
        }
    }

    #[test]
    fn indeks_rosnie_z_wartoscia_i_nie_wychodzi_poza_zakres() {
        let s = spec();
        let mut poprzedni = 0u8;
        for v in [0, 5_000, 25_000, 60_000, 150_000, 400_000, 900_000, i64::MAX / 2] {
            let i = s.index_of(v);
            assert!(i >= poprzedni, "indeks spadł przy {v}");
            poprzedni = i;
        }
        assert_eq!(s.index_of(i64::MAX / 2), 255);
        assert_eq!(s.index_of(-1), 0);
    }
}
