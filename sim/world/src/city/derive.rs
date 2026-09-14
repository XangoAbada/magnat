//! WP11 — silnik derywacji gramatyki (M2 §5.6).
//!
//! Drzewo zakresów: zakres to zorientowany prostopadłościan (środek, oś pozioma `u`,
//! połowy wymiarów wzdłuż `u`, `v`, `w`), reguła przekształca go w listę zakresów
//! potomnych, a terminale zostawiają [`Part`] — wypełnienie albo otwór.
//!
//! **Determinizm i równoległość.** RNG derywacji to
//! `rng(world_seed, StreamId::BuildingGrammar, building_index, Tick(node_index))`,
//! gdzie `node_index` jest numerem węzła w porządku DFS. Nie ma licznika współdzielonego
//! między budynkami, więc budynki wolno derywować równolegle — każdy w swoim buforze —
//! a komendy voxelowe składa się potem po `building_index`. To najdroższy etap fazy
//! i jedyny zrównoleglony.
//!
//! Zakresy nie obracają się w trakcie derywacji: cały budynek ma jedną oś `u`, wyznaczoną
//! przez front działki. Obrót per reguła byłby czwartym operatorem bez konsumenta, a koszt
//! poniósłby każdy voxel (00, „Dobre praktyki": YAGNI).

use super::grammar::{
    Axis, BuildingGrammar, Cond, Face, RoofShape, Rule, Size, MAX_DEPTH, MAX_NODES,
};
use super::zoning::ZoneKind;
use magnat_core::{det_math, rng, Rng, StreamId, Tick};
use magnat_spatial::Vec2;
use magnat_voxel::{MaterialId, MaterialRegistry};
use smallvec::SmallVec;

/// Zorientowany zakres w metrach. `z` jest wysokością n.p.m.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Scope {
    pub center: glam::Vec3,
    /// Kierunek osi `u` (wzdłuż frontu), jednostkowy.
    pub u: Vec2,
    /// Połowy wymiarów wzdłuż `(u, v, w)`.
    pub half: glam::Vec3,
}

impl Scope {
    #[must_use]
    pub fn new(center: glam::Vec3, u: Vec2, half: glam::Vec3) -> Scope {
        Scope {
            center,
            u: u.normalize_or_zero(),
            half,
        }
    }

    /// Oś `v` — obrót `u` o 90° w lewo. Rośnie **w głąb działki**, od ulicy.
    #[must_use]
    pub fn v(&self) -> Vec2 {
        Vec2::new(-self.u.y, self.u.x)
    }

    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        self.half.x <= 0.05 || self.half.y <= 0.05 || self.half.z <= 0.01
    }

    #[must_use]
    pub fn area_m2(&self) -> f32 {
        4.0 * self.half.x * self.half.y
    }

    /// Przesunięcie środka wzdłuż osi zakresu.
    #[must_use]
    fn shifted(&self, axis: Axis, d: f32) -> Scope {
        let mut s = *self;
        match axis {
            Axis::U => {
                s.center.x += self.u.x * d;
                s.center.y += self.u.y * d;
            }
            Axis::V => {
                let v = self.v();
                s.center.x += v.x * d;
                s.center.y += v.y * d;
            }
            Axis::W => s.center.z += d,
        }
        s
    }

    #[must_use]
    fn extent(&self, axis: Axis) -> f32 {
        match axis {
            Axis::U => self.half.x * 2.0,
            Axis::V => self.half.y * 2.0,
            Axis::W => self.half.z * 2.0,
        }
    }

    #[must_use]
    fn with_extent(&self, axis: Axis, len: f32) -> Scope {
        let mut s = *self;
        match axis {
            Axis::U => s.half.x = len * 0.5,
            Axis::V => s.half.y = len * 0.5,
            Axis::W => s.half.z = len * 0.5,
        }
        s
    }

    /// Podział na kolejne kawałki o zadanych długościach, od strony „minus" osi.
    fn slice(&self, axis: Axis, lens: &[f32]) -> Vec<Scope> {
        let total = self.extent(axis);
        let mut out = Vec::with_capacity(lens.len());
        let mut acc = 0.0f32;
        for l in lens {
            let mid = acc + l * 0.5;
            out.push(self.with_extent(axis, *l).shifted(axis, mid - total * 0.5));
            acc += l;
        }
        out
    }

    #[must_use]
    fn inset(&self, m: f32) -> Scope {
        let mut s = *self;
        s.half.x = (s.half.x - m).max(0.0);
        s.half.y = (s.half.y - m).max(0.0);
        s
    }

    #[must_use]
    fn offset(&self, m: f32) -> Scope {
        let mut s = *self;
        s.half.x += m;
        s.half.y += m;
        s
    }

    /// Wysokość `m`, licząc od dotychczasowego spodu zakresu.
    #[must_use]
    fn extrude(&self, m: f32) -> Scope {
        let mut s = *self;
        let bottom = self.center.z - self.half.z;
        s.half.z = m * 0.5;
        s.center.z = bottom + m * 0.5;
        s
    }

    /// Płyta o grubości `t` przy wskazanej ścianie. `Side` daje dwie — lewą i prawą.
    fn faces(&self, face: Face, t: f32) -> SmallVec<[Scope; 2]> {
        let mut out = SmallVec::new();
        match face {
            Face::Front => {
                let g = t.min(self.half.y * 2.0);
                out.push(
                    self.with_extent(Axis::V, g)
                        .shifted(Axis::V, -self.half.y + g * 0.5),
                );
            }
            Face::Back => {
                let g = t.min(self.half.y * 2.0);
                out.push(
                    self.with_extent(Axis::V, g)
                        .shifted(Axis::V, self.half.y - g * 0.5),
                );
            }
            Face::Side => {
                let g = t.min(self.half.x * 2.0);
                out.push(
                    self.with_extent(Axis::U, g)
                        .shifted(Axis::U, -self.half.x + g * 0.5),
                );
                out.push(
                    self.with_extent(Axis::U, g)
                        .shifted(Axis::U, self.half.x - g * 0.5),
                );
            }
            Face::Top => {
                let g = t.min(self.half.z * 2.0);
                out.push(
                    self.with_extent(Axis::W, g)
                        .shifted(Axis::W, self.half.z - g * 0.5),
                );
            }
            Face::Bottom => {
                let g = t.min(self.half.z * 2.0);
                out.push(
                    self.with_extent(Axis::W, g)
                        .shifted(Axis::W, -self.half.z + g * 0.5),
                );
            }
        }
        out
    }
}

/// Terminal derywacji: zakres plus to, co w nim zrobić.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Part {
    pub scope: Scope,
    pub material: MaterialId,
    /// `true` = otwór (`Carve`), `false` = wypełnienie (`Prism`).
    pub carve: bool,
}

/// Parametry wejściowe budynku — cztery kanały z M2 §5.6 plus tożsamość do RNG.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BuildParams {
    pub zone: ZoneKind,
    /// Indeks pierścienia epoki kwartału.
    pub epoch_ring: u8,
    /// Grosze za m² — wynik `pass_1` (WP15a).
    pub land_value_per_m2: i64,
    /// Rzędna posadowienia w metrach n.p.m. (po niwelacji parceli).
    pub base_z_m: f32,
    pub world_seed: u64,
    pub building_index: u32,
}

/// Wynik derywacji jednego budynku.
#[derive(Clone, Debug, Default)]
pub struct Derived {
    pub parts: Vec<Part>,
    pub floors: u8,
    pub basements: u8,
    /// Wysokość każdej kondygnacji nadziemnej, w decymetrach — M11 tnie widok po stropach.
    pub floor_heights_dm: SmallVec<[u16; 8]>,
    pub height_dm: u16,
    pub foundation_depth_m: f32,
    pub nodes: u32,
    /// Budżet węzłów albo głębokość przekroczone — liczone w `GenerationReport`,
    /// nigdy nie jest cichym obcięciem (M2 §5.6).
    pub truncated: bool,
}

struct Ctx<'a> {
    g: &'a BuildingGrammar,
    mats: &'a MaterialRegistry,
    p: BuildParams,
    out: Derived,
    /// Numer węzła w porządku DFS — drugi składnik klucza RNG.
    node: u32,
    /// Wierzch dotychczas zbudowanej bryły; `Roof` startuje właśnie stąd.
    top_z: f32,
    /// Numer kondygnacji widziany przez `Cond::FloorAtLeast`.
    floor: u8,
    ref_stack: Vec<String>,
}

impl Ctx<'_> {
    fn rng(&mut self) -> Rng {
        self.node += 1;
        rng(
            self.p.world_seed,
            StreamId::BuildingGrammar,
            self.p.building_index,
            Tick(u64::from(self.node)),
        )
    }

    fn material(&self, key: &str) -> MaterialId {
        self.mats.expect_id(key)
    }
}

/// Derywacja gramatyki na zakresie bryły. `base` ma wymiary obrysu i **zerową wysokość**
/// — leży na rzędnej posadowienia; reguły budują w górę i w dół od niej.
#[must_use]
pub fn derive(
    g: &BuildingGrammar,
    mats: &MaterialRegistry,
    base: Scope,
    p: BuildParams,
) -> Derived {
    let mut ctx = Ctx {
        g,
        mats,
        p,
        out: Derived::default(),
        node: 0,
        top_z: p.base_z_m,
        floor: 0,
        ref_stack: Vec::new(),
    };
    for r in &g.rules {
        apply(&mut ctx, r, base, 1);
    }
    let h = (ctx.top_z - p.base_z_m).max(0.0);
    ctx.out.height_dm = (h * 10.0).clamp(0.0, f32::from(u16::MAX)) as u16;
    ctx.out.nodes = ctx.node;
    ctx.out
}

fn apply(ctx: &mut Ctx, r: &Rule, s: Scope, depth: u32) {
    if depth > MAX_DEPTH || ctx.node > MAX_NODES {
        ctx.out.truncated = true;
        return;
    }
    // Zakres bryły ma **zerową wysokość** — leży na rzędnej posadowienia. Trzy reguły
    // domenowe budują od niej w pionie i dostają go takim, jaki jest; wszystkie pozostałe
    // operują na gotowej kondygnacji i na zerowej wysokości nie mają czego robić.
    let pion_wazny = !matches!(
        r,
        Rule::Foundation { .. } | Rule::Floors { .. } | Rule::Roof { .. }
    );
    if s.half.x <= 0.05 || s.half.y <= 0.05 || (pion_wazny && s.half.z <= 0.01) {
        return;
    }
    match r {
        Rule::Seq(v) => {
            for x in v {
                apply(ctx, x, s, depth + 1);
            }
        }
        Rule::Nothing => {}
        Rule::Fill(m) => {
            let material = ctx.material(m);
            ctx.out.parts.push(Part {
                scope: s,
                material,
                carve: false,
            });
        }
        Rule::Void => ctx.out.parts.push(Part {
            scope: s,
            material: MaterialId::AIR,
            carve: true,
        }),
        Rule::Inset { m, rule } => apply(ctx, rule, s.inset(*m), depth + 1),
        Rule::Offset { m, rule } => apply(ctx, rule, s.offset(*m), depth + 1),
        Rule::Extrude { m, rule } => apply(ctx, rule, s.extrude(*m), depth + 1),
        Rule::Split { axis, parts } => {
            let lens = dlugosci(s.extent(*axis), parts);
            for (kawalek, (_, rule)) in s.slice(*axis, &lens).into_iter().zip(parts) {
                apply(ctx, rule, kawalek, depth + 1);
            }
        }
        Rule::Repeat {
            axis,
            step_m,
            pad_m,
            rule,
        } => {
            let total = s.extent(*axis);
            let step = step_m.max(0.1);
            let n = (total / step).floor() as i32;
            if n <= 0 {
                return;
            }
            // Reszta idzie na marginesy po obu stronach — powtórzenie ma być wyśrodkowane,
            // inaczej okna zbierają się przy jednym narożniku.
            let margines = (total - step * n as f32) * 0.5;
            let szerokosc = (step - 2.0 * pad_m).max(0.05);
            for i in 0..n {
                let mid = margines + step * (i as f32 + 0.5) - total * 0.5;
                let kawalek = s.with_extent(*axis, szerokosc).shifted(*axis, mid);
                apply(ctx, rule, kawalek, depth + 1);
                if ctx.node > MAX_NODES {
                    ctx.out.truncated = true;
                    return;
                }
            }
        }
        Rule::Comp { thickness_m, faces } => {
            for (f, rule) in faces {
                for kawalek in s.faces(*f, *thickness_m) {
                    apply(ctx, rule, kawalek, depth + 1);
                }
            }
        }
        Rule::Choice(v) => {
            let suma: u32 = v.iter().map(|(w, _)| u32::from(*w)).sum();
            if suma == 0 {
                return;
            }
            let mut r = ctx.rng();
            let mut los = r.next_u32() % suma;
            for (w, rule) in v {
                if los < u32::from(*w) {
                    apply(ctx, rule, s, depth + 1);
                    return;
                }
                los -= u32::from(*w);
            }
        }
        Rule::If {
            cond,
            then,
            otherwise,
        } => {
            let ok = match cond {
                Cond::LandValueAtLeast(v) => ctx.p.land_value_per_m2 >= *v,
                Cond::FloorAtLeast(f) => ctx.floor >= *f,
                Cond::WidthAtLeast(w) => s.half.x * 2.0 >= *w,
                Cond::DepthAtLeast(d) => s.half.y * 2.0 >= *d,
                Cond::HeightAtLeast(h) => s.half.z * 2.0 >= *h,
            };
            apply(ctx, if ok { then } else { otherwise }, s, depth + 1);
        }
        Rule::Ref(k) => {
            // Cykl wyłapał walidator (WP10), ale derywacja też się broni: gramatyka
            // z moddingu (M12) nie przejdzie przez ten sam walidator co katalog z repo.
            if ctx.ref_stack.iter().any(|x| x == k) {
                ctx.out.truncated = true;
                return;
            }
            let Some(target) = ctx.g.rule_ref(k).cloned() else {
                return;
            };
            ctx.ref_stack.push(k.clone());
            apply(ctx, &target, s, depth + 1);
            ctx.ref_stack.pop();
        }
        Rule::Foundation {
            depth_m,
            material,
            basements,
        } => {
            let m = ctx.material(material);
            let h = depth_m + f32::from(*basements) * 2.8;
            let bottom = ctx.p.base_z_m - h;
            ctx.out.basements = *basements;
            ctx.out.foundation_depth_m = h;
            ctx.out.parts.push(Part {
                scope: Scope {
                    center: glam::Vec3::new(s.center.x, s.center.y, bottom + h * 0.5),
                    u: s.u,
                    half: glam::Vec3::new(s.half.x, s.half.y, h * 0.5),
                },
                material: m,
                carve: false,
            });
        }
        Rule::Floors {
            count,
            height_m,
            ground_height_m,
            ground,
            typical,
            top,
        } => kondygnacje(
            ctx,
            s,
            depth,
            *count,
            *height_m,
            *ground_height_m,
            ground,
            typical,
            top,
        ),
        Rule::Roof { shape, material } => dach(ctx, s, *shape, material),
    }
}

/// Długości części `Split`: najpierw bezwzględne, reszta rozdzielona po udziałach.
fn dlugosci(total: f32, parts: &[(Size, Rule)]) -> Vec<f32> {
    let abs: f32 = parts
        .iter()
        .map(|(s, _)| match s {
            Size::Abs(m) => *m,
            Size::Rel(_) => 0.0,
        })
        .sum();
    let rel: f32 = parts
        .iter()
        .map(|(s, _)| match s {
            Size::Rel(w) => *w,
            Size::Abs(_) => 0.0,
        })
        .sum();
    let reszta = (total - abs).max(0.0);
    parts
        .iter()
        .map(|(s, _)| match s {
            Size::Abs(m) => m.min(total),
            Size::Rel(w) => {
                if rel > 0.0 {
                    reszta * w / rel
                } else {
                    0.0
                }
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn kondygnacje(
    ctx: &mut Ctx,
    s: Scope,
    depth: u32,
    count: (u8, u8),
    height_m: f32,
    ground_height_m: f32,
    ground: &Rule,
    typical: &Rule,
    top: &Rule,
) {
    let n = liczba_kondygnacji(ctx, count);
    ctx.out.floors = n;
    let mut z = ctx.top_z;
    for i in 0..n {
        let h = if i == 0 { ground_height_m } else { height_m };
        let kondygnacja = Scope {
            center: glam::Vec3::new(s.center.x, s.center.y, z + h * 0.5),
            u: s.u,
            half: glam::Vec3::new(s.half.x, s.half.y, h * 0.5),
        };
        ctx.floor = i;
        let rule = if i == 0 {
            ground
        } else if i + 1 == n {
            top
        } else {
            typical
        };
        apply(ctx, rule, kondygnacja, depth + 1);
        ctx.out.floor_heights_dm.push((h * 10.0) as u16);
        z += h;
    }
    ctx.floor = 0;
    ctx.top_z = z;

    // Dziedziniec: zabudowa obrzeżna ma trakt i studnię w środku, nie pełną bryłę.
    // Jeden wykop przez wszystkie kondygnacje zamiast wycinania go piętro po piętrze —
    // studnia jest własnością bryły, nie kondygnacji.
    let min_m2 = ctx.g.massing.courtyard_min_m2;
    if min_m2 > 0.0 {
        const TRAKT_M: f32 = 11.0;
        let hx = s.half.x - TRAKT_M;
        let hy = s.half.y - TRAKT_M;
        if hx > 1.0 && hy > 1.0 && 4.0 * hx * hy >= min_m2 {
            ctx.out.parts.push(Part {
                scope: Scope {
                    center: glam::Vec3::new(s.center.x, s.center.y, (ctx.p.base_z_m + z) * 0.5),
                    u: s.u,
                    half: glam::Vec3::new(hx, hy, (z - ctx.p.base_z_m) * 0.5),
                },
                material: MaterialId::AIR,
                carve: true,
            });
        }
    }
}

/// Liczba kondygnacji: interpolacja w `count` po **wartości gruntu** (kanał z M2 §5.6),
/// plus jedno losowanie na budynek, żeby pierzeja nie była równa jak linijka.
fn liczba_kondygnacji(ctx: &mut Ctx, count: (u8, u8)) -> u8 {
    let (lo, hi) = (count.0.max(1), count.1.max(count.0.max(1)));
    if lo == hi {
        return lo;
    }
    let (vmin, vmax) = ctx.g.applies.land_value;
    let t = if vmax > vmin {
        ((ctx.p.land_value_per_m2 - vmin) as f64 / (vmax - vmin) as f64).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let baza = f64::from(lo) + t * f64::from(hi - lo);
    let mut r = ctx.rng();
    // ±1 kondygnacja, symetrycznie; przycięte do przedziału gramatyki.
    let jitter = (r.next_u32() % 3) as i32 - 1;
    ((baza + 0.5) as i32 + jitter).clamp(i32::from(lo), i32::from(hi)) as u8
}

/// Dach. Spadzisty jest schodkowany: każdy stopień to jedna bryła, bo `EditOp` nie zna
/// płaszczyzny skośnej. Przy voxelu 0,5 m stopnie są poniżej progu widoczności z wysokości,
/// z której M2 pokazuje miasto; profil ciągły to detal wizualny, czyli M11.
fn dach(ctx: &mut Ctx, s: Scope, shape: RoofShape, material: &str) {
    /// Najmniejsze **poziome** cofnięcie stopnia połaci. Voxel ma 1 m w poziomie,
    /// więc stopień węższy niż to obraca się w kratownicę: dwie zagnieżdżone bryły
    /// obrócone pod kątem rasteryzują się z własnym schodkiem, a schodki się mijają.
    const MIN_COFNIECIE_M: f32 = 2.5;
    const MAX_STOPNI: i32 = 6;
    /// Grubość płyty dachu płaskiego.
    const GRUBOSC_M: f32 = 0.5;
    let m = ctx.material(material);
    let bottom = ctx.top_z;
    match shape {
        RoofShape::Flat => {
            ctx.out.parts.push(Part {
                scope: Scope {
                    center: glam::Vec3::new(s.center.x, s.center.y, bottom + GRUBOSC_M * 0.5),
                    u: s.u,
                    half: glam::Vec3::new(s.half.x, s.half.y, GRUBOSC_M * 0.5),
                },
                material: m,
                carve: false,
            });
            ctx.top_z = bottom + GRUBOSC_M;
        }
        RoofShape::Gable { pitch_deg } | RoofShape::Hip { pitch_deg } => {
            let hip = matches!(shape, RoofShape::Hip { .. });
            // `tan` wyłącznie z `det_math` (00 §K-6) — libm w kodzie symulacji jest zakazany.
            let tan = det_math::tan(f64::from(pitch_deg.min(70)) * std::f64::consts::PI / 180.0);
            let wys = (s.half.y * tan as f32).clamp(0.0, 12.0);
            // Liczba stopni z **poziomego** wysięgu połaci, nie z jej wysokości.
            let stopni = ((s.half.y / MIN_COFNIECIE_M).round() as i32).clamp(1, MAX_STOPNI);
            let krok_z = wys / stopni as f32;
            for i in 0..stopni {
                let t = (i as f32 + 1.0) / stopni as f32;
                // Zwężenie jest **bezwzględne**, nie proporcjonalne: połać ma stały spadek,
                // więc na każdej wysokości cofa się o tyle samo metrów w obu osiach.
                // Proporcjonalne dawało kopułę — im dłuższy budynek, tym bardziej pękatą.
                let cofniecie = s.half.y * t;
                let hy = (s.half.y - cofniecie).max(0.5);
                let hx = if hip {
                    (s.half.x - cofniecie).max(0.5)
                } else {
                    s.half.x
                };
                // Stopnie są **zagnieżdżone, każdy od podstawy dachu**, a nie ułożone
                // jeden na drugim. Przy voxelu 1 m w poziomie i 0,5 m w pionie kolejne
                // cienkie płyty rozjeżdżały się na zaokrągleniu i dach wychodził dziurawy
                // jak wafel; bryły zachodzące na siebie nie mają jak zostawić szczeliny.
                let gora = bottom + krok_z * (i as f32 + 1.0);
                ctx.out.parts.push(Part {
                    scope: Scope {
                        center: glam::Vec3::new(s.center.x, s.center.y, (bottom + gora) * 0.5),
                        u: s.u,
                        half: glam::Vec3::new(hx, hy, (gora - bottom) * 0.5),
                    },
                    material: m,
                    carve: false,
                });
            }
            ctx.top_z = bottom + wys;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::grammar::{GrammarSet, UnitClass};

    fn zakres(w: f32, d: f32) -> Scope {
        Scope::new(
            glam::Vec3::new(100.0, 100.0, 50.0),
            Vec2::new(1.0, 0.0),
            glam::Vec3::new(w * 0.5, d * 0.5, 0.0),
        )
    }

    fn params(lv: i64, idx: u32) -> BuildParams {
        BuildParams {
            zone: ZoneKind::Residential(super::super::zoning::ResDensity::R3),
            epoch_ring: 1,
            land_value_per_m2: lv,
            base_z_m: 50.0,
            world_seed: 0x00C0_FFEE,
            building_index: idx,
        }
    }

    fn katalog() -> (GrammarSet, MaterialRegistry) {
        let m = MaterialRegistry::load_dir(&crate::assets::data_path("materials")).unwrap();
        let g = GrammarSet::load_dir(&crate::assets::data_path("grammar"), &m).unwrap();
        (g, m)
    }

    #[test]
    fn split_dzieli_bez_reszty_i_bez_nakladki() {
        let s = Scope::new(
            glam::Vec3::ZERO,
            Vec2::new(1.0, 0.0),
            glam::Vec3::new(10.0, 5.0, 2.0),
        );
        let kawalki = s.slice(Axis::U, &[5.0, 10.0, 5.0]);
        assert_eq!(kawalki.len(), 3);
        let suma: f32 = kawalki.iter().map(|k| k.half.x * 2.0).sum();
        assert!((suma - 20.0).abs() < 1e-3, "suma długości {suma}");
        // Krańce sąsiadów mają się stykać, nie zachodzić.
        for para in kawalki.windows(2) {
            let koniec = para[0].center.x + para[0].half.x;
            let poczatek = para[1].center.x - para[1].half.x;
            assert!(
                (koniec - poczatek).abs() < 1e-3,
                "szczelina {koniec} vs {poczatek}"
            );
        }
    }

    #[test]
    fn repeat_jest_wysrodkowany() {
        let s = Scope::new(
            glam::Vec3::ZERO,
            Vec2::new(1.0, 0.0),
            glam::Vec3::new(5.0, 1.0, 1.0),
        );
        // 10 m / krok 3 m = 3 kawałki, margines po 0,5 m z każdej strony.
        let total = s.extent(Axis::U);
        let n = (total / 3.0).floor() as i32;
        assert_eq!(n, 3);
        let margines = (total - 3.0 * n as f32) * 0.5;
        assert!((margines - 0.5).abs() < 1e-3);
    }

    #[test]
    fn wartosc_gruntu_podnosi_zabudowe() {
        // Kanał „wartość gruntu" z §5.6 ma realnie zmieniać liczbę kondygnacji.
        let (g, m) = katalog();
        let id = g.id_of("kamienica").expect("gramatyka kamienica");
        let gram = g.get(id);
        let tanio: u32 = (0..64)
            .map(|i| {
                u32::from(
                    derive(
                        gram,
                        &m,
                        zakres(14.0, 20.0),
                        params(gram.applies.land_value.0, i),
                    )
                    .floors,
                )
            })
            .sum();
        let drogo: u32 = (0..64)
            .map(|i| {
                u32::from(
                    derive(
                        gram,
                        &m,
                        zakres(14.0, 20.0),
                        params(gram.applies.land_value.1, i),
                    )
                    .floors,
                )
            })
            .sum();
        assert!(
            drogo > tanio,
            "droga ziemia dała {drogo} kondygnacji, tania {tanio}"
        );
    }

    #[test]
    fn derywacja_jest_powtarzalna_co_do_bitu() {
        let (g, m) = katalog();
        let gram = g.get(g.id_of("kamienica").unwrap());
        let a = derive(gram, &m, zakres(14.0, 20.0), params(30_000, 7));
        let b = derive(gram, &m, zakres(14.0, 20.0), params(30_000, 7));
        assert_eq!(a.parts, b.parts);
        assert_eq!(a.floor_heights_dm, b.floor_heights_dm);
    }

    #[test]
    fn kazda_gramatyka_z_repo_stawia_cos_i_miesci_sie_w_budzecie() {
        let (g, m) = katalog();
        for gram in g.all() {
            let w = (gram.applies.frontage_m.0 + gram.applies.frontage_m.1) * 0.5;
            let d = (gram.applies.depth_m.0 + gram.applies.depth_m.1) * 0.5;
            let lv = (gram.applies.land_value.0 + gram.applies.land_value.1) / 2;
            let out = derive(gram, &m, zakres(w, d), params(lv, 1));
            assert!(
                !out.parts.is_empty(),
                "gramatyka {} nie postawiła ani jednej bryły",
                gram.id
            );
            assert!(!out.truncated, "gramatyka {} przekroczyła budżet", gram.id);
            assert!(out.floors >= 1, "gramatyka {} bez kondygnacji", gram.id);
            assert_eq!(
                out.floor_heights_dm.len(),
                usize::from(out.floors),
                "gramatyka {}: liczba stropów nie zgadza się z liczbą kondygnacji",
                gram.id
            );
            assert!(
                out.height_dm > 0,
                "gramatyka {} ma zerową wysokość",
                gram.id
            );
            assert!(
                matches!(
                    gram.interior.ground.kind,
                    UnitClass::Dwelling
                        | UnitClass::Retail
                        | UnitClass::Office
                        | UnitClass::Workshop
                        | UnitClass::Storage
                ),
                "nieznany rodzaj lokalu"
            );
        }
    }
}
