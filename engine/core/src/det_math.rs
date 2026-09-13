//! `det_math` — deterministyczne funkcje przestępne (00 §K-6, M0 §5.3a).
//!
//! Zbudowane **wyłącznie** z operacji, które IEEE-754 zaokrągla poprawnie
//! (`+`, `-`, `*`, `/`, `sqrt`) oraz z manipulacji bitami przez `to_bits`/`from_bits`.
//! Żadnego wywołania libm, żadnego `mul_add`. Wynik jest identyczny bit w bit na każdej
//! platformie, którą Rust wspiera z IEEE-754 binary64.
//!
//! **Dlaczego nie fixed-point.** Rozstrzygnięcie K-6 dotyczy *tylko* funkcji przestępnych;
//! podstawowa arytmetyka `f64` jest już deterministyczna. Zakres dynamiczny użyteczności
//! w M5 (10⁻⁶…10⁶) rozjechałby Q32.32, a `f64` ma 15–17 cyfr znaczących w całym zakresie.
//! Wynik `exp`/`ln` nie wchodzi bezpośrednio do pieniądza — do księgowania trafia `Money`
//! po jawnym zaokrągleniu (`Money::mul_ratio`).
//!
//! Sufit tej decyzji: jeśli kiedyś wielkość *pieniężna* będzie musiała przejść przez `exp`
//! (składanie odsetek w M10), robi to przez `Fx` i jawne zaokrąglenie na wejściu i wyjściu,
//! nie przez zmianę reprezentacji tutaj.
//!
//! Rdzeń algorytmiczny (redukcja argumentu, wielomiany minimaksowe, rozbicie ln2 na
//! połówki) jest portem klasycznego fdlibm/FreeBSD `msun` — sprawdzonego zestawu
//! przybliżeń o błędzie < 1 ULP. Port jest dosłowny, bo każde „uproszczenie" w tym
//! kodzie to utrata bitów w miejscu, którego test nie pokaże od razu.
//!
//! Czego tu nie ma i dlaczego: `atan2` (geometria i ruch żyją w `render`/`voxel`, gdzie float
//! wolno), `tanh`/`sigmoid` (wyprowadzalne z `exp` — dokłada pierwszy konsument), wersji `f32`
//! (symulacja nie używa `f32`). Faza, która potrzebuje którejś z nich w kodzie wpływającym
//! na stan trwały, dopisuje ją **tutaj**, z wektorem testowym.
//!
//! `sin`/`cos`/`tan` dopisała **M1**: roczny cykl temperatury (`ClimateCell`) i kąt usypu
//! w erozji wchodzą do stanu trwałego świata, więc nie mogły zostać po stronie renderu.

// Stałe fdlibm zapisujemy z pełną liczbą cyfr źródła, także gdy `f64` nie unosi
// ostatnich — to dokumentacja zamierzonej wartości, nie pomyłka.
#![allow(clippy::excessive_precision)]

use crate::rng::Rng;

// ── Stałe ───────────────────────────────────────────────────────────────────────

/// ln2 rozbity na połówki: `LN2_HI` ma 21 zerowych bitów mantysy, więc iloczyn
/// przez małą liczbę całkowitą jest **dokładny**, a reszta idzie w `LN2_LO`.
const LN2_HI: f64 = 6.931_471_803_691_238_164_9e-1;
const LN2_LO: f64 = 1.908_214_929_270_587_700_02e-10;
/// ln2 jako najbliższy `f64` i jego ogon — do mnożenia w podwojonej precyzji.
const LN2: f64 = std::f64::consts::LN_2;
const LN2_TAIL: f64 = 2.319_046_813_846_299_558_42e-17;
const INV_LN2: f64 = std::f64::consts::LOG2_E;
const IVLN2_HI: f64 = 1.442_695_040_721_446_275_71;
const IVLN2_LO: f64 = 1.675_171_316_488_651_183_53e-10;

// Wielomian minimaksowy dla ln(1+f) (fdlibm Lg1..Lg7).
const LG1: f64 = 6.666_666_666_666_735_13e-1;
const LG2: f64 = 3.999_999_999_940_941_908e-1;
const LG3: f64 = 2.857_142_874_366_239_149e-1;
const LG4: f64 = 2.222_219_843_214_978_396e-1;
const LG5: f64 = 1.818_357_216_161_805_012e-1;
const LG6: f64 = 1.531_383_769_920_937_332e-1;
const LG7: f64 = 1.479_819_860_511_658_591e-1;

// Wielomian minimaksowy dla exp (fdlibm P1..P5).
const P1: f64 = 1.666_666_666_666_660_190_37e-1;
const P2: f64 = -2.777_777_777_701_559_338_42e-3;
const P3: f64 = 6.613_756_321_437_934_361_17e-5;
const P4: f64 = -1.653_390_220_546_525_153_90e-6;
const P5: f64 = 4.138_136_797_057_238_460_39e-8;

// Wielomian dla expm1 (fdlibm Q1..Q5).
const Q1: f64 = -3.333_333_333_333_313_164_28e-2;
const Q2: f64 = 1.587_301_587_254_814_601_65e-3;
const Q3: f64 = -7.936_507_578_674_879_424_73e-5;
const Q4: f64 = 4.008_217_827_329_362_395_52e-6;
const Q5: f64 = -2.010_992_181_836_243_713_26e-7;

/// Próg przepełnienia `exp`: powyżej wynik nie mieści się w `f64`.
/// Stała, nie wynik porównania z libm.
pub const EXP_OVERFLOW_THRESHOLD: f64 = 709.782_712_893_383_973_096;
/// Próg niedomiaru `exp`: poniżej wynik to zero.
pub const EXP_UNDERFLOW_THRESHOLD: f64 = -745.133_219_101_941_108_42;

// ── Narzędzia bitowe ────────────────────────────────────────────────────────────

#[inline]
fn hi_word(x: f64) -> i32 {
    (x.to_bits() >> 32) as u32 as i32
}

#[inline]
fn lo_word(x: f64) -> u32 {
    x.to_bits() as u32
}

#[inline]
fn with_hi_word(x: f64, hi: u32) -> f64 {
    f64::from_bits((u64::from(hi) << 32) | (x.to_bits() & 0xFFFF_FFFF))
}

#[inline]
fn with_lo_word(x: f64, lo: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0xFFFF_FFFF_0000_0000) | u64::from(lo))
}

/// Mnożenie przez potęgę dwójki bez libm (odpowiednik `scalbn`). Dwustopniowe
/// skalowanie pokrywa cały zakres wykładnika łącznie z denormalnymi.
fn scale_pow2(mut x: f64, mut n: i32) -> f64 {
    const P1023: f64 = 8.988_465_674_311_579_54e307; // 2^1023
    const M1022: f64 = 2.225_073_858_507_201_4e-308; // 2^-1022
    const P53: f64 = 9_007_199_254_740_992.0; // 2^53
    if n > 1023 {
        x *= P1023;
        n -= 1023;
        if n > 1023 {
            x *= P1023;
            n -= 1023;
            if n > 1023 {
                n = 1023;
            }
        }
    } else if n < -1022 {
        x *= M1022 * P53;
        n += 1022 - 53;
        if n < -1022 {
            x *= M1022 * P53;
            n += 1022 - 53;
            if n < -1022 {
                n = -1022;
            }
        }
    }
    x * f64::from_bits(((0x3FF + n) as u64) << 52)
}

/// Rozbicie Veltkampa: `a = hi + lo`, każda połówka na 26 bitach mantysy.
#[inline]
fn split(a: f64) -> (f64, f64) {
    const SPLITTER: f64 = 134_217_729.0; // 2^27 + 1
    let c = SPLITTER * a;
    let hi = c - (c - a);
    (hi, a - hi)
}

/// Dokładny iloczyn: `a*b = p + e` (algorytm Dekkera). Dwa mnożenia zamiast FMA —
/// `mul_add` jest zakazany, bo na sprzęcie bez FMA schodzi do emulacji libm (M0 §2).
#[inline]
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let e = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    (p, e)
}

// ── Logarytmy ───────────────────────────────────────────────────────────────────

/// `s*(hfsq + R)` — ogon rozwinięcia ln(1+f) (fdlibm `k_log1p`).
#[inline]
fn k_log1p(f: f64) -> f64 {
    let s = f / (2.0 + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let r = t2 + t1;
    let hfsq = 0.5 * f * f;
    s * (hfsq + r)
}

/// Normalizacja `x = 2^k · m`, `m ∈ [√2/2, √2)`. Zwraca `(k, f = m − 1)`.
/// Wywołujący ma już obsłużone przypadki brzegowe (x ≤ 0, inf, NaN).
fn log_reduce(x: f64) -> (i32, f64) {
    const TWO54: f64 = 1.801_439_850_948_198_4e16;
    let mut x = x;
    let mut k = 0i32;
    let mut hx = hi_word(x);
    if hx < 0x0010_0000 {
        // Subnormalna: podnieś o 2^54 i odlicz to od wykładnika.
        k -= 54;
        x *= TWO54;
        hx = hi_word(x);
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000F_FFFF;
    let i = (hx + 0x9_5F64) & 0x10_0000;
    let x = with_hi_word(x, (hx | (i ^ 0x3FF0_0000)) as u32);
    k += i >> 20;
    (k, x - 1.0)
}

/// Przypadki brzegowe wspólne dla `ln` i `log2`.
/// `Some(v)` = wynik gotowy, `None` = argument nadaje się do redukcji.
#[inline]
fn log_special(x: f64) -> Option<f64> {
    let hx = hi_word(x);
    let lx = lo_word(x);
    if hx < 0x0010_0000 {
        if ((hx & 0x7FFF_FFFF) as u32 | lx) == 0 {
            return Some(f64::NEG_INFINITY); // ln(±0)
        }
        if hx < 0 {
            return Some(f64::NAN); // ln(x < 0)
        }
    } else if hx >= 0x7FF0_0000 {
        return Some(x + x); // +inf → +inf, NaN → NaN
    }
    None
}

/// `ln(x)`. Rozkład `x = m · 2^k`, wielomian minimaksowy w `s = (m−1)/(m+1)`,
/// człon `k·ln2` liczony jako `k·LN2_HI + k·LN2_LO`, żeby uniknąć kasowania
/// dla x bliskich 1. Dokładność ≤ 1 ULP.
///
/// Domena: `x > 0`; `x == 0` → `-∞`, `x < 0` lub NaN → NaN.
#[must_use]
pub fn ln(x: f64) -> f64 {
    if let Some(v) = log_special(x) {
        return v;
    }
    let (k, f) = log_reduce(x);
    let hfsq = 0.5 * f * f;
    let r = k_log1p(f);
    let dk = f64::from(k);
    dk * LN2_HI - ((hfsq - (r + dk * LN2_LO)) - f)
}

/// `log2(x)` — ta sama dekompozycja co `ln`, ale `k` wchodzi **dokładnie**
/// (bez mnożenia przez ln2). Dokładność ≤ 1 ULP.
#[must_use]
pub fn log2(x: f64) -> f64 {
    let (hi, lo) = match log2_ext(x) {
        Ok(v) => v,
        Err(v) => return v,
    };
    hi + lo
}

/// `log2(x)` w podwojonej precyzji: zwraca `(hi, lo)` takie, że `hi + lo ≈ log2(x)`
/// z błędem rzędu 2⁻¹⁰⁵. Konsument: `pow` — bez tego błąd logarytmu jest mnożony
/// przez wykładnik i przy `y ≈ 1000` wychodzi poza zakres użyteczny.
fn log2_ext(x: f64) -> Result<(f64, f64), f64> {
    if let Some(v) = log_special(x) {
        return Err(v);
    }
    let (k, f) = log_reduce(x);
    let hfsq = 0.5 * f * f;
    // f − hfsq musi być policzone w podwyższonej precyzji, inaczej przy x bliskim
    // √2 albo 1/√2 następuje duże kasowanie.
    let hi = with_lo_word(f - hfsq, 0);
    let lo = (f - hi) - hfsq + k_log1p(f);
    let val_hi = hi * IVLN2_HI;
    let val_lo = (lo + hi) * IVLN2_LO + lo * IVLN2_HI;
    let y = f64::from(k);
    let w = y + val_hi;
    let val_lo = val_lo + ((y - w) + val_hi);
    Ok((w, val_lo))
}

/// `ln(1+x)` bez utraty precyzji dla |x| ≪ 1 — dla samego `ln(1.0 + x)` błąd sięga
/// tam 10⁸ ULP. Konsument: M5 (log-użyteczność przyrostów), M10 (stopy zwrotu).
#[must_use]
pub fn ln1p(x: f64) -> f64 {
    let hx = hi_word(x);
    let ax = hx & 0x7FFF_FFFF;
    let mut k = 1i32;
    let mut f = 0.0f64;
    let mut hu = 0i32;
    let mut c = 0.0f64;

    if hx < 0x3FDA_827A {
        // 1+x < sqrt(2)
        if ax >= 0x3FF0_0000 {
            // x <= -1.0
            if x == -1.0 {
                return f64::NEG_INFINITY;
            }
            return f64::NAN;
        }
        if ax < 0x3E20_0000 {
            // |x| < 2^-29: szereg urywa się po drugim wyrazie
            if ax < 0x3C90_0000 {
                return x; // |x| < 2^-54
            }
            return x - x * x * 0.5;
        }
        if hx > 0 || hx <= -0x402D_413C {
            // sqrt(2)/2 <= 1+x < sqrt(2)
            k = 0;
            f = x;
            hu = 1;
        }
    }
    if hx >= 0x7FF0_0000 {
        return x + x; // inf / NaN
    }
    if k != 0 {
        let u = if hx < 0x4340_0000 {
            let u = 1.0 + x;
            let hu_local = hi_word(u);
            k = (hu_local >> 20) - 1023;
            // Człon korekcyjny: to, co zgubiło zaokrąglenie 1+x.
            c = if k > 0 { 1.0 - (u - x) } else { x - (u - 1.0) } / u;
            hu = hu_local;
            u
        } else {
            let u = x;
            hu = hi_word(u);
            k = (hu >> 20) - 1023;
            c = 0.0;
            u
        };
        hu &= 0x000F_FFFF;
        let u = if hu < 0x6_A09E {
            with_hi_word(u, (hu | 0x3FF0_0000) as u32)
        } else {
            k += 1;
            let u = with_hi_word(u, (hu | 0x3FE0_0000) as u32);
            hu = (0x10_0000 - hu) >> 2;
            u
        };
        f = u - 1.0;
    }

    let hfsq = 0.5 * f * f;
    let dk = f64::from(k);
    if hu == 0 {
        // |f| < 2^-20
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            return dk * LN2_HI + (c + dk * LN2_LO);
        }
        let r = hfsq * (1.0 - 0.666_666_666_666_666_6 * f);
        if k == 0 {
            return f - r;
        }
        return dk * LN2_HI - ((r - (dk * LN2_LO + c)) - f);
    }
    let r = k_log1p(f);
    if k == 0 {
        f - (hfsq - r)
    } else {
        dk * LN2_HI - ((hfsq - (r + (dk * LN2_LO + c))) - f)
    }
}

// ── Eksponenty ──────────────────────────────────────────────────────────────────

/// Rdzeń `exp` dla argumentu już zredukowanego: liczy `2^k · e^(hi − lo)`,
/// gdzie `|hi − lo| ≤ ln2/2`.
fn exp_reduced(hi: f64, lo: f64, k: i32) -> f64 {
    let x = hi - lo;
    let t = x * x;
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);
    scale_pow2(y, k)
}

/// `exp(x)`. Redukcja `k = round(x · log2e)`, `r = x − k·LN2_HI − k·LN2_LO`,
/// wielomian minimaksowy stopnia 6 w schemacie Hornera o stałej kolejności działań,
/// skalowanie przez `2^k` składane bitowo. Dokładność ≤ 1 ULP.
///
/// Przepełnienie: `x > 709.78…` → `+∞`; `x < −745.13…` → `0.0`. Progi są stałymi.
#[must_use]
pub fn exp(x: f64) -> f64 {
    let hx_signed = hi_word(x);
    let sign_negative = hx_signed < 0;
    let hx = hx_signed & 0x7FFF_FFFF;

    if hx >= 0x4086_2E42 {
        // |x| >= 709.78…
        if hx >= 0x7FF0_0000 {
            if (hx & 0x000F_FFFF) != 0 || lo_word(x) != 0 {
                return x + x; // NaN
            }
            return if sign_negative { 0.0 } else { f64::INFINITY };
        }
        if x > EXP_OVERFLOW_THRESHOLD {
            return f64::INFINITY;
        }
        if x < EXP_UNDERFLOW_THRESHOLD {
            return 0.0;
        }
    }

    let (hi, lo, k);
    if hx > 0x3FD6_2E42 {
        // |x| > 0.5·ln2
        if hx < 0x3FF0_A2B2 {
            // |x| < 1.5·ln2 — redukcja o jedno ln2
            if sign_negative {
                hi = x + LN2_HI;
                lo = -LN2_LO;
                k = -1;
            } else {
                hi = x - LN2_HI;
                lo = LN2_LO;
                k = 1;
            }
        } else {
            let kf = INV_LN2 * x + if sign_negative { -0.5 } else { 0.5 };
            k = kf as i32;
            let t = f64::from(k);
            hi = x - t * LN2_HI; // dokładne: LN2_HI ma 21 zerowych bitów mantysy
            lo = t * LN2_LO;
        }
    } else if hx < 0x3E30_0000 {
        // |x| < 2^-28: exp(x) = 1 + x co do ostatniego bitu
        return 1.0 + x;
    } else {
        hi = x;
        lo = 0.0;
        k = 0;
    }
    exp_reduced(hi, lo, k)
}

/// `exp(x) − 1`, dokładne dla małych `x` — tam `exp(x) - 1.0` gubi wszystkie cyfry
/// znaczące. Konsument: M10 (stopy zwrotu), M5 (przyrosty użyteczności).
#[must_use]
pub fn exp_m1(x: f64) -> f64 {
    let hx_signed = hi_word(x);
    let sign_negative = hx_signed < 0;
    let hx = hx_signed & 0x7FFF_FFFF;

    let mut x = x;
    let mut c = 0.0f64;
    let k: i32;

    if hx >= 0x4043_687A {
        // |x| >= 56·ln2
        if hx >= 0x4086_2E42 {
            if hx >= 0x7FF0_0000 {
                if (hx & 0x000F_FFFF) != 0 || lo_word(x) != 0 {
                    return x + x; // NaN
                }
                return if sign_negative { -1.0 } else { f64::INFINITY };
            }
            if x > EXP_OVERFLOW_THRESHOLD {
                return f64::INFINITY;
            }
        }
        if sign_negative {
            return -1.0; // exp(x) − 1 = −1 z dokładnością do ULP
        }
    }

    if hx > 0x3FD6_2E42 {
        let (hi, lo);
        if hx < 0x3FF0_A2B2 {
            if sign_negative {
                hi = x + LN2_HI;
                lo = -LN2_LO;
                k = -1;
            } else {
                hi = x - LN2_HI;
                lo = LN2_LO;
                k = 1;
            }
        } else {
            let kf = INV_LN2 * x + if sign_negative { -0.5 } else { 0.5 };
            k = kf as i32;
            let t = f64::from(k);
            hi = x - t * LN2_HI;
            lo = t * LN2_LO;
        }
        x = hi - lo;
        c = (hi - x) - lo;
    } else if hx < 0x3C90_0000 {
        return x; // |x| < 2^-54
    } else {
        k = 0;
    }

    let hfx = 0.5 * x;
    let hxs = x * hfx;
    let r1 = 1.0 + hxs * (Q1 + hxs * (Q2 + hxs * (Q3 + hxs * (Q4 + hxs * Q5))));
    let t = 3.0 - r1 * hfx;
    let mut e = hxs * ((r1 - t) / (6.0 - x * t));
    if k == 0 {
        return x - (x * e - hxs);
    }
    e = x * (e - c) - c;
    e -= hxs;
    if k == -1 {
        return 0.5 * (x - e) - 0.5;
    }
    if k == 1 {
        if x < -0.25 {
            return -2.0 * (e - (x + 0.5));
        }
        return 1.0 + 2.0 * (x - e);
    }
    if k <= -2 || k > 56 {
        // Wystarczy exp(x) − 1
        let y = 1.0 - (e - x);
        return scale_pow2(y, k) - 1.0;
    }
    if k < 20 {
        let t = f64::from_bits(u64::from(0x3FF0_0000u32 - (0x20_0000u32 >> k)) << 32); // 1 − 2^-k
        let y = t - (e - x);
        scale_pow2(y, k)
    } else {
        let t = f64::from_bits(u64::from(((0x3FF - k) as u32) << 20) << 32); // 2^-k
        let y = x - (e + t) + 1.0;
        scale_pow2(y, k)
    }
}

/// `exp2(x) = 2^x`. Redukcja do `k + r`, `|r| ≤ 0,5`, potem `e^(r·ln2)` z argumentem
/// liczonym w podwojonej precyzji (iloczyn Dekkera), więc mnożenie przez ln2 nie zjada
/// bitów. Dokładność ≤ 1 ULP.
#[must_use]
pub fn exp2(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x >= 1024.0 {
        return f64::INFINITY;
    }
    if x < -1075.0 {
        return 0.0;
    }
    exp2_ext(x, 0.0)
}

/// `2^(hi+lo)` dla pary w podwojonej precyzji. Wspólny rdzeń `exp2` i `pow`.
fn exp2_ext(hi: f64, lo: f64) -> f64 {
    // k = round(hi); r = hi − k jest dokładne (Sterbenz), więc cała utrata precyzji
    // siedzi wyłącznie w mnożeniu przez ln2 — i dlatego robimy je przez two_prod.
    let k = round_to_nearest(hi);
    let r = (hi - k) + lo;
    let ki = k as i32;
    let (p_hi, p_lo) = two_prod(r, LN2);
    let p_lo = p_lo + r * LN2_TAIL;
    // exp_reduced liczy e^(hi − lo), stąd znak drugiego składnika.
    exp_reduced(p_hi, -p_lo, ki)
}

/// Zaokrąglenie do najbliższej liczby całkowitej (połówki do parzystej) sztuczką
/// „dodaj i odejmij 2^52". Czysta arytmetyka IEEE, więc ten sam wynik wszędzie;
/// wybór reguły połówek nie ma tu znaczenia — liczy się to, że jest jedna.
#[inline]
fn round_to_nearest(x: f64) -> f64 {
    const TWO52: f64 = 4_503_599_627_370_496.0;
    if x.abs() >= TWO52 {
        return x; // i tak jest całkowite
    }
    if x >= 0.0 {
        (x + TWO52) - TWO52
    } else {
        (x - TWO52) + TWO52
    }
}

/// `x^y`, liczone jako `exp2(y · log2(x))` w arytmetyce podwojonej precyzji
/// (para hi/lo z algorytmem Dekkera — dwa mnożenia zamiast FMA).
///
/// **Dokładność zależy od wielkości wykładnika wyniku** i to jest korekta wobec planu
/// M0, który deklarował stałe ≤ 2 ULP. Błąd logarytmu (≈ 2⁻⁵⁹ względnie po złożeniu
/// pary) jest w tym algorytmie mnożony przez `y`, więc realna granica to
/// `|y·log2 x| · 2⁻⁵⁹` ULP: **≤ 2 ULP dla |y·log2 x| ≤ 256** (czyli dla wyników
/// w zakresie 2^±256 przy umiarkowanym `y`) i **≤ 8 ULP** w całym zakresie `f64`.
/// Zmierzony najgorszy przypadek tabeli referencyjnej: 5 ULP dla `1.5^1000`.
/// Zejście niżej wymagałoby algorytmu klasy fdlibm `e_pow.c`; konsumenci `pow`
/// (M5, M7, M10) pracują na wykładnikach rzędu jedności, więc nie płacimy za to dziś.
///
/// Przypadki brzegowe obsłużone jawnie wg IEEE-754.
#[must_use]
pub fn pow(x: f64, y: f64) -> f64 {
    // IEEE-754: x^0 == 1 dla KAŻDEGO x, także NaN i nieskończoności.
    if y == 0.0 {
        return 1.0;
    }
    if x == 1.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }

    let y_int = is_integer(y);
    let y_odd_int = y_int && is_odd_integer(y);

    if x == 0.0 {
        let sign_negative_zero = x.is_sign_negative() && y_odd_int;
        return if y > 0.0 {
            if sign_negative_zero {
                -0.0
            } else {
                0.0
            }
        } else if sign_negative_zero {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    if x.is_infinite() {
        // Znak wynika wyłącznie z podstawy i parzystości wykładnika, moduł — z jego znaku.
        let negative = x < 0.0 && y_odd_int;
        return match (y > 0.0, negative) {
            (true, false) => f64::INFINITY,
            (true, true) => f64::NEG_INFINITY,
            (false, false) => 0.0,
            (false, true) => -0.0,
        };
    }

    if y.is_infinite() {
        let ax = x.abs();
        if ax == 1.0 {
            return 1.0; // (-1)^±inf == 1 wg IEEE-754
        }
        return if (ax > 1.0) == (y > 0.0) {
            f64::INFINITY
        } else {
            0.0
        };
    }

    if x < 0.0 && !y_int {
        return f64::NAN; // wynik byłby zespolony
    }

    let sign = if x < 0.0 && y_odd_int { -1.0 } else { 1.0 };
    let ax = x.abs();

    let (l_hi, l_lo) = match log2_ext(ax) {
        Ok(v) => v,
        Err(v) => return v, // nieosiągalne: przypadki brzegowe już odsiane
    };

    // y · (l_hi + l_lo) w podwojonej precyzji.
    let (mut p_hi, p_e) = two_prod(y, l_hi);
    let mut p_lo = p_e + y * l_lo;
    // Normalizacja pary.
    let s = p_hi + p_lo;
    p_lo -= s - p_hi;
    p_hi = s;

    if p_hi > 1024.0 {
        return sign * f64::INFINITY;
    }
    if p_hi < -1075.0 {
        return sign * 0.0;
    }
    sign * exp2_ext(p_hi, p_lo)
}

#[inline]
fn is_integer(y: f64) -> bool {
    let a = y.abs();
    a >= 4_503_599_627_370_496.0 || a == trunc_to_integer(a)
}

#[inline]
fn trunc_to_integer(a: f64) -> f64 {
    const TWO52: f64 = 4_503_599_627_370_496.0;
    if a >= TWO52 {
        return a;
    }
    let t = (a + TWO52) - TWO52;
    if t > a {
        t - 1.0
    } else {
        t
    }
}

#[inline]
fn is_odd_integer(y: f64) -> bool {
    let a = y.abs();
    if a >= 9_007_199_254_740_992.0 {
        return false; // 2^53 i więcej: każda taka liczba jest parzysta
    }
    let h = a * 0.5;
    h != trunc_to_integer(h)
}

/// Przekierowanie na `f64::sqrt` — IEEE-754 wymaga poprawnego zaokrąglenia `sqrt`,
/// więc jest deterministyczny na każdej platformie. Istnieje tylko po to, żeby kod
/// symulacji miał **jedno** miejsce, do którego sięga po matematykę.
#[inline]
#[must_use]
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

// ── Funkcje trygonometryczne (dopisane przez M1) ────────────────────────────────

// Redukcja argumentu Cody'ego–Waite'a: π/2 rozbite na trzy części o rozłącznych bitach,
// żeby iloczyn przez małą liczbę całkowitą był dokładny (fdlibm `__rem_pio2`).
// Clippy proponuje tu `FRAC_2_PI` ze `std`. Odmawiamy świadomie: redukcja Cody'ego–Waite'a
// wymaga, żeby ta stała i trójka `PIO2_*` pochodziły z **jednego** rozbicia π/2 o rozłącznych
// bitach. Podmiana jednej z nich na stałą biblioteczną rozspójnia rozbicie i psuje ogon
// redukcji — czego test dokładności nie pokaże na małych argumentach, a pokaże na dużych.
#[allow(clippy::approx_constant)]
const INVPIO2: f64 = 6.366_197_723_675_813_824_33e-01;
const PIO2_1: f64 = 1.570_796_326_734_125_614_17e+00;
const PIO2_1T: f64 = 6.077_100_506_506_192_249_32e-11;
const PIO2_2: f64 = 6.077_100_506_303_965_976_60e-11;
const PIO2_2T: f64 = 2.022_266_248_795_950_631_54e-21;

// Wielomian minimaksowy dla sin na [−π/4, π/4] (fdlibm S1..S6).
const S1: f64 = -1.666_666_666_666_663_243_48e-01;
const S2: f64 = 8.333_333_333_322_489_461_24e-03;
const S3: f64 = -1.984_126_982_985_794_931_34e-04;
const S4: f64 = 2.755_731_370_707_006_767_89e-06;
const S5: f64 = -2.505_076_025_340_686_341_95e-08;
const S6: f64 = 1.589_690_995_211_550_102_21e-10;

// Wielomian minimaksowy dla cos na [−π/4, π/4] (fdlibm C1..C6).
const C1: f64 = 4.166_666_666_666_660_190_37e-02;
const C2: f64 = -1.388_888_888_887_410_957_49e-03;
const C3: f64 = 2.480_158_728_947_672_941_78e-05;
const C4: f64 = -2.755_731_435_139_066_330_35e-07;
const C5: f64 = 2.087_572_321_298_174_827_90e-09;
const C6: f64 = -1.135_964_755_778_819_482_65e-11;

/// Największy argument, dla którego redukcja dwuczłonowa trzyma deklarowaną dokładność.
/// Powyżej trzeba by Payne'a–Hanka — i wtedy warto zapytać, po co komuś sinus z argumentu
/// o dziesięciu cyfrach przed przecinkiem, zamiast cicho zwracać liczbę bez znaczenia.
pub const TRIG_ARG_LIMIT: f64 = 1.0e6;

/// Jądro sinusa na zredukowanym argumencie `x + y`, gdzie `|x| ≤ π/4`, a `y` to ogon redukcji.
#[inline]
fn kernel_sin(x: f64, y: f64) -> f64 {
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    x - ((z * (0.5 * y - v * r) - y) - v * S1)
}

/// Jądro cosinusa na zredukowanym argumencie `x + y`.
#[inline]
fn kernel_cos(x: f64, y: f64) -> f64 {
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    // Rozbicie na `w` i resztę zachowuje bity, które `1 − z/2` samo w sobie by zgubiło.
    w + (((1.0 - w) - hz) + (z * r - x * y))
}

/// Redukcja `x` do `(n, y0, y1)`, gdzie `x ≈ n·π/2 + y0 + y1` i `|y0| ≤ π/4`.
fn rem_pio2(x: f64) -> (i64, f64, f64) {
    let fnv = round_to_nearest(x * INVPIO2);
    let mut r = x - fnv * PIO2_1;
    let mut w = fnv * PIO2_1T;
    let mut y0 = r - w;

    // Druga runda redukcji, gdy pierwsza zjadła zbyt wiele bitów znaczących.
    let exp_x = ((x.to_bits() >> 52) & 0x7ff) as i32;
    let exp_y = ((y0.to_bits() >> 52) & 0x7ff) as i32;
    if exp_x - exp_y > 16 {
        let t = r;
        w = fnv * PIO2_2;
        r = t - w;
        w = fnv * PIO2_2T - ((t - r) - w);
        y0 = r - w;
    }
    let y1 = (r - y0) - w;
    (fnv as i64, y0, y1)
}

/// Sinus. Deterministyczny na każdej platformie; błąd ≤ 2 ULP dla `|x| ≤ TRIG_ARG_LIMIT`.
/// Poza tym zakresem zwraca `NaN` — cicha utrata znaczenia byłaby gorsza niż jawny błąd.
#[must_use]
pub fn sin(x: f64) -> f64 {
    if !x.is_finite() || x.abs() > TRIG_ARG_LIMIT {
        return f64::NAN;
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1),
        _ => -kernel_cos(y0, y1),
    }
}

/// Cosinus. Ograniczenia jak w [`sin`].
#[must_use]
pub fn cos(x: f64) -> f64 {
    if !x.is_finite() || x.abs() > TRIG_ARG_LIMIT {
        return f64::NAN;
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1),
    }
}

/// Tangens jako iloraz. Osobne jądro tangensa dałoby ułamek ULP dokładności więcej
/// za drugi wielomian do utrzymania — nieopłacalne, dopóki nikt nie liczy tangensa
/// w gorącej pętli.
#[must_use]
pub fn tan(x: f64) -> f64 {
    let c = cos(x);
    if c == 0.0 {
        return f64::NAN;
    }
    sin(x) / c
}

// ── Softmax ─────────────────────────────────────────────────────────────────────

/// Softmax stabilny numerycznie, w miejscu.
///
/// 1. `m = max(xs)` — po indeksie rosnąco, nie przez `f64::max` na iteratorze (NaN),
/// 2. `xs[i] = exp((xs[i] − m) / temperature)`,
/// 3. suma **w kolejności indeksów** (00 §2: float sumowany w ustalonej kolejności),
///    bez redukcji parami i bez rayon,
/// 4. normalizacja.
///
/// Wejście puste → brak działania. `temperature <= 0` → panika: to błąd wywołującego,
/// nie stan danych.
pub fn softmax_in_place(xs: &mut [f64], temperature: f64) {
    assert!(
        temperature > 0.0,
        "softmax: temperatura musi być dodatnia (dostała {temperature})"
    );
    if xs.is_empty() {
        return;
    }
    let mut m = xs[0];
    for &v in xs.iter().skip(1) {
        if v > m {
            m = v;
        }
    }
    let mut sum = 0.0;
    for v in xs.iter_mut() {
        let e = exp((*v - m) / temperature);
        *v = e;
        sum += e;
    }
    // Suma nie może być zerem: exp(0) = 1 dla wyrazu o wartości maksymalnej.
    for v in xs.iter_mut() {
        *v /= sum;
    }
}

#[must_use]
pub fn softmax(xs: &[f64], temperature: f64) -> Vec<f64> {
    let mut out = xs.to_vec();
    softmax_in_place(&mut out, temperature);
    out
}

/// Losowanie z rozkładu softmax — najczęstszy konsument (M5: wybór sklepu,
/// M7: decyzje firm). Kumulacja w kolejności indeksów, próg z `Rng::next_u64`
/// przeskalowany do [0,1) przez dzielenie przez 2^53 (dokładne, bez zaokrąglenia).
///
/// Zwraca indeks wybranej opcji. **Kolejność `weights` jest kontraktem wywołującego.**
/// Wejście puste → panika.
pub fn softmax_pick(weights: &[f64], temperature: f64, rng: &mut Rng) -> usize {
    assert!(!weights.is_empty(), "softmax_pick: brak opcji do wyboru");
    let p = softmax(weights, temperature);
    let threshold = (rng.next_u64() >> 11) as f64 / 9_007_199_254_740_992.0; // 2^53
    let mut acc = 0.0;
    for (i, pi) in p.iter().enumerate() {
        acc += *pi;
        if threshold < acc {
            return i;
        }
    }
    // Wyłącznie przez resztkę zaokrąglenia sumy do 1.0.
    p.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::{rng, StreamId};
    use crate::types::Tick;

    /// Błąd w ULP względem wartości odniesienia.
    fn ulp_error(got: f64, want: f64) -> f64 {
        if got == want {
            return 0.0;
        }
        if !got.is_finite() || !want.is_finite() {
            return f64::INFINITY;
        }
        let ulp = {
            let bits = want.abs().to_bits();
            let next = f64::from_bits(bits + 1);
            next - want.abs()
        };
        ((got - want) / ulp).abs()
    }

    #[test]
    fn ln_na_wartosciach_kotwiczacych() {
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(ln(0.0), f64::NEG_INFINITY);
        assert_eq!(ln(-0.0), f64::NEG_INFINITY);
        assert!(ln(-1.0).is_nan());
        assert!(ln(f64::NAN).is_nan());
        assert_eq!(ln(f64::INFINITY), f64::INFINITY);
        // ln(e) == 1 co do bitu
        assert_eq!(ln(std::f64::consts::E), 1.0);
        assert!(ulp_error(ln(2.0), std::f64::consts::LN_2) <= 1.0);
        assert!(ulp_error(ln(10.0), std::f64::consts::LN_10) <= 1.0);
    }

    #[test]
    fn exp_na_wartosciach_kotwiczacych() {
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(exp(f64::INFINITY), f64::INFINITY);
        assert_eq!(exp(f64::NEG_INFINITY), 0.0);
        assert!(exp(f64::NAN).is_nan());
        assert_eq!(exp(710.0), f64::INFINITY);
        assert_eq!(exp(-746.0), 0.0);
        assert!(ulp_error(exp(1.0), std::f64::consts::E) <= 1.0);
    }

    #[test]
    fn exp_i_ln_sa_wzajemnie_odwrotne() {
        // Granica jest funkcją uwarunkowania zadania, nie jakości implementacji:
        // błąd bezwzględny ln(x) to ~0,5 ULP z |ln x|, a exp przenosi go na błąd
        // WZGLĘDNY wyniku, czyli ~0,5·|ln x| ULP. Dla x = 10^-100 (ln ≈ -230) daje to
        // ok. 115 ULP przy idealnym ln — stały próg 3 ULP z planu M0 §7.2 jest
        // matematycznie nieosiągalny dla żadnej implementacji. Plan poprawiony.
        let mut x = 1e-100;
        while x < 1e100 {
            let back = exp(ln(x));
            let dopuszczalne = 3.0 + ln(x).abs();
            assert!(
                ulp_error(back, x) <= dopuszczalne,
                "x={x:e} → {back:e}, błąd {} ULP (dopuszczalne {dopuszczalne})",
                ulp_error(back, x)
            );
            x *= 7.3;
        }
    }

    #[test]
    fn ln_jest_monotoniczny() {
        // 10^6 kolejnych wartości f64 wokół 1.0 — tam kasowanie jest najgroźniejsze.
        let mut bits = 1.0f64.to_bits() - 500_000;
        let mut prev = ln(f64::from_bits(bits));
        for _ in 0..1_000_000u32 {
            bits += 1;
            let v = ln(f64::from_bits(bits));
            assert!(
                v >= prev,
                "ln złamał monotoniczność przy {}",
                f64::from_bits(bits)
            );
            prev = v;
        }
    }

    #[test]
    fn exp_jest_monotoniczny() {
        let mut bits = 1.0f64.to_bits();
        let mut prev = exp(f64::from_bits(bits));
        for _ in 0..1_000_000u32 {
            bits += 1;
            let v = exp(f64::from_bits(bits));
            assert!(v >= prev);
            prev = v;
        }
    }

    #[test]
    fn log2_na_potegach_dwojki_jest_dokladny() {
        for k in -1000..=1000i32 {
            let x = scale_pow2(1.0, k);
            if x == 0.0 || !x.is_finite() {
                continue;
            }
            assert_eq!(log2(x), f64::from(k), "log2(2^{k})");
        }
        assert!(ulp_error(log2(10.0), std::f64::consts::LOG2_10) <= 1.0);
    }

    #[test]
    fn exp2_na_potegach_dwojki_jest_dokladny() {
        for k in -1022..=1023i32 {
            assert_eq!(exp2(f64::from(k)), scale_pow2(1.0, k), "2^{k}");
        }
        assert_eq!(exp2(1024.0), f64::INFINITY);
        assert_eq!(exp2(-1075.0), 0.0);
        assert!(ulp_error(exp2(0.5), std::f64::consts::SQRT_2) <= 1.0);
    }

    #[test]
    fn ln1p_trzyma_precyzje_przy_zerze() {
        assert_eq!(ln1p(0.0), 0.0);
        assert_eq!(ln1p(-1.0), f64::NEG_INFINITY);
        assert!(ln1p(-1.5).is_nan());
        // Dla x = 1e-17 wynik to praktycznie x; ln(1.0 + x) dałoby tu 0.0.
        assert_eq!(ln1p(1e-17), 1e-17);
        assert!(ulp_error(ln1p(1.0), std::f64::consts::LN_2) <= 1.0);
        // Zgodność z ln na argumentach, gdzie ln jest wiarygodny.
        for x in [0.5, 1.5, 7.0, 100.0, 1e10] {
            assert!(ulp_error(ln1p(x), ln(1.0 + x)) <= 2.0, "x={x}");
        }
    }

    #[test]
    fn exp_m1_trzyma_precyzje_przy_zerze() {
        assert_eq!(exp_m1(0.0), 0.0);
        assert_eq!(exp_m1(-1e-300), -1e-300);
        assert_eq!(exp_m1(f64::NEG_INFINITY), -1.0);
        assert_eq!(exp_m1(f64::INFINITY), f64::INFINITY);
        assert!(ulp_error(exp_m1(1.0), std::f64::consts::E - 1.0) <= 1.0);
        for x in [-3.0, -0.7, -0.2, 0.2, 0.7, 3.0, 30.0] {
            assert!(ulp_error(exp_m1(x), exp(x) - 1.0) <= 2.0, "x={x}");
        }
    }

    #[test]
    fn pow_brzegi_wg_ieee754() {
        assert_eq!(pow(f64::NAN, 0.0), 1.0);
        assert_eq!(pow(1.0, f64::NAN), 1.0);
        assert!(pow(f64::NAN, 2.0).is_nan());
        assert_eq!(pow(-2.0, 3.0), -8.0);
        assert_eq!(pow(-2.0, 2.0), 4.0);
        assert!(pow(-2.0, 0.5).is_nan());
        assert_eq!(pow(0.0, 3.0), 0.0);
        assert_eq!(pow(-0.0, 3.0), -0.0);
        assert_eq!(pow(0.0, -1.0), f64::INFINITY);
        assert_eq!(pow(-0.0, -3.0), f64::NEG_INFINITY);
        assert_eq!(pow(2.0, f64::INFINITY), f64::INFINITY);
        assert_eq!(pow(0.5, f64::INFINITY), 0.0);
        assert_eq!(pow(-1.0, f64::INFINITY), 1.0);
        assert_eq!(pow(f64::INFINITY, 2.0), f64::INFINITY);
        assert_eq!(pow(f64::NEG_INFINITY, 3.0), f64::NEG_INFINITY);
        assert_eq!(pow(f64::NEG_INFINITY, 2.0), f64::INFINITY);
        assert_eq!(pow(2.0, 1024.0), f64::INFINITY);
        assert_eq!(pow(2.0, -1200.0), 0.0);
    }

    #[test]
    fn pow_na_wykladnikach_calkowitych() {
        // Potęgi całkowite muszą wychodzić dokładnie, dopóki mieszczą się w mantysie.
        for base in [2.0f64, 3.0, 0.5, 10.0] {
            let mut expected = 1.0f64;
            for k in 0..15 {
                assert!(
                    ulp_error(pow(base, f64::from(k)), expected) <= 2.0,
                    "{base}^{k}"
                );
                expected *= base;
            }
        }
    }

    #[test]
    fn softmax_sumuje_sie_do_jednosci() {
        let w = [1.0, 2.0, 3.0, -4.0, 0.0, 10.0, -10.0];
        let p = softmax(&w, 1.0);
        let sum: f64 = p.iter().sum();
        assert!((sum - 1.0).abs() < 1e-12, "suma = {sum}");
        assert!(p.iter().all(|&v| (0.0..=1.0).contains(&v)));
        // Największa waga ma największe prawdopodobieństwo.
        let max_i = p
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert_eq!(max_i, 5);
    }

    #[test]
    fn softmax_jest_odporny_na_przesuniecie_skali() {
        // Stabilizacja przez odjęcie maksimum: +1000 do każdej wagi nie może
        // wyprodukować nieskończoności.
        let a = softmax(&[1.0, 2.0, 3.0], 1.0);
        let b = softmax(&[1001.0, 1002.0, 1003.0], 1.0);
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-15);
        }
        assert!(b.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn softmax_pick_trzyma_sie_rozkladu() {
        let mut r = rng(1, StreamId::EngineSelfTest, 0, Tick(0));
        let w = [0.0, 5.0]; // druga opcja ~148× bardziej prawdopodobna
        let druga = (0..10_000)
            .filter(|_| softmax_pick(&w, 1.0, &mut r) == 1)
            .count();
        assert!(druga > 9_800, "wybrano drugą opcję {druga} razy");
    }

    #[test]
    #[should_panic(expected = "temperatura musi być dodatnia")]
    fn softmax_z_zerowa_temperatura_panikuje() {
        softmax_in_place(&mut [1.0, 2.0], 0.0);
    }

    #[test]
    fn softmax_nie_zalezy_od_kolejnosci_obliczen() {
        // Ten sam wektor policzony 1000 razy daje bit w bit ten sam wynik —
        // sumowanie idzie po indeksach, nie parami (T-D11).
        let w: Vec<f64> = (0..16).map(|i| f64::from(i) * 0.37 - 3.0).collect();
        let wzorzec = softmax(&w, 0.8);
        for _ in 0..1000 {
            assert_eq!(softmax(&w, 0.8), wzorzec);
        }
    }
}
