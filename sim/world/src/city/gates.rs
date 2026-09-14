//! WP3 — punkty wejścia do miasta i wybór środka miasta (M2 §5.2).
//!
//! Brama jest pierwszą rzeczą, jaka powstaje w mieście, bo od niej zaczyna się
//! aksjomat L-systemu. Kolejność jest więc odwrotna do intuicji: najpierw wiemy,
//! **którędy** się do miasta wjeżdża, a dopiero potem, jak miasto wygląda.
//!
//! Środek miasta nie jest opisany w planie fazy — to luka, którą ta podfaza zamyka
//! (odnotowane w „Korektach planu" dokumentu podfazy). Wybieramy go tak, jak wybierali
//! go ludzie: płaski, suchy grunt blisko żeglownej wody, w środkowej części mapy.

use super::road::NodeId;
use super::CityPlan;
use crate::query::{NavigableClass, TerrainQuery};
use magnat_core::{rng, IVec2, Qty, StreamId, Tick};
use magnat_spatial::Vec2;
use serde::{Deserialize, Serialize};

/// Rodzaj bramy. Kolejność wariantów jest kolejnością przetwarzania — brama drogowa
/// powstaje przed kolejową, bo kolej domyka się do istniejącego układu, nie odwrotnie.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum GateKind {
    Highway,
    RailFreight,
    RailPassenger,
    Port,
    Airport,
}

impl GateKind {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            GateKind::Highway => "highway",
            GateKind::RailFreight => "rail_freight",
            GateKind::RailPassenger => "rail_passenger",
            GateKind::Port => "port",
            GateKind::Airport => "airport",
        }
    }

    /// Przepustowość dobowa w `Qty` (milisztuki). Skala jest **wstępna**: prawdziwym
    /// konsumentem jest limit importu w M6 i to M6 ją skalibruje. M2 ma dostarczyć
    /// pole, nie bilans handlowy.
    /// Czy brama jest kolejowa — tory powstają dopiero w M2c (Etap 4 daje im cel).
    #[must_use]
    pub const fn is_rail(self) -> bool {
        matches!(self, GateKind::RailFreight | GateKind::RailPassenger)
    }

    #[must_use]
    pub const fn capacity(self) -> Qty {
        Qty(match self {
            GateKind::Highway => 60_000_000,
            GateKind::RailFreight => 250_000_000,
            GateKind::RailPassenger => 0,
            GateKind::Port => 400_000_000,
            GateKind::Airport => 4_000_000,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CityGate {
    pub kind: GateKind,
    /// Punkt na krawędzi mapy (`Airport`: wewnątrz).
    pub pos: Vec2,
    pub dir_inward: Vec2,
    pub capacity: Qty,
    pub node: NodeId,
}

/// Obrys lotniska — strefa zakazana dla L-systemu (ograniczenie lokalne 6).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AirportFootprint {
    pub center: Vec2,
    /// Kierunek osi pasa startowego (jednostkowy).
    pub axis: Vec2,
    pub length_m: f32,
    pub width_m: f32,
}

impl AirportFootprint {
    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
        let d = p - self.center;
        let along = d.dot(self.axis).abs();
        let across = d.dot(Vec2::new(-self.axis.y, self.axis.x)).abs();
        along <= self.length_m * 0.5 && across <= self.width_m * 0.5
    }
}

/// Wymagania bramowe profilu gospodarczego — `data/zoning/profile_*.ron`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GateProfile {
    pub schema_version: u32,
    /// Bramy, bez których miasto tego profilu nie ma sensu.
    pub required: Vec<GateKind>,
    /// Bramy losowane; prawdopodobieństwo 0..=1.
    pub optional: Vec<(GateKind, f32)>,
    /// Kwoty stref Etapu 4 — dołożone przez M2c (`schema_version` 2).
    pub mix: super::zoning::ZoneMix,
}

/// Bok siatki, na której szukamy środka miasta, w metrach.
const CENTER_GRID_M: i32 = 128;
/// Kwantyzacja kandydatów bramowych wzdłuż krawędzi mapy (M2 §5.2).
const GATE_QUANT_M: i32 = 200;
/// Głębokość, na jaką „wprowadzamy" drogę przy wycenie kandydata (M2 §5.2).
const GATE_PROBE_M: i32 = 800;
/// Minimalna głębokość wody dla portu, w decymetrach (6 m z M2 §5.2).
const PORT_MIN_DEPTH_DM: u16 = 60;
/// Pas startowy z M2 §5.2.
const RUNWAY_L_M: f32 = 2600.0;
const RUNWAY_W_M: f32 = 600.0;
/// Minimalna odległość lotniska od środka miasta (M2 §5.2).
const AIRPORT_MIN_DIST_M: f32 = 4000.0;
/// Nachylenie < 3% w jednostkach `slope_at` (0,03 · 64 = 1,92).
const AIRPORT_MAX_SLOPE: u8 = 1;

/// Brama wraz z punktem, w którym dotyka jej sieć drogowa.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GateSpot {
    pub kind: GateKind,
    /// Punkt bramy: dla portu nabrzeże na wodzie, dla lotniska środek obrysu,
    /// dla reszty punkt na krawędzi mapy.
    pub pos: Vec2,
    pub dir_inward: Vec2,
    /// Punkt, w którym powstaje **węzeł** sieci. Dla portu jest to brzeg, nie tafla:
    /// węzeł drogowy nie ma prawa stać w wodzie (ograniczenie lokalne 6).
    pub node_pos: Vec2,
}

/// Wynik WP3.
#[derive(Clone, PartialEq, Debug)]
pub struct GatePlan {
    pub center: Vec2,
    pub gates: Vec<GateSpot>,
    pub airport: Option<AirportFootprint>,
    /// Bramy wymagane przez profil, których teren nie pozwolił postawić.
    /// Trafiają do `GenerationReport` — milczące pominięcie byłoby gorsze od braku.
    pub missing: Vec<GateKind>,
}

/// Środek miasta: płaski i suchy grunt w środkowej części mapy, z premią za bliskość
/// żeglownej wody. Bez RNG — rozstrzyga teren, a remisy kolejność komórek.
#[must_use]
pub fn pick_center(plan: &CityPlan, t: &dyn TerrainQuery) -> Vec2 {
    let map = plan.map_size_m();
    let half = map as f32 * 0.5;
    let n = map / CENTER_GRID_M;
    // Margines: środek miasta nie leży na krawędzi mapy, bo miasto ma się wokół niego
    // zmieścić. Jedna czwarta promienia obszaru zurbanizowanego z zapasem.
    let margin = (n / 6).max(1);

    let mut best = (i32::MIN, Vec2::new(half, half));
    for gy in margin..n - margin {
        for gx in margin..n - margin {
            let (x, y) = (gx * CENTER_GRID_M, gy * CENTER_GRID_M);
            let b = t.buildability_at(x, y);
            if !b.slope_ok {
                continue;
            }
            let p = Vec2::new(x as f32, y as f32);
            let d_center = (p - Vec2::splat(half)).length();
            // Centralność: 100 w środku mapy, 0 na jej krawędzi.
            let centrality = 100.0 * (1.0 - d_center / half);
            let mut score = centrality as i32;
            score -= i32::from(b.flood_risk.get()) * 2;
            score -= i32::from(t.slope_at(x, y)) * 8;
            // Premia za wodę: miasta stawiano nad rzeką i nad portem, a nie obok.
            if let Some(d) = navigable_distance_m(t, x, y, 480) {
                score += 220 - d / 3;
            }
            if score > best.0 {
                best = (score, p);
            }
        }
    }
    best.1
}

/// Odległość do najbliższej żeglownej wody, próbkowana pierścieniowo. Zwraca `None`,
/// gdy w promieniu `max_m` jej nie ma.
fn navigable_distance_m(t: &dyn TerrainQuery, x: i32, y: i32, max_m: i32) -> Option<i32> {
    const DIRS: [(i32, i32); 8] = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    let mut r = 60;
    while r <= max_m {
        for (dx, dy) in DIRS {
            if t.navigable(x + dx * r, y + dy * r).is_some() {
                return Some(r);
            }
        }
        r += 60;
    }
    None
}

/// WP3 w całości: profil → lista bram z pozycjami i kierunkami do wnętrza.
#[must_use]
pub fn place_gates(plan: &CityPlan, t: &dyn TerrainQuery, profile: &GateProfile) -> GatePlan {
    let center = pick_center(plan, t);
    let mut chosen: Vec<GateKind> = profile.required.clone();

    // Bramy opcjonalne: jedno losowanie na rodzaj, ze strumienia bram. Indeks encji to
    // wariant bramy, więc dopisanie nowego rodzaju w przyszłości nie przesuwa losowań
    // pozostałych.
    for (kind, p) in &profile.optional {
        if chosen.contains(kind) {
            continue;
        }
        let mut r = rng(plan.seed, StreamId::Gates, 900 + *kind as u32, Tick(0));
        if r.gen_bool_permille((*p * 1000.0) as u16) {
            chosen.push(*kind);
        }
    }
    // **Bez `dedup`**: `required: [Highway, Highway]` to dwa wjazdy z dwóch stron mapy,
    // a nie jeden zapisany dwa razy. Miasto z jednym wjazdem nie ma jak domknąć sieci.
    chosen.sort_unstable();

    let mut out = GatePlan {
        center,
        gates: Vec::new(),
        airport: None,
        missing: Vec::new(),
    };

    for (i, kind) in chosen.iter().copied().enumerate() {
        let idx = i as u32;
        let placed = match kind {
            GateKind::Airport => place_airport(plan, t, center).map(|fp| {
                out.airport = Some(fp);
                (fp.center, (center - fp.center).normalize_or_zero())
            }),
            GateKind::Port => place_port(plan, t, center),
            _ => place_edge_gate(plan, t, center, kind, idx, &out.gates),
        };
        match placed.and_then(|(pos, dir)| {
            shore_point(t, pos, dir).map(|node_pos| GateSpot {
                kind,
                pos,
                dir_inward: dir,
                node_pos,
            })
        }) {
            Some(spot) => out.gates.push(spot),
            None if profile.required.contains(&kind) => out.missing.push(kind),
            None => {}
        }
    }
    out
}

/// Pierwszy suchy punkt w głąb lądu od bramy. Dla bram lądowych zwraca zwykle punkt
/// wyjścia; dla portu — nabrzeże.
fn shore_point(t: &dyn TerrainQuery, pos: Vec2, dir: Vec2) -> Option<Vec2> {
    let mut d = 0.0f32;
    while d <= 900.0 {
        let p = pos + dir * d;
        if !is_water(t, p) {
            return Some(p);
        }
        d += 20.0;
    }
    None
}

/// Brama krawędziowa: kandydaci co 200 m na każdej z czterech krawędzi, koszt
/// wprowadzenia drogi 800 m w głąb, losowanie wśród trzech najlepszych.
fn place_edge_gate(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
    center: Vec2,
    kind: GateKind,
    gate_index: u32,
    already: &[GateSpot],
) -> Option<(Vec2, Vec2)> {
    let map = plan.map_size_m();
    // Bramy nie mogą stać obok siebie — miasto z dwoma wjazdami po tej samej stronie
    // nie ma powodu rosnąć w pozostałe.
    let min_sep = map as f32 / 5.0;

    let mut cand: Vec<(i32, Vec2, Vec2)> = Vec::new();
    for edge in 0..4 {
        let mut s = GATE_QUANT_M;
        while s < map - GATE_QUANT_M {
            let (pos, dir) = match edge {
                0 => (Vec2::new(s as f32, 0.0), Vec2::new(0.0, 1.0)),
                1 => (Vec2::new(s as f32, (map - 1) as f32), Vec2::new(0.0, -1.0)),
                2 => (Vec2::new(0.0, s as f32), Vec2::new(1.0, 0.0)),
                _ => (Vec2::new((map - 1) as f32, s as f32), Vec2::new(-1.0, 0.0)),
            };
            s += GATE_QUANT_M;
            if already.iter().any(|g| (g.pos - pos).length() < min_sep) {
                continue;
            }
            // Brama ma prowadzić do miasta, a nie wzdłuż krawędzi: odrzucamy kandydatów,
            // dla których kierunek do środka odchyla się od normalnej o więcej niż 60°.
            let to_center = (center - pos).normalize_or_zero();
            if to_center.dot(dir) < 0.5 {
                continue;
            }
            if let Some(c) = edge_probe_cost(t, pos, to_center, kind) {
                cand.push((c, pos, to_center));
            }
        }
    }
    if cand.is_empty() {
        return None;
    }
    // Klucz sortowania jest **totalny**: koszt, potem pozycja. Bez drugiego członu
    // dwaj kandydaci o równym koszcie zamieniliby się miejscami przy zmianie kolejności
    // krawędzi i hash generacji przestałby być powtarzalny.
    cand.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.x.total_cmp(&b.1.x))
            .then(a.1.y.total_cmp(&b.1.y))
    });
    let top = cand.len().min(3);
    let mut r = rng(plan.seed, StreamId::Gates, gate_index, Tick(0));
    let pick = r.gen_range_u32(top as u32) as usize;
    Some((cand[pick].1, cand[pick].2))
}

/// Koszt wprowadzenia drogi w głąb lądu. `None` = kandydat dyskwalifikowany.
fn edge_probe_cost(t: &dyn TerrainQuery, pos: Vec2, dir: Vec2, kind: GateKind) -> Option<i32> {
    let max_slope = match kind {
        GateKind::Highway => super::road::spec(super::road::RoadClass::Highway).max_slope_units(),
        _ => super::road::spec(super::road::RoadClass::RailFreight).max_slope_units(),
    };
    let mut cost = 0i32;
    let mut przeprawy = 0;
    let kroki = GATE_PROBE_M / 25;
    let mut poprzednia_woda = false;
    for k in 0..=kroki {
        let p = pos + dir * (k * 25) as f32;
        let (x, y) = (p.x as i32, p.y as i32);
        let slope = t.slope_at(x, y);
        let woda = t.water_at(x, y).depth_dm > 0;
        if woda && !poprzednia_woda {
            przeprawy += 1;
        }
        poprzednia_woda = woda;
        // Sam punkt wjazdu musi być suchy i przejezdny — brama w rzece to nie brama.
        if k == 0 && (woda || slope > max_slope * 3) {
            return None;
        }
        cost += i32::from(slope) * 6;
        cost += i32::from(t.buildability_at(x, y).flood_risk.get());
        if woda {
            cost += 90;
        }
        if slope > max_slope {
            cost += i32::from(slope - max_slope) * 40;
        }
    }
    // Każda przeprawa to most; jedna jest do przyjęcia, trzy oznaczają, że wjazd
    // biegnie wzdłuż doliny zamiast w poprzek.
    if przeprawy > 2 {
        return None;
    }
    Some(cost + przeprawy * 300)
}

/// Port: punkt na **żeglownej wodzie o głębokości ≥ 6 m**, możliwie blisko miasta,
/// z nabrzeżem na sąsiednim brzegu.
///
/// **Doprecyzowanie wobec planu.** Plan mówi „punkt na krawędzi mapy" w komentarzu do
/// pola `pos`, ale warunek stawia inny: „woda o głębokości ≥ 6 m i ciągłe połączenie
/// do krawędzi mapy". To drugie jest właściwe — port leży przy mieście, nie na skraju
/// świata. Ciągłość połączenia wynika tu z klasy wody, a nie z przeszukiwania: `Sea`
/// dochodzi do krawędzi z definicji, `River` w modelu M1 zawsze uchodzi do morza albo
/// poza mapę. `Canal` (jezioro) odpada, bo jezioro bywa bezodpływowe.
fn place_port(plan: &CityPlan, t: &dyn TerrainQuery, center: Vec2) -> Option<(Vec2, Vec2)> {
    let map = plan.map_size_m();
    const KROK_M: i32 = 64;
    let mut best: Option<(i32, Vec2, Vec2)> = None;
    let mut y = KROK_M;
    while y < map {
        let mut x = KROK_M;
        while x < map {
            let p = Vec2::new(x as f32, y as f32);
            x += KROK_M;
            if !matches!(
                t.navigable(p.x as i32, p.y as i32),
                Some(NavigableClass::Sea | NavigableClass::River { .. })
            ) || t.water_depth_at(p.x as i32, p.y as i32) < PORT_MIN_DEPTH_DM
            {
                continue;
            }
            // Nabrzeże: port bez wjazdu od lądu nie jest portem, tylko kotwicowiskiem.
            let to_land = (center - p).normalize_or_zero();
            if shore_point(t, p, to_land).is_none() {
                continue;
            }
            let score = i32::from(t.water_depth_at(p.x as i32, p.y as i32))
                - (p - center).length() as i32 / 4;
            if best.is_none_or(|(b, bp, _)| score > b || (score == b && (p.x, p.y) < (bp.x, bp.y)))
            {
                best = Some((score, p, to_land));
            }
        }
        y += KROK_M;
    }
    best.map(|(_, p, d)| (p, d))
}

/// Lotnisko: prostokąt 2600 × 600 m o nachyleniu < 3%, ≥ 4 km od środka, na suchym terenie.
/// Dwie orientacje pasa (N–S i E–W) — trzecia nie zmieniłaby wyniku dość, żeby zapłacić
/// za nią czasem szukania.
fn place_airport(plan: &CityPlan, t: &dyn TerrainQuery, center: Vec2) -> Option<AirportFootprint> {
    let map = plan.map_size_m() as f32;
    let step = 200;
    let mut best: Option<(i32, AirportFootprint)> = None;

    for axis in [Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)] {
        let half_l = RUNWAY_L_M * 0.5;
        let half_w = RUNWAY_W_M * 0.5;
        let (ex, ey) = (
            axis.x.abs() * half_l + axis.y.abs() * half_w,
            axis.y.abs() * half_l + axis.x.abs() * half_w,
        );
        let mut cy = ey as i32;
        while (cy as f32) < map - ey {
            let mut cx = ex as i32;
            while (cx as f32) < map - ex {
                let c = Vec2::new(cx as f32, cy as f32);
                cx += step;
                if (c - center).length() < AIRPORT_MIN_DIST_M {
                    continue;
                }
                if let Some(score) = runway_score(t, c, axis) {
                    // Bliżej miasta = lepiej, ale dopiero po spełnieniu warunku terenu.
                    let s = score - (c - center).length() as i32 / 20;
                    if best.is_none_or(|(b, _)| s > b) {
                        best = Some((
                            s,
                            AirportFootprint {
                                center: c,
                                axis,
                                length_m: RUNWAY_L_M,
                                width_m: RUNWAY_W_M,
                            },
                        ));
                    }
                }
            }
            cy += step;
        }
    }
    best.map(|(_, fp)| fp)
}

/// `None`, gdy prostokąt nie spełnia warunku terenu.
fn runway_score(t: &dyn TerrainQuery, c: Vec2, axis: Vec2) -> Option<i32> {
    let across = Vec2::new(-axis.y, axis.x);
    let mut score = 0i32;
    let mut i = -13;
    while i <= 13 {
        let mut j = -3;
        while j <= 3 {
            let p = c + axis * (i * 100) as f32 + across * (j * 100) as f32;
            let (x, y) = (p.x as i32, p.y as i32);
            let b = t.buildability_at(x, y);
            if !b.slope_ok || t.slope_at(x, y) > AIRPORT_MAX_SLOPE {
                return None;
            }
            score -= i32::from(b.flood_risk.get());
            j += 1;
        }
        i += 1;
    }
    Some(score)
}

/// Maska przeszkód terenu w prostokącie — używana przez L-system do odrzucania
/// propozycji na wodzie. Wydzielone tutaj, bo to ten sam rodzaj pytania co przy bramach.
#[must_use]
pub fn is_water(t: &dyn TerrainQuery, p: Vec2) -> bool {
    t.water_at(p.x as i32, p.y as i32).depth_dm > 0
}

/// Punkt jako `IVec2` — jedyne miejsce, w którym zaokrąglamy oś drogi do metra.
#[must_use]
pub fn to_ivec(p: Vec2) -> IVec2 {
    IVec2::new(p.x as i32, p.y as i32)
}
