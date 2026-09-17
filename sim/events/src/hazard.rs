//! Ocena hazardu i losowanie (M8c §5.5, PRD §11.1).
//!
//! Cały moduł jest **całkowitoliczbowy i bez stanu**. To nie jest ozdoba: test T3
//! sprawdza, że w module hazardu nie występuje żaden typ zmiennoprzecinkowy, a §5.0
//! fazy stawia ten wymóg mocniej niż `K-6` — mnożnik krzywej i rzut mają być
//! identyczne bitowo na każdej platformie, bo od nich zależy, czy susza w ogóle
//! zaszła, a nie tylko o ile.
//!
//! **Losuje się rzut, nigdy szansa.** Szansa jest funkcją stanu świata: bazowa
//! liczba z katalogu razy iloczyn krzywych nad sondami. Gdyby losować szansę,
//! awaria elektrowni przestałaby być skutkiem zaniedbanej konserwacji i stałaby się
//! loterią — a §1 dokumentu fazy obiecuje dokładnie odwrotnie.

use crate::catalog::{EventDef, SeveritySpec};
use crate::probe::Probe;
use magnat_core::{mix64, rng, StreamId, Tick};

/// Górna granica hazardu: pewność w tej ocenie.
pub const MAX_PPM: u32 = 1_000_000;

/// Jeden czynnik wyliczonego hazardu — do inspektora i do karty zdarzenia.
///
/// Odpowiedź na pytanie „dlaczego jeszcze nie" (i na „dlaczego akurat teraz")
/// jest **tą listą**, a nie liczbą wynikową: gracz ma zobaczyć, że blok stoi
/// na ×5,0 od wieku i ×2,5 od zaległej konserwacji, a nie że hazard wynosi 137 ppm.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FactorTrace {
    pub probe: Probe,
    pub value: i64,
    pub mul_bps: i64,
}

/// Wynik oceny jednej definicji dla jednej instancji zakresu.
#[derive(Clone, Debug, Default)]
pub struct HazardTrace {
    pub ppm: u32,
    pub factors: Vec<FactorTrace>,
}

/// Hazard definicji: `base × Π krzywa_i(sonda_i) / 10000^n`, przycięty do `MAX_PPM`.
///
/// `probe` dostarcza wartości sond; tu nie ma dostępu do świata i nie może być,
/// bo wtedy ocena zależałaby od kolejności definicji.
pub fn hazard_ppm<F: FnMut(Probe) -> i64>(
    def: &EventDef,
    mut probe: F,
    slad: Option<&mut HazardTrace>,
) -> u32 {
    let mut ppm = i128::from(def.trigger.base_ppm);
    let mut czynniki = Vec::new();
    for f in &def.trigger.factors {
        let v = probe(f.probe);
        let m = f.curve.at(v);
        czynniki.push(FactorTrace {
            probe: f.probe,
            value: v,
            mul_bps: m,
        });
        ppm = ppm * i128::from(m.max(0)) / 10_000;
        // Hazard przycięty do pewności nie rośnie dalej — bez tego iloczyn
        // dziesięciu krzywych po ×20 przepełniłby nawet `i128` w katalogu moddera.
        if ppm > i128::from(MAX_PPM) {
            ppm = i128::from(MAX_PPM);
        }
    }
    let wynik = u32::try_from(ppm.clamp(0, i128::from(MAX_PPM))).unwrap_or(0);
    if let Some(s) = slad {
        s.ppm = wynik;
        s.factors = czynniki;
    }
    wynik
}

/// Klucz strumienia dla pary (definicja, instancja zakresu).
///
/// Mieszalnik, a nie suma: definicja 3 na zakładzie 7 i definicja 7 na zakładzie 3
/// mają dostać różne rzuty, a suma ich nie odróżnia (ta sama pułapka, którą
/// `CC-20` znalazł w odcisku awarii sieci).
#[must_use]
pub fn stream_key(def_idx: u16, scope_idx: u64) -> u32 {
    let m = mix64(u64::from(def_idx) << 40 ^ scope_idx.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    (m >> 32) as u32
}

/// Czy zdarzenie zachodzi w tej ocenie. Bez stanu, bez globalnego RNG, bez `f64`.
#[must_use]
pub fn roll(seed: u64, def_idx: u16, scope_idx: u64, tick: Tick, hazard: u32) -> bool {
    if hazard == 0 {
        return false;
    }
    if hazard >= MAX_PPM {
        return true;
    }
    let mut r = rng(
        seed,
        StreamId::EventRoll,
        stream_key(def_idx, scope_idx),
        tick,
    );
    r.gen_range_u32(MAX_PPM) < hazard
}

/// Siła zdarzenia w punktach bazowych.
///
/// Gdy definicja wskazuje sondę, siła bierze się **ze stanu świata** i rzut zostaje
/// jako ±10 % rozrzutu; gdy nie wskazuje — z rzutu w całych widełkach. Rozróżnienie
/// jest istotne dla gracza: susza przy deficycie 80 mm ma być cięższa od suszy przy
/// 40 mm za każdym razem, a nie średnio.
pub fn severity_bps<F: FnMut(Probe) -> i64>(
    spec: &SeveritySpec,
    seed: u64,
    def_idx: u16,
    scope_idx: u64,
    tick: Tick,
    mut probe: F,
) -> u16 {
    let lo = i64::from(spec.min_bps);
    let hi = i64::from(spec.max_bps);
    let mut r = rng(
        seed,
        StreamId::EventSeverity,
        stream_key(def_idx, scope_idx),
        tick,
    );
    let v = match spec.from {
        None => {
            let szer = u32::try_from(hi - lo + 1).unwrap_or(1);
            lo + i64::from(r.gen_range_u32(szer))
        }
        Some((p, x0, x1)) => {
            let x = probe(p);
            let szer = (x1 - x0).max(1);
            let t = ((x - x0).clamp(0, szer) * 10_000) / szer;
            let baza = lo + (hi - lo) * t / 10_000;
            // ±10 % rozrzutu wokół wartości ze stanu świata.
            let jitter = i64::from(r.gen_range_u32(2_001)) - 1_000;
            baza + baza * jitter / 10_000
        }
    };
    u16::try_from(v.clamp(lo, hi)).unwrap_or(spec.min_bps)
}

/// Długość zdarzenia w dobach dla wariantów o z góry znanym czasie.
/// `UntilBelow` zwraca swoje minimum — resztę rozstrzyga świat.
#[must_use]
pub fn duration_days(
    spec: &crate::catalog::DurationSpec,
    seed: u64,
    def_idx: u16,
    scope_idx: u64,
    tick: Tick,
) -> u32 {
    use crate::catalog::DurationSpec as D;
    match *spec {
        D::FixedDays(d) => d.max(1),
        D::RangeDays(a, b) => {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let mut r = rng(
                seed,
                StreamId::EventDuration,
                stream_key(def_idx, scope_idx),
                tick,
            );
            lo + r.gen_range_u32(hi - lo + 1)
        }
        D::UntilBelow { min_days, .. } => min_days.max(1),
    }
}
