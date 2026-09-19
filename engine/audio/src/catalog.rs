//! Katalog dźwięku z `data/audio/audio.ron` (M11d §5.9).
//!
//! W katalogu nie ma ani jednego nagrania i to jest decyzja, nie brak: dźwięki
//! **generuje** [`crate::synth`] z tego opisu przy starcie. Ta sama droga, którą modele
//! w M11a i M11c powstają z `mvoxc gen` — zapis gry jest wtedy tabelą liczb zamiast
//! katalogu plików, a łoże dzielnicy przestraja się, patrząc na liczby.
//!
//! Kolejność `beds` jest kontraktem: indeksuje ją `AmbientBed::as_index()` ze snapshotu.
//! Kolejność `sources` też, bo indeksuje ją [`SoundSourceId`].

use magnat_sim_snapshot::AMBIENT_BED_COUNT;
use serde::Deserialize;
use std::path::Path;

/// Wersja schematu pliku. Zmiana układu pól to zmiana tej liczby.
pub const AUDIO_SCHEMA_VERSION: u32 = 1;

/// Uchwyt do źródła punktowego — pozycja w `sources`.
///
/// Kolejność jest kontraktem katalogu, ale **nie zapisu gry**: żaden emiter nie przeżywa
/// publikacji snapshotu, więc numer źródła nigdzie się nie utrwala. To jest jedyna
/// tablica w `data/`, w której dopisywanie w środku jest nieszkodliwe.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct SoundSourceId(pub u16);

impl SoundSourceId {
    pub const MACHINE: SoundSourceId = SoundSourceId(0);
    pub const SHOP: SoundSourceId = SoundSourceId(1);
    pub const CAR: SoundSourceId = SoundSourceId(2);
    pub const TRUCK: SoundSourceId = SoundSourceId(3);
    pub const CRANE: SoundSourceId = SoundSourceId(4);
    pub const RAIN: SoundSourceId = SoundSourceId(5);
}

/// Klucze źródeł w kolejności stałych [`SoundSourceId`]. Walidator porównuje z nimi
/// katalog, bo indeks bez sprawdzonego klucza jest tylko liczbą.
pub const SOURCE_KEYS: [&str; 6] = ["machine", "shop", "car", "truck", "crane", "rain"];

/// Z czego składa się warstwa głosu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum LayerKind {
    /// Szum ograniczony pasmem `lo_hz`..`hi_hz` — wiatr, opony, gwar, deszcz.
    Noise,
    /// Sinus o częstotliwości `lo_hz` — silnik, syrena, ton siłowni.
    Tone,
    /// Impuls powtarzany `lo_hz` razy na sekundę, z dzwonieniem o `hi_hz` —
    /// uderzenie prasy, ptak, klakson.
    Pulse,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Layer {
    pub kind: LayerKind,
    pub lo_hz: f32,
    pub hi_hz: f32,
    pub gain: f32,
    /// Modulacja amplitudy w Hz; 0 = brak.
    #[serde(default)]
    pub am_hz: f32,
    /// Głębokość modulacji 0..1.
    #[serde(default)]
    pub am_depth: f32,
}

/// Jeden zapętlany głos: łoże, źródło punktowe albo stem muzyki.
#[derive(Clone, Debug, Deserialize)]
pub struct Voice {
    pub key: String,
    /// Długość pętli w sekundach. `0` znaczy „policz z taktu" i dotyczy wyłącznie
    /// stemów muzyki — reszta podaje ją wprost.
    pub loop_s: f32,
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Music {
    pub bpm: f32,
    pub beats_per_bar: u32,
    pub bars_per_loop: u32,
    /// Cztery stemy w kolejności `MusicMood`.
    pub stems: Vec<Voice>,
}

impl Music {
    /// Długość taktu w sekundach — jednostka, w której wolno przełączać stemy (§5.9).
    #[must_use]
    pub fn bar_s(&self) -> f32 {
        self.beats_per_bar as f32 * 60.0 / self.bpm
    }

    /// Długość pętli stemu. Musi być całkowitą liczbą taktów, inaczej crossfade
    /// „na granicy taktu" trafiałby za każdym razem w inne miejsce frazy.
    #[must_use]
    pub fn loop_s(&self) -> f32 {
        self.bar_s() * self.bars_per_loop as f32
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AudioCatalog {
    pub schema_version: u32,
    pub sample_rate: u32,
    pub beds: Vec<Voice>,
    pub sources: Vec<Voice>,
    pub music: Music,
}

#[derive(Debug)]
pub enum CatalogError {
    Io(std::io::Error),
    Parse(String),
    /// Katalog jest formalnie poprawny, ale nie spełnia kontraktu — na przykład ma
    /// mniej łóż niż wariantów `AmbientBed`. To jest błąd **ładowania**, a nie
    /// ostrzeżenie: łoże, którego nie ma, brzmi tak samo jak cisza.
    Contract(String),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::Io(e) => write!(f, "data/audio/audio.ron: {e}"),
            CatalogError::Parse(e) => write!(f, "data/audio/audio.ron: {e}"),
            CatalogError::Contract(e) => write!(f, "data/audio/audio.ron: {e}"),
        }
    }
}

impl std::error::Error for CatalogError {}

impl AudioCatalog {
    pub fn load(path: &Path) -> Result<AudioCatalog, CatalogError> {
        let tekst = std::fs::read_to_string(path).map_err(CatalogError::Io)?;
        let c: AudioCatalog =
            ron::from_str(&tekst).map_err(|e| CatalogError::Parse(e.to_string()))?;
        c.validate()?;
        Ok(c)
    }

    pub fn load_default() -> Result<AudioCatalog, CatalogError> {
        AudioCatalog::load(&magnat_core::data_path("audio/audio.ron"))
    }

    /// Kontrakt katalogu. Każdy z tych warunków, gdyby go nie sprawdzić, dawałby
    /// **ciszę zamiast błędu** — a cichy dźwięk i brakujący dźwięk wyglądają tak samo.
    fn validate(&self) -> Result<(), CatalogError> {
        let blad = |s: String| Err(CatalogError::Contract(s));
        if self.schema_version != AUDIO_SCHEMA_VERSION {
            return blad(format!(
                "schema_version {} zamiast {AUDIO_SCHEMA_VERSION}",
                self.schema_version
            ));
        }
        if self.beds.len() != AMBIENT_BED_COUNT {
            return blad(format!(
                "{} łóż zamiast {AMBIENT_BED_COUNT} — kolejność indeksuje AmbientBed",
                self.beds.len()
            ));
        }
        // Kolejność `sources` indeksują stałe `SoundSourceId`, więc sprawdzamy klucze,
        // a nie samą długość: przestawienie dwóch wierszy zamieniłoby cicho dźwig
        // na deszcz i nic by tego nie złapało (`I-28`).
        for (i, klucz) in SOURCE_KEYS.iter().enumerate() {
            match self.sources.get(i) {
                None => return blad(format!("brak źródła „{klucz}” na pozycji {i}")),
                Some(v) if v.key != *klucz => {
                    return blad(format!(
                        "pozycja {i} to „{}”, a stała SoundSourceId mówi „{klucz}”",
                        v.key
                    ))
                }
                Some(_) => {}
            }
        }
        if self.music.stems.len() != 4 {
            return blad(format!(
                "{} stemów zamiast czterech (MusicMood ma cztery warianty)",
                self.music.stems.len()
            ));
        }
        if self.music.bpm <= 0.0 || self.music.beats_per_bar == 0 || self.music.bars_per_loop == 0 {
            return blad("takt muzyki jest zerowy".into());
        }
        if self.sample_rate < 8_000 {
            return blad(format!("częstotliwość próbkowania {}", self.sample_rate));
        }
        for v in self.beds.iter().chain(&self.sources) {
            if v.loop_s <= 0.0 {
                return blad(format!("„{}” ma zerową pętlę", v.key));
            }
            if v.layers.is_empty() {
                return blad(format!("„{}” nie ma ani jednej warstwy", v.key));
            }
        }
        // Stemy mają `loop_s: 0`, bo długość pętli liczy się z taktu — ale warstw
        // potrzebują tak samo. Stem bez warstw wczytałby się i zagrał ciszę, czyli
        // dokładnie stan nie do odróżnienia od brakującego pliku.
        for v in &self.music.stems {
            if v.layers.is_empty() {
                return blad(format!("stem „{}” nie ma ani jednej warstwy", v.key));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Katalog z repozytorium musi się wczytać i spełniać kontrakt. To jest ten sam
    /// test, który pilnuje palet i modeli: dane są częścią gry, nie załącznikiem.
    #[test]
    fn katalog_z_repozytorium_sie_wczytuje() {
        let c = AudioCatalog::load_default().expect("data/audio/audio.ron");
        assert_eq!(c.beds.len(), AMBIENT_BED_COUNT);
        assert_eq!(c.music.stems.len(), 4);
        // Pętla stemu ma być całkowitą liczbą taktów — inaczej crossfade „na granicy
        // taktu" trafiałby za każdym razem w inne miejsce frazy.
        let takty = c.music.loop_s() / c.music.bar_s();
        assert!(
            (takty - takty.round()).abs() < 1e-4,
            "pętla to {takty} taktu"
        );
    }

    /// Stem bez warstw wczytywał się i grał ciszę — walidator ma to odrzucić.
    #[test]
    fn stem_bez_warstw_nie_przechodzi() {
        let mut c = AudioCatalog::load_default().expect("katalog");
        c.music.stems[2].layers.clear();
        assert!(c.validate().is_err(), "niemy stem przeszedł walidację");
    }

    /// Przestawienie kolejności źródeł zamieniłoby dźwig na deszcz — po cichu.
    #[test]
    fn przestawione_zrodlo_nie_przechodzi() {
        let mut c = AudioCatalog::load_default().expect("katalog");
        c.sources.swap(
            SoundSourceId::CRANE.0 as usize,
            SoundSourceId::RAIN.0 as usize,
        );
        assert!(
            c.validate().is_err(),
            "przestawione źródła przeszły walidację"
        );
    }

    /// Kolejność łóż w danych musi odpowiadać kolejności wariantów w snapshocie —
    /// rozjazd dałby park brzmiący fabryką i nic by tego nie złapało.
    #[test]
    fn klucze_loz_zgadzaja_sie_z_wariantami() {
        let c = AudioCatalog::load_default().expect("katalog");
        for b in magnat_sim_snapshot::AmbientBed::ALL {
            assert_eq!(
                c.beds[b.as_index()].key,
                b.key(),
                "łoże {b:?} stoi pod cudzym kluczem"
            );
        }
    }
}
