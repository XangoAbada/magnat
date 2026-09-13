"""Generator tabeli referencyjnej dla `core::det_math` (M0 WP-03b, kryterium 2).

Wartości odniesienia liczone w arytmetyce dziesiętnej o 60 cyfrach znaczących
(≈ 200 bitów, moduł `decimal` ze standardowej biblioteki Pythona) i zaokrąglane
do najbliższego `f64`. To jest **niezależna implementacja**: nie dotyka libm ani
kodu Rusta, więc porównanie z nią mierzy dokładność, a nie zgodność z samym sobą.

Plan M0 mówił o MPFR (`rug`); `decimal` daje tę samą rolę bez dokładania zależności
i bez osobnego narzędzia w workspace. Odnotowane w dzienniku 00-postep.md.

Uruchomienie (tylko przy zmianie zestawu argumentów):
    python engine/core/tests/reference/generate.py > engine/core/tests/reference/det_math.txt
"""

import struct
from decimal import Decimal, getcontext

getcontext().prec = 60

LN2 = Decimal(2).ln()


def bits(x: float) -> int:
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def to_f64(d: Decimal) -> float:
    return float(d)


def args_log():
    """Argumenty dla ln/log2: rozkład logarytmiczny po całym zakresie wykładnika."""
    out = [1.0, 2.0, 0.5, 10.0, 1e300, 1e-300, 5e-324, 1.7976931348623157e308]
    # Wokół 1.0 — tam kasowanie jest najgroźniejsze.
    for k in range(-60, 61):
        out.append(1.0 + k * 1e-15)
    # Po całym zakresie wykładnika.
    for e in range(-300, 301, 7):
        for m in (1.0, 1.3, 1.7, 1.9999):
            out.append(m * 10.0**e)
    # Wokół granic normalizacji (sqrt(2)/2 i sqrt(2)).
    for m in (0.70710678, 0.7071068, 1.41421356, 1.4142136):
        out.append(m)
    return [x for x in out if x > 0 and x != float("inf")]


def args_exp():
    out = [0.0, 1.0, -1.0, 709.0, -745.0, 0.5, -0.5]
    for k in range(-700, 701, 13):
        out.append(float(k))
        out.append(k + 0.37)
    for k in range(-40, 41):
        out.append(k * 1e-3)
        out.append(k * 1e-10)
    return out


def args_trig():
    """Argumenty dla sin/cos/tan: gęsto wokół zera i wokół wielokrotności pi/2,
    bo tam redukcja argumentu Cody'ego-Waite'a jest najbardziej narazona na utrate bitow."""
    out = [0.0, 1.0, -1.0, 0.5, -0.5]
    pi = Decimal(
        "3.14159265358979323846264338327950288419716939937510582097494"
    )
    for k in range(-64, 65):
        out.append(float(Decimal(k) * pi / 2))
        out.append(float(Decimal(k) * pi / 2 + Decimal("1e-9")))
        out.append(float(Decimal(k) * pi / 2 - Decimal("1e-9")))
    for k in range(-200, 201, 3):
        out.append(k * 0.37)
    for k in range(-30, 31):
        out.append(k * 1e-7)
    # Duze argumenty w granicach TRIG_ARG_LIMIT.
    for k in range(1, 20):
        out.append(k * 5e4)
        out.append(-k * 5e4)
    return out


def dec_sin(d: Decimal) -> Decimal:
    """Sinus z szeregu Taylora po redukcji do [-pi, pi]. 60 cyfr kontekstu wystarcza,
    bo argumenty sa ograniczone do 1e6, a redukcja idzie w pelnej precyzji Decimal."""
    pi = Decimal(
        "3.14159265358979323846264338327950288419716939937510582097494459230781640628620899862803482534211706798"
    )
    two_pi = 2 * pi
    d = d - (d / two_pi).to_integral_value(rounding="ROUND_HALF_EVEN") * two_pi
    term = d
    total = d
    k = 1
    while abs(term) > Decimal(10) ** -70 and k < 200:
        term = -term * d * d / ((2 * k) * (2 * k + 1))
        total += term
        k += 1
    return total


def dec_cos(d: Decimal) -> Decimal:
    pi = Decimal(
        "3.14159265358979323846264338327950288419716939937510582097494459230781640628620899862803482534211706798"
    )
    return dec_sin(d + pi / 2)


def emit(name, values, fn):
    for x in values:
        try:
            # Decimal(float) bierze DOKŁADNĄ wartość binarną; Decimal(repr(x))
            # wzięłoby skróconą dziesiętną i wniosło własne pół ULP błędu.
            want = fn(Decimal(x))
        except Exception:  # poza domeną referencji — pomijamy, brzegi testuje osobny test
            continue
        try:
            w = to_f64(want)
        except OverflowError:
            continue
        print(f"{name} {bits(x):016x} {bits(w):016x}")


def main():
    print("# det_math — tabela referencyjna, 60 cyfr znaczących, zaokrąglona do f64")
    print("# format: <funkcja> <bity_argumentu_hex> <bity_wyniku_hex>")
    emit("ln", args_log(), lambda d: d.ln())
    emit("log2", args_log(), lambda d: d.ln() / LN2)
    emit("exp", args_exp(), lambda d: d.exp())
    emit("exp2", [x / 3.0 for x in range(-3000, 3001, 29)], lambda d: (d * LN2).exp())
    emit(
        "ln1p",
        [x for x in args_exp() if -0.9 < x < 10] + [1e-17, 1e-9, -1e-9, 0.25, -0.25],
        lambda d: (1 + d).ln(),
    )
    emit(
        "exp_m1",
        [x for x in args_exp() if -40 < x < 40],
        lambda d: d.exp() - 1,
    )
    emit_pow()
    # Dopisane przez M1: cykl roczny klimatu i kat usypu wchodza do stanu trwalego.
    emit("sin", args_trig(), dec_sin)
    emit("cos", args_trig(), dec_cos)
    emit("tan", [x for x in args_trig() if abs(dec_cos(Decimal(x))) > Decimal("1e-6")], lambda d: dec_sin(d) / dec_cos(d))


def emit_pow():
    """pow ma dwa argumenty, więc własny format linii: pow <x> <y> <wynik>."""
    bases = [1.5, 2.0, 0.5, 10.0, 3.7, 0.017, 1234.5, 1e-8, 1e8, 0.9999, 1.0001]
    exps = [
        0.5, 1.0, 2.0, 3.0, -1.0, -2.5, 0.1, 7.3, -0.001, 100.0, -100.0, 1000.0, 0.3333,
    ]
    for x in bases:
        dx = Decimal(x)
        for y in exps:
            dy = Decimal(y)
            want = (dy * dx.ln()).exp()
            try:
                w = to_f64(want)
            except OverflowError:
                continue
            if w == 0.0 or w == float("inf"):
                continue
            print(f"pow {bits(x):016x} {bits(y):016x} {bits(w):016x}")


if __name__ == "__main__":
    main()
