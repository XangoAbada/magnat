#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Bramka R2-WP23: opis nie rozjeżdża się z rzeczą, którą opisuje.

`README.md` jest jedynym plikiem, który ktoś z zewnątrz przeczyta pierwszy,
a jego nagłówek stanu nie był ruszany od fazy, w której powstał. Rozjazd nie
jest widoczny dla żadnego testu, bo opis nie ma z czym się nie zgadzać —
dopóki nie powie mu tego ta bramka.

Dwie reguły, obie mechaniczne:

  1. **stan w `README.md` == ostatni odhaczony wiersz `00-postep.md`** —
     nagłówek `Stan:` musi nieść identyfikator ostatniej pozycji `[x]`
     (np. `R2d`, `M11e`). Kolejność wierszy w dokumencie postępu jest
     kolejnością wykonania, więc „ostatni" znaczy „najniżej w pliku";
  2. **jeden nagłówek tabel korekt** — `K-18` pkt (b) każe zbierać poprawki
     w tabeli „Zmiany wpisane po MX". Wariantów tego nagłówka było w repo
     **siedem** w 22 nagłówkach, a przegląd po jednym z nich gubi wpisy
     z pozostałych sześciu. Wpis, którego nie da się znaleźć, jest wpisem,
     którego nie ma.

Czego ta bramka nie sprawdza i dlaczego: **czy treść wiersza korekty istnieje
w dokumencie, na który wskazuje**. To jest druga połowa `plan_guard` i należy
do `R2-WP26` razem z egzekutorem rejestru długu — tam jest jej miejsce, bo
tam powstaje lista adresatów, wobec której da się ją sprawdzić.

Użycie:
    python scripts/plan_guard.py              # bramka
    python scripts/plan_guard.py --self-test
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

README = Path("README.md")
POSTEP = Path("docs/implementation-plan/00-postep.md")
PLAN = Path("docs/implementation-plan")

# Wiersz odhaczonej pozycji w dokumencie postępu: `- [x] **M11e** Budżet klatki — ...`.
# Identyfikator jest pogrubiony i jest pierwszym pogrubieniem w wierszu.
ODHACZONY = re.compile(r"^-\s*\[x\]\s*\*\*([A-Za-z0-9]+)\*\*", re.MULTILINE)

# Nagłówek stanu w README: jedna linia zaczynająca się od `Stan:`.
STAN = re.compile(r"^Stan:\s*(.+)$")

# Nagłówek tabeli korekt niekanoniczny. Szukamy **nagłówka**, nie zdania — dokument
# R2 opisuje warianty w prozie i to nie jest tabela. Numerowana sekcja (`## 4a. …`)
# też nie: to część własnej numeracji dokumentu, z żywymi odsyłaczami `M0 §4a`,
# a nie tabela doklejona na końcu wg `K-18` pkt (b).
NAGLOWEK_OBCY = re.compile(r"^#{1,6}\s+Korekty.*$", re.MULTILINE)

# Kanoniczny nagłówek tabeli korekt (`K-18` pkt b).
NAGLOWEK_KANONICZNY = "Zmiany wpisane po"


def ostatnia_odhaczona(tekst: str) -> str | None:
    """Identyfikator ostatniej pozycji `[x]` — najniższej w pliku."""
    trafienia = ODHACZONY.findall(tekst)
    return trafienia[-1] if trafienia else None


def stan_z_readme(tekst: str) -> str | None:
    for linia in tekst.splitlines():
        m = STAN.match(linia)
        if m:
            return m.group(1)
    return None


def bramka() -> int:
    bledy: list[str] = []

    # Reguła 1 — stan README wobec dokumentu postępu.
    if not README.exists() or not POSTEP.exists():
        print("plan_guard: brak README.md albo 00-postep.md", file=sys.stderr)
        return 2
    faza = ostatnia_odhaczona(POSTEP.read_text(encoding="utf-8"))
    stan = stan_z_readme(README.read_text(encoding="utf-8"))
    if faza is None:
        bledy.append("00-postep.md nie ma ani jednego wiersza `- [x] **X**`")
    elif stan is None:
        bledy.append("README.md nie ma linii zaczynającej się od `Stan:`")
    elif not re.search(rf"\b{re.escape(faza)}\b", stan):
        bledy.append(
            f"README.md deklaruje stan {stan.split('(')[0].strip()!r}, "
            f"a ostatnią odhaczoną pozycją 00-postep.md jest {faza}"
        )

    # Reguła 2 — jeden nagłówek tabel korekt w całym planie.
    for plik in sorted(PLAN.glob("*.md")):
        tekst = plik.read_text(encoding="utf-8")
        for obcy in NAGLOWEK_OBCY.findall(tekst):
            bledy.append(
                f"{plik.as_posix()}: nagłówek tabeli korekt {obcy.strip()!r} — "
                f"kanoniczny to '## {NAGLOWEK_KANONICZNY} <faza>' (`K-18` pkt b)"
            )

    for b in bledy:
        print(f"plan_guard: {b}")
    if bledy:
        print(f"\nplan_guard: {len(bledy)} rozjazd(ów) opisu z rzeczą.")
        return 1
    print(f"plan_guard: ok — README.md mówi {faza}, jeden nagłówek tabel korekt.")
    return 0


def self_test() -> int:
    """Bramka, która nigdy nie świeci na czerwono, nie jest bramką."""
    ok = True

    przypadki_postep = [
        ("- [x] **M11e** Budżet klatki — `x.md`\n- [ ] **R2e** Dług\n", "M11e"),
        ("- [x] **M1** a\n- [x] **R2d** b\n", "R2d"),
        ("- [ ] **M1** a\n", None),
    ]
    for tekst, chciane in przypadki_postep:
        mam = ostatnia_odhaczona(tekst)
        if mam != chciane:
            ok = False
            print(f"self-test: ostatnia_odhaczona({tekst!r}) = {mam!r}, chciane {chciane!r}")

    przypadki_stan = [
        ("# T\n\nStan: **R2d zamknięte** (świat).\n", "**R2d zamknięte** (świat)."),
        ("# T\n\nbez stanu\n", None),
    ]
    for tekst, chciane in przypadki_stan:
        mam = stan_z_readme(tekst)
        if mam != chciane:
            ok = False
            print(f"self-test: stan_z_readme = {mam!r}, chciane {chciane!r}")

    # Rozpoznanie rozjazdu: stan mówi o innej fazie niż ostatni wiersz postępu.
    if re.search(r"\bR2d\b", "**M8 zamknięte, M9a zamknięte** (miasto)."):
        ok = False
        print("self-test: fałszywe dopasowanie fazy w nagłówku stanu")
    if not re.search(r"\bR2d\b", "**R2d zamknięte** (świat)."):
        ok = False
        print("self-test: nie rozpoznano zgodnego nagłówka stanu")

    # Obcy nagłówek wykrywany; wzmianka w prozie i numerowana sekcja — nie.
    if not NAGLOWEK_OBCY.findall("## Korekty projektu technicznego M5c\n"):
        ok = False
        print("self-test: nie wykryto obcego nagłówka tabeli korekt")
    if NAGLOWEK_OBCY.findall('tabela „Korekty wpisane w trakcie MX" (1)\n'):
        ok = False
        print("self-test: fałszywy alarm na wzmiance w prozie")
    if NAGLOWEK_OBCY.findall("## 4a. Korekty planu wprowadzone w trakcie implementacji\n"):
        ok = False
        print("self-test: fałszywy alarm na numerowanej sekcji")

    print("plan_guard --self-test:", "ok" if ok else "BŁĄD")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true", help="sprawdź sam wykrywacz")
    a = ap.parse_args()
    return self_test() if a.self_test else bramka()


if __name__ == "__main__":
    sys.exit(main())
