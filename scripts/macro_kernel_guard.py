#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Bramka WP10.2: `sim/macro` nie ma własnej logiki ekonomicznej.

Reguła z M10a §5.1 brzmi krótko: model makro zawiera **agregację, alokację
i pętlę czasu**, a każda cena, płaca, przepustowość i każde zaksięgowanie
pieniądza przechodzi przez `sim/economy::kernel` (`K-50`). Reguła zapisana
w dokumencie nie broni się sama — broni jej ten skrypt.

Skrypt jest **grepem, nie parserem Rusta**, i to jest świadome: pełna analiza
wymagałaby drzewa składni, a szukamy rzeczy, która w praktyce wygląda zawsze
tak samo. Fałszywy alarm gasi się jedną linią znacznika z powodem; przeoczenie
w drugą stronę kosztowałoby drugi model gospodarki obok mezo.

Dwie reguły, obie wąskie i obie na czymś, co widać w jednej linii:

  1. **skala punktu bazowego w działaniu** — `* 10_000` albo `/ 10_000`
     (i milion). Punkt bazowy jest skalą, w której liczy się pieniądz, i ma
     jedno miejsce: `kernel::apply_bp`. Dzielenie rozsiane po crate'cie to
     tyle samo miejsc, w których wolno pomylić zaokrąglenie;
  2. **stała nazwana stawką** — `*_bp`, `*_rate`, `*margin*`, `*marza*`,
     `*stawka*` z przypisaną liczbą.

Czego skrypt nie widzi i nie ma widzieć: promile ludzi (`* 1_000 / etaty`),
indeksy, długości tablic i liczby dób (`K-1`). To jest agregacja i pętla czasu,
czyli dokładnie to, co w `sim/macro` **ma** być.

Użycie:
    python scripts/macro_kernel_guard.py            # bramka
    python scripts/macro_kernel_guard.py --self-test
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

KATALOG = Path("sim/macro/src")

# Region zwolniony **jawnie i z powodem**. Znacznik ma postać
# `macro-guard: <powód>` i powód jest obowiązkowy — zwolnienie bez powodu
# byłoby wyłączeniem bramki, a nie odpowiedzią na nią. Region otwiera linia
# znacznika, a zamyka pusta linia albo klamra o tym samym wcięciu.
#
# Zwolnienia są **wypisywane w raporcie**: mają być widoczne, a nie ciche.
ZNACZNIK = re.compile(r"macro-guard:\s*(\S.*?)\s*$")

# Nazwy, które zdradzają stawkę nawet bez skali bp w działaniu.
PODEJRZANA_NAZWA = re.compile(
    r"\b(?:const|let|static)\s+(?:mut\s+)?[A-Za-z_][A-Za-z0-9_]*"
    r"(?:_bp|_rate|margin|marza|stawka)\b\s*(?::[^=]+)?=\s*[-0-9]",
    re.IGNORECASE,
)

# Skala punktu bazowego w działaniu.
SKALA_BP = re.compile(
    r"[*/]\s*(?:10_?000|1_?000_?000)\b|\b(?:10_?000|1_?000_?000)\s*[*/]"
)


def linie_kodu(tekst: str):
    """Linie bez komentarzy i bez treści łańcuchów — komentarz nie jest kodem."""
    w_bloku = False
    for nr, linia in enumerate(tekst.splitlines(), 1):
        surowa = linia
        if w_bloku:
            if "*/" in linia:
                linia = linia.split("*/", 1)[1]
                w_bloku = False
            else:
                continue
        if "/*" in linia:
            przed, reszta = linia.split("/*", 1)
            if "*/" in reszta:
                linia = przed + reszta.split("*/", 1)[1]
            else:
                linia, w_bloku = przed, True
        linia = re.sub(r'"(?:[^"\\]|\\.)*"', '""', linia)
        linia = re.sub(r"//.*$", "", linia)
        yield nr, linia, surowa


def zwolnione_regiony(tekst: str) -> list:
    """Zakresy linii otwarte znacznikiem i zamknięte klamrą o tym samym wcięciu."""
    out = []
    linie = tekst.splitlines()
    for i, linia in enumerate(linie):
        m = ZNACZNIK.search(linia)
        if not m:
            continue
        wciecie = len(linia) - len(linia.lstrip())
        koniec = len(linie)
        for j in range(i + 1, len(linie)):
            kandydat = linie[j]
            # Pusta linia kończy region zwolnienia. Dzięki temu znacznik działa
            # tak samo nad blokiem (`impl Default`) jak nad pojedynczą stałą —
            # bez tego stała zwalniałaby wszystko aż do końca najbliższego bloku.
            if not kandydat.strip():
                koniec = j
                break
            w = len(kandydat) - len(kandydat.lstrip())
            if w <= wciecie and kandydat.strip().startswith("}"):
                koniec = j + 1
                break
        out.append((i + 1, koniec, m.group(1)))
    return out


def w_zwolnieniu(nr: int, regiony: list) -> bool:
    return any(a <= nr <= b for a, b, _ in regiony)


def sprawdz_tekst(tekst: str, nazwa: str, zwolnienia: list | None = None) -> list:
    bledy = []
    regiony = zwolnione_regiony(tekst)
    if zwolnienia is not None:
        zwolnienia += [(f"{nazwa}:{a}", powod) for a, _, powod in regiony]
    for nr, kod, surowa in linie_kodu(tekst):
        if w_zwolnieniu(nr, regiony):
            continue
        if PODEJRZANA_NAZWA.search(kod):
            bledy.append(f"{nazwa}:{nr}: stawka nazwana w kodzie: {surowa.strip()}")
        elif SKALA_BP.search(kod):
            bledy.append(
                f"{nazwa}:{nr}: skala punktu bazowego w działaniu"
                f" (użyj kernel::apply_bp): {surowa.strip()}"
            )
    return bledy


def bramka() -> int:
    if not KATALOG.is_dir():
        print(f"brak katalogu {KATALOG} — uruchom z katalogu repozytorium", file=sys.stderr)
        return 2
    bledy = []
    zwolnienia = []
    pliki = sorted(KATALOG.rglob("*.rs"))
    for p in pliki:
        bledy += sprawdz_tekst(p.read_text(encoding="utf-8"), p.as_posix(), zwolnienia)
    if bledy:
        print("sim/macro liczy po swojemu zamiast wołać economy::kernel (M10a/WP10.2):")
        for b in bledy:
            print("  " + b)
        return 1
    print(f"macro_kernel_guard: {len(pliki)} plików, zero stawek poza jądrem")
    for miejsce, powod in zwolnienia:
        print(f"  zwolnione — {miejsce}: {powod}")
    return 0


ZWOLNIONA_STALA = (
    "/// macro-guard: kalibracja, nie reguła\n"
    "const REF_MARGIN_BP: i32 = 1_800;\n"
    "\n"
    "fn f(x: i64) -> i64 { x * 1_800 / 10_000 }\n"
)

ZWOLNIONY_BLOK = (
    "impl Default for P {\n"
    "    // macro-guard: kalibracja, nie reguła\n"
    "    fn default() -> P {\n"
    "        P { margin_bp: 1_800 }\n"
    "    }\n"
    "}\n"
)


def self_test() -> int:
    """Bramka, która nigdy nie świeci na czerwono, nie jest bramką."""
    zle = [
        "fn f(x: i64) -> i64 { x * 1_800 / 10_000 }",
        "const MARZA_BP: i64 = 1800;",
        "let rate_bp = 250;",
        "fn g(c: i64) -> i64 { c / 10_000 }",
        "let v = kwota * 1_000_000 / 3;",
    ]
    dobre = [
        "fn f(x: i64) -> i64 { x / 30 }",
        "let dni = 360;",
        "// marża 1_800 bp liczy kernel, dzieli przez 10_000",
        'let s = "margin_bp: 1800";',
        "let n = cells.len() * 2;",
        "let niedobor = wakaty * 1_000 / etaty;",
        "let m = kernel::apply_bp(kwota, p.spend_rate_bp);",
        ZWOLNIONY_BLOK,
    ]
    # Zwolnienie kończy się na pustej linii: stała nad nią jest zwolniona,
    # funkcja pod nią — już nie.
    zle.append(ZWOLNIONA_STALA)
    ok = True
    for i, t in enumerate(zle):
        if not sprawdz_tekst(t, f"zly{i}"):
            print(f"self-test: nie wykryto stawki w: {t!r}")
            ok = False
    for i, t in enumerate(dobre):
        b = sprawdz_tekst(t, f"dobry{i}")
        if b:
            print(f"self-test: fałszywy alarm na: {t!r} -> {b}")
            ok = False
    print("macro_kernel_guard --self-test:", "ok" if ok else "BŁĄD")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true", help="sprawdź sam wykrywacz")
    a = ap.parse_args()
    return self_test() if a.self_test else bramka()


if __name__ == "__main__":
    sys.exit(main())
