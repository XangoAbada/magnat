//! **Render, dźwięk i poziom detalu nie zmieniają symulacji** (M11 §7.1, DoD §7.5 pkt 1).
//!
//! To jest najważniejszy test fazy i jedyny, którego kryterium brzmi „tolerancja zero".
//! Kontrakt z dok. 00 §4: zbiór encji w kadrze zależy od kamery, a kamera **nie wchodzi
//! do hasha stanu** — więc gdyby cokolwiek po stronie prezentacji zapisywało do świata,
//! obrót kamerą zmieniałby saldo gospodarstwa domowego i determinizm (00 §3) upadałby.
//!
//! # Co ten test obejmuje, a czego nie
//!
//! Przebieg odniesienia **nie woła prezentacji w ogóle**. Przebieg porównawczy przechodzi
//! w każdym ticku całą drogę klienta: wypełnienie snapshotu, złożenie bufora instancji
//! i zaplanowanie miksu dźwięku. Wszystkie warianty §7.1 mierzą się tą jedną osią:
//!
//! | Kryterium §7.1 | Wariant tutaj |
//! |---|---|
//! | `render_off_equals_render_on` | `Prezentacja::Brak` wobec pozostałych |
//! | `camera_path_does_not_matter` | trzy skrypty kamery: nieruchoma, przelot, ulica |
//! | `lod_scale_does_not_matter` | cap 24 576 wobec 4 096 przy tej samej kamerze |
//! | `audio_off_equals_audio_on` | plan miksu liczony albo pomijany |
//!
//! **Czego tu nie ma: GPU.** I to nie jest luka, tylko granica postawiona świadomie.
//! Passy renderu przyjmują `&RenderSnapshot` (§6.3 pkt 3), a `engine/render`
//! i `engine/audio` nie mają w grafie zależności ani jednego crate'u symulacji
//! (§6.3 pkt 1, bramka `dep_isolation` w CI). Droga z karty graficznej do `World`
//! nie istnieje **w typach**, więc przebieg z oknem mógłby najwyżej powtórzyć to,
//! co tu widać. Tym, co realnie sięga po `&World`, jest wyłącznie wypełniacz
//! snapshotu — i to on jest tu wołany naprawdę, a nie w atrapie.
//!
//! `#[ignore]` z tego samego powodu co `snapshot_instancing.rs`: generacja miasta
//! i pół doby symulacji kosztują sekundy. CI uruchamia je przez `--include-ignored`.

use magnat_agents::places::InfinitePlaces;
use magnat_agents::{
    bootstrap_day, register_day, AgentSources, DayLoopSystem, NeedDecaySystem, NeedTable,
    ReplanCooldownSystem,
};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_game::SnapshotFiller;
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_jobs::JobPool;
use magnat_render::instancing::{build_instances, InstanceScratch, LodBands, ModelTable};
use magnat_sim_snapshot::{Aabb, RenderSnapshot, SnapshotCaps, ViewQuery};
use magnat_traffic::TrafficSystem;
use magnat_voxel::PaletteLibrary;
use std::sync::Arc;

const OKNO_MIKRO_M: u32 = 900;
/// Do 8:20 — szczyt poranny, kiedy na chodnikach jest kto ma być.
const MINUT: u64 = 500;
const HASH_CO: u64 = 50;

/// Co robi warstwa prezentacji w danym przebiegu.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Prezentacja {
    /// `None` = przebieg bez renderu w ogóle.
    kamera: Option<Kamera>,
    caps: SnapshotCaps,
    dzwiek: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kamera {
    /// Nieruchomo nad środkiem miasta.
    Stoi,
    /// Przelot przez miasto: okno kadru wędruje z minutą.
    Przelot,
    /// Poziom oczu pieszego — kadr wąski i nisko.
    Ulica,
}

impl Prezentacja {
    const BRAK: Prezentacja = Prezentacja {
        kamera: None,
        caps: SnapshotCaps::DEFAULT,
        dzwiek: false,
    };

    fn z(kamera: Kamera) -> Prezentacja {
        Prezentacja {
            kamera: Some(kamera),
            caps: SnapshotCaps::DEFAULT,
            dzwiek: true,
        }
    }
}

/// Cap obniżony do wartości z §7.1 („4 096 / 24 576").
const CAP_MALY: SnapshotCaps = SnapshotCaps {
    citizens: 4_096,
    vehicles: 4_096,
    sites: 4_096,
    crowd: 2_048,
};

/// Cap, który **na pewno** obcina.
///
/// Osobno od `CAP_MALY`, i to nie jest ostrożność bez powodu: §7.1 wymienia liczby
/// 4 096 i 24 576, ale w mieście 4 km z ośmioma tysiącami mieszkańców w kadrze może
/// nie być ani jednego ponad cztery tysiące — i wtedy wariant „cap 4 096" jest bit
/// w bit powtórką wariantu bez capu, a kryterium trzyma się zielone, bo nikt go
/// nie mierzy. Dwieście pięćdziesiąt sześć obcina zawsze i to sprawdza asercja.
const CAP_TNACY: SnapshotCaps = SnapshotCaps {
    citizens: 256,
    vehicles: 64,
    sites: 64,
    crowd: 64,
};

/// Kadr przebiegu w danej minucie, w milimetrach.
fn zapytanie(k: Kamera, caps: SnapshotCaps, srodek: (f64, f64), minuta: u64) -> ViewQuery {
    let (dx, promien, wys) = match k {
        Kamera::Stoi => (0.0, 900_000, 200_000),
        // Osiemset metrów w poprzek miasta w ciągu przebiegu — okno przechodzi
        // przez dzielnice, w których ludzie akurat idą do pracy.
        Kamera::Przelot => ((minuta as f64) * 1.6, 900_000, 140_000),
        Kamera::Ulica => (-200.0, 120_000, 1_700),
    };
    let oko = [
        ((srodek.0 + dx) * 1000.0) as i32,
        (srodek.1 * 1000.0) as i32,
        wys,
    ];
    ViewQuery {
        aabb: Aabb::around(oko, promien, promien),
        eye: oko,
        caps,
        anim_ms: minuta * 16,
    }
}

/// Wynik przebiegu: łańcuch hashy i szczyt encji, które trafiły do bufora instancji.
///
/// Druga liczba jest **bramką na samego siebie**: przebieg, w którym kamera nie wpuściła
/// do kadru ani jednej encji, porównywałby dwa identyczne przebiegi bez prezentacji
/// i nie dowodziłby niczego. Ta sama ostrożność co przy `micro_mezo_equivalence` w M4d.
struct Wynik {
    hashe: Vec<String>,
    szczyt_instancji: usize,
    /// Najwięcej mieszkańców, jakich snapshot oddał w jednej klatce — dowód, że cap
    /// w danym wariancie faktycznie obciął, a nie tylko został podany.
    szczyt_mieszkancow: usize,
}

/// Jeden przebieg: `MINUT` minut świata, hash stanu co `HASH_CO`.
fn przebieg(seed: u64, p: Prezentacja) -> Wynik {
    let pool = JobPool::new(0);
    let city = zbuduj_miasto(seed, "4km", "lowland", "1990", "mixed", &pool).expect("miasto");
    let mut world = swiat_agentow(seed).expect("świat");
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, 8_000, 200_000).expect("Etap 8");

    let srodek = (f64::from(city.center.x), f64::from(city.center.y));
    let oracle = zaludnione.travel_oracle();
    // Warstwa Mikro chodzi w **obu** przebiegach: gdyby chodziła tylko przy włączonym
    // renderze, test porównywałby dwa różne modele ruchu zamiast dwóch dróg do tego
    // samego. To jest dokładnie ta sama teza co `micro_writes_nothing` w M4d, tylko
    // sprawdzana od strony prezentacji.
    oracle.set_micro_window(Some((srodek.0 as i32, srodek.1 as i32)), OKNO_MIKRO_M);
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

    let mut filler = SnapshotFiller::new();
    if p.kamera.is_some() {
        let palettes =
            PaletteLibrary::load(&magnat_world::data_path("palettes")).expect("data/palettes");
        filler.bind_city(&city, &palettes);
    }
    let mut snap = RenderSnapshot::default();
    let mut scratch = InstanceScratch::default();
    let modele = ModelTable::default();
    let mut hashe = Vec::new();
    let mut szczyt = 0usize;
    let mut szczyt_mieszkancow = 0usize;

    for minuta in 1..=MINUT {
        app.tick();
        if let Some(k) = p.kamera {
            let q = zapytanie(k, p.caps, srodek, minuta);
            filler.fill(&app.world, &q, &mut snap);
            // Bufor instancji i miks dźwięku widzą **wyłącznie** snapshot — i to jest
            // teza, nie założenie: obie funkcje przyjmują `&RenderSnapshot`, więc
            // nie mają czym sięgnąć do świata. Wołamy je mimo to, bo test ma pokazać,
            // że cała droga klienta przebiega bez śladu, a nie że jej nie uruchomiono.
            let oko = glam::DVec3::new(
                f64::from(q.eye[0]) / 1000.0,
                f64::from(q.eye[1]) / 1000.0,
                f64::from(q.eye[2]) / 1000.0,
            );
            build_instances(
                &snap,
                oko,
                &stozek(),
                &modele,
                LodBands::default(),
                &[],
                &mut scratch,
            );
            szczyt = szczyt.max(scratch.instances.len());
            szczyt_mieszkancow = szczyt_mieszkancow.max(snap.citizens.len());
            if p.dzwiek {
                let sluchacz = magnat_audio::Listener {
                    pos: [
                        q.eye[0] as f32 / 1000.0,
                        q.eye[1] as f32 / 1000.0,
                        q.eye[2] as f32 / 1000.0,
                    ],
                    ..Default::default()
                };
                let _ = magnat_audio::plan(&snap, &sluchacz, &|_| 1.0);
            }
        }
        let t = app.world.tick.get();
        if t.is_multiple_of(HASH_CO) {
            hashe.push(format!("{}", magnat_io::world_state_hash(&app.world)));
        }
    }
    Wynik {
        hashe,
        szczyt_instancji: szczyt,
        szczyt_mieszkancow,
    }
}

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

/// `render_off_equals_render_on`, `camera_path_does_not_matter`,
/// `lod_scale_does_not_matter` i `audio_off_equals_audio_on` z §7.1 — jeden przebieg
/// odniesienia i pięć porównań, bo generacja miasta kosztuje sekundy i nie ma powodu
/// robić jej sześć razy po to, żeby wypisać cztery nazwy testów.
#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn prezentacja_nie_zmienia_ani_jednego_hasha() {
    const SEED: u64 = 0x4D41_474E_4154;
    let bez = przebieg(SEED, Prezentacja::BRAK);
    assert!(
        bez.hashe.len() >= 10,
        "przebieg odniesienia dał {} hashy — za mało, żeby cokolwiek dowieść",
        bez.hashe.len()
    );
    // Druga bramka na samego siebie: świat musi się **zmieniać**. Łańcuch stałych
    // hashy byłby zgodny z każdym innym łańcuchem stałych hashy, więc test przechodziłby
    // także wtedy, gdyby symulacja stała. Tego rodzaju zielony wynik jest gorszy od
    // czerwonego, bo wygląda jak dowód.
    let roznych: std::collections::BTreeSet<&String> = bez.hashe.iter().collect();
    assert!(
        roznych.len() > 1,
        "wszystkie {} hashy są identyczne — świat nie tyka i test niczego nie mierzy",
        bez.hashe.len()
    );

    // Ile mieszkańców kadr oddaje bez obcięcia — punkt odniesienia dla wariantu z capem.
    let odniesienie_mieszkancow =
        przebieg(SEED, Prezentacja::z(Kamera::Przelot)).szczyt_mieszkancow;
    assert!(
        odniesienie_mieszkancow > CAP_TNACY.citizens as usize,
        "kadr oddaje {odniesienie_mieszkancow} mieszkańców, czyli mniej niż cap tnący —          obniż cap albo zagęść scenę, bo inaczej wariant capa niczego nie mierzy"
    );

    let warianty = [
        ("render_off_equals_render_on", Prezentacja::z(Kamera::Stoi)),
        (
            "camera_path_does_not_matter (przelot)",
            Prezentacja::z(Kamera::Przelot),
        ),
        (
            "camera_path_does_not_matter (ulica)",
            Prezentacja::z(Kamera::Ulica),
        ),
        (
            "lod_scale_does_not_matter (cap 4096)",
            Prezentacja {
                caps: CAP_MALY,
                ..Prezentacja::z(Kamera::Przelot)
            },
        ),
        (
            "lod_scale_does_not_matter (cap tnący)",
            Prezentacja {
                caps: CAP_TNACY,
                ..Prezentacja::z(Kamera::Przelot)
            },
        ),
        (
            "audio_off_equals_audio_on",
            Prezentacja {
                dzwiek: false,
                ..Prezentacja::z(Kamera::Przelot)
            },
        ),
    ];

    for (nazwa, p) in warianty {
        let w = przebieg(SEED, p);
        assert!(
            w.szczyt_instancji > 0,
            "{nazwa}: kamera nie wpuściła do kadru ani jednej encji — test porównałby \
             wtedy dwa przebiegi bez prezentacji i nie dowiódłby niczego"
        );
        assert_eq!(
            w.hashe, bez.hashe,
            "{nazwa}: warstwa prezentacji zmieniła ciąg hashy stanu — coś po stronie \
             renderu, dźwięku albo wypełniacza zapisuje do symulacji (00 §4)"
        );
    }
}
