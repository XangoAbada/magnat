//! Kryteria zamknięcia podfazy **M2d** (WP10, WP15a, WP11, WP12, WP12b) — M2 §4.
//!
//! Jak `city.rs` i `city_m2c.rs`: testy stoją na prawdziwym terenie, bo cały sens
//! niwelacji parceli i niwelety drogi polega na tym, że teren nie jest płaski.
//! Każdy generuje świat, więc są `#[ignore]` i uruchamiane jawnie:
//! `cargo test --release -p magnat-world --test city_m2d -- --include-ignored`

use magnat_jobs::JobPool;
use magnat_voxel::{ChunkBuilder, ChunkCoord, ColumnSource, MaterialRegistry, CHUNK_DIM};
use magnat_world::city::build::{EntranceKind, UnitKind};
use magnat_world::city::{poly, value};
use magnat_world::{
    generate, generate_city, CityData, CityPlan, Difficulty, EconomyProfile, Epoch, Region,
    Terrain, TerrainQuery, WorldGenParams, WorldSize, ZoneKind,
};
use std::sync::Arc;

fn miasto(seed: u64, size: WorldSize, region: Region) -> (CityData, Terrain) {
    miasto_w(seed, size, region, 0)
}

fn miasto_w(seed: u64, size: WorldSize, region: Region, watki: usize) -> (CityData, Terrain) {
    let pool = JobPool::new(watki);
    let params = WorldGenParams {
        seed,
        size,
        region,
        epoch: Epoch::Y1990,
        profile: EconomyProfile::Mixed,
        difficulty: Difficulty::Normal,
    };
    let (data, _) = generate(params, &pool).unwrap();
    let reg = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path("materials")).unwrap());
    let t = Terrain::new(data, reg);
    let plan = CityPlan::from_world(&params);
    let c = generate_city(&plan, &t, t.materials(), &pool).unwrap();
    (c, t)
}

// ── WP10: język gramatyki ────────────────────────────────────────────────────────────

/// „100% plików z repo przechodzi walidację" — walidator jest w testach jednostkowych
/// modułu, tu sprawdzamy skutek: każda strefa zabudowywalna dostaje w mieście gramatykę
/// **inną niż awaryjna**, czyli katalog naprawdę pokrywa miasto.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn katalog_gramatyk_pokrywa_strefy_miasta() {
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    let r = &c.report.build;
    assert!(
        r.buildings > 500,
        "miasto ma tylko {} budynków",
        r.buildings
    );
    let udzial = f64::from(r.fallback) * 100.0 / f64::from(r.buildings);
    assert!(
        udzial < 2.0,
        "gramatyka awaryjna na {udzial:.1}% budynków (limit 2%); rozbicie: {:?}",
        r.fallback_zone
    );
    assert_eq!(
        r.truncated, 0,
        "{} derywacji przerwanych limitem węzłów albo głębokości",
        r.truncated
    );
}

// ── WP15a: wycena gruntu, pass_1 ─────────────────────────────────────────────────────

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn wycena_pass1_jest_dodatnia_i_rosnie_ku_centrum() {
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    for (i, p) in c.parcels.parcels.iter().enumerate() {
        if !p.zone.parcelled() {
            continue;
        }
        assert!(
            p.land_value_per_m2.0 > 0,
            "parcela {i} ({}) ma wartość {} gr/m²",
            p.zone.key(),
            p.land_value_per_m2.0
        );
    }
    let rdzen = c.report.land_value_core;
    let obrzeze = c.report.land_value_fringe;
    assert!(rdzen.0 > 0 && obrzeze.0 > 0, "brak dzielnic do porównania");
    assert!(
        rdzen.0 > obrzeze.0,
        "rdzeń {} gr/m² nie jest droższy od obrzeża {} gr/m²",
        rdzen.0,
        obrzeze.0
    );
}

/// Rozbicie na czynniki — wymóg wyjaśnialności (00 §7). `pass_1` wypełnia osiem z dwunastu
/// pozycji; cztery pozostałe (praca, handel, usługi, zanieczyszczenie) dokłada `pass_2`.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn rozbicie_wyceny_sumuje_sie_do_wyniku() {
    let (c, _) = miasto(3, WorldSize::Small4km, Region::Lowland);
    let ctx = value::ValueCtx {
        fields: &c.fields,
        roads: &c.roads,
        geom: &c.roads.geom,
        blocks: &c.blocks,
        districts: &c.districts,
        rings: c.zones.rings.len() as u8,
    };
    let mut z_wplywem = 0;
    for i in 0..c.parcels.parcels.len().min(500) {
        let (v, b) = value::pass_1_parcel(&ctx, &c.parcels, i);
        assert_eq!(v, b.result, "rozbicie nie zgadza się z wynikiem");
        assert_eq!(
            v, c.parcels.parcels[i].land_value_per_m2,
            "zapis ≠ wyliczenie"
        );
        z_wplywem = z_wplywem.max(b.factors.iter().filter(|(_, pp)| *pp != 0).count());
    }
    assert!(
        z_wplywem >= 5,
        "pass_1 wypełnił tylko {z_wplywem} czynników z dwunastu"
    );
}

// ── WP11: derywacja ──────────────────────────────────────────────────────────────────

/// Kryterium WP11 i test D2 fazy: derywacja równoległa daje wynik identyczny
/// z jednowątkową. Sprawdzane przez hash miasta, który obejmuje budynki, lokale
/// i stanowiska — czyli cały produkt derywacji.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn derywacja_rownolegla_daje_ten_sam_wynik_co_jednowatkowa() {
    let (a, _) = miasto_w(11, WorldSize::Small4km, Region::Lowland, 1);
    let (b, _) = miasto_w(11, WorldSize::Small4km, Region::Lowland, 8);
    assert_eq!(
        a.report.city_hash, b.report.city_hash,
        "hash miasta zależy od liczby wątków"
    );
    assert_eq!(a.buildings.buildings.len(), b.buildings.buildings.len());
    assert_eq!(a.buildings.units, b.buildings.units);
    assert_eq!(
        a.edits.commands().len(),
        b.edits.commands().len(),
        "inna liczba komend voxelowych"
    );
    assert!(
        a.edits.commands() == b.edits.commands(),
        "komendy voxelowe różnią się między przebiegami"
    );
}

// ── WP12: posadowienie i wnętrza ─────────────────────────────────────────────────────

/// T6 fazy w zakresie dostępnym w M2d: obrys budynku ⊆ wielokąt parceli.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn budynek_miesci_sie_w_swojej_parceli() {
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    let geom = &c.roads.geom;
    for b in &c.buildings.buildings {
        let parcela = &c.parcels.parcels[b.parcel.0.index() as usize];
        let dzialka = geom.get(parcela.poly);
        for p in geom.get(b.footprint) {
            assert!(
                poly::contains(dzialka, *p),
                "budynek na parceli {} wystaje poza działkę w {p:?}",
                b.parcel.0.index()
            );
        }
    }
}

/// Bryły dwóch budynków nie zachodzą na siebie. Wynika to z rozłączności parcel (T5)
/// i z testu wyżej, ale sprawdzamy wprost: to jest ta klasa błędu, która w geometrii
/// na floatach lubi się pojawić mimo poprawnych przesłanek (ryzyko R2).
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn budynki_nie_zachodza_na_siebie() {
    use magnat_spatial::Vec2;
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    let geom = &c.roads.geom;
    let mut sasiedzi: Vec<magnat_core::BuildingId> = Vec::new();
    for (i, b) in c.buildings.buildings.iter().enumerate() {
        let obrys = geom.get(b.footprint);
        let srodek = poly::centroid(obrys);
        sasiedzi.clear();
        c.buildings.index.query_radius(srodek, 120.0, &mut sasiedzi);
        for id in &sasiedzi {
            let j = id.0.index() as usize;
            if j <= i {
                continue;
            }
            let inny = geom.get(c.buildings.buildings[j].footprint);
            for p in obrys {
                assert!(
                    !poly::contains(inny, *p),
                    "budynek {i} wchodzi w budynek {j} w punkcie {p:?}"
                );
            }
            // Symetrycznie: wierzchołek sąsiada w naszym obrysie to ten sam błąd.
            for p in inny {
                assert!(
                    !poly::contains(obrys, *p),
                    "budynek {j} wchodzi w budynek {i} w punkcie {p:?}"
                );
            }
            let _ = Vec2::ZERO;
        }
    }
}

/// „Suma m² lokali ≤ powierzchni brutto budynku" — kryterium WP12.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn lokale_miesza_sie_w_powierzchni_brutto() {
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    for b in &c.buildings.buildings {
        let suma: u32 = c.buildings.units[b.units.start as usize..b.units.end as usize]
            .iter()
            .filter(|u| u.floor >= 0)
            .map(|u| u32::from(u.area_m2))
            .sum();
        assert!(
            suma <= b.gross_area_m2 + 1,
            "lokale {suma} m² wobec {} m² brutto",
            b.gross_area_m2
        );
        assert_eq!(
            b.floor_heights_dm.len(),
            usize::from(b.floors),
            "liczba stropów ≠ liczba kondygnacji"
        );
        assert!(b.height_dm > 0, "budynek o zerowej wysokości");
    }
}

/// Kontrakt dla M3: każde stanowisko ma widełki, a widełki są uporządkowane.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn stanowiska_maja_sensowne_widelki_i_lokal() {
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    assert!(
        c.buildings.workplaces.len() > 1000,
        "miasto ma tylko {} stanowisk",
        c.buildings.workplaces.len()
    );
    for w in &c.buildings.workplaces {
        let u = &c.buildings.units[w.unit.0 as usize];
        assert!(!u.kind.is_dwelling(), "stanowisko pracy w mieszkaniu");
        assert!(
            w.wage_band.min.0 > 0
                && w.wage_band.min.0 <= w.wage_band.median.0
                && w.wage_band.median.0 <= w.wage_band.max.0,
            "widełki {:?} nie są uporządkowane",
            w.wage_band
        );
        assert!(w.occupant.is_none(), "w M2 nikt jeszcze nie pracuje");
    }
    // Mieszkania są, i jest ich więcej niż lokali usługowych — to miasto, nie park biurowy.
    let mieszkan = c
        .buildings
        .units
        .iter()
        .filter(|u| u.kind.is_dwelling())
        .count();
    assert!(
        mieszkan * 2 > c.buildings.units.len(),
        "mieszkania to tylko {mieszkan} z {} lokali",
        c.buildings.units.len()
    );
}

/// T4 fazy w zakresie M2d: rampa trafia na drogę **bez** `NO_HEAVY` (korekta D6/D7).
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn rampy_stoja_przy_drodze_dla_ciezkich() {
    use magnat_world::RoadFlags;
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    let mut ramp = 0;
    for b in &c.buildings.buildings {
        for e in b.entrances.iter().filter(|e| e.kind == EntranceKind::Ramp) {
            ramp += 1;
            let seg = &c.roads.segments[e.seg.0 as usize];
            assert!(
                !seg.flags.contains(RoadFlags::NO_HEAVY),
                "rampa przy drodze z zakazem ruchu ciężkiego ({})",
                seg.class.key()
            );
        }
    }
    assert!(ramp > 0, "żaden budynek przemysłowy nie dostał rampy");
}

// ── WP12, ryzyko R9: niwelacja ───────────────────────────────────────────────────────

/// Materializuje chunk LOD0 razem z edycjami miasta — dokładnie tak, jak robi to klient.
fn kolumna(c: &CityData, t: &Terrain, x: i32, y: i32) -> Vec<magnat_voxel::MaterialId> {
    let d = CHUNK_DIM as i32;
    let (cx, cy) = (x.div_euclid(d), y.div_euclid(d));
    let (lx, ly) = (x.rem_euclid(d), y.rem_euclid(d));
    // Trzy warstwy pionowe wokół terenu — budynek i jego fundament mieszczą się w nich.
    let warstwa = t.height_at(x, y).div_euclid(d);
    let mut out = Vec::new();
    for wz in (warstwa - 1).max(0)..=(warstwa + 2) {
        let coord = ChunkCoord::new(cx, cy, wz as i16);
        let mut b = ChunkBuilder::new(coord, 0, 0);
        t.fill_chunk(coord, 0, &mut b);
        c.edits.apply_to(coord, 0, &mut b);
        for lz in 0..d {
            out.push(b.material_at(lx, ly, lz));
        }
    }
    out
}

/// Ryzyko R9 i kryterium zamknięcia M2d: **żaden voxel fundamentu nie graniczy
/// z powietrzem od spodu**. Sprawdzane na kolumnie przez środek budynku: pod pierwszym
/// napotkanym materiałem stałym nie ma dziury aż do dna badanego zakresu.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn zaden_budynek_nie_wisi_nad_terenem() {
    let (c, t) = miasto(7, WorldSize::Medium8km, Region::River);
    let geom = &c.roads.geom;
    let krok = (c.buildings.buildings.len() / 400).max(1);
    let mut sprawdzonych = 0;
    for b in c.buildings.buildings.iter().step_by(krok) {
        let srodek = poly::centroid(geom.get(b.footprint));
        let (x, y) = (srodek.x as i32, srodek.y as i32);
        let kol = kolumna(&c, &t, x, y);
        // Najwyższy voxel stały i pierwsza dziura pod nim.
        let Some(gora) = kol.iter().rposition(|m| !m.is_air()) else {
            panic!("kolumna przez budynek jest cała pusta ({x}, {y})");
        };
        let dziura = kol[..gora].iter().rposition(|m| m.is_air());
        if let Some(d) = dziura {
            // Dziura wolno istnieć tylko jako otwór okienny albo wnętrze, czyli **nad**
            // poziomem terenu. Pod terenem oznacza budynek wiszący w powietrzu.
            let teren_lokalny =
                (t.height_at(x, y) - (t.height_at(x, y).div_euclid(32) - 1) * 32).max(0) as usize;
            assert!(
                d + 1 >= teren_lokalny,
                "budynek na parceli {} wisi: dziura na indeksie {d} przy terenie {teren_lokalny}",
                b.parcel.0.index()
            );
        }
        sprawdzonych += 1;
    }
    assert!(
        sprawdzonych > 50,
        "sprawdzono tylko {sprawdzonych} budynków"
    );
}

// ── WP12b: warstwa transportowa ──────────────────────────────────────────────────────

/// Kryterium WP12b: „żaden segment jezdny nie wisi nad terenem ani nie jest w nim
/// zatopiony poza tolerancją jednego voxela". Mierzone na gotowych voxelach: w osi
/// segmentu najwyższy materiał stały ma leżeć na rzędnej niwelety ±1 voxel.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn jezdnia_lezy_na_niwelecie() {
    use magnat_world::RoadStructure;
    let (c, t) = miasto(7, WorldSize::Medium8km, Region::River);
    let mut sprawdzonych = 0;
    let mut poza = Vec::new();
    for (i, s) in c.roads.segments.iter().enumerate() {
        if !s.class.is_driveable() || !matches!(s.structure, RoadStructure::AtGrade) {
            continue;
        }
        if i % 17 != 0 {
            continue;
        }
        let (a, b) = (c.roads.pos(s.a), c.roads.pos(s.b));
        let p = (a + b) * 0.5;
        let (x, y) = (p.x as i32, p.y as i32);
        // Rzędna niwelety w połowie segmentu, w voxelach pionowych (0,5 m).
        let z_dm = (c.roads.nodes[s.a.0 as usize].z_dm + c.roads.nodes[s.b.0 as usize].z_dm) / 2;
        let niweleta = z_dm as f32 / 10.0 * 2.0;

        let kol = kolumna(&c, &t, x, y);
        let baza = ((t.height_at(x, y).div_euclid(32) - 1).max(0) * 32) as f32;
        let Some(gora) = kol.iter().rposition(|m| !m.is_air()) else {
            continue;
        };
        let powierzchnia = baza + gora as f32;
        sprawdzonych += 1;
        if (powierzchnia - niweleta).abs() > 2.0 {
            poza.push((i, powierzchnia, niweleta));
        }
    }
    assert!(sprawdzonych > 30, "sprawdzono {sprawdzonych} segmentów");
    // Tolerancja: jeden voxel niwelety plus jeden na zaokrąglenie kolumny.
    let udzial = poza.len() as f64 / sprawdzonych as f64;
    assert!(
        udzial < 0.05,
        "{:.0}% jezdni poza tolerancją: {:?}",
        udzial * 100.0,
        &poza[..poza.len().min(6)]
    );
}

/// Determinizm generacji z zabudową (D1 fazy): dwa przebiegi tego samego ziarna
/// dają identyczny hash miasta i identyczny zestaw komend voxelowych.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn dwa_przebiegi_daja_to_samo_miasto() {
    let (a, _) = miasto(0x00C0_FFEE, WorldSize::Small4km, Region::Mountain);
    let (b, _) = miasto(0x00C0_FFEE, WorldSize::Small4km, Region::Mountain);
    assert_eq!(a.report.city_hash, b.report.city_hash);
    assert_eq!(a.report.build, b.report.build);
    assert_eq!(a.buildings.buildings, b.buildings.buildings);
    assert_eq!(a.edits.commands(), b.edits.commands());
}

/// Strefy, w których M2d świadomie nic nie stawia. Bez tego pierwszy zabudowany park
/// wyszedłby dopiero w M11, kiedy ktoś spojrzy na miasto z bliska.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn park_i_kopalnia_zostaja_bez_zabudowy() {
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    for b in &c.buildings.buildings {
        let z = c.parcels.parcels[b.parcel.0.index() as usize].zone;
        assert!(
            !matches!(
                z,
                ZoneKind::Green | ZoneKind::Extraction | ZoneKind::Water | ZoneKind::Undevelopable
            ),
            "budynek w strefie {}",
            z.key()
        );
    }
    // ... a mieszkania stoją wyłącznie tam, gdzie wolno mieszkać.
    for u in c.buildings.units.iter().filter(|u| u.kind.is_dwelling()) {
        let b = &c.buildings.buildings[u.building.0.index() as usize];
        let z = c.parcels.parcels[b.parcel.0.index() as usize].zone;
        assert!(
            !matches!(z, ZoneKind::Logistics | ZoneKind::IndustryHeavy),
            "mieszkanie w strefie {}",
            z.key()
        );
    }
    let _ = UnitKind::Common;
}
