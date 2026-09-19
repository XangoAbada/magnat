//! Walidacja shaderów bez GPU (M1 §7.3).
//!
//! `wgpu` sprawdza WGSL dopiero przy tworzeniu modułu, czyli przy urządzeniu, czyli przy
//! oknie — a okna w CI nie ma. Literówka w shaderze wychodziłaby wtedy dopiero na maszynie
//! z kartą, przy uruchomieniu klienta, w postaci paniki walidatora. `naga` robi dokładnie
//! tę samą robotę (to ten sam kod, którego używa `wgpu`) i robi ją na procesorze.
//!
//! Test nie sprawdza, czy shader **liczy** dobrze — od tego jest zrzut klatki. Sprawdza,
//! czy jest w ogóle poprawnym modułem: typy się zgadzają, wejścia mają lokalizacje,
//! a wbudowane funkcje dostają argumenty, jakich oczekują.

use naga::valid::{Capabilities, ValidationFlags, Validator};

fn waliduj(nazwa: &str, zrodlo: &str) {
    let modul = naga::front::wgsl::parse_str(zrodlo)
        .unwrap_or_else(|e| panic!("{nazwa}: {}", e.emit_to_string(zrodlo)));
    let mut v = Validator::new(ValidationFlags::all(), Capabilities::all());
    if let Err(e) = v.validate(&modul) {
        panic!("{nazwa}: {}", e.emit_to_string(zrodlo));
    }
}

#[test]
fn kazdy_shader_jest_poprawnym_modulem_wgsl() {
    // Lista jawna, nie skan katalogu: shader, który nikt nie wpiął, ma się tu **nie**
    // pojawić sam — inaczej test rósłby o pliki, których nikt nie kompiluje.
    for (nazwa, zrodlo) in [
        ("voxel.wgsl", include_str!("../src/shaders/voxel.wgsl")),
        ("cap.wgsl", include_str!("../src/shaders/cap.wgsl")),
        ("sign.wgsl", include_str!("../src/shaders/sign.wgsl")),
        ("water.wgsl", include_str!("../src/shaders/water.wgsl")),
        ("sky.wgsl", include_str!("../src/shaders/sky.wgsl")),
        ("post.wgsl", include_str!("../src/shaders/post.wgsl")),
        (
            "far_terrain.wgsl",
            include_str!("../src/shaders/far_terrain.wgsl"),
        ),
        (
            "clusters.wgsl",
            include_str!("../src/shaders/clusters.wgsl"),
        ),
        (
            "instance.wgsl",
            include_str!("../src/shaders/instance.wgsl"),
        ),
        (
            "impostor.wgsl",
            include_str!("../src/shaders/impostor.wgsl"),
        ),
        ("weather.wgsl", include_str!("../src/shaders/weather.wgsl")),
    ] {
        waliduj(nazwa, zrodlo);
    }
}

/// Impostor ma ten sam wymóg co geometria: jeden punkt wejścia wierzchołka dla sceny
/// i dla bufora identyfikatorów, bo pass ID rysuje z porównaniem głębi `Equal`.
#[test]
fn impostor_ma_jeden_punkt_wejscia_wierzcholka() {
    let zrodlo = include_str!("../src/shaders/impostor.wgsl");
    let modul = naga::front::wgsl::parse_str(zrodlo).expect("impostor.wgsl");
    let wierzcholki: Vec<&str> = modul
        .entry_points
        .iter()
        .filter(|e| e.stage == naga::ShaderStage::Vertex)
        .map(|e| e.name.as_str())
        .collect();
    let fragmenty: Vec<&str> = modul
        .entry_points
        .iter()
        .filter(|e| e.stage == naga::ShaderStage::Fragment)
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(wierzcholki, ["vs_main"]);
    assert_eq!(fragmenty, ["fs_main", "fs_id"]);
}

/// Bufor ID stoi na tym, że oba przebiegi rysują **tę samą** geometrię: pass ID porównuje
/// głębię przez `Equal`, więc różnica choćby w jednym mnożeniu zeruje trafienia.
/// Wymusza to jeden punkt wejścia wierzchołka dla obu fragmentów — i to jest sprawdzalne
/// statycznie, bez karty graficznej.
#[test]
fn encje_i_bufor_id_dziela_punkt_wejscia_wierzcholka() {
    let zrodlo = include_str!("../src/shaders/instance.wgsl");
    let modul = naga::front::wgsl::parse_str(zrodlo).expect("instance.wgsl");
    let wierzcholki: Vec<&str> = modul
        .entry_points
        .iter()
        .filter(|e| e.stage == naga::ShaderStage::Vertex)
        .map(|e| e.name.as_str())
        .collect();
    let fragmenty: Vec<&str> = modul
        .entry_points
        .iter()
        .filter(|e| e.stage == naga::ShaderStage::Fragment)
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(
        wierzcholki,
        vec!["vs_main"],
        "bufor ID i scena muszą dzielić jeden punkt wejścia wierzchołka"
    );
    assert_eq!(fragmenty, vec!["fs_main", "fs_id"]);
}

/// Indeks atlasu póz jest arytmetyką rozpisaną w dwóch miejscach: `PoseAtlas::bake`
/// układa teksele, `instance.wgsl` je czyta. Rozjazd którejkolwiek z tych stałych nie
/// daje błędu walidacji — daje postać złożoną z części cudzego klipu, czyli objaw,
/// którego nikt nie powiąże ze zmianą liczby klipów.
#[test]
fn stale_animacji_zgadzaja_sie_z_kodem() {
    let zrodlo = include_str!("../src/shaders/instance.wgsl");
    let wartosc = |nazwa: &str| -> String {
        zrodlo
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("const {nazwa}:")))
            .unwrap_or_else(|| panic!("brak stałej {nazwa} w instance.wgsl"))
            .split('=')
            .nth(1)
            .and_then(|s| s.split(';').next())
            .expect("stała bez wartości")
            .trim()
            .trim_end_matches('u')
            .to_string()
    };
    assert_eq!(
        wartosc("POSE_MAX_CLIPS").parse::<usize>().expect("liczba"),
        magnat_voxel::MAX_CLIPS
    );
    assert_eq!(
        wartosc("POSE_TRANS_SCALE").parse::<f32>().expect("liczba"),
        magnat_voxel::POSE_TRANS_SCALE
    );
    // Kolejność pól w tekselu jest kontraktem tak samo jak stałe: shader rozpakowuje
    // `rot.xy | rot.zw | trans.xy | trans.z`, a `PoseTexel` musi mieć te 16 bajtów.
    assert_eq!(core::mem::size_of::<magnat_voxel::PoseTexel>(), 16);
    assert!(
        zrodlo.contains("frame * parts + min(part, parts - 1u)"),
        "shader zmienił arytmetykę indeksu pozy"
    );
}

/// Wybór barwy z zestawu palety liczy **shader**, a wynik sprawdzają testy **w Rust**
/// (`magnat_voxel::palette::pick`). Dwie implementacje jednego wzoru rozjeżdżają się
/// przy pierwszej zmianie i rozjazdu nie widać jako błędu — widać go jako inne barwy
/// na ekranie niż w teście. Stąd porównanie stałych wprost z pliku shadera.
#[test]
fn stale_palety_zgadzaja_sie_z_kodem() {
    let zrodlo = include_str!("../src/shaders/instance.wgsl");
    let stala = |nazwa: &str| -> u32 {
        let wiersz = zrodlo
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("const {nazwa}:")))
            .unwrap_or_else(|| panic!("brak stałej {nazwa} w instance.wgsl"));
        let wartosc = wiersz
            .split('=')
            .nth(1)
            .and_then(|s| s.split(';').next())
            .expect("stała bez wartości")
            .trim()
            .trim_end_matches('u');
        if let Some(hex) = wartosc.strip_prefix("0x") {
            u32::from_str_radix(hex, 16).expect("stała szesnastkowa")
        } else {
            wartosc.parse().expect("stała dziesiętna")
        }
    };
    assert_eq!(stala("PALETTE_MIX"), magnat_voxel::palette::MIX);
    assert_eq!(stala("PALETTE_ROLE_SALT"), magnat_voxel::palette::ROLE_SALT);
    assert_eq!(
        stala("PALETTE_ROLE_COUNT") as usize,
        magnat_voxel::SlotRole::ALL.len()
    );
    // Przesunięcie w `pick` też jest częścią wzoru — szukamy go w treści funkcji.
    assert!(
        zrodlo.contains("(v >> 13u) % ramp.y"),
        "shader zmienił wzór wyboru barwy"
    );
}

/// Pudło cząstek i liczba cząstek na komin są rozpisane po obu stronach granicy GPU:
/// Rust decyduje, ile instancji narysować, shader — gdzie je postawić. Rozjazd nie daje
/// błędu walidacji, tylko deszcz padający obok kamery albo dym poszatkowany na kawałki
/// z dwóch kominów. Ta sama droga, którą `stale_animacji_zgadzaja_sie_z_kodem` pilnuje
/// atlasu póz.
#[test]
fn stale_pogody_zgadzaja_sie_z_kodem() {
    let zrodlo = include_str!("../src/shaders/weather.wgsl");
    let wartosc = |nazwa: &str| -> String {
        zrodlo
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("const {nazwa}:")))
            .unwrap_or_else(|| panic!("brak stałej {nazwa} w weather.wgsl"))
            .split('=')
            .nth(1)
            .and_then(|s| s.split(';').next())
            .expect("stała bez wartości")
            .trim()
            .trim_end_matches('u')
            .to_string()
    };
    assert_eq!(
        wartosc("BOX_Z_M").parse::<f32>().expect("liczba"),
        magnat_render::weather::PRECIP_BOX_Z_M
    );
    assert_eq!(
        wartosc("PARTICLES_PER_PLUME")
            .parse::<u32>()
            .expect("liczba"),
        magnat_render::weather::PARTICLES_PER_PLUME
    );
}

/// Uniform ramki ma jeden układ i **dziewięć** kopii jego deklaracji w WGSL. Kopia,
/// która zgubi pole albo przestawi dwa, czyta cudze bajty — i jest przy tym poprawnym
/// shaderem, więc walidator `naga` tego nie widzi.
///
/// Porównujemy **kolejność**, a nie samą obecność: o tym, które bajty trafiają do
/// którego pola, decyduje wyłącznie pozycja w strukturze. Pierwsza wersja tego testu
/// sprawdzała przynależność i przepuściłaby przestawienie (`I-26`).
#[test]
fn kazda_kopia_uniformu_ramki_ma_te_same_pola() {
    const UKLAD: [&str; 14] = [
        "view_proj",
        "light_view_proj",
        "cascade_far",
        "cascade_texel",
        "sun_dir",
        "sun_color",
        "sky_color",
        "ground_color",
        "fog",
        "clip",
        "screen",
        "eye",
        "overlay",
        "weather",
    ];
    let mut kopii = 0;
    for (nazwa, zrodlo) in [
        ("voxel.wgsl", include_str!("../src/shaders/voxel.wgsl")),
        ("cap.wgsl", include_str!("../src/shaders/cap.wgsl")),
        ("sign.wgsl", include_str!("../src/shaders/sign.wgsl")),
        ("water.wgsl", include_str!("../src/shaders/water.wgsl")),
        ("sky.wgsl", include_str!("../src/shaders/sky.wgsl")),
        (
            "far_terrain.wgsl",
            include_str!("../src/shaders/far_terrain.wgsl"),
        ),
        (
            "instance.wgsl",
            include_str!("../src/shaders/instance.wgsl"),
        ),
        (
            "impostor.wgsl",
            include_str!("../src/shaders/impostor.wgsl"),
        ),
        ("weather.wgsl", include_str!("../src/shaders/weather.wgsl")),
    ] {
        let start = zrodlo
            .find("struct Frame {")
            .unwrap_or_else(|| panic!("{nazwa}: brak `struct Frame`"));
        let koniec = start
            + zrodlo[start..]
                .find(
                    "
}",
                )
                .unwrap_or_else(|| panic!("{nazwa}: niedomknięty `struct Frame`"));
        let pola: Vec<&str> = zrodlo[start..koniec]
            .lines()
            .skip(1)
            .filter_map(|l| l.trim().split(':').next())
            .filter(|n| !n.is_empty() && !n.starts_with("//"))
            .collect();
        assert_eq!(pola, UKLAD, "{nazwa}: inny układ `struct Frame`");
        kopii += 1;
    }
    assert_eq!(kopii, 9, "test ominął kopię");
}

/// Śnieg i pora roku są rozpisane **dwa razy**: w `voxel.wgsl` dla terenu bliskiego
/// i w `far_terrain.wgsl` dla clipmapy za czterema kilometrami. WGSL nie ma dołączania
/// plików, więc duplikat jest nieunikniony — ale rozjazd byłby widoczny dokładnie
/// na granicy pierścienia LOD, czyli tam, gdzie patrzy się najczęściej (`I-27`).
#[test]
fn obie_kopie_pogody_na_terenie_maja_te_same_liczby() {
    let bliski = include_str!("../src/shaders/voxel.wgsl");
    let daleki = include_str!("../src/shaders/far_terrain.wgsl");
    for wzorzec in [
        // Próg rozpoznania zieleni.
        "albedo.g > albedo.r * 1.15",
        // Trzy przesunięcia sezonowe.
        "vec3<f32>(0.85, 0.82, 0.78)",
        "vec3<f32>(0.92, 1.12, 0.80)",
        "vec3<f32>(1.45, 1.00, 0.45)",
        // Barwa i przyczepność śniegu.
        "vec3<f32>(0.90, 0.93, 0.98)",
        "smoothstep(0.35, 0.85, n.z)",
    ] {
        assert!(bliski.contains(wzorzec), "voxel.wgsl nie ma `{wzorzec}`");
        assert!(
            daleki.contains(wzorzec),
            "far_terrain.wgsl nie ma `{wzorzec}`"
        );
    }
}
