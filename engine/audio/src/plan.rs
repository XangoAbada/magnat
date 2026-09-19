//! Plan miksu: co ma zabrzmieć w tej klatce i jak głośno (M11d §5.9, WP8).
//!
//! ### Dlaczego to jest osobno od urządzenia
//!
//! Wszystko, co decyduje o **tym, co słychać** — klastrowanie głosów, priorytety,
//! budżet puli, „linia stoi = cisza", mieszanka łóż dzielnic — jest tu funkcją czystą
//! od snapshotu i pozycji słuchacza. `kira` dostaje gotową listę i ją odtwarza.
//!
//! Zysk jest konkretny, a nie porządkowy: **komplet testów z §7.4 biegnie bez karty
//! dźwiękowej**. Gdyby reguły siedziały w wywołaniach miksera, CI bez urządzenia
//! nie sprawdziłby ani jednej z nich, a to są reguły, które niosą informację dla gracza
//! (§15.5: zatrzymanie linii ma być **słychać**).

use magnat_sim_snapshot::{AmbientBed, RenderSnapshot, AMBIENT_BED_COUNT, VEHICLE_FLAG_ENGINE_ON};

use crate::catalog::SoundSourceId;

/// Ile głosów światowych mieści pula (§5.9). Podniesienie to zmiana jednej stałej —
/// ryzyko `R5` fazy zakłada, że może okazać się potrzebne po odsłuchu.
pub const MAX_WORLD_VOICES: usize = 32;

/// Promień klastrowania emiterów o tym samym źródle (§5.9).
pub const CLUSTER_RADIUS_M: f32 = 15.0;

/// Sufit wzmocnienia klastra: 20 ciężarówek nie jest 20× głośniejsze niż jedna.
pub const CLUSTER_GAIN_CAP: f32 = 2.5;

/// Najkrótsza rampa wygaszenia wywłaszczanego głosu (§5.9: „trzask jest gorszy
/// niż brak dźwięku").
pub const VOICE_FADE_MS: u64 = 120;

/// Dalej niż tyle metrów emiter nie wchodzi do puli. Poza tym progiem jego udział
/// jest poniżej progu słyszalności, a zajmowałby głos komuś bliżej.
pub const EMITTER_RANGE_M: f32 = 220.0;

/// Słuchacz — pozycja i kurs. W trybie pierwszoosobowym to głowa postaci,
/// w orbitalnym — kamera.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Listener {
    /// Pozycja w metrach świata.
    pub pos: [f32; 3],
    /// Kurs w radianach, 0 = oś +X — ten sam układ co `PedestrianRecord::heading`.
    pub yaw: f32,
}

/// Jeden głos do zagrania w tej klatce.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Voice {
    pub source: SoundSourceId,
    /// Wzmocnienie 0..1 po odległości, okluzji i klastrowaniu.
    pub gain: f32,
    /// −1 = lewo, 0 = środek, 1 = prawo.
    pub pan: f32,
    /// Pozycja wirtualnego emitera w metrach świata — do diagnostyki i do testów.
    pub pos: [f32; 3],
    /// Ile emiterów zlało się w ten głos.
    pub sources: u16,
    /// Stabilna tożsamość głosu między klatkami: `entity_lo` **najniższego** emitera
    /// w klastrze. Bez niej głos startowałby od nowa w każdej klatce, czyli nie byłby
    /// pętlą, tylko serią trzasków.
    pub key: u32,
}

/// Udział łoża w mieszance tła, 0..1. Indeks to `AmbientBed::as_index()`.
pub type BedMix = [f32; AMBIENT_BED_COUNT];

/// Wynik planowania jednej klatki.
#[derive(Clone, Debug, Default)]
pub struct MixPlan {
    pub beds: BedMix,
    pub voices: Vec<Voice>,
    /// Ile emiterów odpadło na progu puli — do `RenderStats` i do diagnostyki
    /// kryterium `voice_budget`.
    pub dropped: usize,
}

/// Emiter przed klastrowaniem.
#[derive(Clone, Copy, Debug)]
struct Emitter {
    source: SoundSourceId,
    pos: [f32; 3],
    gain: f32,
    key: u32,
}

/// Składa plan miksu ze snapshotu. **Funkcja czysta** — to jest cały kontrakt tego
/// modułu i to on pozwala sprawdzić §7.4 bez urządzenia.
///
/// `occlusion` dostaje pozycję emitera i zwraca 0..1: ile dźwięku pochłania przeszkoda.
/// Domknięcie, a nie trait, bo implementacja jest **jedna** (raycast po stronie klienta,
/// który widzi teren i miasto), a testowa jest stałą — trait z jedną implementacją
/// produkcyjną byłby abstrakcją bez drugiego konsumenta.
#[must_use]
pub fn plan(
    snap: &RenderSnapshot,
    listener: &Listener,
    occlusion: &dyn Fn([f32; 3]) -> f32,
) -> MixPlan {
    let mut out = MixPlan {
        beds: bed_mix(snap, listener),
        ..Default::default()
    };
    let mut emitery = zbierz_emitery(snap, listener);
    // Deszcz nie ma emitera w świecie: pada wszędzie, więc jest głosem bez pozycji,
    // wprost nad słuchaczem. Inaczej ulewa brzmiałaby z jednego punktu.
    if snap.weather.precipitation > 0 {
        emitery.push(Emitter {
            source: SoundSourceId::RAIN,
            pos: listener.pos,
            gain: f32::from(snap.weather.precipitation) / 255.0,
            key: u32::MAX,
        });
    }
    let klastry = klastruj(&mut emitery);
    out.voices = wybierz_glosy(klastry, listener, occlusion, &mut out.dropped);
    out
}

/// Mieszanka łóż tła.
///
/// §5.9 zapisywało `AmbientZone` z wagą „z odległości kamery do centroidu dzielnicy",
/// a centroidu dzielnicy w snapshocie **nie ma i nie będzie**: kanał sim → render niesie
/// encje i tablice per dzielnica, nie geometrię miasta (`I-8`). Wagę liczymy więc z tego,
/// co dzielnicę tworzy — z tego, co stoi i jeździ wokół słuchacza.
///
/// **Mieszkaniec nie wchodzi do mieszanki i to jest rozstrzygnięcie, nie przeoczenie**
/// (`I-19`): `CitizenRenderRec.district` niesie dzielnicę **zameldowania**, a nie tę,
/// w której człowiek akurat stoi. Ważenie nią odległości znaczyłoby, że pas przemysłowy
/// pełen dojeżdżających brzmi osiedlem — czyli dokładnie odwrotnie niż wymaga kryterium
/// WP8. Zostają dwa źródła, w których i pozycja, i rodzaj są prawdziwe:
///
/// - **zakład** niesie `ambient_kind` policzony z sektora archetypu i stoi tam, gdzie stoi;
/// - **pojazd** liczy się zawsze jako [`AmbientBed::Traffic`], bo jadące auto brzmi
///   ruchem niezależnie od tego, czyj jest i gdzie mieszka jego właściciel.
fn bed_mix(snap: &RenderSnapshot, listener: &Listener) -> BedMix {
    let mut wagi = [0.0f32; AMBIENT_BED_COUNT];
    let mut dodaj = |bed: AmbientBed, pos: [f32; 3], sila: f32| {
        wagi[bed.as_index()] += sila / (1.0 + dist2(pos, listener.pos) / 2_500.0);
    };
    for s in snap.sites.as_slice() {
        dodaj(AmbientBed::from_index(s.ambient_kind), mm(s.pos), 1.0);
    }
    for v in snap.vehicles.as_slice() {
        // Pojazd niesie tło ruchu mocniej niż stojący budynek — to on jest słyszalny
        // z ulicy i to on znika, kiedy miasto zasypia.
        dodaj(AmbientBed::Traffic, mm(v.pos), 2.0);
    }

    let suma: f32 = wagi.iter().sum();
    if suma <= 0.0 {
        // Pustkowie: cisza z drobnym szumem. Zero we wszystkich łożach znaczyłoby
        // brak dźwięku, a to jest inny stan niż „tu nic nie ma".
        let mut m = [0.0; AMBIENT_BED_COUNT];
        m[AmbientBed::Quiet.as_index()] = 1.0;
        return m;
    }
    for w in &mut wagi {
        *w /= suma;
    }
    wagi
}

/// Emitery punktowe tej klatki: zakłady i pojazdy.
fn zbierz_emitery(snap: &RenderSnapshot, listener: &Listener) -> Vec<Emitter> {
    let mut v = Vec::new();
    let zasieg2 = EMITTER_RANGE_M * EMITTER_RANGE_M;
    for s in snap.sites.as_slice() {
        // **„Linia stoi = cisza" (§15.5)** — i to jest cała implementacja tej obietnicy.
        // Mnożenie, a nie próg: zakład na ćwierć obłożenia ma być cichszy, a nie cichy.
        let gain = f32::from(s.activity) / 255.0;
        if gain <= 0.0 {
            continue;
        }
        let pos = mm(s.pos);
        if dist2(pos, listener.pos) > zasieg2 {
            continue;
        }
        let bed = AmbientBed::from_index(s.ambient_kind);
        v.push(Emitter {
            source: source_of_bed(bed),
            pos,
            gain,
            key: s.entity_lo,
        });
    }
    for veh in snap.vehicles.as_slice() {
        if veh.flags & VEHICLE_FLAG_ENGINE_ON == 0 {
            continue;
        }
        let pos = mm(veh.pos);
        if dist2(pos, listener.pos) > zasieg2 {
            continue;
        }
        // Ładunek rozstrzyga, czy to auto, czy ciężarówka: `model` jest indeksem
        // katalogu pojazdów, którego `engine/audio` nie widzi i widzieć nie ma.
        let ciezki = veh.load > 0 || veh.occupants > 4;
        v.push(Emitter {
            source: if ciezki {
                SoundSourceId::TRUCK
            } else {
                SoundSourceId::CAR
            },
            pos,
            gain: 0.7,
            key: veh.entity_lo,
        });
    }
    v
}

/// Które źródło punktowe odpowiada łożu zakładu.
const fn source_of_bed(b: AmbientBed) -> SoundSourceId {
    match b {
        AmbientBed::Industry => SoundSourceId::MACHINE,
        AmbientBed::Port => SoundSourceId::CRANE,
        _ => SoundSourceId::SHOP,
    }
}

/// Klastrowanie: emitery o tym samym źródle w promieniu [`CLUSTER_RADIUS_M`] łączą się
/// w jeden wirtualny emiter w centroidzie (§5.9).
///
/// **Bez tego nie ma żywego miasta**: 500 pojazdów na skrzyżowaniu to 500 głosów,
/// a pula ma 32. Wzmocnienie sumuje się z sufitem `single × 2,5`, bo dwadzieścia
/// ciężarówek nie jest dwadzieścia razy głośniejsze niż jedna — ucho liczy w decybelach,
/// a nie w sztukach.
///
/// ponytail: dopasowanie idzie **liniowo po dotychczasowych klastrach**, czyli O(n·k).
/// Sufit jest zmierzony i odległy: promień [`EMITTER_RANGE_M`] przepuszcza rzędu kilkuset
/// emiterów, a klastrów po zebraniu jest kilkadziesiąt. Ścieżka wyjścia, gdyby to kiedyś
/// zaczęło kosztować: siatka mieszająca o oczku [`CLUSTER_RADIUS_M`] — ta sama, którą
/// `engine/spatial` ma dla encji.
fn klastruj(emitery: &mut [Emitter]) -> Vec<Voice> {
    // Porządek po (źródło, klucz) czyni wynik niezależnym od kolejności, w jakiej
    // encje wpadły do snapshotu — a ta zależy od kamery.
    emitery.sort_unstable_by(|a, b| a.source.cmp(&b.source).then(a.key.cmp(&b.key)));
    let r2 = CLUSTER_RADIUS_M * CLUSTER_RADIUS_M;
    let mut out: Vec<Voice> = Vec::new();
    for e in emitery.iter() {
        let dopasowany = out
            .iter_mut()
            .find(|v| v.source == e.source && dist2(v.pos, e.pos) <= r2);
        match dopasowany {
            Some(v) => {
                // Centroid ważony liczbą emiterów, nie wzmocnieniem: klaster ma stać
                // tam, gdzie stoją źródła, a nie tam, gdzie jest najgłośniejsze.
                let n = f32::from(v.sources);
                for i in 0..3 {
                    v.pos[i] = (v.pos[i] * n + e.pos[i]) / (n + 1.0);
                }
                v.sources += 1;
                v.gain += e.gain;
                v.key = v.key.min(e.key);
            }
            None => out.push(Voice {
                source: e.source,
                gain: e.gain,
                pan: 0.0,
                pos: e.pos,
                sources: 1,
                key: e.key,
            }),
        }
    }
    // Sufit wzmocnienia liczy się **po** zebraniu klastra, bo dopiero wtedy wiadomo,
    // ile emiterów się na niego złożyło.
    for v in &mut out {
        let pojedynczy = v.gain / f32::from(v.sources);
        v.gain = v.gain.min(pojedynczy * CLUSTER_GAIN_CAP);
    }
    out
}

/// Atenuacja, okluzja, panorama, priorytet i budżet puli.
///
/// Priorytet jest z §5.9: `gain × (1 − occlusion) / (1 + dist²)`. Przy wyczerpaniu puli
/// odpadają głosy o najniższym priorytecie — **wygaszane rampą**, o czym decyduje
/// już strona odtwarzająca; tutaj po prostu nie ma ich na liście.
fn wybierz_glosy(
    mut glosy: Vec<Voice>,
    listener: &Listener,
    occlusion: &dyn Fn([f32; 3]) -> f32,
    dropped: &mut usize,
) -> Vec<Voice> {
    for v in &mut glosy {
        let d2 = dist2(v.pos, listener.pos);
        let okluzja = occlusion(v.pos).clamp(0.0, 1.0);
        // Atenuacja odwrotnie proporcjonalna do kwadratu, z metrem odniesienia —
        // bez niego emiter w punkcie słuchacza miałby wzmocnienie nieskończone.
        let atenuacja = 1.0 / (1.0 + d2 / 100.0);
        v.gain = (v.gain * atenuacja * (1.0 - okluzja)).clamp(0.0, 1.0);
        v.pan = panorama(v.pos, listener);
    }
    glosy.retain(|v| v.gain > 0.001);
    if glosy.len() > MAX_WORLD_VOICES {
        // Malejąco po wzmocnieniu; remis po kluczu, żeby lista nie migotała.
        glosy.sort_unstable_by(|a, b| b.gain.total_cmp(&a.gain).then(a.key.cmp(&b.key)));
        *dropped = glosy.len() - MAX_WORLD_VOICES;
        glosy.truncate(MAX_WORLD_VOICES);
    }
    // Porządek wynikowy po kluczu: odtwarzacz dopasowuje głosy między klatkami po nim,
    // więc stabilna kolejność oszczędza mu wyszukiwania.
    glosy.sort_unstable_by_key(|v| (v.source, v.key));
    glosy
}

/// Panorama: rzut kierunku do emitera na oś „w prawo" słuchacza.
fn panorama(pos: [f32; 3], l: &Listener) -> f32 {
    let (dx, dy) = (pos[0] - l.pos[0], pos[1] - l.pos[1]);
    let dl = (dx * dx + dy * dy).sqrt();
    if dl < 0.5 {
        return 0.0;
    }
    // Kurs 0 patrzy w +X, więc „w prawo" to −Y.
    let (s, c) = (l.yaw.sin(), l.yaw.cos());
    ((dx * s - dy * c) / dl).clamp(-1.0, 1.0)
}

fn mm(p: [i32; 3]) -> [f32; 3] {
    [
        p[0] as f32 * 0.001,
        p[1] as f32 * 0.001,
        p[2] as f32 * 0.001,
    ]
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_sim_snapshot::{SiteRenderRec, VehicleRenderRec};

    fn cisza(_: [f32; 3]) -> f32 {
        0.0
    }

    fn zaklad(x: i32, activity: u8, bed: AmbientBed, id: u32) -> SiteRenderRec {
        SiteRenderRec {
            entity_lo: id,
            pos: [x, 0, 0],
            activity,
            ambient_kind: bed.as_index() as u8,
            ..Default::default()
        }
    }

    /// `stopped_line_is_silent` z §7.4: `activity == 0` → sumaryczne wzmocnienie
    /// emiterów zakładu **dokładnie zero**.
    #[test]
    fn zatrzymana_linia_jest_cicha() {
        let mut snap = RenderSnapshot::default();
        snap.sites.push(zaklad(5_000, 200, AmbientBed::Industry, 1));
        let l = Listener::default();
        let p = plan(&snap, &l, &cisza);
        let glosno: f32 = p.voices.iter().map(|v| v.gain).sum();
        assert!(glosno > 0.0, "pracujący zakład milczy");

        snap.sites.clear();
        snap.sites.push(zaklad(5_000, 0, AmbientBed::Industry, 1));
        let p = plan(&snap, &l, &cisza);
        let glosno: f32 = p.voices.iter().map(|v| v.gain).sum();
        assert_eq!(glosno, 0.0, "zatrzymana linia gra");
    }

    /// `voice_clustering` z §7.4: 500 emiterów tego samego źródła w promieniu 15 m
    /// daje **jeden** głos.
    #[test]
    fn piecset_ciezarowek_to_jeden_glos() {
        let mut snap = RenderSnapshot::default();
        for i in 0..500u32 {
            snap.vehicles.push(VehicleRenderRec {
                entity_lo: i,
                // Rozrzut w obrębie 10 m — wszystkie mieszczą się w promieniu klastra.
                pos: [20_000 + (i as i32 % 10) * 1_000, 0, 0],
                flags: VEHICLE_FLAG_ENGINE_ON,
                load: 200,
                ..Default::default()
            });
        }
        let p = plan(&snap, &Listener::default(), &cisza);
        assert_eq!(p.voices.len(), 1, "{} głosów", p.voices.len());
        assert_eq!(p.voices[0].sources, 500);
        // Sufit wzmocnienia: 500 ciężarówek nie jest 500× głośniejsze niż jedna.
        let jedna = 0.7 * 1.0 / (1.0 + 400.0 / 100.0);
        assert!(p.voices[0].gain <= jedna * CLUSTER_GAIN_CAP * 1.01);
    }

    /// `voice_budget` z §7.4: w żadnej klatce więcej niż [`MAX_WORLD_VOICES`] głosów.
    #[test]
    fn pula_glosow_nie_przepelnia_sie() {
        let mut snap = RenderSnapshot::default();
        // Sto zakładów rozstawionych co 20 m — poza promieniem klastrowania, więc
        // każdy chce własny głos.
        for i in 0..100u32 {
            snap.sites
                .push(zaklad(i as i32 * 20_000, 255, AmbientBed::Industry, i));
        }
        let p = plan(&snap, &Listener::default(), &cisza);
        assert!(
            p.voices.len() <= MAX_WORLD_VOICES,
            "{} głosów",
            p.voices.len()
        );
        // Zostają **najbliższe**, bo priorytet jest malejący z odległością.
        let najdalszy = p
            .voices
            .iter()
            .map(|v| v.pos[0] as i32)
            .max()
            .expect("pusta pula");
        assert!(najdalszy < 100 * 20, "w puli został głos z {najdalszy} m");
    }

    /// Okluzja wycisza, a pełna okluzja wycisza do zera — inaczej mur nie robi różnicy.
    #[test]
    fn okluzja_wycisza() {
        let mut snap = RenderSnapshot::default();
        snap.sites.push(zaklad(3_000, 255, AmbientBed::Industry, 1));
        let l = Listener::default();
        let otwarte = plan(&snap, &l, &cisza);
        let za_murem = plan(&snap, &l, &|_| 1.0);
        let polowa = plan(&snap, &l, &|_| 0.5);
        assert!(otwarte.voices[0].gain > 0.0);
        assert!(za_murem.voices.is_empty(), "mur nie zagłuszył");
        assert!(polowa.voices[0].gain < otwarte.voices[0].gain);
    }

    /// Plan jest funkcją czystą: ten sam snapshot daje tę samą listę w tej samej
    /// kolejności. Bez tego odtwarzacz nie umiałby dopasować głosów między klatkami
    /// i każda klatka zaczynałaby pętle od nowa.
    #[test]
    fn plan_jest_powtarzalny() {
        let mut snap = RenderSnapshot::default();
        for i in 0..40u32 {
            snap.sites.push(zaklad(
                i as i32 * 9_000,
                100 + i as u8,
                AmbientBed::Retail,
                i,
            ));
        }
        let l = Listener {
            pos: [10.0, 5.0, 2.0],
            yaw: 0.7,
        };
        let a = plan(&snap, &l, &cisza);
        let b = plan(&snap, &l, &cisza);
        assert_eq!(a.voices, b.voices);
    }

    /// Mieszanka łóż idzie za tym, co stoi wokół słuchacza. Pustkowie brzmi ciszą,
    /// a nie brakiem dźwięku — to są dwa różne stany.
    #[test]
    fn loze_bierze_sie_z_otoczenia() {
        let snap = RenderSnapshot::default();
        let pusto = plan(&snap, &Listener::default(), &cisza);
        assert_eq!(pusto.beds[AmbientBed::Quiet.as_index()], 1.0);

        let mut snap = RenderSnapshot::default();
        // Dwadzieścia hal tuż obok, dwa parki daleko.
        for i in 0..20u32 {
            snap.sites
                .push(zaklad(10_000, 200, AmbientBed::Industry, i));
        }
        for i in 0..2u32 {
            snap.sites
                .push(zaklad(400_000, 200, AmbientBed::Park, 100 + i));
        }
        let m = plan(&snap, &Listener::default(), &cisza).beds;
        assert!(m[AmbientBed::Industry.as_index()] > 0.9);
        assert!(m[AmbientBed::Park.as_index()] < 0.1);
        let suma: f32 = m.iter().sum();
        assert!((suma - 1.0).abs() < 1e-4, "mieszanka sumuje się do {suma}");
    }

    /// Mieszkaniec **nie** wchodzi do mieszanki, bo jego `district` to dzielnica
    /// zameldowania, a nie ta, w której stoi (`I-19`). Bez tego pas przemysłowy pełen
    /// dojeżdżających brzmiałby osiedlem.
    #[test]
    fn mieszkaniec_nie_narzuca_loza_swoim_zameldowaniem() {
        let mut snap = RenderSnapshot::default();
        snap.district_ambient[4] = AmbientBed::Residential.as_index() as u8;
        for i in 0..500u32 {
            snap.citizens.push(magnat_sim_snapshot::CitizenRenderRec {
                entity_lo: i,
                pos: [1_000, 0, 0],
                district: 4,
                ..Default::default()
            });
        }
        snap.sites.push(zaklad(2_000, 200, AmbientBed::Industry, 1));
        let m = plan(&snap, &Listener::default(), &cisza).beds;
        assert_eq!(m[AmbientBed::Residential.as_index()], 0.0);
        assert_eq!(m[AmbientBed::Industry.as_index()], 1.0);
    }

    /// Jadące auto brzmi ruchem niezależnie od tego, gdzie mieszka jego właściciel.
    #[test]
    fn pojazd_zawsze_niesie_loze_ruchu() {
        let mut snap = RenderSnapshot::default();
        snap.district_ambient[7] = AmbientBed::Park.as_index() as u8;
        snap.vehicles.push(magnat_sim_snapshot::VehicleRenderRec {
            pos: [1_000, 0, 0],
            district: 7,
            ..Default::default()
        });
        let m = plan(&snap, &Listener::default(), &cisza).beds;
        assert_eq!(m[AmbientBed::Traffic.as_index()], 1.0);
    }

    /// Panorama: to, co po prawej ręce, ma być słychać z prawej.
    #[test]
    fn panorama_idzie_za_kursem() {
        let l = Listener {
            pos: [0.0, 0.0, 0.0],
            yaw: 0.0,
        };
        // Kurs 0 patrzy w +X, więc punkt na −Y jest po prawej.
        assert!(panorama([0.0, -10.0, 0.0], &l) > 0.9);
        assert!(panorama([0.0, 10.0, 0.0], &l) < -0.9);
        // Prosto przed sobą — środek.
        assert!(panorama([10.0, 0.0, 0.0], &l).abs() < 0.01);
        // Obrót o ćwierć obrotu przestawia strony.
        let l = Listener {
            yaw: std::f32::consts::FRAC_PI_2,
            ..l
        };
        assert!(panorama([10.0, 0.0, 0.0], &l) > 0.9);
    }

    /// Deszcz nie ma miejsca w świecie: pada nad słuchaczem i jest tym głośniejszy,
    /// im mocniej pada.
    #[test]
    fn deszcz_gra_nad_sluchaczem() {
        let mut snap = RenderSnapshot::default();
        let l = Listener {
            pos: [500.0, 500.0, 2.0],
            yaw: 0.0,
        };
        assert!(plan(&snap, &l, &cisza).voices.is_empty());
        snap.weather.precipitation = 255;
        let p = plan(&snap, &l, &cisza);
        assert_eq!(p.voices.len(), 1);
        assert_eq!(p.voices[0].source, SoundSourceId::RAIN);
        assert_eq!(p.voices[0].pos, l.pos, "deszcz stoi obok słuchacza");
        assert!(p.voices[0].gain > 0.9);
    }
}
