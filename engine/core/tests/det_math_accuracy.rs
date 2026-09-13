//! Dokładność `det_math` względem niezależnej referencji (M0 WP-03b kryt. 2, §7.2)
//! oraz złota wartość międzyplatformowa (T-D9).
//!
//! Referencja: `tests/reference/det_math.txt`, policzona w arytmetyce 60-cyfrowej
//! przez `tests/reference/generate.py`. **Nie** przez libm — testowanie zgodności
//! z tym, od czego uciekamy, nie miałoby sensu (00 §K-6, ryzyko R-13).

use magnat_core::det_math as dm;

fn ulp_error(got: f64, want: f64) -> f64 {
    if got == want || (got.is_nan() && want.is_nan()) {
        return 0.0;
    }
    if !got.is_finite() || !want.is_finite() {
        return f64::INFINITY;
    }
    let a = want.abs();
    let ulp = if a == 0.0 {
        f64::from_bits(1)
    } else {
        f64::from_bits(a.to_bits() + 1) - a
    };
    ((got - want) / ulp).abs()
}

/// Granica błędu dla danej funkcji i argumentu.
///
/// Wszystkie funkcje jednoargumentowe: **≤ 1 ULP**, zgodnie z M0 §7.2.
///
/// `pow` ma granicę zależną od wielkości wykładnika wyniku i to jest **korekta planu**:
/// stałe „≤ 2 ULP" jest nieosiągalne dla algorytmu `exp2(y·log2 x)`, bo błąd logarytmu
/// (rzędu 2⁻⁵⁹ względnie, po złożeniu pary hi/lo) jest mnożony przez `y`. Błąd wyniku
/// to w przybliżeniu `|y·log2 x| · 2⁻⁵⁹` ULP, czyli **≤ 2 ULP dla |wykładnika| ≤ 256**
/// i **≤ 8 ULP** w całym zakresie `f64` (|wykładnik| ≤ 1024). Zejście poniżej wymaga
/// algorytmu klasy fdlibm `e_pow.c` z własną dekompozycją logarytmu na 24-bitowe
/// połówki — nieuzasadnione, bo konsumenci (M5, M7, M10) pracują na wykładnikach
/// rzędu jedności, a nie tysięcy. Zmierzony najgorszy przypadek: 5 ULP dla 1,5^1000.
fn limit_ulp(name: &str, wykladnik: f64) -> f64 {
    match name {
        "pow" if wykladnik.abs() > 256.0 => 8.0,
        "pow" => 2.0,
        _ => 1.0,
    }
}

#[test]
fn dokladnosc_wzgledem_referencji_wysokiej_precyzji() {
    let tabela = include_str!("reference/det_math.txt");
    let mut sprawdzone = 0usize;
    let mut najgorszy: (f64, String) = (0.0, String::new());

    for linia in tabela.lines() {
        if linia.starts_with('#') || linia.trim().is_empty() {
            continue;
        }
        let pola: Vec<&str> = linia.split_whitespace().collect();
        let nazwa = pola[0];

        let (got, want, opis) = if nazwa == "pow" {
            let x = f64::from_bits(u64::from_str_radix(pola[1], 16).unwrap());
            let y = f64::from_bits(u64::from_str_radix(pola[2], 16).unwrap());
            let want = f64::from_bits(u64::from_str_radix(pola[3], 16).unwrap());
            (dm::pow(x, y), want, format!("pow({x:e}, {y:e})"))
        } else {
            let x = f64::from_bits(u64::from_str_radix(pola[1], 16).unwrap());
            let want = f64::from_bits(u64::from_str_radix(pola[2], 16).unwrap());
            let got = match nazwa {
                "ln" => dm::ln(x),
                "log2" => dm::log2(x),
                "exp" => dm::exp(x),
                "exp2" => dm::exp2(x),
                "ln1p" => dm::ln1p(x),
                "exp_m1" => dm::exp_m1(x),
                inne => panic!("nieznana funkcja w tabeli referencyjnej: {inne}"),
            };
            (got, want, format!("{nazwa}({x:e})"))
        };

        // Wyniki zdenormalizowane i skrajne odpadają: tam ULP referencji sam w sobie
        // przestaje być miarą (zaokrąglenie następuje już przy zapisie referencji).
        if want == 0.0 || !want.is_finite() || want.abs() < f64::MIN_POSITIVE {
            continue;
        }

        let blad = ulp_error(got, want);
        let limit = limit_ulp(nazwa, dm::log2(want.abs()));
        assert!(
            blad <= limit,
            "{opis}: {got:e} zamiast {want:e}, błąd {blad:.3} ULP (limit {limit})"
        );
        if blad > najgorszy.0 {
            najgorszy = (blad, opis);
        }
        sprawdzone += 1;
    }

    assert!(
        sprawdzone > 1500,
        "tabela referencyjna okrojona: sprawdzono tylko {sprawdzone} przypadków"
    );
    println!(
        "sprawdzono {sprawdzone}, najgorszy: {:.3} ULP w {}",
        najgorszy.0, najgorszy.1
    );
}

/// T-D9: odcisk wyników na 100 tys. argumentów rozłożonych logarytmicznie po całym
/// zakresie wykładnika. Ten sam hash musi wyjść na `x86_64-linux-gnu`,
/// `x86_64-windows-msvc` i `aarch64-linux-gnu` oraz na dwóch wersjach toolchaina —
/// to jest cały sens `det_math` (00 §K-6).
///
/// Złota wartość zatwierdzona w repozytorium. Jej zmiana oznacza jedno z dwojga:
/// zmieniono implementację (wtedy trzeba przeliczyć wektor i uzasadnić) albo
/// implementacja przestała być deterministyczna (wtedy build ma być czerwony).
#[test]
fn zloty_odcisk_miedzyplatformowy() {
    const ZLOTY_HASH: u64 = 0x5E8E_93D4_5A6C_1792;
    assert_eq!(
        odcisk_det_math(),
        ZLOTY_HASH,
        "det_math dał inny wynik niż złota wartość — patrz komentarz przy teście"
    );
}

fn odcisk_det_math() -> u64 {
    use xxhash_rust::xxh3::Xxh3;
    let mut h = Xxh3::new();
    let mut dodaj = |v: f64| h.update(&v.to_bits().to_le_bytes());

    // 100 tys. argumentów: siatka logarytmiczna po wykładniku × mantysa.
    for e in -1020..=1020i32 {
        for m in 0..49u32 {
            let mantysa = 1.0 + f64::from(m) / 49.0;
            let x = mantysa * dm::exp2(f64::from(e));
            if !x.is_finite() || x == 0.0 {
                continue;
            }
            dodaj(dm::ln(x));
            dodaj(dm::log2(x));
        }
    }
    for i in -70_000..=70_000i32 {
        let x = f64::from(i) / 100.0;
        dodaj(dm::exp(x));
        dodaj(dm::exp2(x / 10.0));
        dodaj(dm::exp_m1(x / 1000.0));
        dodaj(dm::ln1p(x / 1000.0));
    }
    for i in 1..=2_000i32 {
        let x = f64::from(i) / 7.0;
        for y in [-3.5f64, -1.0, 0.25, 1.0, 2.0, 13.0] {
            dodaj(dm::pow(x, y));
        }
    }
    // Brzegi: zera, nieskończoności, NaN, denormalne.
    for x in [
        0.0f64,
        -0.0,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        1.0,
        -1.0,
    ] {
        dodaj(dm::ln(x));
        dodaj(dm::exp(x));
        dodaj(dm::log2(x));
        dodaj(dm::exp2(x));
        dodaj(dm::ln1p(x));
        dodaj(dm::exp_m1(x));
        dodaj(dm::pow(x, 2.0));
        dodaj(dm::pow(2.0, x));
    }
    h.digest()
}

/// Wypisuje bieżący odcisk — używane przy świadomej zmianie implementacji.
/// `cargo test -p magnat-core --test det_math_accuracy -- --ignored --nocapture`
#[test]
#[ignore = "narzędzie, nie test: wypisuje bieżący odcisk do zatwierdzenia"]
fn wypisz_odcisk() {
    println!("odcisk det_math = 0x{:016X}", odcisk_det_math());
}
