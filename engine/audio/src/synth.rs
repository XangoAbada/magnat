//! Synteza próbek z opisu w `data/audio/` (M11d, WP8).
//!
//! ### Dlaczego gra syntezuje dźwięk zamiast go wczytywać
//!
//! `data/audio/` nie ma ani jednego nagrania i to jest decyzja właściciela produktu
//! z 2026-09-19: łoża, maszyny i stemy powstają z opisu, tak samo jak modele voxelowe
//! powstają z `mvoxc gen`. Zysk jest dwojaki — zapis repozytorium jest tabelą liczb
//! zamiast kilkudziesięciu megabajtów WAV-ów, a przestrojenie łoża dzielnicy jest
//! zmianą w diffie, a nie w cudzym edytorze audio.
//!
//! Sufit jest jawny: to brzmi jak **maszyna**, nie jak nagranie. Ścieżka wyjścia
//! nie wymaga zmiany ani jednej linijki tutaj — katalog dostaje pole ze ścieżką pliku,
//! a `kira` umie go wczytać sam.
//!
//! ### Pętla musi być bezszwowa
//!
//! Głos gra w kółko, więc próbka po ostatniej ma być pierwszą. Dlatego każda
//! częstotliwość jest **zaokrąglana do całkowitej liczby okresów w pętli** — inaczej
//! zapętlenie tonu 98 Hz w pętli 4 s daje trzask 0,25 raza na sekundę i nikt nie wie,
//! skąd on jest.

use crate::catalog::{Layer, LayerKind, Voice};

/// Jedna próbka stereo — ten sam układ co `kira::Frame`, ale bez zależności od niego
/// w kodzie, który da się przetestować bez urządzenia.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub left: f32,
    pub right: f32,
}

/// Generator liczb pseudolosowych do szumu.
///
/// Własny, a nie z `engine/core`: `rng` tam jest funkcją `(seed, strumień, encja, tick)`
/// i służy determinizmowi **symulacji**, w którym numer strumienia jest wieczny
/// (`K-4`). Szum w buforze audio nie wchodzi do hasha, nie ma encji i nie ma ticku —
/// zajmowanie dla niego numeru strumienia zapisałoby na zawsze coś, co jest szumem.
struct Noise(u32);

impl Noise {
    /// Zero jest **punktem stałym** xorshift32, więc ziarno zerowe dawałoby stałą
    /// wartość zamiast szumu — czyli ciszę po przejściu przez filtr pasmowy, bez
    /// błędu i bez śladu. Podstawienie jedynki kosztuje jedno porównanie.
    fn new(seed: u32) -> Noise {
        Noise(if seed == 0 { 1 } else { seed })
    }

    fn next(&mut self) -> f32 {
        // xorshift32 — wystarcza w zupełności, bo słuchamy szumu, a nie mierzymy go.
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Filtr pasmowy: dolno- i górnoprzepustowy, każdy **dwubiegunowy**.
///
/// Pasmo jest całą różnicą między szumem opon (180–3200 Hz) a rumorem hali
/// (40–260 Hz), więc zbocze musi być stromsze niż 6 dB na oktawę: przy jednym
/// biegunie rumor hali wynosi tyle samo energii powyżej 3 kHz co szelest liści
/// i łoża przestają się różnić słuchem. Dwa bieguny w kaskadzie kosztują cztery
/// mnożenia i dają 12 dB na oktawę — tyle wystarcza.
struct Band {
    lp: [f32; 2],
    hp: [f32; 2],
    a_lo: f32,
    a_hi: f32,
}

impl Band {
    fn new(lo_hz: f32, hi_hz: f32, sample_rate: u32) -> Band {
        let sr = sample_rate as f32;
        Band {
            lp: [0.0; 2],
            hp: [0.0; 2],
            a_lo: wspolczynnik(lo_hz, sr),
            a_hi: wspolczynnik(hi_hz, sr),
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        self.lp[0] += self.a_hi * (x - self.lp[0]);
        self.lp[1] += self.a_hi * (self.lp[0] - self.lp[1]);
        self.hp[0] += self.a_lo * (self.lp[1] - self.hp[0]);
        self.hp[1] += self.a_lo * (self.hp[0] - self.hp[1]);
        // Dwubiegunowy filtr tłumi także w paśmie przepustowym; wzmocnienie
        // kompensacyjne trzyma poziom tam, gdzie go ustawił opis warstwy.
        (self.lp[1] - self.hp[1]) * 2.2
    }
}

/// Współczynnik filtru jednobiegunowego dla zadanej częstotliwości granicznej.
fn wspolczynnik(hz: f32, sample_rate: f32) -> f32 {
    let x = std::f32::consts::TAU * hz / sample_rate;
    (x / (x + 1.0)).clamp(0.0, 1.0)
}

/// Częstotliwość zaokrąglona tak, żeby w pętli mieściła się całkowita liczba okresów.
/// Bez tego zapętlenie trzaska na szwie.
fn do_petli(hz: f32, loop_s: f32) -> f32 {
    if hz <= 0.0 || loop_s <= 0.0 {
        return 0.0;
    }
    (hz * loop_s).round().max(1.0) / loop_s
}

/// Syntezuje zapętlany bufor stereo dla jednego głosu.
///
/// `seed` rozsuwa szum między głosami — dwa łoża z tym samym pasmem brzmiałyby
/// identycznie, a mają brzmieć podobnie.
#[must_use]
pub fn render_voice(v: &Voice, sample_rate: u32, loop_s: f32, seed: u32) -> Vec<Sample> {
    let n = ((sample_rate as f32) * loop_s).round().max(1.0) as usize;
    let mut out = vec![Sample::default(); n];
    for (i, l) in v.layers.iter().enumerate() {
        // Ziarno per warstwa i per głos, żeby ten sam opis w dwóch głosach nie dał
        // bit w bit tego samego szumu.
        let s = seed.wrapping_mul(0x9E37_79B9).wrapping_add(i as u32 + 1);
        warstwa(&mut out, l, sample_rate, loop_s, s);
    }
    normalizuj(&mut out);
    out
}

fn warstwa(out: &mut [Sample], l: &Layer, sample_rate: u32, loop_s: f32, seed: u32) {
    let sr = sample_rate as f32;
    let am_hz = do_petli(l.am_hz, loop_s);
    let mut rng = Noise::new(seed);
    // Szum i pasmo są wspólne dla obu kanałów, ale każdy dostaje własny filtr —
    // to wystarcza, żeby łoże miało szerokość, a nie stało w środku głowy.
    let mut band_l = Band::new(l.lo_hz.max(1.0), l.hi_hz.max(l.lo_hz + 1.0), sample_rate);
    let mut band_r = Band::new(l.lo_hz.max(1.0), l.hi_hz.max(l.lo_hz + 1.0), sample_rate);
    let mut rng_r = Noise::new(seed ^ 0x5BF0_3635);
    let ton_hz = do_petli(l.lo_hz, loop_s);
    let puls_hz = do_petli(l.lo_hz, loop_s);
    let dzwon_hz = do_petli(l.hi_hz, loop_s);

    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let am = if am_hz > 0.0 {
            1.0 - l.am_depth * 0.5 * (1.0 - (std::f32::consts::TAU * am_hz * t).sin())
        } else {
            1.0
        };
        let g = l.gain * am;
        let (a, b) = match l.kind {
            LayerKind::Noise => (
                band_l.process(rng.next()) * g,
                band_r.process(rng_r.next()) * g,
            ),
            LayerKind::Tone => {
                let v = (std::f32::consts::TAU * ton_hz * t).sin() * g;
                (v, v)
            }
            LayerKind::Pulse => {
                // Faza w obrębie jednego uderzenia; obwiednia wykładnicza, więc impuls
                // ma atak i wybrzmienie zamiast trzasku.
                let faza = (t * puls_hz).fract();
                let wiek = faza / puls_hz;
                let obw = (-wiek * 9.0).exp();
                let v = (std::f32::consts::TAU * dzwon_hz * wiek).sin() * obw * g;
                (v, v)
            }
        };
        s.left += a;
        s.right += b;
    }
}

/// Przycina szczyt do 0,9 — limiter jest w mikserze, ale bufor, który wchodzi w klip
/// już na wejściu, nie ma jak zostać uratowany.
fn normalizuj(buf: &mut [Sample]) {
    let szczyt = buf
        .iter()
        .fold(0.0f32, |m, s| m.max(s.left.abs()).max(s.right.abs()));
    if szczyt <= 0.9 {
        return;
    }
    let k = 0.9 / szczyt;
    for s in buf {
        s.left *= k;
        s.right *= k;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AudioCatalog;

    fn moc(buf: &[Sample]) -> f32 {
        buf.iter().map(|s| s.left * s.left).sum::<f32>() / buf.len() as f32
    }

    /// Pętla musi się domykać: ostatnia próbka blisko pierwszej, inaczej na szwie
    /// jest trzask słyszalny raz na długość pętli.
    ///
    /// Sprawdzamy **stemy muzyki**, a nie łoża, i to jest cała treść tego testu:
    /// stemy są czystymi tonami, więc domknięcie jest sprawdzalne co do ułamka,
    /// a szum z definicji nie domyka się nigdzie. Trzask na szwie muzyki słychać
    /// przy tym najmocniej, bo pętla wraca co cztery takty i zawsze w tym samym
    /// miejscu frazy. Pierwsza wersja tego testu omijała każdy głos z warstwą szumu
    /// — czyli **wszystkie siedem łóż** — i nie dochodziła do ani jednej asercji.
    #[test]
    fn petla_nie_trzeszczy_na_szwie() {
        let c = AudioCatalog::load_default().expect("katalog");
        let petla = c.music.loop_s();
        let mut sprawdzonych = 0;
        for v in &c.music.stems {
            assert!(
                v.layers.iter().all(|l| l.kind != LayerKind::Noise),
                "„{}” ma szum — ten test go nie zmierzy",
                v.key
            );
            let buf = render_voice(v, 22_050, petla, 1);
            let szew = (buf[buf.len() - 1].left - buf[0].left).abs();
            assert!(szew < 0.05, "„{}” szew {szew}", v.key);
            sprawdzonych += 1;
        }
        assert_eq!(sprawdzonych, 4, "test nie doszedł do asercji");
    }

    /// Trzy dzielnice mają brzmieć **rozpoznawalnie różnie** (kryterium WP8). Różnicę
    /// mierzymy tam, gdzie ją słychać: w rozkładzie energii na pasma.
    #[test]
    fn loza_dzielnic_roznia_sie_pasmem() {
        let c = AudioCatalog::load_default().expect("katalog");
        let sr = 22_050;
        let energia = |klucz: &str, lo: f32, hi: f32| -> f32 {
            let v = c.beds.iter().find(|b| b.key == klucz).expect(klucz);
            let buf = render_voice(v, sr, v.loop_s, 7);
            let mut f = Band::new(lo, hi, sr);
            let s: f32 = buf.iter().map(|s| f.process(s.left).powi(2)).sum();
            s / buf.len() as f32
        };
        // Przemysł ma nieść energię nisko, park wysoko. To nie jest kosmetyka —
        // to jest cała treść „rozpoznawalnie różnych łóż".
        let przemysl_nisko = energia("industry", 20.0, 250.0);
        let park_nisko = energia("park", 20.0, 250.0);
        let przemysl_wysoko = energia("industry", 3000.0, 10000.0);
        let park_wysoko = energia("park", 3000.0, 10000.0);
        assert!(
            przemysl_nisko > park_nisko * 4.0,
            "przemysł nisko {przemysl_nisko}, park nisko {park_nisko}"
        );
        assert!(
            park_wysoko > przemysl_wysoko * 4.0,
            "park wysoko {park_wysoko}, przemysł wysoko {przemysl_wysoko}"
        );
        // Śródmieście (ruch) leży pośrodku i ma być głośniejsze od osiedla.
        let c1 = c.beds.iter().find(|b| b.key == "traffic").expect("traffic");
        let c2 = c
            .beds
            .iter()
            .find(|b| b.key == "residential")
            .expect("residential");
        assert!(
            moc(&render_voice(c1, sr, c1.loop_s, 7)) > moc(&render_voice(c2, sr, c2.loop_s, 7))
        );
    }

    /// Bufor nie może wchodzić w klip: limiter miksera ratuje sumę, nie pojedynczy głos.
    #[test]
    fn zaden_glos_nie_przesterowuje() {
        let c = AudioCatalog::load_default().expect("katalog");
        for v in c.beds.iter().chain(&c.sources) {
            let buf = render_voice(v, 22_050, v.loop_s, 3);
            let szczyt = buf
                .iter()
                .fold(0.0f32, |m, s| m.max(s.left.abs()).max(s.right.abs()));
            assert!(szczyt <= 0.9001, "„{}” szczyt {szczyt}", v.key);
            assert!(szczyt > 0.0, "„{}” jest ciszą", v.key);
        }
    }

    /// Ten sam opis i to samo ziarno dają ten sam bufor — inaczej dwa uruchomienia
    /// gry brzmiałyby inaczej i nie dałoby się tego zgłosić jako usterki.
    #[test]
    fn synteza_jest_powtarzalna() {
        let c = AudioCatalog::load_default().expect("katalog");
        let v = &c.beds[1];
        assert_eq!(
            render_voice(v, 22_050, v.loop_s, 11),
            render_voice(v, 22_050, v.loop_s, 11)
        );
        assert_ne!(
            render_voice(v, 22_050, v.loop_s, 11),
            render_voice(v, 22_050, v.loop_s, 12)
        );
    }
}
