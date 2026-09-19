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
