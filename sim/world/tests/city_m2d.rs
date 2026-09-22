//! Kryteria zamknięcia podfazy **M2d** (WP10, WP15a, WP11, WP12, WP12b) — M2 §4.
//!
//! Jak `city.rs` i `city_m2c.rs`: testy stoją na prawdziwym terenie, bo cały sens
//! niwelacji parceli i niwelety drogi polega na tym, że teren nie jest płaski.
//!
//! **Dziewięć z szesnastu jedzie na 2 km i chodzi przy każdym `cargo test`** (`R2-WP25`).
//! Siedem zostaje na 4 i 8 km: wycena, pokrycie katalogu gramatyk, powierzchnia lokali,
//! niweleta i różnorodność zabudowy mierzą **rozkłady**, a rozkład na ćwiartce
//! powierzchni mierzy wariancję próbki, nie generator. Każdy ma wiersz w `D-R7`
//! (`R1-refaktor-po-M5.md`) z nazwą joba nocnego:
//! `cargo test --release -p magnat-world --test city_m2d -- --include-ignored`

use magnat_jobs::JobPool;
use magnat_voxel::{ChunkBuilder, ChunkCoord, ColumnSource, MaterialRegistry, CHUNK_DIM};
use magnat_world::city::build::{EntranceKind, UnitKind};
use magnat_world::city::grammar::GrammarId;
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
    miasto_pelne(seed, size, region, watki, EconomyProfile::Mixed)
}

fn miasto_pelne(
    seed: u64,
    size: WorldSize,
    region: Region,
    watki: usize,
    profile: EconomyProfile,
) -> (CityData, Terrain) {
    let pool = JobPool::new(watki);
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
    let t = Terrain::new(data, reg);
    let plan = CityPlan::from_world(&params);
    let c = generate_city(&plan, &t, t.materials(), &pool).unwrap();
    (c, t)
}

// ── WP10: język gramatyki ────────────────────────────────────────────────────────────

/// Kryterium WP19 (M2f): **macierz pokrycia bez luk**. Dla każdej kombinacji
/// (strefa × epoka × styl), która w mieście **faktycznie występuje**, katalog ma mieć
/// co najmniej jedną gramatykę niebędącą awaryjną — bez rozluźniania filtrów.
///
/// Kombinacje nieużywane nie są sprawdzane z rozmysłu: wieżowiec w średniowiecznej wsi
/// to nie jest luka, tylko kombinacja, której generator nigdy nie wyprodukuje, a test
/// pilnujący 648 pól macierzy mierzyłby pracowitość, nie jakość miasta.
#[test]
fn macierz_pokrycia_gramatyk_nie_ma_luk() {
    let (c, _) = miasto(7, WorldSize::Km2, Region::River);
    let mats =
        magnat_voxel::MaterialRegistry::load_dir(&magnat_world::assets::data_path("materials"))
            .expect("materiały");
    let katalog = magnat_world::city::grammar::GrammarSet::load_dir(
        &magnat_world::assets::data_path("grammar"),
        &mats,
    )
    .expect("gramatyki");
    let mut luki: Vec<(String, String, String, u32)> = Vec::new();
    for p in &c.parcels.parcels {
        // Te same strefy, które pomija `plan_building`: zieleń i wydobycie dostają obiekty
        // dopiero w Etapie 7 (korekta E6), więc brak dla nich gramatyki nie jest luką.
        if !p.zone.parcelled()
            || matches!(
                p.zone,
                ZoneKind::Water | ZoneKind::Undevelopable | ZoneKind::Green | ZoneKind::Extraction
            )
        {
            continue;
        }
        let block = &c.blocks.blocks[p.block.0 as usize];
        let epoka = c
            .zones
            .rings
            .get(usize::from(block.epoch_ring))
            .map_or("", |e| e.key.as_str());
        let styl = c
            .districts
            .districts
            .get(p.district.0 as usize)
            .map_or("", |d| d.kind.key());
        if katalog.covered(p.zone.key(), epoka, styl) {
            continue;
        }
        let klucz = (
            p.zone.key().to_string(),
            epoka.to_string(),
            styl.to_string(),
        );
        match luki.iter_mut().find(|(z, e, s, _)| {
            (z.as_str(), e.as_str(), s.as_str())
                == (klucz.0.as_str(), klucz.1.as_str(), klucz.2.as_str())
        }) {
            Some((_, _, _, n)) => *n += 1,
            None => luki.push((klucz.0, klucz.1, klucz.2, 1)),
        }
    }
    luki.sort_by(|a, b| b.3.cmp(&a.3).then(a.0.cmp(&b.0)));
    assert!(
        luki.is_empty(),
        "katalog nie pokrywa {} kombinacji; najliczniejsze: {}",
        luki.len(),
        luki.iter()
            .take(12)
            .map(|(z, e, s, n)| format!("{z}/{e}/{s} ×{n}"))
            .collect::<Vec<_>>()
            .join(" · ")
    );
}

/// Udział gramatyki awaryjnej i doborów po rozluźnieniu — progi z kryterium WP19.
#[test]
#[ignore = "2 km nie wystarcza — job nocny `determinism`, wiersz w `D-R7` (R1)"]
fn katalog_trafia_bez_awaryjnej_i_prawie_zawsze_bez_rozluznienia() {
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    let r = &c.report.build;
    let awaryjne = f64::from(r.fallback) * 100.0 / f64::from(r.buildings);
    let rozluznione = f64::from(r.relaxed) * 100.0 / f64::from(r.buildings);
    assert!(
        awaryjne < 0.5,
        "gramatyka awaryjna na {awaryjne:.2}% budynków (limit 0,5%); rozbicie: {:?}",
        r.fallback_zone
    );
    assert!(
        r.buildings > 500,
        "miasto ma tylko {} budynków — próg udziału nic wtedy nie znaczy",
        r.buildings
    );
    assert_eq!(
        r.truncated, 0,
        "{} derywacji przerwanych limitem węzłów albo głębokości",
        r.truncated
    );
    assert!(
        rozluznione < 5.0,
        "rozluźniony filtr na {rozluznione:.2}% budynków (limit 5%): epoka {} · styl {} · wartość {}",
        r.relaxed_epoch,
        r.relaxed_style,
        r.relaxed_value
    );
}

/// **Test T13 fazy** (M2f, WP20): różnorodność zabudowy ma próg, a nie opinię.
///
/// Trzy miary, każda odpowiada na inne pytanie:
/// * **powtórki w sąsiedztwie** — czy pierzeja to jedna forma powielona dwadzieścia razy;
/// * **entropia sygnatur w dzielnicy** — czy dzielnica ma więcej niż jeden rodzaj domu;
/// * **entropia sygnatur w mieście** — czy całość jest różnorodna, nawet jeśli pojedyncza
///   dzielnica ma prawo być jednorodna (osiedle płytowe takie jest i tak ma wyglądać).
///
/// Progi są **skalibrowane pomiarem**, nie wymyślone: na 16-punktowej próbce przed
/// zamknięciem WP20 najgorsze wartości wyniosły 11,9 %, 3,45 bita i 5,91 bita. To są
/// podłogi chroniące przed regresją, a nie cele — zmiana, która je przebije, zawaliła
/// różnorodność i ma o tym powiedzieć głośno.
#[test]
#[ignore = "128 generacji na 4 km — job nocny `determinism`, wiersz w `D-R7` (R1)"]
fn t13_roznorodnosc_zabudowy() {
    /// Udział budynków o identycznej sygnaturze wśród sąsiadów w promieniu 60 m.
    const MAX_POWTOREK_PCT: f64 = 15.0;
    /// Entropia Shannona sygnatur w dzielnicy mieszkaniowej o ≥ 100 budynkach.
    const MIN_ENTROPII_DZIELNICY_MBITS: u32 = 1800;
    /// To samo dla całego miasta.
    const MIN_ENTROPII_MIASTA_MBITS: u32 = 4000;

    let profile = [
        EconomyProfile::Mixed,
        EconomyProfile::Industrial,
        EconomyProfile::Tourist,
        EconomyProfile::Agricultural,
    ];
    let mut najgorsze_powtorki: (f64, u64, &str) = (0.0, 0, "");
    let mut najnizsza_dzielnica: (u32, u64, &str) = (u32::MAX, 0, "");
    let mut najnizsze_miasto: (u32, u64, &str) = (u32::MAX, 0, "");
    let mut mierzonych_dzielnic = 0u32;

    for seed in 1..=32u64 {
        for p in profile {
            let (c, _) = miasto_pelne(seed, WorldSize::Small4km, Region::Lowland, 0, p);
            let r = &c.report.build;
            assert!(
                r.signature_pairs > 1000,
                "seed {seed} {p:?}: tylko {} par sąsiedztwa — próg nic wtedy nie znaczy",
                r.signature_pairs
            );
            let powtorki = f64::from(r.signature_repeats) * 100.0 / f64::from(r.signature_pairs);
            if powtorki > najgorsze_powtorki.0 {
                najgorsze_powtorki = (powtorki, seed, p.key());
            }
            if r.city_entropy_mbits < najnizsze_miasto.0 {
                najnizsze_miasto = (r.city_entropy_mbits, seed, p.key());
            }
            mierzonych_dzielnic += r.districts_measured;
            if r.districts_measured > 0 && r.min_district_entropy_mbits < najnizsza_dzielnica.0 {
                najnizsza_dzielnica = (r.min_district_entropy_mbits, seed, p.key());
            }
        }
    }

    assert!(
        mierzonych_dzielnic > 0,
        "żadna dzielnica nie osiągnęła progu wielkości — próg entropii nic nie mierzy"
    );
    assert!(
        najgorsze_powtorki.0 < MAX_POWTOREK_PCT,
        "powtórki sygnatury {:.1}% (limit {MAX_POWTOREK_PCT}%) — seed {} profil {}",
        najgorsze_powtorki.0,
        najgorsze_powtorki.1,
        najgorsze_powtorki.2
    );
    assert!(
        najnizsza_dzielnica.0 >= MIN_ENTROPII_DZIELNICY_MBITS,
        "entropia sygnatur w dzielnicy {:.2} bita (limit {:.2}) — seed {} profil {}",
        f64::from(najnizsza_dzielnica.0) / 1000.0,
        f64::from(MIN_ENTROPII_DZIELNICY_MBITS) / 1000.0,
        najnizsza_dzielnica.1,
        najnizsza_dzielnica.2
    );
    assert!(
        najnizsze_miasto.0 >= MIN_ENTROPII_MIASTA_MBITS,
        "entropia sygnatur w mieście {:.2} bita (limit {:.2}) — seed {} profil {}",
        f64::from(najnizsze_miasto.0) / 1000.0,
        f64::from(MIN_ENTROPII_MIASTA_MBITS) / 1000.0,
        najnizsze_miasto.1,
        najnizsze_miasto.2
    );
}

/// Sygnatura ma odróżniać to, co różne, i sklejać to, co takie samo — w obie strony.
#[test]
fn sygnatura_pakuje_cztery_znaczniki_bez_kolizji() {
    use magnat_voxel::MaterialId;
    use magnat_world::city::build::BuildingSignature;
    use magnat_world::city::derive::RoofKind;

    let baza = BuildingSignature::new(GrammarId(5), 4, MaterialId(9), RoofKind::Gable);
    assert_eq!(
        baza,
        BuildingSignature::new(GrammarId(5), 4, MaterialId(9), RoofKind::Gable)
    );
    for inny in [
        BuildingSignature::new(GrammarId(6), 4, MaterialId(9), RoofKind::Gable),
        BuildingSignature::new(GrammarId(5), 5, MaterialId(9), RoofKind::Gable),
        BuildingSignature::new(GrammarId(5), 4, MaterialId(10), RoofKind::Gable),
        BuildingSignature::new(GrammarId(5), 4, MaterialId(9), RoofKind::Hip),
    ] {
        assert_ne!(
            baza, inny,
            "zmiana jednego znacznika nie zmieniła sygnatury"
        );
    }
    // Nasycenie, nie zawinięcie: 40 kondygnacji ma być nieodróżnialne od 31, a nie od 8.
    assert_eq!(
        BuildingSignature::new(GrammarId(1), 40, MaterialId(1), RoofKind::Flat),
        BuildingSignature::new(GrammarId(1), 31, MaterialId(1), RoofKind::Flat)
    );
    assert_ne!(
        BuildingSignature::new(GrammarId(1), 40, MaterialId(1), RoofKind::Flat),
        BuildingSignature::new(GrammarId(1), 8, MaterialId(1), RoofKind::Flat)
    );
}

// ── WP15a: wycena gruntu, pass_1 ─────────────────────────────────────────────────────

#[test]
#[ignore = "2 km nie wystarcza — job nocny `determinism`, wiersz w `D-R7` (R1)"]
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
#[ignore = "2 km nie wystarcza — job nocny `determinism`, wiersz w `D-R7` (R1)"]
fn rozbicie_wyceny_sumuje_sie_do_wyniku() {
    let (c, _) = miasto(3, WorldSize::Small4km, Region::Lowland);
    let ctx = value::ValueCtx {
        fields: &c.fields,
        roads: &c.roads,
        geom: &c.roads.geom,
        blocks: &c.blocks,
        districts: &c.districts,
        rings: c.zones.rings.len() as u8,
        // **`D-22` z R1, wykonane w `R2-WP25`.** `access: None` to semantyka `pass_1`,
        // a `land_value_per_m2` jest zapisem `pass_2` — `generate_city` woła go po
        // Etapie 7 i nadpisuje pole dla wszystkich parcel. Pierwszy assert, ten od
        // nazwy testu, przechodził; padał drugi, bo porównywał wyliczenie jednego
        // przebiegu z zapisem drugiego. Test pochodzi z M2d, sprzed istnienia `pass_2`.
        access: Some(&c.access),
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
fn derywacja_rownolegla_daje_ten_sam_wynik_co_jednowatkowa() {
    let (a, _) = miasto_w(11, WorldSize::Km2, Region::Lowland, 1);
    let (b, _) = miasto_w(11, WorldSize::Km2, Region::Lowland, 8);
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
fn budynek_miesci_sie_w_swojej_parceli() {
    let (c, _) = miasto(7, WorldSize::Km2, Region::River);
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
fn budynki_nie_zachodza_na_siebie() {
    use magnat_spatial::Vec2;
    let (c, _) = miasto(7, WorldSize::Km2, Region::River);
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
#[ignore = "2 km nie wystarcza — job nocny `determinism`, wiersz w `D-R7` (R1)"]
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
fn stanowiska_maja_sensowne_widelki_i_lokal() {
    let (c, _) = miasto(7, WorldSize::Km2, Region::River);
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
fn rampy_stoja_przy_drodze_dla_ciezkich() {
    use magnat_world::RoadFlags;
    let (c, _) = miasto(7, WorldSize::Km2, Region::River);
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
fn zaden_budynek_nie_wisi_nad_terenem() {
    let (c, t) = miasto(7, WorldSize::Km2, Region::River);
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
#[ignore = "2 km nie wystarcza — job nocny `determinism`, wiersz w `D-R7` (R1)"]
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
fn dwa_przebiegi_daja_to_samo_miasto() {
    let (a, _) = miasto(0x00C0_FFEE, WorldSize::Km2, Region::Mountain);
    let (b, _) = miasto(0x00C0_FFEE, WorldSize::Km2, Region::Mountain);
    assert_eq!(a.report.city_hash, b.report.city_hash);
    assert_eq!(a.report.build, b.report.build);
    assert_eq!(a.buildings.buildings, b.buildings.buildings);
    assert_eq!(a.edits.commands(), b.edits.commands());
}

/// Strefy, w których M2d świadomie nic nie stawia. Bez tego pierwszy zabudowany park
/// wyszedłby dopiero w M11, kiedy ktoś spojrzy na miasto z bliska.
///
/// **Test wykonuje `D-19` z R1** (`R2-WP25`): padał na `master` od nieznanej liczby faz
/// i diagnoza mówiła wprost, że nieaktualny jest **test, nie kod**. Korekta `F2` z M2e
/// pozwala Etapowi 7 wskazać gramatykę imiennie — pawilon w parku, nadszybie na kopalni
/// — a strażnik strefy w `plan_building_inner` brzmi `wymus.is_none() && …`. Test
/// pochodzi z M2d, czyli sprzed tej korekty, i iterował po **wszystkich** budynkach
/// łącznie z dostawionymi przez Etap 7.
///
/// Reguła, której broni, zostaje bez zmian i jest nadal mierzalna: zabudowa **Etapu 4**
/// nie wchodzi do zieleni ani na wyrobisko. Parcele, na których stoi zakład, są z niej
/// wyjęte imiennie — bo tam zabudowa jest decyzją Etapu 7, a nie przeoczeniem.
#[test]
#[ignore = "8 km — job nocny `determinism`, wiersz w `D-R7` (R1)"]
fn park_i_kopalnia_zostaja_bez_zabudowy() {
    let (c, _) = miasto(7, WorldSize::Medium8km, Region::River);
    let z_zakladem: std::collections::BTreeSet<u32> =
        c.sites.sites.iter().map(|s| s.parcel.0.index()).collect();
    let mut wyjete = 0;
    for b in &c.buildings.buildings {
        let idx = b.parcel.0.index();
        let z = c.parcels.parcels[idx as usize].zone;
        if z_zakladem.contains(&idx) {
            wyjete += 1;
            continue;
        }
        assert!(
            !matches!(
                z,
                ZoneKind::Green | ZoneKind::Extraction | ZoneKind::Water | ZoneKind::Undevelopable
            ),
            "budynek w strefie {}",
            z.key()
        );
    }
    assert!(
        wyjete > 0,
        "żadna parcela nie ma zakładu — test byłby spełniony tożsamościowo"
    );
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
