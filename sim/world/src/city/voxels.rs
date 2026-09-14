//! WP12b — warstwa transportowa w voxelach (M2 §5.6b) i wspólne narzędzia zapisu.
//!
//! **M2 nie pisze do chunków, tylko kolejkuje edycje** (00 §3.4, korekta D4). Ten moduł
//! zamienia geometrię miasta na [`EditOp`]: korpus drogi, nawierzchnię, mosty, tunele
//! i niwelację parceli.
//!
//! ## Jednostki
//!
//! Voxel ma **1 m w poziomie i 0,5 m w pionie** (`CHUNK_SPAN_M`/`CHUNK_DIM` i
//! `VOXEL_HEIGHT_DM`). Cała geometria miasta jest w metrach, więc konwersja to
//! „x, y bez zmian, z razy dwa" — i jest zrobiona w jednym miejscu ([`obb`]), żeby
//! nie pojawiła się gdzieś indziej pomnożona przez pół.
//!
//! ## Kolejność warstw
//!
//! §5.6b rozstrzygał ją geometrią (podbudowa jeden voxel pod niweletą). To działa dla
//! `Terrace`, ale M2d kładzie **bryły zorientowane**: droga biegnie pod dowolnym kątem,
//! a `EditOp::Terrace` przyjmuje wyłącznie `IAabb3`, więc wyrównanie pasa drogowego
//! obejmowałoby przy 45° półtora raza za szeroki prostokąt — czyli sąsiednią działkę.
//! Zamiast tego niwelacja to **para brył**: wypełnienie poniżej niwelety i wykop powyżej.
//!
//! Kolejność stosowania rozstrzyga wtedy `EditSource`, bo źródło jest **pierwszym**
//! kluczem porządku kanonicznego. Numery poniżej są kolejnością robót na budowie:
//! najpierw ziemia, potem nawierzchnia, potem budynki, na końcu otwory.

use super::derive::Scope;
use super::districts::DistrictSet;
use super::road::{RoadClass, RoadNetwork, RoadStructure, UNASSIGNED_DISTRICT};
use magnat_spatial::Vec2;
use magnat_voxel::{CarveShape, EditOp, EditQueue, EditSource, MaterialId, MaterialRegistry, Obb3};

/// Korpus drogi — nasyp albo wykop pod podbudowę.
pub const SRC_ROAD_BED: EditSource = EditSource(10);
/// Wykop nad niweletą: ściana wykopu drogowego i prześwit pod mostem.
pub const SRC_ROAD_CUT: EditSource = EditSource(11);
/// Nawierzchnia jezdni i pomost mostu.
pub const SRC_ROAD_SURFACE: EditSource = EditSource(12);
/// Niwelacja parceli: podsypka do rzędnej posadowienia.
pub const SRC_PARCEL_PAD: EditSource = EditSource(20);
/// Niwelacja parceli: skarpa, czyli zdjęcie terenu powyżej rzędnej.
pub const SRC_PARCEL_CUT: EditSource = EditSource(21);
/// Bryły budynku.
pub const SRC_BUILDING: EditSource = EditSource(30);
/// Otwory w budynku — okna, bramy. **Po** bryłach, stąd wyższy numer.
pub const SRC_BUILDING_VOID: EditSource = EditSource(31);
/// Tunele. Na samym końcu: tunel drąży się przez wszystko, co nad nim postawiono.
pub const SRC_TUNNEL: EditSource = EditSource(40);

/// Wysokość w metrach → voxele pionowe (0,5 m).
#[must_use]
pub fn z_to_voxel(z_m: f32) -> f32 {
    z_m * 2.0
}

/// Zakres w metrach → bryła w voxelach.
#[must_use]
pub fn obb(s: &Scope) -> Obb3 {
    Obb3::new(
        [s.center.x, s.center.y, z_to_voxel(s.center.z)],
        [s.u.x, s.u.y],
        [s.half.x, s.half.y, z_to_voxel(s.half.z).max(0.25)],
    )
}

/// Prostopadłościan zorientowany zbudowany z geometrii poziomej i pionowego przedziału.
#[must_use]
pub fn prism(center_xy: Vec2, u: Vec2, half_u: f32, half_v: f32, z0_m: f32, z1_m: f32) -> Obb3 {
    let (a, b) = if z0_m <= z1_m {
        (z0_m, z1_m)
    } else {
        (z1_m, z0_m)
    };
    Obb3::new(
        [center_xy.x, center_xy.y, z_to_voxel((a + b) * 0.5)],
        [u.x, u.y],
        [half_u, half_v, (z_to_voxel(b - a) * 0.5).max(0.25)],
    )
}

/// Niwelacja pasa: wypełnienie do rzędnej `target_z_m` i wykop powyżej.
///
/// `depth_m` mówi, jak głęboko sięga podsypka (musi dojść do terenu w najniższym punkcie),
/// `cut_m` — jak wysoko sięga wykop (do terenu w najwyższym punkcie). Zero po którejś
/// stronie oznacza „tu nie ma nic do zrobienia" i komenda nie powstaje: pusta bryła
/// kosztowałaby tyle samo w indeksie co pełna.
#[allow(clippy::too_many_arguments)]
pub fn level_strip(
    q: &EditQueue,
    src_fill: EditSource,
    src_cut: EditSource,
    center_xy: Vec2,
    u: Vec2,
    half_u: f32,
    half_v: f32,
    target_z_m: f32,
    depth_m: f32,
    cut_m: f32,
    material: MaterialId,
) {
    if depth_m > 0.05 {
        q.push(
            src_fill,
            EditOp::Prism {
                obb: prism(
                    center_xy,
                    u,
                    half_u,
                    half_v,
                    target_z_m - depth_m,
                    target_z_m,
                ),
                material,
            },
        );
    }
    if cut_m > 0.05 {
        q.push(
            src_cut,
            EditOp::Carve {
                shape: CarveShape::Prism(prism(
                    center_xy,
                    u,
                    half_u,
                    half_v,
                    target_z_m,
                    target_z_m + cut_m,
                )),
            },
        );
    }
}

/// Nawierzchnia klasy drogi w danym pierścieniu epoki.
fn surface_key(class: RoadClass, epoch_ring: u8, rings: u8) -> &'static str {
    if class.is_rail() {
        return "rail_ballast";
    }
    // Dwa najstarsze pierścienie i najwęższe klasy: bruk. Reszta: asfalt.
    let stary = rings >= 3 && epoch_ring < 2;
    match class {
        RoadClass::Pedestrian | RoadClass::Service => "cobble",
        _ if stary => "cobble",
        _ => "asphalt",
    }
}

/// Raport warstwy transportowej — wchodzi do `GenerationReport`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RoadVoxelReport {
    pub segments: u32,
    pub commands: u32,
    pub bridges: u32,
    pub tunnels: u32,
    /// Segmenty, dla których różnica niwelety wymusiła podział na stopnie.
    pub stepped: u32,
}

/// Długość jednego stopnia niwelety. Bryła jest prostopadłościanem, więc droga o spadku
/// musi być schodkowana; 32 m to kompromis między liczbą komend a widocznością stopnia
/// (przy spadku 7% stopień ma 2,2 m, czyli mniej niż szerokość pasa).
const STEP_M: f32 = 32.0;

/// Kolejkuje warstwę transportową całej sieci (§5.6b).
///
/// Segmenty idą w kolejności tablicy, ale kolejność i tak nie wpływa na wynik: porządek
/// kanoniczny `EditQueue` wyprowadza się z treści komendy, nie z kolejności kolejkowania.
#[must_use]
pub fn queue_roads(
    q: &EditQueue,
    net: &RoadNetwork,
    districts: &DistrictSet,
    t: &dyn crate::query::TerrainQuery,
    mats: &MaterialRegistry,
) -> RoadVoxelReport {
    let rings = districts
        .districts
        .iter()
        .map(|d| d.founded_epoch.0)
        .max()
        .unwrap_or(0)
        + 1;
    let bed = mats.expect_id("roadbed");
    let mut rep = RoadVoxelReport::default();
    let przed = q.len();

    for s in &net.segments {
        if matches!(s.class, RoadClass::Pedestrian) {
            continue;
        }
        let (a, b) = (net.nodes[s.a.0 as usize].pos, net.nodes[s.b.0 as usize].pos);
        let (za, zb) = (
            net.nodes[s.a.0 as usize].z_dm as f32 / 10.0,
            net.nodes[s.b.0 as usize].z_dm as f32 / 10.0,
        );
        let dl = (b - a).length();
        if dl < 0.5 {
            continue;
        }
        let u = (b - a) / dl;
        let ring = if s.district == UNASSIGNED_DISTRICT {
            0
        } else {
            districts.districts[s.district.0 as usize].founded_epoch.0
        };
        let nawierzchnia = mats.expect_id(surface_key(s.class, ring, rings));
        // Jezdnia zajmuje część pasa drogowego; reszta to chodnik i zieleń, których
        // M2 nie modeluje osobno — one są detalem wizualnym (M11).
        let pol_szer = f32::from(s.row_m) * 0.30;

        // Liczba stopni: tyle, żeby jeden stopień nie przekraczał pół voxela wysokości.
        let dz = (zb - za).abs();
        let max_krokow = (dl / STEP_M).floor().max(1.0) as i32;
        let n = ((dz / 0.25).ceil() as i32).clamp(1, max_krokow);
        if n > 1 {
            rep.stepped += 1;
        }
        rep.segments += 1;

        for i in 0..n {
            let t0 = i as f32 / n as f32;
            let t1 = (i + 1) as f32 / n as f32;
            let srodek = a + (b - a) * ((t0 + t1) * 0.5);
            let pol_dl = dl * (t1 - t0) * 0.5;
            let z = za + (zb - za) * ((t0 + t1) * 0.5);

            match s.structure {
                RoadStructure::Tunnel { .. } => {
                    // Tunel: sam wykop w skale, bez korpusu i bez nawierzchni na wierzchu.
                    q.push(
                        SRC_TUNNEL,
                        EditOp::Carve {
                            shape: CarveShape::Prism(prism(
                                srodek,
                                u,
                                pol_dl,
                                pol_szer,
                                z - 0.5,
                                z + 5.5,
                            )),
                        },
                    );
                    if i == 0 {
                        rep.tunnels += 1;
                    }
                }
                RoadStructure::Bridge { clearance_dm } => {
                    // Pomost na niwelecie i prześwit pod nim. Przęsła i pylony to M11.
                    q.push(
                        SRC_ROAD_CUT,
                        EditOp::Carve {
                            shape: CarveShape::Prism(prism(
                                srodek,
                                u,
                                pol_dl,
                                pol_szer,
                                z - f32::from(clearance_dm) / 10.0 - 1.0,
                                z - 0.6,
                            )),
                        },
                    );
                    q.push(
                        SRC_ROAD_SURFACE,
                        EditOp::Prism {
                            obb: prism(srodek, u, pol_dl, pol_szer, z - 0.6, z),
                            material: nawierzchnia,
                        },
                    );
                    if i == 0 {
                        rep.bridges += 1;
                    }
                }
                RoadStructure::AtGrade | RoadStructure::Embankment { .. } => {
                    // Ile trzeba dosypać i ile zdjąć: skrajne wysokości terenu pod pasem.
                    let (lo, hi) = teren_pod_pasem(t, srodek, u, pol_dl, pol_szer);
                    let dosyp = (z - 0.5 - lo).max(0.0) + 0.5;
                    let zdejmij = (hi - z).max(0.0) + 0.5;
                    level_strip(
                        q,
                        SRC_ROAD_BED,
                        SRC_ROAD_CUT,
                        srodek,
                        u,
                        pol_dl,
                        pol_szer,
                        z - 0.5,
                        dosyp,
                        zdejmij + 0.5,
                        bed,
                    );
                    q.push(
                        SRC_ROAD_SURFACE,
                        EditOp::Prism {
                            obb: prism(srodek, u, pol_dl, pol_szer, z - 0.5, z),
                            material: nawierzchnia,
                        },
                    );
                }
            }
        }
    }
    rep.commands = (q.len() - przed) as u32;
    rep
}

/// Najniższa i najwyższa rzędna terenu pod prostokątnym pasem, w metrach.
/// Próbkowanie co ~4 m — dokładniej nie ma sensu, bo i tak wyrównujemy do jednej rzędnej.
pub fn teren_pod_pasem(
    t: &dyn crate::query::TerrainQuery,
    center: Vec2,
    u: Vec2,
    half_u: f32,
    half_v: f32,
) -> (f32, f32) {
    let v = Vec2::new(-u.y, u.x);
    let nu = ((half_u / 2.0).ceil() as i32).clamp(1, 24);
    let nv = ((half_v / 2.0).ceil() as i32).clamp(1, 12);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for i in -nu..=nu {
        for j in -nv..=nv {
            let p =
                center + u * (half_u * i as f32 / nu as f32) + v * (half_v * j as f32 / nv as f32);
            // `height_at` jest w jednostkach 0,5 m (K-13).
            let h = t.height_at(p.x as i32, p.y as i32) as f32 * 0.5;
            lo = lo.min(h);
            hi = hi.max(h);
        }
    }
    if lo > hi {
        (0.0, 0.0)
    } else {
        (lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zrodla_ida_w_kolejnosci_robot() {
        // Źródło jest pierwszym kluczem porządku kanonicznego, więc te nierówności
        // **są** kolejnością stosowania edycji: ziemia, nawierzchnia, budynek, otwór.
        assert!(SRC_ROAD_BED < SRC_ROAD_CUT);
        assert!(SRC_ROAD_CUT < SRC_ROAD_SURFACE);
        assert!(SRC_ROAD_SURFACE < SRC_PARCEL_PAD);
        assert!(SRC_PARCEL_CUT < SRC_BUILDING);
        assert!(SRC_BUILDING < SRC_BUILDING_VOID);
        assert!(SRC_BUILDING_VOID < SRC_TUNNEL);
    }

    #[test]
    fn bryla_ze_skosnego_pasa_nie_jest_szersza_niz_pas() {
        // Sedno korekty: `Terrace` na `IAabb3` objąłby przy 45° pas √2 razy szerszy.
        let p = prism(
            Vec2::new(100.0, 100.0),
            Vec2::new(1.0, 1.0),
            20.0,
            5.0,
            10.0,
            11.0,
        );
        let mut n = 0;
        let a = p.aabb();
        for y in a.min.y..a.max.y {
            for x in a.min.x..a.max.x {
                if p.contains_voxel(x, y, 21) {
                    n += 1;
                }
            }
        }
        // Pole pasa 40 × 10 m = 400 m², czyli 400 voxeli poziomych (voxel ma 1 m²).
        assert!(
            (340..=470).contains(&n),
            "pas o polu 400 m² zajął {n} voxeli — bryła nie trzyma szerokości"
        );
    }
}
