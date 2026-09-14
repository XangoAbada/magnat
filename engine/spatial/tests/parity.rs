//! Kryterium ukończenia WP1: wynik gridu == wynik przeglądu zupełnego
//! dla 10 tys. losowych zapytań. Test jest własnościowy w sensie 00 §6 — porównuje
//! indeks z definicją, a nie z zapamiętanym wynikiem.

use magnat_core::{rng, StreamId, Tick};
use magnat_jobs::JobPool;
use magnat_spatial::{Aabb2, BatchResult, CategoryGrid, CsrGrid, GridSpec, Vec2};

const MAPA: f32 = 2_048.0;
const ENCJE: u32 = 2_000;
const ZAPYTANIA: usize = 10_000;

fn swiat() -> (GridSpec, Vec<(Vec2, u32)>) {
    let spec = GridSpec::new(Vec2::ZERO, 64, 32, 32);
    let mut r = rng(0xC0FFEE, StreamId::EngineSelfTest, 0, Tick(0));
    let mut v: Vec<(Vec2, u32)> = (0..ENCJE)
        .map(|i| {
            let x = r.gen_range_u32(MAPA as u32) as f32;
            let y = r.gen_range_u32(MAPA as u32) as f32;
            (Vec2::new(x, y), i)
        })
        .collect();
    // Kilka encji celowo poza mapą: przycięcie do komórki brzegowej nie może
    // zrobić z nich fałszywych trafień ani ich zgubić.
    v.push((Vec2::new(-500.0, 10.0), ENCJE));
    v.push((Vec2::new(MAPA + 700.0, MAPA + 700.0), ENCJE + 1));
    (spec, v)
}

fn posortowane(v: &[u32]) -> Vec<u32> {
    let mut v = v.to_vec();
    v.sort_unstable();
    v
}

#[test]
fn promien_zgadza_sie_z_przegladem_zupelnym() {
    let (spec, encje) = swiat();
    let g = CsrGrid::build(spec, encje.iter().copied());
    let mut r = rng(1, StreamId::EngineSelfTest, 0, Tick(0));
    let mut out = Vec::new();
    for q in 0..ZAPYTANIA {
        let c = Vec2::new(
            r.gen_range_u32(MAPA as u32 + 400) as f32 - 200.0,
            r.gen_range_u32(MAPA as u32 + 400) as f32 - 200.0,
        );
        let rad = 10.0 + r.gen_range_u32(240) as f32;
        g.query_radius(c, rad, &mut out);
        let wzorzec: Vec<u32> = encje
            .iter()
            .filter(|(p, _)| (*p - c).length_squared() <= rad * rad)
            .map(|(_, i)| *i)
            .collect();
        assert_eq!(posortowane(&out), posortowane(&wzorzec), "zapytanie {q}");
    }
}

#[test]
fn prostokat_zgadza_sie_z_przegladem_zupelnym() {
    let (spec, encje) = swiat();
    let g = CsrGrid::build(spec, encje.iter().copied());
    let mut r = rng(2, StreamId::EngineSelfTest, 0, Tick(0));
    let mut out = Vec::new();
    for q in 0..1_000 {
        let a = Vec2::new(
            r.gen_range_u32(MAPA as u32) as f32,
            r.gen_range_u32(MAPA as u32) as f32,
        );
        let b = a + Vec2::new(r.gen_range_u32(400) as f32, r.gen_range_u32(400) as f32);
        let q_box = Aabb2::new(a, b);
        g.query_rect(q_box, &mut out);
        let wzorzec: Vec<u32> = encje
            .iter()
            .filter(|(p, _)| q_box.contains(*p))
            .map(|(_, i)| *i)
            .collect();
        assert_eq!(posortowane(&out), posortowane(&wzorzec), "zapytanie {q}");
    }
}

#[test]
fn k_najblizszych_zgadza_sie_z_sortowaniem_zupelnym() {
    let (spec, encje) = swiat();
    let g = CsrGrid::build(spec, encje.iter().copied());
    let mut r = rng(3, StreamId::EngineSelfTest, 0, Tick(0));
    let mut out = Vec::new();
    for q in 0..1_000 {
        let c = Vec2::new(
            r.gen_range_u32(MAPA as u32) as f32,
            r.gen_range_u32(MAPA as u32) as f32,
        );
        let k = 1 + r.gen_range_u32(20) as usize;
        g.k_nearest(c, k, &mut out);

        let mut wzorzec: Vec<(f32, u32)> =
            encje.iter().map(|(p, i)| ((*p - c).length(), *i)).collect();
        wzorzec.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        wzorzec.truncate(k);

        assert_eq!(out.len(), wzorzec.len(), "zapytanie {q}");
        // Porównanie po odległościach: przy remisie kolejność encji jest inna
        // (grid idzie po komórkach), ale zbiór odległości musi być ten sam.
        for (a, b) in out.iter().zip(wzorzec.iter()) {
            assert!((a.0 - b.0).abs() <= 1e-4, "zapytanie {q}: {a:?} vs {b:?}");
        }
    }
}

#[test]
fn wsad_daje_to_samo_co_zapytania_pojedyncze() {
    let (spec, encje) = swiat();
    let g = CsrGrid::build(spec, encje.iter().copied());
    let mut r = rng(4, StreamId::EngineSelfTest, 0, Tick(0));
    let centra: Vec<Vec2> = (0..5_000)
        .map(|_| {
            Vec2::new(
                r.gen_range_u32(MAPA as u32) as f32,
                r.gen_range_u32(MAPA as u32) as f32,
            )
        })
        .collect();

    let mut wynik = BatchResult::new();
    let mut pojedyncze = Vec::new();
    // Ta sama odpowiedź przy 1 i 8 wątkach — składanie idzie po indeksie porcji.
    let mut poprzedni: Option<Vec<Vec<u32>>> = None;
    for threads in [1, 8] {
        let pool = JobPool::new(threads);
        g.query_radius_batch(&centra, 150.0, &pool, &mut wynik);
        assert_eq!(wynik.len(), centra.len());
        let zebrane: Vec<Vec<u32>> = (0..centra.len()).map(|i| wynik.get(i).to_vec()).collect();
        for (i, c) in centra.iter().enumerate() {
            g.query_radius(*c, 150.0, &mut pojedyncze);
            assert_eq!(
                posortowane(&zebrane[i]),
                posortowane(&pojedyncze),
                "centrum {i}"
            );
        }
        if let Some(p) = &poprzedni {
            assert_eq!(*p, zebrane, "wsad zależny od liczby wątków");
        }
        poprzedni = Some(zebrane);
    }
}

#[test]
fn kategorie_nie_mieszaja_sie_ze_soba() {
    let (spec, encje) = swiat();
    let kat = |i: u32| (i % 7) as u8;
    let cg = CategoryGrid::build(spec, encje.iter().map(|(p, i)| (kat(*i), *p, *i)));
    assert_eq!(cg.keys(), &[0u8, 1, 2, 3, 4, 5, 6]);

    let mut r = rng(5, StreamId::EngineSelfTest, 0, Tick(0));
    for _ in 0..500 {
        let c = Vec2::new(
            r.gen_range_u32(MAPA as u32) as f32,
            r.gen_range_u32(MAPA as u32) as f32,
        );
        let k = r.gen_range_u32(7) as u8;
        let mut out = Vec::new();
        cg.for_each_in_radius(k, c, 200.0, |_, i| out.push(i));
        let wzorzec: Vec<u32> = encje
            .iter()
            .filter(|(p, i)| kat(*i) == k && (*p - c).length_squared() <= 200.0 * 200.0)
            .map(|(_, i)| *i)
            .collect();
        assert_eq!(posortowane(&out), posortowane(&wzorzec));
    }
    assert!(cg.grid(99).is_none());
}
