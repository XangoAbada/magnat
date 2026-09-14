//! Determinizm i zgodność z tabelą potrzeb (M3a: kryteria WP1 i WP2; 00 §3.6).

use magnat_agents::{
    register, register_components, AgentState, Employment, Identity, Lifecycle, NeedDecaySystem,
    NeedTable, Needs, Personality, PlanRef, Residence, Skills, Vitals, Wealth,
};
use magnat_agents::{KnowledgeRef, RelationsRef};
use magnat_core::NeedKind;
use magnat_ecs::{App, Entity, ScheduleBuilder, World};
use magnat_io::{load_world, save_world, world_state_hash};

const MIESZKANCOW: u32 = 600;

fn swiat(seed: u64, n: u32) -> World {
    swiat_z(seed, n, true)
}

fn swiat_z(seed: u64, n: u32, zasoby: bool) -> World {
    let mut w = World::new(seed);
    if zasoby {
        register(
            &mut w,
            NeedTable::load_default().expect("data/needs/needs.ron"),
        );
    } else {
        register_components(&mut w);
    }
    for i in 0..n {
        let _ = w
            .spawn()
            .with(Identity {
                first_name: (i % 97) as u16,
                last_name: (i % 131) as u16,
                birth_day: -(360 * 25) - (i as i32 % 9_000),
                flags: Identity::FLAG_ALIVE | ((i % 2) as u8),
                household: i / 3,
                ..Identity::default()
            })
            .with(Personality([(40 + i % 40) as u8; 8]))
            .with(Vitals {
                health: (60 + i % 40) as u8,
                energy: (50 + i % 50) as u8,
                ..Vitals::default()
            })
            .with(Needs::default())
            .with(Skills::default())
            .with(Wealth::default())
            .with(Employment {
                site: i % 50,
                work_days: Employment::WEEKDAYS,
                ..Employment::default()
            })
            .with(Residence::default())
            .with(PlanRef::default())
            .with(AgentState::default())
            .with(KnowledgeRef::default())
            .with(RelationsRef::default())
            .with(Lifecycle::default());
    }
    w
}

fn aplikacja(seed: u64, watki: usize) -> App {
    let world = swiat(seed, MIESZKANCOW);
    let schedule = {
        let mut b = ScheduleBuilder::new();
        b.add(NeedDecaySystem::new(&world));
        b.build().expect("harmonogram")
    };
    App::new(world, schedule, watki)
}

#[test]
fn hash_stanu_przezywa_zapis_i_odczyt() {
    // Kryterium WP1: komponenty M3 są w funkcji haszującej i przeżywają serializację.
    // Bez tego test determinizmu fazy sprawdzałby świat, w którym mieszkańców nie ma.
    // Świat bez zasobów: minimalny snapshot z M0 niesie komponenty, nie slaby
    // ani kolejkę zdarzeń (pełny zapis to M12 — patrz `register_components`).
    // Zakres kryterium WP1 to komponenty i to jest to, co tu sprawdzamy.
    let mut world = swiat_z(7, MIESZKANCOW, false);
    world.tick = magnat_core::Tick(180);
    let przed = world_state_hash(&world);

    let dir = std::env::temp_dir().join("magnat-m3a-snapshot");
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    let plik = dir.join("agents.mgs");
    save_world(&world, &plik).expect("zapis");
    let wczytany = load_world(&plik, world.components()).expect("odczyt");
    let po = world_state_hash(&wczytany);
    let _ = std::fs::remove_file(&plik);

    assert_eq!(przed, po, "hash stanu zmienił się po zapisie i odczycie");
    assert_eq!(wczytany.entity_count(), MIESZKANCOW);

    // Test negatywny: gdyby komponenty M3 **nie** wchodziły do hasha, zmiana poziomu
    // potrzeby jednego mieszkańca niczego by nie ruszyła i powyższa równość byłaby
    // spełniona tożsamościowo.
    let mut zmieniony = wczytany;
    let pierwszy = zmieniony
        .archetypes()
        .iter()
        .flat_map(|a| a.chunks())
        .flat_map(|c| c.entities().to_vec())
        .min()
        .expect("pusty świat");
    zmieniony
        .get_mut::<Needs>(pierwszy)
        .expect("mieszkaniec bez potrzeb")
        .set(NeedKind::Hunger, magnat_core::Q::new(3));
    assert_ne!(
        po,
        world_state_hash(&zmieniony),
        "zmiana poziomu głodu nie ruszyła hasha — komponenty M3 są poza funkcją haszującą"
    );
}

#[test]
fn dwa_przebiegi_tego_samego_ziarna_daja_ten_sam_hash() {
    let odcisk = |watki: usize| {
        let mut app = aplikacja(11, watki);
        let mut hashe = Vec::new();
        for _ in 0..3 {
            app.run_ticks(500);
            hashe.push(world_state_hash(&app.world));
        }
        hashe
    };
    assert_eq!(odcisk(1), odcisk(1));
    assert_eq!(odcisk(1), odcisk(4), "wynik zależy od liczby wątków");
}

#[test]
fn shard_daje_ten_sam_wynik_co_zamknieta_formula() {
    // Kryterium WP2, tolerancja 0: shard 1/60 nie może zmienić poziomu potrzeby
    // względem przeliczenia „wszyscy naraz". To nie jest kwestia kalibracji, tylko
    // konsekwencja liczenia spadku z czasu absolutnego (§5.5).
    let tabela = NeedTable::load_default().expect("data/needs/needs.ron");
    let mut app = aplikacja(3, 1);
    app.run_ticks(1_440); // pełna doba

    let mut sprawdzonych = 0;
    let mut najstarszy_brak = 0u64;
    let encje: Vec<Entity> = app
        .world
        .archetypes()
        .iter()
        .flat_map(|a| a.chunks())
        .flat_map(|c| c.entities().to_vec())
        .collect();
    for e in encje {
        let needs = *app.world.get::<Needs>(e).expect("mieszkaniec bez potrzeb");
        for n in NeedKind::ALL {
            let oczekiwany =
                100u32.saturating_sub(tabela.decay_between(*n, 0, u64::from(needs.updated_at)));
            assert_eq!(
                u32::from(needs.level[n.as_index()]),
                oczekiwany.min(100),
                "mieszkaniec {} potrzeba {} rozjechała się z formułą przy updated_at={}",
                e.index(),
                n.name(),
                needs.updated_at
            );
        }
        // Każdy shard musi dostać swoją kolej w ciągu godziny — inaczej „1/60
        // populacji na tick" znaczyłoby coś innego, niż mówi §5.5.
        najstarszy_brak = najstarszy_brak.max(1_440 - u64::from(needs.updated_at));
        sprawdzonych += 1;
    }
    assert_eq!(sprawdzonych, MIESZKANCOW);
    assert!(
        najstarszy_brak <= 60,
        "najdłużej nieobsłużony mieszkaniec czekał {najstarszy_brak} minut"
    );
}

#[test]
fn czas_do_zera_zgadza_sie_z_tabela_paragrafu_5_5() {
    // Kryterium WP2: mieszkaniec bez żadnego zaspokojenia osiąga stan krytyczny
    // w czasie zgodnym z tabelą (±2 %). Tabela jest **przepisana z dokumentu fazy**:
    // gdyby dane się od niej oderwały, ten test ma paść, a nie zostać poprawiony.
    // Kolumna „pełna → 0" jest konsekwencją tempa, nie drugim parametrem —
    // po korekcie D-2 podaje wartości dokładne, a nie zaokrąglone do ładnej liczby.
    //
    // **Korekta D-15 (wpisana po M3d):** cztery potrzeby wypadły z tej tabeli, bo mają
    // dziś tempo 0. `Safety`, `Housing`, `Mobility` i `Status` nie mają w M3 ani miejsca
    // zaspokojenia, ani mechanizmu odbudowy, więc spadek doprowadzał je do zera
    // u **wszystkich** i ich skutki przestawały cokolwiek różnicować — mobilność
    // z `AbsenceRisk` 1000 zatrzymała w drugiej dobie całe miasto w pracy. Tempo wpisze
    // faza, która wniesie mechanizm: M4, M8, M5/M9. Ich wiersze są niżej, jako lista
    // potrzeb zdarzeniowych — żeby ta zmiana **też** miała strażnika.
    //
    // **M4b wniósł mechanizm dla mobilności i strażnik zadziałał:** potrzebę podnosi
    // zakończona podróż (`TrafficSystem`, `Z-2`), więc `Mobility` przechodzi z listy
    // zdarzeniowych do tabeli temp. `places` zostaje przy niej **puste** — mobilności
    // nie zaspokaja wizyta gdziekolwiek, tylko sam fakt dojechania. Pozostałe trzy
    // czekają dalej: `Safety` i `Housing` na M8, `Status` na M5/M9.
    const TABELA_H: [(NeedKind, f64); 8] = [
        (NeedKind::Mobility, 166.7),
        (NeedKind::Hunger, 16.7),
        (NeedKind::Sleep, 23.8),
        (NeedKind::Hygiene, 23.8),
        (NeedKind::Clothing, 6.9 * 24.0),
        (NeedKind::Leisure, 33.3),
        (NeedKind::Social, 40.0),
        (NeedKind::Development, 83.3 * 24.0),
    ];
    /// Potrzeby zdarzeniowe: nie spadają same, bo w M3 nic ich nie podnosi.
    const ZDARZENIOWE: [NeedKind; 4] = [
        NeedKind::Health,
        NeedKind::Safety,
        NeedKind::Housing,
        NeedKind::Status,
    ];

    let tabela = NeedTable::load_default().expect("data/needs/needs.ron");
    for need in ZDARZENIOWE {
        assert_eq!(
            tabela.decay_between(need, 0, 60 * 24 * 400),
            0,
            "{} spada, a nic jej w M3 nie podnosi (korekta D-15)",
            need.name()
        );
    }
    for (need, godziny) in TABELA_H {
        // Szukamy minuty, w której poziom faktycznie dochodzi do zera — przez tę samą
        // funkcję, której używa system, a nie przez odwrócenie wzoru.
        let mut minuta = 0u64;
        while tabela.decay_between(need, 0, minuta) < 100 {
            minuta += 1;
            assert!(minuta < 60 * 24 * 400, "{} nie spada wcale", need.name());
        }
        let zmierzone = minuta as f64 / 60.0;
        let odchylenie = (zmierzone - godziny).abs() / godziny;
        assert!(
            odchylenie <= 0.02,
            "{}: {zmierzone:.1} h wobec {godziny:.1} h z tabeli (odchylenie {:.1} %)",
            need.name(),
            odchylenie * 100.0
        );
    }

    // Zdrowie jest zdarzeniowe — nie ma go w tabeli czasów i nie może spadać samo.
    assert_eq!(
        tabela.decay_between(NeedKind::Health, 0, 60 * 24 * 3_600),
        0
    );
}
