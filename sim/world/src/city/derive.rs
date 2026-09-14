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

    /// Bryła **na zewnątrz** wskazanej ściany, o wysięgu `m`. Odwrotność [`Scope::faces`]:
    /// tamta bierze płytę od środka, ta dokłada ją poza licem. `Side` daje dwie.
    ///
    /// Ściany poziome nie dają nic — odrzuca je walidator, a derywacja nie ma się tu
    /// czym bronić poza pustą listą (komin to `Comp(Top) → Extrude`, nie `Protrude`).
    fn protrusions(&self, face: Face, m: f32) -> SmallVec<[Scope; 2]> {
        let mut out = SmallVec::new();
        match face {
            Face::Front => out.push(
                self.with_extent(Axis::V, m)
                    .shifted(Axis::V, -self.half.y - m * 0.5),
            ),
            Face::Back => out.push(
                self.with_extent(Axis::V, m)
                    .shifted(Axis::V, self.half.y + m * 0.5),
            ),
            Face::Side => {
                out.push(
                    self.with_extent(Axis::U, m)
                        .shifted(Axis::U, -self.half.x - m * 0.5),
                );
                out.push(
                    self.with_extent(Axis::U, m)
                        .shifted(Axis::U, self.half.x + m * 0.5),
                );
            }
            Face::Top | Face::Bottom => {}
        }
        out
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

/// Najmniejsze sensowne wysunięcie. Voxel ma metr w poziomie, więc balkon cieńszy niż to
/// nie ma się gdzie zrasteryzować — zamiast półbalkonu zostaje nic, i jest to policzone
/// (M2f §5.6c, konsekwencja korekty E12 z M2d).
pub const MIN_PROTRUDE_M: f32 = 1.0;

/// Skrajnia pionowa nad chodnikiem. Poniżej niej wysunięcie nie może wyjść poza działkę,
/// powyżej — może, bo wykusz nad chodnikiem nikomu nie wchodzi w drogę, a balkon
/// na wysokości parteru tak.
pub const OVERHANG_MIN_Z_M: f32 = 3.5;

/// Ile bryła może wystawać poza obrys, zanim wyjdzie z działki. Liczone w [`super::build`],
/// bo tam jest wielokąt parceli; derywacja dostaje gotowe cztery liczby.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Margins {
    pub front_m: f32,
    pub back_m: f32,
    pub side_m: f32,
    /// Dodatkowy wysięg nad chodnikiem, dozwolony wyłącznie powyżej [`OVERHANG_MIN_Z_M`].
    /// Zero dla działki bez frontu drogowego — nie ma wtedy chodnika, nad którym wisieć.
    pub overhang_front_m: f32,
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
    /// Zapas na wysunięcia (`Protrude`) — patrz [`Margins`].
    pub margins: Margins,
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
    /// Wysunięcia postawione, przycięte do granicy działki i odrzucone jako zbyt płytkie.
    /// Trzy liczby, nie jedna: „balkony są, ale płytsze" to co innego niż „balkonów nie ma",
    /// a bez rozróżnienia nie widać, czy brakuje miejsca, czy gramatyka prosi o za dużo.
    pub protrusions: u32,
    pub protrusions_clipped: u32,
    pub protrusions_dropped: u32,
}

struct Ctx<'a> {
    g: &'a BuildingGrammar,
    mats: &'a MaterialRegistry,
    p: BuildParams,
    /// Obrys posadowiony na parceli. Zapas na wysunięcia liczy się względem **jego** lica,
    /// a nie lica bieżącego zakresu: balkon na cofniętym poddaszu ma tyle samo miejsca
    /// co na piętrze poniżej, bo działka nie zwęża się razem z bryłą.
    base: Scope,
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
    /// Ile jeszcze wolno wysunąć się poza wskazaną ścianę zakresu `s`.
    fn margines(&self, face: Face, s: &Scope) -> f32 {
        let b = &self.base;
        let d = Vec2::new(s.center.x - b.center.x, s.center.y - b.center.y);
        let m = &self.p.margins;
        // Luz: o ile lico zakresu jest **schowane** względem lica bryły. Ujemny znaczy,
        // że zakres już wystaje (np. po `Offset`) i zapas odpowiednio topnieje.
        let (zapas, luz) = match face {
            Face::Front => {
                let nad_chodnikiem = s.center.z - s.half.z >= self.p.base_z_m + OVERHANG_MIN_Z_M;
                let z = if nad_chodnikiem {
                    m.front_m.max(m.overhang_front_m)
                } else {
                    m.front_m
                };
                (z, d.dot(b.v()) - s.half.y + b.half.y)
            }
            Face::Back => (m.back_m, b.half.y - d.dot(b.v()) - s.half.y),
            // Obie ściany boczne powstają jednym `Protrude`, więc obowiązuje ciaśniejsza.
            Face::Side => {
                let du = d.dot(b.u);
                (
                    m.side_m,
                    (du - s.half.x + b.half.x).min(b.half.x - du - s.half.x),
                )
            }
            Face::Top | Face::Bottom => (0.0, 0.0),
        };
        (zapas + luz).max(0.0)
    }

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
        base,
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
        Rule::Protrude { face, m, rule } => {
            let d = m.min(ctx.margines(*face, &s));
            if d < MIN_PROTRUDE_M {
                ctx.out.protrusions_dropped += 1;
                return;
            }
            if d < *m - 1e-3 {
                ctx.out.protrusions_clipped += 1;
            }
            for kawalek in s.protrusions(*face, d) {
                ctx.out.protrusions += 1;
                apply(ctx, rule, kawalek, depth + 1);
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
        Rule::Roof {
            shape,
            material,
            dormers,
        } => dach(ctx, s, *shape, material, *dormers),
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
fn dach(ctx: &mut Ctx, s: Scope, shape: RoofShape, material: &str, dormers: (u8, u8)) {
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
            lukarny(ctx, s, bottom, wys, m, dormers);
        }
    }
}

/// Lukarny na połaci frontowej. Wymiary są podyktowane rastrem, nie architekturą:
/// przy voxelu 1 m w poziomie i 0,5 m w pionie lukarna węższa niż dwa metry albo niższa
/// niż półtora znika w zaokrągleniu, więc mniejszej nie ma po co stawiać (korekta E12).
///
/// Bryła idzie **od okapu w głąb**, czyli przebija połać — i o to chodzi: to, co wystaje
/// ponad spadek, jest lukarną, a reszta chowa się w dachu.
fn lukarny(ctx: &mut Ctx, s: Scope, bottom: f32, wys: f32, m: MaterialId, dormers: (u8, u8)) {
    const SZER_M: f32 = 2.0;
    const GLEB_M: f32 = 3.0;
    const WYS_M: f32 = 1.5;
    /// Prześwit między sąsiednimi lukarnami; poniżej tego zlewają się w attykę.
    const ODSTEP_M: f32 = 1.5;

    if dormers.1 == 0 || wys < WYS_M + 1.0 {
        return;
    }
    let (lo, hi) = (dormers.0.min(dormers.1), dormers.1);
    let mut r = ctx.rng();
    let ile = if hi > lo {
        lo + (r.next_u32() % u32::from(hi - lo + 1)) as u8
    } else {
        lo
    };
    // Kalenica musi pomieścić tyle lukarn z prześwitami; nadmiar obcinamy, nie ściskamy.
    let mieszczace = ((s.half.x * 2.0) / (SZER_M + ODSTEP_M)).floor().max(0.0) as u32;
    let n = u32::from(ile).min(mieszczace);
    if n == 0 {
        return;
    }
    // Lukarna siedzi na dolnej trzeciej połaci — wyżej wychodzi z kalenicy, niżej z okapu.
    let dol = bottom + (wys - WYS_M) * 0.35;
    let gleb = GLEB_M.min(s.half.y);
    for i in 0..n {
        let u_mid = (i as f32 + 0.5) / n as f32 * s.half.x * 2.0 - s.half.x;
        let srodek = s
            .shifted(Axis::U, u_mid)
            .shifted(Axis::V, -s.half.y + gleb * 0.5)
            .center;
        ctx.out.parts.push(Part {
            scope: Scope {
                center: glam::Vec3::new(srodek.x, srodek.y, dol + WYS_M * 0.5),
                u: s.u,
                half: glam::Vec3::new(SZER_M * 0.5, gleb * 0.5, WYS_M * 0.5),
            },
            material: m,
            carve: false,
        });
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
            // Działka testowa jest większa od bryły o dwa metry z każdej strony —
            // tyle, żeby wysunięcia miały gdzie wyjść i żeby przycięcie dało się wymusić
            // osobno, zerując zapas.
            margins: Margins {
                front_m: 2.0,
                back_m: 2.0,
                side_m: 2.0,
                overhang_front_m: 0.0,
            },
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

    /// Minimalna gramatyka z jednym wysunięciem na wskazanej kondygnacji.
    /// Trzy kondygnacje po 3 m, więc spód parteru jest na 0, a spód ostatniej na 6 m —
    /// po obu stronach skrajni `OVERHANG_MIN_Z_M`, co daje się rozróżnić testem.
    fn z_wysunieciem(face: Face, m: f32, gdzie_parter: bool) -> BuildingGrammar {
        use crate::city::grammar::{Applies, Interior, Massing, UnitSpec, GRAMMAR_SCHEMA_VERSION};
        let balkon = Rule::Protrude {
            face,
            m,
            rule: Box::new(Rule::Fill("concrete".to_string())),
        };
        let pusto = Rule::Fill("stucco".to_string());
        BuildingGrammar {
            schema_version: GRAMMAR_SCHEMA_VERSION,
            id: "test_protrude".to_string(),
            applies: Applies {
                zones: vec!["r3".to_string()],
                epochs: Vec::new(),
                styles: Vec::new(),
                land_value: (0, 1_000_000),
                frontage_m: (5.0, 50.0),
                depth_m: (5.0, 50.0),
                weight: 10,
                coverage: 1.0,
            },
            massing: Massing {
                setback_front_m: 0.0,
                setback_side_m: 0.0,
                setback_back_m: 0.0,
                courtyard_min_m2: 0.0,
                max_depth_m: 30.0,
            },
            rules: vec![Rule::Floors {
                count: (3, 3),
                height_m: 3.0,
                ground_height_m: 3.0,
                ground: Box::new(if gdzie_parter {
                    balkon.clone()
                } else {
                    pusto.clone()
                }),
                typical: Box::new(pusto.clone()),
                top: Box::new(if gdzie_parter { pusto } else { balkon }),
            }],
            refs: Vec::new(),
            interior: Interior {
                ground: UnitSpec {
                    kind: UnitClass::Dwelling,
                    area_m2: (40, 90),
                },
                typical: UnitSpec {
                    kind: UnitClass::Dwelling,
                    area_m2: (40, 90),
                },
                top: UnitSpec {
                    kind: UnitClass::Dwelling,
                    area_m2: (40, 90),
                },
                circulation_share: 0.1,
            },
        }
    }

    fn params_z_zapasem(margins: Margins) -> BuildParams {
        let mut p = params(30_000, 1);
        p.margins = margins;
        p
    }

    #[test]
    fn protrude_wychodzi_poza_lico_bryly() {
        // Sedno operatora: `Comp` bierze płytę od środka, `Protrude` dokłada ją na zewnątrz.
        let (_, m) = katalog();
        let g = z_wysunieciem(Face::Front, 1.2, false);
        let base = zakres(12.0, 16.0);
        let out = derive(
            &g,
            &m,
            base,
            params_z_zapasem(Margins {
                front_m: 2.0,
                back_m: 2.0,
                side_m: 2.0,
                overhang_front_m: 0.0,
            }),
        );
        assert_eq!(out.protrusions, 1, "wysunięcie nie powstało");
        assert_eq!(out.protrusions_dropped, 0);
        // Oś `v` rośnie w głąb działki, więc lico frontowe bryły jest na `-half.y`.
        let lico = base.center.y - base.half.y;
        let balkon = out
            .parts
            .iter()
            .find(|p| p.scope.center.y < lico)
            .expect("bryła przed licem frontowym");
        let wysieg = lico - (balkon.scope.center.y - balkon.scope.half.y);
        assert!(
            (wysieg - 1.2).abs() < 1e-2,
            "wysięg {wysieg} m zamiast 1,2 m"
        );
    }

    #[test]
    fn bez_miejsca_na_dzialce_wysuniecia_nie_ma_i_jest_policzone() {
        // Cicho pominięty balkon jest gorszy od braku balkonu: nie widać, że katalog
        // prosi o coś, czego działka nie ma jak pomieścić.
        let (_, m) = katalog();
        let g = z_wysunieciem(Face::Back, 1.2, false);
        let out = derive(&g, &m, zakres(12.0, 16.0), params_z_zapasem(Margins::default()));
        assert_eq!(out.protrusions, 0);
        assert_eq!(out.protrusions_dropped, 1, "odrzucenie nie zostało policzone");
    }

    #[test]
    fn wysieg_nad_chodnikiem_dopiero_powyzej_skrajni() {
        // Działka bez zapasu od frontu, ale z prawem wysięgu nad chodnikiem: balkon
        // na ostatniej kondygnacji (spód 6 m) wolno, na parterze (spód 0 m) nie.
        let (_, m) = katalog();
        let zapas = Margins {
            front_m: 0.0,
            back_m: 0.0,
            side_m: 0.0,
            overhang_front_m: 1.2,
        };
        let gora = derive(
            &z_wysunieciem(Face::Front, 1.2, false),
            &m,
            zakres(12.0, 16.0),
            params_z_zapasem(zapas),
        );
        let parter = derive(
            &z_wysunieciem(Face::Front, 1.2, true),
            &m,
            zakres(12.0, 16.0),
            params_z_zapasem(zapas),
        );
        assert_eq!(gora.protrusions, 1, "balkon powyżej skrajni ma stanąć");
        assert_eq!(parter.protrusions, 0, "balkon nad jezdnią na parterze — nie");
        assert_eq!(parter.protrusions_dropped, 1);
    }

    #[test]
    fn wysuniecie_z_cofnietego_poddasza_ma_pelen_zapas() {
        // Zapas liczy się od lica **bryły**, nie od lica zakresu: gdyby liczył się od zakresu,
        // `Inset` przed `Protrude` zjadałby balkon, choć działka się nie zwęziła.
        let (_, m) = katalog();
        let mut g = z_wysunieciem(Face::Back, 1.2, false);
        let Rule::Floors { top, .. } = &mut g.rules[0] else {
            unreachable!("gramatyka testowa ma jedną regułę Floors")
        };
        **top = Rule::Inset {
            m: 0.8,
            rule: top.clone(),
        };
        let out = derive(
            &g,
            &m,
            zakres(12.0, 16.0),
            params_z_zapasem(Margins {
                front_m: 0.0,
                back_m: 1.2,
                side_m: 0.0,
                overhang_front_m: 0.0,
            }),
        );
        assert_eq!(out.protrusions, 1);
        assert_eq!(out.protrusions_clipped, 0, "zapas policzony od lica zakresu");
    }

    #[test]
    fn lukarny_stoja_na_polaci_i_trzymaja_sie_widelek() {
        let (_, m) = katalog();
        let mut g = z_wysunieciem(Face::Back, 1.2, false);
        g.rules.push(Rule::Roof {
            shape: RoofShape::Gable { pitch_deg: 45 },
            material: "roof_tile_red".to_string(),
            dormers: (2, 3),
        });
        let bez = {
            let mut h = g.clone();
            h.rules.pop();
            h.rules.push(Rule::Roof {
                shape: RoofShape::Gable { pitch_deg: 45 },
                material: "roof_tile_red".to_string(),
                dormers: (0, 0),
            });
            derive(&h, &m, zakres(18.0, 16.0), params_z_zapasem(Margins::default()))
        };
        let z_lukarnami = derive(
            &g,
            &m,
            zakres(18.0, 16.0),
            params_z_zapasem(Margins::default()),
        );
        let ile = z_lukarnami.parts.len() - bez.parts.len();
        assert!(
            (2..=3).contains(&ile),
            "{ile} lukarn zamiast 2–3 z widełek"
        );
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
            // Budżet węzłów przy **maksymalnych** wymiarach z `applies`: `Repeat`
            // rozwija się dopiero przy znanych wymiarach, więc to jest przypadek najgorszy,
            // a średnie wymiary nie mówią o nim nic (kryterium WP18).
            let max = derive(
                gram,
                &m,
                zakres(gram.applies.frontage_m.1, gram.applies.depth_m.1),
                params(gram.applies.land_value.1, 1),
            );
            assert!(
                !max.truncated,
                "gramatyka {} przekroczyła budżet przy największej działce ({} węzłów)",
                gram.id,
                max.nodes
            );
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
