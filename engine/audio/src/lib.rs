//! `magnat-audio` — warstwa dźwiękowa gry (M11 §5.9, WP8).
//!
//! ### Co tu jest, a czego nie ma
//!
//! Miksera, magistral, limitera i przejść nie piszemy: robi je `kira` (decyzja 9.6
//! fazy M11, zamknięta 2026-09-19 wg propozycji domyślnej). Piszemy to, czego żadna
//! biblioteka audio nie zna, bo to jest wiedza o **tej** grze:
//!
//! - [`plan`] — co ma zabrzmieć: łoża dzielnic, emitery, klastrowanie, budżet puli,
//!   „linia stoi = cisza";
//! - [`music`] — kiedy wolno przełączyć nastrój;
//! - [`synth`] — z czego zrobić dźwięk, skoro `data/audio/` nie ma nagrań;
//! - [`catalog`] — jak to opisać w danych.
//!
//! ### Audio nie czyta ECS
//!
//! [`AudioEngine::update`] bierze `&RenderSnapshot` i nic więcej — ta sama zasada
//! co render i ten sam mechanizm egzekwowania (§6.3): w `Cargo.toml` tego crate'u
//! nie ma ani jednego `sim/*` poza `sim-snapshot`, a `cargo tree` sprawdza to w CI.
//!
//! ### Podział na plan i odtwarzanie
//!
//! Reguły, które niosą informację dla gracza, siedzą w [`plan`] jako funkcja czysta,
//! a `kira` dostaje gotową listę. Dzięki temu **komplet testów z §7.4 biegnie bez karty
//! dźwiękowej** — gdyby reguły były wywołaniami miksera, CI nie sprawdziłby ani jednej.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod music;
pub mod plan;
pub mod synth;

pub use catalog::{AudioCatalog, CatalogError, SoundSourceId};
pub use music::{MusicDirector, MusicMood, CROSSFADE_BARS};
pub use plan::{plan, Listener, MixPlan, Voice, MAX_WORLD_VOICES, VOICE_FADE_MS};

use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Easing, Tween};
use magnat_sim_snapshot::{RenderSnapshot, AMBIENT_BED_COUNT};
use std::collections::BTreeMap;
use std::time::Duration;

/// Magistrale miksera (§5.9). Kolejność jest kontraktem suwaków głośności w M9.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Bus {
    Ambient = 0,
    World = 1,
    Ui = 2,
    Music = 3,
}

pub const BUS_COUNT: usize = 4;

/// Głośność magistral 0..1, w kolejności [`Bus`].
#[derive(Clone, Copy, Debug)]
pub struct Volumes(pub [f32; BUS_COUNT]);

impl Default for Volumes {
    fn default() -> Self {
        Volumes([0.8, 0.9, 1.0, 0.5])
    }
}

/// Jak często odświeżamy okluzję. Cztery razy na sekundę, nie sześćdziesiąt (§5.9):
/// mur nie przesuwa się między klatkami, a raycast kosztuje.
pub const OCCLUSION_HZ: u32 = 4;

/// Jak długo trwa wyrównanie wzmocnienia głosu, który już gra.
const GAIN_TWEEN_MS: u64 = 80;

#[derive(Debug)]
pub enum AudioError {
    Catalog(CatalogError),
    /// Urządzenie odmówiło. **Nie jest to błąd krytyczny gry**: brak karty dźwiękowej
    /// ma dać cichą grę, a nie brak gry — dlatego [`AudioEngine::new`] zwraca `Result`,
    /// a klient go loguje i idzie dalej.
    Device(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::Catalog(e) => write!(f, "{e}"),
            AudioError::Device(e) => write!(f, "urządzenie dźwiękowe: {e}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// Grający głos: uchwyt `kira` plus to, czego uchwyt nie pamięta.
struct Playing {
    handle: StaticSoundHandle,
    gain: f32,
    pan: f32,
}

/// Silnik dźwięku: mikser, pula głosów, łoża i muzyka.
pub struct AudioEngine {
    /// Trzymany, choć nikt go nie czyta: `AudioManager` jest **właścicielem wątku
    /// miksera**, a jego upuszczenie zatrzymuje dźwięk. Pole bez czytelnika wygląda
    /// tu na zbędne i nie jest.
    _manager: AudioManager<DefaultBackend>,
    buses: [TrackHandle; BUS_COUNT],
    volumes: Volumes,
    /// Próbki źródeł punktowych — z nich powstaje każdy nowy głos światowy.
    /// Łoża i stemy nie mają tu pól, bo grają od startu do końca sesji i ich bufory
    /// żyją w uchwytach.
    sources: Vec<StaticSoundData>,
    /// Łoża grają **wszystkie naraz**, a mieszanka przestawia im wzmocnienie.
    /// Startowanie i zatrzymywanie ich przy każdej zmianie dzielnicy dawałoby
    /// słyszalne wejścia; siedem cichych pętli kosztuje mniej niż jedno takie wejście.
    bed_voices: [StaticSoundHandle; AMBIENT_BED_COUNT],
    /// Ostatnio zlecone wzmocnienie łoża. Bez tego przestawialibyśmy je w **każdej**
    /// klatce, a każde zlecenie zaczyna tween od nowa — łoże dochodziłoby do celu
    /// wykładniczo zamiast w zadanych 1,2 s i nigdy dokładnie.
    bed_gain: [f32; AMBIENT_BED_COUNT],
    /// Głosy światowe po kluczu z [`plan::Voice::key`] — po nim dopasowujemy je
    /// między klatkami, żeby pętla nie startowała od nowa.
    voices: BTreeMap<(u16, u32), Playing>,
    stem_voices: Vec<StaticSoundHandle>,
    music: MusicDirector,
    /// Takt muzyki w sekundach i czas od startu — zegar przejść.
    bar_s: f32,
    czas_s: f32,
    /// Kiedy ostatnio przeliczono okluzję.
    okluzja_s: f32,
    /// Zapamiętana okluzja po **komórce 4 m**, nie po emiterze: klaster głosów wędruje
    /// razem z pojazdami, więc klucz z jego tożsamości gubiłby wynik przy każdym
    /// przeliczeniu centroidu.
    okluzja: BTreeMap<[i32; 3], f32>,
    stats: AudioStats,
}

/// Liczby do raportu i do kryteriów §7.4.
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioStats {
    pub voices_active: usize,
    pub voices_dropped: usize,
    pub mood: u8,
    pub bar: u64,
}

impl AudioEngine {
    /// Buduje mikser, syntezuje katalog i uruchamia łoża.
    ///
    /// Synteza idzie **tutaj**, a nie przy pierwszym użyciu: siedem łóż po cztery
    /// sekundy to ok. 5 MB przy 22,05 kHz i kilkadziesiąt milisekund pracy. Zrobienie
    /// tego w trakcie gry dałoby zacięcie dokładnie w chwili, w której gracz wjeżdża
    /// do nowej dzielnicy.
    pub fn new(volumes: Volumes) -> Result<AudioEngine, AudioError> {
        let cat = AudioCatalog::load_default().map_err(AudioError::Catalog)?;
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| AudioError::Device(e.to_string()))?;

        // Cztery magistrale z limiterem. Limiter jest na każdej z osobna, a nie tylko
        // na sumie: przesterowana magistrala światowa zdusiłaby muzykę, a to jest
        // dokładnie to, czego gracz nie umie powiązać z przyczyną.
        // Żadnego `expect` poniżej: ten konstruktor zwraca `Result` po to, żeby awaria
        // miksera dała **cichą grę**, a nie przewróciła procesu (`I-21`). `array::from_fn`
        // nie umie oddać błędu, więc idziemy pętlą i składamy tablicę na końcu.
        let mut lista = Vec::with_capacity(BUS_COUNT);
        for g in volumes.0 {
            let mut b = TrackBuilder::new().volume(gain_db(g));
            b.add_effect(
                kira::effect::compressor::CompressorBuilder::new()
                    .threshold(-6.0)
                    .ratio(12.0)
                    .attack_duration(Duration::from_millis(3))
                    .release_duration(Duration::from_millis(120)),
            );
            lista.push(
                manager
                    .add_sub_track(b)
                    .map_err(|e| AudioError::Device(e.to_string()))?,
            );
        }
        let mut buses: [TrackHandle; BUS_COUNT] = lista
            .try_into()
            .map_err(|_| AudioError::Device("magistrale".into()))?;

        let sr = cat.sample_rate;
        let beds: [StaticSoundData; AMBIENT_BED_COUNT] = std::array::from_fn(|i| {
            let v = &cat.beds[i];
            sound(&synth::render_voice(v, sr, v.loop_s, i as u32 + 1), sr)
        });
        let sources: Vec<StaticSoundData> = cat
            .sources
            .iter()
            .enumerate()
            .map(|(i, v)| sound(&synth::render_voice(v, sr, v.loop_s, 100 + i as u32), sr))
            .collect();
        let petla_muzyki = cat.music.loop_s();
        let stems: Vec<StaticSoundData> = cat
            .music
            .stems
            .iter()
            .enumerate()
            .map(|(i, v)| {
                sound(
                    &synth::render_voice(v, sr, petla_muzyki, 200 + i as u32),
                    sr,
                )
            })
            .collect();

        // Łoża startują ciche i grają do końca sesji.
        let mut lista = Vec::with_capacity(AMBIENT_BED_COUNT);
        for b in &beds {
            lista.push(
                buses[Bus::Ambient as usize]
                    .play(b.volume(Decibels::SILENCE).loop_region(..))
                    .map_err(|e| AudioError::Device(e.to_string()))?,
            );
        }
        let bed_voices: [StaticSoundHandle; AMBIENT_BED_COUNT] = lista
            .try_into()
            .map_err(|_| AudioError::Device("łoża".into()))?;
        // Stemy muzyki tak samo: cztery pętle w tej samej fazie, crossfade przestawia
        // wyłącznie wzmocnienie. Tylko tak przejście trafia w granicę taktu.
        let mut stem_voices = Vec::with_capacity(stems.len());
        for st in &stems {
            stem_voices.push(
                buses[Bus::Music as usize]
                    .play(st.volume(Decibels::SILENCE).loop_region(..))
                    .map_err(|e| AudioError::Device(e.to_string()))?,
            );
        }

        let mut e = AudioEngine {
            _manager: manager,
            buses,
            volumes,
            sources,
            bed_voices,
            bed_gain: [0.0; AMBIENT_BED_COUNT],
            voices: BTreeMap::new(),
            stem_voices,
            music: MusicDirector::new(),
            bar_s: cat.music.bar_s(),
            czas_s: 0.0,
            okluzja_s: -1.0,
            okluzja: BTreeMap::new(),
            stats: AudioStats::default(),
        };
        // Nastrój startowy ma być słyszalny od razu, a nie po pierwszym przejściu.
        e.stem_voices[MusicMood::Stabilnosc.as_index()].set_volume(gain_db(1.0), natychmiast());
        Ok(e)
    }

    /// Głośność magistral — suwaki w ustawieniach gracza (M9).
    pub fn set_volumes(&mut self, v: Volumes) {
        self.volumes = v;
        for (i, b) in self.buses.iter_mut().enumerate() {
            b.set_volume(gain_db(v.0[i]), tween(GAIN_TWEEN_MS));
        }
    }

    #[must_use]
    pub const fn stats(&self) -> AudioStats {
        self.stats
    }

    #[must_use]
    pub const fn mood(&self) -> MusicMood {
        self.music.mood()
    }

    /// Klatka dźwięku: plan miksu, dopasowanie głosów, muzyka.
    ///
    /// `dt_s` to czas **realny**, nie czas gry: pętla dźwięku ma iść dalej przy pauzie,
    /// bo pauza zatrzymuje gospodarkę, a nie powietrze. Ta sama zasada, którą `czas_s`
    /// renderu stosuje do fal na wodzie.
    ///
    /// `occlusion` dostaje pozycję emitera i zwraca 0..1 — ile pochłania przeszkoda.
    /// Wołane najwyżej [`OCCLUSION_HZ`] razy na sekundę, bo mur się nie przesuwa.
    pub fn update(
        &mut self,
        snap: &RenderSnapshot,
        listener: &Listener,
        dt_s: f32,
        occlusion: &dyn Fn([f32; 3]) -> f32,
    ) {
        self.czas_s += dt_s;
        let swieza_okluzja = self.czas_s - self.okluzja_s >= 1.0 / OCCLUSION_HZ as f32;
        if swieza_okluzja {
            self.okluzja_s = self.czas_s;
            self.okluzja.clear();
        }
        // Zapamiętana okluzja: między odświeżeniami plan dostaje tę samą liczbę,
        // co poprzednio, a nie zero — inaczej dźwięk zza muru pulsowałby 4 razy
        // na sekundę.
        let zapamietana = std::cell::RefCell::new(std::mem::take(&mut self.okluzja));
        let plan = plan::plan(snap, listener, &|pos| {
            let klucz = komorka(pos);
            let mut m = zapamietana.borrow_mut();
            if let Some(v) = m.get(&klucz) {
                return *v;
            }
            let v = occlusion(pos);
            m.insert(klucz, v);
            v
        });
        self.okluzja = zapamietana.into_inner();

        self.apply_beds(&plan.beds);
        self.apply_voices(&plan);
        self.apply_music(snap);

        self.stats = AudioStats {
            voices_active: self.voices.len(),
            voices_dropped: plan.dropped,
            mood: self.music.mood() as u8,
            bar: self.bar(),
        };
    }

    /// Numer taktu od startu sesji.
    fn bar(&self) -> u64 {
        if self.bar_s <= 0.0 {
            return 0;
        }
        (self.czas_s / self.bar_s) as u64
    }

    fn apply_beds(&mut self, mix: &plan::BedMix) {
        for (i, w) in mix.iter().enumerate() {
            // Łoże przestawia się powoli — wejście do dzielnicy ma być przejściem,
            // a nie przełącznikiem. Zlecamy **tylko przy zmianie**, bo każde zlecenie
            // startuje tween od nowa.
            if (self.bed_gain[i] - *w).abs() < 0.01 {
                continue;
            }
            self.bed_gain[i] = *w;
            self.bed_voices[i].set_volume(gain_db(*w), tween(1_200));
        }
    }

    fn apply_voices(&mut self, p: &MixPlan) {
        // Zgaszenie tego, czego już nie ma na liście — **rampą**, nie ucięciem
        // (`no_voice_clicks`, §7.4).
        let chciane: std::collections::BTreeSet<(u16, u32)> =
            p.voices.iter().map(|v| (v.source.0, v.key)).collect();
        self.voices.retain(|k, v| {
            if chciane.contains(k) {
                return true;
            }
            v.handle.stop(tween(VOICE_FADE_MS));
            false
        });

        for v in &p.voices {
            let klucz = (v.source.0, v.key);
            let Some(dane) = self.sources.get(v.source.0 as usize) else {
                continue;
            };
            match self.voices.get_mut(&klucz) {
                Some(g) => {
                    // Zlecenie idzie **tylko przy zmianie**: każde startuje tween od nowa,
                    // więc wysyłanie ich co klatkę znaczyłoby, że żaden się nie kończy.
                    if (g.gain - v.gain).abs() > 0.01 {
                        g.handle.set_volume(gain_db(v.gain), tween(GAIN_TWEEN_MS));
                        g.gain = v.gain;
                    }
                    let pan = v.pan.clamp(-1.0, 1.0);
                    if (g.pan - pan).abs() > 0.02 {
                        g.handle.set_panning(pan, tween(GAIN_TWEEN_MS));
                        g.pan = pan;
                    }
                }
                None => {
                    // Nowy głos wchodzi **rampą**, z tego samego powodu, dla którego
                    // stary wychodzi rampą.
                    let d = dane
                        .volume(gain_db(v.gain))
                        .panning(v.pan.clamp(-1.0, 1.0))
                        .loop_region(..)
                        .fade_in_tween(tween(VOICE_FADE_MS));
                    if let Ok(h) = self.buses[Bus::World as usize].play(d) {
                        self.voices.insert(
                            klucz,
                            Playing {
                                handle: h,
                                gain: v.gain,
                                pan: v.pan.clamp(-1.0, 1.0),
                            },
                        );
                    }
                }
            }
        }
    }

    fn apply_music(&mut self, snap: &RenderSnapshot) {
        let takt = self.bar();
        let poprzedni = self.music.mood();
        if self
            .music
            .step(takt, snap.player.liquidity_ratio, snap.player.profit_trend)
        {
            let czas = (f64::from(self.bar_s) * f64::from(CROSSFADE_BARS)).max(0.05);
            let t = Tween {
                duration: Duration::from_secs_f64(czas),
                easing: Easing::Linear,
                ..Default::default()
            };
            self.stem_voices[poprzedni.as_index()].set_volume(Decibels::SILENCE, t);
            self.stem_voices[self.music.mood().as_index()].set_volume(gain_db(1.0), t);
        }
    }
}

/// Komórka 4 m, w której leży punkt — klucz zapamiętanej okluzji.
///
/// Cztery metry, bo tyle mniej więcej ma ściana: drobniejsza siatka liczyłaby raycast
/// dla każdego drgnięcia centroidu klastra, grubsza gubiłaby różnicę między „przed
/// murem" a „za murem".
fn komorka(pos: [f32; 3]) -> [i32; 3] {
    [
        (pos[0] / 4.0).floor() as i32,
        (pos[1] / 4.0).floor() as i32,
        (pos[2] / 4.0).floor() as i32,
    ]
}

/// Liniowe wzmocnienie 0..1 na decybele. Zero to cisza, a nie −∞ dB w arytmetyce.
fn gain_db(g: f32) -> Decibels {
    if g <= 0.0008 {
        return Decibels::SILENCE;
    }
    Decibels(20.0 * g.clamp(0.0, 1.0).log10())
}

fn tween(ms: u64) -> Tween {
    Tween {
        duration: Duration::from_millis(ms),
        easing: Easing::Linear,
        ..Default::default()
    }
}

fn natychmiast() -> Tween {
    Tween {
        duration: Duration::ZERO,
        ..Default::default()
    }
}

/// Bufor próbek na dane dźwiękowe `kira`.
fn sound(buf: &[synth::Sample], sample_rate: u32) -> StaticSoundData {
    let frames: std::sync::Arc<[kira::Frame]> = buf
        .iter()
        .map(|s| kira::Frame {
            left: s.left,
            right: s.right,
        })
        .collect();
    StaticSoundData {
        sample_rate,
        frames,
        settings: StaticSoundSettings::default(),
        slice: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_sim_snapshot::AmbientBed;

    /// Skala decybelowa: cisza to cisza, jedynka to zero decybeli, a połowa
    /// wzmocnienia to ok. −6 dB.
    #[test]
    fn skala_glosnosci_nie_ma_dziury_w_zerze() {
        assert_eq!(gain_db(0.0), Decibels::SILENCE);
        assert_eq!(gain_db(-1.0), Decibels::SILENCE);
        assert!((gain_db(1.0).0 - 0.0).abs() < 1e-4);
        assert!((gain_db(0.5).0 + 6.02).abs() < 0.05);
    }

    /// `audio_reads_only_snapshot` z §7.4 w postaci wykonalnej bez urządzenia:
    /// sygnatura wejścia to `&RenderSnapshot` i nic poza nim, a plan jest funkcją
    /// czystą. Mutację stanu świata wyklucza już typ — `&` nie ma jak zapisać.
    #[test]
    fn wejscie_audio_to_wylacznie_snapshot() {
        let snap = RenderSnapshot::default();
        let l = Listener::default();
        let a = plan::plan(&snap, &l, &|_| 0.0);
        let b = plan::plan(&snap, &l, &|_| 0.0);
        assert_eq!(a.voices, b.voices);
        assert_eq!(a.beds, b.beds);
    }

    /// `no_voice_clicks` z §7.4: wywłaszczenie głosu **zawsze** idzie rampą ≥ 120 ms.
    ///
    /// Sprawdzenie jest na źródle, a nie na dźwięku, i to jest właściwa forma: trzask
    /// słychać dopiero na karcie, której CI nie ma, a gwarancją jest to, że w całym
    /// crate'cie jest **jedno** miejsce zatrzymujące głos i woła ono [`VOICE_FADE_MS`].
    /// Ta sama droga, którą `tests/shaders.rs` pilnuje stałych w WGSL.
    #[test]
    fn wywlaszczenie_glosu_zawsze_ma_rampe() {
        const { assert!(VOICE_FADE_MS >= 120) };
        let zrodlo = include_str!("lib.rs");
        // Igła sklejona z dwóch kawałków, żeby ta linia nie wpadła we własne sito.
        let igla = concat!(".st", "op(");
        let zatrzymania: Vec<&str> = zrodlo
            .lines()
            .filter(|l| l.contains(igla))
            .map(str::trim)
            .collect();
        assert_eq!(
            zatrzymania.len(),
            1,
            "głos zatrzymuje się w {} miejscach: {zatrzymania:?}",
            zatrzymania.len()
        );
        assert!(
            zatrzymania[0].contains("VOICE_FADE_MS"),
            "zatrzymanie bez rampy: {}",
            zatrzymania[0]
        );
    }

    /// Wszystkie warianty łoża mają swój bufor. Brakujący indeks znaczyłby dzielnicę
    /// bez tła — czyli ciszę nie do odróżnienia od usterki.
    #[test]
    fn kazde_loze_ma_swoj_bufor() {
        let cat = AudioCatalog::load_default().expect("katalog");
        for b in AmbientBed::ALL {
            let v = &cat.beds[b.as_index()];
            let buf = synth::render_voice(v, cat.sample_rate, v.loop_s, 1);
            assert!(!buf.is_empty(), "{b:?} nie ma próbek");
        }
    }
}
