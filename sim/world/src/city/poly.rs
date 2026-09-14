//! Geometria wielokątów generatora miasta (M2 §5.4).
//!
//! Wszystko na `f32` w metrach, w tym samym układzie co `RoadNetwork`. Ryzyko R2 fazy
//! każe trzymać geometrię parcel w stałoprzecinkowym układzie milimetrowym — patrz
//! korekta C4 w dokumencie podfazy: podział pasowy **konstruuje** parcele rozłączne
//! (każde cięcie półpłaszczyzną dzieli wielokąt na dwie części o wspólnej krawędzi),
//! więc nie ma tu sumowania ani przecinania wielokątów, na którym float by poległ.
//! Przy mapie 16 km `f32` ma rozdzielczość ~2 mm wobec tolerancji 1 m² z testu T5.

use magnat_spatial::Vec2;

/// Pole ze znakiem (wzór Gaussa). Dodatnie dla obchodu przeciwnego do wskazówek zegara.
#[must_use]
pub fn signed_area(pts: &[Vec2]) -> f64 {
    let mut a = 0.0f64;
    for i in 0..pts.len() {
        let p = pts[i];
        let q = pts[(i + 1) % pts.len()];
        a += f64::from(p.x) * f64::from(q.y) - f64::from(q.x) * f64::from(p.y);
    }
    a * 0.5
}

/// Środek ciężkości wielokąta. Dla zdegenerowanego (pole ≈ 0) — średnia wierzchołków,
/// bo wzór na centroid dzieli przez pole i tam by się wywrócił.
#[must_use]
pub fn centroid(pts: &[Vec2]) -> Vec2 {
    let a = signed_area(pts);
    if a.abs() < 1e-3 {
        let n = pts.len().max(1) as f32;
        return pts.iter().fold(Vec2::ZERO, |s, p| s + *p) / n;
    }
    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    for i in 0..pts.len() {
        let p = pts[i];
        let q = pts[(i + 1) % pts.len()];
        let w = f64::from(p.x) * f64::from(q.y) - f64::from(q.x) * f64::from(p.y);
        cx += (f64::from(p.x) + f64::from(q.x)) * w;
        cy += (f64::from(p.y) + f64::from(q.y)) * w;
    }
    Vec2::new((cx / (6.0 * a)) as f32, (cy / (6.0 * a)) as f32)
}

/// Test przynależności punktu — promień poziomy, reguła parzystości przecięć.
#[must_use]
pub fn contains(pts: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let n = pts.len();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        if (a.y > p.y) != (b.y > p.y) {
            let t = (p.y - a.y) / (b.y - a.y);
            if p.x < a.x + t * (b.x - a.x) {
                inside = !inside;
            }
        }
    }
    inside
}

/// Przycięcie wielokąta półpłaszczyzną `dot(p − origin, normal) ≥ 0` (Sutherland–Hodgman).
///
/// ponytail: dla wielokąta niewypukłego wynik bywa jednym wielokątem z „mostkiem"
/// o zerowej szerokości zamiast dwóch rozłącznych części. Pole i rozłączność wobec
/// dopełnienia zostają poprawne, więc test T5 (nakładki) tego nie widzi; jeśli kiedyś
/// zacznie przeszkadzać wizualnie, wyjściem jest rozbicie wyniku na składowe po
/// wykryciu powtórzonego wierzchołka.
#[must_use]
pub fn clip_halfplane(pts: &[Vec2], origin: Vec2, normal: Vec2) -> Vec<Vec2> {
    let n = pts.len();
    let mut out: Vec<Vec2> = Vec::with_capacity(n + 2);
    let side = |p: Vec2| (p - origin).dot(normal);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let (sa, sb) = (side(a), side(b));
        if sa >= 0.0 {
            out.push(a);
        }
        if (sa >= 0.0) != (sb >= 0.0) {
            let t = sa / (sa - sb);
            out.push(a + (b - a) * t);
        }
    }
    dedup_ring(&mut out);
    out
}

/// Usunięcie wierzchołków powtórzonych (także pary pierwszy–ostatni). Bez tego cięcie
/// przez wierzchołek zostawia krawędź o zerowej długości, a taka wywraca normalną
/// w podziale pasowym.
fn dedup_ring(v: &mut Vec<Vec2>) {
    v.dedup_by(|a, b| (*a - *b).length_squared() < 1e-6);
    while v.len() > 1 && (v[0] - v[v.len() - 1]).length_squared() < 1e-6 {
        v.pop();
    }
}

/// Otoczka wypukła (monotone chain Andrew). Wynik przeciwnie do wskazówek zegara.
#[must_use]
pub fn convex_hull(pts: &[Vec2]) -> Vec<Vec2> {
    let mut p: Vec<Vec2> = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| (*a - *b).length_squared() < 1e-8);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| {
        f64::from(a.x - o.x) * f64::from(b.y - o.y) - f64::from(a.y - o.y) * f64::from(b.x - o.x)
    };
    let mut dolna: Vec<Vec2> = Vec::with_capacity(p.len());
    for &q in &p {
        while dolna.len() >= 2 && cross(dolna[dolna.len() - 2], dolna[dolna.len() - 1], q) <= 0.0 {
            dolna.pop();
        }
        dolna.push(q);
    }
    let mut gorna: Vec<Vec2> = Vec::with_capacity(p.len());
    for &q in p.iter().rev() {
        while gorna.len() >= 2 && cross(gorna[gorna.len() - 2], gorna[gorna.len() - 1], q) <= 0.0 {
            gorna.pop();
        }
        gorna.push(q);
    }
    dolna.pop();
    gorna.pop();
    dolna.extend(gorna);
    dolna
}

/// Prostokąt otaczający o minimalnym polu. `axis` jest kierunkiem **dłuższego** boku.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Obb {
    pub center: Vec2,
    pub axis: Vec2,
    pub half_long: f32,
    pub half_short: f32,
}

impl Obb {
    #[must_use]
    pub fn area(&self) -> f32 {
        4.0 * self.half_long * self.half_short
    }
}

/// OBB metodą obracających się suwmiarek: minimum leży zawsze na kierunku któregoś
/// boku otoczki wypukłej, więc wystarczy przejrzeć boki. O(h²) przy h wierzchołkach
/// otoczki — dla kwartału o kilkunastu bokach to nic.
#[must_use]
pub fn min_area_obb(pts: &[Vec2]) -> Obb {
    let zerowy = |p: &[Vec2]| Obb {
        center: centroid(p),
        axis: Vec2::new(1.0, 0.0),
        half_long: 0.0,
        half_short: 0.0,
    };
    let hull = convex_hull(pts);
    if hull.len() < 3 {
        return zerowy(pts);
    }
    let mut best: Option<Obb> = None;
    for i in 0..hull.len() {
        let d = (hull[(i + 1) % hull.len()] - hull[i]).normalize_or_zero();
        if d.length_squared() < 0.5 {
            continue;
        }
        let n = Vec2::new(-d.y, d.x);
        let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for &p in &hull {
            let (u, v) = (p.dot(d), p.dot(n));
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(v);
            v1 = v1.max(v);
        }
        let (su, sv) = (u1 - u0, v1 - v0);
        let c = d * ((u0 + u1) * 0.5) + n * ((v0 + v1) * 0.5);
        let obb = if su >= sv {
            Obb {
                center: c,
                axis: d,
                half_long: su * 0.5,
                half_short: sv * 0.5,
            }
        } else {
            Obb {
                center: c,
                axis: n,
                half_long: sv * 0.5,
                half_short: su * 0.5,
            }
        };
        if best.is_none_or(|b| obb.area() < b.area()) {
            best = Some(obb);
        }
    }
    best.unwrap_or_else(|| zerowy(pts))
}

/// Najbliższy punkt odcinka `a..b` i parametr wzdłuż niego.
#[must_use]
pub fn closest_on_segment(a: Vec2, b: Vec2, p: Vec2) -> (Vec2, f32) {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < 1e-6 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(d) / len2).clamp(0.0, 1.0);
    (a + d * t, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kwadrat(s: f32) -> Vec<Vec2> {
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(s, 0.0),
            Vec2::new(s, s),
            Vec2::new(0.0, s),
        ]
    }

    #[test]
    fn pole_i_srodek_kwadratu() {
        let k = kwadrat(10.0);
        assert!((signed_area(&k) - 100.0).abs() < 1e-6);
        let c = centroid(&k);
        assert!((c - Vec2::splat(5.0)).length() < 1e-4);
    }

    #[test]
    fn ciecie_dzieli_pole_bez_reszty() {
        let k = kwadrat(10.0);
        let n = Vec2::new(1.0, 0.0);
        let o = Vec2::new(3.0, 0.0);
        let prawa = clip_halfplane(&k, o, n);
        let lewa = clip_halfplane(&k, o, -n);
        let s = signed_area(&prawa) + signed_area(&lewa);
        assert!((s - 100.0).abs() < 1e-3, "suma pól = {s}");
        assert!((signed_area(&prawa) - 70.0).abs() < 1e-3);
    }

    #[test]
    fn obb_prostokata_ma_dluzsza_os_wzdluz_dluzszego_boku() {
        let r = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(40.0, 0.0),
            Vec2::new(40.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        let o = min_area_obb(&r);
        assert!((o.half_long - 20.0).abs() < 0.01, "{o:?}");
        assert!((o.half_short - 5.0).abs() < 0.01, "{o:?}");
        assert!(o.axis.x.abs() > 0.99, "{o:?}");
        assert!((o.area() - 400.0).abs() < 0.1);
    }

    #[test]
    fn punkt_w_wielokacie() {
        let k = kwadrat(10.0);
        assert!(contains(&k, Vec2::splat(5.0)));
        assert!(!contains(&k, Vec2::splat(15.0)));
    }
}
