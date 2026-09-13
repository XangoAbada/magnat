//! P9 — rozmieszczenie złóż (M1 §5.6, §5.5).
//!
//! Zarodki idą na **siatkę z losowym przesunięciem w komórce**, nie przez Poisson-disk
//! z odrzucaniem. Rozkład wychodzi praktycznie ten sam (minimalny odstęp jest wymuszony
//! przez rozmiar komórki), a koszt jest liniowy i — co ważniejsze — deterministyczny
//! bez żadnej dyscypliny: komórka `k` losuje ze strumienia `rng(seed, WorldDeposits, k, 0)`,
//! więc wynik nie zależy od kolejności przetwarzania ani od liczby wątków (00 §3.1).
//!
//! Typ złoża wybiera ruletka ważona `base × region × profil × epoka` z `data/geology/deposits.ron`.
//! Kształt przycinamy do formacji nośnej: ruda w granicie, węgiel w łupku, ropa pod pokrywą.

use crate::deposit::{Deposit, DepositId, DepositShape};
use crate::params::{EconomyProfile, Epoch, Region, WORK_CELL_M};
use crate::pipeline::GenCtx;
use magnat_core::{rng, IVec2, IVec3, Mass, ResourceKind, StreamId, Tick, Q};
use magnat_voxel::MaterialRegistry;
use serde::Deserialize;

pub const DEPOSITS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize)]
struct RegionWeights {
    mountain: u32,
    lowland: u32,
    river: u32,
    coastal: u32,
    desert: u32,
}

impl RegionWeights {
    fn of(&self, r: Region) -> u32 {
        match r {
            Region::Mountain => self.mountain,
            Region::Lowland => self.lowland,
            Region::River => self.river,
            Region::Coastal => self.coastal,
            Region::Desert => self.desert,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct ProfileWeights {
    industrial: u32,
    port: u32,
    university: u32,
    tourist: u32,
    agricultural: u32,
    mixed: u32,
}

impl ProfileWeights {
    fn of(&self, p: EconomyProfile) -> u32 {
        match p {
            EconomyProfile::Industrial => self.industrial,
            EconomyProfile::Port => self.port,
            EconomyProfile::University => self.university,
            EconomyProfile::Tourist => self.tourist,
            EconomyProfile::Agricultural => self.agricultural,
            EconomyProfile::Mixed => self.mixed,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct EpochWeights {
    y1950: u32,
    y1970: u32,
    y1990: u32,
    y2010: u32,
    y2020: u32,
}

impl EpochWeights {
    fn of(&self, e: Epoch) -> u32 {
        match e {
            Epoch::Y1950 => self.y1950,
            Epoch::Y1970 => self.y1970,
            Epoch::Y1990 => self.y1990,
            Epoch::Y2010 => self.y2010,
            Epoch::Y2020 => self.y2020,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct DepositType {
    kind: String,
    base_weight: u32,
    shape: String,
    host: Vec<String>,
    depth_min_m: i16,
    depth_max_m: i16,
    size_min_m: u32,
    size_max_m: u32,
    thickness_min_dm: u32,
    thickness_max_dm: u32,
    concentration_min: u8,
    concentration_max: u8,
    region: RegionWeights,
    profile: ProfileWeights,
    epoch: EpochWeights,
}

#[derive(Clone, Debug, Deserialize)]
struct DepositFile {
    schema_version: u32,
    seed_spacing_m: u32,
    types: Vec<DepositType>,
    historic_depletion_permille: EpochWeights,
}

pub fn run(ctx: &mut GenCtx) {
    let reg = MaterialRegistry::load_dir(&crate::data_path("materials")).expect("data/materials");
    let text = std::fs::read_to_string(crate::data_path("geology/deposits.ron"))
        .expect("data/geology/deposits.ron");
    let file: DepositFile = ron::from_str(&text).expect("data/geology/deposits.ron: składnia");
    assert_eq!(
        file.schema_version, DEPOSITS_SCHEMA_VERSION,
        "data/geology/deposits.ron: nieobsługiwana wersja schematu"
    );

    let params = ctx.params;
    let dim = ctx.dim();
    let map_m = params.size.meters();
    let spacing = file.seed_spacing_m.max(WORK_CELL_M);
    let cells = (map_m / spacing).max(1) as usize;

    // Wagi typów dla tego świata — liczone raz, nie per zarodek.
    let wagi = type_weights(&file.types, params);
    let suma: u64 = wagi.iter().sum();
    assert!(suma > 0, "wszystkie typy złóż mają wagę 0 dla tego świata");

    let depletion = file.historic_depletion_permille.of(params.epoch);
    let noise = crate::noise::NoiseField::new(params.seed, StreamId::WorldGeology, 0);

    let mut out: Vec<Deposit> = Vec::with_capacity(cells * cells / 2);
    for cy in 0..cells {
        for cx in 0..cells {
            let idx = (cy * cells + cx) as u32;
            let mut r = rng(params.seed, StreamId::WorldDeposits, idx, Tick(0));

            // Nie każda komórka rodzi złoże — inaczej mapa jest w całości zaminowana.
            if !r.gen_bool_permille(620) {
                continue;
            }

            // Pozycja z przesunięciem w obrębie komórki.
            let x_m = (cx as u32 * spacing + r.gen_range_u32(spacing)) as i32;
            let y_m = (cy as u32 * spacing + r.gen_range_u32(spacing)) as i32;
            let (gx, gy) = (
                (x_m / WORK_CELL_M as i32).clamp(0, dim as i32 - 1) as usize,
                (y_m / WORK_CELL_M as i32).clamp(0, dim as i32 - 1) as usize,
            );
            let surface_dm = i32::from(*ctx.world.height.get(gx, gy));

            // Ruletka ważona. Kolejność typów jest kolejnością z pliku — stabilna,
            // więc losowanie nie zmienia się od przestawienia wierszy w RON-ie… a jeśli
            // ktoś je przestawi, zmieni się i to jest zamierzone: plik danych jest
            // częścią tożsamości świata (00 §5).
            let mut los = r.next_u64() % suma;
            let mut typ = 0usize;
            for (i, w) in wagi.iter().enumerate() {
                if los < *w {
                    typ = i;
                    break;
                }
                los -= *w;
            }
            let t = &file.types[typ];

            let zakres = (t.depth_max_m - t.depth_min_m).max(0) as u32;
            let depth_m = t.depth_min_m + r.gen_range_u32(zakres + 1) as i16;
            let size_m = t.size_min_m + r.gen_range_u32(t.size_max_m - t.size_min_m + 1);
            let thickness_dm =
                t.thickness_min_dm + r.gen_range_u32(t.thickness_max_dm - t.thickness_min_dm + 1);
            let concentration = Q::new(
                t.concentration_min
                    + r.gen_range_u32(u32::from(t.concentration_max - t.concentration_min) + 1)
                        as u8,
            );

            // Strop złoża w decymetrach n.p.m. — zawsze poniżej terenu (M1 §7.2).
            let top_dm = surface_dm - i32::from(depth_m) * 10;
            let bottom_dm = top_dm - thickness_dm as i32;

            let shape = build_shape(
                &t.shape,
                &mut r,
                map_m as i32,
                x_m,
                y_m,
                top_dm,
                bottom_dm,
                size_m,
                thickness_dm,
                &reg,
                t,
            );
            let volume_m3 = shape_volume_m3(&shape, size_m);

            // Materiał nośny: ten, który faktycznie leży na głębokości złoża. Jeżeli
            // kolumna w tym miejscu nie ma żadnej z dopuszczalnych formacji, zarodek
            // przepada — tak wychodzi „ruda tylko w granicie" bez listy wyjątków w kodzie.
            let water_dist = *ctx.world.water_dist.get(gx / 4, gy / 4);
            let column = ctx.world.geology.column_at(
                &noise,
                x_m,
                y_m,
                surface_dm,
                slope_at(ctx, gx, gy),
                water_dist,
            );
            let z_voxel = top_dm / crate::geology::VOXEL_DM;
            let Some(host) = column.material_at(z_voxel) else {
                continue;
            };
            if !t.host.iter().any(|h| reg.id_of(h) == Some(host)) {
                continue;
            }

            let density = reg.get(host).density_kg_m3;
            let reserves = Deposit::reserves_from(volume_m3, density, concentration);
            let extracted = Mass(reserves.0 * i64::from(depletion) / 1000);

            out.push(Deposit {
                id: DepositId(out.len() as u32),
                resource: parse_kind(&t.kind),
                shape,
                volume_m3,
                concentration,
                reserves,
                extracted,
                depth_top_m: depth_m,
                depth_bottom_m: depth_m + (thickness_dm / 10) as i16,
                // Odkryte są tylko złoża płytkie — reszty szuka M6 badaniami geologicznymi.
                discovered: depth_m <= 25,
                quality: r.gen_q(),
            });
        }
    }

    ctx.world.deposits = out;
}

/// Waga każdego typu złoża dla zadanego świata: `base × region × profil × epoka`.
///
/// Wydzielone z `run`, bo to jedyny fragment doboru złóż, który da się sprawdzić
/// **bez generowania terenu**. Test na gotowej mapie mierzyłby co innego: tam o wyniku
/// decyduje przede wszystkim to, czy formacja nośna w ogóle występuje na danej głębokości,
/// i waga regionalna ginie pod tym filtrem.
fn type_weights(types: &[DepositType], p: crate::params::WorldGenParams) -> Vec<u64> {
    types
        .iter()
        .map(|t| {
            u64::from(t.base_weight)
                * u64::from(t.region.of(p.region))
                * u64::from(t.profile.of(p.profile))
                * u64::from(t.epoch.of(p.epoch))
        })
        .collect()
}

/// Nachylenie terenu w komórce, 0..=255 — ta sama definicja co w `TerrainQuery::slope_at`.
fn slope_at(ctx: &GenCtx, x: usize, y: usize) -> u8 {
    let h =
        |dx: i64, dy: i64| f32::from(*ctx.world.height.get_clamped(x as i64 + dx, y as i64 + dy));
    let dzdx = (h(1, 0) - h(-1, 0)) / (2.0 * WORK_CELL_M as f32 * 10.0);
    let dzdy = (h(0, 1) - h(0, -1)) / (2.0 * WORK_CELL_M as f32 * 10.0);
    let tan = magnat_core::det_math::sqrt(f64::from(dzdx * dzdx + dzdy * dzdy)) as f32;
    (tan * 64.0).clamp(0.0, 255.0) as u8
}

#[allow(clippy::too_many_arguments)]
fn build_shape(
    shape: &str,
    r: &mut magnat_core::Rng,
    map_m: i32,
    x_m: i32,
    y_m: i32,
    top_dm: i32,
    bottom_dm: i32,
    size_m: u32,
    thickness_dm: u32,
    reg: &MaterialRegistry,
    t: &DepositType,
) -> DepositShape {
    let half = (size_m / 2).max(1) as i32;
    match shape {
        "Seam" => {
            // Pokład biegnie łamaną o kilku kolanach — pokłady nie są prostokątami.
            let mut polyline = Vec::with_capacity(4);
            let (mut px, mut py) = (x_m, y_m);
            for _ in 0..4 {
                // Zaciśnięcie do mapy: pokład wychodzący poza świat nie ma czym być,
                // a jego środek geometryczny wypadałby poza zasięgiem zapytań M2.
                polyline.push(IVec2::new(px.clamp(0, map_m - 1), py.clamp(0, map_m - 1)));
                px += r.gen_range_u32(size_m) as i32 - half;
                py += r.gen_range_u32(size_m) as i32 - half;
            }
            DepositShape::Seam {
                polyline,
                top_z: top_dm,
                thickness_dm: thickness_dm.min(u32::from(u16::MAX)) as u16,
            }
        }
        "Trap" => DepositShape::Trap {
            center: IVec3::new(x_m, y_m, (top_dm + bottom_dm) / 2),
            radii: IVec3::new(half, half, ((top_dm - bottom_dm) / 2).max(1)),
            // Pokrywa uszczelniająca: pierwsza formacja nośna z listy, która istnieje.
            cap_layer: t.host.iter().find_map(|h| reg.id_of(h)).unwrap_or_default(),
        },
        "Aquifer" => {
            let mut poly = Vec::with_capacity(6);
            for (dx, dy) in HEX_DIR {
                // Sześciokąt o nieregularnych promieniach — kształt bez ostrych narożników,
                // a jednocześnie nie idealne koło.
                let promien = half / 2 + r.gen_range_u32(half.max(1) as u32) as i32;
                poly.push(IVec2::new(
                    (x_m + dx * promien / 100).clamp(0, map_m - 1),
                    (y_m + dy * promien / 100).clamp(0, map_m - 1),
                ));
            }
            DepositShape::Aquifer {
                poly,
                top_z: top_dm,
                bottom_z: bottom_dm,
            }
        }
        _ => DepositShape::Ellipsoid {
            center: IVec3::new(x_m, y_m, (top_dm + bottom_dm) / 2),
            radii: IVec3::new(
                half,
                half * (70 + r.gen_range_u32(61) as i32) / 100,
                ((top_dm - bottom_dm) / 2).max(1),
            ),
            yaw_deg: r.gen_range_u32(360) as u16,
        },
    }
}

/// Kierunki wierzchołków sześciokąta ×100 — stała tablica zamiast `sin`/`cos` w pętli.
const HEX_DIR: [(i32, i32); 6] = [
    (100, 0),
    (50, 87),
    (-50, 87),
    (-100, 0),
    (-50, -87),
    (50, -87),
];

/// Objętość formacji w m³, liczona całkowitoliczbowo.
///
/// Przybliżenia są jawne i wystarczające, bo objętość służy do policzenia masy surowca,
/// a ta i tak jest iloczynem trzech wielkości obarczonych rozrzutem rzędu dziesiątek procent.
/// Istotne jest co innego: wymiary muszą pochodzić **z tego samego kształtu**, który potem
/// odpowiada na pytanie „czy punkt należy do złoża" — inaczej kopalnia wydrąży coś innego,
/// niż zapowiadał bilans.
fn shape_volume_m3(shape: &DepositShape, size_m: u32) -> i64 {
    match shape {
        // Elipsoida: 4/3·π·abc ≈ 4,19·abc. Promień pionowy jest w decymetrach.
        DepositShape::Ellipsoid { radii, .. } | DepositShape::Trap { radii, .. } => {
            i64::from(radii.x) * i64::from(radii.y) * (i64::from(radii.z) / 10).max(1) * 419 / 100
        }
        // Pokład: długość łamanej × szerokość × miąższość.
        DepositShape::Seam {
            polyline,
            thickness_dm,
            ..
        } => {
            let mut dlugosc = 0i64;
            for w in polyline.windows(2) {
                dlugosc += magnat_core::det_math::sqrt(w[0].distance_sq(w[1]) as f64) as i64;
            }
            let szerokosc = i64::from(crate::deposit::seam_half_width_m(*thickness_dm)) * 2;
            dlugosc * szerokosc * (i64::from(*thickness_dm) / 10).max(1)
        }
        // Warstwa wodonośna: sześciokąt foremny ≈ 2,6·r².
        DepositShape::Aquifer {
            top_z, bottom_z, ..
        } => {
            let r = i64::from(size_m / 2).max(1);
            let h = (i64::from(top_z - bottom_z) / 10).max(1);
            r * r * 26 / 10 * h
        }
    }
    .max(1)
}

fn parse_kind(s: &str) -> ResourceKind {
    match s {
        "Coal" => ResourceKind::Coal,
        "IronOre" => ResourceKind::IronOre,
        "Oil" => ResourceKind::Oil,
        "Gas" => ResourceKind::Gas,
        "Aggregate" => ResourceKind::Aggregate,
        "Groundwater" => ResourceKind::Groundwater,
        "ClayDeposit" => ResourceKind::ClayDeposit,
        other => panic!("nieznany rodzaj surowca `{other}` w data/geology/deposits.ron"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn swiat(region: Region, epoch: Epoch, profile: EconomyProfile) -> GenCtx<'static> {
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(2)));
        let params = WorldGenParams {
            seed: 11,
            size: WorldSize::Small4km,
            region,
            epoch,
            profile,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        for pass in crate::pipeline::PASSES.iter().take(9) {
            (pass.run)(&mut ctx);
        }
        ctx
    }

    #[test]
    fn zloza_powstaja_i_leza_pod_terenem() {
        // M1 §7.2 `deposits_in_host_rock`.
        let ctx = swiat(Region::Mountain, Epoch::Y1990, EconomyProfile::Industrial);
        assert!(
            ctx.world.deposits.len() > 50,
            "tylko {} złóż na mapie 4 km",
            ctx.world.deposits.len()
        );
        for d in &ctx.world.deposits {
            assert!(d.depth_top_m >= 0, "złoże {} nad terenem", d.id.0);
            assert!(d.depth_bottom_m >= d.depth_top_m);
            assert!(d.reserves.0 > 0, "złoże {} bez zasobów", d.id.0);
            assert!(d.concentration.get() > 0);
        }
    }

    #[test]
    fn bilans_masy_zloz_jest_calkowitoliczbowy_i_nienaruszalny() {
        // Test własnościowy z 00 §6: `extracted ≤ reserves`, nigdy ujemne, bez przepełnienia.
        let mut ctx = swiat(Region::Lowland, Epoch::Y2010, EconomyProfile::Industrial);
        let suma_przed: i64 = ctx.world.deposits.iter().map(|d| d.remaining().0).sum();
        assert!(suma_przed > 0);

        let mut wydobyto = 0i64;
        for (i, d) in ctx.world.deposits.iter_mut().enumerate() {
            // Żądania większe niż zasoby, żeby wymusić przypadek brzegowy.
            let chce = Mass(d.reserves.0 / 3 * (1 + (i as i64 % 5)));
            wydobyto += d.extract(chce).0;
            assert!(
                d.extracted.0 <= d.reserves.0,
                "złoże {i} przekroczyło zasoby"
            );
            assert!(d.remaining().0 >= 0, "złoże {i} ma ujemną pozostałość");
        }
        let suma_po: i64 = ctx.world.deposits.iter().map(|d| d.remaining().0).sum();
        assert_eq!(
            suma_przed - suma_po,
            wydobyto,
            "masa nie bilansuje się: znikło {} g",
            suma_przed - suma_po - wydobyto
        );
    }

    #[test]
    fn epoka_zmienia_historyczne_wyeksploatowanie() {
        let stare = swiat(Region::Lowland, Epoch::Y1950, EconomyProfile::Industrial);
        let nowe = swiat(Region::Lowland, Epoch::Y2020, EconomyProfile::Industrial);
        let udzial = |c: &GenCtx| {
            let r: i64 = c.world.deposits.iter().map(|d| d.reserves.0).sum();
            let e: i64 = c.world.deposits.iter().map(|d| d.extracted.0).sum();
            e as f64 / r.max(1) as f64
        };
        assert!(udzial(&stare) < 0.05, "rok 1950 ma już wybrane złoża");
        assert!(
            udzial(&nowe) > 0.4,
            "rok 2020 zaczyna z dziewiczymi złożami"
        );
    }

    fn wczytaj_typy() -> Vec<DepositType> {
        let text = std::fs::read_to_string(crate::data_path("geology/deposits.ron")).unwrap();
        let f: DepositFile = ron::from_str(&text).unwrap();
        f.types
    }

    #[test]
    fn wagi_reaguja_na_region_profil_i_epoke() {
        let types = wczytaj_typy();
        let idx = |k: &str| types.iter().position(|t| t.kind == k).unwrap();
        let baza = WorldGenParams {
            size: WorldSize::Small4km,
            region: Region::Mountain,
            epoch: Epoch::Y1970,
            profile: EconomyProfile::Mixed,
            ..WorldGenParams::default()
        };

        let wegiel = idx("Coal");
        let przemysl = type_weights(
            &types,
            WorldGenParams {
                profile: EconomyProfile::Industrial,
                ..baza
            },
        );
        let turystyka = type_weights(
            &types,
            WorldGenParams {
                profile: EconomyProfile::Tourist,
                ..baza
            },
        );
        assert!(
            przemysl[wegiel] > turystyka[wegiel] * 3,
            "profil nie przesuwa wagi węgla"
        );

        let ropa = idx("Oil");
        let pustynia = type_weights(
            &types,
            WorldGenParams {
                region: Region::Desert,
                ..baza
            },
        );
        let gory = type_weights(&types, baza);
        assert!(
            pustynia[ropa] > gory[ropa] * 5,
            "region nie przesuwa wagi ropy"
        );

        let stara = type_weights(
            &types,
            WorldGenParams {
                epoch: Epoch::Y1950,
                ..baza
            },
        );
        let nowa = type_weights(
            &types,
            WorldGenParams {
                epoch: Epoch::Y2020,
                ..baza
            },
        );
        assert!(
            stara[wegiel] > nowa[wegiel],
            "epoka nie przesuwa wagi węgla"
        );
    }

    #[test]
    fn profil_przesuwa_strukture_zloz_na_gotowej_mapie() {
        // Na gotowej mapie waga jest tylko jednym z dwóch filtrów — drugim jest obecność
        // formacji nośnej na właściwej głębokości. Sprawdzamy więc kierunek, nie krotność.
        let przemysl = swiat(Region::Mountain, Epoch::Y1970, EconomyProfile::Industrial);
        let turystyka = swiat(Region::Mountain, Epoch::Y1970, EconomyProfile::Tourist);
        let ile = |c: &GenCtx, k: ResourceKind| {
            c.world.deposits.iter().filter(|d| d.resource == k).count()
        };
        assert!(
            ile(&przemysl, ResourceKind::Coal) > ile(&turystyka, ResourceKind::Coal),
            "profil przemysłowy nie dostał więcej węgla"
        );
    }

    #[test]
    fn rozmieszczenie_jest_deterministyczne() {
        let a = swiat(Region::River, Epoch::Y1990, EconomyProfile::Mixed);
        let b = swiat(Region::River, Epoch::Y1990, EconomyProfile::Mixed);
        assert_eq!(a.world.deposits, b.world.deposits);
    }
}
