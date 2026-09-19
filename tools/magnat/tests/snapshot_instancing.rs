//! Cała droga sim → snapshot → instancje, na prawdziwym mieście (M11a WP2).
//!
//! Testy jednostkowe sprawdzają każdy człon osobno: wypełniacz nie rusza stanu, selektor
//! jest deterministyczny, instancje schodzą do wsadów. Żaden z nich nie odpowiada na
//! pytanie, które ma znaczenie — **czy po drodze cokolwiek zostaje**. Wystarczyłby jeden
//! rozjazd jednostek albo jedno okno warstwy Mikro ustawione nie tam, gdzie patrzy kamera,
//! i wynik byłby zerem przy wszystkich zielonych testach jednostkowych.
//!
//! Test stoi w `tools/magnat`, bo to jedyny pakiet widzący naraz `magnat-game`
//! (wypełniacz) i `magnat-render` (instancing). GPU nie jest potrzebne: mierzona jest
//! ścieżka procesora, a ta kończy się na buforze instancji.
//!
//! `#[ignore]` z tego samego powodu co `full_city.rs`: generacja miasta i pół doby
//! symulacji kosztują sekundy. CI uruchamia je jawnie przez `--include-ignored`.

use magnat_agents::places::InfinitePlaces;
use magnat_agents::{
    bootstrap_day, register_day, AgentSources, DayLoopSystem, NeedDecaySystem, NeedTable,
    ReplanCooldownSystem,
};
use std::sync::Arc;
use magnat_ecs::{App, ScheduleBuilder};
use magnat_game::SnapshotFiller;
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_jobs::JobPool;
use magnat_render::instancing::{
    build_instances, InstanceScratch, LodBands, MeshSlot, ModelTable,
};
use magnat_sim_snapshot::{Aabb, RenderSnapshot, SnapshotCaps, ViewQuery};
use magnat_traffic::TrafficSystem;
use magnat_voxel::{ModelLibrary, PaletteLibrary, LOD_COUNT};

/// Promień okna warstwy Mikro. Ta sama liczba, którą ustawia klient.
const OKNO_MIKRO_M: u32 = 900;

/// Stożek widzenia obejmujący wszystko: test mierzy przepływ danych, a nie kadrowanie,
/// a kadrowanie ma własny test jednostkowy.
fn stozek() -> [glam::Vec4; 6] {
    [
        glam::Vec4::new(1.0, 0.0, 0.0, 1.0e6),
        glam::Vec4::new(-1.0, 0.0, 0.0, 1.0e6),
        glam::Vec4::new(0.0, 1.0, 0.0, 1.0e6),
        glam::Vec4::new(0.0, -1.0, 0.0, 1.0e6),
        glam::Vec4::new(0.0, 0.0, 1.0, 1.0e6),
        glam::Vec4::new(0.0, 0.0, -1.0, 1.0e6),
    ]
}

/// Tablica modeli zbudowana z **prawdziwych** siatek z `data/models/`, bez GPU.
fn tablica_modeli(models: &ModelLibrary) -> ModelTable {
    let mut slots = vec![MeshSlot::default(); models.len() * LOD_COUNT as usize];
    for (id, m) in models.iter() {
        for lod in 0..LOD_COUNT {
            let siatka = magnat_voxel::build_model_mesh(m, lod);
            if siatka.is_empty() {
                continue;
            }
            slots[id.0 as usize * LOD_COUNT as usize + lod as usize] = MeshSlot {
                index_offset: 0,
                index_count: siatka.indices.len() as u32,
                base_vertex: 0,
                radius_m: siatka.bounding_radius_m(),
            };
        }
    }
    ModelTable::new(
        slots,
        models.id_of("citizen").expect("data/models/citizen.mvox"),
        models.id_of("car").expect("data/models/car.mvox"),
    )
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn miasto_o_osmej_rano_daje_instancje_z_palety_dzielnicy() {
    let pool = JobPool::new(0);
    let city = zbuduj_miasto(7, "4km", "lowland", "1990", "mixed", &pool).expect("miasto");
    let mut world = swiat_agentow(7).expect("świat");
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, 8_000, 200_000).expect("Etap 8");

    // Okno warstwy Mikro **przed** krokami: warstwa krokuje tylko to, co w nim stoi,
    // więc ustawienie go po tickach dałoby puste miasto i test mierzyłby własny błąd.
    let srodek = city.center;
    let oracle = zaludnione.travel_oracle();
    oracle.set_micro_window(Some((srodek.x as i32, srodek.y as i32)), OKNO_MIKRO_M);
    // Ta sama droga, którą `game::world::stand_up` stawia świat bez rynku: katalog
    // miejsc nieskończonych plus estymator podróży. Bez tego `AgentSources` jest pusty
    // i pętla doby nie krokuje nikogo.
    let tabela = Arc::new(NeedTable::load_default().expect("tabela potrzeb"));
    *world.resource_mut::<AgentSources>() = AgentSources::new(
        Box::new(InfinitePlaces::new(zaludnione.places.clone(), tabela)),
        oracle,
    );
    bootstrap_day(&mut world, 0);

    let mut b = ScheduleBuilder::new();
    b.add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(TrafficSystem::new(&world));
    let schedule = b.build().expect("harmonogram");
    let mut app = App::new(world, schedule, 0);
    // Do 8:20 — szczyt poranny, kiedy na chodnikach jest kto ma być.
    for _ in 0..500 {
        app.tick();
    }
    // Okno przeżywa kroki, ale zerujący je `stand_up` nie biegnie w tym teście —
    // ustawiamy je ponownie, żeby test nie zależał od kolejności resetów.
    app.world
        .resource::<AgentSources>()
        .get()
        .expect("AgentSources")
        .travel
        .set_micro_window(Some((srodek.x as i32, srodek.y as i32)), OKNO_MIKRO_M);
    app.tick();

    let palettes =
        PaletteLibrary::load(&magnat_world::data_path("palettes")).expect("data/palettes");
    let models = ModelLibrary::load_dir(&magnat_world::data_path("models")).expect("data/models");

    let oko_mm = [
        (srodek.x * 1000.0) as i32,
        (srodek.y * 1000.0) as i32,
        30_000,
    ];
    let zapytanie = ViewQuery {
        aabb: Aabb::around(oko_mm, 900_000, 900_000),
        eye: oko_mm,
        caps: SnapshotCaps::DEFAULT,
    };

    let mut filler = SnapshotFiller::new();
    filler.bind_city(&city, &palettes);
    let mut snap = RenderSnapshot::default();
    filler.fill(&app.world, &zapytanie, &mut snap);

    assert!(
        !snap.citizens.is_empty(),
        "snapshot pusty — warstwa Mikro nie oddała ani jednego pieszego"
    );

    // Paleta dzielnicy: przynajmniej jeden mieszkaniec ma stanąć w dzielnicy, dla której
    // katalog ma wpis inny niż zerowy. Zero wszędzie znaczyłoby, że klucze `DistrictKind`
    // rozjechały się z `data/palettes/` — a to jest awaria, której nie widać inaczej
    // niż jako jednolita tonacja całego miasta.
    let uzyte: std::collections::BTreeSet<u16> = snap
        .citizens
        .as_slice()
        .iter()
        .map(|c| snap.district_palette[c.district as usize % 64])
        .collect();
    assert!(
        uzyte.iter().any(|p| *p != 0),
        "każdy mieszkaniec dostał paletę 0 — klucze dzielnic nie zgadzają się z katalogiem"
    );

    // Wygląd: mieszkańcy nie mogą być identyczni. Ten sam wariant u wszystkich znaczy,
    // że `Appearance::derive` dostaje stały klucz i cała dzielnica chodzi w jednym stroju.
    let warianty: std::collections::BTreeSet<u32> = snap
        .citizens
        .as_slice()
        .iter()
        .map(|c| c.appearance)
        .collect();
    assert!(
        warianty.len() * 4 > snap.citizens.len(),
        "{} wyglądów na {} mieszkańców — wariant nie zależy od encji",
        warianty.len(),
        snap.citizens.len()
    );

    // Kryterium WP1 w postaci liczbowej: **ci sami ludzie mają różne ubrania**.
    // Barwę liczy tu ta sama funkcja, którą liczy ją shader (`palette::pick`), więc
    // to jest sprawdzenie całego łańcucha „wygląd → paleta dzielnicy → kolor",
    // a nie tylko tego, że warianty się różnią.
    let barwy: std::collections::BTreeSet<[u8; 4]> = snap
        .citizens
        .as_slice()
        .iter()
        .map(|c| {
            palettes.color(
                magnat_voxel::DistrictPaletteId(snap.district_palette[c.district as usize % 64]),
                magnat_voxel::SlotRole::OutfitMain,
                c.appearance,
            )
        })
        .collect();
    assert!(
        barwy.len() >= 6,
        "{} różnych barw ubrania na {} mieszkańców — paleta nie różnicuje",
        barwy.len(),
        snap.citizens.len()
    );

    let mut scratch = InstanceScratch::default();
    build_instances(
        &snap,
        glam::DVec3::new(
            f64::from(oko_mm[0]) / 1000.0,
            f64::from(oko_mm[1]) / 1000.0,
            f64::from(oko_mm[2]) / 1000.0,
        ),
        &stozek(),
        &tablica_modeli(&models),
        LodBands::default(),
        &mut scratch,
    );
    assert!(
        !scratch.instances.is_empty(),
        "{} mieszkańców w snapshocie, zero instancji — ścieżka klatki gubi wszystko",
        snap.citizens.len()
    );
    assert!(
        scratch.batches.len() <= 24,
        "{} wsadów rysowania, limit 24",
        scratch.batches.len()
    );
    // Każda instancja ma mieć identyfikator — inaczej bufor ID zapisze zero i kliknięcie
    // w widoczną encję nie trafi w nic.
    assert!(
        scratch.instances.iter().all(|i| i.pick != 0),
        "instancja bez identyfikatora w buforze ID"
    );
}
