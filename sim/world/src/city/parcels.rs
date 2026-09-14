//! WP8 — sieć lokalna i podział na parcele (M2 §5.4).
//!
//! Trzy przebiegi, bo mutacja sieci w trakcie liczenia geometrii rozjeżdżałaby
//! identyfikatory segmentów między kwartałami (korekta C7):
//!
//! 1. **Geometria** — każdy kwartał dzielony rekurencyjnie na podkwartały (OBB + ulica
//!    w cięciu), a każdy podkwartał pasowo na działki. Nic tu nie dotyka `RoadNetwork`.
//! 2. **Sieć** — zebrane punkty zaczepienia dzielą segmenty brzegowe (wsadowo, po jednym
//!    segmencie naraz, w kolejności parametru), a ulice lokalne trafiają do sieci.
//! 3. **Fronty** — `CsrGrid<SegmentId>` po gotowej sieci odpowiada na pytanie
//!    „najbliższy odcinek ulicy", z którego powstaje `Frontage`. Ten sam indeks jest
//!    kontraktem dla M3 (M2 §6).
//!
//! Rozłączność parcel (test T5) jest **konstrukcyjna**: każde cięcie półpłaszczyzną
//! dzieli wielokąt na dwie części o wspólnej krawędzi, więc nakładka nie ma skąd powstać.

use super::blocks::{BlockId, BlockSet};
use super::poly;
use super::road::{
    NodeFlags, NodeId, PolyArena, PolyRef, RoadClass, RoadFlags, RoadNetwork, RoadNode,
    RoadSegment, RoadStructure, SegmentId, UNASSIGNED_DISTRICT,
};
use super::zoning::{ZoneKind, ZoneResult};
use super::CityPlan;
use crate::query::TerrainQuery;
use magnat_core::{det_math, rng, CitizenId, DistrictId, FirmId, Money, Rng, StreamId, Tick};
use magnat_spatial::{CsrGrid, GridSpec, ParcelTree, Vec2};

/// Odcinek frontu działki wzdłuż osi drogi, parametryzowany 0..=1 (M2 §5.4).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Frontage {
    pub seg: SegmentId,
    pub t0: f32,
    pub t1: f32,
}

impl Frontage {
    /// Brak frontu: podwórko wewnątrz kwartału. `t0 == t1`, więc długość jest zerowa
    /// i test T7 widzi to bez osobnej gałęzi.
    pub const NONE: Frontage = Frontage {
        seg: SegmentId(u32::MAX),
        t0: 0.0,
        t1: 0.0,
    };

    #[must_use]
    pub fn is_none(&self) -> bool {
        self.seg.0 == u32::MAX || (self.t1 - self.t0).abs() < 1e-6
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParcelOwner {
    Unowned,
    City,
    Citizen(CitizenId),
    Firm(FirmId),
    Developer(FirmId),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParcelStatus {
    Vacant,
    Built,
    UnderConstruction { done_at: magnat_core::SimMinute },
    Derelict,
    Reserved,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Parcel {
    pub block: BlockId,
    pub district: DistrictId,
    pub poly: PolyRef,
    pub area_m2: u32,
    pub frontage: Frontage,
    pub zone: ZoneKind,
    pub owner: ParcelOwner,
    pub status: ParcelStatus,
    /// Grosze za m². Statyczna w M2; wypełnia ją WP15 (M2e).
    pub land_value_per_m2: Money,
    pub building: Option<magnat_core::BuildingId>,
}

/// Wynik WP8.
#[derive(Clone, Debug)]
pub struct ParcelSet {
    pub parcels: Vec<Parcel>,
    /// Quadtree parcel — wejście dla inspekcji („kliknięcie parceli") i testu T5.
    pub tree: ParcelTree,
    /// „Najbliższy odcinek ulicy" — kontrakt dla M3 (M2 §6).
    pub street_index: CsrGrid<SegmentId>,
    /// Ulice lokalne dołożone przez ten pakiet.
    pub local_streets: u32,
    /// Segmenty brzegowe podzielone nowym skrzyżowaniem.
    pub splits: u32,
    /// Działki odrzucone jako zbyt małe (zlane z sąsiednią).
    pub slivers: u32,
    /// Ulice lokalne odrzucone, bo spadek między ulicami, które miały łączyć,
    /// przekraczał limit klasy.
    pub too_steep: u32,
}

// ── Parametry stref ──────────────────────────────────────────────────────────────────

/// Wymiary działki i kwartału per strefa — tabela z M2 §5.4.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ZoneSpec {
    /// Szerokość frontu: minimum / moda / maksimum, w metrach (rozkład trójkątny).
    pub front: (f32, f32, f32),
    pub depth_m: f32,
    /// Powierzchnia, powyżej której kwartał jest dzielony ulicą lokalną, w m².
    /// `f32::INFINITY` = bez limitu (rolnictwo, zieleń, wydobycie).
    pub max_block_m2: f32,
}

#[must_use]
pub fn zone_spec(z: ZoneKind) -> ZoneSpec {
    use super::zoning::ResDensity as D;
    let (front, depth_m, max_ha) = match z {
        ZoneKind::Residential(D::R1) => ((18.0, 22.0, 30.0), 32.0, 4.0),
        ZoneKind::Residential(D::R2) => ((10.0, 14.0, 18.0), 30.0, 2.5),
        ZoneKind::Residential(D::R3) => ((9.0, 14.0, 22.0), 38.0, 1.2),
        ZoneKind::Residential(D::R4) => ((40.0, 60.0, 95.0), 55.0, 6.0),
        ZoneKind::Residential(D::R5) => ((30.0, 45.0, 65.0), 50.0, 4.0),
        ZoneKind::Commercial => ((20.0, 35.0, 60.0), 55.0, 3.0),
        ZoneKind::Office => ((25.0, 40.0, 60.0), 50.0, 2.5),
        ZoneKind::IndustryLight => ((60.0, 85.0, 120.0), 110.0, 8.0),
        ZoneKind::IndustryHeavy => ((120.0, 200.0, 300.0), 260.0, 30.0),
        ZoneKind::Logistics => ((100.0, 140.0, 200.0), 180.0, 20.0),
        ZoneKind::Institutional => ((40.0, 70.0, 140.0), 80.0, 6.0),
        // Rolnictwo i zieleń mają limit kwartału mimo braku wpisu w tabeli §5.4:
        // ściana grafu bywa tam wielkości kilkuset hektarów, a droga polna dzieli
        // pole tak samo jak ulica dzieli kwartał (korekta C6).
        ZoneKind::Agriculture => ((120.0, 200.0, 400.0), 380.0, 40.0),
        ZoneKind::Green => ((0.0, 0.0, 0.0), 0.0, 20.0),
        // Zieleń, wydobycie i teren wyłączony nie dzielą się na działki frontowe:
        // kwartał jest jedną parcelą o kształcie wynikającym z terenu.
        _ => ((0.0, 0.0, 0.0), 0.0, f32::INFINITY),
    };
    ZoneSpec {
        front,
        depth_m,
        max_block_m2: if max_ha.is_finite() {
            max_ha * 10_000.0
        } else {
            f32::INFINITY
        },
    }
}

/// Czy strefa dzieli kwartał na działki frontowe.
fn strip_divided(z: ZoneKind) -> bool {
    !matches!(
        z,
        ZoneKind::Green | ZoneKind::Extraction | ZoneKind::Water | ZoneKind::Undevelopable
    )
}

/// Poniżej tego pola kawałek nie jest działką, tylko odpadem po cięciu.
const MIN_PARCEL_M2: f64 = 60.0;
/// Podwórko większe niż to dostaje własną parcelę z przejazdem bramowym (M2 §5.4).
const COURTYARD_MIN_M2: f64 = 4_000.0;
/// Długość przejazdu bramowego w metrach — front parceli wewnętrznej.
const GATEWAY_M: f32 = 6.0;
/// Poniżej tej odległości od istniejącego węzła nie zakładamy nowego: kikut o długości
/// paru metrów jest gorszy od skrzyżowania przesuniętego o te pare metrów.
const MIN_STUB_M: f32 = 4.0;

// ── Przebieg 1: geometria ────────────────────────────────────────────────────────────

/// Właściciel krawędzi podkwartału: to on decyduje, czy krawędź jest frontem i o ile
/// trzeba się cofnąć, żeby trafić w oś jezdni.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Owner {
    /// Krawędź wewnętrzna — nie frontuje do niczego.
    None,
    /// Istniejący segment sieci (brzeg kwartału).
    Road(SegmentId, u8, u8),
    /// Ulica lokalna dołożona w tym kwartale; nie ma jeszcze `SegmentId`.
    Street(u8, u8),
}

impl Owner {
    fn rank(self) -> u8 {
        match self {
            Owner::None => 0,
            Owner::Road(_, r, _) | Owner::Street(r, _) => r,
        }
    }
    fn row_m(self) -> f32 {
        match self {
            Owner::None => 0.0,
            Owner::Road(_, _, w) | Owner::Street(_, w) => f32::from(w),
        }
    }
    fn seg(self) -> Option<SegmentId> {
        match self {
            Owner::Road(s, _, _) => Some(s),
            _ => None,
        }
    }
}

/// Podkwartał: wielokąt z właścicielem przy każdej krawędzi `pts[i] → pts[i+1]`.
#[derive(Clone, Debug)]
struct SubBlock {
    pts: Vec<Vec2>,
    owner: Vec<Owner>,
}

impl SubBlock {
    fn area(&self) -> f64 {
        poly::signed_area(&self.pts)
    }
}

/// Cięcie podkwartału półpłaszczyzną z przeniesieniem właścicieli krawędzi.
/// Zachowana krawędź niesie swojego właściciela, nowa (linia cięcia) — `new_owner`.
fn cut(sb: &SubBlock, origin: Vec2, normal: Vec2, new_owner: Owner) -> SubBlock {
    let n = sb.pts.len();
    let mut pts: Vec<Vec2> = Vec::with_capacity(n + 2);
    let mut owner: Vec<Owner> = Vec::with_capacity(n + 2);
    let side = |p: Vec2| (p - origin).dot(normal);
    for i in 0..n {
        let a = sb.pts[i];
        let b = sb.pts[(i + 1) % n];
        let (sa, sb_) = (side(a), side(b));
        let o = sb.owner[i];
        if sa >= 0.0 {
            pts.push(a);
            owner.push(o);
        }
        if (sa >= 0.0) != (sb_ >= 0.0) {
            let t = sa / (sa - sb_);
            pts.push(a + (b - a) * t);
            owner.push(if sa >= 0.0 { new_owner } else { o });
        }
    }
    // Krawędzie zerowej długości wywracają normalną w podziale pasowym. Usuwamy punkt
    // **następny** i właściciela krawędzi zerowej, żeby indeksy nie rozjechały się
    // o jeden — para pierwszy–ostatni osobno, bo tam przesunięcie byłoby cykliczne.
    let mut i = 0;
    while i + 1 < pts.len() {
        if (pts[i] - pts[i + 1]).length_squared() < 1e-6 {
            pts.remove(i + 1);
            owner.remove(i);
        } else {
            i += 1;
        }
    }
    while pts.len() > 1 && (pts[0] - pts[pts.len() - 1]).length_squared() < 1e-6 {
        pts.pop();
        owner.pop();
    }
    SubBlock { pts, owner }
}

/// Zaczepienie ulicy lokalnej w brzegu kwartału: punkt na **osi** jezdni i segment,
/// który trzeba w nim podzielić.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Anchor {
    seg: SegmentId,
    pos: Vec2,
}

/// Ulica lokalna przed wejściem do sieci.
#[derive(Clone, Copy, PartialEq, Debug)]
struct LocalStreet {
    a: Vec2,
    b: Vec2,
    class: RoadClass,
    anchor_a: Option<Anchor>,
    anchor_b: Option<Anchor>,
}

/// Działka przed przypisaniem frontu.
#[derive(Clone, Debug)]
struct RawParcel {
    pts: Vec<Vec2>,
    /// Środek krawędzi frontowej — po nim przebieg 3 znajdzie segment ulicy.
    front_mid: Option<Vec2>,
    interior: bool,
}

/// Geometria jednego kwartału: podkwartały → działki, plus ulice lokalne w cięciach.
struct BlockGeometry {
    parcels: Vec<RawParcel>,
    streets: Vec<LocalStreet>,
    slivers: u32,
}

/// Krok pierwszy: rekurencyjny podział kwartału na podkwartały ulicami lokalnymi.
fn split_recursive(
    sb: SubBlock,
    spec: ZoneSpec,
    r: &mut Rng,
    depth: u32,
    out: &mut Vec<SubBlock>,
    streets: &mut Vec<LocalStreet>,
) {
    let area = sb.area();
    // Stop: kwartał w limicie, za wąski na dwa pasy zabudowy albo za głęboka rekurencja.
    let obb = poly::min_area_obb(&sb.pts);
    let za_waski = obb.half_short * 2.0 < spec.depth_m * 2.0;
    if depth >= 8 || area <= f64::from(spec.max_block_m2) || za_waski || sb.pts.len() < 3 {
        out.push(sb);
        return;
    }

    // Ulica w cięciu: `Service`, gdy obie połówki będą mniejsze niż 0,6 ha.
    let class = if area * 0.5 < 6_000.0 {
        RoadClass::Service
    } else {
        RoadClass::Local
    };
    let row = f32::from(class.spec().row_m);

    // Punkt cięcia: 0,5 ± U(−0,12; 0,12) wzdłuż dłuższej osi (M2 §5.4).
    let u = 0.5 + (r.gen_range_u32(241) as f32 / 1000.0 - 0.12);
    let axis = obb.axis;
    let origin = obb.center + axis * ((u - 0.5) * 2.0 * obb.half_long);

    // Oś ulicy przecina podkwartał — końce trafiają na jego brzeg.
    let perp = Vec2::new(-axis.y, axis.x);
    let Some((e0, o0, e1, o1)) = przetnij_brzeg(&sb, origin, perp) else {
        out.push(sb);
        return;
    };
    // Brzeg podkwartału jest odsunięty o połowę pasa drogowego, więc koniec ulicy
    // wraca tą samą połową na zewnątrz — dokładnie na oś jezdni, którą dotyka.
    let na_os = |p: Vec2, e: usize| -> Vec2 {
        let n = sb.pts.len();
        let d = (sb.pts[(e + 1) % n] - sb.pts[e]).normalize_or_zero();
        p + Vec2::new(d.y, -d.x) * (sb.owner[e].row_m() * 0.5)
    };
    let (pa, pb) = (na_os(e0, o0), na_os(e1, o1));
    streets.push(LocalStreet {
        a: pa,
        b: pb,
        class,
        anchor_a: sb.owner[o0].seg().map(|seg| Anchor { seg, pos: pa }),
        anchor_b: sb.owner[o1].seg().map(|seg| Anchor { seg, pos: pb }),
    });

    let owner = Owner::Street(class.rank(), class.spec().row_m);
    let a = cut(&sb, origin + axis * (row * 0.5), axis, owner);
    let b = cut(&sb, origin - axis * (row * 0.5), -axis, owner);
    for h in [a, b] {
        if h.pts.len() >= 3 && h.area() > MIN_PARCEL_M2 {
            split_recursive(h, spec, r, depth + 1, out, streets);
        }
    }
}

/// Przecięcie prostej `origin + t·dir` z brzegiem podkwartału. Zwraca dwa punkty
/// wraz z indeksem krawędzi, na której leżą.
fn przetnij_brzeg(sb: &SubBlock, origin: Vec2, dir: Vec2) -> Option<(Vec2, usize, Vec2, usize)> {
    let n = sb.pts.len();
    let mut trafienia: Vec<(f32, Vec2, usize)> = Vec::new();
    let nrm = Vec2::new(-dir.y, dir.x);
    for i in 0..n {
        let a = sb.pts[i];
        let b = sb.pts[(i + 1) % n];
        let (sa, sbb) = ((a - origin).dot(nrm), (b - origin).dot(nrm));
        if (sa >= 0.0) == (sbb >= 0.0) {
            continue;
        }
        let t = sa / (sa - sbb);
        let p = a + (b - a) * t;
        trafienia.push(((p - origin).dot(dir), p, i));
    }
    if trafienia.len() < 2 {
        return None;
    }
    trafienia.sort_by(|x, y| x.0.total_cmp(&y.0));
    // Bierzemy **sąsiednią parę** trafień, której środek leży wewnątrz podkwartału,
    // a nie pierwsze i ostatnie: przy wielokącie niewypukłym prosta wychodzi z niego
    // i wraca, więc odcinek od pierwszego do ostatniego przecięcia biegłby kawałkiem
    // na zewnątrz — przez cudzy kwartał i w poprzek otaczającej drogi, łamiąc
    // planarność sieci. Z par wewnętrznych wygrywa najdłuższa.
    let mut best: Option<(f32, usize)> = None;
    for i in 0..trafienia.len() - 1 {
        let srodek = (trafienia[i].1 + trafienia[i + 1].1) * 0.5;
        if !poly::contains(&sb.pts, srodek) {
            continue;
        }
        let dl = trafienia[i + 1].0 - trafienia[i].0;
        if best.is_none_or(|(b, _)| dl > b) {
            best = Some((dl, i));
        }
    }
    let (_, i) = best?;
    Some((
        trafienia[i].1,
        trafienia[i].2,
        trafienia[i + 1].1,
        trafienia[i + 1].2,
    ))
}

/// Krok drugi: podział pasowy podkwartału na działki (M2 §5.4, „Podział na parcele").
fn strip_parcels(
    mut sb: SubBlock,
    zone: ZoneKind,
    spec: ZoneSpec,
    r: &mut Rng,
    out: &mut Vec<RawParcel>,
    slivers: &mut u32,
) {
    if !strip_divided(zone) {
        out.push(RawParcel {
            front_mid: front_srodek(&sb),
            pts: sb.pts,
            interior: false,
        });
        return;
    }

    for _ in 0..4 {
        if sb.pts.len() < 3 || sb.area() < MIN_PARCEL_M2 {
            return;
        }
        // Krawędź frontowa: najwyższa klasa drogi, przy remisie najdłuższa,
        // przy dalszym remisie najniższy indeks krawędzi (M2 §5.4).
        let n = sb.pts.len();
        let front = (0..n).filter(|&i| sb.owner[i].rank() > 0).max_by(|&i, &j| {
            let dl = |k: usize| (sb.pts[(k + 1) % n] - sb.pts[k]).length();
            sb.owner[i]
                .rank()
                .cmp(&sb.owner[j].rank())
                .then(dl(i).total_cmp(&dl(j)))
                .then(j.cmp(&i))
        });
        let Some(fi) = front else { break };

        let a = sb.pts[fi];
        let b = sb.pts[(fi + 1) % n];
        let d = (b - a).normalize_or_zero();
        if d.length_squared() < 0.5 {
            break;
        }
        // Obchód jest przeciwny do wskazówek zegara, więc wnętrze leży po lewej.
        let inward = Vec2::new(-d.y, d.x);

        // Pas zabudowy o głębokości `depth`, reszta zostaje na kolejny obieg.
        let plaszczyzna = a + inward * spec.depth_m;
        let pas = cut(&sb, plaszczyzna, -inward, Owner::None);
        let reszta = cut(&sb, plaszczyzna, inward, Owner::None);

        // Podkwartał płytszy niż `depth` jest w całości pasem zabudowy.
        let plytki = reszta.pts.len() < 3 || reszta.area() < MIN_PARCEL_M2;
        let pas = if plytki { sb.clone() } else { pas };
        tnij_pas(&pas, a, d, spec, r, out, slivers);
        if plytki {
            return;
        }
        sb = reszta;

        if sb.pts.len() < 3 || sb.area() < MIN_PARCEL_M2 {
            return;
        }
        if !sb.owner.iter().any(|o| o.rank() > 0) {
            break;
        }
    }

    // Wnętrze kwartału: własna parcela z przejazdem bramowym, gdy jest duże,
    // inaczej podwórko miejskie (M2 §5.4).
    if sb.pts.len() >= 3 && sb.area() >= MIN_PARCEL_M2 {
        out.push(RawParcel {
            front_mid: None,
            pts: sb.pts,
            interior: true,
        });
    }
}

/// Środek najdłuższej krawędzi frontowej — front kwartału traktowanego jako jedna parcela.
fn front_srodek(sb: &SubBlock) -> Option<Vec2> {
    let n = sb.pts.len();
    (0..n)
        .filter(|&i| sb.owner[i].rank() > 0)
        .max_by(|&i, &j| {
            let dl = |k: usize| (sb.pts[(k + 1) % n] - sb.pts[k]).length();
            dl(i).total_cmp(&dl(j)).then(j.cmp(&i))
        })
        .map(|i| (sb.pts[i] + sb.pts[(i + 1) % n]) * 0.5)
}

/// Podział pasa zabudowy na działki cięciami prostopadłymi do frontu.
fn tnij_pas(
    pas: &SubBlock,
    a: Vec2,
    d: Vec2,
    spec: ZoneSpec,
    r: &mut Rng,
    out: &mut Vec<RawParcel>,
    slivers: &mut u32,
) {
    if pas.pts.len() < 3 {
        return;
    }
    let (mut u0, mut u1) = (f32::MAX, f32::MIN);
    for p in &pas.pts {
        let u = (*p - a).dot(d);
        u0 = u0.min(u);
        u1 = u1.max(u);
    }
    let dlugosc = u1 - u0;
    if dlugosc < spec.front.0 * 0.5 {
        if poly::signed_area(&pas.pts) >= MIN_PARCEL_M2 {
            out.push(RawParcel {
                front_mid: Some(a + d * ((u0 + u1) * 0.5)),
                pts: pas.pts.clone(),
                interior: false,
            });
        } else {
            *slivers += 1;
        }
        return;
    }

    // Szerokości frontów z rozkładu trójkątnego; reszta poniżej minimum doklejana
    // do ostatniej działki — działka węższa od minimum nie powstaje nigdy.
    let mut granice: Vec<f32> = vec![u0];
    let mut u = u0;
    while u1 - u > spec.front.0 {
        let w = trojkatny(r, spec.front).min(u1 - u);
        u += w;
        if u1 - u < spec.front.0 {
            break;
        }
        granice.push(u);
    }
    granice.push(u1);

    let mut cur = pas.clone();
    for k in 1..granice.len() {
        let plaszczyzna = a + d * granice[k];
        let (kawalek, reszta) = if k + 1 == granice.len() {
            (cur.clone(), None)
        } else {
            (
                cut(&cur, plaszczyzna, -d, Owner::None),
                Some(cut(&cur, plaszczyzna, d, Owner::None)),
            )
        };
        let pole = poly::signed_area(&kawalek.pts);
        if kawalek.pts.len() >= 3 && pole >= MIN_PARCEL_M2 {
            let srodek = a + d * ((granice[k - 1] + granice[k]) * 0.5);
            out.push(RawParcel {
                front_mid: Some(srodek),
                pts: kawalek.pts,
                interior: false,
            });
        } else {
            *slivers += 1;
        }
        match reszta {
            Some(x) => cur = x,
            None => break,
        }
    }
}

/// Rozkład trójkątny (min, moda, max) — odwrócona dystrybuanta, `sqrt` z `det_math`.
fn trojkatny(r: &mut Rng, (lo, mode, hi): (f32, f32, f32)) -> f32 {
    let u = f64::from(r.gen_range_u32(10_001)) / 10_000.0;
    let (lo, mode, hi) = (f64::from(lo), f64::from(mode), f64::from(hi));
    let zakres = (hi - lo).max(1e-6);
    let prog = (mode - lo) / zakres;
    let v = if u < prog {
        lo + det_math::sqrt(u * zakres * (mode - lo))
    } else {
        hi - det_math::sqrt((1.0 - u) * zakres * (hi - mode))
    };
    v.clamp(lo, hi) as f32
}

// ── Przebieg 2: sieć ─────────────────────────────────────────────────────────────────

/// Podział segmentu w punkcie `p`. Zwraca węzeł podziału; druga połowa dopisywana
/// na koniec `segments`, więc wcześniejsze `SegmentId` pozostają ważne.
fn split_segment(net: &mut RoadNetwork, geom: &mut PolyArena, s: SegmentId, p: Vec2) -> NodeId {
    let seg = net.segments[s.0 as usize];
    let (na, nb) = (net.nodes[seg.a.0 as usize], net.nodes[seg.b.0 as usize]);
    let dl = (nb.pos - na.pos).length().max(1.0);
    let t = ((p - na.pos).dot((nb.pos - na.pos) / dl) / dl).clamp(0.0, 1.0);
    let z = na.z_dm + ((nb.z_dm - na.z_dm) as f32 * t) as i32;
    let n = NodeId(net.nodes.len() as u32);
    net.nodes.push(RoadNode {
        pos: p,
        z_dm: z,
        degree: 2,
        flags: NodeFlags::JUNCTION,
    });
    net.segments[s.0 as usize].b = n;
    net.segments[s.0 as usize].geom = geom.push(&[na.pos, p]);
    net.segments[s.0 as usize].length_dm = ((p - na.pos).length() * 10.0) as u32;
    let mut half = seg;
    half.a = n;
    half.geom = geom.push(&[p, nb.pos]);
    half.length_dm = ((nb.pos - p).length() * 10.0) as u32;
    net.segments.push(half);
    n
}

/// Dopisanie ulicy lokalnej między dwoma węzłami.
fn push_street(
    net: &mut RoadNetwork,
    geom: &mut PolyArena,
    a: NodeId,
    b: NodeId,
    class: RoadClass,
) {
    let spec = class.spec();
    let (pa, pb) = (net.nodes[a.0 as usize].pos, net.nodes[b.0 as usize].pos);
    let mut flags = RoadFlags::SIDEWALK;
    if spec.lanes_bwd == 0 {
        flags = flags.with(RoadFlags::ONEWAY);
    }
    if class.forbids_heavy() {
        flags = flags.with(RoadFlags::NO_HEAVY);
    }
    net.segments.push(RoadSegment {
        a,
        b,
        class,
        geom: geom.push(&[pa, pb]),
        structure: RoadStructure::AtGrade,
        lanes_fwd: spec.lanes_fwd,
        lanes_bwd: spec.lanes_bwd,
        row_m: spec.row_m,
        speed_kph: spec.speed_kph,
        max_tonnage_t: spec.max_tonnage_t,
        length_dm: ((pb - pa).length() * 10.0) as u32,
        district: UNASSIGNED_DISTRICT,
        flags,
    });
    net.nodes[a.0 as usize].degree = net.nodes[a.0 as usize].degree.saturating_add(1);
    net.nodes[b.0 as usize].degree = net.nodes[b.0 as usize].degree.saturating_add(1);
}

/// Przebudowa CSR sąsiedztwa i flag węzłów po dołożeniu ulic lokalnych.
fn rebuild_adjacency(net: &mut RoadNetwork) {
    let n = net.nodes.len();
    let mut degree = vec![0u8; n];
    for s in &net.segments {
        degree[s.a.0 as usize] = degree[s.a.0 as usize].saturating_add(1);
        degree[s.b.0 as usize] = degree[s.b.0 as usize].saturating_add(1);
    }
    let mut adj_start = vec![0u32; n + 1];
    for (i, d) in degree.iter().enumerate() {
        adj_start[i + 1] = adj_start[i] + u32::from(*d);
    }
    let mut kursor = adj_start.clone();
    let mut adj_items = vec![SegmentId(0); adj_start[n] as usize];
    for (i, s) in net.segments.iter().enumerate() {
        for end in [s.a, s.b] {
            let k = &mut kursor[end.0 as usize];
            adj_items[*k as usize] = SegmentId(i as u32);
            *k += 1;
        }
    }
    for (i, node) in net.nodes.iter_mut().enumerate() {
        node.degree = degree[i];
        let gate = NodeFlags(node.flags.0 & NodeFlags::GATE.0);
        node.flags = if degree[i] >= 3 {
            gate.with(NodeFlags::JUNCTION)
        } else if degree[i] == 1 {
            gate.with(NodeFlags::DEAD_END)
        } else {
            gate
        };
    }
    net.adj_start = adj_start;
    net.adj_items = adj_items;
}

// ── Przebieg 3: fronty ───────────────────────────────────────────────────────────────

/// Indeks „najbliższy odcinek ulicy": punkty próbkowane co pół komórki wzdłuż osi.
fn build_street_index(net: &RoadNetwork, map_m: f32) -> CsrGrid<SegmentId> {
    const CELL_M: u16 = 32;
    let spec = GridSpec::covering(
        magnat_spatial::Aabb2::new(Vec2::ZERO, Vec2::new(map_m, map_m)),
        CELL_M,
    );
    let mut items: Vec<(Vec2, SegmentId)> = Vec::new();
    for (i, s) in net.segments.iter().enumerate() {
        if !s.class.is_driveable() {
            continue;
        }
        let (a, b) = (net.nodes[s.a.0 as usize].pos, net.nodes[s.b.0 as usize].pos);
        let kroki = ((b - a).length() / f32::from(CELL_M) * 2.0).ceil().max(1.0) as u32;
        for k in 0..=kroki {
            items.push((a + (b - a) * (k as f32 / kroki as f32), SegmentId(i as u32)));
        }
    }
    CsrGrid::build(spec, items.into_iter())
}

/// Front działki: najbliższy odcinek jezdni i rzut krawędzi frontowej na jego oś.
fn frontage_for(
    net: &RoadNetwork,
    index: &CsrGrid<SegmentId>,
    pts: &[Vec2],
    mid: Vec2,
    gateway: bool,
) -> Frontage {
    let mut kandydaci: Vec<(f32, SegmentId)> = Vec::new();
    index.k_nearest(mid, 1, &mut kandydaci);
    let Some(&(_, seg)) = kandydaci.first() else {
        return Frontage::NONE;
    };
    let s = &net.segments[seg.0 as usize];
    let (a, b) = (net.nodes[s.a.0 as usize].pos, net.nodes[s.b.0 as usize].pos);
    let dl = (b - a).length().max(1.0);
    if gateway {
        let (_, t) = poly::closest_on_segment(a, b, mid);
        let pol = (GATEWAY_M * 0.5 / dl).min(0.49);
        return Frontage {
            seg,
            t0: (t - pol).clamp(0.0, 1.0),
            t1: (t + pol).clamp(0.0, 1.0),
        };
    }
    // Rzut wierzchołków działki leżących przy osi — to daje odcinek frontu, a nie punkt.
    let (mut t0, mut t1) = (f32::MAX, f32::MIN);
    for p in pts {
        let (q, t) = poly::closest_on_segment(a, b, *p);
        if (q - *p).length() <= f32::from(s.row_m) * 0.5 + 2.0 {
            t0 = t0.min(t);
            t1 = t1.max(t);
        }
    }
    // Pojedynczy wierzchołek przy osi daje `t0 == t1`, czyli front o zerowej długości —
    // a to jest dla testu T7 to samo co brak frontu. Taka działka dotyka ulicy rogiem
    // i dostaje przejazd, jak parcela wewnętrzna.
    if t0 > t1 || (t1 - t0) * dl < 1.0 {
        let (_, t) = poly::closest_on_segment(a, b, mid);
        let pol = (GATEWAY_M * 0.5 / dl).min(0.49);
        return Frontage {
            seg,
            t0: (t - pol).clamp(0.0, 1.0),
            t1: (t + pol).clamp(0.0, 1.0),
        };
    }
    Frontage { seg, t0, t1 }
}

// ── WP8 w całości ────────────────────────────────────────────────────────────────────

/// Sieć lokalna i parcele dla całego miasta.
#[must_use]
pub fn subdivide(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
    net: &mut RoadNetwork,
    geom: &mut PolyArena,
    blocks: &mut BlockSet,
    zones: &ZoneResult,
) -> ParcelSet {
    // ── 1. Geometria (sieć nietknięta) ───────────────────────────────────────────────
    let mut per_block: Vec<BlockGeometry> = Vec::with_capacity(blocks.blocks.len());
    for (bi, b) in blocks.blocks.iter().enumerate() {
        let zone = zones.zone[bi];
        let mut g = BlockGeometry {
            parcels: Vec::new(),
            streets: Vec::new(),
            slivers: 0,
        };
        if !zone.parcelled() {
            per_block.push(g);
            continue;
        }
        let pts = geom.get(b.poly).to_vec();
        let bounding = &blocks.bounding_items[b.bounding.start as usize..b.bounding.end as usize];
        if pts.len() < 3 || bounding.len() != pts.len() {
            per_block.push(g);
            continue;
        }
        let owner: Vec<Owner> = bounding
            .iter()
            .map(|s| {
                let seg = &net.segments[s.0 as usize];
                Owner::Road(*s, seg.class.rank(), seg.row_m)
            })
            .collect();

        let spec = zone_spec(zone);
        let mut r = rng(plan.seed, StreamId::Blocks, bi as u32, Tick(0));
        let mut leafs = Vec::new();
        split_recursive(
            SubBlock { pts, owner },
            spec,
            &mut r,
            0,
            &mut leafs,
            &mut g.streets,
        );

        let mut rp = rng(plan.seed, StreamId::Parcels, bi as u32, Tick(0));
        for leaf in leafs {
            strip_parcels(leaf, zone, spec, &mut rp, &mut g.parcels, &mut g.slivers);
        }
        per_block.push(g);
    }

    // ── 2. Sieć: podziały segmentów brzegowych, potem ulice lokalne ──────────────────
    //
    // Podziały wsadowo, po jednym segmencie naraz i w kolejności parametru: ten sam
    // segment bywa zaczepiony z obu sąsiadujących kwartałów, a dzielony pojedynczo
    // rozjechałby identyfikatory drugiemu z nich.
    let mut zadania: Vec<(u32, f32, u32, u8, Vec2)> = Vec::new();
    let mut wszystkie: Vec<(usize, usize)> = Vec::new(); // (kwartał, ulica)
    for (bi, g) in per_block.iter().enumerate() {
        for (si, st) in g.streets.iter().enumerate() {
            let idx = wszystkie.len() as u32;
            wszystkie.push((bi, si));
            for (koniec, ank) in [(0u8, st.anchor_a), (1u8, st.anchor_b)] {
                let Some(a) = ank else { continue };
                let seg = &net.segments[a.seg.0 as usize];
                let (p0, p1) = (
                    net.nodes[seg.a.0 as usize].pos,
                    net.nodes[seg.b.0 as usize].pos,
                );
                let (_, t) = poly::closest_on_segment(p0, p1, a.pos);
                zadania.push((a.seg.0, t, idx, koniec, a.pos));
            }
        }
    }
    zadania.sort_by(|x, y| {
        x.0.cmp(&y.0)
            .then(x.1.total_cmp(&y.1))
            .then(x.2.cmp(&y.2))
            .then(x.3.cmp(&y.3))
    });

    let mut wezly: Vec<[Option<NodeId>; 2]> = vec![[None, None]; wszystkie.len()];
    let mut splits = 0u32;
    let mut i = 0;
    while i < zadania.len() {
        let seg0 = zadania[i].0;
        let mut cur = SegmentId(seg0);
        let mut j = i;
        while j < zadania.len() && zadania[j].0 == seg0 {
            let (_, _, ulica, koniec, pos) = zadania[j];
            let n = zaczep_na_segmencie(net, geom, &mut cur, pos, &mut splits);
            wezly[ulica as usize][usize::from(koniec)] = Some(n);
            j += 1;
        }
        i = j;
    }

    // Ulice lokalne, kwartał po kwartale i w kolejności powstawania (rodzic przed
    // dzieckiem): koniec nieprzypięty do brzegu trafia w ulicę wyciętą wcześniej
    // w tym samym kwartale i **dzieli ją**, żeby styk był skrzyżowaniem, a nie
    // dwiema krawędziami mijającymi się bez węzła.
    let mut local_streets = 0u32;
    let mut zbyt_strome = 0u32;
    let mut lokalne: Vec<SegmentId> = Vec::new();
    let mut biezacy = usize::MAX;
    for (idx, &(bi, si)) in wszystkie.iter().enumerate() {
        if bi != biezacy {
            lokalne.clear();
            biezacy = bi;
        }
        let st = per_block[bi].streets[si];
        let a = wezly[idx][0]
            .unwrap_or_else(|| zaczep_na_lokalnych(net, geom, t, &mut lokalne, st.a, &mut splits));
        let b = wezly[idx][1]
            .unwrap_or_else(|| zaczep_na_lokalnych(net, geom, t, &mut lokalne, st.b, &mut splits));
        if a == b {
            continue;
        }
        // Ulica lokalna też podlega limitowi nachylenia swojej klasy. Jej rzędne są
        // narzucone przez ulice, które łączy, więc nie ma czego negocjować: gdy spadek
        // między nimi przekracza limit, ulica po prostu nie powstaje, a kwartał zostaje
        // niepodzielony. Działki policzone w przebiegu 1 dostaną front od strony
        // istniejącej drogi (przebieg 3 szuka najbliższego odcinka, nie „swojego").
        let (na, nb) = (net.nodes[a.0 as usize], net.nodes[b.0 as usize]);
        let dh = f64::from((nb.z_dm - na.z_dm).abs()) * 0.1;
        let dl = f64::from((nb.pos - na.pos).length()).max(1.0);
        if ((dh / dl) * 64.0) as u8 > st.class.spec().max_slope_units() {
            zbyt_strome += 1;
            continue;
        }
        lokalne.push(SegmentId(net.segments.len() as u32));
        push_street(net, geom, a, b, st.class);
        local_streets += 1;
    }
    rebuild_adjacency(net);

    // ── 3. Fronty, właściciele, indeksy ─────────────────────────────────────────────
    let street_index = build_street_index(net, plan.map_size_m() as f32);
    let mut parcels: Vec<Parcel> = Vec::new();
    let mut slivers = 0u32;
    for (bi, g) in per_block.into_iter().enumerate() {
        slivers += g.slivers;
        let start = parcels.len() as u32;
        let zone = zones.zone[bi];
        let dzielnica = blocks.blocks[bi].district;
        for rp in g.parcels {
            let pole = poly::signed_area(&rp.pts);
            if pole < MIN_PARCEL_M2 {
                slivers += 1;
                continue;
            }
            let duze_podworko = rp.interior && pole >= COURTYARD_MIN_M2;
            let strefa = if rp.interior && !duze_podworko {
                ZoneKind::Green
            } else {
                zone
            };
            let mid = rp.front_mid.unwrap_or_else(|| poly::centroid(&rp.pts));
            let frontage = if rp.interior && !duze_podworko {
                Frontage::NONE
            } else {
                frontage_for(net, &street_index, &rp.pts, mid, rp.interior)
            };
            let owner = wlasciciel(strefa);
            parcels.push(Parcel {
                block: BlockId(bi as u32),
                district: dzielnica,
                poly: geom.push(&rp.pts),
                area_m2: pole as u32,
                frontage,
                zone: strefa,
                owner,
                status: if strefa.is_residential() {
                    ParcelStatus::Reserved
                } else {
                    ParcelStatus::Vacant
                },
                land_value_per_m2: Money(0),
                building: None,
            });
        }
        blocks.blocks[bi].parcels = start..parcels.len() as u32;
        blocks.blocks[bi].zone = zone;
        blocks.blocks[bi].epoch_ring = zones.epoch_ring[bi];
    }

    let aabb: Vec<(magnat_spatial::Aabb2, magnat_core::ParcelId)> = parcels
        .iter()
        .enumerate()
        .map(|(i, p)| (obwiednia(geom.get(p.poly)), parcel_id(i as u32)))
        .collect();
    let tree = ParcelTree::build(&aabb);

    ParcelSet {
        parcels,
        tree,
        street_index,
        local_streets,
        splits,
        slivers,
        too_steep: zbyt_strome,
    }
}

/// Uchwyt parceli. Parcele nie są jeszcze encjami ECS (świat M2 powstaje bez `World`),
/// więc generacja jest stała — indeks niesie całą tożsamość.
#[must_use]
pub fn parcel_id(i: u32) -> magnat_core::ParcelId {
    magnat_core::ParcelId(magnat_core::Entity::new(
        i,
        std::num::NonZeroU32::new(1).expect("1 != 0"),
    ))
}

fn obwiednia(pts: &[Vec2]) -> magnat_spatial::Aabb2 {
    let mut a = magnat_spatial::Aabb2::new(Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in pts {
        a = a.union_point(*p);
    }
    a
}

/// Właściciel parceli w M2 (M2 §5.4, „Właściciele w M2"). Firmy dokłada M2e,
/// gospodarstwa domowe — M3 w Etapie 8.
fn wlasciciel(z: ZoneKind) -> ParcelOwner {
    match z {
        ZoneKind::Green | ZoneKind::Institutional | ZoneKind::Water | ZoneKind::Undevelopable => {
            ParcelOwner::City
        }
        z if z.is_residential() => ParcelOwner::City,
        _ => ParcelOwner::Unowned,
    }
}

fn dodaj_wezel(net: &mut RoadNetwork, t: &dyn TerrainQuery, p: Vec2) -> NodeId {
    // Scalenie z istniejącym węzłem w promieniu 3 m — spotkanie dwóch ulic lokalnych
    // w tym samym punkcie ma dać jedno skrzyżowanie, nie dwa.
    if let Some((i, _)) = net
        .nodes
        .iter()
        .enumerate()
        .rev()
        .take(512)
        .find(|(_, n)| (n.pos - p).length_squared() < 9.0)
    {
        return NodeId(i as u32);
    }
    let n = NodeId(net.nodes.len() as u32);
    net.nodes.push(RoadNode {
        pos: p,
        // `height_at` jest w jednostkach 0,5 m (K-13) — stąd ×5 na decymetry. Bez tego
        // węzeł swobodny miał rzędną 0 i ulica do niego schodziła z niwelety całego
        // miasta, łamiąc limit nachylenia klasy w miejscu, w którym nikt nic nie budował.
        z_dm: t.height_at(p.x as i32, p.y as i32) * 5,
        degree: 0,
        flags: NodeFlags::NONE,
    });
    n
}

/// Węzeł w punkcie `pos` na segmencie `cur`: istniejący koniec, gdy jest blisko,
/// inaczej podział. `cur` przesuwa się na drugą połowę, żeby kolejne zaczepienie
/// tego samego segmentu trafiło w odpowiedni kawałek.
fn zaczep_na_segmencie(
    net: &mut RoadNetwork,
    geom: &mut PolyArena,
    cur: &mut SegmentId,
    pos: Vec2,
    splits: &mut u32,
) -> NodeId {
    let s = net.segments[cur.0 as usize];
    let (p0, p1) = (net.nodes[s.a.0 as usize].pos, net.nodes[s.b.0 as usize].pos);
    let (q, t) = poly::closest_on_segment(p0, p1, pos);
    let dl = (p1 - p0).length();
    if t * dl < MIN_STUB_M {
        return s.a;
    }
    if (1.0 - t) * dl < MIN_STUB_M {
        return s.b;
    }
    let druga = SegmentId(net.segments.len() as u32);
    let n = split_segment(net, geom, *cur, q);
    *cur = druga;
    *splits += 1;
    n
}

/// Węzeł dla końca ulicy lokalnej, który nie dotyka brzegu kwartału: trafia w ulicę
/// wyciętą wcześniej w tym samym kwartale albo — gdy nie trafia w żadną — zakłada
/// węzeł swobodny.
#[allow(clippy::too_many_arguments)]
fn zaczep_na_lokalnych(
    net: &mut RoadNetwork,
    geom: &mut PolyArena,
    t: &dyn TerrainQuery,
    lokalne: &mut Vec<SegmentId>,
    pos: Vec2,
    splits: &mut u32,
) -> NodeId {
    for k in 0..lokalne.len() {
        let seg = lokalne[k];
        let s = net.segments[seg.0 as usize];
        let (p0, p1) = (net.nodes[s.a.0 as usize].pos, net.nodes[s.b.0 as usize].pos);
        let (q, t) = poly::closest_on_segment(p0, p1, pos);
        if (q - pos).length() > MIN_STUB_M {
            continue;
        }
        let dl = (p1 - p0).length();
        if t * dl < MIN_STUB_M {
            return s.a;
        }
        if (1.0 - t) * dl < MIN_STUB_M {
            return s.b;
        }
        let druga = SegmentId(net.segments.len() as u32);
        let n = split_segment(net, geom, seg, q);
        lokalne.push(druga);
        *splits += 1;
        return n;
    }
    dodaj_wezel(net, t, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prostokat(w: f32, h: f32) -> SubBlock {
        SubBlock {
            pts: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(w, 0.0),
                Vec2::new(w, h),
                Vec2::new(0.0, h),
            ],
            owner: vec![Owner::Road(SegmentId(0), 3, 12); 4],
        }
    }

    #[test]
    fn podzial_pasowy_nie_gubi_i_nie_dubluje_pola() {
        let sb = prostokat(120.0, 60.0);
        let spec = zone_spec(ZoneKind::Residential(super::super::zoning::ResDensity::R2));
        let mut r = rng(7, StreamId::Parcels, 0, Tick(0));
        let mut out = Vec::new();
        let mut sl = 0;
        let pole = sb.area();
        strip_parcels(
            sb,
            ZoneKind::Residential(super::super::zoning::ResDensity::R2),
            spec,
            &mut r,
            &mut out,
            &mut sl,
        );
        let suma: f64 = out.iter().map(|p| poly::signed_area(&p.pts)).sum();
        assert!(!out.is_empty(), "brak działek");
        assert!(
            (suma - pole).abs() < pole * 0.02,
            "suma działek {suma} wobec kwartału {pole}"
        );
    }

    #[test]
    fn zadna_dzialka_nie_jest_wezsza_od_minimum() {
        let spec = zone_spec(ZoneKind::Residential(super::super::zoning::ResDensity::R1));
        let mut r = rng(11, StreamId::Parcels, 0, Tick(0));
        let mut out = Vec::new();
        let mut sl = 0;
        strip_parcels(
            prostokat(200.0, 70.0),
            ZoneKind::Residential(super::super::zoning::ResDensity::R1),
            spec,
            &mut r,
            &mut out,
            &mut sl,
        );
        for p in &out {
            let pole = poly::signed_area(&p.pts);
            assert!(pole >= MIN_PARCEL_M2, "działka {pole} m²");
        }
    }

    #[test]
    fn rozklad_trojkatny_miesci_sie_w_widelkach() {
        let mut r = rng(3, StreamId::Parcels, 0, Tick(0));
        let w = (9.0, 14.0, 22.0);
        let mut suma = 0.0;
        for _ in 0..2000 {
            let v = trojkatny(&mut r, w);
            assert!((w.0..=w.2).contains(&v), "{v}");
            suma += v;
        }
        let srednia = suma / 2000.0;
        let oczekiwana = (w.0 + w.1 + w.2) / 3.0;
        assert!(
            (srednia - oczekiwana).abs() < 0.8,
            "średnia {srednia} wobec {oczekiwana}"
        );
    }

    #[test]
    fn ciecie_przenosi_wlasciciela_krawedzi() {
        let sb = prostokat(100.0, 100.0);
        let nowy = Owner::Street(3, 12);
        let a = cut(&sb, Vec2::new(50.0, 0.0), Vec2::new(1.0, 0.0), nowy);
        assert!(a.owner.contains(&nowy), "brak nowej krawędzi");
        assert!(
            a.owner.iter().filter(|o| o.seg().is_some()).count() >= 2,
            "krawędzie brzegowe zgubiły segment"
        );
    }
}
