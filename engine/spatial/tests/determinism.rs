//! Kryterium ukończenia WP2 i test D3 z §7 dokumentu fazy: przebudowa `DynamicGrid`
//! daje **bit-identyczny** `CsrGrid` przy 1, 4 i 8 wątkach oraz w dwóch przebiegach.
//!
//! Porównanie idzie po bitach pozycji (`to_bits`), nie po wartościach — przy floatach
//! to jedyne porównanie, które wykrywa różnicę „ten sam wynik, inna droga".

use magnat_core::{rng, StreamId, Tick};
use magnat_jobs::JobPool;
use magnat_spatial::{CellId, DynamicGrid, GridSpec, Vec2};

const N: u32 = 40_000;

fn agenci(seed: u64, spec: &GridSpec) -> (Vec<Vec2>, Vec<u32>) {
    let mut r = rng(seed, StreamId::EngineSelfTest, 0, Tick(0));
    let w = u32::from(spec.cols) * u32::from(spec.cell_m);
    let h = u32::from(spec.rows) * u32::from(spec.cell_m);
    let mut pos = Vec::with_capacity(N as usize);
    for _ in 0..N {
        pos.push(Vec2::new(
            r.gen_range_u32(w) as f32 + 0.5,
            r.gen_range_u32(h) as f32 + 0.5,
        ));
    }
    (pos, (0..N).collect())
}

/// Odcisk całego indeksu: numer komórki, bity pozycji i identyfikator, po kolei.
fn odcisk(g: &DynamicGrid<u32>) -> Vec<u64> {
    let spec = *g.spec();
    let mut v = Vec::with_capacity(g.len() * 3);
    for c in 0..spec.cell_count() as u32 {
        let (pts, ids) = g.grid().cell(CellId(c));
        for (p, i) in pts.iter().zip(ids) {
            v.push(u64::from(c));
            v.push(u64::from(p.x.to_bits()) << 32 | u64::from(p.y.to_bits()));
            v.push(u64::from(*i));
        }
    }
    v
}

#[test]
fn przebudowa_jest_bit_identyczna_niezaleznie_od_liczby_watkow() {
    let spec = GridSpec::new(Vec2::ZERO, 32, 128, 128);
    let (pos, ids) = agenci(0xC0FFEE, &spec);

    let mut wzorzec: Option<Vec<u64>> = None;
    for threads in [1, 2, 4, 8] {
        let pool = JobPool::new(threads);
        for przebieg in 0..2 {
            let mut g = DynamicGrid::new(spec);
            g.rebuild(&pos, &ids, &pool);
            assert_eq!(g.len(), N as usize);
            let o = odcisk(&g);
            match &wzorzec {
                None => wzorzec = Some(o),
                Some(w) => assert_eq!(*w, o, "{threads} wątków, przebieg {przebieg}"),
            }
        }
    }
}

#[test]
fn ponowna_przebudowa_nie_zostawia_smieci_po_poprzedniej() {
    let spec = GridSpec::new(Vec2::ZERO, 32, 64, 64);
    let pool = JobPool::new(4);
    let (pos_a, ids_a) = agenci(1, &spec);
    let (pos_b, ids_b) = agenci(2, &spec);

    let mut g = DynamicGrid::new(spec);
    g.rebuild(&pos_a, &ids_a, &pool);
    g.rebuild(&pos_b, &ids_b, &pool);

    let mut swiezy: DynamicGrid<u32> = DynamicGrid::new(spec);
    swiezy.rebuild(&pos_b, &ids_b, &pool);
    assert_eq!(odcisk(&g), odcisk(&swiezy));

    // Przebudowa na pusty zbiór zostawia pusty indeks, a nie stary.
    g.rebuild(&[], &[], &pool);
    assert!(g.is_empty());
    assert_eq!(g.grid().cell(CellId(0)).1.len(), 0);
}

#[test]
fn przebudowa_zgadza_sie_z_indeksem_statycznym() {
    // `DynamicGrid` i `CsrGrid::build` muszą dawać ten sam układ — inaczej mezo
    // i mikro widziałyby różne zbiory sąsiadów (00 §4, K-5).
    let spec = GridSpec::new(Vec2::ZERO, 32, 64, 64);
    let pool = JobPool::new(8);
    let (pos, ids) = agenci(3, &spec);
    let mut g = DynamicGrid::new(spec);
    g.rebuild(&pos, &ids, &pool);

    let statyczny =
        magnat_spatial::CsrGrid::build(spec, pos.iter().copied().zip(ids.iter().copied()));
    for c in 0..spec.cell_count() as u32 {
        assert_eq!(
            g.grid().cell(CellId(c)).1,
            statyczny.cell(CellId(c)).1,
            "komórka {c}"
        );
    }
}
