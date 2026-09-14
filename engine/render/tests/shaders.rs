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
        ("water.wgsl", include_str!("../src/shaders/water.wgsl")),
        ("sky.wgsl", include_str!("../src/shaders/sky.wgsl")),
        ("post.wgsl", include_str!("../src/shaders/post.wgsl")),
        (
            "far_terrain.wgsl",
            include_str!("../src/shaders/far_terrain.wgsl"),
        ),
        ("clusters.wgsl", include_str!("../src/shaders/clusters.wgsl")),
        (
            "pedestrian.wgsl",
            include_str!("../src/shaders/pedestrian.wgsl"),
        ),
    ] {
        waliduj(nazwa, zrodlo);
    }
}

/// Bufor ID stoi na tym, że oba przebiegi rysują **tę samą** geometrię: pass ID porównuje
/// głębię przez `Equal`, więc różnica choćby w jednym mnożeniu zeruje trafienia.
/// Wymusza to jeden punkt wejścia wierzchołka dla obu fragmentów — i to jest sprawdzalne
/// statycznie, bez karty graficznej.
#[test]
fn piesi_i_bufor_id_dziela_punkt_wejscia_wierzcholka() {
    let zrodlo = include_str!("../src/shaders/pedestrian.wgsl");
    let modul = naga::front::wgsl::parse_str(zrodlo).expect("pedestrian.wgsl");
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
