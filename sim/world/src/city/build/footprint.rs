//! Geometria bryły: obrys na parceli, zapas na wysunięcia, `Aabb3` i wejścia
//! (M2 §5.6).
//!
//! Wydzielone z `city/build.rs` w R-WP8 bez zmiany zachowania.

use super::*;

// ── Obrys bryły ──────────────────────────────────────────────────────────────────────

/// Najmniejszy sensowny obrys: poniżej tego parcela zostaje pusta.
pub(super) const MIN_FOOTPRINT_M2: f32 = 24.0;
const MIN_BOK_M: f32 = 3.0;
/// Najwęższy front, przy którym w ogóle szukamy gramatyki. Poniżej — odpad podziału.
pub(super) const MIN_FRONT_M: f32 = 6.0;

/// Wyznacza obrys bryły na parceli: prostokąt w układzie osi frontu, cofnięty zgodnie
/// z `massing` i **zmieszczony w wielokącie działki** (test T6).
///
/// Kurczenie zamiast dokładnego wpisania prostokąta maksymalnego: działki z podziału
/// pasowego są prawie prostokątne, więc pierwsze przybliżenie prawie zawsze wystarcza,
/// a algorytm maksymalnego prostokąta w wielokącie jest o dwa rzędy wielkości droższy
/// i nie kupuje tu niczego (00, „Dobre praktyki": YAGNI).
#[must_use]
pub fn footprint_for(
    obrys: &[Vec2],
    front_dir: Option<Vec2>,
    front_point: Option<Vec2>,
    massing: &crate::city::grammar::Massing,
) -> Option<Scope> {
    if obrys.len() < 3 {
        return None;
    }
    let obb = poly::min_area_obb(obrys);
    let os = match front_dir {
        Some(d) if d.length_squared() > 0.25 => d.normalize(),
        _ => obb.axis,
    };
    let srodek = poly::centroid(obrys);

    // **Ulica jest zawsze po stronie „minus" osi `v`.** Kierunek odcinka drogi nie mówi,
    // po której jego stronie leży działka, więc bez tego obrotu `Face::Front` w gramatyce
    // wypadał na podwórzu mniej więcej co drugi budynek — witryna parteru usługowego
    // patrzyła w oficynę, a balkon z `Protrude(Front)` wychodziłby w głąb kwartału.
    // Obrót o 180° nie zmienia prostokąta, tylko jego orientację.
    let u = match front_point {
        Some(fp) if (fp - srodek).dot(Vec2::new(-os.y, os.x)) > 0.0 => -os,
        _ => os,
    };
    let v = Vec2::new(-u.y, u.x);

    // Rozpiętość działki w układzie (u, v).
    let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in obrys {
        let d = *p - srodek;
        let (du, dv) = (d.dot(u), d.dot(v));
        u0 = u0.min(du);
        u1 = u1.max(du);
        v0 = v0.min(dv);
        v1 = v1.max(dv);
    }

    // Front jest po stronie `v0` z założenia (obrót osi wyżej), więc cofnięcia idą
    // wprost, bez rozróżniania przypadków.
    let a = v0 + massing.setback_front_m;
    let mut b = v1 - massing.setback_back_m;
    // Trakt: budynek nie rozlewa się na całą głębokość działki.
    if b - a > massing.max_depth_m {
        b = a + massing.max_depth_m;
    }
    let (su0, su1) = (u0 + massing.setback_side_m, u1 - massing.setback_side_m);
    if su1 - su0 < MIN_BOK_M || b - a < MIN_BOK_M {
        return None;
    }

    let mut half_u = (su1 - su0) * 0.5;
    let mut half_v = (b - a) * 0.5;
    let mut c = srodek + u * ((su0 + su1) * 0.5) + v * ((a + b) * 0.5);

    // Kurczenie do skutku: osiem punktów kontrolnych (narożniki i środki boków).
    for _ in 0..8 {
        if miesci_sie(obrys, c, u, v, half_u, half_v) {
            break;
        }
        half_u *= 0.9;
        half_v *= 0.9;
        // Środek ciągniemy ku centroidowi działki — tam mieści się najłatwiej.
        c += (srodek - c) * 0.15;
    }
    if !miesci_sie(obrys, c, u, v, half_u, half_v) {
        return None;
    }
    if half_u < MIN_BOK_M * 0.5
        || half_v < MIN_BOK_M * 0.5
        || 4.0 * half_u * half_v < MIN_FOOTPRINT_M2
    {
        return None;
    }
    Some(Scope::new(
        glam::Vec3::new(c.x, c.y, 0.0),
        u,
        glam::Vec3::new(half_u, half_v, 0.0),
    ))
}

/// Największy wysięg z granicy działki nad chodnik — powyżej skrajni z `OVERHANG_MIN_Z_M`.
///
/// Nie mierzymy szerokości chodnika, tylko korzystamy z tego, że jest on **zawsze szerszy**:
/// jezdnia zajmuje 60 % pasa drogowego (`voxels.rs`), a najwęższy pas uliczny to `Service`
/// z 8 m, czyli 1,6 m pobocza z każdej strony. Wysięg 1,5 m nie dosięga więc jezdni w żadnej
/// klasie. Gdyby doszła klasa węższa niż 8 m, ta liczba przestaje być prawdziwa i trzeba
/// będzie liczyć pobocze z `row_m` odcinka frontowego.
pub(super) const OVERHANG_M: f32 = 1.5;

/// Krok i zasięg pomiaru zapasu. Ćwierć metra to czwarta część voxela poziomego —
/// dokładniej mierzyć nie ma po co, bo i tak wszystko wyląduje na siatce metrowej.
const ZAPAS_KROK_M: f32 = 0.25;

/// Ile bryła może urosnąć w danym kierunku, zanim wyjdzie z wielokąta działki.
///
/// Liniowo, nie połowieniem: zakres ma osiem kroków, a `miesci_sie` bada osiem punktów,
/// więc całość to 64 testy przynależności na kierunek — mniej, niż kosztowałoby
/// pilnowanie niezmienników wyszukiwania binarnego (00, „Dobre praktyki": boring over clever).
pub(super) fn zapas(
    obrys: &[Vec2],
    c: Vec2,
    u: Vec2,
    v: Vec2,
    hu: f32,
    hv: f32,
    dir: Axis2,
) -> f32 {
    let mut ostatni = 0.0;
    let mut d = ZAPAS_KROK_M;
    while d <= MAX_PROTRUDE_M + 1e-3 {
        let (pc, phu, phv) = match dir {
            Axis2::Front => (c - v * (d * 0.5), hu, hv + d * 0.5),
            Axis2::Back => (c + v * (d * 0.5), hu, hv + d * 0.5),
            Axis2::Side => (c, hu + d, hv),
        };
        if !miesci_sie(obrys, pc, u, v, phu, phv) {
            break;
        }
        ostatni = d;
        d += ZAPAS_KROK_M;
    }
    ostatni
}

/// Kierunek pomiaru zapasu. `Side` mierzy obie strony naraz, bo `Protrude(Side)`
/// stawia obie i obowiązuje ciaśniejsza.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Axis2 {
    Front,
    Back,
    Side,
}

/// Rozpiętość wielokąta w układzie `(u, v)`: szerokość wzdłuż `u` i głębokość wzdłuż `v`.
#[must_use]
pub fn rozpietosc(obrys: &[Vec2], u: Vec2) -> (f32, f32) {
    let v = Vec2::new(-u.y, u.x);
    let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in obrys {
        let (du, dv) = (p.dot(u), p.dot(v));
        u0 = u0.min(du);
        u1 = u1.max(du);
        v0 = v0.min(dv);
        v1 = v1.max(dv);
    }
    ((u1 - u0).max(0.0), (v1 - v0).max(0.0))
}

fn miesci_sie(obrys: &[Vec2], c: Vec2, u: Vec2, v: Vec2, hu: f32, hv: f32) -> bool {
    for (su, sv) in [
        (-1.0, -1.0),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-1.0, 0.0),
        (1.0, 0.0),
    ] {
        let p = c + u * (hu * su as f32) + v * (hv * sv as f32);
        if !poly::contains(obrys, p) {
            return false;
        }
    }
    true
}

pub(super) fn rogi(s: &Scope) -> [Vec2; 4] {
    let v = s.v();
    let c = Vec2::new(s.center.x, s.center.y);
    [
        c - s.u * s.half.x - v * s.half.y,
        c + s.u * s.half.x - v * s.half.y,
        c + s.u * s.half.x + v * s.half.y,
        c - s.u * s.half.x + v * s.half.y,
    ]
}

pub(super) fn bryla(p: &Planned) -> Aabb3 {
    let mut a = Aabb3::EMPTY;
    for r in rogi(&p.base) {
        a = a.union_point(glam::Vec3::new(
            r.x,
            r.y,
            p.base_z_m - p.derived.foundation_depth_m,
        ));
        a = a.union_point(glam::Vec3::new(
            r.x,
            r.y,
            p.base_z_m + f32::from(p.derived.height_dm) / 10.0,
        ));
    }
    // Balkony, wykusze i lukarny wychodzą poza obrys. Bez nich `aabb` odcinałby detal
    // przy krawędzi kadru w M11 — a to jest jedyny odbiorca tego pola (kontrakt §6).
    for part in &p.derived.parts {
        if part.carve {
            continue;
        }
        let (dol, gora) = (
            part.scope.center.z - part.scope.half.z,
            part.scope.center.z + part.scope.half.z,
        );
        for r in rogi(&part.scope) {
            a = a.union_point(glam::Vec3::new(r.x, r.y, dol));
            a = a.union_point(glam::Vec3::new(r.x, r.y, gora));
        }
    }
    a
}

/// Odległość od środka bryły do jej lica w zadanym kierunku — wyjście promienia
/// z prostopadłościanu w układzie osi budynku.
///
/// Wcześniej stało tu `half.y` niezależnie od kierunku, co dla hali 200 × 40 m stawiało
/// drzwi kilkadziesiąt metrów **wewnątrz** bryły albo **poza** nią, zależnie od tego,
/// z której strony leżała droga. Widać to było dopiero w teście T3, jako wejście bez
/// drogi w promieniu 50 m.
fn wyjscie_z_bryly(base: &Scope, kier: Vec2) -> f32 {
    let v = base.v();
    let (du, dv) = (kier.dot(base.u).abs(), kier.dot(v).abs());
    let tu = if du > 1e-4 {
        base.half.x / du
    } else {
        f32::MAX
    };
    let tv = if dv > 1e-4 {
        base.half.y / dv
    } else {
        f32::MAX
    };
    tu.min(tv)
}

/// Wejścia do budynku. Główne przy froncie; rampa — korekta D7 — szukana wśród
/// **wszystkich dróg w sąsiedztwie**, nie tylko na odcinku frontowym.
pub(super) fn wejscia(
    input: &BuildInput,
    parcels: &ParcelSet,
    p: &Planned,
) -> SmallVec<[Entrance; 4]> {
    let mut out: SmallVec<[Entrance; 4]> = SmallVec::new();
    let parcel = &parcels.parcels[p.parcel as usize];
    let srodek = Vec2::new(p.base.center.x, p.base.center.y);
    let z = p.base_z_m;

    if !parcel.frontage.is_none() {
        let t = (parcel.frontage.t0 + parcel.frontage.t1) * 0.5;
        let seg = &input.roads.segments[parcel.frontage.seg.0 as usize];
        let (a, b) = (
            input.roads.nodes[seg.a.0 as usize].pos,
            input.roads.nodes[seg.b.0 as usize].pos,
        );
        let na_ulicy = a + (b - a) * t;
        // Punkt na licu budynku od strony ulicy — tam, gdzie faktycznie są drzwi.
        let kier = (na_ulicy - srodek).normalize_or_zero();
        let lico = srodek + kier * wyjscie_z_bryly(&p.base, kier);
        out.push(Entrance {
            pos: glam::Vec3::new(lico.x, lico.y, z),
            seg: parcel.frontage.seg,
            t,
            kind: EntranceKind::Main,
        });
    }

    if matches!(
        parcel.zone,
        ZoneKind::Logistics | ZoneKind::IndustryHeavy | ZoneKind::IndustryLight
    ) {
        // Dwa promienie, nie jeden: hala w głębi strefy przemysłowej bywa otoczona
        // wyłącznie ulicami z zakazem ruchu ciężkiego w zasięgu 60 m, a droga dojazdowa
        // biegnie 150 m dalej. Zmierzone: przy jednym promieniu 12,7 % budynków stref
        // ciężkich zostawało bez rampy, przy dwóch — poniżej 2 % (korekta I-9).
        let bazowy = p.base.half.x.max(p.base.half.y);
        let mut najlepszy: Option<(i64, SegmentId, Vec2, f32)> = None;
        let mut kandydaci: Vec<SegmentId> = Vec::new();
        for promien in [bazowy + 60.0, bazowy + 220.0] {
            kandydaci.clear();
            parcels
                .street_index
                .query_radius(srodek, promien, &mut kandydaci);
            kandydaci.sort_unstable();
            kandydaci.dedup();
            for s in &kandydaci {
                let s = *s;
                let seg = &input.roads.segments[s.0 as usize];
                if seg.flags.contains(RoadFlags::NO_HEAVY)
                    || seg.class.is_rail()
                    || matches!(seg.class, RoadClass::Pedestrian)
                {
                    continue;
                }
                let (a, b) = (
                    input.roads.nodes[seg.a.0 as usize].pos,
                    input.roads.nodes[seg.b.0 as usize].pos,
                );
                let (punkt, t) = poly::closest_on_segment(a, b, srodek);
                // Klucz całkowitoliczbowy: odległość w centymetrach kwadratowych, remis
                // po numerze segmentu. Bez tego remis rozstrzygałaby kolejność w indeksie.
                let d = (punkt - srodek).length_squared();
                let klucz = (f64::from(d) * 100.0) as i64;
                if najlepszy.is_none_or(|(k, ids, _, _)| (klucz, s) < (k, ids)) {
                    najlepszy = Some((klucz, s, punkt, t));
                }
            }
            if najlepszy.is_some() {
                break;
            }
        }
        if let Some((_, s, punkt, t)) = najlepszy {
            let kier = (punkt - srodek).normalize_or_zero();
            let lico = srodek + kier * wyjscie_z_bryly(&p.base, kier);
            out.push(Entrance {
                pos: glam::Vec3::new(lico.x, lico.y, z),
                seg: s,
                t,
                kind: EntranceKind::Ramp,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::grammar::Massing;

    fn kwadrat(bok: f32) -> Vec<Vec2> {
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(bok, 0.0),
            Vec2::new(bok, bok),
            Vec2::new(0.0, bok),
        ]
    }

    fn massing(front: f32, bok: f32, tyl: f32) -> Massing {
        Massing {
            setback_front_m: front,
            setback_side_m: bok,
            setback_back_m: tyl,
            courtyard_min_m2: 0.0,
            max_depth_m: 100.0,
        }
    }

    #[test]
    fn obrys_miesci_sie_w_dzialce() {
        // Test T6 w miniaturze: wszystkie punkty kontrolne bryły leżą w wielokącie.
        let dzialka = kwadrat(30.0);
        let s = footprint_for(&dzialka, None, None, &massing(3.0, 2.0, 3.0)).expect("obrys");
        for r in rogi(&s) {
            assert!(poly::contains(&dzialka, r), "róg {r:?} poza działką");
        }
    }

    #[test]
    fn cofniecie_frontowe_idzie_od_ulicy() {
        // Ulica na dole (y = −5): budynek ma stać bliżej góry, nie odwrotnie.
        let dzialka = kwadrat(40.0);
        let s = footprint_for(
            &dzialka,
            Some(Vec2::new(1.0, 0.0)),
            Some(Vec2::new(20.0, -5.0)),
            &massing(12.0, 2.0, 2.0),
        )
        .expect("obrys");
        assert!(
            s.center.y > 20.0,
            "budynek stanął przy ulicy mimo cofnięcia 12 m (y = {})",
            s.center.y
        );
    }

    #[test]
    fn front_zawsze_patrzy_na_ulice() {
        // `Face::Front` w gramatyce to lico od ulicy. Kierunek odcinka drogi nie mówi,
        // po której stronie leży działka, więc bez obrotu osi witryna parteru wypadała
        // na podwórzu mniej więcej co drugi budynek — i żaden test tego nie widział.
        let dzialka = kwadrat(40.0);
        for ulica in [Vec2::new(20.0, -5.0), Vec2::new(20.0, 45.0)] {
            for kierunek in [Vec2::new(1.0, 0.0), Vec2::new(-1.0, 0.0)] {
                let s = footprint_for(
                    &dzialka,
                    Some(kierunek),
                    Some(ulica),
                    &massing(2.0, 2.0, 2.0),
                )
                .expect("obrys");
                let v = s.v();
                let do_ulicy = ulica - Vec2::new(s.center.x, s.center.y);
                assert!(
                    do_ulicy.dot(v) < 0.0,
                    "ulica {ulica:?} wypadła po dodatniej stronie osi v (kierunek {kierunek:?})"
                );
            }
        }
    }

    #[test]
    fn zapas_konczy_sie_na_granicy_dzialki() {
        // Bryła 20 × 20 pośrodku działki 30 × 30 ma po 5 m luzu z każdej strony,
        // ale `Protrude` i tak nie sięgnie dalej niż `MAX_PROTRUDE_M`.
        let dzialka = kwadrat(30.0);
        let c = Vec2::new(15.0, 15.0);
        let (u, v) = (Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0));
        for dir in [Axis2::Front, Axis2::Back, Axis2::Side] {
            let z = zapas(&dzialka, c, u, v, 10.0, 10.0, dir);
            assert!(
                (z - MAX_PROTRUDE_M).abs() < ZAPAS_KROK_M,
                "{dir:?}: zapas {z} m przy 5 m luzu i limicie {MAX_PROTRUDE_M} m"
            );
        }
        // Bryła wypełniająca działkę nie ma zapasu w żadną stronę.
        for dir in [Axis2::Front, Axis2::Back, Axis2::Side] {
            assert_eq!(zapas(&dzialka, c, u, v, 15.0, 15.0, dir), 0.0, "{dir:?}");
        }
    }

    #[test]
    fn za_mala_dzialka_nie_dostaje_budynku() {
        assert!(footprint_for(&kwadrat(6.0), None, None, &massing(3.0, 3.0, 3.0)).is_none());
    }

    /// Złożenie całej ścieżki przycinania: obrys z `footprint_for`, zapas z `zapas`,
    /// derywacja z katalogu z repo. To jest „rozszerzony T6" z kryterium WP18 —
    /// osobno przetestowane są obie połowy, ale dopiero tu widać, czy się składają.
    #[test]
    fn zadne_wysuniecie_nie_wychodzi_z_dzialki_ponizej_skrajni() {
        use crate::city::derive::{Margins, OVERHANG_MIN_Z_M};
        use crate::city::grammar::GrammarSet;

        let mats = magnat_voxel::MaterialRegistry::load_dir(&crate::assets::data_path("materials"))
            .expect("materiały");
        let set =
            GrammarSet::load_dir(&crate::assets::data_path("grammar"), &mats).expect("gramatyki");

        // Działka 24 × 40 z ulicą pod spodem. Prostokąt jest wypukły, więc ośmiopunktowy
        // test przynależności w `zapas` jest tu dokładny, a nie przybliżony.
        let dzialka = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(24.0, 0.0),
            Vec2::new(24.0, 40.0),
            Vec2::new(0.0, 40.0),
        ];
        let kierunek = Vec2::new(1.0, 0.0);
        let ulica = Vec2::new(12.0, -4.0);
        let base_z = 50.0;

        let mut widziane = 0;
        for g in set.all() {
            let Some(base) = footprint_for(&dzialka, Some(kierunek), Some(ulica), &g.massing)
            else {
                continue;
            };
            let c = Vec2::new(base.center.x, base.center.y);
            let v = base.v();
            let margins = Margins {
                front_m: zapas(
                    &dzialka,
                    c,
                    base.u,
                    v,
                    base.half.x,
                    base.half.y,
                    Axis2::Front,
                ),
                back_m: zapas(
                    &dzialka,
                    c,
                    base.u,
                    v,
                    base.half.x,
                    base.half.y,
                    Axis2::Back,
                ),
                side_m: zapas(
                    &dzialka,
                    c,
                    base.u,
                    v,
                    base.half.x,
                    base.half.y,
                    Axis2::Side,
                ),
                overhang_front_m: OVERHANG_M,
            };
            let out = derive::derive(
                g,
                &mats,
                base,
                BuildParams {
                    zone: ZoneKind::Residential(crate::city::zoning::ResDensity::R3),
                    epoch_ring: 1,
                    land_value_per_m2: (g.applies.land_value.0 + g.applies.land_value.1) / 2,
                    base_z_m: base_z,
                    world_seed: 0x00C0_FFEE,
                    building_index: 3,
                    margins,
                },
            );
            widziane += out.protrusions;
            for part in out.parts.iter().filter(|p| !p.carve) {
                let dol = part.scope.center.z - part.scope.half.z;
                for r in rogi(&part.scope) {
                    if poly::contains(&dzialka, r) {
                        continue;
                    }
                    assert!(
                        dol >= base_z + OVERHANG_MIN_Z_M - 1e-3,
                        "gramatyka {}: bryła poza działką na wysokości {:.2} m nad posadowieniem",
                        g.id,
                        dol - base_z
                    );
                    let d = odleglosc_do_wielokata(&dzialka, r);
                    assert!(
                        d <= OVERHANG_M + 0.05,
                        "gramatyka {}: wysięg {d:.2} m poza działkę przy limicie {OVERHANG_M} m",
                        g.id
                    );
                }
            }
        }
        assert!(
            widziane > 0,
            "żadna gramatyka z katalogu nie postawiła wysunięcia — test nie bada niczego"
        );
    }

    /// `wyjscie_z_bryly` ma trafiać w **lico** bryły, a nie w okrąg o promieniu `half.y`.
    /// Dla hali 200 × 40 m obie odpowiedzi różnią się o dziesiątki metrów, a błąd widać
    /// dopiero w T3 jako wejście bez drogi w promieniu — stąd test wprost na geometrii.
    #[test]
    fn wyjscie_z_bryly_trafia_w_lico() {
        let hala = Scope::new(
            glam::Vec3::ZERO,
            Vec2::new(1.0, 0.0),
            glam::Vec3::new(100.0, 20.0, 5.0),
        );
        let v = hala.v();
        for stopnie in 0u16..72 {
            let a = f32::from(stopnie) * 5.0_f32.to_radians();
            let kier = Vec2::new(a.cos(), a.sin());
            let p = kier * wyjscie_z_bryly(&hala, kier);
            // Punkt leży na brzegu prostopadłościanu: jedna ze współrzędnych lokalnych
            // dotyka swojego pół-boku, żadna go nie przekracza.
            let (du, dv) = ((p.dot(hala.u) / 100.0).abs(), (p.dot(v) / 20.0).abs());
            assert!(
                (du.max(dv) - 1.0).abs() < 1e-3,
                "kierunek {kier:?}: punkt {p:?} nie leży na licu (du={du}, dv={dv})"
            );
        }
    }

    fn odleglosc_do_wielokata(pts: &[Vec2], p: Vec2) -> f32 {
        let mut d = f32::MAX;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let (q, _) = poly::closest_on_segment(a, b, p);
            d = d.min((q - p).length());
        }
        d
    }
}
