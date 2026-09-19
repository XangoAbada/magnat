//! Podpięcie warstwy dźwiękowej do okna (M11d §5.9, WP8).
//!
//! Klient robi tu trzy rzeczy, których `engine/audio` zrobić nie może, bo nie widzi
//! ani kamery, ani terenu: składa słuchacza, liczy okluzję i podaje czas realny.
//! Wszystko poza tym jest po tamtej stronie — i to jest cała treść §6.3.

use magnat_audio::{AudioEngine, Listener, Volumes};
use magnat_render::CameraState;
use magnat_world::{Terrain, TerrainQuery};

/// Ile próbek terenu na jeden promień okluzji.
///
/// Osiem, a nie sześćdziesiąt cztery: promień jest liczony najwyżej cztery razy
/// na sekundę dla najwyżej trzydziestu dwóch emiterów, więc 256 odczytów wysokości
/// na sekundę — poniżej progu, na którym warto to mierzyć.
const PROBEK: i32 = 8;

/// Ile okluzji dokłada jeden zasłonięty odcinek promienia.
const NA_PROBKE: f32 = 1.0 / PROBEK as f32;

/// Słuchacz z kamery: stoi w **celu** kamery i patrzy w tę samą stronę co ona.
///
/// W celu, a nie w oku, i to jest ta sama poprawka, którą `X-3` zrobiło dla okna
/// warstwy Mikro i dla wycinka snapshotu — z tego samego powodu. Przy orbicie z 900 m
/// oko wisi wysoko nad miastem, więc emiter w promieniu 220 m od oka **nie istnieje**:
/// widok dzielnicy byłby niemy mimo tysiąca ludzi w kadrze. Cel kamery jest tam, gdzie
/// patrzy gracz, i to on ma być punktem odsłuchu.
///
/// W trybie pierwszoosobowym różnicy nie ma: `CameraState::target()` to wtedy oko plus
/// metr w kierunku patrzenia, czyli głowa postaci.
#[must_use]
pub(crate) fn sluchacz(camera: &CameraState) -> Listener {
    let oko = camera.eye();
    let cel = camera.target();
    let d = cel - oko;
    Listener {
        pos: [cel.x as f32, cel.y as f32, cel.z as f32],
        yaw: (d.y as f32).atan2(d.x as f32),
    }
}

/// Okluzja promienia słuchacz → emiter, 0..1.
///
/// ponytail: zasłania **teren**, nie budynki. Sufit jest jawny: fabryka za wzgórzem
/// jest cichsza, fabryka za blokiem nie. Prawdziwy raycast voxelowy wymagałby dostępu
/// do zmaterializowanych chunków, których klient trzyma tylko podzbiór wokół kamery,
/// więc emiter poza tym podzbiorem dostałby zero i tak. Ścieżka wyjścia, gdy to zacznie
/// przeszkadzać: przecięcie odcinka z `Building.aabb` przez `CsrGrid::query_rect`,
/// czyli indeks, który miasto już ma.
pub(crate) fn okluzja(terrain: &Terrain, from: [f32; 3], to: [f32; 3]) -> f32 {
    let mut zaslonietych = 0;
    for i in 1..=PROBEK {
        let t = i as f32 / (PROBEK + 1) as f32;
        let x = from[0] + (to[0] - from[0]) * t;
        let y = from[1] + (to[1] - from[1]) * t;
        let z = from[2] + (to[2] - from[2]) * t;
        // `height_at` jest w jednostkach 0,5 m (`K-13`).
        let grunt = terrain.height_at(x as i32, y as i32) as f32 * 0.5;
        if grunt > z + 1.0 {
            zaslonietych += 1;
        }
    }
    (zaslonietych as f32 * NA_PROBKE).clamp(0.0, 1.0)
}

/// Uruchamia warstwę dźwiękową. `None` znaczy „gra jest cicha" i **nie jest błędem**:
/// maszyna bez karty dźwiękowej ma się uruchomić, a nie odmówić.
#[must_use]
pub(crate) fn uruchom(wylaczone: bool) -> Option<AudioEngine> {
    if wylaczone {
        return None;
    }
    match AudioEngine::new(Volumes::default()) {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("dźwięk wyłączony: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Słuchacz stoi w celu kamery, a nie w jej oku, i patrzy w stronę celu.
    /// Bez tego widok dzielnicy z orbity jest niemy (`I-17`).
    #[test]
    fn sluchacz_stoi_w_celu_kamery() {
        let k = magnat_render::CameraState {
            mode: magnat_render::CameraMode::Orbit {
                target: glam::DVec3::new(100.0, 0.0, 0.0),
                dist: 50.0,
                yaw: 0.0,
                pitch: 0.0,
            },
            ..crate::preview::startowa_kamera(50.0)
        };
        let l = sluchacz(&k);
        let d = k.target() - k.eye();
        let oczekiwany = (d.y as f32).atan2(d.x as f32);
        assert!((l.yaw - oczekiwany).abs() < 1e-5);
        let cel = k.target();
        assert!((l.pos[0] - cel.x as f32).abs() < 1e-3);
        assert!((l.pos[1] - cel.y as f32).abs() < 1e-3);
        // Oko jest 50 m dalej — gdyby słuchacz stał w nim, emitery przy celu wypadłyby
        // poza promień i miasto byłoby ciche.
        let odlegle = (k.eye() - cel).length();
        assert!(odlegle > 1.0, "kamera stoi w celu: {odlegle} m");
    }
}

#[cfg(test)]
mod progi {
    /// Próg przerzedzania latarni jest **trzema czwartymi pojemności klastra** i nic
    /// w kodzie tego nie wiąże: stała siedzi w `magnat_game`, pojemność w `magnat_render`
    /// (a właściwie w `engine/devtools`), i te dwa crate'y widzi naraz wyłącznie klient.
    ///
    /// Test istnieje, bo to jest **ta sama klasa rozjazdu**, którą naprawiło `I-1`:
    /// plan zakładał 256 świateł na klaster, kod M1 stanął na 64, a próg 192 leżał
    /// powyżej sufitu i nie mógł się zapalić. Bez tego wiązania wróciłby przy pierwszej
    /// zmianie pojemności — i znowu nikt by nie zauważył, bo przerzedzanie, które nigdy
    /// nie działa, wygląda jak przerzedzanie, które nie jest potrzebne.
    #[test]
    fn prog_przerzedzania_wynika_z_pojemnosci_klastra() {
        let pojemnosc = magnat_render::CLUSTER_CAPACITY;
        assert_eq!(
            magnat_game::ambience::CLUSTER_PRESSURE_ON,
            pojemnosc * 3 / 4,
            "próg włączenia rozjechał się z pojemnością klastra ({pojemnosc})"
        );
        // Histereza ma zostawić zapas w obie strony — bez rozstępu latarnie migotałyby
        // przy obrocie kamery wzdłuż arterii.
        const {
            assert!(
                magnat_game::ambience::CLUSTER_PRESSURE_OFF
                    < magnat_game::ambience::CLUSTER_PRESSURE_ON
            )
        };
        assert!(
            magnat_game::ambience::CLUSTER_PRESSURE_ON < pojemnosc,
            "próg {} nie mieści się pod sufitem {pojemnosc}",
            magnat_game::ambience::CLUSTER_PRESSURE_ON
        );
    }
}
