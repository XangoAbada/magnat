//! Kryteria zamknięcia podfazy **M2b** (WP3–WP6) — M2 §4, tabela pakietów roboczych.
//!
//! Testy siedzą na prawdziwym terenie, nie na syntetycznej płaszczyźnie: cały sens
//! ograniczeń lokalnych L-systemu polega na tym, że teren mówi „nie", a na płaskim placu
//! nie mówi nigdy.
//!
//! **Świat ma 2 km, nie 4** (`R2-WP25`). Do R2f każdy test tego pliku stawiał miasto
//! 4 km i był przez to `#[ignore]`, więc na generatorze miasta chodził automatycznie
//! wyłącznie hash determinizmu — a hash mówi „tak samo jak wczoraj", nie „poprawnie".
//! Różnicę widać dokładnie wtedy, gdy ktoś zmieni generator i przeliczy wszystkie hashe,
//! bo tak trzeba.
//!
//! Nie skracamy testów — skracamy świat. Miasto 2 km ma tę samą strukturę co 4 km
//! (bramy, arterie, pierścień, kwartały, strefy, dzielnice) i te same ograniczenia
//! lokalne L-systemu; nie ma tylko skali. `WorldSize::Km2` jest rozmiarem **wyłącznie
//! testowym** (`D-N18`) i nie da się go wybrać ani w kreatorze świata, ani z CLI.

use magnat_jobs::JobPool;
use magnat_voxel::MaterialRegistry;
use magnat_world::city::{blocks, dangling_high_class, is_connected};
use magnat_world::{
    generate, generate_city, CityPlan, Difficulty, EconomyProfile, Epoch, GateKind, Region,
    RoadClass, RoadStructure, Terrain, TerrainQuery, WorldGenParams, WorldSize,
};
use std::sync::Arc;

/// Rozmiar świata dla wszystkich testów tego pliku — patrz nagłówek.
const ROZMIAR: WorldSize = WorldSize::Km2;

fn teren(seed: u64, region: Region) -> Terrain {
    teren_profil(seed, region, EconomyProfile::Mixed)
}

/// Teren dla zadanego profilu gospodarczego. Profil wchodzi do generacji **terenu**,
/// nie tylko miasta — złoża zależą od niego, więc test o kopalniach musi podać ten sam
/// profil w obu miejscach, inaczej sprawdza świat, którego nikt nie postawi.
fn teren_profil(seed: u64, region: Region, profile: EconomyProfile) -> Terrain {
    teren_w(seed, region, profile, ROZMIAR)
}

/// To samo dla rozmiaru innego niż [`ROZMIAR`] — dla dwóch testów, które 2 km nie
/// wystarcza, i tylko dla nich (patrz `D-R7` w `R1-refaktor-po-M5.md`).
fn teren_w(seed: u64, region: Region, profile: EconomyProfile, size: WorldSize) -> Terrain {
    let pool = JobPool::new(0);
    let params = WorldGenParams {
        seed,
        size,
        region,
        epoch: Epoch::Y1990,
        profile,
        difficulty: Difficulty::Normal,
    };
    let (data, _) = generate(params, &pool).unwrap();
    let reg = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path("materials")).unwrap());
    Terrain::new(data, reg)
}

fn plan(seed: u64, region: Region, profile: EconomyProfile) -> CityPlan {
    plan_w(seed, region, profile, ROZMIAR)
}

fn plan_w(seed: u64, region: Region, profile: EconomyProfile, size: WorldSize) -> CityPlan {
    CityPlan {
        seed,
        size,
        region,
        epoch: Epoch::Y1990,
        profile,
        difficulty: Difficulty::Normal,
        target_pop: magnat_world::city::target_pop(size),
    }
}

/// WP4: „2 przebiegi tego samego seeda → identyczny bajt w bajt `RoadNetwork`",
/// a przy okazji kryterium zamknięcia całej podfazy.
#[test]
fn dwa_przebiegi_daja_identyczna_siec() {
    let t = teren(0x00C0_FFEE, Region::River);
    let p = plan(0x00C0_FFEE, Region::River, EconomyProfile::Mixed);
    let a = generate_city(&p, &t, t.materials(), &JobPool::new(0)).unwrap();
    let b = generate_city(&p, &t, t.materials(), &JobPool::new(0)).unwrap();

    assert_eq!(a.roads.hash(), b.roads.hash(), "hash sieci się rozjechał");
    // Hash mógłby kolidować; równość strukturalna jest mocniejsza i tania.
    assert_eq!(a.roads, b.roads, "sieć nie jest identyczna bajt w bajt");
    assert_eq!(a.blocks, b.blocks, "kwartały nie są identyczne");
    assert_eq!(a.center, b.center);
    assert!(
        a.roads.segments.len() > 50,
        "sieć zbyt uboga, żeby test cokolwiek dowodził: {} segmentów",
        a.roads.segments.len()
    );
}

/// Ziarno jest jedynym wejściem, więc jego zmiana musi zmienić miasto.
#[test]
fn inne_ziarno_daje_inne_miasto() {
    let (t1, t2) = (teren(1, Region::Lowland), teren(2, Region::Lowland));
    let a = generate_city(
        &plan(1, Region::Lowland, EconomyProfile::Mixed),
        &t1,
        t1.materials(),
        &JobPool::new(0),
    )
    .unwrap();
    let b = generate_city(
        &plan(2, Region::Lowland, EconomyProfile::Mixed),
        &t2,
        t2.materials(),
        &JobPool::new(0),
    )
    .unwrap();
    assert_ne!(a.roads.hash(), b.roads.hash());
}

/// WP3: „dla 20 seedów × 5 profili: każdy wymagany typ bramy istnieje, leży na terenie
/// zgodnym z typem (port na wodzie żeglownej, lotnisko na terenie o nachyleniu < 3%)".
///
/// Region jest dobrany do profilu, a nie losowy: profil portowy wymaga bramy portowej,
/// a ta nie ma prawa powstać w regionie bez żeglownej wody — i to nie jest wada
/// generatora, tylko warunek zadania.
///
/// **Jeden z dwóch testów tego pliku, który 2 km nie wystarcza** (`R2-WP25`): na mapie
/// 2 048 m brama portowa dla ziarna 8 nie trafia do sieci, bo linia brzegowa bywa krótsza
/// niż odcinek arterii dochodzącej. To nie jest wada generatora — to jest ta sama granica,
/// przez którą port nie powstaje w regionie bez wody. Test zostaje na 4 km, zostaje
/// `#[ignore]` i ma wiersz w `D-R7` z nazwą joba nocnego.
#[test]
#[ignore = "20 ziaren x 4 km — job nocny `determinism`, wiersz w `D-R7` (R1)"]
fn bramy_wymagane_istnieja_i_leza_na_wlasciwym_terenie() {
    // Profile pogrupowane po regionie: teren zależy od (ziarno, region), więc dla pięciu
    // profili wystarczą dwie generacje na ziarno, a nie pięć.
    let grupy: [(Region, &[EconomyProfile]); 2] = [
        (
            Region::Lowland,
            &[
                EconomyProfile::Industrial,
                EconomyProfile::University,
                EconomyProfile::Agricultural,
            ],
        ),
        (
            Region::Coastal,
            &[EconomyProfile::Port, EconomyProfile::Tourist],
        ),
    ];
    for seed in 1..=20u64 {
        for (region, profile) in grupy {
            let t = teren_w(seed, region, EconomyProfile::Mixed, WorldSize::Small4km);
            for prof in profile.iter().copied() {
                let c = generate_city(
                    &plan_w(seed, region, prof, WorldSize::Small4km),
                    &t,
                    t.materials(),
                    &JobPool::new(0),
                )
                .unwrap();
                assert!(
                    c.report.missing_gates.is_empty(),
                    "seed {seed} profil {prof:?}: brak bram {:?}",
                    c.report.missing_gates
                );
                // „Brama istnieje" znaczy: jest w gotowej sieci, a nie tylko w planie.
                // Od M2c dotyczy to także bram kolejowych — WP5b buduje im tory,
                // więc lista `deferred_gates` jest już pusta.
                for k in profil_wymaga(prof) {
                    assert!(
                        c.roads.gates.iter().any(|g| g.kind == k),
                        "seed {seed} profil {prof:?}: brama {k:?} nie trafiła do sieci"
                    );
                }
                for g in &c.roads.gates {
                    let (x, y) = (g.pos.x as i32, g.pos.y as i32);
                    match g.kind {
                        GateKind::Port => assert!(
                            t.navigable(x, y).is_some() && t.water_depth_at(x, y) >= 60,
                            "seed {seed}: port poza wodą żeglowną o głębokości 6 m"
                        ),
                        // < 3% → ≤ 1 jednostka `slope_at` (0,03 · 64 = 1,92).
                        GateKind::Airport => assert!(
                            t.slope_at(x, y) <= 1,
                            "seed {seed}: lotnisko na nachyleniu > 3%"
                        ),
                        _ => assert!(
                            t.water_at(x, y).depth_dm == 0,
                            "seed {seed}: brama {:?} w wodzie",
                            g.kind
                        ),
                    }
                }
            }
        }
    }
}

/// Wymagania bramowe profilu — czytane z tego samego pliku, co generator.
fn profil_wymaga(p: EconomyProfile) -> Vec<GateKind> {
    let txt = std::fs::read_to_string(magnat_world::data_path(&format!(
        "zoning/profile_{}.ron",
        p.key()
    )))
    .expect("profil strefowania");
    let g: magnat_world::GateProfile = ron::from_str(&txt).expect("profil strefowania");
    let mut v = g.required;
    v.sort_unstable();
    v.dedup();
    v
}

/// WP4: „sieć bez wiszących końców klasy ≥ Collector".
///
/// Dodatkowo spójność: sieć w kilku kawałkach spełniałaby literę kryterium i nie byłaby
/// miastem. To jest wstęp do testu T1 z §7, który domknie się dopiero w M2e.
#[test]
fn siec_bez_wiszacych_koncow_i_w_jednym_kawalku() {
    for (seed, region) in [
        (7u64, Region::Lowland),
        (7, Region::Mountain),
        (7, Region::River),
        (7, Region::Coastal),
        (7, Region::Desert),
    ] {
        let t = teren(seed, region);
        let c = generate_city(
            &plan(seed, region, EconomyProfile::Mixed),
            &t,
            t.materials(),
            &JobPool::new(0),
        )
        .unwrap();
        let wiszace = dangling_high_class(&c.roads);
        assert!(
            wiszace.is_empty(),
            "{region:?}: {} wiszących końców klasy ≥ Collector",
            wiszace.len()
        );
        assert!(is_connected(&c.roads), "{region:?}: sieć nie jest spójna");
        assert!(
            c.roads.gates.len() >= 2,
            "{region:?}: {} bram przetrwało domknięcie sieci",
            c.roads.gates.len()
        );
    }
}

/// WP5 (część strukturalna): most, tunel i nasyp mieszczą się w limitach swojej klasy.
///
/// Kolej towarowa jest w M2c — powód w nagłówku `lsystem.rs` i w „Korektach planu".
#[test]
fn struktury_miesza_sie_w_limitach_klasy() {
    let t = teren(11, Region::Mountain);
    let c = generate_city(
        &plan(11, Region::Mountain, EconomyProfile::Mixed),
        &t,
        t.materials(),
        &JobPool::new(0),
    )
    .unwrap();
    for s in &c.roads.segments {
        let spec = magnat_world::city::road::spec(s.class);
        let dl_m = f64::from(s.length_dm) / 10.0;
        match s.structure {
            RoadStructure::Bridge { .. } => assert!(
                dl_m <= f64::from(spec.bridge_max_m) + 1.0,
                "most {dl_m:.0} m przy limicie {} m dla {:?}",
                spec.bridge_max_m,
                s.class
            ),
            RoadStructure::Tunnel { .. } => {
                assert!(spec.tunnel_trigger_m > 0, "tunel klasy {:?}", s.class);
                assert!(dl_m <= f64::from(spec.tunnel_max_m) + 1.0);
            }
            RoadStructure::Embankment { height_dm } => {
                assert!(height_dm <= 80, "nasyp {height_dm} dm > 8 m");
            }
            RoadStructure::AtGrade => {}
        }
        // Segment po gruncie nie ma prawa przekroczyć niwelety swojej klasy.
        // Mierzymy spadek między **rzędnymi niwelety**, a nie wysokościami terenu:
        // droga jest w przekroju cięciwą między swoimi końcami, a różnicę pokrywa
        // nasyp albo wykop (patrz „Korekty planu", ograniczenie lokalne 1).
        if matches!(s.structure, RoadStructure::AtGrade) && s.class.is_driveable() {
            let (na, nb) = (c.roads.nodes[s.a.0 as usize], c.roads.nodes[s.b.0 as usize]);
            let dh = f64::from((nb.z_dm - na.z_dm).abs()) * 0.1;
            let dl = f64::from((nb.pos - na.pos).length()).max(1.0);
            let niweleta = ((dh / dl) * 64.0) as u8;
            // Tolerancja jednej jednostki: rzędne są zaokrąglane do decymetra, a połówka
            // segmentu powstała z podziału bywa krótka — 5 dm błędu na 3 m daje właśnie
            // jedną jednostkę. To błąd zapisu, nie złamanie limitu klasy.
            assert!(
                niweleta <= spec.max_slope_units() + 1,
                "{:?} o niwelecie {niweleta} przy limicie {}",
                s.class,
                spec.max_slope_units()
            );
        }
    }
}

/// WP6: kwartały wyznaczone obchodem półkrawędzi domykają bilans pól.
///
/// **Korekta kryterium** (uzasadnienie w dokumencie podfazy): sformułowanie z planu
/// „suma pól kwartałów + pas drogowy = pole obszaru zurbanizowanego" jest spełnione
/// tożsamościowo, bo pas drogowy liczymy jako różnicę ściany i kwartału. Sprawdzamy
/// zamiast tego dwie rzeczy, które naprawdę mogą się nie zgodzić: wzór Eulera dla grafu
/// planarnego i zgodność sumy ścian z polem obrysu zewnętrznego.
#[test]
fn kwartaly_domykaja_bilans_pol() {
    let t = teren(3, Region::Lowland);
    let c = generate_city(
        &plan(3, Region::Lowland, EconomyProfile::Mixed),
        &t,
        t.materials(),
        &JobPool::new(0),
    )
    .unwrap();
    assert!(
        c.blocks.blocks.len() >= 10,
        "tylko {} kwartałów",
        c.blocks.blocks.len()
    );

    // Planarność: żadne dwie krawędzie grafu ścian nie przecinają się bez węzła.
    // To jest założenie, na którym stoi cały obchód półkrawędzi — jedno przecięcie
    // bez węzła scala dwie ściany w jedną i wzór Eulera przestaje się zgadzać.
    let uzyte: Vec<_> = c
        .roads
        .segments
        .iter()
        .filter(|s| {
            s.class.is_driveable() && !s.flags.contains(magnat_world::RoadFlags::GRADE_SEPARATED)
        })
        .map(|s| (c.roads.pos(s.a), c.roads.pos(s.b)))
        .collect();
    let mut kolizje = 0;
    for i in 0..uzyte.len() {
        for j in i + 1..uzyte.len() {
            if przecinaja(uzyte[i], uzyte[j]) {
                kolizje += 1;
            }
        }
    }
    assert_eq!(
        kolizje, 0,
        "{kolizje} przecięć krawędzi bez węzła — graf nie jest planarny"
    );

    // Wzór Eulera dla grafu planarnego o C składowych: ścian jest E − V + 1 + C,
    // a obchód półkrawędzi daje jedną orbitę na ścianę **wewnętrzną** plus po jednej
    // na obwód każdej składowej — razem E − V + 2·C. Porównujemy liczbę orbit, bo to
    // wielkość czysto kombinatoryczna: nie zależy od tego, jak klasyfikujemy ścianę
    // po polu, więc mierzy dokładnie to, co ma zmierzyć — poprawność obchodu.
    // Niezmiennik liczony na stanie sieci **z chwili budowy kwartałów**: M2c dokłada
    // do niej ulice lokalne i tory, więc `face_graph_stats` po całej generacji mierzyłby
    // inny graf niż ten, z którego kwartały powstały.
    let (v, e, skladowe) = c.blocks.euler;
    let oczekiwane = e as i64 - v as i64 + 2 * skladowe as i64;
    assert_eq!(
        i64::from(c.blocks.orbits),
        oczekiwane,
        "orbit {}, a wzór Eulera daje {oczekiwane} (V={v}, E={e}, C={skladowe})",
        c.blocks.orbits
    );

    // Bilans pól: suma kwartałów + pas drogowy = suma ścian, co do 0,5%.
    let suma_kwartalow: f64 = c.blocks.blocks.iter().map(|b| f64::from(b.area_m2)).sum();
    let razem = suma_kwartalow + c.blocks.road_area_m2;
    let blad = (razem - c.blocks.urban_area_m2).abs() / c.blocks.urban_area_m2;
    assert!(
        blad <= 0.005,
        "bilans pól rozjechał się o {:.3}% (kwartały {suma_kwartalow:.0} + drogi {:.0} vs ściany {:.0})",
        blad * 100.0,
        c.blocks.road_area_m2,
        c.blocks.urban_area_m2
    );
}

/// Kontrakt dla M3 (M2 §6): odcinki centrolinii bez grafu.
#[test]
fn street_lines_zwraca_kazdy_segment() {
    let t = teren(5, Region::Lowland);
    let c = generate_city(
        &plan(5, Region::Lowland, EconomyProfile::Mixed),
        &t,
        t.materials(),
        &JobPool::new(0),
    )
    .unwrap();
    let n = magnat_world::street_lines(&c.roads).count();
    assert_eq!(n, c.roads.segments.len());
    for l in magnat_world::street_lines(&c.roads) {
        assert!(l.pts.len() >= 2);
        assert!(l.length_dm > 0);
    }
}

/// Latarnie: wejście dla M11 (M2 §6). Mają istnieć i leżeć przy drodze, nie na niej.
#[test]
fn latarnie_stoja_przy_jezdni() {
    let t = teren(5, Region::Lowland);
    let c = generate_city(
        &plan(5, Region::Lowland, EconomyProfile::Mixed),
        &t,
        t.materials(),
        &JobPool::new(0),
    )
    .unwrap();
    assert!(!c.roads.furniture.is_empty(), "brak małej architektury");
    for f in c.roads.furniture.iter().take(200) {
        let s = &c.roads.segments[f.seg.0 as usize];
        let (a, b) = (c.roads.pos(s.a), c.roads.pos(s.b));
        let p = magnat_spatial::Vec2::new(f.pos.x, f.pos.y);
        let d = (b - a).normalize_or_zero();
        let odleglosc = (p - a).dot(magnat_spatial::Vec2::new(-d.y, d.x)).abs();
        assert!(
            (odleglosc - (f32::from(s.row_m) * 0.5 - 1.5)).abs() < 0.5,
            "latarnia {odleglosc} m od osi przy ROW {} m",
            s.row_m
        );
    }
}

/// Budżet czasu z M2 §7 dla miasta małego (40 tys.) — całość ≤ 12 s w CI.
/// Etap 3 to jego część, więc limit jest tu ostrzejszy.
///
/// **Trzeci test tego pliku, który zostaje na 4 km** (`R2-WP25`), i z najtwardszego
/// powodu z całej trójki: budżet jest liczbą **dla miasta 40-tysięcznego** zmierzoną
/// w wydaniu `release`. Przeliczony na 2 km mierzyłby ćwiartkę powierzchni wobec
/// pełnego limitu, czyli przechodziłby tożsamościowo — a to jest dokładnie ta klasa
/// bramki, którą R2 naprawia. Wiersz w `D-R7` z nazwą joba nocnego.
#[test]
#[ignore = "budzet czasu mierzony na 4 km w release — job nocny `determinism`, `D-R7` (R1)"]
fn etap_3_miesci_sie_w_budzecie_czasu() {
    let t = teren_w(
        9,
        Region::Lowland,
        EconomyProfile::Mixed,
        WorldSize::Small4km,
    );
    let start = std::time::Instant::now();
    let c = generate_city(
        &plan_w(
            9,
            Region::Lowland,
            EconomyProfile::Mixed,
            WorldSize::Small4km,
        ),
        &t,
        t.materials(),
        &JobPool::new(0),
    )
    .unwrap();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    for l in c.report.lines() {
        println!("{l}");
    }
    assert!(ms < 4000.0, "Etap 3 zajął {ms:.0} ms");
}

/// Kwartały mają obrys użytkowy mniejszy od ściany grafu — inaczej pas drogowy
/// nachodziłby na działki (przygotowanie pod T5/T6 w M2e).
#[test]
fn kwartal_miesci_sie_w_swojej_scianie() {
    let t = teren(4, Region::Lowland);
    let c = generate_city(
        &plan(4, Region::Lowland, EconomyProfile::Mixed),
        &t,
        t.materials(),
        &JobPool::new(0),
    )
    .unwrap();
    for b in c.blocks.blocks.iter().take(200) {
        let wnetrze = c.roads.geom.get(b.poly);
        let sciana = c.roads.geom.get(b.face);
        assert_eq!(wnetrze.len(), sciana.len());
        assert!(f64::from(b.area_m2) < pole(sciana) + 1.0);
        assert!(b.area_m2 as f64 >= 300.0);
    }
}

/// Przecięcie właściwe dwóch odcinków — w ścisłym wnętrzu obu.
fn przecinaja(
    a: (magnat_spatial::Vec2, magnat_spatial::Vec2),
    b: (magnat_spatial::Vec2, magnat_spatial::Vec2),
) -> bool {
    let r = a.1 - a.0;
    let s = b.1 - b.0;
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-6 {
        // Współliniowe nachodzenie: wyznacznik się zeruje, więc test przecięcia tego
        // nie widzi — a dla obchodu półkrawędzi jest to ta sama patologia.
        let q = b.0 - a.0;
        let odchylka = (q.x * r.y - q.y * r.x).abs() / r.length().max(1.0);
        if odchylka > 0.5 {
            return false;
        }
        let l = r.length_squared().max(1.0);
        let (t0, t1) = (q.dot(r) / l, (b.1 - a.0).dot(r) / l);
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        return hi > 0.02 && lo < 0.98;
    }
    let q = b.0 - a.0;
    let t = (q.x * s.y - q.y * s.x) / denom;
    let u = (q.x * r.y - q.y * r.x) / denom;
    (0.02..0.98).contains(&t) && (0.02..0.98).contains(&u)
}

fn pole(pts: &[magnat_spatial::Vec2]) -> f64 {
    let mut a = 0.0;
    for i in 0..pts.len() {
        let p = pts[i];
        let q = pts[(i + 1) % pts.len()];
        a += f64::from(p.x) * f64::from(q.y) - f64::from(q.x) * f64::from(p.y);
    }
    (a * 0.5).abs()
}

/// Klasy dróg spełniają hierarchię: kolektor nie jest szerszy od arterii.
#[test]
fn hierarchia_klas_jest_monotoniczna() {
    let kolejno = [
        RoadClass::Highway,
        RoadClass::Arterial,
        RoadClass::Collector,
        RoadClass::Local,
        RoadClass::Service,
    ];
    for w in kolejno.windows(2) {
        let (a, b) = (
            magnat_world::city::road::spec(w[0]),
            magnat_world::city::road::spec(w[1]),
        );
        assert!(a.row_m >= b.row_m);
        assert!(a.seg_len_m >= b.seg_len_m);
        assert!(a.max_slope_pct <= b.max_slope_pct);
    }
    let _ = blocks::BlockId(0);
}

/// `R2-WP17`: **kopalnia stoi na złożu, i w ogóle stoi.**
///
/// Do tej poprawki w wygenerowanym mieście nie było **ani jednego** zakładu
/// wydobywczego z przypisanym złożem, więc cały model wyczerpywania złoża — napisany
/// i przetestowany w `sim/supply/tests/mining.rs` — nie uruchamiał się nigdy.
/// Przyczyny były dwie i obie są tu zamknięte: zakład wydobywczy szukał działki
/// **w strefie**, a nie **na złożu**, oraz odpadał na progu skali `SCALE_MIN` razem
/// z każdym innym zakładem, którego wyrób da się sprowadzić.
///
/// Pięć regionów, bo złoża są własnością terenu i jeden region niczego nie dowodzi.
///
/// **Drugi z dwóch testów, który 2 km nie wystarcza** (`R2-WP25`): na mapie 2 048 m
/// region `Lowland` nie dostaje ani jednego zakładu wydobywczego, więc kryterium byłoby
/// spełnione tożsamościowo — czyli dokładnie to, przed czym ten test broni. Zostaje
/// na 4 km, zostaje `#[ignore]` i ma wiersz w `D-R7` z nazwą joba nocnego.
#[test]
#[ignore = "5 regionow x 4 km — job nocny `determinism`, wiersz w `D-R7` (R1)"]
fn kopalnia_stoi_na_zlozu() {
    for region in [
        Region::Coastal,
        Region::Mountain,
        Region::Lowland,
        Region::River,
        Region::Desert,
    ] {
        let t = teren_w(1, region, EconomyProfile::Industrial, WorldSize::Small4km);
        let p = plan_w(1, region, EconomyProfile::Industrial, WorldSize::Small4km);
        let city = generate_city(&p, &t, t.materials(), &JobPool::new(0)).unwrap();
        let r = &city.sites.report;
        println!(
            "{region:?}: {} zakładów wydobywczych, {} na złożu",
            r.extraction_sites, r.extraction_on_deposit
        );
        assert!(
            r.extraction_sites > 0,
            "{region:?}: zero zakładów wydobywczych — kryterium byłoby spełnione tożsamościowo"
        );
        assert_eq!(
            r.extraction_sites, r.extraction_on_deposit,
            "{region:?}: zakład wydobywczy bez złoża pod spodem"
        );
        // Identyfikator musi wskazywać **istniejące** złoże wymaganego rodzaju, a nie
        // liczbę, która przypadkiem wygląda podobnie (`K-39`).
        for s in &city.sites.sites {
            let a = city.site_catalog.get(s.archetype);
            let Some(want) = a.spec.needs_deposit else {
                continue;
            };
            let d = s.deposit.expect("zakład wydobywczy bez złoża");
            assert_eq!(
                t.deposit(d).resource,
                want,
                "{region:?}: {} stoi na złożu innego surowca",
                a.key()
            );
        }
    }
}
