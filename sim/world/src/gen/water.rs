//! P7 — klasyfikacja wód, geometria hydrauliczna i wektorowa sieć koryt (M1 §5.7).
//!
//! Przebieg zamienia dwa pola liczbowe (wysokość po erozji, akumulacja spływu) na to,
//! czego naprawdę potrzebują kolejne fazy: gdzie jest morze, gdzie jezioro, którędy płynie
//! rzeka, jak szeroka i jak głęboka, i jak to wszystko jest ze sobą połączone.
//!
//! **Wektor jest źródłem prawdy o rzece, nie raster** (M1 ryzyko R2). Erozja liczy się na
//! siatce 4 m, a koryta miejskie mają 20–100 m szerokości; przy materializacji 1 m koryto
//! wcina się z polilinii, dzięki czemu zachowuje ciągłość, której raster 4 m nie utrzyma.

use crate::data::{LakeCells, RiverCell, RiverNetwork, RiverSegment, WaterBits, WaterClass};
use crate::fields::NO_RECEIVER;
use crate::gen::shape::RegionShape;
use crate::params::WORK_CELL_M;
use crate::pipeline::GenCtx;
use magnat_core::det_math;

/// Najmniejsza zlewnia, od której mówimy „rzeka".
///
/// M1 §5.7 podaje 0,25 km². Przy takim progu mapa 4 km (16,8 km² powierzchni) dostaje sieć
/// o najwyższym rzędzie Strahlera 2 — czyli główny ciek z pojedynczymi dopływami, bez
/// rozgałęzień. 0,1 km² daje sieć trzeciego rzędu na mapie 4 km i czwartego na 16 km,
/// czyli to, co wygląda jak dorzecze. Odnotowane jako odstępstwo od liczby w planie.
const RIVER_MIN_AREA_M2: f32 = 100_000.0;
/// Najmniejsze wypełnienie uznawane za jezioro.
///
/// Poniżej tego progu mamy zastoisko, nie zbiornik — i P12 nazwie je mokradłem, bo taka
/// jest prawda o rozlewisku po dziesięciu centymetrach wody. Próg ma też skutek pamięciowy:
/// każda komórka jeziora kosztuje 8 B w stanie trwałym, więc rozlewiska sięgające 20 %
/// mapy wysadzały budżet §5.9 (71 MB wobec 60 MB).
const LAKE_MIN_DEPTH_M: f32 = 1.0;
/// Udział opadu, który spływa powierzchniowo (reszta wsiąka i paruje).
const RUNOFF_COEFFICIENT: f64 = 0.35;
const SECONDS_PER_YEAR: f64 = 31_536_000.0;
/// Stosunek przepływu brzegowego do średniego rocznego.
///
/// Koryto formuje przepływ brzegowy — stan, przy którym woda sięga krawędzi terasy —
/// a nie przepływ średni. Bez tego mnożnika geometria hydrauliczna daje rzeki o szerokości
/// 2 m tam, gdzie mają mieć 10, bo `w = a·Q^b` bierze `Q` właśnie brzegowe.
const BANKFULL_RATIO: f64 = 20.0;
/// Dolne ograniczenia koryta. Geometria hydrauliczna przy progu 0,25 km² daje rzekę
/// szeroką na 1,7 m i głęboką na 7 cm — to jest rów melioracyjny, a nie rzeka, i przy
/// zaokrągleniu do decymetra ginie do zera. Skoro komórka została **nazwana** rzeką,
/// ma mieć koryto, które widać na mapie i w które da się wpłynąć.
const MIN_RIVER_WIDTH_M: f64 = 2.0;
const MIN_RIVER_DEPTH_M: f64 = 0.3;

pub fn run(ctx: &mut GenCtx) {
    let dim = ctx.dim();
    let n = dim * dim;
    let has_sea = ctx.params.region.has_sea();
    let shape = RegionShape::of(ctx.params.region);
    let precip_m_yr = f64::from(annual_precip_mm(shape.humidity)) / 1000.0;

    // ── 1. Klasyfikacja ──────────────────────────────────────────────────────────────
    let mut class = vec![WaterClass::Dry; n];
    let mut lake_cells = LakeCells::default();
    for (i, class) in class.iter_mut().enumerate() {
        let h = ctx.work.height_m[i];
        if has_sea && h <= 0.0 {
            *class = WaterClass::Sea;
            continue;
        }
        let zapas = ctx.work.filled_m[i] - h;
        if zapas >= LAKE_MIN_DEPTH_M {
            *class = WaterClass::Lake;
            lake_cells.push(i as u32, to_dm(ctx.work.filled_m[i]));
            continue;
        }
        if ctx.work.flow_acc[i] >= RIVER_MIN_AREA_M2 {
            *class = WaterClass::River;
        }
    }

    // ── 2. Rząd Strahlera ────────────────────────────────────────────────────────────
    // W tył po stosie: donorzy przed odbiornikiem, więc dopływ ma już swój rząd, zanim
    // policzy go rzeka główna.
    let mut strahler = vec![0u8; n];
    for c in ctx.work.stack.iter().rev() {
        let c = *c as usize;
        if class[c] != WaterClass::River {
            continue;
        }
        let (a, b) = (
            ctx.work.donor_start[c] as usize,
            ctx.work.donor_start[c + 1] as usize,
        );
        let mut max = 0u8;
        let mut ile_max = 0u32;
        for d in &ctx.work.donors[a..b] {
            let o = strahler[*d as usize];
            if o == 0 {
                continue;
            }
            if o > max {
                max = o;
                ile_max = 1;
            } else if o == max {
                ile_max += 1;
            }
        }
        strahler[c] = match (max, ile_max) {
            (0, _) => 1,               // źródło
            (m, k) if k >= 2 => m + 1, // dwa dopływy równego rzędu — rząd rośnie
            (m, _) => m,
        };
    }

    // ── 3. Geometria hydrauliczna i wcięcie koryta ───────────────────────────────────
    let mut river_cells: Vec<RiverCell> = Vec::new();
    // Wcinamy w kolejności stosu (od ujścia w górę), żeby móc wymusić monotoniczny spadek
    // dna: dno komórki musi leżeć wyżej niż dno jej odbiornika, inaczej wcięcie tworzy
    // zagłębienie w korycie, czyli dokładnie to, co P4 przed chwilą usunął.
    let mut bed = ctx.work.height_m.as_slice().to_vec();
    for c in ctx.work.stack.clone() {
        let c = c as usize;
        if class[c] != WaterClass::River {
            continue;
        }
        let q_bankfull = bankfull_discharge(f64::from(ctx.work.flow_acc[c]), precip_m_yr);
        let width_m = (2.5 * det_math::sqrt(q_bankfull)).max(MIN_RIVER_WIDTH_M);
        let depth_m = (0.25 * det_math::pow(q_bankfull, 0.4)).max(MIN_RIVER_DEPTH_M);
        // Zwierciadłem wody jest powierzchnia **wypełniona** z P4, nie surowy teren.
        // To nie jest drobiazg: powierzchnia wypełniona jest z definicji monotoniczna
        // w dół biegu, a surowy teren w płytko zalanych misach nie jest. Liczenie koryta
        // od terenu dawało rzeki o zerowej głębokości tam, gdzie korekta monotoniczności
        // zjadała całe wcięcie — czyli przy ujściach, gdzie widać je najlepiej.
        let surface = ctx.work.filled_m[c];
        // Dno nigdy nie podnosi terenu: jeżeli teren i tak jest niżej, zostaje jak był
        // (koryto wychodzi głębsze, co jest prawdą o tym miejscu, a nie błędem).
        let nowe_dno = (surface - depth_m as f32).min(ctx.work.height_m[c]);
        bed[c] = nowe_dno;

        river_cells.push(RiverCell {
            cell: c as u32,
            strahler: strahler[c],
            // Zaokrąglenie, nie obcięcie: minimalna głębokość 0,3 m w arytmetyce `f32`
            // wypada czasem jako 2,9999 dm i obcięcie robi z niej 2 dm — czyli koryto
            // płytsze niż deklarowany próg, i to akurat dla najmniejszych rzek.
            depth_dm: ((surface - nowe_dno) * 10.0 + 0.5).clamp(1.0, f32::from(u16::MAX)) as u16,
            width_dm: (width_m * 10.0 + 0.5).clamp(0.0, f64::from(u16::MAX)) as u16,
            surface_dm: to_dm(surface),
        });
    }
    river_cells.sort_unstable_by_key(|r| r.cell);
    ctx.work.height_m.as_mut_slice().copy_from_slice(&bed);

    // ── 4. Bity wody ─────────────────────────────────────────────────────────────────
    for (i, klasa) in class.iter().enumerate() {
        let r = ctx.work.receiver[i];
        let sink = r == NO_RECEIVER;
        let dir = if sink {
            0
        } else {
            neighbor_index(i, r as usize, dim)
        };
        ctx.world.water[i] = WaterBits::new(*klasa, dir, sink);
    }

    ctx.world.rivers = build_network(ctx, &class, &strahler, &river_cells);
    ctx.world.river_cells = river_cells;
    ctx.world.lake_cells = lake_cells;
    crate::gen::commit_height(ctx);
}

/// Roczna suma opadu przypisana wilgotności bazowej regionu.
///
/// P7 biegnie **przed** modelem klimatu (P10–P11), bo klimat potrzebuje już wyerodowanego
/// terenu i rzek. Do geometrii koryta wystarcza średnia regionalna: szerokość rzeki zależy
/// od pierwiastka z przepływu, więc 20 % błędu w opadzie to 10 % w szerokości.
#[must_use]
pub fn annual_precip_mm(humidity: u8) -> u16 {
    150 + u16::from(humidity) * 21 / 2
}

/// Przepływ brzegowy w m³/s dla zlewni o zadanym polu.
fn bankfull_discharge(area_m2: f64, precip_m_yr: f64) -> f64 {
    area_m2 * precip_m_yr * RUNOFF_COEFFICIENT / SECONDS_PER_YEAR * BANKFULL_RATIO
}

#[inline]
fn to_dm(m: f32) -> i16 {
    let dm = if m >= 0.0 {
        (m * 10.0 + 0.5) as i32
    } else {
        (m * 10.0 - 0.5) as i32
    };
    dm.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

/// Indeks sąsiada w [`crate::grid::NEIGHBORS_8`] prowadzącego z `from` do `to`.
fn neighbor_index(from: usize, to: usize, dim: usize) -> u8 {
    let (fx, fy) = ((from % dim) as i64, (from / dim) as i64);
    let (tx, ty) = ((to % dim) as i64, (to / dim) as i64);
    let (dx, dy) = ((tx - fx) as i8, (ty - fy) as i8);
    crate::grid::NEIGHBORS_8
        .iter()
        .position(|n| *n == (dx, dy))
        .unwrap_or(0) as u8
}

/// Buduje wektorową sieć koryt. Odcinek biegnie od źródła albo od ujścia dopływu
/// do najbliższego węzła poniżej — czyli do kolejnego zbiegu albo do ujścia z mapy.
fn build_network(
    ctx: &GenCtx,
    class: &[WaterClass],
    strahler: &[u8],
    river_cells: &[RiverCell],
) -> RiverNetwork {
    let dim = ctx.dim();
    let n = dim * dim;

    let river_donors = |c: usize| -> usize {
        let (a, b) = (
            ctx.work.donor_start[c] as usize,
            ctx.work.donor_start[c + 1] as usize,
        );
        ctx.work.donors[a..b]
            .iter()
            .filter(|d| class[**d as usize] == WaterClass::River)
            .count()
    };

    // Węzeł sieci: źródło (brak dopływów) albo zbieg (co najmniej dwa).
    // Komórka o dokładnie jednym dopływie leży w środku odcinka.
    let mut is_node = vec![false; n];
    for c in 0..n {
        if class[c] == WaterClass::River {
            is_node[c] = river_donors(c) != 1;
        }
    }

    // Numer odcinka zaczynającego się w danym węźle — do wiązania `downstream`.
    let mut segment_at = vec![u32::MAX; n];
    let mut starts: Vec<usize> = (0..n).filter(|c| is_node[*c]).collect();
    starts.sort_unstable();
    for (i, c) in starts.iter().enumerate() {
        segment_at[*c] = i as u32;
    }

    let width_depth = |c: usize| -> (u16, u16) {
        river_cells
            .binary_search_by_key(&(c as u32), |r| r.cell)
            .map(|i| (river_cells[i].width_dm, river_cells[i].depth_dm))
            .unwrap_or((0, 0))
    };

    let mut segments = Vec::with_capacity(starts.len());
    for (id, start) in starts.iter().enumerate() {
        let mut points = Vec::new();
        let mut cur = *start;
        let mut max_order = strahler[cur];
        loop {
            points.push((
                ((cur % dim) as i32) * WORK_CELL_M as i32,
                ((cur / dim) as i32) * WORK_CELL_M as i32,
            ));
            max_order = max_order.max(strahler[cur]);
            let r = ctx.work.receiver[cur];
            if r == NO_RECEIVER || class[r as usize] != WaterClass::River {
                // Ujście do morza, do jeziora albo poza mapę — odcinek się kończy.
                if r != NO_RECEIVER {
                    points.push((
                        ((r as usize % dim) as i32) * WORK_CELL_M as i32,
                        ((r as usize / dim) as i32) * WORK_CELL_M as i32,
                    ));
                }
                segments.push(RiverSegment {
                    id: id as u32,
                    points,
                    strahler: max_order,
                    downstream: None,
                    width_dm: width_depth(cur).0,
                    depth_dm: width_depth(cur).1,
                });
                break;
            }
            let r = r as usize;
            if is_node[r] {
                points.push((
                    ((r % dim) as i32) * WORK_CELL_M as i32,
                    ((r / dim) as i32) * WORK_CELL_M as i32,
                ));
                segments.push(RiverSegment {
                    id: id as u32,
                    points,
                    strahler: max_order,
                    downstream: Some(segment_at[r]),
                    width_dm: width_depth(r).0,
                    depth_dm: width_depth(r).1,
                });
                break;
            }
            cur = r;
        }
    }

    RiverNetwork { segments }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn swiat(region: Region, seed: u64) -> GenCtx<'static> {
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(2)));
        let params = WorldGenParams {
            seed,
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        for pass in crate::pipeline::PASSES.iter().take(7) {
            (pass.run)(&mut ctx);
        }
        ctx
    }

    #[test]
    fn geometria_hydrauliczna_daje_wiarygodne_rzeki() {
        // Zlewnia 100 km² przy 600 mm opadu: rzeka rzędu kilku–kilkunastu metrów szerokości
        // i mniej niż metr głębokości. Liczby spoza tego przedziału znaczą, że przelicznik
        // jednostek się rozjechał — a to jest błąd, którego na mapie się nie zauważa.
        let q = bankfull_discharge(100e6, 0.6);
        let w = 2.5 * det_math::sqrt(q);
        let d = 0.25 * det_math::pow(q, 0.4);
        assert!((5.0..25.0).contains(&w), "szerokość {w} m przy 100 km²");
        assert!((0.4..2.0).contains(&d), "głębokość {d} m przy 100 km²");

        // Rzeka o stukrotnie większej zlewni ma być około dziesięć razy szersza.
        let q2 = bankfull_discharge(10_000e6, 0.6);
        let w2 = 2.5 * det_math::sqrt(q2);
        assert!(
            (w2 / w - 10.0).abs() < 0.5,
            "skalowanie szerokości: {}",
            w2 / w
        );
    }

    #[test]
    fn rzeki_docieraja_do_ujscia() {
        // M1 §7.2: każdy odcinek sieci ma ścieżkę do morza, jeziora albo poza mapę.
        let ctx = swiat(Region::River, 6);
        let siec = &ctx.world.rivers;
        assert!(!siec.segments.is_empty(), "brak rzek");
        for s in &siec.segments {
            let mut cur = s;
            let mut kroki = 0;
            while let Some(d) = cur.downstream {
                cur = &siec.segments[d as usize];
                kroki += 1;
                assert!(
                    kroki <= siec.segments.len(),
                    "pętla w sieci koryt od {}",
                    s.id
                );
            }
        }
    }

    #[test]
    fn rzad_strahlera_rosnie_w_dol_rzeki() {
        let ctx = swiat(Region::River, 6);
        let siec = &ctx.world.rivers;
        for s in &siec.segments {
            if let Some(d) = s.downstream {
                assert!(
                    siec.segments[d as usize].strahler >= s.strahler,
                    "rząd spada z {} na {} poniżej zbiegu",
                    s.strahler,
                    siec.segments[d as usize].strahler
                );
            }
        }
        // Mapa 4 km ma 16,8 km² powierzchni, a próg rzeki to 0,25 km² zlewni — na rzekę
        // rzędu 4 nie ma tu miejsca. Rząd 3 wystarcza, żeby stwierdzić, że sieć ma
        // rozgałęzienia, a nie jest wiązką niezależnych rowów.
        let max = siec.segments.iter().map(|s| s.strahler).max().unwrap_or(0);
        assert!(max >= 3, "najwyższy rząd w sieci to {max}");
    }

    #[test]
    fn glebokosc_wody_nigdy_nie_jest_ujemna() {
        // M1 §7.2 `no_negative_water`.
        for region in [Region::Coastal, Region::River, Region::Mountain] {
            let ctx = swiat(region, 14);
            for i in 0..ctx.world.water.len() {
                let d = ctx.world.water_depth_dm(i);
                let klasa = ctx.world.water[i].class();
                if klasa.is_water() {
                    // Koryto zostało wcięte, więc rzeka ma mieć wodę.
                    if klasa == WaterClass::River {
                        assert!(d > 0, "{}: rzeka o zerowej głębokości w {i}", region.key());
                    }
                }
            }
        }
    }

    #[test]
    fn zwierciadlo_wody_opada_w_dol_biegu() {
        // Niezmiennik dotyczy **zwierciadła**, nie dna. Dno bywa lokalnie głębsze
        // (jeśli teren i tak był niżej), ale woda w rzece nie może płynąć pod górę.
        let ctx = swiat(Region::River, 6);
        let znajdz = |c: u32| {
            ctx.world
                .river_cells
                .binary_search_by_key(&c, |r| r.cell)
                .ok()
                .map(|i| ctx.world.river_cells[i].surface_dm)
        };
        for rc in &ctx.world.river_cells {
            let r = ctx.work.receiver[rc.cell as usize];
            if r == NO_RECEIVER {
                continue;
            }
            if let Some(nizej) = znajdz(r) {
                assert!(
                    rc.surface_dm >= nizej,
                    "zwierciadło rośnie w dół biegu: {} → {} w komórce {}",
                    rc.surface_dm,
                    nizej,
                    rc.cell
                );
            }
        }
    }

    #[test]
    fn region_nadmorski_ma_morze_a_srodladowy_nie() {
        let morski = swiat(Region::Coastal, 2);
        let ile_morza = |c: &GenCtx| {
            c.world
                .water
                .as_slice()
                .iter()
                .filter(|w| w.class() == WaterClass::Sea)
                .count()
        };
        assert!(ile_morza(&morski) > 1000, "wybrzeże bez morza");
        assert_eq!(ile_morza(&swiat(Region::Lowland, 2)), 0, "nizina z morzem");
    }
}
